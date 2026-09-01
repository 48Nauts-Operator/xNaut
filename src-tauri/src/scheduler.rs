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
    /// The previous run's zellij session, killed before the new one starts.
    reap: Option<String>,
    request: crate::agent_profiles::LaunchAgentProfileRequest,
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

/// Kills the zellij session a previous run of this automation left behind.
///
/// Four fires on the rig left four sessions and at least three idle `claude`
/// processes, and nothing ever collected them; at `every:1m` that is roughly
/// sixty orphaned agents an hour.
///
/// ponytail: kills the zellij session only. The PTY that hosted its client
/// exits with it, and a stale status row is corrected by the next
/// `adopt_surviving_runs` pass.
async fn reap_previous(session: String, agent: &str) {
    let name = session.clone();
    let result = tokio::task::spawn_blocking(move || crate::zellij::remove_session(&name))
        .await
        .unwrap_or_else(|e| Err(format!("reap task panicked: {e}")));
    match result {
        Ok(()) => crate::ledger::record(
            "automation_reaped",
            agent,
            "",
            &format!("ended the previous run in {session} before starting the next"),
        ),
        Err(e) => eprintln!("[scheduler] could not reap {session}: {e}"),
    }
}

/// Runs one automation: resolve the agent, reap the last run, launch durable
/// through the profile launcher (which is where the composer runs).
///
/// ponytail: the plan is tested, this ordering is not. `plan_fire` proves WHAT
/// a fire does (which session it reaps, that the launch is durable and not a
/// resume); that the reap happens BEFORE the launch is three straight-line
/// statements here, and covering it would mean injecting both effects.
async fn fire(app: &AppHandle, auto: &Automation) -> Result<FireOutcome, String> {
    let plan = plan_fire(auto, |spoken| {
        crate::agent_profiles::resolve_spoken_handle(spoken)
    })?;
    if let Some(previous) = plan.reap {
        reap_previous(previous, &plan.request.handle).await;
    }
    let state = tauri::Manager::state::<crate::state::AppState>(app);
    let response = crate::agent_profiles::agent_profile_launch(app.clone(), state, plan.request).await?;
    Ok(FireOutcome {
        session_id: response.session_id,
        zellij_session: response.zellij_session,
    })
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
    Fut: std::future::Future<Output = Result<FireOutcome, String>>,
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
        let outcome = fire_and_record(&auto, &launch).await;
        // last_fired advances only for a run that actually started. It used to
        // be persisted BEFORE the fire, so a fire nobody handled still consumed
        // the slot; on `hourly` that is an hour of silence per miss, and on the
        // rig it is why a no-op fire looked identical to a successful one.
        if let Ok(run) = &outcome {
            autos[i].last_fired = Some(now.to_rfc3339());
            autos[i].last_session = run.zellij_session.clone();
            if let Err(e) = save_automations(&autos) {
                eprintln!(
                    "[scheduler] failed to persist last_fired for {:?}: {e}",
                    auto.name
                );
            }
        }
        fired.push((autos[i].clone(), outcome));
    }
    fired
}

/// Fires one automation and puts both halves in the ledger.
///
/// A fire AND its outcome, because `last_fired` says "fired", not "ran". All
/// four rig runs succeeded and left zero ledger entries, so "did last night's
/// automations run?" had no answer anywhere in the app.
async fn fire_and_record<F, Fut>(auto: &Automation, launch: &F) -> Result<FireOutcome, String>
where
    F: Fn(Automation) -> Fut,
    Fut: std::future::Future<Output = Result<FireOutcome, String>>,
{
    crate::ledger::record(
        "automation_fired",
        &auto.agent_id,
        "",
        &format!("{} ({})", auto.name, auto.schedule),
    );
    let outcome = launch(auto.clone()).await;
    match &outcome {
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
    outcome
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
    let outcome = fire_and_record(&auto, &|auto: Automation| {
        let app = app.clone();
        async move { fire(&app, &auto).await }
    })
    .await;
    if let Ok(run) = &outcome {
        autos[idx].last_fired = Some(Local::now().to_rfc3339());
        autos[idx].last_session = run.zellij_session.clone();
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
            Err("no agent called \"rigtwo\"".to_string())
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
            Ok(started("abc123"))
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

        tick_with(at(2026, 6, 10, 10, 0), |_| async { Ok(started("abc123")) }).await;
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
            Err("unknown agent id: rigtwo".to_string())
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
}
