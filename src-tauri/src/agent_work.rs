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
    let out = std::process::Command::new("git")
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
            let app = crate::nudge::app().ok_or("The app is not running")?;
            let state = tauri::Manager::state::<crate::state::AppState>(app);
            let request = serde_json::from_value(
                json!({"key":project_key,"name":name,"source_repo":root,"forge_remote":remote}),
            )
            .map_err(|e| e.to_string())?;
            crate::project_management::pm_project_create(state, request).await?
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
            json!({"ok":true,"ticket":ticket.id,"project":project.key,"status":ticket.status,"reused":true,"execution_started":false,"recovered_project":recovered}),
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
    let ticket = Some(required_ticket(args["ticket"].as_str())?);
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
    let receipt_path = launch_receipt_path(&registry, &root, ticket.unwrap(), None)?;
    // Return an existing durable result before checking mutable runtime state.
    // A new task key/agent/thread cannot turn this receipt into a fresh launch.
    if name == "start_repository_task" {
        if let Some(mut prior) = read_launch_receipt(&receipt_path)? {
            verify_receipt_environment(&prior, args.get("environment"))?;
            prior["reused_receipt"] = json!(true);
            prior["note"] = json!("Recovered this ticket's previous launch; no second worker started. Inspect its branch/PR and current registry before deciding how to continue.");
            return Ok(prior);
        }
    }
    let recovered = json!(crate::project_continuity::snapshot(&project)?);
    recovery_guard(&recovered, ticket.unwrap(), None)?;
    let slug = workspace_key(&profile.handle, key);
    let parent = root.join(".worktrees");
    if parent
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("Refusing a symlink as the worktree container.".into());
    }
    let dest = parent.join(&slug);
    let branch = format!("agent/{}/{slug}", profile.handle);
    if dest.exists() {
        verify_workspace(&root, &dest, &branch)?;
    }
    if name == "start_repository_task" {
        let pending =
            json!({"pending":true,"ticket":ticket,"project":project,"repository_root":root});
        if let Some(prior) = reserve(&receipt_path, &pending)? {
            verify_receipt_environment(&prior, args.get("environment"))?;
            return Ok(
                json!({"ok":true,"reused_receipt":true,"receipt":prior,"execution_started":false}),
            );
        }
    }
    let prompt_recovery = crate::agent_history::compact_project(&recovered, ticket.unwrap());
    let mut launch_attempted = false;
    let mut result=async {
        let worktree=crate::agent_profiles::agent_build_workspace(profile.handle.clone(),root.to_string_lossy().into_owned(),slug).await?;
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
                return Err(format!("{error}. Launch admission was attempted; recovery reservation retained at {}. Reconcile the run before retrying.", receipt_path.display()));
            }
        }
        if let Ok(receipt) = &mut result {
            // A launched worker remains a launch even if the subsequent board
            // write fails. Return the receipt and a precise tracking warning;
            // never invite a second worker as recovery from a PM write error.
            if let Err(error) = track_launch(ticket.unwrap(), &profile.handle, receipt) {
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
        let mut ticket = json!({"id":"TEST-1","project":"TEST","title":"Existing assignment","status":"ready",
            "revision":1,"created_at":"fixture","updated_at":"fixture","body":""});
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
