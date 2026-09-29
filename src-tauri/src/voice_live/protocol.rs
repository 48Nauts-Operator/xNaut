// The public Live wire contract, traced from Bucki (48Nauts/Bucky, development
// @ 629ff06): `Services/GPTLiveSession.swift` for the session envelope and
// `Services/OpenAIRealtimeRunner.swift::handleLiveEvent` for the event set that
// the shipped public route actually handles.
//
// Live is NOT Realtime. Bucki carries both; only this one is the persisted
// public selection (`publicVoiceMode=gptLive`). The difference that matters
// here: Live streams output continuously and has no completed-transcript event
// and no `response.cancel`, so turn boundaries and barge-in are ours to decide.
//
// Departure from the source: Bucki hardcodes its own persona, its `marin`
// voice, and a `delegation.responses` block naming an OpenAI backend model.
// xNAUT sends `delegation: {"type": "client"}` unconditionally, because the
// work belongs to the selected xNAUT agent over its own NautGate route — see
// `session.rs`. We never ask the voice provider to reason or to hold tools.

use serde::{Deserialize, Serialize};

/// Endpoint and models observed in the reference snapshot. These are findings
/// from that source, not an availability claim; `voice_live_open` takes them
/// from settings so a deployment can point elsewhere without a rebuild.
pub const DEFAULT_LIVE_URL: &str = "wss://api.openai.com/v1/live/sessions";
pub const DEFAULT_VOICE_MODEL: &str = "gpt-live-1";

/// PCM16 mono at the rate Live negotiates. Bucki pins 24 kHz; we advertise the
/// same and re-sample at the renderer rather than assuming the device matches.
pub const AUDIO_SAMPLE_RATE: u32 = 24_000;

/// Live rejects a commentary append much beyond 500 tokens. Bucki chunks at 400
/// characters, which is conservative for every script we care about.
pub const COMMENTARY_CHUNK: usize = 400;

/// What the server sends us. Unknown types decode to `Other` and are ignored
/// rather than failing the session: the transport is versioned upstream and a
/// new event must not take down a live conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    /// The session was accepted. Nothing may be sent before this.
    SessionStarted,
    /// A chunk of assistant audio, already base64-decoded.
    OutputAudioDelta { pcm: Vec<u8> },
    /// A fragment of the user's speech, recognised server-side.
    InputTranscriptDelta { text: String },
    /// A fragment of what the voice model is saying.
    OutputTranscriptDelta { text: String },
    /// The voice model decided this turn needs the backend. Carries metadata
    /// only — never the task text, which is why we keep our own context.
    DelegationCreated { id: String },
    /// A delegated response finished on the provider's side.
    ResponseCompleted { delegation_id: String },
    ResponseFailed {
        delegation_id: String,
        message: String,
    },
    SessionClosed,
    Error { message: String },
    Other,
}

impl ServerEvent {
    /// Decodes one server frame. Returns `Other` for anything unrecognised.
    pub fn decode(value: &serde_json::Value) -> Self {
        let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "session.started" => ServerEvent::SessionStarted,
            "session.output_audio.delta" => value
                .get("delta")
                .and_then(|v| v.as_str())
                .and_then(decode_base64)
                .map_or(ServerEvent::Other, |pcm| ServerEvent::OutputAudioDelta {
                    pcm,
                }),
            "session.input_transcript.delta" => ServerEvent::InputTranscriptDelta {
                text: delta_text(value),
            },
            "session.output_transcript.delta" => ServerEvent::OutputTranscriptDelta {
                text: delta_text(value),
            },
            "session.delegation.created" => value
                .get("delegation")
                .and_then(|d| d.get("id"))
                .and_then(|v| v.as_str())
                .filter(|id| !id.is_empty())
                .map_or(ServerEvent::Other, |id| ServerEvent::DelegationCreated {
                    id: id.to_string(),
                }),
            // Delegated responses arrive wrapped, with the real type nested.
            "response.event" => decode_response_event(value),
            "session.closed" => ServerEvent::SessionClosed,
            "error" => ServerEvent::Error {
                message: value
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Live API error")
                    .to_string(),
            },
            _ => ServerEvent::Other,
        }
    }
}

fn delta_text(value: &serde_json::Value) -> String {
    value
        .get("delta")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn decode_response_event(value: &serde_json::Value) -> ServerEvent {
    let Some(delegation_id) = value
        .get("delegation_id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
    else {
        return ServerEvent::Other;
    };
    let Some(nested) = value.get("event") else {
        return ServerEvent::Other;
    };
    match nested.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "response.completed" => ServerEvent::ResponseCompleted {
            delegation_id: delegation_id.to_string(),
        },
        kind @ ("response.failed" | "response.incomplete" | "response.cancelled") => {
            ServerEvent::ResponseFailed {
                delegation_id: delegation_id.to_string(),
                message: nested
                    .get("response")
                    .and_then(|r| r.get("error"))
                    .and_then(|e| e.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(kind)
                    .to_string(),
            }
        }
        _ => ServerEvent::Other,
    }
}

fn decode_base64(encoded: &str) -> Option<Vec<u8>> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.decode(encoded).ok()
}

/// What we send. Serialised at the edge so the state machine can be compared
/// against expected values in tests without touching JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientEvent {
    /// Opens the session. Instructions describe how to *speak*, never how to
    /// do the work; the work goes to the xNAUT agent.
    SessionStart {
        model: String,
        instructions: String,
    },
    /// A verified backend result, spoken back to the user by the voice model.
    CommentaryAppend {
        delegation_id: Option<String>,
        content: String,
    },
    /// A typed message, so a voice conversation accepts keyboard input without
    /// leaving the session.
    UserText { text: String },
    /// A buffer of microphone PCM16. Live names this differently from Realtime
    /// (`input_audio_buffer.append`), which is one of the places the two
    /// protocols are not interchangeable.
    InputAudioAppend { pcm: Vec<u8> },
}

impl ClientEvent {
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ClientEvent::SessionStart {
                model,
                instructions,
            } => serde_json::json!({
                "type": "session.start",
                "session": {
                    "model": model,
                    // No provider-side retention: xNAUT's conversation is the record.
                    "store": false,
                    "instructions": instructions,
                    "audio": {
                        "format": { "type": "audio/pcm", "rate": AUDIO_SAMPLE_RATE },
                    },
                    // Client delegation: the voice model speaks, xNAUT decides.
                    "delegation": { "type": "client" },
                },
            }),
            ClientEvent::CommentaryAppend {
                delegation_id,
                content,
            } => serde_json::json!({
                "type": "session.commentary.append",
                "event_id": uuid::Uuid::new_v4().to_string(),
                "delegation_id": delegation_id,
                "content": content,
            }),
            // Client delegation has no provider Responses conversation to
            // receive response.item.create. Session context is the Live route.
            // https://developers.openai.com/api/docs/guides/live-conversations
            ClientEvent::UserText { text } => serde_json::json!({
                "type": "session.thinking.append",
                "event_id": uuid::Uuid::new_v4().to_string(),
                "delegation_id": null,
                "content": format!("User typed this message (the application is sending it to the backend): {}", serde_json::to_string(text).unwrap()),
            }),
            ClientEvent::InputAudioAppend { pcm } => {
                use base64::engine::general_purpose::STANDARD;
                use base64::Engine;
                serde_json::json!({
                    "type": "session.input_audio.append",
                    "audio": STANDARD.encode(pcm),
                })
            }
        }
    }
}

/// Splits a backend answer into commentary appends below Live's per-event cap.
/// Splitting on a character boundary is deliberate: the alternative, dropping
/// the tail, would silently shorten a correct answer.
pub fn commentary_chunks(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in trimmed.chars() {
        if current.chars().count() >= COMMENTARY_CHUNK {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keyboard_context_uses_live_not_a_responses_item() {
        let event = ClientEvent::UserText { text: "Correction: 445".into() }.to_json();
        assert_eq!(event["type"], "session.thinking.append");
        assert!(event["delegation_id"].is_null());
        assert!(event["event_id"].is_string());
        assert!(event["content"].as_str().unwrap().contains("Correction: 445"));
    }

    #[test]
    fn audio_deltas_arrive_decoded() {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let pcm = vec![1u8, 2, 3, 4];
        let event = ServerEvent::decode(&json!({
            "type": "session.output_audio.delta",
            "delta": STANDARD.encode(&pcm),
        }));
        assert_eq!(event, ServerEvent::OutputAudioDelta { pcm });
    }

    #[test]
    fn a_corrupt_audio_delta_is_ignored_rather_than_fatal() {
        let event = ServerEvent::decode(&json!({
            "type": "session.output_audio.delta",
            "delta": "not base64 !!!",
        }));
        assert_eq!(event, ServerEvent::Other);
    }

    #[test]
    fn both_transcript_directions_decode_to_their_own_event() {
        assert_eq!(
            ServerEvent::decode(&json!({"type": "session.input_transcript.delta", "delta": "hi"})),
            ServerEvent::InputTranscriptDelta { text: "hi".into() }
        );
        assert_eq!(
            ServerEvent::decode(&json!({"type": "session.output_transcript.delta", "delta": "yo"})),
            ServerEvent::OutputTranscriptDelta { text: "yo".into() }
        );
    }

    #[test]
    fn delegation_without_an_id_is_not_a_delegation() {
        assert_eq!(
            ServerEvent::decode(&json!({"type": "session.delegation.created", "delegation": {}})),
            ServerEvent::Other
        );
        assert_eq!(
            ServerEvent::decode(
                &json!({"type": "session.delegation.created", "delegation": {"id": ""}})
            ),
            ServerEvent::Other
        );
    }

    #[test]
    fn nested_response_events_carry_their_delegation() {
        assert_eq!(
            ServerEvent::decode(&json!({
                "type": "response.event",
                "delegation_id": "d1",
                "event": {"type": "response.completed"},
            })),
            ServerEvent::ResponseCompleted {
                delegation_id: "d1".into()
            }
        );
    }

    #[test]
    fn every_failure_shape_reports_a_message() {
        for kind in ["response.failed", "response.incomplete", "response.cancelled"] {
            let event = ServerEvent::decode(&json!({
                "type": "response.event",
                "delegation_id": "d1",
                "event": {"type": kind},
            }));
            assert_eq!(
                event,
                ServerEvent::ResponseFailed {
                    delegation_id: "d1".into(),
                    message: kind.into()
                }
            );
        }
    }

    #[test]
    fn an_unknown_event_never_fails_the_session() {
        assert_eq!(
            ServerEvent::decode(&json!({"type": "session.some.future.thing"})),
            ServerEvent::Other
        );
        assert_eq!(ServerEvent::decode(&json!({})), ServerEvent::Other);
    }

    #[test]
    fn the_session_envelope_asks_for_client_delegation() {
        let json = ClientEvent::SessionStart {
            model: DEFAULT_VOICE_MODEL.into(),
            instructions: "speak".into(),
        }
        .to_json();
        assert_eq!(json["session"]["delegation"]["type"], "client");
        assert_eq!(json["session"]["store"], false);
        assert_eq!(json["session"]["audio"]["format"]["rate"], AUDIO_SAMPLE_RATE);
        // No tools and no backend model: the voice provider never does the work.
        assert!(json["session"].get("tools").is_none());
        assert!(json["session"]["delegation"].get("responses").is_none());
    }

    #[test]
    fn commentary_splits_without_losing_a_character() {
        let text = "x".repeat(COMMENTARY_CHUNK * 2 + 17);
        let chunks = commentary_chunks(&text);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().all(|c| c.chars().count() <= COMMENTARY_CHUNK));
    }

    #[test]
    fn commentary_counts_characters_not_bytes() {
        // A byte-sized split would cut these in half and produce invalid UTF-8.
        let text = "é".repeat(COMMENTARY_CHUNK + 5);
        let chunks = commentary_chunks(&text);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().all(|c| c.chars().count() <= COMMENTARY_CHUNK));
    }

    #[test]
    fn an_empty_answer_produces_no_commentary() {
        assert!(commentary_chunks("   \n ").is_empty());
    }
}
