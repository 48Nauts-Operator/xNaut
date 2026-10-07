use super::write_guard::with_hold;
use super::*;
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("xnaut-pm442-{}", uuid::Uuid::new_v4()));
        initialize_local_repo_transactional(&path, "control").unwrap();
        std::fs::create_dir_all(path.join("projects/TEST/tickets")).unwrap();
        Self(path)
    }
    fn ticket(&self) -> TicketRecord {
        owner_action(|| {
            ticket_create_in(
                &self.0,
                serde_json::from_value(json!({"project":"TEST","title":"Retained receipt test"}))
                    .unwrap(),
            )
        })
        .unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn job(ticket: &TicketRecord) -> crate::jury::Job {
    serde_json::from_value(json!({
        "id": uuid::Uuid::new_v4().to_string(), "gate":"signoff", "ticket":ticket.id,
        "project":"TEST", "worktree":"", "author":"codex", "author_run":null,
        "ticket_revision":ticket.revision, "input":"independent check receipt", "input_hash":"hash", "source_sha":"head",
        "policy":crate::jury::Policy::default(), "round":1,"deadline":0,"reviews":[],"decision":null,
        "reason":"waiting for owner", "inbox_id":null,"notify_id":null,"state":"owner_required","signoff":null
    })).unwrap()
}
#[test]
fn paused_automatic_writes_keep_git_quiet_but_owner_ticket_actions_work() {
    let f = Scratch::new();
    let ticket = f.ticket();
    let before = run_git(&f.0, &["rev-parse", "HEAD"]).unwrap();
    let bytes = std::fs::read(find_ticket_path(&f.0, &ticket.id).unwrap()).unwrap();
    with_hold(Some("read_only test hold"), || {
        assert!(ticket_tag_in(&f.0, &ticket.id, "automatic", false)
            .unwrap_err()
            .contains("read_only"));
        assert!(run_maintenance_task(&f.0, "loose-objects")
            .unwrap_err()
            .contains("read_only"));
        assert!(
            record_mutation(&f.0, "background", "TEST", json!({}), &[], "background")
                .unwrap_err()
                .contains("read_only")
        );
        assert_eq!(run_git(&f.0, &["rev-parse", "HEAD"]).unwrap(), before);
        assert_eq!(
            std::fs::read(find_ticket_path(&f.0, &ticket.id).unwrap()).unwrap(),
            bytes
        );
        assert!(run_git(&f.0, &["status", "--porcelain"])
            .unwrap()
            .is_empty());
        owner_action(|| ticket_tag_in(&f.0, &ticket.id, "owner", false)).unwrap();
    });
    assert_ne!(run_git(&f.0, &["rev-parse", "HEAD"]).unwrap(), before);
}
#[test]
fn deferred_documents_replay_once_after_restart_and_keep_original_payload() {
    let f = Scratch::new();
    let before = run_git(&f.0, &["rev-parse", "HEAD"]).unwrap();
    let details = json!({"sha":"artifact-one","reason":"actual evidence"});
    with_hold(Some("read_only test hold"), || {
        assert!(
            record_document_in(&f.0, "vault.saved", "work:TEST/page.md", details.clone()).is_err()
        );
        assert!(
            record_document_in(&f.0, "vault.saved", "work:TEST/page.md", details.clone()).is_err()
        );
        assert_eq!(run_git(&f.0, &["rev-parse", "HEAD"]).unwrap(), before);
        assert!(deferred::warning(&f.0).unwrap().contains("1 deferred"));
    });
    // Replay reconstructs all state from persisted evidence, not a live queue.
    with_hold(None, || deferred::replay(&f.0)).unwrap();
    let after = run_git(&f.0, &["rev-parse", "HEAD"]).unwrap();
    assert_ne!(before, after);
    assert!(deferred::warning(&f.0).unwrap().is_empty());
    // Simulate commit-before-outbox-ack crash by restoring the same envelope.
    deferred::retain(
        &f.0,
        deferred::Pending::Document {
            event: "vault.saved".into(),
            subject: "work:TEST/page.md".into(),
            details: details.clone(),
        },
        "restart",
    )
    .unwrap();
    with_hold(None, || deferred::replay(&f.0)).unwrap();
    assert_eq!(after, run_git(&f.0, &["rev-parse", "HEAD"]).unwrap());
    let events = std::fs::read_dir(f.0.join("events"))
        .unwrap()
        .flatten()
        .filter(|p| p.path().extension().is_some_and(|v| v == "json"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    let event: Value = read_json(&events[0].path()).unwrap();
    assert_eq!(event["details"], details);
}
#[test]
fn paused_jury_receipt_retains_evidence_and_refuses_stale_owner_revision() {
    let f = Scratch::new();
    let ticket = f.ticket();
    let receipt = job(&ticket);
    let before = run_git(&f.0, &["rev-parse", "HEAD"]).unwrap();
    with_hold(Some("read_only test hold"), || {
        assert!(attach_jury_in(&f.0, &receipt, Some("blocked"))
            .unwrap_err()
            .contains("retained locally"));
        assert!(attach_jury_in(&f.0, &receipt, Some("blocked"))
            .unwrap_err()
            .contains("retained locally"));
    });
    assert_eq!(before, run_git(&f.0, &["rev-parse", "HEAD"]).unwrap());
    assert!(deferred::warning(&f.0).unwrap().contains("1 deferred"));
    owner_action(|| ticket_tag_in(&f.0, &ticket.id, "owner-decision", false)).unwrap();
    let owner_head = run_git(&f.0, &["rev-parse", "HEAD"]).unwrap();
    with_hold(None, || deferred::replay(&f.0)).unwrap();
    assert_eq!(owner_head, run_git(&f.0, &["rev-parse", "HEAD"]).unwrap());
    assert!(deferred::warning(&f.0)
        .unwrap()
        .contains("ticket changed while paused"));
    let retained = std::fs::read_dir(common_dir(&f.0).unwrap().join("xnaut-pm-state/deferred"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let value: Value = read_json(&retained).unwrap();
    assert_eq!(value["pending"]["job"]["id"], receipt.id);
}
#[test]
fn unchanged_jury_receipt_replays_and_corrupt_evidence_is_kept_visible() {
    let f = Scratch::new();
    let ticket = f.ticket();
    let receipt = job(&ticket);
    with_hold(Some("paused"), || {
        assert!(attach_jury_in(&f.0, &receipt, Some("blocked")).is_err())
    });
    let bad = common_dir(&f.0)
        .unwrap()
        .join("xnaut-pm-state/deferred")
        .join(format!("{}.json", "f".repeat(64)));
    std::fs::write(&bad, b"{broken").unwrap();
    with_hold(None, || deferred::replay(&f.0)).unwrap();
    let saved: TicketRecord = read_json(&find_ticket_path(&f.0, &ticket.id).unwrap()).unwrap();
    assert_eq!(saved.status, "blocked");
    assert_eq!(saved.approval.jury_reviews[0].id, receipt.id);
    assert!(bad.exists());
    assert!(deferred::warning(&f.0).unwrap().contains("unreadable"));
    let head = run_git(&f.0, &["rev-parse", "HEAD"]).unwrap();
    with_hold(None, || deferred::replay(&f.0)).unwrap();
    assert_eq!(head, run_git(&f.0, &["rev-parse", "HEAD"]).unwrap());
}
#[test]
fn refused_push_fault_survives_reload_and_reports_loose_objects_until_real_success() {
    let f = Scratch::new();
    let remote = f.0.join(".git/missing.git");
    run_git(
        &f.0,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    )
    .unwrap();
    f.ticket();
    let warning = deferred::warning(&f.0).unwrap();
    assert!(warning.contains("Push deferred"));
    assert!(control_repo_pressure_detail(&f.0)
        .unwrap()
        .contains("loose objects"));
    run_git(&f.0, &["init", "--bare", &remote.to_string_lossy()]).unwrap();
    owner_action(|| sync_repo(&f.0, None)).unwrap();
    assert!(deferred::warning(&f.0).unwrap().is_empty());
}
#[test]
fn shared_writer_lease_excludes_maintenance_and_document_writes_on_another_thread() {
    let f = Scratch::new();
    let _held = owner_action(|| ControlWriteLease::acquire(&f.0)).unwrap();
    let path = f.0.clone();
    std::thread::spawn(move || {
        assert!(run_maintenance_task(&path, "loose-objects")
            .unwrap_err()
            .contains("another process"));
        assert!(record_document_in(&path, "vault.saved", "TEST", json!({"proof":1})).is_err());
        assert!(deferred::warning(&path).unwrap().contains("1 deferred"));
    })
    .join()
    .unwrap();
    // A nested call on the admitted thread does not deadlock or reacquire.
    record_mutation(&f.0, "owner", "TEST", json!({}), &[], "owner").unwrap();
}
#[test]
fn linked_worktrees_share_the_same_maintenance_lease() {
    let f = Scratch::new();
    let linked = f.0.join("linked");
    run_git(
        &f.0,
        &[
            "worktree",
            "add",
            "--detach",
            &linked.to_string_lossy(),
            "HEAD",
        ],
    )
    .unwrap();
    let _held = owner_action(|| ControlWriteLease::acquire(&f.0)).unwrap();
    std::thread::spawn(move || {
        assert!(owner_action(|| ControlWriteLease::acquire(&linked))
            .err()
            .unwrap()
            .contains("another process"))
    })
    .join()
    .unwrap();
}

#[test]
fn corrupt_retained_records_do_not_starve_a_later_valid_document() {
    let f = Scratch::new();
    let dir = common_dir(&f.0).unwrap().join("xnaut-pm-state/deferred");
    std::fs::create_dir_all(&dir).unwrap();
    for n in 0..40 {
        std::fs::write(dir.join(format!("{n:064x}.json")), b"{broken").unwrap();
    }
    with_hold(Some("paused"), || {
        assert!(record_document_in(
            &f.0,
            "vault.saved",
            "TEST/late",
            json!({"actual":"late evidence"})
        )
        .is_err())
    });
    with_hold(None, || deferred::replay(&f.0)).unwrap();
    assert_eq!(
        std::fs::read_dir(dir).unwrap().count(),
        40,
        "corrupt files stay, valid event replays"
    );
    assert!(run_git(&f.0, &["log", "-1", "--format=%s"])
        .unwrap()
        .contains("TEST/late"));
}
#[test]
fn pushing_another_branch_cannot_clear_retained_push_failure() {
    let f = Scratch::new();
    deferred::push_result(&f.0, "blocked-branch", &Err("refused".into())).unwrap();
    deferred::push_result(&f.0, "other-branch", &Err("also refused".into())).unwrap();
    deferred::push_result(&f.0, "other-branch", &Ok("published".into())).unwrap();
    let warning = deferred::warning(&f.0).unwrap();
    assert!(warning.contains("blocked-branch"));
    assert!(!warning.contains("other-branch"));
    deferred::push_result(&f.0, "blocked-branch", &Ok("published".into())).unwrap();
    assert!(deferred::warning(&f.0).unwrap().is_empty());
}
#[test]
fn automatic_project_creation_cannot_inherit_the_native_owner_exception() {
    let f = Scratch::new();
    let request = || {
        serde_json::from_value(json!({"key":"MODEL","name":"Model project","source_repo":"https://example.test/org/project.git"})).unwrap()
    };
    with_hold(Some("read_only test hold"), || {
        assert!(project_create_in(&f.0, request())
            .unwrap_err()
            .contains("read_only"));
        assert!(!f.0.join("projects/MODEL").exists());
        owner_action(|| project_create_in(&f.0, request())).unwrap();
    });
    assert!(f.0.join("projects/MODEL/project.json").exists());
}

#[test]
fn deferred_events_do_not_cross_linked_branch_boundaries() {
    let f = Scratch::new();
    let ticket = f.ticket();
    let receipt = job(&ticket);
    let linked = f.0.join("linked");
    run_git(
        &f.0,
        &[
            "worktree",
            "add",
            "-b",
            "other",
            &linked.to_string_lossy(),
            "HEAD",
        ],
    )
    .unwrap();
    with_hold(Some("paused"), || {
        assert!(record_document_in(
            &f.0,
            "vault.saved",
            "TEST/original",
            json!({"proof":"source branch"})
        )
        .is_err());
        assert!(attach_jury_in(&f.0, &receipt, Some("blocked")).is_err());
    });
    let original = run_git(&f.0, &["rev-parse", "HEAD"]).unwrap();
    with_hold(None, || deferred::replay(&linked)).unwrap();
    assert_eq!(original, run_git(&linked, &["rev-parse", "HEAD"]).unwrap());
    assert!(deferred::warning(&linked).unwrap().contains("2 deferred"));
    with_hold(None, || {
        record_document_in(
            &linked,
            "vault.saved",
            "TEST/other",
            json!({"proof":"other branch"}),
        )
    })
    .unwrap();
    assert!(run_git(&linked, &["log", "-1", "--format=%s"])
        .unwrap()
        .contains("TEST/other"));
    with_hold(None, || deferred::replay(&f.0)).unwrap();
    assert!(deferred::warning(&f.0).unwrap().is_empty());
    let saved: TicketRecord = read_json(&find_ticket_path(&f.0, &ticket.id).unwrap()).unwrap();
    assert_eq!(saved.status, "blocked");
    let other: TicketRecord = read_json(&find_ticket_path(&linked, &ticket.id).unwrap()).unwrap();
    assert_eq!(other.revision, ticket.revision);
}
#[test]
fn wrong_committed_acknowledgment_never_discards_retained_evidence() {
    let f = Scratch::new();
    with_hold(Some("paused"), || {
        assert!(record_document_in(
            &f.0,
            "vault.saved",
            "TEST/actual",
            json!({"proof":"actual"})
        )
        .is_err())
    });
    let pending = std::fs::read_dir(common_dir(&f.0).unwrap().join("xnaut-pm-state/deferred"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let id = pending.file_stem().unwrap().to_str().unwrap();
    let path = format!("events/deferred-{id}.json");
    write_json_atomic(&f.0.join(&path),&json!({"version":1,"deferred_pending_sha256":id,"event":"vault.saved","subject":"TEST/actual","details":{"proof":"WRONG"}})).unwrap();
    run_git(&f.0, &["add", "--", &path]).unwrap();
    run_git(
        &f.0,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@local",
            "commit",
            "--only",
            "-m",
            "wrong marker",
            "--",
            &path,
        ],
    )
    .unwrap();
    let before = run_git(&f.0, &["rev-parse", "HEAD"]).unwrap();
    with_hold(None, || deferred::replay(&f.0)).unwrap();
    assert!(pending.exists());
    assert!(deferred::warning(&f.0).unwrap().contains("does not match"));
    assert_eq!(before, run_git(&f.0, &["rev-parse", "HEAD"]).unwrap());
    let retained: Value = read_json(&pending).unwrap();
    assert_eq!(retained["pending"]["details"]["proof"], "actual");
}

#[test]
fn concurrent_real_switch_files_pause_only_their_own_fixture() {
    let paused = Scratch::new();
    let running = Scratch::new();
    let paused_head = run_git(&paused.0, &["rev-parse", "HEAD"]).unwrap();
    let running_head = run_git(&running.0, &["rev-parse", "HEAD"]).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let check = |root: PathBuf, read_only: bool, barrier: std::sync::Arc<std::sync::Barrier>| {
        std::thread::spawn(move || {
            let switches = root.join(".git/fixture-switches");
            let _scope = crate::switches::TestScope::in_dir(
                switches.clone(),
                crate::switches::KillSwitches {
                    read_only,
                    ..Default::default()
                },
            );
            barrier.wait();
            let observed_before = crate::switches::load().read_only;
            let disk =
                read_json::<crate::switches::KillSwitches>(&switches.join("kill-switches.json"));
            let result = record_mutation(
                &root,
                "fixture.automatic",
                "TEST",
                json!({"actual":"strict gate"}),
                &[],
                "fixture automatic write",
            );
            barrier.wait();
            assert_eq!(observed_before, read_only);
            assert_eq!(disk.unwrap().read_only, read_only);
            if read_only {
                assert!(result.unwrap_err().contains("read_only"));
            } else {
                result.unwrap();
            }
            assert_eq!(
                crate::switches::load().read_only,
                read_only,
                "the other fixture cannot replace this file authority"
            );
        })
    };
    let first = check(paused.0.clone(), true, barrier.clone());
    let second = check(running.0.clone(), false, barrier);
    first.join().unwrap();
    second.join().unwrap();
    assert_eq!(
        paused_head,
        run_git(&paused.0, &["rev-parse", "HEAD"]).unwrap()
    );
    assert_ne!(
        running_head,
        run_git(&running.0, &["rev-parse", "HEAD"]).unwrap()
    );
}

#[test]
fn scoped_corrupt_switch_file_still_blocks_the_actual_pm_gate() {
    let f = Scratch::new();
    let root = f.0.join(".git/fixture-switches");
    let _scope =
        crate::switches::TestScope::in_dir(root.clone(), crate::switches::KillSwitches::default());
    let before = run_git(&f.0, &["rev-parse", "HEAD"]).unwrap();
    std::fs::write(root.join("kill-switches.json"), b"{invalid").unwrap();
    assert!(record_mutation(
        &f.0,
        "fixture.automatic",
        "TEST",
        json!({}),
        &[],
        "must refuse"
    )
    .unwrap_err()
    .contains("invalid kill-switch file"));
    assert_eq!(before, run_git(&f.0, &["rev-parse", "HEAD"]).unwrap());
}
