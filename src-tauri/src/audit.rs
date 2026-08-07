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
