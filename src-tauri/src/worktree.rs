// Worktree-per-agent flow. Git invocations ported from Orca's src/main/git/worktree.ts —
// `--no-track` + `push.autoSetupRemote=true` recipe avoids "behind by N" noise
// against the base ref before the agent has published anything.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub path: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    pub is_prunable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddWorktreeOptions {
    /// New branch name to create in the worktree. Required unless `checkout_existing` is true.
    pub branch: String,
    /// Base ref (branch or commit) to branch from. Defaults to the repo's HEAD.
    pub base: Option<String>,
    /// If true, check out `branch` as an existing branch instead of creating a new one.
    #[serde(default)]
    pub checkout_existing: bool,
    /// Skip `git config push.autoSetupRemote true` in the new worktree (rare).
    #[serde(default)]
    pub no_auto_setup_remote: bool,
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
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parses `git worktree list --porcelain` output into structured entries.
fn parse_worktree_list(porcelain: &str) -> Vec<WorktreeInfo> {
    let mut out = Vec::new();
    let mut cur: Option<WorktreeInfo> = None;
    for line in porcelain.lines() {
        if line.is_empty() {
            if let Some(w) = cur.take() {
                out.push(w);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("worktree ") {
            if let Some(w) = cur.take() {
                out.push(w);
            }
            cur = Some(WorktreeInfo {
                path: rest.to_string(),
                branch: None,
                head: None,
                is_bare: false,
                is_detached: false,
                is_locked: false,
                is_prunable: false,
            });
        } else if let Some(w) = cur.as_mut() {
            if let Some(rest) = line.strip_prefix("HEAD ") {
                w.head = Some(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("branch refs/heads/") {
                w.branch = Some(rest.to_string());
            } else if line == "bare" {
                w.is_bare = true;
            } else if line == "detached" {
                w.is_detached = true;
            } else if line.starts_with("locked") {
                w.is_locked = true;
            } else if line.starts_with("prunable") {
                w.is_prunable = true;
            }
        }
    }
    if let Some(w) = cur.take() {
        out.push(w);
    }
    out
}

/// Platform-aware path equality used to dedupe worktree lookups.
/// Windows file paths are case-insensitive; everywhere else they are not.
pub fn are_worktree_paths_equal(a: &Path, b: &Path) -> bool {
    let a_norm = a.to_string_lossy();
    let b_norm = b.to_string_lossy();
    if cfg!(target_os = "windows") {
        a_norm.to_lowercase() == b_norm.to_lowercase()
    } else {
        a_norm == b_norm
    }
}

pub fn list_worktrees(repo: &Path) -> Result<Vec<WorktreeInfo>, String> {
    let out = run_git(repo, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_list(&out))
}

pub fn add_worktree(
    repo: &Path,
    worktree_path: &Path,
    opts: &AddWorktreeOptions,
) -> Result<WorktreeInfo, String> {
    // Reject if the target path matches an existing worktree (platform-aware).
    let existing = list_worktrees(repo)?;
    for w in &existing {
        if are_worktree_paths_equal(Path::new(&w.path), worktree_path) {
            return Err(format!("worktree already exists at {}", w.path));
        }
    }

    let worktree_path_str = worktree_path.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec!["worktree".into(), "add".into()];

    if opts.checkout_existing {
        // Check out an existing branch into the new worktree path.
        args.push(worktree_path_str.clone());
        args.push(opts.branch.clone());
    } else {
        // Create a new branch. `--no-track` keeps `git status` quiet about the base ref.
        args.push("--no-track".into());
        args.push("-b".into());
        args.push(opts.branch.clone());
        args.push(worktree_path_str.clone());
        if let Some(base) = &opts.base {
            args.push(base.clone());
        }
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git(repo, &arg_refs)?;

    // Enable autoSetupRemote so a plain `git push` from the new worktree creates the
    // upstream automatically — no -u flag, no failed first push.
    if !opts.no_auto_setup_remote {
        let _ = Command::new("git")
            .arg("-C")
            .arg(worktree_path)
            .args(["config", "push.autoSetupRemote", "true"])
            .status();
    }

    // Return the newly-created worktree's info by re-listing.
    let after = list_worktrees(repo)?;
    after
        .into_iter()
        .find(|w| are_worktree_paths_equal(Path::new(&w.path), worktree_path))
        .ok_or_else(|| "worktree was created but not found in subsequent list".into())
}

/// Returns Err if the worktree has any uncommitted/untracked changes. Used as a
/// preflight before non-force removal so we don't silently delete unfinished work.
pub fn assert_worktree_clean_for_removal(worktree_path: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .map_err(|e| format!("failed to invoke git status: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        return Err(format!(
            "worktree is not clean (use force=true to override):\n{}",
            stdout.trim()
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoveWorktreeOptions {
    #[serde(default)]
    pub force: bool,
    /// If true (default), delete the branch when no other worktree uses it.
    /// Set to false to keep the branch around after removing the worktree.
    pub delete_branch: Option<bool>,
}

pub fn remove_worktree(
    repo: &Path,
    worktree_path: &Path,
    opts: &RemoveWorktreeOptions,
) -> Result<(), String> {
    // Capture the branch name before removal so we know what to delete after.
    let before = list_worktrees(repo)?;
    let target = before
        .iter()
        .find(|w| are_worktree_paths_equal(Path::new(&w.path), worktree_path))
        .ok_or_else(|| format!("no worktree at {}", worktree_path.display()))?;
    let branch = target.branch.clone();

    if !opts.force {
        assert_worktree_clean_for_removal(worktree_path)?;
    }

    // `git worktree remove` (with --force if requested), then prune to clean records.
    let mut args = vec!["worktree", "remove"];
    if opts.force {
        args.push("--force");
    }
    let path_str = worktree_path.to_string_lossy().into_owned();
    args.push(&path_str);
    run_git(repo, &args)?;
    let _ = run_git(repo, &["worktree", "prune"]);

    // Branch cleanup — only if (a) caller didn't opt out, and (b) no other worktree uses it.
    let delete_branch = opts.delete_branch.unwrap_or(true);
    if delete_branch {
        if let Some(branch) = branch {
            let after = list_worktrees(repo)?;
            let still_used = after.iter().any(|w| w.branch.as_deref() == Some(&branch));
            if !still_used {
                // best-effort: don't fail the whole removal if branch delete trips on an upstream
                let _ = run_git(repo, &["branch", "-D", &branch]);
            }
        }
    }

    Ok(())
}

/// Suggest a worktree path under the repo's parent dir, named after the branch.
/// e.g. repo at `/x/foo`, branch `feat/bar` → `/x/foo-worktrees/feat-bar`.
pub fn suggest_worktree_path(repo: &Path, branch: &str) -> PathBuf {
    let parent = repo.parent().unwrap_or(Path::new("."));
    let repo_name = repo
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into());
    let safe_branch = branch.replace('/', "-");
    parent.join(format!("{repo_name}-worktrees")).join(safe_branch)
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn worktree_list(repo_path: String) -> Result<Vec<WorktreeInfo>, String> {
    list_worktrees(Path::new(&repo_path))
}

#[tauri::command]
pub fn worktree_add(
    repo_path: String,
    worktree_path: String,
    opts: AddWorktreeOptions,
) -> Result<WorktreeInfo, String> {
    add_worktree(Path::new(&repo_path), Path::new(&worktree_path), &opts)
}

#[tauri::command]
pub fn worktree_remove(
    repo_path: String,
    worktree_path: String,
    opts: RemoveWorktreeOptions,
) -> Result<(), String> {
    remove_worktree(Path::new(&repo_path), Path::new(&worktree_path), &opts)
}

#[tauri::command]
pub fn worktree_suggest_path(repo_path: String, branch: String) -> Result<String, String> {
    Ok(suggest_worktree_path(Path::new(&repo_path), &branch)
        .to_string_lossy()
        .into_owned())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_list() {
        let input = "\
worktree /repos/foo
HEAD abc123
branch refs/heads/main

worktree /repos/foo-wt/feat-bar
HEAD def456
branch refs/heads/feat/bar

worktree /repos/foo-wt/detached
HEAD ghi789
detached
locked
";
        let parsed = parse_worktree_list(input);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].path, "/repos/foo");
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert_eq!(parsed[1].branch.as_deref(), Some("feat/bar"));
        assert!(parsed[2].is_detached);
        assert!(parsed[2].is_locked);
        assert!(parsed[2].branch.is_none());
    }

    #[test]
    fn path_equality_is_platform_aware() {
        let a = Path::new("/foo/Bar");
        let b = Path::new("/foo/bar");
        // posix systems should treat these as different
        #[cfg(not(target_os = "windows"))]
        assert!(!are_worktree_paths_equal(a, b));
        #[cfg(target_os = "windows")]
        assert!(are_worktree_paths_equal(a, b));
    }

    #[test]
    fn suggested_path_sanitizes_slashes_in_branch() {
        let p = suggest_worktree_path(Path::new("/x/foo"), "feat/bar");
        assert_eq!(p, PathBuf::from("/x/foo-worktrees/feat-bar"));
    }
}
