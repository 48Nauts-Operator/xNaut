// Repo preflight — the checks that tell a user why a repo "doesn't work".
//
// On a fresh install the usual failures are a missing SSH key, no token, or read
// access without push. Git reports these late and vaguely, and the symptom
// surfaces somewhere unrelated (an agent fails to push three steps later). Each
// check here answers one question and, when it fails, says what to do about it.
//
// Read-only: nothing is created, uploaded or modified. `git push --dry-run`
// negotiates with the server and writes nothing.

use serde::Serialize;
use std::process::Command;
use std::time::Duration;

/// A row in the checklist. `Skipped` matters as much as `Fail`: if auth failed,
/// read and push are UNKNOWN, not broken, and colouring them red sends the user
/// chasing the wrong thing.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckRow {
    pub id: String,
    pub label: String,
    pub status: CheckStatus,
    /// What was actually found — a path, a URL, the git error.
    pub detail: String,
    /// What to do about it. Empty when the check passed.
    pub hint: String,
}

fn row(id: &str, label: &str, status: CheckStatus, detail: &str, hint: &str) -> CheckRow {
    CheckRow {
        id: id.into(),
        label: label.into(),
        status,
        detail: detail.into(),
        hint: hint.into(),
    }
}

/// Host portion of an ssh or https git URL, e.g.
/// `git@github-com-48Nauts:owner/repo.git` → `github-com-48Nauts`.
pub fn remote_host(url: &str) -> Option<String> {
    let u = url.trim();
    if let Some(rest) = u.strip_prefix("git@") {
        return rest.split(':').next().map(str::to_string);
    }
    if let Some(rest) = u.split("://").nth(1) {
        let host = rest.split('/').next()?;
        return Some(host.split('@').next_back()?.to_string());
    }
    None
}

pub(crate) fn git(args: &[&str], cwd: Option<&str>, timeout: Duration) -> (bool, String) {
    let mut cmd = Command::new("git");
    cmd.args(args);
    // Never let git stop on an interactive credential or host-key prompt: a
    // hung check is worse than a failed one.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new");
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    match crate::repo_check::run_with_timeout(cmd, timeout) {
        Some(out) => {
            let ok = out.status.success();
            let text = if ok {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            } else {
                String::from_utf8_lossy(&out.stderr).trim().to_string()
            };
            (ok, text)
        }
        None => (false, format!("timed out after {}s", timeout.as_secs())),
    }
}

/// Like `git`, but WITHOUT trimming stdout.
///
/// `git status --porcelain` puts a two-character status column first, so an
/// unmodified-but-unstaged file reads " M path". Trimming eats that leading
/// space, every path shifts one byte left, and `line[3..]` silently yields
/// ".txt" instead of "a.txt" — a parse bug that looks like "no dirty files".
fn git_untrimmed(args: &[&str], cwd: &str, timeout: Duration) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(cwd);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    let out = run_with_timeout(cmd, timeout)?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs a command, killing it if it outlives `timeout`.
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Option<std::process::Output> {
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
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Runs the preflight. `path` is an optional local checkout; `url` an optional
/// remote. Both absent is valid — the user skipped the repo question — and
/// yields an empty list rather than a wall of failures.
#[tauri::command]
pub async fn repo_preflight(
    path: Option<String>,
    url: Option<String>,
    kind: Option<String>,
) -> Result<Vec<CheckRow>, String> {
    let mut rows = Vec::new();
    let path = path.filter(|p| !p.trim().is_empty());
    let mut url = url.filter(|u| !u.trim().is_empty());

    // 1. Local folder + git repo.
    if let Some(p) = &path {
        let exists = std::path::Path::new(p).is_dir();
        rows.push(row(
            "folder",
            "Folder exists",
            if exists { CheckStatus::Pass } else { CheckStatus::Fail },
            p,
            if exists { "" } else { "Create it, or correct the path." },
        ));
        if exists {
            let is_repo = std::path::Path::new(p).join(".git").exists();
            rows.push(row(
                "gitrepo",
                "Is a git repository",
                if is_repo { CheckStatus::Pass } else { CheckStatus::Fail },
                if is_repo { ".git found" } else { "no .git directory" },
                if is_repo { "" } else { "Run `git init` in the folder, or pick a different one." },
            ));
            // Adopt the folder's own origin when no URL was supplied.
            if url.is_none() && is_repo {
                let (ok, out) = git(&["remote", "get-url", "origin"], Some(p), Duration::from_secs(5));
                if ok && !out.is_empty() {
                    url = Some(out);
                }
            }
        }
    }

    let Some(url) = url else {
        rows.push(row(
            "remote",
            "Remote configured",
            CheckStatus::Skipped,
            "no remote given",
            "Local-only project — nothing to check. Add a remote later if you want one.",
        ));
        return Ok(rows);
    };
    rows.push(row("remote", "Remote configured", CheckStatus::Pass, &url, ""));

    // 2. Does the host in the URL resolve to something? An ssh alias only exists
    //    in ~/.ssh/config, so a URL that works here can fail on another machine.
    if let Some(host) = remote_host(&url) {
        let cfg = dirs::home_dir()
            .map(|h| h.join(".ssh/config"))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let aliased = cfg
            .lines()
            .any(|l| l.trim_start().to_lowercase().starts_with("host ") && l.contains(&host));
        let dotted = host.contains('.');
        let known = aliased || dotted;
        rows.push(row(
            "host",
            "Host resolves",
            if known { CheckStatus::Pass } else { CheckStatus::Fail },
            &if aliased {
                format!("{host} (alias in ~/.ssh/config)")
            } else {
                host.clone()
            },
            if known {
                ""
            } else {
                "Not a domain and no matching Host entry in ~/.ssh/config — this URL will not resolve on another machine."
            },
        ));
    }

    // 3. Read access. The single most informative check: it distinguishes
    //    unknown host, auth failure and missing repo.
    let (read_ok, read_out) = git(&["ls-remote", "--heads", &url], None, Duration::from_secs(20));
    let hint = if read_ok {
        String::new()
    } else {
        let e = read_out.to_lowercase();
        if e.contains("permission denied") || e.contains("publickey") {
            "SSH key not accepted. Add a key for this host (~/.ssh/config), or use an https URL with a token.".into()
        } else if e.contains("could not resolve") || e.contains("name or service") {
            "Host unreachable — check the URL, DNS, or whether it needs a VPN/Tailscale.".into()
        } else if e.contains("not found") || e.contains("does not exist") {
            "Repository not found — check the path, or whether you have access to it.".into()
        } else if e.contains("authentication failed") || e.contains("401") {
            "Authentication failed — the token is missing, wrong, or expired.".into()
        } else {
            "Could not read from the remote.".into()
        }
    };
    rows.push(row(
        "read",
        "Read access",
        if read_ok { CheckStatus::Pass } else { CheckStatus::Fail },
        if read_ok { "ls-remote succeeded" } else { &read_out },
        &hint,
    ));

    // 4. Push access — separate on purpose. Read working and push denied is the
    //    single most common "it doesn't work", and one combined row hides it.
    if read_ok {
        let (push_ok, push_out) = git(
            &["push", "--dry-run", "--porcelain", &url, "HEAD"],
            path.as_deref(),
            Duration::from_secs(25),
        );
        rows.push(row(
            "push",
            "Push access",
            if push_ok { CheckStatus::Pass } else { CheckStatus::Fail },
            if push_ok { "dry-run accepted" } else { &push_out },
            if push_ok {
                ""
            } else {
                "Read works but push was refused — you likely lack write permission on this repository."
            },
        ));
    } else {
        rows.push(row(
            "push",
            "Push access",
            CheckStatus::Skipped,
            "not checked — read failed first",
            "",
        ));
    }

    // 5. Token for this forge kind, when one was named.
    if let Some(kind) = kind.filter(|k| !k.trim().is_empty() && k != "none") {
        let settings = crate::settings::load_or_default();
        let has = settings
            .forges
            .iter()
            .any(|f| f.kind.eq_ignore_ascii_case(&kind) && f.token.as_deref().is_some_and(|t| !t.trim().is_empty()));
        let forgejo_fallback = kind.eq_ignore_ascii_case("forgejo")
            && dirs::home_dir()
                .map(|h| h.join(".config/forgejo/token").exists())
                .unwrap_or(false);
        let ok = has || forgejo_fallback;
        rows.push(row(
            "token",
            &format!("Token for {kind}"),
            if ok { CheckStatus::Pass } else { CheckStatus::Skipped },
            if has {
                "configured in Settings"
            } else if forgejo_fallback {
                "~/.config/forgejo/token"
            } else {
                "none configured"
            },
            if ok {
                ""
            } else {
                "Only needed for creating repos or issues over the API — pushing over SSH does not use it."
            },
        ));
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_host_from_both_url_shapes() {
        assert_eq!(remote_host("git@github.com:o/r.git").as_deref(), Some("github.com"));
        // The alias case — this is exactly what breaks on another machine.
        assert_eq!(
            remote_host("git@github-com-48Nauts:48Nauts-Operator/xNaut.git").as_deref(),
            Some("github-com-48Nauts")
        );
        assert_eq!(
            remote_host("https://user@gitlab.com/o/r.git").as_deref(),
            Some("gitlab.com")
        );
        assert_eq!(
            remote_host("http://cosmos.tail138398.ts.net:3000/o/r.git").as_deref(),
            Some("cosmos.tail138398.ts.net:3000")
        );
        assert_eq!(remote_host("nonsense"), None);
    }

    #[tokio::test]
    async fn skipping_the_repo_question_is_not_a_failure() {
        let rows = repo_preflight(None, None, None).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, CheckStatus::Skipped);
    }
}

/// Facts a project page can show without inventing anything: when the code was
/// last touched, how much is uncommitted, and how many worktrees are attached.
/// Every field is optional — absent beats a fabricated default.
///
/// XNAUT-341: an absent field also carries WHY it is absent. A dash on its own
/// is indistinguishable from a number still loading, and the header is read to
/// answer "is this project moving?". "not a git repository" answers that; a
/// dash does not. `unavailable` is the reason the three git fields below are
/// missing; it is `None` exactly when they were read.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectFacts {
    pub is_repo: bool,
    pub branch: Option<String>,
    /// Uncommitted entries from `git status --porcelain`.
    pub changes: Option<usize>,
    /// Linked worktrees beyond the main checkout.
    pub worktrees: Option<usize>,
    /// Author time of the most recent commit, epoch ms.
    pub last_commit_ms: Option<i64>,
    /// Why the git fields are absent, phrased for a reader, e.g. "not a git
    /// repository". `None` when they were read.
    pub unavailable: Option<String>,
    /// A real repository that has never been committed to. Distinguishes "there
    /// has been no commit" from "the last commit could not be read".
    pub no_commits: bool,
}

#[tauri::command]
pub fn project_facts(path: String) -> Result<ProjectFacts, String> {
    let dir = std::path::Path::new(&path);
    let mut f = ProjectFacts::default();
    if !dir.is_dir() {
        f.unavailable = Some(if dir.exists() {
            "not a folder".into()
        } else {
            "the folder is not on this machine".into()
        });
        return Ok(f);
    }
    let short = Duration::from_secs(5);
    // `git rev-parse` rather than a `.git` probe: a linked worktree's `.git` is
    // a file and a project rooted at a SUBDIRECTORY of a checkout has no `.git`
    // at all, yet both are inside a repository. Probing the entry answered "not
    // a git repository" for both, which was a tolerable dash and is an actively
    // wrong sentence now that the reason is on screen.
    let (ok, out) = git(&["rev-parse", "--is-inside-work-tree"], Some(&path), short);
    f.is_repo = ok && out.trim() == "true";
    if !f.is_repo {
        // Tell the two apart: git missing from the machine is not the same
        // problem as a folder that is not tracked, and they need different
        // fixes. The stderr text is deliberately NOT quoted into the reason;
        // `git()` reports a failure to spawn as "timed out", so interpolating
        // it would print a confident wrong sentence on a machine without git.
        f.unavailable = Some(if ok || out.contains("not a git repository") {
            "not a git repository".into()
        } else {
            "git could not be read here".into()
        });
        return Ok(f);
    }
    let (ok, out) = git(&["rev-parse", "--abbrev-ref", "HEAD"], Some(&path), short);
    if ok && !out.is_empty() {
        f.branch = Some(out);
    }
    let (ok, out) = git(&["status", "--porcelain"], Some(&path), short);
    if ok {
        f.changes = Some(out.lines().filter(|l| !l.trim().is_empty()).count());
    }
    // `git worktree list` always includes the main checkout, so subtract it.
    let (ok, out) = git(&["worktree", "list", "--porcelain"], Some(&path), short);
    if ok {
        let n = out.lines().filter(|l| l.starts_with("worktree ")).count();
        f.worktrees = Some(n.saturating_sub(1));
    }
    let (ok, out) = git(&["log", "-1", "--format=%at"], Some(&path), short);
    if ok {
        if let Ok(secs) = out.trim().parse::<i64>() {
            f.last_commit_ms = Some(secs * 1000);
        }
    } else if out.contains("does not have any commits") {
        // A freshly-initialised checkout. A dash under "Last commit" reads as
        // a number that failed to load; the repository is real, and the answer
        // is that there has not been one yet.
        f.no_commits = true;
    }
    Ok(f)
}

/// Last real activity per project, for the sidebar.
///
/// "Activity" means *someone touched this project*, which is deliberately NOT
/// the same as its last commit. Nautravel had an agent editing 26 files while
/// its newest commit was nine days old; reporting "9d ago" was true about
/// commits and a lie about activity, which is what the row claims to show.
///
/// It is also not zellij's `last_active_ms`: that reads the mtime of the
/// resurrection cache, which zellij rewrites roughly once a second whether the
/// agent is thinking or asleep, so everything would read "just now".
///
/// So: the newest of the last commit and the most recently modified file in the
/// working tree. Uncommitted work is exactly the case that was wrong, and it is
/// the case that means work is happening right now.
///
/// Bounded on purpose — `git status --porcelain` respects .gitignore (so
/// node_modules and target are already out), and only the first
/// MAX_STATTED_FILES entries are stat'ed. A project with thousands of dirty
/// files still answers quickly, and it only needs the newest, which any
/// reasonable sample of a working session will contain.
#[tauri::command]
pub fn projects_activity(paths: Vec<String>) -> Vec<Option<i64>> {
    /// Enough to catch the file being edited right now without stat-ing a
    /// generated-file explosion.
    const MAX_STATTED_FILES: usize = 200;

    paths
        .into_iter()
        .map(|path| {
            let dir = std::path::Path::new(&path);
            if path.trim().is_empty() || !dir.is_dir() || !dir.join(".git").exists() {
                return None;
            }
            let short = Duration::from_secs(5);

            let commit_ms = {
                let (ok, out) = git(&["log", "-1", "--format=%ct"], Some(&path), short);
                if ok {
                    out.trim().parse::<i64>().ok().map(|secs| secs * 1000)
                } else {
                    None
                }
            };

            let dirty_ms = {
                match git_untrimmed(&["status", "--porcelain"], &path, short) {
                    None => None,
                    Some(out) => out
                        .lines()
                        .filter_map(|line| {
                            // "XY <path>", and renames are "XY <old> -> <new>";
                            // the new name is the one that exists on disk.
                            let rest = line.get(3..)?.trim();
                            let name = rest.rsplit(" -> ").next()?.trim_matches('"');
                            let full = dir.join(name);
                            std::fs::metadata(&full).ok()?.modified().ok()
                        })
                        .take(MAX_STATTED_FILES)
                        .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .max(),
                }
            };

            match (commit_ms, dirty_ms) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(a), None) => Some(a),
                (None, b) => b,
            }
        })
        .collect()
}

#[cfg(test)]
mod activity_tests {
    use super::*;

    #[test]
    fn a_non_repo_reports_nothing_rather_than_a_fake_time() {
        let out = projects_activity(vec![
            String::new(),
            "/nonexistent/xnaut-activity".into(),
            std::env::temp_dir().to_string_lossy().into(),
        ]);
        assert_eq!(out, vec![None, None, None]);
    }

    /// The bug this replaced: a project whose newest COMMIT is old but whose
    /// files are being edited right now must report "now", not the commit date.
    #[test]
    fn uncommitted_edits_count_as_activity() {
        let dir = std::env::temp_dir().join(format!("xnaut-act-dirty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str], old: bool| {
            let mut c = std::process::Command::new("git");
            c.args(args).current_dir(&dir);
            if old {
                // Backdate the commit so "last commit" is provably not the answer.
                c.env("GIT_AUTHOR_DATE", "2020-01-01T00:00:00Z");
                c.env("GIT_COMMITTER_DATE", "2020-01-01T00:00:00Z");
            }
            let _ = c.output();
        };
        run(&["init", "-q"], false);
        run(&["config", "user.email", "t@example.com"], false);
        run(&["config", "user.name", "t"], false);
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        run(&["add", "-A"], false);
        run(&["commit", "-qm", "old"], true);

        // Now edit without committing — the Nautravel case.
        std::fs::write(dir.join("a.txt"), "edited just now").unwrap();

        let out = projects_activity(vec![dir.to_string_lossy().into()]);
        let _ = std::fs::remove_dir_all(&dir);
        let ms = out[0].expect("a dirty repo must report a time");
        let now = chrono::Utc::now().timestamp_millis();
        assert!(
            (now - ms).abs() < 120_000,
            "reported the commit date instead of the edit: {ms} vs now {now}"
        );
    }

    #[test]
    fn reads_the_commit_time_from_a_real_repo() {
        let dir = std::env::temp_dir().join(format!("xnaut-act-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let _ = std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output();
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@example.com"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-qm", "one"]);

        let out = projects_activity(vec![dir.to_string_lossy().into()]);
        let _ = std::fs::remove_dir_all(&dir);
        let ms = out[0].expect("a committed repo must report a time");
        let now = chrono::Utc::now().timestamp_millis();
        assert!((now - ms).abs() < 120_000, "commit time should be ~now");
    }
}

/// XNAUT-341. The header shows these four numbers, so every absence it can
/// produce has to carry a sentence a reader can act on. A dash with no reason
/// is the failure being fixed, and the assertions below are all about which
/// sentence comes back rather than about the numbers.
#[cfg(test)]
mod facts_tests {
    use super::*;

    /// `git init` plus an identity, in a directory of our own.
    fn scratch_repo(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("xnaut-facts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@example.com"][..],
            &["config", "user.name", "t"][..],
        ] {
            let _ = std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output();
        }
        dir
    }

    #[test]
    fn a_folder_that_is_not_a_repo_says_so() {
        let dir = std::env::temp_dir().join(format!("xnaut-facts-plain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let f = project_facts(dir.to_string_lossy().into()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!f.is_repo);
        assert_eq!(f.unavailable.as_deref(), Some("not a git repository"));
        // The reason replaces the numbers; it does not invent them.
        assert_eq!(f.changes, None);
        assert_eq!(f.worktrees, None);
        assert_eq!(f.last_commit_ms, None);
    }

    #[test]
    fn a_path_that_is_not_on_this_machine_says_that_instead() {
        let f = project_facts("/nonexistent/xnaut-341-facts".into()).unwrap();
        assert!(!f.is_repo);
        assert_eq!(
            f.unavailable.as_deref(),
            Some("the folder is not on this machine")
        );
    }

    /// The reason a `.git` probe was not good enough: this directory has no
    /// `.git` entry of its own and is still inside a repository.
    #[test]
    fn a_subdirectory_of_a_checkout_is_a_repo() {
        let dir = scratch_repo("subdir");
        let nested = dir.join("src").join("deep");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        for args in [&["add", "-A"][..], &["commit", "-qm", "one"][..]] {
            let _ = std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output();
        }

        assert!(!nested.join(".git").exists(), "fixture must have no .git");
        let f = project_facts(nested.to_string_lossy().into()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(f.is_repo, "a subdirectory of a checkout is inside the repo");
        assert_eq!(f.unavailable, None);
        assert!(f.last_commit_ms.is_some());
    }

    /// A repository with no commits: the numbers are readable, the commit is
    /// not, and the two absences have different causes.
    #[test]
    fn a_repo_with_no_commits_is_not_a_repo_that_could_not_be_read() {
        let dir = scratch_repo("empty");

        let f = project_facts(dir.to_string_lossy().into()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(f.is_repo);
        assert_eq!(f.unavailable, None, "the repo itself was readable");
        assert!(f.no_commits, "an empty repo has never been committed to");
        assert_eq!(f.last_commit_ms, None);
        assert_eq!(f.changes, Some(0));
    }

    #[test]
    fn a_committed_repo_reports_every_number_and_no_reason() {
        let dir = scratch_repo("full");
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        for args in [&["add", "-A"][..], &["commit", "-qm", "one"][..]] {
            let _ = std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output();
        }
        // One uncommitted file, so `changes` is provably counted rather than zero.
        std::fs::write(dir.join("b.txt"), "y").unwrap();

        let f = project_facts(dir.to_string_lossy().into()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(f.is_repo);
        assert_eq!(f.unavailable, None);
        assert!(!f.no_commits);
        assert_eq!(f.changes, Some(1));
        assert_eq!(f.worktrees, Some(0), "the main checkout is not a worktree");
        assert!(f.last_commit_ms.is_some());
    }
}
