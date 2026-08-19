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
//
// One response contract for every xnaut_* tool (XNAUT-193).
//
// Borrowed from ECC (github.com/affaan-m/ecc, MIT),
// `skills/agent-harness-construction/SKILL.md`, whose Observation Design
// section says every tool response should carry `status` (success|warning|
// error), `summary` (one-line result), `next_actions` (actionable follow-ups)
// and `artifacts` (file paths / IDs). Its anti-patterns name the two failures
// this prevents: "opaque tool output with no recovery hints" and "error-only
// output without next steps". An agent that has to infer what happened spends
// a turn guessing, and guesses wrong on the paths that matter.
//
// Three departures, all deliberate:
//
//   ONE PLACE, NOT TEN. ECC states the contract as a rule each tool follows.
//   Here it is applied once, where a tool result becomes an MCP reply, so no
//   tool can forget it and a new tool inherits it. What a tool does own is its
//   follow-ups, in `tool_next_actions`.
//
//   THE OLD PAYLOAD SURVIVES, UNDER `data`. These tools already have callers.
//   An envelope that replaces the answer is a rewrite, not a wrapper.
//
//   `warning` MEANS AN EMPTY ANSWER, not a degraded one. A list or search that
//   matched nothing is the case where an agent otherwise reads the tool as
//   broken and retries it unchanged. Naming that is the value of a third
//   status; we had no other use for one.

use crate::state::AppState;
use crate::status::{self, AgentStatus};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
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
    /// Where the agent was standing when it hit the boundary. This is what tells
    /// the decision log which project the boundary belongs to; without it a state
    /// change can only move a status dot.
    #[serde(default)]
    cwd: Option<String>,
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
    /// Same surface, no write tools. Handed to clients that should only read.
    pub read_token: String,
}

/// Tools that change something. Everything else is readable with either token.
const WRITE_TOOLS: &[&str] = &[
    "xnaut_create_ticket",
    "xnaut_update_ticket",
    "xnaut_create_document",
    "xnaut_update_document",
    "xnaut_log_decision",
];

/// The read-only bearer, derived from the write one rather than stored beside
/// it. Nothing extra to persist, nothing extra to migrate, and revoking the
/// write token revokes this with it.
pub fn read_only_token(mcp_token: &str) -> String {
    format!("{:x}", Sha256::digest(format!("{mcp_token}:read-only")))
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
                "content": { "type": "string" },
                "agent": { "type": "string", "description": "Who is writing. Recorded in the project event trail." }
            }),
            &["project", "rel", "content"],
        ),
        mcp_tool(
            "xnaut_update_document",
            "Update a project-scoped Markdown document only when its current SHA-256 matches expected_sha256.",
            json!({
                "project": { "type": "string" }, "rel": { "type": "string" },
                "content": { "type": "string" }, "expected_sha256": { "type": "string" },
                "agent": { "type": "string", "description": "Who is writing. Recorded in the project event trail." }
            }),
            &["project", "rel", "content", "expected_sha256"],
        ),
        mcp_tool(
            "xnaut_log_decision",
            "Record WHY a decision was made, at the moment it was made. Call this at a boundary \
             (starting or finishing a task, a verification passing or failing, a council verdict, \
             a merge) BEFORE moving on. One line of rationale, not a status report: what you chose, \
             what you rejected, and why. Set open=true when something is left unresolved, and give \
             it a `key` so a later entry can close it; an open item is never closed by anything else.",
            json!({
                "project": { "type": "string" },
                "role": { "type": "string", "description": "Which agent or persona is deciding." },
                "boundary": { "type": "string", "enum": crate::decisions::BOUNDARIES },
                "what": { "type": "string", "description": "The decision, in one line." },
                "why": { "type": "string", "description": "The reason. The point of this tool." },
                "alternatives": { "type": "string", "description": "What was considered and rejected." },
                "key": { "type": "string", "description": "Correlation key, so a later entry can close this one." },
                "open": { "type": "boolean", "description": "True when this leaves something unresolved." }
            }),
            &["project", "why"],
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
            "{}/Development",
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
    // `document_path` rejects `..`, but a symlinked directory inside the project
    // scope would still resolve outside the vault. Trust boundary, so check the
    // real filesystem, not just the string.
    crate::agent_profiles::reject_symlinks_in_rel(&crate::vault::vault_root("work")?, &vault_rel)?;
    if name == "xnaut_read_document" {
        let content = crate::vault::vault_note_read(vault_state, "work".into(), vault_rel.clone())?;
        return Ok(scoped_note_result(&project, &rel, &vault_rel, content));
    }
    let content = required_arg(&args, "content")?.to_owned();
    let actor = args
        .get("agent")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("mcp")
        .to_owned();
    if name == "xnaut_create_document" {
        crate::vault::vault_note_create(
            ctx.app.clone(),
            vault_state,
            "work".into(),
            vault_rel.clone(),
            Some(content.clone()),
        )?;
        record_document_event(
            ctx,
            "document.created",
            &project,
            &rel,
            &vault_rel,
            &content,
            &actor,
        )
        .await;
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
        // Structured, so the caller can re-read and merge instead of parsing prose.
        return Err(json!({
            "error": "document_conflict",
            "vault_rel": vault_rel,
            "expected_sha256": expected,
            "current_sha256": actual
        })
        .to_string());
    }
    crate::vault::vault_note_write(
        ctx.app.clone(),
        ctx.app.state::<crate::vault::VaultManager>(),
        "work".into(),
        vault_rel.clone(),
        content.clone(),
    )?;
    record_document_event(
        ctx,
        "document.updated",
        &project,
        &rel,
        &vault_rel,
        &content,
        &actor,
    )
    .await;
    Ok(scoped_note_result(&project, &rel, &vault_rel, content))
}

async fn record_document_event(
    ctx: &ServerCtx,
    event: &str,
    project: &str,
    rel: &str,
    vault_rel: &str,
    content: &str,
    actor: &str,
) {
    crate::project_management::record_document_event(
        ctx.app.state::<AppState>(),
        event,
        vault_rel,
        json!({
            "project": project,
            "rel": rel,
            "sha256": content_sha256(content),
            "actor": actor
        }),
    )
    .await;
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
        "xnaut_log_decision" => {
            let project = required_arg(&args, "project")?.to_owned();
            let text = |k: &str| {
                args.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let decision = crate::decisions::Decision {
                ts: String::new(),
                role: text("role"),
                boundary: text("boundary"),
                what: text("what"),
                why: text("why"),
                alternatives: text("alternatives"),
                key: text("key"),
                open: args.get("open").and_then(Value::as_bool).unwrap_or(false),
            };
            crate::decisions::append(&project, &decision);
            Ok(json!({ "logged": true, "project": project }))
        }
        "xnaut_list_documents"
        | "xnaut_search_documents"
        | "xnaut_read_document"
        | "xnaut_create_document"
        | "xnaut_update_document" => call_document_tool(ctx, name, args).await,
        _ => Err(format!("unknown xNAUT tool: {name}")),
    }
}

// The one response contract, defined in the file header.

/// Ids and paths only, capped, so a 500-row listing cannot push the rest of
/// the envelope out of the model's view. `data` still carries every row.
const MAX_ARTIFACTS: usize = 20;

/// What to do after each tool, keyed by name. Static, because the useful next
/// move is a property of the tool and not of the row it happened to return.
/// A tool missing from here answers with no next_actions, which
/// `every_tool_answers_in_the_envelope` fails on: a tool nobody can follow up
/// is exactly the omission worth catching before it ships.
fn tool_next_actions(name: &str) -> Vec<&'static str> {
    match name {
        "xnaut_list_projects" => {
            vec!["call xnaut_list_tickets with one of the returned project keys"]
        }
        "xnaut_list_tickets" => vec![
            "call xnaut_update_ticket with a ticket id and expected_revision set to its revision",
            "call xnaut_read_document on a path from a ticket's documentation field",
        ],
        "xnaut_create_ticket" | "xnaut_update_ticket" => vec![
            "the next update needs expected_revision set to data.revision",
            "call xnaut_log_decision to record why this ticket moved",
        ],
        "xnaut_list_documents" | "xnaut_search_documents" => {
            vec!["call xnaut_read_document with a rel from artifacts for its content and sha256"]
        }
        "xnaut_read_document" => {
            vec!["call xnaut_update_document with expected_sha256 set to data.sha256"]
        }
        "xnaut_create_document" | "xnaut_update_document" => vec![
            "a further edit needs expected_sha256 set to data.sha256",
            "point a ticket at it with xnaut_update_ticket documentation",
        ],
        "xnaut_log_decision" => vec!["nothing follows: the entry is appended"],
        _ => vec![],
    }
}

/// The handles an agent can pass to the next call without re-parsing `data`.
fn tool_artifacts(data: &Value) -> Vec<String> {
    let rows: Vec<&Value> = match data {
        Value::Array(rows) => rows.iter().collect(),
        single => vec![single],
    };
    rows.into_iter()
        .filter_map(|row| {
            ["vault_rel", "rel", "id", "key"]
                .iter()
                .find_map(|field| row.get(field).and_then(Value::as_str))
                .map(str::to_owned)
        })
        .take(MAX_ARTIFACTS)
        .collect()
}

/// One line, for a model. Errors arrive as prose or as JSON and both are
/// flattened, because a summary that wraps is a summary that gets skimmed.
fn one_line(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 200 {
        format!("{}...", flat.chars().take(200).collect::<String>())
    } else {
        flat
    }
}

fn tool_summary(name: &str, data: &Value, empty: bool) -> String {
    match data {
        Value::Array(_) if empty => format!("{name} matched nothing"),
        Value::Array(rows) => format!("{name} returned {} row(s)", rows.len()),
        _ => match ["vault_rel", "id", "project"]
            .iter()
            .find_map(|field| data.get(field).and_then(Value::as_str))
        {
            Some(subject) => format!("{name} succeeded on {subject}"),
            None => format!("{name} succeeded"),
        },
    }
}

fn success_envelope(name: &str, data: Value) -> Value {
    let empty = matches!(&data, Value::Array(rows) if rows.is_empty());
    let mut next: Vec<String> = tool_next_actions(name)
        .into_iter()
        .map(str::to_owned)
        .collect();
    if empty {
        // The tool worked. Repeating it verbatim is the wrong follow-up and
        // the one an agent reaches for when it reads empty as failure.
        next.insert(
            0,
            "nothing matched: check the project key or widen the query before retrying".into(),
        );
    }
    json!({
        "status": if empty { "warning" } else { "success" },
        "summary": tool_summary(name, &data, empty),
        "next_actions": next,
        "artifacts": tool_artifacts(&data),
        "data": data,
    })
}

fn error_envelope(name: &str, error: String) -> Value {
    // Every tool error is a string, except the document conflict, which is JSON
    // so the caller can merge rather than parse prose.
    let structured = serde_json::from_str::<Value>(&error)
        .ok()
        .filter(Value::is_object);
    let conflict = structured
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(Value::as_str)
        == Some("document_conflict");
    let next = if conflict {
        vec![
            "call xnaut_read_document on data.vault_rel for the content that landed instead".into(),
            format!("call {name} again with expected_sha256 set to data.current_sha256"),
        ]
    } else if let Some(missing) = error.strip_suffix(" is required") {
        vec![format!("call {name} again with {missing} set")]
    } else {
        vec![
            format!("fix the reported cause and call {name} again"),
            "stop and report if it fails the same way twice".into(),
        ]
    };
    json!({
        "status": "error",
        "summary": format!("{name} failed: {}", one_line(&error)),
        "next_actions": next,
        "artifacts": structured.as_ref().map(tool_artifacts).unwrap_or_default(),
        "data": structured.unwrap_or_else(|| json!({ "error": error })),
    })
}

/// The one place a tool answer becomes an MCP reply. Everything the server can
/// call goes through here, which is what makes the contract unskippable.
fn tool_call_result(name: &str, outcome: Result<Value, String>) -> Value {
    let failed = outcome.is_err();
    let envelope = match outcome {
        Ok(data) => success_envelope(name, data),
        Err(error) => error_envelope(name, error),
    };
    let mut result = json!({ "content": [{ "type": "text", "text": envelope.to_string() }] });
    if failed {
        // Clients branch on this before they read the text, so the envelope
        // does not replace it.
        result["isError"] = Value::Bool(true);
    }
    result
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
    let can_write = if token == ctx.mcp_token {
        true
    } else if token == read_only_token(&ctx.mcp_token) {
        false
    } else {
        return Err((StatusCode::UNAUTHORIZED, "invalid MCP token".into()));
    };
    let result = match request.method.as_str() {
        "initialize" => json!({
            "protocolVersion": "2025-03-26",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "xnaut-project-management", "version": env!("CARGO_PKG_VERSION") }
        }),
        "notifications/initialized" => Value::Null,
        "tools/list" => {
            let mut tools = project_mcp_tools();
            if !can_write {
                tools.retain(|tool| {
                    !tool["name"]
                        .as_str()
                        .is_some_and(|name| WRITE_TOOLS.contains(&name))
                });
            }
            json!({ "tools": tools })
        }
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
            if !can_write && WRITE_TOOLS.contains(&name) {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("{name} needs the write token"),
                ));
            }
            tool_call_result(name, call_project_tool(&ctx, name, args).await)
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

/// The automatic half of the decision log: the WHAT, captured without the agent
/// having to remember anything. The WHY stays the agent's job (xnaut_log_decision)
/// because no amount of telemetry reconstructs a judgement call, and an entry
/// landing here with an empty rationale is counted by `Brief::unexplained` rather
/// than quietly passed off as a decision.
///
/// Only real boundaries are recorded. `working` fires on every output ping, and a
/// log that captures it is an activity log with extra steps.
fn boundary_name(state: AgentStatus) -> Option<&'static str> {
    match state {
        AgentStatus::Done => Some("task.done"),
        AgentStatus::Blocked => Some("agent.blocked"),
        AgentStatus::Waiting => Some("agent.waiting"),
        AgentStatus::Interrupted => Some("agent.interrupted"),
        _ => None,
    }
}

fn record_boundary(session_id: &str, payload: &HookPayload, state: AgentStatus) {
    let Some(boundary) = boundary_name(state) else {
        return;
    };
    // No cwd means no project, and a decision filed against a guess is worse than
    // one not filed at all. Older hook scripts simply do not contribute here.
    let Some(cwd) = payload.cwd.as_deref().filter(|c| !c.trim().is_empty()) else {
        return;
    };
    let what = match (payload.tool_name.as_deref(), payload.prompt.as_deref()) {
        (Some(tool), _) if !tool.trim().is_empty() => format!("{boundary} at {tool}"),
        (_, Some(p)) if !p.trim().is_empty() => {
            format!("{boundary}: {}", p.chars().take(160).collect::<String>())
        }
        _ => boundary.to_string(),
    };
    crate::decisions::append(
        &crate::engram::project_from_cwd(cwd),
        &crate::decisions::Decision {
            ts: String::new(),
            role: "hook".into(),
            boundary: boundary.into(),
            what,
            // Deliberately empty. See above.
            why: String::new(),
            alternatives: String::new(),
            // The session is the correlation key, so an agent that blocks and then
            // finishes closes its own open item. Nothing else closes it.
            key: session_id.to_string(),
            open: state != AgentStatus::Done,
        },
    );
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

    record_boundary(&session_id, &payload, new_state);

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
/// What an agent hands to `open`: a URL, or a path it just wrote.
#[derive(Deserialize)]
pub struct OpenRequest {
    pub target: String,
}

/// Normalise that into something the in-app browser can load. Anything that is
/// neither a web URL nor an absolute path is refused rather than guessed at —
/// the shim resolves relative paths before it gets here, so a leftover relative
/// path means we do not know which directory it belonged to.
pub fn browser_url(target: &str) -> Option<String> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    if target.starts_with("http://") || target.starts_with("https://") || target.starts_with("file://") {
        return Some(target.to_string());
    }
    if Path::new(target).is_absolute() {
        return Some(format!("file://{target}"));
    }
    None
}

/// An agent showing André a page opens it HERE, in a browser tab next to the
/// work, rather than in a system window stacked behind the app. Reached by the
/// `open` shim on the agent's PATH (see agents.rs) as well as directly.
pub async fn handle_open(
    State(ctx): State<ServerCtx>,
    headers: HeaderMap,
    Json(req): Json<OpenRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let session = crate::inbox::authorize(&ctx, &headers).await?;
    let url = browser_url(&req.target)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("cannot open {:?}", req.target)))?;
    // Which agent produced it. The page belongs next to that agent's thread,
    // not in a browser tab of its own — the artifact IS part of the answer.
    // No session (a script reaching in over the MCP bearer) means no agent to
    // attach it to, and the frontend falls back to a tab.
    let agent_id = match &session {
        Some(session_id) => ctx
            .app
            .state::<AppState>()
            .agent_sessions
            .lock()
            .await
            .get(session_id)
            .map(|meta| meta.agent_id.clone()),
        None => None,
    };
    let _ = ctx.app.emit(
        "open-in-browser",
        json!({ "url": url, "agent_id": agent_id, "session_id": session }),
    );
    Ok(Json(json!({ "opened": url, "agent_id": agent_id })))
}

/// A document an agent wants the owner to READ.
#[derive(Deserialize)]
pub struct DocumentRequest {
    #[serde(default)]
    pub title: String,
    pub content: String,
    /// Which agent's split it belongs in. Normally resolved from the session.
    #[serde(default)]
    pub agent: Option<String>,
}

/// Put a document in the split beside the conversation.
///
/// The chat loop has write_document; a coding RUN had nothing, so an agent
/// asked for a blog post wrote a .md and ran `open` on it, which handed it to
/// Xcode. Every agent needs the same surface, whatever it is running inside.
pub async fn handle_document(
    State(ctx): State<ServerCtx>,
    headers: HeaderMap,
    Json(req): Json<DocumentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let session = crate::inbox::authorize(&ctx, &headers).await?;
    if req.content.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "a document needs content".into()));
    }
    let agent = match req.agent.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(agent) => agent.to_string(),
        None => match &session {
            Some(session_id) => ctx
                .app
                .state::<AppState>()
                .agent_sessions
                .lock()
                .await
                .get(session_id)
                .map(|meta| meta.agent_id.clone())
                .unwrap_or_else(|| "nautbot".to_string()),
            None => "nautbot".to_string(),
        },
    };
    let document = crate::canvas::Document {
        title: req.title.trim().to_string(),
        content: req.content,
        ..Default::default()
    };
    let saved = crate::canvas::write_document(&agent, document, crate::canvas::now_iso())
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    let _ = ctx.app.emit("document-changed", json!({ "key": agent }));
    Ok(Json(json!({
        "shown": true,
        "agent": agent,
        "title": saved.title,
        "words": saved.content.split_whitespace().count()
    })))
}

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

    let short = Router::new()
        .route("/v1/hook", post(handle_hook))
        .route("/v1/veto", post(crate::veto::handle_veto))
        // Phase 8b: hunk-style notes broker. Same listener, new namespace.
        .route("/v1/notes", post(crate::agent_notes_broker::handle_notes))
        .route("/v1/mcp", post(handle_mcp))
        .route("/v1/open", post(handle_open))
        .route("/v1/document", post(handle_document))
        .layer(TimeoutLayer::new(REQUEST_TIMEOUT));

    // Mesh inbox (XNAUT-156). These routes PARK: an agent asking André waits
    // on the open request until he answers, so the 5s timeout above must not
    // apply here. The handler caps its own wait and the caller re-issues.
    let inbox = Router::new()
        .route("/v1/inbox/notify", post(crate::inbox::handle_notify))
        .route("/v1/inbox/todo", post(crate::inbox::handle_todo))
        .route("/v1/inbox/ask", post(crate::inbox::handle_ask))
        .route("/v1/inbox/approve", post(crate::inbox::handle_approve))
        .route("/v1/inbox/wait/:id", get(crate::inbox::handle_wait))
        .route("/v1/inbox/list", get(crate::inbox::handle_list))
        // Plan Canvas (XNAUT-192). Same parking rules: the agent holds
        // this request open while a human reads its plan.
        .route("/v1/plan/review", post(crate::plan_review::handle_review))
        .route(
            "/v1/plan/review/:id",
            get(crate::plan_review::handle_review_wait),
        )
        .layer(TimeoutLayer::new(Duration::from_secs(310)));

    let router = short
        .merge(inbox)
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
        read_token: read_only_token(&info.mcp_token),
        token: info.mcp_token,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pages_and_absolute_paths_reach_the_in_app_browser() {
        assert_eq!(
            browser_url("https://example.com/x").as_deref(),
            Some("https://example.com/x")
        );
        assert_eq!(
            browser_url("/tmp/game.html").as_deref(),
            Some("file:///tmp/game.html")
        );
        // A relative path lost its directory on the way here; guessing at it
        // would open the wrong file, so the shim must resolve it first.
        assert!(browser_url("game.html").is_none());
        assert!(browser_url("   ").is_none());
    }

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

    /// `working` fires on every output ping. If it ever became a boundary the
    /// decision log would fill with noise and stop being readable, which is the
    /// exact failure it was built to avoid.
    #[test]
    fn only_real_boundaries_reach_the_decision_log() {
        assert_eq!(boundary_name(AgentStatus::Working), None);
        assert_eq!(boundary_name(AgentStatus::Idle), None);
        assert_eq!(boundary_name(AgentStatus::Done), Some("task.done"));
        assert_eq!(boundary_name(AgentStatus::Blocked), Some("agent.blocked"));
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
                "xnaut_update_document",
                "xnaut_log_decision"
            ]
        );
        // `why` is required: a boundary logged without a rationale is the thing
        // the decision log exists to make impossible to do by accident.
        let decision = tools
            .iter()
            .find(|tool| tool["name"] == "xnaut_log_decision")
            .unwrap();
        assert_eq!(
            decision["inputSchema"]["required"],
            json!(["project", "why"])
        );
        let update = tools
            .iter()
            .find(|tool| tool["name"] == "xnaut_update_ticket")
            .unwrap();
        assert_eq!(
            update["inputSchema"]["required"],
            json!(["id", "expected_revision"])
        );
        let update_document = tools
            .iter()
            .find(|tool| tool["name"] == "xnaut_update_document")
            .unwrap();
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

    /// `document_path` only sees a string, so a symlinked directory inside the
    /// project scope would pass it and still land outside the vault. This is the
    /// guard `call_document_tool` runs on the resolved vault path.
    #[test]
    fn document_writes_reject_symlinked_scope_dirs() {
        let root = std::env::temp_dir().join(format!("xnaut-mcp-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("xNAUT/Development")).unwrap();
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("xNAUT/Development/features")).unwrap();

        assert!(
            crate::agent_profiles::reject_symlinks_in_rel(
                &root,
                "xNAUT/Development/features/design.md"
            )
            .is_err(),
            "accepted a path through a symlinked directory"
        );
        assert!(crate::agent_profiles::reject_symlinks_in_rel(
            &root,
            "xNAUT/Development/notes/design.md"
        )
        .is_ok());

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The representative tool: a read whose payload was already rich. The
    /// envelope has to add the four fields without moving what was there.
    #[test]
    fn a_tool_answer_carries_status_summary_next_actions_and_artifacts() {
        let payload = scoped_note_result(
            "XNAUT",
            "features/design.md",
            "xNAUT/Development/features/design.md",
            "body".into(),
        );
        let reply = tool_call_result("xnaut_read_document", Ok(payload.clone()));
        assert!(reply.get("isError").is_none(), "a read is not an error");

        let envelope: Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(envelope["status"], "success");
        assert_eq!(
            envelope["summary"],
            "xnaut_read_document succeeded on xNAUT/Development/features/design.md"
        );
        assert!(envelope["next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action.as_str().unwrap().contains("expected_sha256")));
        assert_eq!(
            envelope["artifacts"],
            json!(["xNAUT/Development/features/design.md"])
        );
        // The old payload is intact, so nothing that already reads these tools
        // loses a field.
        assert_eq!(envelope["data"], payload);
    }

    /// A conflict is the one error an agent can actually recover from, so it is
    /// the one whose next_actions have to say how.
    #[test]
    fn a_document_conflict_says_how_to_recover() {
        let conflict = json!({
            "error": "document_conflict",
            "vault_rel": "xNAUT/Development/features/design.md",
            "expected_sha256": "aaa",
            "current_sha256": "bbb"
        })
        .to_string();
        let reply = tool_call_result("xnaut_update_document", Err(conflict));
        assert_eq!(reply["isError"], json!(true));

        let envelope: Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(envelope["status"], "error");
        assert_eq!(envelope["data"]["current_sha256"], "bbb");
        assert_eq!(
            envelope["artifacts"],
            json!(["xNAUT/Development/features/design.md"])
        );
        let actions = envelope["next_actions"].to_string();
        assert!(actions.contains("xnaut_read_document"), "{actions}");
        assert!(actions.contains("current_sha256"), "{actions}");
    }

    /// An empty list is not a failure, and an agent that reads it as one
    /// retries the same call unchanged. `warning` exists for this.
    #[test]
    fn an_empty_result_is_a_warning_not_a_success() {
        let reply = tool_call_result("xnaut_search_documents", Ok(json!([])));
        let envelope: Value =
            serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(envelope["status"], "warning");
        assert_eq!(envelope["summary"], "xnaut_search_documents matched nothing");
        assert!(envelope["next_actions"][0]
            .as_str()
            .unwrap()
            .contains("widen the query"));
    }

    /// The guard the ticket asks for: no tool may answer with a bare value.
    /// Every name the server advertises is put through the same helper on both
    /// a success and a failure, and the envelope has to be complete for all of
    /// them. next_actions is the part a newly added tool forgets, so an empty
    /// one fails here rather than reaching an agent.
    #[test]
    fn every_tool_answers_in_the_envelope() {
        for tool in project_mcp_tools() {
            let name = tool["name"].as_str().unwrap();
            for outcome in [
                Ok(json!({ "id": "XNAUT-1", "revision": 3 })),
                Err("project is required".to_string()),
            ] {
                let failed = outcome.is_err();
                let reply = tool_call_result(name, outcome);
                let text = reply["content"][0]["text"].as_str().unwrap().to_owned();
                let envelope: Value = serde_json::from_str(&text).unwrap();
                for field in ["status", "summary", "next_actions", "artifacts", "data"] {
                    assert!(
                        envelope.get(field).is_some(),
                        "{name} answered with a bare value, no {field}: {text}"
                    );
                }
                assert!(
                    matches!(
                        envelope["status"].as_str(),
                        Some("success" | "warning" | "error")
                    ),
                    "{name} status is outside the contract: {}",
                    envelope["status"]
                );
                assert_eq!(
                    envelope["status"] == "error",
                    failed,
                    "{name} status does not match what happened"
                );
                assert!(
                    !envelope["summary"].as_str().unwrap().is_empty(),
                    "{name} has an empty summary"
                );
                assert!(
                    !envelope["next_actions"].as_array().unwrap().is_empty(),
                    "{name} tells the agent nothing to do next"
                );
                assert_eq!(reply.get("isError").is_some(), failed, "{name} isError");
            }
        }
    }

    /// The read-only bearer must not even see the write tools, or an agent will
    /// call one and get a 403 it cannot act on.
    #[test]
    fn read_only_token_hides_every_write_tool() {
        assert_ne!(read_only_token("secret"), "secret");
        assert_eq!(read_only_token("secret"), read_only_token("secret"));

        let readable: Vec<_> = project_mcp_tools()
            .into_iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
            .filter(|name| !WRITE_TOOLS.contains(&name.as_str()))
            .collect();
        assert_eq!(
            readable,
            vec![
                "xnaut_list_projects",
                "xnaut_list_tickets",
                "xnaut_list_documents",
                "xnaut_search_documents",
                "xnaut_read_document"
            ]
        );
    }
}
