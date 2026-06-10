// Search policy replicated from Orca's src/shared/text-search.ts (MIT): rg --json primary,
// git-grep fallback, 15s/100-per-file/2000-total/5MB caps.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};

const TOTAL_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_MATCHES_PER_FILE: usize = 100;
const MAX_TOTAL_RESULTS: usize = 2000;
const MAX_FILESIZE: &str = "5M";
const MAX_LINE_CHARS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchOpts {
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub glob: Option<String>, // e.g. "*.rs"
    #[serde(default)]
    pub max_results: Option<usize>, // cap, default 2000
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchMatch {
    pub path: String, // relative to root, forward slashes
    pub line: u64,
    pub text: String, // the matching line, trimmed to 500 chars
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
    pub backend: String, // "rg" | "git-grep"
}

// ─── Pure helpers ────────────────────────────────────────────────────────────

/// Local copy of the PATH probe (same approach as binary_on_path in agents.rs).
fn binary_on_path(name: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    path.split(':')
        .filter(|dir| !dir.is_empty())
        .any(|dir| Path::new(dir).join(name).is_file())
}

/// Strip a leading "./" and convert backslashes to forward slashes.
fn normalize_path(path: &str) -> String {
    let p = path.replace('\\', "/");
    p.strip_prefix("./").unwrap_or(&p).to_string()
}

/// Drop the trailing newline and cap the line at MAX_LINE_CHARS characters.
fn clip_text(text: &str) -> String {
    let t = text.trim_end_matches(['\r', '\n']);
    if t.len() <= MAX_LINE_CHARS {
        t.to_string()
    } else {
        t.chars().take(MAX_LINE_CHARS).collect()
    }
}

/// Parses one line of `rg --json` output. Only lines with "type":"match" carry a
/// result (begin/end/context/summary lines return None).
fn parse_rg_json_line(line: &str) -> Option<SearchMatch> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "match" {
        return None;
    }
    let data = v.get("data")?;
    let path = data.get("path")?.get("text")?.as_str()?;
    let line_number = data.get("line_number")?.as_u64()?;
    let text = data.get("lines")?.get("text")?.as_str()?;
    Some(SearchMatch {
        path: normalize_path(path),
        line: line_number,
        text: clip_text(text),
    })
}

/// Parses one `git grep -n` line of the form "path:line:text". Splits from the
/// left exactly twice so colons inside the matched text survive intact.
fn parse_git_grep_line(line: &str) -> Option<SearchMatch> {
    let mut parts = line.splitn(3, ':');
    let path = parts.next()?;
    let line_number = parts.next()?.parse::<u64>().ok()?;
    let text = parts.next()?;
    if path.is_empty() {
        return None;
    }
    Some(SearchMatch {
        path: normalize_path(path),
        line: line_number,
        text: clip_text(text),
    })
}

fn stderr_excerpt(stderr: &str) -> String {
    let t = stderr.trim();
    if t.len() <= 300 {
        t.to_string()
    } else {
        t.chars().take(300).collect()
    }
}

// ─── Child-process plumbing ──────────────────────────────────────────────────

/// Drain the child's stderr in the background so a chatty process can't block
/// on a full pipe; the task ends when the pipe closes (exit or kill).
fn drain_stderr(stderr: ChildStderr) -> tokio::task::JoinHandle<String> {
    tokio::spawn(async move {
        let mut stderr = stderr;
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf).await;
        buf
    })
}

/// Stream stdout lines through `parse`, stopping at `cap` results or the 15s
/// budget. Returns (matches, stopped_early); on early stop the child is killed,
/// and the caller must skip exit-code interpretation.
async fn collect_matches(
    child: &mut Child,
    parse: &mut (dyn FnMut(&str) -> Option<SearchMatch> + Send),
    cap: usize,
) -> Result<(Vec<SearchMatch>, bool), String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "child stdout was not captured".to_string())?;
    let mut matches: Vec<SearchMatch> = Vec::new();
    let mut capped = false;

    let read_loop = async {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(m) = parse(&line) {
                matches.push(m);
                if matches.len() >= cap {
                    capped = true;
                    break;
                }
            }
        }
    };

    let timed_out = tokio::time::timeout(TOTAL_TIMEOUT, read_loop).await.is_err();
    if timed_out || capped {
        let _ = child.kill().await;
    }
    Ok((matches, timed_out || capped))
}

// ─── Backends ────────────────────────────────────────────────────────────────

async fn run_rg(
    root: &Path,
    query: &str,
    opts: &SearchOpts,
    cap: usize,
) -> Result<SearchResult, String> {
    let mut cmd = Command::new("rg");
    cmd.arg("--json")
        .arg("--max-filesize")
        .arg(MAX_FILESIZE)
        .arg("--max-count")
        .arg(MAX_MATCHES_PER_FILE.to_string());
    if !opts.case_sensitive {
        cmd.arg("-i");
    }
    if let Some(glob) = &opts.glob {
        cmd.arg("--glob").arg(glob);
    }
    cmd.arg("--").arg(query).arg(".");
    cmd.current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| format!("failed to spawn rg: {e}"))?;
    let stderr_task = drain_stderr(
        child
            .stderr
            .take()
            .ok_or_else(|| "rg stderr was not captured".to_string())?,
    );

    let mut parse = |line: &str| parse_rg_json_line(line);
    let (matches, stopped_early) = collect_matches(&mut child, &mut parse, cap).await?;
    if stopped_early {
        stderr_task.abort();
        return Ok(SearchResult {
            matches,
            truncated: true,
            backend: "rg".into(),
        });
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("failed to wait on rg: {e}"))?;
    let stderr_text = stderr_task.await.unwrap_or_default();
    match status.code() {
        // 0 = matches found, 1 = no matches — both fine.
        Some(0) | Some(1) => Ok(SearchResult {
            matches,
            truncated: false,
            backend: "rg".into(),
        }),
        // 2 (or anything else) = real error.
        code => Err(format!(
            "rg failed (exit {code:?}): {}",
            stderr_excerpt(&stderr_text)
        )),
    }
}

async fn run_git_grep(
    root: &Path,
    query: &str,
    opts: &SearchOpts,
    cap: usize,
) -> Result<SearchResult, String> {
    let mut cmd = Command::new("git");
    cmd.arg("grep").arg("-n").arg("-I");
    if !opts.case_sensitive {
        cmd.arg("--ignore-case");
    }
    cmd.arg("-e").arg(query);
    if let Some(glob) = &opts.glob {
        cmd.arg("--").arg(glob);
    }
    cmd.current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn git: {e}"))?;
    let stderr_task = drain_stderr(
        child
            .stderr
            .take()
            .ok_or_else(|| "git grep stderr was not captured".to_string())?,
    );

    // git grep has no per-file cap flag, so enforce Orca's 100-per-file limit here.
    let mut per_file: HashMap<String, usize> = HashMap::new();
    let mut parse = |line: &str| {
        let m = parse_git_grep_line(line)?;
        let count = per_file.entry(m.path.clone()).or_insert(0);
        *count += 1;
        if *count > MAX_MATCHES_PER_FILE {
            return None;
        }
        Some(m)
    };

    let (matches, stopped_early) = collect_matches(&mut child, &mut parse, cap).await?;
    if stopped_early {
        stderr_task.abort();
        return Ok(SearchResult {
            matches,
            truncated: true,
            backend: "git-grep".into(),
        });
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("failed to wait on git grep: {e}"))?;
    let stderr_text = stderr_task.await.unwrap_or_default();
    match status.code() {
        // 0 = matches found, 1 = no matches — both fine.
        Some(0) | Some(1) => Ok(SearchResult {
            matches,
            truncated: false,
            backend: "git-grep".into(),
        }),
        Some(128) => Err("not a git repository and rg is not installed".to_string()),
        code => Err(format!(
            "git grep failed (exit {code:?}): {}",
            stderr_excerpt(&stderr_text)
        )),
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

pub async fn text_search(
    root: &Path,
    query: &str,
    opts: &SearchOpts,
) -> Result<SearchResult, String> {
    let cap = opts
        .max_results
        .unwrap_or(MAX_TOTAL_RESULTS)
        .clamp(1, MAX_TOTAL_RESULTS);
    if binary_on_path("rg") {
        run_rg(root, query, opts, cap).await
    } else {
        run_git_grep(root, query, opts, cap).await
    }
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn search_text(
    root: String,
    query: String,
    opts: SearchOpts,
) -> Result<SearchResult, String> {
    let root_path = Path::new(&root);
    if !root_path.is_dir() {
        return Err(format!("root is not an existing directory: {root}"));
    }
    if query.is_empty() {
        let backend = if binary_on_path("rg") { "rg" } else { "git-grep" };
        return Ok(SearchResult {
            matches: Vec::new(),
            truncated: false,
            backend: backend.into(),
        });
    }
    text_search(root_path, &query, &opts).await
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rg_json_match_line() {
        let line = r#"{"type":"match","data":{"path":{"text":"./src/main.rs"},"lines":{"text":"fn main() {\n"},"line_number":3,"absolute_offset":20,"submatches":[{"match":{"text":"main"},"start":3,"end":7}]}}"#;
        let m = parse_rg_json_line(line).expect("should parse a match line");
        assert_eq!(m.path, "src/main.rs");
        assert_eq!(m.line, 3);
        assert_eq!(m.text, "fn main() {");
    }

    #[test]
    fn ignores_rg_json_non_match_lines() {
        let begin = r#"{"type":"begin","data":{"path":{"text":"./src/main.rs"}}}"#;
        assert!(parse_rg_json_line(begin).is_none());
        assert!(parse_rg_json_line("not json at all").is_none());
    }

    #[test]
    fn parses_git_grep_line() {
        let m = parse_git_grep_line("src/a.rs:42:fn main()").expect("should parse");
        assert_eq!(m.path, "src/a.rs");
        assert_eq!(m.line, 42);
        assert_eq!(m.text, "fn main()");
    }

    #[test]
    fn git_grep_line_keeps_extra_colons_in_text() {
        let m = parse_git_grep_line(r#"src/b.rs:7:let url = "http://localhost:8090/v1";"#)
            .expect("should parse");
        assert_eq!(m.path, "src/b.rs");
        assert_eq!(m.line, 7);
        assert_eq!(m.text, r#"let url = "http://localhost:8090/v1";"#);
    }

    #[test]
    fn rejects_malformed_git_grep_lines() {
        assert!(parse_git_grep_line("no separators here").is_none());
        assert!(parse_git_grep_line("path-only:not-a-number:text").is_none());
    }

    #[test]
    fn clips_long_lines_to_500_chars() {
        let long = "x".repeat(800);
        assert_eq!(clip_text(&long).chars().count(), 500);
        assert_eq!(clip_text("short\n"), "short");
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize_path("./src/a.rs"), "src/a.rs");
        assert_eq!(normalize_path(r"src\sub\a.rs"), "src/sub/a.rs");
    }
}
