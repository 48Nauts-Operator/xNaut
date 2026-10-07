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
    #[serde(default)]
    pub local_branch: String,
    pub remote: String,
    #[serde(default)]
    pub review_parent: Option<String>,
    /// Author successor of the original delivery. Reuses its PR/branch while
    /// preserving a new native run and immutable artifacts for every repair.
    #[serde(default)]
    pub repair_parent: Option<String>,
    #[serde(default)]
    pub quality: Option<crate::repository_review::Review>,
    /// Local SSH route; revalidated against the canonical destination on use.
    #[serde(default)]
    pub desktop_remote: Option<String>,
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

fn git_command(dir: &Path, args: &[&str]) -> Command {
    use std::process::Stdio;
    let mut command = crate::worktree::git_command();
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    use std::io::Read;
    let mut command = git_command(dir, args);
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

/// SSH route on this desktop. The repository path always comes from project
/// setup; an existing remote may supply an alias, never a different destination.
#[derive(Clone, Debug)]
struct SshRoute {
    host: String,
    user: Option<String>,
    port: Option<u16>,
    path: String,
}
impl SshRoute {
    fn parse(remote: &str) -> Option<Self> {
        if remote.starts_with("ssh://") {
            let url = url::Url::parse(remote).ok()?;
            Some(Self {
                host: url.host_str()?.into(),
                user: (!url.username().is_empty()).then(|| url.username().into()),
                port: url.port(),
                path: url.path().trim_start_matches('/').into(),
            })
        } else if !remote.contains("://") && validate_remote(remote).is_ok() {
            let (host, path) = remote.split_once(':')?;
            let (user, host) = match host.split_once('@') {
                Some((user, host)) => (Some(user.into()), host),
                None => (None, host),
            };
            Some(Self {
                host: host.into(),
                user,
                port: None,
                path: path.into(),
            })
        } else {
            None
        }
    }
    fn url(&self) -> Result<String, String> {
        let mut url = url::Url::parse("ssh://placeholder").unwrap();
        url.set_host(Some(&self.host))
            .map_err(|_| "Invalid SSH hostname")?;
        url.set_username(self.user.as_deref().unwrap_or(""))
            .map_err(|_| "Invalid SSH user")?;
        url.set_port(self.port).map_err(|_| "Invalid SSH port")?;
        url.set_path(&self.path);
        validate_remote(url.as_str())
    }
}

/// Canonical endpoint for forge API matching and worker provisioning. Never
/// use this expansion as the desktop Git route: that would lose IdentityFile,
/// ProxyJump and other settings bound to an SSH alias.
pub(crate) fn portable_remote(remote: &str) -> Result<String, String> {
    let Some(route) = SshRoute::parse(remote) else {
        return Ok(remote.into());
    };
    let mut command = Command::new("ssh");
    command.arg("-G");
    if let Some(user) = &route.user {
        command.args(["-l", user]);
    }
    if let Some(port) = route.port {
        command.args(["-p", &port.to_string()]);
    }
    let out = command
        .args(["--", &route.host])
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
    let port = get("port")
        .parse()
        .map_err(|_| "Incomplete SSH host configuration")?;
    if hostname.is_empty() || user.is_empty() {
        return Err("Incomplete SSH host configuration".into());
    }
    SshRoute {
        host: hostname.into(),
        user: Some(user.into()),
        port: Some(port),
        ..route
    }
    .url()
}

fn select_desktop_remote(
    configured: &str,
    candidates: &[String],
    resolve: impl Fn(&str) -> Result<String, String>,
) -> Result<String, String> {
    let Some(route) = SshRoute::parse(configured) else {
        return Ok(configured.into());
    };
    let canonical = resolve(configured)?;
    let target = SshRoute::parse(&canonical).ok_or("Invalid resolved SSH repository")?;
    // An explicitly configured alias already names the owner's chosen route.
    if !route.host.eq_ignore_ascii_case(&target.host) {
        return Ok(configured.into());
    }
    let mut matches = Vec::new();
    for candidate in candidates {
        if validate_remote(candidate).is_err() {
            continue;
        }
        let Some(mut alias) = SshRoute::parse(candidate) else {
            continue;
        };
        if alias.host.eq_ignore_ascii_case(&target.host) {
            continue;
        }
        let Ok(resolved) = resolve(candidate) else {
            continue;
        };
        let Some(endpoint) = SshRoute::parse(&resolved) else {
            continue;
        };
        if endpoint.host.eq_ignore_ascii_case(&target.host)
            && endpoint.user == target.user
            && endpoint.port == target.port
            && endpoint
                .path
                .trim_end_matches(".git")
                .eq_ignore_ascii_case(target.path.trim_end_matches(".git"))
        {
            // Preserve the configured path's spelling even on case-sensitive forges.
            alias.path = target.path.clone();
            matches.push(alias.url()?);
        }
    }
    matches.sort();
    matches.dedup();
    match matches.as_slice() {
        [] => Ok(configured.into()),
        [route] => Ok(route.clone()),
        _ => Err("Multiple SSH aliases match this repository. Set the intended SSH alias clone URL in Project settings.".into()),
    }
}

pub(crate) fn desktop_remote(source: &Path, configured: &str) -> Result<String, String> {
    if SshRoute::parse(configured).is_none() {
        return Ok(configured.into());
    }
    let mut candidates = Vec::new();
    // Only credentials from this project's own remotes qualify. Never fall
    // back to origin: it can be a mirror on an entirely different forge.
    if let Ok(names) = git(source, &["remote"]) {
        for name in names.lines() {
            for args in [
                vec!["remote", "get-url", "--all", name],
                vec!["remote", "get-url", "--push", "--all", name],
            ] {
                if let Ok(urls) = git(source, &args) {
                    candidates.extend(urls.lines().map(String::from));
                }
            }
        }
    }
    select_desktop_remote(configured, &candidates, portable_remote)
}

pub(crate) fn store_dir() -> Result<PathBuf, String> {
    Ok(crate::agents::registry_dir()?.join("repository-transfers"))
}
pub(crate) fn save_at(dir: &Path, transfer: &Transfer) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", transfer.run_id));
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
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
pub(crate) fn list() -> Result<Vec<Transfer>, String> {
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
        if let crate::worker_bootstrap::Target::GitVm {
            local_path: worker_path,
        } = &transfer.worker
        {
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

pub(crate) fn project_for_source<'a>(
    projects: &'a [crate::project_management::ProjectRecord],
    ticket: Option<&str>,
    project_root: &Path,
) -> Result<&'a crate::project_management::ProjectRecord, String> {
    let matches_source = |project: &crate::project_management::ProjectRecord| {
        let local = crate::project_management::local_source_path(project);
        !local.is_empty()
            && crate::sandbox::launch_env::project_root(Path::new(&local)) == project_root
    };
    if let Some(ticket) = ticket {
        let (key, _) = ticket.rsplit_once('-').ok_or("Invalid ticket identity")?;
        let project = projects
            .iter()
            .find(|p| p.key == key)
            .ok_or("Ticket project is not registered")?;
        return if matches_source(project) {
            Ok(project)
        } else {
            Err("The ticket's project does not match this checkout. Link the correct local folder in Project settings before uploading source.".into())
        };
    }
    // Registered sources may themselves be linked worktrees. Compare their
    // common repository root, just as the launcher does for the task worktree.
    let matches: Vec<_> = projects.iter().filter(|p| matches_source(p)).collect();
    match matches.as_slice() {
        [project] => Ok(*project),
        [] => Err("Link this checkout to a project and add its repository in Project settings before dispatching.".into()),
        _ => Err("Multiple projects share this repository. Supply a ticket from the intended project before publishing task data.".into()),
    }
}

/// A task PR must describe the task delta, not everything the launch checkout
/// happened to have ahead of the forge's default branch (PR #105).
fn task_base(
    path: &Path,
    source_checkout: &Path,
    remote: &str,
    source: &str,
) -> Result<String, String> {
    let refs = git(
        path,
        &["ls-remote", "--symref", remote, "HEAD", "refs/heads/*"],
    )?;
    let default = refs
        .lines()
        .find_map(|line| {
            line.strip_prefix("ref: refs/heads/")
                .and_then(|v| v.split_once('\t'))
                .map(|(name, _)| name.to_string())
        })
        .ok_or("The configured repository needs a default branch with an initial commit.")?;
    let heads: std::collections::BTreeMap<String, String> = refs
        .lines()
        .filter_map(|line| {
            let (sha, reference) = line.split_once('\t')?;
            let name = reference.strip_prefix("refs/heads/")?;
            (sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| (name.to_string(), sha.to_string()))
        })
        .collect();
    let public_branch =
        |name: &str| !name.starts_with("xnaut/inputs/") && !name.starts_with("xnaut/runs/");
    let preferred = git(
        source_checkout,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
    )
    .ok();
    let mut candidates = Vec::new();
    if let Some(name) = preferred.filter(|name| public_branch(name) && heads.contains_key(name)) {
        candidates.push(name);
    }
    if !candidates.contains(&default) {
        candidates.push(default);
    }
    for name in candidates {
        let Some(tip) = heads.get(&name) else {
            continue;
        };
        if tip == source {
            return Ok(name);
        }
        git(path, &["fetch", "--no-tags", remote, tip])?;
        if git(path, &["merge-base", "--is-ancestor", source, tip]).is_ok() {
            return Ok(name);
        }
    }
    // A detached verification checkout can still identify an exact, uniquely
    // published source branch. Never choose an arbitrary alias or input ref.
    let exact: Vec<_> = heads
        .iter()
        .filter(|(name, tip)| public_branch(name) && *tip == source)
        .collect();
    match exact.as_slice() {
        [(name, _)] => Ok((*name).clone()),
        [] => Err("The task's starting commit is not on a published source branch. Push that branch to the project's repository before starting the task; refusing a PR with unrelated source changes.".into()),
        _ => Err("Several published branches match this detached checkout. Select the intended source branch in the project's local folder before starting the task.".into()),
    }
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
    let project = project_for_source(&projects, ticket.as_deref(), project_root)?;
    let configured = validate_remote(&project.forge_remote)?;
    if !git(path, &["status", "--porcelain"])?.is_empty() {
        return Err("Commit or stash this checkout's changes before dispatching. Repository runs use an exact committed revision.".into());
    }
    let source_sha = git(path, &["rev-parse", "HEAD"])?;
    let remote = portable_remote(&configured)?;
    let desktop = desktop_remote(path, &configured)?;
    let review_parent = crate::repository_review::parent_for_workspace(path)?;
    let quality = review_parent
        .is_none()
        .then(crate::repository_review::Review::default);
    let mut prepared = Transfer {
        review_parent,
        repair_parent: None,
        quality,
        run_id: run_id.into(),
        project: project.key.clone(),
        ticket,
        handle: handle.trim_start_matches('@').to_ascii_lowercase(),
        local_path: path.to_string_lossy().into(),
        local_branch: git(path, &["symbolic-ref", "--short", "HEAD"]).unwrap_or_default(),
        remote,
        desktop_remote: Some(desktop),
        worker_remote: None,
        worker: Default::default(),
        source_sha,
        base: String::new(),
        branch: format!("xnaut/runs/{run_id}"),
        workdir: format!("agents/runs/{run_id}"),
        artifacts: format!(".xnaut/runs/{run_id}"),
        state: "prepared".into(),
        pr_url: None,
        error: None,
        filed: false,
    };
    prepare_delivery_base(
        &mut prepared,
        Path::new(&crate::project_management::local_source_path(project)),
        &list()?,
    )?;
    Ok(prepared)
}

/// A validated repair continues its existing PR even when its reviewed head
/// exists only on the native publication branch. Fresh tasks still require a
/// published public source branch; never relax that rule for arbitrary inputs.
fn prepare_delivery_base(t: &mut Transfer, source_checkout: &Path, rows: &[Transfer]) -> Result<(), String> {
    inherit_repair_delivery(t, rows)?;
    if t.repair_parent.is_none() {
        t.base = if let Some(parent) = &t.review_parent {
            rows.iter().find(|row| &row.run_id == parent)
                .ok_or("Parent review receipt missing")?.base.clone()
        } else {
            task_base(Path::new(&t.local_path), source_checkout,
                t.desktop_remote.as_deref().ok_or("Desktop repository route missing")?, &t.source_sha)?
        };
    }
    git(Path::new(&t.local_path), &["check-ref-format", &format!("refs/heads/{}", t.base)])?;
    Ok(())
}

/// Only a persisted, exact repair reservation may inherit publication rights.
/// A caller-provided ticket, branch or PR alone never creates this linkage.
pub(crate) fn inherit_repair_delivery(t: &mut Transfer, rows: &[Transfer]) -> Result<(), String> {
    let parents: Vec<_> = rows
        .iter()
        .filter(|parent| {
            parent.quality.as_ref().is_some_and(|q| {
                q.repair_child.as_deref() == Some(t.run_id.as_str()) && q.state == "repair_reserved"
            })
        })
        .collect();
    if parents.len() > 1 {
        return Err("Repair reservation has multiple parents".into());
    }
    let Some(parent) = parents.first() else {
        return Ok(());
    };
    let q = parent.quality.as_ref().unwrap();
    if t.review_parent.is_some()
        || parent.project != t.project
        || parent.ticket != t.ticket
        || parent.handle != t.handle
        || parent.remote != t.remote
        || parent.local_path != t.local_path
        || parent.local_branch.is_empty()
        || parent.local_branch != t.local_branch
        || parent.pr_url.is_none()
        || t.source_sha != q.head
    {
        return Err(
            "Repair receipt differs from its authorized predecessor/worktree/revision".into(),
        );
    }
    t.repair_parent = Some(parent.run_id.clone());
    t.branch = parent.branch.clone();
    t.base = parent.base.clone();
    t.pr_url = parent.pr_url.clone();
    t.quality = None;
    Ok(())
}

pub(crate) fn transfer_desktop_remote(t: &Transfer) -> Result<String, String> {
    if let Some(route) = &t.desktop_remote {
        if portable_remote(route)? == t.remote {
            return Ok(route.clone());
        }
        return Err("The desktop SSH alias now resolves to a different repository. Restore its configuration before retrying delivery.".into());
    }
    desktop_remote(Path::new(&t.local_path), &t.remote)
}

pub fn stage(transfer: &Transfer, access: &crate::worker_bootstrap::Access) -> Result<(), String> {
    // The common task bootstrap has already provisioned and checked Git/LFS
    // write access. Keep a separate SSH identity in THIS checkout's config.
    let input = format!("refs/heads/xnaut/inputs/{}", transfer.run_id);
    let desktop = transfer_desktop_remote(transfer)?;
    git(
        Path::new(&transfer.local_path),
        &["lfs", "push", &desktop, &transfer.source_sha],
    )?;
    git(
        Path::new(&transfer.local_path),
        &[
            "push",
            &desktop,
            &format!("{}:{input}", transfer.source_sha),
        ],
    )?;
    // A fresh directory per run, with real history. Never delete an earlier run.
    let proxy_config = access
        .http_proxy
        .as_ref()
        .map(|proxy| format!("git config --local http.proxy {}; ", quote(proxy)))
        .unwrap_or_default();
    let command = format!("set -e; export GIT_TERMINAL_PROMPT=0; test ! -e {dir}; mkdir -p {parent}; git -c core.sshCommand={ssh} clone --no-checkout -- {remote} {dir}; cd {dir}; git config --local core.sshCommand {ssh}; {proxy_config}git config --local user.name xNAUT; git config --local user.email xnaut@localhost; git lfs install --local; git fetch origin {input}; test \"$(git rev-parse FETCH_HEAD)\" = {sha}; git checkout -b {branch} {sha}; git push --dry-run origin HEAD:refs/heads/{branch}; test ! -L .xnaut; test ! -L .xnaut/runs; mkdir -p {artifacts}",
        dir=quote(&transfer.workdir), parent=quote(Path::new(&transfer.workdir).parent().ok_or("Invalid worker directory")?.to_str().ok_or("Invalid worker directory")?), remote=quote(&access.remote), ssh=quote(&access.ssh_command), input=quote(&input), sha=quote(&transfer.source_sha), branch=quote(&transfer.branch), artifacts=quote(&transfer.artifacts));
    transfer.worker.command(&command)?;
    let metadata = serde_json::to_string_pretty(transfer).map_err(|e| e.to_string())?;
    transfer
        .worker
        .file(&transfer.workdir, ".git/xnaut-transfer.json", &metadata)?;
    transfer
        .worker
        .file(&transfer.workdir, ".git/xnaut-publish.py", PUBLISHER)?;
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
    let directory = if Path::new(&t.workdir).is_absolute() {
        quote(&t.workdir)
    } else {
        format!("\"$HOME\"/{}", quote(&t.workdir))
    };
    format!("#!/bin/bash -l\ncd {} || exit 1\n{}\nprintf '%s\\n' 'xNAUT: repository-backed run; results are retained until uploaded.'\necho $$ > .git/xnaut-supervisor.pid\nprintf running > .git/xnaut-phase\nenv {}\ncode=$?\nprintf uploading > .git/xnaut-phase\npython3 .git/xnaut-publish.py --finish \"$code\" --retry\npublished=$?\nif [ \"$published\" = 0 ]; then printf finished > .git/xnaut-phase; fi\nexit \"$code\"\n", directory, seed, command)
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ResultRecord {
    run_id: String,
    source_sha: String,
    pub exit_code: i32,
    pub uncommitted_source: bool,
    /// Computed from the fetched Git object, never accepted from worker JSON.
    #[serde(skip)]
    pub published_head: String,
}

pub(crate) fn fetch_result(
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
    let desktop = transfer_desktop_remote(t)?;
    if git(&dir, &["ls-remote", &desktop, &reference])?.is_empty() {
        return Ok(None);
    }
    git(&dir, &["fetch", "--no-tags", &desktop, &reference])?;
    let published_head = git(&dir, &["rev-parse", "FETCH_HEAD"])?;
    git(
        &dir,
        &[
            "merge-base",
            "--is-ancestor",
            &t.source_sha,
            &published_head,
        ],
    )?;
    let result_path = format!("{published_head}:{}/result.json", t.artifacts);
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
    let mut result: ResultRecord = serde_json::from_str(&read(&result_path)?)
        .map_err(|e| format!("Invalid run result: {e}"))?;
    if result.run_id != t.run_id || result.source_sha != t.source_sha {
        return Err("Run result does not match its launch receipt".into());
    }
    result.published_head = published_head.clone();
    let handback_path = format!("{published_head}:{}/handback.json", t.artifacts);
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

/// Publication is a commit locator, not success or liveness. Keep the exact
/// fetched revision even when a preceding remote probe saw the pre-publish HEAD.
fn record_publication_in(
    registry: &Path,
    t: &Transfer,
    result: &ResultRecord,
) -> Result<(), String> {
    if !crate::run_control::list_ids_in(registry)?
        .iter()
        .any(|id| id == &t.run_id)
    {
        return Ok(());
    }
    let run = crate::run_control::load_manifest_in(registry, &t.run_id)?;
    if run.ticket != t.ticket
        || (!run.project.is_empty() && run.project != t.project)
        || run.agent_handle != t.handle
        || run.worktree_path != t.local_path
    {
        return Err("Published commit locator does not match its native assignment".into());
    }
    if run.last_commit != result.published_head {
        crate::run_control::update_in(registry, &t.run_id, |run| {
            run.last_commit = result.published_head.clone();
            run.last_signal = format!(
                "Repository publication fetched at {}; verification remains separate",
                result.published_head
            );
        })?;
    }
    Ok(())
}

/// Publication alone does not complete a reviewer. Bind its typed handback
/// through the saved parent reservation before completing only the child task.
fn accept_reviewer_handback_in(
    registry: &Path, child: &Transfer, parent: &Transfer, result: &ResultRecord,
    handback: Option<&crate::handback::Handback>,
) -> Result<bool, String> {
    let q = parent.quality.as_ref().ok_or("Reviewer parent reservation is missing")?;
    let ticket = parent.ticket.as_deref().ok_or("Reviewer parent ticket is missing")?;
    if child.review_parent.as_deref() != Some(parent.run_id.as_str()) || child.repair_parent.is_some()
        || child.ticket.is_some() || q.child.as_deref() != Some(child.run_id.as_str())
        || child.project != parent.project || child.remote != parent.remote || child.source_sha != q.head
        || child.handle != q.reviewer || child.handle == parent.handle
        || child.local_path != q.worktree || child.local_path == parent.local_path
        || child.workdir != child.worker.run_directory(&child.run_id)
        || child.artifacts != format!(".xnaut/runs/{}", child.run_id)
        || child.branch != format!("xnaut/runs/{}", child.run_id)
        || result.run_id != child.run_id || result.source_sha != child.source_sha
        || result.exit_code != 0 || result.uncommitted_source
    { return Err("Reviewer publication does not match its parent reservation".into()); }
    let mut handback = handback.cloned().ok_or("Reviewer typed handback is missing")?;
    if handback.run_id.as_ref().is_some_and(|id| id != &child.run_id)
        || (!handback.ticket.is_empty() && handback.ticket != ticket)
        || (!handback.from.is_empty() && handback.from.trim_start_matches('@') != child.handle)
    { return Err("Reviewer handback names a different assignment".into()); }
    // Identity is supplied by the trusted saved receipt, as for author filing.
    // This is never filed as the author's PM handback.
    handback.run_id = Some(child.run_id.clone()); handback.ticket = ticket.into(); handback.from = child.handle.clone();
    crate::run_control::record_review_handback_in(registry, child, &handback, &result.published_head)
}

fn finish_reviewer_task(t: &Transfer, result: &ResultRecord, handback: Option<&crate::handback::Handback>) -> Result<(), String> {
    let parent_id = t.review_parent.as_deref().ok_or("Reviewer parent receipt is missing")?;
    if parent_id.is_empty() || !parent_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err("Invalid reviewer parent identity".into());
    }
    let parent: Transfer = serde_json::from_slice(&std::fs::read(store_dir()?.join(format!("{parent_id}.json")))
        .map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if parent.run_id != parent_id { return Err("Reviewer parent receipt identity differs from its file".into()); }
    accept_reviewer_handback_in(&crate::agents::registry_dir()?, t, &parent, result, handback)?;
    Ok(())
}

fn publication_pending(t: &Transfer) -> bool {
    ["running", "pushed"].contains(&t.state.as_str()) || (t.state == "review" && t.review_parent.is_some())
}

async fn reconcile(t: &mut Transfer, hosts: &[crate::settings::ForgeHost]) -> Result<(), String> {
    if t.state == "review" && t.review_parent.is_some() {
        let run = crate::run_control::load_manifest_in(&crate::agents::registry_dir()?, &t.run_id)?;
        // Historical liveness failures may predate collection of an accepted
        // reviewer handback. Recheck its immutable publication; run_control
        // permits only an exact journal-proven liveness failure to complete.
        if matches!(run.state, crate::run_control::RunState::Done | crate::run_control::RunState::Retired) { return Ok(()); }
    }
    if !t.workdir.is_empty() && t.state == "running" {
        let snapshot = t.clone();
        // Liveness evidence comes from the worker, never from opening a viewport.
        if let Ok(Ok(proof)) =
            tokio::task::spawn_blocking(move || snapshot.worker.probe(&snapshot.workdir)).await
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
    record_publication_in(&crate::agents::registry_dir()?, t, &result)?;
    let reviewer_handback = t.review_parent.as_ref().and(handback.clone());
    t.state = "pushed".into();
    let mut pr_error = None;
    if t.pr_url.is_none() && t.review_parent.is_none() {
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
        // Published implementation remains reviewable after a failed author.
        // Uncommitted source is explicitly retained and prevents any pass.
        t.state = "review".into();
        return Err(format!("Published evidence retained: author exit {}; uncommitted source {}. Independent review required; process exit is not completion.", result.exit_code, result.uncommitted_source));
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
                    "caller": null, "owner": crate::agent_profiles::RESERVED_NAUTBOT_HANDLE, "status": "review"
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
    if t.review_parent.is_some() { finish_reviewer_task(t, &result, reviewer_handback.as_ref())?; }
    Ok(())
}

/// Only receipts minted by this installation are fetched. Repository content
/// is parsed as bounded JSON, never executed, and cannot select a ticket/actor.
pub fn spawn_reconciler(_app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            crate::repository_notes::drain().await;
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

/// One pass owned by the durable sweep. No second scheduler or polling task.
pub async fn tick(app: &tauri::AppHandle) {
    use tauri::Manager;
    static TICK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let Ok(_tick) = TICK.try_lock() else {
        return;
    };
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
            .filter(publication_pending)
        {
            // Publication and quality advancement share a cross-process lease;
            // a late publisher must not overwrite a freshly launched review.
            let Ok(dir) = store_dir() else {
                continue;
            };
            let Ok(Some(_lease)) = crate::repository_review::QualityLease::acquire(&dir, &t.run_id)
            else {
                continue;
            };
            let Ok(bytes) = std::fs::read(dir.join(format!("{}.json", t.run_id))) else {
                continue;
            };
            let Ok(current) = serde_json::from_slice::<Transfer>(&bytes) else {
                continue;
            };
            t = current;
            if !publication_pending(&t) {
                continue;
            }
            t.error = reconcile(&mut t, &hosts).await.err();
            let _ = save(&t);
        }
    }
    if let Ok(rows) = list() {
        for mut t in rows
            .iter()
            .filter(|t| {
                t.state == "review"
                    && t.review_parent.is_none()
                    && t.repair_parent.is_none()
                    && t.quality.is_some()
            })
            .cloned()
        {
            let _ = crate::repository_review::advance(app, &mut t, &rows, &hosts).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_ssh_route_preserves_identity_and_exact_configured_destination() {
        let configured = "ssh://git@forge.example:2222/Team/Repo.git";
        let resolve = |remote: &str| -> Result<String, String> {
            Ok(match remote {
                "work:team/repo.git" | "ssh://work/Team/Repo.git" => configured.into(),
                "mirror:team/repo.git" => "ssh://git@github.com:22/Team/Repo.git".into(),
                "other-user:team/repo.git" => "ssh://other@forge.example:2222/Team/Repo.git".into(),
                "other-port:team/repo.git" => "ssh://git@forge.example:22/Team/Repo.git".into(),
                "other-repo:team/else.git" => "ssh://git@forge.example:2222/Team/else.git".into(),
                "work2:team/repo.git" => configured.into(),
                _ => remote.into(),
            })
        };
        let wrong: Vec<String> = [
            "mirror:team/repo.git",
            "other-user:team/repo.git",
            "other-port:team/repo.git",
            "other-repo:team/else.git",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            select_desktop_remote(configured, &wrong, resolve).unwrap(),
            configured
        );
        let mut candidates = wrong;
        candidates.extend(["work:team/repo.git".into(), "work:team/repo.git".into()]);
        assert_eq!(
            select_desktop_remote(configured, &candidates, resolve).unwrap(),
            "ssh://work/Team/Repo.git"
        );
        assert_eq!(
            select_desktop_remote("work:team/repo.git", &[], resolve).unwrap(),
            "work:team/repo.git"
        );
        candidates.push("work2:team/repo.git".into());
        assert!(select_desktop_remote(configured, &candidates, resolve)
            .unwrap_err()
            .contains("Multiple"));
        assert_eq!(
            select_desktop_remote("https://forge.example/Team/Repo.git", &candidates, resolve)
                .unwrap(),
            "https://forge.example/Team/Repo.git"
        );
    }

    /// Explicit operator-only acceptance check: provisions repository access,
    /// performs read/write-dry-run and empty LFS batch probes, starts no agent.
    #[tokio::test]
    #[ignore = "requires an explicitly selected live project and worker"]
    async fn live_repository_access_preflight() {
        let source = std::env::var("XNAUT_PREFLIGHT_SOURCE").expect("explicit source required");
        let remote = validate_remote(
            &std::env::var("XNAUT_PREFLIGHT_REMOTE").expect("explicit remote required"),
        )
        .unwrap();
        let desktop = desktop_remote(Path::new(&source), &remote).unwrap();
        let refs = git(
            Path::new(&source),
            &["ls-remote", "--symref", &desktop, "HEAD"],
        )
        .unwrap();
        assert!(refs.contains("ref: refs/heads/"));
        println!("desktop repository read: passed");
        git(
            Path::new(&source),
            &[
                "push",
                "--dry-run",
                &desktop,
                "HEAD:refs/heads/xnaut/preflight-access-check",
            ],
        )
        .unwrap();
        println!("desktop repository write dry-run: passed");
        let settings = crate::settings::load_or_default();
        let access = crate::worker_bootstrap::prepare(
            &crate::worker_bootstrap::Target::ExeDev,
            &portable_remote(&remote).unwrap(),
            "xnaut/preflight-access-check",
            &settings.forges,
            &settings.worker_network,
        )
        .await
        .unwrap();
        assert!(!access.ssh_command.is_empty());
        println!("worker tools, network, repository read/write dry-run and LFS upload authorization: passed");
    }

    #[test]
    fn ticketless_tasks_resolve_a_project_registered_to_a_linked_worktree() {
        struct Scratch(PathBuf);
        impl Scratch {
            fn path(&self) -> &Path {
                &self.0
            }
        }
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let dir = Scratch(
            std::env::temp_dir().join(format!("xnaut-source-project-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(dir.path()).unwrap();
        let main = dir.path().join("main");
        let linked = dir.path().join("registered-source");
        git(dir.path(), &["init", main.to_str().unwrap()]).unwrap();
        git(
            &main,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "base",
            ],
        )
        .unwrap();
        git(
            &main,
            &[
                "worktree",
                "add",
                "-b",
                "registered",
                linked.to_str().unwrap(),
            ],
        )
        .unwrap();
        // A fixture must not inherit a real machine's XNAUT project-path override.
        let key = format!("FIXTURE{}", uuid::Uuid::new_v4().simple());
        let ticket = format!("{key}-1");
        let project: crate::project_management::ProjectRecord =
            serde_json::from_value(serde_json::json!({
                "key":key, "name":"fixture", "source_path":linked,
                "forge_remote":"https://forge.example/team/xnaut.git", "created_at":"fixture"
            }))
            .unwrap();
        let root = crate::sandbox::launch_env::project_root(&linked);
        let mut projects = vec![project.clone()];
        assert_eq!(project_for_source(&projects, None, &root).unwrap().key, key);
        assert!(project_for_source(&projects, Some("OTHER-1"), &root).is_err());
        projects.push(crate::project_management::ProjectRecord {
            key: format!("{key}B"),
            ..project
        });
        assert!(project_for_source(&projects, None, &root)
            .unwrap_err()
            .contains("Multiple"));
        assert_eq!(
            project_for_source(&projects, Some(&ticket), &root)
                .unwrap()
                .key,
            key
        );
        assert!(project_for_source(&projects, Some(&ticket), dir.path()).is_err());
    }
    #[test]
    fn task_pr_base_excludes_inherited_feature_changes_and_refuses_unpublished_sources() {
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let scratch =
            Scratch(std::env::temp_dir().join(format!("xnaut-pr-base-{}", uuid::Uuid::new_v4())));
        std::fs::create_dir_all(&scratch.0).unwrap();
        let root = scratch.0.join("source");
        let remote = scratch.0.join("remote.git");
        let task = scratch.0.join("task");
        git(
            &scratch.0,
            &["init", "--bare", "-b", "main", remote.to_str().unwrap()],
        )
        .unwrap();
        git(&scratch.0, &["init", "-b", "main", root.to_str().unwrap()]).unwrap();
        git(&root, &["config", "user.name", "Fixture"]).unwrap();
        git(&root, &["config", "user.email", "fixture@example.invalid"]).unwrap();
        std::fs::write(root.join("app.txt"), "main\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-m", "main"]).unwrap();
        let main = git(&root, &["rev-parse", "HEAD"]).unwrap();
        let remote = remote.to_str().unwrap();
        git(&root, &["push", remote, "main"]).unwrap();
        assert_eq!(task_base(&root, &root, remote, &main).unwrap(), "main");
        git(&root, &["checkout", "-b", "feature"]).unwrap();
        std::fs::write(root.join("app.txt"), "unrelated feature\n").unwrap();
        git(&root, &["commit", "-am", "feature"]).unwrap();
        let source = git(&root, &["rev-parse", "HEAD"]).unwrap();
        assert!(task_base(&root, &root, remote, &source)
            .unwrap_err()
            .contains("published source branch"));
        git(&root, &["push", remote, "feature"]).unwrap();
        git(
            &root,
            &[
                "worktree",
                "add",
                "--detach",
                task.to_str().unwrap(),
                &source,
            ],
        )
        .unwrap();
        assert_eq!(task_base(&task, &root, remote, &source).unwrap(), "feature");
        assert_eq!(task_base(&task, &task, remote, &source).unwrap(), "feature");
        std::fs::write(task.join("report.md"), "smoke evidence\n").unwrap();
        git(&task, &["add", "report.md"]).unwrap();
        git(&task, &["commit", "-m", "task report"]).unwrap();
        let base = task_base(&task, &root, remote, &source).unwrap();
        assert_eq!(
            git(&task, &["diff", "--name-only", &format!("{base}...HEAD")]).unwrap(),
            "report.md"
        );
        assert!(git(&task, &["diff", "--name-only", "main...HEAD"])
            .unwrap()
            .contains("app.txt"));
        // Aliased detached tips must not choose a random parent. A checked-out
        // source branch still disambiguates them, and internal input refs don't.
        git(
            &root,
            &[
                "push",
                remote,
                "feature:refs/heads/alias",
                "feature:refs/heads/xnaut/inputs/fixture",
            ],
        )
        .unwrap();
        assert!(task_base(&task, &task, remote, &source)
            .unwrap_err()
            .contains("Several published branches"));
        assert_eq!(task_base(&task, &root, remote, &source).unwrap(), "feature");
    }
    /// XNAUT-465/467: a real published author commit ahead of main must
    /// resume its reserved PR, while the same commit cannot start a fresh task.
    #[test]
    fn reserved_repair_inherits_base_before_fresh_source_branch_validation() {
        struct Scratch(PathBuf);
        impl Scratch { fn path(&self) -> &Path { &self.0 } }
        impl Drop for Scratch { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
        let dir = Scratch(std::env::temp_dir().join(format!("xnaut-repair-base-{}", uuid::Uuid::new_v4())));
        std::fs::create_dir_all(dir.path()).unwrap();
        let root = dir.path().join("source");
        let remote = dir.path().join("remote.git");
        git(dir.path(), &["init", "--bare", "-b", "main", remote.to_str().unwrap()]).unwrap();
        git(dir.path(), &["init", "-b", "main", root.to_str().unwrap()]).unwrap();
        git(&root, &["config", "user.name", "Fixture"]).unwrap();
        git(&root, &["config", "user.email", "fixture@example.invalid"]).unwrap();
        git(&root, &["config", "commit.gpgsign", "false"]).unwrap();
        std::fs::write(root.join("app.txt"), "main\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-m", "main"]).unwrap();
        let base = git(&root, &["rev-parse", "HEAD"]).unwrap();
        git(&root, &["push", remote.to_str().unwrap(), "main"]).unwrap();
        git(&root, &["checkout", "-b", "agent/author/task"]).unwrap();
        std::fs::write(root.join("app.txt"), "author implementation\n").unwrap();
        git(&root, &["commit", "-am", "author implementation"]).unwrap();
        let head = git(&root, &["rev-parse", "HEAD"]).unwrap();
        git(&root, &["push", remote.to_str().unwrap(), "HEAD:refs/heads/xnaut/runs/original"]).unwrap();
        let parent: Transfer = serde_json::from_value(serde_json::json!({
            "run_id":"original","project":"TEST","ticket":"TEST-1","handle":"author",
            "local_path":root,"local_branch":"agent/author/task",
            "remote":"https://forge.example/team/repo.git","source_sha":base,
            "base":"main","branch":"xnaut/runs/original","workdir":"worker",
            "artifacts":".xnaut/runs/original","state":"review",
            "pr_url":"https://forge.example/team/repo/pulls/17","error":null,
            "quality": {"state":"repair_reserved","head":head,"base":base,"repair_child":"repair",
                "reviewer":"ralph","worktree":"review","child":"reviewer","attempts":1,
                "message":"Reserved author repair","report":null,"jev":null,"comment_url":null}
        })).unwrap();
        // Roundtrip persisted receipt: the production seam consumes the same
        // exact reservation identities, not a caller-provided desired PR.
        let rows: Vec<Transfer> = serde_json::from_slice(&serde_json::to_vec(std::slice::from_ref(&parent)).unwrap()).unwrap();
        let candidate = Transfer { run_id:"repair".into(), source_sha:head.clone(), base:String::new(),
            desktop_remote:Some(remote.to_string_lossy().into()), branch:"xnaut/runs/repair".into(),
            artifacts:".xnaut/runs/repair".into(), pr_url:None, quality:Some(Default::default()), ..parent.clone() };
        let mut prepared = candidate.clone();
        prepare_delivery_base(&mut prepared, &root, &rows).unwrap();
        assert_eq!(prepared.base, "main"); assert_eq!(prepared.branch, parent.branch);
        assert_eq!(prepared.pr_url, parent.pr_url); assert_eq!(prepared.source_sha, head);
        assert_eq!(prepared.repair_parent.as_deref(), Some("original"));
        assert!(prepared.quality.is_none());
        assert_eq!(git(&root, &["ls-remote", remote.to_str().unwrap(), "refs/heads/xnaut/runs/original"]).unwrap().split_whitespace().next(), Some(head.as_str()));
        for mutation in ["missing", "unreserved", "head", "local_branch", "ticket", "remote", "duplicate"] {
            let mut child = candidate.clone(); let mut altered = rows.clone();
            match mutation {
                "missing" => altered.clear(),
                "unreserved" => altered[0].quality.as_mut().unwrap().state = "blocked".into(),
                "head" => child.source_sha = base.clone(),
                "local_branch" => child.local_branch = "other".into(),
                "ticket" => child.ticket = Some("TEST-2".into()),
                "remote" => child.remote = "https://forge.example/other/repo.git".into(),
                "duplicate" => altered.push(parent.clone()),
                _ => unreachable!(),
            }
            assert!(prepare_delivery_base(&mut child, &root, &altered).is_err(), "{mutation}");
        }
        let mut fresh = candidate.clone(); fresh.run_id = "new-task".into();
        assert!(prepare_delivery_base(&mut fresh, &root, &rows).unwrap_err().contains("published source branch"));
        // The ordinary fresh-task route still accepts the actual public base.
        fresh.source_sha = base;
        prepare_delivery_base(&mut fresh, &root, &rows).unwrap();
        assert_eq!(fresh.base, "main"); assert!(fresh.repair_parent.is_none());
        assert!(fresh.pr_url.is_none());
    }
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
    #[cfg(unix)]
    #[test]
    fn native_git_inherits_runtime_path_for_external_subcommands_without_changing_parent() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("xnaut-git-path-{}", uuid::Uuid::new_v4()));
        let helpers = root.join("tool installation with spaces");
        std::fs::create_dir_all(&helpers).unwrap();
        let helper = helpers.join("git-xnaut-lfs-path-fixture");
        std::fs::write(
            &helper,
            "#!/bin/sh\nprintf 'external-helper-ready:%s' \"$1\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let before = std::env::var_os("PATH");
        let expected = crate::agents::runtime_path_public().unwrap();
        let mut command = git_command(&root, &["xnaut-lfs-path-fixture", "push"]);
        let configured = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("PATH"))
            .and_then(|(_, value)| value)
            .expect("native Git must explicitly receive the GUI-safe runtime PATH")
            .to_owned();
        assert_eq!(configured, std::ffi::OsString::from(&expected));
        let dirs: Vec<_> = std::iter::once(helpers.clone())
            .chain(std::env::split_paths(&configured))
            .collect();
        command.env("PATH", std::env::join_paths(dirs).unwrap());
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "Git must resolve external subcommands from its child PATH"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "external-helper-ready:push"
        );
        assert_eq!(
            std::env::var_os("PATH"),
            before,
            "no process-global environment mutation"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collected_reviewer_handback_completes_only_its_bound_task_and_replays() {
        use crate::run_control::{self, RunManifest, RunState};
        let registry = std::env::temp_dir().join(format!("xnaut-review-terminal-{}",uuid::Uuid::new_v4()));
        let child: Transfer = serde_json::from_value(serde_json::json!({
            "run_id":"review-child","project":"TEST","ticket":null,"handle":"reviewer",
            "local_path":"/fixture/reviewer","local_branch":"","remote":"ssh://fixture/repo.git",
            "review_parent":"author-parent","source_sha":"a".repeat(40),"base":"main",
            "branch":"xnaut/runs/review-child","workdir":"agents/runs/review-child",
            "artifacts":".xnaut/runs/review-child","state":"review","pr_url":null,"error":null
        })).unwrap();
        let mut parent = child.clone(); parent.run_id = "author-parent".into(); parent.ticket = Some("TEST-1".into());
        parent.handle = "author".into(); parent.local_path = "/fixture/author".into(); parent.review_parent = None;
        parent.quality = Some(serde_json::from_value(serde_json::json!({
            "state":"blocked","reviewer":"reviewer","head":"a".repeat(40),"base":"b".repeat(40),
            "child":"review-child","worktree":"/fixture/reviewer","message":"test evidence is missing","attempts":1
        })).unwrap());
        let parent_before = serde_json::to_value(&parent).unwrap();
        let mut native = RunManifest::requested("reviewer","fixture",&child.local_path,None,None,&[],1);
        native.run_id = child.run_id.clone(); native.project = child.project.clone(); native.remote_env = Some("exe-dev".into());
        run_control::request_in(&registry, native, || Ok(())).unwrap();
        run_control::update_in(&registry,&child.run_id,|run| {
            run.state = RunState::Running; run.pid = Some(123); run.pty_session = Some("interactive-reviewer".into());
        }).unwrap();
        let result = ResultRecord { run_id:child.run_id.clone(), source_sha:child.source_sha.clone(),
            exit_code:0, uncommitted_source:false, published_head:"c".repeat(40) };
        let handback = crate::handback::Handback {
            summary:"Independent review requires an author repair".into(), files_changed:vec![format!("{}/review.json",child.artifacts)],
            commits:vec!["c".repeat(40)], how_verified:"python3 -m unittest -v: one regression failed".into(),
            not_finished:Some("Author must repair the reported defect".into()), confidence:crate::handback::Confidence::High,
            ..Default::default()
        };
        assert!(publication_pending(&child), "already-collected review children must reconcile on restart");
        assert!(accept_reviewer_handback_in(&registry,&child,&parent,&result,None).is_err());
        for field in ["run_id","ticket","from","summary"] {
            let mut wrong = serde_json::to_value(&handback).unwrap(); wrong[field] = serde_json::json!(if field == "summary" { "" } else { "wrong" });
            let wrong = serde_json::from_value(wrong).unwrap();
            assert!(accept_reviewer_handback_in(&registry,&child,&parent,&result,Some(&wrong)).is_err(),"{field}");
        }
        for field in ["review_parent","project","remote","source_sha","handle","local_path","workdir","artifacts","branch"] {
            let mut wrong = serde_json::to_value(&child).unwrap(); wrong[field] = serde_json::json!("wrong");
            let wrong = serde_json::from_value(wrong).unwrap();
            assert!(accept_reviewer_handback_in(&registry,&wrong,&parent,&result,Some(&handback)).is_err(),"{field}");
        }
        let mut unreserved = parent.clone(); unreserved.quality = None;
        assert!(accept_reviewer_handback_in(&registry,&child,&unreserved,&result,Some(&handback)).is_err());
        unreserved = parent.clone(); unreserved.quality.as_mut().unwrap().child = Some("another-reviewer".into());
        assert!(accept_reviewer_handback_in(&registry,&child,&unreserved,&result,Some(&handback)).is_err());
        run_control::update_in(&registry,&child.run_id,|run| run.project = "ANOTHER".into()).unwrap();
        assert!(accept_reviewer_handback_in(&registry,&child,&parent,&result,Some(&handback)).is_err());
        run_control::update_in(&registry,&child.run_id,|run| run.project = child.project.clone()).unwrap();
        assert_eq!(run_control::worker_count_in(&registry).unwrap(),1);
        // Fetching a result alone must never release capacity.
        record_publication_in(&registry,&child,&result).unwrap();
        assert_eq!(run_control::load_manifest_in(&registry,&child.run_id).unwrap().state,RunState::Running);
        assert!(accept_reviewer_handback_in(&registry,&child,&parent,&result,Some(&handback)).unwrap());
        let completed = run_control::load_manifest_in(&registry,&child.run_id).unwrap();
        assert_eq!(completed.state,RunState::Done); assert_eq!(completed.last_commit,result.published_head);
        assert_eq!(completed.pid,Some(123), "task completion makes no physical process-exit claim");
        assert!(completed.last_signal.contains("parent verdict remains separate"));
        assert_eq!(run_control::worker_count_in(&registry).unwrap(),0);
        assert_eq!(serde_json::to_value(&parent).unwrap(),parent_before,"blocked parent verdict remains unchanged");
        assert!(!accept_reviewer_handback_in(&registry,&child,&parent,&result,Some(&handback)).unwrap());
        assert_eq!(run_control::load_manifest_in(&registry,&child.run_id).unwrap().revision,completed.revision);
        std::fs::remove_dir_all(registry).unwrap();

        // Replay historical liveness failures only after the exact collected
        // publication and parent-bound handback establish task completion.
        for case in ["stall", "stall_with_cpu", "missing", "exit", "exit_receipt", "unknown", "wrong_head", "changed_identity", "successor",
            "admission", "retired", "uncollected", "no_journal", "wrong_handback", "wrong_parent", "dirty", "before_publication"] {
            let registry = std::env::temp_dir().join(format!("xnaut-review-recovery-{}",uuid::Uuid::new_v4()));
            let mut native = completed.clone(); native.state = RunState::Requested; native.last_commit = child.source_sha.clone();
            native.revision = 0;
            run_control::request_in(&registry, native, || Ok(())).unwrap();
            run_control::update_in(&registry,&child.run_id,|run| {
                run.state = RunState::Running;
                run.last_commit = if case == "before_publication" { child.source_sha.clone() } else { result.published_head.clone() };
            }).unwrap();
            run_control::update_in(&registry,&child.run_id,|run| {
                run.state = if case == "retired" { RunState::Retired } else { RunState::Failed };
                run.last_signal = match case {
                    "stall_with_cpu" => run_control::STALLED_NO_PROGRESS,
                    "missing" => "pid does not answer; zellij session absent; capture has not grown; no recent hook",
                    "exit" => "process exited with status 137",
                    "unknown" => "cancelled by owner",
                    _ => "stalled: alive but capture, hooks and commits show no progress beyond the window; waiting_on empty",
                }.into();
                if case == "wrong_head" { run.last_commit = "d".repeat(40); }
                if case == "changed_identity" { run.project = "OTHER".into(); }
                if case == "successor" { run.next_run_id = Some("next-review".into()); }
                if case == "admission" { run.admission_refused = true; }
            }).unwrap();
            if case == "changed_identity" {
                run_control::update_in(&registry,&child.run_id,|run| run.project = child.project.clone()).unwrap();
            }
            // A later fetch cannot retroactively bind a different failure head.
            if ["wrong_head", "before_publication"].contains(&case) { record_publication_in(&registry,&child,&result).unwrap(); }
            let journal = registry.join(format!("{}.events.jsonl",child.run_id));
            if case == "exit_receipt" { std::fs::write(registry.join(format!("{}.exit",child.run_id)),"137").unwrap(); }
            if case == "no_journal" { std::fs::remove_file(&journal).unwrap(); }
            let original = std::fs::read(&journal).unwrap_or_default();
            let before = run_control::load_manifest_in(&registry,&child.run_id).unwrap();
            let mut receipt = child.clone();
            if case == "uncollected" { receipt.state = "running".into(); }
            let mut bound = parent.clone();
            if case == "wrong_parent" { bound.quality.as_mut().unwrap().child = Some("new-review".into()); }
            let mut evidence = handback.clone();
            if case == "wrong_handback" { evidence.run_id = Some("foreign-review".into()); }
            let mut publication = result.clone();
            if case == "dirty" { publication.uncommitted_source = true; }
            assert!(accept_reviewer_handback_in(&registry,&receipt,&bound,&publication,None).is_err());
            let recovered = accept_reviewer_handback_in(&registry,&receipt,&bound,&publication,Some(&evidence));
            let after = run_control::load_manifest_in(&registry,&child.run_id).unwrap();
            let sessions = vec![("interactive-reviewer".into(),"reviewer".into(),Some("exe-dev".into()))];
            if ["stall", "stall_with_cpu", "missing"].contains(&case) {
                assert!(recovered.unwrap(),"{case}");
                assert_eq!(after.state,RunState::Done,"{case}");
                assert_eq!(after.pid,Some(123),"completion is not a process-exit assertion");
                assert!(after.last_signal.contains("historical liveness failure"));
                assert!(std::fs::read(&journal).unwrap().starts_with(&original),"failure history must survive");
                assert_eq!(run_control::live_viewport_count_in(&registry,&sessions).unwrap(),0);
                assert!(!accept_reviewer_handback_in(&registry,&receipt,&bound,&publication,Some(&evidence)).unwrap());
                assert_eq!(run_control::load_manifest_in(&registry,&child.run_id).unwrap().revision,after.revision);
            } else {
                assert!(!recovered.unwrap_or(false),"{case}");
                assert_eq!(after,before,"{case}: ambiguous failure stays unchanged");
                assert_eq!(std::fs::read(&journal).unwrap_or_default(),original,"{case}");
            }
            assert_eq!(serde_json::to_value(&parent).unwrap(),parent_before);
            std::fs::remove_dir_all(registry).unwrap();
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
            review_parent: None,
            repair_parent: None,
            quality: None,
            run_id: "fixture".into(),
            project: "TEST".into(),
            ticket: Some("TEST-1".into()),
            handle: "builder".into(),
            local_path: worker.to_string_lossy().into(),
            local_branch: "task".into(),
            remote: remote.to_string_lossy().into(),
            desktop_remote: None,
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
        std::fs::write(&result, serde_json::json!({"run_id":"fixture", "source_sha":source, "exit_code":0, "uncommitted_source":false, "published_head":"fake-worker-claim"}).to_string()).unwrap();
        commit();
        let (published, _) = fetch_result_in(&cache, &t).unwrap().unwrap();
        assert_eq!(
            published.published_head,
            git(&worker, &["rev-parse", "HEAD"]).unwrap()
        );
        let registry = root.join("registry");
        let mut native = crate::run_control::RunManifest::requested(
            &t.handle,
            "fixture",
            &t.local_path,
            t.ticket.clone(),
            None,
            &[],
            1,
        );
        native.run_id = t.run_id.clone();
        native.project = t.project.clone();
        let native = crate::run_control::request_in(&registry, native, || Ok(())).unwrap();
        crate::run_control::update_in(&registry, &native.run_id, |run| {
            run.state = crate::run_control::RunState::Failed;
            run.last_commit = source.clone();
        })
        .unwrap();
        record_publication_in(&registry, &t, &published).unwrap();
        let after = crate::run_control::load_manifest_in(&registry, &t.run_id).unwrap();
        assert_eq!(after.last_commit, published.published_head);
        assert_eq!(
            after.state,
            crate::run_control::RunState::Failed,
            "publication must never imply success"
        );
        let history =
            std::fs::read_to_string(registry.join(format!("{}.events.jsonl", t.run_id))).unwrap();
        assert!(
            history.contains(&source),
            "the earlier observed revision remains in durable history"
        );
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
