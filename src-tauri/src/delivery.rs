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
use std::path::{Path, PathBuf};

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
    let repo = crate::project_management::repo_now()?;
    let registry = crate::agents::registry_dir()?;
    let vault = crate::vault::vault_root("work").ok();
    join_in(&repo, &registry, &records_dir(), vault.as_deref(), &ticket)
}

/// Where the verify records are. Resolved here rather than borrowed because
/// `sandbox_verify` keeps its own copy private, and the one thing that must
/// stay in step is the path, not the function.
fn records_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XNAUT_VERIFY_DIR") {
        return path.into();
    }
    dirs::config_dir()
        .map(|p| p.join("xnaut").join("sandbox-verify").join("records"))
        .unwrap_or_else(|| PathBuf::from(".xnaut-sandbox-verify"))
}

/// The join, over explicit roots so a test can build all five stores in a
/// temp directory.
///
/// `Err` is reserved for "no such ticket" and for a store that will not read
/// at all. Everything else missing is an empty stage.
fn join_in(
    repo: &Path,
    registry: &Path,
    records: &Path,
    vault: Option<&Path>,
    id: &str,
) -> Result<Lifecycle, String> {
    let tickets = crate::project_management::ticket_list_in(repo, None)?;
    let ticket = tickets
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("ticket {id} not found"))?;

    let plan = plan_job(ticket, &registry.join("jury"));
    let verified = verify_for(records, ticket);
    let verify = verified.as_ref().map(|(record, _)| summarise(record));

    let mut stages: Vec<Stage> = STAGES
        .iter()
        .map(|(key, title)| Stage {
            key: (*key).to_string(),
            title: (*title).to_string(),
            ..Default::default()
        })
        .collect();

    let issue = stage_mut(&mut stages, "issue");
    issue.at = ticket.created_at.clone();
    issue.text = ticket.body.clone();
    issue.items = ticket.documentation.clone();

    if let Some(job) = &plan {
        let (text, paths) = plan_text(&job.input);
        let proposed = stage_mut(&mut stages, "proposed");
        proposed.at = latest_review_at(&job.reviews);
        proposed.text = text;
        proposed.items = paths;
    }

    if let Some(handback) = &ticket.handback {
        let final_stage = stage_mut(&mut stages, "final");
        final_stage.at = handback.submitted_at.clone();
        final_stage.text = handback.summary.clone();
        final_stage.items = handback
            .files_changed
            .iter()
            .cloned()
            .chain(handback.commits.iter().map(|sha| format!("commit {sha}")))
            .collect();
    }

    if let (Some((record, _)), Some(summary)) = (&verified, &verify) {
        let tested = stage_mut(&mut stages, "tested");
        tested.at = record.updated_at.clone();
        tested.text = tested_text(record, summary);
        tested.items = summary.failing.clone();
    }

    if is_done(&ticket.status) {
        let done = stage_mut(&mut stages, "done");
        done.at = done_at(repo, ticket).unwrap_or_else(|| ticket.updated_at.clone());
        done.text = format!("status became {}", ticket.status);
    }

    if let Some(signoff) = &ticket.approval.signoff {
        let job = jury_job(ticket, &registry.join("jury"), &signoff.jury_id);
        let merged = stage_mut(&mut stages, "merged");
        merged.at = latest_review_at(&signoff.reviewers);
        merged.text = merged_text(signoff, job.as_ref());
        merged.items = std::iter::once(signoff.merge_sha.clone())
            .chain(signoff.revert_sha.clone())
            .filter(|sha| !sha.is_empty())
            .collect();
    }

    stage_mut(&mut stages, "learnings").text = learnings(ticket, &tickets, registry, vault);

    Ok(Lifecycle {
        ticket: ticket.id.clone(),
        project: ticket.project.clone(),
        title: ticket.title.clone(),
        kind: ticket.ticket_type.clone(),
        priority: ticket.priority.clone(),
        owner: ticket.owner.clone().unwrap_or_default(),
        branch: branch_for(registry, ticket),
        status: ticket.status.clone(),
        body: ticket.body.clone(),
        stages,
        verify,
        evidence: evidence(verified.as_ref().map(|(_, raw)| raw)),
    })
}

/// The table is a constant, so a key that is not in it is a typo in this file.
fn stage_mut<'a>(stages: &'a mut [Stage], key: &str) -> &'a mut Stage {
    stages
        .iter_mut()
        .find(|stage| stage.key == key)
        .expect("stage key is one of STAGES")
}

fn is_done(status: &str) -> bool {
    matches!(status, "done" | "complete")
}

fn ms_to_rfc3339(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|at| at.to_rfc3339())
        .unwrap_or_default()
}

/// When the reviewers finished is the only timestamp a jury job carries that
/// is a fact; its deadline is a future it may never have reached.
fn latest_review_at(reviews: &[crate::jury::ReviewRecord]) -> String {
    reviews
        .iter()
        .map(|review| review.finished_at)
        .max()
        .filter(|ms| *ms > 0)
        .map(ms_to_rfc3339)
        .unwrap_or_default()
}

/// The plan gate job, preferring the one the jury approved.
///
/// The ticket carries its own copy of every jury job it has been through, and
/// that copy travels with the control repo; the registry store is local to
/// the machine that ran the review, so a ticket reviewed on the other machine
/// has nothing there. The ticket is read first for that reason, and the store
/// only fills in for a job that has not been written back yet.
fn plan_job(
    ticket: &crate::project_management::TicketRecord,
    store: &Path,
) -> Option<crate::jury::Job> {
    let mut jobs: Vec<crate::jury::Job> = ticket
        .approval
        .jury_reviews
        .iter()
        .filter(|job| job.gate == crate::jury::Gate::Plan)
        .cloned()
        .collect();
    if jobs.is_empty() {
        jobs = store_jobs(store, &ticket.id)
            .into_iter()
            .filter(|job| job.gate == crate::jury::Gate::Plan)
            .collect();
    }
    jobs.sort_by_key(|job| {
        (
            job.decision == Some(crate::jury::Decision::Approved),
            job.reviews
                .iter()
                .map(|review| review.finished_at)
                .max()
                .unwrap_or(0),
        )
    });
    jobs.pop()
}

fn jury_job(
    ticket: &crate::project_management::TicketRecord,
    store: &Path,
    id: &str,
) -> Option<crate::jury::Job> {
    ticket
        .approval
        .jury_reviews
        .iter()
        .find(|job| job.id == id)
        .cloned()
        .or_else(|| crate::jury::read_job(store, id).ok())
}

fn store_jobs(store: &Path, ticket: &str) -> Vec<crate::jury::Job> {
    let Ok(entries) = std::fs::read_dir(store) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|entry| std::fs::read(entry.path()).ok())
        .filter_map(|body| serde_json::from_slice::<crate::jury::Job>(&body).ok())
        .filter(|job| job.ticket == ticket)
        .collect()
}

/// The plan out of a jury input, which also carries the ticket snapshot and
/// the spend estimate. Composed in `jury_runtime::open`; parsed back here
/// because the plan itself is what a reader wants, not the packet around it.
fn plan_text(input: &str) -> (String, Vec<String>) {
    let Some(start) = input.find("\nPlan:\n") else {
        return (String::new(), Vec::new());
    };
    let rest = &input[start + "\nPlan:\n".len()..];
    let (plan, tail) = match rest.find("\nDeclared paths:\n") {
        Some(end) => (
            &rest[..end],
            &rest[end + "\nDeclared paths:\n".len()..],
        ),
        None => (rest, ""),
    };
    let paths = tail
        .split("\nSpend estimate:")
        .next()
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    (plan.trim().to_string(), paths)
}

/// The verify record this ticket was tested by: the one the handback names,
/// or the newest one carrying the ticket's id.
fn verify_for(
    records: &Path,
    ticket: &crate::project_management::TicketRecord,
) -> Option<(crate::sandbox_verify::VerifyRecord, serde_json::Value)> {
    let Ok(entries) = std::fs::read_dir(records) else {
        return None;
    };
    let mut found: Vec<(crate::sandbox_verify::VerifyRecord, serde_json::Value)> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|entry| std::fs::read(entry.path()).ok())
        .filter_map(|body| serde_json::from_slice::<serde_json::Value>(&body).ok())
        .filter(|raw| raw.get("ticket_id").and_then(|v| v.as_str()) == Some(ticket.id.as_str()))
        .filter_map(|raw| {
            serde_json::from_value(raw.clone())
                .ok()
                .map(|record| (record, raw))
        })
        .collect();
    let named = ticket
        .handback
        .as_ref()
        .and_then(|h| h.verify_record_id.clone());
    if let Some(id) = named {
        if let Some(position) = found.iter().position(|(record, _)| record.id == id) {
            return Some(found.swap_remove(position));
        }
    }
    found.sort_by(|a, b| a.0.created_at.cmp(&b.0.created_at));
    found.pop()
}

fn summarise(record: &crate::sandbox_verify::VerifyRecord) -> VerifySummary {
    // The totals live in the test step. A run that died in install has none,
    // and that is the "could not verify" outcome rather than a zero.
    let log = record
        .steps
        .iter()
        .find(|step| step.name == "test")
        .or_else(|| record.steps.last())
        .map(|step| step.log_tail.as_str())
        .unwrap_or_default();
    VerifySummary {
        record_id: record.id.clone(),
        status: record.status.clone(),
        suites: suites(log),
        failing: failing(log),
        steps: record
            .steps
            .iter()
            .map(|step| StepChip {
                name: step.name.clone(),
                command: step.command.clone(),
                exit_code: step.exit_code,
                // The engine records no per-step timing, so a chip claiming a
                // duration would be inventing one.
                duration_ms: 0,
            })
            .collect(),
        sandbox: record.public_url.clone(),
        commit: record.commit_sha.clone(),
    }
}

/// The two suite totals a verify produces, summed per suite: cargo prints one
/// `test result:` line per test binary, and two lines are one suite's answer.
fn suites(log: &str) -> Vec<SuiteTotal> {
    let mut out = Vec::new();
    let rust = regex::Regex::new(
        r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored",
    )
    .expect("static regex");
    let mut cargo = SuiteTotal {
        name: "rust".into(),
        ..Default::default()
    };
    let mut seen = false;
    for caps in rust.captures_iter(log) {
        seen = true;
        cargo.passed += caps[1].parse::<u32>().unwrap_or(0);
        cargo.failed += caps[2].parse::<u32>().unwrap_or(0);
        cargo.skipped += caps[3].parse::<u32>().unwrap_or(0);
    }
    if seen {
        out.push(cargo);
    }
    let mut ui = SuiteTotal {
        name: "playwright".into(),
        ..Default::default()
    };
    let mut seen = false;
    for (pattern, field) in [
        (r"(?m)^[ \t]*(\d+) passed \(", 0),
        (r"(?m)^[ \t]*(\d+) failed\b", 1),
        (r"(?m)^[ \t]*(\d+) skipped\b", 2),
    ] {
        let re = regex::Regex::new(pattern).expect("static regex");
        for caps in re.captures_iter(log) {
            seen = true;
            let count = caps[1].parse::<u32>().unwrap_or(0);
            match field {
                0 => ui.passed += count,
                1 => ui.failed += count,
                _ => ui.skipped += count,
            }
        }
    }
    if seen {
        out.push(ui);
    }
    out
}

/// What failed, by name. Both runners name their failures in the tail the
/// record keeps, which is why the tail keeps those lines at all.
fn failing(log: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |name: String| {
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    };
    for caps in regex::Regex::new(r"(?m)^test (\S+) \.\.\. FAILED")
        .expect("static regex")
        .captures_iter(log)
    {
        push(caps[1].to_string());
    }
    for caps in regex::Regex::new(r"(?m)^[ \t]*\d+\)[ \t]+(.+)$")
        .expect("static regex")
        .captures_iter(log)
    {
        let name = caps[1]
            .trim()
            .trim_end_matches(|c: char| c == '─' || c == '-' || c.is_whitespace())
            .trim()
            .to_string();
        push(name);
    }
    out
}

fn tested_text(record: &crate::sandbox_verify::VerifyRecord, summary: &VerifySummary) -> String {
    let totals: Vec<String> = summary
        .suites
        .iter()
        .map(|suite| {
            format!(
                "{} {} passed, {} failed, {} skipped",
                suite.name, suite.passed, suite.failed, suite.skipped
            )
        })
        .collect();
    let mut text = record.status.clone();
    if !totals.is_empty() {
        text.push_str("; ");
        text.push_str(&totals.join("; "));
    }
    if record.not_evidence {
        text.push_str("; the tree it ran on is not evidence about this ticket");
    }
    text
}

fn merged_text(signoff: &crate::jury::Signoff, job: Option<&crate::jury::Job>) -> String {
    let sha = &signoff.merge_sha[..signoff.merge_sha.len().min(8)];
    let mut text = match job {
        Some(job) => format!("merged as {sha} into {}", job.policy.integration_branch),
        None => format!("merged as {sha}"),
    };
    if let Some(job) = job {
        let promote = job.policy.promote_branch.trim();
        if !promote.is_empty() && job.state == "integrated" {
            text.push_str(&format!("; promoted to {promote}"));
        }
    }
    if signoff.revoked {
        text.push_str("; revoked");
        if let Some(revert) = &signoff.revert_sha {
            text.push_str(&format!(", reverted by {}", &revert[..revert.len().min(8)]));
        }
    }
    text
}

/// When the status became done, from the control repo's events.
///
/// The events are hand-written as much as they are machine-written, so the
/// status transition sits under one of four different keys depending on who
/// wrote it. The earliest event that names this ticket and a done status is
/// the transition; a ticket whose events never recorded one falls back to the
/// record's own `updated_at`.
fn done_at(repo: &Path, ticket: &crate::project_management::TicketRecord) -> Option<String> {
    let dirs = [
        repo.join("events"),
        repo.join("projects").join(&ticket.project).join("events"),
    ];
    let mut earliest: Option<String> = None;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().extension().is_none_or(|x| x != "json") {
                continue;
            }
            let Ok(body) = std::fs::read(entry.path()) else {
                continue;
            };
            let Ok(event) = serde_json::from_slice::<serde_json::Value>(&body) else {
                continue;
            };
            if !event_names(&event, &ticket.id) || !event_is_done(&event) {
                continue;
            }
            let at = ["at", "timestamp"]
                .iter()
                .find_map(|key| event.get(*key).and_then(|v| v.as_str()))
                .unwrap_or_default()
                .to_string();
            if at.is_empty() {
                continue;
            }
            if earliest.as_ref().is_none_or(|best| at < *best) {
                earliest = Some(at);
            }
        }
    }
    earliest
}

fn event_names(event: &serde_json::Value, id: &str) -> bool {
    ["ticket", "ticket_id", "subject"]
        .iter()
        .any(|key| event.get(*key).and_then(|v| v.as_str()) == Some(id))
}

fn event_is_done(event: &serde_json::Value) -> bool {
    ["changes", "fields", "payload", "details"].iter().any(|key| {
        let Some(status) = event.get(*key).and_then(|v| v.get("status")) else {
            return false;
        };
        // A transition is written either as the new value or as [from, to].
        let value = match status {
            serde_json::Value::Array(values) => values.last().and_then(|v| v.as_str()),
            other => other.as_str(),
        };
        value.is_some_and(is_done)
    })
}

/// The branch the work was done on, from the run registry. Nothing on the
/// ticket records it; the run manifest does, and the handback names the run.
fn branch_for(registry: &Path, ticket: &crate::project_management::TicketRecord) -> String {
    let named = ticket.handback.as_ref().and_then(|h| h.run_id.clone());
    let Ok(ids) = crate::run_control::list_ids_in(registry) else {
        return String::new();
    };
    let mut best: Option<(i64, String)> = None;
    for id in ids {
        let Ok(run) = crate::run_control::load_manifest_in(registry, &id) else {
            continue;
        };
        if run.ticket.as_deref() != Some(ticket.id.as_str()) || run.branch.is_empty() {
            continue;
        }
        if named.as_deref() == Some(run.run_id.as_str()) {
            return run.branch;
        }
        if best.as_ref().is_none_or(|(at, _)| run.started_at > *at) {
            best = Some((run.started_at, run.branch));
        }
    }
    best.map(|(_, branch)| branch).unwrap_or_default()
}

/// What this ticket taught, for the person reading later. Three sources in
/// order, concatenated when more than one has something, and empty when none
/// of them does. An invented learning is worse than none.
fn learnings(
    ticket: &crate::project_management::TicketRecord,
    tickets: &[crate::project_management::TicketRecord],
    registry: &Path,
    vault: Option<&Path>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(block) = doc_learnings(ticket, vault) {
        parts.push(block);
    }
    if let Some(handback) = &ticket.handback {
        let mut line = String::new();
        if let Some(left) = handback.not_finished.as_ref().map(|s| s.trim()) {
            if !left.is_empty() {
                line.push_str(&format!("Not finished: {left}"));
            }
        }
        let confidence = match handback.confidence {
            crate::handback::Confidence::Unstated => "",
            crate::handback::Confidence::Low => "low",
            crate::handback::Confidence::Medium => "medium",
            crate::handback::Confidence::High => "high",
        };
        if !confidence.is_empty() {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&format!("Confidence: {confidence}."));
        }
        if !line.is_empty() {
            parts.push(line);
        }
    }
    if let Some(line) = incident_line(ticket, tickets, registry) {
        parts.push(line);
    }
    parts.join("\n\n")
}

/// The `## Learnings` or `Deliberately not done` block from the ticket's
/// Shipped section. Written as a heading in most documents and as a leading
/// paragraph in some, so both are read.
fn doc_learnings(
    ticket: &crate::project_management::TicketRecord,
    vault: Option<&Path>,
) -> Option<String> {
    let root = vault?;
    for reference in &ticket.documentation {
        let Some(rel) = reference.strip_prefix("work:") else {
            continue;
        };
        let Ok(path) = crate::vault::safe_join(root, rel) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(block) = shipped_learnings(&text, &ticket.id) {
            return Some(block);
        }
    }
    None
}

/// Pure over the document text, because the vault is an awkward thing to
/// build in a test and this is the part with the rules in it.
fn shipped_learnings(doc: &str, id: &str) -> Option<String> {
    let lines: Vec<&str> = doc.lines().collect();
    // The last Shipped section for this ticket: a re-shipped ticket appends a
    // second one, and the later one is what shipped.
    let start = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("## Shipped") && line.contains(id))
        .map(|(index, _)| index)
        .next_back()?;
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| line.starts_with("## Shipped"))
        .map(|(index, _)| index)
        .unwrap_or(lines.len());
    let section = &lines[start + 1..end];

    let wanted = |line: &str| {
        let title = line.trim_start_matches('#').trim().to_ascii_lowercase();
        line.starts_with('#') && (title == "learnings" || title == "deliberately not done")
    };
    let mut blocks: Vec<String> = Vec::new();
    let mut index = 0;
    while index < section.len() {
        let line = section[index];
        if wanted(line) {
            let mut body = Vec::new();
            index += 1;
            while index < section.len() && !section[index].starts_with('#') {
                body.push(section[index]);
                index += 1;
            }
            let body = body.join("\n").trim().to_string();
            if !body.is_empty() {
                blocks.push(body);
            }
            continue;
        }
        if line.trim_start().starts_with("Deliberately not done") {
            let mut body = Vec::new();
            while index < section.len() && !section[index].trim().is_empty() {
                body.push(section[index]);
                index += 1;
            }
            blocks.push(body.join("\n").trim().to_string());
            continue;
        }
        index += 1;
    }
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}

/// Has this ticket's failure been seen before? The incident memory answers
/// that already; this only asks it about this ticket's own newest failure.
///
/// A registry that will not read costs the learnings one of three sources,
/// which is not a reason to fail the whole read: a fresh install has no runs.
fn incident_line(
    ticket: &crate::project_management::TicketRecord,
    tickets: &[crate::project_management::TicketRecord],
    registry: &Path,
) -> Option<String> {
    let incidents = crate::incidents::all(registry, tickets).ok()?;
    let mine = incidents.iter().find(|i| i.ticket == ticket.id)?;
    crate::incidents::recognised(&incidents, &mine.signal, mine.run_id.as_deref())
}

/// What there is to look at. Read off the raw record so that a field the
/// capture side adds later needs no change here, and so that a record written
/// before it existed still reads.
///
/// The overlay must never be blank with nothing said: when the run recorded
/// its own reason for having no artifact, that reason is the note; otherwise
/// the note says there is no recording.
fn evidence(raw: Option<&serde_json::Value>) -> Evidence {
    let Some(raw) = raw else {
        return Evidence {
            note: "no verify run for this ticket yet".into(),
            ..Default::default()
        };
    };
    let field = |key: &str| {
        raw.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let video = field("video_path");
    let screenshot = field("screenshot_path");
    let recorded = field("capture_note");
    let note = if !recorded.is_empty() {
        recorded
    } else if video.is_empty() && screenshot.is_empty() {
        "no recording for this run".to_string()
    } else {
        String::new()
    };
    Evidence {
        video_path: video,
        screenshot_path: screenshot,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xnaut-delivery-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A control repo with one project and one ticket in it.
    fn control(root: &Path, ticket: serde_json::Value) -> PathBuf {
        let repo = root.join("control");
        let project = ticket["project"].as_str().unwrap().to_string();
        let tickets = repo.join("projects").join(&project).join("tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        std::fs::write(
            repo.join("projects").join(&project).join("project.json"),
            json!({"key": project, "name": project, "created_at": "2026-09-01T00:00:00+00:00"})
                .to_string(),
        )
        .unwrap();
        std::fs::write(
            tickets.join(format!("{}.json", ticket["id"].as_str().unwrap())),
            ticket.to_string(),
        )
        .unwrap();
        repo
    }

    fn ticket_json() -> serde_json::Value {
        json!({
            "id": "XNAUT-1", "project": "XNAUT", "title": "a thing", "type": "feature",
            "status": "in_progress", "priority": "high", "owner": "@claude",
            "documentation": [], "tags": [], "release": "", "body": "the issue as written",
            "source_id": "", "revision": 1,
            "created_at": "2026-09-01T00:00:00+00:00", "updated_at": "2026-09-02T00:00:00+00:00"
        })
    }

    fn record_json(ticket: &str, steps: serde_json::Value) -> serde_json::Value {
        json!({
            "id": "rec-1", "run_id": "run-1", "ticket_id": ticket, "project": "XNAUT",
            "repo_path": "/tmp/tree", "commit_sha": "abc1234def", "provider_kind": "gitvm",
            "sandbox_id": "gitvm", "public_url": "https://sunny.nautbox.dev", "status": "failed",
            "steps": steps, "log_dir": "", "video_path": null,
            "created_at": "2026-09-02T10:00:00+00:00", "updated_at": "2026-09-02T10:30:00+00:00"
        })
    }

    fn read(root: &Path, repo: &Path, id: &str) -> Lifecycle {
        join_in(repo, &root.join("registry"), &root.join("records"), None, id).unwrap()
    }

    #[test]
    fn a_ticket_that_only_exists_still_comes_back_with_all_seven_stages() {
        let root = scratch("bare");
        let repo = control(&root, ticket_json());
        let life = read(&root, &repo, "XNAUT-1");

        assert_eq!(life.stages.len(), 7);
        let keys: Vec<&str> = life.stages.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(
            keys,
            ["issue", "proposed", "final", "tested", "done", "merged", "learnings"]
        );
        assert_eq!(life.stages[0].text, "the issue as written");
        assert_eq!(life.stages[0].at, "2026-09-01T00:00:00+00:00");
        for stage in &life.stages[1..] {
            assert!(stage.at.is_empty() && stage.text.is_empty(), "{stage:?}");
        }
        assert!(life.verify.is_none());
        assert_eq!(life.evidence.note, "no verify run for this ticket yet");
        assert_eq!(life.title, "a thing");
        assert_eq!(life.owner, "@claude");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_ticket_nobody_wrote_is_an_error_not_an_empty_lifecycle() {
        let root = scratch("missing");
        let repo = control(&root, ticket_json());
        let error = join_in(
            &repo,
            &root.join("registry"),
            &root.join("records"),
            None,
            "XNAUT-404",
        )
        .unwrap_err();
        assert!(error.contains("XNAUT-404"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_handback_fills_the_final_stage_with_its_files_and_commits() {
        let root = scratch("handback");
        let mut ticket = ticket_json();
        ticket["handback"] = json!({
            "ticket": "XNAUT-1", "summary": "closed the gap",
            "files_changed": ["src-tauri/src/delivery.rs"],
            "commits": ["59d22ab48144ef9433e64ece1b7397b84114bb44"],
            "how_verified": "cargo test", "submitted_at": "2026-09-02T09:00:00+00:00"
        });
        let repo = control(&root, ticket);
        let life = read(&root, &repo, "XNAUT-1");

        let final_stage = life.stages.iter().find(|s| s.key == "final").unwrap();
        assert_eq!(final_stage.text, "closed the gap");
        assert_eq!(final_stage.at, "2026-09-02T09:00:00+00:00");
        assert_eq!(
            final_stage.items,
            [
                "src-tauri/src/delivery.rs",
                "commit 59d22ab48144ef9433e64ece1b7397b84114bb44"
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_verify_record_fills_tested_with_parsed_totals_and_failing_names() {
        let root = scratch("verify");
        let repo = control(&root, ticket_json());
        let records = root.join("records");
        std::fs::create_dir_all(&records).unwrap();
        let log = "test result: FAILED. 1090 passed; 2 failed; 45 ignored; 0 measured\n\
             test delivery::tests::joins ... FAILED\n\
             test delivery::tests::reads ... FAILED\n\
             \n  1) tests/delivery.spec.mjs:9:3 › the panel shows the stages ─────\n\
             \n  1 failed\n  149 passed (2.7m)\n";
        std::fs::write(
            records.join("rec-1.json"),
            record_json(
                "XNAUT-1",
                json!([
                    {"name": "install", "command": "apt-get install", "exit_code": 0, "log_tail": "ok"},
                    {"name": "build", "command": "npm run build", "exit_code": 0, "log_tail": "ok"},
                    {"name": "test", "command": "cargo test", "exit_code": 101, "log_tail": log}
                ]),
            )
            .to_string(),
        )
        .unwrap();

        let life = read(&root, &repo, "XNAUT-1");
        let verify = life.verify.expect("a verify summary");
        assert_eq!(verify.record_id, "rec-1");
        assert_eq!(verify.status, "failed");
        assert_eq!(verify.sandbox, "https://sunny.nautbox.dev");
        assert_eq!(verify.commit, "abc1234def");
        assert_eq!(verify.suites.len(), 2, "{:?}", verify.suites);
        assert_eq!(verify.suites[0].name, "rust");
        assert_eq!(
            (
                verify.suites[0].passed,
                verify.suites[0].failed,
                verify.suites[0].skipped
            ),
            (1090, 2, 45)
        );
        assert_eq!(verify.suites[1].name, "playwright");
        assert_eq!((verify.suites[1].passed, verify.suites[1].failed), (149, 1));
        assert_eq!(
            verify.failing,
            [
                "delivery::tests::joins",
                "delivery::tests::reads",
                "tests/delivery.spec.mjs:9:3 › the panel shows the stages"
            ]
        );
        let names: Vec<&str> = verify.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["install", "build", "test"]);
        assert_eq!(verify.steps[2].exit_code, Some(101));

        let tested = life.stages.iter().find(|s| s.key == "tested").unwrap();
        assert_eq!(tested.at, "2026-09-02T10:30:00+00:00");
        assert!(tested.text.starts_with("failed; rust 1090 passed"), "{}", tested.text);
        assert_eq!(tested.items.len(), 3);
        // No video on the record, so the overlay is told why rather than left blank.
        assert_eq!(life.evidence.note, "no recording for this run");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn evidence_carries_the_paths_and_the_run_s_own_reason_when_there_are_none() {
        let root = scratch("evidence");
        let repo = control(&root, ticket_json());
        let records = root.join("records");
        std::fs::create_dir_all(&records).unwrap();
        let mut record = record_json("XNAUT-1", json!([]));
        record["video_path"] = json!("/tmp/run.webm");
        record["screenshot_path"] = json!("/tmp/failed-1.png");
        std::fs::write(records.join("rec-1.json"), record.to_string()).unwrap();
        let life = read(&root, &repo, "XNAUT-1");
        assert_eq!(life.evidence.video_path, "/tmp/run.webm");
        assert_eq!(life.evidence.screenshot_path, "/tmp/failed-1.png");
        assert_eq!(life.evidence.note, "");

        let mut record = record_json("XNAUT-1", json!([]));
        record["capture_note"] = json!("the UI suite never started");
        std::fs::write(records.join("rec-1.json"), record.to_string()).unwrap();
        let life = read(&root, &repo, "XNAUT-1");
        assert_eq!(life.evidence.note, "the UI suite never started");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_vault_doc_is_the_first_source_of_learnings() {
        let root = scratch("doc-learnings");
        let mut ticket = ticket_json();
        ticket["documentation"] = json!(["work:xnaut/Development/features/2026-09-02_Thing.md"]);
        // A handback with something to say too, so source order is actually tested.
        ticket["handback"] = json!({
            "ticket": "XNAUT-1", "summary": "s", "files_changed": [], "commits": [],
            "how_verified": "", "not_finished": "the panel's dark theme",
            "confidence": "medium", "submitted_at": "2026-09-02T09:00:00+00:00"
        });
        let repo = control(&root, ticket);
        let vault = root.join("vault");
        let docs = vault.join("xnaut/Development/features");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(
            docs.join("2026-09-02_Thing.md"),
            "# Thing\n\n## Shipped XNAUT-1\n\n### What was done\n\nthe work\n\n\
             ### Learnings\n\nAn undefined global is a silent no-op.\n\n## Later section\n\nnot this\n",
        )
        .unwrap();

        let life = join_in(
            &repo,
            &root.join("registry"),
            &root.join("records"),
            Some(&vault),
            "XNAUT-1",
        )
        .unwrap();
        let learnings = life.stages.iter().find(|s| s.key == "learnings").unwrap();
        assert_eq!(
            learnings.text,
            "An undefined global is a silent no-op.\n\nNot finished: the panel's dark theme Confidence: medium."
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn learnings_fall_back_to_the_handback_when_no_document_has_a_block() {
        let root = scratch("handback-learnings");
        let mut ticket = ticket_json();
        ticket["handback"] = json!({
            "ticket": "XNAUT-1", "summary": "s", "files_changed": [], "commits": [],
            "how_verified": "", "not_finished": "the mobile view",
            "confidence": "low", "submitted_at": "2026-09-02T09:00:00+00:00"
        });
        let repo = control(&root, ticket);
        let life = read(&root, &repo, "XNAUT-1");
        let learnings = life.stages.iter().find(|s| s.key == "learnings").unwrap();
        assert_eq!(
            learnings.text,
            "Not finished: the mobile view Confidence: low."
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_ticket_with_nothing_to_teach_returns_empty_learnings() {
        let root = scratch("no-learnings");
        let repo = control(&root, ticket_json());
        let life = read(&root, &repo, "XNAUT-1");
        assert_eq!(
            life.stages.iter().find(|s| s.key == "learnings").unwrap().text,
            ""
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_deliberately_not_done_paragraph_counts_as_a_learnings_block() {
        let doc = "## Shipped XNAUT-1\n\nwhat was done\n\n\
                   Deliberately not done: batching and bisection.\nThey are out of scope.\n\n\
                   Next paragraph.\n";
        assert_eq!(
            shipped_learnings(doc, "XNAUT-1").unwrap(),
            "Deliberately not done: batching and bisection.\nThey are out of scope."
        );
    }

    #[test]
    fn the_later_shipped_section_wins_when_a_ticket_shipped_twice() {
        let doc = "## Shipped XNAUT-1\n\n### Learnings\n\nthe first attempt\n\n\
                   ## Shipped XNAUT-1\n\n### Learnings\n\nthe rebase that landed\n";
        assert_eq!(
            shipped_learnings(doc, "XNAUT-1").unwrap(),
            "the rebase that landed"
        );
    }

    #[test]
    fn the_plan_gate_job_fills_the_proposed_stage_and_the_signoff_fills_merged() {
        let root = scratch("jury");
        let policy = json!({
            "reviewers": [], "threshold": 0.7, "deadline_seconds": 600, "max_spend": 5.0,
            "integration_branch": "dev", "promote_branch": "uat", "owner_only": false,
            "integration_commands": []
        });
        let plan = json!({
            "id": "0102c878-b0fb-4e4f-a32d-28aabe877c26", "gate": "plan", "ticket": "XNAUT-1",
            "project": "XNAUT", "worktree": "/tmp/tree", "author": "@claude", "author_run": null,
            "ticket_revision": 1, "input":
                "Ticket scope:\n{\"id\":\"XNAUT-1\"}\nAssigned worktree: /tmp/tree\nPlan:\nJoin the five stores in one reader.\nDeclared paths:\nsrc-tauri/src/delivery.rs\nSpend estimate: Some(0.2)",
            "input_hash": "h", "source_sha": "abc", "policy": policy,
            "round": 1, "deadline": 1789000000000i64,
            "reviews": [{"run_id": "r1", "runtime": "codex", "input_hash": "h",
                         "finished_at": 1789081276029i64, "review": null, "error": null}],
            "decision": "approved", "reason": "", "inbox_id": null, "notify_id": null,
            "state": "decided", "signoff": null
        });
        let signoff_job = json!({
            "id": "058b7e9c-49a7-4f84-a539-256026037b3c", "gate": "signoff", "ticket": "XNAUT-1",
            "project": "XNAUT", "worktree": "/tmp/tree", "author": "@claude", "author_run": null,
            "ticket_revision": 1, "input": "", "input_hash": "h", "source_sha": "abc",
            "policy": policy, "round": 1, "deadline": 1789000000000i64, "reviews": [],
            "decision": "approved", "reason": "", "inbox_id": null, "notify_id": null,
            "state": "integrated", "signoff": null
        });
        let mut ticket = ticket_json();
        ticket["status"] = json!("complete");
        ticket["jury_reviews"] = json!([plan, signoff_job]);
        ticket["signoff"] = json!({
            "jury_id": "058b7e9c-49a7-4f84-a539-256026037b3c",
            "reviewers": [{"run_id": "r2", "runtime": "claude", "input_hash": "h",
                           "finished_at": 1789081300000i64, "review": null, "error": null}],
            "scores": [0.9], "merge_sha": "59d22ab48144ef9433e64ece1b7397b84114bb44",
            "integration_verify_run": null, "revoked": false, "revert_sha": null
        });
        let repo = control(&root, ticket);
        let life = read(&root, &repo, "XNAUT-1");

        let proposed = life.stages.iter().find(|s| s.key == "proposed").unwrap();
        assert_eq!(proposed.text, "Join the five stores in one reader.");
        assert_eq!(proposed.items, ["src-tauri/src/delivery.rs"]);
        assert!(proposed.at.starts_with("2026-09-10"), "{}", proposed.at);

        let merged = life.stages.iter().find(|s| s.key == "merged").unwrap();
        assert_eq!(merged.text, "merged as 59d22ab4 into dev; promoted to uat");
        assert_eq!(merged.items, ["59d22ab48144ef9433e64ece1b7397b84114bb44"]);

        // No event recorded the transition, so done falls back to updated_at.
        let done = life.stages.iter().find(|s| s.key == "done").unwrap();
        assert_eq!(done.text, "status became complete");
        assert_eq!(done.at, "2026-09-02T00:00:00+00:00");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_events_file_dates_the_done_stage_when_it_recorded_the_transition() {
        let root = scratch("events");
        let mut ticket = ticket_json();
        ticket["status"] = json!("done");
        let repo = control(&root, ticket);
        let events = repo.join("events");
        std::fs::create_dir_all(&events).unwrap();
        std::fs::write(
            events.join("a.json"),
            json!({"type": "ticket.updated", "ticket_id": "XNAUT-1",
                   "at": "2026-09-01T12:00:00+00:00", "changes": {"status": ["in_progress", "done"]}})
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            events.join("b.json"),
            json!({"type": "ticket.updated", "ticket": "XNAUT-1",
                   "at": "2026-09-01T18:00:00+00:00", "fields": {"status": "done"}})
            .to_string(),
        )
        .unwrap();

        let done = read(&root, &repo, "XNAUT-1")
            .stages
            .into_iter()
            .find(|s| s.key == "done")
            .unwrap();
        assert_eq!(done.at, "2026-09-01T12:00:00+00:00", "the transition, not the last touch");
        std::fs::remove_dir_all(root).unwrap();
    }
}
