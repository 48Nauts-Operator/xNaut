//! Dispatch-from-ticket (XNAUT-153).
//!
//! One action turns a PM ticket into a working agent: a worktree off the live
//! lineage, the assigned profile launched inside it with the ticket and every
//! linked spec doc, and the ticket moved to `in_progress` with the branch,
//! worktree and session recorded on it.
//!
//! What Dispatch deliberately does NOT do: block until the tests pass and move
//! the ticket to review itself. There is no agent session-end signal in the
//! backend, so nothing here can know when the harness finished. The agent runs
//! the suite, writes the bundle and moves the ticket, exactly as the loop was
//! run by hand six times on 2026-08-14. The prompt below is what tells it to.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

#[derive(Clone, Debug, Serialize)]
pub struct DispatchResult {
    pub ticket_id: String,
    pub handle: String,
    pub branch: String,
    pub worktree_path: String,
    pub session_id: String,
    pub run_id: Option<String>,
    pub environment: String,
    #[serde(skip)]
    pub(crate) ticket_scope: String,
}

/// Automatic callers preserve ownership on every refusal. Only a capacity gate
/// proven before native launch is retryable; an unknown launch error is uncertain.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefusalKind {
    Policy,
    Capacity,
    RepositoryAccess,
    ExistingAssignment,
    Uncertain,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispatchRefusal {
    pub kind: RefusalKind,
    pub reason: String,
}
impl DispatchRefusal {
    pub fn retryable(&self) -> bool {
        self.kind == RefusalKind::Capacity
    }
    pub fn uncertain(reason: String) -> Self {
        Self {
            kind: RefusalKind::Uncertain,
            reason,
        }
    }
}

pub(crate) fn automatic_admission(ticket: &str, project: &str) -> Result<(), DispatchRefusal> {
    if crate::switches::load().read_only || !crate::instance::role().dispatches() {
        return Err(DispatchRefusal {
            kind: RefusalKind::Policy,
            reason: "read_only or instance role prohibits automatic dispatch".into(),
        });
    }
    ticket_admission(ticket, project)
}

/// An approved remote group is controlled from the owner's workstation, while
/// its workers execute at the pinned provider. This does not authorize the
/// workstation to pick up arbitrary tickets or start local workers.
pub(crate) fn approved_dispatch_policy(
    role: crate::instance::Role,
    read_only: bool,
    environment: Option<&str>,
) -> Result<(), DispatchRefusal> {
    use crate::{instance::Role, sandbox::launch_env::LaunchEnv};
    let reason = if read_only {
        Some("the read_only kill-switch is engaged")
    } else {
        match role {
            Role::Fleet => None,
            Role::Workstation if matches!(environment.and_then(LaunchEnv::from_key),
                Some(LaunchEnv::ExeDev | LaunchEnv::GitVm)) => None,
            Role::Workstation => Some("This plan requires local execution. Choose exe.dev or GitVM to run approved work remotely from this workstation."),
            Role::Sandbox => Some("This sandbox verifies work; it cannot dispatch a swarm. Approve remote work from the owner's workstation or a Fleet instance."),
        }
    };
    match reason {
        Some(reason) => Err(DispatchRefusal { kind: RefusalKind::Policy, reason: reason.into() }),
        None => Ok(()),
    }
}

pub(crate) fn approved_group_admission(
    ticket: &str, project: &str, environment: Option<&str>,
) -> Result<(), DispatchRefusal> {
    approved_dispatch_policy(crate::instance::role(), crate::switches::load().read_only, environment)?;
    ticket_admission(ticket, project)
}

fn ticket_admission(ticket: &str, project: &str) -> Result<(), DispatchRefusal> {
    let refuse = |kind, reason| DispatchRefusal { kind, reason };
    if let Some(reason) = crate::housekeeper::launch_floor() {
        return Err(refuse(RefusalKind::Capacity, reason));
    }
    let registry = crate::agents::registry_dir().map_err(DispatchRefusal::uncertain)?;
    let repo = crate::project_management::repo_now().map_err(DispatchRefusal::uncertain)?;
    let tickets = crate::project_management::ticket_list_in(&repo, Some(project.to_string()))
        .map_err(DispatchRefusal::uncertain)?;
    let current = tickets
        .iter()
        .find(|t| t.id == ticket)
        .ok_or_else(|| refuse(RefusalKind::Policy, "ticket missing from project".into()))?;
    let projects =
        crate::project_management::list_projects(&repo).map_err(DispatchRefusal::uncertain)?;
    if current.approval.owner_only
        || current.tags.iter().any(|t| t == "no-auto-dispatch")
        || projects
            .iter()
            .find(|p| p.key == project)
            .is_none_or(|p| p.owner_only)
    {
        return Err(refuse(
            RefusalKind::Policy,
            "owner-only or no-auto-dispatch policy".into(),
        ));
    }
    let snapshot =
        crate::project_continuity::snapshot(project).map_err(DispatchRefusal::uncertain)?;
    let continuation = crate::run_control::continuation_in(&registry, ticket)
        .map_err(DispatchRefusal::uncertain)?;
    crate::agent_work::recovery_guard(
        &serde_json::to_value(snapshot).map_err(|e| DispatchRefusal::uncertain(e.to_string()))?,
        ticket,
        continuation.as_ref(),
    )
    .map_err(|reason| refuse(RefusalKind::ExistingAssignment, reason))?;
    let live =
        crate::run_control::live_tickets_in(&registry).map_err(DispatchRefusal::uncertain)?;
    crate::spend::would_admit(live.len()).map_err(|reason| refuse(RefusalKind::Capacity, reason))
}

pub(crate) async fn automatic_dispatch(
    app: tauri::AppHandle,
    ticket: String,
    project: String,
) -> Result<DispatchResult, DispatchRefusal> {
    automatic_admission(&ticket, &project)?;
    let repo = crate::project_management::repo_now().map_err(DispatchRefusal::uncertain)?;
    let current = crate::project_management::ticket_list_in(&repo, Some(project.clone()))
        .map_err(DispatchRefusal::uncertain)?
        .into_iter()
        .find(|t| t.id == ticket)
        .ok_or_else(|| DispatchRefusal::uncertain("ticket disappeared".into()))?;
    crate::ticket_triage::dispatch_admission(&current).map_err(|reason| DispatchRefusal {
        kind: RefusalKind::Policy,
        reason,
    })?;
    model_ticket_dispatch(app, ticket, project, None)
        .await
        .map_err(DispatchRefusal::uncertain)
}

/// Every linked vault doc, inlined. An unreadable or non-`work:` reference is
/// listed rather than fatal: a ticket carrying an `obsidian:` pointer is still
/// worth dispatching.
fn linked_docs(refs: &[String]) -> String {
    const BUDGET: usize = 60_000;
    let Ok(vault) = crate::vault::vault_init() else {
        return String::new();
    };
    let root = PathBuf::from(vault).join("work");
    let mut out = String::new();
    for reference in refs {
        let Some(rel) = reference.strip_prefix("work:") else {
            out.push_str(&format!("\n### {reference}\n\nNot a work-vault reference; open it yourself if you need it.\n"));
            continue;
        };
        let body = crate::vault::safe_join(&root, rel)
            .and_then(|path| std::fs::read_to_string(&path).map_err(|e| e.to_string()));
        match body {
            Ok(text) => {
                let room = BUDGET.saturating_sub(out.len());
                if room == 0 {
                    break;
                }
                let text = if text.len() > room { &text[..room] } else { &text };
                out.push_str(&format!("\n### {reference}\n\n{text}\n"));
            }
            Err(error) => out.push_str(&format!("\n### {reference}\n\nCould not be read: {error}\n")),
        }
    }
    out
}

/// The branch a fresh dispatch of `ticket` to `@handle` works on.
///
/// One spelling, because a swarm plan shows the branch on the card BEFORE
/// dispatch creates it (XNAUT-354). A second copy of this format string would
/// pass every test and put a different branch on the card than in the repo.
pub fn branch_for(handle: &str, ticket_id: &str) -> String {
    format!("agent/{handle}/{}", ticket_id.to_ascii_lowercase())
}

/// The branch THIS ticket dispatches onto.
///
/// One spelling for the same reason `branch_for` is one: a card shows the
/// branch before dispatch creates it. A core-team finding tagged for a PoC
/// goes onto `poc/<slug>` rather than `agent/<handle>/<id>` (XNAUT-357),
/// because the branch names the experiment and not whoever happened to be
/// free that week — and the acceptance criterion asks for it by name.
pub fn branch_for_ticket(ticket: &crate::project_management::TicketRecord, handle: &str) -> String {
    crate::core_team::poc_branch_for(ticket).unwrap_or_else(|| branch_for(handle, &ticket.id))
}

/// True when the ticket's branch already carries work: a re-dispatch, whoever
/// ran it before. The prompt then says CONTINUE, and the agent reads the
/// ticket's own notes and handback for what was done, so any runtime picks up
/// where the last one stopped (the whole point: an Anthropic outage hands the
/// task to Codex, and the log in the ticket is the handover).
fn branch_has_history(repo: &std::path::Path, branch: &str) -> bool {
    crate::worktree::git_command()
        .args(["-C", &repo.to_string_lossy(), "rev-list", "--count", &format!("HEAD..{branch}")])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().parse::<u32>().unwrap_or(0) > 0)
        .unwrap_or(false)
}

// XNAUT-465: the xNAUT product's release proof format is not a contract for
// every project. Other projects use their own checked-in verification plan.
fn verification_contract(ticket: &crate::project_management::TicketRecord) -> (String, String) {
    if ticket.project.eq_ignore_ascii_case("XNAUT") {
        (
            "Both suites green: `cargo test --manifest-path src-tauri/Cargo.toml` and `XNAUT_TEST_PORT=4291 npx playwright test`. Zero failures. A test you skipped or ignored to get there does not count.".into(),
            format!("`.xnaut/bundles/{}.md` exists in the worktree with what changed, both suite totals, how to verify by hand, and one line exactly of the form `XNAUT_TEST_TOTALS={{\"rust\":[{{\"passed\":N,\"failed\":0,\"ignored\":M}}],\"ui\":[K]}}` with the numbers from your own runs. Sign-off compares it to the sandbox record and refuses on a mismatch.", ticket.id),
        )
    } else {
        (
            "Use the project's verification plan at the checked-out revision: explicit `.xnaut/verify.json` takes precedence; when absent, the native verifier supports Node auto-detection from `package.json`. Run the resolved required commands and the ticket's acceptance checks. Zero failures. Preserve command, exit code, and log evidence. Missing tools or configuration are explicit verification gaps; never substitute another project's suites or invent passing totals.".into(),
            "Record what changed, the actual project checks and their results, how to verify by hand, and any unresolved requirements. Follow this run's supplied delivery contract for artifact location and structured handback.".into(),
        )
    }
}

/// Shared by initial dispatch and same-task repair. A historical generated
/// prompt is not authority to add another project's delivery obligations.
pub(crate) fn project_author_guidance(project: &str) -> &'static str {
    if project.eq_ignore_ascii_case("XNAUT") {
        "Preserve the original xNAUT ticket's suite, totals bundle, design-document and authorized PM handoff requirements."
    } else {
        "Use this project's checked-in verification plan and the ticket's explicit acceptance and documentation requirements. Historical generated completion examples from other projects do not add requirements. For a repository-delivered remote run, publish the structured handback and evidence under the supplied run artifact directory; the desktop imports the ticket handoff. Do not invent a separate totals bundle, create a work-vault document, or call desktop ticket endpoints unless the actual ticket/project contract explicitly requires and authorizes it. Preserve prior handbacks and explain any corrected interpretation in the new handback; never erase historical claims."
    }
}

fn delivery_contract(ticket: &crate::project_management::TicketRecord) -> String {
    if !ticket.project.eq_ignore_ascii_case("XNAUT") {
        return format!("- {}\n- Commit the authorized work and file its structured handback using the supplied delivery contract. Report unfinished requirements explicitly; do not claim independent verification, merge or release authority.\n", project_author_guidance(&ticket.project));
    }
    format!("- The ticket's design document in the work vault{doc_targets} carries a dated \
           `## Shipped {id}` section: what was done, how, the files, the key code in snippets \
           of at most 30 lines, the totals, and what is deliberately not done. Use \
           `xnaut_read_document` then `xnaut_update_document` (project `{project}`). No linked \
           document: create `Development/features/YYYY-MM-DD_{id}.md` with the frontmatter \
           `Author` and `Last modified`, and add it to the ticket's documentation. Sign-off \
           reads it.\n\
         - Everything is committed, and you move {id} to `done` with the bundle path and a \
           summary appended to its body. `done` hands it to NautBot, who tests it. \
           `complete` is NautBot's word; never set it.\n",
        id=ticket.id, project=ticket.project, doc_targets=doc_targets(&ticket.documentation))
}

fn dispatch_prompt(
    ticket: &crate::project_management::TicketRecord,
    docs: &str,
    poc_minutes: u64,
) -> String {
    let (verification, bundle) = verification_contract(ticket);
    let delivery = delivery_contract(ticket);
    let blocked = if ticket.project.eq_ignore_ascii_case("XNAUT") {
        "Blocked means one line on the ticket saying what, before you stop. Silence reads as abandoned."
    } else {
        "If blocked, name the missing prerequisite and unfinished work in the structured handback using this run's delivery contract. Publication of that report does not complete the implementation."
    };
    // Two agents on tron (XNAUT-303 and 255) spent their run trying to ssh to
    // tron, and reported the rig unreachable. Tell them where they stand.
    let host = match crate::run_control::hostname() {
        h if h.is_empty() => "this machine".to_string(),
        h => h,
    };
    // Form follows Cursor's harness findings (2026-09-10): constraints instead
    // of instructions, no numbered checklist, nothing that explains
    // engineering to the model, and a number wherever an adjective was doing
    // a number's job. Every string a gate parses is unchanged.
    format!(
        "You have been dispatched on {id} ({priority} {kind}).\n\n\
         # {title}\n\n{body}\n{poc}\n\
         ## Linked documents\n{docs}\n\n\
         {recall}\
         ## Where you are\n\n\
         Inside a worktree on your own branch, on the machine `{host}`. Work there. \
         If a ticket names this machine, that is where you are: nothing to ssh to.\n\n\
         ## What done means\n\n\
         {id} is done when every line below is true, and not before.\n\n\
         - {verification}\n\
         - No TODOs. No partial implementations. Nothing left in the code for somebody else \
           to finish.\n\
         - {bundle}\n\
         {delivery}\n\
         {blocked}\n",
        id = ticket.id,
        priority = ticket.priority,
        kind = ticket.ticket_type,
        title = ticket.title,
        body = ticket.body,
        docs = if docs.trim().is_empty() { "\nNone linked.\n" } else { docs },
        recall = recall_for(ticket),
        poc = crate::core_team::poc_brief_for(ticket, poc_minutes),
    )
}

/// What xNAUT remembers about this ticket and its area, for the top of the
/// prompt (XNAUT-331). The ticket's own story first, then the newest notes
/// whose words match its title. Empty when nothing is known, so a fresh
/// project carries no empty heading. Never fails the dispatch.
pub fn recall_for(ticket: &crate::project_management::TicketRecord) -> String {
    let Ok(root) = crate::memory::default_root() else { return String::new() };
    let Ok(idx) = crate::memory::index(&root) else { return String::new() };
    if idx.is_empty() { return String::new(); }
    let mut block = crate::memory::recall_block(&root, &idx, &ticket.id, &[], 6);
    let related: Vec<String> = crate::memory::find(&idx, &ticket.title, Some(&ticket.project), 4)
        .into_iter()
        .filter(|e| e.ticket != ticket.id)
        .map(|e| format!("- [{} {}] {} ({})", e.kind, e.ticket, e.title, e.note))
        .collect();
    if !related.is_empty() {
        if block.is_empty() { block.push_str("## What xNAUT remembers about this area\n\n"); }
        block.push_str(&related.join("\n"));
        block.push('\n');
    }
    if block.is_empty() { String::new() } else { format!("{block}Open a note with memory_read before relying on it.\n\n") }
}

/// The vault documents a ticket names, in the form the document tools take:
/// `work:xnaut/Development/features/X.md` is project `xnaut`, rel
/// `Development/features/X.md`.
fn doc_targets(refs: &[String]) -> String {
    let rels: Vec<String> = refs
        .iter()
        .filter_map(|r| r.strip_prefix("work:"))
        .filter_map(|rel| rel.split_once('/').map(|(_, rest)| format!("`{rest}`")))
        .collect();
    if rels.is_empty() { String::new() } else { format!(" ({})", rels.join(", ")) }
}

fn continuation_prompt(ticket: &crate::project_management::TicketRecord, docs: &str, branch: &str, continuing: bool, poc_minutes: u64) -> String {
    let prompt = dispatch_prompt(ticket, docs, poc_minutes);
    if continuing {
        format!("You are CONTINUING {}, not starting it. Continue the existing branch `{branch}` and worktree, including its commits and uncommitted changes. Read this ticket's notes and handback for what was done and what remains, run the tests, and carry on. Do not restart from scratch.\n\nIf the ticket says a merge was REVERTED, your earlier commits are already in the integration branch's history and undone there: re-apply the change as NEW commits on this branch (for example `git revert` of the revert commit, or `git cherry-pick` of your originals), so the branch carries a fresh, reviewable diff. Adding only new files on top does not restore it.\n\n{prompt}", ticket.id)
    } else { prompt }
}

/// Dispatch a ticket to its owner. Everything that can fail for a reason the
/// owner can fix (no assignee, no profile, no repo path) fails before any
/// worktree is created, so a rejected dispatch leaves nothing behind.
#[tauri::command]
pub async fn pm_ticket_dispatch(
    app: tauri::AppHandle,
    ticket_id: String,
    project: String,
    environment: Option<String>,
) -> Result<DispatchResult, String> {
    dispatch_with_findings_gate(app, ticket_id, project, environment, None, false, PmWriteOrigin::Owner).await
}

pub(crate) async fn dispatch_scoped(
    app: tauri::AppHandle,
    ticket_id: String,
    project: String,
    environment: Option<String>,
    approved: Option<&crate::swarm_plan::PlannedRun>,
) -> Result<DispatchResult, String> {
    dispatch_with_findings_gate(app,ticket_id,project,environment,approved,false,PmWriteOrigin::Automatic).await
}

pub(crate) async fn model_ticket_dispatch(
    app: tauri::AppHandle, ticket_id: String, project: String, environment: Option<String>,
) -> Result<DispatchResult,String> {
    dispatch_with_findings_gate(app,ticket_id,project,environment,None,true,PmWriteOrigin::Automatic).await
}

#[derive(Clone, Copy)]
enum PmWriteOrigin { Owner, Automatic }

async fn dispatch_with_findings_gate(
    app: tauri::AppHandle, ticket_id: String, project: String, environment: Option<String>,
    approved: Option<&crate::swarm_plan::PlannedRun>, model_origin: bool, pm_origin: PmWriteOrigin,
) -> Result<DispatchResult,String> {
    if matches!(pm_origin, PmWriteOrigin::Automatic) {
        if let Some(reason) = crate::switches::automatic_pm_write_hold_strict() { return Err(reason); }
    }
    let tickets = crate::project_management::pm_ticket_list(
        app.state::<crate::state::AppState>(),
        Some(project.clone()),
    )
    .await?;
    let ticket = tickets
        .into_iter()
        .find(|item| item.id == ticket_id)
        .ok_or_else(|| format!("ticket {ticket_id} not found"))?;
    let requires_triage = model_origin || (approved.is_some()
        && crate::swarm_plan::model_group_requires_triage(&project,&ticket.id)?);
    if requires_triage { crate::ticket_triage::dispatch_admission(&ticket)?; }

    let handle = ticket
        .owner
        .as_deref()
        .map(|owner| owner.trim().trim_start_matches('@').to_ascii_lowercase())
        .filter(|owner| !owner.is_empty())
        .ok_or("assign an owner before dispatching this ticket")?;
    let profile = crate::agent_profiles::agent_profile_get(handle.clone())?;
    if approved.is_some_and(|run| {
        !crate::swarm_plan::authorized(run, &ticket) || run.model != profile.model
    }) {
        return Err("approved group scope, owner or model changed before native dispatch".into());
    }
    use crate::sandbox::launch_env::LaunchEnv;
    let requested = environment
        .as_deref()
        .map(|key| {
            LaunchEnv::from_key(key).ok_or_else(|| {
                format!("Unknown execution environment: {key}. Choose local, exe-dev or gitvm.")
            })
        })
        .transpose()?;
    let sandboxes = crate::settings::load_or_default().sandboxes;
    let destination = crate::sandbox::launch_env::resolve(
        requested.or_else(|| profile.execution.pinned_environment()),
        &sandboxes,
    );
    destination.route(&sandboxes)?;
    if let Some(run) = approved {
        approved_dispatch_policy(crate::instance::role(), crate::switches::load().read_only,
            run.environment.as_deref()).map_err(|refusal| refusal.reason)?;
        if run.environment.as_deref() != Some(destination.key()) {
            return Err("approved execution destination changed before dispatch".into());
        }
    }

    if !crate::run_control::runtime_meets_in(
        &crate::agents::registry_dir()?,
        &profile.runtime_id,
        &profile.model,
        &ticket.model_requirement,
    )? {
        return Err(format!(
            "@{handle} model {} does not meet ticket requirement {}",
            profile.model, ticket.model_requirement
        ));
    }
    crate::agent_work::reconcile_prelaunch(&project, &ticket.id)?;
    let continuation =
        crate::run_control::continuation_in(&crate::agents::registry_dir()?, &ticket.id)?;
    if let Some(live) =
        crate::run_control::live_run_for_ticket_in(&crate::agents::registry_dir()?, &ticket.id)?
    {
        return Err(format!(
            "{} already has a live run: {} (@{}, {:?}). Retire it before dispatching again.",
            ticket.id, live.run_id, live.agent_handle, live.state
        ));
    }

    let projects =
        crate::project_management::pm_project_list(app.state::<crate::state::AppState>()).await?;
    let repo = projects
        .iter()
        .find(|item| item.key == project)
        .map(crate::project_management::local_source_path)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| format!("project {project} has no local repo path set"))?;
    if !PathBuf::from(&repo).is_dir() {
        return Err(format!("repo path does not exist: {repo}"));
    }

    let root = PathBuf::from(&repo)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if approved.is_some_and(|run| run.repository_root.as_deref() != root.to_str()) {
        return Err("approved project repository changed; renew group approval".into());
    }
    let recovered = serde_json::to_value(crate::project_continuity::snapshot(&project)?)
        .map_err(|e| e.to_string())?;
    if let Some(run) = &continuation {
        crate::agent_work::verify_workspace(&root, std::path::Path::new(&run.worktree_path), &run.branch)?;
    }
    crate::agent_work::recovery_guard(&recovered, &ticket.id, continuation.as_ref())?;
    // Reserve before worktree/launch side effects. Chat and PM dispatch share
    // the same project+ticket slot, including concurrent calls from other apps.
    let registry = crate::agents::registry_dir()?;
    let receipt_path = crate::agent_work::launch_receipt_path(&registry, &root, &ticket.id,
        continuation.as_ref().map(|run| run.run_id.as_str()))?;
    let branch = continuation.as_ref().map(|r| r.branch.clone())
        .unwrap_or_else(|| branch_for_ticket(&ticket, &handle));
    let worktree_path = match &continuation {
        Some(run) => run.worktree_path.clone(),
        None => crate::worktree::worktree_suggest_path(repo.clone(), branch.clone())?,
    };
    let pending = serde_json::json!({"pending":true,"ticket":ticket.id,"project":project,
        "repository_root":root,"continuation_run_id":continuation.as_ref().map(|run| &run.run_id),
        "handle":handle,"branch":branch,"worktree_path":worktree_path,"environment":destination.key(),
        "requested_at":crate::run_control::now_ms(),"requires_findings_triage":requires_triage});
    if let Some(prior) = crate::agent_work::reserve(&receipt_path, &pending)? {
        return Err(format!("{} already has a launch receipt; no duplicate worker started. Recover: {prior}", ticket.id));
    }

    let mut reservation = crate::agent_work::LaunchReservation::new(receipt_path.clone());
    // Re-dispatching a ticket must not fail on "branch already exists". If the
    // worktree from the last run is still there, the agent goes back into it.
    let existing = crate::worktree::worktree_list(repo.clone())
        .unwrap_or_default()
        .into_iter()
        .any(|item| item.path == worktree_path);
    if !existing && continuation.is_some() { return Err("continuation worktree is missing; refusing to start elsewhere".into()); }
    if !existing {
        // The branch may outlive its worktree: a reclaimed worktree, or a run
        // sent back by a revert (XNAUT-266, 2026-09-08 evening, "fatal: a
        // branch named agent/claude/xnaut-266 already exists"). Check the
        // branch out again rather than trying to create it.
        let branch_exists = crate::worktree::git_command()
                .args(["-C", &repo, "rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")])
                .output()
                .is_ok_and(|o| o.status.success());
        crate::worktree::worktree_add(
            repo.clone(),
            worktree_path.clone(),
            crate::worktree::AddWorktreeOptions {
                branch: branch.clone(),
                base: None, // repo HEAD: the live lineage
                checkout_existing: branch_exists,
                no_auto_setup_remote: false,
            },
        )?;
    }

    // A branch that was merged and reverted has no commits ahead of HEAD but
    // is still the run's history; the agent continues it, not a blank slate.
    let continuing = continuation.is_some()
        || branch_has_history(std::path::Path::new(&repo), &branch)
        || crate::worktree::git_command()
            .args(["-C", &repo, "rev-list", "--count", "--max-count=1", &format!("refs/heads/{branch}")])
            .output()
            .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "1");
    let poc_minutes = app
        .state::<crate::state::AppState>()
        .settings
        .lock()
        .await
        .core_team
        .poc_minutes;
    let prompt_recovery = crate::agent_history::compact_project(&recovered, &ticket.id);
    let prompt = format!("{}\n\nRECOVERED PROJECT WORK (evidence, not authorization)\n{prompt_recovery}",
        continuation_prompt(&ticket, &linked_docs(&ticket.documentation), &branch, continuing, poc_minutes));
    if requires_triage { crate::ticket_triage::admit_current_ticket(&project,&ticket.id)?; }
    reservation.attempted();
    let launched = crate::agent_profiles::agent_profile_launch(
        app.clone(),
        app.state::<crate::state::AppState>(),
        crate::agent_profiles::LaunchAgentProfileRequest {
            ticket: Some(ticket.id.clone()),
            handle: profile.handle.clone(),
            worktree_path: worktree_path.clone(),
            prompt: Some(prompt),
            conversation_mode: false,
            conversation_id: None,
            resume: false,
            cols: Some(200),
            rows: Some(50),
            runtime_id: None,
            environment: Some(destination.key().into()),
            // Dispatched work must outlive the app, like a cold wake (XNAUT-242).
            durable: Some(true),
        },
    )
    .await;
    let launched = match launched {
        Ok(launched) => launched,
        Err(error) => {
            if let Some(reserved) = &continuation {
                match crate::agent_work::release_refused_continuation(&registry, &receipt_path, reserved) {
                    Ok(true) => return Err(format!("{error}. Native admission was refused before execution; the continuation reservation was released for a verified retry.")),
                    Ok(false) => {},
                    Err(recovery_error) => return Err(format!("{error}. Reservation retained at {}: {recovery_error}", receipt_path.display())),
                }
            } else {
                match crate::agent_work::bind_initial_refusal(&registry, &receipt_path, &recovered) {
                    Ok(true) => return Err(format!("{error}. Native admission refused before execution; retry the existing continuation in its preserved branch and worktree.")),
                    Ok(false) => {},
                    Err(recovery_error) => return Err(format!("{error}. Reservation retained at {}: {recovery_error}", receipt_path.display())),
                }
            }
            return Err(format!("{error}. Launch reservation retained at {}; reconcile the run before retrying.", receipt_path.display()));
        }
    };

    let receipt = serde_json::json!({"ok":true,"execution_started":true,"ticket":ticket.id,
        "project":project,"repository_root":root,"handle":handle,"branch":branch,
        "worktree_path":worktree_path,"environment":destination.key(),"launch":launched});
    crate::agent_work::save_launch_receipt(&receipt_path, &receipt).map_err(|error|
        format!("Worker already launched; receipt persistence failed: {error}. Do not retry. Recover {receipt}"))?;

    let note = format!(
        "\n\n## Dispatched {date} to @{handle}\n\n- branch `{branch}`\n- worktree `{worktree_path}`\n- session `{session}`\n- environment `{environment}`\n",
        date = &chrono::Utc::now().to_rfc3339()[..10],
        session = launched.session_id,
        environment = destination.key(),
    );
    let request = crate::project_management::TicketUpdateRequest {
            model_requirement: None,
            // Status transition attribution is separate from the trusted entry
            // point used for read_only admission below.
            caller: None,
            id: ticket.id.clone(),
            expected_revision: ticket.revision,
            title: None,
            ticket_type: None,
            status: Some("in_progress".into()),
            priority: None,
            owner: None,
            clear_owner: false,
            documentation: None,
            body: Some(format!("{}{note}", ticket.body)),
        };
    let state = app.state::<crate::state::AppState>();
    let dispatched_ticket = if matches!(pm_origin, PmWriteOrigin::Owner) {
        crate::project_management::pm_ticket_update(state, request).await?
    } else {
        crate::project_management::ticket_update_automatic(state, request).await?
    };

    Ok(DispatchResult {
        ticket_scope: crate::swarm_plan::scope(&dispatched_ticket),
        ticket_id: ticket.id,
        handle,
        branch,
        worktree_path,
        run_id: launched.run_id,
        session_id: launched.session_id,
        environment: destination.key().into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket() -> crate::project_management::TicketRecord {
        crate::project_management::TicketRecord {
            model_requirement: String::new(),
            approval: Default::default(),
            id: "XNAUT-1".into(),
            project: "XNAUT".into(),
            title: "Do the thing".into(),
            ticket_type: "feature".into(),
            status: "ready".into(),
            priority: "high".into(),
            owner: Some("claude".into()),
            documentation: vec![],
            tags: vec![],
            release: String::new(),
            body: "The body.".into(),
            source_id: String::new(),
            revision: 2,
            created_at: String::new(),
            updated_at: String::new(),
            handback: None,
            parent: None,
        }
    }

    #[test]
    fn prompt_carries_the_ticket_and_the_finish_line() {
        let prompt = dispatch_prompt(&ticket(), "", 90);
        assert!(prompt.contains("XNAUT-1"));
        assert!(prompt.contains("Do the thing"));
        assert!(prompt.contains("The body."));
        assert!(prompt.contains("None linked."));
        // The whole point: the agent, not Dispatch, closes the loop.
        assert!(prompt.contains(".xnaut/bundles/XNAUT-1.md"));
        assert!(prompt.contains("move XNAUT-1 to `done`"));
        assert!(prompt.contains("XNAUT_TEST_PORT=4291"));
    }

    #[test]
    fn foreign_projects_use_their_own_verification_contract() {
        let mut foreign = ticket();
        foreign.id = "MUSIC-1".into();
        foreign.project = "MUSIC".into();
        let prompt = dispatch_prompt(&foreign, "", 90);
        assert!(prompt.contains("`.xnaut/verify.json`"));
        assert!(prompt.contains("command, exit code, and log evidence"));
        assert!(prompt.contains("when absent, the native verifier supports Node auto-detection from `package.json`"));
        assert!(prompt.contains("supplied run artifact directory"));
        assert!(prompt.contains("Publication of that report does not complete the implementation"));
        assert!(prompt.contains("ticket's explicit acceptance and documentation requirements"));
        assert!(!prompt.contains(".xnaut/bundles/MUSIC-1.md"));
        assert!(!prompt.contains("## Shipped MUSIC-1"));
        assert!(!prompt.contains("move MUSIC-1 to `done`"));
        for unrelated in ["cargo test", "playwright test", "XNAUT_TEST_TOTALS"] {
            assert!(!prompt.contains(unrelated), "foreign project inherited {unrelated}");
        }
        assert!(prompt.contains("Missing tools or configuration are explicit verification gaps"));
    }

    #[test]
    fn foreign_author_contract_keeps_explicit_task_requirements_on_continuation() {
        let mut foreign = ticket();
        foreign.id = "PYTHON-1".into();
        foreign.project = "PYTHON".into();
        foreign.body = "Run python3 -m unittest -v. Update docs/import.md. Do not invent the missing external contract.".into();
        let prompt = continuation_prompt(&foreign, "Keep the approved external contract.", "agent/python/import", true, 90);
        assert!(prompt.contains(&foreign.body));
        assert!(prompt.contains("Keep the approved external contract."));
        assert!(prompt.contains("CONTINUING PYTHON-1"));
        assert!(prompt.contains("Preserve prior handbacks"));
        assert!(!prompt.contains("## Shipped PYTHON-1"));
        assert!(!prompt.contains("XNAUT_TEST_TOTALS"));
    }

    #[test]
    fn the_prompt_states_constraints_and_never_a_numbered_checklist() {
        // Cursor's harness post, 2026-09-10: listed steps get done and
        // unlisted ones are quietly deprioritised; constraints mark the edges
        // and leave the model to do good things by default; an adjective
        // where a number belongs produces a handful. This pins the form.
        // The strings the gates parse are pinned by the test above.
        let prompt = dispatch_prompt(&ticket(), "", 90);
        for step in ["\n1. ", "\n2. ", "\n3. ", "1. Implement", "2. Run"] {
            assert!(!prompt.contains(step), "numbered step survived: {step:?}");
        }
        assert!(prompt.contains("What done means"));
        assert!(prompt.contains("No TODOs. No partial implementations."));
        assert!(prompt.contains("at most 30 lines"), "a number, not an adjective");
        assert!(prompt.contains("Zero failures"));
        assert!(prompt.contains("Silence reads as abandoned"));
        // Nothing that explains engineering to the model.
        assert!(!prompt.contains("Implement the ticket"));
    }

    #[test]
    fn the_prompt_carries_what_xnaut_remembers_about_the_ticket() {
        // XNAUT-331: a learning written on an earlier run of this ticket, or on
        // its area, is in front of the next agent before it starts.
        let _lock = crate::vault::test_vault_lock();
        let root = std::env::temp_dir().join(format!("xnaut-dispatch-recall-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("work")).unwrap();
        crate::vault::use_test_vault(root.clone());
        let mut fresh = dispatch_prompt(&ticket(), "", 90);
        assert!(!fresh.contains("What xNAUT remembers"), "nothing known, no heading");
        crate::memory::remember(&root.join("work"), &crate::memory::Entry {
            kind: "learning".into(), project: "XNAUT".into(), ticket: "XNAUT-1".into(),
            text: "the widget must be locked before the read".into(), source: "test:1".into(), ..Default::default()
        }).unwrap();
        fresh = dispatch_prompt(&ticket(), "", 90);
        assert!(fresh.contains("What xNAUT remembers"), "{fresh}");
        assert!(fresh.contains("locked before the read"), "{fresh}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A core-team finding dispatches onto its own branch and carries the PoC
    /// brief; everything else is untouched, down to the word (XNAUT-357).
    #[test]
    fn a_poc_finding_dispatches_onto_its_own_branch_with_the_brief() {
        let mut finding = ticket();
        finding.ticket_type = crate::core_team::FINDING_TYPE.into();
        finding.tags = vec![crate::core_team::POC_TAG.into()];
        finding.body = crate::core_team::Finding {
            repo_url: "https://github.com/acme/loop-runner".into(),
            licence: "MIT".into(),
            file: "src/runner/budget.py".into(),
            does: "budgets an agent loop".into(),
            ..Default::default()
        }
        .body();
        assert_eq!(branch_for_ticket(&finding, "claude"), "poc/acme-loop-runner");
        let prompt = dispatch_prompt(&finding, "", 90);
        assert!(prompt.contains("PoC brief"), "{prompt}");
        assert!(prompt.contains("At most 90 minutes"));
        assert!(prompt.contains("Reimplement, never copy"));

        // An ordinary ticket keeps the branch it always had and gains nothing.
        let ordinary = ticket();
        assert_eq!(
            branch_for_ticket(&ordinary, "claude"),
            branch_for("claude", &ordinary.id)
        );
        assert!(!dispatch_prompt(&ordinary, "", 90).contains("PoC brief"));
    }

    #[test]
    fn a_non_work_reference_is_reported_not_dropped() {
        let out = linked_docs(&["obsidian:Business/Note.md".into()]);
        assert!(out.contains("obsidian:Business/Note.md"));
    }
    #[test]
    fn a_swap_continues_even_before_the_predecessors_first_commit() {
        let prompt = continuation_prompt(&ticket(), "", "agent/previous/xnaut-1", true, 90);
        assert!(prompt.starts_with("You are CONTINUING XNAUT-1"));
        assert!(prompt.contains("agent/previous/xnaut-1"));
        assert!(prompt.contains("uncommitted changes"));
        assert!(!continuation_prompt(&ticket(), "", "agent/new/xnaut-1", false, 90).starts_with("You are CONTINUING"));
    }

}
