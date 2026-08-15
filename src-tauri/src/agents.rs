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
                // ponytail: no OPENAI_BASE_URL override — codex 0.14x authenticates via
                // ChatGPT login (~/.codex/auth.json), and forcing it at NautGate breaks that
                // auth. Users who want NautGate routing for codex (API-key mode) can add the
                // env back in ~/.config/xnaut/agents.toml.
                env: HashMap::new(),
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

/// Executable search paths available to terminal-launched and Finder-launched
/// builds. macOS GUI apps inherit a deliberately small PATH, so checking only
/// the process environment incorrectly hides CLIs installed by Homebrew,
/// user-level installers, or CMUX.
fn runtime_search_dirs() -> Vec<PathBuf> {
    let mut search_dirs = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(home) = dirs::home_dir() {
        search_dirs.extend([
            home.join(".local/bin"),
            home.join("bin"),
            home.join(".opencode/bin"),
            home.join(".bun/bin"),
            home.join("Library/pnpm"),
            home.join(".cargo/bin"),
            home.join(".lmstudio/bin"),
        ]);
    }
    search_dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/opt/homebrew/sbin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/Applications/cmux.app/Contents/Resources/bin"),
    ]);
    let mut seen = std::collections::HashSet::new();
    search_dirs.retain(|dir| seen.insert(dir.clone()));
    search_dirs
}

fn resolve_binary_in(bin: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let direct = PathBuf::from(bin);
    if direct.components().count() > 1 {
        return direct.is_file().then_some(direct);
    }
    dirs.iter()
        .map(|dir| dir.join(bin))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn resolve_binary(bin: &str) -> Option<PathBuf> {
    resolve_binary_in(bin, &runtime_search_dirs())
}

fn runtime_path() -> Option<String> {
    std::env::join_paths(runtime_search_dirs())
        .ok()
        .map(|value| value.to_string_lossy().into_owned())
}

/// The `open` replacement handed to agents, written once and reused.
///
/// It only intercepts pages: everything else (a folder, a .app, a PDF) falls
/// through to the real `/usr/bin/open`, because taking those over would break
/// ordinary work to fix a browser annoyance.
#[cfg(unix)]
pub(crate) fn browser_shim_dir() -> Option<PathBuf> {
    const SHIM: &str = r#"#!/bin/sh
# Written by xNAUT. Shows pages in the app's own browser; everything else is
# handed to the real /usr/bin/open.
for arg in "$@"; do
  case "$arg" in
    -*) continue ;;
    http://*|https://*|file://*) target="$arg" ;;
    *.html|*.htm|*.svg|*.pdf)
      case "$arg" in /*) target="$arg" ;; *) target="$PWD/$arg" ;; esac ;;
    *) continue ;;
  esac
  if [ -n "$XNAUT_HOOK_URL" ] && command -v curl >/dev/null 2>&1; then
    if curl -sf -m 5 -X POST "${XNAUT_HOOK_URL%/}/v1/open" \
        -H "Authorization: Bearer $XNAUT_HOOK_TOKEN" \
        -H "X-Xnaut-Session: $XNAUT_HOOK_TOKEN" \
        -H 'Content-Type: application/json' \
        --data-raw "{\"target\":\"$target\"}" >/dev/null 2>&1; then
      exit 0
    fi
  fi
done
exec /usr/bin/open "$@"
"#;
    use std::os::unix::fs::PermissionsExt;
    let dir = dirs::config_dir()?.join("xnaut").join("bin");
    std::fs::create_dir_all(&dir).ok()?;
    let script = dir.join("open");
    if std::fs::read_to_string(&script).ok().as_deref() != Some(SHIM) {
        std::fs::write(&script, SHIM).ok()?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).ok()?;
    }
    Some(dir)
}

#[cfg(not(unix))]
pub(crate) fn browser_shim_dir() -> Option<PathBuf> {
    None
}

pub(crate) fn binary_on_path(bin: &str) -> bool {
    resolve_binary(bin).is_some()
}

/// `host:port` out of a base URL, defaulting the port by scheme. Good enough
/// for the localhost endpoints we route to; no URL crate needed.
fn host_port(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?']).next()?;
    if authority.is_empty() {
        return None;
    }
    if authority.contains(':') {
        Some(authority.to_string())
    } else {
        let port = if scheme.eq_ignore_ascii_case("https") { 443 } else { 80 };
        Some(format!("{authority}:{port}"))
    }
}

/// Is something actually listening on this base URL?
fn endpoint_alive(url: &str) -> bool {
    let Some(hp) = host_port(url) else {
        return false;
    };
    use std::net::ToSocketAddrs;
    let Ok(mut addrs) = hp.to_socket_addrs() else {
        return false;
    };
    addrs.any(|addr| std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok())
}

/// Decides what a registry base-URL override should actually become.
///
/// The seeded registry points agents at NautGate (`localhost:8090`). That is
/// right when NautGate is running and actively harmful when it isn't: an agent
/// handed a refused socket retries in silence instead of erroring, so the user
/// sees a launch that hangs forever.
///
/// - Endpoint is live → use it unchanged.
/// - Dead, but the harness switch is on and the user has a working local
///   endpoint configured (LM Studio / Ollama) → point there instead.
/// - Dead with nothing to fall back to → `None`, meaning inject nothing and
///   let the agent use its own default (its subscription).
///
/// ANTHROPIC_BASE_URL falls back too. LM Studio implements Anthropic's
/// `/v1/messages` natively, so Claude Code runs against it unchanged — verified
/// end to end: `ANTHROPIC_BASE_URL=http://localhost:1238 claude -p …` returned a
/// normal assistant turn from a local qwen. Claude Code appends its own `/v1`,
/// so the trailing `/v1` the OpenAI-style setting carries is stripped.
fn resolve_base_url(
    key: &str,
    configured: &str,
    local_endpoint: &str,
    harness_local: bool,
) -> Option<String> {
    if endpoint_alive(configured) {
        return Some(configured.to_string());
    }
    if !harness_local || local_endpoint.is_empty() {
        return None;
    }
    if host_port(local_endpoint) == host_port(configured) || !endpoint_alive(local_endpoint) {
        return None;
    }
    match key {
        "OPENAI_BASE_URL" | "OPENAI_API_BASE" => Some(local_endpoint.to_string()),
        "ANTHROPIC_BASE_URL" => Some(anthropic_base(local_endpoint)),
        _ => None,
    }
}

/// Claude Code builds `<base>/v1/messages`, so hand it the origin only.
pub fn anthropic_base(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

fn configured_nautgate_route(
    key: &str,
    registry_endpoint: &str,
    nautgate: Option<&crate::settings::LlmSettings>,
) -> Option<(String, Option<(String, String)>)> {
    if host_port(registry_endpoint).as_deref() != Some("localhost:8090") {
        return None;
    }
    let route = nautgate?;
    let (endpoint, token_name) = match key {
        "ANTHROPIC_BASE_URL" => (anthropic_base(&route.endpoint), "ANTHROPIC_API_KEY"),
        "OPENAI_BASE_URL" | "OPENAI_API_BASE" => (route.endpoint.clone(), "OPENAI_API_KEY"),
        _ => return None,
    };
    let token = route
        .api_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| (token_name.to_string(), value.clone()));
    Some((endpoint, token))
}

#[derive(Debug, Serialize)]
pub struct AgentListing {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub injection_mode: PromptInjectionMode,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LaunchAgentRequest {
    pub agent_id: String,
    /// Working directory for the spawned process — usually the worktree path.
    pub worktree_path: String,
    /// User's initial prompt. Optional — agent will start without one if absent.
    pub prompt: Option<String>,
    /// Optional model selected by an identity profile. Raw runtime launches
    /// omit it and keep the CLI's configured default.
    #[serde(default)]
    pub model: Option<String>,
    /// Agent Space uses a non-interactive JSONL contract. The CLI still runs
    /// in a PTY for optional Terminal inspection, but its screen is never used
    /// as the conversation response.
    #[serde(default)]
    pub conversation_mode: bool,
    /// Provider-native conversation id used to resume context across turns.
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub resume: bool,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    /// Least-privilege policy for this run. Absent means today's defaults.
    #[serde(default)]
    pub policy: Option<crate::policy::AgentPolicy>,
    /// The launching identity's capability list. Only the `plugin:` entries are
    /// read here — which MCP servers THIS agent was given.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LaunchAgentResponse {
    pub session_id: String,
    pub agent_id: String,
    pub injection_mode: PromptInjectionMode,
    pub conversation_id: Option<String>,
    /// Clean stdout of a zellij-backed run. The PTY now hosts zellij, whose
    /// chrome would corrupt a JSON frame, so the conversation reads this file
    /// instead. Absent when the run went straight to a PTY.
    #[serde(default)]
    pub output_path: Option<String>,
}

/// Optional identity attached to a runtime launch. Raw runtime launches keep
/// using the runtime id; profile launches use the stable profile handle so
/// status events and responses identify the agent rather than its harness.
pub(crate) struct AgentLaunchIdentity {
    pub id: String,
    pub label: String,
}

/// Builds (argv, extra_env) for an agent given the injection mode.
fn model_flag(runtime_id: &str) -> Option<&'static str> {
    match runtime_id {
        // Verified locally for Claude Code, Codex, and Pi. Gemini and OpenCode
        // expose the same documented long flag; Grok's configured CLI follows
        // the same contract when present.
        "claude" | "codex" | "gemini" | "grok" | "opencode" | "pi" => Some("--model"),
        _ => None,
    }
}

fn build_launch(
    cfg: &AgentConfig,
    prompt: Option<&str>,
    model: Option<&str>,
) -> (Vec<String>, HashMap<String, String>) {
    let mut argv: Vec<String> = vec![cfg.launch_cmd.clone()];
    argv.extend(cfg.extra_args.iter().cloned());
    let mut env = HashMap::new();

    if let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) {
        if let Some(flag) = model_flag(&cfg.id) {
            argv.push(flag.to_string());
            argv.push(model.to_string());
        }
        // Metadata for hooks and custom wrappers. Claude Code also honors
        // ANTHROPIC_MODEL, which keeps the choice explicit through NautGate.
        env.insert("XNAUT_AGENT_MODEL".into(), model.to_string());
        if cfg.id == "claude" {
            env.insert("ANTHROPIC_MODEL".into(), model.to_string());
        }
    }

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


/// Single-quote escaping for the run script. The 2026-08-09 handover records
/// what happens without it: bare words split, double quotes ended the KDL
/// string, and single quotes made printf repeat its format once per word —
/// seventeen empty panes. The payload goes in a file and the layout runs two
/// plain words.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Where a zellij-backed run keeps its script, its clean output and its errors.
fn run_dir() -> Result<std::path::PathBuf, String> {
    let dir = dirs::home_dir()
        .ok_or_else(|| "could not resolve the home directory".to_string())?
        .join(".config")
        .join("xnaut")
        .join("agent-runs");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create the run directory: {e}"))?;
    Ok(dir)
}

/// Keep the run directory from growing without bound.
///
/// Each run now writes its OWN script, layout and output (a shared name made
/// the second message attach to the first run's session), and a single
/// conversation can leave megabytes behind. Newest 60 files stay, which is
/// several days of real use and still enough to read yesterday's failure.
fn prune_run_dir(dir: &std::path::Path) {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let modified = entry.metadata().ok()?.modified().ok()?;
                path.is_file().then_some((modified, path))
            })
            .collect(),
        Err(_) => return,
    };
    if files.len() <= 60 {
        return;
    }
    files.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, path) in files.into_iter().skip(60) {
        let _ = std::fs::remove_file(path);
    }
}

/// Prepare a zellij-backed run (XNAUT-66).
///
/// Two problems solved at once. A raw PTY dies with the app, so closing a tab
/// killed the agent; and zellij's own chrome corrupts the JSON stream the
/// conversation parses, which is why the direct PTY was kept in the first
/// place. So the script tees the CLI's stdout to a FILE: zellij owns the
/// session and survives, while the chat reads clean bytes that no TUI ever
/// touched. stderr goes to its own file so a warning cannot corrupt a frame.
///
/// Returns (session name, layout path, output path).
fn prepare_zellij_run(
    session: &str,
    cwd: &str,
    argv: &[String],
    env: &std::collections::HashMap<String, String>,
) -> Result<(String, String, String), String> {
    let dir = run_dir()?;
    prune_run_dir(&dir);
    let name = crate::zellij::session_name(session);
    let script = dir.join(format!("{name}.sh"));
    let out = dir.join(format!("{name}.jsonl"));
    let err = dir.join(format!("{name}.err"));

    let mut lines = vec!["#!/bin/sh".to_string()];
    for (key, value) in env {
        lines.push(format!("export {key}={}", shell_quote(value)));
    }
    lines.push(format!("cd {} || exit 1", shell_quote(cwd)));
    let command = argv
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ");
    // The pipeline, not exec: tee has to outlive the CLI to flush the tail.
    lines.push(format!(
        "{command} 2>>{} | tee -a {}",
        shell_quote(&err.to_string_lossy()),
        shell_quote(&out.to_string_lossy())
    ));
    // Let the session END when the run ends. Holding the pane open with a
    // `read` kept the session alive forever, and `zellij launch_command`
    // ATTACHES to an existing name instead of starting a layout — so the next
    // message re-entered the finished session and never ran. The evidence the
    // hold was protecting lives in the .jsonl and .err files either way.
    // The marker goes into the OUTPUT FILE, not the pane: agent_run_output
    // tails that file and stops on this string. Printed to the pane it was
    // invisible to the reader, which then polled for the full 30-minute
    // deadline on every run that had already finished.
    lines.push(format!(
        "printf '\\n[xnaut] run finished\\n' >>{}",
        shell_quote(&out.to_string_lossy())
    ));

    std::fs::write(&script, lines.join("\n") + "\n")
        .map_err(|e| format!("could not write the run script: {e}"))?;
    let layout = crate::zellij::write_layout(
        &name,
        cwd,
        &format!("sh {}", shell_quote(&script.to_string_lossy())),
    )?;
    Ok((
        name,
        layout.to_string_lossy().into_owned(),
        out.to_string_lossy().into_owned(),
    ))
}

/// Build the machine-readable Agent Space command. Only runtimes with a
/// verified JSONL/non-interactive contract belong here. An unsupported runtime
/// must fail clearly instead of leaking its TUI into the chat surface.
fn build_conversation_launch(
    cfg: &AgentConfig,
    prompt: &str,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    conversation_id: Option<&str>,
    resume: bool,
    policy: Option<&crate::policy::AgentPolicy>,
    capabilities: &[String],
) -> Result<(Vec<String>, HashMap<String, String>, Option<String>), String> {
    let model = model.map(str::trim).filter(|value| !value.is_empty());
    let effort = reasoning_effort
        .map(str::trim)
        .filter(|value| matches!(*value, "low" | "medium" | "high" | "xhigh"));
    let mut env = HashMap::new();
    if let Some(model) = model {
        env.insert("XNAUT_AGENT_MODEL".into(), model.to_string());
        if cfg.id == "claude" {
            env.insert("ANTHROPIC_MODEL".into(), model.to_string());
        }
    }

    // Enabled plugins reach the run the same way for every runtime that has a
    // documented switch for it. Assembled once, here, so a plugin the owner
    // switched on cannot be present for claude and missing for codex.
    let plugins = crate::plugins::active_for(&capabilities);
    let plugin_flags = crate::plugins::launch_flags(&cfg.id, &plugins);

    match cfg.id.as_str() {
        "claude" => {
            let id = conversation_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let mut argv = vec![cfg.launch_cmd.clone()];
            argv.extend(cfg.extra_args.iter().cloned());
            argv.extend(["--print".into(), "--output-format".into(), "stream-json".into(), "--verbose".into()]);
            if let Some(policy) = policy {
                argv.extend(crate::policy::launch_flags(&cfg.id, policy));
            }
            argv.extend(plugin_flags.iter().cloned());
            if let Some(model) = model {
                argv.extend(["--model".into(), model.to_string()]);
            }
            if let Some(effort) = effort {
                argv.extend(["--effort".into(), effort.to_string()]);
            }
            if resume {
                argv.extend(["--resume".into(), id.clone()]);
            } else {
                argv.extend(["--session-id".into(), id.clone()]);
            }
            argv.push(prompt.to_string());
            Ok((argv, env, Some(id)))
        }
        "codex" => {
            let mut argv = vec![cfg.launch_cmd.clone(), "exec".into()];
            if resume {
                let id = conversation_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Codex conversation id is missing; start a new thread".to_string())?;
                argv.push("resume".into());
                argv.push("--json".into());
                argv.extend(crate::policy::launch_flags(
                    &cfg.id,
                    policy.unwrap_or(&crate::policy::AgentPolicy::default()),
                ));
                argv.extend(plugin_flags.iter().cloned());
                if let Some(model) = model {
                    argv.extend(["--model".into(), model.to_string()]);
                }
                argv.extend([id.to_string(), prompt.to_string()]);
                Ok((argv, env, Some(id.to_string())))
            } else {
                argv.extend(["--json".into(), "--color".into(), "never".into()]);
                argv.extend(crate::policy::launch_flags(
                    &cfg.id,
                    policy.unwrap_or(&crate::policy::AgentPolicy::default()),
                ));
                argv.extend(plugin_flags.iter().cloned());
                if let Some(model) = model {
                    argv.extend(["--model".into(), model.to_string()]);
                }
                argv.push(prompt.to_string());
                Ok((argv, env, None))
            }
        }
        "gemini" => {
            let mut argv = vec![
                cfg.launch_cmd.clone(),
                "--output-format".into(),
                "stream-json".into(),
                "--approval-mode".into(),
                "yolo".into(),
            ];
            if let Some(model) = model {
                argv.extend(["--model".into(), model.to_string()]);
            }
            if resume {
                let id = conversation_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        "Gemini conversation id is missing; start a new thread".to_string()
                    })?;
                argv.extend(["--resume".into(), id.to_string()]);
                argv.extend(["--prompt".into(), prompt.to_string()]);
                Ok((argv, env, Some(id.to_string())))
            } else {
                argv.extend(["--prompt".into(), prompt.to_string()]);
                Ok((argv, env, None))
            }
        }
        _ => Err(format!(
            "{} does not yet expose a verified structured conversation mode",
            cfg.label
        )),
    }
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

fn write_claude_project_trust(
    config_path: &std::path::Path,
    project_path: &str,
) -> Result<(), String> {
    let body = std::fs::read_to_string(config_path)
        .map_err(|error| format!("failed to read Claude settings: {error}"))?;
    let mut root: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("Claude settings are not valid JSON: {error}"))?;
    let projects = root
        .as_object_mut()
        .ok_or_else(|| "Claude settings root is not an object".to_string())?
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}));
    let projects = projects
        .as_object_mut()
        .ok_or_else(|| "Claude projects settings are not an object".to_string())?;
    let project = projects
        .entry(project_path.to_string())
        .or_insert_with(|| serde_json::json!({}));
    project
        .as_object_mut()
        .ok_or_else(|| "Claude project settings are not an object".to_string())?
        .insert(
            "hasTrustDialogAccepted".to_string(),
            serde_json::Value::Bool(true),
        );

    let rendered = serde_json::to_vec_pretty(&root)
        .map_err(|error| format!("failed to encode Claude settings: {error}"))?;
    let temporary = config_path.with_extension(format!("json.xnaut-{}", std::process::id()));
    std::fs::write(&temporary, rendered)
        .map_err(|error| format!("failed to stage Claude settings: {error}"))?;
    if let Ok(metadata) = std::fs::metadata(config_path) {
        let _ = std::fs::set_permissions(&temporary, metadata.permissions());
    }
    std::fs::rename(&temporary, config_path)
        .map_err(|error| format!("failed to save Claude project trust: {error}"))
}

/// Selecting a project in Agent Space is the user's explicit trust decision.
/// Record that decision before Claude starts so its TUI cannot consume the
/// prompt while waiting at the otherwise invisible first-run trust screen.
fn accept_claude_project_trust(worktree_path: &str) -> Result<(), String> {
    let config = dirs::home_dir()
        .ok_or_else(|| "home directory is unavailable".to_string())?
        .join(".claude.json");
    if !config.is_file() {
        return Ok(());
    }
    write_claude_project_trust(&config, worktree_path)
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
    launch_agent_with_env(app, state, req, HashMap::new(), None).await
}

/// Shared runtime launch path used by both the raw runtime launcher and the
/// identity profile launcher. Profile-specific environment values are applied
/// after registry routing so a persisted identity cannot be shadowed by a
/// stale value in `agents.toml`.
pub(crate) async fn launch_agent_with_env(
    app: AppHandle,
    state: State<'_, AppState>,
    req: LaunchAgentRequest,
    identity_env: HashMap<String, String>,
    launch_identity: Option<AgentLaunchIdentity>,
) -> Result<LaunchAgentResponse, String> {
    let registry = load_or_seed_registry()?;
    let cfg = registry
        .find(&req.agent_id)
        .ok_or_else(|| format!("unknown agent id: {}", req.agent_id))?
        .clone();

    let detected_binary = resolve_binary(&cfg.detect_cmd).ok_or_else(|| {
        format!(
            "agent binary not found: {} (install it or edit {})",
            cfg.detect_cmd,
            config_path().display()
        )
    })?;
    let launch_binary = resolve_binary(&cfg.launch_cmd)
        .or_else(|| (cfg.launch_cmd == cfg.detect_cmd).then_some(detected_binary))
        .ok_or_else(|| {
            format!(
                "agent launch binary not found: {} (edit {})",
                cfg.launch_cmd,
                config_path().display()
            )
        })?;

    if !launch_binary.is_file() {
        return Err(format!(
            "agent launch binary is not a file: {}",
            launch_binary.display()
        ));
    }

    if let Some(trust) = cfg.preflight_trust {
        apply_preflight_trust(trust, &req.worktree_path);
    }
    if cfg.detect_cmd == "claude" {
        accept_claude_project_trust(&req.worktree_path)?;
    }

    let prompt_ref = req.prompt.as_deref();
    let (mut argv, mut extra_env, conversation_id) = if req.conversation_mode {
        let prompt = prompt_ref.ok_or_else(|| "Conversation prompt is required".to_string())?;
        build_conversation_launch(
            &cfg,
            prompt,
            req.model.as_deref(),
            req.reasoning_effort.as_deref(),
            req.conversation_id.as_deref(),
            req.resume,
            req.policy.as_ref(),
            &req.capabilities,
        )?
    } else {
        let (argv, env) = build_launch(&cfg, prompt_ref, req.model.as_deref());
        (argv, env, None)
    };
    argv[0] = launch_binary.to_string_lossy().into_owned();
    if let Some(path) = runtime_path() {
        extra_env.insert("PATH".into(), path);
    }
    // An agent that builds a page runs `open` on it out of habit, and the page
    // lands in a system browser window behind the app. Put our own `open` first
    // on its PATH so the page arrives in an xNAUT browser tab instead.
    if let Some(shim) = browser_shim_dir() {
        let base = extra_env
            .get("PATH")
            .cloned()
            .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
        extra_env.insert("PATH".into(), format!("{}:{}", shim.display(), base));
        // Anything that honours $BROWSER (gh, many CLIs) gets it for free.
        extra_env.insert(
            "BROWSER".into(),
            shim.join("open").to_string_lossy().into_owned(),
        );
    }
    // Registry-configured env (NautGate base URLs etc.) — applied under any
    // mode-specific vars so the injection-mode logic keeps precedence.
    // Routed through resolve_base_url first: the seeded registry points at
    // NautGate, and on a machine without it a dead base URL makes the agent
    // hang silently rather than fail (claude retries a refused socket).
    let (local_endpoint, local_model, harness_local, nautgate) = {
        let s = state.settings.lock().await;
        (
            s.llm.endpoint.clone(),
            s.llm.model.clone(),
            s.llm.harness_local,
            crate::chat::provider_llm(&s, "nautgate"),
        )
    };
    let mut routed_local = false;
    for (k, v) in &cfg.env {
        let configured_route = configured_nautgate_route(k, v, nautgate.as_ref());
        let resolved = configured_route
            .as_ref()
            .map(|route| route.0.clone())
            .or_else(|| resolve_base_url(k, v, &local_endpoint, harness_local));
        if let Some(resolved) = resolved {
            routed_local |= resolved != *v;
            extra_env.entry(k.clone()).or_insert(resolved);
        }
        if let Some((_, Some((token_name, token)))) = configured_route {
            extra_env.insert(token_name, token);
        }
    }
    // Pointing Claude Code at a local server is not enough on its own: it still
    // needs an auth source (any non-empty key — the local server ignores it) and
    // a model name that server actually serves, or it asks for a claude-* model
    // nothing there can answer. Both verified against LM Studio.
    if routed_local && extra_env.contains_key("ANTHROPIC_BASE_URL") {
        extra_env
            .entry("ANTHROPIC_API_KEY".into())
            .or_insert_with(|| "local".into());
        if !local_model.is_empty() {
            extra_env
                .entry("ANTHROPIC_MODEL".into())
                .or_insert_with(|| local_model.clone());
        }
    }

    extra_env.extend(identity_env);

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

    // XNAUT-66: an Agent Space run is backed by a zellij session so it outlives
    // the app. Closing the tab used to kill the agent, because the PTY owned the
    // process. Now the PTY is only a viewport onto a session that keeps running,
    // and reopening reattaches (launch_command attaches when the name exists).
    //
    // Falls back to the direct PTY when zellij is missing, or for the
    // non-conversation path, which has its own persistence story via loom_run.
    let zellij_run = if req.conversation_mode && crate::zellij::is_installed() {
        let identity = launch_identity
            .as_ref()
            .map(|identity| identity.id.clone())
            .unwrap_or_else(|| cfg.id.clone());
        // A per-run suffix. Reusing one name per agent meant the SECOND
        // message attached to the first run's finished session instead of
        // starting anything, and the chat replayed that run's output file
        // from the top — the "mixed up" thread of 2026-08-15.
        let run_id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        match prepare_zellij_run(
            &format!("xnaut-{identity}-{run_id}"),
            &req.worktree_path,
            &argv,
            &extra_env,
        ) {
            Ok(prepared) => Some(prepared),
            Err(error) => {
                // A failed setup must not cost the run: fall back and say why.
                eprintln!("[agents] zellij-backed run unavailable ({error}); using a direct PTY");
                None
            }
        }
    } else {
        None
    };

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
        // With zellij the layout runs the agent; the PTY hosts zellij itself.
        command: match &zellij_run {
            Some(_) => None,
            None => Some(argv),
        },
        session_name: zellij_run.as_ref().map(|(name, _, _)| name.clone()),
        session_layout: zellij_run.as_ref().map(|(_, layout, _)| layout.clone()),
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
    let (launched_agent_id, launched_agent_label) = launch_identity
        .map(|identity| (identity.id, identity.label))
        .unwrap_or_else(|| (cfg.id.clone(), cfg.label.clone()));
    status::register_agent_session(
        &state.agent_sessions,
        &app,
        &session_id,
        &launched_agent_id,
        &launched_agent_label,
    )
    .await;

    // For StdinAfterStart mode, write the prompt after a small delay so the
    // TUI has rendered. This is the simple/dumb version of Orca's
    // `draftPasteReadySignal` — Phase 5 will swap it for hook-driven readiness.
    if !req.conversation_mode && cfg.prompt_injection_mode == PromptInjectionMode::StdinAfterStart {
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
        agent_id: launched_agent_id,
        injection_mode: cfg.prompt_injection_mode,
        conversation_id,
        output_path: zellij_run.map(|(_, _, out)| out),
    })
}


#[derive(Debug, Serialize)]
pub struct RunOutput {
    pub text: String,
    pub next_offset: u64,
    pub finished: bool,
}

/// Read new bytes from a zellij-backed run's output file.
///
/// The conversation polls this instead of the PTY stream: zellij owns the PTY
/// now and its chrome would corrupt a JSON frame. Offsets make it a tail
/// rather than a re-read, so a long run does not re-parse itself every tick,
/// and the file outlives the app — reopening a thread can replay what was
/// missed instead of showing an empty pane.
#[tauri::command]
pub fn agent_run_output(path: String, offset: u64) -> Result<RunOutput, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(&path) {
        Ok(file) => file,
        // Not an error: the script may not have flushed its first line yet.
        Err(_) => {
            return Ok(RunOutput {
                text: String::new(),
                next_offset: offset,
                finished: false,
            })
        }
    };
    let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    // A truncated or rotated file must not leave the reader stuck past the end.
    let start = if offset > len { 0 } else { offset };
    file.seek(SeekFrom::Start(start))
        .map_err(|e| format!("could not seek the run output: {e}"))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| format!("could not read the run output: {e}"))?;
    let next_offset = start + buffer.len() as u64;
    // The run script writes this marker when the CLI exits, so the reader can
    // stop polling without guessing from silence.
    let text = String::from_utf8_lossy(&buffer).to_string();
    let finished = text.contains("[xnaut] run finished");
    Ok(RunOutput {
        text,
        next_offset,
        finished,
    })
}


/// Reattach to an agent's zellij session (XNAUT-66).
///
/// The run survives the app, but the PTY that was watching it does not. On
/// reopening a thread the stored session id points at a dead viewport, which
/// is what "it did not attach" looks like. This spawns a fresh PTY that runs
/// `zellij attach <name>`, so the live session comes back with its scrollback
/// instead of a blank pane.
#[tauri::command]
pub async fn agent_session_attach(
    app: AppHandle,
    state: State<'_, AppState>,
    handle: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<Option<String>, String> {
    // Only attach to something that is actually running: creating the session
    // here would start a bare shell and look like a working agent.
    let probe = handle.clone();
    let live = tokio::task::spawn_blocking(move || live_sessions_for(&probe))
        .await
        .unwrap_or_default();
    let Some(name) = live.into_iter().next_back() else {
        return Ok(None);
    };
    let pty_config = PtyConfig {
        shell: None,
        working_dir: None,
        env: None,
        cols: cols.unwrap_or(120),
        rows: rows.unwrap_or(30),
        command: None,
        session_name: Some(name),
        // No layout: the session already knows what it is running.
        session_layout: None,
    };
    let session_id = pty::create_pty_session(app, state, pty_config)
        .await
        .map_err(|e| format!("failed to attach to the agent session: {e}"))?;
    Ok(Some(session_id))
}

/// The live zellij sessions belonging to one agent. Names carry a per-run
/// suffix, so this is a prefix match; the bare name is matched too for runs
/// started before the suffix existed.
fn live_sessions_for(handle: &str) -> Vec<String> {
    let base = crate::zellij::session_name(&format!("xnaut-{}", handle.trim()));
    let prefix = format!("{base}-");
    crate::zellij::list_live_sessions()
        .into_iter()
        .filter(|live| live == &base || live.starts_with(&prefix))
        .collect()
}

/// Whether an agent has a live session to attach to.
///
/// Async + `spawn_blocking` on purpose: `list_live_sessions` spawns `zellij`
/// and waits for it. A *sync* Tauri command runs on the main thread, so that
/// wait froze the webview — which showed up as the right pane rendering
/// nothing at all. Same failure class as the keystroke stall in b528872.
#[tauri::command]
pub async fn agent_session_alive(handle: String) -> bool {
    tokio::task::spawn_blocking(move || !live_sessions_for(&handle).is_empty())
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub fn agent_registry_path() -> Result<String, String> {
    Ok(config_path().to_string_lossy().into_owned())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_script_tees_to_the_file_the_reader_tails_and_marks_its_end() {
        // Both halves have been wrong in production: the marker was printed to
        // the zellij pane (so a finished run polled for 30 minutes), and the
        // session was held open by a `read` (so the NEXT message attached to
        // it and never ran). Assert the script, not the intention.
        let (name, layout, out) = prepare_zellij_run(
            "xnaut-selftest-script",
            "/tmp",
            &["claude".to_string(), "--print".to_string(), "hello world".to_string()],
            &std::collections::HashMap::from([("TOKEN".to_string(), "s3cret".to_string())]),
        )
        .expect("prepare");
        let script = run_dir().unwrap().join(format!("{name}.sh"));
        let text = std::fs::read_to_string(&script).expect("script");
        assert!(text.contains(&format!("| tee -a '{out}'")), "stdout is not teed: {text}");
        assert!(
            text.contains(&format!("[xnaut] run finished\\n' >>'{out}'")),
            "the finish marker never reaches the tailed file: {text}"
        );
        assert!(!text.contains("read _"), "a held-open session blocks the next run");
        assert!(text.contains("'hello world'"), "arguments must survive quoting");
        let _ = std::fs::remove_file(&script);
        let _ = std::fs::remove_file(&layout);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn executable_resolution_uses_supplied_fallback_directories() {
        let root = std::env::temp_dir().join(format!(
            "xnaut-agent-bin-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let bin_dir = root.join(".local/bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let executable = bin_dir.join("codex");
        std::fs::write(&executable, "test").unwrap();

        assert_eq!(
            resolve_binary_in("codex", std::slice::from_ref(&bin_dir)),
            Some(executable.clone())
        );
        assert_eq!(
            resolve_binary_in(executable.to_string_lossy().as_ref(), &[]),
            Some(executable)
        );
        assert_eq!(resolve_binary_in("missing-agent", &[bin_dir]), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_search_includes_common_macos_gui_fallbacks() {
        let search_dirs = runtime_search_dirs();
        assert!(search_dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(search_dirs.contains(&PathBuf::from(
            "/Applications/cmux.app/Contents/Resources/bin"
        )));
        if let Some(home) = dirs::home_dir() {
            assert!(search_dirs.contains(&home.join(".local/bin")));
        }
    }

    #[test]
    fn a_dead_base_url_is_never_injected() {
        // Port 1 is reserved and never listening: the NautGate-less machine.
        let dead = "http://localhost:1";
        assert_eq!(resolve_base_url("ANTHROPIC_BASE_URL", dead, "", true), None);
        assert_eq!(resolve_base_url("OPENAI_BASE_URL", dead, "", true), None);
    }

    #[test]
    fn the_switch_is_off_by_default_so_subscriptions_are_left_alone() {
        let live = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let local = format!("http://{}/v1", live.local_addr().unwrap());
        // harness_local = false: never redirect an agent at the local model.
        assert_eq!(
            resolve_base_url("OPENAI_BASE_URL", "http://localhost:1", &local, false),
            None
        );
    }

    #[test]
    fn a_dead_nautgate_falls_back_to_the_local_endpoint() {
        let live = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = live.local_addr().unwrap();
        let local = format!("http://{addr}/v1");
        let dead = "http://localhost:1";

        // OpenAI-shaped vars take the endpoint as configured.
        assert_eq!(
            resolve_base_url("OPENAI_BASE_URL", dead, &local, true),
            Some(local.clone())
        );
        // Claude Code appends its own /v1, so it gets the origin.
        assert_eq!(
            resolve_base_url("ANTHROPIC_BASE_URL", dead, &local, true),
            Some(format!("http://{addr}"))
        );
        // A live configured endpoint is left exactly as configured.
        assert_eq!(
            resolve_base_url("OPENAI_BASE_URL", &local, "", true),
            Some(local)
        );
    }

    #[test]
    fn anthropic_base_drops_the_openai_v1_suffix() {
        assert_eq!(anthropic_base("http://localhost:1238/v1"), "http://localhost:1238");
        assert_eq!(anthropic_base("http://localhost:1238/v1/"), "http://localhost:1238");
        assert_eq!(anthropic_base("http://localhost:1238"), "http://localhost:1238");
    }

    #[test]
    fn configured_nautgate_route_supplies_the_settings_token_to_agent_clis() {
        let route = crate::settings::LlmSettings {
            provider: "nautgate".into(),
            endpoint: "http://localhost:8090/v1".into(),
            model: "gpt-5.6-sol".into(),
            api_key: Some("settings-token".into()),
            system_prompt: None,
            harness_local: false,
        };

        let claude = configured_nautgate_route(
            "ANTHROPIC_BASE_URL",
            "http://localhost:8090",
            Some(&route),
        )
        .unwrap();
        assert_eq!(claude.0, "http://localhost:8090");
        assert_eq!(
            claude.1,
            Some(("ANTHROPIC_API_KEY".into(), "settings-token".into()))
        );

        let openai = configured_nautgate_route(
            "OPENAI_BASE_URL",
            "http://localhost:8090/v1",
            Some(&route),
        )
        .unwrap();
        assert_eq!(openai.0, "http://localhost:8090/v1");
        assert_eq!(
            openai.1,
            Some(("OPENAI_API_KEY".into(), "settings-token".into()))
        );
    }

    #[test]
    fn an_existing_settings_file_without_the_new_field_still_loads() {
        // Every installed copy predates harness_local; serde(default) must cover
        // it or the app fails to read its own settings on upgrade.
        let old = r#"{"provider":"","endpoint":"http://localhost:8090/v1",
            "model":"claude-sonnet-4-6","api_key":null,"system_prompt":null}"#;
        let parsed: crate::settings::LlmSettings = serde_json::from_str(old).unwrap();
        assert!(!parsed.harness_local, "must default to off");
        assert_eq!(parsed.endpoint, "http://localhost:8090/v1");
    }

    #[test]
    fn host_port_defaults_the_port_by_scheme() {
        assert_eq!(host_port("http://localhost:8090"), Some("localhost:8090".into()));
        assert_eq!(host_port("http://localhost:8090/v1"), Some("localhost:8090".into()));
        assert_eq!(host_port("https://api.anthropic.com"), Some("api.anthropic.com:443".into()));
        assert_eq!(host_port("http://example.com/x"), Some("example.com:80".into()));
        assert_eq!(host_port("not-a-url"), None);
    }

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
        let (argv, env) = build_launch(&cfg(PromptInjectionMode::Argv, None, None), Some("hello"), None);
        assert_eq!(argv, vec!["test", "chat", "hello"]);
        assert!(env.is_empty());
    }

    #[test]
    fn build_launch_flag_prompt_inserts_flag_then_prompt() {
        let (argv, _) = build_launch(
            &cfg(PromptInjectionMode::FlagPrompt, Some("--prefill"), None),
            Some("hi"),
            None,
        );
        assert_eq!(argv, vec!["test", "chat", "--prefill", "hi"]);
    }

    #[test]
    fn build_launch_flag_prompt_interactive_appends_dash_i() {
        let (argv, _) = build_launch(
            &cfg(PromptInjectionMode::FlagPromptInteractive, Some("-p"), None),
            Some("hi"),
            None,
        );
        assert_eq!(argv, vec!["test", "chat", "-p", "hi", "-i"]);
    }

    #[test]
    fn build_launch_flag_interactive_only_adds_dash_i_no_prompt_in_argv() {
        let (argv, _) = build_launch(
            &cfg(PromptInjectionMode::FlagInteractive, None, None),
            Some("ignored-on-argv"),
            None,
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
            None,
        );
        assert_eq!(argv, vec!["test", "chat"]);
        assert_eq!(
            env.get("XNAUT_PREFILL").map(|s| s.as_str()),
            Some("via-env")
        );
    }

    #[test]
    fn selected_model_is_forwarded_before_the_prompt() {
        let mut runtime = cfg(PromptInjectionMode::FlagPrompt, Some("--prefill"), None);
        runtime.id = "claude".into();
        runtime.launch_cmd = "claude".into();
        runtime.extra_args = vec!["--dangerously-skip-permissions".into()];
        let (argv, env) = build_launch(&runtime, Some("hello"), Some("claude-opus-5"));
        assert_eq!(
            argv,
            vec![
                "claude",
                "--dangerously-skip-permissions",
                "--model",
                "claude-opus-5",
                "--prefill",
                "hello"
            ]
        );
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("claude-opus-5"));
        assert_eq!(env.get("XNAUT_AGENT_MODEL").map(String::as_str), Some("claude-opus-5"));
    }

    #[test]
    fn claude_agent_space_uses_jsonl_print_mode_and_resumable_context() {
        let mut runtime = cfg(PromptInjectionMode::FlagPrompt, Some("--prefill"), None);
        runtime.id = "claude".into();
        runtime.label = "Claude Code".into();
        runtime.launch_cmd = "claude".into();
        runtime.extra_args = vec!["--dangerously-skip-permissions".into()];
        let (argv, _, id) = build_conversation_launch(
            &runtime,
            "Build it",
            Some("claude-opus-5"),
            Some("high"),
            Some("7f90c2b1-2fa5-4a76-a0ce-aa60e235e41d"),
            true,
            None,
            &[],
        )
        .unwrap();
        assert_eq!(
            argv,
            vec![
                "claude", "--dangerously-skip-permissions", "--print", "--output-format",
                "stream-json", "--verbose", "--model", "claude-opus-5", "--effort", "high",
                "--resume", "7f90c2b1-2fa5-4a76-a0ce-aa60e235e41d", "Build it"
            ]
        );
        assert_eq!(id.as_deref(), Some("7f90c2b1-2fa5-4a76-a0ce-aa60e235e41d"));
        assert!(!argv.iter().any(|value| value == "--prefill"));
    }

    #[test]
    fn codex_agent_space_uses_exec_json_in_the_selected_workspace() {
        let mut runtime = cfg(PromptInjectionMode::Argv, None, None);
        runtime.id = "codex".into();
        runtime.label = "Codex".into();
        runtime.launch_cmd = "codex".into();
        runtime.extra_args.clear();
        let (argv, _, id) = build_conversation_launch(
            &runtime,
            "Run tests",
            Some("gpt-5.6-codex"),
            Some("high"),
            None,
            false,
            None,
            &[],
        )
        .unwrap();
        assert_eq!(
            argv,
            vec![
                // --approve-for-me IS workspace-write with approvals handled;
                // pairing it with --sandbox is a hard error in codex exec.
                "codex", "exec", "--json", "--color", "never", "--approve-for-me",
                "--model", "gpt-5.6-codex", "Run tests"
            ]
        );
        assert_eq!(id, None);
    }

    #[test]
    fn gemini_agent_space_uses_headless_json_and_resumable_context() {
        let mut runtime = cfg(PromptInjectionMode::FlagPromptInteractive, Some("-p"), None);
        runtime.id = "gemini".into();
        runtime.label = "Gemini".into();
        runtime.launch_cmd = "gemini".into();
        runtime.extra_args.clear();
        let (argv, _, id) = build_conversation_launch(
            &runtime,
            "Continue",
            Some("gemini-3-pro"),
            None,
            Some("70272ea8-4083-4590-ba02-242d377fa77b"),
            true,
            None,
            &[],
        )
        .unwrap();
        assert_eq!(
            argv,
            vec![
                "gemini",
                "--output-format",
                "stream-json",
                "--approval-mode",
                "yolo",
                "--model",
                "gemini-3-pro",
                "--resume",
                "70272ea8-4083-4590-ba02-242d377fa77b",
                "--prompt",
                "Continue"
            ]
        );
        assert_eq!(id.as_deref(), Some("70272ea8-4083-4590-ba02-242d377fa77b"));
        assert!(!argv.iter().any(|value| value == "-i"));
    }

    #[test]
    fn unverified_tui_runtime_is_rejected_from_the_conversation_surface() {
        let runtime = cfg(PromptInjectionMode::FlagInteractive, None, None);
        let error = build_conversation_launch(&runtime, "hello", None, None, None, false, None, &[])
            .unwrap_err();
        assert!(error.contains("structured conversation mode"));
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

    #[test]
    fn claude_project_trust_preserves_settings_and_accepts_only_the_selected_path() {
        let root = std::env::temp_dir().join(format!(
            "xnaut-claude-trust-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let config = root.join(".claude.json");
        std::fs::write(
            &config,
            r#"{"hasCompletedOnboarding":true,"projects":{"/existing":{"allowedTools":["Read"]}}}"#,
        )
        .unwrap();

        write_claude_project_trust(&config, "/work/honey-site").unwrap();

        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert_eq!(saved["hasCompletedOnboarding"], true);
        assert_eq!(saved["projects"]["/existing"]["allowedTools"][0], "Read");
        assert_eq!(
            saved["projects"]["/work/honey-site"]["hasTrustDialogAccepted"],
            true
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
