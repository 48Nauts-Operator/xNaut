// The disk this app fills and never empties.
//
// Every agent run gets a git worktree, and the first Rust build inside it
// creates a target directory of roughly six gigabytes that nothing ever
// removes. Measured twice on André's machine:
//
//   2026-09-01   168 GB across 25 worktrees, disk at 94%.  Cleared by hand.
//   2026-09-03   136 GB across 26 agent worktrees, 98%, 23 GB free.  By hand.
//
// Both times he noticed before the app did, and that is the actual defect.
// xNAUT created those worktrees, knows where every one of them is, and said
// nothing. This module is the part that speaks up.
//
// ---- shape ----------------------------------------------------------------
//
// It is the disk analogue of `scheduler::reap_idle_runs`, deliberately: every
// fact is injected so the decision is testable without a repo, every gate can
// only say NO, and the fail-safe direction is stated rather than assumed.
//
// THE FAIL-SAFE DIRECTION: an unreadable answer keeps. Leaving six gigabytes on
// a disk costs six gigabytes and the owner can always press the button again;
// removing a checkout that held the only copy of something costs work no ref
// can bring back. So every gate below that cannot get an answer keeps the
// worktree, and `Facts` carries `Option<bool>` where the honest answer is "I
// could not tell" rather than a `false` that reads like a decision.
//
// ---- the four safety rules, in the order they matter ----------------------
//
//   1. Uncommitted changes are never touched. Not removed, not cleaned around.
//   2. A HEAD the mainline does not already contain is never touched. The
//      branch is the deliverable and the worktree is scaffolding; removing a
//      worktree does NOT delete its branch, which is the whole reason any of
//      this is safe.
//   3. `target/` is only ever cleared, never the source beside it, and only
//      where a build regenerates it.
//   4. Nothing happens without an explicit, per-item apply. The default is a
//      report.
//
// Rule 1 is the one that bites. The prototype at `scripts/housekeeper.mjs`
// listed one worktree as BOTH clearable and kept on its first dry run: 27 GB of
// build cache sitting beside an uncommitted file. Clearing a target directory
// next to unsaved work is probably harmless and definitely contradicts what the
// report said it was doing, and a housekeeper nobody trusts does not get run.
// Hence `Report` holds ONE row per thing, offered or kept with a reason: a row
// cannot appear in two lists when there is only one list.
//
// ---- what is NOT automatic, and why ---------------------------------------
//
// Nothing removes anything on a timer. The tick only measures the volume and
// writes a ledger line. Clearing a regenerable build cache would be defensible
// automatically, and it is still not, for this slice: the report has never been
// run against André's real machine, and the first thing an automatic reclaimer
// has to earn is the belief that its "kept" column is honest. Once the report
// has been read a few times and its refusals hold up, an automatic cache sweep
// at the critical band is a small change on top of `verdict_for_cache`, which
// already decides it.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A build cache is only offered once it has sat unbuilt this long. A worktree
/// somebody is still building in has a target directory that is minutes old.
const STALE_CACHE_DAYS: f64 = 3.0;

/// Where "the disk is filling" starts being worth saying out loud. Below this
/// the footer shows nothing, because a pill that is always there is furniture.
pub const WARN_PCT: u8 = 80;
pub const HIGH_PCT: u8 = 90;
pub const CRITICAL_PCT: u8 = 95;

// ─── What one reclaimable thing is ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The whole checkout. Removing it leaves the branch behind.
    Worktree,
    /// `src-tauri/target` inside a checkout that is being kept.
    BuildCache,
}

/// One row of the report. There is no skip path: every worktree the repo knows
/// about produces at least one of these, offered or kept, and `reason` is
/// always filled. A silent omission is the failure this project keeps paying
/// for, so the type does not have a way to express one.
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    /// What would actually be removed.
    pub path: String,
    /// The checkout this row is about, which for a cache is its parent.
    pub worktree: String,
    pub branch: Option<String>,
    pub kind: Kind,
    pub bytes: u64,
    pub offered: bool,
    /// Why it is offered, or why it is not. Never empty.
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Volume {
    pub total: u64,
    pub free: u64,
    /// Share of the volume a non-root process can no longer write to. Runs
    /// about a point above `df`'s capacity column on APFS; the reason and the
    /// measurement behind that are on `used_percent`.
    pub used_pct: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// The ref that decided "already merged", resolved to a name. `None` means
    /// no mainline could be resolved at all, in which case nothing is offered.
    pub mainline: Option<String>,
    /// `None` when the volume could not be read. Not zero: an unknown disk and
    /// a full one must not render the same.
    pub volume: Option<Volume>,
    pub items: Vec<Item>,
    pub offered_bytes: u64,
    pub kept_bytes: u64,
}

// ─── The decision ────────────────────────────────────────────────────────────

/// Everything a verdict needs, injected. No filesystem, no git, no clock.
#[derive(Debug, Clone)]
pub struct Facts {
    /// The repository's own working tree, as opposed to a linked worktree.
    pub is_repo_checkout: bool,
    /// Created by an agent harness under `.claude/worktrees/agent-*`. Only
    /// these are ever offered for removal; a worktree a person made by hand is
    /// their filing, not our litter.
    pub is_agent_worktree: bool,
    /// The registration is there and the directory is not.
    pub missing: bool,
    /// Count of uncommitted and untracked files. `None` when `git status` could
    /// not be read, which keeps.
    pub dirty: Option<usize>,
    /// Whether the mainline already contains this HEAD. `None` when the
    /// question could not be answered, which keeps.
    pub in_mainline: Option<bool>,
    pub lock: Lock,
    /// The agent whose writer lease is live on this directory, if any. A run in
    /// progress, told by the one store that knows (XNAUT-232).
    pub writer: Option<String>,
}

/// A git worktree lock, and whether anyone is still behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lock {
    None,
    /// Locked, and either the process named in the reason is still running or
    /// the reason names nobody we can check. Both keep.
    Held(String),
    /// Locked by a process that has exited. Claude Code writes its pid into the
    /// lock and never lifts it, so this is the ordinary state of every finished
    /// agent worktree; treating it as Held would make this whole module refuse
    /// the exact case it was built for.
    Stale(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Offer(String),
    Keep(String),
}

impl Verdict {
    fn offered(&self) -> bool {
        matches!(self, Verdict::Offer(_))
    }
    fn reason(&self) -> String {
        match self {
            Verdict::Offer(why) | Verdict::Keep(why) => why.clone(),
        }
    }
}

/// Did xNAUT create this worktree for an agent? Three spellings, one answer:
/// Claude Code's own `.claude/worktrees/agent-*`, the dispatch path's
/// `<repo>-worktrees/agent-<handle>-<ticket>` (XNAUT-153), and any checkout on
/// an `agent/<handle>/...` branch. Before 2026-09-06 only the first counted,
/// so the fourteen gigabytes of finished dispatch worktrees were "not xNAUT's
/// to remove" while the disk sat at 88%.
pub(crate) fn is_agent_worktree(path: &str, branch: Option<&str>) -> bool {
    path.contains("/.claude/worktrees/agent-")
        || path.contains("-worktrees/agent-")
        || branch.is_some_and(|b| b.starts_with("agent/"))
}

/// May this whole checkout be offered for removal?
///
/// Read the gates in order; each one is a way to say no, and the last line is
/// the only path to yes. `mainline` is named in the refusals because "HEAD is
/// not in the mainline" without saying WHICH mainline is the kind of message
/// that gets ignored.
pub fn verdict_for_worktree(f: &Facts, mainline: Option<&str>) -> Verdict {
    if f.is_repo_checkout {
        return Verdict::Keep("the repository's own working tree".into());
    }
    if f.missing {
        // Nothing on disk to reclaim, and `git worktree prune` is the tool for
        // a dangling registration. Saying so is more useful than silently
        // dropping the row.
        return Verdict::Keep(
            "the directory is already gone; `git worktree prune` clears the registration".into(),
        );
    }
    // RULE 1, and the fail-safe direction with it.
    match f.dirty {
        None => return Verdict::Keep("git status could not be read here".into()),
        Some(n) if n > 0 => {
            return Verdict::Keep(format!(
                "{n} uncommitted file{}",
                if n == 1 { "" } else { "s" }
            ))
        }
        Some(_) => {}
    }
    // RULE 2. A mainline that would not resolve leaves `in_mainline` as None
    // for every worktree, so an unusable ref offers nothing rather than
    // everything.
    let Some(mainline) = mainline else {
        return Verdict::Keep("no mainline ref resolved, so nothing counts as merged".into());
    };
    match f.in_mainline {
        None => {
            return Verdict::Keep(format!(
                "could not tell whether {mainline} contains this HEAD"
            ))
        }
        Some(false) => return Verdict::Keep(format!("HEAD is not in {mainline}")),
        Some(true) => {}
    }
    if let Lock::Held(reason) = &f.lock {
        return Verdict::Keep(format!("locked: {reason}"));
    }
    // RULE 4, and the one this module was missing. An agent AT WORK in a
    // worktree has committed nothing yet, so rules 1 and 2 both pass: nothing
    // uncommitted, and a branch with no commits is trivially contained in the
    // mainline. On 2026-09-06 the automatic reclaim deleted XNAUT-300's
    // worktree thirty-one seconds after its agent was launched into it. Git's
    // lock is not this signal; xNAUT's writer lease is.
    // RULE 4, and the one this module was missing. An agent AT WORK in a
    // worktree has committed nothing yet, so rules 1 and 2 both pass: nothing
    // uncommitted, and a branch with no commits is trivially contained in the
    // mainline. On 2026-09-06 the automatic reclaim deleted XNAUT-300's
    // worktree thirty-one seconds after its agent was launched into it. Git's
    // lock is not this signal; xNAUT's writer lease is.
    if let Some(holder) = &f.writer {
        return Verdict::Keep(format!("@{holder} is building here"));
    }
    if !f.is_agent_worktree {
        return Verdict::Keep(
            "not an agent worktree; xNAUT did not create it, so it is not xNAUT's to remove".into(),
        );
    }
    Verdict::Offer(format!(
        "merged into {mainline}, nothing uncommitted, no live agent"
    ))
}

/// May this checkout's build cache be cleared, given the checkout itself is
/// being kept?
///
/// Only called for kept worktrees, so a worktree and its own cache can never
/// both be offered and the same bytes can never be counted twice.
///
/// `!dirty` is not paranoia; it is rule 1 and it is the bug the prototype's
/// first dry run produced. See the module note.
pub fn verdict_for_cache(f: &Facts, idle_days: f64) -> Verdict {
    if f.missing {
        return Verdict::Keep("the directory is already gone".into());
    }
    match f.dirty {
        None => return Verdict::Keep("git status could not be read here".into()),
        Some(n) if n > 0 => {
            return Verdict::Keep(format!(
                "{n} uncommitted file{} beside it",
                if n == 1 { "" } else { "s" }
            ))
        }
        Some(_) => {}
    }
    if let Lock::Held(reason) = &f.lock {
        return Verdict::Keep(format!("locked: {reason}"));
    }
    if let Some(holder) = &f.writer {
        return Verdict::Keep(format!("@{holder} is building here"));
    }
    if idle_days <= STALE_CACHE_DAYS {
        return Verdict::Keep(format!(
            "built {} ago; a cache in use is not litter",
            humanise_days(idle_days)
        ));
    }
    Verdict::Offer(format!(
        "build cache, idle {}, the next build regenerates it",
        humanise_days(idle_days)
    ))
}

fn humanise_days(days: f64) -> String {
    if days < 1.0 {
        format!("{}h", (days * 24.0).round() as i64)
    } else {
        format!("{}d", days.round() as i64)
    }
}

// ─── Reading the world ───────────────────────────────────────────────────────

/// Reads git's lock reason and asks whether whoever took it is still there.
///
/// The pid probe is `writer_lease::pid_alive`, shared rather than copied: both
/// callers want the same fail-safe direction (unreadable counts as alive) and
/// two copies are free to drift apart.
pub fn classify_lock(reason: Option<&str>, is_locked: bool) -> Lock {
    if !is_locked {
        return Lock::None;
    }
    let reason = reason.unwrap_or("no reason given").to_string();
    match pid_in(&reason) {
        // A lock naming nobody could belong to anything, so it holds.
        None => Lock::Held(reason),
        Some(pid) if crate::writer_lease::pid_alive(pid) => Lock::Held(reason),
        Some(_) => Lock::Stale(reason),
    }
}

/// The pid inside a lock reason like
/// "claude agent agent-a09 (pid 38368 start Mon Aug 31 09:20:51 2026)".
/// `None` when the reason does not name one, which keeps.
fn pid_in(reason: &str) -> Option<u32> {
    let after = reason.split("pid ").nth(1)?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Do these two paths name the same thing?
///
/// git reports worktree paths already resolved, so a caller holding the path it
/// was given ("/var/folders/...") and git's answer ("/private/var/folders/...")
/// are the same directory spelled two ways. A byte comparison there makes
/// `reclaim` refuse an item its own report offered, which reads exactly like a
/// safety refusal and is not one. Case handling comes from
/// `worktree::are_worktree_paths_equal`, the one place that already knows
/// Windows is case-insensitive and nothing else is.
fn same_path(a: &Path, b: &Path) -> bool {
    let ra = std::fs::canonicalize(a);
    let rb = std::fs::canonicalize(b);
    crate::worktree::are_worktree_paths_equal(
        ra.as_deref().unwrap_or(a),
        rb.as_deref().unwrap_or(b),
    )
}

fn git_out(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The refs that could mean "the mainline", best first.
///
/// A local `main` is what a person means when they say it, so it leads. Then
/// every remote's own default and its `main`/`master`.
///
/// The remotes are not optional decoration. Measured on André's checkout on
/// 2026-09-04: this repository has NO local `main` at all. Every local branch is
/// feature work, and the mainline lives only as `origin/main` and
/// `forgejo/main`. With just the local names the resolver returned `None`, the
/// fail-safe direction did its job, and every single one of the 28 worktrees was
/// kept with "no mainline ref resolved". Safe, and completely inert: the exact
/// 136 GB this module exists to find would still be sitting there.
fn mainline_candidates(repo: &Path) -> Vec<String> {
    let mut refs: Vec<String> = vec!["main".into(), "master".into()];
    for remote in git_out(repo, &["remote"]).unwrap_or_default().lines() {
        let remote = remote.trim();
        if remote.is_empty() {
            continue;
        }
        // The named branches first and `HEAD` last: all three usually resolve
        // to the same commit, and "origin/main" reads as an answer in the
        // report while "origin/HEAD" reads as a shrug. `HEAD` still covers a
        // repo whose default is called something else entirely.
        refs.push(format!("{remote}/main"));
        refs.push(format!("{remote}/master"));
        refs.push(format!("{remote}/HEAD"));
    }
    refs
}

/// The ref that decides "already merged", tried in order, first one that
/// resolves wins. `None` when none of them does, and then nothing is offered.
fn resolve_mainline(repo: &Path, requested: Option<&str>) -> Option<String> {
    let candidates: Vec<String> = match requested {
        Some(r) if !r.trim().is_empty() => vec![r.trim().to_string()],
        _ => mainline_candidates(repo),
    };
    candidates.into_iter().find(|r| {
        git_out(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{r}^{{commit}}"),
            ],
        )
        .is_some()
    })
}

/// Does `mainline` already contain `head`?
///
/// Three outcomes, not two: `git merge-base --is-ancestor` exits 0 for yes and
/// 1 for no, and anything else (a missing object, a broken repo) is a question
/// that was not answered. That third case must not collapse into `false`, which
/// keeps by accident, nor into `true`, which offers by accident.
fn is_ancestor(repo: &Path, head: &str, mainline: &str) -> Option<bool> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["merge-base", "--is-ancestor", head, mainline])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    match status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

/// Does the mainline contain `head`, here OR on any remote this repository
/// tracks?
///
/// The merge that retires an agent worktree is made by NautBot and PUSHED; the
/// local branch of the same name never moves. Asking only the local ref means
/// a worktree whose ticket integrated seconds ago is refused with "HEAD is not
/// in dev", and refused again on every later pass, so it keeps its build cache
/// forever. On 2026-09-09 that was 25 GB across five agent worktrees on tron,
/// three of them for tickets already merged and promoted; the volume reached
/// 100% full at 21:09 and the fleet's runs then failed for reasons that named
/// anything except disk.
///
/// Same three outcomes as `is_ancestor`, because the fail-safe direction is
/// the same: one yes is enough, every ref answering no is a no, and no ref
/// able to answer at all is `None`, which keeps.
fn merged_into(repo: &Path, head: &str, mainline: &str) -> Option<bool> {
    let mut refs = vec![mainline.to_string()];
    // A mainline already written as `origin/dev` is asking about one remote;
    // do not go looking for `origin/origin/dev`.
    if !mainline.contains('/') {
        for remote in git_out(repo, &["remote"]).unwrap_or_default().lines() {
            let remote = remote.trim();
            if !remote.is_empty() {
                refs.push(format!("{remote}/{mainline}"));
            }
        }
    }
    let mut answered = false;
    for reference in refs {
        match is_ancestor(repo, head, &reference) {
            Some(true) => return Some(true),
            Some(false) => answered = true,
            None => {}
        }
    }
    answered.then_some(false)
}

/// Uncommitted and untracked files in a checkout. `None` when git could not
/// answer, which keeps.
fn dirty_count(worktree: &Path) -> Option<usize> {
    let out = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    })
}

/// Bytes under a directory.
///
/// ponytail: THE CEILING. This stats every file, so a six-gigabyte target
/// directory costs a real walk, and twenty-six of them cost twenty-six walks.
/// That is exactly why the report is on demand and the 60s tick reads only
/// `statvfs`, which is a single syscall. If the report ever needs to be
/// cheaper, the fix is a size cache keyed by path and mtime, not a shortcut
/// that guesses.
///
/// Symlinked directories are counted as links and not followed: following them
/// would double-count at best and loop at worst.
///
/// `skip` names directories that belong to some OTHER row of the report and are
/// physically inside this one. Without it a row's number is not what removing
/// that row reclaims. Measured on André's checkout: the repository's own
/// worktree came out at 84.3 GB because the walk descended into `.worktrees/`
/// and `.claude/worktrees/` and counted every nested checkout again, and the
/// report's total read 202 GB on a volume holding rather less than that. A
/// housekeeper whose arithmetic does not add up does not get run.
fn dir_bytes(path: &Path, skip: &[PathBuf]) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            // `DirEntry::metadata` does not follow symlinks, so a symlinked
            // directory lands here as a plain entry and is never descended.
            if meta.is_dir() {
                let child = entry.path();
                if !skip.contains(&child) {
                    stack.push(child);
                }
            } else {
                total += meta.len();
            }
        }
    }
    total
}

/// Days since anything in this directory was written, from its own mtime.
///
/// ponytail: mtime on the target directory itself, which cargo touches when it
/// adds or removes a direct child. A build that only rewrites files deeper down
/// leaves it older than the work, so this UNDER-estimates activity and can only
/// make the cache look staler than it is. That is the wrong direction, so it is
/// paired with the dirty gate above rather than trusted alone.
fn idle_days(path: &Path) -> f64 {
    let elapsed = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .and_then(|t| {
            t.elapsed()
                .map_err(|e| std::io::Error::other(e.to_string()))
        });
    elapsed.map(|d| d.as_secs_f64() / 86_400.0).unwrap_or(0.0)
}

/// Free and total bytes on the volume holding `path`.
#[cfg(unix)]
pub fn volume_usage(path: &Path) -> Option<Volume> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `stat` is a plain POSIX struct written entirely by the call, and
    // the pointer is a valid NUL-terminated path that outlives it.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let unit = stat.f_bsize as u64;
    let blocks = stat.f_blocks as u64;
    let bavail = stat.f_bavail as u64;
    Some(Volume {
        total: blocks * unit,
        // What a non-root process can actually write, which is the number that
        // decides whether the next build succeeds.
        free: bavail * unit,
        used_pct: used_percent(blocks, bavail),
    })
}

#[cfg(not(unix))]
pub fn volume_usage(_path: &Path) -> Option<Volume> {
    // ponytail: no probe on Windows yet. `None` renders as "disk unknown",
    // which is the honest failure; the alternative is a fabricated percentage.
    None
}

/// How much of the volume a non-root process can no longer write to, rounded
/// up.
///
/// ponytail: THE CEILING, and it is worth being precise about because the first
/// version of this claimed to match `df` and did not. Measured on this machine
/// on 2026-09-04, `statfs` and `df -k` called in the same second:
///
///   statfs   blocks 242837545   bfree 43623462   bavail 43623462
///   df -k    blocks 971350180   used 733712236   avail 174493844   -> 81%
///
/// The kernel hands `statfs` the SAME number for `f_bfree` and `f_bavail` on
/// APFS, so `blocks - bfree` cannot be df's Used; df's implied free block count
/// is 59409486, which comes from `getattrlist`/`ATTR_VOL_SPACEUSED` and its
/// separate accounting of purgeable space. Reproducing that is a second
/// platform-specific syscall for one percentage point, and it buys nothing:
///
///   this  82%   what you cannot write to, over the whole volume
///   df    81%   what is really stored, over stored + available
///
/// So the number is defined as what it measures rather than bent toward df. It
/// still runs about a point high, which for a disk-pressure warning is the
/// right direction: early is recoverable, late is the 98% morning.
///
/// Rounded to NEAREST, not up. Rounding up turned a measured 82.04% into 83 and
/// widened the gap with df to two points, one of which was pure rounding noise
/// rather than anything about the disk. A warning should be early because the
/// measurement says so, not because the arithmetic leans.
pub(crate) fn used_percent(blocks: u64, bavail: u64) -> u8 {
    if blocks == 0 {
        return 0;
    }
    let used = blocks.saturating_sub(bavail);
    ((used.saturating_mul(100) + blocks / 2) / blocks).min(100) as u8
}

// ─── The report ──────────────────────────────────────────────────────────────

/// Every worktree the repo knows about, each with a verdict and a reason.
///
/// Enumeration comes from `worktree::list_worktrees`, the one place in this
/// tree that parses `git worktree list --porcelain`. This module deliberately
/// does not become a second one.
pub fn scan(repo: &Path, mainline: Option<&str>) -> Result<Report, String> {
    let worktrees = crate::worktree::list_worktrees(repo)?;
    let mainline = resolve_mainline(repo, mainline);
    let repo_real = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    // Worktrees nest: `.worktrees/*` and `.claude/worktrees/agent-*` live INSIDE
    // the repository's own checkout. Each is its own row, so no row may count
    // another's bytes.
    let all_paths: Vec<PathBuf> = worktrees.iter().map(|w| PathBuf::from(&w.path)).collect();

    let mut items = Vec::new();
    for w in &worktrees {
        let path = PathBuf::from(&w.path);
        let real = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let missing = !path.exists();
        let facts = Facts {
            is_repo_checkout: real == repo_real,
            is_agent_worktree: is_agent_worktree(&w.path, w.branch.as_deref()),
            missing,
            dirty: if missing { None } else { dirty_count(&path) },
            in_mainline: match (&mainline, &w.head) {
                (Some(m), Some(head)) => merged_into(repo, head, m),
                _ => None,
            },
            lock: classify_lock(w.lock_reason.as_deref(), w.is_locked),
            writer: crate::writer_lease::live_holder(&path).map(|h| h.handle),
        };

        let verdict = verdict_for_worktree(&facts, mainline.as_deref());
        let offered = verdict.offered();
        // An offered worktree takes its cache with it, so no separate row and
        // no separate number. A kept one gets a cache row of its own, and then
        // its own row must not include those bytes: EVERY ROW'S SIZE IS EXACTLY
        // WHAT REMOVING THAT ROW RECLAIMS, which is the only reading under
        // which the totals mean anything.
        let cache = path.join("src-tauri").join("target");
        let cache_row = !offered && !missing && cache.is_dir();

        let mut skip: Vec<PathBuf> = all_paths
            .iter()
            .filter(|other| **other != path && other.starts_with(&path))
            .cloned()
            .collect();
        if cache_row {
            skip.push(cache.clone());
        }

        items.push(Item {
            path: w.path.clone(),
            worktree: w.path.clone(),
            branch: w.branch.clone(),
            kind: Kind::Worktree,
            bytes: if missing { 0 } else { dir_bytes(&path, &skip) },
            offered,
            reason: verdict.reason(),
        });
        if !cache_row {
            continue;
        }
        let cache_verdict = verdict_for_cache(&facts, idle_days(&cache));
        items.push(Item {
            path: cache.to_string_lossy().into_owned(),
            worktree: w.path.clone(),
            branch: w.branch.clone(),
            kind: Kind::BuildCache,
            bytes: dir_bytes(&cache, &[]),
            offered: cache_verdict.offered(),
            reason: cache_verdict.reason(),
        });
    }

    let offered_bytes = items.iter().filter(|i| i.offered).map(|i| i.bytes).sum();
    let kept_bytes = items.iter().filter(|i| !i.offered).map(|i| i.bytes).sum();
    Ok(Report {
        mainline,
        volume: volume_usage(repo),
        items,
        offered_bytes,
        kept_bytes,
    })
}

// ─── Applying, one item at a time ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Reclaimed {
    pub path: String,
    pub bytes: u64,
}

/// Removes exactly one thing the report offered.
///
/// The verdict is recomputed here rather than trusted from the caller, and that
/// is the point of the function. Between the report rendering and the owner
/// pressing a button an agent can start writing in that checkout; a frontend
/// that remembers "this row was offered" would then remove work that became
/// unique thirty seconds ago. So the button carries an identity, never a
/// permission.
///
/// Two independent locks on rule 1: this recheck, and `force: false` on the
/// removal, which makes git refuse a dirty checkout on its own.
/// `delete_branch: false` is rule 2 made operational: the branch is the
/// deliverable and it survives.
pub fn reclaim(
    repo: &Path,
    target: &Path,
    kind: Kind,
    mainline: Option<&str>,
) -> Result<Reclaimed, String> {
    let report = scan(repo, mainline)?;
    let item = report
        .items
        .iter()
        .find(|i| i.kind == kind && same_path(Path::new(&i.path), target))
        .ok_or_else(|| format!("{} is not in the report", target.display()))?;
    if !item.offered {
        return Err(format!("refused: {}", item.reason));
    }
    let bytes = item.bytes;
    let branch = item.branch.clone().unwrap_or_else(|| "(detached)".into());
    // Everything below acts on GIT'S spelling of the path, not the caller's.
    // `worktree::remove_worktree` finds its target by string comparison against
    // `git worktree list`, so handing it "/var/folders/..." when git says
    // "/private/var/folders/..." fails with "no worktree at", which reads
    // exactly like a safety refusal and is not one.
    let doomed = PathBuf::from(&item.path);

    match kind {
        Kind::Worktree => {
            // A stale lock (its holder gone) is lifted only now, after the
            // verdict established the holder is dead. `git worktree remove`
            // refuses a locked checkout outright, so without this the ordinary
            // finished-agent worktree could never be removed.
            let _ = crate::worktree::unlock_worktree(repo, &doomed);
            crate::worktree::remove_worktree(
                repo,
                &doomed,
                &crate::worktree::RemoveWorktreeOptions {
                    force: false,
                    delete_branch: Some(false),
                },
            )?;
            crate::ledger::record(
                "worktree_reclaimed",
                "housekeeper",
                "",
                &format!(
                    "removed {} ({}); its branch {branch} was kept",
                    doomed.display(),
                    human_bytes(bytes),
                ),
            );
        }
        Kind::BuildCache => {
            // RULE 3: only ever the target directory, never the source beside
            // it. The path came out of `scan`, which builds it as
            // `<worktree>/src-tauri/target` and nothing else, so this cannot be
            // pointed at a checkout by a caller.
            if !doomed.ends_with("src-tauri/target") {
                return Err(format!(
                    "refused: {} is not a build cache",
                    doomed.display()
                ));
            }
            std::fs::remove_dir_all(&doomed)
                .map_err(|e| format!("could not clear {}: {e}", doomed.display()))?;
            crate::ledger::record(
                "cache_reclaimed",
                "housekeeper",
                "",
                &format!("cleared {} ({})", doomed.display(), human_bytes(bytes)),
            );
        }
    }
    Ok(Reclaimed {
        path: doomed.to_string_lossy().into_owned(),
        bytes,
    })
}

pub fn human_bytes(n: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    if (n as f64) >= GB {
        format!("{:.1} GB", n as f64 / GB)
    } else {
        format!("{} MB", n / (1024 * 1024))
    }
}

// ─── Speaking up on the tick ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Ok,
    Warn,
    High,
    Critical,
}

pub fn band(used_pct: u8) -> Band {
    if used_pct >= CRITICAL_PCT {
        Band::Critical
    } else if used_pct >= HIGH_PCT {
        Band::High
    } else if used_pct >= WARN_PCT {
        Band::Warn
    } else {
        Band::Ok
    }
}

/// Six hours between repeats of the same band. Long enough that a disk sitting
/// at 91% for a week does not bury the ledger, short enough that a warning is
/// still on screen at the end of a working day.
const REPEAT_AFTER_MS: i64 = 6 * 60 * 60 * 1000;

/// Should this tick say something, given what was last said?
///
/// Pure so the rate limit is testable without waiting six hours. Announces on
/// any rise into a worse band, and repeats a bad band every six hours; a fall
/// back to Ok is recorded silently, so the next rise announces again rather
/// than being swallowed as "already told you".
pub(crate) fn should_announce(now: Band, last: Option<(Band, i64)>, now_ms: i64) -> bool {
    if now == Band::Ok {
        return false;
    }
    match last {
        None => true,
        Some((was, _)) if now > was => true,
        Some((_, at)) => now_ms - at >= REPEAT_AFTER_MS,
    }
}

fn last_announced() -> &'static std::sync::Mutex<Option<(Band, i64)>> {
    static LAST: std::sync::OnceLock<std::sync::Mutex<Option<(Band, i64)>>> =
        std::sync::OnceLock::new();
    LAST.get_or_init(|| std::sync::Mutex::new(None))
}

/// Measures the volume the worktrees live on and, when it has crossed into a
/// band worth saying out loud, writes ONE ledger line and emits one event.
///
/// Called from `scheduler::tick`, on the same 60s clock as `reap_idle_runs`,
/// because this is the disk analogue of that reaper and a second background
/// task would be a second thing to reason about. It costs one `statvfs`; the
/// expensive part (`scan`) only runs when a person opens the panel.
///
/// It never removes anything. Surfacing is the whole job here.
pub fn watch_disk(app: &tauri::AppHandle) {
    use tauri::Emitter;
    let Some(home) = dirs::home_dir() else { return };
    let Some(volume) = volume_usage(&home) else {
        return;
    };
    let now_band = band(volume.used_pct);
    if now_band >= Band::Warn && auto_reclaim_due(chrono::Utc::now().timestamp_millis()) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { auto_reclaim(&app).await });
    }
    let now_ms = chrono::Utc::now().timestamp_millis();

    let mut last = last_announced().lock().unwrap_or_else(|e| e.into_inner());
    if !should_announce(now_band, *last, now_ms) {
        if now_band == Band::Ok {
            *last = None;
        }
        return;
    }
    *last = Some((now_band, now_ms));
    drop(last);

    let detail = format!(
        "disk {}% full, {} free. Agent worktrees and their build caches are the \
         usual cause; open Worktrees to see what can be reclaimed.",
        volume.used_pct,
        human_bytes(volume.free),
    );
    crate::ledger::record("disk_pressure", "housekeeper", "", &detail);
    let _ = app.emit(
        "housekeeper://pressure",
        serde_json::json!({ "band": now_band, "usedPct": volume.used_pct, "free": volume.free, "detail": detail }),
    );
}

// ─── Automatic reclaim ────────────────────────────────────────────────────────
//
// The header above said "nothing removes anything on a timer" and why: the
// report's refusals had to be read first. They were, on 2026-09-06: the
// verdicts were applied by hand to 31 worktrees and 60 GB, and every "kept"
// row held up. So at the Warn band and above, the offered rows go, at most
// once an hour, one ledger line each, exactly as the button would have done
// them. The verdict is the safety; the band is only when it is worth asking.

const AUTO_RECLAIM_EVERY_MS: i64 = 60 * 60 * 1000;

fn last_auto_reclaim() -> &'static std::sync::Mutex<Option<i64>> {
    static LAST: std::sync::OnceLock<std::sync::Mutex<Option<i64>>> = std::sync::OnceLock::new();
    LAST.get_or_init(|| std::sync::Mutex::new(None))
}

/// Once an hour. Marks the hour when it says yes, so a slow scan cannot be
/// started twice by two close ticks.
pub(crate) fn auto_reclaim_due(now_ms: i64) -> bool {
    let mut last = last_auto_reclaim().lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|at| now_ms - at < AUTO_RECLAIM_EVERY_MS) {
        return false;
    }
    *last = Some(now_ms);
    true
}

/// The mainline for automatic reclaim is the branch the project's source_path
/// has checked out, not `main`: on xNAUT `main` is a month stale and nothing
/// would ever count as merged.
fn checked_out_branch(dir: &Path) -> Option<String> {
    git_out(dir, &["symbolic-ref", "--short", "HEAD"]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

async fn auto_reclaim(app: &tauri::AppHandle) {
    use tauri::Manager;
    let state = app.state::<crate::state::AppState>();
    let Ok(projects) = crate::project_management::pm_project_list(state).await else {
        return;
    };
    let roots: Vec<PathBuf> = projects
        .iter()
        .map(|p| PathBuf::from(crate::project_management::local_source_path(p).trim()))
        .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
        .collect();
    let (count, bytes) = tokio::task::spawn_blocking(move || {
        let mut seen = std::collections::HashSet::new();
        let (mut count, mut bytes) = (0usize, 0u64);
        for root in roots {
            // One repository, however many of its worktrees are projects.
            let Some(common) = git_out(&root, &["rev-parse", "--path-format=absolute", "--git-common-dir"]) else { continue };
            if !seen.insert(common.trim().to_string()) {
                continue;
            }
            let mainline = checked_out_branch(&root);
            let Ok(report) = scan(&root, mainline.as_deref()) else { continue };
            for item in report.items.iter().filter(|i| i.offered) {
                match reclaim(&root, Path::new(&item.path), item.kind, mainline.as_deref()) {
                    Ok(done) => { count += 1; bytes += done.bytes; }
                    Err(why) => crate::ledger::record("reclaim_refused", "housekeeper", "", &why),
                }
            }
        }
        (count, bytes)
    })
    .await
    .unwrap_or((0, 0));
    if count > 0 {
        crate::ledger::record(
            "auto_reclaimed",
            "housekeeper",
            "",
            &format!("{count} item{} removed, {} back", if count == 1 { "" } else { "s" }, human_bytes(bytes)),
        );
    }
}

/// Reclaim one finished worktree now, because its ticket just integrated.
/// Waiting for the disk warn band meant ten tickets would fill tron before
/// anything was removed (André, 2026-09-08). Same verdict and same locks as
/// the hourly pass: a worktree with a live writer or dirty files is refused
/// and stays for the warn band; the branch is never deleted. Best effort;
/// the refusal is recorded, never raised.
pub fn reclaim_integrated(project_root: &Path, worktree: &Path) {
    let mainline = checked_out_branch(project_root);
    match reclaim(project_root, worktree, Kind::Worktree, mainline.as_deref()) {
        Ok(done) => crate::ledger::record(
            "reclaimed_on_integration",
            "housekeeper",
            "",
            &format!("{} removed, {} back", worktree.display(), human_bytes(done.bytes)),
        ),
        Err(why) => crate::ledger::record("reclaim_refused", "housekeeper", "", &why),
    }
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn housekeeper_report(repo_path: String, mainline: Option<String>) -> Result<Report, String> {
    scan(Path::new(&repo_path), mainline.as_deref())
}

#[tauri::command]
pub fn housekeeper_reclaim(
    repo_path: String,
    path: String,
    kind: Kind,
    mainline: Option<String>,
) -> Result<Reclaimed, String> {
    reclaim(
        Path::new(&repo_path),
        Path::new(&path),
        kind,
        mainline.as_deref(),
    )
}

/// Just the volume, for the footer pill. Cheap enough to poll; deliberately
/// does not walk a single directory.
#[tauri::command]
pub fn housekeeper_disk() -> Option<Volume> {
    volume_usage(&dirs::home_dir()?)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {

    /// The bug this rule exists for: a fresh dispatch worktree has no commits
    /// (so it is contained in the mainline) and nothing uncommitted, and the
    /// automatic reclaim deleted one thirty-one seconds after its agent was
    /// launched into it (2026-09-06, XNAUT-300). The writer lease is the only
    /// store that knew a run was live there.
    #[test]
    fn a_worktree_an_agent_is_working_in_is_never_offered() {
        let mut f = safe();
        assert!(
            verdict_for_worktree(&f, Some("main")).offered(),
            "clean, merged and ours: offered when nobody is working"
        );
        f.writer = Some("claude".into());
        let v = verdict_for_worktree(&f, Some("main"));
        assert!(!v.offered(), "an agent is building here");
        assert!(v.reason().contains("@claude"), "the refusal names who: {}", v.reason());
        // And its build cache is not fair game either, for the same reason.
        let cache = verdict_for_cache(&f, 99.0);
        assert!(!cache.offered(), "the cache beside a live run stays: {}", cache.reason());
    }

    /// The dispatch path's worktrees and any `agent/` branch are xNAUT's to
    /// remove; a checkout a person made under .worktrees/ is not.
    #[test]
    fn dispatch_worktrees_count_as_agent_worktrees() {
        assert!(super::is_agent_worktree("/r/.claude/worktrees/agent-abc", None));
        assert!(super::is_agent_worktree("/r/.worktrees/safety-net-worktrees/agent-claude-xnaut-206", None));
        assert!(super::is_agent_worktree("/r/.worktrees/anything", Some("agent/claude/xnaut-44")));
        assert!(!super::is_agent_worktree("/r/.worktrees/xnaut-276-verify-the-ticket", Some("fix/xnaut-276-verify-the-ticket")));
        assert!(!super::is_agent_worktree("/r", Some("main")));
    }

    #[test]
    fn automatic_reclaim_runs_at_most_once_an_hour() {
        let t0 = 1_700_000_000_000i64;
        // A fresh process may already have been asked in this test binary;
        // step far enough ahead that the first call is due either way.
        assert!(super::auto_reclaim_due(t0 + 10 * super::AUTO_RECLAIM_EVERY_MS));
        assert!(!super::auto_reclaim_due(t0 + 10 * super::AUTO_RECLAIM_EVERY_MS + 1000));
        assert!(super::auto_reclaim_due(t0 + 11 * super::AUTO_RECLAIM_EVERY_MS + 1));
    }
    use super::*;

    /// A worktree with nothing unique in it: clean, merged, agent-made, free.
    fn safe() -> Facts {
        Facts {
            is_repo_checkout: false,
            is_agent_worktree: true,
            missing: false,
            dirty: Some(0),
            in_mainline: Some(true),
            lock: Lock::None,
            writer: None,
        }
    }

    #[test]
    fn an_unsafe_worktree_is_never_offered_for_removal() {
        // THE load-bearing test. Every way a checkout can hold the only copy of
        // something, each on its own, against a worktree that is otherwise
        // perfectly removable. If any of these ever returns Offer, the app
        // deletes work that no ref can bring back.
        let cases: Vec<(&str, Facts)> = vec![
            (
                "one uncommitted file",
                Facts {
                    dirty: Some(1),
                    ..safe()
                },
            ),
            (
                "many uncommitted files",
                Facts {
                    dirty: Some(37),
                    ..safe()
                },
            ),
            (
                "git status unreadable",
                Facts {
                    dirty: None,
                    ..safe()
                },
            ),
            (
                "HEAD not in the mainline",
                Facts {
                    in_mainline: Some(false),
                    ..safe()
                },
            ),
            (
                "mainline membership unknown",
                Facts {
                    in_mainline: None,
                    ..safe()
                },
            ),
            (
                "an agent is still holding it",
                Facts {
                    lock: Lock::Held("pid 1".into()),
                    ..safe()
                },
            ),
            (
                "the repo's own checkout",
                Facts {
                    is_repo_checkout: true,
                    ..safe()
                },
            ),
            (
                "a worktree a person made",
                Facts {
                    is_agent_worktree: false,
                    ..safe()
                },
            ),
            (
                "the directory is gone",
                Facts {
                    missing: true,
                    ..safe()
                },
            ),
        ];
        for (name, facts) in cases {
            let verdict = verdict_for_worktree(&facts, Some("main"));
            assert!(
                !verdict.offered(),
                "{name}: offered a worktree that must never be offered ({verdict:?})",
            );
            assert!(
                !verdict.reason().is_empty(),
                "{name}: refused without saying why"
            );
        }
        // And the control: without any of those, it IS offered. Without this
        // line the test above passes on a function that only ever says no.
        assert!(verdict_for_worktree(&safe(), Some("main")).offered());
    }

    #[test]
    fn a_refusal_names_the_thing_it_refused_over() {
        // "Kept: 3 uncommitted files" is useful; "kept" is the silent skip this
        // project keeps paying for.
        let three = verdict_for_worktree(
            &Facts {
                dirty: Some(3),
                ..safe()
            },
            Some("main"),
        );
        assert_eq!(three.reason(), "3 uncommitted files");
        let one = verdict_for_worktree(
            &Facts {
                dirty: Some(1),
                ..safe()
            },
            Some("main"),
        );
        assert_eq!(one.reason(), "1 uncommitted file");
        // The mainline is named, because "not in the mainline" without saying
        // which mainline is not an answer.
        let unmerged = verdict_for_worktree(
            &Facts {
                in_mainline: Some(false),
                ..safe()
            },
            Some("feature/loops-platform"),
        );
        assert_eq!(unmerged.reason(), "HEAD is not in feature/loops-platform");
        // And a live lock repeats what git said, so the owner can find the pid.
        let held = verdict_for_worktree(
            &Facts {
                lock: Lock::Held("claude agent (pid 38368 ...)".into()),
                ..safe()
            },
            Some("main"),
        );
        assert!(held.reason().contains("38368"), "{held:?}");
    }

    #[test]
    fn an_unresolvable_mainline_offers_nothing() {
        // The fail-safe direction at the level of the whole run: if the ref
        // that decides "already merged" is not there, no worktree is merged.
        let verdict = verdict_for_worktree(&safe(), None);
        assert!(!verdict.offered());
        assert_eq!(
            verdict.reason(),
            "no mainline ref resolved, so nothing counts as merged"
        );
    }

    #[test]
    fn a_build_cache_beside_uncommitted_work_is_never_cleared() {
        // The bug the prototype's first dry run produced: one worktree listed
        // as BOTH clearable and kept, 27 GB of cache beside an uncommitted
        // file. Clearing a target directory next to unsaved work is probably
        // harmless and definitely contradicts what the report says it is doing.
        let dirty = Facts {
            dirty: Some(1),
            ..safe()
        };
        let verdict = verdict_for_cache(&dirty, 40.0);
        assert!(!verdict.offered(), "{verdict:?}");
        assert_eq!(verdict.reason(), "1 uncommitted file beside it");
        // Unreadable status keeps it too.
        assert!(!verdict_for_cache(
            &Facts {
                dirty: None,
                ..safe()
            },
            40.0
        )
        .offered());
        // A cache somebody is still building in is not litter.
        let fresh = verdict_for_cache(&safe(), 0.5);
        assert!(!fresh.offered(), "{fresh:?}");
        assert!(fresh.reason().contains("12h"), "{fresh:?}");
        // Control: idle, clean, unlocked, and it is offered.
        assert!(verdict_for_cache(&safe(), 40.0).offered());
    }

    #[test]
    fn a_dead_agents_lock_is_stale_and_a_live_one_holds() {
        // Claude Code writes its pid into the worktree lock and never lifts it,
        // so every finished agent worktree stays locked forever. Treating that
        // as "an agent is working here" would make this module refuse the exact
        // 136 GB it exists to find.
        let mut child = std::process::Command::new("true").spawn().expect("spawn");
        let dead = child.id();
        child.wait().expect("reap");

        let alive = std::process::id();
        let live_reason =
            format!("claude agent agent-x (pid {alive} start Mon Aug 31 09:20:51 2026)");
        let dead_reason =
            format!("claude agent agent-y (pid {dead} start Mon Aug 31 09:20:51 2026)");

        assert!(matches!(
            classify_lock(Some(&live_reason), true),
            Lock::Held(_)
        ));
        assert!(matches!(
            classify_lock(Some(&dead_reason), true),
            Lock::Stale(_)
        ));
        // An unlocked worktree is not locked, and a lock naming nobody holds.
        assert_eq!(classify_lock(Some(&dead_reason), false), Lock::None);
        assert!(matches!(
            classify_lock(Some("held by hand"), true),
            Lock::Held(_)
        ));
        assert!(matches!(classify_lock(None, true), Lock::Held(_)));
    }

    #[test]
    fn the_pid_comes_out_of_the_reason_git_actually_writes() {
        assert_eq!(
            pid_in(
                "claude agent agent-a09f62c10c919a32e (pid 38368 start Mon Aug 31 09:20:51 2026)"
            ),
            Some(38368),
        );
        assert_eq!(pid_in("locked by hand while I think"), None);
        assert_eq!(pid_in("pid notanumber"), None);
    }

    #[test]
    fn the_percentage_is_what_cannot_be_written() {
        // Measured against this machine's own numbers on 2026-09-04: 242837545
        // blocks with 43623462 available is 82.04% unavailable, and the app
        // showed 82 while `df` showed 81. The one-point gap is APFS purgeable
        // space that only ATTR_VOL_SPACEUSED accounts for; see used_percent.
        assert_eq!(used_percent(242837545, 43623462), 82);
        assert_eq!(used_percent(100, 5), 95);
        assert_eq!(used_percent(100, 100), 0, "an empty disk is not a full one");
        assert_eq!(
            used_percent(0, 0),
            0,
            "an unreadable answer is not a full disk"
        );
        // Nearest, not up. Rounding up turned the measured 82.04 above into 83
        // and made the app disagree with `df` by two points, one of them pure
        // arithmetic. 90.1% is 90.
        assert_eq!(used_percent(1000, 99), 90);
        assert_eq!(used_percent(1000, 94), 91, "90.6% is 91");
    }

    #[test]
    fn the_bands_are_where_the_measurements_landed() {
        assert_eq!(band(79), Band::Ok);
        assert_eq!(band(80), Band::Warn);
        assert_eq!(band(94), Band::High, "2026-09-01 was 94%");
        assert_eq!(band(98), Band::Critical, "2026-09-03 was 98%");
    }

    #[test]
    fn a_warning_repeats_on_a_rise_but_not_every_minute() {
        // The tick runs every 60 seconds. A ledger line every minute is a log
        // file with extra steps, and it buries the automation rows beside it.
        let hour = 3_600_000;
        assert!(
            should_announce(Band::High, None, 0),
            "the first crossing always speaks"
        );
        assert!(
            !should_announce(Band::Ok, None, 0),
            "a healthy disk says nothing"
        );
        assert!(
            !should_announce(Band::High, Some((Band::High, 0)), 60_000),
            "a minute later, same band: silence",
        );
        assert!(
            should_announce(Band::Critical, Some((Band::High, 0)), 60_000),
            "getting worse speaks immediately",
        );
        assert!(
            !should_announce(Band::Warn, Some((Band::High, 0)), 60_000),
            "getting better is not news",
        );
        assert!(
            should_announce(Band::High, Some((Band::High, 0)), 6 * hour),
            "still bad six hours later is worth repeating",
        );
    }

    #[test]
    fn a_size_is_measured_not_guessed() {
        let dir = std::env::temp_dir().join(format!("xnaut-hk-size-{}", std::process::id()));
        let nested = dir.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.join("top"), vec![0u8; 1000]).unwrap();
        std::fs::write(nested.join("deep"), vec![0u8; 2000]).unwrap();
        assert_eq!(dir_bytes(&dir, &[]), 3000, "every level counts");
        // A directory that belongs to another row is not this row's bytes.
        assert_eq!(
            dir_bytes(&dir, &[dir.join("a")]),
            1000,
            "the skipped subtree is not counted"
        );
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            dir_bytes(&dir, &[]),
            0,
            "a directory that is not there is not a size"
        );
    }

    #[test]
    fn a_nested_worktree_is_not_counted_twice() {
        // Found by running the report against André's real checkout on
        // 2026-09-04: `.worktrees/*` and `.claude/worktrees/agent-*` live INSIDE
        // the repository's own working tree, so the walk descended into every
        // one of them and billed their gigabytes to the parent as well as to
        // themselves. The repo row read 84.3 GB and the report's total read
        // 202 GB on a volume that did not hold that much. Every row's size has
        // to be exactly what removing that row reclaims, or the totals are
        // fiction.
        let repo = scratch_repo("nested");
        std::fs::write(repo.join("bulk"), vec![0u8; 4000]).unwrap();
        let inner = repo.join(".worktrees").join("child");
        std::fs::create_dir_all(repo.join(".worktrees")).unwrap();
        assert!(git_in(
            &repo,
            &[
                "worktree",
                "add",
                "--no-track",
                "-b",
                "child",
                &inner.to_string_lossy(),
                "main"
            ],
        )
        .status
        .success());
        // Big enough that the parent's own metadata cannot be mistaken for it.
        const PAYLOAD: usize = 5_000_000;
        std::fs::write(inner.join("payload"), vec![0u8; PAYLOAD]).unwrap();

        let report = scan(&repo, Some("main")).unwrap();
        let row = |p: &Path| {
            report
                .items
                .iter()
                .find(|i| i.kind == Kind::Worktree && same_path(Path::new(&i.path), p))
                .unwrap_or_else(|| panic!("no row for {}", p.display()))
        };
        assert!(
            row(&inner).bytes >= PAYLOAD as u64,
            "the child owns its own payload: {:?}",
            row(&inner)
        );
        assert!(
            row(&repo).bytes < PAYLOAD as u64,
            "the parent was billed for the child's payload too: parent {}",
            row(&repo).bytes,
        );

        std::fs::remove_dir_all(&repo).unwrap();
    }

    #[test]
    fn a_kept_worktree_and_its_cache_do_not_report_the_same_bytes() {
        // The other half of the same arithmetic. A kept worktree gets a cache
        // row of its own, and the two rows are summed into `kept_bytes`, so the
        // worktree row must exclude the cache it just handed to that row.
        let repo = scratch_repo("cachebytes");
        const BLOB: usize = 5_000_000;
        let cache = repo.join("src-tauri").join("target");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("blob"), vec![0u8; BLOB]).unwrap();
        std::fs::write(repo.join("source.rs"), vec![0u8; 500]).unwrap();

        let report = scan(&repo, Some("main")).unwrap();
        let wt = report
            .items
            .iter()
            .find(|i| i.kind == Kind::Worktree)
            .unwrap();
        let cache_row = report
            .items
            .iter()
            .find(|i| i.kind == Kind::BuildCache)
            .unwrap();
        assert!(cache_row.bytes >= BLOB as u64, "{cache_row:?}");
        assert!(
            wt.bytes < BLOB as u64,
            "the checkout row still counted the cache: {wt:?}"
        );
        assert_eq!(
            report.kept_bytes,
            report
                .items
                .iter()
                .filter(|i| !i.offered)
                .map(|i| i.bytes)
                .sum::<u64>(),
        );

        std::fs::remove_dir_all(&repo).unwrap();
    }

    #[test]
    fn the_mainline_is_found_on_a_remote_when_there_is_no_local_main() {
        // Measured on André's own checkout, 2026-09-04: it has NO local `main`.
        // Every local branch is feature work and the mainline lives only as
        // `origin/main` and `forgejo/main`. Looking for local names alone
        // returned None, so all 28 worktrees were kept with "no mainline ref
        // resolved" - safe, and completely inert. A housekeeper that refuses
        // everything has not been built, it has been disabled.
        let repo = scratch_repo("remote-main");
        // A bare "remote" to fetch from, then drop the local main entirely.
        let remote = scratch_repo("remote-origin");
        assert!(git_in(
            &repo,
            &["remote", "add", "origin", &remote.to_string_lossy()]
        )
        .status
        .success());
        assert!(git_in(&repo, &["fetch", "--quiet", "origin"])
            .status
            .success());
        assert!(
            git_in(&repo, &["checkout", "--quiet", "-b", "feature/only"])
                .status
                .success()
        );
        assert!(git_in(&repo, &["branch", "-D", "main"]).status.success());
        assert!(
            !git_in(
                &repo,
                &["rev-parse", "--verify", "--quiet", "main^{commit}"]
            )
            .status
            .success(),
            "the local main must really be gone for this test to mean anything",
        );

        assert_eq!(
            resolve_mainline(&repo, None).as_deref(),
            Some("origin/main")
        );
        // And an explicit request still wins over the search.
        assert_eq!(
            resolve_mainline(&repo, Some("feature/only")).as_deref(),
            Some("feature/only")
        );

        std::fs::remove_dir_all(&repo).unwrap();
        std::fs::remove_dir_all(&remote).unwrap();
    }

    /// A throwaway repo with one commit on `main`. Everything below lives
    /// inside `std::env::temp_dir()` and is removed at the end of its test;
    /// nothing here ever touches a real checkout.
    fn scratch_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("xnaut-hk-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&root)
                .args([
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "user.name=t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(
                ok.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&ok.stderr)
            );
        };
        git(&["init", "-b", "main"]);
        std::fs::write(root.join("README"), "hi").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-m", "one"]);
        root
    }

    fn git_in(repo: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap()
    }

    #[test]
    fn against_real_git_the_branch_outlives_its_worktree_and_dirty_work_is_refused() {
        // The end-to-end version of the load-bearing rule, run through actual
        // git rather than injected facts. Two agent worktrees on the same
        // merged commit: one clean, one with an uncommitted file.
        let repo = scratch_repo("e2e");
        let agents = repo.join(".claude").join("worktrees");
        std::fs::create_dir_all(&agents).unwrap();
        let clean = agents.join("agent-clean");
        let dirty = agents.join("agent-dirty");
        for (path, branch) in [
            (&clean, "worktree-agent-clean"),
            (&dirty, "worktree-agent-dirty"),
        ] {
            let out = git_in(
                &repo,
                &[
                    "worktree",
                    "add",
                    "--no-track",
                    "-b",
                    branch,
                    &path.to_string_lossy(),
                    "main",
                ],
            );
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        std::fs::write(dirty.join("unsaved.txt"), "the only copy of something").unwrap();

        let report = scan(&repo, Some("main")).unwrap();
        let row = |p: &Path| {
            report
                .items
                .iter()
                .find(|i| i.kind == Kind::Worktree && same_path(Path::new(&i.path), p))
                .unwrap_or_else(|| panic!("no row for {}; every worktree gets one", p.display()))
        };
        assert_eq!(report.mainline.as_deref(), Some("main"));
        // The repo's own checkout is listed and kept, never silently dropped.
        assert!(!row(&repo).offered);
        assert!(row(&clean).offered, "{:?}", row(&clean));
        assert!(!row(&dirty).offered, "{:?}", row(&dirty));
        assert_eq!(row(&dirty).reason, "1 uncommitted file");

        // Refusing is an error, and the file is still there afterwards.
        let refused = reclaim(&repo, &dirty, Kind::Worktree, Some("main")).unwrap_err();
        assert_eq!(refused, "refused: 1 uncommitted file");
        assert!(
            dirty.join("unsaved.txt").exists(),
            "refusing must not delete"
        );

        // And the offered one goes, WITHOUT taking its branch. This is what
        // makes any of it safe: the worktree is scaffolding, the branch is the
        // deliverable.
        reclaim(&repo, &clean, Kind::Worktree, Some("main")).unwrap();
        assert!(!clean.exists(), "the directory is gone");
        assert!(
            git_in(&repo, &["rev-parse", "--verify", "worktree-agent-clean"])
                .status
                .success(),
            "the branch must outlive the worktree",
        );

        std::fs::remove_dir_all(&repo).unwrap();
    }

    #[test]
    fn a_merge_that_only_exists_on_the_remote_still_counts_as_merged() {
        // NautBot merges and PUSHES; the local branch of the same name never
        // moves. Before this, a worktree whose ticket had integrated minutes
        // earlier was refused as "HEAD is not in dev" on every pass, and kept
        // its multi-gigabyte build cache indefinitely. Three of those took
        // tron to 100% full on 2026-09-09.
        let repo = scratch_repo("pushed-merge");
        let commit = |dir: &Path, msg: &str| {
            assert!(git_in(dir, &["add", "-A"]).status.success());
            assert!(git_in(
                dir,
                &[
                    "-c", "user.email=t@t", "-c", "user.name=t",
                    "-c", "commit.gpgsign=false", "commit", "-m", msg,
                ],
            )
            .status
            .success());
        };

        // A `dev` that both sides start from.
        assert!(git_in(&repo, &["branch", "dev", "main"]).status.success());

        // The agent's worktree, one commit ahead of dev.
        let agents = repo.join(".claude").join("worktrees");
        std::fs::create_dir_all(&agents).unwrap();
        let wt = agents.join("agent-pushed");
        assert!(git_in(
            &repo,
            &[
                "worktree", "add", "--no-track", "-b", "worktree-agent-pushed",
                &wt.to_string_lossy(), "dev",
            ],
        )
        .status
        .success());
        std::fs::write(wt.join("shipped.txt"), "the agent's work").unwrap();
        commit(&wt, "the ticket");
        let head = String::from_utf8_lossy(&git_in(&wt, &["rev-parse", "HEAD"]).stdout)
            .trim()
            .to_string();

        // A bare remote that HAS the merge, reached only as origin/dev. Local
        // `dev` is deliberately left where it was.
        let bare = repo.parent().unwrap().join("xnaut-hk-pushed-merge-origin.git");
        let _ = std::fs::remove_dir_all(&bare);
        assert!(Command::new("git")
            .args(["init", "--bare", "-b", "dev"])
            .arg(&bare)
            .output()
            .unwrap()
            .status
            .success());
        assert!(git_in(&repo, &["remote", "add", "origin", &bare.to_string_lossy()])
            .status
            .success());
        assert!(git_in(&wt, &["push", "origin", "HEAD:dev"]).status.success());
        assert!(git_in(&repo, &["fetch", "--quiet", "origin"]).status.success());

        // Precondition: local dev does NOT contain it, origin/dev does.
        assert_eq!(is_ancestor(&repo, &head, "dev"), Some(false));
        assert_eq!(is_ancestor(&repo, &head, "origin/dev"), Some(true));

        // The rule under test, and then the whole report through it.
        assert_eq!(merged_into(&repo, &head, "dev"), Some(true));
        let report = scan(&repo, Some("dev")).unwrap();
        let row = report
            .items
            .iter()
            .find(|i| i.kind == Kind::Worktree && same_path(Path::new(&i.path), &wt))
            .expect("every worktree gets a row");
        assert!(row.offered, "the merge exists, just not locally: {row:?}");

        // A commit on neither ref is still unique work, and a repository with
        // no remote at all is unchanged.
        std::fs::write(wt.join("later.txt"), "after the push").unwrap();
        commit(&wt, "not pushed");
        let later = String::from_utf8_lossy(&git_in(&wt, &["rev-parse", "HEAD"]).stdout)
            .trim()
            .to_string();
        assert_eq!(merged_into(&repo, &later, "dev"), Some(false));
        assert_eq!(merged_into(&repo, &head, "no-such-branch"), None);

        let _ = std::fs::remove_dir_all(&bare);
        std::fs::remove_dir_all(&repo).unwrap();
    }

    #[test]
    fn an_unmerged_branch_survives_a_full_scan_and_a_direct_apply() {
        // Rule 2 through real git: a commit the mainline does not contain is
        // work that exists nowhere else, whatever the directory looks like.
        let repo = scratch_repo("unmerged");
        let agents = repo.join(".claude").join("worktrees");
        std::fs::create_dir_all(&agents).unwrap();
        let wt = agents.join("agent-ahead");
        assert!(git_in(
            &repo,
            &[
                "worktree",
                "add",
                "--no-track",
                "-b",
                "worktree-agent-ahead",
                &wt.to_string_lossy(),
                "main"
            ],
        )
        .status
        .success());
        std::fs::write(wt.join("new.txt"), "committed here and nowhere else").unwrap();
        assert!(git_in(&wt, &["add", "-A"]).status.success());
        assert!(git_in(
            &wt,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "ahead"
            ],
        )
        .status
        .success());

        let report = scan(&repo, Some("main")).unwrap();
        let row = report
            .items
            .iter()
            .find(|i| i.kind == Kind::Worktree && same_path(Path::new(&i.path), &wt))
            .expect("every worktree gets a row");
        assert!(
            !row.offered,
            "a clean worktree ahead of main is still unique work: {row:?}"
        );
        assert_eq!(row.reason, "HEAD is not in main");
        assert_eq!(
            reclaim(&repo, &wt, Kind::Worktree, Some("main")).unwrap_err(),
            "refused: HEAD is not in main"
        );
        assert!(wt.join("new.txt").exists());

        std::fs::remove_dir_all(&repo).unwrap();
    }

    #[test]
    fn reclaim_refuses_a_path_the_report_never_offered() {
        // The apply path recomputes the verdict instead of trusting the caller,
        // so a button press cannot outlive the reason it was drawn. Here the
        // path is a real directory inside a real repository that is simply not
        // one of the report's rows: the refusal has to come out as an error
        // naming it, never as a deletion.
        let repo = scratch_repo("notarow");
        let ordinary = repo.join("src");
        std::fs::create_dir_all(&ordinary).unwrap();
        std::fs::write(ordinary.join("keep.rs"), "the only copy").unwrap();

        let err = reclaim(&repo, &ordinary, Kind::Worktree, Some("main")).unwrap_err();
        assert!(
            err.contains("is not in the report"),
            "a refusal says which one it is: {err}"
        );
        assert!(
            ordinary.join("keep.rs").exists(),
            "refusing must not delete"
        );
        // The same for a build cache that is not a cache.
        let err = reclaim(&repo, &ordinary, Kind::BuildCache, Some("main")).unwrap_err();
        assert!(err.contains("is not in the report"), "{err}");
        assert!(ordinary.join("keep.rs").exists());

        std::fs::remove_dir_all(&repo).unwrap();
    }
}
