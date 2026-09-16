//! Optional loopback-only speech transport. Credentials never enter the webview.
//! Wire protocol: companions/local-voice/README.md. No cloud fallback or LLM.
mod playback;

use crate::state::AppState;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest, protocol::WebSocketConfig, Message,
};

#[derive(Deserialize)]
struct Config {
    endpoint: String,
    token: String,
}

impl Config {
    fn validate(&self) -> Result<url::Url, String> {
        let url = url::Url::parse(&self.endpoint).map_err(|_| "invalid local voice endpoint")?;
        // Literal IPs only: no proxy, DNS rebinding, credentials or redirects.
        if url.scheme() != "http"
            || !matches!(url.host_str(), Some("127.0.0.1") | Some("[::1]"))
            || url.port().is_none()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(
                "local voice requires http://127.0.0.1:PORT (or [::1]), with no path or query"
                    .into(),
            );
        }
        if self.token.len() < 32
            || self.token.len() > 256
            || !self
                .token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err("invalid local voice token".into());
        }
        Ok(url)
    }
    fn load() -> Result<Self, String> {
        let path = dirs::data_dir()
            .ok_or("no app data directory")?
            .join("xnaut/voice-local.json");
        let metadata = std::fs::metadata(&path).map_err(|_| format!("Local voice setup required. Start the optional companion with --connection-file {}", path.display()))?;
        if metadata.len() > 4096 {
            return Err("local voice profile is too large".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err("local voice profile must be private (chmod 600)".into());
            }
        }
        let config: Self = serde_json::from_slice(
            &std::fs::read(path).map_err(|_| "cannot read local voice profile")?,
        )
        .map_err(|_| "invalid local voice profile JSON")?;
        config.validate()?;
        Ok(config)
    }
}

#[derive(Deserialize, Serialize)]
pub struct Capabilities {
    version: u32,
    recognition: bool,
    synthesis: bool,
    streaming_input: bool,
    streaming_output: bool,
    format: String,
    sample_rate: u32,
    channels: u16,
    max_text_bytes: usize,
    max_input_seconds: u32,
    synthetic: bool,
}
impl Capabilities {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || !self.recognition
            || !self.synthesis
            || !self.streaming_output
            || self.format != "pcm_s16le"
            || self.sample_rate != 16_000
            || self.channels != 1
            || self.max_text_bytes < 4096
            || self.max_input_seconds < 120
            || self.synthetic
        {
            return Err("local voice service capabilities are unsupported (test services are not live speech)".into());
        }
        Ok(())
    }
}

pub struct Session {
    id: String,
    window: String,
    config: Config,
    closed: AtomicBool,
    interrupted: AtomicU64,
    playing: AtomicU64,
}
impl Session {
    pub(crate) fn window_is(&self, window: &str) -> bool {
        self.window == window
    }
    fn owns(&self, window: &str, id: &str) -> bool {
        self.window == window && self.id == id
    }
    fn cancelled(&self, generation: u64) -> bool {
        self.closed.load(Ordering::Acquire)
            || (generation > 0
                && (generation <= self.interrupted.load(Ordering::Acquire)
                    || generation != self.playing.load(Ordering::Acquire)))
    }
}

async fn owned(state: &AppState, window: &str, id: &str) -> Result<Arc<Session>, String> {
    state
        .local_voice
        .lock()
        .await
        .as_ref()
        .filter(|s| s.owns(window, id) && !s.cancelled(0))
        .cloned()
        .ok_or_else(|| "local voice session ended or belongs to another window".into())
}

#[tauri::command]
pub async fn voice_local_open(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<Capabilities, String> {
    uuid::Uuid::parse_str(&session_id).map_err(|_| "invalid voice session ID")?;
    let config = Config::load()?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(
            config
                .validate()?
                .join("voice/v1/capabilities")
                .map_err(|e| e.to_string())?,
        )
        .bearer_auth(&config.token)
        .send()
        .await
        .map_err(|_| "local voice service is unavailable")?
        .error_for_status()
        .map_err(|_| "local voice service refused the connection; check its token")?;
    let mut body = Vec::new();
    while let Some(bytes) = response
        .chunk()
        .await
        .map_err(|_| "cannot read local voice capabilities")?
    {
        if body.len() + bytes.len() > 4096 {
            return Err("local voice capabilities exceed size limit".into());
        }
        body.extend(bytes);
    }
    let caps: Capabilities =
        serde_json::from_slice(&body).map_err(|_| "invalid local voice capabilities")?;
    caps.validate()?;
    let mut active = state.local_voice.lock().await;
    if active.is_some() {
        return Err("another local voice session is already open".into());
    }
    if state.voice.lock().await.is_some() {
        return Err("finish the current dictation before opening local voice".into());
    }
    *active = Some(Arc::new(Session {
        id: session_id,
        window: window.label().into(),
        config,
        closed: AtomicBool::new(false),
        interrupted: AtomicU64::new(0),
        playing: AtomicU64::new(0),
    }));
    Ok(caps)
}

#[tauri::command]
pub async fn voice_local_close(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let mut active = state.local_voice.lock().await;
    if active
        .as_ref()
        .is_some_and(|s| s.owns(window.label(), &session_id))
    {
        if let Some(session) = active.take() {
            session.closed.store(true, Ordering::Release);
        }
    }
    Ok(())
}

pub async fn release_window(state: &AppState, window: &str) {
    let mut active = state.local_voice.lock().await;
    if active.as_ref().is_some_and(|s| s.window == window) {
        if let Some(session) = active.take() {
            session.closed.store(true, Ordering::Release);
        }
    }
}

#[tauri::command]
pub async fn voice_local_interrupt(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
    generation: u64,
) -> Result<(), String> {
    let session = owned(state.inner(), window.label(), &session_id).await?;
    session.interrupted.fetch_max(generation, Ordering::AcqRel);
    Ok(())
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
async fn connect(session: &Session) -> Result<Socket, String> {
    let mut url = session
        .config
        .validate()?
        .join("voice/v1/session")
        .map_err(|e| e.to_string())?;
    url.set_scheme("ws").map_err(|_| "invalid websocket URL")?;
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|_| "invalid websocket request")?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", session.config.token)
            .parse()
            .map_err(|_| "invalid token header")?,
    );
    let config = WebSocketConfig {
        max_message_size: Some(16_404),
        max_frame_size: Some(16_404),
        ..Default::default()
    };
    tokio::time::timeout(
        Duration::from_secs(8),
        tokio_tungstenite::connect_async_with_config(request, Some(config), false),
    )
    .await
    .map_err(|_| "local voice connection timed out")?
    .map(|(socket, _)| socket)
    .map_err(|_| "local voice connection failed".to_string())
}

async fn cancelled(session: &Session, generation: u64) {
    while !session.cancelled(generation) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn pack(id: uuid::Uuid, sequence: u32, pcm: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + pcm.len() * 2);
    bytes.extend_from_slice(id.as_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    for sample in pcm {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}
fn unpack(bytes: &[u8], id: uuid::Uuid, sequence: u32) -> Result<Vec<i16>, String> {
    if bytes.len() < 22
        || bytes.len() > 16_404
        || (bytes.len() - 20) % 2 != 0
        || &bytes[..16] != id.as_bytes()
        || bytes[16..20] != sequence.to_be_bytes()
    {
        return Err("invalid, stale or out-of-order local audio frame".into());
    }
    Ok(bytes[20..]
        .chunks_exact(2)
        .map(|x| i16::from_le_bytes([x[0], x[1]]))
        .collect())
}

async fn exchange(
    session: Arc<Session>,
    generation: u64,
    text: Option<String>,
    pcm: &[i16],
) -> Result<String, String> {
    let work = async {
        let mut socket = connect(&session).await?;
        let id = uuid::Uuid::new_v4();
        let synthesis = text.is_some();
        let request = if let Some(text) = text {
            json!({"type":"synthesize","id":id,"text":text})
        } else {
            json!({"type":"transcribe","id":id})
        };
        socket
            .send(Message::Text(request.to_string()))
            .await
            .map_err(|_| "local voice send failed")?;
        if !synthesis {
            for (sequence, samples) in pcm.chunks(8192).enumerate() {
                socket
                    .send(Message::Binary(pack(id, sequence as u32, samples)))
                    .await
                    .map_err(|_| "local voice audio send failed")?;
            }
            socket
                .send(Message::Text(
                    json!({"type":"input.commit","id":id}).to_string(),
                ))
                .await
                .map_err(|_| "local voice commit failed")?;
        }
        let mut player = None;
        let mut sequence = 0;
        let mut transcript = None;
        let mut ready = false;
        let mut samples_received = 0;
        while let Some(message) = socket.next().await {
            match message.map_err(|_| "local voice stream disconnected")? {
                Message::Text(raw) => {
                    if raw.len() > 8192 {
                        return Err("local voice control frame too large".into());
                    }
                    let event: Value = serde_json::from_str(&raw)
                        .map_err(|_| "invalid local voice control frame")?;
                    if event.get("id").and_then(Value::as_str) != Some(&id.to_string()) {
                        return Err("local voice response has wrong utterance ID".into());
                    }
                    match event.get("type").and_then(Value::as_str) {
                        Some("error") => {
                            return Err(event
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("local voice engine failed")
                                .chars()
                                .take(200)
                                .collect())
                        }
                        Some("ready") if !ready => {
                            if event["version"] != 1
                                || event["sample_rate"] != 16_000
                                || event["channels"] != 1
                                || event["format"] != "pcm_s16le"
                            {
                                return Err("local voice changed audio format".into());
                            }
                            ready = true;
                            if synthesis {
                                let s = session.clone();
                                player = Some(
                                    playback::Playback::open(Arc::new(move || {
                                        s.cancelled(generation)
                                    }))
                                    .await?,
                                );
                            }
                        }
                        Some("transcript.final") if ready && !synthesis && transcript.is_none() => {
                            let text = event["text"].as_str().ok_or("invalid local transcript")?;
                            if text.len() > 8192 {
                                return Err("local transcript exceeds limit".into());
                            }
                            transcript = Some(text.to_string());
                            sequence += 1;
                        }
                        Some("done") if ready && sequence > 0 && event["frames"] == sequence => {
                            if let Some(p) = player.take() {
                                p.finish().await?;
                            }
                            return Ok(transcript.unwrap_or_default());
                        }
                        _ => return Err("unexpected local voice event".into()),
                    }
                }
                Message::Binary(bytes) if synthesis && ready => {
                    let samples = unpack(&bytes, id, sequence)?;
                    samples_received += samples.len();
                    if samples_received > 16_000 * 120 {
                        return Err("speech output exceeded duration limit".into());
                    }
                    player
                        .as_ref()
                        .ok_or("speaker not ready")?
                        .push(&samples)
                        .await?;
                    sequence += 1;
                }
                Message::Ping(_) | Message::Pong(_) => {}
                _ => return Err("local voice stream ended before completion".into()),
            }
        }
        Err("local voice stream ended before completion".into())
    };
    tokio::select! {
        biased;
        _ = cancelled(&session, generation) => Err("voice cancelled".into()),
        result = tokio::time::timeout(Duration::from_secs(240), work) => result.map_err(|_| "local voice request timed out".to_string())?,
    }
}

#[tauri::command]
pub async fn voice_local_transcribe(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
    capture_id: String,
) -> Result<crate::voice::Transcript, String> {
    let session = owned(state.inner(), window.label(), &session_id).await?;
    let (pcm, seconds) = crate::voice::take_pcm(state.inner(), window.label(), &capture_id).await?;
    let text = exchange(session, 0, None, &pcm).await?;
    Ok(crate::voice::Transcript { text, seconds })
}

#[tauri::command]
pub async fn voice_local_speak(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
    generation: u64,
    text: String,
) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > 4096 {
        return Err("speech text must be 1–4096 bytes".into());
    }
    let session = owned(state.inner(), window.label(), &session_id).await?;
    if state.voice.lock().await.is_some() {
        return Err("finish recording before starting speech playback".into());
    }
    if generation == 0 || generation <= session.interrupted.load(Ordering::Acquire) {
        return Err("speech cancelled".into());
    }
    // Monotonic generations reject duplicated and reordered native IPC calls.
    let previous = session.playing.fetch_max(generation, Ordering::AcqRel);
    if generation <= previous {
        return Err("stale speech generation".into());
    }
    exchange(session, generation, Some(text), &[])
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transport_commits_pcm_once_and_accepts_only_its_final_transcript() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let request: Value =
                serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(request["type"], "transcribe");
            let id = uuid::Uuid::parse_str(request["id"].as_str().unwrap()).unwrap();
            let audio = socket.next().await.unwrap().unwrap().into_data();
            assert_eq!(unpack(&audio, id, 0).unwrap(), [11, 12, 13]);
            let commit: Value =
                serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(commit["type"], "input.commit");
            for event in [
                json!({"type":"ready","id":id,"version":1,"sample_rate":16000,"channels":1,"format":"pcm_s16le"}),
                json!({"type":"transcript.final","id":id,"text":"The synthetic transcript."}),
                json!({"type":"done","id":id,"frames":1}),
            ] {
                socket.send(Message::Text(event.to_string())).await.unwrap();
            }
        });
        let session = Arc::new(Session {
            id: "test".into(),
            window: "main".into(),
            config: Config {
                endpoint,
                token: "x".repeat(32),
            },
            closed: AtomicBool::new(false),
            interrupted: AtomicU64::new(0),
            playing: AtomicU64::new(0),
        });
        assert_eq!(
            exchange(session, 0, None, &[11, 12, 13]).await.unwrap(),
            "The synthetic transcript."
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn closing_a_session_cancels_a_stalled_transport() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            while socket.next().await.is_some() {}
        });
        let session = Arc::new(Session {
            id: "test".into(),
            window: "main".into(),
            config: Config {
                endpoint,
                token: "x".repeat(32),
            },
            closed: AtomicBool::new(false),
            interrupted: AtomicU64::new(0),
            playing: AtomicU64::new(0),
        });
        let close = session.clone();
        let task = tokio::spawn(async move { exchange(session, 0, None, &[1, 2]).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        close.closed.store(true, Ordering::Release);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err(),
            "voice cancelled"
        );
        server.abort();
    }
    #[test]
    fn endpoints_cannot_escape_loopback_or_put_credentials_in_urls() {
        for endpoint in [
            "https://example.com",
            "http://localhost:8791",
            "http://127.0.0.1:8791/path",
            "http://127.0.0.1:8791?token=secret",
            "http://user@127.0.0.1:8791",
        ] {
            assert!(Config {
                endpoint: endpoint.into(),
                token: "a".repeat(32)
            }
            .validate()
            .is_err());
        }
        for endpoint in ["http://127.0.0.1:8791", "http://[::1]:8791"] {
            assert!(Config {
                endpoint: endpoint.into(),
                token: "a".repeat(32)
            }
            .validate()
            .is_ok());
        }
    }
    #[test]
    fn audio_frames_bind_pcm_to_one_utterance_and_sequence() {
        let id = uuid::Uuid::new_v4();
        let bytes = pack(id, 2, &[0, -32768, 32767]);
        assert_eq!(unpack(&bytes, id, 2).unwrap(), [0, -32768, 32767]);
        assert!(unpack(&bytes, id, 3).is_err());
        assert!(unpack(&bytes, uuid::Uuid::new_v4(), 2).is_err());
        assert!(unpack(&bytes[..21], id, 2).is_err());
    }
    #[test]
    fn interrupt_before_speak_and_close_both_fence_late_output() {
        let session = Session {
            id: "session".into(),
            window: "main".into(),
            config: Config {
                endpoint: String::new(),
                token: String::new(),
            },
            closed: AtomicBool::new(false),
            interrupted: AtomicU64::new(2),
            playing: AtomicU64::new(3),
        };
        assert!(session.cancelled(2));
        assert!(!session.cancelled(3));
        session.closed.store(true, Ordering::Release);
        assert!(session.cancelled(3));
        assert!(session.cancelled(0));
        assert!(!session.owns("other", "session"));
    }
}
