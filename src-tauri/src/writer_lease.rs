// A writer lease over a checkout: one agent mutates one worktree, or nobody does.
//
// Ported from fusion-harness (MIT), `extensions/fusion-harness/modules/
// writer-lease.ts` by IndyDevDan. The mechanism is his: an exclusive create
// (`fs.openSync(path, "wx")`) at a path keyed by the sha256 of the canonical
// realpath, the holder written into the file with its pid, and a
// `process.kill(pid, 0)` liveness check so a dead owner's lock is reclaimed
// instead of deadlocking the next run.
//
// We needed it because our isolation was a convention. Every agent is *given*
// its own worktree, and nothing at all stopped two of them being handed the
// same one: `agent_build_workspace` derives the destination from the task
// text, so two agents told to do the same work in the same repository landed
// in the same directory, on whichever branch got there first.
//
// Three departures from the original, all forced by the shape of this app:
//
//   1. The holder is (handle, pid), not pid alone. His harness is one process
//      per run, so a pid identifies a writer. Ours is one process for the
//      whole app, so every agent shares xNAUT's pid and pid alone would say
//      they are all the same writer. The handle is what actually distinguishes
//      them; the pid is kept for the liveness check below.
//   2. Same handle reclaims its own lease. Resuming a piece of work is the
//      common case here (`agent_build_workspace` already returns an existing
//      worktree rather than making a second one), and an agent must not be
//      locked out of the directory it is already working in. It re-takes the
//      lease rather than merely passing the check, so the record always names
//      a live pid; a resumed lease still holding the last run's dead pid is
//      one the next agent reclaims out from under a working holder.
//   3. The lock lives under our config directory rather than beside the
//      checkout, so it never shows up in `git status` of the worktree it
//      protects, and survives that worktree being removed and remade.
//
// ponytail: a lease outlives the run that took it. It is dropped when the
// worktree is removed (`worktree::remove_worktree`) or when the app's pid
// dies; there is no per-run release, because a run has no single end here.
// The cost is that a second agent sent at a finished agent's worktree is
// refused rather than admitted, and the refusal names the holder and the lock
// file. Wire release into the run lifecycle if that refusal ever becomes wrong
// rather than merely conservative.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Holder {
    pub handle: String,
    pub pid: u32,
    pub path: String,
    pub at: String,
}

/// Take the lease on `dir` for `handle`, or explain who holds it.
///
/// Ok(()) means this handle may write there: it took the lease, already had
/// it, or reclaimed one whose owner is gone.
pub fn claim(dir: &Path, handle: &str) -> Result<(), String> {
    let lock = lock_path(dir)?;
    let holder = Holder {
        handle: handle.to_string(),
        pid: std::process::id(),
        path: dir.to_string_lossy().into_owned(),
        at: chrono::Utc::now().to_rfc3339(),
    };
    match write_new(&lock, &holder) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() != ErrorKind::AlreadyExists => {
            return Err(format!("could not take the worktree lease: {error}"));
        }
        Err(_) => {}
    }
    // Somebody holds it. Three ways that is not a refusal: it is us, its owner
    // is dead, or the file is unreadable and therefore tells us nothing.
    match read_holder(&lock) {
        Some(current) if current.handle != handle && pid_alive(current.pid) => Err(format!(
            "@{} is already building in {} (pid {}). Give this agent its own \
             worktree, or release the lease at {}.",
            current.handle,
            dir.display(),
            current.pid,
            lock.display()
        )),
        _ => {
            // Ours to take: this handle resuming its own work, a dead owner,
            // or a file that tells us nothing. Rewrite it either way. A
            // resumed lease left holding the previous run's dead pid is one
            // the next agent reclaims out from under a live holder.
            //
            // Not atomic against another claimant reclaiming the same dead
            // lease in the same instant; that race needs two agents starting
            // inside the same millisecond on a worktree whose owner has just
            // died, and the loser writes an all but identical file.
            let _ = fs::remove_file(&lock);
            write_new(&lock, &holder)
                .map_err(|error| format!("could not reclaim the worktree lease: {error}"))
        }
    }
}

/// Drop the lease on `dir`, if there is one. Called when the worktree itself
/// goes away, which is the one moment nobody can still be writing there.
pub fn release(dir: &Path) {
    if let Ok(lock) = lock_path(dir) {
        let _ = fs::remove_file(lock);
    }
}

fn lock_path(dir: &Path) -> Result<PathBuf, String> {
    // The canonical realpath, so `/tmp/x` and `/private/tmp/x` are one lease
    // and a symlinked worktree cannot be claimed twice under two names.
    let real = dir
        .canonicalize()
        .map_err(|error| format!("could not resolve {}: {error}", dir.display()))?;
    let key = format!("{:x}", Sha256::digest(real.to_string_lossy().as_bytes()));
    let leases = lease_dir()?;
    fs::create_dir_all(&leases)
        .map_err(|error| format!("could not create the lease directory: {error}"))?;
    Ok(leases.join(format!("{key}.json")))
}

fn lease_dir() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("XNAUT_LEASE_DIR") {
        return Ok(PathBuf::from(root));
    }
    dirs::config_dir()
        .map(|dir| dir.join("xnaut").join("worktree-leases"))
        .ok_or_else(|| "could not resolve the config directory".to_string())
}

/// The exclusive create. This is the whole lock: two callers race here and the
/// kernel picks one.
fn write_new(lock: &Path, holder: &Holder) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock)?;
    let body = serde_json::to_vec_pretty(holder).unwrap_or_default();
    file.write_all(&body)
}

fn read_holder(lock: &Path) -> Option<Holder> {
    serde_json::from_slice(&fs::read(lock).ok()?).ok()
}

/// `kill(pid, 0)`: does this process still exist. An unreadable answer counts
/// as alive, because refusing a live worktree is recoverable and handing two
/// agents one checkout is not.
///
/// Shared with `housekeeper.rs`, which asks the same question of the pid inside
/// a git worktree lock. Both want the same fail-safe direction, and a second
/// copy would be free to drift into the other one.
#[cfg(unix)]
pub(crate) fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false; // we never write 0; `kill -0 0` signals our own group
    }
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(true)
}

#[cfg(not(unix))]
pub(crate) fn pid_alive(_pid: u32) -> bool {
    true // ponytail: no cheap probe on Windows; the refusal names the lock file.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every test shares ONE lease directory and gets its OWN worktree path.
    /// The other way round races: cargo runs these on threads of one process,
    /// and `XNAUT_LEASE_DIR` is process-global.
    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join("xnaut-lease-tests");
        fs::create_dir_all(root.join("leases")).unwrap();
        std::env::set_var("XNAUT_LEASE_DIR", root.join("leases"));
        let tree = root.join(name);
        let _ = fs::remove_dir_all(&tree);
        fs::create_dir_all(&tree).unwrap();
        release(&tree); // a lease left by an earlier run is not this run's state
        tree
    }

    /// A pid that WAS a process and is not one now: run one and reap it.
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    #[test]
    fn one_agent_takes_it_and_keeps_it() {
        let tree = scratch("same-handle");
        claim(&tree, "claude").unwrap();
        // Resuming the same work must not lock the agent out of its own tree.
        claim(&tree, "claude").unwrap();
        assert_eq!(
            read_holder(&lock_path(&tree).unwrap()).unwrap().handle,
            "claude"
        );
    }

    #[test]
    fn a_second_agent_is_refused_by_name() {
        let tree = scratch("second-agent");
        claim(&tree, "claude").unwrap();
        let error = claim(&tree, "codex").unwrap_err();
        assert!(
            error.contains("@claude"),
            "the refusal names the holder: {error}"
        );
        assert_eq!(
            read_holder(&lock_path(&tree).unwrap()).unwrap().handle,
            "claude",
            "a refused claim takes nothing"
        );
    }

    /// The mechanism above was wired only into the build flow's workspace
    /// step. Dispatch, cold wakes and direct launches all reach
    /// `agent_profile_launch` with a worktree already chosen, and on
    /// 2026-09-05 a second agent launched straight into a worktree the first
    /// was editing. The claim has to sit in the launch, where every fresh run
    /// passes, and this test fails if someone moves it out again.
    #[test]
    fn every_fresh_launch_claims_the_lease() {
        let source = include_str!("agent_profiles.rs");
        let start = source
            .find("pub async fn agent_profile_launch(")
            .expect("the launch entry exists");
        let body = &source[start..];
        let end = body.find("\n#[tauri::command]").unwrap_or(body.len());
        let needle = ["writer_lease::", "claim("].concat();
        assert!(
            body[..end].contains(&needle),
            "agent_profile_launch no longer takes the writer lease before spawning"
        );
    }

    #[test]
    fn a_dead_owners_lease_is_reclaimed_not_deadlocked() {
        let tree = scratch("dead-owner");
        let lock = lock_path(&tree).unwrap();
        let dead = Holder {
            handle: "codex".into(),
            pid: dead_pid(),
            path: tree.to_string_lossy().into_owned(),
            at: "2026-01-01T00:00:00Z".into(),
        };
        write_new(&lock, &dead).unwrap();
        claim(&tree, "claude").unwrap();
        assert_eq!(
            read_holder(&lock_path(&tree).unwrap()).unwrap().handle,
            "claude"
        );
    }

    #[test]
    fn resuming_refreshes_the_pid_so_nobody_reclaims_a_live_lease() {
        // Found by running the end-to-end test twice: a holder that resumed
        // its own lease used to return Ok without rewriting the record, so
        // the lease still named the previous run's dead pid and the next
        // agent reclaimed a worktree that was very much in use.
        let tree = scratch("resume-refreshes");
        let lock = lock_path(&tree).unwrap();
        write_new(
            &lock,
            &Holder {
                handle: "claude".into(),
                pid: dead_pid(),
                path: tree.to_string_lossy().into_owned(),
                at: "2026-08-27T00:00:00Z".into(),
            },
        )
        .unwrap();
        claim(&tree, "claude").unwrap();
        assert_eq!(
            read_holder(&lock).unwrap().pid,
            std::process::id(),
            "a resumed lease has to name a live process"
        );
        claim(&tree, "codex").unwrap_err();
    }

    #[test]
    fn a_corrupt_lease_does_not_wedge_the_worktree() {
        let tree = scratch("corrupt");
        fs::write(lock_path(&tree).unwrap(), b"not json").unwrap();
        claim(&tree, "claude").unwrap();
        assert_eq!(
            read_holder(&lock_path(&tree).unwrap()).unwrap().handle,
            "claude"
        );
    }

    #[test]
    fn releasing_hands_the_worktree_to_the_next_agent() {
        let tree = scratch("release");
        claim(&tree, "claude").unwrap();
        release(&tree);
        assert!(read_holder(&lock_path(&tree).unwrap()).is_none());
        claim(&tree, "codex").unwrap();
    }
}
