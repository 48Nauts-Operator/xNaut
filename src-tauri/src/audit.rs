// Audit log — an append-only record of what xNAUT did, and when.
//
// Deliberately append-only JSONL rather than a rewritten document: a log that is
// read, mutated and written back can lose its history to a single bad parse.
// That is not hypothetical here — tasks.json does exactly that (load_tasks turns
// a parse error into an empty Vec, and the next save persists the emptiness).
// Appending a line can drop at most the line being written.
//
// One malformed line therefore never costs the entries around it: reading skips
// what it cannot parse instead of giving up.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// RFC3339, UTC.
    pub ts: String,
    /// Dotted event name, e.g. "project.created".
    pub event: String,
    /// Human-readable one-liner — what you would want to read six months later.
    pub summary: String,
    /// Structured payload; shape varies per event.
    #[serde(default)]
    pub details: serde_json::Value,
}

fn audit_path() -> Option<PathBuf> {
    crate::loop_acceptance::platform_config_dir().map(|p| p.join("xnaut").join("audit.jsonl"))
}

/// Appends one entry. Never returns an error to the caller: an audit write must
/// not be able to fail the thing it is recording.
pub fn record(event: &str, summary: &str, details: serde_json::Value) {
    let Some(path) = audit_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let entry = AuditEntry {
        ts: chrono::Utc::now().to_rfc3339(),
        event: event.to_string(),
        summary: summary.to_string(),
        details,
    };
    let Ok(line) = serde_json::to_string(&entry) else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

/// Most recent entries first. `limit` caps the result; None returns everything.
#[tauri::command]
pub fn audit_list(limit: Option<usize>) -> Result<Vec<AuditEntry>, String> {
    let Some(path) = audit_path() else {
        return Ok(Vec::new());
    };
    let body = match std::fs::read_to_string(&path) {
        Ok(b) => b,
        Err(_) => return Ok(Vec::new()), // no log yet is not an error
    };
    let mut out: Vec<AuditEntry> = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        // Skip unparseable lines rather than failing the read: one bad line
        // must not hide the rest of the history.
        .filter_map(|l| serde_json::from_str::<AuditEntry>(l).ok())
        .collect();
    out.reverse();
    if let Some(n) = limit {
        out.truncate(n);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_line_does_not_hide_the_others() {
        let body = "{\"ts\":\"a\",\"event\":\"one\",\"summary\":\"s\",\"details\":null}\n\
                    not json at all\n\
                    {\"ts\":\"b\",\"event\":\"two\",\"summary\":\"s\",\"details\":null}\n";
        let parsed: Vec<AuditEntry> = body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<AuditEntry>(l).ok())
            .collect();
        assert_eq!(parsed.len(), 2, "the bad line should cost only itself");
        assert_eq!(parsed[0].event, "one");
        assert_eq!(parsed[1].event, "two");
    }
}

#[cfg(test)]
mod acl_tests {
    /// Every #[tauri::command] must appear in permissions/default.toml, or the
    /// frontend gets "Command not found" at runtime with nothing at compile
    /// time to warn you. project_create shipped broken for exactly this reason,
    /// despite the rule being known — so it is a test now, not a habit.
    /// The other direction: every command the FRONTEND calls must exist.
    ///
    /// The ACL test above catches a Rust command nobody allowed. This catches a
    /// call to a command nobody wrote, which fails at runtime with "Command not
    /// found" and is then swallowed by whatever `.catch(() => {})` the call site
    /// happens to have. Four of these were shipping when this test was written
    /// (2026-08-19), found by reading the code for documentation rather than by
    /// anything automated:
    ///
    ///   add_trigger          the frontend's name; main.rs registered create_trigger,
    ///                        so creating a trigger never worked. Both are gone
    ///                        now: triggers live entirely in the frontend
    ///                        (XNAUT-199)
    ///   close_ssh_session    invoked when a session closes, registered nowhere
    ///   ai_analyze_error     the "explain this error" path
    ///   create_shared_session
    ///
    /// Same silent-failure family as calling an undefined `window.*` global.
    #[test]
    fn every_command_the_frontend_calls_exists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri has a parent");
        let main = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
        )
        .expect("main.rs must be readable");
        let handler = main
            .split_once("generate_handler![")
            .and_then(|(_, rest)| rest.split_once(']'))
            .map(|(block, _)| block.to_string())
            .expect("main.rs must have an invoke_handler");

        let mut missing: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(root.join("src/js")).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "js") {
                continue;
            }
            let body = std::fs::read_to_string(&path).unwrap_or_default();
            for piece in body.split("invoke(").skip(1) {
                let piece = piece.trim_start();
                let Some(rest) = piece.strip_prefix('\'') else { continue };
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if name.is_empty() || rest.chars().nth(name.len()) != Some('\'') {
                    continue;
                }
                // Plugin commands are routed by Tauri itself, not by us.
                if name.starts_with("plugin") {
                    continue;
                }
                let registered = handler
                    .split(',')
                    .any(|item| item.trim().rsplit("::").next().map(str::trim) == Some(name.as_str()));
                if !registered && !missing.contains(&name) {
                    missing.push(format!(
                        "{name} (called from {})",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
        }
        missing.sort();
        assert!(
            missing.is_empty(),
            "the frontend calls commands that are not registered in main.rs. They fail at runtime \
             with \"Command not found\" and the error is usually swallowed: {missing:?}"
        );
    }

    #[test]
    fn every_command_is_allowed_by_the_acl() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let acl = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("permissions/default.toml"),
        )
        .expect("permissions/default.toml must be readable");

        let mut missing = Vec::new();
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let body = std::fs::read_to_string(&path).unwrap_or_default();
                let mut lines = body.lines().peekable();
                while let Some(line) = lines.next() {
                    if !line.trim().starts_with("#[tauri::command]") {
                        continue;
                    }
                    // The fn may be one or two lines below (attributes between).
                    for _ in 0..3 {
                        let Some(next) = lines.next() else { break };
                        if let Some(rest) = next.trim().strip_prefix("pub ") {
                            let rest = rest.strip_prefix("async ").unwrap_or(rest);
                            if let Some(name) = rest.strip_prefix("fn ") {
                                let name: String =
                                    name.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                                if !name.is_empty() && !acl.contains(&format!("\"{name}\"")) {
                                    missing.push(name);
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "commands missing from permissions/default.toml (they will fail at runtime with \
             \"Command not found\"): {missing:?}"
        );
    }

    /// The third direction, and the one that made XNAUT-265 necessary: an ACL
    /// entry for a command that no longer exists.
    ///
    /// `every_command_is_allowed_by_the_acl` walks Rust -> toml and
    /// `every_command_the_frontend_calls_exists` walks JS -> main.rs. Neither
    /// can see a name that is ONLY in default.toml. That is what deleting a
    /// command half-way leaves behind, and it is invisible: a grant for nothing
    /// costs no compile error and no runtime error, it just quietly widens the
    /// ACL and lies to the next reader about what the app can do. Removing the
    /// eleven dead commands in XNAUT-265 (share/join/unshare_session, the five
    /// pm_* and the three plow_*) meant editing main.rs and default.toml in
    /// lockstep; this is what says so when someone edits only one of them.
    ///
    /// Scope: only the flat name lists. Permission-block identifiers are
    /// `allow-*` and are checked by main.rs's acl_audit instead.
    #[test]
    fn every_command_named_in_the_acl_is_still_registered() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let acl = std::fs::read_to_string(root.join("permissions/default.toml"))
            .expect("permissions/default.toml must be readable");
        let main = std::fs::read_to_string(root.join("src/main.rs"))
            .expect("main.rs must be readable");
        let handler = main
            .split_once("generate_handler![")
            .and_then(|(_, rest)| rest.split_once(']'))
            .map(|(block, _)| block.to_string())
            .expect("main.rs must have an invoke_handler");

        // Every bare "name" the toml grants, from both `commands.allow = [...]`
        // rows and the allow-all-commands list.
        let mut granted: Vec<String> = Vec::new();
        for raw in acl.split('"').skip(1).step_by(2) {
            let name = raw.trim();
            // Permission identifiers, descriptions and TOML keys are not commands.
            if name.is_empty()
                || name.starts_with("allow-")
                || name.starts_with("deny-")
                || !name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                continue;
            }
            if !granted.contains(&name.to_string()) {
                granted.push(name.to_string());
            }
        }
        assert!(
            granted.len() > 300,
            "found only {} granted command names; the scraper stopped matching",
            granted.len()
        );

        let stale: Vec<&String> = granted
            .iter()
            .filter(|name| {
                !handler.split(',').any(|item| {
                    item.trim().rsplit("::").next().map(str::trim) == Some(name.as_str())
                })
            })
            .collect();
        assert!(
            stale.is_empty(),
            "permissions/default.toml grants commands that main.rs no longer registers. \
             The grant is dead weight and reads as a feature that exists: {stale:?}"
        );
    }

    /// The mobile mirror only works while the PTY reader keeps calling the tee
    /// (XNAUT-201). spawn_pty_reader cannot be driven from a test: it needs a
    /// real AppHandle, and the mock runtime is a different type than the Wry
    /// one the signature takes. So the call site is guarded here instead, in
    /// the same source-reading spirit as the two tests above. The tee itself is
    /// covered by pty::tests::a_registered_session_can_be_mirrored_to_the_phone.
    #[test]
    fn the_pty_reader_still_tees_every_read_to_the_mobile_tap() {
        let pty = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pty.rs"),
        )
        .expect("pty.rs must be readable");
        assert!(
            pty.contains("block_on(tee_output("),
            "spawn_pty_reader no longer tees reads: the phone would hold an open socket \
             and see nothing"
        );
    }
}
