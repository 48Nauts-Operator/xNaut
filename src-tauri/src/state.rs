// ABOUTME: Thread-safe application state manager for XNAUT terminal sessions, SSH connections, and triggers.
// ABOUTME: Uses Arc<Mutex<>> for safe concurrent access across async tasks and Tauri commands.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use portable_pty::{PtyPair, Child, MasterPty};
use uuid::Uuid;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

/// Represents an active PTY session with its process and reader
pub struct PtySession {
    pub id: String,
    pub pty_pair: Arc<Mutex<PtyPair>>,
    pub child: Arc<Mutex<Box<dyn Child + Send>>>,
    pub reader: Arc<std::sync::Mutex<Box<dyn std::io::Read + Send>>>,
    pub writer: Arc<std::sync::Mutex<Box<dyn std::io::Write + Send>>>,
    pub created_at: std::time::SystemTime,
}

/// Represents an active SSH connection
#[derive(Debug)]
pub struct SshSession {
    pub id: String,
    pub host: String,
    pub username: String,
    pub connected_at: std::time::SystemTime,
    // SSH session will be managed separately to avoid complex trait bounds
}

/// Represents a terminal trigger pattern
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Trigger {
    pub id: String,
    pub pattern: String,
    pub action: TriggerAction,
    pub enabled: bool,
}

/// Actions that can be triggered by pattern matches
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum TriggerAction {
    Notify { message: String },
    RunCommand { command: String },
    AiAssist { prompt: String },
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

/// Main application state container
pub struct AppState {
    pub pty_sessions: Arc<Mutex<HashMap<String, Arc<PtySession>>>>,
    pub ssh_sessions: Arc<Mutex<HashMap<String, SshSession>>>,
    pub triggers: Arc<Mutex<HashMap<String, Trigger>>>,
    pub shared_sessions: Arc<Mutex<HashMap<String, SharedSession>>>,
}

impl AppState {
    /// Creates a new AppState with empty collections
    pub fn new() -> Self {
        Self {
            pty_sessions: Arc::new(Mutex::new(HashMap::new())),
            ssh_sessions: Arc::new(Mutex::new(HashMap::new())),
            triggers: Arc::new(Mutex::new(HashMap::new())),
            shared_sessions: Arc::new(Mutex::new(HashMap::new())),
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
        assert!(code1.len() > 0 && code1.len() <= 10);
    }
}
