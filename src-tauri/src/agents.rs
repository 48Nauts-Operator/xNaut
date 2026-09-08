// Agent registry + launch dispatch. Ports the TUI_AGENT_CONFIG shape from
// Orca (src/shared/tui-agent-config.ts) but stores the registry as user-editable
// TOML at `agents.toml` in the app config dir (see `config_path`) so users can
// add agents without rebuilding. That is `~/Library/Application Support/xnaut`
// on macOS, NOT `~/.config/xnaut`.

use crate::pty::{self, PtyConfig};
use crate::state::AppState;
use crate::status;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
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
    Gemini,
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
    /// Marks how far this file has been reconciled against the built-in seed.
    /// Absent (0) means the file predates reconciliation and still needs the
    /// one-time heal in [`reconcile`]. Serialized first so it lands above the
    /// `[[agents]]` tables, which TOML requires of a top-level key.
    #[serde(default)]
    pub seed_revision: u32,
    pub agents: Vec<AgentConfig>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        AgentRegistry {
            seed_revision: SEED_REVISION,
            agents: Vec::new(),
        }
    }
}

/// Bumped only for a new one-time heal, never for ordinary changes to
/// [`default_registry`]. Adding a runtime or changing a default field needs no
/// bump: new ids are always merged in, and changed fields are always reported.
///
/// 2: codex gains `--dangerously-bypass-approvals-and-sandbox`. Every file so
/// far has an empty codex `extra_args`, which is the stale seed rather than a
/// choice, and the heal rule for empty `extra_args` applies exactly.
/// 3: gemini launches as `-i <prompt>`; the old `-p <prompt> -i` shape is
/// refused by gemini-cli 0.38 at startup.
/// 4: gemini pre-trusts its folder (preflight_trust = gemini).
const SEED_REVISION: u32 = 4;

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

/// `XNAUT_AGENTS_PATH` redirects the registry, the way `XNAUT_LEDGER_PATH` and
/// `XNAUT_SWITCHES_DIR` redirect theirs. Reconciliation writes to this file, so
/// a test without a redirect would edit the owner's real runtimes.
fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("XNAUT_AGENTS_PATH") {
        return PathBuf::from(path);
    }
    config_dir().join("agents.toml")
}

/// The registry's real path, for error messages that tell someone to go edit
/// it. Hard-coded `~/.config/xnaut/agents.toml` sent people to a file that does
/// not exist on macOS, where the config dir is `~/Library/Application Support`.
pub(crate) fn registry_path_display() -> String {
    config_path().display().to_string()
}

/// Where the copy of the registry taken before a given write lands.
///
/// Beside the file, suffixed with the moment it was taken:
/// `agents.toml` becomes `agents.toml.20260905T101112345Z.bak`. No colons, so
/// the same name is legal on Windows. Milliseconds because two writes in the
/// same second would otherwise overwrite each other's history, which is the one
/// thing a history file must not do.
fn version_path(path: &Path, at: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{at}.bak"));
    path.with_file_name(name)
}

/// Copies the registry aside before it is overwritten.
///
/// The whole of XNAUT-278 in one function. `agents.toml` carried a wrong
/// `prompt_injection_mode` for three months and the only way to see what had
/// changed was to reason about which build wrote it. A copy per write turns
/// that archaeology into `diff`. Returns the copy's path, or `None` when there
/// was nothing to copy yet (first seed) or the caller is a test that has not
/// redirected the path.
///
/// ponytail: no pruning. The file is ~2 KB and is written on the rare paths
/// only (first seed, one-time heal, an explicit rollback), so an install
/// accumulates a handful of copies over its life, not a log. Add a retention
/// sweep the day one of these files is measured to matter.
fn keep_previous_version(path: &Path) -> Result<Option<PathBuf>, String> {
    if !writes_allowed(path) || !path.exists() {
        return Ok(None);
    }
    let at = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ").to_string();
    let dest = version_path(path, &at);
    std::fs::copy(path, &dest).map_err(|e| {
        format!(
            "failed to keep a copy of {} at {}: {e}",
            path.display(),
            dest.display()
        )
    })?;
    Ok(Some(dest))
}

/// Where the drift record lands: `agents.toml` gives `agents.drift.json`.
fn drift_record_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "agents".into());
    path.with_file_name(format!("{stem}.drift.json"))
}

/// What reconciliation found, written down.
///
/// [`MergeNote`]s already said exactly which field differs and what each side
/// holds; they were emitted to a picker tooltip and a boot log line and were
/// gone by the time anyone asked. This is the same finding as a file, so the
/// question "what is different about THIS machine" has an answer that is still
/// there tomorrow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriftRecord {
    /// When the registry was last reconciled, RFC 3339.
    pub recorded_at: String,
    /// The registry this describes, so a record found on its own is not a riddle.
    pub registry: String,
    /// Which reconciliation revision the file is on. The one-line answer to
    /// "which registry is this machine running".
    pub seed_revision: u32,
    /// The build that wrote the record. A note reads differently once you know
    /// it was written by a version two releases back.
    pub app_version: String,
    pub notes: Vec<MergeNote>,
}

fn write_drift_record(path: &Path, registry: &AgentRegistry, notes: &[MergeNote]) {
    if !writes_allowed(path) {
        return;
    }
    let record = DriftRecord {
        recorded_at: chrono::Utc::now().to_rfc3339(),
        registry: path.display().to_string(),
        seed_revision: registry.seed_revision,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        notes: notes.to_vec(),
    };
    // Deliberately not fatal. A registry that loads is more valuable than a
    // record of how it differs, so a read-only config dir must not stop an
    // agent from launching.
    let dest = drift_record_path(path);
    match serde_json::to_vec_pretty(&record) {
        Ok(body) => {
            if let Err(e) = std::fs::write(&dest, body) {
                eprintln!("[agents] could not write {}: {e}", dest.display());
            }
        }
        Err(e) => eprintln!("[agents] could not serialize the drift record: {e}"),
    }
}

/// The last reconciliation's findings, read back from disk. `None` when no
/// record has been written yet or the file has been damaged; a missing record
/// is an absence of evidence, never an assertion that the registry is clean.
pub fn drift_record() -> Option<DriftRecord> {
    let body = std::fs::read_to_string(drift_record_path(&config_path())).ok()?;
    serde_json::from_str(&body).ok()
}

/// Default seed: six agents covering the most common cases.
/// Users can edit the file at [`config_path`] to add more.
fn default_registry() -> AgentRegistry {
    AgentRegistry {
        seed_revision: SEED_REVISION,
        agents: vec![
            AgentConfig {
                id: "claude".into(),
                label: "Claude Code".into(),
                detect_cmd: "claude".into(),
                launch_cmd: "claude".into(),
                extra_args: vec!["--dangerously-skip-permissions".into()],
                expected_process: "claude".into(),
                // Argv, NOT --prefill. `claude "<task>"` starts and RUNS the
                // task. `--prefill` is a draft affordance: it puts the text in
                // the composer and says "scroll to review it all before
                // pressing Enter", which is correct for a human drafting and
                // fatal for a wake, because it waits for a keypress that never
                // comes. Two releases were spent teaching xNAUT to synthesise
                // that keypress before noticing the flag itself was the wrong
                // tool (XNAUT-249).
                prompt_injection_mode: PromptInjectionMode::Argv,
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
                // Codex's own equivalent of claude's --dangerously-skip-permissions.
                // Without it the interactive TUI parks on "Would you like to run
                // the following command?" for every git push, curl to the hook
                // server and inbox notify, and that prompt never reaches the
                // inbox: the app only shows "Action Required" in the title. On
                // 2026-09-05 NautBot sat on three of them until André pressed
                // Enter by hand. xNAUT's veto layer is the guard rail; codex's
                // approval dialog is a second one with no one on the other end.
                extra_args: vec!["--dangerously-bypass-approvals-and-sandbox".into()],
                expected_process: "codex".into(),
                prompt_injection_mode: PromptInjectionMode::Argv,
                draft_prompt_flag: None,
                draft_prompt_env_var: None,
                preflight_trust: Some(PreflightTrust::Codex),
                // ponytail: no OPENAI_BASE_URL override — codex 0.14x authenticates via
                // ChatGPT login (~/.codex/auth.json), and forcing it at NautGate breaks that
                // auth. Users who want NautGate routing for codex (API-key mode) can add the
                // env back in their own agents.toml.
                env: HashMap::new(),
            },
            AgentConfig {
                id: "gemini".into(),
                label: "Gemini".into(),
                detect_cmd: "gemini".into(),
                launch_cmd: "gemini".into(),
                extra_args: vec![],
                expected_process: "gemini".into(),
                // gemini-cli 0.38: `-i <prompt>` runs the prompt and stays
                // interactive; `-p <prompt> -i` (the old shape) dies at
                // "Not enough arguments following: i" and prints --help. The
                // first gemini dispatch on tron, 2026-09-06, was exactly that.
                prompt_injection_mode: PromptInjectionMode::FlagPrompt,
                draft_prompt_flag: Some("-i".into()),
                draft_prompt_env_var: None,
                preflight_trust: Some(PreflightTrust::Gemini),
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

/// One thing reconciliation did to the user's registry, or declined to do.
///
/// A merge that cannot be applied safely has to be findable, or it is just the
/// seed-once bug wearing a merge's clothes: the file says one thing, the app
/// does another, and nobody can tell. These ride out on [`agent_list`] so the
/// runtime picker can show them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeNote {
    pub agent_id: String,
    pub message: String,
}

#[derive(Debug)]
pub struct LoadedRegistry {
    pub registry: AgentRegistry,
    pub notes: Vec<MergeNote>,
}

const REGISTRY_HEADER: &str = "\
# xNAUT agent runtimes.
#
# This file is yours. xNAUT only ever ADDS runtimes it has learned about since
# it was written; it never overwrites an entry you have edited. Where one of
# your fields differs from this build's default, the difference is reported in
# the runtime picker instead of being applied behind your back.
#
# seed_revision records that the one-time reconciliation has already run.
# Removing it re-runs that heal against your edited values.
";

/// Loads the user's registry, seeding it on first run and reconciling it with
/// the built-in defaults on every run after that.
pub fn load_or_seed_registry() -> Result<AgentRegistry, String> {
    Ok(load_registry()?.registry)
}

/// The same load, plus what reconciliation had to say about it.
pub fn load_registry() -> Result<LoadedRegistry, String> {
    let path = config_path();
    if !path.exists() {
        let registry = default_registry();
        write_registry(&path, &registry)?;
        write_drift_record(&path, &registry, &[]);
        return Ok(LoadedRegistry {
            registry,
            notes: Vec::new(),
        });
    }

    let body = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let user = toml::from_str::<AgentRegistry>(&body).map_err(|e| {
        format!(
            "failed to parse {}: {e}. Fix it by hand: xNAUT will not rewrite a registry it cannot read, so nothing here has been touched.",
            path.display()
        )
    })?;

    let healed = user.seed_revision < SEED_REVISION;
    let (registry, notes, added) = reconcile(user);

    // Persist, or the file goes on describing a machine the app is not running.
    // A pre-revision file is rewritten once, to record both the heal and the
    // revision. After that the file is only ever APPENDED to, so hand-written
    // comments, ordering and formatting survive every later release.
    if healed {
        write_registry(&path, &registry)?;
    } else if !added.is_empty() {
        append_agents(&path, &body, &added)?;
    }
    // Every load, not just the loads that wrote. Drift is a property of the
    // file as it stands, so a machine that has been quietly diverging for a
    // release still has a current record of how.
    write_drift_record(&path, &registry, &notes);
    Ok(LoadedRegistry { registry, notes })
}

/// Puts one runtime back to this build's defaults.
///
/// The way out of a drifted registry that does not involve opening TOML and
/// getting the injection mode right by hand. Scoped to a single id on purpose:
/// the reason someone reaches for this is that ONE runtime misbehaves, and
/// resetting the whole file would take away every other deliberate edit as the
/// price of fixing it.
///
/// Destructive by nature, so it goes through [`write_registry`], which keeps
/// the previous file. That copy is the undo.
pub fn rollback_agent(id: &str) -> Result<Option<PathBuf>, String> {
    let path = config_path();
    let mut registry = load_registry()?.registry;
    let default = default_registry()
        .agents
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| {
            format!("this build has no default for `{id}`, so there is nothing to roll back to. It is a runtime you added yourself; edit {} to change it.", path.display())
        })?;

    match registry.agents.iter_mut().find(|a| a.id == id) {
        Some(existing) => *existing = default,
        // Rolling back a runtime the file dropped is a restore, not an error.
        None => registry.agents.push(default),
    }
    let kept = write_registry(&path, &registry)?;
    // The old record described the drift this just removed.
    write_drift_record(&path, &registry, &reconcile(registry.clone()).1);
    Ok(kept)
}

/// Merges the built-in defaults into a loaded registry.
///
/// The merge rule is per ENTRY, not per field: an id the file already has
/// belongs to the user and is kept verbatim, an id the file lacks is added.
/// An entry is the unit a person edits, so it is the honest unit to arbitrate.
/// Field-level merging would have to decide whether `extra_args = []` on the
/// `claude` entry is a stale seed or a deliberate refusal of
/// `--dangerously-skip-permissions`, and guessing wrong there silently hands an
/// agent permissions its owner took away.
///
/// Nothing is dropped quietly: every default that differs from the user's entry
/// comes back as a [`MergeNote`].
///
/// `plugins.rs::load_store` solves the same problem per FIELD, refreshing the
/// mechanics of any entry not marked `owner_edited`. That works there and
/// cannot work here: the plugin store is written by `plugin_save`, so there is
/// a moment at which to stamp the flag. This file has no writer but a text
/// editor, so no such moment exists, and an entry's own contents are the only
/// evidence of intent there will ever be. Do not port that rule over without
/// first giving this registry a UI that can record the edit.
///
/// Returns the merged registry, the notes, and the entries that were newly
/// added (which is what has to reach the file on disk).
///
/// ponytail: a runtime the user DELETED comes back, because the file records
/// what it has, not what was ever seeded into it, so "deleted" and "never
/// seeded" look identical. It comes back visibly, in the file and the picker,
/// which is the cheap ceiling; a per-id `seeded = [...]` list would raise it.
fn reconcile(mut registry: AgentRegistry) -> (AgentRegistry, Vec<MergeNote>, Vec<AgentConfig>) {
    let healing = registry.seed_revision < SEED_REVISION;
    let mut notes = Vec::new();
    let mut added = Vec::new();

    for default in default_registry().agents {
        match registry.agents.iter_mut().find(|a| a.id == default.id) {
            Some(existing) => {
                if healing {
                    heal_pre_revision(existing, &default, &mut notes);
                }
                for drift in field_drift(existing, &default) {
                    notes.push(MergeNote {
                        agent_id: existing.id.clone(),
                        message: format!("kept your {drift}"),
                    });
                }
            }
            None => {
                notes.push(MergeNote {
                    agent_id: default.id.clone(),
                    message: format!(
                        "added: this build knows the `{}` runtime and your file did not",
                        default.id
                    ),
                });
                added.push(default.clone());
                registry.agents.push(default);
            }
        }
    }
    registry.seed_revision = SEED_REVISION;
    (registry, notes, added)
}

/// The one-time heal for a file written before reconciliation existed.
///
/// A pre-revision file carries no record of what was seeded into it, so there
/// is genuinely no way to tell a value its owner chose from a value that is
/// merely three months old. This makes that judgement ONCE, writes the result
/// to disk with the revision, and reports every field it touched. The code it
/// replaces made the same guess silently on every single load, forever, and
/// left the file saying the opposite of what the app did.
///
/// The judgement: an empty `extra_args` or `env` is stale rather than
/// deliberately cleared, and a `claude` entry still on `flag-prompt` is stale
/// rather than chosen. That is exactly the damage on a file dated 2026-06-10:
/// it dropped `--dangerously-skip-permissions`, dropped the NautGate routing,
/// and parked woken runs at a composer nobody was there to submit (XNAUT-182).
/// Healing it is the point. After the revision is written the guess is never
/// repeated: the user's values are authoritative and a changed default is
/// reported instead.
fn heal_pre_revision(
    existing: &mut AgentConfig,
    default: &AgentConfig,
    notes: &mut Vec<MergeNote>,
) {
    let healed = |field: &str, was: String, notes: &mut Vec<MergeNote>| {
        notes.push(MergeNote {
            agent_id: default.id.clone(),
            message: format!(
                "one-time repair: {field} was {was} in a file written before xNAUT reconciled this registry, and took this build's default. Edit the file to change it back; xNAUT will not touch it again."
            ),
        });
    };

    if existing.extra_args.is_empty() && !default.extra_args.is_empty() {
        existing.extra_args = default.extra_args.clone();
        healed("extra_args", "empty".into(), notes);
    }
    if existing.env.is_empty() && !default.env.is_empty() {
        existing.env = default.env.clone();
        healed("env", "empty".into(), notes);
    }
    if existing.id == "claude" && existing.prompt_injection_mode == PromptInjectionMode::FlagPrompt {
        existing.prompt_injection_mode = default.prompt_injection_mode;
        healed(
            "prompt_injection_mode",
            "flag-prompt, which parks a woken run at a composer nobody submits".into(),
            notes,
        );
    }
    // Revision 3: gemini's launch shape. Every registry so far carries
    // `-p <prompt> -i`, which gemini-cli 0.38 rejects before it starts.
    if existing.id == "gemini"
        && existing.prompt_injection_mode == PromptInjectionMode::FlagPromptInteractive
    {
        existing.prompt_injection_mode = default.prompt_injection_mode;
        existing.draft_prompt_flag = default.draft_prompt_flag.clone();
        healed(
            "prompt_injection_mode",
            "flag-prompt-interactive with -p, which gemini-cli 0.38 refuses at launch".into(),
            notes,
        );
    }
    // Revision 4: gemini pre-trusts its folder, or every cold launch parks on
    // the trust dialog.
    if existing.id == "gemini" && existing.preflight_trust.is_none() && default.preflight_trust.is_some() {
        existing.preflight_trust = default.preflight_trust;
        healed("preflight_trust", "unset, so gemini asked for folder trust on every launch".into(), notes);
    }
}

/// Every field where the user's entry and this build's default disagree,
/// rendered for a human.
///
/// ponytail: written out by hand, so a field added to [`AgentConfig`] that
/// nobody adds here goes unreported. Deriving it from a serde round-trip would
/// stay complete on its own, at the cost of rendering values as raw TOML; the
/// ceiling is one line per field in this function.
fn field_drift(user: &AgentConfig, default: &AgentConfig) -> Vec<String> {
    fn note(out: &mut Vec<String>, field: &str, user: String, default: String) {
        if user != default {
            out.push(format!(
                "{field} = {user} (this build's default is {default})"
            ));
        }
    }
    // A HashMap has no stable order, so render it sorted or identical maps
    // compare unequal at random.
    fn env(map: &HashMap<String, String>) -> String {
        let mut pairs: Vec<_> = map.iter().collect();
        pairs.sort();
        format!("{pairs:?}")
    }

    let mut out = Vec::new();
    note(
        &mut out,
        "label",
        format!("{:?}", user.label),
        format!("{:?}", default.label),
    );
    note(
        &mut out,
        "detect_cmd",
        format!("{:?}", user.detect_cmd),
        format!("{:?}", default.detect_cmd),
    );
    note(
        &mut out,
        "launch_cmd",
        format!("{:?}", user.launch_cmd),
        format!("{:?}", default.launch_cmd),
    );
    note(
        &mut out,
        "extra_args",
        format!("{:?}", user.extra_args),
        format!("{:?}", default.extra_args),
    );
    note(
        &mut out,
        "expected_process",
        format!("{:?}", user.expected_process),
        format!("{:?}", default.expected_process),
    );
    note(
        &mut out,
        "prompt_injection_mode",
        format!("{:?}", user.prompt_injection_mode),
        format!("{:?}", default.prompt_injection_mode),
    );
    note(
        &mut out,
        "draft_prompt_flag",
        format!("{:?}", user.draft_prompt_flag),
        format!("{:?}", default.draft_prompt_flag),
    );
    note(
        &mut out,
        "draft_prompt_env_var",
        format!("{:?}", user.draft_prompt_env_var),
        format!("{:?}", default.draft_prompt_env_var),
    );
    note(
        &mut out,
        "preflight_trust",
        format!("{:?}", user.preflight_trust),
        format!("{:?}", default.preflight_trust),
    );
    note(&mut out, "env", env(&user.env), env(&default.env));
    out
}

/// Loading used to be read-only, so a test in some unrelated module that
/// happened to reach the registry was harmless. Reconciliation persists, so it
/// is not harmless any more: it would heal and rewrite the owner's real
/// runtimes. Under `cargo test` the registry is only ever written when the test
/// has redirected `XNAUT_AGENTS_PATH` at a scratch file of its own.
#[cfg(test)]
fn writes_allowed(path: &Path) -> bool {
    // Asks about THIS path, never about a global.
    //
    // The previous version read XNAUT_AGENTS_PATH while `path` had been
    // captured earlier by config_path(). That is a time-of-check race across
    // threads: a test that captured the REAL path before another test set the
    // variable was then granted permission to write it. It happened. On
    // 2026-09-03 the owner's real agents.toml was rewritten from 943 bytes to
    // 1778 by a suite run, and had to be restored by hand.
    //
    // A guard that is a pure function of its argument cannot race, because
    // there is no second read to disagree with the first.
    path != real_config_path()
}

/// The owner's actual registry, ignoring any test redirect. Only the guard
/// needs this; everything else goes through config_path.
fn real_config_path() -> PathBuf {
    config_dir().join("agents.toml")
}

#[cfg(not(test))]
fn writes_allowed(_path: &Path) -> bool {
    true
}

/// Writes the whole registry. Used on first seed, on the single pre-revision
/// heal, and on a rollback, all of which need a top-level key that TOML will
/// only accept above the `[[agents]]` tables.
///
/// A full write is the destructive one: it is a serde round-trip, so the
/// owner's comments and ordering do not survive it. That is exactly why the
/// copy is taken here. Returns the copy's path when there was a file to copy.
fn write_registry(path: &Path, registry: &AgentRegistry) -> Result<Option<PathBuf>, String> {
    if !writes_allowed(path) {
        return Ok(None);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    }
    let kept = keep_previous_version(path)?;
    let body = toml::to_string_pretty(registry)
        .map_err(|e| format!("failed to serialize agent registry: {e}"))?;
    std::fs::write(path, format!("{REGISTRY_HEADER}\n{body}"))
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    Ok(kept)
}

/// Adds new runtimes by appending `[[agents]]` blocks, so not one byte the user
/// wrote is rewritten. A serde round-trip would drop their comments and
/// reorder their entries every time this build learned a new runtime.
fn append_agents(path: &Path, body: &str, added: &[AgentConfig]) -> Result<(), String> {
    if !writes_allowed(path) {
        return Ok(());
    }
    keep_previous_version(path)?;
    let block = toml::to_string_pretty(&AgentRegistry {
        seed_revision: SEED_REVISION,
        agents: added.to_vec(),
    })
    .map_err(|e| format!("failed to serialize new agent entries: {e}"))?;
    // `seed_revision` is already at the top of the file; a second copy down
    // here would be a duplicate key and would break the next parse.
    let block = block
        .lines()
        .skip_while(|line| !line.starts_with("[["))
        .collect::<Vec<_>>()
        .join("\n");

    let mut out = String::new();
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n# Added by xNAUT: runtimes this build knows that your file did not have.\n");
    out.push_str(&block);
    out.push('\n');

    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(out.as_bytes()))
        .map_err(|e| format!("failed to append to {}: {e}", path.display()))
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

/// The PATH an agent runtime gets, exposed so an MCP server started from a
/// chat turn finds npx the same way a launched agent does. A Finder-launched
/// app has a minimal PATH and would otherwise fail with "npx not found".
pub(crate) fn runtime_path_public() -> Option<String> {
    runtime_path()
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
    *.md|*.markdown)
      # A document belongs in xNAUT's split pane, not in whatever the system
      # has registered for .md — which on this Mac is Xcode.
      case "$arg" in /*) doc="$arg" ;; *) doc="$PWD/$arg" ;; esac
      if [ -n "$XNAUT_HOOK_URL" ] && [ -r "$doc" ] && command -v python3 >/dev/null 2>&1; then
        python3 - "$doc" "$XNAUT_HOOK_URL" <<'PYDOC' && exit 0
import json, os, sys, urllib.request
# The token comes from the environment, never from argv: an argument is
# readable by any `ps` on the machine (XNAUT-206).
path, base, token = sys.argv[1], sys.argv[2].rstrip('/'), os.environ['XNAUT_HOOK_TOKEN']
text = open(path, encoding='utf-8', errors='replace').read()
title = os.path.basename(path).rsplit('.', 1)[0].replace('-', ' ').replace('_', ' ').strip().title()
body = json.dumps({'title': title, 'content': text}).encode()
request = urllib.request.Request(base + '/v1/document', data=body, headers={
    'Content-Type': 'application/json',
    'Authorization': 'Bearer ' + token,
    'X-Xnaut-Session': token,
})
urllib.request.urlopen(request, timeout=10).read()
PYDOC
      fi
      continue ;;
    *.html|*.htm|*.svg|*.pdf)
      case "$arg" in /*) target="$arg" ;; *) target="$PWD/$arg" ;; esac ;;
    *) continue ;;
  esac
  if [ -n "$XNAUT_HOOK_URL" ] && command -v curl >/dev/null 2>&1; then
    # Headers ride stdin (curl -K -), not argv, so the session token is not
    # sitting in `ps` output for the life of the request (XNAUT-206).
    if printf 'header = "Authorization: Bearer %s"\nheader = "X-Xnaut-Session: %s"\n' \
        "$XNAUT_HOOK_TOKEN" "$XNAUT_HOOK_TOKEN" \
      | curl -sf -m 5 -K - -X POST "${XNAUT_HOOK_URL%/}/v1/open" \
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
    // Configured is not running. This path short-circuits `resolve_base_url`,
    // so without this check a machine that has NautGate in settings but not on
    // the box hands Claude Code a dead gateway; it cannot reach the API and its
    // status line reads "Not logged in · Run /login", which looks exactly like
    // an expired credential. Cost four dispatches on tron, 2026-09-06. Falling
    // through to `None` injects nothing and the agent uses its own
    // subscription, which is the correct behaviour on a box without a gateway.
    if !endpoint_alive(&route.endpoint) {
        return None;
    }
    let (endpoint, token_name) = match key {
        // Deliberately no token for Claude. Claude Code authenticates with its
        // own `sk-ant-oat01-` Max token, and NautGate forwards that verbatim to
        // the subscription (`anthropic_oauth_forwarder.py:112`). Setting
        // ANTHROPIC_API_KEY overrides that OAuth session, so the gateway sees an
        // `ng_` key instead, misses the subscription lane, and falls through to
        // the metered key. That regressed on 2026-08-14 in b723265 and every
        // Claude agent died the moment the metered balance hit zero.
        "ANTHROPIC_BASE_URL" => return Some((anthropic_base(&route.endpoint), None)),
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

#[derive(Serialize)]
struct MaxLaunchRegistration<'a> {
    app: &'static str,
    project: &'a str,
    native_session: &'a str,
    run_id: &'a str,
    ttl_seconds: u32,
}

#[derive(Deserialize)]
struct MaxLaunchRegistrationResponse {
    base_url: String,
}

async fn register_nautgate_max_launch(
    route: &crate::settings::LlmSettings,
    project: &str,
    native_session: &str,
    run_id: &str,
) -> Result<String, String> {
    let token = route
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "NautGate Max launch binding requires the configured NautGate key".to_string()
        })?;
    let origin = route
        .endpoint
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or(route.endpoint.trim_end_matches('/'));
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|error| format!("could not create NautGate launch client: {error}"))?
        .post(format!("{origin}/v1/max/launches"))
        .bearer_auth(token)
        .json(&MaxLaunchRegistration {
            app: "xnaut",
            project,
            native_session,
            run_id,
            ttl_seconds: 21_600,
        })
        .send()
        .await
        .map_err(|error| format!("NautGate Max launch registration failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "NautGate rejected Max launch registration ({})",
            response.status()
        ));
    }
    let registered: MaxLaunchRegistrationResponse = response
        .json()
        .await
        .map_err(|error| format!("invalid NautGate Max launch response: {error}"))?;
    if registered.base_url.trim().is_empty() {
        return Err("NautGate Max launch response omitted base_url".into());
    }
    Ok(registered.base_url)
}

#[tauri::command]
pub async fn nautgate_max_launch_register(
    state: State<'_, AppState>,
    project: String,
    native_session: String,
    run_id: String,
) -> Result<String, String> {
    let route = {
        let settings = state.settings.lock().await;
        crate::chat::provider_llm(&settings, "nautgate")
    }
    .ok_or_else(|| "NautGate is not configured".to_string())?;
    register_nautgate_max_launch(&route, &project, &native_session, &run_id).await
}

#[derive(Debug, Serialize)]
pub struct AgentListing {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub injection_mode: PromptInjectionMode,
    /// What reconciliation did to this entry, or could not do. `None` when the
    /// file and this build agree. The picker shows it as a tooltip, which is
    /// the whole difference between a stale registry and a findable one.
    ///
    /// ponytail: only the worktree modal's runtime picker renders it. The other
    /// four `agent_list` callers ignore the field rather than each growing
    /// their own affordance; the ceiling is one line per surface.
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LaunchAgentRequest {
    #[serde(default)]
    pub ticket: Option<String>,
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
    /// Back this run with a zellij session even outside conversation mode, so
    /// it survives the app quitting (XNAUT-242). None defaults to
    /// `conversation_mode`, which keeps every existing caller's behavior; a
    /// cold wake sets true, because a fleet that dies with the app is not a
    /// fleet. Deliberately not conversation_mode itself: that also swaps in
    /// the conversation harness, the wrong contract for a woken worker.
    #[serde(default)]
    pub durable: Option<bool>,
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
    /// Who this run belongs to, e.g. "rudi".
    ///
    /// Reaches the agent as XNAUT_AGENT_HANDLE and is stamped into the veto
    /// payload by the hook script: the harness's PreToolUse envelope names the
    /// tool but never the caller, so without this a rule cannot be scoped to
    /// one agent and two agents on one file cannot be told apart.
    #[serde(default)]
    pub agent_handle: String,
}

#[derive(Debug, Serialize)]
pub struct LaunchAgentResponse {
    pub run_id: Option<String>,
    pub session_id: String,
    pub agent_id: String,
    pub injection_mode: PromptInjectionMode,
    pub conversation_id: Option<String>,
    /// Clean stdout of a zellij-backed run. The PTY now hosts zellij, whose
    /// chrome would corrupt a JSON frame, so the conversation reads this file
    /// instead. Absent when the run went straight to a PTY.
    #[serde(default)]
    pub output_path: Option<String>,
    /// Exact background zellij session, used to terminate a guard-paused run.
    #[serde(default)]
    pub zellij_session: Option<String>,
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

/// `pub(crate)` since XNAUT-266 slice 2: a remote launch needs the SAME argv
/// this builds, because the flag shape per runtime (argv, `-p`, `-i`) is not a
/// thing to have two answers for. The remote path takes argv[0] as the bare
/// command name on purpose, since the binary it resolves to lives on the VM.
pub(crate) fn build_launch(
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


/// Which runtime a NautLoom-style model string names, and the model to pass it
/// (XNAUT-266).
///
/// The looms, the swarm pane and the Designer all speak in MODELS — "codex",
/// "codex:gpt-5", "claude-opus-5", "local", "" — and each of them had its own
/// copy of this translation: a bash `case "$MODEL" in codex*)` in
/// `nautloom.rs`, and a chain of regexes in `multiagent-pane.js`. Two copies of
/// "which binary is this" is two places for a runtime to be launched
/// differently from the fleet, which is the duplication this ticket exists to
/// remove.
///
/// "local" and the empty string both mean "the CLI's own default model", not a
/// model named `local`: the local-LLM path passes its endpoint as env instead
/// (see `nautloom::loom_run`), because Claude Code has no flag for it.
pub fn runtime_for_model(model: &str) -> (&'static str, Option<String>) {
    let model = model.trim();
    let after = |prefix: &str| -> Option<String> {
        model
            .strip_prefix(prefix)
            .map(|rest| rest.trim_start_matches([':', '-']).trim().to_string())
            .filter(|rest| !rest.is_empty())
    };
    if model.starts_with("codex") {
        return ("codex", after("codex"));
    }
    if model.starts_with("pi") {
        return ("pi", after("pi"));
    }
    if model.is_empty() || model.eq_ignore_ascii_case("local") {
        return ("claude", None);
    }
    ("claude", Some(model.to_string()))
}

/// ONE shell line that runs an agent CLI headless against a goal file
/// (XNAUT-266).
///
/// The counterpart to `build_launch`, which answers the same question for an
/// INTERACTIVE run. Both are the registry's answer rather than the caller's:
/// which binary an agent is, and what it needs in order to run unattended, is
/// the runtime's property and not the pane's.
///
/// The flags are per-runtime because the runtimes genuinely differ, and each
/// one earns its place:
/// - claude: `-p` with `stream-json` so the log fills AS THE RUN HAPPENS.
///   `--verbose` alone prints only the final result, which is why a working
///   agent's log used to look dead for its whole run.
/// - claude's `--settings`: hooks are retained for a MANAGED run (XNAUT-107)
///   and disabled for ordinary headless work. Decided in the shell rather than
///   here because the answer depends on the environment the line ends up in —
///   a sandbox never has those variables, the owner's Mac does.
/// - codex: prints plain text already and needs no stream formatting.
///
/// The two knobs are the only ones the four callers actually differed on, and
/// both are properties of an UNATTENDED RUN rather than flags to pass through.
/// A pass-through would have recreated the problem one indirection later.
#[derive(Debug, Clone, Default)]
pub struct Headless<'a> {
    /// A model string in the looms' vocabulary: "codex", "codex:gpt-5",
    /// "claude-opus-5", "local", or empty for the CLI's default.
    pub model: &'a str,
    /// The file the goal was written to, read by the shell at run time so a
    /// ticket-sized prompt never has to survive command-line quoting.
    pub goal_file: &'a str,
    /// Continue an earlier session of this agent instead of starting fresh.
    pub resume: Option<&'a str>,
    /// Run with NO user MCP servers. The personas and the Designer set this
    /// because they only use file tools, and MCP teardown stalled their runs
    /// for minutes after the final message.
    pub isolate_mcp: bool,
}

pub fn headless_command(spec: &Headless) -> Result<String, String> {
    let (runtime_id, model) = runtime_for_model(spec.model);
    let registry = load_or_seed_registry()?;
    let cfg = registry
        .find(runtime_id)
        .ok_or_else(|| format!("unknown agent runtime: {runtime_id}"))?;
    let bin = &cfg.launch_cmd;
    let goal = format!("\"$(cat {})\"", shell_quote(spec.goal_file));
    let flag = match model.as_deref().filter(|m| !m.is_empty()) {
        Some(model) => match model_flag(runtime_id) {
            Some(flag) => format!("{flag} {} ", shell_quote(model)),
            None => String::new(),
        },
        None => String::new(),
    };
    Ok(match runtime_id {
        "claude" => {
            // Sanitised HERE rather than at each call site: a session id is an
            // id, and anything else in it would be shell.
            let resume = match spec.resume.map(str::trim).filter(|id| !id.is_empty()) {
                Some(id) => {
                    let clean: String = id
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .collect();
                    if clean.is_empty() {
                        String::new()
                    } else {
                        format!("--resume {clean} ")
                    }
                }
                None => String::new(),
            };
            let mcp = if spec.isolate_mcp {
                "--strict-mcp-config --mcp-config '{\"mcpServers\":{}}' "
            } else {
                ""
            };
            format!(
                "{bin} -p --settings \"$([ -n \"$XNAUT_VETO_URL$XNAUT_HOOK_TOKEN\" ] && echo '{{}}' \
|| echo '{{\"disableAllHooks\":true}}')\" --verbose --output-format stream-json \
{flag}{resume}{mcp}--dangerously-skip-permissions {goal}"
            )
        }
        "codex" => format!("{bin} exec --dangerously-bypass-approvals-and-sandbox {flag}{goal}"),
        _ => format!("{bin} {flag}{goal}"),
    })
}

/// The same line, for the panes that used to build it themselves.
#[tauri::command]
pub fn agent_headless_command(
    model: String,
    goal_file: String,
    resume: Option<String>,
    isolate_mcp: Option<bool>,
) -> Result<String, String> {
    headless_command(&Headless {
        model: &model,
        goal_file: &goal_file,
        resume: resume.as_deref(),
        isolate_mcp: isolate_mcp.unwrap_or(false),
    })
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
pub(crate) fn run_dir() -> Result<std::path::PathBuf, String> {
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
/// Codex takes its hooks as launch flags rather than from a settings file, so
/// the veto (XNAUT-132) has to be attached here rather than by
/// agent_hook_setup. Empty when the script is not on disk, which is the
/// fail-open path: no flags, no policy, a launch that still works.
fn codex_veto_flags() -> Vec<String> {
    let Some(script) = crate::agent_hook_setup::veto_script_path() else {
        return Vec::new();
    };
    let command = script.display().to_string();
    if command.contains('"') || command.contains('\\') {
        // The path goes into a TOML fragment; a quote in it would break the
        // config rather than the policy, and a broken config is a dead launch.
        return Vec::new();
    }
    vec![
        "--enable".into(),
        "hooks".into(),
        "--dangerously-bypass-hook-trust".into(),
        "-c".into(),
        format!("hooks.PreToolUse=[{{hooks=[{{type=\"command\",command=\"{command}\"}}]}}]"),
    ]
}

pub(crate) fn prepare_zellij_run(
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
    // script(1), not a tee pipeline (XNAUT-242): a TUI agent writes to the
    // tty, not stdout, so the pipe captured nothing and the out file stayed
    // empty on every cold run. script gives the command a real tty AND
    // captures every byte of it — the same stream the watchers know how to
    // render. BSD and util-linux disagree on argument order, hence the uname
    // switch; stderr rides the capture, so the err file only ever holds
    // failures from before script starts.
    let out_q = shell_quote(&out.to_string_lossy());
    let err_q = shell_quote(&err.to_string_lossy());
    lines.push(format!(
        "if [ \"$(uname)\" = Darwin ]; then script -q {out_q} /bin/sh -c {cmd}; else script -q -c {cmd} {out_q}; fi 2>>{err_q}",
        cmd = shell_quote(&command),
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
    // Every token the run holds is an `export` line in here, and the run
    // directory is under the user's own home but the file was landing at 0644
    // (XNAUT-203). Same exposure as the argv leak, one directory over.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o600));
    }
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
    // (mcp url, mcp token) when the local agent server is up. Threaded in
    // rather than fetched here so this stays a pure builder.
    xnaut_mcp: Option<(String, String, String)>,
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
    let plugin_flags = crate::plugins::launch_flags(
        &cfg.id,
        &plugins,
        xnaut_mcp
            .as_ref()
            .map(|(url, token, session)| (url.as_str(), token.as_str(), session.as_str())),
    );
    // The values behind those flags. Codex names its plugin credentials rather
    // than spelling them on the command line (XNAUT-206), so they have to be
    // in its environment or the servers start unauthenticated.
    env.extend(crate::plugins::launch_env(&cfg.id, &plugins));

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
            let mut argv = vec![cfg.launch_cmd.clone()];
            // In front of the subcommand: codex parses global flags before it.
            argv.extend(codex_veto_flags());
            argv.push("exec".into());
            // codex stops dead outside a git repository with "Not inside a
            // trusted directory and --skip-git-repo-check was not specified",
            // and that message went to stderr where the chat never showed it.
            // An explicitly selected folder can still be exactly that case,
            // so pass the flag; the sandbox policy is what actually bounds the
            // run. Scratch workspaces themselves are repositories (XNAUT-274).
            argv.push("--skip-git-repo-check".into());
            if resume {
                let id = conversation_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Codex conversation id is missing; start a new thread".to_string())?;
                argv.push("resume".into());
                argv.push("--json".into());
                // resume takes a DIFFERENT argument set; see policy::resume_flags.
                argv.push("--skip-git-repo-check".into());
                argv.extend(crate::policy::resume_flags(
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
            if let Err(why) = accept_codex_project_trust(worktree_path) {
                eprintln!("[agents] could not pre-trust {worktree_path} for codex: {why}");
            }
        }
        PreflightTrust::Gemini => {
            if let Err(why) = accept_gemini_folder_trust(worktree_path) {
                eprintln!("[agents] could not pre-trust {worktree_path} for gemini: {why}");
            }
        }
    }
}

/// gemini-cli asks "Do you trust the files in this folder?" and records the
/// answer in `~/.gemini/trustedFolders.json` as `{"<path>": "TRUST_FOLDER"}`.
/// The second gemini launch on tron (2026-09-06 14:51) parked on that dialog
/// after the first had died on its argv. Same treatment as codex and Claude.
fn accept_gemini_folder_trust(worktree_path: &str) -> Result<(), String> {
    let dir = std::fs::canonicalize(worktree_path)
        .unwrap_or_else(|_| std::path::PathBuf::from(worktree_path));
    let store = dirs::home_dir()
        .ok_or_else(|| "home directory is unavailable".to_string())?
        .join(".gemini")
        .join("trustedFolders.json");
    write_gemini_folder_trust(&store, &dir.to_string_lossy())
}

fn write_gemini_folder_trust(store: &std::path::Path, dir: &str) -> Result<(), String> {
    let mut map: serde_json::Map<String, serde_json::Value> = std::fs::read_to_string(store)
        .ok()
        .and_then(|body| serde_json::from_str(&body).ok())
        .unwrap_or_default();
    if map.get(dir).and_then(|v| v.as_str()) == Some("TRUST_FOLDER") {
        return Ok(());
    }
    map.insert(dir.to_string(), serde_json::Value::String("TRUST_FOLDER".into()));
    if let Some(parent) = store.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(store, serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// Codex asks "Do you trust the contents of this directory?" on the first run
/// in any git repository it has not seen, and records the answer in
/// `~/.codex/config.toml` as `[projects."<path>"] trust_level = "trusted"`.
/// Since XNAUT-274 made the wake workspace a repository, every cold NautBot
/// launch parked on that prompt (2026-09-06 12:15, the first triage wake).
/// The answer is written before the launch, the same way `.claude.json` is
/// pre-trusted for Claude Code. Idempotent; the path is codex's own spelling,
/// canonicalised.
fn accept_codex_project_trust(worktree_path: &str) -> Result<(), String> {
    let dir = std::fs::canonicalize(worktree_path)
        .unwrap_or_else(|_| std::path::PathBuf::from(worktree_path));
    let config = dirs::home_dir()
        .ok_or_else(|| "home directory is unavailable".to_string())?
        .join(".codex")
        .join("config.toml");
    write_codex_project_trust(&config, &dir.to_string_lossy())
}

fn write_codex_project_trust(config: &std::path::Path, dir: &str) -> Result<(), String> {
    let existing = std::fs::read_to_string(config).unwrap_or_default();
    let header = format!("[projects.{dir:?}]");
    if existing.lines().any(|line| line.trim() == header) {
        return Ok(());
    }
    if let Some(parent) = config.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut body = existing;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(&format!("\n{header}\ntrust_level = \"trusted\"\n"));
    std::fs::write(config, body).map_err(|e| e.to_string())
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

#[cfg(test)]
mod codex_trust_tests {
    #[test]
    fn gemini_folder_trust_is_written_in_its_own_shape() {
        let dir = std::env::temp_dir().join(format!("xnaut-gemini-trust-{}", uuid::Uuid::new_v4()));
        let store = dir.join(".gemini").join("trustedFolders.json");
        super::write_gemini_folder_trust(&store, "/Users/x/agent-workspaces/gemini").unwrap();
        super::write_gemini_folder_trust(&store, "/Users/x/agent-workspaces/gemini").unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&store).unwrap()).unwrap();
        assert_eq!(v["/Users/x/agent-workspaces/gemini"], "TRUST_FOLDER");
        assert_eq!(v.as_object().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_trust_entry_is_written_once_in_codexs_own_shape() {
        let dir = std::env::temp_dir().join(format!("xnaut-codex-trust-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.toml");
        std::fs::write(&config, "model = \"gpt-5\"\n").unwrap();
        super::write_codex_project_trust(&config, "/Users/x/Application Support/xnaut/agent-workspaces/nautbot").unwrap();
        super::write_codex_project_trust(&config, "/Users/x/Application Support/xnaut/agent-workspaces/nautbot").unwrap();
        let body = std::fs::read_to_string(&config).unwrap();
        assert_eq!(body.matches("trust_level").count(), 1, "written once: {body}");
        assert!(body.starts_with("model = \"gpt-5\"\n"), "the owner's config survives: {body}");
        let parsed: toml::Value = toml::from_str(&body).expect("codex can still parse its config");
        assert_eq!(
            parsed["projects"]["/Users/x/Application Support/xnaut/agent-workspaces/nautbot"]["trust_level"].as_str(),
            Some("trusted")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn agent_list() -> Result<Vec<AgentListing>, String> {
    let LoadedRegistry { registry, notes } = load_registry()?;
    Ok(registry
        .agents
        .into_iter()
        .map(|a| {
            let mine: Vec<&str> = notes
                .iter()
                .filter(|n| n.agent_id == a.id)
                .map(|n| n.message.as_str())
                .collect();
            AgentListing {
                available: binary_on_path(&a.detect_cmd),
                injection_mode: a.prompt_injection_mode,
                note: (!mine.is_empty()).then(|| mine.join("; ")),
                id: a.id,
                label: a.label,
            }
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
/// Configuration boundary only; the registry module never selects a home path.
pub(crate) fn registry_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("XNAUT_REGISTRY_DIR") { return Ok(PathBuf::from(dir)); }
    dirs::home_dir().map(|p| p.join(".config/xnaut/registry"))
        .ok_or_else(|| "home directory unavailable for run registry".into())
}

pub(crate) async fn launch_agent_with_env(
    app: AppHandle, state: State<'_, AppState>, req: LaunchAgentRequest,
    identity_env: HashMap<String,String>, launch_identity: Option<AgentLaunchIdentity>,
) -> Result<LaunchAgentResponse,String> {
    use crate::run_control::{self, RunManifest, RunState};
    let dir = registry_dir()?;
    let handle = launch_identity.as_ref().map(|i| i.id.as_str()).unwrap_or(&req.agent_id);
    let run = RunManifest::requested(handle, &req.agent_id, &req.worktree_path,
        req.ticket.clone(), req.model.clone(), run_control::now_ms());
    let live = state.agent_sessions.lock().await.values().filter(|m| status::counts_as_live(m.status)).count();
    let run = run_control::request_in(&dir,run,|| {
        if let Some(ticket_id) = req.ticket.as_deref() {
            let repo = crate::project_management::repo_now()?;
            let ticket = crate::project_management::ticket_list_in(&repo, None)?.into_iter()
                .find(|t| t.id == ticket_id).ok_or("launch ticket not found")?;
            if !run_control::runtime_meets_in(&dir, &req.agent_id, req.model.as_deref().unwrap_or_default(), &ticket.model_requirement)? {
                return Err(format!("launch model does not meet {}", ticket.model_requirement));
            }
        }
        let switches = crate::switches::load();
        if switches.read_only { return Err("read_only kill-switch engaged".into()); }
        if switches.is_quarantined(handle) { return Err(format!("@{handle} is quarantined")); }
        if !req.conversation_mode && !req.resume {
            crate::spend::admit_launch(live)?;
            crate::writer_lease::claim(std::path::Path::new(&req.worktree_path),handle)?;
        }
        Ok(())
    })?;
    let launched = launch_agent_unregistered(app,state,req,identity_env,launch_identity,&dir,&run.run_id).await;
    match launched {
        Ok(mut response) => {
            run_control::update_in(&dir,&run.run_id,|r| {
                r.pty_session = Some(response.session_id.clone());
                r.zellij_session = response.zellij_session.clone();
                r.output_path = response.output_path.clone();
                r.state = RunState::Running;
                r.last_signal = "PTY created; awaiting external liveness proof".into();
            })?;
            response.run_id = Some(run.run_id);
            Ok(response)
        }
        Err(error) => {
            run_control::update_in(&dir,&run.run_id,|r| {
                r.state = RunState::Failed; r.last_signal = format!("launch failed: {error}");
            })?;
            Err(error)
        }
    }
}

async fn launch_agent_unregistered(
    app: AppHandle,
    state: State<'_, AppState>,
    req: LaunchAgentRequest,
    identity_env: HashMap<String, String>,
    launch_identity: Option<AgentLaunchIdentity>,
    registry_dir: &std::path::Path,
    launch_run_id: &str,
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
    // XNAUT-246: every launched agent gets xNAUT's own MCP server, so the
    // ticket, decision and document tools exist for it. Without this an agent
    // has no tool path at all and edits the control repo by hand, which
    // bypasses every rail the tool path enforces.
    // The agent's own session token, minted here rather than at the hook
    // block below, because the MCP config needs it too and is built first.
    // Bound to the real PTY session id once that exists.
    let session_token = uuid::Uuid::new_v4().to_string();
    let xnaut_mcp = {
        let info = state.hook_server.lock().await.clone();
        info.map(|info| {
            (
                info.url.replace("/v1/hook", "/v1/mcp"),
                info.mcp_token.clone(),
                session_token.clone(),
            )
        })
    };
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
            xnaut_mcp.clone(),
        )?
    } else {
        let (mut argv, mut env) = build_launch(&cfg, prompt_ref, req.model.as_deref());
        // The interactive path is what a WAKE uses, and it was getting no
        // plugin or xNAUT config at all.
        let plugins = crate::plugins::active_for(&req.capabilities);
        env.extend(crate::plugins::launch_env(&cfg.id, &plugins));
        argv.extend(crate::plugins::launch_flags(
            &cfg.id,
            &plugins,
            xnaut_mcp
                .as_ref()
                .map(|(url, token, session)| (url.as_str(), token.as_str(), session.as_str())),
        ));
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
    let mut anthropic_via_nautgate = false;
    for (k, v) in &cfg.env {
        let configured_route = configured_nautgate_route(k, v, nautgate.as_ref());
        if configured_route.is_some() && k == "ANTHROPIC_BASE_URL" {
            anthropic_via_nautgate = true;
        }
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
    if cfg.detect_cmd == "claude" && anthropic_via_nautgate {
        let route = nautgate
            .as_ref()
            .ok_or_else(|| "NautGate route disappeared during launch".to_string())?;
        let native_session = conversation_id
            .as_deref()
            .or(req.conversation_id.as_deref())
            .unwrap_or(&launch_run_id);
        let bound_base = register_nautgate_max_launch(
            route,
            &req.worktree_path,
            native_session,
            &launch_run_id,
        )
        .await?;
        extra_env.insert("ANTHROPIC_BASE_URL".into(), bound_base);
    }
    // Pointing Claude Code at a local server is not enough on its own: it still
    // needs an auth source (any non-empty key — the local server ignores it) and
    // a model name that server actually serves, or it asks for a claude-* model
    // nothing there can answer. Both verified against LM Studio.
    // ...but not when NautGate is the route: there the placeholder key would
    // override Claude Code's OAuth token exactly like the bug above.
    if routed_local && !anthropic_via_nautgate && extra_env.contains_key("ANTHROPIC_BASE_URL") {
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
    extra_env.insert("XNAUT_RUN_ID".into(), launch_run_id.to_string());
    if let Some(model) = &req.model { extra_env.insert("XNAUT_MODEL".into(), model.clone()); }

    // Phase 5: if the hook server is live, give the agent the URL + a freshly-minted
    // bearer token so its hook scripts can POST status updates. We can't know the
    // PTY session_id yet (PTY isn't spawned), so use a placeholder and rewrite the
    // token entry after we have the real id. The window is tiny and the server
    // ignores unknown tokens, so any race is harmless.
    let hook_token_placeholder = if let Some(info) = state.hook_server.lock().await.clone() {
        // The SAME token the MCP config carries, so the hook scripts and the
        // agent's tool calls are one identity. Two tokens would mean the tool
        // calls resolve to no session, which is exactly how the ticket rails
        // stopped firing (XNAUT-248).
        let placeholder = session_token.clone();
        // A route, not a base: everything downstream appends to this, so it is
        // normalised once here (XNAUT-183).
        extra_env.insert("XNAUT_HOOK_URL".into(), crate::foundation::hook_base(&info.url));
        // XNAUT-132. Same listener, the one route that can say no. Without
        // this the veto script exits 0 immediately, which is the correct
        // fail-open behaviour but means no policy applies.
        extra_env.insert("XNAUT_VETO_URL".into(), info.url.replace("/v1/hook", "/v1/veto"));
        // The SessionStart brief. Same listener, the one route whose body
        // becomes context rather than being read by the app. Without it
        // xnaut-brief.sh exits 0 on its first line and the agent wakes up
        // knowing nothing — the hook installs, fires, and does nothing.
        extra_env.insert("XNAUT_BRIEF_URL".into(), info.url.replace("/v1/hook", "/v1/brief"));
        extra_env.insert("XNAUT_HOOK_TOKEN".into(), placeholder.clone());
        // The bearer for xNAUT's own MCP server, read by codex through
        // `bearer_token_env_var` (XNAUT-290). Claude gets it inside its
        // --mcp-config file instead; both are the same token.
        extra_env.insert("XNAUT_MCP_KEY".into(), info.mcp_token.clone());
        // The veto script stamps this into the payload: the harness's envelope
        // names the tool but never the caller, so without it a rule cannot be
        // scoped to one agent (XNAUT-132) and two agents on one file cannot be
        // told apart (XNAUT-190).
        if !req.agent_handle.trim().is_empty() {
            extra_env.insert("XNAUT_AGENT_HANDLE".into(), req.agent_handle.trim().to_string());
        }
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
    //
    // XNAUT-242: a cold wake is NEITHER of those stories — not a conversation,
    // not a loom run — and fell between them into the bare-PTY branch, so
    // quitting the app killed the woken fleet mid-ticket. `durable` (default:
    // conversation_mode, every old caller unchanged) lets the wake path opt
    // into the zellij backing without the conversation harness.
    argv = crate::run_control::launch_argv_in(registry_dir, launch_run_id, &argv)?;
    let durable = req.durable.unwrap_or(req.conversation_mode);
    let zellij_run = if (req.conversation_mode || durable) && crate::zellij::is_installed() {
        let identity = launch_identity
            .as_ref()
            .map(|identity| identity.id.clone())
            .unwrap_or_else(|| cfg.id.clone());
        // A per-run suffix. Reusing one name per agent meant the SECOND
        // message attached to the first run's finished session instead of
        // starting anything, and the chat replayed that run's output file
        // from the top — the "mixed up" thread of 2026-08-15.
        let run_id = launch_run_id.to_ascii_lowercase();
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
        // With zellij the layout runs the agent and the PTY hosts zellij; the
        // argv rides along regardless, because pty.rs may find zellij missing
        // at spawn time. Dropping it there produced the ghost the rig caught:
        // a bare `zsh -i -l` registered as "claude · Claude Code · working"
        // with no agent behind it (XNAUT-260).
        command: Some(argv),
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
        zellij_run.as_ref().map(|(_, _, out)| out.clone()),
        zellij_run.as_ref().map(|(name, _, _)| name.clone()),
    )
    .await;

    // Getting the prompt IN FRONT of the agent is only half the job; the other
    // half is submitting it, and which half is missing depends on the mode.
    //
    // StdinAfterStart (pi): nothing is in the composer, so paste it, then
    // press Enter as a separate keystroke.
    //
    // FlagPrompt (claude, `--prefill <text>`): the prompt is ALREADY in the
    // composer from argv, and --prefill deliberately does not submit. Claude
    // Code shows "Pre-filled prompt (N chars) · scroll to review it all
    // before pressing Enter" and waits forever. A woken agent therefore
    // looked idle and unresponsive, and a human pressing Enter in the pane
    // was the only thing that ever started it. So: no paste, just the Enter.
    //
    // Argv (codex) and FlagPromptInteractive run the prompt themselves and
    // are left alone.
    let needs_paste = cfg.prompt_injection_mode == PromptInjectionMode::StdinAfterStart;
    // FlagPrompt used to be listed here too, to synthesise the Enter a
    // prefilled composer waits for. It is not, any more: claude runs its
    // prompt from argv, and a mode whose whole purpose is to wait for a
    // human should not be handed a robot keypress. Anything still on
    // FlagPrompt is a DRAFT, and a draft is meant to sit there.
    let needs_enter = needs_paste;
    if !req.conversation_mode && needs_enter {
        if let Some(prompt) = req.prompt.clone() {
            let session_id_clone = session_id.clone();
            let pty_sessions = state.pty_sessions.clone();
            tokio::spawn(async move {
                // ponytail: a fixed wait for the TUI to render, not a
                // readiness signal. A cold `claude` start is the slow case;
                // 2.5s covers it on this hardware. The ceiling: on a loaded
                // machine the Enter can land before the composer exists, and
                // the agent waits again. Hook-driven readiness is the real
                // answer when this bites.
                tokio::time::sleep(Duration::from_millis(2500)).await;
                if needs_paste {
                    let sessions = pty_sessions.lock().await;
                    if let Some(session) = sessions.get(&session_id_clone) {
                        // Bracketed paste so most TUI agents accept the multi-line prompt as one unit.
                        let payload = format!("\x1b[200~{}\x1b[201~", prompt);
                        if let Ok(mut w) = session.writer.lock() {
                            let _ = w.write_all(payload.as_bytes());
                            let _ = w.flush();
                        }
                    }
                }
                // Enter is always its own keystroke: inside a bracketed
                // paste it is swallowed with the paste, and after --prefill
                // there is nothing else to send.
                if needs_paste {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                }
                {
                    let sessions = pty_sessions.lock().await;
                    if let Some(session) = sessions.get(&session_id_clone) {
                        if let Ok(mut w) = session.writer.lock() {
                            let _ = w.write_all(b"\r");
                            let _ = w.flush();
                        }
                    }
                }
            });
        }
    }

    Ok(LaunchAgentResponse {
        run_id: Some(launch_run_id.to_string()),
        session_id,
        agent_id: launched_agent_id,
        injection_mode: cfg.prompt_injection_mode,
        conversation_id,
        output_path: zellij_run.as_ref().map(|(_, _, out)| out.clone()),
        zellij_session: zellij_run.as_ref().map(|(name, _, _)| name.clone()),
    })
}


#[derive(Debug, Serialize)]
pub struct RunOutput {
    pub text: String,
    pub next_offset: u64,
    pub finished: bool,
    /// The tail of the run's stderr. A CLI that refuses to start says why
    /// HERE and nowhere else — "Not inside a trusted directory" sat in this
    /// file while the chat showed "the run finished without a conversational
    /// response. Open Terminal." Nobody opens the terminal.
    #[serde(default)]
    pub error_tail: String,
}

/// Last few lines of the run's stderr, if it wrote any.
fn error_tail_for(path: &str) -> String {
    let err_path = path.strip_suffix(".jsonl").map(|base| format!("{base}.err"));
    let Some(err_path) = err_path else {
        return String::new();
    };
    let Ok(text) = std::fs::read_to_string(&err_path) else {
        return String::new();
    };
    let tail: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(6)
        .collect();
    tail.into_iter().rev().collect::<Vec<_>>().join("\n")
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
                error_tail: String::new(),
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
        error_tail: error_tail_for(&path),
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

/// Reset one runtime to this build's defaults. Returns the path of the copy
/// kept of the previous registry, so the caller can say where the way back is;
/// `None` only when there was no file to copy.
#[tauri::command]
pub fn agent_registry_rollback(agent_id: String) -> Result<Option<String>, String> {
    Ok(rollback_agent(&agent_id)?.map(|p| p.display().to_string()))
}

/// The last reconciliation's findings, for a surface that wants to show what
/// this machine's registry differs on without re-running a load.
#[tauri::command]
pub fn agent_registry_drift() -> Option<DriftRecord> {
    drift_record()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {

    /// The guard must refuse the owner's real registry, whatever else is going
    /// on in the process. That is the exact shape that rewrote it: one thread
    /// holding the real path while another had set the redirect.
    ///
    /// This test deliberately sets no environment variable. The old guard read
    /// one, which is why it raced, and a test that mutated it here would break
    /// every parallel test that depends on it. It did, once, before this
    /// comment existed.
    #[test]
    fn the_write_guard_refuses_the_real_registry_and_admits_a_redirect() {
        let real = real_config_path();
        assert!(
            !writes_allowed(&real),
            "a test was allowed to write the owner's real registry at {}",
            real.display()
        );
        assert!(
            writes_allowed(std::path::Path::new("/tmp/xnaut-guard-probe.toml")),
            "a redirected path is exactly what a test is allowed to write"
        );
    }

    use super::*;

    /// A launch binding is minted per launch, held in NautGate's memory, and
    /// expires with a fixed TTL. There is no renewal xNAUT could perform:
    /// re-registering mints a different URL and the running process's env is
    /// already set. So the only thing this side can do is notice the 401 and
    /// stop the run, the way it already stops on the Max-guard pause. Without
    /// that, a session that outlives its binding (six hours, or one NautGate
    /// restart) retries into a wall with nothing on screen to say why.
    #[test]
    fn a_dead_launch_binding_stops_the_run_like_a_guard_pause() {
        let space = include_str!("../../src/js/agent-space.js");
        for needle in ["nautgate_max_guard_paused", "invalid_or_expired_max_launch"] {
            assert!(
                space.contains(needle),
                "agent-space.js stopped watching for {needle}, so a run that hits it \
                 keeps retrying against a route that will never answer"
            );
        }
    }

    /// Launching an agent into a worktree is real work, so the run has to
    /// outlive the app. `durable` defaults to `conversation_mode`, false here,
    /// so leaving the field unset is not neutral: it put every launch from the
    /// worktree manager in a bare PTY owned by the app. The rig quit xNAUT and
    /// all three worktree agents were gone inside ten seconds while every
    /// zellij-backed session survived (XNAUT-262).
    ///
    /// The second half asserts the default itself, because that is what makes
    /// the call site load-bearing rather than decorative.
    #[test]
    fn a_worktree_manager_launch_outlives_the_app() {
        let js = include_str!("../../src/js/worktree.js");
        let after = js
            .split_once("'agent_launch'")
            .expect("worktree.js no longer launches an agent")
            .1;
        let req = &after[..after
            .find("});")
            .expect("worktree.js agent_launch call is unterminated")];
        assert!(
            req.contains("durable: true"),
            "worktree.js launches its agent without durable:true, so the run is a \
             bare PTY owned by the app and quitting kills the work mid-task"
        );

        let bare: LaunchAgentRequest = serde_json::from_value(serde_json::json!({
            "agent_id": "claude",
            "worktree_path": "/tmp/wt",
            "prompt": null,
            "cols": 120,
            "rows": 30,
        }))
        .expect("the launch request shape changed");
        assert!(
            !bare.durable.unwrap_or(bare.conversation_mode),
            "an omitted durable no longer means non-durable, so the assertion above \
             can no longer tell a durable launch from a bare one"
        );
    }

    /// The veto flags ride in front of every codex launch (XNAUT-132) and are
    /// asserted by their own test. Argv tests about everything else drop them
    /// rather than restating them, so a change to the policy plumbing does not
    /// have to be pasted into every expectation.
    fn strip_veto(argv: &[String]) -> Vec<String> {
        let veto = codex_veto_flags();
        // The prefix sits AFTER the binary name, so argv[0] stays.
        if veto.is_empty() || argv.len() <= veto.len() || !argv[1..].starts_with(&veto[..]) {
            return argv.to_vec();
        }
        let mut out = vec![argv[0].clone()];
        out.extend(argv[1 + veto.len()..].iter().cloned());
        out
    }

    /// Launch a real CLI with the argv we would really use, and read its exit.
    #[cfg(test)]
    fn run_argv_for_test(argv: &[String], cwd: &std::path::Path) -> (bool, String) {
        use std::process::Command;
        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(cwd)
            .env("PATH", runtime_path().unwrap_or_else(|| std::env::var("PATH").unwrap_or_default()))
            .output();
        match output {
            Ok(out) => (
                out.status.success(),
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ),
            ),
            Err(error) => (false, error.to_string()),
        }
    }

    #[test]
    #[ignore = "runs the real codex CLI twice; run with --ignored"]
    fn a_codex_build_thread_survives_its_second_turn() {
        // "It should always be like me opening a CC session." The first turn
        // worked and every follow-up died instantly with
        // "unexpected argument '--approve-for-me'", because `codex exec resume`
        // takes a different argument set. Composition is not the proof; the
        // CLI accepting both argvs is.
        let mut runtime = cfg(PromptInjectionMode::Argv, None, None);
        runtime.id = "codex".into();
        runtime.launch_cmd = "codex".into();
        runtime.extra_args.clear();
        let dir = std::env::temp_dir().join(format!("xnaut-resume-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let policy = crate::policy::AgentPolicy::default();

        let (first, _, id) = build_conversation_launch(
            &runtime, "Reply with the single word OK and stop.", None, None, None, false, Some(&policy), &[],
            None,
        )
        .unwrap();
        let (ok, output) = run_argv_for_test(&first, &dir);
        assert!(ok, "the FIRST turn failed: {output}");

        // codex reports the session id in its own output; take it from there
        // rather than trusting our guess.
        let session = output
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .find(|word| word.len() == 36 && word.matches('-').count() == 4)
            .map(str::to_string)
            .or(id)
            .expect("a session id to resume");

        let (second, _, _) = build_conversation_launch(
            &runtime, "Reply with the single word AGAIN and stop.", None, None, Some(&session), true, Some(&policy), &[],
            None,
        )
        .unwrap();
        let (resumed, resume_output) = run_argv_for_test(&second, &dir);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            !resume_output.contains("unexpected argument"),
            "resume was handed a flag it rejects: {resume_output}"
        );
        assert!(resumed, "the SECOND turn failed: {resume_output}");
    }

    #[test]
    #[ignore = "runs the real claude CLI twice; run with --ignored"]
    fn a_claude_build_thread_survives_its_second_turn() {
        // The other runtime, checked the same way: composition proves nothing,
        // the CLI accepting both argvs does.
        let mut runtime = cfg(PromptInjectionMode::FlagPrompt, None, None);
        runtime.id = "claude".into();
        runtime.launch_cmd = "claude".into();
        runtime.extra_args = vec!["--dangerously-skip-permissions".into()];
        let dir = std::env::temp_dir().join(format!("xnaut-resume-claude-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let policy = crate::policy::AgentPolicy::default();

        let (first, _, id) = build_conversation_launch(
            &runtime, "Reply with the single word OK and stop.", None, None, None, false, Some(&policy), &[],
            None,
        )
        .unwrap();
        let (ok, output) = run_argv_for_test(&first, &dir);
        assert!(ok, "the FIRST turn failed: {output}");
        let session = id.expect("claude is given its session id");

        let (second, _, _) = build_conversation_launch(
            &runtime, "Reply with the single word AGAIN and stop.", None, None, Some(&session), true, Some(&policy), &[],
            None,
        )
        .unwrap();
        let (resumed, resume_output) = run_argv_for_test(&second, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            !resume_output.contains("unexpected argument") && !resume_output.contains("Unknown option"),
            "resume was handed a flag it rejects: {resume_output}"
        );
        assert!(resumed, "the SECOND turn failed: {resume_output}");
    }

    #[test]
    fn a_run_that_fails_hands_back_what_the_cli_actually_said() {
        // The real one: codex refused to start with "Not inside a trusted
        // directory and --skip-git-repo-check was not specified", and the
        // chat answered "the run finished without a conversational response.
        // Open Terminal to see what it did." Nobody opens the terminal.
        let dir = std::env::temp_dir().join(format!("xnaut-runout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("run.jsonl");
        std::fs::write(&out, "[xnaut] run finished\n").unwrap();
        std::fs::write(
            dir.join("run.err"),
            "some noise\nNot inside a trusted directory and --skip-git-repo-check was not specified.\n",
        )
        .unwrap();

        let read = agent_run_output(out.to_string_lossy().into_owned(), 0).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(read.finished);
        assert!(
            read.error_tail.contains("--skip-git-repo-check"),
            "the reason never reached the caller: {:?}",
            read.error_tail
        );
    }

    #[test]
    fn codex_is_allowed_to_run_outside_a_git_repository() {
        // An explicit non-repository project is still allowed, and without the
        // flag Codex stops before it starts. The sandbox policy bounds the run.
        let mut runtime = cfg(PromptInjectionMode::Argv, None, None);
        runtime.id = "codex".into();
        runtime.launch_cmd = "codex".into();
        runtime.extra_args.clear();
        let (argv, _, _) = build_conversation_launch(
            &runtime,
            "draw me a diagram",
            None,
            None,
            None,
            false,
            None,
            &[],
            None,
        )
        .unwrap();
        assert!(argv.contains(&"--skip-git-repo-check".to_string()), "{argv:?}");
    }

    #[test]
    fn a_run_script_captures_the_tty_into_the_tailed_file_and_marks_its_end() {
        // Three generations of wrong in production: the marker printed to the
        // zellij pane (a finished run polled for 30 minutes), the session held
        // open by a `read` (the NEXT message attached to it and never ran),
        // and a tee pipeline a TUI never writes to (every cold run's file
        // stayed at 0 bytes while the agent visibly worked, 2026-08-31).
        // script(1) gives the command a tty and captures ALL of it. Assert
        // the script, not the intention.
        let (name, layout, out) = prepare_zellij_run(
            "xnaut-selftest-script",
            "/tmp",
            &["claude".to_string(), "--print".to_string(), "hello world".to_string()],
            &std::collections::HashMap::from([("TOKEN".to_string(), "s3cret".to_string())]),
        )
        .expect("prepare");
        let script = run_dir().unwrap().join(format!("{name}.sh"));
        let text = std::fs::read_to_string(&script).expect("script");
        assert!(
            text.contains(&format!("script -q '{out}'")) && text.contains(&format!("script -q -c")),
            "the tty is not captured into the tailed file on both platforms: {text}"
        );
        assert!(!text.contains("| tee"), "the tee pipeline is back; a TUI never writes to it");
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
    fn a_configured_but_dead_nautgate_is_not_a_route() {
        // The seeded registry points every claude agent at localhost:8090. A
        // machine with NautGate in settings but not running must fall through
        // to the agent's own subscription, not be handed a dead gateway.
        let dead = crate::settings::LlmSettings {
            endpoint: "http://localhost:1".into(),
            ..Default::default()
        };
        assert_eq!(
            configured_nautgate_route("ANTHROPIC_BASE_URL", "http://localhost:8090", Some(&dead)),
            None
        );
        let live = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let alive = crate::settings::LlmSettings {
            endpoint: format!("http://{}/v1", live.local_addr().unwrap()),
            ..Default::default()
        };
        assert!(
            configured_nautgate_route("ANTHROPIC_BASE_URL", "http://localhost:8090", Some(&alive))
                .is_some()
        );
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
    fn configured_nautgate_route_preserves_claude_oauth_and_supplies_openai_token() {
        // The route checks liveness. A fixture listener makes that evidence
        // local to this test instead of depending on the owner's gateway.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
        let route = crate::settings::LlmSettings {
            provider: "nautgate".into(),
            endpoint: endpoint.clone(),
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
        assert_eq!(claude.0, anthropic_base(&endpoint));
        // No credential: Claude Code's own Max OAuth token has to survive, or
        // the gateway misses the subscription lane and bills the metered key.
        assert_eq!(claude.1, None);

        let openai = configured_nautgate_route(
            "OPENAI_BASE_URL",
            "http://localhost:8090/v1",
            Some(&route),
        )
        .unwrap();
        assert_eq!(openai.0, endpoint);
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

    /// The translation four call sites each had their own copy of. "local" and
    /// "" are the CLI's default model, NOT a model called local: the local-LLM
    /// path passes its endpoint as env because Claude Code has no flag for it.
    #[test]
    fn a_model_string_names_its_runtime() {
        assert_eq!(runtime_for_model("codex"), ("codex", None));
        assert_eq!(
            runtime_for_model("codex:gpt-5"),
            ("codex", Some("gpt-5".into()))
        );
        assert_eq!(runtime_for_model("pi"), ("pi", None));
        assert_eq!(
            runtime_for_model("claude-opus-5"),
            ("claude", Some("claude-opus-5".into()))
        );
        assert_eq!(runtime_for_model(""), ("claude", None));
        assert_eq!(runtime_for_model("local"), ("claude", None));
    }

    /// ONE answer to "how do I run this agent unattended", so a runtime cannot
    /// be launched one way by the fleet and another by a loom.
    ///
    /// Guards what the four copies had drifted apart on. The swarm pane sent
    /// `--allow-dangerously-skip-permissions` while the other three sent
    /// `--dangerously-skip-permissions`; both are real Claude Code flags, so
    /// nothing was broken — but four copies drifting on a permissions flag with
    /// nobody noticing is exactly the failure mode, and the next drift would not
    /// be harmless. One spelling now, asserted rather than assumed.
    #[test]
    fn one_place_builds_a_headless_agent_line() {
        // `load_or_seed_registry` reads XNAUT_AGENTS_PATH, which is
        // process-global; without this lock a neighbouring test's deliberately
        // unparseable registry becomes this test's failure.
        let (_guard, _path) = scratch_registry("headless-one-place");
        let claude = headless_command(&Headless {
            model: "claude-opus-5",
            goal_file: ".loom-goal.txt",
            resume: Some("abc-123"),
            isolate_mcp: true,
        })
        .unwrap();
        assert!(claude.contains(" --dangerously-skip-permissions "), "{claude}");
        assert!(!claude.contains("--allow-dangerously-skip-permissions"), "{claude}");
        assert!(claude.contains("--output-format stream-json"), "{claude}");
        assert!(claude.contains("--model 'claude-opus-5'"), "{claude}");
        assert!(claude.contains("--resume abc-123"), "{claude}");
        assert!(claude.contains("--strict-mcp-config"), "{claude}");
        assert!(claude.contains("\"$(cat '.loom-goal.txt')\""), "{claude}");

        // codex prints plain text and takes neither of the claude-only knobs.
        let codex = headless_command(&Headless {
            model: "codex",
            goal_file: ".loom-goal.txt",
            resume: Some("abc-123"),
            isolate_mcp: true,
        })
        .unwrap();
        assert!(codex.starts_with("codex exec --dangerously-bypass-approvals-and-sandbox"), "{codex}");
        assert!(!codex.contains("--resume"), "{codex}");
        assert!(!codex.contains("stream-json"), "{codex}");

        // A default run asks for neither.
        let plain = headless_command(&Headless {
            model: "",
            goal_file: ".loom-goal.txt",
            ..Default::default()
        })
        .unwrap();
        assert!(!plain.contains("--model"), "{plain}");
        assert!(!plain.contains("--resume"), "{plain}");
        assert!(!plain.contains("--strict-mcp-config"), "{plain}");
    }

    /// A session id is an id. Anything else in it would be shell, and this is
    /// now the ONE place that has to get that right rather than four.
    #[test]
    fn a_resume_id_cannot_carry_shell() {
        let (_guard, _path) = scratch_registry("headless-resume");
        let line = headless_command(&Headless {
            model: "",
            goal_file: "g.txt",
            resume: Some("abc; rm -rf /"),
            isolate_mcp: false,
        })
        .unwrap();
        // `-` survives because ids contain it; the semicolon, the spaces and
        // the slash — everything that could BE shell — do not.
        assert!(line.contains("--resume abcrm-rf "), "{line}");
        assert!(!line.contains("rm -rf /"), "{line}");
        assert!(!line.contains(';'), "{line}");
    }

    /// The line this builds is SHELL, so reading it proves nothing: single
    /// quotes make `\"` a literal backslash, and an over-escaped version would
    /// hand Claude Code `{\"disableAllHooks\":true}` — rejected as malformed
    /// JSON at the far end of a headless run, where nobody is watching. So this
    /// runs the line through bash and inspects the argv the agent would
    /// actually receive.
    ///
    /// This is `tests/headless-profile.spec.mjs` moved to where the code now
    /// lives (XNAUT-266). That spec evaluated four hand-built command
    /// expressions out of three JavaScript files and one bash heredoc, because
    /// there were four copies to guard; there is one now, so the guard belongs
    /// beside it, and the four rows below are the four callers' option
    /// combinations rather than four implementations.
    fn argv_of(line: &str, veto_url: &str, hook_token: &str) -> Vec<String> {
        let dir = std::env::temp_dir().join(format!("xnaut-headless-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let goal = "A goal with \"quotes\" and $literal text";
        for name in [".loom-goal.txt", ".build-goal.txt"] {
            std::fs::write(dir.join(name), goal).unwrap();
        }
        // Shadow every agent binary with a function that prints its own argv
        // NUL-separated, so nothing actually launches and nothing is lost to
        // word splitting on the way back.
        // Each shadow is self-contained rather than delegating to a shared
        // helper: NautLoom's runner `export -f`s the agent function into a
        // NESTED bash, and a helper it called would not travel with it.
        let capture = "claude() { printf '%s\\0' \"$@\"; }\n\
                       codex() { printf '%s\\0' \"$@\"; }\n\
                       pi() { printf '%s\\0' \"$@\"; }\n";
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!("{capture}{line}"))
            .current_dir(&dir)
            .env("XNAUT_VETO_URL", veto_url)
            .env("XNAUT_HOOK_TOKEN", hook_token)
            .output()
            .expect("bash");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            out.status.success(),
            "the command line did not parse: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|word| !word.is_empty())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn every_caller_gets_an_argv_the_agent_can_actually_parse() {
        let (_guard, _path) = scratch_registry("headless-argv");
        let goal = "A goal with \"quotes\" and $literal text";
        // The four callers, by the options they ask for.
        let callers: [(&str, Headless, bool); 4] = [
            (
                "NautFlow persona",
                Headless {
                    model: "test-model",
                    goal_file: ".loom-goal.txt",
                    resume: Some("test-session"),
                    isolate_mcp: true,
                },
                true,
            ),
            (
                "Designer",
                Headless {
                    model: "test-model",
                    goal_file: ".loom-goal.txt",
                    resume: Some("test-session"),
                    isolate_mcp: true,
                },
                true,
            ),
            (
                "build planner",
                Headless {
                    model: "",
                    goal_file: ".loom-goal.txt",
                    resume: None,
                    isolate_mcp: true,
                },
                false,
            ),
            (
                "multiagent worker",
                Headless {
                    model: "",
                    goal_file: ".build-goal.txt",
                    resume: None,
                    isolate_mcp: false,
                },
                false,
            ),
        ];
        for (name, spec, expects_model) in callers {
            let isolated = spec.isolate_mcp;
            let resumes = spec.resume.is_some();
            let line = headless_command(&spec).unwrap();
            // Unattended, then the two ways a run is MANAGED (XNAUT-107).
            for (veto, token) in [("", ""), ("http://fixture.invalid/veto", ""), ("", "tok")] {
                let args = argv_of(&line, veto, token);
                assert!(args.contains(&"-p".to_string()), "{name}: {args:?}");
                assert_eq!(args.last().map(String::as_str), Some(goal), "{name}");

                let settings = args
                    .iter()
                    .position(|a| a == "--settings")
                    .map(|i| args[i + 1].clone())
                    .unwrap_or_else(|| panic!("{name} passed no --settings"));
                let parsed: serde_json::Value = serde_json::from_str(&settings)
                    .unwrap_or_else(|e| panic!("{name} settings are not JSON ({e}): {settings}"));
                let managed = !veto.is_empty() || !token.is_empty();
                assert_eq!(
                    parsed,
                    if managed {
                        serde_json::json!({})
                    } else {
                        serde_json::json!({ "disableAllHooks": true })
                    },
                    "{name} under veto={veto:?} token={token:?}"
                );

                assert_eq!(
                    args.contains(&"--strict-mcp-config".to_string()),
                    isolated,
                    "{name}"
                );
                if isolated {
                    let i = args.iter().position(|a| a == "--mcp-config").unwrap();
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&args[i + 1]).unwrap(),
                        serde_json::json!({ "mcpServers": {} }),
                        "{name}"
                    );
                }
                let has = |flag: &str| args.iter().any(|a| a == flag);
                assert_eq!(has("--model"), expects_model, "{name}");
                assert_eq!(has("--resume"), resumes, "{name}");
                if expects_model {
                    let i = args.iter().position(|a| a == "--model").unwrap();
                    assert_eq!(args[i + 1], "test-model", "{name}");
                }
                if resumes {
                    let i = args.iter().position(|a| a == "--resume").unwrap();
                    assert_eq!(args[i + 1], "test-session", "{name}");
                }
            }
        }
    }

    /// The NautLoom sandbox runner used to build its own line in bash and its
    /// own second layer of quoting on top; the spec that guarded it had to
    /// execute a `case` statement scraped out of a Rust string literal. The
    /// line now arrives staged, so what is left to prove is that it survives
    /// the runner's nested shell — `tmux new-session "... $AGENT ..."` puts it
    /// through bash a second time.
    #[test]
    fn a_staged_line_survives_nautlooms_nested_shell() {
        let (_guard, _path) = scratch_registry("headless-nested");
        let line = headless_command(&Headless {
            model: "test-model",
            goal_file: ".loom-goal.txt",
            ..Default::default()
        })
        .unwrap();
        let staged = std::env::temp_dir().join(format!("xnaut-nested-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&staged, &line).unwrap();
        // What the runner does: read the line, then run it through one more
        // shell, exactly as `tmux new-session "cd /workspace && $AGENT"` does.
        let nested = format!(
            "AGENT=\"$(cat {})\"\nexport -f claude\nbash -c \"$AGENT\"",
            crate::agents::shell_quote(&staged.to_string_lossy())
        );
        let args = argv_of(&nested, "", "");
        let _ = std::fs::remove_file(&staged);
        let i = args.iter().position(|a| a == "--settings").unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&args[i + 1]).unwrap(),
            serde_json::json!({ "disableAllHooks": true }),
            "{args:?}"
        );
        let m = args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(args[m + 1], "test-model");
        assert!(!args.contains(&"--strict-mcp-config".to_string()), "{args:?}");
        assert_eq!(
            args.last().map(String::as_str),
            Some("A goal with \"quotes\" and $literal text")
        );
    }

    /// Headless work isolates its hooks; an INTERACTIVE launch must never.
    ///
    /// The spec used to assert this by grepping agents.rs for `disableAllHooks`
    /// at all, which stopped being the right question the moment the headless
    /// builder moved into this file. The question was always about
    /// `build_launch`, so it is asked of `build_launch`.
    #[test]
    fn an_interactive_launch_never_disables_hooks() {
        for mode in [
            PromptInjectionMode::Argv,
            PromptInjectionMode::FlagPrompt,
            PromptInjectionMode::FlagPromptInteractive,
            PromptInjectionMode::FlagInteractive,
            PromptInjectionMode::StdinAfterStart,
        ] {
            let (argv, env) = build_launch(&cfg(mode, Some("-p"), None), Some("hello"), None);
            let all = format!("{} {:?}", argv.join(" "), env);
            assert!(!all.contains("disableAllHooks"), "{all}");
            assert!(!all.contains("--settings"), "{all}");
        }
    }

    #[test]
    fn build_launch_argv_mode_appends_prompt_at_end() {
        let (argv, env) = build_launch(&cfg(PromptInjectionMode::Argv, None, None), Some("hello"), None);
        assert_eq!(argv, vec!["test", "chat", "hello"]);
        assert!(env.is_empty());
    }

    /// Points `XNAUT_AGENTS_PATH` at a fresh scratch file and holds a
    /// process-wide lock while it does. The path comes from an env var, which
    /// is process-global, and reconciliation WRITES: without the lock two of
    /// these tests would reconcile each other's file, and without the redirect
    /// they would reconcile the owner's real runtimes.
    fn scratch_registry(name: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        let guard = LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let path =
            std::env::temp_dir().join(format!("xnaut-agents-{}-{name}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("XNAUT_AGENTS_PATH", &path);
        (guard, path)
    }

    /// The kept copies of one registry, oldest first. Matched by the live
    /// file's own name so two tests sharing a temp dir cannot see each other's
    /// history.
    fn versions_of(path: &Path) -> Vec<PathBuf> {
        let prefix = format!(
            "{}.",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        let mut found: Vec<PathBuf> = path
            .parent()
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                    .filter(|candidate| {
                        let name = candidate
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        name.starts_with(&prefix) && name.ends_with(".bak")
                    })
                    .collect()
            })
            .unwrap_or_default();
        found.sort();
        found
    }

    /// A pid can be reused between suite runs, so start from no history.
    fn clear_versions(path: &Path) {
        for old in versions_of(path) {
            let _ = std::fs::remove_file(old);
        }
        let _ = std::fs::remove_file(drift_record_path(path));
    }

    /// A registry already on this build's revision, so loading it neither
    /// heals nor appends. Any kept version a test then sees was made by the
    /// write it was actually testing.
    fn settled_registry(edit: impl Fn(&mut AgentConfig)) -> String {
        let mut registry = default_registry();
        registry.agents.iter_mut().for_each(edit);
        format!(
            "{REGISTRY_HEADER}\n{}",
            toml::to_string_pretty(&registry).unwrap()
        )
    }

    #[test]
    fn a_registry_write_keeps_the_file_it_replaced() {
        // XNAUT-278. A full write is a serde round-trip: comments, ordering
        // and any field this build no longer knows about are gone the moment
        // it lands. Without a copy the only record of what the machine used to
        // say is whatever anyone happened to remember.
        let (_guard, path) = scratch_registry("keeps-previous");
        clear_versions(&path);
        let original = settled_registry(|agent| {
            if agent.id == "claude" {
                agent.prompt_injection_mode = PromptInjectionMode::FlagPrompt;
            }
        });
        std::fs::write(&path, &original).unwrap();

        rollback_agent("claude").unwrap();

        // The live file really did change, or this proves nothing.
        let live = std::fs::read_to_string(&path).unwrap();
        assert!(
            live.contains(r#"prompt_injection_mode = "argv""#),
            "the write under test did not happen:\n{live}"
        );

        let kept = versions_of(&path);
        assert_eq!(
            kept.len(),
            1,
            "one write should leave exactly one copy, found {kept:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&kept[0]).unwrap(),
            original,
            "the copy is only worth having if it is the file byte for byte"
        );
    }

    #[test]
    fn a_rollback_restores_one_runtime_and_touches_no_other() {
        // Scoped on purpose. Someone reaches for this because ONE runtime
        // misbehaves, and a reset that also took away their pinned model or
        // their own launch command would cost more than the drift did.
        let (_guard, path) = scratch_registry("rollback-one");
        clear_versions(&path);
        std::fs::write(
            &path,
            settled_registry(|agent| match agent.id.as_str() {
                "claude" => {
                    agent.prompt_injection_mode = PromptInjectionMode::FlagPrompt;
                    agent.extra_args = vec![];
                    agent.env = HashMap::new();
                }
                "pi" => {
                    agent.launch_cmd = "my-own-pi".into();
                    agent.extra_args = vec!["--mine".into()];
                }
                _ => {}
            }),
        )
        .unwrap();

        rollback_agent("claude").unwrap();

        let after: AgentRegistry =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let seeded = default_registry();
        let default_claude = seeded.find("claude").unwrap();
        let claude = after.find("claude").expect("claude survives a rollback");
        assert_eq!(
            claude.prompt_injection_mode, default_claude.prompt_injection_mode,
            "the injection mode is the field XNAUT-278 exists for"
        );
        assert_eq!(claude.extra_args, default_claude.extra_args);
        assert_eq!(claude.env, default_claude.env);

        let pi = after
            .find("pi")
            .expect("pi survives someone else's rollback");
        assert_eq!(
            pi.launch_cmd, "my-own-pi",
            "rolling back claude took another runtime's edits with it"
        );
        assert_eq!(pi.extra_args, vec!["--mine"]);
    }

    #[test]
    fn the_drift_record_outlives_the_load_that_found_it() {
        // The gap XNAUT-278 closes. Reconciliation already knew the file said
        // one thing and the build another; it said so to a tooltip and a log
        // line, both of which are gone by the time anyone asks why this
        // machine behaves differently.
        let (_guard, path) = scratch_registry("drift-record");
        clear_versions(&path);
        std::fs::write(
            &path,
            settled_registry(|agent| {
                if agent.id == "claude" {
                    agent.launch_cmd = "claude-nightly".into();
                }
            }),
        )
        .unwrap();

        let loaded = load_registry().unwrap();
        assert!(
            loaded
                .notes
                .iter()
                .any(|note| note.message.contains("launch_cmd")),
            "nothing drifted, so the record proves nothing: {:?}",
            loaded.notes
        );
        // Everything the load learned, gone. This is the state a restart is in.
        drop(loaded);

        let record = drift_record().expect("the drift was written down");
        assert_eq!(record.seed_revision, SEED_REVISION);
        assert_eq!(record.registry, path.display().to_string());
        assert!(
            record
                .notes
                .iter()
                .any(|note| note.agent_id == "claude" && note.message.contains("claude-nightly")),
            "a record has to carry the value, not just the field name: {:?}",
            record.notes
        );
        // And reading it again does not depend on the load that produced it.
        assert_eq!(drift_record().unwrap(), record);
    }

    /// The shape of the file this bug was found on: seeded 2026-06-10, five
    /// runtimes, `extra_args = []` on every one of them, and no `pi`.
    const REGISTRY_AS_SEEDED_2026_06_10: &str = r#"
[[agents]]
id = "claude"
label = "Claude Code"
detect_cmd = "claude"
launch_cmd = "claude"
extra_args = []
expected_process = "claude"
prompt_injection_mode = "flag-prompt"
draft_prompt_flag = "--prefill"

[[agents]]
id = "codex"
label = "Codex"
detect_cmd = "codex"
launch_cmd = "codex"
extra_args = []
expected_process = "codex"
prompt_injection_mode = "argv"
preflight_trust = "codex"
"#;

    #[test]
    fn a_users_own_edit_to_an_existing_entry_survives_a_merge_that_adds_a_runtime() {
        // The load-bearing one. A person who pinned their own launch flags is
        // not asking for this build's opinion of them, and the merge that
        // teaches xNAUT about `pi` must not be the thing that quietly takes
        // their flags away.
        let (_guard, path) = scratch_registry("user-edit-survives");
        std::fs::write(
            &path,
            format!(
                "seed_revision = {SEED_REVISION}\n{}",
                r#"
[[agents]]
id = "claude"
label = "Claude, my way"
detect_cmd = "claude"
launch_cmd = "claude"
extra_args = ["--model", "opus"]
expected_process = "claude"
prompt_injection_mode = "argv"
"#
            ),
        )
        .unwrap();

        let loaded = load_registry().unwrap();
        let claude = loaded.registry.find("claude").expect("claude survives");
        assert_eq!(claude.extra_args, vec!["--model", "opus"]);
        assert_eq!(claude.label, "Claude, my way");
        assert!(
            claude.env.is_empty(),
            "an entry the user owns keeps the env they gave it, not the seed's"
        );
        // ...and the merge really did add something, or this proves nothing.
        assert!(
            loaded.registry.find("pi").is_some(),
            "the new runtime still has to arrive"
        );

        // Same again after the write, because the file is what the next launch
        // reads.
        let on_disk = std::fs::read_to_string(&path).unwrap();
        let reread: AgentRegistry = toml::from_str(&on_disk).unwrap();
        assert_eq!(
            reread.find("claude").unwrap().extra_args,
            vec!["--model", "opus"],
            "reconciliation wrote the seed's flags over the user's:\n{on_disk}"
        );
        assert!(reread.find("pi").is_some(), "{on_disk}");
    }

    #[test]
    fn a_runtime_this_build_learned_reaches_a_file_that_predates_it() {
        let (_guard, path) = scratch_registry("new-runtime-arrives");
        std::fs::write(&path, REGISTRY_AS_SEEDED_2026_06_10).unwrap();

        let loaded = load_registry().unwrap();
        for id in ["gemini", "grok", "opencode", "pi"] {
            assert!(
                loaded.registry.find(id).is_some(),
                "{id} never reached the registry"
            );
        }
        let on_disk: AgentRegistry =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            on_disk.find("pi").is_some(),
            "a merge only held in memory leaves the file lying about the machine"
        );
        assert!(loaded
            .notes
            .iter()
            .any(|n| n.agent_id == "pi" && n.message.contains("added")));
    }

    #[test]
    fn a_difference_between_the_file_and_this_build_is_reported_not_applied() {
        // The user turned the dangerous flag off on purpose. It stays off, and
        // it stays visible: silently skipping the default is the seed-once bug
        // again, just later in the pipeline.
        let (_guard, _path) = scratch_registry("drift-is-reported");
        let claude = default_registry()
            .agents
            .into_iter()
            .find(|a| a.id == "claude")
            .unwrap();
        let (merged, notes, _) = reconcile(AgentRegistry {
            seed_revision: SEED_REVISION,
            agents: vec![AgentConfig {
                extra_args: vec![],
                ..claude
            }],
        });
        assert!(
            merged.find("claude").unwrap().extra_args.is_empty(),
            "a reconciled file's values are the user's, not the seed's"
        );
        let note = notes
            .iter()
            .find(|n| n.agent_id == "claude" && n.message.contains("extra_args"))
            .unwrap_or_else(|| panic!("no note about extra_args in {notes:?}"));
        assert!(
            note.message.contains("--dangerously-skip-permissions"),
            "a note has to name the value the user is missing: {}",
            note.message
        );
    }

    #[test]
    fn the_one_time_heal_repairs_a_stale_file_and_then_never_fires_again() {
        // The 2026-06-10 file has no record of what was seeded into it, so the
        // heal has to guess once. What must not happen is guessing forever:
        // after the revision is written, clearing a field by hand has to stick.
        let (_guard, path) = scratch_registry("heal-once");
        std::fs::write(&path, REGISTRY_AS_SEEDED_2026_06_10).unwrap();

        let healed = load_registry().unwrap();
        let claude = healed.registry.find("claude").unwrap();
        assert_eq!(
            claude.extra_args,
            vec!["--dangerously-skip-permissions"],
            "the heal is the whole point (XNAUT-182)"
        );
        assert_eq!(claude.prompt_injection_mode, PromptInjectionMode::Argv);
        assert!(healed
            .notes
            .iter()
            .any(|n| n.agent_id == "claude" && n.message.contains("one-time repair")));

        // Now the owner takes that flag back off, on the healed file.
        let body = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            &path,
            body.replace(r#"extra_args = ["--dangerously-skip-permissions"]"#, "extra_args = []"),
        )
        .unwrap();

        let second = load_registry().unwrap();
        assert!(
            second.registry.find("claude").unwrap().extra_args.is_empty(),
            "the heal fired twice and took back a flag its owner removed"
        );
    }

    /// Revision 2. A file already healed once (revision 1, claude carrying its
    /// flag) still has an empty codex entry, and that is the file NautBot was
    /// launched from on 2026-09-05 when it parked on codex's own approval
    /// prompts. The second heal has to reach it, and only it.
    #[test]
    fn the_second_heal_gives_codex_its_bypass_flag() {
        let (_guard, path) = scratch_registry("heal-codex");
        std::fs::write(
            &path,
            r#"seed_revision = 1

[[agents]]
id = "claude"
label = "Claude Code"
detect_cmd = "claude"
launch_cmd = "claude"
extra_args = ["--dangerously-skip-permissions"]
expected_process = "claude"
prompt_injection_mode = "argv"

[[agents]]
id = "codex"
label = "Codex"
detect_cmd = "codex"
launch_cmd = "codex"
extra_args = []
expected_process = "codex"
prompt_injection_mode = "argv"
"#,
        )
        .unwrap();

        let healed = load_registry().unwrap();
        assert_eq!(
            healed.registry.find("codex").unwrap().extra_args,
            vec!["--dangerously-bypass-approvals-and-sandbox"],
            "codex still asks for approval nobody is there to give"
        );
        assert_eq!(
            healed.registry.find("claude").unwrap().extra_args,
            vec!["--dangerously-skip-permissions"],
            "the second heal must not touch an entry the first one already fixed"
        );
        let (argv, _) = build_launch(healed.registry.find("codex").unwrap(), Some("go"), None);
        assert!(
            argv.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()),
            "the flag has to reach the interactive launch, which is the wake path: {argv:?}"
        );
    }

    /// Revision 3: a registry whose gemini entry carries the old launch shape
    /// takes the new one, and builds an argv gemini-cli 0.38 accepts.
    #[test]
    fn the_third_heal_gives_gemini_a_launch_it_accepts() {
        let (_guard, path) = scratch_registry("heal-gemini");
        std::fs::write(
            &path,
            r#"seed_revision = 2

[[agents]]
id = "gemini"
label = "Gemini"
detect_cmd = "gemini"
launch_cmd = "gemini"
extra_args = []
expected_process = "gemini"
prompt_injection_mode = "flag-prompt-interactive"
draft_prompt_flag = "-p"
"#,
        )
        .unwrap();
        let healed = load_registry().unwrap();
        let gemini = healed.registry.find("gemini").unwrap();
        let (argv, _) = build_launch(gemini, Some("do the thing"), None);
        assert_eq!(argv, vec!["gemini", "-i", "do the thing"], "{argv:?}");
        assert!(!argv.iter().any(|a| a == "-p"));
    }

    #[test]
    fn adding_a_runtime_never_rewrites_a_byte_the_user_wrote() {
        let (_guard, path) = scratch_registry("append-only");
        let mine = format!(
            "seed_revision = {SEED_REVISION}\n{}\n# my own note about why grok is missing here\n",
            r#"
[[agents]]
id = "claude"
label = "Claude Code"
detect_cmd = "claude"
launch_cmd = "claude"
extra_args = []
expected_process = "claude"
prompt_injection_mode = "argv"
"#
        );
        std::fs::write(&path, &mine).unwrap();

        load_registry().unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.starts_with(&mine),
            "the user's file was rewritten rather than appended to:\n{after}"
        );
        assert!(
            after.contains("# my own note about why grok is missing here"),
            "a serde round-trip ate the user's comments:\n{after}"
        );
        assert!(after.contains(r#"id = "pi""#), "{after}");
        // And it still parses, which is the thing appending can get wrong.
        let reread: AgentRegistry = toml::from_str(&after).unwrap();
        assert_eq!(reread.seed_revision, SEED_REVISION);
        assert!(reread.find("pi").is_some());
    }

    #[test]
    fn a_missing_registry_is_seeded_with_every_runtime_this_build_knows() {
        let (_guard, path) = scratch_registry("first-run");
        let loaded = load_registry().unwrap();
        assert_eq!(loaded.registry.agents.len(), default_registry().agents.len());
        let on_disk: AgentRegistry =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            on_disk.seed_revision, SEED_REVISION,
            "a file seeded today must not be healed tomorrow"
        );
    }

    #[test]
    fn a_registry_that_cannot_be_parsed_is_left_alone_and_says_which_file() {
        let (_guard, path) = scratch_registry("unparseable");
        std::fs::write(&path, "[[agents]]\nid = \"claude\"\nthis is not toml\n").unwrap();
        let err = load_registry().expect_err("a broken registry must not load");
        assert!(err.contains(&path.display().to_string()), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[[agents]]\nid = \"claude\"\nthis is not toml\n",
            "xNAUT overwrote a file it could not understand"
        );
    }

    #[test]
    fn prefill_modes_still_need_someone_to_press_enter() {
        // Found live 2026-08-28: a woken claude sat at its composer forever.
        // `claude --prefill <text>` puts the prompt in front of the agent and
        // deliberately does not submit it, so FlagPrompt needs the Enter that
        // StdinAfterStart needs, even though it needs no paste. Argv and
        // FlagPromptInteractive run the prompt themselves.
        // The real fix was not a better keypress, it was not needing one:
        // claude launches with its prompt as argv and runs it.
        let claude = default_registry()
            .agents
            .into_iter()
            .find(|a| a.id == "claude")
            .expect("claude is seeded");
        assert_eq!(
            claude.prompt_injection_mode,
            PromptInjectionMode::Argv,
            "claude must RUN its prompt, not park it in a composer"
        );
        // And an install seeded before reconciliation existed must be
        // corrected, because that file was written once and never rewritten.
        let stale = AgentRegistry {
            seed_revision: 0,
            agents: vec![AgentConfig {
                prompt_injection_mode: PromptInjectionMode::FlagPrompt,
                ..claude.clone()
            }],
        };
        let (healed, _, _) = reconcile(stale);
        assert_eq!(
            healed.agents[0].prompt_injection_mode,
            PromptInjectionMode::Argv,
            "an existing agents.toml still waits at a prefilled composer"
        );
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
            None,
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

    /// The veto has to reach codex, and it has to reach it in the right place.
    ///
    /// This pins the SHAPE so a future edit cannot quietly move a flag past the
    /// subcommand or the prompt, which is how flags have been lost before.
    #[test]
    fn codex_carries_the_veto_and_the_flags_stay_in_front() {
        let flags = codex_veto_flags();
        if flags.is_empty() {
            // No bundled script in this tree; nothing to assert about.
            return;
        }
        assert_eq!(flags[0], "--enable");
        assert_eq!(flags[1], "hooks");
        assert!(flags.contains(&"--dangerously-bypass-hook-trust".to_string()));
        let config = flags.last().unwrap();
        assert!(config.starts_with("hooks.PreToolUse=[{hooks=[{type=\"command\""), "{config}");
        assert!(config.contains("xnaut-veto.sh"), "{config}");
        // The TOML fragment must be one argument, or codex sees a stray word.
        assert!(!config.contains(' '), "the config fragment must not split: {config}");

        let mut runtime = cfg(PromptInjectionMode::Argv, None, None);
        runtime.id = "codex".into();
        runtime.launch_cmd = "codex".into();
        runtime.extra_args.clear();
        let (argv, _, _) = build_conversation_launch(
            &runtime, "Run tests", None, None, None, false, None, &[],
            None,
        )
        .unwrap();
        // In front of the subcommand, immediately after the binary.
        assert_eq!(&argv[1..1 + flags.len()], &flags[..], "{argv:?}");
        assert_eq!(argv[1 + flags.len()], "exec", "{argv:?}");
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
            None,
        )
        .unwrap();
        assert_eq!(
            // The veto prefix has its own test; this one is about the exec shape.
            strip_veto(&argv),
            vec![
                // --approve-for-me IS workspace-write with approvals handled;
                // pairing it with --sandbox is a hard error in codex exec.
                // --skip-git-repo-check still admits an explicitly selected
                // non-repository folder; scratch is bootstrapped (XNAUT-274).
                // -c network_access lets a dev server bind; without it codex's
                // sandbox refuses with listen EPERM before anything starts.
                "codex", "exec", "--skip-git-repo-check", "--json", "--color", "never",
                "--approve-for-me", "-c", "sandbox_workspace_write.network_access=true",
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
            None,
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
        let error = build_conversation_launch(&runtime, "hello", None, None, None, false, None, &[], None)
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
