//! Durable approved groups; the existing sweep refills their exact queued membership.

use crate::project_management::TicketRecord;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A ticket worth putting an agent on. `review`, `done` and `complete` are
/// somebody else's turn, and a swarm that re-dispatched them would undo work.
pub const OPEN_STATUSES: [&str; 3] = ["inbox", "ready", "in_progress"];

pub fn is_open(status: &str) -> bool {
    OPEN_STATUSES.contains(&status.trim())
}

/// What a profile that has never been set gets.
pub const DEFAULT_MAX_PARALLEL: usize = 3;

/// The ceiling on one swarm however high the profile is set.
///
/// A fork-bomb guard, not a tested limit: each slot is one worktree and one
/// agent process, so the machine is the real constraint. It is 64 rather than
/// something tidier because the scale test (XNAUT-185) asks for thirty
/// concurrent agents, and a cap of 20 put that test out of reach by arithmetic
/// before a single agent was spawned.
pub const HARD_CAP: usize = 64;

/// One ticket, and the run it would become.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedRun {
    pub ticket: String,
    pub title: String,
    /// The agent handle the ticket is assigned to. Dispatch reads this off the
    /// ticket too; the plan only shows it so the card names who is going.
    pub owner: String,
    /// Effective model shown to the owner and frozen by approval.
    pub model: String,
    #[serde(default)]
    pub cloud_model: Option<crate::cloud_model::Pin>,
    pub branch: String,
    pub scope: String,
    #[serde(default)]
    pub repository_root: Option<String>,
    #[serde(default)]
    pub runtime_id: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
    /// An explicit per-plan destination takes precedence over the profile default.
    /// Older plans keep checking their resolved profile destination.
    #[serde(default)]
    pub requested_environment: Option<String>,
    #[serde(default)]
    pub repository_remote: Option<String>,
}

/// A ticket that was asked for and is not in the plan, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub ticket: String,
    pub reason: String,
}

/// A batch of runs, validated, waiting for a yes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmPlan {
    pub id: String,
    pub project: String,
    pub runs: Vec<PlannedRun>,
    pub skipped: Vec<Skipped>,
    /// The cap this plan was built under, after clamping.
    pub max_parallel: usize,
    pub created_at: i64,
    #[serde(default)]
    pub dispatch_hold: Option<String>,
}

/// What the plan is built against: the board, who can run, and what is
/// already running. Passed in rather than read here so the rules can be
/// tested without a control repo, a registry or an app.
pub struct Board<'a> {
    pub tickets: &'a [TicketRecord],
    /// Agent handle → the model that handle's profile launches with. A handle
    /// missing from this map has no profile, and dispatch would refuse it.
    pub models: &'a HashMap<String, String>,
    /// Ticket ids that already have a run nobody has retired.
    pub live: &'a HashSet<String>,
}

fn owner_of(ticket: &TicketRecord) -> Option<String> {
    ticket
        .owner
        .as_deref()
        .map(|owner| owner.trim().trim_start_matches('@').to_ascii_lowercase())
        .filter(|owner| !owner.is_empty())
}

/// Build a plan for `project`.
///
/// `requested` empty means "every open ticket this project has". Otherwise it
/// is exactly the ids the owner named, in the order they named them, and an id
/// the board does not have is reported rather than invented.
pub fn plan_from(
    id: &str,
    project: &str,
    requested: &[String],
    board: &Board<'_>,
    max_parallel: usize,
    now: i64,
) -> Result<SwarmPlan, String> {
    let project = project.trim();
    if project.is_empty() {
        return Err("a swarm is planned for one project; name it".into());
    }
    let cap = max_parallel.clamp(1, HARD_CAP);

    let mut wanted: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut skipped: Vec<Skipped> = Vec::new();
    if requested.is_empty() {
        for ticket in board
            .tickets
            .iter()
            .filter(|t| t.project == project && is_open(&t.status))
        {
            if seen.insert(ticket.id.clone()) {
                wanted.push(ticket.id.clone());
            }
        }
    } else {
        for raw in requested {
            let id = raw.trim().to_ascii_uppercase();
            if id.is_empty() {
                continue;
            }
            if !seen.insert(id.clone()) {
                skipped.push(Skipped {
                    ticket: id,
                    reason: "listed twice".into(),
                });
                continue;
            }
            wanted.push(id);
        }
    }

    let mut runs: Vec<PlannedRun> = Vec::new();
    for id in wanted {
        let Some(ticket) = board.tickets.iter().find(|t| t.id == id) else {
            skipped.push(Skipped {
                ticket: id,
                reason: "not a ticket the PM has".into(),
            });
            continue;
        };
        if ticket.project != project {
            let reason = format!("belongs to {}, not {project}", ticket.project);
            skipped.push(Skipped { ticket: id, reason });
            continue;
        }
        if !is_open(&ticket.status) {
            let reason = format!("status is {}", ticket.status);
            skipped.push(Skipped { ticket: id, reason });
            continue;
        }
        let Some(owner) = owner_of(ticket) else {
            skipped.push(Skipped {
                ticket: id,
                reason: "no owner to dispatch it to".into(),
            });
            continue;
        };
        let Some(model) = board.models.get(&owner) else {
            let reason = format!("@{owner} has no agent profile");
            skipped.push(Skipped { ticket: id, reason });
            continue;
        };
        if board.live.contains(&ticket.id) {
            skipped.push(Skipped {
                ticket: id,
                reason: "already has a live run".into(),
            });
            continue;
        }
        if ticket.approval.owner_only || ticket.tags.iter().any(|t| t == "no-auto-dispatch") {
            skipped.push(Skipped {
                ticket: id,
                reason: "owner-only or no-auto-dispatch ticket".into(),
            });
            continue;
        }
        runs.push(PlannedRun {
            branch: crate::dispatch::branch_for_ticket(ticket, &owner),
            ticket: ticket.id.clone(),
            title: ticket.title.clone(),
            owner,
            model: model.clone(),
            cloud_model: None,
            scope: scope(ticket),
            repository_root: None,
            runtime_id: None,
            environment: None,
            requested_environment: None,
            repository_remote: None,
        });
    }

    // One worktree per run is what GitVM requires and what the pane got right
    // by accident. Here it holds by construction — one ticket, one branch —
    // and is checked anyway, because "by construction" is a claim a test
    // should be able to fail.
    let mut branches: HashSet<&str> = HashSet::new();
    if let Some(clash) = runs
        .iter()
        .find(|run| !branches.insert(run.branch.as_str()))
    {
        return Err(format!("two runs would share the branch {}", clash.branch));
    }

    Ok(SwarmPlan {
        id: id.to_string(),
        project: project.to_string(),
        runs,
        skipped,
        max_parallel: cap,
        created_at: now,
        dispatch_hold: None,
    })
}

/// What a freshly built plan is worth asking about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Offer {
    /// Nothing here can run. The skips are the answer; there is no question.
    Nothing,
    /// One runnable ticket, which is a dispatch, not a swarm. This is the
    /// acceptance criterion "a single-ticket request gets no swarm question",
    /// and it holds whether the owner named one ticket or the project simply
    /// has one open: a plan that is not offered raises no card, because the
    /// card is raised by the plan travelling to the UI.
    Single(String),
    /// More than one run: show it and wait.
    Swarm,
}

pub fn offer(plan: &SwarmPlan) -> Offer {
    match plan.runs.len() {
        0 => Offer::Nothing,
        1 => Offer::Single(plan.runs[0].ticket.clone()),
        _ => Offer::Swarm,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemberState {
    Queued,
    Starting,
    Tracking,
    Blocked,
    Verified,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Member {
    pub ticket: String,
    pub state: MemberState,
    pub reason: String,
    pub run_id: Option<String>,
    pub started: Option<Started>,
    #[serde(default)]
    pub refusal: Option<crate::dispatch::DispatchRefusal>,
    #[serde(default)]
    pub dispatched_scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupEvent {
    pub id: String,
    pub project: String,
    pub ticket: String,
    pub actor: String,
    pub source: String,
    pub at_ms: i64,
    pub state: MemberState,
    pub reason: String,
    pub run_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub plan: SwarmPlan,
    pub approved_at: Option<i64>,
    #[serde(default)]
    pub requires_findings_triage: bool,
    #[serde(default)]
    pub stopped_at: Option<i64>,
    pub members: Vec<Member>,
    pub events: Vec<GroupEvent>,
}

// Full substantive input, including the full body: a dispatch-like heading is
// not proof that later prose is authorized. Only native dispatch may store the
// exact scope resulting from its own revision-checked evidence append.
pub(crate) fn scope(ticket: &TicketRecord) -> String {
    serde_json::json!({"title":ticket.title,"type":ticket.ticket_type,"body":ticket.body,
        "documentation":ticket.documentation,"model_requirement":ticket.model_requirement,
        "tags":ticket.tags})
    .to_string()
}

pub fn new_id() -> String {
    format!("swarm-{}", uuid::Uuid::new_v4().simple())
}

fn path_in(registry: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("invalid swarm plan id".into());
    }
    Ok(registry.join("swarm-plans").join(format!("{id}.json")))
}

fn save_in(registry: &Path, group: &Group) -> Result<(), String> {
    persist_group_in(registry, group, false)
}

fn persist_group_in(registry: &Path, group: &Group, create_only: bool) -> Result<(), String> {
    use std::io::Write;
    let path = path_in(registry, &group.plan.id)?;
    std::fs::create_dir_all(path.parent().ok_or("missing group directory")?)
        .map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true)
            .open(&tmp).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(group).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if create_only {
            // Publish complete JSON atomically, without replacing an existing
            // approval. A preview cannot dispatch; it need not take the lease
            // held across the coordinator's network calls. create_new on the
            // final file would expose partial JSON to concurrent readers.
            std::fs::hard_link(&tmp, &path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    "plan id already exists; approval cannot be replaced".into()
                } else { e.to_string() }
            })
        } else {
            std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
        }
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

pub(crate) fn groups_in(registry: &Path, project: Option<&str>) -> Result<Vec<Group>, String> {
    let dir = registry.join("swarm-plans");
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut groups = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let group: Group = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if project.is_none_or(|p| p == group.plan.project) {
            groups.push(group);
        }
    }
    groups.sort_by_key(|g| (g.plan.created_at, g.plan.id.clone()));
    Ok(groups)
}

// Dispatch and native admission must use the same approval, including when
// several persisted plans overlap. A stopped newest approval still supersedes
// older approvals; an unapproved preview does not. Break timestamp ties in the
// same order as the persisted group listing.
fn latest_approved_group<'a>(groups: &'a [Group], ticket: &str) -> Option<&'a Group> {
    groups
        .iter()
        .filter(|g| g.approved_at.is_some() && g.members.iter().any(|m| m.ticket == ticket))
        .max_by_key(|g| (g.approved_at, g.plan.created_at, g.plan.id.as_str()))
}

fn remember_in(registry: &Path, plan: SwarmPlan) -> Result<(), String> {
    let members = plan
        .runs
        .iter()
        .map(|r| Member {
            ticket: r.ticket.clone(),
            state: MemberState::Queued,
            reason: "waiting for approval".into(),
            run_id: None,
            started: None,
            refusal: None,
            dispatched_scope: None,
        })
        .collect();
    persist_group_in(
        registry,
        &Group {
            plan,
            approved_at: None,
            requires_findings_triage: false,
            stopped_at: None,
            members,
            events: Vec::new(),
        },
        true,
    )
}

pub fn remember(plan: SwarmPlan) -> Result<(), String> {
    remember_in(&crate::agents::registry_dir()?, plan)
}

fn transition(
    group: &mut Group,
    index: usize,
    state: MemberState,
    reason: String,
    run_id: Option<String>,
    at_ms: i64,
) {
    let member = &mut group.members[index];
    if member.state == state && member.reason == reason && member.run_id == run_id {
        return;
    }
    member.state = state.clone();
    member.reason = reason.clone();
    member.run_id = run_id.clone();
    group.events.push(GroupEvent {
        id: uuid::Uuid::new_v4().to_string(),
        project: group.plan.project.clone(),
        ticket: member.ticket.clone(),
        actor: "nautbot".into(),
        source: format!("swarm-plans/{}.json", group.plan.id),
        at_ms,
        state,
        reason,
        run_id,
    });
}

fn approve(group: &mut Group, at_ms: i64) {
    if group.approved_at.is_some() || group.stopped_at.is_some() {
        return;
    }
    group.approved_at = Some(at_ms);
    for i in 0..group.members.len() {
        transition(
            group,
            i,
            MemberState::Queued,
            "approved; waiting for capacity".into(),
            None,
            at_ms,
        );
    }
}

pub(crate) fn authorized(run: &PlannedRun, ticket: &TicketRecord) -> bool {
    owner_of(ticket).as_deref() == Some(run.owner.as_str())
        && scope(ticket) == run.scope
        && !ticket.approval.owner_only
        && !ticket.tags.iter().any(|t| t == "no-auto-dispatch")
}

fn authorized_member(
    registry: &Path,
    run: &PlannedRun,
    member: &Member,
    ticket: &TicketRecord,
) -> bool {
    let mut expected = run.clone();
    if let Some(scope) = &member.dispatched_scope {
        expected.scope = scope.clone();
    }
    let run = &expected;
    if authorized(run, ticket) {
        return true;
    }
    if owner_of(ticket).as_deref() != Some("nautbot") {
        return false;
    }
    let mut author_ticket = ticket.clone();
    author_ticket.owner = Some(run.owner.clone());
    if !authorized(run, &author_ticket) {
        return false;
    }
    let Some(handback) = &ticket.handback else {
        return false;
    };
    if handback
        .from
        .trim()
        .trim_start_matches('@')
        .to_ascii_lowercase()
        != run.owner
    {
        return false;
    }
    let Some(mut id) = handback.run_id.clone() else {
        return false;
    };
    let mut seen = HashSet::new();
    while seen.insert(id.clone()) {
        let Ok(manifest) = crate::run_control::load_manifest_in(registry, &id) else {
            return false;
        };
        if manifest.ticket.as_deref() != Some(run.ticket.as_str())
            || manifest.agent_handle != run.owner
        {
            return false;
        }
        if member.run_id.as_deref() == Some(id.as_str())
            || member.started.as_ref().is_some_and(|s| {
                manifest.branch == s.branch
                    && manifest.worktree_path == s.worktree_path
                    && (manifest.pty_session.as_deref() == Some(s.session_id.as_str())
                        || manifest.zellij_session.as_deref() == Some(s.session_id.as_str()))
            })
        {
            return true;
        }
        let Some(previous) = manifest.previous_run_id.as_deref() else {
            return false;
        };
        let Ok(prior) = crate::run_control::load_manifest_in(registry, previous) else {
            return false;
        };
        if prior.next_run_id.as_deref() != Some(manifest.run_id.as_str())
            || prior.project != manifest.project
            || prior.ticket != manifest.ticket
            || prior.branch != manifest.branch
            || prior.worktree_path != manifest.worktree_path
            || prior.agent_handle != manifest.agent_handle
        {
            return false;
        }
        id = previous.to_owned();
    }
    false
}

/// Shared by refill and every repair/reviewer admission. Legacy approvals
/// without runtime/environment pins require renewed approval, never guessing.
fn current_pins(run: &PlannedRun, project: &str) -> Result<bool, String> {
    let repo = crate::project_management::repo_now()?;
    let projects = crate::project_management::list_projects(&repo)?;
    let Some(p) = projects.iter().find(|p| p.key == project) else {
        return Ok(false);
    };
    let profile = crate::agent_profiles::agent_profile_get(run.owner.clone())?;
    let root = Path::new(crate::project_management::local_source_path(p).trim())
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let settings = crate::settings::load_or_default();
    let environment = resolve_environment(run.requested_environment.as_deref(), profile.execution, &settings.sandboxes)?;
    let (profile, cloud) = crate::cloud_model::apply(&settings, &profile, environment.key())?;
    Ok(!p.owner_only
        && pins_match(
            run,
            root.to_str(),
            &p.forge_remote,
            &profile.model,
            &profile.runtime_id,
            environment.key(),
            cloud.as_ref().map(|c| &c.pin),
        ))
}
fn pins_match(
    run: &PlannedRun,
    root: Option<&str>,
    remote: &str,
    model: &str,
    runtime: &str,
    environment: &str,
    cloud: Option<&crate::cloud_model::Pin>,
) -> bool {
    run.cloud_model.as_ref() == cloud
        && run.repository_root.as_deref() == root
        && root.is_some()
        && run.repository_remote.as_deref() == Some(remote)
        && run.model == model
        && run.runtime_id.as_deref() == Some(runtime)
        && run.environment.as_deref() == Some(environment)
}

struct WorkerGroupScope {
    tickets: HashSet<String>,
    review_worktrees: HashSet<String>,
}
impl WorkerGroupScope {
    fn new(group: &Group, transfers: &[crate::repository_transfer::Transfer]) -> Self {
        let tickets: HashSet<_> = group.members.iter().map(|m| m.ticket.clone()).collect();
        let parents: HashSet<_> = transfers
            .iter()
            .filter(|t| t.ticket.as_ref().is_some_and(|id| tickets.contains(id)))
            .map(|t| t.run_id.as_str())
            .collect();
        let mut review_worktrees = HashSet::new();
        for transfer in transfers {
            if parents.contains(transfer.run_id.as_str()) {
                if let Some(q) = &transfer.quality {
                    if !q.worktree.is_empty() {
                        review_worktrees.insert(q.worktree.clone());
                    }
                }
            }
            if transfer
                .review_parent
                .as_deref()
                .is_some_and(|id| parents.contains(id))
            {
                review_worktrees.insert(transfer.local_path.clone());
            }
        }
        Self {
            tickets,
            review_worktrees,
        }
    }
    fn contains(&self, run: &crate::run_control::RunManifest) -> bool {
        run.ticket
            .as_ref()
            .is_some_and(|ticket| self.tickets.contains(ticket))
            || self.review_worktrees.contains(&run.worktree_path)
    }
    fn count(&self, registry: &Path) -> Result<usize, String> {
        let mut count = 0;
        for id in crate::run_control::list_ids_in(registry)? {
            let run = crate::run_control::load_manifest_in(registry, &id)?;
            if crate::run_control::consumes_worker_capacity(&run) && self.contains(&run) {
                count += 1;
            }
        }
        Ok(count)
    }
    fn count_for_member(&self, registry: &Path, member: &Member) -> Result<usize, String> {
        let mut count = self.count(registry)?;
        if let Some(id) = &member.run_id {
            let run = crate::run_control::load_manifest_in(registry,id)?;
            if run.state == crate::run_control::RunState::Requested && run.previous_run_id.is_some()
                && run.ticket.as_deref() == Some(member.ticket.as_str()) && self.contains(&run) {
                // Its reserved slot is being consumed, not a second slot requested.
                count = count.saturating_sub(1);
            }
        }
        Ok(count)
    }
}

/// Called while native StoreLock is held, before a Requested worker becomes
/// admitted or a repair successor consumes a slot. Includes ticketless reviewers.
pub(crate) fn worker_admission_in(
    registry: &Path,
    run: &crate::run_control::RunManifest,
    replacing: Option<&str>,
) -> Result<(), String> {
    worker_admission_for_ticket_in(registry, run, replacing, None)
}
pub(crate) fn worker_admission_for_ticket_in(
    registry: &Path,
    run: &crate::run_control::RunManifest,
    replacing: Option<&str>,
    review_ticket: Option<&str>,
) -> Result<(), String> {
    crate::agent_work::admit_model_reservation_in(registry,run)?;
    let groups = groups_in(registry, None)?;
    let mut ticket = run
        .ticket
        .clone()
        .or_else(|| review_ticket.map(str::to_owned));
    if ticket.is_none() {
        if let Some(parent) = crate::repository_transfer::list()?.iter().find(|t| {
            t.quality
                .as_ref()
                .is_some_and(|q| q.worktree == run.worktree_path && q.reviewer == run.agent_handle)
        }) {
            ticket = parent.ticket.clone();
        }
    }
    let global_cap = crate::spend::load_ceiling().max_concurrent as usize;
    let mut group_capacity = None;
    if let Some(ticket) = ticket {
        if let Some(group) = latest_approved_group(&groups, &ticket) {
            if group.stopped_at.is_some() {
                return Err("Approved group was stopped; worker admission refused".into());
            }
            let (planned, member) = group
                .plan
                .runs
                .iter()
                .zip(&group.members)
                .find(|(p, _)| p.ticket == ticket)
                .ok_or("Group membership is inconsistent")?;
            let repo = crate::project_management::repo_now()?;
            let current =
                crate::project_management::ticket_list_in(&repo, Some(group.plan.project.clone()))?
                    .into_iter()
                    .find(|t| t.id == ticket)
                    .ok_or("Group ticket unavailable")?;
            if !authorized_member(registry, planned, member, &current)
                || !current_pins(planned, &group.plan.project)?
            {
                return Err(
                    "Approved group owner, scope, repository, runtime or environment changed"
                        .into(),
                );
            }
            if run.ticket.is_some()
                && (run.agent_handle != planned.owner
                    || run.runtime_id != planned.runtime_id.as_deref().unwrap_or("")
                    || run.model.as_deref().unwrap_or("") != planned.model
                    || run.cloud_model != planned.cloud_model
                    || run.remote_env.as_deref().unwrap_or("local")
                        != planned.environment.as_deref().unwrap_or(""))
            {
                return Err("Native author identity differs from the approved group".into());
            }
            let profile_cap = crate::agent_profiles::agent_profile_get(
                crate::agent_profiles::RESERVED_NAUTBOT_HANDLE.into(),
            )?
            .max_parallel as usize;
            group_capacity = Some((
                group.plan.max_parallel.min(profile_cap.clamp(1, HARD_CAP)),
                WorkerGroupScope::new(group, &crate::repository_transfer::list()?),
            ));
        }
    }
    // During repair reservation, the proven-stopped predecessor may itself
    // be a failed continuation. Its ordinary launch guard intentionally refuses
    // reuse; this callback is reserving a NEW successor under positive proof.
    let pending = if replacing.is_some() {
        None
    } else {
        run.ticket
            .as_deref()
            .map(|ticket| crate::run_control::continuation_in(registry, ticket))
            .transpose()?
            .flatten()
    };
    let own = pending
        .as_ref()
        .filter(|p| {
            p.worktree_path == run.worktree_path
                && p.branch == run.branch
                && p.state == crate::run_control::RunState::Requested
        })
        .map(|p| p.run_id.as_str())
        .unwrap_or(&run.run_id);
    crate::run_control::worker_capacity_in(registry, global_cap, own, replacing)?;
    if let Some((cap, scope)) = group_capacity {
        crate::run_control::worker_capacity_matching_in(registry, cap, own, replacing, |run| {
            scope.contains(run)
        })?;
    }
    Ok(())
}

pub(crate) fn authorizes_ticket_repair(
    project: &str,
    ticket: &str,
    owner: &str,
) -> Result<bool, String> {
    let repo = crate::project_management::repo_now()?;
    let registry = crate::agents::registry_dir()?;
    let tickets = crate::project_management::ticket_list_in(&repo, Some(project.to_string()))?;
    let Some(current) = tickets.iter().find(|t| t.id == ticket) else {
        return Ok(false);
    };
    let groups = groups_in(&registry, Some(project))?;
    let Some(group) = groups
        .iter()
        .filter(|g| g.approved_at.is_some() && g.members.iter().any(|m| m.ticket == ticket))
        .max_by_key(|g| g.approved_at)
    else {
        return Ok(false);
    };
    if group.stopped_at.is_some() {
        return Ok(false);
    }
    for (run, member) in group.plan.runs.iter().zip(&group.members) {
        if run.ticket == ticket
            && run.owner == owner
            && authorized_member(&registry, run, member, current)
            && current_pins(run, project)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

// One filesystem lease across confirms, the sweep, and other app instances. A crashed
// holder releases the OS lock; persisted Starting entries still require reconciliation.
struct GroupLease(std::fs::File);
impl GroupLease {
    fn acquire(registry: &Path) -> Result<Self, String> {
        let dir = registry.join("swarm-plans");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(".lock"))
            .map_err(|e| e.to_string())?;
        match file.try_lock() {
            Ok(()) => {},
            Err(std::fs::TryLockError::WouldBlock) =>
                return Err("group coordinator is already advancing; retry next sweep".into()),
            Err(std::fs::TryLockError::Error(error)) => return Err(error.to_string()),
        }
        Ok(Self(file))
    }
}
impl Drop for GroupLease {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// Read the board, the profiles and the registry, and build a plan under
/// NautBot's cap. Nothing is stored: the caller decides whether this plan is
/// worth asking about.
pub fn build(project: &str, requested: &[String]) -> Result<SwarmPlan, String> {
    build_with_environment(project, requested, None)
}

// Replanning retains the owner's last approved destination even when the
// model omits it. An unapproved preview cannot change that intent.
fn replan_environment<'a>(explicit: Option<&'a str>, groups: &'a [Group], ticket: &str) -> Option<&'a str> {
    explicit.or_else(|| latest_approved_group(groups, ticket)
        .and_then(|group| group.plan.runs.iter().find(|run| run.ticket == ticket))
        .and_then(|run| run.environment.as_deref()))
}

fn needs_repository(run: &PlannedRun) -> bool {
    matches!(run.environment.as_deref(), Some("exe-dev" | "gitvm"))
}

fn resolve_environment(
    requested: Option<&str>,
    execution: crate::agent_profiles::AgentExecution,
    sandboxes: &[crate::settings::SandboxProviderSettings],
) -> Result<crate::sandbox::launch_env::LaunchEnv, String> {
    use crate::sandbox::launch_env::{resolve, LaunchEnv};
    let explicit = requested.map(|key| LaunchEnv::from_key(key)
        .ok_or_else(|| format!("Unknown execution environment: {key}. Choose local, exe-dev or gitvm.")))
        .transpose()?;
    let destination = resolve(explicit.or_else(|| execution.pinned_environment()), sandboxes);
    // A requested remote destination must never silently fall back to local.
    destination.route(sandboxes)?;
    Ok(destination)
}

fn ensure_dispatch_role(role: crate::instance::Role, plan: &SwarmPlan) -> Result<(), String> {
    for run in &plan.runs {
        crate::dispatch::approved_dispatch_policy(role, false, run.environment.as_deref())
            .map_err(|refusal| format!("{}: {}", run.ticket, refusal.reason))?;
    }
    Ok(())
}

pub fn build_with_environment(
    project: &str,
    requested: &[String],
    environment: Option<&str>,
) -> Result<SwarmPlan, String> {
    let repo = crate::project_management::repo_now()?;
    let tickets = crate::project_management::ticket_list_in(&repo, None)?;
    let registry = crate::agents::registry_dir()?;
    let live = crate::run_control::live_tickets_in(&registry)?;

    let mut models: HashMap<String, String> = HashMap::new();
    for owner in tickets.iter().filter_map(owner_of) {
        if models.contains_key(&owner) {
            continue;
        }
        if let Ok(profile) = crate::agent_profiles::agent_profile_get(owner.clone()) {
            models.insert(owner, profile.model);
        }
    }

    let cap = crate::agent_profiles::agent_profile_get(
        crate::agent_profiles::RESERVED_NAUTBOT_HANDLE.to_string(),
    )
    .map(|profile| profile.max_parallel as usize)
    .unwrap_or(DEFAULT_MAX_PARALLEL);

    let mut plan = plan_from(
        &new_id(),
        project,
        requested,
        &Board {
            tickets: &tickets,
            models: &models,
            live: &live,
        },
        cap,
        crate::run_control::now_ms(),
    )?;
    let projects = crate::project_management::list_projects(&repo)?;
    let source = projects
        .iter()
        .find(|p| p.key == project)
        .map(crate::project_management::local_source_path)
        .filter(|path| !path.trim().is_empty())
        .ok_or("project has no local repository")?;
    let root = Path::new(&source)
        .canonicalize()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string();
    let settings = crate::settings::load_or_default();
    let previous = groups_in(&registry, Some(project))?;
    for run in &mut plan.runs {
        let profile = crate::agent_profiles::agent_profile_get(run.owner.clone())?;
        run.repository_root = Some(root.clone());
        run.runtime_id = Some(profile.runtime_id.clone());
        let destination = replan_environment(environment, &previous, &run.ticket);
        run.environment = Some(resolve_environment(destination, profile.execution, &settings.sandboxes)?.key().into());
        let (profile, cloud) = crate::cloud_model::apply(&settings, &profile, run.environment.as_deref().unwrap_or("local"))?;
        run.model = profile.model.clone();
        run.cloud_model = cloud.as_ref().map(|c| c.pin.clone());
        if needs_repository(run) {
            let ticket = tickets.iter().find(|ticket| ticket.id == run.ticket)
                .ok_or("Planned ticket disappeared")?;
            crate::cloud_model::validate_repository_profile(&profile, &ticket.model_requirement)?;
            crate::agent_profiles::validate_remote_runtime_with(&profile, cloud.as_ref())?;
        }
        run.requested_environment = destination.map(str::to_owned);
        run.repository_remote = projects
            .iter()
            .find(|p| p.key == project)
            .map(|p| p.forge_remote.clone());
    }
    if let Some(run) = plan.runs.iter().find(|run| needs_repository(run)) {
        crate::repository_transfer::preflight(Path::new(&root),
            run.repository_remote.as_deref().unwrap_or_default())?;
    }
    plan.dispatch_hold = ensure_dispatch_role(crate::instance::role(), &plan).err();
    Ok(plan)
}

/// One run a confirmed swarm actually started.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Started {
    pub ticket: String,
    pub handle: String,
    pub branch: String,
    pub worktree_path: String,
    pub session_id: String,
}

/// What a confirm did. `failed` is not an error: the runs before it are
/// already working, and a caller that saw only an error would not know that.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SwarmDispatched {
    pub plan_id: String,
    pub project: String,
    pub started: Vec<Started>,
    pub failed: Vec<Skipped>,
    pub queued: Vec<String>,
    pub total: usize,
    pub members: Vec<Member>,
}

impl SwarmDispatched {
    pub(crate) fn tool_result(&self) -> serde_json::Value {
        serde_json::json!({
            "ok": true,
            "plan_id": self.plan_id,
            "project": self.project,
            "started": self.started,
            "queued": self.queued,
            "failed": self.failed,
            "total": self.total,
            "members": self.members,
            "note": format!(
                "Recorded launches {}; queued {}; blocked {}; {} total planned for {}. Recorded launches are cumulative for this plan, not new workers from this call or the number still running. Queued members retain their approval and await refill; this plan is durable, not consumed. Inspect member states and reasons before recovery. Do not create a replacement merely because zero workers started, and do not claim queued work is running or completed.",
                self.started.len(), self.queued.len(), self.failed.len(), self.total, self.project
            )
        })
    }
}

fn report(group: &Group) -> SwarmDispatched {
    SwarmDispatched {
        plan_id: group.plan.id.clone(),
        project: group.plan.project.clone(),
        total: group.plan.runs.len(),
        members: group.members.clone(),
        started: group
            .members
            .iter()
            .filter_map(|m| m.started.clone())
            .collect(),
        failed: group
            .members
            .iter()
            .filter(|m| m.state == MemberState::Blocked)
            .map(|m| Skipped {
                ticket: m.ticket.clone(),
                reason: m.reason.clone(),
            })
            .collect(),
        queued: group
            .members
            .iter()
            .filter(|m| m.state == MemberState::Queued)
            .map(|m| m.ticket.clone())
            .collect(),
    }
}

pub async fn dispatch_plan(
    app: tauri::AppHandle,
    plan_id: &str,
) -> Result<SwarmDispatched, String> {
    dispatch_plan_from(app,plan_id,false).await
}

pub(crate) async fn dispatch_model_plan(app: tauri::AppHandle, plan_id: &str) -> Result<SwarmDispatched,String> {
    dispatch_plan_from(app,plan_id,true).await
}

fn approve_from(group: &mut Group, model_origin: bool, now: i64,
    mut admit: impl FnMut(&str,&str)->Result<(),String>) -> Result<(),String> {
    if model_origin {
        // Validate saved membership, never a subset supplied by a model.
        // Failure does not create partial approval.
        for run in &group.plan.runs { admit(&group.plan.project,&run.ticket)?; }
        if group.approved_at.is_none() { group.requires_findings_triage = true; }
    } else {
        // Only the actual owner UI confirmation enters this path.
        if group.requires_findings_triage {
            for member in &group.members {
                group.events.push(GroupEvent { id:uuid::Uuid::new_v4().to_string(),
                    project:group.plan.project.clone(),ticket:member.ticket.clone(),actor:"owner".into(),
                    source:format!("swarm-plans/{}.json",group.plan.id),at_ms:now,state:member.state.clone(),
                    reason:"Owner explicitly confirmed this group; findings disposition override recorded".into(),run_id:member.run_id.clone() });
            }
        }
        group.requires_findings_triage = false;
    }
    approve(group,now);
    Ok(())
}

pub(crate) fn model_group_requires_triage(project: &str, ticket: &str) -> Result<bool,String> {
    Ok(groups_in(&crate::agents::registry_dir()?,Some(project))?.iter()
        .filter(|g|g.approved_at.is_some() && g.stopped_at.is_none() && g.members.iter().any(|m|m.ticket == ticket))
        .max_by_key(|g|g.approved_at).is_some_and(|g|g.requires_findings_triage))
}

async fn dispatch_plan_from(app: tauri::AppHandle, plan_id: &str, model_origin: bool) -> Result<SwarmDispatched,String> {
    let registry = crate::agents::registry_dir()?;
    confirm_in(&registry, plan_id, model_origin, crate::instance::role(),
        crate::switches::load().read_only, crate::ticket_triage::admit_current_ticket)?;
    refill(&app).await?;
    groups_in(&registry, None)?
        .iter()
        .find(|g| g.plan.id == plan_id)
        .map(report)
        .ok_or_else(|| "plan disappeared".into())
}

/// The card and model tool share this exact persisted approval boundary.
fn confirm_in(
    registry: &Path, plan_id: &str, model_origin: bool, role: crate::instance::Role,
    read_only: bool, admit: impl FnMut(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
    if read_only {
        return Err("the read_only kill-switch is engaged".into());
    }
    let _lease = GroupLease::acquire(registry)?;
    let path = path_in(registry, plan_id)?;
    let mut group: Group = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if group.stopped_at.is_some() {
        return Err("group was stopped; a new exact scope approval is required".into());
    }
    ensure_dispatch_role(role, &group.plan)?;
    approve_from(&mut group, model_origin, crate::run_control::now_ms(), admit)?;
    save_in(registry, &group)
}

/// Returns every approved membership, including blocked members: ordinary fleet
/// dispatch must never route around an approved group's refusal or capacity limit.
pub(crate) fn managed_tickets(registry: &Path) -> Result<HashSet<String>, String> {
    Ok(groups_in(registry, None)?
        .iter()
        .filter(|g| g.approved_at.is_some())
        .flat_map(|g| g.members.iter().map(|m| m.ticket.clone()))
        .collect())
}

fn waits_for_preflight(member: &Member) -> bool {
    member.state == MemberState::Blocked && member.refusal.as_ref()
        .is_some_and(|r| matches!(r.kind, crate::dispatch::RefusalKind::RepositoryAccess | crate::dispatch::RefusalKind::RuntimeReadiness))
}

fn slot_available(member: &Member, live: usize, cap: usize) -> bool {
    (member.state == MemberState::Queued || waits_for_preflight(member)) && live < cap
}

fn recover_member(
    registry: &Path,
    group: &mut Group,
    i: usize,
    snapshot: &crate::project_continuity::ProjectSnapshot,
    now: i64,
) -> bool {
    let ticket = &group.members[i].ticket;
    let assignments: Vec<_> = snapshot
        .assignments
        .iter()
        .filter(|a| a.ticket.as_deref() == Some(ticket.as_str()))
        .collect();
    if !assignments.is_empty() {
        if group.members[i].started.is_none() {
            let continuation = match crate::run_control::continuation_in(registry, ticket) {
                Ok(continuation) => continuation,
                Err(error) => {
                    let retained_id = group.members[i].run_id.clone()
                        .or_else(|| assignments.last().map(|a| a.run_id.clone()));
                    transition(group, i, MemberState::Blocked,
                        format!("continuation recovery refused: {error}"), retained_id, now);
                    return true;
                }
            };
            if let Some(next) = continuation {
                if next.state == crate::run_control::RunState::Requested
                    && next.previous_run_id.as_ref().is_some_and(|id| next.last_signal ==
                        format!("stopped failed worker recovered without source changes after {id}")) {
                    let valid_parent = next.previous_run_id.as_ref()
                        .and_then(|id| crate::run_control::load_manifest_in(registry,id).ok())
                        .is_some_and(|p| p.state == crate::run_control::RunState::Retired && p.ticket_returned
                            && p.next_run_id.as_deref() == Some(next.run_id.as_str())
                            && p.retirement.as_ref().is_some_and(|r| r.stopped_at.is_some()));
                    let guard = crate::agent_work::recovery_guard(&serde_json::json!(snapshot),ticket,Some(&next));
                    if !valid_parent || guard.is_err() {
                        transition(group,i,MemberState::Blocked,
                            format!("Stopped-worker continuation is not proven: {}",guard.err().unwrap_or_else(||"predecessor stop proof missing".into())),Some(next.run_id),now);
                        return true;
                    }
                    transition(group,i,MemberState::Queued,
                        "verified stopped-worker continuation is ready for admission".into(),Some(next.run_id),now);
                    return false;
                }
                if next.admission_refused && !crate::run_control::admission_refused_before_execution(&next) {
                    transition(group, i, MemberState::Blocked,
                        "admission refusal conflicts with execution evidence; inspect preserved run before retry".into(),
                        Some(next.run_id), now);
                    return true;
                }
                if crate::run_control::admission_refused_before_execution(&next) {
                    if let Err(error) = crate::agent_work::recovery_guard(
                        &serde_json::json!(snapshot), ticket, Some(&next),
                    ) {
                        transition(group, i, MemberState::Blocked,
                            format!("worker never started; continuation recovery refused: {error}"),
                            Some(next.run_id), now);
                        return true;
                    }
                    let failed_attempts = assignments.iter().filter(|a| {
                        crate::run_control::load_manifest_in(registry, &a.run_id).map_or(true, |run| {
                            run.started_at >= group.approved_at.unwrap_or(i64::MAX)
                                && !crate::run_control::spend_prelaunch_refused(&run)
                        })
                    }).count();
                    if failed_attempts >= 3 {
                        transition(group, i, MemberState::Blocked,
                            "prelaunch retry limit reached; inspect preserved refusal evidence before further dispatch".into(),
                            Some(next.run_id), now);
                        return true;
                    }
                    // Rechecking an unchanged repository outage is not a new
                    // queue transition; keep its original refusal and event trail.
                    if waits_for_preflight(&group.members[i]) { return false; }
                    transition(group, i, MemberState::Queued,
                        "native admission refusal proved no worker started; retry preserved workspace".into(),
                        Some(next.run_id), now);
                    return false;
                }
            }
        }
        let active = assignments
            .iter()
            .find(|a| a.run_state.is_some_and(|s| !s.terminal()));
        let (state, reason) = if active.is_some() {
            (
                MemberState::Tracking,
                "existing assignment; tracking without replacement",
            )
        } else if snapshot.tickets.iter().any(|t| {
            t.id == *ticket && t.state == crate::project_continuity::ContinuityState::Verified
        }) {
            (
                MemberState::Verified,
                "independent verification evidence recorded",
            )
        } else if assignments
            .iter()
            .any(|a| a.state == crate::project_continuity::ContinuityState::Blocked)
        {
            (
                MemberState::Blocked,
                "existing assignment blocked; inspect preserved implementation and review findings",
            )
        } else {
            (
                MemberState::Tracking,
                "implementation retained; awaiting review or repair evidence",
            )
        };
        let id = active
            .copied()
            // A run ID is an identity, not a timestamp. Retained predecessors
            // can sort after their completed successor (legacy IDs especially).
            .or_else(|| assignments.iter().rev().find(|a| a.next_run_id.is_none()).copied())
            .unwrap_or(assignments[assignments.len() - 1])
            .run_id
            .clone();
        transition(group, i, state, reason.into(), Some(id), now);
        return true;
    }
    if group.members[i].state == MemberState::Starting {
        transition(group,i,MemberState::Blocked,"interrupted dispatch has no conclusive assignment evidence; inspect reservation before retry".into(),None,now);
        return true;
    }
    false
}

/// Native services are kept at this seam so restart/error-isolation tests drive
/// the same persisted refill loop without starting a Tauri app or real worker.
trait RefillBackend: Sync {
    fn repository_preflight(&self, run: PlannedRun) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
    fn runtime_preflight(&self, run: PlannedRun) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
    fn reconcile(&self, project: &str, ticket: &str) -> Result<(), String>;
    fn snapshot(&self, project: &str)
        -> Result<crate::project_continuity::ProjectSnapshot, String>;
    fn pins(&self, run: &PlannedRun, project: &str) -> Result<bool, String>;
    fn capacity(&self, registry: &Path, group: &Group, index: usize) -> Result<bool, String>;
    fn admission(
        &self,
        run: &PlannedRun,
        project: &str,
    ) -> Result<(), crate::dispatch::DispatchRefusal>;
    fn dispatch(
        &self,
        run: PlannedRun,
        project: String,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<crate::dispatch::DispatchResult, String>>
                + Send
                + '_,
        >,
    >;
}

struct NativeRefill<'a>(&'a tauri::AppHandle);
impl RefillBackend for NativeRefill<'_> {
    fn runtime_preflight(&self, run: PlannedRun) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let profile = crate::agent_profiles::agent_profile_get(run.owner.clone())?;
                let (profile, cloud) = crate::cloud_model::apply(&crate::settings::load_or_default(), &profile, run.environment.as_deref().unwrap_or("local"))?;
                if run.cloud_model != cloud.as_ref().map(|c| c.pin.clone()) || run.model != profile.model {
                    return Err("Approved cloud model or connection changed; renew approval.".into());
                }
                if run.environment.as_deref() == Some("exe-dev") {
                    crate::agent_profiles::preflight_exe_runtime(&profile, cloud.as_ref())
                } else {
                    crate::agent_profiles::validate_remote_runtime_with(&profile, cloud.as_ref())
                }
            }).await.map_err(|_| "Worker runtime check could not complete")?
        })
    }
    fn repository_preflight(&self, run: PlannedRun) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || crate::repository_transfer::preflight(
                Path::new(run.repository_root.as_deref().ok_or("Approved repository folder missing")?),
                run.repository_remote.as_deref().ok_or("Approved repository URL missing")?,
            )).await.map_err(|_| "Desktop repository check could not complete")?
        })
    }
    fn reconcile(&self, project: &str, ticket: &str) -> Result<(), String> {
        crate::agent_work::reconcile_prelaunch(project, ticket).map(|_| ())
    }
    fn snapshot(
        &self,
        project: &str,
    ) -> Result<crate::project_continuity::ProjectSnapshot, String> {
        crate::project_continuity::snapshot(project)
    }
    fn pins(&self, run: &PlannedRun, project: &str) -> Result<bool, String> {
        current_pins(run, project)
    }
    fn capacity(&self, registry: &Path, group: &Group, index: usize) -> Result<bool, String> {
        let profile_cap = crate::agent_profiles::agent_profile_get(
            crate::agent_profiles::RESERVED_NAUTBOT_HANDLE.into(),
        )
        .map(|p| p.max_parallel as usize)
        .unwrap_or(DEFAULT_MAX_PARALLEL);
        Ok(slot_available(
            &group.members[index],
            WorkerGroupScope::new(group, &crate::repository_transfer::list()?).count_for_member(registry, &group.members[index])?,
            group.plan.max_parallel.min(profile_cap.clamp(1, HARD_CAP)),
        ))
    }
    fn admission(
        &self,
        run: &PlannedRun,
        project: &str,
    ) -> Result<(), crate::dispatch::DispatchRefusal> {
        crate::dispatch::approved_group_admission(&run.ticket, project, run.environment.as_deref())?;
        let profile = crate::agent_profiles::agent_profile_get(run.owner.clone())
            .map_err(crate::dispatch::DispatchRefusal::uncertain)?;
        let (profile, cloud) = crate::cloud_model::apply(&crate::settings::load_or_default(), &profile,
            run.environment.as_deref().unwrap_or("local")).map_err(crate::dispatch::DispatchRefusal::uncertain)?;
        if profile.model != run.model || run.cloud_model != cloud.map(|c| c.pin) {
            return Err(crate::dispatch::DispatchRefusal {
                kind: crate::dispatch::RefusalKind::Policy,
                reason: "approved owner profile/model changed".into(),
            });
        }
        Ok(())
    }
    fn dispatch(
        &self,
        run: PlannedRun,
        project: String,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<crate::dispatch::DispatchResult, String>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            crate::dispatch::dispatch_scoped(
                self.0.clone(),
                run.ticket.clone(),
                project,
                run.requested_environment.clone(),
                Some(&run),
            )
            .await
        })
    }
}

fn block_local(
    registry: &Path,
    group: &mut Group,
    index: usize,
    reason: String,
) -> Result<(), String> {
    let reason = crate::project_wiki::redact(&reason);
    let id = group.members[index].run_id.clone();
    group.members[index].refusal =
        Some(crate::dispatch::DispatchRefusal::uncertain(reason.clone()));
    transition(
        group,
        index,
        MemberState::Blocked,
        reason,
        id,
        crate::run_control::now_ms(),
    );
    save_in(registry, group)
}

pub(crate) async fn refill(app: &tauri::AppHandle) -> Result<(), String> {
    let registry = crate::agents::registry_dir()?;
    let _lease = GroupLease::acquire(&registry)?;
    let repo = crate::project_management::repo_now()?;
    let tickets = crate::project_management::ticket_list_in(&repo, None)?;
    crate::project_management::list_projects(&repo)?; // shared PM integrity, before any dispatch
    refill_in(&registry, &tickets, &NativeRefill(app), crate::instance::role(),
        crate::switches::load().read_only).await
}

async fn refill_in(
    registry: &Path,
    tickets: &[TicketRecord],
    backend: &impl RefillBackend,
    role: crate::instance::Role,
    read_only: bool,
) -> Result<(), String> {
    if read_only || role == crate::instance::Role::Sandbox {
        return Ok(());
    }
    // Corrupt shared run/group registries remain global failures. Do not hide
    // them as a missing member profile and launch around uncertain occupancy.
    crate::run_control::worker_count_in(registry)?;
    let groups = groups_in(registry, None)?;
    if groups.iter().any(|g| {
        g.plan.runs.len() != g.members.len()
            || g.plan
                .runs
                .iter()
                .zip(&g.members)
                .any(|(r, m)| r.ticket != m.ticket)
    }) {
        return Err("Group registry membership is inconsistent; refill refused".into());
    }
    let mut repository_checks: HashMap<_, Result<(), String>> = HashMap::new();
    let mut runtime_checks: HashMap<_, Result<(), String>> = HashMap::new();
    for mut group in groups
        .iter()
        .filter(|g| g.approved_at.is_some() && g.stopped_at.is_none())
        .cloned()
    {
        let mut unavailable = vec![false; group.members.len()];
        for (i, is_unavailable) in unavailable.iter_mut().enumerate() {
            if let Some(latest) = latest_approved_group(&groups, &group.members[i].ticket)
                .filter(|latest| latest.plan.id != group.plan.id)
            {
                let id = group.members[i].run_id.clone();
                group.members[i].refusal = None;
                transition(
                    &mut group, i, MemberState::Blocked,
                    format!("superseded by approved plan {}; existing runs and evidence retained", latest.plan.id),
                    id, crate::run_control::now_ms(),
                );
                save_in(registry, &group)?;
                *is_unavailable = true;
                continue;
            }
            if let Err(error) = backend.reconcile(&group.plan.project, &group.members[i].ticket) {
                let reason = format!(
                    "Prelaunch recovery unavailable for {}: {error}",
                    group.members[i].ticket
                );
                block_local(registry, &mut group, i, reason)?;
                *is_unavailable = true;
            }
        }
        if unavailable.iter().all(|blocked| *blocked) {
            continue;
        }
        let snapshot = match backend.snapshot(&group.plan.project) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                for (i, is_unavailable) in unavailable.iter().enumerate() {
                    if !is_unavailable {
                        let reason = format!(
                            "Project recovery unavailable for {}: {error}",
                            group.plan.project
                        );
                        block_local(registry, &mut group, i, reason)?;
                    }
                }
                continue;
            }
        };
        for (i, is_unavailable) in unavailable.into_iter().enumerate() {
            if is_unavailable {
                continue;
            }
            let run = group.plan.runs[i].clone();
            let now = crate::run_control::now_ms();
            let current = tickets
                .iter()
                .find(|t| t.id == run.ticket && t.project == group.plan.project);
            let pins = match backend.pins(&run, &group.plan.project) {
                Ok(pins) => pins,
                Err(error) => {
                    block_local(registry, &mut group, i,
                        format!("Approved profile or project configuration unavailable for @{} ({}): {error}", run.owner, run.ticket))?;
                    continue;
                }
            };
            if !pins
                || current.is_none_or(|t| !authorized_member(registry, &run, &group.members[i], t))
            {
                block_local(
                    registry,
                    &mut group,
                    i,
                    "approved owner, scope or project policy changed".into(),
                )?;
                continue;
            }
            if recover_member(registry, &mut group, i, &snapshot, now) {
                save_in(registry, &group)?;
                continue;
            }
            if group.members[i].state != MemberState::Queued && !waits_for_preflight(&group.members[i]) {
                continue;
            }
            if let Err(refusal) = crate::dispatch::approved_dispatch_policy(role, read_only, run.environment.as_deref()) {
                group.members[i].refusal = Some(refusal.clone());
                transition(&mut group, i, MemberState::Blocked, refusal.reason, None, now);
                save_in(registry, &group)?;
                continue;
            }
            if current.is_none_or(|t| !is_open(&t.status)) {
                transition(
                    &mut group,
                    i,
                    MemberState::Blocked,
                    "ticket left the approved open queue; inspect current evidence".into(),
                    None,
                    now,
                );
                save_in(registry, &group)?;
                continue;
            }
            if group.requires_findings_triage {
                if let Err(reason) = crate::ticket_triage::dispatch_admission(current.unwrap()) {
                    block_local(registry,&mut group,i,reason)?;
                    continue;
                }
            }
            if !backend.capacity(registry, &group, i)? {
                transition(
                    &mut group,
                    i,
                    MemberState::Queued,
                    "approved; waiting for capacity".into(),
                    None,
                    now,
                );
                save_in(registry, &group)?;
                continue;
            }
            if needs_repository(&run) {
                let key = (group.plan.project.clone(), run.repository_root.clone(), run.repository_remote.clone());
                let check = match repository_checks.get(&key) {
                    Some(result) => result.clone(),
                    None => {
                        let result = backend.repository_preflight(run.clone()).await;
                        repository_checks.insert(key, result.clone());
                        result
                    }
                };
                if let Err(error) = check {
                    let reason = crate::project_wiki::redact(&format!("Repository access must recover before dispatch: {error} No new run was created."));
                    group.members[i].refusal = Some(crate::dispatch::DispatchRefusal {
                        kind: crate::dispatch::RefusalKind::RepositoryAccess, reason: reason.clone(),
                    });
                    let id = group.members[i].run_id.clone();
                    transition(&mut group, i, MemberState::Blocked, reason, id, now);
                    save_in(registry, &group)?;
                    continue;
                }
                let key = (run.owner.clone(), run.environment.clone());
                let check = match runtime_checks.get(&key) {
                    Some(result) => result.clone(),
                    None => {
                        let result = backend.runtime_preflight(run.clone()).await;
                        runtime_checks.insert(key, result.clone());
                        result
                    }
                };
                if let Err(error) = check {
                    let reason = crate::project_wiki::redact(&format!("Worker runtime must be ready before dispatch: {error} No new run was created."));
                    group.members[i].refusal = Some(crate::dispatch::DispatchRefusal {
                        kind: crate::dispatch::RefusalKind::RuntimeReadiness, reason: reason.clone(),
                    });
                    let id = group.members[i].run_id.clone();
                    transition(&mut group, i, MemberState::Blocked, reason, id, now);
                    save_in(registry, &group)?;
                    continue;
                }
            }
            if let Err(refusal) = backend.admission(&run, &group.plan.project) {
                let state = if refusal.retryable() {
                    MemberState::Queued
                } else {
                    MemberState::Blocked
                };
                group.members[i].refusal = Some(refusal.clone());
                transition(&mut group, i, state, refusal.reason, None, now);
                save_in(registry, &group)?;
                continue;
            }
            group.members[i].refusal = None;
            transition(
                &mut group,
                i,
                MemberState::Starting,
                "dispatch reserved by approved group".into(),
                None,
                now,
            );
            save_in(registry, &group)?;
            match backend
                .dispatch(run.clone(), group.plan.project.clone())
                .await
            {
                Ok(r) => {
                    group.members[i].dispatched_scope = Some(r.ticket_scope.clone());
                    group.members[i].started = Some(Started {
                        ticket: r.ticket_id,
                        handle: r.handle,
                        branch: r.branch,
                        worktree_path: r.worktree_path,
                        session_id: r.session_id,
                    });
                    transition(
                        &mut group,
                        i,
                        MemberState::Tracking,
                        "native dispatch launched; awaiting evidence".into(),
                        r.run_id,
                        crate::run_control::now_ms(),
                    );
                }
                Err(reason) => {
                    group.members[i].refusal =
                        Some(crate::dispatch::DispatchRefusal::uncertain(reason.clone()));
                    transition(
                        &mut group,
                        i,
                        MemberState::Blocked,
                        format!("dispatch outcome requires reconciliation: {reason}"),
                        None,
                        crate::run_control::now_ms(),
                    )
                }
            }
            save_in(registry, &group)?;
        }
    }
    Ok(())
}

/// Stops future refill/repair authorization, retaining workers and all evidence.
#[tauri::command]
pub fn swarm_plan_stop(plan_id: String) -> Result<Group, String> {
    let registry = crate::agents::registry_dir()?;
    let _lease = GroupLease::acquire(&registry)?;
    let path = path_in(&registry, plan_id.trim())?;
    let mut group: Group = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    crate::run_control::under_admission_lock_in(&registry, || {
        stop(&mut group, crate::run_control::now_ms());
        save_in(&registry, &group)
    })?;
    Ok(group)
}

fn stop(group: &mut Group, now: i64) {
    if group.stopped_at.is_some() {
        return;
    }
    group.stopped_at = Some(now);
    for i in 0..group.members.len() {
        let id = group.members[i].run_id.clone();
        transition(
            group,
            i,
            MemberState::Blocked,
            "owner stopped this group; existing workers and evidence retained".into(),
            id,
            now,
        );
    }
}

/// Confirm a swarm plan from the card in the chat thread.
///
/// This explicit owner confirmation shares the persisted queue with NautBot's
/// model tool, while retaining its separate authorization boundary. The model
/// tool always requires a current findings disposition.
#[tauri::command]
pub async fn swarm_plan_dispatch(
    app: tauri::AppHandle,
    plan_id: String,
) -> Result<SwarmDispatched, String> {
    if crate::switches::load().read_only {
        return Err("the read_only kill-switch is engaged".into());
    }
    dispatch_plan(app, plan_id.trim()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replanning_keeps_approved_remote_destination_despite_local_preview_or_profile() {
        let fixture = RefillFixture::five_remote("exe-dev");
        let mut approved = groups_in(&fixture.registry, None).unwrap().remove(0);
        approve(&mut approved, 1);
        save_in(&fixture.registry, &approved).unwrap();
        let mut preview = approved.plan.clone();
        preview.id = "local-preview".into();
        preview.created_at = 2;
        for run in &mut preview.runs {
            run.environment = Some("local".into());
            run.requested_environment = None;
        }
        remember_in(&fixture.registry, preview).unwrap();
        let groups = groups_in(&fixture.registry, Some("FORCASTER")).unwrap();
        for run in &approved.plan.runs {
            let inherited = replan_environment(None, &groups, &run.ticket);
            assert_eq!(inherited, Some("exe-dev"));
            let providers = [crate::settings::SandboxProviderSettings {
                kind: "exe-dev".into(), base_url: String::new(), api_key: None,
            }];
            assert_eq!(resolve_environment(inherited, crate::agent_profiles::AgentExecution::Local,
                &providers).unwrap(), crate::sandbox::launch_env::LaunchEnv::ExeDev);
            assert_eq!(replan_environment(Some("local"), &groups, &run.ticket), Some("local"));
        }
        assert_eq!(replan_environment(None, &groups, "OTHER-1"), None);
        assert!(groups.iter().find(|g| g.plan.id == "local-preview").unwrap().approved_at.is_none());
    }

    #[tokio::test]
    async fn repository_outage_blocks_five_members_without_run_attempts_and_recovers_continuations() {
        use crate::{instance::Role, run_control};
        use std::sync::atomic::Ordering;
        for retained in [false, true] {
            let mut fixture = RefillFixture::five_remote("exe-dev");
            if retained { fixture.seed_refused_continuations(); }
            let before = run_control::list_ids_in(&fixture.registry).unwrap();
            confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).unwrap();
            fixture.repository_error = Some("Desktop repository check failed before worker setup: authentication failed".into());
            let mut outage_events = None;
            for sweep in 1..=3 {
                refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
                let group = groups_in(&fixture.registry, None).unwrap().remove(0);
                assert!(group.members.iter().all(|m| m.state == MemberState::Blocked
                    && m.refusal.as_ref().unwrap().kind == crate::dispatch::RefusalKind::RepositoryAccess));
                assert!(fixture.launched.lock().unwrap().is_empty());
                if let Some(count) = outage_events { assert_eq!(group.events.len(), count, "unchanged outage cannot flood the event trail"); }
                outage_events = Some(group.events.len());
                assert_eq!(run_control::list_ids_in(&fixture.registry).unwrap(), before);
                assert_eq!(fixture.repository_checks.load(Ordering::SeqCst), sweep,
                    "one repository probe per shared destination per sweep, not five");
            }
            fixture.repository_error = None;
            refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
            assert_eq!(fixture.launched.lock().unwrap().len(), 3);
            let group = groups_in(&fixture.registry, None).unwrap().remove(0);
            assert_eq!(report(&group).queued.len(), 2);
            for member in group.members.iter().take(3) {
                let run = run_control::load_manifest_in(&fixture.registry, member.run_id.as_ref().unwrap()).unwrap();
                assert_eq!(run.remote_env.as_deref(), Some("exe-dev"));
                if retained { assert!(run.previous_run_id.is_some()); }
            }
            refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
            assert_eq!(fixture.launched.lock().unwrap().len(), 3, "no duplicate launches");
        }
    }

    #[tokio::test]
    async fn runtime_outage_preserves_five_continuations_until_worker_readiness_recovers() {
        use crate::{instance::Role, run_control};
        let mut fixture = RefillFixture::five_remote("exe-dev");
        fixture.seed_refused_continuations();
        let before = run_control::list_ids_in(&fixture.registry).unwrap();
        confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).unwrap();
        fixture.runtime_error = Some("Configured worker command is missing".into());
        let mut events = None;
        for _ in 0..3 {
            refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
            let group = groups_in(&fixture.registry, None).unwrap().remove(0);
            assert!(group.members.iter().all(|m| m.state == MemberState::Blocked
                && m.refusal.as_ref().unwrap().kind == crate::dispatch::RefusalKind::RuntimeReadiness));
            assert_eq!(run_control::list_ids_in(&fixture.registry).unwrap(), before);
            assert!(fixture.launched.lock().unwrap().is_empty());
            if let Some(count) = events { assert_eq!(group.events.len(), count); }
            events = Some(group.events.len());
        }
        fixture.runtime_error = None;
        refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        assert_eq!(fixture.launched.lock().unwrap().len(), 3);
        let group = groups_in(&fixture.registry, None).unwrap().remove(0);
        assert_eq!(report(&group).queued.len(), 2);
        for member in group.members.iter().take(3) {
            let run = run_control::load_manifest_in(&fixture.registry, member.run_id.as_ref().unwrap()).unwrap();
            assert!(run.previous_run_id.is_some());
            assert_eq!(run.remote_env.as_deref(), Some("exe-dev"));
        }
        refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        assert_eq!(fixture.launched.lock().unwrap().len(), 3);
    }

    #[test]
    fn completed_continuation_tracks_its_successor_instead_of_lexically_later_failed_history() {
        use crate::run_control::{RunManifest, RunState};
        let fixture = RefillFixture::five_remote("exe-dev");
        let mut group = groups_in(&fixture.registry, None).unwrap().remove(0);
        let planned = group.plan.runs[0].clone();
        let mut old = RunManifest::requested(&planned.owner, "fixture", "/preserved", Some(planned.ticket.clone()), None, &[], 1);
        old.run_id = "zz-legacy-refused".into(); old.state = RunState::Failed;
        old.next_run_id = Some("01-new-completed".into());
        let mut new = old.clone(); new.run_id = "01-new-completed".into(); new.state = RunState::Done;
        new.started_at = 2; new.previous_run_id = Some(old.run_id.clone()); new.next_run_id = None;
        group.members[0].started = Some(Started { ticket: planned.ticket, handle: planned.owner,
            branch: planned.branch, worktree_path: "/preserved".into(), session_id: "new-session".into() });
        group.members[0].run_id = Some(new.run_id.clone());
        let snapshot = crate::project_continuity::reconcile("FORCASTER", &fixture.tickets, &[old,new], &[], 3);
        assert!(recover_member(&fixture.registry, &mut group, 0, &snapshot, 3));
        assert_eq!(group.members[0].run_id.as_deref(), Some("01-new-completed"));
    }

    #[test]
    fn explicit_swarm_destination_survives_storage_and_overrides_local_profile() {
        use crate::agent_profiles::AgentExecution;
        use crate::sandbox::launch_env::LaunchEnv;
        let providers = vec![crate::settings::SandboxProviderSettings {
            kind: "exe-dev".into(), base_url: String::new(), api_key: None,
        }];
        let tickets = [ticket("FORCASTER-2", "FORCASTER", "ready", Some("codex"))];
        let mut plan = plan_from("destination", "FORCASTER", &[],
            &board(&tickets, &models(), &HashSet::new()), 3, 0).unwrap();
        let run = &mut plan.runs[0];
        run.requested_environment = Some("exe-dev".into());
        run.environment = Some(resolve_environment(run.requested_environment.as_deref(),
            AgentExecution::Local, &providers).unwrap().key().into());
        let stored: SwarmPlan = serde_json::from_slice(&serde_json::to_vec(&plan).unwrap()).unwrap();
        let run = &stored.runs[0];
        assert_eq!(run.environment.as_deref(), Some("exe-dev"));
        assert_eq!(resolve_environment(run.requested_environment.as_deref(),
            AgentExecution::Local, &providers).unwrap(), LaunchEnv::ExeDev);
        assert_eq!(resolve_environment(None, AgentExecution::Local, &providers).unwrap(), LaunchEnv::Local);
        assert!(resolve_environment(Some("unknown"), AgentExecution::Local, &providers).is_err());
        assert!(resolve_environment(Some("gitvm"), AgentExecution::Local, &providers).is_err());
        let mut legacy = serde_json::to_value(&stored).unwrap();
        legacy["runs"][0].as_object_mut().unwrap().remove("requested_environment");
        legacy.as_object_mut().unwrap().remove("dispatch_hold");
        let legacy: SwarmPlan = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.runs[0].requested_environment, None);
        assert_eq!(legacy.dispatch_hold, None);
    }

    #[test]
    fn five_queued_members_are_reported_as_five_and_remain_approved() {
        let dir = scratch();
        let tickets: Vec<_> = (2..=6).map(|n|
            ticket(&format!("FORCASTER-{n}"), "FORCASTER", "ready", Some("codex"))).collect();
        let plan = plan_from("queued", "FORCASTER", &[],
            &board(&tickets, &models(), &HashSet::new()), 3, 0).unwrap();
        remember_in(&dir, plan).unwrap();
        let mut group = groups_in(&dir, None).unwrap().remove(0);
        approve(&mut group, 1);
        save_in(&dir, &group).unwrap();
        let mut restarted = groups_in(&dir, None).unwrap().remove(0);
        approve(&mut restarted, 2);
        let result = report(&restarted).tool_result();
        assert_eq!(result["total"], 5);
        assert_eq!(result["queued"].as_array().unwrap().len(), 5);
        assert!(result["started"].as_array().unwrap().is_empty());
        assert_eq!(result["members"][0]["reason"], "approved; waiting for capacity");
        assert!(result["note"].as_str().unwrap().contains("Recorded launches 0; queued 5; blocked 0; 5 total"));
        assert_eq!(restarted.approved_at, Some(1));
        assert_eq!(restarted.events.len(), 5);
        restarted.members[0].state = MemberState::Tracking;
        restarted.members[0].reason = "existing assignment; tracking without replacement".into();
        restarted.members[0].run_id = Some("existing".into());
        let tracking = report(&restarted).tool_result();
        assert_eq!(tracking["total"], 5);
        assert_eq!(tracking["members"][0]["state"], "tracking");
        assert_eq!(tracking["members"][0]["run_id"], "existing");
        assert_eq!(tracking["queued"].as_array().unwrap().len(), 4);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn approved_remote_dispatch_policy_preserves_local_and_automatic_restrictions() {
        use crate::instance::Role;
        for environment in [None, Some("local"), Some("exe-dev"), Some("gitvm"), Some("unknown")] {
            for role in [Role::Fleet, Role::Workstation, Role::Sandbox] {
                let remote = matches!(environment, Some("exe-dev" | "gitvm"));
                let allowed = role == Role::Fleet || (role == Role::Workstation && remote);
                assert_eq!(crate::dispatch::approved_dispatch_policy(role, false, environment).is_ok(), allowed);
                assert!(crate::dispatch::approved_dispatch_policy(role, true, environment).is_err());
            }
        }
        assert!(!Role::Workstation.dispatches(), "unattended dispatch is still Fleet-only");
    }

    fn ticket(id: &str, project: &str, status: &str, owner: Option<&str>) -> TicketRecord {
        let mut record: TicketRecord = serde_json::from_value(serde_json::json!({
            "id": id, "project": project, "title": format!("{id} title"), "type": "feature",
            "status": status, "priority": "medium", "owner": owner, "documentation": [],
            "body": "", "source_id": "", "revision": 1, "created_at": "", "updated_at": ""
        }))
        .expect("fixture ticket");
        record.tags = vec![];
        record
    }

    fn models() -> HashMap<String, String> {
        HashMap::from([
            ("claude".to_string(), "claude-opus-5".to_string()),
            ("codex".to_string(), "gpt-5.6-sol".to_string()),
        ])
    }

    fn board<'a>(
        tickets: &'a [TicketRecord],
        models: &'a HashMap<String, String>,
        live: &'a HashSet<String>,
    ) -> Board<'a> {
        Board {
            tickets,
            models,
            live,
        }
    }

    fn reason_for<'a>(plan: &'a SwarmPlan, id: &str) -> &'a str {
        plan.skipped
            .iter()
            .find(|s| s.ticket == id)
            .map(|s| s.reason.as_str())
            .unwrap_or_else(|| panic!("{id} was neither planned nor skipped: {plan:?}"))
    }

    #[test]
    fn a_plan_holds_only_tickets_the_pm_actually_has() {
        // The acceptance criterion, and the whole reason the planning left the
        // pane: its LLM was ASKED not to invent tickets. This asks the board.
        let tickets = vec![
            ticket("XNAUT-1", "XNAUT", "ready", Some("claude")),
            ticket("XNAUT-2", "XNAUT", "complete", Some("claude")),
            ticket("OTHER-9", "OTHER", "ready", Some("claude")),
        ];
        let (models, live) = (models(), HashSet::new());
        let requested = ["XNAUT-1", "XNAUT-2", "OTHER-9", "XNAUT-404"].map(String::from);
        let plan = plan_from(
            "p1",
            "XNAUT",
            &requested,
            &board(&tickets, &models, &live),
            10,
            7,
        )
        .expect("plan");

        assert_eq!(plan.runs.len(), 1);
        assert_eq!(plan.runs[0].ticket, "XNAUT-1");
        assert_eq!(plan.runs[0].owner, "claude");
        assert_eq!(
            plan.runs[0].model, "claude-opus-5",
            "the OWNER's model, not a swarm-wide one"
        );
        assert_eq!(plan.runs[0].branch, "agent/claude/xnaut-1");
        assert_eq!(reason_for(&plan, "XNAUT-404"), "not a ticket the PM has");
        assert_eq!(reason_for(&plan, "XNAUT-2"), "status is complete");
        assert_eq!(reason_for(&plan, "OTHER-9"), "belongs to OTHER, not XNAUT");
        assert_eq!(plan.created_at, 7);

        // Asked for nothing, it is the project's whole open board — and only
        // this project's.
        let all = plan_from("p2", "XNAUT", &[], &board(&tickets, &models, &live), 10, 7).unwrap();
        assert_eq!(
            all.runs
                .iter()
                .map(|r| r.ticket.as_str())
                .collect::<Vec<_>>(),
            ["XNAUT-1"]
        );
        assert!(
            all.skipped.is_empty(),
            "nothing was asked for, so nothing was refused"
        );
    }

    #[test]
    fn every_ticket_left_out_says_why_it_was_left_out() {
        let tickets = vec![
            ticket("XNAUT-1", "XNAUT", "ready", None),
            ticket("XNAUT-2", "XNAUT", "ready", Some("ghost")),
            ticket("XNAUT-3", "XNAUT", "in_progress", Some("claude")),
            ticket("XNAUT-4", "XNAUT", "ready", Some("@Codex")),
        ];
        let models = models();
        let live = HashSet::from(["XNAUT-3".to_string()]);
        let plan = plan_from("p1", "XNAUT", &[], &board(&tickets, &models, &live), 10, 0).unwrap();

        assert_eq!(plan.runs.len(), 1);
        assert_eq!(plan.runs[0].ticket, "XNAUT-4");
        // "@Codex" on the ticket and "codex" in the profile store are the same
        // agent; dispatch normalises the same way, so the plan must too.
        assert_eq!(plan.runs[0].owner, "codex");
        assert_eq!(reason_for(&plan, "XNAUT-1"), "no owner to dispatch it to");
        assert_eq!(reason_for(&plan, "XNAUT-2"), "@ghost has no agent profile");
        assert_eq!(reason_for(&plan, "XNAUT-3"), "already has a live run");
    }

    #[test]
    fn the_cap_limits_concurrency_without_dropping_queue_members() {
        let tickets: Vec<TicketRecord> = (1..=5)
            .map(|n| ticket(&format!("XNAUT-{n}"), "XNAUT", "ready", Some("claude")))
            .collect();
        let (models, live) = (models(), HashSet::new());
        let plan = plan_from("p1", "XNAUT", &[], &board(&tickets, &models, &live), 3, 0).unwrap();
        assert_eq!(plan.runs.len(), 5);
        assert_eq!(plan.max_parallel, 3);
        assert!(plan.skipped.is_empty());

        // A profile set to nonsense is clamped, never obeyed. Thirty concurrent
        // agents must stay reachable (XNAUT-185), so the ceiling is above it.
        let wild = plan_from("p2", "XNAUT", &[], &board(&tickets, &models, &live), 0, 0).unwrap();
        assert_eq!(wild.max_parallel, 1);
        let huge = plan_from(
            "p3",
            "XNAUT",
            &[],
            &board(&tickets, &models, &live),
            9_000,
            0,
        )
        .unwrap();
        assert_eq!(huge.max_parallel, HARD_CAP);
        assert!(HARD_CAP >= 30, "the scale test asks for thirty");
    }

    #[test]
    fn one_worktree_per_run_even_when_a_ticket_is_named_twice() {
        let tickets = vec![
            ticket("XNAUT-1", "XNAUT", "ready", Some("claude")),
            ticket("XNAUT-2", "XNAUT", "ready", Some("codex")),
        ];
        let (models, live) = (models(), HashSet::new());
        let requested = ["XNAUT-1", "xnaut-1", "XNAUT-2"].map(String::from);
        let plan = plan_from(
            "p1",
            "XNAUT",
            &requested,
            &board(&tickets, &models, &live),
            10,
            0,
        )
        .expect("plan");
        assert_eq!(plan.runs.len(), 2, "the repeat is not a second run");
        assert_eq!(reason_for(&plan, "XNAUT-1"), "listed twice");
        let branches: HashSet<&str> = plan.runs.iter().map(|r| r.branch.as_str()).collect();
        assert_eq!(
            branches.len(),
            plan.runs.len(),
            "two runs shared a worktree"
        );
        // The branch is dispatch's, not a second spelling of it.
        assert!(plan
            .runs
            .iter()
            .all(|r| r.branch == crate::dispatch::branch_for(&r.owner, &r.ticket)));

        assert!(plan_from("p1", "  ", &[], &board(&tickets, &models, &live), 3, 0).is_err());
    }

    #[test]
    fn one_runnable_ticket_is_a_dispatch_and_never_a_swarm_question() {
        // The acceptance criterion. The card is raised by the plan travelling
        // to the UI, so a plan that is not OFFERED cannot ask anything — and
        // this holds whether the owner named one ticket or the project has
        // only one open, which is the case a description alone would miss.
        let tickets = vec![
            ticket("XNAUT-1", "XNAUT", "ready", Some("claude")),
            ticket("XNAUT-2", "XNAUT", "review", Some("claude")),
            ticket("XNAUT-3", "XNAUT", "ready", Some("codex")),
        ];
        let (models, live) = (models(), HashSet::new());
        let board = board(&tickets, &models, &live);

        let named = plan_from("p1", "XNAUT", &["XNAUT-1".into()], &board, 10, 0).unwrap();
        assert_eq!(offer(&named), Offer::Single("XNAUT-1".into()));

        // Two open on the board, one of them already closed: still one run.
        let only_one = plan_from(
            "p2",
            "XNAUT",
            &["XNAUT-1".into(), "XNAUT-2".into()],
            &board,
            10,
            0,
        )
        .unwrap();
        assert_eq!(offer(&only_one), Offer::Single("XNAUT-1".into()));

        // A cap of one is NOT a reason to skip the question — it is a swarm
        // the owner has throttled, and the tickets it dropped need saying.
        let throttled = plan_from("p3", "XNAUT", &[], &board, 1, 0).unwrap();
        assert_eq!(offer(&throttled), Offer::Swarm);
        assert_eq!(throttled.runs.len(), 2);

        assert_eq!(
            offer(&plan_from("p4", "XNAUT", &[], &board, 10, 0).unwrap()),
            Offer::Swarm
        );
        let none = plan_from("p5", "XNAUT", &["XNAUT-404".into()], &board, 10, 0).unwrap();
        assert_eq!(offer(&none), Offer::Nothing);
    }

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xnaut-group-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn approved_queue_survives_restart_and_duplicate_confirmation() {
        let dir = scratch();
        let tickets: Vec<_> = (1..=3)
            .map(|n| ticket(&format!("XNAUT-{n}"), "XNAUT", "ready", Some("claude")))
            .collect();
        let plan = plan_from(
            "durable",
            "XNAUT",
            &[],
            &board(&tickets, &models(), &HashSet::new()),
            2,
            7,
        )
        .unwrap();
        remember_in(&dir, plan.clone()).unwrap();
        assert!(
            remember_in(&dir, plan).is_err(),
            "same id cannot replace an existing approval"
        );
        let mut group = groups_in(&dir, Some("XNAUT")).unwrap().remove(0);
        assert_eq!(group.members.len(), 3);
        assert!(group.approved_at.is_none());
        approve(&mut group, 10);
        save_in(&dir, &group).unwrap();
        let mut restarted = groups_in(&dir, None).unwrap().remove(0);
        let event_ids: Vec<_> = restarted.events.iter().map(|e| e.id.clone()).collect();
        approve(&mut restarted, 20);
        save_in(&dir, &restarted).unwrap();
        assert_eq!(restarted.approved_at, Some(10));
        assert_eq!(
            restarted
                .events
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>(),
            event_ids
        );
        assert!(restarted.events.iter().all(|e| e.at_ms == 10));
        assert_eq!(report(&restarted).queued.len(), 3);
        assert!(groups_in(&dir, Some("OTHER")).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn model_group_checks_every_saved_member_and_retains_gate_until_owner_confirmation() {
        let dir = scratch();
        let mut tickets = vec![ticket("XNAUT-1","XNAUT","ready",Some("claude")),
            ticket("XNAUT-2","XNAUT","ready",Some("codex"))];
        tickets[1].source_id = "forgejo:team/repo#2".into();
        let plan = plan_from("model-origin","XNAUT",&[],&board(&tickets,&models(),&HashSet::new()),2,7).unwrap();
        remember_in(&dir,plan).unwrap();
        let mut group = groups_in(&dir,None).unwrap().remove(0);
        let before = serde_json::to_value(&group).unwrap();
        let admit = |_: &str,id: &str| crate::ticket_triage::admission_from_records(tickets.iter().find(|t|t.id == id).unwrap(),&[]);
        assert!(approve_from(&mut group,true,10,admit).unwrap_err().contains("no recorded triage"));
        assert_eq!(serde_json::to_value(&group).unwrap(),before,"no partial model approval or events");
        // Ordinary spoken batch requests retain their existing behavior.
        tickets[1].source_id.clear();
        approve_from(&mut group,true,20,|_,id|crate::ticket_triage::admission_from_records(tickets.iter().find(|t|t.id == id).unwrap(),&[])).unwrap();
        save_in(&dir,&group).unwrap();
        let mut restarted = groups_in(&dir,None).unwrap().remove(0);
        assert!(restarted.requires_findings_triage);
        let approval = restarted.approved_at;
        let history = restarted.events.len();
        // A model retry cannot present an owner-approval flag or erase the gate.
        assert!(approve_from(&mut restarted,true,30,|_,_|Err("stale finding".into())).is_err());
        assert!(restarted.requires_findings_triage); assert_eq!(restarted.approved_at,approval);
        assert_eq!(restarted.events.len(),history);
        // Only the separate owner UI entry point supplies model_origin=false.
        approve_from(&mut restarted,false,40,|_,_|panic!("owner confirmation is explicit")).unwrap();
        assert!(!restarted.requires_findings_triage);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn group_cap_excludes_other_projects_but_counts_its_ticketless_reviewer() {
        use crate::run_control::{self, RunManifest};
        let registry = scratch();
        let target = ticket("XNAUT-1", "XNAUT", "ready", Some("claude"));
        let plan = plan_from(
            "parallel-projects",
            "XNAUT",
            &[],
            &board(&[target], &models(), &HashSet::new()),
            2,
            0,
        )
        .unwrap();
        remember_in(&registry, plan).unwrap();
        let group = groups_in(&registry, None).unwrap().remove(0);
        let parent: crate::repository_transfer::Transfer = serde_json::from_value(serde_json::json!({"run_id":"author", "project":"XNAUT", "ticket":"XNAUT-1", "handle":"claude", "local_path":"/task", "remote":"https://fixture/team/repo.git", "source_sha":"a".repeat(40), "base":"main", "branch":"task", "workdir":"worker", "artifacts":".xnaut/runs/author", "state":"review", "pr_url":"https://fixture/pulls/1"})).unwrap();
        let mut reviewer = parent.clone();
        reviewer.run_id = "reviewer".into();
        reviewer.ticket = None;
        reviewer.review_parent = Some(parent.run_id.clone());
        reviewer.local_path = "/group-review".into();
        let scope = WorkerGroupScope::new(&group, &[parent, reviewer]);
        // Even a conversation opened in a matching review checkout is exempt.
        let mut chat = RunManifest::requested("owner-chat", "fixture", "/group-review", None, None, &[], 1);
        chat.user_conversation = true;
        run_control::request_in(&registry, chat, || Ok(())).unwrap();
        assert_eq!(scope.count(&registry).unwrap(), 0);
        for n in 0..2 {
            let run = RunManifest::requested(
                "other-project",
                "fixture",
                &format!("/elsewhere-{n}"),
                Some(format!("OTHER-{n}")),
                None,
                &[],
                n,
            );
            run_control::request_in(&registry, run, || Ok(())).unwrap();
        }
        let launch = |path: &str, ticket: Option<String>| {
            let run = RunManifest::requested("worker", "fixture", path, ticket, None, &[], 10);
            let own = run.run_id.clone();
            run_control::request_in(&registry, run, || {
                run_control::worker_capacity_in(&registry, 8, &own, None)?;
                run_control::worker_capacity_matching_in(&registry, 2, &own, None, |r| {
                    scope.contains(r)
                })
            })
        };
        assert!(
            launch("/task", Some("XNAUT-1".into())).is_ok(),
            "other projects do not consume this group's cap"
        );
        assert!(
            launch("/group-review", None).is_ok(),
            "independent reviewer consumes the second group slot"
        );
        assert_eq!(scope.count(&registry).unwrap(), 2);
        assert!(launch("/third-group-worker", Some("XNAUT-1".into()))
            .unwrap_err()
            .contains("worker capacity:"));
        assert_eq!(run_control::worker_count_in(&registry).unwrap(), 4);
        std::fs::remove_dir_all(registry).unwrap();
    }

    #[test]
    fn persisted_approval_pins_repository_runtime_and_resolved_environment() {
        let dir = scratch();
        let original = ticket("XNAUT-1", "XNAUT", "ready", Some("claude"));
        let mut plan = plan_from(
            "pins",
            "XNAUT",
            &[],
            &board(&[original], &models(), &HashSet::new()),
            2,
            0,
        )
        .unwrap();
        let run = &mut plan.runs[0];
        run.repository_root = Some("/approved/repository".into());
        run.repository_remote = Some("ssh://forge/team/repo.git".into());
        run.runtime_id = Some("claude-code".into());
        run.environment = Some("gitvm".into());
        remember_in(&dir, plan).unwrap();
        let mut group = groups_in(&dir, None).unwrap().remove(0);
        approve(&mut group, 1);
        save_in(&dir, &group).unwrap();
        let group = groups_in(&dir, None).unwrap().remove(0);
        let run = &group.plan.runs[0];
        assert!(pins_match(
            run,
            Some("/approved/repository"),
            "ssh://forge/team/repo.git",
            &run.model,
            "claude-code",
            "gitvm",
            None
        ));
        for field in ["root", "remote", "model", "runtime", "environment"] {
            assert!(
                !pins_match(
                    run,
                    Some(if field == "root" {
                        "/changed"
                    } else {
                        "/approved/repository"
                    }),
                    if field == "remote" {
                        "ssh://different/team/repo.git"
                    } else {
                        "ssh://forge/team/repo.git"
                    },
                    if field == "model" {
                        "changed-model"
                    } else {
                        &run.model
                    },
                    if field == "runtime" {
                        "changed-runtime"
                    } else {
                        "claude-code"
                    },
                    if field == "environment" {
                        "exe-dev"
                    } else {
                        "gitvm"
                    },
                    None
                ),
                "{field}"
            );
        }
        let mut cloud_run = run.clone();
        let connection = crate::cloud_model::Pin { provider: "gateway".into(), model: run.model.clone(), endpoint: "https://models.example/v1".into() };
        cloud_run.cloud_model = Some(connection.clone());
        let cloud_run: PlannedRun = serde_json::from_slice(&serde_json::to_vec(&cloud_run).unwrap()).unwrap();
        assert!(pins_match(&cloud_run, Some("/approved/repository"), "ssh://forge/team/repo.git",
            &run.model, "claude-code", "gitvm", Some(&connection)));
        for changed in [None, Some(crate::cloud_model::Pin { provider: "other".into(), ..connection.clone() }),
            Some(crate::cloud_model::Pin { endpoint: "https://other.example/v1".into(), ..connection.clone() }),
            Some(crate::cloud_model::Pin { model: "other-model".into(), ..connection.clone() })] {
            assert!(!pins_match(&cloud_run, Some("/approved/repository"), "ssh://forge/team/repo.git",
                &run.model, "claude-code", "gitvm", changed.as_ref()));
        }
        let mut legacy = run.clone();
        legacy.runtime_id = None;
        assert!(!pins_match(
            &legacy,
            Some("/approved/repository"),
            "ssh://forge/team/repo.git",
            &run.model,
            "claude-code",
            "gitvm",
            None
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scope_owner_and_owner_only_changes_revoke_group_authorization() {
        let original = ticket("XNAUT-1", "XNAUT", "ready", Some("claude"));
        let plan = plan_from(
            "scope",
            "XNAUT",
            &[],
            &board(std::slice::from_ref(&original), &models(), &HashSet::new()),
            2,
            0,
        )
        .unwrap();
        let run = &plan.runs[0];
        assert!(authorized(run, &original));
        let mut changed = original.clone();
        changed.owner = Some("codex".into());
        assert!(!authorized(run, &changed));
        changed = original.clone();
        changed.body = "new task scope".into();
        assert!(!authorized(run, &changed));
        changed = original.clone();
        changed.approval.owner_only = true;
        assert!(!authorized(run, &changed));
        changed = original.clone();
        changed
            .body
            .push_str("\n\n## Dispatched today\nworker evidence");
        assert!(
            !authorized(run, &changed),
            "even dispatch-looking suffixes require native scope evidence"
        );
        changed
            .body
            .push_str("\nNew owner instructions: change another project");
        assert!(!authorized(run, &changed));
    }

    #[test]
    fn group_requeues_only_proven_prelaunch_lineage_and_bounds_repeated_failures() {
        use crate::run_control::{self, RunManifest, PrelaunchPhase};
        let dir = scratch();
        let task = ticket("XNAUT-1", "XNAUT", "ready", Some("claude"));
        let plan = plan_from("prelaunch", "XNAUT", &[], &board(&[task.clone()], &models(), &HashSet::new()), 2, 0).unwrap();
        remember_in(&dir, plan).unwrap();
        let mut group = groups_in(&dir, None).unwrap().remove(0);
        approve(&mut group, 1);
        let mut runs = Vec::new();
        let phases = [PrelaunchPhase::RepositoryStaging, PrelaunchPhase::SpendAdmission,
            PrelaunchPhase::SpendAdmission, PrelaunchPhase::RepositoryStaging,
            PrelaunchPhase::SpendAdmission, PrelaunchPhase::RepositoryStaging];
        for (attempt, phase) in phases.into_iter().enumerate() {
            let mut run = RunManifest::requested("claude", "fixture", "/preserved", Some(task.id.clone()), None, &[], 10 + attempt as i64);
            run.branch = "agent/preserved".into();
            run_control::bind_pending_in(&dir, &mut run).unwrap();
            run_control::refuse_prelaunch_in(&dir, run, phase, "fixture prelaunch failure").unwrap();
            runs = run_control::list_ids_in(&dir).unwrap().iter().map(|id| run_control::load_manifest_in(&dir, id).unwrap()).collect();
            let snapshot = crate::project_continuity::reconcile("XNAUT", &[task.clone()], &runs, &[], 100);
            group.members[0].state = MemberState::Blocked;
            if attempt < 5 {
                assert!(!recover_member(&dir, &mut group, 0, &snapshot, 100));
                assert_eq!(group.members[0].state, MemberState::Queued);
                save_in(&dir, &group).unwrap();
                group = groups_in(&dir, None).unwrap().remove(0);
            } else {
                assert!(recover_member(&dir, &mut group, 0, &snapshot, 100));
                assert_eq!(group.members[0].state, MemberState::Blocked);
                assert!(group.members[0].reason.contains("retry limit"));
                save_in(&dir, &group).unwrap();
                group = groups_in(&dir, None).unwrap().remove(0);
                assert!(recover_member(&dir, &mut group, 0, &snapshot, 101));
                assert_eq!(group.members[0].state, MemberState::Blocked);
            }
        }
        assert_eq!(runs.len(), 6);
        assert!(runs.iter().all(|r| r.branch == "agent/preserved" && r.worktree_path == "/preserved"));
        // A fresh, explicitly approved scope may retry after the prerequisite was
        // repaired. Reconfirming the old plan never resets its retry budget.
        let mut fresh = group.plan.clone();
        fresh.id = "prelaunch-repaired".into();
        fresh.created_at = 200;
        remember_in(&dir, fresh).unwrap();
        let mut fresh = groups_in(&dir, None).unwrap().into_iter()
            .find(|g| g.plan.id == "prelaunch-repaired").unwrap();
        approve(&mut fresh, 201);
        let snapshot = crate::project_continuity::reconcile("XNAUT", &[task], &runs, &[], 202);
        assert!(!recover_member(&dir, &mut fresh, 0, &snapshot, 202));
        assert_eq!(fresh.members[0].state, MemberState::Queued);
        assert_eq!(run_control::list_ids_in(&dir).unwrap().len(), 6);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn interrupted_dispatch_blocks_without_evidence_and_recovers_existing_assignment() {
        let dir = scratch();
        let ticket = ticket("XNAUT-1", "XNAUT", "ready", Some("claude"));
        let plan = plan_from(
            "recover",
            "XNAUT",
            &[],
            &board(&[ticket], &models(), &HashSet::new()),
            2,
            0,
        )
        .unwrap();
        remember_in(&dir, plan).unwrap();
        let mut group = groups_in(&dir, None).unwrap().remove(0);
        approve(&mut group, 1);
        transition(
            &mut group,
            0,
            MemberState::Starting,
            "reserved".into(),
            None,
            2,
        );
        save_in(&dir, &group).unwrap();
        let mut restarted = groups_in(&dir, None).unwrap().remove(0);
        let mut snapshot: crate::project_continuity::ProjectSnapshot =
            serde_json::from_value(serde_json::json!({
                "project":"XNAUT","observed_at":3,"tickets":[],"assignments":[],"diagnostics":[]
            }))
            .unwrap();
        assert!(recover_member(&dir, &mut restarted, 0, &snapshot, 3));
        assert_eq!(restarted.members[0].state, MemberState::Blocked);
        crate::project_continuity::add_launch_receipt(
            &mut snapshot,
            &serde_json::json!({
                "ticket":"XNAUT-1","project":"XNAUT","handle":"claude","branch":"agent/claude/xnaut-1",
                "worktree_path":"/existing","launch":{"run_id":"original-run"}
            }),
            "test receipt",
        );
        assert!(recover_member(&dir, &mut restarted, 0, &snapshot, 4));
        assert_eq!(restarted.members[0].run_id.as_deref(), Some("original-run"));
        assert_ne!(
            restarted.members[0].state,
            MemberState::Queued,
            "existing work cannot be relaunched"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn previously_verified_group_member_tracks_corrected_blocked_evidence_after_restart() {
        let dir = scratch();
        let ticket = ticket("XNAUT-1", "XNAUT", "ready", Some("claude"));
        let plan = plan_from("corrected-review", "XNAUT", &[],
            &board(&[ticket], &models(), &HashSet::new()), 2, 0).unwrap();
        remember_in(&dir, plan).unwrap();
        let mut group = groups_in(&dir, None).unwrap().remove(0);
        approve(&mut group, 1);
        transition(&mut group, 0, MemberState::Verified,
            "independent verification evidence recorded".into(), Some("author-run".into()), 2);
        save_in(&dir, &group).unwrap();
        let mut group = groups_in(&dir, None).unwrap().remove(0);
        let previous_events = group.events.len();
        let mut snapshot: crate::project_continuity::ProjectSnapshot = serde_json::from_value(serde_json::json!({
            "project":"XNAUT","observed_at":3,"tickets":[],"assignments":[],"diagnostics":[]
        })).unwrap();
        crate::project_continuity::add_launch_receipt(&mut snapshot, &serde_json::json!({
            "ticket":"XNAUT-1","project":"XNAUT","handle":"claude","branch":"agent/claude/xnaut-1",
            "worktree_path":"/existing","launch":{"run_id":"author-run"}
        }), "preserved author receipt");
        snapshot.assignments[0].state = crate::project_continuity::ContinuityState::Blocked;
        assert!(recover_member(&dir, &mut group, 0, &snapshot, 3));
        assert_eq!(group.members[0].state, MemberState::Blocked);
        assert_eq!(group.events.len(), previous_events + 1);
        assert!(group.events.iter().any(|e| e.state == MemberState::Verified));
        save_in(&dir, &group).unwrap();
        let mut group = groups_in(&dir, None).unwrap().remove(0);
        assert!(recover_member(&dir, &mut group, 0, &snapshot, 4));
        assert_eq!(group.members[0].state, MemberState::Blocked);
        assert_eq!(group.events.len(), previous_events + 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    struct RefillFixture {
        registry: PathBuf,
        tickets: Vec<TicketRecord>,
        fail_reconcile: Option<String>,
        fail_snapshot: Option<String>,
        launched: std::sync::Mutex<Vec<String>>,
        append_dispatch_note: bool,
        repository_error: Option<String>,
        runtime_error: Option<String>,
        repository_checks: std::sync::atomic::AtomicUsize,
    }
    impl RefillFixture {
        fn five_remote(environment: &str) -> Self {
            let registry = scratch();
            let tickets: Vec<_> = (2..=6).map(|n|
                ticket(&format!("FORCASTER-{n}"), "FORCASTER", "ready", Some("codex"))).collect();
            let mut plan = plan_from("remote", "FORCASTER", &[],
                &board(&tickets, &models(), &HashSet::new()), 3, 0).unwrap();
            for run in &mut plan.runs {
                run.requested_environment = Some(environment.into());
                run.environment = Some(environment.into());
            }
            remember_in(&registry, plan).unwrap();
            std::fs::create_dir_all(registry.join("profiles")).unwrap();
            std::fs::write(registry.join("profiles/codex.json"),
                serde_json::to_vec(&serde_json::json!({"model": models()["codex"]})).unwrap()).unwrap();
            Self { registry, tickets, fail_reconcile: None, fail_snapshot: None,
                launched: std::sync::Mutex::new(vec![]), append_dispatch_note: false,
                repository_error: None, runtime_error: None, repository_checks: std::sync::atomic::AtomicUsize::new(0) }
        }

        // Reproduce the persisted 1.30.2 -> 1.30.4 FORCASTER failure: an
        // initial repository refusal, then a refused continuation after the
        // repository setting changed. Neither request reached Starting.
        fn seed_refused_continuations(&self) -> Vec<String> {
            use crate::run_control::{self, PrelaunchPhase, RunManifest};
            let mut group = groups_in(&self.registry, None).unwrap().remove(0);
            let mut refused = Vec::new();
            std::fs::create_dir_all(self.registry.join("chat-launches")).unwrap();
            for (i, planned) in group.plan.runs.iter().enumerate() {
                let mut initial = RunManifest::requested(&planned.owner, "fixture",
                    &self.registry.join(format!("work-{}", planned.ticket)).to_string_lossy(),
                    Some(planned.ticket.clone()), Some(planned.model.clone()), &[], 10);
                initial.project = group.plan.project.clone();
                initial.branch = planned.branch.clone();
                initial.remote_env = planned.environment.clone();
                let initial = run_control::refuse_prelaunch_in(&self.registry, initial,
                    PrelaunchPhase::RepositoryPreparation, "Add a repository URL in Project settings (Forgejo or GitHub).").unwrap();
                let receipt = |run: &RunManifest, previous: Option<&str>| {
                    let path = crate::agent_work::launch_receipt_path(&self.registry, &self.registry,
                        &planned.ticket, previous).unwrap();
                    crate::agent_work::save_launch_receipt(&path, &serde_json::json!({
                        "ticket": planned.ticket, "project": group.plan.project,
                        "handle": planned.owner, "branch": run.branch, "worktree_path": run.worktree_path,
                        "environment": run.remote_env, "repository_root": self.registry,
                        "continuation_run_id": previous, "launch": {"run_id": run.run_id},
                        "pending": false, "ok": false, "admission_refused": true,
                        "execution_started": false, "prelaunch_failure": run.prelaunch_failure,
                        "refused_run_revision": run.revision,
                    })).unwrap();
                };
                receipt(&initial, None);
                let mut next = RunManifest::requested(&planned.owner, "fixture",
                    &self.registry.join(format!("work-{}", planned.ticket)).to_string_lossy(),
                    Some(planned.ticket.clone()), Some(planned.model.clone()), &[], 20);
                next.project = group.plan.project.clone();
                next.remote_env = planned.environment.clone();
                run_control::bind_pending_in(&self.registry, &mut next).unwrap();
                let id = next.run_id.clone();
                assert!(run_control::request_in(&self.registry, next, ||
                    Err("Approved group owner, scope, repository, runtime or environment changed".into())).is_err());
                let saved = run_control::load_manifest_in(&self.registry, &id).unwrap();
                assert!(saved.admission_refused && saved.prelaunch_failure.is_none());
                receipt(&saved, Some(&initial.run_id));
                group.members[i].state = MemberState::Tracking;
                group.members[i].reason = "implementation retained; awaiting review or repair evidence".into();
                group.members[i].run_id = Some(id.clone());
                refused.push(id);
            }
            save_in(&self.registry, &group).unwrap();
            refused
        }

        fn new() -> Self {
            let registry = scratch();
            let tickets = vec![
                ticket("FIRST-1", "FIRST", "ready", Some("claude")),
                ticket("FIRST-2", "FIRST", "ready", Some("codex")),
                ticket("SECOND-1", "SECOND", "ready", Some("codex")),
            ];
            for (index, project) in ["FIRST", "SECOND"].iter().enumerate() {
                let plan = plan_from(
                    &format!("group-{index}"),
                    project,
                    &[],
                    &board(&tickets, &models(), &HashSet::new()),
                    2,
                    index as i64,
                )
                .unwrap();
                remember_in(&registry, plan).unwrap();
            }
            for mut group in groups_in(&registry, None).unwrap() {
                approve(&mut group, 10);
                save_in(&registry, &group).unwrap();
            }
            std::fs::create_dir_all(registry.join("profiles")).unwrap();
            for (owner, model) in models() {
                std::fs::write(
                    registry.join("profiles").join(format!("{owner}.json")),
                    serde_json::to_vec(&serde_json::json!({"model":model})).unwrap(),
                )
                .unwrap();
            }
            Self {
                registry,
                tickets,
                fail_reconcile: None,
                fail_snapshot: None,
                launched: std::sync::Mutex::new(vec![]),
                append_dispatch_note: false,
                repository_error: None,
                runtime_error: None,
                repository_checks: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }
    impl Drop for RefillFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.registry);
        }
    }
    impl RefillBackend for RefillFixture {
        fn runtime_preflight(&self, _run: PlannedRun) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
            Box::pin(async move { self.runtime_error.clone().map_or(Ok(()), Err) })
        }
        fn repository_preflight(&self, _run: PlannedRun) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
            Box::pin(async move {
                self.repository_checks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                self.repository_error.clone().map_or(Ok(()), Err)
            })
        }
        fn reconcile(&self, _project: &str, ticket: &str) -> Result<(), String> {
            if self.fail_reconcile.as_deref() == Some(ticket) {
                Err("exact pending launch receipt is unreadable".into())
            } else {
                Ok(())
            }
        }
        fn snapshot(
            &self,
            project: &str,
        ) -> Result<crate::project_continuity::ProjectSnapshot, String> {
            if self.fail_snapshot.as_deref() == Some(project) {
                return Err("project recovery source unavailable".into());
            }
            let runs = crate::run_control::list_ids_in(&self.registry)?
                .iter()
                .map(|id| crate::run_control::load_manifest_in(&self.registry, id))
                .collect::<Result<Vec<_>, _>>()?;
            let mut snapshot = crate::project_continuity::reconcile(
                project, &self.tickets, &runs, &[], crate::run_control::now_ms(),
            );
            let receipts = self.registry.join("chat-launches");
            if receipts.exists() {
                for path in std::fs::read_dir(receipts).map_err(|e| e.to_string())? {
                    let path = path.map_err(|e| e.to_string())?.path();
                    let receipt: serde_json::Value = serde_json::from_slice(
                        &std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                    crate::project_continuity::add_launch_receipt(&mut snapshot, &receipt, &path.to_string_lossy());
                }
            }
            Ok(snapshot)
        }
        fn pins(&self, run: &PlannedRun, _project: &str) -> Result<bool, String> {
            let bytes = std::fs::read(
                self.registry
                    .join("profiles")
                    .join(format!("{}.json", run.owner)),
            )
            .map_err(|_| format!("agent profile @{} is missing", run.owner))?;
            let profile: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            Ok(profile["model"] == run.model)
        }
        fn capacity(&self, registry: &Path, group: &Group, index: usize) -> Result<bool, String> {
            Ok(slot_available(
                &group.members[index],
                WorkerGroupScope::new(group, &[]).count_for_member(registry, &group.members[index])?,
                group.plan.max_parallel,
            ))
        }
        fn admission(
            &self,
            _run: &PlannedRun,
            _project: &str,
        ) -> Result<(), crate::dispatch::DispatchRefusal> {
            Ok(())
        }
        fn dispatch(
            &self,
            run: PlannedRun,
            project: String,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<crate::dispatch::DispatchResult, String>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async move {
                let pending = crate::run_control::continuation_in(&self.registry,&run.ticket)?;
                let worktree = pending.as_ref().map(|r|r.worktree_path.clone()).unwrap_or_else(|| self
                    .registry
                    .join(format!("work-{}", run.ticket))
                    .to_string_lossy()
                    .into_owned());
                let mut manifest = crate::run_control::RunManifest::requested(
                    &run.owner,
                    "fixture",
                    &worktree,
                    Some(run.ticket.clone()),
                    Some(run.model.clone()),
                    &[],
                    crate::run_control::now_ms(),
                );
                manifest.branch = run.branch.clone();
                manifest.project = project;
                manifest.remote_env = run.environment.clone();
                crate::run_control::bind_pending_in(&self.registry,&mut manifest)?;
                let own = manifest.run_id.clone();
                let registered = crate::run_control::request_in(&self.registry, manifest, || {
                    crate::run_control::worker_capacity_in(&self.registry, 8, &own, None)
                })?;
                self.launched.lock().unwrap().push(run.ticket.clone());
                let ticket_scope = if self.append_dispatch_note {
                    let mut ticket = self.tickets.iter().find(|t| t.id == run.ticket).unwrap().clone();
                    ticket.body.push_str("\nNative dispatch evidence");
                    scope(&ticket)
                } else {
                    run.scope.clone()
                };
                Ok(crate::dispatch::DispatchResult {
                    ticket_id: run.ticket,
                    handle: run.owner,
                    branch: run.branch,
                    worktree_path: worktree,
                    session_id: format!("fixture-{}", registered.run_id),
                    run_id: Some(registered.run_id),
                    environment: run.environment.unwrap_or_else(|| "fixture".into()),
                    ticket_scope,
                })
            })
        }
    }

    #[tokio::test]
    async fn recovered_failed_worker_uses_its_reserved_slot_and_starts_once() {
        use crate::{instance::Role,run_control::{self,RunManifest,RunState,Proofs}};
        let fixture=RefillFixture::five_remote("exe-dev");
        let mut group=groups_in(&fixture.registry,None).unwrap().remove(0);
        group.plan.runs.truncate(1);group.members.truncate(1);group.plan.max_parallel=1;
        save_in(&fixture.registry,&group).unwrap();
        let planned=&group.plan.runs[0];
        let mut previous=RunManifest::requested("old-agent","fixture",
            &fixture.registry.join("original-worktree").to_string_lossy(),Some(planned.ticket.clone()),Some(planned.model.clone()),&[],10);
        previous.branch="original-preserved-branch".into();previous.project=group.plan.project.clone();previous.remote_env=Some("exe-dev".into());
        let id=previous.run_id.clone();
        run_control::request_in(&fixture.registry,previous,||Ok(())).unwrap();
        run_control::update_in(&fixture.registry,&id,|r|r.state=RunState::Failed).unwrap();
        let failed=run_control::load_manifest_in(&fixture.registry,&id).unwrap();
        let proof=Proofs{pid_absent:true,session_known:true,capture_known:true,capture_quiet:true,
            worktree_exists:true,branch_matches:true,commit:"a".repeat(40),..Default::default()};
        let child=run_control::reserve_failed_continuation_in(&fixture.registry,&failed,&proof,&planned.model,20,|_|Ok(())).unwrap();
        refill_in(&fixture.registry,&fixture.tickets,&fixture,Role::Workstation,false).await.unwrap();
        assert!(fixture.launched.lock().unwrap().is_empty(),"reservation cannot approve a group");
        confirm_in(&fixture.registry,"remote",false,Role::Workstation,false,|_,_|Ok(())).unwrap();
        for _ in 0..2 { refill_in(&fixture.registry,&fixture.tickets,&fixture,Role::Workstation,false).await.unwrap(); }
        assert_eq!(*fixture.launched.lock().unwrap(),vec![planned.ticket.clone()]);
        let started=run_control::load_manifest_in(&fixture.registry,&child.run_id).unwrap();
        assert_eq!(started.state,RunState::Starting);
        assert_eq!(started.worktree_path,failed.worktree_path);assert_eq!(started.branch,failed.branch);
        assert_eq!(run_control::list_ids_in(&fixture.registry).unwrap().len(),2);
    }

    #[tokio::test]
    async fn workstation_remote_swarm_approval_restart_refill_and_duplicate_confirmation() {
        use crate::{instance::Role, run_control::{self, RunState}};
        // Both confirmation entry points and both remote providers exercise the
        // production approval store, queue, policy, capacity and run registry.
        for environment in ["exe-dev", "gitvm"] {
            for model_origin in [false, true] {
                let fixture = RefillFixture::five_remote(environment);
                refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
                assert!(fixture.launched.lock().unwrap().is_empty(), "unapproved work must not launch");
                confirm_in(&fixture.registry, "remote", model_origin, Role::Workstation, false, |_, _| Ok(())).unwrap();
                refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
                let group = groups_in(&fixture.registry, None).unwrap().remove(0);
                let result = report(&group).tool_result();
                assert_eq!(result["total"], 5);
                assert_eq!(result["started"].as_array().unwrap().len(), 3);
                assert_eq!(result["queued"].as_array().unwrap().len(), 2);
                assert_eq!(result["failed"].as_array().unwrap().len(), 0);
                let originals: Vec<_> = group.members.iter().filter_map(|m| m.run_id.clone()).collect();
                for _ in 0..2 {
                    confirm_in(&fixture.registry, "remote", model_origin, Role::Workstation, false, |_, _| Ok(())).unwrap();
                    refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
                }
                assert_eq!(fixture.launched.lock().unwrap().len(), 3, "repeat confirmation cannot launch duplicates");
                // Finish one worker. Reloading the persisted group must fill
                // exactly that slot, preserving every previous run identity.
                run_control::update_in(&fixture.registry, &originals[0], |r| r.state = RunState::Done).unwrap();
                refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
                assert_eq!(fixture.launched.lock().unwrap().len(), 4);
                run_control::update_in(&fixture.registry, &originals[1], |r| r.state = RunState::Done).unwrap();
                refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
                let reopened = groups_in(&fixture.registry, None).unwrap().remove(0);
                assert_eq!(fixture.launched.lock().unwrap().len(), 5);
                assert!(report(&reopened).queued.is_empty());
                assert_eq!(reopened.approved_at, group.approved_at);
                assert_eq!(reopened.members.iter().take(3).filter_map(|m| m.run_id.clone()).collect::<Vec<_>>(), originals);
                let ids = run_control::list_ids_in(&fixture.registry).unwrap();
                assert_eq!(ids.len(), 5);
                for id in ids {
                    assert_eq!(run_control::load_manifest_in(&fixture.registry, &id).unwrap().remote_env.as_deref(), Some(environment));
                }
            }
        }
    }

    #[tokio::test]
    async fn refused_continuations_recover_five_retained_runs_without_duplicates_or_destination_changes() {
        use crate::{instance::Role, run_control::{self, RunState}};
        let fixture = RefillFixture::five_remote("exe-dev");
        let refused = fixture.seed_refused_continuations();
        let old_journals: Vec<_> = refused.iter().map(|id|
            std::fs::read(fixture.registry.join(format!("{id}.events.jsonl"))).unwrap()).collect();
        let before = groups_in(&fixture.registry, None).unwrap().remove(0);
        assert_eq!((report(&before).started.len(), report(&before).queued.len(), report(&before).failed.len()), (0, 0, 0));
        // Restart must recover from the native journals, including a missing
        // snapshot, and current explicit approval must still be required.
        std::fs::remove_file(fixture.registry.join(format!("{}.run.json", refused[0]))).unwrap();
        refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        assert!(fixture.launched.lock().unwrap().is_empty());
        confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).unwrap();
        refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        let group = groups_in(&fixture.registry, None).unwrap().remove(0);
        assert_eq!((report(&group).started.len(), report(&group).queued.len(), report(&group).failed.len()), (3, 2, 0));
        for _ in 0..2 {
            confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).unwrap();
            refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        }
        assert_eq!(fixture.launched.lock().unwrap().len(), 3);
        for member in group.members.iter().take(2) {
            run_control::update_in(&fixture.registry, member.run_id.as_ref().unwrap(), |r| r.state = RunState::Done).unwrap();
            refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        }
        let group = groups_in(&fixture.registry, None).unwrap().remove(0);
        assert_eq!(fixture.launched.lock().unwrap().len(), 5);
        assert_eq!(run_control::list_ids_in(&fixture.registry).unwrap().len(), 15);
        for (i, member) in group.members.iter().enumerate() {
            let current = run_control::load_manifest_in(&fixture.registry, member.run_id.as_ref().unwrap()).unwrap();
            let prior = run_control::load_manifest_in(&fixture.registry, &refused[i]).unwrap();
            assert_eq!(current.previous_run_id.as_deref(), Some(refused[i].as_str()));
            assert_eq!(prior.next_run_id.as_deref(), Some(current.run_id.as_str()));
            assert_eq!((&current.branch, &current.worktree_path), (&prior.branch, &prior.worktree_path));
            assert_eq!(current.remote_env.as_deref(), Some("exe-dev"));
            assert!(prior.admission_refused && prior.state == RunState::Failed);
            let journal = std::fs::read(fixture.registry.join(format!("{}.events.jsonl", refused[i]))).unwrap();
            assert!(journal.starts_with(&old_journals[i]), "refusal evidence must only be appended to");
        }
    }

    #[tokio::test]
    async fn overlapping_approvals_recover_under_latest_group_and_keep_its_dispatched_scope() {
        use crate::{instance::Role, run_control::{self, RunState}};
        for tied_approval in [false, true] {
            let mut fixture = RefillFixture::five_remote("exe-dev");
            fixture.append_dispatch_note = true;
            let mut old = groups_in(&fixture.registry, None).unwrap().remove(0);
            approve(&mut old, 100);
            save_in(&fixture.registry, &old).unwrap();
            let refused = fixture.seed_refused_continuations();
            let old = groups_in(&fixture.registry, None).unwrap().remove(0);
            let mut latest = old.clone();
            latest.plan.id = "remote-new".into();
            latest.plan.created_at = 1;
            latest.approved_at = Some(if tied_approval { 100 } else { 101 });
            save_in(&fixture.registry, &latest).unwrap();

            refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
            let groups = groups_in(&fixture.registry, None).unwrap();
            let latest = &groups[1];
            assert_eq!((report(latest).started.len(), report(latest).queued.len(), report(latest).failed.len()), (3, 2, 0));
            for (i, member) in groups[0].members.iter().enumerate() {
                assert!(member.started.is_none(), "superseded plan must not dispatch");
                assert_eq!(member.run_id.as_deref(), Some(refused[i].as_str()));
                assert!(member.reason.contains("remote-new"));
            }
            for member in latest.members.iter().filter(|m| m.started.is_some()) {
                let ticket = fixture.tickets.iter_mut().find(|t| t.id == member.ticket).unwrap();
                ticket.body.push_str("\nNative dispatch evidence");
                let planned = latest.plan.runs.iter().find(|r| r.ticket == member.ticket).unwrap();
                assert_ne!(planned.scope, scope(ticket));
                assert!(authorized_member(&fixture.registry, planned, member, ticket), "native admission must accept the dispatch's own scope append");
                let current = run_control::load_manifest_in(&fixture.registry, member.run_id.as_ref().unwrap()).unwrap();
                assert_eq!(current.remote_env.as_deref(), Some("exe-dev"));
            }
            // Reload persisted plans and repeat confirmation: no older plan may
            // take ownership back or turn the dispatch's scope append into a refusal.
            confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).unwrap();
            for _ in 0..2 {
                refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
            }
            let groups = groups_in(&fixture.registry, None).unwrap();
            assert_eq!(fixture.launched.lock().unwrap().len(), 3);
            assert_eq!(report(&groups[1]).failed.len(), 0);
            for member in groups[1].members.iter().take(2) {
                run_control::update_in(&fixture.registry, member.run_id.as_ref().unwrap(), |r| r.state = RunState::Done).unwrap();
            }
            refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
            let groups = groups_in(&fixture.registry, None).unwrap();
            assert_eq!(report(&groups[1]).started.len(), 5);
            assert_eq!(fixture.launched.lock().unwrap().len(), 5);
            assert_eq!(run_control::list_ids_in(&fixture.registry).unwrap().len(), 15);
            assert!(groups[0].members.iter().all(|m| m.started.is_none()));
        }
    }

    #[tokio::test]
    async fn overlapping_approvals_respect_newest_stop_and_unapproved_partial_plans() {
        use crate::instance::Role;
        for approved in [false, true] {
            let fixture = RefillFixture::five_remote("exe-dev");
            let mut old = groups_in(&fixture.registry, None).unwrap().remove(0);
            approve(&mut old, 100);
            save_in(&fixture.registry, &old).unwrap();
            let mut newer = old.clone();
            newer.plan.id = "remote-new".into();
            newer.plan.created_at = 1;
            newer.plan.runs.truncate(2);
            newer.members.truncate(2);
            newer.approved_at = if approved { Some(101) } else { None };
            if approved { stop(&mut newer, 102); }
            save_in(&fixture.registry, &newer).unwrap();
            for _ in 0..2 {
                refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
            }
            let launched = fixture.launched.lock().unwrap().clone();
            let expected: Vec<_> = if approved { (4..=6).collect() } else { (2..=4).collect() };
            assert_eq!(launched, expected.iter().map(|n| format!("FORCASTER-{n}")).collect::<Vec<_>>());
            let groups = groups_in(&fixture.registry, None).unwrap();
            assert!(groups[1].members.iter().all(|m| m.started.is_none()));
            assert_eq!(groups[1].stopped_at.is_some(), approved);
        }
    }

    #[tokio::test]
    async fn refused_continuations_still_obey_scope_profile_read_only_and_stop() {
        use crate::instance::Role;
        for gate in ["scope", "profile", "read-only", "sandbox", "stop"] {
            let mut fixture = RefillFixture::five_remote("exe-dev");
            fixture.seed_refused_continuations();
            confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).unwrap();
            match gate {
                "scope" => for ticket in &mut fixture.tickets { ticket.body.push_str("new instructions"); },
                "profile" => std::fs::write(fixture.registry.join("profiles/codex.json"), r#"{"model":"changed"}"#).unwrap(),
                "stop" => {
                    let mut group = groups_in(&fixture.registry, None).unwrap().remove(0);
                    stop(&mut group, 100);
                    save_in(&fixture.registry, &group).unwrap();
                },
                _ => {},
            }
            for _ in 0..2 {
                refill_in(&fixture.registry, &fixture.tickets, &fixture,
                    if gate == "sandbox" { Role::Sandbox } else { Role::Workstation }, gate == "read-only").await.unwrap();
            }
            assert!(fixture.launched.lock().unwrap().is_empty(), "{gate}");
            assert_eq!(crate::run_control::list_ids_in(&fixture.registry).unwrap().len(), 10, "{gate}");
        }
    }

    #[test]
    fn refused_continuation_conflicts_are_blocked_with_the_recovery_reason() {
        use crate::run_control::{self, RunState};
        for conflict in ["admitted", "output", "session", "receipt", "diagnostic", "review"] {
            let fixture = RefillFixture::five_remote("exe-dev");
            let refused = fixture.seed_refused_continuations();
            let mut group = groups_in(&fixture.registry, None).unwrap().remove(0);
            approve(&mut group, 1);
            match conflict {
                "admitted" => { run_control::update_in(&fixture.registry, &refused[0], |r| r.admission_refused = false).unwrap(); },
                "output" => { run_control::update_in(&fixture.registry, &refused[0], |r| r.capture_bytes = 12).unwrap(); },
                "session" => { run_control::update_in(&fixture.registry, &refused[0], |r| r.pty_session = Some("existing-worker".into())).unwrap(); },
                _ => {},
            }
            let mut snapshot = fixture.snapshot("FORCASTER").unwrap();
            match conflict {
                "receipt" => crate::project_continuity::add_launch_receipt(&mut snapshot,
                    &serde_json::json!({"ticket":"FORCASTER-2", "launch":{"run_id":"unrelated-run"}}), "unresolved receipt"),
                "diagnostic" => snapshot.diagnostics.push(crate::project_continuity::Diagnostic {
                    blocking: true, source: "fixture".into(), message: "unreadable evidence".into(),
                }),
                "review" => snapshot.tickets.iter_mut().find(|t| t.id == "FORCASTER-2").unwrap().status = "review".into(),
                _ => {},
            }
            assert!(recover_member(&fixture.registry, &mut group, 0, &snapshot, 100), "{conflict}");
            assert_eq!(group.members[0].state, MemberState::Blocked, "{conflict}");
            let expected = match conflict {
                "admitted" => "failed after admission",
                "output" | "session" => "conflicts with execution evidence",
                "receipt" => "existing or unresolved work",
                "diagnostic" => "Project recovery is incomplete",
                "review" => "reconcile its existing work before reopening",
                _ => unreachable!(),
            };
            assert!(group.members[0].reason.contains(expected), "{conflict}: {}", group.members[0].reason);
            if conflict == "receipt" {
                assert!(group.members[0].reason.contains("unrelated-run"), "the unrelated receipt must cause the refusal");
            }
            assert_eq!(group.members[0].run_id.as_deref(), Some(refused[0].as_str()), "{conflict}");
            assert!(!group.members[0].reason.contains("implementation retained"), "{conflict}");
            assert!(group.members[0].reason.contains("recovery refused") || group.members[0].reason.contains("conflicts with execution"), "{conflict}");
            assert_eq!(run_control::load_manifest_in(&fixture.registry, &refused[0]).unwrap().state, RunState::Failed);
            assert_eq!(crate::run_control::list_ids_in(&fixture.registry).unwrap().len(), 10);
        }
    }

    #[tokio::test]
    async fn workstation_swarm_rejects_local_mixed_missing_and_unknown_destinations_before_approval() {
        use crate::instance::Role;
        for destination in [None, Some("local"), Some("unknown")] {
            let fixture = RefillFixture::five_remote("exe-dev");
            let mut group = groups_in(&fixture.registry, None).unwrap().remove(0);
            group.plan.runs[4].environment = destination.map(str::to_owned);
            save_in(&fixture.registry, &group).unwrap();
            assert!(confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).is_err());
            assert!(groups_in(&fixture.registry, None).unwrap()[0].approved_at.is_none());
            refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
            assert!(fixture.launched.lock().unwrap().is_empty());
        }
        let fixture = RefillFixture::five_remote("local");
        confirm_in(&fixture.registry, "remote", false, Role::Fleet, false, |_, _| Ok(())).unwrap();
        refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        let result = report(&groups_in(&fixture.registry, None).unwrap()[0]).tool_result();
        assert_eq!(result["failed"].as_array().unwrap().len(), 5);
        assert!(fixture.launched.lock().unwrap().is_empty(), "legacy local approvals cannot start on this workstation");
    }

    #[tokio::test]
    async fn swarm_read_only_sandbox_and_stop_preserve_queued_work_without_launching() {
        use crate::instance::Role;
        let fixture = RefillFixture::five_remote("exe-dev");
        for (role, read_only) in [(Role::Workstation, true), (Role::Sandbox, false)] {
            assert!(confirm_in(&fixture.registry, "remote", false, role, read_only, |_, _| Ok(())).is_err());
            assert!(groups_in(&fixture.registry, None).unwrap()[0].approved_at.is_none());
        }
        confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).unwrap();
        let before = std::fs::read(path_in(&fixture.registry, "remote").unwrap()).unwrap();
        for (role, read_only) in [(Role::Workstation, true), (Role::Sandbox, false)] {
            refill_in(&fixture.registry, &fixture.tickets, &fixture, role, read_only).await.unwrap();
            assert_eq!(std::fs::read(path_in(&fixture.registry, "remote").unwrap()).unwrap(), before);
        }
        let mut group = groups_in(&fixture.registry, None).unwrap().remove(0);
        stop(&mut group, 100);
        save_in(&fixture.registry, &group).unwrap();
        assert!(confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).is_err());
        refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        assert!(fixture.launched.lock().unwrap().is_empty());
        assert_eq!(groups_in(&fixture.registry, None).unwrap()[0].members.len(), 5);
    }

    #[tokio::test]
    async fn swarm_changed_ticket_scope_blocks_only_affected_member() {
        use crate::instance::Role;
        let mut fixture = RefillFixture::five_remote("exe-dev");
        confirm_in(&fixture.registry, "remote", false, Role::Workstation, false, |_, _| Ok(())).unwrap();
        fixture.tickets[0].body.push_str("changed instructions after approval");
        refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        let group = groups_in(&fixture.registry, None).unwrap().remove(0);
        assert_eq!(group.members[0].state, MemberState::Blocked);
        assert!(group.members[0].reason.contains("scope"));
        assert_eq!(report(&group).started.len(), 3);
        assert_eq!(report(&group).queued.len(), 1);
        assert_eq!(group.members.len(), 5);
        assert!(!fixture.launched.lock().unwrap().contains(&fixture.tickets[0].id));
    }

    #[tokio::test]
    async fn swarm_model_confirmation_failure_cannot_partially_approve_or_launch() {
        use crate::instance::Role;
        let fixture = RefillFixture::five_remote("exe-dev");
        assert!(confirm_in(&fixture.registry, "remote", true, Role::Workstation, false, |_, ticket| {
            if ticket == "FORCASTER-6" { Err("finding needs review".into()) } else { Ok(()) }
        }).is_err());
        let group = groups_in(&fixture.registry, None).unwrap().remove(0);
        assert!(group.approved_at.is_none());
        assert!(group.events.is_empty());
        refill_in(&fixture.registry, &fixture.tickets, &fixture, Role::Workstation, false).await.unwrap();
        assert!(fixture.launched.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn refill_isolates_missing_profile_and_recovery_failures_across_restart() {
        for failure in ["profile", "prelaunch", "snapshot"] {
            let mut fixture = RefillFixture::new();
            let reason = match failure {
                "profile" => {
                    std::fs::remove_file(fixture.registry.join("profiles/claude.json")).unwrap();
                    "agent profile @claude is missing"
                }
                "prelaunch" => {
                    fixture.fail_reconcile = Some("FIRST-1".into());
                    "exact pending launch receipt is unreadable"
                }
                _ => {
                    fixture.fail_snapshot = Some("FIRST".into());
                    "project recovery source unavailable"
                }
            };
            refill_in(&fixture.registry, &fixture.tickets, &fixture, crate::instance::Role::Fleet, false)
                .await
                .unwrap();
            let first = groups_in(&fixture.registry, None).unwrap();
            assert_eq!(first[0].members[0].state, MemberState::Blocked);
            assert!(first[0].members[0].reason.contains(reason));
            assert_eq!(first[1].members[0].state, MemberState::Tracking);
            assert!(first[1].members[0].run_id.is_some());
            if failure != "snapshot" {
                assert_eq!(
                    first[0].members[1].state,
                    MemberState::Tracking,
                    "an independent member of the same group still dispatches"
                );
            }
            let blocked_events = first[0]
                .events
                .iter()
                .filter(|e| e.ticket == "FIRST-1")
                .count();
            for _ in 0..2 {
                // Reloading is performed inside the production loop each pass.
                refill_in(&fixture.registry, &fixture.tickets, &fixture, crate::instance::Role::Fleet, false)
                    .await
                    .unwrap();
            }
            let reopened = groups_in(&fixture.registry, None).unwrap();
            assert_eq!(reopened[0].members[0].reason, first[0].members[0].reason);
            assert_eq!(
                reopened[0]
                    .events
                    .iter()
                    .filter(|e| e.ticket == "FIRST-1")
                    .count(),
                blocked_events
            );
            assert_eq!(reopened[1].members[0].run_id, first[1].members[0].run_id);
            let launched = fixture.launched.lock().unwrap();
            assert_eq!(
                launched
                    .iter()
                    .filter(|id| id.as_str() == "SECOND-1")
                    .count(),
                1
            );
            assert!(!launched.contains(&"FIRST-1".into()));
            assert_eq!(
                crate::run_control::list_ids_in(&fixture.registry)
                    .unwrap()
                    .len(),
                launched.len()
            );
        }
    }

    #[tokio::test]
    async fn refill_keeps_shared_native_registry_corruption_fail_closed() {
        let fixture = RefillFixture::new();
        std::fs::write(fixture.registry.join("corrupt.run.json"), b"{broken").unwrap();
        assert!(refill_in(&fixture.registry, &fixture.tickets, &fixture, crate::instance::Role::Fleet, false)
            .await
            .is_err());
        assert!(fixture.launched.lock().unwrap().is_empty());
        assert!(groups_in(&fixture.registry, None)
            .unwrap()
            .iter()
            .flat_map(|group| &group.members)
            .all(|member| member.state == MemberState::Queued));
    }

    #[test]
    fn portable_group_lock_excludes_processes_and_releases_on_drop() {
        crate::run_control::tests::cross_process_lock_fixture(
            "swarm_plan::tests::portable_group_lock_excludes_processes_and_releases_on_drop",
            |root| root.join("swarm-plans/.lock"), |root| GroupLease::acquire(root).unwrap(),
        );
    }
    #[test]
    fn coordinator_lock_excludes_overlapping_refills_and_releases_on_drop() {
        let dir = scratch();
        let first = GroupLease::acquire(&dir).unwrap();
        assert!(GroupLease::acquire(&dir).is_err());
        drop(first);
        assert!(GroupLease::acquire(&dir).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn preview_persists_during_sweep_without_granting_dispatch() {
        let fixture = RefillFixture::five_remote("exe-dev");
        let original = groups_in(&fixture.registry, None).unwrap().remove(0);
        let lease = GroupLease::acquire(&fixture.registry).unwrap();
        let mut preview = original.plan.clone();
        preview.id = "preview-during-sweep".into();
        remember_in(&fixture.registry, preview).unwrap();
        assert!(GroupLease::acquire(&fixture.registry).is_err(), "planning must not release execution's lease");
        let groups = groups_in(&fixture.registry, None).unwrap();
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|g| g.approved_at.is_none()
            && g.members.iter().all(|m| m.state == MemberState::Queued && m.run_id.is_none())));
        refill_in(&fixture.registry, &fixture.tickets, &fixture,
            crate::instance::Role::Workstation, false).await.unwrap();
        assert!(fixture.launched.lock().unwrap().is_empty());
        drop(lease);
        std::fs::remove_dir_all(&fixture.registry).unwrap();
    }

    #[test]
    fn concurrent_previews_cannot_replace_an_approval() {
        let fixture = RefillFixture::five_remote("exe-dev");
        let original = groups_in(&fixture.registry, None).unwrap().remove(0);
        let barrier = std::sync::Barrier::new(8);
        let winners = std::thread::scope(|scope| {
            let attempts: Vec<_> = (0..8).map(|_| scope.spawn(|| {
                let mut plan = original.plan.clone();
                plan.id = "same-preview".into();
                barrier.wait();
                remember_in(&fixture.registry, plan)
            })).collect();
            attempts.into_iter().map(|attempt| attempt.join().unwrap())
                .filter(Result::is_ok).count()
        });
        assert_eq!(winners, 1, "exactly one complete preview must be published");
        let path = path_in(&fixture.registry, "same-preview").unwrap();
        let mut group: Group = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        approve(&mut group, 42);
        save_in(&fixture.registry, &group).unwrap();
        let approved_bytes = std::fs::read(&path).unwrap();
        let mut replacement = group.plan.clone();
        replacement.runs.clear();
        assert!(remember_in(&fixture.registry, replacement).unwrap_err().contains("approval cannot be replaced"));
        assert_eq!(std::fs::read(&path).unwrap(), approved_bytes);
        assert!(std::fs::read_dir(fixture.registry.join("swarm-plans")).unwrap()
            .all(|entry| entry.unwrap().path().extension().is_none_or(|ext| ext != "tmp")));
        std::fs::remove_dir_all(&fixture.registry).unwrap();
    }
    #[test]
    fn two_active_workers_leave_third_queued_then_refill_one_slot() {
        let dir = scratch();
        let tickets: Vec<_> = (1..=3)
            .map(|n| ticket(&format!("XNAUT-{n}"), "XNAUT", "ready", Some("claude")))
            .collect();
        let plan = plan_from(
            "capacity",
            "XNAUT",
            &[],
            &board(&tickets, &models(), &HashSet::new()),
            2,
            0,
        )
        .unwrap();
        remember_in(&dir, plan).unwrap();
        let mut group = groups_in(&dir, None).unwrap().remove(0);
        approve(&mut group, 1);
        for i in 0..2 {
            transition(
                &mut group,
                i,
                MemberState::Tracking,
                "native worker".into(),
                Some(format!("run-{i}")),
                2,
            );
        }
        save_in(&dir, &group).unwrap();
        let restarted = groups_in(&dir, None).unwrap().remove(0);
        assert!(!slot_available(
            &restarted.members[2],
            2,
            restarted.plan.max_parallel
        ));
        assert!(slot_available(
            &restarted.members[2],
            1,
            restarted.plan.max_parallel
        ));
        assert!(
            !slot_available(&restarted.members[0], 1, restarted.plan.max_parallel),
            "completed slot does not redispatch tracked work"
        );
        assert_eq!(report(&restarted).queued, vec!["XNAUT-3"]);
        stop(&mut group, 3);
        let events = group.events.len();
        stop(&mut group, 4);
        approve(&mut group, 5);
        assert_eq!(group.stopped_at, Some(3));
        assert_eq!(group.events.len(), events);
        assert!(!slot_available(&group.members[2], 0, 2));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn native_scope_receipt_accepts_only_its_exact_body_not_later_appended_instructions() {
        let dir = scratch();
        let original = ticket("XNAUT-1", "XNAUT", "ready", Some("claude"));
        let plan = plan_from(
            "receipt",
            "XNAUT",
            &[],
            &board(std::slice::from_ref(&original), &models(), &HashSet::new()),
            2,
            0,
        )
        .unwrap();
        remember_in(&dir, plan).unwrap();
        let mut group = groups_in(&dir, None).unwrap().remove(0);
        let mut dispatched = original.clone();
        dispatched
            .body
            .push_str("\n\n## Dispatched today\nNative receipt");
        assert!(!authorized_member(
            &dir,
            &group.plan.runs[0],
            &group.members[0],
            &dispatched
        ));
        group.members[0].dispatched_scope = Some(scope(&dispatched));
        assert!(authorized_member(
            &dir,
            &group.plan.runs[0],
            &group.members[0],
            &dispatched
        ));
        dispatched
            .body
            .push_str("\nAlso change the billing service");
        assert!(!authorized_member(
            &dir,
            &group.plan.runs[0],
            &group.members[0],
            &dispatched
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
