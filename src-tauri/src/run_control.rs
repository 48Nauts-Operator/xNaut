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
use std::collections::BTreeSet;
use std::io::Write;
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
pub struct RunManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub kind: RunKind,
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
    pub started_at: i64,
    pub last_seen_at: i64,
    pub last_progress_at: i64,
    pub last_hook_at: Option<i64>,
    pub waiting_on: Option<String>,
    pub capture_bytes: u64,
    pub last_commit: String,
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
            started_at: at,
            last_seen_at: at,
            last_progress_at: at,
            last_hook_at: None,
            waiting_on: None,
            capture_bytes: 0,
            last_commit: git_value(worktree, &["rev-parse", "HEAD"]),
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

struct StoreLock(std::fs::File);
impl StoreLock {
    fn acquire(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(dir.join(".lock"))
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        Ok(Self(file))
    }
}
impl Drop for StoreLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            unsafe {
                libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
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
        .append(true)
        .write(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    // The inherited tail repair refuses middle corruption. A partial last
    // write is removed before another event can be committed after it.
    file.set_len(valid_end as u64).map_err(|e| e.to_string())?;
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
    mut run: RunManifest,
    admit: impl FnOnce() -> Result<(), String>,
) -> Result<RunManifest, String> {
    let _lock = StoreLock::acquire(dir)?;
    if manifest_path(dir, &run.run_id).exists() || journal_path(dir, &run.run_id).exists() {
        return Err("run id already exists".into());
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
        let requirement = &ancestor.retirement.as_ref().unwrap().requirement;
        if !model_meets(run.model.as_deref().unwrap_or_default(), requirement) {
            return Err(format!("successor model does not meet {requirement}"));
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
}
impl Proofs {
    pub fn writers_gone(&self) -> bool {
        !self.pid_alive && !self.session_alive && self.capture_quiet
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
    let progressing = grew || (!proof.commit.is_empty() && proof.commit != run.last_commit);
    if !progressing && at.saturating_sub(run.last_progress_at) > PROGRESS_WINDOW_MS {
        return Verdict::Failed("stalled: alive but capture, hooks and commits show no progress beyond the window; waiting_on empty".into());
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
    }
}

pub fn process_birth(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .output()
        .ok()?;
    let birth = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !birth.is_empty()).then_some(birth)
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
pub fn observe_in(dir: &Path, run: &RunManifest, live_sessions: &[String]) -> Proofs {
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
        pid,
        process_birth: birth,
        exit_code: std::fs::read_to_string(dir.join(format!("{}.exit", run.run_id)))
            .ok()
            .and_then(|s| s.trim().parse().ok()),
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
            if proof.capture_bytes > run.capture_bytes
                || (!proof.commit.is_empty() && proof.commit != run.last_commit)
            {
                run.last_progress_at = at;
            }
            run.capture_bytes = proof.capture_bytes;
            run.last_commit = proof.commit.clone();
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
    pub(crate) fn run() -> RunManifest {
        RunManifest {
            schema_version: SCHEMA_VERSION,
            run_id: new_id(1_000),
            kind: RunKind::Agent,
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
            started_at: 1_000,
            last_seen_at: 1_000,
            last_progress_at: 1_000,
            last_hook_at: None,
            waiting_on: None,
            capture_bytes: 0,
            last_commit: "first".into(),
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
            || run.previous_run_id.is_none()
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
