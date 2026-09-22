// XNAUT-316. An agent may divide its own ticket.
//
// The one role Cursor's harness never deleted was ownership: any planner whose
// scope is too big spawns subplanners that fully own a slice, recursively.
// Until now no agent here could create work at all, so the fleet's throughput
// was bounded by how fast one person writes tickets. This is the accountable-
// owner half of their tree. It touches no gate: a child goes through verify
// and the jury exactly as any ticket does.

use crate::project_management::{TicketCreateRequest, TicketRecord};

/// How deep a tree may go, counting the root as depth 0. A number to tune,
/// not a principle; two is enough to learn whether the idea holds.
pub const MAX_DEPTH: u32 = 2;

/// Depth of `id` in its parent chain. A ticket nobody carved out is 0. A
/// cycle or a missing parent stops the walk rather than spinning.
pub fn depth_of(tickets: &[TicketRecord], id: &str) -> u32 {
    let mut depth = 0;
    let mut current = id;
    let mut seen = std::collections::HashSet::new();
    while let Some(parent) = tickets
        .iter()
        .find(|t| t.id == current)
        .and_then(|t| t.parent.as_deref())
    {
        if !seen.insert(current.to_string()) || depth >= MAX_DEPTH + 1 {
            break;
        }
        depth += 1;
        current = parent;
    }
    depth
}

/// Children of `parent` that are not finished. `done` and `complete` count as
/// finished; everything else, including `blocked`, is still open.
pub fn open_children(tickets: &[TicketRecord], parent: &str) -> Vec<String> {
    let mut open: Vec<String> = tickets
        .iter()
        .filter(|t| t.parent.as_deref() == Some(parent))
        .filter(|t| !matches!(t.status.as_str(), "done" | "complete"))
        .map(|t| t.id.clone())
        .collect();
    open.sort();
    open
}

/// The request for a child, or the reason there cannot be one.
///
/// The caller must own the parent: dividing somebody else's ticket is not
/// delegation, it is taking it. The child inherits everything that constrains
/// the parent and nothing that describes it: project, release, tags,
/// documentation and model requirement come across; title and body are the
/// child's own, and the body says whose child it is so the agent that picks it
/// up knows there is a whole above it.
pub fn child_request(
    tickets: &[TicketRecord],
    parent_id: &str,
    caller: &str,
    title: &str,
    body: &str,
) -> Result<TicketCreateRequest, String> {
    let parent = tickets
        .iter()
        .find(|t| t.id == parent_id)
        .ok_or_else(|| format!("{parent_id} not found"))?;
    let caller = caller.trim().trim_start_matches('@').to_ascii_lowercase();
    let owner = parent
        .owner
        .as_deref()
        .map(|o| o.trim().trim_start_matches('@').to_ascii_lowercase())
        .unwrap_or_default();
    if caller.is_empty() || caller != owner {
        return Err(format!(
            "{parent_id} is owned by {}; only its owner may divide it",
            if owner.is_empty() { "nobody".to_string() } else { format!("@{owner}") }
        ));
    }
    if !matches!(parent.status.as_str(), "in_progress" | "ready") {
        return Err(format!("{parent_id} is {}; only work in progress divides", parent.status));
    }
    let depth = depth_of(tickets, parent_id) + 1;
    if depth > MAX_DEPTH {
        return Err(format!(
            "{parent_id} is already {} deep; the tree stops at {MAX_DEPTH}",
            depth - 1
        ));
    }
    Ok(TicketCreateRequest {
        model_requirement: parent.model_requirement.clone(),
        project: parent.project.clone(),
        title: title.trim().to_string(),
        ticket_type: "task".into(),
        status: "ready".into(),
        priority: parent.priority.clone(),
        owner: Some(caller),
        documentation: parent.documentation.clone(),
        body: format!("Child of {parent_id}: {}\n\n{}", parent.title.trim(), body.trim()),
        parent: Some(parent_id.to_string()),
        release: parent.release.clone(),
        tags: parent.tags.clone(),
        source_id: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket(id: &str, owner: Option<&str>, status: &str, parent: Option<&str>) -> TicketRecord {
        let mut t: TicketRecord = serde_json::from_value(serde_json::json!({
            "id": id, "project": "XNAUT", "title": format!("Title of {id}"), "type": "feature",
            "status": status, "priority": "high", "owner": owner, "documentation": ["work:xnaut/Development/features/x.md"],
            "tags": ["swarm"], "release": "1.27.0", "body": "", "source_id": "",
            "model_requirement": "opus", "revision": 1, "created_at": "", "updated_at": ""
        }))
        .expect("fixture ticket");
        t.parent = parent.map(str::to_string);
        t
    }

    #[test]
    fn a_child_inherits_what_constrains_the_parent_and_nothing_that_describes_it() {
        let tickets = vec![ticket("XNAUT-1", Some("@claude"), "in_progress", None)];
        let req = child_request(&tickets, "XNAUT-1", "claude", "Do the left half", "Green means X.").unwrap();
        assert_eq!(req.parent.as_deref(), Some("XNAUT-1"));
        assert_eq!(req.project, "XNAUT");
        assert_eq!(req.release, "1.27.0");
        assert_eq!(req.tags, vec!["swarm".to_string()]);
        assert_eq!(req.model_requirement, "opus");
        assert_eq!(req.documentation, vec!["work:xnaut/Development/features/x.md".to_string()]);
        assert_eq!(req.owner.as_deref(), Some("claude"), "owned by the agent that carved it");
        assert_eq!(req.status, "ready", "the sweep dispatches it like any ready ticket");
        assert_eq!(req.title, "Do the left half");
        assert!(req.body.starts_with("Child of XNAUT-1: Title of XNAUT-1"), "{}", req.body);
        assert!(req.body.ends_with("Green means X."));
    }

    #[test]
    fn only_the_owner_divides_and_only_work_in_progress() {
        let tickets = vec![
            ticket("XNAUT-1", Some("@claude"), "in_progress", None),
            ticket("XNAUT-2", None, "ready", None),
            ticket("XNAUT-3", Some("claude"), "done", None),
        ];
        let err = child_request(&tickets, "XNAUT-1", "codex", "t", "b").unwrap_err();
        assert!(err.contains("owned by @claude"), "{err}");
        let err = child_request(&tickets, "XNAUT-2", "claude", "t", "b").unwrap_err();
        assert!(err.contains("owned by nobody"), "{err}");
        let err = child_request(&tickets, "XNAUT-3", "claude", "t", "b").unwrap_err();
        assert!(err.contains("is done"), "{err}");
        assert!(child_request(&tickets, "XNAUT-9", "claude", "t", "b").unwrap_err().contains("not found"));
    }

    #[test]
    fn the_tree_stops_at_max_depth_and_a_cycle_does_not_spin() {
        let tickets = vec![
            ticket("XNAUT-1", Some("claude"), "in_progress", None),
            ticket("XNAUT-2", Some("claude"), "in_progress", Some("XNAUT-1")),
            ticket("XNAUT-3", Some("claude"), "in_progress", Some("XNAUT-2")),
            ticket("XNAUT-8", Some("claude"), "in_progress", Some("XNAUT-9")),
            ticket("XNAUT-9", Some("claude"), "in_progress", Some("XNAUT-8")),
        ];
        assert_eq!(depth_of(&tickets, "XNAUT-1"), 0);
        assert_eq!(depth_of(&tickets, "XNAUT-2"), 1);
        assert_eq!(depth_of(&tickets, "XNAUT-3"), 2);
        // Depth 1 may still divide; depth 2 may not.
        assert!(child_request(&tickets, "XNAUT-2", "claude", "t", "b").is_ok());
        let err = child_request(&tickets, "XNAUT-3", "claude", "t", "b").unwrap_err();
        assert!(err.contains("tree stops at 2"), "{err}");
        // A cycle terminates and reads as deep, so it cannot be divided further.
        assert!(depth_of(&tickets, "XNAUT-8") <= MAX_DEPTH + 1);
        assert!(child_request(&tickets, "XNAUT-8", "claude", "t", "b").is_err());
    }

    #[test]
    fn a_parents_handback_waits_for_its_children_through_the_real_filing_path() {
        // The upward flow of a planner tree, for free: the handoff that
        // matters is the one that arrives after everything under it has.
        use crate::project_management::{file_handback_with_registry_in, ticket_create_in, TicketCreateRequest};
        let (_root, control, registry, _store, _t, _job) = crate::jury_signoff::tests::fixture("subdivide-gate");
        let make = |title: &str, status: &str| -> TicketRecord {
            ticket_create_in(&control, TicketCreateRequest {
                model_requirement: String::new(), project: "XNAUT".into(), title: title.into(),
                ticket_type: "feature".into(), status: status.into(), priority: "high".into(),
                owner: Some("claude".into()), documentation: vec![], body: String::new(),
                parent: None, release: String::new(), tags: vec![], source_id: String::new(),
            }).unwrap()
        };
        let parent = make("Big thing", "in_progress");
        let tickets = crate::project_management::ticket_list_in(&control, None).unwrap();
        let mut child_req = child_request(&tickets, &parent.id, "claude", "Left half", "Green means X.").unwrap();
        let child = ticket_create_in(&control, child_req.clone()).unwrap();
        assert_eq!(child.parent.as_deref(), Some(parent.id.as_str()), "the link survives the write");

        // A handback the checker would accept, so the only thing standing
        // between it and the store is the children.
        let handback = crate::handback::Handback {
            ticket: parent.id.clone(),
            summary: "did the big thing".into(),
            files_changed: vec!["src/x.rs".into()],
            commits: vec!["abc1234".into()],
            how_verified: "cargo test: 1 passed".into(),
            not_finished: Some("nothing".into()),
            confidence: crate::handback::Confidence::High,
            from: "claude".into(),
            ..Default::default()
        };
        let err = file_handback_with_registry_in(&control, &registry, &handback).unwrap_err();
        assert!(err.contains(&child.id), "the refusal names the open child: {err}");
        assert!(err.contains("children still open"), "{err}");

        // A second parent whose only child is already done files normally.
        let parent2 = make("Other thing", "in_progress");
        let tickets = crate::project_management::ticket_list_in(&control, None).unwrap();
        child_req = child_request(&tickets, &parent2.id, "claude", "Only half", "Green.").unwrap();
        child_req.status = "done".into();
        ticket_create_in(&control, child_req).unwrap();
        let handback2 = crate::handback::Handback { ticket: parent2.id.clone(), ..handback.clone() };
        assert!(
            matches!(file_handback_with_registry_in(&control, &registry, &handback2), Ok(crate::project_management::Filing::Filed { .. })),
            "done children do not hold the parent"
        );
    }

    #[test]
    fn a_parent_is_open_until_every_child_is_done() {
        let mut tickets = vec![
            ticket("XNAUT-1", Some("claude"), "in_progress", None),
            ticket("XNAUT-2", Some("claude"), "in_progress", Some("XNAUT-1")),
            ticket("XNAUT-3", Some("claude"), "blocked", Some("XNAUT-1")),
            ticket("XNAUT-4", Some("claude"), "done", Some("XNAUT-1")),
            ticket("XNAUT-5", Some("claude"), "complete", Some("XNAUT-1")),
            ticket("XNAUT-6", Some("claude"), "in_progress", Some("XNAUT-7")),
        ];
        assert_eq!(open_children(&tickets, "XNAUT-1"), vec!["XNAUT-2", "XNAUT-3"]);
        assert!(open_children(&tickets, "XNAUT-4").is_empty(), "no children at all");
        for t in tickets.iter_mut().filter(|t| t.parent.as_deref() == Some("XNAUT-1")) {
            t.status = "done".into();
        }
        assert!(open_children(&tickets, "XNAUT-1").is_empty());
    }
}
