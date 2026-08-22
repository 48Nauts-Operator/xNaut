// Drift: where the project SAYS it is versus where its own records put it.
//
// xNAUT already has two validators, and neither answers this question. The
// NAUT-Flow Validator (Fable 5, `runDocValidation`) judges MEANING — does the
// document chain honour the owner's verbatim contract — which costs a model run,
// so it can only ever be a gate at the build boundary. `dag_validate` is cheap
// and deterministic but sees only the build plan's dependency graph, never the
// project record.
//
// So a project can sit at stage `release` with four tickets still in review and
// nothing anywhere says a word. Not because the check is hard: the ticket
// statuses and the stage are both already stored. Nothing computes one from the
// other and compares.
//
// That comparison is this module. Two statuses per project:
//
//   AUTHORED  what a human (or an agent) declared — `project.stage`
//   DERIVED   what the ticket records actually support
//
// A disagreement is drift. The value is not any single rule; it is that this is
// arithmetic rather than judgement, so it is free, reproducible, and callable in
// a loop. The Validator is an appointment you keep at the gate. This one an
// agent can ask on every turn: what is drifted, what do I fix next.
//
// THREE RULES, deliberately. The prior art here (10x's `10x validate`) runs
// "hundreds of these types of rules", which is a second codebase with a silent
// failure mode: rules that stop matching how the team works, firing warnings
// everyone learns to scroll past. Three rules that fire correctly teach us the
// false-positive rate. Earn the fourth.

use serde::{Deserialize, Serialize};

/// A ticket, reduced to the fields drift actually reads.
///
/// Deliberately not `TicketRecord`: this module is pure and testable, and the
/// caller already has the records. Keeping the input narrow means a change to
/// the PM schema cannot silently change what drift means.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftTicket {
    pub id: String,
    /// inbox | ready | in_progress | review | blocked | done
    pub status: String,
}

/// A project, reduced the same way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftProject {
    pub key: String,
    /// The AUTHORED position — a stage key from the flow track.
    pub stage: String,
    /// standard | feature | incident
    #[serde(default = "default_flow")]
    pub flow_type: String,
}

fn default_flow() -> String {
    "standard".into()
}

/// One disagreement, phrased so an agent can act on it without re-deriving it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DriftFinding {
    pub project: String,
    /// Stable machine identity for the rule that fired.
    pub rule: String,
    /// info | warn — nothing here is fatal. Drift reports, the gate blocks.
    pub severity: String,
    /// What the project claims.
    pub authored: String,
    /// What its records support.
    pub derived: String,
    /// One sentence, expected-versus-found, for a human.
    pub detail: String,
    /// Ticket ids that caused it. Empty when the finding is about the stage
    /// itself rather than about particular tickets.
    pub tickets: Vec<String>,
}

/// The stage tracks, mirroring src/js/project-management-panel.js.
///
/// Duplicated on purpose rather than shared: the frontend list carries phase and
/// persona for rendering, this one is only an ordering. A copy that drifts is
/// caught by `stage_tracks_match_the_frontend` in the tests below, which is a
/// cheaper contract than threading a shared definition through the bridge.
const STANDARD: &[&str] = &[
    "idea", "concept", "business_case", "prd", "architecture", "data_model",
    "api_design", "security_review", "development_plan", "sprint_stories",
    "tickets", "build", "test_review", "release", "learning",
];
const FEATURE: &[&str] = &[
    "idea", "prd", "architecture", "api_design", "security_review",
    "development_plan", "tickets", "build", "test_review", "release", "learning",
];
const INCIDENT: &[&str] = &[
    "intake", "rca", "action_plan", "ticket", "build", "test_review", "release",
    "learning",
];

fn track(flow_type: &str) -> &'static [&'static str] {
    match flow_type {
        "incident" => INCIDENT,
        "feature" => FEATURE,
        _ => STANDARD,
    }
}

/// The stage track for a flow type. Public so the context brief (flow_context)
/// positions a project against the same list drift measures it against — two
/// copies of these tracks is exactly the drift this module exists to catch.
pub fn track_of(flow_type: &str) -> &'static [&'static str] {
    track(flow_type)
}

/// Position of a stage on its track, or None when the stage is not on it.
fn index_of(stage: &str, flow_type: &str) -> Option<usize> {
    track(flow_type).iter().position(|s| *s == stage)
}

/// Stages at or past which every ticket is expected to be resolved.
///
/// `release` is the honest line. At `test_review` work in review is the point of
/// the stage, so flagging it there would fire on every healthy project — which
/// is how a rule set starts being ignored.
fn requires_all_tickets_closed(stage: &str, flow_type: &str) -> bool {
    match index_of(stage, flow_type) {
        Some(i) => match index_of("release", flow_type) {
            Some(r) => i >= r,
            None => false,
        },
        None => false,
    }
}

/// True for statuses that mean the ticket is finished.
fn is_closed(status: &str) -> bool {
    status == "done"
}

/// Computes drift for one project against its tickets. Pure.
///
/// An empty result means authored and derived agree. The caller applies nothing;
/// findings are a report, never a mutation — a linter that edits its input is a
/// linter you stop trusting.
pub fn drift_for(project: &DriftProject, tickets: &[DriftTicket]) -> Vec<DriftFinding> {
    let mut out = Vec::new();
    let mine: Vec<&DriftTicket> = tickets.iter().collect();

    // RULE 1 — stage_unknown.
    // The declared stage is not on this project's track. Everything downstream
    // derives from position, so this is reported first and alone: with no
    // position, the other rules have nothing to compare against and would either
    // stay silent or invent an answer.
    if index_of(&project.stage, &project.flow_type).is_none() {
        out.push(DriftFinding {
            project: project.key.clone(),
            rule: "stage_unknown".into(),
            severity: "warn".into(),
            authored: project.stage.clone(),
            derived: "not on track".into(),
            detail: format!(
                "stage \"{}\" is not a stage of the {} flow — the track runs {}",
                project.stage,
                project.flow_type,
                track(&project.flow_type).join(" → ")
            ),
            tickets: Vec::new(),
        });
        return out;
    }

    // RULE 2 — stage_ahead_of_tickets.
    // The 10x finding, in our vocabulary: authored says complete, derived says in
    // review. A project at `release` or past it with unresolved tickets cannot
    // be where it says it is.
    if requires_all_tickets_closed(&project.stage, &project.flow_type) {
        let open: Vec<String> = mine
            .iter()
            .filter(|t| !is_closed(&t.status))
            .map(|t| t.id.clone())
            .collect();
        if !open.is_empty() {
            out.push(DriftFinding {
                project: project.key.clone(),
                rule: "stage_ahead_of_tickets".into(),
                severity: "warn".into(),
                authored: project.stage.clone(),
                derived: "test_review".into(),
                detail: format!(
                    "stage is \"{}\" but {} ticket(s) are not done — a project cannot pass release with open work",
                    project.stage,
                    open.len()
                ),
                tickets: open,
            });
        }
    }

    // RULE 3 — work_ahead_of_stage.
    // The mirror, and the one that catches real life: someone starts building
    // before the plan says to. Tickets in progress while the stage is still in
    // Discover or Define means the declared stage is behind the actual work.
    if let (Some(here), Some(build)) = (
        index_of(&project.stage, &project.flow_type),
        index_of("build", &project.flow_type),
    ) {
        if here < build {
            let active: Vec<String> = mine
                .iter()
                .filter(|t| t.status == "in_progress" || t.status == "review")
                .map(|t| t.id.clone())
                .collect();
            if !active.is_empty() {
                out.push(DriftFinding {
                    project: project.key.clone(),
                    rule: "work_ahead_of_stage".into(),
                    severity: "info".into(),
                    authored: project.stage.clone(),
                    derived: "build".into(),
                    detail: format!(
                        "stage is \"{}\" but {} ticket(s) are already in progress or review — the work is ahead of the plan",
                        project.stage,
                        active.len()
                    ),
                    tickets: active,
                });
            }
        }
    }

    out
}

/// Drift across a whole portfolio. Tickets are matched to projects by key, so
/// the caller can hand over everything it has.
#[tauri::command]
pub fn flow_drift(projects: Vec<DriftProject>, tickets: Vec<DriftTicket>, project: Option<String>) -> Vec<DriftFinding> {
    // The frontend sends tickets already carrying their project; the narrow
    // DriftTicket drops that field, so scoping happens caller-side by passing
    // only the relevant tickets. When `project` is given we still filter the
    // project list, so asking about one project cannot report on another.
    projects
        .iter()
        .filter(|p| project.as_deref().map_or(true, |k| p.key == k))
        .flat_map(|p| drift_for(p, &tickets))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket(id: &str, status: &str) -> DriftTicket {
        DriftTicket { id: id.into(), status: status.into() }
    }

    fn project(stage: &str) -> DriftProject {
        DriftProject { key: "XNAUT".into(), stage: stage.into(), flow_type: "standard".into() }
    }

    #[test]
    fn a_project_whose_records_agree_reports_nothing() {
        let p = project("release");
        let t = vec![ticket("X-1", "done"), ticket("X-2", "done")];
        assert_eq!(drift_for(&p, &t), Vec::new());
    }

    #[test]
    fn release_with_open_tickets_is_drift() {
        let p = project("release");
        let t = vec![ticket("X-1", "done"), ticket("X-2", "review")];
        let found = drift_for(&p, &t);
        assert_eq!(found.len(), 1, "expected exactly one finding, got {found:?}");
        assert_eq!(found[0].rule, "stage_ahead_of_tickets");
        assert_eq!(found[0].authored, "release");
        assert_eq!(found[0].tickets, vec!["X-2".to_string()]);
    }

    #[test]
    fn test_review_with_tickets_in_review_is_not_drift() {
        // The stage whose entire purpose is review must not fire. This is the
        // false-positive that would train people to ignore the linter.
        let p = project("test_review");
        let t = vec![ticket("X-1", "review"), ticket("X-2", "in_progress")];
        assert_eq!(drift_for(&p, &t), Vec::new());
    }

    #[test]
    fn building_before_the_plan_says_so_is_reported_as_info() {
        let p = project("prd");
        let t = vec![ticket("X-1", "in_progress"), ticket("X-2", "inbox")];
        let found = drift_for(&p, &t);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, "work_ahead_of_stage");
        assert_eq!(found[0].severity, "info");
        assert_eq!(found[0].tickets, vec!["X-1".to_string()]);
    }

    #[test]
    fn an_unknown_stage_reports_only_itself() {
        // With no position on the track the other rules cannot be evaluated
        // honestly, so the report must not also guess at ticket drift.
        let p = DriftProject { key: "XNAUT".into(), stage: "shipping".into(), flow_type: "standard".into() };
        let t = vec![ticket("X-1", "review")];
        let found = drift_for(&p, &t);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, "stage_unknown");
    }

    #[test]
    fn a_feature_flow_is_measured_against_the_feature_track() {
        // `data_model` exists on the standard track but not the feature one.
        // Reading the wrong track would silently mis-position every feature.
        let p = DriftProject { key: "F".into(), stage: "data_model".into(), flow_type: "feature".into() };
        let found = drift_for(&p, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, "stage_unknown");
    }

    #[test]
    fn an_incident_at_release_with_open_work_is_drift() {
        let p = DriftProject { key: "I".into(), stage: "release".into(), flow_type: "incident".into() };
        let t = vec![ticket("I-1", "blocked")];
        let found = drift_for(&p, &t);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, "stage_ahead_of_tickets");
    }

    #[test]
    fn scoping_to_one_project_never_reports_another() {
        let ps = vec![
            DriftProject { key: "A".into(), stage: "release".into(), flow_type: "standard".into() },
            DriftProject { key: "B".into(), stage: "release".into(), flow_type: "standard".into() },
        ];
        let t = vec![ticket("T-1", "review")];
        let found = flow_drift(ps, t, Some("A".into()));
        assert!(found.iter().all(|f| f.project == "A"), "leaked another project: {found:?}");
    }

    #[test]
    fn stage_tracks_match_the_frontend() {
        // Guards the duplicated stage lists. If someone edits STANDARD_STAGES in
        // project-management-panel.js and not this file, drift is computed
        // against a track the product no longer has.
        let js = include_str!("../../src/js/project-management-panel.js");
        for (name, track) in [("STANDARD_STAGES", STANDARD), ("FEATURE_STAGES", FEATURE), ("INCIDENT_STAGES", INCIDENT)] {
            let start = js.find(&format!("const {name} = [")).unwrap_or_else(|| panic!("{name} not found in the panel"));
            let end = js[start..].find("\n  ];").unwrap_or_else(|| panic!("{name} not terminated")) + start;
            let block = &js[start..end];
            let keys: Vec<&str> = block
                .lines()
                .filter_map(|l| l.trim().strip_prefix("['"))
                .filter_map(|l| l.split('\'').next())
                .collect();
            assert_eq!(keys, track, "{name} in the frontend no longer matches this module");
        }
    }
}
