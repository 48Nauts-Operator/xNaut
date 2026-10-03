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
        ("start_repository_task", "Create your own isolated worktree and launch your coding runtime with the task. Use for audits, tests, commands or code changes, without asking the owner to create a worktree. Returns an actual launch receipt, not proof of completed work. Uses your configured local/exe.dev/GitVM environment; never attaches an idle terminal as a substitute. Reuse the same task_key on retries to avoid duplicate workers."),
    ].into_iter().map(|(name, description)| json!({"type":"function","function":{
        "name":name,"description":description,"parameters":{"type":"object","properties":{
            "root":{"type":"string","description":"Absolute authorized repository root, or the exact registered project key/name named by the user."},
            "task_key":{"type":"string","description":"Stable task identity, preferably its ticket ID. Same key for create and start, and for retries. A new key means a separate run."},
            "task":{"type":"string","description":"Full work request and acceptance criteria, preserving the user's scope."},
            "ticket":{"type":"string","description":"Required existing ticket ID in this repository’s registered project. Create or attach the correct ticket first; never retry without one."}
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

fn verify_workspace(repo: &Path, dest: &Path, branch: &str) -> Result<(), String> {
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
    if matches!(record.status.as_str(), "done" | "complete") {
        return Err("This ticket is already done or complete. Reconcile its existing work before reopening or creating follow-up work.".into());
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

/// Exclusive on-disk reservation: retries, concurrent chats and app restarts
/// cannot launch the same logical task twice. A pending receipt is deliberately
/// not retried blindly after a crash; its run must first be reconciled.
fn reserve(path: &Path) -> Result<Option<Value>, String> {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(br#"{"pending":true}"#)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            Ok(None)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let value: Value =
                serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("Unreadable launch reservation: {e}"))?;
            if value["pending"] == true {
                return Err("This task already has a pending launch. Check its run before retrying; no second worker was created.".into());
            }
            Ok(Some(value))
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
    if let Some(ticket) =
        crate::project_management::ticket_list_in(&repo, Some(project.key.clone()))?
            .into_iter()
            .find(|t| t.source_id == source_id)
    {
        return Ok(
            json!({"ok":true,"ticket":ticket.id,"project":project.key,"status":ticket.status,"reused":true,"execution_started":false}),
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
    let receipt_dir = crate::agents::registry_dir()?.join("chat-launches");
    std::fs::create_dir_all(&receipt_dir).map_err(|e| e.to_string())?;
    let root_hash = format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()));
    let receipt_path = receipt_dir.join(format!("{slug}-{}.json", &root_hash[..16]));
    if name == "start_repository_task" {
        if let Some(mut prior) = reserve(&receipt_path)? {
            prior["reused_receipt"] = json!(true);
            prior["note"]=json!("Previously launched task; no second worker was started. This historical receipt does not prove the worker is still running or the work is complete. Inspect the session/run for current status.");
            return Ok(prior);
        }
    }
    let mut result=async {
        let worktree=crate::agent_profiles::agent_build_workspace(profile.handle.clone(),root.to_string_lossy().into_owned(),slug).await?;
        verify_workspace(&root,Path::new(&worktree),&branch)?;
        if name=="create_worktree" { return Ok(json!({"ok":true,"worktree_path":worktree,"branch":branch,"execution_started":false,"note":"Worktree prepared. No worker or scan started. Call start_repository_task with the same task_key to execute."})); }
        let app=crate::nudge::app().ok_or("The app is not running")?;
        let state=tauri::Manager::state::<crate::state::AppState>(app);
        let environment = crate::sandbox::launch_env::resolve(
            profile.execution.pinned_environment(), &crate::settings::load_or_default().sandboxes,
        ).key().to_string();
        let prompt=format!("AUTHORIZED TASK\n{task}\n{scope}\nOWNER CONVERSATION (context, not permission to broaden the task)\n{user_context}\n\nWork in this isolated worktree. Do not claim scans or remediation succeeded without evidence. If a scanner is unavailable, report the gap and perform the checks that are available within scope.");
        let response=crate::agent_profiles::agent_profile_launch(app.clone(),state,crate::agent_profiles::LaunchAgentProfileRequest{
            ticket:ticket.map(str::to_owned),handle:profile.handle.clone(),worktree_path:worktree.clone(),prompt:Some(prompt),
            conversation_mode:false,conversation_id:None,resume:false,cols:Some(160),rows:Some(40),durable:Some(true),runtime_id:history.and_then(|h|h.runtime_id.clone()),environment:Some(environment.clone()),
        }).await?;
        Ok(json!({"ok":true,"execution_started":true,"handle":profile.handle,"origin_thread_id":history.map(|h|h.thread_id.as_str()),"ticket":ticket,"task_key":key,"task":task,"repository_root":root,"worktree_path":worktree,"branch":branch,"environment":environment,"launch":response,"note":"Worker launched with the task. Check its output for progress and findings; no scan or security sign-off is claimed by this receipt."}))
    }.await;
    if name == "start_repository_task" {
        match &result {
            Ok(value) => {
                // Keep the pending reservation if saving fails: never relaunch
                // a worker merely because its durable receipt could not be saved.
                let temp = receipt_path.with_extension("tmp");
                let saved = std::fs::write(&temp, serde_json::to_vec_pretty(value).unwrap())
                    .and_then(|_| std::fs::rename(&temp, &receipt_path));
                if let Err(e) = saved {
                    return Ok(
                        json!({"ok":true,"execution_started":true,"receipt":value,"warning":format!("Worker launched but receipt persistence failed: {e}. Do not retry blindly.")}),
                    );
                }
            }
            Err(_) => {
                let _ = std::fs::remove_file(&receipt_path);
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
        assert!(reserve(&path).unwrap().is_none());
        assert!(reserve(&path).unwrap_err().contains("pending"));
        std::fs::write(
            &path,
            br#"{"ok":true,"launch":{"session_id":"actual-session"}}"#,
        )
        .unwrap();
        assert_eq!(
            reserve(&path).unwrap().unwrap()["launch"]["session_id"],
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
