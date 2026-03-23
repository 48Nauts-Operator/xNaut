// ABOUTME: Custom error types for XNAUT application using thiserror for ergonomic error handling.
// ABOUTME: Provides specific error variants for PTY, SSH, AI, and session management operations.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum XnautError {
    #[error("PTY session not found: {0}")]
    PtySessionNotFound(String),

    #[error("Failed to create PTY: {0}")]
    PtyCreationFailed(String),

    #[error("Failed to write to PTY: {0}")]
    PtyWriteFailed(String),

    #[error("Failed to resize PTY: {0}")]
    PtyResizeFailed(String),

    #[error("SSH connection failed: {0}")]
    SshConnectionFailed(String),

    #[error("SSH authentication failed")]
    SshAuthFailed,

    #[error("SSH session not found: {0}")]
    SshSessionNotFound(String),

    #[error("Shared session not found: {0}")]
    SharedSessionNotFound(String),

    #[error("Trigger not found: {0}")]
    TriggerNotFound(String),

    #[error("Invalid trigger pattern: {0}")]
    InvalidTriggerPattern(String),

    #[error("AI service error: {0}")]
    AiServiceError(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl From<XnautError> for String {
    fn from(error: XnautError) -> Self {
        error.to_string()
    }
}

pub type XnautResult<T> = Result<T, XnautError>;
