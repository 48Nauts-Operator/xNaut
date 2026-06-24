// PM Space (v1.7) — registry of external (client) projects linked to Tasks Mode
// entries, plus financials computed from the v1.5 worklog sessions.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A client-side contact person on an external project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub name: String,
    pub email: String,
    pub role: String,
}

/// One external (client) project. Linked 1:1 to a Tasks Mode entry via
/// `task_id`; the financial fields feed the PM Space dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalProject {
    /// uuid v4 — empty on save means "assign one".
    pub id: String,
    /// Links to tasks.json TaskSession.id.
    pub task_id: String,
    pub client_company: String,
    #[serde(default)]
    pub contacts: Vec<Contact>,
    #[serde(default)]
    pub scope: String,
    pub rate_chf_per_hour: f64,
    #[serde(default)]
    pub offer_amount_chf: Option<f64>,
    #[serde(default)]
    pub expected_close: Option<String>,
    #[serde(default)]
    pub plow_opportunity_id: Option<String>,
    #[serde(default)]
    pub lineary_project_id: Option<String>,
    /// RFC3339 — filled on first save.
    pub created: String,
}

/// Computed financial snapshot for one external project, derived from the
/// worklog sessions that match the project's task name or client company.
#[derive(Debug, Clone, Serialize)]
pub struct Financials {
    pub hours: f64,
    pub sessions: u32,
    pub burn_chf: f64,
    pub offer_chf: Option<f64>,
    /// offer − burn.
    pub margin_chf: Option<f64>,
    /// offer ÷ hours (None when hours ≈ 0 or no offer).
    pub effective_rate_chf: Option<f64>,
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn projects_path() -> PathBuf {
    config_dir().join("pm-projects.json")
}

/// Worklog session files live under the home dir, NOT the config dir —
/// see worklog.rs.
fn worklogs_dir() -> PathBuf {
    dirs::home_dir()
        .map(|p| p.join(".xnaut").join("worklogs"))
        .unwrap_or_else(|| PathBuf::from(".xnaut/worklogs"))
}

/// Loads the PM project registry. A missing file or parse error yields an
/// empty list (parse errors are logged, matching the tasks.rs pattern).
pub fn load_pm_projects() -> Vec<ExternalProject> {
    let path = projects_path();
    match std::fs::read_to_string(&path) {
        Ok(body) => serde_json::from_str(&body).unwrap_or_else(|e| {
            eprintln!(
                "[pm] parse error in {}: {e} — starting with empty registry",
                path.display()
            );
            Vec::new()
        }),
        Err(_) => Vec::new(),
    }
}

/// Writes the full registry to config_dir()/pm-projects.json, creating the
/// dir. Client/financial data lives in here — keep the file owner-only.
pub fn save_pm_projects(projects: &[ExternalProject]) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let path = projects_path();
    let body = serde_json::to_string_pretty(projects).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Pure upsert: replace the entry with a matching id, else append.
fn upsert_into(projects: &mut Vec<ExternalProject>, p: ExternalProject) {
    if let Some(existing) = projects.iter_mut().find(|x| x.id == p.id) {
        *existing = p;
    } else {
        projects.push(p);
    }
}

/// Round money (and hours) to 2 decimals.
fn money(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Duration of one worklog session in hours. Open sessions (no `ended`) are
/// measured against `now`. Negative or >24h spans are clamped to 0..24 as a
/// corrupt-data guard. Unparseable timestamps yield 0.
fn session_hours(started: &str, ended: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> f64 {
    let start = match chrono::DateTime::parse_from_rfc3339(started) {
        Ok(t) => t.with_timezone(&chrono::Utc),
        Err(_) => return 0.0,
    };
    let end = match ended {
        Some(e) => match chrono::DateTime::parse_from_rfc3339(e) {
            Ok(t) => t.with_timezone(&chrono::Utc),
            Err(_) => return 0.0,
        },
        None => now,
    };
    let hours = (end - start).num_seconds() as f64 / 3600.0;
    hours.clamp(0.0, 24.0)
}

/// True when a worklog session's `project` field matches the resolved task
/// name or the client company (case-insensitive, trimmed).
fn name_matches(session_project: &str, task_name: &str, client_company: &str) -> bool {
    let p = session_project.trim().to_lowercase();
    p == task_name.trim().to_lowercase() || p == client_company.trim().to_lowercase()
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

/// Lists all registered external projects.
#[tauri::command]
pub fn pm_list() -> Result<Vec<ExternalProject>, String> {
    Ok(load_pm_projects())
}

/// Returns the external project linked to a task, if any.
#[tauri::command]
pub fn pm_get(task_id: String) -> Result<Option<ExternalProject>, String> {
    Ok(load_pm_projects()
        .into_iter()
        .find(|p| p.task_id == task_id))
}

/// Inserts or updates (by id) an external project. Empty id gets a fresh
/// uuid; empty created gets the current RFC3339 timestamp.
#[tauri::command]
pub fn pm_save(mut project: ExternalProject) -> Result<ExternalProject, String> {
    if project.id.is_empty() {
        project.id = uuid::Uuid::new_v4().to_string();
    }
    if project.created.is_empty() {
        project.created = chrono::Utc::now().to_rfc3339();
    }
    let mut projects = load_pm_projects();
    upsert_into(&mut projects, project.clone());
    save_pm_projects(&projects)?;
    Ok(project)
}

/// Removes a registry entry by id.
#[tauri::command]
pub fn pm_delete(id: String) -> Result<(), String> {
    let mut projects = load_pm_projects();
    let before = projects.len();
    projects.retain(|p| p.id != id);
    if projects.len() == before {
        return Err(format!("no PM project with id {id}"));
    }
    save_pm_projects(&projects)
}

/// Computes hours/burn/margin for the project linked to `task_id` by scanning
/// the worklog session files under ~/.xnaut/worklogs.
#[tauri::command]
pub fn pm_financials(task_id: String) -> Result<Financials, String> {
    let project = load_pm_projects()
        .into_iter()
        .find(|p| p.task_id == task_id)
        .ok_or_else(|| "no PM entry for this project".to_string())?;

    // Worklog sessions store the task's display name in their `project`
    // field, so resolve it from the task registry (fallback: client company).
    let task_name = crate::tasks::load_tasks()
        .into_iter()
        .find(|t| t.id == task_id)
        .map(|t| t.name)
        .unwrap_or_else(|| project.client_company.clone());

    let now = chrono::Utc::now();
    let mut hours = 0.0_f64;
    let mut sessions = 0_u32;

    if let Ok(entries) = std::fs::read_dir(worklogs_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(body) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
                continue; // unparseable files are skipped silently
            };
            let Some(session_project) = value.get("project").and_then(|v| v.as_str()) else {
                continue;
            };
            if !name_matches(session_project, &task_name, &project.client_company) {
                continue;
            }
            let Some(started) = value.get("started").and_then(|v| v.as_str()) else {
                continue;
            };
            let ended = value.get("ended").and_then(|v| v.as_str());
            hours += session_hours(started, ended, now);
            sessions += 1;
        }
    }

    let hours = money(hours);
    let burn_chf = money(hours * project.rate_chf_per_hour);
    let offer_chf = project.offer_amount_chf;
    let margin_chf = offer_chf.map(|o| money(o - burn_chf));
    let effective_rate_chf = match offer_chf {
        Some(o) if hours > 0.005 => Some(money(o / hours)),
        _ => None,
    };

    Ok(Financials {
        hours,
        sessions,
        burn_chf,
        offer_chf,
        margin_chf,
        effective_rate_chf,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    fn project(id: &str, task_id: &str) -> ExternalProject {
        ExternalProject {
            id: id.into(),
            task_id: task_id.into(),
            client_company: "Acme AG".into(),
            contacts: vec![],
            scope: String::new(),
            rate_chf_per_hour: 180.0,
            offer_amount_chf: None,
            expected_close: None,
            plow_opportunity_id: None,
            lineary_project_id: None,
            created: "2026-06-10T00:00:00Z".into(),
        }
    }

    #[test]
    fn session_hours_normal_two_hour_session() {
        let now = utc("2026-06-10T20:00:00Z");
        let h = session_hours("2026-06-10T08:00:00Z", Some("2026-06-10T10:00:00Z"), now);
        assert!((h - 2.0).abs() < 1e-9);
    }

    #[test]
    fn session_hours_open_session_uses_now() {
        let now = utc("2026-06-10T11:30:00Z");
        let h = session_hours("2026-06-10T08:00:00Z", None, now);
        assert!((h - 3.5).abs() < 1e-9);
    }

    #[test]
    fn session_hours_over_24h_clamps_to_24() {
        let now = utc("2026-06-20T00:00:00Z");
        let h = session_hours("2026-06-10T08:00:00Z", Some("2026-06-13T08:00:00Z"), now);
        assert!((h - 24.0).abs() < 1e-9);
    }

    #[test]
    fn session_hours_ended_before_started_clamps_to_zero() {
        let now = utc("2026-06-10T20:00:00Z");
        let h = session_hours("2026-06-10T10:00:00Z", Some("2026-06-10T08:00:00Z"), now);
        assert_eq!(h, 0.0);
    }

    #[test]
    fn money_rounds_to_two_decimals() {
        assert_eq!(money(123.456), 123.46);
        assert_eq!(money(123.454), 123.45);
        assert_eq!(money(0.005), 0.01);
        assert_eq!(money(0.0), 0.0);
    }

    #[test]
    fn upsert_appends_when_id_is_new() {
        let mut projects = vec![project("a", "t1")];
        upsert_into(&mut projects, project("b", "t2"));
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[1].id, "b");
    }

    #[test]
    fn upsert_replaces_in_place_when_id_exists() {
        let mut projects = vec![project("a", "t1"), project("b", "t2")];
        let mut updated = project("a", "t1");
        updated.client_company = "Globex GmbH".into();
        upsert_into(&mut projects, updated);
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].client_company, "Globex GmbH");
        assert_eq!(projects[1].id, "b");
    }
}
