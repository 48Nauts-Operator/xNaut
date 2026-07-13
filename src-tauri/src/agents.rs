// Agent registry + launch dispatch. Ports the TUI_AGENT_CONFIG shape from
// Orca (src/shared/tui-agent-config.ts) but stores the registry as user-editable
// TOML at ~/.config/xnaut/agents.toml so users can add agents without rebuilding.

use crate::pty::{self, PtyConfig};
use crate::state::AppState;
use crate::status;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, State};

/// How an agent CLI accepts a prompt. Five strategies cover every coding CLI
/// we've seen in the wild — see Orca's tui-agent-config.ts for the original.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PromptInjectionMode {
    /// Prompt is the last positional argument (e.g. codex).
    Argv,
    /// Prompt is passed via a flag (e.g. claude `--prefill <prompt>`).
    FlagPrompt,
    /// Flag-and-prompt followed by an interactive flag.
    FlagPromptInteractive,
    /// Just `-i` (or similar); prompt sent later via stdin.
    FlagInteractive,
    /// Wait for TUI to render, then write prompt to stdin (e.g. pi).
    StdinAfterStart,
}

/// Pre-launch "trust this folder?" gate workaround. Some CLIs ask on first run;
/// Orca pre-writes the trust artifact so the menu never fires. Stubbed for now
/// — actual artifact paths will land in a follow-up.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PreflightTrust {
    Cursor,
    Copilot,
    Codex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Stable identifier used by the frontend and config file (e.g. "claude").
    pub id: String,
    /// Human label for UI (e.g. "Claude Code").
    pub label: String,
    /// Binary to look up on PATH for the "available?" check.
    pub detect_cmd: String,
    /// Binary actually exec'd (sometimes differs from detect_cmd — e.g. kiro/kiro-cli).
    pub launch_cmd: String,
    /// Optional extra args appended before the prompt (e.g. `["chat"]`).
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Process name expected in `ps`. Used by future status detection.
    pub expected_process: String,
    pub prompt_injection_mode: PromptInjectionMode,
    /// Flag that precedes the prompt for FlagPrompt / FlagPromptInteractive modes.
    pub draft_prompt_flag: Option<String>,
    /// Env var that carries the prompt for agents that prefer it that way (e.g. pi).
    pub draft_prompt_env_var: Option<String>,
    /// Pre-launch trust artifact to write.
    pub preflight_trust: Option<PreflightTrust>,
    /// Extra env vars set on launch — carries the NautGate routing
    /// (ANTHROPIC_BASE_URL / OPENAI_BASE_URL) the claudeps/justpi shell
    /// functions set, so agents launched from xNaut take the same path.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistry {
    pub agents: Vec<AgentConfig>,
}

impl AgentRegistry {
    pub fn find(&self, id: &str) -> Option<&AgentConfig> {
        self.agents.iter().find(|a| a.id == id)
    }
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn config_path() -> PathBuf {
    config_dir().join("agents.toml")
}

/// Default seed — five agents covering the most common cases.
/// Users can edit ~/.config/xnaut/agents.toml to add more.
fn default_registry() -> AgentRegistry {
    AgentRegistry {
        agents: vec![
            AgentConfig {
                id: "claude".into(),
                label: "Claude Code".into(),
                detect_cmd: "claude".into(),
                launch_cmd: "claude".into(),
                extra_args: vec!["--dangerously-skip-permissions".into()],
                expected_process: "claude".into(),
                prompt_injection_mode: PromptInjectionMode::FlagPrompt,
                draft_prompt_flag: Some("--prefill".into()),
                draft_prompt_env_var: None,
                preflight_trust: None,
                env: HashMap::from([("ANTHROPIC_BASE_URL".into(), "http://localhost:8090".into())]),
            },
            AgentConfig {
                id: "codex".into(),
                label: "Codex".into(),
                detect_cmd: "codex".into(),
                launch_cmd: "codex".into(),
                extra_args: vec![],
                expected_process: "codex".into(),
                prompt_injection_mode: PromptInjectionMode::Argv,
                draft_prompt_flag: None,
                draft_prompt_env_var: None,
                preflight_trust: Some(PreflightTrust::Codex),
                env: HashMap::from([("OPENAI_BASE_URL".into(), "http://localhost:8090/v1".into())]),
            },
            AgentConfig {
                id: "gemini".into(),
                label: "Gemini".into(),
                detect_cmd: "gemini".into(),
                launch_cmd: "gemini".into(),
                extra_args: vec![],
                expected_process: "gemini".into(),
                prompt_injection_mode: PromptInjectionMode::FlagPromptInteractive,
                draft_prompt_flag: Some("-p".into()),
                draft_prompt_env_var: None,
                preflight_trust: None,
                env: HashMap::new(),
            },
            AgentConfig {
                id: "grok".into(),
                label: "Grok".into(),
                detect_cmd: "grok".into(),
                launch_cmd: "grok".into(),
                extra_args: vec![],
                expected_process: "grok".into(),
                prompt_injection_mode: PromptInjectionMode::FlagInteractive,
                draft_prompt_flag: None,
                draft_prompt_env_var: None,
                preflight_trust: None,
                env: HashMap::new(),
            },
            AgentConfig {
                id: "opencode".into(),
                label: "OpenCode".into(),
                detect_cmd: "opencode".into(),
                launch_cmd: "opencode".into(),
                extra_args: vec![],
                expected_process: "opencode".into(),
                prompt_injection_mode: PromptInjectionMode::StdinAfterStart,
                draft_prompt_flag: None,
                draft_prompt_env_var: None,
                preflight_trust: None,
                env: HashMap::new(),
            },
            AgentConfig {
                id: "pi".into(),
                label: "Pi".into(),
                detect_cmd: "pi".into(),
                launch_cmd: "pi".into(),
                extra_args: vec![],
                expected_process: "pi".into(),
                prompt_injection_mode: PromptInjectionMode::StdinAfterStart,
                draft_prompt_flag: None,
                draft_prompt_env_var: None,
                preflight_trust: None,
                env: HashMap::from([("OPENAI_BASE_URL".into(), "http://localhost:8090/v1".into())]),
            },
        ],
    }
}

/// Loads the user's registry from `~/.config/xnaut/agents.toml`, writing a
/// default seed if the file doesn't exist yet.
pub fn load_or_seed_registry() -> Result<AgentRegistry, String> {
    let path = config_path();
    if !path.exists() {
        let dir = config_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
        let default = default_registry();
        let serialized = toml::to_string_pretty(&default)
            .map_err(|e| format!("failed to serialize default registry: {e}"))?;
        std::fs::write(&path, serialized)
            .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        return Ok(default);
    }
    let body = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    toml::from_str::<AgentRegistry>(&body)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

/// Quick PATH lookup — splits PATH and stat's each candidate. Cheap and avoids
/// pulling in a `which` crate just for this.
fn binary_on_path(bin: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let candidate = PathBuf::from(dir).join(bin);
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Serialize)]
pub struct AgentListing {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub injection_mode: PromptInjectionMode,
}

#[derive(Debug, Deserialize)]
pub struct LaunchAgentRequest {
    pub agent_id: String,
    /// Working directory for the spawned process — usually the worktree path.
    pub worktree_path: String,
    /// User's initial prompt. Optional — agent will start without one if absent.
    pub prompt: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct LaunchAgentResponse {
    pub session_id: String,
    pub agent_id: String,
    pub injection_mode: PromptInjectionMode,
}

/// Builds (argv, extra_env) for an agent given the injection mode.
fn build_launch(cfg: &AgentConfig, prompt: Option<&str>) -> (Vec<String>, HashMap<String, String>) {
    let mut argv: Vec<String> = vec![cfg.launch_cmd.clone()];
    argv.extend(cfg.extra_args.iter().cloned());
    let mut env = HashMap::new();

    // If an env-var carrier is configured (e.g. pi's ORCA_PI_PREFILL), set it
    // regardless of mode — the agent's own startup will pick it up.
    if let (Some(var), Some(p)) = (cfg.draft_prompt_env_var.as_ref(), prompt) {
        env.insert(var.clone(), p.to_string());
    }

    match cfg.prompt_injection_mode {
        PromptInjectionMode::Argv => {
            if let Some(p) = prompt {
                argv.push(p.to_string());
            }
        }
        PromptInjectionMode::FlagPrompt => {
            if let (Some(flag), Some(p)) = (cfg.draft_prompt_flag.as_ref(), prompt) {
                argv.push(flag.clone());
                argv.push(p.to_string());
            }
        }
        PromptInjectionMode::FlagPromptInteractive => {
            if let (Some(flag), Some(p)) = (cfg.draft_prompt_flag.as_ref(), prompt) {
                argv.push(flag.clone());
                argv.push(p.to_string());
            }
            argv.push("-i".into());
        }
        PromptInjectionMode::FlagInteractive => {
            argv.push("-i".into());
        }
        PromptInjectionMode::StdinAfterStart => {
            // No argv-side injection; prompt is written to stdin after spawn.
        }
    }

    (argv, env)
}

/// Stubbed preflight-trust artifact writer. The real per-agent artifact paths
/// belong here; for now we just log so we don't silently lie about gating.
fn apply_preflight_trust(trust: PreflightTrust, worktree_path: &str) {
    match trust {
        PreflightTrust::Cursor => {
            eprintln!(
                "[agents] preflight_trust=cursor requested for {} — artifact writer not implemented yet (first launch will prompt for trust)",
                worktree_path
            );
        }
        PreflightTrust::Copilot => {
            eprintln!(
                "[agents] preflight_trust=copilot requested for {} — artifact writer not implemented yet (first launch will prompt for trust)",
                worktree_path
            );
        }
        PreflightTrust::Codex => {
            eprintln!(
                "[agents] preflight_trust=codex requested for {} — artifact writer not implemented yet (first launch will prompt for trust)",
                worktree_path
            );
        }
    }
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn agent_list() -> Result<Vec<AgentListing>, String> {
    let reg = load_or_seed_registry()?;
    Ok(reg
        .agents
        .into_iter()
        .map(|a| AgentListing {
            available: binary_on_path(&a.detect_cmd),
            injection_mode: a.prompt_injection_mode,
            id: a.id,
            label: a.label,
        })
        .collect())
}

#[tauri::command]
pub async fn agent_launch(
    app: AppHandle,
    state: State<'_, AppState>,
    req: LaunchAgentRequest,
) -> Result<LaunchAgentResponse, String> {
    let registry = load_or_seed_registry()?;
    let cfg = registry
        .find(&req.agent_id)
        .ok_or_else(|| format!("unknown agent id: {}", req.agent_id))?
        .clone();

    if !binary_on_path(&cfg.detect_cmd) {
        return Err(format!(
            "agent binary not on PATH: {} (install it or edit ~/.config/xnaut/agents.toml)",
            cfg.detect_cmd
        ));
    }

    if let Some(trust) = cfg.preflight_trust {
        apply_preflight_trust(trust, &req.worktree_path);
    }

    let prompt_ref = req.prompt.as_deref();
    let (argv, mut extra_env) = build_launch(&cfg, prompt_ref);
    // Registry-configured env (NautGate base URLs etc.) — applied under any
    // mode-specific vars so the injection-mode logic keeps precedence.
    for (k, v) in &cfg.env {
        extra_env.entry(k.clone()).or_insert_with(|| v.clone());
    }

    // Phase 5: if the hook server is live, give the agent the URL + a freshly-minted
    // bearer token so its hook scripts can POST status updates. We can't know the
    // PTY session_id yet (PTY isn't spawned), so use a placeholder and rewrite the
    // token entry after we have the real id. The window is tiny and the server
    // ignores unknown tokens, so any race is harmless.
    let hook_token_placeholder = if let Some(info) = state.hook_server.lock().await.clone() {
        let placeholder = uuid::Uuid::new_v4().to_string();
        extra_env.insert("XNAUT_HOOK_URL".into(), info.url.clone());
        extra_env.insert("XNAUT_HOOK_TOKEN".into(), placeholder.clone());
        Some((placeholder, info.tokens))
    } else {
        None
    };

    // XNAUT-25: with the hook server live, write the agent's status hooks so it
    // pushes done/permission state instead of relying on the silence heuristic.
    // Best-effort — never fails the launch. Must run before spawn so the agent
    // reads the hooks at startup.
    if hook_token_placeholder.is_some() {
        crate::agent_hook_setup::apply_agent_setup(&cfg.detect_cmd, &req.worktree_path);
    }

    let pty_config = PtyConfig {
        shell: None,
        working_dir: Some(req.worktree_path.clone()),
        env: if extra_env.is_empty() {
            None
        } else {
            Some(extra_env)
        },
        cols: req.cols.unwrap_or(120),
        rows: req.rows.unwrap_or(30),
        command: Some(argv),
    };

    let session_id = pty::create_pty_session(app.clone(), state.clone(), pty_config)
        .await
        .map_err(|e| format!("failed to spawn agent PTY: {e}"))?;

    // Bind the placeholder hook token to the freshly-allocated session id.
    if let Some((token, tokens_map)) = hook_token_placeholder {
        let mut map = tokens_map.lock().await;
        map.insert(token, session_id.clone());
    }

    // Register with the status tracker so Phase 4's overlay can show the dot.
    status::register_agent_session(
        &state.agent_sessions,
        &app,
        &session_id,
        &cfg.id,
        &cfg.label,
    )
    .await;

    // For StdinAfterStart mode, write the prompt after a small delay so the
    // TUI has rendered. This is the simple/dumb version of Orca's
    // `draftPasteReadySignal` — Phase 5 will swap it for hook-driven readiness.
    if cfg.prompt_injection_mode == PromptInjectionMode::StdinAfterStart {
        if let Some(prompt) = req.prompt.clone() {
            let session_id_clone = session_id.clone();
            let pty_sessions = state.pty_sessions.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let sessions = pty_sessions.lock().await;
                if let Some(session) = sessions.get(&session_id_clone) {
                    // Bracketed paste so most TUI agents accept the multi-line prompt as one unit.
                    let payload = format!("\x1b[200~{}\x1b[201~\r", prompt);
                    if let Ok(mut w) = session.writer.lock() {
                        let _ = w.write_all(payload.as_bytes());
                        let _ = w.flush();
                    }
                }
            });
        }
    }

    Ok(LaunchAgentResponse {
        session_id,
        agent_id: cfg.id,
        injection_mode: cfg.prompt_injection_mode,
    })
}

#[tauri::command]
pub fn agent_registry_path() -> Result<String, String> {
    Ok(config_path().to_string_lossy().into_owned())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: PromptInjectionMode, flag: Option<&str>, env: Option<&str>) -> AgentConfig {
        AgentConfig {
            id: "test".into(),
            label: "Test".into(),
            detect_cmd: "test".into(),
            launch_cmd: "test".into(),
            extra_args: vec!["chat".into()],
            expected_process: "test".into(),
            prompt_injection_mode: mode,
            draft_prompt_flag: flag.map(String::from),
            draft_prompt_env_var: env.map(String::from),
            preflight_trust: None,
            env: HashMap::new(),
        }
    }

    #[test]
    fn build_launch_argv_mode_appends_prompt_at_end() {
        let (argv, env) = build_launch(&cfg(PromptInjectionMode::Argv, None, None), Some("hello"));
        assert_eq!(argv, vec!["test", "chat", "hello"]);
        assert!(env.is_empty());
    }

    #[test]
    fn build_launch_flag_prompt_inserts_flag_then_prompt() {
        let (argv, _) = build_launch(
            &cfg(PromptInjectionMode::FlagPrompt, Some("--prefill"), None),
            Some("hi"),
        );
        assert_eq!(argv, vec!["test", "chat", "--prefill", "hi"]);
    }

    #[test]
    fn build_launch_flag_prompt_interactive_appends_dash_i() {
        let (argv, _) = build_launch(
            &cfg(PromptInjectionMode::FlagPromptInteractive, Some("-p"), None),
            Some("hi"),
        );
        assert_eq!(argv, vec!["test", "chat", "-p", "hi", "-i"]);
    }

    #[test]
    fn build_launch_flag_interactive_only_adds_dash_i_no_prompt_in_argv() {
        let (argv, _) = build_launch(
            &cfg(PromptInjectionMode::FlagInteractive, None, None),
            Some("ignored-on-argv"),
        );
        assert_eq!(argv, vec!["test", "chat", "-i"]);
    }

    #[test]
    fn build_launch_stdin_after_start_carries_env_but_not_argv() {
        let (argv, env) = build_launch(
            &cfg(
                PromptInjectionMode::StdinAfterStart,
                None,
                Some("XNAUT_PREFILL"),
            ),
            Some("via-env"),
        );
        assert_eq!(argv, vec!["test", "chat"]);
        assert_eq!(
            env.get("XNAUT_PREFILL").map(|s| s.as_str()),
            Some("via-env")
        );
    }

    #[test]
    fn default_registry_has_known_agents() {
        let r = default_registry();
        let ids: Vec<_> = r.agents.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"claude"));
        assert!(ids.contains(&"codex"));
        assert!(ids.contains(&"gemini"));
        assert!(ids.contains(&"grok"));
        assert!(ids.contains(&"opencode"));
    }
}
