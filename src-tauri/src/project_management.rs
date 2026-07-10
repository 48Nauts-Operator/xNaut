use crate::settings::ProjectManagementSettings;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub source_repo: String,
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
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectCreateRequest {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub source_repo: String,
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
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub owner: Option<Option<String>>,
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
        status.ticket_count = count_files(&path.join("projects"), ".md");
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
    initialize_local_repo_transactional(&path, &name)?;

    let mut remote_url = String::new();
    let mut warning = String::new();
    if request.create_remote {
        let forge_index = request.forge_index.unwrap_or(0);
        let host = state
            .settings
            .lock()
            .await
            .forges
            .get(forge_index)
            .cloned()
            .ok_or("forge index out of range")?;
        remote_url = crate::forges::create_repo(
            &host,
            &name,
            true,
            "Private xNaut Project Management control repository",
        )
        .await?;
        run_git(&path, &["remote", "add", "origin", &remote_url])?;
        if let Err(error) = run_git(&path, &["push", "-u", "origin", "main"]) {
            warning = format!("Repository created, but the first push failed: {error}");
        }
    }

    let mut settings = state.settings.lock().await.clone();
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
    repo_path: String,
) -> Result<ModuleStatus, String> {
    let path = resolve_path(&repo_path)?;
    let mut project_management = ProjectManagementSettings {
        enabled: true,
        repo_path: path.to_string_lossy().into_owned(),
        remote_url: run_git(&path, &["remote", "get-url", "origin"]).unwrap_or_default(),
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
pub async fn pm_project_list(
    state: State<'_, crate::state::AppState>,
) -> Result<Vec<ProjectRecord>, String> {
    let settings = state.settings.lock().await.project_management.clone();
    list_projects(&configured_repo(&settings)?)
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
        source_repo: request.source_repo.trim().into(),
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
    let next = std::fs::read_dir(&tickets_dir)
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
        + 1;
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
    if let Some(owner) = request.owner {
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
        std::fs::remove_dir_all(root).unwrap();
    }
}
