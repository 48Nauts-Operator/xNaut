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

/// A model-written paragraph laid OVER the mechanical brief, never instead of
/// it. The summariser compresses prose. It does not get to say what is open,
/// what is resolved, or what was important — those are computed in code above,
/// and both halves are rendered together so the reader can see the difference.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Summary {
    pub text: String,
    /// RFC3339, when it was written.
    pub at: String,
    /// How many decisions it was written from. The log is append-only, so this
    /// is also its version: `n < detail.len()` means the prose is behind.
    pub n: usize,
    /// Why there is no text, when there is none. Empty on success. Surfaced
    /// rather than swallowed: a summary that is silently absent looks like a
    /// run with nothing to say.
    #[serde(default)]
    pub error: String,
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
    /// The last prose summary written for this project, if any. Read from the
    /// log, so the poll that refreshes this view costs nothing extra and never
    /// triggers a model call by itself.
    pub summary: Option<Summary>,
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
        summary: last_summary(project),
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

// ─── The summariser ──────────────────────────────────────────────────────────
//
// The layer the module header said could exist: prose over the mechanical
// brief. What it is allowed to do is narrow on purpose.
//
// The failure this must not reproduce is the one that justified the whole
// module: an agent asked at the end to explain a run reconstructs it fluently
// and smooths over the argument that mattered. The summariser is asked the same
// question, so it would fail the same way if it were given the same job. It is
// not. It never sees the run; it sees the log, which was written at the
// boundaries by the agents standing at them. It rewrites those lines into
// something readable and nothing else. Selection, grouping and what counts as
// open are computed above in code, and rendered alongside this prose rather
// than replaced by it, so a summary that goes wrong is visibly wrong next to
// the list it failed to describe.
//
// jcode (MIT, github.com/1jehuang/jcode, benchmarks/confidence.html) measured
// the same effect from the other side: across 70 trials, an agent's confidence
// at completion rose an average of 16 points whether the step passed or failed.
// End-of-run self-report carries no signal. We depart from their response to
// it — they gate on the confidence delta, we keep the model out of the
// judgement entirely and let it write only the prose.

const SUMMARY_SYSTEM: &str = "\
You are writing a short brief over a decision log from a run of AI agents on a \
software project. Each entry is a fork an agent stood at and the reason it went \
the way it did, written at the moment it happened.

Your reader stepped away and wants to know where things stand.

Rules:
- Lead with what the run is actually doing and why. Do not narrate the log \
entry by entry.
- State disagreements and unresolved questions plainly. Do not resolve them, \
do not conclude them, do not imply they are settled or minor.
- SETTLED entries are finished. Write them in the past tense. A settled \
entry's reason describes a problem that was already dealt with, so never \
restate it as something currently happening or currently broken. Only an \
UNRESOLVED entry describes a live problem.
- Never invent a reason for an entry that has none. If reasons are missing, \
say how many and move on.
- No advice, no next steps, no encouragement, no praise for the agents.
- Plain sentences, no headings, no bullets, no markdown. Under 150 words.
- Never use an em-dash. Use a full stop or a semicolon.";

/// Render the brief as the summariser's input. Deliberately flat text: the model
/// gets the same lines a human would read, with the open ones marked, so it
/// cannot mistake structure for permission to restructure.
fn summary_input(b: &Brief) -> String {
    let mut s = String::new();
    for d in &b.detail {
        let role = if d.role.is_empty() { "agent" } else { &d.role };
        s.push_str(&format!("[{}] {}", role, d.boundary));
        // Both states are named. Marking only the open ones leaves "settled" as
        // the absence of a word, and the model reads a closed entry's reason as
        // a live problem: it wrote "the backend currently returns cached prose"
        // about a bug that decision had already fixed.
        s.push_str(if d.open { " (UNRESOLVED)" } else { " (SETTLED)" });
        if !d.what.trim().is_empty() {
            s.push_str(&format!("\n  did: {}", d.what.trim()));
        }
        if d.why.trim().is_empty() {
            s.push_str("\n  reason: none recorded");
        } else {
            s.push_str(&format!("\n  reason: {}", d.why.trim()));
        }
        if !d.alternatives.trim().is_empty() {
            s.push_str(&format!("\n  rejected: {}", d.alternatives.trim()));
        }
        s.push('\n');
    }
    s.push_str(&format!(
        "\n{} of {} entries have no recorded reason. {} are still unresolved.",
        b.unexplained,
        b.detail.len(),
        b.open.len()
    ));
    s
}

/// The last summary written for this project, or none.
///
/// Stored as an event in the same per-project log rather than a file of its
/// own: `read()` filters on `kind == "decision"`, so a summary cannot leak into
/// the brief it describes, and it inherits the log's durability for free.
pub fn last_summary(project: &str) -> Option<Summary> {
    crate::build_log::read_events(&log_id(project))
        .into_iter()
        .filter(|e| e.kind == "summary")
        .filter_map(|e| serde_json::from_value::<Summary>(e.data).ok())
        .last()
}

/// Whether the prose is far enough behind the log to be worth rewriting.
///
/// ponytail: a count, not a timer. The log is append-only and boundaries are
/// rare by construction, so "three new decisions" is both the staleness signal
/// and the rate limit. One new entry is usually not a different story; a timer
/// would re-run the model on a project nobody is working on.
fn is_stale(b: &Brief) -> bool {
    if b.detail.is_empty() {
        return false;
    }
    match &b.summary {
        None => true,
        Some(s) => b.detail.len().saturating_sub(s.n) >= 3,
    }
}

/// Write a fresh summary. Returns the existing one untouched when it is current
/// and `force` is false.
///
/// Failure is recorded, not raised: an unreachable model must degrade to "no
/// prose, mechanical brief unchanged", the same rule engram.rs follows on
/// recall. A decision log that stops rendering because a summariser is down
/// would be a worse product than one with no summariser at all.
pub async fn summarize(
    llm: &crate::settings::LlmSettings,
    project: &str,
    force: bool,
) -> Summary {
    let b = brief(project);
    if !force && !is_stale(&b) {
        return b.summary.unwrap_or_default();
    }
    if b.detail.is_empty() {
        return Summary::default();
    }
    let n = b.detail.len();
    let out = match crate::chat::complete_oneshot(llm, Some(SUMMARY_SYSTEM), &summary_input(&b))
        .await
    {
        Ok(t) => Summary {
            text: t.trim().to_string(),
            at: chrono::Utc::now().to_rfc3339(),
            n,
            error: String::new(),
        },
        Err(e) => Summary {
            text: String::new(),
            at: chrono::Utc::now().to_rfc3339(),
            n,
            error: e,
        },
    };
    crate::build_log::build_log_append(
        log_id(project),
        if out.error.is_empty() { "info" } else { "warn" }.into(),
        "summariser".into(),
        if out.error.is_empty() {
            out.text.clone()
        } else {
            format!("summary failed: {}", out.error)
        },
        Some("summary".into()),
        serde_json::to_value(&out).ok(),
    );
    out
}

#[tauri::command]
pub async fn decision_log_summarize(
    state: tauri::State<'_, crate::state::AppState>,
    project: String,
    force: Option<bool>,
) -> Result<Summary, String> {
    let llm = state.settings.lock().await.llm.clone();
    Ok(summarize(&llm, &project, force.unwrap_or(false)).await)
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

    fn brief_of(all: Vec<Decision>, summary: Option<Summary>) -> Brief {
        Brief {
            unexplained: all.iter().filter(|x| x.why.trim().is_empty()).count(),
            headline: headline_of(&all),
            open: open_of(&all),
            summary,
            detail: all,
        }
    }

    /// The summariser must never be handed an entry it could mistake for having
    /// a rationale, and an unresolved item must be unmistakable in its input.
    /// If either slips, the model writes a tidy paragraph over a live argument,
    /// which is the exact failure this module exists to prevent.
    #[test]
    fn summary_input_marks_unresolved_and_admits_missing_reasons() {
        let b = brief_of(
            vec![
                d("hook", "", "s1", false),
                d("Security", "burst size still disputed", "burst", true),
            ],
            None,
        );
        let input = summary_input(&b);
        assert!(input.contains("(UNRESOLVED)"), "open items must be marked");
        assert!(
            input.contains("(SETTLED)"),
            "closed items must be marked too, or the model reads their reason as a live problem"
        );
        assert!(
            input.contains("reason: none recorded"),
            "a missing reason must be stated, never omitted"
        );
        assert!(
            input.contains("1 of 2 entries have no recorded reason"),
            "counts must reach the model"
        );
    }

    /// Staleness is what keeps a 5s poll from being a 5s model call.
    #[test]
    fn summary_reruns_only_when_the_log_moved_on() {
        let one = vec![d("Builder", "a", "", false)];
        assert!(is_stale(&brief_of(one.clone(), None)), "no prose yet");
        assert!(!is_stale(&brief_of(vec![], None)), "empty log stays empty");

        let cur = Some(Summary { n: 1, ..Default::default() });
        assert!(!is_stale(&brief_of(one, cur.clone())), "current prose is left alone");

        let grown = vec![
            d("Builder", "a", "", false),
            d("Builder", "b", "", false),
            d("Builder", "c", "", false),
            d("Builder", "e", "", false),
        ];
        assert!(is_stale(&brief_of(grown, cur)), "three entries on is a new story");
    }

    #[test]
    fn missing_rationale_is_counted_not_hidden() {
        let all = vec![d("hook", "", "", false), d("Builder", "because", "", false)];
        assert_eq!(all.iter().filter(|x| x.why.trim().is_empty()).count(), 1);
    }
}
