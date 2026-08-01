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
        Err(format!("could not expose :{port} to the sandbox: {}", err.trim()))
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

    /// Runs a shell command inside the sandbox (rsyncs local changes in first).
    pub fn run(dir: &Path, script: &str) -> Result<std::process::Output, String> {
        exec(dir, &["run", script])
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
