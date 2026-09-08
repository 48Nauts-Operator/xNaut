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
pub fn schedule(app: &AppHandle, record: crate::sandbox_verify::VerifyRecord) {
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
pub fn evidence_reason(
    t: &crate::project_management::TicketRecord,
    record: &crate::sandbox_verify::VerifyRecord,
    bundle: &str,
) -> Option<String> {
    if t.status != "complete"
        || record.status != "passed"
        || record.not_evidence
        || record.steps.is_empty()
        || record.steps.iter().any(|s| s.exit_code != Some(0))
    {
        return Some("NautBot completion and green evidence are required".into());
    }
    let Some(h) = &t.handback else {
        return Some("typed handback missing".into());
    };
    if h.files_changed.is_empty() || h.commits.is_empty() || h.how_verified.trim().is_empty() {
        return Some("incomplete handback".into());
    }
    let Some(left) = &h.not_finished else {
        return Some("not_finished is missing".into());
    };
    if !left.trim().is_empty()
        && left.trim() != "nothing"
        && !t.body.contains(&format!("Accepted not_finished: {left}"))
    {
        return Some("unfinished work has not been accepted by the ticket".into());
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
        return Some("verification record lacks full Rust and UI totals".into());
    }
    let declared = bundle
        .lines()
        .find_map(|line| line.strip_prefix("XNAUT_TEST_TOTALS="))
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
    if declared.as_ref() != Some(&totals) {
        return Some("bundle totals do not match recorded verification totals; include XNAUT_TEST_TOTALS=<JSON>".into());
    }
    None
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
    let t = ticket(repo, &record.ticket_id)?;
    let tree = Path::new(&record.repo_path);
    let (policy, policy_error) = match crate::jury_runtime::policy(repo, &t.project) {
        Ok(p) => (p, None),
        Err(e) => (Policy::default(), Some(e)),
    };
    if let Some(j) = t
        .approval
        .jury_reviews
        .iter()
        .find(|j| {
            j.gate == Gate::Signoff
                && j.source_sha == record.commit_sha
                // A job parked on the owner is not reusable: its inputs may
                // have moved on (policy, ticket revision), in which case its
                // owner decision is refused as stale, and reusing it here
                // meant no fresh review could ever start. XNAUT-305 sat in
                // that deadlock for an hour on 2026-09-07 after a green verify
                // with the totals it had been escalated for lacking.
                && j.state != "owner_required"
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
    let mut reason = policy_error
        .or_else(|| base_result.err())
        .or_else(|| evidence_reason(&t, record, &bundle));
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
    if git(tree, &["rev-parse", "HEAD"])? != record.commit_sha
        || !git(tree, &["status", "--porcelain"])?.is_empty()
    {
        reason = Some("verified tree changed or has uncommitted changes".into());
    }
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
    let job =
        crate::jury_runtime::new_job(Gate::Signoff, &t, tree, input, policy, author_run, None)?;
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
fn integration_base(tree: &Path, reference: &str) -> Result<String, String> {
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
pub fn merge_and_verify(
    app: Option<&AppHandle>,
    repo: &Path,
    registry: &Path,
    root: &Path,
    job: &mut Job,
) -> Result<(), String> {
    let _active = crate::jury_runtime::Active::new(&job.id);
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
    let sha = git(&clone, &["rev-parse", "HEAD"])?;
    if sha == base {
        return Err("source is already integrated; no merge to sign".into());
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
    publish(&tree, &clone, &reference, &sha, &base)?;
    job.state = "merged".into();
    write_job(root, job)?;
    crate::project_management::attach_jury_in(repo, job, None)?;
    verify_integration(app, repo, registry, root, job)
}

pub fn verify_integration(
    app: Option<&AppHandle>,
    repo: &Path,
    registry: &Path,
    root: &Path,
    job: &mut Job,
) -> Result<(), String> {
    let _active = crate::jury_runtime::Active::new(&job.id);
    let clone = checkout(root, job);
    let mut run = RunManifest::requested(
        "nautbot",
        "local-verify",
        clone.to_str().ok_or("invalid checkout")?,
        Some(job.ticket.clone()),
        None,
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
    for (index, command) in job.policy.integration_commands.iter().enumerate() {
        let path = root.join(format!("{}-{}-verify-{index}.log", job.id, run.run_id));
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
        let exit = outcome.unwrap_or(-1);
        green &= exit == 0;
        results.push(serde_json::json!({"command":command,"exit_code":exit,"log":path}));
        if !green {
            break;
        }
    }
    run_control::update_in(registry, &run.run_id, |r| {
        r.state = if green {
            RunState::Done
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
    if ticket(repo, &job.ticket)?
        .approval
        .jury_reviews
        .iter()
        .any(|j| j.id == job.id && ["revoke_requested", "revoked"].contains(&j.state.as_str()))
    {
        green = false;
    }
    if !green {
        job.reason = "integration build failed or revoked; see integration proof".into();
        rollback(repo, root, job)?;
    } else {
        job.state = "integrated".into();
        crate::inbox::jury_archive_asks(app, &job.id, &job.ticket);
        write_job(root, job)?;
        slim_checkout(root, job);
        // A green integration ends any block this job's earlier revoke put
        // on the ticket (XNAUT-306 sat blocked with a green re-run beside it).
        let status = (ticket(repo, &job.ticket)?.status == "blocked").then_some("complete");
        crate::project_management::attach_jury_in(repo, job, status)?;
    }
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
    cmd.env("GIT_CEILING_DIRECTORIES", state.join("tmp"))
        .env("RUST_TEST_THREADS", "1")
        .env("ZELLIJ_SOCKET_DIR", "../.xnaut/test-state/sockets");
    Ok(())
}
pub fn rollback(repo: &Path, root: &Path, job: &mut Job) -> Result<(), String> {
    job.state = "rollback_requested".into();
    job.signoff.as_mut().ok_or("no signoff to revert")?.revoked = true;
    write_job(root, job)?;
    crate::project_management::attach_jury_in(repo, job, Some("blocked"))?;
    let tree = PathBuf::from(&job.worktree);
    let clone = checkout(root, job);
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
    crate::inbox::jury_archive_asks(None, &job.id, &job.ticket);
    slim_checkout(root, job);
    write_job(root, job)?;
    crate::project_management::attach_jury_in(repo, job, Some("in_progress"))?;
    Ok(())
}

pub fn request_revoke(repo: &Path, root: &Path, id: &str) -> Result<(), String> {
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
    use super::*;
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
        let root = std::env::temp_dir().join(format!("xnaut-jury-{name}-{}", uuid::Uuid::new_v4()));
        // Never the real inbox: three "XNAUT-930 Plan" asks from this fixture
        // reached the owner's Mesh on tron (2026-09-08) and could not be
        // answered, because their ticket repo was a temp dir long gone.
        std::env::set_var("XNAUT_INBOX_DIR", root.join("inbox"));
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
        git(&source, &["branch", "feat/xnaut-264-orphan-reap"]).unwrap();
        std::fs::write(source.join("feature.txt"), "reviewed implementation\n").unwrap();
        git(&source, &["add", "."]).unwrap();
        git(&source, &["commit", "-m", "implement XNAUT-930"]).unwrap();
        let t:crate::project_management::TicketRecord=serde_json::from_value(serde_json::json!({"id":"XNAUT-930","project":"XNAUT","title":"Implement the isolated proof feature","type":"feature","status":"complete","priority":"high","owner":"codex","body":"Change feature.txt in the assigned worktree, with tests.","revision":1,"created_at":"2026-09-07T00:00:00Z","updated_at":"2026-09-07T00:00:00Z"})).unwrap();
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

        merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
        assert_ne!(job.state, "owner_required", "{}", job.reason);
        let merged = job.signoff.as_ref().unwrap().merge_sha.clone();
        let on_origin = git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]).unwrap();
        assert_eq!(on_origin, merged, "the merge must land on the remote");
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
        git(&tree, &["checkout", "feat/xnaut-264-orphan-reap"]).unwrap();
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
        assert!(evidence_reason(&t, &record, &bundle).is_none());
        assert!(evidence_reason(&t, &record, &bundle.replace("7", "8")).is_some());
        record.not_evidence = true;
        assert!(evidence_reason(&t, &record, &bundle).is_some());
    }
}
