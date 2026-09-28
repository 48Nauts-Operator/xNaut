// Turn accounting: exactly one committed user message and one authoritative
// assistant message per spoken turn, however many fragments, retries or
// duplicate delegation events the transport delivers.
//
// This is the deliberate DEPARTURE from Bucki (48Nauts/Bucky, development @
// 629ff06, `Services/OpenAIRealtimeRunner.swift::flushLiveTranscript`). Bucki
// appends a transcript entry per *flush*, grouping fragments into "readable
// captions" on an 80-character / terminal-punctuation / one-second-idle rule.
// A single long utterance therefore becomes several chat messages there. That
// is fine for a transcript ribbon and wrong for a conversation that xNAUT has
// to reopen later: XNAUT-416 requires exactly one committed user message per
// turn. So we keep Bucki's fragment grouping for *live captions* and add a
// turn-scoped ledger that decides what actually enters the conversation.
//
// The second departure: the authoritative assistant message is the delegated
// xNAUT agent's answer, not the voice model's paraphrase. The architecture is
// explicit that Live "may paraphrase and generate conversational speech", so
// its output transcript is presentation metadata and the agent's result is the
// record. Both are retained; only one is committed.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// A message the conversation should persist. Emitted at most once per
/// (turn, role) for the life of the session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommittedMessage {
    pub turn: u64,
    pub role: Role,
    pub text: String,
}

#[derive(Debug, Default, Clone)]
struct Turn {
    /// Everything the user said this turn, in arrival order.
    user_fragments: String,
    /// What the voice model said. Kept for captions and for the backend
    /// context; never the committed assistant message.
    spoken_fragments: String,
    user_committed: bool,
    assistant_committed: bool,
    /// Delegation ids seen for this turn. Live can raise more than one; the
    /// first owns the turn and later ones must not start a second agent run.
    delegation: Option<String>,
    dispatched: bool,
}

#[derive(Debug, Default, Clone)]
pub struct TurnLedger {
    turns: Vec<Turn>,
    /// The turn currently accepting user speech, if any. `None` between turns.
    open: Option<u64>,
}

impl TurnLedger {
    /// Appends a fragment of user speech, opening a turn if none is open.
    /// Returns the turn it landed in.
    pub fn push_user(&mut self, text: &str) -> u64 {
        let turn = match self.open {
            Some(turn) => turn,
            None => {
                self.turns.push(Turn::default());
                let turn = self.turns.len() as u64 - 1;
                self.open = Some(turn);
                turn
            }
        };
        self.turns[turn as usize].user_fragments.push_str(text);
        turn
    }

    /// Appends a fragment of the voice model's speech to the open turn. Speech
    /// with no turn open is the model talking outside a user exchange — a
    /// greeting, say — and is dropped rather than inventing a turn for it.
    pub fn push_spoken(&mut self, text: &str) {
        if let Some(turn) = self.open {
            self.turns[turn as usize].spoken_fragments.push_str(text);
        }
    }

    /// Binds a delegation id to the open turn. Returns the turn to dispatch,
    /// or `None` when there is nothing to dispatch: no open turn, no speech
    /// recognised yet, or this turn already went to the agent. That last case
    /// is what stops a repeated `session.delegation.created` from starting a
    /// second run of the same work.
    pub fn open_delegation(&mut self, id: &str) -> Option<u64> {
        let turn = self.open?;
        let entry = &mut self.turns[turn as usize];
        if entry.user_fragments.trim().is_empty() || entry.dispatched {
            return None;
        }
        entry.delegation = Some(id.to_string());
        entry.dispatched = true;
        Some(turn)
    }

    pub fn delegation_of(&self, turn: u64) -> Option<&str> {
        self.turns.get(turn as usize)?.delegation.as_deref()
    }

    /// Commits the user's message for a turn. `None` on the second call, on an
    /// unknown turn, or when nothing intelligible was said.
    pub fn commit_user(&mut self, turn: u64) -> Option<CommittedMessage> {
        let entry = self.turns.get_mut(turn as usize)?;
        if entry.user_committed {
            return None;
        }
        let text = entry.user_fragments.trim().to_string();
        if text.is_empty() {
            return None;
        }
        entry.user_committed = true;
        Some(CommittedMessage {
            turn,
            role: Role::User,
            text,
        })
    }

    /// Commits the agent's answer for a turn and closes it. `None` on the
    /// second call — which is what makes a delegation retry, a duplicate
    /// completion event and a reconnect all safe.
    pub fn commit_assistant(&mut self, turn: u64, text: &str) -> Option<CommittedMessage> {
        let entry = self.turns.get_mut(turn as usize)?;
        if entry.assistant_committed {
            return None;
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            return None;
        }
        entry.assistant_committed = true;
        if self.open == Some(turn) {
            self.open = None;
        }
        Some(CommittedMessage {
            turn,
            role: Role::Assistant,
            text,
        })
    }

    /// Ends the open turn without an answer — the session is closing, or the
    /// agent failed. The user's words are still committed, because they were
    /// really said and the conversation should show them.
    pub fn abandon_open(&mut self) -> Option<u64> {
        self.open.take()
    }

    pub fn open_turn(&self) -> Option<u64> {
        self.open
    }

    /// Fragments of the open turn that no commit has consumed yet. Delegation
    /// metadata carries no task text, so the dispatcher needs these.
    pub fn open_fragments(&self) -> (String, String) {
        self.open
            .and_then(|turn| self.turns.get(turn as usize))
            .map(|entry| {
                (
                    entry.user_fragments.trim().to_string(),
                    entry.spoken_fragments.trim().to_string(),
                )
            })
            .unwrap_or_default()
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn many_fragments_become_one_user_message() {
        let mut ledger = TurnLedger::default();
        // A real utterance arrives in small pieces, several of them mid-word.
        for fragment in ["What ", "is ", "the ", "build ", "status", "?"] {
            assert_eq!(ledger.push_user(fragment), 0);
        }
        let committed = ledger.commit_user(0).expect("one user message");
        assert_eq!(committed.text, "What is the build status?");
        assert_eq!(committed.role, Role::User);
        assert_eq!(
            ledger.commit_user(0),
            None,
            "a second commit must not duplicate the message"
        );
    }

    #[test]
    fn a_repeated_delegation_event_does_not_dispatch_twice() {
        let mut ledger = TurnLedger::default();
        ledger.push_user("run the tests");
        assert_eq!(ledger.open_delegation("d1"), Some(0));
        assert_eq!(
            ledger.open_delegation("d2"),
            None,
            "the turn is already with the agent"
        );
        assert_eq!(ledger.delegation_of(0), Some("d1"), "the first id owns it");
    }

    #[test]
    fn delegation_before_any_speech_is_not_dispatched() {
        let mut ledger = TurnLedger::default();
        assert_eq!(ledger.open_delegation("d1"), None, "no turn is open");
        ledger.push_user("   ");
        assert_eq!(
            ledger.open_delegation("d1"),
            None,
            "whitespace is not a request"
        );
    }

    #[test]
    fn the_assistant_message_commits_once_across_retries() {
        let mut ledger = TurnLedger::default();
        ledger.push_user("hello");
        ledger.open_delegation("d1");
        ledger.commit_user(0);
        let first = ledger.commit_assistant(0, "Hi there.").expect("committed");
        assert_eq!(first.text, "Hi there.");
        assert_eq!(first.role, Role::Assistant);
        assert_eq!(
            ledger.commit_assistant(0, "Hi there again."),
            None,
            "a retry must not append a second answer"
        );
    }

    #[test]
    fn a_new_turn_opens_after_the_answer_lands() {
        let mut ledger = TurnLedger::default();
        ledger.push_user("first");
        ledger.open_delegation("d1");
        ledger.commit_user(0);
        ledger.commit_assistant(0, "one");
        assert_eq!(ledger.open_turn(), None);

        assert_eq!(ledger.push_user("second"), 1, "speech opens turn 1");
        ledger.open_delegation("d2");
        assert_eq!(ledger.commit_user(1).unwrap().text, "second");
        assert_eq!(ledger.commit_assistant(1, "two").unwrap().text, "two");
    }

    #[test]
    fn the_voice_models_paraphrase_is_never_the_committed_answer() {
        let mut ledger = TurnLedger::default();
        ledger.push_user("status?");
        ledger.push_spoken("Let me check that for you...");
        ledger.open_delegation("d1");
        let (heard, spoken) = ledger.open_fragments();
        assert_eq!(heard, "status?");
        assert_eq!(spoken, "Let me check that for you...");
        // Only the agent's answer commits.
        let committed = ledger.commit_assistant(0, "Build 412 passed.").unwrap();
        assert_eq!(committed.text, "Build 412 passed.");
    }

    #[test]
    fn model_speech_outside_a_turn_is_dropped() {
        let mut ledger = TurnLedger::default();
        ledger.push_spoken("Good morning!");
        assert_eq!(ledger.open_turn(), None, "a greeting is not a turn");
        assert_eq!(ledger.open_fragments(), (String::new(), String::new()));
    }

    #[test]
    fn an_empty_answer_commits_nothing_and_leaves_the_turn_open() {
        let mut ledger = TurnLedger::default();
        ledger.push_user("hello");
        assert_eq!(ledger.commit_assistant(0, "   "), None);
        assert_eq!(ledger.open_turn(), Some(0));
        // The real answer still commits afterwards.
        assert!(ledger.commit_assistant(0, "Hi.").is_some());
    }

    #[test]
    fn abandoning_a_turn_still_allows_the_user_message_to_commit() {
        let mut ledger = TurnLedger::default();
        ledger.push_user("did you get that");
        assert_eq!(ledger.abandon_open(), Some(0));
        assert_eq!(ledger.open_turn(), None);
        assert_eq!(
            ledger.commit_user(0).unwrap().text,
            "did you get that",
            "what was really said is still part of the conversation"
        );
    }

    #[test]
    fn committing_an_unknown_turn_is_not_a_panic() {
        let mut ledger = TurnLedger::default();
        assert_eq!(ledger.commit_user(7), None);
        assert_eq!(ledger.commit_assistant(7, "hello"), None);
        assert_eq!(ledger.delegation_of(7), None);
    }
}
