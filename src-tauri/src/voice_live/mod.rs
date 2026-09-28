//! Public continuous voice conversation (XNAUT-416, V1).
//!
//! One explicit start holds many spoken turns: the microphone streams into the
//! provider, replies stream back, the user can talk over them, and every turn
//! is executed by the xNAUT agent already selected in the conversation.
//!
//! Traced from Bucki (48Nauts/Bucky, development @ 629ff06) — the active public
//! route, `publicVoiceMode=gptLive`. See `session.rs` for the behaviour and the
//! places we deliberately depart from it; the largest is that work is dispatched
//! to the selected xNAUT agent over its own route rather than to Bucki's forced
//! ChatGPT Codex login.
//!
//! Layering: everything that decides *what happens* is in `session.rs` and is
//! pure. This file only performs the resulting actions — socket, microphone,
//! speaker, and the event that asks the frontend to run a turn. The frontend
//! runs it through the composer's existing send path, so permissions, history
//! and NautGate routing are the ones the app already resolved rather than a
//! second implementation that could drift.
//!
//! Private Jarvis/local speech is V2 and is not reachable from here; the
//! optional local companion remains in `voice_local`.

mod context;
mod gate;
mod protocol;
pub mod settings;
pub(crate) mod session;
pub(crate) mod turn;

#[cfg(test)]
mod conversation_tests;

pub use session::ExecutionBinding;
pub use turn::Role;

use crate::state::AppState;
use futures_util::{SinkExt, StreamExt};
use protocol::{ClientEvent, ServerEvent, AUDIO_SAMPLE_RATE};
use serde::{Deserialize, Serialize};
use session::{Caption, DelegationRequest, LiveSession, SessionAction};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::{Emitter, Manager};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
use turn::CommittedMessage;

/// Microphone frames are sent about this often. Small enough that turn
/// detection and barge-in stay responsive, large enough not to flood the
/// socket with tiny frames.
const FRAME_MS: usize = 40;
const FRAME_SAMPLES: usize = AUDIO_SAMPLE_RATE as usize * FRAME_MS / 1000;

/// A conversation that produces nothing for this long is not a conversation.
/// Live bills for connection time, so an abandoned session must not sit open.
const IDLE_TIMEOUT: Duration = Duration::from_secs(900);

/// Credentials live in their own private file, never in repository-tracked
/// settings. Saved credentials are never returned to the webview. Same shape and the same permission check
/// as the local-voice profile, for the same reason.
#[derive(Deserialize, Serialize)]
struct Config {
    /// Omitted in the common case: the public Live endpoint traced from the
    /// reference snapshot. Present when a deployment routes somewhere else.
    #[serde(default)]
    endpoint: Option<String>,
    api_key: String,
    #[serde(default)]
    model: Option<String>,
}

impl Config {
    fn path() -> Result<std::path::PathBuf, String> {
        Ok(dirs::data_dir()
            .ok_or("no app data directory")?
            .join("xnaut/voice-live.json"))
    }

    fn validate(&self) -> Result<url::Url, String> {
        let endpoint = self
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(protocol::DEFAULT_LIVE_URL);
        let url = url::Url::parse(endpoint).map_err(|_| "invalid voice endpoint")?;
        // Encrypted transport only: this carries a bearer credential and the
        // user's speech. No credentials in the URL, no redirect targets.
        if url.scheme() != "wss"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("public voice requires a wss:// endpoint with no embedded credentials".into());
        }
        if self.api_key.trim().is_empty() || self.api_key.len() > 512 || self.api_key.chars().any(char::is_whitespace) {
            return Err("invalid voice API key".into());
        }
        if self.model.as_ref().is_some_and(|m| m.trim().is_empty() || m.len() > 128 || m.chars().any(char::is_control)) {
            return Err("invalid voice model".into());
        }
        Ok(url)
    }

    fn load() -> Result<Self, String> {
        Self::load_at(&Self::path()?)
    }

    fn load_at(path: &std::path::Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path)
            .map_err(|_| "Public voice setup required. Add your API key in Settings → Voice.")?;
        if metadata.len() > 4096 {
            return Err("voice profile is too large".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err("voice profile must be private (chmod 600)".into());
            }
        }
        let config: Self =
            serde_json::from_slice(&std::fs::read(&path).map_err(|_| "cannot read voice profile")?)
                .map_err(|_| "invalid voice profile JSON")?;
        config.validate()?;
        Ok(config)
    }
}

/// What the frontend receives. One event name per session so a late frame from
/// a retired session cannot paint into a different conversation.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum LiveEvent {
    Playback { speaking: bool },
    /// Connected and accepted; speech is flowing.
    Ready { restored: bool },
    /// Live transcript ribbon. Presentation only.
    Caption(Caption),
    /// Persist this into the conversation. Exactly once per turn per role.
    Commit(CommittedMessage),
    /// Run this turn on the selected agent, then call `voice_live_result`.
    Dispatch(DelegationRequest),
    /// Playback stopped because the user spoke.
    Interrupted { generation: u64 },
    Ended { reason: String },
}

pub struct Handle {
    id: String,
    window: String,
    session: Arc<Mutex<LiveSession>>,
    outbound: mpsc::UnboundedSender<ClientEvent>,
    closed: Arc<AtomicBool>,
    /// Current playback fence. The renderer's cancellation closure compares
    /// against it in the audio callback, so a stale frame is dropped at the
    /// device rather than merely unqueued.
    generation: Arc<AtomicU64>,
    /// The speaker, opened lazily and rebuilt after every interruption so one
    /// generation's audio can never be mixed into the next one's.
    renderer: Renderer,
}

type Renderer = Arc<Mutex<Option<(u64, crate::voice_local::playback::Playback)>>>;

impl Handle {
    fn owns(&self, window: &str, id: &str) -> bool {
        self.window == window && self.id == id
    }
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

async fn owned(state: &AppState, window: &str, id: &str) -> Result<Arc<Handle>, String> {
    state
        .live_voice
        .lock()
        .await
        .as_ref()
        .filter(|handle| handle.owns(window, id) && !handle.is_closed())
        .cloned()
        .ok_or_else(|| "voice session ended or belongs to another window".into())
}

/// True when a public voice profile exists, so the UI can offer the control
/// instead of presenting a button that fails on click.
#[tauri::command]
pub fn voice_live_ready() -> bool {
    Config::load().is_ok()
}

/// The actual TLS/authentication path, also exercised by the opt-in account smoke test.
async fn connect(
    config: &Config,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    String,
> {
    let mut request = config
        .validate()?
        .as_str()
        .into_client_request()
        .map_err(|e| format!("invalid voice endpoint: {e}"))?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", config.api_key)
            .parse()
            .map_err(|_| "invalid voice API key")?,
    );
    let (socket, _) = tokio::time::timeout(
        Duration::from_secs(15),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| "the voice service did not answer within 15 seconds".to_string())?
    .map_err(|_| "cannot reach the voice service; check the endpoint and key".to_string())?;

    Ok(socket)
}

/// Connects the session and starts the reader, writer and microphone.
async fn attach(
    app: &tauri::AppHandle,
    window_label: &str,
    session_id: &str,
    session: Arc<Mutex<LiveSession>>,
    config: &Config,
) -> Result<Arc<Handle>, String> {
    let (mut sink, mut stream) = connect(config).await?.split();
    let (outbound, mut pending) = mpsc::unbounded_channel::<ClientEvent>();
    let closed = Arc::new(AtomicBool::new(false));
    // A fresh transport starts at fence zero; the session is new too, because
    // a dropped conversation is reopened rather than re-socketed.
    let generation = Arc::new(AtomicU64::new(0));

    let handle = Arc::new(Handle {
        id: session_id.to_string(),
        window: window_label.to_string(),
        session: session.clone(),
        outbound,
        closed: closed.clone(),
        generation,
        renderer: Arc::new(Mutex::new(None)),
    });
    let opening = session.lock().await.open();
    for action in opening {
        perform(app, &handle, action).await;
    }

    // Writer: one task owns the sink, so audio frames and control events cannot
    // interleave mid-message.
    {
        let closed = closed.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = pending.recv().await {
                if closed.load(Ordering::Acquire) {
                    break;
                }
                if sink
                    .send(Message::Text(event.to_json().to_string()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            let _ = sink.close().await;
        });
    }

    // Reader: decode, advance the state machine, perform what it returns.
    {
        let app = app.clone();
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let message = match tokio::time::timeout(IDLE_TIMEOUT, stream.next()).await {
                    Err(_) => {
                        finish(&app, &handle, "The voice session timed out.").await;
                        break;
                    }
                    Ok(None) => {
                        finish(&app, &handle, "The voice service disconnected.").await;
                        break;
                    }
                    Ok(Some(Err(_))) => {
                        finish(&app, &handle, "The voice connection failed.").await;
                        break;
                    }
                    Ok(Some(Ok(message))) => message,
                };
                let payload = match message {
                    Message::Text(text) => text,
                    Message::Close(_) => {
                        finish(&app, &handle, "The voice service closed the session.").await;
                        break;
                    }
                    // Ping/pong and binary frames are not part of this protocol.
                    _ => continue,
                };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
                    continue;
                };
                let event = ServerEvent::decode(&value);
                let (actions, became_ready, restored) = {
                    let mut session = handle.session.lock().await;
                    let before = session.is_ready();
                    let actions = session.handle(event, now());
                    (
                        actions,
                        !before && session.is_ready(),
                        session.has_restored_context(),
                    )
                };
                if became_ready {
                    emit(&app, &handle, LiveEvent::Ready { restored });
                }
                let ended = actions
                    .iter()
                    .any(|action| matches!(action, SessionAction::Ended { .. }));
                for action in actions {
                    perform(&app, &handle, action).await;
                }
                if ended {
                    handle.closed.store(true, Ordering::Release);
                    break;
                }
            }
            release(&app, &handle).await;
        });
    }

    spawn_capture(app.clone(), handle.clone())?;
    {
        let app = app.clone();
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            let mut previous = false;
            while !handle.is_closed() {
                let speaking = handle.renderer.lock().await.as_ref()
                    .is_some_and(|(_, player)| player.is_playing());
                if speaking != previous {
                    emit(&app, &handle, LiveEvent::Playback { speaking });
                    previous = speaking;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }
    Ok(handle)
}

/// Opens the conversation. `history` is the saved conversation being reopened,
/// oldest first; it is replayed into the backend context and never re-executed.
#[tauri::command]
pub async fn voice_live_open(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
    binding: ExecutionBinding,
    history: Vec<RestoredMessage>,
    transcription_only: Option<bool>,
) -> Result<(), String> {
    uuid::Uuid::parse_str(&session_id).map_err(|_| "invalid voice session ID")?;
    if binding.conversation_id.trim().is_empty() {
        return Err("a voice session must be pinned to a conversation".into());
    }
    let config = Config::load()?;

    // One microphone owner across the whole app: dictation, the optional local
    // companion and this must never record at the same time.
    let mut active = state.live_voice.lock().await;
    if active.as_ref().is_some_and(|h| !h.is_closed()) {
        return Err("another voice conversation is already open".into());
    }
    if state.voice.lock().await.is_some() {
        return Err("finish the current dictation before starting voice".into());
    }
    if state.local_voice.lock().await.is_some() {
        return Err("end the local voice session before starting public voice".into());
    }

    let model = config
        .model
        .clone()
        .unwrap_or_else(|| protocol::DEFAULT_VOICE_MODEL.to_string());
    let mut live = LiveSession::new(binding, model);
    live.set_transcription_only(transcription_only.unwrap_or(false));
    let restored: Vec<(Role, String)> = history
        .into_iter()
        .map(|message| (message.role, message.text))
        .collect();
    if !restored.is_empty() {
        live.restore(&restored);
    }
    let handle = attach(
        &app,
        window.label(),
        &session_id,
        Arc::new(Mutex::new(live)),
        &config,
    )
    .await?;
    *active = Some(handle);
    Ok(())
}

/// How a delegated turn ended. `turn` and `epoch` come straight back from the
/// dispatch event, which is what lets a result from a retired connection be
/// recognised and dropped.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnOutcome {
    pub turn: u64,
    pub epoch: u64,
    pub answer: String,
    /// The run's real outcome. A failure is still spoken, never dressed up as
    /// an answer.
    #[serde(default)]
    pub failed: bool,
}

/// A message from the saved conversation, replayed as context on reopen.
#[derive(Debug, Clone, Deserialize)]
pub struct RestoredMessage {
    pub role: Role,
    pub text: String,
}

/// The selected agent finished a delegated turn. `failed` reports the run's
/// actual outcome rather than dressing a failure as an answer.
#[tauri::command]
pub async fn voice_live_result(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
    outcome: TurnOutcome,
) -> Result<(), String> {
    let handle = owned(state.inner(), window.label(), &session_id).await?;
    let actions = {
        let mut session = handle.session.lock().await;
        if outcome.failed {
            session.fail_delegation(outcome.turn, outcome.epoch, &outcome.answer)
        } else {
            session.complete_delegation(outcome.turn, outcome.epoch, &outcome.answer)
        }
    };
    for action in actions {
        perform(&app, &handle, action).await;
    }
    Ok(())
}

/// Typed input during a voice conversation.
#[tauri::command]
pub async fn voice_live_text(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
    text: String,
) -> Result<(), String> {
    let handle = owned(state.inner(), window.label(), &session_id).await?;
    let actions = {
        let mut session = handle.session.lock().await;
        session.send_text(&text)
    };
    for action in actions {
        perform(&app, &handle, action).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn voice_live_close(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let handle = {
        let mut active = state.live_voice.lock().await;
        match active.as_ref() {
            Some(handle) if handle.owns(window.label(), &session_id) => active.take(),
            _ => None,
        }
    };
    if let Some(handle) = handle {
        finish(&app, &handle, "You ended the voice conversation.").await;
    }
    Ok(())
}

/// A closing or reloading window releases its conversation, so the microphone
/// and the billable connection never outlive the surface that owns them.
pub async fn release_window(state: &AppState, window: &str) {
    let handle = {
        let mut active = state.live_voice.lock().await;
        match active.as_ref() {
            Some(handle) if handle.window == window => active.take(),
            _ => None,
        }
    };
    if let Some(handle) = handle {
        handle.closed.store(true, Ordering::Release);
        let mut session = handle.session.lock().await;
        session.close("The window closed.");
    }
}

async fn finish(app: &tauri::AppHandle, handle: &Arc<Handle>, reason: &str) {
    if handle.closed.swap(true, Ordering::AcqRel) {
        return;
    }
    let actions = {
        let mut session = handle.session.lock().await;
        session.close(reason)
    };
    for action in actions {
        perform(app, handle, action).await;
    }
    release(app, handle).await;
}

/// Drops the registry entry if it still points at this session. Called from the
/// reader task as well as from `finish`, because a socket that dies on its own
/// must not leave a dead handle holding the microphone lease.
async fn release(app: &tauri::AppHandle, handle: &Arc<Handle>) {
    handle.closed.store(true, Ordering::Release);
    // Move the fence so any frame still in flight is refused by the renderer.
    handle.generation.fetch_add(1, Ordering::AcqRel);
    if let Some(state) = app.try_state::<AppState>() {
        let mut active = state.live_voice.lock().await;
        if active.as_ref().is_some_and(|open| open.id == handle.id) {
            active.take();
        }
    }
}

fn emit(app: &tauri::AppHandle, handle: &Arc<Handle>, event: LiveEvent) {
    // Per-session event name: a retired session's late frame cannot paint into
    // whatever conversation is on screen now.
    let _ = app.emit(&format!("voice-live://{}", handle.id), event);
}

async fn perform(app: &tauri::AppHandle, handle: &Arc<Handle>, action: SessionAction) {
    match action {
        SessionAction::Send(event) => {
            let _ = handle.outbound.send(event);
        }
        SessionAction::Play { generation, pcm } => {
            play(handle, generation, pcm).await;
        }
        SessionAction::StopPlayback { generation } => {
            handle.generation.store(generation, Ordering::Release);
            stop_playback(handle).await;
            emit(app, handle, LiveEvent::Interrupted { generation });
        }
        SessionAction::Commit(message) => emit(app, handle, LiveEvent::Commit(message)),
        SessionAction::Dispatch(request) => emit(app, handle, LiveEvent::Dispatch(request)),
        SessionAction::Caption(caption) => emit(app, handle, LiveEvent::Caption(caption)),
        SessionAction::Ended { reason } => emit(app, handle, LiveEvent::Ended { reason }),
    }
}

async fn play(handle: &Arc<Handle>, generation: u64, pcm: Vec<u8>) {
    if generation != handle.generation.load(Ordering::Acquire) || handle.is_closed() {
        return;
    }
    let samples: Vec<i16> = pcm
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    if samples.is_empty() {
        return;
    }
    let mut slot = handle.renderer.lock().await;
    if slot.as_ref().is_some_and(|(open, _)| *open != generation) {
        // The generation moved on; retire the old device before opening a new one.
        if let Some((_, player)) = slot.take() {
            let _ = player.finish().await;
        }
    }
    if slot.is_none() {
        let fence = handle.generation.clone();
        let closed = handle.closed.clone();
        let mine = generation;
        match crate::voice_local::playback::Playback::open(
            Arc::new(move || {
                closed.load(Ordering::Acquire) || fence.load(Ordering::Acquire) != mine
            }),
            AUDIO_SAMPLE_RATE,
        )
        .await
        {
            Ok(player) => *slot = Some((generation, player)),
            Err(_) => return,
        }
    }
    if let Some((_, player)) = slot.as_ref() {
        // A refused push means the fence moved or the device went away; either
        // way the next frame reopens, so there is nothing to repair here.
        let _ = player.push(&samples).await;
    }
}

async fn stop_playback(handle: &Arc<Handle>) {
    let mut slot = handle.renderer.lock().await;
    if let Some((_, player)) = slot.take() {
        // Dropping through `finish` joins the device thread, which is what
        // actually silences the speaker.
        let _ = player.finish().await;
    }
}

/// Streams the microphone into the session: resampled to the negotiated rate,
/// levelled for the barge-in heuristic, framed and sent.
fn spawn_capture(app: tauri::AppHandle, handle: Arc<Handle>) -> Result<(), String> {
    let (frames_tx, mut frames_rx) = mpsc::unbounded_channel::<(Vec<i16>, f64)>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let closed = handle.closed.clone();

    std::thread::spawn(move || {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let opened = (|| -> Result<(cpal::Stream, u32, u16), String> {
            let device = cpal::default_host()
                .default_input_device()
                .ok_or_else(|| "no microphone found".to_string())?;
            let config = device
                .default_input_config()
                .map_err(|e| format!("microphone unavailable: {e}"))?;
            let rate = config.sample_rate().0;
            let channels = config.channels();
            if rate == 0 || channels == 0 {
                return Err("invalid microphone format".into());
            }
            let err_fn = |e| eprintln!("[voice-live] capture error: {e}");
            let sink = frames_tx.clone();
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &_| {
                        let pcm: Vec<i16> =
                            data.iter().map(|s| (s * i16::MAX as f32) as i16).collect();
                        let _ = sink.send((resample(&pcm, rate, channels), level(&pcm)));
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &_| {
                        let _ = sink.send((resample(data, rate, channels), level(data)));
                    },
                    err_fn,
                    None,
                ),
                other => return Err(format!("unsupported microphone sample format: {other}")),
            }
            .map_err(|e| format!("cannot open microphone: {e}"))?;
            stream
                .play()
                .map_err(|e| format!("cannot start microphone: {e}"))?;
            Ok((stream, rate, channels))
        })();
        match opened {
            Err(error) => {
                let _ = ready_tx.send(Err(error));
            }
            Ok((stream, _, _)) => {
                let _ = ready_tx.send(Ok(()));
                // Hold the device until the session closes. Dropping the stream
                // is what releases the microphone.
                while !closed.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                drop(stream);
            }
        }
    });

    ready_rx
        .recv_timeout(Duration::from_secs(15))
        .map_err(|_| "the microphone did not become ready within 15 seconds".to_string())??;

    tauri::async_runtime::spawn(async move {
        let mut buffer: Vec<i16> = Vec::with_capacity(FRAME_SAMPLES * 2);
        while let Some((samples, level)) = frames_rx.recv().await {
            if handle.is_closed() {
                break;
            }
            // Energy first: stopping playback before the server's transcript
            // arrives is most of what makes barge-in feel immediate.
            let actions = {
                let mut session = handle.session.lock().await;
                session.microphone_level(level, output_level(&handle), now())
            };
            for action in actions {
                perform(&app, &handle, action).await;
            }
            buffer.extend_from_slice(&samples);
            while buffer.len() >= FRAME_SAMPLES {
                let frame: Vec<i16> = buffer.drain(..FRAME_SAMPLES).collect();
                let pcm = frame.iter().flat_map(|s| s.to_le_bytes()).collect();
                if handle
                    .outbound
                    .send(ClientEvent::InputAudioAppend { pcm })
                    .is_err()
                {
                    return;
                }
            }
        }
    });
    Ok(())
}

/// How loud our own output currently is, used to keep the speaker from
/// interrupting the assistant on a machine without hardware echo cancellation.
/// Zero while nothing is playing.
fn output_level(handle: &Arc<Handle>) -> f64 {
    match handle.renderer.try_lock() {
        Ok(slot) if slot.is_some() => 0.25,
        _ => 0.0,
    }
}

fn now() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

/// Root-mean-square level, normalised to 0..1.
fn level(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples
        .iter()
        .map(|s| {
            let v = *s as f64 / i16::MAX as f64;
            v * v
        })
        .sum();
    (sum / samples.len() as f64).sqrt()
}

/// Device rate and channel count to mono at the negotiated rate. Nearest
/// neighbour: this is the microphone path, where the provider's recogniser is
/// far more tolerant than the speaker path a listener judges.
fn resample(samples: &[i16], from_rate: u32, channels: u16) -> Vec<i16> {
    if samples.is_empty() || from_rate == 0 || channels == 0 {
        return Vec::new();
    }
    let channels = channels as usize;
    let frames = samples.len() / channels;
    if frames == 0 {
        return Vec::new();
    }
    let target = (frames as u64 * AUDIO_SAMPLE_RATE as u64 / from_rate as u64) as usize;
    (0..target)
        .map(|n| {
            let source = n * frames / target.max(1);
            // Average the channels rather than taking the first: a headset
            // whose speech lands on the right channel must not go silent.
            let start = source * channels;
            let sum: i32 = samples[start..start + channels]
                .iter()
                .map(|s| *s as i32)
                .sum();
            (sum / channels as i32) as i16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses the configured account for a short, billable connection. No microphone
    /// or conversation content is sent. Run explicitly; never requires CI secrets.
    #[tokio::test]
    #[ignore = "requires the private voice-live.json profile and a live account"]
    async fn live_account_accepts_session_start_over_tls() {
        settings::voice_live_settings_test()
            .await
            .expect("saved profile must pass the Settings connection test");
    }

    fn profile(endpoint: Option<&str>) -> Config {
        Config {
            endpoint: endpoint.map(str::to_string),
            api_key: "k".repeat(40),
            model: None,
        }
    }

    #[test]
    fn a_plain_http_endpoint_is_refused() {
        let error = profile(Some("http://api.example.com/v1/live/sessions"))
            .validate()
            .unwrap_err();
        assert!(error.contains("wss://"), "error was {error}");
    }

    #[test]
    fn credentials_embedded_in_the_url_are_refused() {
        assert!(profile(Some("wss://user:secret@api.example.com/v1/live/sessions"))
            .validate()
            .is_err());
    }

    #[test]
    fn an_empty_or_oversized_key_is_refused() {
        for key in ["   ".to_string(), "k".repeat(513)] {
            let mut config = profile(None);
            config.api_key = key;
            assert!(config.validate().is_err());
        }
    }

    #[test]
    fn an_omitted_endpoint_falls_back_to_the_traced_live_url() {
        let url = profile(None).validate().expect("the default is usable");
        assert_eq!(url.as_str(), protocol::DEFAULT_LIVE_URL);
        // A blank string is an omission, not a different endpoint.
        assert_eq!(
            profile(Some("   ")).validate().unwrap().as_str(),
            protocol::DEFAULT_LIVE_URL
        );
    }

    #[test]
    fn a_valid_profile_passes() {
        assert!(profile(Some("wss://voice.internal.example/v1/live/sessions"))
            .validate()
            .is_ok());
    }

    #[test]
    fn silence_measures_zero_and_a_full_scale_tone_measures_one() {
        assert_eq!(level(&[]), 0.0);
        assert_eq!(level(&[0; 128]), 0.0);
        assert!((level(&[i16::MAX; 128]) - 1.0).abs() < 0.001);
    }

    #[test]
    fn resampling_lands_on_the_negotiated_rate() {
        // One second of 48 kHz mono becomes one second at 24 kHz.
        let input = vec![1000i16; 48_000];
        assert_eq!(resample(&input, 48_000, 1).len(), AUDIO_SAMPLE_RATE as usize);
    }

    #[test]
    fn stereo_is_averaged_rather_than_half_discarded() {
        // Silence on the left, speech on the right: taking channel 0 would
        // send an empty conversation to the recogniser.
        let stereo: Vec<i16> = (0..2_000).flat_map(|_| [0i16, 8_000]).collect();
        let mono = resample(&stereo, AUDIO_SAMPLE_RATE, 2);
        assert_eq!(mono.len(), 2_000);
        assert!(mono.iter().all(|s| *s == 4_000));
    }

    #[test]
    fn a_degenerate_buffer_never_panics() {
        assert!(resample(&[], 48_000, 1).is_empty());
        assert!(resample(&[1, 2, 3], 0, 1).is_empty());
        assert!(resample(&[1, 2, 3], 48_000, 0).is_empty());
        // Fewer samples than channels: not a whole frame.
        assert!(resample(&[1], 48_000, 2).is_empty());
    }

    #[test]
    fn a_frame_is_forty_milliseconds_of_audio() {
        assert_eq!(FRAME_SAMPLES, 960);
    }
}
