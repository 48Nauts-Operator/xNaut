// PM Space (v1.7) — the LEGACY registry of external (client) projects.
//
// This is now a read-only migration source, not a feature. The Project
// Management board owns client projects; `project_management::migrate_legacy_pm_data`
// (called from `pm_project_import_existing`) folds whatever is still sitting in
// pm-projects.json into a ProjectRecord's `client` field, which
// project-management-panel.js:756 reads as `legacy`. Nothing writes this file
// any more.
//
// XNAUT-265 removed the rest: pm_list / pm_get / pm_save / pm_delete /
// pm_financials, the save path, and the margin calculator. All five commands
// were registered and ACL'd, and exactly one of them had a caller: pm_save, from
// pm-intake-modal.js, whose opener (window.xnautOpenPmIntake) nothing ever
// called. The read side had no caller at all, so the store was write-only: a
// wired-up button would have saved a project no surface could display.

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

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn projects_path() -> PathBuf {
    config_dir().join("pm-projects.json")
}

/// Parses a registry file body. A parse error yields an empty list rather than
/// aborting the migration: a corrupt legacy file must not stop the Project
/// Management board from loading. Split out of `load_pm_projects` so the shape
/// the migration depends on is testable without touching the real config dir.
fn parse_registry(body: &str, whence: &std::path::Path) -> Vec<ExternalProject> {
    serde_json::from_str(body).unwrap_or_else(|e| {
        eprintln!(
            "[pm] parse error in {}: {e} — starting with empty registry",
            whence.display()
        );
        Vec::new()
    })
}

/// Loads the legacy PM project registry. A missing file yields an empty list.
pub fn load_pm_projects() -> Vec<ExternalProject> {
    let path = projects_path();
    match std::fs::read_to_string(&path) {
        Ok(body) => parse_registry(&body, &path),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This module survives XNAUT-265 for exactly one reason: it feeds
    /// `migrate_legacy_pm_data`, which reads `client_company`,
    /// `offer_amount_chf`, `rate_chf_per_hour`, `scope`, `contacts`, `task_id`
    /// and `created` off each entry (project_management.rs:1093-1122) and hands
    /// them to the Project Management board.
    ///
    /// A legacy file on disk was written by a build that is gone, so nothing in
    /// this repo re-serializes it and no compile error would fire if a field
    /// were renamed. This pins the wire shape the migration depends on.
    #[test]
    fn the_legacy_shape_the_migration_reads_still_parses() {
        let body = r#"[{
            "id": "9f1c",
            "task_id": "t-42",
            "client_company": "Acme AG",
            "contacts": [{"name": "Rita", "email": "rita@acme.ch", "role": "CTO"}],
            "scope": "Rebuild the intake",
            "rate_chf_per_hour": 180.0,
            "offer_amount_chf": 24000.0,
            "created": "2026-06-10T00:00:00Z"
        }]"#;
        let projects = parse_registry(body, std::path::Path::new("test"));
        assert_eq!(projects.len(), 1, "one entry in, one entry out");
        let p = &projects[0];
        assert_eq!(p.task_id, "t-42");
        assert_eq!(p.client_company, "Acme AG");
        assert_eq!(p.scope, "Rebuild the intake");
        assert_eq!(p.rate_chf_per_hour, 180.0);
        assert_eq!(p.offer_amount_chf, Some(24000.0));
        assert_eq!(p.created, "2026-06-10T00:00:00Z");
        assert_eq!(p.contacts.len(), 1);
        assert_eq!(p.contacts[0].name, "Rita");
        assert_eq!(p.contacts[0].email, "rita@acme.ch");
    }

    /// A corrupt legacy file must degrade to "no legacy projects", never take
    /// the Project Management board down with it: `pm_project_import_existing`
    /// calls straight into this on every panel refresh.
    #[test]
    fn a_corrupt_registry_yields_no_projects_rather_than_failing() {
        let projects = parse_registry("{ this is not json", std::path::Path::new("test"));
        assert!(projects.is_empty());
    }
}

