// The decision log — why, captured at the moment of the decision.
//
// This is NOT another activity log. worklog.rs already records what commands
// ran (with a merkle proof, for billing), and build_log.rs records what
// happened during a build. Both answer "what did it do". Neither answers the
// only question worth asking when you parachute into eight hours of agent work:
// *why is it like this*.
//
// The reason this cannot be solved by summarising afterwards: an agent asked at
// the end to explain its own run is reconstructing from a context window that
// has already evicted the fork it is being asked about. It will produce
// something fluent, confidently smoothing over the argument that actually
// mattered. So the rationale is written AT the boundary, by the agent that is
// standing at it, or it is not written at all.
//
// Two halves, deliberately:
//   - the WHAT is captured automatically by the hook (agent_hooks.rs), because
//     anything an agent has to remember to do, it will eventually not do;
//   - the WHY is a one-line rationale the agent must emit itself, because no
//     amount of structured telemetry can reconstruct a judgement call.
//
// Storage is build_log's JSONL, keyed by project rather than by build: the
// question is "what happened on this project", and a decision that only exists
// inside one build's file is invisible the moment the next run starts.

use serde::{Deserialize, Serialize};

/// Boundaries worth stopping at. Deliberately short: a hook on every tool call
/// produces an activity log with extra steps, which is the thing this is not.
/// Anything unrecognised is kept verbatim rather than dropped — a log that
/// silently discards what it does not understand is worse than a noisy one.
pub const BOUNDARIES: [&str; 6] = [
    "task.start",
    "task.done",
    "verify.pass",
    "verify.fail",
    "council.verdict",
    "merge",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Decision {
    #[serde(default)]
    pub ts: String,
    /// Which agent/persona stood at the fork. "Architect", "Builder", "hook".
    #[serde(default)]
    pub role: String,
    /// One of BOUNDARIES, or a verbatim unknown.
    #[serde(default)]
    pub boundary: String,
    /// The structured what. Written by the hook, or by the agent alongside why.
    #[serde(default)]
    pub what: String,
    /// The one-line rationale. THE point of this module. Empty means the
    /// boundary was recorded but nobody said why — which the brief reports
    /// rather than hides.
    #[serde(default)]
    pub why: String,
    /// What was considered and rejected. Optional, but it is the difference
    /// between a decision and an announcement.
    #[serde(default)]
    pub alternatives: String,
    /// Correlation key. A later entry naming the same `key` with `open: false`
    /// closes this one. That is the ONLY way an open item stops being open.
    #[serde(default)]
    pub key: String,
    /// Whether this left something unresolved.
    #[serde(default)]
    pub open: bool,
}

/// The layered brief. Layers exist so volume never buries the state of the
/// world: `headline` is what you read in five seconds, `open` is what you read
/// before touching anything, `detail` is what you read when you actually care.
///
/// The rule that makes this trustworthy, and the rule every future summariser
/// over it must also obey: it may SHORTEN and GROUP, it may never RESOLVE. An
/// open disagreement has to still read as open at every layer. A summariser
/// that is allowed to decide is just another agent with an opinion, except its
/// opinion is laundered as a record of what happened.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Brief {
    /// At most five lines: the most recent decision per role.
    pub headline: Vec<Decision>,
    /// Everything still unresolved, oldest first. Never summarised away.
    pub open: Vec<Decision>,
    /// Boundaries recorded with no rationale. Surfaced, not hidden: a silent
    /// gap in the record is exactly what this module exists to prevent.
    pub unexplained: usize,
    /// Full log, newest last.
    pub detail: Vec<Decision>,
}

fn normalise_boundary(b: &str) -> String {
    let t = b.trim();
    if t.is_empty() {
        return "task.done".into();
    }
    t.to_string()
}

/// Append one decision. Never fails: the same rule build_log.rs and audit.rs
/// follow, because a log that can fail the thing it records is a liability.
pub fn append(project: &str, d: &Decision) {
    let mut d = d.clone();
    if d.ts.is_empty() {
        d.ts = chrono::Utc::now().to_rfc3339();
    }
    d.boundary = normalise_boundary(&d.boundary);
    let level = if d.boundary == "verify.fail" { "warn" } else { "info" };
    crate::build_log::build_log_append(
        log_id(project),
        level.into(),
        if d.role.is_empty() { "agent".into() } else { d.role.clone() },
        if d.what.is_empty() { d.why.clone() } else { d.what.clone() },
        Some("decision".into()),
        serde_json::to_value(&d).ok(),
    );
}

/// Decisions live in their own file per project, beside the build logs. Keeping
/// them out of `<build_id>.jsonl` is what makes them outlive a single run.
pub fn log_id(project: &str) -> String {
    format!("decisions-{}", project.trim())
}

/// Read every decision for a project, oldest first.
pub fn read(project: &str) -> Vec<Decision> {
    crate::build_log::read_events(&log_id(project))
        .into_iter()
        .filter(|e| e.kind == "decision")
        .filter_map(|e| serde_json::from_value::<Decision>(e.data).ok())
        .collect()
}

/// Build the layered brief. Deliberately mechanical — no model runs here.
///
/// That is not a placeholder for a smarter version later. Selection and
/// grouping are the parts that must be incapable of editorialising, so they are
/// code. A language model can be layered ON TOP to compress the prose of the
/// headline, but it must never be what decides which items are open.
pub fn brief(project: &str) -> Brief {
    let all = read(project);
    Brief {
        unexplained: all.iter().filter(|d| d.why.trim().is_empty()).count(),
        headline: headline_of(&all),
        open: open_of(&all),
        detail: all,
    }
}

/// Everything still unresolved, oldest first. An entry is closed by any LATER
/// entry sharing its key and not itself open; position is identity, because the
/// file is append-only. Nothing else closes an open item — not age, not a
/// summariser, not a later run being cheerful about it.
fn open_of(all: &[Decision]) -> Vec<Decision> {
    let mut open = Vec::new();
    for (i, d) in all.iter().enumerate() {
        if !d.open {
            continue;
        }
        let closed = !d.key.trim().is_empty()
            && all.iter().skip(i + 1).any(|l| l.key == d.key && !l.open);
        if !closed {
            open.push(d.clone());
        }
    }
    open
}

/// Most recent decision per role, newest first, capped at five. Per-role rather
/// than simply the last five lines, so one chatty agent cannot push every other
/// agent's state off the top of the page.
fn headline_of(all: &[Decision]) -> Vec<Decision> {
    let mut h: Vec<Decision> = Vec::new();
    for d in all.iter().rev() {
        if h.len() >= 5 {
            break;
        }
        if h.iter().any(|x| x.role == d.role) {
            continue;
        }
        h.push(d.clone());
    }
    h
}

#[tauri::command]
pub fn decision_log_append(
    project: String,
    role: Option<String>,
    boundary: Option<String>,
    what: Option<String>,
    why: Option<String>,
    alternatives: Option<String>,
    key: Option<String>,
    open: Option<bool>,
) {
    append(
        &project,
        &Decision {
            ts: String::new(),
            role: role.unwrap_or_default(),
            boundary: boundary.unwrap_or_default(),
            what: what.unwrap_or_default(),
            why: why.unwrap_or_default(),
            alternatives: alternatives.unwrap_or_default(),
            key: key.unwrap_or_default(),
            open: open.unwrap_or(false),
        },
    );
}

#[tauri::command]
pub fn decision_log_brief(project: String) -> Brief {
    brief(&project)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(role: &str, why: &str, key: &str, open: bool) -> Decision {
        Decision {
            ts: String::new(),
            role: role.into(),
            boundary: "task.done".into(),
            what: String::new(),
            why: why.into(),
            alternatives: String::new(),
            key: key.into(),
            open,
        }
    }

    /// The one rule that makes the brief worth reading: an open item stays open
    /// until something explicitly closes it. If this ever passes by accident,
    /// the brief starts quietly resolving disagreements on the summariser's
    /// behalf, which is the exact failure it exists to prevent.
    #[test]
    fn open_items_survive_until_explicitly_closed() {
        let all = vec![
            d("Architect", "token bucket over fixed window", "rate-limit", true),
            d("Builder", "implemented it", "", false),
            d("Security", "disagrees on the burst size", "burst", true),
            d("Builder", "threshold loosened, agreed", "rate-limit", false),
        ];
        let open = open_of(&all);
        assert_eq!(open.len(), 1, "only the unclosed key stays open");
        assert_eq!(open[0].key, "burst");
    }

    /// A keyless open item can never be closed by anything, so it must persist.
    /// Losing it would be silent, which is the worst kind of wrong here.
    #[test]
    fn keyless_open_items_never_disappear() {
        let all = vec![d("Reviewer", "unsure this is safe", "", true)];
        assert_eq!(open_of(&all).len(), 1);
    }

    #[test]
    fn headline_gives_each_role_a_slot() {
        let all = vec![
            d("Builder", "first", "", false),
            d("Builder", "second", "", false),
            d("Builder", "third", "", false),
            d("Architect", "quiet but important", "", false),
        ];
        let h = headline_of(&all);
        assert_eq!(h.len(), 2, "one slot per role, not the last N lines");
        assert_eq!(h[0].role, "Architect", "newest first");
        assert_eq!(h[1].why, "third", "and the role's latest, not its first");
    }

    #[test]
    fn missing_rationale_is_counted_not_hidden() {
        let all = vec![d("hook", "", "", false), d("Builder", "because", "", false)];
        assert_eq!(all.iter().filter(|x| x.why.trim().is_empty()).count(), 1);
    }
}
