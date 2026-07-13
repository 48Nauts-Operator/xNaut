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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopsSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub ticket_triage: TicketTriageSettings,
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
    #[serde(default)]
    pub mcp_servers: Vec<McpServerSettings>,
    /// Configured forge hosts; first entry is the default ("core") host.
    pub forges: Vec<ForgeHost>,
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
            },
            llm_providers: Vec::new(),
            agent_chat_selection: AgentChatSelection::default(),
            engram: EngramSettings::default(),
            project_management: ProjectManagementSettings::default(),
            loops: LoopsSettings::default(),
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
            editor: String::new(),
            mcp_port: default_mcp_port(),
            mcp_token: String::new(),
            sandboxes: Vec::new(),
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
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load_or_default() -> Settings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(body) => serde_json::from_str(&body).unwrap_or_else(|e| {
            eprintln!(
                "[settings] parse error in {}: {e} — using defaults",
                path.display()
            );
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
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
    if let Some(t) = &host.token {
        if !t.is_empty() {
            return Some(t.clone());
        }
    }
    if host.kind == "forgejo" {
        if let Ok(t) = std::env::var("FORGEJO_TOKEN") {
            if !t.is_empty() {
                return Some(t);
            }
        }
        if let Some(home) = dirs::home_dir() {
            if let Ok(t) = std::fs::read_to_string(home.join(".config/forgejo/token")) {
                let t = t.trim().to_string();
                if !t.is_empty() {
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
    Ok(())
}

#[cfg(test)]
mod tests {
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
        let parsed: Settings = serde_json::from_value(value).expect("old settings must still parse");
        assert_eq!(parsed.mcp_port, 51737, "missing mcp_port must default");
        assert!(
            parsed.mcp_token.is_empty(),
            "missing mcp_token must default to empty (minted on launch)"
        );
        assert!(parsed.sandboxes.is_empty(), "missing sandboxes must default to empty");
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
}
