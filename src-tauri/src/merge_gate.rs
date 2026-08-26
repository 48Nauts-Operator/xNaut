// The merge with a gate (XNAUT-173 item 4): the joint that lands an agent's
// finished ticket, and refuses when it should not land. A self-building
// system without a refusing merge eventually destroys its own repository.
//
// The gate is a deterministic risk score over the diff, not a model's
// opinion: a scorer that reads the diff can be argued with by the diff
// ("this change is low risk"), a line count cannot. Bands, per André
// 2026-08-26: score >= 8 is high risk and needs a human approval from the
// Mesh inbox; below that NautBot's own deliberate merge_ticket call is the
// approval. Every merge is --no-ff, so the safeguard is one revert of the
// merge commit (unmerge_ticket).

use std::path::Path;
use std::process::Command;

/// One file's numstat row.
#[derive(Debug, Clone, PartialEq)]
pub struct FileStat {
    pub path: String,
    pub added: u32,
    pub removed: u32,
}

/// `git diff --numstat` output. Binary files report "-" and count as 0/0;
/// their path still participates in the sensitivity check.
pub fn parse_numstat(output: &str) -> Vec<FileStat> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let added = parts.next()?.trim();
            let removed = parts.next()?.trim();
            let path = parts.next()?.trim();
            if path.is_empty() {
                return None;
            }
            Some(FileStat {
                path: path.to_string(),
                added: added.parse().unwrap_or(0),
                removed: removed.parse().unwrap_or(0),
            })
        })
        .collect()
}

/// Paths where a wrong line breaks more than the feature: the ACL, the
/// release pipeline, app wiring, dependencies, settings and secrets.
fn is_sensitive(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("permissions/")
        || p.contains("capabilities/")
        || p.contains(".github/")
        || p.ends_with("tauri.conf.json")
        || p.ends_with("cargo.toml")
        || p.ends_with("main.rs")
        || p.ends_with("state.rs")
        || p.ends_with("settings.rs")
        || p.contains("secret")
}

fn is_testish(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("test") || p.contains("spec")
}

#[derive(Debug, serde::Serialize)]
pub struct Risk {
    /// 0..=10.
    pub score: u8,
    /// "low" | "medium" | "high"
    pub band: &'static str,
    /// One line per signal that fired, so the refusal can say WHY.
    pub reasons: Vec<String>,
}

/// A score of this or more needs a human approval from the Mesh inbox.
pub const HUMAN_APPROVAL_AT: u8 = 8;

/// Deterministic risk score for a candidate merge. `verified` is whether a
/// passing verify record exists for the ticket; its absence is a risk signal
/// rather than a hard refusal, because most tickets have no verify plan yet.
pub fn risk_score(files: &[FileStat], verified: bool) -> Risk {
    let mut score: u32 = 0;
    let mut reasons = Vec::new();
    let total: u32 = files.iter().map(|f| f.added + f.removed).sum();
    match total {
        t if t > 500 => {
            score += 3;
            reasons.push(format!("{t} lines changed (+3)"));
        }
        t if t > 150 => {
            score += 2;
            reasons.push(format!("{t} lines changed (+2)"));
        }
        t if t > 50 => {
            score += 1;
            reasons.push(format!("{t} lines changed (+1)"));
        }
        _ => {}
    }
    if files.len() > 10 {
        score += 1;
        reasons.push(format!("{} files touched (+1)", files.len()));
    }
    let sensitive: Vec<&str> = files
        .iter()
        .filter(|f| is_sensitive(&f.path))
        .map(|f| f.path.as_str())
        .collect();
    if !sensitive.is_empty() {
        score += 3;
        reasons.push(format!("sensitive paths: {} (+3)", sensitive.join(", ")));
    }
    let test_deletion: Vec<&str> = files
        .iter()
        .filter(|f| is_testish(&f.path) && f.removed > f.added)
        .map(|f| f.path.as_str())
        .collect();
    if !test_deletion.is_empty() {
        score += 2;
        reasons.push(format!(
            "tests shrink: {} (+2)",
            test_deletion.join(", ")
        ));
    }
    if !files.is_empty() && !files.iter().any(|f| is_testish(&f.path)) {
        score += 1;
        reasons.push("no test file touched (+1)".to_string());
    }
    if !verified {
        score += 2;
        reasons.push("no passing verify record for this ticket (+2)".to_string());
    }
    let score = score.min(10) as u8;
    let band = match score {
        s if s >= HUMAN_APPROVAL_AT => "high",
        s if s >= 4 => "medium",
        _ => "low",
    };
    Risk {
        score,
        band,
        reasons,
    }
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Branches whose history mentions the ticket id, newest commit first,
/// excluding the branch we would merge into. Zero or several candidates come
/// back as data for the caller to resolve; guessing a branch is how the
/// wrong work lands.
pub fn candidate_branches(repo: &Path, ticket_id: &str) -> Result<Vec<String>, String> {
    let current = git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let commits = git(
        repo,
        &[
            "log",
            "--all",
            "--format=%H",
            "--regexp-ignore-case",
            &format!("--grep={ticket_id}"),
        ],
    )?;
    let mut branches: Vec<String> = Vec::new();
    for sha in commits.lines().take(20) {
        let containing = git(
            repo,
            &["branch", "--format=%(refname:short)", "--contains", sha],
        )?;
        for b in containing.lines() {
            let b = b.trim();
            if b.is_empty() || b == current || branches.iter().any(|x| x == b) {
                continue;
            }
            branches.push(b.to_string());
        }
    }
    Ok(branches)
}

#[derive(Debug)]
pub enum MergeOutcome {
    /// The merge commit's sha.
    Merged(String),
    /// Conflicting paths; the merge was aborted and the tree is clean again.
    Conflict(Vec<String>),
}

/// Merges `branch` into the repo's checked-out branch, --no-ff so one revert
/// undoes it. Refuses a dirty tree outright: merging over uncommitted work
/// destroys state that belongs to whoever left it there.
pub fn merge_branch(repo: &Path, branch: &str, message: &str) -> Result<MergeOutcome, String> {
    let dirty = git(repo, &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        return Err(format!(
            "the working tree at {} has uncommitted changes; merge refused. Commit or stash them first.",
            repo.display()
        ));
    }
    match git(repo, &["merge", "--no-ff", "-m", message, branch]) {
        Ok(_) => {
            let sha = git(repo, &["rev-parse", "HEAD"])?;
            Ok(MergeOutcome::Merged(sha))
        }
        Err(merge_error) => {
            let conflicts = git(repo, &["diff", "--name-only", "--diff-filter=U"])
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>();
            let _ = git(repo, &["merge", "--abort"]);
            if conflicts.is_empty() {
                Err(format!("merge failed: {merge_error}"))
            } else {
                Ok(MergeOutcome::Conflict(conflicts))
            }
        }
    }
}

/// The safeguard: reverts the newest merge commit whose message mentions the
/// ticket. One commit in, one commit out; nothing is rewritten, so the undo
/// is itself undoable.
pub fn revert_ticket_merge(repo: &Path, ticket_id: &str) -> Result<String, String> {
    let dirty = git(repo, &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        return Err(format!(
            "the working tree at {} has uncommitted changes; revert refused.",
            repo.display()
        ));
    }
    let sha = git(
        repo,
        &[
            "log",
            "--merges",
            "--regexp-ignore-case",
            &format!("--grep={ticket_id}"),
            "-1",
            "--format=%H",
        ],
    )?;
    if sha.is_empty() {
        return Err(format!("no merge commit mentions {ticket_id}"));
    }
    git(repo, &["revert", "-m", "1", "--no-edit", &sha])
        .map_err(|e| format!("revert of {sha} failed: {e}"))?;
    git(repo, &["rev-parse", "HEAD"])
}

/// The latest verify record for a ticket, if any.
pub async fn latest_verify(ticket_id: &str) -> Option<crate::sandbox_verify::VerifyRecord> {
    let records = crate::sandbox_verify::sandbox_verify_records().await.ok()?;
    records
        .into_iter()
        .filter(|r| r.ticket_id.eq_ignore_ascii_case(ticket_id))
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(path: &str, added: u32, removed: u32) -> FileStat {
        FileStat {
            path: path.into(),
            added,
            removed,
        }
    }

    #[test]
    fn numstat_parses_and_tolerates_binary_rows() {
        let parsed = parse_numstat("12\t3\tsrc/a.rs\n-\t-\tlogo.png\n");
        assert_eq!(parsed[0], stat("src/a.rs", 12, 3));
        assert_eq!(parsed[1], stat("logo.png", 0, 0));
    }

    #[test]
    fn a_small_verified_change_with_tests_is_low() {
        let files = vec![stat("src/a.rs", 20, 5), stat("src/a_test.rs", 15, 0)];
        let risk = risk_score(&files, true);
        assert_eq!(risk.band, "low", "{:?}", risk.reasons);
    }

    #[test]
    fn touching_the_acl_pushes_toward_approval() {
        let files = vec![stat("src-tauri/permissions/default.toml", 4, 0)];
        let risk = risk_score(&files, true);
        assert!(risk.score >= 3, "{:?}", risk.reasons);
        assert!(risk.reasons.iter().any(|r| r.contains("sensitive")));
    }

    #[test]
    fn a_big_unverified_untested_sensitive_change_is_high() {
        let files = vec![
            stat("src-tauri/src/main.rs", 400, 200),
            stat("src/js/app.js", 100, 50),
        ];
        let risk = risk_score(&files, false);
        assert!(risk.score >= HUMAN_APPROVAL_AT, "{:?}", risk.reasons);
        assert_eq!(risk.band, "high");
    }

    #[test]
    fn shrinking_tests_is_named_as_a_signal() {
        let files = vec![stat("tests/exe-computer.spec.mjs", 2, 80)];
        let risk = risk_score(&files, true);
        assert!(risk.reasons.iter().any(|r| r.contains("tests shrink")));
    }

    #[test]
    fn the_score_is_capped() {
        let files: Vec<FileStat> = (0..20)
            .map(|i| stat(&format!("src-tauri/permissions/f{i}.toml"), 100, 100))
            .collect();
        assert_eq!(risk_score(&files, false).score, 10);
    }
}

/// Numstat of what merging `branch` would bring in (merge-base three-dot).
pub fn numstat_against_head(repo: &Path, branch: &str) -> Result<Vec<FileStat>, String> {
    git(repo, &["diff", "--numstat", &format!("HEAD...{branch}")]).map(|out| parse_numstat(&out))
}

#[cfg(test)]
mod git_tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A throwaway repo with one commit on main, removed on drop. Same
    /// std-only pattern as settings.rs's tests; no tempfile dependency.
    /// Signing is off in-repo so the test never touches the machine's real
    /// signing setup.
    struct ScratchRepo(std::path::PathBuf);
    impl ScratchRepo {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for ScratchRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch_repo() -> ScratchRepo {
        let dir = std::env::temp_dir().join(format!(
            "xnaut-merge-gate-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        run(&dir, &["init", "-b", "main"]);
        run(&dir, &["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        run(&dir, &["add", "-A"]);
        run(&dir, &["commit", "-m", "init"]);
        ScratchRepo(dir)
    }

    #[test]
    fn merge_then_revert_round_trips() {
        let dir = scratch_repo();
        let repo = dir.path();
        run(repo, &["checkout", "-b", "agent/claudi/thing"]);
        std::fs::write(repo.join("b.txt"), "two\n").unwrap();
        run(repo, &["add", "-A"]);
        run(repo, &["commit", "-m", "XNAUT-999: add b"]);
        run(repo, &["checkout", "main"]);

        // The branch is found from the ticket id in its history.
        let candidates = candidate_branches(repo, "XNAUT-999").unwrap();
        assert_eq!(candidates, vec!["agent/claudi/thing"]);

        let outcome = merge_branch(repo, "agent/claudi/thing", "merge: XNAUT-999").unwrap();
        let sha = match outcome {
            MergeOutcome::Merged(sha) => sha,
            MergeOutcome::Conflict(paths) => panic!("unexpected conflict: {paths:?}"),
        };
        assert!(repo.join("b.txt").is_file());

        // The safeguard: one revert takes the work back out.
        let reverted = revert_ticket_merge(repo, "XNAUT-999").unwrap();
        assert_ne!(reverted, sha);
        assert!(
            !repo.join("b.txt").is_file(),
            "revert removed the merged file"
        );
    }

    #[test]
    fn a_conflict_aborts_and_leaves_the_tree_clean() {
        let dir = scratch_repo();
        let repo = dir.path();
        run(repo, &["checkout", "-b", "agent/x/clash"]);
        std::fs::write(repo.join("a.txt"), "branch says\n").unwrap();
        run(repo, &["add", "-A"]);
        run(repo, &["commit", "-m", "XNAUT-998: branch side"]);
        run(repo, &["checkout", "main"]);
        std::fs::write(repo.join("a.txt"), "main says\n").unwrap();
        run(repo, &["add", "-A"]);
        run(repo, &["commit", "-m", "main side"]);

        match merge_branch(repo, "agent/x/clash", "merge: XNAUT-998").unwrap() {
            MergeOutcome::Conflict(paths) => assert_eq!(paths, vec!["a.txt"]),
            MergeOutcome::Merged(sha) => panic!("should not merge: {sha}"),
        }
        // Aborted: the tree is clean and still says main's line.
        assert_eq!(
            std::fs::read_to_string(repo.join("a.txt")).unwrap(),
            "main says\n"
        );
    }

    #[test]
    fn a_dirty_tree_refuses_the_merge() {
        let dir = scratch_repo();
        let repo = dir.path();
        run(repo, &["branch", "agent/x/any"]);
        std::fs::write(repo.join("a.txt"), "uncommitted\n").unwrap();
        let error = merge_branch(repo, "agent/x/any", "m").unwrap_err();
        assert!(error.contains("uncommitted"), "{error}");
    }
}
