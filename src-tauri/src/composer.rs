// The ONE prompt composer (XNAUT-127).
//
// Everything an agent is told is assembled here, in this order:
//   1. the xNAUT Foundation      how to exist here, and how to reach André
//   2. its own instructions      the persona from its profile
//   3. what it may not do        only the policy lines nothing enforces
//   4. the skills it has         names, one-liners and PATHS — never bodies
//   5. the task
//
// Why one place: the 2026-08-10 audit found the same composition copy-pasted
// three times, with the BAMT path building a fourth of its own, so a change to
// the preamble reached some agents and not others. A single function is the
// only way "we told every agent X" can be a true statement.
//
// Why paths and not bodies for skills: preloading instructions spends the
// context window before the task is read. The agent gets a menu and opens
// what it needs.

use crate::agent_profiles::AgentProfile;

/// The enabled-skills block. Absolute paths, so any runtime can read them —
/// including the ones that have no skill mechanism of their own.
fn skills_block(profile: &AgentProfile) -> String {
    let enabled: Vec<String> = profile
        .capabilities
        .iter()
        .filter_map(|entry| entry.strip_prefix("skill:").map(str::to_string))
        .collect();
    if enabled.is_empty() {
        return String::new();
    }
    let catalog = crate::skills::skill_catalog(None).unwrap_or_default();
    let mut lines = Vec::new();
    for name in enabled {
        match catalog.iter().find(|skill| skill.name == name) {
            Some(skill) => {
                let description = if skill.description.trim().is_empty() {
                    String::new()
                } else {
                    format!(" — {}", skill.description.trim())
                };
                lines.push(format!("- **{}**{}\n  Read: {}", skill.name, description, skill.path));
            }
            // A skill can vanish (a folder moved, a harness uninstalled).
            // Saying so is better than silently dropping a capability the
            // owner believes is switched on.
            None => lines.push(format!("- **{name}** — enabled but not found on disk right now.")),
        }
    }
    format!(
        "\n## Your skills\n\nRead a skill's file before using it; do not guess at its steps.\n\n{}\n",
        lines.join("\n")
    )
}

/// The agents this one may hand work to.
///
/// Nothing blocks a hand-off at dispatch: there is no hand-off tool and no
/// caller of this list outside the prompt, so an enforced-sounding label on the
/// Collaborators tab would be an overclaim. Saying it in the prompt is what the
/// setting actually buys, and it is worth the four lines: without it the chips
/// are stored on the profile and read by nobody.
fn collaborators_block(profile: &AgentProfile) -> String {
    let handles: Vec<String> = profile
        .capabilities
        .iter()
        .filter_map(|entry| entry.strip_prefix("collab:").map(|handle| format!("@{handle}")))
        .collect();
    if handles.is_empty() {
        return String::new();
    }
    format!(
        "\n## Who you may hand work to\n\n{}\n\nNothing stops you asking someone else; this is who the owner picked.\n",
        handles.join(", ")
    )
}

fn policy_block(profile: &AgentProfile) -> String {
    let lines = crate::policy::advisory_lines(&profile.runtime_id, &profile.policy);
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "\n## Limits for this run\n\n{}\n",
        lines
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn persona_block(profile: &AgentProfile) -> String {
    if profile.purpose.trim().is_empty() {
        return String::new();
    }
    format!("\n## Your instructions\n\n{}\n", profile.purpose.trim())
}

/// The full preamble plus the task. `resume` skips everything but the task:
/// a continuing conversation already carries it, and repeating it every turn
/// would burn the window and read as nagging.
///
/// `conventions` is the project's standing-conventions block (markers.rs,
/// ENGRAMOSS-10). It sits ABOVE the persona for the same reason the
/// Foundation does: it is not this agent's opinion, it is the project's
/// decision, and an agent that reads its own instructions first will answer a
/// convention question from its persona instead of from the marker. None when
/// the project has written none down, which is most of them today.
pub fn compose(
    profile: &AgentProfile,
    hook_url: &str,
    task: &str,
    resume: bool,
    conventions: Option<&str>,
) -> String {
    if resume {
        return task.to_string();
    }
    let mut out = crate::foundation::text_with_hook(hook_url);
    out.push('\n');
    if let Some(block) = conventions {
        out.push_str(block);
    }
    out.push_str(&persona_block(profile));
    out.push_str(&policy_block(profile));
    out.push_str(&skills_block(profile));
    out.push_str(&collaborators_block(profile));
    out.push_str("\n## Task\n\n");
    out.push_str(task.trim());
    out.push('\n');
    out
}

/// The marker a chat turn puts on its first line when the request needs a
/// coding harness. Deterministic beats sentiment analysis: one exact token the
/// UI can test for, rather than guessing intent from prose.
pub const BUILD_MARKER: &str = "BUILD-REQUEST";

/// The rules half of a chat turn's system prompt. A raw string so what the
/// model receives is exactly what is written here — the escaped version was
/// unreadable and, worse, untestable against the live model.
pub const CHAT_RULES: &str = r#"You are in an xNAUT chat turn. Your actual tools are the schemas supplied with this request, including connected plugins and tool discovery. Do not assume a filesystem, shell or network tool is absent: inspect the supplied tools and discover any deferred tools first. A tool mentioned in your persona may belong only to a coding runtime; do not invent a call to it. Answer questions directly and briefly.

Asked for a document — a report, a spec, release notes, a plan, anything longer than a couple of paragraphs? Write it with write_document. It opens beside the conversation where it can be read and saved, instead of scrolling past in chat. Read it first if one exists, and send the complete document when you rewrite it.

Asked for a diagram, a map, an architecture or a flow? Draw it with update_canvas — it appears on the owner's canvas beside this conversation. Read the canvas first if one exists; the visible graph is authoritative. Send the COMPLETE graph every time, reusing ids so boxes he has moved stay where he put them. A drawing is never a build.

Use the tools you have before asking for anything. A connected plugin's tools are yours: if one of them can do the job — draw the diagram, read the repository, search the docs — call it and answer. A drawing is not a build. If a local plugin's server is not running, start it with start_local_service rather than reporting that it will not connect.

For reviews of a published task PR, use request_repository_review with the saved review_task run_id from registered context. It preserves the original PR, exact-commit review and project merge gates; do not launch a duplicate generic task. Queue status alone is not proof a worker started.
For authorized commands, audits, tests or code changes, call start_repository_task with the registered or user-named repository. Every agent can create its own isolated worktree and launch its own runtime; no NautBot delegation or owner-created worktree is required. Use create_worktree only when preparing a workspace without launching work. Before any worktree creation or worker launch, find or create a ticket in the correct project with the full requirements and acceptance criteria. Use prepare_repository_ticket to resolve the project from its authorized repository and create tracking without asking the owner to register it manually. No work without a ticket. Never substitute an unrelated default project or retry a refused launch without a ticket. Keep task_key stable on retries, pass the ticket ID, and preserve its scope. Track run IDs, branches, blockers, PRs and verification on that ticket; launching is not completion. A launch receipt proves launch, not progress or completion. Use BUILD-REQUEST on the first line followed by a one-line task only when the repository cannot be resolved; the UI then asks for the missing location.

Creating an agent is NOT a build either: call create_agent. Opening a repository to write an agent by hand is the wrong answer to "make me an agent called X".

Installing a plugin or an MCP server is NOT a build, and it is not something to hand back as instructions. You have tools for it: call connect_plugin, which finds any credential xNAUT already holds, switches the plugin on, proves it starts, and hands it to this agent. Then say what happened in one line — "Forgejo connected" — or, if a credential genuinely could not be found anywhere, name exactly which one.

Never answer a request to add, connect or enable something by describing where to click. Do it, check it, report it.

Attaching a terminal is view-only: it starts no command, scanner or worker. Creating a ticket or marking it in_progress is bookkeeping, not proof of execution. For audits/reviews, inspect the named repository or use the actual scanner. Registered project context supplies known local repository paths. If Aikido is unavailable, read-only repository inspection can still make progress; disclose the scanner coverage gap. If commands are required, call start_repository_task rather than attaching an idle session. Only NautBot dispatches work to OTHER agents; every agent can create a worktree and start its own authorized task.

Never say you did something you did not do. Every action you take here is a tool call, and the owner can see which tools ran under your answer: an answer that claims work with no tool behind it is visibly a story, and it costs him a debugging session. If you did not call it, say so. If it failed, quote the error. If you are about to write "started", "running" or "assigned", check that the tool actually returned that, and say plainly when you cannot.

A blocker is work, not an ending. Diagnose it, try the obvious fix, and only then ask - one precise question with concrete options, never a shrug. Going quiet with the job half done is the one thing that makes you useless.

When something fails, troubleshoot it before you report it. A connector that dies with "could not determine executable to run" means the package ships no runnable binary: call inspect_package on it, search_packages for one that does, repair_plugin with the working command, and connect again. Come back with "Forgejo connected" and one line on what you changed. Report a failure only when you have actually tried to fix it and cannot.
"#;

/// System prompt for a CHAT turn — the default way to talk to an agent.
///
/// Talking to an agent must not start a coding session. Asking NautBot for a
/// status or how to install something is a question, and answering it by
/// spawning a CLI harness in a worktree is both slow and wrong. So a message
/// goes to the agent's own baseline model, and the harness is reserved for
/// work that actually touches a repository — which the agent asks for first.
pub fn chat_system(profile: &AgentProfile) -> String {
    let mut out = format!(
        "You are {} (@{}), one of the agents in xNAUT.\n",
        profile.display_name.trim(),
        profile.handle
    );
    if !profile.purpose.trim().is_empty() {
        out.push_str(&format!("\n{}\n", profile.purpose.trim()));
    }
    out.push('\n');
    out.push_str(CHAT_RULES);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::AgentPolicy;

    fn profile(capabilities: Vec<String>, runtime: &str) -> AgentProfile {
        let mut profile = crate::agent_profiles::AgentProfile {
            handle: "planner".into(),
            display_name: "Planner".into(),
            tagline: String::new(),
            purpose: "Turn an approved spec into an ordered plan.".into(),
            runtime_id: runtime.into(),
            provider: "anthropic".into(),
            model: String::new(),
            chat_provider: String::new(),
            chat_model: String::new(),
            reasoning_effort: String::new(),
            max_parallel: crate::swarm_plan::DEFAULT_MAX_PARALLEL as u32,
            execution: crate::agent_profiles::AgentExecution::Local,
            role: "planner".into(),
            capabilities,
            notifications: true,
            policy: AgentPolicy::default(),
            accent_color: "#f5b840".into(),
            default_project: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        profile.policy.filesystem = "read-only".into();
        profile
    }

    #[test]
    fn every_agent_is_told_how_to_reach_andre() {
        // The inbox is useless if the agent never learns the endpoint; this is
        // the line that turns Mesh from a UI into a habit.
        let composed = compose(&profile(vec![], "claude"), "http://127.0.0.1:9/", "Ship it", false, None);
        assert!(composed.contains("/v1/inbox/ask"));
        assert!(composed.contains("http://127.0.0.1:9/v1/inbox/wait/"));
    }

    #[test]
    fn the_persona_and_the_task_both_survive() {
        let composed = compose(&profile(vec![], "claude"), "http://x", "Write the plan", false, None);
        assert!(composed.contains("Turn an approved spec into an ordered plan."));
        assert!(composed.trim_end().ends_with("Write the plan"));
    }

    #[test]
    fn the_collaborators_the_owner_picked_reach_the_prompt() {
        // The chips were written onto the profile and read by nobody, while the
        // tab said "Enforced at dispatch". The prompt is the only place this
        // list can be true, so it has to actually arrive there.
        let composed = compose(
            &profile(vec!["collab:rudi".into(), "skill:none".into(), "collab:nautbot".into()], "claude"),
            "http://x",
            "t",
            false,
            None,
        );
        assert!(composed.contains("Who you may hand work to"));
        assert!(composed.contains("@rudi, @nautbot"));
        // No collaborators means no section: an empty heading reads as a limit
        // that was set and left blank.
        assert!(!compose(&profile(vec![], "claude"), "http://x", "t", false, None)
            .contains("Who you may hand work to"));
    }

    #[test]
    fn only_unenforced_limits_are_stated() {
        // claude enforces read-only by removing the write tools, so repeating
        // it in prose would imply the agent has a choice.
        let claude = compose(&profile(vec![], "claude"), "http://x", "t", false, None);
        assert!(!claude.contains("Limits for this run"));
        // gemini has no lever, so the prompt is all that is left.
        let gemini = compose(&profile(vec![], "gemini"), "http://x", "t", false, None);
        assert!(gemini.contains("Limits for this run"));
        assert!(gemini.contains("read-only"));
    }

    #[test]
    fn skills_are_announced_as_paths_never_as_bodies() {
        let composed = compose(
            &profile(vec!["skill:definitely-not-installed".into()], "claude"),
            "http://x",
            "t",
            false,
            None,
        );
        assert!(composed.contains("## Your skills"));
        // An enabled skill that is missing must say so rather than quietly
        // disappear, or the owner keeps believing it is switched on.
        assert!(composed.contains("not found on disk"));
    }

    #[test]
    fn a_chat_turn_refuses_to_pretend_it_can_build() {
        // The whole point of the chat transport: a question must not start a
        // coding session, and the agent must say so in a way the UI can act
        // on rather than describing what it would have done.
        let system = chat_system(&profile(vec![], "claude"));
        assert!(system.contains("actual tools are the schemas supplied"));
        assert!(system.contains("do not invent a call"));
        assert!(!system.contains("no filesystem, no shell"));
        assert!(system.contains(BUILD_MARKER));
        assert!(system.contains("Turn an approved spec into an ordered plan."));
    }

    #[test]
    fn a_resumed_turn_carries_only_the_task() {
        let composed = compose(&profile(vec![], "claude"), "http://x", "next step", true, None);
        assert_eq!(composed, "next step");
    }
}
