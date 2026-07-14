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

#[derive(Clone)]
struct Ctx {
    app: AppHandle,
    token: String,
    contexts: CtxCache,
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
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 8931,
            token: String::new(),
        }
    }
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
    let ctx = Ctx {
        app,
        token,
        contexts,
    };

    let router = Router::new()
        .route("/", get(index))
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:session_id", axum::routing::delete(close_session))
        .route("/api/sessions/:session_id/files", get(list_files))
        .route("/api/sessions/:session_id/file", get(read_file))
        .route("/api/sessions/:session_id/git", get(git_overview))
        .route("/ws/:session_id", get(ws_attach))
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
    !token.is_empty() && q.get("token").map(|t| t == token).unwrap_or(false)
}

fn authed(ctx: &Ctx, q: &HashMap<String, String>) -> bool {
    token_ok(&ctx.token, q)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../../src/mobile.html"))
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

async fn handle_socket(ctx: Ctx, session_id: String, mut socket: WebSocket) {
    let state = ctx.app.state::<AppState>();

    // Snapshot scrollback + subscribe under one lock so no chunk is lost between.
    let (replay, mut rx, cols, rows) = {
        let taps = state.mobile_taps.lock().await;
        match taps.get(&session_id) {
            Some(tap) => (tap.ring.clone(), tap.tx.subscribe(), tap.cols, tap.rows),
            None => {
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
        }
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

/// Resizes the PTY. Same mechanics as pty::resize_pty, reachable without a
/// tauri::State command context. Keeps tap dims current for the next attach.
async fn resize_session(state: &AppState, session_id: &str, cols: u16, rows: u16) {
    if cols < 10 || rows < 5 || cols > 500 || rows > 200 {
        return; // garbage guard
    }
    {
        let sessions = state.pty_sessions.lock().await;
        let Some(session) = sessions.get(session_id) else {
            return;
        };
        let pty_pair = session.pty_pair.lock().await;
        let _ = pty_pair.master.resize(portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }
    if let Some(tap) = state.mobile_taps.lock().await.get_mut(session_id) {
        tap.cols = cols;
        tap.rows = rows;
    }
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
                if let Some(ip) = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
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
}
