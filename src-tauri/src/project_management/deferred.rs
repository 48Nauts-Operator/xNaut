//! Local evidence survives a paused/busy control repo without adding Git objects.
//! Replay is revision checked; conflicts stay visible instead of overwriting an
//! owner's ticket. Files live beside the common Git lease, never in ticket data.
use super::*;
use sha2::{Digest, Sha256};
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Pending {
    Document {
        event: String,
        subject: String,
        details: Value,
    },
    Jury {
        job: Box<crate::jury::Job>,
        status: Option<String>,
        expected_revision: u64,
    },
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    branch: String,
    at: String,
    reason: String,
    pending: Pending,
}
thread_local! { static REPLAY: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) }; }
pub(super) fn replay_id() -> Option<String> {
    REPLAY.with(|v| v.borrow().clone())
}
fn state_dir(repo: &Path) -> Result<PathBuf, String> {
    Ok(common_dir(repo)?.join("xnaut-pm-state"))
}
pub(super) fn retain(repo: &Path, pending: Pending, reason: &str) -> Result<(), String> {
    let branch = run_git(repo, &["symbolic-ref", "--short", "HEAD"])?;
    let bytes = serde_json::to_vec(&(&branch, &pending)).map_err(|e| e.to_string())?;
    let id = format!("{:x}", Sha256::digest(&bytes));
    let dir = state_dir(repo)?.join("deferred");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("cannot retain deferred PM evidence: {e}"))?;
    let path = dir.join(format!("{id}.json"));
    if path.exists() {
        // Do not erase malformed or different evidence under a familiar name.
        let saved: Envelope = read_json(&path)?;
        if serde_json::to_vec(&(&saved.branch, &saved.pending)).map_err(|e| e.to_string())? != bytes
        {
            return Err("deferred PM evidence identity conflict; original retained".into());
        }
        return Ok(());
    }
    write_json_atomic(
        &path,
        &Envelope {
            branch,
            at: chrono::Utc::now().to_rfc3339(),
            reason: reason.into(),
            pending,
        },
    )
}
/// Bounded replay once per normal sweep. Corrupt/conflicting records stay on
/// disk, appear in status, and do not starve unrelated deferred documents.
pub(super) fn replay(repo: &Path) -> Result<(), String> {
    let _lease = ControlWriteLease::acquire(repo)?;
    let branch = run_git(repo, &["symbolic-ref", "--short", "HEAD"])?;
    let dir = state_dir(repo)?.join("deferred");
    let files = match std::fs::read_dir(&dir) {
        Ok(files) => files,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("cannot inspect deferred PM evidence: {e}")),
    };
    let mut files: Vec<_> = files
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|v| v == "json"))
        .collect();
    files.sort_by_key(|p| (p.metadata().and_then(|m| m.modified()).ok(), p.clone()));
    let mut attempted = 0;
    for path in files {
        if attempted >= 32 {
            break;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|v| v.to_str())
            .filter(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
        else {
            continue;
        };
        let Ok(mut envelope) = read_json::<Envelope>(&path) else {
            continue;
        };
        if envelope.branch != branch {
            continue;
        }
        let bytes = serde_json::to_vec(&(&envelope.branch, &envelope.pending))
            .map_err(|e| e.to_string())?;
        if format!("{:x}", Sha256::digest(&bytes)) != id {
            envelope.reason =
                "Deferred evidence filename/content mismatch; original retained for inspection"
                    .into();
            write_json_atomic(&path, &envelope)?;
            continue;
        }
        attempted += 1;
        let committed = match committed_matches(repo, id, &envelope.pending) {
            Ok(value) => value,
            Err(reason) => {
                envelope.reason = format!("Deferred committed evidence could not be verified; original retained: {reason}");
                write_json_atomic(&path, &envelope)?;
                continue;
            }
        };
        match committed {
            Some(true) => {
                std::fs::remove_file(&path)
                    .map_err(|e| format!("cannot acknowledge deferred PM evidence: {e}"))?;
                continue;
            }
            Some(false) => {
                envelope.reason = "Committed deferred event does not match retained evidence; reconciliation required, original retained".into();
                write_json_atomic(&path, &envelope)?;
                continue;
            }
            None => {}
        }
        let result = replay_one(repo, id, &envelope.pending);
        match result {
            Ok(()) => {
                std::fs::remove_file(&path)
                    .map_err(|e| format!("cannot acknowledge deferred PM evidence: {e}"))?;
            }
            Err(reason) => {
                envelope.reason = reason;
                write_json_atomic(&path, &envelope)?;
            }
        }
    }
    Ok(())
}
fn committed_matches(repo: &Path, id: &str, pending: &Pending) -> Result<Option<bool>, String> {
    let path = format!("events/deferred-{id}.json");
    if run_git(repo, &["cat-file", "-e", &format!("HEAD:{path}")]).is_err() {
        return Ok(None);
    }
    let raw = run_git(repo, &["show", &format!("HEAD:{path}")])?;
    let Ok(event) = serde_json::from_str::<Value>(&raw) else {
        return Ok(Some(false));
    };
    if event["deferred_pending_sha256"] != id {
        return Ok(Some(false));
    }
    let matches = match pending {
        Pending::Document {
            event: kind,
            subject,
            details,
        } => {
            event["event"] == *kind && event["subject"] == *subject && event["details"] == *details
        }
        Pending::Jury {
            job,
            status,
            expected_revision,
        } => {
            let revision = expected_revision.saturating_add(1);
            if event["event"] != "ticket.jury"
                || event["subject"] != job.ticket
                || event["details"]["jury_id"] != job.id
                || event["details"]["state"] != job.state
                || event["details"]["decision"]
                    != serde_json::to_value(job.decision).map_err(|e| e.to_string())?
                || event["details"]["revision"] != revision
            {
                return Ok(Some(false));
            }
            // Bind to the actual ticket tree committed with that event, not a
            // later owner's edit or merely a string containing a job ID.
            let commit = run_git(repo, &["log", "-1", "--format=%H", "--", &path])?;
            let ticket_path = find_ticket_path(repo, &job.ticket)?;
            let relative = ticket_path
                .strip_prefix(repo)
                .map_err(|_| "deferred ticket path escaped repository")?
                .to_string_lossy();
            let raw = run_git(repo, &["show", &format!("{commit}:{relative}")])?;
            let Ok(ticket) = serde_json::from_str::<TicketRecord>(&raw) else {
                return Ok(Some(false));
            };
            ticket.id == job.ticket
                && ticket.project == job.project
                && ticket.revision == revision
                && status.as_ref().is_none_or(|s| ticket.status == *s)
                && ticket.approval.jury_reviews.iter().any(|saved| {
                    serde_json::to_value(saved).ok() == serde_json::to_value(job.as_ref()).ok()
                })
        }
    };
    Ok(Some(matches))
}

fn replay_one(repo: &Path, id: &str, pending: &Pending) -> Result<(), String> {
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            REPLAY.with(|v| *v.borrow_mut() = None);
        }
    }
    REPLAY.with(|v| *v.borrow_mut() = Some(id.into()));
    let _clear = Clear;
    match pending {
        Pending::Document {
            event,
            subject,
            details,
        } => record_mutation(
            repo,
            event,
            subject,
            details.clone(),
            &[],
            &format!("feat(pm): {event} {subject}"),
        ),
        Pending::Jury {
            job,
            status,
            expected_revision,
        } => {
            let ticket: TicketRecord = read_json(&find_ticket_path(repo, &job.ticket)?)?;
            if ticket.revision != *expected_revision {
                return Err("Deferred jury receipt needs reconciliation: ticket changed while paused; original receipt retained".into());
            }
            // The caller holds the cross-process lease. attach_jury takes the
            // thread mutex itself; do not acquire that mutex around replay.
            attach_jury_in(repo, job, status.as_deref()).map(|_| ())
        }
    }
}
#[derive(Serialize, Deserialize)]
struct PushFault {
    first_at: String,
    last_at: String,
    branch: String,
    head: String,
    attempts: u64,
    reason: String,
}
pub(super) fn push_result(
    repo: &Path,
    branch: &str,
    result: &Result<String, String>,
) -> Result<(), String> {
    let dir = state_dir(repo)?;
    let path = dir.join("push-deferred.json");
    let mut faults: std::collections::BTreeMap<String, PushFault> = if path.exists() {
        read_json(&path)?
    } else {
        Default::default()
    };
    if result.is_ok() {
        // A linked worktree pushing branch B cannot acknowledge branch A's
        // retained failure. All worktrees share this evidence directory.
        faults.remove(branch);
        if faults.is_empty() {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("cannot clear push-deferred fault: {e}")),
            }
            return Ok(());
        }
        return write_json_atomic(&path, &faults);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let previous = faults.remove(branch);
    let now = chrono::Utc::now().to_rfc3339();
    faults.insert(branch.into(), PushFault {
        first_at: previous.as_ref().map(|p| p.first_at.clone()).unwrap_or_else(|| now.clone()),
        last_at: now, branch: branch.into(), head: run_git(repo, &["rev-parse", "HEAD"])? ,
        attempts: previous.map(|p| p.attempts.saturating_add(1)).unwrap_or(1),
        reason: "Control repository push failed; local commits retained. Synchronize or repair remote access; no reset or prune was attempted.".into(),
    });
    write_json_atomic(&path, &faults)
}
pub(super) fn warning(repo: &Path) -> Result<String, String> {
    let dir = state_dir(repo)?;
    let mut messages = Vec::new();
    let push = dir.join("push-deferred.json");
    if push.exists() {
        let faults: std::collections::BTreeMap<String, PushFault> = read_json(&push)?;
        for fault in faults.values() {
            messages.push(format!(
                "Push deferred since {} ({} attempts, branch {}): {}",
                fault.first_at, fault.attempts, fault.branch, fault.reason
            ));
        }
    }
    match std::fs::read_dir(dir.join("deferred")) {
        Ok(files) => {
            let mut count = 0;
            let mut reasons = std::collections::BTreeSet::new();
            for entry in files {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.extension().is_some_and(|v| v == "json") {
                    count += 1;
                    let reason = match read_json::<Envelope>(&path) {
                        Ok(record) => crate::project_wiki::redact(&record.reason),
                        Err(_) => {
                            "Retained PM evidence is unreadable; file preserved for inspection"
                                .into()
                        }
                    };
                    if reasons.len() < 3 {
                        reasons.insert(reason);
                    }
                }
            }
            if count > 0 {
                messages.push(format!("{count} deferred PM receipt(s) retained locally; paused, busy or conflicting writes have not been discarded. Automatic replay requires read_only off and unchanged ticket revisions. {}", reasons.into_iter().collect::<Vec<_>>().join("; ")));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot inspect retained PM evidence: {e}")),
    }
    Ok(messages.join(" "))
}
