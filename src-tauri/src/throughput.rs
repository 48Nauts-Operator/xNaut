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

/// One verification a lane's work produced. `passed` is the whole of it: this
/// is the error rate the swarm lane exists to measure (XNAUT-319), and the
/// audited lane is counted the same way so the two can be read side by side.
#[derive(Debug, Clone, PartialEq)]
pub struct Verify {
    pub ticket: String,
    /// Epoch seconds.
    pub at: i64,
    pub passed: bool,
}

/// Both lanes, on the same numbers. The comparison is the point: if the swarm
/// lane's error rate is small and steady beside the audited lane's, that is
/// the evidence for moving the jury from every merge to a sampled audit. If it
/// is not, this is the number that says so (XNAUT-319).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Lanes {
    pub audited: Throughput,
    pub swarm: Throughput,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Throughput {
    /// "audited" or "swarm", and the branch that lane merges to.
    pub lane: String,
    pub branch: String,
    /// False when the lane's branch does not exist in any repository read.
    /// Its zeros then mean "never opened", not "made no merges" — the same
    /// distinction `collect` draws with `None`.
    pub branch_exists: bool,
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
    /// Verifications this lane's tickets produced in the window, and the share
    /// of them that were red. On the swarm lane a red build is fixed by the
    /// next worker rather than reverted, so this is its tolerated error rate
    /// rather than an alarm.
    pub verifies_green: usize,
    pub verifies_red: usize,
    pub error_rate: f64,
    /// Always false. Named so the absence is visible rather than a zero that
    /// looks like a measurement.
    pub tool_calls_counted: bool,
}

/// Who is a person, for the commit split. The supervisor passes its own git
/// identity; anything else that is not the machine counts as an agent.
pub const MACHINE_AUTHOR: &str = "NautBot";

pub fn summarise(
    lane: crate::swarm::Lane,
    branch: &str,
    jobs: &[Job],
    commits: &[Commit],
    verifies: &[Verify],
    humans: &[String],
    window_hours: f64,
) -> Throughput {
    let mut t = Throughput {
        lane: lane.name().into(),
        branch: branch.into(),
        window_hours,
        ..Default::default()
    };
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
    for v in verifies {
        if v.passed {
            t.verifies_green += 1;
        } else {
            t.verifies_red += 1;
        }
    }
    if window_hours > 0.0 {
        t.merges_per_hour = t.merges_machine as f64 / window_hours;
    }
    if t.signoffs_integrated > 0 {
        t.escalations_per_merge = t.signoffs_escalated as f64 / t.signoffs_integrated as f64;
    }
    let builds = t.verifies_green + t.verifies_red;
    if builds > 0 {
        t.error_rate = t.verifies_red as f64 / builds as f64;
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

/// Verifications inside the window, as the error rate needs them. A record
/// still running, cancelled or orphaned is not a build result and is not
/// counted either way; `now` is epoch seconds.
pub fn verifies_in(records: &[crate::sandbox_verify::VerifyRecord], hours: u64, now: i64) -> Vec<Verify> {
    let since = now - (hours as i64) * 3600;
    records
        .iter()
        .filter_map(|r| {
            let passed = match r.status.as_str() {
                "passed" => true,
                "failed" => false,
                _ => return None,
            };
            let at = chrono::DateTime::parse_from_rfc3339(&r.created_at).ok()?.timestamp();
            (at >= since).then(|| Verify { ticket: r.ticket_id.clone(), at, passed })
        })
        .collect()
}

/// The repositories behind `roots`, one entry per git object store, so a
/// worktree and its parent checkout are not counted twice.
fn repos(roots: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        let common = std::process::Command::new("git")
            .arg("-C").arg(root)
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        let Some(common) = common else { continue };
        if seen.insert(common) {
            out.push(root.clone());
        }
    }
    out
}

/// One lane's numbers, over repositories already deduplicated. `answered` is
/// false when no repository has the branch at all.
fn lane_numbers(
    repos: &[std::path::PathBuf],
    lane: crate::swarm::Lane,
    branch: &str,
    jobs: &[Job],
    verifies: &[Verify],
    hours: u64,
) -> (Throughput, bool) {
    let mut commits = Vec::new();
    let mut humans = Vec::new();
    let mut answered = false;
    for repo in repos {
        if let Some(found) = recent_commits(repo, branch, hours) {
            answered = true;
            commits.extend(found);
            humans.extend(local_human(repo));
        }
    }
    humans.sort();
    humans.dedup();
    let mut t = summarise(lane, branch, jobs, &commits, verifies, &humans, hours as f64);
    t.branch_exists = answered;
    (t, answered)
}

/// The whole picture for one supervisor, both lanes side by side: every
/// project's integration branch and the swarm branch over the last `hours`,
/// deduplicated by repository, plus the jury store and the verification
/// records split by which lane's ticket produced them.
///
/// `None` only when no project could answer for the AUDITED lane; a swarm
/// branch nobody has opened yet is zeros with `branch_exists: false`, which is
/// a different statement from "could not read the repository".
pub fn collect_lanes(
    roots: &[std::path::PathBuf],
    registry: &Path,
    swarm_tickets: &std::collections::HashSet<String>,
    verifies: &[Verify],
    branch: &str,
    hours: u64,
) -> Option<Lanes> {
    let repos = repos(roots);
    let jobs = jobs_in(&registry.join("jury"));
    // A job is the lane's if its ticket is. There should never be one on the
    // swarm lane; counting them there rather than dropping them is what makes
    // that claim checkable instead of assumed.
    let (swarm_jobs, audited_jobs): (Vec<Job>, Vec<Job>) =
        jobs.into_iter().partition(|j| swarm_tickets.contains(&j.ticket));
    let (swarm_v, audited_v): (Vec<Verify>, Vec<Verify>) = verifies
        .iter()
        .cloned()
        .partition(|v| swarm_tickets.contains(&v.ticket));
    let (audited, answered) = lane_numbers(
        &repos,
        crate::swarm::Lane::Audited,
        branch,
        &audited_jobs,
        &audited_v,
        hours,
    );
    if !answered {
        return None;
    }
    let (swarm, _) = lane_numbers(
        &repos,
        crate::swarm::Lane::Swarm,
        crate::swarm::BRANCH,
        &swarm_jobs,
        &swarm_v,
        hours,
    );
    Some(Lanes { audited, swarm })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::swarm::Lane;

    fn commit(author: &str, is_merge: bool) -> Commit {
        Commit { author: author.into(), at: 0, is_merge }
    }
    fn audited(jobs: &[Job], commits: &[Commit], humans: &[String], hours: f64) -> Throughput {
        summarise(Lane::Audited, "dev", jobs, commits, &[], humans, hours)
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
        let t = audited(&[], &commits, &["Cand0rian".into()], 168.0);
        assert_eq!((t.merges_machine, t.commits_agent, t.commits_hand), (7, 16, 36));
        assert!((t.merges_per_hour - 7.0 / 168.0).abs() < 1e-9);
        // A machine commit that is not a merge is not a merge.
        let t = audited(&[], &[commit(MACHINE_AUTHOR, false)], &[], 1.0);
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
        let t = audited(&jobs, &[], &[], 1.0);
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
        let t = audited(&[plan], &[], &[], 0.0);
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
        let none = std::collections::HashSet::new();
        let audited = |roots: &[std::path::PathBuf]| {
            collect_lanes(roots, &dir.join("registry"), &none, &[], "dev", 24)
        };
        let t = audited(&[repo.clone()]).expect("a real root answers").audited;
        assert_eq!(t.commits_hand + t.commits_agent, 1);
        assert_eq!(t.window_hours, 24.0);
        // A root that is not a repository, alone, is None; beside a real one it is ignored.
        assert!(audited(&[std::env::temp_dir()]).is_none());
        assert!(audited(&[std::env::temp_dir(), repo.clone()]).is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_lanes_numbers_appear_beside_the_audited_lanes() {
        // XNAUT-319. Both lanes on the same numbers, from the same stores, so
        // the swarm lane's error rate can be read against the audited lane's
        // instead of argued about.
        let dir = std::env::temp_dir().join(format!("xnaut-lanes-{}", uuid::Uuid::new_v4()));
        let store = dir.join("registry/jury");
        std::fs::create_dir_all(&store).unwrap();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let o = std::process::Command::new("git").arg("-C").arg(&repo)
                .args(["-c", "user.email=t@t", "-c", "user.name=Hand", "-c", "commit.gpgsign=false"])
                .args(args).output().unwrap();
            assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        };
        git(&["init", "-q", "-b", "dev"]);
        git(&["config", "user.name", "Hand"]);
        git(&["commit", "-q", "--allow-empty", "-m", "base"]);

        // The lane has not been opened: zeros, and it says so rather than
        // reading as a lane that merged nothing.
        let swarm_tickets = std::collections::HashSet::from(["XNAUT-2".to_string()]);
        let lanes = collect_lanes(&[repo.clone()], &dir.join("registry"), &swarm_tickets, &[], "dev", 24)
            .expect("the audited lane answers");
        assert_eq!(lanes.audited.lane, "audited");
        assert_eq!(lanes.audited.branch, "dev");
        assert!(lanes.audited.branch_exists);
        assert_eq!(lanes.swarm.lane, "swarm");
        assert_eq!(lanes.swarm.branch, crate::swarm::BRANCH);
        assert!(!lanes.swarm.branch_exists, "an unopened lane is not a lane with no merges");
        assert_eq!(lanes.swarm.merges_machine, 0);

        // Open the lane and merge on it, as `swarm::merge` does.
        git(&["checkout", "-q", "-b", "swarm"]);
        git(&["checkout", "-q", "-b", "work"]);
        git(&["-c", "user.name=tron", "commit", "-q", "--allow-empty", "-m", "lane work"]);
        git(&["checkout", "-q", "swarm"]);
        git(&["-c", "user.name=NautBot", "merge", "-q", "--no-ff", "-m", "merge", "work"]);
        git(&["checkout", "-q", "dev"]);

        // Two green verifications and one red on the lane; one red on dev's.
        let v = |ticket: &str, passed: bool| Verify { ticket: ticket.into(), at: 0, passed };
        let verifies = vec![v("XNAUT-2", true), v("XNAUT-2", true), v("XNAUT-2", false), v("XNAUT-1", false)];
        // A sign-off job on the audited lane's ticket, and none on the lane's.
        let job = job("owner_signed");
        std::fs::write(store.join("j1.json"), serde_json::to_vec(&job).unwrap()).unwrap();

        let lanes = collect_lanes(&[repo.clone()], &dir.join("registry"), &swarm_tickets, &verifies, "dev", 24)
            .expect("the audited lane answers");
        assert!(lanes.swarm.branch_exists);
        assert_eq!(lanes.swarm.merges_machine, 1, "the lane's own merge");
        assert_eq!(lanes.swarm.commits_agent, 1);
        assert_eq!(lanes.audited.merges_machine, 0, "the lane's merge is not dev's");
        assert_eq!((lanes.audited.commits_agent, lanes.audited.commits_hand), (0, 1));
        // The error rate, the number the experiment turns on, on both lanes.
        assert_eq!((lanes.swarm.verifies_green, lanes.swarm.verifies_red), (2, 1));
        assert!((lanes.swarm.error_rate - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!((lanes.audited.verifies_green, lanes.audited.verifies_red), (0, 1));
        assert!((lanes.audited.error_rate - 1.0).abs() < 1e-9);
        // The jury runs on one lane only, and the count proves it rather than
        // assuming it: the job belongs to the audited ticket.
        assert_eq!(lanes.audited.signoffs_escalated, 1);
        assert_eq!(lanes.swarm.signoffs_escalated, 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_a_finished_verification_inside_the_window_is_a_build_result() {
        let now = 1_757_500_000_i64;
        let rec = |status: &str, at: i64| {
            let mut r: crate::sandbox_verify::VerifyRecord = serde_json::from_value(serde_json::json!({
                "id": "v", "run_id": "r", "ticket_id": "XNAUT-1", "project": "XNAUT",
                "repo_path": "", "provider_kind": "local", "sandbox_id": "", "public_url": "",
                "status": status, "steps": [], "log_dir": "", "video_path": null,
                "created_at": "", "updated_at": ""
            })).unwrap();
            r.created_at = chrono::DateTime::from_timestamp(at, 0).unwrap().to_rfc3339();
            r
        };
        let records = vec![
            rec("passed", now - 3600),
            rec("failed", now - 3600),
            rec("running", now - 3600),   // not a result either way
            rec("cancelled", now - 3600), // nor this
            rec("passed", now - 100 * 3600), // outside the window
        ];
        let v = verifies_in(&records, 24, now);
        assert_eq!(v.len(), 2);
        assert_eq!(v.iter().filter(|v| v.passed).count(), 1);
        // A record whose timestamp cannot be read is not silently counted as now.
        let mut broken = rec("passed", now);
        broken.created_at = "not a time".into();
        assert!(verifies_in(&[broken], 24, now).is_empty());
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
