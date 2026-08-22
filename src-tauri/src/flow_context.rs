// The brief an agent gets when it wakes up.
//
// xNAUT already holds everything an agent needs to be useful on a project: the
// NAUT-Flow stage, the tickets, the vault documents, the event trail, and now
// drift. What it has never had is a moment where all of that is HANDED OVER.
// The agent has to know to ask, which means a fresh session opens knowing
// nothing and spends its first turns rediscovering the project — or worse,
// doesn't, and works from assumptions.
//
// The fix is not more context, it is context that arrives unbidden. This module
// builds one packet and renders it two ways:
//
//   OPERATOR  a human glancing at where a project stands.
//   AGENT     the same facts, with paths and ids, ready to act on.
//
// ONE SOURCE, TWO RENDERINGS is the load-bearing part. A summary for humans and
// a separate context file for agents is two things that drift apart, and the
// drift is silent because nobody reads both. Here the renderings cannot
// disagree: they are two formats over one `FlowContext`.
//
// The prior art is 10x's `10x context --mode operator|agent`, described in
// David Ondrej's 2026-08-20 interview with Alex Lieberman and their director of
// engineering. Their framing of the hook that delivers it — "benevolent prompt
// injection", so that "every session starts as a senior engineer on the
// project" — is the goal this serves.
//
// What this is NOT: a judgement. Nothing here decides whether the project is in
// good shape. It reports position, work, documents and drift, and lets the
// reader draw the conclusion. The Validator judges; this briefs.

use crate::flow_drift::{self, DriftFinding, DriftProject, DriftTicket};
use serde::{Deserialize, Serialize};
use tauri::Manager;

/// Everything the brief knows. Serialised as-is for callers that want the data
/// rather than the prose.
#[derive(Debug, Clone, Serialize)]
pub struct FlowContext {
    pub project: String,
    pub name: String,
    pub purpose: String,
    pub flow_type: String,
    /// The AUTHORED stage.
    pub stage: String,
    /// Position on the track, 1-based, and its length: "stage 4 of 15".
    pub stage_index: usize,
    pub stage_count: usize,
    pub ticket_counts: Vec<(String, usize)>,
    /// Tickets an agent could pick up right now, most actionable first.
    pub actionable: Vec<ContextTicket>,
    pub blocked: Vec<ContextTicket>,
    pub documents: Vec<String>,
    pub drift: Vec<DriftFinding>,
    pub recent: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTicket {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub priority: String,
}

/// The input. The caller (frontend or MCP) already has these records; passing
/// them keeps this module pure and testable, and means the brief can be built
/// for a project that is not the active one.
#[derive(Debug, Clone, Deserialize)]
pub struct ContextInput {
    pub key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default = "standard")]
    pub flow_type: String,
    #[serde(default = "idea")]
    pub stage: String,
    #[serde(default)]
    pub tickets: Vec<ContextTicket>,
    #[serde(default)]
    pub documents: Vec<String>,
    #[serde(default)]
    pub recent: Vec<String>,
}

fn standard() -> String {
    "standard".into()
}
fn idea() -> String {
    "idea".into()
}

/// Statuses in the order an agent should care about them.
const STATUS_ORDER: &[&str] = &["in_progress", "review", "ready", "blocked", "inbox", "done"];

fn rank(status: &str) -> usize {
    STATUS_ORDER.iter().position(|s| *s == status).unwrap_or(usize::MAX)
}

/// Builds the packet. Pure.
pub fn build(input: &ContextInput) -> FlowContext {
    let track = flow_drift::track_of(&input.flow_type);
    let stage_index = track.iter().position(|s| *s == input.stage).map(|i| i + 1).unwrap_or(0);

    let mut counts: Vec<(String, usize)> = Vec::new();
    for status in STATUS_ORDER {
        let n = input.tickets.iter().filter(|t| t.status == *status).count();
        if n > 0 {
            counts.push(((*status).to_string(), n));
        }
    }

    // Actionable excludes blocked and done: a brief that lists work an agent
    // cannot start is a brief it learns to skim.
    let mut actionable: Vec<ContextTicket> = input
        .tickets
        .iter()
        .filter(|t| matches!(t.status.as_str(), "in_progress" | "review" | "ready"))
        .cloned()
        .collect();
    actionable.sort_by_key(|t| (rank(&t.status), t.id.clone()));

    let blocked: Vec<ContextTicket> =
        input.tickets.iter().filter(|t| t.status == "blocked").cloned().collect();

    let drift = flow_drift::drift_for(
        &DriftProject {
            key: input.key.clone(),
            stage: input.stage.clone(),
            flow_type: input.flow_type.clone(),
        },
        &input
            .tickets
            .iter()
            .map(|t| DriftTicket { id: t.id.clone(), status: t.status.clone() })
            .collect::<Vec<_>>(),
    );

    FlowContext {
        project: input.key.clone(),
        name: if input.name.is_empty() { input.key.clone() } else { input.name.clone() },
        purpose: input.purpose.clone(),
        flow_type: input.flow_type.clone(),
        stage: input.stage.clone(),
        stage_index,
        stage_count: track.len(),
        ticket_counts: counts,
        actionable,
        blocked,
        documents: input.documents.clone(),
        drift,
        recent: input.recent.clone(),
    }
}

/// The human rendering: where this project stands, in a glance.
pub fn render_operator(ctx: &FlowContext) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} · {}\n", ctx.name, ctx.flow_type));
    if !ctx.purpose.is_empty() {
        out.push_str(&format!("{}\n", ctx.purpose));
    }
    out.push_str(&match ctx.stage_index {
        0 => format!("Stage: {} (not on the {} track)\n", ctx.stage, ctx.flow_type),
        i => format!("Stage: {} — {} of {}\n", ctx.stage, i, ctx.stage_count),
    });

    if ctx.ticket_counts.is_empty() {
        out.push_str("Tickets: none\n");
    } else {
        let parts: Vec<String> =
            ctx.ticket_counts.iter().map(|(s, n)| format!("{n} {s}")).collect();
        out.push_str(&format!("Tickets: {}\n", parts.join(" · ")));
    }

    if !ctx.drift.is_empty() {
        out.push_str(&format!("\nDrift ({}):\n", ctx.drift.len()));
        for d in &ctx.drift {
            out.push_str(&format!("  [{}] {}\n", d.severity, d.detail));
        }
    }
    if !ctx.blocked.is_empty() {
        out.push_str(&format!("\nBlocked ({}):\n", ctx.blocked.len()));
        for t in &ctx.blocked {
            out.push_str(&format!("  {} {}\n", t.id, t.title));
        }
    }
    out
}

/// The agent rendering: the same facts, plus the ids and paths needed to act,
/// and an explicit statement of what to do next.
///
/// Written as a briefing rather than a data dump because it is injected into a
/// prompt. The closing instruction matters: a packet that describes a project
/// without saying what is expected invites the agent to summarise it back.
pub fn render_agent(ctx: &FlowContext) -> String {
    let mut out = String::new();
    out.push_str("<xnaut-project-brief>\n");
    out.push_str(&format!("You are working on {} ({}).\n", ctx.name, ctx.project));
    if !ctx.purpose.is_empty() {
        out.push_str(&format!("Purpose: {}\n", ctx.purpose));
    }
    out.push_str(&match ctx.stage_index {
        0 => format!(
            "NAUT-Flow stage: {} — WARNING: not a stage of the {} track.\n",
            ctx.stage, ctx.flow_type
        ),
        i => format!(
            "NAUT-Flow stage: {} ({} of {}, {} flow).\n",
            ctx.stage, i, ctx.stage_count, ctx.flow_type
        ),
    });

    if !ctx.ticket_counts.is_empty() {
        let parts: Vec<String> =
            ctx.ticket_counts.iter().map(|(s, n)| format!("{n} {s}")).collect();
        out.push_str(&format!("Tickets: {}.\n", parts.join(", ")));
    }

    if !ctx.actionable.is_empty() {
        out.push_str("\nWork you can pick up now (most actionable first):\n");
        for t in ctx.actionable.iter().take(10) {
            let p = if t.priority.is_empty() { String::new() } else { format!(" ({})", t.priority) };
            out.push_str(&format!("  {} [{}]{} {}\n", t.id, t.status, p, t.title));
        }
    }
    if !ctx.blocked.is_empty() {
        out.push_str("\nBlocked — do not start these, they need a decision:\n");
        for t in &ctx.blocked {
            out.push_str(&format!("  {} {}\n", t.id, t.title));
        }
    }

    // Drift is placed last-but-one and phrased as a correction task, because it
    // is the one section that asks the agent to change something rather than
    // just know it.
    if !ctx.drift.is_empty() {
        out.push_str("\nDrift — the project's declared position disagrees with its records:\n");
        for d in &ctx.drift {
            out.push_str(&format!(
                "  [{}] {}: declared \"{}\", records support \"{}\" — {}\n",
                d.severity, d.rule, d.authored, d.derived, d.detail
            ));
            if !d.tickets.is_empty() {
                out.push_str(&format!("      tickets: {}\n", d.tickets.join(", ")));
            }
        }
        out.push_str("  Resolve drift by correcting whichever side is wrong. Do not silently move the stage to make a warning disappear.\n");
    }

    if !ctx.documents.is_empty() {
        out.push_str("\nProject documents (vault, read before planning):\n");
        for d in ctx.documents.iter().take(40) {
            out.push_str(&format!("  {d}\n"));
        }
    }
    if !ctx.recent.is_empty() {
        out.push_str("\nRecent activity:\n");
        for r in ctx.recent.iter().take(15) {
            out.push_str(&format!("  {r}\n"));
        }
    }

    out.push_str("\nThis brief is the project's current state, not a task. Wait for the actual request before changing anything.\n");
    out.push_str("</xnaut-project-brief>\n");
    out
}

/// One command, two modes. `mode` is "operator" or "agent"; anything else is
/// refused rather than defaulted, because a typo silently returning the wrong
/// audience's text is the failure this design exists to prevent.
#[tauri::command]
pub fn flow_context(input: ContextInput, mode: String) -> Result<String, String> {
    let ctx = build(&input);
    match mode.as_str() {
        "operator" => Ok(render_operator(&ctx)),
        "agent" => Ok(render_agent(&ctx)),
        other => Err(format!("unknown mode \"{other}\" — expected \"operator\" or \"agent\"")),
    }
}

/// The structured packet, for callers that want to render it themselves.
#[tauri::command]
pub fn flow_context_data(input: ContextInput) -> FlowContext {
    build(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The brief hook is delivered by three parts that must agree: the script
    /// reads XNAUT_BRIEF_URL, agents.rs derives it from the hook url, and
    /// agent_hooks.rs serves that path. Any one of them alone is dead code —
    /// the hook installs, fires on every session, and prints nothing.
    #[test]
    fn the_brief_is_wired_end_to_end() {
        let script = include_str!("../scripts/hooks/xnaut-brief.sh");
        assert!(script.contains("XNAUT_BRIEF_URL"), "the script reads a different env var");

        let agents = include_str!("agents.rs");
        assert!(
            agents.contains("XNAUT_BRIEF_URL"),
            "nothing injects XNAUT_BRIEF_URL, so the script exits on its first line"
        );
        assert!(
            agents.contains(r#""/v1/hook", "/v1/brief""#),
            "the brief url is not derived from the hook url"
        );

        let hooks = include_str!("agent_hooks.rs");
        assert!(
            hooks.contains(r#".route("/v1/brief""#),
            "nothing serves /v1/brief, so every session start 404s"
        );

        let setup = include_str!("agent_hook_setup.rs");
        assert!(
            setup.contains("SessionStart"),
            "the hook is never installed into a worktree's settings"
        );
    }

    fn t(id: &str, status: &str) -> ContextTicket {
        ContextTicket {
            id: id.into(),
            title: format!("work {id}"),
            status: status.into(),
            priority: "high".into(),
        }
    }

    fn input() -> ContextInput {
        ContextInput {
            key: "XNAUT".into(),
            name: "xNAUT".into(),
            purpose: "the deck".into(),
            flow_type: "standard".into(),
            stage: "build".into(),
            tickets: vec![t("X-1", "in_progress"), t("X-2", "blocked"), t("X-3", "ready"), t("X-4", "done")],
            documents: vec!["xnaut/Development/NAUT-Flow/04-Product-requirements.md".into()],
            recent: vec!["X-4 moved to done".into()],
        }
    }

    #[test]
    fn both_modes_report_the_same_stage() {
        // The whole point of one source, two renderings. If these can disagree
        // the design has already failed.
        let ctx = build(&input());
        let op = render_operator(&ctx);
        let ag = render_agent(&ctx);
        assert!(op.contains("build"), "operator lost the stage: {op}");
        assert!(ag.contains("build"), "agent lost the stage: {ag}");
        assert!(op.contains("12 of 15"), "operator lost the position: {op}");
        assert!(ag.contains("12 of 15"), "agent lost the position: {ag}");
    }

    #[test]
    fn the_agent_brief_never_offers_blocked_work_as_actionable() {
        let ctx = build(&input());
        assert!(ctx.actionable.iter().all(|t| t.status != "blocked"));
        let ag = render_agent(&ctx);
        let actionable_section = ag.split("Blocked").next().unwrap();
        assert!(!actionable_section.contains("X-2"), "blocked ticket offered as work: {ag}");
    }

    #[test]
    fn actionable_work_is_ordered_by_how_actionable_it_is() {
        let ctx = build(&input());
        let ids: Vec<&str> = ctx.actionable.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["X-1", "X-3"], "in_progress must come before ready");
    }

    #[test]
    fn drift_reaches_the_agent_brief_with_its_tickets() {
        let mut i = input();
        i.stage = "release".into();
        let ctx = build(&i);
        assert_eq!(ctx.drift.len(), 1);
        let ag = render_agent(&ctx);
        assert!(ag.contains("stage_ahead_of_tickets"), "drift rule missing: {ag}");
        assert!(ag.contains("X-1"), "drift ticket ids missing: {ag}");
        assert!(ag.contains("Do not silently move the stage"), "correction guidance missing");
    }

    #[test]
    fn a_clean_project_briefs_without_a_drift_section() {
        let mut i = input();
        i.stage = "prd".into();
        i.tickets = vec![t("X-9", "inbox")];
        let ctx = build(&i);
        assert!(ctx.drift.is_empty());
        assert!(!render_agent(&ctx).contains("Drift"));
        assert!(!render_operator(&ctx).contains("Drift"));
    }

    #[test]
    fn an_unknown_mode_is_refused_rather_than_defaulted() {
        let err = flow_context(input(), "Agent".into()).unwrap_err();
        assert!(err.contains("unknown mode"), "{err}");
    }

    #[test]
    fn the_brief_says_it_is_not_a_task() {
        // Without this the agent reads a wall of project state as an
        // instruction and starts summarising it back, or worse, acting.
        let ag = render_agent(&build(&input()));
        assert!(ag.contains("not a task"), "{ag}");
    }

    #[test]
    fn an_empty_project_still_produces_a_usable_brief() {
        let i = ContextInput {
            key: "NEW".into(),
            name: String::new(),
            purpose: String::new(),
            flow_type: "feature".into(),
            stage: "idea".into(),
            tickets: Vec::new(),
            documents: Vec::new(),
            recent: Vec::new(),
        };
        let ctx = build(&i);
        assert_eq!(ctx.name, "NEW", "an unnamed project must fall back to its key");
        let op = render_operator(&ctx);
        assert!(op.contains("Tickets: none"));
        assert!(render_agent(&ctx).contains("1 of 11"), "feature track length wrong");
    }
}

/// GET /v1/brief?cwd=… — the SessionStart hook's endpoint.
///
/// The hook prints whatever this returns straight into the agent's context, so
/// the body is the agent-mode brief and nothing else: no JSON envelope, no error
/// text. A failure answers 204 with an empty body rather than an error string,
/// because the alternative is a curl error pasted into the session as though the
/// project had said it.
///
/// The project is resolved from `cwd` — the deepest `source_path` that is a
/// prefix of it, so a worktree under a project's folder still resolves to that
/// project. No match means no brief, which is correct: an agent working outside
/// any known project has nothing to be briefed about.
pub async fn handle_brief(
    axum::extract::State(ctx): axum::extract::State<crate::agent_hooks::ServerCtx>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let empty = || (axum::http::StatusCode::NO_CONTENT, String::new()).into_response();
    if crate::inbox::authorize(&ctx, &headers).await.is_err() {
        return empty();
    }
    let Some(cwd) = params.get("cwd") else { return empty() };

    let settings = {
        let guard = ctx.app.state::<crate::state::AppState>();
        let settings = guard.settings.lock().await;
        settings.project_management.clone()
    };
    let Ok(repo) = crate::project_management::configured_repo(&settings) else { return empty() };
    let Ok(projects) = crate::project_management::list_projects(&repo) else { return empty() };

    // Deepest matching source_path wins: /repo and /repo/.worktrees/x are both
    // prefixes of a worktree cwd, and the more specific one is the right answer.
    let Some(project) = projects
        .iter()
        .filter(|p| !p.source_path.is_empty() && cwd.starts_with(&p.source_path))
        .max_by_key(|p| p.source_path.len())
    else {
        return empty();
    };

    let tickets: Vec<ContextTicket> = std::fs::read_dir(
        repo.join("projects").join(&project.key).join("tickets"),
    )
    .map(|entries| {
        entries
            .flatten()
            .filter(|e| e.path().extension().and_then(|v| v.to_str()) == Some("json"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .map(|v| ContextTicket {
                id: v["id"].as_str().unwrap_or_default().to_string(),
                title: v["title"].as_str().unwrap_or_default().to_string(),
                status: v["status"].as_str().unwrap_or_default().to_string(),
                priority: v["priority"].as_str().unwrap_or_default().to_string(),
            })
            .collect()
    })
    .unwrap_or_default();

    let input = ContextInput {
        key: project.key.clone(),
        name: project.name.clone(),
        purpose: project.purpose.clone(),
        flow_type: project.flow_type.clone(),
        stage: project.stage.clone(),
        tickets,
        documents: Vec::new(),
        recent: Vec::new(),
    };
    (axum::http::StatusCode::OK, render_agent(&build(&input))).into_response()
}
