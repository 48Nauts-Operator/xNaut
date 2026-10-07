//! Self-service workspaces and launches for the identity in an agent chat.
//! This does not grant cross-agent dispatch or access to arbitrary repositories.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub fn is_tool(name: &str) -> bool {
    matches!(
        name,
        "prepare_repository_ticket" | "create_worktree" | "start_repository_task"
    )
}

pub fn specs() -> Vec<Value> {
    let mut specs: Vec<Value> = [
        ("create_worktree", "Create your own isolated Git worktree for an authorized task. All agent identities can use this. Resolve the repository from registered-project context or a user-supplied path. Creates no worker and runs no audit."),
        ("start_repository_task", "Create your own isolated worktree and launch your coding runtime with the task. Use for audits, tests, commands or code changes, without asking the owner to create a worktree. Returns an actual launch receipt, not proof of completed work. Pass environment when the owner requests local, exe.dev or GitVM; otherwise uses your configured environment. A remote error is a blocker, never permission to retry locally. Never attaches an idle terminal as a substitute. Reuse the same task_key on retries to avoid duplicate workers."),
    ].into_iter().map(|(name, description)| json!({"type":"function","function":{
        "name":name,"description":description,"parameters":{"type":"object","properties":{
            "root":{"type":"string","description":"Absolute authorized repository root, or the exact registered project key/name named by the user."},
            "task_key":{"type":"string","description":"Stable task identity, preferably its ticket ID. Same key for create and start, and for retries. A new key means a separate run."},
            "task":{"type":"string","description":"Full work request and acceptance criteria, preserving the user's scope."},
            "ticket":{"type":"string","description":"Required existing ticket ID in this repository’s registered project. Create or attach the correct ticket first; never retry without one."},
            "environment":{"type":"string","enum":["local","exe-dev","gitvm"],"description":"Execution destination for this task. Pass exe-dev when the owner asks for exe.dev, even if your profile says local. Omit only when no task-specific destination was requested. Remote failure must not silently downgrade to local."}
        },"required":["root","task_key","task","ticket"],"additionalProperties":false}
    }})).collect();
    specs.push(json!({"type":"function","function":{
        "name":"prepare_repository_ticket",
        "description":"Find or create the ticket BEFORE repository work. Resolves the project by the authorized repository root, never a default board. If unregistered, registers that existing checkout using its Git remote. Reuses the same task_key on retries. Check saved tasks/list_tickets first to avoid duplicating existing work. This creates tracking only; no worktree or worker starts.",
        "parameters":{"type":"object","properties":{
            "root":{"type":"string"},"task_key":{"type":"string"},
            "title":{"type":"string"},"task":{"type":"string","description":"Full owner requirements and acceptance criteria recovered from this conversation."}
        },"required":["root","task_key","title","task"],"additionalProperties":false}
    }}));
    specs
}

fn authorize_root(root: &str, allowed: &[PathBuf]) -> Result<PathBuf, String> {
    let path = crate::repository_read::authorized_root(root, allowed)?;
    if !path.join(".git").exists() {
        return Err("A Git repository root is required; scratch folders are not worktrees.".into());
    }
    Ok(path)
}

fn workspace_key(handle: &str, task_key: &str) -> String {
    // Bounded, deterministic, and distinct even when two agents use the same ticket.
    let digest = Sha256::digest(format!("{handle}\0{task_key}").as_bytes());
    format!("chat-{}", &format!("{digest:x}")[..24])
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = crate::worktree::git_command()
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub(crate) fn verify_workspace(repo: &Path, dest: &Path, branch: &str) -> Result<(), String> {
    if dest
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("Refusing a symlink in place of the agent worktree.".into());
    }
    let root = PathBuf::from(git(dest, &["rev-parse", "--show-toplevel"])?)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let canonical = dest.canonicalize().map_err(|e| e.to_string())?;
    if root != canonical || git(dest, &["branch", "--show-current"])? != branch {
        return Err(
            "Existing directory is not this agent's expected Git worktree and branch.".into(),
        );
    }
    let common = |at: &Path| -> Result<PathBuf, String> {
        PathBuf::from(git(
            at,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?)
        .canonicalize()
        .map_err(|e| e.to_string())
    };
    if common(repo)? != common(dest)? {
        return Err("Existing worktree belongs to another repository.".into());
    }
    Ok(())
}

fn required_ticket(ticket: Option<&str>) -> Result<&str, String> {
    ticket.map(str::trim).filter(|id| !id.is_empty())
        .ok_or_else(|| "A project-matched ticket is required before creating a worktree or launching work. Find or create the ticket first; never retry without it.".into())
}

fn ticket_context(ticket: Option<&str>, root: &Path) -> Result<String, String> {
    let id = required_ticket(ticket)?;
    let registry = crate::project_management::repo_now()?;
    ticket_context_in(&registry, id, root)
}

fn ticket_context_in(registry: &Path, id: &str, root: &Path) -> Result<String, String> {
    ticket_context_admitted_in(registry, id, root, crate::ticket_triage::dispatch_admission)
}

fn ticket_context_admitted_in(
    registry: &Path, id: &str, root: &Path,
    admit: impl FnOnce(&crate::project_management::TicketRecord) -> Result<(), String>,
) -> Result<String, String> {
    let record = crate::project_management::ticket_list_in(registry, None)?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| format!("Ticket {id} not found"))?;
    if matches!(record.status.as_str(), "review" | "done" | "complete") {
        return Err("This ticket is already in review, done or complete. Reconcile its existing work before reopening or creating follow-up work.".into());
    }
    let project = crate::project_management::list_projects(registry)?
        .into_iter()
        .find(|p| p.key == record.project)
        .ok_or("Ticket project not found")?;
    let source = PathBuf::from(crate::project_management::local_source_path(&project))
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if source != root {
        return Err("Ticket and repository do not belong to the same project.".into());
    }
    // This boundary is called by model-generated worktree/launch tools. The
    // authenticated handle, task text, and owner conversation are not recorded
    // approval of an imported finding. Explicit owner dispatch/group approval
    // use their separate native paths; models cannot opt into that exception.
    admit(&record)?;
    Ok(format!("\nREGISTERED TICKET {} — {}\n{}\nPreserve this scope (including read-only restrictions). Do not expand it implicitly. Report execution evidence, findings, tests actually run and coverage gaps. A launch is not a completed audit.\n", record.id,record.title,record.body))
}

/// Native admission is ticket-scoped, independent of agent, task key or thread.
/// A retired/failed runtime can still own implementation. Only run_control's
/// verified successor path may continue it, in exactly the same Git worktree.
pub(crate) fn recovery_guard(
    snapshot: &Value,
    ticket: &str,
    continuation: Option<&crate::run_control::RunManifest>,
) -> Result<(), String> {
    if snapshot["diagnostics"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["blocking"] != false))
    {
        return Err(format!("Project recovery is incomplete; no worker was started. Repair these evidence sources before retrying: {}", snapshot["diagnostics"]));
    }
    let row = snapshot["tickets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["id"].as_str() == Some(ticket))
        .ok_or("Ticket missing from recovered project evidence")?;
    if matches!(row["status"].as_str(), Some("review" | "done" | "complete")) {
        return Err(format!("{ticket} is {}; reconcile its existing work before reopening. Ticket status alone is not independent verification. Evidence: {}", row["status"], row["evidence"]));
    }
    let assignments: Vec<_> = snapshot["assignments"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|run| run["ticket"].as_str() == Some(ticket))
        .collect();
    if let Some(next) = continuation {
        let mut lineage = std::collections::HashSet::from([next.run_id.clone()]);
        let mut previous = next.previous_run_id.clone();
        while let Some(id) = previous {
            if !lineage.insert(id.clone()) {
                return Err("Cyclic recovered continuation".into());
            }
            previous = assignments
                .iter()
                .find(|run| run["run_id"].as_str() == Some(&id))
                .and_then(|run| run["previous_run_id"].as_str())
                .map(str::to_owned);
        }
        if !assignments.is_empty()
            && assignments.iter().all(|run| {
                run["run_id"]
                    .as_str()
                    .is_some_and(|id| lineage.contains(id))
                    && run["branch"] == next.branch
                    && run["worktree"] == next.worktree_path
            })
        {
            // Admission still verifies predecessor retirement/proof and model
            // policy atomically in run_control::request_in.
            return Ok(());
        }
    }
    // The ticket's own launch notes outlive local registry retention. Explicit
    // artifact evidence must be reconciled even when there is no manifest.
    let artifacts = row["evidence"].as_array().into_iter().flatten().any(|e| {
        matches!(
            e["kind"].as_str(),
            Some(
                "branch"
                    | "worktree"
                    | "pr"
                    | "launch"
                    | "receipt"
                    | "commit"
                    | "assignment"
                    | "pull_request"
                    | "handback"
                    | "launch_receipt"
                    | "transfer"
            )
        )
    });
    if !assignments.is_empty() || artifacts || row["status"] == "blocked" {
        return Err(format!("{ticket} has existing or unresolved work; no replacement was started. Recover the ticket handoff and inspect its branch/PR. Continue through the verified existing-run handoff, preserving its worktree. Assignments: {}; ticket evidence: {}", json!(assignments), row["evidence"]));
    }
    Ok(())
}

/// One persistent admission slot across chat and PM dispatch. A successor gets
/// a distinct slot only after native recovery verifies its existing lineage.
pub(crate) fn launch_receipt_path(
    registry: &Path,
    root: &Path,
    ticket: &str,
    continuation: Option<&str>,
) -> Result<PathBuf, String> {
    let dir = registry.join("chat-launches");
    let digest = Sha256::digest(
        format!(
            "{}\0{ticket}\0{}",
            root.display(),
            continuation.unwrap_or("initial")
        )
        .as_bytes(),
    );
    Ok(dir.join(format!("ticket-{digest:x}.json")))
}

/// Called inside shared native admission after asynchronous repository staging.
/// Only an internal model-origin reservation requests this gate; explicit owner
/// UI launches retain their established authorization path.
pub(crate) fn admit_model_reservation_in(registry: &Path, run: &crate::run_control::RunManifest) -> Result<(),String> {
    let Some(reserved_root) = model_reservation_root_in(registry,run)? else { return Ok(()); };
    let ticket = run.ticket.as_deref().ok_or("Model launch ticket disappeared")?;
    let control = crate::project_management::repo_now()?;
    let projects = crate::project_management::list_projects(&control)?;
    let project = projects.iter().find(|p|p.key == run.project)
        .ok_or("Model launch project disappeared before admission")?;
    let root = PathBuf::from(crate::project_management::local_source_path(project));
    let root = root.canonicalize().map_err(|e|e.to_string())?;
    admit_reserved_model_root(&reserved_root,&root,||crate::ticket_triage::admit_current_ticket(&run.project,ticket))
}

pub(crate) fn pin_model_reservation_in(registry: &Path, run: &mut crate::run_control::RunManifest) -> Result<(),String> {
    run.findings_reservation_root = model_reservation_root_in(registry,run)?.map(|p|p.to_string_lossy().into_owned());
    Ok(())
}

fn model_reservation_root_in(registry: &Path, run: &crate::run_control::RunManifest) -> Result<Option<PathBuf>,String> {
    let Some(ticket) = run.ticket.as_deref() else { return Ok(None); };
    let entries = match std::fs::read_dir(registry.join("chat-launches")) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && run.findings_reservation_root.is_none() => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut selected = None;
    for entry in entries {
        let path = entry.map_err(|e|e.to_string())?.path();
        if path.extension().and_then(|s|s.to_str()) != Some("json") { continue; }
        // Shared registry records are scoped before their proof is considered.
        // Unknown/foreign historical data cannot grant model authorization.
        let Ok(bytes) = std::fs::read(&path) else { continue; };
        let Ok(receipt) = serde_json::from_slice::<Value>(&bytes) else { continue; };
        if receipt["pending"] != true || receipt["requires_findings_triage"] != true
            || receipt["ticket"] != ticket || receipt["project"] != run.project
            || receipt["worktree_path"] != run.worktree_path { continue; }
        let root = PathBuf::from(receipt["repository_root"].as_str().ok_or("Model launch reservation root is missing")?);
        let continuation = receipt["continuation_run_id"].as_str();
        if !root.is_absolute() || receipt["branch"] != run.branch
            || ![None,Some(run.run_id.as_str()),run.previous_run_id.as_deref()].contains(&continuation)
            || launch_receipt_path(registry,&root,ticket,continuation)? != path
        { return Err("Model launch reservation identity changed before native admission".into()); }
        if selected.replace(root).is_some() { return Err("Multiple pending model launch reservations need reconciliation".into()); }
    }
    if let Some(expected) = &run.findings_reservation_root {
        if selected.as_deref() != Some(Path::new(expected)) {
            return Err("Model launch reservation disappeared or changed during staging".into());
        }
    }
    Ok(selected)
}

/// GitVM registers capacity before warm-up. Recheck after staging without
/// registering that admitted ID again or claiming an agent process ran.
pub(crate) fn admit_staged_model_run_in(registry: &Path, id: &str) -> Result<(),String> {
    admit_staged_model_run_with(registry,id,|run|admit_model_reservation_in(registry,run))
}

fn admit_staged_model_run_with(registry: &Path, id: &str,
    admit: impl FnOnce(&crate::run_control::RunManifest)->Result<(),String>) -> Result<(),String> {
    let original = crate::run_control::load_manifest_in(registry,id)?;
    if let Err(error) = admit(&original) {
        crate::run_control::update_in(registry,id,|run| {
            if run.revision == original.revision && run.state == crate::run_control::RunState::Starting
                && run.pid.is_none() && run.process_birth.is_none() && run.pty_session.is_none() && run.zellij_session.is_none()
                && run.last_hook_at.is_none() && run.capture_bytes == 0 {
                run.state = crate::run_control::RunState::Failed;
                run.admission_refused = true;
                run.prelaunch_failure = Some(crate::run_control::PrelaunchFailure {
                    phase:crate::run_control::PrelaunchPhase::RepositoryStaging,recorded_at:crate::run_control::now_ms() });
                run.last_signal = format!("Model findings admission refused after staging: {error}. No agent was started.");
            }
        })?;
        return Err(error);
    }
    Ok(())
}

fn admit_reserved_model_root(root: &Path, current_root: &Path, admit: impl FnOnce()->Result<(),String>) -> Result<(),String> {
    if root != current_root { return Err("Model launch project repository changed during staging".into()); }
    admit()
}

#[cfg(test)]
fn admit_model_reservation_at(registry: &Path, run: &crate::run_control::RunManifest, current_root: &Path,
    admit: impl FnOnce()->Result<(),String>) -> Result<(),String> {
    let Some(root) = model_reservation_root_in(registry,run)? else { return Ok(()); };
    admit_reserved_model_root(&root,current_root,admit)
}

/// Remove a reservation only when we know launch admission was never reached.
/// Once attempted, even an error can hide a live or partially created worker.
pub(crate) struct LaunchReservation {
    path: PathBuf,
    attempted: bool,
}
impl LaunchReservation {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            attempted: false,
        }
    }
    pub(crate) fn attempted(&mut self) {
        self.attempted = true;
    }
}
impl Drop for LaunchReservation {
    fn drop(&mut self) {
        if !self.attempted {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Called only by the owner of a just-attempted reservation, after launch
/// returned an error. A newer persisted admission refusal proves no worker
/// entered execution; an unchanged or post-admission failure proves nothing.
pub(crate) fn release_refused_continuation(
    registry: &Path,
    receipt_path: &Path,
    reserved: &crate::run_control::RunManifest,
) -> Result<bool, String> {
    let _lock = crate::run_control::StoreLock::acquire(registry)?;
    let Some(ticket) = reserved.ticket.as_deref() else {
        return Ok(false);
    };
    let Some(mut latest) = crate::run_control::continuation_in(registry, ticket)? else {
        return Ok(false);
    };
    if latest.state != crate::run_control::RunState::Failed
        || !latest.admission_refused
        || latest.branch != reserved.branch
        || latest.worktree_path != reserved.worktree_path
    {
        return Ok(false);
    }
    if latest.run_id == reserved.run_id && latest.revision <= reserved.revision {
        return Ok(false);
    }
    let refused = latest.clone();
    let mut seen = std::collections::HashSet::new();
    while latest.run_id != reserved.run_id {
        if !seen.insert(latest.run_id.clone()) {
            return Ok(false);
        }
        let Some(previous) = latest.previous_run_id.as_deref() else {
            return Ok(false);
        };
        latest = crate::run_control::load_manifest_in(registry, previous)?;
    }
    let mut pending: Value =
        serde_json::from_slice(&std::fs::read(receipt_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if pending["pending"] != true || pending["continuation_run_id"] != reserved.run_id {
        return Ok(false);
    }
    pending["pending"] = json!(false);
    pending["ok"] = json!(false);
    pending["execution_started"] = json!(false);
    pending["admission_refused"] = json!(true);
    pending["launch"] = json!({"run_id":refused.run_id});
    pending["prelaunch_failure"] = json!(refused.prelaunch_failure);
    pending["refused_run_revision"] = json!(refused.revision);
    pending["note"] = json!("Native admission refused before execution; original reservation retained as evidence. Retry the current continuation in its preserved workspace.");
    save_launch_receipt(receipt_path, &pending)?;
    if refused.run_id == reserved.run_id {
        // A Requested continuation can fail admission under the same ID. Its
        // next attempt needs this slot again: move, never erase, the finalized
        // receipt to a sibling file that the snapshot reader also consumes.
        archive_refusal_receipt(receipt_path, &pending, refused.revision)?;
    }
    Ok(true)
}

/// Call under the native store lock so archive replay cannot race admission
/// or another reconciler. Never overwrite a differing historical receipt.
fn archive_refusal_receipt(path: &Path, receipt: &Value, revision: u64) -> Result<(), String> {
    let archived = path.with_file_name(format!("{}-refused-{revision}.json",
        path.file_stem().and_then(|s| s.to_str()).ok_or("Invalid receipt path")?));
    if archived.exists() {
        let previous: Value = serde_json::from_slice(&std::fs::read(&archived).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if previous != *receipt { return Err("Refusal evidence archive differs; inspect before retrying".into()); }
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    } else {
        std::fs::rename(path, archived).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn reconcile_refusal_archive_in(registry: &Path, root: &Path, project: &str, ticket: &str) -> Result<bool, String> {
    use crate::run_control::{self, RunKind, RunState};
    let _lock = run_control::StoreLock::acquire(registry)?;
    let Some(run) = run_control::continuation_in(registry, ticket)? else { return Ok(false); };
    if run.kind != RunKind::Agent || run.state != RunState::Failed || !run.admission_refused
        || run.previous_run_id.is_none() || run.pid.is_some() || run.process_birth.is_some()
        || run.pty_session.is_some() || run.zellij_session.is_some() || run.last_hook_at.is_some()
        || run.capture_bytes != 0 || run.project != project { return Ok(false); }
    let path = launch_receipt_path(registry, root, ticket, Some(&run.run_id))?;
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    let receipt: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if receipt["pending"] != false || receipt["ok"] != false || receipt["execution_started"] != false
        || receipt["admission_refused"] != true || receipt["continuation_run_id"] != run.run_id
        || receipt["launch"]["run_id"] != run.run_id || receipt["refused_run_revision"] != run.revision
        || receipt["ticket"] != ticket || receipt["project"] != project
        || receipt["repository_root"].as_str() != root.to_str() || receipt["handle"] != run.agent_handle
        || receipt["branch"] != run.branch || receipt["worktree_path"] != run.worktree_path
        || receipt["environment"].as_str() != run.remote_env.as_deref()
        || receipt["prelaunch_failure"] != json!(run.prelaunch_failure) { return Ok(false); }
    archive_refusal_receipt(&path, &receipt, run.revision)?;
    Ok(true)
}

/// Bind a newly refused first launch to its native run instead of erasing the
/// reservation. Only the owner of an attempted reservation calls this after
/// launch returned Err; unknown failures stay pending. The preserved receipt
/// then joins the normal continuation chain on the next turn/restart.
pub(crate) fn bind_initial_refusal(
    registry: &Path,
    receipt_path: &Path,
    before: &Value,
) -> Result<bool, String> {
    let mut pending: Value =
        serde_json::from_slice(&std::fs::read(receipt_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let Some(ticket) = pending["ticket"].as_str() else {
        return Ok(false);
    };
    let Some(requested_at) = pending["requested_at"].as_i64() else {
        return Ok(false);
    };
    if pending["pending"] != true
        || !pending["continuation_run_id"].is_null()
        || before["assignments"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["ticket"].as_str() == Some(ticket))
    {
        return Ok(false);
    }
    let Some(run) = crate::run_control::continuation_in(registry, ticket)? else {
        return Ok(false);
    };
    if !crate::run_control::initial_admission_refused(&run)
        || run.started_at < requested_at
        || pending["branch"] != run.branch
        || pending["worktree_path"] != run.worktree_path
        || pending["handle"] != run.agent_handle
    {
        return Ok(false);
    }
    pending["pending"] = json!(false);
    pending["ok"] = json!(false);
    pending["execution_started"] = json!(false);
    pending["admission_refused"] = json!(true);
    pending["launch"] = json!({"run_id":run.run_id});
    pending["note"] = json!("Native admission refused before worker execution. Retry through the existing continuation, preserving this branch and worktree.");
    save_launch_receipt(receipt_path, &pending)?;
    Ok(true)
}

/// Reconcile only native pre-execution evidence. Unknown/crashed launches remain
/// pending. This updates bookkeeping, never grants dispatch permission.
pub(crate) fn reconcile_prelaunch(project: &str, ticket: &str) -> Result<bool, String> {
    let registry = crate::agents::registry_dir()?;
    let control = crate::project_management::repo_now()?;
    let projects = crate::project_management::list_projects(&control)?;
    let Some(project_record) = projects.iter().find(|p| p.key == project) else { return Ok(false); };
    let root = Path::new(&crate::project_management::local_source_path(project_record)).canonicalize().map_err(|e| e.to_string())?;
    reconcile_prelaunch_in(&registry, &root, project, ticket)
}

fn reconcile_prelaunch_in(registry: &Path, root: &Path, project: &str, ticket: &str) -> Result<bool, String> {
    if reconcile_refusal_archive_in(registry, root, project, ticket)? { return Ok(true); }
    if reconcile_spend_prelaunch_in(registry, root, project, ticket)? { return Ok(true); }
    // Ordinary launches and completed reconciliations have no dependency on
    // the historical transfer store. Uncertainty matters only for a pending slot.
    if pending_initial_receipt(registry, root, project, ticket)?.is_none() { return Ok(false); }
    let store = registry.join("repository-transfers");
    let mut transfers = Vec::new();
    if store.exists() {
        for entry in std::fs::read_dir(store).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
            let value: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            if value["project"] != project || value["ticket"] != ticket { continue; }
            let transfer: crate::repository_transfer::Transfer = serde_json::from_value(value).map_err(|e| e.to_string())?;
            if path.file_stem().and_then(|s| s.to_str()) != Some(transfer.run_id.as_str()) { return Err("Transfer identity mismatch".into()); }
            transfers.push(transfer);
        }
    }
    reconcile_initial_prelaunch_in(registry, root, project, ticket, &transfers)
}

/// Exact native group chronology binds the old string refusal to this one
/// reservation. A pending slot, an absent PID, or a capacity-like message alone
/// is never evidence that execution did not start.
fn legacy_group_spend_proof<'a>(
    group: &'a crate::swarm_plan::Group, pending: &Value, path: &Path,
) -> Option<&'a str> {
    use crate::swarm_plan::MemberState;
    let ticket = pending["ticket"].as_str()?;
    let requested = pending["requested_at"].as_i64()?;
    if pending["pending"] != true || group.approved_at.is_none_or(|at| at > requested)
        || pending["project"].as_str() != Some(group.plan.project.as_str()) { return None; }
    let run = group.plan.runs.iter().find(|r| r.ticket == ticket)?;
    if pending["handle"] != run.owner || pending["branch"] != run.branch
        || pending["repository_root"].as_str() != run.repository_root.as_deref()
        || pending["environment"].as_str() != run.environment.as_deref() { return None; }
    let member = group.members.iter().find(|m| m.ticket == ticket)?;
    let receipt_id = format!("receipt:{}", path.display());
    let observation_reason = "implementation retained; awaiting review or repair evidence";
    let receipt_observed = member.state == MemberState::Tracking
        && member.run_id.as_deref() == Some(receipt_id.as_str()) && member.reason == observation_reason;
    if member.started.is_some() || (!receipt_observed
        && (member.state != MemberState::Blocked || member.run_id.is_some())) { return None; }
    let refusal = member.refusal.as_ref()?;
    if refusal.kind != crate::dispatch::RefusalKind::Uncertain { return None; }
    let suffix = format!(". Launch reservation retained at {}; reconcile the run before retrying.", path.display());
    let error = refusal.reason.strip_suffix(&suffix)?;
    let blocked_reason = format!("dispatch outcome requires reconciliation: {}", refusal.reason);
    if !crate::spend::is_concurrent_refusal(error)
        || (!receipt_observed && member.reason != blocked_reason) { return None; }
    let source = format!("swarm-plans/{}.json", group.plan.id);
    let mut events = group.events.iter().rev().filter(|e| e.ticket == ticket);
    let mut blocked = events.next()?;
    if receipt_observed {
        // Recovery may have observed the unresolved receipt as retained work.
        // This exact native bookkeeping event is not a worker launch or a
        // replacement for the earlier synchronous refusal proof.
        if blocked.state != MemberState::Tracking || blocked.reason != observation_reason
            || blocked.run_id.as_deref() != Some(receipt_id.as_str()) || blocked.actor != "nautbot"
            || blocked.source != source || blocked.project != group.plan.project { return None; }
        let observed_at = blocked.at_ms;
        blocked = events.next()?;
        if observed_at < blocked.at_ms { return None; }
    }
    let starting = events.next()?;
    if blocked.state != MemberState::Blocked || blocked.reason != blocked_reason
        || blocked.at_ms < requested || starting.at_ms > requested
        || starting.state != MemberState::Starting || starting.reason != "dispatch reserved by approved group"
        || [blocked, starting].iter().any(|e| e.project != group.plan.project || e.actor != "nautbot"
            || e.source != format!("swarm-plans/{}.json", group.plan.id) || e.run_id.is_some()) { return None; }
    Some(error)
}

fn reconcile_spend_prelaunch_in(registry: &Path, root: &Path, project: &str, ticket: &str) -> Result<bool, String> {
    use crate::run_control::{self, RunManifest};
    let Some(current) = run_control::continuation_in(registry, ticket)? else { return Ok(false); };
    if !run_control::prelaunch_refused(&current) { return Ok(false); }
    let mut slots = Vec::new();
    for id in std::iter::once(current.run_id.as_str()).chain(current.previous_run_id.as_deref()) {
        let path = launch_receipt_path(registry, root, ticket, Some(id))?;
        let body = match std::fs::read(&path) {
            Ok(body) => body,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        let pending: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        if pending["pending"] == true { slots.push((path, pending)); }
    }
    if slots.len() != 1 { return Ok(false); }
    let (path, pending) = &slots[0];
    let Some(prior_id) = pending["continuation_run_id"].as_str() else { return Ok(false); };
    let prior = run_control::load_manifest_in(registry, prior_id)?;
    let Some(requested) = pending["requested_at"].as_i64() else { return Ok(false); };
    if pending["project"] != project || pending["ticket"] != ticket
        || pending["repository_root"].as_str() != root.to_str()
        || pending["handle"] != prior.agent_handle || pending["branch"] != prior.branch
        || pending["worktree_path"] != prior.worktree_path || pending["environment"] != "exe-dev"
        || prior.project != project || prior.remote_env.as_deref() != Some("exe-dev")
        || prior.started_at > requested { return Ok(false); }
    if current.run_id != prior.run_id {
        // Crash after writing native proof but before binding its receipt.
        if run_control::spend_prelaunch_refused(&current) && current.started_at >= requested {
            return release_refused_continuation(registry, path, &prior);
        }
        return Ok(false);
    }
    let groups = crate::swarm_plan::groups_in(registry, Some(project))?;
    let Some(group) = groups.iter().filter(|g| g.approved_at.is_some()
        && g.members.iter().any(|m| m.ticket == ticket)).max_by_key(|g| g.approved_at) else { return Ok(false); };
    let Some(error) = legacy_group_spend_proof(group, pending, path) else { return Ok(false); };
    let runs = run_control::list_ids_in(registry)?.iter().map(|id| run_control::load_manifest_in(registry, id)).collect::<Result<Vec<_>,_>>()?;
    if runs.iter().any(|r| r.run_id != prior.run_id
        && (r.ticket.as_deref() == Some(ticket) || r.worktree_path == prior.worktree_path)) { return Ok(false); }
    let store = registry.join("repository-transfers");
    if store.exists() {
        for entry in std::fs::read_dir(store).map_err(|e| e.to_string())? {
            let candidate = entry.map_err(|e| e.to_string())?.path();
            if candidate.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
            let value: Value = serde_json::from_slice(&std::fs::read(candidate).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            if value["ticket"] != ticket && value["local_path"] != prior.worktree_path { continue; }
            if value["run_id"] != prior.run_id || value["state"] != "preparation_failed"
                || value["project"] != project || value["handle"] != prior.agent_handle
                || value["local_path"] != prior.worktree_path || value["local_branch"] != prior.branch
                || !value["pr_url"].is_null() || value["filed"] == true { return Ok(false); }
        }
    }
    let worktree = Path::new(&prior.worktree_path);
    verify_workspace(root, worktree, &prior.branch)?;
    if crate::repository_transfer::git(worktree, &["rev-parse", "HEAD"])? != prior.last_commit
        || !crate::repository_transfer::git(worktree, &["status", "--porcelain"])?.is_empty() { return Ok(false); }
    let mut next = RunManifest::requested(&prior.agent_handle, &prior.runtime_id, &prior.worktree_path,
        prior.ticket.clone(), prior.model.clone(), &[], requested);
    next.project = project.into(); next.remote_env = prior.remote_env.clone();
    run_control::refuse_spend_after_in(registry, &prior, next, error)?;
    release_refused_continuation(registry, path, &prior)
}

fn pending_initial_receipt(registry: &Path, root: &Path, project: &str, ticket: &str) -> Result<Option<Value>, String> {
    let path = launch_receipt_path(registry, root, ticket, None)?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let pending: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if pending["pending"] != true || pending["ticket"] != ticket || pending["project"] != project
        || pending["repository_root"].as_str() != root.to_str() || !pending["continuation_run_id"].is_null() { return Ok(None); }
    Ok(Some(pending))
}

fn native_request_time(id: &str) -> Option<i64> {
    const ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    if id.len() != 26 || !id.bytes().all(|b| ALPHABET.as_bytes().contains(&b)) { return None; }
    let mut at = 0u64;
    for b in id.bytes().take(10) { at = at * 32 + ALPHABET.bytes().position(|v| v == b)? as u64; }
    (at < (1u64 << 48)).then_some(at as i64)
}

fn reconcile_initial_prelaunch_in(registry: &Path, root: &Path, project: &str, ticket: &str, transfers: &[crate::repository_transfer::Transfer]) -> Result<bool, String> {
    use crate::run_control::{self, PrelaunchPhase, RunManifest};
    let path = launch_receipt_path(registry, root, ticket, None)?;
    let Some(mut pending) = pending_initial_receipt(registry, root, project, ticket)? else { return Ok(false); };
    let Some(requested_at) = pending["requested_at"].as_i64() else { return Ok(false); };
    let Some(worktree) = pending["worktree_path"].as_str() else { return Ok(false); };
    let Some(branch) = pending["branch"].as_str().filter(|s| !s.is_empty()) else { return Ok(false); };
    let Some(handle) = pending["handle"].as_str().filter(|s| !s.is_empty()) else { return Ok(false); };
    let runs = run_control::list_ids_in(registry)?.iter().map(|id| run_control::load_manifest_in(registry, id)).collect::<Result<Vec<_>,_>>()?;
    let existing: Vec<_> = runs.iter().filter(|r| r.ticket.as_deref() == Some(ticket)).collect();
    if runs.iter().any(|r| r.worktree_path == worktree && r.ticket.as_deref() != Some(ticket)) { return Ok(false); }
    let relevant: Vec<_> = transfers.iter().filter(|t| t.ticket.as_deref() == Some(ticket)).collect();
    let run = if existing.len() == 1 && run_control::prelaunch_refused(existing[0]) {
        let run = existing[0];
        if relevant.iter().any(|t| t.run_id != run.run_id || t.state != "preparation_failed") { return Ok(false); }
        run.clone()
    } else if existing.is_empty() && relevant.len() == 1 {
        let t = relevant[0];
        // preparation_failed was written only on the caught staging Err branch,
        // before the PTY/worker call. prepared/running/launch_failed do not qualify.
        let Some(started_at) = native_request_time(&t.run_id).filter(|at| *at >= requested_at) else { return Ok(false); };
        if t.state != "preparation_failed" || t.project != project || t.handle != handle
            || t.local_path != worktree || t.local_branch != branch || t.pr_url.is_some()
            || t.review_parent.is_some() || t.repair_parent.is_some() || t.filed
            || t.branch != format!("xnaut/runs/{}", t.run_id)
            || t.workdir != format!("agents/runs/{}", t.run_id)
            || t.artifacts != format!(".xnaut/runs/{}", t.run_id)
            || !matches!(t.worker, crate::worker_bootstrap::Target::ExeDev)
            || pending["environment"] != "exe-dev" { return Ok(false); }
        verify_workspace(root, Path::new(worktree), branch)?;
        if crate::repository_transfer::git(Path::new(worktree), &["rev-parse", "HEAD"])? != t.source_sha { return Ok(false); }
        // Historical profile/model are unknown in legacy transfer receipts. Do
        // not invent them: fresh admission revalidates the approved current pins.
        let mut run = RunManifest::requested(handle, "unknown", worktree, Some(ticket.into()), None, &[], started_at);
        run.run_id = t.run_id.clone();
        run.remote_env = Some("exe-dev".into());
        run_control::refuse_prelaunch_in(registry, run, PrelaunchPhase::LegacyRepositoryStaging,
            "Native repository staging failed before the worker launch boundary")?
    } else { return Ok(false); };
    if run.project != project || run.worktree_path != worktree || run.branch != branch || run.agent_handle != handle
        || run.previous_run_id.is_some() || run.started_at < requested_at { return Ok(false); }
    // Preserve the original reservation and link its exact native refusal.
    pending["pending"] = json!(false);
    pending["ok"] = json!(false);
    pending["execution_started"] = json!(false);
    pending["admission_refused"] = json!(true);
    pending["launch"] = json!({"run_id": run.run_id});
    pending["prelaunch_failure"] = json!(run.prelaunch_failure);
    pending["note"] = json!("Native prelaunch failure reconciled; continue the preserved local branch/worktree through fresh admission.");
    save_launch_receipt(&path, &pending)?;
    Ok(true)
}

fn workspace_target(
    root: &Path,
    handle: &str,
    task_key: &str,
    continuation: Option<&crate::run_control::RunManifest>,
) -> (PathBuf, String) {
    match continuation {
        Some(run) => (PathBuf::from(&run.worktree_path), run.branch.clone()),
        None => {
            let slug = workspace_key(handle, task_key);
            (
                root.join(".worktrees").join(&slug),
                format!("agent/{handle}/{slug}"),
            )
        }
    }
}

pub(crate) fn save_launch_receipt(path: &Path, receipt: &Value) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(receipt).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}

fn read_launch_receipt(path: &Path) -> Result<Option<Value>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Unreadable launch reservation at {}: {e}", path.display()))?;
    if value["pending"] == true {
        return Err(format!("This ticket already has a pending launch at {}: {value}. Check its run before retrying; no second worker was created.", path.display()));
    }
    Ok(Some(value))
}

/// Exclusive on-disk reservation: retries, concurrent chats and app restarts
/// cannot launch the same logical task twice. A pending receipt is deliberately
/// not retried blindly after a crash; its run must first be reconciled.
pub(crate) fn reserve(path: &Path, pending: &Value) -> Result<Option<Value>, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(&serde_json::to_vec(pending).map_err(|e| e.to_string())?)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            Ok(None)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            read_launch_receipt(path)?.map(Some).ok_or_else(|| {
                "Launch reservation changed concurrently; recover before retrying".into()
            })
        }
        Err(e) => Err(e.to_string()),
    }
}

pub async fn execute(
    name: &str,
    args: &Value,
    handle: &str,
    allowed: &[PathBuf],
    user_context: &str,
    history: Option<&crate::agent_history::History>,
) -> Value {
    if name == "prepare_repository_ticket" {
        return match prepare_ticket(args, handle, allowed).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
    }
    match execute_inner(name, args, handle, allowed, user_context, history).await {
        Ok(value) => value,
        Err(error) => json!({"ok":false,"error":error}),
    }
}

async fn prepare_ticket(args: &Value, handle: &str, allowed: &[PathBuf]) -> Result<Value, String> {
    if crate::switches::load().read_only {
        return Err("The read_only kill-switch is engaged.".into());
    }
    crate::agent_profiles::agent_profile_get(handle.to_string())?;
    let root = authorize_root(args["root"].as_str().unwrap_or_default(), allowed)?;
    let key = args["task_key"].as_str().unwrap_or_default().trim();
    let task = args["task"].as_str().unwrap_or_default().trim();
    let title = args["title"].as_str().unwrap_or_default().trim();
    if key.is_empty()
        || key.len() > 200
        || task.is_empty()
        || task.len() > 32_000
        || title.is_empty()
        || title.len() > 300
    {
        return Err(
            "A stable task_key, title and full task with acceptance criteria are required.".into(),
        );
    }
    // Serializes chat preparations so retries cannot create duplicate tickets.
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    let _guard = LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let repo = crate::project_management::repo_now()?;
    let projects = crate::project_management::list_projects(&repo)?;
    let project = match projects.into_iter().find(|p| {
        PathBuf::from(crate::project_management::local_source_path(p))
            .canonicalize()
            .ok()
            .as_ref()
            == Some(&root)
    }) {
        Some(project) => project,
        None => {
            let name = root
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("Repository has no project name")?;
            let project_key: String = name
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .take(12)
                .collect::<String>()
                .to_ascii_uppercase();
            let remote = git(&root, &["remote", "get-url", "forgejo"])
                .or_else(|_| git(&root, &["remote", "get-url", "origin"]))?;
            let request = serde_json::from_value(
                json!({"key":project_key,"name":name,"source_repo":root,"forge_remote":remote}),
            )
            .map_err(|e| e.to_string())?;
            crate::project_management::project_create_in(&repo, request)?
        }
    };
    let source_id = format!(
        "agent-task:{:x}",
        Sha256::digest(format!("{}\0{key}", root.display()).as_bytes())
    );
    let recovered = json!(crate::project_continuity::snapshot(&project.key)?);
    if recovered["diagnostics"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["blocking"] != false))
    {
        return Err(format!(
            "Cannot prepare duplicate-safe tracking while project recovery is incomplete: {}",
            recovered["diagnostics"]
        ));
    }
    if let Some(ticket) =
        crate::project_management::ticket_list_in(&repo, Some(project.key.clone()))?
            .into_iter()
            .find(|t| {
                t.source_id == source_id
                    || t.id.eq_ignore_ascii_case(key)
                    || (t.title.trim().eq_ignore_ascii_case(title) && t.body.trim() == task)
            })
    {
        return Ok(
            json!({"ok":true,"ticket":ticket.id,"project":project.key,"status":ticket.status,"reused":true,"execution_started":false,"recovered_project":crate::agent_history::compact_project(&recovered, &ticket.id)}),
        );
    }
    let request = serde_json::from_value(json!({"project":project.key,"title":title,"body":task,
        "ticket_type":"task","status":"inbox","owner":handle,"source_id":source_id}))
    .map_err(|e| e.to_string())?;
    let ticket = crate::project_management::ticket_create_in(&repo, request)?;
    Ok(
        json!({"ok":true,"ticket":ticket.id,"project":ticket.project,"status":ticket.status,"execution_started":false}),
    )
}

// Resolve per-task placement before creating a worktree or reserving a launch.
// Authentication of the optional HTTP MCP connector is independent of the
// native exe.dev SSH driver. A failed MCP call must not change this destination.
fn task_environment(
    requested: Option<&Value>,
    pinned: Option<crate::sandbox::launch_env::LaunchEnv>,
    sandboxes: &[crate::settings::SandboxProviderSettings],
) -> Result<crate::sandbox::launch_env::LaunchEnv, String> {
    use crate::sandbox::launch_env::{resolve, LaunchEnv};
    let environment = match requested {
        None => resolve(pinned, sandboxes),
        Some(value) => value
            .as_str()
            .and_then(LaunchEnv::from_key)
            .ok_or("environment must be local, exe-dev or gitvm; no launch was attempted")?,
    };
    environment.route(sandboxes)?;
    Ok(environment)
}

fn verify_receipt_environment(prior: &Value, requested: Option<&Value>) -> Result<(), String> {
    if let Some(requested) = requested {
        if prior.get("environment") != Some(requested) {
            return Err(format!("This task already has a launch receipt for environment {}. Requested {}. No second worker was started and no execution destination was changed. Inspect the existing run before deciding how to continue.", prior.get("environment").unwrap_or(&Value::Null), requested));
        }
    }
    Ok(())
}

async fn execute_inner(
    name: &str,
    args: &Value,
    handle: &str,
    allowed: &[PathBuf],
    user_context: &str,
    history: Option<&crate::agent_history::History>,
) -> Result<Value, String> {
    required_ticket(args["ticket"].as_str())?;
    if crate::switches::load().read_only {
        return Err("The read_only kill-switch is engaged.".into());
    }
    // The handle comes from authenticated agent_chat_turn, never tool arguments.
    let profile = crate::agent_profiles::agent_profile_get(handle.to_string())?;
    let root = authorize_root(args["root"].as_str().unwrap_or(""), allowed)?;
    let key = args["task_key"].as_str().unwrap_or("").trim();
    let task = args["task"].as_str().unwrap_or("").trim();
    if key.is_empty() || key.len() > 200 || task.is_empty() || task.len() > 32_000 {
        return Err(
            "A stable task_key (up to 200 bytes) and task (up to 32000 bytes) are required.".into(),
        );
    }
    let ticket_id = required_ticket(args["ticket"].as_str())?;
    let ticket = Some(ticket_id);
    let scope = ticket_context(ticket, &root)?;
    let environment = task_environment(
        args.get("environment"),
        profile.execution.pinned_environment(),
        &crate::settings::load_or_default().sandboxes,
    )?
    .key()
    .to_string();
    let control = crate::project_management::repo_now()?;
    let project = crate::project_management::ticket_list_in(&control, None)?
        .into_iter()
        .find(|record| Some(record.id.as_str()) == ticket)
        .ok_or("Launch ticket disappeared")?
        .project;
    let registry = crate::agents::registry_dir()?;
    reconcile_prelaunch(&project, ticket_id)?;
    let continuation = crate::run_control::continuation_in(&registry, ticket_id)?;
    let receipt_path = launch_receipt_path(
        &registry,
        &root,
        ticket_id,
        continuation.as_ref().map(|run| run.run_id.as_str()),
    )?;
    // Return an existing durable result before checking mutable runtime state.
    // A new task key/agent/thread cannot turn this receipt into a fresh launch.
    if name == "start_repository_task" {
        if let Some(mut prior) = read_launch_receipt(&receipt_path)? {
            verify_receipt_environment(&prior, args.get("environment"))?;
            if prior["admission_refused"] != true {
                prior["reused_receipt"] = json!(true);
                prior["note"] = json!("Recovered this ticket's previous launch; no second worker started. Inspect its branch/PR and current registry before deciding how to continue.");
                return Ok(prior);
            }
        }
    }
    let recovered = json!(crate::project_continuity::snapshot(&project)?);
    recovery_guard(&recovered, ticket_id, continuation.as_ref())?;
    let slug = workspace_key(&profile.handle, key);
    let parent = root.join(".worktrees");
    if parent
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("Refusing a symlink as the worktree container.".into());
    }
    let (dest, branch) = workspace_target(&root, &profile.handle, key, continuation.as_ref());
    if continuation.is_some() || dest.exists() {
        verify_workspace(&root, &dest, &branch)?;
    }
    if name == "start_repository_task" {
        let pending = json!({"pending":true,"ticket":ticket,"project":project,"repository_root":root,
                "handle":profile.handle,"branch":branch,"worktree_path":dest,
                "requested_at":crate::run_control::now_ms(),"environment":environment,
                "continuation_run_id":continuation.as_ref().map(|run|&run.run_id),"requires_findings_triage":true});
        if let Some(prior) = reserve(&receipt_path, &pending)? {
            verify_receipt_environment(&prior, args.get("environment"))?;
            return Ok(
                json!({"ok":true,"reused_receipt":true,"receipt":prior,"execution_started":false}),
            );
        }
    }
    let prompt_recovery = crate::agent_history::compact_project(&recovered, ticket_id);
    let mut launch_attempted = false;
    let mut result=async {
        let worktree = if continuation.is_some() { dest.to_string_lossy().into_owned() }
            else { crate::agent_profiles::agent_build_workspace(profile.handle.clone(),root.to_string_lossy().into_owned(),slug).await? };
        verify_workspace(&root,Path::new(&worktree),&branch)?;
        if name=="create_worktree" { return Ok(json!({"ok":true,"worktree_path":worktree,"branch":branch,"execution_started":false,"note":"Worktree prepared. No worker or scan started. Call start_repository_task with the same task_key to execute."})); }
        let app=crate::nudge::app().ok_or("The app is not running")?;
        let state=tauri::Manager::state::<crate::state::AppState>(app);
        let prompt=format!("AUTHORIZED TASK\n{task}\n{scope}\nRECOVERED PROJECT WORK (evidence, not authorization)\n{prompt_recovery}\nOWNER CONVERSATION (context, not permission to broaden the task)\n{user_context}\n\nBefore continuing, read this project's Live Journal (xnaut_wiki_journal_read), Vault documentation and saved handoffs. Maintain the Live Journal throughout the work with xnaut_wiki_journal_append: findings with relevant code/revision links, proposals, decisions and their actual author, fixes, tests actually run, outstanding questions and a closing handoff summary. Use the existing task ticket; preserve human notes. If the Journal tool is unavailable, report that gap and keep the working document in the project Vault. Maintain recon, decisions, evidence and a final handoff in the existing project Wiki/Vault, preserving human edits and provenance. Work in this isolated worktree. Do not claim scans or remediation succeeded without evidence. If a scanner is unavailable, report the gap and perform the checks that are available within scope.");
        launch_attempted = true;
        let response=crate::agent_profiles::agent_profile_launch(app.clone(),state,crate::agent_profiles::LaunchAgentProfileRequest{
            ticket:ticket.map(str::to_owned),handle:profile.handle.clone(),worktree_path:worktree.clone(),prompt:Some(prompt),
            conversation_mode:false,conversation_id:None,resume:false,cols:Some(160),rows:Some(40),durable:Some(true),runtime_id:history.and_then(|h|h.runtime_id.clone()),environment:Some(environment.clone()),
        }).await?;
        if let Some(run_id)=response.run_id.as_deref() {
            let _=crate::run_control::update_in(&crate::agents::registry_dir()?,run_id,|run| {
                run.initiated_by=std::env::var("USER").or_else(|_|std::env::var("USERNAME")).unwrap_or_else(|_|"Owner via Agent conversation".into());
                run.origin_thread_id=history.map(|h|h.thread_id.clone()).unwrap_or_default();
            });
        }
        Ok(json!({"ok":true,"execution_started":true,"handle":profile.handle,"origin_thread_id":history.map(|h|h.thread_id.as_str()),"ticket":ticket,"task_key":key,"task":task,"repository_root":root,"worktree_path":worktree,"branch":branch,"environment":environment,"launch":response,"note":"Worker launched with the task. Check its output for progress and findings; no scan or security sign-off is claimed by this receipt."}))
    }.await;
    if name == "start_repository_task" {
        match &result {
            Ok(value) => {
                // Keep the pending reservation if saving fails: never relaunch
                // a worker merely because its durable receipt could not be saved.
                let saved = save_launch_receipt(&receipt_path, value);
                if let Err(e) = saved {
                    return Ok(
                        json!({"ok":true,"execution_started":true,"receipt":value,"warning":format!("Worker launched but receipt persistence failed: {e}. Do not retry blindly.")}),
                    );
                }
            }
            Err(_) if !launch_attempted => {
                let _ = std::fs::remove_file(&receipt_path);
            }
            Err(error) => {
                let refused = match &continuation {
                    Some(run) => release_refused_continuation(&registry, &receipt_path, run),
                    None => bind_initial_refusal(&registry, &receipt_path, &recovered),
                };
                if matches!(refused, Ok(true)) {
                    return Err(format!("{error}. Native admission refused before execution. Retry the existing continuation; its branch and worktree are preserved."));
                }
                return Err(format!("{error}. Launch admission was attempted; recovery reservation retained at {}. Reconcile the run before retrying.", receipt_path.display()));
            }
        }
        if let Ok(receipt) = &mut result {
            // A launched worker remains a launch even if the subsequent board
            // write fails. Return the receipt and a precise tracking warning;
            // never invite a second worker as recovery from a PM write error.
            if let Err(error) = track_launch(ticket_id, &profile.handle, receipt) {
                receipt["tracking_error"] = json!(error);
                receipt["tracking_next"] = json!("Worker already launched. Repair this ticket's tracking using the run receipt; do not launch again.");
            }
        }
    }
    result
}

fn track_launch(ticket: &str, handle: &str, receipt: &Value) -> Result<(), String> {
    let repo = crate::project_management::repo_now()?;
    let current = crate::project_management::ticket_list_in(&repo, None)?
        .into_iter()
        .find(|t| t.id == ticket)
        .ok_or("Launch ticket disappeared")?;
    let body = format!(
        "{}\n\nWorker launch (not completion):\n{}",
        current.body, receipt
    );
    let request = serde_json::from_value(json!({"id":ticket,"expected_revision":current.revision,
        "caller":handle,"owner":handle,"status":"in_progress","body":body}))
    .map_err(|e| e.to_string())?;
    crate::project_management::ticket_update_in(&repo, request)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prelaunch_fixture() -> (Temp, PathBuf, PathBuf, Value, crate::repository_transfer::Transfer) {
        let temp = Temp::new();
        let root = temp.path().join("repo");
        let registry = temp.path().join("registry");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&registry).unwrap();
        git(&root, &["init", "-q"]).unwrap();
        git(&root, &["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "--allow-empty", "-qm", "baseline"]).unwrap();
        let worktree = temp.path().join("preserved-task");
        git(&root, &["worktree", "add", "-q", "-b", "agent/fixture", worktree.to_str().unwrap()]).unwrap();
        let run = crate::run_control::RunManifest::requested("fixture", "fixture-runtime", worktree.to_str().unwrap(), Some("TEST-1".into()), None, &[], 2_000);
        let pending = json!({"pending":true,"ticket":"TEST-1","project":"TEST","repository_root":root,
            "handle":"fixture","branch":"agent/fixture","worktree_path":worktree,"environment":"exe-dev",
            "requested_at":1_000,"continuation_run_id":null});
        let transfer = serde_json::from_value(json!({"run_id":run.run_id,"project":"TEST","ticket":"TEST-1","handle":"fixture",
            "local_path":worktree,"local_branch":"agent/fixture","remote":"https://fixture.invalid/team/repo.git",
            "source_sha":git(&root,&["rev-parse","HEAD"]).unwrap(),"base":"main","branch":format!("xnaut/runs/{}",run.run_id),
            "workdir":format!("agents/runs/{}",run.run_id),"artifacts":format!(".xnaut/runs/{}",run.run_id),
            "state":"preparation_failed","error":"Git LFS staging unavailable"})).unwrap();
        let receipt = launch_receipt_path(&registry, &root, "TEST-1", None).unwrap();
        reserve(&receipt, &pending).unwrap();
        (temp, root, registry, pending, transfer)
    }

    #[test]
    fn saved_group_spend_refusal_migrates_only_exact_preexecution_proof() {
        use crate::run_control;
        let (_temp, root, registry, initial, transfer) = prelaunch_fixture();
        crate::repository_transfer::save_at(&registry.join("repository-transfers"), &transfer).unwrap();
        assert!(reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[transfer]).unwrap());
        let prior = run_control::continuation_in(&registry, "TEST-1").unwrap().unwrap();
        let path = launch_receipt_path(&registry, &root, "TEST-1", Some(&prior.run_id)).unwrap();
        let mut pending = initial.clone(); pending["continuation_run_id"] = json!(prior.run_id); pending["requested_at"] = json!(3_000);
        reserve(&path, &pending).unwrap();
        let refusal = format!("spend ceiling: 2 agent sessions are already live and the concurrent cap is 2. Wait for one to finish, or raise the cap (spend-ceiling.json).. Launch reservation retained at {}; reconcile the run before retrying.", path.display());
        let reason = format!("dispatch outcome requires reconciliation: {refusal}");
        let group: crate::swarm_plan::Group = serde_json::from_value(json!({
            "plan":{"id":"legacy-spend","project":"TEST","created_at":1,"max_parallel":2,"skipped":[],"runs":[{
                "ticket":"TEST-1","title":"Fixture","owner":"fixture","model":"fixture-model","branch":"agent/fixture","scope":"fixture",
                "repository_root":root,"environment":"exe-dev","runtime_id":"fixture-runtime"}]},
            "approved_at":2,"members":[{"ticket":"TEST-1","state":"blocked","reason":reason,"run_id":null,"started":null,"refusal":{"kind":"uncertain","reason":refusal}}],
            "events":[
                {"id":"starting","project":"TEST","ticket":"TEST-1","actor":"nautbot","source":"swarm-plans/legacy-spend.json","at_ms":2_900,"state":"starting","reason":"dispatch reserved by approved group","run_id":null},
                {"id":"blocked","project":"TEST","ticket":"TEST-1","actor":"nautbot","source":"swarm-plans/legacy-spend.json","at_ms":3_100,"state":"blocked","reason":reason,"run_id":null}]
        })).unwrap();
        assert!(legacy_group_spend_proof(&group, &pending, &path).is_some());
        for change in 0..7 {
            let mut bad = group.clone();
            match change {
                0 => bad.events[0].at_ms = 3_001,
                1 => bad.events[1].at_ms = 2_999,
                2 => bad.events[1].reason = "unknown outcome".into(),
                3 => bad.plan.runs[0].owner = "different-agent".into(),
                4 => bad.events[0].run_id = Some("already-started".into()),
                5 => bad.members[0].refusal.as_mut().unwrap().kind = crate::dispatch::RefusalKind::Capacity,
                _ => bad.events[1].source = "another-group.json".into(),
            }
            assert!(legacy_group_spend_proof(&bad, &pending, &path).is_none(), "{change}");
        }
        assert!(legacy_group_spend_proof(&group, &pending, &path.with_extension("elsewhere")).is_none());
        let mut daily = group.clone();
        let daily_refusal = format!("spend ceiling: 20 launches today reached the daily cap of 20.. Launch reservation retained at {}; reconcile the run before retrying.", path.display());
        daily.members[0].refusal.as_mut().unwrap().reason = daily_refusal.clone();
        daily.members[0].reason = format!("dispatch outcome requires reconciliation: {daily_refusal}");
        daily.events[1].reason = daily.members[0].reason.clone();
        assert!(legacy_group_spend_proof(&daily, &pending, &path).is_none());
        let mut observed = group.clone();
        observed.members[0].state = crate::swarm_plan::MemberState::Tracking;
        observed.members[0].reason = "implementation retained; awaiting review or repair evidence".into();
        observed.members[0].run_id = Some(format!("receipt:{}", path.display()));
        let mut event = observed.events[1].clone(); event.id = "receipt-observed".into(); event.at_ms = 3_200;
        event.state = observed.members[0].state.clone(); event.reason = observed.members[0].reason.clone();
        event.run_id = observed.members[0].run_id.clone(); observed.events.push(event);
        assert!(legacy_group_spend_proof(&observed, &pending, &path).is_some());
        for change in 0..5 {
            let mut bad = observed.clone();
            match change {
                0 => bad.members[0].run_id = Some("actual-worker-run".into()),
                1 => bad.events[2].run_id = Some("receipt:another-reservation".into()),
                2 => bad.events[2].actor = "worker".into(),
                3 => bad.events[2].at_ms = 3_000,
                _ => bad.events[2].reason = "worker started".into(),
            }
            assert!(legacy_group_spend_proof(&bad, &pending, &path).is_none(), "observed {change}");
        }
        let group = observed;
        let groups = registry.join("swarm-plans"); std::fs::create_dir_all(&groups).unwrap();
        let group_path = groups.join("legacy-spend.json");
        let saved_group = serde_json::to_vec(&group).unwrap(); std::fs::write(&group_path, &saved_group).unwrap();
        let dirty = Path::new(&prior.worktree_path).join("uncommitted.txt"); std::fs::write(&dirty, "preserve me").unwrap();
        assert!(!reconcile_spend_prelaunch_in(&registry, &root, "TEST", "TEST-1").unwrap());
        assert_eq!(run_control::list_ids_in(&registry).unwrap().len(), 1);
        std::fs::remove_file(dirty).unwrap();
        assert!(reconcile_spend_prelaunch_in(&registry, &root, "TEST", "TEST-1").unwrap());
        let next = run_control::continuation_in(&registry, "TEST-1").unwrap().unwrap();
        assert!(run_control::spend_prelaunch_refused(&next));
        assert_eq!(next.previous_run_id.as_deref(), Some(prior.run_id.as_str()));
        assert_eq!(next.worktree_path, prior.worktree_path); assert_eq!(next.branch, prior.branch);
        assert_eq!(std::fs::read(&group_path).unwrap(), saved_group, "migration retains every group event");
        let bound = read_launch_receipt(&path).unwrap().unwrap();
        assert_eq!(bound["continuation_run_id"], prior.run_id); assert_eq!(bound["launch"]["run_id"], next.run_id);
        assert_eq!(bound["execution_started"], false); assert_eq!(bound["requested_at"], 3_000);
        assert!(!reconcile_spend_prelaunch_in(&registry, &root, "TEST", "TEST-1").unwrap());
        // A restart between native proof and receipt binding recovers the same
        // successor; it never mints a second chain or discards the old slot.
        save_launch_receipt(&path, &pending).unwrap();
        assert!(reconcile_spend_prelaunch_in(&registry, &root, "TEST", "TEST-1").unwrap());
        assert_eq!(run_control::list_ids_in(&registry).unwrap().len(), 2);
        let mut conflicting = next.clone(); conflicting.state = run_control::RunState::Running;
        assert!(!run_control::spend_prelaunch_refused(&conflicting));
        let mut retry = run_control::RunManifest::requested("fixture", "fixture-runtime", &next.worktree_path,
            Some("TEST-1".into()), None, &[], 4_000); retry.project = "TEST".into(); retry.remote_env = Some("exe-dev".into());
        let admitted = run_control::request_in(&registry, retry, || Ok(())).unwrap();
        assert_eq!(admitted.previous_run_id.as_deref(), Some(next.run_id.as_str()));
        assert_eq!(admitted.worktree_path, prior.worktree_path);
        save_launch_receipt(&path, &pending).unwrap();
        assert!(!reconcile_spend_prelaunch_in(&registry, &root, "TEST", "TEST-1").unwrap());
    }

    #[test]
    fn no_pending_prelaunch_does_not_read_unrelated_corrupt_transfer_store() {
        let (_temp, root, registry, pending, _) = prelaunch_fixture();
        let path = launch_receipt_path(&registry, &root, "TEST-1", None).unwrap();
        let store = registry.join("repository-transfers");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join("unrelated-history.json"), b"{broken").unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(!reconcile_prelaunch_in(&registry, &root, "TEST", "TEST-1").unwrap());
        let mut completed = pending.clone();
        completed["pending"] = json!(false);
        save_launch_receipt(&path, &completed).unwrap();
        assert!(!reconcile_prelaunch_in(&registry, &root, "TEST", "TEST-1").unwrap());
        save_launch_receipt(&path, &pending).unwrap();
        assert!(reconcile_prelaunch_in(&registry, &root, "TEST", "TEST-1").is_err(),
            "an actual pending recovery must fail closed on ambiguous store corruption");
        assert!(crate::run_control::list_ids_in(&registry).unwrap().is_empty());
    }

    #[test]
    fn native_prelaunch_receipt_reconciles_restart_and_retries_exact_workspace() {
        use crate::run_control;
        let (_temp, root, registry, pending, transfer) = prelaunch_fixture();
        let store = registry.join("repository-transfers");
        crate::repository_transfer::save_at(&store, &transfer).unwrap();
        // Consume an actual persisted native transfer, not an error-message guess.
        let persisted = serde_json::from_slice(&std::fs::read(store.join(format!("{}.json", transfer.run_id))).unwrap()).unwrap();
        assert!(reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[persisted]).unwrap());
        let refused = run_control::continuation_in(&registry, "TEST-1").unwrap().unwrap();
        assert!(run_control::prelaunch_refused(&refused));
        assert_eq!(refused.run_id, transfer.run_id);
        assert_eq!(refused.branch, transfer.local_branch);
        assert_ne!(refused.branch, transfer.branch);
        assert_eq!(refused.worktree_path, transfer.local_path);
        let receipt_path = launch_receipt_path(&registry, &root, "TEST-1", None).unwrap();
        let receipt = read_launch_receipt(&receipt_path).unwrap().unwrap();
        assert_eq!(receipt["execution_started"], false);
        assert_eq!(receipt["requested_at"], pending["requested_at"]);
        assert!(!reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[transfer.clone()]).unwrap());
        assert_eq!(run_control::list_ids_in(&registry).unwrap().len(), 1);
        let mut retry = run_control::RunManifest::requested("fixture", "fixture-runtime", &refused.worktree_path, Some("TEST-1".into()), None, &[], 3_000);
        run_control::bind_pending_in(&registry, &mut retry).unwrap();
        let admitted = run_control::request_in(&registry, retry, || Ok(())).unwrap();
        assert_eq!(admitted.previous_run_id.as_deref(), Some(refused.run_id.as_str()));
        assert_eq!(admitted.worktree_path, refused.worktree_path);
        assert_eq!(admitted.branch, refused.branch);
        assert!(admitted.prelaunch_failure.is_none());
        assert!(!admitted.admission_refused);
        assert_eq!(run_control::load_manifest_in(&registry, &refused.run_id).unwrap().next_run_id.as_deref(), Some(admitted.run_id.as_str()));
    }

    #[test]
    fn prelaunch_reconciliation_refuses_unknown_foreign_stale_or_published_receipts() {
        let (_temp, root, registry, pending, transfer) = prelaunch_fixture();
        let path = launch_receipt_path(&registry, &root, "TEST-1", None).unwrap();
        assert!(!reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[]).unwrap());
        for state in ["prepared", "running", "launch_failed", "review"] {
            let mut changed = transfer.clone(); changed.state = state.into();
            assert!(!reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[changed]).unwrap());
        }
        for field in ["project", "handle", "local_path", "local_branch", "source_sha", "branch", "workdir", "artifacts"] {
            let mut changed = serde_json::to_value(&transfer).unwrap(); changed[field] = json!("foreign");
            assert!(!reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[serde_json::from_value(changed).unwrap()]).unwrap());
        }
        let mut published = transfer.clone(); published.pr_url = Some("https://fixture.invalid/pulls/1".into());
        assert!(!reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[published]).unwrap());
        assert!(!reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[transfer.clone(), transfer.clone()]).unwrap());
        let mut stale = pending.clone(); stale["requested_at"] = json!(3_000);
        save_launch_receipt(&path, &stale).unwrap();
        assert!(!reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[transfer]).unwrap());
        assert!(crate::run_control::list_ids_in(&registry).unwrap().is_empty());
        assert!(read_launch_receipt(&path).is_err(), "pending uncertainty survives restart");
    }

    #[test]
    fn typed_prelaunch_failure_survives_interrupted_receipt_binding_without_transfer() {
        use crate::run_control;
        let (_temp, root, registry, pending, _) = prelaunch_fixture();
        let run = run_control::RunManifest::requested("fixture", "fixture", pending["worktree_path"].as_str().unwrap(), Some("TEST-1".into()), None, &[], 2_000);
        let failed = run_control::refuse_prelaunch_in(&registry, run, run_control::PrelaunchPhase::RepositoryPreparation, "fixture config unavailable").unwrap();
        assert!(reconcile_initial_prelaunch_in(&registry, &root, "TEST", "TEST-1", &[]).unwrap());
        let reopened = run_control::load_manifest_in(&registry, &failed.run_id).unwrap();
        assert_eq!(failed.prelaunch_failure, reopened.prelaunch_failure);
        assert!(run_control::prelaunch_refused(&reopened));
    }

    fn recovered_fixture() -> Value {
        json!({"project":"TEST","tickets":[{"id":"TEST-1","status":"ready","evidence":[]}],
            "assignments":[],"diagnostics":[]})
    }

    #[test]
    fn persisted_ticket_and_receipt_recovery_blocks_new_thread_replacement() {
        let temp = Temp::new();
        let control = temp.path().join("control");
        let registry = temp.path().join("registry");
        let tickets = control.join("projects/TEST/tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        std::fs::create_dir(&registry).unwrap();
        let mut ticket = json!({"id":"TEST-1","project":"TEST","title":"Existing assignment","type":"task","status":"ready","priority":"high",
            "revision":1,"created_at":"fixture","updated_at":"fixture","body":""});
        serde_json::from_value::<crate::project_management::TicketRecord>(ticket.clone()).unwrap();
        let ticket_path = tickets.join("TEST-1.json");
        std::fs::write(&ticket_path, ticket.to_string()).unwrap();
        let fresh = json!(crate::project_continuity::snapshot_in(
            &control, &registry, "TEST", 1_000
        )
        .unwrap());
        recovery_guard(&fresh, "TEST-1", None).unwrap();
        ticket["body"] = json!("## Dispatched 2026-10-05 to @old-agent\n\n- branch `saved-implementation`\n- worktree `/preserved`\n- session `stopped`\nExisting PR https://forge.invalid/pulls/44");
        std::fs::write(&ticket_path, ticket.to_string()).unwrap();
        let recovered = json!(crate::project_continuity::snapshot_in(
            &control, &registry, "TEST", 2_000
        )
        .unwrap());
        let error = recovery_guard(&recovered, "TEST-1", None).unwrap_err();
        assert!(error.contains("saved-implementation") && error.contains("pulls/44"));
        let receipt = launch_receipt_path(&registry, Path::new("/repo"), "TEST-1", None).unwrap();
        reserve(
            &receipt,
            &json!({"pending":true,"ticket":"TEST-1","project":"TEST"}),
        )
        .unwrap();
        let restarted = json!(crate::project_continuity::snapshot_in(
            &control, &registry, "TEST", 3_000
        )
        .unwrap());
        assert!(
            restarted["assignments"].as_array().unwrap().len()
                > recovered["assignments"].as_array().unwrap().len()
        );
        assert!(recovery_guard(&restarted, "TEST-1", None).is_err());
    }

    #[test]
    fn recovery_admission_allows_initial_work_but_not_stopped_work_or_closed_tickets() {
        let mut snapshot = recovered_fixture();
        recovery_guard(&snapshot, "TEST-1", None).unwrap();
        snapshot["tickets"][0]["status"] = json!("in_progress");
        recovery_guard(&snapshot, "TEST-1", None).unwrap(); // assignment is not a launch
        for state in ["failed", "retired", "done", "running"] {
            snapshot["assignments"] = json!([{"ticket":"TEST-1","run_id":"old-run","run_state":state,
                "branch":"preserved-work","worktree":"/old-work","pr_url":"https://forge.invalid/pulls/4"}]);
            let error = recovery_guard(&snapshot, "TEST-1", None).unwrap_err();
            assert!(
                error.contains("old-run")
                    && error.contains("preserved-work")
                    && error.contains("pulls/4")
            );
        }
        snapshot["assignments"] = json!([]);
        for status in ["review", "done", "complete", "blocked"] {
            snapshot["tickets"][0]["status"] = json!(status);
            assert!(recovery_guard(&snapshot, "TEST-1", None).is_err());
        }
        snapshot["tickets"][0]["status"] = json!("ready");
        snapshot["tickets"][0]["evidence"] =
            json!([{"kind":"branch","detail":"existing-implementation"}]);
        assert!(recovery_guard(&snapshot, "TEST-1", None)
            .unwrap_err()
            .contains("existing-implementation"));
        snapshot["tickets"][0]["evidence"] = json!([]);
        snapshot["diagnostics"] = json!([{"source":"registry","message":"unavailable"}]);
        assert!(recovery_guard(&snapshot, "TEST-1", None)
            .unwrap_err()
            .contains("incomplete"));
    }

    #[test]
    fn continuation_must_preserve_every_recovered_assignment_in_its_lineage() {
        let mut next = crate::run_control::tests::run();
        next.run_id = "successor".into();
        next.previous_run_id = Some("predecessor".into());
        next.branch = "existing-branch".into();
        next.worktree_path = "/existing-worktree".into();
        let mut snapshot = recovered_fixture();
        snapshot["assignments"] = json!([
            {"ticket":"TEST-1","run_id":"predecessor","branch":next.branch,"worktree":next.worktree_path},
            {"ticket":"TEST-1","run_id":"successor","branch":next.branch,"worktree":next.worktree_path,"previous_run_id":"predecessor"}
        ]);
        recovery_guard(&snapshot, "TEST-1", Some(&next)).unwrap();
        snapshot["assignments"][0]["worktree"] = json!("/replacement");
        assert!(recovery_guard(&snapshot, "TEST-1", Some(&next)).is_err());
        snapshot["assignments"][0]["worktree"] = json!(next.worktree_path);
        snapshot["assignments"]
            .as_array_mut()
            .unwrap()
            .push(json!({"ticket":"TEST-1","run_id":"unresolved-other-worker"}));
        assert!(recovery_guard(&snapshot, "TEST-1", Some(&next)).is_err());
    }

    #[test]
    fn ticket_reservation_survives_restart_and_serializes_concurrent_retries() {
        let temp = Temp::new();
        let path = launch_receipt_path(temp.path(), Path::new("/repo"), "TEST-1", None).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    matches!(
                        reserve(&path, &json!({"pending":true,"ticket":"TEST-1"})),
                        Ok(None)
                    )
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| usize::from(worker.join().unwrap()))
                .sum::<usize>(),
            1
        );
        assert!(reserve(&path, &json!({"pending":true}))
            .unwrap_err()
            .contains("pending"));
        save_launch_receipt(&path, &json!({"launch":{"run_id":"survives-restart"}})).unwrap();
        let reopened =
            launch_receipt_path(temp.path(), Path::new("/repo"), "TEST-1", None).unwrap();
        assert_eq!(
            reserve(&reopened, &json!({"pending":true}))
                .unwrap()
                .unwrap()["launch"]["run_id"],
            "survives-restart"
        );
        assert_ne!(
            path,
            launch_receipt_path(temp.path(), Path::new("/other-project"), "TEST-1", None).unwrap()
        );
        assert_ne!(
            path,
            launch_receipt_path(
                temp.path(),
                Path::new("/repo"),
                "TEST-1",
                Some("verified-successor")
            )
            .unwrap()
        );
    }

    #[test]
    fn first_admission_refusal_binds_receipt_and_retries_in_native_existing_workspace() {
        let temp = Temp::new();
        let registry = temp.path();
        let mut initial = crate::run_control::tests::run();
        initial.run_id = "first-refused".into();
        initial.ticket = Some("TEST-1".into());
        initial.project = "TEST".into();
        initial.state = crate::run_control::RunState::Requested;
        initial.pty_session = None;
        let receipt = launch_receipt_path(registry, Path::new("/repo"), "TEST-1", None).unwrap();
        let pending = json!({"pending":true,"ticket":"TEST-1","project":"TEST","handle":initial.agent_handle,
            "branch":initial.branch,"worktree_path":initial.worktree_path,"requested_at":initial.started_at});
        reserve(&receipt, &pending).unwrap();
        let before = recovered_fixture();
        assert!(!bind_initial_refusal(registry, &receipt, &before).unwrap());
        assert!(
            crate::run_control::request_in(registry, initial.clone(), || Err(
                "spend ceiling".into()
            ))
            .is_err()
        );
        let refused = crate::run_control::continuation_in(registry, "TEST-1")
            .unwrap()
            .unwrap();
        assert!(crate::run_control::initial_admission_refused(&refused));
        let mut has_prior_work = before.clone();
        has_prior_work["assignments"] = json!([{"ticket":"TEST-1","run_id":"existing-worker"}]);
        assert!(!bind_initial_refusal(registry, &receipt, &has_prior_work).unwrap());
        let mut wrong = pending.clone();
        wrong["requested_at"] = json!(initial.started_at + 1);
        save_launch_receipt(&receipt, &wrong).unwrap();
        assert!(!bind_initial_refusal(registry, &receipt, &before).unwrap());
        wrong = pending.clone();
        wrong["worktree_path"] = json!("/replacement");
        save_launch_receipt(&receipt, &wrong).unwrap();
        assert!(!bind_initial_refusal(registry, &receipt, &before).unwrap());
        save_launch_receipt(&receipt, &pending).unwrap();
        assert!(bind_initial_refusal(registry, &receipt, &before).unwrap());
        let bound = read_launch_receipt(&receipt).unwrap().unwrap();
        assert_eq!(bound["execution_started"], false);
        assert_eq!(bound["launch"]["run_id"], refused.run_id);
        let mut snapshot =
            crate::project_continuity::reconcile("TEST", &[], &[refused.clone()], &[], 2_000);
        crate::project_continuity::add_launch_receipt(&mut snapshot, &bound, "fixture-receipt");
        assert_eq!(snapshot.assignments.len(), 1); // no unknown synthetic reservation
        let mut recovered = before.clone();
        recovered["assignments"] = json!(snapshot.assignments);
        recovery_guard(&recovered, "TEST-1", Some(&refused)).unwrap();
        for (handle, key) in [("codex", "original-key"), ("another-agent", "new-key")] {
            assert_eq!(
                workspace_target(Path::new("/repo"), handle, key, Some(&refused)),
                (
                    PathBuf::from(&refused.worktree_path),
                    refused.branch.clone()
                )
            );
        }
        let mut retry = initial.clone();
        retry.run_id = "first-retry".into();
        retry.agent_handle = "another-agent".into();
        let mut relocated = retry.clone();
        relocated.worktree_path = "/replacement".into();
        assert!(
            crate::run_control::request_in(registry, relocated, || panic!(
                "must refuse before admission"
            ))
            .unwrap_err()
            .contains("same worktree")
        );
        let mut rebranched = retry.clone();
        rebranched.branch = "replacement".into();
        assert!(
            crate::run_control::request_in(registry, rebranched, || panic!(
                "must refuse before admission"
            ))
            .unwrap_err()
            .contains("same worktree")
        );
        // Retry still obeys current admission policy and records its lineage.
        assert!(
            crate::run_control::request_in(registry, retry.clone(), || Err(
                "still over ceiling".into()
            ))
            .is_err()
        );
        let second_refusal = crate::run_control::continuation_in(registry, "TEST-1")
            .unwrap()
            .unwrap();
        assert_eq!(
            second_refusal.previous_run_id.as_deref(),
            Some("first-refused")
        );
        retry.run_id = "accepted-retry".into();
        let accepted = crate::run_control::request_in(registry, retry, || Ok(())).unwrap();
        assert_eq!(accepted.previous_run_id.as_deref(), Some("first-retry"));
        assert_eq!(accepted.worktree_path, initial.worktree_path);
        assert_eq!(accepted.branch, initial.branch);
        assert_eq!(accepted.state, crate::run_control::RunState::Starting);
        assert!(crate::run_control::continuation_in(registry, "TEST-1")
            .unwrap()
            .is_none());
        assert!(
            crate::run_control::load_manifest_in(registry, "first-refused")
                .unwrap()
                .admission_refused
        );
    }

    #[test]
    fn first_refusal_recovery_never_accepts_admitted_or_uncertain_workers() {
        let mut run = crate::run_control::tests::run();
        run.state = crate::run_control::RunState::Failed;
        run.admission_refused = true;
        // A failed flag with a session is contradictory and cannot anchor retry.
        assert!(!crate::run_control::initial_admission_refused(&run));
        run.pty_session = None;
        assert!(crate::run_control::initial_admission_refused(&run));
        run.admission_refused = false;
        assert!(!crate::run_control::initial_admission_refused(&run));
        run.admission_refused = true;
        run.pid = Some(999);
        assert!(!crate::run_control::initial_admission_refused(&run));
        run.pid = None;
        run.state = crate::run_control::RunState::Running;
        assert!(!crate::run_control::initial_admission_refused(&run));
    }

    #[test]
    fn refused_successor_releases_its_pending_slot_but_uncertain_failure_does_not() {
        let temp = Temp::new();
        let registry = temp.path();
        let mut successor = crate::run_control::tests::run();
        successor.run_id = "successor".into();
        successor.previous_run_id = Some("predecessor".into());
        successor.state = crate::run_control::RunState::Requested;
        successor.revision = 3;
        let receipt = launch_receipt_path(
            registry,
            Path::new("/repo"),
            "XNAUT-900",
            Some(&successor.run_id),
        )
        .unwrap();
        let pending =
            json!({"pending":true,"ticket":"XNAUT-900","continuation_run_id":successor.run_id});
        reserve(&receipt, &pending).unwrap();
        let persist = |run: &crate::run_control::RunManifest| {
            std::fs::write(
                registry.join(format!("{}.run.json", run.run_id)),
                serde_json::to_vec(run).unwrap(),
            )
            .unwrap();
        };
        persist(&successor);
        assert!(!release_refused_continuation(registry, &receipt, &successor).unwrap());
        let mut refused = successor.clone();
        refused.state = crate::run_control::RunState::Failed;
        refused.admission_refused = true;
        refused.revision += 1;
        persist(&refused);
        assert!(release_refused_continuation(registry, &receipt, &successor).unwrap());
        assert!(!receipt.exists());
        let archived = receipt.with_file_name(format!("{}-refused-{}.json", receipt.file_stem().unwrap().to_str().unwrap(), refused.revision));
        let evidence: Value = serde_json::from_slice(&std::fs::read(archived).unwrap()).unwrap();
        assert_eq!(evidence["launch"]["run_id"], refused.run_id);
        assert_eq!(evidence["execution_started"], false);
        // Native continuation remains available and the same ticket can reserve
        // again; its old failed manifest has not been deleted or rewritten.
        assert_eq!(
            crate::run_control::continuation_in(registry, "XNAUT-900")
                .unwrap()
                .unwrap(),
            refused
        );
        assert!(reserve(&receipt, &pending).unwrap().is_none());
        assert!(!release_refused_continuation(registry, &receipt, &refused).unwrap());
        let reserved_retry = refused.clone();
        let mut retry = refused.clone();
        retry.run_id = "retry-successor".into();
        retry.previous_run_id = Some(refused.run_id.clone());
        retry.revision = 1;
        refused.next_run_id = Some(retry.run_id.clone());
        persist(&refused);
        persist(&retry);
        assert!(release_refused_continuation(registry, &receipt, &reserved_retry).unwrap());
        reserve(&receipt, &pending).unwrap();
        retry.admission_refused = false;
        persist(&retry);
        assert!(release_refused_continuation(registry, &receipt, &reserved_retry).is_err());
        assert!(receipt.exists());
    }

    #[test]
    fn finalized_same_id_refusal_archive_replays_after_restart() {
        let temp = Temp::new(); let registry = temp.path(); let root = Path::new("/repo");
        let mut run = crate::run_control::tests::run();
        run.run_id = "refused-continuation".into(); run.previous_run_id = Some("predecessor".into());
        run.state = crate::run_control::RunState::Failed; run.admission_refused = true; run.revision = 4;
        run.pty_session = None; run.remote_env = Some("exe-dev".into());
        let manifest = registry.join(format!("{}.run.json",run.run_id));
        std::fs::write(&manifest, serde_json::to_vec(&run).unwrap()).unwrap();
        let ticket = run.ticket.as_deref().unwrap();
        let path = launch_receipt_path(registry, root, ticket, Some(&run.run_id)).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let receipt = json!({"pending":false,"ok":false,"execution_started":false,"admission_refused":true,
            "project":run.project,"ticket":ticket,"repository_root":root,"handle":run.agent_handle,
            "branch":run.branch,"worktree_path":run.worktree_path,"environment":"exe-dev",
            "continuation_run_id":run.run_id,"launch":{"run_id":run.run_id},"refused_run_revision":run.revision,
            "prelaunch_failure":run.prelaunch_failure,"requested_at":10});
        // Simulate the saved final canonical receipt with its rename interrupted.
        for field in ["refused_run_revision", "launch", "handle", "execution_started"] {
            let mut bad = receipt.clone(); bad[field] = json!("wrong"); save_launch_receipt(&path, &bad).unwrap();
            assert!(!reconcile_refusal_archive_in(registry, root, &run.project, ticket).unwrap(), "{field}");
            assert!(path.exists());
        }
        save_launch_receipt(&path, &receipt).unwrap();
        assert!(reconcile_prelaunch_in(registry, root, &run.project, ticket).unwrap());
        assert!(!path.exists());
        let archived = path.with_file_name(format!("{}-refused-{}.json",path.file_stem().unwrap().to_str().unwrap(),run.revision));
        assert_eq!(read_launch_receipt(&archived).unwrap().unwrap(), receipt);
        assert!(!reconcile_prelaunch_in(registry, root, &run.project, ticket).unwrap());
        // Existing matching archive is idempotent; conflicting evidence is kept.
        save_launch_receipt(&path, &receipt).unwrap();
        assert!(reconcile_refusal_archive_in(registry, root, &run.project, ticket).unwrap());
        save_launch_receipt(&path, &receipt).unwrap();
        save_launch_receipt(&archived, &json!({"different":"evidence"})).unwrap();
        assert!(reconcile_refusal_archive_in(registry, root, &run.project, ticket).is_err());
        assert!(path.exists());
        save_launch_receipt(&archived, &receipt).unwrap();
        let mut live = run.clone(); live.state = crate::run_control::RunState::Running;
        std::fs::write(&manifest, serde_json::to_vec(&live).unwrap()).unwrap();
        assert!(!reconcile_refusal_archive_in(registry, root, &run.project, ticket).unwrap());
        assert!(path.exists());
        std::fs::write(&manifest, serde_json::to_vec(&run).unwrap()).unwrap();
        assert!(reconcile_refusal_archive_in(registry, root, &run.project, ticket).unwrap());
        assert!(reserve(&path, &json!({"pending":true,"continuation_run_id":run.run_id})).unwrap().is_none());
        assert!(!reconcile_refusal_archive_in(registry, root, &run.project, ticket).unwrap());
        assert!(path.exists(), "a newly reserved attempt must stay pending");
    }

    #[test]
    fn only_pre_admission_failure_releases_reservation() {
        let temp = Temp::new();
        let path = temp.path().join("reservation.json");
        reserve(&path, &json!({"pending":true})).unwrap();
        drop(LaunchReservation::new(path.clone()));
        assert!(!path.exists());
        reserve(&path, &json!({"pending":true})).unwrap();
        let mut guard = LaunchReservation::new(path.clone());
        guard.attempted();
        drop(guard);
        assert!(path.exists());
    }

    #[test]
    fn explicit_exe_placement_overrides_local_profile_without_http_credentials() {
        use crate::sandbox::launch_env::LaunchEnv;
        let configured = [crate::settings::SandboxProviderSettings {
            kind: "exe-dev".into(),
            base_url: String::new(),
            api_key: None,
        }];
        assert_eq!(
            task_environment(Some(&json!("exe-dev")), Some(LaunchEnv::Local), &configured).unwrap(),
            LaunchEnv::ExeDev
        );
        assert_eq!(
            task_environment(None, Some(LaunchEnv::Local), &configured).unwrap(),
            LaunchEnv::Local
        );
        assert_eq!(
            task_environment(None, None, &configured).unwrap(),
            LaunchEnv::ExeDev
        );
        assert!(
            task_environment(Some(&json!("exe-dev")), Some(LaunchEnv::Local), &[])
                .unwrap_err()
                .contains("refused")
        );
        for invalid in [json!("exe.dev"), json!(""), Value::Null, json!(false)] {
            assert!(task_environment(Some(&invalid), Some(LaunchEnv::Local), &configured).is_err());
        }
        for spec in specs()
            .into_iter()
            .filter(|s| s["function"]["name"] != "prepare_repository_ticket")
        {
            assert_eq!(
                spec["function"]["parameters"]["properties"]["environment"]["enum"],
                json!(["local", "exe-dev", "gitvm"])
            );
        }
    }

    #[test]
    fn a_local_receipt_cannot_be_reported_as_a_requested_remote_launch() {
        let prior = json!({"environment":"local","launch":{"run_id":"existing"}});
        assert!(verify_receipt_environment(&prior, Some(&json!("exe-dev")))
            .unwrap_err()
            .contains("No second worker"));
        assert!(verify_receipt_environment(&prior, Some(&json!("local"))).is_ok());
        assert!(verify_receipt_environment(&prior, None).is_ok());
        assert!(verify_receipt_environment(&json!({}), Some(&json!("exe-dev"))).is_err());
    }

    #[test]
    fn a_real_ticket_must_match_the_repository_before_work_can_start() {
        let temp = Temp::new();
        let project = temp.path().join("projects/HISTFIX");
        std::fs::create_dir_all(project.join("tickets")).unwrap();
        let source = temp.path().join("source");
        let other = temp.path().join("other");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&other).unwrap();
        let source = source.canonicalize().unwrap();
        std::fs::write(project.join("project.json"),json!({"key":"HISTFIX","name":"History fixture","source_path":source,"created_at":"fixture"}).to_string()).unwrap();
        std::fs::write(project.join("tickets/HISTFIX-1.json"),json!({"id":"HISTFIX-1","project":"HISTFIX","title":"DJ sequencing","type":"task","status":"ready","priority":"high","body":"Preserve the agreed constraints","revision":1,"created_at":"fixture","updated_at":"fixture"}).to_string()).unwrap();
        assert!(ticket_context_in(temp.path(), "HISTFIX-1", &source)
            .unwrap()
            .contains("Preserve the agreed constraints"));
        assert!(
            ticket_context_in(temp.path(), "HISTFIX-1", &other.canonicalize().unwrap())
                .unwrap_err()
                .contains("same project")
        );
        assert!(ticket_context_in(temp.path(), "NONEXISTENT-1", &source)
            .unwrap_err()
            .contains("not found"));
    }
    #[tokio::test]
    async fn ticketless_launch_and_worktree_are_refused_before_any_side_effect() {
        let temp = Temp::new();
        for name in ["create_worktree", "start_repository_task"] {
            for ticket in [Value::Null, json!(""), json!("   ")] {
                let result = execute(
                    name,
                    &json!({"root":temp.path(),"ticket":ticket,"task_key":"retry","task":"build"}),
                    "nautbot",
                    &[],
                    "",
                    None,
                )
                .await;
                assert_eq!(result["ok"], false);
                assert!(result["error"].as_str().unwrap().contains("ticket"));
                assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
            }
        }
    }
    #[test]
    fn chat_work_requires_current_approved_finding_before_worktree_or_reservation() {
        let temp = Temp::new();
        let project = temp.path().join("projects/APP");
        std::fs::create_dir_all(project.join("tickets")).unwrap();
        let source = temp.path().join("source"); std::fs::create_dir(&source).unwrap();
        let source = source.canonicalize().unwrap();
        std::fs::write(project.join("project.json"),json!({"key":"APP","name":"Fixture","source_path":source,"created_at":"fixture"}).to_string()).unwrap();
        let mut ticket: crate::project_management::TicketRecord = serde_json::from_value(json!({
            "id":"APP-1","project":"APP","title":"Finding in calc","type":"bug","status":"ready","priority":"high",
            "source_id":"forgejo:team/app#7","body":"Inspect the imported report","revision":1,"created_at":"fixture","updated_at":"fixture"
        })).unwrap();
        let path = project.join("tickets/APP-1.json");
        let save = |t: &crate::project_management::TicketRecord| std::fs::write(&path,serde_json::to_vec(t).unwrap()).unwrap();
        save(&ticket);
        let check = |records: &[crate::ticket_triage::TriageRecord]| ticket_context_admitted_in(temp.path(),"APP-1",&source,
            |t| crate::ticket_triage::admission_from_records(t,records));
        assert!(check(&[]).unwrap_err().contains("no recorded triage disposition"));
        let evidence = source.join("calc.py"); std::fs::write(&evidence,"return 0\n").unwrap();
        let scope_hash = format!("{:x}",Sha256::digest(serde_json::to_vec(&(
            &ticket.id,&ticket.project,&ticket.source_id,&ticket.title,&ticket.body)).unwrap()));
        let approved: crate::ticket_triage::TriageRecord = serde_json::from_value(json!({
            "fingerprint":"fixture","source_id":ticket.source_id,
            "binding":{"ticket":ticket.id,"project":ticket.project,"source_id":ticket.source_id,"scope_hash":scope_hash},
            "run_id":"triage-one","forge_index":0,"forge_kind":"forgejo","owner":"team","repo":"app","issue_number":7,
            "issue_url":"https://forge/issues/7","project":"APP","provider":"lmstudio","model":"local","classification":"confirmed","confidence":0.9,
            "status":"approved","comment_url":"","created_at":"2026-10-06T10:00:00Z","updated_at":"2026-10-06T10:00:00Z",
            "decision":{"actor":"owner","approved":true,"at":"2026-10-06T10:00:00Z","comment":"Approved this finding"},
            "analysis":{"classification":"confirmed","confidence":0.9,"severity":"high","affected_components":["calc"],"likely_cause":"Recorded defect",
                "evidence":[{"source":"ticket","reference":"https://forge/issues/7","summary":"Imported finding"}],"questions":[],"recommended_next_step":"Fix the approved scope"},
            "evidence_files":[{"path":evidence,"digest":format!("{:x}",Sha256::digest(b"return 0\n"))}]
        })).unwrap();
        assert!(check(std::slice::from_ref(&approved)).unwrap().contains("Inspect the imported report"));
        let mut unapproved = approved.clone(); unapproved.status = "waiting_for_approval".into(); unapproved.decision = None;
        assert!(check(&[unapproved]).unwrap_err().contains("not actionable"));
        let mut duplicate = approved.clone(); duplicate.classification = crate::ticket_triage::TriageClassification::Duplicate;
        assert!(check(&[duplicate]).unwrap_err().contains("not actionable"));
        let body = ticket.body.clone(); ticket.body.push_str("\n\n## Dispatched fixture\nNew unapproved instructions"); save(&ticket);
        assert!(check(std::slice::from_ref(&approved)).unwrap_err().contains("scope changed"));
        ticket.body = body; save(&ticket);
        std::fs::write(&evidence,"return 1\n").unwrap();
        assert!(check(std::slice::from_ref(&approved)).unwrap_err().contains("evidence changed"));
        std::fs::write(&evidence,"return 0\n").unwrap();
        // A context accepted before asynchronous staging is not permission to
        // launch after the finding/evidence changes. The internal reservation
        // is read again at shared native worker admission.
        let registry = temp.path().join("native-admission");
        let mut run = crate::run_control::tests::run(); run.project = ticket.project.clone();
        run.ticket = Some(ticket.id.clone()); run.worktree_path = source.join("worker").to_string_lossy().into();
        run.branch = "model/finding".into();
        let receipt_path = launch_receipt_path(&registry,&source,&ticket.id,None).unwrap();
        std::fs::create_dir_all(receipt_path.parent().unwrap()).unwrap();
        let mut receipt = json!({"pending":true,"requires_findings_triage":true,"ticket":ticket.id,
            "project":ticket.project,"repository_root":source,"worktree_path":run.worktree_path,"branch":run.branch});
        std::fs::write(&receipt_path,receipt.to_string()).unwrap();
        pin_model_reservation_in(&registry,&mut run).unwrap();
        assert!(admit_model_reservation_at(&registry,&run,&source,||check(std::slice::from_ref(&approved)).map(|_|())).is_ok());
        std::fs::write(&receipt_path,"{").unwrap();
        assert!(admit_model_reservation_at(&registry,&run,&source,||panic!("corrupt model receipt cannot become owner permission")).unwrap_err().contains("disappeared"));
        std::fs::remove_file(&receipt_path).unwrap();
        assert!(admit_model_reservation_at(&registry,&run,&source,||panic!("missing model receipt cannot become owner permission")).unwrap_err().contains("disappeared"));
        std::fs::write(&receipt_path,receipt.to_string()).unwrap();
        let moved_root = source.join("different-project-root");
        assert!(admit_model_reservation_at(&registry,&run,&moved_root,||panic!("changed project root must hold before disposition")).unwrap_err().contains("repository changed"));
        assert_eq!(model_reservation_root_in(&registry,&run).unwrap(),Some(source.clone()),"reservation remains recoverable without current project metadata");
        ticket.body.push_str("new scope during staging"); save(&ticket);
        assert!(admit_model_reservation_at(&registry,&run,&source,||check(std::slice::from_ref(&approved)).map(|_|())).unwrap_err().contains("scope changed"));
        // Failed-admission retry: the chat slot is keyed to the refused
        // predecessor while bind_pending_in selects a new native child ID.
        std::fs::remove_file(&receipt_path).unwrap();
        run.previous_run_id = Some("refused-predecessor".into());
        let retry_path = launch_receipt_path(&registry,&source,&ticket.id,run.previous_run_id.as_deref()).unwrap();
        receipt["continuation_run_id"] = json!("refused-predecessor");
        std::fs::write(&retry_path,receipt.to_string()).unwrap();
        assert!(admit_model_reservation_at(&registry,&run,&source,||check(std::slice::from_ref(&approved)).map(|_|())).unwrap_err().contains("scope changed"));
        let receipt_path = retry_path;
        receipt["requires_findings_triage"] = json!(false);
        std::fs::write(&receipt_path,receipt.to_string()).unwrap();
        assert!(admit_model_reservation_at(&registry,&run,&source,||Ok(())).is_err(),"stored native origin cannot be downgraded through a receipt");
        run.findings_reservation_root = None;
        assert!(admit_model_reservation_at(&registry,&run,&source,||panic!("actual owner dispatch retains its separate authorization")).is_ok());
        receipt["requires_findings_triage"] = json!(true); receipt["branch"] = json!("foreign");
        std::fs::write(&receipt_path,receipt.to_string()).unwrap();
        assert!(admit_model_reservation_at(&registry,&run,&source,||Ok(())).unwrap_err().contains("identity changed"));
        ticket.source_id.clear(); save(&ticket);
        // Ordinary owner work retains the production path's early nonfinding
        // admission without loading the user's triage store.
        assert!(ticket_context_in(temp.path(),"APP-1",&source).is_ok());
        assert!(!source.join(".worktrees").exists());
        assert!(!temp.path().join("registry").exists());
    }

    #[test]
    fn gitvm_staged_findings_refusal_retains_admitted_identity_without_launching() {
        let temp = Temp::new();
        let registry = temp.path().join("registry");
        let mut run = crate::run_control::tests::run();
        run.state = crate::run_control::RunState::Requested;
        run.pty_session = None;
        run.remote_env = Some("gitvm".into());
        run.findings_reservation_root = Some("/original/project".into());
        let admitted = crate::run_control::request_in(&registry,run,||Ok(())).unwrap();
        assert_eq!(admitted.state,crate::run_control::RunState::Starting);
        let error = admit_staged_model_run_with(&registry,&admitted.run_id,|current| {
            assert_eq!(current.findings_reservation_root,admitted.findings_reservation_root);
            Err("finding evidence changed during warm-up".into())
        }).unwrap_err();
        assert!(error.contains("evidence changed"));
        let refused = crate::run_control::load_manifest_in(&registry,&admitted.run_id).unwrap();
        assert!(crate::run_control::prelaunch_refused(&refused));
        assert_eq!(refused.run_id,admitted.run_id);
        assert!(refused.revision > admitted.revision);
        assert!(refused.last_signal.contains("No agent was started"));
        assert_eq!(crate::run_control::list_ids_in(&registry).unwrap(),vec![admitted.run_id]);
        // A simultaneous actual worker observation is retained, never relabelled
        // as a proven pre-execution refusal by the final model guard.
        crate::run_control::update_in(&registry,&refused.run_id,|r| {
            r.state = crate::run_control::RunState::Running;
            r.pty_session = Some("worker-view".into()); r.admission_refused = false;
            r.prelaunch_failure = None;
        }).unwrap();
        assert!(admit_staged_model_run_with(&registry,&refused.run_id,|_|Err("stale finding".into())).is_err());
        let running = crate::run_control::load_manifest_in(&registry,&refused.run_id).unwrap();
        assert_eq!(running.state,crate::run_control::RunState::Running);
        assert_eq!(running.pty_session.as_deref(),Some("worker-view"));
    }
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("xnaut-agent-work-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn workspaces_are_agent_specific_and_retries_are_stable() {
        assert_eq!(
            workspace_key("cortana", "JOBUP-11"),
            workspace_key("cortana", "JOBUP-11")
        );
        assert_ne!(
            workspace_key("cortana", "JOBUP-11"),
            workspace_key("claudi", "JOBUP-11")
        );
        assert_ne!(
            workspace_key("cortana", "JOBUP-11"),
            workspace_key("cortana", "JOBUP-12")
        );
        assert!(!workspace_key("cortana", "../../escape").contains('/'));
    }
    #[tokio::test]
    async fn two_agents_can_create_their_own_worktrees_for_the_same_ticket() {
        let temp = Temp::new();
        let repo = temp.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]).unwrap();
        git(
            &repo,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "baseline",
            ],
        )
        .unwrap();
        let leases = std::env::temp_dir()
            .join("xnaut-lease-tests")
            .join("leases");
        std::fs::create_dir_all(&leases).unwrap();
        std::env::set_var("XNAUT_LEASE_DIR", &leases);
        let mut paths = Vec::new();
        for handle in ["cortana", "claudi"] {
            let key = workspace_key(handle, "JOBUP-11");
            let path = crate::agent_profiles::agent_build_workspace(
                handle.into(),
                repo.to_string_lossy().into_owned(),
                key.clone(),
            )
            .await
            .unwrap();
            verify_workspace(&repo, Path::new(&path), &format!("agent/{handle}/{key}")).unwrap();
            let again = crate::agent_profiles::agent_build_workspace(
                handle.into(),
                repo.to_string_lossy().into_owned(),
                key,
            )
            .await
            .unwrap();
            assert_eq!(again, path);
            paths.push(path);
        }
        assert_ne!(paths[0], paths[1]);
        std::fs::write(Path::new(&paths[0]).join("findings.md"), "evidence").unwrap();
        assert!(!repo.join("findings.md").exists());
        assert!(!Path::new(&paths[1]).join("findings.md").exists());
        for path in paths {
            crate::writer_lease::release(Path::new(&path));
        }
    }
    #[test]
    fn duplicate_and_uncertain_launches_are_not_restarted() {
        let dir = Temp::new();
        let path = dir.path().join("receipt.json");
        assert!(reserve(&path, &json!({"pending":true})).unwrap().is_none());
        assert!(reserve(&path, &json!({"pending":true}))
            .unwrap_err()
            .contains("pending"));
        std::fs::write(
            &path,
            br#"{"ok":true,"launch":{"session_id":"actual-session"}}"#,
        )
        .unwrap();
        assert_eq!(
            reserve(&path, &json!({"pending":true})).unwrap().unwrap()["launch"]["session_id"],
            "actual-session"
        );
    }
    #[test]
    fn worktree_identity_and_authorization_require_real_git_evidence() {
        let dir = Temp::new();
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]).unwrap();
        git(
            &repo,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "baseline",
            ],
        )
        .unwrap();
        let root = repo.canonicalize().unwrap();
        assert!(authorize_root(repo.to_str().unwrap(), &[]).is_err());
        assert_eq!(
            authorize_root(repo.to_str().unwrap(), &[root.clone()]).unwrap(),
            root
        );
        let dest = repo.join(".worktrees/test");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                "agent/cortana/test",
                dest.to_str().unwrap(),
            ],
        )
        .unwrap();
        verify_workspace(&repo, &dest, "agent/cortana/test").unwrap();
        assert!(verify_workspace(&repo, &dest, "agent/claudi/test").is_err());
        let empty = repo.join(".worktrees/empty");
        std::fs::create_dir(&empty).unwrap();
        assert!(verify_workspace(&repo, &empty, "agent/cortana/test").is_err());
        std::fs::write(dest.join("isolated.txt"), "worker").unwrap();
        assert!(!repo.join("isolated.txt").exists());
    }
}
