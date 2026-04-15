// ABOUTME: PTY (Pseudo-terminal) session management using portable-pty crate.
// ABOUTME: Handles creation, I/O, resizing, and lifecycle of terminal sessions with async event emission to frontend.

use crate::state::{AppState, PtySession};
use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

/// Configuration for creating a new PTY session
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PtyConfig {
    pub shell: Option<String>,
    pub working_dir: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub cols: u16,
    pub rows: u16,
}

impl Default for PtyConfig {
    fn default() -> Self {
        Self {
            shell: None,
            working_dir: None,
            env: None,
            cols: 80,
            rows: 24,
        }
    }
}

/// Creates a new PTY session and starts reading output
pub async fn create_pty_session(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    config: PtyConfig,
) -> Result<String> {
    let session_id = AppState::generate_session_id();

    // Create PTY with specified size
    let pty_system = NativePtySystem::default();
    let pty_size = PtySize {
        rows: config.rows,
        cols: config.cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    let pty_pair = pty_system
        .openpty(pty_size)
        .context("Failed to create PTY")?;

    // Determine shell to use
    let shell = config.shell.unwrap_or_else(|| {
        #[cfg(target_os = "windows")]
        return "powershell.exe".to_string();
        #[cfg(not(target_os = "windows"))]
        return std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    });

    // Build command - make it interactive and login shell
    let mut cmd = CommandBuilder::new(&shell);

    // Add interactive flags for common shells
    #[cfg(not(target_os = "windows"))]
    {
        if shell.contains("bash") || shell.contains("zsh") || shell.contains("fish") {
            cmd.args(vec!["-i", "-l"]);
        }
    }

    if let Some(cwd) = config.working_dir {
        // Expand ~ in working directory
        let expanded = if cwd.starts_with("~/") {
            dirs::home_dir()
                .map(|h| h.join(&cwd[2..]).to_string_lossy().to_string())
                .unwrap_or(cwd)
        } else {
            cwd
        };
        cmd.cwd(expanded);
    }

    // Remove env vars that prevent tools from running inside xNAUT
    // (e.g. CLAUDECODE causes Claude Code to think it's a nested session)
    cmd.env_remove("CLAUDECODE");

    // Set essential environment variables for proper terminal functionality
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    // Force color output for common tools
    cmd.env("CLICOLOR", "1");
    cmd.env("CLICOLOR_FORCE", "1");
    cmd.env("FORCE_COLOR", "1");

    // Disable Oh My Zsh auto-updates in PTY sessions
    cmd.env("DISABLE_AUTO_UPDATE", "true");

    // Pass through important environment variables from parent process
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    if let Ok(user) = std::env::var("USER") {
        cmd.env("USER", user);
    }
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    // Always ensure UTF-8 locale — critical for Unicode rendering (box-drawing, symbols)
    // Fall back to en_US.UTF-8 if parent process has no LANG (e.g. launched from Finder)
    let lang = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".to_string());
    cmd.env("LANG", &lang);
    cmd.env("LC_ALL", &lang);

    // Pass through ZSH-specific environment variables
    if let Ok(zdotdir) = std::env::var("ZDOTDIR") {
        cmd.env("ZDOTDIR", zdotdir);
    }
    if let Ok(zsh) = std::env::var("ZSH") {
        cmd.env("ZSH", zsh);
    }
    if let Ok(zsh_theme) = std::env::var("ZSH_THEME") {
        cmd.env("ZSH_THEME", zsh_theme);
    }

    // Add directory reporting via OSC sequences for shells
    // This helps track current directory changes
    if shell.contains("zsh") {
        // For ZSH, add precmd hook to report directory
        cmd.env("XNAUT_SHELL_INTEGRATION", "1");
    } else if shell.contains("bash") {
        // For Bash, use PROMPT_COMMAND
        cmd.env("XNAUT_SHELL_INTEGRATION", "1");
    }

    if let Some(env) = config.env {
        for (key, value) in env {
            cmd.env(key, value);
        }
    }

    // Spawn child process
    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .context("Failed to spawn shell process")?;

    // Get reader and writer upfront before wrapping pty_pair
    let reader = pty_pair
        .master
        .try_clone_reader()
        .context("Failed to clone reader")?;
    let writer = pty_pair
        .master
        .take_writer()
        .context("Failed to get writer")?;

    // Create session
    let session = Arc::new(PtySession {
        _id: session_id.clone(),
        pty_pair: Arc::new(Mutex::new(pty_pair)),
        child: Arc::new(Mutex::new(child)),
        reader: Arc::new(std::sync::Mutex::new(Box::new(reader))),
        writer: Arc::new(std::sync::Mutex::new(writer)),
        created_at: std::time::SystemTime::now(),
    });

    // Store session in state
    state
        .pty_sessions
        .lock()
        .await
        .insert(session_id.clone(), session.clone());

    // Start reading PTY output in background task
    spawn_pty_reader(app, session_id.clone(), session.clone());

    // Shell integration: OSC 7 directory tracking (emit CWD on each prompt)
    let shell_for_hooks = shell.clone();
    let session_for_hooks = session.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let hooks = if shell_for_hooks.contains("zsh") {
            Some(" precmd() { printf '\\e]7;file://%s%s\\a' \"${HOST}\" \"${PWD}\"; }\n")
        } else if shell_for_hooks.contains("bash") {
            Some(" PROMPT_COMMAND='printf \"\\e]7;file://%s%s\\a\" \"$HOSTNAME\" \"$PWD\"'\n")
        } else {
            None
        };
        if let Some(hook_cmd) = hooks {
            if let Ok(mut w) = session_for_hooks.writer.lock() {
                let _ = w.write_all(hook_cmd.as_bytes());
            }
        }
    });

    Ok(session_id)
}

/// Spawns async task to read PTY output and emit to frontend
fn spawn_pty_reader(app: AppHandle, session_id: String, session: Arc<PtySession>) {
    tokio::spawn(async move {
        let mut buffer = [0u8; 8192];
        loop {
            // Use the stored reader
            let read_result = {
                let mut reader = session.reader.lock().unwrap();
                reader.read(&mut buffer)
            };

            match read_result {
                Ok(0) => {
                    // EOF reached, session ended — capture exit code
                    let exit_code = {
                        let mut child = session.child.lock().await;
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                // Process already exited
                                status.exit_code() as i64
                            }
                            Ok(None) => {
                                // Process still running, wait briefly
                                match child.wait() {
                                    Ok(status) => status.exit_code() as i64,
                                    Err(_) => -1,
                                }
                            }
                            Err(_) => -1,
                        }
                    };
                    let _ = app.emit(
                        &format!("terminal-closed:{}", session_id),
                        serde_json::json!({
                            "sessionId": session_id,
                            "exitCode": exit_code,
                        }),
                    );
                    break;
                }
                Ok(n) => {
                    // Emit output to frontend
                    let data = &buffer[..n];
                    let base64_data = STANDARD.encode(data);

                    let _ = app.emit(
                        &format!("terminal-output:{}", session_id),
                        serde_json::json!({
                            "sessionId": session_id,
                            "data": base64_data,
                        }),
                    );

                    // Process output for triggers (convert to UTF-8 for pattern matching)
                    if let Ok(_text) = String::from_utf8(data.to_vec()) {
                        // Trigger processing happens in background, don't await
                        // to avoid blocking PTY output
                        // Note: trigger integration would need state access here
                    }
                }
                Err(e) => {
                    eprintln!("Error reading PTY output: {}", e);
                    break;
                }
            }
        }
    });
}

/// Writes data to PTY session
pub async fn write_to_pty(
    state: tauri::State<'_, AppState>,
    session_id: String,
    data: Vec<u8>,
) -> Result<()> {
    let sessions = state.pty_sessions.lock().await;
    let session = sessions.get(&session_id).context("PTY session not found")?;

    // Use the stored writer
    let mut writer = session.writer.lock().unwrap();
    writer.write_all(&data).context("Failed to write to PTY")?;
    writer.flush().context("Failed to flush PTY")?;

    Ok(())
}

/// Resizes a PTY session
pub async fn resize_pty(
    state: tauri::State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let sessions = state.pty_sessions.lock().await;
    let session = sessions.get(&session_id).context("PTY session not found")?;

    let pty_pair = session.pty_pair.lock().await;
    let new_size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    pty_pair
        .master
        .resize(new_size)
        .context("Failed to resize PTY")?;

    Ok(())
}

/// Closes a PTY session
pub async fn close_pty(state: tauri::State<'_, AppState>, session_id: String) -> Result<()> {
    let mut sessions = state.pty_sessions.lock().await;

    if let Some(session) = sessions.remove(&session_id) {
        // Kill child process
        let mut child = session.child.lock().await;
        let _ = child.kill();
        Ok(())
    } else {
        Err(anyhow::anyhow!("PTY session not found"))
    }
}

/// Lists all active PTY sessions
pub async fn list_pty_sessions(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<serde_json::Value>> {
    let sessions = state.pty_sessions.lock().await;
    let session_list: Vec<_> = sessions
        .iter()
        .map(|(id, session)| {
            serde_json::json!({
                "id": id,
                "createdAt": session.created_at
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            })
        })
        .collect();

    Ok(session_list)
}

/// Configuration for creating a command session (non-interactive)
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandConfig {
    pub program: String,
    pub args: Option<Vec<String>>,
    pub working_dir: String,
    pub env: Option<HashMap<String, String>>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

/// Creates a command session that runs a specific program (not an interactive shell).
/// Used by the Ralph orchestrator to run AI CLIs like `claude --print ...` in a PTY.
pub async fn create_command_session(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    config: CommandConfig,
) -> Result<String> {
    let session_id = AppState::generate_session_id();

    let cols = config.cols.unwrap_or(120);
    let rows = config.rows.unwrap_or(40);

    // Create PTY with specified size
    let pty_system = NativePtySystem::default();
    let pty_size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    let pty_pair = pty_system
        .openpty(pty_size)
        .context("Failed to create PTY")?;

    // Build command — spawn program directly, not an interactive shell
    let mut cmd = CommandBuilder::new(&config.program);

    if let Some(args) = &config.args {
        cmd.args(args.iter().map(|s| s.as_str()).collect::<Vec<&str>>());
    }

    // Expand ~ in working directory
    let working_dir = if config.working_dir.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            home.join(&config.working_dir[2..])
                .to_string_lossy()
                .to_string()
        } else {
            config.working_dir.clone()
        }
    } else {
        config.working_dir.clone()
    };
    cmd.cwd(&working_dir);

    // Remove env vars that prevent tools from running inside xNAUT
    cmd.env_remove("CLAUDECODE");

    // Set essential environment variables
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("CLICOLOR", "1");
    cmd.env("CLICOLOR_FORCE", "1");
    cmd.env("FORCE_COLOR", "1");

    // Pass through important env vars from parent
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    if let Ok(user) = std::env::var("USER") {
        cmd.env("USER", user);
    }
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    let lang = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".to_string());
    cmd.env("LANG", &lang);
    cmd.env("LC_ALL", &lang);

    // Apply custom environment variables
    if let Some(env) = config.env {
        for (key, value) in env {
            cmd.env(key, value);
        }
    }

    // Spawn child process
    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .context("Failed to spawn command process")?;

    // Get reader and writer
    let reader = pty_pair
        .master
        .try_clone_reader()
        .context("Failed to clone reader")?;
    let writer = pty_pair
        .master
        .take_writer()
        .context("Failed to get writer")?;

    // Create session
    let session = Arc::new(PtySession {
        _id: session_id.clone(),
        pty_pair: Arc::new(Mutex::new(pty_pair)),
        child: Arc::new(Mutex::new(child)),
        reader: Arc::new(std::sync::Mutex::new(Box::new(reader))),
        writer: Arc::new(std::sync::Mutex::new(writer)),
        created_at: std::time::SystemTime::now(),
    });

    // Store session
    state
        .pty_sessions
        .lock()
        .await
        .insert(session_id.clone(), session.clone());

    // Start reading output
    spawn_pty_reader(app, session_id.clone(), session.clone());

    Ok(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_config_default() {
        let config = PtyConfig::default();
        assert_eq!(config.cols, 80);
        assert_eq!(config.rows, 24);
        assert!(config.shell.is_none());
    }

    #[test]
    fn test_command_config_defaults() {
        let config = CommandConfig {
            program: "echo".to_string(),
            args: Some(vec!["hello".to_string()]),
            working_dir: "/tmp".to_string(),
            env: None,
            cols: None,
            rows: None,
        };
        assert_eq!(config.program, "echo");
        assert!(config.cols.is_none());
    }
}
