// XNAUT-303 sign-off and compensation, using the shared jury decision service.
// Merge/revert operations are journaled before publishing refs and use Git's
// compare-and-swap update-ref; failures never silently reset somebody's branch.
use crate::jury::*;
use crate::jury_runtime::{git, ticket};
use crate::run_control::{self, RunKind, RunManifest, RunState};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tauri::AppHandle;

static SIGNOFF_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
// XNAUT-317: only Git publication/compensation is serial. Each integration
// build owns a private clone and must run without either of these locks.
static INTEGRATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Called only from the owner inbox action, never the reviewer answer path.
/// Preserve the actual votes; owner approval is separate authority.
pub fn owner_decision(
    repo: &Path,
    root: &Path,
    id: &str,
    inbox: &str,
    approved: bool,
) -> Result<Job, String> {
    let mut job = read_job(root, id)?;
    if job.inbox_id.as_deref() != Some(inbox)
        || job.decision != Some(Decision::Owner)
        || job.state != "owner_required"
    {
        return Err("this owner decision does not match an outstanding jury escalation".into());
    }
    if approved {
        if crate::jury_runtime::scope_hash(&ticket(repo, &job.ticket)?) != job.ticket_scope_hash
            || git(Path::new(&job.worktree), &["rev-parse", "HEAD"])? != job.source_sha
        {
            return Err("reviewed inputs changed; submit a fresh review before approval".into());
        }
        if let Some(file) = &job.plan_file {
            if std::fs::read_to_string(file).ok().map(|s| hash(&s)) != job.plan_hash {
                return Err("plan changed; submit a fresh review".into());
            }
        }
    }
    job.owner_approved = approved;
    job.state = if approved {
        if job.gate == Gate::Signoff {
            "owner_signed"
        } else {
            "owner_settled"
        }
    } else {
        "owner_changes_requested"
    }
    .into();
    job.reason = format!(
        "Owner {} this exact input {}.\n{}",
        if approved {
            "approved"
        } else {
            "requested changes to"
        },
        job.input_hash,
        job.reason
    );
    write_job(root, &job)?;
    crate::project_management::attach_jury_in(repo, &job, None)?;
    crate::inbox::jury_archive_asks(None, &job.id, &job.ticket);
    Ok(job)
}
/// The owner's way out of a parked sign-off (XNAUT-399): retire the job,
/// archive its card, and start a fresh review of the same green record
/// against the ticket as it is NOW. One click, no hand edits of job files.
pub fn rereview(
    app: Option<&AppHandle>,
    repo: &Path,
    registry: &Path,
    root: &Path,
    id: &str,
) -> Result<Job, String> {
    let mut old = read_job(root, id)?;
    if old.gate != Gate::Signoff {
        return Err("re-review is for sign-off jobs".into());
    }
    if old.state == "integrated" {
        return Err("this sign-off already integrated; nothing to review again".into());
    }
    let record = crate::sandbox_verify::passed_record_for(&old.ticket, &old.source_sha)
        .ok_or_else(|| format!("no green verify record for {} at {}", old.ticket, &old.source_sha[..8.min(old.source_sha.len())]))?;
    old.state = "superseded".into();
    old.reason = format!("Superseded by a re-review the owner asked for.\n{}", old.reason);
    write_job(root, &old)?;
    crate::project_management::attach_jury_in(repo, &old, None)?;
    crate::inbox::jury_archive_asks(app, &old.id, &old.ticket);
    start(app, repo, registry, root, &record)
}

#[tauri::command]
pub async fn jury_rereview(app: AppHandle, jury_id: String) -> Result<Job, String> {
    let repo = crate::project_management::repo_now()?;
    let registry = crate::agents::registry_dir()?;
    let root = crate::jury_runtime::store()?;
    tokio::task::spawn_blocking(move || rereview(Some(&app), &repo, &registry, &root, &jury_id))
        .await
        .map_err(|e| e.to_string())?
}

pub fn schedule(app: &AppHandle, record: crate::sandbox_verify::VerifyRecord) {
    // Silently skip what cannot be reviewed. `schedule` turns every error into
    // an owner escalation, so without this a pre-registry record (null commit
    // sha) posted "sign-off needs owner intervention" carrying a git error, on
    // every tick, for tickets closed weeks ago (XNAUT-252, 2026-09-09).
    if nothing_to_sign(&record).is_some() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let work = || -> Result<(), String> {
            let repo = crate::project_management::repo_now()?;
            let registry = crate::agents::registry_dir()?;
            let root = crate::jury_runtime::store()?;
            start(Some(&app), &repo, &registry, &root, &record)?;
            Ok(())
        };
        if let Err(error) = work() {
            let _ = crate::inbox::jury_post(
                Some(&app),
                "approve",
                crate::inbox::PostRequest {
                    project: record.project,
                    ticket: Some(record.ticket_id.clone()),
                    from: "nautbot".into(),
                    title: format!("{} sign-off needs owner intervention", record.ticket_id),
                    body: error,
                    ..Default::default()
                },
                None,
            );
        }
    });
}

pub fn test_totals(log: &str) -> serde_json::Value {
    let rust = regex::Regex::new(
        r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored",
    )
    .unwrap();
    let ui = regex::Regex::new(r"(?m)^\s*(\d+) passed \(").unwrap();
    let rust:Vec<_>=rust.captures_iter(log).map(|c|serde_json::json!({"passed":c[1].parse::<u64>().unwrap(),"failed":c[2].parse::<u64>().unwrap(),"ignored":c[3].parse::<u64>().unwrap()})).collect();
    let ui: Vec<_> = ui
        .captures_iter(log)
        .map(|c| c[1].parse::<u64>().unwrap())
        .collect();
    serde_json::json!({"rust":rust,"ui":ui})
}
/// The ticket's design document must carry a `## Shipped <ID>` section by
/// sign-off: the doc travels with the code (André, 2026-09-08: "it should
/// also write it during the flow"). Pure over the document text; the caller
/// resolves the vault. A ticket with no linked document is not excused: the
/// dispatch prompt tells the agent to create one and link it.
pub(crate) fn shipped_section_missing(ticket_id: &str, docs: &[(String, Option<String>)]) -> Option<String> {
    if docs.is_empty() {
        return Some(format!("{ticket_id} links no design document; the flow requires one with a Shipped section"));
    }
    let marker = format!("## Shipped {ticket_id}");
    if docs.iter().any(|(_, text)| text.as_deref().is_some_and(|t| t.contains(&marker))) {
        return None;
    }
    Some(format!(
        "no `{marker}` section in the ticket's document ({})",
        docs.iter().map(|(r, _)| r.as_str()).collect::<Vec<_>>().join(", ")
    ))
}

/// Every design document linked to the ticket, as (relative path, text). The
/// text is None when the vault cannot produce it, which is a different thing
/// from an empty document and is why the pair is kept.
fn doc_texts(t: &crate::project_management::TicketRecord) -> Vec<(String, Option<String>)> {
    let root = crate::vault::vault_root("work").ok();
    t.documentation
        .iter()
        .filter_map(|r| r.strip_prefix("work:").map(str::to_string))
        .map(|rel| {
            let text = root
                .as_ref()
                .and_then(|root| crate::vault::safe_join(root, &rel).ok())
                .and_then(|p| std::fs::read_to_string(p).ok());
            (rel, text)
        })
        .collect()
}

fn missing_shipped_doc(t: &crate::project_management::TicketRecord) -> Option<String> {
    shipped_section_missing(&t.id, &doc_texts(t))
}

/// What the evidence rule makes of a ticket: why it cannot merge without the
/// owner, and what it accepted along the way that is worth saying out loud.
#[derive(Debug, Default, PartialEq)]
pub struct Evidence {
    pub reason: Option<String>,
    /// The not-done items the ticket itself declared out of scope. Empty is
    /// the ordinary case; a non-empty list is the difference between a merge
    /// that silently ignored unfinished work and one that named it (XNAUT-380).
    pub accepted: Vec<String>,
}

fn refused(why: impl Into<String>) -> Evidence {
    Evidence {
        reason: Some(why.into()),
        accepted: vec![],
    }
}

/// Whether the handback's not-done list may pass, and which of its items the
/// ticket had already declared.
///
/// Until XNAUT-380 any item at all went to the owner. That is right when the
/// agent cut scope on its own, and it was noise on every dispatched ticket
/// whose own body says "not in scope": on the 2026-09-14 autonomy run it was
/// the single human click in an otherwise unattended loop. So an item the
/// ticket or its design document already declares is accepted and named;
/// anything else still escalates, and the card names only those.
fn not_done_verdict(
    t: &crate::project_management::TicketRecord,
    policy: &crate::jury::Policy,
    left: &str,
) -> Evidence {
    if left.trim().is_empty() || left.trim() == "nothing" {
        return Evidence::default();
    }
    // The whole-list form the rule started with: the exact handback words
    // pasted into the ticket body accept the lot, switch or no switch.
    if t.body.contains(&format!("Accepted not_finished: {left}")) {
        return Evidence {
            reason: None,
            accepted: vec![left.trim().to_string()],
        };
    }
    if policy.escalate_every_not_done {
        return refused("unfinished work has not been accepted by the ticket");
    }
    let docs: Vec<String> = doc_texts(t).into_iter().filter_map(|(_, text)| text).collect();
    let scope = crate::signoff_scope::ticket_acceptance(left, &t.body, &docs);
    if scope.open.is_empty() {
        return Evidence {
            reason: None,
            accepted: scope.accepted,
        };
    }
    Evidence {
        reason: Some(format!(
            "unfinished work has not been accepted by the ticket: {}",
            scope.open.join("; ")
        )),
        accepted: scope.accepted,
    }
}

pub fn evidence(
    t: &crate::project_management::TicketRecord,
    record: &crate::sandbox_verify::VerifyRecord,
    bundle: &str,
    policy: &crate::jury::Policy,
) -> Evidence {
    if t.status != "complete"
        || record.status != "passed"
        || record.not_evidence
        || record.steps.is_empty()
        // XNAUT-349: a soft step's non-zero exit is a recorded observation,
        // not a red build, so the exit codes are read through the severities.
        || !crate::sandbox_verify::steps_are_green(record, crate::jury::strict_mode())
    {
        return refused("NautBot completion and green evidence are required");
    }
    let Some(h) = &t.handback else {
        return refused("typed handback missing");
    };
    if h.files_changed.is_empty() || h.commits.is_empty() || h.how_verified.trim().is_empty() {
        return refused("incomplete handback");
    }
    let Some(left) = &h.not_finished else {
        return refused("not_finished is missing");
    };
    let scope = not_done_verdict(t, policy, left);
    if scope.reason.is_some() {
        return scope;
    }
    // From here a refusal loses the accepted list, which is the right way
    // round: nothing merges, so there is nothing to have accepted.
    if let Some(why) = missing_shipped_doc(t) {
        return refused(why);
    }
    let totals = test_totals(
        &record
            .steps
            .iter()
            .map(|s| s.log_tail.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    if totals["rust"].as_array().is_none_or(|a| a.is_empty())
        || totals["ui"].as_array().is_none_or(|a| a.is_empty())
    {
        return refused("verification record lacks full Rust and UI totals");
    }
    let declared = bundle
        .lines()
        .find_map(|line| line.strip_prefix("XNAUT_TEST_TOTALS="))
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
    if declared.as_ref() != Some(&totals) {
        return refused("bundle totals do not match recorded verification totals; include XNAUT_TEST_TOTALS=<JSON>");
    }
    scope
}

/// The commit a branch's review diff starts from: its merge base with the
/// integration tip, so the diff shows what the branch ADDS. Diffing against
/// the tip itself shows every commit the tip gained since the branch point as
/// a deletion by the branch; on 2026-09-07 both reviewers refused XNAUT-305
/// for "removing" four fixes that had landed on the tip that afternoon. The
/// merge keeps both sides, and the evidence has to say so.
pub(crate) fn review_base(tree: &Path, tip: Option<&str>, source: &str) -> String {
    tip.and_then(|tip| git(tree, &["merge-base", tip, source]).ok())
        .unwrap_or_else(|| source.to_string())
}

/// Why this record is not a merge decision at all. Sign-off exists to decide
/// whether work reaches the integration branch; three cases are not decisions
/// and must never reach the owner:
///
/// - no commit: nothing to diff, nothing to bind evidence to. Records from
///   before the run registry carry a null sha (XNAUT-252).
/// - a commit this repository does not have: it was built somewhere else, or
///   in a worktree long gone.
/// - a commit already on the integration branch: the merge happened. There is
///   nothing left to decide.
///
/// Without the third, turning the gate on swept the whole historical board:
/// 100 tickets completed long before the jury existed each got a job and each
/// escalated, 298 asks in ninety minutes (2026-09-09).
/// Drift in the tree the verify ran in, or None when the tree cannot report on
/// it. The verified artifact is the COMMIT, and a commit is immutable. But
/// `repo_path` is a worktree many tickets share, so requiring its HEAD to equal
/// this ticket's commit means at most one ticket can ever pass and every other
/// one escalates to the owner: on 2026-09-09 that produced 285 undecidable
/// escalations across 17 tickets, all reading "verified tree changed". A tree
/// that has moved on is simply not the subject any more; only a tree still
/// parked on the verified commit can say anything about uncommitted drift.
pub(crate) fn tree_drift(tree: &Path, commit_sha: &str) -> Option<String> {
    if git(tree, &["rev-parse", "HEAD"]).ok().as_deref() != Some(commit_sha) {
        return None;
    }
    match git(tree, &["status", "--porcelain"]) {
        Ok(out) if out.is_empty() => None,
        Ok(_) => Some("verified tree has uncommitted changes".into()),
        Err(e) => Some(e),
    }
}

/// Drift in the verified tree, as a check rather than a verdict.
///
/// XNAUT-349: this is the observation that had nowhere advisory to go. It is
/// worth showing, because a tree that has moved on says something about how
/// the evidence was produced, and it is not worth refusing a merge over,
/// because the verified artifact is the commit and a commit is immutable.
/// Soft says both of those at once. `XNAUT_STRICT_CHECKS=1` promotes it for a
/// deliberate strict pass.
pub(crate) fn drift_check(tree: &Path, commit_sha: &str) -> Check {
    let drift = tree_drift(tree, commit_sha);
    Check::soft(
        "verified tree is clean",
        drift.is_none(),
        drift.as_deref().unwrap_or_default(),
    )
}

/// The sign-off escalations a fresh one replaces. The dedupe in `start`
/// deliberately refuses to reuse a job parked on the owner, because its inputs
/// may have moved on. Without retiring the old one, though, every sweep pass
/// added another: 285 jobs across 17 tickets by 2026-09-09, each carrying an
/// inbox ask that had already been archived, so the owner faced a queue of
/// decisions with nowhere left to write. The newest is the only actionable one.
pub(crate) fn superseded_signoffs(t: &crate::project_management::TicketRecord) -> Vec<Job> {
    t.approval
        .jury_reviews
        .iter()
        .filter(|j| j.gate == Gate::Signoff && j.state == "owner_required")
        .cloned()
        .map(|mut j| {
            j.reason = format!("Superseded by a newer sign-off escalation.\n{}", j.reason);
            j.state = "superseded".into();
            j
        })
        .collect()
}

pub(crate) fn nothing_to_sign(record: &crate::sandbox_verify::VerifyRecord) -> Option<String> {
    let sha = record.commit_sha.trim();
    if sha.is_empty() {
        return Some(format!("{} has no commit to review", record.ticket_id));
    }
    let tree = Path::new(&record.repo_path);
    if git(tree, &["cat-file", "-e", &format!("{sha}^{{commit}}")]).is_err() {
        return Some(format!("{} names a commit this repository does not have", record.ticket_id));
    }
    let branch = crate::jury_runtime::policy_integration_branch();
    for reference in [format!("refs/remotes/origin/{branch}"), format!("refs/heads/{branch}")] {
        if git(tree, &["rev-parse", "--verify", "--quiet", &reference]).is_ok()
            && git(tree, &["merge-base", "--is-ancestor", sha, &reference]).is_ok()
            && !reverted_after(tree, sha, &reference)
        {
            return Some(format!("{} is already on {branch}; the merge it would decide has happened", record.ticket_id));
        }
    }
    None
}

/// A merge the app took back out again. `rollback` commits the revert on the
/// integration branch with an `XNAUT jury revert <job>` marker, so the merged
/// commit stays an ANCESTOR of the branch while its changes are gone. Reading
/// ancestry alone, the work looks landed forever: CHESSTRAINER-4's integration
/// verify failed on a PATH bug (XNAUT-410), the merge was reverted, and every
/// re-review after that was refused with "already on dev" (XNAUT-411).
fn reverted_after(tree: &Path, sha: &str, reference: &str) -> bool {
    git(tree, &["log", "--format=%s", &format!("{sha}..{reference}")])
        .map(|log| log.lines().any(|line| line.contains("XNAUT jury revert")))
        .unwrap_or(false)
}

pub fn start(
    app: Option<&AppHandle>,
    repo: &Path,
    registry: &Path,
    root: &Path,
    record: &crate::sandbox_verify::VerifyRecord,
) -> Result<Job, String> {
    let _serial = SIGNOFF_LOCK
        .lock()
        .map_err(|_| "sign-off worker lock unavailable")?;
    if let Some(why) = nothing_to_sign(record) {
        return Err(why);
    }
    let t = ticket(repo, &record.ticket_id)?;
    // The swarm lane has no gate (XNAUT-319). The sweep routes its work
    // straight to the lane merge, so arriving here means a second caller; a
    // gate that is off in one place only is not off.
    if let Some(why) = crate::swarm::no_jury_reason(&t) {
        return Err(why);
    }
    // Reserve admission in the job store before releasing SIGNOFF_LOCK. A
    // review is not attached to its ticket until it finishes; another sweep
    // during that interval must reuse it rather than launch another pair.
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())?.flatten() {
        if entry.path().extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Some(j) = std::fs::read(entry.path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Job>(&bytes).ok())
        else {
            continue;
        };
        if j.gate == Gate::Signoff
            && j.ticket == t.id
            && j.project == t.project
            && j.source_sha == record.commit_sha
            && ["requested", "reviewing"].contains(&j.state.as_str())
        {
            return Ok(j);
        }
    }
    let tree = Path::new(&record.repo_path);
    let (policy, policy_error) = match crate::jury_runtime::policy(repo, &t.project) {
        Ok(p) => (p, None),
        Err(e) => (Policy::default(), Some(e)),
    };
    let scope_now = crate::jury_runtime::scope_hash(&t);
    if let Some(j) = t
        .approval
        .jury_reviews
        .iter()
        .find(|j| {
            j.gate == Gate::Signoff
                && j.state != "superseded"
                // A job parked on the owner is reusable only for the SAME
                // record and scope. A different record (XNAUT-305: a fresh
                // green verify carrying the totals the first one lacked) or a
                // changed scope must start a fresh review, or no review could
                // ever follow an escalation. The same record on the next tick
                // must NOT: the sweep offers every green record every three
                // minutes, and superseding the parked job each time wrote a
                // receipt whose revision bump made the replacement refuse
                // itself, forever (XNAUT-431, 4,905 receipts on 2026-09-24).
                // Keyed on the record, not the commit: `new_job` pins the
                // tree's HEAD as source_sha, so a record verified on an
                // earlier commit never matched the job it had opened.
                && if j.state == "owner_required" {
                    j.record_id.as_deref() == Some(record.id.as_str())
                        && j.ticket_scope_hash == scope_now
                } else {
                    j.source_sha == record.commit_sha
                }
        })
    {
        return Ok(j.clone());
    }
    let base_result = git(
        tree,
        &[
            "rev-parse",
            &format!("refs/heads/{}", policy.integration_branch),
        ],
    );
    // Review what the branch ADDS, from the point it branched. Diffing against
    // the integration tip shows every commit the tip gained since as a
    // deletion by the branch: on 2026-09-07 both reviewers refused XNAUT-305
    // for "removing" four fixes that had landed on the tip that afternoon.
    // The merge keeps both sides; the evidence must say so.
    let base = review_base(tree, base_result.as_deref().ok(), &record.commit_sha);
    let paths: Vec<String> = git(tree, &["diff", "--name-only", &base, &record.commit_sha])?
        .lines()
        .map(str::to_string)
        .collect();
    let bundle = git(
        tree,
        &[
            "show",
            &format!("{}:.xnaut/bundles/{}.md", record.commit_sha, t.id),
        ],
    )
    .unwrap_or_default();
    let scope = evidence(&t, record, &bundle, &policy);
    let mut reason = policy_error
        .or_else(|| base_result.err())
        .or(scope.reason);
    if !t.handback.as_ref().is_some_and(|h| {
        h.commits.iter().all(|sha| {
            git(
                tree,
                &["merge-base", "--is-ancestor", sha, &record.commit_sha],
            )
            .is_ok()
        })
    }) {
        reason = Some("handback commits are not ancestors of verified commit".into());
    }
    let checks = vec![drift_check(tree, &record.commit_sha)];
    let strict = crate::jury::strict_mode();
    reason = first_failure(&checks, strict)
        .map(Check::reason)
        .or(reason);
    let project_owner = crate::project_management::list_projects(repo)?
        .iter()
        .find(|p| p.key == t.project)
        .is_none_or(|p| p.owner_only);
    reason = owner_reason(
        &policy,
        t.approval.owner_only || project_owner,
        "",
        &paths,
        tree,
        None,
    )
    .or(reason);
    let diff = if reason.is_none() {
        git(tree, &["diff", "--no-ext-diff", &base, &record.commit_sha])?
    } else {
        String::new()
    };
    let input = format!(
        "Ticket:\n{}\nVerified record:\n{}\nBundle:\n{bundle}\nChanged paths:\n{}\nDiff:\n{diff}",
        crate::jury_runtime::ticket_snapshot(&t),
        serde_json::to_string_pretty(record).unwrap(),
        paths.join("\n")
    );
    let author_run = run_control::list_ids_in(registry)?
        .iter()
        .filter_map(|id| run_control::load_manifest_in(registry, id).ok())
        .filter(|r| r.kind == RunKind::Agent && r.ticket.as_deref() == Some(&t.id))
        .max_by_key(|r| r.started_at)
        .map(|r| r.run_id);
    // One live escalation per ticket. The dedupe above deliberately refuses to
    // reuse a job parked on the owner, because its inputs may have moved on.
    // Without retiring the old one, though, every sweep pass added another:
    // 285 jobs across 17 tickets by 2026-09-09, each with an inbox ask that
    // had already been archived, so the owner faced a queue of decisions that
    // no longer had anywhere to write. The newest escalation is the only one
    // anybody can act on.
    for stale in superseded_signoffs(&t) {
        write_job(root, &stale)?;
        crate::project_management::attach_jury_in(repo, &stale, None)?;
        crate::inbox::jury_archive_asks(None, &stale.id, &stale.ticket);
    }
    // Re-read: retiring the stale escalations above wrote receipts, and a job
    // born with the revision from before them starts out stale.
    let t = ticket(repo, &record.ticket_id)?;
    let mut job =
        crate::jury_runtime::new_job(Gate::Signoff, &t, tree, input, policy, author_run, None)?;
    job.record_id = Some(record.id.clone());
    // The record carries the soft misses too, or the tier is invisible and a
    // reader cannot tell an advisory observation from a refusal.
    job.checks = checks;
    // XNAUT-380: a merge that leaves declared work undone says which work, or
    // the difference between "the ticket accepted this" and "nobody looked" is
    // invisible on the record.
    if !scope.accepted.is_empty() {
        job.notes.push(format!(
            "Declared out of scope by the ticket: {}",
            scope.accepted.join("; ")
        ));
    }
    let _active = crate::jury_runtime::Active::new(&job.id);
    write_job(root, &job)?;
    drop(_serial);
    let mut job = crate::jury_runtime::run_job(app, repo, registry, root, job, reason)?;
    if job.decision == Some(Decision::Approved) {
        if let Err(e) = merge_and_verify(app, repo, registry, root, &mut job) {
            job.state = "owner_required".into();
            job.reason = e;
            job.decision = Some(Decision::Owner);
            write_job(root, &job)?;
            crate::project_management::attach_jury_in(repo, &job, Some("blocked"))?;
            crate::jury_runtime::announce_job(app, root, &mut job)?;
        }
    }
    Ok(job)
}

fn integration_ref(job: &Job) -> String {
    format!("refs/heads/{}", job.policy.integration_branch)
}
/// The integration clone stays (a later revoke reverts in it, a re-verify
/// rebuilds in it), but its build output does not: each clone carried a full
/// Rust target, 9.2 GB from two sign-offs put tron at 99% on 2026-09-08 and
/// opened the disk warning on every tick. The next build regenerates it.
/// Fast-forward the promotion branch (uat) to `sha` on the remote. Never
/// forced: if uat has moved in a way dev has not, that is a person's problem
/// and the reason says so. Without a remote there is nothing to promote to.
fn promote(tree: &Path, job: &Job, sha: &str) -> Result<(), String> {
    let target = job.policy.promote_branch.trim();
    if target.is_empty() || !has_origin(tree) {
        return Ok(());
    }
    git(tree, &["push", "origin", &format!("{sha}:refs/heads/{target}")])
        .map(|_| ())
        .map_err(|e| format!("promotion to {target} refused (not a fast-forward?): {e}"))
}

// Caller holds INTEGRATION_LOCK. A build can finish after a newer merge or
// compensation; its green result proves only its immutable snapshot.
fn promote_current(tree: &Path, job: &Job, sha: &str) -> Result<(), String> {
    if integration_base(tree, &integration_ref(job))? != sha {
        return Err("integration tip changed during build; promotion awaits verification of the current tip".into());
    }
    promote(tree, job, sha)
}

fn slim_checkout(root: &Path, job: &Job) {
    let clone = checkout(root, job);
    for scratch in ["src-tauri/target", "node_modules", ".xnaut/test-state"] {
        let _ = std::fs::remove_dir_all(clone.join(scratch));
    }
}

fn checkout(root: &Path, job: &Job) -> PathBuf {
    root.join(format!("integration-{}", job.id))
}
fn has_origin(tree: &Path) -> bool {
    git(tree, &["remote"]).is_ok_and(|r| r.lines().any(|l| l == "origin"))
}

/// Where the integration branch currently is. With a remote, that is the
/// remote's branch after a fetch: the integration branch LIVES on Forgejo,
/// and every checkout of it (the served worktree, tron's main checkout) is a
/// follower. Without one, the local ref.
pub(crate) fn integration_base(tree: &Path, reference: &str) -> Result<String, String> {
    if has_origin(tree) {
        let branch = reference.trim_start_matches("refs/heads/");
        git(tree, &["fetch", "--no-tags", "origin", branch])?;
        return git(tree, &["rev-parse", &format!("refs/remotes/origin/{branch}")]);
    }
    git(tree, &["rev-parse", reference])
}

fn publish(
    tree: &Path,
    clone: &Path,
    reference: &str,
    sha: &str,
    expected: &str,
) -> Result<(), String> {
    git(
        tree,
        &[
            "fetch",
            "--no-tags",
            clone.to_str().ok_or("non-UTF8 path")?,
            sha,
        ],
    )?;
    if has_origin(tree) {
        // Push, never move a local branch: on 2026-09-07 the sign-off refused
        // to merge XNAUT-305 because feat/xnaut-264-orphan-reap was checked
        // out in tron's main checkout, and every owner approval re-asked the
        // owner (36 items). The remote is the integration surface; a checkout
        // advances when it pulls, and its working files are never touched.
        // Compare-and-swap through force-with-lease, same guarantee as before.
        git(
            tree,
            &[
                "push",
                "origin",
                &format!("{sha}:{reference}"),
                &format!("--force-with-lease={reference}:{expected}"),
            ],
        )?;
        // The push moved refs/remotes/origin/<branch> itself.
        return Ok(());
    }
    git(tree, &["update-ref", reference, sha, expected])?;
    Ok(())
}
/// Restore only a recorded compensation for this ticket/source lineage. All
/// edits occur in the private clone, before the single guarded publication.
fn restore_recorded_reverts(
    repo: &Path, tree: &Path, clone: &Path, base: &str, job: &Job,
) -> Result<Vec<RevertRestoration>, String> {
    let current = ticket(repo, &job.ticket)?;
    let signoffs: Vec<_> = current.approval.jury_reviews.iter()
        .filter(|old| old.ticket == job.ticket && old.project == job.project)
        .filter_map(|old| old.signoff.as_ref())
        .chain(current.approval.signoff.as_ref()).chain(job.signoff.as_ref()).collect();
    let mut restored = std::collections::BTreeMap::new();
    for evidence in signoffs.iter().flat_map(|s| &s.restorations) {
        if git(tree, &["merge-base", "--is-ancestor", &evidence.restoration_sha, base]).is_ok() {
            let parents = git(tree, &["show", "-s", "--format=%P", &evidence.restoration_sha])?;
            let inverse = git(tree, &["diff", "--binary", &evidence.revert_sha, &format!("{}^", evidence.revert_sha)])?;
            if parents.split_whitespace().count() != 1 || inverse.is_empty()
                || git(tree, &["diff", "--binary", parents.trim(), &evidence.restoration_sha])? != inverse
                || git(tree, &["merge-base", "--is-ancestor", &evidence.revert_sha, parents.trim()]).is_err()
                || git(tree, &["show", "-s", "--format=%s", &evidence.revert_sha])? != format!("XNAUT jury revert {}", evidence.jury_id)
            {
                return Err("Recorded jury restoration does not match its actual Git patch; owner reconciliation required".into());
            }
            restored.insert(evidence.revert_sha.clone(), evidence.clone());
        }
    }
    let mut pending = std::collections::BTreeMap::new();
    for signoff in signoffs {
        let Some(revert) = &signoff.revert_sha else { continue };
        if restored.contains_key(revert) { continue; }
        let parents = git(tree, &["show", "-s", "--format=%P", &signoff.merge_sha])?;
        let parents: Vec<_> = parents.split_whitespace().collect();
        if parents.len() < 2 {
            return Err("Recorded jury revert has no source merge parent; restore it with owner review before remerging".into());
        }
        if git(tree, &["merge-base", "--is-ancestor", parents[1], &job.source_sha]).is_err() {
            continue;
        }
        if !signoff.revoked
            || git(tree, &["merge-base", "--is-ancestor", &signoff.merge_sha, revert]).is_err()
            || git(tree, &["merge-base", "--is-ancestor", revert, base]).is_err()
            || git(tree, &["show", "-s", "--format=%s", revert])? != format!("XNAUT jury revert {}", signoff.jury_id)
        {
            return Err("Recorded jury revert does not match current integration history; owner reconciliation required".into());
        }
        let revert_parents = git(tree, &["show", "-s", "--format=%P", revert])?;
        if revert_parents.split_whitespace().count() != 1 {
            return Err("Recorded compensation is not a single-parent revert; owner reconciliation required".into());
        }
        // A marker alone is not proof: require the exact inverse patch.
        let original = git(tree, &["diff", "--binary", parents[0], &signoff.merge_sha])?;
        let inverse = git(tree, &["diff", "--binary", revert, revert_parents.trim()])?;
        if original.is_empty() || original != inverse {
            return Err("Recorded jury compensation differs from the reviewed merge; restore it with owner review".into());
        }
        if let Some(previous) = pending.insert(revert.clone(), (*signoff).clone()) {
            if previous.merge_sha != signoff.merge_sha || previous.jury_id != signoff.jury_id {
                return Err("Conflicting jury revert receipts require owner reconciliation".into());
            }
        }
    }
    if pending.len() > 1 {
        return Err("Multiple unrestored jury reverts require explicit owner reconciliation before merging".into());
    }
    // Without a bound receipt an ancestor plus a revert marker is not proof
    // that the source contents are present (the original XNAUT-445 trap).
    let common = git(tree, &["merge-base", &job.source_sha, base])?;
    if pending.is_empty() && restored.is_empty()
        && reverted_after(tree, &common, base)
    {
        return Err("Source is already an ancestor after a jury revert, but its restoration receipt is missing; owner reconciliation required".into());
    }
    for (revert, signoff) in pending {
        if let Err(error) = git(clone, &["-c", "user.name=NautBot", "-c", "user.email=nautbot@xnaut.local", "revert", "--no-edit", &revert]) {
            let _ = git(clone, &["revert", "--abort"]);
            return Err(format!("Restoring recorded jury revert conflicts; integration was not published: {error}"));
        }
        restored.insert(revert.clone(), RevertRestoration {
            jury_id: signoff.jury_id, merge_sha: signoff.merge_sha, revert_sha: revert,
            restoration_sha: git(clone, &["rev-parse", "HEAD"])?,
        });
    }
    Ok(restored.into_values().collect())
}

pub fn merge_and_verify(
    app: Option<&AppHandle>,
    repo: &Path,
    registry: &Path,
    root: &Path,
    job: &mut Job,
) -> Result<(), String> {
    let _active = crate::jury_runtime::Active::new(&job.id);
    let publication = INTEGRATION_LOCK.lock().map_err(|_| "integration lock unavailable")?;
    let tree = PathBuf::from(&job.worktree);
    if job.decision != Some(Decision::Approved) && !job.owner_approved {
        return Err("merge requires two jury approvals".into());
    }
    if crate::jury_runtime::scope_hash(&ticket(repo, &job.ticket)?) != job.ticket_scope_hash {
        return Err("ticket scope changed after sign-off".into());
    }
    let current_policy = crate::jury_runtime::policy(repo, &job.project)?;
    if serde_json::to_string(&current_policy).unwrap()
        != serde_json::to_string(&job.policy).unwrap()
    {
        return Err("owner approval policy changed after sign-off".into());
    }
    if git(&tree, &["rev-parse", "HEAD"])? != job.source_sha {
        return Err("reviewed source changed".into());
    }
    // XNAUT-352: refuse BEFORE the merge. An empty command list used to mean
    // the loop in `verify_integration` ran zero commands and reported green,
    // so a policy nobody wrote would have signed the merge off itself.
    if job.policy.integration_commands.is_empty() {
        return Err(crate::jury::NO_POLICY.into());
    }
    let reference = integration_ref(job);
    let base = integration_base(&tree, &reference)?;
    // Without a remote the local ref is the target, and a checked-out branch
    // belongs to a live surface: never advance it behind that checkout's index
    // and working files. With a remote this does not arise; see `publish`.
    if !has_origin(&tree)
        && git(&tree, &["worktree", "list", "--porcelain"])?
            .lines()
            .any(|line| line == format!("branch {reference}"))
    {
        return Err(
            "integration branch is checked out; owner must choose an unoccupied integration branch"
                .into(),
        );
    }
    let clone = checkout(root, job);
    // Nothing has been merged yet, so a clone left by an earlier attempt of
    // this job is scratch. Left in place, `git clone` refused "destination
    // path already exists", the failure was escalated to the owner, and each
    // approval re-asked (XNAUT-255, 2026-09-08 afternoon).
    if clone.exists() {
        std::fs::remove_dir_all(&clone).map_err(|e| format!("stale integration clone: {e}"))?;
    }
    job.state = "merging".into();
    write_job(root, job)?;
    git(
        root,
        &[
            "clone",
            "--no-hardlinks",
            "--no-checkout",
            tree.to_str().ok_or("invalid tree")?,
            clone.to_str().ok_or("invalid checkout")?,
        ],
    )?;
    git(&clone, &["checkout", "--detach", &base])?;
    let restorations = restore_recorded_reverts(repo, &tree, &clone, &base, job)?;
    let restored_head = git(&clone, &["rev-parse", "HEAD"])?;
    if let Err(e) = git(
        &clone,
        &[
            "-c",
            "user.name=NautBot",
            "-c",
            "user.email=nautbot@xnaut.local",
            "merge",
            "--no-ff",
            "--no-edit",
            &job.source_sha,
        ],
    ) {
        let _ = git(&clone, &["merge", "--abort"]);
        return Err(format!("integration merge conflict: {e}"));
    }
    let mut sha = git(&clone, &["rev-parse", "HEAD"])?;
    if restored_head != base {
        // First parent must remain the pre-restoration integration tip: a red
        // verification must compensate BOTH restoration and new branch work.
        // Retain the private history as a third parent for durable evidence.
        let tree_sha = git(&clone, &["rev-parse", "HEAD^{tree}"])?;
        sha = git(&clone, &["-c", "user.name=NautBot", "-c", "user.email=nautbot@xnaut.local",
            "commit-tree", &tree_sha, "-p", &base, "-p", &job.source_sha, "-p", &sha,
            "-m", &format!("Restore jury compensation and integrate {}", job.ticket)])?;
        git(&clone, &["checkout", "--detach", &sha])?;
    }
    // A merge that produced nothing means the integration ref already holds
    // the source: an owner merged the branch by hand, or an earlier attempt
    // published and died before it recorded. That is the state sign-off
    // exists to reach, not a failure. Treating it as one sent XNAUT-354 back
    // to `owner_required` three seconds after the owner approved it, with the
    // ticket `blocked` and the same approval card shown again (2026-09-14).
    // So: sign the tip that contains the work, skip the publication there is
    // nothing to publish, and verify that tip exactly as a fresh merge would.
    let already_integrated = sha == base;
    if already_integrated
        && git(&clone, &["merge-base", "--is-ancestor", &job.source_sha, &base]).is_err()
    {
        return Err("the merge produced nothing, yet the integration ref does not contain the source".into());
    }
    job.signoff = Some(Signoff {
        jury_id: job.id.clone(),
        reviewers: job.reviews.clone(),
        scores: job
            .reviews
            .iter()
            .filter_map(|r| r.review.as_ref().map(|v| v.confidence))
            .collect(),
        merge_sha: sha.clone(),
        integration_verify_run: None,
        revoked: false,
        revert_sha: None,
        restorations,
    });
    job.state = "merge_prepared".into();
    write_job(root, job)?;
    if ticket(repo, &job.ticket)?
        .approval
        .jury_reviews
        .iter()
        .any(|j| j.id == job.id && ["revoke_requested", "revoked"].contains(&j.state.as_str()))
    {
        return Err("jury approval revoked before merge publication".into());
    }
    if !already_integrated {
        publish(&tree, &clone, &reference, &sha, &base)?;
    }
    job.state = "merged".into();
    write_job(root, job)?;
    crate::project_management::attach_jury_in(repo, job, None)?;
    // Mutation target: holding this through verify_integration serializes
    // npm/cargo again, even though the two jobs have different clones.
    drop(publication);
    verify_integration(app, repo, registry, root, job)
}

/// The version the integrated tree carries, read from its Cargo.toml: the
/// release this ticket will ship in, since dev is tagged as that version.
pub(crate) fn integrated_version(job: &Job) -> Option<String> {
    let sha = job.signoff.as_ref()?.merge_sha.clone();
    let text = git(Path::new(&job.worktree), &["show", &format!("{sha}:src-tauri/Cargo.toml")]).ok()?;
    text.lines()
        .find_map(|l| l.trim().strip_prefix("version"))
        .and_then(|rest| rest.split('"').nth(1))
        .map(str::to_string)
}

fn verifier_unavailable(exit: i32, log: &str, execution_error: Option<&str>) -> Option<&'static str> {
    if exit < 0 || execution_error.is_some() {
        return Some("the verification process could not start or finish");
    }
    let text = log.to_ascii_lowercase();
    // An observed failed assertion is still red, even if its diagnostic
    // quotes one of the environment messages below.
    if text.contains("test result: failed") || text.contains("assertion failed:")
        || regex::Regex::new(r"(?m)^\s*[1-9][0-9]* failed(?:\s|$)").unwrap().is_match(&text)
    { return None; }
    if text.contains("no space left on device") || text.contains("enospc") {
        return Some("disk space is exhausted");
    }
    if text.contains("verifier unavailable: playwright")
        || text.contains("no playwright browser is installed")
        || (text.contains("executable doesn't exist") && (text.contains("playwright") || text.contains("chromium")))
    { return Some("the Playwright browser is unavailable"); }
    if (exit == 127 && (text.contains("not found") || text.contains("not recognized")))
        || (text.contains("toolchain") && text.contains("is not installed"))
        || text.contains("rustup could not choose a version")
    { return Some("the required verification toolchain is unavailable"); }
    None
}

fn verification_log_tail(path: &Path) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(128 * 1024))).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Restart recovery may compensate a published merge only for actual bound
/// failed checks, not because its verifier never ran or its process vanished.
pub(crate) fn recorded_integration_red(registry: &Path, root: &Path, job: &Job) -> bool {
    let Some(id) = job.signoff.as_ref().and_then(|s| s.integration_verify_run.as_ref()) else { return false; };
    let Ok(run) = run_control::load_manifest_in(registry, id) else { return false; };
    if run.state != RunState::Failed || run.kind != RunKind::Verify
        || run.ticket.as_deref() != Some(job.ticket.as_str())
        || Path::new(&run.worktree_path) != checkout(root, job)
        || git(&checkout(root, job), &["rev-parse", "HEAD"]).ok().as_deref()
            != job.signoff.as_ref().map(|s| s.merge_sha.as_str())
    { return false; }
    let Ok(raw) = std::fs::read(root.join(format!("{}-{id}-integration-proof.json", job.id))) else { return false; };
    let Ok(proof) = serde_json::from_slice::<serde_json::Value>(&raw) else { return false; };
    if serde_json::from_str::<serde_json::Value>(&run.last_signal).ok().as_ref() != Some(&proof) { return false; }
    let Some(steps) = proof.as_array() else { return false; };
    if steps.is_empty() || steps.len() > job.policy.integration_commands.len() { return false; }
    for (index, step) in steps.iter().enumerate() {
        if step["command"].as_str() != Some(job.policy.integration_commands[index].as_str()) { return false; }
        let Some(exit) = step["exit_code"].as_i64().and_then(|n| i32::try_from(n).ok()) else { return false; };
        if exit == 0 { continue; }
        if exit < 0 || step["attempts"].as_array().is_some_and(|attempts|
            attempts.iter().any(|a| !a["execution_error"].is_null())) { return false; }
        if !step["unavailable"].is_null() { return false; }
        let Some(path) = step["log"].as_str().map(Path::new) else { return false; };
        let expected = format!("{}-{id}-verify-{index}", job.id);
        if path.parent() != Some(root) || !path.file_name().and_then(|p| p.to_str())
            .is_some_and(|p| p == format!("{expected}.log") || p == format!("{expected}-retry1.log"))
        { return false; }
        let Ok(log) = verification_log_tail(path) else { return false; };
        return index + 1 == steps.len() && verifier_unavailable(exit, &log, None).is_none();
    }
    false
}

pub(crate) fn park_unavailable_verifier(
    app: Option<&AppHandle>, repo: &Path, root: &Path, job: &mut Job, reason: &str,
) -> Result<(), String> {
    job.state = "owner_required".into();
    job.decision = Some(Decision::Owner);
    job.owner_approved = false;
    job.reason = format!("Verifier unavailable: {reason}. Merge retained without verified success; restore the environment and explicitly retry verification. See preserved integration proof and logs.");
    job.inbox_id = None;
    write_job(root, job)?;
    crate::project_management::attach_jury_in(repo, job, Some("blocked"))?;
    crate::jury_runtime::announce_job(app, root, job)?;
    Ok(())
}

pub fn verify_integration(
    app: Option<&AppHandle>,
    repo: &Path,
    registry: &Path,
    root: &Path,
    job: &mut Job,
) -> Result<(), String> {
    let _active = crate::jury_runtime::Active::new(&job.id);
    // The other entry to this function is the supervisor's resume path, which
    // never passed through `merge_and_verify`'s guard. Zero commands is a
    // refusal here too, never the green a zero-iteration loop used to give.
    if job.policy.integration_commands.is_empty() {
        return Err(crate::jury::NO_POLICY.into());
    }
    let clone = checkout(root, job);
    let mut run = RunManifest::requested(
        "nautbot",
        "local-verify",
        clone.to_str().ok_or("invalid checkout")?,
        Some(job.ticket.clone()),
        None,
        &[],
        run_control::now_ms(),
    );
    run.kind = RunKind::Verify;
    let run = run_control::request_in(registry, run, || Ok(()))?;
    job.signoff
        .as_mut()
        .ok_or("missing signoff")?
        .integration_verify_run = Some(run.run_id.clone());
    job.state = "verifying".into();
    write_job(root, job)?;
    let mut green = true;
    let mut results = vec![];
    let mut unavailable = None;
    for (index, command) in job.policy.integration_commands.iter().enumerate() {
      // One retry per step, as the sandbox plan has. The integration build
      // runs on the supervisor's own machine beside the fleet, and a step
      // that dies with "worker process exited unexpectedly" is contention,
      // not the merge (XNAUT-266 on dev, 2026-09-08: 6 of 133 UI tests on the
      // first pass). A step red twice is red.
      let mut exit = -1;
      let mut path = PathBuf::new();
      let mut attempts = vec![];
      for attempt in 0..2 {
        path = root.join(format!("{}-{}-verify-{index}{}.log", job.id, run.run_id, if attempt == 0 { String::new() } else { format!("-retry{attempt}") }));
        let output = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut cmd = Command::new("/bin/sh");
        cmd.current_dir(&clone)
            .args(["-c", command])
            .stdout(output.try_clone().map_err(|e| e.to_string())?)
            .stderr(output);
        isolated_test_env(&mut cmd, &clone.join(".xnaut/test-state"))?;
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let outcome = (|| -> Result<i32, String> {
            let mut child =
                crate::jury_runtime::ChildGuard(cmd.spawn().map_err(|e| e.to_string())?);
            let pid = child.id();
            run_control::update_in(registry, &run.run_id, |r| {
                r.pid = Some(pid);
                r.process_birth = run_control::process_birth(pid);
                r.output_path = Some(path.to_string_lossy().into());
                r.state = RunState::Running;
                r.last_signal = command.clone();
            })?;
            let deadline = run_control::now_ms() + 3_600_000;
            loop {
                if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                    return Ok(status.code().unwrap_or(-1));
                }
                if run_control::now_ms() > deadline {
                    let _ = Command::new("/bin/kill")
                        .args(["-KILL", "--", &format!("-{pid}")])
                        .status();
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("integration verification timed out".into());
                }
                if read_job(root, &job.id)?.state == "revoke_requested" {
                    let _ = Command::new("/bin/kill")
                        .args(["-KILL", "--", &format!("-{pid}")])
                        .status();
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("signoff revoked during verification".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        })();
        exit = outcome.as_ref().copied().unwrap_or(-1);
        let log = verification_log_tail(&path);
        let execution_error = outcome.as_ref().err().or(log.as_ref().err());
        unavailable = if exit != 0 || execution_error.is_some() {
            verifier_unavailable(exit, log.as_deref().unwrap_or(""), execution_error.map(String::as_str))
        } else { None };
        attempts.push(serde_json::json!({"attempt":attempt + 1,"exit_code":exit,"log":path,
            "execution_error":execution_error,"unavailable":unavailable}));
        if exit == 0 {
            break;
        }
        // Environment repair requires an explicit decision, not another
        // expensive build. Ordinary red checks retain their bounded retry.
        if unavailable.is_some() { break; }
      }
        green &= exit == 0 && unavailable.is_none();
        results.push(serde_json::json!({"command":command,"exit_code":exit,"log":path,"attempts":attempts,"unavailable":unavailable}));
        if !green {
            break;
        }
    }
    run_control::update_in(registry, &run.run_id, |r| {
        r.state = if green {
            RunState::Done
        } else if unavailable.is_some() {
            RunState::Blocked
        } else {
            RunState::Failed
        };
        r.last_signal = serde_json::to_string(&results).unwrap();
    })?;
    crate::project_management::write_json_atomic(
        &root.join(format!("{}-{}-integration-proof.json", job.id, run.run_id)),
        &results,
    )?;
    // Mutation target: a red integration build must actually compensate Git,
    // not merely colour the ticket red while the broken merge stays present.
    let publication = INTEGRATION_LOCK.lock().map_err(|_| "integration lock unavailable")?;
    if ticket(repo, &job.ticket)?
        .approval
        .jury_reviews
        .iter()
        .any(|j| j.id == job.id && ["revoke_requested", "revoked"].contains(&j.state.as_str()))
    {
        green = false;
        unavailable = None; // Explicit revocation still compensates the merge.
    }
    if let Some(reason) = unavailable {
        drop(publication);
        return park_unavailable_verifier(app, repo, root, job, reason);
    }
    if !green {
        job.reason = "integration build failed or revoked; see integration proof".into();
        rollback_locked(repo, root, job)?;
    } else {
        job.state = "integrated".into();
        if let Some(s) = &job.signoff {
            crate::memory::note(crate::memory::Entry {
                kind: "fix".into(),
                project: job.project.clone(),
                ticket: job.ticket.clone(),
                run_id: job.author_run.clone().unwrap_or_default(),
                text: format!("{} integrated into {} as {}", job.ticket, job.policy.integration_branch, &s.merge_sha[..s.merge_sha.len().min(8)]),
                fix: s.merge_sha.clone(),
                source: format!("signoff:{}:integrated", job.id),
                ..Default::default()
            });
        }
        if let Some(sha) = job.signoff.as_ref().map(|s| s.merge_sha.clone()) {
            if let Err(e) = promote_current(Path::new(&job.worktree), job, &sha) {
                job.reason = e;
            }
        }
        crate::inbox::jury_archive_asks(app, &job.id, &job.ticket);
        write_job(root, job)?;
        slim_checkout(root, job);
        // The ticket is integrated; its worktree has done its work. The
        // project root is the checkout the worktree was branched from.
        if let Some(project_root) = crate::project_management::list_projects(repo)
            .ok()
            .and_then(|ps| ps.into_iter().find(|p| p.key == job.project))
            .map(|p| PathBuf::from(crate::project_management::local_source_path(&p).trim()))
            .filter(|p| p.is_dir())
        {
            crate::housekeeper::reclaim_integrated(&project_root, Path::new(&job.worktree));
        }
        // A green integration ends any block this job's earlier revoke put
        // on the ticket (XNAUT-306 sat blocked with a green re-run beside it).
        let status = (ticket(repo, &job.ticket)?.status == "blocked").then_some("complete");
        crate::project_management::attach_jury_in(repo, job, status)?;
    }
    drop(publication);
    crate::inbox::jury_post(
        app,
        "notify",
        crate::inbox::PostRequest {
            project: job.project.clone(),
            ticket: Some(job.ticket.clone()),
            from: "nautbot".into(),
            title: format!(
                "{} integration {}",
                job.ticket,
                if green { "green" } else { "reverted" }
            ),
            body: format!(
                "Jury {}\n{}\n{}",
                job.id,
                job.reason,
                serde_json::to_string_pretty(&results).unwrap()
            ),
            ..Default::default()
        },
        None,
    )?;
    Ok(())
}

pub fn isolated_test_env(cmd: &mut Command, state: &Path) -> Result<(), String> {
    for (key, sub) in [
        ("TMPDIR", "tmp"),
        ("XNAUT_REGISTRY_DIR", "registry"),
        ("XNAUT_LEDGER_PATH", "ledger.jsonl"),
        ("XNAUT_SWITCHES_DIR", "switches"),
        ("XNAUT_AGENTS_PATH", "agents.json"),
        ("XNAUT_TEST_VAULT", "vault"),
        ("XNAUT_LEASE_DIR", "leases"),
        ("XNAUT_SPEND_DIR", "spend"),
        ("XNAUT_WORKLOG_DIR", "worklog"),
        ("XNAUT_WORKLOG_ROOT", "worklog"),
        ("XNAUT_VAULT_ROOT", "vault"),
        ("XNAUT_EVIDENCE_DIR", "evidence"),
        ("XNAUT_PLUGINS_PATH", "plugins.json"),
        ("XNAUT_AUTOMATIONS_PATH", "automations.json"),
        ("XNAUT_INBOX_DIR", "inbox"),
        ("XNAUT_VERIFY_DIR", "verify"),
    ] {
        let path = state.join(sub);
        std::fs::create_dir_all(if sub.ends_with(".json") || sub.ends_with(".jsonl") {
            state
        } else {
            &path
        })
        .map_err(|e| e.to_string())?;
        cmd.env(key, path);
    }
    // Git ignores a ceiling equal to its starting directory. Stop at TMPDIR's
    // parent as well, so a test probing TMPDIR cannot discover this clone.
    cmd.env("GIT_CEILING_DIRECTORIES", state)
        .env("RUST_TEST_THREADS", "1")
        .env("ZELLIJ_SOCKET_DIR", "../.xnaut/test-state/sockets");
    // XNAUT-443: the macOS cache is purgeable. Keep fleet browser binaries
    // in application data across disposable integration clones. An explicit
    // operator/CI path remains authoritative; setup installs only if missing.
    let browsers = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::loop_acceptance::platform_data_local_dir().map(|p| p.join("xnaut/playwright-browsers")))
        .ok_or("persistent Playwright browser location unavailable")?;
    cmd.env("PLAYWRIGHT_BROWSERS_PATH", browsers);
    // The PATH an agent launch gets, ahead of the app's own. A Finder-launched
    // app has a minimal PATH and the integration verify ran `npm ci` through
    // /bin/sh into "npm: command not found", which reverted an approved
    // sign-off (CHESSTRAINER-4, 2026-09-15). Same class as XNAUT-405.
    let mut path = crate::agents::runtime_path_public().unwrap_or_default();
    if let Some(inherited) = std::env::var_os("PATH") {
        if !path.is_empty() {
            path.push(':');
        }
        path.push_str(&inherited.to_string_lossy());
    }
    cmd.env("PATH", path);
    Ok(())
}
pub fn rollback(repo: &Path, root: &Path, job: &mut Job) -> Result<(), String> {
    let _publication = INTEGRATION_LOCK.lock().map_err(|_| "integration lock unavailable")?;
    rollback_locked(repo, root, job)
}

fn rollback_locked(repo: &Path, root: &Path, job: &mut Job) -> Result<(), String> {
    job.state = "rollback_requested".into();
    job.signoff.as_mut().ok_or("no signoff to revert")?.revoked = true;
    write_job(root, job)?;
    crate::project_management::attach_jury_in(repo, job, Some("blocked"))?;
    let tree = PathBuf::from(&job.worktree);
    let clone = checkout(root, job);
    // The clone is scratch and may be gone (slimmed, removed, or never made
    // on this supervisor). A revert needs a repository to revert in.
    if !clone.join(".git").exists() {
        let _ = std::fs::remove_dir_all(&clone);
        git(
            root,
            &[
                "clone",
                "--no-hardlinks",
                "--no-checkout",
                tree.to_str().ok_or("invalid tree")?,
                clone.to_str().ok_or("invalid checkout")?,
            ],
        )?;
    }
    let reference = integration_ref(job);
    let current = integration_base(&tree, &reference)?;
    let merge = job.signoff.as_ref().unwrap().merge_sha.clone();
    let marker = format!("XNAUT jury revert {}", job.id);
    let existing = git(
        &tree,
        &[
            "log",
            &current,
            "--format=%H",
            "-1",
            &format!("--grep={marker}"),
        ],
    )?;
    let reverted = if !existing.is_empty() {
        existing
    } else {
        git(
            &clone,
            &["fetch", tree.to_str().ok_or("invalid tree")?, &current],
        )?;
        // Verification can leave tracked files dirty (including the deliberate
        // failing-test proof). Preserve that diff, then clean only this private
        // disposable clone before applying the compensation commit.
        let verification_diff = git(&clone, &["diff", "--binary"])?;
        std::fs::write(
            root.join(format!("{}-pre-revert.diff", job.id)),
            verification_diff,
        )
        .map_err(|e| e.to_string())?;
        git(&clone, &["checkout", "--force", "--detach", &current])?;
        git(&clone, &["revert", "--no-commit", "-m", "1", &merge])?;
        git(
            &clone,
            &[
                "-c",
                "user.name=NautBot",
                "-c",
                "user.email=nautbot@xnaut.local",
                "commit",
                "-m",
                &marker,
            ],
        )?;
        let reverted = git(&clone, &["rev-parse", "HEAD"])?;
        job.signoff.as_mut().unwrap().revert_sha = Some(reverted.clone());
        write_job(root, job)?;
        publish(&tree, &clone, &reference, &reverted, &current)?;
        reverted
    };
    job.signoff.as_mut().unwrap().revert_sha = Some(reverted);
    job.state = "reverted".into();
    if let Some(sha) = job.signoff.as_ref().and_then(|s| s.revert_sha.clone()) {
        let _ = promote(Path::new(&job.worktree), job, &sha);
    }
    crate::inbox::jury_archive_asks(None, &job.id, &job.ticket);
    slim_checkout(root, job);
    write_job(root, job)?;
    crate::project_management::attach_jury_in(repo, job, Some("in_progress"))?;
    Ok(())
}

pub fn request_revoke(repo: &Path, root: &Path, id: &str) -> Result<(), String> {
    let _publication = INTEGRATION_LOCK.lock().map_err(|_| "integration lock unavailable")?;
    let mut job = read_job(root, id)?;
    if job.decision != Some(Decision::Approved) && !job.owner_approved {
        return Err("only an approved jury decision is revocable".into());
    }
    job.state = "revoke_requested".into();
    job.reason = "owner revoked jury approval".into();
    write_job(root, &job)?;
    crate::project_management::attach_jury_in(repo, &job, Some("blocked"))?;
    Ok(())
}
pub fn revoke(app: Option<&AppHandle>, repo: &Path, root: &Path, id: &str) -> Result<(), String> {
    let _active = crate::jury_runtime::Active::new(id);
    let mut job = read_job(root, id)?;
    if let Some(run) = &job.author_run {
        stop_then_release(
            &crate::agents::registry_dir()?,
            &crate::writer_lease::lease_dir()?,
            run,
            1000,
        )?;
    }
    if job.signoff.is_some() {
        rollback(repo, root, &mut job)?;
    }
    job.state = "revoked".into();
    write_job(root, &job)?;
    crate::project_management::attach_jury_in(repo, &job, Some("blocked"))?;
    crate::inbox::jury_post(
        app,
        "notify",
        crate::inbox::PostRequest {
            project: job.project.clone(),
            ticket: Some(job.ticket.clone()),
            from: "nautbot".into(),
            title: format!("{} jury approval revoked", job.ticket),
            body: job.reason,
            ..Default::default()
        },
        None,
    )?;
    Ok(())
}
pub fn stop_then_release(
    registry: &Path,
    leases: &Path,
    id: &str,
    grace_ms: u64,
) -> Result<(), String> {
    let run = run_control::update_in(registry, id, |r| {
        r.state = RunState::Retiring;
        r.last_signal = "revocation: stop before releasing lease".into();
    })?;
    if let Some(pid) = run.pid {
        if pid == std::process::id() {
            return Err("refusing to stop the supervisor".into());
        }
        if run_control::process_birth(pid) == run.process_birth && run.process_birth.is_some() {
            let _ = Command::new("/bin/kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        }
    }
    if let Some(session) = &run.zellij_session {
        if !session.starts_with("xnaut-") {
            return Err("invalid run session".into());
        }
        let _ = Command::new("zellij")
            .args(["delete-session", "--force", session])
            .status();
    }
    let size = || {
        run.output_path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0)
    };
    let before = size();
    std::thread::sleep(std::time::Duration::from_millis(grace_ms));
    let pid_alive = run
        .pid
        .is_some_and(|p| run_control::process_birth(p).is_some());
    let sessions = crate::zellij::live_sessions();
    let session_alive = run
        .zellij_session
        .as_ref()
        .is_some_and(|s| sessions.contains(s));
    if pid_alive || session_alive || size() != before {
        run_control::update_in(registry, id, |r| {
            r.state = RunState::Undead;
            r.last_signal = "stop proof failed; writer lease retained".into();
        })?;
        return Err("stop proof failed; writer lease retained".into());
    }
    // All three independent proofs precede this sole lease-release call.
    run_control::retire_stopped_in(registry, id, |r| {
        if r.pid
            .is_some_and(|p| run_control::process_birth(p).is_some())
            || size() != before
        {
            return Err("stop proof changed; lease retained".into());
        }
        crate::writer_lease::release_holder_in(
            leases,
            Path::new(&r.worktree_path),
            &r.agent_handle,
            r.owner_pid,
        )
    })
}

#[cfg(test)]
pub(crate) mod tests {
    #[test]
    fn the_verify_shell_gets_the_agent_runtime_path() {
        let dir = std::env::temp_dir().join(format!("xnaut-itenv-{}", std::process::id()));
        let mut cmd = Command::new("/bin/sh");
        isolated_test_env(&mut cmd, &dir).unwrap();
        let path = cmd
            .get_envs()
            .find(|(k, _)| *k == "PATH")
            .and_then(|(_, v)| v)
            .map(|v| v.to_string_lossy().into_owned())
            .expect("PATH is set on the verify shell");
        assert!(path.contains("/opt/homebrew/bin"), "homebrew missing: {path}");
        let browsers = cmd.get_envs().find(|(k, _)| *k == "PLAYWRIGHT_BROWSERS_PATH")
            .and_then(|(_, value)| value).expect("persistent browser path is set");
        let expected = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH").filter(|v| !v.is_empty())
            .unwrap_or_else(|| crate::loop_acceptance::platform_data_local_dir().unwrap().join("xnaut/playwright-browsers").into_os_string());
        assert_eq!(browsers, expected);
        let _ = std::fs::remove_dir_all(dir);
    }

    use super::*;
    #[test]
    fn a_shared_worktree_parked_on_another_ticket_is_not_drift() {
        let (_root, _control, _registry, _store, _t, job) = fixture("drift");
        let tree = Path::new(&job.worktree);
        let head = git(tree, &["rev-parse", "HEAD"]).unwrap();

        // Parked on the verified commit and clean: nothing to report.
        assert_eq!(tree_drift(tree, &head), None);

        // Parked on the verified commit with uncommitted work: real drift.
        std::fs::write(tree.join("dirty.txt"), "uncommitted").unwrap();
        assert_eq!(
            tree_drift(tree, &head).as_deref(),
            Some("verified tree has uncommitted changes")
        );

        // The same dirty worktree, asked about a DIFFERENT ticket's commit.
        // This is the shared-worktree case that escalated 285 sign-offs on
        // 2026-09-09: the tree is not this ticket's subject, so it has no
        // standing to refuse the review.
        assert_eq!(tree_drift(tree, "0000000000000000000000000000000000000000"), None);
    }

    /// XNAUT-349: the 293 escalations across 17 tickets came from an advisory
    /// observation with nowhere advisory to go. Drift is soft now: recorded,
    /// shown, and fatal to nothing unless a strict run asks for it.
    #[test]
    fn worktree_drift_is_recorded_without_refusing_the_review() {
        let (_root, _control, _registry, _store, _t, job) = fixture_with_env("drift-soft", false);
        let tree = Path::new(&job.worktree);
        let head = git(tree, &["rev-parse", "HEAD"]).unwrap();
        let clean = drift_check(tree, &head);
        assert!(clean.passed && !clean.fails(false) && !clean.fails(true));

        std::fs::write(tree.join("dirty.txt"), "uncommitted").unwrap();
        let drifted = drift_check(tree, &head);
        assert_eq!(drifted.severity, crate::jury::Severity::Soft);
        assert!(!drifted.passed, "the observation is on the record");
        assert_eq!(drifted.detail, "verified tree has uncommitted changes");
        assert!(
            !drifted.fails(false),
            "a shared worktree's drift has no standing to refuse the commit"
        );
        assert!(drifted.fails(true), "strict promotes it for a deliberate run");
        assert_eq!(
            first_failure(&[drifted], true).map(Check::reason),
            Some("verified tree is clean: verified tree has uncommitted changes".into())
        );
    }

    #[test]
    fn a_fresh_signoff_escalation_retires_the_one_it_replaces() {
        let (_root, _control, _registry, _store, mut t, job) = fixture("supersede");

        let mut parked = job.clone();
        parked.gate = Gate::Signoff;
        parked.state = "owner_required".into();
        parked.reason = "verified tree changed".into();

        let mut plan = job.clone();
        plan.id = "plan-job".into();
        plan.gate = Gate::Plan;
        plan.state = "owner_required".into();

        let mut settled = job.clone();
        settled.id = "settled-job".into();
        settled.gate = Gate::Signoff;
        settled.state = "integrated".into();

        t.approval.jury_reviews = vec![parked.clone(), plan, settled];
        let stale = superseded_signoffs(&t);

        // Only the sign-off parked on the owner is replaced. A plan gate
        // waiting on the owner is a different question, and a job that already
        // integrated is history.
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].id, parked.id);
        assert_eq!(stale[0].state, "superseded");
        assert!(stale[0].reason.contains("Superseded by a newer"));
        assert!(
            stale[0].reason.contains("verified tree changed"),
            "the original reason is kept: {}",
            stale[0].reason
        );

        // Nothing parked means nothing retired.
        t.approval.jury_reviews.retain(|j| j.state != "owner_required");
        assert!(superseded_signoffs(&t).is_empty());
    }

    #[test]
    fn a_source_already_in_the_integration_ref_is_signed_not_refused() {
        // XNAUT-354, 2026-09-14: the owner merged the branch by hand before
        // the sign-off ran. The approval has to land on the tip that already
        // holds the work, not bounce the job back to the owner.
        let (_root, control, registry, store, _t, mut job) = fixture("already-in");
        job.decision = Some(Decision::Owner);
        job.state = "owner_required".into();
        job.inbox_id = Some("in-already".into());
        write_job(&store, &job).unwrap();
        let tree = Path::new(&job.worktree);
        git(tree, &["branch", "-f", &job.policy.integration_branch, &job.source_sha]).unwrap();
        let mut approved = owner_decision(&control, &store, &job.id, "in-already", true).unwrap();
        merge_and_verify(None, &control, &registry, &store, &mut approved).unwrap();
        assert_eq!(approved.state, "integrated");
        assert_eq!(approved.signoff.as_ref().unwrap().merge_sha, job.source_sha);
    }
    #[test]
    fn explicit_owner_approval_preserves_failed_votes_and_rejects_stale_input() {
        let (_root, control, registry, store, _t, mut job) = fixture("owner");
        job.decision = Some(Decision::Owner);
        job.state = "owner_required".into();
        job.reviews[1].review = None;
        job.reviews[1].error = Some("absent".into());
        job.inbox_id = Some("in-owner".into());
        write_job(&store, &job).unwrap();
        let mut approved = owner_decision(&control, &store, &job.id, "in-owner", true).unwrap();
        assert!(approved.owner_approved);
        assert_eq!(approved.decision, Some(Decision::Owner));
        assert!(approved.reviews[1].review.is_none());
        merge_and_verify(None, &control, &registry, &store, &mut approved).unwrap();
        assert_eq!(approved.state, "integrated");
        let (_root, control, _registry, store, _t, mut stale) = fixture("owner-stale");
        stale.decision = Some(Decision::Owner);
        stale.state = "owner_required".into();
        stale.inbox_id = Some("in-stale".into());
        write_job(&store, &stale).unwrap();
        std::fs::write(Path::new(&stale.worktree).join("extra.txt"), "new commit").unwrap();
        git(Path::new(&stale.worktree), &["add", "."]).unwrap();
        git(
            Path::new(&stale.worktree),
            &["commit", "-m", "source changed"],
        )
        .unwrap();
        assert!(owner_decision(&control, &store, &stale.id, "in-stale", true).is_err());
    }
    #[test]
    fn revocation_retains_a_live_writer_lease_until_all_stop_proofs_pass() {
        use sha2::{Digest, Sha256};
        let (root, _control, registry, _store, t, job) = fixture("stop-proof");
        let leases = root.join("leases");
        std::fs::create_dir_all(&leases).unwrap();
        let marker = root.join("ready");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "trap '' TERM; : > \"$1\"; exec /bin/sleep 600",
                "jury-proof",
            ])
            .arg(&marker);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = crate::jury_runtime::ChildGuard(command.spawn().unwrap());
        for _ in 0..100 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(marker.exists());
        let mut run = RunManifest::requested(
            "codex",
            "fixture",
            &job.worktree,
            Some(t.id),
            None,
            &[],
            run_control::now_ms(),
        );
        run.pid = Some(child.id());
        run.process_birth = run_control::process_birth(child.id());
        let run = run_control::request_in(&registry, run, || Ok(())).unwrap();
        let key = format!(
            "{:x}",
            Sha256::digest(
                Path::new(&job.worktree)
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .as_bytes()
            )
        );
        let lease = leases.join(format!("{key}.json"));
        crate::project_management::write_json_atomic(
            &lease,
            &crate::writer_lease::Holder {
                handle: "codex".into(),
                pid: run.owner_pid,
                path: job.worktree.clone(),
                at: "now".into(),
            },
        )
        .unwrap();
        assert!(stop_then_release(&registry, &leases, &run.run_id, 10).is_err());
        assert!(lease.exists());
        assert_eq!(
            run_control::load_manifest_in(&registry, &run.run_id)
                .unwrap()
                .state,
            RunState::Undead
        );
        child.kill().unwrap();
        child.wait().unwrap();
        stop_then_release(&registry, &leases, &run.run_id, 10).unwrap();
        assert!(!lease.exists());
        assert_eq!(
            run_control::load_manifest_in(&registry, &run.run_id)
                .unwrap()
                .state,
            RunState::Retired
        );
    }
    pub fn fixture(
        name: &str,
    ) -> (
        PathBuf,
        PathBuf,
        PathBuf,
        PathBuf,
        crate::project_management::TicketRecord,
        Job,
    ) {
        fixture_with_env(name, true)
    }

    fn fixture_with_env(name: &str, legacy_env: bool) -> (
        PathBuf, PathBuf, PathBuf, PathBuf,
        crate::project_management::TicketRecord, Job,
    ) {
        let root = std::env::temp_dir().join(format!("xnaut-jury-{name}-{}", uuid::Uuid::new_v4()));
        // Never the real inbox: three "XNAUT-930 Plan" asks from this fixture
        // reached the owner's Mesh on tron (2026-09-08) and could not be
        // answered, because their ticket repo was a temp dir long gone.
        crate::inbox::use_test_inbox(root.join("inbox"));
        if legacy_env {
            std::env::set_var("XNAUT_INBOX_DIR", root.join("inbox"));
        }
        // The ticket's design document, in an isolated vault, already carrying
        // its Shipped section: sign-off requires one (2026-09-08).
        if legacy_env {
            std::env::set_var("XNAUT_TEST_VAULT", root.join("vault"));
        }
        crate::vault::use_test_vault(root.join("vault"));
        // Never the developer's own ~/.config/xnaut: a fixture that removes a
        // project's approval.toml has to actually end up with no policy, and
        // on any machine that has run xNAUT the home copy answered instead.
        crate::jury_runtime::use_test_config(root.join("config"));
        let doc = root.join("vault/work/XNAUT/Development/features/proof.md");
        std::fs::create_dir_all(doc.parent().unwrap()).unwrap();
        std::fs::write(&doc, "---\nAuthor: fixture\nLast modified: 2026-09-08\n---\n\n# Proof\n\n## Shipped XNAUT-930\nfeature.txt changed, with tests.\n").unwrap();
        let source = root.join("source");
        let control = root.join("control");
        let registry = root.join("registry");
        let store = registry.join("jury");
        for dir in [&source, &control, &store] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::create_dir_all(control.join("events")).unwrap();
        for dir in [&source, &control] {
            git(dir, &["init", "-b", "agent/codex/xnaut-930"]).unwrap();
            git(dir, &["config", "user.name", "Jury proof"]).unwrap();
            git(dir, &["config", "user.email", "jury-proof@xnaut.local"]).unwrap();
        }
        std::fs::write(source.join("feature.txt"), "baseline\n").unwrap();
        git(&source, &["add", "."]).unwrap();
        git(&source, &["commit", "-m", "baseline"]).unwrap();
        git(&source, &["branch", "dev"]).unwrap();
        std::fs::write(source.join("feature.txt"), "reviewed implementation\n").unwrap();
        git(&source, &["add", "."]).unwrap();
        git(&source, &["commit", "-m", "implement XNAUT-930"]).unwrap();
        let t:crate::project_management::TicketRecord=serde_json::from_value(serde_json::json!({"id":"XNAUT-930","project":"XNAUT","title":"Implement the isolated proof feature","type":"feature","status":"complete","priority":"high","owner":"codex","body":"Change feature.txt in the assigned worktree, with tests.","documentation":["work:XNAUT/Development/features/proof.md"],"revision":1,"created_at":"2026-09-07T00:00:00Z","updated_at":"2026-09-07T00:00:00Z"})).unwrap();
        let path = control.join("projects/XNAUT/tickets/XNAUT-930.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        crate::project_management::write_json_atomic(&path, &t).unwrap();
        crate::project_management::write_json_atomic(&control.join("projects/XNAUT/project.json"),&serde_json::json!({"key":"XNAUT","name":"XNAUT jury proof","created_at":"2026-09-07T00:00:00Z"})).unwrap();
        git(&control, &["add", "."]).unwrap();
        git(&control, &["commit", "-m", "dispatched proof ticket"]).unwrap();
        let mut policy = crate::jury::tests::policy();
        policy.integration_commands = vec!["test -f feature.txt".into()];
        std::fs::write(
            control.join("projects/XNAUT/approval.toml"),
            toml::to_string(&policy).unwrap(),
        )
        .unwrap();
        let mut job = crate::jury_runtime::new_job(
            Gate::Signoff,
            &t,
            &source,
            "Evidence for isolated test".into(),
            policy,
            None,
            None,
        )
        .unwrap();
        job.reviews = crate::jury::tests::reviews(&job.input_hash);
        job.decision = Some(Decision::Approved);
        job.state = "decided".into();
        (root, control, registry, store, t, job)
    }

    fn concurrent_build_proof(red_first: bool, revoke_first: bool) {
        let (root, control, registry, store, t, mut first) = fixture_with_env("parallel", false);
        let tree = PathBuf::from(&first.worktree);
        let bare = root.join("origin.git");
        git(&root, &["init", "--bare", bare.to_str().unwrap()]).unwrap();
        git(&tree, &["remote", "add", "origin", bare.to_str().unwrap()]).unwrap();
        git(&tree, &["push", "origin", "dev:dev", "dev:uat"]).unwrap();
        let baseline = git(&bare, &["rev-parse", "uat"]).unwrap();
        let second_tree = root.join("second");
        git(&tree, &["worktree", "add", "-b", "agent/codex/xnaut-931", second_tree.to_str().unwrap(), "dev"]).unwrap();
        std::fs::write(second_tree.join("other.txt"), "independent change\n").unwrap();
        git(&second_tree, &["add", "other.txt"]).unwrap();
        git(&second_tree, &["commit", "-m", "implement independent change"]).unwrap();
        let mut second_ticket = t.clone();
        second_ticket.id = "XNAUT-931".into();
        crate::project_management::write_json_atomic(
            &control.join("projects/XNAUT/tickets/XNAUT-931.json"), &second_ticket,
        ).unwrap();
        git(&control, &["add", "."]).unwrap();
        git(&control, &["commit", "-m", "dispatch second ticket"]).unwrap();
        // Each real verifier announces its clone, then waits for the test to
        // release it. A bounded timeout also makes lock mutations fail safely.
        first.policy.integration_commands = vec![
            "pwd > \"$PWD.started\"; n=0; while ! test -f \"$PWD.release\"; do n=$((n+1)); test $n -lt 200 || exit 18; sleep 0.05; done; test ! -f \"$PWD.red\"".into()
        ];
        first.policy.promote_branch = "uat".into();
        std::fs::write(control.join("projects/XNAUT/approval.toml"), toml::to_string(&first.policy).unwrap()).unwrap();
        let mut second = crate::jury_runtime::new_job(
            Gate::Signoff, &second_ticket, &second_tree, "second evidence".into(),
            first.policy.clone(), None, None,
        ).unwrap();
        second.decision = Some(Decision::Approved);
        second.reviews = crate::jury::tests::reviews(&second.input_hash);
        second.state = "decided".into();
        let first_clone = checkout(&store, &first);
        let second_clone = checkout(&store, &second);
        let marker = |clone: &Path, suffix: &str| PathBuf::from(format!("{}.{suffix}", clone.display()));
        if red_first {
            std::fs::write(marker(&first_clone, "red"), "red").unwrap();
        }
        // Generous on purpose: under the full suite a thousand tests share
        // the machine and the second clone can take well over five seconds
        // to reach its command. Release is gated on a marker this test
        // writes, so a longer wait cannot manufacture a false overlap.
        let entered = |clone: &Path| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            while std::time::Instant::now() < deadline {
                if marker(clone, "started").exists() { return true; }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        };
        let first_id = first.id.clone();
        let (first, second, overlap) = std::thread::scope(|scope| {
            let spawn = |mut job: Job| {
                let root = &root;
                let control = &control;
                let registry = &registry;
                let store = &store;
                scope.spawn(move || {
                    crate::inbox::use_test_inbox(root.join("inbox"));
                    let result = merge_and_verify(None, control, registry, store, &mut job);
                    (job, result)
                })
            };
            let a = spawn(first);
            let a_entered = entered(&first_clone);
            let b = spawn(second);
            let overlap = a_entered && entered(&second_clone);
            if overlap && revoke_first {
                request_revoke(&control, &store, &first_id).unwrap();
            }
            std::fs::write(marker(&first_clone, "release"), "go").unwrap();
            let (first, result) = a.join().unwrap();
            // Release both before asserting, so a failed mutation cannot
            // strand a worker or poison the process-wide publication mutex.
            let promotion_while_second_builds = git(&bare, &["rev-parse", "uat"]).unwrap();
            std::fs::write(marker(&second_clone, "release"), "go").unwrap();
            let (second, second_result) = b.join().unwrap();
            result.unwrap();
            second_result.unwrap();
            if overlap && !red_first && !revoke_first {
                assert_eq!(promotion_while_second_builds, baseline, "a late green snapshot cannot promote over the newer pending tip");
            }
            (first, second, overlap)
        });
        assert!(overlap, "both integration commands must start before either is released");
        assert_ne!(first_clone, second_clone);
        // The command writes its resolved working directory. On macOS the
        // temp dir is a symlink, /var -> /private/var, so compare against
        // the resolved path or the assertion holds on Linux only.
        let resolved = |p: &Path| std::fs::canonicalize(p).unwrap().to_string_lossy().into_owned();
        assert_eq!(std::fs::read_to_string(marker(&first_clone, "started")).unwrap().trim(), resolved(&first_clone));
        assert_eq!(std::fs::read_to_string(marker(&second_clone, "started")).unwrap().trim(), resolved(&second_clone));
        let first_signoff = first.signoff.as_ref().unwrap();
        let second_signoff = second.signoff.as_ref().unwrap();
        assert_ne!(first_signoff.integration_verify_run, second_signoff.integration_verify_run);
        assert_eq!(second.state, "integrated");
        assert_eq!(git(&bare, &["show", "dev:other.txt"]).unwrap(), "independent change");
        if red_first || revoke_first {
            assert_eq!(first.state, "reverted");
            assert!(first_signoff.revoked);
            assert_eq!(ticket(&control, &first.ticket).unwrap().status, "in_progress");
            assert_eq!(git(&bare, &["show", "dev:feature.txt"]).unwrap(), "baseline");
            assert!(second.reason.contains("tip changed"), "{}", second.reason);
            let before = git(&bare, &["rev-parse", "dev"]).unwrap();
            let mut retry = first.clone();
            rollback(&control, &store, &mut retry).unwrap();
            assert_eq!(git(&bare, &["rev-parse", "dev"]).unwrap(), before, "compensation remains idempotent");
        } else {
            assert_eq!(first.state, "integrated");
            assert_eq!(git(&bare, &["show", "dev:feature.txt"]).unwrap(), "reviewed implementation");
        }
        assert_eq!(git(&bare, &["rev-parse", "uat"]).unwrap(), git(&bare, &["rev-parse", "dev"]).unwrap());
    }

    #[test]
    fn two_green_integrations_build_concurrently_in_private_clones() {
        concurrent_build_proof(false, false);
    }

    #[test]
    fn red_concurrent_integration_reverts_only_its_change() {
        concurrent_build_proof(true, false);
    }

    #[test]
    fn revocation_during_concurrent_verification_compensates_before_late_green() {
        concurrent_build_proof(false, true);
    }

    #[test]
    fn revoked_approval_cannot_publish_a_merge() {
        let (_root, control, registry, store, _t, mut job) = fixture_with_env("revoke-before", false);
        let tree = PathBuf::from(&job.worktree);
        let before = git(&tree, &["rev-parse", "dev"]).unwrap();
        write_job(&store, &job).unwrap();
        request_revoke(&control, &store, &job.id).unwrap();
        let error = merge_and_verify(None, &control, &registry, &store, &mut job).unwrap_err();
        assert!(error.contains("revoked before merge"), "{error}");
        assert_eq!(git(&tree, &["rev-parse", "dev"]).unwrap(), before);
    }

    /// XNAUT-352: the fleet's other 43 projects have no approval.toml, and
    /// until today they were judged under xNAUT's example file. A project
    /// without one now refuses to sign off, naming the missing policy, and
    /// runs nobody else's build commands on the way there.
    #[test]
    fn a_project_without_an_approval_toml_refuses_instead_of_running_our_build() {
        let (_root, control, registry, store, _t, mut job) =
            fixture_with_env("no-policy", false);
        std::fs::remove_file(control.join("projects/XNAUT/approval.toml")).unwrap();
        let tree = PathBuf::from(&job.worktree);
        let before = git(&tree, &["rev-parse", "dev"]).unwrap();
        job.policy = crate::jury::Policy::default();
        write_job(&store, &job).unwrap();
        let error = merge_and_verify(None, &control, &registry, &store, &mut job).unwrap_err();
        assert_eq!(error, crate::jury::NO_POLICY, "the refusal names the policy");
        assert_eq!(
            git(&tree, &["rev-parse", "dev"]).unwrap(),
            before,
            "nothing may be merged under a policy nobody wrote"
        );
        // And the resume path, which never passes through the merge guard:
        // zero commands is a refusal, not the green a zero-iteration loop gave.
        let error = verify_integration(None, &control, &registry, &store, &mut job).unwrap_err();
        assert_eq!(error, crate::jury::NO_POLICY);
        assert!(job.signoff.is_none(), "nothing was signed");
    }

    #[test]
    fn admission_reuses_a_review_before_it_is_attached_to_the_ticket() {
        let (_root, control, registry, store, _t, mut job) = fixture_with_env("admission", false);
        job.state = "requested".into();
        job.decision = None;
        write_job(&store, &job).unwrap();
        let record = serde_json::from_value(serde_json::json!({
            "id":"v", "run_id":"r", "ticket_id":job.ticket, "project":job.project,
            "repo_path":job.worktree, "commit_sha":job.source_sha, "provider_kind":"local",
            "sandbox_id":"", "public_url":"", "status":"passed", "steps":[],
            "log_dir":"", "video_path":null, "created_at":"today", "updated_at":"today"
        })).unwrap();
        let reused = start(None, &control, &registry, &store, &record).unwrap();
        assert_eq!(reused.id, job.id);
        assert!(ticket(&control, &job.ticket).unwrap().approval.jury_reviews.is_empty());
        // On the first sign-off there is no jury directory yet. Missing
        // evidence still escalates normally instead of failing admission I/O.
        std::fs::remove_dir_all(&store).unwrap();
        let fresh = start(None, &control, &registry, &store, &record).unwrap();
        assert_eq!(fresh.state, "owner_required");
        assert!(store.join(format!("{}.json", fresh.id)).exists());
    }

    #[test]
    fn a_parked_signoff_is_reused_for_the_same_record_and_reopened_by_a_new_one() {
        // XNAUT-431. The sweep offers the same green record every tick.
        let (_root, control, registry, store, _t, mut job) = fixture_with_env("parked", false);
        job.state = "owner_required".into();
        job.decision = Some(Decision::Owner);
        job.reason = "integration merge conflict".into();
        job.record_id = Some("v".into());
        write_job(&store, &job).unwrap();
        crate::project_management::attach_jury_in(&control, &job, None).unwrap();
        let record = |id: &str| -> crate::sandbox_verify::VerifyRecord {
            serde_json::from_value(serde_json::json!({
                "id":id, "run_id":"r", "ticket_id":job.ticket, "project":job.project,
                "repo_path":job.worktree, "commit_sha":job.source_sha, "provider_kind":"local",
                "sandbox_id":"", "public_url":"", "status":"passed", "steps":[],
                "log_dir":"", "video_path":null, "created_at":"today", "updated_at":"today"
            })).unwrap()
        };
        let same = start(None, &control, &registry, &store, &record("v")).unwrap();
        assert_eq!(same.id, job.id, "the parked job is the answer, not a replacement");
        assert_eq!(same.state, "owner_required");
        let jobs = ticket(&control, &job.ticket).unwrap().approval.jury_reviews;
        assert_eq!(jobs.len(), 1, "no receipt, no supersede: {:?}", jobs.iter().map(|j| &j.state).collect::<Vec<_>>());
        // A different record is a new question and retires the parked one.
        let fresh = start(None, &control, &registry, &store, &record("v2")).unwrap();
        assert_ne!(fresh.id, job.id);
        assert_eq!(fresh.record_id.as_deref(), Some("v2"));
        let jobs = ticket(&control, &job.ticket).unwrap().approval.jury_reviews;
        assert_eq!(jobs.iter().find(|j| j.id == job.id).unwrap().state, "superseded");
    }

    #[test]
    fn private_clone_test_environment_stops_git_discovery_at_tmpdir_parent() {
        let (_root, _control, _registry, _store, _t, job) = fixture_with_env("git-ceiling", false);
        let state = Path::new(&job.worktree).join(".xnaut/test-state");
        let mut command = Command::new("git");
        isolated_test_env(&mut command, &state).unwrap();
        let output = command.current_dir(state.join("tmp"))
            .args(["rev-parse", "--show-toplevel"]).output().unwrap();
        assert!(!output.status.success(), "a non-repository fixture must not discover the integration clone");
        assert!(String::from_utf8_lossy(&output.stderr).contains("not a git repository"));
    }
    #[test]
    fn red_integration_reverts_git_and_returns_ticket_to_author() {
        let (_root, control, registry, store, _, mut job) = fixture("red");
        let tree = PathBuf::from(&job.worktree);
        let reference = integration_ref(&job);
        let before = git(&tree, &["rev-parse", &reference]).unwrap();
        job.policy.integration_commands =
            vec!["printf 'dirty verification\\n' > feature.txt; exit 17".into()];
        std::fs::write(
            control.join("projects/XNAUT/approval.toml"),
            toml::to_string(&job.policy).unwrap(),
        )
        .unwrap();
        merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
        assert_eq!(
            job.state, "reverted",
            "a red build must compensate, not keep the merge"
        );
        assert!(job.signoff.as_ref().unwrap().revoked);
        assert!(job.signoff.as_ref().unwrap().revert_sha.is_some());
        let after = git(&tree, &["rev-parse", &reference]).unwrap();
        assert_ne!(before, after, "revert is a commit, never reset");
        assert!(
            git(&tree, &["diff", &before, &after]).unwrap().is_empty(),
            "broken change remains integrated"
        );
        let returned = ticket(&control, &job.ticket).unwrap();
        assert_eq!(returned.status, "in_progress");
        assert_eq!(returned.owner.as_deref(), Some("codex"));
        rollback(&control, &store, &mut job).unwrap();
        assert_eq!(
            git(&tree, &["rev-parse", &reference]).unwrap(),
            after,
            "recovery duplicated revert"
        );
    }

    #[test]
    fn unavailable_integration_verifier_keeps_merge_and_parks_without_restart_loop() {
        for (name, command) in [
            ("browser", "printf 'No Playwright browser is installed\\n' >&2; exit 1"),
            ("disk", "printf 'No space left on device\\n' >&2; exit 1"),
            ("toolchain", "printf 'cargo: command not found\\n' >&2; exit 127"),
        ] {
            let (_root, control, registry, store, _, mut job) = fixture_with_env(name, false);
            job.policy.integration_commands = vec![command.into()];
            std::fs::write(control.join("projects/XNAUT/approval.toml"), toml::to_string(&job.policy).unwrap()).unwrap();
            merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
            assert_eq!(job.state, "owner_required", "{name}");
            assert!(job.reason.starts_with("Verifier unavailable:"));
            assert!(job.inbox_id.is_some());
            let signoff = job.signoff.as_ref().unwrap();
            assert!(!signoff.revoked && signoff.revert_sha.is_none());
            assert_eq!(git(Path::new(&job.worktree), &["show", "dev:feature.txt"]).unwrap(), "reviewed implementation");
            let run = signoff.integration_verify_run.as_ref().unwrap();
            assert_eq!(run_control::load_manifest_in(&registry, run).unwrap().state, RunState::Blocked);
            let proof: serde_json::Value = serde_json::from_slice(&std::fs::read(store.join(format!("{}-{run}-integration-proof.json", job.id))).unwrap()).unwrap();
            assert_eq!(proof[0]["attempts"].as_array().unwrap().len(), 1);
            assert_ne!(proof[0]["exit_code"], 0);
            let revision = ticket(&control, &job.ticket).unwrap().revision;
            let ids = run_control::list_ids_in(&registry).unwrap();
            for _ in 0..2 { crate::jury_runtime::reconcile(None, &control, &registry, &store).unwrap(); }
            assert_eq!(run_control::list_ids_in(&registry).unwrap(), ids);
            assert_eq!(ticket(&control, &job.ticket).unwrap().revision, revision);
            assert_eq!(ticket(&control, &job.ticket).unwrap().status, "blocked");
        }
        assert_eq!(verifier_unavailable(1, "test result: FAILED.\nNo Playwright browser is installed", None), None,
            "a failed test quoting the environment message remains red");
    }

    #[test]
    fn interrupted_published_integration_compensates_only_bound_actual_red_checks() {
        for case in ["actual_red", "environment", "wrong_run", "legacy_unknown_exit", "execution_error"] {
            let (_root, control, registry, store, _, mut job) = fixture_with_env(case, false);
            job.policy.integration_commands = vec!["printf 'actual failing command\\n'; exit 17".into()];
            std::fs::write(control.join("projects/XNAUT/approval.toml"), toml::to_string(&job.policy).unwrap()).unwrap();
            merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
            assert_eq!(job.state, "reverted");
            // Reconstruct the interruption immediately before compensation in
            // this owned Git fixture; retain the real executor's failed proof.
            let signoff = job.signoff.as_mut().unwrap();
            let merged = signoff.merge_sha.clone();
            let run_id = signoff.integration_verify_run.clone().unwrap();
            signoff.revoked = false;
            signoff.revert_sha = None;
            git(Path::new(&job.worktree), &["update-ref", "refs/heads/dev", &merged]).unwrap();
            git(&checkout(&store, &job), &["checkout", "--detach", &merged]).unwrap();
            job.state = "merged".into();
            if case == "environment" {
                std::fs::write(store.join(format!("{}-{run_id}-verify-0-retry1.log", job.id)), "No Playwright browser is installed\n").unwrap();
            } else if case == "wrong_run" {
                run_control::update_in(&registry, &run_id, |r| r.ticket = Some("XNAUT-999".into())).unwrap();
            } else if ["legacy_unknown_exit", "execution_error"].contains(&case) {
                let path = store.join(format!("{}-{run_id}-integration-proof.json", job.id));
                let mut proof: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                if case == "legacy_unknown_exit" {
                    proof[0]["exit_code"] = (-1).into();
                    proof[0].as_object_mut().unwrap().remove("attempts");
                    std::fs::write(store.join(format!("{}-{run_id}-verify-0-retry1.log", job.id)), "").unwrap();
                } else {
                    proof[0]["attempts"][0]["execution_error"] = "process did not finish".into();
                }
                crate::project_management::write_json_atomic(&path, &proof).unwrap();
                run_control::update_in(&registry, &run_id, |r| r.last_signal = serde_json::to_string(&proof).unwrap()).unwrap();
            }
            write_job(&store, &job).unwrap();
            crate::jury_runtime::reconcile(None, &control, &registry, &store).unwrap();
            let recovered = read_job(&store, &job.id).unwrap();
            assert_eq!(recovered.state, if case == "actual_red" { "reverted" } else { "owner_required" }, "{case}");
            assert_eq!(git(Path::new(&job.worktree), &["show", "dev:feature.txt"]).unwrap(),
                if case == "actual_red" { "baseline" } else { "reviewed implementation" });
        }
    }

    fn fresh_remerge(control: &Path, previous: &Job, command: &str) -> Job {
        let current = ticket(control, &previous.ticket).unwrap();
        let mut policy = previous.policy.clone();
        policy.integration_commands = vec![command.into()];
        std::fs::write(control.join("projects/XNAUT/approval.toml"), toml::to_string(&policy).unwrap()).unwrap();
        let mut job = crate::jury_runtime::new_job(Gate::Signoff, &current, Path::new(&previous.worktree),
            "Fresh independent approval after compensation".into(), policy, None, None).unwrap();
        job.reviews = crate::jury::tests::reviews(&job.input_hash);
        job.decision = Some(Decision::Approved);
        job.state = "signed".into();
        job
    }

    #[test]
    fn remerge_restores_recorded_revert_content_and_keeps_compensation_history() {
        for (new_work, red_after_restore) in [(false, false), (true, false), (true, true)] {
            let (_root, control, registry, store, _, mut first) = fixture_with_env("remerge-content", false);
            merge_and_verify(None, &control, &registry, &store, &mut first).unwrap();
            rollback(&control, &store, &mut first).unwrap();
            let revert = first.signoff.as_ref().unwrap().revert_sha.clone().unwrap();
            let tree = Path::new(&first.worktree);
            assert_eq!(git(tree, &["show", "dev:feature.txt"]).unwrap(), "baseline");
            // Cover both unchanged branch reapproval and a repair adding new
            // work; ancestry alone used to drop the original feature in both.
            if new_work {
                std::fs::write(tree.join("repair.txt"), "follow-up repair\n").unwrap();
                git(tree, &["add", "repair.txt"]).unwrap();
                git(tree, &["commit", "-m", "repair reviewed branch"]).unwrap();
            }
            let mut next = fresh_remerge(&control, &first, if red_after_restore { "exit 17" } else {
                "test \"$(cat feature.txt)\" = 'reviewed implementation'"
            });
            merge_and_verify(None, &control, &registry, &store, &mut next).unwrap();
            let evidence = &next.signoff.as_ref().unwrap().restorations;
            assert_eq!(evidence.len(), 1);
            assert_eq!(evidence[0].revert_sha, revert);
            assert_eq!(read_job(&store, &first.id).unwrap().signoff.unwrap().revert_sha.as_deref(), Some(revert.as_str()));
            assert_eq!(git(tree, &["show", "dev:feature.txt"]).unwrap(), if red_after_restore { "baseline" } else { "reviewed implementation" });
            assert_eq!(next.state, if red_after_restore { "reverted" } else { "integrated" });
            assert_eq!(git(tree, &["cat-file", "-e", "dev:repair.txt"]).is_ok(), new_work && !red_after_restore,
                "rollback must remove both restored and newly merged work");
            if !red_after_restore {
                let tip = git(tree, &["rev-parse", "dev"]).unwrap();
                let mut again = fresh_remerge(&control, &next, "test -f feature.txt");
                merge_and_verify(None, &control, &registry, &store, &mut again).unwrap();
                assert_eq!(git(tree, &["rev-parse", "dev"]).unwrap(), tip, "already restored work must not be applied again");
            }
        }
    }

    #[test]
    fn remerge_refuses_conflicting_or_unproved_restoration_without_publication() {
        for case in ["conflict", "missing_receipt", "missing_receipt_with_repair", "wrong_revert"] {
            let (_root, control, registry, store, _, mut first) = fixture_with_env(case, false);
            merge_and_verify(None, &control, &registry, &store, &mut first).unwrap();
            rollback(&control, &store, &mut first).unwrap();
            let tree = Path::new(&first.worktree);
            if case == "conflict" {
                git(tree, &["checkout", "dev"]).unwrap();
                std::fs::write(tree.join("feature.txt"), "owner changed integration\n").unwrap();
                git(tree, &["commit", "-am", "owner integration change"]).unwrap();
                git(tree, &["checkout", "agent/codex/xnaut-930"]).unwrap();
            } else {
                let mut current = ticket(&control, &first.ticket).unwrap();
                current.approval.jury_reviews.clear();
                current.approval.signoff = if case.starts_with("missing_receipt") { None } else {
                    let mut receipt = first.signoff.clone().unwrap();
                    receipt.revert_sha = Some(receipt.merge_sha.clone()); Some(receipt)
                };
                crate::project_management::write_json_atomic(&control.join("projects/XNAUT/tickets/XNAUT-930.json"), &current).unwrap();
                if case == "missing_receipt_with_repair" {
                    std::fs::write(tree.join("repair.txt"), "new branch work\n").unwrap();
                    git(tree, &["add", "repair.txt"]).unwrap();
                    git(tree, &["commit", "-m", "repair after lost receipt"]).unwrap();
                }
            }
            let before = git(tree, &["rev-parse", "dev"]).unwrap();
            let author = git(tree, &["rev-parse", "HEAD"]).unwrap();
            let mut next = fresh_remerge(&control, &first, "test -f feature.txt");
            let error = merge_and_verify(None, &control, &registry, &store, &mut next).unwrap_err();
            assert!(error.contains("conflict") || error.contains("reconciliation"), "{case}: {error}");
            assert_eq!(git(tree, &["rev-parse", "dev"]).unwrap(), before);
            assert_eq!(git(tree, &["rev-parse", "HEAD"]).unwrap(), author);
            assert_eq!(git(tree, &["status", "--porcelain"]).unwrap(), "");
        }
    }
    #[test]
    fn the_reviewed_diff_starts_at_the_merge_base_not_the_moving_tip() {
        let (_root, _control, _registry, _store, _t, job) = fixture("merge-base");
        let tree = PathBuf::from(&job.worktree);
        let branch = job.policy.integration_branch.clone();
        let fork = git(&tree, &["rev-parse", &branch]).unwrap();
        // The tip moves on after the branch point.
        git(&tree, &["checkout", "-q", &branch]).unwrap();
        std::fs::write(tree.join("tip-only.txt"), "landed after the branch\n").unwrap();
        git(&tree, &["add", "tip-only.txt"]).unwrap();
        git(&tree, &["commit", "-q", "-m", "a fix on the tip"]).unwrap();
        let tip = git(&tree, &["rev-parse", &branch]).unwrap();
        git(&tree, &["checkout", "-q", &job.source_sha]).unwrap();
        let base = review_base(&tree, Some(&tip), &job.source_sha);
        assert_eq!(base, fork, "review from the fork point, not the moving tip");
        let diff = git(&tree, &["diff", "--name-only", &base, &job.source_sha]).unwrap();
        assert!(!diff.contains("tip-only.txt"), "{diff}");
        assert!(diff.contains("feature.txt"), "{diff}");
    }
    #[test]
    fn with_a_remote_the_merge_is_pushed_and_a_checked_out_integration_branch_is_left_alone() {
        // 2026-09-07: feat/xnaut-264-orphan-reap was checked out in tron's main
        // checkout, so every owner approval of XNAUT-305 was refused with
        // "integration branch is checked out" and re-asked: 36 inbox items.
        let (root, control, registry, store, _, mut job) = fixture("remote");
        let tree = PathBuf::from(&job.worktree);
        let branch = job.policy.integration_branch.clone();
        // A bare origin holding the integration branch, and a second checkout
        // with that branch checked out, as tron has.
        let bare = root.join("origin.git");
        git(&tree, &["init", "--bare", bare.to_str().unwrap()]).unwrap();
        git(&tree, &["remote", "add", "origin", bare.to_str().unwrap()]).unwrap();
        git(&tree, &["push", "origin", &format!("refs/heads/{branch}:refs/heads/{branch}")]).unwrap();
        let other = root.join("other-checkout");
        git(&tree, &["worktree", "add", other.to_str().unwrap(), &branch]).unwrap();
        let local_before = git(&tree, &["rev-parse", &format!("refs/heads/{branch}")]).unwrap();

        job.policy.promote_branch = "uat".into();
        // The compiled-in default promotes nowhere since XNAUT-352, so a test
        // that promotes has to say so in the owner policy like any other.
        std::fs::write(
            control.join("projects/XNAUT/approval.toml"),
            toml::to_string(&job.policy).unwrap(),
        )
        .unwrap();
        git(&tree, &["push", "origin", &format!("refs/heads/{branch}:refs/heads/uat")]).unwrap();
        // A clone left by an earlier attempt must not block the merge.
        std::fs::create_dir_all(checkout(&store, &job).join("leftover")).unwrap();
        merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
        assert_ne!(job.state, "owner_required", "{}", job.reason);
        let merged = job.signoff.as_ref().unwrap().merge_sha.clone();
        let on_origin = git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]).unwrap();
        assert_eq!(on_origin, merged, "the merge must land on the remote");
        assert_eq!(job.state, "integrated", "{}", job.reason);
        assert_eq!(
            git(&bare, &["rev-parse", "refs/heads/uat"]).unwrap(),
            merged,
            "a green integration promotes uat to the same merge"
        );
        assert_eq!(
            git(&tree, &["rev-parse", &format!("refs/heads/{branch}")]).unwrap(),
            local_before,
            "a checked-out local branch is never moved underneath its working files"
        );
    }
    #[test]
    fn a_verifier_killed_by_a_restart_is_run_again_not_rolled_back() {
        // 2026-09-08 09:31: an install restarted the app while XNAUT-306's
        // integration build ran. Reconcile found the verifier gone, treated
        // absence as a red build, revoked a merge that had passed 974 Rust
        // and 133 UI tests, and the ticket sat blocked with a good merge.
        let (_root, control, registry, store, _, mut job) = fixture("restart-verify");
        merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
        assert_eq!(job.state, "integrated");
        let merge = job.signoff.as_ref().unwrap().merge_sha.clone();
        let run_id = job.signoff.as_ref().unwrap().integration_verify_run.clone().unwrap();
        // Back to "verifying", with a verifier that no longer exists.
        job.state = "verifying".into();
        write_job(&store, &job).unwrap();
        run_control::update_in(&registry, &run_id, |r| {
            r.pid = Some(1);
            r.process_birth = Some("not-the-birth-of-pid-1".into());
        })
        .unwrap();
        crate::project_management::attach_jury_in(&control, &job, Some("blocked")).unwrap();
        assert_eq!(ticket(&control, &job.ticket).unwrap().status, "blocked");
        crate::jury_runtime::reconcile(None, &control, &registry, &store).unwrap();
        let after: Job = serde_json::from_slice(&std::fs::read(store.join(format!("{}.json", job.id))).unwrap()).unwrap();
        assert_eq!(after.state, "integrated", "{}", after.reason);
        assert_eq!(ticket(&control, &job.ticket).unwrap().status, "complete", "a green re-run lifts the block");
        assert!(!after.signoff.as_ref().unwrap().revoked, "a restart is not a red build");
        assert_eq!(after.signoff.as_ref().unwrap().merge_sha, merge);
        assert_ne!(after.signoff.as_ref().unwrap().integration_verify_run.as_deref(), Some(run_id.as_str()), "the build ran again");
    }

    #[test]
    fn an_integration_build_that_keeps_dying_stops_and_asks() {
        // Re-running a verifier killed by the supervisor's own restart is
        // right. Re-running one that keeps dying is a loop that costs a full
        // build a turn: on tron on 2026-09-09 reconcile started eleven of them
        // between 16:03 and 21:29 for XNAUT-75, a ticket that had merged and
        // promoted at 17:50, while the volume sat at 100% full.
        let (_root, control, registry, store, _, mut job) = fixture("verify-loop");
        merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
        assert_eq!(job.state, "integrated");
        assert_eq!(job.verify_restarts, 0);

        let kill = |job: &Job| {
            let run = job.signoff.as_ref().unwrap().integration_verify_run.clone().unwrap();
            run_control::update_in(&registry, &run, |r| {
                r.pid = Some(1);
                r.process_birth = Some("not-the-birth-of-pid-1".into());
            })
            .unwrap();
        };

        for expected in 1..=crate::jury_runtime::MAX_VERIFY_RESTARTS {
            let mut current = read_job(&store, &job.id).unwrap();
            current.state = "verifying".into();
            write_job(&store, &current).unwrap();
            kill(&current);
            crate::jury_runtime::reconcile(None, &control, &registry, &store).unwrap();
            let after = read_job(&store, &job.id).unwrap();
            assert_eq!(after.verify_restarts, expected, "{}", after.reason);
            assert_eq!(after.state, "integrated", "a re-run that passes still integrates");
        }

        // Budget spent: the next death is a question, not a twelfth build.
        let mut current = read_job(&store, &job.id).unwrap();
        current.state = "verifying".into();
        write_job(&store, &current).unwrap();
        kill(&current);
        crate::jury_runtime::reconcile(None, &control, &registry, &store).unwrap();
        let after = read_job(&store, &job.id).unwrap();
        assert_eq!(after.state, "owner_required");
        assert_eq!(after.verify_restarts, crate::jury_runtime::MAX_VERIFY_RESTARTS);
        assert!(
            after.reason.contains("something outside this merge"),
            "{}",
            after.reason
        );
        assert!(
            !after.signoff.as_ref().unwrap().revoked,
            "a build that never ran is still not a red build"
        );
    }

    #[test]
    fn sign_off_wants_a_shipped_section_in_the_tickets_document() {
        let with = vec![("Development/features/x.md".to_string(), Some("# Doc\n\n## Shipped XNAUT-9\nwhat, how, snippets\n".to_string()))];
        assert!(shipped_section_missing("XNAUT-9", &with).is_none());
        let without = vec![("Development/features/x.md".to_string(), Some("# Doc\n".to_string()))];
        assert!(shipped_section_missing("XNAUT-9", &without).unwrap().contains("Shipped XNAUT-9"));
        let other = vec![("Development/features/x.md".to_string(), Some("## Shipped XNAUT-8\n".to_string()))];
        assert!(shipped_section_missing("XNAUT-9", &other).is_some(), "another ticket's section does not count");
        assert!(shipped_section_missing("XNAUT-9", &[]).unwrap().contains("links no design document"));
    }

    #[test]
    fn work_already_on_the_integration_branch_is_not_a_decision() {
        // Turning the gate on swept the historical board: 100 tickets that
        // completed long before the jury existed each got a job and each
        // escalated, 298 asks in ninety minutes (2026-09-09). Sign-off decides
        // a merge; if the merge happened there is nothing to decide.
        let (_root, _control, _registry, _store, _, job) = fixture("already-merged");
        let tree = PathBuf::from(&job.worktree);
        let branch = job.policy.integration_branch.clone();
        let record = |sha: &str| -> crate::sandbox_verify::VerifyRecord {
            serde_json::from_value(serde_json::json!({
                "id":"v","run_id":"r","ticket_id":job.ticket,"project":"XNAUT",
                "repo_path":job.worktree,"commit_sha":sha,"provider_kind":"local","sandbox_id":"",
                "public_url":"","status":"passed","steps":[],"log_dir":"","video_path":null,
                "created_at":"today","updated_at":"today"
            })).unwrap()
        };

        // The branch commit is unmerged work: a real decision.
        assert!(nothing_to_sign(&record(&job.source_sha)).is_none(), "unmerged work must still be reviewed");

        // The integration tip is by definition already integrated.
        let tip = git(&tree, &["rev-parse", &branch]).unwrap();
        let why = nothing_to_sign(&record(&tip)).expect("already-merged work must not ask");
        assert!(why.contains("already on"), "{why}");

        // A commit this repository has never seen is not reviewable either.
        let why = nothing_to_sign(&record("0000000000000000000000000000000000000000"))
            .expect("an unknown commit must not ask");
        assert!(why.contains("does not have"), "{why}");

        // And no commit at all, as before.
        assert!(nothing_to_sign(&record("")).unwrap().contains("no commit to review"));

        // XNAUT-411: a merge the app reverted leaves the commit an ancestor of
        // the branch while its changes are gone. That is not "already on dev",
        // it is work waiting to be integrated again.
        let revert = git(
            &tree,
            &[
                "commit-tree",
                &format!("{tip}^{{tree}}"),
                "-p",
                &tip,
                "-m",
                "XNAUT jury revert some-job-id",
            ],
        )
        .unwrap();
        git(&tree, &["update-ref", &format!("refs/heads/{branch}"), &revert]).unwrap();
        assert!(
            nothing_to_sign(&record(&tip)).is_none(),
            "a reverted merge must be reviewable again"
        );
    }

    #[test]
    fn a_record_with_no_commit_is_not_reviewable_and_never_escalates() {
        // XNAUT-252, 2026-09-09: verify records from before the registry carry
        // a null commit sha. Scheduling on one diffed against an empty
        // revision and posted "sign-off needs owner intervention" carrying
        // "git diff --name-only  : ambiguous argument ''", every tick, on a
        // ticket closed weeks earlier.
        let (_root, control, registry, store, _, job) = fixture("no-commit");
        let mut record: crate::sandbox_verify::VerifyRecord = serde_json::from_value(serde_json::json!({
            "id":"v","run_id":"r","ticket_id":job.ticket,"project":"XNAUT",
            "repo_path":job.worktree,"commit_sha":"","provider_kind":"local","sandbox_id":"",
            "public_url":"","status":"passed","steps":[],"log_dir":"","video_path":null,
            "created_at":"today","updated_at":"today"
        })).unwrap();
        let refused = start(None, &control, &registry, &store, &record)
            .expect_err("a record with no commit must be refused, not reviewed");
        assert!(refused.contains("no commit to review"), "{refused}");
        // And a real sha still gets through to the ticket check.
        record.commit_sha = job.source_sha.clone();
        let ok = start(None, &control, &registry, &store, &record);
        assert!(ok.is_ok(), "a record with a commit must still start: {ok:?}");
    }

    #[test]
    fn green_integration_has_verify_record_and_revocation_is_reversible() {
        let (_root, control, registry, store, _, mut job) = fixture("green");
        merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
        assert_eq!(job.state, "integrated");
        assert!(checkout(&store, &job).exists(), "the clone stays for a later revoke or re-verify");
        assert!(!checkout(&store, &job).join("src-tauri/target").exists(), "its build output does not");
        let id = job
            .signoff
            .as_ref()
            .unwrap()
            .integration_verify_run
            .as_ref()
            .unwrap();
        let run = run_control::load_manifest_in(&registry, id).unwrap();
        assert_eq!(run.kind, RunKind::Verify);
        assert_eq!(run.state, RunState::Done);
        request_revoke(&control, &store, &job.id).unwrap();
        assert_eq!(ticket(&control, &job.ticket).unwrap().status, "blocked");
        revoke(None, &control, &store, &job.id).unwrap();
        let t = ticket(&control, &job.ticket).unwrap();
        assert_eq!(t.status, "blocked");
        assert!(t.approval.signoff.unwrap().revert_sha.is_some());
    }
    #[test]
    fn conflict_does_not_publish_any_integration_commit() {
        let (_root, control, registry, store, _, mut job) = fixture("conflict");
        let tree = PathBuf::from(&job.worktree);
        git(&tree, &["checkout", "dev"]).unwrap();
        std::fs::write(tree.join("feature.txt"), "conflict\n").unwrap();
        git(&tree, &["commit", "-am", "integration changed"]).unwrap();
        let base = git(&tree, &["rev-parse", "HEAD"]).unwrap();
        git(&tree, &["checkout", "agent/codex/xnaut-930"]).unwrap();
        assert!(
            merge_and_verify(None, &control, &registry, &store, &mut job)
                .unwrap_err()
                .contains("conflict")
        );
        assert_eq!(
            git(&tree, &["rev-parse", &integration_ref(&job)]).unwrap(),
            base
        );
    }
    #[test]
    fn bundle_totals_must_match_actual_verification_output() {
        let (_root, _control, _registry, _store, mut t, job) = fixture("totals");
        let mut record:crate::sandbox_verify::VerifyRecord=serde_json::from_value(serde_json::json!({"id":"v","run_id":"r","ticket_id":t.id,"project":"XNAUT","repo_path":job.worktree,"commit_sha":job.source_sha,"provider_kind":"local","sandbox_id":"","public_url":"","status":"passed","steps":[{"name":"all","command":"cargo test && npx playwright test","exit_code":0,"log_tail":"test result: ok. 7 passed; 0 failed; 1 ignored\n  3 passed (1s)"}],"log_dir":"","video_path":null,"created_at":"today","updated_at":"today"})).unwrap();
        t.handback = Some(crate::handback::Handback {
            run_id: None,
            ticket: t.id.clone(),
            summary: "Feature implemented".into(),
            files_changed: vec!["feature.txt".into()],
            commits: vec![job.source_sha],
            how_verified: "cargo test and npx playwright test".into(),
            verify_record_id: Some("v".into()),
            not_finished: Some("nothing".into()),
            confidence: crate::handback::Confidence::High,
            from: "codex".into(),
            submitted_at: "today".into(),
        });
        let totals = test_totals(&record.steps[0].log_tail);
        let bundle = format!("XNAUT_TEST_TOTALS={totals}");
        let policy = crate::jury::tests::policy();
        assert!(evidence(&t, &record, &bundle, &policy).reason.is_none());
        assert!(
            evidence(&t, &record, &bundle.replace("7", "8"), &policy)
                .reason
                .is_some()
        );
        record.not_evidence = true;
        assert!(evidence(&t, &record, &bundle, &policy).reason.is_some());
    }

    /// XNAUT-380. The same green ticket, with work the agent deliberately left
    /// out: what the ticket declared is accepted and named, what it did not is
    /// the only thing the owner is asked about, and the switch brings back the
    /// behaviour that produced the click on 2026-09-14.
    #[test]
    fn declared_unfinished_work_is_accepted_and_the_rest_still_escalates() {
        let (_root, _control, _registry, _store, mut t, job) = fixture("notdone");
        let record:crate::sandbox_verify::VerifyRecord=serde_json::from_value(serde_json::json!({"id":"v","run_id":"r","ticket_id":t.id,"project":"XNAUT","repo_path":job.worktree,"commit_sha":job.source_sha,"provider_kind":"local","sandbox_id":"","public_url":"","status":"passed","steps":[{"name":"all","command":"cargo test && npx playwright test","exit_code":0,"log_tail":"test result: ok. 7 passed; 0 failed; 1 ignored\n  3 passed (1s)"}],"log_dir":"","video_path":null,"created_at":"today","updated_at":"today"})).unwrap();
        let bundle = format!("XNAUT_TEST_TOTALS={}", test_totals(&record.steps[0].log_tail));
        t.body = format!(
            "{}\n\nNOT IN SCOPE\n- the Windows leg, which waits on a signing certificate\n",
            t.body
        );
        let handback = |left: &str| crate::handback::Handback {
            run_id: None,
            ticket: t.id.clone(),
            summary: "Feature implemented".into(),
            files_changed: vec!["feature.txt".into()],
            commits: vec![job.source_sha.clone()],
            how_verified: "cargo test --bin xnaut: 7 passed".into(),
            verify_record_id: Some("v".into()),
            not_finished: Some(left.into()),
            confidence: crate::handback::Confidence::High,
            from: "codex".into(),
            submitted_at: "today".into(),
        };
        let policy = crate::jury::tests::policy();

        // Covered: no card, and the record names what it accepted.
        t.handback = Some(handback("the Windows leg is untested; waits on a signing cert"));
        let covered = evidence(&t, &record, &bundle, &policy);
        assert_eq!(covered.reason, None);
        assert_eq!(covered.accepted, vec!["the Windows leg is untested".to_string()]);

        // Uncovered: escalates, naming only the item nothing declared.
        t.handback = Some(handback("the SSH importer is not written"));
        let open = evidence(&t, &record, &bundle, &policy).reason.unwrap();
        assert_eq!(
            open,
            "unfinished work has not been accepted by the ticket: the SSH importer is not written"
        );

        // Mixed: the declared half is accepted, the card is about the other.
        t.handback = Some(handback(
            "- the Windows leg is untested\n- the SSH importer is not written",
        ));
        let mixed = evidence(&t, &record, &bundle, &policy);
        assert_eq!(
            mixed.reason.unwrap(),
            "unfinished work has not been accepted by the ticket: the SSH importer is not written"
        );
        assert_eq!(mixed.accepted, vec!["the Windows leg is untested".to_string()]);

        // The switch keeps the old behaviour for a project that wants it.
        t.handback = Some(handback("the Windows leg is untested"));
        let strict = crate::jury::Policy {
            escalate_every_not_done: true,
            ..policy.clone()
        };
        assert_eq!(
            evidence(&t, &record, &bundle, &strict).reason.unwrap(),
            "unfinished work has not been accepted by the ticket"
        );
        // And the hand-written acceptance line still overrides even that.
        t.body = format!("{}\nAccepted not_finished: the Windows leg is untested", t.body);
        assert_eq!(evidence(&t, &record, &bundle, &strict).reason, None);
    }
}
