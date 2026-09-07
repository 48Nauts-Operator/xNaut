// The wake-up half of the ticket pull loop: NautBot assigns a ticket, then
// nudges the owner to go check its bucket. The nudge deliberately carries no
// work — the ticket does — so a lost or doubled nudge costs nothing but one
// redundant list call. Delivery is keystrokes into the agent's live PTY, the
// one interface every runtime accepts; the bracketed-paste form is the same
// mechanism the StdinAfterStart launcher already uses.

use std::io::Write;
use tauri::{AppHandle, Manager};

use crate::status::{AgentSessionMeta, AgentStatus};

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
///
/// `agent_is_live` is consulted only for a Working row, and only because
/// Working is the one status a dead agent can still be wearing. A zellij-backed
/// row's PTY hosts the zellij CLIENT, whose status bar repaints on its own
/// clock, and every repaint pings the status tracker; the row therefore reads
/// Working long after the agent inside the pane has gone. The rig proved it on
/// 2026-09-01: `claude` was killed with the session left up, and two wakes 90
/// seconds apart both answered `skipped_busy` with nothing delivered and
/// `ok:true`, while /api/observatory read the same agent as idle (XNAUT-268).
///
/// It takes the ROW, not the handle. The first cut of this took no argument and
/// asked a handle-scoped question; see `agent_behind_session` for why that
/// question has no correct answer.
///
/// Blocked / Permission are left alone on purpose. Those come from the agent's
/// own hooks, so they are evidence that an agent is there; only Working can be
/// manufactured by repaint noise.
pub(crate) fn pick_session(
    sessions: &std::collections::HashMap<String, AgentSessionMeta>,
    handle: &str,
    agent_is_live: impl Fn(&AgentSessionMeta) -> bool,
) -> Delivery0 {
    let handle = normalize_handle(handle);
    let best = sessions
        .values()
        .filter(|meta| normalize_handle(&meta.agent_id) == handle)
        .max_by_key(|meta| meta.started_at_ms);
    match best {
        None => Delivery0::NoSession,
        Some(meta) if accepts_input(meta.status) => Delivery0::Type(meta.session_id.clone()),
        Some(meta) if meta.status == AgentStatus::Working && !agent_is_live(meta) => {
            Delivery0::Dead(meta.session_id.clone())
        }
        Some(_) => Delivery0::Busy,
    }
}

/// Internal decision, before the PTY write has happened.
#[derive(Debug, PartialEq)]
pub(crate) enum Delivery0 {
    Type(String),
    /// A row exists for the handle and nothing is behind it. Retire the row
    /// and launch cold; the row itself is the thing that would otherwise eat
    /// every future wake.
    Dead(String),
    Busy,
    NoSession,
}

/// What a liveness probe can honestly say about one session.
#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum Liveness {
    Alive,
    Dead,
    /// The session carries no per-run link to check, so the question is left
    /// unanswered rather than guessed at.
    Unknown,
}

/// Is there an agent behind THIS session?
///
/// Pure: both facts are injected, so the rig's scenario is reproducible without
/// a zellij server or a live agent.
///
/// This replaces a HANDLE-scoped probe that walked `ps` for any claude/codex/pi
/// whose working directory was the handle's scratch workspace. That question is
/// not the question being asked, and it answered wrong in both directions:
///
/// - Wrong ALIVE, the failure the rig reproduced twice on 2026-09-01: the wakes
///   at 08:54:26Z and 08:55:40Z both answered `skipped_busy` with
///   `session_id:null` and dropped their messages permanently. The old probe
///   read the WHOLE machine's process table, so its answer never depended on
///   the agent that had just been killed. It had two ways to say "alive"
///   anyway, and both were verified by hand on macOS rather than inferred:
///   `lsof -a -p PID -d cwd -Fn` against a process owned by another user exits
///   1 with empty stdout, which the probe counted as inconclusive and resolved
///   as "alive", so a single root-owned claude anywhere pinned every session of
///   every handle to alive; and two runs of one handle share one scratch
///   workspace, so a live sibling answered for a dead run. Which of the two
///   fired on the rig cannot be settled without the rig, and it does not need
///   to be: neither is a question about this session, so no repair to that
///   probe's internals reaches either one.
/// - Wrong DEAD, flagged by its own author: an agent launched into a project
///   worktree rather than its scratch workspace matched nothing.
///
/// The run's capture file is the honest per-session link. `script(1)` opens it
/// for the life of the agent's command (agents.rs `prepare_zellij_run`), its
/// path carries this run's name, and no other session can hold it. It is also
/// cwd-independent, so it retires the ceiling above rather than moving it.
pub(crate) fn session_liveness(
    meta: &AgentSessionMeta,
    session_is_live: impl Fn(&str) -> bool,
    capture_is_held: impl Fn(&str) -> Option<bool>,
) -> Liveness {
    // ponytail: only a zellij-backed row gets a session-scoped answer. A plain
    // PTY row's agent IS the PTY's own child, so there is no second process to
    // ask about and nothing better than "alive" to say.
    let Some(name) = meta.zellij_session.as_deref() else {
        return Liveness::Unknown;
    };
    if !session_is_live(name) {
        return Liveness::Dead;
    }
    let Some(path) = meta.output_path.as_deref() else {
        return Liveness::Unknown;
    };
    match capture_is_held(path) {
        Some(true) => Liveness::Alive,
        Some(false) => Liveness::Dead,
        None => Liveness::Unknown,
    }
}

/// Does any process still hold the run's capture file open for writing?
///
/// `Some(false)` is a real answer, not a failure: lsof ran, the file is there,
/// and nothing is writing to it. `None` is the honest "cannot tell" the old
/// probe conflated with "alive" across the whole machine; here it is scoped to
/// this one file, so an unanswerable case costs this session and no other.
fn capture_is_held(path: &str) -> Option<bool> {
    // No file, no run to speak of. Never Some(false): a capture file that was
    // never created says nothing about whether an agent is working.
    if !std::path::Path::new(path).is_file() {
        return None;
    }
    let out = std::process::Command::new("lsof")
        .args(["-t", "--", path])
        .output()
        .ok()?;
    Some(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// The production wiring of `session_liveness`.
///
/// Fail-safe toward "alive", and deliberately so, but the direction now costs
/// far less than it did. A wrong "alive" drops one wake permanently, which is
/// the bug being fixed; a wrong "dead" kills a working agent's session and
/// cold-launches it again with the same message, so the work restarts rather
/// than vanishing. Retiring a live agent is still the more violent mistake, so
/// Unknown stays on the "alive" side; the fix is that Unknown is now rare and
/// local, where the old probe made it the machine-wide default.
fn agent_behind_session(meta: &AgentSessionMeta) -> bool {
    let live = crate::zellij::live_sessions();
    let liveness = session_liveness(
        meta,
        |name| live.iter().any(|session| session == name),
        capture_is_held,
    );
    liveness != Liveness::Dead
}

/// What became of the keystrokes at the PTY.
#[derive(Debug, PartialEq)]
pub(crate) enum Typed {
    Delivered,
    /// The write failed, which means the SESSION is gone, not that the wake is.
    SessionDead(String),
}

/// Writes one chunk into a session's PTY, reading any failure as a dead session
/// rather than a failed wake.
///
/// On 2026-09-01 the rig deleted a zellij session out from under the app while
/// the app still held the session record. Every wake after that returned HTTP
/// 400 `write to PTY failed: Input/output error (os error 5)`, persistent and
/// not transient, and wrote ZERO ledger entries, so the agent was permanently
/// unwakeable and invisibly so. Deleting the stale record by hand fixed it on
/// the next wake. The record was the fault, so the record is what a write error
/// retires (XNAUT-268).
///
/// Every error, not only EIO: there is no write error a PTY recovers from by
/// being written to again, and the recovery path costs one cold launch.
pub(crate) fn type_into(writer: &mut dyn Write, bytes: &[u8], what: &str) -> Typed {
    match writer.write_all(bytes).and_then(|_| writer.flush()) {
        Ok(()) => Typed::Delivered,
        Err(error) => Typed::SessionDead(format!("{what} failed: {error}")),
    }
}

/// Retires a session that cannot take the wake: drops its row, ends its run,
/// and leaves a line in the ledger. All three halves are a bug the rig found.
///
/// The ROW, because a record for a session that no longer answers eats every
/// later wake for that handle.
///
/// The RUN, because dropping the row alone leaves the old agent running. One
/// wake produced two live PIDs that had to be reaped by hand, and /api/sessions
/// (which joins pty_sessions against this map) demoted the survivor from
/// `{"Rig Two", isAgent:true}` to `{"shell", isAgent:false}` while its process
/// kept working, so the app could no longer see an agent for that handle at all
/// and a later wake cold-launched without even attempting a nudge. Ending the
/// run is what keeps those two facts consistent: no row, no process.
///
/// The LEDGER line, because a wake that fails must never be invisible.
///
/// `end_run` is injected so the kill is observable in a test.
async fn retire_dead_session(
    sessions: &crate::status::AgentSessions,
    kind: &str,
    handle: &str,
    session_id: &str,
    detail: &str,
    end_run: impl Fn(&AgentSessionMeta),
) {
    let retired = { sessions.lock().await.remove(session_id) };
    if let Some(meta) = &retired {
        end_run(meta);
    }
    crate::ledger::record(kind, handle, "", detail);
}

/// Ends a retired run for real.
///
/// ponytail: only zellij-backed runs are killed. They are the ones that outlive
/// the app, and the ones the rig caught still running after their replacement
/// had started. A plain PTY row's child dies with its pane, so there is no
/// second agent to reap there.
fn end_run(meta: &AgentSessionMeta) {
    let Some(name) = &meta.zellij_session else { return };
    if let Err(error) = crate::zellij::remove_session(name) {
        let _ = crate::debug_log::debug_log_append(vec![format!(
            "[nudge] could not end retired run {name}: {error}"
        )]);
    }
}

/// Retire a session that cannot take the wake, then launch the agent fresh with
/// the same message. The one recovery every dead-session path shares.
async fn retire_and_relaunch(
    app: &AppHandle,
    sessions: &crate::status::AgentSessions,
    kind: &str,
    handle: &str,
    session_id: &str,
    detail: &str,
    message: &str,
) -> (Delivery, Option<String>) {
    retire_dead_session(sessions, kind, handle, session_id, detail, end_run).await;
    match cold_launch(app, handle, message).await {
        Ok(sid) => (Delivery::Launched, Some(sid)),
        Err(error) => {
            let _ = crate::debug_log::debug_log_append(vec![format!(
                "[nudge] cold launch after retiring {session_id} failed: {error}"
            )]);
            (Delivery::NoSession, None)
        }
    }
}

/// Nudges the agent behind `handle` with `message`. Resolves the live session
/// from the status tracker, refuses to type into a busy one, and reports what
/// happened as data.
/// A session younger than this is still starting; silence is not death.
const YOUNG_SESSION_MINUTES: i64 = 5;

async fn session_is_young(sessions: &crate::status::AgentSessions, session_id: &str) -> bool {
    let started = sessions.lock().await.get(session_id).map(|m| m.started_at_ms);
    started.is_some_and(|at| chrono::Utc::now().timestamp_millis() - at < YOUNG_SESSION_MINUTES * 60_000)
}

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
        // A snapshot, not a held lock: the liveness probe below shells out, and
        // the status tracker is read on every PTY frame.
        let sessions = state.agent_sessions.lock().await.clone();
        pick_session(&sessions, handle, agent_behind_session)
    };
    let (delivery, session_id) = match decision {
        Delivery0::NoSession => {
            // No row does not mean no agent. The session map is app memory and
            // it has been observed empty while a run was demonstrably alive
            // (2026-09-07, tron: the XNAUT-300 codex agent had 31MB of capture
            // and no row, so this branch launched a stray into the DEFAULT
            // workspace with no ticket, branch or worktree while the real agent
            // kept working). Ask zellij, which knows what is alive whether or
            // not this app remembers, and type into it instead.
            let adopted = {
                let handle = handle.to_string();
                tokio::task::spawn_blocking(move || {
                    crate::zellij::live_session_for_handle(&handle)
                })
                .await
                .ok()
                .flatten()
            };
            if let Some(name) = adopted {
                let typed = tokio::task::spawn_blocking({
                    let name = name.clone();
                    let message = message.to_string();
                    move || crate::zellij::type_into_session(&name, &message)
                })
                .await
                .map_err(|e| e.to_string())?;
                match typed {
                    Ok(()) => {
                        crate::ledger::record("nudged", handle, "", message);
                        return Ok(serde_json::json!({
                            "ok": true,
                            "handle": normalize_handle(handle),
                            "delivery": Delivery::Typed,
                            "session_id": name,
                        }));
                    }
                    Err(why) => {
                        let _ = crate::debug_log::debug_log_append(vec![format!(
                            "[nudge] {name} is live but typing failed, cold-launching: {why}"
                        )]);
                    }
                }
            }
            match cold_launch(app, handle, message).await {
                Ok(session_id) => (Delivery::Launched, Some(session_id)),
                Err(error) => {
                    let _ = crate::debug_log::debug_log_append(vec![format!(
                        "[nudge] cold launch of {handle} failed: {error}"
                    )]);
                    (Delivery::NoSession, None)
                }
            }
        }
        Delivery0::Dead(session_id) => {
            let detail =
                format!("{session_id} reads working but no agent process is alive; cold-launching instead");
            retire_and_relaunch(
                app,
                &state.agent_sessions,
                "wake_failed",
                handle,
                &session_id,
                &detail,
                message,
            )
            .await
        }
        Delivery0::Busy => (Delivery::SkippedBusy, None),
        Delivery0::Type(session_id) => {
            // The writer is an Arc, so it comes out from under the pty_sessions
            // lock: nothing here may hold a lock across the awaits below.
            let writer = {
                let sessions = state.pty_sessions.lock().await;
                sessions.get(&session_id).map(|session| session.writer.clone())
            };
            let Some(writer) = writer else {
                // An ADOPTED row has no PTY behind it: the app that owned the
                // PTY is a previous life, but the zellij session and the agent
                // in it are alive. Until 2026-09-05 this branch cold-launched a
                // replacement, and every app restart minted one more NautBot
                // working the same board (XNAUT-289: four in one day). The
                // wake goes through zellij itself instead.
                let zellij_session = {
                    let sessions = state.agent_sessions.lock().await;
                    sessions.get(&session_id).and_then(|meta| meta.zellij_session.clone())
                };
                if let Some(name) = zellij_session.filter(|name| crate::zellij::session_exists(name)) {
                    let typed = tokio::task::spawn_blocking({
                        let message = message.to_string();
                        move || crate::zellij::type_into_session(&name, &message)
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                    match typed {
                        Ok(()) => {
                            crate::ledger::record("nudged", handle, "", message);
                            return Ok(serde_json::json!({
                                "ok": true,
                                "handle": normalize_handle(handle),
                                "delivery": Delivery::Typed,
                                "session_id": session_id,
                            }));
                        }
                        Err(why) => {
                            let _ = crate::debug_log::debug_log_append(vec![format!(
                                "[nudge] typing into {session_id} through zellij failed, relaunching: {why}"
                            )]);
                        }
                    }
                }
                // A stale row whose zellij session is gone too: a fresh cold
                // launch is the right move, not an error (XNAUT-242).
                let detail = format!("the PTY behind {session_id} is gone");
                let (delivery, sid) = retire_and_relaunch(
                    app,
                    &state.agent_sessions,
                    "wake_failed",
                    handle,
                    &session_id,
                    &detail,
                    message,
                )
                .await;
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
            let typed = {
                let payload = format!("\x1b[200~{message}\x1b[201~");
                let mut guard = writer
                    .lock()
                    .map_err(|_| "PTY writer poisoned".to_string())?;
                type_into(&mut **guard, payload.as_bytes(), "write to PTY")
            };
            // ponytail: a fixed pause, not a readiness handshake. The TUI gives
            // no signal that a paste has been absorbed, and 150ms is far below
            // the acknowledgement window that follows.
            let typed = match typed {
                Typed::Delivered => {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    let mut guard = writer
                        .lock()
                        .map_err(|_| "PTY writer poisoned".to_string())?;
                    type_into(&mut **guard, b"\r", "submit after paste")
                }
                dead => dead,
            };
            match typed {
                // A write error used to leave the caller with an HTTP 400 and
                // the app with the same doomed record, so every later wake
                // failed identically and silently (XNAUT-268).
                Typed::SessionDead(why) => {
                    retire_and_relaunch(
                        app,
                        &state.agent_sessions,
                        "wake_failed",
                        handle,
                        &session_id,
                        &why,
                        message,
                    )
                    .await
                }
                // ACKNOWLEDGEMENT, not optimism (XNAUT-263). "Typed" used to
                // mean "bytes were written to a PTY", which is not the same as
                // "an agent took the task": on the tron rig a wake was typed
                // into a session whose agent had finished its turn, reported
                // success, and reached nobody for three hours. An agent that
                // received keystrokes produces output within seconds; if none
                // appears, the session is treated as dead and the work is
                // cold-launched instead.
                Typed::Delivered => {
                    if awaited_output(&state.agent_sessions, &session_id).await {
                        (Delivery::Typed, Some(session_id))
                    } else if session_is_young(&state.agent_sessions, &session_id).await {
                        // A session still bringing its TUI up answers nothing
                        // for a while and is not dead. On 2026-09-06 the sweep
                        // typed into a two-minute-old grok, heard nothing,
                        // retired it and launched a third agent for the same
                        // ticket. The keystrokes are in its buffer; leave it.
                        crate::ledger::record(
                            "wake_unacknowledged_young",
                            handle,
                            "",
                            &format!("{session_id} is under {} minutes old and has not answered yet; left alone", YOUNG_SESSION_MINUTES),
                        );
                        (Delivery::Typed, Some(session_id))
                    } else {
                        let detail = format!(
                            "{session_id} did not answer the keystrokes; cold-launching instead"
                        );
                        retire_and_relaunch(
                            app,
                            &state.agent_sessions,
                            "wake_unacknowledged",
                            handle,
                            &session_id,
                            &detail,
                            message,
                        )
                        .await
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
            runtime_id: None,
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
        assert_eq!(pick_session(&sessions, "@claudi", |_| true), Delivery0::NoSession);
    }

    #[test]
    fn an_idle_session_is_typed_into() {
        let mut sessions = HashMap::new();
        sessions.insert("s1".into(), meta("s1", "claudi", AgentStatus::Idle, 10));
        assert_eq!(pick_session(&sessions, "@Claudi", |_| true), Delivery0::Type("s1".into()));
    }

    #[test]
    fn a_working_session_is_never_typed_into() {
        let mut sessions = HashMap::new();
        sessions.insert("s1".into(), meta("s1", "claudi", AgentStatus::Working, 10));
        assert_eq!(pick_session(&sessions, "claudi", |_| true), Delivery0::Busy);
    }

    #[test]
    fn the_newest_session_wins() {
        let mut sessions = HashMap::new();
        sessions.insert("old".into(), meta("old", "claudi", AgentStatus::Idle, 10));
        sessions.insert("new".into(), meta("new", "claudi", AgentStatus::Idle, 20));
        assert_eq!(pick_session(&sessions, "claudi", |_| true), Delivery0::Type("new".into()));
    }

    #[test]
    fn a_blocked_session_is_skipped_not_answered() {
        // Blocked means the agent is showing its own prompt (an inbox ask, a
        // permission screen). A nudge typed there could answer that prompt.
        let mut sessions = HashMap::new();
        sessions.insert("s1".into(), meta("s1", "claudi", AgentStatus::Blocked, 10));
        assert_eq!(pick_session(&sessions, "claudi", |_| true), Delivery0::Busy);
    }

    #[test]
    fn a_working_row_with_no_agent_behind_it_is_retired_not_skipped() {
        // The rig killed rigtwo's `claude` and left its zellij session up. The
        // zellij client kept repainting the PTY, the row kept reading Working,
        // and two wakes 90 seconds apart both answered skipped_busy with
        // nothing delivered while /api/observatory read the agent as idle
        // (XNAUT-268).
        let mut sessions = HashMap::new();
        sessions.insert("s1".into(), meta("s1", "rigtwo", AgentStatus::Working, 10));
        assert_eq!(
            pick_session(&sessions, "rigtwo", |_| false),
            Delivery0::Dead("s1".into()),
            "Working is not proof of a working agent"
        );
        // And the busy guard still holds when the agent is really there: a
        // wake typed mid-turn garbles the agent's own input.
        assert_eq!(pick_session(&sessions, "rigtwo", |_| true), Delivery0::Busy);
    }

    // ─── The production failure ─────────────────────────────────────────────
    //
    // Everything above passed while the feature did not work on the rig, so
    // none of it is evidence. These exercise the REAL probe against real files
    // and real processes, in the shape the rig produced.

    /// A zellij-backed row, the way a durable cold launch registers one.
    fn zellij_meta(session_id: &str, handle: &str, name: &str, capture: &str) -> AgentSessionMeta {
        let mut row = meta(session_id, handle, AgentStatus::Working, 10);
        row.zellij_session = Some(name.into());
        row.output_path = Some(capture.into());
        row
    }

    /// Holds a capture file open the way `script(1)` does while its agent runs,
    /// and lets go when killed the way the rig killed `claude`.
    struct Writer(std::process::Child);
    impl Writer {
        fn holding(path: &str) -> Self {
            std::fs::write(path, b"").expect("capture file");
            let child = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("exec 9>>'{path}'; exec sleep 120"))
                .spawn()
                .expect("stand-in agent spawns");
            // The handle has to be open before anything asks about it.
            for _ in 0..50 {
                if capture_is_held(path) == Some(true) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            Self(child)
        }
        fn kill_9(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    impl Drop for Writer {
        fn drop(&mut self) {
            self.kill_9();
        }
    }

    #[test]
    fn a_killed_agent_behind_a_live_session_is_retired_not_skipped() {
        // THE RIG SCENARIO, end to end through the real probe. A durable run
        // for @rigtwo, its zellij session still up and still repainting, and
        // its `claude` killed with -9. On 2026-09-01 this answered
        // skipped_busy at 08:54:26Z and again at 08:55:40Z, and both messages
        // were dropped permanently.
        let dir = std::env::temp_dir().join(format!("xnaut-nudge-rig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let capture = dir.join("xnaut-rigtwo-deadbeef.jsonl");
        let capture = capture.to_string_lossy().into_owned();

        let mut agent = Writer::holding(&capture);
        let row = zellij_meta("s1", "rigtwo", "xnaut-rigtwo-deadbeef", &capture);
        let mut sessions = HashMap::new();
        sessions.insert("s1".into(), row.clone());

        // The zellij session is up for the whole test, exactly as the rig left
        // it; only the agent inside it dies. `session_is_live` is pinned true
        // so the session's own liveness cannot be what carries the verdict.
        let alive = |meta: &AgentSessionMeta| {
            session_liveness(meta, |_| true, capture_is_held) != Liveness::Dead
        };

        assert_eq!(
            pick_session(&sessions, "rigtwo", alive),
            Delivery0::Busy,
            "a working agent must still be left alone"
        );

        agent.kill_9();

        assert_eq!(
            pick_session(&sessions, "rigtwo", alive),
            Delivery0::Dead("s1".into()),
            "the wake was dropped for good because a dead agent read as busy"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_handles_dead_session_is_not_saved_by_its_live_one() {
        // WHY the old probe could not be right. It took a handle and asked the
        // whole machine "is any claude running in this handle's scratch
        // workspace", so two runs of one handle shared a single answer and the
        // live one spoke for the dead one. No fix to that probe's internals
        // reaches this case; only the row can tell the two runs apart.
        let dir = std::env::temp_dir().join(format!("xnaut-nudge-two-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let live_capture = dir.join("xnaut-rigtwo-11111111.jsonl");
        let live_capture = live_capture.to_string_lossy().into_owned();
        let dead_capture = dir.join("xnaut-rigtwo-22222222.jsonl");
        let dead_capture = dead_capture.to_string_lossy().into_owned();

        // One run of @rigtwo genuinely working, one killed. Same handle, same
        // scratch workspace, two different runs.
        let _working = Writer::holding(&live_capture);
        std::fs::write(&dead_capture, b"").expect("capture file");

        let older = zellij_meta("live", "rigtwo", "xnaut-rigtwo-11111111", &live_capture);
        let mut newer = zellij_meta("dead", "rigtwo", "xnaut-rigtwo-22222222", &dead_capture);
        // The wake targets the newest run, which is the one that was killed.
        newer.started_at_ms = 20;
        let alive = |meta: &AgentSessionMeta| {
            session_liveness(meta, |_| true, capture_is_held) != Liveness::Dead
        };

        assert_eq!(
            session_liveness(&older, |_| true, capture_is_held),
            Liveness::Alive,
            "the run that is working must read alive"
        );
        assert_eq!(
            session_liveness(&newer, |_| true, capture_is_held),
            Liveness::Dead,
            "the run that was killed must read dead, however busy its sibling is"
        );

        let mut sessions = HashMap::new();
        sessions.insert("live".into(), older);
        sessions.insert("dead".into(), newer);
        assert_eq!(
            pick_session(&sessions, "rigtwo", alive),
            Delivery0::Dead("dead".into()),
            "a handle-scoped answer let the live run vouch for the dead one"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_capture_file_nothing_holds_reads_dead_and_a_missing_one_reads_unknown() {
        // The two answers `lsof` gives, kept apart on purpose. The old probe
        // ran `lsof -a -p PID -d cwd -Fn` across every claude/codex/pi on the
        // machine and folded "could not read that process" into "alive"; a
        // single root-owned agent process pinned every handle to alive
        // forever, which is why the rig's two wakes were identical.
        let dir = std::env::temp_dir().join(format!("xnaut-nudge-cap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let present = dir.join("present.jsonl");
        std::fs::write(&present, b"some output").expect("capture file");
        assert_eq!(
            capture_is_held(&present.to_string_lossy()),
            Some(false),
            "a file nothing is writing to is a real answer, not a shrug"
        );
        let missing = dir.join("never-created.jsonl");
        assert_eq!(
            capture_is_held(&missing.to_string_lossy()),
            None,
            "a run that never opened a capture file cannot be called dead"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_ghost_row_whose_zellij_session_ended_is_dead() {
        // The session is gone but the row survived it. Nothing is behind this
        // row by definition, and the row is what eats every later wake.
        let row = zellij_meta("s1", "rigtwo", "xnaut-rigtwo-deadbeef", "/nonexistent.jsonl");
        assert_eq!(
            session_liveness(&row, |_| false, |_| Some(true)),
            Liveness::Dead,
            "no session, no agent, whatever a stale capture file says"
        );
    }

    #[test]
    fn a_plain_pty_row_is_left_alone() {
        // ponytail: a row with no zellij session has no second process to ask
        // about; its agent is the PTY's own child. Unknown, so the busy guard
        // holds and behaviour there is unchanged.
        let row = meta("s1", "rigtwo", AgentStatus::Working, 10);
        assert_eq!(
            session_liveness(&row, |_| false, |_| Some(false)),
            Liveness::Unknown,
        );
        let mut sessions = HashMap::new();
        sessions.insert("s1".into(), row);
        assert_eq!(
            pick_session(&sessions, "rigtwo", agent_behind_session),
            Delivery0::Busy,
        );
    }

    /// A writer that fails the way a PTY whose session was deleted does.
    struct DeadPty;
    impl Write for DeadPty {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from_raw_os_error(5)) // EIO
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn an_eio_write_retires_the_record_and_leaves_a_ledger_line() {
        // Deleting a zellij session out from under the app left the record
        // behind: every wake returned HTTP 400 `write to PTY failed:
        // Input/output error (os error 5)`, forever, and wrote ZERO ledger
        // entries, so the agent was unwakeable and nothing showed it
        // (XNAUT-268).
        let _guard = crate::ledger::scratch("nudge-eio");
        let Typed::SessionDead(why) = type_into(&mut DeadPty, b"wake up", "write to PTY") else {
            panic!("an EIO write means the session is dead, not that the wake failed");
        };
        assert!(why.contains("write to PTY failed"), "{why}");

        let sessions: crate::status::AgentSessions = Default::default();
        sessions
            .lock()
            .await
            .insert("s1".into(), meta("s1", "rigone", AgentStatus::Working, 10));
        retire_dead_session(&sessions, "wake_failed", "rigone", "s1", &why, |_| {}).await;

        assert!(
            sessions.lock().await.is_empty(),
            "the doomed record survived, so the next wake fails identically"
        );
        let ledger = crate::ledger::ledger_recent(Some(10));
        assert!(
            ledger
                .iter()
                .any(|entry| entry.kind == "wake_failed" && entry.agent == "rigone"),
            "a wake that fails must never be invisible: {ledger:?}"
        );
    }

    #[tokio::test]
    async fn retiring_a_session_ends_its_run() {
        // One wake, two live agents: the cold launch that followed an
        // unacknowledged nudge left the ORIGINAL agent running, and dropping
        // its row demoted it in /api/sessions from an agent to a plain shell
        // while its process kept going, so nothing could address it again
        // (XNAUT-268).
        let _guard = crate::ledger::scratch("nudge-retire");
        let sessions: crate::status::AgentSessions = Default::default();
        let mut row = meta("s1", "rigtwo", AgentStatus::Working, 10);
        row.zellij_session = Some("xnaut-rigtwo-ba2b93c4".into());
        sessions.lock().await.insert("s1".into(), row);

        let ended = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen = ended.clone();
        retire_dead_session(
            &sessions,
            "wake_unacknowledged",
            "rigtwo",
            "s1",
            "no answer to the keystrokes",
            move |meta| {
                seen.lock()
                    .unwrap()
                    .push(meta.zellij_session.clone().unwrap_or_default())
            },
        )
        .await;

        assert_eq!(
            ended.lock().unwrap().as_slice(),
            ["xnaut-rigtwo-ba2b93c4"],
            "the superseded run kept running after its replacement started"
        );
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
