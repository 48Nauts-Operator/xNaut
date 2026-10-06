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
    /// The model that owner's profile launches with.
    ///
    /// Per OWNER, never per swarm. The pane had one dropdown that overrode the
    /// model for every run in the batch, which is how a runtime gets launched
    /// with a name its CLI has never heard of (XNAUT-266). The profile already
    /// answers this question for the fleet; the swarm asks it rather than
    /// keeping a second answer.
    pub model: String,
    pub branch: String,
    pub scope: String,
    #[serde(default)]
    pub repository_root: Option<String>,
    #[serde(default)]
    pub runtime_id: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
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
            scope: scope(ticket),
            repository_root: None,
            runtime_id: None,
            environment: None,
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
    use std::io::Write;
    let path = path_in(registry, &group.plan.id)?;
    std::fs::create_dir_all(path.parent().ok_or("missing group directory")?)
        .map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(group).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
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

fn remember_in(registry: &Path, plan: SwarmPlan) -> Result<(), String> {
    let _lease = GroupLease::acquire(registry)?;
    let path = path_in(registry, &plan.id)?;
    if path.exists() {
        return Err("plan id already exists; approval cannot be replaced".into());
    }
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
    save_in(
        registry,
        &Group {
            plan,
            approved_at: None,
            stopped_at: None,
            members,
            events: Vec::new(),
        },
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
    let environment = crate::sandbox::launch_env::resolve(
        profile.execution.pinned_environment(),
        &crate::settings::load_or_default().sandboxes,
    );
    Ok(!p.owner_only
        && pins_match(
            run,
            root.to_str(),
            &p.forge_remote,
            &profile.model,
            &profile.runtime_id,
            environment.key(),
        ))
}
fn pins_match(
    run: &PlannedRun,
    root: Option<&str>,
    remote: &str,
    model: &str,
    runtime: &str,
    environment: &str,
) -> bool {
    run.repository_root.as_deref() == root
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
        if let Some(group) = groups
            .iter()
            .filter(|g| g.approved_at.is_some() && g.members.iter().any(|m| m.ticket == ticket))
            .max_by_key(|g| g.approved_at)
        {
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
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("group coordinator is already advancing; retry next sweep".into());
            }
        }
        Ok(Self(file))
    }
}
impl Drop for GroupLease {
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

/// Read the board, the profiles and the registry, and build a plan under
/// NautBot's cap. Nothing is stored: the caller decides whether this plan is
/// worth asking about.
pub fn build(project: &str, requested: &[String]) -> Result<SwarmPlan, String> {
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
    for run in &mut plan.runs {
        let profile = crate::agent_profiles::agent_profile_get(run.owner.clone())?;
        run.repository_root = Some(root.clone());
        run.runtime_id = Some(profile.runtime_id.clone());
        run.environment = Some(
            crate::sandbox::launch_env::resolve(
                profile.execution.pinned_environment(),
                &settings.sandboxes,
            )
            .key()
            .into(),
        );
        run.repository_remote = projects
            .iter()
            .find(|p| p.key == project)
            .map(|p| p.forge_remote.clone());
    }
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmDispatched {
    pub plan_id: String,
    pub project: String,
    pub started: Vec<Started>,
    pub failed: Vec<Skipped>,
    pub queued: Vec<String>,
}

fn report(group: &Group) -> SwarmDispatched {
    SwarmDispatched {
        plan_id: group.plan.id.clone(),
        project: group.plan.project.clone(),
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
    if crate::switches::load().read_only {
        return Err("the read_only kill-switch is engaged".into());
    }
    let registry = crate::agents::registry_dir()?;
    {
        let _lease = GroupLease::acquire(&registry)?;
        let path = path_in(&registry, plan_id)?;
        let mut group: Group =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if group.stopped_at.is_some() {
            return Err("group was stopped; a new exact scope approval is required".into());
        }
        approve(&mut group, crate::run_control::now_ms());
        save_in(&registry, &group)?;
    }
    refill(&app).await?;
    groups_in(&registry, None)?
        .iter()
        .find(|g| g.plan.id == plan_id)
        .map(report)
        .ok_or_else(|| "plan disappeared".into())
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

fn slot_available(member: &Member, live: usize, cap: usize) -> bool {
    member.state == MemberState::Queued && live < cap
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
            if let Ok(Some(next)) = crate::run_control::continuation_in(registry, ticket) {
                if crate::run_control::prelaunch_refused(&next)
                    && crate::agent_work::recovery_guard(&serde_json::json!(snapshot), ticket, Some(&next)).is_ok()
                {
                    if assignments.len() >= 3 {
                        transition(group, i, MemberState::Blocked,
                            "prelaunch retry limit reached; inspect preserved staging evidence before further dispatch".into(),
                            Some(next.run_id), now);
                        return true;
                    }
                    transition(group, i, MemberState::Queued,
                        "native prelaunch refusal proved no worker started; retry preserved workspace".into(),
                        Some(next.run_id), now);
                    return false;
                }
            }
        }
        if group.members[i].started.is_none()
            && assignments
                .iter()
                .filter_map(|a| crate::run_control::load_manifest_in(registry, &a.run_id).ok())
                .any(|r| {
                    assignments.iter().all(|a| a.run_id == r.run_id)
                        && crate::run_control::initial_admission_refused(&r)
                        && r.last_signal
                            .starts_with("admission failed: worker capacity:")
                })
        {
            transition(
                group,
                i,
                MemberState::Queued,
                "capacity refusal proved no worker started; retry existing native continuation"
                    .into(),
                None,
                now,
            );
            return false;
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

pub(crate) async fn refill(app: &tauri::AppHandle) -> Result<(), String> {
    if crate::switches::load().read_only || !crate::instance::role().dispatches() {
        return Ok(());
    }
    let registry = crate::agents::registry_dir()?;
    let _lease = GroupLease::acquire(&registry)?;
    let repo = crate::project_management::repo_now()?;
    let tickets = crate::project_management::ticket_list_in(&repo, None)?;
    let projects = crate::project_management::list_projects(&repo)?;
    for mut group in groups_in(&registry, None)?
        .into_iter()
        .filter(|g| g.approved_at.is_some() && g.stopped_at.is_none())
    {
        for member in &group.members {
            crate::agent_work::reconcile_prelaunch(&group.plan.project, &member.ticket)?;
        }
        let snapshot = crate::project_continuity::snapshot(&group.plan.project)?;
        for i in 0..group.members.len() {
            let run = group.plan.runs[i].clone();
            let now = crate::run_control::now_ms();
            let current = tickets
                .iter()
                .find(|t| t.id == run.ticket && t.project == group.plan.project);
            let project_allowed = projects.iter().any(|p| {
                p.key == group.plan.project
                    && !p.owner_only
                    && Path::new(crate::project_management::local_source_path(p).trim())
                        .canonicalize()
                        .ok()
                        .is_some_and(|root| run.repository_root.as_deref() == root.to_str())
            });
            if !project_allowed
                || !current_pins(&run, &group.plan.project)?
                || current.is_none_or(|t| !authorized_member(&registry, &run, &group.members[i], t))
            {
                let id = group.members[i].run_id.clone();
                transition(
                    &mut group,
                    i,
                    MemberState::Blocked,
                    "approved owner, scope or project policy changed".into(),
                    id,
                    now,
                );
                save_in(&registry, &group)?;
                continue;
            }
            if recover_member(&registry, &mut group, i, &snapshot, now) {
                save_in(&registry, &group)?;
                continue;
            }
            if group.members[i].state != MemberState::Queued {
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
                save_in(&registry, &group)?;
                continue;
            }
            let profile_cap = crate::agent_profiles::agent_profile_get(
                crate::agent_profiles::RESERVED_NAUTBOT_HANDLE.into(),
            )
            .map(|p| p.max_parallel as usize)
            .unwrap_or(DEFAULT_MAX_PARALLEL);
            if !slot_available(
                &group.members[i],
                WorkerGroupScope::new(&group, &crate::repository_transfer::list()?)
                    .count(&registry)?,
                group.plan.max_parallel.min(profile_cap.clamp(1, HARD_CAP)),
            ) {
                transition(
                    &mut group,
                    i,
                    MemberState::Queued,
                    "approved; waiting for capacity".into(),
                    None,
                    now,
                );
                save_in(&registry, &group)?;
                continue;
            }
            if let Err(refusal) =
                crate::dispatch::automatic_admission(&run.ticket, &group.plan.project)
            {
                let state = if refusal.retryable() {
                    MemberState::Queued
                } else {
                    MemberState::Blocked
                };
                group.members[i].refusal = Some(refusal.clone());
                transition(&mut group, i, state, refusal.reason, None, now);
                save_in(&registry, &group)?;
                continue;
            }
            if !crate::agent_profiles::agent_profile_get(run.owner.clone())
                .is_ok_and(|p| p.model == run.model)
            {
                transition(
                    &mut group,
                    i,
                    MemberState::Blocked,
                    "approved owner profile/model changed".into(),
                    None,
                    now,
                );
                save_in(&registry, &group)?;
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
            save_in(&registry, &group)?;
            match crate::dispatch::dispatch_scoped(
                app.clone(),
                run.ticket.clone(),
                group.plan.project.clone(),
                None,
                Some(&run),
            )
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
            save_in(&registry, &group)?;
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
/// The same door NautBot's `swarm_dispatch` tool goes through, because the
/// owner can answer either way — press the button or say yes — and two
/// implementations of "start the batch" is how the two would drift apart.
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
            "gitvm"
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
                    }
                ),
                "{field}"
            );
        }
        let mut legacy = run.clone();
        legacy.runtime_id = None;
        assert!(!pins_match(
            &legacy,
            Some("/approved/repository"),
            "ssh://forge/team/repo.git",
            &run.model,
            "claude-code",
            "gitvm"
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
        for attempt in 0..3 {
            let mut run = RunManifest::requested("claude", "fixture", "/preserved", Some(task.id.clone()), None, &[], 10 + attempt);
            run.branch = "agent/preserved".into();
            run_control::bind_pending_in(&dir, &mut run).unwrap();
            run_control::refuse_prelaunch_in(&dir, run, PrelaunchPhase::RepositoryStaging, "fixture staging failure").unwrap();
            runs = run_control::list_ids_in(&dir).unwrap().iter().map(|id| run_control::load_manifest_in(&dir, id).unwrap()).collect();
            let snapshot = crate::project_continuity::reconcile("XNAUT", &[task.clone()], &runs, &[], 100);
            group.members[0].state = MemberState::Blocked;
            if attempt < 2 {
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
        assert_eq!(runs.len(), 3);
        assert!(runs.iter().all(|r| r.branch == "agent/preserved" && r.worktree_path == "/preserved"));
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
    fn coordinator_lock_excludes_overlapping_refills_and_releases_on_drop() {
        let dir = scratch();
        let first = GroupLease::acquire(&dir).unwrap();
        assert!(GroupLease::acquire(&dir).is_err());
        drop(first);
        assert!(GroupLease::acquire(&dir).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
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
