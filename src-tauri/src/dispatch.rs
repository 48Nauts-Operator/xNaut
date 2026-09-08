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

use serde::Serialize;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Clone, Debug, Serialize)]
pub struct DispatchResult {
    pub ticket_id: String,
    pub handle: String,
    pub branch: String,
    pub worktree_path: String,
    pub session_id: String,
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

/// True when the ticket's branch already carries work: a re-dispatch, whoever
/// ran it before. The prompt then says CONTINUE, and the agent reads the
/// ticket's own notes and handback for what was done, so any runtime picks up
/// where the last one stopped (the whole point: an Anthropic outage hands the
/// task to Codex, and the log in the ticket is the handover).
fn branch_has_history(repo: &std::path::Path, branch: &str) -> bool {
    std::process::Command::new("git")
        .args(["-C", &repo.to_string_lossy(), "rev-list", "--count", &format!("HEAD..{branch}")])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().parse::<u32>().unwrap_or(0) > 0)
        .unwrap_or(false)
}

fn dispatch_prompt(ticket: &crate::project_management::TicketRecord, docs: &str) -> String {
    // Two agents on tron (XNAUT-303 and 255) spent their run trying to ssh to
    // tron, and reported the rig unreachable. Tell them where they stand.
    let host = match crate::run_control::hostname() {
        h if h.is_empty() => "this machine".to_string(),
        h => h,
    };
    format!(
        "You have been dispatched on {id} ({priority} {kind}).\n\n\
         # {title}\n\n{body}\n\n\
         ## Linked documents\n{docs}\n\n\
         ## What finishing looks like\n\n\
         You are already inside a worktree on your own branch, on the machine `{host}`. \
         Work there. If a ticket names this machine, that is where you are: nothing to ssh to.\n\n\
         1. Implement the ticket.\n\
         2. Run `cargo test --manifest-path src-tauri/Cargo.toml` and the UI suite \
            (`XNAUT_TEST_PORT=4291 npx playwright test`). Both green, or the ticket is not done.\n\
         3. Write the test bundle to `.xnaut/bundles/{id}.md` in the worktree: what changed, \
            the two suite results with their totals, and how to verify it by hand.\n\
         4. Update the ticket's design document in the work vault{doc_targets}: append a dated \
            `## Shipped {id}` section with what was done, how, the files and the key code \
            (short snippets are welcome), the test totals, and what is deliberately not done. \
            Use `xnaut_read_document` then `xnaut_update_document` (project `{project}`). The \
            doc travels with the code; sign-off reads it. No linked doc: create \
            `Development/features/YYYY-MM-DD_{id}.md` with the frontmatter `Author` and \
            `Last modified`, and add it to the ticket's documentation.\n\
         5. Commit, then move {id} to `done` with the bundle path and a summary appended \
            to its body. `done` hands it to NautBot, who tests it. Never set `complete`; \
            that is NautBot's word.\n\n\
         If something blocks you, say so on the ticket rather than going quiet.\n",
        id = ticket.id,
        priority = ticket.priority,
        kind = ticket.ticket_type,
        title = ticket.title,
        body = ticket.body,
        project = ticket.project,
        docs = if docs.trim().is_empty() { "\nNone linked.\n" } else { docs },
        doc_targets = doc_targets(&ticket.documentation),
    )
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

fn continuation_prompt(ticket: &crate::project_management::TicketRecord, docs: &str, branch: &str, continuing: bool) -> String {
    let prompt = dispatch_prompt(ticket, docs);
    if continuing {
        format!("You are CONTINUING {}, not starting it. Continue the existing branch `{branch}` and worktree, including its commits and uncommitted changes. Read this ticket's notes and handback for what was done and what remains, run the tests, and carry on. Do not restart from scratch.\n\n{prompt}", ticket.id)
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
) -> Result<DispatchResult, String> {
    let tickets = crate::project_management::pm_ticket_list(
        app.state::<crate::state::AppState>(),
        Some(project.clone()),
    )
    .await?;
    let ticket = tickets
        .into_iter()
        .find(|item| item.id == ticket_id)
        .ok_or_else(|| format!("ticket {ticket_id} not found"))?;

    let handle = ticket
        .owner
        .as_deref()
        .map(|owner| owner.trim().trim_start_matches('@').to_ascii_lowercase())
        .filter(|owner| !owner.is_empty())
        .ok_or("assign an owner before dispatching this ticket")?;
    let profile = crate::agent_profiles::agent_profile_get(handle.clone())?;
    if !crate::run_control::runtime_meets_in(&crate::agents::registry_dir()?, &profile.runtime_id, &profile.model, &ticket.model_requirement)? {
        return Err(format!("@{handle} model {} does not meet ticket requirement {}", profile.model, ticket.model_requirement));
    }
    let continuation = crate::run_control::continuation_in(&crate::agents::registry_dir()?, &ticket.id)?;
    if let Some(live) = crate::run_control::live_run_for_ticket_in(&crate::agents::registry_dir()?, &ticket.id)? {
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

    let branch = continuation.as_ref().map(|r| r.branch.clone())
        .unwrap_or_else(|| format!("agent/{handle}/{}", ticket.id.to_ascii_lowercase()));
    let worktree_path = match &continuation {
        Some(run) => run.worktree_path.clone(),
        None => crate::worktree::worktree_suggest_path(repo.clone(), branch.clone())?,
    };
    // Re-dispatching a ticket must not fail on "branch already exists". If the
    // worktree from the last run is still there, the agent goes back into it.
    let existing = crate::worktree::worktree_list(repo.clone())
        .unwrap_or_default()
        .into_iter()
        .any(|item| item.path == worktree_path);
    if !existing && continuation.is_some() { return Err("continuation worktree is missing; refusing to start elsewhere".into()); }
    if !existing {
        crate::worktree::worktree_add(
            repo.clone(),
            worktree_path.clone(),
            crate::worktree::AddWorktreeOptions {
                branch: branch.clone(),
                base: None, // repo HEAD: the live lineage
                checkout_existing: false,
                no_auto_setup_remote: false,
            },
        )?;
    }

    let continuing = continuation.is_some() || branch_has_history(std::path::Path::new(&repo), &branch);
    let prompt = continuation_prompt(&ticket, &linked_docs(&ticket.documentation), &branch, continuing);
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
            // Dispatched work must outlive the app, like a cold wake (XNAUT-242).
            durable: Some(true),
        },
    )
    .await?;

    let note = format!(
        "\n\n## Dispatched {date} to @{handle}\n\n- branch `{branch}`\n- worktree `{worktree_path}`\n- session `{session}`\n",
        date = &chrono::Utc::now().to_rfc3339()[..10],
        session = launched.session_id,
    );
    crate::project_management::pm_ticket_update(
        app.state::<crate::state::AppState>(),
        crate::project_management::TicketUpdateRequest {
            model_requirement: None,
            // Dispatch is the owner pressing a button, not an agent writing:
            // unattributed, and therefore not gated (XNAUT-243).
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
        },
    )
    .await?;

    Ok(DispatchResult {
        ticket_id: ticket.id,
        handle,
        branch,
        worktree_path,
        session_id: launched.session_id,
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
            body: "The body.".into(),
            source_id: String::new(),
            revision: 2,
            created_at: String::new(),
            updated_at: String::new(),
            handback: None,
        }
    }

    #[test]
    fn prompt_carries_the_ticket_and_the_finish_line() {
        let prompt = dispatch_prompt(&ticket(), "");
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
    fn a_non_work_reference_is_reported_not_dropped() {
        let out = linked_docs(&["obsidian:Business/Note.md".into()]);
        assert!(out.contains("obsidian:Business/Note.md"));
    }
    #[test]
    fn a_swap_continues_even_before_the_predecessors_first_commit() {
        let prompt = continuation_prompt(&ticket(), "", "agent/previous/xnaut-1", true);
        assert!(prompt.starts_with("You are CONTINUING XNAUT-1"));
        assert!(prompt.contains("agent/previous/xnaut-1"));
        assert!(prompt.contains("uncommitted changes"));
        assert!(!continuation_prompt(&ticket(), "", "agent/new/xnaut-1", false).starts_with("You are CONTINUING"));
    }

}
