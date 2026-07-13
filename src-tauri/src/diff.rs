// Git-diff backend for Phase 8a — structured JSON the diff pane can render.
//
// Calls `git diff` (or `git show`) and parses the unified-diff output into a
// per-file, per-hunk model. Heavy lifting (syntax highlighting, layout) lives
// in the frontend — this module's job is to deliver clean structured data.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Context,
    Add,
    Del,
    Header,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    /// 1-indexed line number on the pre-image side, or None for added lines.
    pub old_line: Option<u32>,
    /// 1-indexed line number on the post-image side, or None for deleted lines.
    pub new_line: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiffFile {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub is_binary: bool,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiffSet {
    pub files: Vec<DiffFile>,
}

fn run_git(args: &[&str], cwd: &Path) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Parses unified-diff output (the kind `git diff` emits with no special flags).
pub fn parse_unified_diff(raw: &str) -> DiffSet {
    let mut set = DiffSet::default();
    let mut current: Option<DiffFile> = None;
    let mut hunk: Option<DiffHunk> = None;
    let mut old_n: u32 = 0;
    let mut new_n: u32 = 0;

    let push_hunk = |current: &mut Option<DiffFile>, hunk: &mut Option<DiffHunk>| {
        if let (Some(file), Some(h)) = (current.as_mut(), hunk.take()) {
            file.hunks.push(h);
        }
    };
    let push_file = |set: &mut DiffSet, current: &mut Option<DiffFile>| {
        if let Some(f) = current.take() {
            set.files.push(f);
        }
    };

    for line in raw.split_inclusive('\n') {
        let stripped = line.trim_end_matches('\n');
        if let Some(rest) = stripped.strip_prefix("diff --git ") {
            push_hunk(&mut current, &mut hunk);
            push_file(&mut set, &mut current);
            let mut file = DiffFile::default();
            // best-effort: "a/foo b/bar"
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2 {
                file.old_path = Some(parts[0].trim_start_matches("a/").to_string());
                file.new_path = Some(parts[1].trim_start_matches("b/").to_string());
            }
            current = Some(file);
            continue;
        }
        if stripped == "Binary files differ" || stripped.starts_with("Binary files ") {
            if let Some(f) = current.as_mut() {
                f.is_binary = true;
            }
            continue;
        }
        if let Some(rest) = stripped.strip_prefix("--- ") {
            if let Some(f) = current.as_mut() {
                if rest != "/dev/null" {
                    f.old_path = Some(rest.trim_start_matches("a/").to_string());
                } else {
                    f.old_path = None;
                }
            }
            continue;
        }
        if let Some(rest) = stripped.strip_prefix("+++ ") {
            if let Some(f) = current.as_mut() {
                if rest != "/dev/null" {
                    f.new_path = Some(rest.trim_start_matches("b/").to_string());
                } else {
                    f.new_path = None;
                }
            }
            continue;
        }
        if let Some(rest) = stripped.strip_prefix("@@") {
            push_hunk(&mut current, &mut hunk);
            // @@ -A,B +C,D @@ optional-context
            let header = stripped.to_string();
            if let Some((meta, _)) = rest.split_once("@@") {
                let meta = meta.trim();
                let mut old_start = 0;
                let mut old_count = 1;
                let mut new_start = 0;
                let mut new_count = 1;
                for tok in meta.split_whitespace() {
                    if let Some(t) = tok.strip_prefix('-') {
                        let (s, c) = parse_range(t);
                        old_start = s;
                        old_count = c;
                    } else if let Some(t) = tok.strip_prefix('+') {
                        let (s, c) = parse_range(t);
                        new_start = s;
                        new_count = c;
                    }
                }
                old_n = old_start;
                new_n = new_start;
                hunk = Some(DiffHunk {
                    header,
                    old_start,
                    old_count,
                    new_start,
                    new_count,
                    lines: Vec::new(),
                });
            }
            continue;
        }
        // Body lines — only meaningful inside a hunk.
        let h = match hunk.as_mut() {
            Some(h) => h,
            None => continue,
        };
        let first = stripped.chars().next();
        match first {
            Some('+') => {
                h.lines.push(DiffLine {
                    kind: LineKind::Add,
                    old_line: None,
                    new_line: Some(new_n),
                    content: stripped[1..].to_string(),
                });
                new_n += 1;
            }
            Some('-') => {
                h.lines.push(DiffLine {
                    kind: LineKind::Del,
                    old_line: Some(old_n),
                    new_line: None,
                    content: stripped[1..].to_string(),
                });
                old_n += 1;
            }
            Some(' ') | None => {
                h.lines.push(DiffLine {
                    kind: LineKind::Context,
                    old_line: Some(old_n),
                    new_line: Some(new_n),
                    content: if stripped.is_empty() {
                        String::new()
                    } else {
                        stripped[1..].to_string()
                    },
                });
                old_n += 1;
                new_n += 1;
            }
            _ => {
                // "\ No newline at end of file" and similar — pass through as header
                h.lines.push(DiffLine {
                    kind: LineKind::Header,
                    old_line: None,
                    new_line: None,
                    content: stripped.to_string(),
                });
            }
        }
    }
    push_hunk(&mut current, &mut hunk);
    push_file(&mut set, &mut current);
    set
}

fn parse_range(tok: &str) -> (u32, u32) {
    if let Some((s, c)) = tok.split_once(',') {
        (s.parse().unwrap_or(0), c.parse().unwrap_or(1))
    } else {
        (tok.parse().unwrap_or(0), 1)
    }
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn diff_for_worktree(worktree: String) -> Result<DiffSet, String> {
    let worktree = expand_tilde(&worktree);
    let path = Path::new(&worktree);

    if path.is_file() {
        // File inside a git repo → show its working-tree changes. Otherwise (no
        // repo, or an in-repo file with no changes) → show the whole file so ANY
        // file is viewable in the diff pane.
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        if let Ok(toplevel) = run_git(&["rev-parse", "--show-toplevel"], dir) {
            let raw = run_git(
                &[
                    "diff", "--no-color", "--no-ext-diff", "--unified=3", "HEAD", "--", &worktree,
                ],
                Path::new(toplevel.trim()),
            )?;
            if !raw.trim().is_empty() {
                return Ok(parse_unified_diff(&raw));
            }
        }
        return Ok(parse_unified_diff(&git_no_index(&worktree)?));
    }

    // Not a local file that exists → give an accurate reason, not a git fatal.
    if !path.exists() {
        if worktree.contains("://") {
            return Err(format!(
                "That's a URL — the diff reads local files and worktrees, not remote URLs: {worktree}"
            ));
        }
        return Err(format!("Path not found on disk: {worktree}"));
    }

    // Directory → whole-worktree diff.
    let toplevel = run_git(&["rev-parse", "--show-toplevel"], path)
        .map_err(|_| format!("Not inside a git repository: {worktree}"))?;
    let raw = run_git(
        &["diff", "--no-color", "--no-ext-diff", "--unified=3", "HEAD"],
        Path::new(toplevel.trim()),
    )?;
    Ok(parse_unified_diff(&raw))
}

/// Expand a leading `~/` — the input is a raw string the shell never touched.
fn expand_tilde(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => dirs::home_dir()
            .map(|home| home.join(rest).to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string()),
        None => p.to_string(),
    }
}

/// Whole-file view for ANY file (repo or not): diff it against /dev/null.
/// `git diff --no-index` needs no repo and exits 1 when files differ — expected.
fn git_no_index(file: &str) -> Result<String, String> {
    let out = Command::new("git")
        .args([
            "diff", "--no-color", "--no-ext-diff", "--unified=3", "--no-index", "/dev/null", file,
        ])
        .output()
        .map_err(|e| format!("git: {e}"))?;
    match out.status.code() {
        Some(0) | Some(1) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        _ => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
    }
}

#[tauri::command]
pub fn diff_for_commit(worktree: String, commit: String) -> Result<DiffSet, String> {
    let raw = run_git(
        &[
            "show",
            "--no-color",
            "--no-ext-diff",
            "--unified=3",
            "--first-parent",
            &commit,
        ],
        Path::new(&worktree),
    )?;
    Ok(parse_unified_diff(&raw))
}

#[tauri::command]
pub fn diff_against_ref(worktree: String, base_ref: String) -> Result<DiffSet, String> {
    let _ = PathBuf::from(&worktree);
    let raw = run_git(
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--unified=3",
            &base_ref,
        ],
        Path::new(&worktree),
    )?;
    Ok(parse_unified_diff(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_simple_hunk() {
        let raw = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"old\");
+    println!(\"new\");
+    println!(\"added\");
 }
";
        let set = parse_unified_diff(raw);
        assert_eq!(set.files.len(), 1);
        let f = &set.files[0];
        assert_eq!(f.new_path.as_deref(), Some("foo.rs"));
        let h = &f.hunks[0];
        assert_eq!(h.old_start, 1);
        assert_eq!(h.new_start, 1);
        // 1 context + 1 del + 2 add + 1 context = 5 lines
        let adds = h
            .lines
            .iter()
            .filter(|l| matches!(l.kind, LineKind::Add))
            .count();
        let dels = h
            .lines
            .iter()
            .filter(|l| matches!(l.kind, LineKind::Del))
            .count();
        assert_eq!(adds, 2);
        assert_eq!(dels, 1);
        // Line numbering: first context = old:1 new:1; del = old:2 new:None; adds = old:None new:2,3; final context = old:3 new:4
        assert_eq!(h.lines[0].old_line, Some(1));
        assert_eq!(h.lines[0].new_line, Some(1));
        assert_eq!(h.lines[1].old_line, Some(2));
        assert_eq!(h.lines[1].new_line, None);
        assert_eq!(h.lines[2].new_line, Some(2));
        assert_eq!(h.lines[3].new_line, Some(3));
        assert_eq!(h.lines[4].old_line, Some(3));
        assert_eq!(h.lines[4].new_line, Some(4));
    }

    #[test]
    fn parses_binary_files_marker() {
        let raw = "\
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
";
        let set = parse_unified_diff(raw);
        assert_eq!(set.files.len(), 1);
        assert!(set.files[0].is_binary);
    }

    #[test]
    fn parses_range_with_count() {
        assert_eq!(parse_range("10,5"), (10, 5));
        assert_eq!(parse_range("7"), (7, 1));
    }
}
