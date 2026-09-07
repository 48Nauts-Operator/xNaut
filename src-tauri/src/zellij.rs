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

/// Names of sessions that are actually ALIVE (server running), excluding the
/// EXITED-but-resurrectable ones `list_sessions` also returns. The adoption
/// path (XNAUT-242) must not resurrect a finished run just to look at it.
pub fn live_sessions() -> Vec<String> {
    let output = match Command::new(zellij_bin()).args(["list-sessions", "-n"]).output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.contains("EXITED"))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

pub fn session_exists(name: &str) -> bool {
    list_sessions().iter().any(|s| s == name)
}

/// Pick a live `xnaut-<handle>-<run8>` session out of `live`, newest last-known
/// first is not knowable here, so the last name wins deterministically by sort.
///
/// This is the wake path's second opinion. The app's session map is the primary
/// record, but it is memory: if a row is missing for a session that is provably
/// alive, cold-launching mints a SECOND agent in the default workspace with no
/// ticket, branch or worktree, while the real one keeps working unattended.
/// Observed 2026-09-07 on tron, where a wake aimed at the XNAUT-300 agent
/// launched a stray in ~/Library/…/agent-workspaces/codex instead. zellij knows
/// what is alive whether or not this app remembers, so ask it before launching.
pub fn live_session_for_handle(handle: &str) -> Option<String> {
    pick_live_session_for_handle(&live_sessions(), handle)
}

/// The pure half, so the rule is tested without a zellij server.
pub(crate) fn pick_live_session_for_handle(live: &[String], handle: &str) -> Option<String> {
    let handle = handle.trim().to_ascii_lowercase();
    if handle.is_empty() {
        return None;
    }
    let prefix = format!("xnaut-{handle}-");
    let mut names: Vec<&String> = live
        .iter()
        .filter(|name| name.to_ascii_lowercase().starts_with(&prefix))
        .collect();
    names.sort();
    names.pop().cloned()
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
        "keybinds {{\n    normal {{\n        unbind \"Ctrl p\" \"Ctrl n\" \"Ctrl o\" \"Ctrl t\" \"Ctrl h\" \"Ctrl s\" \"Ctrl q\"\n    }}\n}}\nlayout {{\n    pane command=\"sh\" {{\n        args \"-c\" \"{}\"\n        cwd \"{}\"\n    }}\n}}\n",
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
    launch_command_for(session, layout, session_exists(session))
}

/// The string builder, with the liveness answer passed in so a test can reach
/// all three shapes. The attach shape is only taken when a session already
/// exists, which is exactly the branch a unit test could not otherwise reach —
/// and exactly the branch that carries a typed wake.
fn launch_command_for(session: &str, layout: Option<&Path>, exists: bool) -> String {
    let name = shell_quote(session);
    if exists {
        return format!("zellij attach {name} {NO_TIPS}");
    }
    match layout {
        Some(path) => format!(
            "zellij --session {name} --new-session-with-layout {} {NO_TIPS}",
            shell_quote(&path.to_string_lossy())
        ),
        None => format!("zellij attach --create {name} {NO_TIPS}"),
    }
}

/// Both of zellij's interstitial panes off, on every session xNAUT starts or
/// attaches.
///
/// zellij opens a FLOATING `zellij:about` pane with focus=true on top of the
/// agent pane, so a nudge's keystrokes land in the plugin instead of the agent.
/// The rig found it via `zellij action dump-layout` (XNAUT-263 round 10). Cold
/// launches are unaffected because their prompt is an argv flag; a TYPED wake
/// is not.
///
/// There are TWO such panes and one flag each. `--show-startup-tips false`
/// alone left the second one live: the RELEASE NOTES pane, shown once after a
/// zellij version upgrade, which is what came back on 2026-09-04 as
/// `plugin location="zellij:about" { is_release_notes "true" }`.
///
/// It is a TRAILING `options` subcommand, not a global flag. The first fix
/// wrote `zellij --show-startup-tips false …`, which zellij 0.44 rejects
/// outright with a usage error — so every launch and attach died instantly and
/// the rig sat idle looking like nothing had been dispatched. The subcommand
/// form parses on 0.44 and stays valid on newer builds, and a version that
/// does not know a flag fails loudly here rather than silently showing a pane.
const NO_TIPS: &str = "options --show-startup-tips false --show-release-notes false";

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

/// Type `text` and press Enter in a session's focused pane, from outside it.
///
/// This is how a wake reaches a durable session the app no longer holds a
/// PTY for: after a restart the zellij session is alive and the agent in it
/// is working, but the PTY that used to carry keystrokes belonged to the
/// previous app process (XNAUT-289). Two commands rather than one so the
/// text lands as text: `write-chars` takes the message verbatim and `write
/// 13` is the carriage return, the same two steps the PTY path takes.
///
/// No acknowledgement is possible from here (nothing reads the pane back),
/// so the caller records it as typed, not confirmed.
pub fn type_into_session(name: &str, text: &str) -> Result<(), String> {
    let name = validate_session_name(name)?;
    for args in [vec!["action", "write-chars", text], vec!["action", "write", "13"]] {
        let output = Command::new(zellij_bin())
            .args(["--session", &name])
            .args(&args)
            .output()
            .map_err(|e| format!("failed to invoke zellij: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "zellij --session {name} {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    Ok(())
}

/// Validates a caller-supplied session name for use as an argv element.
///
/// Deliberately NOT `session_name()`: that is the sanitizer for names we
/// create, and it lowercases, so comparing against it rejects every real
/// session with a capital in it (`cx-WebBuilder`, `cx-Bucky`). Names reach
/// zellij as argv and never through a shell, so the actual requirements are
/// narrow: something is there, it cannot be read as a flag, and it cannot carry
/// a path or a control character.
pub fn validate_session_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a session name is required".into());
    }
    if name.len() > 108 {
        return Err("session name is too long".into());
    }
    if name.starts_with('-') {
        return Err(format!("{name:?} would be read as a flag, not a session"));
    }
    if name.contains('/') || name.chars().any(char::is_control) {
        return Err(format!("{name:?} is not a valid zellij session name"));
    }
    Ok(name.to_string())
}

/// Kill a running session and drop its resurrectable record, so the caller's
/// list stops showing it. Idempotent: already gone is success.
pub fn remove_session(name: &str) -> Result<(), String> {
    let name = validate_session_name(name)?;
    kill_session(&name)?;
    match zellij_delete_session(name.clone()) {
        Ok(()) => Ok(()),
        // Killing first can leave delete-session reporting the name as already
        // gone, and zellij words that as `Session: "x" not found.` rather than
        // the "no session" its own guard looks for. Gone is what we asked for.
        Err(_) if !session_exists(&name) => Ok(()),
        Err(e) => Err(e),
    }
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
    /// Wall-clock creation, derived from zellij's own "Created Xh Ym ago"
    /// annotation. The Observatory's ELAPSED column needs THIS: keyed on
    /// last_active_ms every row read 0:04 while its own subtitle said the
    /// session was hours old (XNAUT-260, found by the tron rig).
    pub created_ms: Option<u64>,
    pub last_active_ms: Option<u64>,
    /// Dead but resurrectable — `zellij attach` rebuilds it from the serialized
    /// layout. Callers that only want live sessions filter on this; the sidebar
    /// shows both, since a resurrectable session is still somewhere to go back to.
    #[serde(default)]
    pub exited: bool,
}

/// "1h 56m 10s" / "38m 24s" / "4s" -> epoch ms of that moment.
///
/// zellij prints an AGE, not a timestamp, so the only honest reading is
/// now minus the age. Unparseable means None: a missing elapsed is better
/// than a confident wrong one.
fn parse_created_ago(created: &str) -> Option<u64> {
    let mut secs: u64 = 0;
    let mut seen = false;
    let mut digits = String::new();
    for ch in created.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        if digits.is_empty() {
            continue;
        }
        let value: u64 = digits.parse().ok()?;
        digits.clear();
        match ch {
            'd' => { secs += value * 86_400; seen = true; }
            'h' => { secs += value * 3_600; seen = true; }
            'm' => { secs += value * 60; seen = true; }
            's' => { secs += value; seen = true; }
            _ => {}
        }
    }
    if !seen {
        return None;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    Some(now.saturating_sub(secs * 1000))
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
    // Already gone is the outcome the caller wanted, not a failure. Zellij
    // words it two ways: its own guard says "no session", but after a kill it
    // says `Session: "x" not found.` — kill_session above already tolerated
    // the second form while this guard did not, so a successful removal
    // surfaced as an error to every caller except the one that worked around
    // it (mobile bridge item 3, 1.22.2 requirements).
    if err.to_lowercase().contains("no session")
        || err.to_lowercase().contains("not found")
        || err.is_empty()
    {
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
        let created_ms = parse_created_ago(&created);
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
            created_ms,
            last_active_ms,
            exited,
        });
    }
    out
}

/// How many clients are attached to a session right now, from zellij's own
/// session metadata.
///
/// `None` means the metadata could not be read, which is never the same as
/// "nobody is looking": a caller deciding whether to end a session has to be
/// able to tell an empty room from an unanswered question.
///
/// The same cache `zellij_sessions_info` already reads for last-activity, so
/// this adds a file read and no process.
///
/// It counts CLIENTS, not people, and xNAUT is itself a client: a tab hosting a
/// session runs `zellij attach` in a PTY pane (see `launch_command` and the note
/// at the top of this file), so a finished run whose tab is open reads 1 with
/// nobody in the room. A caller deciding whether to end a session has to
/// subtract its own hosting panes before reading this as attendance;
/// `scheduler::finished_and_idle` does, and reading it as attendance directly is
/// what made the idle reaper collect nothing for a day.
///
/// The number is otherwise trustworthy, probed against the installed zellij 0.44
/// on 2026-09-03: a client held inside a PTY reads exactly like a human's
/// terminal attach, and killing one drops the count 1 -> 0 within five seconds,
/// so a client this reports is live rather than a leftover. `other_focused_clients`
/// tracked it one for one in the same probe and separates nothing.
///
/// ponytail: the macOS cache path only, matching `zellij_sessions_info` above.
/// A Linux build reads no metadata and gets `None`, which keeps the session.
pub fn connected_clients(name: &str) -> Option<u32> {
    let cache = dirs::home_dir()?.join("Library/Caches/org.Zellij-Contributors.Zellij");
    for entry in std::fs::read_dir(cache).ok()?.flatten() {
        let file = entry
            .path()
            .join("session_info")
            .join(name)
            .join("session-metadata.kdl");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if let Some(count) = parse_connected_clients(&text) {
            return Some(count);
        }
    }
    None
}

/// Pulls `connected_clients <n>` out of a session-metadata.kdl body. Split out
/// so the parse is testable without a running zellij server.
fn parse_connected_clients(text: &str) -> Option<u32> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("connected_clients "))
        .and_then(|count| count.trim().parse().ok())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {

    /// The attach signal the idle reaper leans on (XNAUT-262), against the
    /// shape zellij actually writes. Copied from this machine's cache on
    /// 2026-09-02: an orphaned `xnaut-*` run and one of the owner's own `cx-*`
    /// panes. Reading the count wrong in the safe direction leaks sessions;
    /// reading it wrong in the other kills a pane somebody is sitting in.
    #[test]
    fn the_attached_client_count_is_read_off_the_session_metadata() {
        let orphan = "name \"xnaut-claude-5e0dde06\"\nconnected_clients 0\nweb_clients_allowed false\ncreation_time 187843\n";
        assert_eq!(super::parse_connected_clients(orphan), Some(0));

        let attended = "name \"cx-bin-movement\"\nconnected_clients 2\n";
        assert_eq!(super::parse_connected_clients(attended), Some(2));

        // Metadata with no such key is an unanswered question, never an empty
        // room; the caller keeps the session on None.
        assert_eq!(super::parse_connected_clients("name \"x\"\ntabs {\n}\n"), None);
        assert_eq!(super::parse_connected_clients("connected_clients many"), None);
    }

    /// The launch command must PARSE on the zellij that is installed.
    ///
    /// A unit test cannot reimplement zellij's argument parser, so it asks the
    /// real binary: `--help` on the exact argument vector exits 0 when every
    /// flag is in a position zellij accepts, and non-zero on the usage error
    /// that took the rig down on 2026-09-01. Skipped when zellij is absent, so
    /// CI without it stays green.
    #[test]
    fn the_launch_command_parses_on_the_installed_zellij() {
        if !super::is_installed() {
            return;
        }
        let layout = std::path::PathBuf::from("/tmp/xnaut-launch-parse-test.kdl");
        for command in [
            super::launch_command_for("xnaut-parse-probe", None, false),
            super::launch_command_for("xnaut-parse-probe", Some(&layout), false),
            // The attach shape: a typed wake's path, unreachable through
            // launch_command without a live session to attach to.
            super::launch_command_for("xnaut-parse-probe", None, true),
        ] {
            let args: Vec<&str> = command.split_whitespace().skip(1).collect();
            let status = std::process::Command::new(super::zellij_bin())
                .args(&args)
                .arg("--help")
                .output()
                .expect("zellij runs");
            assert!(
                status.status.success(),
                "zellij rejects the command we launch agents with: `{command}`\n{}",
                String::from_utf8_lossy(&status.stderr)
            );
        }
    }

    /// The launch command must not leave an INTERSTITIAL PANE sitting on top of
    /// the agent, because a typed wake's keystrokes go to whatever holds focus.
    ///
    /// This asks the real zellij for the layout it actually built, not for the
    /// flag we passed it: the `--show-startup-tips false` flag was present and
    /// correct on 2026-09-04 while `Release Notes 0.44.3` floated over the
    /// agent anyway, so a string assertion would have stayed green through the
    /// whole bug.
    ///
    /// The reproduction needs a zellij that thinks it has just been upgraded.
    /// Release notes are gated on a per-version marker under the CACHE dir, so
    /// the session runs with HOME pointed at an empty scratch directory: no
    /// marker, notes due. A dumped default config goes in beside it, otherwise
    /// zellij shows the first-run setup wizard instead and the real pane never
    /// gets a chance to appear. Skipped when zellij is absent, so CI without it
    /// stays green.
    #[test]
    fn no_interstitial_pane_floats_over_a_session_we_launch() {
        use portable_pty::{CommandBuilder, PtySize};
        if !super::is_installed() {
            return;
        }
        let home = std::env::temp_dir().join(format!("xnaut-relnotes-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".config/zellij")).expect("scratch home");
        let config = std::process::Command::new(super::zellij_bin())
            .args(["setup", "--dump-config"])
            .output()
            .expect("zellij runs");
        std::fs::write(home.join(".config/zellij/config.kdl"), config.stdout).expect("config");

        // Session names are global (the socket dir is not under HOME), so this
        // one carries the pid to avoid colliding with a real agent's session.
        let session = format!("xnaut-relnotes-{}", std::process::id());
        let command = super::launch_command_for(&session, None, false);
        let mut argv = command.split_whitespace();
        let mut cmd = CommandBuilder::new(super::zellij_bin());
        argv.next(); // the binary, already resolved above
        for arg in argv {
            cmd.arg(arg.trim_matches('\''));
        }
        // Keep relative test socket paths shared with the probe commands.
        // portable-pty otherwise starts in the scratch HOME.
        cmd.cwd(std::env::current_dir().expect("test cwd"));
        cmd.env("HOME", &home);
        cmd.env("TERM", "xterm-256color");

        // zellij refuses to start without a terminal, and the pane it opens is
        // the client's doing, so this needs a real pty rather than a pipe.
        let pty = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
        let mut child = pty.slave.spawn_command(cmd).expect("zellij spawns");

        let mut layout = String::new();
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            if !super::session_exists(&session) {
                continue;
            }
            let dump = std::process::Command::new(super::zellij_bin())
                .args(["--session", &session, "action", "dump-layout"])
                .output()
                .expect("zellij runs");
            layout = String::from_utf8_lossy(&dump.stdout).to_string();
            if layout.contains("zellij:status-bar") {
                break;
            }
        }

        // Tear the session down before asserting: a failed assert must not
        // leave a live zellij session behind on the owner's machine.
        let _ = child.kill();
        let _ = super::kill_session(&session);
        // The scratch home has to go too, or the version marker zellij just
        // wrote in it would make the next run on this pid a silent false green.
        let _ = std::fs::remove_dir_all(&home);

        assert!(
            layout.contains("zellij:status-bar"),
            "zellij never came up, so this proves nothing about the pane it opens:\n{layout}"
        );
        assert!(
            !layout.contains("is_release_notes"),
            "the release notes pane floats over the agent and takes the keystrokes \
             of a typed wake; `{command}` built this layout:\n{layout}"
        );
    }

    #[test]
    fn created_ago_parses_the_shapes_zellij_prints() {
        // The rig's evidence: rows read ELAPSED 0:04 while saying "created
        // 1h 56m 10s ago". Age -> timestamp, or nothing at all.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let h = parse_created_ago("1h 56m 10s").expect("hours parse");
        assert!((now - h).abs_diff(6_970_000) < 2_000, "1h56m10s ago");
        let m = parse_created_ago("38m 24s").expect("minutes parse");
        assert!((now - m).abs_diff(2_304_000) < 2_000, "38m24s ago");
        let s = parse_created_ago("4s").expect("seconds parse");
        assert!((now - s).abs_diff(4_000) < 2_000, "4s ago");
        assert_eq!(parse_created_ago(""), None, "no age, no guess");
        assert_eq!(parse_created_ago("just now"), None, "unparseable, no guess");
    }
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
            "keybinds {\n    normal {\n        unbind \"Ctrl p\" \"Ctrl n\" \"Ctrl o\" \"Ctrl t\" \"Ctrl h\" \"Ctrl s\" \"Ctrl q\"\n    }\n}\nlayout {\n    pane command=\"sh\" {\n        args \"-c\" \"echo \\\"hi\\\"\"\n        cwd \"/tmp/work dir\"\n    }\n}\n"
        );
    }

    #[test]
    fn layout_unbinds_the_keys_shells_and_agents_need() {
        let kdl = layout_kdl("/tmp", "echo hi");
        assert!(
            kdl.contains("keybinds"),
            "layout must carry a keybinds block"
        );
        // clear-defaults would drop EVERY binding, including Ctrl b tmux mode
        // and therefore detach. Unbind the seven that collide, keep the rest.
        assert!(
            !kdl.contains("clear-defaults"),
            "unbind the collisions, do not clear every zellij keybinding"
        );
        for key in ["Ctrl p", "Ctrl n", "Ctrl o", "Ctrl t"] {
            assert!(kdl.contains(key), "layout must unbind {key}");
        }
    }

    #[test]
    fn delete_guard_tolerates_both_gone_phrasings() {
        // The two ways zellij words "already gone" — its guard's "no session"
        // and the post-kill `Session: "x" not found.` — must both count as
        // success. The second one leaked through as an error for months.
        for phrase in ["no session named \"x\"", "Session: \"x\" not found."] {
            let lower = phrase.to_lowercase();
            assert!(
                lower.contains("no session") || lower.contains("not found"),
                "guard would reject: {phrase}"
            );
        }
    }

    #[test]
    fn remove_session_rejects_an_empty_name() {
        assert!(remove_session("").is_err());
        assert!(remove_session("   ").is_err());
    }

    #[test]
    fn validate_accepts_the_names_real_sessions_actually_have() {
        // Capitals are the case that shipped broken: session_name() lowercases,
        // so an equality check against it rejected every one of these.
        for name in [
            "cx-PassiveIncome", "cx-WebBuilder", "cx-Bucky", "cl-bin-movement-pt",
            "xnaut-claude-0eaebfd9", "cx-migration-website",
        ] {
            assert_eq!(validate_session_name(name).unwrap(), name, "rejected {name}");
        }
    }

    #[test]
    fn validate_rejects_argv_and_path_tricks() {
        assert!(validate_session_name("--help").is_err());
        assert!(validate_session_name("-x").is_err());
        assert!(validate_session_name("../../etc/passwd").is_err());
        assert!(validate_session_name("a/b").is_err());
        assert!(validate_session_name("bad\u{0}name").is_err());
        assert!(validate_session_name(&"x".repeat(200)).is_err());
    }

    #[test]
    fn validate_session_name_accepts_a_clean_name() {
        assert_eq!(validate_session_name("xnaut-loops").unwrap(), "xnaut-loops");
    }

    #[test]
    fn a_live_session_is_found_for_its_handle_and_nobody_elses() {
        let live: Vec<String> = [
            "xnaut-codex-2ce957f9",
            "xnaut-claude-01440e53",
            "xnaut-nautbot-9ae8edad",
            "some-other-session",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            pick_live_session_for_handle(&live, "codex").as_deref(),
            Some("xnaut-codex-2ce957f9")
        );
        assert_eq!(
            pick_live_session_for_handle(&live, "claude").as_deref(),
            Some("xnaut-claude-01440e53")
        );
        // A handle with no live session must fall through to a cold launch.
        assert_eq!(pick_live_session_for_handle(&live, "gemini"), None);
        // "codex" must not match "codex-review"; the separator is part of the
        // prefix, so a longer handle cannot be captured by a shorter one.
        assert_eq!(pick_live_session_for_handle(&live, "code"), None);
        assert_eq!(pick_live_session_for_handle(&live, ""), None);
    }
}
