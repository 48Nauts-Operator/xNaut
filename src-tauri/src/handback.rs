// The structured handback: what an agent submits instead of an essay.
//
// Idea borrowed from AI Jason's control-graph patterns ("I don't prompt agents
// anymore", 2026-08-25), reviewed in the loop audit of 2026-08-30 as repair
// item 1: "the done-handback is prose today; a schema (what changed, how
// verified, record id, files touched) makes NautBot's review mechanical". The
// mechanisms here are xNAUT's own (PM tickets, sandbox verify records, the
// Mesh inbox) and we departed from the source in one way that matters: his
// schema is a prompt convention, ours is a typed record with a deterministic
// gate in front of it, so an agent cannot hand back an essay by ignoring the
// instruction.
//
// WHY A GATE AND NOT A JUDGE.
//
// The obvious move is to ask a model whether a handback is any good. That
// rebuilds the bottleneck one layer down: a judge's verdict is prose about
// prose, it costs a call, and it is wrong in ways nobody can reproduce. The
// checker here is a pure function over the record. It answers one question,
// mechanically: IS THIS REVIEWABLE AT ALL. Whether the work is any good stays
// a human's job; whether the report can be reviewed without opening the
// session is not a judgement call, and should never have been one.
//
// THE DESIGN CONSTRAINT THAT SHAPED THE SCHEMA.
//
// Every field is arranged so that an empty or hand-waved answer is VISIBLY
// empty rather than plausible prose. Three mechanisms do that work:
//
//   1. `not_finished` and `confidence` distinguish "never answered" from
//      "answered with nothing". `Option<String>` and `Confidence::Unstated`
//      are distinct values, not silent defaults, so a skipped field is a
//      blocking gap rather than an optimistic guess.
//
//   2. `how_verified` demands positive evidence: a command someone else can
//      re-run, or a verify record id. "tests pass" carries neither, so it
//      fails. This is a whitelist and not a blocklist of weasel words on
//      purpose; a blocklist only teaches agents new adjectives.
//
//   3. The escape hatch is LABELLED. Work that really was checked by hand
//      says `manual: ...` and passes, but earns a note on the verdict, so a
//      reviewer sees at a glance that nothing here can be re-run. An
//      unlabelled escape hatch is how "tests pass" got here in the first
//      place.
//
// The verdict carries a `fix` on every gap, because the point of refusing a
// handback is that the agent repairs and resubmits. A refusal an agent cannot
// act on is just a slower silence.

use serde::{Deserialize, Serialize};

/// How sure the agent is. An enum and not a number: a float invites `0.85`,
/// which is theatre, and nothing downstream can act on the second digit.
///
/// `Unstated` is the default so that a handback which never mentions
/// confidence is distinguishable from one that claims high confidence. That
/// distinction is the whole reason this is not a `bool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    #[default]
    Unstated,
    Low,
    Medium,
    High,
}

/// What an agent hands back when it finishes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Handback {
    /// The ticket this finishes, e.g. `XNAUT-264`.
    #[serde(default)]
    pub ticket: String,
    /// One line. Not the report; the subject line of the report.
    #[serde(default)]
    pub summary: String,
    /// Paths, not descriptions of paths.
    #[serde(default)]
    pub files_changed: Vec<String>,
    /// Commit shas, so the change can be located in history.
    #[serde(default)]
    pub commits: Vec<String>,
    /// What was RUN and what it said. Not an adjective.
    #[serde(default)]
    pub how_verified: String,
    /// The sandbox verify record, when one exists.
    #[serde(default)]
    pub verify_record_id: Option<String>,
    /// What is left and what it waits on. `None` means the agent never
    /// answered, which is a different thing from answering "nothing", and the
    /// checker treats it differently.
    #[serde(default)]
    pub not_finished: Option<String>,
    #[serde(default)]
    pub confidence: Confidence,
    /// The agent handle. Set from the session behind the token, never from
    /// the request body: a caller cannot name itself.
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub submitted_at: String,
}

/// One thing wrong with a handback, and what to write instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gap {
    /// The field at fault, so a UI can point at it.
    pub field: String,
    /// `blocking` or `note`. Any blocking gap makes the handback unreviewable.
    pub severity: String,
    /// What is wrong, quoting what was actually written where that helps.
    pub problem: String,
    /// What to put there instead. A refusal without this is a slower silence.
    pub fix: String,
}

impl Gap {
    fn blocking(field: &str, problem: impl Into<String>, fix: impl Into<String>) -> Self {
        Gap {
            field: field.to_string(),
            severity: "blocking".to_string(),
            problem: problem.into(),
            fix: fix.into(),
        }
    }

    fn note(field: &str, problem: impl Into<String>, fix: impl Into<String>) -> Self {
        Gap {
            field: field.to_string(),
            severity: "note".to_string(),
            problem: problem.into(),
            fix: fix.into(),
        }
    }
}

/// The answer to "can this be reviewed without opening the session".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    /// `reviewable` or `not_reviewable`.
    pub lane: String,
    pub gaps: Vec<Gap>,
}

impl Verdict {
    pub fn is_reviewable(&self) -> bool {
        self.lane == "reviewable"
    }

    /// The blocking gaps only, for a caller that wants to say why it refused.
    pub fn blocking(&self) -> impl Iterator<Item = &Gap> {
        self.gaps.iter().filter(|gap| gap.severity == "blocking")
    }
}

/// Runners whose presence means someone can re-run the check.
///
/// A whitelist, and deliberately tight. Two candidates were cut after they
/// accepted prose: `git ` matches "I checked git history", and a bare `sh `
/// matches "a fresh install". Both would have passed a report with no command
/// in it at all, which is the exact failure this list exists to catch.
const RUNNERS: &[&str] = &[
    "cargo ",
    "npm ",
    "pnpm ",
    "yarn ",
    "bun ",
    "pytest",
    "go test",
    "just ",
    "make ",
    "./",
    "python ",
    "node ",
    "xcodebuild",
    "swift test",
    "docker ",
    "deno ",
    "rspec",
    "phpunit",
    "gradle",
    "mvn ",
    "dotnet ",
    "jest",
    "vitest",
    "playwright",
    "curl ",
];

/// The labelled escape hatch for work that genuinely has no command.
const MANUAL_PREFIX: &str = "manual:";

/// An answer to `not_finished` meaning "nothing is outstanding". Sanctioned so
/// that a genuinely complete run has a word to say, rather than being pushed
/// into inventing an outstanding item to satisfy the gate.
const NOTHING_OUTSTANDING: &[&str] = &["nothing", "none", "nothing outstanding", "nothing left"];

/// A ticket id, `KEY-123`. Written by hand rather than with a regex because
/// this is the only pattern in the module and a dependency for one shape is a
/// poor trade.
fn is_ticket_id(value: &str) -> bool {
    let Some((key, number)) = value.split_once('-') else {
        return false;
    };
    !key.is_empty()
        && key.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && key.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// 8-4-4-4-12 hex. Verify record ids are v4 uuids.
fn is_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && [8usize, 4, 4, 4, 12]
            .iter()
            .zip(&groups)
            .all(|(want, got)| got.len() == *want && got.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Does this text contain a uuid anywhere in it?
fn mentions_uuid(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
        .any(is_uuid)
}

/// A path, as opposed to a description of one.
///
/// The test is whitespace, not slashes or extensions. "several backend files"
/// is prose and has spaces; `justfile`, `Makefile` and `Dockerfile` are real
/// paths with neither a slash nor a dot, and an extension test would have
/// rejected all three.
fn is_path_shaped(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_whitespace)
}

/// A commit sha: 7 to 40 hex characters.
fn is_sha_shaped(value: &str) -> bool {
    let value = value.trim();
    (7..=40).contains(&value.len()) && value.chars().all(|c| c.is_ascii_hexdigit())
}

/// ~80 characters of what was actually written, so a refusal quotes its
/// evidence rather than asserting it. Char-indexed: handbacks carry é and →,
/// and slicing a `&str` mid-codepoint panics.
fn quoted(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = flat.chars().collect();
    if chars.len() <= 80 {
        format!("\"{flat}\"")
    } else {
        format!("\"{}…\"", chars[..80].iter().collect::<String>())
    }
}

/// Is `not_finished` the sanctioned "nothing"?
fn says_nothing_outstanding(text: &str) -> bool {
    let cleaned = text
        .trim()
        .trim_end_matches(['.', '!', ',', ';'])
        .trim()
        .to_ascii_lowercase();
    NOTHING_OUTSTANDING.contains(&cleaned.as_str())
}

/// Can this handback be reviewed without opening the session?
///
/// Pure. No clock, no filesystem, no network, no model. Everything it decides
/// is a property of the record in front of it, which is what makes the answer
/// reproducible and the same in a test as in production.
pub fn review(handback: &Handback) -> Verdict {
    let mut gaps = Vec::new();

    // ── ticket ──────────────────────────────────────────────────────────
    let ticket = handback.ticket.trim();
    if ticket.is_empty() {
        gaps.push(Gap::blocking(
            "ticket",
            "no ticket named, so this handback belongs to nothing",
            "set ticket to the id you were working, e.g. XNAUT-264",
        ));
    } else if !is_ticket_id(ticket) {
        gaps.push(Gap::blocking(
            "ticket",
            format!("{} is not a ticket id", quoted(ticket)),
            "use the KEY-NUMBER form, e.g. XNAUT-264",
        ));
    }

    // ── summary ─────────────────────────────────────────────────────────
    let summary = handback.summary.trim();
    if summary.is_empty() {
        gaps.push(Gap::blocking(
            "summary",
            "no summary, so the board has nothing to show",
            "one line saying what this ticket now does that it did not before",
        ));
    } else if summary.contains('\n') {
        gaps.push(Gap::note(
            "summary",
            "the summary runs to several lines",
            "keep summary to one line; the detail belongs in the ticket body",
        ));
    }

    // ── files_changed ───────────────────────────────────────────────────
    if handback.files_changed.iter().all(|f| f.trim().is_empty()) {
        gaps.push(Gap::blocking(
            "files_changed",
            "no files named, so nobody can review the change without opening the session",
            "list every path you touched, as paths; run git diff --name-only if you lost track",
        ));
    } else {
        for entry in &handback.files_changed {
            if entry.trim().is_empty() {
                continue;
            }
            if !is_path_shaped(entry) {
                gaps.push(Gap::blocking(
                    "files_changed",
                    format!("{} describes files instead of naming them", quoted(entry)),
                    "replace it with the actual paths, one entry each",
                ));
            }
        }
    }

    // ── commits ─────────────────────────────────────────────────────────
    let commits: Vec<&String> = handback
        .commits
        .iter()
        .filter(|c| !c.trim().is_empty())
        .collect();
    if commits.is_empty() {
        gaps.push(Gap::note(
            "commits",
            "no commit named, so the change cannot be located in history",
            "add the sha you committed; leave it empty only if nothing was committed",
        ));
    } else {
        for commit in commits {
            if !is_sha_shaped(commit) {
                gaps.push(Gap::note(
                    "commits",
                    format!("{} is not a commit sha", quoted(commit)),
                    "use the sha from git rev-parse HEAD, not the message",
                ));
            }
        }
    }

    // ── how_verified ────────────────────────────────────────────────────
    //
    // The field this whole module exists for. Positive evidence required:
    // something another person can re-run, or a record they can open.
    let verified = handback.how_verified.trim();
    let record_named = handback
        .verify_record_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|id| !id.is_empty());
    if verified.is_empty() {
        gaps.push(Gap::blocking(
            "how_verified",
            "nothing says how this was checked",
            "name the command you ran and what it printed, or a verify record id; \
             if you checked by hand, say so with a manual: prefix",
        ));
    } else {
        let lower = verified.to_ascii_lowercase();
        let has_runner = RUNNERS.iter().any(|runner| lower.contains(runner));
        let has_record = record_named || mentions_uuid(verified);
        let is_manual = lower.trim_start().starts_with(MANUAL_PREFIX);
        if is_manual {
            gaps.push(Gap::note(
                "how_verified",
                "verified by hand, so there is no command anyone can re-run",
                "no action needed; the reviewer is being told to check this one themselves",
            ));
        } else if !has_runner && !has_record {
            gaps.push(Gap::blocking(
                "how_verified",
                format!("{} names no command and no record, so it cannot be re-run", quoted(verified)),
                "write the command and its result, e.g. \"cargo test --bin xnaut: 809 passed, 0 failed\"; \
                 if you checked by hand, prefix it with manual:",
            ));
        }
    }

    // ── verify_record_id ────────────────────────────────────────────────
    //
    // Blocking rather than a note when it is malformed: a bogus record id is
    // worse than none, because it reads as evidence and a reviewer trusting it
    // never opens the record that would have told them.
    if let Some(id) = handback.verify_record_id.as_deref().map(str::trim) {
        if !id.is_empty() && !is_uuid(id) {
            gaps.push(Gap::blocking(
                "verify_record_id",
                format!("{} is not a verify record id", quoted(id)),
                "use the uuid from the verify record, or leave the field out entirely",
            ));
        }
    }

    // ── not_finished ────────────────────────────────────────────────────
    let outstanding = match handback.not_finished.as_deref() {
        None => {
            gaps.push(Gap::blocking(
                "not_finished",
                "the field was never answered, so nobody knows whether anything is left",
                "say what is outstanding and what it waits on; write \"nothing\" if the work is whole",
            ));
            None
        }
        Some(text) if text.trim().is_empty() => {
            gaps.push(Gap::blocking(
                "not_finished",
                "the field is blank, which reads as either \"nothing left\" or \"I did not check\"",
                "write \"nothing\" if the work is whole, otherwise say what is left and what it waits on",
            ));
            None
        }
        Some(text) => Some(text),
    };
    let nothing_left = outstanding.is_some_and(|text| says_nothing_outstanding(text));

    // ── confidence ──────────────────────────────────────────────────────
    match handback.confidence {
        Confidence::Unstated => gaps.push(Gap::blocking(
            "confidence",
            "confidence was never stated, and an unstated confidence reads as high",
            "set confidence to high, medium or low",
        )),
        // A contradiction, and the reviewer needs it loud. Low confidence
        // means the agent believes something may be wrong; "nothing
        // outstanding" means it believes nothing is. Both cannot hold, and
        // the pair is exactly what a mechanical review should catch.
        Confidence::Low if nothing_left => gaps.push(Gap::blocking(
            "confidence",
            "confidence is low but nothing is listed as outstanding",
            "either say what you are unsure about in not_finished, or raise the confidence",
        )),
        Confidence::Medium if nothing_left => gaps.push(Gap::note(
            "confidence",
            "confidence is medium with nothing outstanding",
            "if something is unresolved, naming it in not_finished saves the reviewer finding it",
        )),
        _ => {}
    }

    let blocked = gaps.iter().any(|gap| gap.severity == "blocking");
    Verdict {
        lane: if blocked { "not_reviewable" } else { "reviewable" }.to_string(),
        gaps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handback that passes. Every test below starts here and breaks ONE
    /// thing, so a failure names the rule that broke rather than the fixture.
    fn good() -> Handback {
        Handback {
            ticket: "XNAUT-264".into(),
            summary: "orphaned verify runs are reaped at boot".into(),
            files_changed: vec![
                "src-tauri/src/sandbox_verify.rs".into(),
                "src-tauri/src/main.rs".into(),
            ],
            commits: vec!["832e5aecafe1".into()],
            how_verified: "cargo test --bin xnaut: 809 passed, 0 failed".into(),
            verify_record_id: None,
            not_finished: Some("nothing".into()),
            confidence: Confidence::High,
            from: "claude".into(),
            submitted_at: "2026-09-05T10:00:00Z".into(),
        }
    }

    fn fields_blocked(handback: &Handback) -> Vec<String> {
        review(handback)
            .blocking()
            .map(|gap| gap.field.clone())
            .collect()
    }

    #[test]
    fn a_complete_handback_is_reviewable() {
        let verdict = review(&good());
        assert!(
            verdict.is_reviewable(),
            "a complete handback was refused: {:?}",
            verdict.gaps
        );
        assert!(verdict.gaps.is_empty(), "unexpected gaps: {:?}", verdict.gaps);
    }

    // ── how_verified: the field the module exists for ───────────────────

    #[test]
    fn tests_pass_is_not_a_verification() {
        // The exact phrase from the loop audit. If this ever goes green, the
        // gate has stopped doing the one job it was built for.
        for adjective in [
            "tests pass",
            "all tests pass",
            "verified",
            "works",
            "looks good",
            "all green",
            "LGTM",
            "I checked it and it is fine",
            "the build is clean",
        ] {
            let mut handback = good();
            handback.how_verified = adjective.into();
            assert_eq!(
                fields_blocked(&handback),
                vec!["how_verified"],
                "{adjective:?} was accepted as a verification"
            );
        }
    }

    #[test]
    fn a_command_and_its_result_is_a_verification() {
        for command in [
            "cargo test --bin xnaut: 809 passed, 0 failed",
            "npm run test -- --run, 42 passing",
            "pytest -q, 17 passed",
            "./scripts/mutation-check.cjs, exit 0",
            "just check, no warnings",
            "xcodebuild -scheme xNAUT build, BUILD SUCCEEDED",
            "go test ./..., ok",
            "playwright test, 6 passed",
        ] {
            let mut handback = good();
            handback.how_verified = command.into();
            assert!(
                review(&handback).is_reviewable(),
                "{command:?} was refused as a verification"
            );
        }
    }

    #[test]
    fn a_verify_record_stands_in_for_a_command() {
        // The sandbox verify ran it on a real machine; quoting the record is
        // stronger evidence than quoting a command, not weaker.
        let mut handback = good();
        handback.how_verified = "the sandbox verify plan for this ticket".into();
        handback.verify_record_id = Some("9f1c2d3e-4a5b-4c6d-8e9f-0a1b2c3d4e5f".into());
        assert!(
            review(&handback).is_reviewable(),
            "a named verify record was refused: {:?}",
            review(&handback).gaps
        );

        // And the id inline in the prose counts too, since that is how an
        // agent that never read the schema closely will write it.
        let mut inline = good();
        inline.how_verified =
            "sandbox verify record 9f1c2d3e-4a5b-4c6d-8e9f-0a1b2c3d4e5f passed".into();
        inline.verify_record_id = None;
        assert!(review(&inline).is_reviewable());
    }

    #[test]
    fn a_hand_check_passes_but_is_labelled() {
        let mut handback = good();
        handback.how_verified =
            "manual: opened the Agents pane, clicked Wake, the roster repainted".into();
        let verdict = review(&handback);
        assert!(verdict.is_reviewable(), "a labelled hand check was refused");
        // The whole point of allowing it: the reviewer is TOLD there is no
        // command. An unlabelled escape hatch is how "tests pass" got here.
        let note = verdict
            .gaps
            .iter()
            .find(|gap| gap.field == "how_verified")
            .expect("the hand check earned no note");
        assert_eq!(note.severity, "note");
        assert!(note.problem.contains("by hand"), "{}", note.problem);
    }

    #[test]
    fn an_empty_verification_is_refused() {
        let mut handback = good();
        handback.how_verified = "   ".into();
        assert_eq!(fields_blocked(&handback), vec!["how_verified"]);
    }

    #[test]
    fn prose_mentioning_git_or_a_shell_is_still_prose() {
        // Both stems were in the first version of RUNNERS and both accepted
        // reports with no command in them. Kept as a test so neither comes
        // back.
        for prose in ["I checked git history and it looks right", "a fresh install works"] {
            let mut handback = good();
            handback.how_verified = prose.into();
            assert_eq!(
                fields_blocked(&handback),
                vec!["how_verified"],
                "{prose:?} was accepted"
            );
        }
    }

    // ── files_changed ───────────────────────────────────────────────────

    #[test]
    fn a_handback_that_names_no_files_is_refused() {
        let mut handback = good();
        handback.files_changed = vec![];
        assert_eq!(fields_blocked(&handback), vec!["files_changed"]);

        let mut blanks = good();
        blanks.files_changed = vec!["".into(), "   ".into()];
        assert_eq!(fields_blocked(&blanks), vec!["files_changed"]);
    }

    #[test]
    fn describing_files_is_not_naming_them() {
        let mut handback = good();
        handback.files_changed = vec!["several files in the backend".into()];
        let verdict = review(&handback);
        assert!(!verdict.is_reviewable());
        let gap = verdict
            .gaps
            .iter()
            .find(|gap| gap.field == "files_changed")
            .expect("no gap on files_changed");
        // The refusal quotes its evidence rather than asserting it.
        assert!(
            gap.problem.contains("several files in the backend"),
            "the gap does not quote what was written: {}",
            gap.problem
        );
    }

    #[test]
    fn dotless_filenames_are_paths() {
        // An extension test would have rejected all three, and all three are
        // real files in this repo's world.
        let mut handback = good();
        handback.files_changed = vec!["justfile".into(), "Makefile".into(), "Dockerfile".into()];
        assert!(
            review(&handback).is_reviewable(),
            "a dotless filename was called prose: {:?}",
            review(&handback).gaps
        );
    }

    // ── not_finished: never answered is not the same as nothing left ────

    #[test]
    fn an_unanswered_not_finished_is_refused() {
        let mut handback = good();
        handback.not_finished = None;
        let verdict = review(&handback);
        assert_eq!(fields_blocked(&handback), vec!["not_finished"]);
        assert!(
            verdict.gaps[0].problem.contains("never answered"),
            "the gap does not distinguish unanswered from empty: {}",
            verdict.gaps[0].problem
        );
    }

    #[test]
    fn a_blank_not_finished_is_refused_differently() {
        let mut handback = good();
        handback.not_finished = Some("  ".into());
        let verdict = review(&handback);
        assert_eq!(fields_blocked(&handback), vec!["not_finished"]);
        assert!(
            verdict.gaps[0].problem.contains("blank"),
            "blank and unanswered give the same gap: {}",
            verdict.gaps[0].problem
        );
    }

    #[test]
    fn nothing_outstanding_is_a_valid_answer() {
        for word in ["nothing", "Nothing.", "none", "NOTHING LEFT"] {
            let mut handback = good();
            handback.not_finished = Some(word.into());
            assert!(
                review(&handback).is_reviewable(),
                "{word:?} was refused as an answer"
            );
        }
    }

    #[test]
    fn real_outstanding_work_is_a_valid_answer() {
        let mut handback = good();
        handback.not_finished =
            Some("the Windows leg is untested; waits on a runner with a signing cert".into());
        assert!(review(&handback).is_reviewable());
    }

    // ── confidence ──────────────────────────────────────────────────────

    #[test]
    fn an_unstated_confidence_is_refused() {
        let mut handback = good();
        handback.confidence = Confidence::Unstated;
        assert_eq!(fields_blocked(&handback), vec!["confidence"]);
    }

    #[test]
    fn low_confidence_with_nothing_outstanding_is_a_contradiction() {
        let mut handback = good();
        handback.confidence = Confidence::Low;
        handback.not_finished = Some("nothing".into());
        assert_eq!(fields_blocked(&handback), vec!["confidence"]);
    }

    #[test]
    fn low_confidence_that_says_what_is_wrong_is_fine() {
        let mut handback = good();
        handback.confidence = Confidence::Low;
        handback.not_finished =
            Some("the reap may race a live run; I could not reproduce the race".into());
        assert!(
            review(&handback).is_reviewable(),
            "an honest low-confidence handback was refused: {:?}",
            review(&handback).gaps
        );
    }

    #[test]
    fn medium_confidence_with_nothing_outstanding_is_only_a_note() {
        let mut handback = good();
        handback.confidence = Confidence::Medium;
        let verdict = review(&handback);
        assert!(verdict.is_reviewable());
        assert_eq!(verdict.gaps.len(), 1);
        assert_eq!(verdict.gaps[0].severity, "note");
    }

    // ── ticket and summary ──────────────────────────────────────────────

    #[test]
    fn a_handback_without_a_ticket_belongs_to_nothing() {
        let mut handback = good();
        handback.ticket = "".into();
        assert_eq!(fields_blocked(&handback), vec!["ticket"]);
    }

    #[test]
    fn a_ticket_that_is_not_an_id_is_refused() {
        for bogus in ["the orphan reap one", "xnaut-264", "XNAUT", "XNAUT-", "-264", "XNAUT-abc"] {
            let mut handback = good();
            handback.ticket = bogus.into();
            assert_eq!(
                fields_blocked(&handback),
                vec!["ticket"],
                "{bogus:?} was accepted as a ticket id"
            );
        }
    }

    #[test]
    fn real_ticket_ids_are_accepted() {
        for id in ["XNAUT-264", "ENGRAMOSS-9", "A1-1"] {
            let mut handback = good();
            handback.ticket = id.into();
            assert!(review(&handback).is_reviewable(), "{id:?} was refused");
        }
    }

    #[test]
    fn a_handback_without_a_summary_is_refused() {
        let mut handback = good();
        handback.summary = "  ".into();
        assert_eq!(fields_blocked(&handback), vec!["summary"]);
    }

    #[test]
    fn a_multi_line_summary_is_only_a_note() {
        let mut handback = good();
        handback.summary = "reaped orphans\nand also rewrote the sweep".into();
        let verdict = review(&handback);
        assert!(verdict.is_reviewable());
        assert_eq!(verdict.gaps[0].severity, "note");
    }

    // ── commits and verify_record_id ────────────────────────────────────

    #[test]
    fn no_commit_is_a_note_not_a_refusal() {
        // Research and docs tickets finish without one. Refusing them would
        // teach agents to paste a sha that is not theirs.
        let mut handback = good();
        handback.commits = vec![];
        let verdict = review(&handback);
        assert!(verdict.is_reviewable());
        assert_eq!(verdict.gaps.len(), 1);
        assert_eq!(verdict.gaps[0].field, "commits");
        assert_eq!(verdict.gaps[0].severity, "note");
    }

    #[test]
    fn a_commit_message_is_not_a_sha() {
        let mut handback = good();
        handback.commits = vec!["fix(vault): skip dependency dirs".into()];
        let verdict = review(&handback);
        assert!(verdict.is_reviewable(), "a bad sha should not block");
        assert_eq!(verdict.gaps[0].field, "commits");
        assert_eq!(verdict.gaps[0].severity, "note");
    }

    #[test]
    fn a_bogus_verify_record_id_blocks() {
        // Worse than none: it reads as evidence, so a reviewer who trusts it
        // never opens the record that would have told them.
        let mut handback = good();
        handback.verify_record_id = Some("passed".into());
        assert_eq!(fields_blocked(&handback), vec!["verify_record_id"]);
    }

    #[test]
    fn an_absent_verify_record_id_is_fine() {
        let mut handback = good();
        handback.verify_record_id = None;
        assert!(review(&handback).is_reviewable());
        handback.verify_record_id = Some("".into());
        assert!(review(&handback).is_reviewable());
    }

    // ── the shape of the verdict itself ─────────────────────────────────

    #[test]
    fn every_gap_says_what_to_write_instead() {
        // A refusal an agent cannot act on is a slower silence. Build one
        // handback that trips every rule at once and check all of them.
        let empty = Handback::default();
        let verdict = review(&empty);
        assert!(!verdict.is_reviewable());
        assert!(verdict.gaps.len() >= 5, "{:?}", verdict.gaps);
        for gap in &verdict.gaps {
            assert!(!gap.field.trim().is_empty(), "a gap names no field");
            assert!(!gap.problem.trim().is_empty(), "{} has no problem", gap.field);
            assert!(!gap.fix.trim().is_empty(), "{} tells nobody what to do", gap.field);
            assert!(
                matches!(gap.severity.as_str(), "blocking" | "note"),
                "{} has severity {}",
                gap.field,
                gap.severity
            );
        }
    }

    #[test]
    fn an_empty_handback_is_refused_on_every_required_field() {
        let blocked = fields_blocked(&Handback::default());
        for field in [
            "ticket",
            "summary",
            "files_changed",
            "how_verified",
            "not_finished",
            "confidence",
        ] {
            assert!(
                blocked.iter().any(|f| f == field),
                "an empty handback passed {field}: {blocked:?}"
            );
        }
    }

    #[test]
    fn the_checker_is_pure() {
        // Same input, same answer, and no mutation of the record. If this
        // ever fails, something reached for a clock or the filesystem, and
        // the verdict stopped being reproducible in a review.
        let handback = good();
        let before = handback.clone();
        let first = review(&handback);
        let second = review(&handback);
        assert_eq!(first, second);
        assert_eq!(handback, before);
    }

    #[test]
    fn the_verdict_survives_the_json_round_trip() {
        // It is stored on a ticket and read back after a restart, so the
        // record has to come back the same.
        let handback = good();
        let text = serde_json::to_string(&handback).expect("serialize");
        let back: Handback = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(handback, back);

        let verdict = review(&Handback::default());
        let text = serde_json::to_string(&verdict).expect("serialize");
        let back: Verdict = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(verdict, back);
    }

    #[test]
    fn a_missing_confidence_deserializes_as_unstated_not_high() {
        // The reason confidence is a four-valued enum. If a missing field
        // ever defaults to high, every agent that ignores the field starts
        // claiming certainty it never expressed.
        let handback: Handback = serde_json::from_str(r#"{"ticket":"XNAUT-1"}"#).expect("parse");
        assert_eq!(handback.confidence, Confidence::Unstated);
        assert_eq!(handback.not_finished, None);
    }

    #[test]
    fn non_ascii_text_does_not_panic_the_quoter() {
        // review_gate learned this one the hard way: byte-slicing a body that
        // carries é or → panics mid-codepoint.
        //
        // The offset is ARRANGED, not hoped for. `quoted` truncates at 80, so
        // the string puts 78 ASCII bytes in front of a 3-byte '→', which then
        // occupies bytes 78, 79 and 80: byte offset 80 lands INSIDE the
        // character, and a byte slice there panics. The first version of this
        // test used a natural sentence and its byte 80 happened to fall on a
        // boundary, so it exercised the path and caught nothing.
        let straddling = format!("{}\u{2192} and some more prose after it", "a".repeat(78));
        assert!(
            !straddling.is_char_boundary(80),
            "the fixture no longer straddles byte 80, so it proves nothing"
        );
        assert!(straddling.chars().count() > 80, "the fixture is too short to truncate");

        let mut handback = good();
        handback.files_changed = vec![straddling];
        let verdict = review(&handback);
        assert!(!verdict.is_reviewable());
        assert!(verdict.gaps.iter().any(|gap| gap.field == "files_changed"));
    }
}
