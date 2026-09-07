// Plan Canvas: an agent hands over a plan and BLOCKS until the owner approves
// it or sends it back with notes anchored to the lines he means.
//
// Idea from ECC (github.com/affaan-m/ecc, MIT), `docs/design/plan-canvas.md`,
// which credits lavish-axi by @kunchenguid.
//
// Where we depart, and why: ECC's canvas exists because a CLI cannot draw, so
// it stands up a loopback web server with a DNS-rebinding guard, ships its own
// markdown renderer and pins a Mermaid CDN, all to borrow a browser from the
// outside. xNAUT IS the browser, so none of that is ported. What is left is the
// part that was ever the feature:
//   - the plan on disk, rendered by the pane that already renders plans,
//   - range-anchored notes against it (notes.rs, from the hunk port),
//   - a verdict the waiting agent receives (inbox.rs create_and_announce +
//     the /v1/inbox/wait long-poll the veto's ask tier already uses).
//
// A review round owns its notes. Posting a revised plan clears the previous
// round's notes rather than leaving them anchored to line numbers the revision
// moved, which would hand the agent feedback pointing at the wrong paragraph.

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tauri::Emitter;

use crate::notes::NotesDoc;

/// Keys we plant in the inbox item's context so a later `wait` can find its way
/// back to the plan without a second store.
const CTX_PROJECT: &str = "plan_project";
const CTX_FILE: &str = "plan_file";

/// One note as the agent receives it: numbered in plan order, with the lines it
/// is stuck to and the text of those lines, so acting on it needs no lookup.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanNote {
    pub n: usize,
    pub lines: [u32; 2],
    pub quote: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanVerdict {
    pub id: String,
    /// approved | changes_requested | pending
    pub decision: String,
    pub plan_path: String,
    pub notes: Vec<PlanNote>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PlanReviewRequest {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub spend_estimate: Option<f64>,
    /// Worktree root the plan belongs to. The plan file is written under it.
    pub project: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub from: String,
    /// The plan, as markdown.
    #[serde(default, alias = "markdown")]
    pub plan: String,
    /// Relative to `project`. Defaults to PLAN.md, which is what plan-pane
    /// already opens.
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// An inbox status turned into the answer the agent asked for.
///
/// The inbox vocabulary is approved/denied because it was built for tool calls.
/// A plan sent back is not denied work, it is work with notes on it, so the
/// verdict says so. Anything still open is `pending`: the caller re-issues the
/// wait, exactly as the veto shim does.
pub fn decision_from_status(status: &str) -> &'static str {
    match status {
        "approved" => "approved",
        "denied" => "changes_requested",
        _ => "pending",
    }
}

/// A plan file path an agent may not aim outside the project it named.
pub fn resolve_plan_file(file: Option<&str>) -> Result<String, String> {
    let raw = file.map(str::trim).filter(|f| !f.is_empty()).unwrap_or("PLAN.md");
    if raw.starts_with('/') || raw.starts_with('~') {
        return Err("the plan file must be relative to the project".to_string());
    }
    if Path::new(raw)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("the plan file must stay inside the project".to_string());
    }
    Ok(raw.to_string())
}

/// The project path as both sides will spell it: no trailing slash.
pub fn project_root(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        raw.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn quote_lines(plan: &str, range: [u32; 2]) -> String {
    let lines: Vec<&str> = plan.lines().collect();
    let start = range[0].max(1) as usize;
    let end = (range[1] as usize).min(lines.len());
    if start > lines.len() || end < start {
        return String::new();
    }
    let text = lines[start - 1..end].join("\n");
    // Long enough to recognise the paragraph, short enough not to echo the
    // whole plan back at the agent that wrote it.
    if text.chars().count() > 300 {
        let cut: String = text.chars().take(300).collect();
        format!("{cut}…")
    } else {
        text
    }
}

/// Notes for one plan file, numbered in the order they appear in the plan.
///
/// Insertion order would number them by when the reviewer happened to click,
/// so note 1 could sit below note 4 and "see note 2" would mean nothing.
pub fn collect_notes(doc: &NotesDoc, file: &str, plan: &str) -> Vec<PlanNote> {
    let mut anchored: Vec<([u32; 2], String)> = doc
        .files
        .iter()
        .filter(|f| f.path == file)
        .flat_map(|f| f.annotations.iter())
        .map(|a| {
            let range = a.new_range.or(a.old_range).unwrap_or([0, 0]);
            let mut text = a.summary.trim().to_string();
            if let Some(why) = a.rationale.as_deref().map(str::trim).filter(|w| !w.is_empty()) {
                text.push('\n');
                text.push_str(why);
            }
            (range, text)
        })
        .filter(|(_, text)| !text.is_empty())
        .collect();
    anchored.sort_by_key(|(range, _)| (range[0], range[1]));
    anchored
        .into_iter()
        .enumerate()
        .map(|(i, (lines, text))| PlanNote {
            n: i + 1,
            lines,
            quote: quote_lines(plan, lines),
            text,
        })
        .collect()
}

/// Resolve an outstanding review: wait for the human, then read back the notes
/// he left. Shared by the POST that opens the review and the GET that re-waits.
async fn settle(id: &str, timeout_ms: u64) -> Result<PlanVerdict, String> {
    let item = crate::inbox::wait_for_answer(id, timeout_ms)
        .await
        .ok_or_else(|| format!("plan review not found: {id}"))?;
    let project = item.context.get(CTX_PROJECT).cloned().unwrap_or_default();
    let file = item
        .context
        .get(CTX_FILE)
        .cloned()
        .unwrap_or_else(|| "PLAN.md".to_string());
    let plan_path = PathBuf::from(&project).join(&file);
    let plan = std::fs::read_to_string(&plan_path).unwrap_or_default();
    let doc = crate::notes::read_notes(Path::new(&project)).unwrap_or_default();
    let mut notes=collect_notes(&doc, &file, &plan);
    if let Some(reason)=item.context.get("jury_notes") {
        notes.push(PlanNote{n:notes.len()+1,lines:[1,1],quote:quote_lines(&plan,[1,1]),text:reason.clone()});
    }
    Ok(PlanVerdict {
        id: id.to_string(),
        decision: decision_from_status(&item.status).to_string(),
        plan_path: plan_path.to_string_lossy().to_string(),
        notes,
    })
}

pub async fn handle_review(
    State(ctx): State<crate::agent_hooks::ServerCtx>,
    headers: HeaderMap,
    Json(req): Json<PlanReviewRequest>,
) -> Result<Json<PlanVerdict>, (StatusCode, String)> {
    let session = crate::inbox::authorize(&ctx, &headers).await?;
    let bad = |e: String| (StatusCode::BAD_REQUEST, e);

    if req.plan.trim().is_empty() {
        return Err(bad("a plan review needs a plan".to_string()));
    }
    // A trailing slash survives PathBuf::from, and the pane matches its own
    // trimmed project path against what lands in `context`, so "/a/b/" and
    // "/a/b" would open a review no pane could ever claim.
    let project = PathBuf::from(project_root(&req.project));
    if !project.is_dir() {
        return Err(bad(format!(
            "no such project directory: {}",
            project.display()
        )));
    }
    let file = resolve_plan_file(req.file.as_deref()).map_err(bad)?;
    let plan_path = project.join(&file);
    // A relative filename can still be a symlink to the owner's live tree.
    // Prove the nearest existing ancestor before creating or truncating it.
    let canonical=project.canonicalize().map_err(|e|bad(e.to_string()))?;
    let mut existing=plan_path.clone();
    while std::fs::symlink_metadata(&existing).is_err() {if !existing.pop(){return Err(bad("unresolvable plan path".into()));}}
    if !existing.canonicalize().map_err(|e|bad(e.to_string()))?.starts_with(&canonical) {
        return Err(bad("plan path follows a symlink outside the worktree".into()));
    }
    if let (Some(session),Ok(registry))=(session.as_deref(),crate::agents::registry_dir()) {
        for id in crate::run_control::list_ids_in(&registry).unwrap_or_default() {
            if let Ok(run)=crate::run_control::load_manifest_in(&registry,&id) {
                if run.kind==crate::run_control::RunKind::Agent && !run.state.terminal()
                    && (run.pty_session.as_deref()==Some(session)||run.zellij_session.as_deref()==Some(session))
                    && Path::new(&run.worktree_path).canonicalize().ok().as_ref()!=Some(&canonical) {
                    return Err((StatusCode::FORBIDDEN,"plan file must be submitted from the registered worktree; request outside-worktree authority through the owner inbox".into()));
                }
            }
        }
    }
    if let Some(parent) = plan_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("mkdir: {e}")))?;
    }
    std::fs::write(&plan_path, &req.plan)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("write plan: {e}")))?;
    // The previous round's notes were already delivered with its verdict; kept,
    // they would anchor to lines this revision moved.
    let _ = crate::notes::clear_notes(&project, Some(&file), true);

    let title = match req.title.trim() {
        "" => format!("Review the plan: {file}"),
        given => given.to_string(),
    };

    let mut context: BTreeMap<String, String> = BTreeMap::new();
    context.insert(CTX_PROJECT.to_string(), project.to_string_lossy().to_string());
    context.insert(CTX_FILE.to_string(), file.clone());

    let item = crate::inbox::jury_post(
        None,
        "approve",
        crate::inbox::PostRequest {
            project: req.project.trim().to_string(),
            from: req.from.clone(),
            title,
            body: String::new(),
            context,
            ..Default::default()
        },
        session.clone(),
    )
    .map_err(bad)?;

    // The pane, not the Mesh list, is where a plan gets read. This is what
    // opens it on the right plan with the right pending review attached.
    let _ = ctx.app.emit(
        "plan-review",
        serde_json::json!({
            "id": item.id,
            "project": project.to_string_lossy(),
            "planPath": plan_path.to_string_lossy(),
            "title": item.title,
        }),
    );

    if let Err(reason)=crate::jury_runtime::plan(&ctx.app,&project,session.as_deref(),&plan_path,&req.plan,&item.id,&req.paths,req.spend_estimate) {
        crate::inbox::jury_context(&item.id,"",&reason,"[]").map_err(bad)?;
        crate::inbox::jury_announce_owner(&ctx.app,&item.id).map_err(bad)?;
    }
    let timeout = req.timeout_ms.unwrap_or(120_000);
    settle(&item.id, timeout)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::NOT_FOUND, e))
}

#[derive(Debug, Deserialize)]
pub struct WaitQuery {
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Keep waiting on a review that outlived one long-poll. A plan left on the
/// screen overnight is the normal case, not the edge one.
pub async fn handle_review_wait(
    State(ctx): State<crate::agent_hooks::ServerCtx>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<WaitQuery>,
) -> Result<Json<PlanVerdict>, (StatusCode, String)> {
    crate::inbox::authorize(&ctx, &headers).await?;
    settle(&id, query.timeout_ms.unwrap_or(120_000))
        .await
        .map(Json)
        .map_err(|e| (StatusCode::NOT_FOUND, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::{Annotation, FileNotes};

    fn note(range: [u32; 2], summary: &str) -> Annotation {
        Annotation {
            new_range: Some(range),
            summary: summary.to_string(),
            source: Some("user".into()),
            ..Default::default()
        }
    }

    #[test]
    fn an_unanswered_plan_is_pending_not_approved() {
        // The whole point of blocking: silence must never read as a yes.
        assert_eq!(decision_from_status("open"), "pending");
        assert_eq!(decision_from_status(""), "pending");
        assert_eq!(decision_from_status("answered"), "pending");
    }

    #[test]
    fn sending_a_plan_back_is_changes_requested_not_denied() {
        assert_eq!(decision_from_status("approved"), "approved");
        assert_eq!(decision_from_status("denied"), "changes_requested");
    }

    #[test]
    fn notes_are_numbered_down_the_plan_not_by_click_order() {
        let plan = "# Title\n\nfirst para\n\nsecond para\n";
        let doc = NotesDoc {
            version: 1,
            summary: None,
            files: vec![FileNotes {
                path: "PLAN.md".into(),
                summary: None,
                // Clicked bottom-up, as a reviewer scanning backwards would.
                annotations: vec![note([5, 5], "and this one"), note([3, 3], "fix this")],
            }],
        };
        let got = collect_notes(&doc, "PLAN.md", plan);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].n, 1);
        assert_eq!(got[0].text, "fix this");
        assert_eq!(got[0].quote, "first para");
        assert_eq!(got[1].n, 2);
        assert_eq!(got[1].quote, "second para");
    }

    #[test]
    fn notes_on_another_file_do_not_leak_into_the_plan() {
        let doc = NotesDoc {
            version: 1,
            summary: None,
            files: vec![FileNotes {
                path: "src/lib.rs".into(),
                summary: None,
                annotations: vec![note([1, 1], "unrelated")],
            }],
        };
        assert!(collect_notes(&doc, "PLAN.md", "# Title\n").is_empty());
    }

    #[test]
    fn a_note_carries_its_rationale_too() {
        let doc = NotesDoc {
            version: 1,
            summary: None,
            files: vec![FileNotes {
                path: "PLAN.md".into(),
                summary: None,
                annotations: vec![Annotation {
                    rationale: Some("it races the migration".into()),
                    ..note([1, 1], "drop step 3")
                }],
            }],
        };
        let got = collect_notes(&doc, "PLAN.md", "# Title\n");
        assert_eq!(got[0].text, "drop step 3\nit races the migration");
    }

    #[test]
    fn a_plan_file_cannot_escape_its_project() {
        assert_eq!(resolve_plan_file(None).unwrap(), "PLAN.md");
        assert_eq!(resolve_plan_file(Some("docs/PLAN.md")).unwrap(), "docs/PLAN.md");
        assert!(resolve_plan_file(Some("../../etc/passwd")).is_err());
        assert!(resolve_plan_file(Some("/etc/passwd")).is_err());
    }

    #[test]
    fn a_project_path_is_spelled_the_same_way_the_pane_spells_it() {
        // The pane trims trailing slashes before matching; if the review kept
        // one, the buttons would never appear for the agent that is blocked.
        assert_eq!(project_root("/tmp/proj/"), "/tmp/proj");
        assert_eq!(project_root("  /tmp/proj  "), "/tmp/proj");
        assert_eq!(project_root("/tmp/proj"), "/tmp/proj");
        assert_eq!(project_root("/"), "/");
    }

    #[test]
    fn a_quote_never_runs_past_the_end_of_the_plan() {
        assert_eq!(quote_lines("one\ntwo\n", [2, 9]), "two");
        assert_eq!(quote_lines("one\n", [9, 9]), "");
    }
}
