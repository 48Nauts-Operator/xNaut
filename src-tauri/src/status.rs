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

const IDLE_AFTER_MS: i64 = 2_000;
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
}

pub type AgentSessions = Arc<Mutex<HashMap<String, AgentSessionMeta>>>;

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
    };
    {
        let mut map = sessions.lock().await;
        map.insert(session_id.to_string(), meta.clone());
    }
    let _ = app.emit("agent-status-changed", &meta);
}

/// Pings on every PTY output frame for an agent session. If the session isn't
/// in the agent registry (e.g. it's a plain shell), this is a no-op.
pub async fn ping_session_output(sessions: &AgentSessions, app: &AppHandle, session_id: &str) {
    let now = now_ms();
    let updated = {
        let mut map = sessions.lock().await;
        match map.get_mut(session_id) {
            Some(meta) => {
                meta.last_output_at_ms = now;
                let was_working = meta.status == AgentStatus::Working;
                if !was_working {
                    meta.status = AgentStatus::Working;
                    meta.status_changed_at_ms = now;
                    Some(meta.clone())
                } else {
                    None
                }
            }
            None => None,
        }
    };
    if let Some(meta) = updated {
        let _ = app.emit("agent-status-changed", &meta);
    }
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
        let _ = app.emit("agent-status-changed", &meta);
    }
}

/// Sets the session to an arbitrary state. Used by the Phase 5 hook listener
/// for Blocked / Waiting / Permission / Idle transitions that aren't otherwise
/// derivable from PTY output.
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
                Some(meta.clone())
            }
            _ => None,
        }
    };
    if let Some(meta) = updated {
        let _ = app.emit("agent-status-changed", &meta);
    }
}

/// Marks a session interrupted (user-cancelled / agent crashed without hook).
/// Mirrors Orca's narrow interrupt-synthesis fallback.
pub async fn mark_session_interrupted(
    sessions: &AgentSessions,
    app: &AppHandle,
    session_id: &str,
) {
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
        let _ = app.emit("agent-status-changed", &meta);
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
                    if meta.status == AgentStatus::Working
                        && now - meta.last_output_at_ms >= IDLE_AFTER_MS
                    {
                        meta.status = AgentStatus::Idle;
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
}
