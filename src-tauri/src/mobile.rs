// ABOUTME: Mobile companion bridge (XNAUT-32) — axum server on a fixed, persisted port
// ABOUTME: serving the phone PWA, a session-list API, and a WebSocket PTY mirror.
//
// Security model: bound to 0.0.0.0 so the phone can reach it over the tailnet;
// transport encryption is Tailscale's WireGuard. Every /api and /ws request must
// carry the pairing token (?token=). GET / serves only the static app shell.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Per-session working context: cwd + git repo/branch. Refreshed by a
/// background sweep (never per-request — lsof under a lock was the source of
/// the keystroke-stall bug this bridge exposed).
#[derive(Clone, Default, Serialize)]
pub struct SessionContext {
    pub cwd: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
}

type CtxCache = std::sync::Arc<tokio::sync::Mutex<HashMap<String, SessionContext>>>;

/// The doctor must never wait for the machine-wide preflight scan it reports.
/// Project roots may live on disconnected mounts, and one slow `is_dir`/read
/// used to hold every doctor request past the control client's 30s deadline.
#[derive(Clone, Default)]
struct PreflightSnapshot {
    checks: Vec<crate::preflight::Check>,
    ready: bool,
    updated_at: Option<String>,
}

type PreflightCache = std::sync::Arc<tokio::sync::RwLock<PreflightSnapshot>>;

#[derive(Clone)]
struct Ctx {
    app: AppHandle,
    token: String,
    contexts: CtxCache,
    preflight: PreflightCache,
}

/// Generates a pairing token: "nxt_" + 16 random bytes, URL-safe base64.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("getrandom failed");
    format!("nxt_{}", URL_SAFE_NO_PAD.encode(bytes))
}

/// Bridge config lives in its OWN file (mobile.json), not settings.json:
/// other xNaut versions round-trip settings.json through their older Settings
/// struct and silently strip fields they don't know — which kept deleting the
/// pairing token while a release build was running.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct MobileConfig {
    pub enabled: bool,
    /// Fixed port — persisted, never randomized (the phone pairing depends on it).
    pub port: u16,
    /// Pairing token ("nxt_…"); generated on first boot, stable thereafter.
    pub token: String,
    /// Registered devices (1.22.2 item 1/4): each carries its own bridge
    /// token, so one phone can be revoked without re-pairing the others.
    #[serde(default)]
    pub devices: Vec<DeviceRecord>,
    /// ntfy topic for push (the swappable transport's current config; empty
    /// means push is a logged no-op). See push.rs for the seam.
    #[serde(default)]
    pub push_ntfy_topic: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceRecord {
    pub id: String,
    pub label: String,
    /// This device's own bridge token — accepted everywhere the pairing
    /// token is, revoked by deleting the device.
    pub bridge_token: String,
    /// APNs device token, collected now so the APNs transport has its
    /// audience the day the push key exists.
    #[serde(default)]
    pub apns_token: String,
    #[serde(default)]
    pub last_seen_ms: u64,
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 8931,
            token: String::new(),
            devices: Vec::new(),
            push_ntfy_topic: String::new(),
        }
    }
}

/// Persists the config with the same permissions first-run uses.
pub fn save_config(cfg: &MobileConfig) -> Result<(), String> {
    let path = mobile_config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn mobile_config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| std::path::PathBuf::from(".xnaut"))
        .join("mobile.json")
}

/// Loads mobile.json, generating + persisting the pairing token on first run.
pub fn load_or_init_config() -> MobileConfig {
    let path = mobile_config_path();
    let mut cfg: MobileConfig = std::fs::read_to_string(&path)
        .ok()
        .and_then(|body| serde_json::from_str(&body).ok())
        .unwrap_or_default();
    if cfg.token.is_empty() {
        cfg.token = generate_token();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(body) = serde_json::to_string_pretty(&cfg) {
            let _ = std::fs::write(&path, body);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    &path,
                    std::fs::Permissions::from_mode(0o600),
                );
            }
        }
    }
    cfg
}

/// Binds 0.0.0.0:<port> and serves the bridge. Returns the bound port.
pub async fn start_server(app: AppHandle, port: u16, token: String) -> Result<u16, String> {
    let contexts: CtxCache = Default::default();
    spawn_context_sweep(app.clone(), contexts.clone());
    let preflight: PreflightCache = Default::default();
    spawn_preflight_sweep(preflight.clone());
    let ctx = Ctx {
        app,
        token,
        contexts,
        preflight,
    };

    let router = Router::new()
        .route("/", get(index))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:session_id", axum::routing::delete(close_session))
        .route("/api/sessions/:session_id/files", get(list_files))
        .route("/api/sessions/:session_id/file", get(read_file))
        .route("/api/sessions/:session_id/git", get(git_overview))
        .route("/api/artifacts", get(list_artifacts))
        .route("/artifact/:token/*path", get(serve_artifact))
        .route("/api/observatory", get(observatory))
        .route(
            "/api/observatory/stop-all",
            axum::routing::post(observatory_stop_all),
        )
        .route(
            "/api/agents/:session_id/interrupt",
            axum::routing::post(interrupt_agent),
        )
        .route("/api/looms/:run_id/stop", axum::routing::post(stop_loom))
        .route("/api/agents/:handle/wake", axum::routing::post(wake_agent_route))
        .route("/api/control/doctor", get(control_doctor))
        .route("/api/control/eval", axum::routing::post(control_eval))
        .route(
            "/api/control/prune-sessions",
            axum::routing::post(control_prune_sessions),
        )
        .route("/api/manager", get(manager_state))
        .route("/api/manager/message", axum::routing::post(manager_message))
        .route("/api/manager/launch", axum::routing::post(manager_launch))
        .route("/api/automations", get(list_automations))
        .route(
            "/api/automations/:id/run",
            axum::routing::post(run_automation),
        )
        .route("/api/inbox", get(list_inbox))
        .route("/api/inbox/:id/decide", axum::routing::post(decide_inbox))
        .route("/api/inbox/:id/answer", axum::routing::post(answer_inbox))
        .route(
            "/api/devices",
            get(list_devices).post(register_device),
        )
        .route("/api/devices/:id", axum::routing::delete(remove_device))
        .route("/api/vaults", get(list_vaults))
        .route("/api/vault/:vault/search", get(vault_search_route))
        .route("/api/vault/:vault/note", get(vault_note_route))
        .route("/api/zellij", get(list_zellij))
        .route("/api/zellij/:name", axum::routing::delete(remove_zellij))
        .route("/api/zellij/:name/open", axum::routing::post(open_zellij))
        .route("/ws/:session_id", get(ws_attach))
        .layer(axum::middleware::from_fn(accept_bearer))
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .map_err(|e| format!("failed to bind mobile bridge on port {port}: {e}"))?;

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            eprintln!("[mobile] server crashed: {e}");
        }
    });

    Ok(port)
}

const PREFLIGHT_REFRESH: std::time::Duration = std::time::Duration::from_secs(60);

/// Run at most one preflight scan at a time and publish only complete results.
/// A stuck scan consumes one blocking worker, not one worker per doctor retry;
/// callers keep receiving the previous snapshot (or `ready: false` at boot).
fn spawn_preflight_sweep(cache: PreflightCache) {
    tokio::spawn(async move {
        loop {
            refresh_preflight(cache.clone(), crate::preflight::run).await;
            tokio::time::sleep(PREFLIGHT_REFRESH).await;
        }
    });
}

async fn refresh_preflight<F>(cache: PreflightCache, scan: F)
where
    F: FnOnce() -> Vec<crate::preflight::Check> + Send + 'static,
{
    let Ok(checks) = tokio::task::spawn_blocking(scan).await else {
        return;
    };
    *cache.write().await = PreflightSnapshot {
        checks,
        ready: true,
        updated_at: Some(chrono::Utc::now().to_rfc3339()),
    };
}

/// Every 10s: one combined lsof for all session PIDs, then git repo/branch per
/// unique cwd — all inside spawn_blocking, results cached for list_sessions.
fn spawn_context_sweep(app: AppHandle, cache: CtxCache) {
    tokio::spawn(async move {
        loop {
            let pids: Vec<(String, u32)> = {
                let Some(state) = app.try_state::<AppState>() else {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    continue;
                };
                let sessions = state.pty_sessions.lock().await;
                let mut out = Vec::with_capacity(sessions.len());
                for (id, s) in sessions.iter() {
                    if let Some(pid) = s.child.lock().await.process_id() {
                        out.push((id.clone(), pid));
                    }
                }
                out
            };

            let fresh = tokio::task::spawn_blocking(move || sweep_contexts(&pids))
                .await
                .unwrap_or_default();
            *cache.lock().await = fresh;

            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
}

fn sweep_contexts(pids: &[(String, u32)]) -> HashMap<String, SessionContext> {
    let mut out = HashMap::new();
    if pids.is_empty() {
        return out;
    }
    // One lsof for all PIDs: output is grouped as p<pid> … fcwd … n<path>.
    let list = pids
        .iter()
        .map(|(_, p)| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let Ok(lsof) = std::process::Command::new("lsof")
        .args(["-p", &list, "-Fn"])
        .output()
    else {
        return out;
    };
    let mut cwd_by_pid: HashMap<u32, String> = HashMap::new();
    let mut current_pid: Option<u32> = None;
    let mut in_cwd = false;
    for line in String::from_utf8_lossy(&lsof.stdout).lines() {
        if let Some(p) = line.strip_prefix('p') {
            current_pid = p.parse().ok();
            in_cwd = false;
        } else if line == "fcwd" {
            in_cwd = true;
        } else if let Some(path) = line.strip_prefix('n') {
            if in_cwd {
                if let Some(pid) = current_pid {
                    cwd_by_pid.insert(pid, path.to_string());
                }
                in_cwd = false;
            }
        } else if line.starts_with('f') {
            in_cwd = false;
        }
    }

    let git = |cwd: &str, args: &[&str]| -> Option<String> {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .ok()?;
        o.status.success().then(|| {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        })
    };

    let mut git_by_cwd: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for (id, pid) in pids {
        let cwd = cwd_by_pid.get(pid).cloned();
        let (repo, branch) = match &cwd {
            Some(c) => git_by_cwd
                .entry(c.clone())
                .or_insert_with(|| {
                    let top = git(c, &["rev-parse", "--show-toplevel"]);
                    let repo = top.as_ref().and_then(|t| {
                        std::path::Path::new(t)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                    });
                    let branch = git(c, &["rev-parse", "--abbrev-ref", "HEAD"]);
                    (repo, branch)
                })
                .clone(),
            None => (None, None),
        };
        out.insert(
            id.clone(),
            SessionContext {
                cwd,
                repo,
                branch,
            },
        );
    }
    out
}

fn token_ok(token: &str, q: &HashMap<String, String>) -> bool {
    let Some(presented) = q.get("token") else {
        return false;
    };
    // Constant-time, and an unset pairing token matches nothing: a bridge that
    // came up without one cannot answer "is this the right token", and an
    // unknown answer is not permission (XNAUT-350).
    if crate::agent_hooks::secret_matches(token, presented) {
        return true;
    }
    // Per-device tokens (1.22.2 item 4). Read on the miss path only; the
    // bridge sees phone-scale traffic, not server-scale. A hit stamps
    // last_seen so GET /api/devices can answer "when was this phone alive".
    let mut cfg = load_or_init_config();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut hit = false;
    for device in &mut cfg.devices {
        if crate::agent_hooks::secret_matches(&device.bridge_token, presented) {
            device.last_seen_ms = now;
            hit = true;
        }
    }
    if hit {
        let _ = save_config(&cfg);
    }
    hit
}

/// Folds `Authorization: Bearer <token>` into the query string the handlers
/// already read. The phone sends the header so a token that grants control of
/// this Mac never lands in a URL, and therefore never in a proxy or shell
/// history. Every existing handler keeps working unchanged.
async fn accept_bearer(mut req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let has_query_token = req
        .uri()
        .query()
        .is_some_and(|q| q.split('&').any(|kv| kv.starts_with("token=")));
    if !has_query_token {
        let bearer = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_string);
        if let Some(tok) = bearer {
            let base = req
                .uri()
                .path_and_query()
                .map(|pq| pq.as_str().to_string())
                .unwrap_or_else(|| "/".to_string());
            let sep = if req.uri().query().is_some() {
                '&'
            } else {
                '?'
            };
            let mut parts = req.uri().clone().into_parts();
            if let Ok(pq) = format!("{base}{sep}token={tok}").parse() {
                parts.path_and_query = Some(pq);
                if let Ok(uri) = axum::http::Uri::from_parts(parts) {
                    *req.uri_mut() = uri;
                }
            }
        }
    }
    next.run(req).await
}

fn authed(ctx: &Ctx, q: &HashMap<String, String>) -> bool {
    token_ok(&ctx.token, q)
}

/// The only route on this bridge that answers a caller with no token, and it
/// says so by name: `agent_hooks::ANONYMOUS_ROUTES` carries the reason. Take
/// the entry away and the shell stops being served, which is the point of
/// making the opt-in a written choice rather than a missing check.
async fn index() -> Response {
    if !crate::agent_hooks::anonymous_allowed("GET /") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Html(include_str!("../../src/mobile.html")).into_response()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionInfo {
    id: String,
    label: String,
    status: String,
    is_agent: bool,
    created_at_ms: i64,
    cwd: Option<String>,
    repo: Option<String>,
    branch: Option<String>,
    /// Backed by a zellij session, so it survives xNAUT quitting.
    durable: bool,
}

/// One zellij session as the phone sees it. `exited` means dead but
/// resurrectable: `zellij attach` rebuilds it from the serialized layout, so it
/// is still somewhere to go back to, not garbage.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DurableSession {
    pub name: String,
    pub created: String,
    pub last_active_ms: Option<u64>,
    pub exited: bool,
}

/// Live sessions first, then most-recently-active first inside each group.
/// A session with no `last_active_ms` sorts to the end of its group rather than
/// the front, so unknown never outranks known.
pub(crate) fn shape_durable(infos: Vec<crate::zellij::ZellijSessionInfo>) -> Vec<DurableSession> {
    let mut out: Vec<DurableSession> = infos
        .into_iter()
        .map(|i| DurableSession {
            name: i.name,
            created: i.created,
            last_active_ms: i.last_active_ms,
            exited: i.exited,
        })
        .collect();
    out.sort_by(|a, b| {
        a.exited
            .cmp(&b.exited)
            .then(
                b.last_active_ms
                    .unwrap_or(0)
                    .cmp(&a.last_active_ms.unwrap_or(0)),
            )
            .then(a.name.cmp(&b.name))
    });
    out
}

async fn list_sessions(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let state = ctx.app.state::<AppState>();
    let contexts = ctx.contexts.lock().await.clone();
    let agents = state.agent_sessions.lock().await;
    let sessions = state.pty_sessions.lock().await;

    let mut out: Vec<SessionInfo> = sessions
        .iter()
        .map(|(id, s)| {
            let created_at_ms = s
                .created_at
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let sc = contexts.get(id).cloned().unwrap_or_default();
            let (label, status, is_agent) = match agents.get(id) {
                Some(meta) => (
                    meta.label.clone(),
                    format!("{:?}", meta.status).to_lowercase(),
                    true,
                ),
                None => ("shell".to_string(), "idle".to_string(), false),
            };
            SessionInfo {
                id: id.clone(),
                label,
                status,
                is_agent,
                created_at_ms,
                cwd: sc.cwd,
                repo: sc.repo,
                branch: sc.branch,
                durable: s.session_name.is_some(),
            }
        })
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.created_at_ms));

    axum::Json(out).into_response()
}

async fn create_session(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let state = ctx.app.state::<AppState>();
    let config = crate::pty::PtyConfig::default();
    match crate::pty::create_pty_session(ctx.app.clone(), state, config).await {
        Ok(session_id) => {
            // Let the desktop adopt the session as a tab (app.js listener).
            let _ = ctx.app.emit(
                "mobile-session-created",
                serde_json::json!({ "sessionId": session_id }),
            );
            axum::Json(serde_json::json!({ "sessionId": session_id })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn close_session(
    State(ctx): State<Ctx>,
    Path(session_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let state = ctx.app.state::<AppState>();
    match crate::pty::close_pty(state, session_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}

/// Browse root for a session: its git toplevel when inside a repo, else cwd.
async fn session_base(ctx: &Ctx, session_id: &str) -> Option<std::path::PathBuf> {
    let sc = ctx.contexts.lock().await.get(session_id).cloned()?;
    let cwd = sc.cwd?;
    let base = tokio::task::spawn_blocking(move || {
        std::process::Command::new("git")
            .args(["-C", &cwd, "rev-parse", "--show-toplevel"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or(cwd)
    })
    .await
    .ok()?;
    Some(std::path::PathBuf::from(base))
}

/// Resolves `path` (absolute or base-relative) and confines it to `base`.
fn confined(base: &std::path::Path, path: &str) -> Option<std::path::PathBuf> {
    let candidate = if path.is_empty() {
        base.to_path_buf()
    } else if std::path::Path::new(path).is_absolute() {
        std::path::PathBuf::from(path)
    } else {
        base.join(path)
    };
    let real = candidate.canonicalize().ok()?;
    let base = base.canonicalize().ok()?;
    real.starts_with(&base).then_some(real)
}

async fn list_files(
    State(ctx): State<Ctx>,
    Path(session_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(base) = session_base(&ctx, &session_id).await else {
        return (StatusCode::NOT_FOUND, "no cwd for session yet").into_response();
    };
    let Some(dir) = confined(&base, q.get("path").map(String::as_str).unwrap_or("")) else {
        return (StatusCode::FORBIDDEN, "path outside session root").into_response();
    };
    let entries = tokio::task::spawn_blocking(move || {
        let mut out: Vec<serde_json::Value> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let is_dir = e.file_type().ok()?.is_dir();
                Some(serde_json::json!({
                    "name": name,
                    "isDir": is_dir,
                    "path": e.path().to_string_lossy(),
                }))
            })
            .collect();
        out.sort_by(|a, b| {
            let da = a["isDir"].as_bool().unwrap_or(false);
            let db = b["isDir"].as_bool().unwrap_or(false);
            db.cmp(&da).then_with(|| {
                a["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .cmp(&b["name"].as_str().unwrap_or("").to_lowercase())
            })
        });
        out
    })
    .await
    .unwrap_or_default();
    axum::Json(serde_json::json!({
        "base": base.to_string_lossy(),
        "entries": entries,
    }))
    .into_response()
}

const MAX_FILE_BYTES: u64 = 256 * 1024;

async fn read_file(
    State(ctx): State<Ctx>,
    Path(session_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(base) = session_base(&ctx, &session_id).await else {
        return (StatusCode::NOT_FOUND, "no cwd for session yet").into_response();
    };
    let Some(file) = confined(&base, q.get("path").map(String::as_str).unwrap_or("")) else {
        return (StatusCode::FORBIDDEN, "path outside session root").into_response();
    };
    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let meta = std::fs::metadata(&file).map_err(|e| e.to_string())?;
        if meta.len() > MAX_FILE_BYTES {
            return Err(format!("file too large ({} KB)", meta.len() / 1024));
        }
        let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
        String::from_utf8(bytes).map_err(|_| "binary file".to_string())
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match result {
        Ok(content) => axum::Json(serde_json::json!({ "content": content })).into_response(),
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e).into_response(),
    }
}

async fn git_overview(
    State(ctx): State<Ctx>,
    Path(session_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(base) = session_base(&ctx, &session_id).await else {
        return (StatusCode::NOT_FOUND, "no cwd for session yet").into_response();
    };
    let overview = tokio::task::spawn_blocking(move || {
        let run = |args: &[&str]| -> Option<String> {
            let o = std::process::Command::new("git")
                .arg("-C")
                .arg(&base)
                .args(args)
                .output()
                .ok()?;
            o.status
                .success()
                .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
        };
        let branch = run(&["rev-parse", "--abbrev-ref", "HEAD"])
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let changes: Vec<serde_json::Value> = run(&["status", "--porcelain"])
            .unwrap_or_default()
            .lines()
            .filter(|l| l.len() > 3)
            .map(|l| {
                serde_json::json!({
                    "status": l[..2].trim(),
                    "path": l[3..].trim(),
                })
            })
            .collect();
        let commits: Vec<serde_json::Value> = run(&["log", "-20", "--pretty=%h%x09%s"])
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let (hash, subject) = l.split_once('\t')?;
                Some(serde_json::json!({ "hash": hash, "subject": subject }))
            })
            .collect();
        serde_json::json!({ "branch": branch, "changes": changes, "commits": commits })
    })
    .await
    .unwrap_or_else(|_| serde_json::json!({}));
    axum::Json(overview).into_response()
}

/// Agents drop share-with-the-phone output here (HTML reports, prototypes,
/// images). Served raw over the tailnet — no claude.ai login involved.
fn artifacts_root() -> std::path::PathBuf {
    let root = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".xnaut-vault/artifacts");
    let _ = std::fs::create_dir_all(&root);
    root
}

async fn list_artifacts(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut items = tokio::task::spawn_blocking(|| {
        let root = artifacts_root();
        let mut out: Vec<serde_json::Value> = Vec::new();
        // Recursive walk, capped — artifacts are a hand-curated folder, not a repo.
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                if out.len() >= 200 {
                    return out;
                }
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let (Ok(meta), Ok(rel)) = (e.metadata(), path.strip_prefix(&root)) {
                    let modified_ms = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    out.push(serde_json::json!({
                        "path": rel.to_string_lossy(),
                        "name": e.file_name().to_string_lossy(),
                        "modifiedAtMs": modified_ms,
                        "size": meta.len(),
                    }));
                }
            }
        }
        out
    })
    .await
    .unwrap_or_default();
    items.sort_by_key(|v| std::cmp::Reverse(v["modifiedAtMs"].as_i64().unwrap_or(0)));
    axum::Json(items).into_response()
}

fn mime_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "md" | "txt" | "log" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Token travels as a path segment (not a query param) so relative asset
/// references inside served HTML resolve under the same authorized prefix.
async fn serve_artifact(
    State(ctx): State<Ctx>,
    Path((token, path)): Path<(String, String)>,
) -> Response {
    if !crate::agent_hooks::secret_matches(&ctx.token, &token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let root = artifacts_root();
    let Some(file) = confined(&root, &path) else {
        return (StatusCode::FORBIDDEN, "path outside artifacts root").into_response();
    };
    match tokio::task::spawn_blocking(move || std::fs::read(&file).map(|b| (b, file))).await {
        Ok(Ok((bytes, file))) => (
            [(axum::http::header::CONTENT_TYPE, mime_for(&file))],
            bytes,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

// ─── Observatory + Multi-Agent Manager (phone remote control) ────────────────

/// Live sandbox loom runs (status "started" with a living driver pid).
fn live_loom_rows() -> Vec<serde_json::Value> {
    let runs = crate::nautloom::loom_runs_list(Some(200)).unwrap_or_default();
    runs.into_iter()
        .filter(|r| r.status == "started" && r.pid != 0 && crate::nautloom::loom_run_alive(r.pid))
        .map(|r| {
            serde_json::json!({
                "kind": "sandbox",
                "id": r.id,
                "pid": r.pid,
                "title": format!("{}{}", r.weave, r.goal.lines().next().map(|g| format!(" · {}", &g[..g.len().min(60)])).unwrap_or_default()),
                "sub": "sandbox run",
                "model": if r.model.is_empty() { "—".to_string() } else { r.model.clone() },
                "startedMs": r.started_ms,
                "status": "working",
            })
        })
        .collect()
}

async fn observatory(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let state = ctx.app.state::<AppState>();

    let mut agents: Vec<serde_json::Value> = Vec::new();
    if let Ok(sessions) = crate::status::agent_sessions_list(state).await {
        for s in sessions {
            let status = format!("{:?}", s.status).to_lowercase();
            if status == "done" {
                continue;
            }
            agents.push(serde_json::json!({
                "kind": "terminal",
                "id": s.session_id,
                "title": format!("{} · {}", s.agent_id, s.label),
                "sub": "Interactive terminal session",
                "model": s.agent_id,
                "startedMs": s.started_at_ms,
                "status": status,
            }));
        }
    }
    agents.extend(tokio::task::spawn_blocking(live_loom_rows).await.unwrap_or_default());
    agents.sort_by_key(|a| std::cmp::Reverse(a["startedMs"].as_i64().unwrap_or(0)));

    // Usage APIs are external + rate-limited; failures return null, phone keeps last.
    let max = crate::usage::max_usage(None).await.ok();
    let codex = tokio::task::spawn_blocking(crate::usage::codex_usage)
        .await
        .ok()
        .and_then(Result::ok);
    let manager = ctx.app.state::<AppState>().mobile_manager.lock().await.clone();

    axum::Json(serde_json::json!({
        "usage": { "max": max, "codex": codex },
        "agents": agents,
        "manager": manager,
    }))
    .into_response()
}

async fn interrupt_agent(
    State(ctx): State<Ctx>,
    Path(session_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let state = ctx.app.state::<AppState>();
    match crate::status::agent_session_interrupt(ctx.app.clone(), state, session_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::CONFLICT, e).into_response(),
    }
}

async fn stop_loom(
    State(ctx): State<Ctx>,
    Path(run_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let pid: u32 = q.get("pid").and_then(|p| p.parse().ok()).unwrap_or(0);
    let done = tokio::task::spawn_blocking(move || {
        if pid != 0 {
            let _ = crate::nautloom::loom_run_stop(pid);
        }
        crate::nautloom::loom_run_mark(run_id, "cancelled".into())
    })
    .await;
    match done {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        _ => (StatusCode::CONFLICT, "could not stop run").into_response(),
    }
}

async fn observatory_stop_all(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Swarm queue lives in the desktop pane — it stops itself on this event.
    let _ = ctx.app.emit("mobile-swarm-stopall", serde_json::json!({}));
    // Terminal agents: interrupt server-side.
    let state = ctx.app.state::<AppState>();
    if let Ok(sessions) = crate::status::agent_sessions_list(state).await {
        for s in sessions {
            let st = ctx.app.state::<AppState>();
            let _ = crate::status::agent_session_interrupt(ctx.app.clone(), st, s.session_id)
                .await;
        }
    }
    // Live sandbox runs: kill + mark.
    let _ = tokio::task::spawn_blocking(|| {
        for row in live_loom_rows() {
            if let (Some(pid), Some(id)) = (row["pid"].as_u64(), row["id"].as_str()) {
                let _ = crate::nautloom::loom_run_stop(pid as u32);
                let _ = crate::nautloom::loom_run_mark(id.to_string(), "cancelled".into());
            }
        }
    })
    .await;
    StatusCode::NO_CONTENT.into_response()
}

async fn manager_state(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let v = ctx.app.state::<AppState>().mobile_manager.lock().await.clone();
    axum::Json(v).into_response()
}

async fn manager_message(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
    body: String,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let text = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["text"].as_str().map(String::from))
        .unwrap_or(body);
    if text.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "empty message").into_response();
    }
    let _ = ctx
        .app
        .emit("mobile-manager-message", serde_json::json!({ "text": text }));
    StatusCode::ACCEPTED.into_response()
}

async fn manager_launch(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let _ = ctx.app.emit("mobile-manager-launch", serde_json::json!({}));
    StatusCode::ACCEPTED.into_response()
}

/// Wake an agent from outside the app: the same backend nudge NautBot's tool
/// uses, no frontend and no LLM in the path. This is what lets a test rig
/// (XNAUT-255) run the wake/quit/adopt cycle headlessly — and it is the first
/// bridge route that can START work, so it takes the same token as the rest.
/// Body: optional plain-text wake message.
async fn wake_agent_route(
    State(ctx): State<Ctx>,
    Path(handle): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    body: String,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let message = wake_message(&body);
    match crate::nudge::nudge_agent(&ctx.app, &handle, &message).await {
        Ok(value) => axum::Json(value).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error).into_response(),
    }
}

/// The prompt an agent should read, from whatever a caller posted.
///
/// The route takes a raw body so `curl --data 'go check your tickets'` works,
/// but every JSON client posts `{"text": "..."}` — and that envelope was handed
/// to the agent verbatim. The rig's round 11 found its own task section reading
/// `{"text": "R11 COLD LAUNCH PROBE. Do exactly this..."}`, and the same
/// wrapper copied into the ledger detail. Agents coped; they should not have to.
///
/// Unwrap `text` or `message` when the body is a JSON object carrying one, and
/// otherwise pass the body through untouched — a plain sentence that happens to
/// start with a brace is still a sentence.
fn wake_message(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "Check your tickets.".to_string();
    }
    serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .as_ref()
        .and_then(|value| value.get("text").or_else(|| value.get("message")))
        .and_then(|text| text.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| trimmed.to_string())
}

// ── The control surface (XNAUT-265) ──────────────────────────────────────────
//
// "Build the lever": give an agent a command, not a scripting problem. Every
// rig round so far re-invented its own harness out of cliclick, screencapture
// and hand-rolled accessibility dumps, which costs tokens and makes each round
// unreproducible.
//
// xNAUT cannot use the Chrome DevTools Protocol — Tauri renders in WKWebView on
// macOS, which speaks Safari's inspector protocol instead — so the lever is
// this bridge, which is already in-process, already token-gated, and already
// how the fleet is woken from outside.

#[derive(Serialize)]
struct ControlDoctor {
    ok: bool,
    version: String,
    window_visible: bool,
    agent_sessions: usize,
    zellij_sessions: usize,
    verify_records: usize,
    read_only: bool,
    pm_enabled: bool,
    /// The board clock (sweep.rs, every 180s), UTC RFC3339 with the Z spelled
    /// out. `null` means it has never ticked in this app's life, which is a
    /// different fact from a tick that found nothing to do; every field above
    /// stays healthy either way, which is what made it worth adding.
    last_sweep_at: Option<String>,
    /// How long ago, so nobody has to compare a UTC stamp to a local clock.
    last_sweep_age_secs: Option<i64>,
    /// Completed sweeps since start. Tells "woke once" from "running steadily".
    sweep_ticks: u64,
    /// The status clock (status.rs, every 750ms). If it dies, every agent dot
    /// freezes at its last value and the fleet looks calm.
    last_status_tick_at: Option<String>,
    last_status_tick_age_secs: Option<i64>,
    /// Preconditions, each with the fix. Every field above says what is true;
    /// these say what to do about it. That is the difference between reporting
    /// `pm_enabled: false` and saying which setting to go and change.
    preflight: Vec<crate::preflight::Check>,
    /// False only between bridge startup and the first completed background
    /// scan. The timestamp makes a slow refresh visible without making doctor
    /// wait for it.
    preflight_ready: bool,
    preflight_updated_at: Option<String>,
    /// What the fleet did in the last 24 hours, as numbers (XNAUT-315):
    /// machine merges an hour, agent against hand commits, and what the
    /// escalations were worth. `null` when no project's repository could be
    /// read, which is not the same as zero.
    throughput: Option<crate::throughput::Lanes>,
}

/// One loop's clock, as doctor reports it.
///
/// Split out of the handler so the distinction that matters is testable without
/// an AppHandle: never ticked is `(None, None, 0)`, a tick that found nothing to
/// do is a real timestamp. Those two looking identical is the bug this fixes.
fn clock_fields(hb: &crate::heartbeat::Heartbeat) -> (Option<String>, Option<i64>, u64) {
    match hb.read() {
        Some(beat) => (Some(beat.at), Some(beat.age_secs), beat.ticks),
        None => (None, None, 0),
    }
}

/// Drop the EXITED `xnaut-*` sessions, on demand and from another machine.
///
/// The rig's between-cycles sweep (XNAUT-255). The scheduler already does this
/// hourly with a day-old threshold, which is right for a machine somebody uses
/// and wrong for one that runs a cycle every ten minutes: there, the sessions
/// worth removing are the ones from the cycle that just ended. `?hours=0` says
/// so explicitly, and the rules that protect a live session and the owner's own
/// panes hold at every threshold — see `zellij::prunable_exited`.
async fn control_prune_sessions(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let older_than_ms = match q.get("hours").map(|h| h.parse::<u64>()) {
        Some(Ok(hours)) => hours.saturating_mul(3_600_000),
        Some(Err(_)) => {
            return (StatusCode::BAD_REQUEST, "hours must be a whole number").into_response()
        }
        None => crate::zellij::PRUNE_EXITED_AFTER_MS,
    };
    match tokio::task::spawn_blocking(move || crate::zellij::prune_exited_sessions(older_than_ms))
        .await
    {
        Ok(report) => axum::Json(report).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("prune task panicked: {e}"),
        )
            .into_response(),
    }
}

/// One call that answers "is this app healthy and what is it holding right
/// now" — the first thing any verification round needs, and previously six
/// separate probes.
async fn control_doctor(State(ctx): State<Ctx>, Query(q): Query<HashMap<String, String>>) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let window_visible = tauri::Manager::get_webview_window(&ctx.app, "main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    let agent_sessions = {
        let state = tauri::Manager::state::<crate::state::AppState>(&ctx.app);
        let map = state.agent_sessions.lock().await;
        map.len()
    };
    let zellij_sessions = tokio::task::spawn_blocking(crate::zellij::live_sessions)
        .await
        .unwrap_or_default()
        .len();
    let records = crate::sandbox_verify::sandbox_verify_records().await.unwrap_or_default();
    let verify_records = records.len();
    let switches = crate::switches::load();
    let throughput = {
        // Through the same resolver every fleet reader uses, not the
        // settings-backed one: on tron the settings never named the control
        // repo, repo_now() found it anyway, and the doctor said `null` beside
        // pm_enabled: true (2026-09-10).
        let roots: Vec<std::path::PathBuf> = crate::project_management::repo_now()
            .and_then(|repo| crate::project_management::list_projects(&repo))
            .unwrap_or_default()
            .iter()
            .map(|p| std::path::PathBuf::from(crate::project_management::local_source_path(p).trim()))
            .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
            .collect();
        let branch = crate::jury_runtime::policy_integration_branch();
        // Both lanes, side by side (XNAUT-319): the swarm lane's numbers are
        // only evidence next to the audited lane's.
        let verifies = crate::throughput::verifies_in(&records, 24, chrono::Utc::now().timestamp());
        tokio::task::spawn_blocking(move || {
            let registry = crate::agents::registry_dir().ok()?;
            let swarm = crate::project_management::repo_now()
                .map(|repo| crate::swarm::lane_tickets(&repo))
                .unwrap_or_default();
            crate::throughput::collect_lanes(&roots, &registry, &swarm, &verifies, &branch, 24)
        })
        .await
        .unwrap_or(None)
    };
    let (last_sweep_at, last_sweep_age_secs, sweep_ticks) =
        clock_fields(&crate::heartbeat::SWEEP);
    let (last_status_tick_at, last_status_tick_age_secs, _) =
        clock_fields(&crate::heartbeat::STATUS_DECAY);
    let preflight = ctx.preflight.read().await.clone();
    axum::Json(ControlDoctor {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        window_visible,
        agent_sessions,
        zellij_sessions,
        verify_records,
        read_only: switches.read_only,
        pm_enabled: crate::project_management::repo_now().is_ok(),
        last_sweep_at,
        last_sweep_age_secs,
        sweep_ticks,
        last_status_tick_at,
        last_status_tick_age_secs,
        preflight: preflight.checks,
        preflight_ready: preflight.ready,
        preflight_updated_at: preflight.updated_at,
        throughput,
    })
    .into_response()
}

/// Run one expression in the app's own webview and return what it evaluated to.
///
/// This is the AX tree's counterpart: the accessibility dump says what is on
/// screen, this says what the app believes. The rig needed exactly this to
/// prove XNAUT-259 — that `window.xnautActiveProjectPath()` returned the right
/// path while the dialog showed none — and had to open the inspector by hand
/// to get it.
///
/// The result comes back through the same debug-log channel the app already
/// mirrors console output into, so no new plumbing and no eval-to-string
/// smuggling: the caller polls the log. Body is the expression.
async fn control_eval(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
    body: String,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let expression = body.trim().to_string();
    if expression.is_empty() {
        return (StatusCode::BAD_REQUEST, "an expression is required").into_response();
    }
    let Some(window) = tauri::Manager::get_webview_window(&ctx.app, "main") else {
        return (StatusCode::SERVICE_UNAVAILABLE, "no main window").into_response();
    };
    // A marker so the caller can find its own answer in the log, and a
    // try/catch so a thrown expression reports rather than vanishing.
    let marker = format!("ctl-{}", uuid::Uuid::new_v4().simple());
    let script = format!(
        "(function(){{try{{const v=({expression});console.log('{marker}',typeof v==='string'?v:JSON.stringify(v));}}catch(e){{console.log('{marker}','ERR '+String(e));}}}})()"
    );
    if let Err(error) = window.eval(&script) {
        return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
    }
    axum::Json(serde_json::json!({ "ok": true, "marker": marker }))
        .into_response()
}

/// Desktop pane → bridge: publish Manager thread + swarm state for the phone.
#[tauri::command]
pub async fn mobile_manager_publish(
    state: tauri::State<'_, AppState>,
    value: serde_json::Value,
) -> Result<(), String> {
    *state.mobile_manager.lock().await = value;
    Ok(())
}

/// Task manager: the desktop's automations, listable and fire-able from the
/// phone. Reuses the scheduler's own command fns (precheck runs server-side).
async fn list_automations(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match crate::scheduler::automation_list() {
        Ok(autos) => axum::Json(autos).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn run_automation(
    State(ctx): State<Ctx>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match crate::scheduler::automation_fire_now(ctx.app.clone(), id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::CONFLICT, e).into_response(),
    }
}

// ─── Devices + push registry (1.22.2 items 1 and 4) ─────────────────────────

#[derive(serde::Deserialize)]
struct RegisterDevice {
    /// APNs device token; optional because ntfy-era phones have none.
    #[serde(default)]
    token: String,
    label: String,
}

async fn register_device(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
    axum::Json(req): axum::Json<RegisterDevice>,
) -> Response {
    if !authed(&ctx, &q) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let label = req.label.trim().to_string();
    if label.is_empty() {
        return (StatusCode::BAD_REQUEST, "label is required").into_response();
    }
    let mut cfg = load_or_init_config();
    let device = DeviceRecord {
        id: uuid::Uuid::new_v4().to_string(),
        label,
        bridge_token: generate_token(),
        apns_token: req.token.trim().to_string(),
        last_seen_ms: now_ms(),
    };
    cfg.devices.push(device.clone());
    if let Err(error) = save_config(&cfg) {
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }
    // The bridge token is returned ONCE, here. GET /api/devices never
    // repeats it, same rule as every pairing secret.
    axum::Json(serde_json::json!({
        "id": device.id,
        "label": device.label,
        "bridgeToken": device.bridge_token,
    }))
    .into_response()
}

async fn list_devices(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let cfg = load_or_init_config();
    let rows: Vec<_> = cfg
        .devices
        .iter()
        .map(|d| {
            serde_json::json!({ "id": d.id, "label": d.label, "lastSeenMs": d.last_seen_ms })
        })
        .collect();
    axum::Json(rows).into_response()
}

async fn remove_device(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
    Path(id): Path<String>,
) -> Response {
    if !authed(&ctx, &q) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let mut cfg = load_or_init_config();
    let before = cfg.devices.len();
    cfg.devices.retain(|d| d.id != id);
    if cfg.devices.len() == before {
        return (StatusCode::NOT_FOUND, "no such device").into_response();
    }
    if let Err(error) = save_config(&cfg) {
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ─── Vault, read-only (1.22.2 item 2) ────────────────────────────────────────
//
// Read-only ON PURPOSE, the iOS side's own words: "Editing a note on a phone
// is a worse experience than not editing it, and every write route is another
// thing to secure." Search and read is the slice that is useful away from the
// desk. Writing, if ever, is its own decision.

/// The phone cannot "open" a vault first the way the desktop UI does, so
/// these routes open on demand through the same vault_open the desktop uses
/// (index + watcher), once, then serve from the index.
fn ensure_vault_open(app: &AppHandle, vault: &str) -> Result<(), String> {
    let mgr = app.state::<crate::vault::VaultManager>();
    if mgr.indexes.lock().unwrap().contains_key(vault) {
        return Ok(());
    }
    crate::vault::vault_open(app.clone(), app.state(), vault.to_string()).map(|_| ())
}

async fn list_vaults(State(ctx): State<Ctx>, Query(q): Query<HashMap<String, String>>) -> Response {
    if !authed(&ctx, &q) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let mut rows = Vec::new();
    for vault in ["work", "personal"] {
        if ensure_vault_open(&ctx.app, vault).is_err() {
            continue; // a vault that cannot open is simply not listed
        }
        let mgr = ctx.app.state::<crate::vault::VaultManager>();
        let count = mgr
            .indexes
            .lock()
            .unwrap()
            .get(vault)
            .map(|idx| idx.notes.len())
            .unwrap_or(0);
        rows.push(serde_json::json!({ "name": vault, "noteCount": count }));
    }
    axum::Json(rows).into_response()
}

async fn vault_search_route(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
    Path(vault): Path<String>,
) -> Response {
    if !authed(&ctx, &q) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let query = q.get("q").cloned().unwrap_or_default();
    if query.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "q is required").into_response();
    }
    if let Err(error) = ensure_vault_open(&ctx.app, &vault) {
        return (StatusCode::BAD_REQUEST, error).into_response();
    }
    match crate::vault::vault_search(ctx.app.state(), vault, query) {
        Ok(hits) => axum::Json(hits).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error).into_response(),
    }
}

async fn vault_note_route(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
    Path(vault): Path<String>,
) -> Response {
    if !authed(&ctx, &q) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let Some(rel) = q.get("rel").filter(|r| !r.trim().is_empty()).cloned() else {
        return (StatusCode::BAD_REQUEST, "rel is required").into_response();
    };
    if let Err(error) = ensure_vault_open(&ctx.app, &vault) {
        return (StatusCode::BAD_REQUEST, error).into_response();
    }
    let title = {
        let mgr = ctx.app.state::<crate::vault::VaultManager>();
        let indexes = mgr.indexes.lock().unwrap();
        indexes
            .get(&vault)
            .and_then(|idx| idx.notes.get(&rel))
            .map(|meta| meta.title.clone())
    };
    match crate::vault::vault_note_read(ctx.app.state(), vault, rel.clone()) {
        Ok(markdown) => axum::Json(serde_json::json!({
            "rel": rel,
            "title": title.unwrap_or_else(|| rel.clone()),
            "markdown": markdown,
        }))
        .into_response(),
        Err(error) => (StatusCode::NOT_FOUND, error).into_response(),
    }
}


/// Zellij sessions on this Mac, including ones xNAUT never started.
/// `zellij_sessions_info` shells out, so it goes on the blocking pool.
async fn list_zellij(State(ctx): State<Ctx>, Query(q): Query<HashMap<String, String>>) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match tokio::task::spawn_blocking(crate::zellij::zellij_sessions_info).await {
        Ok(infos) => axum::Json(shape_durable(infos)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("zellij list panicked: {e}"),
        )
            .into_response(),
    }
}

/// Remove a zellij session: kill it, then drop the resurrectable record, or
/// the row comes straight back on the next list.
async fn remove_zellij(
    State(ctx): State<Ctx>,
    Path(name): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Validate here so a bad name is a client error and a zellij failure is not.
    let name = match crate::zellij::validate_session_name(&name) {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    match tokio::task::spawn_blocking(move || crate::zellij::remove_session(&name)).await {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("zellij remove panicked: {e}"),
        )
            .into_response(),
    }
}

/// Attach an existing zellij session as a desktop tab. No cwd or command: the
/// session already exists, so launch_command resolves to `zellij attach`.
async fn open_zellij(
    State(ctx): State<Ctx>,
    Path(name): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let name = match crate::zellij::validate_session_name(&name) {
        Ok(n) => n,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    let state = ctx.app.state::<AppState>();
    // Born at the caller's grid. Zellij sizes a session to its smallest
    // attached client, so creating at 80x24 and resizing after the fact makes
    // the session visibly jump twice; the phone knows its own grid, so it says
    // so up front.
    let parse = |k: &str| {
        q.get(k)
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|n| *n > 0)
    };
    let mut config = crate::pty::PtyConfig {
        session_name: Some(name),
        ..Default::default()
    };
    if let (Some(cols), Some(rows)) = (parse("cols"), parse("rows")) {
        config.cols = cols;
        config.rows = rows;
    }
    match crate::pty::create_pty_session(ctx.app.clone(), state, config).await {
        Ok(session_id) => {
            // Deliberately NOT emitting mobile-session-created. The session is
            // already running on the Mac; the phone wants to talk to it, not to
            // make a window appear on a screen nobody is looking at. This PTY
            // exists only to host `zellij attach` so /ws/{id} has something to
            // mirror, and the phone deletes it on the way out, which detaches
            // from zellij without killing the session.
            //
            // Pass ?surface=1 to also adopt it as a desktop tab.
            if q.get("surface").is_some_and(|v| v == "1") {
                let _ = ctx.app.emit(
                    "mobile-session-created",
                    serde_json::json!({ "sessionId": session_id }),
                );
            }
            axum::Json(serde_json::json!({ "sessionId": session_id })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Open inbox items: the agents that are blocked waiting on a human. The
/// hook server parks their request, so answering one here unblocks the agent
/// on the Mac immediately.
async fn list_inbox(
    State(ctx): State<Ctx>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match tokio::task::spawn_blocking(|| crate::inbox::inbox_list(None, Some("open".into()))).await
    {
        Ok(Ok(items)) => axum::Json(items).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("inbox list panicked: {e}"),
        )
            .into_response(),
    }
}

/// Approve or deny a parked request. `decision` is approved|denied; anything
/// else is refused by inbox_decide itself.
async fn decide_inbox(
    State(ctx): State<Ctx>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(decision) = q.get("decision").cloned() else {
        return (StatusCode::BAD_REQUEST, "decision is required").into_response();
    };
    match crate::inbox::inbox_decide(ctx.app.clone(), id, decision) {
        Ok(item) => axum::Json(item).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// Answer an `ask`, which carries free text rather than a yes or no.
async fn answer_inbox(
    State(ctx): State<Ctx>,
    Path(id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    body: String,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match crate::inbox::inbox_answer(ctx.app.clone(), id, body) {
        Ok(item) => axum::Json(item).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn ws_attach(
    State(ctx): State<Ctx>,
    Path(session_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    if !authed(&ctx, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.on_upgrade(move |socket| handle_socket(ctx, session_id, socket))
}

/// What an attaching phone gets: replay ring, live subscription and the desktop
/// dims, snapshotted under one lock so no chunk is lost between them.
///
/// `None` means the session has no tap, and the only thing the bridge can do is
/// close the socket. Every session gets one at creation (see pty::register_session).
pub(crate) async fn attach_tap(
    state: &AppState,
    session_id: &str,
) -> Option<(Vec<u8>, tokio::sync::broadcast::Receiver<Vec<u8>>, u16, u16)> {
    let taps = state.mobile_taps.lock().await;
    let tap = taps.get(session_id)?;
    Some((tap.ring.clone(), tap.tx.subscribe(), tap.cols, tap.rows))
}

async fn handle_socket(ctx: Ctx, session_id: String, mut socket: WebSocket) {
    let state = ctx.app.state::<AppState>();

    let Some((replay, mut rx, cols, rows)) = attach_tap(&state, &session_id).await else {
        let _ = socket.send(Message::Close(None)).await;
        return;
    };

    // Desktop dims first — the phone sizes its terminal to these and fit-zooms.
    let meta = serde_json::json!({ "type": "meta", "cols": cols, "rows": rows }).to_string();
    if socket.send(Message::Text(meta)).await.is_err() {
        return;
    }

    if !replay.is_empty() && socket.send(Message::Binary(replay)).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            chunk = rx.recv() => match chunk {
                Ok(data) => {
                    if socket.send(Message::Binary(data)).await.is_err() {
                        break;
                    }
                }
                // Lagged: a slow phone missed chunks — keep streaming from here.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                // Closed: session ended, tap dropped.
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if v["type"] == "input" {
                            if let Some(data) = v["data"].as_str() {
                                write_input(&state, &session_id, data.as_bytes()).await;
                            }
                        } else if v["type"] == "resize" {
                            // Phone-fit mode: the phone takes over the PTY size
                            // while attached (desktop reflows; it re-asserts its
                            // dims on its own next resize).
                            if let (Some(c), Some(r)) = (v["cols"].as_u64(), v["rows"].as_u64())
                            {
                                resize_session(&state, &session_id, c as u16, r as u16).await;
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
        }
    }
}

/// Phone-driven resize. The dims arrive off the wire, so they get a range check
/// the desktop's own resize does not need; the resize itself is pty's.
async fn resize_session(state: &AppState, session_id: &str, cols: u16, rows: u16) {
    if cols < 10 || rows < 5 || cols > 500 || rows > 200 {
        return; // garbage guard
    }
    let _ = crate::pty::resize_session(state, session_id, cols, rows).await;
}

/// Writes keystrokes to the PTY. Same mechanics as pty::write_to_pty, reachable
/// without a tauri::State command context.
async fn write_input(state: &AppState, session_id: &str, data: &[u8]) {
    let sessions = state.pty_sessions.lock().await;
    if let Some(session) = sessions.get(session_id) {
        if let Ok(mut writer) = session.writer.lock() {
            let _ = writer.write_all(data);
            let _ = writer.flush();
        }
    }
}

// ─── Tauri command: pairing info for the Settings → Mobile panel ─────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileInfo {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    pub url: String,
    pub qr_svg: String,
}

/// Best-guess reachable host: Tailscale IP if the CLI answers, else LAN IP,
/// else localhost. ponytail: shell-out beats a new interface-enumeration dep.
fn guess_host() -> String {
    let candidates = [
        ("tailscale", vec!["ip", "-4"]),
        (
            "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
            vec!["ip", "-4"],
        ),
        ("ipconfig", vec!["getifaddr", "en0"]),
    ];
    for (cmd, args) in candidates {
        if let Ok(out) = std::process::Command::new(cmd).args(&args).output() {
            if out.status.success() {
                // Must parse as an address, not merely be non-empty. The macOS
                // Tailscale.app CLI shim prints "The Tailscale CLI failed to
                // start: ..." to stdout and still exits 0, so an exit-status
                // check alone put that sentence in the host slot of the URL and
                // the QR: "http://The Tailscale CLI failed to start: ...:8931/".
                if let Some(ip) = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .map(str::trim)
                    .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
                {
                    return ip.to_string();
                }
            }
        }
    }
    "localhost".to_string()
}

#[tauri::command]
pub async fn mobile_info() -> Result<MobileInfo, String> {
    let mobile = load_or_init_config();
    let host = guess_host();
    // Token travels in the fragment so it never appears in server logs.
    let url = format!("http://{}:{}/#{}", host, mobile.port, mobile.token);
    let qr_svg = qrcode::QrCode::new(url.as_bytes())
        .map(|code| {
            code.render()
                .min_dimensions(220, 220)
                .dark_color(qrcode::render::svg::Color("#EDEDED"))
                .light_color(qrcode::render::svg::Color("#161616"))
                .build()
        })
        .unwrap_or_default();
    Ok(MobileInfo {
        enabled: mobile.enabled,
        port: mobile.port,
        token: mobile.token,
        url,
        qr_svg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{MobileTap, MOBILE_RING_CAP};

    #[test]
    fn a_wake_reads_a_sentence_not_an_envelope() {
        // What every JSON client posts, and what the rig's agent actually read
        // in its own task section before this existed.
        assert_eq!(wake_message(r#"{"text":"go check XNAUT-263"}"#), "go check XNAUT-263");
        assert_eq!(wake_message(r#"{"message":"same thing"}"#), "same thing");
        // A raw body stays a raw body; curl --data 'sentence' must keep working.
        assert_eq!(wake_message("go check XNAUT-263"), "go check XNAUT-263");
        // Prose is not an envelope just because it looks like one.
        assert_eq!(wake_message("{not json at all"), "{not json at all");
        assert_eq!(wake_message(r#"{"other":"key"}"#), r#"{"other":"key"}"#);
        assert_eq!(wake_message("   "), "Check your tickets.");
    }

    #[test]
    fn token_has_prefix_and_entropy() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert!(t1.starts_with("nxt_"));
        assert_ne!(t1, t2);
        assert!(t1.len() > 20);
    }

    #[test]
    fn tap_ring_caps_and_keeps_tail() {
        let mut tap = MobileTap::new(80, 24);
        tap.push(&vec![b'a'; MOBILE_RING_CAP]);
        tap.push(b"tail");
        assert_eq!(tap.ring.len(), MOBILE_RING_CAP);
        assert!(tap.ring.ends_with(b"tail"));
    }

    #[test]
    fn tap_broadcasts_to_subscriber() {
        let mut tap = MobileTap::new(80, 24);
        let mut rx = tap.tx.subscribe();
        tap.push(b"hello");
        assert_eq!(rx.try_recv().unwrap(), b"hello".to_vec());
    }

    #[test]
    fn token_ok_rejects_missing_wrong_and_empty() {
        let q = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        assert!(token_ok("nxt_abc", &q(&[("token", "nxt_abc")])));
        assert!(!token_ok("nxt_abc", &q(&[("token", "wrong")])));
        assert!(!token_ok("nxt_abc", &q(&[])));
        assert!(!token_ok("", &q(&[("token", "")])));
    }

    /// XNAUT-350. A bridge that came up without a pairing token cannot answer
    /// "is this the right token", and an unknown answer is not permission.
    #[test]
    fn a_bridge_with_no_token_admits_nobody() {
        let q = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        assert!(!token_ok("", &q(&[("token", "")])));
        assert!(!token_ok("", &q(&[("token", "nxt_anything")])));
    }

    /// The app shell is the one route here that answers an unauthenticated
    /// caller, and it does so because it is written down by name.
    #[test]
    fn the_shell_is_the_only_anonymous_route_on_this_bridge() {
        assert!(crate::agent_hooks::anonymous_allowed("GET /"));
        for route in ["GET /api/sessions", "GET /api/inbox", "GET /ws/:session_id"] {
            assert!(
                !crate::agent_hooks::anonymous_allowed(route),
                "{route} must take a token"
            );
        }
    }

    fn zinfo(name: &str, exited: bool, last: Option<u64>) -> crate::zellij::ZellijSessionInfo {
        crate::zellij::ZellijSessionInfo {
            name: name.to_string(),
            created: "2026-08-25 21:00".to_string(),
            created_ms: None,
            last_active_ms: last,
            exited,
        }
    }

    #[test]
    fn durable_puts_live_before_exited() {
        let out = shape_durable(vec![
            zinfo("dead-one", true, Some(10)),
            zinfo("live-one", false, Some(5)),
        ]);
        assert_eq!(out[0].name, "live-one");
        assert_eq!(out[1].name, "dead-one");
    }

    #[test]
    fn durable_sorts_recent_first_within_a_group() {
        let out = shape_durable(vec![
            zinfo("older", false, Some(100)),
            zinfo("newer", false, Some(900)),
        ]);
        assert_eq!(out[0].name, "newer");
    }

    #[test]
    fn durable_missing_last_active_sorts_last_not_first() {
        let out = shape_durable(vec![
            zinfo("unknown", false, None),
            zinfo("known", false, Some(1)),
        ]);
        assert_eq!(out[0].name, "known");
    }

    #[test]
    fn durable_carries_exited_flag_through() {
        let out = shape_durable(vec![zinfo("d", true, Some(1))]);
        assert!(out[0].exited);
    }

    /// A doctor body with only the clock fields varied. Everything else is
    /// fixed, so any difference in the JSON is the clock and nothing else.
    fn doctor_with(hb: &crate::heartbeat::Heartbeat) -> String {
        let (last_sweep_at, last_sweep_age_secs, sweep_ticks) = clock_fields(hb);
        serde_json::to_string(&ControlDoctor {
            ok: true,
            version: "test".into(),
            window_visible: true,
            agent_sessions: 0,
            zellij_sessions: 0,
            verify_records: 0,
            read_only: false,
            pm_enabled: true,
            last_sweep_at,
            last_sweep_age_secs,
            sweep_ticks,
            last_status_tick_at: None,
            last_status_tick_age_secs: None,
            preflight: Vec::new(),
            preflight_ready: false,
            preflight_updated_at: None,
            throughput: None,
        })
        .expect("doctor serializes")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_slow_preflight_scan_cannot_hold_the_doctor_response() {
        let cache: PreflightCache = Default::default();
        let refresh = tokio::spawn(refresh_preflight(cache.clone(), || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            vec![crate::preflight::board_check(&Err("slow mount".into()))]
        }));

        // Doctor only takes this lock and clones the last complete snapshot.
        // It does not await the scan, so even a probe well beyond a client's
        // deadline cannot turn retries into an ever-growing queue of scans.
        let during = tokio::time::timeout(std::time::Duration::from_millis(50), async {
            cache.read().await.clone()
        })
        .await
        .expect("the cached doctor snapshot must remain immediately readable");
        assert!(!during.ready);
        assert!(during.checks.is_empty());

        refresh.await.unwrap();
        let after = cache.read().await.clone();
        assert!(after.ready);
        assert_eq!(after.checks.len(), 1);
        assert!(after.updated_at.is_some());
    }

    #[test]
    fn doctor_tells_a_sweep_that_never_ran_from_one_that_ran_and_found_nothing() {
        // The hour lost on 2026-09-02. Every other field doctor reports is
        // identical in both of these, because a dead sweep breaks none of them.
        let never = crate::heartbeat::Heartbeat::new();
        let quiet = crate::heartbeat::Heartbeat::new();
        quiet.beat(); // ran, and the board had nothing on it

        let dead = doctor_with(&never);
        let alive = doctor_with(&quiet);

        assert_ne!(
            dead, alive,
            "a sweep that never ran must not report the same thing as one that ran and \
             found nothing to do"
        );
        assert!(
            dead.contains(r#""last_sweep_at":null"#),
            "never-ticked is null, not an empty string or a plausible old date: {dead}"
        );
        assert!(
            alive.contains(r#""last_sweep_at":"#) && !alive.contains(r#""last_sweep_at":null"#),
            "a quiet tick still stamps a timestamp: {alive}"
        );
        assert!(
            alive.contains(r#""sweep_ticks":1"#),
            "and says how many ticks it has managed: {alive}"
        );
    }
}
