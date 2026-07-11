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
use std::collections::HashMap;
use std::net::SocketAddr;
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
    ]
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

/// Spawns the listener. Returns the URL the hook scripts should POST to.
/// Bound to 127.0.0.1 on a random port chosen by the OS.
pub async fn start_server(
    app: AppHandle,
    tokens: HookTokenMap,
) -> Result<(String, String), String> {
    let mcp_token = Uuid::new_v4().to_string();
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

    let addr: SocketAddr = "127.0.0.1:0"
        .parse()
        .map_err(|e: std::net::AddrParseError| format!("bad listener addr: {e}"))?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("failed to bind hook listener: {e}"))?;
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
                "xnaut_update_ticket"
            ]
        );
        let update = tools.last().unwrap();
        assert_eq!(
            update["inputSchema"]["required"],
            json!(["id", "expected_revision"])
        );
    }
}
