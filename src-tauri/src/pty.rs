// ABOUTME: PTY (Pseudo-terminal) session management using portable-pty crate.
// ABOUTME: Handles creation, I/O, resizing, and lifecycle of terminal sessions with async event emission to frontend.

use crate::state::{AppState, PtySession};
use crate::status;
use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
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
    /// When set, run this argv directly instead of an interactive shell.
    /// Used by the agent launcher (see agents.rs) — first element is the
    /// program, remaining elements are arguments.
    pub command: Option<Vec<String>>,
    /// Stable Zellij session name for this tab (XNAUT-66).
    ///
    /// When set — and Zellij is installed — the tab is backed by a Zellij
    /// session instead of a bare shell, so the work survives the app: quit,
    /// close the lid, or reopen and the tab reattaches to the same running
    /// session. `zellij attach <name>` from any other terminal, including over
    /// SSH, lands in the same place.
    ///
    /// Absent (or no Zellij) falls back to a plain interactive shell, which is
    /// the previous behaviour.
    #[serde(default)]
    pub session_name: Option<String>,
}

impl Default for PtyConfig {
    fn default() -> Self {
        Self {
            shell: None,
            working_dir: None,
            env: None,
            cols: 80,
            rows: 24,
            command: None,
            session_name: None,
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

    // Two modes: direct argv (used by agent launcher) or interactive shell.
    // When config.command is provided, skip the shell entirely so we can run
    // `claude`, `codex`, etc. as PID 1 of the PTY.
    let (mut cmd, shell) = if let Some(argv) = config.command.as_ref().filter(|v| !v.is_empty()) {
        let mut c = CommandBuilder::new(&argv[0]);
        if argv.len() > 1 {
            c.args(&argv[1..]);
        }
        (c, argv[0].clone())
    } else {
        let shell = config.shell.unwrap_or_else(|| {
            #[cfg(target_os = "windows")]
            return "powershell.exe".to_string();
            #[cfg(not(target_os = "windows"))]
            return std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        });
        let mut c = CommandBuilder::new(&shell);
        #[cfg(not(target_os = "windows"))]
        {
            // A named tab runs inside Zellij so it outlives the app (XNAUT-66).
            // Launched through the LOGIN shell (-lc) on purpose: zellij lives in
            // ~/.local/bin or /opt/homebrew/bin, which a bare CommandBuilder
            // does not have on PATH.
            let zellij_cmd = config
                .session_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .filter(|_| crate::zellij::is_installed())
                .map(|name| crate::zellij::launch_command(name, None));
            match zellij_cmd {
                Some(launch) => c.args(vec!["-lc", &launch]),
                None if shell.contains("bash") || shell.contains("zsh") || shell.contains("fish") => {
                    c.args(vec!["-i", "-l"])
                }
                None => {}
            }
        }
        (c, shell)
    };

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

    // Route AI traffic through ClawProxy if running (privacy monitor)
    // Check if ClawProxy is available on port 8099
    if std::net::TcpStream::connect("127.0.0.1:8099").is_ok() {
        cmd.env("OPENAI_API_BASE", "http://localhost:8099/v1");
        cmd.env("OPENAI_BASE_URL", "http://localhost:8099/v1");
        cmd.env("ANTHROPIC_BASE_URL", "http://localhost:8099/v1");
    }

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

    // Directory tracking is handled by polling via get_current_directory (lsof)
    // No shell hook injection needed — avoids echoed commands and path flashing

    Ok(session_id)
}

/// Spawns async tasks to read PTY output and emit it to the frontend,
/// COALESCED to ~60 events/sec.
///
/// Why: every `emit` becomes a RunJavaScript IPC into the WKWebView, and WebKit
/// re-aligns every live DOM timer per evaluated script (UserGestureIndicator →
/// didChangeTimerAlignmentInterval). A streaming agent produces hundreds of PTY
/// reads per second; emitting one event per read saturates the WebContent main
/// thread until the whole UI stops reacting to clicks — the long-running-app
/// freeze (#54), confirmed live by `sample` on a frozen instance (2026-07-25).
/// Reads append to a pending buffer; a 16 ms flusher emits one merged event.
fn spawn_pty_reader(app: AppHandle, session_id: String, session: Arc<PtySession>) {
    use std::sync::atomic::{AtomicBool, Ordering};
    let pending: Arc<std::sync::Mutex<Vec<u8>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let closed = Arc::new(AtomicBool::new(false));
    let exit_code = Arc::new(std::sync::Mutex::new(-1i64));

    // Flusher: drains the pending buffer every 16 ms; after the reader closes,
    // performs the final drain and then emits terminal-closed (ordering is
    // preserved: closed never overtakes the last output).
    {
        let app = app.clone();
        let session_id = session_id.clone();
        let pending = Arc::clone(&pending);
        let closed = Arc::clone(&closed);
        let exit_code = Arc::clone(&exit_code);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                let data = {
                    let mut p = pending.lock().unwrap();
                    std::mem::take(&mut *p)
                };
                if !data.is_empty() {
                    let base64_data = STANDARD.encode(&data);
                    let _ = app.emit(
                        &format!("terminal-output:{}", session_id),
                        serde_json::json!({
                            "sessionId": session_id,
                            "data": base64_data,
                        }),
                    );
                    // Status ping once per flush (was once per read).
                    if let Some(state) = app.try_state::<AppState>() {
                        status::ping_session_output(&state.agent_sessions, &app, &session_id).await;
                    }
                } else if closed.load(Ordering::Acquire) {
                    let code = *exit_code.lock().unwrap();
                    let _ = app.emit(
                        &format!("terminal-closed:{}", session_id),
                        serde_json::json!({
                            "sessionId": session_id,
                            "exitCode": code,
                        }),
                    );
                    // Notify the status tracker — no-op if this wasn't an agent session.
                    if let Some(state) = app.try_state::<AppState>() {
                        status::mark_session_done(&state.agent_sessions, &app, &session_id).await;
                    }
                    break;
                }
            }
        });
    }

    // The READER runs on a dedicated OS thread, never the tokio pool: `reader.read()`
    // blocks until the shell produces output, parking one async worker per open
    // session. A few tabs starve the runtime and that shows up as multi-second
    // keystroke stalls (b528872). This is a SECOND, independent fix from the
    // coalescing flusher above — that one solves the emit flood (#54), this one
    // solves runtime starvation. A release needs both.
    std::thread::spawn(move || {
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
                    let code = {
                        let mut child = tauri::async_runtime::block_on(session.child.lock());
                        match child.try_wait() {
                            Ok(Some(status)) => status.exit_code() as i64,
                            Ok(None) => match child.wait() {
                                Ok(status) => status.exit_code() as i64,
                                Err(_) => -1,
                            },
                            Err(_) => -1,
                        }
                    };
                    // Dropping the tap drops its broadcast sender, which closes
                    // any attached mobile websocket (XNAUT-32).
                    if let Some(state) = app.try_state::<AppState>() {
                        tauri::async_runtime::block_on(async {
                            state.mobile_taps.lock().await.remove(&session_id);
                        });
                    }
                    *exit_code.lock().unwrap() = code;
                    closed.store(true, Ordering::Release);
                    break;
                }
                Ok(n) => {
                    pending.lock().unwrap().extend_from_slice(&buffer[..n]);
                    // The mobile tee stays PER-READ: the phone mirrors the raw
                    // stream and is not subject to the 16 ms UI coalescing.
                    if let Some(state) = app.try_state::<AppState>() {
                        tauri::async_runtime::block_on(async {
                            if let Some(tap) = state.mobile_taps.lock().await.get_mut(&session_id) {
                                tap.push(&buffer[..n]);
                            }
                        });
                    }
                }
                Err(e) => {
                    eprintln!("Error reading PTY output: {}", e);
                    closed.store(true, Ordering::Release);
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
    // Resolved up front: config is partially moved further down.
    let attach_target = zellij_attach_target(&config);

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

    // A zellij attach IS an agent session — register it so the existing status
    // machinery applies. ping_session_output (below, on every output frame) is a
    // no-op for unregistered sessions, which is why an attached agent showed no
    // state at all: it was never in the registry, so nothing could report on it.
    // Registering means output drives Working, the decay task drops it to Idle,
    // and the hooks can raise Permission/Blocked — all without a second
    // mechanism.
    if let Some(sess) = attach_target {
        let agent_id = match sess.split('-').next() {
            Some("cl") => "claude",
            Some("cx") => "codex",
            _ => "agent",
        };
        crate::status::register_agent_session(
            &state.agent_sessions,
            &app,
            &session_id,
            agent_id,
            &sess,
        )
        .await;
    }

    // Start reading output
    spawn_pty_reader(app, session_id.clone(), session.clone());

    Ok(session_id)
}

/// The session name from a `zellij attach [--create] <name>` command, if that is
/// what this config runs. Used to tell an attached agent apart from an ordinary
/// one-off command, so only the former lands in the agent registry.
fn zellij_attach_target(config: &CommandConfig) -> Option<String> {
    let args = config.args.as_ref()?;
    let joined = args.join(" ");
    let idx = joined.find("zellij attach")?;
    let rest = joined[idx + "zellij attach".len()..].trim_start();
    let rest = rest.strip_prefix("--create").unwrap_or(rest).trim_start();
    let name = rest
        .split_whitespace()
        .next()?
        .trim_matches(|c| c == '\'' || c == '"');
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
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
