//! Repository-backed exe.dev runs. Receipts are local; results are ordinary
//! branches in the repository explicitly selected in project setup.
use crate::sandbox::exe::shell_single_quote as quote;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

const PUBLISHER: &str = include_str!("repository_publish.py");
const MEDIA: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "avif", "heic", "bmp", "tif", "tiff", "raw", "dng", "mp4",
    "mov", "webm", "mkv", "avi", "m4v", "mp3", "wav",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transfer {
    pub run_id: String,
    pub project: String,
    pub ticket: Option<String>,
    pub handle: String,
    pub local_path: String,
    pub remote: String,
    /// Forge-provided SSH endpoint for the same repository. Legacy receipts
    /// keep using `remote`; desktop reconciliation always uses project setup.
    #[serde(default)]
    pub worker_remote: Option<String>,
    #[serde(default)]
    pub worker: crate::worker_bootstrap::Target,
    pub source_sha: String,
    pub base: String,
    pub branch: String,
    pub workdir: String,
    pub artifacts: String,
    pub state: String,
    pub pr_url: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub filed: bool,
}

/// No tokens in URLs, local paths, shell commands, or implicit origin fallback.
pub fn validate_remote(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Add a repository URL in Project settings (Forgejo or GitHub).".into());
    }
    if value.starts_with("https://") || value.starts_with("http://") || value.starts_with("ssh://")
    {
        let url = url::Url::parse(value).map_err(|_| "Invalid repository URL")?;
        if url.host_str().is_none()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || (matches!(url.scheme(), "http" | "https") && !url.username().is_empty())
            || url.path().trim_matches('/').split('/').count() != 2
        {
            return Err("Use a repository clone URL without embedded credentials.".into());
        }
    } else {
        let (host, path) = value
            .split_once(':')
            .ok_or("Use an HTTP(S) or SSH repository clone URL.")?;
        if host.starts_with('-')
            || host.contains('/')
            || host.is_empty()
            || path.starts_with('/')
            || path.split('/').count() != 2
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "@._-".contains(c))
        {
            return Err("Use an HTTP(S) or SSH repository clone URL.".into());
        }
    }
    let repo_path = if value.contains("://") {
        url::Url::parse(value)
            .map_err(|_| "Invalid repository URL")?
            .path()
            .trim_matches('/')
            .to_string()
    } else {
        value.split_once(':').unwrap().1.to_string()
    };
    if repo_path.split('/').any(|part| {
        part.is_empty()
            || part == ".git"
            || !part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
    }) {
        return Err("Use an owner/repository clone URL.".into());
    }
    let path = value.rsplit_once(':').map(|(_, p)| p).unwrap_or(value);
    if path.split('/').any(|s| s == ".." || s == ".") {
        return Err("Invalid repository path".into());
    }
    Ok(value.into())
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("Git output unavailable")?;
    let reader = std::thread::spawn(move || {
        let mut data = Vec::new();
        let _ = stdout.take(4 * 1024 * 1024).read_to_end(&mut data);
        data
    });
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() > std::time::Duration::from_secs(180) {
            // Own process group: this never signals another user's Git command.
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(format!(
                "Git {} timed out; data is retained for retry.",
                args[0]
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    };
    let stdout = reader.join().map_err(|_| "Git output reader failed")?;
    if !status.success() {
        return Err(format!(
            "Git {} failed; check the configured repository and credentials.",
            args[0]
        ));
    }
    Ok(String::from_utf8_lossy(&stdout).trim().into())
}

/// Resolve a machine-local SSH alias before shipping it to another machine.
/// The key itself stays local; the VM must have its own access configured.
pub(crate) fn portable_remote(remote: &str) -> Result<String, String> {
    if remote.contains("://") {
        return Ok(remote.into());
    }
    let (host, path) = remote.split_once(':').ok_or("Invalid SSH repository")?;
    let out = Command::new("ssh")
        .args(["-G", host])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("Cannot resolve the repository's SSH host.".into());
    }
    let config = String::from_utf8_lossy(&out.stdout);
    let get = |key: &str| {
        config
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key} ")))
            .unwrap_or("")
    };
    let hostname = get("hostname");
    let user = get("user");
    let port = get("port");
    if hostname.is_empty() || user.is_empty() || port.is_empty() {
        return Err("Incomplete SSH host configuration".into());
    }
    validate_remote(&format!("ssh://{user}@{hostname}:{port}/{path}"))
}

pub(crate) fn store_dir() -> Result<PathBuf, String> {
    Ok(crate::agents::registry_dir()?.join("repository-transfers"))
}
pub(crate) fn save_at(dir: &Path, transfer: &Transfer) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", transfer.run_id));
    let tmp = path.with_extension("tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(transfer).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}
pub fn save(transfer: &Transfer) -> Result<(), String> {
    save_at(&store_dir()?, transfer)
}
fn list() -> Result<Vec<Transfer>, String> {
    let dir = store_dir()?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut rows = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(row) = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|v| serde_json::from_slice(&v).map_err(|e| e.to_string()))
            {
                rows.push(row);
            }
        }
    }
    Ok(rows)
}

/// A copied workspace alone does not preserve the worker's ability to retry
/// an upload: its deploy key lives outside the workspace. Keep the sandbox
/// until repository reconciliation has confirmed delivery.
pub fn guard_worker_teardown(local_path: &Path) -> Result<(), String> {
    for transfer in list()? {
        if let crate::worker_bootstrap::Target::GitVm { local_path: worker_path } = &transfer.worker {
            if worker_path == local_path && transfer.state == "running" {
                return Err(format!("Task {} still has unconfirmed repository delivery; retain its worker for upload retry.", transfer.run_id));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn repository_transfer_list(project: String) -> Result<Vec<Transfer>, String> {
    Ok(list()?
        .into_iter()
        .filter(|r| r.project == project)
        .collect())
}

pub fn prepare(
    path: &Path,
    project_root: &Path,
    ticket: Option<String>,
    handle: &str,
    run_id: &str,
) -> Result<Transfer, String> {
    let projects =
        crate::project_management::list_projects(&crate::project_management::repo_now()?)?;
    let project = projects.iter().find(|p| {
        ticket.as_deref().is_some_and(|t| t.rsplit_once('-').is_some_and(|(key, _)| key == p.key))
    }).or_else(|| projects.iter().find(|p| {
        let local = crate::project_management::local_source_path(p);
        !local.is_empty() && Path::new(&local) == project_root
    })).ok_or("Link this checkout to a project and add its repository in Project settings before dispatching.")?;
    let registered_path = crate::project_management::local_source_path(project);
    if registered_path.is_empty() || crate::sandbox::launch_env::project_root(Path::new(&registered_path)) != project_root {
        return Err("The ticket's project does not match this checkout. Link the correct local folder in Project settings before uploading source.".into());
    }
    let configured = validate_remote(&project.forge_remote)?;
    if !git(path, &["status", "--porcelain"])?.is_empty() {
        return Err("Commit or stash this checkout's changes before dispatching. Repository runs use an exact committed revision.".into());
    }
    let source_sha = git(path, &["rev-parse", "HEAD"])?;
    let remote = portable_remote(&configured)?;
    let refs = git(path, &["ls-remote", "--symref", &remote, "HEAD"])?;
    let base = refs
        .lines()
        .find_map(|l| {
            l.strip_prefix("ref: refs/heads/")
                .and_then(|s| s.split_once('\t'))
                .map(|(b, _)| b.to_string())
        })
        .ok_or("The configured repository needs a default branch with an initial commit.")?;
    git(path, &["check-ref-format", &format!("refs/heads/{base}")])?;
    Ok(Transfer {
        run_id: run_id.into(),
        project: project.key.clone(),
        ticket,
        handle: handle.trim_start_matches('@').to_ascii_lowercase(),
        local_path: path.to_string_lossy().into(),
        remote,
        worker_remote: None,
        worker: Default::default(),
        source_sha,
        base,
        branch: format!("xnaut/runs/{run_id}"),
        workdir: format!("agents/runs/{run_id}"),
        artifacts: format!(".xnaut/runs/{run_id}"),
        state: "prepared".into(),
        pr_url: None,
        error: None,
        filed: false,
    })
}

pub fn stage(transfer: &Transfer, access: &crate::worker_bootstrap::Access) -> Result<(), String> {
    // The common task bootstrap has already provisioned and checked Git/LFS
    // write access. Keep a separate SSH identity in THIS checkout's config.
    let input = format!("refs/heads/xnaut/inputs/{}", transfer.run_id);
    git(
        Path::new(&transfer.local_path),
        &["lfs", "push", &transfer.remote, &transfer.source_sha],
    )?;
    git(
        Path::new(&transfer.local_path),
        &[
            "push",
            &transfer.remote,
            &format!("{}:{input}", transfer.source_sha),
        ],
    )?;
    // A fresh directory per run, with real history. Never delete an earlier run.
    let command = format!("set -e; export GIT_TERMINAL_PROMPT=0; test ! -e {dir}; mkdir -p {parent}; git -c core.sshCommand={ssh} clone --no-checkout -- {remote} {dir}; cd {dir}; git config --local core.sshCommand {ssh}; git config --local user.name xNAUT; git config --local user.email xnaut@localhost; git lfs install --local; git fetch origin {input}; test \"$(git rev-parse FETCH_HEAD)\" = {sha}; git checkout -b {branch} {sha}; git push --dry-run origin HEAD:refs/heads/{branch}; test ! -L .xnaut; test ! -L .xnaut/runs; mkdir -p {artifacts}",
        dir=quote(&transfer.workdir), parent=quote(Path::new(&transfer.workdir).parent().ok_or("Invalid worker directory")?.to_str().ok_or("Invalid worker directory")?), remote=quote(&access.remote), ssh=quote(&access.ssh_command), input=quote(&input), sha=quote(&transfer.source_sha), branch=quote(&transfer.branch), artifacts=quote(&transfer.artifacts));
    transfer.worker.command(&command)?;
    let metadata = serde_json::to_string_pretty(transfer).map_err(|e| e.to_string())?;
    transfer.worker.file(&transfer.workdir, ".git/xnaut-transfer.json", &metadata)?;
    transfer.worker.file(&transfer.workdir, ".git/xnaut-publish.py", PUBLISHER)?;
    let attrs = MEDIA
        .iter()
        .flat_map(|ext| [ext.to_string(), ext.to_uppercase()])
        .map(|ext| format!("*.{ext} filter=lfs diff=lfs merge=lfs -text\n"))
        .collect::<String>();
    transfer.worker.file(
        &transfer.workdir,
        &format!("{}/.gitattributes", transfer.artifacts),
        &attrs,
    )?;
    transfer.worker.file(
        &transfer.workdir,
        &format!("{}/run.json", transfer.artifacts),
        &metadata,
    )?;
    Ok(())
}

pub fn instructions(t: &Transfer) -> String {
    format!("\n\nRepository delivery for this run (overrides local callback instructions):\n\
        Work only on branch {branch}. Commit authorized source changes there. Store ALL task reports, notes, screenshots, pictures, and videos under {artifacts}/; media there is tracked using Git LFS. Never publish credentials or caches.\n\
        Write {artifacts}/handback.json with summary, files_changed (paths), commits (SHAs), how_verified (commands and results), not_finished (say 'nothing' only when true), confidence ('low', 'medium', or 'high').\n\
        When finished run: python3 .git/xnaut-publish.py --finish 0\n\
        This durably pushes the task branch to the project's configured repository. If it fails, report upload pending; files stay on this worker. Do not claim delivery until it succeeds. Do not call desktop localhost handback/Mesh endpoints. xNAUT will import the handback and open the PR on reconnect. Do not merge or push the default branch.\n",
        branch=t.branch, artifacts=t.artifacts)
}

pub fn run_script(t: &Transfer, command: &str, seed: &str) -> String {
    let directory = if Path::new(&t.workdir).is_absolute() { quote(&t.workdir) } else { format!("\"$HOME\"/{}", quote(&t.workdir)) };
    format!("#!/bin/bash -l\ncd {} || exit 1\n{}\nprintf '%s\\n' 'xNAUT: repository-backed run; results are retained until uploaded.'\necho $$ > .git/xnaut-supervisor.pid\nprintf running > .git/xnaut-phase\nenv {}\ncode=$?\nprintf uploading > .git/xnaut-phase\npython3 .git/xnaut-publish.py --finish \"$code\" --retry\nexit \"$code\"\n", directory, seed, command)
}

#[derive(Debug, Deserialize)]
struct ResultRecord {
    run_id: String,
    source_sha: String,
    exit_code: i32,
    uncommitted_source: bool,
}

fn fetch_result(
    t: &Transfer,
) -> Result<Option<(ResultRecord, Option<crate::handback::Handback>)>, String> {
    fetch_result_in(&store_dir()?, t)
}
fn fetch_result_in(
    store: &Path,
    t: &Transfer,
) -> Result<Option<(ResultRecord, Option<crate::handback::Handback>)>, String> {
    let dir = store.join(format!("{}.git", t.run_id));
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        git(&dir, &["init", "--bare"])?;
    }
    let reference = format!("refs/heads/{}", t.branch);
    if git(&dir, &["ls-remote", &t.remote, &reference])?.is_empty() {
        return Ok(None);
    }
    git(&dir, &["fetch", "--no-tags", &t.remote, &reference])?;
    git(
        &dir,
        &["merge-base", "--is-ancestor", &t.source_sha, "FETCH_HEAD"],
    )?;
    let result_path = format!("FETCH_HEAD:{}/result.json", t.artifacts);
    // A metadata branch without a final result is still running.
    if git(&dir, &["cat-file", "-e", &result_path]).is_err() {
        return Ok(None);
    }
    let read = |path: &str| -> Result<String, String> {
        let size: usize = git(&dir, &["cat-file", "-s", path])?
            .parse()
            .map_err(|_| "Invalid result size")?;
        if size > 256 * 1024 {
            return Err("Run result exceeds 256 KiB".into());
        }
        git(&dir, &["show", path])
    };
    let result: ResultRecord = serde_json::from_str(&read(&result_path)?)
        .map_err(|e| format!("Invalid run result: {e}"))?;
    if result.run_id != t.run_id || result.source_sha != t.source_sha {
        return Err("Run result does not match its launch receipt".into());
    }
    let handback_path = format!("FETCH_HEAD:{}/handback.json", t.artifacts);
    let handback = if git(&dir, &["cat-file", "-e", &handback_path]).is_ok() {
        Some(
            serde_json::from_str(&read(&handback_path)?)
                .map_err(|e| format!("Invalid handback: {e}"))?,
        )
    } else {
        None
    };
    Ok(Some((result, handback)))
}

async fn reconcile(t: &mut Transfer, hosts: &[crate::settings::ForgeHost]) -> Result<(), String> {
    if !t.workdir.is_empty() && t.state == "running" {
        let snapshot = t.clone();
        // Liveness evidence comes from the worker, never from opening a viewport.
        if let Ok(Ok(proof)) = tokio::task::spawn_blocking(move || {
            snapshot.worker.probe(&snapshot.workdir)
        })
        .await
        {
            if let Ok(registry) = crate::agents::registry_dir() {
                let pong = crate::run_control::Pong {
                    run_id: t.run_id.clone(),
                    agent_pid: proof["agent_pid"].as_u64().map(|p| p as u32),
                    head: proof["head"].as_str().unwrap_or_default().into(),
                    ..Default::default()
                };
                let _ =
                    crate::run_control::beacon_in(&registry, &pong, crate::run_control::now_ms());
                if pong.agent_pid.is_some() {
                    let _ = crate::run_control::update_in(&registry, &t.run_id, |run| {
                        if run.state == crate::run_control::RunState::Starting {
                            run.state = crate::run_control::RunState::Running;
                        }
                    });
                }
            }
        }
    }
    let snapshot = t.clone();
    let Some((result, handback)) = tokio::task::spawn_blocking(move || fetch_result(&snapshot))
        .await
        .map_err(|e| e.to_string())??
    else {
        return Ok(());
    };
    t.state = "pushed".into();
    let mut pr_error = None;
    if t.pr_url.is_none() {
        let opened: Result<String, String> = async {
        let (host, parsed) = crate::forges::host_for_remote(hosts, &t.remote).ok_or("Results pushed. Configure this repository's forge connection in Settings to open its PR.")?;
        let mut host = host.clone();
        host.owner = parsed.owner;
        let note_snapshot = t.handle == "owner" && t.workdir.is_empty();
        let title = if note_snapshot { format!("{}: project notebook", t.project) } else { format!("{}: agent results", t.ticket.as_deref().unwrap_or(&t.run_id)) };
        let body = if note_snapshot {
            format!("Project notebook snapshot for {}. Notes are saved as Markdown and JSON under `.xnaut/notes/`.\n\nSource: `{}`. Review before merging.", t.project, t.source_sha)
        } else { format!("Results for xNAUT run `{}`.\n\nSource: `{}`\n\nReports, notes and media: `{}/`\n\nAgent exit code: {}. Uncommitted source: {}.\n\n{}\n\nReview the evidence before merging.", t.run_id, t.source_sha, t.artifacts, result.exit_code, result.uncommitted_source,
            handback.as_ref().map(|h| h.summary.as_str()).unwrap_or("No structured handback was supplied.")) };
        crate::forges::ensure_pr(
                &host,
                &parsed.repo,
                &t.branch,
                &t.base,
                &title,
                &body,
            )
            .await
        }.await;
        match opened {
            Ok(url) => {
                t.pr_url = Some(url);
                save(t)?;
            }
            Err(error) => pr_error = Some(error),
        }
    }
    if result.exit_code != 0 || result.uncommitted_source {
        return Err("Results and PR preserved; the agent exited unsuccessfully or left source changes uncommitted. Review required.".into());
    }
    if t.ticket.is_some() && !t.filed {
        let mut h = handback
            .ok_or("Results pushed; structured handback is missing. Ticket remains unchanged.")?;
        h.run_id = Some(t.run_id.clone());
        h.ticket = t.ticket.clone().unwrap();
        h.from = t.handle.clone();
        h.submitted_at = chrono::Utc::now().to_rfc3339();
        let h_copy = h.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let repo = crate::project_management::repo_now()?;
            let existing = crate::project_management::ticket_list_in(&repo, None)?.into_iter().find(|r| r.id == h_copy.ticket).ok_or("Run ticket no longer exists")?;
            if existing.owner.as_deref().map(|s| s.trim_start_matches('@')) != Some(h_copy.from.as_str())
                && !existing.handback.as_ref().is_some_and(|h| h.run_id == h_copy.run_id)
            { return Err("Ticket ownership changed; repository results preserved without overwriting its handback.".into()); }
            // A crash after filing must not append the same event twice.
            if !existing.handback.as_ref().is_some_and(|h| h.run_id == h_copy.run_id) {
                match crate::project_management::file_handback_in(&repo, &h_copy)? {
                    crate::project_management::Filing::Refused(_) => return Err("Handback is incomplete; evidence is saved in the PR, ticket remains unchanged.".into()),
                    crate::project_management::Filing::Filed { .. } => (),
                }
            }
            let current = crate::project_management::ticket_list_in(&repo, None)?.into_iter().find(|r| r.id == h_copy.ticket).ok_or("Run ticket no longer exists")?;
            if current.status != "review" && current.status != "complete" {
                if current.owner.as_deref().map(|s| s.trim_start_matches('@')) != Some(h_copy.from.as_str()) {
                    return Err("Results preserved; ticket ownership changed, so it was not reassigned.".into());
                }
                let request = serde_json::from_value(serde_json::json!({
                    "id": current.id, "expected_revision": current.revision,
                    "caller": h_copy.from, "status": "review"
                })).map_err(|e| e.to_string())?;
                crate::project_management::ticket_update_in(&repo, request)?;
            }
            Ok(())
        }).await.map_err(|e| e.to_string())??;
        t.filed = true;
    }
    if let Some(error) = pr_error {
        return Err(error);
    }
    t.state = "review".into();
    Ok(())
}

/// Only receipts minted by this installation are fetched. Repository content
/// is parsed as bounded JSON, never executed, and cannot select a ticket/actor.
pub fn spawn_reconciler(app: tauri::AppHandle) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        loop {
            crate::repository_notes::drain().await;
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
    tauri::async_runtime::spawn(async move {
        loop {
            let hosts = app
                .state::<crate::state::AppState>()
                .settings
                .lock()
                .await
                .forges
                .clone();
            if let Ok(rows) = list() {
                for mut t in rows
                    .into_iter()
                    .filter(|r| r.state == "running" || r.state == "pushed")
                {
                    t.error = reconcile(&mut t, &hosts).await.err();
                    let _ = save(&t);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_must_be_explicit_and_credential_free() {
        for good in [
            "https://github.com/org/repo.git",
            "http://forge.local:3000/org/repo.git",
            "ssh://git@forge.example:2222/org/repo.git",
            "forgejo:org/repo.git",
            "git@github.com:org/repo.git",
        ] {
            assert!(validate_remote(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "/tmp/repo",
            "origin",
            "https://token@github.com/org/repo",
            "http://token@forge.local/org/repo",
            "https://user:secret@host/org/repo",
            "file:///tmp/repo",
            "-oProxyCommand=evil:org/repo",
            "git@host:../repo",
            "https://host/org/repo?token=secret",
        ] {
            assert!(validate_remote(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn result_fetch_keeps_history_checks_identity_and_bounds_untrusted_data() {
        let root = std::env::temp_dir().join(format!("xnaut-transfer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("remote.git");
        let worker = root.join("worker");
        let cache = root.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        git(&root, &["init", "--bare", remote.to_str().unwrap()]).unwrap();
        git(&root, &["init", "-b", "main", worker.to_str().unwrap()]).unwrap();
        git(&worker, &["config", "user.name", "test"]).unwrap();
        git(&worker, &["config", "user.email", "test@localhost"]).unwrap();
        std::fs::write(worker.join("source.txt"), "base").unwrap();
        git(&worker, &["add", "."]).unwrap();
        git(&worker, &["commit", "-m", "base"]).unwrap();
        let source = git(&worker, &["rev-parse", "HEAD"]).unwrap();
        let t = Transfer {
            run_id: "fixture".into(),
            project: "TEST".into(),
            ticket: Some("TEST-1".into()),
            handle: "builder".into(),
            local_path: worker.to_string_lossy().into(),
            remote: remote.to_string_lossy().into(),
            worker_remote: None,
            worker: Default::default(),
            source_sha: source.clone(),
            base: "main".into(),
            branch: "xnaut/runs/fixture".into(),
            workdir: "agents/runs/fixture".into(),
            artifacts: ".xnaut/runs/fixture".into(),
            state: "running".into(),
            pr_url: None,
            error: None,
            filed: false,
        };
        assert!(fetch_result_in(&cache, &t).unwrap().is_none());
        git(&worker, &["checkout", "-b", &t.branch]).unwrap();
        std::fs::create_dir_all(worker.join(&t.artifacts)).unwrap();
        let result = worker.join(&t.artifacts).join("result.json");
        let commit = || {
            git(&worker, &["add", "."]).unwrap();
            git(&worker, &["commit", "-m", "result"]).unwrap();
            git(
                &worker,
                &["push", &t.remote, &format!("HEAD:refs/heads/{}", t.branch)],
            )
            .unwrap();
        };
        std::fs::write(&result, serde_json::json!({"run_id":"fixture", "source_sha":source, "exit_code":0, "uncommitted_source":false}).to_string()).unwrap();
        commit();
        assert!(fetch_result_in(&cache, &t).unwrap().is_some());
        let mut wrong = t.clone();
        wrong.source_sha = "0000000000000000000000000000000000000000".into();
        assert!(fetch_result_in(&cache, &wrong).is_err());
        std::fs::write(&result, serde_json::json!({"run_id":"another-run", "source_sha":source, "exit_code":0, "uncommitted_source":false}).to_string()).unwrap();
        commit();
        assert!(fetch_result_in(&cache, &t)
            .unwrap_err()
            .contains("launch receipt"));
        std::fs::write(&result, "x".repeat(300_000)).unwrap();
        commit();
        assert!(fetch_result_in(&cache, &t).unwrap_err().contains("256 KiB"));
        save_at(&cache, &t).unwrap();
        let loaded: Transfer =
            serde_json::from_slice(&std::fs::read(cache.join("fixture.json")).unwrap()).unwrap();
        assert_eq!(loaded.remote, t.remote);
        std::fs::remove_dir_all(root).unwrap();
    }
}
