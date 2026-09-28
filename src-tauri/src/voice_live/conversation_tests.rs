//! End-to-end evidence for the XNAUT-416 V1 acceptance list.
//!
//! These drive a whole conversation through the real state machine with
//! synthetic audio and a scripted provider, which is what the ticket asks for:
//! "automated multi-turn conversation and synthetic-audio evidence" for
//! continuous start, streamed replies, interruption, exact-once persistence,
//! reopen/context continuation, and the selected agent/project/model/
//! permissions. The unit tests beside each module cover the pieces; this file
//! is about the sequence.
//!
//! What these deliberately do NOT establish, because no component test can:
//! real microphone capture, audible playback, speaker echo, OS permission
//! prompts, and live-account behaviour. Those are listed as manual checks in
//! `.xnaut/bundles/XNAUT-416.md`.

use super::protocol::{ClientEvent, ServerEvent};
use super::session::{DelegationRequest, ExecutionBinding, LiveSession, SessionAction};
use super::turn::{CommittedMessage, Role};

/// A synthetic 24 kHz PCM16 frame — a sine burst, so an assertion about
/// "audio reached the renderer" is about real sample data rather than a token.
fn synthetic_pcm(ms: usize, hz: f64) -> Vec<u8> {
    let samples = super::AUDIO_SAMPLE_RATE as usize * ms / 1000;
    (0..samples)
        .flat_map(|n| {
            let t = n as f64 / super::AUDIO_SAMPLE_RATE as f64;
            let value = (t * hz * std::f64::consts::TAU).sin() * 8_000.0;
            (value as i16).to_le_bytes()
        })
        .collect()
}

fn binding() -> ExecutionBinding {
    ExecutionBinding {
        conversation_id: "chat-voice-1".into(),
        agent: "claude".into(),
        project: "XNAUT".into(),
        provider: "nautgate".into(),
        model: "claude-opus-5".into(),
        permission: "project-write".into(),
    }
}

/// A conversation under test: the session plus everything it produced, so an
/// assertion can look at the whole run rather than one call's return value.
struct Conversation {
    session: LiveSession,
    committed: Vec<CommittedMessage>,
    dispatched: Vec<DelegationRequest>,
    sent: Vec<ClientEvent>,
    played: Vec<(u64, Vec<u8>)>,
    stops: Vec<u64>,
    ended: Option<String>,
    clock: f64,
}

impl Conversation {
    fn open(history: &[(Role, String)]) -> Self {
        let mut session = LiveSession::new(binding(), "gpt-live-1".into());
        if !history.is_empty() {
            session.restore(history);
        }
        let opening = session.open();
        let mut conversation = Self {
            session,
            committed: Vec::new(),
            dispatched: Vec::new(),
            sent: Vec::new(),
            played: Vec::new(),
            stops: Vec::new(),
            ended: None,
            clock: 0.0,
        };
        conversation.absorb(opening);
        conversation.feed(ServerEvent::SessionStarted);
        conversation
    }

    fn absorb(&mut self, actions: Vec<SessionAction>) {
        for action in actions {
            match action {
                SessionAction::Commit(message) => self.committed.push(message),
                SessionAction::Dispatch(request) => self.dispatched.push(request),
                SessionAction::Send(event) => self.sent.push(event),
                SessionAction::Play { generation, pcm } => self.played.push((generation, pcm)),
                SessionAction::StopPlayback { generation } => self.stops.push(generation),
                SessionAction::Ended { reason } => self.ended = Some(reason),
                SessionAction::Caption(_) => {}
            }
        }
    }

    /// Advances a simulated clock, so timing-dependent behaviour (the barge-in
    /// suppression window) is exercised without any real waiting.
    fn tick(&mut self, seconds: f64) -> f64 {
        self.clock += seconds;
        self.clock
    }

    fn feed(&mut self, event: ServerEvent) {
        let now = self.tick(0.05);
        let actions = self.session.handle(event, now);
        self.absorb(actions);
    }

    /// Speaks an utterance the way the provider delivers one: several fragments.
    fn say(&mut self, utterance: &str) {
        for word in utterance.split_inclusive(' ') {
            self.feed(ServerEvent::InputTranscriptDelta {
                text: word.to_string(),
            });
        }
    }

    /// The provider decides the turn needs the backend.
    fn delegate(&mut self, id: &str) {
        self.feed(ServerEvent::DelegationCreated { id: id.to_string() });
    }

    /// The selected xNAUT agent answers the most recent dispatched turn. The
    /// clock moves because the agent took time to do the work, and that pause
    /// is load-bearing: audio arriving inside the barge-in suppression window
    /// is *supposed* to be dropped, so a harness with no pause would test the
    /// suppression instead of the playback.
    fn agent_answers(&mut self, answer: &str) {
        self.tick(0.5);
        let request = self.dispatched.last().expect("a turn was dispatched");
        let (turn, epoch) = (request.turn, request.epoch);
        let actions = self.session.complete_delegation(turn, epoch, answer);
        self.absorb(actions);
    }

    /// The assistant speaks: audio frames plus the matching output transcript.
    /// Begins clear of the suppression window, as it does in life — nobody
    /// starts answering in the same quarter-second the question ended.
    fn assistant_speaks(&mut self, caption: &str, frames: usize) {
        self.tick(0.4);
        self.feed(ServerEvent::OutputTranscriptDelta {
            text: caption.to_string(),
        });
        for _ in 0..frames {
            self.feed(ServerEvent::OutputAudioDelta {
                pcm: synthetic_pcm(40, 220.0),
            });
        }
    }

    fn user_messages(&self) -> Vec<&str> {
        self.role_messages(Role::User)
    }
    fn assistant_messages(&self) -> Vec<&str> {
        self.role_messages(Role::Assistant)
    }
    fn role_messages(&self, role: Role) -> Vec<&str> {
        self.committed
            .iter()
            .filter(|m| m.role == role)
            .map(|m| m.text.as_str())
            .collect()
    }
    fn audio_bytes(&self) -> usize {
        self.played.iter().map(|(_, pcm)| pcm.len()).sum()
    }
}

// --------------------------------------------------------------------------
// Start once, hold many turns.
// --------------------------------------------------------------------------

#[test]
fn one_start_carries_a_five_turn_spoken_conversation() {
    let mut chat = Conversation::open(&[]);
    let script = [
        ("what is the build status?", "Build 412 passed."),
        ("and the test suite?", "1338 passed, none failed."),
        ("any warnings?", "Two, both in vendored code."),
        ("open a ticket for them", "Opened XNAUT-450."),
        ("thanks", "Any time."),
    ];
    for (n, (question, answer)) in script.iter().enumerate() {
        chat.say(question);
        chat.delegate(&format!("d{n}"));
        chat.agent_answers(answer);
        chat.assistant_speaks(answer, 3);
    }

    // No restart anywhere in that: one open, five exchanges.
    assert_eq!(chat.user_messages(), script.map(|(q, _)| q).to_vec());
    assert_eq!(chat.assistant_messages(), script.map(|(_, a)| a).to_vec());
    assert_eq!(
        chat.committed.len(),
        10,
        "five turns, two messages each, no more"
    );
    assert!(chat.ended.is_none(), "the session stayed open throughout");
    assert!(
        chat.audio_bytes() > 0,
        "synthetic reply audio reached the renderer"
    );
}

#[test]
fn turn_numbers_stay_paired_across_the_whole_conversation() {
    let mut chat = Conversation::open(&[]);
    for n in 0..4 {
        chat.say(&format!("question {n}"));
        chat.delegate(&format!("d{n}"));
        chat.agent_answers(&format!("answer {n}"));
    }
    for turn in 0..4u64 {
        let pair: Vec<&CommittedMessage> =
            chat.committed.iter().filter(|m| m.turn == turn).collect();
        assert_eq!(pair.len(), 2, "turn {turn} has exactly one pair");
        assert_eq!(pair[0].role, Role::User);
        assert_eq!(pair[1].role, Role::Assistant);
    }
}

// --------------------------------------------------------------------------
// Exactly-once persistence under duplicate and retried events.
// --------------------------------------------------------------------------

#[test]
fn a_noisy_provider_still_produces_one_message_pair_per_turn() {
    let mut chat = Conversation::open(&[]);
    chat.say("restart the staging build");
    // The provider repeats itself: duplicate delegations and duplicate
    // completions are both things a real transport does on retry.
    chat.delegate("d1");
    chat.delegate("d1");
    chat.delegate("d2");
    chat.agent_answers("Restarted staging.");
    chat.agent_answers("Restarted staging.");
    chat.feed(ServerEvent::ResponseCompleted {
        delegation_id: "d1".into(),
    });

    assert_eq!(chat.user_messages(), vec!["restart the staging build"]);
    assert_eq!(chat.assistant_messages(), vec!["Restarted staging."]);
    assert_eq!(
        chat.dispatched.len(),
        1,
        "the agent ran once, not three times"
    );
}

#[test]
fn a_long_utterance_in_many_fragments_is_one_message() {
    let mut chat = Conversation::open(&[]);
    let utterance = "could you check whether the release workflow pushed to forgejo \
                     before it tagged the github mirror";
    chat.say(utterance);
    chat.delegate("d1");
    assert_eq!(chat.user_messages(), vec![utterance]);
}

#[test]
fn the_voice_models_paraphrase_never_becomes_a_second_assistant_message() {
    let mut chat = Conversation::open(&[]);
    chat.say("what did the gate say?");
    chat.delegate("d1");
    // Live paraphrases while the backend works, then again afterwards.
    chat.feed(ServerEvent::OutputTranscriptDelta {
        text: "Let me check the gate for you.".into(),
    });
    chat.agent_answers("The gate refused: two checks are missing.");
    chat.feed(ServerEvent::OutputTranscriptDelta {
        text: "So the gate said no, two checks missing.".into(),
    });
    assert_eq!(
        chat.assistant_messages(),
        vec!["The gate refused: two checks are missing."],
        "the conversation records the agent's answer, not the paraphrase"
    );
}

// --------------------------------------------------------------------------
// Streaming replies and interruption.
// --------------------------------------------------------------------------

#[test]
fn talking_over_a_reply_stops_it_and_fences_out_the_audio_behind_it() {
    let mut chat = Conversation::open(&[]);
    chat.say("explain the release process");
    chat.delegate("d1");
    chat.agent_answers("First the version files, then Forgejo, then the tag.");
    chat.assistant_speaks("First the version files", 5);
    let before = chat.audio_bytes();
    assert!(before > 0, "the reply was streaming");

    // The user cuts in.
    chat.say("skip ahead");
    assert_eq!(chat.stops.len(), 1, "one barge-in raises one stop");
    let fence = chat.stops[0];

    // Frames the provider had already sent keep arriving and must be dropped.
    let during = chat.audio_bytes();
    for _ in 0..4 {
        chat.feed(ServerEvent::OutputAudioDelta {
            pcm: synthetic_pcm(40, 220.0),
        });
    }
    assert_eq!(
        chat.audio_bytes(),
        during,
        "stale audio must not reach the renderer after a barge-in"
    );

    // Once the window passes, the next reply plays under a new fence.
    chat.tick(1.0);
    chat.feed(ServerEvent::OutputAudioDelta {
        pcm: synthetic_pcm(40, 440.0),
    });
    assert!(chat.audio_bytes() > during, "the new reply plays");
    assert_eq!(
        chat.played.last().unwrap().0,
        fence,
        "and carries the generation raised by the interruption"
    );
}

#[test]
fn an_interruption_does_not_cancel_the_running_agent_turn() {
    let mut chat = Conversation::open(&[]);
    chat.say("run the whole suite");
    chat.delegate("d1");
    chat.assistant_speaks("Starting the suite", 3);
    chat.say("actually also check clippy");
    assert_eq!(chat.stops.len(), 1, "playback stopped");
    // The original run still lands: interruption is not cancellation.
    chat.agent_answers("1338 passed; clippy clean.");
    assert_eq!(chat.assistant_messages(), vec!["1338 passed; clippy clean."]);
}

#[test]
fn interrupting_twice_in_one_breath_raises_one_stop() {
    let mut chat = Conversation::open(&[]);
    chat.say("talk to me");
    chat.delegate("d1");
    chat.agent_answers("Talking.");
    chat.assistant_speaks("Talking", 3);
    // Several fragments of one interjection, all inside the window.
    chat.say("no wait stop");
    assert_eq!(
        chat.stops.len(),
        1,
        "one stretch of speech is one interruption"
    );
}

// --------------------------------------------------------------------------
// Reopen with context.
// --------------------------------------------------------------------------

#[test]
fn a_reopened_conversation_answers_from_its_saved_context() {
    // Session one.
    let mut first = Conversation::open(&[]);
    first.say("my deploy key is named tron-ci");
    first.delegate("d1");
    first.agent_answers("Noted: tron-ci.");
    first.session.close("the user ended the conversation");

    // What the conversation persisted is what a reopen gets back.
    let saved: Vec<(Role, String)> = first
        .committed
        .iter()
        .map(|m| (m.role, m.text.clone()))
        .collect();
    assert_eq!(saved.len(), 2);

    // Session two, later.
    let mut second = Conversation::open(&saved);
    assert!(second.session.has_restored_context());
    second.say("what was my deploy key called?");
    second.delegate("d2");

    let request = second.dispatched.last().expect("dispatched");
    assert!(
        request.context.contains("tron-ci"),
        "the reopened session must carry the earlier fact: {}",
        request.context
    );
    // And the restored turns are context, not work to redo.
    assert!(
        second.user_messages() == vec!["what was my deploy key called?"],
        "reopening must not re-commit the old conversation"
    );
    assert_eq!(
        second.dispatched.len(),
        1,
        "reopening must not re-run completed work"
    );
}

#[test]
fn reopening_an_empty_conversation_is_an_ordinary_start() {
    let chat = Conversation::open(&[]);
    assert!(!chat.session.has_restored_context());
    assert!(chat.committed.is_empty());
    assert!(chat.dispatched.is_empty());
}

// --------------------------------------------------------------------------
// Execution context.
// --------------------------------------------------------------------------

#[test]
fn every_turn_dispatches_to_the_selected_agent_project_and_permissions() {
    let mut chat = Conversation::open(&[]);
    for n in 0..3 {
        chat.say(&format!("task {n}"));
        chat.delegate(&format!("d{n}"));
        chat.agent_answers("done");
    }
    assert_eq!(chat.dispatched.len(), 3);
    for request in &chat.dispatched {
        assert_eq!(request.binding, binding());
        assert_eq!(request.binding.agent, "claude");
        assert_eq!(request.binding.project, "XNAUT");
        assert_eq!(request.binding.model, "claude-opus-5");
        assert_eq!(request.binding.permission, "project-write");
        assert_eq!(request.binding.conversation_id, "chat-voice-1");
    }
}

#[test]
fn the_session_never_asks_the_voice_provider_to_do_the_work() {
    let mut chat = Conversation::open(&[]);
    chat.say("delete the staging database");
    chat.delegate("d1");
    let start = chat
        .sent
        .iter()
        .find_map(|event| match event {
            ClientEvent::SessionStart { .. } => Some(event.to_json()),
            _ => None,
        })
        .expect("the session opened");
    // Client delegation, no provider-side tools, no backend model: the voice
    // provider speaks and xNAUT decides.
    assert_eq!(start["session"]["delegation"]["type"], "client");
    assert!(start["session"].get("tools").is_none());
    assert_eq!(start["session"]["store"], false);
}

#[test]
fn the_answer_the_user_hears_is_the_answer_that_was_recorded() {
    let mut chat = Conversation::open(&[]);
    chat.say("what is the version?");
    chat.delegate("d1");
    chat.agent_answers("Version 1.27.2.");
    let spoken: String = chat
        .sent
        .iter()
        .filter_map(|event| match event {
            ClientEvent::CommentaryAppend { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(spoken, "Version 1.27.2.");
    assert_eq!(chat.assistant_messages(), vec!["Version 1.27.2."]);
}

// --------------------------------------------------------------------------
// Failure and lifecycle.
// --------------------------------------------------------------------------

#[test]
fn a_failed_agent_turn_is_answered_rather_than_silently_dropped() {
    let mut chat = Conversation::open(&[]);
    chat.say("deploy to production");
    chat.delegate("d1");
    let request = chat.dispatched.last().unwrap().clone();
    let actions = chat
        .session
        .fail_delegation(request.turn, request.epoch, "the sandbox is offline");
    chat.absorb(actions);
    assert_eq!(chat.assistant_messages().len(), 1);
    assert!(chat.assistant_messages()[0].contains("the sandbox is offline"));
    // The conversation continues afterwards.
    chat.say("try again");
    chat.delegate("d2");
    chat.agent_answers("Deployed.");
    assert_eq!(chat.assistant_messages().len(), 2);
}

#[test]
fn a_dropped_connection_keeps_what_was_said_and_reports_why() {
    let mut chat = Conversation::open(&[]);
    chat.say("are you still there");
    chat.feed(ServerEvent::Error {
        message: "rate limited".into(),
    });
    assert_eq!(
        chat.user_messages(),
        vec!["are you still there"],
        "words really spoken belong in the conversation"
    );
    assert_eq!(chat.ended.as_deref(), Some("rate limited"));
}

#[test]
fn a_dropped_conversation_is_resumed_by_reopening_it_with_its_history() {
    // The reconnect story is reopen-with-history, not a re-socket: it restores
    // the context, and it is the same path a user takes days later.
    let mut first = Conversation::open(&[(Role::User, "remember tron-ci".into())]);
    first.say("first question");
    first.delegate("d1");
    let stale = first.dispatched.last().unwrap().clone();
    first.session.close("the socket dropped");

    // The old run's answer is no longer welcome anywhere.
    let late = first
        .session
        .complete_delegation(stale.turn, stale.epoch, "answer from the dead connection");
    assert!(late.is_empty(), "a closed session cannot speak");

    // What the reopen gets back is the stored conversation: what was already
    // there, plus what this session committed. The unanswered question is in
    // it — it was really asked — and its answer is not, because there was none.
    let earlier = vec![(Role::User, "remember tron-ci".to_string())];
    let saved: Vec<(Role, String)> = earlier
        .into_iter()
        .chain(first.committed.iter().map(|m| (m.role, m.text.clone())))
        .collect();
    assert_eq!(
        saved.len(),
        2,
        "the earlier turn plus one committed user message, no answer"
    );

    let mut second = Conversation::open(&saved);
    second.say("and now?");
    second.delegate("d2");
    let context = &second.dispatched.last().unwrap().context;
    assert!(context.contains("tron-ci"), "history survives: {context}");
    assert!(
        !context.contains("answer from the dead connection"),
        "the retired connection's work does not"
    );
    assert_eq!(
        second.dispatched.len(),
        1,
        "reopening runs the new turn only"
    );
}

#[test]
fn a_synthetic_frame_is_the_pcm_the_renderer_receives() {
    let mut chat = Conversation::open(&[]);
    chat.say("say something");
    chat.delegate("d1");
    chat.agent_answers("Something.");
    let frame = synthetic_pcm(40, 440.0);
    chat.feed(ServerEvent::OutputAudioDelta { pcm: frame.clone() });
    let (_, played) = chat.played.last().expect("a frame reached the renderer");
    assert_eq!(played, &frame, "audio passes through unmodified");
    assert_eq!(frame.len(), 960 * 2, "40 ms of 24 kHz PCM16");
}
