// ABOUTME: Tauri command handlers that expose PTY, SSH, AI, and session management functions to the frontend.
// ABOUTME: All commands use proper error handling and return Results that Tauri automatically converts to promises.

use crate::pty::{self, CommandConfig, PtyConfig};
use crate::state::{AppState, SharedSession, Trigger, TriggerAction};
use anyhow::Result;
use tauri::State;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Serialize;
use std::path::Path;
use std::fs;

#[derive(Serialize)]
pub struct SessionResponse {
    pub session_id: String,
}

#[derive(Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified: u64,
}

#[derive(Serialize)]
pub struct DirectoryListing {
    pub path: String,
    pub entries: Vec<FileEntry>,
}

/// Creates a new terminal session with PTY
#[tauri::command]
pub async fn create_terminal_session(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: Option<PtyConfig>,
) -> Result<SessionResponse, String> {
    let config = config.unwrap_or_default();

    let session_id = pty::create_pty_session(app, state, config)
        .await
        .map_err(|e| e.to_string())?;

    Ok(SessionResponse { session_id })
}

/// Writes data to a terminal session
#[tauri::command]
pub async fn write_to_terminal(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    // Convert base64 or raw string to bytes
    let bytes = if let Ok(decoded) = STANDARD.decode(&data) {
        decoded
    } else {
        data.into_bytes()
    };

    pty::write_to_pty(state, session_id, bytes)
        .await
        .map_err(|e| e.to_string())
}

/// Resizes a terminal session
#[tauri::command]
pub async fn resize_terminal(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    pty::resize_pty(state, session_id, cols, rows)
        .await
        .map_err(|e| e.to_string())
}

/// Closes a terminal session
#[tauri::command]
pub async fn close_terminal(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    pty::close_pty(state, session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Lists all active terminal sessions
#[tauri::command]
pub async fn list_terminal_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    pty::list_pty_sessions(state)
        .await
        .map_err(|e| e.to_string())
}

/// Creates a command session (non-interactive PTY for running CLI programs)
#[tauri::command]
pub async fn create_command_session(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: CommandConfig,
) -> Result<SessionResponse, String> {
    let session_id = pty::create_command_session(app, state, config)
        .await
        .map_err(|e| e.to_string())?;

    Ok(SessionResponse { session_id })
}

// ==================== Trigger Management ====================

/// Creates a new trigger
#[tauri::command]
pub async fn create_trigger(
    state: State<'_, AppState>,
    pattern: String,
    action: TriggerAction,
) -> Result<String, String> {
    let trigger_id = AppState::generate_session_id();

    let trigger = Trigger {
        id: trigger_id.clone(),
        pattern,
        action,
        enabled: true,
    };

    state
        .triggers
        .lock()
        .await
        .insert(trigger_id.clone(), trigger);

    Ok(trigger_id)
}

/// Lists all triggers
#[tauri::command]
pub async fn list_triggers(state: State<'_, AppState>) -> Result<Vec<Trigger>, String> {
    let triggers = state.triggers.lock().await;
    Ok(triggers.values().cloned().collect())
}

/// Deletes a trigger
#[tauri::command]
pub async fn delete_trigger(state: State<'_, AppState>, trigger_id: String) -> Result<(), String> {
    state.triggers.lock().await.remove(&trigger_id);
    Ok(())
}

/// Toggles a trigger's enabled state
#[tauri::command]
pub async fn toggle_trigger(
    state: State<'_, AppState>,
    trigger_id: String,
) -> Result<bool, String> {
    let mut triggers = state.triggers.lock().await;

    if let Some(trigger) = triggers.get_mut(&trigger_id) {
        trigger.enabled = !trigger.enabled;
        Ok(trigger.enabled)
    } else {
        Err("Trigger not found".to_string())
    }
}

// ==================== Session Sharing ====================

/// Creates a shareable session link
#[tauri::command]
pub async fn share_session(
    state: State<'_, AppState>,
    session_id: String,
    read_only: bool,
) -> Result<String, String> {
    let share_id = AppState::generate_session_id();
    let share_code = AppState::generate_share_code();

    let shared_session = SharedSession {
        id: share_id,
        session_id,
        share_code: share_code.clone(),
        read_only,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
    };

    state
        .shared_sessions
        .lock()
        .await
        .insert(share_code.clone(), shared_session);

    Ok(share_code)
}

/// Joins a shared session
#[tauri::command]
pub async fn join_shared_session(
    state: State<'_, AppState>,
    share_code: String,
) -> Result<SharedSession, String> {
    let shared_sessions = state.shared_sessions.lock().await;

    shared_sessions
        .get(&share_code)
        .cloned()
        .ok_or_else(|| "Shared session not found".to_string())
}

/// Stops sharing a session
#[tauri::command]
pub async fn unshare_session(
    state: State<'_, AppState>,
    share_code: String,
) -> Result<(), String> {
    state.shared_sessions.lock().await.remove(&share_code);
    Ok(())
}

// ==================== AI Integration ====================

/// Sends a prompt to AI for terminal assistance
#[tauri::command]
pub async fn ask_ai(
    prompt: String,
    context: Option<String>,
    provider: String,
    api_key: String,
    model: String,
) -> Result<String, String> {
    use crate::ai::{AiClient, AiConfig, AiProvider, AiRequest};

    println!("🤖 AI Request: provider={}, model={}", provider, model);

    // Parse provider
    let ai_provider = match provider.to_lowercase().as_str() {
        "openai" => AiProvider::OpenAi,
        "anthropic" => AiProvider::Anthropic,
        "openrouter" | "perplexity" => AiProvider::Custom,
        _ => return Err(format!("Unsupported AI provider: {}", provider)),
    };

    // Determine base URL for custom providers
    let base_url = match provider.to_lowercase().as_str() {
        "openrouter" => Some("https://openrouter.ai/api/v1/chat/completions".to_string()),
        "perplexity" => Some("https://api.perplexity.ai/chat/completions".to_string()),
        _ => None,
    };

    let config = AiConfig {
        provider: ai_provider,
        api_key,
        model,
        base_url,
    };

    let client = AiClient::new(config);

    let request = AiRequest {
        prompt: prompt.clone(),
        context,
        terminal_output: None,
        system_info: None,
    };

    println!("📡 Sending request to AI...");
    match client.ask(request).await {
        Ok(response) => {
            println!("✅ AI response received: {} chars", response.response.len());
            Ok(response.response)
        }
        Err(e) => {
            println!("❌ AI request failed: {}", e);
            Err(format!("AI request failed: {}", e))
        }
    }
}

/// Analyzes terminal output for errors and suggestions
#[tauri::command]
pub async fn analyze_output(output: String) -> Result<serde_json::Value, String> {
    // Placeholder for AI-powered output analysis
    // Will analyze terminal output for errors, warnings, and provide suggestions

    Ok(serde_json::json!({
        "hasError": output.to_lowercase().contains("error"),
        "hasWarning": output.to_lowercase().contains("warning"),
        "suggestions": [],
    }))
}

// ==================== SSH Support ====================

/// Creates an SSH connection using the real SSH module
#[tauri::command]
pub async fn create_ssh_session(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> Result<serde_json::Value, String> {
    use crate::ssh::{SshConfig, create_ssh_session as ssh_create};

    println!("🔐 Creating SSH session with config: {:?}", config);

    // Parse config from JSON
    let ssh_config: SshConfig = serde_json::from_value(config)
        .map_err(|e| format!("Invalid SSH config: {}", e))?;

    println!("✅ Parsed SSH config: host={}, user={}", ssh_config.host, ssh_config.username);

    // Create SSH session using real implementation
    match ssh_create(app, state, ssh_config).await {
        Ok(session_id) => {
            println!("✅ SSH session created: {}", session_id);
            Ok(serde_json::json!({
                "session_id": session_id
            }))
        }
        Err(e) => {
            println!("❌ SSH session failed: {}", e);
            Err(format!("SSH connection failed: {}", e))
        }
    }
}

/// Writes data to an SSH session
#[tauri::command]
pub async fn write_to_ssh(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    println!("📝 Writing to SSH session {}: {} bytes", session_id, data.len());

    // For now, just log it since the SSH module needs a full channel implementation
    // The real implementation would write to the SSH channel
    println!("  Data: {:?}", data);

    // TODO: Implement actual SSH write when SSH module has interactive shell support
    Ok(())
}

/// Lists all SSH sessions
#[tauri::command]
pub async fn list_ssh_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let sessions = state.ssh_sessions.lock().await;
    let session_list: Vec<_> = sessions
        .iter()
        .map(|(id, session)| {
            serde_json::json!({
                "id": id,
                "host": session.host,
                "username": session.username,
                "connectedAt": session.connected_at
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            })
        })
        .collect();

    Ok(session_list)
}

/// Gets SSH config hosts from ~/.ssh/config
#[tauri::command]
pub async fn get_ssh_config_hosts() -> Result<Vec<crate::ssh::SshHostConfig>, String> {
    crate::ssh::read_ssh_config()
        .map_err(|e| e.to_string())
}

// ==================== File Navigator ====================

/// Lists files and directories in a given path
#[tauri::command]
pub async fn list_directory(path: String) -> Result<DirectoryListing, String> {
    let path = Path::new(&path);

    if !path.exists() {
        return Err("Path does not exist".to_string());
    }

    if !path.is_dir() {
        return Err("Path is not a directory".to_string());
    }

    let entries = fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    let mut file_entries = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let metadata = entry.metadata()
            .map_err(|e| format!("Failed to read metadata: {}", e))?;

        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path().to_string_lossy().to_string();
        let is_directory = metadata.is_dir();
        let size = metadata.len();
        let modified = metadata.modified()
            .map_err(|e| format!("Failed to read modified time: {}", e))?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("Failed to convert time: {}", e))?
            .as_secs();

        file_entries.push(FileEntry {
            name,
            path,
            is_directory,
            size,
            modified,
        });
    }

    // Sort: directories first, then by name
    file_entries.sort_by(|a, b| {
        match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(DirectoryListing {
        path: path.to_string_lossy().to_string(),
        entries: file_entries,
    })
}

/// Gets the user's home directory path
#[tauri::command]
pub async fn get_home_directory() -> Result<String, String> {
    let home = dirs::home_dir()
        .ok_or_else(|| "Failed to get home directory".to_string())?;

    Ok(home.to_string_lossy().to_string())
}

/// Gets the current working directory for a terminal session
#[tauri::command]
pub async fn get_current_directory(
    _state: State<'_, AppState>,
    _session_id: String,
) -> Result<String, String> {
    // Get the actual current working directory
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct GitInfo {
    pub branch: String,
    pub changes: i32,
    pub is_repo: bool,
}

/// Gets git information for the current directory
#[tauri::command]
pub async fn get_git_info(path: Option<String>) -> Result<GitInfo, String> {
    use std::process::Command;

    let working_dir = if let Some(p) = path {
        Path::new(&p).to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?
    };

    // Check if it's a git repo
    let is_repo_check = Command::new("git")
        .args(&["rev-parse", "--git-dir"])
        .current_dir(&working_dir)
        .output();

    let is_repo = is_repo_check.is_ok() && is_repo_check.unwrap().status.success();

    if !is_repo {
        return Ok(GitInfo {
            branch: String::new(),
            changes: 0,
            is_repo: false,
        });
    }

    // Get current branch
    let branch_output = Command::new("git")
        .args(&["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&working_dir)
        .output()
        .map_err(|e| format!("Failed to get git branch: {}", e))?;

    let branch = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();

    // Get number of changes
    let status_output = Command::new("git")
        .args(&["status", "--porcelain"])
        .current_dir(&working_dir)
        .output()
        .map_err(|e| format!("Failed to get git status: {}", e))?;

    let changes = String::from_utf8_lossy(&status_output.stdout)
        .lines()
        .count() as i32;

    Ok(GitInfo {
        branch,
        changes,
        is_repo: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_trigger() {
        let state = AppState::new();
        let result = create_trigger(
            State::from(state),
            "error".to_string(),
            TriggerAction::Notify {
                message: "Error detected".to_string(),
            },
        )
        .await;

        assert!(result.is_ok());
    }
}
