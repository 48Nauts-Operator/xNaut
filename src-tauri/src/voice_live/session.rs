// The continuous voice conversation, as a pure state machine.
//
// Ported from Bucki (48Nauts/Bucky, development @ 629ff06),
// `Services/OpenAIRealtimeRunner.swift::handleLiveEvent` / `runCodexDelegation`
// / `resetLiveState`. Licence: the Bucky repository carries no distributed
// licence file; this is a reimplementation of the observed behaviour, not
// copied source.
//
// Why a state machine with no I/O in it: the acceptance for XNAUT-416 is about
// ordering and identity — one committed message per turn, no stale audio after
// a barge-in, no second agent run from a repeated event. Those are decidable
// from the event sequence alone, so the sequence is all this file knows about.
// Sockets, audio devices and agent processes live in `mod.rs` and consume the
// `SessionAction`s returned here. That is what lets the tests drive a full
// multi-turn conversation deterministically.
//
// Departures from the reference, all deliberate:
//  * Delegation goes to the selected xNAUT agent over its configured route.
//    Bucki's `CodexVoiceBackend` forces `forced_login_method="chatgpt"`,
//    `--ignore-user-config` and strips `OPENAI_API_KEY`/`OPENAI_BASE_URL`.
//    Copying that would bypass NautGate. See `Bucky-REFERENCE.md`.
//  * `epoch` is an explicit counter rather than a fresh UUID per reset. The
//    counter is ordered, so a late result can be recognised as *older* instead
//    of merely different.
//  * Turn accounting and context replay depart as documented in `turn.rs` and
//    `context.rs`.

use crate::voice_live::context::ConversationContext;
use crate::voice_live::gate::InterruptionGate;
use crate::voice_live::protocol::{commentary_chunks, ClientEvent, ServerEvent};
use crate::voice_live::turn::{CommittedMessage, Role, TurnLedger};
use serde::{Deserialize, Serialize};

/// How the voice model is told to behave. It speaks; it does not work. Every
/// substantive request goes to the xNAUT agent the user already selected, so
/// the spoken answer and the written conversation come from the same place.
pub const VOICE_INSTRUCTIONS: &str = "\
You are the voice of xNAUT. Speak naturally and concisely.
Delegate every substantive question, lookup, reasoning step and action to your backend.
The backend is the user's selected xNAUT agent and holds its tools and permissions.
Never claim an action succeeded before its result comes back.
Let the user interrupt or add detail while work is running, and relay corrections.
Treat typed messages as requests too. Give backend results in clear spoken language.";

/// Which agent, project and permissions a dispatched turn belongs to. Captured
/// once when the session opens and attached to every delegation, so a turn can
/// never be executed against a different agent than the one on screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionBinding {
    /// The xNAUT conversation this voice session is pinned to.
    pub conversation_id: String,
    pub agent: String,
    pub project: String,
    pub provider: String,
    pub model: String,
    /// The effective permission context the agent runs under, as the app
    /// already resolved it. Carried, never re-derived here.
    pub permission: String,
}

/// One unit of work for the selected xNAUT agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationRequest {
    pub turn: u64,
    /// The session epoch this was dispatched under. A result carrying an older
    /// epoch belongs to a retired connection and is dropped.
    pub epoch: u64,
    pub delegation_id: String,
    /// Restored history, this session's turns, and the words that have not
    /// committed yet. Delegation metadata carries no task text.
    pub context: String,
    pub binding: ExecutionBinding,
}

/// A live caption. Presentation metadata only — never the conversation record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    pub turn: Option<u64>,
    pub role: Role,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionAction {
    /// Send this to the provider.
    Send(ClientEvent),
    /// Play assistant audio. `generation` fences it: the renderer drops a frame
    /// whose generation is behind the session's current one.
    Play { generation: u64, pcm: Vec<u8> },
    /// Stop playback now and discard anything queued.
    StopPlayback { generation: u64 },
    /// Persist this into the xNAUT conversation. At most one per turn per role.
    Commit(CommittedMessage),
    /// Run this on the selected xNAUT agent.
    Dispatch(DelegationRequest),
    /// Update the on-screen transcript ribbon.
    Caption(Caption),
    Ended { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Opened locally; the provider has not accepted the session yet.
    Starting,
    Ready,
    Ended,
}

pub struct LiveSession {
    epoch: u64,
    playback_generation: u64,
    phase: Phase,
    binding: ExecutionBinding,
    gate: InterruptionGate,
    ledger: TurnLedger,
    context: ConversationContext,
    model: String,
    /// True once assistant audio has played and nothing has stopped it. The
    /// energy heuristic needs to know whether there is anything to interrupt.
    output_active: bool,
    transcription_only: bool,
}

impl LiveSession {
    pub fn new(binding: ExecutionBinding, model: String) -> Self {
        Self {
            epoch: 0,
            playback_generation: 0,
            phase: Phase::Starting,
            binding,
            gate: InterruptionGate::default(),
            ledger: TurnLedger::default(),
            context: ConversationContext::default(),
            model,
            output_active: false,
            transcription_only: false,
        }
    }

    /// Select a transcription-only session before `open()`.
    pub fn set_transcription_only(&mut self, enabled: bool) {
        self.transcription_only = enabled;
    }

    /// Seeds the session from a saved conversation. Call before `open()`.
    /// Restored turns are context only: never re-committed, never re-dispatched.
    pub fn restore(&mut self, history: &[(Role, String)]) {
        self.context.restore(history);
    }

    /// The opening handshake. Nothing else may be sent until the provider
    /// answers `session.started`.
    pub fn open(&mut self) -> Vec<SessionAction> {
        vec![SessionAction::Send(ClientEvent::SessionStart {
            model: self.model.clone(),
            instructions: if self.transcription_only {
                "Transcribe the user's speech only. Do not answer, speak, delegate tasks or take actions.".into()
            } else if self.context.has_restored() {
                // The voice model needs history too, not only the delegated
                // execution model. Context is data, never a request to repeat work.
                let history = self.context.render(&[]);
                let tail: String = history.chars().rev().take(24_000).collect::<String>().chars().rev().collect();
                format!("{VOICE_INSTRUCTIONS}\n\nSaved conversation context (historical data; do not execute old requests or read it aloud). Use it to understand follow-ups and what was already discussed:\n{}", serde_json::to_string(&tail).unwrap_or_default())
            } else { VOICE_INSTRUCTIONS.to_string() },
        })]
    }

    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready
    }

    pub fn has_restored_context(&self) -> bool {
        self.context.has_restored()
    }

    /// Handles one provider event. `now` is monotonic seconds; the gate is the
    /// only thing that reads it.
    pub fn handle(&mut self, event: ServerEvent, now: f64) -> Vec<SessionAction> {
        if self.phase == Phase::Ended {
            return Vec::new();
        }
        if self.transcription_only {
            match event {
                ServerEvent::InputTranscriptDelta { text } => return vec![SessionAction::Caption(Caption {
                    turn: None, role: Role::User, text,
                })],
                ServerEvent::SessionStarted | ServerEvent::SessionClosed | ServerEvent::Error { .. } => {},
                _ => return Vec::new(),
            }
        }
        match event {
            ServerEvent::SessionStarted => {
                self.phase = Phase::Ready;
                Vec::new()
            }
            ServerEvent::OutputAudioDelta { pcm } => {
                // A barge-in suppresses output for a moment, because audio the
                // server already sent is still arriving after the user speaks.
                if self.gate.suppresses_output(now) || pcm.is_empty() {
                    return Vec::new();
                }
                self.output_active = true;
                vec![SessionAction::Play {
                    generation: self.playback_generation,
                    pcm,
                }]
            }
            ServerEvent::InputTranscriptDelta { text } => self.on_user_speech(&text, now),
            ServerEvent::OutputTranscriptDelta { text } => {
                if text.trim().is_empty() {
                    return Vec::new();
                }
                self.ledger.push_spoken(&text);
                vec![SessionAction::Caption(Caption {
                    turn: self.ledger.open_turn(),
                    role: Role::Assistant,
                    text,
                })]
            }
            ServerEvent::DelegationCreated { id } => self.on_delegation(&id),
            // The provider's own response bookkeeping. The authoritative answer
            // is the agent's, delivered through `complete_delegation`, so these
            // only matter as a failure signal.
            ServerEvent::ResponseCompleted { .. } => Vec::new(),
            ServerEvent::ResponseFailed { message, .. } => {
                let turn = self.ledger.open_turn();
                let mut actions = Vec::new();
                if let Some(turn) = turn {
                    actions.extend(self.finish_turn_with(turn, &format!("Voice turn failed: {message}")));
                }
                actions
            }
            ServerEvent::SessionClosed => self.close("The voice session closed."),
            ServerEvent::Error { message } => self.close(&message),
            ServerEvent::Other => Vec::new(),
        }
    }

    fn on_user_speech(&mut self, text: &str, now: f64) -> Vec<SessionAction> {
        if text.trim().is_empty() {
            return Vec::new();
        }
        let mut actions = Vec::new();
        // A server-side transcript is authoritative evidence of speech, so it
        // confirms a barge-in that the energy heuristic may have only armed.
        if self.gate.confirm_speech(now) && self.output_active {
            actions.push(self.stop_playback());
        }
        let turn = self.ledger.push_user(text);
        actions.push(SessionAction::Caption(Caption {
            turn: Some(turn),
            role: Role::User,
            text: text.to_string(),
        }));
        actions
    }

    fn on_delegation(&mut self, id: &str) -> Vec<SessionAction> {
        // `open_delegation` returns None when this turn already went to the
        // agent, which is what keeps a repeated event from running it twice.
        let Some(turn) = self.ledger.open_delegation(id) else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        // The user's words commit at dispatch: that is the moment the turn is
        // real, and it is before any result can arrive to race with it.
        // `open_delegation` has already refused an empty turn, so this commit
        // succeeds and puts the request into `context` — which is why nothing
        // needs to be passed as pending below.
        let mut pending = Vec::new();
        match self.ledger.commit_user(turn) {
            Some(message) => {
                self.context.push(Role::User, &message.text);
                actions.push(SessionAction::Commit(message));
            }
            // Defensive: a turn that somehow dispatches twice must still carry
            // what was said rather than reaching the agent with empty context.
            None => pending.push((Role::User, self.ledger.open_fragments().0)),
        }
        actions.push(SessionAction::Dispatch(DelegationRequest {
            turn,
            epoch: self.epoch,
            delegation_id: id.to_string(),
            context: self.context.render(&pending),
            binding: self.binding.clone(),
        }));
        actions
    }

    /// The selected agent answered. Commits the authoritative assistant message
    /// and speaks it back. A result from a retired epoch is dropped: the user
    /// switched provider or reconnected, and that work no longer owns the turn.
    pub fn complete_delegation(&mut self, turn: u64, epoch: u64, answer: &str) -> Vec<SessionAction> {
        if epoch != self.epoch || self.phase == Phase::Ended {
            return Vec::new();
        }
        self.finish_turn_with(turn, answer)
    }

    /// The agent failed. The failure is still the turn's answer — it is what
    /// happened, the user asked for it out loud, and silence would read as
    /// success.
    pub fn fail_delegation(&mut self, turn: u64, epoch: u64, message: &str) -> Vec<SessionAction> {
        if epoch != self.epoch || self.phase == Phase::Ended {
            return Vec::new();
        }
        self.finish_turn_with(turn, &format!("The task could not finish. {message}"))
    }

    fn finish_turn_with(&mut self, turn: u64, answer: &str) -> Vec<SessionAction> {
        let Some(message) = self.ledger.commit_assistant(turn, answer) else {
            // Already answered: a duplicate completion, or a retry that raced
            // the first result. Neither may speak or persist a second time.
            return Vec::new();
        };
        self.context.push(Role::Assistant, &message.text);
        let delegation_id = self.ledger.delegation_of(turn).map(str::to_string);
        let mut actions = vec![SessionAction::Commit(message.clone())];
        actions.extend(
            commentary_chunks(&message.text)
                .into_iter()
                .map(|content| {
                    SessionAction::Send(ClientEvent::CommentaryAppend {
                        delegation_id: delegation_id.clone(),
                        content,
                    })
                }),
        );
        actions
    }

    /// Local microphone energy. Stops playback before the server's transcript
    /// arrives, which is most of the difference between barge-in that feels
    /// instant and barge-in that feels broken.
    pub fn microphone_level(&mut self, level: f64, output_level: f64, now: f64) -> Vec<SessionAction> {
        if self.phase == Phase::Ended {
            return Vec::new();
        }
        if self.gate.update(level, output_level, self.output_active, now) {
            return vec![self.stop_playback()];
        }
        Vec::new()
    }

    /// Typed input during a voice conversation. It is a user request like any
    /// other, so it enters the same turn ledger rather than a side channel.
    pub fn send_text(&mut self, text: &str) -> Vec<SessionAction> {
        if self.transcription_only || self.phase == Phase::Ended || text.trim().is_empty() {
            return Vec::new();
        }
        let turn = self.ledger.push_user(text);
        vec![
            SessionAction::Caption(Caption {
                turn: Some(turn),
                role: Role::User,
                text: text.to_string(),
            }),
            SessionAction::Send(ClientEvent::UserText {
                text: text.to_string(),
            }),
        ]
    }

    /// Stops playback and moves the fence forward so frames already in flight
    /// are dropped by the renderer rather than played after the user spoke.
    fn stop_playback(&mut self) -> SessionAction {
        self.playback_generation += 1;
        self.output_active = false;
        SessionAction::StopPlayback {
            generation: self.playback_generation,
        }
    }

    pub fn close(&mut self, reason: &str) -> Vec<SessionAction> {
        if self.phase == Phase::Ended {
            return Vec::new();
        }
        let mut actions = vec![self.stop_playback()];
        // Words that were really said belong in the conversation even when the
        // session died before they could be answered.
        if let Some(turn) = self.ledger.abandon_open() {
            if let Some(message) = self.ledger.commit_user(turn) {
                actions.push(SessionAction::Commit(message));
            }
        }
        self.phase = Phase::Ended;
        self.epoch += 1;
        actions.push(SessionAction::Ended {
            reason: reason.to_string(),
        });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> ExecutionBinding {
        ExecutionBinding {
            conversation_id: "chat-1".into(),
            agent: "claude".into(),
            project: "XNAUT".into(),
            provider: "nautgate".into(),
            model: "claude-opus-5".into(),
            permission: "project-write".into(),
        }
    }

    #[test]
    fn transcription_only_cannot_speak_commit_or_dispatch() {
        let mut session = LiveSession::new(binding(), "gpt-live-1".into());
        session.set_transcription_only(true);
        session.handle(ServerEvent::SessionStarted, 0.0);
        let actions = session.handle(ServerEvent::InputTranscriptDelta { text: "A draft.".into() }, 1.0);
        assert!(matches!(&actions[..], [SessionAction::Caption(Caption { role: Role::User, text, .. })] if text == "A draft."));
        for event in [
            ServerEvent::OutputAudioDelta { pcm: vec![1, 0, 2, 0] },
            ServerEvent::OutputTranscriptDelta { text: "An unwanted answer".into() },
            ServerEvent::DelegationCreated { id: "do-not-run".into() },
        ] { assert!(session.handle(event, 2.0).is_empty()); }
        assert!(session.send_text("Do not dispatch").is_empty());
        assert!(session.close("done").iter().all(|a| !matches!(a, SessionAction::Commit(_) | SessionAction::Dispatch(_) | SessionAction::Play { .. })));
    }

    fn ready() -> LiveSession {
        let mut session = LiveSession::new(binding(), "gpt-live-1".into());
        session.open();
        session.handle(ServerEvent::SessionStarted, 0.0);
        session
    }

    fn speak(session: &mut LiveSession, text: &str, now: f64) -> Vec<SessionAction> {
        session.handle(
            ServerEvent::InputTranscriptDelta { text: text.into() },
            now,
        )
    }

    fn commits(actions: &[SessionAction]) -> Vec<CommittedMessage> {
        actions
            .iter()
            .filter_map(|a| match a {
                SessionAction::Commit(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    /// The (turn, epoch) pair a dispatch hands the frontend to return with its
    /// result. Reading it here rather than from an accessor keeps the tests on
    /// the same path the app uses.
    fn ticket(actions: &[SessionAction]) -> (u64, u64) {
        let request = &dispatches(actions)[0];
        (request.turn, request.epoch)
    }

    fn dispatches(actions: &[SessionAction]) -> Vec<DelegationRequest> {
        actions
            .iter()
            .filter_map(|a| match a {
                SessionAction::Dispatch(d) => Some(d.clone()),
                _ => None,
            })
            .collect()
    }

    // --- acceptance: start once, hold several turns -------------------------

    #[test]
    fn one_start_carries_three_spoken_turns() {
        let mut session = ready();
        let mut all = Vec::new();
        for (n, (question, answer)) in [
            ("what is the build status?", "Build 412 passed."),
            ("and the tests?", "1338 passed, none failed."),
            ("thanks", "Any time."),
        ]
        .into_iter()
        .enumerate()
        {
            let now = n as f64 * 10.0;
            all.extend(speak(&mut session, question, now));
            let dispatched = session.handle(
                ServerEvent::DelegationCreated {
                    id: format!("d{n}"),
                },
                now,
            );
            let (turn, epoch) = ticket(&dispatched);
            all.extend(dispatched);
            all.extend(session.complete_delegation(turn, epoch, answer));
        }
        let committed = commits(&all);
        assert_eq!(
            committed.len(),
            6,
            "three turns, one user and one assistant message each: {committed:#?}"
        );
        assert_eq!(committed[0].text, "what is the build status?");
        assert_eq!(committed[1].text, "Build 412 passed.");
        assert_eq!(committed[4].text, "thanks");
        // Turn numbers advance without a restart.
        assert_eq!(
            committed.iter().map(|m| m.turn).collect::<Vec<_>>(),
            vec![0, 0, 1, 1, 2, 2]
        );
    }

    // --- acceptance: exactly-once persistence --------------------------------

    #[test]
    fn many_transcript_deltas_commit_one_user_message() {
        let mut session = ready();
        for fragment in ["re", "start ", "the ", "build"] {
            speak(&mut session, fragment, 1.0);
        }
        let actions = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let committed = commits(&actions);
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].text, "restart the build");
    }

    #[test]
    fn a_repeated_delegation_event_dispatches_once() {
        let mut session = ready();
        speak(&mut session, "restart the build", 1.0);
        let first = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let second = session.handle(ServerEvent::DelegationCreated { id: "d2".into() }, 1.1);
        assert_eq!(dispatches(&first).len(), 1);
        assert!(
            dispatches(&second).is_empty(),
            "the turn is already with the agent"
        );
        assert!(
            commits(&second).is_empty(),
            "and it must not commit the user message twice"
        );
    }

    #[test]
    fn a_duplicate_agent_result_neither_persists_nor_speaks_twice() {
        let mut session = ready();
        speak(&mut session, "status?", 1.0);
        let dispatched = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let (turn, epoch) = ticket(&dispatched);
        let first = session.complete_delegation(turn, epoch, "All green.");
        let second = session.complete_delegation(turn, epoch, "All green.");
        assert_eq!(commits(&first).len(), 1);
        assert!(second.is_empty(), "the retry produces nothing at all");
    }

    #[test]
    fn the_spoken_paraphrase_is_a_caption_not_a_message() {
        let mut session = ready();
        speak(&mut session, "status?", 1.0);
        let spoken = session.handle(
            ServerEvent::OutputTranscriptDelta {
                text: "Let me look that up for you.".into(),
            },
            1.1,
        );
        assert!(commits(&spoken).is_empty());
        assert!(matches!(spoken[0], SessionAction::Caption(_)));
    }

    // --- acceptance: streaming and interruption ------------------------------

    #[test]
    fn audio_plays_while_nothing_has_interrupted_it() {
        let mut session = ready();
        let actions = session.handle(ServerEvent::OutputAudioDelta { pcm: vec![1, 2] }, 1.0);
        assert_eq!(
            actions,
            vec![SessionAction::Play {
                generation: 0,
                pcm: vec![1, 2]
            }]
        );
    }

    #[test]
    fn speaking_over_the_answer_stops_it_and_fences_the_audio_behind_it() {
        let mut session = ready();
        session.handle(ServerEvent::OutputAudioDelta { pcm: vec![1] }, 1.0);
        let interrupted = speak(&mut session, "actually, wait", 1.1);
        assert!(
            interrupted.contains(&SessionAction::StopPlayback { generation: 1 }),
            "the barge-in must stop playback: {interrupted:#?}"
        );
        // Frames the server already sent are dropped for the suppression window.
        let stale = session.handle(ServerEvent::OutputAudioDelta { pcm: vec![2] }, 1.15);
        assert!(stale.is_empty(), "stale audio must not reach the renderer");
        // And what plays afterwards carries the new generation, so a renderer
        // holding an old frame can tell the difference.
        let fresh = session.handle(ServerEvent::OutputAudioDelta { pcm: vec![3] }, 2.0);
        assert_eq!(
            fresh,
            vec![SessionAction::Play {
                generation: 1,
                pcm: vec![3]
            }]
        );
    }

    #[test]
    fn microphone_energy_stops_playback_before_the_transcript_arrives() {
        let mut session = ready();
        session.handle(ServerEvent::OutputAudioDelta { pcm: vec![1] }, 1.0);
        assert!(session.microphone_level(0.9, 0.01, 1.0).is_empty(), "arms");
        let stopped = session.microphone_level(0.9, 0.01, 1.2);
        assert_eq!(stopped, vec![SessionAction::StopPlayback { generation: 1 }]);
    }

    #[test]
    fn energy_does_not_interrupt_when_nothing_is_playing() {
        let mut session = ready();
        for step in 0..10 {
            assert!(session
                .microphone_level(1.0, 0.0, 1.0 + step as f64 * 0.1)
                .is_empty());
        }
    }

    #[test]
    fn interruption_does_not_cancel_the_agents_work() {
        let mut session = ready();
        speak(&mut session, "run the suite", 1.0);
        let dispatched = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let (turn, epoch) = ticket(&dispatched);
        session.handle(ServerEvent::OutputAudioDelta { pcm: vec![1] }, 1.1);
        speak(&mut session, "hang on", 1.2);
        // The task keeps running and its answer still lands.
        let done = session.complete_delegation(turn, epoch, "1338 passed.");
        assert_eq!(commits(&done).len(), 1);
    }

    // --- acceptance: execution context ---------------------------------------

    #[test]
    fn every_dispatch_carries_the_selected_agent_project_and_permissions() {
        let mut session = ready();
        speak(&mut session, "do the thing", 1.0);
        let actions = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let request = &dispatches(&actions)[0];
        assert_eq!(request.binding, binding());
        assert_eq!(request.binding.permission, "project-write");
        assert_eq!(request.delegation_id, "d1");
    }

    #[test]
    fn the_dispatch_context_contains_what_was_actually_said() {
        let mut session = ready();
        speak(&mut session, "deploy the staging build", 1.0);
        let actions = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        assert!(dispatches(&actions)[0]
            .context
            .contains("deploy the staging build"));
    }

    #[test]
    fn a_later_turn_can_refer_to_an_earlier_answer() {
        let mut session = ready();
        speak(&mut session, "what port does the bridge use?", 1.0);
        let first = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let (turn, epoch) = ticket(&first);
        session.complete_delegation(turn, epoch, "Port 51737.");
        speak(&mut session, "and is it open?", 2.0);
        let second = session.handle(ServerEvent::DelegationCreated { id: "d2".into() }, 2.0);
        let context = &dispatches(&second)[0].context;
        assert!(context.contains("Port 51737"), "context was {context}");
        assert!(context.contains("what port does the bridge use?"));
    }

    // --- acceptance: reopen with context -------------------------------------

    #[test]
    fn a_reopened_conversation_dispatches_with_its_saved_context() {
        let mut session = LiveSession::new(binding(), "gpt-live-1".into());
        session.restore(&[
            (Role::User, "My deploy key is named tron-ci.".into()),
            (Role::Assistant, "Noted, tron-ci.".into()),
        ]);
        session.open();
        session.handle(ServerEvent::SessionStarted, 0.0);
        assert!(session.has_restored_context());

        speak(&mut session, "what was my deploy key called?", 1.0);
        let actions = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let request = &dispatches(&actions)[0];
        assert!(
            request.context.contains("tron-ci"),
            "the reopened session must carry the earlier fact: {}",
            request.context
        );
        assert_eq!(request.turn, 0, "restored turns are context, not new turns");
    }

    #[test]
    fn reopening_does_not_re_commit_or_re_dispatch_completed_work() {
        let mut session = LiveSession::new(binding(), "gpt-live-1".into());
        session.restore(&[
            (Role::User, "restart the build".into()),
            (Role::Assistant, "Restarted.".into()),
        ]);
        let opening = session.open();
        assert!(matches!(&opening[0], SessionAction::Send(ClientEvent::SessionStart { instructions, .. }) if instructions.contains("restart the build") && instructions.contains("historical data")));
        let started = session.handle(ServerEvent::SessionStarted, 0.0);
        assert!(commits(&opening).is_empty() && commits(&started).is_empty());
        assert!(dispatches(&opening).is_empty() && dispatches(&started).is_empty());
    }

    // --- lifecycle -----------------------------------------------------------

    #[test]
    fn nothing_is_dispatched_before_the_provider_accepts_the_session() {
        let mut session = LiveSession::new(binding(), "gpt-live-1".into());
        let opening = session.open();
        assert!(!session.is_ready());
        assert!(matches!(
            opening.as_slice(),
            [SessionAction::Send(ClientEvent::SessionStart { .. })]
        ));
    }

    #[test]
    fn the_answer_is_spoken_back_in_commentary_chunks() {
        let mut session = ready();
        speak(&mut session, "explain it", 1.0);
        let dispatched = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let (turn, epoch) = ticket(&dispatched);
        let long = "word ".repeat(300);
        let actions = session.complete_delegation(turn, epoch, &long);
        let spoken: Vec<&ClientEvent> = actions
            .iter()
            .filter_map(|a| match a {
                SessionAction::Send(event) => Some(event),
                _ => None,
            })
            .collect();
        assert!(spoken.len() > 1, "a long answer is chunked");
        assert!(spoken.iter().all(|event| matches!(
            event,
            ClientEvent::CommentaryAppend { delegation_id: Some(id), .. } if id == "d1"
        )));
    }

    #[test]
    fn a_result_from_a_closed_connection_is_dropped() {
        let mut session = ready();
        speak(&mut session, "long job", 1.0);
        let dispatched = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let (turn, stale_epoch) = ticket(&dispatched);
        session.close("the socket dropped");
        assert!(
            session
                .complete_delegation(turn, stale_epoch, "answer from the old connection")
                .is_empty(),
            "work from a closed connection must not speak or persist"
        );
    }

    #[test]
    fn reopening_after_a_drop_carries_the_history_and_not_the_abandoned_turn() {
        // A dropped conversation is reopened, not re-socketed: the saved
        // history is what restores context, and the unanswered turn does not
        // come back as work to redo.
        let mut first = LiveSession::new(binding(), "gpt-live-1".into());
        first.restore(&[(Role::User, "remember tron-ci".into())]);
        first.open();
        first.handle(ServerEvent::SessionStarted, 0.0);
        speak(&mut first, "something", 1.0);
        first.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        first.close("the socket dropped");

        let mut second = LiveSession::new(binding(), "gpt-live-1".into());
        second.restore(&[(Role::User, "remember tron-ci".into())]);
        second.open();
        second.handle(ServerEvent::SessionStarted, 0.0);
        speak(&mut second, "and now?", 1.0);
        let actions = second.handle(ServerEvent::DelegationCreated { id: "d2".into() }, 1.0);
        let context = &dispatches(&actions)[0].context;
        assert!(context.contains("tron-ci"), "history survives the reopen");
        assert!(
            !context.contains("something"),
            "the turn the drop interrupted is not replayed as work"
        );
    }

    #[test]
    fn a_failed_agent_run_answers_the_turn_rather_than_going_silent() {
        let mut session = ready();
        speak(&mut session, "deploy", 1.0);
        let dispatched = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        let (turn, epoch) = ticket(&dispatched);
        let actions = session.fail_delegation(turn, epoch, "the sandbox is offline");
        let committed = commits(&actions);
        assert_eq!(committed.len(), 1);
        assert!(committed[0].text.contains("the sandbox is offline"));
    }

    #[test]
    fn closing_commits_what_was_said_but_never_answered() {
        let mut session = ready();
        speak(&mut session, "are you there", 1.0);
        let actions = session.close("the socket dropped");
        let committed = commits(&actions);
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].text, "are you there");
        assert!(actions.iter().any(|a| matches!(a, SessionAction::Ended { .. })));
    }

    #[test]
    fn a_closed_session_ignores_everything_that_arrives_late() {
        let mut session = ready();
        session.close("done");
        assert!(session
            .handle(ServerEvent::OutputAudioDelta { pcm: vec![1] }, 2.0)
            .is_empty());
        assert!(speak(&mut session, "hello?", 2.0).is_empty());
        assert!(session.close("again").is_empty());
        assert!(session.microphone_level(1.0, 0.0, 2.0).is_empty());
        assert!(session.send_text("hello?").is_empty());
    }

    #[test]
    fn a_provider_error_ends_the_session_with_its_reason() {
        let mut session = ready();
        let actions = session.handle(
            ServerEvent::Error {
                message: "rate limited".into(),
            },
            1.0,
        );
        assert!(actions.contains(&SessionAction::Ended {
            reason: "rate limited".into()
        }));
    }

    #[test]
    fn typed_input_joins_the_same_conversation_turn() {
        let mut session = ready();
        let actions = session.send_text("check the logs");
        assert!(actions.iter().any(|a| matches!(
            a,
            SessionAction::Send(ClientEvent::UserText { text }) if text == "check the logs"
        )));
        // And it commits through the ordinary turn path.
        let dispatched = session.handle(ServerEvent::DelegationCreated { id: "d1".into() }, 1.0);
        assert_eq!(commits(&dispatched)[0].text, "check the logs");
    }

    #[test]
    fn silence_and_empty_frames_produce_nothing() {
        let mut session = ready();
        assert!(speak(&mut session, "   ", 1.0).is_empty());
        assert!(session
            .handle(ServerEvent::OutputAudioDelta { pcm: vec![] }, 1.0)
            .is_empty());
        assert!(session
            .handle(
                ServerEvent::OutputTranscriptDelta { text: " ".into() },
                1.0
            )
            .is_empty());
        assert!(session.handle(ServerEvent::Other, 1.0).is_empty());
    }
}
