// Backend context for a delegated turn, including the part Bucki does not do.
//
// Traced from Bucki (48Nauts/Bucky, development @ 629ff06): `liveContext` in
// `Services/OpenAIRealtimeRunner.swift` accumulates "role: text" lines, caps at
// 100 entries, and is joined into the string handed to the backend. We keep
// that shape because it is simple and it works.
//
// The DEPARTURE is what happens when a conversation is reopened. In the
// reference, `resetLiveState()` calls `liveContext.removeAll()` and
// `BuckiManager.init` restores saved history into *UI state only*. So a
// reopened Bucki conversation shows its transcript but starts the backend with
// an empty context — the assistant cannot refer to anything said before the
// restart. `Bucky-REFERENCE.md` flags this as unresolved and asks for it to be
// implemented explicitly. XNAUT-416 requires reopening "with context intact",
// so `restore()` seeds the context from the persisted conversation, and
// `render()` puts it ahead of anything said this session.
//
// Restored turns are context and nothing else: they are never re-committed to
// the conversation and never re-dispatched to an agent, which is what "continue
// without replaying completed work" means.

use crate::voice_live::turn::Role;

/// Matches Bucki's retention. Old enough to hold a long exchange, bounded so a
/// day-long session cannot grow the prompt without limit.
const MAX_LINES: usize = 100;

/// How many restored lines a reopen may contribute. Kept below `MAX_LINES` so
/// a long history can never crowd out what the user just said.
const MAX_RESTORED_LINES: usize = 60;

#[derive(Debug, Default, Clone)]
pub struct ConversationContext {
    /// Lines recovered from the saved conversation when the session opened.
    restored: Vec<String>,
    /// Lines produced during this session.
    live: Vec<String>,
}

impl ConversationContext {
    /// Seeds context from a reopened conversation, oldest first. Anything with
    /// no text is skipped rather than rendered as an empty turn.
    pub fn restore(&mut self, history: &[(Role, String)]) {
        self.restored = history
            .iter()
            .filter_map(|(role, text)| {
                let text = text.trim();
                (!text.is_empty()).then(|| format!("{}: {}", label(*role), text))
            })
            .collect();
        let excess = self.restored.len().saturating_sub(MAX_RESTORED_LINES);
        if excess > 0 {
            self.restored.drain(..excess);
        }
    }

    pub fn push(&mut self, role: Role, text: &str) {
        self.push_line(format!("{}: {}", label(role), text.trim()));
    }

    fn push_line(&mut self, line: String) {
        if line.split_once(": ").is_none_or(|(_, body)| body.is_empty()) {
            return;
        }
        self.live.push(line);
        let excess = self.live.len().saturating_sub(MAX_LINES);
        if excess > 0 {
            self.live.drain(..excess);
        }
    }

    /// The context string handed to the agent. `pending` carries fragments the
    /// ledger has not committed yet — delegation metadata contains no task
    /// text, so without them the agent would be asked to act on nothing.
    pub fn render(&self, pending: &[(Role, String)]) -> String {
        let mut lines = Vec::with_capacity(self.restored.len() + self.live.len() + pending.len());
        if !self.restored.is_empty() {
            lines.push("Earlier in this conversation:".to_string());
            lines.extend(self.restored.iter().cloned());
            lines.push("Continuing now:".to_string());
        }
        lines.extend(self.live.iter().cloned());
        lines.extend(pending.iter().filter_map(|(role, text)| {
            let text = text.trim();
            (!text.is_empty()).then(|| format!("{}: {}", label(*role), text))
        }));
        lines.join("\n")
    }

    pub fn has_restored(&self) -> bool {
        !self.restored.is_empty()
    }

}

fn label(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(pairs: &[(Role, &str)]) -> Vec<(Role, String)> {
        pairs
            .iter()
            .map(|(role, text)| (*role, (*text).to_string()))
            .collect()
    }

    #[test]
    fn a_reopened_conversation_reaches_the_backend() {
        let mut context = ConversationContext::default();
        context.restore(&history(&[
            (Role::User, "My deploy key is named tron-ci."),
            (Role::Assistant, "Noted."),
        ]));
        let rendered = context.render(&[(Role::User, "what was my deploy key called?".into())]);
        assert!(
            rendered.contains("tron-ci"),
            "the fact from the earlier session must be in context: {rendered}"
        );
        assert!(rendered.contains("Earlier in this conversation:"));
        assert!(rendered.contains("Continuing now:"));
    }

    #[test]
    fn restored_history_precedes_this_sessions_speech() {
        let mut context = ConversationContext::default();
        context.restore(&history(&[(Role::User, "older")]));
        context.push(Role::User, "newer");
        let rendered = context.render(&[]);
        let older = rendered.find("older").expect("restored line");
        let newer = rendered.find("newer").expect("live line");
        assert!(older < newer, "chronology must survive the reopen");
    }

    #[test]
    fn a_session_with_no_history_renders_no_restore_preamble() {
        let context = ConversationContext::default();
        let rendered = context.render(&[(Role::User, "hello".into())]);
        assert_eq!(rendered, "user: hello");
        assert!(!context.has_restored());
    }

    #[test]
    fn empty_restored_turns_are_skipped() {
        let mut context = ConversationContext::default();
        context.restore(&history(&[
            (Role::User, "   "),
            (Role::Assistant, "real answer"),
        ]));
        let rendered = context.render(&[]);
        assert!(rendered.contains("real answer"));
        assert_eq!(
            rendered.lines().filter(|l| l.starts_with("user:")).count(),
            0
        );
    }

    #[test]
    fn a_long_history_is_trimmed_to_the_most_recent_turns() {
        let mut context = ConversationContext::default();
        let long: Vec<(Role, String)> = (0..MAX_RESTORED_LINES * 2)
            .map(|n| (Role::User, format!("line {n}")))
            .collect();
        context.restore(&long);
        let rendered = context.render(&[]);
        assert!(
            !rendered.contains("line 0"),
            "the oldest turns are dropped first"
        );
        assert!(
            rendered.contains(&format!("line {}", MAX_RESTORED_LINES * 2 - 1)),
            "the most recent turn is always kept"
        );
    }

    #[test]
    fn live_context_is_bounded_by_the_retention_cap() {
        let mut context = ConversationContext::default();
        for n in 0..MAX_LINES * 2 {
            context.push(Role::User, &format!("line {n}"));
        }
        let rendered = context.render(&[]);
        assert_eq!(rendered.lines().count(), MAX_LINES);
        assert!(!rendered.contains("line 0"));
    }

    #[test]
    fn pending_fragments_reach_the_agent_before_they_commit() {
        let context = ConversationContext::default();
        // Delegation metadata carries no task text; without this the agent
        // would be dispatched with nothing to act on.
        let rendered = context.render(&[(Role::User, "restart the build".into())]);
        assert!(rendered.contains("restart the build"));
    }

    #[test]
    fn a_blank_line_is_never_recorded() {
        let mut context = ConversationContext::default();
        context.push(Role::User, "   ");
        assert_eq!(context.render(&[]), "");
    }
}
