// The wake-up half of the ticket pull loop: NautBot assigns a ticket, then
// nudges the owner to go check its bucket. The nudge deliberately carries no
// work — the ticket does — so a lost or doubled nudge costs nothing but one
// redundant list call. Delivery is keystrokes into the agent's live PTY, the
// one interface every runtime accepts; the bracketed-paste form is the same
// mechanism the StdinAfterStart launcher already uses.

use std::io::Write;
use tauri::{AppHandle, Manager};

use crate::status::AgentStatus;

/// What became of a nudge. Data, not an error: the caller (usually a model)
/// has to be able to say WHY the agent was not woken.
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// Typed into the named live session.
    Typed,
    /// The agent is mid-turn; typing would garble its input. The periodic
    /// check catches the ticket instead.
    SkippedBusy,
    /// No live session existed, so one was launched cold with the message
    /// as its task. agent_profile_launch runs fully backend-side (the PTY
    /// needs no pane — the mobile bridge proved that in cde6453), so waking
    /// a cold agent is a launch, not an apology.
    Launched,
    /// No live session, and launching was declined or failed.
    NoSession,
}

/// Session statuses it is safe to type into: the TUI is at rest at its input
/// prompt. Working is mid-turn; Blocked/Permission mean the agent is showing
/// its own prompt and unexpected input could answer it.
fn accepts_input(status: AgentStatus) -> bool {
    matches!(
        status,
        AgentStatus::Idle | AgentStatus::Waiting | AgentStatus::Done | AgentStatus::Interrupted
    )
}

fn normalize_handle(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('@')
        .unwrap_or(trimmed)
        .to_ascii_lowercase()
}

/// Picks the session to nudge for a handle from the status tracker's rows:
/// the most recently started session whose agent_id is the handle. Pure so
/// the busy/idle decision is testable without a PTY.
pub(crate) fn pick_session(
    sessions: &std::collections::HashMap<String, crate::status::AgentSessionMeta>,
    handle: &str,
) -> Delivery0 {
    let handle = normalize_handle(handle);
    let best = sessions
        .values()
        .filter(|meta| normalize_handle(&meta.agent_id) == handle)
        .max_by_key(|meta| meta.started_at_ms);
    match best {
        None => Delivery0::NoSession,
        Some(meta) if accepts_input(meta.status) => Delivery0::Type(meta.session_id.clone()),
        Some(_) => Delivery0::Busy,
    }
}

/// Internal decision, before the PTY write has happened.
#[derive(Debug, PartialEq)]
pub(crate) enum Delivery0 {
    Type(String),
    Busy,
    NoSession,
}

/// Nudges the agent behind `handle` with `message`. Resolves the live session
/// from the status tracker, refuses to type into a busy one, and reports what
/// happened as data.
pub async fn nudge_agent(app: &AppHandle, handle: &str, message: &str) -> Result<serde_json::Value, String> {
    // "claudi" is a display name; the handle is "claude". Resolve whatever
    // was actually said before anything else, so the wake, the quarantine
    // check and the session lookup all use the one canonical name.
    let handle = &crate::agent_profiles::resolve_spoken_handle(handle)?;
    // The kill-switches gate every nudge here, the one choke point both the
    // chat tool and the tauri command pass through.
    let switches = crate::switches::load();
    if switches.read_only {
        return Err("the read_only kill-switch is engaged; nudges are paused".to_string());
    }
    if switches.is_quarantined(handle) {
        return Err(format!(
            "{handle} is quarantined; the nudge was not delivered"
        ));
    }
    let state = app
        .try_state::<crate::state::AppState>()
        .ok_or("app state unavailable")?;
    let decision = {
        let sessions = state.agent_sessions.lock().await;
        pick_session(&sessions, handle)
    };
    let (delivery, session_id) = match decision {
        Delivery0::NoSession => match cold_launch(app, handle, message).await {
            Ok(session_id) => (Delivery::Launched, Some(session_id)),
            Err(error) => {
                let _ = crate::debug_log::debug_log_append(vec![format!(
                    "[nudge] cold launch of {handle} failed: {error}"
                )]);
                (Delivery::NoSession, None)
            }
        },
        Delivery0::Busy => (Delivery::SkippedBusy, None),
        Delivery0::Type(session_id) => {
            let sessions = state.pty_sessions.lock().await;
            let Some(session) = sessions.get(&session_id) else {
                // An ADOPTED row has no PTY behind it (the app that owned the
                // PTY is a previous life), and a stale row's PTY is simply
                // gone. Either way the right move is a fresh cold launch, not
                // an error — found on the tron rig: "status row exists but
                // PTY xnaut-claude-ba2b93c4 is gone" broke every wake after
                // an adoption (XNAUT-242).
                drop(sessions);
                {
                    let mut map = state.agent_sessions.lock().await;
                    map.remove(&session_id);
                }
                let (delivery, sid) = match cold_launch(app, handle, message).await {
                    Ok(sid) => (Delivery::Launched, Some(sid)),
                    Err(error) => {
                        let _ = crate::debug_log::debug_log_append(vec![format!(
                            "[nudge] cold relaunch after stale row failed: {error}"
                        )]);
                        (Delivery::NoSession, None)
                    }
                };
                crate::ledger::record(
                    match delivery {
                        Delivery::Launched => "dispatched",
                        _ => "wake_failed",
                    },
                    handle,
                    "",
                    message,
                );
                return Ok(serde_json::json!({
                    "ok": true,
                    "handle": normalize_handle(handle),
                    "delivery": delivery,
                    "session_id": sid,
                }));
            };
            // Bracketed paste so multi-line text lands as one unit, then Enter.
            // The writer guard is a std Mutex and must not survive into the
            // acknowledgement wait below: holding it across an await makes the
            // whole command future non-Send, which the compiler reports three
            // modules away.
            // The paste and the submit are TWO writes, and they have to be.
            //
            // A trailing \r inside the same write as the bracketed-paste end
            // marker is swallowed as paste content: the TUI reads the whole
            // chunk, sees the paste block, and treats the carriage return as a
            // newline in the composer rather than a submit. The rig proved this
            // twice on 2026-09-01 — the wake text sat in Claude's composer
            // character for character, unsubmitted, for 90 seconds, and a bare
            // `zellij action write 13` into the same pane ran it in 2 seconds.
            //
            // So: paste, let the TUI finish handling it, then submit on its own.
            {
                let payload = format!("\x1b[200~{message}\x1b[201~");
                let mut writer = session
                    .writer
                    .lock()
                    .map_err(|_| "PTY writer poisoned".to_string())?;
                writer
                    .write_all(payload.as_bytes())
                    .and_then(|_| writer.flush())
                    .map_err(|e| format!("write to PTY failed: {e}"))?;
            }
            // ponytail: a fixed pause, not a readiness handshake. The TUI gives
            // no signal that a paste has been absorbed, and 150ms is far below
            // the acknowledgement window that follows.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            {
                let mut writer = session
                    .writer
                    .lock()
                    .map_err(|_| "PTY writer poisoned".to_string())?;
                writer
                    .write_all(b"\r")
                    .and_then(|_| writer.flush())
                    .map_err(|e| format!("submit after paste failed: {e}"))?;
            }
            drop(sessions);
            // ACKNOWLEDGEMENT, not optimism (XNAUT-263). "Typed" used to mean
            // "bytes were written to a PTY", which is not the same as "an
            // agent took the task": on the tron rig a wake was typed into a
            // session whose agent had finished its turn, reported success, and
            // reached nobody for three hours. An agent that received keystrokes
            // produces output within seconds; if none appears, the session is
            // treated as dead and the work is cold-launched instead.
            let acknowledged = awaited_output(&state.agent_sessions, &session_id).await;
            if acknowledged {
                (Delivery::Typed, Some(session_id))
            } else {
                crate::ledger::record(
                    "wake_unacknowledged",
                    handle,
                    "",
                    &format!("{session_id} did not answer the keystrokes; cold-launching instead"),
                );
                {
                    let mut map = state.agent_sessions.lock().await;
                    map.remove(&session_id);
                }
                match cold_launch(app, handle, message).await {
                    Ok(sid) => (Delivery::Launched, Some(sid)),
                    Err(error) => {
                        let _ = crate::debug_log::debug_log_append(vec![format!(
                            "[nudge] cold launch after an unacknowledged wake failed: {error}"
                        )]);
                        (Delivery::NoSession, None)
                    }
                }
            }
        }
    };
    // The wake goes in the ledger, whatever happened. Found 2026-08-31: a
    // wake left NO trace, so the Agent timeline had no entry for a day on
    // which an agent demonstrably ran — the newest loop was open at its own
    // return edge. The ticket rides in the message when the caller named one.
    crate::ledger::record(
        match delivery {
            Delivery::Launched => "dispatched",
            Delivery::Typed => "nudged",
            Delivery::SkippedBusy => "wake_skipped_busy",
            Delivery::NoSession => "wake_failed",
        },
        handle,
        "",
        message,
    );
    Ok(serde_json::json!({
        "ok": true,
        "handle": normalize_handle(handle),
        "delivery": delivery,
        "session_id": session_id,
    }))
}

/// Did the session produce any output after the keystrokes?
///
/// The status tracker already stamps `last_output_at_ms` on every PTY frame,
/// so the acknowledgement costs nothing extra: remember the stamp, wait, and
/// see whether it moved. A live agent echoes the pasted text immediately; a
/// finished one, or a shell whose agent exited, does not.
///
/// Deliberately short and deliberately fail-safe in the OTHER direction from
/// `agent_alive_in`: here a wrong "not acknowledged" costs one redundant cold
/// launch, while a wrong "acknowledged" loses the task silently, which is the
/// failure being fixed.
async fn awaited_output(sessions: &crate::status::AgentSessions, session_id: &str) -> bool {
    const WINDOW: std::time::Duration = std::time::Duration::from_millis(3000);
    const STEP: std::time::Duration = std::time::Duration::from_millis(250);

    // WHICH signal matters, and why the obvious one is wrong. The first
    // version of this watched `last_output_at_ms`, which moves on any PTY
    // frame — and in a zellij-backed session the PTY hosts the zellij CLIENT,
    // whose status bar repaints on its own. Every wake therefore looked
    // acknowledged, including the two the rig aimed at a finished agent
    // (round 10: no wake_unacknowledged, ever, in the whole ledger).
    //
    // The run's capture FILE is the honest signal: script(1) writes it, and
    // only the agent's own tty produces bytes for it. A live agent echoes the
    // pasted text into it within milliseconds; a finished one cannot.
    let (capture, before_ms) = {
        let map = sessions.lock().await;
        match map.get(session_id) {
            Some(meta) => (meta.output_path.clone(), meta.last_output_at_ms),
            None => return false,
        }
    };

    if let Some(path) = capture {
        let size_of = |p: &str| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let before = size_of(&path);
        let deadline = std::time::Instant::now() + WINDOW;
        while std::time::Instant::now() < deadline {
            tokio::time::sleep(STEP).await;
            if size_of(&path) > before {
                return true;
            }
        }
        return false;
    }

    // No capture file: a plain PTY session, where the frame timestamp IS the
    // agent's own output and the original signal holds.
    let deadline = std::time::Instant::now() + WINDOW;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(STEP).await;
        let map = sessions.lock().await;
        match map.get(session_id) {
            Some(meta) if meta.last_output_at_ms > before_ms => return true,
            Some(_) => {}
            None => return false,
        }
    }
    false
}

/// Launches the agent fresh in its scratch workspace with the nudge as the
/// task. The composer runs inside agent_profile_launch, so a cold-woken
/// agent gets the Foundation — and with it the ticket loop — like any other
/// profile launch.
async fn cold_launch(app: &AppHandle, handle: &str, message: &str) -> Result<String, String> {
    let handle = normalize_handle(handle);
    let worktree = crate::agent_profiles::agent_scratch_workspace(handle.clone())?;
    let state = tauri::Manager::state::<crate::state::AppState>(app);
    let response = crate::agent_profiles::agent_profile_launch(
        app.clone(),
        state,
        crate::agent_profiles::LaunchAgentProfileRequest {
            handle: handle.clone(),
            worktree_path: worktree,
            prompt: Some(message.to_string()),
            conversation_mode: false,
            conversation_id: None,
            resume: false,
            cols: Some(200),
            rows: Some(50),
            // A cold-woken agent must survive the app quitting: zellij-backed
            // like a conversation, without the conversation harness (XNAUT-242).
            durable: Some(true),
        },
    )
    .await?;
    confirm_durable_session(&handle, &response.session_id).await;
    Ok(response.session_id)
}

/// A durable launch has to prove it produced a durable session.
///
/// `agent_profile_launch` returns as soon as the PTY exists, and a PTY exists
/// whether or not the process inside it lived past its first millisecond. On
/// 2026-09-01 a bad zellij flag made every launch die in the parser: the wake
/// reported `launched`, the ledger agreed, and the rig sat idle for an hour
/// with nothing on screen. The forward edge was fine; there was no return edge.
///
/// So: wait for the session to actually appear, and if it never does, say so
/// in the ledger. Not an error — the PTY may still hold a working non-durable
/// agent — but never again a silent claim that an agent outlives the app when
/// it does not.
///
/// ponytail: polls rather than watches. zellij offers no readiness signal, and
/// three seconds of 200ms polls costs nothing next to a launch.
async fn confirm_durable_session(handle: &str, session_id: &str) {
    // The zellij session is named xnaut-<handle>-<run id>, and the run id is
    // not the PTY session id, so the prefix is what we can check for.
    let prefix = format!("xnaut-{handle}-");
    for _ in 0..15 {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if crate::zellij::live_sessions()
            .iter()
            .any(|name| name.starts_with(&prefix))
        {
            return;
        }
    }
    crate::ledger::record(
        "launch_not_durable",
        "nautbot",
        "",
        &format!(
            "{session_id} was launched durable but no zellij session appeared; \
             it will not survive the app quitting"
        ),
    );
}

#[tauri::command]
pub async fn agent_nudge(
    app: AppHandle,
    handle: String,
    message: Option<String>,
) -> Result<serde_json::Value, String> {
    let message = message.unwrap_or_else(|| "Check your tickets.".to_string());
    nudge_agent(&app, &handle, &message).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::AgentSessionMeta;
    use std::collections::HashMap;

    fn meta(session_id: &str, agent_id: &str, status: AgentStatus, started: i64) -> AgentSessionMeta {
        AgentSessionMeta {
            session_id: session_id.into(),
            agent_id: agent_id.into(),
            label: agent_id.into(),
            pane_key: session_id.into(),
            status,
            started_at_ms: started,
            last_output_at_ms: started,
            status_changed_at_ms: started,
            output_path: None,
            zellij_session: None,
        }
    }

    #[test]
    fn no_session_for_an_unknown_handle() {
        let sessions = HashMap::new();
        assert_eq!(pick_session(&sessions, "@claudi"), Delivery0::NoSession);
    }

    #[test]
    fn an_idle_session_is_typed_into() {
        let mut sessions = HashMap::new();
        sessions.insert("s1".into(), meta("s1", "claudi", AgentStatus::Idle, 10));
        assert_eq!(pick_session(&sessions, "@Claudi"), Delivery0::Type("s1".into()));
    }

    #[test]
    fn a_working_session_is_never_typed_into() {
        let mut sessions = HashMap::new();
        sessions.insert("s1".into(), meta("s1", "claudi", AgentStatus::Working, 10));
        assert_eq!(pick_session(&sessions, "claudi"), Delivery0::Busy);
    }

    #[test]
    fn the_newest_session_wins() {
        let mut sessions = HashMap::new();
        sessions.insert("old".into(), meta("old", "claudi", AgentStatus::Idle, 10));
        sessions.insert("new".into(), meta("new", "claudi", AgentStatus::Idle, 20));
        assert_eq!(pick_session(&sessions, "claudi"), Delivery0::Type("new".into()));
    }

    #[test]
    fn a_blocked_session_is_skipped_not_answered() {
        // Blocked means the agent is showing its own prompt (an inbox ask, a
        // permission screen). A nudge typed there could answer that prompt.
        let mut sessions = HashMap::new();
        sessions.insert("s1".into(), meta("s1", "claudi", AgentStatus::Blocked, 10));
        assert_eq!(pick_session(&sessions, "claudi"), Delivery0::Busy);
    }
}

/// The AppHandle for callers with no Tauri context of their own — the chat
/// tool loop runs deep inside `agent_tools::execute`, which threads no app.
/// Set once at startup; `None` only in unit tests, which is the right answer
/// there (no PTY exists to type into).
static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

pub fn set_app(app: AppHandle) {
    let _ = APP.set(app);
}

pub fn app() -> Option<&'static AppHandle> {
    APP.get()
}
