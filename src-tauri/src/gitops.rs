// Git plumbing for the v1.6 right-pane Git view: ahead/behind, outgoing files +
// commits, side-by-side diff data, stage/commit/push, and AI commit messages.

use serde::Serialize;
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
    let path = parts.last()?.trim();
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
