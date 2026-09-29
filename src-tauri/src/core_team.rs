// The core team (XNAUT-357): the standing method, run as a loop.
//
// xNAUT is built by reading other people's solutions to the same problems,
// reimplementing rather than copying, and naming the source in the file header.
// CLAUDE.md writes that down as a rule an agent follows WHEN IT HAPPENS to find
// something, which means the finding step has never had a clock on it: three of
// the build stage's four upgrades on 2026-08-08 came from other projects, and
// every one of them was found by accident.
//
// This module is the same method with a clock. André, 2026-09-13: "a Researcher
// Agent scans github for new AI Agent tooling ... a Reviewer Agent weights the
// findings, anything with a score of X is tagged PoC ... a PoC Agent builds it
// in a worktree via sandbox and makes a proposal ... a Judge reviews pro and con
// and decides if it goes in or not."
//
// Four members, and each one is an agent that already exists rather than a new
// kind of thing:
//
//   * the Researcher is @researcher (XNAUT-356), the one profile that looks
//     outside the room and cites what it read;
//   * the Reviewer is @reviewer, seeded on a provider NautBot does not use,
//     because a second opinion out of the same weights is not a second opinion;
//   * the PoC agent is an ordinary dispatch: a worktree, a branch, a run in the
//     registry, the same budget and the same ceiling as every other run;
//   * the Judge is the jury — two blind reviewers on different runtimes, no
//     timeout default — asked a pro/con question instead of a merge question.
//
// FOUR DECISIONS worth reading before changing anything here.
//
// 1. `poc` is a TAG, not a status. Andre's sentence says "tagged PoC", and a
//    new global ticket status would have grown a column on every project's
//    board to hold findings that only exist on one.
//
// 2. `finding` IS a new ticket type. A finding is not a feature someone decided
//    to build; it is a candidate nobody has agreed to yet, and a board that
//    cannot tell those apart at a glance turns the backlog into a wish list.
//
// 3. The licence is a GATE, not a scoring dimension. A dimension can be
//    outvoted by four good ones, and "we scored 88 so we ported from a licence
//    we cannot name" is the one outcome this loop must never produce. An
//    unknown licence is refused exactly as hard as a copyleft one: crediting a
//    source means naming its terms, and we cannot name what we did not read.
//
// 4. Nothing merges. An `in` verdict writes `council.verdict` to the decision
//    log and puts the proposal on the Plan Canvas, where Andre clicks. The
//    whole loop is experimental and its output is a proposal, not a commit.

use serde::{Deserialize, Serialize};

use crate::project_management::TicketRecord;

/// The ticket type a finding is filed as.
pub const FINDING_TYPE: &str = "finding";

/// The tag a finding earns by clearing the threshold. See decision 1 above.
pub const POC_TAG: &str = "poc";

/// The tag a finding gets when it was weighed and did not clear.
pub const SHELVED_TAG: &str = "shelved";

/// What a run that ran out of budget leaves on the ticket, verbatim, because
/// the acceptance criterion quotes it.
pub const TOO_BIG: &str = "too big to PoC";

/// The decision-log boundary the Judge writes at. Already in
/// [`crate::decisions::BOUNDARIES`] and, until this module, written by nobody.
pub const VERDICT_BOUNDARY: &str = "council.verdict";

/// The profile roles the core team resolves on. The role is the contract and
/// the handle is a name, the same rule as every persona since XNAUT-355.
pub const REVIEWER_ROLE: &str = "reviewer";

// ─── Findings ────────────────────────────────────────────────────────────────

/// One repository the Researcher thinks is worth a look.
///
/// `licence` and `file` are not decoration: the acceptance criterion asks for
/// them filled on every finding ticket, because they are the two facts that
/// decide whether the thing can be borrowed at all and where to start reading.
/// A finding missing either is refused at the door rather than filed and
/// chased later.
///
/// The aliases are for the model, not for us. A search model hands back
/// `license`, `url` and `repo` at least as often as the spellings here, and
/// dropping a good finding over a vowel would be a silly way to lose one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    #[serde(alias = "url", alias = "repo", alias = "html_url")]
    pub repo_url: String,
    #[serde(default, alias = "full_name")]
    pub name: String,
    #[serde(default, deserialize_with = "lenient_u64")]
    pub stars: u64,
    #[serde(default, alias = "license")]
    pub licence: String,
    #[serde(default, alias = "pushed_at", alias = "last_commit_at")]
    pub last_commit: String,
    /// What it does, in the Researcher's own two lines.
    #[serde(default, alias = "what_it_does", alias = "summary")]
    pub does: String,
    /// What in xNAUT it would touch.
    #[serde(default, alias = "what_it_would_touch", alias = "touches_xnaut")]
    pub touches: String,
    /// The one file worth opening first.
    #[serde(default, alias = "interesting_file", alias = "source_file")]
    pub file: String,
}

/// Stars arrive as `1200`, `"1.2k"`, `1200.0` or `null` depending on the model.
/// None of those is a reason to drop a finding, and none of them is worth a
/// parser: anything that is not plainly a number counts as zero, and the star
/// count is a tiebreaker rather than a gate.
fn lenient_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Number(number) => number.as_f64().unwrap_or(0.0).max(0.0) as u64,
        serde_json::Value::String(text) => text.trim().parse::<u64>().unwrap_or(0),
        _ => 0,
    })
}

impl Finding {
    /// The fields a finding may not be filed without, named.
    ///
    /// Returned as a list rather than a bool so the skip line can say which
    /// one was missing. "3 findings skipped" is a support question; "2 with no
    /// licence, 1 with no file" is an instruction to the next prompt.
    pub fn gaps(&self) -> Vec<&'static str> {
        let mut gaps = Vec::new();
        if self.repo_url.trim().is_empty() {
            gaps.push("repo");
        }
        if self.licence.trim().is_empty() {
            gaps.push("licence");
        }
        if self.file.trim().is_empty() {
            gaps.push("file");
        }
        if self.does.trim().is_empty() {
            gaps.push("what it does");
        }
        gaps
    }

    /// The name to show, falling back to `owner/name` off the URL.
    ///
    /// Owner AND name, not just the name: two projects called `agent-loop` are
    /// not rare, and a slug that dropped the owner would give them the same PoC
    /// branch and the same vault document.
    pub fn display_name(&self) -> String {
        let name = self.name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
        let normalised = normalise_repo(&self.repo_url);
        let mut segments: Vec<&str> = normalised.split('/').filter(|s| !s.is_empty()).collect();
        // Drop the host; everything after it is the project's own path.
        if segments.len() > 2 {
            segments = segments.split_off(segments.len() - 2);
        } else if segments.len() == 2 && segments[0].contains('.') {
            segments.remove(0);
        }
        segments.join("/")
    }

    /// The ticket title a finding is filed under.
    pub fn title(&self) -> String {
        let does = self.does.trim().lines().next().unwrap_or("").trim();
        match does.is_empty() {
            true => self.display_name(),
            false => format!("{}: {does}", self.display_name()),
        }
    }

    /// The ticket body, in the one shape [`parse_finding`] reads back.
    ///
    /// A fixed block of `- key: value` lines and two headed sections, rather
    /// than JSON in a fence, because this body is read by a person on the
    /// board far more often than by this parser.
    pub fn body(&self) -> String {
        format!(
            "- repo: {}\n- name: {}\n- stars: {}\n- licence: {}\n- last commit: {}\n- file: {}\n\n## What it does\n\n{}\n\n## What it would touch\n\n{}\n",
            self.repo_url.trim(),
            self.display_name(),
            self.stars,
            self.licence.trim(),
            self.last_commit.trim(),
            self.file.trim(),
            self.does.trim(),
            self.touches.trim(),
        )
    }
}

/// Read a finding back off a ticket body.
///
/// Lenient by construction: a finding ticket accumulates review blocks,
/// dispatch notes and verdicts underneath, and a parser that demanded the
/// whole document match would stop working the first time somebody appended
/// to it. Only the leading key block and the two named sections are read;
/// everything after the next heading belongs to whoever wrote it.
pub fn parse_finding(body: &str) -> Finding {
    let mut finding = Finding::default();
    let mut section: Option<&str> = None;
    let mut does = Vec::new();
    let mut touches = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("## ") {
            section = match heading.trim().to_ascii_lowercase().as_str() {
                "what it does" => Some("does"),
                "what it would touch" => Some("touches"),
                _ => None,
            };
            continue;
        }
        match section {
            Some("does") => does.push(line),
            Some("touches") => touches.push(line),
            Some(_) => {}
            None => {
                if let Some((key, value)) = trimmed
                    .strip_prefix("- ")
                    .and_then(|rest| rest.split_once(':'))
                {
                    let value = value.trim().to_string();
                    match key.trim().to_ascii_lowercase().as_str() {
                        "repo" => finding.repo_url = value,
                        "name" => finding.name = value,
                        "stars" => finding.stars = value.parse().unwrap_or(0),
                        "licence" | "license" => finding.licence = value,
                        "last commit" => finding.last_commit = value,
                        "file" => finding.file = value,
                        _ => {}
                    }
                }
            }
        }
    }
    finding.does = does.join("\n").trim().to_string();
    finding.touches = touches.join("\n").trim().to_string();
    finding
}

/// The dedup key for a repository.
///
/// `git@github.com:a/b.git`, `https://github.com/A/B/`, `http://www.github.com/a/b?tab=readme`
/// and `github.com/a/b` are one repository, and a weekly beat that files four
/// tickets for it is the failure the acceptance criterion names. Case included:
/// GitHub itself treats owner and repo case-insensitively.
pub fn normalise_repo(url: &str) -> String {
    let mut value = url.trim().to_ascii_lowercase();
    for prefix in ["https://", "http://", "ssh://", "git+https://"] {
        if let Some(rest) = value.strip_prefix(prefix) {
            value = rest.to_string();
        }
    }
    if let Some(rest) = value.strip_prefix("git@") {
        value = rest.replacen(':', "/", 1);
    }
    if let Some(rest) = value.strip_prefix("www.") {
        value = rest.to_string();
    }
    for cut in ['?', '#'] {
        if let Some((head, _)) = value.split_once(cut) {
            value = head.to_string();
        }
    }
    value = value.trim_end_matches('/').to_string();
    value = value.trim_end_matches(".git").to_string();
    value.trim_end_matches('/').to_string()
}

/// A short, filesystem-safe name for a finding: the PoC branch and doc use it.
pub fn slug_for(finding: &Finding) -> String {
    let raw = finding.display_name();
    let mut slug = String::new();
    for character in raw.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    let slug: String = slug.chars().take(40).collect();
    let slug = slug.trim_matches('-').to_string();
    match slug.is_empty() {
        true => "poc".to_string(),
        false => slug,
    }
}

/// The branch a PoC of this finding is built on.
pub fn poc_branch(slug: &str) -> String {
    format!("poc/{slug}")
}

/// The PoC branch for a ticket, when the ticket is a finding that has been
/// tagged for one.
///
/// This is the seam `dispatch.rs` asks, and it is a function rather than a
/// second copy of the format string for the reason `branch_for` already gives:
/// a swarm card shows the branch BEFORE dispatch creates it, and two spellings
/// would put a different branch on the card than in the repo.
pub fn poc_branch_for(ticket: &TicketRecord) -> Option<String> {
    if !ticket.ticket_type.eq_ignore_ascii_case(FINDING_TYPE) {
        return None;
    }
    if !ticket.tags.iter().any(|tag| tag.trim().eq_ignore_ascii_case(POC_TAG)) {
        return None;
    }
    let finding = parse_finding(&ticket.body);
    if finding.repo_url.trim().is_empty() {
        return None;
    }
    Some(poc_branch(&slug_for(&finding)))
}

/// Where a PoC writes up what it learned.
pub fn poc_doc_rel(date: &str, slug: &str) -> String {
    format!("Development/poc/{date}_{slug}.md")
}

/// Every repository the board has already seen, normalised.
pub fn seen_repos(tickets: &[TicketRecord]) -> Vec<String> {
    let mut seen: Vec<String> = tickets
        .iter()
        .filter(|ticket| ticket.ticket_type.eq_ignore_ascii_case(FINDING_TYPE))
        .map(|ticket| normalise_repo(&parse_finding(&ticket.body).repo_url))
        .filter(|url| !url.is_empty())
        .collect();
    seen.sort();
    seen.dedup();
    seen
}

/// A finding the beat did not file, and why. Never a silent drop: a scan that
/// quietly discards half its findings looks exactly like a quiet week.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub repo_url: String,
    pub reason: String,
}

/// Split a scan into what gets filed and what does not.
///
/// Dedup runs against the board AND against the batch itself, because one
/// answer naming the same repository twice is a duplicate the moment the
/// second ticket is written, not next week.
pub fn triage(findings: Vec<Finding>, seen: &[String]) -> (Vec<Finding>, Vec<Skipped>) {
    let mut known: Vec<String> = seen.to_vec();
    let mut keep = Vec::new();
    let mut skipped = Vec::new();
    for finding in findings {
        let key = normalise_repo(&finding.repo_url);
        let gaps = finding.gaps();
        if !gaps.is_empty() {
            skipped.push(Skipped {
                repo_url: finding.repo_url.clone(),
                reason: format!("no {}", gaps.join(", no ")),
            });
            continue;
        }
        if known.contains(&key) {
            skipped.push(Skipped {
                repo_url: finding.repo_url.clone(),
                reason: "already seen".to_string(),
            });
            continue;
        }
        known.push(key);
        keep.push(finding);
    }
    (keep, skipped)
}

// ─── The Researcher's brief ──────────────────────────────────────────────────

/// What the Researcher is asked for on the beat.
///
/// The seen list is IN the prompt rather than only in the dedup afterwards.
/// Filtering after the fact works and costs a full answer to throw half of it
/// away; a search model handed the exclusions spends its budget on repositories
/// we have not read. The dedup still runs, because a prompt is a request.
pub fn scan_prompt(topics: &[String], seen: &[String]) -> String {
    let topics = topics
        .iter()
        .map(|topic| topic.trim())
        .filter(|topic| !topic.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let mut prompt = format!(
        "Search GitHub for repositories about: {topics}.\n\
         Prefer repositories with a commit in the last 90 days. At most 8.\n\n\
         Return ONLY a JSON array, no prose around it. Each element:\n\
         {{\"repo_url\": \"https://github.com/owner/name\", \"name\": \"owner/name\", \
         \"stars\": 0, \"licence\": \"the SPDX id, e.g. MIT or Apache-2.0\", \
         \"last_commit\": \"YYYY-MM-DD\", \"does\": \"at most two sentences\", \
         \"touches\": \"which part of a Rust/Tauri agent workspace this would change\", \
         \"file\": \"the one path in that repository worth opening first\"}}\n\n\
         Every field must come from what you actually read. Omit a repository \
         rather than guessing its licence or inventing a file path: a finding \
         with no licence and no file cannot be acted on and is thrown away."
    );
    if !seen.is_empty() {
        prompt.push_str("\n\nAlready read, do not return these:\n");
        for repo in seen {
            prompt.push_str(&format!("- {repo}\n"));
        }
    }
    prompt
}

/// Pull findings out of whatever envelope the model wrapped them in.
///
/// A search model answers with prose, a fenced block, or a bare array,
/// depending on the day. `jury::parse_review` solved the same problem for a
/// single object by walking the parsed value; an array in a fence is not
/// parseable to begin with, so this one finds the balanced brackets first.
pub fn parse_findings(answer: &str) -> Vec<Finding> {
    let Some(slice) = json_array(answer) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<Finding>>(slice).unwrap_or_default()
}

/// The first balanced `[...]` in the text, ignoring brackets inside strings.
fn json_array(text: &str) -> Option<&str> {
    balanced(text, b'[', b']')
}

// ─── The Reviewer's rubric ───────────────────────────────────────────────────

/// The five dimensions, each scored 0..=5, and what each is worth.
///
/// Weights rather than an average because they are not equally informative.
/// Fit carries the most: a brilliant mechanism for a problem xNAUT does not
/// have is worth nothing here, and it is the only dimension a model reliably
/// gets wrong in the optimistic direction. Novelty is next because the whole
/// loop exists to find what we have NOT already built; the Reviewer is told to
/// read the code graph before scoring it, so novelty is a claim about this
/// codebase rather than about the world.
pub const WEIGHTS: [(&str, u32); 5] = [
    ("fit", 30),
    ("novelty", 25),
    ("port size", 15),
    ("licence", 15),
    ("activity", 15),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rubric {
    /// Does xNAUT have this problem?
    #[serde(default)]
    pub fit: u8,
    /// Is it a mechanism we do not already have? Scored after reading the graph.
    #[serde(default)]
    pub novelty: u8,
    /// How small is the port? 5 is one file, 0 is a rewrite.
    #[serde(default, alias = "port_size_score")]
    pub port_size: u8,
    /// Can we credit it? 5 is a permissive licence we can name.
    #[serde(default, alias = "license")]
    pub licence: u8,
    /// Is anyone still maintaining it?
    #[serde(default)]
    pub activity: u8,
}

impl Rubric {
    /// 0..=100. Each dimension is clamped to its 0..=5 range first: a model
    /// that answers `fit: 9` has not scored a nine, it has misread the scale,
    /// and letting it through would put a finding above the threshold on
    /// arithmetic nobody chose.
    pub fn score(&self) -> u32 {
        let values = [
            self.fit,
            self.novelty,
            self.port_size,
            self.licence,
            self.activity,
        ];
        values
            .iter()
            .zip(WEIGHTS.iter())
            .map(|(value, (_, weight))| u32::from((*value).min(5)) * weight / 5)
            .sum()
    }
}

/// What a licence lets us do with the thing behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Licence {
    Permissive,
    Copyleft,
    /// No licence, an unrecognised one, or GitHub's `NOASSERTION`.
    Unknown,
}

/// Classify a licence string.
///
/// Unknown and Copyleft are refused with equal force, and that is deliberate.
/// Copyleft is the obvious one. Unknown is the one that would actually have
/// happened: a model answering "custom" or "see LICENSE" reads as a small
/// uncertainty, and the standing rule is that a borrowed mechanism names its
/// source AND its licence in the file header. A licence we cannot name is a
/// credit we cannot write.
pub fn licence_class(licence: &str) -> Licence {
    let value = licence.trim().to_ascii_lowercase().replace('_', "-");
    if value.is_empty() || value.contains("noassertion") || value.contains("other") {
        return Licence::Unknown;
    }
    const COPYLEFT: [&str; 8] = ["gpl", "agpl", "lgpl", "sspl", "busl", "osl", "epl", "cc-by-sa"];
    if COPYLEFT.iter().any(|needle| value.contains(needle)) {
        return Licence::Copyleft;
    }
    const PERMISSIVE: [&str; 10] = [
        "mit",
        "apache",
        "bsd",
        "isc",
        "unlicense",
        "0bsd",
        "zlib",
        "mpl",
        "cc0",
        "public domain",
    ];
    match PERMISSIVE.iter().any(|needle| value.contains(needle)) {
        true => Licence::Permissive,
        false => Licence::Unknown,
    }
}

/// What the Reviewer decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    /// Above the threshold and portable: it earns a PoC.
    Poc,
    /// Weighed and not worth the run.
    Shelved,
    /// Refused on the licence, whatever it scored.
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub rubric: Rubric,
    pub score: u32,
    pub threshold: u32,
    pub licence: Licence,
    pub verdict: ReviewVerdict,
    /// One line a person can act on.
    pub why: String,
}

impl Review {
    /// The tag this review puts on the ticket.
    pub fn tag(&self) -> &'static str {
        match self.verdict {
            ReviewVerdict::Poc => POC_TAG,
            _ => SHELVED_TAG,
        }
    }

    /// The block appended to the finding ticket. Dated, so a re-review reads
    /// as a second opinion rather than overwriting the first.
    pub fn note(&self, date: &str, handle: &str) -> String {
        let dimensions = [
            ("fit", self.rubric.fit),
            ("novelty", self.rubric.novelty),
            ("port size", self.rubric.port_size),
            ("licence", self.rubric.licence),
            ("activity", self.rubric.activity),
        ]
        .iter()
        .map(|(name, value)| format!("{name} {value}/5"))
        .collect::<Vec<_>>()
        .join(", ");
        format!(
            "\n\n## Weighed {date} by @{handle}\n\n- score: {}/100 (threshold {})\n- {dimensions}\n- licence: {:?}\n- verdict: {:?}\n\n{}\n",
            self.score, self.threshold, self.licence, self.verdict, self.why
        )
    }
}

/// Weigh a scored finding against the threshold.
///
/// The licence gate runs FIRST and cannot be outvoted; see decision 3 in the
/// module header.
pub fn weigh(finding: &Finding, rubric: Rubric, threshold: u32) -> Review {
    let licence = licence_class(&finding.licence);
    let score = rubric.score();
    let (verdict, why) = match licence {
        Licence::Copyleft => (
            ReviewVerdict::Refused,
            format!(
                "{} is copyleft; xNAUT credits what it ports and cannot carry those terms",
                finding.licence.trim()
            ),
        ),
        Licence::Unknown => (
            ReviewVerdict::Refused,
            match finding.licence.trim() {
                "" => "no licence named, so there is no credit line to write".to_string(),
                other => format!("licence \"{other}\" is not one we can name in a credit header"),
            },
        ),
        Licence::Permissive if score >= threshold => (
            ReviewVerdict::Poc,
            format!("{score}/100 clears the {threshold} threshold; worth a prototype"),
        ),
        Licence::Permissive => (
            ReviewVerdict::Shelved,
            format!("{score}/100 is under the {threshold} threshold"),
        ),
    };
    Review {
        rubric,
        score,
        threshold,
        licence,
        verdict,
        why,
    }
}

/// What the Reviewer is told it is for.
pub const REVIEW_SYSTEM: &str = "\
You are weighing one open-source repository as a candidate for porting a single mechanism into xNAUT, \
a Rust/Tauri agent workspace. You are not deciding whether the repository is good software. \
You are deciding whether xNAUT has the problem it solves, and whether one mechanism inside it \
could be reimplemented here in a small number of files with its source credited.

Score five dimensions, each an integer 0 to 5:
- fit: does xNAUT have this problem at all? 0 if the problem is not ours.
- novelty: is this a mechanism xNAUT does not already have? Read the listed modules before scoring. 0 if we already do this.
- port_size: how small is the port? 5 is one file, 0 is a rewrite of a subsystem.
- licence: can the source be credited and reimplemented? 5 permissive and clearly stated, 0 unstated or copyleft.
- activity: is anyone still maintaining it?

Return ONLY one JSON object: {\"fit\":0,\"novelty\":0,\"port_size\":0,\"licence\":0,\"activity\":0,\"why\":\"one sentence\"}. \
Uncertainty scores low. Never invent a fact about the repository you were not given.";

/// The Reviewer's input for one finding.
pub fn review_prompt(finding: &Finding, modules: &[String]) -> String {
    format!(
        "Candidate:\n{}\n\nxNAUT's existing backend modules, for the novelty score:\n{}\n",
        finding.body(),
        modules.join(", ")
    )
}

/// A scored rubric and the sentence behind it, off the model's answer.
#[derive(Clone, Debug, Default, Deserialize)]
struct ScoredRubric {
    #[serde(flatten)]
    rubric: Rubric,
    #[serde(default)]
    why: String,
}

/// Read the Reviewer's answer. `Err` names what was wrong with it rather than
/// defaulting to zeros: a rubric of all zeros and an unparseable answer are
/// the same number and completely different events.
pub fn parse_rubric(answer: &str) -> Result<(Rubric, String), String> {
    let slice = json_object(answer).ok_or("the reviewer returned no JSON object")?;
    let scored: ScoredRubric = serde_json::from_str(slice)
        .map_err(|error| format!("the reviewer's JSON did not fit the rubric: {error}"))?;
    Ok((scored.rubric, scored.why.trim().to_string()))
}

/// The first balanced `{...}`, ignoring braces inside strings.
fn json_object(text: &str) -> Option<&str> {
    balanced(text, b'{', b'}')
}

/// The first balanced `open..close` run, ignoring anything inside a JSON
/// string. One scanner for both brackets, because the string-aware part is the
/// only hard part and two copies of it would be two places to get escapes
/// wrong. Byte-indexed, and the slice is taken with `get`, so a multi-byte
/// character can never produce a panicking split.
fn balanced(text: &str, open: u8, close: u8) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|byte| *byte == open)?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        if *byte == b'"' {
            in_string = true;
        } else if *byte == open {
            depth += 1;
        } else if *byte == close {
            depth -= 1;
            if depth == 0 {
                return text.get(start..=index);
            }
        }
    }
    None
}

// ─── The PoC's document ──────────────────────────────────────────────────────

/// The sections a PoC write-up must carry, in the order the template writes
/// them. Every one of them is a question the Judge cannot answer without it.
pub const POC_SECTIONS: [&str; 6] = [
    "Source",
    "File",
    "Licence",
    "Departures",
    "Measurement",
    "Verdict",
];

/// What a PoC handed back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PocDoc {
    pub source: String,
    pub file: String,
    pub licence: String,
    pub departures: String,
    pub measurement: String,
    pub verdict: String,
}

/// The document a PoC run is told to write.
pub fn poc_template(finding: &Finding, slug: &str) -> String {
    format!(
        "---\nAuthor: xNAUT core team\nLast modified: \n---\n\n\
         # PoC: {slug}\n\n\
         ## Source\n\n{}\n\n\
         ## File\n\n{}\n\n\
         ## Licence\n\n{}\n\n\
         ## Departures\n\nWhat this implementation does differently from the original, and why.\n\n\
         ## Measurement\n\nA number, measured here, against the same number without the change.\n\n\
         ## Verdict\n\nin or out, and the reason.\n",
        finding.repo_url.trim(),
        finding.file.trim(),
        finding.licence.trim(),
    )
}

/// Read a PoC document, or name every section it is missing.
///
/// The `Err` is a LIST because a document returned one gap at a time is three
/// round trips. The acceptance criterion — "a PoC doc without a credit line is
/// returned, not judged" — is this function returning `Err` before the Judge
/// is asked to spend anything.
pub fn read_poc_doc(text: &str) -> Result<PocDoc, Vec<String>> {
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        if let Some(heading) = line.trim().strip_prefix("## ") {
            sections.push((heading.trim().to_ascii_lowercase(), Vec::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push(line);
        }
    }
    let get = |want: &str| -> String {
        sections
            .iter()
            .find(|(name, _)| name == &want.to_ascii_lowercase())
            .map(|(_, body)| body.join("\n").trim().to_string())
            .unwrap_or_default()
    };
    let doc = PocDoc {
        source: get("Source"),
        file: get("File"),
        licence: get("Licence"),
        departures: get("Departures"),
        measurement: get("Measurement"),
        verdict: get("Verdict"),
    };
    let mut gaps: Vec<String> = POC_SECTIONS
        .iter()
        .zip([
            &doc.source,
            &doc.file,
            &doc.licence,
            &doc.departures,
            &doc.measurement,
            &doc.verdict,
        ])
        .filter(|(_, value)| value.trim().is_empty() || is_template_prose(value))
        .map(|(name, _)| format!("the PoC document has no {}", name.to_ascii_lowercase()))
        .collect();
    // A measurement with no number in it is an impression wearing a heading.
    // "noticeably faster" passed the section check on every draft it was ever
    // tried on, which is exactly why the check cannot be the section alone.
    if gaps.is_empty() && !doc.measurement.chars().any(|c| c.is_ascii_digit()) {
        gaps.push("the measurement has no number in it".to_string());
    }
    match gaps.is_empty() {
        true => Ok(doc),
        false => Err(gaps),
    }
}

/// Whether a section still holds the template's own instruction. A document
/// handed back unedited must not read as a document that was filled in.
fn is_template_prose(value: &str) -> bool {
    const PROMPTS: [&str; 3] = [
        "what this implementation does differently",
        "a number, measured here",
        "in or out, and the reason",
    ];
    let lowered = value.trim().to_ascii_lowercase();
    PROMPTS.iter().any(|prompt| lowered.starts_with(prompt))
}

/// Whether a file carries a credit header naming where its mechanism came from.
///
/// The verbs are this repository's own: `gate_score.rs`, `plateau.rs`,
/// `shared_notes.rs`, `canvas.rs`, `veto.rs` and `writer_lease.rs` between
/// them say Ported, Borrowed, Idea, Adapted and Taken, all followed by a
/// parenthesised source and licence. Matching the form we already write beats
/// inventing a stricter one nobody's existing file would pass.
pub fn has_credit_header(text: &str) -> bool {
    text.lines().take(40).any(|line| {
        let lowered = line.to_ascii_lowercase();
        const VERBS: [&str; 5] = [
            "ported from",
            "borrowed from",
            "idea from",
            "adapted from",
            "taken from",
        ];
        VERBS.iter().any(|verb| lowered.contains(verb))
            && line
                .split_once('(')
                .is_some_and(|(_, rest)| rest.contains(')'))
    })
}

/// Everything that would stop the Judge being asked, named at once.
///
/// `credited` is the set of files changed on the PoC branch that carry a
/// credit header. Requiring ONE rather than all of them: a port is a file plus
/// its test plus a line in `main.rs`, and demanding the header on the
/// registration line would teach agents to paste credit into places it does
/// not belong.
pub fn poc_gaps(doc_text: &str, credited: &[String]) -> Vec<String> {
    let mut gaps = match read_poc_doc(doc_text) {
        Ok(_) => Vec::new(),
        Err(gaps) => gaps,
    };
    if credited.is_empty() {
        gaps.push(
            "no file on the PoC branch carries a credit header naming its source and licence"
                .to_string(),
        );
    }
    gaps
}

// ─── The Judge ───────────────────────────────────────────────────────────────

/// The evidence the Judge weighs. Deliberately flat text: the jury prompt
/// wraps it as UNTRUSTED DATA, and structure the reviewer could mistake for
/// instructions is exactly what that wrapper exists to defuse.
pub fn judge_input(ticket: &TicketRecord, doc_rel: &str, doc_text: &str, credited: &[String]) -> String {
    format!(
        "Finding ticket {} ({}):\n{}\n\nPoC document {doc_rel}:\n{doc_text}\n\nFiles on the PoC branch carrying a credit header:\n{}\n",
        ticket.id,
        ticket.title,
        ticket.body,
        match credited.is_empty() {
            true => "none".to_string(),
            false => credited.join("\n"),
        }
    )
}

/// What a judgement came to: the three words the decision log carries.
pub const VERDICT_IN: &str = "in";
pub const VERDICT_OUT: &str = "out";
/// The jury could not settle it, so the owner must. Not a rejection.
pub const VERDICT_OWNER: &str = "owner";

/// The decision-log entry for a verdict.
///
/// `role` is the Judge rather than a handle: two runtimes voted and neither of
/// them is the author of the decision.
///
/// `open` is the interesting field. `in` and `out` both END the argument the
/// finding opened, so both are settled. An ESCALATION does not: the jury said
/// it cannot decide, and the decision log's one rule is that an open item stays
/// open until something explicitly closes it. Filing an escalation as a
/// rejection would let the brief report a live question as a finished one,
/// which is the exact failure `decisions.rs` exists to prevent — and the `key`
/// is what a later `in` or `out` on the same PoC closes it with.
pub fn verdict_decision(
    slug: &str,
    verdict: &str,
    why: &str,
    against: &str,
) -> crate::decisions::Decision {
    crate::decisions::Decision {
        ts: String::new(),
        role: "Judge".to_string(),
        boundary: VERDICT_BOUNDARY.to_string(),
        what: format!("PoC {slug}: {verdict}"),
        why: why.trim().to_string(),
        alternatives: against.trim().to_string(),
        key: format!("poc:{slug}"),
        open: verdict == VERDICT_OWNER,
    }
}

/// The marker a judged finding carries, so the sweep judges each PoC once.
///
/// The TICKET is the memory, not a field in the sweep's in-process state: a
/// verdict that survives only until the app restarts would be re-bought every
/// launch, and a jury round is two agent runs.
pub const JUDGED_HEADING: &str = "## Judged";

/// The marker on a PoC whose document was sent back.
pub const RETURNED_HEADING: &str = "## Returned";

/// Findings whose PoC is finished and which nobody has judged.
///
/// `done` and `review` are the same claim from an agent — I have finished,
/// someone else must look — and for a PoC that someone is the Judge. A ticket
/// already carrying a verdict or a return is left alone: both are answers, and
/// re-judging a returned document before anyone has fixed it would buy the same
/// refusal twice.
pub fn judgeable(tickets: &[TicketRecord]) -> Vec<&TicketRecord> {
    tickets
        .iter()
        .filter(|ticket| ticket.ticket_type.eq_ignore_ascii_case(FINDING_TYPE))
        .filter(|ticket| {
            ticket
                .tags
                .iter()
                .any(|tag| tag.trim().eq_ignore_ascii_case(POC_TAG))
        })
        .filter(|ticket| matches!(ticket.status.as_str(), "done" | "review"))
        .filter(|ticket| {
            !ticket.body.contains(JUDGED_HEADING) && !ticket.body.contains(RETURNED_HEADING)
        })
        .collect()
}

// ─── The wall clock ──────────────────────────────────────────────────────────

/// Is this run a PoC? The branch says so, and it is the only thing that does.
///
/// Not the ticket: by the time the wall clock is checked the run is what
/// exists, and a manifest carries its branch. `poc/` is the one prefix
/// `poc_branch` ever writes.
pub fn is_poc_run(branch: &str) -> bool {
    branch.trim().starts_with("poc/")
}

/// PoC runs that have outlived their budget.
///
/// A wall clock rather than tokens, and that is the honest bound for this one
/// thing: a PoC is a timeboxed experiment, "ninety minutes" is a sentence the
/// owner can reason about, and a token count is not. A run that needs longer
/// has answered the question — it is too big to PoC — which is why the phrase
/// the acceptance criterion asks for is the OUTCOME here and not a failure.
pub fn over_budget(
    runs: &[crate::run_control::RunManifest],
    now_ms: i64,
    minutes: u64,
) -> Vec<&crate::run_control::RunManifest> {
    let ceiling = (minutes.max(1) as i64).saturating_mul(60_000);
    runs.iter()
        .filter(|run| run.kind == crate::run_control::RunKind::Agent)
        .filter(|run| !run.state.terminal())
        .filter(|run| is_poc_run(&run.branch))
        .filter(|run| run.started_at > 0 && now_ms.saturating_sub(run.started_at) > ceiling)
        .collect()
}

/// What the ticket says when the clock ran out. Carries the phrase verbatim.
pub fn too_big_note(minutes: u64, branch: &str) -> String {
    format!(
        "\n\n## {TOO_BIG}\n\nThe run passed its {minutes}-minute wall clock and was stopped. \
         `{branch}` is left where it reached; there is no judgement to make on it.\n"
    )
}

// ─── The beat ────────────────────────────────────────────────────────────────

/// When the core team last looked, so a weekly beat survives a restart.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BeatState {
    #[serde(default)]
    pub last_beat_ms: i64,
    #[serde(default)]
    pub last_filed: usize,
    #[serde(default)]
    pub last_reason: String,
}

/// Is the beat due?
///
/// A count of days rather than a cron expression: the beat is a rhythm, not an
/// appointment, and a machine that was asleep on Monday should scan when it
/// wakes rather than wait for the next Monday. `0` last-beat means never, which
/// is due.
pub fn beat_due(last_beat_ms: i64, now_ms: i64, days: u64) -> bool {
    if last_beat_ms <= 0 {
        return true;
    }
    let interval = (days.max(1) as i64).saturating_mul(24 * 60 * 60 * 1000);
    now_ms.saturating_sub(last_beat_ms) >= interval
}

/// Where the beat's memory lives: beside the run registry, which is already
/// the per-machine state directory every other clock in the app writes to.
pub fn beat_state_path(registry: &std::path::Path) -> std::path::PathBuf {
    registry.join("core-team.json")
}

pub fn read_beat_state(registry: &std::path::Path) -> BeatState {
    std::fs::read(beat_state_path(registry))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn write_beat_state(registry: &std::path::Path, state: &BeatState) -> Result<(), String> {
    std::fs::create_dir_all(registry).map_err(|error| error.to_string())?;
    crate::project_management::write_json_atomic(&beat_state_path(registry), state)
}

// ─── Status ──────────────────────────────────────────────────────────────────

/// Who is on the core team, and what stops it running. Answered from the
/// profile store and settings alone, like `research_status`: a pane must be
/// able to say why the loop is idle without spending a token to find out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreTeamStatus {
    pub enabled: bool,
    pub project: String,
    pub threshold: u32,
    pub beat_days: u64,
    pub topics: Vec<String>,
    /// Empty when the team can run; otherwise a sentence a person can act on.
    pub blocked: Vec<String>,
    pub researcher: String,
    pub reviewer: String,
    pub reviewer_provider: String,
    pub last_beat_ms: i64,
}

/// What `blocked` says when the only thing missing is the switch.
///
/// A constant because two callers read it and one of them SKIPS it: the switch
/// governs the unattended beat, not a scan the owner pressed a button for, and
/// a manual scan refused by the automation toggle would be a button that can
/// never work. Matching on a substring of a sentence would be the kind of
/// silent coupling that survives every test and breaks on a reword.
pub const OFF_REASON: &str = "the core team is off in Settings — it is experimental";

impl CoreTeamStatus {
    /// Everything in the way of a scan the owner asked for by hand.
    pub fn blockers_the_owner_cannot_override(&self) -> Vec<String> {
        self.blocked
            .iter()
            .filter(|line| line.as_str() != OFF_REASON)
            .cloned()
            .collect()
    }
}

/// Work out what is missing, from the two stores that decide it.
pub fn status_from(
    settings: &crate::settings::Settings,
    profiles: &[crate::agent_profiles::AgentProfile],
    last_beat_ms: i64,
) -> CoreTeamStatus {
    let core = &settings.core_team;
    let mut blocked = Vec::new();
    if !core.enabled {
        blocked.push(OFF_REASON.to_string());
    }
    let research = crate::research::status_from(profiles, settings);
    if !research.available {
        blocked.push(format!("the Researcher cannot run: {}", research.reason));
    }
    let reviewer = profiles
        .iter()
        .find(|profile| profile.role.trim().eq_ignore_ascii_case(REVIEWER_ROLE));
    match reviewer {
        None => blocked.push("no reviewer profile — add one in the Agent Library".to_string()),
        Some(profile) => {
            if profile
                .provider
                .trim()
                .eq_ignore_ascii_case(research.provider.trim())
            {
                // The Reviewer exists to be a SECOND opinion. On the same
                // provider as the Researcher it is the same weights reading
                // back its own answer, which is the exact failure that kept
                // Council out of NautFlow (XNAUT-356).
                blocked.push(format!(
                    "@{} reviews on {}, the provider the Researcher already used",
                    profile.handle, profile.provider
                ));
            }
            if crate::chat::provider_llm(settings, &profile.provider).is_none() {
                blocked.push(format!(
                    "no {} endpoint — add one in Settings › AI",
                    profile.provider
                ));
            }
        }
    }
    if core.topics.iter().all(|topic| topic.trim().is_empty()) {
        blocked.push("no topics — the Researcher has nothing to search for".to_string());
    }
    CoreTeamStatus {
        enabled: core.enabled,
        project: core.project.clone(),
        threshold: core.poc_threshold,
        beat_days: core.beat_days,
        topics: core.topics.clone(),
        blocked,
        researcher: research.handle,
        reviewer: reviewer.map(|p| p.handle.clone()).unwrap_or_default(),
        reviewer_provider: reviewer.map(|p| p.provider.clone()).unwrap_or_default(),
        last_beat_ms,
    }
}

// ─── Commands ────────────────────────────────────────────────────────────────

/// Who is on the core team and what is stopping it. Costs nothing.
#[tauri::command]
pub async fn core_team_status(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<CoreTeamStatus, String> {
    let settings = state.settings.lock().await.clone();
    let profiles = crate::agent_profiles::agent_profile_list().unwrap_or_default();
    let last = crate::agents::registry_dir()
        .map(|registry| read_beat_state(&registry).last_beat_ms)
        .unwrap_or(0);
    Ok(status_from(&settings, &profiles, last))
}

/// One finding, weighed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Weighed {
    pub ticket: String,
    pub score: u32,
    pub verdict: ReviewVerdict,
    /// Empty when it was weighed; otherwise why it could not be.
    pub error: String,
}

/// What one scan did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ScanResult {
    pub filed: Vec<String>,
    /// What the Reviewer made of each one. Same length as `filed` unless the
    /// Reviewer itself failed, and a failure is a row rather than an absence.
    pub weighed: Vec<Weighed>,
    pub skipped: Vec<Skipped>,
    /// Empty when the scan ran. Otherwise why nothing was searched for.
    pub blocked: String,
}

/// Send the Researcher out, file what comes back, and weigh every ticket it
/// filed.
///
/// The two halves are ONE command because splitting them leaves a hole: a scan
/// that files without weighing produces findings that nothing will ever pick
/// up, since the Reviewer only ever runs on what a scan just filed. That bound
/// — the Reviewer costs one call per NEW finding and never re-weighs the board
/// — is also the cost bound the ticket asks for, so it is kept by making the
/// two inseparable rather than by remembering to call both.
///
/// Every refusal is a NAMED skip, the rule `swarm_plan` already follows: a
/// scan that quietly files three of eight findings is worse than one that
/// refuses, because nothing on screen says which five are missing. A finding
/// the Reviewer could not score is a `Weighed` row carrying the error, for the
/// same reason.
#[tauri::command]
pub async fn core_team_scan(
    state: tauri::State<'_, crate::state::AppState>,
    topics: Option<Vec<String>>,
) -> Result<ScanResult, String> {
    let settings = state.settings.lock().await.clone();
    let profiles = crate::agent_profiles::agent_profile_list().unwrap_or_default();
    // The switch is skipped here on purpose. It governs the unattended beat;
    // this command is somebody pressing Scan now, which is how you find out
    // whether the loop works BEFORE you agree to let it run weekly.
    let status = status_from(&settings, &profiles, 0);
    let blockers = status.blockers_the_owner_cannot_override();
    if !blockers.is_empty() {
        return Ok(ScanResult {
            blocked: blockers.join("; "),
            ..ScanResult::default()
        });
    }
    let project = settings.core_team.project.clone();
    let repo = crate::project_management::configured_repo(&settings.project_management)?;
    let existing = crate::project_management::ticket_list_in(&repo, Some(project.clone()))?;
    let seen = seen_repos(&existing);
    let topics = topics.unwrap_or_else(|| settings.core_team.topics.clone());

    let brief = crate::research::research_brief(scan_prompt(&topics, &seen), None).await?;
    if !brief.available {
        return Ok(ScanResult {
            blocked: brief.reason,
            ..ScanResult::default()
        });
    }
    let (keep, skipped) = triage(parse_findings(&brief.answer), &seen);
    let mut filed = Vec::new();
    for finding in keep {
        let created = crate::project_management::ticket_create_in(
            &repo,
            crate::project_management::TicketCreateRequest {
                model_requirement: String::new(),
                project: project.clone(),
                title: finding.title(),
                ticket_type: FINDING_TYPE.to_string(),
                status: "inbox".to_string(),
                priority: "low".to_string(),
                owner: None,
                documentation: Vec::new(),
                body: finding.body(),
                parent: None,
                release: String::new(),
                tags: Vec::new(),
                source_id: String::new(),
            },
        )?;
        filed.push(created.id);
    }
    // Sequential, and each failure is its own row rather than an early return:
    // one candidate the Reviewer cannot score must not cost the other seven
    // their review.
    let mut weighed = Vec::new();
    for ticket in &filed {
        weighed.push(match core_team_review(state.clone(), ticket.clone()).await {
            Ok(review) => Weighed {
                ticket: ticket.clone(),
                score: review.score,
                verdict: review.verdict,
                error: String::new(),
            },
            Err(error) => Weighed {
                ticket: ticket.clone(),
                score: 0,
                verdict: ReviewVerdict::Shelved,
                error,
            },
        });
    }
    Ok(ScanResult {
        filed,
        weighed,
        skipped,
        blocked: String::new(),
    })
}

/// Weigh one finding on the Reviewer's provider and write the score onto it.
///
/// Not a Tauri command: `core_team_scan` is the only caller and the only one
/// that should be, because the Reviewer's cost bound IS "once per finding, at
/// the moment it is filed". A button that re-weighs an arbitrary ticket would
/// be a way around that bound, and an ACL entry nothing in the frontend calls
/// is surface with no user.
pub async fn core_team_review(
    state: tauri::State<'_, crate::state::AppState>,
    ticket_id: String,
) -> Result<Review, String> {
    let settings = state.settings.lock().await.clone();
    let profiles = crate::agent_profiles::agent_profile_list().unwrap_or_default();
    let reviewer = profiles
        .iter()
        .find(|profile| profile.role.trim().eq_ignore_ascii_case(REVIEWER_ROLE))
        .ok_or("no reviewer profile — add one in the Agent Library")?;
    let mut llm = crate::chat::provider_llm(&settings, &reviewer.provider).ok_or_else(|| {
        format!(
            "no {} endpoint — add one in Settings › AI",
            reviewer.provider
        )
    })?;
    if !reviewer.model.trim().is_empty() {
        llm.model = reviewer.model.trim().to_string();
    }

    let repo = crate::project_management::configured_repo(&settings.project_management)?;
    let ticket = crate::project_management::ticket_list_in(&repo, None)?
        .into_iter()
        .find(|item| item.id == ticket_id)
        .ok_or_else(|| format!("ticket {ticket_id} not found"))?;
    if !ticket.ticket_type.eq_ignore_ascii_case(FINDING_TYPE) {
        return Err(format!("{} is not a finding", ticket.id));
    }
    let finding = parse_finding(&ticket.body);

    let answer = crate::chat::complete_oneshot(
        &llm,
        Some(REVIEW_SYSTEM),
        &review_prompt(&finding, &backend_modules()),
    )
    .await?;
    let (rubric, why) = parse_rubric(&answer)?;
    let mut review = weigh(&finding, rubric, settings.core_team.poc_threshold);
    if !why.is_empty() {
        review.why = format!("{} {why}", review.why);
    }

    let mut tags = ticket.tags.clone();
    tags.retain(|tag| !tag.eq_ignore_ascii_case(POC_TAG) && !tag.eq_ignore_ascii_case(SHELVED_TAG));
    tags.push(review.tag().to_string());
    let body = format!(
        "{}{}",
        ticket.body,
        review.note(&chrono::Utc::now().to_rfc3339()[..10], &reviewer.handle)
    );
    // Tags are not on TicketUpdateRequest, so the record is rewritten in place
    // through the same atomic write every other ticket mutation uses.
    let retagged = crate::project_management::ticket_retag_in(&repo, &ticket.id, tags, body)?;
    if review.verdict == ReviewVerdict::Poc {
        promote_to_poc(&repo, &retagged)?;
    }
    Ok(review)
}

/// Put a finding that cleared the threshold in the fleet's way.
///
/// `ready` with an owner is the ONE shape the sweep dispatches (`sweep.rs`,
/// `ready_with_owner`), and the PoC is supposed to run like any other swarm
/// run rather than through a second launcher of its own. Everything below the
/// threshold stays in `inbox`, which the sweep never touches, so a shelved
/// finding cannot be triaged into work by accident.
///
/// The owner comes off the same installed-runtime list the sweep's triage
/// uses. An unowned finding would sit at `ready` forever: unowned triage only
/// looks at high and critical, and findings are filed low.
fn promote_to_poc(
    repo: &std::path::Path,
    ticket: &TicketRecord,
) -> Result<(), String> {
    let owner = crate::sweep::assignable_owners().into_iter().next();
    let note = match &owner {
        Some(handle) => format!("\n\nQueued for a PoC run: @{handle} builds it on `{}`.\n",
            poc_branch_for(ticket).unwrap_or_default()),
        None => "\n\nQueued for a PoC run, but no agent runtime is installed on this machine to build it.\n".to_string(),
    };
    crate::project_management::ticket_update_in(
        repo,
        crate::project_management::TicketUpdateRequest {
            model_requirement: None,
            caller: None,
            id: ticket.id.clone(),
            expected_revision: ticket.revision,
            title: None,
            ticket_type: None,
            status: Some("ready".to_string()),
            priority: Some("medium".to_string()),
            owner: Some(owner),
            clear_owner: false,
            documentation: None,
            body: Some(format!("{}{note}", ticket.body)),
        },
    )
    .map(|_| ())
}

/// The backend modules the Reviewer reads before scoring novelty.
///
/// The file list rather than a summary: "xNAUT already has a decision log" is
/// the kind of claim a model will make about a codebase it has not seen, and
/// `decisions.rs` sitting in the list is the cheapest possible proof.
fn backend_modules() -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut modules: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.ends_with(".rs"))
        .collect();
    modules.sort();
    modules
}

/// What a judgement did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct JudgeResult {
    pub ticket: String,
    pub slug: String,
    /// "in", "out", "owner" — or "returned" when the document was refused.
    pub decision: String,
    pub why: String,
    /// Everything that stopped the Judge being asked. Empty when it ran.
    pub returned: Vec<String>,
    /// The inbox item the Plan Canvas opens on, when the verdict was `in`.
    pub plan_inbox_id: String,
}

/// Judge a finished PoC.
///
/// The document check runs BEFORE anything is spent, because the acceptance
/// criterion is that an uncredited PoC is returned rather than judged, and a
/// jury job that opens and then refuses has already paid two reviewers to read
/// a document that was never admissible.
///
/// Not a Tauri command, for the same reason as `core_team_review`: the sweep
/// is the only caller, judging is what happens to a PoC that finished rather
/// than something to press, and an ACL entry nothing calls is dead surface.
pub async fn core_team_judge(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    ticket_id: String,
) -> Result<JudgeResult, String> {
    let settings = state.settings.lock().await.clone();
    let repo = crate::project_management::configured_repo(&settings.project_management)?;
    let ticket = crate::project_management::ticket_list_in(&repo, None)?
        .into_iter()
        .find(|item| item.id == ticket_id)
        .ok_or_else(|| format!("ticket {ticket_id} not found"))?;
    let finding = parse_finding(&ticket.body);
    let slug = slug_for(&finding);
    let branch = poc_branch(&slug);
    // The PoC was built on the checkout the ticket's project points at, so
    // that is the repository that knows where its worktree went. A CORE board
    // whose `source_path` is the xNAUT checkout is not an accident: the
    // findings are about changing xNAUT, and a board pointed somewhere else
    // dispatches its PoC into the wrong repository.
    let source = crate::project_management::pm_project_list(state.clone())
        .await?
        .iter()
        .find(|item| item.key == ticket.project)
        .map(crate::project_management::local_source_path)
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| format!("project {} has no local repo path set", ticket.project))?;
    let worktree = worktree_for_branch(std::path::Path::new(&source), &branch)
        .ok_or_else(|| format!("no worktree for {branch}; run the PoC first"))?;

    let (doc_rel, doc_text) = latest_poc_doc(&settings.core_team.decision_project, &slug)
        .ok_or_else(|| format!("no PoC document for {slug} in the vault"))?;
    let credited = credited_files(&worktree, &branch);
    let gaps = poc_gaps(&doc_text, &credited);
    if !gaps.is_empty() {
        let note = format!(
            "\n\n{RETURNED_HEADING} {}\n\nThe PoC document was not judged:\n{}\n",
            &chrono::Utc::now().to_rfc3339()[..10],
            gaps.iter()
                .map(|gap| format!("- {gap}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        append_to_ticket(&repo, &ticket, &note)?;
        return Ok(JudgeResult {
            ticket: ticket.id,
            slug,
            decision: "returned".to_string(),
            why: gaps.join("; "),
            returned: gaps,
            plan_inbox_id: String::new(),
        });
    }

    let policy = crate::jury_runtime::policy(&repo, &ticket.project)?;
    let input = judge_input(&ticket, &doc_rel, &doc_text, &credited);
    let job = crate::jury_runtime::new_job(
        crate::jury::Gate::Poc,
        &ticket,
        &worktree,
        input,
        policy,
        None,
        None,
    )?;
    let registry = crate::agents::registry_dir()?;
    let root = crate::jury_runtime::store()?;
    let decided = {
        let repo = repo.clone();
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::jury_runtime::run_job(Some(&app), &repo, &registry, &root, job, None)
        })
        .await
        .map_err(|error| error.to_string())??
    };

    let approved = decided.decision == Some(crate::jury::Decision::Approved);
    let against = decided
        .reviews
        .iter()
        .filter_map(|record| record.review.as_ref())
        .filter(|review| review.decision != "approved")
        .flat_map(|review| review.reasons.clone())
        .collect::<Vec<_>>()
        .join("; ");
    let decision = match decided.decision {
        Some(crate::jury::Decision::Approved) => VERDICT_IN,
        Some(crate::jury::Decision::Owner) => VERDICT_OWNER,
        _ => VERDICT_OUT,
    };
    // The decision log is keyed by project, and the question "should this go
    // into xNAUT" is xNAUT's, not the CORE board's. The acceptance criterion
    // asks for the verdict in the Decisions pane FOR XNAUT, so that is where
    // it is written.
    crate::decisions::append(
        &settings.core_team.decision_project,
        &verdict_decision(&slug, decision, &decided.reason, &against),
    );

    let mut plan_inbox_id = String::new();
    if approved {
        plan_inbox_id = to_plan_canvas(&app, &worktree, &slug, &ticket, &doc_text)?;
    }
    let note = format!(
        "\n\n{JUDGED_HEADING} {} — {decision}\n\n{}\n",
        &chrono::Utc::now().to_rfc3339()[..10],
        decided.reason
    );
    append_to_ticket(&repo, &ticket, &note)?;
    if decision == VERDICT_OUT {
        close_finding(&repo, &ticket.id)?;
    }
    Ok(JudgeResult {
        ticket: ticket.id,
        slug,
        decision: decision.to_string(),
        why: decided.reason,
        returned: Vec::new(),
        plan_inbox_id,
    })
}

/// Put an approved PoC on the Plan Canvas and stop there.
///
/// Deliberately not `jury_runtime::plan`: that opens the PLAN gate, which
/// would put the same proposal in front of two more reviewers who have just
/// approved it. The Judge has decided; what is left is the owner's click, so
/// this writes the plan into the worktree and posts the approve item the pane
/// opens on, which is exactly what `plan_review::handle_review` does after its
/// own gate.
fn to_plan_canvas(
    app: &tauri::AppHandle,
    worktree: &std::path::Path,
    slug: &str,
    ticket: &TicketRecord,
    doc_text: &str,
) -> Result<String, String> {
    use tauri::Emitter;
    let file = "PLAN.md";
    let plan = format!(
        "# Take {slug} in?\n\nThe core team judged this PoC IN. Nothing has merged: approving here is what puts it on the board.\n\nFinding: {} — {}\n\n{doc_text}\n",
        ticket.id, ticket.title
    );
    let plan_path = worktree.join(file);
    std::fs::write(&plan_path, &plan).map_err(|error| format!("write plan: {error}"))?;
    let item = crate::inbox::create_and_announce(
        app,
        "approve",
        crate::inbox::PostRequest {
            project: worktree.to_string_lossy().to_string(),
            from: "core-team".to_string(),
            ticket: Some(ticket.id.clone()),
            title: format!("Take {slug} in?"),
            body: String::new(),
            context: std::collections::BTreeMap::from([
                (
                    "plan_project".to_string(),
                    worktree.to_string_lossy().to_string(),
                ),
                ("plan_file".to_string(), file.to_string()),
            ]),
            ..Default::default()
        },
        None,
    )?;
    let _ = app.emit(
        "plan-review",
        serde_json::json!({
            "id": item.id,
            "project": worktree.to_string_lossy(),
            "planPath": plan_path.to_string_lossy(),
            "title": item.title,
        }),
    );
    Ok(item.id)
}

/// The worktree a branch is checked out in, read off the repository itself.
///
/// `git worktree list` from the SOURCE REPO, not from the process's working
/// directory: the app's cwd is wherever it was launched from, which on a
/// bundled macOS app is `/`. Parsed rather than guessed from a path convention
/// because the PoC run may have been given any path, and the repository is the
/// only thing that knows where it actually went.
pub fn worktree_for_branch(repo: &std::path::Path, branch: &str) -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    parse_worktree_list(&text, branch)
}

/// The path `git worktree list --porcelain` gives for a branch.
///
/// Its records are blank-line separated and the `worktree` line comes first,
/// so the path is carried forward until a `branch` line claims it. Refs arrive
/// as `refs/heads/<branch>`; a bare name is accepted too because older gits
/// print one.
fn parse_worktree_list(text: &str, branch: &str) -> Option<std::path::PathBuf> {
    let mut path: Option<std::path::PathBuf> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            path = Some(std::path::PathBuf::from(rest.trim()));
        } else if let Some(rest) = line.strip_prefix("branch ") {
            let reference = rest.trim();
            if reference == branch || reference == format!("refs/heads/{branch}") {
                return path;
            }
        }
    }
    None
}

/// The newest PoC document for a slug, as (vault-relative path, text).
fn latest_poc_doc(vault_project: &str, slug: &str) -> Option<(String, String)> {
    let vault = crate::vault::vault_init().ok()?;
    let dir = std::path::Path::new(&vault)
        .join("work")
        .join(vault_project)
        .join("Development")
        .join("poc");
    let mut hits: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(&format!("_{slug}.md")))
        })
        .collect();
    hits.sort();
    let path = hits.pop()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let rel = format!(
        "Development/poc/{}",
        path.file_name()?.to_str()?
    );
    Some((rel, text))
}

/// Files changed on the PoC branch that carry a credit header.
fn credited_files(worktree: &std::path::Path, branch: &str) -> Vec<String> {
    let base = crate::jury_runtime::policy_integration_branch();
    let output = std::process::Command::new("git")
        .args([
            "diff",
            "--name-only",
            &format!("{base}...{branch}"),
        ])
        .current_dir(worktree)
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .filter(|path| {
            std::fs::read_to_string(worktree.join(path))
                .map(|text| has_credit_header(&text))
                .unwrap_or(false)
        })
        .map(str::to_string)
        .collect()
}

fn append_to_ticket(
    repo: &std::path::Path,
    ticket: &TicketRecord,
    note: &str,
) -> Result<(), String> {
    crate::project_management::ticket_update_in(
        repo,
        crate::project_management::TicketUpdateRequest {
            model_requirement: None,
            caller: None,
            id: ticket.id.clone(),
            expected_revision: ticket.revision,
            title: None,
            ticket_type: None,
            status: None,
            priority: None,
            owner: None,
            clear_owner: false,
            documentation: None,
            body: Some(format!("{}{note}", ticket.body)),
        },
    )
    .map(|_| ())
}

/// Close a finding the Judge sent out, with the reason on it.
///
/// `done`, never `complete`: `complete` is NautBot's word for tested and
/// approved, and a finding nobody is going to build has not been tested.
fn close_finding(repo: &std::path::Path, id: &str) -> Result<(), String> {
    let ticket = crate::project_management::ticket_list_in(repo, None)?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("ticket {id} not found"))?;
    crate::project_management::ticket_update_in(
        repo,
        crate::project_management::TicketUpdateRequest {
            model_requirement: None,
            caller: None,
            id: ticket.id.clone(),
            expected_revision: ticket.revision,
            title: None,
            ticket_type: None,
            status: Some("done".to_string()),
            priority: None,
            owner: None,
            clear_owner: false,
            documentation: None,
            body: None,
        },
    )
    .map(|_| ())
}

/// The prompt a PoC run is dispatched with, appended to the ticket body so
/// `dispatch.rs` carries it unchanged.
///
/// Wall clock and the doc path are NUMBERS and PATHS rather than adjectives,
/// the same form the dispatch prompt settled on: "keep it small" produces a
/// subsystem, "at most 90 minutes, one module" produces a prototype.
pub fn poc_brief(finding: &Finding, slug: &str, minutes: u64) -> String {
    format!(
        "\n\n## PoC brief\n\n\
         Build a prototype of ONE mechanism from {} on the branch `{}`, in this worktree.\n\n\
         - Read `{}` first. That is the file the Researcher named.\n\
         - Reimplement, never copy. The file you write carries a credit header naming the \
           project, the file or symbol, and the licence ({}), and says where you departed \
           from the original and why.\n\
         - At most {minutes} minutes of wall clock. Over that, stop, write \"{TOO_BIG}\" on \
           the ticket with what you had reached, and leave the branch where it is.\n\
         - Write up `{}` from the template below, keeping all six headings. The \
           Measurement is a NUMBER you measured here, against the same number without \
           the change; an impression is returned unjudged.\n\
         - Nothing merges. The Judge reads the document and the owner decides.\n\n\
         ```markdown\n{}```\n",
        finding.repo_url.trim(),
        poc_branch(slug),
        finding.file.trim(),
        finding.licence.trim(),
        poc_doc_rel("YYYY-MM-DD", slug),
        poc_template(finding, slug),
    )
}

/// The PoC brief for a ticket, or nothing when the ticket is not one.
///
/// `dispatch.rs` asks this of every ticket it launches, so a PoC agent gets
/// the credit rule, the budget and the document contract in the same prompt
/// as everything else, and an ordinary ticket gets not one extra word.
pub fn poc_brief_for(ticket: &TicketRecord, minutes: u64) -> String {
    if poc_branch_for(ticket).is_none() {
        return String::new();
    }
    let finding = parse_finding(&ticket.body);
    let slug = slug_for(&finding);
    poc_brief(&finding, &slug, minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding() -> Finding {
        Finding {
            repo_url: "https://github.com/acme/loop-runner".into(),
            name: "acme/loop-runner".into(),
            stars: 1200,
            licence: "MIT".into(),
            last_commit: "2026-09-01".into(),
            does: "Runs an agent loop with a budget and stops on a plateau.".into(),
            touches: "loops.rs, plateau.rs".into(),
            file: "src/runner/budget.py".into(),
        }
    }

    fn ticket(body: &str, kind: &str, tags: &[&str]) -> TicketRecord {
        let mut record: TicketRecord = serde_json::from_value(serde_json::json!({
            "id": "CORE-1", "project": "CORE", "title": "t", "type": kind,
            "status": "inbox", "priority": "low", "owner": null, "documentation": [],
            "body": body, "source_id": "", "revision": 1, "created_at": "", "updated_at": ""
        }))
        .expect("fixture ticket");
        record.tags = tags.iter().map(|t| t.to_string()).collect();
        record
    }

    /// The body on the board and the record in code are the same thing. If
    /// this ever drifts, the Reviewer scores a finding with empty fields and
    /// nothing errors — the silent-wrong failure this codebase keeps meeting.
    #[test]
    fn a_finding_survives_the_trip_through_a_ticket_body() {
        let original = finding();
        let read_back = parse_finding(&original.body());
        assert_eq!(read_back, original);
    }

    /// A finding ticket grows review blocks, dispatch notes and a verdict
    /// underneath. Parsing must survive all of it.
    #[test]
    fn later_sections_never_disturb_the_finding() {
        let body = format!(
            "{}\n\n## Weighed 2026-09-14 by @reviewer\n\n- score: 80/100\n\n## Dispatched\n\n- branch `poc/x`\n",
            finding().body()
        );
        let read_back = parse_finding(&body);
        assert_eq!(read_back.repo_url, finding().repo_url);
        assert_eq!(read_back.does, finding().does);
        assert_eq!(read_back.touches, finding().touches);
    }

    /// The acceptance criterion: no duplicates for a repo seen before. Four
    /// spellings of one repository are one repository.
    #[test]
    fn one_repository_has_one_key_however_it_is_spelled() {
        let canonical = "github.com/acme/loop-runner";
        for spelling in [
            "https://github.com/Acme/Loop-Runner",
            "http://www.github.com/acme/loop-runner/",
            "git@github.com:acme/loop-runner.git",
            "github.com/acme/loop-runner?tab=readme-ov-file",
            "https://github.com/acme/loop-runner#install",
        ] {
            assert_eq!(normalise_repo(spelling), canonical, "{spelling}");
        }
    }

    #[test]
    fn a_repo_already_on_the_board_is_skipped_and_said_so() {
        let board = vec![ticket(&finding().body(), FINDING_TYPE, &[])];
        let (keep, skipped) = triage(vec![finding()], &seen_repos(&board));
        assert!(keep.is_empty());
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].reason, "already seen");
    }

    /// A duplicate inside one answer is a duplicate the moment the second
    /// ticket is written, not next week.
    #[test]
    fn a_batch_dedupes_against_itself_too() {
        let mut second = finding();
        second.repo_url = "git@github.com:acme/loop-runner.git".into();
        let (keep, skipped) = triage(vec![finding(), second], &[]);
        assert_eq!(keep.len(), 1);
        assert_eq!(skipped.len(), 1);
    }

    /// "licence and source file filled" is the acceptance criterion, so a
    /// finding without them is refused at the door and the skip names which.
    #[test]
    fn a_finding_with_no_licence_or_file_never_reaches_the_board() {
        let mut bare = finding();
        bare.licence = String::new();
        bare.file = "   ".into();
        let (keep, skipped) = triage(vec![bare], &[]);
        assert!(keep.is_empty());
        assert_eq!(skipped[0].reason, "no licence, no file");
    }

    #[test]
    fn the_weights_add_up_and_a_perfect_rubric_is_a_hundred() {
        assert_eq!(WEIGHTS.iter().map(|(_, w)| w).sum::<u32>(), 100);
        let perfect = Rubric { fit: 5, novelty: 5, port_size: 5, licence: 5, activity: 5 };
        assert_eq!(perfect.score(), 100);
        assert_eq!(Rubric::default().score(), 0);
    }

    /// A model that answers `fit: 9` has misread the scale, not scored a nine.
    /// Letting it through puts a finding over the threshold on arithmetic
    /// nobody chose.
    #[test]
    fn a_dimension_out_of_range_is_clamped_not_believed() {
        let wild = Rubric { fit: 9, novelty: 200, port_size: 5, licence: 5, activity: 5 };
        assert_eq!(wild.score(), 100);
    }

    /// Decision 3 in the module header: the licence is a gate. A finding that
    /// scores 100 on a licence we cannot name is still refused.
    #[test]
    fn a_licence_we_cannot_name_is_refused_at_any_score() {
        let perfect = Rubric { fit: 5, novelty: 5, port_size: 5, licence: 5, activity: 5 };
        for licence in ["GPL-3.0", "AGPL-3.0", "", "NOASSERTION", "custom terms"] {
            let mut candidate = finding();
            candidate.licence = licence.into();
            let review = weigh(&candidate, perfect, 60);
            assert_eq!(review.verdict, ReviewVerdict::Refused, "{licence}");
            assert_eq!(review.score, 100, "the score is still reported");
            assert_eq!(review.tag(), SHELVED_TAG, "{licence}");
        }
    }

    #[test]
    fn the_threshold_is_the_cost_knob() {
        let rubric = Rubric { fit: 4, novelty: 4, port_size: 3, licence: 5, activity: 3 };
        let score = rubric.score();
        assert_eq!(weigh(&finding(), rubric, score).verdict, ReviewVerdict::Poc);
        assert_eq!(
            weigh(&finding(), rubric, score + 1).verdict,
            ReviewVerdict::Shelved
        );
    }

    #[test]
    fn licences_are_classified_by_what_they_let_us_do() {
        assert_eq!(licence_class("Apache-2.0"), Licence::Permissive);
        assert_eq!(licence_class("mit"), Licence::Permissive);
        assert_eq!(licence_class("BSD-3-Clause"), Licence::Permissive);
        assert_eq!(licence_class("GPL-3.0-only"), Licence::Copyleft);
        assert_eq!(licence_class("AGPL-3.0"), Licence::Copyleft);
        assert_eq!(licence_class(""), Licence::Unknown);
        assert_eq!(licence_class("see LICENSE"), Licence::Unknown);
    }

    /// A search model answers in prose, in a fence, or bare. All three days
    /// have to produce findings.
    #[test]
    fn findings_come_out_of_whatever_the_model_wrapped_them_in() {
        let bare = r#"[{"repo_url":"https://github.com/a/b","licence":"MIT","file":"x.py","does":"d"}]"#;
        assert_eq!(parse_findings(bare).len(), 1);
        let fenced = format!("Here is what I found:\n\n```json\n{bare}\n```\n\nHope that helps.");
        assert_eq!(parse_findings(&fenced).len(), 1);
        // A bracket inside a string must not close the array early.
        let tricky = r#"[{"repo_url":"https://github.com/a/b","does":"an array ] in prose","licence":"MIT","file":"x"}]"#;
        assert_eq!(parse_findings(tricky).len(), 1);
        assert_eq!(parse_findings("nothing structured here").len(), 0);
    }

    /// Stars arrive as a number, a string or null depending on the model, and
    /// none of those is a reason to drop a finding.
    #[test]
    fn a_star_count_in_any_shape_never_costs_a_finding() {
        let answer = r#"[
          {"url":"https://github.com/a/b","license":"MIT","source_file":"x.py","summary":"d","stars":"1200"},
          {"url":"https://github.com/c/d","license":"MIT","source_file":"y.py","summary":"d","stars":null}
        ]"#;
        let found = parse_findings(answer);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].stars, 1200);
        assert_eq!(found[1].stars, 0);
        // The aliases are the point: `license`, `url` and `summary` all landed.
        assert_eq!(found[0].licence, "MIT");
        assert_eq!(found[0].file, "x.py");
        assert_eq!(found[0].does, "d");
    }

    #[test]
    fn the_scan_prompt_carries_the_topics_and_what_not_to_return() {
        let prompt = scan_prompt(
            &["agent harnesses".into(), "".into()],
            &["github.com/a/b".to_string()],
        );
        assert!(prompt.contains("agent harnesses"));
        assert!(prompt.contains("do not return these"));
        assert!(prompt.contains("github.com/a/b"));
        assert!(!prompt.contains(", ,"), "an empty topic left a hole: {prompt}");
        // No exclusion list, no heading for one.
        assert!(!scan_prompt(&["x".into()], &[]).contains("do not return these"));
    }

    #[test]
    fn a_rubric_that_did_not_parse_is_an_error_not_a_zero() {
        let (rubric, why) =
            parse_rubric(r#"Sure! {"fit":4,"novelty":3,"port_size":5,"licence":5,"activity":2,"why":"small"}"#)
                .expect("a wrapped object still parses");
        assert_eq!(rubric.fit, 4);
        assert_eq!(why, "small");
        assert!(parse_rubric("I could not decide").is_err());
    }

    /// The acceptance criterion, in one test: a PoC doc with no credit line is
    /// RETURNED, and the return names every gap at once rather than one per
    /// round trip.
    #[test]
    fn an_uncredited_poc_is_returned_with_every_gap_named() {
        let doc = poc_template(&finding(), "loop-runner");
        let gaps = poc_gaps(&doc, &[]);
        assert!(gaps.iter().any(|gap| gap.contains("credit header")), "{gaps:?}");
        assert!(gaps.iter().any(|gap| gap.contains("no departures")), "{gaps:?}");
        assert!(gaps.iter().any(|gap| gap.contains("no measurement")), "{gaps:?}");
        assert!(gaps.iter().any(|gap| gap.contains("no verdict")), "{gaps:?}");
        // Source, file and licence were filled in by the template from the
        // finding, so they are NOT gaps.
        assert!(!gaps.iter().any(|gap| gap.contains("no source")), "{gaps:?}");
    }

    #[test]
    fn a_complete_poc_document_is_admissible() {
        let doc = "## Source\n\nacme/loop-runner\n\n## File\n\nsrc/runner/budget.py\n\n## Licence\n\nMIT\n\n## Departures\n\nWe key the budget per project.\n\n## Measurement\n\n41 runs before, 12 after.\n\n## Verdict\n\nin: it stops the plateau.\n";
        let read = read_poc_doc(doc).expect("a filled document");
        assert_eq!(read.licence, "MIT");
        assert!(poc_gaps(doc, &["src-tauri/src/x.rs".to_string()]).is_empty());
    }

    /// "noticeably faster" passed the section check on every draft it was
    /// tried on. A measurement with no number is an impression wearing a
    /// heading.
    #[test]
    fn a_measurement_with_no_number_is_not_a_measurement() {
        let doc = "## Source\n\nx\n\n## File\n\ny\n\n## Licence\n\nMIT\n\n## Departures\n\nz\n\n## Measurement\n\nNoticeably faster.\n\n## Verdict\n\nin\n";
        let gaps = poc_gaps(doc, &["a.rs".to_string()]);
        assert_eq!(gaps, vec!["the measurement has no number in it".to_string()]);
    }

    /// The credit form is this repository's own. If the check ever stops
    /// matching the headers we actually write, every PoC is returned forever
    /// and the loop dies silently.
    #[test]
    fn the_credit_check_matches_the_headers_this_repo_already_writes() {
        for header in [
            "// Ported from CORAL (Apache 2.0), `coral/agent/heartbeat.py::streak_for_epsilon`.",
            "// Borrowed from ECC (github.com/affaan-m/ecc, MIT),",
            "// Idea from cdknorow/coral (Apache 2.0), `internal/background/token_poller.go`,",
            "// Adapted from xNAUT (MIT), XNAUT-277 commit 18b7632,",
            "//! Taken from somewhere (MIT), `a/b.rs`.",
        ] {
            assert!(has_credit_header(&format!("{header}\n\nuse std::fs;")), "{header}");
        }
        assert!(!has_credit_header("// A perfectly ordinary file.\n"));
        // Named but not sourced is not a credit.
        assert!(!has_credit_header("// Ported from CORAL.\n"));
        // Buried 80 lines down is not a header.
        assert!(!has_credit_header(&format!(
            "{}// Ported from CORAL (MIT), `a.py`.\n",
            "// filler\n".repeat(60)
        )));
    }

    /// The branch on the card and the branch in the repo are one spelling, and
    /// only a finding that was actually tagged for a PoC gets one.
    #[test]
    fn only_a_tagged_finding_has_a_poc_branch() {
        let body = finding().body();
        assert_eq!(
            poc_branch_for(&ticket(&body, FINDING_TYPE, &[POC_TAG])).as_deref(),
            Some("poc/acme-loop-runner")
        );
        assert_eq!(poc_branch_for(&ticket(&body, FINDING_TYPE, &[])), None);
        assert_eq!(poc_branch_for(&ticket(&body, "feature", &[POC_TAG])), None);
        // A finding with no repo in it cannot name a branch.
        assert_eq!(poc_branch_for(&ticket("nothing", FINDING_TYPE, &[POC_TAG])), None);
    }

    /// Two projects called `agent-loop` under different owners are two
    /// projects. A slug that dropped the owner would give them one branch and
    /// one vault document.
    #[test]
    fn a_nameless_finding_still_slugs_by_owner_and_name() {
        let mut nameless = finding();
        nameless.name = String::new();
        assert_eq!(nameless.display_name(), "acme/loop-runner");
        assert_eq!(slug_for(&nameless), "acme-loop-runner");
        let mut other = nameless.clone();
        other.repo_url = "https://github.com/other/loop-runner".into();
        assert_ne!(slug_for(&other), slug_for(&nameless));
    }

    #[test]
    fn a_slug_is_short_safe_and_never_empty() {
        assert_eq!(slug_for(&finding()), "acme-loop-runner");
        let mut odd = finding();
        odd.name = "Weird///Name!!!".into();
        assert_eq!(slug_for(&odd), "weird-name");
        odd.name = "!!!".into();
        odd.repo_url = "!!!".into();
        assert_eq!(slug_for(&odd), "poc");
        odd.name = "a".repeat(90);
        assert!(slug_for(&odd).len() <= 40);
    }

    /// The Judge has to find the worktree the PoC was built in. Reading it off
    /// the repository beats guessing a path convention, and this is the format
    /// it answers in — including the detached record, which has no branch line
    /// at all and must not hand back the previous entry's path.
    #[test]
    fn the_worktree_is_read_off_the_repository_not_guessed() {
        let listing = "\
worktree /Users/a/xnaut
HEAD abc123
branch refs/heads/dev

worktree /Users/a/detached
HEAD def456
detached

worktree /Users/a/worktrees/poc-loop-runner
HEAD 999aaa
branch refs/heads/poc/acme-loop-runner
";
        assert_eq!(
            parse_worktree_list(listing, "poc/acme-loop-runner"),
            Some(std::path::PathBuf::from("/Users/a/worktrees/poc-loop-runner"))
        );
        assert_eq!(
            parse_worktree_list(listing, "dev"),
            Some(std::path::PathBuf::from("/Users/a/xnaut"))
        );
        assert_eq!(parse_worktree_list(listing, "poc/nothing"), None);
        // A prefix is not a branch: `dev` must not answer for `dev-2`.
        assert_eq!(parse_worktree_list(listing, "ev"), None);
        assert_eq!(parse_worktree_list("", "dev"), None);
    }

    #[test]
    fn the_poc_doc_goes_where_the_ticket_says() {
        assert_eq!(
            poc_doc_rel("2026-09-14", "acme-loop-runner"),
            "Development/poc/2026-09-14_acme-loop-runner.md"
        );
    }

    /// The last edge of the loop. Without it a finished prototype sits in
    /// `done` forever because judging it was something a person had to
    /// remember to ask for.
    #[test]
    fn a_finished_poc_is_judged_once_and_never_twice() {
        let body = finding().body();
        let poc = |status: &str, extra: &str| {
            let mut record = ticket(&format!("{body}{extra}"), FINDING_TYPE, &[POC_TAG]);
            record.status = status.into();
            record
        };
        let mut done = poc("done", "");
        done.id = "CORE-2".into();
        let board = vec![
            done,
            poc("review", ""),
            // Still building.
            poc("in_progress", ""),
            // Already answered, both ways.
            poc("done", "\n\n## Judged 2026-09-14 — in\n"),
            poc("done", "\n\n## Returned 2026-09-14\n"),
            // Weighed and shelved: never had a PoC to judge.
            {
                let mut shelved = ticket(&body, FINDING_TYPE, &[SHELVED_TAG]);
                shelved.status = "done".into();
                shelved
            },
            // An ordinary finished ticket is somebody else's business.
            {
                let mut ordinary = ticket("", "feature", &[POC_TAG]);
                ordinary.status = "done".into();
                ordinary
            },
        ];
        let judgeable = judgeable(&board);
        assert_eq!(judgeable.len(), 2, "{:?}", judgeable.iter().map(|t| &t.id).collect::<Vec<_>>());
        assert_eq!(judgeable[0].id, "CORE-2");
        // The headings the filter reads are the ones the judge actually writes.
        assert!(JUDGED_HEADING.starts_with("## "));
        assert!(RETURNED_HEADING.starts_with("## "));
    }

    /// The budget is the app's clock, not the model's. A brief saying "at most
    /// 90 minutes" is an instruction, and a model three hours into a port is
    /// not the thing to ask whether it should stop.
    #[test]
    fn only_a_live_poc_run_past_its_clock_is_over_budget() {
        use crate::run_control::{RunKind, RunManifest, RunState};
        let run = |branch: &str, kind: RunKind, state: RunState, age_minutes: i64| {
            let mut manifest = RunManifest::requested(
                "claude", "claude", "/tmp/w", Some("CORE-1".into()), None, &[], 0,
            );
            manifest.branch = branch.into();
            manifest.kind = kind;
            manifest.state = state;
            manifest.started_at = NOW - age_minutes * 60_000;
            manifest
        };
        // A real epoch-ms clock: minutes subtracted from a toy base go negative,
        // and a negative start time is indistinguishable from an unset one.
        const NOW: i64 = 1_789_000_000_000;
        let runs = vec![
            run("poc/acme-loop-runner", RunKind::Agent, RunState::Running, 120),
            // Inside the clock.
            run("poc/young", RunKind::Agent, RunState::Running, 30),
            // Not a PoC.
            run("agent/claude/xnaut-1", RunKind::Agent, RunState::Running, 120),
            // Already finished: stopping it again buys nothing.
            run("poc/done", RunKind::Agent, RunState::Done, 120),
            // A reviewer is not a PoC run even on a poc branch.
            run("poc/review", RunKind::Review, RunState::Running, 120),
        ];
        let over = over_budget(&runs, NOW, 90);
        assert_eq!(over.len(), 1, "{:?}", over.iter().map(|r| &r.branch).collect::<Vec<_>>());
        assert_eq!(over[0].branch, "poc/acme-loop-runner");
        // A run with no start time cannot be timed, and guessing would kill it.
        let mut undated = runs;
        undated[0].started_at = 0;
        assert!(over_budget(&undated, NOW, 90).is_empty());
    }

    #[test]
    fn the_over_budget_note_carries_the_phrase_the_ticket_asks_for() {
        let note = too_big_note(90, "poc/acme-loop-runner");
        assert!(note.contains(TOO_BIG));
        assert!(note.contains("90-minute"));
        assert!(note.contains("poc/acme-loop-runner"));
        assert!(is_poc_run("poc/x"));
        assert!(!is_poc_run("agent/claude/xnaut-1"));
    }

    /// A beat is a rhythm, not an appointment: a machine asleep on Monday
    /// scans when it wakes.
    #[test]
    fn the_beat_is_due_when_a_week_has_passed_and_never_before() {
        let day = 24 * 60 * 60 * 1000i64;
        assert!(beat_due(0, 1_000, 7), "never run is due");
        assert!(!beat_due(1_000, 1_000 + 6 * day, 7));
        assert!(beat_due(1_000, 1_000 + 7 * day, 7));
        // A zero interval must not make every tick a beat.
        assert!(!beat_due(1_000, 1_000 + day / 2, 0));
    }

    /// The verdict is the first thing in this codebase to write
    /// `council.verdict`, and the Decisions pane reads that string.
    #[test]
    fn the_verdict_is_written_at_the_boundary_the_pane_reads() {
        let decision = verdict_decision("loop-runner", VERDICT_IN, "both reviewers approved", "too big");
        assert_eq!(decision.boundary, VERDICT_BOUNDARY);
        assert!(
            crate::decisions::BOUNDARIES.contains(&VERDICT_BOUNDARY),
            "the boundary must be one the brief knows"
        );
        assert_eq!(decision.what, "PoC loop-runner: in");
        assert_eq!(decision.alternatives, "too big", "the con side is kept");
        assert!(!decision.open, "a verdict ends the argument");
        assert_eq!(verdict_decision("x", VERDICT_OUT, "", "").what, "PoC x: out");
        // An escalation is not a rejection. It stays OPEN, so the brief cannot
        // report a live question as a finished one, and it carries the key a
        // later verdict closes it with.
        let escalated = verdict_decision("x", VERDICT_OWNER, "reviewers disagree", "");
        assert!(escalated.open, "an escalation is unresolved");
        assert_eq!(escalated.key, "poc:x");
        assert!(!verdict_decision("x", VERDICT_OUT, "", "").open);
    }

    /// The PoC brief carries numbers and paths, not adjectives, and it names
    /// the exact phrase the acceptance criterion asks for on an over-budget run.
    #[test]
    fn the_poc_brief_states_the_budget_the_credit_and_the_measurement() {
        let brief = poc_brief(&finding(), "acme-loop-runner", 90);
        assert!(brief.contains("poc/acme-loop-runner"));
        assert!(brief.contains("src/runner/budget.py"));
        assert!(brief.contains("MIT"));
        assert!(brief.contains("At most 90 minutes"));
        assert!(brief.contains(TOO_BIG));
        assert!(brief.contains("Development/poc/YYYY-MM-DD_acme-loop-runner.md"));
        assert!(brief.contains("Reimplement, never copy"));
        assert!(brief.contains("Nothing merges"));
        // The template travels WITH the brief. Naming six headings and leaving
        // the agent to guess their spelling is how a document comes back with
        // "## Measurements" and is returned for having no measurement.
        for heading in POC_SECTIONS {
            assert!(brief.contains(&format!("## {heading}")), "{heading}");
        }
    }

    /// The Reviewer exists to be a SECOND opinion. On the Researcher's own
    /// provider it is the same weights reading back its own answer, which is
    /// the exact failure that kept Council out of NautFlow.
    #[test]
    fn a_reviewer_on_the_researchers_provider_blocks_the_team() {
        use crate::settings::{LlmProviderSettings, Settings};
        let profile = |handle: &str, role: &str, provider: &str| crate::agent_profiles::AgentProfile {
            handle: handle.into(),
            display_name: handle.into(),
            tagline: String::new(),
            purpose: String::new(),
            runtime_id: "claude".into(),
            provider: provider.into(),
            model: "m".into(),
            chat_provider: String::new(),
            chat_model: String::new(),
            reasoning_effort: String::new(),
            max_parallel: 0,
            execution: crate::agent_profiles::AgentExecution::Local,
            role: role.into(),
            capabilities: vec![],
            notifications: true,
            accent_color: "#fff".into(),
            policy: crate::policy::AgentPolicy::default(),
            default_project: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let settings = Settings {
            core_team: crate::settings::CoreTeamSettings {
                enabled: true,
                ..crate::settings::CoreTeamSettings::default()
            },
            llm_providers: vec![
                LlmProviderSettings {
                    name: "perplexity".into(),
                    endpoint: "https://api.perplexity.ai".into(),
                    api_key: Some("k".into()),
                    enabled: true,
                },
                LlmProviderSettings {
                    name: "anthropic".into(),
                    endpoint: "https://api.anthropic.com/v1".into(),
                    api_key: Some("k".into()),
                    enabled: true,
                },
            ],
            ..Settings::default()
        };
        let same = status_from(
            &settings,
            &[
                profile("researcher", "researcher", "perplexity"),
                profile("reviewer", "reviewer", "perplexity"),
            ],
            0,
        );
        assert!(
            same.blocked.iter().any(|line| line.contains("already used")),
            "{:?}",
            same.blocked
        );
        let split = status_from(
            &settings,
            &[
                profile("researcher", "researcher", "perplexity"),
                profile("reviewer", "reviewer", "anthropic"),
            ],
            0,
        );
        assert!(split.blocked.is_empty(), "{:?}", split.blocked);
        assert_eq!(split.reviewer_provider, "anthropic");
    }

    /// The board has to be able to file a finding, or the Researcher's whole
    /// output is refused by the type validator and the loop dies at step one.
    #[test]
    fn the_board_knows_what_a_finding_is_everywhere_it_is_asked() {
        assert!(crate::project_management::TICKET_TYPES.contains(&FINDING_TYPE));
        // The panel's dropdown is the same list. Two copies is how a type the
        // backend accepts becomes one the board cannot show.
        let panel = include_str!("../../src/js/project-management-panel.js");
        assert!(
            panel.contains("'task', 'finding']"),
            "the board's type list dropped `finding`"
        );
    }

    /// An exported global nothing calls is a feature that exists in the source
    /// and nowhere else (XNAUT-189, and again with the roster panel's missing
    /// `window.xnautActiveProjectKey`). Both halves are pinned here.
    #[test]
    fn the_core_team_pane_is_exported_and_actually_called() {
        let pane = include_str!("../../src/js/core-team-settings.js");
        assert!(pane.contains("window.xnautRenderCoreTeamSettings = async"));
        let app = include_str!("../../src/js/app.js");
        assert!(
            app.contains("window.xnautRenderCoreTeamSettings(document.getElementById('core-team-settings-host'))"),
            "the Core Team pane is exported and never rendered"
        );
        assert!(
            app.contains("coreteam: () => `<div id=\"core-team-settings-host\">"),
            "no settings section mounts the pane's host"
        );
        let html = include_str!("../../src/index.html");
        assert!(html.contains("js/core-team-settings.js"), "the pane is never loaded");
        assert!(
            html.contains("data-section=\"coreteam\""),
            "nothing in the nav rail reaches the pane"
        );
    }

    /// The commands exist in all three places a Tauri command has to exist, or
    /// the frontend gets "not allowed by ACL" at runtime with nothing at
    /// compile time to say so.
    #[test]
    fn every_core_team_command_is_registered_and_permitted() {
        let main = include_str!("main.rs");
        let permissions = include_str!("../permissions/default.toml");
        // Only the two the frontend calls. `core_team_review` and
        // `core_team_judge` are deliberately NOT commands: the scan and the
        // sweep are their only callers, and an ACL entry nothing in the
        // frontend invokes is surface with no user.
        for command in ["core_team_status", "core_team_scan"] {
            assert!(
                main.contains(&format!("core_team::{command},")),
                "{command} is not in the invoke handler"
            );
            assert!(
                permissions.contains(&format!("\"{command}\"")),
                "{command} is not in permissions/default.toml"
            );
        }
    }

    /// The switch governs the unattended beat. A "Scan now" the owner pressed
    /// is not the beat, and a button that can never work until you have
    /// already agreed to the weekly run is a button nobody can try first.
    #[test]
    fn the_switch_stops_the_beat_and_never_a_scan_the_owner_asked_for() {
        let status = status_from(&crate::settings::Settings::default(), &[], 0);
        assert!(status.blocked.contains(&OFF_REASON.to_string()));
        let owner_pressed = status.blockers_the_owner_cannot_override();
        assert!(
            !owner_pressed.contains(&OFF_REASON.to_string()),
            "the switch must not refuse a manual scan"
        );
        // Everything else still does: no researcher is no scan, switch or not.
        assert!(
            owner_pressed.iter().any(|line| line.contains("Researcher")),
            "{owner_pressed:?}"
        );
    }

    /// Off is the default and it says so, because the loop is experimental and
    /// a weekly beat nobody asked for spends money on its own.
    #[test]
    fn the_core_team_is_off_until_someone_turns_it_on() {
        let settings = crate::settings::Settings::default();
        assert!(!settings.core_team.enabled);
        let status = status_from(&settings, &[], 0);
        assert!(
            status.blocked.iter().any(|line| line.contains("experimental")),
            "{:?}",
            status.blocked
        );
        assert_eq!(status.project, "CORE");
        assert!(status.threshold > 0 && status.threshold <= 100);
    }
}
