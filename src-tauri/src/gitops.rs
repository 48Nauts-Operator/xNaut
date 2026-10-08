// Git plumbing for the v1.6 right-pane Git view: ahead/behind, outgoing files +
// commits, side-by-side diff data, stage/commit/push, and AI commit messages.

use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct AheadBehind {
    pub ahead: u32,
    pub behind: u32,
    pub branch: String,
    pub upstream: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
    /// "A" | "M" | "D" | "R" | "?"
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Worktree {
    pub path: String,
    pub branch: String,
    pub head: String,
    /// Uncommitted files in that worktree, so the panel can show a count.
    pub changes: u32,
    /// True for the worktree the panel is anchored at, so the UI can mark it.
    pub current: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommitMeta {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub author: String,
    pub date: String,
    pub refs: String,
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("failed to invoke git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {} failed: {}", args.join(" "), stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Upstream of the current branch (e.g. "origin/main"), or None when unset.
fn upstream_of(repo: &Path) -> Option<String> {
    run_git(repo, &["rev-parse", "--abbrev-ref", "@{upstream}"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Fallback base when no upstream is configured: the first remote branch.
fn first_remote_branch(repo: &Path) -> Option<String> {
    let out = run_git(repo, &["branch", "-r", "--format=%(refname:short)"]).ok()?;
    out.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.ends_with("/HEAD"))
        .map(|l| l.to_string())
}

// ─── Pure parsers ────────────────────────────────────────────────────────────

/// Parses one `git diff --numstat` line: "12\t3\tsrc/main.rs".
/// Binary files report "-\t-\tfoo.png" → additions/deletions 0.
fn parse_numstat_line(line: &str) -> Option<(String, u32, u32)> {
    let mut parts = line.splitn(3, '\t');
    let add = parts.next()?.trim();
    let del = parts.next()?.trim();
    let path = parts.next()?.trim();
    if path.is_empty() {
        return None;
    }
    let additions = add.parse::<u32>().unwrap_or(0);
    let deletions = del.parse::<u32>().unwrap_or(0);
    Some((path.to_string(), additions, deletions))
}

/// Parses one `git status --porcelain` line into (status, path).
/// Status letter is the first non-space of the two-char code; "??" → "?".
/// Renames ("R  old -> new") yield the new path.
fn parse_porcelain_line(line: &str) -> Option<(String, String)> {
    if line.len() < 4 {
        return None;
    }
    let code = &line[..2];
    let status = if code == "??" {
        "?".to_string()
    } else {
        code.chars().find(|c| !c.is_whitespace())?.to_string()
    };
    let mut path = line[3..].trim().to_string();
    if let Some((_, new)) = path.split_once(" -> ") {
        path = new.trim().to_string();
    }
    Some((status, path))
}

/// Parses one `git diff --name-status` line into (status letter, path).
/// Renames ("R100\told\tnew") yield the new path.
fn parse_name_status_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.split('\t');
    let code = parts.next()?.trim();
    let letter = code.chars().next()?.to_string();
    let path = parts.next_back()?.trim();
    if path.is_empty() {
        return None;
    }
    Some((letter, path.to_string()))
}

/// Parses one tab-separated log line:
/// %H \t %h \t %s \t %an \t %ad \t %D
fn parse_log_line(line: &str) -> Option<CommitMeta> {
    let mut parts = line.splitn(6, '\t');
    Some(CommitMeta {
        sha: parts.next()?.to_string(),
        short_sha: parts.next()?.to_string(),
        subject: parts.next()?.to_string(),
        author: parts.next()?.to_string(),
        date: parts.next()?.to_string(),
        refs: parts.next().unwrap_or("").to_string(),
    })
}

/// Strips a surrounding markdown code fence (``` or ```lang) from an LLM reply.
fn strip_code_fences(reply: &str) -> String {
    let trimmed = reply.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        // Drop the optional language tag on the opening fence line.
        let body = match rest.split_once('\n') {
            Some((_lang, body)) => body,
            None => return String::new(),
        };
        let body = body.trim_end().strip_suffix("```").unwrap_or(body);
        return body.trim().to_string();
    }
    trimmed.to_string()
}

/// Joins numstat counts and status letters on file path.
fn join_changed_files(numstat: &str, statuses: &[(String, String)]) -> Vec<ChangedFile> {
    let counts: Vec<(String, u32, u32)> = numstat.lines().filter_map(parse_numstat_line).collect();
    statuses
        .iter()
        .map(|(status, path)| {
            let (additions, deletions) = counts
                .iter()
                .find(|(p, _, _)| p == path)
                .map(|(_, a, d)| (*a, *d))
                .unwrap_or((0, 0));
            ChangedFile {
                path: path.clone(),
                additions,
                deletions,
                status: status.clone(),
            }
        })
        .collect()
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn git_ahead_behind(repo: String) -> Result<AheadBehind, String> {
    let repo = Path::new(&repo);
    let branch = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    let upstream = upstream_of(repo);

    let (ahead, behind) = match &upstream {
        Some(up) => {
            let range = format!("{up}...HEAD");
            let out = run_git(repo, &["rev-list", "--left-right", "--count", &range])?;
            // Output is "behind<TAB>ahead".
            let mut parts = out.split_whitespace();
            let behind = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let ahead = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            (ahead, behind)
        }
        None => {
            let ahead = run_git(repo, &["rev-list", "--count", "HEAD", "--not", "--remotes"])
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            (ahead, 0)
        }
    };

    Ok(AheadBehind {
        ahead,
        behind,
        branch,
        upstream,
    })
}

#[tauri::command]
pub fn git_outgoing_files(repo: String) -> Result<Vec<ChangedFile>, String> {
    let repo = Path::new(&repo);
    let base = match upstream_of(repo).or_else(|| first_remote_branch(repo)) {
        Some(b) => b,
        None => return Ok(Vec::new()),
    };
    let range = format!("{base}...HEAD");
    let numstat = run_git(repo, &["diff", "--numstat", &range])?;
    let name_status = run_git(repo, &["diff", "--name-status", &range])?;
    let statuses: Vec<(String, String)> = name_status
        .lines()
        .filter_map(parse_name_status_line)
        .collect();
    Ok(join_changed_files(&numstat, &statuses))
}

#[tauri::command]
pub fn git_uncommitted_files(repo: String) -> Result<Vec<ChangedFile>, String> {
    let repo = Path::new(&repo);
    let porcelain = run_git(repo, &["status", "--porcelain"])?;
    // HEAD vs working tree; plain fallback covers unborn branches (no commits yet).
    // Untracked files don't appear in numstat and get 0/0 from the join.
    let numstat = run_git(repo, &["diff", "--numstat", "HEAD"])
        .or_else(|_| run_git(repo, &["diff", "--numstat"]))
        .unwrap_or_default();
    let statuses: Vec<(String, String)> =
        porcelain.lines().filter_map(parse_porcelain_line).collect();
    Ok(join_changed_files(&numstat, &statuses))
}

/// Files changed by commits that explicitly reference a ticket ID. When the
/// current branch itself names the ticket, include its uncommitted work too.
#[tauri::command]
pub fn git_worktree_list(repo: String) -> Result<Vec<Worktree>, String> {
    let root = Path::new(&repo);
    let canon_repo = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let out = run_git(root, &["worktree", "list", "--porcelain"])?;
    let mut trees: Vec<Worktree> = Vec::new();
    let mut cur: Option<Worktree> = None;
    for line in out.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(done) = cur.take() {
                trees.push(done);
            }
            cur = Some(Worktree {
                path: path.trim().to_string(),
                branch: String::new(),
                head: String::new(),
                changes: 0,
                current: false,
            });
        } else if let Some(head) = line.strip_prefix("HEAD ") {
            if let Some(w) = cur.as_mut() {
                w.head = head.trim().chars().take(8).collect();
            }
        } else if let Some(branch) = line.strip_prefix("branch ") {
            if let Some(w) = cur.as_mut() {
                w.branch = branch.trim().trim_start_matches("refs/heads/").to_string();
            }
        } else if line.trim() == "detached" {
            if let Some(w) = cur.as_mut() {
                w.branch = "(detached)".to_string();
            }
        }
    }
    if let Some(done) = cur.take() {
        trees.push(done);
    }
    for w in trees.iter_mut() {
        let wt = Path::new(&w.path);
        w.current = std::fs::canonicalize(wt).map(|p| p == canon_repo).unwrap_or(false);
        w.changes = run_git(wt, &["status", "--porcelain"])
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count() as u32)
            .unwrap_or(0);
    }
    Ok(trees)
}

#[tauri::command]
pub async fn git_ticket_files(repo: String, ticket_id: String) -> Result<Vec<ChangedFile>, String> {
    // Synchronous Tauri commands execute on the UI thread. A large ticket
    // history froze the native event loop for minutes when Vault opened.
    ticket_files_task(move || ticket_files(repo, ticket_id)).await
}

static TICKET_FILE_QUERIES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

async fn ticket_files_task<F>(query: F) -> Result<Vec<ChangedFile>, String>
where
    F: FnOnce() -> Result<Vec<ChangedFile>, String> + Send + 'static,
{
    let permit = TICKET_FILE_QUERIES.acquire().await.map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        query()
    }).await.map_err(|e| e.to_string())?
}

fn ticket_files(repo: String, ticket_id: String) -> Result<Vec<ChangedFile>, String> {
    let root = Path::new(&repo);
    let hashes = run_git(
        root,
        &["log", "--all", "--format=%H", "--regexp-ignore-case", &format!("--grep={ticket_id}")],
    )?;
    let mut files: HashMap<String, ChangedFile> = HashMap::new();
    for hash in hashes.lines().filter(|line| !line.trim().is_empty()) {
        let numstat = run_git(root, &["show", "--format=", "--numstat", hash])?;
        let statuses: HashMap<String, String> = run_git(root, &["show", "--format=", "--name-status", hash])?
            .lines()
            .filter_map(parse_name_status_line)
            .map(|(status, path)| (path, status))
            .collect();
        for (path, additions, deletions) in numstat.lines().filter_map(parse_numstat_line) {
            let entry = files.entry(path.clone()).or_insert(ChangedFile {
                path: path.clone(), additions: 0, deletions: 0,
                status: statuses.get(&path).cloned().unwrap_or_else(|| "M".into()),
            });
            entry.additions += additions;
            entry.deletions += deletions;
        }
    }
    let branch = run_git(root, &["branch", "--show-current"]).unwrap_or_default();
    let ticket_slug = ticket_id.to_lowercase().replace('_', "-");
    if branch.to_lowercase().replace('_', "-").contains(&ticket_slug) {
        for changed in git_uncommitted_files(repo.clone())? {
            let entry = files.entry(changed.path.clone()).or_insert(ChangedFile {
                path: changed.path.clone(), additions: 0, deletions: 0, status: changed.status.clone(),
            });
            entry.additions += changed.additions;
            entry.deletions += changed.deletions;
            entry.status = changed.status;
        }
    }
    let mut result: Vec<_> = files.into_values().collect();
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

#[tauri::command]
pub fn git_outgoing_commits(repo: String, limit: u32) -> Result<Vec<CommitMeta>, String> {
    let repo = Path::new(&repo);
    let limit_str = limit.to_string();
    let mut args = vec![
        "log",
        "--format=%H%x09%h%x09%s%x09%an%x09%ad%x09%D",
        "--date=format:%b %d",
        "-n",
        &limit_str,
    ];
    if upstream_of(repo).is_some() {
        args.push("@{upstream}..HEAD");
    }
    let out = run_git(repo, &args)?;
    Ok(out.lines().filter_map(parse_log_line).collect())
}

#[tauri::command]
pub fn git_file_diff(
    repo: String,
    path: String,
    staged: bool,
    outgoing: bool,
) -> Result<String, String> {
    let repo = Path::new(&repo);
    let diff = if outgoing {
        match upstream_of(repo).or_else(|| first_remote_branch(repo)) {
            Some(base) => {
                let range = format!("{base}...HEAD");
                run_git(repo, &["diff", &range, "--", &path])?
            }
            None => String::new(),
        }
    } else if staged {
        run_git(repo, &["diff", "--cached", "--", &path])?
    } else {
        run_git(repo, &["diff", "--", &path])?
    };

    // Untracked file: empty diff, exists on disk, unknown to the index —
    // synthesize an add-diff via --no-index so the pane still shows content.
    if diff.trim().is_empty() && repo.join(&path).is_file() {
        let tracked = run_git(repo, &["ls-files", "--error-unmatch", "--", &path]).is_ok();
        if !tracked {
            let output = Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["diff", "--no-index", "/dev/null", &path])
                .output()
                .map_err(|e| format!("failed to invoke git: {e}"))?;
            // --no-index exits 1 when the files differ — that's success here.
            return match output.status.code() {
                Some(0) | Some(1) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
                _ => Err(format!(
                    "git diff --no-index failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )),
            };
        }
    }

    Ok(diff)
}

/// The full diff of one commit — what a released project's "Recent commits"
/// row opens, so work that is already committed and pushed is still reviewable.
#[tauri::command]
pub fn git_commit_diff(repo: String, sha: String) -> Result<String, String> {
    let repo = Path::new(&repo);
    let sha = sha.trim();
    if sha.is_empty() {
        return Err("which commit?".into());
    }
    run_git(repo, &["show", "--format=fuller", sha])
}

#[tauri::command]
pub fn git_stage(repo: String, path: String) -> Result<(), String> {
    run_git(Path::new(&repo), &["add", "--", &path]).map(|_| ())
}

#[tauri::command]
pub fn git_unstage(repo: String, path: String) -> Result<(), String> {
    run_git(Path::new(&repo), &["restore", "--staged", "--", &path]).map(|_| ())
}

#[tauri::command]
pub fn git_commit(repo: String, message: String) -> Result<String, String> {
    if message.trim().is_empty() {
        return Err("commit message is empty".into());
    }
    let repo = Path::new(&repo);
    run_git(repo, &["commit", "-m", &message])?;
    Ok(run_git(repo, &["rev-parse", "HEAD"])?.trim().to_string())
}

#[tauri::command]
pub fn git_push(repo: String, force_with_lease: bool) -> Result<String, String> {
    let mut args = vec!["push"];
    if force_with_lease {
        args.push("--force-with-lease");
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(&args)
        .output()
        .map_err(|e| format!("failed to invoke git: {e}"))?;
    // git prints progress to stderr even on success — return both streams.
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    let combined = combined.trim().to_string();
    // Keep only the tail so huge progress dumps don't flood the UI.
    const TAIL: usize = 2000;
    let tail = if combined.len() > TAIL {
        let cut = combined.len() - TAIL;
        let start = (cut..combined.len())
            .find(|&i| combined.is_char_boundary(i))
            .unwrap_or(0);
        combined[start..].to_string()
    } else {
        combined
    };
    if output.status.success() {
        Ok(tail)
    } else {
        Err(tail)
    }
}

#[tauri::command]
pub fn git_branches(repo: String) -> Result<Vec<String>, String> {
    let out = run_git(Path::new(&repo), &["branch", "--format=%(refname:short)"])?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

const COMMIT_MESSAGE_SYSTEM: &str = "You write git commit messages. Use conventional commit \
format (feat:, fix:, chore:, refactor:, docs:, test:, ...), imperative mood, subject line \
under 72 characters, optional body separated by a blank line. Return ONLY the commit \
message — no commentary, no markdown fences.";

#[tauri::command]
pub async fn git_ai_commit_message(
    state: tauri::State<'_, crate::state::AppState>,
    repo: String,
) -> Result<String, String> {
    let repo_path = Path::new(&repo);
    let mut diff = run_git(repo_path, &["diff", "--cached"])?;
    if diff.trim().is_empty() {
        diff = run_git(repo_path, &["diff"])?;
    }
    if diff.trim().is_empty() {
        return Err("nothing to commit".into());
    }
    if diff.len() > 12000 {
        let end = (0..=12000)
            .rev()
            .find(|&i| diff.is_char_boundary(i))
            .unwrap_or(0);
        diff.truncate(end);
    }

    let llm = state.settings.lock().await.llm.clone();
    let reply = crate::chat::complete_oneshot(
        &llm,
        Some(COMMIT_MESSAGE_SYSTEM),
        &format!("Generate the commit message for this diff:\n\n{diff}"),
    )
    .await?;
    Ok(strip_code_fences(&reply))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

// ── Delivery: commit log and release history (XNAUT-208) ────────────────────
//
// The Delivery panel joins three things that were already immutable and never
// joined: what was asked (the control repo), what changed (git), what shipped
// (version tags). These two commands supply the git half. Grouping and stats
// stay in JS; Rust only shells out.

#[derive(Debug, Clone, Serialize)]
pub struct CommitStat {
    pub sha: String,
    pub short_sha: String,
    pub subject: String,
    pub author: String,
    pub date: String,
    pub added: u32,
    pub deleted: u32,
    pub files: Vec<String>,
    /// First ticket id in the subject, e.g. "XNAUT-208".
    pub ticket: Option<String>,
    /// Earliest `v*` tag containing this commit, i.e. the release it shipped in.
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Release {
    pub tag: String,
    pub date: String,
    pub subject: String,
    pub commits: u32,
    pub tickets: Vec<String>,
}

/// First `KEY-123` in a commit subject, matched against the project keys the
/// caller knows about. A generic `[A-Z]+-\d+` scan reads "UTF-8" as a ticket;
/// the key list is both simpler and unable to be wrong.
fn ticket_of(subject: &str, keys: &[String]) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for key in keys {
        let needle = format!("{key}-");
        for (at, _) in subject.match_indices(&needle) {
            // Must start a word: "XXNAUT-1" is not XNAUT-1.
            if at > 0 && subject.as_bytes()[at - 1].is_ascii_alphanumeric() {
                continue;
            }
            let rest = &subject[at + needle.len()..];
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            if digits == 0 {
                continue;
            }
            // Must end a word: "XNAUT-1a" is not a ticket.
            if rest[digits..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
            {
                continue;
            }
            let id = format!("{key}-{}", &rest[..digits]);
            if best.as_ref().is_none_or(|(seen, _)| at < *seen) {
                best = Some((at, id));
            }
        }
    }
    best.map(|(_, id)| id)
}

/// A version tag is `v` followed by a digit. Everything else in refs/tags
/// (`rc-*`, `backup/*`, a stray annotated note) is not a release.
fn is_version_tag(tag: &str) -> bool {
    tag.strip_prefix('v')
        .and_then(|rest| rest.chars().next())
        .is_some_and(|c| c.is_ascii_digit())
}

/// Walk every version tag range once, oldest first. Returns the per-tag release
/// rows and a sha -> tag map. One git call per tag, not one per commit: the tag
/// count is single digits where the commit count is hundreds.
fn walk_tags(
    repo: &Path,
    keys: &[String],
) -> (Vec<Release>, std::collections::HashMap<String, String>) {
    let mut map = std::collections::HashMap::new();
    let listing = match run_git(
        repo,
        &[
            "for-each-ref",
            "--sort=creatordate",
            "--format=%(refname:short)%09%(creatordate:short)%09%(contents:subject)",
            "refs/tags",
        ],
    ) {
        Ok(out) => out,
        Err(_) => return (Vec::new(), map),
    };

    let tags: Vec<(String, String, String)> = listing
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let tag = parts.next()?.to_string();
            if !is_version_tag(&tag) {
                return None;
            }
            let date = parts.next().unwrap_or("").to_string();
            let subject = parts.next().unwrap_or("").to_string();
            Some((tag, date, subject))
        })
        .collect();

    let mut releases = Vec::new();
    for (idx, (tag, date, subject)) in tags.iter().enumerate() {
        let range = match idx {
            0 => tag.clone(),
            _ => format!("{}..{}", tags[idx - 1].0, tag),
        };
        let out = run_git(repo, &["log", "--no-merges", "--format=%H%x09%s", &range])
            .unwrap_or_default();
        let mut tickets: Vec<String> = Vec::new();
        let mut commits = 0u32;
        for line in out.lines() {
            let (sha, subj) = line.split_once('\t').unwrap_or((line, ""));
            commits += 1;
            // Oldest tag wins: the release a commit first shipped in.
            map.entry(sha.to_string()).or_insert_with(|| tag.clone());
            if let Some(id) = ticket_of(subj, keys) {
                if !tickets.contains(&id) {
                    tickets.push(id);
                }
            }
        }
        releases.push(Release {
            tag: tag.clone(),
            date: date.clone(),
            subject: subject.clone(),
            commits,
            tickets,
        });
    }
    releases.reverse(); // newest first, the order anyone reads them in
    (releases, map)
}

#[tauri::command]
pub fn git_release_history(repo: String, keys: Vec<String>) -> Result<Vec<Release>, String> {
    Ok(walk_tags(Path::new(&repo), &keys).0)
}

/// What a release actually said, and what went into it.
///
/// The Releases tab used to show a tag, a date and a count: everything a
/// person could get from `git tag`. The thing they came for is the release
/// text, which lives in CHANGELOG.md in this repo and in the annotated tag's
/// body in others, and the commits behind it.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ReleaseNotes {
    pub tag: String,
    pub previous: String,
    /// Where this release can be read in a browser, empty when no remote of
    /// this checkout is a forge we can address.
    pub url: String,
    pub notes: String,
    /// `changelog`, `tag`, or `none`. The reader should know which they got.
    pub notes_source: String,
    pub commits: Vec<CommitStat>,
}

#[tauri::command]
pub fn git_release_notes(repo: String, tag: String, keys: Vec<String>) -> Result<ReleaseNotes, String> {
    let path = Path::new(&repo);
    if !is_version_tag(&tag) {
        return Err(format!("not a version tag: {tag}"));
    }
    let (releases, tag_of) = walk_tags(path, &keys);
    let idx = releases
        .iter()
        .position(|r| r.tag == tag)
        .ok_or_else(|| format!("no such tag in this repository: {tag}"))?;
    // `releases` is newest first, so the one after it in the list is older.
    let previous = releases.get(idx + 1).map(|r| r.tag.clone()).unwrap_or_default();

    let (notes, notes_source) = release_notes_text(path, &tag);
    let range = if previous.is_empty() { tag.clone() } else { format!("{previous}..{tag}") };
    let commits = commits_in(path, &range, &keys, &tag_of)?;
    let url = release_url(path, &tag);
    Ok(ReleaseNotes { tag, previous, url, notes, notes_source, commits })
}

/// The release's page on the forge it is published to. GitHub is preferred
/// because that is where xNAUT's releases are public; Forgejo answers the
/// same path, so a project that only has the private remote still gets a
/// link rather than nothing.
///
/// The host is read from the URL's host segment, never assumed from the
/// remote's NAME: `origin` here is an SSH alias (`github-com-48Nauts`), so
/// matching on "origin" or on a literal "github.com" both miss.
fn release_url(repo: &Path, tag: &str) -> String {
    let remotes = run_git(repo, &["remote"]).unwrap_or_default();
    let urls: Vec<String> = remotes
        .lines()
        .filter_map(|name| run_git(repo, &["remote", "get-url", name.trim()]).ok())
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .collect();
    let pick = urls
        .iter()
        .find(|u| host_of(u).contains("github"))
        .or_else(|| urls.first());
    let Some(url) = pick else { return String::new() };
    let host = host_of(url);
    let path = path_of(url);
    if host.is_empty() || path.is_empty() {
        return String::new();
    }
    // An SSH alias is not a hostname a browser can resolve. The alias carries
    // the real host in its own name, which is the convention here.
    let web_host = if host.contains("github") {
        "github.com".to_string()
    } else if host.starts_with("http") {
        host
    } else {
        host
    };
    let scheme = if web_host == "github.com" { "https" } else { "http" };
    format!("{scheme}://{web_host}/{path}/releases/tag/{tag}")
}

/// The host segment of a git remote, SSH or HTTP, lowercased.
fn host_of(url: &str) -> String {
    let rest = url
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(url)
        .trim_start_matches(|c| c != '@' && c == ' ');
    let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
    rest.split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// The `owner/repo` part, without the `.git` suffix.
fn path_of(url: &str) -> String {
    let rest = url
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(url);
    let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
    let after_host = match rest.find([':', '/']) {
        Some(i) => &rest[i + 1..],
        None => return String::new(),
    };
    // `host:3000/owner/repo` leaves the port at the front once the colon is
    // consumed; drop a leading all-digit segment.
    let mut parts: Vec<&str> = after_host.split('/').filter(|p| !p.is_empty()).collect();
    if parts.first().is_some_and(|p| p.chars().all(|c| c.is_ascii_digit())) {
        parts.remove(0);
    }
    parts
        .join("/")
        .trim_end_matches(".git")
        .trim_matches('/')
        .to_string()
}

/// The changelog section for this version, else the annotated tag's own body.
/// A lightweight tag has neither, and says so rather than showing the subject
/// twice.
fn release_notes_text(repo: &Path, tag: &str) -> (String, String) {
    let version = tag.trim_start_matches('v');
    if let Ok(changelog) = std::fs::read_to_string(repo.join("CHANGELOG.md")) {
        if let Some(section) = changelog_section(&changelog, version) {
            return (section, "changelog".into());
        }
    }
    let body = run_git(repo, &["tag", "-l", "--format=%(contents:body)", tag])
        .unwrap_or_default()
        .trim()
        .to_string();
    if body.is_empty() { (String::new(), "none".into()) } else { (body, "tag".into()) }
}

/// Everything under `## [1.27.0]` (or `## 1.27.0`) up to the next `## `.
/// Keep-a-Changelog is the shape here, but the only thing assumed is that a
/// version heading starts with `## ` and contains the version.
fn changelog_section(changelog: &str, version: &str) -> Option<String> {
    let mut lines = changelog.lines();
    let mut out: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in lines.by_ref() {
        if let Some(rest) = line.strip_prefix("## ") {
            if inside {
                break;
            }
            // `[1.27.0] - 2026-09-11` and `1.27.0` both match; `1.27.01` does
            // not, or 1.2 would swallow 1.27.
            let head = rest.trim_start_matches('[');
            if head.starts_with(version)
                && head[version.len()..].chars().next().is_none_or(|c| !c.is_ascii_digit() && c != '.')
            {
                inside = true;
                continue;
            }
        }
        if inside {
            out.push(line);
        }
    }
    if !inside {
        return None;
    }
    let text = out.join("\n").trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

#[tauri::command]
pub fn git_commit_log(
    repo: String,
    since: String,
    limit: u32,
    keys: Vec<String>,
) -> Result<Vec<CommitStat>, String> {
    let repo = Path::new(&repo);
    let (_, tag_of) = walk_tags(repo, &keys);
    let since_arg = format!("--since={since}");
    let limit_arg = limit.to_string();
    let out = run_git(
        repo,
        &[
            "log",
            // Every ref, not HEAD: feature work lives in worktrees on their own
            // branches, and "what did this project do" means all of it. HEAD
            // alone reports 71 of the last five days' 145 commits.
            "--all",
            "--no-merges",
            "--numstat",
            "--date=short",
            // \x01 marks a header line; numstat rows in between belong to it.
            "--format=\u{1}%H%x09%h%x09%s%x09%an%x09%ad",
            &since_arg,
            "-n",
            &limit_arg,
        ],
    )?;

    Ok(parse_log(&out, &keys, &tag_of))
}

/// `git log --numstat` with the \x01-marked header format, parsed. Shared by
/// the windowed log and the per-release one so the two can never disagree
/// about what a commit is.
fn parse_log(
    out: &str,
    keys: &[String],
    tag_of: &std::collections::HashMap<String, String>,
) -> Vec<CommitStat> {
    let mut commits: Vec<CommitStat> = Vec::new();
    for line in out.lines() {
        if let Some(header) = line.strip_prefix('\u{1}') {
            let f: Vec<&str> = header.split('\t').collect();
            if f.len() < 5 {
                continue;
            }
            commits.push(CommitStat {
                sha: f[0].to_string(),
                short_sha: f[1].to_string(),
                subject: f[2].to_string(),
                author: f[3].to_string(),
                date: f[4].to_string(),
                added: 0,
                deleted: 0,
                files: Vec::new(),
                ticket: ticket_of(f[2], keys),
                tag: tag_of.get(f[0]).cloned(),
            });
            continue;
        }
        let Some(current) = commits.last_mut() else {
            continue;
        };
        let mut parts = line.split('\t');
        let (Some(add), Some(del), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        // "-" in place of a count means a binary file. It still counts as touched.
        current.added += add.parse::<u32>().unwrap_or(0);
        current.deleted += del.parse::<u32>().unwrap_or(0);
        current.files.push(path.to_string());
    }
    commits
}

/// The commits in one tag range, newest first.
fn commits_in(
    repo: &Path,
    range: &str,
    keys: &[String],
    tag_of: &std::collections::HashMap<String, String>,
) -> Result<Vec<CommitStat>, String> {
    let out = run_git(
        repo,
        &[
            "log",
            "--no-merges",
            "--numstat",
            "--date=short",
            "--format=\u{1}%H%x09%h%x09%s%x09%an%x09%ad",
            range,
        ],
    )?;
    Ok(parse_log(&out, keys, tag_of))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn ticket_history_queries_leave_the_ui_executor_free_and_bound_parallel_work() {
        use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
        let caller = std::thread::current().id();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut jobs = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let active = active.clone();
            let peak = peak.clone();
            let (notify, ready) = tokio::sync::oneshot::channel();
            let (respond, ui_response) = std::sync::mpsc::channel();
            jobs.spawn(async move {
                let query = ticket_files_task(move || {
                    assert_ne!(std::thread::current().id(), caller, "Git must not run on the caller's UI thread");
                    let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(count, Ordering::SeqCst);
                    notify.send(()).unwrap();
                    // The query cannot finish until the UI executor responds.
                    // Running it synchronously would deadlock this handshake.
                    ui_response.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(vec![])
                });
                let ui = async move { ready.await.unwrap(); respond.send(()).unwrap(); };
                let (result, ()) = tokio::join!(query, ui);
                result.unwrap();
            });
        }
        while let Some(job) = jobs.join_next().await { job.unwrap(); }
        assert!(peak.load(Ordering::SeqCst) <= 2, "a large project cannot launch an unbounded Git burst");
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_remote_becomes_a_release_page_whatever_shape_the_url_is() {
        // The SSH alias in use here: the host segment is not github.com, and
        // matching on the remote's name would pick the wrong forge entirely.
        assert_eq!(host_of("git@github-com-48Nauts:48Nauts-Operator/xNaut.git"), "github-com-48nauts");
        assert_eq!(path_of("git@github-com-48Nauts:48Nauts-Operator/xNaut.git"), "48Nauts-Operator/xNaut");
        assert_eq!(host_of("https://github.com/48Nauts-Operator/xNaut.git"), "github.com");
        assert_eq!(path_of("https://github.com/48Nauts-Operator/xNaut.git"), "48Nauts-Operator/xNaut");

        // Forgejo on the NAS, with a port in the URL.
        assert_eq!(host_of("http://cosmos.tail138398.ts.net:3000/48Nauts/xnaut.git"), "cosmos.tail138398.ts.net");
        assert_eq!(path_of("http://cosmos.tail138398.ts.net:3000/48Nauts/xnaut.git"), "48Nauts/xnaut");
        assert_eq!(path_of("forgejo:48Nauts/xnaut.git"), "48Nauts/xnaut");

        // Nothing that can be addressed is an empty link, not a broken one.
        assert_eq!(path_of("weird"), "");
    }

    #[test]
    fn the_changelog_section_for_a_version_is_that_version_only() {
        let log = "# Changelog\n\n## [1.27.0] - 2026-09-11\n\nThe release where delivery became readable.\n\n### Added\n- A thing.\n\n## [1.26.4] - 2026-09-11\n\nThe one before.\n";
        let got = changelog_section(log, "1.27.0").expect("1.27.0 is in there");
        assert!(got.starts_with("The release where delivery became readable."));
        assert!(got.contains("- A thing."));
        assert!(!got.contains("The one before."), "it stops at the next version: {got}");

        // The older entry is readable too, and it is the last one in the file.
        assert_eq!(changelog_section(log, "1.26.4").unwrap(), "The one before.");

        // A version that is not there is absent, not the whole file, and a
        // prefix is not a match or 1.2 would return 1.27.0's notes.
        assert!(changelog_section(log, "9.9.9").is_none());
        assert!(changelog_section(log, "1.2").is_none());

        // A heading with no body is nothing to show, not an empty section.
        assert!(changelog_section("## [2.0.0]\n\n## [1.0.0]\n\nreal\n", "2.0.0").is_none());

        // The bare form, without brackets or a date.
        assert_eq!(changelog_section("## 3.1.0\nbody\n", "3.1.0").unwrap(), "body");
    }

    #[test]
    fn parses_numstat_lines() {
        assert_eq!(
            parse_numstat_line("12\t3\tsrc/main.rs"),
            Some(("src/main.rs".to_string(), 12, 3))
        );
        // Binary files report "-" for both counts → 0/0.
        assert_eq!(
            parse_numstat_line("-\t-\tfoo.png"),
            Some(("foo.png".to_string(), 0, 0))
        );
        assert_eq!(parse_numstat_line(""), None);
    }

    #[test]
    fn parses_porcelain_status_lines() {
        assert_eq!(
            parse_porcelain_line(" M file"),
            Some(("M".to_string(), "file".to_string()))
        );
        assert_eq!(
            parse_porcelain_line("?? new"),
            Some(("?".to_string(), "new".to_string()))
        );
        assert_eq!(
            parse_porcelain_line("A  staged"),
            Some(("A".to_string(), "staged".to_string()))
        );
        assert_eq!(
            parse_porcelain_line("R  old.rs -> new.rs"),
            Some(("R".to_string(), "new.rs".to_string()))
        );
        assert_eq!(parse_porcelain_line(""), None);
    }

    #[test]
    fn parses_name_status_lines() {
        assert_eq!(
            parse_name_status_line("M\tsrc/lib.rs"),
            Some(("M".to_string(), "src/lib.rs".to_string()))
        );
        assert_eq!(
            parse_name_status_line("R100\told.rs\tnew.rs"),
            Some(("R".to_string(), "new.rs".to_string()))
        );
    }

    #[test]
    fn parses_log_lines() {
        let line =
            "abc123full\tabc123\tfeat: add git pane\tAndre\tJun 09\tHEAD -> main, origin/main";
        let meta = parse_log_line(line).unwrap();
        assert_eq!(meta.sha, "abc123full");
        assert_eq!(meta.short_sha, "abc123");
        assert_eq!(meta.subject, "feat: add git pane");
        assert_eq!(meta.author, "Andre");
        assert_eq!(meta.date, "Jun 09");
        assert_eq!(meta.refs, "HEAD -> main, origin/main");

        // Commit with no refs decoration still parses (refs empty).
        let bare = parse_log_line("sha\tsh\tsubject\tauthor\tJun 01\t").unwrap();
        assert_eq!(bare.refs, "");
    }

    #[test]
    fn strips_code_fences_from_llm_reply() {
        assert_eq!(
            strip_code_fences("feat: plain message"),
            "feat: plain message"
        );
        assert_eq!(strip_code_fences("```\nfeat: fenced\n```"), "feat: fenced");
        assert_eq!(
            strip_code_fences("```text\nfix: with lang tag\n\nbody line\n```"),
            "fix: with lang tag\n\nbody line"
        );
    }

    #[test]
    fn joins_numstat_counts_with_statuses() {
        let numstat = "10\t2\ta.rs\n-\t-\tb.png\n";
        let statuses = vec![
            ("M".to_string(), "a.rs".to_string()),
            ("A".to_string(), "b.png".to_string()),
            ("?".to_string(), "untracked.txt".to_string()),
        ];
        let files = join_changed_files(numstat, &statuses);
        assert_eq!(files.len(), 3);
        assert_eq!((files[0].additions, files[0].deletions), (10, 2));
        assert_eq!(files[0].status, "M");
        assert_eq!((files[1].additions, files[1].deletions), (0, 0));
        assert_eq!((files[2].additions, files[2].deletions), (0, 0));
        assert_eq!(files[2].status, "?");
    }

    // These two are the "silently matches nothing" shape: a wrong boundary here
    // shows up as an empty Release column, never as an error.
    #[test]
    fn reads_the_ticket_id_out_of_a_subject() {
        let keys = ["XNAUT".to_string(), "NAUTGATE".to_string()];
        let id = |s| ticket_of(s, &keys);
        assert_eq!(id("feat: XNAUT-208 delivery loop").as_deref(), Some("XNAUT-208"));
        assert_eq!(id("XNAUT-12: fix").as_deref(), Some("XNAUT-12"));
        assert_eq!(id("fix(pm): NAUTGATE-35 tools lane").as_deref(), Some("NAUTGATE-35"));
        assert_eq!(id("first of XNAUT-1 and XNAUT-2").as_deref(), Some("XNAUT-1"));
        // Not ticket ids.
        assert_eq!(id("chore: bump to v1.18.1"), None);
        assert_eq!(id("fix UTF-8 decoding"), None);
        assert_eq!(id("about XNAUT-1a, a typo"), None);
        assert_eq!(id("XXNAUT-9 is not our key"), None);
        assert_eq!(id("ENGRAM-4 belongs to another project"), None);
        assert_eq!(id("no id here at all"), None);
    }

    #[test]
    fn only_v_digit_tags_count_as_releases() {
        assert!(is_version_tag("v1.18.1"));
        assert!(is_version_tag("v2"));
        assert!(!is_version_tag("v-old"));
        assert!(!is_version_tag("release-1.0"));
        assert!(!is_version_tag("backup/v1.0"));
        assert!(!is_version_tag(""));
    }
}
