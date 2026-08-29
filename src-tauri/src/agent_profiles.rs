use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

const PROFILE_STORE_VERSION: u32 = 1;
pub const RESERVED_NAUTBOT_HANDLE: &str = "nautbot";
const DEFAULT_ACCENT_COLOR: &str = "#f5b840";

/// Persistent agent identity. Runtime mechanics remain in `agents.rs`; this
/// record is the human- and mesh-facing identity that references a runtime.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentProfile {
    pub handle: String,
    pub display_name: String,
    pub tagline: String,
    pub purpose: String,
    pub runtime_id: String,
    pub provider: String,
    /// The model the RUNTIME is launched with: it becomes `--model <value>` on
    /// the CLI, so it has to be a name that CLI knows.
    pub model: String,
    /// The model the in-app chat turn uses, when it differs.
    ///
    /// These were one field, and that conflated two unrelated things: chat goes
    /// over an OpenAI-compatible API where what matters is whether the ROUTE can
    /// carry tool calls, while a launch hands the string to codex or claude,
    /// where what matters is whether that CLI knows the name. Pointing NautBot's
    /// chat at a local model to get tool calls back (XNAUT-195) would otherwise
    /// have handed `codex --model lmstudio/qwen/...` to a CLI that has never
    /// heard of it. Empty means "same as `model`", so every existing profile
    /// keeps behaving exactly as it did.
    #[serde(default)]
    pub chat_model: String,
    #[serde(default)]
    pub reasoning_effort: String,
    #[serde(default)]
    pub execution: AgentExecution,
    pub role: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_notifications")]
    pub notifications: bool,
    #[serde(default = "default_accent_color")]
    pub accent_color: String,
    /// Least privilege per agent: a planner has no business holding a shell.
    /// Defaults reproduce today's behaviour so existing profiles are unchanged.
    #[serde(default)]
    pub policy: crate::policy::AgentPolicy,
    pub default_project: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

impl AgentProfile {
    /// The model an in-app chat turn should use.
    ///
    /// `chat_model` when it is set, otherwise `model`. Empty means "unchanged",
    /// which is what every profile written before this field existed says.
    pub fn chat_model_or_model(&self) -> &str {
        if self.chat_model.trim().is_empty() {
            self.model.trim()
        } else {
            self.chat_model.trim()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentExecution {
    #[default]
    Local,
    Sandbox,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct AgentProfileStore {
    #[serde(default = "profile_store_version")]
    version: u32,
    #[serde(default)]
    profiles: Vec<AgentProfile>,
}

fn profile_store_version() -> u32 {
    PROFILE_STORE_VERSION
}

fn default_notifications() -> bool {
    true
}

fn default_accent_color() -> String {
    DEFAULT_ACCENT_COLOR.to_string()
}

/// Brand yellow belongs to NautBot alone — it is the master and orchestrator,
/// and an identity colour only reads as identity if it is not shared. Every
/// other seeded agent takes a stable colour derived from its handle, so the
/// list is scannable and a given agent keeps its colour across restarts.
fn seeded_accent_color(handle: &str) -> String {
    const PALETTE: [&str; 6] = [
        "#b49ae8", // violet
        "#7fa6d9", // steel blue
        "#10b981", // green
        "#e8896b", // clay
        "#6bc7e8", // cyan
        "#d98cc4", // orchid
    ];
    if handle == "nautbot" {
        return DEFAULT_ACCENT_COLOR.to_string();
    }
    let sum: u32 = handle.bytes().map(u32::from).sum();
    PALETTE[(sum as usize) % PALETTE.len()].to_string()
}

/// Request used by the identity-aware launcher. It deliberately mirrors the
/// existing runtime launch request, replacing `agent_id` with a profile handle.
#[derive(Clone, Debug, Deserialize)]
pub struct LaunchAgentProfileRequest {
    pub handle: String,
    pub worktree_path: String,
    pub prompt: Option<String>,
    #[serde(default)]
    pub conversation_mode: bool,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub resume: bool,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentAccess {
    pub read: Vec<String>,
    pub write: Vec<String>,
    pub denied: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentRuntime {
    pub provider: String,
    pub model: String,
    pub mode: String,
}

impl Default for AgentRuntime {
    fn default() -> Self {
        Self {
            provider: "global".to_string(),
            model: String::new(),
            mode: "chat".to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LegacyAgentProfile {
    pub id: String,
    pub name: String,
    pub status: String,
    pub version: u32,
    pub role: String,
    pub runtime: AgentRuntime,
    pub skills: Vec<String>,
    pub access: AgentAccess,
    pub tools: Vec<String>,
    pub constraints: Vec<String>,
    pub outputs: Vec<String>,
    pub body: String,
    pub rel: String,
    pub built_in: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentCatalogItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub version: u32,
    pub role: String,
    pub rel: String,
    pub built_in: bool,
}

pub fn built_in_profiles() -> Vec<LegacyAgentProfile> {
    vec![
        built_in_profile(BuiltInProfileSpec {
            id: "agentfather",
            name: "AgentFather",
            role: "agent orchestration",
            skills: &["design-agent-profile", "coordinate-specialists"],
            access: access_preset("conservative"),
            tools: &["create_agent_profile", "agent_profile_catalog"],
            constraints: &[
                "Do not inspect source code.",
                "Do not run terminal commands.",
                "Create role profiles through approved storage only.",
            ],
            outputs: &["agent-profile"],
        }),
        built_in_profile(BuiltInProfileSpec {
            id: "loopbuilder",
            name: "LoopBuilder",
            role: "Agent Loop design",
            skills: &["design-agent-loop", "bound-retries", "estimate-loop-cost"],
            access: access_preset("loop_designer"),
            tools: &["loop_create_draft", "loop_validate"],
            constraints: &[
                "Create drafts only; never activate or run an Agent Loop.",
                "Every cycle must include a bounded Retry node.",
                "Use human approval before privileged or irreversible actions.",
                "Do not request secrets or unrestricted permissions.",
            ],
            outputs: &["agent-loop-draft", "loop-validation-report"],
        }),
        built_in_profile(BuiltInProfileSpec {
            id: "librarian",
            name: "Librarian",
            role: "knowledge curation",
            skills: &["organize-vault", "summarize-notes"],
            access: access_preset("vault_writer"),
            tools: &["vault_note_read", "vault_note_write", "vault_search"],
            constraints: &["Preserve source attribution."],
            outputs: &["curated-note", "catalog-entry"],
        }),
        built_in_profile(BuiltInProfileSpec {
            id: "analyst",
            name: "Analyst",
            role: "analysis",
            skills: &["research", "synthesize-findings"],
            access: access_preset("vault_reader"),
            tools: &["vault_note_read", "vault_search"],
            constraints: &["Separate facts from assumptions."],
            outputs: &["analysis-brief"],
        }),
        built_in_profile(BuiltInProfileSpec {
            id: "pm",
            name: "PM",
            role: "project management",
            skills: &["plan-roadmap", "manage-tasks"],
            access: access_preset("vault_writer"),
            tools: &["tasks_list", "tasks_create_project", "vault_note_write"],
            constraints: &["Keep decisions traceable to project goals."],
            outputs: &["project-plan", "task-list"],
        }),
        built_in_profile(BuiltInProfileSpec {
            id: "architect",
            name: "Architect",
            role: "architecture",
            skills: &["create-architecture", "review-design"],
            access: access_preset("vault_writer"),
            tools: &["vault_note_read", "vault_note_write", "graph_scan"],
            constraints: &["Do not edit implementation code."],
            outputs: &["architecture-note"],
        }),
        built_in_profile(BuiltInProfileSpec {
            id: "security",
            name: "Security",
            role: "security review",
            skills: &["threat-model", "review-controls"],
            access: access_preset("vault_reader"),
            tools: &["vault_note_read", "vault_search"],
            constraints: &["Treat secrets and credentials as denied content."],
            outputs: &["security-review"],
        }),
        built_in_profile(BuiltInProfileSpec {
            id: "planner",
            name: "Planner",
            role: "planning",
            skills: &["break-down-work", "sequence-tasks"],
            access: access_preset("vault_writer"),
            tools: &["tasks_list", "vault_note_write"],
            constraints: &["Keep plans executable and scoped."],
            outputs: &["implementation-plan"],
        }),
        built_in_profile(BuiltInProfileSpec {
            id: "builder",
            name: "Builder",
            role: "implementation",
            skills: &["implement-plan", "update-files"],
            access: access_preset("builder"),
            tools: &["read_source", "edit_source", "run_tests"],
            constraints: &["Stay within assigned files."],
            outputs: &["code-change"],
        }),
        built_in_profile(BuiltInProfileSpec {
            id: "reviewer",
            name: "Reviewer",
            role: "review",
            skills: &["review-implementation", "verify-tests"],
            access: access_preset("reviewer"),
            tools: &["read_source", "run_tests"],
            constraints: &["Prioritize defects, regressions, and missing tests."],
            outputs: &["review-report"],
        }),
    ]
}

pub fn profile_rel_for_id(raw: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;

    for ch in raw.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(ch);
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }

    if slug.is_empty() {
        slug = "agent-profile".to_string();
    }

    format!("System/Agents/Custom/{slug}.md")
}

pub fn validate_profile_frontmatter(profile: &LegacyAgentProfile) -> Result<(), String> {
    validate_frontmatter_value("id", &profile.id)?;
    validate_frontmatter_value("name", &profile.name)?;
    validate_frontmatter_value("status", &profile.status)?;
    validate_frontmatter_value("role", &profile.role)?;
    validate_frontmatter_value("runtime.provider", &profile.runtime.provider)?;
    validate_frontmatter_value("runtime.model", &profile.runtime.model)?;
    validate_frontmatter_value("runtime.mode", &profile.runtime.mode)?;
    validate_frontmatter_values("skills", &profile.skills)?;
    validate_frontmatter_values("access.read", &profile.access.read)?;
    validate_frontmatter_values("access.write", &profile.access.write)?;
    validate_frontmatter_values("access.denied", &profile.access.denied)?;
    validate_frontmatter_values("tools", &profile.tools)?;
    validate_frontmatter_values("constraints", &profile.constraints)?;
    validate_frontmatter_values("outputs", &profile.outputs)?;
    Ok(())
}

#[tauri::command]
pub fn agent_profiles_seed() -> Result<Vec<LegacyAgentProfile>, String> {
    let root = crate::vault::vault_root("work")?;
    for profile in built_in_profiles() {
        validate_profile_frontmatter(&profile)?;
        let abs = crate::vault::safe_join(&root, &profile.rel)?;
        reject_symlinks_in_rel(&root, &profile.rel)?;
        if abs.exists() {
            continue;
        }
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        fs::write(&abs, render_profile_markdown(&profile))
            .map_err(|e| format!("write {}: {e}", abs.display()))?;
    }
    agent_profiles_list()
}

#[tauri::command]
pub fn agent_profiles_list() -> Result<Vec<LegacyAgentProfile>, String> {
    let root = crate::vault::vault_root("work")?;
    let mut profiles = Vec::new();
    read_profiles_dir(&root, "System/Agents", &mut profiles)?;
    profiles.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.rel.cmp(&b.rel)));
    Ok(profiles)
}

#[tauri::command]
pub fn agent_profile_read(rel: String) -> Result<LegacyAgentProfile, String> {
    ensure_agent_rel(&rel)?;
    let root = crate::vault::vault_root("work")?;
    let abs = crate::vault::safe_join(&root, &rel)?;
    reject_symlinks_in_rel(&root, &rel)?;
    let body = fs::read_to_string(&abs).map_err(|e| format!("read {}: {e}", abs.display()))?;
    parse_profile_markdown(&rel, &body, is_built_in_rel(&rel))
}

#[tauri::command]
pub fn agent_profile_save(profile: LegacyAgentProfile) -> Result<LegacyAgentProfile, String> {
    if profile.built_in || is_built_in_id(&profile.id) {
        return Err("built-in profiles cannot be saved".to_string());
    }

    let rel = if profile.rel.trim().is_empty() {
        profile_rel_for_id(&profile.id)
    } else {
        profile.rel.clone()
    };
    ensure_custom_rel(&rel)?;

    let root = crate::vault::vault_root("work")?;
    let abs = crate::vault::safe_join(&root, &rel)?;
    reject_symlinks_in_rel(&root, &rel)?;
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }

    let mut saved = profile;
    saved.rel = rel.clone();
    saved.built_in = false;
    validate_profile_frontmatter(&saved)?;
    fs::write(&abs, render_profile_markdown(&saved))
        .map_err(|e| format!("write {}: {e}", abs.display()))?;
    agent_profile_read(rel)
}

#[tauri::command]
pub fn agent_profile_delete(handle: Option<String>, rel: Option<String>) -> Result<(), String> {
    match (handle, rel) {
        (Some(handle), None) => {
            let _guard = profile_store_guard()?;
            delete_identity_profile(&profile_store_path(), &handle)
        }
        (None, Some(rel)) => delete_legacy_profile(&rel),
        (Some(_), Some(_)) => Err("provide either handle or rel, not both".to_string()),
        (None, None) => Err("profile handle is required".to_string()),
    }
}

fn delete_legacy_profile(rel: &str) -> Result<(), String> {
    if is_built_in_rel(rel) {
        return Err("built-in profiles cannot be deleted".to_string());
    }
    ensure_custom_rel(rel)?;

    let root = crate::vault::vault_root("work")?;
    let abs = crate::vault::safe_join(&root, rel)?;
    reject_symlinks_in_rel(&root, rel)?;
    fs::remove_file(&abs).map_err(|e| format!("delete {}: {e}", abs.display()))
}

// ─── Persistent identity profiles (XNAUT-143) ──────────────────────────────

fn profile_store_path() -> PathBuf {
    dirs::config_dir()
        .map(|path| path.join("xnaut").join("agent-profiles.toml"))
        .unwrap_or_else(|| PathBuf::from(".xnaut/agent-profiles.toml"))
}

fn profile_store_guard() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    static PROFILE_STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    PROFILE_STORE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "agent profile store lock is poisoned".to_string())
}

fn normalize_handle(raw: &str) -> String {
    raw.trim()
        .strip_prefix('@')
        .unwrap_or(raw.trim())
        .to_ascii_lowercase()
}

fn validate_handle(handle: &str) -> Result<(), String> {
    let handle = normalize_handle(handle);
    if handle.is_empty() || handle.len() > 64 {
        return Err("handle must contain 1-64 characters".to_string());
    }
    if !handle
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
        || !handle.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(
            "handle must start with a letter or number and use only ASCII letters, numbers, hyphens, or underscores"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_identity_profile(profile: &AgentProfile) -> Result<(), String> {
    validate_handle(&profile.handle)?;
    if profile.display_name.trim().is_empty() {
        return Err("display_name must not be empty".to_string());
    }
    if profile.tagline.chars().count() > 72 {
        return Err("tagline must not exceed 72 characters".to_string());
    }
    if profile.purpose.trim().is_empty() {
        return Err("purpose must not be empty".to_string());
    }
    if profile.runtime_id.trim().is_empty() {
        return Err("runtime_id must not be empty".to_string());
    }
    if profile.provider.trim().is_empty() {
        return Err("provider must not be empty".to_string());
    }
    if !profile.reasoning_effort.is_empty()
        && !matches!(
            profile.reasoning_effort.as_str(),
            "none" | "low" | "medium" | "high" | "xhigh"
        )
    {
        return Err("reasoning_effort must be none, low, medium, high, or xhigh".to_string());
    }
    if profile.role.trim().is_empty() {
        return Err("role must not be empty".to_string());
    }
    if profile.role.chars().any(char::is_control) {
        return Err("role must not contain control characters".to_string());
    }
    if !profile.accent_color.starts_with('#')
        || profile.accent_color.len() != 7
        || !profile.accent_color[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("accent_color must be a six-digit hex color".to_string());
    }
    for capability in &profile.capabilities {
        if capability.trim().is_empty()
            || capability.contains(',')
            || capability.chars().any(char::is_control)
        {
            return Err(
                "capabilities must be non-empty, comma-free values without control characters"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn load_profile_store(path: &Path) -> Result<AgentProfileStore, String> {
    if !path.exists() {
        return Ok(AgentProfileStore {
            version: PROFILE_STORE_VERSION,
            profiles: Vec::new(),
        });
    }
    let body = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut store: AgentProfileStore = toml::from_str(&body)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    if store.version > PROFILE_STORE_VERSION {
        return Err(format!(
            "unsupported agent profile store version {} (this xNaut supports {})",
            store.version, PROFILE_STORE_VERSION
        ));
    }

    let mut handles = std::collections::HashSet::new();
    for profile in &mut store.profiles {
        profile.handle = normalize_handle(&profile.handle);
        validate_identity_profile(profile)?;
        if !handles.insert(profile.handle.clone()) {
            return Err(format!(
                "duplicate agent profile handle: @{}",
                profile.handle
            ));
        }
    }
    Ok(store)
}

fn write_profile_store(path: &Path, store: &AgentProfileStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let body = toml::to_string_pretty(store)
        .map_err(|error| format!("failed to serialize agent profiles: {error}"))?;
    let temporary = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    fs::write(&temporary, body)
        .map_err(|error| format!("failed to write {}: {error}", temporary.display()))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("failed to replace {}: {error}", path.display()));
    }
    Ok(())
}

fn inferred_provider(runtime_id: &str) -> String {
    match runtime_id {
        "claude" => "anthropic",
        "codex" => "openai",
        "gemini" => "google",
        "grok" => "xai",
        _ => "global",
    }
    .to_string()
}

fn runtime_handle(runtime_id: &str) -> String {
    let mut handle = String::new();
    let mut pending_dash = false;
    for character in runtime_id.trim().chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            if pending_dash && !handle.is_empty() {
                handle.push('-');
            }
            handle.push(character.to_ascii_lowercase());
            pending_dash = false;
        } else {
            pending_dash = true;
        }
        if handle.len() >= 64 {
            handle.truncate(64);
            break;
        }
    }
    if handle.is_empty() {
        "agent".to_string()
    } else {
        handle
    }
}

fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn default_profile_for_runtime(
    runtime: &crate::agents::AgentConfig,
    timestamp: &str,
) -> AgentProfile {
    let display_name = if runtime.label.trim().is_empty() {
        runtime.id.clone()
    } else {
        runtime.label.clone()
    };
    AgentProfile {
        handle: runtime_handle(&runtime.id),
        display_name: display_name.clone(),
        tagline: truncate_chars(&format!("{display_name} coding agent"), 72),
        purpose: format!(
            "Use {} to help with coding, analysis, and project work.",
            display_name
        ),
        runtime_id: runtime.id.clone(),
        provider: inferred_provider(&runtime.id),
        model: String::new(),
        chat_model: String::new(),
        reasoning_effort: String::new(),
        execution: AgentExecution::Local,
        role: "coding-agent".to_string(),
        capabilities: vec!["terminal".to_string(), "code".to_string()],
        notifications: true,
        policy: crate::policy::AgentPolicy::default(),
        accent_color: seeded_accent_color(&runtime_handle(&runtime.id)),
        default_project: None,
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    }
}

fn default_nautbot_profile(runtime_id: &str, timestamp: &str) -> AgentProfile {
    AgentProfile {
        handle: RESERVED_NAUTBOT_HANDLE.to_string(),
        display_name: "NautBot".to_string(),
        tagline: "Your guide and control layer for xNaut.".to_string(),
        purpose: "Help install, create, show, explain, guide, and coordinate work across xNaut before specialist agents are needed.".to_string(),
        runtime_id: runtime_id.to_string(),
        provider: "nautgate".to_string(),
        model: "gpt-5.6-sol".to_string(),
        chat_model: String::new(),
        reasoning_effort: "high".to_string(),
        execution: AgentExecution::Local,
        role: "core-orchestrator".to_string(),
        capabilities: vec![
            "guide".to_string(),
            "install".to_string(),
            "create".to_string(),
            "explain".to_string(),
            "coordinate".to_string(),
        ],
        notifications: true,
        // The orchestrator needs the full local toolset to do its job.
        policy: crate::policy::AgentPolicy::default(),
        accent_color: DEFAULT_ACCENT_COLOR.to_string(),
        default_project: None,
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    }
}

/// The Librarian, as an agent rather than a pane of its own.
///
/// It was a right-pane view driving the vault through a private JSON action
/// protocol, which made it a second kind of agent living where the others are
/// not. It holds the vault tools now and talks in a thread like everyone else.
fn default_librarian_profile(runtime_id: &str, timestamp: &str) -> AgentProfile {
    AgentProfile {
        handle: "librarian".to_string(),
        display_name: "Librarian".to_string(),
        tagline: "Keeps the vault: finds what is written, and writes what is decided.".to_string(),
        purpose: "Search the work vault before answering, and write documents into it with the frontmatter every note here carries. Prefer adding to an existing note over creating a near-duplicate. Say which note you read or wrote, by path.".to_string(),
        runtime_id: runtime_id.to_string(),
        provider: "nautgate".to_string(),
        model: "gpt-5.6-sol".to_string(),
        chat_model: String::new(),
        reasoning_effort: "high".to_string(),
        execution: AgentExecution::Local,
        role: "librarian".to_string(),
        capabilities: vec!["vault".to_string(), "search".to_string(), "write".to_string()],
        notifications: true,
        // Reads and writes the vault through tools, never a shell.
        policy: crate::policy::AgentPolicy {
            filesystem: "read-only".to_string(),
            shell: false,
            ..crate::policy::AgentPolicy::default()
        },
        accent_color: "#6aa9ff".to_string(),
        default_project: None,
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    }
}

/// Fill fields that did not exist when a profile was written.
///
/// The store is seeded once and then only rewritten when someone edits a
/// profile, so every field added later is absent on every machine that has run
/// xNAUT before. `agents.toml` had exactly this bug (XNAUT-182) and profiles
/// were the same file one door down (XNAUT-197).
///
/// Returns true when something changed, so the caller writes it back once
/// rather than recomputing on every turn.
///
/// ponytail: only fields whose empty value is unambiguously "never set" belong
/// here. `chat_model` qualifies — empty means "same as model", which is what
/// every old profile means anyway — so this is currently a guard with one
/// inhabitant, and the point is that the next field has somewhere to go.
fn backfill_profiles(store: &mut AgentProfileStore) -> bool {
    let mut changed = false;
    for profile in &mut store.profiles {
        // NautBot's chat route is the one an agent inherits at creation, so a
        // profile that predates the split should not be left pointing at a
        // route that cannot carry tool calls.
        if profile.accent_color.trim().is_empty() {
            profile.accent_color = DEFAULT_ACCENT_COLOR.to_string();
            changed = true;
        }
    }
    changed
}

fn load_or_seed_profile_store(path: &Path) -> Result<AgentProfileStore, String> {
    let is_new = !path.exists();
    let mut store = load_profile_store(path)?;
    let needs_nautbot = !store
        .profiles
        .iter()
        .any(|profile| profile.handle == RESERVED_NAUTBOT_HANDLE);
    let needs_librarian = !store.profiles.iter().any(|profile| profile.handle == "librarian");
    if !is_new && !needs_nautbot && !needs_librarian {
        // Not a no-op: a store written before a field existed is missing it,
        // and serde(default) makes missing arrive as empty without a word.
        if backfill_profiles(&mut store) {
            write_profile_store(path, &store)?;
        }
        return Ok(store);
    }
    let registry = crate::agents::load_or_seed_registry()?;
    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut changed = false;
    if needs_nautbot {
        let runtime_id = registry
            .find("codex")
            .or_else(|| registry.agents.first())
            .map(|runtime| runtime.id.as_str())
            .ok_or_else(|| "cannot create NautBot without an agent runtime".to_string())?;
        store
            .profiles
            .push(default_nautbot_profile(runtime_id, &timestamp));
        changed = true;
    }
    if needs_librarian {
        let runtime_id = registry
            .find("claude")
            .or_else(|| registry.agents.first())
            .map(|runtime| runtime.id.as_str())
            .unwrap_or("claude");
        store.profiles.push(default_librarian_profile(runtime_id, &timestamp));
        changed = true;
    }
    if !is_new {
        if changed {
            write_profile_store(path, &store)?;
        }
        return Ok(store);
    }

    let mut handles = store
        .profiles
        .iter()
        .map(|profile| profile.handle.clone())
        .collect::<std::collections::HashSet<_>>();
    for runtime in registry
        .agents
        .iter()
        .filter(|runtime| crate::agents::binary_on_path(&runtime.detect_cmd))
    {
        let mut profile = default_profile_for_runtime(runtime, &timestamp);
        let base = profile.handle.clone();
        let mut suffix = 2;
        while !handles.insert(profile.handle.clone()) {
            let suffix_text = format!("-{suffix}");
            let keep = 64usize.saturating_sub(suffix_text.len());
            profile.handle = format!("{}{}", truncate_chars(&base, keep), suffix_text);
            suffix += 1;
        }
        store.profiles.push(profile);
        changed = true;
    }
    if changed {
        write_profile_store(path, &store)?;
    }
    Ok(store)
}

fn ensure_runtime_exists(runtime_id: &str) -> Result<(), String> {
    let registry = crate::agents::load_or_seed_registry()?;
    if registry.find(runtime_id).is_none() {
        return Err(format!("unknown agent runtime: {runtime_id}"));
    }
    Ok(())
}

fn prepare_created_profile(
    mut profile: AgentProfile,
    store: &AgentProfileStore,
) -> Result<AgentProfile, String> {
    profile.handle = normalize_handle(&profile.handle);
    if profile.handle == RESERVED_NAUTBOT_HANDLE {
        return Err("@nautbot is reserved for the protected core agent".to_string());
    }
    validate_identity_profile(&profile)?;
    ensure_runtime_exists(&profile.runtime_id)?;
    if store
        .profiles
        .iter()
        .any(|existing| existing.handle.eq_ignore_ascii_case(&profile.handle))
    {
        return Err(format!("agent handle already exists: @{}", profile.handle));
    }
    let timestamp = chrono::Utc::now().to_rfc3339();
    profile.created_at = timestamp.clone();
    profile.updated_at = timestamp;
    Ok(profile)
}

/// Create an agent from a handful of words, for the agent tool.
///
/// "Create me a new agent called Frontend Developer, short Fronti" opened a
/// WORKTREE and started reading repository guides. Creating an agent is an
/// xNAUT action, like switching a plugin on: one write, one sentence back.
pub fn create_profile_from(
    handle: &str,
    display_name: &str,
    tagline: &str,
    purpose: &str,
    runtime: Option<&str>,
    model: Option<&str>,
) -> Result<AgentProfile, String> {
    let _guard = profile_store_guard()?;
    let path = profile_store_path();
    let mut store = load_or_seed_profile_store(&path)?;
    let registry = crate::agents::load_or_seed_registry()?;
    // Default to the runtime NautBot uses: it is the one this machine is known
    // to have, rather than whichever happens to be first in the registry.
    let runtime_id = runtime
        .map(str::to_string)
        .or_else(|| store.profiles.iter().find(|p| p.handle == RESERVED_NAUTBOT_HANDLE).map(|p| p.runtime_id.clone()))
        .or_else(|| registry.agents.first().map(|r| r.id.clone()))
        .ok_or_else(|| "no agent runtime is available".to_string())?;
    let nautbot = store.profiles.iter().find(|p| p.handle == RESERVED_NAUTBOT_HANDLE).cloned();

    let profile = AgentProfile {
        handle: normalize_handle(handle),
        display_name: display_name.trim().to_string(),
        tagline: tagline.trim().to_string(),
        purpose: purpose.trim().to_string(),
        runtime_id,
        provider: nautbot.as_ref().map(|p| p.provider.clone()).unwrap_or_else(|| "nautgate".into()),
        model: model
            .map(str::to_string)
            .or_else(|| nautbot.as_ref().map(|p| p.model.clone()))
            .unwrap_or_default(),
        // A new agent inherits NautBot's chat route, so an agent created while
        // the gateway cannot carry tool calls is not born unable to act.
        chat_model: nautbot.as_ref().map(|p| p.chat_model.clone()).unwrap_or_default(),
        reasoning_effort: "high".to_string(),
        execution: AgentExecution::Local,
        role: "specialist".to_string(),
        capabilities: vec![],
        notifications: true,
        policy: crate::policy::AgentPolicy::default(),
        accent_color: DEFAULT_ACCENT_COLOR.to_string(),
        default_project: None,
        created_at: String::new(),
        updated_at: String::new(),
    };
    let prepared = prepare_created_profile(profile, &store)?;
    store.profiles.push(prepared.clone());
    write_profile_store(&path, &store)?;
    Ok(prepared)
}

/// Remove a profile, for tests that create one. Not a command: deleting an
/// agent is a decision the owner makes in the UI.
#[cfg(test)]
pub fn delete_profile_for_test(handle: &str) -> Result<(), String> {
    delete_identity_profile(&profile_store_path(), handle)
}

fn delete_identity_profile(path: &Path, raw_handle: &str) -> Result<(), String> {
    let handle = normalize_handle(raw_handle);
    validate_handle(&handle)?;
    if handle == RESERVED_NAUTBOT_HANDLE {
        return Err("@nautbot is protected and cannot be deleted".to_string());
    }
    let mut store = load_or_seed_profile_store(path)?;
    let original_len = store.profiles.len();
    store.profiles.retain(|profile| profile.handle != handle);
    if store.profiles.len() == original_len {
        return Err(format!("agent profile not found: @{handle}"));
    }
    write_profile_store(path, &store)
}

fn mesh_identity_env(profile: &AgentProfile) -> std::collections::HashMap<String, String> {
    std::collections::HashMap::from([
        ("ENGRAM_NICKNAME".to_string(), profile.handle.clone()),
        ("ENGRAM_ROLE".to_string(), profile.role.clone()),
        (
            "ENGRAM_CAPABILITIES".to_string(),
            profile.capabilities.join(","),
        ),
    ])
}

/// Resolves what a human or model actually SAYS to a real handle: the
/// handle itself, or a display name, case-insensitive, @ tolerated. The
/// first dogfood run of the ticket loop died on exactly this: the profile
/// is handle "claude", display name "Claudi", and NautBot said "claudi" —
/// so the wake found no profile and the assigned ticket was owned by a
/// string no agent would ever match. Every surface that accepts a handle
/// resolves through here; unknown names come back as Err naming the roster.
pub fn resolve_spoken_handle(spoken: &str) -> Result<String, String> {
    let wanted = normalize_handle(spoken);
    if wanted.is_empty() {
        return Err("an agent name is required".to_string());
    }
    let profiles = agent_profile_list()?;
    if let Some(profile) = profiles.iter().find(|p| p.handle == wanted) {
        return Ok(profile.handle.clone());
    }
    if let Some(profile) = profiles
        .iter()
        .find(|p| p.display_name.trim().to_ascii_lowercase() == wanted)
    {
        return Ok(profile.handle.clone());
    }
    let roster: Vec<String> = profiles
        .iter()
        .map(|p| format!("@{} ({})", p.handle, p.display_name))
        .collect();
    Err(format!(
        "no agent called {spoken:?}. The roster: {}",
        roster.join(", ")
    ))
}

#[tauri::command]
pub fn agent_profile_list() -> Result<Vec<AgentProfile>, String> {
    let _guard = profile_store_guard()?;
    let mut profiles = load_or_seed_profile_store(&profile_store_path())?.profiles;
    profiles.sort_by(|left, right| {
        left.display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase())
            .then_with(|| left.handle.cmp(&right.handle))
    });
    Ok(profiles)
}

#[tauri::command]
pub fn agent_profile_get(handle: String) -> Result<AgentProfile, String> {
    let _guard = profile_store_guard()?;
    let handle = normalize_handle(&handle);
    validate_handle(&handle)?;
    load_or_seed_profile_store(&profile_store_path())?
        .profiles
        .into_iter()
        .find(|profile| profile.handle == handle)
        .ok_or_else(|| format!("agent profile not found: @{handle}"))
}

#[tauri::command]
pub fn agent_profile_create(profile: AgentProfile) -> Result<AgentProfile, String> {
    let _guard = profile_store_guard()?;
    let path = profile_store_path();
    let mut store = load_or_seed_profile_store(&path)?;
    let created = prepare_created_profile(profile, &store)?;
    store.profiles.push(created.clone());
    write_profile_store(&path, &store)?;
    Ok(created)
}

#[tauri::command]
pub fn agent_profile_update(
    handle: String,
    mut profile: AgentProfile,
) -> Result<AgentProfile, String> {
    let _guard = profile_store_guard()?;
    let path = profile_store_path();
    let mut store = load_or_seed_profile_store(&path)?;
    let original_handle = normalize_handle(&handle);
    let Some(index) = store
        .profiles
        .iter()
        .position(|existing| existing.handle == original_handle)
    else {
        return Err(format!("agent profile not found: @{original_handle}"));
    };

    profile.handle = normalize_handle(&profile.handle);
    if original_handle == RESERVED_NAUTBOT_HANDLE && profile.handle != original_handle {
        return Err("@nautbot is protected and cannot be renamed".to_string());
    }
    if profile.handle == RESERVED_NAUTBOT_HANDLE && original_handle != RESERVED_NAUTBOT_HANDLE {
        return Err("@nautbot is reserved for the protected core agent".to_string());
    }
    validate_identity_profile(&profile)?;
    ensure_runtime_exists(&profile.runtime_id)?;
    if store
        .profiles
        .iter()
        .enumerate()
        .any(|(candidate, existing)| {
            candidate != index && existing.handle.eq_ignore_ascii_case(&profile.handle)
        })
    {
        return Err(format!("agent handle already exists: @{}", profile.handle));
    }
    profile.created_at = store.profiles[index].created_at.clone();
    profile.updated_at = chrono::Utc::now().to_rfc3339();
    store.profiles[index] = profile.clone();
    write_profile_store(&path, &store)?;
    Ok(profile)
}

#[tauri::command]
pub fn agent_profile_duplicate(
    handle: String,
    new_handle: String,
    display_name: Option<String>,
) -> Result<AgentProfile, String> {
    let _guard = profile_store_guard()?;
    let path = profile_store_path();
    let mut store = load_or_seed_profile_store(&path)?;
    let source_handle = normalize_handle(&handle);
    let mut duplicate = store
        .profiles
        .iter()
        .find(|profile| profile.handle == source_handle)
        .cloned()
        .ok_or_else(|| format!("agent profile not found: @{source_handle}"))?;
    duplicate.handle = new_handle;
    if let Some(display_name) = display_name {
        duplicate.display_name = display_name;
    } else {
        duplicate.display_name = format!("{} Copy", duplicate.display_name);
    }
    let duplicate = prepare_created_profile(duplicate, &store)?;
    store.profiles.push(duplicate.clone());
    write_profile_store(&path, &store)?;
    Ok(duplicate)
}

/// The roster as an agent sees it: who exists and what each one holds.
pub fn roster_snapshot() -> Vec<serde_json::Value> {
    let Ok(store) = load_or_seed_profile_store(&profile_store_path()) else {
        return Vec::new();
    };
    store
        .profiles
        .into_iter()
        .map(|profile| {
            let plugins: Vec<String> = profile
                .capabilities
                .iter()
                .filter_map(|entry| entry.strip_prefix("plugin:").map(str::to_string))
                .collect();
            // TAGS ONLY. An agent learns another agent's handle and role,
            // never which runtime or model is behind it. Two reasons: a model
            // that knows who it is arguing with can posture, defer or compete
            // rather than answer, and a roster naming runtimes invites routing
            // work by brand instead of by role. The runtime is a property of
            // the identity, and it is the owner's business.
            serde_json::json!({
                "handle": profile.handle,
                "name": profile.display_name,
                "role": profile.role,
                "plugins": plugins,
            })
        })
        .collect()
}

/// Hand one plugin to one agent, or take it back. Returns what that agent
/// holds afterwards, so the caller reports the state rather than the intent.
pub fn set_plugin_grant(handle: &str, id: &str, granted: bool) -> Result<Vec<String>, String> {
    let handle = normalize_handle(handle);
    validate_handle(&handle)?;
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("which plugin?".into());
    }
    if !crate::plugins::catalog_snapshot()
        .iter()
        .any(|plugin| plugin["id"] == serde_json::json!(id))
    {
        return Err(format!("no plugin called {id:?} in the library"));
    }
    let _guard = profile_store_guard()?;
    let path = profile_store_path();
    let mut store = load_or_seed_profile_store(&path)?;
    let profile = store
        .profiles
        .iter_mut()
        .find(|profile| profile.handle == handle)
        .ok_or_else(|| format!("agent profile not found: @{handle}"))?;
    let entry = format!("plugin:{id}");
    profile.capabilities.retain(|item| item != &entry);
    if granted {
        profile.capabilities.push(entry);
    }
    let held: Vec<String> = profile
        .capabilities
        .iter()
        .filter_map(|item| item.strip_prefix("plugin:").map(str::to_string))
        .collect();
    write_profile_store(&path, &store)?;
    Ok(held)
}

/// A branch name from what the owner asked for. Lowercase, dashes, bounded.
fn branch_slug(task: &str) -> String {
    let slug: String = task
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "work".to_string()
    } else {
        slug.chars().take(48).collect()
    }
}

/// Where a build actually runs: a worktree of the named repository.
///
/// An agent gets its own worktree rather than the owner's checkout, so a run
/// cannot touch what he has open — the same rule the humans here work under.
/// A path that is not a git repository is used directly; that is a deliberate
/// choice for scratch folders, not a silent fallback for a broken repo.
#[tauri::command]
pub async fn agent_build_workspace(
    handle: String,
    repo_path: String,
    task: String,
) -> Result<String, String> {
    let handle = normalize_handle(&handle);
    validate_handle(&handle)?;
    let repo = std::path::PathBuf::from(repo_path.trim());
    if !repo.is_dir() {
        return Err(format!("not a folder: {}", repo.display()));
    }
    tokio::task::spawn_blocking(move || {
        let git = |args: &[&str]| -> (bool, String) {
            match std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
            {
                Ok(out) => (
                    out.status.success(),
                    String::from_utf8_lossy(if out.status.success() {
                        &out.stdout
                    } else {
                        &out.stderr
                    })
                    .trim()
                    .to_string(),
                ),
                Err(error) => (false, error.to_string()),
            }
        };
        let (is_repo, _) = git(&["rev-parse", "--git-dir"]);
        if !is_repo {
            return Ok(repo.to_string_lossy().into_owned());
        }
        let branch = format!("agent/{handle}/{}", branch_slug(&task));
        let dest = repo.join(".worktrees").join(branch_slug(&task));
        let dest_str = dest.to_string_lossy().to_string();
        if dest.is_dir() {
            return Ok(dest_str); // resuming the same piece of work
        }
        let (added, error) = git(&["worktree", "add", "-b", &branch, &dest_str]);
        if added {
            return Ok(dest_str);
        }
        // The branch surviving a removed worktree is the common case; reuse it
        // rather than inventing a second name for the same work.
        let (reused, reuse_error) = git(&["worktree", "add", &dest_str, &branch]);
        if reused {
            Ok(dest_str)
        } else {
            Err(format!("could not open a worktree: {error}; {reuse_error}"))
        }
    })
    .await
    .map_err(|error| format!("worktree task failed: {error}"))?
}

/// One conversational turn with an agent, against ITS baseline model.
///
/// This is what a message does by default. Launching a coding harness for
/// every question was the wrong flow: it is slow, it burns a worktree and a
/// CLI session on "what is the status", and it gave NautBot a coding runtime
/// it was never meant to drive. The harness now starts only when the agent
/// says the request needs one (`BUILD-REQUEST`) and the owner names a
/// repository.
#[tauri::command]
pub async fn agent_chat_turn(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    handle: String,
    request_id: String,
    messages: Vec<crate::chat::ChatMessage>,
) -> Result<String, String> {
    let profile = {
        let _guard = profile_store_guard()?;
        let handle = normalize_handle(&handle);
        validate_handle(&handle)?;
        load_or_seed_profile_store(&profile_store_path())?
            .profiles
            .into_iter()
            .find(|profile| profile.handle == handle)
            .ok_or_else(|| format!("agent profile not found: @{handle}"))?
    };
    let mut turn = vec![crate::chat::ChatMessage {
        role: "system".into(),
        content: crate::composer::chat_system(&profile),
    }];
    turn.extend(messages);
    let effort = (!profile.reasoning_effort.trim().is_empty()).then(|| profile.reasoning_effort.clone());
    let provider = profile.provider.trim();

    // An agent that can only DESCRIBE how to switch a plugin on is answering
    // about the product instead of operating it. Try the tool loop first; fall
    // back to a plain completion when the provider cannot do tool calls, so a
    // local model still answers rather than erroring.
    let llm = {
        let settings = state.settings.lock().await;
        let chat_model = profile.chat_model_or_model().to_string();
        if provider.is_empty() || provider == "global" {
            let mut llm = settings.llm.clone();
            if !chat_model.is_empty() {
                llm.model = chat_model;
            }
            Some(llm)
        } else {
            crate::chat::provider_llm(&settings, provider).map(|mut llm| {
                if !chat_model.is_empty() {
                    llm.model = chat_model;
                }
                llm
            })
        }
    };
    // Set when the tool loop could not run, so the reply can say why rather
    // than looking like an agent that simply chose not to act.
    let mut tool_failure: Option<String> = None;
    if let Some(llm) = llm {
        if !llm.model.trim().is_empty() {
            let history: Vec<serde_json::Value> = turn
                .iter()
                .map(|message| serde_json::json!({ "role": message.role, "content": message.content }))
                .collect();
            match crate::agent_tools::run_turn(&llm, &llm.model, history, effort.as_deref(), &profile.capabilities, &profile.handle).await {
                Ok(crate::agent_tools::TurnOutcome {
                    text,
                    performed,
                    surface,
                    needs_auth,
                    open_graph,
                    wrote_document,
                    attach_session,
                }) => {
                    if let Some(session) = attach_session {
                        let _ = tauri::Emitter::emit(
                            &app,
                            "attach-zellij-session",
                            serde_json::json!({ "session": session, "agent_id": profile.handle }),
                        );
                    }
                    if open_graph {
                        let _ = tauri::Emitter::emit(&app, "open-graph", serde_json::json!({}));
                    }
                    // A local page the turn produced (an Excalidraw canvas, a
                    // preview) belongs on screen beside the conversation, not
                    // as a URL to copy.
                    if let Some(url) = surface {
                        let _ = tauri::Emitter::emit(
                            &app,
                            "open-in-browser",
                            serde_json::json!({ "url": url, "agent_id": profile.handle }),
                        );
                    }
                    // A plugin that wants a login becomes a card in the
                    // thread, with the button right there.
                    if let Some(card) = needs_auth {
                        let _ = tauri::Emitter::emit(
                            &app,
                            "plugin-needs-auth",
                            serde_json::json!({ "agent_id": profile.handle, "plugin": card }),
                        );
                    }
                    if wrote_document {
                        let _ = tauri::Emitter::emit(
                            &app,
                            "document-changed",
                            serde_json::json!({ "key": profile.handle }),
                        );
                    }
                    // XNAUT-251: what the turn ACTUALLY did, sent to the UI
                    // whether or not anything happened. Three test runs were
                    // lost to NautBot describing work it had not done
                    // ("verification started and is still running"), and prose
                    // cannot be checked. A receipt can: an answer claiming an
                    // action with an empty tool list is visibly a story.
                    let _ = tauri::Emitter::emit(
                        &app,
                        "agent-turn-tools",
                        serde_json::json!({ "request_id": request_id, "tools": performed }),
                    );
                    if !performed.is_empty() {
                        // The UI repaints from the store, so a plugin switched
                        // on mid-conversation shows up without a reload.
                        let _ = tauri::Emitter::emit(&app, "agent-profiles-changed", serde_json::json!({ "did": performed }));
                    }
                    return Ok(text);
                }
                Err(error) => {
                    // The fallback is right — an agent that cannot call tools
                    // should still answer — but it was SILENT, and that is what
                    // cost four days. The model, asked to do something, replied
                    // "the tool isn't available, please enable it", which reads
                    // as a missing feature rather than a broken route. Twice
                    // now: an Anthropic balance on 2026-08-15, and NautGate
                    // answering "tool calls are not supported by this transport"
                    // for every OpenAI-family model because they route over the
                    // chatgpt-subscription relay (2026-08-19, XNAUT-195).
                    //
                    // So it says so, in the thread, in the reply itself. The
                    // owner reads the answer; nobody reads debug.log.
                    let _ = crate::debug_log::debug_log_append(vec![format!(
                        "[agent_chat_turn] tool loop unavailable, falling back to a plain completion: {error}"
                    )]);
                    tool_failure = Some(error);
                }
            }
        }
    }

    let reply = if provider.is_empty() || provider == "global" {
        crate::chat::chat_send_model(
            app,
            state,
            request_id,
            profile.chat_model_or_model().to_string(),
            turn,
            effort,
        )
        .await
    } else {
        crate::chat::chat_send_provider(
            app,
            state,
            request_id,
            provider.to_string(),
            profile.chat_model_or_model().to_string(),
            turn,
            effort,
        )
        .await
    };
    Ok(match tool_failure {
        Some(error) => format!("{}{}", reply?, tool_failure_notice(profile.chat_model_or_model(), &error)),
        None => reply?,
    })
}

/// What the thread is told when the tool loop could not run.
///
/// Written for the owner, not the log: it names the model, quotes the upstream
/// verbatim, and says what the answer above is missing. Anything vaguer and the
/// next person spends four days believing a feature was never built.
pub fn tool_failure_notice(model: &str, error: &str) -> String {
    format!(
        "\n\n---\n**Answered without tools.** `{model}` could not run a tool call, so nothing was \
created, changed or looked up above. The upstream said:\n\n> {}\n\nPick a model whose route \
supports tool calls, or fix that route; the agent's own capabilities are unaffected.",
        error.trim()
    )
}

#[tauri::command]
pub async fn agent_profile_launch(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    req: LaunchAgentProfileRequest,
) -> Result<crate::agents::LaunchAgentResponse, String> {
    let profile = {
        let _guard = profile_store_guard()?;
        let handle = normalize_handle(&req.handle);
        validate_handle(&handle)?;
        load_or_seed_profile_store(&profile_store_path())?
            .profiles
            .into_iter()
            .find(|profile| profile.handle == handle)
            .ok_or_else(|| format!("agent profile not found: @{handle}"))?
    };
    if profile.execution == AgentExecution::Sandbox {
        return Err(
            "sandbox profile launch is not wired to an interactive PTY yet; choose local execution"
                .to_string(),
        );
    }

    // Everything the agent is told is assembled in ONE place (composer.rs):
    // the Foundation, its own instructions, the limits nothing else enforces,
    // its skills, then the task. Until this call existed the composer was
    // dead code and a live run got the bare task — which is why an agent
    // could not find the inbox and shelled out to a system browser.
    let hook_url = state
        .hook_server
        .lock()
        .await
        .clone()
        .map(|info| info.url)
        .unwrap_or_default();
    let prompt = req
        .prompt
        .as_deref()
        .map(|task| crate::composer::compose(&profile, &hook_url, task, req.resume));

    let identity_env = mesh_identity_env(&profile);
    let launch_identity = crate::agents::AgentLaunchIdentity {
        id: profile.handle.clone(),
        label: profile.display_name.clone(),
    };
    crate::agents::launch_agent_with_env(
        app,
        state,
        crate::agents::LaunchAgentRequest {
            agent_id: profile.runtime_id,
            worktree_path: req.worktree_path,
            prompt,
            model: (!profile.model.trim().is_empty()).then_some(profile.model.clone()),
            conversation_mode: req.conversation_mode,
            conversation_id: req.conversation_id,
            resume: req.resume,
            reasoning_effort: (!profile.reasoning_effort.trim().is_empty())
                .then_some(profile.reasoning_effort.clone()),
            cols: req.cols,
            rows: req.rows,
            // Least privilege travels with the identity: the runtime gets the
            // agent's own policy, not a blanket default.
            policy: Some(profile.policy.clone()),
            // Which MCP servers this agent was handed, from its own list.
            capabilities: profile.capabilities.clone(),
            agent_handle: profile.handle.clone(),
        },
        identity_env,
        Some(launch_identity),
    )
    .await
}

/// A bounded scratch workspace for an agent with no project.
///
/// Not every message is a coding run: asking NautBot a question needs no
/// repository, and interrogating the owner before they can type is an
/// obstacle, not a safety feature. Home is still forbidden — too broad, and
/// coding CLIs stop at a trust prompt there — so an agent without a project
/// gets its own folder under our config directory instead. Small, bounded,
/// deletable, and never someone's real work.
#[tauri::command]
pub fn agent_scratch_workspace(handle: String) -> Result<String, String> {
    let handle = normalize_handle(&handle);
    validate_handle(&handle)?;
    let dir = dirs::config_dir()
        .map(|p| p.join("xnaut").join("agent-workspaces").join(&handle))
        .ok_or_else(|| "could not resolve the config directory".to_string())?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create the agent workspace: {e}"))?;
    Ok(dir.to_string_lossy().into_owned())
}

/// Resolve the explicit workspace an interactive agent may use. Agents must
/// never silently fall back to the user's home directory: coding CLIs stop at
/// trust prompts there and, more importantly, the scope is far too broad.
#[tauri::command]
pub fn agent_project_prepare(path: String, new_project: bool) -> Result<String, String> {
    let requested = std::path::PathBuf::from(path.trim());
    if path.trim().is_empty() {
        return Err("Choose a local project path".to_string());
    }
    if !requested.is_absolute() {
        return Err("Project path must be absolute".to_string());
    }

    if new_project {
        std::fs::create_dir_all(&requested)
            .map_err(|error| format!("Could not create project folder: {error}"))?;
    } else if !requested.is_dir() {
        return Err("Existing project folder was not found".to_string());
    }

    let resolved = requested
        .canonicalize()
        .map_err(|error| format!("Could not resolve project folder: {error}"))?;
    if !resolved.is_dir() {
        return Err("Project path is not a folder".to_string());
    }
    if resolved.parent().is_none() || dirs::home_dir().is_some_and(|home| resolved == home) {
        return Err("Choose a project folder, not the filesystem root or your home folder".to_string());
    }
    Ok(resolved.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn agent_profile_catalog() -> Result<serde_json::Value, String> {
    let items: Vec<AgentCatalogItem> = agent_profiles_list()?
        .into_iter()
        .map(|profile| AgentCatalogItem {
            id: profile.id,
            name: profile.name,
            status: profile.status,
            version: profile.version,
            role: profile.role,
            rel: profile.rel,
            built_in: profile.built_in,
        })
        .collect();
    Ok(serde_json::json!({ "items": items }))
}

#[tauri::command]
pub fn agent_profile_test(
    profile: LegacyAgentProfile,
    sample_rel: Option<String>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "reads": profile.access.read,
        "writes": profile.access.write,
        "tools": profile.tools,
        "blocked": profile.access.denied,
        "sample_rel": sample_rel.unwrap_or_default(),
        "summary": "Dry run only. No files changed."
    }))
}

pub fn parse_profile_markdown(
    rel: &str,
    body: &str,
    built_in: bool,
) -> Result<LegacyAgentProfile, String> {
    let (frontmatter, body) = frontmatter_block(body)?;
    let mut xnaut_agent = false;
    let mut id: Option<String> = None;
    let mut name: Option<String> = None;
    let mut status: Option<String> = None;
    let mut version: Option<u32> = None;
    let mut role: Option<String> = None;
    let mut runtime = AgentRuntime::default();
    let mut skills = Vec::new();
    let mut access = AgentAccess::default();
    let mut tools = Vec::new();
    let mut constraints = Vec::new();
    let mut outputs = Vec::new();
    let mut current_top_list: Option<&str> = None;
    let mut in_access = false;
    let mut in_runtime = false;
    let mut current_access_list: Option<&str> = None;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(value) = line.strip_prefix("    - ") {
            match current_access_list {
                Some("read") => access.read.push(clean_scalar(value)),
                Some("write") => access.write.push(clean_scalar(value)),
                Some("denied") => access.denied.push(clean_scalar(value)),
                _ => return Err(format!("unexpected nested list item: {line}")),
            }
            continue;
        }

        if let Some(value) = line.strip_prefix("  - ") {
            match current_top_list {
                Some("skills") => skills.push(clean_scalar(value)),
                Some("tools") => tools.push(clean_scalar(value)),
                Some("constraints") => constraints.push(clean_scalar(value)),
                Some("outputs") => outputs.push(clean_scalar(value)),
                _ => return Err(format!("unexpected list item: {line}")),
            }
            continue;
        }

        if in_access && line.starts_with("  ") {
            let (key, value) = split_key_value(trimmed)?;
            current_top_list = None;
            current_access_list = Some(key);
            match key {
                "read" => access.read = parse_list_scalar(value),
                "write" => access.write = parse_list_scalar(value),
                "denied" => access.denied = parse_list_scalar(value),
                _ => return Err(format!("unknown access key: {key}")),
            }
            continue;
        }

        if in_runtime && line.starts_with("  ") {
            let (key, value) = split_key_value(trimmed)?;
            current_top_list = None;
            current_access_list = None;
            match key {
                "provider" => runtime.provider = clean_scalar(value),
                "model" => runtime.model = clean_scalar(value),
                "mode" => runtime.mode = clean_scalar(value),
                _ => return Err(format!("unknown runtime key: {key}")),
            }
            continue;
        }

        let (key, value) = split_key_value(line)?;
        current_top_list = None;
        current_access_list = None;
        in_access = false;
        in_runtime = false;

        match key {
            "xnaut_agent" => xnaut_agent = value == "true",
            "id" => id = Some(clean_scalar(value)),
            "name" => name = Some(clean_scalar(value)),
            "status" => status = Some(clean_scalar(value)),
            "version" => {
                version = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| format!("invalid version: {value}"))?,
                );
            }
            "role" => role = Some(clean_scalar(value)),
            "runtime" => in_runtime = true,
            "skills" => {
                skills = parse_list_scalar(value);
                current_top_list = Some("skills");
            }
            "access" => in_access = true,
            "tools" => {
                tools = parse_list_scalar(value);
                current_top_list = Some("tools");
            }
            "constraints" => {
                constraints = parse_list_scalar(value);
                current_top_list = Some("constraints");
            }
            "outputs" => {
                outputs = parse_list_scalar(value);
                current_top_list = Some("outputs");
            }
            _ => return Err(format!("unknown profile key: {key}")),
        }
    }

    if !xnaut_agent {
        return Err("xnaut_agent must be true".to_string());
    }
    let id = id.unwrap_or_default();
    if id.is_empty() {
        return Err("id must not be empty".to_string());
    }
    let name = name.unwrap_or_default();
    if name.is_empty() {
        return Err("name must not be empty".to_string());
    }
    let status = status.unwrap_or_default();
    if status != "enabled" && status != "disabled" {
        return Err("status must be enabled or disabled".to_string());
    }
    let version = version.ok_or_else(|| "version must be present".to_string())?;
    let role = role.unwrap_or_default();
    if role.is_empty() {
        return Err("role must not be empty".to_string());
    }

    Ok(LegacyAgentProfile {
        id,
        name,
        status,
        version,
        role,
        runtime,
        skills,
        access,
        tools,
        constraints,
        outputs,
        body: body.to_string(),
        rel: rel.to_string(),
        built_in,
    })
}

pub fn render_profile_markdown(profile: &LegacyAgentProfile) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str("xnaut_agent: true\n");
    out.push_str(&format!("id: {}\n", profile.id));
    out.push_str(&format!("name: {}\n", profile.name));
    out.push_str(&format!("status: {}\n", profile.status));
    out.push_str(&format!("version: {}\n", profile.version));
    out.push_str(&format!("role: {}\n", profile.role));
    out.push_str("runtime:\n");
    out.push_str(&format!("  provider: {}\n", profile.runtime.provider));
    out.push_str(&format!("  model: {}\n", profile.runtime.model));
    out.push_str(&format!("  mode: {}\n", profile.runtime.mode));
    push_list(&mut out, "skills", &profile.skills);
    out.push_str("access:\n");
    push_nested_list(&mut out, "read", &profile.access.read);
    push_nested_list(&mut out, "write", &profile.access.write);
    push_nested_list(&mut out, "denied", &profile.access.denied);
    push_list(&mut out, "tools", &profile.tools);
    push_list(&mut out, "constraints", &profile.constraints);
    push_list(&mut out, "outputs", &profile.outputs);
    out.push_str("---\n");
    out.push_str(&profile.body);
    out
}

fn frontmatter_block(body: &str) -> Result<(&str, &str), String> {
    let offset = if body.starts_with("---\r\n") {
        "---\r\n".len()
    } else if body.starts_with("---\n") {
        "---\n".len()
    } else {
        return Err("profile markdown must start with frontmatter".to_string());
    };

    let closing = body[offset..]
        .find("\n---\r\n")
        .or_else(|| body[offset..].find("\n---\n"))
        .map(|index| offset + index)
        .ok_or_else(|| "profile markdown missing closing frontmatter".to_string())?;
    let rest_start = closing + "\n---".len();
    let rest = body[rest_start..]
        .strip_prefix("\r\n")
        .or_else(|| body[rest_start..].strip_prefix('\n'))
        .unwrap_or(&body[rest_start..]);

    Ok((&body[offset..closing], rest))
}

fn parse_list_scalar(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Some(inner) = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        return inner
            .split(',')
            .map(clean_scalar)
            .filter(|value| !value.is_empty())
            .collect();
    }

    vec![clean_scalar(trimmed)]
}

fn split_key_value(line: &str) -> Result<(&str, &str), String> {
    let (key, value) = line
        .split_once(':')
        .ok_or_else(|| format!("expected key/value line: {line}"))?;
    Ok((key.trim(), value.trim()))
}

fn clean_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn push_list(out: &mut String, key: &str, values: &[String]) {
    out.push_str(&format!("{key}:\n"));
    for value in values {
        out.push_str(&format!("  - {value}\n"));
    }
}

fn push_nested_list(out: &mut String, key: &str, values: &[String]) {
    out.push_str(&format!("  {key}:\n"));
    for value in values {
        out.push_str(&format!("    - {value}\n"));
    }
}

struct BuiltInProfileSpec {
    id: &'static str,
    name: &'static str,
    role: &'static str,
    skills: &'static [&'static str],
    access: AgentAccess,
    tools: &'static [&'static str],
    constraints: &'static [&'static str],
    outputs: &'static [&'static str],
}

fn built_in_profile(spec: BuiltInProfileSpec) -> LegacyAgentProfile {
    LegacyAgentProfile {
        id: spec.id.to_string(),
        name: spec.name.to_string(),
        status: "enabled".to_string(),
        version: 1,
        role: spec.role.to_string(),
        runtime: AgentRuntime::default(),
        skills: strings(spec.skills),
        access: spec.access,
        tools: strings(spec.tools),
        constraints: strings(spec.constraints),
        outputs: strings(spec.outputs),
        body: format!(
            "# {}\n\nYou are the xNAUT {} agent for {}.\n",
            spec.name, spec.name, spec.role
        ),
        rel: format!("System/Agents/{}.md", spec.name),
        built_in: true,
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn access_preset(name: &str) -> AgentAccess {
    match name {
        "loop_designer" => AgentAccess {
            read: strings(&["agent_profiles", "agent_loops"]),
            write: strings(&["agent_loop_drafts"]),
            denied: strings(&["source_code", "terminal", "secrets", "loop_activation"]),
        },
        "conservative" => AgentAccess {
            read: strings(&["agent_profiles", "vault_catalog"]),
            write: strings(&["agent_profiles_custom"]),
            denied: strings(&["source_code", "terminal", "secrets"]),
        },
        "vault_writer" => AgentAccess {
            read: strings(&["vault"]),
            write: strings(&["vault"]),
            denied: strings(&["source_code", "terminal", "secrets"]),
        },
        "builder" => AgentAccess {
            read: strings(&["vault", "source_code"]),
            write: strings(&["assigned_files"]),
            denied: strings(&["secrets"]),
        },
        "reviewer" => AgentAccess {
            read: strings(&["vault", "source_code", "test_output"]),
            write: strings(&["review_notes"]),
            denied: strings(&["secrets"]),
        },
        _ => AgentAccess {
            read: strings(&["vault"]),
            write: Vec::new(),
            denied: strings(&["source_code", "terminal", "secrets"]),
        },
    }
}

fn validate_frontmatter_values(label: &str, values: &[String]) -> Result<(), String> {
    for value in values {
        validate_frontmatter_value(label, value)?;
    }
    Ok(())
}

fn validate_frontmatter_value(label: &str, value: &str) -> Result<(), String> {
    if value.contains('\r') || value.contains('\n') || value.contains("---") {
        return Err(format!("invalid frontmatter value for {label}"));
    }
    Ok(())
}

/// Refuse a rel path whose any component is a symlink. Also used by the MCP
/// document tools (XNAUT-14), where `..`-rejection alone would still let a
/// symlinked directory inside the project scope point at the rest of the disk.
pub(crate) fn reject_symlinks_in_rel(root: &Path, rel: &str) -> Result<(), String> {
    reject_backslash_rel(rel)?;

    let mut current = String::new();
    for component in rel.split('/') {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(component);

        let path = crate::vault::safe_join(root, &current)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!("refusing symlink path: {}", path.display()));
            }
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(format!("metadata {}: {err}", path.display())),
        }
    }

    Ok(())
}

fn read_profiles_dir(
    root: &Path,
    dir_rel: &str,
    profiles: &mut Vec<LegacyAgentProfile>,
) -> Result<(), String> {
    reject_backslash_rel(dir_rel)?;
    let dir = crate::vault::safe_join(root, dir_rel)?;
    reject_symlinks_in_rel(root, dir_rel)?;
    let metadata = match fs::symlink_metadata(&dir) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("metadata {}: {err}", dir.display())),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("refusing symlink directory: {}", dir.display()));
    }
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|e| format!("metadata {}: {e}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let child_rel = format!("{dir_rel}/{name}");
        reject_backslash_rel(&child_rel)?;

        if metadata.is_dir() {
            read_profiles_dir(root, &child_rel, profiles)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }

        let rel = child_rel;
        let abs = crate::vault::safe_join(root, &rel)?;
        reject_symlinks_in_rel(root, &rel)?;
        let body = fs::read_to_string(&abs).map_err(|e| format!("read {}: {e}", abs.display()))?;
        profiles.push(parse_profile_markdown(&rel, &body, is_built_in_rel(&rel))?);
    }

    Ok(())
}

fn ensure_agent_rel(rel: &str) -> Result<(), String> {
    reject_backslash_rel(rel)?;
    if rel.starts_with("System/Agents/") && rel.ends_with(".md") {
        Ok(())
    } else {
        Err("profile path must be under System/Agents and end with .md".to_string())
    }
}

fn ensure_custom_rel(rel: &str) -> Result<(), String> {
    reject_backslash_rel(rel)?;
    if rel.starts_with("System/Agents/Custom/") && rel.ends_with(".md") {
        Ok(())
    } else {
        Err("custom profile path must be under System/Agents/Custom and end with .md".to_string())
    }
}

fn reject_backslash_rel(rel: &str) -> Result<(), String> {
    if rel.contains('\\') {
        Err("profile path must not contain backslashes".to_string())
    } else {
        Ok(())
    }
}

fn is_built_in_rel(rel: &str) -> bool {
    built_in_profiles().iter().any(|profile| profile.rel == rel)
}

fn is_built_in_id(id: &str) -> bool {
    built_in_profiles().iter().any(|profile| profile.id == id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_spoken_name_surface_resolves_through_one_fn() {
        // The first dogfood run died on "claudi" (display name) vs "claude"
        // (handle). This pins the fix's shape: the nudge and both ticket
        // owner writes go through resolve_spoken_handle, so a display name
        // can never again become a wake miss or an unmatchable ticket owner.
        let nudge = include_str!("nudge.rs");
        assert!(nudge.contains("resolve_spoken_handle"), "nudge stopped resolving spoken names");
        let tools = include_str!("agent_tools.rs");
        assert!(
            tools.matches("resolve_spoken_handle").count() >= 2,
            "ticket owner writes stopped resolving spoken names"
        );
    }

    #[test]
    fn the_agent_facing_roster_never_names_a_runtime_or_model() {
        // André 2026-08-26: agents address each other by TAG. A model that
        // knows which model it is talking to postures instead of answering.
        // This asserts the shape of what list_agents hands a model; it is
        // deliberately a source check because roster_snapshot reads the real
        // profile store, which a unit test has no business touching.
        let source = include_str!("agent_profiles.rs");
        let snapshot = source
            .split("pub fn roster_snapshot")
            .nth(1)
            .expect("roster_snapshot exists");
        let body = &snapshot[..snapshot.find("\n}\n").unwrap_or(snapshot.len())];
        assert!(
            !body.contains("runtime_id") && !body.contains("\"model\""),
            "roster_snapshot leaked a runtime or model to the agent-facing roster"
        );
    }

    use super::*;

    #[test]
    /// A store written before a field existed must not stay that way. This is
    /// the trap agents.toml fell into (XNAUT-182) and profiles were one door
    /// down: seeded once, rewritten only when someone edits a profile, so a
    /// field added later never reaches a machine that has run xNAUT before.
    #[test]
    fn a_profile_written_before_a_field_existed_gets_it_filled() {
        let mut store = AgentProfileStore {
            version: 1,
            profiles: vec![AgentProfile {
                handle: "old".into(),
                display_name: "Old".into(),
                tagline: String::new(),
                purpose: String::new(),
                runtime_id: "codex".into(),
                provider: "nautgate".into(),
                model: "gpt-5.6-sol".into(),
                chat_model: String::new(),
                reasoning_effort: String::new(),
                execution: AgentExecution::Local,
                role: "specialist".into(),
                capabilities: vec![],
                notifications: true,
                // What a profile written before the field existed looks like.
                accent_color: String::new(),
                policy: Default::default(),
                default_project: None,
                created_at: String::new(),
                updated_at: String::new(),
            }],
        };
        assert!(backfill_profiles(&mut store), "a gap has to be reported as a change");
        assert_eq!(store.profiles[0].accent_color, DEFAULT_ACCENT_COLOR);
        // Idempotent: a second pass must not claim a change, or the store is
        // rewritten on every single turn.
        assert!(!backfill_profiles(&mut store));
    }

    #[test]
    /// One field used to mean two things: what the chat turn asks an API for,
    /// and what gets handed to a CLI as `--model`. Pointing NautBot's chat at a
    /// local model to get tool calls back would otherwise have launched
    /// `codex --model lmstudio/qwen/...`, which codex has never heard of.
    #[test]
    fn the_chat_model_and_the_launch_model_are_separate() {
        let mut profile = AgentProfile {
            handle: "nautbot".into(),
            display_name: "NautBot".into(),
            tagline: String::new(),
            purpose: String::new(),
            runtime_id: "codex".into(),
            provider: "nautgate".into(),
            model: "gpt-5.6-sol".into(),
            chat_model: String::new(),
            reasoning_effort: String::new(),
            execution: AgentExecution::Local,
            role: "core-orchestrator".into(),
            capabilities: vec![],
            notifications: true,
            accent_color: default_accent_color(),
            policy: Default::default(),
            default_project: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        // Unset: every profile written before this field existed keeps behaving
        // exactly as it did.
        assert_eq!(profile.chat_model_or_model(), "gpt-5.6-sol");

        profile.chat_model = "lmstudio/qwen/qwen3.6-35b-a3b".into();
        assert_eq!(profile.chat_model_or_model(), "lmstudio/qwen/qwen3.6-35b-a3b");
        // The launch model is untouched, which is the whole point.
        assert_eq!(profile.model, "gpt-5.6-sol");

        // Whitespace is not a setting.
        profile.chat_model = "   ".into();
        assert_eq!(profile.chat_model_or_model(), "gpt-5.6-sol");
    }

    #[test]
    /// A reply that silently lost its tools is indistinguishable from an agent
    /// that chose not to act, and it cost four days twice (XNAUT-195): once to
    /// an Anthropic credit balance, once to NautGate routing every OpenAI model
    /// over a transport with no tool support. The notice has to name the model
    /// and quote the upstream, because those are the two things that turn "the
    /// feature is missing" into "this route is broken".
    #[test]
    fn the_tool_failure_notice_names_the_model_and_the_upstream() {
        let notice = tool_failure_notice(
            "gpt-5.6-sol",
            "502 Bad Gateway: upstream_failed (502): tool calls are not supported by this transport",
        );
        assert!(notice.contains("gpt-5.6-sol"), "{notice}");
        assert!(notice.contains("tool calls are not supported by this transport"), "{notice}");
        assert!(notice.to_lowercase().contains("without tools"), "{notice}");
        // It must be appended, not substituted: the answer above is still the
        // answer, and throwing it away would be its own kind of silence.
        assert!(notice.starts_with("\n\n---"), "the notice has to append: {notice}");
    }

    #[test]
    fn parses_profile_frontmatter_and_body() {
        let sample = r#"---
xnaut_agent: true
id: architect
name: Architect
status: enabled
version: 1
role: architecture
skills:
  - create-architecture
access:
  read:
    - vault
  write:
    - vault
  denied:
    - source_code
tools:
  - read_vault
constraints:
  - Do not edit implementation code.
outputs:
  - architecture
---
# Persona

You are a systems architect.
"#;

        let profile = parse_profile_markdown("System/Agents/Architect.md", sample, true).unwrap();

        assert_eq!(profile.id, "architect");
        assert_eq!(profile.name, "Architect");
        assert_eq!(profile.access.denied, vec!["source_code"]);
        assert!(profile.body.contains("systems architect"));
        assert!(profile.built_in);
    }

    #[test]
    fn parses_crlf_profile_frontmatter_and_body() {
        let sample = "---\r\nxnaut_agent: true\r\nid: architect\r\nname: Architect\r\nstatus: enabled\r\nversion: 1\r\nrole: architecture\r\nskills:\r\n  - create-architecture\r\naccess:\r\n  read:\r\n    - vault\r\n  write:\r\n    - vault\r\n  denied:\r\n    - source_code\r\ntools:\r\n  - read_vault\r\nconstraints:\r\n  - Do not edit implementation code.\r\noutputs:\r\n  - architecture\r\n---\r\n# Persona\r\n\r\nYou are a systems architect.\r\n";

        let profile = parse_profile_markdown("System/Agents/Architect.md", sample, true).unwrap();

        assert_eq!(profile.id, "architect");
        assert_eq!(profile.access.denied, vec!["source_code"]);
        assert!(profile.body.contains("systems architect"));
    }

    #[test]
    fn render_round_trips_required_fields() {
        let profile = LegacyAgentProfile {
            id: "reviewer".to_string(),
            name: "Reviewer".to_string(),
            status: "enabled".to_string(),
            version: 1,
            role: "review".to_string(),
            runtime: AgentRuntime {
                provider: "lmstudio".to_string(),
                model: "qwen/qwen3.6-35b-a3b".to_string(),
                mode: "chat".to_string(),
            },
            skills: vec!["review-implementation".to_string()],
            access: AgentAccess {
                read: vec!["vault".to_string()],
                write: vec!["draft_notes".to_string()],
                denied: vec!["secrets".to_string()],
            },
            tools: vec!["read_vault".to_string()],
            constraints: vec!["Do not approve missing tests.".to_string()],
            outputs: vec!["qa-report".to_string()],
            body: "# Persona\n\nYou review implementation readiness.\n".to_string(),
            rel: "System/Agents/Reviewer.md".to_string(),
            built_in: true,
        };

        let rendered = render_profile_markdown(&profile);
        let parsed = parse_profile_markdown(&profile.rel, &rendered, profile.built_in).unwrap();

        assert_eq!(parsed.id, "reviewer");
        assert_eq!(parsed.runtime.provider, "lmstudio");
        assert_eq!(parsed.runtime.model, "qwen/qwen3.6-35b-a3b");
        assert_eq!(parsed.runtime.mode, "chat");
        assert_eq!(parsed.skills, vec!["review-implementation"]);
        assert_eq!(parsed.access.write, vec!["draft_notes"]);
        assert!(parsed.body.contains("implementation readiness"));
    }

    #[test]
    fn missing_runtime_defaults_to_global_chat() {
        let parsed = parse_profile_markdown(
            "System/Agents/Architect.md",
            &valid_profile_markdown(),
            true,
        )
        .unwrap();

        assert_eq!(parsed.runtime, AgentRuntime::default());
    }

    #[test]
    fn built_in_agentfather_has_conservative_access() {
        let profiles = built_in_profiles();
        let father = profiles.iter().find(|p| p.id == "agentfather").unwrap();
        assert!(father.tools.contains(&"create_agent_profile".to_string()));
        assert!(father.access.denied.contains(&"source_code".to_string()));
        assert!(father.access.denied.contains(&"terminal".to_string()));
    }

    #[test]
    fn built_in_loopbuilder_can_only_create_drafts() {
        let profiles = built_in_profiles();
        let builder = profiles.iter().find(|p| p.id == "loopbuilder").unwrap();
        assert_eq!(builder.access.write, vec!["agent_loop_drafts"]);
        assert!(builder
            .access
            .denied
            .contains(&"loop_activation".to_string()));
        assert!(builder.access.denied.contains(&"source_code".to_string()));
        assert!(builder.tools.contains(&"loop_validate".to_string()));
    }

    #[test]
    fn full_project_access_preset_denies_secrets() {
        let access = access_preset("builder");

        assert!(access.read.contains(&"source_code".to_string()));
        assert!(access.write.contains(&"assigned_files".to_string()));
        assert!(access.denied.contains(&"secrets".to_string()));
    }

    #[test]
    fn narrower_access_presets_remain_constrained() {
        let constrained = [
            access_preset("conservative"),
            access_preset("vault_writer"),
            access_preset("vault_reader"),
            access_preset("unknown"),
        ];

        for access in constrained {
            assert!(!access.read.contains(&"source_code".to_string()));
            assert!(!access.write.contains(&"assigned_files".to_string()));
            assert!(access.denied.contains(&"source_code".to_string()));
            assert!(access.denied.contains(&"terminal".to_string()));
            assert!(access.denied.contains(&"secrets".to_string()));
        }
    }

    #[test]
    fn profile_filename_is_safe() {
        assert_eq!(
            profile_rel_for_id("SAP Migration Architect"),
            "System/Agents/Custom/sap-migration-architect.md"
        );
        assert_eq!(profile_rel_for_id("../bad"), "System/Agents/Custom/bad.md");
    }

    #[test]
    fn recursive_listing_accepts_relative_dirs() {
        let root =
            std::env::temp_dir().join(format!("xnaut-agent-profiles-{}", std::process::id()));
        let custom = root.join("System/Agents/Custom");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(custom.join("Analyst.md"), valid_profile_markdown()).unwrap();

        let mut profiles = Vec::new();
        read_profiles_dir(&root, "System/Agents", &mut profiles).unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].rel, "System/Agents/Custom/Analyst.md");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_frontmatter_scalar_injection() {
        let mut profile = valid_profile();
        profile.name = "Good\nrole: injected".to_string();

        let err = validate_profile_frontmatter(&profile).unwrap_err();

        assert!(err.contains("name"));
    }

    #[test]
    fn rejects_frontmatter_list_injection() {
        let mut profile = valid_profile();
        profile.skills = vec!["safe\n  - injected".to_string()];

        let err = validate_profile_frontmatter(&profile).unwrap_err();

        assert!(err.contains("skills"));
    }

    #[test]
    fn built_in_profiles_have_valid_frontmatter() {
        for profile in built_in_profiles() {
            validate_profile_frontmatter(&profile).unwrap();
            let rendered = render_profile_markdown(&profile);
            parse_profile_markdown(&profile.rel, &rendered, profile.built_in).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn recursive_listing_skips_symlinked_profiles_and_dirs() {
        let root =
            std::env::temp_dir().join(format!("xnaut-agent-symlinks-{}", std::process::id()));
        let agents = root.join("System/Agents");
        let outside = root.join("Outside");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("External.md"), valid_profile_markdown()).unwrap();
        std::os::unix::fs::symlink(outside.join("External.md"), agents.join("Linked.md")).unwrap();
        std::os::unix::fs::symlink(&outside, agents.join("LinkedDir")).unwrap();

        let mut profiles = Vec::new();
        read_profiles_dir(&root, "System/Agents", &mut profiles).unwrap();

        assert!(profiles.is_empty());

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_command_target_symlink_paths() {
        let root = std::env::temp_dir().join(format!(
            "xnaut-agent-command-symlink-{}",
            std::process::id()
        ));
        let custom = root.join("System/Agents/Custom");
        let outside = root.join("Outside.md");
        let link = custom.join("Linked.md");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(&outside, valid_profile_markdown()).unwrap();
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        let err = reject_symlinks_in_rel(&root, "System/Agents/Custom/Linked.md").unwrap_err();

        assert!(err.contains("symlink"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn recursive_listing_rejects_symlinked_parent_components() {
        let root =
            std::env::temp_dir().join(format!("xnaut-agent-parent-symlink-{}", std::process::id()));
        let outside = root.join("Outside");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(outside.join("Agents")).unwrap();
        std::fs::write(outside.join("Agents/External.md"), valid_profile_markdown()).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("System")).unwrap();

        let mut profiles = Vec::new();
        let err = read_profiles_dir(&root, "System/Agents", &mut profiles).unwrap_err();

        assert!(err.contains("symlink"));
        assert!(profiles.is_empty());

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn save_rejects_symlinked_parent_before_creating_outside_dirs() {
        // Writes the real vault and resolves its root more than once (here and
        // inside agent_profile_save), so it cannot run while another test has
        // XNAUT_TEST_VAULT set.
        let _vault = crate::vault::test_vault_lock();
        let root = crate::vault::vault_root("work").unwrap();
        let link_name = format!("xnaut-save-parent-symlink-{}", std::process::id());
        let link_rel = format!("System/Agents/Custom/{link_name}");
        let rel = format!("{link_rel}/child/Profile.md");
        let link = crate::vault::safe_join(&root, &link_rel).unwrap();
        let outside =
            std::env::temp_dir().join(format!("xnaut-save-parent-outside-{}", std::process::id()));
        let custom = crate::vault::safe_join(&root, "System/Agents/Custom").unwrap();
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        let mut profile = valid_profile();
        profile.rel = rel;
        let err = agent_profile_save(profile).unwrap_err();

        assert!(err.contains("symlink"));
        assert!(!outside.join("child").exists());

        let _ = std::fs::remove_file(link);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn rejects_backslash_profile_paths() {
        assert!(ensure_agent_rel("System/Agents/Custom/..\\..\\outside.md").is_err());
        assert!(ensure_custom_rel("System/Agents/Custom/..\\..\\outside.md").is_err());
    }

    #[test]
    fn rejects_xnaut_agent_not_true() {
        let sample =
            valid_profile_markdown().replacen("xnaut_agent: true", "xnaut_agent: false", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "xnaut_agent must be true");
    }

    #[test]
    fn rejects_empty_id() {
        let sample = valid_profile_markdown().replacen("id: architect", "id: ", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "id must not be empty");
    }

    #[test]
    fn rejects_empty_name() {
        let sample = valid_profile_markdown().replacen("name: Architect", "name: ", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "name must not be empty");
    }

    #[test]
    fn rejects_invalid_status() {
        let sample = valid_profile_markdown().replacen("status: enabled", "status: paused", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "status must be enabled or disabled");
    }

    #[test]
    fn rejects_missing_version() {
        let sample = valid_profile_markdown().replacen("version: 1\n", "", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "version must be present");
    }

    #[test]
    fn rejects_missing_role() {
        let sample = valid_profile_markdown().replacen("role: architecture\n", "", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "role must not be empty");
    }

    #[test]
    fn rejects_empty_role() {
        let sample = valid_profile_markdown().replacen("role: architecture", "role: ", 1);

        let err = parse_profile_markdown("System/Agents/Architect.md", &sample, true).unwrap_err();

        assert_eq!(err, "role must not be empty");
    }

    fn valid_profile_markdown() -> String {
        r#"---
xnaut_agent: true
id: architect
name: Architect
status: enabled
version: 1
role: architecture
skills:
  - create-architecture
access:
  read:
    - vault
  write:
    - vault
  denied:
    - source_code
tools:
  - read_vault
constraints:
  - Do not edit implementation code.
outputs:
  - architecture
---
# Persona

You are a systems architect.
"#
        .to_string()
    }

    fn valid_profile() -> LegacyAgentProfile {
        LegacyAgentProfile {
            id: "custom-reviewer".to_string(),
            name: "Custom Reviewer".to_string(),
            status: "enabled".to_string(),
            version: 1,
            role: "review".to_string(),
            runtime: AgentRuntime::default(),
            skills: vec!["review-implementation".to_string()],
            access: AgentAccess {
                read: vec!["vault".to_string()],
                write: vec!["draft_notes".to_string()],
                denied: vec!["secrets".to_string()],
            },
            tools: vec!["read_vault".to_string()],
            constraints: vec!["Do not approve missing tests.".to_string()],
            outputs: vec!["qa-report".to_string()],
            body: "# Persona\n\nYou review implementation readiness.\n".to_string(),
            rel: "System/Agents/Custom/custom-reviewer.md".to_string(),
            built_in: false,
        }
    }

    fn identity_profile(handle: &str) -> AgentProfile {
        AgentProfile {
            handle: handle.to_string(),
            display_name: "Build Mate".to_string(),
            tagline: "Ships careful changes".to_string(),
            purpose: "Implement scoped work and verify the result.".to_string(),
            runtime_id: "codex".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5".to_string(),
            chat_model: String::new(),
            reasoning_effort: String::new(),
            execution: AgentExecution::Local,
            role: "builder".to_string(),
            capabilities: vec!["code".to_string(), "tests".to_string()],
            notifications: true,
            policy: crate::policy::AgentPolicy::default(),
            accent_color: "#f5b840".to_string(),
            default_project: Some("xnaut".to_string()),
            created_at: "2026-08-14T12:00:00Z".to_string(),
            updated_at: "2026-08-14T12:00:00Z".to_string(),
        }
    }

    fn identity_test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "xnaut-agent-identity-{name}-{}-{}.toml",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn identity_store_round_trips_and_normalizes_handles() {
        let path = identity_test_path("roundtrip");
        let store = AgentProfileStore {
            version: PROFILE_STORE_VERSION,
            profiles: vec![identity_profile("@Build-Mate")],
        };

        write_profile_store(&path, &store).unwrap();
        let loaded = load_profile_store(&path).unwrap();

        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.profiles[0].handle, "build-mate");
        assert_eq!(loaded.profiles[0].execution, AgentExecution::Local);
        assert!(loaded.profiles[0].notifications);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn identity_store_rejects_case_insensitive_duplicate_handles() {
        let path = identity_test_path("duplicates");
        let store = AgentProfileStore {
            version: PROFILE_STORE_VERSION,
            profiles: vec![identity_profile("Reviewer"), identity_profile("reviewer")],
        };
        write_profile_store(&path, &store).unwrap();

        let error = load_profile_store(&path).unwrap_err();

        assert!(error.contains("duplicate"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn identity_validation_enforces_tagline_limit_and_capability_encoding() {
        let mut profile = identity_profile("builder");
        profile.tagline = "x".repeat(73);
        assert!(validate_identity_profile(&profile)
            .unwrap_err()
            .contains("72"));

        profile.tagline = "Within the limit".to_string();
        profile.capabilities = vec!["code,tests".to_string()];
        assert!(validate_identity_profile(&profile)
            .unwrap_err()
            .contains("comma-free"));

        profile.capabilities = vec!["code".to_string()];
        profile.role = "builder\nENGRAM_CAPABILITIES=unsafe".to_string();
        assert!(validate_identity_profile(&profile)
            .unwrap_err()
            .contains("control"));
    }

    #[test]
    fn custom_runtime_ids_seed_as_valid_bounded_handles() {
        assert_eq!(runtime_handle("  My Custom/Runtime  "), "my-custom-runtime");
        assert_eq!(runtime_handle("***"), "agent");
        let long = runtime_handle(&"A".repeat(100));
        assert_eq!(long.len(), 64);
        validate_handle(&long).unwrap();
    }

    #[test]
    fn identity_delete_is_handle_based_and_protects_nautbot() {
        let path = identity_test_path("delete");
        let store = AgentProfileStore {
            version: PROFILE_STORE_VERSION,
            profiles: vec![identity_profile("builder")],
        };
        write_profile_store(&path, &store).unwrap();

        delete_identity_profile(&path, "@builder").unwrap();
        let remaining = load_profile_store(&path).unwrap().profiles;
        // The deleted one is gone; the seeded ones are re-created. Asserting a
        // COUNT here broke the moment the Librarian joined the roster, which
        // is a fact about seeding rather than about deleting.
        assert!(!remaining.iter().any(|profile| profile.handle == "builder"));
        let nautbot = remaining
            .iter()
            .find(|profile| profile.handle == "nautbot")
            .expect("nautbot is re-seeded");
        assert_eq!(nautbot.provider, "nautgate");
        assert_eq!(nautbot.model, "gpt-5.6-sol");
        assert_eq!(nautbot.reasoning_effort, "high");
        assert!(
            remaining.iter().any(|profile| profile.handle == "librarian"),
            "the Librarian is part of the seeded roster now"
        );
        assert!(delete_identity_profile(&path, "@nautbot")
            .unwrap_err()
            .contains("protected"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mesh_identity_uses_the_engram_client_environment_contract() {
        let env = mesh_identity_env(&identity_profile("build-mate"));

        assert_eq!(
            env.get("ENGRAM_NICKNAME").map(String::as_str),
            Some("build-mate")
        );
        assert_eq!(env.get("ENGRAM_ROLE").map(String::as_str), Some("builder"));
        assert_eq!(
            env.get("ENGRAM_CAPABILITIES").map(String::as_str),
            Some("code,tests")
        );
    }

    #[test]
    fn identity_profile_input_defaults_non_identity_metadata() {
        let profile: AgentProfile = serde_json::from_value(serde_json::json!({
            "handle": "helper",
            "display_name": "Helper",
            "tagline": "Helps",
            "purpose": "Help with project work.",
            "runtime_id": "codex",
            "provider": "openai",
            "model": "",
            "role": "assistant",
            "capabilities": []
        }))
        .unwrap();

        assert_eq!(profile.execution, AgentExecution::Local);
        assert!(profile.notifications);
        assert_eq!(profile.accent_color, DEFAULT_ACCENT_COLOR);
        assert!(profile.created_at.is_empty());
        assert!(profile.updated_at.is_empty());
    }

    #[test]
    fn agent_project_prepare_creates_and_resolves_an_explicit_project_folder() {
        let root = std::env::temp_dir().join(format!(
            "xnaut-agent-project-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let project = root.join("honey-site");

        let resolved = agent_project_prepare(project.to_string_lossy().into_owned(), true).unwrap();

        assert_eq!(std::path::PathBuf::from(resolved), project.canonicalize().unwrap());
        assert!(project.is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn agent_project_prepare_refuses_missing_existing_and_broad_home_scopes() {
        let missing = std::env::temp_dir().join(format!(
            "xnaut-missing-project-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        assert!(agent_project_prepare(missing.to_string_lossy().into_owned(), false)
            .unwrap_err()
            .contains("not found"));

        if let Some(home) = dirs::home_dir() {
            assert!(agent_project_prepare(home.to_string_lossy().into_owned(), false)
                .unwrap_err()
                .contains("not the filesystem root or your home folder"));
        }
    }
}
