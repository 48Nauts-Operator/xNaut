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
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
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
    std::fs::create_dir_all(&root).map_err(|e| format!("failed to create {}: {e}", root.display()))?;
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

/// Remote workdir inside the sandbox. One sandbox per design (GitVM allows one
/// per directory), so the slug is the sandbox key.
const REMOTE_DIR: &str = "/workspace";

fn driver(
    settings: &crate::settings::Settings,
) -> Result<crate::sandbox::SandboxDriver, String> {
    let provider = settings
        .sandboxes
        .first()
        .ok_or("no sandbox provider configured (Settings → Sandboxes)")?;
    crate::sandbox::SandboxDriver::for_settings(provider)
}

/// Shell-quotes a path for the exec'd command line.
fn q(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

/// Spins a sandbox for this design, syncs the vault source in, installs and
/// starts the dev server, and records the lease on the design. Returns the
/// design with `public_url` populated.
#[tauri::command]
pub async fn designer_spin_up(
    state: tauri::State<'_, crate::state::AppState>,
    project: String,
    slug: String,
) -> Result<Design, String> {
    let design = read_design(&project, &slug)?;
    if is_live(&design) {
        return Ok(design); // already running — reuse, never double-spin
    }
    let source = source_dir(&project, &slug)?;
    let settings = state.settings.lock().await.clone();
    let driver = driver(&settings)?;

    let spec = crate::sandbox::SandboxSpec::default();
    let lease_secs = spec.timeout_secs as i64;
    let handle = driver.create(&spec).await?;
    driver.wait_ready(&handle.id, 180).await?;

    // Sync the vault source in. A brand-new design has only design.json, and
    // the agent scaffolds the real project on its first turn.
    let push = format!(
        "mkdir -p {dir} && echo synced",
        dir = q(REMOTE_DIR)
    );
    driver.exec(&handle.id, &push).await?;
    push_source(&driver, &handle.id, &source).await?;

    // Install + start the dev server bound to the exposed port. Hot reload is
    // what makes an edit land in the canvas in seconds instead of a rebuild.
    let start = format!(
        "cd {dir} && (test -f package.json && (npm install --no-audit --no-fund >/dev/null 2>&1; \
         nohup npm run dev -- --host 0.0.0.0 --port {port} >/tmp/dev.log 2>&1 &) \
         || (nohup npx --yes serve -l {port} . >/tmp/dev.log 2>&1 &)) && sleep 2 && echo started",
        dir = q(REMOTE_DIR),
        port = spec.exposed_port
    );
    driver.exec(&handle.id, &start).await?;

    set_sandbox(
        &project,
        &slug,
        &handle.id,
        &handle.public_url,
        now_ms() + lease_secs * 1000,
    )
}

/// Copies the design's vault folder into the sandbox with a tar over stdin —
/// one exec, no per-file round trips.
async fn push_source(
    driver: &crate::sandbox::SandboxDriver,
    id: &str,
    source: &Path,
) -> Result<(), String> {
    let tar = std::process::Command::new("tar")
        .args(["-czf", "-", "-C"])
        .arg(source)
        .arg(".")
        .output()
        .map_err(|e| format!("tar failed: {e}"))?;
    if !tar.status.success() {
        return Err("tar failed while packing the design".into());
    }
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let payload = STANDARD.encode(&tar.stdout);
    let cmd = format!(
        "echo {payload} | base64 -d | tar -xzf - -C {dir} && echo pushed",
        payload = q(&payload),
        dir = q(REMOTE_DIR)
    );
    driver.exec(id, &cmd).await.map(|_| ())
}

/// Pulls the sandbox workdir back into the vault (skipping build artefacts).
/// MUST run before every stop.
async fn pull_source(
    driver: &crate::sandbox::SandboxDriver,
    id: &str,
    source: &Path,
) -> Result<(), String> {
    let cmd = format!(
        "cd {dir} && tar -czf - --exclude=node_modules --exclude=.git --exclude=dist \
         --exclude=.next --exclude=.astro . | base64 -w0",
        dir = q(REMOTE_DIR)
    );
    let out = driver.exec(id, &cmd).await?;
    if out.exit_code != 0 {
        return Err(format!("pull failed: {}", out.stderr));
    }
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let bytes = STANDARD
        .decode(out.stdout.trim())
        .map_err(|e| format!("pull decode failed: {e}"))?;
    let mut child = std::process::Command::new("tar")
        .args(["-xzf", "-", "-C"])
        .arg(source)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("untar failed: {e}"))?;
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .ok_or("no stdin on tar")?
            .write_all(&bytes)
            .map_err(|e| format!("untar write failed: {e}"))?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("untar failed while restoring the design".into());
    }
    Ok(())
}

/// Extends the lease while a design is open, and pulls the source back as a
/// checkpoint so a surprise teardown cannot lose work.
#[tauri::command]
pub async fn designer_renew(
    state: tauri::State<'_, crate::state::AppState>,
    project: String,
    slug: String,
) -> Result<Design, String> {
    let design = read_design(&project, &slug)?;
    if design.sandbox_id.is_empty() {
        return Err("design has no sandbox".into());
    }
    let settings = state.settings.lock().await.clone();
    let driver = driver(&settings)?;
    let source = source_dir(&project, &slug)?;
    // Checkpoint first: a lease we fail to extend must not cost the work.
    pull_source(&driver, &design.sandbox_id, &source).await?;
    let lease_secs = crate::sandbox::SandboxSpec::default().timeout_secs as i64;
    driver
        .exec(&design.sandbox_id, "echo alive")
        .await
        .map_err(|e| format!("sandbox unreachable: {e}"))?;
    set_sandbox(
        &project,
        &slug,
        &design.sandbox_id,
        &design.public_url,
        now_ms() + lease_secs * 1000,
    )
}

/// Reply of one agent turn.
#[derive(Debug, Clone, Serialize)]
pub struct AgentReply {
    pub text: String,
    pub files: Vec<String>,
}

/// Runs the design agent INSIDE the design's own sandbox, so the scaffold, the
/// edits and the dev server all share one workspace (GitVM allows one sandbox
/// per directory — a second one would fight this design's own runtime).
///
/// The caller composes `prompt` (doctrine + kind starter + the user's ask); this
/// only handles execution and the code return.
#[tauri::command]
pub async fn designer_agent_run(
    state: tauri::State<'_, crate::state::AppState>,
    project: String,
    slug: String,
    prompt: String,
    model: String,
) -> Result<AgentReply, String> {
    let design = read_design(&project, &slug)?;
    if !is_live(&design) {
        return Err("no live sandbox for this design".into());
    }
    let settings = state.settings.lock().await.clone();
    let driver = driver(&settings)?;

    let agent = if model.starts_with("codex") {
        "codex exec --dangerously-bypass-approvals-and-sandbox \"$(cat /tmp/goal.txt)\"".to_string()
    } else {
        // No user MCP servers: the designer only needs file tools, and MCP
        // teardown stalls runs for minutes after the final message.
        format!(
            "claude -p --model {model} --strict-mcp-config --mcp-config '{{\"mcpServers\":{{}}}}' \
             --dangerously-skip-permissions \"$(cat /tmp/goal.txt)\""
        )
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let cmd = format!(
        "export PATH=\"/usr/local/bin:$HOME/.local/bin:$PATH\"; \
         echo {goal} | base64 -d > /tmp/goal.txt && cd {dir} && {agent} 2>&1",
        goal = q(&STANDARD.encode(prompt.as_bytes())),
        dir = q(REMOTE_DIR),
    );
    let out = driver.exec(&design.sandbox_id, &cmd).await?;
    let text = if out.stdout.trim().is_empty() {
        out.stderr.clone()
    } else {
        out.stdout.clone()
    };

    // Which files changed — git inside the workspace is the honest answer when
    // the scaffold initialised one; otherwise fall back to mtime.
    let touched = driver
        .exec(
            &design.sandbox_id,
            &format!(
                "cd {dir} && (git status --porcelain 2>/dev/null | head -20 \
                 || find . -newermt '-5 minutes' -type f -not -path './node_modules/*' | head -20)",
                dir = q(REMOTE_DIR)
            ),
        )
        .await
        .map(|r| r.stdout)
        .unwrap_or_default();
    let files: Vec<String> = touched
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    // Checkpoint the work back to the vault straight away: an agent turn is
    // exactly when there is something new worth not losing.
    let source = source_dir(&project, &slug)?;
    if let Err(error) = pull_source(&driver, &design.sandbox_id, &source).await {
        eprintln!("[designer] checkpoint pull failed: {error}");
    }

    Ok(AgentReply {
        text: text.trim().to_string(),
        files,
    })
}

/// Pulls the source back, then destroys the sandbox and clears the lease.
#[tauri::command]
pub async fn designer_stop(
    state: tauri::State<'_, crate::state::AppState>,
    project: String,
    slug: String,
) -> Result<Design, String> {
    let design = read_design(&project, &slug)?;
    if design.sandbox_id.is_empty() {
        return Ok(design);
    }
    let settings = state.settings.lock().await.clone();
    let driver = driver(&settings)?;
    let source = source_dir(&project, &slug)?;
    // Code return BEFORE teardown — teardown destroys /workspace (XNAUT-40).
    if let Err(error) = pull_source(&driver, &design.sandbox_id, &source).await {
        return Err(format!(
            "refusing to stop: could not pull the source back ({error}). \
             The sandbox is still running; retry or copy the work out first."
        ));
    }
    driver.destroy(&design.sandbox_id).await?;
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
        };
        assert!(!is_live(&d));
        d.sandbox_id = "sb-1".into();
        assert!(!is_live(&d)); // lease in the past
        d.sandbox_expires_ms = now_ms() + 60_000;
        assert!(is_live(&d));
    }

}
