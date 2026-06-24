// Lifecycle actions for Tasks Mode (v1.6): scaffold projects (folder + git +
// forge repo + agent in Zellij), lightweight tasks, task→project promotion, and tasks from forge issues.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::settings::{ForgeHost, ProjectCategory, Settings};
use crate::tasks::TaskSession;

/// Everything the frontend needs to open a new xNaut PTY pane for a task.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LaunchSpec {
    pub task: crate::tasks::TaskSession,
    /// Shell command the frontend runs in a new xNaut PTY pane (zellij attach/create).
    pub command: String,
    pub cwd: String,
}

// ─── Pure helpers ────────────────────────────────────────────────────────────

/// Sanitizes a user-supplied name into a safe folder name: keeps alphanumerics
/// plus `-`, `_`, `.`; everything else collapses to a single `-`.
fn sanitize_name(raw: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
            out.push(c);
            last_dash = c == '-';
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Builds a branch slug from an issue title: first 5 words, lowercased,
/// alphanumerics only, joined with `-`.
fn slug_from_title(title: &str) -> String {
    title
        .split_whitespace()
        .take(5)
        .map(|w| {
            w.chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Baseline-prompt context file name per agent: Claude reads CLAUDE.md,
/// everything else gets the AGENTS.md convention.
fn context_file_name(agent_id: &str) -> &'static str {
    if agent_id == "claude" {
        "CLAUDE.md"
    } else {
        "AGENTS.md"
    }
}

/// POSIX single-quote escaping: `it's` → `'it'\''s'`.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Assembles `ENV1='v1' ENV2='v2' <cmd> <args...>` as a single shell string.
/// Env keys are sorted for deterministic output.
fn build_shell_command(env: &HashMap<String, String>, cmd: &str, args: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut keys: Vec<&String> = env.keys().collect();
    keys.sort();
    for k in keys {
        parts.push(format!("{k}={}", shell_quote(&env[k])));
    }
    parts.push(cmd.to_string());
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

/// Context file body for issue-driven tasks: instruction header + issue facts + full body.
fn issue_context_markdown(
    number: u64,
    title: &str,
    html_url: &str,
    labels: &[String],
    body: &str,
    branch: &str,
) -> String {
    let labels_line = if labels.is_empty() {
        "(none)".to_string()
    } else {
        labels.join(", ")
    };
    format!(
        "You are working on the following issue. Branch: {branch}.\n\n\
         # Issue #{number}: {title}\n\n\
         - URL: {html_url}\n\
         - Labels: {labels_line}\n\n\
         ## Description\n\n{body}\n"
    )
}

// ─── Private plumbing ────────────────────────────────────────────────────────

/// Same recipe as worktree.rs — run git with `-C <repo>` and surface stderr on failure.
fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("failed to invoke git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {}", args.join(" "), stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// git init + add -A + commit in `path`. Files must already be written.
fn git_init_commit(path: &Path, message: &str) -> Result<(), String> {
    run_git(path, &["init"]).map_err(|e| format!("git init failed: {e}"))?;
    run_git(path, &["add", "-A"]).map_err(|e| format!("git add failed: {e}"))?;
    run_git(path, &["commit", "-m", message]).map_err(|e| format!("git commit failed: {e}"))?;
    Ok(())
}

/// Adds `origin` and pushes. The push is non-fatal by design — the repo exists
/// locally and on the forge; the user can push later (e.g. forge offline over Tailscale).
fn add_remote_and_push(path: &Path, clone_url: &str) -> Result<(), String> {
    run_git(path, &["remote", "add", "origin", clone_url])
        .map_err(|e| format!("git remote add failed: {e}"))?;
    if let Err(e) = run_git(path, &["push", "-u", "origin", "HEAD"]) {
        eprintln!("[scaffold] push to origin failed (repo exists locally — push later): {e}");
    }
    Ok(())
}

fn resolve_category(settings: &Settings, label: &str) -> Result<ProjectCategory, String> {
    settings
        .categories
        .iter()
        .find(|c| c.label == label)
        .cloned()
        .ok_or_else(|| {
            let valid: Vec<&str> = settings
                .categories
                .iter()
                .map(|c| c.label.as_str())
                .collect();
            format!(
                "unknown category label '{label}' — valid labels: {}",
                valid.join(", ")
            )
        })
}

fn resolve_forge(settings: &Settings, index: usize) -> Result<ForgeHost, String> {
    settings.forges.get(index).cloned().ok_or_else(|| {
        format!(
            "forge_index {index} out of range ({} forge(s) configured)",
            settings.forges.len()
        )
    })
}

/// Builds the single shell command for an agent: registry env vars + launch_cmd
/// + extra_args. The pseudo-agent "shell" (not in the registry) returns the
/// user's shell instead.
fn agent_shell_command(agent_id: &str, _prompt_file_hint: Option<&str>) -> Result<String, String> {
    if agent_id == "shell" {
        return Ok(std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into()));
    }
    let registry = crate::agents::load_or_seed_registry()?;
    let cfg = registry.find(agent_id).ok_or_else(|| {
        format!("unknown agent id: {agent_id} (edit ~/.config/xnaut/agents.toml)")
    })?;
    Ok(build_shell_command(
        &cfg.env,
        &cfg.launch_cmd,
        &cfg.extra_args,
    ))
}

/// If the chosen agent routes through NautGate (a localhost:8090 URL in its
/// env), verify the gateway is up before launching — otherwise the agent would
/// silently fall back to direct API. Skipped when no NautGate env is present.
async fn nautgate_preflight(agent_id: &str) -> Result<(), String> {
    if agent_id == "shell" {
        return Ok(());
    }
    let registry = crate::agents::load_or_seed_registry()?;
    let Some(cfg) = registry.find(agent_id) else {
        // Unknown ids fail with a clear error in agent_shell_command.
        return Ok(());
    };
    if !cfg.env.values().any(|v| v.contains("localhost:8090")) {
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;
    match client.get("http://localhost:8090/health").send().await {
        Ok(resp) if resp.status().is_success() => Ok(()),
        _ => Err(
            "NautGate is not reachable on localhost:8090 — start it first (the agent would silently fall back to direct API)"
                .into(),
        ),
    }
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

/// Creates a full project: folder under project_root/category, git repo with a
/// baseline-prompt context file, a forge repo wired as origin, and a Zellij
/// session running the chosen agent.
#[tauri::command]
pub async fn scaffold_init_project(
    state: tauri::State<'_, crate::state::AppState>,
    name: String,
    category_label: String,
    forge_index: usize,
    agent_id: String,
    baseline_prompt: String,
    private: bool,
) -> Result<LaunchSpec, String> {
    let settings = state.settings.lock().await.clone();

    // a. Resolve category + target path.
    let category = resolve_category(&settings, &category_label)?;
    let sanitized = sanitize_name(&name);
    if sanitized.is_empty() {
        return Err(format!(
            "project name '{name}' sanitizes to an empty folder name"
        ));
    }
    let path = PathBuf::from(&settings.project_root)
        .join(&category.folder)
        .join(&sanitized);
    if path.exists() {
        return Err(format!("path already exists: {}", path.display()));
    }

    // b. Zellij must be present before we touch the filesystem.
    if !crate::zellij::is_installed() {
        return Err("zellij not installed — brew install zellij".into());
    }

    // c. NautGate preflight for the chosen agent.
    nautgate_preflight(&agent_id).await?;

    // d. Local scaffold: dir, context file, README, initial commit.
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("failed to create {}: {e}", path.display()))?;
    let ctx = context_file_name(&agent_id);
    std::fs::write(path.join(ctx), &baseline_prompt)
        .map_err(|e| format!("failed to write {ctx}: {e}"))?;
    std::fs::write(path.join("README.md"), format!("# {name}\n"))
        .map_err(|e| format!("failed to write README.md: {e}"))?;
    git_init_commit(&path, "init: project scaffold (xNaut)")?;

    // e. Forge repo + origin. Push failure is non-fatal.
    let forge = resolve_forge(&settings, forge_index)?;
    let clone_url = crate::forges::create_repo(&forge, &name, private, "Created by xNaut")
        .await
        .map_err(|e| format!("forge repo creation failed: {e}"))?;
    add_remote_and_push(&path, &clone_url)?;

    // f. Zellij session + layout + launch command.
    let path_str = path.to_string_lossy().into_owned();
    let session = crate::zellij::session_name(&name);
    let shell_cmd = agent_shell_command(&agent_id, Some(ctx))?;
    let layout = crate::zellij::write_layout(&session, &path_str, &shell_cmd)?;
    let command = crate::zellij::launch_command(&session, Some(&layout));

    // g. Register in the task registry.
    let task = TaskSession {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        kind: "project".into(),
        path: path_str.clone(),
        zellij_session: session,
        agent_id: Some(agent_id),
        created: chrono::Utc::now().to_rfc3339(),
        project_type: Some(category.label),
        forge_remote: Some(clone_url),
    };
    crate::tasks::upsert_task(task.clone())?;
    Ok(LaunchSpec {
        task,
        command,
        cwd: path_str,
    })
}

/// Creates a lightweight task: a scratch folder under ~/.xnaut/tasks (no git,
/// no forge) with a Zellij session running the agent — or the user's shell.
#[tauri::command]
pub async fn scaffold_init_task(
    name: String,
    agent_id: Option<String>,
) -> Result<LaunchSpec, String> {
    let sanitized = sanitize_name(&name);
    if sanitized.is_empty() {
        return Err(format!(
            "task name '{name}' sanitizes to an empty folder name"
        ));
    }
    let home = dirs::home_dir().ok_or("cannot resolve home directory")?;
    let path = home.join(".xnaut").join("tasks").join(&sanitized);
    if path.exists() {
        return Err(format!("path already exists: {}", path.display()));
    }
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("failed to create {}: {e}", path.display()))?;

    let path_str = path.to_string_lossy().into_owned();
    let session = crate::zellij::session_name(&name);
    let shell_cmd = agent_shell_command(agent_id.as_deref().unwrap_or("shell"), None)?;
    let layout = crate::zellij::write_layout(&session, &path_str, &shell_cmd)?;
    let command = crate::zellij::launch_command(&session, Some(&layout));

    let task = TaskSession {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        kind: "task".into(),
        path: path_str.clone(),
        zellij_session: session,
        agent_id,
        created: chrono::Utc::now().to_rfc3339(),
        project_type: None,
        forge_remote: None,
    };
    crate::tasks::upsert_task(task.clone())?;
    Ok(LaunchSpec {
        task,
        command,
        cwd: path_str,
    })
}

/// Promotes a lightweight task to a project: moves the folder under
/// project_root/category, initializes git, creates a forge repo as origin, and
/// updates the registry entry in place. The Zellij session name is kept.
#[tauri::command]
pub async fn scaffold_promote_task(
    state: tauri::State<'_, crate::state::AppState>,
    task_id: String,
    category_label: String,
    forge_index: usize,
) -> Result<crate::tasks::TaskSession, String> {
    let settings = state.settings.lock().await.clone();

    let mut tasks = crate::tasks::load_tasks();
    let idx = tasks
        .iter()
        .position(|t| t.id == task_id)
        .ok_or_else(|| format!("no task with id {task_id}"))?;
    if tasks[idx].kind != "task" {
        return Err(format!(
            "entry {task_id} is kind '{}', not 'task'",
            tasks[idx].kind
        ));
    }

    let category = resolve_category(&settings, &category_label)?;
    let folder_name = sanitize_name(&tasks[idx].name);
    let new_path = PathBuf::from(&settings.project_root)
        .join(&category.folder)
        .join(&folder_name);
    if new_path.exists() {
        return Err(format!("path already exists: {}", new_path.display()));
    }
    if let Some(parent) = new_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    std::fs::rename(&tasks[idx].path, &new_path).map_err(|e| {
        format!(
            "failed to move {} → {}: {e}",
            tasks[idx].path,
            new_path.display()
        )
    })?;

    git_init_commit(&new_path, "init: promoted task to project (xNaut)")?;

    let forge = resolve_forge(&settings, forge_index)?;
    let clone_url = crate::forges::create_repo(&forge, &folder_name, true, "Created by xNaut")
        .await
        .map_err(|e| format!("forge repo creation failed: {e}"))?;
    add_remote_and_push(&new_path, &clone_url)?;

    let task = &mut tasks[idx];
    task.kind = "project".into();
    task.path = new_path.to_string_lossy().into_owned();
    task.project_type = Some(category.label);
    task.forge_remote = Some(clone_url);
    let updated = task.clone();
    crate::tasks::save_tasks(&tasks)?;
    Ok(updated)
}

/// Starts a task from a forge issue/PR: creates a worktree off the local clone
/// on a fix/issue-N branch, writes the issue as the agent's context file, and
/// launches the agent in a Zellij session.
#[tauri::command]
pub async fn scaffold_task_from_issue(
    state: tauri::State<'_, crate::state::AppState>,
    forge_index: usize,
    repo: String,
    number: u64,
    agent_id: String,
    local_repo_path: String,
    base_branch: String,
) -> Result<LaunchSpec, String> {
    // a. Resolve forge + fetch the issue.
    let settings = state.settings.lock().await.clone();
    let forge = resolve_forge(&settings, forge_index)?;
    let issue = crate::forges::get_issue(&forge, &repo, number)
        .await
        .map_err(|e| format!("failed to fetch issue #{number} from {repo}: {e}"))?;

    // b. Branch + worktree path.
    let slug = slug_from_title(&issue.title);
    let branch = format!("fix/issue-{number}-{slug}");
    let home = dirs::home_dir().ok_or("cannot resolve home directory")?;
    let wt_path = home
        .join(".naut-worktrees")
        .join(&forge.owner)
        .join(&repo)
        .join(format!("issue-{number}"));
    if wt_path.exists() {
        return Err(format!(
            "worktree path already exists: {}",
            wt_path.display()
        ));
    }

    // c. Create the worktree off the local clone.
    crate::worktree::add_worktree(
        Path::new(&local_repo_path),
        &wt_path,
        &crate::worktree::AddWorktreeOptions {
            branch: branch.clone(),
            base: Some(base_branch),
            checkout_existing: false,
            no_auto_setup_remote: false,
        },
    )
    .map_err(|e| format!("worktree creation failed: {e}"))?;

    // d. Write the issue as the agent's context file.
    let ctx = context_file_name(&agent_id);
    let context = issue_context_markdown(
        issue.number,
        &issue.title,
        &issue.html_url,
        &issue.labels,
        &issue.body,
        &branch,
    );
    std::fs::write(wt_path.join(ctx), context)
        .map_err(|e| format!("failed to write {ctx}: {e}"))?;

    // e. Preflight + session + registry.
    nautgate_preflight(&agent_id).await?;
    let wt_str = wt_path.to_string_lossy().into_owned();
    let session = crate::zellij::session_name(&format!("{repo}-issue-{number}"));
    let shell_cmd = agent_shell_command(&agent_id, Some(ctx))?;
    let layout = crate::zellij::write_layout(&session, &wt_str, &shell_cmd)?;
    let command = crate::zellij::launch_command(&session, Some(&layout));

    let task = TaskSession {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("{repo} issue #{number}: {}", issue.title),
        kind: "task".into(),
        path: wt_str.clone(),
        zellij_session: session,
        agent_id: Some(agent_id),
        created: chrono::Utc::now().to_rfc3339(),
        project_type: None,
        forge_remote: None,
    };
    crate::tasks::upsert_task(task.clone())?;
    Ok(LaunchSpec {
        task,
        command,
        cwd: wt_str,
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_alnum_dash_underscore_dot() {
        assert_eq!(sanitize_name("My Cool App!"), "My-Cool-App");
        assert_eq!(sanitize_name("v1.2_x-y"), "v1.2_x-y");
        assert_eq!(sanitize_name("  spaced   out  "), "spaced-out");
        assert_eq!(sanitize_name("über/app"), "ber-app");
    }

    #[test]
    fn slug_takes_first_five_words_lowercased() {
        assert_eq!(
            slug_from_title("Fix: the login page crashes on Safari"),
            "fix-the-login-page-crashes"
        );
        assert_eq!(slug_from_title("One"), "one");
        assert_eq!(slug_from_title("!!! ??? Bug"), "bug");
    }

    #[test]
    fn context_file_name_maps_claude_else_agents() {
        assert_eq!(context_file_name("claude"), "CLAUDE.md");
        assert_eq!(context_file_name("codex"), "AGENTS.md");
        assert_eq!(context_file_name("pi"), "AGENTS.md");
    }

    #[test]
    fn build_shell_command_quotes_env_values_and_sorts_keys() {
        let env = HashMap::from([
            ("X".to_string(), "it's".to_string()),
            (
                "ANTHROPIC_BASE_URL".to_string(),
                "http://localhost:8090".to_string(),
            ),
        ]);
        let cmd = build_shell_command(
            &env,
            "claude",
            &["--dangerously-skip-permissions".to_string()],
        );
        assert_eq!(
            cmd,
            "ANTHROPIC_BASE_URL='http://localhost:8090' X='it'\\''s' claude --dangerously-skip-permissions"
        );
    }

    #[test]
    fn build_shell_command_without_env_is_just_cmd_and_args() {
        let cmd = build_shell_command(&HashMap::new(), "gemini", &[]);
        assert_eq!(cmd, "gemini");
    }

    #[test]
    fn issue_context_starts_with_instruction_header() {
        let md = issue_context_markdown(
            7,
            "Login broken",
            "http://forge/x/issues/7",
            &["bug".to_string()],
            "Steps to reproduce…",
            "fix/issue-7-login-broken",
        );
        assert!(md.starts_with(
            "You are working on the following issue. Branch: fix/issue-7-login-broken."
        ));
        assert!(md.contains("# Issue #7: Login broken"));
        assert!(md.contains("Labels: bug"));
        assert!(md.contains("Steps to reproduce…"));
    }
}
