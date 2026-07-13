// Local HTTP listener for agent status hooks. Phase 5 of the Orca port —
// agents POST their state here instead of us trying to parse it out of
// their terminal output. This module is infrastructure: the per-agent
// hook-script writers (so claude/codex/etc. actually call into this URL)
// land in a follow-up.
//
// Security posture: bound to 127.0.0.1 only, random port chosen by the OS,
// per-session bearer tokens. 1MB body cap and a 5s request timeout cover
// the obvious slowloris / oversized-payload abuse from a misbehaving hook
// script — anything more is out of scope for a localhost-only surface.

use crate::state::AppState;
use crate::status::{self, AgentStatus};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;
use tower_http::{limit::RequestBodyLimitLayer, timeout::TimeoutLayer};
use uuid::Uuid;

const MAX_BODY_BYTES: usize = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Lookup from hook token → session_id. Stored in AppState so agents.rs can
/// mint a token at launch time and the listener can resolve it later.
pub type HookTokenMap = Arc<Mutex<HashMap<String, String>>>;

#[derive(Clone)]
pub struct HookServerInfo {
    pub url: String,
    pub tokens: HookTokenMap,
    pub mcp_token: String,
}

#[derive(Clone)]
pub struct ServerCtx {
    pub app: AppHandle,
    pub tokens: HookTokenMap,
    pub mcp_token: String,
}

#[derive(Debug, Deserialize)]
struct HookPayload {
    state: String,
    /// Optional caller-side metadata; we ignore most of it for now but accept
    /// it so hook scripts can send a richer envelope without breaking.
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    interrupted: Option<bool>,
}

#[derive(Debug, Serialize)]
struct HookResponse {
    ok: bool,
    session_id: Option<String>,
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct McpRequest {
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
pub struct ProjectMcpInfo {
    pub url: String,
    pub token: String,
}

fn mcp_tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required }
    })
}

fn project_mcp_tools() -> Vec<Value> {
    vec![
        mcp_tool(
            "xnaut_list_projects",
            "List xNAUT projects from the configured control repository.",
            json!({}),
            &[],
        ),
        mcp_tool(
            "xnaut_list_tickets",
            "List xNAUT tickets, optionally filtered by project key.",
            json!({ "project": { "type": "string" } }),
            &[],
        ),
        mcp_tool(
            "xnaut_create_ticket",
            "Create a Git-backed xNAUT ticket.",
            json!({
                "project": { "type": "string" }, "title": { "type": "string" },
                "ticket_type": { "type": "string", "enum": ["idea", "feature", "bug", "incident", "task"] },
                "status": { "type": "string", "enum": ["inbox", "ready", "in_progress", "review", "blocked", "done"] },
                "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
                "owner": { "type": "string" }, "documentation": { "type": "array", "items": { "type": "string" } },
                "body": { "type": "string" }
            }),
            &["project", "title"],
        ),
        mcp_tool(
            "xnaut_update_ticket",
            "Update an xNAUT ticket using optimistic revision control.",
            json!({
                "id": { "type": "string" }, "expected_revision": { "type": "integer" },
                "title": { "type": "string" }, "ticket_type": { "type": "string" }, "status": { "type": "string" },
                "priority": { "type": "string" }, "owner": { "type": ["string", "null"] },
                "clear_owner": { "type": "boolean" }, "documentation": { "type": "array", "items": { "type": "string" } },
                "body": { "type": "string" }
            }),
            &["id", "expected_revision"],
        ),
        mcp_tool(
            "xnaut_list_documents",
            "List Markdown documents inside one xNAUT project's work Vault scope.",
            json!({ "project": { "type": "string" } }),
            &["project"],
        ),
        mcp_tool(
            "xnaut_search_documents",
            "Search Markdown documents inside one xNAUT project's work Vault scope.",
            json!({ "project": { "type": "string" }, "query": { "type": "string" } }),
            &["project", "query"],
        ),
        mcp_tool(
            "xnaut_read_document",
            "Read a project-scoped Markdown document and return its content SHA-256 for conflict-safe updates.",
            json!({ "project": { "type": "string" }, "rel": { "type": "string" } }),
            &["project", "rel"],
        ),
        mcp_tool(
            "xnaut_create_document",
            "Create a Markdown document inside one xNAUT project's work Vault scope.",
            json!({
                "project": { "type": "string" }, "rel": { "type": "string" },
                "content": { "type": "string" }
            }),
            &["project", "rel", "content"],
        ),
        mcp_tool(
            "xnaut_update_document",
            "Update a project-scoped Markdown document only when its current SHA-256 matches expected_sha256.",
            json!({
                "project": { "type": "string" }, "rel": { "type": "string" },
                "content": { "type": "string" }, "expected_sha256": { "type": "string" }
            }),
            &["project", "rel", "content", "expected_sha256"],
        ),
    ]
}

fn required_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn document_path(rel: &str) -> Result<String, String> {
    if rel.contains('\\') || rel.contains(':') {
        return Err("document path must use relative forward-slash segments".into());
    }
    let path = Path::new(rel);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            ) || matches!(component, Component::Normal(value) if value.to_string_lossy().starts_with('.'))
        })
    {
        return Err("document path must be a visible relative path inside the project".into());
    }
    let normalized = rel.trim_matches('/').to_owned();
    if normalized.is_empty() || !normalized.to_ascii_lowercase().ends_with(".md") {
        return Err("document path must end in .md".into());
    }
    Ok(normalized)
}

async fn project_document_scope(ctx: &ServerCtx, args: &Value) -> Result<(String, String), String> {
    let key = required_arg(args, "project")?;
    let projects = crate::project_management::pm_project_list(ctx.app.state::<AppState>()).await?;
    let project = projects
        .into_iter()
        .find(|project| project.key.eq_ignore_ascii_case(key))
        .ok_or_else(|| format!("project not found: {key}"))?;
    let folder: String = project
        .name
        .chars()
        .map(|character| {
            if matches!(
                character,
                '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            ) {
                '-'
            } else {
                character
            }
        })
        .collect();
    let folder = folder.trim();
    Ok((
        project.key,
        format!(
            "Development/{}",
            if folder.is_empty() { key } else { folder }
        ),
    ))
}

fn ensure_work_vault(ctx: &ServerCtx) -> Result<(), String> {
    let state = ctx.app.state::<crate::vault::VaultManager>();
    let open = state.indexes.lock().unwrap().contains_key("work");
    if !open {
        crate::vault::vault_open(ctx.app.clone(), state, "work".into())?;
    }
    Ok(())
}

fn content_sha256(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

fn scoped_note_result(project: &str, rel: &str, vault_rel: &str, content: String) -> Value {
    json!({
        "project": project,
        "rel": rel,
        "vault_rel": vault_rel,
        "content": content,
        "sha256": content_sha256(&content)
    })
}

async fn call_document_tool(ctx: &ServerCtx, name: &str, args: Value) -> Result<Value, String> {
    ensure_work_vault(ctx)?;
    let (project, scope) = project_document_scope(ctx, &args).await?;
    let vault_state = ctx.app.state::<crate::vault::VaultManager>();
    if name == "xnaut_list_documents" {
        let tree = crate::vault::vault_tree(vault_state, "work".into())?;
        let notes = tree["notes"].as_array().cloned().unwrap_or_default();
        return Ok(Value::Array(
            notes
                .into_iter()
                .filter(|note| {
                    note.get("rel")
                        .and_then(Value::as_str)
                        .is_some_and(|rel| rel.starts_with(&format!("{scope}/")))
                })
                .collect(),
        ));
    }
    if name == "xnaut_search_documents" {
        let query = required_arg(&args, "query")?.to_owned();
        let hits = crate::vault::vault_search(vault_state, "work".into(), query)?;
        return Ok(Value::Array(
            hits.as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|hit| {
                    hit.get("rel")
                        .and_then(Value::as_str)
                        .is_some_and(|rel| rel.starts_with(&format!("{scope}/")))
                })
                .collect(),
        ));
    }
    let rel = document_path(required_arg(&args, "rel")?)?;
    let vault_rel = format!("{scope}/{rel}");
    if name == "xnaut_read_document" {
        let content = crate::vault::vault_note_read(vault_state, "work".into(), vault_rel.clone())?;
        return Ok(scoped_note_result(&project, &rel, &vault_rel, content));
    }
    let content = required_arg(&args, "content")?.to_owned();
    if name == "xnaut_create_document" {
        crate::vault::vault_note_create(
            ctx.app.clone(),
            vault_state,
            "work".into(),
            vault_rel.clone(),
            Some(content.clone()),
        )?;
        return Ok(scoped_note_result(&project, &rel, &vault_rel, content));
    }
    let expected = required_arg(&args, "expected_sha256")?;
    let current = crate::vault::vault_note_read(
        ctx.app.state::<crate::vault::VaultManager>(),
        "work".into(),
        vault_rel.clone(),
    )?;
    let actual = content_sha256(&current);
    if actual != expected {
        return Err(format!(
            "document conflict: expected sha256 {expected}, current sha256 {actual}"
        ));
    }
    crate::vault::vault_note_write(
        ctx.app.clone(),
        ctx.app.state::<crate::vault::VaultManager>(),
        "work".into(),
        vault_rel.clone(),
        content.clone(),
    )?;
    Ok(scoped_note_result(&project, &rel, &vault_rel, content))
}

async fn call_project_tool(ctx: &ServerCtx, name: &str, args: Value) -> Result<Value, String> {
    let state = ctx.app.state::<AppState>();
    match name {
        "xnaut_list_projects" => {
            serde_json::to_value(crate::project_management::pm_project_list(state).await?)
                .map_err(|error| error.to_string())
        }
        "xnaut_list_tickets" => {
            let project = args
                .get("project")
                .and_then(Value::as_str)
                .map(str::to_owned);
            serde_json::to_value(crate::project_management::pm_ticket_list(state, project).await?)
                .map_err(|error| error.to_string())
        }
        "xnaut_create_ticket" => {
            let request = serde_json::from_value(args).map_err(|error| error.to_string())?;
            serde_json::to_value(crate::project_management::pm_ticket_create(state, request).await?)
                .map_err(|error| error.to_string())
        }
        "xnaut_update_ticket" => {
            let request = serde_json::from_value(args).map_err(|error| error.to_string())?;
            serde_json::to_value(crate::project_management::pm_ticket_update(state, request).await?)
                .map_err(|error| error.to_string())
        }
        "xnaut_list_documents"
        | "xnaut_search_documents"
        | "xnaut_read_document"
        | "xnaut_create_document"
        | "xnaut_update_document" => call_document_tool(ctx, name, args).await,
        _ => Err(format!("unknown xNAUT tool: {name}")),
    }
}

async fn handle_mcp(
    State(ctx): State<ServerCtx>,
    headers: HeaderMap,
    Json(request): Json<McpRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token".into()))?;
    if token != ctx.mcp_token {
        return Err((StatusCode::UNAUTHORIZED, "invalid MCP token".into()));
    }
    let result = match request.method.as_str() {
        "initialize" => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "xnaut-project-management", "version": env!("CARGO_PKG_VERSION") }
        }),
        "notifications/initialized" => Value::Null,
        "tools/list" => json!({ "tools": project_mcp_tools() }),
        "tools/call" => {
            let name = request
                .params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let args = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match call_project_tool(&ctx, name, args).await {
                Ok(value) => json!({ "content": [{ "type": "text", "text": value.to_string() }] }),
                Err(error) => {
                    json!({ "content": [{ "type": "text", "text": error }], "isError": true })
                }
            }
        }
        _ => {
            return Ok(Json(
                json!({ "jsonrpc": "2.0", "id": request.id, "error": { "code": -32601, "message": "method not found" } }),
            ))
        }
    };
    Ok(Json(
        json!({ "jsonrpc": "2.0", "id": request.id, "result": result }),
    ))
}

fn parse_state(s: &str) -> Option<AgentStatus> {
    match s {
        "working" => Some(AgentStatus::Working),
        "blocked" => Some(AgentStatus::Blocked),
        "waiting" => Some(AgentStatus::Waiting),
        "done" => Some(AgentStatus::Done),
        "idle" => Some(AgentStatus::Idle),
        "permission" => Some(AgentStatus::Permission),
        "interrupted" => Some(AgentStatus::Interrupted),
        _ => None,
    }
}

async fn handle_hook(
    State(ctx): State<ServerCtx>,
    headers: HeaderMap,
    Json(payload): Json<HookPayload>,
) -> Result<Json<HookResponse>, (StatusCode, String)> {
    let token = headers
        .get("x-xnaut-session")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "missing X-Xnaut-Session header".into(),
        ))?;

    let session_id = {
        let map = ctx.tokens.lock().await;
        map.get(token)
            .cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "unknown session token".into()))?
    };

    let new_state = parse_state(&payload.state).ok_or((
        StatusCode::BAD_REQUEST,
        format!("unknown state: {}", payload.state),
    ))?;

    let state = ctx.app.try_state::<AppState>().ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "AppState unavailable".into(),
    ))?;

    // Dispatch to the right status helper so the event payload matches what the
    // Phase 4 frontend already listens for.
    match new_state {
        AgentStatus::Done => {
            status::mark_session_done(&state.agent_sessions, &ctx.app, &session_id).await;
        }
        AgentStatus::Interrupted => {
            status::mark_session_interrupted(&state.agent_sessions, &ctx.app, &session_id).await;
        }
        AgentStatus::Working => {
            // Equivalent to a fresh output ping — keeps decay logic consistent.
            status::ping_session_output(&state.agent_sessions, &ctx.app, &session_id).await;
        }
        other => {
            status::set_session_status(&state.agent_sessions, &ctx.app, &session_id, other).await;
        }
    }

    // If the script said "interrupted: true" but used a non-interrupted state,
    // treat that as Orca's interrupt-synthesis fallback.
    if payload.interrupted.unwrap_or(false) && new_state != AgentStatus::Interrupted {
        status::mark_session_interrupted(&state.agent_sessions, &ctx.app, &session_id).await;
    }

    // Mention received tool_name/prompt for debug logging; not currently surfaced.
    if payload.tool_name.is_some() || payload.prompt.is_some() {
        eprintln!(
            "[agent_hooks] session={} state={} tool={:?} prompt={}",
            session_id,
            payload.state,
            payload.tool_name,
            payload.prompt.is_some()
        );
    }

    Ok(Json(HookResponse {
        ok: true,
        session_id: Some(session_id),
        state: Some(payload.state),
    }))
}

/// Mints a fresh per-session hook token and stores it in the map.
/// Currently inlined into agents.rs to avoid an extra await — kept as a
/// public helper for tests + future external callers.
#[allow(dead_code)]
pub async fn mint_token(tokens: &HookTokenMap, session_id: &str) -> String {
    let token = Uuid::new_v4().to_string();
    let mut map = tokens.lock().await;
    map.insert(token.clone(), session_id.to_string());
    token
}

/// Forgets a session token (called when a session ends, to prevent stale auth).
/// Currently unused — session cleanup happens lazily via the status decay path —
/// but kept as the eventual hook for explicit teardown.
#[allow(dead_code)]
pub async fn forget_token(tokens: &HookTokenMap, token: &str) {
    let mut map = tokens.lock().await;
    map.remove(token);
}

/// Spawns the listener. Returns the URL the hook scripts should POST to and the
/// MCP bearer token. Both `port` and `mcp_token` are persistent (from settings)
/// so the URL/token pasted into claude/codex configs survive app restarts —
/// previously these were random each launch, which silently broke those configs
/// on every restart.
pub async fn start_server(
    app: AppHandle,
    tokens: HookTokenMap,
    port: u16,
    mcp_token: String,
) -> Result<(String, String), String> {
    let ctx = ServerCtx {
        app: app.clone(),
        tokens,
        mcp_token: mcp_token.clone(),
    };

    let router = Router::new()
        .route("/v1/hook", post(handle_hook))
        // Phase 8b: hunk-style notes broker. Same listener, new namespace.
        .route("/v1/notes", post(crate::agent_notes_broker::handle_notes))
        .route("/v1/mcp", post(handle_mcp))
        .layer(TimeoutLayer::new(REQUEST_TIMEOUT))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .with_state(ctx);

    // Prefer the persistent fixed port so the config pasted into claude/codex
    // keeps working across restarts. If it's already taken, fall back to an
    // OS-assigned port (degraded: config would need re-pasting) rather than
    // leaving the whole hook/MCP server unable to start.
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!(
                "[agent_hooks] fixed MCP port {port} unavailable ({e}); \
                 falling back to a random port — MCP config will need re-pasting"
            );
            tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .map_err(|e| format!("failed to bind hook listener: {e}"))?
        }
    };
    let local_addr = listener
        .local_addr()
        .map_err(|e| format!("failed to read listener addr: {e}"))?;
    let url = format!("http://{}/v1/hook", local_addr);

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            eprintln!("[agent_hooks] server crashed: {e}");
        }
    });

    Ok((url, mcp_token))
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn agent_hooks_url(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let info = state
        .hook_server
        .lock()
        .await
        .clone()
        .ok_or_else(|| "hook server not started yet".to_string())?;
    Ok(info.url)
}

#[tauri::command]
pub async fn project_mcp_info(state: tauri::State<'_, AppState>) -> Result<ProjectMcpInfo, String> {
    let info = state
        .hook_server
        .lock()
        .await
        .clone()
        .ok_or_else(|| "local agent server not started yet".to_string())?;
    Ok(ProjectMcpInfo {
        url: info.url.replace("/v1/hook", "/v1/mcp"),
        token: info.mcp_token,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_state_accepts_canonical_orca_values() {
        for s in [
            "working",
            "blocked",
            "waiting",
            "done",
            "idle",
            "permission",
            "interrupted",
        ] {
            assert!(parse_state(s).is_some(), "rejected {s}");
        }
    }

    #[test]
    fn parse_state_rejects_garbage() {
        assert!(parse_state("running").is_none());
        assert!(parse_state("").is_none());
        assert!(parse_state("Working").is_none()); // case-sensitive on purpose
    }

    #[test]
    fn project_mcp_exposes_revision_safe_ticket_tools() {
        let tools = project_mcp_tools();
        let names: Vec<_> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(
            names,
            vec![
                "xnaut_list_projects",
                "xnaut_list_tickets",
                "xnaut_create_ticket",
                "xnaut_update_ticket",
                "xnaut_list_documents",
                "xnaut_search_documents",
                "xnaut_read_document",
                "xnaut_create_document",
                "xnaut_update_document"
            ]
        );
        let update = tools
            .iter()
            .find(|tool| tool["name"] == "xnaut_update_ticket")
            .unwrap();
        assert_eq!(
            update["inputSchema"]["required"],
            json!(["id", "expected_revision"])
        );
        let update_document = tools.last().unwrap();
        assert_eq!(
            update_document["inputSchema"]["required"],
            json!(["project", "rel", "content", "expected_sha256"])
        );
    }

    #[test]
    fn project_document_paths_reject_scope_escapes() {
        assert_eq!(
            document_path("features/design.md").unwrap(),
            "features/design.md"
        );
        for invalid in [
            "../outside.md",
            "/absolute.md",
            ".secret/note.md",
            "folder\\escape.md",
            "C:/escape.md",
            "not-markdown.txt",
        ] {
            assert!(document_path(invalid).is_err(), "accepted {invalid}");
        }
    }
}
