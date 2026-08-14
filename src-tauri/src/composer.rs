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
pub fn compose(profile: &AgentProfile, hook_url: &str, task: &str, resume: bool) -> String {
    if resume {
        return task.to_string();
    }
    let mut out = crate::foundation::text_with_hook(hook_url);
    out.push('\n');
    out.push_str(&persona_block(profile));
    out.push_str(&policy_block(profile));
    out.push_str(&skills_block(profile));
    out.push_str("\n## Task\n\n");
    out.push_str(task.trim());
    out.push('\n');
    out
}

/// The marker a chat turn puts on its first line when the request needs a
/// coding harness. Deterministic beats sentiment analysis: one exact token the
/// UI can test for, rather than guessing intent from prose.
pub const BUILD_MARKER: &str = "BUILD-REQUEST";

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
    out.push_str(&format!(
        "\nYou are in a chat turn: no filesystem, no shell, no network tools. \
Answer questions directly and briefly.\n\n\
If the request needs code written, files changed, or commands run, do NOT \
pretend to do it and do NOT describe how you would. Reply with exactly \
`{BUILD_MARKER}` on the first line, then ONE line naming what you would \
build. xNAUT will ask the owner for the repository and open a worktree for \
you to work in.\n"
    ));
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
            reasoning_effort: String::new(),
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
        let composed = compose(&profile(vec![], "claude"), "http://127.0.0.1:9/", "Ship it", false);
        assert!(composed.contains("/v1/inbox/ask"));
        assert!(composed.contains("http://127.0.0.1:9/v1/inbox/wait/"));
    }

    #[test]
    fn the_persona_and_the_task_both_survive() {
        let composed = compose(&profile(vec![], "claude"), "http://x", "Write the plan", false);
        assert!(composed.contains("Turn an approved spec into an ordered plan."));
        assert!(composed.trim_end().ends_with("Write the plan"));
    }

    #[test]
    fn only_unenforced_limits_are_stated() {
        // claude enforces read-only by removing the write tools, so repeating
        // it in prose would imply the agent has a choice.
        let claude = compose(&profile(vec![], "claude"), "http://x", "t", false);
        assert!(!claude.contains("Limits for this run"));
        // gemini has no lever, so the prompt is all that is left.
        let gemini = compose(&profile(vec![], "gemini"), "http://x", "t", false);
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
        assert!(system.contains("no filesystem, no shell"));
        assert!(system.contains(BUILD_MARKER));
        assert!(system.contains("Turn an approved spec into an ordered plan."));
    }

    #[test]
    fn a_resumed_turn_carries_only_the_task() {
        let composed = compose(&profile(vec![], "claude"), "http://x", "next step", true);
        assert_eq!(composed, "next step");
    }
}
