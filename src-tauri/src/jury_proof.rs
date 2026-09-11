// Explicit live tron proof using the same production jury, PM, registry,
// integration and compensation functions. All stores belong to this worktree.
use crate::{
    jury::*,
    jury_runtime::*,
    jury_signoff::*,
    run_control::{self, RunKind, RunManifest, RunState},
};
use std::{path::Path, process::Command};

fn output(tree: &Path, command: &str) -> crate::sandbox_verify::VerifyStep {
    let mut cmd = Command::new("/bin/sh");
    cmd.current_dir(tree).args(["-c", command]);
    isolated_test_env(&mut cmd, &tree.join(".xnaut/test-state")).unwrap();
    let out = cmd.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{command}: {text}");
    crate::sandbox_verify::VerifyStep {
        name: command.into(),
        command: command.into(),
        exit_code: out.status.code(),
        log_tail: text,
        started_at: String::new(),
        duration_ms: 0,
    }
}
fn save_ticket(control: &Path, t: &crate::project_management::TicketRecord) {
    crate::project_management::write_json_atomic(
        &control.join(format!("projects/XNAUT/tickets/{}.json", t.id)),
        t,
    )
    .unwrap();
    git(control, &["add", "."]).unwrap();
    git(
        control,
        &[
            "commit",
            "--allow-empty",
            "-m",
            "update isolated dispatched proof ticket",
        ],
    )
    .unwrap();
}
fn plan_input(t: &crate::project_management::TicketRecord, tree: &Path) -> String {
    format!("Ticket scope:\n{}\nAssigned worktree: {}\nPlan: replace the exact UTF-8 bytes `baseline\\n` in feature.txt with `reviewed implementation\\n` (\\n denotes a newline). Both regression tests assert the trimmed string equals `reviewed implementation`; the negative control restores `baseline\\n` and requires both suites to fail before restoring the new value. Add lib.rs test message_is_correct and view.spec.mjs test reviewed message for this exact text. Run cargo build, the full cargo test suite, and the full Playwright suite against this fixture, prove assertions fail for baseline, and run git diff --check. Record the actual totals and a typed handback. Changes stay in this isolated worktree. No release, external publication, main branch change or irreversible action. Spend estimate 0.20 within ceiling 5.00.",ticket_snapshot(t),tree.display())
}

#[test]
#[ignore = "live configured two-runtime jury, real Rust/UI fixture suites, merge and failing-test compensation on tron"]
fn jury_live_ticket_lifecycle() {
    let (_spend_guard, _) = crate::spend::scratch("jury-live-lifecycle");
    crate::spend::spend_ceiling_set(crate::spend::SpendCeiling {
        max_concurrent: 3,
        max_daily_launches: 20,
        ..Default::default()
    })
    .unwrap();
    let (root, control, registry, store, mut t, _) =
        crate::jury_signoff::tests::fixture("lifecycle");
    let tree = root.join("source");
    // This is an isolated dispatched fixture, not the owner's live board.
    t.status = "in_progress".into();
    t.body="Implement the feature.txt text change with Rust and Playwright regression tests; add the verification bundle. Work only in the assigned worktree. The proof integration branch is dev in this isolated source repository.".into();
    save_ticket(&control, &t);
    let current = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let playwright = current.join("node_modules/@playwright/test/cli.js");
    let module = current.join("node_modules/@playwright/test/index.mjs");
    assert!(playwright.is_file());
    git(
        &tree,
        &[
            "checkout",
            "-B",
            "agent/codex/xnaut-930",
            "dev",
        ],
    )
    .unwrap();
    std::fs::write(tree.join("Cargo.toml"),"[package]\nname=\"jury-proof\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[lib]\npath=\"lib.rs\"\n").unwrap();
    std::fs::write(tree.join("lib.rs"),"pub fn message()-> &'static str {include_str!(\"feature.txt\").trim()}\n#[test] fn nonempty(){assert!(!message().is_empty());}\n").unwrap();
    std::fs::write(
        tree.join(".gitignore"),
        "target/\n.xnaut/test-state/\ntest-results/\n",
    )
    .unwrap();
    std::fs::write(
        tree.join("playwright.config.mjs"),
        "export default {testDir:'.',testMatch:'view.spec.mjs',workers:1};\n",
    )
    .unwrap();
    std::fs::write(tree.join("view.spec.mjs"),format!("import {{test,expect}} from {}; import {{readFileSync}} from 'node:fs'; test('message',async({{page}})=>{{await page.setContent('<p>'+readFileSync('feature.txt','utf8')+'</p>');await expect(page.locator('p')).not.toBeEmpty();}});\n",serde_json::to_string(module.to_str().unwrap()).unwrap())).unwrap();
    git(&tree, &["add", "."]).unwrap();
    git(&tree, &["commit", "-m", "fixture baseline and full suites"]).unwrap();
    let baseline = git(&tree, &["rev-parse", "HEAD"]).unwrap();
    let old = git(&tree, &["rev-parse", "dev"]).unwrap();
    git(
        &tree,
        &[
            "update-ref",
            "refs/heads/dev",
            &baseline,
            &old,
        ],
    )
    .unwrap();
    let mut dispatch = Command::new("/bin/sleep").arg("600").spawn().unwrap();
    let mut run = RunManifest::requested(
        "codex",
        "fixture-dispatch",
        tree.to_str().unwrap(),
        Some(t.id.clone()),
        None,
        &[],
        run_control::now_ms(),
    );
    run.pid = Some(dispatch.id());
    run.process_birth = run_control::process_birth(dispatch.id());
    let run = run_control::request_in(&registry, run, || Ok(())).unwrap();
    run_control::update_in(&registry, &run.run_id, |r| {
        r.state = RunState::Running;
        r.last_signal = "isolated ticket dispatched; live plan submitted".into();
    })
    .unwrap();
    let mut policy = crate::jury::tests::live_policy();
    policy.deadline_seconds = 180;
    let ui = format!("node '{}' test", playwright.display());
    policy.integration_commands = vec!["cargo build".into(), "cargo test".into(), ui.clone()];
    std::fs::write(
        control.join("projects/XNAUT/approval.toml"),
        toml::to_string(&policy).unwrap(),
    )
    .unwrap();
    git(&control, &["add", "."]).unwrap();
    git(
        &control,
        &["commit", "-m", "owner policy for isolated proof"],
    )
    .unwrap();
    let job = new_job(
        Gate::Plan,
        &t,
        &tree,
        plan_input(&t, &tree),
        policy.clone(),
        Some(run.run_id.clone()),
        None,
    )
    .unwrap();
    let approved = run_job(None, &control, &registry, &store, job, None).unwrap();
    let _ = dispatch.kill();
    let _ = dispatch.wait();
    run_control::update_in(&registry, &run.run_id, |r| r.state = RunState::Done).unwrap();
    println!(
        "PLAN_PROOF={}",
        serde_json::json!({"root":root,"job":approved})
    );
    assert_eq!(
        approved.decision,
        Some(Decision::Approved),
        "{}",
        approved.reason
    );
    std::fs::write(tree.join("feature.txt"), "reviewed implementation\n").unwrap();
    std::fs::write(tree.join("lib.rs"),"pub fn message()-> &'static str {include_str!(\"feature.txt\").trim()}\n#[test] fn message_is_correct(){assert_eq!(message(),\"reviewed implementation\");}\n").unwrap();
    std::fs::write(tree.join("view.spec.mjs"),format!("import {{test,expect}} from {}; import {{readFileSync}} from 'node:fs'; test('reviewed message',async({{page}})=>{{await page.setContent('<p>'+readFileSync('feature.txt','utf8')+'</p>');await expect(page.locator('p')).toHaveText('reviewed implementation');}});\n",serde_json::to_string(module.to_str().unwrap()).unwrap())).unwrap();
    // Prove both regression assertions reject the old behavior before recording
    // green verification evidence for the implemented feature.
    std::fs::write(tree.join("feature.txt"), "baseline\n").unwrap();
    for command in ["cargo test", ui.as_str()] {
        let mut cmd = Command::new("/bin/sh");
        cmd.current_dir(&tree).args(["-c", command]);
        isolated_test_env(&mut cmd, &tree.join(".xnaut/test-state")).unwrap();
        let result = cmd.output().unwrap();
        assert!(
            !result.status.success(),
            "regression test accepted baseline: {command}"
        );
        println!(
            "BASELINE_REJECTED={command}: exit {:?}",
            result.status.code()
        );
    }
    std::fs::write(tree.join("feature.txt"), "reviewed implementation\n").unwrap();
    output(&tree, "git diff --check");
    let rust = output(&tree, "cargo test");
    let browser = output(&tree, &ui);
    let totals = test_totals(&format!("{}\n{}", rust.log_tail, browser.log_tail));
    std::fs::create_dir_all(tree.join(".xnaut/bundles")).unwrap();
    std::fs::write(tree.join(".xnaut/bundles/XNAUT-930.md"),format!("# XNAUT-930\nfeature.txt now reads reviewed implementation. Rust and UI regression tests check the exact text. cargo test: 1 passed; Playwright: 1 passed. Negative controls: cargo test exited 101 and Playwright exited 1 for baseline, then both passed after restoration. Source branch agent/codex/xnaut-930; authorized integration target feat/xnaut-264-orphan-reap in this isolated repository. No unfinished work.\nXNAUT_TEST_TOTALS={totals}\n")).unwrap();
    git(&tree, &["add", "."]).unwrap();
    git(
        &tree,
        &["commit", "-m", "implement XNAUT-930 with tested bundle"],
    )
    .unwrap();
    let sha = git(&tree, &["rev-parse", "HEAD"]).unwrap();
    let steps = vec![
        output(&tree, "cargo build"),
        output(&tree, "cargo test"),
        output(&tree, &ui),
    ];
    let now = chrono::Utc::now().to_rfc3339();
    let record:crate::sandbox_verify::VerifyRecord=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4().to_string(),"run_id":run.run_id,"ticket_id":t.id,"project":"XNAUT","repo_path":tree,"commit_sha":sha,"provider_kind":"local-tron","sandbox_id":"isolated-jury-proof","public_url":"","status":"passed","steps":steps,"log_dir":root,"created_at":now,"updated_at":now})).unwrap();
    let handback = crate::handback::Handback {
        run_id: None,
        ticket: t.id.clone(),
        summary: "Feature text and exact Rust/UI regression coverage implemented".into(),
        files_changed: git(&tree, &["diff", "--name-only", &baseline, "HEAD"])
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect(),
        commits: vec![sha],
        how_verified: format!("cargo build; cargo test: 1 passed; {ui}: 1 passed"),
        verify_record_id: Some(record.id.clone()),
        not_finished: Some("nothing".into()),
        confidence: crate::handback::Confidence::High,
        from: "codex".into(),
        submitted_at: now,
    };
    assert!(matches!(
        crate::project_management::file_handback_in(&control, &handback).unwrap(),
        crate::project_management::Filing::Filed { .. }
    ));
    let settled = crate::sandbox_verify::settle_ticket_in(&control, &record)
        .unwrap()
        .unwrap();
    assert_eq!(settled.status, "complete");
    crate::project_management::write_json_atomic(&root.join("verify.json"), &record).unwrap();
    let mut signed = start(None, &control, &registry, &store, &record).unwrap();
    println!(
        "SIGNOFF_PROOF={}",
        serde_json::json!({"root":root,"job":signed})
    );
    assert_eq!(signed.state, "integrated", "{}", signed.reason);
    // Deliberately inject a real failing Rust test AFTER successful sign-off.
    let clone = store.join(format!("integration-{}", signed.id));
    use std::io::Write;
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(clone.join("lib.rs"))
            .unwrap(),
        "#[test] fn injected_regression(){{assert!(false,\"deliberate post-signoff failure\");}}"
    )
    .unwrap();
    verify_integration(None, &control, &registry, &store, &mut signed).unwrap();
    assert_eq!(signed.state, "reverted");
    assert!(
        git(&tree, &["diff", &baseline, "feat/xnaut-264-orphan-reap"])
            .unwrap()
            .is_empty()
    );
    println!(
        "REVERT_PROOF={}",
        serde_json::json!({"root":root,"job":signed,"ticket":ticket(&control,&t.id).unwrap()})
    );
}

#[test]
#[ignore = "kill a real Codex reviewer while the independent second reviewer completes"]
fn jury_live_killed_reviewer_escalates() {
    let (root, control, registry, store, mut t, mut job) =
        crate::jury_signoff::tests::fixture("killed");
    t.status = "in_progress".into();
    t.body = "Implement the feature.txt text change from baseline to reviewed implementation with Rust and Playwright regression tests and a verification bundle, only in the assigned worktree.".into();
    save_ticket(&control, &t);
    job.ticket_scope_hash = scope_hash(&t);
    job.policy.reviewers = crate::jury::tests::live_policy().reviewers;
    job.gate = Gate::Plan;
    job.decision = None;
    job.reviews.clear();
    job.input = plan_input(&t, Path::new(&job.worktree));
    job.input_hash = hash(&job.input);
    job.deadline = run_control::now_ms() + 180_000;
    let watched = registry.clone();
    let watcher = std::thread::spawn(move || {
        let deadline = run_control::now_ms() + 30_000;
        loop {
            for id in run_control::list_ids_in(&watched).unwrap_or_default() {
                let r = run_control::load_manifest_in(&watched, &id).unwrap();
                if r.kind == RunKind::Review
                    && r.runtime_id == "codex"
                    && r.state == RunState::Running
                {
                    if let Some(pid) = r.pid {
                        Command::new("/bin/kill")
                            .args(["-KILL", "--", &format!("-{pid}")])
                            .status()
                            .unwrap();
                        return serde_json::json!({"run_id":id,"pid":pid,"killed_at":run_control::now_ms()});
                    }
                }
            }
            assert!(
                run_control::now_ms() < deadline,
                "no live Codex reviewer appeared"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });
    let job = run_job(None, &control, &registry, &store, job, None).unwrap();
    let killed = watcher.join().unwrap();
    println!(
        "KILLED_REVIEWER_PROOF={}",
        serde_json::json!({"root":root,"killed":killed,"job":job})
    );
    assert_eq!(job.decision, Some(Decision::Owner));
    assert!(job.inbox_id.is_some());
    assert!(
        job.reviews
            .iter()
            .any(|r| r.runtime == job.policy.reviewers[1]
                && r.review.as_ref().is_some_and(|r| r.decision == "approved")),
        "the surviving independent reviewer must have completed"
    );
}
