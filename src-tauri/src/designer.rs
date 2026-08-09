// Designer (XNAUT-61) — the Paper replacement inside a project workspace.
//
// A design is a REAL project, not a mock: source lives in the work vault at
// work/<Project>/Design/<slug>/ and is built + served by a GitVM sandbox, so
// the canvas shows a running site on a real domain. The vault is always the
// truth; the sandbox is disposable and must be pulled back before teardown
// (XNAUT-40 — teardown destroys /workspace).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One chat turn, persisted with the design so the history survives restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignMessage {
    /// "user" | "agent"
    pub role: String,
    pub text: String,
    /// Files the agent touched this turn, e.g. ["+ src/pages/index.astro"].
    #[serde(default)]
    pub files: Vec<String>,
    pub at_ms: i64,
}

/// design.json — everything about a design except its source files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Design {
    pub slug: String,
    pub name: String,
    /// "website" | "deck" | "document" | "appui"
    pub kind: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub archived: bool,
    /// Live sandbox id, if one is currently running for this design.
    #[serde(default)]
    pub sandbox_id: String,
    /// Public URL of the running dev server (empty when not live).
    #[serde(default)]
    pub public_url: String,
    /// Unix ms when the current sandbox lease expires (0 = not live).
    #[serde(default)]
    pub sandbox_expires_ms: i64,
    #[serde(default)]
    pub messages: Vec<DesignMessage>,
    /// Claude session of the last turn — follow-ups `--resume` it instead of
    /// re-reading the whole project every message.
    #[serde(default)]
    pub session_id: String,
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Vault folder holding every design of a project: work/<Project>/Design/.
fn designs_root(project: &str) -> Result<PathBuf, String> {
    if project.is_empty() || project.contains('/') || project.contains("..") {
        return Err(format!("invalid project: {project}"));
    }
    Ok(crate::vault::vault_root("work")?
        .join(project)
        .join("Design"))
}

fn design_dir(project: &str, slug: &str) -> Result<PathBuf, String> {
    if slug.is_empty() || slug.contains('/') || slug.contains("..") {
        return Err(format!("invalid slug: {slug}"));
    }
    Ok(designs_root(project)?.join(slug))
}

fn manifest_path(project: &str, slug: &str) -> Result<PathBuf, String> {
    Ok(design_dir(project, slug)?.join("design.json"))
}

fn read_design(project: &str, slug: &str) -> Result<Design, String> {
    let path = manifest_path(project, slug)?;
    let body = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    serde_json::from_str(&body).map_err(|e| format!("invalid {}: {e}", path.display()))
}

fn write_design(project: &str, design: &Design) -> Result<(), String> {
    let dir = design_dir(project, &design.slug)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let body = serde_json::to_string_pretty(design).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("design.json"), body).map_err(|e| e.to_string())
}

/// "Marketing site — v3" -> "marketing-site-v3", uniquified against existing dirs.
pub fn slugify(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = if base.is_empty() {
        "design".to_string()
    } else {
        base
    };
    if !taken(&base) {
        return base;
    }
    for n in 2..1000 {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{base}-{}", now_ms())
}

// The starter stack per kind lives in designer-agent.js, which composes the
// prompt — one copy, no drift.

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn designer_list(project: String) -> Result<Vec<Design>, String> {
    let root = designs_root(&project)?;
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(out); // no designs yet is not an error
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let slug = entry.file_name().to_string_lossy().into_owned();
        // One unreadable manifest must not blank the whole list (cf. XNAUT-60).
        match read_design(&project, &slug) {
            Ok(design) => out.push(design),
            Err(error) => eprintln!("[designer] skipping {slug}: {error}"),
        }
    }
    out.sort_by_key(|d| std::cmp::Reverse(d.updated_at_ms));
    Ok(out)
}

#[tauri::command]
pub fn designer_create(project: String, name: String, kind: String) -> Result<Design, String> {
    let root = designs_root(&project)?;
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("failed to create {}: {e}", root.display()))?;
    let slug = slugify(&name, &|candidate| root.join(candidate).exists());
    let now = now_ms();
    let design = Design {
        slug,
        name: if name.trim().is_empty() {
            "Untitled design".into()
        } else {
            name
        },
        kind,
        created_at_ms: now,
        updated_at_ms: now,
        archived: false,
        sandbox_id: String::new(),
        public_url: String::new(),
        sandbox_expires_ms: 0,
        messages: Vec::new(),
        session_id: String::new(),
    };
    write_design(&project, &design)?;
    Ok(design)
}

#[tauri::command]
pub fn designer_get(project: String, slug: String) -> Result<Design, String> {
    read_design(&project, &slug)
}

#[tauri::command]
pub fn designer_rename(project: String, slug: String, name: String) -> Result<Design, String> {
    let mut design = read_design(&project, &slug)?;
    design.name = name;
    design.updated_at_ms = now_ms();
    write_design(&project, &design)?;
    Ok(design)
}

/// Archive (or restore). Designs are never deleted — the folder stays put.
#[tauri::command]
pub fn designer_set_archived(
    project: String,
    slug: String,
    archived: bool,
) -> Result<Design, String> {
    let mut design = read_design(&project, &slug)?;
    design.archived = archived;
    design.updated_at_ms = now_ms();
    write_design(&project, &design)?;
    Ok(design)
}

/// Appends a chat turn. The agent loop calls this after each reply so the
/// transcript survives a restart.
#[tauri::command]
pub fn designer_append_message(
    project: String,
    slug: String,
    message: DesignMessage,
) -> Result<Design, String> {
    let mut design = read_design(&project, &slug)?;
    design.messages.push(message);
    design.updated_at_ms = now_ms();
    write_design(&project, &design)?;
    Ok(design)
}

/// Records sandbox state on the design (called by the lifecycle commands).
pub fn set_sandbox(
    project: &str,
    slug: &str,
    id: &str,
    url: &str,
    expires_ms: i64,
) -> Result<Design, String> {
    let mut design = read_design(project, slug)?;
    design.sandbox_id = id.to_string();
    design.public_url = url.to_string();
    design.sandbox_expires_ms = expires_ms;
    write_design(project, &design)?;
    Ok(design)
}

/// Absolute path of a design's source folder — the sandbox syncs from here and
/// pulls back to here.
pub fn source_dir(project: &str, slug: &str) -> Result<PathBuf, String> {
    let dir = design_dir(project, slug)?;
    if !dir.is_dir() {
        return Err(format!("unknown design: {project}/{slug}"));
    }
    Ok(dir)
}

/// True when the recorded lease is still in the future.
pub fn is_live(design: &Design) -> bool {
    !design.sandbox_id.is_empty() && design.sandbox_expires_ms > now_ms()
}

// ─── Sandbox lifecycle ───────────────────────────────────────────────────────
//
// The sandbox is the runtime, the vault is the truth. Every stop pulls the
// source back FIRST (teardown destroys /workspace — XNAUT-40), so a design can
// always be reopened and re-spun from the vault.

// ─── Sandbox lifecycle, via the gitvm CLI ────────────────────────────────────
//
// The CLI is directory-scoped: `gitvm warm-up` in a directory creates that
// directory's sandbox, `gitvm run` rsyncs it to /workspace and executes there,
// `gitvm pull` brings the work back, `gitvm stop` destroys it. That is exactly
// one sandbox per design, and — unlike the SandboxDriver seam — it needs no
// provider entry in settings, which is why the first version failed with
// "no sandbox provider configured".
//
// Teardown destroys /workspace, so every stop pulls first (XNAUT-40).

/// Template that BUILDS AND SERVES the design.
///
/// agent-desktop is not a preference, it is the only option: `gitvm run` and
/// `gitvm pull` sync over **rsync**, and agent-desktop is the ONLY template
/// whose image installs it (verified 2026-08-01 against every template
/// dockerfile). app-host and base expose :80 and carry Node 20, which would
/// otherwise make them the right choice for serving a site, but `gitvm run`
/// against them dies with `rsync: command not found` and no /workspace.
const TEMPLATE: &str = "agent-desktop";

/// Port the design's dev server listens on inside the sandbox, and the port the
/// ingress tunnel is pointed at.
///
/// This is a DELIBERATE override of the template's manifest default, and the
/// only one in this file. agent-desktop declares exposedPort 6080 because that
/// is where websockify/noVNC serves the desktop — the port is occupied, so a
/// site cannot be served on it. Every other value in `.gitvm.json` comes from
/// the manifest; this one cannot, because no manifest describes a template that
/// both syncs over rsync and leaves a web port free. XNAUT-63 tracks adding
/// rsync to app-host, which would let this constant disappear.
///
/// Overriding the port is only safe because spin-up now PROVES the tunnel
/// answers before reporting success, and destroys the sandbox when it does not.
const SERVE_PORT: u16 = 3000;

/// Local services reverse-forwarded into every design sandbox, so a built site
/// can reach them at its own `localhost`. NautGate (:8090) is the one designs
/// actually call — a generated app that talks to an LLM points there.
const LOCAL_PORTS: &[u16] = &[8090];

/// How long to keep waiting on a 502 before calling the sandbox dead.
///
/// A first build runs `npm install` for a fresh Astro/Next project, which is
/// minutes, not seconds. The old 20-second give-up destroyed the sandbox
/// mid-install and retried from scratch — a loop that could never converge.
const PROBE_WAIT_SECS: u64 = 420;
use crate::sandbox::cli as gvm;
use tauri::Emitter;

/// One step of a build, streamed to the chat box as it happens.
fn step(app: &tauri::AppHandle, slug: &str, text: &str) {
    let _ = app.emit(
        "designer-progress",
        serde_json::json!({ "slug": slug, "text": text }),
    );
    // Also to the durable log. A status emitted to the UI and nowhere else is
    // gone the moment the next one replaces it, which is why "Starting the
    // sandbox…" could sit on screen for an hour with no way to find out what
    // the app was actually blocked on.
    dlog(slug, "info", text, "designer.step", None);
}

/// One line in this design's durable log.
///
/// Same store as the build log, so `Build log` can read a Designer session back
/// weeks later: ~/Library/Application Support/xnaut/looms/logs/design-<slug>.jsonl
/// Never returns an error; a log must not be able to fail the thing it records.
fn dlog(slug: &str, level: &str, event: &str, kind: &str, data: Option<serde_json::Value>) {
    crate::build_log::build_log_append(
        format!("design-{slug}"),
        level.to_string(),
        "designer".to_string(),
        event.to_string(),
        Some(kind.to_string()),
        data,
    );
}

/// Run a step that talks to the sandbox, timing it and logging both ends.
///
/// EVERY gitvm call gets a deadline. On 2026-08-09 a fresh design sat on
/// "Starting the sandbox…" indefinitely: the sandbox was created and healthy,
/// nothing was listening on the exposed port, and there was no way to tell
/// whether the call had failed or was simply still running, because a blocked
/// call looks exactly like a slow one. A hang is now a reported failure with a
/// duration attached, not a spinner.
async fn timed<T, F>(slug: &str, what: &str, secs: u64, f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let started = std::time::Instant::now();
    dlog(
        slug,
        "debug",
        &format!("{what}: starting"),
        "designer.call",
        None,
    );
    let handle = tokio::task::spawn_blocking(f);
    let out = match tokio::time::timeout(std::time::Duration::from_secs(secs), handle).await {
        Err(_) => {
            let msg = format!("{what} did not return within {secs}s");
            dlog(slug, "error", &msg, "designer.timeout", None);
            return Err(msg);
        }
        Ok(Err(join)) => {
            let msg = format!("{what} panicked: {join}");
            dlog(slug, "error", &msg, "designer.panic", None);
            return Err(msg);
        }
        Ok(Ok(r)) => r,
    };
    let ms = started.elapsed().as_millis() as u64;
    match &out {
        Ok(_) => dlog(
            slug,
            "debug",
            &format!("{what}: ok in {ms}ms"),
            "designer.call",
            None,
        ),
        Err(e) => dlog(
            slug,
            "error",
            &format!("{what} failed after {ms}ms: {e}"),
            "designer.call",
            None,
        ),
    }
    out
}

/// Starts the right server for what is currently in the sandbox.
///
/// Without this the sandbox exists but nothing listens on the exposed port,
/// which is exactly what a white canvas looks like.
///
/// "Right" matters: the first spin-up happens BEFORE the agent has written
/// anything, so there is no package.json yet and a plain static server is
/// started so the canvas is not blank. Once the agent scaffolds a real project
/// that static server is wrong — it hands `import 'reveal.js'` to the browser
/// verbatim and the page dies on "Failed to resolve module specifier". A
/// port-is-busy check cannot see that, so it reported "already up" forever and
/// the real dev server never got a chance (the deck that would not render).
/// The placeholder is therefore evicted as soon as a package.json appears.
///
/// It also serves a holding page from its own directory, never /workspace.
/// Serving project files raw poisons the browser cache in a way that survives
/// the fix: python's response carries Last-Modified from the file mtime, so
/// after Vite takes over the browser revalidates, gets 304 for an unchanged
/// file, and keeps the untransformed body — `import 'reveal.js'` forever.
/// Nothing the sandbox does later can clear that; not serving the file in the
/// first place is the only reliable fix.
fn ensure_dev_server(dir: &Path, port: u16) -> Result<String, String> {
    // Check the PORT, never pgrep: `pgrep -f "http.server"` matches gitvm's own
    // command line (it contains the string), so it always claimed the server
    // was already up and nothing ever started — the white canvas.
    // `setsid -f`, not `(setsid ... &)`: with the subshell form the remote
    // command never returned, so `gitvm run` held its SSH channel open for the
    // caller's whole deadline while the server was already answering 200. The
    // forced fork detaches it and the shell returns in about five seconds.
    // Note the same subshell form DOES return when run by hand, so the exact
    // trigger is not pinned down; `-f` is the robust idiom regardless.
    let script = format!(
        "cd /workspace && \
         if [ -f package.json ]; then \
           holder=$(ss -ltnp 2>/dev/null | grep ':{port} ' | grep python3 \
                    | grep -o 'pid=[0-9]*' | head -1 | cut -d= -f2); \
           if [ -n \"$holder\" ]; then \
             echo 'evicting the static placeholder'; \
             kill \"$holder\" 2>/dev/null || true; sleep 1; \
           fi; \
           if ss -ltn 2>/dev/null | grep -q ':{port} '; then \
             echo 'dev server already up on {port}'; \
           else \
             if ! ( [ -d node_modules ] && [ -n \"$(ls -A node_modules 2>/dev/null)\" ] \
                    || npm install --no-audit --no-fund >/tmp/install.log 2>&1 ); then \
               echo 'npm install failed:' >&2; tail -12 /tmp/install.log >&2; exit 1; \
             fi; \
             echo 'dependencies installed'; \
             setsid -f npm run dev -- --host 0.0.0.0 --port {port} </dev/null >/tmp/dev.log 2>&1; \
             for i in $(seq 1 300); do \
               ss -ltn 2>/dev/null | grep -q ':{port} ' && break; sleep 1; \
             done; \
             ss -ltn 2>/dev/null | grep -q ':{port} ' && echo 'dev server up on {port}' \
               || (echo 'dev server failed:' >&2; tail -12 /tmp/dev.log >&2; exit 1); \
           fi; \
         elif ss -ltn 2>/dev/null | grep -q ':{port} '; then \
           echo 'serving on {port}'; \
         else \
           mkdir -p /tmp/designer-holding && \
           printf '%s' '<!doctype html><meta charset=utf-8><title>Preparing…</title><body style=\"font:15px/1.6 system-ui;display:grid;place-items:center;height:100vh;margin:0;color:#666\">Preparing this design…</body>' \
             > /tmp/designer-holding/index.html; \
           cd /tmp/designer-holding; \
           setsid -f python3 -m http.server {port} --bind 0.0.0.0 </dev/null >/tmp/dev.log 2>&1; \
           for i in $(seq 1 10); do \
             ss -ltn 2>/dev/null | grep -q ':{port} ' && break; sleep 1; \
           done; \
           ss -ltn 2>/dev/null | grep -q ':{port} ' \
             && echo 'holding page on {port} (no project yet)' \
             || (echo 'holding page failed:' >&2; tail -12 /tmp/dev.log >&2; exit 1); \
         fi",
        port = port
    );
    let out = gvm::run_checked(dir, &script)?;
    let message = gvm::text(&out).trim().to_string();
    if message.is_empty() {
        return Err("gitvm run succeeded but the dev-server step returned no status".into());
    }
    Ok(message)
}

/// `.gitvm.json` for this design: the TEMPLATE's own manifest values, plus the
/// one documented port override.
///
/// Size and TTL are read from the manifest, never invented. `gitvm warm-up`
/// always sends vcpus/memoryMB/timeout/exposedPort and falls back to
/// agent-desktop's (4 / 8192 / 21600 / 6080) for *whatever* template it is
/// given, so omitting a field does NOT mean "use the manifest" — it means
/// "silently use the desktop template's value", which on any other template
/// points the tunnel at a port nothing serves. Writing the fetched values is
/// the only way the manifest actually wins.
///
/// Returns the port the dev server must bind.
fn write_gitvm_config(dir: &Path) -> Result<u16, String> {
    let d = gvm::template_defaults(TEMPLATE)?;
    let cfg = serde_json::json!({
        "template": TEMPLATE,
        "vcpus": d.vcpus,
        "memoryMB": d.memory_mb,
        "timeout": d.timeout_secs,
        "exposedPort": SERVE_PORT, // see SERVE_PORT — 6080 is noVNC's
        "excludes": ["node_modules/", ".astro/", ".next/", "dist/"],
    });
    std::fs::write(
        dir.join(".gitvm.json"),
        serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("failed to write .gitvm.json: {e}"))?;
    // The vault is a git repo and a design is a real project, so `npm install`
    // drops ~18 MB of node_modules straight into it. Keep the source, never the
    // build output or the sandbox handle.
    let _ = std::fs::write(
        dir.join(".gitignore"),
        "node_modules/\ndist/\n.astro/\n.next/\n.gitvm/\n.loom-*.txt\n.loom-agent.sh\n",
    );
    Ok(SERVE_PORT)
}

/// The exposed port this design's sandbox was actually created with, read from
/// the `.gitvm.json` warm-up used. Falls back to the template manifest.
fn read_exposed_port(dir: &Path) -> u16 {
    std::fs::read_to_string(dir.join(".gitvm.json"))
        .ok()
        .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
        .and_then(|v| v["exposedPort"].as_u64())
        .map(|p| p as u16)
        .unwrap_or(SERVE_PORT)
}

/// Only a caller-side deadline may be recovered by observing that the server
/// came up anyway. A real `gitvm run` error (failed rsync, install, or command)
/// must remain an error even if an older placeholder still answers HTTP 200.
fn dev_server_call_timed_out(error: &str) -> bool {
    error.starts_with("starting the dev server did not return within ")
}

/// Creates (or re-attaches to) this design's sandbox and records the lease.
#[tauri::command]
pub async fn designer_spin_up(
    app: tauri::AppHandle,
    project: String,
    slug: String,
) -> Result<Design, String> {
    let design = read_design(&project, &slug)?;
    let dir = source_dir(&project, &slug)?;
    if is_live(&design) {
        // Already running — never double-spin, but DO re-read the URL: designs
        // created before the vnc/publicUrl fix have the noVNC viewer stored,
        // which renders a blank canvas forever otherwise.
        let d0 = dir.clone();
        if let Ok(Ok(url)) = tokio::task::spawn_blocking(move || gvm::public_url(&d0)).await {
            if !url.is_empty() && url != design.public_url {
                return set_sandbox(
                    &project,
                    &slug,
                    &design.sandbox_id,
                    &url,
                    design.sandbox_expires_ms,
                );
            }
        }
        return Ok(design);
    }
    // ADOPT a sandbox this design has lost track of, before trying to make one.
    //
    // design.json and .gitvm/state.json are two records of the same fact, and
    // they can disagree: a crash between `create` and `set_sandbox`, or a design
    // touched outside the app, leaves design.json with an empty sandbox_id while
    // state.json still holds a perfectly healthy box.
    //
    // That combination WEDGES the design permanently. is_live() sees no sandbox
    // so it will not reconnect, and warm-up refuses to run because state.json
    // exists and the control plane agrees the box is running, so it cannot
    // create one either. Both attempts fail and the UI says "Sandbox did not
    // answer" about a sandbox that is answering. Observed 2026-08-09 on a design
    // whose box was serving HTTP 200 with four hours left on its lease.
    if design.sandbox_id.is_empty() && dir.join(".gitvm/state.json").exists() {
        let d0 = dir.clone();
        let found = timed(&slug, "checking for an adoptable sandbox", 45, move || {
            Ok((|| {
                if gvm::state_is_stale(&d0) {
                    return None; // control plane says 404: really gone, make a new one
                }
                let url = gvm::public_url(&d0).ok().filter(|u| !u.is_empty())?;
                // Only adopt something that actually answers. A stale hostname that
                // resolves to nothing is worse than no sandbox, because it looks live.
                if !matches!(gvm::probe(&url), Some(c) if (200..400).contains(&c)) {
                    return None;
                }
                let body = std::fs::read_to_string(d0.join(".gitvm/state.json")).ok()?;
                let st: serde_json::Value = serde_json::from_str(&body).ok()?;
                let id = st["sandboxId"].as_str()?.to_string();
                // Take the lease from the server rather than assuming a full one.
                let expires = gvm::expires_ms(&d0).unwrap_or(0);
                Some((id, url, expires))
            })())
        })
        .await?;

        if let Some((id, url, expires)) = found {
            step(
                &app,
                &slug,
                "Reconnected to the sandbox that was already running.",
            );
            return set_sandbox(&project, &slug, &id, &url, expires);
        }
    }

    let port = write_gitvm_config(&dir)?;
    let lease_secs = i64::from(gvm::template_defaults(TEMPLATE)?.timeout_secs);

    // Two attempts: a VM can die silently between create and first use, leaving
    // the row "running" server-side. Never leave that behind — every failed
    // attempt is destroyed before the next one, or its ingress hostname leaks.
    let mut last_error = String::new();
    for attempt in 1..=2 {
        step(
            &app,
            &slug,
            if attempt == 1 {
                "Starting the sandbox…".to_string()
            } else {
                // Say WHY. This message used to be a fixed string, so a real
                // failure ("already warm", no API key, control plane down) was
                // indistinguishable from a slow boot, and the only way to find
                // out was to run the CLI by hand.
                format!(
                    "Sandbox did not start ({}). Destroying it and retrying…",
                    last_error.trim()
                )
            }
            .as_str(),
        );
        // Each call is separately deadlined and logged, so a stall names the
        // step it stalled in instead of the whole spin-up going quiet.
        let started: Result<(String, String), String> = async {
            // A reaped VM leaves .gitvm/state.json behind and warm-up refuses to
            // run while it exists ("already warm"), so the directory would be
            // wedged forever. Clear it only when the control plane says the
            // sandbox is really gone.
            let d = dir.clone();
            let stale = timed(&slug, "checking existing sandbox state", 25, move || {
                Ok(gvm::state_is_stale(&d))
            })
            .await?;
            if stale {
                let d = dir.clone();
                timed(&slug, "clearing stale sandbox state", 45, move || {
                    gvm::stop(&d)
                })
                .await?;
            }
            let d = dir.clone();
            timed(&slug, "gitvm warm-up", 180, move || gvm::warm_up(&d)).await?;
            // A built site that calls a local service (NautGate on :8090) needs
            // that service to exist at the sandbox's own localhost, or its dev
            // server proxies the refused connection back as a bare 500.
            for port in LOCAL_PORTS {
                let d = dir.clone();
                let local_port = *port;
                let what = format!("exposing local port {local_port}");
                if let Err(error) = timed(&slug, &what, 30, move || {
                    gvm::expose_local_port(&d, local_port)
                })
                .await
                {
                    eprintln!("[designer] {error}"); // not fatal: most designs never call out
                    dlog(&slug, "warn", &error, "designer.forward", None);
                }
            }
            // warm-up echoes the template's convenience URL; the authoritative
            // publicUrl comes from `gitvm status` as JSON.
            let d = dir.clone();
            let url = timed(&slug, "reading sandbox public URL", 5, move || {
                gvm::public_url(&d)
            })
            .await?;
            // Serve whatever is already there before claiming the box is up —
            // "created" is not "reachable", and the create response alone has
            // never been proof of either.
            // THE step that was invisible. On a fresh design nothing ever bound
            // the exposed port and /tmp/designer-holding was never created, and
            // because its answer went nowhere there was no way to tell whether
            // it had failed, hung, or never run. Its reply is now a log line.
            let d = dir.clone();
            let serve_timeout = if dir.join("package.json").is_file() {
                360 // first dependency install can legitimately take minutes
            } else {
                // 30s assumed a healthy control plane. Observed 2026-08-09 with a
                // slow one: warm-up alone took 113s, and this step, which is an
                // ssh round trip plus an rsync, blew a 30s budget while actually
                // succeeding. A deadline exists to catch a HANG, not to punish a
                // slow day.
                120
            };
            let msg = match timed(&slug, "starting the dev server", serve_timeout, move || {
                ensure_dev_server(&d, port)
            })
            .await
            {
                Ok(m) => {
                    dlog(
                        &slug,
                        "info",
                        &format!("dev server: {m}"),
                        "designer.serve",
                        None,
                    );
                    m
                }
                Err(e) => {
                    // A TIMEOUT IS NOT A FAILURE IF THE WORK LANDED. This step's
                    // whole purpose is observable from outside: either something
                    // is serving on the public URL or it is not. On 2026-08-09
                    // the call blew its deadline while the holding page had
                    // already started and the URL was returning 200, and xNAUT
                    // tore the sandbox down anyway. Ask the outcome before
                    // believing the call.
                    let answered = if dev_server_call_timed_out(&e) {
                        let probe_url = url.clone();
                        tokio::task::spawn_blocking(move || gvm::probe(&probe_url))
                            .await
                            .unwrap_or(None)
                    } else {
                        None
                    };
                    if matches!(answered, Some(c) if (200..400).contains(&c)) {
                        dlog(
                            &slug,
                            "warn",
                            &format!(
                                "dev server call timed out ({e}) but {url} answers, continuing"
                            ),
                            "designer.serve",
                            None,
                        );
                        String::from("dev server already answering")
                    } else {
                        dlog(
                            &slug,
                            "error",
                            &format!("dev server step failed: {e}"),
                            "designer.serve",
                            None,
                        );
                        return Err(e);
                    }
                }
            };
            Ok((url, msg))
        }
        .await;

        match started {
            Ok((url, msg)) => {
                step(&app, &slug, &msg);
                // Does the public URL actually answer? Two very different
                // failures hide behind "no":
                //
                // * A 502/503/504 means the tunnel is FINE and the upstream is
                //   not listening yet — on a first build `npm install` runs for
                //   minutes. Destroying the sandbox here throws the install away
                //   and the retry starts from zero, so it can never succeed: an
                //   infinite regression that eats a sandbox per attempt.
                // * No response at all means the hostname or the VM is gone,
                //   which a retry can genuinely fix.
                //
                // So a gateway error is a REASON TO WAIT, not to tear down.
                let mut code;
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_secs(PROBE_WAIT_SECS);
                let mut announced = false;
                loop {
                    let probe_url = url.clone();
                    code = tokio::task::spawn_blocking(move || gvm::probe(&probe_url))
                        .await
                        .unwrap_or(None);
                    dlog(
                        &slug,
                        "debug",
                        &format!(
                            "probe {url} -> {}",
                            code.map(|c| c.to_string())
                                .unwrap_or_else(|| "no answer".into())
                        ),
                        "designer.probe",
                        None,
                    );
                    if matches!(code, Some(c) if (200..400).contains(&c)) {
                        break;
                    }
                    let gateway_busy = matches!(code, Some(502..=504));
                    if !gateway_busy || std::time::Instant::now() >= deadline {
                        break;
                    }
                    if !announced {
                        announced = true;
                        step(
                            &app,
                            &slug,
                            "Sandbox is up; waiting for the dev server (first build installs dependencies)…",
                        );
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
                if matches!(code, Some(c) if (200..400).contains(&c)) {
                    step(&app, &slug, &format!("Sandbox serving at {url}"));
                    return set_sandbox(
                        &project,
                        &slug,
                        "gitvm",
                        &url,
                        now_ms() + lease_secs * 1000,
                    );
                }
                last_error = match code {
                    Some(c) => format!(
                        "{url} answered HTTP {c} after waiting {PROBE_WAIT_SECS}s for the dev server"
                    ),
                    None => format!("{url} did not answer"),
                };
            }
            Err(error) => last_error = error,
        }
        // Destroy before retrying (and before giving up) — this is the DELETE
        // that was missing when four orphan tunnels leaked.
        // A pre-create failure (missing key/control plane down) has no state to
        // destroy. Calling `gitvm stop` then only replaces the useful error with
        // "no sandbox in this dir".
        if dir.join(".gitvm/state.json").is_file() {
            let d = dir.clone();
            if let Err(error) = timed(&slug, "destroying failed sandbox", 45, move || {
                gvm::stop(&d)
            })
            .await
            {
                return Err(format!(
                    "sandbox failed ({last_error}) and cleanup also failed ({error}); refusing to create another sandbox"
                ));
            }
        }
    }
    Err(format!(
        "sandbox never became reachable ({last_error}); it has been destroyed, not left running"
    ))
}

/// Extends the lease and checkpoints the work back to the vault.
#[tauri::command]
pub async fn designer_renew(project: String, slug: String) -> Result<Design, String> {
    let design = read_design(&project, &slug)?;
    if design.sandbox_id.is_empty() {
        return Err("design has no sandbox".into());
    }
    let dir = source_dir(&project, &slug)?;
    let d2 = dir.clone();
    timed(&slug, "checkpointing before lease renewal", 60, move || {
        gvm::pull(&d2)
    })
    .await?;
    set_sandbox(
        &project,
        &slug,
        &design.sandbox_id,
        &design.public_url,
        now_ms() + i64::from(gvm::template_defaults(TEMPLATE)?.timeout_secs) * 1000,
    )
}

/// Publishes what the agent just wrote: rsync the design folder into the
/// sandbox, make sure the dev server is serving it, and checkpoint back.
///
/// The agent run itself is NOT here — it goes through `loom_run` +
/// `xnautDriveRun`, the same path a NautFlow persona uses. This command is only
/// the sandbox half of a turn, called once the driver reports the run finished.
#[tauri::command]
pub async fn designer_publish(
    app: tauri::AppHandle,
    project: String,
    slug: String,
) -> Result<Vec<String>, String> {
    let dir = source_dir(&project, &slug)?;
    step(&app, &slug, "Syncing the build into the sandbox…");
    // `gitvm run` rsyncs local → /workspace on every invocation, so this both
    // ships the new source and (re)starts the server if it is not listening.
    // The port is the template's own exposed port — read it back from the
    // config warm-up used, never a constant of ours.
    let port = read_exposed_port(&dir);
    let d1 = dir.clone();
    let serve_timeout = if dir.join("package.json").is_file() {
        360
    } else {
        30
    };
    match timed(
        &slug,
        "publishing the dev server",
        serve_timeout,
        move || ensure_dev_server(&d1, port),
    )
    .await
    {
        Ok(msg) => step(&app, &slug, &msg),
        Err(error) => {
            step(&app, &slug, &format!("dev server: {error}"));
            return Err(error);
        }
    }
    // Bring the work home immediately — an agent turn is exactly when there is
    // something new worth not losing (teardown destroys /workspace, XNAUT-40).
    let d2 = dir.clone();
    if let Err(error) = timed(&slug, "checkpointing the published design", 60, move || {
        gvm::pull(&d2)
    })
    .await
    {
        eprintln!("[designer] checkpoint pull failed: {error}");
        dlog(&slug, "warn", &error, "designer.checkpoint", None);
    }
    // What changed, from the vault copy we just pulled.
    Ok(std::process::Command::new("git")
        .args(["status", "--porcelain", "--", "."])
        .current_dir(&dir)
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .take(20)
                .map(|l| l.trim().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}

/// Remembers the claude session of the last turn so follow-ups can `--resume`.
#[tauri::command]
pub fn designer_set_session(
    project: String,
    slug: String,
    session_id: String,
) -> Result<Design, String> {
    let mut design = read_design(&project, &slug)?;
    design.session_id = session_id;
    write_design(&project, &design)?;
    Ok(design)
}

/// Pulls the source back, then destroys the sandbox and clears the lease.
#[tauri::command]
pub async fn designer_stop(project: String, slug: String) -> Result<Design, String> {
    let design = read_design(&project, &slug)?;
    if design.sandbox_id.is_empty() {
        return Ok(design);
    }
    let dir = source_dir(&project, &slug)?;
    let d2 = dir.clone();
    let pulled = timed(&slug, "pulling design before stop", 60, move || {
        gvm::pull(&d2)
    })
    .await;
    if let Err(error) = pulled {
        return Err(format!(
            "refusing to stop: could not pull the work back ({error}). \
             The sandbox is still running; retry or copy it out first."
        ));
    }
    let d3 = dir.clone();
    timed(&slug, "stopping design sandbox", 45, move || gvm::stop(&d3)).await?;
    set_sandbox(&project, &slug, "", "", 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_makes_safe_unique_slugs() {
        let free = |_: &str| false;
        assert_eq!(slugify("Marketing site — v3", &free), "marketing-site-v3");
        assert_eq!(slugify("  ", &free), "design");
        assert_eq!(slugify("../etc/passwd", &free), "etc-passwd");
        // collision -> suffixed, never overwrites an existing design
        let taken = |s: &str| s == "deck";
        assert_eq!(slugify("Deck", &taken), "deck-2");
    }

    #[test]
    fn invalid_project_and_slug_are_refused() {
        assert!(designs_root("../escape").is_err());
        assert!(designs_root("").is_err());
        assert!(design_dir("WebBuilder", "..").is_err());
        assert!(design_dir("WebBuilder", "a/b").is_err());
    }

    #[test]
    fn is_live_needs_id_and_unexpired_lease() {
        let mut d = Design {
            slug: "s".into(),
            name: "n".into(),
            kind: "website".into(),
            created_at_ms: 0,
            updated_at_ms: 0,
            archived: false,
            sandbox_id: String::new(),
            public_url: String::new(),
            sandbox_expires_ms: 0,
            messages: Vec::new(),
            session_id: String::new(),
        };
        assert!(!is_live(&d));
        d.sandbox_id = "sb-1".into();
        assert!(!is_live(&d)); // lease in the past
        d.sandbox_expires_ms = now_ms() + 60_000;
        assert!(is_live(&d));
    }

    #[test]
    fn only_a_dev_server_deadline_can_be_recovered_by_a_probe() {
        assert!(dev_server_call_timed_out(
            "starting the dev server did not return within 120s"
        ));
        assert!(!dev_server_call_timed_out(
            "gitvm run failed with exit 23: rsync failed"
        ));
        assert!(!dev_server_call_timed_out("npm install failed"));
    }
}
