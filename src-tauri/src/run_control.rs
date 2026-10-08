// Durable local run registry (XNAUT-300, Phase 1).
// Adapted from xNAUT (MIT), XNAUT-277 commit 18b7632,
// run_control.rs::{RunManifest, atomic_json, replay_events_in, append_record_in}.
// We generalise its manifest and journal to lifecycle snapshots and omit the
// provider-bound command/resume machinery. This is the same store mechanism.
// Historical ideas: Tuxedo's Bulletin Board (1983), Erlang/OTP supervision,
// Condor (1988) checkpoint-and-migrate, and Garcia-Molina/Salem sagas (1987).
// These are conceptual antecedents; no source was copied from those systems.
// Every file operation takes an explicit directory. No helper selects HOME.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const GRACE_MS: i64 = 60_000;
pub const PROGRESS_WINDOW_MS: i64 = 15 * 60_000;
const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Requested,
    Starting,
    Running,
    Blocked,
    Degraded,
    Done,
    Failed,
    Retired,
    Retiring,
    Undead,
}
impl RunState {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Retired)
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    Agent,
    Verify,
    Review,
    Loom,
    Automation,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrelaunchPhase { RepositoryPreparation, RepositoryStaging, LegacyRepositoryStaging, SpendAdmission }

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PrelaunchFailure {
    pub phase: PrelaunchPhase,
    pub recorded_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RunManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub kind: RunKind,
    /// Native launch classification: owner conversations/resumes do not consume
    /// unattended worker capacity. Missing legacy classifications stay conservative.
    #[serde(default)]
    pub user_conversation: bool,
    pub ticket: Option<String>,
    pub project: String,
    /// The hostname, which is what a human recognises in a panel. Kept, and
    /// still not an identity: see `instance` below.
    pub machine: String,
    /// WHICH INSTANCE started this run (XNAUT-370), as the minted key rather
    /// than `machine`. A box that is renamed is the same instance; two boxes
    /// called `mac-mini.local` on different networks are not, and the realm
    /// (XNAUT-369) joins on this.
    ///
    /// `serde(default)` because every manifest written before today has no
    /// such key, and an empty string reads as unknown rather than as this
    /// machine. 241 manifests on tron predate it.
    #[serde(default)]
    pub instance: String,
    /// What that instance was for when the run started. Recorded rather than
    /// looked up, so a manifest read next week still says what was true when
    /// the run was launched.
    #[serde(default)]
    pub role: String,
    /// The app version that launched the run — the other half of the
    /// Studio/tron drift of 2026-09-13.
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub initiated_by: String,
    #[serde(default)]
    pub origin_thread_id: String,
    pub agent_handle: String,
    pub runtime_id: String,
    pub model: Option<String>,
    pub worktree_path: String,
    pub branch: String,
    pub pty_session: Option<String>,
    pub zellij_session: Option<String>,
    pub output_path: Option<String>,
    /// The `LaunchEnv` key when this run's processes are on a machine THIS one
    /// cannot inspect (XNAUT-307). `None` means local, which is every run that
    /// existed before the field did — hence `serde(default)`.
    ///
    /// It is load-bearing rather than decorative: `observe_in` reads local
    /// pids, a local zellij session and a local capture file, and all three
    /// are meaningless for a run inside a sandbox. Left unset, a remote run
    /// reads as a dead local one within the grace window and the reconciler
    /// fails it while the agent is still working.
    #[serde(default)]
    pub remote_env: Option<String>,
    /// The supervisor is NOT proof of the child process being alive.
    pub owner_pid: u32,
    pub pid: Option<u32>,
    pub process_birth: Option<String>,
    pub state: RunState,
    pub previous_run_id: Option<String>,
    #[serde(default)]
    pub next_run_id: Option<String>,
    #[serde(default)]
    pub retirement: Option<Retirement>,
    #[serde(default)]
    pub undead_notified: bool,
    #[serde(default)]
    pub admission_refused: bool,
    #[serde(default)]
    pub prelaunch_failure: Option<PrelaunchFailure>,
    /// Native-only origin proof retained across asynchronous repository staging.
    #[serde(default)]
    pub findings_reservation_root: Option<String>,
    pub started_at: i64,
    pub last_seen_at: i64,
    pub last_progress_at: i64,
    pub last_hook_at: Option<i64>,
    pub waiting_on: Option<String>,
    pub capture_bytes: u64,
    pub last_commit: String,
    /// Monotonic observed child CPU work, accumulated from identity-bound deltas.
    #[serde(default)]
    pub cpu_ms: u64,
    /// Last valid sample, retained across unreadable process tables and restart.
    #[serde(default)]
    pub cpu_samples: BTreeMap<u32, ProcessCpuSample>,
    /// High-water revision of the run's ticket (XNAUT-439).
    #[serde(default)]
    pub ticket_revision: u64,
    /// High-water bytes under the worktree's `.xnaut/` tree (XNAUT-439).
    #[serde(default)]
    pub verify_log_bytes: u64,
    pub last_signal: String,
    pub ticket_returned: bool,
    pub revision: u64,
}

/// A project and where its code sits ON THIS MACHINE: everything `requested`
/// needs to say which project a worktree belongs to, and nothing else.
///
/// It is a parameter rather than a lookup inside the manifest constructor so
/// that `requested` stays pure. `board()` is the impure half, kept apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSite {
    pub key: String,
    pub path: String,
}

impl ProjectSite {
    /// The projects on the board, each with its path on this machine. Errors
    /// are an empty board, not a failed launch: an unattributed run is a
    /// smaller loss than a run that never starts.
    pub fn board() -> Vec<Self> {
        let Ok(repo) = crate::project_management::repo_now() else {
            return Vec::new();
        };
        Self::from_records(&crate::project_management::list_projects(&repo).unwrap_or_default())
    }
    pub fn from_records(projects: &[crate::project_management::ProjectRecord]) -> Vec<Self> {
        projects
            .iter()
            .map(|p| Self {
                key: p.key.clone(),
                path: crate::project_management::local_source_path(p),
            })
            .filter(|site| !site.path.trim().is_empty())
            .collect()
    }
}

/// Which project owns a directory, or "" when no project on the board does.
///
/// Longest prefix wins, so a project whose path is a prefix of another's does
/// not swallow the deeper one. The comparison is by path COMPONENT, not by
/// string: `/dev/xnaut-old` is not inside `/dev/xnaut`, and Windows separators
/// have to count the same as `/`.
///
/// An empty answer stays meaningful. It says the work happened outside every
/// project on the board, which is a fact rather than a missing value.
pub fn project_for_worktree(worktree: &str, sites: &[ProjectSite]) -> String {
    let tree = Path::new(worktree.trim());
    if worktree.trim().is_empty() {
        return String::new();
    }
    let mut best: Option<(usize, &str)> = None;
    for site in sites {
        let root = Path::new(site.path.trim());
        if !tree.starts_with(root) {
            continue;
        }
        let depth = root.components().count();
        if best.is_none_or(|(deepest, _)| depth > deepest) {
            best = Some((depth, site.key.as_str()));
        }
    }
    best.map(|(_, key)| key.to_string()).unwrap_or_default()
}

impl RunManifest {
    pub fn requested(
        handle: &str,
        runtime: &str,
        worktree: &str,
        ticket: Option<String>,
        model: Option<String>,
        sites: &[ProjectSite],
        at: i64,
    ) -> Self {
        // Read once, here: a manifest is stamped by the instance that LAUNCHES
        // the run, and every later writer copies what the manifest already
        // says rather than re-reading its own.
        let stamp = crate::instance::stamp();
        Self {
            schema_version: SCHEMA_VERSION,
            run_id: new_id(at),
            kind: RunKind::Agent,
            user_conversation: false,
            // The ticket first: it is a stronger claim than a directory. Only
            // when there is no ticket does the worktree answer, and it usually
            // is the only thing that can: an agent launched from the app
            // carries no ticket, and 235 of this machine's 241 manifests were
            // stamped with an empty project for exactly that reason
            // (XNAUT-346).
            project: ticket
                .as_deref()
                .and_then(|t| t.rsplit_once('-'))
                .map(|(p, _)| p.to_string())
                .unwrap_or_else(|| project_for_worktree(worktree, sites)),
            ticket,
            machine: hostname(),
            instance: stamp.id,
            role: stamp.role,
            app_version: stamp.version,
            initiated_by: String::new(),
            origin_thread_id: String::new(),
            agent_handle: handle.trim_start_matches('@').to_lowercase(),
            runtime_id: runtime.into(),
            model,
            worktree_path: worktree.into(),
            branch: git_value(worktree, &["symbolic-ref", "--short", "HEAD"]),
            pty_session: None,
            zellij_session: None,
            output_path: None,
            remote_env: None,
            owner_pid: std::process::id(),
            pid: None,
            process_birth: None,
            state: RunState::Requested,
            previous_run_id: None,
            next_run_id: None,
            retirement: None,
            undead_notified: false,
            admission_refused: false,
            prelaunch_failure: None,
            findings_reservation_root: None,
            started_at: at,
            last_seen_at: at,
            last_progress_at: at,
            last_hook_at: None,
            waiting_on: None,
            capture_bytes: 0,
            last_commit: git_value(worktree, &["rev-parse", "HEAD"]),
            // Zero rather than a reading of the world: there is no process yet
            // to have burned CPU, and letting the first sweep set the three
            // baselines costs one advance of `last_progress_at` inside the
            // grace window, which changes no verdict.
            cpu_ms: 0,
            cpu_samples: BTreeMap::new(),
            ticket_revision: 0,
            verify_log_bytes: 0,
            last_signal: "requested".into(),
            ticket_returned: false,
            revision: 0,
        }
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
// ULID: 48-bit millisecond time followed by 80 random bits, Crockford base32.
fn new_id(at: i64) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut n = ((at.max(0) as u128 & ((1u128 << 48) - 1)) << 80)
        | (uuid::Uuid::new_v4().as_u128() & ((1u128 << 80) - 1));
    let mut out = [b'0'; 26];
    for c in out.iter_mut().rev() {
        *c = ALPHABET[(n & 31) as usize];
        n >>= 5;
    }
    String::from_utf8(out.to_vec()).expect("ASCII alphabet")
}
pub(crate) fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().into())
        .unwrap_or_default()
}
fn git_value(dir: &str, args: &[&str]) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().into())
        .unwrap_or_default()
}
fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("invalid run id".into());
    }
    Ok(())
}
fn manifest_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.run.json"))
}
fn journal_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.events.jsonl"))
}
pub fn pid_path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    Ok(dir.join(format!("{id}.pid")))
}

pub(crate) struct StoreLock(std::fs::File);
impl StoreLock {
    pub(crate) fn acquire(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(dir.join(".lock"))
            .map_err(|e| e.to_string())?;
        file.lock().map_err(|e| e.to_string())?;
        Ok(Self(file))
    }
}
impl Drop for StoreLock {
    fn drop(&mut self) {
        // Explicit release also handles inherited/cloned file descriptors.
        let _ = self.0.unlock();
    }
}
/// Operations revoking worker authority share the admission lock so a stop
/// cannot race between the final scope/capacity check and durable registration.
pub(crate) fn under_admission_lock_in<T>(dir: &Path, apply: impl FnOnce() -> Result<T,String>) -> Result<T,String> {
    let _lock = StoreLock::acquire(dir)?;
    apply()
}
/// Explicit reviewers always consume a slot; ticketless Agent reviewers retain the
/// default worker classification. Ticket presence is not a capacity discriminator.
pub(crate) fn consumes_worker_capacity(run: &RunManifest) -> bool {
    !run.state.terminal()
        && (run.kind == RunKind::Review
            || (run.kind == RunKind::Agent && !run.user_conversation))
}

/// Caller holds StoreLock. Requested successors reserve capacity across restarts;
/// ticketless independent reviewers consume a worker slot just like authors.
pub(crate) fn worker_count_in(dir: &Path) -> Result<usize,String> {
    let mut count = 0;
    for id in list_ids_in(dir)? {
        let run = load_manifest_in(dir, &id)?;
        if consumes_worker_capacity(&run) { count += 1; }
    }
    Ok(count)
}
/// A terminal native run can leave its viewport attached. Only an exact,
/// unique session/agent match retires that UI row from the legacy spend count;
/// unknown, ambiguous, or failed workers remain conservative. Atomic native
/// worker admission still counts detached workers and Requested reservations.
pub(crate) fn live_viewport_count_in(
    dir: &Path,
    sessions: &[(String, String, Option<String>)],
) -> Result<usize, String> {
    let runs = list_ids_in(dir)?.iter()
        .map(|id| load_manifest_in(dir, id)).collect::<Result<Vec<_>, _>>()?;
    // Only native IDs select receipts. Missing/corrupt evidence cannot retire
    // a viewport; neither a handle nor a guessed tmux name is proof alone.
    let transfers = runs.iter().filter_map(|run| {
        let body = std::fs::read(dir.join("repository-transfers").join(format!("{}.json", run.run_id))).ok()?;
        let transfer: crate::repository_transfer::Transfer = serde_json::from_slice(&body).ok()?;
        (transfer.run_id == run.run_id).then_some(transfer)
    }).collect::<Vec<crate::repository_transfer::Transfer>>();
    Ok(live_viewport_count(&runs, sessions, &transfers))
}
fn live_viewport_count(
    runs: &[RunManifest],
    sessions: &[(String, String, Option<String>)],
    transfers: &[crate::repository_transfer::Transfer],
) -> usize {
    sessions.iter().filter(|(session, handle, environment)| {
        let mut matching = runs.iter().filter(|run| {
            run.pty_session.as_deref() == Some(session.as_str())
                || run.zellij_session.as_deref() == Some(session.as_str())
                || (environment.as_deref() == Some("exe-dev") && run.remote_env == *environment
                    && *session == crate::sandbox::launch_env::repository_session_name(&run.agent_handle, &run.run_id)
                    && transfers.iter().filter(|t| t.run_id == run.run_id).count() == 1
                    && transfers.iter().any(|t| t.run_id == run.run_id && t.handle == run.agent_handle
                        && t.project == run.project && t.ticket == run.ticket && t.local_path == run.worktree_path
                        && t.workdir == format!("agents/runs/{}", run.run_id)
                        && matches!(t.worker, crate::worker_bootstrap::Target::ExeDev)
                        && ["review", "pushed"].contains(&t.state.as_str())))
        });
        let Some(run) = matching.next() else { return true; };
        matching.next().is_some()
            || run.agent_handle != *handle || run.remote_env != *environment
            || !matches!(run.state, RunState::Done | RunState::Retired)
    }).count()
}

pub(crate) fn worker_capacity_in(dir: &Path, cap: usize, own: &str, replacing: Option<&str>) -> Result<(),String> {
    worker_capacity_matching_in(dir, cap, own, replacing, |_| true)
}
pub(crate) fn worker_capacity_matching_in(dir: &Path, cap: usize, own: &str, replacing: Option<&str>, belongs: impl Fn(&RunManifest)->bool) -> Result<(),String> {
    let mut live = 0;
    for id in list_ids_in(dir)? {
        let run = load_manifest_in(dir, &id)?;
        if id != own && replacing != Some(id.as_str())
            && consumes_worker_capacity(&run) && belongs(&run) { live += 1; }
    }
    if live >= cap { return Err(format!("worker capacity: {live} durable author/reviewer reservations already consume limit {cap}")); }
    Ok(())
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("missing parent")?;
    let temp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

#[derive(Deserialize, Serialize)]
struct RunEvent {
    sequence: u64,
    event_id: String,
    run: RunManifest,
}
/// The journal is the write-ahead authority; a failed manifest rename is
/// recovered from its last committed snapshot, without replaying an action.
fn replay_events_in(dir: &Path, id: &str) -> Result<(Option<RunManifest>, usize), String> {
    let body = match std::fs::read(journal_path(dir, id)) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((None, 0)),
        Err(e) => return Err(e.to_string()),
    };
    let mut latest = None;
    let mut valid_end = 0;
    let mut cursor = 0;
    let mut previous = 0;
    for segment in body.split_inclusive(|c| *c == b'\n') {
        cursor += segment.len();
        if segment.iter().all(u8::is_ascii_whitespace) {
            valid_end = cursor;
            continue;
        }
        let event: RunEvent = match serde_json::from_slice(segment) {
            Ok(event) => event,
            Err(_) if cursor == body.len() => break,
            Err(e) => return Err(format!("corrupt middle journal record for {id}: {e}")),
        };
        if event.sequence <= previous
            || event.run.run_id != id
            || event.run.schema_version != SCHEMA_VERSION
        {
            return Err(format!(
                "invalid journal sequence, identity or schema for {id}"
            ));
        }
        previous = event.sequence;
        latest = Some(event.run);
        valid_end = cursor;
    }
    Ok((latest, valid_end))
}
pub fn load_manifest_in(dir: &Path, id: &str) -> Result<RunManifest, String> {
    validate_id(id)?;
    if let Some(run) = replay_events_in(dir, id)?.0 {
        return Ok(run);
    }
    let run: RunManifest =
        serde_json::from_slice(&std::fs::read(manifest_path(dir, id)).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if run.run_id != id || run.schema_version != SCHEMA_VERSION {
        return Err("unsupported run identity or schema".into());
    }
    Ok(run)
}
fn persist_locked(dir: &Path, run: &mut RunManifest) -> Result<(), String> {
    validate_id(&run.run_id)?;
    let (latest, valid_end) = replay_events_in(dir, &run.run_id)?;
    run.revision = latest.map(|r| r.revision).unwrap_or(0) + 1;
    let event = RunEvent {
        sequence: run.revision,
        event_id: uuid::Uuid::new_v4().to_string(),
        run: run.clone(),
    };
    let path = journal_path(dir, &run.run_id);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| e.to_string())?;
    // The inherited tail repair refuses middle corruption. A partial last
    // write is removed before another event can be committed after it.
    // Append-only Windows handles cannot truncate a partial journal tail.
    // StoreLock serializes the full truncate/seek/write operation on all hosts.
    file.set_len(valid_end as u64).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    if valid_end > 0 && std::fs::read(&path).map_err(|e| e.to_string())?.last() != Some(&b'\n') {
        writeln!(file).map_err(|e| e.to_string())?;
    }
    writeln!(
        file,
        "{}",
        serde_json::to_string(&event).map_err(|e| e.to_string())?
    )
    .and_then(|_| file.sync_data())
    .map_err(|e| e.to_string())?;
    atomic_json(&manifest_path(dir, &run.run_id), run)
}
pub fn request_in(
    dir: &Path,
    run: RunManifest,
    admit: impl FnOnce() -> Result<(), String>,
) -> Result<RunManifest, String> {
    let _lock = StoreLock::acquire(dir)?;
    request_locked_in(dir, run, admit)
}
fn request_locked_in(
    dir: &Path,
    mut run: RunManifest,
    admit: impl FnOnce() -> Result<(), String>,
) -> Result<RunManifest, String> {
    if manifest_path(dir, &run.run_id).exists() || journal_path(dir, &run.run_id).exists() {
        // A remote launch must know its reserved continuation ID before it
        // stages repository receipts. Consume that exact reservation once.
        let pending = load_manifest_in(dir, &run.run_id)?;
        if pending.state != RunState::Requested || pending.previous_run_id.is_none()
            || pending.previous_run_id != run.previous_run_id || pending.ticket != run.ticket
            || pending.worktree_path != run.worktree_path || pending.branch != run.branch
        { return Err("run id already exists".into()); }
    }
    if let Some(holder) = protected_worktree_in(dir, Path::new(&run.worktree_path))? {
        return Err(format!("worktree retained by protected run {holder}"));
    }
    for id in list_ids_in(dir)? {
        let other = load_manifest_in(dir, &id)?;
        if !other.state.terminal()
            && other.previous_run_id.is_some()
            && (other.worktree_path == run.worktree_path || other.ticket == run.ticket)
            && !(other.state == RunState::Requested && other.ticket == run.ticket)
        {
            return Err(format!(
                "continuation {} already owns this work",
                other.run_id
            ));
        }
    }
    if let Some(pending) = run
        .ticket
        .as_deref()
        .map(|t| continuation_in(dir, t))
        .transpose()?
        .flatten()
    {
        let retry = pending.state == RunState::Failed && pending.admission_refused;
        let previous = if retry {
            pending.clone()
        } else {
            load_manifest_in(dir, pending.previous_run_id.as_deref().unwrap())?
        };
        if !retry
            && (previous.state != RunState::Retired
                || !previous.ticket_returned
                || previous
                    .retirement
                    .as_ref()
                    .and_then(|s| s.stopped_at)
                    .is_none())
        {
            return Err("predecessor handoff is not finished".into());
        }
        if run.worktree_path != pending.worktree_path || run.branch != pending.branch {
            return Err("a successor must continue the same worktree and branch".into());
        }
        let mut ancestor = previous.clone();
        let mut visited = BTreeSet::new();
        while ancestor.retirement.is_none() {
            // A first admission refusal never owned a running worker. Its
            // exact workspace can anchor a retry chain without fabricating a
            // retirement/stop proof. The current admit callback still checks
            // ticket model, spend, quarantine, read-only and writer policies.
            if ancestor.previous_run_id.is_none() && initial_admission_refused(&ancestor) {
                break;
            }
            if !visited.insert(ancestor.run_id.clone()) {
                return Err("cyclic continuation chain".into());
            }
            ancestor = load_manifest_in(
                dir,
                ancestor
                    .previous_run_id
                    .as_deref()
                    .ok_or("missing continuation policy")?,
            )?;
        }
        if let Some(retirement) = &ancestor.retirement {
            let requirement = &retirement.requirement;
            if !model_meets(run.model.as_deref().unwrap_or_default(), requirement) {
                return Err(format!("successor model does not meet {requirement}"));
            }
        }
        if retry {
            run.previous_run_id = Some(previous.run_id.clone());
            let mut previous = previous;
            previous.next_run_id = Some(run.run_id.clone());
            persist_locked(dir, &mut run)?;
            persist_locked(dir, &mut previous)?;
        } else {
            run.run_id = pending.run_id;
            run.previous_run_id = pending.previous_run_id;
        }
    }
    persist_locked(dir, &mut run)?;
    if let Err(error) = admit() {
        run.state = RunState::Failed;
        run.admission_refused = true;
        run.last_signal = format!("admission failed: {error}");
        persist_locked(dir, &mut run)?;
        return Err(error);
    }
    run.state = RunState::Starting;
    run.last_signal = "admitted; starting".into();
    persist_locked(dir, &mut run)?;
    Ok(run)
}
/// Only the native producer calls this after a synchronous pre-worker operation
/// returned Err. Missing manifests or process probes alone never establish this proof.
pub(crate) fn refuse_prelaunch_in(dir: &Path, mut run: RunManifest, phase: PrelaunchPhase, reason: &str) -> Result<RunManifest, String> {
    run.prelaunch_failure = Some(PrelaunchFailure { phase, recorded_at: now_ms() });
    let id = run.run_id.clone();
    let error = request_in(dir, run, || Err(reason.to_owned())).err().ok_or("Prelaunch refusal unexpectedly admitted a worker")?;
    let saved = load_manifest_in(dir, &id).map_err(|_| error.clone())?;
    if !prelaunch_refused(&saved) { return Err(error); }
    Ok(saved)
}

/// Migrate a recorded pre-execution refusal only while the exact prior
/// reservation is still current. A concurrent resume cannot be overwritten.
pub(crate) fn refuse_spend_after_in(
    dir: &Path, prior: &RunManifest, mut next: RunManifest, reason: &str,
) -> Result<RunManifest, String> {
    let _lock = StoreLock::acquire(dir)?;
    let current = continuation_in(dir, prior.ticket.as_deref().ok_or("Missing refusal ticket")?)?
        .ok_or("Prelaunch predecessor is no longer current")?;
    if current.run_id != prior.run_id || current.revision != prior.revision
        || !prelaunch_refused(&current) || !crate::spend::is_concurrent_refusal(reason)
    { return Err("Prelaunch predecessor or refusal changed; preserve reservation".into()); }
    next.prelaunch_failure = Some(PrelaunchFailure { phase: PrelaunchPhase::SpendAdmission, recorded_at: now_ms() });
    let id = next.run_id.clone();
    let error = request_locked_in(dir, next, || Err(reason.to_owned())).err()
        .ok_or("Spend refusal unexpectedly admitted a worker")?;
    let saved = load_manifest_in(dir, &id).map_err(|_| error)?;
    if !prelaunch_refused(&saved) { return Err("Spend refusal proof was not saved".into()); }
    Ok(saved)
}
pub(crate) fn spend_prelaunch_refused(run: &RunManifest) -> bool {
    prelaunch_refused(run) && run.prelaunch_failure.as_ref()
        .is_some_and(|proof| proof.phase == PrelaunchPhase::SpendAdmission)
}

pub(crate) fn prelaunch_refused(run: &RunManifest) -> bool {
    run.prelaunch_failure.is_some() && admission_refused_before_execution(run)
}

pub fn update_in(
    dir: &Path,
    id: &str,
    change: impl FnOnce(&mut RunManifest),
) -> Result<RunManifest, String> {
    let _lock = StoreLock::acquire(dir)?;
    let mut run = load_manifest_in(dir, id)?;
    let old_state = run.state;
    let old_signal = run.last_signal.clone();
    let old_commit = run.last_commit.clone();
    change(&mut run);
    persist_locked(dir, &mut run)?;
    drop(_lock);
    #[cfg(not(test))]
    if old_state != run.state && (run.state.terminal() || run.state == RunState::Blocked) {
        let stopped = run.clone();
        std::thread::spawn(move || {
            if let Err(error) = crate::project_wiki::capture_stop(&stopped) {
                eprintln!("[wiki] stop checkpoint pending: {error}");
            }
        });
    }
    #[cfg(not(test))]
    if old_state != run.state || old_signal != run.last_signal || old_commit != run.last_commit {
        let observed=run.clone();
        std::thread::spawn(move || {
            if let Err(error)=crate::project_wiki::journal::capture_run(&observed) {
                eprintln!("[journal] receipt capture pending: {error}");
            }
        });
    }
    #[cfg(test)]
    let _ = (old_state,old_signal,old_commit);
    Ok(run)
}

/// Final revocation boundary. Admissions use this same lock: a newer writer
/// cannot appear between the last external proof and releasing this lease.
pub fn retire_stopped_in(dir: &Path, id: &str, prove_and_release: impl FnOnce(&RunManifest)->Result<(),String>) -> Result<(),String> {
    let _lock=StoreLock::acquire(dir)?;
    let mut run=load_manifest_in(dir,id)?;
    if run.state!=RunState::Retiring {return Err("run is not retiring".into());}
    let newer=list_ids_in(dir)?.iter().filter_map(|id|load_manifest_in(dir,id).ok()).any(|r|r.kind==RunKind::Agent && r.worktree_path==run.worktree_path && r.started_at>run.started_at && !r.state.terminal());
    if newer {return Err("newer writer exists; refusing lease release".into());}
    if let Err(e)=prove_and_release(&run) {run.state=RunState::Undead;run.last_signal=e.clone();persist_locked(dir,&mut run)?;return Err(e);}
    run.state=RunState::Retired;run.last_signal="stop proved; lease released".into();persist_locked(dir,&mut run)
}
pub fn list_ids_in(dir: &Path) -> Result<Vec<String>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let mut ids = BTreeSet::new();
    for entry in entries {
        let name = entry
            .map_err(|e| e.to_string())?
            .file_name()
            .to_string_lossy()
            .to_string();
        if let Some(id) = name
            .strip_suffix(".run.json")
            .or_else(|| name.strip_suffix(".events.jsonl"))
        {
            validate_id(id)?;
            ids.insert(id.to_string());
        }
    }
    Ok(ids.into_iter().collect())
}

#[derive(Clone, Debug, Default)]
pub struct Proofs {
    pub pid_alive: bool,
    pub session_alive: bool,
    pub capture_bytes: u64,
    pub pid_absent: bool,
    pub session_known: bool,
    pub capture_known: bool,
    pub capture_quiet: bool,
    pub worktree_exists: bool,
    pub branch_matches: bool,
    pub commit: String,
    pub pid: Option<u32>,
    pub process_birth: Option<String>,
    pub exit_code: Option<i32>,
    /// Observed child work accumulated from birth-bound CPU deltas (XNAUT-439).
    /// A ten-minute compile may move none of the other progress readings.
    pub cpu_ms: u64,
    /// None means unknown; Some(empty) is a proven root with no children.
    pub cpu_samples: Option<BTreeMap<u32, ProcessCpuSample>>,
    /// The revision of the run's own ticket, so an agent writing its progress
    /// into the ticket body counts as progress (XNAUT-439).
    pub ticket_revision: u64,
    /// Bytes under the worktree's `.xnaut/` tree: the bundle being written and
    /// the verify evidence beside it (XNAUT-439).
    pub verify_log_bytes: u64,
}
impl Proofs {
    pub fn writers_gone(&self) -> bool {
        !self.pid_alive && !self.session_alive && self.capture_quiet
    }
    /// Did anything this machine can see move since the record was written?
    ///
    /// ONE definition, read by both the verdict and the reconciler. They used
    /// to spell out `grew || commit changed` separately, which was already two
    /// copies of a two-term condition; at six terms, two copies drift, and a
    /// reconciler that disagrees with the verdict about what progress means is
    /// how a run gets failed and then kept in the same pass.
    pub fn progressed(&self, run: &RunManifest) -> bool {
        self.capture_bytes > run.capture_bytes
            || (!self.commit.is_empty() && self.commit != run.last_commit)
            || self.cpu_ms > run.cpu_ms
            || self.ticket_revision > run.ticket_revision
            || self.verify_log_bytes > run.verify_log_bytes
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Keep,
    Running,
    Waiting,
    Done,
    Failed(String),
}
pub(crate) const STALLED_NO_PROGRESS: &str = "stalled: alive but capture, hooks, commits, child CPU, ticket revision and verify log show no progress beyond the window; waiting_on empty";
/// Pure: no filesystem, process, app, repository, agent, or clock lookup.
pub fn verdict(run: &RunManifest, proof: &Proofs, at: i64) -> Verdict {
    if run.state.terminal()
        || matches!(
            run.state,
            RunState::Degraded | RunState::Retiring | RunState::Undead
        )
        || (run.state == RunState::Requested && run.previous_run_id.is_some())
    {
        return Verdict::Keep;
    }
    if let Some(code) = proof.exit_code {
        return if code == 0 {
            Verdict::Done
        } else {
            Verdict::Failed(format!("process exited with status {code}"))
        };
    }
    let grew = proof.capture_bytes > run.capture_bytes;
    let hook_recent = run
        .last_hook_at
        .is_some_and(|t| at.saturating_sub(t) <= GRACE_MS);
    let alive = proof.pid_alive || proof.session_alive || grew || hook_recent;
    if at.saturating_sub(run.started_at) < GRACE_MS {
        return if alive {
            Verdict::Running
        } else {
            Verdict::Keep
        };
    }
    let mut missing = Vec::new();
    if !alive {
        missing.extend([
            "pid does not answer",
            "zellij session absent",
            "capture has not grown",
            "no recent hook",
        ]);
    }
    if !proof.worktree_exists {
        missing.push("worktree absent");
    }
    // A live run on another commit is not abandonment: a mutation check or a
    // bisect detaches HEAD for a minute, and on tron 2026-09-13 that minute
    // marked XNAUT-354's run failed while its agent was finishing the ticket
    // (XNAUT-360). The mismatch proves something only once the writer is
    // gone; a live run that wanders off for good is caught by the progress
    // window below, which sees no new commit on its branch.
    if !alive && !proof.branch_matches {
        missing.push("worktree branch mismatch");
    }
    if !missing.is_empty() {
        return Verdict::Failed(missing.join("; "));
    }
    if run
        .waiting_on
        .as_deref()
        .is_some_and(|w| !w.trim().is_empty())
    {
        return Verdict::Waiting;
    }
    // What counts as progress, and why it is six things and not three.
    //
    // On tron 2026-09-22 this window failed run 01M354JH4E9TVBGJ36H7SNJC5G
    // while its agent sat in `cargo test` on a cold worktree — a ten-minute
    // compile that writes no capture, fires no hook and makes no commit. The
    // same window failed XNAUT-379's first run on 2026-09-14. Both agents were
    // working; the detector was measuring the wrong things.
    //
    // A compile is invisible to capture, hooks and commits, and visible in
    // three other places: the CPU its children burn, the ticket body the agent
    // writes its results into, and the verify evidence growing in the
    // worktree. A run whose children burn CPU is not stalled.
    if !proof.progressed(run) && at.saturating_sub(run.last_progress_at) > PROGRESS_WINDOW_MS {
        return Verdict::Failed(STALLED_NO_PROGRESS.into());
    }
    Verdict::Running
}

// ─── The beacon: liveness from a machine this one cannot inspect (XNAUT-307) ──

/// How long a sandbox may go silent before it is treated as GONE.
///
/// The beacon pongs every 60 s and it pongs UNCONDITIONALLY — an agent that
/// exited still produces `agent_pid: null`, a run with nothing to show still
/// produces the same bytes and the same head. So silence has exactly one
/// meaning, and it is the meaning the reaper acts on: the VM is not there any
/// more. Five minutes is four missed pongs, which is generous for a tunnel
/// over Tailscale and still far inside any window where destroying the box
/// would cost work.
pub const BEACON_LAPSE_MS: i64 = 5 * 60_000;

/// One beacon report from inside a sandbox.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Pong {
    pub run_id: String,
    /// The agent process inside the VM, or `None` once it has exited. `None`
    /// is a REPORT, not an absence of one: the VM is still answering.
    #[serde(default)]
    pub agent_pid: Option<u32>,
    #[serde(default)]
    pub capture_bytes: u64,
    /// `git rev-parse --short HEAD` in the sandbox workspace.
    #[serde(default)]
    pub head: String,
    #[serde(default)]
    pub dirty: u32,
}

/// Has the workspace's HEAD moved since the registry last saw it?
///
/// The registry stores the FULL sha (`RunManifest::requested` runs
/// `rev-parse HEAD`) and the beacon reports the SHORT one, because that is
/// what the ticket specifies and what a human reads in a log. Comparing them
/// with `!=` would report movement on every single pong, which would keep a
/// genuinely stalled run looking productive forever — the exact failure this
/// ticket exists to prevent, inverted.
pub fn head_moved(known: &str, reported: &str) -> bool {
    let (known, reported) = (known.trim(), reported.trim());
    if reported.is_empty() {
        // No git in the sandbox, or no commit yet. Absence of evidence.
        return false;
    }
    if known.is_empty() {
        return true;
    }
    !known.starts_with(reported) && !reported.starts_with(known)
}

/// Fold one pong into a run. Pure: no clock, no filesystem, no process.
///
/// Returns whether this pong showed PROGRESS, which is the only thing that
/// advances `last_progress_at`. Every pong advances `last_seen_at`, because
/// that field answers "is the machine there", and the machine is there.
///
/// It deliberately does NOT touch `last_hook_at`. That field means "the AGENT
/// said something about itself" and `verdict` treats it as proof of work; the
/// beacon only ever proves the VM is up. Writing it here would let a beacon
/// pinging beside a dead agent read as an agent making progress, which is a
/// reaper that never fires rather than one that fires too early — a quieter
/// bug than XNAUT-266's, and a more expensive one.
pub fn apply_pong(run: &mut RunManifest, pong: &Pong, at: i64) -> bool {
    run.last_seen_at = at;
    run.pid = pong.agent_pid;
    let grew = pong.capture_bytes > run.capture_bytes;
    let moved = head_moved(&run.last_commit, &pong.head);
    if grew {
        run.capture_bytes = pong.capture_bytes;
    }
    if moved {
        run.last_commit = pong.head.trim().to_string();
    }
    if grew || moved {
        run.last_progress_at = at;
    }
    run.last_signal = format!(
        "beacon: agent {}, capture {} bytes, head {}, {} dirty{}",
        match pong.agent_pid {
            Some(pid) => format!("pid {pid}"),
            None => "gone".into(),
        },
        pong.capture_bytes,
        if pong.head.trim().is_empty() {
            "unknown"
        } else {
            pong.head.trim()
        },
        pong.dirty,
        if grew || moved { "" } else { "; no progress" }
    );
    grew || moved
}

/// Apply a pong to the stored run, under the store lock.
pub fn beacon_in(dir: &Path, pong: &Pong, at: i64) -> Result<(RunManifest, bool), String> {
    let progressed = std::cell::Cell::new(false);
    let run = update_in(dir, &pong.run_id, |run| {
        progressed.set(apply_pong(run, pong, at));
    })?;
    Ok((run, progressed.get()))
}

/// The proofs for a run whose processes live on another machine.
///
/// Pure, and separate from `observe_in` for exactly that reason: everything
/// `observe_in` reads — `pid_answers`, `process_birth`, the capture file's
/// mtime, the local worktree's branch — describes THIS Mac and says nothing
/// true about a VM in Frankfurt. The beacon's last pong is the only evidence
/// there is, and it is enough for the verdict `verdict` already computes.
///
/// The two liveness fields are kept apart on purpose:
/// - `session_alive` is the VM answering, which is what silence contradicts.
/// - `pid_alive` is the AGENT still running inside it, which the beacon
///   reports separately and which goes to `None` while the box stays up.
///
/// `worktree_exists` and `branch_matches` are asserted rather than checked.
/// The worktree that matters is `/workspace` on the far side; the local
/// directory the run was pushed from may legitimately be gone, and failing a
/// live remote agent because a local folder moved would be the same category
/// of mistake as reaping it because it got old.
pub fn remote_proofs(run: &RunManifest, at: i64) -> Proofs {
    let answering = at.saturating_sub(run.last_seen_at) <= BEACON_LAPSE_MS;
    Proofs {
        pid_alive: answering && run.pid.is_some(),
        pid_absent: answering && run.pid.is_none(),
        session_alive: answering,
        session_known: true,
        capture_known: true,
        capture_bytes: run.capture_bytes,
        capture_quiet: !answering,
        worktree_exists: true,
        branch_matches: true,
        commit: run.last_commit.clone(),
        pid: run.pid,
        process_birth: None,
        exit_code: None,
        // The three XNAUT-439 readings are carried through unchanged rather
        // than measured. A sandboxed run's children live on the far side, so
        // this machine's process table and this machine's copy of the worktree
        // say nothing about them; reporting a local zero would read as a fall
        // from the stored mark, which is silence the beacon has not claimed.
        cpu_ms: run.cpu_ms,
        cpu_samples: None,
        ticket_revision: run.ticket_revision,
        verify_log_bytes: run.verify_log_bytes,
    }
}

/// One row of the process table: enough to walk a subtree and read its work.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcessCpuSample {
    pub birth: String,
    pub cpu_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcRow {
    pub birth: String,
    pub pid: u32,
    pub ppid: u32,
    pub cpu_ms: u64,
}

/// `ps` prints accumulated CPU as `[[dd-]hh:]mm:ss.ff`.
///
/// ACCUMULATED, not instantaneous, and that is the whole point. A sample of
/// `%cpu` taken in the gap between two rustc invocations reads zero and proves
/// nothing; a total only ever climbs for a process that is doing work, so two
/// readings a sweep apart answer "did this subtree work?" without needing to
/// catch it in the act.
pub fn parse_cpu_time(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    let (days, rest) = match raw.split_once('-') {
        Some((d, rest)) => (d.parse::<u64>().ok()?, rest),
        None => (0, raw),
    };
    let mut parts = rest.split(':').rev();
    let secs: f64 = parts.next()?.trim().parse().ok()?;
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    let unit = |p: Option<&str>| -> Option<u64> {
        match p {
            Some(v) => v.trim().parse().ok(),
            None => Some(0),
        }
    };
    let mins = unit(parts.next())?;
    let hours = unit(parts.next())?;
    if parts.next().is_some() {
        return None;
    }
    Some(((days * 24 + hours) * 3600 + mins * 60) * 1000 + (secs * 1000.0).round() as u64)
}

pub fn parse_proc_row(line: &str) -> Option<ProcRow> {
    let mut field = line.split_whitespace();
    let pid = field.next()?.parse().ok()?;
    let ppid = field.next()?.parse().ok()?;
    let cpu_ms = parse_cpu_time(field.next()?)?;
    let birth = field.collect::<Vec<_>>().join(" ");
    if birth.is_empty() { return None; }
    Some(ProcRow { pid, ppid, cpu_ms, birth })
}

/// Pure: accumulated CPU of everything BELOW `root`, root itself excluded.
///
/// The exclusion is load-bearing. A run's own CLI process burns a little CPU
/// every time it polls, so counting it would make every run look busy forever
/// and this detector would never fire again — the opposite bug, and the worse
/// one, because a genuinely abandoned run would then hold its ticket for good.
/// What proves work is a CHILD burning CPU: cargo, rustc, node, playwright. An
/// idle shell under the session accumulates nothing, so including it is free.
#[cfg(test)]
fn descendant_cpu_ms(rows: &[ProcRow], root: u32) -> u64 {
    descendant_cpu_samples(rows, root, "birth").unwrap_or_default().values()
        .fold(0u64, |total, sample| total.saturating_add(sample.cpu_ms))
}

/// Bind the whole subtree to the recorded root birth in this same snapshot.
/// A reused PID or unreadable table is not an empty, valid baseline.
fn descendant_cpu_samples(
    rows: &[ProcRow], root: u32, birth: &str,
) -> Option<BTreeMap<u32, ProcessCpuSample>> {
    let birth = birth.split_whitespace().collect::<Vec<_>>().join(" ");
    if root == 0 || birth.is_empty()
        || !rows.iter().any(|row| row.pid == root && row.birth == birth)
    { return None; }
    let mut samples = BTreeMap::new();
    let mut frontier = vec![root];
    let mut seen = BTreeSet::from([root]);
    while let Some(parent) = frontier.pop() {
        for row in rows.iter().filter(|r| r.ppid == parent) {
            if !seen.insert(row.pid) { continue; }
            samples.insert(row.pid, ProcessCpuSample { birth: row.birth.clone(), cpu_ms: row.cpu_ms });
            frontier.push(row.pid);
        }
    }
    Some(samples)
}

fn child_cpu_progress(
    run: &RunManifest, rows: &[ProcRow], pid: Option<u32>, birth: Option<&str>, alive: bool,
) -> (u64, Option<BTreeMap<u32, ProcessCpuSample>>) {
    let current = if alive {
        pid.zip(birth).and_then(|(pid, birth)| descendant_cpu_samples(rows, pid, birth))
    } else { None };
    let delta = current.as_ref().map_or(0, |samples| {
        samples.iter().fold(0u64, |total, (pid, sample)| {
            let before = run.cpu_samples.get(pid).filter(|old| old.birth == sample.birth)
                .map_or(0, |old| old.cpu_ms);
            total.saturating_add(sample.cpu_ms.saturating_sub(before))
        })
    });
    (run.cpu_ms.saturating_add(delta), current)
}

pub fn process_table() -> Vec<ProcRow> {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,time=,lstart="])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(parse_proc_row)
        .collect()
}

/// What every run in one sweep pass reads from the machine, sampled ONCE.
///
/// This exists for cost, and the cost is not hypothetical. `reconcile_in`
/// observes every non-done run, which on this machine's registry is 121 of 211
/// manifests — 109 of them already failed. Sampling the process table inside
/// `observe_in` would fork `ps` 121 times and reload settings 121 times on
/// every pass, which is precisely the per-call work that put 116 git processes
/// at 786% CPU on the control repo in XNAUT-432. One sample per pass instead.
///
/// Staleness is irrelevant here: the readings are compared against a record
/// written sweeps earlier, so a snapshot a few milliseconds old answers the
/// same question as a fresh one.
pub struct Machine {
    pub rows: Vec<ProcRow>,
    pub control_repo: Option<PathBuf>,
}
impl Machine {
    pub fn sample() -> Self {
        Self {
            rows: process_table(),
            control_repo: crate::project_management::repo_path_now(),
        }
    }
    fn ticket_revision(&self, project: &str, ticket: Option<&str>) -> u64 {
        match (self.control_repo.as_deref(), ticket) {
            (Some(repo), Some(ticket)) => ticket_revision_in(repo, project, ticket),
            _ => 0,
        }
    }
}

/// Total bytes of the worktree's `.xnaut/` tree: the bundle an agent is
/// writing, the verify evidence beside it, its measurements.
///
/// That tree is gitignored by design, which is exactly why it is worth
/// reading — it moves while no commit does, through the same fifteen minutes
/// this detector used to read as abandonment.
///
/// `file_type()` does not follow symlinks, so a link is neither file nor
/// directory here and a loop cannot be walked into. The depth bound is belt
/// and braces: a sweep must not be the thing that hangs.
pub fn verify_log_bytes_in(worktree: &Path) -> u64 {
    // An empty worktree path would make this `./.xnaut` — RELATIVE to whatever
    // directory the app happens to be running in, so a run with no worktree
    // would be credited with the bytes of some other tree entirely. `verdict`
    // fails such a run as "worktree absent" before it reads progress, but the
    // bogus figure would still be stored as the high-water mark and would then
    // hide real movement if the path were ever repaired.
    if worktree.as_os_str().is_empty() {
        return 0;
    }
    fn walk(dir: &Path, depth: u32, total: &mut u64) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => walk(&entry.path(), depth - 1, total),
                Ok(t) if t.is_file() => {
                    if let Ok(meta) = entry.metadata() {
                        *total = total.saturating_add(meta.len());
                    }
                }
                _ => {}
            }
        }
    }
    let mut total = 0;
    walk(&worktree.join(".xnaut"), 8, &mut total);
    total
}

/// The revision of the run's own ticket, or 0 when there is no ticket, no
/// configured control repo, or no readable record.
///
/// An agent that is working writes its progress into the ticket body — the
/// launch prompt instructs it to, and the run on 2026-09-22 did exactly that
/// with its test results. That write is visible to this machine without the
/// agent's cooperation, which is what makes it usable as a liveness proof.
///
/// The control repo records no ACTOR on a ticket edit (the `ticket.updated`
/// event carries type, ticket, project, at, revision and title, and no
/// writer), so this cannot prove the run's OWN agent made the edit rather than
/// the owner or NautBot. It is scoped to the run's own ticket, which is the
/// closest attribution the record allows, and a wrong reading here errs toward
/// alive — the direction that does not kill a working agent.
pub fn ticket_revision_in(repo: &Path, project: &str, ticket: &str) -> u64 {
    // Both halves become path components, so anything that could climb out of
    // the tickets directory is refused rather than sanitised.
    let safe = |s: &str| {
        !s.is_empty()
            && s.len() <= 64
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    if !safe(project) || !safe(ticket) {
        return 0;
    }
    let path = repo
        .join("projects")
        .join(project)
        .join("tickets")
        .join(format!("{ticket}.json"));
    // A partial parse, not `TicketRecord`: a field this build does not know
    // must not turn a live agent's ticket edit into a zero.
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("revision")?.as_u64())
        .unwrap_or(0)
}

#[cfg(not(windows))]
pub fn process_birth(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .output()
        .ok()?;
    let birth = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !birth.is_empty()).then_some(birth)
}
/// Use the native creation timestamp on Windows; a Unix `ps` process cannot
/// identify Windows PIDs. Query and liveness share one handle, preventing PID
/// reuse between the checks. Win32 contract: GetProcessTimes / WaitForSingleObject.
#[cfg(windows)]
pub fn process_birth(pid: u32) -> Option<String> {
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Default)]
    struct FileTime { low: u32, high: u32 }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetProcessTimes(handle: *mut c_void, creation: *mut FileTime,
            exit: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    if pid == 0 { return None; }
    // PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE; never request write access.
    let handle = unsafe { OpenProcess(0x1000 | 0x100000, 0, pid) };
    if handle.is_null() { return None; }
    let (mut creation, mut exit, mut kernel, mut user) =
        (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
    // All pointers refer to initialized FILETIME layouts and the handle is owned
    // here. WAIT_TIMEOUT (0x102) proves it has not signalled process exit.
    let birth = unsafe {
        let read = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
        let alive = WaitForSingleObject(handle, 0) == 0x102;
        CloseHandle(handle);
        (read != 0 && alive).then(|| format!("windows-filetime:{}",
            ((creation.high as u64) << 32) | creation.low as u64))
    };
    birth
}
fn pid_answers(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(pid as i32, 0) == 0
                || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    }
    #[cfg(not(unix))]
    {
        process_birth(pid).is_some()
    }
}
/// The launcher writes the ACTUAL CLI pid and its birth stamp before exec.
/// Zellij's viewport pid and the supervising app pid never prove the CLI.
/// Observe one run, sampling the machine for it alone.
///
/// Convenient for a single-run caller and for tests. A sweep that observes many
/// runs must sample once with `Machine::sample()` and call `observe_with`, or it
/// pays for a `ps` and a settings load per run.
pub fn observe_in(dir: &Path, run: &RunManifest, live_sessions: &[String]) -> Proofs {
    observe_with(&Machine::sample(), dir, run, live_sessions)
}
pub fn observe_with(
    machine: &Machine,
    dir: &Path,
    run: &RunManifest,
    live_sessions: &[String],
) -> Proofs {
    // A sandboxed run is observed by its beacon, never by this machine's
    // process table (XNAUT-307). Routed here rather than at the call sites so
    // the two production closures in `sweep.rs` and the one in
    // `project_management.rs` cannot disagree about it.
    if run.remote_env.is_some() {
        return remote_proofs(run, now_ms());
    }
    let stamp = pid_path(dir, &run.run_id)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let mut lines = stamp.lines();
    let stamped_pid = lines.next().and_then(|p| p.parse::<u32>().ok());
    let stamped_birth = lines
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let pid = run.pid.or(stamped_pid);
    let birth = run.process_birth.clone().or(stamped_birth);
    let alive = pid
        .zip(birth.as_ref())
        .is_some_and(|(p, b)| pid_answers(p) && process_birth(p).as_ref() == Some(b));
    // The three readings a compile DOES move (XNAUT-439). The CPU walk is
    // rooted at the stamped CLI pid, which is the launcher's record of the real
    // process; zellij's viewport pid would root it in the wrong place and sweep
    // in every other pane's children.
    //
    // Taken only for a run that can still receive a fresh verdict. A terminal
    // one cannot — `verdict` returns `Keep` and `reconcile_in` reads only its
    // capture baseline — and skipping those is most of the saving: 109 of the
    // 211 manifests on this machine's registry are already failed.
    let (cpu_ms, cpu_samples) = if run.state.terminal() { (0, None) } else {
        child_cpu_progress(run, &machine.rows, pid, birth.as_deref(), alive)
    };
    let (ticket_revision, verify_log_bytes) = if run.state.terminal() {
        (0, 0)
    } else {
        (
            machine.ticket_revision(&run.project, run.ticket.as_deref()),
            verify_log_bytes_in(Path::new(&run.worktree_path)),
        )
    };
    Proofs {
        pid_alive: alive,
        pid_absent: pid.is_some_and(|p| !pid_answers(p)),
        session_known: true,
        capture_known: run
            .output_path
            .as_ref()
            .is_none_or(|p| match std::fs::metadata(p) {
                Ok(_) => true,
                Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            }),
        session_alive: run
            .zellij_session
            .as_ref()
            .is_some_and(|s| live_sessions.contains(s)),
        capture_bytes: run
            .output_path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(run.capture_bytes),
        capture_quiet: run
            .output_path
            .as_ref()
            .map(|p| {
                std::fs::metadata(p)
                    .map(|m| {
                        m.len() <= run.capture_bytes
                            && m.modified()
                                .ok()
                                .and_then(|t| t.elapsed().ok())
                                .is_some_and(|age| age.as_millis() >= GRACE_MS as u128)
                    })
                    .unwrap_or(true)
            })
            .unwrap_or(true),
        worktree_exists: Path::new(&run.worktree_path).is_dir(),
        branch_matches: run.branch.is_empty()
            || git_value(&run.worktree_path, &["symbolic-ref", "--short", "HEAD"]) == run.branch,
        commit: git_value(&run.worktree_path, &["rev-parse", "HEAD"]),
        exit_code: std::fs::read_to_string(dir.join(format!("{}.exit", run.run_id)))
            .ok()
            .and_then(|s| s.trim().parse().ok()),
        cpu_ms,
        cpu_samples,
        ticket_revision,
        verify_log_bytes,
        pid,
        process_birth: birth,
    }
}
/// Reconcile one record with a fresh proof under the store lock. Failed runs
/// remain eligible for ticket-return retries; done/retired runs are protected.
pub fn reconcile_in(
    dir: &Path,
    at: i64,
    mut observe: impl FnMut(&RunManifest) -> Proofs,
) -> Result<Vec<(RunManifest, Proofs)>, String> {
    let _lock = StoreLock::acquire(dir)?;
    let mut failed = vec![];
    for id in list_ids_in(dir)? {
        let mut run = load_manifest_in(dir, &id)?;
        if matches!(
            run.state,
            RunState::Done | RunState::Retired | RunState::Retiring | RunState::Undead
        ) {
            continue;
        }
        let proof = observe(&run);
        if run.state != RunState::Failed {
            let decision = verdict(&run, &proof, at);
            if proof.pid_alive || proof.session_alive {
                run.last_seen_at = at;
            }
            if proof.progressed(&run) {
                run.last_progress_at = at;
            }
            run.capture_bytes = proof.capture_bytes;
            run.last_commit = proof.commit.clone();
            // CPU is already accumulated from per-process deltas. Other
            // counters retain high-water marks across temporarily absent data.
            run.cpu_ms = run.cpu_ms.max(proof.cpu_ms);
            if let Some(samples) = &proof.cpu_samples {
                run.cpu_samples = samples.clone();
            }
            run.ticket_revision = run.ticket_revision.max(proof.ticket_revision);
            run.verify_log_bytes = run.verify_log_bytes.max(proof.verify_log_bytes);
            run.pid = proof.pid;
            run.process_birth = proof.process_birth.clone();
            match decision {
                Verdict::Keep => {}
                Verdict::Running => {
                    run.state = RunState::Running;
                }
                Verdict::Waiting => {
                    run.state = RunState::Blocked;
                }
                Verdict::Done => {
                    run.state = RunState::Done;
                    run.last_signal = "process exited successfully".into();
                }
                Verdict::Failed(reason) => {
                    run.state = RunState::Failed;
                    run.last_signal = reason;
                }
            }
            persist_locked(dir, &mut run)?;
        } else if run.capture_bytes != proof.capture_bytes {
            // A failed writer may stop later. Advance the capture baseline
            // so its final buffered output does not defer return forever.
            run.capture_bytes = proof.capture_bytes;
            persist_locked(dir, &mut run)?;
        }
        if run.state == RunState::Failed && !run.ticket_returned {
            failed.push((run, proof));
        }
    }
    Ok(failed)
}
/// Trusted session events and explicit inbox waits update the durable record,
/// including repeated Working hooks (which the UI deduplicates).
pub fn signal_session_in(
    dir: &Path,
    session: &str,
    status: Option<RunState>,
    waiting: Option<Option<String>>,
    at: i64,
) -> Result<(), String> {
    let _lock = StoreLock::acquire(dir)?;
    for id in list_ids_in(dir)? {
        let mut run = load_manifest_in(dir, &id)?;
        if run.pty_session.as_deref() != Some(session)
            && run.zellij_session.as_deref() != Some(session)
        {
            continue;
        }
        if run.state.terminal() || matches!(run.state, RunState::Retiring | RunState::Undead) {
            continue;
        }
        run.last_hook_at = Some(at);
        run.last_seen_at = at;
        run.last_progress_at = at;
        if let Some(status) = status.filter(|_| run.state != RunState::Degraded) {
            run.state = status;
        }
        if let Some(waiting) = waiting.clone() {
            // Polling an inbox can itself fire Working hooks. Only the answer
            // to that exact inbox request clears its explicit wait.
            let inbox_wait = run
                .waiting_on
                .as_deref()
                .is_some_and(|w| w.starts_with("in-"));
            if !inbox_wait || waiting.as_deref().is_some_and(|w| w.starts_with("in-")) {
                run.waiting_on = waiting;
            }
        }
        if run.state != RunState::Degraded {
            run.last_signal = "trusted session signal".into();
        }
        persist_locked(dir, &mut run)?;
    }
    Ok(())
}

/// Bind a handback to the authenticated session, never to the app's own env.
/// Legacy sessions with no registry entry may still file an unbound handback.
pub fn bind_handback_in(
    dir: &Path,
    session: Option<&str>,
    handback: &mut crate::handback::Handback,
) -> Result<(), String> {
    let Some(session) = session else {
        return if handback.run_id.is_some() {
            Err("a run-bound handback needs an authenticated session".into())
        } else {
            Ok(())
        };
    };
    let mut matches = Vec::new();
    for id in list_ids_in(dir)? {
        let run = load_manifest_in(dir, &id)?;
        if run.kind == RunKind::Agent
            && run.ticket.as_deref() == Some(handback.ticket.as_str())
            && run.agent_handle == handback.from.trim_start_matches('@').to_ascii_lowercase()
            && (run.pty_session.as_deref() == Some(session)
                || run.zellij_session.as_deref() == Some(session))
        {
            matches.push(run);
        }
    }
    if let Some(id) = &handback.run_id {
        if !matches.iter().any(|r| &r.run_id == id) {
            return Err(
                "handback run does not match the authenticated session, ticket and handle".into(),
            );
        }
    } else {
        handback.run_id = matches
            .into_iter()
            .max_by_key(|r| (r.started_at, r.run_id.clone()))
            .map(|r| r.run_id);
    }
    Ok(())
}

/// Pure identity check. A ticket's current status or another run's report is
/// not evidence that this run finished.
pub fn handback_matches(run: &RunManifest, handback: &crate::handback::Handback) -> bool {
    run.kind == RunKind::Agent
        && handback.run_id.as_deref() == Some(run.run_id.as_str())
        && run.ticket.as_deref() == Some(handback.ticket.as_str())
        && run.agent_handle == handback.from.trim_start_matches('@').to_ascii_lowercase()
}

fn mark_completed(run: &mut RunManifest, reason: &str) -> bool {
    if run.state.terminal() || matches!(run.state, RunState::Retiring | RunState::Undead) {
        return false;
    }
    run.state = RunState::Done;
    run.waiting_on = None;
    run.last_signal = reason.into();
    true
}

/// Serialize the durable PM filing and its completion signal with reconcile.
/// PM's ticket-update path releases its mutation lock before signalling here,
/// so this registry -> PM lock order cannot invert against a ticket update.
/// If the app dies after store succeeds, recovery reads the exact stored run id.
pub fn record_handback_in<T>(
    dir: &Path,
    handback: &crate::handback::Handback,
    store: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let Some(id) = &handback.run_id else {
        return store();
    };
    let _lock = StoreLock::acquire(dir)?;
    let mut run = load_manifest_in(dir, id)?;
    if !handback_matches(&run, handback) {
        return Err("handback run does not match its ticket and handle".into());
    }
    let stored = store()?;
    if mark_completed(&mut run, "accepted typed handback") {
        persist_locked(dir, &mut run)?;
    }
    Ok(stored)
}

/// Return the original failure event only for a continuous, identity-bound
/// liveness-only failure after this exact publication was already observed.
/// The journal remains intact; a recovered task result never asserts PID exit.
fn reviewer_liveness_failure_in(dir: &Path, run: &RunManifest, head: &str) -> Result<Option<String>, String> {
    if run.state != RunState::Failed || run.last_commit != head || run.admission_refused
        || run.prelaunch_failure.is_some() || run.previous_run_id.is_some() || run.next_run_id.is_some()
        || run.retirement.is_some() || run.ticket_returned
    { return Ok(None); }
    match std::fs::read_to_string(dir.join(format!("{}.exit", run.run_id))) {
        Ok(code) if code.trim() == "0" => {},
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(e) => return Err(e.to_string()),
    }
    let (_, valid_end) = replay_events_in(dir, &run.run_id)?;
    if valid_end == 0 { return Ok(None); }
    let body = std::fs::read(journal_path(dir, &run.run_id)).map_err(|e| e.to_string())?;
    let mut previous: Option<RunManifest> = None;
    let mut failure = None;
    for line in body[..valid_end].split(|b| *b == b'\n').filter(|s| !s.iter().all(u8::is_ascii_whitespace)) {
        let event: RunEvent = serde_json::from_slice(line).map_err(|e| e.to_string())?;
        let observed = &event.run;
        if observed.project != run.project || observed.agent_handle != run.agent_handle
            || observed.kind != run.kind || observed.ticket != run.ticket || observed.user_conversation
            || observed.worktree_path != run.worktree_path || observed.branch != run.branch
            || observed.remote_env != run.remote_env || observed.admission_refused
            || observed.prelaunch_failure.is_some() || observed.previous_run_id.is_some()
            || observed.next_run_id.is_some() || observed.retirement.is_some() || observed.ticket_returned
        { return Ok(None); }
        if observed.state == RunState::Failed {
            if observed.last_commit != head || !matches!(observed.last_signal.as_str(),
                "stalled: alive but capture, hooks and commits show no progress beyond the window; waiting_on empty"
                | STALLED_NO_PROGRESS
                | "pid does not answer; zellij session absent; capture has not grown; no recent hook")
            { return Ok(None); }
            if failure.is_none() {
                if !previous.as_ref().is_some_and(|p| p.state == RunState::Running && p.last_commit == head) {
                    return Ok(None);
                }
                failure = Some(event.event_id);
            }
        } else if failure.is_some() || matches!(observed.state, RunState::Done | RunState::Retired | RunState::Retiring | RunState::Undead) {
            return Ok(None);
        }
        previous = Some(event.run);
    }
    Ok(failure)
}

/// A ticketless independent reviewer hands evidence to its parent delivery,
/// not to PM. Complete that task under the same contract as author handbacks;
/// an attached interactive process is not claimed to have exited, and the
/// parent's independent verdict is neither accepted nor changed here.
pub(crate) fn record_review_handback_in(
    dir: &Path, transfer: &crate::repository_transfer::Transfer,
    handback: &crate::handback::Handback, published_head: &str,
) -> Result<bool, String> {
    let _lock = StoreLock::acquire(dir)?;
    let mut run = load_manifest_in(dir, &transfer.run_id)?;
    let environment = match transfer.worker { crate::worker_bootstrap::Target::ExeDev => "exe-dev", _ => "gitvm" };
    if !matches!(run.kind, RunKind::Agent | RunKind::Review) || run.user_conversation
        || run.ticket.is_some() || transfer.ticket.is_some() || transfer.review_parent.is_none()
        || run.project != transfer.project || run.agent_handle != transfer.handle
        || run.worktree_path != transfer.local_path || run.remote_env.as_deref() != Some(environment)
        || handback.run_id.as_deref() != Some(run.run_id.as_str()) || handback.from != run.agent_handle
        || !crate::handback::review(handback).is_reviewable()
        || published_head.len() != 40 || !published_head.bytes().all(|b| b.is_ascii_hexdigit())
    { return Err("Reviewer handback does not match its native assignment and publication".into()); }
    let recovery = if transfer.state == "review" { reviewer_liveness_failure_in(dir, &run, published_head)? } else { None };
    if let Some(event) = recovery {
        run.state = RunState::Done;
        run.waiting_on = None;
        run.last_signal = format!("review task published; typed handback accepted after historical liveness failure {event} (parent verdict remains separate; process exit not asserted)");
    } else if !mark_completed(&mut run, "review task published; typed handback accepted (parent verdict remains separate)") {
        return Ok(false);
    }
    run.last_commit = published_head.into();
    persist_locked(dir, &mut run)?;
    Ok(true)
}

/// Recover the PM-committed half of a filing interrupted before the registry
/// journal was written. Runs without their own accepted handback stay unchanged.
pub fn recover_handbacks_in(
    dir: &Path,
    tickets: &[crate::project_management::TicketRecord],
) -> Result<(), String> {
    let _lock = StoreLock::acquire(dir)?;
    for id in list_ids_in(dir)? {
        let mut run = load_manifest_in(dir, &id)?;
        let completed = tickets
            .iter()
            .filter(|t| Some(t.id.as_str()) == run.ticket.as_deref())
            .filter_map(|t| t.handback.as_ref())
            .any(|h| handback_matches(&run, h) && crate::handback::review(h).is_reviewable());
        if completed && mark_completed(&mut run, "recovered accepted typed handback") {
            persist_locked(dir, &mut run)?;
        }
    }
    Ok(())
}

/// Record the transition while its newest ticket run is demonstrably alive.
/// Never infer completion later from a board status that can outlive that run.
pub fn ticket_completed_in(
    dir: &Path,
    ticket: &str,
    owner: &str,
    before: &str,
    after: &str,
    mut observe: impl FnMut(&RunManifest) -> Proofs,
) -> Result<(), String> {
    if before != "in_progress" || !matches!(after, "done" | "review") {
        return Ok(());
    }
    let _lock = StoreLock::acquire(dir)?;
    let mut runs = Vec::new();
    for id in list_ids_in(dir)? {
        let run = load_manifest_in(dir, &id)?;
        if run.kind == RunKind::Agent && run.ticket.as_deref() == Some(ticket) {
            runs.push(run);
        }
    }
    if let Some(mut run) = runs
        .into_iter()
        .max_by_key(|r| (r.started_at, r.run_id.clone()))
    {
        if run.agent_handle == owner.trim_start_matches('@').to_ascii_lowercase() {
            let proof = observe(&run);
            if (proof.pid_alive || proof.session_alive)
                && mark_completed(
                    &mut run,
                    "ticket left in_progress for done/review while alive",
                )
            {
                persist_locked(dir, &mut run)?;
            }
        }
    }
    Ok(())
}

/// Wrap the CLI, not its viewport. Birth and exit records are structured
/// launcher evidence, not interpretation of provider capture text.
pub fn launch_argv_in(dir: &Path, id: &str, argv: &[String]) -> Result<Vec<String>, String> {
    validate_id(id)?;
    if argv.is_empty() {
        return Err("empty run command".into());
    }
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let pid = quote(&pid_path(dir, id)?.to_string_lossy());
    let exit = quote(&dir.join(format!("{id}.exit")).to_string_lossy());
    let command = argv.iter().map(|s| quote(s)).collect::<Vec<_>>().join(" ");
    let inner =
        format!("printf '%s\\n' \"$$\" >{pid}; ps -p \"$$\" -o lstart= >>{pid}; exec {command}");
    Ok(vec![
        "/bin/sh".into(),
        "-c".into(),
        format!(
            "/bin/sh -c {}; code=$?; printf '%s\\n' \"$code\" >{exit}; exit \"$code\"",
            quote(&inner)
        ),
    ])
}

/// Serialize the external ticket acknowledgement with admissions and signals.
/// A newer local run for the ticket takes precedence even before its dispatch
/// note reaches PM. The PM revision check protects concurrent board edits.
pub fn finish_failed_in(
    dir: &Path,
    id: &str,
    action: impl FnOnce(&RunManifest) -> Result<bool, String>,
) -> Result<(), String> {
    let _lock = StoreLock::acquire(dir)?;
    let mut run = load_manifest_in(dir, id)?;
    if run.state != RunState::Failed || run.ticket_returned {
        return Ok(());
    }
    for other_id in list_ids_in(dir)? {
        if other_id == id {
            continue;
        }
        let other = load_manifest_in(dir, &other_id)?;
        if (other.ticket == run.ticket || other.worktree_path == run.worktree_path)
            && other.started_at >= run.started_at
            && (!other.state.terminal() || other.pty_session.is_some())
        {
            return Ok(());
        }
    }
    if action(&run)? {
        run.ticket_returned = true;
        persist_locked(dir, &mut run)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn completed_viewport_does_not_hold_worker_spend_capacity() {
        let mut author = run();
        author.pty_session = Some("author-viewport".into());
        author.state = RunState::Done;
        let sessions = vec![("author-viewport".into(), "codex".into(), None), ("unregistered-live".into(), "other".into(), None)];
        assert_eq!(live_viewport_count(std::slice::from_ref(&author), &sessions, &[]), 1);
        for state in [RunState::Running, RunState::Starting, RunState::Blocked, RunState::Failed, RunState::Undead] {
            author.state = state;
            assert_eq!(live_viewport_count(std::slice::from_ref(&author), &sessions, &[]), 2, "{state:?}");
        }
        author.state = RunState::Done;
        let mut another = author.clone();
        another.run_id = "conflicting-identity".into();
        assert_eq!(live_viewport_count(&[author.clone(), another], &sessions, &[]), 2);
        author.agent_handle = "different-owner".into();
        assert_eq!(live_viewport_count(&[author], &sessions, &[]), 2);
        assert_eq!(live_viewport_count(&[], &sessions, &[]), 2);
    }

    #[test]
    fn adopted_remote_viewport_requires_completed_native_and_exact_transfer_binding() {
        let mut author = run(); author.state = RunState::Done;
        author.remote_env = Some("exe-dev".into());
        let session = crate::sandbox::launch_env::repository_session_name(&author.agent_handle, &author.run_id);
        let sessions = vec![(session, author.agent_handle.clone(), Some("exe-dev".into()))];
        let transfer: crate::repository_transfer::Transfer = serde_json::from_value(serde_json::json!({
            "run_id":author.run_id,"project":author.project,"ticket":author.ticket,"handle":author.agent_handle,
            "local_path":author.worktree_path,"remote":"ssh://fixture/repo.git","source_sha":"head","base":"main","branch":"task",
            "workdir":format!("agents/runs/{}",author.run_id),"artifacts":"artifacts","state":"review","pr_url":null,"error":null
        })).unwrap();
        let rows = std::slice::from_ref(&author);
        assert_eq!(live_viewport_count(rows, &sessions, std::slice::from_ref(&transfer)), 0);
        assert_eq!(live_viewport_count(rows, &sessions, &[]), 1);
        for field in ["run_id", "project", "ticket", "handle", "local_path", "workdir", "state"] {
            let mut bad = serde_json::to_value(&transfer).unwrap(); bad[field] = serde_json::json!("unbound");
            assert_eq!(live_viewport_count(rows, &sessions, &[serde_json::from_value(bad).unwrap()]), 1, "{field}");
        }
        assert_eq!(live_viewport_count(rows, &sessions, &[transfer.clone(), transfer.clone()]), 1);
        let mut wrong_env = sessions.clone(); wrong_env[0].2 = Some("gitvm".into());
        assert_eq!(live_viewport_count(rows, &wrong_env, std::slice::from_ref(&transfer)), 1);
        // A.json carrying B's payload cannot substitute for missing B.json.
        let dir = directory("misfiled-viewport-proof");
        let mut other = author.clone(); other.run_id = "another-native-run".into();
        for run in [&author, &other] {
            std::fs::write(dir.join(format!("{}.run.json",run.run_id)), serde_json::to_vec(run).unwrap()).unwrap();
        }
        let mut misplaced = transfer.clone(); misplaced.run_id = other.run_id.clone();
        misplaced.workdir = format!("agents/runs/{}",other.run_id);
        let store = dir.join("repository-transfers"); std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join(format!("{}.json",author.run_id)), serde_json::to_vec(&misplaced).unwrap()).unwrap();
        let other_sessions = vec![(crate::sandbox::launch_env::repository_session_name(&other.agent_handle,&other.run_id),other.agent_handle.clone(),Some("exe-dev".into()))];
        assert_eq!(live_viewport_count_in(&dir, &other_sessions).unwrap(), 1);
        std::fs::write(store.join(format!("{}.json",other.run_id)), b"{broken").unwrap();
        assert_eq!(live_viewport_count_in(&dir, &other_sessions).unwrap(), 1);
        std::fs::write(store.join(format!("{}.json",other.run_id)), serde_json::to_vec(&misplaced).unwrap()).unwrap();
        assert_eq!(live_viewport_count_in(&dir, &other_sessions).unwrap(), 0);
        std::fs::remove_dir_all(dir).unwrap();
        author.state = RunState::Running;
        assert_eq!(live_viewport_count(&[author], &sessions, &[transfer]), 1);
    }

    /// Exercise each production lease against a separate process, including
    /// Windows where the previous cfg(unix) implementation acquired no lock.
    pub(crate) fn cross_process_lock_fixture<L>(
        name: &str, lock_path: impl Fn(&Path) -> PathBuf, acquire: impl Fn(&Path) -> L,
    ) {
        if std::env::var("XNAUT_TEST_LOCK_CASE").as_deref() == Ok(name) {
            let root = PathBuf::from(std::env::var_os("XNAUT_TEST_LOCK_ROOT").unwrap());
            let file = std::fs::OpenOptions::new().read(true).write(true).open(lock_path(&root)).unwrap();
            let blocked = match file.try_lock() {
                Ok(()) => { file.unlock().unwrap(); false },
                Err(std::fs::TryLockError::WouldBlock) => true,
                Err(std::fs::TryLockError::Error(error)) => panic!("contender lock error: {error}"),
            };
            assert_eq!(blocked, std::env::var("XNAUT_TEST_LOCK_HELD").unwrap() == "true");
            return;
        }
        let root = std::env::temp_dir().join(format!("xnaut-portable-lock-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let check = |held: bool| {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture"])
                .env("XNAUT_TEST_LOCK_CASE", name).env("XNAUT_TEST_LOCK_ROOT", &root)
                .env("XNAUT_TEST_LOCK_HELD", held.to_string()).output().unwrap();
            assert!(output.status.success(), "contender failed: {} {}",
                String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"), "contender test did not execute");
        };
        let holder = acquire(&root);
        check(true);
        drop(holder);
        check(false);
        // Reacquisition proves the old process did not leave a stale lease.
        drop(acquire(&root));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn portable_registry_lock_excludes_processes_and_releases_on_drop() {
        cross_process_lock_fixture(
            "run_control::tests::portable_registry_lock_excludes_processes_and_releases_on_drop",
            |root| root.join(".lock"), |root| StoreLock::acquire(root).unwrap(),
        );
    }

    pub(crate) fn run() -> RunManifest {
        RunManifest {
            schema_version: SCHEMA_VERSION,
            run_id: new_id(1_000),
            kind: RunKind::Agent,
            user_conversation: false,
            ticket: Some("XNAUT-900".into()),
            project: "XNAUT".into(),
            machine: "test".into(),
            instance: "inst-test".into(),
            role: "fleet".into(),
            app_version: "0.0.0-test".into(),
            initiated_by: String::new(),
            origin_thread_id: String::new(),
            agent_handle: "codex".into(),
            runtime_id: "codex".into(),
            model: None,
            worktree_path: "/not-needed-by-the-pure-verdict".into(),
            branch: "agent/test".into(),
            pty_session: Some("test-session".into()),
            zellij_session: None,
            output_path: None,
            remote_env: None,
            owner_pid: 1,
            pid: None,
            process_birth: None,
            state: RunState::Running,
            previous_run_id: None,
            next_run_id: None,
            retirement: None,
            undead_notified: false,
            admission_refused: false,
            prelaunch_failure: None,
            findings_reservation_root: None,
            started_at: 1_000,
            last_seen_at: 1_000,
            last_progress_at: 1_000,
            last_hook_at: None,
            waiting_on: None,
            capture_bytes: 0,
            last_commit: "first".into(),
            cpu_ms: 0,
            cpu_samples: BTreeMap::new(),
            ticket_revision: 0,
            verify_log_bytes: 0,
            last_signal: "test".into(),
            ticket_returned: false,
            revision: 0,
        }
    }
    pub(crate) fn proof() -> Proofs {
        Proofs {
            worktree_exists: true,
            branch_matches: true,
            capture_quiet: true,
            commit: "first".into(),
            ..Default::default()
        }
    }
    /// A manifest names the instance that launched it, not just the hostname
    /// (XNAUT-370). The hostname stays because a human recognises it; the
    /// instance key is what the realm joins on, and the version is what makes
    /// the Studio/tron build drift readable off the record itself.
    #[test]
    fn a_manifest_is_signed_by_the_instance_that_launched_it() {
        let _env = crate::instance::env_lock();
        std::env::set_var("XNAUT_INSTANCE_ID", "inst-launcher");
        std::env::set_var("XNAUT_INSTANCE_ROLE", "fleet");
        let run = RunManifest::requested(
            "@claude", "claude", "/tmp", Some("XNAUT-370".into()), None, &[], 1_000,
        );
        std::env::remove_var("XNAUT_INSTANCE_ID");
        std::env::remove_var("XNAUT_INSTANCE_ROLE");

        assert_eq!(run.instance, "inst-launcher");
        assert_eq!(run.role, "fleet");
        assert_eq!(run.app_version, env!("CARGO_PKG_VERSION"));
    }

    /// 241 manifests on tron were written before these three fields existed.
    /// If adding them made those files unreadable the run registry would lose
    /// every run it is supposed to reconcile, which is a far worse failure
    /// than the attribution gap being open.
    #[test]
    fn a_manifest_written_before_the_stamp_existed_still_reads() {
        // One manifest, held: `run()` mints a fresh ULID on every call, so
        // comparing against a second call would compare two different runs.
        let original = run();
        let mut trimmed = serde_json::to_value(&original).unwrap().as_object().unwrap().clone();
        trimmed.remove("instance");
        trimmed.remove("role");
        trimmed.remove("app_version");

        let parsed: RunManifest = serde_json::from_value(trimmed.into()).unwrap();
        assert_eq!(parsed.run_id, original.run_id);
        assert!(
            parsed.instance.is_empty() && parsed.app_version.is_empty(),
            "a manifest that never named one reads as unknown, not as this machine"
        );
    }

    // ─── The beacon (XNAUT-307) ────────────────────────────────────────────

    fn pong(id: &str, bytes: u64, head: &str) -> Pong {
        Pong {
            run_id: id.to_string(),
            agent_pid: Some(4242),
            capture_bytes: bytes,
            head: head.into(),
            dirty: 0,
        }
    }

    /// The two clocks the reaper reads, and the difference between them.
    /// Every pong says the machine is there; only a pong with movement in it
    /// says work is happening. Collapsing the two would make a stalled run
    /// immortal, which is the same bug as reaping a live one, inverted.
    #[test]
    fn a_pong_always_advances_last_seen_and_only_movement_advances_progress() {
        let mut run = run();
        let id = run.run_id.clone();
        run.last_seen_at = 0;
        run.last_progress_at = 0;
        run.capture_bytes = 100;

        // Nothing moved: the box is there, the work is not.
        let quiet = Pong {
            capture_bytes: 100,
            head: String::new(),
            ..pong(&id, 100, "")
        };
        assert!(!apply_pong(&mut run, &quiet, 60_000));
        assert_eq!(run.last_seen_at, 60_000, "silence is not the same as death");
        assert_eq!(run.last_progress_at, 0, "nothing moved, so nothing progressed");

        // The capture grew.
        assert!(apply_pong(&mut run, &pong(&id, 900, ""), 120_000));
        assert_eq!(run.last_progress_at, 120_000);
        assert_eq!(run.capture_bytes, 900);

        // A capture that SHRANK is not progress. A truncated or rotated log
        // must not read as an agent producing output.
        let shrunk = Pong {
            capture_bytes: 10,
            ..pong(&id, 10, "")
        };
        assert!(!apply_pong(&mut run, &shrunk, 180_000));
        assert_eq!(run.capture_bytes, 900, "the high-water mark stands");
        assert_eq!(run.last_progress_at, 120_000);
    }

    /// The registry stores the FULL sha and the beacon reports the SHORT one.
    /// Comparing them with `!=` would report movement on every pong, and a
    /// genuinely stalled run would look productive forever.
    #[test]
    fn a_short_head_does_not_fake_movement_against_the_full_sha() {
        let full = "0123456789abcdef0123456789abcdef01234567";
        assert!(!head_moved(full, "0123456"), "the same commit, abbreviated");
        assert!(head_moved(full, "fedcba9"), "a genuinely different commit");
        assert!(!head_moved(full, ""), "no git answer is not evidence of a move");
        assert!(!head_moved(full, "  0123456  "), "whitespace is not a commit");

        let mut run = run();
        let id = run.run_id.clone();
        run.last_commit = full.into();
        run.last_progress_at = 0;
        assert!(!apply_pong(&mut run, &pong(&id, 0, "0123456"), 90_000));
        assert_eq!(run.last_progress_at, 0);
        assert_eq!(run.last_commit, full, "an abbreviation must not overwrite it");

        assert!(apply_pong(&mut run, &pong(&id, 0, "fedcba9"), 90_000));
        assert_eq!(run.last_progress_at, 90_000);
        assert_eq!(run.last_commit, "fedcba9");
    }

    /// The beacon proves the VM is up. It must never be able to prove the
    /// AGENT is working — `last_hook_at` is the agent's own word about itself
    /// and `verdict` treats it as proof of life within the grace window. A
    /// beacon pinging beside a dead agent would otherwise be immortal.
    #[test]
    fn a_beacon_never_speaks_for_the_agent() {
        let mut run = run();
        let id = run.run_id.clone();
        run.last_hook_at = None;
        apply_pong(&mut run, &pong(&id, 1, "abc"), 5_000);
        assert!(
            run.last_hook_at.is_none(),
            "the beacon must not forge the agent's hook"
        );
    }

    /// The agent exiting is REPORTED, not inferred from silence.
    #[test]
    fn the_reported_agent_pid_is_recorded_including_its_absence() {
        let mut run = run();
        let id = run.run_id.clone();
        apply_pong(&mut run, &pong(&id, 1, ""), 1_000);
        assert_eq!(run.pid, Some(4242));
        let gone = Pong {
            agent_pid: None,
            ..pong(&id, 1, "")
        };
        apply_pong(&mut run, &gone, 2_000);
        assert_eq!(run.pid, None);
        assert_eq!(run.last_seen_at, 2_000, "the VM is still answering");
        assert!(run.last_signal.contains("gone"), "{}", run.last_signal);
    }

    /// A sandboxed run observed with LOCAL proofs is a failed run: no local
    /// pid, no local zellij session, no local capture. That is what would have
    /// happened to every remote launch the moment one was registered, so the
    /// routing is asserted rather than assumed.
    #[test]
    fn a_remote_run_is_observed_by_its_beacon_and_not_by_this_machine() {
        let dir = directory("remote-observe");
        let mut run = run();
        run.remote_env = Some("gitvm".into());
        run.pid = Some(4242);
        let at = now_ms();
        run.started_at = at;
        run.last_seen_at = at;
        run.last_progress_at = at;
        request_in(&dir, run.clone(), || Ok(())).unwrap();

        let proof = observe_in(&dir, &run, &[]);
        assert!(
            proof.pid_alive,
            "a recent beacon reporting an agent pid is the liveness proof"
        );
        assert!(proof.session_alive, "the VM answered");
        assert!(
            proof.worktree_exists && proof.branch_matches,
            "the local worktree says nothing about /workspace"
        );
        assert_eq!(verdict(&run, &proof, at + GRACE_MS + 1), Verdict::Running);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Silence past the lapse window, and the same run reads as gone.
    #[test]
    fn a_remote_run_whose_beacon_stopped_reads_as_dead() {
        let mut run = run();
        run.remote_env = Some("gitvm".into());
        run.last_seen_at = 0;
        let at = BEACON_LAPSE_MS + 60_000;
        let proof = remote_proofs(&run, at);
        assert!(!proof.pid_alive && !proof.session_alive);
        assert!(proof.capture_quiet);
        assert!(proof.writers_gone(), "silence must release the ticket");
        assert!(matches!(verdict(&run, &proof, at), Verdict::Failed(_)));
    }

    /// The stored run really is updated, under the lock, by run id.
    #[test]
    fn a_pong_is_written_through_to_the_stored_run() {
        let dir = directory("beacon-store");
        let mut run = run();
        run.last_progress_at = 0;
        run.capture_bytes = 0;
        let run = request_in(&dir, run, || Ok(())).unwrap();

        let (stored, progressed) =
            beacon_in(&dir, &pong(&run.run_id, 512, "abc1234"), 700_000).unwrap();
        assert!(progressed);
        assert_eq!(stored.capture_bytes, 512);
        assert_eq!(stored.last_seen_at, 700_000);
        assert_eq!(stored.last_progress_at, 700_000);
        assert_eq!(
            load_manifest_in(&dir, &run.run_id).unwrap().capture_bytes,
            512,
            "it has to survive the reload, not just the return value"
        );

        // A pong for a run that does not exist is an error, not a new row.
        let bogus = Pong {
            run_id: new_id(9_000),
            ..pong(&run.run_id, 1, "")
        };
        assert!(beacon_in(&dir, &bogus, 800_000).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    pub(crate) fn directory(label: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".xnaut/test-state")
            .join(format!("{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    pub(crate) fn handback(run: &RunManifest) -> crate::handback::Handback {
        crate::handback::Handback {
            run_id: Some(run.run_id.clone()),
            ticket: run.ticket.clone().unwrap(),
            from: run.agent_handle.clone(),
            summary: "Completed isolated run lifecycle fixture".into(),
            files_changed: vec!["src-tauri/src/run_control.rs".into()],
            commits: vec!["a123456789abcdef".into()],
            how_verified: "manual: exercised the isolated completion fixture".into(),
            not_finished: Some("nothing".into()),
            confidence: crate::handback::Confidence::High,
            submitted_at: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        }
    }

    #[test]
    fn completion_identity_and_dead_process_arms_are_pure() {
        let mut run = run();
        let mut h = handback(&run);
        assert!(handback_matches(&run, &h));
        h.run_id = Some("another-run".into());
        assert!(!handback_matches(&run, &h));
        h = handback(&run);
        h.ticket = "XNAUT-901".into();
        assert!(!handback_matches(&run, &h));
        h = handback(&run);
        h.from = "another-agent".into();
        assert!(!handback_matches(&run, &h));
        let mut dead = proof();
        assert!(matches!(
            verdict(&run, &dead, 1_000_000),
            Verdict::Failed(_)
        ));
        assert!(mark_completed(&mut run, "accepted handback"));
        assert_eq!(verdict(&run, &dead, 1_000_000), Verdict::Keep);
        dead.exit_code = Some(137);
        assert_eq!(verdict(&run, &dead, 1_000_000), Verdict::Keep);
        for state in [
            RunState::Failed,
            RunState::Retired,
            RunState::Retiring,
            RunState::Undead,
        ] {
            run.state = state;
            assert!(!mark_completed(&mut run, "late signal"));
            assert_eq!(run.state, state);
        }
    }

    #[test]
    fn a_ticket_with_a_live_run_reports_it_and_a_finished_one_does_not() {
        // XNAUT-266 got two claude runs in one worktree, 2026-09-08.
        let dir = directory("live-run-per-ticket");
        assert!(live_run_for_ticket_in(&dir, "XNAUT-900").unwrap().is_none());
        let live = request_in(&dir, run(), || Ok(())).unwrap();
        let found = live_run_for_ticket_in(&dir, "XNAUT-900").unwrap().expect("a running run is live");
        assert_eq!(found.run_id, live.run_id);
        assert!(live_run_for_ticket_in(&dir, "XNAUT-901").unwrap().is_none(), "other tickets unaffected");
        update_in(&dir, &live.run_id, |r| r.state = RunState::Done).unwrap();
        assert!(live_run_for_ticket_in(&dir, "XNAUT-900").unwrap().is_none(), "done is not live");
    }

    #[test]
    fn handback_is_session_bound_and_only_successful_storage_marks_done() {
        let dir = directory("handback-binding");
        let run = request_in(&dir, run(), || Ok(())).unwrap();
        let mut h = handback(&run);
        assert!(bind_handback_in(&dir, Some("wrong-session"), &mut h).is_err());
        assert!(bind_handback_in(&dir, None, &mut h).is_err());
        h.run_id = None;
        bind_handback_in(&dir, Some("test-session"), &mut h).unwrap();
        assert_eq!(h.run_id.as_deref(), Some(run.run_id.as_str()));
        let result: Result<(), String> =
            record_handback_in(&dir, &h, || Err("PM write failed".into()));
        assert!(result.is_err());
        assert_eq!(
            load_manifest_in(&dir, &run.run_id).unwrap().state,
            RunState::Starting
        );
        let mut wrong = h.clone();
        wrong.ticket = "XNAUT-901".into();
        assert!(record_handback_in(&dir, &wrong, || -> Result<(), String> {
            panic!("identity mismatch must not store anything")
        })
        .is_err());
        record_handback_in(&dir, &h, || Ok(())).unwrap();
        assert_eq!(
            load_manifest_in(&dir, &run.run_id).unwrap().state,
            RunState::Done
        );
        signal_session_in(
            &dir,
            "test-session",
            Some(RunState::Running),
            None,
            1_000_000,
        )
        .unwrap();
        assert_eq!(
            load_manifest_in(&dir, &run.run_id).unwrap().state,
            RunState::Done
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ticket_completion_requires_transition_and_live_latest_owner() {
        for (before, after, alive, owner, expected) in [
            ("in_progress", "done", true, "codex", RunState::Done),
            ("in_progress", "review", true, "@codex", RunState::Done),
            ("in_progress", "done", false, "codex", RunState::Starting),
            ("ready", "done", true, "codex", RunState::Starting),
            ("in_progress", "ready", true, "codex", RunState::Starting),
            ("in_progress", "done", true, "claude", RunState::Starting),
        ] {
            let dir = directory("ticket-completion");
            let run = request_in(&dir, run(), || Ok(())).unwrap();
            ticket_completed_in(&dir, "XNAUT-900", owner, before, after, |_| Proofs {
                pid_alive: alive,
                ..proof()
            })
            .unwrap();
            assert_eq!(
                load_manifest_in(&dir, &run.run_id).unwrap().state,
                expected,
                "{before}/{after}/{alive}/{owner}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
        let dir = directory("latest-ticket-completion");
        let old = request_in(&dir, run(), || Ok(())).unwrap();
        let mut newer = run();
        newer.started_at += 1;
        newer.agent_handle = "claude".into();
        let newer = request_in(&dir, newer, || Ok(())).unwrap();
        ticket_completed_in(&dir, "XNAUT-900", "codex", "in_progress", "done", |_| {
            Proofs {
                pid_alive: true,
                ..proof()
            }
        })
        .unwrap();
        for r in [old, newer] {
            assert_eq!(
                load_manifest_in(&dir, &r.run_id).unwrap().state,
                RunState::Starting
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pure_proofs_distinguish_dead_waiting_and_stalled() {
        let mut run = run();
        let mut proof = proof();
        let at = run.started_at + PROGRESS_WINDOW_MS + 1;
        let Verdict::Failed(reason) = verdict(&run, &proof, at) else {
            panic!("dead run must fail")
        };
        for expected in [
            "pid does not answer",
            "zellij session absent",
            "capture has not grown",
        ] {
            assert!(reason.contains(expected), "{reason}");
        }
        proof.pid_alive = true;
        run.waiting_on = Some("in-owner-approval".into());
        assert_eq!(verdict(&run, &proof, at), Verdict::Waiting);
        assert_eq!(verdict(&run, &proof, at + 86_400_000), Verdict::Waiting);
        run.waiting_on = Some("  ".into());
        assert!(
            matches!(verdict(&run,&proof,at),Verdict::Failed(r) if r.contains("stalled") && r.contains("waiting_on empty"))
        );
        proof.capture_bytes = 1;
        assert_eq!(verdict(&run, &proof, at), Verdict::Running);
        proof.capture_bytes = 0;
        proof.commit = "new-commit".into();
        assert_eq!(verdict(&run, &proof, at), Verdict::Running);
        proof.commit = "first".into();
        run.last_progress_at = at;
        assert_eq!(verdict(&run, &proof, at), Verdict::Running);
    }
    // ─── A compile is work (XNAUT-439) ─────────────────────────────────────

    /// The bug, as a test. Run 01M354JH4E9TVBGJ36H7SNJC5G on tron was alive in
    /// its zellij session, on its own branch, fifteen minutes into
    /// `cargo test` on a cold worktree — and so wrote no capture, fired no
    /// hook and made no commit. It was marked failed while it worked.
    ///
    /// Each of the three new readings alone must be enough to rescue it,
    /// because a given minute of that compile may move only one of them.
    #[test]
    fn a_compile_is_progress_though_capture_hooks_and_commits_stand_still() {
        let run = run();
        let base = proof();
        let at = run.started_at + PROGRESS_WINDOW_MS + 1;

        // The exact shape of the 2026-09-22 run: alive, on its branch, with
        // capture and HEAD unmoved since the record was written.
        let alive = || Proofs { pid_alive: true, ..base.clone() };
        assert_eq!(alive().capture_bytes, run.capture_bytes);
        assert_eq!(alive().commit, run.last_commit);

        // Before the fix this was the whole story, and it still is when
        // genuinely nothing moves.
        assert!(
            matches!(verdict(&run, &alive(), at), Verdict::Failed(r) if r.contains("stalled")),
            "six flat readings past the window is still a stall"
        );

        for (what, proof) in [
            ("rustc burning CPU", Proofs { cpu_ms: 1, ..alive() }),
            ("a ticket-body edit", Proofs { ticket_revision: 1, ..alive() }),
            ("a growing verify log", Proofs { verify_log_bytes: 1, ..alive() }),
        ] {
            assert_eq!(
                verdict(&run, &proof, at),
                Verdict::Running,
                "{what} is progress, however quiet the capture is"
            );
        }
    }

    fn cpu_row(pid: u32, ppid: u32, birth: &str, cpu_ms: u64) -> ProcRow {
        ProcRow { pid, ppid, birth: birth.into(), cpu_ms }
    }

    #[test]
    fn child_cpu_churn_survives_restart_without_a_historical_peak_stalling_work() {
        let dir = std::env::temp_dir().join(format!("xnaut-439-cpu-{}", uuid::Uuid::new_v4()));
        let mut run = run();
        run.started_at = 1_000;
        run.last_progress_at = 1_000;
        run.last_commit.clear();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{}.run.json", run.run_id)), serde_json::to_vec(&run).unwrap()).unwrap();
        let sample = |at, rows: &[ProcRow]| {
            reconcile_in(&dir, at, |current| {
                let (cpu_ms, cpu_samples) = child_cpu_progress(current, rows, Some(100), Some("root"), true);
                Proofs { pid_alive: true, cpu_ms, cpu_samples, worktree_exists: true,
                    branch_matches: true, commit: current.last_commit.clone(), ..Default::default() }
            }).unwrap();
            load_manifest_in(&dir, &run.run_id).unwrap()
        };
        let root = cpu_row(100, 1, "root", 900_000);
        let first = sample(2_000, &[root.clone(), cpu_row(200, 100, "first", 600_000)]);
        assert_eq!(first.cpu_ms, 600_000);
        assert_eq!(first.last_progress_at, 2_000);
        // Every call reloads persisted samples. Smaller replacement children
        // must move progress even after the large compile has left the table.
        let mut at = 2_000;
        for n in 1..=4 {
            at += PROGRESS_WINDOW_MS / 2;
            let current = sample(at, &[root.clone(), cpu_row(200, 100, &format!("child-{n}"), 12)]);
            assert_eq!(current.cpu_ms, 600_000 + n * 12);
            assert_eq!(current.last_progress_at, at);
            assert_eq!(current.state, RunState::Running);
        }
        let gone = sample(at + 1, std::slice::from_ref(&root));
        assert_eq!(gone.last_progress_at, at, "child exit alone is not progress");
        assert!(gone.cpu_samples.is_empty());
        let idle = sample(at + PROGRESS_WINDOW_MS + 1, &[root]);
        assert_eq!(idle.state, RunState::Failed);
        assert_eq!(idle.last_signal, STALLED_NO_PROGRESS);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn child_cpu_requires_live_root_birth_and_preserves_unknown_sample_baseline() {
        let mut run = run();
        run.cpu_ms = 600_000;
        run.cpu_samples.insert(200, ProcessCpuSample { birth: "child".into(), cpu_ms: 12 });
        let rows = [cpu_row(100, 1, "root", 9_000), cpu_row(200, 100, "child", 15)];
        for (pid, birth, alive) in [
            (Some(100), Some("root"), false),
            (Some(100), Some("reused-root"), true),
            (Some(100), None, true), (None, Some("root"), true),
            (Some(0), Some("root"), true),
        ] {
            assert_eq!(child_cpu_progress(&run, &rows, pid, birth, alive), (600_000, None));
        }
        assert_eq!(child_cpu_progress(&run, &[], Some(100), Some("root"), true), (600_000, None));
        let (total, samples) = child_cpu_progress(&run, &rows, Some(100), Some("root"), true);
        assert_eq!(total, 600_003, "unknown observations cannot reset or double-count the previous sample");
        run.cpu_ms = total;
        run.cpu_samples = samples.unwrap();
        assert_eq!(child_cpu_progress(&run, &rows, Some(100), Some("root"), true).0, total);
        let replaced = [rows[0].clone(), cpu_row(200, 100, "new-child", 2)];
        assert_eq!(child_cpu_progress(&run, &replaced, Some(100), Some("root"), true).0, total + 2);
    }

    #[test]
    fn observe_rejects_cpu_from_a_reused_pid_even_when_its_viewport_is_live() {
        let registry = std::env::temp_dir().join(format!("xnaut-439-birth-{}", uuid::Uuid::new_v4()));
        let mut run = run();
        run.pid = Some(std::process::id());
        run.process_birth = Some("not this process birth".into());
        run.zellij_session = Some("still-open".into());
        run.cpu_ms = 100;
        let machine = Machine { control_repo: None, rows: vec![
            cpu_row(std::process::id(), 1, "not this process birth", 0),
            cpu_row(u32::MAX, std::process::id(), "foreign-child", 500_000),
        ] };
        let proof = observe_with(&machine, &registry, &run, &["still-open".into()]);
        assert!(proof.session_alive);
        assert!(!proof.pid_alive);
        assert_eq!(proof.cpu_ms, run.cpu_ms);
        assert!(proof.cpu_samples.is_none());
    }

    /// The reconciler and the verdict must agree about what progress means.
    /// They each used to spell the condition out, and a reconciler that keeps
    /// a record the verdict has already failed is how a run reads failed on
    /// the board while its ticket is held open.
    #[test]
    fn the_reconciler_and_the_verdict_read_progress_from_one_definition() {
        let run = run();
        for proof in [
            Proofs { capture_bytes: 1, ..proof() },
            Proofs { commit: "moved".into(), ..proof() },
            Proofs { cpu_ms: 1, ..proof() },
            Proofs { ticket_revision: 1, ..proof() },
            Proofs { verify_log_bytes: 1, ..proof() },
        ] {
            assert!(proof.progressed(&run), "{proof:?}");
        }
        assert!(
            !proof().progressed(&run),
            "the baseline proof matches the record, so nothing moved"
        );
        // Equal is not greater: a reading that merely repeats itself is silence.
        let mut flat = proof();
        flat.cpu_ms = 0;
        flat.ticket_revision = 0;
        flat.verify_log_bytes = 0;
        assert!(!flat.progressed(&run));
    }

    /// `ps` prints accumulated CPU in four shapes, and the detector reads all
    /// of them or silently scores a busy subtree as zero.
    #[test]
    fn accumulated_cpu_parses_in_every_shape_ps_prints() {
        assert_eq!(parse_cpu_time("0:00.00"), Some(0));
        assert_eq!(parse_cpu_time("0:01.50"), Some(1_500));
        assert_eq!(parse_cpu_time("503:51.52"), Some(503 * 60_000 + 51_520));
        assert_eq!(parse_cpu_time("2:03:04.00"), Some(7_384_000));
        assert_eq!(parse_cpu_time("1-02:03:04.00"), Some(93_784_000));
        assert_eq!(parse_cpu_time("  0:02.00  "), Some(2_000));
        for junk in ["", "-", "abc", "1:2:3:4.0", "0:-1.0", "x:00.00"] {
            assert_eq!(parse_cpu_time(junk), None, "{junk:?}");
        }
    }

    /// The walk sums DESCENDANTS and never the run's own process.
    ///
    /// Including the root would make every run look busy forever — a polling
    /// CLI always accrues a little — and the detector would stop working
    /// altogether. That is a worse bug than the one being fixed, so it gets a
    /// test of its own rather than a comment.
    #[test]
    fn the_cpu_walk_sums_children_and_grandchildren_but_never_the_run_itself() {
        let rows = [
            ProcRow { birth: "birth".into(), pid: 100, ppid: 1, cpu_ms: 9_000 },   // the agent CLI
            ProcRow { birth: "birth".into(), pid: 200, ppid: 100, cpu_ms: 50 },    // its shell
            ProcRow { birth: "birth".into(), pid: 300, ppid: 200, cpu_ms: 400_000 }, // cargo
            ProcRow { birth: "birth".into(), pid: 400, ppid: 300, cpu_ms: 600_000 }, // rustc
            ProcRow { birth: "birth".into(), pid: 500, ppid: 1, cpu_ms: 777 },     // someone else's
        ];
        assert_eq!(descendant_cpu_ms(&rows, 100), 50 + 400_000 + 600_000);
        assert_eq!(descendant_cpu_ms(&rows, 300), 600_000);
        assert_eq!(descendant_cpu_ms(&rows, 400), 0, "a leaf has no children");
        assert_eq!(descendant_cpu_ms(&rows, 999), 0, "an absent pid is not an error");
        // pid 0 would otherwise match every orphan's ppid and sum the box.
        assert_eq!(descendant_cpu_ms(&rows, 0), 0);
        // A reparenting cycle must not hang the sweep.
        let cyclic = [
            ProcRow { birth: "birth".into(), pid: 10, ppid: 20, cpu_ms: 1 },
            ProcRow { birth: "birth".into(), pid: 20, ppid: 10, cpu_ms: 2 },
        ];
        assert_eq!(descendant_cpu_ms(&cyclic, 10), 2);
    }

    #[test]
    fn a_process_table_row_parses_and_junk_is_dropped() {
        assert_eq!(
            parse_proc_row(" 80149 32641   1:02.50 birth "),
            Some(ProcRow { birth: "birth".into(), pid: 80_149, ppid: 32_641, cpu_ms: 62_500 })
        );
        for junk in ["", "80149", "80149 32641", "a b 0:00.00", "1 2 nope"] {
            assert_eq!(parse_proc_row(junk), None, "{junk:?}");
        }
    }

    /// The worktree's `.xnaut/` tree is gitignored, which is exactly why it is
    /// worth reading: it grows through the same minutes no commit does.
    #[test]
    fn the_verify_log_reading_is_the_worktree_xnaut_tree() {
        let tree = std::env::temp_dir()
            .join(format!("xnaut-439-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tree);
        assert_eq!(
            verify_log_bytes_in(&tree),
            0,
            "a worktree with no .xnaut tree reads zero, not an error"
        );
        std::fs::create_dir_all(tree.join(".xnaut/bundles")).unwrap();
        assert_eq!(verify_log_bytes_in(&tree), 0, "empty directories weigh nothing");
        std::fs::write(tree.join(".xnaut/verify.json"), "{}").unwrap();
        std::fs::write(tree.join(".xnaut/bundles/XNAUT-439.md"), "totals").unwrap();
        assert_eq!(verify_log_bytes_in(&tree), 2 + 6, "nested files are counted");
        // An empty path must not become `./.xnaut` and read some unrelated tree.
        assert_eq!(verify_log_bytes_in(Path::new("")), 0);
        std::fs::remove_dir_all(&tree).unwrap();
    }

    /// The agent on 2026-09-22 wrote its test results into the ticket body.
    /// That write is the one thing it did that this machine could have seen.
    #[test]
    fn the_ticket_revision_is_read_from_the_run_s_own_ticket() {
        let repo = std::env::temp_dir()
            .join(format!("xnaut-439-control-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        let tickets = repo.join("projects/XNAUT/tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        std::fs::write(tickets.join("XNAUT-439.json"), r#"{"revision":7}"#).unwrap();
        assert_eq!(ticket_revision_in(&repo, "XNAUT", "XNAUT-439"), 7);

        // A record with no revision, or none at all, is no signal — never a panic.
        std::fs::write(tickets.join("XNAUT-440.json"), r#"{"id":"XNAUT-440"}"#).unwrap();
        assert_eq!(ticket_revision_in(&repo, "XNAUT", "XNAUT-440"), 0);
        assert_eq!(ticket_revision_in(&repo, "XNAUT", "XNAUT-999"), 0);
        std::fs::write(tickets.join("XNAUT-441.json"), "not json").unwrap();
        assert_eq!(ticket_revision_in(&repo, "XNAUT", "XNAUT-441"), 0);

        // Both halves become path components, so a crafted id is refused
        // rather than allowed to read a file outside the tickets directory.
        std::fs::write(repo.join("secret.json"), r#"{"revision":99}"#).unwrap();
        for (project, ticket) in [
            ("XNAUT", "../../../secret"),
            ("..", "XNAUT-439"),
            ("XNAUT", ""),
            ("", "XNAUT-439"),
            ("XNAUT", "a/b"),
        ] {
            assert_eq!(
                ticket_revision_in(&repo, project, ticket),
                0,
                "{project:?}/{ticket:?} must not resolve"
            );
        }
        std::fs::remove_dir_all(&repo).unwrap();
    }

    /// A sandboxed run's children live on the far side. Reporting this
    /// machine's zero for them would read as a fall from the stored mark, so
    /// the beacon's readings are carried through untouched.
    #[test]
    fn a_remote_run_carries_its_own_readings_rather_than_this_machine_s() {
        let mut run = run();
        run.remote_env = Some("gitvm".into());
        run.cpu_ms = 600_000;
        run.ticket_revision = 7;
        run.verify_log_bytes = 4_096;
        run.last_seen_at = 10_000;
        let proof = remote_proofs(&run, 10_000);
        assert_eq!(proof.cpu_ms, 600_000);
        assert_eq!(proof.ticket_revision, 7);
        assert_eq!(proof.verify_log_bytes, 4_096);
        assert!(
            !proof.progressed(&run),
            "carried-through readings are not progress by themselves"
        );
    }

    /// End to end against the real process table and a real directory.
    ///
    /// Everything above this test is pure, and a pure test would pass just as
    /// happily if `ps` took different flags on this platform, printed a column
    /// in another order, or returned nothing at all — the detector would score
    /// every busy subtree at zero and go on failing working agents, with no
    /// test red and no error anywhere. The same silent-no-op class as calling a
    /// `window.*` global that was never assigned.
    #[cfg(unix)]
    #[test]
    fn observe_reads_real_child_cpu_a_real_xnaut_tree_and_a_real_ticket() {
        let root = std::env::temp_dir()
            .join(format!("xnaut-439-observe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (registry, tree, repo) =
            (root.join("registry"), root.join("tree"), root.join("control"));
        std::fs::create_dir_all(registry.join("x")).unwrap();
        std::fs::create_dir_all(tree.join(".xnaut/bundles")).unwrap();
        std::fs::create_dir_all(repo.join("projects/XNAUT/tickets")).unwrap();
        std::fs::write(tree.join(".xnaut/bundles/XNAUT-439.md"), "0123456789").unwrap();
        std::fs::write(
            repo.join("projects/XNAUT/tickets/XNAUT-439.json"),
            r#"{"revision":11}"#,
        )
        .unwrap();

        let mut run = run();
        run.pid = Some(std::process::id());
        run.process_birth = process_birth(std::process::id());
        run.worktree_path = tree.to_string_lossy().into();
        run.branch = String::new(); // no git repo here; branch is not under test
        run.ticket = Some("XNAUT-439".into());
        run.project = "XNAUT".into();

        // A real child burning real CPU, reaped on every path out of here.
        let mut burner = std::process::Command::new("sh")
            .args(["-c", "while :; do :; done"])
            .spawn()
            .expect("spawn a cpu burner");
        let mut cpu = 0;
        for _ in 0..100 {
            cpu = observe_in(&registry, &run, &[]).cpu_ms;
            if cpu > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = burner.kill();
        let _ = burner.wait();
        assert!(
            cpu > 0,
            "a child burning cpu must read as cpu: `ps -axo pid=,ppid=,time=` \
             returned nothing this detector could use"
        );

        let proof = observe_in(&registry, &run, &[]);
        assert_eq!(proof.verify_log_bytes, 10, "the real .xnaut tree is measured");
        assert_eq!(
            ticket_revision_in(&repo, "XNAUT", "XNAUT-439"),
            11,
            "the real ticket record is read"
        );

        // A terminal run never gets a fresh verdict, so the three readings are
        // not taken for it. 109 of the 211 manifests on this machine's registry
        // are already failed, and walking a worktree for each of them on every
        // sweep is the cost this skip exists to avoid.
        for state in [RunState::Failed, RunState::Done, RunState::Retired] {
            let mut terminal = run.clone();
            terminal.state = state;
            let proof = observe_in(&registry, &terminal, &[]);
            assert_eq!(
                (proof.cpu_ms, proof.ticket_revision, proof.verify_log_bytes),
                (0, 0, 0),
                "{state:?} is not judged again, so it is not measured"
            );
            assert_eq!(
                proof.capture_bytes, run.capture_bytes,
                "the capture baseline is still read: the return path needs it"
            );
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Every manifest on tron predates these three fields. A registry that
    /// cannot read its own records loses every run it is meant to reconcile,
    /// which is worse than the stall window being wrong.
    #[test]
    fn a_manifest_written_before_the_progress_readings_existed_still_reads() {
        let original = run();
        let mut trimmed =
            serde_json::to_value(&original).unwrap().as_object().unwrap().clone();
        for field in ["cpu_ms", "cpu_samples", "ticket_revision", "verify_log_bytes"] {
            assert!(trimmed.remove(field).is_some(), "{field} must be written");
        }
        let parsed: RunManifest = serde_json::from_value(trimmed.into()).unwrap();
        assert_eq!(parsed.run_id, original.run_id);
        assert_eq!(
            (parsed.cpu_ms, parsed.ticket_revision, parsed.verify_log_bytes),
            (0, 0, 0),
            "an unstated reading is no reading, so the first sweep sets it"
        );
    }

    #[test]
    fn grace_and_terminal_states_do_not_turn_into_ghost_failures() {
        let mut run = run();
        let proof = proof();
        assert_eq!(
            verdict(&run, &proof, run.started_at + GRACE_MS - 1),
            Verdict::Keep
        );
        for state in [RunState::Done, RunState::Failed, RunState::Retired] {
            run.state = state;
            assert_eq!(
                verdict(&run, &proof, run.started_at + PROGRESS_WINDOW_MS + 1),
                Verdict::Keep
            );
        }
    }
    #[test]
    fn waiting_does_not_hide_a_dead_process_or_wrong_worktree() {
        let mut run = run();
        run.waiting_on = Some("approval".into());
        let mut proof = proof();
        assert!(matches!(
            verdict(&run, &proof, 1_000_000),
            Verdict::Failed(_)
        ));
        // A LIVE process off its branch is a mutation check, not a wrong
        // worktree (XNAUT-360): waiting stays waiting.
        proof.pid_alive = true;
        proof.branch_matches = false;
        assert_eq!(verdict(&run, &proof, 1_000_000), Verdict::Waiting);
        // Gone AND off its branch is the wrong worktree, named as such.
        proof.pid_alive = false;
        assert!(
            matches!(verdict(&run,&proof,1_000_000),Verdict::Failed(r) if r.contains("branch mismatch"))
        );
    }
    #[test]
    fn a_live_run_that_detached_head_for_a_mutation_check_keeps_running() {
        let run = run();
        let mut proof = proof();
        proof.pid_alive = true;
        proof.capture_bytes = run.capture_bytes + 1;
        proof.branch_matches = false;
        assert_eq!(verdict(&run, &proof, 1_000_000), Verdict::Running);
    }
    #[test]
    fn durable_conversations_leave_two_worker_slots_and_ticketless_reviewers_count() {
        let dir = directory("conversation-capacity");
        for n in 0..2 {
            let mut chat = RunManifest::requested("owner-chat", "fixture",
                &format!("/chat-{n}"), None, None, &[], 10 + n);
            chat.user_conversation = true;
            let saved = request_in(&dir, chat, || Ok(())).unwrap();
            let reopened = load_manifest_in(&dir, &saved.run_id).unwrap();
            assert!(reopened.user_conversation, "classification survives reopening");
            assert!(!consumes_worker_capacity(&reopened));
        }
        assert_eq!(worker_count_in(&dir).unwrap(), 0);
        let launch = |path: &str, ticket: Option<String>| {
            let task = RunManifest::requested("worker", "fixture", path, ticket, None, &[], 20);
            let own = task.run_id.clone();
            request_in(&dir, task, || worker_capacity_in(&dir, 2, &own, None))
        };
        let first = launch("/task-one", Some("TEST-1".into())).unwrap();
        launch("/task-two", Some("TEST-2".into())).unwrap();
        assert_eq!(worker_count_in(&dir).unwrap(), 2);
        assert!(launch("/task-three", Some("TEST-3".into())).unwrap_err().starts_with("worker capacity:"));
        update_in(&dir, &first.run_id, |run| run.state = RunState::Done).unwrap();
        let reviewer = launch("/independent-review", None).unwrap();
        assert!(!reviewer.user_conversation);
        assert_eq!(worker_count_in(&dir).unwrap(), 2);
        assert!(launch("/another-task", Some("TEST-4".into())).unwrap_err().starts_with("worker capacity:"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_unknown_and_explicit_review_records_remain_capacity_consumers() {
        let mut record = serde_json::to_value(run()).unwrap();
        record.as_object_mut().unwrap().remove("user_conversation");
        record["ticket"] = serde_json::Value::Null;
        let mut legacy: RunManifest = serde_json::from_value(record).unwrap();
        assert!(!legacy.user_conversation);
        assert!(consumes_worker_capacity(&legacy));
        legacy.user_conversation = true;
        assert!(!consumes_worker_capacity(&legacy));
        legacy.kind = RunKind::Review;
        assert!(consumes_worker_capacity(&legacy));
        legacy.state = RunState::Done;
        assert!(!consumes_worker_capacity(&legacy));
    }

    #[test]
    fn prelaunch_proof_cannot_reclassify_a_registered_worker_or_plain_failure() {
        let dir = directory("prelaunch-proof");
        let mut requested = run();
        requested.pid = None;
        requested.pty_session = None;
        let admitted = request_in(&dir, requested, || Ok(())).unwrap();
        assert!(refuse_prelaunch_in(&dir, admitted.clone(), PrelaunchPhase::RepositoryStaging, "late error").is_err());
        assert_eq!(load_manifest_in(&dir, &admitted.run_id).unwrap(), admitted);
        update_in(&dir, &admitted.run_id, |r| r.state = RunState::Failed).unwrap();
        let failed = load_manifest_in(&dir, &admitted.run_id).unwrap();
        assert!(!prelaunch_refused(&failed));
        assert!(refuse_prelaunch_in(&dir, failed.clone(), PrelaunchPhase::RepositoryStaging, "unknown error").is_err());
        assert_eq!(load_manifest_in(&dir, &failed.run_id).unwrap(), failed);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn requested_ids_are_unique_ulids_and_refusals_are_durable() {
        let dir = directory("admission");
        let run = run();
        let id = run.run_id.clone();
        assert_eq!(id.len(), 26);
        assert_ne!(id, new_id(1_000));
        assert!(new_id(2_000) > id);
        assert!(request_in(&dir, run, || Err("ceiling reached".into())).is_err());
        let refused = load_manifest_in(&dir, &id).unwrap();
        assert_eq!(refused.state, RunState::Failed);
        assert!(refused.last_signal.contains("ceiling reached"));
        assert_eq!(refused.revision, 2);
        assert!(request_in(&dir, refused, || Ok(()))
            .unwrap_err()
            .contains("already exists"));
        assert!(load_manifest_in(&dir, "../escape").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn journal_recovers_a_lost_snapshot_and_repairs_only_a_partial_tail() {
        let dir = directory("journal");
        let run = request_in(&dir, run(), || Ok(())).unwrap();
        std::fs::remove_file(manifest_path(&dir, &run.run_id)).unwrap();
        assert_eq!(
            load_manifest_in(&dir, &run.run_id).unwrap().state,
            RunState::Starting
        );
        let path = journal_path(&dir, &run.run_id);
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{\"sequence\":")
            .unwrap();
        update_in(&dir, &run.run_id, |r| r.state = RunState::Running).unwrap();
        assert_eq!(load_manifest_in(&dir, &run.run_id).unwrap().revision, 3);
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"broken\n{}\n")
            .unwrap();
        assert!(update_in(&dir, &run.run_id, |_| {})
            .unwrap_err()
            .contains("corrupt middle"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn explicit_wait_and_repeated_hooks_are_durable_progress() {
        let dir = directory("signals");
        let run = request_in(&dir, run(), || Ok(())).unwrap();
        signal_session_in(&dir, "test-session", Some(RunState::Running), None, 2_000).unwrap();
        signal_session_in(&dir, "test-session", Some(RunState::Running), None, 3_000).unwrap();
        assert_eq!(
            load_manifest_in(&dir, &run.run_id)
                .unwrap()
                .last_progress_at,
            3_000
        );
        signal_session_in(
            &dir,
            "test-session",
            None,
            Some(Some("in-approval".into())),
            4_000,
        )
        .unwrap();
        signal_session_in(
            &dir,
            "test-session",
            Some(RunState::Running),
            Some(None),
            5_000,
        )
        .unwrap();
        assert_eq!(
            load_manifest_in(&dir, &run.run_id)
                .unwrap()
                .waiting_on
                .as_deref(),
            Some("in-approval")
        );
        clear_wait_in(&dir, "test-session", "in-other", 6_000).unwrap();
        assert!(load_manifest_in(&dir, &run.run_id)
            .unwrap()
            .waiting_on
            .is_some());
        clear_wait_in(&dir, "test-session", "in-approval", 7_000).unwrap();
        assert!(load_manifest_in(&dir, &run.run_id)
            .unwrap()
            .waiting_on
            .is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_newer_worktree_holder_prevents_an_old_failure_return() {
        let dir = directory("newer-holder");
        let old = request_in(&dir, run(), || Ok(())).unwrap();
        update_in(&dir, &old.run_id, |r| r.state = RunState::Failed).unwrap();
        let mut newer = run();
        newer.started_at = old.started_at + 1;
        newer.ticket = Some("XNAUT-901".into());
        request_in(&dir, newer, || Ok(())).unwrap();
        finish_failed_in(&dir, &old.run_id, |_| {
            panic!("must not release a newer worktree holder")
        })
        .unwrap();
        assert!(!load_manifest_in(&dir, &old.run_id).unwrap().ticket_returned);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn reused_pid_birth_is_not_proof_of_the_original_process() {
        let dir = directory("pid");
        let mut run = run();
        run.pid = Some(std::process::id());
        run.process_birth = Some("a different process".into());
        assert!(!observe_in(&dir, &run, &[]).pid_alive);
        run.process_birth = process_birth(std::process::id());
        assert!(observe_in(&dir, &run, &[]).pid_alive);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

pub fn clear_wait_in(dir: &Path, session: &str, wait_id: &str, at: i64) -> Result<(), String> {
    let _lock = StoreLock::acquire(dir)?;
    for id in list_ids_in(dir)? {
        let mut run = load_manifest_in(dir, &id)?;
        if run.pty_session.as_deref() == Some(session)
            && !run.state.terminal()
            && !matches!(run.state, RunState::Retiring | RunState::Undead)
            && run.waiting_on.as_deref() == Some(wait_id)
        {
            run.waiting_on = None;
            run.last_progress_at = at;
            if run.state != RunState::Degraded {
                run.state = RunState::Running;
                run.last_signal = format!("wait answered: {wait_id}");
            }
            persist_locked(dir, &mut run)?;
        }
    }
    Ok(())
}

/// A durable stop attempt. Capture quietness is measured from observed byte
/// counts after retirement begins, never inferred from an old file mtime.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Retirement {
    pub started_at: i64,
    pub quiet_since: i64,
    pub capture_bytes: u64,
    pub requirement: String,
    pub stopped_at: Option<i64>,
    #[serde(default)]
    pub dead_since: Option<i64>,
}

pub fn model_meets(model: &str, requirement: &str) -> bool {
    requirement.trim().is_empty() || model.trim().eq_ignore_ascii_case(requirement.trim())
}

pub fn swap_required(run: &RunManifest, requirement: &str) -> bool {
    !requirement.trim().is_empty()
        && matches!(run.state, RunState::Degraded | RunState::Blocked)
        && run
            .waiting_on
            .as_deref()
            .is_none_or(|w| w.trim().is_empty())
}

/// Bind a not-yet-launched remote worker to the native pending continuation.
/// This happens before any repository receipt/script contains the run ID.
pub(crate) fn bind_pending_in(dir: &Path, run: &mut RunManifest) -> Result<(), String> {
    let Some(ticket) = run.ticket.as_deref() else { return Ok(()); };
    let Some(pending) = continuation_in(dir,ticket)? else { return Ok(()); };
    if pending.worktree_path != run.worktree_path { return Err("Continuation must preserve its original worktree".into()); }
    if pending.state == RunState::Failed && pending.admission_refused {
        run.previous_run_id = Some(pending.run_id);
    } else if pending.state == RunState::Requested {
        run.run_id = pending.run_id;
        run.previous_run_id = pending.previous_run_id;
    } else { return Err("Continuation is not eligible for admission".into()); }
    run.branch = pending.branch;
    Ok(())
}

/// Reserve one author repair after externally proving that the previous
/// repository worker AND its publisher finished. No signals or guessing from
/// terminal registry state. This shares admission's store lock and identity.
#[cfg(test)]
pub(crate) fn reserve_repair_in(dir: &Path, id: &str, proof: &Proofs, at: i64) -> Result<RunManifest,String> {
    reserve_repair_admitted_in(dir, id, proof, at, |_| Ok(()))
}
pub(crate) fn reserve_repair_admitted_in(dir: &Path, id: &str, proof: &Proofs, at: i64, admit: impl FnOnce(&RunManifest) -> Result<(),String>) -> Result<RunManifest,String> {
    let _lock = StoreLock::acquire(dir)?;
    let mut previous = load_manifest_in(dir,id)?;
    if previous.kind != RunKind::Agent || previous.ticket.is_none() { return Err("Repair requires a ticketed author run".into()); }
    if previous.next_run_id.is_none() {
        let children = list_ids_in(dir)?.iter().map(|id| load_manifest_in(dir,id)).collect::<Result<Vec<_>,_>>()?;
        let matching: Vec<_> = children.iter().filter(|r| r.previous_run_id.as_deref() == Some(id) && r.ticket == previous.ticket && r.worktree_path == previous.worktree_path).collect();
        if matching.len() > 1 { return Err("Multiple continuation identities; owner recovery required".into()); }
        if let Some(child) = matching.first() {
            // Only replay our reserved repair, not arbitrary successor data.
            if child.branch != previous.branch || child.state != RunState::Requested || !child.last_signal.starts_with("independent review requested author repair after ") { return Err("Existing successor requires recovery".into()); }
            previous.state = RunState::Retired;
            previous.retirement = Some(Retirement { started_at:at,quiet_since:at,capture_bytes:proof.capture_bytes,requirement:String::new(),stopped_at:Some(at),dead_since:Some(at) });
            previous.ticket_returned = true; previous.next_run_id = Some(child.run_id.clone());
            persist_locked(dir,&mut previous)?;
        }
    }
    if let Some(next) = &previous.next_run_id {
        let child = load_manifest_in(dir,next)?;
        if child.previous_run_id.as_deref() == Some(id) && child.ticket == previous.ticket && child.worktree_path == previous.worktree_path && child.branch == previous.branch {
            return Ok(child);
        }
        return Err("Existing continuation does not match this author".into());
    }
    if !(proof.pid_absent && !proof.pid_alive && proof.session_known && !proof.session_alive
        && proof.capture_known && proof.capture_quiet && proof.worktree_exists && proof.branch_matches && !proof.commit.is_empty()) {
        return Err("Author stop/publication proof is incomplete; preserve existing work and inspect the worker".into());
    }
    for other_id in list_ids_in(dir)? {
        let other = load_manifest_in(dir,&other_id)?;
        if other.run_id != id && other.kind == RunKind::Agent && !other.state.terminal()
            && (other.ticket == previous.ticket || other.worktree_path == previous.worktree_path) {
            return Err("Another author owns this ticket or worktree; repair was not reserved".into());
        }
    }
    let mut next = RunManifest::requested(&previous.agent_handle,&previous.runtime_id,&previous.worktree_path,previous.ticket.clone(),previous.model.clone(),&[],at);
    next.remote_env = previous.remote_env.clone();
    next.project = previous.project.clone(); next.branch = previous.branch.clone(); next.previous_run_id = Some(previous.run_id.clone()); next.last_commit = proof.commit.clone();
    next.last_signal = format!("independent review requested author repair after {}",previous.run_id);
    admit(&next)?;
    previous.state = RunState::Retired;
    previous.retirement = Some(Retirement { started_at:at,quiet_since:at,capture_bytes:proof.capture_bytes,requirement:String::new(),stopped_at:Some(at),dead_since:Some(at) });
    previous.ticket_returned = true; previous.next_run_id = Some(next.run_id.clone());
    // Persist child first. A restart recovers the same child by predecessor;
    // callers must not replace it with a new ID after an interrupted write.
    persist_locked(dir,&mut next)?;
    persist_locked(dir,&mut previous)?;
    Ok(next)
}

/// Native admission records this refusal before entering Starting. It applies
/// to initial requests and continuations, including legacy policy refusals
/// without a typed repository/spend prelaunch phase. Missing process evidence
/// alone is never proof: the persisted admission_refused flag is required.
pub(crate) fn admission_refused_before_execution(run: &RunManifest) -> bool {
    run.kind == RunKind::Agent && run.state == RunState::Failed && run.admission_refused
        && run.pid.is_none() && run.process_birth.is_none()
        && run.pty_session.is_none() && run.zellij_session.is_none()
        && run.last_hook_at.is_none() && run.capture_bytes == 0
}

/// Native proof that an initial request never reached worker execution.
pub(crate) fn initial_admission_refused(run: &RunManifest) -> bool {
    run.previous_run_id.is_none() && admission_refused_before_execution(run)
}

/// Admission and retirement share this lock. A pending successor reserves its
/// predecessor's worktree but is not runnable until triage binds a profile.
pub fn continuation_in(dir: &Path, ticket: &str) -> Result<Option<RunManifest>, String> {
    let runs = list_ids_in(dir)?
        .iter()
        .map(|id| load_manifest_in(dir, id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut pending = None;
    for run in &runs {
        if run.ticket.as_deref() != Some(ticket)
            || (run.previous_run_id.is_none() && !initial_admission_refused(run))
            || run.next_run_id.is_some()
            || runs
                .iter()
                .any(|child| child.previous_run_id.as_deref() == Some(&run.run_id))
        {
            continue;
        }
        if run.state == RunState::Failed && !run.admission_refused {
            return Err(format!("successor {} failed after admission; retain its worktree until recovery proves it stopped", run.run_id));
        }
        if run.state == RunState::Requested
            || (run.state == RunState::Failed && run.admission_refused)
        {
            if pending.is_some() {
                return Err(format!("multiple pending successors for {ticket}"));
            }
            pending = Some(run.clone());
        }
    }
    Ok(pending)
}

/// A run on this ticket that is still alive by the registry's own record:
/// admitted and not terminal. Dispatch must refuse a second one. On
/// 2026-09-08 XNAUT-266 got two claude runs in the same worktree because the
/// writer lease is per handle and cannot see a same-handle twin; the second
/// noticed and stood down, which was luck.
pub fn live_run_for_ticket_in(dir: &Path, ticket: &str) -> Result<Option<RunManifest>, String> {
    for id in list_ids_in(dir)? {
        let run = load_manifest_in(dir, &id)?;
        if run.ticket.as_deref() == Some(ticket) && is_live_agent_run(&run) {
            return Ok(Some(run));
        }
    }
    Ok(None)
}

/// The newest live agent run writing in `worktree`, optionally narrowed to a
/// PTY/zellij session and agent handle. Paths are canonicalised because a macOS
/// checkout can be reached through `/tmp` or `/private/tmp`, and the writer
/// lease treats those as the same worktree too.
pub fn live_run_for_worktree_in(
    dir: &Path,
    worktree: &Path,
    session: Option<&str>,
    handle: Option<&str>,
) -> Result<Option<RunManifest>, String> {
    let tree = worktree
        .canonicalize()
        .map_err(|error| format!("could not resolve {}: {error}", worktree.display()))?;
    let mut newest = None;
    for id in list_ids_in(dir)? {
        let run = load_manifest_in(dir, &id)?;
        let same_tree = Path::new(&run.worktree_path)
            .canonicalize()
            .ok()
            .as_ref()
            == Some(&tree);
        let same_session = session.is_none_or(|wanted| {
            run.pty_session.as_deref() == Some(wanted)
                || run.zellij_session.as_deref() == Some(wanted)
        });
        let same_handle = handle.is_none_or(|wanted| run.agent_handle == wanted);
        if same_tree
            && same_session
            && same_handle
            && is_live_agent_run(&run)
            && newest
                .as_ref()
                .is_none_or(|current: &RunManifest| run.started_at > current.started_at)
        {
            newest = Some(run);
        }
    }
    Ok(newest)
}

#[cfg(test)]
mod live_worktree_tests {
    use super::*;

    #[test]
    fn a_worktree_names_only_its_newest_live_run() {
        let root = std::env::temp_dir().join(format!(
            "xnaut-live-worktree-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let registry = root.join("registry");
        let worktree = root.join("worktree");
        std::fs::create_dir_all(&registry).unwrap();
        std::fs::create_dir_all(&worktree).unwrap();

        let requested = RunManifest::requested(
            "builder",
            "codex",
            worktree.to_str().unwrap(),
            Some("XNAUT-379".into()),
            None,
            &[],
            now_ms(),
        );
        let id = requested.run_id.clone();
        request_in(&registry, requested, || Ok(())).unwrap();
        update_in(&registry, &id, |run| {
            run.state = RunState::Running;
            run.pty_session = Some("session-379".into());
        })
        .unwrap();

        let live = live_run_for_worktree_in(
            &registry,
            &worktree,
            Some("session-379"),
            Some("builder"),
        )
            .unwrap()
            .unwrap();
        assert_eq!(live.run_id, id);
        assert_eq!(live.ticket.as_deref(), Some("XNAUT-379"));
        assert!(live_run_for_worktree_in(&registry, &worktree, Some("other"), None)
            .unwrap()
            .is_none());
        assert!(live_run_for_worktree_in(&registry, &worktree, None, Some("other"))
            .unwrap()
            .is_none());

        update_in(&registry, &id, |run| run.state = RunState::Done).unwrap();
        assert!(live_run_for_worktree_in(&registry, &worktree, None, None)
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Is this run an agent that nobody has retired, superseded or finished?
///
/// The predicate `live_run_for_ticket_in` has always used, named so a caller
/// asking the same question about EVERY ticket at once cannot answer it a
/// second, slightly different way.
fn is_live_agent_run(run: &RunManifest) -> bool {
    run.kind == RunKind::Agent
        && !run.state.terminal()
        && run.state != RunState::Requested
        && run.next_run_id.is_none()
}

/// Every ticket with a live run, in one pass.
///
/// A swarm plan asks this of the whole board at once (XNAUT-354); asking
/// `live_run_for_ticket_in` per ticket would re-read the registry once per
/// ticket for an answer one read already contains.
pub fn live_tickets_in(dir: &Path) -> Result<std::collections::HashSet<String>, String> {
    let mut live = std::collections::HashSet::new();
    for id in list_ids_in(dir)? {
        let run = load_manifest_in(dir, &id)?;
        if let Some(ticket) = run.ticket.clone() {
            if is_live_agent_run(&run) {
                live.insert(ticket);
            }
        }
    }
    Ok(live)
}

/// The protected states must block even same-handle and dead-supervisor lease
/// reclamation. A new supervisor does not prove the old child stopped.
pub fn protected_worktree_in(dir: &Path, worktree: &Path) -> Result<Option<String>, String> {
    let real = worktree
        .canonicalize()
        .unwrap_or_else(|_| worktree.to_path_buf());
    for id in list_ids_in(dir)? {
        let run = load_manifest_in(dir, &id)?;
        if (matches!(run.state, RunState::Retiring | RunState::Undead)
            || (run.state == RunState::Retired && run.retirement.is_some() && !run.ticket_returned))
            && Path::new(&run.worktree_path)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(&run.worktree_path))
                == real
        {
            return Ok(Some(run.run_id));
        }
    }
    Ok(None)
}

/// Nonblocking retirement: the sweep supplies a fresh observation each tick.
/// All three proofs AND an observed grace window precede the external lease
/// and PM mutation. Replaying after any interrupted write is safe.
pub fn retirement_step_in(
    dir: &Path,
    id: &str,
    requirement: &str,
    at: i64,
    proof: &Proofs,
    finish: impl FnOnce(&RunManifest) -> Result<(), String>,
) -> Result<RunManifest, String> {
    let _lock = StoreLock::acquire(dir)?;
    let mut run = load_manifest_in(dir, id)?;
    if swap_required(&run, requirement) {
        // Never stop an older record after another run has taken the ticket.
        for other_id in list_ids_in(dir)? {
            if other_id == id {
                continue;
            }
            let other = load_manifest_in(dir, &other_id)?;
            if other.started_at >= run.started_at
                && !other.state.terminal()
                && (other.ticket == run.ticket || other.worktree_path == run.worktree_path)
            {
                return Ok(run);
            }
        }
        run.state = RunState::Retiring;
        run.pid = proof.pid.or(run.pid);
        run.process_birth = proof.process_birth.clone().or(run.process_birth);
        run.retirement = Some(Retirement {
            started_at: at,
            quiet_since: at,
            capture_bytes: proof.capture_bytes,
            requirement: requirement.trim().into(),
            stopped_at: None,
            dead_since: (proof.pid_absent && proof.session_known && !proof.session_alive)
                .then_some(at),
        });
        persist_locked(dir, &mut run)?;
        return Ok(run);
    }
    if run.state == RunState::Retiring {
        let stop = run
            .retirement
            .as_mut()
            .ok_or("retiring run lacks stop attempt")?;
        if stop.capture_bytes != proof.capture_bytes || !proof.capture_known {
            stop.capture_bytes = proof.capture_bytes;
            stop.quiet_since = at;
        }
        // A delayed sweep can first observe the final buffered bytes after its
        // nominal deadline. Give a newly confirmed dead writer a full quiet
        // window, while bounding a capture that continues growing after death.
        if proof.pid_absent && proof.session_known && !proof.session_alive {
            stop.dead_since.get_or_insert(at);
        }
        // This is the transfer guard. Removing it must break the refusal test.
        let proven = proof.pid_absent
            && !proof.pid_alive
            && proof.session_known
            && !proof.session_alive
            && proof.capture_known
            && at.saturating_sub(stop.quiet_since) >= GRACE_MS;
        if proven {
            stop.stopped_at = Some(at);
            run.state = RunState::Retired;
        } else if at.saturating_sub(stop.dead_since.unwrap_or(stop.started_at)) >= 2 * GRACE_MS {
            run.state = RunState::Undead;
            run.last_signal = format!("{}; swap refused: pid_absent={}, session_known={}, session_alive={}, capture_known={}, quiet_ms={}",
                run.last_signal, proof.pid_absent, proof.session_known, proof.session_alive,
                proof.capture_known, at.saturating_sub(stop.quiet_since));
        }
        persist_locked(dir, &mut run)?;
    }
    if run.state != RunState::Retired || run.ticket_returned || run.retirement.is_none() {
        return Ok(run);
    }
    if run.retirement.as_ref().and_then(|s| s.stopped_at).is_none() {
        return Err("retired swap lacks stop proof".into());
    }
    // Record the next id first; replay repairs an interrupted reservation using
    // that same id, never allocating a second successor for the worktree.
    if run.next_run_id.is_none() {
        run.next_run_id = Some(new_id(at));
        persist_locked(dir, &mut run)?;
    }
    let next_id = run.next_run_id.as_ref().unwrap();
    if !list_ids_in(dir)?.contains(next_id) {
        let mut next =
            RunManifest::requested("", "", &run.worktree_path, run.ticket.clone(), None, &[], at);
        next.run_id = next_id.clone();
        next.previous_run_id = Some(run.run_id.clone());
        next.branch = run.branch.clone();
        next.project = run.project.clone();
        next.last_signal = format!(
            "continuation requested after {}: {}",
            run.run_id, run.last_signal
        );
        persist_locked(dir, &mut next)?;
    }
    finish(&run)?;
    run.ticket_returned = true;
    persist_locked(dir, &mut run)?;
    Ok(run)
}

/// Do not signal a reused pid or the app itself. Failure to identify/stop the
/// child is resolved by the proof deadline, never by releasing the lease.
pub fn stop_writer(run: &RunManifest) -> Result<(), String> {
    if run.state != RunState::Retiring {
        return Err("run is not retiring".into());
    }
    let mut errors = Vec::new();
    if let Some(pid) = run.pid {
        if pid_answers(pid) {
            if pid == std::process::id()
                || pid == run.owner_pid
                || run.process_birth.is_none()
                || process_birth(pid) != run.process_birth
            {
                errors.push("refused SIGTERM: process identity is not the registered child".into());
            } else {
                #[cfg(unix)]
                if unsafe { libc::kill(pid as i32, libc::SIGTERM) } != 0 {
                    errors.push(std::io::Error::last_os_error().to_string());
                }
            }
        }
    }
    if let Some(session) = &run.zellij_session {
        if let Err(e) = crate::zellij::zellij_delete_session(session.clone()) {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Unlike the UI's best-effort list, an unqueryable session server is unknown,
/// not proof of absence. Each stop observation refreshes the session list.
pub fn observe_swap_in(dir: &Path, run: &RunManifest) -> Proofs {
    let sessions = crate::zellij::sessions_checked();
    let mut proof = observe_in(dir, run, sessions.as_deref().unwrap_or(&[]));
    proof.session_known = run.zellij_session.is_none() || sessions.is_ok();
    proof
}

#[cfg(test)]
mod swap_tests {
    use super::*;
    #[test]
    fn policy_is_explicit_and_declared_waits_win() {
        let mut run = tests::run();
        run.state = RunState::Degraded;
        assert!(swap_required(&run, "required"));
        assert!(!swap_required(&run, ""));
        assert!(!swap_required(&run, "  "));
        run.state = RunState::Blocked;
        assert!(swap_required(&run, "required"));
        run.waiting_on = Some("in-owner".into());
        assert!(!swap_required(&run, "required"));
        run.state = RunState::Degraded;
        assert!(!swap_required(&run, "required"));
        assert!(!model_meets("", "required"));
        assert!(model_meets(" REQUIRED ", "required"));
        assert!(!model_meets("some-other-quality-tier", "required"));
    }
}

/// Triage must not immediately pick the runtime it just retired merely because
/// its profile still advertises the requested model. A later healthy run can
/// establish recovery. With no policy configured, retain the original behavior.
/// How long a runtime that could not be executed here is left alone before
/// anyone tries it again. Long enough that a vanished CLI is not rediscovered
/// once per ticket; short enough that reinstalling one needs no other action.
pub(crate) const LAUNCH_FAILURE_COOLDOWN_MS: i64 = 30 * 60 * 1000;

/// A run that never began. The launch path writes exactly this prefix when the
/// runtime could not be executed at all: a binary that is not there, a login
/// that expired, a provider that refused the session. It is the one failure
/// that says something about the RUNTIME rather than about the work, which is
/// why it withholds the runtime from every ticket and not only from tickets
/// that name a model.
///
/// It expires, deliberately. Without a cooling period the refusal is
/// self-sealing: an unassignable runtime is never dispatched, so no later run
/// can ever prove it healthy again, and reinstalling the CLI would fix nothing
/// until somebody launched it by hand.
fn never_started(run: &RunManifest, now_ms: i64) -> bool {
    run.state == RunState::Failed
        && run.last_signal.starts_with("launch failed:")
        && now_ms.saturating_sub(run.started_at) < LAUNCH_FAILURE_COOLDOWN_MS
}

pub fn runtime_meets_in(
    dir: &Path,
    runtime: &str,
    configured_model: &str,
    requirement: &str,
) -> Result<bool, String> {
    let latest = list_ids_in(dir)?
        .iter()
        .map(|id| load_manifest_in(dir, id))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|r| {
            r.runtime_id == runtime && r.state != RunState::Requested && !r.admission_refused
        })
        .max_by_key(|r| r.started_at);
    // Codex vanished from tron on 2026-09-09 and triage kept handing it work:
    // four dispatches failed one at a time with "agent binary not found:
    // codex", each one correctly clearing the owner, and each next pass
    // assigning the same dead runtime again. The registry had recorded every
    // one of those failures. Only the empty-requirement short-circuit that
    // used to stand here stopped anybody from reading them.
    if latest
        .as_ref()
        .is_some_and(|run| never_started(run, now_ms()))
    {
        return Ok(false);
    }
    if requirement.trim().is_empty() {
        return Ok(true);
    }
    if !model_meets(configured_model, requirement) {
        return Ok(false);
    }
    Ok(latest.is_none_or(|run| {
        !matches!(
            run.state,
            RunState::Degraded | RunState::Retiring | RunState::Undead | RunState::Failed
        ) && !(run.state == RunState::Retired && run.retirement.is_some())
            && !(run.state == RunState::Blocked
                && run
                    .waiting_on
                    .as_deref()
                    .is_none_or(|w| w.trim().is_empty()))
            && model_meets(run.model.as_deref().unwrap_or_default(), requirement)
    }))
}

#[cfg(test)]
mod runtime_policy_tests {
    use super::*;
    #[test]
    fn adoption_refreshes_the_holder_but_cannot_reclaim_a_retiring_writer() {
        let dir = tests::directory("adopt-swap-holder");
        let mut run = tests::run();
        run.zellij_session = Some("adopt-session".into());
        let run = request_in(&dir, run, || Ok(())).unwrap();
        adopt_writer_in(&dir, "adopt-session", 1234, || Ok(())).unwrap();
        assert_eq!(load_manifest_in(&dir, &run.run_id).unwrap().owner_pid, 1234);
        update_in(&dir, &run.run_id, |r| r.state = RunState::Retiring).unwrap();
        assert!(adopt_writer_in(&dir, "adopt-session", 5678, || panic!(
            "retiring lease cannot be reclaimed"
        ))
        .is_err());
        assert_eq!(load_manifest_in(&dir, &run.run_id).unwrap().owner_pid, 1234);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn triage_uses_observed_runtime_health_and_recognizes_recovery() {
        let dir = tests::directory("runtime-policy");
        assert!(runtime_meets_in(&dir, "codex", "required", "required").unwrap());
        let mut run = tests::run();
        run.model = Some("required".into());
        let run = request_in(&dir, run, || Ok(())).unwrap();
        update_in(&dir, &run.run_id, |r| {
            r.state = RunState::Degraded;
            r.model = Some("old".into());
        })
        .unwrap();
        assert!(!runtime_meets_in(&dir, "codex", "required", "required").unwrap());
        assert!(runtime_meets_in(&dir, "codex", "required", "").unwrap());
        assert!(runtime_meets_in(&dir, "other-runtime", "required", "required").unwrap());
        assert!(!runtime_meets_in(&dir, "other-runtime", "other", "required").unwrap());
        let mut recovered = tests::run();
        recovered.started_at = 2000;
        recovered.model = Some("required".into());
        request_in(&dir, recovered, || Ok(())).unwrap();
        assert!(runtime_meets_in(&dir, "codex", "required", "required").unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_runtime_that_could_not_start_is_withheld_from_every_ticket_for_a_while() {
        // Codex vanished from tron on 2026-09-09. Four dispatches failed one
        // at a time with "agent binary not found: codex", each one clearing
        // the owner, and each next triage pass assigning the same dead
        // runtime. None of those tickets named a model, so the model-only
        // gate never looked at the four failures already in the registry.
        let dir = tests::directory("launch-failure-gate");
        let mut run = tests::run();
        run.started_at = now_ms();
        let run = request_in(&dir, run, || Ok(())).unwrap();

        // Healthy to begin with, model requirement or not.
        assert!(runtime_meets_in(&dir, "codex", "any", "").unwrap());

        update_in(&dir, &run.run_id, |r| {
            r.state = RunState::Failed;
            r.last_signal = "launch failed: agent binary not found: codex".into();
        })
        .unwrap();
        assert!(
            !runtime_meets_in(&dir, "codex", "any", "").unwrap(),
            "a ticket that names no model must still not be handed a runtime that cannot start"
        );
        assert!(!runtime_meets_in(&dir, "codex", "any", "required").unwrap());
        assert!(
            runtime_meets_in(&dir, "other-runtime", "any", "").unwrap(),
            "one runtime failing says nothing about another"
        );

        // A run that started and then failed is about the work, not the
        // runtime, and does not withhold it from tickets naming no model.
        update_in(&dir, &run.run_id, |r| {
            r.last_signal = "stalled: alive but capture shows no progress".into();
        })
        .unwrap();
        assert!(runtime_meets_in(&dir, "codex", "any", "").unwrap());

        // The refusal expires, or it would be self-sealing: an unassignable
        // runtime is never dispatched, so nothing could ever prove it healthy.
        update_in(&dir, &run.run_id, |r| {
            r.last_signal = "launch failed: agent binary not found: codex".into();
            r.started_at = now_ms() - LAUNCH_FAILURE_COOLDOWN_MS - 1;
        })
        .unwrap();
        assert!(
            runtime_meets_in(&dir, "codex", "any", "").unwrap(),
            "after the cooling period the CLI is tried again"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// Adoption changes the lease's supervisor pid. Keep the registry in the same
/// transaction so a later retirement releases precisely that adopted holder.
pub fn adopt_writer_in(
    dir: &Path,
    session: &str,
    owner_pid: u32,
    claim: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let _lock = StoreLock::acquire(dir)?;
    let mut matching = Vec::new();
    for id in list_ids_in(dir)? {
        let run = load_manifest_in(dir, &id)?;
        if run.zellij_session.as_deref() == Some(session) {
            if matches!(
                run.state,
                RunState::Retiring | RunState::Undead | RunState::Retired
            ) {
                return Err(format!("run {} retains its writer lease", run.run_id));
            }
            matching.push(run);
        }
    }
    claim()?;
    for mut run in matching {
        run.owner_pid = owner_pid;
        persist_locked(dir, &mut run)?;
    }
    Ok(())
}

// ── One run, for a person to read (XNAUT-328, XNAUT-329) ─────────────────
//
// Every panel that wanted to follow a run to its detail had nowhere to go:
// this module owned the manifest, the signal, what the run waits on and its
// capture, and exposed none of it. An Actions row saying "registry_failed"
// could not be opened, which is what the owner asked about on 2026-09-11.

#[derive(serde::Serialize)]
pub struct RunDetail {
    pub run: RunManifest,
    /// The tail of the captured output, or empty when there is no capture.
    pub capture_tail: String,
    /// Why the capture is not there, when it is not.
    pub capture_note: String,
}

const CAPTURE_TAIL_BYTES: u64 = 16 * 1024;

/// One run's manifest with the end of its capture. Read-only.
#[tauri::command]
pub async fn run_detail(run_id: String) -> Result<RunDetail, String> {
    let dir = crate::agents::registry_dir()?;
    let run = load_manifest_in(&dir, &run_id)?;
    let (capture_tail, capture_note) = match run.output_path.as_deref() {
        None => (String::new(), "this run kept no capture".to_string()),
        Some(p) => match std::fs::metadata(p) {
            Err(e) => (String::new(), format!("capture unreadable: {e}")),
            Ok(m) => {
                let from = m.len().saturating_sub(CAPTURE_TAIL_BYTES);
                match read_tail(std::path::Path::new(p), from) {
                    Ok(s) => (s, String::new()),
                    Err(e) => (String::new(), format!("capture unreadable: {e}")),
                }
            }
        },
    };
    Ok(RunDetail { run, capture_tail, capture_note })
}

fn read_tail(path: &std::path::Path, from: u64) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(from)).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod run_detail_tests {
    use super::*;

    #[test]
    fn a_run_with_no_capture_says_so_instead_of_returning_nothing() {
        let dir = std::env::temp_dir().join(format!("xnaut-rundetail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut run = RunManifest::requested("@claude", "claude", "/tmp/wt", Some("XNAUT-1".into()), None, &[], now_ms());
        run.output_path = None;
        let id = run.run_id.clone();
        std::fs::write(dir.join(format!("{id}.run.json")), serde_json::to_string(&run).unwrap()).unwrap();
        let loaded = load_manifest_in(&dir, &id).unwrap();
        assert_eq!(loaded.run_id, id);

        // And the tail read itself: the last bytes, not the whole file.
        let log = dir.join("capture.log");
        std::fs::write(&log, "a".repeat(40_000) + "THE-END").unwrap();
        let meta = std::fs::metadata(&log).unwrap();
        let tail = read_tail(&log, meta.len().saturating_sub(CAPTURE_TAIL_BYTES)).unwrap();
        assert!(tail.ends_with("THE-END"));
        assert!(tail.len() as u64 <= CAPTURE_TAIL_BYTES + 8, "tail is bounded: {}", tail.len());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod project_attribution_tests {
    use super::*;

    fn sites() -> Vec<ProjectSite> {
        vec![
            ProjectSite { key: "XNAUT".into(), path: "/dev/xnaut".into() },
            ProjectSite { key: "DEEP".into(), path: "/dev/xnaut/vendor/deep".into() },
            ProjectSite { key: "CMGR".into(), path: "/dev/company-manager".into() },
        ]
    }
    fn run(worktree: &str, ticket: Option<&str>) -> RunManifest {
        RunManifest::requested(
            "claude",
            "claude",
            worktree,
            ticket.map(str::to_string),
            None,
            &sites(),
            now_ms(),
        )
    }

    #[test]
    fn a_ticket_names_the_project_even_from_another_projects_worktree() {
        // The ticket is the stronger claim: CMGR work done in xNaut's checkout
        // is still CMGR work.
        assert_eq!(run("/dev/xnaut", Some("CMGR-12")).project, "CMGR");
    }

    #[test]
    fn without_a_ticket_the_worktree_names_the_project() {
        // 235 of this machine's 241 manifests are this case: an agent launched
        // from the app, no ticket, and a worktree that says everything.
        assert_eq!(run("/dev/company-manager", None).project, "CMGR");
        assert_eq!(run("/dev/xnaut/.worktrees/safety-net", None).project, "XNAUT");
    }

    #[test]
    fn the_longest_prefix_wins_and_a_sibling_directory_is_not_a_prefix() {
        // DEEP sits inside XNAUT's path. The deeper project owns the run.
        assert_eq!(run("/dev/xnaut/vendor/deep/src", None).project, "DEEP");
        // Component matching, not string matching: "/dev/xnaut-old" only
        // starts with "/dev/xnaut" as text.
        assert_eq!(run("/dev/xnaut-old", None).project, "");
    }

    #[test]
    fn a_run_outside_every_project_keeps_an_empty_project() {
        // Empty is an answer, not a gap: the work happened outside the board.
        assert_eq!(run("/tmp/scratch", None).project, "");
        assert_eq!(run("", None).project, "");
        // And with no board to match against, nothing is invented.
        assert_eq!(project_for_worktree("/dev/xnaut", &[]), "");
    }
}

// ── The registry as a list (XNAUT-345) ───────────────────────────────────
//
// `run_detail` answers for one id, and there was no way to ask for many. So
// the Observatory read the manifest FILES: a directory listing plus one
// read per file, which made this module's on-disk format a frontend
// dependency. Renaming a field would have broken a panel silently.

/// One run, reduced to what a list of runs is for: who ran, where, for which
/// project, and whether it is still going. Deliberately not the manifest.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RunRow {
    pub run_id: String,
    pub kind: RunKind,
    pub ticket: Option<String>,
    pub project: String,
    pub agent_handle: String,
    pub runtime_id: String,
    pub zellij_session: Option<String>,
    pub worktree_path: String,
    pub branch: String,
    pub state: RunState,
    pub started_at: i64,
    pub last_seen_at: i64,
}

impl From<&RunManifest> for RunRow {
    fn from(run: &RunManifest) -> Self {
        Self {
            run_id: run.run_id.clone(),
            kind: run.kind,
            ticket: run.ticket.clone(),
            project: run.project.clone(),
            agent_handle: run.agent_handle.clone(),
            runtime_id: run.runtime_id.clone(),
            zellij_session: run.zellij_session.clone(),
            worktree_path: run.worktree_path.clone(),
            branch: run.branch.clone(),
            state: run.state,
            started_at: run.started_at,
            last_seen_at: run.last_seen_at,
        }
    }
}

/// How many runs a caller that names no limit gets.
const REGISTRY_LIST_DEFAULT: usize = 100;

/// The newest `limit` runs, newest first.
///
/// Run ids are ULIDs, so the id order IS the time order and the newest can be
/// picked without reading every file. A manifest that will not load is
/// skipped: one corrupt file must not blank a panel that is showing thirty
/// healthy runs.
pub fn registry_rows_in(dir: &Path, limit: usize) -> Result<Vec<RunRow>, String> {
    let ids = list_ids_in(dir)?;
    let mut rows: Vec<RunRow> = Vec::new();
    for id in ids.iter().rev() {
        if rows.len() >= limit {
            break;
        }
        if let Ok(run) = load_manifest_in(dir, id) {
            rows.push(RunRow::from(&run));
        }
    }
    rows.sort_by(|a, b| {
        b.started_at
            .cmp(&a.started_at)
            .then_with(|| b.run_id.cmp(&a.run_id))
    });
    Ok(rows)
}

/// The run registry as a list. Read-only.
#[tauri::command]
pub async fn run_registry_list(limit: Option<usize>) -> Result<Vec<RunRow>, String> {
    let dir = crate::agents::registry_dir()?;
    registry_rows_in(&dir, limit.unwrap_or(REGISTRY_LIST_DEFAULT).max(1))
}

#[cfg(test)]
mod registry_list_tests {
    use super::*;

    fn sites() -> Vec<ProjectSite> {
        vec![ProjectSite { key: "XNAUT".into(), path: "/dev/xnaut".into() }]
    }
    fn registry(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("xnaut-registry-list-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn seed(dir: &Path, at: i64, session: &str) -> RunManifest {
        let mut run = RunManifest::requested(
            "claude", "claude", "/dev/xnaut/.worktrees/safety-net", None, None, &sites(), at,
        );
        run.zellij_session = Some(session.into());
        std::fs::write(
            dir.join(format!("{}.run.json", run.run_id)),
            serde_json::to_string(&run).unwrap(),
        )
        .unwrap();
        run
    }

    #[test]
    fn the_list_is_newest_first_bounded_and_carries_what_a_panel_reads() {
        let dir = registry("order");
        let old = seed(&dir, 1_000_000, "cx-old");
        let mid = seed(&dir, 2_000_000, "cx-mid");
        let new = seed(&dir, 3_000_000, "cx-new");

        let rows = registry_rows_in(&dir, 10).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.run_id.as_str()).collect::<Vec<_>>(),
            vec![new.run_id.as_str(), mid.run_id.as_str(), old.run_id.as_str()],
        );
        // The projection the Observatory joins on: the session name and the
        // project the fix above stamped.
        assert_eq!(rows[0].zellij_session.as_deref(), Some("cx-new"));
        assert_eq!(rows[0].project, "XNAUT");
        assert_eq!(rows[0].worktree_path, "/dev/xnaut/.worktrees/safety-net");
        assert_eq!(rows[0].agent_handle, "claude");
        assert_eq!(rows[0].state, RunState::Requested);
        assert_eq!(rows[0].started_at, 3_000_000);

        // Bounded, and the bound keeps the newest.
        let two = registry_rows_in(&dir, 2).unwrap();
        assert_eq!(two.len(), 2);
        assert_eq!(two[0].run_id, new.run_id);
        assert_eq!(two[1].run_id, mid.run_id);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_manifest_that_will_not_parse_is_skipped_rather_than_fatal() {
        let dir = registry("corrupt");
        let good = seed(&dir, 1_000_000, "cx-good");
        let broken = new_id(2_000_000);
        std::fs::write(dir.join(format!("{broken}.run.json")), "{ not json").unwrap();

        let rows = registry_rows_in(&dir, 10).unwrap();
        assert_eq!(rows.len(), 1, "the corrupt file is skipped, not fatal");
        assert_eq!(rows[0].run_id, good.run_id);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_absent_registry_is_an_empty_list() {
        let dir = std::env::temp_dir().join(format!("xnaut-registry-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(registry_rows_in(&dir, 10).unwrap().is_empty());
    }
}
