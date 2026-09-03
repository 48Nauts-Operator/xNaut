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
//
// ---- why the ticket-to-evidence link lives HERE and not on the record ------
//
// The ledger and the evidence chain did not join. This side carried a ticket
// and no session; `evidence.rs` is keyed by session and carries no ticket. So
// "what happened to XNAUT-73" was answerable and "which commands did the agent
// run for it" was not, short of opening a transcript.
//
// The link goes on the ledger row, for one reason that decides it: the
// evidence chain is HASH-LINKED and this file is not. Every evidence record
// hashes over its own canonical bytes and every `prev_hash` is verified, so a
// new field there changes the canonical form of every record written after it
// and leaves a chain whose shape depends on when a row was written. This file
// is plain JSONL with no hashes; a new field costs an extra key and nothing
// else. 648 records verify on this machine today and they still verify after
// this change, because this change does not touch them.
//
// A wrong link is impossible because there is no code path that INFERS one.
// `session` is only ever written by `record_in_session`, from an id the caller
// already holds; `record` writes it empty. Nothing matches on timestamps, on
// the agent handle, or on the working directory — and it must not, because
// none of those is unique over time. Real proof from André's own machine: on
// 2026-08-31 the handle `claude`, in the same workspace directory, ran
// XNAUT-58 at 11:18 and XNAUT-73 at 11:46. Any nearest-in-time or same-handle
// join maps both sessions to both tickets and looks confident doing it.
//
// The cost of refusing to infer is that a row nobody attributed stays
// unjoined. That is the correct outcome and it is rendered as such rather than
// as an empty list; see `evidence::ticket_evidence`.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub at: String,
    /// What happened, as one of the kinds something in this tree actually
    /// emits. The list this doc used to carry (planned, step, handed_back,
    /// blocked, reviewed, failed) named six kinds with no `record` call
    /// anywhere behind them; they are older vocabulary that still sits in
    /// André's on-disk ledger and had become a promise the code did not keep.
    ///
    /// The real set, by the module that writes it:
    ///   nudge.rs   dispatched, nudged, wake_skipped_busy, wake_failed,
    ///              wake_unacknowledged, launch_not_durable
    ///   sweep.rs   sweep_failed, sweep_paused, sweep_idle, sweep_retry,
    ///              sweep_retry_failed, sweep_retry_dropped, sweep_gave_up,
    ///              sweep_verify, sweep_verify_failed, sweep_dispatch,
    ///              sweep_refused
    ///   scheduler.rs  automation_fired, automation_ran, automation_failed,
    ///                 automation_reaped, idle_reaped
    ///   housekeeper.rs  disk_pressure, worktree_reclaimed, cache_reclaimed
    ///   veto.rs    conflict, refused, asked
    ///   status.rs  adopted
    ///   sandbox_verify.rs  verify_orphaned
    ///
    /// A reader is free to write any string; this is the vocabulary, not a
    /// validator. Keep it honest when a kind is added or retired.
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
    /// The evidence chain's session id for the work this row describes, when
    /// the caller held it. THE JOIN between "what happened to this ticket" and
    /// "which tool calls the agent made" (XNAUT-213 left the two sides unable
    /// to point at each other).
    ///
    /// Empty means UNKNOWN, never "none". Only a caller that has the harness
    /// session id in hand may fill it, via `record_in_session`; `record` writes
    /// it empty and there is deliberately no path that derives it from
    /// anything else. See the module note on why this side carries the link.
    #[serde(default)]
    pub session: String,
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
    record_in_session(kind, agent, ticket, detail, "");
}

/// Append one event that names the evidence session its work was recorded
/// under, so the ticket and the tool calls can be read as one story.
///
/// `session` must be the harness session id the evidence chain is keyed by,
/// held by the caller. Pass "" when it is genuinely not known; that is honest
/// and it reads as unjoinable downstream. Never pass a PTY session id, a
/// zellij session name, or a launch run id: those are different identifier
/// spaces, and a value from the wrong space is worse than no value, because it
/// joins to nothing while looking exactly like a link that does.
///
/// ponytail: THE CEILING. No production caller passes a session id yet, so
/// every row the running app writes is honestly unjoinable rather than
/// wrongly joined. Two things stand in the way and both are outside this
/// module:
///
///   1. `nudge.rs` writes its dispatch rows with an empty TICKET as well
///      (`record(kind, handle, "", message)`, three call sites). The ticket
///      only rides in the free-text detail, which is why two of the three
///      real `dispatched` rows on this machine say ticket "" while their
///      detail reads "Start with XNAUT-73".
///   2. The wake path launches with `conversation_mode: false`, and that
///      branch of `agents.rs::agent_profile_launch` returns
///      `conversation_id: None` and never passes `--session-id`. The harness
///      mints its own uuid and xNAUT never learns it. The conversation path
///      already mints one (`agents.rs`, the "claude" arm of
///      `build_conversation_launch`); doing the same on the interactive path
///      is what makes this field fillable, and it changes the argv of every
///      wake, so it is a deliberate change with its own test, not a drive-by.
///
/// Until then the join is exercised by tests and by hand, and the panel says
/// "unattributed" instead of guessing. That is the correct failure.
pub fn record_in_session(kind: &str, agent: &str, ticket: &str, detail: &str, session: &str) {
    let entry = Entry {
        at: chrono::Utc::now().to_rfc3339(),
        kind: kind.to_string(),
        agent: agent.trim().trim_start_matches('@').to_lowercase(),
        ticket: ticket.trim().to_uppercase(),
        detail: detail.trim().chars().take(300).collect(),
        elapsed_secs: None,
        session: session.trim().to_string(),
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

/// Every row this ticket wrote, oldest first, whatever it says.
///
/// Returned even when none of them names a session, because "the ledger has
/// nine rows for this ticket and not one of them was attributed" and "this
/// ticket does not exist" are different facts and the caller has to be able to
/// tell them apart.
pub(crate) fn rows_for_ticket(ticket: &str) -> Vec<Entry> {
    let wanted = ticket.trim().to_uppercase();
    if wanted.is_empty() {
        return Vec::new();
    }
    read_all().into_iter().filter(|entry| entry.ticket == wanted).collect()
}

/// Every session id any ticket has claimed, for spotting the evidence nobody
/// attributed. Distinct and sorted.
pub(crate) fn claimed_sessions() -> std::collections::BTreeSet<String> {
    read_all()
        .into_iter()
        .filter(|entry| !entry.session.is_empty() && !entry.ticket.is_empty())
        .map(|entry| entry.session)
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
                //
                // `planned` and `step` used to sit alongside `dispatched` here
                // and nothing in the tree has ever emitted either, so two of
                // the three arms only ever matched rows written by a version
                // that no longer exists. Worse than idle: an August `step` row
                // was reported as a run still in flight, its elapsed growing by
                // a second every second, weeks after the run ended. Measured to
                // its own timestamp it says the true thing, which is how long
                // after dispatch that step happened.
                let end = if entry.kind == "dispatched" { now } else { at };
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
        // Kinds something in the tree actually emits. The fixtures used to be
        // `planned` and `step`, which nothing has written for a long time.
        record("dispatched", "@claude", "xnaut-82", "project templates");
        record("nudged", "claude", "XNAUT-82", "check your tickets");
        record("sweep_verify", "claude", "XNAUT-82", "sat in done unreviewed");

        let recent = ledger_recent(Some(10));
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].kind, "sweep_verify", "newest first");
        // Every entry for a dispatched ticket carries how long it has been
        // going, which is the question being asked.
        assert!(recent.iter().all(|entry| entry.elapsed_secs.is_some()), "{recent:?}");
        assert_eq!(recent[0].agent, "claude", "the @ is not part of a handle");
        assert_eq!(recent[0].ticket, "XNAUT-82", "ids are compared upper-cased");
    }

    #[test]
    fn a_retired_kind_is_not_timed_as_a_run_still_going() {
        // `planned` and `step` sat alongside `dispatched` in the "still going"
        // match and nothing in the tree emits either; they are vocabulary from
        // a version that no longer exists. André's on-disk ledger still holds
        // 6 `planned` and 12 `step` rows from 16 August, and every one of them
        // was reported as a run in flight, elapsed growing by a second every
        // second, weeks after the run ended.
        //
        // Written by hand because `record` stamps now, and the whole question
        // is what happens to a row that is old.
        let _guard = scratch("retired-kinds");
        let mut handle = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path())
            .unwrap();
        writeln!(
            handle,
            r#"{{"at":"2026-08-16T21:00:00+00:00","kind":"dispatched","agent":"ralph","ticket":"XNAUT-176","detail":"go"}}"#
        )
        .unwrap();
        writeln!(
            handle,
            r#"{{"at":"2026-08-16T21:00:30+00:00","kind":"step","agent":"ralph","ticket":"XNAUT-176","detail":"3 steps"}}"#
        )
        .unwrap();
        drop(handle);

        let recent = ledger_recent(None);
        let step = recent
            .iter()
            .find(|entry| entry.kind == "step")
            .expect("the old row still reads");
        assert_eq!(
            step.elapsed_secs,
            Some(30),
            "measured to its own timestamp, not to now: {recent:?}"
        );
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
    fn a_row_written_before_the_join_existed_still_reads() {
        // All 60 rows in André's ledger predate the `session` field. If adding
        // it made them unparseable the Agent timeline would go blank, which is
        // a worse failure than the gap being closed. Written by hand because
        // `record_in_session` cannot produce a line that lacks the key.
        let _guard = scratch("legacy-row");
        let mut handle =
            std::fs::OpenOptions::new().create(true).append(true).open(path()).unwrap();
        writeln!(
            handle,
            r#"{{"at":"2026-08-16T18:37:11.684363+00:00","kind":"dispatched","agent":"codex","ticket":"XNAUT-26","detail":"bundle skills","elapsed_secs":null}}"#
        )
        .unwrap();
        drop(handle);

        let recent = ledger_recent(None);
        assert_eq!(recent.len(), 1, "the old row still parses: {recent:?}");
        assert_eq!(recent[0].ticket, "XNAUT-26");
        assert!(recent[0].session.is_empty(), "a row that never named one reads as unknown");
        // And it is honestly unjoinable rather than silently absent.
        assert!(rows_for_ticket("XNAUT-26").iter().all(|row| row.session.is_empty()));
        assert!(claimed_sessions().is_empty(), "an empty session is not a claim");
    }

    #[test]
    fn only_a_caller_holding_the_id_can_write_the_link() {
        // The whole no-wrong-link argument rests on this: `record` cannot
        // produce an attribution, and `record_in_session` writes exactly what
        // it was handed. There is no third path, and no inference anywhere.
        let _guard = scratch("no-inference");
        record("dispatched", "claude", "XNAUT-58", "plain dispatch names no session");
        record_in_session("dispatched", "claude", "XNAUT-73", "this one does", "sess-abc");

        let rows = rows_for_ticket("XNAUT-58");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].session.is_empty(), "record must never invent a session");
        assert_eq!(rows_for_ticket("XNAUT-73")[0].session, "sess-abc");
        // Only the ticket that actually claimed one shows up as a claim, so a
        // sibling run in the same second cannot borrow it.
        assert_eq!(claimed_sessions().into_iter().collect::<Vec<_>>(), vec!["sess-abc"]);
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
