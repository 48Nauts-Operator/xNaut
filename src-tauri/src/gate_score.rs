// Scoring the acceptance gate, rather than only asking whether it passed.
//
// `95-Build-Gate.py` already emits one line per check — "PASS: <verified>" or
// "FAIL: expected X, found Y — fix: <instruction>" — and exits 0 only if all
// pass. Until now only the exit code was read, which throws away everything
// interesting: a verdict cannot plateau. "Stuck at 4 failing checks" and "stuck
// at 40" are the same boolean, so a manager watching it cannot tell an agent
// that is grinding forward from one that is going in circles.
//
// Counting the lines turns the same output into a number, and a number can be
// compared against the previous number. That is the whole idea.
//
// The gate is run against a DETACHED WORKTREE at a specific commit, borrowed
// from CORAL (Apache 2.0) — `coral/grader/daemon.py` grades inside
// `git worktree add --detach <commit>` so the agent's ongoing commits cannot
// perturb the tree being graded. Scoring the live worktree instead would race
// every save the agent makes, and the score would measure the moment it was
// taken rather than the commit it claims to describe.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// One gate run, reduced to numbers.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct GateScore {
    pub passed: usize,
    pub failed: usize,
    pub total: usize,
    /// passed / total, in 0.0..=1.0. `None` when the gate produced no check
    /// lines at all — that is "no measurement", which must not be confused with
    /// a measured zero. A crashed gate scoring 0.0 would look like real, terrible
    /// progress and would reset a plateau streak that should have kept counting.
    pub score: Option<f64>,
    /// The FAIL lines, verbatim, so a nudge can quote what is actually broken
    /// instead of saying "it failed".
    pub failures: Vec<String>,
    /// Exit status of the gate itself, when it ran.
    pub exit_code: Option<i32>,
    /// Set when the gate could not be run at all (absent, timed out, no commit).
    pub error: Option<String>,
}

/// Counts `PASS:` / `FAIL:` lines. Tolerates leading whitespace and bullet
/// markers, because the gate is written by a model and drifts: "- [PASS] x" and
/// "PASS: x" both appear in practice, and a parser that only accepts one of them
/// silently scores every check as missing.
pub fn parse_gate_output(output: &str) -> GateScore {
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures = Vec::new();

    for raw in output.lines() {
        let line = raw.trim();
        let bare = line
            .trim_start_matches(['-', '*', '•'])
            .trim_start()
            .trim_start_matches('[');
        let upper = bare.to_uppercase();
        if upper.starts_with("PASS:") || upper.starts_with("PASS]") {
            passed += 1;
        } else if upper.starts_with("FAIL:") || upper.starts_with("FAIL]") {
            failed += 1;
            failures.push(line.to_string());
        }
    }

    let total = passed + failed;
    GateScore {
        passed,
        failed,
        total,
        score: if total == 0 {
            None
        } else {
            Some(passed as f64 / total as f64)
        },
        failures,
        exit_code: None,
        error: None,
    }
}

/// Where the Validator actually writes the gate.
///
/// This was wrong on first release and the failure was silent, which is the
/// interesting part: the gate lives with the NAUT-Flow documents **in the
/// vault**, not in the product repo —
/// `~/.xnaut-vault/work/<project>/Development/NAUT-Flow/95-Build-Gate.py` — a
/// completely different tree. Searching only the repo found nothing on every
/// real project, reported "no gate", and the guardian fell back to its wall
/// clock. Nothing errored; the scoring simply never happened.
fn vault_gate(project: &str) -> Option<PathBuf> {
    let slug: String = project
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .filter(|c| c.is_ascii_alphanumeric() || "._- ".contains(*c))
        .collect();
    let p = dirs::home_dir()?
        .join(".xnaut-vault")
        .join("work")
        .join(slug.trim_matches(['-', '.']))
        .join("Development")
        .join("NAUT-Flow")
        .join("95-Build-Gate.py");
    p.is_file().then_some(p)
}

/// The Validator's gate, beside the repo or one level down.
fn find_gate(dir: &Path) -> Option<PathBuf> {
    const GATE: &str = "95-Build-Gate.py";
    let direct = dir.join(GATE);
    if direct.is_file() {
        return Some(direct);
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let candidate = entry.path().join(GATE);
        if entry.path().is_dir() && candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn run_capture(mut cmd: Command, timeout: Duration) -> Option<std::process::Output> {
    use std::io::Read;
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut o) = child.stdout.take() {
                    let _ = o.read_to_end(&mut stdout);
                }
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_end(&mut stderr);
                }
                return Some(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

fn git(args: &[&str], cwd: &Path, timeout: Duration) -> (bool, String) {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(cwd);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    match run_capture(cmd, timeout) {
        Some(out) => {
            let ok = out.status.success();
            let text = if ok {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            } else {
                String::from_utf8_lossy(&out.stderr).trim().to_string()
            };
            (ok, text)
        }
        None => (false, "git timed out".into()),
    }
}

fn err(message: String) -> GateScore {
    GateScore {
        error: Some(message),
        ..Default::default()
    }
}

/// Scores the gate for `repo_path` at its current HEAD commit.
///
/// The gate runs in a throwaway detached worktree of that commit, so a score
/// always describes a committed state and never a half-saved file. The worktree
/// is removed afterwards whether or not the gate succeeded — leaving them behind
/// would slowly fill `git worktree list` with junk the user has to prune.
/// `project` lets the vault be searched when the repo has no gate of its own,
/// which is the normal case — the Validator writes it beside the NAUT-Flow
/// documents.
#[tauri::command]
pub async fn gate_score_run(repo_path: String, project: Option<String>) -> Result<GateScore, String> {
    tauri::async_runtime::spawn_blocking(move || {
        score_at_head(Path::new(&repo_path), project.as_deref())
    })
    .await
    .map_err(|e| format!("gate scoring panicked: {e}"))
}

fn score_at_head(repo: &Path, project: Option<&str>) -> GateScore {
    if !repo.is_dir() {
        return err(format!("{} is not a directory", repo.display()));
    }
    // In the repo if it happens to be there; otherwise the vault.
    let external = match find_gate(repo) {
        Some(_) => None,
        None => match project.and_then(vault_gate) {
            Some(p) => Some(p),
            None => {
                // Not an error: a project with no gate is normal, and the caller
                // falls back to a wall clock. "No gate" and "scored zero" are
                // different facts.
                return err("no 95-Build-Gate.py in the repo or the vault".into());
            }
        },
    };
    let (ok, head) = git(&["rev-parse", "HEAD"], repo, Duration::from_secs(10));
    if !ok || head.is_empty() {
        return err(format!("could not resolve HEAD: {head}"));
    }

    let dest = std::env::temp_dir().join(format!("xnaut-gate-{}", &head[..head.len().min(12)]));
    // A leftover from an interrupted earlier run would make `worktree add` fail.
    let _ = std::fs::remove_dir_all(&dest);
    let dest_str = dest.to_string_lossy().to_string();
    let (added, add_err) = git(
        &["worktree", "add", "--detach", &dest_str, &head],
        repo,
        Duration::from_secs(60),
    );
    if !added {
        return err(format!("git worktree add failed: {add_err}"));
    }

    let result = run_gate_in(&dest, external.as_deref());

    // Always clean up: --force because the gate may have written into the tree.
    let _ = git(
        &["worktree", "remove", "--force", &dest_str],
        repo,
        Duration::from_secs(30),
    );
    let _ = std::fs::remove_dir_all(&dest);
    result
}

/// Runs the gate with `dir` as the working directory — the built product — even
/// when the script itself lives outside it, which is the vault case.
fn run_gate_in(dir: &Path, external: Option<&Path>) -> GateScore {
    let gate = match external {
        Some(p) => p.to_path_buf(),
        None => match find_gate(dir) {
            Some(p) => p,
            None => return err("gate vanished from the checkout".into()),
        },
    };
    let gate_str = gate.to_string_lossy().to_string();

    // uv first: the Validator writes the gate with a PEP 723 header, so its deps
    // are declared inline and there is nothing to install. python3 is the
    // fallback for a stdlib-only gate on a machine without uv.
    let mut attempts: Vec<(&str, Vec<String>)> = Vec::new();
    attempts.push(("uv", vec!["run".into(), gate_str.clone()]));
    attempts.push(("python3", vec![gate_str.clone()]));

    let mut last_error = String::new();
    for (program, args) in attempts {
        let mut cmd = Command::new(program);
        cmd.args(&args).current_dir(dir);
        // Homebrew and ~/.local/bin: a bundled app inherits a minimal PATH, so
        // uv would be "not found" even where it is installed.
        if let Some(path) = std::env::var_os("PATH") {
            let mut joined = std::ffi::OsString::from("/opt/homebrew/bin:/usr/local/bin:");
            if let Some(home) = dirs::home_dir() {
                joined.push(home.join(".local/bin").as_os_str());
                joined.push(":");
            }
            joined.push(path);
            cmd.env("PATH", joined);
        }
        let Some(out) = run_capture(cmd, Duration::from_secs(600)) else {
            last_error = format!("{program} timed out after 600s");
            continue;
        };
        let mut text = String::from_utf8_lossy(&out.stdout).to_string();
        text.push('\n');
        text.push_str(&String::from_utf8_lossy(&out.stderr));

        let mut score = parse_gate_output(&text);
        if score.total == 0 {
            // Ran, but said nothing we understand — try the next interpreter
            // rather than reporting "no checks" as if the gate had no work.
            last_error = format!("{program} produced no PASS/FAIL lines");
            continue;
        }
        score.exit_code = out.status.code();
        return score;
    }
    err(if last_error.is_empty() {
        "could not run the gate".into()
    } else {
        last_error
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_the_gates_own_format() {
        let out = "PASS: index.html exists with real content\n\
                   FAIL: expected /api/health 200, found 404 — fix: register the route\n\
                   PASS: npm test exits 0\n";
        let s = parse_gate_output(out);
        assert_eq!((s.passed, s.failed, s.total), (2, 1, 3));
        assert_eq!(s.score, Some(2.0 / 3.0));
        assert_eq!(s.failures.len(), 1);
        assert!(s.failures[0].contains("/api/health"));
    }

    #[test]
    fn tolerates_the_bulleted_variant_the_model_also_writes() {
        // The Validator prompt shows "- [PASS] x" for the report and "PASS: x"
        // for the gate; gates in the wild use both. Accepting only one silently
        // scores every check as missing.
        let s = parse_gate_output("- [PASS] one\n  - [FAIL] two\n* [PASS] three\n");
        assert_eq!((s.passed, s.failed, s.total), (2, 1, 3));
    }

    #[test]
    fn no_check_lines_is_no_measurement_not_a_zero() {
        // A crashed gate must not look like real, terrible progress: scoring it
        // 0.0 would reset a plateau streak that ought to keep counting.
        let s = parse_gate_output("Traceback (most recent call last):\n  ImportError\n");
        assert_eq!(s.total, 0);
        assert_eq!(s.score, None);
    }

    #[test]
    fn all_passing_scores_one() {
        let s = parse_gate_output("PASS: a\nPASS: b\n");
        assert_eq!(s.score, Some(1.0));
        assert!(s.failures.is_empty());
    }

    #[test]
    fn prose_mentioning_failure_is_not_counted_as_a_check() {
        let s = parse_gate_output("Checking whether the build will FAIL: soon\nPASS: real check\n");
        // The first line starts with "Checking", not a marker — only the second
        // is a check. (A line genuinely starting "FAIL:" is still counted.)
        assert_eq!((s.passed, s.failed), (1, 0));
    }

    /// The whole path, against a real git repo: the gate must be scored from the
    /// COMMIT, not the working tree. The test proves that by committing a gate
    /// that passes, then dirtying the working copy so it would fail if the live
    /// tree were scored.
    #[test]
    fn scores_the_commit_and_ignores_uncommitted_edits() {
        let dir = std::env::temp_dir().join(format!("xnaut-gate-it-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let ok = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            assert!(ok, "git {args:?} failed");
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@example.com"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(
            dir.join("95-Build-Gate.py"),
            "print('PASS: committed check one')\nprint('PASS: committed check two')\n",
        )
        .unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-qm", "gate"]);

        // Dirty the working copy: if scoring read the live tree, this would win.
        std::fs::write(
            dir.join("95-Build-Gate.py"),
            "print('FAIL: uncommitted edit should not be scored')\n",
        )
        .unwrap();

        let s = score_at_head(&dir, None);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(s.error.is_none(), "unexpected error: {:?}", s.error);
        assert_eq!(
            (s.passed, s.failed),
            (2, 0),
            "scored the working tree instead of the commit"
        );
        assert_eq!(s.score, Some(1.0));
    }

    /// The bug this fixes, against the real vault. WebBuilder has a gate at
    /// ~/.xnaut-vault/work/WebBuilder/Development/NAUT-Flow/95-Build-Gate.py and
    /// nothing in its repo — the exact case that silently reported "no gate".
    #[test]
    #[ignore]
    fn finds_the_real_vault_gate() {
        assert!(
            vault_gate("WebBuilder").is_some(),
            "WebBuilder's gate lives in the vault and must be found"
        );
        assert!(vault_gate("no-such-project-xnaut").is_none());
        // A project name cannot escape the vault.
        assert!(vault_gate("../../etc").is_none());
    }

    #[test]
    fn missing_repo_reports_an_error_rather_than_a_score() {
        let s = score_at_head(Path::new("/nonexistent/xnaut-gate-test"), None);
        assert!(s.error.is_some());
        assert_eq!(s.score, None);
    }
}
