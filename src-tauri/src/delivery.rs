// Delivery lifecycle (XNAUT-329).
//
// Delivery > Tests used to show raw step logs and nothing about what was
// tested or what it proved. Every part of a ticket's story is already on
// disk, in five different places: the ticket JSON, the plan job the jury
// approved, the handback the agent filed, the verify record, and the
// sign-off. This module is the one reader that joins them, so the panel can
// show a ticket's life as stages instead of a log.
//
// Nothing here computes anything new. A stage with no source is returned
// empty rather than omitted: "not reached yet" is information.

use serde::Serialize;

/// One stage of the lifecycle. Always present, `at`/`text` empty when the
/// ticket has not reached it.
#[derive(Serialize, Default, Clone, Debug)]
pub struct Stage {
    /// issue | proposed | final | tested | done | merged | learnings
    pub key: String,
    pub title: String,
    /// RFC3339, empty when not reached.
    pub at: String,
    pub text: String,
    /// Files, commits, failing tests: whatever the stage lists.
    pub items: Vec<String>,
}

#[derive(Serialize, Default, Clone, Debug)]
pub struct SuiteTotal {
    pub name: String,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
}

#[derive(Serialize, Default, Clone, Debug)]
pub struct StepChip {
    pub name: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
}

/// What the verify proved, interpreted. The raw logs stay behind the panel's
/// one "raw output" control.
#[derive(Serialize, Default, Clone, Debug)]
pub struct VerifySummary {
    pub record_id: String,
    pub status: String,
    pub suites: Vec<SuiteTotal>,
    pub failing: Vec<String>,
    pub steps: Vec<StepChip>,
    pub sandbox: String,
    pub commit: String,
}

/// Paths on this machine, or a reason there are none (XNAUT-330 fills them).
#[derive(Serialize, Default, Clone, Debug)]
pub struct Evidence {
    pub video_path: String,
    pub screenshot_path: String,
    pub note: String,
}

#[derive(Serialize, Default, Clone, Debug)]
pub struct Lifecycle {
    pub ticket: String,
    pub project: String,
    pub title: String,
    pub kind: String,
    pub priority: String,
    pub owner: String,
    pub branch: String,
    pub status: String,
    pub body: String,
    /// Always seven, in order: issue, proposed, final, tested, done, merged,
    /// learnings.
    pub stages: Vec<Stage>,
    pub verify: Option<VerifySummary>,
    pub evidence: Evidence,
}

pub const STAGES: [(&str, &str); 7] = [
    ("issue", "Issue"),
    ("proposed", "Proposed solution"),
    ("final", "Final solution"),
    ("tested", "Tested"),
    ("done", "Done"),
    ("merged", "Merged"),
    ("learnings", "Learnings"),
];

/// One ticket's whole story, joined from the five sources.
#[tauri::command]
pub async fn delivery_lifecycle(ticket: String) -> Result<Lifecycle, String> {
    let _ = ticket;
    Err("not implemented".into())
}
