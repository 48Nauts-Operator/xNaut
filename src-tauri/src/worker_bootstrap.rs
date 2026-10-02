//! Repeatable setup at the task-launch boundary, not a manual repair of a VM.
//! Every task proves tool, network, Git write and LFS access before publication.
use crate::sandbox::exe;
use crate::settings::{ForgeHost, WorkerNetworkSettings};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

const TOOLS: &str = include_str!("worker_tools.sh");
const ACCESS: &str = include_str!("worker_access.py");

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Target {
    #[default]
    ExeDev,
    GitVm {
        local_path: PathBuf,
    },
}

impl Target {
    pub fn exchange(
        &self,
        command: &str,
        input: &[u8],
        seconds: u32,
    ) -> Result<std::process::Output, String> {
        match self {
            Self::ExeDev => exe::repository_exchange(command, input, seconds),
            Self::GitVm { local_path } => {
                crate::sandbox::cli::repository_exchange(local_path, command, input, seconds)
            }
        }
    }
    pub fn command(&self, command: &str) -> Result<(), String> {
        if self.exchange(command, &[], 600)?.status.success() {
            Ok(())
        } else {
            Err("Worker repository preparation failed. Existing runs were preserved; no agent was started.".into())
        }
    }
    pub fn file(&self, workdir: &str, relative: &str, body: &str) -> Result<(), String> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        self.command(&format!(
            "printf %s {} | base64 -d > {}",
            exe::shell_single_quote(&STANDARD.encode(body)),
            exe::shell_single_quote(&format!("{workdir}/{relative}"))
        ))
    }
    pub fn probe(&self, workdir: &str) -> Result<Value, String> {
        let out = self.exchange(&exe::repository_probe_command(workdir), &[], 30)?;
        if !out.status.success() {
            return Err("Could not read worker progress; existing data is retained.".into());
        }
        serde_json::from_slice(&out.stdout).map_err(|_| "Invalid worker progress".into())
    }
}

pub struct Access {
    pub remote: String,
    pub ssh_command: String,
    pub http_proxy: Option<String>,
}

fn failure(code: &str) -> String {
    let detail = match code {
        "network_setup_required" => "The repository is not reachable from workers. Configure fleet network access once in Settings → Worker access, or use a worker-reachable repository endpoint. New tasks will reuse that setup automatically.",
        "network_install_failed" => "The worker could not install the private-network client. Check its package/network policy in provider setup.",
        "network_tags_invalid" => "Worker network tags need the tag: prefix, for example tag:workers. Enter only tags authorized in your Tailscale policy; leave this field empty if the enrollment key already assigns its tags.",
        "network_tags_denied" => "Tailscale rejected the requested worker tags. Use tags authorized for this enrollment key in your tailnet policy, or leave Worker network tags empty if the key already assigns its tags.",
        "network_enrollment_failed" => "Automatic fleet network enrollment failed. Check the reusable enrollment credential in Settings → Worker access.",
        "network_device_approval_required" => "The worker network requires administrator device approval. Use a preauthorized fleet enrollment credential in Settings → Worker access.",
        "network_daemon_unavailable" => "The worker's private-network service could not start. Check the provider image's service permissions.",
        "network_route_unavailable" => "The worker is enrolled, but the repository is unreachable. Check the fleet's network rules and repository address.",
        "repository_read_denied" => "The worker's repository read check failed after provisioning its deploy key. Check the repository's SSH endpoint and access policy.",
        "repository_write_denied" => "The worker can read the repository but its task-branch write check failed. Check repository write permissions.",
        "lfs_authorization_failed" | "lfs_upload_unavailable" => "Git access is ready, but the worker's LFS upload check failed. Check the repository's LFS service and its network address.",
        "repository_key_failed" => "The worker could not prepare its repository identity. Check its home-directory permissions.",
        "XNAUT_BOOTSTRAP_INSTALL_PERMISSION" => "Automatic worker setup needs root or passwordless sudo to install missing tools. Configure this once in the provider image.",
        "XNAUT_BOOTSTRAP_UNSUPPORTED_OS" => "This worker image has no supported package manager. Provide Python 3, Git, Git LFS, OpenSSH, tmux and curl in the provider image.",
        "XNAUT_BOOTSTRAP_INSTALL_FAILED" | "XNAUT_BOOTSTRAP_TOOLS_MISSING" => "Automatic worker tool installation failed. Check the provider's package repositories and network policy.",
        _ => "Automatic worker setup could not be verified. Existing task data is retained; retry after checking the provider connection.",
    };
    format!("{detail} No agent was started.")
}

async fn exchange(target: Target, payload: Value) -> Result<Value, String> {
    let result = tokio::task::spawn_blocking(move || {
        let input = serde_json::to_vec(&payload).map_err(|_| failure("worker_bootstrap_failed"))?;
        target.exchange(
            &format!("python3 -c {}", exe::shell_single_quote(ACCESS)),
            &input,
            600,
        )
    })
    .await
    .map_err(|_| failure("worker_bootstrap_failed"))??;
    let value: Value =
        serde_json::from_slice(&result.stdout).map_err(|_| failure("worker_bootstrap_failed"))?;
    if result.status.success() && value["ok"] == true {
        Ok(value)
    } else {
        Err(failure(value["error"].as_str().unwrap_or_default()))
    }
}

pub async fn prepare(
    target: &Target,
    remote: &str,
    branch: &str,
    hosts: &[ForgeHost],
    network: &WorkerNetworkSettings,
) -> Result<Access, String> {
    let tools_target = target.clone();
    let tools = tokio::task::spawn_blocking(move || tools_target.exchange(TOOLS, &[], 600))
        .await
        .map_err(|_| failure("worker_bootstrap_failed"))??;
    if !tools.status.success()
        || !String::from_utf8_lossy(&tools.stdout)
            .lines()
            .any(|l| l == "XNAUT_BOOTSTRAP_TOOLS_READY")
    {
        let marker = String::from_utf8_lossy(&tools.stdout);
        return Err(failure(
            marker
                .lines()
                .find(|l| l.starts_with("XNAUT_BOOTSTRAP_"))
                .unwrap_or_default(),
        ));
    }
    let (host, parsed) = crate::forges::host_for_remote(hosts, remote)
        .ok_or("Connect this project's forge in Settings → Forges so every worker can receive repository access automatically. No agent was started.")?;
    let worker_remote = crate::forges::worker_clone_url(host, &parsed.owner, &parsed.repo).await?;
    let identity = exchange(target.clone(), json!({"operation": "prepare", "remote": worker_remote, "api_url": host.base_url, "network": network})).await?;
    let key = identity["public_key"]
        .as_str()
        .ok_or("Worker public key is missing")?;
    crate::forges::ensure_worker_key(host, &parsed.owner, &parsed.repo, key).await?;
    let verified = exchange(
        target.clone(),
        json!({"operation": "verify", "remote": worker_remote, "branch": branch}),
    )
    .await?;
    if verified["public_key"] != identity["public_key"] {
        return Err(
            "Worker identity changed during setup. No agent was started; retry setup.".into(),
        );
    }
    Ok(Access {
        remote: worker_remote,
        http_proxy: verified["http_proxy"].as_str().map(String::from),
        ssh_command: verified["ssh_command"]
            .as_str()
            .ok_or("Worker SSH setup is missing")?
            .into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failures_identify_the_failed_requirement_without_raw_output() {
        assert!(failure("network_setup_required").contains("once"));
        assert!(failure("repository_write_denied").contains("can read"));
        assert!(failure("lfs_upload_unavailable").contains("LFS"));
        assert!(!failure("some secret from stderr").contains("secret"));
    }
}
