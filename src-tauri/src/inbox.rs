// Mesh — the human inbox (XNAUT-156). Agents ask André and WAIT.
//
// Design doc: work:xnaut/Development/features/2026-08-14_Inbox-Design.md
//
// The surface is called "Mesh" in the UI; the store and API keep the
// technical name `inbox` so they never collide with the Engram mesh
// transport underneath.
//
// Why an HTTP API and not (only) an MCP server: MCP tool schemas load into
// EVERY agent session whether used or not, and the headless BAMT-style runs
// deliberately get no user MCP servers at all. One prompt line teaching a
// curl-able endpoint costs ~30 tokens per session instead of a six-tool
// schema, and it reaches sandboxes and scripts too.
//
// Why append-only JSONL and not memory: a blocking approval can sit open for
// hours across an app restart. `wait` resolves from the file, so a restart
// mid-question loses nothing — the same lesson the work log taught us
// (XNAUT-139).

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Server-side ceiling for one long-poll request. The caller re-issues
/// `wait` after a timeout, so a three-hour approval still works without
/// holding a socket open for three hours.
const MAX_WAIT_MS: u64 = 300_000;
// XNAUT-244: the FIRST response must beat a normal HTTP client timeout, or
// the caller never learns the id and cannot even fall back to polling
// /v1/inbox/wait/:id. NautBot hit exactly that: "the Mesh ask endpoint did
// not return a question ID, so there is nothing I can safely wait on."
// 20s is under every default client timeout we ship against (reqwest 30s,
// curl none but scripts usually 30s, the shim 30s) and long enough that a
// human who is already looking at the screen answers in the first call.
// Longer waits are the CALLER's to ask for via timeout_ms, and the answer
// still arrives through the documented poll either way.
const DEFAULT_WAIT_MS: u64 = 20_000;
const POLL_INTERVAL_MS: u64 = 500;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct InboxOption {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub recommended: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct InboxLink {
    pub label: String,
    pub href: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct InboxItem {
    pub id: String,
    pub at: String,
    #[serde(default)]
    pub project: String,
    /// Agent handle that posted this, e.g. "builder". "system" for infra.
    #[serde(default)]
    pub from: String,
    /// notify | ask | approve | todo
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// info | warn | error
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub options: Vec<InboxOption>,
    /// Agent-supplied run state — what fills the detail view's table so a
    /// decision never requires opening the session.
    #[serde(default)]
    pub context: BTreeMap<String, String>,
    #[serde(default)]
    pub links: Vec<InboxLink>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub ticket: Option<String>,
    /// open | answered | approved | denied | done | archived
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub answered_at: Option<String>,
}

impl InboxItem {
    pub fn is_open(&self) -> bool {
        self.status == "open"
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "record", rename_all = "lowercase")]
enum InboxRecord {
    Created(InboxItem),
    Answer {
        id: String,
        #[serde(default)]
        answer: Option<String>,
        /// approved | denied — set for `approve` items.
        #[serde(default)]
        decision: Option<String>,
        at: String,
    },
    Status {
        id: String,
        status: String,
        at: String,
    },
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn inbox_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut").join("inbox"))
        .unwrap_or_else(|| std::path::PathBuf::from(".xnaut/inbox"))
}

/// One file per project. Anything that is not alphanumeric/-/_ collapses to
/// `-` so a project name can never escape the inbox directory.
pub fn project_slug(project: &str) -> String {
    let cleaned: String = project
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "global".to_string()
    } else {
        trimmed
    }
}

fn store_path(project: &str) -> std::path::PathBuf {
    inbox_dir().join(format!("{}.jsonl", project_slug(project)))
}

fn append_record(project: &str, record: &InboxRecord) -> Result<(), String> {
    let path = store_path(project);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create the inbox directory: {e}"))?;
    }
    let line = serde_json::to_string(record).map_err(|e| format!("inbox encode failed: {e}"))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("could not open the inbox store: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("could not write the inbox store: {e}"))
}

/// Fold the append-only log into current item state. Unknown or malformed
/// lines are skipped rather than failing the whole read: a half-written line
/// from a crash must not hide every pending approval.
pub fn fold_items(contents: &str) -> Vec<InboxItem> {
    let mut order: Vec<String> = Vec::new();
    let mut items: BTreeMap<String, InboxItem> = BTreeMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let record: InboxRecord = match serde_json::from_str(line) {
            Ok(record) => record,
            Err(_) => continue,
        };
        match record {
            InboxRecord::Created(mut item) => {
                if item.status.is_empty() {
                    item.status = "open".to_string();
                }
                if !items.contains_key(&item.id) {
                    order.push(item.id.clone());
                }
                items.insert(item.id.clone(), item);
            }
            InboxRecord::Answer {
                id,
                answer,
                decision,
                at,
            } => {
                if let Some(item) = items.get_mut(&id) {
                    item.answer = answer.clone();
                    item.answered_at = Some(at);
                    item.status = decision.unwrap_or_else(|| "answered".to_string());
                }
            }
            InboxRecord::Status { id, status, at } => {
                if let Some(item) = items.get_mut(&id) {
                    item.status = status;
                    if item.answered_at.is_none() {
                        item.answered_at = Some(at);
                    }
                }
            }
        }
    }
    // Newest first — the inbox is read top-down.
    order
        .into_iter()
        .rev()
        .filter_map(|id| items.remove(&id))
        .filter(|item| item.status != "deleted")
        .collect()
}

fn read_items(project: &str) -> Vec<InboxItem> {
    let contents = std::fs::read_to_string(store_path(project)).unwrap_or_default();
    fold_items(&contents)
}

fn all_projects() -> Vec<String> {
    let dir = inbox_dir();
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names
}

pub(crate) fn find_item(id: &str) -> Option<(String, InboxItem)> {
    for project in all_projects() {
        if let Some(item) = read_items(&project).into_iter().find(|item| item.id == id) {
            return Some((project, item));
        }
    }
    None
}

/// Secrets must never reach the store: an agent pasting an env dump into a
/// question would otherwise persist it in plain text forever.
fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let lowered = line.to_ascii_lowercase();
        let looks_secret = ["token", "api_key", "apikey", "secret", "password", "bearer "]
            .iter()
            .any(|needle| lowered.contains(needle));
        if looks_secret && line.contains('=') || looks_secret && line.contains(':') {
            let cut = line.find(['=', ':']).unwrap_or(0);
            out.push_str(&line[..=cut]);
            out.push_str(" [redacted]");
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

// ---- creation ------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct PostRequest {
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub from: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub options: Vec<InboxOption>,
    #[serde(default)]
    pub context: BTreeMap<String, String>,
    #[serde(default)]
    pub links: Vec<InboxLink>,
    #[serde(default)]
    pub ticket: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Files this outcome changed (XNAUT-190).
    ///
    /// An agent finishing used to report prose, so "what did it touch" could
    /// only be answered by reading the sentence or opening the diff. ECC's
    /// comms module names this Completed{summary, files_changed}; this is the
    /// second half, and it makes a finished run reviewable without opening it.
    #[serde(default, alias = "files_changed")]
    pub files: Vec<String>,
}

fn create_item(kind: &str, req: PostRequest, session_id: Option<String>) -> Result<InboxItem, String> {
    if req.title.trim().is_empty() {
        return Err("an inbox item needs a title".to_string());
    }
    let item = InboxItem {
        id: format!("in-{}", uuid::Uuid::new_v4()),
        at: now_iso(),
        project: req.project.clone(),
        from: if req.from.trim().is_empty() {
            "system".to_string()
        } else {
            req.from.trim().to_string()
        },
        kind: kind.to_string(),
        title: redact(req.title.trim()),
        body: redact(&req.body),
        level: if req.level.trim().is_empty() {
            "info".to_string()
        } else {
            req.level.trim().to_string()
        },
        options: req.options,
        context: {
            let mut context = req.context;
            if !req.files.is_empty() {
                context.insert("files".to_string(), req.files.join("\n"));
            }
            context
        },
        links: req.links,
        session_id,
        ticket: req.ticket,
        // notify and todo need no reply, so they are not "open" work.
        status: if kind == "ask" || kind == "approve" {
            "open".to_string()
        } else if kind == "todo" {
            "open".to_string()
        } else {
            "done".to_string()
        },
        answer: None,
        answered_at: None,
    };
    append_record(&item.project, &InboxRecord::Created(item.clone()))?;
    Ok(item)
}

fn record_answer(
    id: &str,
    answer: Option<String>,
    decision: Option<String>,
) -> Result<InboxItem, String> {
    let (project, _) = find_item(id).ok_or_else(|| format!("inbox item not found: {id}"))?;
    append_record(
        &project,
        &InboxRecord::Answer {
            id: id.to_string(),
            answer: answer.map(|a| redact(&a)),
            decision,
            at: now_iso(),
        },
    )?;
    read_items(&project)
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("inbox item vanished after the answer: {id}"))
}

fn record_status(id: &str, status: &str) -> Result<InboxItem, String> {
    let (project, _) = find_item(id).ok_or_else(|| format!("inbox item not found: {id}"))?;
    append_record(
        &project,
        &InboxRecord::Status {
            id: id.to_string(),
            status: status.to_string(),
            at: now_iso(),
        },
    )?;
    read_items(&project)
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("inbox item vanished after the status change: {id}"))
}

fn announce(app: &AppHandle, item: &InboxItem) {
    // Push (1.22.2 item 1): the inbox already parks a blocked agent; without
    // this the phone only finds out if someone happens to be looking. Only
    // the kinds a human must act on push; notify/todo would train the owner
    // to ignore the sound.
    if item.kind == "ask" || item.kind == "approve" {
        crate::push::notify(crate::push::PushNote {
            title: item.title.clone(),
            body: format!(
                "{}{}",
                if item.from.is_empty() { String::new() } else { format!("@{} · ", item.from) },
                item.project
            ),
            kind: item.kind.clone(),
            inbox_id: Some(item.id.clone()),
            project: Some(item.project.clone()),
        });
    }
    let _ = app.emit("inbox-changed", item);
}

// ---- HTTP surface --------------------------------------------------------

async fn session_for(ctx: &crate::agent_hooks::ServerCtx, headers: &HeaderMap) -> Option<String> {
    if let Some(token) = headers.get("x-xnaut-session").and_then(|v| v.to_str().ok()) {
        let map = ctx.tokens.lock().await;
        if let Some(session) = map.get(token) {
            return Some(session.clone());
        }
    }
    None
}

/// Either a live session token (agent launched by us) or the MCP bearer
/// (sandboxes, scripts, CI reaching in over the bridge).
pub(crate) async fn authorize(
    ctx: &crate::agent_hooks::ServerCtx,
    headers: &HeaderMap,
) -> Result<Option<String>, (StatusCode, String)> {
    if let Some(session) = session_for(ctx, headers).await {
        return Ok(Some(session));
    }
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !ctx.mcp_token.is_empty() && bearer == ctx.mcp_token {
        return Ok(None);
    }
    Err((
        StatusCode::UNAUTHORIZED,
        "missing X-Xnaut-Session or Bearer token".to_string(),
    ))
}

pub(crate) async fn wait_for_answer(id: &str, timeout_ms: u64) -> Option<InboxItem> {
    let budget = timeout_ms.min(MAX_WAIT_MS);
    let deadline = std::time::Instant::now() + Duration::from_millis(budget);
    loop {
        if let Some((_, item)) = find_item(id) {
            if !item.is_open() {
                return Some(item);
            }
        }
        if std::time::Instant::now() >= deadline {
            return find_item(id).map(|(_, item)| item);
        }
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

/// Create an inbox item and put it on screen, without waiting for the answer.
///
/// The veto needs this half on its own (XNAUT-189): the hook script cannot hold
/// a request open for as long as a human takes, so the veto answers "ask" with
/// an id immediately and the script waits on /v1/inbox/wait/:id afterwards.
pub fn create_and_announce(
    app: &AppHandle,
    kind: &str,
    req: PostRequest,
    session_id: Option<String>,
) -> Result<InboxItem, String> {
    let item = create_item(kind, req, session_id)?;
    announce(app, &item);
    Ok(item)
}

pub async fn handle_notify(
    State(ctx): State<crate::agent_hooks::ServerCtx>,
    headers: HeaderMap,
    Json(req): Json<PostRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let session = authorize(&ctx, &headers).await?;
    let item = create_item("notify", req, session).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    announce(&ctx.app, &item);
    Ok(Json(json!({ "id": item.id, "status": item.status })))
}

pub async fn handle_todo(
    State(ctx): State<crate::agent_hooks::ServerCtx>,
    headers: HeaderMap,
    Json(req): Json<PostRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let session = authorize(&ctx, &headers).await?;
    let item = create_item("todo", req, session).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    announce(&ctx.app, &item);
    Ok(Json(json!({ "id": item.id, "status": item.status })))
}

async fn create_and_wait(
    ctx: crate::agent_hooks::ServerCtx,
    headers: HeaderMap,
    req: PostRequest,
    kind: &str,
) -> Result<Json<InboxItem>, (StatusCode, String)> {
    let session = authorize(&ctx, &headers).await?;
    let timeout = req.timeout_ms.unwrap_or(DEFAULT_WAIT_MS);
    let item = create_item(kind, req, session).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    announce(&ctx.app, &item);
    // wait_for_answer returns the item either way: answered, or still open
    // once the budget is spent. Returning it rather than nothing is what
    // makes the id reachable, which is the whole point of the fix.
    let settled = wait_for_answer(&item.id, timeout).await.unwrap_or(item);
    Ok(Json(settled))
}

pub async fn handle_ask(
    State(ctx): State<crate::agent_hooks::ServerCtx>,
    headers: HeaderMap,
    Json(req): Json<PostRequest>,
) -> Result<Json<InboxItem>, (StatusCode, String)> {
    create_and_wait(ctx, headers, req, "ask").await
}

pub async fn handle_approve(
    State(ctx): State<crate::agent_hooks::ServerCtx>,
    headers: HeaderMap,
    Json(req): Json<PostRequest>,
) -> Result<Json<InboxItem>, (StatusCode, String)> {
    create_and_wait(ctx, headers, req, "approve").await
}

#[derive(Debug, Deserialize)]
pub struct WaitQuery {
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

pub async fn handle_wait(
    State(ctx): State<crate::agent_hooks::ServerCtx>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<WaitQuery>,
) -> Result<Json<InboxItem>, (StatusCode, String)> {
    authorize(&ctx, &headers).await?;
    let timeout = query.timeout_ms.unwrap_or(DEFAULT_WAIT_MS);
    wait_for_answer(&id, timeout)
        .await
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("inbox item not found: {id}")))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

pub async fn handle_list(
    State(ctx): State<crate::agent_hooks::ServerCtx>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<InboxItem>>, (StatusCode, String)> {
    authorize(&ctx, &headers).await?;
    Ok(Json(collect(query.project.as_deref(), query.status.as_deref())))
}

fn collect(project: Option<&str>, status: Option<&str>) -> Vec<InboxItem> {
    let projects = match project {
        Some(p) if !p.trim().is_empty() => vec![project_slug(p)],
        _ => all_projects(),
    };
    let mut items: Vec<InboxItem> = projects.iter().flat_map(|p| read_items(p)).collect();
    if let Some(status) = status.filter(|s| !s.trim().is_empty()) {
        items.retain(|item| item.status == status);
    }
    items.sort_by(|a, b| b.at.cmp(&a.at));
    items
}

// ---- Tauri commands (the Mesh UI) ---------------------------------------

#[tauri::command]
pub fn inbox_list(project: Option<String>, status: Option<String>) -> Result<Vec<InboxItem>, String> {
    Ok(collect(project.as_deref(), status.as_deref()))
}

#[tauri::command]
pub fn inbox_answer(app: AppHandle, id: String, answer: String) -> Result<InboxItem, String> {
    let item = record_answer(&id, Some(answer), None)?;
    announce(&app, &item);
    Ok(item)
}

#[tauri::command]
pub fn inbox_decide(app: AppHandle, id: String, decision: String) -> Result<InboxItem, String> {
    let decision = decision.trim().to_ascii_lowercase();
    if decision != "approved" && decision != "denied" {
        return Err("decision must be approved or denied".to_string());
    }
    let item = record_answer(&id, None, Some(decision))?;
    announce(&app, &item);
    Ok(item)
}

#[tauri::command]
pub fn inbox_set_status(app: AppHandle, id: String, status: String) -> Result<InboxItem, String> {
    let status = status.trim().to_ascii_lowercase();
    if !["open", "done", "archived", "deleted"].contains(&status.as_str()) {
        return Err("status must be open, done, archived or deleted".to_string());
    }
    let item = record_status(&id, &status)?;
    announce(&app, &item);
    Ok(item)
}

/// Bulk approve/deny/archive from the selection bar. Returns how many items
/// actually changed — a partial failure must not look like a full success.
#[tauri::command]
pub fn inbox_bulk(app: AppHandle, ids: Vec<String>, action: String) -> Result<usize, String> {
    let action = action.trim().to_ascii_lowercase();
    let mut changed = 0usize;
    for id in ids {
        let result = match action.as_str() {
            "approve" => record_answer(&id, None, Some("approved".to_string())),
            "deny" => record_answer(&id, None, Some("denied".to_string())),
            "archive" => record_status(&id, "archived"),
            "done" => record_status(&id, "done"),
            other => return Err(format!("unknown bulk action: {other}")),
        };
        if let Ok(item) = result {
            announce(&app, &item);
            changed += 1;
        }
    }
    Ok(changed)
}

/// Lets NautBot (or the UI) file an item without going through HTTP.
#[tauri::command]
pub fn inbox_post(app: AppHandle, kind: String, req: PostRequest) -> Result<InboxItem, String> {
    let kind = kind.trim().to_ascii_lowercase();
    if !["notify", "ask", "approve", "todo"].contains(&kind.as_str()) {
        return Err("kind must be notify, ask, approve or todo".to_string());
    }
    let item = create_item(&kind, req, None)?;
    announce(&app, &item);
    Ok(item)
}

#[cfg(test)]
mod tests {
    /// XNAUT-244: the first response has to arrive before a normal client
    /// gives up, or the caller never learns the id and the documented
    /// /v1/inbox/wait/:id fallback is unreachable. NautBot reported exactly
    /// this failure. The number matters less than staying under the
    /// timeouts our own callers use, so pin the ceiling.
    #[test]
    fn the_first_ask_response_beats_a_normal_client_timeout() {
        assert!(
            super::DEFAULT_WAIT_MS <= 25_000,
            "an ask that blocks longer than a client's timeout returns nothing at all, id included"
        );
        assert!(
            super::MAX_WAIT_MS >= super::DEFAULT_WAIT_MS,
            "a caller must still be able to ask for a longer wait"
        );
    }


    use super::*;

    fn created(id: &str, kind: &str) -> String {
        serde_json::to_string(&InboxRecord::Created(InboxItem {
            id: id.to_string(),
            at: "2026-08-14T20:00:00Z".to_string(),
            project: "xnaut".to_string(),
            from: "builder".to_string(),
            kind: kind.to_string(),
            title: "Approve the migration?".to_string(),
            body: String::new(),
            level: "warn".to_string(),
            options: vec![],
            context: BTreeMap::new(),
            links: vec![],
            session_id: None,
            ticket: None,
            status: String::new(),
            answer: None,
            answered_at: None,
        }))
        .unwrap()
    }

    #[test]
    fn a_created_ask_folds_to_open() {
        let items = fold_items(&created("in-1", "ask"));
        assert_eq!(items.len(), 1);
        assert!(items[0].is_open(), "a fresh ask must block on the human");
    }

    #[test]
    fn an_answer_closes_the_item_and_keeps_the_text() {
        let log = format!(
            "{}\n{}",
            created("in-1", "ask"),
            serde_json::to_string(&InboxRecord::Answer {
                id: "in-1".into(),
                answer: Some("run the full suite".into()),
                decision: None,
                at: "2026-08-14T20:05:00Z".into(),
            })
            .unwrap()
        );
        let items = fold_items(&log);
        assert_eq!(items[0].status, "answered");
        assert_eq!(items[0].answer.as_deref(), Some("run the full suite"));
        assert!(!items[0].is_open(), "an answered item must stop blocking");
    }

    #[test]
    fn a_decision_records_approved_or_denied() {
        let log = format!(
            "{}\n{}",
            created("in-2", "approve"),
            serde_json::to_string(&InboxRecord::Answer {
                id: "in-2".into(),
                answer: None,
                decision: Some("denied".into()),
                at: "2026-08-14T20:06:00Z".into(),
            })
            .unwrap()
        );
        assert_eq!(fold_items(&log)[0].status, "denied");
    }

    #[test]
    fn a_half_written_line_does_not_hide_the_rest() {
        // A crash mid-append must not make pending approvals invisible.
        let log = format!("{}\n{{\"record\":\"crea", created("in-3", "ask"));
        assert_eq!(fold_items(&log).len(), 1);
    }

    #[test]
    fn deleted_items_leave_the_list() {
        let log = format!(
            "{}\n{}",
            created("in-4", "notify"),
            serde_json::to_string(&InboxRecord::Status {
                id: "in-4".into(),
                status: "deleted".into(),
                at: "2026-08-14T20:07:00Z".into(),
            })
            .unwrap()
        );
        assert!(fold_items(&log).is_empty());
    }

    #[test]
    fn newest_items_come_first() {
        let log = format!("{}\n{}", created("in-a", "notify"), created("in-b", "notify"));
        let items = fold_items(&log);
        assert_eq!(items[0].id, "in-b");
    }

    #[test]
    fn project_slugs_cannot_escape_the_inbox_directory() {
        assert_eq!(project_slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(project_slug(""), "global");
        assert_eq!(project_slug("xNAUT"), "xnaut");
    }

    #[test]
    fn secrets_never_reach_the_store() {
        let redacted = redact("ANTHROPIC_API_KEY=sk-live-abcdef\nall good");
        assert!(!redacted.contains("sk-live-abcdef"), "{redacted}");
        assert!(redacted.contains("all good"));
    }
}
