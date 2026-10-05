//! Read-only project continuity projection (XNAUT-460).
//! Reuses xNAUT's (MIT) run_control journal reader and repository_review receipts.
//! This is not another lifecycle controller: no probes, writes, dispatch or merge
//! authorization. `verified` means recorded independent evidence for the named
//! revision, never that a process exited, a ticket says done, or today's remote
//! branch still points at that revision. Missing sources remain visible.

use crate::project_management::TicketRecord;
use crate::repository_transfer::Transfer;
use crate::run_control::{self, RunManifest, RunState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContinuityState {
    Active,
    Stalled,
    Blocked,
    Review,
    Verified,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Evidence {
    pub kind: String,
    pub source: String,
    pub detail: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Diagnostic {
    #[serde(default = "default_blocking")]
    pub blocking: bool,
    pub source: String,
    pub message: String,
}
fn default_blocking() -> bool {
    true
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AssignmentSnapshot {
    pub run_id: String,
    pub ticket: Option<String>,
    pub owner: String,
    pub branch: String,
    pub worktree: String,
    pub previous_run_id: Option<String>,
    pub next_run_id: Option<String>,
    pub run_state: Option<RunState>,
    pub last_commit: String,
    pub pr_url: Option<String>,
    pub review_state: Option<String>,
    pub state: ContinuityState,
    pub evidence: Vec<Evidence>,
    pub next_action: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TicketSnapshot {
    pub id: String,
    pub title: String,
    pub status: String,
    pub owner: Option<String>,
    pub state: ContinuityState,
    pub assignment_ids: Vec<String>,
    pub evidence: Vec<Evidence>,
    pub next_action: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub project: String,
    pub observed_at: i64,
    pub tickets: Vec<TicketSnapshot>,
    pub assignments: Vec<AssignmentSnapshot>,
    pub diagnostics: Vec<Diagnostic>,
}

fn evidence(kind: &str, source: impl Into<String>, detail: impl Into<String>) -> Evidence {
    Evidence {
        kind: kind.into(),
        source: source.into(),
        detail: crate::project_wiki::redact(&detail.into()),
    }
}
fn action(state: ContinuityState) -> String {
    match state {
        ContinuityState::Active => "Inspect the existing worker and recorded progress before assigning more work.",
        ContinuityState::Stalled => "Inspect the existing worker, branch and artifacts; recover its work before considering a replacement.",
        ContinuityState::Blocked => "Resolve the recorded blocker and inspect preserved implementation before resuming.",
        ContinuityState::Review => "Inspect the existing branch, handback and PR; obtain independent verification for its current revision.",
        ContinuityState::Verified => "Use the cited verified revision; confirm current refs and authorization before integration.",
        ContinuityState::Unknown => "Recover missing run or verification evidence and inspect existing work before dispatching.",
    }.into()
}
fn empty_assignment(id: &str) -> AssignmentSnapshot {
    AssignmentSnapshot {
        run_id: id.into(),
        ticket: None,
        owner: String::new(),
        branch: String::new(),
        worktree: String::new(),
        previous_run_id: None,
        next_run_id: None,
        run_state: None,
        last_commit: String::new(),
        pr_url: None,
        review_state: None,
        state: ContinuityState::Unknown,
        evidence: vec![],
        next_action: action(ContinuityState::Unknown),
    }
}
fn state_for_run(run: &RunManifest, at: i64) -> ContinuityState {
    if run.state == RunState::Blocked || run.waiting_on.as_ref().is_some_and(|s| !s.is_empty()) {
        return ContinuityState::Blocked;
    }
    match run.state {
        RunState::Running | RunState::Starting | RunState::Requested => {
            if run.last_seen_at > at || run.last_progress_at > at {
                return ContinuityState::Unknown;
            }
            if at.saturating_sub(run.last_seen_at) > run_control::GRACE_MS
                || at.saturating_sub(run.last_progress_at) > run_control::PROGRESS_WINDOW_MS
            {
                ContinuityState::Stalled
            } else {
                ContinuityState::Active
            }
        }
        RunState::Degraded | RunState::Retiring | RunState::Undead => ContinuityState::Stalled,
        // Even a last_commit may be the initial checkout; it is a locator,
        // not proof that this worker implemented anything.
        _ => ContinuityState::Unknown,
    }
}
fn sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn review_verified(t: &Transfer, run: Option<&RunManifest>, revoked: bool) -> bool {
    let Some(q) = &t.quality else {
        return false;
    };
    let Some(r) = &q.report else {
        return false;
    };
    !revoked
        && ["ready", "merged"].contains(&q.state.as_str())
        && q.reviewer != t.handle
        && !q.reviewer.is_empty()
        && q.child
            .as_ref()
            .is_some_and(|id| !id.is_empty() && id != &t.run_id)
        && q.comment_url.as_ref().is_some_and(|s| !s.is_empty())
        && sha(&q.head)
        && sha(&q.base)
        && run.is_none_or(|run| run.state.terminal() && run.last_commit == q.head)
        && r["head"].as_str() == Some(q.head.as_str())
        && r["base"].as_str() == Some(q.base.as_str())
        && r["verdict"] == "pass"
        && r["published_head"].as_str().is_some_and(sha)
        && r["coverage_gaps"].as_array().is_some_and(Vec::is_empty)
        && r["findings"]
            .as_array()
            .is_some_and(|fs| fs.iter().all(|f| f["severity"] == "info"))
        && r["tests"].as_array().is_some_and(|tests| {
            !tests.is_empty()
                && tests.iter().all(|test| {
                    test["exit_code"] == 0
                        && test["command"]
                            .as_str()
                            .is_some_and(|s| !s.trim().is_empty())
                        && test["evidence"].as_str().is_some_and(|path| {
                            !path.is_empty()
                                && r["evidence_excerpts"].as_array().is_some_and(|es| {
                                    es.iter().any(|e| {
                                        e["path"] == path
                                            && e["output"]
                                                .as_str()
                                                .is_some_and(|s| !s.trim().is_empty())
                                    })
                                })
                        })
                })
        })
}

/// Deterministic projection. Inputs are historical records, not live probes.
/// Distinct run IDs always survive, including stopped predecessors. Repeated
/// copies of one run ID choose highest revision, with a deterministic tie-break.
pub fn reconcile(
    project: &str,
    tickets: &[TicketRecord],
    runs: &[RunManifest],
    transfers: &[Transfer],
    at_ms: i64,
) -> ProjectSnapshot {
    let mut result = ProjectSnapshot {
        project: project.into(),
        observed_at: at_ms,
        tickets: vec![],
        assignments: vec![],
        diagnostics: vec![],
    };
    let mut selected: BTreeMap<&str, &RunManifest> = BTreeMap::new();
    for r in runs.iter().filter(|r| r.project == project) {
        if let Some(old) = selected.get(r.run_id.as_str()) {
            if *old != r {
                result.diagnostics.push(Diagnostic { blocking: true, source: format!("run:{}", r.run_id), message: "Conflicting copies of this run; highest revision selected. Inspect source history.".into() });
            }
            if (old.revision, serde_json::to_string(old).unwrap_or_default())
                >= (r.revision, serde_json::to_string(r).unwrap_or_default())
            {
                continue;
            }
        }
        selected.insert(&r.run_id, r);
    }
    let mut rows = BTreeMap::new();
    for r in selected.values() {
        let mut a = empty_assignment(&r.run_id);
        a.ticket = r.ticket.clone();
        a.owner = r.agent_handle.clone();
        a.branch = r.branch.clone();
        a.worktree = r.worktree_path.clone();
        a.previous_run_id = r.previous_run_id.clone();
        a.next_run_id = r.next_run_id.clone();
        a.run_state = Some(r.state);
        a.last_commit = r.last_commit.clone();
        a.state = state_for_run(r, at_ms);
        a.evidence.push(evidence("run", format!("run:{}", r.run_id), format!("Recorded {:?}; last seen {}, last progress {}; last commit {} (may be initial checkout); signal {}; waiting on {}. This is stored observation, not a live process check.", r.state, r.last_seen_at, r.last_progress_at, r.last_commit, r.last_signal, r.waiting_on.as_deref().unwrap_or("none"))));
        rows.insert(r.run_id.clone(), a);
    }
    let mut ts: Vec<_> = transfers.iter().filter(|t| t.project == project).collect();
    ts.sort_by_key(|t| {
        (
            t.run_id.clone(),
            serde_json::to_string(t).unwrap_or_default(),
        )
    });
    for t in ts {
        let a = rows
            .entry(t.run_id.clone())
            .or_insert_with(|| empty_assignment(&t.run_id));
        if a.ticket.is_none() {
            a.ticket = t.ticket.clone();
        }
        if a.owner.is_empty() {
            a.owner = t.handle.clone();
        }
        if a.branch.is_empty() {
            a.branch = t.branch.clone();
        }
        if a.worktree.is_empty() {
            a.worktree = t.local_path.clone();
        }
        a.pr_url = t.pr_url.clone();
        a.evidence.push(evidence(
            "transfer",
            format!("transfer:{}", t.run_id),
            format!(
                "Repository handoff {}; branch {}; artifacts {}; error {}",
                t.state,
                t.branch,
                t.artifacts,
                t.error.as_deref().unwrap_or("none")
            ),
        ));
        if let Some(pr) = &t.pr_url {
            a.evidence.push(evidence(
                "pull_request",
                pr.clone(),
                "Existing PR; inspect it before starting another implementation.",
            ));
        }
        if let Some(q) = &t.quality {
            a.review_state = Some(q.state.clone());
            a.evidence.push(evidence(
                "review",
                format!("transfer:{}#quality", t.run_id),
                format!(
                    "Stored independent review {}; head {}; base {}; {}",
                    q.state, q.head, q.base, q.message
                ),
            ));
        }
        let revoked = tickets
            .iter()
            .filter(|ticket| Some(&ticket.id) == a.ticket.as_ref())
            .any(|ticket| {
                ticket
                    .approval
                    .signoff
                    .as_ref()
                    .is_some_and(|s| s.revoked || s.revert_sha.is_some())
                    || ticket
                        .approval
                        .jury_reviews
                        .iter()
                        .any(|j| ["revoked", "revoke_requested"].contains(&j.state.as_str()))
            });
        if review_verified(t, selected.get(t.run_id.as_str()).copied(), revoked) {
            a.state = ContinuityState::Verified;
            a.evidence.push(evidence("verification", format!("transfer:{}#quality/report", t.run_id), format!("Independent evidenced tests passed at {}; recorded review accepted. Current remote refs and merge permission were not checked.", t.quality.as_ref().unwrap().head)));
        } else if t
            .quality
            .as_ref()
            .is_some_and(|q| ["blocked", "changes_requested"].contains(&q.state.as_str()))
            || revoked
        {
            a.state = ContinuityState::Blocked;
        } else if (a.pr_url.is_some() || t.quality.is_some())
            && a.state != ContinuityState::Active
            && a.state != ContinuityState::Blocked
        {
            a.state = ContinuityState::Review;
        }
    }
    let pr_pattern = regex::Regex::new(r#"https?://[^\s<>"')]+/(?:pulls|pull)/[0-9]+"#);
    for ticket in tickets.iter().filter(|t| t.project == project) {
        if let Some(h) = &ticket.handback {
            // Unbound legacy handbacks stay ticket evidence; never attach them
            // to whichever worker happens to be newest.
            if let Some(id) = &h.run_id {
                let a = rows
                    .entry(id.clone())
                    .or_insert_with(|| empty_assignment(id));
                if a.ticket.is_none() {
                    a.ticket = Some(ticket.id.clone());
                }
                if a.ticket.as_deref() == Some(ticket.id.as_str()) {
                    a.evidence.push(evidence("handback", format!("ticket:{}#handback", ticket.id), format!("Author claim submitted {}: {}; commits {:?}; verification claim {}; unfinished {:?}", h.submitted_at, h.summary, h.commits, h.how_verified, h.not_finished)));
                    if a.owner.is_empty() {
                        a.owner = h.from.clone();
                    }
                    if matches!(a.state, ContinuityState::Unknown | ContinuityState::Stalled) {
                        a.state = ContinuityState::Review;
                    }
                }
            }
        }
        let mut ev = vec![evidence("ticket", format!("ticket:{}", ticket.id), format!("Recorded status {}, owner {:?}, revision {}, updated {}. Ticket status is not verification.", ticket.status, ticket.owner, ticket.revision, ticket.updated_at))];
        // Older tasks may have only a PR link in their durable PM body.
        // A link locates existing work; it never proves the PR was merged.
        if let Ok(pattern) = &pr_pattern {
            for link in pattern.find_iter(&ticket.body) {
                ev.push(evidence("pull_request", link.as_str(), "PR referenced in ticket body; recover and inspect the existing implementation."));
            }
        }
        if let Some(h) = &ticket.handback {
            ev.push(evidence(
                "handback",
                format!("ticket:{}#handback", ticket.id),
                format!(
                    "Claim from {} at {}: {}; verification claim: {}; unfinished {:?}",
                    h.from, h.submitted_at, h.summary, h.how_verified, h.not_finished
                ),
            ));
        }
        result.tickets.push(TicketSnapshot {
            id: ticket.id.clone(),
            title: ticket.title.clone(),
            status: ticket.status.clone(),
            owner: ticket.owner.clone(),
            state: ContinuityState::Unknown,
            assignment_ids: vec![],
            evidence: ev,
            next_action: String::new(),
        });
    }
    result.assignments = rows.into_values().collect();
    for ticket in tickets.iter().filter(|t| t.project == project) {
        for (index, chunk) in ticket.body.split("## Dispatched ").skip(1).enumerate() {
            let section = chunk.split("\n## ").next().unwrap_or(chunk);
            let field = |name: &str| {
                section
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix(&format!("- {name} `"))
                            .and_then(|v| v.strip_suffix('`'))
                    })
                    .unwrap_or("")
            };
            let session = field("session");
            let matched = runs.iter().find(|r| {
                r.project == project
                    && r.ticket.as_deref() == Some(ticket.id.as_str())
                    && !session.is_empty()
                    && (r.zellij_session.as_deref() == Some(session)
                        || r.pty_session.as_deref() == Some(session))
            });
            let id = matched
                .map(|r| r.run_id.clone())
                .unwrap_or_else(|| format!("dispatch:{}:{index}", ticket.id));
            let position = result
                .assignments
                .iter()
                .position(|a| a.run_id == id)
                .unwrap_or_else(|| {
                    result.assignments.push(empty_assignment(&id));
                    result.assignments.len() - 1
                });
            let a = &mut result.assignments[position];
            a.ticket = Some(ticket.id.clone());
            if a.branch.is_empty() {
                a.branch = field("branch").into();
            }
            if a.worktree.is_empty() {
                a.worktree = field("worktree").into();
            }
            if a.owner.is_empty() {
                a.owner = section
                    .lines()
                    .next()
                    .and_then(|line| line.split(" to @").nth(1))
                    .unwrap_or("")
                    .trim()
                    .into();
            }
            a.evidence.push(evidence(
                "launch",
                format!("ticket:{}#dispatch-{index}", ticket.id),
                format!(
                    "Historical dispatch {}; session {}. Current liveness and completion unknown.",
                    section.lines().next().unwrap_or(""),
                    session
                ),
            ));
        }
        for (index, chunk) in ticket
            .body
            .split("Worker launch (not completion):")
            .skip(1)
            .enumerate()
        {
            let mut reader =
                serde_json::Deserializer::from_str(chunk.trim_start()).into_iter::<Value>();
            if let Some(Ok(mut receipt)) = reader.next() {
                if receipt["ticket"].is_null() {
                    receipt["ticket"] = Value::String(ticket.id.clone());
                }
                add_launch_receipt(
                    &mut result,
                    &receipt,
                    &format!("ticket:{}#launch-{index}", ticket.id),
                );
            }
        }
    }
    refresh(&mut result);
    result
}

fn resolved_by_successor(assignment: &AssignmentSnapshot, all: &[AssignmentSnapshot]) -> bool {
    if !assignment.run_state.is_some_and(RunState::terminal) {
        return false;
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut current = assignment;
    while let Some(next) = &current.next_run_id {
        if !seen.insert(&current.run_id) {
            return false;
        }
        let Some(successor) = all.iter().find(|a| &a.run_id == next) else {
            return false;
        };
        if successor.previous_run_id.as_deref() != Some(current.run_id.as_str())
            || successor.ticket != current.ticket
            || successor.branch != current.branch
            || successor.worktree != current.worktree
        {
            return false;
        }
        if successor.state == ContinuityState::Verified {
            return true;
        }
        if !successor.run_state.is_some_and(RunState::terminal) {
            return false;
        }
        current = successor;
    }
    false
}

fn refresh(snapshot: &mut ProjectSnapshot) {
    snapshot.assignments.sort_by(|a, b| a.run_id.cmp(&b.run_id));
    snapshot.tickets.sort_by(|a, b| a.id.cmp(&b.id));
    snapshot
        .diagnostics
        .sort_by(|a, b| (&a.source, &a.message).cmp(&(&b.source, &b.message)));
    snapshot.diagnostics.dedup();
    for diagnostic in &mut snapshot.diagnostics {
        diagnostic.message = crate::project_wiki::redact(&diagnostic.message);
    }
    for a in &mut snapshot.assignments {
        a.next_action = action(a.state);
    }
    for t in &mut snapshot.tickets {
        let assignments: Vec<_> = snapshot
            .assignments
            .iter()
            .filter(|a| a.ticket.as_deref() == Some(t.id.as_str()))
            .collect();
        t.assignment_ids = assignments.iter().map(|a| a.run_id.clone()).collect();
        // Keep every historical row but allow a proven, reciprocal successor
        // chain in the same workspace to resolve its stopped predecessors.
        let assignments: Vec<_> = assignments
            .into_iter()
            .filter(|a| !resolved_by_successor(a, &snapshot.assignments))
            .collect();
        // Do not let one verified predecessor hide another unresolved run.
        t.state = if t.status == "blocked" {
            ContinuityState::Blocked
        } else if assignments
            .iter()
            .any(|a| a.state == ContinuityState::Active)
        {
            ContinuityState::Active
        } else if assignments
            .iter()
            .any(|a| a.state == ContinuityState::Blocked)
        {
            ContinuityState::Blocked
        } else if assignments
            .iter()
            .any(|a| a.state == ContinuityState::Stalled)
        {
            ContinuityState::Stalled
        } else if assignments
            .iter()
            .any(|a| a.state == ContinuityState::Review)
        {
            ContinuityState::Review
        } else if !assignments.is_empty()
            && assignments
                .iter()
                .all(|a| a.state == ContinuityState::Verified)
        {
            ContinuityState::Verified
        } else if t
            .evidence
            .iter()
            .any(|e| e.kind == "handback" || e.kind == "pull_request")
        {
            ContinuityState::Review
        } else {
            ContinuityState::Unknown
        };
        t.next_action = action(t.state);
    }
}

/// Attach authorized launch receipts recovered from any persisted conversation.
/// Receipt-only workers are retained even when registry/PM tracking failed.
pub fn add_launch_receipt(snapshot: &mut ProjectSnapshot, receipt: &Value, source: &str) {
    let receipt = receipt.get("receipt").unwrap_or(receipt);
    let Some(ticket) = receipt["ticket"].as_str() else {
        return;
    };
    if !ticket.starts_with(&format!("{}-", snapshot.project)) {
        return;
    }
    let id = receipt["launch"]["run_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("receipt:{source}"));
    let index = snapshot
        .assignments
        .iter()
        .position(|a| a.run_id == id)
        .unwrap_or_else(|| {
            snapshot.assignments.push(empty_assignment(&id));
            snapshot.assignments.len() - 1
        });
    let a = &mut snapshot.assignments[index];
    if a.ticket.is_none() {
        a.ticket = Some(ticket.into());
    }
    if a.owner.is_empty() {
        a.owner = receipt["handle"].as_str().unwrap_or("").into();
    }
    if a.worktree.is_empty() {
        a.worktree = receipt["worktree_path"].as_str().unwrap_or("").into();
    }
    if a.branch.is_empty() {
        a.branch = receipt["branch"].as_str().unwrap_or("").into();
    }
    let ev = evidence("launch_receipt", source, format!("Historical launch receipt; execution_started {}; thread {}; task key {}; pending {}. Does not prove current liveness or completion.", receipt["execution_started"], receipt["origin_thread_id"], receipt["task_key"], receipt["pending"]));
    if !a.evidence.contains(&ev) {
        a.evidence.push(ev);
    }
    refresh(snapshot);
}

fn read_jsons<T: serde::de::DeserializeOwned>(
    dir: &Path,
    optional: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(std::path::PathBuf, T)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            if !(optional && e.kind() == std::io::ErrorKind::NotFound) {
                diagnostics.push(Diagnostic {
                    blocking: true,
                    source: dir.display().to_string(),
                    message: format!("Source unavailable: {e}"),
                });
            }
            return vec![];
        }
    };
    let mut paths = vec![];
    for entry in entries {
        match entry {
            Ok(entry) if entry.path().extension().and_then(|s| s.to_str()) == Some("json") => {
                paths.push(entry.path())
            }
            Ok(_) => {}
            Err(e) => diagnostics.push(Diagnostic {
                blocking: true,
                source: dir.display().to_string(),
                message: e.to_string(),
            }),
        }
    }
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            match std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
            {
                Ok(row) => Some((path, row)),
                Err(e) => {
                    diagnostics.push(Diagnostic {
                        blocking: true,
                        source: path.display().to_string(),
                        message: format!("Unreadable evidence: {e}"),
                    });
                    None
                }
            }
        })
        .collect()
}

// Shared stores contain other projects. Deserialize the scope envelope before
// typed data so corrupt foreign records cannot expose content or block this one.
fn record_scope(value: &Value, project: &str) -> Option<bool> {
    let value = value.get("receipt").unwrap_or(value);
    if let Some(key) = value["project"].as_str().filter(|s| !s.is_empty()) {
        return Some(key == project);
    }
    value["ticket"]
        .as_str()
        .map(|id| id.starts_with(&format!("{project}-")))
}
fn unscoped_diagnostic(source: &str, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.push(Diagnostic {
        blocking: false,
        source: source.into(),
        message: "Some shared records could not be attributed to a project. Their contents are omitted; inspect the shared store separately if expected work is missing.".into(),
    });
}
fn scoped_jsons<T: serde::de::DeserializeOwned>(
    dir: &Path,
    project: &str,
    label: &str,
    known: &std::collections::BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(std::path::PathBuf, T)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return vec![],
        Err(_) => {
            diagnostics.push(Diagnostic {
                blocking: true,
                source: label.into(),
                message: "Shared store unavailable; cannot recover this project's records.".into(),
            });
            return vec![];
        }
    };
    let mut rows = vec![];
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                unscoped_diagnostic(label, diagnostics);
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let linked = path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|id| known.contains(id));
        let raw = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        let scope = raw.as_ref().and_then(|value| record_scope(value, project));
        if scope == Some(false) {
            if linked {
                diagnostics.push(Diagnostic { blocking: true, source: label.into(), message: "Project references a record attributed to another project; resolve the identity conflict.".into() });
            }
            continue;
        }
        if scope != Some(true) && !linked {
            unscoped_diagnostic(label, diagnostics);
            continue;
        }
        match raw
            .ok_or_else(|| "JSON unavailable or invalid".to_string())
            .and_then(|raw| serde_json::from_value(raw).map_err(|e| e.to_string()))
        {
            Ok(row) => rows.push((path, row)),
            Err(e) => diagnostics.push(Diagnostic {
                blocking: true,
                source: path.display().to_string(),
                message: format!("Unreadable project evidence: {e}"),
            }),
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}
fn failed_run_scope(registry: &Path, id: &str, project: &str) -> Option<bool> {
    let mut scopes = vec![];
    if let Ok(bytes) = std::fs::read(registry.join(format!("{id}.run.json"))) {
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            scopes.push(record_scope(&value, project));
        }
    }
    if let Ok(bytes) = std::fs::read_to_string(registry.join(format!("{id}.events.jsonl"))) {
        for line in bytes.lines() {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                scopes.push(record_scope(&value["run"], project));
            }
        }
    }
    if scopes.contains(&Some(true)) {
        Some(true)
    } else if !scopes.is_empty() && scopes.iter().all(|scope| *scope == Some(false)) {
        Some(false)
    } else {
        None
    }
}

pub fn snapshot(project: &str) -> Result<ProjectSnapshot, String> {
    let repo = crate::project_management::repo_now()?;
    let mut result = snapshot_in(
        &repo,
        &crate::agents::registry_dir()?,
        project,
        run_control::now_ms(),
    )?;
    let project_path = repo.join("projects").join(project).join("project.json");
    match crate::project_management::read_json::<crate::project_management::ProjectRecord>(
        &project_path,
    ) {
        Ok(record) if record.key == project => {
            let local = crate::project_management::local_source_path(&record);
            if !local.is_empty() {
                match crate::agent_history::project_receipts(project, Path::new(&local)) {
                    Ok(receipts) => {
                        for (source, receipt) in receipts {
                            add_launch_receipt(&mut result, &receipt, &source);
                        }
                    }
                    Err(message) => result.diagnostics.push(Diagnostic {
                        blocking: true,
                        source: "conversation launch receipts".into(),
                        message,
                    }),
                }
            }
        }
        _ => result.diagnostics.push(Diagnostic {
            blocking: true,
            source: project_path.display().to_string(),
            message: "Project identity unavailable; conversation receipts could not be scoped."
                .into(),
        }),
    }
    refresh(&mut result);
    Ok(result)
}

/// Git omits empty directories when a newly registered project is cloned.
/// Missing tickets are empty only when the registered identity and this exact
/// control checkout prove that no ticket files are tracked. Failed Git queries
/// and directories deleted from an existing ticket tree remain unavailable.
fn proven_empty_ticket_checkout(control_repo: &Path, project: &str, ticket_dir: &Path) -> bool {
    if !std::fs::symlink_metadata(ticket_dir)
        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    {
        return false;
    }
    let project_path = control_repo
        .join("projects")
        .join(project)
        .join("project.json");
    let Ok(record) = crate::project_management::read_json::<crate::project_management::ProjectRecord>(
        &project_path,
    ) else {
        return false;
    };
    if record.key != project {
        return false;
    }
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(control_repo)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args(args)
            .output()
    };
    let Ok(root) = git(&["rev-parse", "--show-toplevel"]) else {
        return false;
    };
    if !root.status.success() {
        return false;
    }
    let Ok(root) = std::str::from_utf8(&root.stdout) else {
        return false;
    };
    let Ok(actual_root) = Path::new(root.trim_end()).canonicalize() else {
        return false;
    };
    if control_repo.canonicalize().ok().as_ref() != Some(&actual_root) {
        return false;
    }
    let Ok(tracked) = git(&["ls-files", "--", &format!("projects/{project}/tickets")]) else {
        return false;
    };
    tracked.status.success() && tracked.stdout.is_empty()
}

/// Read existing stores without creating them or reconciling their lifecycle.
/// A missing PM/registry source is reported explicitly; absent optional transfer
/// and chat-launch directories mean those mechanisms have recorded no work.
pub fn snapshot_in(
    control_repo: &Path,
    registry: &Path,
    project: &str,
    at_ms: i64,
) -> Result<ProjectSnapshot, String> {
    if project.is_empty()
        || !project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("Invalid project key".into());
    }
    let mut diagnostics = vec![];
    let ticket_dir = control_repo.join("projects").join(project).join("tickets");
    let fresh_checkout = proven_empty_ticket_checkout(control_repo, project, &ticket_dir);
    let tickets: Vec<TicketRecord> = read_jsons(&ticket_dir, fresh_checkout, &mut diagnostics)
        .into_iter()
        .map(|(_, r)| r)
        .collect();
    // Recover explicit IDs first, so even invalid global records can be
    // attributed when a project-local handback/launch receipt names them.
    let ticket_evidence = reconcile(project, &tickets, &[], &[], at_ms);
    let mut known: std::collections::BTreeSet<String> = ticket_evidence
        .assignments
        .iter()
        .map(|a| a.run_id.clone())
        .collect();
    let transfer_dir = registry.join("repository-transfers");
    let transfers: Vec<Transfer> = scoped_jsons(
        &transfer_dir,
        project,
        "repository transfers",
        &known,
        &mut diagnostics,
    )
    .into_iter()
    .map(|(_, r)| r)
    .collect();
    known.extend(transfers.iter().map(|t| t.run_id.clone()));
    let receipts: Vec<(_, Value)> = scoped_jsons(
        &registry.join("chat-launches"),
        project,
        "launch receipts",
        &known,
        &mut diagnostics,
    );
    for (_, r) in &receipts {
        let r = r.get("receipt").unwrap_or(r);
        if let Some(id) = r["launch"]["run_id"].as_str() {
            known.insert(id.into());
        }
    }
    let mut runs = vec![];
    let mut ids = std::collections::BTreeSet::new();
    match std::fs::read_dir(registry) {
        Ok(entries) => for entry in entries {
            match entry {
                Ok(entry) => {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Some(id) = name.strip_suffix(".run.json").or_else(|| name.strip_suffix(".events.jsonl")) { ids.insert(id.to_string()); }
                },
                Err(_) => diagnostics.push(Diagnostic { blocking: true, source: "run registry".into(), message: "Run registry directory could not be read completely.".into() }),
            }
        },
        Err(e) => diagnostics.push(Diagnostic {
            blocking: e.kind() != std::io::ErrorKind::NotFound,
            source: "run registry".into(),
            message: if e.kind() == std::io::ErrorKind::NotFound { "Run registry has not been created. Historical ticket and launch evidence is still checked.".into() } else { "Run registry unavailable; cannot recover this project's assignments.".into() },
        }),
    }
    for id in ids {
        match run_control::load_manifest_in(registry, &id) {
            Ok(run) if run.project == project => runs.push(run),
            Ok(_) => {}
            Err(e) => match failed_run_scope(registry, &id, project) {
                Some(true) => diagnostics.push(Diagnostic {
                    blocking: true,
                    source: registry
                        .join(format!("{id}.run.json"))
                        .display()
                        .to_string(),
                    message: format!("Unreadable project run history: {e}"),
                }),
                _ if known.contains(&id) => diagnostics.push(Diagnostic {
                    blocking: true,
                    source: registry
                        .join(format!("{id}.run.json"))
                        .display()
                        .to_string(),
                    message: "Project references an unreadable or conflicting run record.".into(),
                }),
                Some(false) => {}
                None => unscoped_diagnostic("run registry", &mut diagnostics),
            },
        }
    }
    let mut result = reconcile(project, &tickets, &runs, &transfers, at_ms);
    result.diagnostics.extend(diagnostics);
    for (path, receipt) in receipts {
        add_launch_receipt(&mut result, &receipt, &path.display().to_string());
    }
    // Resolve symbolic provenance to real files only in the IO adapter.
    for ev in result
        .tickets
        .iter_mut()
        .flat_map(|t| t.evidence.iter_mut())
        .chain(
            result
                .assignments
                .iter_mut()
                .flat_map(|a| a.evidence.iter_mut()),
        )
    {
        for (prefix, dir, suffix) in [
            ("ticket:", ticket_dir.as_path(), ".json"),
            ("run:", registry, ".events.jsonl"),
            ("transfer:", transfer_dir.as_path(), ".json"),
        ] {
            if let Some(rest) = ev.source.strip_prefix(prefix) {
                let (id, fragment) = rest.split_once('#').unwrap_or((rest, ""));
                let source_path = if prefix == "run:" && !dir.join(format!("{id}{suffix}")).exists()
                {
                    dir.join(format!("{id}.run.json"))
                } else {
                    dir.join(format!("{id}{suffix}"))
                };
                ev.source = format!(
                    "{}{}{}",
                    source_path.display(),
                    if fragment.is_empty() { "" } else { "#" },
                    fragment
                );
                break;
            }
        }
    }
    refresh(&mut result);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ticket() -> TicketRecord {
        serde_json::from_value(json!({"id":"XNAUT-900","project":"XNAUT","title":"Recover implementation","type":"feature","status":"in_progress","priority":"high","owner":"codex","revision":1,"created_at":"2026-10-05T12:00:00Z","updated_at":"2026-10-05T12:00:00Z"})).unwrap()
    }
    fn run() -> RunManifest {
        crate::run_control::tests::run()
    }
    fn transfer(r: &RunManifest) -> Transfer {
        serde_json::from_value(json!({"run_id":r.run_id,"project":r.project,"ticket":r.ticket,"handle":r.agent_handle,"local_path":r.worktree_path,"remote":"git@example.invalid:team/project.git","source_sha":"a".repeat(40),"base":"dev","branch":r.branch,"workdir":"/worker","artifacts":".xnaut/runs/test","state":"failed","pr_url":"https://forge.invalid/pulls/17","error":"Worker exited after publication"})).unwrap()
    }
    fn accepted_review(t: &mut Transfer, r: &RunManifest) {
        let head = r.last_commit.clone();
        let base = "b".repeat(40);
        t.quality = Some(crate::repository_review::Review {
            state: "ready".into(),
            reviewer: "reviewer".into(),
            head: head.clone(),
            base: base.clone(),
            child: Some("review-child".into()),
            comment_url: Some("https://forge.invalid/pulls/17#review".into()),
            report: Some(
                json!({"head":head,"base":base,"verdict":"pass","published_head":"c".repeat(40),"coverage_gaps":[],"findings":[],"tests":[{"command":"cargo test","exit_code":0,"evidence":"test.log"}],"evidence_excerpts":[{"path":"test.log","output":"Tests passed"}]}),
            ),
            ..Default::default()
        });
    }
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("xnaut-continuity-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // XNAUT-462: reproduce the reported shape of the incident from disk, not
    // an in-memory status string: three tasks, five workers, only one PR.
    #[test]
    fn restart_recovers_all_three_tasks_and_five_workers_before_any_relaunch() {
        let scratch = Scratch::new();
        let registry = scratch.0.join("registry");
        let ticket_dir = scratch.0.join("projects/XNAUT/tickets");
        std::fs::create_dir_all(&ticket_dir).unwrap();
        std::fs::create_dir_all(registry.join("repository-transfers")).unwrap();
        for number in 900..903 {
            let mut t = ticket();
            t.id = format!("XNAUT-{number}");
            std::fs::write(
                ticket_dir.join(format!("{}.json", t.id)),
                serde_json::to_vec(&t).unwrap(),
            )
            .unwrap();
        }
        let mut ids = Vec::new();
        for index in 0..5 {
            let mut r = run();
            r.ticket = Some(format!("XNAUT-{}", 900 + index % 3));
            r.state = if index == 0 {
                RunState::Done
            } else {
                RunState::Failed
            };
            r.branch = format!("agent/preserved-{index}");
            r.worktree_path = format!("/preserved/worktree-{index}");
            r.last_commit = "d".repeat(40);
            ids.push(r.run_id.clone());
            std::fs::write(
                registry.join(format!("{}.run.json", r.run_id)),
                serde_json::to_vec(&r).unwrap(),
            )
            .unwrap();
            if index == 0 {
                let tr = transfer(&r);
                std::fs::write(
                    registry.join("repository-transfers/only-pr.json"),
                    serde_json::to_vec(&tr).unwrap(),
                )
                .unwrap();
            }
        }
        let first = snapshot_in(&scratch.0, &registry, "XNAUT", 50_000).unwrap();
        assert_eq!(first.tickets.len(), 3);
        assert_eq!(first.assignments.len(), 5);
        assert_eq!(
            first
                .assignments
                .iter()
                .filter(|a| a.pr_url.is_some())
                .count(),
            1
        );
        assert!(first.diagnostics.is_empty());
        let restored: ProjectSnapshot =
            serde_json::from_slice(&serde_json::to_vec(&first).unwrap()).unwrap();
        assert_eq!(
            restored,
            snapshot_in(&scratch.0, &registry, "XNAUT", 50_000).unwrap()
        );
        for t in &restored.tickets {
            assert_ne!(t.state, ContinuityState::Verified);
            let refusal =
                crate::agent_work::recovery_guard(&json!(restored), &t.id, None).unwrap_err();
            assert!(refusal.contains("no replacement"));
        }
        assert!(ids
            .iter()
            .all(|id| restored.assignments.iter().any(|a| &a.run_id == id)));
    }

    #[test]
    fn fresh_running_then_stale_after_restart_is_not_verified() {
        let r = run();
        let t = ticket();
        let current = reconcile("XNAUT", &[t.clone()], &[r.clone()], &[], 1_001);
        assert_eq!(current.assignments[0].state, ContinuityState::Active);
        let restarted: RunManifest =
            serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
        let later = reconcile(
            "XNAUT",
            &[t],
            &[restarted],
            &[],
            1_000 + run_control::PROGRESS_WINDOW_MS + 1,
        );
        assert_eq!(later.assignments[0].state, ContinuityState::Stalled);
        assert_eq!(later.assignments[0].run_id, current.assignments[0].run_id);
    }
    #[test]
    fn process_done_and_legacy_ticket_complete_are_not_verification() {
        for status in ["done", "complete", "completed"] {
            let mut t = ticket();
            t.status = status.into();
            let mut r = run();
            r.state = RunState::Done;
            let s = reconcile("XNAUT", &[t], &[r], &[], 2_000);
            assert_eq!(s.tickets[0].state, ContinuityState::Unknown);
            assert_eq!(s.assignments[0].state, ContinuityState::Unknown);
        }
    }
    #[test]
    fn failed_after_commit_and_pr_is_preserved_for_review() {
        let mut r = run();
        r.state = RunState::Failed;
        r.last_commit = "d".repeat(40);
        let tr = transfer(&r);
        let s = reconcile("XNAUT", &[ticket()], &[r.clone()], &[tr], 2_000);
        let a = &s.assignments[0];
        assert_eq!(a.state, ContinuityState::Review);
        assert_eq!(a.run_state, Some(RunState::Failed));
        assert_eq!(a.last_commit, r.last_commit);
        assert!(a.pr_url.is_some());
        assert!(a.next_action.contains("existing"));
    }
    #[test]
    fn stopped_without_pr_still_has_branch_worktree_and_commit() {
        let mut r = run();
        r.state = RunState::Retired;
        let s = reconcile("XNAUT", &[ticket()], &[r.clone()], &[], 2_000);
        assert_eq!(s.assignments.len(), 1);
        assert_eq!(s.assignments[0].branch, r.branch);
        assert_eq!(s.assignments[0].worktree, r.worktree_path);
        assert_eq!(s.assignments[0].last_commit, r.last_commit);
        assert_eq!(s.assignments[0].state, ContinuityState::Unknown);
    }
    #[test]
    fn predecessor_and_parallel_runs_survive_without_duplicate_copies() {
        let mut predecessor = run();
        predecessor.state = RunState::Failed;
        let mut successor = run();
        successor.previous_run_id = Some(predecessor.run_id.clone());
        predecessor.next_run_id = Some(successor.run_id.clone());
        let s = reconcile(
            "XNAUT",
            &[ticket()],
            &[successor.clone(), predecessor.clone(), successor.clone()],
            &[],
            2_000,
        );
        assert_eq!(s.assignments.len(), 2);
        assert_eq!(s.tickets[0].assignment_ids.len(), 2);
        assert!(s
            .assignments
            .iter()
            .any(|a| a.previous_run_id.as_ref() == Some(&predecessor.run_id)));
        let reversed = reconcile("XNAUT", &[ticket()], &[predecessor, successor], &[], 2_000);
        assert_eq!(s, reversed);
    }
    #[test]
    fn conflicting_revision_selection_is_order_independent() {
        let a = run();
        let mut b = a.clone();
        b.revision += 1;
        b.state = RunState::Failed;
        let first = reconcile("XNAUT", &[], &[a.clone(), b.clone()], &[], 2_000);
        let reversed = reconcile("XNAUT", &[], &[b, a], &[], 2_000);
        assert_eq!(first, reversed);
        assert_eq!(first.assignments[0].run_state, Some(RunState::Failed));
        assert_eq!(first.diagnostics.len(), 1);
    }
    #[test]
    fn exact_independent_evidence_is_required_for_verified() {
        let mut r = run();
        r.state = RunState::Done;
        r.last_commit = "d".repeat(40);
        let mut tr = transfer(&r);
        accepted_review(&mut tr, &r);
        assert_eq!(
            reconcile("XNAUT", &[ticket()], &[r.clone()], &[tr.clone()], 2_000).assignments[0]
                .state,
            ContinuityState::Verified
        );
        for mutation in ["head", "tests", "reviewer", "state", "evidence"] {
            let mut broken = tr.clone();
            let q = broken.quality.as_mut().unwrap();
            match mutation {
                "head" => q.head = "f".repeat(40),
                "tests" => q.report.as_mut().unwrap()["tests"][0]["exit_code"] = json!(1),
                "reviewer" => q.reviewer = r.agent_handle.clone(),
                "state" => q.state = "running".into(),
                _ => q.report.as_mut().unwrap()["evidence_excerpts"] = json!([]),
            }
            assert_ne!(
                reconcile("XNAUT", &[ticket()], &[r.clone()], &[broken], 2_000).assignments[0]
                    .state,
                ContinuityState::Verified,
                "{mutation}"
            );
        }
        r.last_commit = "e".repeat(40);
        assert_ne!(
            reconcile("XNAUT", &[ticket()], &[r], &[tr], 2_000).assignments[0].state,
            ContinuityState::Verified
        );
    }
    #[test]
    fn revoked_approval_cannot_be_verified_and_review_does_not_hide_other_runs() {
        let mut r = run();
        r.state = RunState::Done;
        r.last_commit = "d".repeat(40);
        let mut tr = transfer(&r);
        accepted_review(&mut tr, &r);
        let mut t = ticket();
        t.approval.signoff = Some(serde_json::from_value(json!({"jury_id":"old","reviewers":[],"scores":[],"merge_sha":"a".repeat(40),"integration_verify_run":null,"revoked":true,"revert_sha":null})).unwrap());
        let s = reconcile("XNAUT", &[t], &[r.clone()], &[tr.clone()], 2_000);
        assert_eq!(s.assignments[0].state, ContinuityState::Blocked);
        let mut unresolved = run();
        unresolved.state = RunState::Failed;
        let s = reconcile("XNAUT", &[ticket()], &[r, unresolved], &[tr], 2_000);
        assert_ne!(s.tickets[0].state, ContinuityState::Verified);
    }
    #[test]
    fn handback_prose_is_review_not_verified_and_unbound_stays_unbound() {
        let mut t = ticket();
        let mut r = run();
        r.state = RunState::Done;
        t.handback = Some(crate::handback::Handback {
            run_id: Some(r.run_id.clone()),
            ticket: t.id.clone(),
            summary: "Implemented".into(),
            how_verified: "All tests passed".into(),
            ..Default::default()
        });
        let s = reconcile("XNAUT", &[t.clone()], &[r.clone()], &[], 2_000);
        assert_eq!(s.assignments[0].state, ContinuityState::Review);
        t.handback.as_mut().unwrap().run_id = None;
        let s = reconcile("XNAUT", &[t], &[r], &[], 2_000);
        assert_eq!(s.assignments[0].state, ContinuityState::Unknown);
        assert_eq!(s.tickets[0].state, ContinuityState::Review);
    }
    #[test]
    fn unavailable_sources_are_visible_and_reads_do_not_create_stores() {
        let scratch = Scratch::new();
        let repo = scratch.0.join("missing-control");
        let registry = scratch.0.join("missing-runs");
        let s = snapshot_in(&repo, &registry, "XNAUT", 2_000).unwrap();
        assert!(s.assignments.is_empty());
        assert_eq!(s.diagnostics.len(), 2);
        assert!(!repo.exists());
        assert!(!registry.exists());
        assert!(snapshot_in(&repo, &registry, "../other", 2_000).is_err());
    }
    #[test]
    fn journal_replay_and_corrupt_source_diagnostics_are_restart_safe() {
        let scratch = Scratch::new();
        let registry = scratch.0.join("registry");
        let tickets = scratch.0.join("projects/XNAUT/tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        std::fs::write(
            tickets.join("XNAUT-900.json"),
            serde_json::to_vec(&ticket()).unwrap(),
        )
        .unwrap();
        std::fs::write(tickets.join("broken.json"), b"{").unwrap();
        let requested = run_control::request_in(&registry, run(), || Ok(())).unwrap();
        run_control::update_in(&registry, &requested.run_id, |r| {
            r.state = RunState::Failed;
            r.last_commit = "after-implementation".into();
        })
        .unwrap();
        let before = snapshot_in(&scratch.0, &registry, "XNAUT", 2_000).unwrap();
        let after = snapshot_in(&scratch.0, &registry, "XNAUT", 2_000).unwrap();
        assert_eq!(before, after);
        assert_eq!(after.assignments[0].last_commit, "after-implementation");
        assert_eq!(after.diagnostics.len(), 1);
        assert!(after.diagnostics[0].source.ends_with("broken.json"));
        assert!(after.assignments[0]
            .evidence
            .iter()
            .any(|e| e.source.ends_with(".events.jsonl")));
    }
    #[test]
    fn receipts_merge_by_run_id_and_pending_orphans_never_disappear() {
        let r = run();
        let mut s = reconcile("XNAUT", &[ticket()], &[r.clone()], &[], 2_000);
        let receipt = json!({"ticket":"XNAUT-900","handle":"codex","execution_started":true,"launch":{"run_id":r.run_id},"branch":"agent/test"});
        add_launch_receipt(&mut s, &receipt, "thread:other");
        add_launch_receipt(&mut s, &receipt, "thread:other");
        assert_eq!(s.assignments.len(), 1);
        assert_eq!(
            s.assignments[0]
                .evidence
                .iter()
                .filter(|e| e.kind == "launch_receipt")
                .count(),
            1
        );
        add_launch_receipt(
            &mut s,
            &json!({"ticket":"XNAUT-900","pending":true}),
            "pending.json",
        );
        assert_eq!(s.assignments.len(), 2);
        assert!(s
            .assignments
            .iter()
            .any(|a| a.run_id == "receipt:pending.json" && a.state == ContinuityState::Unknown));
        add_launch_receipt(
            &mut s,
            &json!({"ticket":"OTHER-1","pending":true}),
            "unrelated.json",
        );
        assert_eq!(s.assignments.len(), 2);
    }
    #[test]
    fn transfer_only_assignment_and_missing_ticket_are_preserved() {
        let r = run();
        let tr = transfer(&r);
        let s = reconcile("XNAUT", &[], &[], &[tr], 2_000);
        assert_eq!(s.assignments.len(), 1);
        assert_eq!(s.assignments[0].run_id, r.run_id);
        assert_eq!(s.assignments[0].state, ContinuityState::Review);
    }
    #[test]
    fn verified_successor_resolves_but_retains_stopped_predecessor() {
        let mut predecessor = run();
        predecessor.state = RunState::Retired;
        let mut successor = run();
        successor.state = RunState::Done;
        successor.last_commit = "d".repeat(40);
        predecessor.next_run_id = Some(successor.run_id.clone());
        successor.previous_run_id = Some(predecessor.run_id.clone());
        let mut tr = transfer(&successor);
        accepted_review(&mut tr, &successor);
        let s = reconcile(
            "XNAUT",
            &[ticket()],
            &[predecessor, successor],
            &[tr],
            2_000,
        );
        assert_eq!(s.assignments.len(), 2);
        assert_eq!(s.tickets[0].assignment_ids.len(), 2);
        assert_eq!(s.tickets[0].state, ContinuityState::Verified);
    }
    #[test]
    fn pm_body_recovers_pr_dispatch_and_launch_when_registry_is_lost() {
        let mut t = ticket();
        t.body = "Existing PR https://forge.invalid/pulls/9074\n\n## Dispatched 2026-10-05 to @codex\n\n- branch `agent/preserved`\n- worktree `/preserved`\n- session `stopped-session`\n\nWorker launch (not completion):\n{\"ticket\":\"XNAUT-900\",\"launch\":{\"run_id\":\"orphan-run\"},\"branch\":\"agent/other\",\"execution_started\":true}".into();
        let s = reconcile("XNAUT", &[t], &[], &[], 2_000);
        assert_eq!(s.assignments.len(), 2);
        assert!(s
            .assignments
            .iter()
            .any(|a| a.branch == "agent/preserved" && a.worktree == "/preserved"));
        assert!(s.assignments.iter().any(|a| a.run_id == "orphan-run"));
        assert!(s.tickets[0]
            .evidence
            .iter()
            .any(|e| e.kind == "pull_request" && e.source.ends_with("9074")));
        assert_eq!(s.tickets[0].state, ContinuityState::Review);
    }
    #[test]
    fn freeform_evidence_redacts_credentials() {
        let mut r = run();
        r.last_signal = "api_key=fixture-private-value".into();
        let s = reconcile("XNAUT", &[], &[r], &[], 2_000);
        assert!(!s.assignments[0].evidence[0]
            .detail
            .contains("fixture-private-value"));
    }
    #[test]
    fn unrelated_corruption_neither_blocks_nor_exposes_shared_records() {
        let scratch = Scratch::new();
        let registry = scratch.0.join("registry");
        std::fs::create_dir_all(scratch.0.join("projects/XNAUT/tickets")).unwrap();
        std::fs::create_dir_all(registry.join("repository-transfers")).unwrap();
        std::fs::create_dir_all(registry.join("chat-launches")).unwrap();
        let other = run();
        std::fs::write(
            registry.join(format!("{}.run.json", other.run_id)),
            br#"{"project":"OTHER","private":"private-other-content","invalid":"schema"}"#,
        )
        .unwrap();
        std::fs::write(
            registry.join("repository-transfers/private-other-name.json"),
            br#"{"project":"OTHER","private":"private-other-content"}"#,
        )
        .unwrap();
        std::fs::write(
            registry.join("chat-launches/private-other-name.json"),
            br#"{"ticket":"OTHER-1","private":"private-other-content"}"#,
        )
        .unwrap();
        std::fs::write(
            registry.join("private-unknown-name.run.json"),
            b"{private-other-content",
        )
        .unwrap();
        std::fs::write(
            registry.join("repository-transfers/private-unknown-name.json"),
            b"{private-other-content",
        )
        .unwrap();
        let s = snapshot_in(&scratch.0, &registry, "XNAUT", 2_000).unwrap();
        assert!(s.assignments.is_empty());
        assert!(!s.diagnostics.is_empty());
        assert!(s.diagnostics.iter().all(|d| !d.blocking));
        let output = serde_json::to_string(&s).unwrap();
        assert!(!output.contains("private-other"));
        assert!(!output.contains("private-unknown"));
        assert!(!output.contains("OTHER"));
    }
    #[test]
    fn current_project_corruption_and_explicitly_linked_runs_still_block() {
        let scratch = Scratch::new();
        let registry = scratch.0.join("registry");
        let ticket_dir = scratch.0.join("projects/XNAUT/tickets");
        std::fs::create_dir_all(&ticket_dir).unwrap();
        std::fs::create_dir_all(registry.join("repository-transfers")).unwrap();
        let r = run();
        let mut t = ticket();
        t.handback = Some(crate::handback::Handback {
            run_id: Some(r.run_id.clone()),
            ..Default::default()
        });
        std::fs::write(
            ticket_dir.join("XNAUT-900.json"),
            serde_json::to_vec(&t).unwrap(),
        )
        .unwrap();
        std::fs::write(registry.join(format!("{}.run.json", r.run_id)), b"{").unwrap();
        std::fs::write(
            registry.join("repository-transfers/own.json"),
            br#"{"project":"XNAUT","invalid":"schema"}"#,
        )
        .unwrap();
        let s = snapshot_in(&scratch.0, &registry, "XNAUT", 2_000).unwrap();
        assert_eq!(s.diagnostics.iter().filter(|d| d.blocking).count(), 2);
        assert!(s.assignments.iter().any(|a| a.run_id == r.run_id));
    }
    #[test]
    fn unused_registry_is_nonblocking_but_unavailable_registry_is_blocking() {
        let scratch = Scratch::new();
        let registry = scratch.0.join("registry");
        std::fs::create_dir_all(scratch.0.join("projects/XNAUT/tickets")).unwrap();
        let empty = snapshot_in(&scratch.0, &registry, "XNAUT", 2_000).unwrap();
        assert!(empty.diagnostics.iter().all(|d| !d.blocking));
        assert_eq!(empty.diagnostics.len(), 1);
        assert!(!registry.exists());
        std::fs::write(&registry, b"not a directory").unwrap();
        let unavailable = snapshot_in(&scratch.0, &registry, "XNAUT", 2_000).unwrap();
        assert!(unavailable.diagnostics.iter().any(|d| d.blocking));
    }
    #[test]
    fn older_diagnostics_default_to_blocking() {
        let d: Diagnostic =
            serde_json::from_value(json!({"source":"old reader","message":"unavailable"})).unwrap();
        assert!(d.blocking);
    }
    fn fixture_git(repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(repo)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                "user.name=Continuity Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn registered_control_fixture(scratch: &Scratch) -> std::path::PathBuf {
        let repo = scratch.0.join("control");
        std::fs::create_dir_all(repo.join("projects/XNAUT/tickets")).unwrap();
        std::fs::write(
            repo.join("projects/XNAUT/project.json"),
            serde_json::to_vec(
                &json!({"key":"XNAUT","name":"Fixture","created_at":"2026-10-05T12:00:00Z"}),
            )
            .unwrap(),
        )
        .unwrap();
        fixture_git(&repo, &["init"]);
        fixture_git(&repo, &["add", "projects/XNAUT/project.json"]);
        fixture_git(&repo, &["commit", "-m", "Register fresh project"]);
        repo
    }
    #[test]
    fn fresh_registered_clone_without_empty_ticket_directory_is_not_blocked() {
        let scratch = Scratch::new();
        let source = registered_control_fixture(&scratch);
        fixture_git(&scratch.0, &["clone", source.to_str().unwrap(), "clone"]);
        let clone = scratch.0.join("clone");
        let tickets = clone.join("projects/XNAUT/tickets");
        assert!(!tickets.exists());
        let snapshot = snapshot_in(&clone, &scratch.0.join("registry"), "XNAUT", 2_000).unwrap();
        assert!(snapshot.tickets.is_empty());
        assert!(snapshot.diagnostics.iter().all(|d| !d.blocking));
        assert!(
            !tickets.exists(),
            "The reader must not create missing directories"
        );
    }
    #[test]
    fn missing_previously_tracked_ticket_directory_stays_blocking() {
        let scratch = Scratch::new();
        let repo = registered_control_fixture(&scratch);
        let tickets = repo.join("projects/XNAUT/tickets");
        std::fs::write(
            tickets.join("XNAUT-900.json"),
            serde_json::to_vec(&ticket()).unwrap(),
        )
        .unwrap();
        fixture_git(&repo, &["add", "projects/XNAUT/tickets"]);
        fixture_git(&repo, &["commit", "-m", "Record assignment"]);
        std::fs::remove_dir_all(&tickets).unwrap();
        let snapshot = snapshot_in(&repo, &scratch.0.join("registry"), "XNAUT", 2_000).unwrap();
        assert!(snapshot
            .diagnostics
            .iter()
            .any(|d| d.blocking && d.source == tickets.display().to_string()));
        assert!(!tickets.exists());
    }
}
