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

/// Accepts an explicit JSON `null` as the field's default.
///
/// `#[serde(default)]` only covers a MISSING key — a hand-authored
/// `"task_id": null` (DATFLOW, 2026-07-28) made the whole manifest
/// unparseable, which blanked the entire Projects board.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    #[serde(default)]
    pub owner_only: bool,
    pub key: String,
    pub name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub purpose: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub owner: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub client_name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub contact_name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub contact_email: String,
    #[serde(default)]
    pub budget_chf: Option<f64>,
    #[serde(default)]
    pub hourly_rate_chf: Option<f64>,
    #[serde(default = "default_flow_type")]
    pub flow_type: String,
    // Empty means the project is not in NautFlow, which is a legitimate and
    // common state rather than a missing value (XNAUT-87). A manifest written
    // before this field existed, or by an import, genuinely has no stage — and
    // defaulting it to "idea" told a shipped product it was still writing its
    // requirements. `null_as_default` so an explicit null reads as absent too;
    // the field is a String, and #[serde(default)] alone does not cover null.
    #[serde(default, deserialize_with = "null_as_default")]
    pub stage: String,
    #[serde(default = "default_revision")]
    pub revision: u64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub source_repo: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub source_path: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub forge_remote: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub task_id: String,
    /// May the fleet work this project on its own: triage its unowned tickets,
    /// wake owners, dispatch? Off by default. On the morning of 2026-09-06 the
    /// first triage wake handed out July tickets from Engram, Plough,
    /// NautTutor and NautGate that nobody had pointed the fleet at.
    #[serde(default)]
    pub fleet: bool,
    #[serde(default)]
    pub client: Option<crate::pm::ExternalProject>,
    /// Whether this project takes issues in from its forge or Linear, and on
    /// what trigger (XNAUT-382). Skipped when it is the default so switching
    /// nothing on writes nothing: every project record is a file in git, and
    /// thirty `"issue_intake": {"enabled": false, …}` blocks are thirty diffs
    /// that say nothing.
    #[serde(default, skip_serializing_if = "crate::issue_intake::IssueIntake::is_default")]
    pub issue_intake: crate::issue_intake::IssueIntake,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketRecord {
    /// Empty disables model-triggered swaps. Otherwise an explicit model identity.
    #[serde(default)]
    pub model_requirement: String,
    #[serde(flatten, default)]
    pub approval: crate::jury::TicketApproval,
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
    /// Free-form labels, the owner's to set (André, 2026-09-08).
    #[serde(default)]
    pub tags: Vec<String>,
    /// The release this ticket ships in, e.g. "1.26.3". Empty is Unassigned.
    /// Stamped at integration from the merged tree's Cargo.toml when empty;
    /// every change emits `ticket.release` with the previous and new value.
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub source_id: String,
    /// The structured handback the finishing agent filed, if it filed one.
    ///
    /// `skip_serializing_if` so a ticket that never had one does not grow an
    /// empty key: every ticket JSON in the control repo is a file in git, and
    /// a null on 300 tickets is 300 lines of diff saying nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handback: Option<crate::handback::Handback>,
    /// The ticket this one was carved out of, when an agent divided its own
    /// work (XNAUT-316). Absent on everything a person wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
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
    #[serde(default)]
    pub forge_remote: Option<String>,
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
    pub forge_remote: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TicketCreateRequest {
    /// Empty disables model-triggered swaps. Otherwise an explicit model identity.
    #[serde(default)]
    pub model_requirement: String,
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
    /// Set only when an agent carves a child out of its ticket (XNAUT-316).
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// What this ticket was made FROM, when something made it: a legacy todo
    /// id, or an issue on somebody else's tracker (XNAUT-382). Empty for
    /// everything a person typed. It is the guard that stops the same issue
    /// becoming two tickets, so it is set at creation or never.
    #[serde(default)]
    pub source_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TicketUpdateRequest {
    /// Empty disables model-triggered swaps. Otherwise an explicit model identity.
    #[serde(default)]
    pub model_requirement: Option<String>,
    pub id: String,
    pub expected_revision: u64,
    /// Who is making this write, as an agent handle. XNAUT-243: the rails
    /// used to live in the CHAT tool only, so an agent writing through the
    /// MCP tool (which is the path a RUN actually uses) got no handback and
    /// no complete guard. They live here now, at the one write both callers
    /// pass through, and every caller states who it is. `None` means an
    /// unattributed write: the app's own UI, which is the owner operating
    /// their own board.
    #[serde(default)]
    pub caller: Option<String>,
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

/// The stage keys one flow's track is made of. `incident` runs its own track;
/// `feature` is the standard track minus the stages a feature already has
/// answers for, so it is a subset and validates against the same list.
fn stage_keys(flow_type: &str) -> &'static [&'static str] {
    if flow_type == "incident" {
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
    }
}

/// The stage a project carries after an update (XNAUT-87).
///
/// Split out of `pm_project_update` because three cases were tangled in one
/// `if let` chain and only two of them worked. The third — leaving NautFlow —
/// was unreachable: an empty requested stage was filtered away as "no change",
/// so a project could enter a flow and never get out, and the overview then had
/// no honest state to render. Being in no flow is a legitimate state, so it has
/// to be both representable and reachable.
///
/// `flow_changed` re-bases the stage onto the new flow's track, because a stage
/// key belongs to exactly one track — but only for a project that is actually
/// ON a track. Changing an unstaged project's flow type says which track it
/// WOULD run, not that it has started running one.
fn next_project_stage(
    current: &str,
    requested: Option<&str>,
    flow_type: &str,
    flow_changed: bool,
) -> Result<String, String> {
    let rebased = if flow_changed && !current.is_empty() {
        if flow_type == "incident" {
            "intake".to_string()
        } else {
            default_project_stage()
        }
    } else {
        current.to_string()
    };
    match requested.map(str::trim) {
        None => Ok(rebased),
        Some("") => Ok(String::new()),
        Some(stage) => validate_choice(stage, "project stage", stage_keys(flow_type)),
    }
}

fn default_revision() -> u64 {
    1
}

/// Git flags every call into the control repo carries. The app decides when
/// maintenance runs; git never does on its own (XNAUT-432). Left to itself,
/// git weighs `gc --auto` after every commit and rebase, and at seven to
/// fifteen commits a minute it said yes over and over: André measured 116
/// git processes at 786% CPU, all repacking the same repository, and the
/// aborted packs they left behind are the likeliest source of the week's
/// missing objects (XNAUT-430).
const NO_AUTO_MAINTENANCE: [&str; 4] = ["-c", "gc.auto=0", "-c", "maintenance.auto=false"];

fn git_command(repo: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(NO_AUTO_MAINTENANCE);
    cmd
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_command(repo)
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
    let output = git_command(repo)
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

// ---- control repo maintenance (XNAUT-432) ---------------------------------
//
// One bounded task, one core, every twenty minutes, under the same lock the
// ticket writes take. Sequence by construction: the sweep is the only caller,
// the interval is checked before a task starts, and the lock means a task can
// never overlap a write or another task. "Smaller bits over a longer time"
// (André, 2026-09-21) instead of eight cores at once.

/// The rotation. Each task is bounded by git's own design: loose-objects packs
/// at most fifty thousand loose objects, commit-graph is incremental,
/// incremental-repack merges only the small packs, pack-refs is cheap.
pub(crate) const MAINTENANCE_TASKS: [&str; 4] =
    ["loose-objects", "commit-graph", "incremental-repack", "pack-refs"];
pub(crate) const MAINTENANCE_EVERY: std::time::Duration = std::time::Duration::from_secs(20 * 60);
/// A `tmp_pack_*` older than this is a pack-objects that died; git never
/// reclaims those by itself.
const STALE_TMP_PACK_SECS: u64 = 3600;

struct MaintenanceState {
    last: Option<std::time::Instant>,
    next_task: usize,
}

fn maintenance_state() -> &'static Mutex<MaintenanceState> {
    static STATE: OnceLock<Mutex<MaintenanceState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(MaintenanceState { last: None, next_task: 0 }))
}

/// Pure, so the spacing is testable without a clock.
pub(crate) fn maintenance_due(
    last: Option<std::time::Instant>,
    now: std::time::Instant,
    every: std::time::Duration,
) -> bool {
    match last {
        None => true,
        Some(then) => now.duration_since(then) >= every,
    }
}

/// Called every sweep tick. Returns the task it ran, or `None` when nothing
/// was due. The interval is claimed BEFORE the task runs, so a slow task and
/// the next tick cannot start a second one.
pub fn maintain_control_repo(repo: &Path) -> Result<Option<&'static str>, String> {
    let task = {
        let mut state = maintenance_state()
            .lock()
            .map_err(|_| "control repo maintenance state is unavailable")?;
        let now = std::time::Instant::now();
        if !maintenance_due(state.last, now, MAINTENANCE_EVERY) {
            return Ok(None);
        }
        state.last = Some(now);
        let task = MAINTENANCE_TASKS[state.next_task % MAINTENANCE_TASKS.len()];
        state.next_task = (state.next_task + 1) % MAINTENANCE_TASKS.len();
        task
    };
    run_maintenance_task(repo, task)?;
    Ok(Some(task))
}

/// One task, now, under the mutation lock. Pins the repo's own config first
/// so git run by hand in that repo cannot restart the storm either, and
/// clears the debris of the last one.
pub(crate) fn run_maintenance_task(repo: &Path, task: &str) -> Result<(), String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let started = std::time::Instant::now();
    run_git(repo, &["config", "gc.auto", "0"])?;
    run_git(repo, &["config", "maintenance.auto", "false"])?;
    let cleared = sweep_stale_tmp_packs(repo);
    let task_arg = format!("--task={task}");
    run_git(
        repo,
        &["-c", "pack.threads=1", "-c", "core.multiPackIndex=true", "maintenance", "run", "--quiet", &task_arg],
    )?;
    let _ = crate::debug_log::debug_log_append(vec![format!(
        "[pm] control repo maintenance: {task} in {} ms, {cleared} stale temp pack(s) removed",
        started.elapsed().as_millis()
    )]);
    Ok(())
}

fn sweep_stale_tmp_packs(repo: &Path) -> usize {
    let Ok(rel) = run_git(repo, &["rev-parse", "--git-path", "objects/pack"]) else { return 0 };
    let dir = if Path::new(&rel).is_absolute() { PathBuf::from(&rel) } else { repo.join(&rel) };
    let Ok(entries) = std::fs::read_dir(&dir) else { return 0 };
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("tmp_pack_") {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .map(|age| age.as_secs() >= STALE_TMP_PACK_SECS)
            .unwrap_or(false);
        if old && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
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
            "type": { "enum": TICKET_TYPES },
            "status": { "enum": TICKET_STATUSES },
            "priority": { "enum": ["low", "medium", "high", "critical"] },
            "owner": { "type": ["string", "null"] },
            "documentation": { "type": "array", "items": { "type": "string" } },
            "body": { "type": "string" },
            "source_id": { "type": "string" },
            "revision": { "type": "integer", "minimum": 1 },
            "created_at": { "type": "string" },
            "updated_at": { "type": "string" },
            "handback": { "type": ["object", "null"] }
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
    if !dirty_paths(repo)?.is_empty() {
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

pub fn configured_repo(settings: &ProjectManagementSettings) -> Result<PathBuf, String> {
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

pub(crate) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let written = std::fs::write(&tmp, bytes)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
        .and_then(|_| {
            std::fs::rename(&tmp, path)
                .map_err(|error| format!("failed to replace {}: {error}", path.display()))
        });
    // A write that failed halfway leaves its temp file behind, and that file
    // is an untracked path in a git repository. On tron the disk filled at
    // 17:55 on 2026-09-09, one such file survived, and every board write from
    // that machine was refused as "uncommitted changes" for the next 26 hours.
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// How long an atomic-write temp file may exist before it is certainly
/// abandoned. A live write lasts milliseconds; nothing legitimate is this old.
const ABANDONED_TMP_SECS: u64 = 30;

/// `git status --porcelain`, after removing the abandoned temp files that
/// `write_json_atomic` can leave when a write fails halfway. Only untracked
/// paths named `<something>.tmp-<uuid>` and older than `ABANDONED_TMP_SECS`
/// are touched; everything else is reported exactly as git reports it.
/// Does the working tree hold dirt that would ride along with a write to
/// `ticket`? Only that ticket's own file counts. Every mutation commits with
/// `--only <its own paths>`, so another ticket's half-written edit or the
/// event files of a write whose commit lost a race cannot be swept in.
///
/// Refusing on any dirt at all stopped the entire board instead. On
/// 2026-09-15 one uncommitted XNAUT-274.json and a handful of stray events
/// made every ticket write fail; a green sandbox verify could not move
/// CHESSTRAINER-5 to `complete`, no sign-off started, and the reason went to
/// stderr where nobody reads it (XNAUT-412).
fn dirt_blocks(repo: &Path, ticket: &str) -> Result<bool, String> {
    let Ok(path) = find_ticket_path(repo, ticket) else {
        return Ok(false);
    };
    let Ok(rel) = path.strip_prefix(repo) else {
        return Ok(false);
    };
    let rel = rel.to_string_lossy().into_owned();
    // `run_git` trims its output, so the first porcelain line has lost its
    // leading space and a fixed 3-character offset reads one char short.
    // The path is the last field on the line either way.
    Ok(dirty_paths(repo)?
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .any(|p| p.trim_matches('"') == rel))
}

fn dirty_paths(repo: &Path) -> Result<String, String> {
    let status = run_git(repo, &["status", "--porcelain", "--untracked-files=all"])?;
    let mut pruned = false;
    for line in status.lines() {
        let Some(rel) = line.strip_prefix("?? ") else { continue };
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if !name.contains(".tmp-") {
            continue;
        }
        let full = repo.join(rel.trim_end_matches('/'));
        let old_enough = full
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() >= ABANDONED_TMP_SECS);
        if old_enough && std::fs::remove_file(&full).is_ok() {
            pruned = true;
        }
    }
    if pruned {
        run_git(repo, &["status", "--porcelain", "--untracked-files=all"])
    } else {
        Ok(status)
    }
}

pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid {}: {error}", path.display()))
}

pub(crate) fn record_mutation(
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
    publish(repo);
    Ok(())
}

/// Push what was just committed, so a fleet write is on the remote before the
/// next machine reads. Until 2026-09-07 nothing here pushed at all; only the
/// manual `pm_module_sync` did. Every agent write therefore piled up locally
/// on the machine that made it, and the next foreign write collided with the
/// pile: fifteen unpushed commits on tron in one evening, one handback lost to
/// the conflict, one ticket close and two dispatches refused. XNAUT-297 made
/// the READ side rebase first; this is the write side.
///
/// Best effort on purpose. The commit already exists; a remote that is down
/// or ahead must not turn a successful write into an error the agent retries.
/// A rejected push is left for the next write's rebase, which is exactly the
/// case XNAUT-297 handles, and it is logged so it is never silent.
fn publish(repo: &Path) {
    let Ok(remotes) = run_git(repo, &["remote"]) else { return };
    if !remotes.lines().any(|remote| remote == "origin") {
        return;
    }
    let Ok(branch) = run_git(repo, &["symbolic-ref", "--short", "HEAD"]) else { return };
    if let Err(error) = run_git(repo, &["push", "origin", &branch]) {
        let _ = crate::debug_log::debug_log_append(vec![format!(
            "[pm] push of {branch} deferred to the next write: {}",
            error.lines().last().unwrap_or(&error)
        )]);
    }
}

/// Append a Vault document mutation to the project event trail (XNAUT-14).
/// Best-effort on purpose: the vault write has already happened, so a control
/// repo that is missing, disabled or busy must not turn a successful write into
/// an error the agent will retry.
pub(crate) async fn record_document_event(
    state: State<'_, crate::state::AppState>,
    event: &str,
    subject: &str,
    details: Value,
) {
    let settings = state.settings.lock().await.project_management.clone();
    let Ok(repo) = configured_repo(&settings) else {
        return;
    };
    if let Err(error) = record_mutation(
        &repo,
        event,
        subject,
        details,
        &[],
        &format!("feat(pm): {event} {subject}"),
    ) {
        eprintln!("[pm] document event not recorded: {error}");
    }
}

/// Where a project's repository is ON THIS MACHINE.
///
/// The board is shared between machines (one control repo, Forgejo behind
/// it) and `source_path` on a project record is one absolute path, so it can
/// be right on one host only. On 2026-09-06 the fleet moved to tron and every
/// source_path pointed into /Users/cand0rian; tron could verify and dispatch
/// nothing. The override lives beside the app's other per-machine files,
/// `project-paths.json`, `{"XNAUT": "/Users/zelda/DevHub_Studio/..."}`, and is
/// applied at READ time by the fleet's readers only: the board is never
/// rewritten with a path that is true on one machine.
pub fn local_source_path(project: &ProjectRecord) -> String {
    local_path_overrides()
        .get(&project.key)
        .cloned()
        .unwrap_or_else(|| project.source_path.clone())
}

fn local_path_overrides() -> std::collections::HashMap<String, String> {
    let Some(dir) = crate::loop_acceptance::platform_config_dir() else {
        return Default::default();
    };
    let path = dir.join("xnaut").join("project-paths.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|body| serde_json::from_str(&body).ok())
        .unwrap_or_default()
}

fn display_paths(mut projects: Vec<ProjectRecord>, paths: &std::collections::HashMap<String, String>) -> Vec<ProjectRecord> {
    for project in &mut projects {
        if let Some(path) = paths.get(&project.key) { project.source_path = path.clone(); }
    }
    projects
}
fn projects_for_display(projects: Vec<ProjectRecord>) -> Vec<ProjectRecord> {
    display_paths(projects, &local_path_overrides())
}

pub fn list_projects(repo: &Path) -> Result<Vec<ProjectRecord>, String> {
    let mut projects = Vec::new();
    for entry in std::fs::read_dir(repo.join("projects"))
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let manifest = entry.path().join("project.json");
        if manifest.is_file() {
            // One corrupt manifest must not blank the whole board — skip it
            // (loudly) and keep every project that still parses.
            match read_json(&manifest) {
                Ok(project) => projects.push(project),
                Err(error) => eprintln!("[pm] skipping unreadable project: {error}"),
            }
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
            owner_only: false,
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
            // Imported, not scaffolded: this project has never walked NautFlow,
            // so it has no stage (XNAUT-87). The flow is opt-in, and stamping
            // "idea" on every import is what made the overview tell a shipped
            // product to go and define its requirements.
            stage: String::new(),
            revision: default_revision(),
            source_repo: if forge_remote.is_empty() {
                task.path.clone()
            } else {
                forge_remote.clone()
            },
            source_path: task.path.clone(),
            forge_remote,
            task_id: task.id.clone(),
            fleet: false,
            client: None,
            issue_intake: Default::default(),
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
                owner_only: false,
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
                // Migrated from the legacy client store; never in NautFlow.
                stage: String::new(),
                revision: default_revision(),
                source_repo: String::new(),
                source_path: String::new(),
                forge_remote: String::new(),
                task_id: client.task_id.clone(),
                fleet: false,
                client: None,
                issue_intake: Default::default(),
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
                owner_only: false,
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
                // Preserved legacy work, not a flow anyone ran (XNAUT-87).
                stage: String::new(),
                revision: default_revision(),
                source_repo: String::new(),
                source_path: String::new(),
                forge_remote: String::new(),
                task_id: String::new(),
                fleet: false,
                client: None,
                issue_intake: Default::default(),
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
                model_requirement: String::new(),
                approval: Default::default(),
                id: id.clone(),
                project: project.key.clone(),
                title: todo.text.clone(),
                ticket_type: "task".into(),
                status: if todo.done { "done".into() } else { "inbox".into() },
                priority: "medium".into(),
                owner: None,
                documentation: Vec::new(),
                tags: vec![],
                release: String::new(),
                body: format!("Migrated from the legacy xNaut project todo store.\n\nOriginal project ID: {task_id}"),
                source_id: todo.id.clone(),
                handback: None,
                parent: None,
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
    Ok(projects_for_display(list_projects(&configured_repo(&settings)?)?))
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
    Ok(projects_for_display(migrate_legacy_pm_data(
        &repo,
        &crate::pm::load_pm_projects(),
        &crate::project_todos::load_all(),
    )?))
}

#[tauri::command]
pub async fn pm_project_create(
    state: State<'_, crate::state::AppState>,
    request: ProjectCreateRequest,
) -> Result<ProjectRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    let repo = configured_repo(&settings)?;
    let key = validate_project_key(&request.key)?;
    let forge_remote = crate::repository_transfer::validate_remote(request.forge_remote.as_deref().unwrap_or(&request.source_repo))?;
    let name = request.name.trim();
    if name.is_empty() {
        return Err("project name is required".into());
    }
    let flow_type = validate_choice(
        &request.flow_type,
        "flow type",
        &["standard", "feature", "incident"],
    )?;
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
        owner_only: false,
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
        forge_remote,
        task_id: String::new(),
        fleet: false,
        client: None,
        issue_intake: Default::default(),
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
    let flow_type = validate_choice(
        &request.flow_type,
        "flow type",
        &["standard", "feature", "incident"],
    )?;
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
    let previous_remote = record.forge_remote.clone();
    let source_repo = request.source_repo.trim();
    record.name = name.into();
    record.purpose = request.purpose.trim().into();
    record.owner = request.owner.trim().into();
    record.client_name = request.client_name.trim().into();
    record.contact_name = request.contact_name.trim().into();
    record.contact_email = request.contact_email.trim().into();
    record.budget_chf = request.budget_chf;
    record.hourly_rate_chf = request.hourly_rate_chf;
    record.stage = next_project_stage(
        &record.stage,
        request.stage.as_deref(),
        &flow_type,
        record.flow_type != flow_type,
    )?;
    record.flow_type = flow_type;
    // A display record can carry this machine's override. Saving Settings
    // must not copy that path back into the shared control repository.
    let mut paths = local_path_overrides();
    let local_edit = paths.contains_key(&key)
        && (Path::new(source_repo).is_absolute() || (source_repo.is_empty() && request.forge_remote.is_some()));
    if local_edit {
        paths.insert(key.clone(), source_repo.into());
    } else {
        record.source_repo = source_repo.into();
        record.source_path = if Path::new(source_repo).is_absolute() {
        source_repo.into()
    } else if source_repo.is_empty() && request.forge_remote.is_some() {
        String::new()
    } else {
        // Legacy stage updates may still send a clone URL as source_repo.
        // That must not erase the independently linked local folder.
        record.source_path.clone()
        };
    }
    // Legacy callers updating a stage keep the configured destination.
    // The settings form always supplies it explicitly and cannot clear it.
    if let Some(remote) = request.forge_remote.as_deref() {
        record.forge_remote = crate::repository_transfer::validate_remote(remote)?;
    } else if record.forge_remote.is_empty() && crate::repository_transfer::validate_remote(source_repo).is_ok() {
        record.forge_remote = source_repo.into();
    }
    if previous_remote != record.forge_remote {
        crate::repository_review::revoke_for_repository_change(&key)?;
    }
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
    if local_edit {
        let dir = crate::loop_acceptance::platform_config_dir().ok_or("Machine configuration folder unavailable")?;
        write_json_atomic(&dir.join("xnaut/project-paths.json"), &paths)?;
    }
    Ok(projects_for_display(vec![record]).remove(0))
}

/// Set a project's issue-intake settings and nothing else (XNAUT-382).
///
/// Deliberately NOT part of `project_update_in`. That one takes an
/// `expected_revision` and rewrites eleven fields from a form; this is one
/// toggle on a settings pane, and routing it through the form would make a
/// checkbox capable of blanking a project's contact details. It still bumps
/// the revision and still commits, so a toggle is as visible in the board's
/// history as any other write.
pub fn set_issue_intake_in(
    repo: &Path,
    project: &str,
    intake: crate::issue_intake::IssueIntake,
) -> Result<crate::issue_intake::IssueIntake, String> {
    let key = validate_project_key(project)?;
    if intake.label.trim().is_empty() && intake.trigger == crate::issue_intake::Trigger::Labelled {
        return Err("a label trigger needs a label".into());
    }
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let manifest = repo.join("projects").join(&key).join("project.json");
    if !manifest.is_file() {
        return Err(format!("project does not exist: {key}"));
    }
    let mut record: ProjectRecord = read_json(&manifest)?;
    let intake = crate::issue_intake::IssueIntake {
        label: intake.label.trim().to_string(),
        linear_team: intake.linear_team.trim().to_string(),
        ..intake
    };
    if record.issue_intake == intake {
        return Ok(intake);
    }
    record.issue_intake = intake.clone();
    record.revision += 1;
    write_json_atomic(&manifest, &record)?;
    record_mutation(
        &repo,
        "project.updated",
        &key,
        json!({
            "issue_intake": &record.issue_intake,
            "revision": record.revision,
        }),
        &[manifest],
        &format!("chore(pm): issue intake for {key}"),
    )?;
    Ok(intake)
}

/// The control repo without a Tauri `State` handle.
///
/// The agent tools run inside the chat loop, which has no `State`. Reading the
/// settings from disk is how they reach the repo; the alternative was a second
/// ticket-writing path, and two writers drift apart. Every mutation below still
/// goes through `record_mutation`, so an agent's edit produces the same JSON +
/// event + commit a human's does.
pub fn repo_now() -> Result<PathBuf, String> {
    configured_repo(&crate::settings::load_or_default().project_management)
}

/// Where the control repo sits, WITHOUT the validity inspection `repo_now`
/// performs. For readers that only want to open one known file.
///
/// `configured_repo` calls `inspect`, which runs several git commands and
/// counts every ticket file in every project. The run sweep reads one ticket
/// per live run on every pass; paying `inspect` for each of those would put
/// the sweep straight back into the git-storm territory of XNAUT-432, where
/// 116 git processes at 786% CPU came from exactly this kind of per-call work
/// on this exact repository. A caller that wants validity still calls
/// `repo_now`; a caller that wants a path gets a path.
pub fn repo_path_now() -> Option<PathBuf> {
    let settings = crate::settings::load_or_default().project_management;
    if !settings.enabled || settings.repo_path.trim().is_empty() {
        return None;
    }
    resolve_path(&settings.repo_path).ok()
}

pub fn ticket_list_in(repo: &Path, project: Option<String>) -> Result<Vec<TicketRecord>, String> {
let roots: Vec<PathBuf> = if let Some(project) = project {
        vec![repo
            .join("projects")
            .join(validate_project_key(&project)?)
            .join("tickets")]
    } else {
        list_projects(repo)?
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
pub async fn pm_ticket_list(
    state: State<'_, crate::state::AppState>,
    project: Option<String>,
) -> Result<Vec<TicketRecord>, String> {
    let settings = state.settings.lock().await.project_management.clone();
    ticket_list_in(&configured_repo(&settings)?, project)
}

/// Workspace headers only need the number of stored tickets, not hundreds of
/// megabytes of historical jury evidence serialized across the webview bridge.
fn ticket_count_in(repo: &Path, project: &str) -> Result<usize, String> {
    let key = validate_project_key(project)?;
    let root = repo.join("projects").join(&key);
    if !root.join("project.json").is_file() { return Err("Project is not registered".into()); }
    let entries = match std::fs::read_dir(root.join("tickets")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.to_string()),
    };
    Ok(entries.flatten().filter(|entry| entry.file_type().is_ok_and(|t| t.is_file())
        && entry.path().extension().and_then(|v| v.to_str()) == Some("json")
        && entry.file_name().to_string_lossy().starts_with(&format!("{key}-"))).count())
}

#[tauri::command]
pub async fn pm_project_ticket_count(state: State<'_, crate::state::AppState>, project: String) -> Result<usize, String> {
    let settings = state.settings.lock().await.project_management.clone();
    ticket_count_in(&configured_repo(&settings)?, &project)
}

/// One hand-off in a ticket's life.
#[derive(Debug, Clone, Serialize)]
pub struct OwnerChange {
    /// None means the ticket was unassigned at that point.
    pub owner: Option<String>,
    pub at: String,
}

/// Who has held this ticket, oldest first.
///
/// Read from the control repo's git history rather than from the events,
/// because the events only started carrying the owner today and the
/// interesting hand-offs are older than that. Every ticket write is one
/// commit, so this is exact rather than inferred.
pub fn ticket_owner_history(repo: &Path, id: &str) -> Result<Vec<OwnerChange>, String> {
    let path = find_ticket_path(repo, id)?;
    let rel = path
        .strip_prefix(repo)
        .unwrap_or(&path)
        .to_string_lossy()
        .to_string();
    let log = std::process::Command::new("git")
        .args(["log", "--format=%H %aI", "--", &rel])
        .current_dir(repo)
        .output()
        .map_err(|error| format!("git log: {error}"))?;
    let mut history: Vec<OwnerChange> = Vec::new();
    // git log is newest first; walk oldest first so "changed" means changed.
    let lines: Vec<String> = String::from_utf8_lossy(&log.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    for line in lines.into_iter().rev() {
        let mut parts = line.split_whitespace();
        let (Some(sha), Some(at)) = (parts.next(), parts.next()) else {
            continue;
        };
        let show = std::process::Command::new("git")
            .args(["show", &format!("{sha}:{rel}")])
            .current_dir(repo)
            .output();
        let Ok(show) = show else { continue };
        let Ok(value) = serde_json::from_slice::<Value>(&show.stdout) else {
            continue;
        };
        let owner = value
            .get("owner")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|o| !o.trim().is_empty());
        if history.last().map(|last| &last.owner) != Some(&owner) {
            history.push(OwnerChange {
                owner,
                at: at.to_string(),
            });
        }
    }
    Ok(history)
}

#[tauri::command]
pub async fn pm_ticket_owner_history(
    state: State<'_, crate::state::AppState>,
    id: String,
) -> Result<Vec<OwnerChange>, String> {
    let settings = state.settings.lock().await.project_management.clone();
    ticket_owner_history(&configured_repo(&settings)?, &id)
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

/// Every status a ticket may hold.
///
/// `done` and `complete` are two different claims and both are needed:
/// `done` is the agent's word, "the work is finished"; `complete` is
/// NautBot's, "tested, checked and approved". Collapsing them is how a board
/// ends up full of finished-but-never-verified work.
/// Every kind a ticket may be.
///
/// ONE list, because until XNAUT-357 this was written out by hand in four
/// places — two validators, the record schema and the MCP tool schema — and a
/// new kind added to three of them is a kind the tool an agent actually uses
/// refuses. `finding` is that new kind: a candidate the core team turned up,
/// which is not a feature anybody has agreed to build, and a board that cannot
/// tell those two apart turns its backlog into a wish list.
pub const TICKET_TYPES: &[&str] = &["idea", "feature", "bug", "incident", "task", "finding"];

pub const TICKET_STATUSES: &[&str] = &[
    "inbox",
    "ready",
    "in_progress",
    "review",
    "blocked",
    "done",
    "complete",
];

pub fn ticket_create_in(repo: &Path, request: TicketCreateRequest) -> Result<TicketRecord, String> {
    let key = validate_project_key(&request.project)?;
    let title = request.title.trim();
    if title.is_empty() {
        return Err("ticket title is required".into());
    }
    let ticket_type = validate_choice(
        &request.ticket_type,
        "ticket type",
        TICKET_TYPES,
    )?;
    let status = validate_choice(
        &request.status,
        "status",
        TICKET_STATUSES,
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
        approval: Default::default(),
id: id.clone(),
        project: key,
        title: title.into(),
        ticket_type,
        status,
        priority,
        owner: request.owner.filter(|value| !value.trim().is_empty()),
        documentation: request.documentation,
        tags: request.tags,
        release: request.release,
        body: request.body,
        model_requirement: request.model_requirement.trim().to_string(),
        source_id: request.source_id.trim().to_string(),
        handback: None,
        parent: request.parent,
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
pub async fn pm_ticket_create(
    state: State<'_, crate::state::AppState>,
    request: TicketCreateRequest,
) -> Result<TicketRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    ticket_create_in(&configured_repo(&settings)?, request)
}

/// `complete` is NautBot's word: tested, checked and approved. ONE shared
/// refusal, used by the shared write below and by the chat loop, which must
/// refuse BEFORE the repo is opened so a machine with no PM repo gives the
/// same answer (the first exe.dev verify run caught the late version of this
/// rail: a disabled PM module answered first). `caller` None is the app's own
/// UI: the owner operating their board directly, not an agent, not gated.
pub fn foreign_complete_refusal(caller: Option<&str>, status: Option<&str>) -> Option<String> {
    let caller = caller.map(|c| c.trim().trim_start_matches('@').to_ascii_lowercase());
    let is_agent = caller.is_some();
    let is_nautbot = caller.as_deref() == Some(crate::agent_profiles::RESERVED_NAUTBOT_HANDLE);
    if status == Some("complete") && is_agent && !is_nautbot {
        return Some(
            "only NautBot can set a ticket to complete. Set it to done and it goes back to NautBot, who tests and approves it."
                .to_string(),
        );
    }
    None
}

thread_local! {
    /// Set only by the verify rail while it settles a record it has just
    /// validated. Everything else that says `complete` as NautBot must show a
    /// green record on disk.
    static VERIFIED_SETTLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run `f` as the verify rail: the `complete` it writes is backed by the
/// record in its hand, not by a second read of the store.
pub(crate) fn as_verified_settle<T>(f: impl FnOnce() -> T) -> T {
    VERIFIED_SETTLE.with(|v| v.set(true));
    let out = f();
    VERIFIED_SETTLE.with(|v| v.set(false));
    out
}

pub fn ticket_update_in(repo: &Path, request: TicketUpdateRequest) -> Result<TicketRecord, String> {
    ticket_update_with_registry_in(repo, &crate::agents::registry_dir()?, request)
}

/// XNAUT-472: serialize this transaction across app processes as well as threads.
/// Keep the lease in the common Git directory so linked worktrees share it.
struct TicketUpdateLease(std::fs::File);
impl TicketUpdateLease {
    fn acquire(repo: &Path) -> Result<Self, String> {
        let common = run_git(repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let file = std::fs::OpenOptions::new()
            .read(true).write(true).create(true).truncate(false)
            .open(Path::new(&common).join("xnaut-ticket-update.lock"))
            .map_err(|e| format!("cannot open ticket update lease: {e}"))?;
        match file.try_lock() {
            Ok(()) => Ok(Self(file)),
            Err(std::fs::TryLockError::WouldBlock) => Err(
                "another process is updating this control repository; reload the ticket and retry".into(),
            ),
            Err(std::fs::TryLockError::Error(e)) => Err(format!("cannot lock ticket update: {e}")),
        }
    }
}
impl Drop for TicketUpdateLease {
    fn drop(&mut self) { let _ = self.0.unlock(); }
}

struct TicketReconcileWorktree<'a> {
    repo: &'a Path,
    path: PathBuf,
}
impl Drop for TicketReconcileWorktree<'_> {
    fn drop(&mut self) {
        // Only this disposable detached checkout is removed, including a
        // scratch merge conflict. No shared file/index or original commit moves.
        let _ = run_git(self.repo, &["worktree", "remove", "--force", &self.path.to_string_lossy()]);
    }
}

fn finish_ticket_reconciliation(
    repo: &Path, branch: &str, original: &str, merged: &str,
) -> Result<(), String> {
    if run_git(repo, &["symbolic-ref", "--short", "HEAD"])? != branch
        || run_git(repo, &["rev-parse", "HEAD"])? != original
    {
        return Err("control repository changed during ticket reconciliation; no ticket was written; reload and retry".into());
    }
    // This candidate descends from the original HEAD. Git's normal checkout
    // checks preserve both staged and unstaged disjoint edits and refuse any
    // tracked/untracked path it would overwrite. Explicitly defeat user-level
    // merge.autoStash: other people's changes must never be moved aside.
    run_git(repo, &["merge", "--ff-only", "--no-autostash", "--no-edit", merged])
        .map_err(|e| format!("ticket update cannot reconcile incoming changes without touching pending files; preserve and resolve the paths below, then reload and retry. Local commits and pending edits were not stashed or reset: {e}"))?;
    if run_git(repo, &["rev-parse", "HEAD"])? != merged {
        return Err("control repository moved during ticket reconciliation; reload the ticket before retrying".into());
    }
    Ok(())
}

fn reconcile_ticket_with_unrelated_dirt(
    repo: &Path, branch: &str, remote_ref: &str,
) -> Result<(), String> {
    let original = run_git(repo, &["rev-parse", "HEAD"])?;
    let incoming = run_git(repo, &["rev-parse", remote_ref])?;
    let common = run_git(repo, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let path = Path::new(&common).join(format!("xnaut-ticket-reconcile-{}", uuid::Uuid::new_v4()));
    run_git(repo, &["worktree", "add", "--detach", &path.to_string_lossy(), &original])?;
    let scratch = TicketReconcileWorktree { repo, path };
    // Merge only committed history in isolation. Rebasing the shared checkout
    // refuses unrelated dirt; merging here retains every local handback commit
    // as an ancestor and never stages another ticket's pending edits.
    if let Err(error) = run_git(&scratch.path, &[
        "-c", "user.name=xNaut", "-c", "user.email=xnaut@local",
        "merge", "--no-edit", "--no-autostash", &incoming,
    ]) {
        let paths = run_git(&scratch.path, &["diff", "--name-only", "--diff-filter=U"])
            .unwrap_or_default();
        return Err(format!("committed control history could not be reconciled; resolve these paths before retrying this ticket: {paths}. Shared checkout, index, and local handback commits are preserved: {error}"));
    }
    let merged = run_git(&scratch.path, &["rev-parse", "HEAD"])?;
    finish_ticket_reconciliation(repo, branch, &original, &merged)
}

/// XNAUT-414: a fetch that cannot reach the remote does not fail the write.
/// A Finder-launched app has no ssh agent, so `git fetch` over the Tailscale
/// remote failed and its error was returned as the WRITE's error: every
/// ticket update from the app failed while the same write from a terminal
/// worked, and CHESSTRAINER-5 verified green five times without moving.
fn ticket_update_with_registry_in(repo: &Path, registry: &Path, request: TicketUpdateRequest) -> Result<TicketRecord, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let _lease = TicketUpdateLease::acquire(repo)?;
    // XNAUT-297: reconcile committed fleet edits before reading the revision.
    // Local-only control repos still work. Never stash, reset, or choose a side
    // of a conflict: a committed handback must remain recoverable on its branch.
    let has_origin = run_git(repo, &["remote"])?.lines().any(|remote| remote == "origin");
    if has_origin {
        let git_dir = PathBuf::from(run_git(repo, &["rev-parse", "--absolute-git-dir"])?);
        if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
            return Err("control repository already has a rebase in progress; resolve it before updating a ticket".into());
        }
        if git_dir.join("MERGE_HEAD").exists() {
            return Err("control repository already has a merge in progress; resolve it before updating a ticket".into());
        }
        // Only dirt that would ride along blocks the write (XNAUT-412): every
        // mutation commits with `--only` its own paths, so another ticket's
        // leftover edit cannot be swept in. See `dirt_blocks`.
        if dirt_blocks(repo, &request.id)? {
            return Err(format!("{} has uncommitted changes in the control repository; resolve them before updating it", request.id));
        }
        let branch = run_git(repo, &["symbolic-ref", "--short", "HEAD"])?;
        let remote_ref = format!("refs/remotes/origin/{branch}");
        for attempt in 0..2 {
            // Unreachable remote, local write anyway (XNAUT-414).
            if let Err(error) = run_git(repo, &["fetch", "origin"]) {
                let _ = crate::debug_log::debug_log_append(vec![format!(
                    "[pm] fetch of origin failed, writing locally: {}",
                    error.lines().last().unwrap_or(&error)
                )]);
                break;
            }
            // An empty remote has no branch until the first sync.
            if run_git(repo, &["show-ref", "--verify", "--quiet", &remote_ref]).is_err() {
                break;
            }
            // Nothing to rebase onto: HEAD already holds the remote. Asking
            // git anyway made it refuse on ANY unstaged file ("cannot rebase:
            // You have unstaged changes"), so one ticket's stranded write
            // blocked every other ticket's update, the very thing XNAUT-412
            // stopped one layer down. Tron, 2026-09-20: XNAUT-394's file was
            // left modified by an interrupted write and XNAUT-379's green
            // verify could not settle behind it.
            if run_git(repo, &["merge-base", "--is-ancestor", &remote_ref, "HEAD"]).is_ok() {
                break;
            }
            if !run_git(repo, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
                reconcile_ticket_with_unrelated_dirt(repo, &branch, &remote_ref)?;
                break;
            }
            match run_git(repo, &["rebase", "--no-autostash", &remote_ref]) {
                Ok(_) => break,
                Err(error) => {
                    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
                        run_git(repo, &["rebase", "--abort"])
                            .map_err(|abort| format!("{error}; rebase cleanup failed: {abort}"))?;
                    }
                    if attempt == 1 {
                        return Err(format!("ticket update refused after two rebase attempts; local commits preserved: {error}"));
                    }
                }
            }
        }
    }
    // ── The two rails, enforced HERE so no caller can miss them ─────────
    //
    // XNAUT-243, found by dogfooding: these were written in agent_tools.rs
    // (the chat loop) and were therefore absent from agent_hooks.rs's MCP
    // tool, which is the path an agent in a run uses. @claude finished a
    // ticket, set done, and it stayed owned by @claude, so NautBot would
    // never have seen it as awaiting review.
    //
    // `caller` None is the app's own UI: the owner operating their board
    // directly, who is not an agent and is not gated.
    let caller = request
        .caller
        .as_deref()
        .map(|c| c.trim().trim_start_matches('@').to_ascii_lowercase());
    let is_agent = caller.is_some();
    let is_nautbot = caller.as_deref() == Some(crate::agent_profiles::RESERVED_NAUTBOT_HANDLE);
    let mut request = request;
    let mut handed_back = false;
    // The complete guard is the shared foreign_complete_refusal above; a
    // second inline copy is how the two paths drifted apart before.
    if let Some(refusal) =
        foreign_complete_refusal(request.caller.as_deref(), request.status.as_deref())
    {
        return Err(refusal);
    }
    // `complete` means tested and approved, and the test is the sandbox verify.
    // NautBot is an agent with the same tool as everyone else; on XNAUT-107
    // (2026-09-07) it set complete and merged 41 seconds after its verify run
    // FAILED at the rsync step. The sweep's rail checked the record; NautBot's
    // hand did not. One check, here, for both: no green run, no complete.
    if request.status.as_deref() == Some("complete")
        && is_nautbot
        && !VERIFIED_SETTLE.with(|v| v.get())
        && !crate::sandbox_verify::green_record_exists(&request.id)
    {
        return Err(format!(
            "{} has no passed sandbox verify with evidence; complete is what a green run earns, not a word NautBot can choose",
            request.id
        ));
    }
    match request.status.as_deref() {
        // `done` and `review` are the SAME claim from an agent: I have
        // finished, someone else must look. Only `done` used to hand the
        // ticket back, so an agent that reached for the more natural word
        // left it owned by itself and nobody was told (observed on
        // XNAUT-233, which sat in review owned by @claude). Both hand back.
        Some("done") | Some("review") if is_agent && !is_nautbot => {
            request.owner = Some(Some(
                crate::agent_profiles::RESERVED_NAUTBOT_HANDLE.to_string(),
            ));
            request.clear_owner = false;
            handed_back = true;
            // Say it on the ticket as well as in the record. A status change
            // tells you something moved; a line tells you who said what, and
            // that is what a reviewer opening this ticket in a week needs.
            let said = format!(
                "@{} handed this to @{} for review.",
                caller.as_deref().unwrap_or("agent"),
                crate::agent_profiles::RESERVED_NAUTBOT_HANDLE
            );
            request.body = Some(match request.body {
                Some(body) if !body.trim().is_empty() => format!("{}\n\n{said}", body.trim_end()),
                _ => said,
            });
        }
        _ => {}
    }
    let request = request;

    let path = find_ticket_path(&repo, &request.id)?;
    // A non-cooperating writer may have edited the requested ticket while the
    // remote/scratch work was running. Never absorb that edit into this write.
    let rel = path.strip_prefix(repo).map_err(|_| "ticket path escaped control repository")?;
    if has_origin && !run_git(repo, &["status", "--porcelain", "--", &rel.to_string_lossy()])?.is_empty() {
        return Err(format!("{} acquired uncommitted changes while reconciling; preserve them, reload the ticket and retry", request.id));
    }
    let mut record: TicketRecord = read_json(&path)?;
    let previous_status = record.status.clone();
    let previous_owner = record.owner.clone().unwrap_or_default();
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
            TICKET_TYPES,
        )?;
    }
    if let Some(status) = request.status {
        record.status = validate_choice(
            &status,
            "status",
            TICKET_STATUSES,
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
    if let Some(requirement) = request.model_requirement {
        record.model_requirement = requirement.trim().to_string();
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
        // The owner belongs in the event: "who holds this now" is the
        // question the board asks, and reconstructing it from git afterwards
        // is work the write already knew the answer to.
        json!({
            "status": record.status,
            "revision": record.revision,
            "owner": record.owner,
        }),
        &[path],
        &format!("chore(pm): update {}", record.id),
    )?;
    drop(_lease);
    drop(_guard);
    crate::run_control::ticket_completed_in(
        registry, &record.id, &previous_owner, &previous_status, &record.status,
        |run| crate::run_control::observe_in(registry, run, &[]),
    )?;
    // The handback is a WRITE; on its own it tells nobody. Andre, 2026-08-29,
    // after a ticket came back correctly and sat there: "what if we implement
    // a nudge by the working agent to NautBot when set to done?" So the same
    // transition that reassigns the ticket also tells NautBot it has one.
    if handed_back {
        announce_handback(&record.id, &record.title);
    }
    Ok(record)
}

/// Set a ticket's tags and body in one write.
///
/// Separate from [`ticket_update_in`] because tags are not on
/// `TicketUpdateRequest` and must not be: they are the owner's to set (André,
/// 2026-09-08), and an agent handed a general tag field would relabel work it
/// does not own. This is the one exception, and it is narrow on purpose — the
/// Reviewer moving a finding it has just scored between `poc` and `shelved`
/// (XNAUT-357) — so it takes no status, no owner and no title.
///
/// Goes through the same lock, the same atomic write and the same event as
/// every other ticket mutation, so a retag is as reviewable in the control
/// repo's history as a status change.
pub fn ticket_retag_in(
    repo: &Path,
    id: &str,
    tags: Vec<String>,
    body: String,
) -> Result<TicketRecord, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let path = find_ticket_path(repo, id)?;
    let mut record: TicketRecord = read_json(&path)?;
    record.tags = tags
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect();
    record.body = body;
    record.revision += 1;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_json_atomic(&path, &record)?;
    record_mutation(
        repo,
        "ticket.updated",
        &record.id,
        json!({ "tags": record.tags, "revision": record.revision }),
        &[path],
        &format!("chore(pm): retag {}", record.id),
    )?;
    Ok(record)
}

/// What happened to a filed handback.
#[derive(Debug, Clone)]
pub enum Filing {
    /// The checker refused it. Nothing was written; the verdict says why.
    Refused(crate::handback::Verdict),
    /// Stored on the ticket. The verdict may still carry notes.
    Filed {
        ticket: Box<TicketRecord>,
        verdict: crate::handback::Verdict,
    },
}

/// Review a handback, and store it only if it is reviewable.
///
/// THE rail, in one place. The HTTP route and the MCP tool are two callers of
/// one vocabulary, and this codebase has already paid for letting such a pair
/// drift: XNAUT-243 put the done/complete rails in the chat tool only, so the
/// MCP path an agent actually uses in a run had none of them, and a ticket sat
/// finished-but-unassigned with nobody told. A rail written twice is a rail
/// that will eventually exist once.
///
/// A refusal writes NOTHING. Half-storing an unreviewable handback would leave
/// a record on the ticket that reads as a report and is not one, which is
/// worse than the prose it replaced.
pub fn file_handback_in(
    repo: &Path,
    handback: &crate::handback::Handback,
) -> Result<Filing, String> {
    file_handback_with_registry_in(repo, &crate::agents::registry_dir()?, handback)
}

pub(crate) fn file_handback_with_registry_in(
    repo: &Path, registry: &Path, handback: &crate::handback::Handback,
) -> Result<Filing, String> {
    let verdict = crate::handback::review(handback);
    if !verdict.is_reviewable() {
        return Ok(Filing::Refused(verdict));
    }
    // A parent is not finished while its children are open. This is the
    // upward flow of a planner tree for free: the handoff that matters is
    // the one that arrives after everything under it has arrived (XNAUT-316).
    let open = crate::subdivide::open_children(&ticket_list_in(repo, None)?, &handback.ticket);
    if !open.is_empty() {
        return Err(format!(
            "{} has children still open: {}. Their handbacks come first.",
            handback.ticket,
            open.join(", ")
        ));
    }
    let ticket = crate::run_control::record_handback_in(registry, handback, || {
        attach_handback_in(repo, handback)
    })?;
    // What the agent learned is a memory the next agent on these files reads
    // before it starts (XNAUT-331).
    crate::memory::note(crate::memory::Entry {
        kind: "learning".into(),
        project: ticket.project.clone(),
        ticket: handback.ticket.clone(),
        run_id: handback.run_id.clone().unwrap_or_default(),
        files: handback.files_changed.clone(),
        text: handback.summary.clone(),
        cause: handback.not_finished.clone().filter(|n| !n.trim().eq_ignore_ascii_case("nothing")).unwrap_or_default(),
        fix: handback.commits.join(", "),
        source: format!("handback:{}:{}", handback.ticket, handback.run_id.clone().unwrap_or_else(|| handback.submitted_at.clone())),
        ..Default::default()
    });
    Ok(Filing::Filed {
        ticket: Box::new(ticket),
        verdict,
    })
}

/// Attach a structured handback to its ticket, so it survives a restart and a
/// human can read it later.
///
/// Deliberately NOT a field on `TicketUpdateRequest`, for two reasons learned
/// from reading `ticket_update_in` above:
///
///   1. It carries optimistic concurrency, and an agent filing a handback does
///      not know the ticket's revision. Forcing it to read-then-write would
///      make the commonest write in the system the one most likely to lose a
///      race with the sweep.
///   2. It carries the done/complete rails, and a handback is not a status
///      change. Threading it through there would mean every future change to
///      those rails has to reason about a payload that has nothing to do with
///      them.
///
/// It takes the same `mutation_lock` and writes the same three things a valid
/// control-repo change is made of: the ticket JSON, an event, and a commit.
/// Filing a handback does not move the ticket; the agent still sets `done`
/// itself, and that write still hands the ticket to NautBot.
///
/// Private on purpose: everything outside goes through `file_handback_in`, so
/// there is no way to reach the write while skipping the check.
fn attach_handback_in(
    repo: &Path,
    handback: &crate::handback::Handback,
) -> Result<TicketRecord, String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "Project Management mutation lock is unavailable")?;
    let path = find_ticket_path(repo, &handback.ticket)?;
    let mut record: TicketRecord = read_json(&path)?;
    record.handback = Some(handback.clone());
    record.revision += 1;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_json_atomic(&path, &record)?;
    record_mutation(
        repo,
        "ticket.handback",
        &record.id,
        json!({
            "revision": record.revision,
            "from": handback.from,
            "files_changed": handback.files_changed.len(),
            "verify_record_id": handback.verify_record_id,
        }),
        &[path],
        &format!("chore(pm): handback for {}", record.id),
    )?;
    Ok(record)
}

/// App-owned jury receipts share the ticket mutation lock and event/commit
/// boundary with handbacks. Public agent update tools cannot forge receipts.
pub(crate) fn attach_jury_in(repo: &Path, job: &crate::jury::Job, status: Option<&str>) -> Result<TicketRecord,String> {
    let _guard=mutation_lock().lock().map_err(|_|"PM mutation lock unavailable")?;
    let path=find_ticket_path(repo,&job.ticket)?;
    let mut record:TicketRecord=read_json(&path)?;
    let previous=record.approval.jury_reviews.iter().find(|j|j.id==job.id);
    if previous.is_none() && job.decision==Some(crate::jury::Decision::Approved) && record.revision!=job.ticket_revision {return Err("ticket changed before jury receipt commit".into());}
    if previous.is_some_and(|j|["revoke_requested","revoked"].contains(&j.state.as_str()))
        && !["revoke_requested","revoked","rollback_requested","reverted"].contains(&job.state.as_str()) {return Err("revocation supersedes this jury update".into());}
    if let Some(old)=record.approval.jury_reviews.iter_mut().find(|j|j.id==job.id) { *old=job.clone(); }
    else { record.approval.jury_reviews.push(job.clone()); }
    if let Some(signoff)=&job.signoff { record.approval.signoff=Some(signoff.clone()); }
    let mut release_change: Option<(String, String)> = None;
    if job.state=="integrated" && record.release.trim().is_empty() {
        if let Some(version)=crate::jury_signoff::integrated_version(job) {
            release_change=Some((record.release.clone(), version.clone()));
            record.release=version;
        }
    }
    if let Some(status)=status {
        record.status=status.into();
        if status=="in_progress" && !job.author.trim().is_empty() { record.owner=Some(job.author.clone()); }
    }
    record.revision+=1;
    record.updated_at=chrono::Utc::now().to_rfc3339();
    write_json_atomic(&path,&record)?;
    record_mutation(repo,"ticket.jury",&record.id,json!({"jury_id":job.id,"decision":job.decision,"state":job.state,"revision":record.revision}),&[path.clone()],&format!("chore(pm): jury receipt for {}",record.id))?;
    if let Some((previous,new))=release_change {
        record_mutation(repo,"ticket.release",&record.id,json!({"previous":previous,"new":new,"revision":record.revision}),&[],&format!("chore(pm): {} release {} -> {}",record.id,if previous.is_empty(){"unassigned"}else{&previous},new))?;
    }
    Ok(record)
}

/// Set (or clear, with "") the release a ticket ships in. Emits
/// `ticket.release` with the previous and new value on every change.
pub fn ticket_release_in(repo: &Path, id: &str, release: &str) -> Result<TicketRecord, String> {
    let _guard = mutation_lock().lock().map_err(|_| "PM mutation lock unavailable")?;
    let release = release.trim().to_string();
    let path = find_ticket_path(repo, id)?;
    let mut record: TicketRecord = read_json(&path)?;
    if record.release == release { return Ok(record); }
    let previous = std::mem::replace(&mut record.release, release.clone());
    record.revision += 1;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_json_atomic(&path, &record)?;
    record_mutation(repo, "ticket.release", &record.id, json!({"previous": previous, "new": release, "revision": record.revision}), &[path],
        &format!("chore(pm): {} release {} -> {}", record.id, if previous.is_empty() { "unassigned" } else { &previous }, if release.is_empty() { "unassigned" } else { &release }))?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_ticket_release(
    state: State<'_, crate::state::AppState>,
    id: String,
    release: String,
) -> Result<TicketRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    ticket_release_in(&configured_repo(&settings)?, &id, &release)
}

/// Add or remove one tag on a ticket. Idempotent; the write is a mutation
/// like any other, so it is committed, pushed and evented.
pub fn ticket_tag_in(repo: &Path, id: &str, tag: &str, remove: bool) -> Result<TicketRecord, String> {
    let _guard = mutation_lock().lock().map_err(|_| "PM mutation lock unavailable")?;
    let tag = tag.trim().to_string();
    if tag.is_empty() { return Err("a tag is required".into()); }
    let path = find_ticket_path(repo, id)?;
    let mut record: TicketRecord = read_json(&path)?;
    let had = record.tags.contains(&tag);
    if remove { record.tags.retain(|t| t != &tag); } else if !had { record.tags.push(tag.clone()); }
    if had == !remove { return Ok(record); }
    record.revision += 1;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_json_atomic(&path, &record)?;
    record_mutation(repo, "ticket.updated", &record.id, json!({"tags": record.tags, "revision": record.revision}), &[path], &format!("chore(pm): tag {} {}{}", record.id, if remove { "-" } else { "+" }, tag))?;
    Ok(record)
}

#[tauri::command]
pub async fn pm_ticket_tag(
    state: State<'_, crate::state::AppState>,
    id: String,
    tag: String,
    remove: Option<bool>,
) -> Result<TicketRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    ticket_tag_in(&configured_repo(&settings)?, &id, &tag, remove.unwrap_or(false))
}

/// Tell NautBot a ticket came back. Deliberately does NOT cold-launch it: a
/// wake that starts a frontier agent every time any agent finishes anything
/// is a spend decision nobody made. A live NautBot is nudged; an absent one
/// gets a Mesh inbox item, which is the surface the owner reads anyway and
/// which survives the app being closed.
fn announce_handback(id: &str, title: &str) {
    let Some(app) = crate::nudge::app().cloned() else {
        return;
    };
    let id = id.to_string();
    let title = title.to_string();
    tauri::async_runtime::spawn(async move {
        let has_session = {
            let state = tauri::Manager::state::<crate::state::AppState>(&app);
            let sessions = state.agent_sessions.lock().await;
            sessions.values().any(|meta| {
                meta.agent_id.trim().eq_ignore_ascii_case(
                    crate::agent_profiles::RESERVED_NAUTBOT_HANDLE,
                ) && meta.status != crate::status::AgentStatus::Working
            })
        };
        if has_session {
            let message = format!("{id} is done and back with you. Review it.");
            let _ = crate::nudge::nudge_agent(
                &app,
                crate::agent_profiles::RESERVED_NAUTBOT_HANDLE,
                &message,
            )
            .await;
            return;
        }
        let _ = crate::inbox::create_and_announce(
            &app,
            "todo",
            crate::inbox::PostRequest {
                project: id.split('-').next().unwrap_or("").to_string(),
                from: "system".to_string(),
                title: format!("{id} is done and needs review"),
                body: title,
                ticket: Some(id),
                ..Default::default()
            },
            None,
        );
    });
}

#[tauri::command]
pub async fn pm_ticket_update(
    state: State<'_, crate::state::AppState>,
    request: TicketUpdateRequest,
) -> Result<TicketRecord, String> {
    let settings = state.settings.lock().await.project_management.clone();
    ticket_update_in(&configured_repo(&settings)?, request)
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
mod abandoned_tmp_tests {
    use super::*;
    fn repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xnaut-tmpprune-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("projects/X/tickets")).unwrap();
        let git = |args: &[&str]| {
            let o = std::process::Command::new("git").arg("-C").arg(&dir)
                .args(["-c","user.email=t@t","-c","user.name=t","-c","commit.gpgsign=false"]).args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        };
        git(&["init","-q"]);
        std::fs::write(dir.join("README"), "x").unwrap();
        git(&["add","-A"]); git(&["commit","-q","-m","init"]);
        dir
    }
    #[test]
    fn an_abandoned_atomic_write_is_not_an_uncommitted_change_but_a_fresh_one_is() {
        // tron, 2026-09-09 17:55: the disk filled mid-write, one temp file
        // survived, and every board write from that machine was refused for
        // the next 26 hours as "uncommitted changes".
        let dir = repo("stale");
        let stale = dir.join("projects/X/tickets/X-87.tmp-3f3de3c4-0037-4bbe-b933-31a7079035d2");
        std::fs::write(&stale, "{").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(ABANDONED_TMP_SECS + 5);
        std::fs::File::options().write(true).open(&stale).unwrap().set_modified(old).unwrap();
        assert!(dirty_paths(&dir).unwrap().is_empty(), "the abandoned file does not count");
        assert!(!stale.exists(), "and it is gone");

        // A temp file that is seconds old may be a write in flight: left alone, and it counts.
        let fresh = dir.join("projects/X/tickets/X-88.tmp-0000");
        std::fs::write(&fresh, "{").unwrap();
        assert!(!dirty_paths(&dir).unwrap().is_empty());
        assert!(fresh.exists());
        std::fs::remove_file(&fresh).unwrap();

        // A real untracked ticket is a real change.
        std::fs::write(dir.join("projects/X/tickets/X-1.json"), "{}").unwrap();
        assert!(dirty_paths(&dir).unwrap().contains("X-1.json"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
    /// XNAUT-412: another ticket's leftover edit, or the app's own event
    /// exhaust, must not block a write to THIS ticket. Its own dirt still does.
    #[test]
    fn only_the_ticket_being_written_blocks_the_write() {
        let dir = repo("dirt-scope");
        let tickets = dir.join("projects/XT/tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        for id in ["XT-1", "XT-2"] {
            std::fs::write(tickets.join(format!("{id}.json")), r#"{"id":"PLACEHOLDER"}"#.replace("PLACEHOLDER", id)).unwrap();
        }
        let git = |args: &[&str]| {
            std::process::Command::new("git").current_dir(&dir)
                .args(["-c","user.email=t@t","-c","user.name=t","-c","commit.gpgsign=false"]).args(args).output().unwrap();
        };
        git(&["add","-A"]); git(&["commit","-q","-m","tickets"]);

        // Another ticket edited and not committed, plus a stray event.
        std::fs::write(tickets.join("XT-2.json"), r#"{"id":"XT-2","note":"edited"}"#).unwrap();
        std::fs::create_dir_all(dir.join("events")).unwrap();
        std::fs::write(dir.join("events/stray.json"), "{}").unwrap();
        assert!(!dirt_blocks(&dir, "XT-1").unwrap(), "another ticket's dirt is not XT-1's business");

        // Its own file, half written: that is a conflict.
        std::fs::write(tickets.join("XT-1.json"), r#"{"id":"XT-1","note":"half"}"#).unwrap();
        assert!(dirt_blocks(&dir, "XT-1").unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn maintenance_is_spaced_and_rotates_through_four_bounded_tasks() {
        use std::time::{Duration, Instant};
        let every = Duration::from_secs(1200);
        let t0 = Instant::now();
        assert!(maintenance_due(None, t0, every), "the first tick is due");
        assert!(!maintenance_due(Some(t0), t0 + Duration::from_secs(1199), every));
        assert!(maintenance_due(Some(t0), t0 + every, every));
        assert_eq!(MAINTENANCE_TASKS, ["loose-objects", "commit-graph", "incremental-repack", "pack-refs"]);
        assert_eq!(MAINTENANCE_EVERY, every);
    }

    /// André, 2026-09-21: 116 git processes at 786% CPU repacking the control
    /// repo, spawned by git's own auto-gc after the app's commits. Every call
    /// the app makes now forbids that, the repo's config is pinned the same
    /// way, a dead pack-objects' temp file is cleared, and every task in the
    /// rotation runs to completion on a real repository.
    #[test]
    fn a_maintenance_task_pins_auto_gc_off_clears_stale_temp_packs_and_runs_every_task() {
        let dir = repo("maintenance");
        let args: Vec<String> = git_command(&dir).get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.contains(&"gc.auto=0".to_string()) && args.contains(&"maintenance.auto=false".to_string()), "{args:?}");

        let packs = dir.join(".git/objects/pack");
        std::fs::create_dir_all(&packs).unwrap();
        let stale = packs.join("tmp_pack_stale");
        let fresh = packs.join("tmp_pack_fresh");
        std::fs::write(&stale, "x").unwrap();
        std::fs::write(&fresh, "x").unwrap();
        let two_hours_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600);
        std::fs::File::options().write(true).open(&stale).unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(two_hours_ago)).unwrap();

        run_maintenance_task(&dir, "loose-objects").unwrap();
        assert!(!stale.exists(), "a pack-objects that died an hour ago left this behind");
        assert!(fresh.exists(), "a temp pack may still be in use");
        assert_eq!(run_git(&dir, &["config", "gc.auto"]).unwrap(), "0");
        assert_eq!(run_git(&dir, &["config", "maintenance.auto"]).unwrap(), "false");
        for task in MAINTENANCE_TASKS {
            run_maintenance_task(&dir, task).unwrap_or_else(|e| panic!("{task}: {e}"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_atomic_write_leaves_no_temp_file_behind() {
        let dir = repo("failed-write");
        // The target's parent does not exist, so the rename fails; the temp
        // was written beside a path that cannot be replaced.
        let target = dir.join("no/such/dir/X-1.json");
        assert!(write_json_atomic(&target, &serde_json::json!({})).is_err());
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-")).collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_display_paths_do_not_rewrite_shared_project_records() {
        let record: ProjectRecord = serde_json::from_value(json!({"key":"XT","name":"Fixture","source_path":"/studio/project","source_repo":"https://forge.example/team/app.git","created_at":"fixture"})).unwrap();
        let paths = std::collections::HashMap::from([("XT".to_string(), "/tron/project".to_string())]);
        let shown = display_paths(vec![record.clone()], &paths);
        assert_eq!(shown[0].source_path, "/tron/project");
        assert_eq!(shown[0].source_repo, record.source_repo);
        assert_eq!(record.source_path, "/studio/project");
        assert_eq!(display_paths(vec![record], &Default::default())[0].source_path, "/studio/project");
    }

    #[test]
    fn workspace_ticket_count_does_not_read_historical_review_payloads() {
        let dir = std::env::temp_dir().join(format!("xnaut-ticket-count-{}", uuid::Uuid::new_v4()));
        let root = dir.join("projects/XT");
        std::fs::create_dir_all(root.join("tickets")).unwrap();
        std::fs::write(root.join("project.json"), "{}").unwrap();
        // Counting stored ticket files must not parse their potentially huge
        // bodies. Backups, unrelated keys and directories are not tickets.
        for name in ["XT-1.json", "XT-2.json", "OTHER-1.json", "XT-3.json.bak"] {
            std::fs::write(root.join("tickets").join(name), "unread payload").unwrap();
        }
        std::fs::create_dir(root.join("tickets/XT-4.json")).unwrap();
        assert_eq!(ticket_count_in(&dir, "XT").unwrap(), 2);
        assert!(ticket_count_in(&dir, "OTHER").is_err());
        assert!(ticket_count_in(&dir, "../XT").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    /// A scratch control repo with one ticket in it.
    ///
    /// Keyed by the caller's name as well as the pid and thread, because the
    /// suite runs in parallel and two tests sharing a directory would settle
    /// each other's tickets. No env var is involved: every PM write takes an
    /// explicit `repo: &Path`, which is what makes this safe to run alongside
    /// the real control repo.
    fn scratch_repo(name: &str, ticket_id: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "xnaut-pm-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("projects/XNAUT/tickets")).unwrap();
        std::fs::create_dir_all(root.join("events")).unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            std::process::Command::new("git")
                .args(&args)
                .current_dir(&root)
                .output()
                .unwrap();
        }
        // The manifest matters: ticket_list_in enumerates PROJECTS, not
        // directories, so a board with no project.json reads as empty.
        std::fs::write(
            root.join("projects/XNAUT/project.json"),
            serde_json::to_string_pretty(&json!({
                "key": "XNAUT", "name": "xNAUT", "revision": 1,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
            }))
            .unwrap(),
        )
        .unwrap();
        let ticket = json!({
            "id": ticket_id, "project": "XNAUT", "title": ticket_id, "type": "task",
            "status": "in_progress", "priority": "medium", "owner": "@claude",
            "documentation": [], "body": "", "source_id": "",
            "revision": 1, "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
        });
        std::fs::write(
            root.join(format!("projects/XNAUT/tickets/{ticket_id}.json")),
            serde_json::to_string_pretty(&ticket).unwrap(),
        )
        .unwrap();
        root
    }

    // Two independent clones model machines with concurrent, unpushed work.
    // Publish one writer only after the other's commit exists, deterministically
    // exercising the divergent-history window without relying on thread timing.
    fn fleet_writers(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let first = scratch_repo(name, "XNAUT-900");
        let mut second_ticket: TicketRecord =
            read_json(&find_ticket_path(&first, "XNAUT-900").unwrap()).unwrap();
        second_ticket.id = "XNAUT-901".into();
        write_json_atomic(&first.join("projects/XNAUT/tickets/XNAUT-901.json"), &second_ticket).unwrap();
        std::fs::write(first.join("events/.gitkeep"), "").unwrap();
        run_git(&first, &["add", "."]).unwrap();
        run_git(&first, &["commit", "-m", "seed two tickets"]).unwrap();
        // Fixtures inside .git stay out of the control repo working tree.
        let remote_in_git = first.join(".git/fleet-remote.git");
        run_git(&first, &["init", "--bare", remote_in_git.to_str().unwrap()]).unwrap();
        run_git(&first, &["remote", "add", "origin", remote_in_git.to_str().unwrap()]).unwrap();
        run_git(&first, &["push", "-u", "origin", "main"]).unwrap();
        let second = first.join(".git/fleet-second");
        run_git(&first, &["clone", "-b", "main", remote_in_git.to_str().unwrap(), second.to_str().unwrap()]).unwrap();
        run_git(&second, &["config", "user.name", "fleet-test"]).unwrap();
        run_git(&second, &["config", "user.email", "fleet@test"]).unwrap();
        (first, second, remote_in_git)
    }

    fn fleet_update(repo: &Path, id: &str, revision: u64, body: &str) -> Result<TicketRecord, String> {
        ticket_update_with_registry_in(repo, &repo.join(".git/fixture-registry"), TicketUpdateRequest {
            model_requirement: None,
            id: id.into(), expected_revision: revision, title: None,
            ticket_type: None, status: None, priority: None, owner: None,
            clear_owner: false, documentation: None, body: Some(body.into()), caller: None,
        })
    }

    fn commit_fixture_path(repo: &Path, relative: &str, bytes: &[u8]) -> String {
        let path = repo.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        run_git(repo, &["add", "--", relative]).unwrap();
        run_git(repo, &["commit", "--only", "-m", "fixture change", "--", relative]).unwrap();
        run_git(repo, &["rev-parse", "HEAD"]).unwrap()
    }

    #[test]
    fn dirty_fleet_divergence_preserves_staging_events_and_committed_handback() {
        let (first, second, remote) = fleet_writers("dirty-divergence");
        let target = "projects/XNAUT/tickets/XNAUT-900.json";
        let other = "projects/XNAUT/tickets/XNAUT-901.json";
        let mut ticket: TicketRecord = read_json(&first.join(target)).unwrap();
        ticket.handback = Some(a_filed_handback("XNAUT-900"));
        ticket.revision = 2;
        let local = commit_fixture_path(&first, target, &serde_json::to_vec(&ticket).unwrap());
        let remote_commit = commit_fixture_path(&second, "remote-note.md", b"independent fleet edit");
        run_git(&second, &["push", "origin", "main"]).unwrap();
        std::fs::write(first.join(other), b"staged unrelated ticket bytes\n").unwrap();
        run_git(&first, &["add", "--", other]).unwrap();
        std::fs::write(first.join(other), b"newer unstaged ticket bytes\n").unwrap();
        std::fs::write(first.join("events/stray.json"), b"untracked event bytes\0\n").unwrap();
        // A user's auto-stash preference must not move any of those bytes aside.
        run_git(&first, &["config", "merge.autoStash", "true"]).unwrap();
        let staged = run_git(&first, &["diff", "--cached", "--binary"]).unwrap();
        let updated = fleet_update(&first, "XNAUT-900", 2, "ready for independent review").unwrap();
        assert_eq!(updated.revision, 3);
        assert_eq!(updated.handback, ticket.handback);
        assert_eq!(std::fs::read(first.join(other)).unwrap(), b"newer unstaged ticket bytes\n");
        assert_eq!(run_git(&first, &["diff", "--cached", "--binary"]).unwrap(), staged);
        assert_eq!(std::fs::read(first.join("events/stray.json")).unwrap(), b"untracked event bytes\0\n");
        for ancestor in [&local, &remote_commit] {
            run_git(&first, &["merge-base", "--is-ancestor", ancestor, "HEAD"]).unwrap();
        }
        let published: TicketRecord = serde_json::from_str(
            &run_git(&remote, &["show", &format!("main:{target}")]).unwrap(),
        ).unwrap();
        assert_eq!(published.revision, 3);
        assert_eq!(published.handback, ticket.handback);
        assert!(run_git(&remote, &["show", "main:events/stray.json"]).is_err());
        assert!(!run_git(&remote, &["show", &format!("main:{other}")]).unwrap().contains("unrelated ticket bytes"));
        assert!(!first.join(".git/MERGE_HEAD").exists());
        assert!(!first.join(".git/rebase-merge").exists());
        assert!(!run_git(&first, &["worktree", "list", "--porcelain"]).unwrap().contains("xnaut-ticket-reconcile-"));
        std::fs::remove_dir_all(first).unwrap();
    }

    #[test]
    fn dirty_fleet_overlap_refuses_without_overwriting_or_stashing_pending_bytes() {
        let (first, second, _) = fleet_writers("dirty-overlap");
        let other = "projects/XNAUT/tickets/XNAUT-901.json";
        let original = commit_fixture_path(&first, "local-handback.md", b"preserved local history");
        fleet_update(&second, "XNAUT-901", 1, "incoming same path").unwrap();
        std::fs::write(first.join(other), b"pending other ticket").unwrap();
        run_git(&first, &["add", "--", other]).unwrap();
        std::fs::write(first.join(other), b"pending newer other ticket").unwrap();
        std::fs::write(first.join("events/stray.json"), b"keep event").unwrap();
        run_git(&first, &["config", "merge.autoStash", "true"]).unwrap();
        let index = run_git(&first, &["diff", "--cached", "--binary"]).unwrap();
        let ticket = std::fs::read(first.join("projects/XNAUT/tickets/XNAUT-900.json")).unwrap();
        let error = fleet_update(&first, "XNAUT-900", 1, "must not write").unwrap_err();
        assert!(error.contains("pending files") && error.contains("XNAUT-901.json"), "{error}");
        assert_eq!(run_git(&first, &["rev-parse", "HEAD"]).unwrap(), original);
        assert_eq!(run_git(&first, &["diff", "--cached", "--binary"]).unwrap(), index);
        assert_eq!(std::fs::read(first.join(other)).unwrap(), b"pending newer other ticket");
        assert_eq!(std::fs::read(first.join("events/stray.json")).unwrap(), b"keep event");
        assert_eq!(std::fs::read(first.join("projects/XNAUT/tickets/XNAUT-900.json")).unwrap(), ticket);
        assert!(run_git(&first, &["stash", "list"]).unwrap().is_empty());
        assert!(!first.join(".git/MERGE_HEAD").exists());
        std::fs::remove_dir_all(first).unwrap();
    }

    #[test]
    fn dirty_fleet_reconciliation_still_refuses_a_stale_target_revision() {
        let (first, second, _) = fleet_writers("dirty-stale-target");
        let local = commit_fixture_path(&first, "local-handback.md", b"local evidence");
        fleet_update(&second, "XNAUT-900", 1, "newer target record").unwrap();
        let other = first.join("projects/XNAUT/tickets/XNAUT-901.json");
        std::fs::write(&other, b"unrelated pending work").unwrap();
        let error = fleet_update(&first, "XNAUT-900", 1, "stale overwrite").unwrap_err();
        assert!(error.contains("expected revision 1, current revision 2"), "{error}");
        let target: TicketRecord = read_json(&first.join("projects/XNAUT/tickets/XNAUT-900.json")).unwrap();
        assert_eq!(target.body, "newer target record");
        assert_eq!(std::fs::read(&other).unwrap(), b"unrelated pending work");
        run_git(&first, &["merge-base", "--is-ancestor", &local, "HEAD"]).unwrap();
        std::fs::remove_dir_all(first).unwrap();
    }

    #[test]
    fn dirty_fleet_committed_conflict_preserves_original_handback_and_index() {
        let (first, second, _) = fleet_writers("dirty-committed-conflict");
        let target = "projects/XNAUT/tickets/XNAUT-900.json";
        let mut ticket: TicketRecord = read_json(&first.join(target)).unwrap();
        ticket.handback = Some(a_filed_handback("XNAUT-900"));
        ticket.revision = 2;
        let original = commit_fixture_path(&first, target, &serde_json::to_vec(&ticket).unwrap());
        fleet_update(&second, "XNAUT-900", 1, "competing remote record").unwrap();
        std::fs::write(first.join("events/pending.json"), b"pending event").unwrap();
        let error = fleet_update(&first, "XNAUT-900", 2, "must not write").unwrap_err();
        assert!(error.contains("committed control history") && error.contains(target), "{error}");
        assert_eq!(run_git(&first, &["rev-parse", "HEAD"]).unwrap(), original);
        assert_eq!(read_json::<TicketRecord>(&first.join(target)).unwrap().handback, ticket.handback);
        assert!(run_git(&first, &["diff", "--cached"]).unwrap().is_empty());
        assert_eq!(std::fs::read(first.join("events/pending.json")).unwrap(), b"pending event");
        assert!(!first.join(".git/MERGE_HEAD").exists());
        assert!(!run_git(&first, &["worktree", "list", "--porcelain"]).unwrap().contains("xnaut-ticket-reconcile-"));
        std::fs::remove_dir_all(first).unwrap();
    }

    #[test]
    fn dirty_fleet_checkout_moving_during_scratch_work_is_refused() {
        let (first, _, _) = fleet_writers("dirty-moved-head");
        let original = run_git(&first, &["rev-parse", "HEAD"]).unwrap();
        let moved = commit_fixture_path(&first, "concurrent.md", b"another writer's commit");
        let error = finish_ticket_reconciliation(&first, "main", &original, &original).unwrap_err();
        assert!(error.contains("changed during ticket reconciliation"), "{error}");
        assert_eq!(run_git(&first, &["rev-parse", "HEAD"]).unwrap(), moved);
        assert_eq!(std::fs::read(first.join("concurrent.md")).unwrap(), b"another writer's commit");
        std::fs::remove_dir_all(first).unwrap();
    }

    #[test]
    fn portable_ticket_update_lease_excludes_processes_and_releases_on_drop() {
        crate::run_control::tests::cross_process_lock_fixture(
            "project_management::tests::portable_ticket_update_lease_excludes_processes_and_releases_on_drop",
            |root| root.join(".git/xnaut-ticket-update.lock"),
            |root| {
                run_git(root, &["init", "--quiet"]).unwrap();
                TicketUpdateLease::acquire(root).unwrap()
            },
        );
    }

    /// Tron, 2026-09-20: XNAUT-394's file sat modified after an interrupted
    /// write; the repo was level with origin; XNAUT-379's green verify could
    /// not settle because `git rebase origin/main` refuses on any unstaged
    /// file even with nothing to rebase. Another ticket's dirt must not
    /// block a write when there is nothing to pull.
    #[test]
    fn another_tickets_stranded_write_does_not_block_an_update_when_level_with_origin() {
        let (first, _second, _remote) = fleet_writers("dirt-level");
        let stranded = first.join("projects/XNAUT/tickets/XNAUT-901.json");
        let mut other: TicketRecord = read_json(&stranded).unwrap();
        other.body = "half written, never committed".into();
        write_json_atomic(&stranded, &other).unwrap();
        std::fs::write(first.join("events/stray.json"), "{}").unwrap();
        let before = read_json::<TicketRecord>(&find_ticket_path(&first, "XNAUT-900").unwrap()).unwrap();

        let moved = fleet_update(&first, "XNAUT-900", before.revision, "written past the dirt").unwrap();
        assert_eq!(moved.body, "written past the dirt");

        // The other ticket's dirt is still there, untouched and uncommitted.
        let still: TicketRecord = read_json(&stranded).unwrap();
        assert_eq!(still.body, "half written, never committed");
        let status = run_git(&first, &["status", "--porcelain"]).unwrap();
        assert!(status.contains("XNAUT-901.json"), "{status}");
        assert!(!status.contains("XNAUT-900.json"), "the write itself was committed: {status}");
        std::fs::remove_dir_all(&first).unwrap();
    }

    #[test]
    fn release_is_its_own_field_empty_means_unassigned_and_every_change_is_an_event() {
        // André, 2026-09-08: a consistent field, an event with previous and
        // new on change, empty allowed for Unassigned.
        let repo = scratch_repo("release-field", "XNAUT-900");
        // Keep the cleanup regression inside an owned container: even a
        // mistaken parent removal must never reach the machine's temp root.
        let sandbox = repo.with_extension("isolation");
        std::fs::create_dir(&sandbox).unwrap();
        let isolated_repo = sandbox.join("repo");
        std::fs::rename(&repo, &isolated_repo).unwrap();
        let repo = isolated_repo;
        let neighbor = sandbox.join("unrelated-fixture");
        std::fs::write(&neighbor, "keep").unwrap();
        let before = read_json::<TicketRecord>(&find_ticket_path(&repo, "XNAUT-900").unwrap()).unwrap();
        assert_eq!(before.release, "", "unassigned by default");
        let events = |repo: &Path| -> Vec<serde_json::Value> {
            let mut out = vec![];
            for e in std::fs::read_dir(repo.join("events")).unwrap().flatten() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(e.path()).unwrap_or_default()) {
                    if v["event"] == "ticket.release" { out.push(v); }
                }
            }
            out
        };
        let set = ticket_release_in(&repo, "XNAUT-900", "1.26.3").unwrap();
        assert_eq!(set.release, "1.26.3");
        let ev = events(&repo);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0]["details"]["previous"], "");
        assert_eq!(ev[0]["details"]["new"], "1.26.3");
        // Same value again: no write, no event.
        let again = ticket_release_in(&repo, "XNAUT-900", "1.26.3").unwrap();
        assert_eq!(again.revision, set.revision);
        assert_eq!(events(&repo).len(), 1);
        // Back to unassigned is a change like any other.
        let cleared = ticket_release_in(&repo, "XNAUT-900", "").unwrap();
        assert_eq!(cleared.release, "");
        let ev = events(&repo);
        assert_eq!(ev.len(), 2);
        assert!(ev.iter().any(|e| e["details"]["previous"] == "1.26.3" && e["details"]["new"] == ""));
        // Tags are separate and free-form.
        let tagged = ticket_tag_in(&repo, "XNAUT-900", "area:jury", false).unwrap();
        assert_eq!(tagged.tags, vec!["area:jury"]);
        assert_eq!(ticket_tag_in(&repo, "XNAUT-900", "area:jury", true).unwrap().tags.len(), 0);
        std::fs::remove_dir_all(&repo).unwrap();
        assert_eq!(std::fs::read_to_string(&neighbor).unwrap(), "keep");
        std::fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn a_ticket_write_is_on_the_remote_before_the_call_returns() {
        // The whole class of "rebase conflict" failures on 2026-09-07 came
        // from writes that were committed and never pushed. A write is not
        // done until the other machine can read it.
        let (first, second, remote) = fleet_writers("fleet-publish");
        let updated = fleet_update(&first, "XNAUT-900", 1, "written on tron").unwrap();
        let remote_ticket: TicketRecord = serde_json::from_str(
            &run_git(&remote, &["show", "main:projects/XNAUT/tickets/XNAUT-900.json"]).unwrap(),
        )
        .unwrap();
        assert_eq!(remote_ticket.revision, updated.revision, "the write never reached the remote");
        assert_eq!(remote_ticket.body, "written on tron");
        // And the other machine's next write sees it without a collision.
        let from_second = fleet_update(&second, "XNAUT-900", updated.revision, "then the Studio").unwrap();
        assert_eq!(from_second.revision, updated.revision + 1);
    }

    #[test]
    fn a_rejected_push_does_not_fail_the_write() {
        // Remote ahead and not fetched: the push is refused, the local commit
        // stands, and the next write's rebase reconciles. The write itself
        // must still succeed, or an agent retries a handback that already
        // exists.
        let (first, second, _remote) = fleet_writers("fleet-publish-rejected");
        fleet_update(&second, "XNAUT-901", 1, "Studio first").unwrap();
        // `first` is now behind. Write a DIFFERENT ticket there: the rebase in
        // ticket_update_in brings it up to date, so this push succeeds; then
        // make the remote move again underneath and confirm a stale push is
        // survivable by calling publish directly on a repo that is behind.
        run_git(&first, &["fetch", "origin"]).unwrap();
        fleet_update(&first, "XNAUT-900", 1, "tron").unwrap();
        fleet_update(&second, "XNAUT-901", 2, "Studio again").unwrap();
        // first is behind again; a direct publish must not panic or error out.
        publish(&first);
        assert!(run_git(&first, &["status", "--porcelain"]).unwrap().is_empty());
        assert!(!first.join(".git/rebase-merge").exists());
    }

    #[test]
    fn fleet_different_tickets_preserve_both_writers() {
        let (first, second, remote) = fleet_writers("fleet-different");
        let handback = a_filed_handback("XNAUT-900");
        file_handback_in(&first, &handback).unwrap();
        fleet_update(&second, "XNAUT-901", 1, "Studio edit").unwrap();
        run_git(&second, &["push", "origin", "main"]).unwrap();

        let updated = fleet_update(&first, "XNAUT-900", 2, "Agent follow-up").unwrap();
        assert_eq!(updated.handback.as_ref(), Some(&handback));
        run_git(&first, &["push", "origin", "main"]).expect("both histories must fast-forward onto the remote");
        let remote_ticket = |id: &str| -> TicketRecord {
            serde_json::from_str(&run_git(&remote, &["show", &format!("main:projects/XNAUT/tickets/{id}.json")]).unwrap()).unwrap()
        };
        assert_eq!(remote_ticket("XNAUT-900").handback, Some(handback));
        assert_eq!(remote_ticket("XNAUT-901").body, "Studio edit");
        assert_eq!(run_git(&first, &["log", "--format=%s"]).unwrap().lines().count(), 4);
        std::fs::remove_dir_all(first).unwrap();
    }

    #[test]
    fn fleet_same_ticket_reports_conflict_and_preserves_handback() {
        let (first, second, remote) = fleet_writers("fleet-conflict");
        let handback = a_filed_handback("XNAUT-900");
        // The handback's push must FAIL here: this test is the unpushed-commit
        // case that `publish` normally prevents (the remote is unreachable
        // for that one write), so the divergence it guards can still occur.
        let remote_url = run_git(&first, &["remote", "get-url", "origin"]).unwrap();
        run_git(&first, &["remote", "set-url", "origin", "/nonexistent/remote.git"]).unwrap();
        file_handback_in(&first, &handback).unwrap();
        run_git(&first, &["remote", "set-url", "origin", &remote_url]).unwrap();
        let original_head = run_git(&first, &["rev-parse", "HEAD"]).unwrap();
        fleet_update(&second, "XNAUT-900", 1, "Studio competing edit").unwrap();
        run_git(&second, &["push", "origin", "main"]).unwrap();

        let error = fleet_update(&first, "XNAUT-900", 2, "must not be written")
            .expect_err("a divergent same-ticket write must tell its caller");
        assert!(error.contains("after two rebase attempts"), "{error}");
        assert_eq!(run_git(&first, &["reflog", "--format=%gs"]).unwrap()
            .lines().filter(|line| line.starts_with("rebase (abort)")).count(), 2,
            "both failed attempts must have been aborted");
        assert_eq!(run_git(&first, &["rev-parse", "HEAD"]).unwrap(), original_head);
        assert!(run_git(&first, &["status", "--porcelain"]).unwrap().is_empty());
        assert!(!first.join(".git/rebase-merge").exists());
        let local: TicketRecord = read_json(&find_ticket_path(&first, "XNAUT-900").unwrap()).unwrap();
        assert_eq!(local.handback, Some(handback));
        assert_eq!(local.body, "");
        let published: TicketRecord = serde_json::from_str(&run_git(&remote, &["show", "main:projects/XNAUT/tickets/XNAUT-900.json"]).unwrap()).unwrap();
        assert_eq!(published.body, "Studio competing edit");
        std::fs::remove_dir_all(first).unwrap();
    }

    #[test]
    fn fleet_same_ticket_stale_revision_is_refused_after_fetch() {
        let (first, second, _) = fleet_writers("fleet-stale");
        fleet_update(&second, "XNAUT-900", 1, "published edit").unwrap();
        run_git(&second, &["push", "origin", "main"]).unwrap();
        let error = fleet_update(&first, "XNAUT-900", 1, "stale edit").unwrap_err();
        assert!(error.contains("expected revision 1, current revision 2"), "{error}");
        let local: TicketRecord = read_json(&find_ticket_path(&first, "XNAUT-900").unwrap()).unwrap();
        assert_eq!(local.body, "published edit");
        assert!(run_git(&first, &["status", "--porcelain"]).unwrap().is_empty());
        std::fs::remove_dir_all(first).unwrap();
    }

    fn a_filed_handback(ticket: &str) -> crate::handback::Handback {
        crate::handback::Handback {
            run_id: None,
            ticket: ticket.into(),
            summary: "orphaned verify runs are reaped at boot".into(),
            files_changed: vec!["src-tauri/src/sandbox_verify.rs".into()],
            commits: vec!["832e5aecafe1".into()],
            how_verified: "cargo test --bin xnaut: 809 passed, 0 failed".into(),
            verify_record_id: None,
            not_finished: Some("nothing".into()),
            confidence: crate::handback::Confidence::High,
            from: "claude".into(),
            submitted_at: "2026-09-05T10:00:00Z".into(),
        }
    }

    #[test]
    fn ticket_done_and_review_signal_the_live_run_in_the_production_update_path() {
        for status in ["done", "review"] {
            for alive in [true, false] {
                let root = scratch_repo(&format!("completion-{status}-{alive}"), "XNAUT-900");
                let registry = root.join(".git/registry");
                let mut run = crate::run_control::tests::run();
                run.agent_handle = "claude".into();
                if alive {
                    run.pid = Some(std::process::id());
                    run.process_birth = crate::run_control::process_birth(std::process::id());
                }
                let run = crate::run_control::request_in(&registry, run, || Ok(())).unwrap();
                let request = serde_json::from_value(json!({
                    "id": "XNAUT-900", "expected_revision": 1,
                    "status": status, "caller": "claude"
                }))
                .unwrap();
                let ticket = ticket_update_with_registry_in(&root, &registry, request).unwrap();
                assert_eq!(ticket.status, status);
                assert_eq!(ticket.owner.as_deref(), Some("nautbot"));
                let after = crate::run_control::load_manifest_in(&registry, &run.run_id).unwrap();
                assert_eq!(
                    after.state,
                    if alive {
                        crate::run_control::RunState::Done
                    } else {
                        crate::run_control::RunState::Starting
                    }
                );
                std::fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn a_handback_survives_being_written_and_read_back() {
        // The restart test. The handback is only worth filing if it is still
        // there when the session that filed it is gone, so this reads the
        // ticket back off disk through the same loader the app uses at boot
        // rather than inspecting the returned value.
        let root = scratch_repo("attach", "XNAUT-900");
        let filed = a_filed_handback("XNAUT-900");
        let Filing::Filed { ticket: written, .. } =
            file_handback_in(&root, &filed).expect("file")
        else {
            panic!("a complete handback was refused");
        };
        assert_eq!(written.revision, 2, "the revision did not move");

        let reloaded = ticket_list_in(&root, None)
            .expect("list")
            .into_iter()
            .find(|t| t.id == "XNAUT-900")
            .expect("the ticket vanished");
        assert_eq!(
            reloaded.handback.as_ref(),
            Some(&filed),
            "the handback did not survive the round trip to disk"
        );

        // And the three things a valid control-repo change is made of: the
        // ticket JSON, an event, and a commit.
        let events: Vec<_> = std::fs::read_dir(root.join("events"))
            .expect("events dir")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(events.len(), 1, "the handback wrote no event file");
        let event: Value =
            serde_json::from_str(&std::fs::read_to_string(events[0].path()).unwrap()).unwrap();
        assert_eq!(event["event"], "ticket.handback");
        assert_eq!(event["subject"], "XNAUT-900");
        let log = std::process::Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&log.stdout).contains("handback for XNAUT-900"),
            "the handback was never committed"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_ticket_without_a_handback_does_not_grow_an_empty_key() {
        // 300 tickets are 300 files in git. A null on each is 300 lines of
        // diff saying nothing, which is why the field is skip_serializing_if.
        let root = scratch_repo("nokey", "XNAUT-901");
        let raw =
            std::fs::read_to_string(root.join("projects/XNAUT/tickets/XNAUT-901.json")).unwrap();
        let record: TicketRecord = serde_json::from_str(&raw).expect("a ticket predating the field");
        assert_eq!(record.handback, None, "an old ticket parsed with a handback");
        let round_tripped = serde_json::to_string(&record).unwrap();
        assert!(
            !round_tripped.contains("handback"),
            "rewriting an old ticket added a handback key: {round_tripped}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_handback_for_a_ticket_that_does_not_exist_is_refused() {
        let root = scratch_repo("missing", "XNAUT-902");
        let error = file_handback_in(&root, &a_filed_handback("XNAUT-999"))
            .expect_err("a handback for a nonexistent ticket was accepted");
        assert!(error.to_lowercase().contains("xnaut-999"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unreviewable_handback_writes_nothing_at_all() {
        // The rail, at the one place both callers go through. A half-write
        // would leave a record on the ticket that reads as a report and is
        // not one, which is worse than the prose it replaced.
        let root = scratch_repo("refused", "XNAUT-903");
        let mut weak = a_filed_handback("XNAUT-903");
        weak.how_verified = "tests pass".into();

        let Filing::Refused(verdict) = file_handback_in(&root, &weak).expect("file") else {
            panic!("\"tests pass\" was stored as a verification");
        };
        assert!(verdict.blocking().any(|gap| gap.field == "how_verified"));

        let ticket = ticket_list_in(&root, None)
            .expect("list")
            .into_iter()
            .find(|t| t.id == "XNAUT-903")
            .expect("the ticket vanished");
        assert_eq!(ticket.handback, None, "a refused handback was stored anyway");
        assert_eq!(ticket.revision, 1, "a refusal moved the revision");
        let events = std::fs::read_dir(root.join("events")).expect("events dir").count();
        assert_eq!(events, 0, "a refusal wrote an event");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_write_is_only_reachable_through_the_check() {
        // XNAUT-243's lesson as a test: the two callers must not be able to
        // reach the store while skipping the review. Enforced by privacy, so
        // this asserts the shape of the module rather than a behaviour.
        let source = include_str!("project_management.rs");
        // Needles built at runtime, never written as literals: a literal here
        // appears in this file and the search finds ITSELF, which turns the
        // negative assertion into a permanent failure and the positive one
        // into a permanent pass.
        let name = "attach_handback_in";
        assert!(
            source.contains(&format!("fn {name}")),
            "the raw write was renamed; check this test still guards it"
        );
        assert!(
            !source.contains(&format!("pub fn {name}")),
            "the raw write is public again, so a caller can store an unreviewed handback"
        );
        for caller in [
            include_str!("agent_hooks.rs"),
            include_str!("agent_tools.rs"),
        ] {
            assert!(
                !caller.contains(name),
                "a caller reaches the raw write instead of file_handback_in"
            );
        }
    }

    /// XNAUT-243: the rails must hold at the SHARED write, because the chat
    /// tool and the MCP tool are two callers of one vocabulary and only one
    /// of them used to carry them. Dogfooding found this the hard way: an
    /// agent finished a ticket through MCP, set done, and the ticket stayed
    /// owned by the worker.
    ///
    /// The handback, exercised rather than grepped. Five attempts were spent
    /// fixing this by reading code and asserting on source text, and each was
    /// correct about a path that was not the one running. This calls the real
    /// write on a real repo and looks at the resulting file.
    #[test]
    fn an_agent_finishing_a_ticket_really_hands_it_back() {
        let root = std::env::temp_dir().join(format!(
            "xnaut-handback-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("projects/XNAUT/tickets")).unwrap();
        std::fs::create_dir_all(root.join("events")).unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            std::process::Command::new("git")
                .args(&args)
                .current_dir(&root)
                .output()
                .unwrap();
        }

        let write = |id: &str, status: &str, owner: &str| {
            let ticket = serde_json::json!({
                "id": id, "project": "XNAUT", "title": id, "type": "task",
                "status": status, "priority": "medium", "owner": owner,
                "documentation": [], "body": "", "source_id": "",
                "revision": 1, "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
            });
            std::fs::write(
                root.join(format!("projects/XNAUT/tickets/{id}.json")),
                serde_json::to_string_pretty(&ticket).unwrap(),
            )
            .unwrap();
        };

        let finish = |id: &str, status: &str, caller: Option<&str>| {
            ticket_update_in(
                &root,
                TicketUpdateRequest {
                    model_requirement: None,
                    id: id.into(),
                    expected_revision: 1,
                    title: None,
                    ticket_type: None,
                    status: Some(status.into()),
                    priority: None,
                    owner: None,
                    clear_owner: false,
                    documentation: None,
                    body: None,
                    caller: caller.map(str::to_string),
                },
            )
        };

        // Both of the agent's words hand the ticket back, in the same write.
        write("XNAUT-1", "in_progress", "claude");
        assert_eq!(
            finish("XNAUT-1", "done", Some("claude")).unwrap().owner.as_deref(),
            Some("nautbot"),
            "done must hand back"
        );
        write("XNAUT-2", "in_progress", "claude");
        assert_eq!(
            finish("XNAUT-2", "review", Some("claude")).unwrap().owner.as_deref(),
            Some("nautbot"),
            "review is the same claim and must hand back too"
        );
        // An agent cannot say complete.
        write("XNAUT-3", "done", "claude");
        assert!(
            finish("XNAUT-3", "complete", Some("claude")).is_err(),
            "an agent must not mark its own homework"
        );
        // NautBot cannot either, by hand: complete is what a green verify
        // earns. On XNAUT-107 it set complete 41 s after its verify failed.
        write("XNAUT-4", "done", "nautbot");
        let refused = finish("XNAUT-4", "complete", Some("nautbot")).expect_err("no green run, no complete");
        assert!(refused.contains("no passed sandbox verify"), "{refused}");
        // The verify rail, holding the passed record, may.
        assert_eq!(
            as_verified_settle(|| finish("XNAUT-4", "complete", Some("nautbot")))
                .unwrap()
                .status,
            "complete"
        );
        // The owner's own UI is unattributed and ungated: it may set anything
        // and its writes are not rewritten underneath it.
        write("XNAUT-5", "in_progress", "andre");
        let by_owner = finish("XNAUT-5", "done", None).unwrap();
        assert_eq!(by_owner.owner.as_deref(), Some("andre"), "the UI is not an agent");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_handback_announces_itself() {
        // Andre 2026-08-29: a ticket came back correctly and then sat there,
        // because reassigning an owner is a WRITE and a write tells nobody.
        // The same transition now nudges NautBot, or leaves an inbox item
        // when NautBot is not running.
        let source = include_str!("project_management.rs");
        let body = source
            .split("pub fn ticket_update_in")
            .nth(1)
            .expect("ticket_update_in exists");
        let head = body.split("/// What happened to a filed handback.").next().unwrap();
        assert!(head.contains("handed_back = true"), "the handback stopped being recorded");
        // Both words hand back. XNAUT-233 sat in `review` owned by the agent
        // that finished it, because only `done` was covered.
        assert!(
            head.contains(r#"Some("done") | Some("review")"#),
            "review stopped handing the ticket back"
        );
        assert!(head.contains("announce_handback"), "the handback stopped announcing itself");
        // And it must never start an agent on its own: that is a spend
        // decision, and nobody made it.
        let announce = source
            .split("fn announce_handback")
            .nth(1)
            .expect("announce_handback exists");
        let announce = &announce[..announce.len().min(2000)];
        assert!(
            !announce.contains("agent_profile_launch") && !announce.contains("cold_launch"),
            "the handback must not cold-launch NautBot"
        );
    }

    #[test]
    fn done_hands_back_and_complete_is_nautbots_word_at_the_shared_write() {
        let source = include_str!("project_management.rs");
        let body = source
            .split("pub fn ticket_update_in")
            .nth(1)
            .expect("ticket_update_in exists");
        // The window only has to cover the shared write's guard block; it
        // grew when the reconcile gained its XNAUT-412 and -414 comments.
        let head = &body[..body.len().min(6000)];
        assert!(
            head.contains("RESERVED_NAUTBOT_HANDLE"),
            "the rails left the shared write"
        );
        assert!(
            head.contains("foreign_complete_refusal"),
            "the complete guard left the shared write"
        );
        // The refusal text itself lives in exactly one place, the shared
        // foreign_complete_refusal, so the two paths cannot drift.
        assert_eq!(
            source.matches("only NautBot can set a ticket to complete").count(),
            3, // the helper, plus this test's own two assertion literals
            "the complete refusal must have exactly one implementation"
        );
        // And no caller may keep a private copy: a second implementation is
        // how the two paths drifted apart in the first place. The chat loop
        // calls the shared refusal BEFORE opening the repo instead.
        let tools = include_str!("agent_tools.rs");
        assert!(
            !tools.contains("only NautBot can set a ticket to complete"),
            "agent_tools re-grew its own copy of the complete guard"
        );
        assert!(
            tools.contains("foreign_complete_refusal"),
            "the chat loop stopped using the shared complete guard"
        );
    }


    use super::*;

    #[test]
    fn project_manifest_tolerates_null_strings() {
        // Regression: a hand-authored `"task_id": null` (DATFLOW) used to fail
        // the whole manifest and blank the Projects board.
        let json = r#"{
            "key": "DATFLOW", "name": "DAT Stream",
            "task_id": null, "owner": null, "forge_remote": null,
            "created_at": "2026-07-28T07:55:00Z"
        }"#;
        let project: ProjectRecord = serde_json::from_str(json).unwrap();
        assert_eq!(project.key, "DATFLOW");
        assert_eq!(project.task_id, "");
        assert_eq!(project.owner, "");
        assert_eq!(project.forge_remote, "");
    }

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

    // XNAUT-87. This test used to assert the opposite — that a manifest with no
    // stage field reads back as "idea". That default was the backend half of
    // the fabricated stage on the project overview: every project acquired a
    // position in NautFlow whether or not anyone had run it through one, and
    // the page then reported that position as state.
    #[test]
    fn a_manifest_without_a_stage_is_in_no_flow() {
        let project: ProjectRecord = serde_json::from_value(json!({
            "key": "XNAUT",
            "name": "xNaut",
            "created_at": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        assert_eq!(project.flow_type, "standard");
        assert_eq!(
            project.stage, "",
            "a manifest with no stage was given one it never had"
        );
        assert_eq!(project.revision, 1);
        assert!(project.purpose.is_empty());
        assert!(project.budget_chf.is_none());
    }

    // An explicit null is not a missing key to serde, and stage is a String —
    // the trap CLAUDE.md records as having once blanked the whole Projects
    // board. Both spellings of absence must land on the same answer.
    #[test]
    fn a_null_stage_is_read_as_no_flow_rather_than_refused() {
        let project: ProjectRecord = serde_json::from_value(json!({
            "key": "XNAUT",
            "name": "xNaut",
            "stage": null,
            "created_at": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        assert_eq!(project.stage, "");
    }

    // A project that IS in a flow still round-trips its stage. The fix is about
    // absence; it must not start dropping real values.
    #[test]
    fn a_manifest_with_a_stage_keeps_it() {
        let project: ProjectRecord = serde_json::from_value(json!({
            "key": "XNAUT",
            "name": "xNaut",
            "stage": "build",
            "created_at": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        assert_eq!(project.stage, "build");
    }

    // The case the whole ticket is about: NautFlow is opt-in, so a project has
    // to be able to get back out of it. An explicit empty stage is that exit,
    // and it used to be swallowed as "field omitted".
    #[test]
    fn an_explicit_empty_stage_takes_a_project_out_of_the_flow() {
        assert_eq!(
            next_project_stage("build", Some(""), "standard", false).unwrap(),
            "",
            "a project asked to leave NautFlow was kept in it"
        );
        // Whitespace is a person clearing a field, not a stage named "  ".
        assert_eq!(
            next_project_stage("build", Some("   "), "standard", false).unwrap(),
            ""
        );
    }

    #[test]
    fn omitting_the_stage_leaves_it_exactly_as_it_was() {
        assert_eq!(
            next_project_stage("build", None, "standard", false).unwrap(),
            "build"
        );
        assert_eq!(next_project_stage("", None, "standard", false).unwrap(), "");
    }

    // Switching flow re-bases a project that is ON a track, because stage keys
    // belong to one track. A project on no track must not be pulled onto one by
    // a flow-type change — that would be the fabricated stage all over again,
    // arriving through the Settings form instead of through the renderer.
    #[test]
    fn changing_flow_rebases_a_staged_project_and_leaves_an_unstaged_one_alone() {
        assert_eq!(
            next_project_stage("prd", None, "incident", true).unwrap(),
            "intake"
        );
        assert_eq!(
            next_project_stage("intake", None, "standard", true).unwrap(),
            "idea"
        );
        assert_eq!(
            next_project_stage("", None, "incident", true).unwrap(),
            "",
            "changing the flow type put a project into a flow it never entered"
        );
    }

    #[test]
    fn a_requested_stage_must_belong_to_the_projects_own_flow() {
        assert_eq!(
            next_project_stage("", Some("build"), "standard", false).unwrap(),
            "build"
        );
        // "concept" is a standard-track stage; the incident track has no such
        // thing, and accepting it would store a stage nothing can render.
        assert!(next_project_stage("", Some("concept"), "incident", false).is_err());
        assert!(next_project_stage("", Some("nonsense"), "standard", false).is_err());
    }

    // Creating a project through the New Project form IS opting into the flow,
    // so that path keeps its starting stage. The fix must not turn the flow off
    // for the people who asked for it.
    #[test]
    fn a_project_created_through_the_form_still_starts_in_its_flow() {
        assert_eq!(default_project_stage(), "idea");
        assert!(stage_keys("standard").contains(&"idea"));
        assert!(stage_keys("incident").contains(&"intake"));
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

    // The backend half of the fabricated stage: imports and migrations used to
    // stamp "idea" on projects nobody had ever run through NautFlow, which is
    // most of them. Kept separate from the idempotency test above so a failure
    // names the stage rather than the import.
    #[test]
    fn imported_and_migrated_projects_carry_no_stage() {
        let root = std::env::temp_dir().join(format!("xnaut-pm-test-{}", uuid::Uuid::new_v4()));
        let repo = root.join("control");
        initialize_local_repo_transactional(&repo, "control").unwrap();
        let task = crate::tasks::TaskSession {
            id: "task-stageless".into(),
            name: "Stageless".into(),
            kind: "project".into(),
            path: "/tmp/Stageless".into(),
            zellij_session: String::new(),
            agent_id: None,
            created: "2026-01-01T00:00:00Z".into(),
            project_type: None,
            forge_remote: None,
        };
        let imported = import_task_projects(&repo, &[task]).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(
            imported[0].stage, "",
            "an imported project was stamped with a stage it never reached"
        );

        let todos = std::collections::HashMap::from([(
            "orphan-task".into(),
            vec![crate::project_todos::Todo {
                id: "todo-orphan".into(),
                text: "Preserve removed work".into(),
                done: true,
                created: "2026-01-03T00:00:00Z".into(),
            }],
        )]);
        let migrated = migrate_legacy_pm_data(&repo, &[], &todos).unwrap();
        assert!(!migrated.is_empty(), "the migration produced no project");
        for project in &migrated {
            assert_eq!(
                project.stage, "",
                "migrated project {} was stamped with a stage",
                project.key
            );
        }
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
                owner_only: false,
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
                fleet: false,
                client: None,
                issue_intake: Default::default(),
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
}
