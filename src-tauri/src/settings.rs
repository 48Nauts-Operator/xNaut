// Typed settings store for Tasks Mode (v1.6). Persists as JSON at
// ~/.config/xnaut/settings.json (same config dir as agents.toml).
// Settings carry forge tokens / LLM keys — file is chmod 600 on write.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectCategory {
    /// UI label, e.g. "Development".
    pub label: String,
    /// Folder under project_root, e.g. "02-Development".
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmSettings {
    #[serde(default)]
    pub provider: String,
    /// OpenAI-compatible endpoint, e.g. http://localhost:11434/v1 (Ollama)
    /// or http://localhost:8090/v1 (NautGate).
    pub endpoint: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Run the coding-agent harnesses (Claude Code, Codex, pi) against this
    /// endpoint instead of their own cloud. Off by default: an agent with a
    /// working subscription should keep using it.
    #[serde(default)]
    pub harness_local: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmProviderSettings {
    pub name: String,
    pub endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentChatSelection {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EngramSettings {
    #[serde(default)]
    pub enabled: bool,
    /// Engram-OSS API base, e.g. http://stargate.tail138398.ts.net:8085.
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectManagementSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub repo_path: String,
    #[serde(default)]
    pub remote_url: String,
}

/// The core team's knobs (XNAUT-357).
///
/// `enabled` is false and stays false until somebody turns it on. The loop
/// spends money on its own — a weekly search, a review per finding, a full
/// agent run per PoC — and a feature that starts spending because it shipped is
/// not experimental, it is a surprise.
///
/// `poc_threshold` is THE cost knob. Everything below it costs one review;
/// everything above it costs a run. Andre moves this number, not the code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreTeamSettings {
    #[serde(default)]
    pub enabled: bool,
    /// The board findings are filed on.
    #[serde(default = "default_core_project")]
    pub project: String,
    /// The project whose decision log the Judge's verdict is written to. The
    /// question "should this go into xNAUT" belongs to xNAUT's log, not to the
    /// board the finding happens to sit on.
    #[serde(default = "default_core_decision_project")]
    pub decision_project: String,
    #[serde(default = "default_core_topics")]
    pub topics: Vec<String>,
    /// 0..=100. A finding at or above this earns a PoC run.
    #[serde(default = "default_poc_threshold")]
    pub poc_threshold: u32,
    /// How many days between beats.
    #[serde(default = "default_beat_days")]
    pub beat_days: u64,
    /// Wall clock for one PoC run, in minutes. Over it, the run stops and the
    /// ticket says so.
    #[serde(default = "default_poc_minutes")]
    pub poc_minutes: u64,
}

fn default_core_project() -> String {
    "CORE".to_string()
}
fn default_core_decision_project() -> String {
    "xnaut".to_string()
}
fn default_core_topics() -> Vec<String> {
    vec![
        "AI agent harnesses and orchestration".to_string(),
        "agent memory and context management".to_string(),
        "agent sandboxing and verification".to_string(),
    ]
}
/// 70/100. High enough that a finding has to be a good fit AND novel here to
/// clear it; low enough that a strong candidate with a middling port size
/// still does. It is a starting position, not a measurement.
fn default_poc_threshold() -> u32 {
    70
}
fn default_beat_days() -> u64 {
    7
}
fn default_poc_minutes() -> u64 {
    90
}

impl Default for CoreTeamSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            project: default_core_project(),
            decision_project: default_core_decision_project(),
            topics: default_core_topics(),
            poc_threshold: default_poc_threshold(),
            beat_days: default_beat_days(),
            poc_minutes: default_poc_minutes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopsSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub ticket_triage: TicketTriageSettings,
    // `max_parallel_runs` lived here and was written by the Multi-Agent
    // Manager's number box. It moved to @nautbot's profile with XNAUT-354: the
    // cap belongs to whoever starts the batch, and a global setting could not
    // say whose limit it was. An old key left on disk is ignored.
    /// DEPRECATED, kept as an alias for one release (XNAUT-370). It asked
    /// "whether THIS machine's sweep launches agents", which was the right
    /// answer to the wrong question: the machines differ in more than one way,
    /// and `instance.role` says which of the three things this one is for.
    ///
    /// `Option` rather than a defaulted bool so the three states stay
    /// distinguishable on disk: `Some` is a file written before roles existed
    /// and is migrated by `instance::adopt_role`, `None` is a file that never
    /// mentioned it and inherits the `fleet` default. Defaulting it to `true`
    /// here would make every file look like an explicit "dispatch" and hide
    /// which of the two it was.
    ///
    /// Still WRITTEN as well as read, for the one release: a settings file
    /// saved by this build has to keep working in the build before it, which
    /// is the exact two-machines-two-versions case this ticket is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_here: Option<bool>,
}

/// What this copy of xNAUT is, and what it is for (XNAUT-370). The behaviour
/// lives in `instance.rs`; this is only where it is stored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstanceSettings {
    /// The minted key for this instance. Never a hostname: hostnames are
    /// reassigned, duplicated across networks and edited by people, and the
    /// realm (XNAUT-369) joins on this. Empty until the app's first start
    /// mints one.
    #[serde(default)]
    pub id: String,
    /// `fleet` | `workstation` | `sandbox`. Empty means never configured, which
    /// reads as `fleet` — the behaviour every machine has had until now. Stored
    /// as a string rather than an enum on purpose: an unknown value written by
    /// a newer build must not fail the whole settings parse and drop the owner
    /// back to defaults.
    #[serde(default)]
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketTriageSettings {
    #[serde(default)]
    pub auto_enabled: bool,
    #[serde(default = "default_triage_interval")]
    pub interval_seconds: u64,
    #[serde(default)]
    pub forge_index: usize,
    #[serde(default)]
    pub repositories: Vec<String>,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub repo_path: Option<String>,
    #[serde(default)]
    pub vault_path: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_triage_interval() -> u64 {
    300
}

impl Default for TicketTriageSettings {
    fn default() -> Self {
        Self {
            auto_enabled: false,
            interval_seconds: default_triage_interval(),
            forge_index: 0,
            repositories: Vec::new(),
            provider: String::new(),
            model: String::new(),
            project: None,
            repo_path: None,
            vault_path: None,
        }
    }
}

impl Default for LoopsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            ticket_triage: TicketTriageSettings::default(),
            // None, not Some(true): a default-constructed Settings has never
            // been told anything about dispatch, and `instance::resolve` reads
            // that silence as `fleet` — same behaviour, one place that says so.
            dispatch_here: None,
        }
    }
}

/// The second clock of the idle reaper: sessions xNAUT did NOT launch
/// (XNAUT-344). Its own ceiling and its own switch, because the four-hour
/// capture clock in `scheduler::finished_and_idle` is right for a run that went
/// quiet and wrong for a terminal a person left open.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignSessionReaperSettings {
    /// Off means the pile comes back, so it ships on. It is still one switch
    /// and it is the owner's.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Hours of MEASURED idleness before a session xNAUT did not launch is
    /// collected. Twenty-four by default: six times the clock for our own runs,
    /// because these are the owner's terminals rather than finished agents, and
    /// a day of no activity at all is the earliest point where "he is done with
    /// it" beats "he stepped away". The number belongs to him, not to the code.
    #[serde(default = "default_foreign_idle_hours")]
    pub idle_hours: u64,
}

fn default_foreign_idle_hours() -> u64 {
    24
}

impl Default for ForeignSessionReaperSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_hours: default_foreign_idle_hours(),
        }
    }
}

/// The compaction-storm ceiling and its switch (XNAUT-348).
///
/// Both shapes are here on purpose, because they catch different failures. The
/// total is the backstop for a run that grinds all day; the rate is what
/// actually catches a storm, where the whole point is that the compactions come
/// back to back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionStormSettings {
    /// Off means the storm of 2026-09-11 runs for nine minutes again, so it
    /// ships on. It is still one switch and it is the owner's.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Compactions in one run before it is called thrashing. Zero turns this
    /// half off and leaves the rate.
    #[serde(default = "default_max_compactions_per_run")]
    pub max_per_run: u32,
    /// Compactions inside `window_minutes` before it is called thrashing. Zero
    /// turns this half off and leaves the total.
    #[serde(default = "default_max_compactions_per_window")]
    pub max_per_window: u32,
    /// How long the rate window is.
    #[serde(default = "default_compaction_window_minutes")]
    pub window_minutes: u64,
}

/// Twelve. The five real captures on this machine compacted 3, 3, 3, 5 and 10
/// times; the 10 is the storm run the owner stopped by hand. Twelve leaves a
/// run more than twice the worst healthy count observed and still ends the real
/// incident a couple of minutes past where he had to intervene.
fn default_max_compactions_per_run() -> u32 {
    12
}

/// Three, and it is Claude Code's own judgement rather than a number invented
/// here: its autocompact detector calls itself thrashing when "the context
/// refilled to the limit within 3 turns of the previous compact, 3 times in a
/// row". Three compactions inside ten minutes is that same verdict in
/// wall-clock terms.
fn default_max_compactions_per_window() -> u32 {
    3
}

/// Ten minutes. One healthy compaction on this machine took 7m 2s (measured in
/// `xnaut-claude-0efec8e7.jsonl`, which prints its own elapsed time), so three
/// of them cannot honestly fit inside ten minutes of real work. Long enough
/// that a slow compaction plus a slow turn is not a storm, short enough that
/// nine minutes of thrashing does not finish first.
fn default_compaction_window_minutes() -> u64 {
    10
}

impl Default for CompactionStormSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_per_run: default_max_compactions_per_run(),
            max_per_window: default_max_compactions_per_window(),
            window_minutes: default_compaction_window_minutes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerSettings {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub url: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

/// One configured forge host. `kind` selects the API dialect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeHost {
    /// "forgejo" | "github" | "gitlab"
    pub kind: String,
    /// Base URL, e.g. http://cosmos.tail138398.ts.net:3000 or https://api.github.com.
    pub base_url: String,
    /// Default owner/org for new repos and queries, e.g. "48Nauts".
    pub owner: String,
    /// Token. For Forgejo an empty value falls back to ~/.config/forgejo/token.
    #[serde(default)]
    pub token: Option<String>,
}

/// Linear's half of issue intake (XNAUT-382).
///
/// One personal API key for the workspace; the TEAM is per project, on the
/// board, because one workspace routinely has a team per product and two
/// xNAUT projects may read two of them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinearSettings {
    /// A Linear personal API key (`lin_api_…`). Goes in `Authorization` raw,
    /// with no `Bearer` prefix; Linear answers 400 to a prefixed one.
    #[serde(default)]
    pub api_key: String,
    /// Overridable so the intake tests can point at a local server. Empty is
    /// `https://api.linear.app/graphql`.
    #[serde(default)]
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Root for project folders: <project_root>/<category folder>/<name>.
    pub project_root: String,
    pub categories: Vec<ProjectCategory>,
    pub llm: LlmSettings,
    #[serde(default)]
    pub llm_providers: Vec<LlmProviderSettings>,
    #[serde(default)]
    pub agent_chat_selection: AgentChatSelection,
    pub engram: EngramSettings,
    #[serde(default)]
    pub project_management: ProjectManagementSettings,
    #[serde(default)]
    pub loops: LoopsSettings,
    /// Which machine this is and what it is for (XNAUT-370).
    #[serde(default)]
    pub instance: InstanceSettings,
    /// The core team's beat, threshold and topics (XNAUT-357).
    #[serde(default)]
    pub core_team: CoreTeamSettings,
    /// The idle reaper's ceiling and switch for sessions xNAUT did not launch.
    #[serde(default)]
    pub foreign_session_reaper: ForeignSessionReaperSettings,
    /// The supervisor's ceiling on how often a run may compact before it is
    /// treated as thrashing rather than working.
    #[serde(default)]
    pub compaction_storm: CompactionStormSettings,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerSettings>,
    /// Configured forge hosts; first entry is the default ("core") host.
    pub forges: Vec<ForgeHost>,
    /// Linear, as an issue-intake source (XNAUT-382). Machine-local because it
    /// holds a key; which PROJECTS read it is on the board, not here.
    #[serde(default)]
    pub linear: LinearSettings,
    /// Editor command for file clicks, e.g. "nvim". Empty = $EDITOR.
    #[serde(default)]
    pub editor: String,
    /// Fixed local port for the agent hook / project MCP HTTP server. Persisted
    /// so the URL pasted into claude/codex configs survives app restarts.
    #[serde(default = "default_mcp_port")]
    pub mcp_port: u16,
    /// Stable bearer token for the project MCP server. Empty means "not yet
    /// generated" — main.rs mints one on first launch and saves it here.
    #[serde(default)]
    pub mcp_token: String,
    /// Configured sandbox providers for the Sandbox Verify module (GitVM etc.).
    /// First entry is the default; mirrors the `forges` pattern.
    #[serde(default)]
    pub sandboxes: Vec<SandboxProviderSettings>,
    /// Fleet enrollment, reused automatically by every new task worker.
    /// Repository deploy keys are minted separately; forge tokens stay local.
    #[serde(default)]
    pub worker_network: WorkerNetworkSettings,
    /// Keys written by other xNaut versions/branches. Round-tripping them
    /// prevents one version's save from silently deleting another version's
    /// config (this bit us: an older dev build stripped mcp_token/loops/
    /// sandboxes from a live settings.json).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct WorkerNetworkSettings {
    #[serde(default)]
    pub auth_key: String,
    #[serde(default)]
    pub tags: String,
}

impl std::fmt::Debug for WorkerNetworkSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerNetworkSettings").field("configured", &!self.auth_key.is_empty()).field("tags", &self.tags).finish()
    }
}

/// One configured sandbox backend for the Sandbox Verify module. `kind` selects
/// the driver dialect; only "gitvm" is implemented (e2b/daytona are future).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxProviderSettings {
    /// "gitvm" | "e2b" | "daytona"
    pub kind: String,
    /// Base URL of the provider API (the GITVM_URL value for gitvm).
    pub base_url: String,
    /// API key. Empty/absent for gitvm falls back to env GITVM_API_KEY.
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Resolves the API key for a sandbox provider: explicit `api_key`, else for
/// gitvm the `GITVM_API_KEY` environment variable. Mirrors `resolve_forge_token`.
pub fn resolve_sandbox_key(provider: &SandboxProviderSettings) -> Option<String> {
    if let Some(key) = &provider.api_key {
        if !key.trim().is_empty() {
            return Some(key.clone());
        }
    }
    if provider.kind == "gitvm" {
        if let Ok(key) = std::env::var("GITVM_API_KEY") {
            if !key.trim().is_empty() {
                return Some(key);
            }
        }
    }
    None
}

/// Default fixed port for the local hook/MCP server. Chosen high and uncommon
/// to avoid collisions; overridable via settings if it ever conflicts.
pub fn default_mcp_port() -> u16 {
    51737
}

impl Default for Settings {
    fn default() -> Self {
        let cat = |label: &str, folder: &str| ProjectCategory {
            label: label.into(),
            folder: folder.into(),
        };
        Self {
            project_root: shellexpand_home("~/DevHub_Studio/factory"),
            categories: vec![
                cat("Development", "02-Development"),
                cat("IOS", "03-iOS"),
                cat("Research", "04-Research"),
                cat("DevOps", "05-DevOps"),
                cat("Frontrow", "06-Frontrow"),
                cat("Docs", "08-Docs"),
                cat("Personal", "09-Personal"),
                cat("DAT", "11-DAT"),
                cat("PlayGround", "00-PlayGround"),
                cat("Blueprints", "01-Blueprints"),
                cat("Security-Research", "XX-Security-Research"),
            ],
            llm: LlmSettings {
                provider: "".into(),
                endpoint: "http://localhost:8090/v1".into(),
                model: "claude-sonnet-4-6".into(),
                api_key: None,
                system_prompt: None,
                harness_local: false,
            },
            llm_providers: Vec::new(),
            agent_chat_selection: AgentChatSelection::default(),
            engram: EngramSettings::default(),
            project_management: ProjectManagementSettings::default(),
            loops: LoopsSettings::default(),
            instance: InstanceSettings::default(),
            core_team: CoreTeamSettings::default(),
            foreign_session_reaper: ForeignSessionReaperSettings::default(),
            compaction_storm: CompactionStormSettings::default(),
            mcp_servers: vec![McpServerSettings {
                name: "excalidraw".into(),
                enabled: false,
                url: "http://127.0.0.1:3001/mcp".into(),
                api_key: None,
            }],
            forges: vec![ForgeHost {
                kind: "forgejo".into(),
                base_url: "http://cosmos.tail138398.ts.net:3000".into(),
                owner: "48Nauts".into(),
                token: None,
            }],
            linear: LinearSettings::default(),
            editor: String::new(),
            mcp_port: default_mcp_port(),
            mcp_token: String::new(),
            sandboxes: Vec::new(),
            worker_network: WorkerNetworkSettings::default(),
            extra: serde_json::Map::new(),
        }
    }
}

fn shellexpand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

fn config_dir() -> PathBuf {
    if crate::loop_acceptance::ENABLED { return crate::loop_acceptance::config(); }
    crate::loop_acceptance::platform_config_dir()
        .map(|p| p.join(if crate::CONTINUITY_PREVIEW { "xnaut/continuity-preview" } else if crate::JOURNAL_PREVIEW { "xnaut/journal-preview-3" } else if crate::FULL_WIKI_PREVIEW { "xnaut/full-wiki-preview" } else { "xnaut" }))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load_or_default() -> Settings {
    let path = settings_path();
    let mut settings = match std::fs::read_to_string(&path) {
        Ok(body) => serde_json::from_str(&body).unwrap_or_else(|e| {
            assert!(!crate::loop_acceptance::ENABLED,"Acceptance settings became invalid; refusing owner defaults");
            eprintln!(
                "[settings] parse error in {}: {e} — using defaults",
                path.display()
            );
            Settings::default()
        }),
        Err(_) => {
            assert!(!crate::loop_acceptance::ENABLED,"Acceptance settings disappeared; refusing owner defaults");
            Settings::default()
        },
    };
    if migrate_legacy_nautgate_settings(&mut settings) {
        if let Err(error) = save(&settings) {
            eprintln!("[settings] could not persist legacy NautGate migration: {error}");
        }
    }
    // The role the legacy `loops.dispatch_here` implies, for THIS reader only.
    // Nothing is written: `instance::adopt` at app start is the one caller
    // allowed to persist it. A migration that rewrites the owner's settings as
    // a side effect of reading them is how one build's save strips another's
    // keys, and `cargo test` reads this file too.
    crate::instance::adopt_role(&mut settings);
    settings
}

fn decode_webkit_local_storage_value(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(b"{") && bytes.get(1) != Some(&0) {
        return String::from_utf8(bytes.to_vec()).ok();
    }
    if bytes.len() % 2 != 0 {
        return None;
    }
    let words = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&words).ok()
}

fn legacy_nautgate_from_db(path: &std::path::Path) -> Option<(String, String)> {
    use rusqlite::OpenFlags;
    let connection = rusqlite::Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let bytes: Vec<u8> = connection
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'xnaut-settings' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok()?;
    let value: serde_json::Value = serde_json::from_str(&decode_webkit_local_storage_value(&bytes)?).ok()?;
    let token = value.get("apiKeyNautGate")?.as_str()?.trim().to_string();
    if token.is_empty() {
        return None;
    }
    let endpoint = value
        .get("nautgateUrl")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http://localhost:8090/v1")
        .to_string();
    Some((endpoint, token))
}

fn collect_legacy_local_storage_databases(
    directory: &std::path::Path,
    depth: usize,
    output: &mut Vec<PathBuf>,
) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_legacy_local_storage_databases(&path, depth - 1, output);
        } else if path.file_name().and_then(|name| name.to_str()) == Some("localstorage.sqlite3") {
            output.push(path);
        }
    }
}

fn apply_legacy_nautgate(settings: &mut Settings, endpoint: String, token: String) -> bool {
    if settings.llm_providers.iter().any(|p| p.name.eq_ignore_ascii_case("nautgate") && !p.enabled) { return false; }
    let already_configured = settings
        .llm_providers
        .iter()
        .find(|provider| provider.name.eq_ignore_ascii_case("nautgate"))
        .and_then(|provider| provider.api_key.as_deref())
        .is_some_and(|value| !value.trim().is_empty());
    if already_configured {
        return false;
    }

    if let Some(provider) = settings
        .llm_providers
        .iter_mut()
        .find(|provider| provider.name.eq_ignore_ascii_case("nautgate"))
    {
        provider.endpoint = endpoint.clone();
        provider.api_key = Some(token.clone());
        provider.enabled = true;
    } else {
        settings.llm_providers.push(LlmProviderSettings {
            name: "nautgate".to_string(),
            endpoint: endpoint.clone(),
            api_key: Some(token.clone()),
            enabled: true,
        });
    }
    if settings.llm.provider.eq_ignore_ascii_case("nautgate") {
        settings.llm.endpoint = endpoint;
        settings.llm.api_key = Some(token);
    }
    true
}

/// The original app used the short WebKit data-store id `xnaut`; the current
/// bundle id is `com.nautcode.xnaut`. WebKit isolates localStorage by that id,
/// so provider credentials entered before the rename are otherwise invisible
/// forever. Import only the missing NautGate credential into the owner-only
/// Rust settings file; never overwrite a token configured by the current app.
fn migrate_legacy_nautgate_settings(settings: &mut Settings) -> bool {
    if settings.llm_providers.iter().any(|p| p.name.eq_ignore_ascii_case("nautgate") && !p.enabled) { return false; }
    if settings
        .llm_providers
        .iter()
        .find(|provider| provider.name.eq_ignore_ascii_case("nautgate"))
        .and_then(|provider| provider.api_key.as_deref())
        .is_some_and(|value| !value.trim().is_empty())
    {
        return false;
    }
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let root = home.join("Library/WebKit/xnaut/WebsiteData/Default");
    let mut databases = Vec::new();
    collect_legacy_local_storage_databases(&root, 6, &mut databases);
    databases.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    for database in databases.into_iter().rev() {
        if let Some((endpoint, token)) = legacy_nautgate_from_db(&database) {
            return apply_legacy_nautgate(settings, endpoint, token);
        }
    }
    false
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let path = settings_path();
    let body = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    // Tokens/keys live in here — keep it owner-only, like ~/.config/forgejo/token.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Resolves the token for a forge host: explicit token, else for Forgejo the
/// FORGEJO_TOKEN env var or ~/.config/forgejo/token file.
pub fn resolve_forge_token(host: &ForgeHost) -> Option<String> {
    // Every source is cleaned the same way. A token goes straight into an
    // Authorization header, and a header value cannot carry a newline, a
    // tab or anything outside printable ASCII: reqwest refuses to build the
    // request with "builder error: failed to parse header value", and the
    // Tasks panel showed exactly that on 2026-09-11 for a token that had
    // been pasted into settings with its trailing newline.
    // The token is the LAST non-empty line: a pasted file often carries a
    // label or a username above it (the owner's did, 2026-09-11), and
    // gluing the lines together would forge a value that is wrong rather
    // than one that is refused. Within that line, only printable ASCII.
    let clean = |t: &str| -> Option<String> {
        let line = t.lines().map(str::trim).filter(|l| !l.is_empty()).last()?;
        let t: String = line.chars().filter(|c| c.is_ascii_graphic()).collect();
        (!t.is_empty()).then_some(t)
    };
    if let Some(t) = host.token.as_deref().and_then(clean) {
        return Some(t);
    }
    if host.kind == "forgejo" {
        if let Some(t) = std::env::var("FORGEJO_TOKEN").ok().as_deref().and_then(clean) {
            return Some(t);
        }
        if let Some(home) = dirs::home_dir() {
            if let Ok(t) = std::fs::read_to_string(home.join(".config/forgejo/token")) {
                if let Some(t) = clean(&t) {
                    return Some(t);
                }
            }
        }
    }
    None
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn settings_get(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Settings, String> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
pub async fn settings_set(
    state: tauri::State<'_, crate::state::AppState>,
    settings: Settings,
) -> Result<(), String> {
    save(&settings)?;
    *state.settings.lock().await = settings;
    crate::tool_support::forget_all();
    Ok(())
}

#[cfg(test)]
mod forge_token_tests {
    use super::*;
    #[test]
    fn legacy_credentials_cannot_reenable_disabled_gateway() {
        let mut settings=Settings::default();
        settings.llm_providers.push(LlmProviderSettings{name:"nautgate".into(),endpoint:"http://configured/v1".into(),api_key:None,enabled:false});
        assert!(!apply_legacy_nautgate(&mut settings,"http://legacy/v1".into(),"legacy-key".into()));
        assert!(!settings.llm_providers[0].enabled);
        assert!(settings.llm_providers[0].api_key.is_none());
    }

    #[test]
    fn a_pasted_token_with_a_newline_or_a_stray_byte_still_makes_a_valid_header() {
        // "builder error: failed to parse header value", Tasks panel, 2026-09-11.
        let host = |token: Option<&str>| ForgeHost {
            kind: "forgejo".into(),
            base_url: "http://cosmos.tail138398.ts.net:3000".into(),
            owner: "48Nauts".into(),
            token: token.map(str::to_string),
        };
        assert_eq!(resolve_forge_token(&host(Some("abc123\n"))).as_deref(), Some("abc123"));
        assert_eq!(resolve_forge_token(&host(Some("  abc123\r\n"))).as_deref(), Some("abc123"));
        assert_eq!(resolve_forge_token(&host(Some("abc\u{a0}123"))).as_deref(), Some("abc123"));
        // A label above the token is not part of the token.
        assert_eq!(resolve_forge_token(&host(Some("cand0rian\nabc123\n"))).as_deref(), Some("abc123"));
        for t in [Some("abc123"), Some("abc123\n")] {
            let v = resolve_forge_token(&host(t)).unwrap();
            assert!(reqwest::header::HeaderValue::from_str(&format!("token {v}")).is_ok());
        }
        // Whitespace only is no token, so the next source is tried.
        std::env::remove_var("FORGEJO_TOKEN");
        let none = resolve_forge_token(&host(Some("\n  \n")));
        assert!(none.is_none() || !none.unwrap().is_empty());
    }
}

#[cfg(test)]
mod tests {
    /// The Settings page reads the local providers' URL from localStorage, the
    /// request reads it from here. Two stores for one fact: LM Studio on 1238
    /// meant the model dropdown probed a dead 1234, said "not reachable", and
    /// could not be changed, while the request went to 1238 and failed on a
    /// model id the dropdown was never able to show.
    #[test]
    fn the_settings_page_hydrates_local_provider_urls_from_here() {
        let app = include_str!("../../src/js/app.js");
        for line in [
            "if (!settings.lmstudioUrl && lmstudio?.endpoint) settings.lmstudioUrl = originOf(lmstudio.endpoint);",
            "if (!settings.ollamaUrl && ollama?.endpoint) settings.ollamaUrl = originOf(ollama.endpoint);",
        ] {
            assert!(
                app.contains(line),
                "app.js stopped hydrating a local provider URL from the durable settings, \
                 so its model dropdown falls back to the vendor's stock port: {line}"
            );
        }
    }

    use super::*;

    #[test]
    fn default_settings_roundtrip_json() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.categories.len(), 11);
        assert_eq!(back.forges[0].kind, "forgejo");
    }

    #[test]
    fn old_settings_without_mcp_fields_parse_with_defaults() {
        // An existing settings.json predating the mcp_port/mcp_token fields must
        // still deserialize (via serde defaults) — otherwise load_or_default
        // would fall back to Default and silently wipe the user's real settings.
        // Simulate that file by stripping exactly those keys from a real Default.
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("mcp_port");
        obj.remove("mcp_token");
        obj.remove("sandboxes");
        let parsed: Settings =
            serde_json::from_value(value).expect("old settings must still parse");
        assert_eq!(parsed.mcp_port, 51737, "missing mcp_port must default");
        assert!(
            parsed.mcp_token.is_empty(),
            "missing mcp_token must default to empty (minted on launch)"
        );
        assert!(
            parsed.sandboxes.is_empty(),
            "missing sandboxes must default to empty"
        );
        assert_eq!(parsed.categories.len(), 11, "real settings preserved");
    }

    #[test]
    fn categories_map_labels_to_folders() {
        let s = Settings::default();
        let dev = s
            .categories
            .iter()
            .find(|c| c.label == "Development")
            .unwrap();
        assert_eq!(dev.folder, "02-Development");
        let ios = s.categories.iter().find(|c| c.label == "IOS").unwrap();
        assert_eq!(ios.folder, "03-iOS");
    }

    #[test]
    fn legacy_webkit_nautgate_token_is_imported_without_overwriting_current_credentials() {
        let root = std::env::temp_dir().join(format!(
            "xnaut-legacy-settings-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let database = root.join("localstorage.sqlite3");
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .execute(
                "CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB NOT NULL)",
                [],
            )
            .unwrap();
        let json = r#"{"nautgateUrl":"http://localhost:8090/v1","apiKeyNautGate":"legacy-token"}"#;
        let bytes = json
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        connection
            .execute(
                "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                rusqlite::params!["xnaut-settings", bytes],
            )
            .unwrap();
        drop(connection);

        let (endpoint, token) = legacy_nautgate_from_db(&database).unwrap();
        let mut settings = Settings::default();
        assert!(apply_legacy_nautgate(&mut settings, endpoint, token));
        let provider = settings
            .llm_providers
            .iter()
            .find(|provider| provider.name == "nautgate")
            .unwrap();
        assert_eq!(provider.endpoint, "http://localhost:8090/v1");
        assert_eq!(provider.api_key.as_deref(), Some("legacy-token"));

        assert!(!apply_legacy_nautgate(
            &mut settings,
            "http://other.invalid/v1".into(),
            "replacement".into()
        ));
        assert_eq!(
            settings.llm_providers.last().unwrap().api_key.as_deref(),
            Some("legacy-token")
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
