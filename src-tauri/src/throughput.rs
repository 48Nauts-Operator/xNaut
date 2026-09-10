// XNAUT-315. What the fleet actually does, as numbers.
//
// Cursor's harness post (walked through 2026-09-10) optimised one measurement
// and reported it: commits per hour. We had never counted anything, so every
// argument about gates, agents and lanes was unfalsifiable, including the
// ones made on the page comparing us to them. This is a join over stores that
// already exist. It writes nothing.
//
// Deliberately absent: tool calls per run. The run journal records state
// transitions and the manifest keeps only the LAST hook time, so there is no
// source for it. Saying "not counted" is better than a number built on the
// wrong thing.

use crate::jury::{Gate, Job};
use std::path::Path;

/// One commit on the integration branch, as much of it as the count needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub author: String,
    /// Epoch seconds.
    pub at: i64,
    pub is_merge: bool,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Throughput {
    pub window_hours: f64,
    /// Merge commits authored by the machine. The number their post is built on.
    pub merges_machine: usize,
    pub merges_per_hour: f64,
    /// Non-merge commits by agents, and by people. The ratio between them is
    /// the honest measure of how much of the work the fleet did.
    pub commits_agent: usize,
    pub commits_hand: usize,
    /// Sign-off jobs that integrated, and how many of those were parked on
    /// the owner at some point on the way.
    pub signoffs_integrated: usize,
    pub signoffs_escalated: usize,
    pub escalations_per_merge: f64,
    /// What the escalations were worth. An owner decision that changed the
    /// outcome is a catch; one waved through unchanged is a cost. Together
    /// with the count above this is what lets a gate be judged instead of
    /// argued about.
    pub escalations_caught: usize,
    pub escalations_waved_through: usize,
    /// Always false. Named so the absence is visible rather than a zero that
    /// looks like a measurement.
    pub tool_calls_counted: bool,
}

/// Who is a person, for the commit split. The supervisor passes its own git
/// identity; anything else that is not the machine counts as an agent.
pub const MACHINE_AUTHOR: &str = "NautBot";

pub fn summarise(jobs: &[Job], commits: &[Commit], humans: &[String], window_hours: f64) -> Throughput {
    let mut t = Throughput { window_hours, ..Default::default() };
    for c in commits {
        if c.author == MACHINE_AUTHOR {
            if c.is_merge {
                t.merges_machine += 1;
            }
        } else if humans.iter().any(|h| h.eq_ignore_ascii_case(&c.author)) {
            t.commits_hand += 1;
        } else {
            t.commits_agent += 1;
        }
    }
    for job in jobs.iter().filter(|j| j.gate == Gate::Signoff) {
        match job.state.as_str() {
            "integrated" | "signed" | "merged" | "verifying" => t.signoffs_integrated += 1,
            _ => {}
        }
        // Parked on the owner at some point: either still there, or the
        // owner already answered. The answer says what the escalation was.
        match job.state.as_str() {
            "owner_required" => t.signoffs_escalated += 1,
            "owner_signed" | "owner_settled" => {
                t.signoffs_escalated += 1;
                t.escalations_waved_through += 1;
            }
            "owner_changes_requested" | "denied" | "reverted" | "revoked" => {
                t.signoffs_escalated += 1;
                t.escalations_caught += 1;
            }
            _ => {}
        }
    }
    if window_hours > 0.0 {
        t.merges_per_hour = t.merges_machine as f64 / window_hours;
    }
    if t.signoffs_integrated > 0 {
        t.escalations_per_merge = t.signoffs_escalated as f64 / t.signoffs_integrated as f64;
    }
    t
}

/// Commits on `branch` in the last `hours`, from git. `None` when the
/// repository cannot answer, which is not the same as zero commits.
pub fn recent_commits(repo: &Path, branch: &str, hours: u64) -> Option<Vec<Commit>> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "log",
            branch,
            &format!("--since={hours} hours ago"),
            "--format=%an%x1f%at%x1f%P",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let mut f = line.split('\x1f');
                let author = f.next()?.to_string();
                let at = f.next()?.parse().ok()?;
                let parents = f.next().unwrap_or_default();
                Some(Commit { author, at, is_merge: parents.split_whitespace().count() > 1 })
            })
            .collect(),
    )
}

/// Every sign-off job the jury store holds.
pub fn jobs_in(store: &Path) -> Vec<Job> {
    let Ok(entries) = std::fs::read_dir(store) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice::<Job>(&std::fs::read(e.path()).ok()?).ok())
        .collect()
}

/// The supervisor's own git identity, so its commits count as a hand.
pub fn local_human(repo: &Path) -> Vec<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["config", "user.name"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| vec![String::from_utf8_lossy(&o.stdout).trim().to_string()])
        .unwrap_or_default()
}

/// The whole picture for one supervisor: every project's integration branch
/// over the last `hours`, deduplicated by repository, plus the jury store.
/// `None` only when no project could answer at all.
pub fn collect(roots: &[std::path::PathBuf], registry: &Path, branch: &str, hours: u64) -> Option<Throughput> {
    let mut seen = std::collections::HashSet::new();
    let mut commits = Vec::new();
    let mut humans = Vec::new();
    let mut answered = false;
    for root in roots {
        let common = std::process::Command::new("git")
            .arg("-C").arg(root)
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        let Some(common) = common else { continue };
        if !seen.insert(common) { continue }
        if let Some(found) = recent_commits(root, branch, hours) {
            answered = true;
            commits.extend(found);
            humans.extend(local_human(root));
        }
    }
    if !answered { return None }
    humans.sort(); humans.dedup();
    Some(summarise(&jobs_in(&registry.join("jury")), &commits, &humans, hours as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(author: &str, is_merge: bool) -> Commit {
        Commit { author: author.into(), at: 0, is_merge }
    }
    fn job(state: &str) -> Job {
        let mut j: Job = serde_json::from_value(serde_json::json!({
            "id": state, "gate": "signoff", "ticket": "XNAUT-1", "project": "XNAUT",
            "worktree": "", "author": "", "ticket_revision": 1, "ticket_scope_hash": "",
            "input": "", "input_hash": "", "source_sha": "", "policy": {}, "round": 1,
            "deadline": 0, "reviews": [], "owner_approved": false, "reason": "",
            "state": state
        }))
        .unwrap();
        j.state = state.into();
        j
    }

    #[test]
    fn commits_split_into_machine_merges_agent_work_and_hands() {
        // The week of 2026-09-03 to 09-09 on dev, in miniature: 7 machine
        // merges, 16 agent commits, 36 by hand.
        let mut commits = vec![];
        commits.extend((0..7).map(|_| commit(MACHINE_AUTHOR, true)));
        commits.extend((0..16).map(|_| commit("tron", false)));
        commits.extend((0..36).map(|_| commit("Cand0rian", false)));
        let t = summarise(&[], &commits, &["Cand0rian".into()], 168.0);
        assert_eq!((t.merges_machine, t.commits_agent, t.commits_hand), (7, 16, 36));
        assert!((t.merges_per_hour - 7.0 / 168.0).abs() < 1e-9);
        // A machine commit that is not a merge is not a merge.
        let t = summarise(&[], &[commit(MACHINE_AUTHOR, false)], &[], 1.0);
        assert_eq!(t.merges_machine, 0);
        // Nobody claims to have counted tool calls.
        assert!(!t.tool_calls_counted);
    }

    #[test]
    fn an_escalation_is_a_catch_only_when_the_owner_changed_the_outcome() {
        let jobs = vec![
            job("integrated"),
            job("integrated"),
            job("owner_required"),          // still parked
            job("owner_signed"),            // waved through unchanged: a cost
            job("owner_changes_requested"), // changed the outcome: a catch
            job("reverted"),                // a catch after the fact
        ];
        let t = summarise(&jobs, &[], &[], 1.0);
        assert_eq!(t.signoffs_integrated, 2);
        assert_eq!(t.signoffs_escalated, 4);
        assert_eq!(t.escalations_waved_through, 1);
        assert_eq!(t.escalations_caught, 2);
        assert!((t.escalations_per_merge - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_plan_gate_is_not_a_signoff_and_an_empty_window_divides_by_nothing() {
        let mut plan = job("owner_required");
        plan.gate = Gate::Plan;
        let t = summarise(&[plan], &[], &[], 0.0);
        assert_eq!(t.signoffs_escalated, 0);
        assert_eq!(t.merges_per_hour, 0.0);
        assert_eq!(t.escalations_per_merge, 0.0);
    }

    #[test]
    fn collect_answers_for_a_real_root_and_an_empty_registry() {
        // The doctor showed `throughput: null` on tron on 2026-09-10 with a
        // valid project root and 13 commits in the window. `None` means no
        // root answered; this pins that a plain repository does answer.
        let dir = std::env::temp_dir().join(format!("xnaut-collect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("registry")).unwrap();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let o = std::process::Command::new("git").arg("-C").arg(&repo)
                .args(["-c", "user.email=t@t", "-c", "user.name=Hand", "-c", "commit.gpgsign=false"])
                .args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        };
        git(&["init", "-q", "-b", "dev"]);
        git(&["commit", "-q", "--allow-empty", "-m", "one"]);
        let t = collect(&[repo.clone()], &dir.join("registry"), "dev", 24).expect("a real root answers");
        assert_eq!(t.commits_hand + t.commits_agent, 1);
        assert_eq!(t.window_hours, 24.0);
        // A root that is not a repository, alone, is None; beside a real one it is ignored.
        assert!(collect(&[std::env::temp_dir()], &dir.join("registry"), "dev", 24).is_none());
        assert!(collect(&[std::env::temp_dir(), repo.clone()], &dir.join("registry"), "dev", 24).is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn recent_commits_reads_authors_and_merges_from_a_real_repository() {
        let dir = std::env::temp_dir().join(format!("xnaut-throughput-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let o = std::process::Command::new("git")
                .arg("-C").arg(&dir)
                .args(["-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
                .args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        };
        git(&["init", "-q", "-b", "dev"]);
        git(&["-c", "user.name=Cand0rian", "commit", "-q", "--allow-empty", "-m", "hand"]);
        git(&["checkout", "-q", "-b", "agent"]);
        git(&["-c", "user.name=tron", "commit", "-q", "--allow-empty", "-m", "agent"]);
        git(&["checkout", "-q", "dev"]);
        git(&["-c", "user.name=NautBot", "merge", "-q", "--no-ff", "-m", "merge", "agent"]);

        let commits = recent_commits(&dir, "dev", 24).expect("a repository answers");
        assert_eq!(commits.len(), 3);
        let merge = commits.iter().find(|c| c.is_merge).unwrap();
        assert_eq!(merge.author, MACHINE_AUTHOR);
        assert_eq!(commits.iter().filter(|c| !c.is_merge).count(), 2);
        // The supervisor's identity is whatever git resolves here, local or
        // global; pin a local one so the test does not depend on this machine.
        git(&["config", "user.name", "Somebody Local"]);
        assert_eq!(local_human(&dir), vec!["Somebody Local".to_string()]);

        assert_eq!(recent_commits(&std::env::temp_dir(), "dev", 1), None, "not a repository is None, not empty");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
