//! The swarm NautBot offers (XNAUT-354).
//!
//! There were two ways to put agents on a batch of tickets. The Multi-Agent
//! Manager pane (July) planned a swarm in its own chat and fired `loom_run`
//! directly: no run registry, no jury, no ledger, its own model dropdown and
//! its own max-parallel box. NautBot started runs through `dispatch.rs`, which
//! has all of that. Same intent, and the older one outside every control.
//!
//! So the planning moves here and the starting stays in `dispatch.rs`. What
//! this module is actually for is the gap between the two: a batch is a
//! decision the owner makes ONCE, and the thing they confirm has to be the
//! thing that runs. A plan is therefore built, shown, and then CONSUMED —
//! [`take`] removes it — so a confirm arriving twice, from the card and from
//! the chat, cannot start the same eight agents twice. The pane had the same
//! hazard and answered it by nulling a variable; here it is the only way to
//! read a plan back.
//!
//! Every refusal is a NAMED skip rather than a silent drop. The pane asked an
//! LLM not to invent tickets and mostly got its way; this asks the board, and
//! says out loud why each ticket it was handed is not in the plan. A swarm
//! that quietly runs four of the six tickets you named is worse than one that
//! refuses, because nothing on screen says which two are missing.
//!
//! Not a checkbox, deliberately (André, 2026-09-13): a mode set earlier
//! changes what a later sentence means, so there is no swarm toggle anywhere.
//! A request that spans more than one ticket produces a plan and a question; a
//! request naming one ticket goes to `dispatch_ticket` exactly as it did
//! before, and is never asked about.

use crate::project_management::TicketRecord;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
}

/// A ticket that was asked for and is not in the plan, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub ticket: String,
    pub reason: String,
}

/// A batch of runs, validated, waiting for a yes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
        // The cap is checked HERE, after every other reason, so a ticket that
        // could never have run is named for what is actually wrong with it
        // rather than for being eleventh in a queue of ten.
        if runs.len() >= cap {
            let reason = format!("over the {cap}-run cap on @nautbot's profile");
            skipped.push(Skipped { ticket: id, reason });
            continue;
        }
        runs.push(PlannedRun {
            branch: crate::dispatch::branch_for(&owner, &ticket.id),
            ticket: ticket.id.clone(),
            title: ticket.title.clone(),
            owner,
            model: model.clone(),
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

/// How many plans are kept waiting.
///
/// A plan is a question, not a record: the ledger holds what was dispatched.
/// Bounded so a conversation that proposes twenty swarms and confirms none
/// cannot grow without end.
const MAX_PLANS: usize = 16;

fn plans() -> &'static Mutex<Vec<SwarmPlan>> {
    static PLANS: OnceLock<Mutex<Vec<SwarmPlan>>> = OnceLock::new();
    PLANS.get_or_init(|| Mutex::new(Vec::new()))
}

fn held() -> std::sync::MutexGuard<'static, Vec<SwarmPlan>> {
    plans()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A plan id nothing else will mint.
pub fn new_id() -> String {
    format!("swarm-{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

/// Keep a plan until somebody answers it.
pub fn remember(plan: SwarmPlan) {
    let mut store = held();
    store.retain(|kept| kept.id != plan.id);
    store.push(plan);
    let over = store.len().saturating_sub(MAX_PLANS);
    if over > 0 {
        store.drain(..over);
    }
}

/// Read a plan back without answering it — what the card renders from.
pub fn peek(id: &str) -> Option<SwarmPlan> {
    held().iter().find(|plan| plan.id == id).cloned()
}

/// The plan, REMOVED.
///
/// Taking is the only way to read a plan for dispatch, so a yes from the card
/// and a yes in the chat cannot both start the batch. The second one finds
/// nothing and says so.
pub fn take(id: &str) -> Option<SwarmPlan> {
    let mut store = held();
    let at = store.iter().position(|plan| plan.id == id)?;
    Some(store.remove(at))
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

    plan_from(
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
    )
}

/// One run a confirmed swarm actually started.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Started {
    pub ticket: String,
    pub handle: String,
    pub branch: String,
    pub worktree_path: String,
    pub session_id: String,
}

/// What a confirm did. `failed` is not an error: the runs before it are
/// already working, and a caller that saw only an error would not know that.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SwarmDispatched {
    pub plan_id: String,
    pub project: String,
    pub started: Vec<Started>,
    pub failed: Vec<Skipped>,
}

/// Hand every run in a confirmed plan to `dispatch.rs`.
///
/// Sequential on purpose. Dispatch returns as soon as the agent is launched,
/// so the batch is concurrent regardless; running the LAUNCHES in parallel
/// would only race the worktree creation, which is the one part of this that
/// touches the same git repository from several places at once.
pub async fn dispatch_plan(
    app: tauri::AppHandle,
    plan_id: &str,
) -> Result<SwarmDispatched, String> {
    let plan = take(plan_id)
        .ok_or_else(|| format!("no swarm plan {plan_id} is waiting — plan it again"))?;
    if plan.runs.is_empty() {
        return Err(format!("swarm plan {plan_id} has no runs in it"));
    }
    let mut started = Vec::new();
    let mut failed = Vec::new();
    for run in &plan.runs {
        match crate::dispatch::pm_ticket_dispatch(
            app.clone(),
            run.ticket.clone(),
            plan.project.clone(),
        )
        .await
        {
            Ok(result) => {
                crate::ledger::record(
                    "swarm_dispatch",
                    crate::agent_profiles::RESERVED_NAUTBOT_HANDLE,
                    &result.ticket_id,
                    &format!("{plan_id}: @{} on {}", result.handle, result.branch),
                );
                started.push(Started {
                    ticket: result.ticket_id,
                    handle: result.handle,
                    branch: result.branch,
                    worktree_path: result.worktree_path,
                    session_id: result.session_id,
                });
            }
            Err(error) => {
                crate::ledger::record(
                    "swarm_dispatch_failed",
                    crate::agent_profiles::RESERVED_NAUTBOT_HANDLE,
                    &run.ticket,
                    &format!("{plan_id}: {error}"),
                );
                failed.push(Skipped {
                    ticket: run.ticket.clone(),
                    reason: error,
                });
            }
        }
    }
    Ok(SwarmDispatched {
        plan_id: plan.id,
        project: plan.project,
        started,
        failed,
    })
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
    fn the_cap_bounds_the_plan_and_names_what_it_dropped() {
        let tickets: Vec<TicketRecord> = (1..=5)
            .map(|n| ticket(&format!("XNAUT-{n}"), "XNAUT", "ready", Some("claude")))
            .collect();
        let (models, live) = (models(), HashSet::new());
        let plan = plan_from("p1", "XNAUT", &[], &board(&tickets, &models, &live), 3, 0).unwrap();
        assert_eq!(plan.runs.len(), 3);
        assert_eq!(plan.max_parallel, 3);
        assert_eq!(plan.skipped.len(), 2);
        assert_eq!(
            reason_for(&plan, "XNAUT-4"),
            "over the 3-run cap on @nautbot's profile"
        );

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
        assert_eq!(offer(&throttled), Offer::Single("XNAUT-1".into()));
        assert_eq!(
            reason_for(&throttled, "XNAUT-3"),
            "over the 1-run cap on @nautbot's profile"
        );

        assert_eq!(
            offer(&plan_from("p4", "XNAUT", &[], &board, 10, 0).unwrap()),
            Offer::Swarm
        );
        let none = plan_from("p5", "XNAUT", &["XNAUT-404".into()], &board, 10, 0).unwrap();
        assert_eq!(offer(&none), Offer::Nothing);
    }

    #[test]
    fn a_plan_is_consumed_by_the_first_yes() {
        // The card and the chat can both say yes. Only one batch may start.
        let plan = SwarmPlan {
            id: "swarm-take-me".into(),
            project: "XNAUT".into(),
            runs: vec![],
            skipped: vec![],
            max_parallel: 3,
            created_at: 0,
        };
        remember(plan.clone());
        assert_eq!(peek("swarm-take-me").as_ref(), Some(&plan));
        assert_eq!(take("swarm-take-me").as_ref(), Some(&plan));
        assert_eq!(take("swarm-take-me"), None, "the second yes found a plan");
        assert_eq!(peek("swarm-take-me"), None);

        // Remembering the same id twice keeps one, and the store is bounded.
        for n in 0..(MAX_PLANS + 4) {
            remember(SwarmPlan {
                id: format!("swarm-bound-{n}"),
                ..plan.clone()
            });
        }
        assert!(plans().lock().unwrap().len() <= MAX_PLANS);
        assert!(
            peek("swarm-bound-0").is_none(),
            "the oldest plan should have been evicted"
        );
        for n in 0..(MAX_PLANS + 4) {
            take(&format!("swarm-bound-{n}"));
        }
        assert_eq!(new_id().len(), "swarm-".len() + 8);
        assert_ne!(new_id(), new_id());
    }
}
