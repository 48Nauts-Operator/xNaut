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
    dirs::config_dir().map(|p| p.join("xnaut").join("audit.jsonl"))
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
}
