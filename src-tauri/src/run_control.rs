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
    pub machine: String,
    pub agent_handle: String,
    pub runtime_id: String,
    pub model: Option<String>,
    pub worktree_path: String,
    pub branch: String,
    pub pty_session: Option<String>,
    pub zellij_session: Option<String>,
    pub output_path: Option<String>,
    /// The supervisor is NOT proof of the child process being alive.
    pub owner_pid: u32,
    pub pid: Option<u32>,
    pub process_birth: Option<String>,
    pub state: RunState,
    pub previous_run_id: Option<String>,
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

impl RunManifest {
    pub fn requested(
        handle: &str,
        runtime: &str,
        worktree: &str,
        ticket: Option<String>,
        model: Option<String>,
        at: i64,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            run_id: new_id(at),
            kind: RunKind::Agent,
            project: ticket
                .as_deref()
                .and_then(|t| t.rsplit_once('-'))
                .map(|(p, _)| p.to_string())
                .unwrap_or_default(),
            ticket,
            machine: hostname(),
            agent_handle: handle.trim_start_matches('@').to_lowercase(),
            runtime_id: runtime.into(),
            model,
            worktree_path: worktree.into(),
            branch: git_value(worktree, &["symbolic-ref", "--short", "HEAD"]),
            pty_session: None,
            zellij_session: None,
            output_path: None,
            owner_pid: std::process::id(),
            pid: None,
            process_birth: None,
            state: RunState::Requested,
            previous_run_id: None,
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
fn hostname() -> String {
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
    persist_locked(dir, &mut run)?;
    if let Err(error) = admit() {
        run.state = RunState::Failed;
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
    change(&mut run);
    persist_locked(dir, &mut run)?;
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
    if run.state.terminal() {
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
    if !proof.branch_matches {
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
        if matches!(run.state, RunState::Done | RunState::Retired | RunState::Retiring | RunState::Undead) {
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
        if run.state.terminal() {
            continue;
        }
        run.last_hook_at = Some(at);
        run.last_seen_at = at;
        run.last_progress_at = at;
        if let Some(status) = status {
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
        run.last_signal = "trusted session signal".into();
        persist_locked(dir, &mut run)?;
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
            agent_handle: "codex".into(),
            runtime_id: "codex".into(),
            model: None,
            worktree_path: "/not-needed-by-the-pure-verdict".into(),
            branch: "agent/test".into(),
            pty_session: Some("test-session".into()),
            zellij_session: None,
            output_path: None,
            owner_pid: 1,
            pid: None,
            process_birth: None,
            state: RunState::Running,
            previous_run_id: None,
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
    pub(crate) fn directory(label: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".xnaut/test-state")
            .join(format!("{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
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
        proof.pid_alive = true;
        proof.branch_matches = false;
        assert!(
            matches!(verdict(&run,&proof,1_000_000),Verdict::Failed(r) if r.contains("branch mismatch"))
        );
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
            && run.waiting_on.as_deref() == Some(wait_id)
        {
            run.waiting_on = None;
            run.last_progress_at = at;
            run.state = RunState::Running;
            run.last_signal = format!("wait answered: {wait_id}");
            persist_locked(dir, &mut run)?;
        }
    }
    Ok(())
}
