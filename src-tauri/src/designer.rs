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

/// Dev server port exposed by the sandbox. Must match what the agent binds.
pub const DEV_PORT: u16 = 3000;
const LEASE_SECS: i64 = 6 * 3600;

use crate::sandbox::cli as gvm;

/// Shell-quotes a value for a command line.
fn q(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// `.gitvm.json` pins the sandbox shape for this design.
fn write_gitvm_config(dir: &Path) -> Result<(), String> {
    let cfg = serde_json::json!({
        "template": "agent-desktop",
        "vcpus": 4,
        "memoryMB": 8192,
        "timeout": LEASE_SECS,
        "exposedPort": DEV_PORT,
        "excludes": ["node_modules/", ".astro/", ".next/", "dist/"],
        "authSync": ["claude", "codex"],
    });
    std::fs::write(
        dir.join(".gitvm.json"),
        serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("failed to write .gitvm.json: {e}"))
}

/// Creates (or re-attaches to) this design's sandbox and records the lease.
#[tauri::command]
pub async fn designer_spin_up(project: String, slug: String) -> Result<Design, String> {
    let design = read_design(&project, &slug)?;
    if is_live(&design) {
        return Ok(design); // already running — never double-spin
    }
    let dir = source_dir(&project, &slug)?;
    write_gitvm_config(&dir)?;

    let d2 = dir.clone();
    let url = tokio::task::spawn_blocking(move || gvm::warm_up(&d2))
        .await
        .map_err(|e| e.to_string())??;

    set_sandbox(&project, &slug, "gitvm", &url, now_ms() + LEASE_SECS * 1000)
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
    tokio::task::spawn_blocking(move || gvm::pull(&d2))
        .await
        .map_err(|e| e.to_string())??;
    set_sandbox(
        &project,
        &slug,
        &design.sandbox_id,
        &design.public_url,
        now_ms() + LEASE_SECS * 1000,
    )
}

/// Runs the design agent inside this design's sandbox, then checkpoints.
/// Spins the sandbox up first if it is not live — the caller never has to.
#[tauri::command]
pub async fn designer_agent_run(
    project: String,
    slug: String,
    prompt: String,
    model: String,
) -> Result<AgentReply, String> {
    let mut design = read_design(&project, &slug)?;
    if !is_live(&design) {
        design = designer_spin_up(project.clone(), slug.clone()).await?;
    }
    let dir = source_dir(&project, &slug)?;

    let agent = if model.starts_with("codex") {
        "codex exec --dangerously-bypass-approvals-and-sandbox \"$(cat /tmp/goal.txt)\"".to_string()
    } else {
        format!(
            "claude -p --model {model} --strict-mcp-config --mcp-config '{{\"mcpServers\":{{}}}}' \
             --dangerously-skip-permissions \"$(cat /tmp/goal.txt)\""
        )
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    // The goal travels base64-encoded so quoting in the brief can never break
    // the command line.
    let script = format!(
        "echo {goal} | base64 -d > /tmp/goal.txt && cd /workspace && {agent} 2>&1",
        goal = q(&STANDARD.encode(prompt.as_bytes())),
    );

    let d2 = dir.clone();
    let out = tokio::task::spawn_blocking(move || gvm::run(&d2, &script))
        .await
        .map_err(|e| e.to_string())??;
    let text = gvm::text(&out);

    // Bring the work home immediately — an agent turn is exactly when there is
    // something new worth not losing.
    let d3 = dir.clone();
    let pulled = tokio::task::spawn_blocking(move || gvm::pull(&d3))
        .await
        .map_err(|e| e.to_string())?;
    if let Err(error) = pulled {
        eprintln!("[designer] checkpoint pull failed: {error}");
    }

    // What changed, from the vault copy we just pulled.
    let files = std::process::Command::new("git")
        .args(["status", "--porcelain"])
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
        .unwrap_or_default();

    if !out.status.success() && text.trim().is_empty() {
        return Err("agent run failed with no output".into());
    }
    Ok(AgentReply {
        text: text.trim().to_string(),
        files,
    })
}

/// Reply of one agent turn.
#[derive(Debug, Clone, Serialize)]
pub struct AgentReply {
    pub text: String,
    pub files: Vec<String>,
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
    let pulled = tokio::task::spawn_blocking(move || gvm::pull(&d2))
        .await
        .map_err(|e| e.to_string())?;
    if let Err(error) = pulled {
        return Err(format!(
            "refusing to stop: could not pull the work back ({error}). \
             The sandbox is still running; retry or copy it out first."
        ));
    }
    let d3 = dir.clone();
    tokio::task::spawn_blocking(move || gvm::stop(&d3))
        .await
        .map_err(|e| e.to_string())??;
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
