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
    use std::path::Path;

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
