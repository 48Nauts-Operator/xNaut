// v1.6 Automations: user-defined scheduled agent runs (modeled on Orca's automation modal).
// The backend only evaluates schedules + prechecks and emits events — the frontend opens sessions.

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
    /// agents.toml id.
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
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn automations_path() -> PathBuf {
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
    let dir = config_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let path = automations_path();
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

// ─── Background task ─────────────────────────────────────────────────────────

/// Spawns the scheduler loop: every 60s, evaluate all automations and emit
/// "automation://fire" for each one that's due. The frontend opens the session.
pub fn spawn_scheduler_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            tick(&app).await;
        }
    });
}

async fn tick(app: &AppHandle) {
    let mut autos = load_automations();
    if autos.is_empty() {
        return;
    }
    let now = Local::now();
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
        // Persist last_fired BEFORE emitting — a crashed frontend must not be
        // able to cause a refire loop. If we can't persist, don't fire.
        autos[i].last_fired = Some(now.to_rfc3339());
        if let Err(e) = save_automations(&autos) {
            eprintln!(
                "[scheduler] failed to persist last_fired for {:?}: {e} — not firing",
                auto.name
            );
            continue;
        }
        if let Err(e) = app.emit(
            "automation://fire",
            serde_json::json!({ "automation": autos[i] }),
        ) {
            eprintln!(
                "[scheduler] failed to emit automation://fire for {:?}: {e}",
                auto.name
            );
        }
    }
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
        Some(slot) => *slot = automation.clone(),
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
    autos[idx].last_fired = Some(Local::now().to_rfc3339());
    save_automations(&autos)?;
    app.emit(
        "automation://fire",
        serde_json::json!({ "automation": autos[idx] }),
    )
    .map_err(|e| format!("failed to emit automation://fire: {e}"))
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
}
