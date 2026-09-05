// Publishing gets a state machine, not a prompt (XNAUT-209).
//
// Merging is reversible: merge_gate scores a diff and one revert undoes it.
// Publishing is not. A tag is fetched, a cask is bumped, an installer is on
// someone's disk. So the boundary gets the two things an irreversible step
// needs and a prompt cannot supply: separation of duties, and a machine that
// stops rather than retries.
//
// Separation of duties. The agent that decides a build is releasable and the
// agent that publishes it are not the same agent. That is not enforced by
// asking nicely in a system prompt; `verify` refuses when the record's
// `tested_by` is the caller doing the release.
//
// The contract is one full commit SHA. A test report approves that SHA and
// nothing else; any commit after it invalidates the approval, and the report
// is consumed exactly once. Signed here means attributable and tamper-evident
// by the Worklog principle, not notarised: the record is JSON committed to
// the control repo, and git supplies the history and the author. An HSM is
// disproportionate until releases are externally audited.
//
// A run that fails a check stays stopped at its last state with the reason
// attached. It never silently retries forward.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One command the tester ran, and what it exited with.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct CommandRun {
    pub cmd: String,
    #[serde(default)]
    pub exit: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct TestResults {
    #[serde(default)]
    pub passed: u32,
    #[serde(default)]
    pub failed: u32,
    #[serde(default)]
    pub skipped: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Artifact {
    pub path: String,
    #[serde(default)]
    pub sha256: String,
}

/// The test report. One JSON file per candidate, committed to the control
/// repo: `projects/<KEY>/release-candidates/<KEY>-rc-<n>.json`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct ReleaseCandidate {
    pub id: String,
    pub project: String,
    /// Absolute path of the repo that was tested. The drift check reads it.
    pub repo: String,
    /// Full SHA. The whole contract; a short SHA is not accepted.
    pub sha: String,
    /// Agent handle from agent-profiles.toml.
    pub tested_by: String,
    #[serde(default)]
    pub commands: Vec<CommandRun>,
    #[serde(default)]
    pub results: TestResults,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    /// Where it was tested. Filled by xNAUT, not by the tester: a field an
    /// agent can populate freely is a field that ends up holding `env` output,
    /// and this record is committed to a git repo.
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    /// approved | rejected | approved_with_risks
    pub verdict: String,
    pub created_at: String,
    /// Set to the release id the first time it is accepted, and never again.
    #[serde(default)]
    pub consumed_by: Option<String>,
}

pub const VERDICTS: [&str; 3] = ["approved", "rejected", "approved_with_risks"];

/// The machine, in order. Index in this list IS the state; the only legal move
/// is to the next entry.
///
/// ponytail: a &str list rather than an enum. It serialises as itself, the
/// transition rule is one `position()`, and an enum would need Display,
/// FromStr and a serde rename for identical behaviour.
pub const RELEASE_STATES: [&str; 7] = [
    "candidate_received",
    "approval_verified",
    "release_started",
    "artifact_published",
    "smoke_verified",
    "docs_updated",
    "released",
];

/// The single legal successor, or None at the end (and for an unknown state).
pub fn next_state(state: &str) -> Option<&'static str> {
    let at = RELEASE_STATES.iter().position(|known| *known == state)?;
    RELEASE_STATES.get(at + 1).copied()
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Transition {
    pub state: String,
    pub at: String,
}

/// One release run: `projects/<KEY>/releases/<KEY>-rel-<n>.json`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct ReleaseRun {
    pub id: String,
    pub project: String,
    pub candidate: String,
    pub sha: String,
    pub released_by: String,
    pub state: String,
    /// Why it refused. Set once; a stopped run does not move again.
    #[serde(default)]
    pub stopped: Option<String>,
    #[serde(default)]
    pub history: Vec<Transition>,
    pub started_at: String,
    pub updated_at: String,
}

/// What the five checks need that is not in the record itself.
pub struct Preflight<'a> {
    /// The SHA about to be published. Usually HEAD, but a tag can name another.
    pub release_sha: &'a str,
    /// Commits on HEAD that are not in `candidate.sha`. See `drift`.
    pub commits_after: u32,
    /// The handle whose report this release is willing to trust.
    pub designated_tester: &'a str,
    /// The agent doing the release.
    pub releaser: &'a str,
}

/// The five checks, in the order the design doc lists them. Any one failing
/// returns the reason; the caller stops the run with it and does not retry.
pub fn verify(candidate: &ReleaseCandidate, ctx: &Preflight) -> Result<(), String> {
    // 1. The approved SHA is still the release SHA.
    if candidate.sha.is_empty() || candidate.sha != ctx.release_sha {
        return Err(format!(
            "{} approved {}, but this release publishes {}",
            candidate.id,
            short(&candidate.sha),
            short(ctx.release_sha)
        ));
    }
    // 2. The report belongs to the designated Test Agent, and that agent is
    //    not the one releasing. Nobody approves their own work.
    if !candidate.tested_by.eq_ignore_ascii_case(ctx.designated_tester) {
        return Err(format!(
            "{} was tested by @{}, and the designated tester is @{}",
            candidate.id, candidate.tested_by, ctx.designated_tester
        ));
    }
    if candidate.tested_by.eq_ignore_ascii_case(ctx.releaser) {
        return Err(format!(
            "@{} tested {} and cannot also release it; the tester and the releaser are separate agents",
            ctx.releaser, candidate.id
        ));
    }
    // 3. Required checks passed.
    if candidate.verdict != "approved" && candidate.verdict != "approved_with_risks" {
        return Err(format!(
            "{} carries verdict '{}', which is not an approval",
            candidate.id, candidate.verdict
        ));
    }
    if candidate.commands.is_empty() {
        return Err(format!("{} records no commands, so nothing was proven", candidate.id));
    }
    if let Some(failed) = candidate.commands.iter().find(|run| run.exit != 0) {
        return Err(format!(
            "{} approved a build where `{}` exited {}",
            candidate.id, failed.cmd, failed.exit
        ));
    }
    if candidate.results.failed > 0 {
        return Err(format!(
            "{} reports {} failing test(s)",
            candidate.id, candidate.results.failed
        ));
    }
    // 4. No newer commit invalidated the approval.
    if ctx.commits_after > 0 {
        return Err(format!(
            "{} commit(s) landed after {} was tested; the approval is stale and a new test report is needed",
            ctx.commits_after,
            short(&candidate.sha)
        ));
    }
    // 5. The report has not already been consumed.
    if let Some(by) = candidate.consumed_by.as_deref().filter(|by| !by.is_empty()) {
        return Err(format!("{} was already consumed by {}", candidate.id, by));
    }
    Ok(())
}

fn short(sha: &str) -> String {
    sha.chars().take(12).collect()
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|error| format!("git {}: {error}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// How many commits are on HEAD that the tested SHA does not contain.
pub fn drift(repo: &Path, sha: &str) -> Result<u32, String> {
    let count = git(repo, &["rev-list", "--count", &format!("{sha}..HEAD")])?;
    count
        .parse()
        .map_err(|_| format!("git rev-list returned '{count}', which is not a count"))
}

pub fn head_sha(repo: &Path) -> Result<String, String> {
    git(repo, &["rev-parse", "HEAD"])
}

fn candidates_dir(control: &Path, project: &str) -> PathBuf {
    control.join("projects").join(project).join("release-candidates")
}

fn releases_dir(control: &Path, project: &str) -> PathBuf {
    control.join("projects").join(project).join("releases")
}

/// Next free `<KEY>-<kind>-<n>`, by scanning what is already there.
fn next_id(dir: &Path, project: &str, kind: &str) -> String {
    let prefix = format!("{project}-{kind}-");
    let highest = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".json")?.to_string();
            stem.strip_prefix(&prefix)?.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0);
    format!("{prefix}{}", highest + 1)
}

fn write_new(path: &Path, value: &impl Serialize) -> Result<(), String> {
    // A gate fails closed: a colliding id refuses rather than overwriting a
    // record someone else is about to release from.
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    write_existing(path, value)
}

fn write_existing(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    crate::project_management::write_json_atomic(path, value)
}

/// The Test Agent's move: write the report and commit it. The git commit is
/// what makes it attributable and tamper-evident.
pub fn record_candidate(
    control: &Path,
    mut candidate: ReleaseCandidate,
) -> Result<ReleaseCandidate, String> {
    if candidate.project.trim().is_empty() {
        return Err("a candidate needs a project key".into());
    }
    if candidate.sha.len() < 40 {
        return Err("a candidate needs the full 40-character commit SHA".into());
    }
    if candidate.tested_by.trim().is_empty() {
        return Err("a candidate needs the handle of the agent that tested it".into());
    }
    if !VERDICTS.contains(&candidate.verdict.as_str()) {
        return Err(format!(
            "verdict must be one of {}; got '{}'",
            VERDICTS.join(", "),
            candidate.verdict
        ));
    }
    let dir = candidates_dir(control, &candidate.project);
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
    candidate.id = next_id(&dir, &candidate.project, "rc");
    candidate.created_at = chrono::Utc::now().to_rfc3339();
    candidate
        .environment
        .insert("os".into(), std::env::consts::OS.to_string());
    candidate
        .environment
        .insert("arch".into(), std::env::consts::ARCH.to_string());
    candidate.consumed_by = None;
    let path = dir.join(format!("{}.json", candidate.id));
    write_new(&path, &candidate)?;
    crate::project_management::record_mutation(
        control,
        "release.candidate_recorded",
        &candidate.id,
        json!({
            "project": candidate.project,
            "sha": candidate.sha,
            "tested_by": candidate.tested_by,
            "verdict": candidate.verdict,
        }),
        &[path],
        &format!("chore(release): test report {}", candidate.id),
    )?;
    Ok(candidate)
}

pub fn load_candidate(control: &Path, project: &str, id: &str) -> Result<ReleaseCandidate, String> {
    let path = candidates_dir(control, project).join(format!("{id}.json"));
    crate::project_management::read_json(&path)
}

pub fn load_run(control: &Path, project: &str, id: &str) -> Result<ReleaseRun, String> {
    let path = releases_dir(control, project).join(format!("{id}.json"));
    crate::project_management::read_json(&path)
}

fn save_run(control: &Path, run: &ReleaseRun, event: &str, message: &str) -> Result<(), String> {
    let path = releases_dir(control, &run.project).join(format!("{}.json", run.id));
    write_existing(&path, run)?;
    crate::project_management::record_mutation(
        control,
        event,
        &run.id,
        json!({
            "project": run.project,
            "candidate": run.candidate,
            "sha": run.sha,
            "state": run.state,
            "stopped": run.stopped,
        }),
        &[path],
        message,
    )
}

/// Open a run against a candidate and put it through the five checks.
///
/// The run is persisted at `candidate_received` before anything is verified,
/// so a refusal is recorded rather than merely returned. On acceptance the
/// candidate is consumed in the same step, which is what makes "exactly once"
/// hold: a second run against the same report fails check 5.
pub fn start_release(
    control: &Path,
    project: &str,
    candidate_id: &str,
    releaser: &str,
    designated_tester: &str,
    release_sha: Option<&str>,
) -> Result<ReleaseRun, String> {
    let mut candidate = load_candidate(control, project, candidate_id)?;
    let repo = PathBuf::from(&candidate.repo);
    if !repo.is_dir() {
        return Err(format!("{} names a repo that is not there: {}", candidate.id, candidate.repo));
    }
    let release_sha = match release_sha {
        Some(sha) if !sha.trim().is_empty() => sha.trim().to_string(),
        _ => head_sha(&repo)?,
    };
    let commits_after = drift(&repo, &candidate.sha).unwrap_or(u32::MAX);

    let now = chrono::Utc::now().to_rfc3339();
    let dir = releases_dir(control, project);
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
    let mut run = ReleaseRun {
        id: next_id(&dir, project, "rel"),
        project: project.to_string(),
        candidate: candidate.id.clone(),
        sha: release_sha.clone(),
        released_by: releaser.to_string(),
        state: RELEASE_STATES[0].to_string(),
        stopped: None,
        history: vec![Transition { state: RELEASE_STATES[0].to_string(), at: now.clone() }],
        started_at: now.clone(),
        updated_at: now,
    };
    save_run(
        control,
        &run,
        "release.candidate_received",
        &format!("chore(release): {} received {}", run.id, candidate.id),
    )?;

    let ctx = Preflight {
        release_sha: &release_sha,
        commits_after,
        designated_tester,
        releaser,
    };
    if let Err(reason) = verify(&candidate, &ctx) {
        return Err(stop(control, run, reason)?);
    }

    // Consume the report before advancing. If this write fails the run stays
    // at candidate_received, which is the safe end of the race.
    candidate.consumed_by = Some(run.id.clone());
    let candidate_path = candidates_dir(control, project).join(format!("{}.json", candidate.id));
    write_existing(&candidate_path, &candidate)?;
    run = advance(control, project, &run.id, RELEASE_STATES[1])?;
    Ok(run)
}

/// Move one state forward. Anything else refuses: a skip, a step back, an
/// unknown state, or a run that has already stopped.
pub fn advance(
    control: &Path,
    project: &str,
    run_id: &str,
    to: &str,
) -> Result<ReleaseRun, String> {
    let mut run = load_run(control, project, run_id)?;
    if let Some(reason) = run.stopped.as_deref() {
        return Err(format!(
            "{} stopped at {}: {}. A stopped release does not resume; open a new one.",
            run.id, run.state, reason
        ));
    }
    match next_state(&run.state) {
        None => Err(format!("{} is at {}, which is the end", run.id, run.state)),
        Some(legal) if legal != to => Err(format!(
            "{} is at {} and the only move is {}, not {}",
            run.id, run.state, legal, to
        )),
        Some(legal) => {
            let now = chrono::Utc::now().to_rfc3339();
            run.state = legal.to_string();
            run.history.push(Transition { state: legal.to_string(), at: now.clone() });
            run.updated_at = now;
            save_run(
                control,
                &run,
                &format!("release.{legal}"),
                &format!("chore(release): {} {}", run.id, legal),
            )?;
            Ok(run)
        }
    }
}

/// Stop a run where it stands, with the reason attached. Returns the reason so
/// the caller can hand it straight back as the error.
pub fn stop(control: &Path, mut run: ReleaseRun, reason: String) -> Result<String, String> {
    run.stopped = Some(reason.clone());
    run.updated_at = chrono::Utc::now().to_rfc3339();
    save_run(
        control,
        &run,
        "release.stopped",
        &format!("chore(release): {} stopped at {}", run.id, run.state),
    )?;
    Ok(format!("{} stopped at {}: {}", run.id, run.state, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> ReleaseCandidate {
        ReleaseCandidate {
            id: "XNAUT-rc-1".into(),
            project: "XNAUT".into(),
            repo: String::new(),
            sha: "a".repeat(40),
            tested_by: "ralph".into(),
            commands: vec![CommandRun { cmd: "cargo test".into(), exit: 0 }],
            results: TestResults { passed: 412, failed: 0, skipped: 3 },
            warnings: vec![],
            artifacts: vec![],
            environment: BTreeMap::new(),
            verdict: "approved".into(),
            created_at: "2026-09-05T00:00:00+00:00".into(),
            consumed_by: None,
        }
    }

    fn ctx<'a>(sha: &'a str) -> Preflight<'a> {
        Preflight {
            release_sha: sha,
            commits_after: 0,
            designated_tester: "ralph",
            releaser: "otto",
        }
    }

    #[test]
    fn an_approved_report_for_the_current_sha_passes() {
        let candidate = good();
        assert_eq!(verify(&candidate, &ctx(&candidate.sha)), Ok(()));
    }

    /// Each of the five checks refuses on its own, with a reason that names
    /// what refused. A gate whose failures are indistinguishable is a gate
    /// nobody can act on.
    #[test]
    fn each_of_the_five_checks_refuses_by_itself() {
        let sha = "a".repeat(40);
        let cases: Vec<(&str, ReleaseCandidate, Preflight, &str)> = vec![
            // 1. the approved SHA is no longer the release SHA
            ("sha moved", good(), Preflight { release_sha: "b", ..ctx(&sha) }, "publishes"),
            // 2a. wrong tester
            (
                "wrong tester",
                ReleaseCandidate { tested_by: "someone-else".into(), ..good() },
                ctx(&sha),
                "designated tester",
            ),
            // 2b. the releaser tested it
            (
                "self approval",
                ReleaseCandidate { tested_by: "otto".into(), ..good() },
                Preflight { designated_tester: "otto", ..ctx(&sha) },
                "cannot also release",
            ),
            // 3. checks did not pass
            (
                "failing command",
                ReleaseCandidate {
                    commands: vec![CommandRun { cmd: "cargo test".into(), exit: 101 }],
                    ..good()
                },
                ctx(&sha),
                "exited 101",
            ),
            // 4. a newer commit invalidated the approval
            ("stale", good(), Preflight { commits_after: 2, ..ctx(&sha) }, "stale"),
            // 5. already consumed
            (
                "consumed",
                ReleaseCandidate { consumed_by: Some("XNAUT-rel-1".into()), ..good() },
                ctx(&sha),
                "already consumed",
            ),
        ];
        for (label, candidate, preflight, needle) in cases {
            let refusal = verify(&candidate, &preflight)
                .expect_err(&format!("{label} should have refused"));
            assert!(
                refusal.contains(needle),
                "{label}: expected a reason mentioning '{needle}', got '{refusal}'"
            );
        }
    }

    #[test]
    fn the_machine_moves_one_state_and_stops_at_released() {
        assert_eq!(next_state("candidate_received"), Some("approval_verified"));
        assert_eq!(next_state("released"), None);
        assert_eq!(next_state("published"), None);
    }

    /// The whole thing on disk: a report is written and committed, a release
    /// walks the seven states, a skip is refused, and the report cannot be
    /// used twice.
    #[test]
    fn a_release_walks_the_machine_once_and_the_report_cannot_be_reused() {
        let root = std::env::temp_dir().join(format!("xnaut-release-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let control = root.join("control");
        let product = root.join("product");
        for dir in [&control, &product] {
            std::fs::create_dir_all(dir.join("events")).unwrap();
            git(dir, &["init", "-q", "-b", "main"]).unwrap();
            git(dir, &["config", "user.email", "t@t"]).unwrap();
            git(dir, &["config", "user.name", "t"]).unwrap();
            git(dir, &["commit", "-q", "--allow-empty", "-m", "root"]).unwrap();
        }
        let sha = head_sha(&product).unwrap();

        let candidate = record_candidate(
            &control,
            ReleaseCandidate {
                project: "XNAUT".into(),
                repo: product.to_string_lossy().into_owned(),
                sha: sha.clone(),
                tested_by: "ralph".into(),
                commands: vec![CommandRun { cmd: "cargo test".into(), exit: 0 }],
                verdict: "approved".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(candidate.id, "XNAUT-rc-1");

        let run =
            start_release(&control, "XNAUT", &candidate.id, "otto", "ralph", None).unwrap();
        assert_eq!(run.state, "approval_verified");

        // A skip is refused, and refusing does not move the run.
        let skip = advance(&control, "XNAUT", &run.id, "released").unwrap_err();
        assert!(skip.contains("release_started"), "{skip}");
        assert_eq!(load_run(&control, "XNAUT", &run.id).unwrap().state, "approval_verified");

        for state in &RELEASE_STATES[2..] {
            assert_eq!(advance(&control, "XNAUT", &run.id, state).unwrap().state, *state);
        }
        assert!(advance(&control, "XNAUT", &run.id, "released").unwrap_err().contains("the end"));

        // The report is spent. A second release from it stops at check 5.
        let second = start_release(&control, "XNAUT", &candidate.id, "otto", "ralph", None)
            .unwrap_err();
        assert!(second.contains("already consumed"), "{second}");
        assert!(second.contains("stopped at candidate_received"), "{second}");

        // Every transition left an event behind.
        let events = std::fs::read_dir(control.join("events")).unwrap().count();
        assert!(events >= 9, "expected an event per transition, found {events}");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
