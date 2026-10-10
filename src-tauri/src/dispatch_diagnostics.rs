//! Give the orchestrator the same native route and worker observations used by
//! admission. A diagnostic never edits settings, creates a run or stops a writer.
use serde_json::{json, Value};

fn route_report(
    settings: &crate::settings::Settings,
    profile: &crate::agent_profiles::AgentProfile,
    environment: &str,
    requirement: &str,
) -> (
    Value,
    Option<(
        crate::agent_profiles::AgentProfile,
        Option<crate::cloud_model::Resolved>,
    )>,
) {
    let mut report = json!({
        "gateway_enabled": crate::chat::gateway_enabled(settings),
        "saved_default": {"provider": settings.cloud_agent_model.provider,
            "model": settings.cloud_agent_model.model},
        "profile": {"handle":profile.handle,"runtime":profile.runtime_id,
            "model":profile.model,"shell_allowed":profile.policy.shell},
        "model_requirement":requirement,"environment":environment,
    });
    match crate::cloud_model::apply(settings, profile, environment) {
        Ok((effective, cloud)) => {
            report["effective_route"] = json!({"provider":effective.provider,"model":effective.model,
                "connection":cloud.as_ref().map(|c| &c.pin)});
            report["repository_policy"] =
                match crate::cloud_model::validate_repository_profile(&effective, requirement) {
                    Ok(()) => json!({"ready":true}),
                    Err(error) => json!({"ready":false,"reason":error}),
                };
            (report, Some((effective, cloud)))
        }
        Err(error) => {
            report["route_error"] = json!(error);
            (report, None)
        }
    }
}

pub(crate) fn diagnose(project: &str, ticket: &str, environment: &str) -> Result<Value, String> {
    if project.is_empty() || ticket.is_empty() || !matches!(environment, "exe-dev" | "gitvm") {
        return Err(
            "Supply a project, ticket and its intended exe-dev or gitvm destination".into(),
        );
    }
    let repo = crate::project_management::repo_now()?;
    let current = crate::project_management::ticket_list_in(&repo, Some(project.into()))?
        .into_iter()
        .find(|t| t.id == ticket)
        .ok_or("Ticket is not in this project")?;
    let handle = current
        .owner
        .as_deref()
        .ok_or("Ticket has no assigned agent")?
        .trim_start_matches('@');
    let profile = crate::agent_profiles::agent_profile_get(handle.into())?;
    let settings = crate::settings::load_or_default();
    let (mut report, resolved) =
        route_report(&settings, &profile, environment, &current.model_requirement);
    report["ticket"] = json!({"id":ticket,"revision":current.revision,"status":current.status});
    report["checked_at"] = json!(chrono::Utc::now().to_rfc3339());
    report["execution_started"] = json!(false);
    let registry = crate::agents::registry_dir()?;
    let mut runs = crate::run_control::list_ids_in(&registry)?
        .iter()
        .map(|id| crate::run_control::load_manifest_in(&registry, id))
        .collect::<Result<Vec<_>, _>>()?;
    runs.retain(|r| r.project == project && r.ticket.as_deref() == Some(ticket));
    runs.sort_by_key(|r| std::cmp::Reverse(r.started_at));
    let transfers = crate::repository_transfer::list()?;
    let latest_worker = runs.iter().find_map(|r| {
        transfers.iter().find(|t| {
            t.run_id == r.run_id
                && t.project == project
                && t.ticket.as_deref() == Some(ticket)
                && !t.workdir.is_empty()
        })
    });
    let mut observations = Vec::new();
    // Bound remote work, and disclose truncation instead of implying completeness.
    report["total_retained_runs"] = json!(runs.len());
    for run in runs.iter().take(4) {
        let mut observation = json!({"run_id":run.run_id,"recorded_state":run.state,
            "runtime":run.runtime_id,"model":run.model,"environment":run.remote_env,
            "last_signal":run.last_signal,"previous_run_id":run.previous_run_id,
            "next_run_id":run.next_run_id,"worktree":run.worktree_path,"branch":run.branch});
        if let Some(transfer) = transfers.iter().find(|t| {
            t.run_id == run.run_id && t.project == project && t.ticket.as_deref() == Some(ticket)
        }) {
            observation["publication"] = json!({"state":transfer.state,"pr":transfer.pr_url});
        }
        observations.push(observation);
    }
    if let Some(transfer) = latest_worker {
        report["latest_worker"] = match transfer.worker.probe(&transfer.workdir) {
            Ok(proof) => json!({"run_id":transfer.run_id,"observed":true,"phase":proof["phase"],
                "agent_pid":proof["agent_pid"],"head":proof["head"],
                "note":"Process presence is a live observation, not implementation or safe-stop proof"}),
            Err(error) => json!({"run_id":transfer.run_id,"observed":false,"reason":error}),
        };
    }
    let workers = crate::run_control::worker_count_in(&registry)?;
    report["capacity"] = match crate::spend::would_admit(workers) {
        Ok(()) => json!({"recorded_workers":workers,"can_admit":true}),
        Err(error) => json!({"recorded_workers":workers,"can_admit":false,"reason":error}),
    };
    report["runs"] = json!(observations);
    report["continuation"] = match crate::run_control::continuation_in(&registry, ticket) {
        Ok(next) => json!({"inspection_succeeded":true,"pending_run_id":next.map(|r| r.run_id)}),
        Err(error) => json!({"inspection_succeeded":false,"reason":error}),
    };
    if let Some((effective, cloud)) = resolved {
        let target = if environment == "exe-dev" {
            Some(crate::worker_bootstrap::Target::ExeDev)
        } else {
            runs.iter()
                .find_map(|r| {
                    transfers.iter().find(|t| {
                        t.run_id == r.run_id
                            && matches!(t.worker, crate::worker_bootstrap::Target::GitVm { .. })
                    })
                })
                .map(|t| t.worker.clone())
        };
        report["runtime_readiness"] = match target {
            Some(target) => match crate::agent_profiles::diagnose_remote_runtime(
                &effective,
                cloud.as_ref(),
                &target,
            ) {
                Ok(()) => {
                    json!({"ready":true,"scope":"Installed harness, authentication and model catalog; no inference or task executed"})
                }
                Err(error) => json!({"ready":false,"reason":error}),
            },
            None => {
                json!({"ready":null,"reason":"No existing GitVM worker for this ticket; diagnosis does not provision one"})
            }
        };
    }
    Ok(report)
}

/// Reserve a continuation only for an externally stopped failure whose entire
/// published delta is its own run artifacts. Source-bearing work uses review.
/// This never signals a process and never starts the successor.
pub(crate) fn recover_stopped(
    project: &str,
    ticket: &str,
    id: &str,
    revision: u64,
) -> Result<Value, String> {
    use crate::run_control::{self, RunState};
    if crate::switches::load().read_only
        || crate::instance::role() == crate::instance::Role::Sandbox
    {
        return Err("This instance cannot recover worker assignments".into());
    }
    let repo = crate::project_management::repo_now()?;
    let current = crate::project_management::ticket_list_in(&repo, Some(project.into()))?
        .into_iter()
        .find(|t| t.id == ticket)
        .ok_or("Ticket is not in this project")?;
    if current.revision != revision || !crate::swarm_plan::is_open(&current.status) {
        return Err("Ticket changed or is not open; inspect before recovery".into());
    }
    let registry = crate::agents::registry_dir()?;
    let run = run_control::load_manifest_in(&registry, id)?;
    if run.project != project
        || run.ticket.as_deref() != Some(ticket)
        || run.state != RunState::Failed
        || run.admission_refused
    {
        return Err("Not this ticket's admitted failed worker; existing work retained".into());
    }
    let transfer = crate::repository_transfer::list()?
        .into_iter()
        .find(|t| t.run_id == id)
        .ok_or("The native transfer receipt is missing")?;
    let environment = match transfer.worker {
        crate::worker_bootstrap::Target::ExeDev => "exe-dev",
        _ => "gitvm",
    };
    if transfer.project != project
        || transfer.ticket.as_deref() != Some(ticket)
        || transfer.local_path != run.worktree_path
        || transfer.local_branch != run.branch
        || transfer.handle != run.agent_handle
        || run.remote_env.as_deref() != Some(environment)
        || transfer.review_parent.is_some()
        || transfer.repair_parent.is_some()
        || transfer.workdir != transfer.worker.run_directory(id)
    {
        return Err("Run and transfer identities differ; no recovery performed".into());
    }
    let owner = current
        .owner
        .as_deref()
        .ok_or("Ticket has no assigned agent")?
        .trim_start_matches('@');
    let settings = crate::settings::load_or_default();
    let profile = crate::agent_profiles::agent_profile_get(owner.into())?;
    let (effective, cloud) = crate::cloud_model::apply(&settings, &profile, environment)?;
    crate::cloud_model::validate_repository_profile(&effective, &current.model_requirement)?;
    let tree = std::path::Path::new(&run.worktree_path);
    let git = |args: &[&str]| crate::repository_transfer::git(tree, args);
    if git(&["rev-parse", "HEAD"])? != transfer.source_sha
        || git(&["symbolic-ref", "--short", "HEAD"])? != run.branch
        || !git(&["status", "--porcelain"])?.is_empty()
    {
        return Err("Original worktree changed; preserve it and inspect before recovery".into());
    }
    let binding = json!({"operation":"prove_stopped_failure","run_id":id,"project":project,
        "ticket":ticket,"handle":transfer.handle,"workdir":transfer.workdir,"artifacts":transfer.artifacts,
        "branch":transfer.branch,"source_sha":transfer.source_sha,"remote":transfer.remote,"environment":environment});
    let observed = transfer.worker.completed_task_handoff(&binding)?;
    if observed["state"] != "stopped_without_source_changes" || observed["run_id"] != id {
        return Err("Stopped worker proof is incomplete".into());
    }
    let (published, _) = crate::repository_transfer::fetch_result(&transfer)?
        .ok_or("Final publication could not be fetched")?;
    if published.uncommitted_source
        || observed["head"].as_str() != Some(published.published_head.as_str())
    {
        return Err("Published revision differs from stopped worker proof".into());
    }
    let proof = run_control::Proofs {
        pid_absent: true,
        session_known: true,
        capture_known: true,
        capture_quiet: true,
        worktree_exists: true,
        branch_matches: true,
        commit: transfer.source_sha.clone(),
        ..Default::default()
    };
    let next = run_control::reserve_failed_continuation_in(
        &registry,
        &run,
        &proof,
        &current.model_requirement,
        run_control::now_ms(),
        |next| {
            let fresh = crate::project_management::ticket_list_in(&repo, Some(project.into()))?
                .into_iter()
                .find(|t| t.id == ticket)
                .ok_or("Ticket disappeared")?;
            if fresh.revision != revision {
                return Err("Ticket changed during recovery".into());
            }
            next.agent_handle = effective.handle.clone();
            next.runtime_id = effective.runtime_id.clone();
            next.model = Some(effective.model.clone());
            next.cloud_model = cloud.as_ref().map(|c| c.pin.clone());
            crate::swarm_plan::worker_admission_in(&registry, next, Some(id))
        },
    )?;
    Ok(
        json!({"state":"continuation_reserved","run_id":next.run_id,"previous_run_id":id,
        "ticket":ticket,"environment":environment,"worktree":next.worktree_path,"branch":next.branch,
        "preserved_publication":published.published_head,"worker_proof":observed,"execution_started":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(unix)]
    fn worker_stop_proof_protocol_suite() {
        let output = std::process::Command::new("python3")
            .args(["-m", "unittest", "-v", "test_repository_handoff"])
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
            .output()
            .expect("worker proof protocol tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn stopped_failure_reserves_one_continuation_and_pins_required_model() {
        use crate::run_control::{self as rc, Proofs, RunManifest, RunState};
        let temp =
            std::env::temp_dir().join(format!("xnaut-stopped-recovery-{}", uuid::Uuid::new_v4()));
        let registry = temp.join("registry");
        let mut run = RunManifest::requested(
            "old",
            "codex",
            "preserved-worktree",
            Some("TEST-1".into()),
            Some("old-model".into()),
            &[],
            1,
        );
        run.branch = "preserved-branch".into();
        run.project = "TEST".into();
        run.remote_env = Some("exe-dev".into());
        let id = run.run_id.clone();
        rc::request_in(&registry, run, || Ok(())).unwrap();
        rc::update_in(&registry, &id, |r| r.state = RunState::Failed).unwrap();
        let failed = rc::load_manifest_in(&registry, &id).unwrap();
        assert!(rc::reserve_failed_continuation_in(
            &registry,
            &failed,
            &Proofs::default(),
            "claude-required",
            2,
            |_| Ok(())
        )
        .is_err());
        assert_eq!(rc::load_manifest_in(&registry, &id).unwrap(), failed);
        let proof = Proofs {
            pid_absent: true,
            session_known: true,
            capture_known: true,
            capture_quiet: true,
            worktree_exists: true,
            branch_matches: true,
            commit: "a".repeat(40),
            ..Default::default()
        };
        assert!(rc::reserve_failed_continuation_in(
            &registry,
            &failed,
            &proof,
            "claude-required",
            2,
            |_| Err("capacity".into())
        )
        .is_err());
        assert_eq!(rc::list_ids_in(&registry).unwrap().len(), 1);
        let child = rc::reserve_failed_continuation_in(
            &registry,
            &failed,
            &proof,
            "claude-required",
            3,
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(child.previous_run_id.as_deref(), Some(id.as_str()));
        assert_eq!(child.worktree_path, failed.worktree_path);
        assert_eq!(child.branch, failed.branch);
        assert_eq!(child.state, RunState::Requested);
        let parent = rc::load_manifest_in(&registry, &id).unwrap();
        assert_eq!(parent.state, RunState::Retired);
        assert_eq!(parent.retirement.unwrap().requirement, "claude-required");
        let replay = rc::reserve_failed_continuation_in(
            &registry,
            &failed,
            &proof,
            "claude-required",
            4,
            |_| panic!("must reuse reservation"),
        )
        .unwrap();
        assert_eq!(child.run_id, replay.run_id);
        assert_eq!(rc::list_ids_in(&registry).unwrap().len(), 2);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn diagnosis_explains_gateway_default_and_policy_without_disclosing_credentials() {
        let mut settings = crate::settings::Settings::default();
        settings.cloud_agent_model.provider = "lmstudio".into();
        settings.cloud_agent_model.model = "qwen-default".into();
        settings.llm_providers = vec![crate::settings::LlmProviderSettings {
            name: "nautgate".into(),
            endpoint: "https://gate.example/v1".into(),
            api_key: Some("PRIVATE-GATEWAY-KEY".into()),
            enabled: true,
        }];
        let profile = serde_json::from_value(json!({"handle":"claude","display_name":"Claude",
            "tagline":"","purpose":"build","runtime_id":"claude","provider":"nautgate",
            "model":"claude-explicit","role":"builder","default_project":null,
            "policy":{"shell":false}}))
        .unwrap();
        let (report, _) = route_report(&settings, &profile, "exe-dev", "Claude");
        assert_eq!(report["saved_default"]["model"], "qwen-default");
        assert_eq!(report["effective_route"]["model"], "claude-explicit");
        assert_eq!(report["repository_policy"]["ready"], false);
        assert!(!report.to_string().contains("PRIVATE-GATEWAY-KEY"));
        settings.llm_providers[0].endpoint = "http://localhost:8090/v1".into();
        let (report, resolved) = route_report(&settings, &profile, "exe-dev", "");
        assert!(resolved.is_none());
        assert!(report["route_error"]
            .as_str()
            .unwrap()
            .contains("localhost"));
    }
}

#[cfg(test)]
mod live_checks {
    use super::*;
    #[tokio::test]
    #[ignore = "explicit live NautBot inference and read-only worker diagnosis"]
    async fn live_nautbot_reasons_from_dispatch_evidence() {
        let project = std::env::var("XNAUT_LIVE_DIAG_PROJECT").expect("Explicit project required");
        let ticket = std::env::var("XNAUT_LIVE_DIAG_TICKET").expect("Explicit ticket required");
        let settings = crate::settings::load_or_default();
        let profile = crate::agent_profiles::agent_profile_get("nautbot".into()).unwrap();
        let llm =
            crate::chat::provider_llm(&settings, profile.chat_provider_or_provider()).unwrap();
        let outcome=crate::agent_tools::run_turn(&llm,profile.chat_model_or_model(),vec![
            json!({"role":"system","content":"This is a read-only diagnosis test. Use only diagnose_dispatch. Do not create, update, recover, dispatch, stop or approve anything. Base your answer on its returned evidence. Distinguish configured route, model readiness, saved run state and observed process state. Never claim readiness proves execution."}),
            json!({"role":"user","content":format!("Diagnose why {ticket} in project {project} cannot currently continue on exe-dev. Claude is required through NautGate, with Qwen retained as the saved default. Inspect the actual native dispatch diagnosis and explain the specific remaining blocker and next action.")})
        ],None,&[],"nautbot").await.unwrap();
        println!(
            "XNAUT_NAUTBOT_REASONING {}",
            json!({"model":profile.chat_model_or_model(),"performed":outcome.performed,"text":outcome.text})
        );
        assert!(outcome
            .performed
            .iter()
            .any(|call| call.starts_with("diagnose_dispatch")));
        assert!(outcome
            .performed
            .iter()
            .all(|call| call.starts_with("diagnose_dispatch")));
    }
    #[tokio::test]
    #[ignore = "explicit operator recovery; reserves one continuation on a named real ticket"]
    async fn live_nautbot_recover_stopped_dispatch() {
        assert_eq!(
            std::env::var("XNAUT_LIVE_RECOVERY_ACTION").as_deref(),
            Ok("reserve-only")
        );
        let project = std::env::var("XNAUT_LIVE_DIAG_PROJECT").expect("Explicit project required");
        let ticket = std::env::var("XNAUT_LIVE_DIAG_TICKET").expect("Explicit ticket required");
        let id = std::env::var("XNAUT_LIVE_RECOVERY_RUN").expect("Explicit failed run required");
        let revision: u64 = std::env::var("XNAUT_LIVE_RECOVERY_REVISION")
            .expect("Expected revision required")
            .parse()
            .unwrap();
        let result = crate::agent_tools::execute(
            "recover_stopped_dispatch",
            &json!({"project":project,"ticket":ticket,"run_id":id,"expected_revision":revision}),
            "nautbot",
        )
        .await;
        println!("XNAUT_RECOVERY_RESULT {result}");
        assert_eq!(result["ok"], true);
        assert_eq!(result["recovery"]["execution_started"], false);
    }
    #[tokio::test]
    #[ignore = "requires explicit live project/ticket; probes an existing worker without launching"]
    async fn live_nautbot_dispatch_diagnosis() {
        let project = std::env::var("XNAUT_LIVE_DIAG_PROJECT").expect("Explicit project required");
        let ticket = std::env::var("XNAUT_LIVE_DIAG_TICKET").expect("Explicit ticket required");
        let result = crate::agent_tools::execute(
            "diagnose_dispatch",
            &json!({"project":project,"ticket":ticket,"environment":"exe-dev"}),
            "nautbot",
        )
        .await;
        println!("XNAUT_DIAG_RESULT {result}");
        assert_eq!(result["ok"], true);
        assert_eq!(result["diagnosis"]["execution_started"], false);
    }
}
