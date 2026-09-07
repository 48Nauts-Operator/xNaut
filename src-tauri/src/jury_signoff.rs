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
        .find(|j| j.gate == Gate::Signoff && j.source_sha == record.commit_sha)
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
    let base = base_result
        .as_ref()
        .cloned()
        .unwrap_or_else(|_| record.commit_sha.clone());
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
fn checkout(root: &Path, job: &Job) -> PathBuf {
    root.join(format!("integration-{}", job.id))
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
    let base = git(&tree, &["rev-parse", &reference])?;
    // A checked-out integration branch belongs to a live/shared surface. Never
    // advance its ref behind that checkout's index and working files.
    if git(&tree, &["worktree", "list", "--porcelain"])?
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
        write_job(root, job)?;
        crate::project_management::attach_jury_in(repo, job, None)?;
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
    let current = git(&tree, &["rev-parse", &reference])?;
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
    fn green_integration_has_verify_record_and_revocation_is_reversible() {
        let (_root, control, registry, store, _, mut job) = fixture("green");
        merge_and_verify(None, &control, &registry, &store, &mut job).unwrap();
        assert_eq!(job.state, "integrated");
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
