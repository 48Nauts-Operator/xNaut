// What the agents have been doing, in one list, with the clock running.
//
// André, 2026-08-16: "We should also have a NautBot Audit Log on the right pane
// so we can clearly see the state of dispatch, maybe even with timer how long
// the run. If I got 20 Agents running its messy and unclear."
//
// Everything that happens in the loop is already recorded SOMEWHERE — a ticket
// moved, a thread appeared, a session started, a plan ticked — and all of those
// are different places with different shapes. With one agent that is merely
// annoying; with twenty it means there is no answer to "what is happening right
// now" short of opening twenty panes.
//
// So: one append-only line per event, and an elapsed time computed from the
// dispatch that opened the run. Append-only on purpose — an audit log that can
// be rewritten answers a different question from the one being asked.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub at: String,
    /// dispatched | planned | step | handed_back | blocked | reviewed | failed
    pub kind: String,
    pub agent: String,
    #[serde(default)]
    pub ticket: String,
    #[serde(default)]
    pub detail: String,
    /// Seconds since the dispatch that opened this ticket's run, filled in when
    /// the log is read rather than stored — the elapsed time of a run that is
    /// still going changes every second, and a stored number would be a lie
    /// from the moment it was written.
    #[serde(default)]
    pub elapsed_secs: Option<i64>,
}

fn path() -> PathBuf {
    if let Ok(path) = std::env::var("XNAUT_LEDGER_PATH") {
        return PathBuf::from(path);
    }
    dirs::config_dir()
        .map(|dir| dir.join("xnaut").join("agent-ledger.jsonl"))
        .unwrap_or_else(|| PathBuf::from("agent-ledger.jsonl"))
}

/// Append one event. Never fails a caller: a run must not die because its
/// audit line could not be written.
pub fn record(kind: &str, agent: &str, ticket: &str, detail: &str) {
    let entry = Entry {
        at: chrono::Utc::now().to_rfc3339(),
        kind: kind.to_string(),
        agent: agent.trim().trim_start_matches('@').to_lowercase(),
        ticket: ticket.trim().to_uppercase(),
        detail: detail.trim().chars().take(300).collect(),
        elapsed_secs: None,
    };
    let Ok(line) = serde_json::to_string(&entry) else {
        return;
    };
    let file = path();
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut handle) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = writeln!(handle, "{line}");
    }
}

fn read_all() -> Vec<Entry> {
    std::fs::read_to_string(path())
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Entry>(line).ok())
        .collect()
}

/// The most recent events, newest first, with elapsed time filled in.
///
/// Elapsed runs from the ticket's last `dispatched` to either its last
/// terminal event or, for work still in flight, to now. That is the number the
/// question is actually asking: how long has this been going.
#[tauri::command]
pub fn ledger_recent(limit: Option<usize>) -> Vec<Entry> {
    let entries = read_all();
    let now = chrono::Utc::now();

    let started: std::collections::HashMap<String, chrono::DateTime<chrono::Utc>> = entries
        .iter()
        .filter(|entry| entry.kind == "dispatched" && !entry.ticket.is_empty())
        .filter_map(|entry| {
            chrono::DateTime::parse_from_rfc3339(&entry.at)
                .ok()
                .map(|at| (entry.ticket.clone(), at.with_timezone(&chrono::Utc)))
        })
        .collect();

    let mut recent: Vec<Entry> = entries
        .into_iter()
        .rev()
        .take(limit.unwrap_or(40).clamp(1, 500))
        .map(|mut entry| {
            if let Some(start) = started.get(&entry.ticket) {
                let at = chrono::DateTime::parse_from_rfc3339(&entry.at)
                    .map(|value| value.with_timezone(&chrono::Utc))
                    .unwrap_or(now);
                // A run still going is measured to NOW; one that ended is
                // measured to when it ended.
                let end = if matches!(entry.kind.as_str(), "dispatched" | "planned" | "step") { now } else { at };
                entry.elapsed_secs = Some((end - *start).num_seconds().max(0));
            }
            entry
        })
        .collect();
    recent.dedup_by(|a, b| a.at == b.at && a.kind == b.kind && a.ticket == b.ticket);
    recent
}

/// Points XNAUT_LEDGER_PATH at a fresh scratch file for the duration of a
/// test, holding a process-wide lock while it does. The path comes from an env
/// var, which is process-global: two tests writing ledgers at once would read
/// each other's lines. Shared with every module that asserts on ledger output,
/// so they queue behind one mutex rather than each inventing their own.
#[cfg(test)]
pub(crate) fn scratch(name: &str) -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let guard = LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let file = std::env::temp_dir().join(format!("xnaut-ledger-{}-{name}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&file);
    std::env::set_var("XNAUT_LEDGER_PATH", &file);
    guard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_event_leads_and_a_running_job_is_timed_to_now() {
        let _guard = scratch("order");
        record("dispatched", "@claude", "xnaut-82", "project templates");
        record("planned", "claude", "XNAUT-82", "4 steps");
        record("step", "claude", "XNAUT-82", "read the scaffold");

        let recent = ledger_recent(Some(10));
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].kind, "step", "newest first");
        // Every entry for a dispatched ticket carries how long it has been
        // going, which is the question being asked.
        assert!(recent.iter().all(|entry| entry.elapsed_secs.is_some()), "{recent:?}");
        assert_eq!(recent[0].agent, "claude", "the @ is not part of a handle");
        assert_eq!(recent[0].ticket, "XNAUT-82", "ids are compared upper-cased");
    }

    #[test]
    fn an_event_for_a_ticket_nobody_dispatched_has_no_elapsed() {
        // A chat-side action is real and belongs in the log; inventing a
        // duration for it would be inventing a run that never started.
        let _guard = scratch("orphan");
        record("reviewed", "nautbot", "XNAUT-999", "closed by hand");
        let recent = ledger_recent(None);
        assert_eq!(recent.len(), 1);
        assert!(recent[0].elapsed_secs.is_none());
    }

    #[test]
    fn a_broken_line_does_not_take_the_log_with_it() {
        // It is append-only and written from several places; one bad write
        // must not blind the whole pane.
        let _guard = scratch("corrupt");
        record("dispatched", "claude", "XNAUT-1", "one");
        let file = path();
        let mut handle = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(handle, "{{not json").unwrap();
        drop(handle);
        record("handed_back", "claude", "XNAUT-1", "two");

        let recent = ledger_recent(None);
        assert_eq!(recent.len(), 2, "the readable lines still read: {recent:?}");
    }

    #[test]
    fn the_log_is_append_only() {
        // An audit log that can be rewritten answers a different question from
        // the one being asked.
        let _guard = scratch("append");
        record("dispatched", "claude", "XNAUT-5", "first");
        let first = std::fs::read_to_string(path()).unwrap();
        record("handed_back", "claude", "XNAUT-5", "second");
        let second = std::fs::read_to_string(path()).unwrap();
        assert!(second.starts_with(&first), "an earlier line was rewritten");
    }
}
