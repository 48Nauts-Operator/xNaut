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
            let session = sessions
                .get(&session_id)
                .ok_or_else(|| format!("status row exists but PTY {session_id} is gone"))?;
            // Bracketed paste so multi-line text lands as one unit, then Enter.
            let payload = format!("\x1b[200~{message}\x1b[201~\r");
            let mut writer = session
                .writer
                .lock()
                .map_err(|_| "PTY writer poisoned".to_string())?;
            writer
                .write_all(payload.as_bytes())
                .and_then(|_| writer.flush())
                .map_err(|e| format!("write to PTY failed: {e}"))?;
            (Delivery::Typed, Some(session_id))
        }
    };
    Ok(serde_json::json!({
        "ok": true,
        "handle": normalize_handle(handle),
        "delivery": delivery,
        "session_id": session_id,
    }))
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
            handle,
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
    Ok(response.session_id)
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
