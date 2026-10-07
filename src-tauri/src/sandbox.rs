// ABOUTME: Pluggable sandbox provider for the Sandbox Verify module (XNAUT-19).
// ABOUTME: Enum dispatch (no trait object / async-trait dep). GitVM is the only
// driver; e2b/daytona are future enum variants. HTTP surface mirrors the
// build-port-pulse skill: X-API-Key auth, /v1/sandboxes create/status/exec/destroy.
#![allow(dead_code)] // API is consumed by sandbox_verify.rs in later Phase-1 steps.

use crate::settings::{resolve_sandbox_key, SandboxProviderSettings};
use serde_json::Value;
use std::time::Duration;

pub struct SandboxSpec {
    pub template: String,
    pub vcpus: u32,
    pub memory_mb: u32,
    pub timeout_secs: u32,
    pub exposed_port: u16,
}

impl Default for SandboxSpec {
    fn default() -> Self {
        // Verify runs are short-lived — 1h timeout, not the 24h demo default, so a
        // missing DELETE endpoint still self-destructs promptly (open question #1).
        Self {
            template: "pi-dev".into(),
            vcpus: 2,
            memory_mb: 4096,
            timeout_secs: 3600,
            exposed_port: 80,
        }
    }
}

pub struct SandboxHandle {
    pub id: String,
    pub public_url: String,
}

pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// The seam. One variant per provider; add e2b/daytona here + one impl block.
pub enum SandboxDriver {
    GitVm(GitVmDriver),
}

impl SandboxDriver {
    pub fn for_settings(provider: &SandboxProviderSettings) -> Result<Self, String> {
        match provider.kind.as_str() {
            "gitvm" => Ok(Self::GitVm(GitVmDriver::new(provider)?)),
            other => Err(format!(
                "unsupported sandbox provider: {other} (supported: gitvm)"
            )),
        }
    }

    pub async fn create(&self, spec: &SandboxSpec) -> Result<SandboxHandle, String> {
        match self {
            Self::GitVm(driver) => driver.create(spec).await,
        }
    }

    pub async fn wait_ready(&self, id: &str, max_secs: u64) -> Result<(), String> {
        match self {
            Self::GitVm(driver) => driver.wait_ready(id, max_secs).await,
        }
    }

    pub async fn exec(&self, id: &str, command: &str) -> Result<ExecResult, String> {
        match self {
            Self::GitVm(driver) => driver.exec(id, command).await,
        }
    }

    pub async fn destroy(&self, id: &str) -> Result<(), String> {
        match self {
            Self::GitVm(driver) => driver.destroy(id).await,
        }
    }
}

/// Where an agent launch runs (XNAUT-266).
///
/// One launcher, the environment as an option. The owner named the options
/// himself: `local`, `exe-dev`, `gitvm`, and "option 4: ... next one". Adding
/// that fourth is one variant plus one arm in `key`, `from_key` and `status_of`;
/// nothing outside this module branches on which environment it got.
///
/// Configuration is the only switch, and `settings.sandboxes` is that
/// configuration: declare an `exe-dev` entry and unpinned launches resolve
/// there, remove every entry and everything falls back to `local`. Settings
/// order is the preference order, so "if I pay X USD for exe.dev then I want to
/// use it, always" is expressed by putting exe-dev first. That mirrors `forges`
/// and `sandboxes`' own comment, where the first entry is already the default.
pub mod launch_env {
    use crate::settings::{resolve_sandbox_key, SandboxProviderSettings};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum LaunchEnv {
        /// zellij on this machine. Today's default, and the fallback whenever
        /// nothing usable is configured.
        Local,
        /// A persistent VM per project, reached over the owner's ssh key.
        ExeDev,
        /// A sandbox per worktree, the runner NautLoom already drives.
        GitVm,
    }

    impl LaunchEnv {
        /// The wire name, spelled exactly as the owner says it. These are the
        /// same strings `.xnaut/verify.json` already uses for its `provider`
        /// field, so verify and launch share one vocabulary instead of two.
        pub fn key(self) -> &'static str {
            match self {
                Self::Local => "local",
                Self::ExeDev => "exe-dev",
                Self::GitVm => "gitvm",
            }
        }

        pub fn from_key(key: &str) -> Option<Self> {
            match key.trim() {
                "local" => Some(Self::Local),
                "exe-dev" => Some(Self::ExeDev),
                "gitvm" => Some(Self::GitVm),
                _ => None,
            }
        }
    }

    /// Every environment, in a stable order, for the "what is configured"
    /// answer. Local first because it is the fallback everything lands on.
    pub const ALL: [LaunchEnv; 3] = [LaunchEnv::Local, LaunchEnv::ExeDev, LaunchEnv::GitVm];

    /// One environment and whether configuration can actually reach it.
    pub struct EnvStatus {
        pub env: LaunchEnv,
        pub ready: bool,
        /// Why, in terms the owner can act on. Quoted verbatim in refusals.
        pub detail: String,
    }

    /// Is this environment reachable with what is configured right now?
    ///
    /// Deliberately a pure read of settings: an ambient GitVM CLI credential on
    /// the machine does NOT opt the fleet into sandboxes, because the switch has
    /// to be one thing the owner can see and flip. The detail string says so
    /// rather than leaving him to guess why a working `gitvm` is "not ready".
    pub fn status_of(env: LaunchEnv, sandboxes: &[SandboxProviderSettings]) -> EnvStatus {
        let entry = |kind: &str| sandboxes.iter().find(|p| p.kind.trim() == kind);
        let (ready, detail) = match env {
            LaunchEnv::Local => (true, "zellij on this machine; always available".to_string()),
            LaunchEnv::ExeDev => match entry("exe-dev") {
                None => (
                    false,
                    "no \"exe-dev\" entry in settings.sandboxes".to_string(),
                ),
                Some(_) => (
                    true,
                    "configured; the credential is this machine's registered exe.dev ssh key"
                        .to_string(),
                ),
            },
            LaunchEnv::GitVm => match entry("gitvm") {
                None => (
                    false,
                    "no \"gitvm\" entry in settings.sandboxes; a CLI key alone does not opt the fleet in"
                        .to_string(),
                ),
                Some(provider) => match resolve_sandbox_key(provider) {
                    None => (
                        false,
                        "\"gitvm\" entry has no api key; set api_key or GITVM_API_KEY".to_string(),
                    ),
                    Some(_) => (true, "configured with an api key".to_string()),
                },
            },
        };
        EnvStatus { env, ready, detail }
    }

    pub fn survey(sandboxes: &[SandboxProviderSettings]) -> Vec<EnvStatus> {
        ALL.iter().map(|env| status_of(*env, sandboxes)).collect()
    }

    /// `EnvStatus` on the wire, for a surface that offers a choice of where to
    /// run (XNAUT-86's Connect button).
    #[derive(Debug, serde::Serialize)]
    pub struct EnvOption {
        pub env: String,
        pub ready: bool,
        pub detail: String,
    }

    /// Every launch environment and whether configuration can reach it.
    ///
    /// The frontend asks rather than deciding for itself, because `status_of`
    /// is where "configured" is defined and a second definition in JavaScript
    /// would be a second answer. In particular a UI that read
    /// `settings.sandboxes` directly would call a `gitvm` entry with no api key
    /// ready, and offer a button that can only fail.
    ///
    /// `detail` is the sentence the refusal itself would use, so a disabled
    /// button says exactly what a refused launch would have said.
    #[tauri::command]
    pub fn launch_env_options() -> Vec<EnvOption> {
        env_options(&crate::settings::load_or_default().sandboxes)
    }

    /// The pure half, so the shape is testable without a settings file.
    pub fn env_options(sandboxes: &[SandboxProviderSettings]) -> Vec<EnvOption> {
        survey(sandboxes)
            .into_iter()
            .map(|status| EnvOption {
                env: status.env.key().to_string(),
                ready: status.ready,
                detail: status.detail,
            })
            .collect()
    }

    /// The single place that answers "where does this run".
    ///
    /// `pinned` is the one deliberate exception the ticket keeps: a profile that
    /// names its own environment gets it whatever settings say, which is what
    /// keeps every local profile on exactly the path it took before this seam
    /// existed. Everything else asks configuration, and configuration only.
    pub fn resolve(pinned: Option<LaunchEnv>, sandboxes: &[SandboxProviderSettings]) -> LaunchEnv {
        if let Some(env) = pinned {
            return env;
        }
        // ponytail: settings order is the whole priority model. A per-provider
        // `priority` field is the ceiling; reordering the list is enough today.
        sandboxes
            .iter()
            .filter_map(|provider| LaunchEnv::from_key(&provider.kind))
            .find(|env| status_of(*env, sandboxes).ready)
            .unwrap_or(LaunchEnv::Local)
    }

    /// What the launcher does once the environment is known.
    pub enum LaunchRoute {
        /// Spawn on this machine, down exactly the path that ran before this
        /// seam existed. The local driver is a passthrough on purpose: local
        /// behaviour must not move, and the test below is what holds it still.
        Local,
        /// Spawn on the exe.dev VM. The run lives in a tmux session there and
        /// the local PTY is only a viewport onto it, which is the same shape
        /// as local, where zellij owns the process and the PTY is a window
        /// onto that. The symmetry is what makes one adoption story cover
        /// both.
        ExeDev,
        /// Spawn in the GitVM sandbox belonging to this worktree. Same shape
        /// again: tmux inside the sandbox owns the agent, the local PTY hosts
        /// an ssh that is only the viewport. Three environments, one contract.
        GitVm,
    }

    impl LaunchEnv {
        /// The one question the launcher asks.
        ///
        /// Every environment now has a driver, so the only refusal left is the
        /// honest one: this run resolved somewhere that is NOT CONFIGURED. It
        /// names every environment and its state, so it is actionable instead
        /// of flat.
        pub fn route(self, sandboxes: &[SandboxProviderSettings]) -> Result<LaunchRoute, String> {
            // Configured is the whole test, and it is the owner's acceptance
            // criterion read literally: if he pays for exe.dev and says so in
            // settings, that is where the run goes.
            if !status_of(self, sandboxes).ready {
                return Err(not_configured(self, sandboxes));
            }
            Ok(match self {
                Self::Local => LaunchRoute::Local,
                Self::ExeDev => LaunchRoute::ExeDev,
                Self::GitVm => LaunchRoute::GitVm,
            })
        }
    }

    fn not_configured(env: LaunchEnv, sandboxes: &[SandboxProviderSettings]) -> String {
        let lines: Vec<String> = survey(sandboxes)
            .iter()
            .map(|status| {
                format!(
                    "  {}: {} ({})",
                    status.env.key(),
                    if status.ready { "ready" } else { "not ready" },
                    status.detail
                )
            })
            .collect();
        format!(
            "resolves to the `{}` environment, which is not configured, so this launch is refused \
rather than run somewhere you did not choose.\n{}\n\
Set this profile's execution to local to run it here now.",
            env.key(),
            lines.join("\n")
        )
    }

    // ── one vocabulary for naming a run, whatever hosts it ──────────────────
    //
    // The session name IS the adoption story, and it has to be the same story
    // in all three environments or "one contract out" is a slogan. Locally
    // `agents.rs` prefix-matches zellij's session list; on the VM and in a
    // sandbox `tmux list-sessions` is asked the identical question. Nothing
    // anywhere remembers a name: it is DERIVED from the handle, which is on
    // disk in the profile store, so an app restart costs the viewport and
    // never the run.

    /// The multiplexer session one run lives in.
    pub fn session_name(handle: &str, run_id: &str) -> String {
        let run = &run_id[..run_id.len().min(8)];
        crate::zellij::session_name(&format!("xnaut-{}-{run}", handle.trim()))
    }

    /// Repository workers use tmux, which accepts the full run identity.
    /// Zellij's short-name cap can truncate a long handle's ULID to "01",
    /// attaching a new task to a different run instead of starting it.
    pub fn repository_session_name(handle: &str, run_id: &str) -> String {
        let id: String = run_id.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect();
        format!("{}{id}", session_prefix(handle))
    }

    /// The prefix every session belonging to one agent starts with.
    ///
    /// Built the same way as `session_name`, so truncation of a long handle
    /// truncates both consistently and the prefix still matches.
    pub fn session_prefix(handle: &str) -> String {
        format!(
            "{}-",
            crate::zellij::session_name(&format!("xnaut-{}", handle.trim()))
        )
    }

    /// Filter a `tmux list-sessions` (or zellij) listing down to one agent.
    pub fn sessions_for_handle(listing: &str, handle: &str) -> Vec<String> {
        let prefix = session_prefix(handle);
        listing
            .lines()
            .map(str::trim)
            .filter(|name| name.starts_with(&prefix))
            .map(str::to_string)
            .collect()
    }

    /// The PROJECT an environment belongs to: the repository, not the worktree
    /// (XNAUT-266, isolation granularity).
    ///
    /// This is the difference between a run that costs three minutes and one
    /// that costs thirty. An environment keyed by worktree is an environment
    /// per TICKET, and every ticket then pays for a cold `target/` and a cold
    /// `node_modules`. Keyed by repository, one agent keeps one environment
    /// across all its tickets and the build cache is warm from the second run
    /// onwards — which is the whole reason to prefer a persistent VM.
    ///
    /// `--git-common-dir` rather than `--show-toplevel`: inside a linked
    /// worktree the toplevel IS the worktree, which is exactly the answer we
    /// must not take. The common dir is the shared `.git`, and its parent is
    /// the main checkout every worktree of that repo agrees on.
    pub fn project_root(worktree: &std::path::Path) -> std::path::PathBuf {
        let common = std::process::Command::new("git")
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .current_dir(worktree)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|path| !path.is_empty());
        match common {
            // `.git` → its parent is the main checkout. A bare repo has no
            // parent worth naming, so fall back rather than climb out of it.
            Some(path) => std::path::Path::new(&path)
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| worktree.to_path_buf()),
            // Not a repository, or no git: the directory is its own project.
            // Never an error — a scratch workspace is a legitimate place to
            // run, and refusing here would block it for no gain.
            None => worktree.to_path_buf(),
        }
    }

    /// The directory name one agent's environment for one project takes.
    ///
    /// Handle FIRST so a listing sorts by agent, which is how the owner asks
    /// the question ("what has @builder got running?"). Two agents on one
    /// project get two directories on purpose: `push` mirrors with `--delete`,
    /// so sharing one would mean each agent periodically erasing the other's
    /// working copy.
    pub fn env_key(handle: &str, project_root: &std::path::Path) -> Result<String, String> {
        let slug = |raw: &str| -> String {
            raw.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect()
        };
        let handle = slug(handle.trim().trim_start_matches('@'));
        let project = slug(&project_root.to_string_lossy());
        if !handle.chars().any(|c| c.is_ascii_alphanumeric())
            || !project.chars().any(|c| c.is_ascii_alphanumeric())
        {
            return Err(format!(
                "a launch environment needs an agent handle and a project path to key itself; \
got handle {handle:?} and project {:?}",
                project_root.display()
            ));
        }
        Ok(format!("{handle}/{project}"))
    }

    // ── the first-run wizard a fresh machine still shows ───────────────────
    //
    // Measured on the exe.dev VM 2026-09-03: `claude` there had never been
    // run, so it opened its first-run THEME PICKER and sat on it, ahead of any
    // prompt. The pane showed a wizard and the run never started. The local
    // path has had the same class of bug twice and fixed it twice — codex on
    // the wake workspace (XNAUT-274, 2026-09-06 12:15) and gemini on tron
    // (2026-09-06 14:51) — so this is a known failure with a known shape, just
    // never applied to a machine we do not own.
    //
    // Two things differ from the local seeders in `agents.rs`, and they are
    // the reason this is not simply those functions called over ssh:
    //
    //   1. The remote needs MORE. Locally the CLI has been run by the owner,
    //      so only per-project trust is missing. A fresh image has never run
    //      it at all, so `hasCompletedOnboarding` — the theme picker, the
    //      thing actually measured — has to be seeded too.
    //   2. It must MERGE, never overwrite. `gitvm warm-up --authSync` copies
    //      credentials into `~/.claude.json`; a seeder that wrote the file
    //      fresh would log the agent out to save it a wizard. The writes below
    //      read-modify-write and rename into place, the same shape
    //      `write_claude_project_trust` uses locally.
    //
    // PRECONDITION: the caller has already `cd`'d to the run's working
    // directory, and this uses `$PWD` to name it. That is deliberate — the
    // trust key has to be the absolute path as the CLI on that machine sees
    // it, and for exe.dev that path is under a `$HOME` this side cannot know.
    // Asking the far side is exact; computing it here would be a guess.
    //
    // Best effort by construction: every branch ends in a fallback that says
    // what could not be seeded, because the honest failure is a visible line
    // in the pane, and the dishonest one is a silent no-op that returns the
    // wizard nobody then explains.

    /// Shell that pre-answers a runtime's first-run prompts on a remote
    /// machine. Empty when the runtime has no known wizard to answer.
    pub fn onboarding_seed(cfg: &crate::agents::AgentConfig) -> String {
        use crate::agents::PreflightTrust;

        // Claude keys off the binary rather than `preflight_trust`, matching
        // `agents.rs`'s own `cfg.detect_cmd == "claude"` special case: its
        // trust lives in the same file as its onboarding flag, so one merge
        // answers both.
        if cfg.detect_cmd == "claude" || cfg.launch_cmd == "claude" {
            return json_seed(
                "claude",
                r#"
d.setdefault("projects", {})
if not isinstance(d["projects"], dict): d["projects"] = {}
e = d["projects"].get(w)
if not isinstance(e, dict): e = {}
e["hasTrustDialogAccepted"] = True
d["projects"][w] = e
d["hasCompletedOnboarding"] = True
d["bypassPermissionsModeAccepted"] = True
"#,
                ".claude.json",
            );
        }

        match cfg.preflight_trust {
            // Codex's answer is TOML, and appending a stanza needs no
            // interpreter at all — so this one branch cannot fail for want of
            // python. The header spelling is codex's own, and matches the
            // `{dir:?}` form `write_codex_project_trust` writes locally.
            Some(PreflightTrust::Codex) => "\
mkdir -p \"$HOME\"/.codex\n\
if ! grep -qxF \"[projects.\\\"$PWD\\\"]\" \"$HOME\"/.codex/config.toml 2>/dev/null; then\n\
  printf '\\n[projects.\"%s\"]\\ntrust_level = \"trusted\"\\n' \"$PWD\" >> \"$HOME\"/.codex/config.toml\n\
fi\n"
                .to_string(),
            Some(PreflightTrust::Gemini) => json_seed(
                "gemini",
                "\nd[w] = \"TRUST_FOLDER\"\n",
                ".gemini/trustedFolders.json",
            ),
            // Cursor and Copilot have no artifact writer locally either
            // (`apply_preflight_trust` says so out loud). Inventing one here,
            // unexercised against those CLIs, would be a guess written to a
            // config file — worse than the wizard it replaced.
            Some(PreflightTrust::Cursor) | Some(PreflightTrust::Copilot) | None => String::new(),
        }
    }

    /// A read-modify-write of one JSON config on the remote, as shell.
    ///
    /// The path and the mutation are the only things that vary. `$PWD` reaches
    /// python through the ENVIRONMENT rather than through string
    /// interpolation, so a directory containing a quote is data instead of
    /// syntax. The python source is single-quoted for the shell and therefore
    /// contains no single quotes — double quotes only, deliberately.
    fn json_seed(label: &str, mutate: &str, rel: &str) -> String {
        let body = format!(
            r#"import json, os, pathlib
w = os.environ["XNAUT_SEED_DIR"]
p = pathlib.Path.home() / "{rel}"
p.parent.mkdir(parents=True, exist_ok=True)
try:
    d = json.loads(p.read_text())
except Exception:
    d = {{}}
if not isinstance(d, dict): d = {{}}
{mutate}
t = p.with_name(p.name + ".xnaut-seed")
t.write_text(json.dumps(d, indent=2))
t.replace(p)
"#
        );
        debug_assert!(
            !body.contains('\''),
            "the python seed is single-quoted for the shell and must not contain one"
        );
        format!(
            "XNAUT_SEED_DIR=\"$PWD\" python3 -c '{body}' 2>/dev/null \
|| printf '\\033[33mxNAUT: could not seed {label} onboarding on this machine; \
its first run may open a wizard\\033[0m\\n'\n"
        )
    }

    /// The live-environment ledger (XNAUT-266, lifecycle).
    ///
    /// `spend::admit_launch` counts LAUNCHES, and a launch count is exactly the
    /// wrong unit for a paid provider: twenty short launches that reuse one VM
    /// cost one VM, while twenty launches that each create a sandbox cost
    /// twenty. Nothing in the app knew how many machines were up, so a paid
    /// provider could accumulate them quietly — a bug whose symptom is an
    /// invoice rather than a stack trace.
    ///
    /// So this file records what EXISTS, not what happened. Two rules come out
    /// of it, and both are enforced at the one launch point:
    ///
    ///   1. A cap on live environments per provider. Reuse is always free —
    ///      an agent returning to a project it already has an environment for
    ///      is the case the granularity rule exists to encourage.
    ///   2. Idle reaping before admitting. A launch that would breach the cap
    ///      first reaps anything past its idle window, so the cap throttles a
    ///      genuinely busy fleet rather than a fleet that merely once was.
    ///
    /// `Local` never appears here. There is nothing to accumulate and nothing
    /// to bill on the owner's own Mac, and counting it would make the cap fire
    /// on the one environment that is free.
    pub mod live {
        use serde::{Deserialize, Serialize};
        use std::path::{Path, PathBuf};

        /// What an environment's run must be doing for it to be destroyed
        /// (XNAUT-307).
        ///
        /// This enum replaced an elapsed-time comparison, and the replacement
        /// is the ticket. The old rule destroyed a GitVM sandbox 45 minutes
        /// after launch whether or not an agent was working in it, and
        /// teardown destroys `/workspace` (XNAUT-40) — so a timer could and
        /// did stand to delete a day of uncommitted work. Age is not evidence
        /// of anything. The beacon is.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Liveness {
            /// The beacon answered and the run is moving. Never reapable, at
            /// any age.
            Alive,
            /// The beacon answered and the run has DECLARED a wait — a
            /// question in the inbox, a review, an approval. Silence from the
            /// agent is expected here and is not a stall.
            Waiting,
            /// The beacon answered, but nothing has moved for longer than the
            /// progress window and the run has not said it is waiting.
            Stalled,
            /// The beacon stopped, or the run record it named is gone. The VM
            /// is not there, or is no longer ours to account for.
            Gone,
            /// The run reached a terminal state. The work is over; the machine
            /// is not.
            Finished,
            /// The ledger entry names no run at all — written by a version
            /// that had no beacon, or by a path that does not register one.
            /// There is no evidence either way, so there is no destroying it.
            Unlinked,
        }

        /// May this environment be destroyed?
        ///
        /// Pure, tiny, and the single place the question is answered. Deleting
        /// the `Alive` arm is the mutation that must turn a test red.
        pub fn reapable(liveness: Liveness) -> bool {
            match liveness {
                Liveness::Gone | Liveness::Stalled | Liveness::Finished => true,
                // `Unlinked` is deliberately NOT reapable. An entry with no run
                // behind it is unaccounted for, not proven idle, and the honest
                // response to "I cannot tell" is to refuse the launch and name
                // the machine — not to destroy a workspace on a guess.
                Liveness::Alive | Liveness::Waiting | Liveness::Unlinked => false,
            }
        }

        /// Read one environment's liveness off its run. Pure.
        ///
        /// `run` is `None` in two different situations and they mean opposite
        /// things, which is why the entry's own `run_id` decides between them:
        /// an entry that never named a run is `Unlinked` and untouchable,
        /// while an entry that names a run the registry no longer has is
        /// `Gone` — nothing can be alive behind a record that does not exist.
        pub fn liveness(
            entry: &Environment,
            run: Option<&crate::run_control::RunManifest>,
            at: i64,
        ) -> Liveness {
            let Some(id) = entry.run_id.as_deref().filter(|id| !id.trim().is_empty()) else {
                return Liveness::Unlinked;
            };
            let Some(run) = run.filter(|run| run.run_id == id) else {
                return Liveness::Gone;
            };
            if run.state.terminal() {
                return Liveness::Finished;
            }
            if at.saturating_sub(run.last_seen_at) > crate::run_control::BEACON_LAPSE_MS {
                return Liveness::Gone;
            }
            if run
                .waiting_on
                .as_deref()
                .is_some_and(|w| !w.trim().is_empty())
            {
                return Liveness::Waiting;
            }
            if at.saturating_sub(run.last_progress_at) > crate::run_control::PROGRESS_WINDOW_MS {
                return Liveness::Stalled;
            }
            Liveness::Alive
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub struct Environment {
            /// A `LaunchEnv` key: `exe-dev` or `gitvm`.
            pub env: String,
            pub handle: String,
            /// The project root, absolute — the same string `env_key` slugs.
            pub project: String,
            /// What the driver needs to find this environment again: the local
            /// directory GitVM keys its sandbox by, or the remote workdir on
            /// the exe.dev VM.
            pub dir: String,
            pub created_ms: i64,
            pub last_used_ms: i64,
            /// The registry run whose beacon speaks for this machine
            /// (XNAUT-307). `serde(default)` because ledgers written before
            /// the beacon existed have no such link, and those entries are
            /// `Unlinked` rather than reapable.
            #[serde(default)]
            pub run_id: Option<String>,
        }

        #[derive(Debug, Clone, Default, Serialize, Deserialize)]
        pub struct Ledger {
            #[serde(default)]
            pub environments: Vec<Environment>,
        }

        impl Ledger {
            fn position(&self, env: &str, handle: &str, project: &str) -> Option<usize> {
                self.environments
                    .iter()
                    .position(|e| e.env == env && e.handle == handle && e.project == project)
            }

            pub fn find(&self, env: &str, handle: &str, project: &str) -> Option<&Environment> {
                self.position(env, handle, project)
                    .map(|i| &self.environments[i])
            }

            /// Everything of one provider still on the books.
            pub fn of(&self, env: &str) -> Vec<&Environment> {
                self.environments.iter().filter(|e| e.env == env).collect()
            }

            /// exe.dev's driver always targets exe::VM. Its per-agent/project
            /// rows support session adoption; they are not separately billed
            /// machines. Keep every row rather than merging away run ownership.
            fn machine_count(&self, env: &str) -> usize {
                let rows = self.of(env).len();
                if env == "exe-dev" { usize::from(rows > 0) } else { rows }
            }

            /// Record this environment as in use right now, creating the entry
            /// on first sight.
            ///
            /// `run` is the registry run whose beacon will speak for this
            /// machine. It is `Option` because the exe.dev path does not
            /// register one, and passing `None` must leave an existing link
            /// alone rather than clearing it — a launch that forgot to say
            /// which run it was would otherwise demote a live, beaconed
            /// environment to `Unlinked` and make it unreapable forever.
            pub fn touch(
                &mut self,
                env: &str,
                handle: &str,
                project: &str,
                dir: &str,
                run: Option<&str>,
                now: i64,
            ) {
                match self.position(env, handle, project) {
                    Some(i) => {
                        self.environments[i].last_used_ms = now;
                        // A worktree can move; the key is agent+project, and
                        // the directory is a detail that follows it.
                        self.environments[i].dir = dir.to_string();
                        if let Some(run) = run.filter(|r| !r.trim().is_empty()) {
                            self.environments[i].run_id = Some(run.to_string());
                        }
                    }
                    None => self.environments.push(Environment {
                        env: env.to_string(),
                        handle: handle.to_string(),
                        project: project.to_string(),
                        dir: dir.to_string(),
                        created_ms: now,
                        last_used_ms: now,
                        run_id: run.map(str::to_string).filter(|r| !r.trim().is_empty()),
                    }),
                }
            }

            pub fn forget(&mut self, env: &str, handle: &str, project: &str) {
                self.environments
                    .retain(|e| !(e.env == env && e.handle == handle && e.project == project));
            }

            /// Everything of one provider whose run is over or unreachable,
            /// EXCEPT the one this launch is about to reuse.
            ///
            /// XNAUT-307 replaced this function's body wholesale. It used to
            /// select on `now - last_used_ms > idle_ms`, and `last_used_ms` is
            /// stamped at LAUNCH and never again — so "idle for 45 minutes"
            /// actually meant "launched 45 minutes ago", and an agent working
            /// steadily since then was exactly as reapable as an abandoned
            /// box. Elapsed time is not in this function any more, and there
            /// is no parameter through which it could return.
            ///
            /// `look_up` is passed in rather than read here so the decision
            /// stays pure: the caller owns the registry, this owns the rule.
            ///
            /// The reuse exception matters and survives the rewrite: an agent
            /// coming back to a project it already has an environment for is
            /// the case the granularity rule exists to encourage, and reaping
            /// the environment it is asking for would throw away the warm
            /// cache to make room for a cold copy of itself.
            pub fn reapable_now(
                &self,
                env: &str,
                keep: Option<(&str, &str)>,
                at: i64,
                look_up: impl Fn(&str) -> Option<crate::run_control::RunManifest>,
            ) -> Vec<(Environment, Liveness)> {
                self.environments
                    .iter()
                    .filter(|e| e.env == env)
                    .filter(|e| !matches!(keep, Some((h, p)) if e.handle == h && e.project == p))
                    .filter_map(|e| {
                        let run = e.run_id.as_deref().and_then(&look_up);
                        let state = liveness(e, run.as_ref(), at);
                        reapable(state).then(|| (e.clone(), state))
                    })
                    .collect()
            }

            /// The provider's environments this launch may NOT touch, with the
            /// reason. Used to say why a ceiling refusal is a refusal rather
            /// than something the reaper should have cleared.
            pub fn held(
                &self,
                env: &str,
                at: i64,
                look_up: impl Fn(&str) -> Option<crate::run_control::RunManifest>,
            ) -> Vec<(Environment, Liveness)> {
                self.environments
                    .iter()
                    .filter(|e| e.env == env)
                    .map(|e| {
                        let run = e.run_id.as_deref().and_then(&look_up);
                        (e.clone(), liveness(e, run.as_ref(), at))
                    })
                    .filter(|(_, state)| !reapable(*state))
                    .collect()
            }

            /// May this agent have an environment of this provider?
            ///
            /// Reuse is always admitted; only a NEW machine can breach a cap on
            /// machines. A cap of 0 means "none of this provider", which is a
            /// legitimate way to switch a paid provider off without editing
            /// settings.
            pub fn admit(
                &self,
                env: &str,
                handle: &str,
                project: &str,
                cap: u32,
            ) -> Result<(), String> {
                if self.find(env, handle, project).is_some()
                    || (env == "exe-dev" && self.machine_count(env) > 0)
                {
                    return Ok(());
                }
                let live = self.machine_count(env);
                if live < cap as usize {
                    return Ok(());
                }
                let names: Vec<String> = self
                    .of(env)
                    .iter()
                    .map(|e| format!("@{} on {}", e.handle, e.project))
                    .collect();
                Err(format!(
                    "environment ceiling: {live} `{env}` environments are already recorded and the cap \
is {cap}; no new environment was started.\n  {}\nExisting entries were preserved. This ledger \
does not by itself prove that a run is active or waiting on a ticket; check its beacon/status. Retry when capacity is \
available or explicitly adjust max_live_environments in spend-ceiling.json. Do not stop active \
or waiting runs to make room.",
                    if names.is_empty() {
                        "(none recorded)".to_string()
                    } else {
                        names.join("\n  ")
                    },
                ))
            }
        }

        fn ledger_path() -> PathBuf {
            crate::spend::config_dir().join("launch-environments.json")
        }

        pub fn load() -> Ledger {
            std::fs::read_to_string(ledger_path())
                .ok()
                .and_then(|body| serde_json::from_str(&body).ok())
                .unwrap_or_default()
        }

        pub fn store(ledger: &Ledger) -> Result<(), String> {
            let dir = crate::spend::config_dir();
            std::fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
            let body = serde_json::to_string_pretty(ledger).map_err(|e| e.to_string())?;
            std::fs::write(ledger_path(), body)
                .map_err(|e| format!("write the environment ledger: {e}"))
        }

        pub fn now_ms() -> i64 {
            chrono::Utc::now().timestamp_millis()
        }

        /// Destroy one recorded environment, and say plainly when it could not
        /// be destroyed.
        ///
        /// XNAUT-40 is the whole reason this is a function rather than a call
        /// to `cli::stop`: GitVM teardown DESTROYS `/workspace`, so the pull
        /// comes first and a FAILED pull cancels the destroy. Losing a slot is
        /// recoverable; losing an agent's uncommitted work is not.
        ///
        /// exe.dev is not destroyed at all. There is one persistent VM and the
        /// workdir's warm build cache is the reason to pay for it; reaping
        /// there means forgetting the slot, not deleting the directory.
        pub fn reap(entry: &Environment) -> Result<(), String> {
            if entry.env != "gitvm" {
                return Ok(());
            }
            let dir = Path::new(&entry.dir);
            crate::repository_transfer::guard_worker_teardown(dir)?;
            if !dir.is_dir() {
                // The worktree is gone, so there is nothing to pull work back
                // into. `stop` would still be right, but it is the CLI's own
                // directory-scoped state we would be reaching for, and it
                // lives in that directory. Say so instead of pretending.
                return Err(format!(
                    "{} no longer exists, so its sandbox cannot be pulled back before teardown; \
run `gitvm stop` there by hand if it is still up",
                    entry.dir
                ));
            }
            super::super::cli::pull(dir)
                .map_err(|why| format!("refusing to destroy {}: the pull back failed ({why})", entry.dir))?;
            super::super::cli::stop(dir)
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            fn env(handle: &str, project: &str, used: i64) -> Environment {
                Environment {
                    env: "gitvm".into(),
                    handle: handle.into(),
                    project: project.into(),
                    dir: format!("/tmp/{handle}"),
                    created_ms: 0,
                    last_used_ms: used,
                    run_id: None,
                }
            }

            /// An environment linked to a run, which is what every entry the
            /// GitVM launcher writes looks like from XNAUT-307 on.
            fn linked(handle: &str, project: &str, run: &str) -> Environment {
                Environment {
                    run_id: Some(run.into()),
                    ..env(handle, project, 0)
                }
            }

            /// A run the beacon has just spoken for.
            fn run_seen(id: &str, at: i64) -> crate::run_control::RunManifest {
                let mut run = crate::run_control::tests::run();
                run.run_id = id.to_string();
                run.last_seen_at = at;
                run.last_progress_at = at;
                run
            }

            /// A registry that answers for exactly the runs it is given.
            fn registry(
                runs: Vec<crate::run_control::RunManifest>,
            ) -> impl Fn(&str) -> Option<crate::run_control::RunManifest> {
                move |id: &str| runs.iter().find(|r| r.run_id == id).cloned()
            }

            /// The bug this module exists for: counting launches lets machines
            /// accumulate. Counting machines does not.
            #[test]
            fn a_new_environment_is_refused_at_the_cap_but_reuse_never_is() {
                let ledger = Ledger {
                    environments: vec![env("a", "/p1", 0), env("b", "/p2", 0)],
                };
                let refused = ledger.admit("gitvm", "c", "/p3", 2).unwrap_err();
                assert!(refused.contains("cap is 2"), "{refused}");
                assert!(refused.contains("@a on /p1"), "{refused}");
                // The whole point of one-environment-per-agent-per-project:
                // coming back costs nothing, whatever the cap says.
                assert!(ledger.admit("gitvm", "a", "/p1", 2).is_ok());
                assert!(ledger.admit("gitvm", "a", "/p1", 0).is_ok());
            }

            #[test]
            fn exe_dev_agent_rows_share_one_machine_without_touching_existing_runs() {
                let mut ledger = Ledger::default();
                ledger.touch("exe-dev", "claude", "/xnaut", "/claude-worktree", None, 10);
                ledger.touch("exe-dev", "cortana", "/jobup", "/cortana-worktree", None, 20);
                let existing = ledger.environments.clone();
                assert_eq!(ledger.machine_count("exe-dev"), 1);
                // Exactly the reported regression, including a different
                // identity and a different worktree of the same project.
                assert!(ledger.admit("exe-dev", "codex", "/xnaut", 2).is_ok());
                assert!(ledger.admit("exe-dev", "codex", "/another-project", 1).is_ok());
                ledger.touch("exe-dev", "codex", "/xnaut", "/new-task", None, 30);
                assert_eq!(&ledger.environments[..2], existing.as_slice());
                assert_eq!(ledger.machine_count("exe-dev"), 1);
                assert_eq!(ledger.environments.len(), 3);
                assert!(ledger.reapable_now("exe-dev", None, 40, |_| None).is_empty());
                // A zero budget still refuses the first machine; a previously
                // admitted machine follows the existing reuse policy.
                assert!(Ledger::default().admit("exe-dev", "codex", "/xnaut", 0).is_err());
                assert!(ledger.admit("exe-dev", "codex", "/other", 0).is_ok());
            }

            /// A cap is per provider. exe.dev's single VM must not be spent by
            /// GitVM sandboxes, nor the other way round.
            #[test]
            fn the_cap_counts_one_provider_at_a_time() {
                let mut ledger = Ledger::default();
                ledger.touch("gitvm", "a", "/p1", "/tmp/a", None, 0);
                ledger.touch("gitvm", "b", "/p2", "/tmp/b", None, 0);
                assert!(ledger.admit("exe-dev", "c", "/p3", 1).is_ok());
                assert!(ledger.admit("gitvm", "c", "/p3", 1).is_err());
            }

            /// Reaping returns slots, and never the slot being asked for.
            #[test]
            fn reaping_spares_the_environment_this_launch_wants_back() {
                let now = 10_000_000;
                let ledger = Ledger {
                    environments: vec![
                        linked("silent", "/p1", "r-silent"),
                        linked("returning", "/p2", "r-returning"),
                        linked("busy", "/p3", "r-busy"),
                    ],
                };
                // `returning` is as reapable as `silent` — both went quiet —
                // and is spared anyway, because it is the one being reused.
                let runs = registry(vec![
                    run_seen("r-silent", 0),
                    run_seen("r-returning", 0),
                    run_seen("r-busy", now),
                ]);
                let reapable =
                    ledger.reapable_now("gitvm", Some(("returning", "/p2")), now, runs);
                let names: Vec<&str> =
                    reapable.iter().map(|(e, _)| e.handle.as_str()).collect();
                assert_eq!(names, vec!["silent"]);
            }

            // ─── XNAUT-307: liveness, never age ────────────────────────────

            /// THE regression. XNAUT-266 destroyed a sandbox 45 minutes after
            /// launch whether or not anyone was in it, and teardown destroys
            /// `/workspace` (XNAUT-40). A working agent must survive any age.
            ///
            /// Ten hours old, beacon answering a second ago: untouchable.
            #[test]
            fn a_sandbox_with_a_live_beacon_is_never_reaped_however_old_it_is() {
                let ten_hours = 10 * 60 * 60 * 1000;
                let entry = Environment {
                    created_ms: 0,
                    last_used_ms: 0, // stamped at launch and never moved since
                    ..linked("busy", "/p1", "r-1")
                };
                let ledger = Ledger {
                    environments: vec![entry],
                };
                let run = run_seen("r-1", ten_hours - 1_000);
                assert_eq!(
                    liveness(&ledger.environments[0], Some(&run), ten_hours),
                    Liveness::Alive
                );
                assert!(ledger
                    .reapable_now("gitvm", None, ten_hours, registry(vec![run]))
                    .is_empty());
            }

            /// The other half: silence IS the trigger. A beacon that stopped
            /// past the lapse window means the VM is gone, at any age — a
            /// sandbox that died two minutes after launch is reaped as
            /// promptly as one that died two days in.
            #[test]
            fn a_lapsed_beacon_is_reaped_whatever_the_age() {
                let young = crate::run_control::BEACON_LAPSE_MS + 60_000;
                let ledger = Ledger {
                    environments: vec![linked("gone", "/p1", "r-1")],
                };
                let run = run_seen("r-1", 0);
                assert_eq!(
                    liveness(&ledger.environments[0], Some(&run), young),
                    Liveness::Gone
                );
                let reaped = ledger.reapable_now("gitvm", None, young, registry(vec![run]));
                assert_eq!(reaped.len(), 1);
                assert_eq!(reaped[0].1, Liveness::Gone);
            }

            /// A run that DECLARED a wait is not stalled, however long the
            /// silence. An agent blocked on the owner's answer in the Mesh
            /// inbox can legitimately be quiet for hours, and destroying its
            /// workspace while it waits for a human is the same lost day the
            /// timer used to cost.
            #[test]
            fn a_run_waiting_on_the_owner_is_left_alone_past_the_progress_window() {
                let long = crate::run_control::PROGRESS_WINDOW_MS * 4;
                let ledger = Ledger {
                    environments: vec![linked("asking", "/p1", "r-1")],
                };
                let mut run = run_seen("r-1", long);
                run.last_progress_at = 0; // nothing has moved in four windows
                run.waiting_on = Some("inbox:in-5f3ca904".into());
                assert_eq!(
                    liveness(&ledger.environments[0], Some(&run), long),
                    Liveness::Waiting
                );
                assert!(ledger
                    .reapable_now("gitvm", None, long, registry(vec![run.clone()]))
                    .is_empty());

                // Clear the declared wait and the same run IS a stall. The
                // difference between these two assertions is the entire value
                // of `waiting_on`.
                run.waiting_on = None;
                assert_eq!(
                    liveness(&ledger.environments[0], Some(&run), long),
                    Liveness::Stalled
                );
                assert!(!ledger
                    .reapable_now("gitvm", None, long, registry(vec![run]))
                    .is_empty());
            }

            /// A ledger written before the beacon existed names no run. There
            /// is no evidence about it either way, and the honest answer to
            /// "I cannot tell" is to keep the machine and say so — not to
            /// destroy a workspace on a guess.
            #[test]
            fn an_entry_that_names_no_run_is_never_destroyed_on_a_guess() {
                let ancient = 30 * 24 * 60 * 60 * 1000;
                let ledger = Ledger {
                    environments: vec![env("legacy", "/p1", 0)],
                };
                assert_eq!(
                    liveness(&ledger.environments[0], None, ancient),
                    Liveness::Unlinked
                );
                assert!(ledger
                    .reapable_now("gitvm", None, ancient, registry(vec![]))
                    .is_empty());
                // It is not invisible, though: it is what a ceiling refusal
                // has to name, or the owner cannot act on it.
                let held = ledger.held("gitvm", ancient, registry(vec![]));
                assert_eq!(held.len(), 1);
                assert_eq!(held[0].1, Liveness::Unlinked);
            }

            /// An entry that DOES name a run the registry no longer has is the
            /// opposite case, and must not be confused with the one above:
            /// nothing can be alive behind a record that does not exist.
            #[test]
            fn a_named_run_the_registry_has_lost_is_gone_rather_than_unknown() {
                let ledger = Ledger {
                    environments: vec![linked("orphan", "/p1", "r-vanished")],
                };
                assert_eq!(
                    liveness(&ledger.environments[0], None, 1_000),
                    Liveness::Gone
                );
                assert_eq!(
                    ledger
                        .reapable_now("gitvm", None, 1_000, registry(vec![]))
                        .len(),
                    1
                );
            }

            /// A finished run's machine is returned. The work is over; the VM
            /// is still billing.
            #[test]
            fn a_terminal_run_returns_its_machine() {
                let ledger = Ledger {
                    environments: vec![linked("done", "/p1", "r-1")],
                };
                let mut run = run_seen("r-1", 1_000);
                run.state = crate::run_control::RunState::Done;
                assert_eq!(
                    liveness(&ledger.environments[0], Some(&run), 1_000),
                    Liveness::Finished
                );
                assert!(reapable(Liveness::Finished));
            }

            /// The rule in one line, so a future edit has to argue with it:
            /// working and waiting are kept, everything else is returned.
            #[test]
            fn only_a_run_that_is_over_or_unreachable_may_be_destroyed() {
                assert!(!reapable(Liveness::Alive));
                assert!(!reapable(Liveness::Waiting));
                assert!(!reapable(Liveness::Unlinked));
                assert!(reapable(Liveness::Gone));
                assert!(reapable(Liveness::Stalled));
                assert!(reapable(Liveness::Finished));
            }

            /// A ceiling refusal has to say why each machine was KEPT, or it
            /// reads as a reaper that failed to run.
            #[test]
            fn the_ceiling_refusal_no_longer_promises_an_idle_timeout() {
                let ledger = Ledger {
                    environments: vec![linked("a", "/p1", "r-1")],
                };
                let refused = ledger.admit("gitvm", "b", "/p2", 1).unwrap_err();
                assert!(refused.contains("beacon"), "{refused}");
                assert!(!refused.contains("XNAUT-307"), "historical bug IDs are not task dependencies");
                assert!(refused.contains("does not by itself prove"), "the ledger cannot assert liveness");
                assert!(
                    !refused.contains("idle out"),
                    "nothing idles out any more: {refused}"
                );
                assert!(
                    !refused.contains("45 minutes"),
                    "age must not appear in the refusal: {refused}"
                );
            }

            /// A launch that does not know its run must not ERASE the link an
            /// earlier one recorded — that would demote a live, beaconed
            /// machine to `Unlinked` and make it unreapable forever.
            #[test]
            fn a_later_touch_without_a_run_keeps_the_link_it_found() {
                let mut ledger = Ledger::default();
                ledger.touch("gitvm", "a", "/p1", "/tmp/one", Some("r-1"), 100);
                ledger.touch("gitvm", "a", "/p1", "/tmp/one", None, 200);
                assert_eq!(ledger.environments[0].run_id.as_deref(), Some("r-1"));
                // A newer run does replace it: the machine is being reused by
                // a different run, and that run's beacon is the live one.
                ledger.touch("gitvm", "a", "/p1", "/tmp/one", Some("r-2"), 300);
                assert_eq!(ledger.environments[0].run_id.as_deref(), Some("r-2"));
            }

            /// A ledger written before the field existed still loads, and its
            /// entries read as unlinked rather than as anything reapable.
            #[test]
            fn a_ledger_from_before_the_beacon_still_loads() {
                let old = r#"{"environments":[{"env":"gitvm","handle":"a","project":"/p1",
                    "dir":"/tmp/a","created_ms":1,"last_used_ms":2}]}"#;
                let ledger: Ledger = serde_json::from_str(old).expect("the old shape still loads");
                assert_eq!(ledger.environments.len(), 1);
                assert!(ledger.environments[0].run_id.is_none());
            }

            /// Touch is upsert: the second launch of one agent on one project
            /// moves the clock instead of adding a machine.
            #[test]
            fn touching_twice_records_one_environment() {
                let mut ledger = Ledger::default();
                ledger.touch("gitvm", "a", "/p1", "/tmp/one", None, 100);
                ledger.touch("gitvm", "a", "/p1", "/tmp/two", None, 900);
                assert_eq!(ledger.environments.len(), 1);
                assert_eq!(ledger.environments[0].last_used_ms, 900);
                assert_eq!(ledger.environments[0].created_ms, 100);
                assert_eq!(ledger.environments[0].dir, "/tmp/two");
                ledger.forget("gitvm", "a", "/p1");
                assert!(ledger.environments.is_empty());
            }

            /// The reaper against a REAL sandbox. `#[ignore]`d: it needs a
            /// warm GitVM box and it destroys it.
            ///
            ///   XNAUT_REAP_DIR=/tmp/xnaut-307-live \
            ///   cargo test --bin xnaut reap_a_live_sandbox -- --ignored --nocapture
            ///
            /// The claim being checked is XNAUT-40's ordering, and it is the
            /// one that costs work to get wrong: `reap` must pull `/workspace`
            /// back BEFORE it destroys the machine. So the check writes a file
            /// that exists only inside the sandbox, reaps, and looks for it
            /// locally afterwards. A `stop` that ran first would leave nothing
            /// to find, and no assertion on exit codes would notice.
            #[test]
            #[ignore]
            fn reap_a_live_sandbox_pulls_before_it_destroys() {
                let dir = std::path::PathBuf::from(
                    std::env::var("XNAUT_REAP_DIR").expect("XNAUT_REAP_DIR"),
                );
                let marker = std::env::var("XNAUT_REAP_MARKER")
                    .unwrap_or_else(|_| "agent-uncommitted-work.txt".into());
                assert!(
                    !dir.join(&marker).exists(),
                    "{marker} must NOT exist locally before the reap, or this proves nothing"
                );

                let entry = Environment {
                    env: "gitvm".into(),
                    handle: "claude".into(),
                    project: dir.to_string_lossy().into_owned(),
                    dir: dir.to_string_lossy().into_owned(),
                    created_ms: 0,
                    last_used_ms: 0,
                    run_id: Some("live".into()),
                };
                reap(&entry).expect("the reap ran");

                assert!(
                    dir.join(&marker).exists(),
                    "the sandbox's uncommitted work must be on this machine before teardown \
(XNAUT-40); {marker} is not in {}",
                    dir.display()
                );
                println!("  pulled {marker} back, then destroyed the sandbox");
            }

            /// XNAUT-40, as a test rather than a comment: a directory that is
            /// gone cannot be pulled back, so it is NOT torn down silently.
            #[test]
            fn a_missing_worktree_is_not_torn_down_behind_your_back() {
                let entry = Environment {
                    dir: "/nonexistent/xnaut-266-reap".into(),
                    ..env("a", "/p1", 0)
                };
                let error = reap(&entry).unwrap_err();
                assert!(error.contains("no longer exists"), "{error}");
                assert!(error.contains("gitvm stop"), "{error}");
            }

            /// Local is free and unlimited; it must never reach the ledger,
            /// and reaping it must never mean running a teardown.
            #[test]
            fn local_is_never_a_machine_to_reap() {
                let entry = Environment {
                    env: "local".into(),
                    ..env("a", "/p1", 0)
                };
                assert!(reap(&entry).is_ok());
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn provider(kind: &str, api_key: Option<&str>) -> SandboxProviderSettings {
            SandboxProviderSettings {
                kind: kind.into(),
                base_url: "https://example.invalid".into(),
                api_key: api_key.map(str::to_string),
            }
        }

        // ── onboarding seeding ─────────────────────────────────────────────
        //
        // These RUN the generated shell rather than string-matching it. The
        // bug being prevented is a wizard on a machine this test cannot
        // reach, and the only part reproducible here is the shell itself —
        // so it is executed against a temporary HOME and the resulting
        // config files are read back. A seed that parses but writes the
        // wrong shape would pass a `contains` assertion and still strand the
        // agent.

        fn runtime(detect: &str, trust: Option<crate::agents::PreflightTrust>) -> crate::agents::AgentConfig {
            crate::agents::AgentConfig {
                id: detect.into(),
                label: detect.into(),
                detect_cmd: detect.into(),
                launch_cmd: detect.into(),
                extra_args: vec![],
                expected_process: detect.into(),
                prompt_injection_mode: crate::agents::PromptInjectionMode::Argv,
                draft_prompt_flag: None,
                draft_prompt_env_var: None,
                preflight_trust: trust,
                env: Default::default(),
            }
        }

        // These generated scripts run on Unix remote workers. On Windows CI,
        // Git Bash executes them with an isolated native Python home. Never use
        // the unrelated Windows/WSL bash shim or the runner's real credentials.
        fn seed_shell(home: &std::path::Path, workdir: &std::path::Path) -> std::process::Command {
            let mut command = std::process::Command::new(
                std::env::var_os("XNAUT_TEST_BASH").unwrap_or_else(|| "bash".into()));
            command.args(["--noprofile", "--norc"])
                .current_dir(workdir).env("HOME", home).env("USERPROFILE", home);
            // Preserve the remote Unix trust key when Git Bash invokes native
            // Python; MSYS would otherwise rewrite XNAUT_SEED_DIR to a drive path.
            command.env("MSYS2_ENV_CONV_EXCL", "XNAUT_SEED_DIR");
            #[cfg(windows)]
            command.env("HOME", home.to_string_lossy().replace('\\', "/"));
            command
        }
        fn run_seed(seed: &str, home: &std::path::Path, workdir: &std::path::Path) -> std::process::Output {
            let script = if std::env::var_os("XNAUT_TEST_PYTHON").is_some() {
                format!("python3() {{ \"$XNAUT_TEST_PYTHON\" \"$@\"; }}\n{seed}")
            } else { seed.to_string() };
            seed_shell(home, workdir).args(["-c", &script]).output().expect("the seed shell ran")
        }
        fn seed_workdir(workdir: &std::path::Path) -> String {
            let out = seed_shell(workdir, workdir).args(["-c", "printf '%s' \"$PWD\""])
                .output().unwrap();
            assert!(out.status.success());
            String::from_utf8(out.stdout).unwrap()
        }

        /// Run a seed the way the run script does: from the working
        /// directory, with a HOME of its own. Returns that HOME.
        fn seed_into(seed: &str, workdir: &std::path::Path) -> std::path::PathBuf {
            let home = std::env::temp_dir().join(format!("xnaut-seed-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&home).unwrap();
            std::fs::create_dir_all(workdir).unwrap();
            let out = run_seed(seed, &home, workdir);
            assert!(
                out.status.success(),
                "the seed exited {:?}: {}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr)
            );
            home
        }

        /// The measured failure (exe.dev VM, 2026-09-03): a `claude` that has
        /// never run opens its THEME PICKER, ahead of the prompt. The local
        /// seeder answers trust only, because locally the CLI has already
        /// been run — so a remote seed that copied it would still hang.
        #[test]
        fn a_fresh_machine_gets_claudes_onboarding_answered_and_not_only_its_trust() {
            let work = std::env::temp_dir().join(format!("xnaut-work-{}", uuid::Uuid::new_v4()));
            let home = seed_into(&onboarding_seed(&runtime("claude", None)), &work);
            let saved: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(home.join(".claude.json")).unwrap())
                    .unwrap();

            // The theme picker.
            assert_eq!(saved["hasCompletedOnboarding"], true);
            // And the trust dialog, keyed by the directory the agent runs in,
            // which the seed learned from $PWD rather than being told.
            let key = seed_workdir(&work);
            assert_eq!(
                saved["projects"][&key]["hasTrustDialogAccepted"],
                true,
                "the trust key must be the absolute workdir: {saved}"
            );

            let _ = std::fs::remove_dir_all(&home);
            let _ = std::fs::remove_dir_all(&work);
        }

        /// THE one that costs money to get wrong. `gitvm warm-up --authSync`
        /// copies credentials into `~/.claude.json`. A seeder that wrote the
        /// file fresh would log the agent out of the machine it was being
        /// prepared for — trading a wizard for an auth prompt.
        #[test]
        fn seeding_merges_so_synced_credentials_survive_it() {
            let work = std::env::temp_dir().join(format!("xnaut-work-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&work).unwrap();
            let home = std::env::temp_dir().join(format!("xnaut-seed-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&home).unwrap();
            std::fs::write(
                home.join(".claude.json"),
                r#"{"oauthAccount":{"emailAddress":"a@b.c"},"projects":{"/other":{"allowedTools":["Read"]}}}"#,
            )
            .unwrap();

            let out = run_seed(&onboarding_seed(&runtime("claude", None)), &home, &work);
            assert!(out.status.success());

            let saved: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(home.join(".claude.json")).unwrap())
                    .unwrap();
            assert_eq!(saved["oauthAccount"]["emailAddress"], "a@b.c");
            assert_eq!(saved["projects"]["/other"]["allowedTools"][0], "Read");
            assert_eq!(saved["hasCompletedOnboarding"], true);

            let _ = std::fs::remove_dir_all(&home);
            let _ = std::fs::remove_dir_all(&work);
        }

        /// Codex's answer is TOML in codex's own spelling, and running twice
        /// must not stack two stanzas — the launch path re-runs this on every
        /// sandbox start.
        #[test]
        fn codex_trust_is_written_in_its_own_spelling_exactly_once() {
            let work = std::env::temp_dir().join(format!("xnaut-work-{}", uuid::Uuid::new_v4()));
            let seed = onboarding_seed(&runtime("codex", Some(crate::agents::PreflightTrust::Codex)));
            let home = seed_into(&seed, &work);
            assert!(run_seed(&seed, &home, &work).status.success());

            let body = std::fs::read_to_string(home.join(".codex").join("config.toml")).unwrap();
            let header = format!("[projects.\"{}\"]", seed_workdir(&work));
            assert_eq!(
                body.lines().filter(|l| l.trim() == header).count(),
                1,
                "one stanza after two runs: {body}"
            );
            assert!(body.contains("trust_level = \"trusted\""), "{body}");

            let _ = std::fs::remove_dir_all(&home);
            let _ = std::fs::remove_dir_all(&work);
        }

        #[test]
        fn gemini_folder_trust_is_written_in_its_own_shape() {
            let work = std::env::temp_dir().join(format!("xnaut-work-{}", uuid::Uuid::new_v4()));
            let home = seed_into(
                &onboarding_seed(&runtime("gemini", Some(crate::agents::PreflightTrust::Gemini))),
                &work,
            );
            let saved: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(home.join(".gemini").join("trustedFolders.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(saved[seed_workdir(&work)], "TRUST_FOLDER");

            let _ = std::fs::remove_dir_all(&home);
            let _ = std::fs::remove_dir_all(&work);
        }

        /// A directory whose name is hostile to a shell is DATA. $PWD reaches
        /// python through the environment for exactly this reason.
        #[test]
        fn a_workdir_containing_quotes_is_data_and_not_syntax() {
            // Windows forbids double quotes in filenames; apostrophe, dollar
            // and spaces still exercise shell data handling there.
            let name = if cfg!(windows) { "xnaut-it's $x" } else { "xnaut-it's \"$x\"" };
            let work = std::env::temp_dir().join(format!("{name}-{}", uuid::Uuid::new_v4()));
            let home = seed_into(&onboarding_seed(&runtime("claude", None)), &work);
            let saved: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(home.join(".claude.json")).unwrap())
                    .unwrap();
            assert_eq!(
                saved["projects"][seed_workdir(&work)]["hasTrustDialogAccepted"],
                true,
                "{saved}"
            );

            let _ = std::fs::remove_dir_all(&home);
            let _ = std::fs::remove_dir_all(&work);
        }

        /// Cursor and Copilot have no artifact writer locally either. An
        /// invented one, unexercised against those CLIs, would be a guess
        /// written into a config file.
        #[test]
        fn a_runtime_with_no_known_wizard_is_seeded_with_nothing() {
            assert!(onboarding_seed(&runtime("cursor", Some(crate::agents::PreflightTrust::Cursor))).is_empty());
            assert!(onboarding_seed(&runtime("pi", None)).is_empty());
        }

        /// The seed names its directory with `$PWD`, so it is only correct
        /// AFTER the script has cd'd. Both remote environments must place it
        /// there — on exe.dev the workdir is under a `$HOME` this side cannot
        /// know, which is why the path is asked for rather than computed.
        /// XNAUT-396: `remote_command` puts env assignments before the binary,
        /// and bash's `exec` takes the first word as the program. Both scripts
        /// must route the line through `env` or every remote launch with a
        /// model or identity dies at its last line.
        #[test]
        fn both_remote_scripts_exec_through_env_so_prefixed_assignments_survive() {
            let cmd = "ANTHROPIC_MODEL='claude-opus-5' claude --model x";
            for script in [
                super::super::exe::run_script("w", cmd, "s", ""),
                super::super::cli::run_script(cmd, "s", ""),
            ] {
                let last = script.trim_end().lines().last().unwrap();
                assert_eq!(last, format!("exec env {cmd}"), "{script}");
            }
        }

        #[test]
        fn both_remote_scripts_seed_after_the_cd_that_defines_pwd() {
            let seed = "SEEDMARK\n";

            let exe = super::super::exe::run_script("w", "claude", "s", seed);
            let (cd, mark) = (exe.find("cd ").unwrap(), exe.find("SEEDMARK").unwrap());
            assert!(cd < mark, "exe.dev seeds before its cd: {exe}");
            assert!(mark < exe.find("exec env claude").unwrap(), "{exe}");

            let cli = super::super::cli::run_script("claude", "s", seed);
            let (cd, mark) = (cli.find("cd /workspace").unwrap(), cli.find("SEEDMARK").unwrap());
            assert!(cd < mark, "gitvm seeds before its cd: {cli}");
            assert!(mark < cli.find("exec env claude").unwrap(), "{cli}");
        }

        /// THE no-behaviour-change test. Every profile in the store today is
        /// `execution: local`, and a local pin must survive any configuration:
        /// the day exe.dev is switched on, those launches still take the same
        /// path they took before this seam existed.
        #[test]
        fn a_local_pin_stays_local_whatever_is_configured() {
            let configured = [provider("exe-dev", None), provider("gitvm", Some("k"))];
            assert_eq!(
                resolve(Some(LaunchEnv::Local), &configured),
                LaunchEnv::Local
            );
            assert!(matches!(
                LaunchEnv::Local.route(&configured),
                Ok(LaunchRoute::Local)
            ));
        }

        /// Remove every entry and everything falls back to local; that is the
        /// "plug and play" promise read from the unplugged end.
        #[test]
        fn nothing_configured_resolves_to_local() {
            assert_eq!(resolve(None, &[]), LaunchEnv::Local);
            assert!(matches!(
                LaunchEnv::Local.route(&[]),
                Ok(LaunchRoute::Local)
            ));
        }

        /// What a surface offering a choice of environment is told (XNAUT-86).
        ///
        /// The UI must not decide "configured" for itself — a `gitvm` entry
        /// with no api key looks configured from JavaScript and is not — so it
        /// asks, and what it gets back has to carry the REASON as well as the
        /// verdict. A disabled Connect button that cannot say why is the
        /// fabricated-state failure XNAUT-87 spent a ticket removing.
        #[test]
        fn the_launch_options_carry_the_verdict_and_the_reason() {
            let options = env_options(&[]);
            assert_eq!(options.len(), ALL.len(), "every environment is offered");

            let local = options.iter().find(|o| o.env == "local").expect("local");
            assert!(local.ready, "local is always available");

            let gitvm = options.iter().find(|o| o.env == "gitvm").expect("gitvm");
            assert!(!gitvm.ready);
            assert!(
                gitvm.detail.contains("no \"gitvm\" entry in settings.sandboxes"),
                "the reason has to be actionable, got {:?}",
                gitvm.detail
            );

            // Half-configured is NOT ready, and says which half is missing.
            let keyless = env_options(&[provider("gitvm", None)]);
            let gitvm = keyless.iter().find(|o| o.env == "gitvm").expect("gitvm");
            assert!(!gitvm.ready, "a gitvm entry with no key is not reachable");
            assert!(gitvm.detail.contains("api key"), "{:?}", gitvm.detail);

            let ready = env_options(&[provider("gitvm", Some("k"))]);
            assert!(ready.iter().find(|o| o.env == "gitvm").expect("gitvm").ready);
        }

        /// The option list and the launcher cannot disagree: anything reported
        /// ready has to actually route, and anything not ready has to refuse.
        /// Two answers to "can I run here" is how a button that works appears
        /// disabled, or worse.
        #[test]
        fn an_option_reported_ready_is_an_option_that_routes() {
            for sandboxes in [
                vec![],
                vec![provider("gitvm", None)],
                vec![provider("gitvm", Some("k"))],
                vec![provider("exe-dev", None), provider("gitvm", Some("k"))],
            ] {
                for option in env_options(&sandboxes) {
                    let env = LaunchEnv::from_key(&option.env).expect("a known environment");
                    assert_eq!(
                        option.ready,
                        env.route(&sandboxes).is_ok(),
                        "{} reported ready={} but routes differently",
                        option.env,
                        option.ready
                    );
                }
            }
        }

        /// Settings order is the switch: whichever ready provider is listed
        /// first is where unpinned launches go.
        #[test]
        fn the_first_ready_entry_in_settings_order_wins() {
            let exe_first = [provider("exe-dev", None), provider("gitvm", Some("k"))];
            assert_eq!(resolve(None, &exe_first), LaunchEnv::ExeDev);
            let gitvm_first = [provider("gitvm", Some("k")), provider("exe-dev", None)];
            assert_eq!(resolve(None, &gitvm_first), LaunchEnv::GitVm);
        }

        /// A provider that cannot be reached is skipped rather than chosen, so
        /// a half-configured gitvm does not strand every launch.
        #[test]
        fn an_unusable_provider_is_skipped_not_chosen() {
            // Only meaningful when the env fallback is absent, same guard as
            // `gitvm_new_requires_a_key`.
            if std::env::var("GITVM_API_KEY").is_ok() {
                return;
            }
            let broken_first = [provider("gitvm", None), provider("exe-dev", None)];
            assert_eq!(resolve(None, &broken_first), LaunchEnv::ExeDev);
            assert_eq!(resolve(None, &[provider("gitvm", None)]), LaunchEnv::Local);
        }

        /// The option names are the owner's own words, and an unknown kind is
        /// ignored instead of guessed at.
        #[test]
        fn keys_are_the_names_the_owner_used() {
            for env in ALL {
                assert_eq!(LaunchEnv::from_key(env.key()), Some(env));
            }
            assert_eq!(LaunchEnv::from_key("local"), Some(LaunchEnv::Local));
            assert_eq!(LaunchEnv::from_key("exe-dev"), Some(LaunchEnv::ExeDev));
            assert_eq!(LaunchEnv::from_key("gitvm"), Some(LaunchEnv::GitVm));
            assert!(LaunchEnv::from_key("e2b").is_none());
        }

        /// The refusal replaces a flat "not wired yet": it names every
        /// environment and says which one is missing what.
        ///
        /// Aimed at gitvm, which now HAS a driver, so this is the last
        /// remaining refusal and it is the honest one — resolved somewhere
        /// that is not configured. The sandbox list deliberately carries no
        /// gitvm entry, so the refusal reads the "not configured" branch
        /// rather than the "no api key" one, which an ambient GITVM_API_KEY
        /// could otherwise flip.
        #[test]
        fn a_refusal_names_every_environment_and_its_state() {
            let err = match LaunchEnv::GitVm.route(&[provider("exe-dev", None)]) {
                Err(message) => message,
                Ok(_) => panic!("gitvm is not configured here"),
            };
            assert!(err.contains("exe-dev: ready"), "{err}");
            assert!(err.contains("local: ready"), "{err}");
            assert!(err.contains("gitvm: not ready"), "{err}");
            assert!(err.contains("settings.sandboxes"), "{err}");
            assert!(err.contains("not configured"), "{err}");
        }

        /// Every option the owner named now has a driver behind it, which is
        /// the ticket read literally: "option 1: local / option 2: exe.dev /
        /// option 3: gitvm". Configuration is the only thing that decides.
        #[test]
        fn every_configured_environment_routes() {
            let both = [provider("exe-dev", None), provider("gitvm", Some("k"))];
            assert!(matches!(LaunchEnv::Local.route(&both), Ok(LaunchRoute::Local)));
            assert!(matches!(
                LaunchEnv::ExeDev.route(&both),
                Ok(LaunchRoute::ExeDev)
            ));
            assert!(matches!(
                LaunchEnv::GitVm.route(&both),
                Ok(LaunchRoute::GitVm)
            ));
        }

        /// One vocabulary for naming a run: the prefix that finds an agent's
        /// sessions again is built from the same pieces as the name itself,
        /// so adoption cannot drift from launch.
        #[test]
        fn a_session_name_always_starts_with_its_agents_prefix() {
            let name = session_name("Builder", "0123456789abcdef");
            assert!(
                name.starts_with(&session_prefix("Builder")),
                "{name} vs {}",
                session_prefix("Builder")
            );
            let listing = format!("{name}\nxnaut-someone-else-1\n{}\n", session_name("b", "z"));
            assert_eq!(sessions_for_handle(&listing, "Builder"), vec![name]);
        }

        #[test]
        fn repository_tmux_sessions_keep_the_entire_run_identity() {
            let first = repository_session_name("acceptancegitvm", "01M3XYK11A97DVDZXFE4R7H21S");
            let second = repository_session_name("acceptancegitvm", "01M3XYK11A97DVDZXFE4R7H21T");
            assert_ne!(first, second);
            assert!(first.ends_with("01m3xyk11a97dvdzxfe4r7h21s"));
            let legacy = session_name("acceptancegitvm", "01M3XYK11A97DVDZXFE4R7H21S");
            let listing = format!("{first}\n{second}\n{legacy}\nxnaut-other-run\n");
            assert_eq!(sessions_for_handle(&listing, "acceptancegitvm"), vec![first, second, legacy]);
        }

        /// Isolation granularity: an environment is keyed by agent AND
        /// project, and two worktrees of one repository are one project.
        /// Keyed any other way, every ticket pays for a cold build cache.
        #[test]
        fn one_environment_per_agent_per_project() {
            let repo = std::path::Path::new("/repos/xnaut");
            let mine = env_key("@builder", repo).unwrap();
            assert_eq!(mine, env_key("builder", repo).unwrap());
            // A different ticket in the same repo is the SAME environment...
            assert_eq!(mine, env_key("builder", repo).unwrap());
            // ...a different agent is not, because `push` mirrors with
            // --delete and sharing would mean erasing each other's work.
            assert_ne!(mine, env_key("reviewer", repo).unwrap());
            // ...and neither is a different repository.
            assert_ne!(mine, env_key("builder", std::path::Path::new("/repos/other")).unwrap());
            assert!(env_key("", repo).is_err());
        }

        /// A worktree resolves to the repository that owns it, and a plain
        /// directory that is no repository at all is its own project rather
        /// than an error — a scratch workspace is a legitimate place to run.
        #[test]
        fn a_worktree_resolves_to_its_repository() {
            let tmp = std::env::temp_dir().join("xnaut-266-project-root");
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(tmp.join("nested")).unwrap();
            assert_eq!(project_root(&tmp.join("nested")), tmp.join("nested"));

            let ok = std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&tmp)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                // Compared canonically: macOS temp is a /var → /private/var
                // symlink, and git answers with the resolved path.
                let want = std::fs::canonicalize(&tmp).unwrap();
                assert_eq!(
                    std::fs::canonicalize(project_root(&tmp.join("nested"))).unwrap(),
                    want
                );
            }
            let _ = std::fs::remove_dir_all(&tmp);
        }

        /// The owner's acceptance test, read literally: "if I pay X USD for
        /// exe.dev then I want to use it, always." Declaring it in settings is
        /// paying for it as far as the app can tell, and an unpinned launch
        /// then goes there rather than quietly staying on his Mac.
        #[test]
        fn a_configured_exe_dev_actually_routes_there() {
            let paid = [provider("exe-dev", None)];
            assert_eq!(resolve(None, &paid), LaunchEnv::ExeDev);
            assert!(matches!(
                LaunchEnv::ExeDev.route(&paid),
                Ok(LaunchRoute::ExeDev)
            ));
            // ...and removing the entry puts it straight back on this machine,
            // with no other switch to remember.
            assert_eq!(resolve(None, &[]), LaunchEnv::Local);
            assert!(matches!(
                LaunchEnv::ExeDev.route(&[]),
                Err(message) if message.contains("exe-dev: not ready")
            ));
        }
    }
}

pub struct GitVmDriver {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl GitVmDriver {
    pub fn new(provider: &SandboxProviderSettings) -> Result<Self, String> {
        let api_key = resolve_sandbox_key(provider).ok_or_else(|| {
            "no GitVM API key (set provider api_key or GITVM_API_KEY)".to_string()
        })?;
        let base_url = provider.base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err("gitvm base_url is empty".into());
        }
        Ok(Self {
            base_url,
            api_key,
            http: reqwest::Client::new(),
        })
    }

    async fn create(&self, spec: &SandboxSpec) -> Result<SandboxHandle, String> {
        let body = serde_json::json!({
            "template": spec.template,
            "vcpus": spec.vcpus,
            "memoryMB": spec.memory_mb,
            "timeout": spec.timeout_secs,
            "exposedPort": spec.exposed_port,
        });
        let resp = self
            .http
            .post(format!("{}/v1/sandboxes", self.base_url))
            .header("X-API-Key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("gitvm create: {e}"))?;
        let value = json_or_err(resp, "create").await?;
        let id = value
            .get("sandboxId")
            .and_then(Value::as_str)
            .ok_or("gitvm create: response missing sandboxId")?
            .to_string();
        let public_url = value
            .get("publicUrl")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        Ok(SandboxHandle { id, public_url })
    }

    async fn status(&self, id: &str) -> Result<String, String> {
        let resp = self
            .http
            .get(format!("{}/v1/sandboxes/{id}", self.base_url))
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| format!("gitvm status: {e}"))?;
        let value = json_or_err(resp, "status").await?;
        Ok(value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    async fn wait_ready(&self, id: &str, max_secs: u64) -> Result<(), String> {
        let start = tokio::time::Instant::now();
        loop {
            let status = self.status(id).await?;
            if status == "running" {
                return Ok(());
            }
            if start.elapsed().as_secs() > max_secs {
                return Err(format!(
                    "gitvm sandbox {id} not ready after {max_secs}s (status: {status})"
                ));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn exec(&self, id: &str, command: &str) -> Result<ExecResult, String> {
        let resp = self
            .http
            .post(format!("{}/v1/sandboxes/{id}/exec", self.base_url))
            .header("X-API-Key", &self.api_key)
            .json(&serde_json::json!({ "command": command }))
            .send()
            .await
            .map_err(|e| format!("gitvm exec: {e}"))?;
        let value = json_or_err(resp, "exec").await?;
        Ok(ExecResult {
            exit_code: value.get("exitCode").and_then(Value::as_i64).unwrap_or(0) as i32,
            stdout: value
                .get("stdout")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            stderr: value
                .get("stderr")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }

    async fn destroy(&self, id: &str) -> Result<(), String> {
        // Best-effort — the sandbox auto-destroys at its timeout regardless, so a
        // missing/failed DELETE is not fatal to a verify run.
        let _ = self
            .http
            .delete(format!("{}/v1/sandboxes/{id}", self.base_url))
            .header("X-API-Key", &self.api_key)
            .send()
            .await;
        Ok(())
    }
}

async fn json_or_err(resp: reqwest::Response, ctx: &str) -> Result<Value, String> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("gitvm {ctx}: reading body: {e}"))?;
    if !status.is_success() {
        let snippet: String = text.chars().take(300).collect();
        return Err(format!("gitvm {ctx}: HTTP {status}: {snippet}"));
    }
    serde_json::from_str(&text).map_err(|e| format!("gitvm {ctx}: bad json: {e}"))
}

/// The `gitvm` CLI — the second way this app talks to GitVM, and the one that
/// works with no configuration (the HTTP driver above needs a provider entry in
/// settings.sandboxes, which is empty on a normal install).
///
/// It is directory-scoped: every command binds to the sandbox belonging to the
/// directory it runs in, which gives one sandbox per directory for free.
///
/// XNAUT-62 will make this the single GitVM entry point for the whole app —
/// today the JS panels still shell out to `gitvm run` / `gitvm pull` with
/// hand-built command strings, which is the duplication that ticket removes.
pub mod cli {
    use super::Value;
    use std::path::Path;

    /// Control plane, same env overrides the CLI honours.
    pub fn api_base() -> String {
        std::env::var("GITVM_API")
            .unwrap_or_else(|_| "http://gitvmd-control-01.tail138398.ts.net:7070".into())
    }

    /// API key: env first, then the CLI's key file — one resolution order.
    pub fn api_key() -> Result<String, String> {
        if let Ok(k) = std::env::var("GITVM_API_KEY") {
            if !k.trim().is_empty() {
                return Ok(k.trim().to_string());
            }
        }
        let file = std::env::var("GITVM_KEY_FILE").unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join(".config/gitvm/key").to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        std::fs::read_to_string(&file)
            .map(|s| s.trim().to_string())
            .map_err(|_| format!("no GitVM API key (set GITVM_API_KEY or write {file})"))
    }

    /// A template's own defaults, straight from the control plane manifest.
    #[derive(Debug, Clone)]
    pub struct TemplateDefaults {
        pub vcpus: u32,
        pub memory_mb: u32,
        pub timeout_secs: u32,
        pub exposed_port: u16,
    }

    /// Reads `/v1/templates` and returns `name`'s declared defaults.
    ///
    /// This exists because `gitvm warm-up` ALWAYS sends vcpus/memoryMB/timeout/
    /// exposedPort, and its fallbacks are agent-desktop's (4 / 8192 / 21600 /
    /// **6080**) for every template. So a `.gitvm.json` that "lets the manifest
    /// decide" by omitting the port actually asks for 6080 — and on a template
    /// that serves :80 the tunnel points at a port nothing listens on, which
    /// looks exactly like a dead sandbox (502). Never guess these numbers:
    /// fetch them and write them.
    pub fn template_defaults(name: &str) -> Result<TemplateDefaults, String> {
        let out = std::process::Command::new("curl")
            .args([
                "-sf",
                "--connect-timeout",
                "5",
                "--max-time",
                "20",
                "-H",
                &format!("X-API-Key: {}", api_key()?),
                &format!("{}/v1/templates", api_base()),
            ])
            .output()
            .map_err(|e| format!("templates: {e}"))?;
        if !out.status.success() {
            return Err("templates: control plane unreachable".into());
        }
        let list: Value =
            serde_json::from_slice(&out.stdout).map_err(|e| format!("templates: bad json: {e}"))?;
        let entry = list
            .as_array()
            .and_then(|a| a.iter().find(|t| t["name"] == name))
            .ok_or_else(|| format!("no such template: {name}"))?;
        // Manifests are inconsistent about case (agent-desktop uses `exposedPort`,
        // pi-dev/app-host use `ExposedPort`), so match either.
        let get = |lower: &str, upper: &str| -> Option<u64> {
            entry["defaults"][lower]
                .as_u64()
                .or_else(|| entry["defaults"][upper].as_u64())
        };
        Ok(TemplateDefaults {
            vcpus: get("vcpus", "VCPUs").unwrap_or(2) as u32,
            memory_mb: get("memoryMB", "MemoryMB").unwrap_or(4096) as u32,
            timeout_secs: get("ttl", "TTL").unwrap_or(7200) as u32,
            exposed_port: get("exposedPort", "ExposedPort")
                .ok_or_else(|| format!("template {name} declares no exposed port"))?
                as u16,
        })
    }

    /// Is this directory's sandbox still known to the control plane?
    ///
    /// `.gitvm/state.json` outlives the sandbox (a reaped or crashed VM leaves
    /// it behind), and `gitvm warm-up` refuses to run while it exists — so a
    /// dead sandbox would wedge the directory forever without this check.
    pub fn state_is_stale(dir: &Path) -> bool {
        let Ok(body) = std::fs::read_to_string(dir.join(".gitvm/state.json")) else {
            return false; // no state at all is not stale, it's clean
        };
        let Ok(state) = serde_json::from_str::<Value>(&body) else {
            return true;
        };
        let Some(id) = state["sandboxId"].as_str() else {
            return true;
        };
        let Ok(key) = api_key() else { return false }; // can't tell → don't destroy
        let out = std::process::Command::new("curl")
            .args([
                "-s",
                "--connect-timeout",
                "5",
                "--max-time",
                "20",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "-H",
                &format!("X-API-Key: {key}"),
                &format!("{}/v1/sandboxes/{id}", api_base()),
            ])
            .output();
        match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "404",
            Err(_) => false,
        }
    }

    /// Makes a service running on THIS Mac reachable at `localhost:<port>`
    /// inside the sandbox, via a reverse ssh forward over the jump host.
    ///
    /// A sandboxed app that calls a local API (NautGate on :8090, a local
    /// Postgres, an Ollama) otherwise gets ECONNREFUSED — the VM's localhost is
    /// its own. Vite's dev-server proxy turns that refusal into a bare HTTP 500
    /// with an empty body, which is what "NautGate error 500" in a built site
    /// actually means (verified 2026-08-01).
    ///
    /// Idempotent: an existing forward makes the new one fail on
    /// ExitOnForwardFailure, which is reported as already-open, not an error.
    pub fn expose_local_port(dir: &Path, port: u16) -> Result<(), String> {
        // Connection details and ssh flags from the one place that reads them
        // (XNAUT-266). This used to parse `.gitvm/state.json` itself and carry
        // its own copy of the flags, including its own default jump host.
        let guest = guest(dir)?;
        let mut args = vec![
            "-f".to_string(), // background once the forward is established
            "-N".to_string(), // no remote command, just the tunnel
        ];
        args.extend(ssh_opts(&guest));
        args.extend([
            "-o".into(),
            "ExitOnForwardFailure=yes".into(),
            "-o".into(),
            "ServerAliveInterval=30".into(),
            "-R".into(),
            format!("{port}:localhost:{port}"),
            format!("root@{}", guest.ip),
        ]);
        // `ssh -f` leaves a live daemon. Pipes captured by output() remain
        // open in that daemon, so waiting for their EOF wedges task launch.
        // A private file retains startup errors without waiting on the tunnel.
        let log = std::env::temp_dir().join(format!("xnaut-forward-{}.log", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let stderr = options.open(&log).map_err(|e| format!("forward log: {e}"))?;
        let status = std::process::Command::new("ssh")
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(stderr)
            .status();
        let err = std::fs::read_to_string(&log).unwrap_or_default();
        let _ = std::fs::remove_file(&log);
        if status.map_err(|e| format!("ssh: {e}"))?.success() {
            return Ok(());
        }
        if err.contains("remote port forwarding failed") {
            return Ok(()); // already forwarded by an earlier spin-up
        }
        Err(format!(
            "could not expose :{port} to the sandbox: {}",
            err.trim()
        ))
    }

    /// HTTP status of the sandbox's public URL, or None if it did not answer.
    pub fn probe(url: &str) -> Option<u32> {
        let out = std::process::Command::new("curl")
            .args([
                "-s",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "--max-time",
                "15",
                url,
            ])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout).trim().parse().ok()
    }

    /// Runs a gitvm subcommand with `dir` as the working directory.
    pub fn exec(dir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
        let home = dirs::home_dir().ok_or("no home dir")?;
        let path = format!(
            "{}/bin:/opt/homebrew/bin:/usr/local/bin:{}",
            home.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        std::process::Command::new("gitvm")
            .args(args)
            .current_dir(dir)
            .env("PATH", path)
            .output()
            .map_err(|e| format!("gitvm {}: {e}", args.join(" ")))
    }

    /// stdout + stderr together — gitvm reports on both.
    pub fn text(out: &std::process::Output) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }

    /// Origin of the first http(s) URL in the output.
    ///
    /// Only the origin: warm-up advertises the template's convenience URL,
    /// which for agent-desktop is the noVNC viewer
    /// (`https://host/vnc.html?autoconnect=1`). Embedding that shows a blank
    /// VNC page instead of the site, so path and query are dropped and what is
    /// left is the host serving the exposed port.
    pub fn extract_url(text: &str) -> Option<String> {
        let start = text.find("http://").or_else(|| text.find("https://"))?;
        let rest = &text[start..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')')
            .unwrap_or(rest.len());
        let full = rest[..end].trim_end_matches(['.', ',']);
        let scheme_end = full.find("//")? + 2;
        let host_end = full[scheme_end..]
            .find('/')
            .map(|i| scheme_end + i)
            .unwrap_or(full.len());
        Some(full[..host_end].to_string())
    }

    /// Creates (or re-attaches to) the sandbox for `dir`, returning its URL.
    pub fn warm_up(dir: &Path) -> Result<String, String> {
        let out = exec(dir, &["warm-up"])?;
        let body = text(&out);
        if !out.status.success() {
            // The CLI returns nonzero for an existing live sandbox. A retry
            // must retain it (and its unpublished files), not stop/recreate it.
            // Saved state alone is insufficient: prove the guest still answers.
            if body.trim().starts_with("gitvm: already warm")
                && ssh(dir, "true").is_ok_and(|probe| probe.status.success())
            {
                return public_url(dir);
            }
            return Err(format!("gitvm warm-up failed: {}", body.trim()));
        }
        if let Some(url) = extract_url(&body) {
            return Ok(url);
        }
        // Not every version prints the URL on warm-up; status always knows it.
        let st = exec(dir, &["status"])?;
        Ok(extract_url(&text(&st)).unwrap_or_default())
    }

    /// The sandbox's real public URL.
    ///
    /// Read from `.gitvm/state.json`, which is the create response the CLI saved
    /// verbatim. NOT from `gitvm status` — that command jq-projects the response
    /// down to `{status, slug, exposedPort, expiresAt}` and drops publicUrl
    /// entirely (verified 2026-08-01), so parsing it can only ever fail. The
    /// warm-up banner is no good either: it prints the template's desktop
    /// convenience URL (`…/vnc.html?…`), not the exposed port.
    pub fn public_url(dir: &Path) -> Result<String, String> {
        let body = std::fs::read_to_string(dir.join(".gitvm/state.json"))
            .map_err(|_| "no sandbox state — is it warm?".to_string())?;
        let state: Value =
            serde_json::from_str(&body).map_err(|e| format!("bad sandbox state: {e}"))?;
        if let Some(url) = state["publicUrl"].as_str().filter(|u| !u.is_empty()) {
            return Ok(url.trim_end_matches('/').to_string());
        }
        // Older states stored only the slug.
        state["slug"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|slug| format!("https://{slug}.nautbox.dev"))
            .ok_or_else(|| "sandbox state has no public URL".to_string())
    }

    /// When the control plane says this sandbox's lease ends, in epoch millis.
    ///
    /// `gitvm status` prints the local state and then the SERVER's view, and only
    /// the server block carries `expiresAt`. Adopting a running sandbox without
    /// this would mean inventing an expiry from the template default, which
    /// overstates the lease by however long the box has already been alive, and
    /// an overstated lease is worse than none: the UI reports a live sandbox
    /// after it has been reaped.
    pub fn expires_ms(dir: &Path) -> Option<i64> {
        let out = exec(dir, &["status"]).ok()?;
        parse_expires_ms(&text(&out))
    }

    /// Pull `expiresAt` out of `gitvm status` output.
    ///
    /// Split out from the command so it can be tested against real output; the
    /// text is two JSON objects with prose between them, so a plain parse fails.
    pub fn parse_expires_ms(body: &str) -> Option<i64> {
        let i = body.find("\"expiresAt\"")?;
        let rest = &body[i..];
        let start = rest.find(':')? + 1;
        let q1 = rest[start..].find('"')? + start + 1;
        let q2 = rest[q1..].find('"')? + q1;
        chrono::DateTime::parse_from_rfc3339(&rest[q1..q2])
            .ok()
            .map(|t| t.timestamp_millis())
    }

    /// Runs a shell command inside the sandbox (rsyncs local changes in first).
    pub fn run(dir: &Path, script: &str) -> Result<std::process::Output, String> {
        exec(dir, &["run", script])
    }

    /// Runs a command that is infrastructure, not a user test, and therefore
    /// must succeed before its caller can continue.
    ///
    /// `Command::output` only means the local process was spawned. The old
    /// Designer treated exit 255 from `gitvm run` as `Ok(Output)`, announced an
    /// empty dev-server message, and then probed an unbound port for seven
    /// minutes. Keep `run` raw for Sandbox Verify, where a red test is data;
    /// use this checked variant for lifecycle/setup work.
    pub fn run_checked(dir: &Path, script: &str) -> Result<std::process::Output, String> {
        require_run_success(run(dir, script)?)
    }

    fn require_run_success(out: std::process::Output) -> Result<std::process::Output, String> {
        if out.status.success() {
            return Ok(out);
        }
        let code = out
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        let detail = text(&out).trim().to_string();
        Err(if detail.is_empty() {
            format!("gitvm run failed with exit {code}")
        } else {
            format!("gitvm run failed with exit {code}: {detail}")
        })
    }

    #[cfg(test)]
    mod checked_run_tests {
        use super::require_run_success;

        #[test]
        fn nonzero_process_output_is_not_a_successful_run() {
            let out = std::process::Command::new("sh")
                .args(["-c", "echo remote setup failed >&2; exit 7"])
                .output()
                .unwrap();
            let error = require_run_success(out).unwrap_err();
            assert!(error.contains("exit 7"), "{error}");
            assert!(error.contains("remote setup failed"), "{error}");
        }
    }

    /// Brings the sandbox workspace back. MUST run before `stop` — teardown
    /// destroys /workspace (XNAUT-40).
    pub fn pull(dir: &Path) -> Result<(), String> {
        let out = exec(dir, &["pull", "."])?;
        if out.status.success() {
            Ok(())
        } else {
            Err(text(&out).trim().to_string())
        }
    }

    /// Destroys the sandbox. Always `pull` first.
    pub fn stop(dir: &Path) -> Result<(), String> {
        let out = exec(dir, &["stop"])?;
        if out.status.success() {
            Ok(())
        } else {
            Err(text(&out).trim().to_string())
        }
    }

    // ── a durable agent run inside the sandbox (XNAUT-266) ──────────────────
    //
    // `run` above shells ONE command and waits for it to end; a launch is a
    // long-lived interactive session whose output streams back as a PTY. Same
    // gap the exe.dev driver had, and deliberately the same answer: tmux inside
    // the sandbox owns the agent, a local PTY hosts an ssh that is only the
    // viewport. Three environments, one shape, one adoption story.
    //
    // The CLI cannot host that itself — `gitvm ssh` opens an interactive shell
    // and takes no command — so these speak the same raw jump-host ssh the CLI
    // does, with the connection details read from the state file the CLI wrote.
    // Every remote invocation below mirrors `cmd_run`: `root@<ip>` over the
    // jump host, then `sudo -u user -H bash -lc`, because /workspace is
    // `user`-owned and an agent running as root would hand back files the pull
    // cannot write over.

    /// How to reach one directory's sandbox, out of the state file the CLI
    /// wrote when it created the box.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Guest {
        pub ip: String,
        pub jump: String,
    }

    /// Read the connection details, or say which of the two things is wrong:
    /// there is no sandbox for this directory, or there is one and it has no
    /// address. A caller deciding whether to warm up needs to tell those apart.
    pub fn guest(dir: &Path) -> Result<Guest, String> {
        let body = std::fs::read_to_string(dir.join(".gitvm/state.json"))
            .map_err(|_| format!("no sandbox for {} — warm one up first", dir.display()))?;
        let state: Value =
            serde_json::from_str(&body).map_err(|e| format!("bad sandbox state: {e}"))?;
        let ip = state["guestIp"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("the sandbox state has no guest ip")?;
        Ok(Guest {
            ip: ip.to_string(),
            jump: state["jump"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("root@gitvmd-control-01.tail138398.ts.net")
                .to_string(),
        })
    }

    /// The ssh flags the CLI itself uses, so a sandbox that answers `gitvm run`
    /// answers these too rather than tripping over host-key prompts.
    ///
    /// `pub(crate)` because it is the ONE place these are written (XNAUT-266).
    /// There were three: this driver, `expose_local_port` below, and
    /// `nautloom::loom_sandbox_stats`, each with its own idea of the timeout
    /// and its own default jump host.
    pub(crate) fn ssh_opts(guest: &Guest) -> Vec<String> {
        vec![
            "-J".into(),
            guest.jump.clone(),
            "-o".into(),
            "UserKnownHostsFile=/dev/null".into(),
            "-o".into(),
            "StrictHostKeyChecking=no".into(),
            "-o".into(),
            "LogLevel=ERROR".into(),
            "-o".into(),
            "ConnectTimeout=15".into(),
        ]
    }

    /// Keepalives, for the ssh that HOSTS a run rather than shelling one
    /// command. Same reasoning as the exe.dev driver: a command that ends
    /// eventually reports a dead network, a session that does not would sit
    /// there silently forever.
    const KEEPALIVE: [&str; 4] = [
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=4",
    ];

    /// One command in the sandbox, as `user`, no rsync.
    ///
    /// NOT `cli::run`: that mirrors the local directory in with `--delete`
    /// first, which would erase whatever the agent has written since it
    /// started. Everything after the launch — asking tmux what is alive,
    /// staging a script — has to go this way instead (the same reason
    /// `loom_sandbox_stats` polls over raw ssh).
    pub fn ssh(dir: &Path, command: &str) -> Result<std::process::Output, String> {
        let guest = guest(dir)?;
        let mut args = ssh_opts(&guest);
        args.push(format!("root@{}", guest.ip));
        args.push(as_user(command));
        std::process::Command::new("ssh")
            .args(&args)
            .output()
            .map_err(|e| format!("ssh to the sandbox: {e}"))
    }

    pub fn repository_exchange(dir: &Path, command: &str, input: &[u8], seconds: u32) -> Result<std::process::Output, String> {
        use std::io::Write;
        use std::process::Stdio;
        let guest = guest(dir)?;
        let mut child = std::process::Command::new("ssh")
            .args(ssh_opts(&guest)).args(KEEPALIVE).args(["-o", "BatchMode=yes"])
            .arg(format!("root@{}", guest.ip))
            .arg(as_user(&format!("timeout {seconds}s bash -lc {}", super::exe::shell_single_quote(command))))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .spawn().map_err(|_| "Could not connect to the sandbox for automatic setup.".to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(input).is_err() {
                let _ = child.kill(); let _ = child.wait();
                return Err("Sandbox setup connection closed before receiving configuration.".into());
            }
        }
        child.wait_with_output().map_err(|_| "Sandbox setup connection failed.".into())
    }

    /// Run something as the sandbox's `user`, in /workspace, under a login
    /// shell — exactly what `gitvm run` wraps its command in.
    fn as_user(command: &str) -> String {
        format!(
            "cd /workspace && sudo -u user -H bash -lc {}",
            super::exe::shell_single_quote(command)
        )
    }

    /// Mirror the worktree into /workspace and make sure tmux is there to hold
    /// the run.
    ///
    /// `gitvm run` IS the CLI's only rsync, so a no-op command is how a caller
    /// asks for a push; the tmux check rides along in the same round trip. It
    /// has to happen before the launch and not inside it: the launch starts
    /// tmux from OUTSIDE the sandbox, so a box without tmux would fail as a
    /// blank screen rather than as an error.
    pub fn push(dir: &Path) -> Result<(), String> {
        // `apt-get update` FIRST, and it is not optional. Measured on a fresh
        // `agent-desktop` sandbox 2026-09-08 (sb-8cac65d0): the image ships
        // with package lists that do not carry tmux, so a bare install answers
        // "Package tmux is not available, but is referred to by another
        // package" and exits non-zero. Every GitVM launch would have failed
        // here — the leg XNAUT-266 shipped without ever running live. With the
        // update in front, the same install produces tmux 3.4.
        //
        // Both steps are quiet and best-effort; the `command -v` after them is
        // the only thing that decides, so a sandbox whose image ALREADY has
        // tmux never pays for either.
        run_checked(
            dir,
            "command -v tmux >/dev/null 2>&1 || { sudo apt-get update -q >/dev/null 2>&1; \
             sudo apt-get install -y -q tmux >/dev/null 2>&1; }; \
             command -v tmux >/dev/null 2>&1 || { echo 'tmux is not installed in this sandbox and \
could not be installed' >&2; exit 1; }",
        )
        .map(|_| ())
    }

    /// Put a run's script in the sandbox and return its remote path.
    ///
    /// base64 for the same reason the exe.dev driver does it: the body carries
    /// a composed prompt full of quotes and newlines, and it would otherwise
    /// have to survive ssh's shell, sudo's shell and bash -lc intact.
    pub fn stage_script(dir: &Path, session: &str, body: &str) -> Result<String, String> {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let path = format!("/workspace/.xnaut/{session}.sh");
        let command = format!(
            "mkdir -p /workspace/.xnaut && printf %s {} | base64 -d > {path} && chmod +x {path}",
            super::exe::shell_single_quote(&STANDARD.encode(body))
        );
        let out = ssh(dir, &command)?;
        if out.status.success() {
            Ok(path)
        } else {
            Err(format!(
                "could not stage the run script in the sandbox: {}",
                text(&out).trim()
            ))
        }
    }

    /// The script body a sandboxed run executes.
    ///
    /// `exec` so the agent becomes the session's own process: when it exits the
    /// tmux session ends, which is what makes `live_sessions_for` mean "still
    /// working" rather than "once did".
    ///
    /// The banner names the one thing that is different here and costs work if
    /// it is a surprise — teardown destroys /workspace (XNAUT-40), so the pull
    /// is what saves the run's output, and the owner should read that at the
    /// top of the pane rather than after losing a day's edits.
    /// `seed` answers the CLI's first-run wizard and MUST come after the `cd`:
    /// it names the trusted directory with `$PWD`. A sandbox is a fresh VM
    /// from a template every time, so unlike exe.dev this one pays the
    /// seeding cost on EVERY launch — which is why it is shell in the script
    /// rather than a one-off provisioning step.
    pub fn run_script(command: &str, session: &str, seed: &str) -> String {
        format!(
            "#!/bin/bash -l\n\
             cd /workspace || {{ echo 'xNAUT: /workspace is missing in this sandbox'; exec bash -l; }}\n\
             {seed}\
             printf '\\033[36mxNAUT: running in the GitVM sandbox, tmux session %s\\033[0m\\n' {}\n\
             printf '\\033[36mxNAUT: this run survives the app; teardown destroys /workspace, so \
pull before you stop it\\033[0m\\n'\n\
             exec env {}\n",
            super::exe::shell_single_quote(session),
            command
        )
    }

    /// The argv a LOCAL pty hosts to start-or-attach a sandboxed run.
    ///
    /// `-tt` because an interactive agent needs a tty on the far side, and `-A`
    /// because starting and adopting are then the same command: a launch that
    /// races an existing session of that name attaches instead of forking a
    /// second agent onto one worktree.
    pub fn launch_argv(guest: &Guest, session: &str, script: &str) -> Vec<String> {
        remote_pty_argv(
            guest,
            &format!(
                "tmux new-session -A -s {} -c /workspace {}",
                super::exe::shell_single_quote(session),
                super::exe::shell_single_quote(script)
            ),
        )
    }

    /// The argv a LOCAL pty hosts to re-attach an existing sandboxed run.
    ///
    /// `attach-session`, never `new-session -A`: adoption must not CREATE. A
    /// name with nothing behind it has to fail, or a finished run comes back as
    /// a bare shell that reads to the roster as a working agent.
    pub fn attach_argv(guest: &Guest, session: &str) -> Vec<String> {
        remote_pty_argv(
            guest,
            &format!(
                "tmux attach-session -t {}",
                super::exe::shell_single_quote(session)
            ),
        )
    }

    fn remote_pty_argv(guest: &Guest, remote: &str) -> Vec<String> {
        let mut argv: Vec<String> = vec!["ssh".into()];
        argv.extend(ssh_opts(guest));
        argv.extend(KEEPALIVE.iter().map(|opt| opt.to_string()));
        argv.push("-tt".into());
        argv.push(format!("root@{}", guest.ip));
        argv.push(as_user(remote));
        argv
    }

    /// The live runs one agent has in this directory's sandbox.
    ///
    /// The sandbox half of adoption. Errors are NOT collapsed into an empty
    /// list, and the EXIT CODE decides rather than the wording: ssh answers 255
    /// for its own failures, tmux answers 1 when no server is up. "The sandbox
    /// is unreachable" and "this agent has no run there" have opposite
    /// consequences for a caller deciding whether to start another one.
    pub fn live_sessions_for(dir: &Path, handle: &str) -> Result<Vec<String>, String> {
        let out = ssh(dir, "tmux list-sessions -F '#{session_name}'")?;
        match out.status.code() {
            Some(0) => Ok(super::launch_env::sessions_for_handle(
                &String::from_utf8_lossy(&out.stdout),
                handle,
            )),
            Some(255) | None => Err(format!(
                "the sandbox for {} is unreachable, so whether @{handle} has a run there is \
unknown: {}",
                dir.display(),
                text(&out).trim()
            )),
            Some(_) => Ok(Vec::new()),
        }
    }

    #[cfg(test)]
    mod launch_tests {
        use super::*;

        fn a_guest() -> Guest {
            Guest {
                ip: "10.0.0.7".into(),
                jump: "root@control".into(),
            }
        }

        /// Everything remote runs as `user`, never as root. /workspace is
        /// user-owned (the CLI chowns it on every push), so an agent running as
        /// root writes files the pull back cannot overwrite next time.
        #[test]
        fn a_sandboxed_run_is_never_root() {
            let argv = launch_argv(&a_guest(), "xnaut-a-1", "/workspace/.xnaut/x.sh");
            let remote = argv.last().unwrap();
            assert!(remote.starts_with("cd /workspace && sudo -u user -H bash -lc"), "{remote}");
            assert!(argv.contains(&"root@10.0.0.7".to_string()), "{argv:?}");
            assert!(argv.contains(&"-J".to_string()) && argv.contains(&"root@control".to_string()));
        }

        /// Launch may create; adoption may not. Attaching with `new-session -A`
        /// is how a finished run comes back as a bare shell that the roster
        /// reads as a working agent.
        #[test]
        fn adoption_attaches_and_never_creates() {
            let attach = attach_argv(&a_guest(), "xnaut-a-1").join(" ");
            assert!(attach.contains("tmux attach-session -t"), "{attach}");
            assert!(!attach.contains("new-session"), "{attach}");
            let launch = launch_argv(&a_guest(), "xnaut-a-1", "/workspace/x.sh").join(" ");
            assert!(launch.contains("tmux new-session -A -s"), "{launch}");
        }

        /// A viewport that hosts a session rather than a command needs
        /// keepalives, or a dropped link is indistinguishable from an agent
        /// thinking.
        #[test]
        fn the_viewport_ssh_has_keepalives_and_a_tty() {
            let argv = launch_argv(&a_guest(), "s", "/workspace/x.sh");
            assert!(argv.contains(&"ServerAliveInterval=15".to_string()), "{argv:?}");
            assert!(argv.contains(&"-tt".to_string()), "{argv:?}");
        }

        /// The run script hands the session to the agent itself, so a session
        /// that is alive means an agent that is working.
        #[test]
        fn the_run_script_execs_the_agent() {
            let body = run_script("claude --dangerously-skip-permissions", "xnaut-a-1", "");
            assert!(body.contains("\nexec env claude"), "{body}");
            assert!(body.contains("pull before you stop it"), "{body}");
        }

        /// A directory with no sandbox says so, rather than producing an argv
        /// that would ssh nowhere.
        #[test]
        fn a_directory_with_no_sandbox_says_so() {
            let dir = std::env::temp_dir().join("xnaut-266-no-sandbox");
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let error = guest(&dir).unwrap_err();
            assert!(error.contains("no sandbox"), "{error}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// exe.dev as a verify runner, over the owner's SSH identity (XNAUT-252).
///
/// One PERSISTENT VM for all verifies, one directory per project under the VM
/// user's home. Persistence is the point of choosing exe.dev over a fresh
/// GitVM sandbox for Rust targets: the toolchain and the target/ cache survive
/// between runs, so a verify costs minutes warm instead of half an hour cold.
/// It follows that there is no pull-before-stop dance here (XNAUT-40 was a
/// teardown problem, and nothing is torn down) — and no teardown means a red
/// run leaves the workdir exactly as it failed, inspectable over ssh.
///
/// Control plane and exec both ride plain `ssh`: `ssh exe.dev <cmd>` for
/// ls/new, `ssh <vm>.exe.xyz <cmd>` to run a step. The xNAUT plugin's HTTPS
/// API token is read-only scoped (ls/whoami), which is right for a pane and
/// useless for a runner; the owner's registered SSH key is the credential
/// that can actually do the work, and the app runs as the owner.
pub mod exe {
    use std::path::Path;

    /// The one VM verifies run on. Created on first use, never destroyed here.
    pub const VM: &str = "nautbox-verify";
    const CONTROL: &str = "exe.dev";

    /// BatchMode so a missing/unregistered key fails in seconds with ssh's
    /// own message instead of hanging a verify on an invisible prompt.
    const SSH_OPTS: [&str; 6] = [
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ConnectTimeout=15",
    ];

    // Acceptance may read existing SSH identities/trust but cannot add host keys
    // or attach to the owner's ControlMaster sockets. Production flags unchanged.
    fn ssh_opts()->Vec<String> {
        let mut out=Vec::new();
        if crate::loop_acceptance::ENABLED {
            for value in ["-o","StrictHostKeyChecking=yes","-o","UpdateHostKeys=no","-o","ControlMaster=no","-o","ControlPath=none"] {out.push(value.into());}
        }
        out.extend(SSH_OPTS.iter().map(|s|s.to_string()));out
    }

    fn ssh(dest: &str, command: &str) -> Result<std::process::Output, String> {
        std::process::Command::new("ssh")
            .args(ssh_opts())
            .arg(dest)
            .arg(command)
            .output()
            .map_err(|e| format!("ssh {dest}: {e}"))
    }

    /// stdout + stderr together, like `cli::text`.
    pub fn text(out: &std::process::Output) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }

    pub fn codex_auth_ready() -> Result<(), String> {
        let out = ssh(&vm_host(), "bash -lc 'timeout 15s codex login status >/dev/null 2>&1'")?;
        if out.status.success() { Ok(()) } else {
            Err("Codex authentication is unavailable on exe.dev (nautbox-verify). Open its terminal and configure Codex authentication before dispatching; no agent was started.".into())
        }
    }

    fn vm_host() -> String {
        format!("{VM}.exe.xyz")
    }

    /// Where a project's checkout lives on the VM, relative to $HOME —
    /// home-relative so no sudo is ever needed for the workspace.
    ///
    /// Errors on a project that slugs to nothing. `loops_run_sandbox_node`
    /// defaults `project` to "" when a run carries none, so this is reachable
    /// rather than theoretical, and the consequence is destructive: every such
    /// run would share `verify/` and `push`'s `rsync --delete` would wipe
    /// whatever the previous project left there.
    pub fn workdir(project: &str) -> Result<String, String> {
        let slug: String = project
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        if !slug.chars().any(|c| c.is_ascii_alphanumeric()) {
            return Err(format!(
                "exe.dev verify needs a project name to key its workdir; got {project:?}"
            ));
        }
        Ok(format!("verify/{slug}"))
    }

    /// Where ONE AGENT's checkout of one project lives on the VM
    /// (XNAUT-266, isolation granularity).
    ///
    /// Separate from `workdir` above, which keys the VERIFIER's directory by
    /// project alone. An agent needs its own, because two agents sharing one
    /// directory would take turns erasing each other with `push`'s `--delete`.
    ///
    /// Keyed by the REPOSITORY, never the worktree, and that is the whole
    /// point: the agent's next ticket is a different worktree of the same
    /// repo, lands in the same remote directory, and finds `target/` and
    /// `node_modules` exactly as it left them — those two are `push`'s
    /// exclusions, so the mirror never touches them. Warm from the second run
    /// onward is the reason to pay for a persistent VM; keyed per ticket, it
    /// would be cold every time.
    pub fn agent_workdir(handle: &str, project_root: &Path) -> Result<String, String> {
        Ok(format!(
            "agents/{}",
            super::launch_env::env_key(handle, project_root)?
        ))
    }

    /// What `ensure` has to do to get from the control plane's listing to a VM
    /// that will actually answer ssh.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Warm {
        Create,
        Start,
        Ready,
    }

    /// Read the VM's state out of an `ls --json <name>` payload.
    ///
    /// Measured 2026-09-03 against the live control plane: the payload is
    /// `{"vms":[{...,"vm_name":"nautbox-verify","status":"stopped",...}]}`, and
    /// a name with no VM behind it returns `{"vms":[]}` with exit 0. Matching
    /// `vm_name` exactly replaces the old `listing.contains(VM)`, which would
    /// have accepted a VM merely named with ours as a prefix.
    pub(crate) fn warm_action(listing: &str) -> Result<Warm, String> {
        let parsed: serde_json::Value = serde_json::from_str(listing.trim())
            .map_err(|e| format!("exe.dev `ls --json {VM}` returned no JSON ({e}): {listing}"))?;
        let found = parsed["vms"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|vm| vm["vm_name"].as_str() == Some(VM));
        Ok(match found {
            None => Warm::Create,
            // Anything that is not "running" needs a boot. Treating an unknown
            // status as Ready is how a stopped VM used to reach `push`.
            Some(vm) if vm["status"].as_str() == Some("running") => Warm::Ready,
            Some(_) => Warm::Start,
        })
    }

    /// Make sure the VM exists AND is running. Returns its public URL.
    /// Idempotent: create only on absence, boot only when it is not running.
    ///
    /// The boot half is new and it is what blocked every exe.dev verify. `ssh
    /// <vm>.exe.xyz` does not wake a stopped VM: measured 2026-09-03, it prints
    /// `VM "nautbox-verify" is not running.` and exits 1 in under two seconds.
    /// Nothing in the path started one, so a VM that had idled out (this one
    /// had, since 2026-09-01) could only ever fail at `push`.
    pub fn ensure() -> Result<String, String> {
        let out = ssh(CONTROL, &format!("ls --json {VM}"))?;
        if !out.status.success() {
            return Err(format!(
                "exe.dev control plane refused `ls` — is this machine's ssh key registered? {}",
                text(&out).trim()
            ));
        }
        match warm_action(&String::from_utf8_lossy(&out.stdout))? {
            Warm::Ready => {}
            Warm::Create => {
                let created = ssh(CONTROL, &format!("new --name {VM} --image exeuntu"))?;
                if !created.status.success() {
                    return Err(format!("exe.dev new {VM}: {}", text(&created).trim()));
                }
            }
            Warm::Start => {
                let started = ssh(CONTROL, &format!("restart {VM}"))?;
                if !started.status.success() {
                    return Err(format!("exe.dev restart {VM}: {}", text(&started).trim()));
                }
            }
        }
        wait_ready()?;
        Ok(format!("https://{}", vm_host()))
    }

    /// Poll until the VM answers ssh.
    ///
    /// `restart` returns as soon as the control plane has scheduled the boot,
    /// so returning straight from `ensure` would only move the failure into
    /// `push`. Measured 2026-09-03: `restart` came back in 2.9s and the VM
    /// answered on the first probe after it, so the budget below is slack for
    /// a cold create rather than the expected cost.
    fn wait_ready() -> Result<(), String> {
        const ATTEMPTS: u32 = 20;
        const GAP: std::time::Duration = std::time::Duration::from_secs(3);
        let mut last = String::new();
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(GAP);
            }
            match ssh(&vm_host(), "true") {
                Ok(out) if out.status.success() => return Ok(()),
                Ok(out) => last = text(&out).trim().to_string(),
                Err(error) => last = error,
            }
        }
        Err(format!(
            "{VM} did not answer ssh within {}s of warm-up: {last}",
            ATTEMPTS * GAP.as_secs() as u32
        ))
    }

    /// Ship the repo to the VM's project dir. `--delete` keeps it an exact
    /// mirror of the checkout; target/ and node_modules are excluded because
    /// the VM builds its own (that cache surviving is the feature).
    pub fn push(dir: &Path, project: &str) -> Result<(), String> {
        push_to(dir, &workdir(project)?)
    }

    /// The same mirror, to a workdir the caller already knows. `push` is this
    /// with the verifier's key; a launch brings the agent's own.
    pub fn push_to(dir: &Path, workdir: &str) -> Result<(), String> {
        let host = vm_host();
        let mkdir = ssh(&host, &format!("mkdir -p {workdir}"))?;
        if !mkdir.status.success() {
            return Err(format!("mkdir on {VM}: {}", text(&mkdir).trim()));
        }
        let out = std::process::Command::new("rsync")
            .args([
                "-az",
                "--delete",
                "--exclude",
                "target",
                "--exclude",
                "node_modules",
                "-e",
                &format!("ssh {}", ssh_opts().join(" ")),
                &format!("{}/", dir.display()),
                &format!("{host}:{workdir}/"),
            ])
            .output()
            .map_err(|e| format!("rsync: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("rsync to {VM}: {}", text(&out).trim()))
        }
    }

    /// Avoid returning credential-helper output in errors.
    pub fn repository_command(command: &str) -> Result<(), String> {
        let out = ssh(&vm_host(), &format!("timeout 180s bash -lc {}", shell_single_quote(command)))?;
        if out.status.success() { Ok(()) }
        else { Err("Repository preparation on exe.dev failed; check worker repository access and Git LFS. Existing runs were preserved.".into()) }
    }

    /// Bootstrap payloads can carry a fleet enrollment credential. Use stdin,
    /// never argv, base64 command text, debug logs, or task artifacts.
    pub fn repository_exchange(command: &str, input: &[u8], seconds: u32) -> Result<std::process::Output, String> {
        use std::io::Write;
        use std::process::Stdio;
        let mut child = std::process::Command::new("ssh")
            .args(ssh_opts())
            .args(["-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3"])
            .arg(vm_host())
            .arg(format!("timeout {seconds}s bash -lc {}", shell_single_quote(command)))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .spawn().map_err(|_| "Could not connect to the worker for automatic setup.".to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(input).is_err() {
                let _ = child.kill(); let _ = child.wait();
                return Err("Worker setup connection closed before receiving configuration.".into());
            }
        }
        child.wait_with_output().map_err(|_| "Worker setup connection failed.".into())
    }

    pub fn repository_probe(workdir: &str) -> Result<serde_json::Value, String> {
        let out = ssh(&vm_host(), &repository_probe_command(workdir))?;
        if !out.status.success() { return Err("Could not read worker progress; existing data is retained.".into()); }
        serde_json::from_slice(&out.stdout).map_err(|_| "Invalid worker progress".into())
    }

    pub fn repository_probe_command(workdir: &str) -> String {
        let script = r#"import json, pathlib, subprocess, sys
p = pathlib.Path.home() / sys.argv[1]
phase = (p / '.git/xnaut-phase').read_text().strip()
pid = None
if phase == 'running':
    parent = int((p / '.git/xnaut-supervisor.pid').read_text())
    child = subprocess.run(['pgrep', '-o', '-P', str(parent)], capture_output=True, text=True, timeout=5)
    if child.returncode == 0: pid = int(child.stdout.strip())
head = subprocess.run(['git', '-C', str(p), 'rev-parse', 'HEAD'], capture_output=True, text=True, timeout=5).stdout.strip()
print(json.dumps({'agent_pid': pid, 'head': head, 'phase': phase}))
"#;
        format!("timeout 20s python3 -c {} {}", shell_single_quote(script), shell_single_quote(workdir))
    }

    pub fn repository_file(workdir: &str, relative: &str, body: &str) -> Result<(), String> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        repository_command(&format!("printf %s {} | base64 -d > {}", shell_single_quote(&STANDARD.encode(body)), shell_single_quote(&format!("{workdir}/{relative}"))))
    }

    /// Run one verify step in the project dir. A login shell so per-user
    /// toolchains (rustup's ~/.cargo/env, nvm) are on PATH the way they are
    /// for a human ssh-ing in.
    pub fn run(project: &str, command: &str) -> Result<std::process::Output, String> {
        let workdir = workdir(project)?;
        let quoted = format!("cd {workdir} && {{ {command}; }}");
        let wrapped = format!(
            "bash -lc {}",
            shell_single_quote(&quoted)
        );
        ssh(&vm_host(), &wrapped)
    }

    /// Single-quote for a remote shell: close, escape, reopen.
    pub(crate) fn shell_single_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', r"'\''"))
    }

    // ── a durable agent run on the VM (XNAUT-266, slice 2) ──────────────────
    //
    // `run` above shells ONE command and waits for it to end. A launch is the
    // opposite shape: a long-lived interactive session whose output has to
    // stream back as a PTY. The route is `ssh -tt` into tmux on the VM, which
    // makes the local PTY a viewport onto a session that outlives it. That is
    // exactly what zellij is for a local run, and the symmetry is deliberate:
    // the same adoption story then works on both sides.

    /// Keepalives, for the ssh that HOSTS a run rather than shelling one
    /// command.
    ///
    /// `run` waits on something that ends, so a dead network eventually
    /// surfaces as a failed command. A launch waits on something that does
    /// not, so without these a dropped link leaves the viewport open forever
    /// showing nothing, with no error: the silence this project keeps paying
    /// for. Four missed probes at 15s drops it inside a minute, and the drop
    /// is recoverable rather than fatal because the tmux session is on the VM,
    /// not in the ssh.
    const KEEPALIVE: [&str; 4] = [
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=4",
    ];

    // The naming vocabulary is NOT this module's. It lives in `launch_env`
    // because all three environments have to answer the same question the same
    // way, or "one contract out" is a slogan rather than a guarantee: local
    // zellij, the exe.dev VM and a GitVM sandbox are each asked "which sessions
    // belong to @builder?" and each must derive the answer from the handle
    // alone. These re-exports keep the call sites reading in their own idiom.
    pub use super::launch_env::{session_name, session_prefix, sessions_for_handle};

    /// The live remote runs belonging to one agent, asked of the VM.
    ///
    /// The remote half of adoption, and the reason a restart cannot orphan a
    /// paid VM: after a restart this rebuilds the prefix from the handle and
    /// asks tmux what is still up.
    ///
    /// Errors are NOT collapsed into an empty list. "The VM is unreachable"
    /// and "this agent has no run there" have opposite consequences, and
    /// conflating them is precisely how a running agent becomes invisible. The
    /// EXIT CODE decides, not the wording: ssh answers 255 for its own
    /// failures, while tmux answers 1 when no server is up. Both tmux messages
    /// were measured on the VM 2026-09-03 and they differ ("no server running
    /// on /tmp/tmux-1000/default" once a socket has existed, "error connecting
    /// to ... (No such file or directory)" before that), so matching on text
    /// would have been a coin flip.
    pub fn live_sessions_for(handle: &str) -> Result<Vec<String>, String> {
        let out = ssh(&vm_host(), "tmux list-sessions -F '#{session_name}'")?;
        match out.status.code() {
            Some(0) => Ok(sessions_for_handle(
                &String::from_utf8_lossy(&out.stdout),
                handle,
            )),
            Some(255) | None => Err(format!(
                "{VM} is unreachable, so whether @{handle} has a run there is unknown: {}",
                text(&out).trim()
            )),
            Some(_) => Ok(Vec::new()),
        }
    }

    /// Put a run's script on the VM and return its remote path.
    ///
    /// base64 on purpose. The body is a shell script that travels through
    /// ssh's own shell before anything writes it down, and it carries a
    /// composed prompt full of quotes, newlines and backticks. Encoding it
    /// means none of that has to survive three levels of quoting, which is the
    /// same reason the local path puts its payload in a file rather than in
    /// the layout (`agents.rs::prepare_zellij_run`, and the seventeen empty
    /// panes of 2026-08-09).
    pub fn stage_script(project: &str, session: &str, body: &str) -> Result<String, String> {
        stage_script_in(&workdir(project)?, session, body)
    }

    /// The same staging, into a workdir the caller already knows.
    pub fn stage_script_in(workdir: &str, session: &str, body: &str) -> Result<String, String> {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let dir = format!("{workdir}/.xnaut");
        let path = format!("{dir}/{session}.sh");
        // ponytail: one argv-sized blob. A composed prompt is kilobytes and
        // Linux ARG_MAX is megabytes, so the ceiling is far off; stdin-piping
        // through the ssh is the fix if a prompt ever reaches it.
        let command = format!(
            "mkdir -p {} && printf %s {} | base64 -d > {} && chmod +x {}",
            shell_single_quote(&dir),
            shell_single_quote(&STANDARD.encode(body)),
            shell_single_quote(&path),
            shell_single_quote(&path)
        );
        let out = ssh(&vm_host(), &command)?;
        if out.status.success() {
            Ok(path)
        } else {
            Err(format!(
                "could not stage the run script on {VM}: {}",
                text(&out).trim()
            ))
        }
    }

    /// A home-relative remote path, made absolute BY THE REMOTE SHELL.
    ///
    /// `workdir` is home-relative on purpose, and an ssh command lands in
    /// $HOME, so `run` and `push` never needed more than that. tmux does.
    /// `new-session -c` is resolved by the tmux SERVER, whose working
    /// directory is not the client's, and a relative path there makes the
    /// whole `new-session` do nothing.
    ///
    /// It does nothing SILENTLY, which is why this is a function with a
    /// comment rather than an inline string. Measured on the VM 2026-09-03: a
    /// relative `-c` exits 0, prints not one word, and leaves no session
    /// behind, while the `ssh -tt` in front of it still paints a tmux-shaped
    /// screen and exits 0 too. A launch therefore looked completely successful
    /// and there was no agent anywhere.
    ///
    /// `"$HOME"/'rel'` and not `"$HOME/rel"`: the shell expands the first half
    /// and takes the second literally, so nothing in the path can ever be
    /// re-parsed as shell.
    fn remote_path(rel: &str) -> String {
        format!("\"$HOME\"/{}", shell_single_quote(rel))
    }

    /// The script body a remote run executes.
    ///
    /// A login shell so `/usr/local/bin` and any per-user toolchain are on
    /// PATH exactly as they are for a human ssh-ing in, and `exec` so the
    /// agent becomes the session's own process: when it exits the tmux session
    /// ends, which is what makes `live_sessions_for` mean "still working"
    /// rather than "once did".
    ///
    /// The banner is not decoration. A remote run is missing things a local
    /// one has (see `agent_profiles::launch_on_exe_dev`), and the one place
    /// the owner is certainly looking when he wonders why is the top of the
    /// pane.
    /// `seed` answers the CLI's first-run wizard and MUST come after the `cd`:
    /// it names the trusted directory with `$PWD`. See
    /// `launch_env::onboarding_seed`.
    ///
    /// `exec env` rather than `exec`: the command line carries env
    /// assignments in front of the binary (`remote_command`), and bash's
    /// `exec` takes the first word as the program. Every exe.dev launch died
    /// on `exec: ANTHROPIC_MODEL=...: not found` on 2026-09-14 (XNAUT-396).
    pub fn run_script(workdir: &str, command: &str, session: &str, seed: &str) -> String {
        format!(
            "#!/bin/bash -l\n\
             cd {} || {{ echo \"xNAUT: {} is not on {VM}\"; exec bash -l; }}\n\
             {}\
             printf '\\033[36mxNAUT: running on {VM}.exe.xyz in tmux session %s\\033[0m\\n' {}\n\
             printf '\\033[36mxNAUT: this run survives the app; reattach finds it by name\\033[0m\\n'\n\
             exec env {}\n",
            remote_path(workdir),
            workdir,
            seed,
            shell_single_quote(session),
            command
        )
    }

    /// The argv a LOCAL pty hosts to start-or-attach a remote run.
    ///
    /// `-tt` because an interactive agent needs a tty on the far side. `-A`
    /// because starting and adopting are then the SAME command: a launch that
    /// races an existing session of that name attaches to it instead of
    /// forking a second agent onto one worktree. Verified against the VM
    /// 2026-09-03; the second invocation attached and never ran its command.
    pub fn launch_argv(session: &str, workdir: &str, script: &str) -> Vec<String> {
        remote_pty_argv(&format!(
            "tmux new-session -A -s {} -c {} {}",
            shell_single_quote(session),
            remote_path(workdir),
            remote_path(script)
        ))
    }

    /// The argv a LOCAL pty hosts to re-attach an existing remote run.
    ///
    /// `attach-session`, never `new-session -A`: adoption must not CREATE.
    /// Creating on adoption is how a finished run comes back as a bare shell
    /// that reads to the roster as a working agent, which is the local ghost
    /// of XNAUT-260 with a monthly bill attached.
    pub fn attach_argv(session: &str) -> Vec<String> {
        remote_pty_argv(&format!(
            "tmux attach-session -t {}",
            shell_single_quote(session)
        ))
    }

    fn remote_pty_argv(remote: &str) -> Vec<String> {
        let mut argv: Vec<String> = vec!["ssh".into()];
        argv.extend(ssh_opts().iter().map(|opt| opt.to_string()));
        argv.extend(KEEPALIVE.iter().map(|opt| opt.to_string()));
        argv.push("-tt".into());
        argv.push(vm_host());
        argv.push(remote.into());
        argv
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn workdir_slugs_the_project() {
            assert_eq!(workdir("XNAUT").unwrap(), "verify/xnaut");
            assert_eq!(workdir("Näut/2").unwrap(), "verify/n-ut-2");
        }

        /// An agent's directory is keyed by the REPOSITORY, so its next ticket
        /// lands in the same place and finds `target/` warm — `push` excludes
        /// `target` and `node_modules`, so the mirror never touches them, and
        /// that is the whole reason a persistent VM beats a fresh sandbox
        /// (XNAUT-266). Keyed per worktree it would be cold every ticket.
        ///
        /// It is also NOT the verifier's directory: sharing one would make two
        /// `--delete` mirrors take turns erasing each other.
        #[test]
        fn an_agents_workdir_is_per_agent_per_repository() {
            let repo = std::path::Path::new("/repos/xnaut");
            let mine = agent_workdir("@builder", repo).unwrap();
            assert!(mine.starts_with("agents/builder/"), "{mine}");
            assert_eq!(mine, agent_workdir("builder", repo).unwrap());
            assert_ne!(mine, agent_workdir("reviewer", repo).unwrap());
            assert_ne!(
                mine,
                agent_workdir("builder", std::path::Path::new("/repos/other")).unwrap()
            );
            assert_ne!(mine, workdir("xnaut").unwrap());
            assert!(agent_workdir("", repo).is_err());
        }

        /// A nameless run must not be handed `verify/` itself: `push` rsyncs
        /// with `--delete`, so sharing that directory would delete another
        /// project's checkout. `loops_run_sandbox_node` really does default the
        /// project to "" when a run has none.
        #[test]
        fn a_nameless_project_is_refused_rather_than_sharing_the_root() {
            for empty in ["", "   ", "///", "--"] {
                let err = match workdir(empty) {
                    Err(error) => error,
                    Ok(dir) => panic!("{empty:?} should be refused, got {dir}"),
                };
                assert!(
                    err.contains("project name"),
                    "error should say what is missing: {err}"
                );
            }
        }

        /// The payloads below are verbatim from the live control plane on
        /// 2026-09-03, trimmed to the fields `warm_action` reads.
        #[test]
        fn a_stopped_vm_is_started_not_assumed_ready() {
            let stopped = r#"{"vms":[{"dns_name":"nautbox-verify.exe.xyz","status":"stopped","vm_name":"nautbox-verify"}]}"#;
            assert_eq!(warm_action(stopped).unwrap(), Warm::Start);

            let running = r#"{"vms":[{"dns_name":"nautbox-verify.exe.xyz","status":"running","vm_name":"nautbox-verify"}]}"#;
            assert_eq!(warm_action(running).unwrap(), Warm::Ready);

            // An absent VM lists as an empty array with exit 0, not an error.
            assert_eq!(warm_action(r#"{"vms":[]}"#).unwrap(), Warm::Create);

            // A near-miss name is not our VM. `listing.contains(VM)` accepted
            // this; matching vm_name exactly does not.
            let other = r#"{"vms":[{"status":"running","vm_name":"nautbox-verify-2"}]}"#;
            assert_eq!(warm_action(other).unwrap(), Warm::Create);

            // A status we have never seen is not proof the VM is up.
            let odd = r#"{"vms":[{"status":"provisioning","vm_name":"nautbox-verify"}]}"#;
            assert_eq!(warm_action(odd).unwrap(), Warm::Start);
        }

        /// The control plane answering with a human sentence instead of JSON is
        /// how an expired account shows up. That must be an error carrying what
        /// it said, not a silent Ready.
        #[test]
        fn non_json_from_the_control_plane_is_an_error_that_quotes_it() {
            let err = warm_action("your session has expired").unwrap_err();
            assert!(
                err.contains("your session has expired"),
                "error should quote what came back: {err}"
            );
        }

        #[test]
        fn quoting_survives_single_quotes() {
            assert_eq!(shell_single_quote("it's"), r"'it'\''s'");
        }

        /// THE adoption test. Nothing about finding a remote run again may
        /// depend on anything held in memory: after a restart all that exists
        /// is the handle, read off disk from the profile store. So the name a
        /// launch picks and the prefix a restart searches with have to be two
        /// views of one derivation.
        #[test]
        fn a_restart_can_rebuild_the_session_name_from_the_handle_alone() {
            let name = session_name("nautbot", "a1b2c3d4e5f6");
            // The run id is truncated the same way the local path truncates
            // it, so the two vocabularies stay one.
            assert_eq!(name, "xnaut-nautbot-a1b2c3d4");
            // A process that knows ONLY the handle rebuilds the prefix and
            // finds it.
            assert!(name.starts_with(&session_prefix("nautbot")));
            assert_eq!(
                sessions_for_handle(&format!("other-thing\n{name}\nxnaut-librarian-99"), "nautbot"),
                vec![name.clone()]
            );
            // ...and does not claim another agent's run.
            assert!(sessions_for_handle(&name, "librarian").is_empty());
        }

        /// A launch and an adoption ask tmux different questions on purpose.
        /// `-A` may create, because that is a launch; attach may not, because
        /// creating on adoption resurrects a finished run as a bare shell that
        /// reads as a working agent.
        #[test]
        fn adoption_attaches_and_never_creates() {
            let launch = launch_argv("xnaut-nautbot-a1b2c3d4", "verify/x", "verify/x/.xnaut/r.sh");
            let remote = launch.last().expect("the remote command is last");
            assert!(remote.contains("new-session -A"), "{remote}");
            assert!(remote.contains("verify/x/.xnaut/r.sh"), "{remote}");

            let adopt = attach_argv("xnaut-nautbot-a1b2c3d4");
            let remote = adopt.last().expect("the remote command is last");
            assert!(remote.contains("attach-session"), "{remote}");
            assert!(
                !remote.contains("new-session"),
                "adoption must not create: {remote}"
            );
        }

        /// Both argvs must ask for a tty and for keepalives, or the two
        /// failure modes this slice exists to make legible come back: an
        /// agent with no tty on the far side, and a dropped link that hangs
        /// the viewport open showing nothing forever.
        #[test]
        fn a_hosted_run_gets_a_tty_and_a_deadline() {
            for argv in [
                launch_argv("s", "d", "r.sh"),
                attach_argv("s"),
            ] {
                assert_eq!(argv[0], "ssh");
                assert!(argv.iter().any(|a| a == "-tt"), "{argv:?}");
                assert!(
                    argv.iter().any(|a| a == "ServerAliveInterval=15"),
                    "{argv:?}"
                );
                assert!(argv.iter().any(|a| a == &vm_host()), "{argv:?}");
            }
        }

        /// Every path tmux is handed must be absolute, expanded by the remote
        /// shell. A relative one costs the whole launch and says nothing: on
        /// the VM 2026-09-03 `new-session -c verify/xnaut` exited 0, printed
        /// nothing and created no session, while the `ssh -tt` still painted a
        /// tmux screen and exited 0. This test is the only thing standing
        /// between that and a launch that reports success with no agent.
        #[test]
        fn tmux_is_never_handed_a_relative_path() {
            let launch = launch_argv("s", "verify/x", "verify/x/.xnaut/r.sh");
            let remote = launch.last().expect("the remote command is last");
            assert!(
                remote.contains(r#"-c "$HOME"/'verify/x'"#),
                "the start directory must be absolute: {remote}"
            );
            assert!(
                remote.contains(r#""$HOME"/'verify/x/.xnaut/r.sh'"#),
                "the script path must be absolute: {remote}"
            );
            // The script cds for itself too, in case tmux ever loses the -c.
            assert!(
                run_script("verify/x", "claude", "s", "").contains(r#"cd "$HOME"/'verify/x'"#),
                "the script must cd absolutely as well"
            );
        }

        /// The script has to survive a prompt that is hostile to shells, and
        /// it has to `exec` so the agent IS the session: a wrapper left alive
        /// after the agent exits would keep the session listed, and every
        /// liveness answer downstream would then be a lie.
        #[test]
        fn the_run_script_execs_the_agent_and_survives_a_quoted_workdir() {
            let script = run_script("verify/it's", "claude --model x", "xnaut-a-1", "");
            assert!(script.starts_with("#!/bin/bash -l"), "{script}");
            assert!(script.contains(r"'verify/it'\''s'"), "{script}");
            assert!(script.contains("exec env claude --model x"), "{script}");
            // `$HOME` must reach the shell unquoted or it is a literal.
            assert!(script.contains("\"$HOME\"/"), "{script}");
        }

        /// An unreachable VM must NOT read as "this agent has no run". The
        /// exit code is the discriminator because the two tmux messages for
        /// "no server" differ between a socket that never existed and one that
        /// went away, both measured on the VM 2026-09-03.
        #[test]
        fn no_sessions_and_no_vm_are_different_answers() {
            for message in [
                "no server running on /tmp/tmux-1000/default",
                "error connecting to /tmp/tmux-1000/default (No such file or directory)",
            ] {
                assert!(
                    sessions_for_handle(message, "nautbot").is_empty(),
                    "a tmux complaint is not a session name: {message}"
                );
            }
        }

        /// `warm_action` against the LIVE control plane, not a pasted payload.
        ///
        /// The pure test above pins the decision; this pins the shape it reads.
        /// `ls --json <name>` is the one thing here that can drift under us
        /// without any code changing, and a drift would land as `Warm::Create`
        /// on an existing VM (`new` fails, verify dies) or a parse error.
        ///
        /// It deliberately does NOT stop the VM first to exercise the boot
        /// branch. The VM is shared, so halting it would break whatever else is
        /// mid-verify; and it cannot be held down anyway, because exe.dev
        /// restarts an in-guest halt within seconds (measured 2026-09-03: the
        /// control plane never once reported `stopped` across 30 probes over
        /// 152s, and the VM came back reporting `up 1 min`). The boot branch is
        /// covered by the pure test above, against the payload this VM really
        /// produced while it was stopped.
        #[test]
        #[ignore]
        fn live_control_plane_still_answers_the_shape_we_parse() {
            let out = ssh(CONTROL, &format!("ls --json {VM}")).expect("ssh ran");
            assert!(out.status.success(), "ls failed: {}", text(&out).trim());
            let body = String::from_utf8_lossy(&out.stdout);
            let action = warm_action(&body).expect("payload parses");
            assert_ne!(
                action,
                Warm::Create,
                "{VM} exists, so the parse must find it: {body}"
            );
        }

        /// THE orphan test, against the real VM.
        ///
        /// Proves the one claim this slice rests on: the run belongs to the VM,
        /// not to the ssh watching it, and after everything local is gone a
        /// process holding ONLY the handle finds it again. The viewport is
        /// killed on purpose, standing in for the app quitting.
        ///
        /// Ignored because it needs the owner's registered exe.dev key and a
        /// network. Run with `-- --ignored` when touching this module.
        #[test]
        #[ignore]
        fn live_a_run_outlives_its_viewport_and_is_found_by_handle_alone() {
            const HANDLE: &str = "livetest";
            ensure().expect("VM is up");
            let run_id = uuid::Uuid::new_v4().simple().to_string();
            let session = session_name(HANDLE, &run_id);
            let workdir = workdir("XNAUT").expect("a workdir");
            // `sleep` stands in for the agent. What is under test is the
            // session, not which binary is in it; the agent CLIs are on the VM
            // (`/usr/local/bin/claude`, verified 2026-09-03) and `run_script`
            // is the same either way.
            let staged = stage_script(
                "XNAUT",
                &session,
                &run_script(&workdir, "sleep 300", &session, ""),
            )
            .expect("script staged");

            // Start it exactly the way the app does: the argv a local PTY hosts.
            let argv = launch_argv(&session, &workdir, &staged);
            let mut viewport = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                // Match the native terminal contract when run over headless SSH.
                // TERM=dumb makes tmux exit before creating the session.
                .env("TERM", "xterm-256color")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("the viewport ssh started");

            // The session AND the script it staged: the VM is shared and paid
            // for, so a test leaves nothing of its own behind.
            let cleanup = || {
                let _ = ssh(
                    &vm_host(),
                    &format!(
                        "tmux kill-session -t {session} 2>/dev/null; rm -f {}",
                        remote_path(&staged)
                    ),
                );
            };
            let check = || -> Result<Vec<String>, String> { live_sessions_for(HANDLE) };

            // Give tmux a moment to register the session. The last ERROR is
            // kept, not discarded: swallowing it here is what turned "tmux
            // refused a relative path" into a bare "never appeared" and cost
            // an hour of guessing.
            let mut last = Ok(Vec::new());
            let found = (0..20).any(|_| {
                std::thread::sleep(std::time::Duration::from_millis(500));
                last = check();
                matches!(&last, Ok(live) if live.contains(&session))
            });
            if !found {
                let _ = viewport.kill();
                cleanup();
                panic!("{session} never appeared in tmux on {VM}; last answer: {last:?}");
            }

            // The app dies. This is the moment a naive design orphans the VM.
            viewport.kill().expect("viewport killed");
            let _ = viewport.wait();
            std::thread::sleep(std::time::Duration::from_secs(2));

            // A process that knows only the handle still finds the run.
            let after = check().expect("the VM still answers");
            let survived = after.contains(&session);

            // And re-attaching does not create anything new.
            let adopt = attach_argv(&session);
            let attached = std::process::Command::new(&adopt[0])
                .args(&adopt[1..])
                .env("TERM", "xterm-256color")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .and_then(|mut child| {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let alive = child.try_wait().map(|done| done.is_none());
                    let _ = child.kill();
                    let _ = child.wait();
                    alive
                })
                .unwrap_or(false);

            let count = check().map(|live| live.len()).unwrap_or_default();
            cleanup();

            assert!(survived, "the run died with its viewport: {after:?}");
            assert!(attached, "attach-session did not hold onto {session}");
            assert_eq!(count, 1, "attaching created a second session");

            // Cleaned up: the VM is shared and paid for.
            let left = live_sessions_for(HANDLE).expect("the VM answers");
            assert!(left.is_empty(), "left {left:?} running on {VM}");
            let litter = ssh(&vm_host(), &format!("ls {}", remote_path(&staged)))
                .expect("ssh ran");
            assert!(
                !litter.status.success(),
                "left {staged} behind on {VM}"
            );
        }

        /// The real thing, end to end: VM, rsync, a command in the workdir.
        /// Ignored because it needs the owner's registered exe.dev ssh key and
        /// a network; run explicitly with `-- --ignored` when touching this
        /// module.
        #[test]
        #[ignore]
        fn live_roundtrip() {
            let url = ensure().expect("VM exists or was created");
            assert!(url.starts_with("https://"), "got {url}");
            let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("repo root above src-tauri");
            push(repo, "XNAUT").expect("rsync up");
            let out = run("XNAUT", "ls src-tauri/Cargo.toml && echo live-ok").expect("ssh ran");
            let body = text(&out);
            assert!(body.contains("live-ok"), "step did not run: {body}");
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn gitvm() -> SandboxProviderSettings {
        SandboxProviderSettings {
            kind: "gitvm".into(),
            base_url: "https://gv.example/".into(),
            api_key: Some("k".into()),
        }
    }

    #[test]
    fn for_settings_selects_gitvm_and_rejects_others() {
        assert!(matches!(
            SandboxDriver::for_settings(&gitvm()),
            Ok(SandboxDriver::GitVm(_))
        ));
        let e2b = SandboxProviderSettings {
            kind: "e2b".into(),
            base_url: "x".into(),
            api_key: Some("k".into()),
        };
        let err = match SandboxDriver::for_settings(&e2b) {
            Err(error) => error,
            Ok(_) => panic!("e2b should be unsupported"),
        };
        assert!(
            err.contains("e2b") && err.contains("gitvm"),
            "error should name the bad kind and the supported one: {err}"
        );
    }

    #[test]
    fn gitvm_new_trims_base_url() {
        let driver = GitVmDriver::new(&gitvm()).unwrap();
        assert_eq!(
            driver.base_url, "https://gv.example",
            "trailing slash trimmed"
        );
    }

    /// `gitvm status` does not contain publicUrl, so the URL must come from the
    /// saved create response — reading it from status output was the bug that
    /// stored `…/vnc.html?autoconnect=1` as a design's canvas URL.
    #[test]
    fn public_url_comes_from_saved_state_not_status() {
        let dir = std::env::temp_dir().join(format!("xnaut-sbtest-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".gitvm")).unwrap();
        std::fs::write(
            dir.join(".gitvm/state.json"),
            r#"{"sandboxId":"sb-1","slug":"crisp-cricket-5743","publicUrl":"https://crisp-cricket-5743.nautbox.dev/"}"#,
        )
        .unwrap();
        assert_eq!(
            cli::public_url(&dir).unwrap(),
            "https://crisp-cricket-5743.nautbox.dev",
            "trailing slash trimmed, taken from state.json"
        );
        // A state without publicUrl still resolves via the slug.
        std::fs::write(
            dir.join(".gitvm/state.json"),
            r#"{"sandboxId":"sb-1","slug":"daring-bear-4767"}"#,
        )
        .unwrap();
        assert_eq!(
            cli::public_url(&dir).unwrap(),
            "https://daring-bear-4767.nautbox.dev"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// warm-up advertises the desktop viewer; only the origin is usable.
    #[test]
    fn extract_url_keeps_only_the_origin() {
        assert_eq!(
            cli::extract_url("desktop: https://crisp-cricket-5743.nautbox.dev/vnc.html?autoconnect=1&resize=scale")
                .unwrap(),
            "https://crisp-cricket-5743.nautbox.dev"
        );
    }

    #[test]
    fn gitvm_new_requires_a_key() {
        let nokey = SandboxProviderSettings {
            kind: "gitvm".into(),
            base_url: "https://gv.example".into(),
            api_key: None,
        };
        // Only asserts the error path when the env fallback is also absent.
        if std::env::var("GITVM_API_KEY").is_err() {
            assert!(GitVmDriver::new(&nokey).is_err());
        }
    }
}

#[cfg(test)]
mod expiry_tests {
    use crate::sandbox::cli::parse_expires_ms;

    #[test]
    fn expires_at_is_read_from_the_server_block_not_invented() {
        // Real `gitvm status` output: the local state first, then the server's
        // view, and only the server carries expiresAt. Inventing it from the
        // template default would overstate the lease by however long the box has
        // already been alive, and the UI would report a live sandbox after it
        // had been reaped.
        let body = r#"local state:
{
  "sandboxId": "sb-d3e5f5a8",
  "status": "running",
  "publicUrl": "https://royal-badger-0059.nautbox.dev"
}

server state:
{
  "status": "running",
  "slug": "royal-badger-0059",
  "exposedPort": 3000,
  "expiresAt": "2026-08-09T18:03:40Z"
}"#;
        assert_eq!(parse_expires_ms(body), Some(1_786_298_620_000)); // 2026-08-09T18:03:40Z
    }

    #[test]
    fn missing_or_malformed_expiry_is_none_rather_than_a_guess() {
        assert!(parse_expires_ms("local state:\n{}").is_none());
        assert!(parse_expires_ms(r#"{"expiresAt": "not a date"}"#).is_none());
        assert!(parse_expires_ms("").is_none());
    }
}
