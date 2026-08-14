// Zellij session wrapper for Tasks Mode v1.6 — every task is a named Zellij session.
// xNaut PTY panes host a task by running `zellij attach` (or create-with-layout on first run).

use std::path::{Path, PathBuf};
use std::process::Command;

/// True if the zellij binary is runnable, on PATH or at the Homebrew path.
pub fn is_installed() -> bool {
    Command::new(zellij_bin())
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Names of currently-running zellij sessions. Parses `zellij list-sessions -n -s`
/// (no formatting, short = name only). Empty vec when none or zellij missing.
pub fn list_sessions() -> Vec<String> {
    let output = match Command::new(zellij_bin())
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
    let output = Command::new(zellij_bin())
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

/// The longest session name zellij 0.44 accepts. Past it zellij rejects the name
/// with the underflow message "must be less than 0 characters" — the cap comes
/// from the unix socket path budget, not from anything configurable.
/// ponytail: a constant, not a probe of the installed zellij. Bump it if a later
/// zellij raises the budget; nothing breaks from being under it.
const MAX_SESSION_NAME: usize = 24;

/// Sanitize an arbitrary task/project name into a valid session name:
/// lowercase, alphanumerics and dashes only, collapse repeats, trim dashes,
/// capped at MAX_SESSION_NAME, fallback "task" if empty.
pub fn session_name(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let mut name: String = out
        .trim_matches('-')
        .chars()
        .take(MAX_SESSION_NAME)
        .collect();
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

/// The zellij binary to run, and the ONLY bare spawn this module may make.
///
/// A launchd-launched app inherits PATH=/usr/bin:/bin:/usr/sbin:/sbin, where a
/// bare `zellij` cannot even spawn. The comment that used to sit here claimed
/// the list commands already fell back to the Homebrew path. `list_sessions`
/// did not, and the cost was 1.14.0 shipping with an Observatory that listed
/// nothing and a sidebar that opened a dead shell instead of the running
/// session, while five sessions were live the whole time (PATH read straight
/// off the running process: ps eww showed /usr/bin:/bin:/usr/sbin:/sbin).
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

/// What a PTY pane needs in order to host an agent in a named zellij session.
#[derive(Debug, serde::Serialize)]
pub struct ZellijOpen {
    /// The sanitized session name actually used. Callers label their tab with
    /// THIS, not the raw name they passed, or the label and the session diverge.
    pub name: String,
    /// The shell command to run in the pane: attach when the session exists,
    /// otherwise create it from a layout that runs `command`.
    pub command: String,
}

/// Attach-or-create for an agent session, replacing the `just -g _zj` recipe
/// (XNAUT-38 Phase 0). The old chain was
/// `sh -c` -> just -> a printf'd KDL layout -> zellij -> `zsh -ic`, and three
/// separate quoting styles were shipped and broke in it. Here the layout is a
/// file written with real escaping and the caller gets back one command.
#[tauri::command]
pub fn zellij_open_command(
    session: String,
    cwd: String,
    command: String,
) -> Result<ZellijOpen, String> {
    let name = session_name(&session);
    // Always a layout: `zellij attach --create` makes a session with a plain
    // shell in it and never runs the agent — the original "empty zellij" bug.
    let layout = write_layout(&name, &cwd, &command)?;
    Ok(ZellijOpen {
        command: launch_command(&name, Some(&layout)),
        name,
    })
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
                l[a..].find(']').map(|b| {
                    l[a + 1..a + b]
                        .trim_start_matches("Created")
                        .trim()
                        .to_string()
                })
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
        out.push(ZellijSessionInfo {
            name,
            created,
            last_active_ms,
            exited,
        });
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
    fn session_name_truncates_to_zellij_cap() {
        // Over the cap zellij rejects the name outright, so this is the one
        // property that has to hold for every project name the UI can produce.
        let raw = "a".repeat(60);
        let name = session_name(&raw);
        assert_eq!(name.len(), MAX_SESSION_NAME);
        assert_eq!(name, "a".repeat(MAX_SESSION_NAME));
        assert!(session_name("cl-nautflow-incident-loop-worktree").len() <= MAX_SESSION_NAME);
    }

    /// The same rule is implemented in JS three times (project-management-panel
    /// `shellSession`, observatory-panel `loadRows`, and the launcher, which gets
    /// the name back from `zellij_open_command`). If they diverge, xNAUT attaches
    /// to and deletes sessions under a name that was never created. These are the
    /// exact strings the JS regex chain
    /// `.toLowerCase().replace(/[^a-z0-9]+/g,'-').replace(/^-+|-+$/g,'').slice(0,24).replace(/-+$/,'')`
    /// produces for real worktree basenames — change one side, this fails.
    #[test]
    fn session_name_matches_the_js_copies() {
        assert_eq!(
            session_name("cl-nautflow-incident-loop"),
            "cl-nautflow-incident-loo"
        );
        assert_eq!(session_name("cl-my_project.v2"), "cl-my-project-v2");
        assert_eq!(session_name("cl-Some Project"), "cl-some-project");
        // Truncation that lands on a dash must not leave a trailing one.
        let raw = format!("{}-b", "a".repeat(23));
        assert_eq!(session_name(&raw), "a".repeat(23));
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
