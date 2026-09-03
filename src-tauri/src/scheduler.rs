// v1.6 Automations: user-defined scheduled agent runs (modeled on Orca's automation modal).
//
// The backend evaluates schedules and prechecks AND opens the session. It used
// to only emit "automation://fire" and leave the launch to a listener in the
// Automations panel, which the tron rig took apart on 2026-09-01: the cadence
// was exact and all four fires ran, but the listener only existed once the
// panel had been opened, so on a fresh app every fire was a silent no-op; the
// launch used agents.toml runtime ids and could not target a profile handle;
// it was non-durable and never reaped; and it bypassed the composer, so the
// agent got the bare task with no Foundation. Doing the launch here, through
// the same agent_profile_launch the wake path uses, closes all four.

use chrono::{DateTime, Datelike, Local, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

fn default_precheck_timeout() -> u64 {
    60
}

fn default_grace_hours() -> u64 {
    12
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    /// uuid v4
    pub id: String,
    pub name: String,
    pub prompt: String,
    /// Shell cmd; the run is skipped when it prints nothing or exits non-zero.
    #[serde(default)]
    pub precheck: Option<String>,
    #[serde(default = "default_precheck_timeout")]
    pub precheck_timeout_secs: u64,
    pub project_path: String,
    /// "worktree" (run in project_path) | "new_run" (frontend creates fresh worktree).
    pub workspace: String,
    /// Base branch for new_run.
    #[serde(default)]
    pub branch: String,
    /// The agent to run. Named for the agents.toml runtime ids it originally
    /// held; what the owner actually picks (and what the wake API and Agent
    /// Space use) is a profile handle, so it is resolved through
    /// `resolve_spoken_handle` before every launch.
    pub agent_id: String,
    /// "fresh" | "reuse"
    pub session_mode: String,
    /// See `is_due` for supported forms.
    pub schedule: String,
    /// Skip if fired within this window (daily/weekdays only; 0 disables).
    #[serde(default = "default_grace_hours")]
    pub grace_hours: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// RFC3339
    #[serde(default)]
    pub last_fired: Option<String>,
    /// The zellij session the last run is still living in. Scheduler-owned:
    /// it is what the next fire reaps, so an automation cannot accumulate
    /// agents. Durable runs outlive the app, which is why this is persisted.
    #[serde(default)]
    pub last_session: Option<String>,
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn automations_path() -> PathBuf {
    // Same override the ledger uses, and for the same reason: the ordering
    // rules in `tick_with` are only testable against a scratch store.
    if let Ok(path) = std::env::var("XNAUT_AUTOMATIONS_PATH") {
        return PathBuf::from(path);
    }
    config_dir().join("automations.json")
}

/// Loads automations from `~/.config/xnaut/automations.json`. Missing file or
/// parse error both yield an empty list (errors are eprintln'd, never fatal).
pub fn load_automations() -> Vec<Automation> {
    let path = automations_path();
    if !path.exists() {
        return Vec::new();
    }
    let body = match std::fs::read_to_string(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[scheduler] failed to read {}: {e}", path.display());
            return Vec::new();
        }
    };
    match serde_json::from_str::<Vec<Automation>>(&body) {
        Ok(autos) => autos,
        Err(e) => {
            eprintln!("[scheduler] failed to parse {}: {e}", path.display());
            Vec::new()
        }
    }
}

pub fn save_automations(automations: &[Automation]) -> Result<(), String> {
    let path = automations_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    }
    let serialized = serde_json::to_string_pretty(automations)
        .map_err(|e| format!("failed to serialize automations: {e}"))?;
    std::fs::write(&path, serialized)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))
}

// ─── Schedule evaluation ─────────────────────────────────────────────────────

/// Whether a schedule is due at `now` given when it last fired. Supported forms:
/// "hourly", "daily@HH:MM", "weekdays@HH:MM", "every:Nm", "every:Nh".
/// Unknown formats are never due.
pub fn is_due(schedule: &str, now: DateTime<Local>, last_fired: Option<DateTime<Local>>) -> bool {
    if schedule == "hourly" {
        return match last_fired {
            None => true,
            Some(last) => now.signed_duration_since(last) >= chrono::Duration::hours(1),
        };
    }
    if let Some(hhmm) = schedule.strip_prefix("daily@") {
        return due_at_time(hhmm, now, last_fired, false);
    }
    if let Some(hhmm) = schedule.strip_prefix("weekdays@") {
        return due_at_time(hhmm, now, last_fired, true);
    }
    if let Some(spec) = schedule.strip_prefix("every:") {
        return due_every(spec, now, last_fired);
    }
    eprintln!("[scheduler] unknown schedule format: {schedule:?}");
    false
}

/// daily/weekdays: due once we're past today's HH:MM and haven't fired today yet.
fn due_at_time(
    hhmm: &str,
    now: DateTime<Local>,
    last_fired: Option<DateTime<Local>>,
    weekdays_only: bool,
) -> bool {
    let Ok(target) = NaiveTime::parse_from_str(hhmm, "%H:%M") else {
        eprintln!("[scheduler] bad HH:MM in schedule: {hhmm:?}");
        return false;
    };
    if weekdays_only && matches!(now.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    if now.time() < target {
        return false;
    }
    match last_fired {
        None => true,
        Some(last) => last.date_naive() < now.date_naive(),
    }
}

/// every:Nm / every:Nh — due once the interval has elapsed since last fire
/// (or immediately if never fired).
fn due_every(spec: &str, now: DateTime<Local>, last_fired: Option<DateTime<Local>>) -> bool {
    let n: i64 = match spec
        .get(..spec.len().saturating_sub(1))
        .and_then(|digits| digits.parse().ok())
    {
        Some(n) if n > 0 => n,
        _ => {
            eprintln!("[scheduler] bad interval in schedule: every:{spec}");
            return false;
        }
    };
    let interval = match spec.chars().last() {
        Some('m') => chrono::Duration::minutes(n),
        Some('h') => chrono::Duration::hours(n),
        _ => {
            eprintln!("[scheduler] bad interval unit in schedule: every:{spec}");
            return false;
        }
    };
    match last_fired {
        None => true,
        Some(last) => now.signed_duration_since(last) >= interval,
    }
}

/// Grace window: an extra guard against double-fires for the at-a-time forms
/// (daily/weekdays). hourly/every already encode their spacing in the schedule
/// itself, so grace doesn't apply there. grace_hours == 0 disables the guard.
fn blocked_by_grace(
    schedule: &str,
    grace_hours: u64,
    now: DateTime<Local>,
    last_fired: Option<DateTime<Local>>,
) -> bool {
    if grace_hours == 0 {
        return false;
    }
    if !(schedule.starts_with("daily@") || schedule.starts_with("weekdays@")) {
        return false;
    }
    match last_fired {
        None => false,
        Some(last) => now.signed_duration_since(last) < chrono::Duration::hours(grace_hours as i64),
    }
}

fn parse_last_fired(raw: Option<&str>) -> Option<DateTime<Local>> {
    raw.and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Local))
            .map_err(|e| eprintln!("[scheduler] bad last_fired timestamp {s:?}: {e}"))
            .ok()
    })
}

// ─── Precheck ────────────────────────────────────────────────────────────────

/// Runs a precheck shell command in `cwd`. Ok(true) only when the command exits
/// successfully AND prints something to stdout (after trim). Timeout => Ok(false).
/// Err only when the command can't be spawned at all.
pub async fn run_precheck(cmd: &str, cwd: &str, timeout_secs: u64) -> Result<bool, String> {
    let output_fut = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(timeout_secs), output_fut).await {
        Err(_elapsed) => Ok(false),
        Ok(Err(e)) => Err(format!("failed to spawn precheck: {e}")),
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            Ok(out.status.success() && !stdout.trim().is_empty())
        }
    }
}

// ─── Firing a run ────────────────────────────────────────────────────────────

/// What a started run left behind, so the next fire can find it again.
#[derive(Debug, Clone, PartialEq)]
pub struct FireOutcome {
    pub session_id: String,
    /// Present when the run is zellij-backed, which a durable launch is
    /// whenever zellij is installed. This is what the next fire reaps.
    pub zellij_session: Option<String>,
}

/// Everything a fire will do, decided before any of it happens.
struct FirePlan {
    /// The previous run's zellij session, ended once the replacement is known
    /// to be admissible.
    reap: Option<String>,
    request: crate::agent_profiles::LaunchAgentProfileRequest,
}

/// What one fire actually did, both halves of it.
///
/// The launch outcome alone is not enough to persist the automation correctly:
/// `last_session` has to be dropped whenever the previous run is gone, INCLUDING
/// when the replacement then failed to start. Leaving it set is what made every
/// later tick log "ended the previous run in xnaut-rigtwo-e99ea4dd" for a
/// session that had been dead for over a minute.
struct FireReport {
    /// The previous run is no longer there, so `last_session` names nothing.
    reaped: bool,
    outcome: Result<FireOutcome, String>,
}

impl FireReport {
    /// A fire that never got as far as touching anything.
    fn refused(error: String) -> Self {
        Self {
            reaped: false,
            outcome: Err(error),
        }
    }
}

/// Builds the plan for one fire. Pure, and the resolver is a parameter, so the
/// three properties the rig found wrong are checkable without a PTY.
fn plan_fire<R>(auto: &Automation, resolve: R) -> Result<FirePlan, String>
where
    R: FnOnce(&str) -> Result<String, String>,
{
    // `rigtwo` is a real profile handle and the fire died on it with
    // "unknown agent id: rigtwo", because agent_launch only knows the six
    // agents.toml runtime ids. Resolve what the owner typed the way the wake
    // path does, so a handle and a display name both land on the same agent.
    let handle = resolve(&auto.agent_id)?;
    Ok(FirePlan {
        reap: auto
            .last_session
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string),
        request: crate::agent_profiles::LaunchAgentProfileRequest {
            handle,
            worktree_path: auto.project_path.clone(),
            prompt: Some(auto.prompt.clone()),
            conversation_mode: false,
            conversation_id: None,
            // NEVER a resume. agent_profile_launch skips the composer on a
            // resume, and that is exactly the bug: the rig's automation-run
            // agent carried the bare task in its argv while a wake-run one
            // carried "# xNAUT Foundation (v2)". No foundation, no handle,
            // no Mesh inbox, no session identity.
            resume: false,
            cols: Some(200),
            rows: Some(50),
            // A scheduled run must survive the app quitting, like a cold wake.
            // Non-durable was the other half of the orphan problem: the four
            // rig runs all died on restart having never been cleaned up.
            durable: Some(true),
        },
    })
}

/// What a reap attempt found.
#[derive(Debug, PartialEq)]
enum Reaped {
    /// A live session was there and is now ended.
    Ended,
    /// Nothing was live under that name; the automation was pointing at a
    /// corpse.
    NothingThere,
    /// The session is still there. Something has to try again.
    Failed(String),
}

/// Puts a reap attempt in the ledger under `kind`, and says whether `session`
/// still names anything.
///
/// `NothingThere` records NOTHING, which is the whole point: `last_session` was
/// never cleared, so every tick after the first reap logged "ended the previous
/// run in xnaut-rigtwo-e99ea4dd" for a session that had been dead for over a
/// minute. A ledger row that asserts work was ended when nothing was ended is
/// worse than no row at all.
///
/// `kind` and `detail` are the caller's, because there are two reasons to end a
/// run and the ledger has to say which: an automation clearing the way for its
/// next fire, and the idle reaper collecting a run that finished and sat.
fn record_reap(outcome: &Reaped, kind: &str, detail: &str, session: &str, agent: &str) -> bool {
    match outcome {
        Reaped::Ended => {
            crate::ledger::record(kind, agent, "", detail);
            true
        }
        // Still cleared: a name that names nothing must not be reaped again
        // next minute, and must not be reported as a reap either.
        Reaped::NothingThere => true,
        Reaped::Failed(e) => {
            eprintln!("[scheduler] could not reap {session}: {e}");
            false
        }
    }
}

/// Ends a zellij-backed run and frees the ceiling slot it was holding.
///
/// Four fires on the rig left four sessions and at least three idle `claude`
/// processes, and nothing ever collected them; at `every:1m` that is roughly
/// sixty orphaned agents an hour.
///
/// The status row has to go too. The ceiling counts ROWS, not zellij sessions,
/// so a killed run still occupying a Working row is what refused the
/// replacement 0.4ms after the kill (09:07:22.859956). Interrupted is both true
/// and outside `counts_as_live`.
///
/// ponytail: kills the zellij session only. The PTY that hosted its client
/// exits with it.
async fn reap_session(
    app: &AppHandle,
    session: String,
    agent: &str,
    kind: &str,
    detail: &str,
) -> bool {
    let name = session.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        if !crate::zellij::list_live_sessions().contains(&name) {
            // Best effort: drop an EXITED remnant so the name stops showing up.
            let _ = crate::zellij::remove_session(&name);
            return Reaped::NothingThere;
        }
        match crate::zellij::remove_session(&name) {
            Ok(()) => Reaped::Ended,
            Err(e) => Reaped::Failed(e),
        }
    })
    .await
    .unwrap_or_else(|e| Reaped::Failed(format!("reap task panicked: {e}")));

    let cleared = record_reap(&outcome, kind, detail, &session, agent);
    if cleared {
        let state = tauri::Manager::state::<crate::state::AppState>(app);
        for id in tracked_in(&state.agent_sessions, &session).await {
            crate::status::mark_session_interrupted(&state.agent_sessions, app, &id).await;
        }
    }
    cleared
}

/// Whether a status row is the run hosted in `zellij`. Adopted rows are keyed
/// by the session name itself, launched ones by a PTY uuid.
fn hosted_in(meta: &crate::status::AgentSessionMeta, zellij: &str) -> bool {
    meta.zellij_session.as_deref() == Some(zellij) || meta.session_id == zellij
}

/// The tracked sessions hosted in one zellij session.
async fn tracked_in(sessions: &crate::status::AgentSessions, zellij: &str) -> Vec<String> {
    sessions
        .lock()
        .await
        .values()
        .filter(|meta| hosted_in(meta, zellij))
        .map(|meta| meta.session_id.clone())
        .collect()
}

// ─── The idle reaper (XNAUT-262) ─────────────────────────────────────────────

/// How long a run may sit with nothing written to its capture file before it is
/// collected.
///
/// There is no termination contract without this. An interactive `claude` never
/// exits when it finishes a task; it returns to its prompt and waits forever, so
/// every durable run is immortal and they accumulate. Measured on the test rig
/// 2026-09-02: seven zellij sessions, the oldest 1 day 6 hours old, every one an
/// agent that had finished its work long ago. This machine had three
/// `xnaut-claude-*` sessions over two days old on the same date. Making the
/// launches durable made the pile survive app restarts too.
///
/// Four hours, chosen to be forgiving rather than tidy. An agent left idle
/// before a meeting or over lunch is still there afterwards; one left at the end
/// of a day is collected overnight instead of accumulating for a week. It is an
/// order of magnitude under the leak actually measured, so the leak is still
/// collected inside the same working day. Somebody genuinely sitting with an
/// agent is covered by the attach check in `finished_and_idle`, not by this
/// clock, which only has to survive them stepping away from the keyboard.
const IDLE_REAP_AFTER_MS: i64 = 4 * 60 * 60 * 1_000;

/// How many of xNAUT's own PTY panes are hosting this zellij session right now.
///
/// This is the number the attach gate has to subtract, and the reason the reaper
/// collected nothing in production. EVERY path that opens a session in the app
/// runs `zellij attach <name>` inside a PTY pane (zellij.rs, `launch_command`),
/// and that process is a zellij client for as long as the tab is open. It is
/// created when the run starts and it is not related to the agent's command, so
/// it long outlives it. The app is therefore the normal reason a finished run
/// reports a connected client, and the gate was refusing on the app's own
/// reflection.
///
/// ponytail: counts the records and trusts `session_name` on them, rather than
/// hunting the client processes and asking which are descendants of this app. A
/// record whose `zellij attach` fell through to the frontend's `|| exec sh`
/// fallback still names the session it failed to reach, so it would be counted
/// as a client it does not hold. Reaching that needs the same run id
/// (`xnaut-<handle>-<8 hex>`) to come back from the dead AND somebody to attach
/// to it from outside, which is not a path worth more code than this comment.
type PtySessions = std::sync::Arc<
    tokio::sync::Mutex<
        std::collections::HashMap<String, std::sync::Arc<crate::state::PtySession>>,
    >,
>;

async fn hosting_ptys(sessions: &PtySessions, zellij: &str) -> u32 {
    sessions
        .lock()
        .await
        .values()
        .filter(|pty| pty.session_name.as_deref() == Some(zellij))
        .count() as u32
}

/// Has this run finished its work and sat at its prompt long enough to collect?
///
/// Every fact is injected, so the decision is testable without a zellij server,
/// and every gate is a way to say NO. That direction is the load-bearing choice:
/// leaving a finished session up for another hour costs a slot, while ending a
/// working agent destroys work nothing can recover. So an unreadable answer
/// keeps the session, every time.
fn finished_and_idle(
    name: &str,
    wrote_at: Option<i64>,
    now: i64,
    tracked_busy: bool,
    clients: Option<u32>,
    own_clients: u32,
) -> bool {
    // Only runs xNAUT launched. The owner's own zellij sessions share the same
    // server and are none of this reaper's business; on 2026-09-02 those
    // (`cx-*`) were the majority of live sessions on the machine.
    if !name.starts_with("xnaut-") {
        return false;
    }
    // EVIDENCE, not a guess. script(1) holds the capture file open for the life
    // of the agent's command (agents.rs `prepare_zellij_run`), so its mtime
    // stops the moment the agent stops writing, and a zellij status-bar repaint
    // never touches it. status.rs reads the same signal for exactly this reason.
    // No capture file means nothing here can establish "finished", so nothing is
    // ended.
    let Some(wrote_at) = wrote_at else {
        return false;
    };
    if now - wrote_at < IDLE_REAP_AFTER_MS {
        return false;
    }
    // A row the app still tracks gets the last word when it says the agent is
    // busy. Working is mid-task; Blocked, Permission and Waiting come from the
    // agent's own hooks and mean a human owes it an answer, so ending one throws
    // away work that was a keystroke from continuing.
    if tracked_busy {
        return false;
    }
    // Somebody OTHER THAN THIS APP is looking at the pane.
    //
    // Against zero this gate could never fire in normal use, and in production
    // it never did: seven finished runs on the rig, capture files silent for
    // nine hours, every one of them reporting `connected_clients 1`, nobody
    // sitting in any of them, `idle_reaped` rows in the ledger zero. The client
    // is xNAUT's own: a tab hosting a session runs `zellij attach` in a PTY, and
    // that client is tied to the tab, not to the agent's command. A durable run
    // with its tab open is the NORMAL state, so the gate refused every time and
    // the whole termination contract was dead on arrival.
    //
    // What is subtracted is only what the app can positively account for as its
    // own (`hosting_ptys`). A client left over after that is a real external
    // viewer and still keeps the session, so the gate went from unfireable to
    // meaningful without becoming a guess.
    //
    // The count is worth trusting in both directions. Probed against the
    // installed zellij 0.44 on 2026-09-03: a client held inside a PTY reads
    // `connected_clients 1` exactly like a human's terminal attach, and killing
    // it drops the count to 0 within five seconds. So a surplus client is live,
    // not a ghost. The same probe rules out focus as the discriminator the
    // measurement seemed to offer: `other_focused_clients` was 1 for the
    // PTY-held client too, and absent whenever no client was attached, so it
    // carries the same information as the count and separates nothing.
    //
    // `None` is unreadable metadata, not an empty room, and it keeps the
    // session.
    clients.is_some_and(|attached| attached <= own_clients)
}

/// Does any tracked row hosted in this zellij session still count as live?
///
/// `counts_as_live` on purpose: status.rs keeps ONE definition of which statuses
/// occupy a slot, and a second copy drifting apart is how a reap frees something
/// the gate still counts.
async fn any_live_row(sessions: &crate::status::AgentSessions, zellij: &str) -> bool {
    sessions
        .lock()
        .await
        .values()
        .filter(|meta| hosted_in(meta, zellij))
        .any(|meta| crate::status::counts_as_live(meta.status))
}

/// The agent handle inside a run's session name (`xnaut-<handle>-<run id>`), for
/// the ledger line. Empty when the name does not carry one.
fn handle_in(session: &str) -> String {
    session
        .strip_prefix("xnaut-")
        .and_then(|rest| rest.rsplit_once('-'))
        .map(|(handle, _run)| handle.to_string())
        .unwrap_or_default()
}

/// Ends every xNAUT run that finished its task and has been idling since.
///
/// Driven off the LIVE SESSION LIST, not off the status tracker, because the
/// tracker is not an index of what is running: `spawn_decay_task` drops a row 30
/// minutes after its last status change, well inside this grace period, and the
/// measured pile is made of runs that outlived the app entirely. The tracker is
/// still consulted per session, for the rows it does hold.
async fn reap_idle_runs(app: &AppHandle) {
    let live = tokio::task::spawn_blocking(crate::zellij::live_sessions)
        .await
        .unwrap_or_default();
    let Ok(run_dir) = crate::agents::run_dir() else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let state = tauri::Manager::state::<crate::state::AppState>(app);
    for name in live {
        let capture = run_dir.join(format!("{name}.jsonl"));
        let wrote_at = crate::status::capture_mtime_ms(&capture.to_string_lossy());
        let busy = any_live_row(&state.agent_sessions, &name).await;
        let probe = name.clone();
        let clients = tokio::task::spawn_blocking(move || crate::zellij::connected_clients(&probe))
            .await
            .unwrap_or(None);
        let own = hosting_ptys(&state.pty_sessions, &name).await;
        if !finished_and_idle(&name, wrote_at, now, busy, clients, own) {
            continue;
        }
        let agent = handle_in(&name);
        let detail = format!(
            "ended {name}: it finished its task and sat idle at its prompt for over \
             {} hours",
            IDLE_REAP_AFTER_MS / 3_600_000
        );
        reap_session(app, name, &agent, "idle_reaped", &detail).await;
    }
}

/// Live agent sessions as they will be AT LAUNCH: the run this fire is about to
/// reap is already discounted, because the reap frees its slot.
///
/// Counting it is the whole bug. On the rig the two sessions "already live" were
/// the owner's own and the one the fire had just killed; taken this way the
/// count reads 1 and the replacement fits.
async fn live_at_launch(sessions: &crate::status::AgentSessions, reaping: Option<&str>) -> usize {
    sessions
        .lock()
        .await
        .values()
        .filter(|meta| crate::status::counts_as_live(meta.status))
        .filter(|meta| match reaping {
            Some(zellij) => !hosted_in(meta, zellij),
            None => true,
        })
        .count()
}

/// The three effects of a fire, in the only order that cannot destroy a working
/// agent: admit, then reap, then launch.
///
/// The rig caught the destructive order on 2026-09-01, three ledger rows 0.4ms
/// apart: automation_fired at 09:07:22.809722, automation_reaped at .859567,
/// automation_failed ("2 agent sessions are already live and the concurrent cap
/// is 2") at .859956. Reaping first and asking second means a tight cap turns
/// the automation into a killer: it destroys its own mid-task agent every cycle
/// and starts nothing. Doing nothing is strictly better than that.
///
/// Asking first and reaping second is the safe order, and it does not refuse a
/// launch that would have fit, because `admit` is decided against the state the
/// launch will actually see (`live_at_launch` discounts the run being reaped).
/// A refusal leaves the previous run untouched, working.
async fn admit_reap_launch<R, RFut, L, LFut>(
    admit: Result<(), String>,
    reap: Option<String>,
    kill: R,
    launch: L,
) -> FireReport
where
    R: FnOnce(String) -> RFut,
    RFut: std::future::Future<Output = bool>,
    L: FnOnce() -> LFut,
    LFut: std::future::Future<Output = Result<FireOutcome, String>>,
{
    if let Err(refused) = admit {
        return FireReport::refused(refused);
    }
    let reaped = match reap {
        Some(session) => kill(session).await,
        None => false,
    };
    FireReport {
        reaped,
        outcome: launch().await,
    }
}

/// Runs one automation: resolve the agent, check the ceiling against the state
/// the launch will see, reap the last run, launch durable through the profile
/// launcher (which is where the composer runs).
async fn fire(app: &AppHandle, auto: &Automation) -> FireReport {
    let plan = match plan_fire(auto, |spoken| {
        crate::agent_profiles::resolve_spoken_handle(spoken)
    }) {
        Ok(plan) => plan,
        Err(e) => return FireReport::refused(e),
    };
    let state = tauri::Manager::state::<crate::state::AppState>(app);
    let sessions = state.agent_sessions.clone();
    // ponytail: this asks the spend ceiling, it does not reserve against it.
    // Another launch could take the slot between here and the ceiling's own
    // gate inside agent_profile_launch, and by then the reap has run. Closing
    // that window means holding a ceiling lease across the launch; the
    // scheduler ticks once a minute against a cap of 2, so it is not worth the
    // machinery.
    let admit = crate::spend::would_admit(live_at_launch(&sessions, plan.reap.as_deref()).await);
    let agent = plan.request.handle.clone();
    let reap_app = app.clone();
    admit_reap_launch(
        admit,
        plan.reap.clone(),
        |session| async move {
            let detail =
                format!("ended the previous run in {session} before starting the next");
            reap_session(&reap_app, session, &agent, "automation_reaped", &detail).await
        },
        || async {
            crate::agent_profiles::agent_profile_launch(app.clone(), state, plan.request)
                .await
                .map(|response| FireOutcome {
                    session_id: response.session_id,
                    zellij_session: response.zellij_session,
                })
        },
    )
    .await
}

/// The automation://fire payload. It carries the OUTCOME, not just the intent:
/// a fire the UI cannot tell succeeded from failed is how "unknown agent id:
/// rigtwo" reached one console.error and nothing else.
fn fire_payload(auto: &Automation, outcome: &Result<FireOutcome, String>) -> serde_json::Value {
    match outcome {
        Ok(run) => serde_json::json!({
            "automation": auto,
            "session_id": run.session_id,
            "error": serde_json::Value::Null,
        }),
        Err(error) => serde_json::json!({
            "automation": auto,
            "session_id": serde_json::Value::Null,
            "error": error,
        }),
    }
}

// ─── Background task ─────────────────────────────────────────────────────────

/// Spawns the scheduler loop: every 60s, run every automation that is due and
/// announce the result on "automation://fire" so an open panel can show it.
pub fn spawn_scheduler_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            tick(&app).await;
        }
    });
}

async fn tick(app: &AppHandle) {
    // The termination contract for cold runs (XNAUT-262). On the scheduler's own
    // clock rather than a task of its own: a 60s cadence against a four-hour
    // grace is nowhere near the limiting factor, and this is where reaping
    // already lives.
    reap_idle_runs(app).await;
    // The disk analogue of the reaper above (XNAUT-264): agent worktrees and
    // their six-gigabyte build caches filled André's disk to 98% twice in four
    // days and he noticed before the app did, both times. This costs one
    // `statvfs` and removes nothing; it only speaks up.
    crate::housekeeper::watch_disk(app);
    let fired = tick_with(Local::now(), |auto| {
        let app = app.clone();
        async move { fire(&app, &auto).await }
    })
    .await;
    for (auto, outcome) in fired {
        if let Err(e) = app.emit("automation://fire", fire_payload(&auto, &outcome)) {
            eprintln!(
                "[scheduler] failed to emit automation://fire for {:?}: {e}",
                auto.name
            );
        }
    }
}

/// Evaluates every automation and runs the due ones. `launch` performs the
/// actual run; it is a parameter so the ordering this function exists to
/// enforce is testable without a PTY or a profile store.
///
/// Returns each fired automation as persisted, paired with what happened, for
/// the caller to announce.
async fn tick_with<F, Fut>(
    now: DateTime<Local>,
    launch: F,
) -> Vec<(Automation, Result<FireOutcome, String>)>
where
    F: Fn(Automation) -> Fut,
    Fut: std::future::Future<Output = FireReport>,
{
    let mut autos = load_automations();
    let mut fired = Vec::new();
    for i in 0..autos.len() {
        let auto = autos[i].clone();
        if !auto.enabled {
            continue;
        }
        let last = parse_last_fired(auto.last_fired.as_deref());
        if !is_due(&auto.schedule, now, last) {
            continue;
        }
        if blocked_by_grace(&auto.schedule, auto.grace_hours, now, last) {
            continue;
        }
        if let Some(cmd) = auto.precheck.as_deref().filter(|c| !c.trim().is_empty()) {
            match run_precheck(cmd, &auto.project_path, auto.precheck_timeout_secs).await {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!(
                        "[scheduler] precheck for {:?} returned no-go — skipping run",
                        auto.name
                    );
                    continue;
                }
                Err(e) => {
                    eprintln!(
                        "[scheduler] precheck for {:?} failed: {e} — skipping run",
                        auto.name
                    );
                    continue;
                }
            }
        }
        let report = fire_and_record(&auto, &launch).await;
        // A reaped run is gone whatever the launch then did, so its name must
        // not survive the fire. Keeping it is what made every later tick claim
        // it had ended a session that died a minute ago.
        let mut dirty = report.reaped && autos[i].last_session.is_some();
        if report.reaped {
            autos[i].last_session = None;
        }
        // last_fired advances only for a run that actually started. It used to
        // be persisted BEFORE the fire, so a fire nobody handled still consumed
        // the slot; on `hourly` that is an hour of silence per miss, and on the
        // rig it is why a no-op fire looked identical to a successful one.
        if let Ok(run) = &report.outcome {
            autos[i].last_fired = Some(now.to_rfc3339());
            autos[i].last_session = run.zellij_session.clone();
            dirty = true;
        }
        if dirty {
            if let Err(e) = save_automations(&autos) {
                eprintln!(
                    "[scheduler] failed to persist last_fired for {:?}: {e}",
                    auto.name
                );
            }
        }
        fired.push((autos[i].clone(), report.outcome));
    }
    fired
}

/// Fires one automation and puts both halves in the ledger.
///
/// A fire AND its outcome, because `last_fired` says "fired", not "ran". All
/// four rig runs succeeded and left zero ledger entries, so "did last night's
/// automations run?" had no answer anywhere in the app.
async fn fire_and_record<F, Fut>(auto: &Automation, launch: &F) -> FireReport
where
    F: Fn(Automation) -> Fut,
    Fut: std::future::Future<Output = FireReport>,
{
    crate::ledger::record(
        "automation_fired",
        &auto.agent_id,
        "",
        &format!("{} ({})", auto.name, auto.schedule),
    );
    let report = launch(auto.clone()).await;
    match &report.outcome {
        Ok(run) => crate::ledger::record(
            "automation_ran",
            &auto.agent_id,
            "",
            &format!("{} started in {}", auto.name, run.session_id),
        ),
        Err(error) => crate::ledger::record(
            "automation_failed",
            &auto.agent_id,
            "",
            &format!("{} did not start: {error}", auto.name),
        ),
    }
    report
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn automation_list() -> Result<Vec<Automation>, String> {
    Ok(load_automations())
}

#[tauri::command]
pub fn automation_save(mut automation: Automation) -> Result<Automation, String> {
    if automation.id.trim().is_empty() {
        automation.id = uuid::Uuid::new_v4().to_string();
    }
    let mut autos = load_automations();
    match autos.iter_mut().find(|a| a.id == automation.id) {
        Some(slot) => {
            // last_fired and last_session are the scheduler's, not the
            // editor's. The edit modal rebuilds the record field by field, and
            // a dropped last_session is a live agent nothing will ever reap.
            automation.last_fired = slot.last_fired.clone();
            automation.last_session = slot.last_session.clone();
            *slot = automation.clone();
        }
        None => autos.push(automation.clone()),
    }
    save_automations(&autos)?;
    Ok(automation)
}

#[tauri::command]
pub fn automation_delete(id: String) -> Result<(), String> {
    let mut autos = load_automations();
    let before = autos.len();
    autos.retain(|a| a.id != id);
    if autos.len() == before {
        return Err(format!("unknown automation id: {id}"));
    }
    save_automations(&autos)
}

#[tauri::command]
pub async fn automation_fire_now(app: AppHandle, id: String) -> Result<(), String> {
    let mut autos = load_automations();
    let idx = autos
        .iter()
        .position(|a| a.id == id)
        .ok_or_else(|| format!("unknown automation id: {id}"))?;
    let auto = autos[idx].clone();
    if let Some(cmd) = auto.precheck.as_deref().filter(|c| !c.trim().is_empty()) {
        let pass = run_precheck(cmd, &auto.project_path, auto.precheck_timeout_secs).await?;
        if !pass {
            return Err("precheck did not pass — run skipped".into());
        }
    }
    // The same path the schedule takes, so "Run now" and a scheduled fire
    // cannot drift apart in what the agent receives or what gets recorded.
    let FireReport { reaped, outcome } = fire_and_record(&auto, &|auto: Automation| {
        let app = app.clone();
        async move { fire(&app, &auto).await }
    })
    .await;
    let mut dirty = reaped && autos[idx].last_session.is_some();
    if reaped {
        autos[idx].last_session = None;
    }
    if let Ok(run) = &outcome {
        autos[idx].last_fired = Some(Local::now().to_rfc3339());
        autos[idx].last_session = run.zellij_session.clone();
        dirty = true;
    }
    if dirty {
        save_automations(&autos)?;
    }
    let _ = app.emit("automation://fire", fire_payload(&autos[idx], &outcome));
    outcome.map(|_| ())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    // 2026-06-10 is a Wednesday, 2026-06-13 is a Saturday.

    #[test]
    fn hourly_never_fired_is_due() {
        assert!(is_due("hourly", at(2026, 6, 10, 10, 0), None));
    }

    #[test]
    fn hourly_fired_30m_ago_is_not_due() {
        assert!(!is_due(
            "hourly",
            at(2026, 6, 10, 10, 0),
            Some(at(2026, 6, 10, 9, 30)),
        ));
    }

    #[test]
    fn daily_at_nine_is_due_at_ten_when_not_fired_today() {
        assert!(is_due("daily@09:00", at(2026, 6, 10, 10, 0), None));
        assert!(is_due(
            "daily@09:00",
            at(2026, 6, 10, 10, 0),
            Some(at(2026, 6, 9, 9, 1)),
        ));
    }

    #[test]
    fn daily_at_nine_is_not_due_at_eight() {
        assert!(!is_due("daily@09:00", at(2026, 6, 10, 8, 0), None));
    }

    #[test]
    fn daily_at_nine_is_not_due_again_after_firing_today() {
        assert!(!is_due(
            "daily@09:00",
            at(2026, 6, 10, 10, 0),
            Some(at(2026, 6, 10, 9, 1)),
        ));
    }

    #[test]
    fn weekdays_at_nine_is_not_due_on_saturday() {
        assert!(!is_due("weekdays@09:00", at(2026, 6, 13, 10, 0), None));
    }

    #[test]
    fn every_30m_fired_31m_ago_is_due() {
        assert!(is_due(
            "every:30m",
            at(2026, 6, 10, 10, 1),
            Some(at(2026, 6, 10, 9, 30)),
        ));
    }

    #[test]
    fn every_30m_fired_29m_ago_is_not_due() {
        assert!(!is_due(
            "every:30m",
            at(2026, 6, 10, 9, 59),
            Some(at(2026, 6, 10, 9, 30)),
        ));
    }

    #[test]
    fn unknown_schedule_is_never_due() {
        assert!(!is_due("fortnightly", at(2026, 6, 10, 10, 0), None));
        assert!(!is_due("every:30x", at(2026, 6, 10, 10, 0), None));
        assert!(!is_due("daily@9am", at(2026, 6, 10, 10, 0), None));
    }

    #[test]
    fn grace_blocks_daily_within_window_only() {
        let now = at(2026, 6, 10, 10, 0);
        let two_h_ago = Some(at(2026, 6, 10, 8, 0));
        let thirteen_h_ago = Some(at(2026, 6, 9, 21, 0));
        assert!(blocked_by_grace("daily@09:00", 12, now, two_h_ago));
        assert!(!blocked_by_grace("daily@09:00", 12, now, thirteen_h_ago));
        assert!(!blocked_by_grace("daily@09:00", 0, now, two_h_ago));
        assert!(!blocked_by_grace("daily@09:00", 12, now, None));
        // hourly/every encode their own spacing — grace never applies.
        assert!(!blocked_by_grace("hourly", 12, now, two_h_ago));
        assert!(!blocked_by_grace("every:30m", 12, now, two_h_ago));
    }

    // ── The six defects the tron rig found on 2026-09-01 ─────────────────────

    fn automation(name: &str) -> Automation {
        Automation {
            id: format!("id-{name}"),
            name: name.to_string(),
            prompt: "Audit the repo.".into(),
            precheck: None,
            precheck_timeout_secs: 60,
            project_path: "/tmp/project".into(),
            workspace: "worktree".into(),
            branch: "development".into(),
            agent_id: "Rigtwo".into(),
            session_mode: "fresh".into(),
            schedule: "hourly".into(),
            grace_hours: 0,
            enabled: true,
            last_fired: None,
            last_session: None,
        }
    }

    /// Scratch automations store + scratch ledger, under the ledger's lock so
    /// two modules cannot fight over XNAUT_LEDGER_PATH.
    fn scratch(name: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        let guard = crate::ledger::scratch(name);
        let file = std::env::temp_dir()
            .join(format!("xnaut-automations-{}-{name}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        std::env::set_var("XNAUT_AUTOMATIONS_PATH", &file);
        (guard, file)
    }

    fn ledger_kinds() -> Vec<String> {
        crate::ledger::ledger_recent(Some(50))
            .into_iter()
            .map(|entry| entry.kind)
            .collect()
    }

    fn started(session: &str) -> FireOutcome {
        FireOutcome {
            session_id: session.into(),
            zellij_session: Some(format!("xnaut-rigtwo-{session}")),
        }
    }

    /// A fire that reaped nothing and ended `outcome`.
    fn report(outcome: Result<FireOutcome, String>) -> FireReport {
        FireReport {
            reaped: false,
            outcome,
        }
    }

    #[tokio::test]
    // The scratch guard is a std Mutex held across the await on purpose: it is
    // what keeps two tests off one process-wide XNAUT_LEDGER_PATH, and dropping
    // it before the tick is exactly the race it exists to prevent.
    #[allow(clippy::await_holding_lock)]
    async fn a_fire_that_never_ran_does_not_consume_the_schedule() {
        // The rig's failure mode: last_fired was persisted BEFORE the event, so
        // a fire nobody handled still burned the slot. On `hourly` that is an
        // hour of silence per miss.
        let (_guard, _path) = scratch("not-consumed");
        save_automations(&[automation("audit")]).unwrap();

        let fired = tick_with(at(2026, 6, 10, 10, 0), |_| async {
            report(Err("no agent called \"rigtwo\"".to_string()))
        })
        .await;

        assert_eq!(fired.len(), 1, "the automation was due and was tried");
        assert!(fired[0].1.is_err());
        assert_eq!(
            load_automations()[0].last_fired,
            None,
            "a run that never started must be retried on the next tick"
        );

        // And the mirror image: a run that DID start advances it.
        let fired = tick_with(at(2026, 6, 10, 10, 0), |_| async {
            report(Ok(started("abc123")))
        })
        .await;
        assert_eq!(fired.len(), 1);
        let stored = load_automations().remove(0);
        assert!(stored.last_fired.is_some(), "a started run consumes the slot");
        assert_eq!(stored.last_session.as_deref(), Some("xnaut-rigtwo-abc123"));
    }

    #[tokio::test]
    // The scratch guard is a std Mutex held across the await on purpose: it is
    // what keeps two tests off one process-wide XNAUT_LEDGER_PATH, and dropping
    // it before the tick is exactly the race it exists to prevent.
    #[allow(clippy::await_holding_lock)]
    async fn a_successful_run_reaches_the_ledger() {
        // Four successful runs on the rig left ZERO ledger entries. The only
        // record was last_fired, which says "fired", not "ran", so "did last
        // night's automations run?" had no answer anywhere in the app.
        let (_guard, _path) = scratch("ledger-ran");
        save_automations(&[automation("audit")]).unwrap();

        tick_with(at(2026, 6, 10, 10, 0), |_| async {
            report(Ok(started("abc123")))
        })
        .await;
        let kinds = ledger_kinds();
        assert!(kinds.contains(&"automation_fired".to_string()), "{kinds:?}");
        assert!(kinds.contains(&"automation_ran".to_string()), "{kinds:?}");
    }

    #[tokio::test]
    // The scratch guard is a std Mutex held across the await on purpose: it is
    // what keeps two tests off one process-wide XNAUT_LEDGER_PATH, and dropping
    // it before the tick is exactly the race it exists to prevent.
    #[allow(clippy::await_holding_lock)]
    async fn a_failed_run_reaches_the_ledger_too() {
        // The outcome that was reaching a single console.error and nothing
        // else: no toast, no ledger row, no badge, no way to notice.
        let (_guard, _path) = scratch("ledger-failed");
        save_automations(&[automation("audit")]).unwrap();

        tick_with(at(2026, 6, 10, 10, 0), |_| async {
            report(Err("unknown agent id: rigtwo".to_string()))
        })
        .await;
        let entries = crate::ledger::ledger_recent(Some(50));
        let kinds: Vec<&str> = entries.iter().map(|e| e.kind.as_str()).collect();
        assert!(kinds.contains(&"automation_fired"), "{kinds:?}");
        assert!(kinds.contains(&"automation_failed"), "{kinds:?}");
        assert!(
            entries
                .iter()
                .any(|e| e.kind == "automation_failed" && e.detail.contains("rigtwo")),
            "the reason has to be in the row, not just the fact: {entries:?}"
        );
    }

    #[test]
    fn the_agent_is_resolved_as_a_handle_not_a_runtime_id() {
        // `rigtwo` is a real profile handle and the fire died on it with
        // "unknown agent id: rigtwo": agent_launch only knows the six
        // agents.toml runtime ids. A display name must land on its handle.
        let auto = automation("audit");
        let plan = plan_fire(&auto, |spoken| {
            assert_eq!(spoken, "Rigtwo", "the resolver sees what the owner typed");
            Ok("rigtwo".to_string())
        })
        .expect("a known agent plans a fire");
        assert_eq!(plan.request.handle, "rigtwo");

        // And an unknown one is an error the caller can show, not a log line.
        let refused = plan_fire(&auto, |_| Err("no agent called \"rigtwo\"".to_string()));
        assert_eq!(refused.err().as_deref(), Some("no agent called \"rigtwo\""));
    }

    #[test]
    fn a_failed_fire_is_announced_with_its_reason() {
        // The panel has to be able to say WHY nothing ran. The old payload
        // carried only the automation, so a fire that failed and a fire that
        // worked looked identical to every listener.
        let auto = automation("audit");
        let payload = fire_payload(&auto, &Err("no agent called \"rigtwo\"".to_string()));
        assert_eq!(payload["error"], "no agent called \"rigtwo\"");
        assert!(payload["session_id"].is_null());

        let payload = fire_payload(&auto, &Ok(started("abc123")));
        assert_eq!(payload["session_id"], "abc123");
        assert!(payload["error"].is_null());
    }

    #[test]
    fn a_fire_is_durable_and_reaps_the_run_before_it() {
        // Four fires left four sessions and three idle claude processes,
        // because every launch was durable:false and nothing collected the
        // previous one. At every:1m that is ~60 orphaned agents an hour.
        let mut auto = automation("audit");
        auto.last_session = Some("xnaut-rigtwo-old".into());
        let plan = plan_fire(&auto, |_| Ok("rigtwo".to_string())).unwrap();
        assert_eq!(plan.reap.as_deref(), Some("xnaut-rigtwo-old"));
        assert_eq!(
            plan.request.durable,
            Some(true),
            "a scheduled run must outlive the app, like a cold wake"
        );

        // Nothing to reap on the first run, and a blank name is not a session.
        let mut first = automation("audit");
        assert_eq!(plan_fire(&first, |_| Ok("rigtwo".into())).unwrap().reap, None);
        first.last_session = Some("  ".into());
        assert_eq!(plan_fire(&first, |_| Ok("rigtwo".into())).unwrap().reap, None);
    }

    #[test]
    fn an_automation_run_gets_the_foundation() {
        // Process argv on the rig: a wake-launched agent carried
        // "# xNAUT Foundation (v2)…", an automation-launched one carried only
        // the bare task. The composer runs inside agent_profile_launch, and it
        // is skipped for a resume. So an automation is never a resume, and it
        // always carries its prompt as a task to compose.
        let auto = automation("audit");
        let plan = plan_fire(&auto, |_| Ok("rigtwo".to_string())).unwrap();
        assert!(!plan.request.resume, "a resume gets the bare task");
        assert_eq!(plan.request.prompt.as_deref(), Some("Audit the repo."));
        assert!(!plan.request.conversation_mode);
    }

    // ── The destructive fire the rig caught on 2026-09-01 ───────────────────

    fn live_row(session_id: &str, zellij: Option<&str>) -> crate::status::AgentSessionMeta {
        crate::status::AgentSessionMeta {
            session_id: session_id.to_string(),
            agent_id: "rigtwo".into(),
            label: "rigtwo".into(),
            pane_key: format!("{session_id}:{session_id}"),
            status: crate::status::AgentStatus::Working,
            started_at_ms: 0,
            last_output_at_ms: 0,
            status_changed_at_ms: 0,
            output_path: None,
            zellij_session: zellij.map(str::to_string),
        }
    }

    async fn sessions_with(
        rows: Vec<crate::status::AgentSessionMeta>,
    ) -> crate::status::AgentSessions {
        let map: std::collections::HashMap<String, crate::status::AgentSessionMeta> = rows
            .into_iter()
            .map(|meta| (meta.session_id.clone(), meta))
            .collect();
        std::sync::Arc::new(tokio::sync::Mutex::new(map))
    }

    #[tokio::test]
    async fn a_capped_fire_leaves_the_working_agent_alive() {
        // The rig, three ledger rows 0.4ms apart: automation_fired at
        // 09:07:22.809722, automation_reaped at .859567, automation_failed at
        // .859956 with "2 agent sessions are already live and the concurrent
        // cap is 2". Reaping first and asking second means the automation
        // destroys its own mid-task agent and starts nothing, every cycle.
        let killed = std::cell::Cell::new(false);
        let launched = std::cell::Cell::new(false);
        let ceiling = "spend ceiling: 2 agent sessions are already live and the \
                       concurrent cap is 2.";
        let report = admit_reap_launch(
            Err(ceiling.to_string()),
            Some("xnaut-rigtwo-e99ea4dd".to_string()),
            |_| async {
                killed.set(true);
                true
            },
            || async {
                launched.set(true);
                Ok(started("new"))
            },
        )
        .await;

        assert!(
            !killed.get(),
            "a fire that cannot launch must not kill the run it cannot replace"
        );
        assert!(!launched.get(), "nothing was launched");
        assert!(!report.reaped, "so last_session still names a live run");
        let refused = report.outcome.unwrap_err();
        assert!(refused.contains("concurrent cap is 2"), "{refused}");
    }

    #[tokio::test]
    async fn an_admitted_fire_reaps_before_it_launches() {
        // The other half: the reap still has to free the slot BEFORE the launch
        // asks for one, or the automation accumulates agents instead.
        let order = std::cell::RefCell::new(Vec::new());
        let log = &order;
        let report = admit_reap_launch(
            Ok(()),
            Some("xnaut-rigtwo-old".to_string()),
            |session| async move {
                log.borrow_mut().push(format!("kill {session}"));
                true
            },
            || async {
                log.borrow_mut().push("launch".to_string());
                Ok(started("new"))
            },
        )
        .await;

        assert_eq!(order.into_inner(), vec!["kill xnaut-rigtwo-old", "launch"]);
        assert!(report.reaped);
        assert!(report.outcome.is_ok());
    }

    #[tokio::test]
    async fn the_ceiling_counts_the_state_the_launch_will_see() {
        // Asking before reaping only works if the question discounts the slot
        // the reap frees. The two sessions "already live" on the rig were the
        // owner's own and the one the fire was about to end; counted this way
        // the answer is 1 and the replacement fits.
        let sessions = sessions_with(vec![
            live_row("owner-pty", Some("xnaut-claude-owner")),
            live_row("run-pty", Some("xnaut-rigtwo-e99ea4dd")),
        ])
        .await;
        assert_eq!(live_at_launch(&sessions, None).await, 2);
        assert_eq!(
            live_at_launch(&sessions, Some("xnaut-rigtwo-e99ea4dd")).await,
            1,
            "the run being reaped is not competing with its own replacement"
        );

        // An adopted row is keyed by the zellij name itself, and must match too.
        let adopted = sessions_with(vec![live_row("xnaut-rigtwo-e99ea4dd", None)]).await;
        assert_eq!(
            live_at_launch(&adopted, Some("xnaut-rigtwo-e99ea4dd")).await,
            0
        );
    }

    #[test]
    fn a_reap_that_found_nothing_claims_nothing() {
        // last_session was never cleared, so every tick after the first logged
        // "ended the previous run in xnaut-rigtwo-e99ea4dd" for a session that
        // had been dead over a minute. The ledger asserted work nobody did.
        let (_guard, _path) = scratch("reap-ledger");
        let dead = "xnaut-rigtwo-e99ea4dd";
        let detail = "ended the previous run";
        assert!(
            record_reap(&Reaped::NothingThere, "automation_reaped", detail, dead, "rigtwo"),
            "a name that names nothing is still cleared, so it is not retried"
        );
        assert!(
            ledger_kinds().is_empty(),
            "nothing was ended, so nothing is recorded: {:?}",
            ledger_kinds()
        );

        assert!(record_reap(
            &Reaped::Ended,
            "automation_reaped",
            detail,
            dead,
            "rigtwo"
        ));
        assert_eq!(ledger_kinds(), vec!["automation_reaped".to_string()]);

        assert!(
            !record_reap(
                &Reaped::Failed("zellij died".into()),
                "automation_reaped",
                detail,
                dead,
                "rigtwo"
            ),
            "a session still standing keeps its name, so the next tick retries"
        );
        assert_eq!(ledger_kinds().len(), 1, "and records no second reap");
    }

    #[tokio::test]
    // The scratch guard is a std Mutex held across the await on purpose: it is
    // what keeps two tests off one process-wide XNAUT_LEDGER_PATH, and dropping
    // it before the tick is exactly the race it exists to prevent.
    #[allow(clippy::await_holding_lock)]
    async fn a_reaped_run_stops_being_the_next_fire_s_victim() {
        // Even when the launch then fails, the reaped session is gone. Keeping
        // its name is what made the ledger repeat the same phantom reap every
        // minute for over an hour on the rig.
        let (_guard, _path) = scratch("clears-last-session");
        let mut stored = automation("audit");
        stored.last_session = Some("xnaut-rigtwo-e99ea4dd".into());
        save_automations(&[stored]).unwrap();

        tick_with(at(2026, 6, 10, 10, 0), |_| async {
            FireReport {
                reaped: true,
                outcome: Err("zellij: could not start the session".to_string()),
            }
        })
        .await;

        let after = load_automations().remove(0);
        assert_eq!(
            after.last_session, None,
            "the reaped session must not be reaped again next minute"
        );
        assert_eq!(after.last_fired, None, "and nothing ran, so nothing fired");
    }

    #[test]
    fn editing_an_automation_cannot_orphan_its_live_run() {
        // The edit modal rebuilds the record field by field. A dropped
        // last_session is a durable agent nothing will ever reap.
        let (_guard, _path) = scratch("save-preserves");
        let mut stored = automation("audit");
        stored.last_fired = Some("2026-06-10T10:00:00+00:00".into());
        stored.last_session = Some("xnaut-rigtwo-old".into());
        save_automations(&[stored.clone()]).unwrap();

        let mut edited = automation("audit");
        edited.name = "Audit, renamed".into();
        automation_save(edited).unwrap();

        let after = load_automations().remove(0);
        assert_eq!(after.name, "Audit, renamed", "the edit landed");
        assert_eq!(after.last_session.as_deref(), Some("xnaut-rigtwo-old"));
        assert_eq!(after.last_fired.as_deref(), Some("2026-06-10T10:00:00+00:00"));
    }

    // ── The termination contract for cold runs (XNAUT-262) ──────────────────
    //
    // An interactive `claude` never exits after finishing its task, so without
    // a reaper every durable run is immortal. The rig measured the result on
    // 2026-09-02: seven zellij sessions, the oldest 1 day 6 hours old, all
    // idle agents that had finished long ago.
    //
    // Time is injected rather than slept, exactly as status.rs does it: the
    // capture file is written once and `now` is moved past it, so every case is
    // hermetic and instant.

    /// A run's capture file, as script(1) leaves one.
    struct Capture(std::path::PathBuf);
    impl Capture {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "xnaut-reap-{tag}-{}-{:?}.jsonl",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::write(&path, b"the agent's own tty").expect("capture file");
            Self(path)
        }
        /// The moment the agent last wrote, read back the way production does.
        fn wrote_at(&self) -> i64 {
            crate::status::capture_mtime_ms(&self.0.to_string_lossy())
                .expect("a written capture file has an mtime")
        }
    }
    impl Drop for Capture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn a_run_still_writing_to_its_capture_file_is_never_reaped() {
        // THE LOAD-BEARING CASE. A working agent must survive the reaper, and
        // the capture file is the only honest per-session evidence there is:
        // script(1) holds it for the life of the agent's command, so its mtime
        // moves while the agent works and a zellij status-bar repaint never
        // touches it.
        let capture = Capture::new("working");
        let wrote_at = capture.wrote_at();

        for elapsed in [0, 1_000, IDLE_REAP_AFTER_MS - 1] {
            // Both readings of the attach gate, so the capture file is what
            // holds the line and not an incidental second no. `own = 1` is the
            // production shape: the app's own tab hosting the run.
            for (clients, own) in [(Some(0), 0), (Some(1), 1)] {
                assert!(
                    !finished_and_idle(
                        "xnaut-rigtwo-deadbeef",
                        Some(wrote_at),
                        wrote_at + elapsed,
                        false,
                        clients,
                        own,
                    ),
                    "an agent that wrote {elapsed}ms ago is working; ending it destroys the work"
                );
            }
        }

        // And the reaper still has to be able to say yes, or the assertions
        // above are green for the wrong reason.
        assert!(
            finished_and_idle(
                "xnaut-rigtwo-deadbeef",
                Some(wrote_at),
                wrote_at + IDLE_REAP_AFTER_MS,
                false,
                Some(0),
                0,
            ),
            "a run silent past the grace period is what this exists to collect"
        );
    }

    #[test]
    fn a_session_somebody_else_is_attached_to_is_left_alone() {
        // A client xNAUT cannot account for as one of its own hosting panes is
        // a real external viewer: somebody who ran `zellij attach` from their
        // own terminal or over SSH to read a finished transcript. Measured
        // against the installed zellij 0.44 on 2026-09-03, a client that goes
        // away is off the metadata within five seconds, so a surplus client is
        // live rather than a leftover.
        let long_ago = Some(0);
        let now = IDLE_REAP_AFTER_MS * 10;
        let quiet = |clients, own| {
            finished_and_idle("xnaut-rigtwo-deadbeef", long_ago, now, false, clients, own)
        };

        assert!(
            !quiet(Some(1), 0),
            "somebody outside the app is looking at this pane, however long it has been quiet"
        );
        assert!(
            !quiet(Some(2), 1),
            "one client is the app's own tab; the other is a person, and the person wins"
        );
        assert!(
            !quiet(None, 1),
            "unreadable metadata is an unanswered question, not an empty room"
        );
        assert!(quiet(Some(0), 0));
    }

    #[test]
    fn the_pile_the_reaper_was_written_for_is_actually_collected() {
        // THE PRODUCTION CONDITION, reproduced. The rig on 2026-09-02: seven
        // zellij sessions, agents all finished, capture files last written nine
        // hours earlier so the four hour grace was exceeded twice over, and
        // `idle_reaped` rows in the ledger ZERO. Every one of those sessions
        // reported `connected_clients 1` with nobody sitting in it.
        //
        // The client was the app's own. A tab hosting a run executes `zellij
        // attach` in a PTY pane (zellij.rs, `launch_command`), and that client
        // is tied to the life of the tab, not to the agent's command, so it is
        // still there hours after the agent stops. Compared against zero the
        // gate refused every time, which made the reaper unfireable in normal
        // use rather than merely conservative.
        let nine_hours_idle = IDLE_REAP_AFTER_MS * 9 / 4;
        let wrote_at = 0;

        assert!(
            finished_and_idle(
                "xnaut-rigtwo-deadbeef",
                Some(wrote_at),
                wrote_at + nine_hours_idle,
                false,
                Some(1), // what the rig's session-metadata.kdl actually said
                1,       // and the one app tab that explains it
            ),
            "a finished run whose only viewer is the app's own tab is exactly the pile \
             this reaper exists to collect; refusing it collects nothing, ever"
        );
    }

    /// A real `PtySession` record, because that is the type the reaper counts
    /// and a hand-rolled stand-in cannot catch the field being read wrong.
    fn pty_hosting(zellij: Option<&str>) -> std::sync::Arc<crate::state::PtySession> {
        use portable_pty::{CommandBuilder, PtySize};
        let pty = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
        let child = pty
            .slave
            .spawn_command(CommandBuilder::new("true"))
            .expect("spawn");
        let reader = pty.master.try_clone_reader().expect("reader");
        let writer = pty.master.take_writer().expect("writer");
        std::sync::Arc::new(crate::state::PtySession {
            _id: "pty".into(),
            pty_pair: std::sync::Arc::new(tokio::sync::Mutex::new(pty)),
            child: std::sync::Arc::new(tokio::sync::Mutex::new(child)),
            reader: std::sync::Arc::new(std::sync::Mutex::new(reader)),
            writer: std::sync::Arc::new(std::sync::Mutex::new(writer)),
            created_at: std::time::SystemTime::now(),
            session_name: zellij.map(str::to_string),
        })
    }

    #[tokio::test]
    async fn the_app_can_tell_which_clients_are_its_own_hosting_panes() {
        // The subtraction the whole fix rests on. Counting nothing leaves the
        // reaper as unfireable as it was; counting too much lets it end a
        // session somebody outside the app is attached to.
        let mut map = std::collections::HashMap::new();
        map.insert("a".to_string(), pty_hosting(Some("xnaut-rigtwo-deadbeef")));
        map.insert("b".to_string(), pty_hosting(Some("xnaut-rigtwo-deadbeef")));
        map.insert("c".to_string(), pty_hosting(Some("xnaut-other-cafe0000")));
        // A plain shell tab hosts no zellij session and holds no client.
        map.insert("d".to_string(), pty_hosting(None));
        let sessions: PtySessions = std::sync::Arc::new(tokio::sync::Mutex::new(map));

        assert_eq!(hosting_ptys(&sessions, "xnaut-rigtwo-deadbeef").await, 2);
        assert_eq!(hosting_ptys(&sessions, "xnaut-other-cafe0000").await, 1);
        assert_eq!(
            hosting_ptys(&sessions, "xnaut-nobody-00000000").await,
            0,
            "a session the app is not hosting explains none of its clients"
        );

        for pty in sessions.lock().await.values() {
            let _ = pty.child.lock().await.kill();
        }
    }

    #[test]
    fn a_run_the_tracker_still_calls_busy_is_left_alone() {
        // Blocked, Permission and Waiting come from the agent's own hooks and
        // mean a human owes it an answer. Those are evidence something is
        // there, and ending one throws away work a keystroke from continuing.
        assert!(
            !finished_and_idle("xnaut-rigtwo-deadbeef", Some(0), IDLE_REAP_AFTER_MS * 10, true, Some(0), 0),
            "a row the app still counts as live outranks a quiet capture file"
        );
    }

    #[tokio::test]
    async fn the_tracker_s_busy_answer_uses_the_one_live_definition() {
        // `counts_as_live` and nothing else: status.rs keeps ONE list of the
        // statuses that occupy a slot, and a second copy drifting apart is how
        // a reap frees something the launch gate still counts.
        let working = sessions_with(vec![live_row("pty", Some("xnaut-rigtwo-deadbeef"))]).await;
        assert!(any_live_row(&working, "xnaut-rigtwo-deadbeef").await);

        let mut resting = live_row("pty", Some("xnaut-rigtwo-deadbeef"));
        resting.status = crate::status::AgentStatus::Idle;
        let resting = sessions_with(vec![resting]).await;
        assert!(!any_live_row(&resting, "xnaut-rigtwo-deadbeef").await);

        // An adopted row is keyed by the zellij name itself, so it has to match
        // through `hosted_in` too or the reaper cannot see the very rows the
        // restart path creates.
        let adopted = sessions_with(vec![live_row("xnaut-rigtwo-deadbeef", None)]).await;
        assert!(any_live_row(&adopted, "xnaut-rigtwo-deadbeef").await);
    }

    #[test]
    fn a_session_xnaut_did_not_launch_is_never_touched() {
        // The owner's own zellij sessions share the same server. On 2026-09-02
        // they were the MAJORITY of live sessions on this machine, so a reaper
        // that ignored the prefix would collect his working panes.
        assert!(!finished_and_idle(
            "cx-DockerMon",
            Some(0),
            IDLE_REAP_AFTER_MS * 10,
            false,
            Some(0),
            0
        ));
        assert_eq!(handle_in("xnaut-rigtwo-deadbeef"), "rigtwo");
    }

    #[test]
    fn a_run_with_no_capture_file_is_never_reaped() {
        // Nothing here can establish "finished", so nothing is ended. That is
        // an attached session (pty.rs) rather than a launched run, and it has
        // no script(1) capture to read.
        assert!(!finished_and_idle(
            "xnaut-rigtwo-deadbeef",
            None,
            IDLE_REAP_AFTER_MS * 10,
            false,
            Some(0),
            0
        ));
    }

    #[test]
    fn an_idle_reap_says_in_the_ledger_that_it_was_idle() {
        // A session that vanishes with no explanation is indistinguishable from
        // a crash. The kind has to be its own, so the timeline can tell an
        // automation clearing its way from a run that was collected.
        let (_guard, _path) = scratch("idle-reap-ledger");
        assert!(record_reap(
            &Reaped::Ended,
            "idle_reaped",
            "ended xnaut-rigtwo-deadbeef: it finished its task and sat idle",
            "xnaut-rigtwo-deadbeef",
            "rigtwo",
        ));
        let entries = crate::ledger::ledger_recent(Some(10));
        let reaped = entries
            .iter()
            .find(|entry| entry.kind == "idle_reaped")
            .expect("an ended run must never be invisible");
        assert_eq!(reaped.agent, "rigtwo");
        assert!(reaped.detail.contains("sat idle"), "{:?}", reaped.detail);
    }
}
