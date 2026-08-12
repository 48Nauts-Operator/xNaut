// Zellij session wrapper for Tasks Mode v1.6 — every task is a named Zellij session.
// xNaut PTY panes host a task by running `zellij attach` (or create-with-layout on first run).

use std::path::{Path, PathBuf};
use std::process::Command;

/// True if the zellij binary is on PATH.
pub fn is_installed() -> bool {
    Command::new("zellij")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Names of currently-running zellij sessions. Parses `zellij list-sessions -n -s`
/// (no formatting, short = name only). Empty vec when none or zellij missing.
pub fn list_sessions() -> Vec<String> {
    let output = match Command::new("zellij")
        .args(["list-sessions", "-n", "-s"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    // zellij exits non-zero when no sessions exist — treat that as empty, not an error.
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn session_exists(name: &str) -> bool {
    list_sessions().iter().any(|s| s == name)
}

/// Names of LIVE sessions only. Unlike `list_sessions` (`-s`, names only, dead
/// sessions included) this keeps the full `-n` lines so EXITED-but-resurrectable
/// sessions can be filtered out — the Observatory's liveness source for build
/// worktree agents, which must survive a webview reload.
pub fn list_live_sessions() -> Vec<String> {
    let run = |bin: &str| {
        Command::new(bin)
            .args(["list-sessions", "-n"])
            .output()
            .ok()
    };
    // A Finder-launched app has a minimal PATH — fall back to the Homebrew binary.
    let Some(output) = run("zellij").or_else(|| run("/opt/homebrew/bin/zellij")) else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new(); // zellij exits non-zero when no sessions exist
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.contains("EXITED"))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}

/// Escapes a string for embedding inside a KDL double-quoted string.
fn kdl_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Pure builder for the single-pane KDL layout body — split out so it's testable
/// without touching the filesystem.
fn layout_kdl(cwd: &str, shell_command: &str) -> String {
    format!(
        "layout {{\n    pane command=\"sh\" {{\n        args \"-c\" \"{}\"\n        cwd \"{}\"\n    }}\n}}\n",
        kdl_escape(shell_command),
        kdl_escape(cwd)
    )
}

/// Writes a single-pane KDL layout that runs `shell_command` (via sh -c) in `cwd`,
/// to ~/.config/xnaut/layouts/<session>.kdl. Returns the layout path.
pub fn write_layout(session: &str, cwd: &str, shell_command: &str) -> Result<PathBuf, String> {
    let dir = dirs::home_dir()
        .ok_or_else(|| "could not resolve home directory".to_string())?
        .join(".config")
        .join("xnaut")
        .join("layouts");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create layout dir {}: {e}", dir.display()))?;
    let path = dir.join(format!("{session}.kdl"));
    std::fs::write(&path, layout_kdl(cwd, shell_command))
        .map_err(|e| format!("failed to write layout {}: {e}", path.display()))?;
    Ok(path)
}

/// Single-quote shell escaping: wrap in single quotes, escape embedded single
/// quotes as '\'' .
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The shell command an xNaut PTY pane should run to host this task's session:
/// - if the session already exists: `zellij attach <name>`
/// - else with layout: `zellij --session <name> --new-session-with-layout <layout_path>`
/// - else: `zellij attach --create <name>`
pub fn launch_command(session: &str, layout: Option<&Path>) -> String {
    let name = shell_quote(session);
    if session_exists(session) {
        return format!("zellij attach {name}");
    }
    match layout {
        Some(path) => format!(
            "zellij --session {name} --new-session-with-layout {}",
            shell_quote(&path.to_string_lossy())
        ),
        None => format!("zellij attach --create {name}"),
    }
}

/// Kills the named session via `zellij kill-session <name>`. Ok on success OR
/// when the session doesn't exist (already gone is good enough).
#[allow(dead_code)]
pub fn kill_session(name: &str) -> Result<(), String> {
    let output = Command::new("zellij")
        .args(["kill-session", name])
        .output()
        .map_err(|e| format!("failed to invoke zellij: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.to_lowercase().contains("not found") || !session_exists(name) {
        return Ok(());
    }
    Err(format!(
        "zellij kill-session {name} failed: {}",
        stderr.trim()
    ))
}

/// Sanitize an arbitrary task/project name into a valid session name:
/// lowercase, alphanumerics and dashes only, collapse repeats, trim dashes,
/// max 40 chars, fallback "task" if empty.
pub fn session_name(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let mut name: String = out.trim_matches('-').chars().take(40).collect();
    name = name.trim_matches('-').to_string();
    if name.is_empty() {
        "task".to_string()
    } else {
        name
    }
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn zellij_check() -> Result<bool, String> {
    Ok(is_installed())
}

#[tauri::command]
pub fn zellij_sessions() -> Result<Vec<String>, String> {
    Ok(list_sessions())
}

#[tauri::command]
pub fn zellij_live_sessions() -> Result<Vec<String>, String> {
    Ok(list_live_sessions())
}

/// Live sessions with their creation annotation and a last-activity timestamp.
/// Last activity comes from the session-resurrection cache
/// (~/Library/Caches/org.Zellij-Contributors.Zellij/*/session_info/<name>/),
/// which zellij rewrites periodically while a session is alive — the session
/// socket's mtime is only the creation time.
#[derive(Debug, serde::Serialize)]
pub struct ZellijSessionInfo {
    pub name: String,
    pub created: String,
    pub last_active_ms: Option<u64>,
    /// Dead but resurrectable — `zellij attach` rebuilds it from the serialized
    /// layout. Callers that only want live sessions filter on this; the sidebar
    /// shows both, since a resurrectable session is still somewhere to go back to.
    #[serde(default)]
    pub exited: bool,
}

/// The zellij binary to run. A Finder-launched app inherits a minimal PATH, so a
/// bare `zellij` cannot even spawn there. The list commands already fall back to
/// the Homebrew path; delete has to as well, or a session the caller believes it
/// killed is still there to swallow the next launch (XNAUT-93).
fn zellij_bin() -> &'static str {
    if Command::new("zellij").arg("--version").output().is_ok() {
        "zellij"
    } else {
        "/opt/homebrew/bin/zellij"
    }
}

/// Kills a session and discards its resurrection layout, so it stops appearing
/// in `zellij ls` as "EXITED — attach to resurrect". `--force` is required to
/// take a session that still has a client attached; without it zellij refuses
/// and the caller is left with a session it cannot remove.
///
/// The name is passed as an argument, never through a shell, so a session name
/// cannot turn into a command.
#[tauri::command]
pub fn zellij_delete_session(name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a session name is required".into());
    }
    let out = Command::new(zellij_bin())
        .args(["delete-session", name, "--force"])
        .output()
        .map_err(|e| format!("could not run zellij: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    // Already gone is the outcome the caller wanted, not a failure.
    if err.to_lowercase().contains("no session") || err.is_empty() {
        return Ok(());
    }
    Err(err)
}

#[tauri::command]
pub fn zellij_sessions_info() -> Vec<ZellijSessionInfo> {
    let run = |bin: &str| {
        Command::new(bin)
            .args(["list-sessions", "-n"])
            .output()
            .ok()
    };
    let Some(output) = run("zellij").or_else(|| run("/opt/homebrew/bin/zellij")) else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let cache = dirs::home_dir().map(|h| h.join("Library/Caches/org.Zellij-Contributors.Zellij"));
    let mut out = Vec::new();
    for l in String::from_utf8_lossy(&output.stdout).lines() {
        if l.trim().is_empty() {
            continue;
        }
        let exited = l.contains("EXITED");
        let Some(name) = l.split_whitespace().next().map(str::to_string) else {
            continue;
        };
        let created = l
            .find('[')
            .and_then(|a| {
                l[a..]
                    .find(']')
                    .map(|b| l[a + 1..a + b].trim_start_matches("Created").trim().to_string())
            })
            .unwrap_or_default();
        let mut last_active_ms = None;
        if let Some(cache) = &cache {
            if let Ok(entries) = std::fs::read_dir(cache) {
                for e in entries.flatten() {
                    let dir = e.path().join("session_info").join(&name);
                    for f in ["session-metadata.kdl", "session-layout.kdl"] {
                        if let Ok(md) = std::fs::metadata(dir.join(f)) {
                            if let Ok(t) = md.modified() {
                                if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                                    let ms = d.as_millis() as u64;
                                    if last_active_ms.is_none_or(|c| ms > c) {
                                        last_active_ms = Some(ms);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        out.push(ZellijSessionInfo { name, created, last_active_ms, exited });
    }
    out
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_name_sanitizes() {
        assert_eq!(session_name("NautWire L5 Audit!"), "nautwire-l5-audit");
    }

    #[test]
    fn session_name_empty_falls_back() {
        assert_eq!(session_name(""), "task");
        assert_eq!(session_name("!!!"), "task");
    }

    #[test]
    fn session_name_truncates_to_40() {
        let raw = "a".repeat(60);
        let name = session_name(&raw);
        assert_eq!(name.len(), 40);
        assert_eq!(name, "a".repeat(40));
    }

    #[test]
    fn session_name_collapses_and_trims_dashes() {
        assert_eq!(session_name("--Foo___Bar--"), "foo-bar");
    }

    #[test]
    fn launch_command_escapes_single_quotes() {
        // Session names from session_name() never contain quotes, but the escaping
        // must hold for arbitrary input anyway.
        let cmd = launch_command("it's-a-task", None);
        assert!(cmd.contains("'it'\\''s-a-task'"));
    }

    #[test]
    fn layout_kdl_shape_and_escaping() {
        let kdl = layout_kdl("/tmp/work dir", "echo \"hi\"");
        assert_eq!(
            kdl,
            "layout {\n    pane command=\"sh\" {\n        args \"-c\" \"echo \\\"hi\\\"\"\n        cwd \"/tmp/work dir\"\n    }\n}\n"
        );
    }
}
