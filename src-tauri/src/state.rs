// ABOUTME: Thread-safe application state manager for XNAUT terminal sessions, SSH connections, and shared sessions.
// ABOUTME: Uses Arc<Mutex<>> for safe concurrent access across async tasks and Tauri commands.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use portable_pty::{Child, PtyPair};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Represents an active PTY session with its process and reader
pub struct PtySession {
    pub _id: String,
    pub pty_pair: Arc<Mutex<PtyPair>>,
    pub child: Arc<Mutex<Box<dyn Child + Send>>>,
    pub reader: Arc<std::sync::Mutex<Box<dyn std::io::Read + Send>>>,
    pub writer: Arc<std::sync::Mutex<Box<dyn std::io::Write + Send>>>,
    pub created_at: std::time::SystemTime,
    /// The zellij session backing this tab, when there is one. Retained so the
    /// bridge can report durability without re-deriving it from the child's
    /// command line.
    pub session_name: Option<String>,
}

/// Represents an active SSH connection
pub struct SshSession {
    pub _id: String,
    pub host: String,
    pub username: String,
    pub connected_at: std::time::SystemTime,
    /// The interactive shell channel. Holding it holds the whole connection:
    /// every ssh2 handle shares one reference-counted session inner, so there
    /// is nothing else to keep. A std mutex on purpose, because the reader
    /// thread and the write command both take it around blocking libssh2 calls.
    pub channel: Arc<std::sync::Mutex<ssh2::Channel>>,
}

/// Session sharing state
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SharedSession {
    pub id: String,
    pub session_id: String,
    pub share_code: String,
    pub read_only: bool,
    pub created_at: i64,
}

/// Per-session output tap for the mobile bridge (XNAUT-32): scrollback ring
/// buffer for attach replay + broadcast channel for live mirroring.
pub struct MobileTap {
    pub tx: tokio::sync::broadcast::Sender<Vec<u8>>,
    pub ring: Vec<u8>,
    /// Desktop PTY dimensions — the phone renders at these or takes over
    /// via the phone-fit resize op.
    pub cols: u16,
    pub rows: u16,
}

/// Max bytes of scrollback replayed to a freshly attached mobile client.
pub const MOBILE_RING_CAP: usize = 256 * 1024;
/// Raw PTY scrollback retained for desktop conversation mirrors and late
/// terminal attachment. This is deliberately bounded per session.
pub const TERMINAL_SCROLLBACK_CAP: usize = 512 * 1024;

impl MobileTap {
    pub fn new(cols: u16, rows: u16) -> Self {
        // ponytail: 64-msg lag window; slow phones skip chunks instead of blocking the PTY reader
        let (tx, _) = tokio::sync::broadcast::channel(64);
        Self {
            tx,
            ring: Vec::new(),
            cols,
            rows,
        }
    }

    /// Appends a chunk to the ring (trimming the front past MOBILE_RING_CAP)
    /// and fans it out to live subscribers.
    pub fn push(&mut self, chunk: &[u8]) {
        self.ring.extend_from_slice(chunk);
        if self.ring.len() > MOBILE_RING_CAP {
            let excess = self.ring.len() - MOBILE_RING_CAP;
            self.ring.drain(..excess);
        }
        let _ = self.tx.send(chunk.to_vec());
    }
}

impl Default for MobileTap {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

/// Main application state container
pub struct AppState {
    pub pty_sessions: Arc<Mutex<HashMap<String, Arc<PtySession>>>>,
    pub ssh_sessions: Arc<Mutex<HashMap<String, SshSession>>>,
    pub shared_sessions: Arc<Mutex<HashMap<String, SharedSession>>>,
    pub active_worklog: Arc<Mutex<Option<crate::worklog::WorkSession>>>,
    /// Agent-session metadata for the Phase 4 status overlay.
    /// Keys are PTY session IDs that were spawned via the agent launcher;
    /// plain shell sessions are absent.
    pub agent_sessions: crate::status::AgentSessions,
    /// Hook server URL + per-session token map (Phase 5).
    pub hook_server: Arc<Mutex<Option<crate::agent_hooks::HookServerInfo>>>,
    /// Tasks Mode settings (v1.6) — loaded from ~/.config/xnaut/settings.json on boot.
    pub settings: Arc<Mutex<crate::settings::Settings>>,
    /// Mobile bridge output taps, keyed by PTY session ID (XNAUT-32).
    pub mobile_taps: Arc<Mutex<HashMap<String, MobileTap>>>,
    /// Per-session raw PTY tail. Agent Space uses it to restore output emitted
    /// before the frontend received the newly-created session id.
    pub terminal_scrollback: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    /// Multi-Agent Manager state published by the desktop pane for the phone
    /// (thread + swarm queue). JSON blob — the desktop JS owns the shape.
    pub mobile_manager: Arc<Mutex<serde_json::Value>>,
    /// In-flight microphone capture (XNAUT-187). None unless the user is
    /// holding the dictate button.
    pub voice: Arc<Mutex<Option<VoiceCapture>>>,
}

/// Handles onto a capture running on its own thread.
///
/// A cpal Stream is not Send, so it can never be stored here; the capture
/// thread owns it and this struct only holds what crosses threads safely.
/// Dropping `stop` ends the thread, which drops the stream and the device.
pub struct VoiceCapture {
    pub samples: Arc<std::sync::Mutex<Vec<i16>>>,
    pub sample_rate: u32,
    pub channels: u16,
    pub stop: std::sync::mpsc::Sender<()>,
}

impl AppState {
    /// Creates a new AppState with empty collections
    pub fn new() -> Self {
        Self {
            pty_sessions: Arc::new(Mutex::new(HashMap::new())),
            ssh_sessions: Arc::new(Mutex::new(HashMap::new())),
            shared_sessions: Arc::new(Mutex::new(HashMap::new())),
            active_worklog: Arc::new(Mutex::new(None)),
            agent_sessions: Arc::new(Mutex::new(HashMap::new())),
            hook_server: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(crate::settings::load_or_default())),
            mobile_taps: Arc::new(Mutex::new(HashMap::new())),
            terminal_scrollback: Arc::new(Mutex::new(HashMap::new())),
            mobile_manager: Arc::new(Mutex::new(serde_json::Value::Null)),
            voice: Arc::new(Mutex::new(None)),
        }
    }

    /// Generates a unique session ID
    pub fn generate_session_id() -> String {
        Uuid::new_v4().to_string()
    }

    /// Generates a shareable session code
    pub fn generate_share_code() -> String {
        // Generate a short, human-readable share code
        let uuid = Uuid::new_v4();
        let bytes = uuid.as_bytes();
        URL_SAFE_NO_PAD.encode(&bytes[..6])
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_creation() {
        let state = AppState::new();
        assert_eq!(state.pty_sessions.try_lock().unwrap().len(), 0);
        assert_eq!(state.ssh_sessions.try_lock().unwrap().len(), 0);
    }

    #[test]
    fn test_session_id_generation() {
        let id1 = AppState::generate_session_id();
        let id2 = AppState::generate_session_id();
        assert_ne!(id1, id2);
        assert!(Uuid::parse_str(&id1).is_ok());
    }

    #[test]
    fn test_share_code_generation() {
        let code1 = AppState::generate_share_code();
        let code2 = AppState::generate_share_code();
        assert_ne!(code1, code2);
        assert!(!code1.is_empty() && code1.len() <= 10);
    }
}
