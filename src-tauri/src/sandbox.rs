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
    }

    impl LaunchEnv {
        /// The one question the launcher asks. A refusal names every
        /// environment and its state, so it is actionable instead of flat.
        pub fn route(self, sandboxes: &[SandboxProviderSettings]) -> Result<LaunchRoute, String> {
            match self {
                Self::Local => Ok(LaunchRoute::Local),
                // Configured is the whole test, and it is the owner's
                // acceptance criterion read literally: if he pays for exe.dev
                // and says so in settings, that is where the run goes.
                Self::ExeDev if status_of(Self::ExeDev, sandboxes).ready => Ok(LaunchRoute::ExeDev),
                // ponytail: gitvm has a working driver for VERIFY (`cli::run`)
                // but hands back no interactive PTY, which is what a fleet
                // launch is. Refusing loudly beats quietly running the agent
                // somewhere the owner did not choose.
                remote => Err(no_route_yet(remote, sandboxes)),
            }
        }
    }

    fn no_route_yet(env: LaunchEnv, sandboxes: &[SandboxProviderSettings]) -> String {
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
            "resolves to the `{}` environment, but no launch driver routes an interactive agent \
PTY there yet, so this launch is refused rather than run somewhere you did not choose.\n{}\n\
Set this profile's execution to local to run it here now.",
            env.key(),
            lines.join("\n")
        )
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
        /// Aimed at gitvm since slice 2, because exe-dev now routes. The
        /// sandbox list deliberately carries no gitvm entry, so the refusal
        /// reads the "not configured" branch rather than the "no api key" one,
        /// which an ambient GITVM_API_KEY could otherwise flip.
        #[test]
        fn a_refusal_names_every_environment_and_its_state() {
            let err = match LaunchEnv::GitVm.route(&[provider("exe-dev", None)]) {
                Err(message) => message,
                Ok(_) => panic!("gitvm has no launch driver yet"),
            };
            assert!(err.contains("exe-dev: ready"), "{err}");
            assert!(err.contains("local: ready"), "{err}");
            assert!(err.contains("gitvm: not ready"), "{err}");
            assert!(err.contains("settings.sandboxes"), "{err}");
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
        let body = std::fs::read_to_string(dir.join(".gitvm/state.json"))
            .map_err(|_| "no sandbox state — is it warm?".to_string())?;
        let state: Value =
            serde_json::from_str(&body).map_err(|e| format!("bad sandbox state: {e}"))?;
        let ip = state["guestIp"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("sandbox state has no guest ip")?;
        let jump = state["jump"]
            .as_str()
            .unwrap_or("root@gitvmd-control-01.tail138398.ts.net");
        let out = std::process::Command::new("ssh")
            .args([
                "-f", // background once the forward is established
                "-N", // no remote command, just the tunnel
                "-J",
                jump,
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "LogLevel=ERROR",
                "-o",
                "ExitOnForwardFailure=yes",
                "-o",
                "ServerAliveInterval=30",
                "-R",
                &format!("{port}:localhost:{port}"),
                &format!("root@{ip}"),
            ])
            .output()
            .map_err(|e| format!("ssh: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&out.stderr);
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

    fn ssh(dest: &str, command: &str) -> Result<std::process::Output, String> {
        std::process::Command::new("ssh")
            .args(SSH_OPTS)
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
        let workdir = workdir(project)?;
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
                &format!("ssh {}", SSH_OPTS.join(" ")),
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

    /// The tmux session one remote run lives in.
    ///
    /// DERIVED from the handle and the run id, never remembered. This is the
    /// whole adoption story. A local run is found again after a restart by
    /// rebuilding `xnaut-<handle>-` from the profile handle and prefix-matching
    /// zellij's session list (`agents.rs::live_sessions_for`); nothing about
    /// that lookup needs memory, because the handle is on disk in the profile
    /// store. The remote name is built by the SAME sanitiser so the two are one
    /// vocabulary rather than two, and `live_sessions_for` below asks tmux on
    /// the VM the identical question. An app restart therefore costs the
    /// viewport, never the run.
    pub fn session_name(handle: &str, run_id: &str) -> String {
        let run = &run_id[..run_id.len().min(8)];
        crate::zellij::session_name(&format!("xnaut-{}-{run}", handle.trim()))
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

    /// Filter a `tmux list-sessions` listing down to one agent's runs.
    pub(crate) fn sessions_for_handle(listing: &str, handle: &str) -> Vec<String> {
        let prefix = session_prefix(handle);
        listing
            .lines()
            .map(str::trim)
            .filter(|name| name.starts_with(&prefix))
            .map(str::to_string)
            .collect()
    }

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
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let workdir = workdir(project)?;
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
    pub fn run_script(workdir: &str, command: &str, session: &str) -> String {
        format!(
            "#!/bin/bash -l\n\
             cd {} || {{ echo \"xNAUT: {} is not on {VM}\"; exec bash -l; }}\n\
             printf '\\033[36mxNAUT: running on {VM}.exe.xyz in tmux session %s\\033[0m\\n' {}\n\
             printf '\\033[36mxNAUT: this run survives the app; reattach finds it by name\\033[0m\\n'\n\
             exec {}\n",
            remote_path(workdir),
            workdir,
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
        argv.extend(SSH_OPTS.iter().map(|opt| opt.to_string()));
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
                run_script("verify/x", "claude", "s").contains(r#"cd "$HOME"/'verify/x'"#),
                "the script must cd absolutely as well"
            );
        }

        /// The script has to survive a prompt that is hostile to shells, and
        /// it has to `exec` so the agent IS the session: a wrapper left alive
        /// after the agent exits would keep the session listed, and every
        /// liveness answer downstream would then be a lie.
        #[test]
        fn the_run_script_execs_the_agent_and_survives_a_quoted_workdir() {
            let script = run_script("verify/it's", "claude --model x", "xnaut-a-1");
            assert!(script.starts_with("#!/bin/bash -l"), "{script}");
            assert!(script.contains(r"'verify/it'\''s'"), "{script}");
            assert!(script.contains("exec claude --model x"), "{script}");
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
                &run_script(&workdir, "sleep 300", &session),
            )
            .expect("script staged");

            // Start it exactly the way the app does: the argv a local PTY hosts.
            let argv = launch_argv(&session, &workdir, &staged);
            let mut viewport = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .stdin(std::process::Stdio::null())
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
                .stdin(std::process::Stdio::null())
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
