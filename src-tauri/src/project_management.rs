use crate::settings::ProjectManagementSettings;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tauri::State;

const MANIFEST_NAME: &str = "xnaut-projects.json";

fn mutation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleSetupRequest {
    pub repo_path: String,
    pub repo_name: String,
    #[serde(default)]
    pub create_remote: bool,
    #[serde(default)]
    pub forge_index: Option<usize>,
    #[serde(default)]
    pub forge_owner: Option<String>,
    #[serde(default)]
    pub forge_token: Option<String>,
    #[serde(default)]
    pub personal_owner: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleConnectRequest {
    pub repo_path: String,
    #[serde(default)]
    pub remote_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModuleStatus {
    pub enabled: bool,
    pub configured: bool,
    pub valid: bool,
    pub repo_path: String,
    pub remote_url: String,
    pub git_repository: bool,
    pub project_count: usize,
    pub ticket_count: usize,
    pub error: String,
    pub warning: String,
    pub branch: String,
    pub last_commit: String,
    pub dirty: bool,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub version: u64,
    pub event: String,
    pub subject: String,
    pub timestamp: String,
    #[serde(default)]
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub client_name: String,
    #[serde(default)]
    pub contact_name: String,
    #[serde(default)]
    pub contact_email: String,
    #[serde(default)]
    pub budget_chf: Option<f64>,
    #[serde(default)]
    pub hourly_rate_chf: Option<f64>,
    #[serde(default = "default_flow_type")]
    pub flow_type: String,
    #[serde(default = "default_project_stage")]
    pub stage: String,
    #[serde(default = "default_revision")]
    pub revision: u64,
    #[serde(default)]
    pub source_repo: String,
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub forge_remote: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub client: Option<crate::pm::ExternalProject>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketRecord {
    pub id: String,
    pub project: String,
    pub title: String,
    #[serde(rename = "type")]
    pub ticket_type: String,
    pub status: String,
    pub priority: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub documentation: Vec<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub source_id: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeArtifact {
    pub kind: String,
    pub vault_ref: String,
    pub status: String,
    #[serde(default)]
    pub content_hash: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeReview {
    pub id: String,
    pub reviewer: String,
    pub verdict: String,
    pub summary: String,
    #[serde(default)]
    pub findings: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeApproval {
    pub actor: String,
    pub approved: bool,
    pub comment: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub id: String,
    pub project: String,
    pub title: String,
    pub profile: String,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub source_ticket: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub baseline_refs: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<ChangeArtifact>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub reviews: Vec<ChangeReview>,
    #[serde(default)]
    pub approvals: Vec<ChangeApproval>,
    #[serde(default)]
    pub workflow_id: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeCreateRequest {
    pub project: String,
    pub title: String,
    pub profile: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub source_ticket: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeReviewRequest {
    pub project: String,
    pub change_id: String,
    pub expected_revision: u64,
    pub reviewer: String,
    pub verdict: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeArtifactStatusRequest {
    pub project: String,
    pub change_id: String,
    pub expected_revision: u64,
    pub kind: String,
    pub ready: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeApprovalRequest {
    pub project: String,
    pub change_id: String,
    pub expected_revision: u64,
    pub actor: String,
    pub approved: bool,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectCreateRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub client_name: String,
    #[serde(default)]
    pub contact_name: String,
    #[serde(default)]
    pub contact_email: String,
    #[serde(default)]
    pub budget_chf: Option<f64>,
    #[serde(default)]
    pub hourly_rate_chf: Option<f64>,
    #[serde(default = "default_flow_type")]
    pub flow_type: String,
    #[serde(default)]
    pub source_repo: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectUpdateRequest {
    pub key: String,
    pub expected_revision: u64,
    pub name: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub client_name: String,
    #[serde(default)]
    pub contact_name: String,
    #[serde(default)]
    pub contact_email: String,
    #[serde(default)]
    pub budget_chf: Option<f64>,
    #[serde(default)]
    pub hourly_rate_chf: Option<f64>,
    #[serde(default = "default_flow_type")]
    pub flow_type: String,
    #[serde(default)]
    pub source_repo: String,
    #[serde(default)]
    pub stage: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TicketCreateRequest {
    pub project: String,
    pub title: String,
    #[serde(default = "default_ticket_type")]
    pub ticket_type: String,
    #[serde(default = "default_ticket_status")]
    pub status: String,
    #[serde(default = "default_ticket_priority")]
    pub priority: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub documentation: Vec<String>,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TicketUpdateRequest {
    pub id: String,
    pub expected_revision: u64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub ticket_type: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub owner: Option<Option<String>>,
    #[serde(default)]
    pub clear_owner: bool,
    #[serde(default)]
    pub documentation: Option<Vec<String>>,
    #[serde(default)]
    pub body: Option<String>,
}

fn default_ticket_type() -> String {
    "task".into()
}
fn default_ticket_status() -> String {
    "inbox".into()
}
fn default_ticket_priority() -> String {
    "medium".into()
}

fn default_flow_type() -> String {
    "standard".into()
}

fn default_project_stage() -> String {
    "idea".into()
}

fn default_revision() -> u64 {
    1
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| format!("failed to invoke git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git_authenticated(
    repo: &Path,
    args: &[&str],
    authorization_header: &str,
) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.extraHeader")
        .env("GIT_CONFIG_VALUE_0", authorization_header)
        .output()
        .map_err(|error| format!("failed to invoke git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn validate_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty()
        || name.starts_with('.')
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(
            "repository name must contain only letters, numbers, dashes, and underscores".into(),
        );
    }
    Ok(name.to_string())
}

fn validate_project_key(raw: &str) -> Result<String, String> {
    let key = raw.trim().to_ascii_uppercase();
    if key.len() < 2
        || key.len() > 12
        || !key
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
    {
        return Err("project key must be 2-12 letters or numbers".into());
    }
    Ok(key)
}

fn project_key_seed(name: &str) -> String {
    let mut key: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .take(12)
        .collect();
    if key.is_empty() {
        key = "PROJECT".into();
    } else if key.len() == 1 {
        key.push('X');
    }
    key
}

fn unique_project_key(name: &str, used: &std::collections::HashSet<String>) -> String {
    let base = project_key_seed(name);
    if !used.contains(&base) {
        return base;
    }
    for suffix in 2..10_000 {
        let suffix = suffix.to_string();
        let keep = 12usize.saturating_sub(suffix.len());
        let candidate = format!("{}{}", &base[..base.len().min(keep)], suffix);
        if !used.contains(&candidate) {
            return candidate;
        }
    }
    format!("P{}", uuid::Uuid::new_v4().simple())[..12].to_string()
}

fn validate_choice(value: &str, field: &str, allowed: &[&str]) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if allowed.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(format!("invalid {field}: {value}"))
    }
}

fn resolve_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("repository path is required".into());
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Ok(dirs::home_dir()
            .ok_or("cannot resolve home directory")?
            .join(rest));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err("repository path must be absolute or start with ~/".into());
    }
    Ok(path)
}

fn initial_readme(name: &str) -> String {
    format!(
        "# {name}\n\nPrivate xNaut Project Management repository.\n\n- `projects/` contains project manifests and tickets.\n- `events/` contains append-only workflow events.\n- `agents/` contains project-management agent policies.\n- `schema/` contains machine-readable format versions.\n\nThis repository contains operational metadata, not source code or secrets.\n"
    )
}

fn ticket_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://xnaut.local/schema/ticket-v1.json",
        "title": "xNaut Project Ticket",
        "type": "object",
        "required": ["id", "project", "title", "type", "status", "created_at", "updated_at", "revision"],
        "properties": {
            "id": { "type": "string", "pattern": "^[A-Z][A-Z0-9]+-[0-9]+$" },
            "project": { "type": "string" },
            "title": { "type": "string", "minLength": 1 },
            "type": { "enum": ["idea", "feature", "bug", "incident", "task"] },
            "status": { "enum": ["inbox", "ready", "in_progress", "review", "blocked", "done"] },
            "priority": { "enum": ["low", "medium", "high", "critical"] },
            "owner": { "type": ["string", "null"] },
            "documentation": { "type": "array", "items": { "type": "string" } },
            "body": { "type": "string" },
            "source_id": { "type": "string" },
            "revision": { "type": "integer", "minimum": 1 },
            "created_at": { "type": "string" },
            "updated_at": { "type": "string" }
        }
    })
}

fn initialize_local_repo(path: &Path, name: &str) -> Result<(), String> {
    if path.exists() {
        let mut entries = std::fs::read_dir(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if entries.next().is_some() {
            return Err(format!("setup folder is not empty: {}", path.display()));
        }
    }
    for rel in ["projects", "events", "agents", "schema"] {
        std::fs::create_dir_all(path.join(rel))
            .map_err(|error| format!("failed to create {rel}: {error}"))?;
    }
    let now = chrono::Utc::now().to_rfc3339();
    let manifest = json!({
        "format": "xnaut-project-management",
        "version": 1,
        "name": name,
        "created_at": now,
    });
    std::fs::write(
        path.join(MANIFEST_NAME),
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write module manifest: {error}"))?;
    std::fs::write(path.join("README.md"), initial_readme(name))
        .map_err(|error| format!("failed to write README: {error}"))?;
    std::fs::write(
        path.join("schema/ticket-v1.schema.json"),
        serde_json::to_vec_pretty(&ticket_schema()).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write ticket schema: {error}"))?;
    std::fs::write(path.join("projects/.gitkeep"), "")
        .map_err(|error| format!("failed to initialize projects folder: {error}"))?;
    std::fs::write(path.join("events/.gitkeep"), "")
        .map_err(|error| format!("failed to initialize events folder: {error}"))?;
    std::fs::write(path.join("agents/.gitkeep"), "")
        .map_err(|error| format!("failed to initialize agents folder: {error}"))?;

    run_git(path, &["init"])?;
    run_git(path, &["branch", "-M", "main"])?;
    run_git(path, &["add", "-A"])?;
    run_git(
        path,
        &[
            "-c",
            "user.name=xNaut",
            "-c",
            "user.email=xnaut@local",
            "commit",
            "-m",
            "chore: initialize xNaut project management",
        ],
    )?;
    Ok(())
}

fn initialize_local_repo_transactional(path: &Path, name: &str) -> Result<(), String> {
    if path.exists() {
        let mut entries = std::fs::read_dir(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if entries.next().is_some() {
            return Err(format!("setup folder is not empty: {}", path.display()));
        }
    }
    let parent = path
        .parent()
        .ok_or("repository path needs a parent folder")?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create parent folder: {error}"))?;
    let leaf = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid repository path")?;
    let staging = parent.join(format!(".{leaf}.xnaut-setup-{}", uuid::Uuid::new_v4()));
    let result = initialize_local_repo(&staging, name).and_then(|_| {
        if path.exists() {
            std::fs::remove_dir(path)
                .map_err(|error| format!("failed to replace empty setup folder: {error}"))?;
        }
        std::fs::rename(&staging, path)
            .map_err(|error| format!("failed to finish repository setup: {error}"))
    });
    if result.is_err() && staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn is_initialized_repo(path: &Path) -> bool {
    path.join(".git").is_dir() && path.join(MANIFEST_NAME).is_file()
}

fn configure_origin(path: &Path, requested_remote: &str) -> Result<String, String> {
    let requested_remote = requested_remote.trim();
    if !requested_remote.is_empty() {
        if run_git(path, &["remote", "get-url", "origin"]).is_ok() {
            run_git(path, &["remote", "set-url", "origin", requested_remote])?;
        } else {
            run_git(path, &["remote", "add", "origin", requested_remote])?;
        }
    }
    Ok(run_git(path, &["remote", "get-url", "origin"]).unwrap_or_default())
}

fn sync_repo(repo: &Path, authenticated_remote: Option<(&str, &str)>) -> Result<(), String> {
    let origin = run_git(repo, &["remote", "get-url", "origin"]).unwrap_or_default();
    if origin.is_empty() {
        return Err("no origin remote is configured".into());
    }
    if !run_git(repo, &["status", "--porcelain"])?.is_empty() {
        return Err(
            "control repository has uncommitted changes; resolve them before syncing".into(),
        );
    }
    let branch = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let remote_ref = format!("refs/remotes/origin/{branch}");
    if let Some((remote_url, authorization)) = authenticated_remote {
        let refspec = format!("+refs/heads/{branch}:{remote_ref}");
        // An empty remote has no branch yet; that is expected on the first sync.
        let fetch = run_git_authenticated(repo, &["fetch", remote_url, &refspec], authorization);
        if let Err(error) = fetch {
            let remote_heads = run_git_authenticated(
                repo,
                &["ls-remote", "--heads", remote_url, &branch],
                authorization,
            )?;
            if !remote_heads.is_empty() {
                return Err(error);
            }
        }
        if run_git(repo, &["show-ref", "--verify", "--quiet", &remote_ref]).is_ok() {
            if let Err(error) = run_git(repo, &["rebase", &remote_ref]) {
                let _ = run_git(repo, &["rebase", "--abort"]);
                return Err(error);
            }
        }
        let push_ref = format!("refs/heads/{branch}:refs/heads/{branch}");
        run_git_authenticated(repo, &["push", remote_url, &push_ref], authorization)?;
        run_git(repo, &["update-ref", &remote_ref, "HEAD"])?;
        run_git(
            repo,
            &[
                "branch",
                "--set-upstream-to",
                &format!("origin/{branch}"),
                &branch,
            ],
        )?;
        return Ok(());
    }

    run_git(repo, &["fetch", "origin"])?;
    if run_git(repo, &["show-ref", "--verify", "--quiet", &remote_ref]).is_ok() {
        if let Err(error) = run_git(repo, &["pull", "--rebase", "origin", &branch]) {
            let _ = run_git(repo, &["rebase", "--abort"]);
            return Err(error);
        }
    }
    run_git(repo, &["push", "-u", "origin", &branch])?;
    Ok(())
}

fn authenticated_remote(
    settings: &crate::settings::Settings,
    origin: &str,
) -> Option<(String, String)> {
    let repo_name = origin
        .trim_end_matches('/')
        .rsplit('/')
        .next()?
        .trim_end_matches(".git");
    let host = settings.forges.iter().find(|host| {
        !host.owner.trim().is_empty() && origin.contains(&format!("/{}/", host.owner.trim()))
    })?;
    let token = crate::settings::resolve_forge_token(host)?;
    let remote_url = format!(
        "{}/{}/{}.git",
        host.base_url.trim_end_matches('/'),
        host.owner.trim(),
        repo_name
    );
    let authorization = match host.kind.as_str() {
        "forgejo" => format!("Authorization: token {token}"),
        "github" => format!("Authorization: Bearer {token}"),
        "gitlab" => format!("PRIVATE-TOKEN: {token}"),
        _ => return None,
    };
    Some((remote_url, authorization))
}

fn count_files(root: &Path, suffix: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                count_files(&path, suffix)
            } else if path.to_string_lossy().ends_with(suffix) {
                1
            } else {
                0
            }
        })
        .sum()
}

fn inspect(settings: &ProjectManagementSettings) -> ModuleStatus {
    let configured = !settings.repo_path.trim().is_empty();
    let mut status = ModuleStatus {
        enabled: settings.enabled,
        configured,
        valid: false,
        repo_path: settings.repo_path.clone(),
        remote_url: settings.remote_url.clone(),
        git_repository: false,
        project_count: 0,
        ticket_count: 0,
        error: String::new(),
        warning: String::new(),
        branch: String::new(),
        last_commit: String::new(),
        dirty: false,
        ahead: 0,
        behind: 0,
    };
    if !configured {
        return status;
    }
    let Ok(path) = resolve_path(&settings.repo_path) else {
        status.error = "configured repository path is invalid".into();
        return status;
    };
    status.git_repository = path.join(".git").is_dir();
    status.valid = status.git_repository && path.join(MANIFEST_NAME).is_file();
    if status.valid {
        status.project_count = std::fs::read_dir(path.join("projects"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().is_dir())
                    .count()
            })
            .unwrap_or(0);
        status.ticket_count =
            count_files(&path.join("projects"), ".json").saturating_sub(status.project_count);
        status.branch = run_git(&path, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
        status.last_commit = run_git(&path, &["log", "-1", "--format=%h %s"]).unwrap_or_default();
        status.dirty = !run_git(&path, &["status", "--porcelain"])
            .unwrap_or_default()
            .is_empty();
        if let Ok(counts) = run_git(
            &path,
            &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
        ) {
            let values: Vec<usize> = counts
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect();
            if values.len() == 2 {
                status.ahead = values[0];
                status.behind = values[1];
            }
        }
    } else {
        status.error = "folder is not an initialized xNaut Project Management repository".into();
    }
    status
}

fn configured_repo(settings: &ProjectManagementSettings) -> Result<PathBuf, String> {
    if !settings.enabled {
        return Err("Project Management module is disabled".into());
    }
    let status = inspect(settings);
    if !status.valid {
        return Err(if status.error.is_empty() {
            "Project Management repository is not configured".into()
        } else {
            status.error
        });
    }
    resolve_path(&settings.repo_path)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(&tmp, bytes)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    std::fs::rename(&tmp, path)
        .map_err(|error| format!("failed to replace {}: {error}", path.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid {}: {error}", path.display()))
}

fn record_mutation(
    repo: &Path,
    event_type: &str,
    subject: &str,
    details: Value,
    paths: &[PathBuf],
    message: &str,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let event_path = repo.join("events").join(format!(
        "{}-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
        uuid::Uuid::new_v4()
    ));
    write_json_atomic(
        &event_path,
        &json!({
            "version": 1,
            "event": event_type,
            "subject": subject,
            "timestamp": now,
            "details": details,
        }),
    )?;
    let mut commit_paths = paths.to_vec();
    commit_paths.push(event_path);
    let relative: Result<Vec<String>, String> = commit_paths
        .iter()
        .map(|path| {
            path.strip_prefix(repo)
                .map(|rel| rel.to_string_lossy().into_owned())
                .map_err(|_| "mutation path escaped repository".into())
        })
        .collect();
    let relative = relative?;
    let mut add_args = vec!["add".to_string(), "--".into()];
    add_args.extend(relative.iter().cloned());
    let add_refs: Vec<&str> = add_args.iter().map(String::as_str).collect();
    run_git(repo, &add_refs)?;
    let mut args = vec![
        "-c".to_string(),
        "user.name=xNaut".into(),
        "-c".into(),
        "user.email=xnaut@local".into(),
        "commit".into(),
        "--only".into(),
        "-m".into(),
        message.into(),
        "--".into(),
    ];
    args.extend(relative);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git(repo, &refs)?;
    Ok(())
}

fn list_projects(repo: &Path) -> Result<Vec<ProjectRecord>, String> {
    let mut projects = Vec::new();
    for entry in std::fs::read_dir(repo.join("projects"))
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let manifest = entry.path().join("project.json");
        if manifest.is_file() {
            projects.push(read_json(&manifest)?);
        }
    }
    projects.sort_by(|a: &ProjectRecord, b: &ProjectRecord| a.key.cmp(&b.key));
    Ok(projects)
}

fn import_task_projects(
    repo: &Path,
    tasks: &[crate::tasks::TaskSession],
) -> Result<Vec<ProjectRecord>, String> {
    let mut existing = list_projects(repo)?;
    let mut used: std::collections::HashSet<String> =
        existing.iter().map(|project| project.key.clone()).collect();
    let mut imported = Vec::new();
    let mut linked = Vec::new();
    let mut paths = Vec::new();
    for task in tasks.iter().filter(|task| task.kind == "project") {
        let forge_remote = task.forge_remote.clone().unwrap_or_default();
        let existing_index = existing.iter().position(|project| {
            (!task.id.is_empty() && project.task_id == task.id)
                || (!task.path.is_empty()
                    && (project.source_path == task.path || project.source_repo == task.path))
                || (!forge_remote.is_empty()
                    && (project.forge_remote == forge_remote
                        || project.source_repo == forge_remote))
                || project.name.eq_ignore_ascii_case(&task.name)
        });
        if let Some(index) = existing_index {
            let project = &mut existing[index];
            let mut changed = false;
            if project.task_id.is_empty() && !task.id.is_empty() {
                project.task_id = task.id.clone();
                changed = true;
            }
            if project.source_path.is_empty() && !task.path.is_empty() {
                project.source_path = task.path.clone();
                changed = true;
            }
            if project.forge_remote.is_empty() && !forge_remote.is_empty() {
                project.forge_remote = forge_remote.clone();
                changed = true;
            }
            if project.source_repo.is_empty() {
                project.source_repo = if forge_remote.is_empty() {
                    task.path.clone()
                } else {
                    forge_remote.clone()
                };
                changed = !project.source_repo.is_empty() || changed;
            }
            if changed {
                let manifest = repo
                    .join("projects")
                    .join(&project.key)
                    .join("project.json");
                write_json_atomic(&manifest, project)?;
                paths.push(manifest);
                linked.push(project.key.clone());
            }
            continue;
        }
        let key = unique_project_key(&task.name, &used);
        used.insert(key.clone());
        let project_dir = repo.join("projects").join(&key);
        std::fs::create_dir_all(project_dir.join("tickets"))
            .map_err(|error| format!("failed to import project {}: {error}", task.name))?;
        let record = ProjectRecord {
            key,
            name: task.name.clone(),
            purpose: String::new(),
            owner: String::new(),
            client_name: String::new(),
            contact_name: String::new(),
            contact_email: String::new(),
            budget_chf: None,
            hourly_rate_chf: None,
            flow_type: default_flow_type(),
            stage: default_project_stage(),
            revision: default_revision(),
            source_repo: if forge_remote.is_empty() {
                task.path.clone()
            } else {
                forge_remote.clone()
            },
            source_path: task.path.clone(),
            forge_remote,
            task_id: task.id.clone(),
            client: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let manifest = project_dir.join("project.json");
        write_json_atomic(&manifest, &record)?;
        paths.push(manifest);
        imported.push(record);
    }
    if !paths.is_empty() {
        let keys: Vec<&str> = imported
            .iter()
            .map(|project| project.key.as_str())
            .collect();
        record_mutation(
            repo,
            "projects.imported",
            "xnaut-registry",
            json!({ "projects": keys, "linked": linked }),
            &paths,
            &format!(
                "feat(pm): reconcile {} xNaut projects",
                imported.len() + linked.len()
            ),
        )?;
    }
    list_projects(repo)
}

fn normalized_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn next_ticket_sequence(tickets_dir: &Path) -> Result<u64, String> {
    Ok(std::fs::read_dir(tickets_dir)
        .map_err(|error| error.to_string())?
        .flatten()
        .filter_map(|entry| {
            entry
                .path()
                .file_stem()?
                .to_str()?
                .split_once('-')?
                .1
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap_or(0)
        + 1)
}

fn migrate_legacy_pm_data(
    repo: &Path,
    clients: &[crate::pm::ExternalProject],
    todos: &std::collections::HashMap<String, Vec<crate::project_todos::Todo>>,
) -> Result<Vec<ProjectRecord>, String> {
    let mut projects = list_projects(repo)?;
    let mut used: std::collections::HashSet<String> =
        projects.iter().map(|project| project.key.clone()).collect();
    let mut paths = Vec::new();
    let mut migrated_clients = 0usize;

    for client in clients {
        let company = normalized_name(&client.client_company);
        let index = projects.iter().position(|project| {
            project.task_id == client.task_id
                || (!normalized_name(&project.name).is_empty()
                    && company.contains(&normalized_name(&project.name)))
        });
        let index = if let Some(index) = index {
            index
        } else {
            let key = unique_project_key(&client.client_company, &used);
            used.insert(key.clone());
            let project_dir = repo.join("projects").join(&key);
            std::fs::create_dir_all(project_dir.join("tickets"))
                .map_err(|error| format!("failed to migrate {}: {error}", client.client_company))?;
            projects.push(ProjectRecord {
                key,
                name: client.client_company.clone(),
                purpose: client.scope.clone(),
                owner: String::new(),
                client_name: client.client_company.clone(),
                contact_name: client
                    .contacts
                    .first()
                    .map(|contact| contact.name.clone())
                    .unwrap_or_default(),
                contact_email: client
                    .contacts
                    .first()
                    .map(|contact| contact.email.clone())
                    .unwrap_or_default(),
                budget_chf: client.offer_amount_chf,
                hourly_rate_chf: Some(client.rate_chf_per_hour),
                flow_type: default_flow_type(),
                stage: default_project_stage(),
                revision: default_revision(),
                source_repo: String::new(),
                source_path: String::new(),
                forge_remote: String::new(),
                task_id: client.task_id.clone(),
                client: None,
                created_at: client.created.clone(),
            });
            projects.len() - 1
        };
        let same = projects[index]
            .client
            .as_ref()
            .and_then(|current| serde_json::to_value(current).ok())
            == serde_json::to_value(client).ok();
        if !same {
            projects[index].client = Some(client.clone());
            let manifest = repo
                .join("projects")
                .join(&projects[index].key)
                .join("project.json");
            write_json_atomic(&manifest, &projects[index])?;
            paths.push(manifest);
            migrated_clients += 1;
        }
    }

    let existing_source_ids: std::collections::HashSet<String> = projects
        .iter()
        .flat_map(|project| {
            let dir = repo.join("projects").join(&project.key).join("tickets");
            std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|entry| read_json::<TicketRecord>(&entry.path()).ok())
                .map(|ticket| ticket.source_id)
                .filter(|source_id| !source_id.is_empty())
                .collect::<Vec<_>>()
        })
        .collect();
    let mut migrated_todos = 0usize;
    for (task_id, entries) in todos {
        let project_index = if let Some(index) = projects
            .iter()
            .position(|project| project.task_id == *task_id)
        {
            index
        } else if let Some(index) = projects.iter().position(|project| project.key == "LEGACY") {
            index
        } else {
            let key = unique_project_key("LEGACY", &used);
            used.insert(key.clone());
            let project_dir = repo.join("projects").join(&key);
            std::fs::create_dir_all(project_dir.join("tickets"))
                .map_err(|error| format!("failed to create legacy project: {error}"))?;
            let project = ProjectRecord {
                key,
                name: "Legacy PM Migration".into(),
                purpose: "Preserved work from the legacy project store.".into(),
                owner: String::new(),
                client_name: String::new(),
                contact_name: String::new(),
                contact_email: String::new(),
                budget_chf: None,
                hourly_rate_chf: None,
                flow_type: default_flow_type(),
                stage: default_project_stage(),
                revision: default_revision(),
                source_repo: String::new(),
                source_path: String::new(),
                forge_remote: String::new(),
                task_id: String::new(),
                client: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            let manifest = project_dir.join("project.json");
            write_json_atomic(&manifest, &project)?;
            paths.push(manifest);
            projects.push(project);
            projects.len() - 1
        };
        let project = &projects[project_index];
        let tickets_dir = repo.join("projects").join(&project.key).join("tickets");
        for todo in entries {
            if existing_source_ids.contains(&todo.id) {
                continue;
            }
            let id = format!("{}-{}", project.key, next_ticket_sequence(&tickets_dir)?);
            let ticket = TicketRecord {
                id: id.clone(),
                project: project.key.clone(),
                title: todo.text.clone(),
                ticket_type: "task".into(),
                status: if todo.done { "done".into() } else { "inbox".into() },
                priority: "medium".into(),
                owner: None,
                documentation: Vec::new(),
                body: format!("Migrated from the legacy xNaut project todo store.\n\nOriginal project ID: {task_id}"),
                source_id: todo.id.clone(),
                revision: 1,
                created_at: todo.created.clone(),
                updated_at: todo.created.clone(),
            };
            let path = tickets_dir.join(format!("{id}.json"));
            write_json_atomic(&path, &ticket)?;
            paths.push(path);
            migrated_todos += 1;
        }
    }

    if !paths.is_empty() {
        record_mutation(
            repo,
            "legacy_pm.migrated",
            "legacy-pm",
            json!({ "client_records": migrated_clients, "todos": migrated_todos }),
            &paths,
            "feat(pm): migrate legacy project data",
        )?;
    }
    list_projects(repo)
}

fn list_events(
    repo: &Path,
    subject: Option<&str>,
    limit: usize,
) -> Result<Vec<EventRecord>, String> {
    let mut events = Vec::new();
    for entry in std::fs::read_dir(repo.join("events"))
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let event: EventRecord = read_json(&path)?;
        if subject.is_none_or(|wanted| event.subject == wanted) {
            events.push(event);
        }
    }
    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    events.truncate(limit.clamp(1, 500));
    Ok(events)
}

fn project_folder_name(project: &ProjectRecord) -> String {
    let value: String = project
        .name
        .chars()
        .map(|character| {
            if matches!(
                character,
                '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            ) {
                '-'
            } else {
                character
            }
        })
        .collect();
    let value = value.trim();
    if value.is_empty() {
        project.key.clone()
    } else {
        value.into()
    }
}

fn validate_change_profile(profile: &str) -> Result<String, String> {
    validate_choice(
        profile,
        "change profile",
        &["feature", "bug", "incident", "maintenance"],
    )
}

fn validate_change_id(project: &str, value: &str) -> Result<String, String> {
    let project = validate_project_key(project)?;
    let prefix = format!("{project}-CHG-");
    if !value.starts_with(&prefix)
        || value.len() > 80
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err("invalid change id".into());
    }
    Ok(value.into())
}

fn change_manifest(repo: &Path, project: &str, change_id: &str) -> Result<PathBuf, String> {
    let project = validate_project_key(project)?;
    let change_id = validate_change_id(&project, change_id)?;
    Ok(repo
        .join("projects")
        .join(project)
        .join("changes")
        .join(change_id)
        .join("change.json"))
}

fn read_change(repo: &Path, project: &str, change_id: &str) -> Result<ChangeRecord, String> {
    let path = change_manifest(repo, project, change_id)?;
    if !path.is_file() {
        return Err(format!("change not found: {change_id}"));
    }
    read_json(&path)
}

fn file_hash(path: &Path) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(std::fs::read(path).map_err(|error| error.to_string())?);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn baseline_refs(project: &ProjectRecord) -> Result<Vec<String>, String> {
    let root = PathBuf::from(crate::vault::vault_init()?)
        .join("work")
        .join("Development")
        .join(project_folder_name(project))
        .join("NAUT-Flow");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(Vec::new());
    };
    let mut refs: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("md"))
        .filter_map(|entry| {
            entry
                .path()
                .strip_prefix(PathBuf::from(crate::vault::vault_init().ok()?).join("work"))
                .ok()
                .map(|path| format!("work:{}", path.to_string_lossy()))
        })
        .collect();
    refs.sort();
    Ok(refs)
}

fn artifact_template(
    project: &ProjectRecord,
    change_id: &str,
    title: &str,
    profile: &str,
    kind: &str,
    source_ticket: &str,
    baseline: &[String],
) -> String {
    let baseline_links = if baseline.is_empty() {
        "- No approved baseline artifact was found. Resolve this before approval.".into()
    } else {
        baseline
            .iter()
            .map(|item| format!("- `{item}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let body = match kind {
        "proposal" => "## Problem and opportunity\n\nPending.\n\n## Intended outcome\n\nPending.\n\n## Scope\n\n### In scope\n\n- Pending\n\n### Out of scope\n\n- Pending\n\n## Risks and assumptions\n\n- Pending",
        "requirements_delta" => "## Added requirements\n\n### REQ-NEW-001\n\n**Requirement:** Pending.\n\n**Scenario:** Given ..., when ..., then ...\n\n## Modified requirements\n\n- None identified.\n\n## Removed requirements\n\n- None identified.\n\n## Compatibility impact\n\nPending.",
        "design" => "## Current-state evidence\n\nPending.\n\n## Proposed design\n\nPending.\n\n## Data and API impact\n\nPending.\n\n## Security and privacy\n\nPending.\n\n## Failure handling and rollback\n\nPending.",
        _ => "## Delivery tasks\n\n- [ ] Pending implementation task\n\n## Verification\n\n- [ ] Pending test case\n\n## Dependencies\n\n- Pending\n\n## Release and rollback\n\nPending.",
    };
    format!(
        "---\nchange_id: {change_id}\nproject: {}\nprofile: {profile}\nartifact: {kind}\nstatus: draft\nsource_ticket: {}\n---\n\n# {title} - {}\n\n<!-- xnaut:pending -->\n\n## Change context\n\nThis artifact describes proposed behavior only. It does not modify the approved project baseline.\n\n## Approved baseline references\n\n{baseline_links}\n\n{body}\n",
        project.key,
        if source_ticket.is_empty() { "none" } else { source_ticket },
        kind.replace('_', " ")
    )
}

fn create_change_artifacts(
    project: &ProjectRecord,
    change_id: &str,
    title: &str,
    profile: &str,
    source_ticket: &str,
    baseline: &[String],
) -> Result<Vec<ChangeArtifact>, String> {
    let relative_dir = PathBuf::from("Development")
        .join(project_folder_name(project))
        .join("Changes")
        .join(change_id);
    let root = PathBuf::from(crate::vault::vault_init()?).join("work");
    let target = root.join(&relative_dir);
    std::fs::create_dir_all(&target)
        .map_err(|error| format!("failed to create Change artifact folder: {error}"))?;
    let definitions = [
        ("proposal", "01-Proposal.md"),
        ("requirements_delta", "02-Requirements-Delta.md"),
        ("design", "03-Design.md"),
        ("tasks", "04-Tasks.md"),
    ];
    let mut artifacts = Vec::new();
    for (kind, name) in definitions {
        let path = target.join(name);
        if !path.exists() {
            std::fs::write(
                &path,
                artifact_template(
                    project,
                    change_id,
                    title,
                    profile,
                    kind,
                    source_ticket,
                    baseline,
                ),
            )
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        }
        artifacts.push(ChangeArtifact {
            kind: kind.into(),
            vault_ref: format!("work:{}", relative_dir.join(name).to_string_lossy()),
            status: "draft".into(),
            content_hash: file_hash(&path)?,
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
    }
    Ok(artifacts)
}

fn change_workflow() -> crate::loops::WorkflowDefinition {
    use crate::loops::*;
    let port = |id: &str| WorkflowPort {
        id: id.into(),
        data_type: "change".into(),
        required: true,
    };
    let node = |id: &str, kind: NodeKind, inputs: &[&str], outputs: &[&str]| WorkflowNode {
        id: id.into(),
        kind,
        name: id.replace('_', " "),
        inputs: inputs.iter().map(|id| port(id)).collect(),
        outputs: outputs.iter().map(|id| port(id)).collect(),
        config: json!({ "access_preset": "read_only" }),
        permissions: Vec::new(),
        permission_layers: Vec::new(),
        model_policy: None,
        timeout_seconds: Some(300),
        max_retries: 1,
    };
    let edge = |id: &str, from: &str, out: &str, to: &str, input: &str| WorkflowConnection {
        id: id.into(),
        from_node: from.into(),
        from_port: out.into(),
        to_node: to.into(),
        to_port: input.into(),
    };
    let trigger = node("trigger", NodeKind::Trigger, &[], &["change"]);
    let mut baseline = node(
        "inspect_baseline",
        NodeKind::Action,
        &["change"],
        &["success", "error"],
    );
    baseline.permissions = vec![PermissionRule {
        resource: "baseline".into(),
        action: "read".into(),
    }];
    let mut draft = node(
        "draft_artifacts",
        NodeKind::Action,
        &["baseline"],
        &["success", "error"],
    );
    draft.permissions = vec![PermissionRule {
        resource: "change_artifact".into(),
        action: "create".into(),
    }];
    let mut review = node(
        "independent_review",
        NodeKind::Agent,
        &["artifacts"],
        &["approved", "changes_required", "error"],
    );
    review.model_policy = Some(ModelPolicy {
        kind: ModelPolicyKind::Balanced,
        provider: None,
        model: None,
    });
    review.config = json!({ "access_preset": "read_only", "expected_input_tokens": 10000, "expected_output_tokens": 2000 });
    review.max_retries = 10;
    let approval = node(
        "approval",
        NodeKind::HumanApproval,
        &["review"],
        &["approved", "rejected"],
    );
    let output = node("output", NodeKind::Output, &["result"], &[]);
    WorkflowDefinition {
        schema_version: 1, id: "system-openspec-change".into(), version: 1,
        name: "OpenSpec Change".into(), description: "Create and approve an OpenSpec-inspired xNAUT Change without mutating the project baseline.".into(),
        project: None, status: WorkflowStatus::Draft,
        limits: WorkflowLimits { max_duration_seconds: 604800, max_node_executions: 12, max_agent_calls: Some(2), max_tokens: Some(30000), max_cost_usd: Some(5.0), on_budget_exhausted: BudgetExhaustionAction::Pause },
        governance: WorkflowGovernance {
            require_frontier_approval: true, require_independent_review: false, require_delivery_evidence: false, independent_review: None,
            allowed_providers: Vec::new(), permission_layers: vec![PermissionLayer {
                name: "change-management".into(),
                allow: vec![
                    PermissionRule { resource: "baseline".into(), action: "read".into() },
                    PermissionRule { resource: "change_artifact".into(), action: "create".into() },
                ],
                deny: vec![PermissionRule { resource: "baseline".into(), action: "write".into() }],
            }], model_rates: Vec::new(),
        },
        nodes: vec![trigger, baseline, draft, review, approval, output],
        connections: vec![
            edge("c1", "trigger", "change", "inspect_baseline", "change"),
            edge("c2", "inspect_baseline", "success", "draft_artifacts", "baseline"),
            edge("c3", "inspect_baseline", "error", "output", "result"),
            edge("c4", "draft_artifacts", "success", "independent_review", "artifacts"),
            edge("c5", "draft_artifacts", "error", "output", "result"),
            edge("c6", "independent_review", "approved", "approval", "review"),
            edge("c7", "independent_review", "changes_required", "output", "result"),
            edge("c8", "independent_review", "error", "output", "result"),
            edge("c9", "approval", "approved", "output", "result"),
            edge("c10", "approval", "rejected", "output", "result"),
        ],
        presentation: WorkflowPresentation::default(), created_at: String::new(), updated_at: String::new(),
    }
}

fn ensure_change_workflow() -> Result<crate::loops::WorkflowDefinition, String> {
    if let Ok(existing) = crate::loops::loops_workflow_get("system-openspec-change".into(), None) {
        let active = crate::loops::loops_workflow_list(None)?
            .iter()
            .find(|item| item.id == existing.id)
            .and_then(|item| item.active_version);
        if active != Some(existing.version) {
            crate::loops::loops_workflow_activate(existing.id.clone(), existing.version)?;
        }
        return Ok(existing);
    }
    let saved = crate::loops::loops_workflow_save(change_workflow())?;
    crate::loops::loops_workflow_activate(saved.id.clone(), saved.version)?;
    Ok(saved)
}

fn find_ticket_path(repo: &Path, id: &str) -> Result<PathBuf, String> {
    let (project, sequence) = id.split_once('-').ok_or("invalid ticket id")?;
    let project = validate_project_key(project)?;
    if sequence.is_empty() || !sequence.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("invalid ticket id".into());
    }
    let canonical_id = format!("{project}-{sequence}");
    if canonical_id != id {
        return Err("invalid ticket id".into());
    }
    let path = repo
        .join("projects")
        .join(&project)
        .join("tickets")
        .join(format!("{canonical_id}.json"));
    if !path.is_file() {
        return Err(format!("ticket not found: {id}"));
    }
    Ok(path)
}

#[tauri::command]
pub async fn pm_module_status(
    state: State<'_, crate::state::AppState>,
) -> Result<ModuleStatus, String> {
    let settings = state.settings.lock().await;
    Ok(inspect(&settings.project_management))
}

#[tauri::command]
pub async fn pm_module_initialize(
    state: State<'_, crate::state::AppState>,
    request: ModuleSetupRequest,
) -> Result<ModuleStatus, String> {
    let path = resolve_path(&request.repo_path)?;
    let name = validate_name(&request.repo_name)?;
    if !is_initialized_repo(&path) {
        initialize_local_repo_transactional(&path, &name)?;
    }

    let mut remote_url = String::new();
    let mut warning = String::new();
    if request.create_remote {
        let forge_index = request.forge_index.unwrap_or(0);
        let mut host = state
            .settings
            .lock()
            .await
            .forges
            .get(forge_index)
            .cloned()
            .ok_or("forge index out of range")?;
        if let Some(owner) = request
            .forge_owner
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            host.owner = owner.into();
        }
        if !request.personal_owner && host.owner.trim().is_empty() {
            return Err("organization name is required".into());
        }
        if let Some(token) = request
            .forge_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            host.token = Some(token.into());
        }
        remote_url = crate::forges::create_repo_for_owner(
            &host,
            &name,
            true,
            "Private xNaut Project Management control repository",
            request.personal_owner,
        )
        .await?;
        run_git(&path, &["remote", "add", "origin", &remote_url])?;
        if let Err(error) = run_git(&path, &["push", "-u", "origin", "main"]) {
            warning = format!("Repository created, but the first push failed: {error}");
        }
    }

    let mut settings = state.settings.lock().await.clone();
    if request.create_remote {
        if let Some(host) = settings.forges.get_mut(request.forge_index.unwrap_or(0)) {
            if let Some(owner) = request
                .forge_owner
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                host.owner = owner.into();
            }
            if let Some(token) = request
                .forge_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                host.token = Some(token.into());
            }
        }
    }
    settings.project_management = ProjectManagementSettings {
        enabled: true,
        repo_path: path.to_string_lossy().into_owned(),
        remote_url,
    };
    crate::settings::save(&settings)?;
    *state.settings.lock().await = settings.clone();
    let mut status = inspect(&settings.project_management);
    status.warning = warning;
    Ok(status)
}

#[tauri::command]
pub async fn pm_module_connect(
    state: State<'_, crate::state::AppState>,
    request: ModuleConnectRequest,
) -> Result<ModuleStatus, String> {
    let path = resolve_path(&request.repo_path)?;
    if !is_initialized_repo(&path) {
        return Err("folder is not an initialized xNaut Project Management repository".into());
    }
    let mut project_management = ProjectManagementSettings {
        enabled: true,
        repo_path: path.to_string_lossy().into_owned(),
        remote_url: configure_origin(&path, &request.remote_url)?,
    };
    let status = inspect(&project_management);
    if !status.valid {
        return Err(status.error);
    }
    let mut settings = state.settings.lock().await.clone();
    settings.project_management = project_management.clone();
    crate::settings::save(&settings)?;
    *state.settings.lock().await = settings;
    project_management.enabled = true;
    Ok(inspect(&project_management))
}

#[tauri::command]
pub async fn pm_module_sync(
    state: State<'_, crate::state::AppState>,
) -> Result<ModuleStatus, String> {
    let settings = state.settings.lock().await.clone();
    let repo = configured_repo(&settings.project_management)?;
    let origin = run_git(&repo, &["remote", "get-url", "origin"]).unwrap_or_default();
    let auth = authenticated_remote(&settings, &origin);
    sync_repo(
        &repo,
        auth.as_ref()
            .map(|(url, header)| (url.as_str(), header.as_str())),
    )?;
    Ok(inspect(&settings.project_management))
}

#[tauri::command]
pub async fn pm_project_list(
    state: State<'_, crate::state::AppState>,
) -> Result<Vec<ProjectRecord>, String> {
    let settings = state.settings.lock().await.project_management.clone();
    list_projects(&configured_repo(&settings)?)
}

#[tauri::command]
pub async fn pm_project_import_existing(
    state: State<'_, crate::state::AppState>,
) -> Result<Vec<ProjectRecord>, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    import_task_projects(&repo, &crate::tasks::load_tasks())?;
    migrate_legacy_pm_data(
        &repo,
        &crate::pm::load_pm_projects(),
        &crate::project_todos::load_all(),
    )
}

#[tauri::command]
pub async fn pm_project_create(
    state: State<'_, crate::state::AppState>,
    request: ProjectCreateRequest,
) -> Result<ProjectRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let key = validate_project_key(&request.key)?;
    let name = request.name.trim();
    if name.is_empty() {
        return Err("project name is required".into());
    }
    let flow_type = validate_choice(&request.flow_type, "flow type", &["standard", "incident"])?;
    for (label, value) in [
        ("budget", request.budget_chf),
        ("hourly rate", request.hourly_rate_chf),
    ] {
        if value.is_some_and(|amount| !amount.is_finite() || amount < 0.0) {
            return Err(format!("{label} must be a positive number"));
        }
    }
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let project_dir = repo.join("projects").join(&key);
    if project_dir.exists() {
        return Err(format!("project already exists: {key}"));
    }
    std::fs::create_dir_all(project_dir.join("tickets"))
        .map_err(|error| format!("failed to create project: {error}"))?;
    let record = ProjectRecord {
        key: key.clone(),
        name: name.into(),
        purpose: request.purpose.trim().into(),
        owner: request.owner.trim().into(),
        client_name: request.client_name.trim().into(),
        contact_name: request.contact_name.trim().into(),
        contact_email: request.contact_email.trim().into(),
        budget_chf: request.budget_chf,
        hourly_rate_chf: request.hourly_rate_chf,
        stage: if flow_type == "incident" {
            "intake".into()
        } else {
            default_project_stage()
        },
        flow_type,
        revision: default_revision(),
        source_repo: request.source_repo.trim().into(),
        source_path: if Path::new(request.source_repo.trim()).is_absolute() {
            request.source_repo.trim().into()
        } else {
            String::new()
        },
        forge_remote: if request.source_repo.contains("://")
            || request.source_repo.starts_with("git@")
        {
            request.source_repo.trim().into()
        } else {
            String::new()
        },
        task_id: String::new(),
        client: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let manifest = project_dir.join("project.json");
    write_json_atomic(&manifest, &record)?;
    record_mutation(
        &repo,
        "project.created",
        &key,
        json!({ "name": record.name }),
        &[manifest],
        &format!("feat(pm): create project {key}"),
    )?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_project_update(
    state: State<'_, crate::state::AppState>,
    request: ProjectUpdateRequest,
) -> Result<ProjectRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let key = validate_project_key(&request.key)?;
    let name = request.name.trim();
    if name.is_empty() {
        return Err("project name is required".into());
    }
    let flow_type = validate_choice(&request.flow_type, "flow type", &["standard", "incident"])?;
    for (label, value) in [
        ("budget", request.budget_chf),
        ("hourly rate", request.hourly_rate_chf),
    ] {
        if value.is_some_and(|amount| !amount.is_finite() || amount < 0.0) {
            return Err(format!("{label} must be a positive number"));
        }
    }
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let manifest = repo.join("projects").join(&key).join("project.json");
    if !manifest.is_file() {
        return Err(format!("project does not exist: {key}"));
    }
    let mut record: ProjectRecord = read_json(&manifest)?;
    if record.revision != request.expected_revision {
        return Err(format!(
            "project changed since it was loaded: expected revision {}, current revision {}",
            request.expected_revision, record.revision
        ));
    }
    let source_repo = request.source_repo.trim();
    record.name = name.into();
    record.purpose = request.purpose.trim().into();
    record.owner = request.owner.trim().into();
    record.client_name = request.client_name.trim().into();
    record.contact_name = request.contact_name.trim().into();
    record.contact_email = request.contact_email.trim().into();
    record.budget_chf = request.budget_chf;
    record.hourly_rate_chf = request.hourly_rate_chf;
    if record.flow_type != flow_type {
        record.stage = if flow_type == "incident" {
            "intake".into()
        } else {
            default_project_stage()
        };
    }
    if let Some(stage) = request
        .stage
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let allowed: &[&str] = if flow_type == "incident" {
            &[
                "intake",
                "rca",
                "action_plan",
                "ticket",
                "build",
                "test_review",
                "release",
                "learning",
            ]
        } else {
            &[
                "idea",
                "concept",
                "business_case",
                "prd",
                "architecture",
                "data_model",
                "api_design",
                "security_review",
                "development_plan",
                "sprint_stories",
                "tickets",
                "build",
                "test_review",
                "release",
                "learning",
            ]
        };
        record.stage = validate_choice(stage, "project stage", allowed)?;
    }
    record.flow_type = flow_type;
    record.source_repo = source_repo.into();
    record.source_path = if Path::new(source_repo).is_absolute() {
        source_repo.into()
    } else {
        String::new()
    };
    record.forge_remote = if source_repo.contains("://") || source_repo.starts_with("git@") {
        source_repo.into()
    } else {
        String::new()
    };
    record.revision += 1;
    write_json_atomic(&manifest, &record)?;
    record_mutation(
        &repo,
        "project.updated",
        &key,
        json!({ "name": record.name, "revision": record.revision, "stage": record.stage }),
        &[manifest],
        &format!("chore(pm): update project {key}"),
    )?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_change_list(
    state: State<'_, crate::state::AppState>,
    project: String,
) -> Result<Vec<ChangeRecord>, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let project = validate_project_key(&project)?;
    let root = repo.join("projects").join(project).join("changes");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(Vec::new());
    };
    let mut changes = Vec::new();
    for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
        let manifest = entry.path().join("change.json");
        if manifest.is_file() {
            changes.push(read_json(&manifest)?);
        }
    }
    changes.sort_by(|left: &ChangeRecord, right| right.updated_at.cmp(&left.updated_at));
    Ok(changes)
}

#[tauri::command]
pub async fn pm_change_get(
    state: State<'_, crate::state::AppState>,
    project: String,
    change_id: String,
) -> Result<ChangeRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    read_change(&configured_repo(&settings)?, &project, &change_id)
}

#[tauri::command]
pub async fn pm_change_create(
    app: tauri::AppHandle,
    state: State<'_, crate::state::AppState>,
    request: ChangeCreateRequest,
) -> Result<ChangeRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let project_key = validate_project_key(&request.project)?;
    let title = request.title.trim();
    if title.is_empty() {
        return Err("change title is required".into());
    }
    let profile = validate_change_profile(&request.profile)?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let project_path = repo
        .join("projects")
        .join(&project_key)
        .join("project.json");
    if !project_path.is_file() {
        return Err(format!("project does not exist: {project_key}"));
    }
    let project: ProjectRecord = read_json(&project_path)?;
    let change_id = format!(
        "{project_key}-CHG-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..10]
    );
    let change_dir = repo
        .join("projects")
        .join(&project_key)
        .join("changes")
        .join(&change_id);
    std::fs::create_dir_all(change_dir.join("reviews"))
        .map_err(|error| format!("failed to create change folder: {error}"))?;
    let baseline = baseline_refs(&project)?;
    let artifacts = create_change_artifacts(
        &project,
        &change_id,
        title,
        &profile,
        request.source_ticket.trim(),
        &baseline,
    )?;
    let workflow = ensure_change_workflow()?;
    let mut run = crate::loops::loops_run_start(
        app.clone(),
        crate::loops::StartRunRequest {
            workflow_id: workflow.id.clone(),
            workflow_version: Some(workflow.version),
            project: Some(project_key.clone()),
            input: json!({ "change_id": change_id, "profile": profile, "source_ticket": request.source_ticket }),
        },
    )?;
    run = crate::loops::loops_run_claim_node(app.clone(), run.id, "inspect_baseline".into())?;
    run = crate::loops::loops_run_complete_node(
        app.clone(),
        crate::loops::CompleteNodeRequest {
            run_id: run.id,
            node_id: "inspect_baseline".into(),
            output: json!({ "baseline_refs": baseline }),
            outcomes: vec!["success".into()],
            usage: None,
        },
    )?;
    run = crate::loops::loops_run_claim_node(app.clone(), run.id, "draft_artifacts".into())?;
    run = crate::loops::loops_run_complete_node(
        app,
        crate::loops::CompleteNodeRequest {
            run_id: run.id,
            node_id: "draft_artifacts".into(),
            output: json!({ "artifacts": artifacts }),
            outcomes: vec!["success".into()],
            usage: None,
        },
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    let record = ChangeRecord {
        id: change_id.clone(),
        project: project_key.clone(),
        title: title.into(),
        profile,
        status: "draft".into(),
        summary: request.summary.trim().into(),
        source_ticket: request.source_ticket.trim().into(),
        source_url: request.source_url.trim().into(),
        baseline_refs: baseline,
        artifacts,
        agents: request
            .agents
            .into_iter()
            .map(|agent| agent.trim().to_string())
            .filter(|agent| !agent.is_empty())
            .collect(),
        reviews: Vec::new(),
        approvals: Vec::new(),
        workflow_id: workflow.id,
        run_id: run.id,
        evidence: Vec::new(),
        revision: 1,
        created_at: now.clone(),
        updated_at: now,
    };
    let manifest = change_dir.join("change.json");
    write_json_atomic(&manifest, &record)?;
    record_mutation(
        &repo,
        "change.created",
        &change_id,
        json!({ "project": project_key, "profile": record.profile, "run_id": record.run_id }),
        &[manifest],
        &format!("feat(pm): create change {change_id}"),
    )?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_change_refresh(
    state: State<'_, crate::state::AppState>,
    project: String,
    change_id: String,
    expected_revision: u64,
) -> Result<ChangeRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let manifest = change_manifest(&repo, &project, &change_id)?;
    let mut record: ChangeRecord = read_json(&manifest)?;
    if record.revision != expected_revision {
        return Err(format!(
            "change changed since it was loaded: expected revision {expected_revision}, current revision {}",
            record.revision
        ));
    }
    let vault_root = PathBuf::from(crate::vault::vault_init()?).join("work");
    for artifact in &mut record.artifacts {
        let rel = artifact
            .vault_ref
            .strip_prefix("work:")
            .ok_or("invalid Change artifact reference")?;
        let path = crate::vault::safe_join(&vault_root, rel)?;
        let content = std::fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        artifact.content_hash = file_hash(&path)?;
        artifact.status = if content.lines().any(|line| line.trim() == "status: ready") {
            "ready".into()
        } else {
            "draft".into()
        };
        artifact.updated_at = chrono::Utc::now().to_rfc3339();
    }
    record.status = if record.artifacts.iter().all(|item| item.status == "ready") {
        "ready_for_review".into()
    } else {
        "draft".into()
    };
    record.revision += 1;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_json_atomic(&manifest, &record)?;
    record_mutation(
        &repo,
        "change.artifacts_refreshed",
        &record.id,
        json!({ "revision": record.revision, "status": record.status }),
        &[manifest],
        &format!("chore(pm): refresh change {}", record.id),
    )?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_change_set_artifact_status(
    state: State<'_, crate::state::AppState>,
    request: ChangeArtifactStatusRequest,
) -> Result<ChangeRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let manifest = change_manifest(&repo, &request.project, &request.change_id)?;
    let mut record: ChangeRecord = read_json(&manifest)?;
    if record.revision != request.expected_revision {
        return Err("change changed since it was loaded".into());
    }
    if !matches!(
        record.status.as_str(),
        "draft" | "ready_for_review" | "changes_required"
    ) {
        return Err("artifact readiness cannot change after review approval".into());
    }
    let artifact = record
        .artifacts
        .iter_mut()
        .find(|artifact| artifact.kind == request.kind)
        .ok_or("Change artifact not found")?;
    let rel = artifact
        .vault_ref
        .strip_prefix("work:")
        .ok_or("invalid Change artifact reference")?;
    let root = PathBuf::from(crate::vault::vault_init()?).join("work");
    let path = crate::vault::safe_join(&root, rel)?;
    let mut content = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if request.ready {
        if content.matches("Pending").count() >= 2 {
            return Err("complete the pending artifact sections before marking it ready".into());
        }
        content = content.replacen("status: draft", "status: ready", 1);
        content = content.replace("<!-- xnaut:pending -->\n\n", "");
    } else {
        content = content.replacen("status: ready", "status: draft", 1);
        if !content.contains("<!-- xnaut:pending -->") {
            content = content.replacen("\n# ", "\n<!-- xnaut:pending -->\n\n# ", 1);
        }
    }
    std::fs::write(&path, content)
        .map_err(|error| format!("failed to update {}: {error}", path.display()))?;
    artifact.status = if request.ready {
        "ready".into()
    } else {
        "draft".into()
    };
    artifact.content_hash = file_hash(&path)?;
    artifact.updated_at = chrono::Utc::now().to_rfc3339();
    record.status = if record.artifacts.iter().all(|item| item.status == "ready") {
        "ready_for_review".into()
    } else {
        "draft".into()
    };
    record.revision += 1;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_json_atomic(&manifest, &record)?;
    record_mutation(
        &repo,
        "change.artifact_status",
        &record.id,
        json!({ "kind": request.kind, "ready": request.ready, "revision": record.revision }),
        &[manifest],
        &format!("chore(pm): update change artifact {}", record.id),
    )?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_change_review(
    app: tauri::AppHandle,
    state: State<'_, crate::state::AppState>,
    request: ChangeReviewRequest,
) -> Result<ChangeRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let verdict = validate_choice(
        &request.verdict,
        "review verdict",
        &["approved", "changes_required"],
    )?;
    if request.reviewer.trim().is_empty() {
        return Err("reviewer is required".into());
    }
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let manifest = change_manifest(&repo, &request.project, &request.change_id)?;
    let mut record: ChangeRecord = read_json(&manifest)?;
    if record.revision != request.expected_revision {
        return Err("change changed since it was loaded".into());
    }
    if record.status != "ready_for_review" {
        return Err("all Change artifacts must be ready before independent review".into());
    }
    if record
        .agents
        .first()
        .is_some_and(|agent| agent == request.reviewer.trim())
    {
        return Err("independent reviewer must differ from the drafting Agent".into());
    }
    let review = ChangeReview {
        id: uuid::Uuid::new_v4().to_string(),
        reviewer: request.reviewer.trim().into(),
        verdict: verdict.clone(),
        summary: request.summary.trim().into(),
        findings: request
            .findings
            .into_iter()
            .map(|finding| finding.trim().to_string())
            .filter(|finding| !finding.is_empty())
            .take(100)
            .collect(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let review_path = manifest
        .parent()
        .ok_or("invalid change manifest")?
        .join("reviews")
        .join(format!("{}.json", review.id));
    write_json_atomic(&review_path, &review)?;
    let mut run = crate::loops::loops_run_claim_node(
        app.clone(),
        record.run_id.clone(),
        "independent_review".into(),
    )?;
    if verdict == "approved" {
        run = crate::loops::loops_run_complete_node(
            app.clone(),
            crate::loops::CompleteNodeRequest {
                run_id: run.id,
                node_id: "independent_review".into(),
                output: serde_json::to_value(&review).map_err(|error| error.to_string())?,
                outcomes: vec![verdict.clone()],
                usage: None,
            },
        )?;
        run = crate::loops::loops_run_claim_node(app, run.id, "approval".into())?;
    } else {
        run = crate::loops::loops_run_fail_node(
            app,
            crate::loops::FailNodeRequest {
                run_id: run.id,
                node_id: "independent_review".into(),
                error: "independent review requires artifact changes".into(),
            },
        )?;
    }
    record.reviews.push(review);
    record.status = if verdict == "approved" {
        "awaiting_approval".into()
    } else {
        "changes_required".into()
    };
    record.revision += 1;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_json_atomic(&manifest, &record)?;
    record_mutation(
        &repo,
        "change.reviewed",
        &record.id,
        json!({ "revision": record.revision, "verdict": verdict, "reviewer": request.reviewer, "run_status": run.status }),
        &[manifest, review_path],
        &format!("chore(pm): review change {}", record.id),
    )?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_change_approve(
    app: tauri::AppHandle,
    state: State<'_, crate::state::AppState>,
    request: ChangeApprovalRequest,
) -> Result<ChangeRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    if request.actor.trim().is_empty() {
        return Err("approval actor is required".into());
    }
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let manifest = change_manifest(&repo, &request.project, &request.change_id)?;
    let mut record: ChangeRecord = read_json(&manifest)?;
    if record.revision != request.expected_revision {
        return Err("change changed since it was loaded".into());
    }
    if record.status != "awaiting_approval"
        || record
            .reviews
            .last()
            .is_none_or(|review| review.verdict != "approved")
    {
        return Err("approved independent review is required before human approval".into());
    }
    let approval = ChangeApproval {
        actor: request.actor.trim().into(),
        approved: request.approved,
        comment: request.comment.trim().into(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let mut run = crate::loops::loops_run_approve(
        app.clone(),
        crate::loops::ApprovalRequest {
            run_id: record.run_id.clone(),
            node_id: "approval".into(),
            actor: approval.actor.clone(),
            approved: approval.approved,
            comment: approval.comment.clone(),
        },
    )?;
    if request.approved
        && run
            .nodes
            .get("output")
            .is_some_and(|node| node.status == crate::loops::NodeRunStatus::Ready)
    {
        run = crate::loops::loops_run_claim_node(app.clone(), run.id, "output".into())?;
        run = crate::loops::loops_run_complete_node(
            app,
            crate::loops::CompleteNodeRequest {
                run_id: run.id,
                node_id: "output".into(),
                output: json!({ "change_id": record.id, "execution_ready": true }),
                outcomes: Vec::new(),
                usage: None,
            },
        )?;
    }
    record.approvals.push(approval);
    record.status = if request.approved {
        "execution_ready".into()
    } else {
        "changes_required".into()
    };
    record.revision += 1;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_json_atomic(&manifest, &record)?;
    record_mutation(
        &repo,
        if request.approved {
            "change.approved"
        } else {
            "change.rejected"
        },
        &record.id,
        json!({ "revision": record.revision, "actor": request.actor, "run_status": run.status }),
        &[manifest],
        &format!("chore(pm): decide change {}", record.id),
    )?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_ticket_list(
    state: State<'_, crate::state::AppState>,
    project: Option<String>,
) -> Result<Vec<TicketRecord>, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let roots: Vec<PathBuf> = if let Some(project) = project {
        vec![repo
            .join("projects")
            .join(validate_project_key(&project)?)
            .join("tickets")]
    } else {
        list_projects(&repo)?
            .into_iter()
            .map(|item| repo.join("projects").join(item.key).join("tickets"))
            .collect()
    };
    let mut tickets = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|value| value.to_str()) == Some("json") {
                tickets.push(read_json(&entry.path())?);
            }
        }
    }
    tickets.sort_by(|a: &TicketRecord, b: &TicketRecord| b.updated_at.cmp(&a.updated_at));
    Ok(tickets)
}

#[tauri::command]
pub async fn pm_event_list(
    state: State<'_, crate::state::AppState>,
    subject: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<EventRecord>, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    list_events(&repo, subject.as_deref(), limit.unwrap_or(100))
}

#[tauri::command]
pub async fn pm_ticket_create(
    state: State<'_, crate::state::AppState>,
    request: TicketCreateRequest,
) -> Result<TicketRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let key = validate_project_key(&request.project)?;
    let title = request.title.trim();
    if title.is_empty() {
        return Err("ticket title is required".into());
    }
    let ticket_type = validate_choice(
        &request.ticket_type,
        "ticket type",
        &["idea", "feature", "bug", "incident", "task"],
    )?;
    let status = validate_choice(
        &request.status,
        "status",
        &["inbox", "ready", "in_progress", "review", "blocked", "done"],
    )?;
    let priority = validate_choice(
        &request.priority,
        "priority",
        &["low", "medium", "high", "critical"],
    )?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let tickets_dir = repo.join("projects").join(&key).join("tickets");
    if !tickets_dir.is_dir() {
        return Err(format!("project not found: {key}"));
    }
    let next = next_ticket_sequence(&tickets_dir)?;
    let id = format!("{key}-{next}");
    let now = chrono::Utc::now().to_rfc3339();
    let record = TicketRecord {
        id: id.clone(),
        project: key,
        title: title.into(),
        ticket_type,
        status,
        priority,
        owner: request.owner.filter(|value| !value.trim().is_empty()),
        documentation: request.documentation,
        body: request.body,
        source_id: String::new(),
        revision: 1,
        created_at: now.clone(),
        updated_at: now,
    };
    let path = tickets_dir.join(format!("{id}.json"));
    write_json_atomic(&path, &record)?;
    record_mutation(
        &repo,
        "ticket.created",
        &id,
        json!({ "status": record.status, "type": record.ticket_type }),
        &[path],
        &format!("feat(pm): create {id}"),
    )?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_ticket_update(
    state: State<'_, crate::state::AppState>,
    request: TicketUpdateRequest,
) -> Result<TicketRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let path = find_ticket_path(&repo, &request.id)?;
    let mut record: TicketRecord = read_json(&path)?;
    if record.revision != request.expected_revision {
        return Err(format!(
            "ticket changed since it was loaded: expected revision {}, current revision {}",
            request.expected_revision, record.revision
        ));
    }
    if let Some(title) = request.title {
        if title.trim().is_empty() {
            return Err("ticket title is required".into());
        }
        record.title = title.trim().into();
    }
    if let Some(ticket_type) = request.ticket_type {
        record.ticket_type = validate_choice(
            &ticket_type,
            "ticket type",
            &["idea", "feature", "bug", "incident", "task"],
        )?;
    }
    if let Some(status) = request.status {
        record.status = validate_choice(
            &status,
            "status",
            &["inbox", "ready", "in_progress", "review", "blocked", "done"],
        )?;
    }
    if let Some(priority) = request.priority {
        record.priority = validate_choice(
            &priority,
            "priority",
            &["low", "medium", "high", "critical"],
        )?;
    }
    if request.clear_owner {
        record.owner = None;
    } else if let Some(owner) = request.owner {
        record.owner = owner.filter(|value| !value.trim().is_empty());
    }
    if let Some(documentation) = request.documentation {
        record.documentation = documentation;
    }
    if let Some(body) = request.body {
        record.body = body;
    }
    record.revision += 1;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_json_atomic(&path, &record)?;
    record_mutation(
        &repo,
        "ticket.updated",
        &record.id,
        json!({ "status": record.status, "revision": record.revision }),
        &[path],
        &format!("chore(pm): update {}", record.id),
    )?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_ticket_delete(
    state: State<'_, crate::state::AppState>,
    id: String,
    expected_revision: u64,
) -> Result<(), String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let path = find_ticket_path(&repo, &id)?;
    let record: TicketRecord = read_json(&path)?;
    if record.revision != expected_revision {
        return Err(format!(
            "ticket changed since it was loaded: expected revision {expected_revision}, current revision {}",
            record.revision
        ));
    }
    std::fs::remove_file(&path).map_err(|error| format!("failed to delete {id}: {error}"))?;
    record_mutation(
        &repo,
        "ticket.deleted",
        &id,
        json!({ "title": record.title, "revision": record.revision }),
        &[path],
        &format!("chore(pm): delete {id}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_repository_names() {
        assert_eq!(validate_name("xnaut-control").unwrap(), "xnaut-control");
        assert!(validate_name("../escape").is_err());
        assert!(validate_name("has spaces").is_err());
    }

    #[test]
    fn default_module_is_disabled_and_unconfigured() {
        let status = inspect(&ProjectManagementSettings::default());
        assert!(!status.enabled);
        assert!(!status.configured);
        assert!(!status.valid);
    }

    #[test]
    fn ticket_schema_has_versioned_required_fields() {
        let schema = ticket_schema();
        assert_eq!(schema["title"], "xNaut Project Ticket");
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "revision"));
    }

    #[test]
    fn legacy_project_manifests_default_to_the_idea_stage() {
        let project: ProjectRecord = serde_json::from_value(json!({
            "key": "XNAUT",
            "name": "xNaut",
            "created_at": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        assert_eq!(project.flow_type, "standard");
        assert_eq!(project.stage, "idea");
        assert_eq!(project.revision, 1);
        assert!(project.purpose.is_empty());
        assert!(project.budget_chf.is_none());
    }

    #[test]
    fn initializes_a_valid_git_backed_module() {
        let root = std::env::temp_dir().join(format!("xnaut-pm-test-{}", uuid::Uuid::new_v4()));
        let repo = root.join("control");
        initialize_local_repo_transactional(&repo, "control").unwrap();
        let status = inspect(&ProjectManagementSettings {
            enabled: true,
            repo_path: repo.to_string_lossy().into_owned(),
            remote_url: String::new(),
        });
        assert!(status.valid);
        assert_eq!(
            run_git(&repo, &["rev-list", "--count", "HEAD"]).unwrap(),
            "1"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn records_mutations_without_committing_unrelated_files() {
        let root = std::env::temp_dir().join(format!("xnaut-pm-test-{}", uuid::Uuid::new_v4()));
        let repo = root.join("control");
        initialize_local_repo_transactional(&repo, "control").unwrap();
        std::fs::write(repo.join("unrelated.txt"), "leave me alone").unwrap();
        let project = repo.join("projects/TEST/project.json");
        std::fs::create_dir_all(project.parent().unwrap()).unwrap();
        write_json_atomic(&project, &json!({ "key": "TEST" })).unwrap();
        record_mutation(
            &repo,
            "project.created",
            "TEST",
            json!({}),
            &[project],
            "test mutation",
        )
        .unwrap();
        assert_eq!(
            run_git(&repo, &["rev-list", "--count", "HEAD"]).unwrap(),
            "2"
        );
        assert!(run_git(&repo, &["status", "--short"])
            .unwrap()
            .contains("unrelated.txt"));
        let events = list_events(&repo, Some("TEST"), 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "project.created");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn connects_an_existing_local_module_to_an_existing_remote() {
        let root = std::env::temp_dir().join(format!("xnaut-pm-test-{}", uuid::Uuid::new_v4()));
        let repo = root.join("control");
        initialize_local_repo_transactional(&repo, "control").unwrap();
        let remote = "ssh://git@example.test:2222/team/xnaut-control.git";
        assert_eq!(configure_origin(&repo, remote).unwrap(), remote);
        assert_eq!(
            run_git(&repo, &["remote", "get-url", "origin"]).unwrap(),
            remote
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn syncs_to_an_empty_remote_on_first_push() {
        let root = std::env::temp_dir().join(format!("xnaut-pm-test-{}", uuid::Uuid::new_v4()));
        let repo = root.join("control");
        let remote = root.join("remote.git");
        initialize_local_repo_transactional(&repo, "control").unwrap();
        let output = Command::new("git")
            .args(["init", "--bare", remote.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(output.status.success());
        configure_origin(&repo, remote.to_str().unwrap()).unwrap();
        sync_repo(&repo, None).unwrap();
        assert_eq!(
            run_git(&repo, &["rev-list", "--count", "@{upstream}"]).unwrap(),
            "1"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn imports_projects_and_migrates_legacy_data_idempotently() {
        let root = std::env::temp_dir().join(format!("xnaut-pm-test-{}", uuid::Uuid::new_v4()));
        let repo = root.join("control");
        initialize_local_repo_transactional(&repo, "control").unwrap();
        let task = crate::tasks::TaskSession {
            id: "task-ayus".into(),
            name: "Ayus".into(),
            kind: "project".into(),
            path: "/tmp/Ayus".into(),
            zellij_session: String::new(),
            agent_id: None,
            created: "2026-01-01T00:00:00Z".into(),
            project_type: None,
            forge_remote: None,
        };
        let manual_dir = repo.join("projects/AYUS");
        std::fs::create_dir_all(manual_dir.join("tickets")).unwrap();
        write_json_atomic(
            &manual_dir.join("project.json"),
            &ProjectRecord {
                key: "AYUS".into(),
                name: "Ayus".into(),
                purpose: String::new(),
                owner: String::new(),
                client_name: String::new(),
                contact_name: String::new(),
                contact_email: String::new(),
                budget_chf: None,
                hourly_rate_chf: None,
                flow_type: default_flow_type(),
                stage: default_project_stage(),
                revision: default_revision(),
                source_repo: String::new(),
                source_path: String::new(),
                forge_remote: String::new(),
                task_id: String::new(),
                client: None,
                created_at: "2026-01-01T00:00:00Z".into(),
            },
        )
        .unwrap();
        let imported = import_task_projects(&repo, &[task]).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].task_id, "task-ayus");
        assert_eq!(imported[0].source_path, "/tmp/Ayus");
        let client = crate::pm::ExternalProject {
            id: "client-ayus".into(),
            task_id: "old-ayus-id".into(),
            client_company: "Ayus Medical Group AG".into(),
            contacts: Vec::new(),
            scope: "Pilot".into(),
            rate_chf_per_hour: 180.0,
            offer_amount_chf: Some(0.0),
            expected_close: None,
            plow_opportunity_id: None,
            lineary_project_id: None,
            created: "2026-01-01T00:00:00Z".into(),
        };
        let todos = std::collections::HashMap::from([
            (
                "task-ayus".into(),
                vec![crate::project_todos::Todo {
                    id: "todo-ayus".into(),
                    text: "Review pilot".into(),
                    done: false,
                    created: "2026-01-02T00:00:00Z".into(),
                }],
            ),
            (
                "removed-task".into(),
                vec![crate::project_todos::Todo {
                    id: "todo-legacy".into(),
                    text: "Preserve removed work".into(),
                    done: true,
                    created: "2026-01-03T00:00:00Z".into(),
                }],
            ),
        ]);
        let projects = migrate_legacy_pm_data(&repo, &[client], &todos).unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects
            .iter()
            .any(|project| project.key == "AYUS" && project.client.is_some()));
        assert!(projects.iter().any(|project| project.key == "LEGACY"));
        assert_eq!(
            count_files(&repo.join("projects"), ".json") - projects.len(),
            2
        );
        let commits = run_git(&repo, &["rev-list", "--count", "HEAD"]).unwrap();
        migrate_legacy_pm_data(&repo, &[], &todos).unwrap();
        assert_eq!(
            run_git(&repo, &["rev-list", "--count", "HEAD"]).unwrap(),
            commits
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn openspec_change_workflow_passes_authoritative_audit() {
        let workflow = change_workflow();
        let report = crate::loops::audit_definition(&workflow);
        assert!(report.valid, "{:?}", report.findings);
        assert!(workflow
            .governance
            .permission_layers
            .iter()
            .flat_map(|layer| &layer.deny)
            .any(|rule| rule.resource == "baseline" && rule.action == "write"));
    }

    #[test]
    fn change_artifacts_have_stable_refs_and_start_as_drafts() {
        let unique = uuid::Uuid::new_v4().simple().to_string();
        let project = ProjectRecord {
            key: "TEST".into(),
            name: format!("Change Test {unique}"),
            purpose: String::new(),
            owner: String::new(),
            client_name: String::new(),
            contact_name: String::new(),
            contact_email: String::new(),
            budget_chf: None,
            hourly_rate_chf: None,
            flow_type: default_flow_type(),
            stage: default_project_stage(),
            revision: 1,
            source_repo: String::new(),
            source_path: String::new(),
            forge_remote: String::new(),
            task_id: String::new(),
            client: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let change_id = format!("TEST-CHG-{unique}");
        let artifacts = create_change_artifacts(
            &project,
            &change_id,
            "Test change",
            "feature",
            "TEST-1",
            &["work:Development/Test/NAUT-Flow/02-Concept.md".into()],
        )
        .unwrap();
        assert_eq!(artifacts.len(), 4);
        assert!(artifacts.iter().all(|artifact| artifact.status == "draft"));
        assert!(artifacts
            .iter()
            .all(|artifact| artifact.vault_ref.contains(&change_id)));
        let root = PathBuf::from(crate::vault::vault_init().unwrap())
            .join("work")
            .join("Development")
            .join(project_folder_name(&project));
        std::fs::remove_dir_all(root).unwrap();
    }
}
