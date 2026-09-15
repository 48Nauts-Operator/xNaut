// XNAUT-319. The swarm lane: a branch where the gates are off and errors are
// expected.
//
// Cursor's harness post (walked through 2026-09-10) makes one claim we could
// not answer: demanding total correctness before every commit serialises
// everything, and one typo halts the system. That is exactly what happened
// here on 2026-09-09. Their answer is a small, steady, tolerated error rate.
//
// That is the wrong posture for dev, which feeds uat and releases. It is the
// right EXPERIMENT for a branch that reaches nobody. So: a named branch,
// `swarm`, on which the jury and sign-off do not run, a worker merges its own
// green build directly, and a red build is fixed by the next worker rather
// than reverted. Nothing reaches dev from here without a person asking; there
// is no code path in this file that writes the integration branch, and
// `merge` refuses outright if the two are ever configured to be the same ref.
//
// A ticket is on the lane when it carries the `swarm` tag, which is the
// owner's to set (André, 2026-09-08) and which XNAUT-316's children already
// inherit from their parent — so dividing a swarm ticket fills the lane
// without anybody re-deciding where the work belongs.
//
// The publish sequence below is deliberately a lane-local copy of the one in
// `jury_signoff` rather than a shared helper: the whole value of this module
// is that the ref it advances can be read off one screen. A shared publisher
// that takes the branch as an argument would put "never touches dev" one
// caller away from a typo.

use crate::jury::Gate;
use crate::jury_runtime::git;
use crate::project_management::TicketRecord;
use crate::sandbox_verify::VerifyRecord;
use std::path::{Path, PathBuf};

/// The tag that puts a ticket on the lane.
pub const LANE_TAG: &str = "swarm";

/// The branch the lane merges to. Never dev, never uat, never main.
pub const BRANCH: &str = "swarm";

/// Which lane a ticket's work is measured and merged on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// Every merge reviewed by the jury and signed off. The default.
    Audited,
    /// No gate. The experiment.
    Swarm,
}

impl Lane {
    pub fn name(self) -> &'static str {
        match self {
            Lane::Audited => "audited",
            Lane::Swarm => "swarm",
        }
    }
}

pub fn lane_of_tags(tags: &[String]) -> Lane {
    if tags.iter().any(|t| t.trim().eq_ignore_ascii_case(LANE_TAG)) {
        Lane::Swarm
    } else {
        Lane::Audited
    }
}

pub fn lane_of(t: &TicketRecord) -> Lane {
    lane_of_tags(&t.tags)
}

pub fn is_swarm(t: &TicketRecord) -> bool {
    lane_of(t) == Lane::Swarm
}

/// Why the jury does not open for this ticket, or `None` if it should.
///
/// The sweep already routes swarm work away from the jury, so reaching this
/// means somebody called the sign-off path directly. It is still a refusal
/// rather than a silent skip: a gate that can be opened by a second caller is
/// not off, it is off in one place.
pub fn no_jury_reason(t: &TicketRecord) -> Option<String> {
    is_swarm(t).then(|| {
        format!(
            "{} is on the {BRANCH} lane; the jury and sign-off do not run there",
            t.id
        )
    })
}

/// The id a plan verdict carries when the lane answered it instead of a jury.
/// Not a job id: there is no job, and a caller that polls for one should find
/// nothing rather than a fabricated decision.
pub const LANE_VERDICT_ID: &str = "swarm-lane";

/// The lane ticket the live agent run in `worktree` is working, or `None` when
/// that run is on the audited lane, has no ticket, or does not exist.
///
/// The plan gate asks this before it opens anything. Without it a lane worker
/// posting a plan gets the full two-reviewer gate, and every failure in it
/// parks on the owner — which is the serialisation this lane exists to remove,
/// arriving through the one door that was left open.
pub fn lane_ticket_for(worktree: &Path, session: Option<&str>) -> Option<String> {
    let registry = crate::agents::registry_dir().ok()?;
    let id = run_ticket_for(&registry, worktree, session)?;
    let repo = crate::project_management::repo_now().ok()?;
    let t = crate::jury_runtime::ticket(&repo, &id).ok()?;
    is_swarm(&t).then_some(id)
}

/// The ticket the newest live agent run in `worktree` is working, matched the
/// same way the plan gate matches its own run: by canonical worktree, and by
/// session when the caller supplied one.
pub fn run_ticket_for(registry: &Path, worktree: &Path, session: Option<&str>) -> Option<String> {
    let tree = worktree.canonicalize().ok()?;
    crate::run_control::list_ids_in(registry)
        .ok()?
        .iter()
        .filter_map(|id| crate::run_control::load_manifest_in(registry, id).ok())
        .filter(|r| {
            r.kind == crate::run_control::RunKind::Agent
                && !r.state.terminal()
                && Path::new(&r.worktree_path).canonicalize().ok().as_ref() == Some(&tree)
                && session.is_none_or(|s| {
                    r.pty_session.as_deref() == Some(s) || r.zellij_session.as_deref() == Some(s)
                })
        })
        .max_by_key(|r| r.started_at)?
        .ticket
}

/// Every ticket on the lane, by id, across all projects on the board.
pub fn lane_tickets(repo: &Path) -> std::collections::HashSet<String> {
    crate::project_management::ticket_list_in(repo, None)
        .unwrap_or_default()
        .iter()
        .filter(|t| is_swarm(t))
        .map(|t| t.id.clone())
        .collect()
}

/// What the sweep does with one green verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Open a sign-off job: the audited lane.
    Jury,
    /// Merge it onto the lane branch, no gate.
    Swarm,
    /// Already handled, or not finished.
    Nothing,
}

/// Route one passed verification. The audited arm is the condition the sweep
/// carried inline before this ticket, moved here so both lanes are decided in
/// the same place and the swarm arm cannot be reached by accident.
pub fn route(tickets: &[TicketRecord], record: &VerifyRecord) -> Route {
    let Some(t) = tickets.iter().find(|t| t.id == record.ticket_id) else {
        return Route::Nothing;
    };
    // Green IS the approval: a passing record is what moves a ticket to
    // `complete` (sandbox_verify::PASSED_STATUS). Both lanes wait for it.
    if t.status != "complete" {
        return Route::Nothing;
    }
    if is_swarm(t) {
        // Before any of the audited lane's dedupe, so a swarm ticket that
        // somehow carries an old jury review still never gets a new one.
        return Route::Swarm;
    }
    // A superseded review is a retired job (Re-review, XNAUT-399): it must
    // not block the fresh one it made room for. A parked (owner_required)
    // review still blocks, or every tick would open another escalation.
    if t.approval.signoff.is_some()
        || t.approval
            .jury_reviews
            .iter()
            .any(|j| j.gate == Gate::Signoff && j.source_sha == record.commit_sha && j.state != "superseded")
    {
        return Route::Nothing;
    }
    Route::Jury
}

/// What a lane merge did.
#[derive(Debug, Clone, PartialEq)]
pub enum Merge {
    /// The lane already contains this work. The sweep sees the same green
    /// record every tick, so this is the common answer, not an error.
    AlreadyOn { lane: String },
    /// A merge commit was published on the lane.
    Merged { lane: String, before: String },
    /// The lane branch did not exist and now starts at this work.
    Opened { lane: String },
}

impl Merge {
    pub fn lane_sha(&self) -> &str {
        match self {
            Merge::AlreadyOn { lane } | Merge::Merged { lane, .. } | Merge::Opened { lane } => lane,
        }
    }
}

fn has_origin(tree: &Path) -> bool {
    git(tree, &["remote"]).is_ok_and(|r| r.lines().any(|l| l == "origin"))
}

/// Where the lane currently is: the remote's branch after a fetch when there
/// is a remote, the local ref otherwise, and `None` when the lane has never
/// been opened.
fn lane_tip(tree: &Path, branch: &str) -> Option<String> {
    if has_origin(tree) {
        let _ = git(tree, &["fetch", "--no-tags", "origin", branch]);
        return git(tree, &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/origin/{branch}")]).ok();
    }
    git(tree, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).ok()
}

fn is_ancestor(tree: &Path, sha: &str, of: &str) -> bool {
    git(tree, &["merge-base", "--is-ancestor", sha, of]).is_ok()
}

/// Merge `sha` onto the lane branch. Nothing is reviewed, nothing is
/// promoted, and nothing is verified afterwards: a red lane build is the next
/// worker's to fix, which is the entire point of the experiment. A revert
/// here would reintroduce the serialisation the lane exists to measure.
///
/// `integration` is passed only so this can refuse to run when the lane and
/// the integration branch have been configured to the same ref. It is never
/// written.
pub fn merge(
    tree: &Path,
    root: &Path,
    ticket: &str,
    sha: &str,
    integration: &str,
) -> Result<Merge, String> {
    let branch = BRANCH;
    if integration.trim() == branch {
        return Err(format!(
            "the {branch} lane cannot be the integration branch; nothing reaches {integration} without a person asking"
        ));
    }
    if git(tree, &["cat-file", "-e", &format!("{sha}^{{commit}}")]).is_err() {
        return Err(format!("{ticket} names a commit this repository does not have"));
    }
    let reference = format!("refs/heads/{branch}");
    match lane_tip(tree, branch) {
        Some(tip) if is_ancestor(tree, sha, &tip) => Ok(Merge::AlreadyOn { lane: tip }),
        Some(tip) => {
            let merged = merge_in_clone(tree, root, ticket, sha, &tip)?;
            publish(tree, root, ticket, &reference, &merged, &tip)?;
            Ok(Merge::Merged { lane: merged, before: tip })
        }
        // The lane's first commit. The branch is CREATED at this work; the
        // integration branch is read nowhere in this arm, so opening the lane
        // cannot move dev even by a fast-forward.
        None => {
            publish(tree, root, ticket, &reference, sha, "")?;
            Ok(Merge::Opened { lane: sha.to_string() })
        }
    }
}

fn clone_dir(root: &Path, ticket: &str) -> PathBuf {
    root.join(format!("swarm-{ticket}"))
}

/// Build the merge commit in a scratch clone, exactly as the audited lane
/// does, so a conflict never touches a checkout somebody is working in.
fn merge_in_clone(
    tree: &Path,
    root: &Path,
    ticket: &str,
    sha: &str,
    base: &str,
) -> Result<String, String> {
    let clone = clone_dir(root, ticket);
    if clone.exists() {
        std::fs::remove_dir_all(&clone).map_err(|e| format!("stale lane clone: {e}"))?;
    }
    std::fs::create_dir_all(root).map_err(|e| format!("lane clone root: {e}"))?;
    git(
        root,
        &[
            "clone",
            "--no-hardlinks",
            "--no-checkout",
            tree.to_str().ok_or("invalid tree")?,
            clone.to_str().ok_or("invalid lane clone")?,
        ],
    )?;
    // The lane tip may only exist on the origin of `tree`, which the clone
    // has as a second hop; fetch both endpoints explicitly.
    let _ = git(&clone, &["fetch", "--no-tags", "origin", base]);
    let _ = git(&clone, &["fetch", "--no-tags", "origin", sha]);
    git(&clone, &["checkout", "--detach", base])?;
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
            "-m",
            &format!("Merge {ticket} into {BRANCH}"),
            sha,
        ],
    ) {
        let _ = git(&clone, &["merge", "--abort"]);
        return Err(format!("{BRANCH} lane merge conflict: {e}"));
    }
    git(&clone, &["rev-parse", "HEAD"])
}

/// Move the lane ref, and only the lane ref. `expected` is the tip the merge
/// was built on, so a lane that moved underneath us loses the race instead of
/// overwriting the winner — the same compare-and-swap the audited lane uses.
/// An empty `expected` opens a branch that does not exist yet.
fn publish(
    tree: &Path,
    root: &Path,
    ticket: &str,
    reference: &str,
    sha: &str,
    expected: &str,
) -> Result<(), String> {
    let clone = clone_dir(root, ticket);
    if clone.is_dir() {
        git(tree, &["fetch", "--no-tags", clone.to_str().ok_or("non-UTF8 path")?, sha])?;
    }
    if has_origin(tree) {
        // Opening the lane is a plain create: there is no tip to lease
        // against, and a create that races another worker is rejected as a
        // non-fast-forward, which is the outcome we want anyway.
        if expected.is_empty() {
            git(tree, &["push", "origin", &format!("{sha}:{reference}")])?;
            return Ok(());
        }
        git(
            tree,
            &[
                "push",
                "origin",
                &format!("{sha}:{reference}"),
                &format!("--force-with-lease={reference}:{expected}"),
            ],
        )?;
        return Ok(());
    }
    if expected.is_empty() {
        git(tree, &["update-ref", reference, sha])?;
    } else {
        git(tree, &["update-ref", reference, sha, expected])?;
    }
    Ok(())
}

/// Merge one green record onto the lane, in the background, from the sweep.
/// Failures are a ledger row, not an owner escalation: the lane's whole
/// premise is that its errors are tolerated and fixed by the next worker.
pub fn schedule(record: VerifyRecord) {
    tauri::async_runtime::spawn_blocking(move || {
        let work = || -> Result<Merge, String> {
            let root = crate::jury_runtime::store()?;
            let branch = crate::jury_runtime::policy_integration_branch();
            merge(
                Path::new(&record.repo_path),
                &root,
                &record.ticket_id,
                &record.commit_sha,
                &branch,
            )
        };
        match work() {
            Ok(Merge::AlreadyOn { .. }) => {}
            Ok(done) => crate::ledger::record(
                "swarm_merge",
                "nautbot",
                &record.ticket_id,
                &format!("{BRANCH} at {}", done.lane_sha()),
            ),
            Err(error) => crate::ledger::record("swarm_merge_failed", "nautbot", &record.ticket_id, &error),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn ticket(id: &str, status: &str, tags: &[&str]) -> TicketRecord {
        let mut t: TicketRecord = serde_json::from_value(serde_json::json!({
            "id": id, "project": "XNAUT", "title": "t", "type": "feature", "status": status,
            "priority": "medium", "owner": "claude", "documentation": [], "body": "",
            "source_id": "", "revision": 1, "created_at": "", "updated_at": ""
        }))
        .expect("fixture ticket");
        t.tags = tags.iter().map(|s| s.to_string()).collect();
        t
    }

    fn record(ticket: &str, sha: &str) -> VerifyRecord {
        let mut r: VerifyRecord = serde_json::from_value(serde_json::json!({
            "id": "v1", "run_id": "r1", "ticket_id": ticket, "project": "XNAUT",
            "repo_path": "", "provider_kind": "local", "sandbox_id": "", "public_url": "",
            "status": "passed", "steps": [], "log_dir": "", "video_path": null,
            "created_at": "2026-09-10T12:00:00Z", "updated_at": "2026-09-10T12:00:00Z"
        }))
        .expect("fixture record");
        r.commit_sha = sha.into();
        r
    }

    #[test]
    fn a_swarm_lane_ticket_never_opens_a_jury_job() {
        let tickets = vec![
            ticket("XNAUT-1", "complete", &["swarm"]),
            ticket("XNAUT-2", "complete", &[]),
            ticket("XNAUT-3", "in_progress", &["swarm"]),
        ];
        assert_eq!(route(&tickets, &record("XNAUT-1", "abc")), Route::Swarm);
        assert_eq!(route(&tickets, &record("XNAUT-2", "abc")), Route::Jury);
        assert_eq!(route(&tickets, &record("XNAUT-3", "abc")), Route::Nothing);
        assert_eq!(route(&tickets, &record("XNAUT-9", "abc")), Route::Nothing);
        // The tag is the lane, whatever case the owner typed it in, and it
        // travels to a child ticket because XNAUT-316 copies the parent's tags.
        assert_eq!(lane_of_tags(&["Swarm".into()]), Lane::Swarm);
        assert_eq!(lane_of_tags(&["ios".into(), "swarm".into()]), Lane::Swarm);
        assert_eq!(lane_of_tags(&["swarming".into()]), Lane::Audited);
        // And the gate itself refuses, not only the router.
        assert!(no_jury_reason(&tickets[0]).unwrap().contains("do not run there"));
        assert!(no_jury_reason(&tickets[1]).is_none());
    }

    #[test]
    fn the_signoff_gate_refuses_a_swarm_ticket_through_the_real_start_path() {
        let (root, control, registry, store, mut t, _job) =
            crate::jury_signoff::tests::fixture("swarm-no-jury");
        let tree = root.join("source");
        let sha = git(&tree, &["rev-parse", "HEAD"]).unwrap();
        t.tags = vec![LANE_TAG.into()];
        crate::project_management::write_json_atomic(
            &control.join("projects/XNAUT/tickets/XNAUT-930.json"),
            &t,
        )
        .unwrap();
        let mut rec = record("XNAUT-930", &sha);
        rec.repo_path = tree.to_string_lossy().into();
        let error = crate::jury_signoff::start(None, &control, &registry, &store, &rec)
            .expect_err("the jury does not run on the lane");
        assert!(error.contains("swarm lane"), "{error}");
        // Nothing was written: no job file, no review on the ticket.
        assert!(
            std::fs::read_dir(&store).map(|d| d.flatten().count()).unwrap_or(0) == 0,
            "the lane opened a jury job"
        );
        let after = crate::project_management::ticket_list_in(&control, None).unwrap();
        assert!(after[0].approval.jury_reviews.is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_merge_on_swarm_never_touches_dev() {
        let (root, _control, _registry, _store, _t, _job) =
            crate::jury_signoff::tests::fixture("swarm-merge");
        let tree = root.join("source");
        let sha = git(&tree, &["rev-parse", "HEAD"]).unwrap();
        let dev_before = git(&tree, &["rev-parse", "refs/heads/dev"]).unwrap();

        // The lane does not exist yet: opening it starts at this work and
        // reads dev nowhere.
        let opened = merge(&tree, &root, "XNAUT-930", &sha, "dev").unwrap();
        assert_eq!(opened, Merge::Opened { lane: sha.clone() });
        assert_eq!(git(&tree, &["rev-parse", "refs/heads/dev"]).unwrap(), dev_before, "dev moved");
        assert_eq!(git(&tree, &["rev-parse", "refs/heads/swarm"]).unwrap(), sha);

        // A second worker's green build merges on top, still without dev.
        std::fs::write(tree.join("feature.txt"), "the next worker\n").unwrap();
        git(&tree, &["add", "."]).unwrap();
        git(&tree, &["commit", "-m", "XNAUT-931"]).unwrap();
        let next = git(&tree, &["rev-parse", "HEAD"]).unwrap();
        let merged = merge(&tree, &root, "XNAUT-931", &next, "dev").unwrap();
        let lane = merged.lane_sha().to_string();
        assert!(matches!(merged, Merge::Merged { ref before, .. } if *before == sha));
        assert_eq!(git(&tree, &["rev-parse", "refs/heads/dev"]).unwrap(), dev_before);
        assert_eq!(git(&tree, &["rev-parse", "refs/heads/swarm"]).unwrap(), lane);
        assert!(is_ancestor(&tree, &next, &lane));
        assert!(git(&tree, &["rev-parse", "--verify", "--quiet", "refs/heads/uat"]).is_err());

        // Seeing the same green record again is not a second merge, and
        // nothing is reverted: a red lane build stays for the next worker.
        assert_eq!(
            merge(&tree, &root, "XNAUT-931", &next, "dev").unwrap(),
            Merge::AlreadyOn { lane: lane.clone() }
        );
        assert_eq!(git(&tree, &["rev-parse", "refs/heads/swarm"]).unwrap(), lane);
        assert_eq!(git(&tree, &["rev-parse", "refs/heads/dev"]).unwrap(), dev_before);

        // Configured onto the integration branch, it refuses rather than merges.
        let error = merge(&tree, &root, "XNAUT-931", &next, "swarm").unwrap_err();
        assert!(error.contains("cannot be the integration branch"), "{error}");
        assert!(merge(&tree, &root, "XNAUT-931", "0000000", "dev").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_lane_has_no_plan_gate_and_asks_nobody() {
        // A lane worker's plan must not open the two-reviewer gate, because
        // every failure inside it parks on the owner — the serialisation the
        // lane exists to remove, arriving through the one door left open.
        let dir = std::env::temp_dir().join(format!("xnaut-swarm-plan-{}", uuid::Uuid::new_v4()));
        let registry = dir.join("registry");
        let tree = dir.join("worktree");
        std::fs::create_dir_all(&registry).unwrap();
        std::fs::create_dir_all(&tree).unwrap();
        assert_eq!(run_ticket_for(&registry, &tree, None), None, "no run, no ticket");

        let mut run = crate::run_control::RunManifest::requested(
            "claude",
            "claude",
            tree.to_str().unwrap(),
            Some("XNAUT-930".into()),
            None,
            &[],
            crate::run_control::now_ms(),
        );
        run.pty_session = Some("session-a".into());
        crate::run_control::request_in(&registry, run, || Ok(())).unwrap();
        assert_eq!(run_ticket_for(&registry, &tree, None).as_deref(), Some("XNAUT-930"));
        assert_eq!(run_ticket_for(&registry, &tree, Some("session-a")).as_deref(), Some("XNAUT-930"));
        // Somebody else's session in the same directory is not this run.
        assert_eq!(run_ticket_for(&registry, &tree, Some("session-b")), None);
        assert_eq!(run_ticket_for(&registry, &dir, None), None, "another directory is another run");

        // The lane arm fires on the tag and only on the tag.
        assert!(is_swarm(&ticket("XNAUT-930", "in_progress", &["swarm"])));
        assert!(!is_swarm(&ticket("XNAUT-930", "in_progress", &["ios"])));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn with_a_remote_the_lane_lives_there_and_dev_is_still_untouched() {
        // The integration branch LIVES on Forgejo and every checkout follows
        // it; the lane is the same. Opening it is a plain create — there is no
        // tip to lease against — and the second merge is the compare-and-swap.
        let (root, _control, _registry, _store, _t, _job) =
            crate::jury_signoff::tests::fixture("swarm-remote");
        let tree = root.join("source");
        let bare = root.join("remote.git");
        git(&root, &["init", "--bare", "-b", "dev", bare.to_str().unwrap()]).unwrap();
        git(&tree, &["remote", "add", "origin", bare.to_str().unwrap()]).unwrap();
        git(&tree, &["push", "-q", "origin", "dev"]).unwrap();
        let dev_before = git(&bare, &["rev-parse", "refs/heads/dev"]).unwrap();

        let sha = git(&tree, &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(merge(&tree, &root, "XNAUT-930", &sha, "dev").unwrap(), Merge::Opened { lane: sha.clone() });
        assert_eq!(git(&bare, &["rev-parse", "refs/heads/dev"]).unwrap(), dev_before, "dev moved on the remote");
        assert_eq!(git(&bare, &["rev-parse", "refs/heads/swarm"]).unwrap(), sha);

        std::fs::write(tree.join("feature.txt"), "the next worker\n").unwrap();
        git(&tree, &["add", "."]).unwrap();
        git(&tree, &["commit", "-m", "XNAUT-931"]).unwrap();
        let next = git(&tree, &["rev-parse", "HEAD"]).unwrap();
        let merged = merge(&tree, &root, "XNAUT-931", &next, "dev").unwrap();
        assert!(matches!(merged, Merge::Merged { ref before, .. } if *before == sha));
        assert_eq!(git(&bare, &["rev-parse", "refs/heads/swarm"]).unwrap(), merged.lane_sha());
        assert_eq!(git(&bare, &["rev-parse", "refs/heads/dev"]).unwrap(), dev_before, "dev moved on the remote");
        // The lane tip is read from the remote, so the same record seen again
        // is still not a second merge.
        assert!(matches!(merge(&tree, &root, "XNAUT-931", &next, "dev").unwrap(), Merge::AlreadyOn { .. }));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn lane_tickets_reads_the_tag_off_the_board() {
        let (root, control, _registry, _store, mut t, _job) =
            crate::jury_signoff::tests::fixture("swarm-board");
        assert!(lane_tickets(&control).is_empty());
        t.tags = vec!["ios".into(), LANE_TAG.into()];
        crate::project_management::write_json_atomic(
            &control.join("projects/XNAUT/tickets/XNAUT-930.json"),
            &t,
        )
        .unwrap();
        assert_eq!(
            lane_tickets(&control),
            std::collections::HashSet::from(["XNAUT-930".to_string()])
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
