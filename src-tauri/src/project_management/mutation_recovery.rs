//! XNAUT-473: exact app-owned interrupted commit recovery, not repository repair.
//! Never removes Git locks, rewrites pending bytes, stashes, resets or skips hooks.
use super::*;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
const ATTEMPTS: usize = 4;
const WAIT_MS: u64 = 120;
#[derive(Clone, Serialize, Deserialize)]
struct ExpectedPath {
    path: String,
    sha256: Option<String>,
    mode: String,
    // Retain interrupted content for inspection; never use it to overwrite a
    // changed checkout. Successful receipts retain hashes and the commit only.
    content_base64: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Intent {
    version: u32,
    id: String,
    created_at: String,
    worktree: PathBuf,
    git_dir: PathBuf,
    branch: String,
    base: String,
    projects: Vec<String>,
    subject: String,
    event_type: String,
    event_path: String,
    message: String,
    paths: Vec<ExpectedPath>,
    attempts: u64,
    original_index: Vec<Vec<u8>>,
    intended_index: Vec<Vec<u8>>,
    last_outcome: String,
    last_failure: Option<String>,
    committed: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MutationStatus {
    pub id: String,
    pub subject: String,
    pub branch: String,
    pub state: String,
    pub reason: String,
    pub next_action: String,
    pub paths: Vec<String>,
    pub commit: Option<String>,
    pub attempts: u64,
}
#[derive(Debug, Serialize)]
pub struct Diagnosis {
    pub branch: String,
    pub index_lock: String,
    pub automatic_write_hold: Option<String>,
    pub mutations: Vec<MutationStatus>,
    pub legacy_unattributed_paths: Vec<String>,
    pub warnings: Vec<String>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn root(repo: &Path) -> Result<PathBuf, String> {
    Ok(common_dir(repo)?.join("xnaut-pm-state/mutations"))
}
fn receipt_path(repo: &Path, id: &str, done: bool) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "invalid native mutation ID")?;
    Ok(root(repo)?
        .join(if done { "completed" } else { "pending" })
        .join(format!("{id}.json")))
}
fn persist(repo: &Path, intent: &Intent, done: bool) -> Result<(), String> {
    use std::io::Write;
    let path = receipt_path(repo, &intent.id, done)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let temp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(intent).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(&temp, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
fn branch(repo: &Path) -> Result<String, String> {
    run_git(repo, &["symbolic-ref", "--short", "HEAD"])
}
fn git_dir(repo: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(run_git(repo, &["rev-parse", "--absolute-git-dir"])?)
        .map_err(|e| e.to_string())
}
fn index_lock(repo: &Path) -> Result<PathBuf, String> {
    Ok(git_dir(repo)?.join("index.lock"))
}
fn safe_relative(repo: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(repo)
        .map_err(|_| "mutation path escaped repository")?;
    if relative
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
        || relative.as_os_str().is_empty()
    {
        return Err("mutation paths must be exact relative files".into());
    }
    let text = relative.to_str().ok_or("mutation path is not UTF-8")?;
    if text.starts_with(".git/") || text == ".git" {
        return Err("Git metadata is not a mutation path".into());
    }
    let mut current = repo.to_path_buf();
    for part in relative.components() {
        current.push(part);
        match current.symlink_metadata() {
            Ok(m) if m.file_type().is_symlink() => {
                return Err("mutation path contains a symlink".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(text.replace('\\', "/"))
}
fn head(repo: &Path) -> Result<String, String> {
    let out = git_command(repo)
        .args(["rev-parse", "--verify", "--quiet", "HEAD"])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().into());
    }
    if out.status.code() == Some(1) && branch(repo).is_ok() {
        return Ok(String::new());
    }
    Err("Cannot read control repository HEAD".into())
}
fn target_index(repo: &Path, paths: &[ExpectedPath]) -> Result<Vec<Vec<u8>>, String> {
    let wanted: BTreeSet<_> = paths.iter().map(|p| p.path.as_bytes()).collect();
    Ok(raw_git(repo, &["ls-files", "--stage", "-z"])?
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .filter(|row| {
            row.iter()
                .position(|b| *b == b'\t')
                .is_some_and(|tab| wanted.contains(&row[tab + 1..]))
        })
        .map(Vec::from)
        .collect())
}
fn intended_index(repo: &Path, paths: &[ExpectedPath]) -> Result<Vec<Vec<u8>>, String> {
    use std::io::Write;
    let mut rows = vec![];
    for path in paths {
        if path.sha256.is_none() {
            continue;
        }
        let bytes = std::fs::read(repo.join(&path.path)).map_err(|e| e.to_string())?;
        let mut child = git_command(repo)
            .args(["hash-object", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        child
            .stdin
            .take()
            .ok_or("Git stdin unavailable")?
            .write_all(&bytes)
            .map_err(|e| e.to_string())?;
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("Cannot identify intended Git blob".into());
        }
        let object = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        rows.push(format!("{} {} 0\t{}", path.mode, object, path.path).into_bytes());
    }
    Ok(rows)
}
fn expected(repo: &Path, path: &Path) -> Result<ExpectedPath, String> {
    let relative = safe_relative(repo, path)?;
    let meta = match path.symlink_metadata() {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ExpectedPath {
                path: relative,
                sha256: None,
                mode: String::new(),
                content_base64: None,
            })
        }
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() {
        return Err("mutation path is not a regular file".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = false;
    let mut mode = if executable { "100755" } else { "100644" }.to_owned();
    if run_git(repo, &["config", "--bool", "core.filemode"]).is_ok_and(|v| v == "false") {
        if let Ok(entry) = run_git(repo, &["ls-files", "--stage", "--", &relative]) {
            if entry.starts_with("100755 ") {
                mode = "100755".into();
            } else {
                mode = "100644".into();
            }
        }
    }
    Ok(ExpectedPath {
        path: relative,
        sha256: Some(digest(&bytes)),
        mode,
        content_base64: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
    })
}
fn pending(repo: &Path) -> Result<Vec<(PathBuf, Result<Intent, String>)>, String> {
    let dir = root(repo)?.join("pending");
    let files = match std::fs::read_dir(dir) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let mut rows = Vec::new();
    for file in files {
        let path = file.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|s| s == "json") {
            rows.push((path.clone(), read_json(&path)));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows)
}
pub(super) fn ensure_no_overlap(repo: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let wanted: BTreeSet<_> = paths
        .iter()
        .map(|p| safe_relative(repo, p))
        .collect::<Result<_, _>>()?;
    let current = branch(repo)?;
    for (_, record) in pending(repo)? {
        let intent = record
            .map_err(|_| "unreadable native mutation receipt; diagnose before another write")?;
        if intent.branch == current && intent.paths.iter().any(|p| wanted.contains(&p.path)) {
            return Err(format!("native PM mutation {} is interrupted on these paths; diagnose and recover this receipt before retrying the logical edit",intent.id));
        }
    }
    Ok(())
}
pub(super) fn commit(
    repo: &Path,
    id: &str,
    event_type: &str,
    subject: &str,
    event_path: &Path,
    paths: &[PathBuf],
    message: &str,
) -> Result<(), String> {
    let mut expected_paths = paths
        .iter()
        .map(|p| expected(repo, p))
        .collect::<Result<Vec<_>, _>>()?;
    expected_paths.sort_by(|a, b| a.path.cmp(&b.path));
    if expected_paths.windows(2).any(|p| p[0].path == p[1].path) {
        return Err("duplicate mutation path".into());
    }
    let projects: BTreeSet<_> = expected_paths
        .iter()
        .filter_map(|p| {
            p.path
                .strip_prefix("projects/")
                .and_then(|v| v.split('/').next())
                .map(str::to_owned)
        })
        .collect();
    let mut intent = Intent {
        version: 1,
        id: id.into(),
        created_at: chrono::Utc::now().to_rfc3339(),
        worktree: std::fs::canonicalize(repo).map_err(|e| e.to_string())?,
        git_dir: git_dir(repo)?,
        branch: branch(repo)?,
        base: head(repo)?,
        projects: projects.into_iter().collect(),
        subject: subject.into(),
        event_type: event_type.into(),
        event_path: safe_relative(repo, event_path)?,
        message: message.into(),
        original_index: target_index(repo, &expected_paths)?,
        intended_index: intended_index(repo, &expected_paths)?,
        paths: expected_paths,
        attempts: 0,
        last_outcome: "prepared".into(),
        last_failure: None,
        committed: None,
    };
    persist(repo, &intent, false)?;
    let status = resume(repo, &mut intent, true)?;
    if status.state == "committed" || status.state == "already_committed" {
        Ok(())
    } else {
        Err(format!(
            "native PM mutation {} retained: {} ({}). Call pm_mutation_diagnose for the project, then pm_mutation_recover with this native mutation_id when allowed. Reload the ticket afterward to check whether the original edit is already applied. Do not recreate the ticket or blindly repeat this write; no worker was launched by this PM mutation.",
            intent.id, status.reason, status.state
        ))
    }
}
fn status(intent: &Intent, state: &str, reason: impl Into<String>) -> MutationStatus {
    MutationStatus {
        id: intent.id.clone(),
        subject: intent.subject.clone(),
        branch: intent.branch.clone(),
        state: state.into(),
        reason: reason.into(),
        next_action: match state {
            "committed" | "already_committed" => "Reload the ticket and reconcile the original request against saved content. Do not replay an already applied edit or infer a worker launch.",
            "index_contention" | "ready_to_recover" => "Use pm_mutation_recover with this exact native mutation_id, then reload the ticket. Keep lock files untouched; do not recreate the ticket or blindly repeat the original edit.",
            "paused" => "Keep the native intent; diagnose is available during pause. Recover only after the owner lifts the write hold.",
            "explicit_recovery_required" => "Inspect the retained failure before explicitly retrying this native mutation_id. Automatic retries will not rerun a rejecting hook or ambiguous attempt.",
            _ => "Keep the original receipt and edits. Diagnose the scope/content conflict; do not reset, stash, remove locks, stage unrelated files, or repeat the original logical write.",
        }.into(),
        paths: intent.paths.iter().map(|p| p.path.clone()).collect(),
        commit: intent.committed.clone(),
        attempts: intent.attempts,
    }
}
fn identity(repo: &Path, intent: &Intent) -> Result<(), String> {
    if intent.version != 1 || uuid::Uuid::parse_str(&intent.id).is_err() {
        return Err("unsupported or invalid native mutation receipt".into());
    }
    if std::fs::canonicalize(repo).map_err(|e| e.to_string())? != intent.worktree
        || git_dir(repo)? != intent.git_dir
        || branch(repo)? != intent.branch
    {
        return Err("receipt belongs to a different worktree or branch".into());
    }
    if intent.paths.is_empty() || !intent.paths.iter().any(|p| p.path == intent.event_path) {
        return Err("native event binding is missing".into());
    }
    for path in &intent.paths {
        safe_relative(repo, &repo.join(&path.path))?;
        if intent.committed.is_none() {
            match (&path.sha256, &path.content_base64) {
                (Some(hash), Some(encoded)) => {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .map_err(|_| "native retained content is malformed")?;
                    if digest(&bytes) != *hash {
                        return Err("native retained content/hash mismatch".into());
                    }
                }
                (None, None) => {}
                _ => return Err("native retained content is incomplete".into()),
            }
        }
    }
    Ok(())
}
fn unchanged(repo: &Path, intent: &Intent) -> Result<(), String> {
    if head(repo)? != intent.base {
        return Err(
            "HEAD changed since native intent; preserve and reconcile existing history".into(),
        );
    }
    let index = target_index(repo, &intent.paths)?;
    if index != intent.original_index && index != intent.intended_index {
        return Err("Intent-owned index entries changed; staged edits remain untouched".into());
    }
    for path in &intent.paths {
        let now = expected(repo, &repo.join(&path.path))?;
        if now.sha256 != path.sha256 || now.mode != path.mode {
            return Err(format!(
                "content or mode changed: {}; original native evidence retained",
                path.path
            ));
        }
    }
    Ok(())
}
fn raw_git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = git_command(repo)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(crate::project_wiki::redact(&String::from_utf8_lossy(
            &out.stderr,
        )));
    }
    Ok(out.stdout)
}
fn commit_proof(repo: &Path, intent: &Intent) -> Result<Option<String>, String> {
    if head(repo)?.is_empty() {
        return Ok(None);
    }
    let marker = format!("XNAUT-PM-Mutation: {}", intent.id);
    let commits = run_git(
        repo,
        &[
            "log",
            "-2",
            "--format=%H",
            "--fixed-strings",
            &format!("--grep={marker}"),
            "HEAD",
        ],
    )?;
    let rows: Vec<_> = commits.lines().filter(|s| !s.is_empty()).collect();
    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() != 1 {
        return Err("multiple commits claim this mutation ID; original evidence retained".into());
    }
    let commit = rows[0];
    let message = run_git(repo, &["log", "-1", "--format=%B", commit])?;
    if !message.lines().any(|l| l == marker)
        || run_git(repo, &["rev-parse", &format!("{commit}^@")])? != intent.base
    {
        return Err("mutation commit ancestry or marker mismatch".into());
    }
    let event: Value = serde_json::from_slice(&raw_git(
        repo,
        &["show", &format!("{commit}:{}", intent.event_path)],
    )?)
    .map_err(|_| "committed mutation event is malformed")?;
    if event["pm_mutation_id"] != intent.id
        || event["event"] != intent.event_type
        || event["subject"] != intent.subject
    {
        return Err("committed mutation event identity mismatch".into());
    }
    let allowed: BTreeSet<_> = intent.paths.iter().map(|p| p.path.as_str()).collect();
    let changed = raw_git(
        repo,
        &[
            "diff-tree",
            "--no-commit-id",
            "--root",
            "--name-only",
            "-r",
            "-z",
            commit,
        ],
    )?;
    if changed
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .any(|p| std::str::from_utf8(p).map_or(true, |p| !allowed.contains(p)))
    {
        return Err(
            "commit included an unrelated path; inspect hook activity without resetting history"
                .into(),
        );
    }
    for path in &intent.paths {
        let object = format!("{commit}:{}", path.path);
        if let Some(hash) = &path.sha256 {
            if digest(&raw_git(repo, &["show", &object])?) != *hash {
                return Err(format!(
                    "committed bytes differ from native intent: {}",
                    path.path
                ));
            }
            let tree = run_git(repo, &["ls-tree", commit, "--", &path.path])?;
            if !tree.starts_with(&format!("{} blob ", path.mode)) {
                return Err("committed mode differs from native intent".into());
            }
        } else if run_git(repo, &["cat-file", "-e", &object]).is_ok() {
            return Err("expected deletion is not present in commit".into());
        }
    }
    Ok(Some(commit.into()))
}
fn finish(repo: &Path, intent: &mut Intent, commit: String) -> Result<(), String> {
    intent.committed = Some(commit);
    intent.last_failure = None;
    intent.last_outcome = "committed".into();
    for path in &mut intent.paths {
        path.content_base64 = None;
    }
    persist(repo, intent, true)?;
    match std::fs::remove_file(receipt_path(repo, &intent.id, false)?) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    Ok(())
}
fn unrelated_index(repo: &Path, intent: &Intent) -> Result<Vec<Vec<u8>>, String> {
    let allowed: BTreeSet<_> = intent.paths.iter().map(|p| p.path.as_bytes()).collect();
    Ok(raw_git(repo, &["ls-files", "--stage", "-z"])?
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
        .filter(|row| {
            row.iter()
                .position(|b| *b == b'\t')
                .is_none_or(|tab| !allowed.contains(&row[tab + 1..]))
        })
        .map(Vec::from)
        .collect())
}
fn reported_lock(line: &str) -> Option<&str> {
    line.strip_prefix("fatal: Unable to create '")?
        .strip_suffix("': File exists.")
}
fn same_index_lock(repo: &Path, reported: &str) -> Result<bool, String> {
    let path = Path::new(reported);
    if !path.is_absolute() || path.file_name().and_then(|s| s.to_str()) != Some("index.lock") {
        return Ok(false);
    }
    // Canonicalize the existing parent, not the lock: its holder may already
    // have released it. This handles Windows Git C:/ vs \\?\ paths and macOS
    // /var symlinks without treating an unrelated lock or error as retryable.
    let Some(parent) = path.parent() else {
        return Ok(false);
    };
    let Ok(parent) = std::fs::canonicalize(parent) else {
        return Ok(false);
    };
    Ok(parent == git_dir(repo)?)
}
fn index_contention(repo: &Path, out: &std::process::Output) -> Result<bool, String> {
    if out.status.code() != Some(128) {
        return Ok(false);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let Some(path) = stderr.lines().next().and_then(reported_lock) else {
        return Ok(false);
    };
    same_index_lock(repo, path)
}
fn resume(repo: &Path, intent: &mut Intent, initial: bool) -> Result<MutationStatus, String> {
    if let Err(reason) = identity(repo, intent) {
        return Ok(status(intent, "scope_mismatch", reason));
    }
    match commit_proof(repo, intent) {
        Ok(Some(commit)) => {
            finish(repo, intent, commit)?;
            return Ok(status(
                intent,
                "already_committed",
                "Exact committed event and file tree verified; no new commit",
            ));
        }
        Err(reason) => return Ok(status(intent, "proof_mismatch", reason)),
        Ok(None) => {}
    }
    if let Err(reason) = unchanged(repo, intent) {
        return Ok(status(intent, "content_changed", reason));
    }
    let baseline = unrelated_index(repo, intent)?;
    let message = format!("{}\n\nXNAUT-PM-Mutation: {}", intent.message, intent.id);
    let paths: Vec<String> = intent.paths.iter().map(|p| p.path.clone()).collect();
    for attempt in 0..ATTEMPTS {
        if !initial {
            if let Some(reason) = crate::switches::automatic_pm_write_hold_strict() {
                return Ok(status(intent, "paused", reason));
            }
        }
        if let Err(reason) = unchanged(repo, intent) {
            return Ok(status(intent, "content_changed", reason));
        }
        if unrelated_index(repo, intent)? != baseline {
            return Ok(status(
                intent,
                "index_changed",
                "Unrelated index entries changed during recovery; no further attempt",
            ));
        }
        intent.attempts += 1;
        intent.last_outcome = "attempting".into();
        persist(repo, intent, false)?;
        let mut add = vec!["add", "--"];
        add.extend(paths.iter().map(String::as_str));
        let staged = git_command(repo)
            .args(&add)
            .output()
            .map_err(|e| e.to_string())?;
        let output = if staged.status.success() {
            let mut commit = vec![
                "-c",
                "user.name=xNaut",
                "-c",
                "user.email=xnaut@local",
                "commit",
                "--only",
                "-m",
                &message,
                "--",
            ];
            commit.extend(paths.iter().map(String::as_str));
            git_command(repo)
                .args(&commit)
                .output()
                .map_err(|e| e.to_string())?
        } else {
            staged
        };
        let proof = match commit_proof(repo, intent) {
            Ok(proof) => proof,
            Err(reason) => {
                intent.last_failure = Some(reason.clone());
                intent.last_outcome = "proof_mismatch".into();
                persist(repo, intent, false)?;
                return Ok(status(intent, "proof_mismatch", reason));
            }
        };
        if let Some(commit) = proof {
            if unrelated_index(repo, intent)? != baseline {
                return Ok(status(
                    intent,
                    "index_changed",
                    "Commit exists but unrelated index entries changed; inspect hook activity",
                ));
            }
            finish(repo, intent, commit)?;
            return Ok(status(
                intent,
                "committed",
                "Exact native mutation committed; unrelated index entries preserved",
            ));
        }
        let contention = index_contention(repo, &output)?;
        let reason = if contention {
            "Git index lock contention; holder is unknown and no lock was removed".into()
        } else if output.status.success() {
            "Git returned success without an exact native mutation commit; inspect retained evidence".into()
        } else {
            crate::project_wiki::redact(&String::from_utf8_lossy(&output.stderr))
                .chars()
                .take(2000)
                .collect::<String>()
        };
        intent.last_outcome = if contention {
            "index_contention"
        } else {
            "git_refused"
        }
        .into();
        intent.last_failure = Some(reason.clone());
        persist(repo, intent, false)?;
        if !contention {
            return Ok(status(intent, "git_refused", reason));
        }
        if attempt + 1 < ATTEMPTS {
            std::thread::sleep(std::time::Duration::from_millis(WAIT_MS));
        }
    }
    Ok(status(
        intent,
        "index_contention",
        "Bounded retry exhausted; native intent retained; lock ownership remains unknown",
    ))
}
fn inspect_intent(repo: &Path, intent: &Intent) -> MutationStatus {
    if let Err(reason) = identity(repo, intent) {
        return status(intent, "scope_mismatch", reason);
    }
    match commit_proof(repo, intent) {
        Ok(Some(commit)) => {
            let mut found = intent.clone();
            found.committed = Some(commit);
            return status(
                &found,
                "already_committed",
                "Exact native commit exists; only acknowledgment remains",
            );
        }
        Err(reason) => return status(intent, "proof_mismatch", reason),
        Ok(None) => {}
    }
    if let Err(reason) = unchanged(repo, intent) {
        return status(intent, "content_changed", reason);
    }
    if let Some(reason) = crate::switches::automatic_pm_write_hold_strict() {
        return status(intent, "paused", reason);
    }
    if index_lock(repo).is_ok_and(|p| p.exists()) {
        return status(
            intent,
            "index_contention",
            "Git index lock is present; holder unknown; no stale-lock inference or removal",
        );
    }
    if !matches!(
        intent.last_outcome.as_str(),
        "prepared" | "index_contention"
    ) {
        return status(
            intent,
            "explicit_recovery_required",
            format!(
                "Previous {} outcome requires an explicit recovery request; saved reason: {}",
                intent.last_outcome,
                intent
                    .last_failure
                    .as_deref()
                    .unwrap_or("attempt interrupted before an outcome was recorded")
            ),
        );
    }
    status(intent,"ready_to_recover","Exact app-owned content and branch match; recovery may retry normal Git with hooks enabled")
}
pub fn diagnose(repo: &Path, project: &str) -> Result<Diagnosis, String> {
    let project = validate_project_key(project)?;
    let mut report = Diagnosis {
        branch: branch(repo)?,
        index_lock: if index_lock(repo)?.exists() {
            "present_unknown"
        } else {
            "absent"
        }
        .into(),
        automatic_write_hold: crate::switches::automatic_pm_write_hold_strict(),
        mutations: vec![],
        legacy_unattributed_paths: vec![],
        warnings: vec![],
    };
    let mut owned = BTreeSet::new();
    for (path, row) in pending(repo)? {
        let intent = match row {
            Ok(i) if path.file_stem().and_then(|p| p.to_str()) == Some(i.id.as_str()) => i,
            _ => {
                report.warnings.push(
                    "A native mutation receipt is unreadable or misfiled; it remains untouched"
                        .into(),
                );
                continue;
            }
        };
        if !intent.projects.contains(&project) {
            continue;
        }
        owned.extend(intent.paths.iter().map(|p| p.path.clone()));
        let item = if intent.projects.len() != 1 {
            status(&intent,"scope_mismatch","Multi-project native mutation requires native maintenance recovery; project-specific agent recovery cannot consume it")
        } else {
            inspect_intent(repo, &intent)
        };
        report.mutations.push(item);
    }
    let dirty = raw_git(
        repo,
        &["status", "--porcelain", "-z", "--untracked-files=all"],
    )?;
    let prefix = format!("projects/{project}/");
    let mut entries = dirty.split(|b| *b == 0).filter(|l| !l.is_empty());
    while let Some(line) = entries.next() {
        if line.len() < 4 {
            continue;
        }
        let renamed = line[..2].iter().any(|b| matches!(*b, b'R' | b'C'));
        let mut paths = vec![&line[3..]];
        if renamed {
            if let Some(old) = entries.next() {
                paths.push(old);
            }
        }
        for path in paths {
            if let Ok(path) = std::str::from_utf8(path) {
                if path.starts_with(&prefix) && !owned.contains(path) {
                    report.legacy_unattributed_paths.push(path.into());
                }
            }
        }
    }
    if !report.legacy_unattributed_paths.is_empty() {
        report.warnings.push("Pending project files lack matching native mutation attribution; no recovery will stage or commit them".into());
    }
    Ok(report)
}
pub fn recover(repo: &Path, project: &str, id: &str) -> Result<MutationStatus, String> {
    let project = validate_project_key(project)?;
    let pending = receipt_path(repo, id, false)?;
    let path = if pending.exists() {
        pending
    } else {
        receipt_path(repo, id, true)?
    };
    let mut intent: Intent = read_json(&path).map_err(|_| {
        "native mutation ID is unknown or unreadable; legacy edits remain untouched"
    })?;
    if intent.id != id || intent.projects != vec![project] {
        return Ok(status(
            &intent,
            "scope_mismatch",
            "Receipt ID/project differs or spans multiple projects; no mutation attempted",
        ));
    }
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "PM mutation lock unavailable")?;
    let _lease = match ControlWriteLease::acquire(repo) {
        Ok(l) => l,
        Err(reason) => {
            return Ok(status(
                &intent,
                if crate::switches::automatic_pm_write_hold_strict().is_some() {
                    "paused"
                } else {
                    "repository_busy"
                },
                reason,
            ))
        }
    };
    // Never inherit an owner-action exception for recovery of background work.
    if let Some(reason) = crate::switches::automatic_pm_write_hold_strict() {
        return Ok(status(&intent, "paused", reason));
    }
    let result = resume(repo, &mut intent, false)?;
    if matches!(result.state.as_str(), "committed" | "already_committed") {
        publish(repo);
    }
    Ok(result)
}
/// Existing callers already hold the PM thread mutex when updating a ticket.
/// The OS lease is reentrant on that same thread; do not take the mutex again.
pub(super) fn recover_automatic(
    repo: &Path,
    subject: Option<&str>,
) -> Result<Vec<MutationStatus>, String> {
    if let Some(reason) = crate::switches::automatic_pm_write_hold_strict() {
        return Err(reason);
    }
    let _lease = ControlWriteLease::acquire(repo)?;
    let current = branch(repo)?;
    let worktree = std::fs::canonicalize(repo).map_err(|e| e.to_string())?;
    let mut results = vec![];
    for (path, row) in pending(repo)? {
        let mut intent =
            row.map_err(|_| "native mutation receipt is unreadable; diagnosis required")?;
        if path.file_stem().and_then(|p| p.to_str()) != Some(intent.id.as_str()) {
            return Err("native mutation filename/identity mismatch".into());
        }
        if intent.branch != current
            || intent.worktree != worktree
            || subject.is_some_and(|s| intent.subject != s)
        {
            continue;
        }
        if results.len() >= 16 {
            break;
        }
        let result = if !matches!(
            intent.last_outcome.as_str(),
            "prepared" | "index_contention"
        ) && commit_proof(repo, &intent)?.is_none()
        {
            status(&intent,"explicit_recovery_required",format!("Previous outcome {} is not proven transient contention; automatic maintenance will not repeat hooks or an ambiguous attempt",intent.last_outcome))
        } else {
            resume(repo, &mut intent, false)?
        };
        if matches!(result.state.as_str(), "committed" | "already_committed") {
            publish(repo);
        }
        results.push(result);
    }
    Ok(results)
}
pub(super) fn warning(repo: &Path) -> Result<String, String> {
    let count = pending(repo)?.len();
    Ok(if count == 0 {
        String::new()
    } else {
        format!("{count} interrupted native PM mutation(s) retained. Use PM mutation diagnosis/recovery; unknown locks and changed or unattributed files are never removed or committed automatically.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        repo: PathBuf,
        _switches: crate::switches::TestScope,
    }
    impl Fixture {
        fn new() -> Self {
            let repo = std::env::temp_dir().join(format!("xnaut-pm473-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(repo.join("projects/TEST/tickets")).unwrap();
            std::fs::create_dir_all(repo.join("events")).unwrap();
            let f = Self {
                repo,
                _switches: crate::switches::TestScope::unpaused("pm473"),
            };
            for args in [
                vec!["init", "-b", "main"],
                vec!["config", "user.name", "fixture"],
                vec!["config", "user.email", "fixture@local"],
                vec!["config", "commit.gpgsign", "false"],
                vec!["config", "core.autocrlf", "false"],
                vec!["config", "core.hooksPath", ".git/hooks"],
            ] {
                f.git(&args);
            }
            std::fs::write(
                f.repo.join("projects/TEST/tickets/TEST-1.json"),
                b"original\n",
            )
            .unwrap();
            std::fs::write(f.repo.join("unrelated"), b"original unrelated\n").unwrap();
            f.git(&["add", "."]);
            f.git(&["commit", "-m", "baseline"]);
            f
        }
        fn git(&self, args: &[&str]) -> String {
            run_git(&self.repo, args).unwrap()
        }
        fn target(&self) -> PathBuf {
            self.repo.join("projects/TEST/tickets/TEST-1.json")
        }
        fn write(&self) -> Result<(), String> {
            std::fs::write(self.target(), b"native intended\n").unwrap();
            record_mutation(
                &self.repo,
                "ticket_update",
                "TEST-1",
                json!({"revision":2}),
                &[self.target()],
                "native fixture update",
            )
        }
        fn only_pending(&self) -> Intent {
            let rows = pending(&self.repo).unwrap();
            assert_eq!(rows.len(), 1);
            read_json(&rows[0].0).unwrap()
        }
        fn own_unknown_lock(&self) -> PathBuf {
            let path = index_lock(&self.repo).unwrap();
            std::fs::write(&path, b"fixture owns this lock; product must not remove it").unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.repo);
        }
    }

    #[test]
    fn contention_classification_requires_exact_git_error_and_same_real_index_parent() {
        let f = Fixture::new();
        let actual = index_lock(&f.repo).unwrap();
        assert!(same_index_lock(&f.repo, &actual.to_string_lossy()).unwrap());
        let equivalent = f.repo.join(".git/objects/../index.lock");
        assert!(same_index_lock(&f.repo, &equivalent.to_string_lossy()).unwrap());
        let other = Fixture::new();
        assert!(
            !same_index_lock(&f.repo, &index_lock(&other.repo).unwrap().to_string_lossy()).unwrap()
        );
        assert!(
            !same_index_lock(&f.repo, &f.repo.join(".git/HEAD.lock").to_string_lossy()).unwrap()
        );
        assert!(!same_index_lock(&f.repo, ".git/index.lock").unwrap());
        assert_eq!(
            reported_lock("fatal: Unable to create 'C:/fixture/.git/index.lock': File exists."),
            Some("C:/fixture/.git/index.lock")
        );
        assert!(reported_lock(
            "fatal: Unable to create '/fixture/.git/index.lock': Permission denied"
        )
        .is_none());
        assert!(reported_lock(
            "hook says fatal: Unable to create '/fixture/.git/index.lock': File exists."
        )
        .is_none());
        #[cfg(windows)]
        {
            let canonical = actual.to_string_lossy();
            let git_style = canonical
                .strip_prefix(r"\\?\")
                .unwrap_or(&canonical)
                .replace('\\', "/");
            assert!(same_index_lock(&f.repo, &git_style).unwrap());
        }
    }

    #[test]
    fn held_unknown_lock_retains_exact_intent_then_recovers_once_without_touching_unrelated_index()
    {
        let f = Fixture::new();
        std::fs::write(f.repo.join("unrelated"), b"staged unrelated\n").unwrap();
        f.git(&["add", "unrelated"]);
        std::fs::write(f.repo.join("unrelated"), b"unstaged unrelated\n").unwrap();
        let before = f.git(&["rev-parse", "HEAD"]);
        let staged = f.git(&["show", ":unrelated"]);
        let lock = f.own_unknown_lock();
        let bytes = std::fs::read(&lock).unwrap();
        let error = f.write().unwrap_err();
        assert!(error.contains("index_contention"), "{error}");
        let original = f.only_pending();
        assert_eq!(original.attempts, ATTEMPTS as u64);
        let d = diagnose(&f.repo, "TEST").unwrap();
        assert_eq!(d.index_lock, "present_unknown");
        assert_eq!(d.mutations.len(), 1);
        assert!(d.legacy_unattributed_paths.is_empty());
        assert_eq!(
            recover(&f.repo, "TEST", &original.id).unwrap().state,
            "index_contention"
        );
        assert_eq!(std::fs::read(&lock).unwrap(), bytes);
        assert_eq!(f.git(&["rev-parse", "HEAD"]), before);
        std::fs::remove_file(lock).unwrap(); // Only the fixture releases its own lock.
        assert_eq!(
            recover(&f.repo, "TEST", &original.id).unwrap().state,
            "committed"
        );
        let committed = f.git(&["rev-parse", "HEAD"]);
        assert_eq!(
            recover(&f.repo, "TEST", &original.id).unwrap().state,
            "already_committed"
        );
        assert_eq!(f.git(&["rev-parse", "HEAD"]), committed);
        assert_eq!(f.git(&["show", ":unrelated"]), staged);
        assert_eq!(
            std::fs::read(f.repo.join("unrelated")).unwrap(),
            b"unstaged unrelated\n"
        );
        assert_eq!(std::fs::read_dir(f.repo.join("events")).unwrap().count(), 1);
        // Simulate a crash after Git commit, before the pending receipt was acknowledged.
        persist(&f.repo, &original, false).unwrap();
        assert_eq!(
            recover(&f.repo, "TEST", &original.id).unwrap().state,
            "already_committed"
        );
        assert_eq!(f.git(&["rev-parse", "HEAD"]), committed);
        assert!(pending(&f.repo).unwrap().is_empty());
    }
    #[test]
    fn changed_worktree_or_staged_only_content_refuses_recovery() {
        let f = Fixture::new();
        let lock = f.own_unknown_lock();
        f.write().unwrap_err();
        let intent = f.only_pending();
        std::fs::remove_file(lock).unwrap();
        std::fs::write(f.target(), b"owner worktree edit\n").unwrap();
        assert_eq!(
            recover(&f.repo, "TEST", &intent.id).unwrap().state,
            "content_changed"
        );
        f.git(&["add", "projects/TEST/tickets/TEST-1.json"]);
        std::fs::write(f.target(), b"native intended\n").unwrap();
        let staged = f.git(&["show", ":projects/TEST/tickets/TEST-1.json"]);
        let result = recover(&f.repo, "TEST", &intent.id).unwrap();
        assert_eq!(result.state, "content_changed");
        assert!(result.reason.contains("index entries"));
        assert_eq!(
            f.git(&["show", ":projects/TEST/tickets/TEST-1.json"]),
            staged
        );
        assert_eq!(pending(&f.repo).unwrap().len(), 1);
    }
    #[test]
    fn branch_scope_pause_and_unattributed_edits_are_held() {
        let f = Fixture::new();
        let lock = f.own_unknown_lock();
        f.write().unwrap_err();
        let intent = f.only_pending();
        std::fs::remove_file(lock).unwrap();
        f.git(&["checkout", "-b", "other"]);
        assert_eq!(
            recover(&f.repo, "TEST", &intent.id).unwrap().state,
            "scope_mismatch"
        );
        f.git(&["checkout", "main"]);
        assert_eq!(
            recover(&f.repo, "OTHER", &intent.id).unwrap().state,
            "scope_mismatch"
        );
        let mut switches = crate::switches::load();
        switches.read_only = true;
        crate::switches::kill_switches_set(switches).unwrap();
        assert!(diagnose(&f.repo, "TEST")
            .unwrap()
            .automatic_write_hold
            .is_some());
        assert_eq!(
            recover(&f.repo, "TEST", &intent.id).unwrap().state,
            "paused"
        );
        assert!(recover(&f.repo, "TEST", &uuid::Uuid::new_v4().to_string())
            .unwrap_err()
            .contains("unknown"));
        std::fs::write(
            f.repo.join("projects/TEST/tickets/TEST-legacy.json"),
            b"unattributed",
        )
        .unwrap();
        assert!(diagnose(&f.repo, "TEST")
            .unwrap()
            .legacy_unattributed_paths
            .iter()
            .any(|p| p.ends_with("TEST-legacy.json")));
    }
    #[test]
    fn mismatched_committed_marker_never_acknowledges_retained_intent() {
        let f = Fixture::new();
        let lock = f.own_unknown_lock();
        f.write().unwrap_err();
        let intent = f.only_pending();
        std::fs::remove_file(lock).unwrap();
        let event = f.repo.join(&intent.event_path);
        let mut altered: Value = read_json(&event).unwrap();
        altered["subject"] = "foreign".into();
        write_json_atomic(&event, &altered).unwrap();
        f.git(&[
            "add",
            "projects/TEST/tickets/TEST-1.json",
            &intent.event_path,
        ]);
        f.git(&[
            "commit",
            "-m",
            &format!("forged marker\n\nXNAUT-PM-Mutation: {}", intent.id),
        ]);
        let before = f.git(&["rev-parse", "HEAD"]);
        assert_eq!(
            recover(&f.repo, "TEST", &intent.id).unwrap().state,
            "proof_mismatch"
        );
        assert_eq!(f.git(&["rev-parse", "HEAD"]), before);
        assert_eq!(pending(&f.repo).unwrap().len(), 1);
    }
    #[test]
    fn automatic_recovery_reloads_saved_contention_without_repeating_logical_edit() {
        let f = Fixture::new();
        let lock = f.own_unknown_lock();
        f.write().unwrap_err();
        let intent = f.only_pending();
        std::fs::remove_file(lock).unwrap();
        let result = recover_automatic(&f.repo, Some("TEST-1")).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].state, "committed");
        assert_eq!(result[0].id, intent.id);
        assert!(recover_automatic(&f.repo, None).unwrap().is_empty());
        assert_eq!(f.git(&["rev-list", "--count", "HEAD"]), "2");
        assert_eq!(std::fs::read_dir(f.repo.join("events")).unwrap().count(), 1);
    }

    #[test]
    fn initial_and_multi_project_normal_commits_remain_supported() {
        let f = Fixture::new();
        std::fs::create_dir_all(f.repo.join("projects/OTHER")).unwrap();
        let second = f.repo.join("projects/OTHER/project.json");
        std::fs::write(&second, b"{}\n").unwrap();
        std::fs::write(f.target(), b"both projects\n").unwrap();
        record_mutation(
            &f.repo,
            "project_reconcile",
            "all",
            json!({}),
            &[f.target(), second],
            "reconcile two projects",
        )
        .unwrap();
        assert!(pending(&f.repo).unwrap().is_empty());
        // Existing PM fixtures and first-time repositories can have an unborn HEAD.
        let fresh = f.repo.join("fresh");
        std::fs::create_dir_all(fresh.join("events")).unwrap();
        run_git(&fresh, &["init", "-b", "main"]).unwrap();
        run_git(&fresh, &["config", "commit.gpgsign", "false"]).unwrap();
        let path = fresh.join("projects/TEST/project.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{}\n").unwrap();
        record_mutation(
            &fresh,
            "project_create",
            "TEST",
            json!({}),
            &[path],
            "first project",
        )
        .unwrap();
        assert!(!head(&fresh).unwrap().is_empty());
    }

    #[cfg(unix)]
    fn hook(f: &Fixture, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = f.repo.join(".git/hooks/pre-commit");
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn rejecting_hook_is_retained_and_not_automatically_repeated() {
        let f = Fixture::new();
        hook(
            &f,
            "#!/bin/sh\necho invoked >> .git/hook-invocations\nexit 1\n",
        );
        assert!(f.write().unwrap_err().contains("git_refused"));
        let intent = f.only_pending();
        assert_eq!(intent.attempts, 1);
        let results = recover_automatic(&f.repo, None).unwrap();
        assert_eq!(results[0].state, "explicit_recovery_required");
        assert_eq!(
            std::fs::read_to_string(f.repo.join(".git/hook-invocations"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert_eq!(f.only_pending().attempts, 1);
        assert_eq!(
            recover(&f.repo, "TEST", &intent.id).unwrap().state,
            "git_refused"
        );
        assert_eq!(
            std::fs::read_to_string(f.repo.join(".git/hook-invocations"))
                .unwrap()
                .lines()
                .count(),
            2
        );
    }
    #[cfg(unix)]
    struct GitHolder {
        child: std::process::Child,
        release: PathBuf,
    }
    #[cfg(unix)]
    impl Drop for GitHolder {
        fn drop(&mut self) {
            // Release only this fixture's hook, including assertion unwinds.
            // Git itself removes its lock; the test never unlinks index.lock.
            let _ = std::fs::write(&self.release, b"release fixture holder");
            let _ = self.child.wait();
        }
    }

    #[cfg(unix)]
    #[test]
    fn real_git_holder_release_allows_bounded_retry_with_normal_hooks() {
        let f = Fixture::new();
        hook(&f,"#!/bin/sh\nif [ -n \"$XN473_HOLDER\" ]; then\n touch .git/holder-ready\n while [ ! -e .git/release-holder ]; do sleep 0.01; done\n exit 1\nfi\necho recovery >> .git/hook-invocations\nexit 0\n");
        // Ordinary/empty commits release index.lock before invoking hooks.
        // A real partial commit retains it while constructing its temporary
        // index, matching the observed native contention path.
        std::fs::write(f.repo.join("holder-only"), b"holder staged content\n").unwrap();
        f.git(&["add", "holder-only"]);
        let child = git_command(&f.repo)
            .args(["commit", "--only", "-m", "holder", "--", "holder-only"])
            .env("XN473_HOLDER", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut holder = GitHolder {
            child,
            release: f.repo.join(".git/release-holder"),
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !f.repo.join(".git/holder-ready").exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(f.repo.join(".git/holder-ready").exists());
        assert!(index_lock(&f.repo).unwrap().exists());
        let repo = f.repo.clone();
        let release = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            let mut observed_contention = false;
            while std::time::Instant::now() < deadline {
                if pending(&repo).unwrap_or_default().iter().any(|(_, r)| {
                    r.as_ref()
                        .is_ok_and(|i| i.last_outcome == "index_contention")
                }) {
                    observed_contention = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::fs::write(repo.join(".git/release-holder"), b"release fixture holder").unwrap();
            observed_contention
        });
        let result = f.write();
        let observed_contention = release.join().unwrap();
        assert!(!holder.child.wait().unwrap().success());
        result.unwrap();
        assert!(observed_contention,"holder must be released only after the native retry path recorded actual index contention");
        assert!(pending(&f.repo).unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(f.repo.join(".git/hook-invocations"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert_eq!(f.git(&["rev-list", "--count", "HEAD"]), "2");
        assert_eq!(f.git(&["show", ":holder-only"]), "holder staged content");
        assert!(run_git(&f.repo, &["cat-file", "-e", "HEAD:holder-only"]).is_err());
    }
}
