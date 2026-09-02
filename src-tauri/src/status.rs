// Agent status tracking. Phase 4 of the Orca port — regex/output-silence fallback
// while Phase 5 (hook server) is not yet wired. Vocabulary is the literal Orca
// set: Working / Blocked / Waiting / Done + UI-only Idle / Permission / Interrupted.
//
// Detection model: an agent session is "working" while its PTY emits output;
// after `IDLE_AFTER_MS` of silence it decays to "idle". On PTY EOF it transitions
// to "done". Hook-based detection (Phase 5) will replace this with a push signal.

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

/// Silence before Working decays to Idle.
///
/// Two seconds answered "is this terminal emitting bytes", not "is this agent
/// working": a thinking pause flipped the state to Idle and back, so the
/// indicator strobed. Eight is the compromise — long enough to bridge a pause,
/// short enough that a finished agent stops looking busy.
///
/// It is only a heuristic. An agent launched through xNAUT reports its own turn
/// boundaries via the hooks (done/waiting/idle) and never relies on this; a
/// session attached from an existing zellij tab has no hooks, so silence is all
/// there is. Instrumenting those is the real fix.
const IDLE_AFTER_MS: i64 = 8_000;
/// Once Working is entered it holds at least this long, so a burst of output
/// followed by a pause cannot strobe the indicator.
const MIN_WORKING_MS: i64 = 5_000;
const STALE_AFTER_MS: i64 = 30 * 60 * 1_000;
const DECAY_TICK_MS: u64 = 750;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Working,
    Blocked,
    Waiting,
    Done,
    Idle,
    Permission,
    Interrupted,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSessionMeta {
    pub session_id: String,
    pub agent_id: String,
    pub label: String,
    /// Composite `tab_id:leaf_id` for future pane-identity rendezvous.
    /// Today both default to the session_id since splits aren't wired yet.
    pub pane_key: String,
    pub status: AgentStatus,
    pub started_at_ms: i64,
    pub last_output_at_ms: i64,
    pub status_changed_at_ms: i64,
    /// The zellij run's captured tty stream, when this session hosts one
    /// (XNAUT-242): the PTY shows the zellij client's repaint protocol, which
    /// no simple renderer can read; the FILE holds the pane's real bytes.
    #[serde(default)]
    pub output_path: Option<String>,
    /// The zellij session this run is hosted in, when it has one. Without it
    /// a freshly dispatched agent is counted twice — once as this row (keyed
    /// by PTY uuid) and once as its zellij session (XNAUT-260, diagnosed by
    /// the rig by matching ELAPSED values across rows).
    #[serde(default)]
    pub zellij_session: Option<String>,
}

pub type AgentSessions = Arc<Mutex<HashMap<String, AgentSessionMeta>>>;

/// The statuses that occupy a spend-ceiling slot.
///
/// ONE definition on purpose. The launch gate counts these, and so does the
/// scheduler when it asks whether the run it is about to reap would leave room
/// for its replacement; two copies of this list drifting apart is exactly how a
/// reap could free a slot the gate still counted (rig, 2026-09-01).
pub fn counts_as_live(status: AgentStatus) -> bool {
    matches!(
        status,
        AgentStatus::Working
            | AgentStatus::Blocked
            | AgentStatus::Waiting
            | AgentStatus::Permission
    )
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// When did the AGENT ITSELF last produce output? `None` when this row carries
/// no capture file to ask.
///
/// Not the PTY's frame clock. A zellij-backed row's PTY hosts the zellij
/// CLIENT, whose status bar repaints on its own timer, so `last_output_at_ms`
/// moves whether or not anything is still behind the pane; killing an agent
/// produces a burst of repaints, so a dead row looks busiest of all. On
/// 2026-09-01 a rig with seven agents showed SIX of them Working while the one
/// agent actually running was not among them. Exactly inverted, and
/// /api/sessions agreed with the wrong answer because it reads this same map.
///
/// The capture file is the honest per-session signal, and the same one
/// XNAUT-268 used to fix the wake path: `script(1)` writes it from the agent's
/// own tty for the life of the agent's command (agents.rs `prepare_zellij_run`)
/// and closes it when that command ends, so its mtime stops the moment the
/// agent does and a repaint never touches it.
///
/// mtime rather than size: it answers the same question with no state to carry
/// between ticks.
fn agent_output_at_ms(meta: &AgentSessionMeta) -> Option<i64> {
    // ponytail: a row with no capture file keeps the old frame clock. That is
    // a zellij session xNAUT attached to rather than launched (pty.rs), so no
    // script(1) capture of it exists to read; its status stays as noisy as it
    // was. Instrumenting an attached session is the real fix.
    capture_mtime_ms(meta.output_path.as_deref()?)
}

/// When a capture file was last written, or `None` when there is no file to
/// ask. The idle reaper (scheduler.rs) works from the live session list rather
/// than from status rows, so it reads the same signal by path.
pub(crate) fn capture_mtime_ms(path: &str) -> Option<i64> {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|since| since.as_millis() as i64)
}

fn pane_key_for(session_id: &str) -> String {
    // Today: every agent owns its own tab; leaf is the same as the session.
    // When splits land, the caller will pass tab/leaf explicitly.
    format!("{session_id}:{session_id}")
}

/// Records a freshly-launched agent. Called from agents.rs.
pub async fn register_agent_session(
    sessions: &AgentSessions,
    app: &AppHandle,
    session_id: &str,
    agent_id: &str,
    label: &str,
    output_path: Option<String>,
    zellij_session: Option<String>,
) {
    let now = now_ms();
    let meta = AgentSessionMeta {
        session_id: session_id.to_string(),
        agent_id: agent_id.to_string(),
        label: label.to_string(),
        pane_key: pane_key_for(session_id),
        status: AgentStatus::Working,
        started_at_ms: now,
        last_output_at_ms: now,
        status_changed_at_ms: now,
        output_path,
        zellij_session,
    };
    {
        let mut map = sessions.lock().await;
        map.insert(session_id.to_string(), meta.clone());
    }
    let _ = app.emit("agent-status-changed", &meta);
}

/// Re-adopt zellij-backed runs that survived an app restart (XNAUT-242).
///
/// The tracker lives in app memory, so a restart forgot every live run and
/// the pane said "No agent sessions running" while claude demonstrably
/// worked on (observed 2026-08-31, run 65dce236 with 116KB of captured
/// output and no app attached). Names are xnaut-<handle>-<run8>; the
/// capture file, when present, makes the adopted row readable immediately.
pub async fn adopt_surviving_runs(sessions: &AgentSessions, app: &AppHandle) {
    let live = tokio::task::spawn_blocking(crate::zellij::live_sessions)
        .await
        .unwrap_or_default();
    let run_dir = crate::agents::run_dir().ok();
    let now = now_ms();
    // Real session ages, so an adopted row does not report the APP's uptime as
    // its elapsed time (the rig measured every adopted row at 12:59 while the
    // sessions were 2-4 hours old, XNAUT-260).
    let ages: std::collections::HashMap<String, u64> =
        tokio::task::spawn_blocking(crate::zellij::zellij_sessions_info)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter_map(|info| info.created_ms.map(|ms| (info.name, ms)))
            .collect();
    // Prune adopted rows whose session has since ended: an adopted row that
    // outlives its zellij session is a ghost that eats wakes.
    {
        let mut map = sessions.lock().await;
        let dead: Vec<String> = map
            .iter()
            .filter(|(id, meta)| meta.label.ends_with("· adopted") && !live.contains(id))
            .map(|(id, _)| id.clone())
            .collect();
        for id in dead {
            map.remove(&id);
            let _ = app.emit("agent-status-dropped", &serde_json::json!({ "sessionId": id }));
        }
    }
    for name in live {
        let Some(rest) = name.strip_prefix("xnaut-") else { continue };
        let Some((handle, _run)) = rest.rsplit_once('-') else { continue };
        if handle.is_empty() {
            continue;
        }
        let output_path = run_dir
            .as_ref()
            .map(|dir| dir.join(format!("{name}.jsonl")))
            .filter(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned());
        let meta = AgentSessionMeta {
            session_id: name.clone(),
            agent_id: handle.to_string(),
            label: format!("{handle} · adopted"),
            pane_key: pane_key_for(&name),
            status: AgentStatus::Working,
            started_at_ms: ages.get(&name).map(|ms| *ms as i64).unwrap_or(now),
            last_output_at_ms: now,
            status_changed_at_ms: now,
            output_path,
            zellij_session: Some(name.clone()),
        };
        {
            let mut map = sessions.lock().await;
            if map.contains_key(&name) {
                continue;
            }
            map.insert(name.clone(), meta.clone());
        }
        crate::ledger::record("adopted", handle, "", &name);
        let _ = app.emit("agent-status-changed", &meta);
    }
}

/// Pings on every PTY output frame for an agent session. If the session isn't
/// in the agent registry (e.g. it's a plain shell), this is a no-op.
///
/// `last_output_at_ms` is stamped unconditionally: it is the frame clock, other
/// UI reads it, and a frame really did arrive. What the frame no longer buys is
/// a captured row's status. See `agent_output_at_ms` for the 2026-09-01
/// inversion; the short version is that on a zellij-backed row this frame may
/// be nothing but a status-bar repaint, and a dead agent repaints hardest. For
/// those rows the decay tick settles both edges against the capture file
/// instead. A row with no capture file has no second process behind its PTY, so
/// its frames are its agent's own and the old inference stands.
pub async fn ping_session_output(sessions: &AgentSessions, app: &AppHandle, session_id: &str) {
    let now = now_ms();
    let updated = {
        let mut map = sessions.lock().await;
        match map.get_mut(session_id) {
            Some(meta) => apply_frame(meta, now).then(|| meta.clone()),
            None => None,
        }
    };
    if let Some(meta) = updated {
        let _ = app.emit("agent-status-changed", &meta);
    }
}

/// Stamps the frame clock, and promotes to Working only where the frame is
/// honest evidence. Returns whether the STATUS moved, which is what the caller
/// emits on.
fn apply_frame(meta: &mut AgentSessionMeta, now: i64) -> bool {
    // Unconditional: a frame really did arrive, this is the frame clock, and
    // other UI reads it. Only the Working INFERENCE below is in question.
    meta.last_output_at_ms = now;
    // A frame on a captured row may be nothing but the zellij client repainting
    // its status bar, and on 2026-09-01 that read six exited agents as Working
    // while the one that was running read Idle. Those rows are settled against
    // the capture file by `decay_step` instead. A row with no capture file has
    // no second process behind its PTY, so its frames are its agent's own.
    if meta.output_path.is_some() || meta.status == AgentStatus::Working {
        return false;
    }
    meta.status = AgentStatus::Working;
    meta.status_changed_at_ms = now;
    true
}


/// Push (1.22.2 item 1): the transitions a phone should interrupt someone
/// for — an agent needing a human (blocked / waiting / permission) and a run
/// ending (done / interrupted). Working and Idle never push; they are the
/// normal hum of the machine.
fn push_transition(meta: &AgentSessionMeta) {
    let verb = match meta.status {
        AgentStatus::Blocked => "is blocked",
        AgentStatus::Waiting => "is waiting on you",
        AgentStatus::Permission => "needs permission",
        AgentStatus::Done => "finished its run",
        AgentStatus::Interrupted => "was interrupted",
        AgentStatus::Working | AgentStatus::Idle => return,
    };
    crate::push::notify(crate::push::PushNote {
        title: format!("{} {}", meta.label, verb),
        body: meta.session_id.clone(),
        kind: "status".into(),
        inbox_id: None,
        project: None,
    });
}

/// Called when a PTY exits cleanly (EOF) or the agent crashes.
pub async fn mark_session_done(sessions: &AgentSessions, app: &AppHandle, session_id: &str) {
    let now = now_ms();
    let updated = {
        let mut map = sessions.lock().await;
        match map.get_mut(session_id) {
            Some(meta) => {
                meta.status = AgentStatus::Done;
                meta.status_changed_at_ms = now;
                Some(meta.clone())
            }
            None => None,
        }
    };
    if let Some(meta) = updated {
        push_transition(&meta);
        let _ = app.emit("agent-status-changed", &meta);
    }
}

/// Sets the session to an arbitrary state. Used by the Phase 5 hook listener
/// for Working / Blocked / Waiting / Permission / Idle transitions that aren't
/// otherwise derivable from PTY output.
///
/// This is the TRUSTED door, and since 2026-09-01 hook-reported Working comes
/// through it too. A hook only fires because something is running to fire it,
/// so it is evidence in a way a PTY frame is not; routing Working through the
/// output ping instead would have muted it on exactly the captured rows the
/// ping stopped believing.
pub async fn set_session_status(
    sessions: &AgentSessions,
    app: &AppHandle,
    session_id: &str,
    new_status: AgentStatus,
) {
    let now = now_ms();
    let updated = {
        let mut map = sessions.lock().await;
        match map.get_mut(session_id) {
            Some(meta) if meta.status != new_status => {
                meta.status = new_status;
                meta.status_changed_at_ms = now;
                // A hook saying Working is also a report of fresh output, so
                // the decay tick treats it exactly as it treated the output
                // ping that used to carry this state.
                if new_status == AgentStatus::Working {
                    meta.last_output_at_ms = now;
                }
                Some(meta.clone())
            }
            _ => None,
        }
    };
    if let Some(meta) = updated {
        push_transition(&meta);
        let _ = app.emit("agent-status-changed", &meta);
    }
}

/// Marks a session interrupted (user-cancelled / agent crashed without hook).
/// Mirrors Orca's narrow interrupt-synthesis fallback.
pub async fn mark_session_interrupted(sessions: &AgentSessions, app: &AppHandle, session_id: &str) {
    let now = now_ms();
    let updated = {
        let mut map = sessions.lock().await;
        match map.get_mut(session_id) {
            Some(meta) => {
                meta.status = AgentStatus::Interrupted;
                meta.status_changed_at_ms = now;
                Some(meta.clone())
            }
            None => None,
        }
    };
    if let Some(meta) = updated {
        push_transition(&meta);
        let _ = app.emit("agent-status-changed", &meta);
    }
}

/// The status this row should move to on this tick, or `None` to leave it.
///
/// Both output-derived edges live here, and ONLY here for a captured row. That
/// is the answer to "do not probe on every output frame": the probe is one
/// `stat` per row per `DECAY_TICK_MS`, taken at the moment the decision is
/// actually made, so the frames themselves (up to one flush per 16ms per
/// session) cost nothing. Bounded by row count, not by how loudly a pane
/// repaints, which is the quantity that was wrong.
///
/// Only Working and Idle are moved between. Blocked, Permission, Waiting, Done
/// and Interrupted are the agent's own hooks talking; those are evidence that
/// something is there, and this function has no better information than they
/// do.
fn decay_step(meta: &AgentSessionMeta, now: i64) -> Option<AgentStatus> {
    let agent_at = agent_output_at_ms(meta);
    let quiet_since = agent_at.unwrap_or(meta.last_output_at_ms);
    let quiet = now - quiet_since >= IDLE_AFTER_MS;
    match meta.status {
        AgentStatus::Working if quiet && now - meta.status_changed_at_ms >= MIN_WORKING_MS => {
            Some(AgentStatus::Idle)
        }
        // A captured row earns Working back the same way it keeps it: its agent
        // wrote to its own tty. Rows without a capture file are promoted on the
        // frame itself, in `ping_session_output`, and must not be promoted here
        // as well or a plain shell's silence would flap.
        AgentStatus::Idle if agent_at.is_some() && !quiet => Some(AgentStatus::Working),
        _ => None,
    }
}

/// Spawns the decay loop. Working → Idle after IDLE_AFTER_MS of silence;
/// any state stale longer than STALE_AFTER_MS is dropped from the map so
/// the status strip doesn't accumulate forever.
pub fn spawn_decay_task(app: AppHandle) {
    tokio::spawn(async move {
        let state = match app.try_state::<AppState>() {
            Some(s) => s,
            None => {
                eprintln!("[status] AppState not available — decay task aborting");
                return;
            }
        };
        let sessions = state.agent_sessions.clone();
        loop {
            tokio::time::sleep(Duration::from_millis(DECAY_TICK_MS)).await;
            let now = now_ms();
            let mut changed: Vec<AgentSessionMeta> = Vec::new();
            let mut to_drop: Vec<String> = Vec::new();
            {
                let mut map = sessions.lock().await;
                for (id, meta) in map.iter_mut() {
                    if let Some(next) = decay_step(meta, now) {
                        meta.status = next;
                        meta.status_changed_at_ms = now;
                        changed.push(meta.clone());
                    }
                    if now - meta.status_changed_at_ms >= STALE_AFTER_MS {
                        to_drop.push(id.clone());
                    }
                }
                for id in &to_drop {
                    map.remove(id);
                }
            }
            for meta in changed {
                let _ = app.emit("agent-status-changed", &meta);
            }
            for id in to_drop {
                let _ = app.emit(
                    "agent-status-dropped",
                    &serde_json::json!({ "sessionId": id }),
                );
            }
        }
    });
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn agent_sessions_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AgentSessionMeta>, String> {
    let map = state.agent_sessions.lock().await;
    Ok(map.values().cloned().collect())
}

#[tauri::command]
pub async fn agent_session_interrupt(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    mark_session_interrupted(&state.agent_sessions, &app, &session_id).await;
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_key_format_is_session_session() {
        assert_eq!(pane_key_for("abc"), "abc:abc");
    }

    #[test]
    fn now_ms_is_monotonic_within_a_test() {
        let a = now_ms();
        std::thread::sleep(Duration::from_millis(2));
        let b = now_ms();
        assert!(b >= a);
    }

    // ─── The 2026-09-01 inversion ───────────────────────────────────────────
    //
    // A rig with seven agents showed SIX rows Working while the one agent
    // actually running was not among them, and /api/sessions agreed with the
    // wrong answer. The cause: a zellij-backed row's PTY hosts the zellij
    // CLIENT, so its status-bar repaints reached the status tracker as if they
    // were the agent's own output, and killing an agent repaints hardest of
    // all. These pin the capture file as the signal instead.
    //
    // Time is injected rather than slept: every case is a distance between the
    // capture file's mtime and "now", so the file is written once and `now` is
    // moved, which keeps the tests hermetic and instant.

    struct Capture(std::path::PathBuf);
    impl Capture {
        /// A run's capture file, exactly as script(1) leaves one.
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "xnaut-status-{tag}-{}-{:?}.jsonl",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::write(&path, b"the agent's own tty").expect("capture file");
            Self(path)
        }
        fn row(&self, status: AgentStatus) -> AgentSessionMeta {
            let mut row = plain_row(status);
            row.output_path = Some(self.0.to_string_lossy().into_owned());
            row.zellij_session = Some("xnaut-rigtwo-deadbeef".into());
            row
        }
        /// The moment the agent last wrote, read back the way production does.
        fn wrote_at(&self, row: &AgentSessionMeta) -> i64 {
            agent_output_at_ms(row).expect("a written capture file has an mtime")
        }
    }
    impl Drop for Capture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// A row whose PTY carries its agent directly, with no capture file.
    fn plain_row(status: AgentStatus) -> AgentSessionMeta {
        AgentSessionMeta {
            session_id: "s1".into(),
            agent_id: "rigtwo".into(),
            label: "Rig Two".into(),
            pane_key: pane_key_for("s1"),
            status,
            started_at_ms: 0,
            last_output_at_ms: 0,
            status_changed_at_ms: 0,
            output_path: None,
            zellij_session: None,
        }
    }

    #[test]
    fn an_exited_agents_repainting_pane_does_not_hold_its_row_at_working() {
        // THE OBSERVED INVERSION. The agent is gone, so its capture file
        // stopped a minute ago; its pane is repainting right now, so the frame
        // clock reads this instant. Six rows looked like this on the rig.
        let capture = Capture::new("exited");
        let mut row = capture.row(AgentStatus::Working);
        let now = capture.wrote_at(&row) + 60_000;
        row.last_output_at_ms = now;
        row.status_changed_at_ms = now - 60_000;

        assert_eq!(
            decay_step(&row, now),
            Some(AgentStatus::Idle),
            "a row whose agent has exited must not read Working, however loudly its pane repaints"
        );
    }

    #[test]
    fn a_repaint_alone_never_promotes_a_row_to_working() {
        // The other half of the inversion: the frame that arrives is the zellij
        // client's, and it must not be able to raise a quiet row. The frame
        // clock is still stamped, because other UI reads it.
        let capture = Capture::new("repaint");
        let mut row = capture.row(AgentStatus::Idle);
        let now = capture.wrote_at(&row) + 60_000;

        assert!(
            !apply_frame(&mut row, now),
            "a repaint must not move the status"
        );
        assert_eq!(row.status, AgentStatus::Idle);
        assert_eq!(
            row.last_output_at_ms, now,
            "the frame clock is a separate fact and still has to be stamped"
        );
        assert_eq!(
            decay_step(&row, now),
            None,
            "and the tick must not promote it either, on the strength of that frame"
        );
    }

    #[test]
    fn an_agent_mid_turn_reads_working() {
        // The fix has to be able to say yes. Its agent wrote to its own tty a
        // moment ago, which a repaint can never do.
        let capture = Capture::new("midturn");
        let row = capture.row(AgentStatus::Idle);
        let now = capture.wrote_at(&row) + 100;

        assert_eq!(
            decay_step(&row, now),
            Some(AgentStatus::Working),
            "an agent that is genuinely writing must read Working"
        );
    }

    #[test]
    fn the_hook_states_are_left_alone_by_the_capture_file() {
        // Blocked, Permission, Waiting and Done come from the agent's own
        // hooks. Those are trustworthy; only the output-derived transition was
        // not, so narrowing it must not reach them. A long-stale capture file
        // is the case that would sweep them up if the tick treated silence as
        // authority over every row rather than over Working alone.
        let capture = Capture::new("hooks");
        let now = capture.wrote_at(&capture.row(AgentStatus::Idle)) + 60_000;

        for status in [
            AgentStatus::Blocked,
            AgentStatus::Permission,
            AgentStatus::Waiting,
            AgentStatus::Done,
        ] {
            let held = capture.row(status);
            assert_eq!(
                decay_step(&held, now),
                None,
                "{status:?} belongs to the hooks and the tick has nothing better to say"
            );
        }
    }

    #[test]
    fn a_plain_pty_row_is_still_promoted_by_its_own_output() {
        // The narrowing is aimed at captured rows only. A plain PTY row's agent
        // IS the PTY's child, so its frames are the agent's own and the
        // original inference stands; breaking that would blank every
        // non-durable session's dot.
        let mut row = plain_row(AgentStatus::Idle);
        assert!(
            apply_frame(&mut row, 1_000),
            "a plain PTY frame is its agent's own output"
        );
        assert_eq!(row.status, AgentStatus::Working);

        // And its silence still decays, off the frame clock, since it has no
        // capture file to ask.
        let quiet = plain_row(AgentStatus::Working);
        assert_eq!(
            decay_step(&quiet, IDLE_AFTER_MS + MIN_WORKING_MS),
            Some(AgentStatus::Idle)
        );
    }
}
