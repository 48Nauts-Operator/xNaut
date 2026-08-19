// ABOUTME: SSH connection management using ssh2 crate for remote terminal sessions.
// ABOUTME: Supports password and key-based authentication with session lifecycle management.

use crate::state::{AppState, SshSession};
use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ssh2::{Channel, Session};
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// How long the reader waits between drains.
///
/// 16 ms is the PTY flusher's interval, chosen for the same reason: every emit
/// is a RunJavaScript IPC into the webview, and one event per read is what
/// saturated the WebContent thread in #54. Draining everything available per
/// tick into one event gives the same coalescing without a second task.
const READ_POLL: Duration = Duration::from_millis(16);

/// How long a single non-blocking call may spin on EAGAIN before it is an error.
/// Keystrokes are a handful of bytes, so hitting this means the link is gone.
const IO_BUDGET: Duration = Duration::from_secs(2);

/// Configuration for SSH connection
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub key_passphrase: Option<String>,
}

fn default_port() -> u16 {
    22
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: default_port(),
            username: String::new(),
            password: None,
            key_path: None,
            key_passphrase: None,
        }
    }
}

/// Which credential a profile actually carries.
#[derive(Debug, PartialEq)]
pub enum Auth<'a> {
    Password(&'a str),
    Key {
        path: &'a str,
        passphrase: Option<&'a str>,
    },
}

/// Picks the credential to authenticate with.
///
/// Pulled out of the connect path because this is where key auth died: the
/// profile editor sent `privateKey`, this struct read `key_path`, serde
/// dropped the unknown field, and every key profile fell out of the bottom as
/// "No authentication method provided" (XNAUT-200). A pure function can be
/// tested against the exact JSON the editor sends; a connect cannot.
pub fn auth_method(config: &SshConfig) -> Result<Auth<'_>> {
    if let Some(password) = config.password.as_deref().filter(|p| !p.is_empty()) {
        return Ok(Auth::Password(password));
    }
    if let Some(path) = config.key_path.as_deref().filter(|p| !p.trim().is_empty()) {
        return Ok(Auth::Key {
            path,
            passphrase: config.key_passphrase.as_deref().filter(|p| !p.is_empty()),
        });
    }
    Err(anyhow::anyhow!(
        "No authentication method provided (password or key required)"
    ))
}

/// Represents a parsed SSH host configuration
#[derive(Debug, Clone, serde::Serialize)]
pub struct SshHostConfig {
    pub name: String,             // Host alias from config
    pub hostname: Option<String>, // Actual hostname/IP
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

/// Retries one non-blocking libssh2 call until it lands or the budget runs out.
///
/// The session is non-blocking so the reader cannot wedge the writer, and the
/// price of that is EAGAIN on every call, not just reads.
fn retry_would_block<T>(budget: Duration, mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let start = Instant::now();
    loop {
        match op() {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if start.elapsed() >= budget {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "SSH channel stayed blocked",
                    ));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            other => return other,
        }
    }
}

/// Writes every byte, tolerating short writes and EAGAIN.
///
/// `write_all` is not usable here: it discards how far it got when the call
/// fails, so one EAGAIN halfway through a paste would resend from the start.
fn write_all_retrying<W: Write>(sink: &mut W, data: &[u8], budget: Duration) -> io::Result<()> {
    let mut rest = data;
    while !rest.is_empty() {
        let written = retry_would_block(budget, || sink.write(rest))?;
        if written == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "SSH channel accepted no bytes",
            ));
        }
        rest = &rest[written..];
    }
    Ok(())
}

/// Expands a leading `~/` so a hand-typed key path works.
/// Paths coming from ~/.ssh/config are already absolute (see parse_ssh_config).
fn expand_tilde(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => dirs::home_dir()
            .map(|home| home.join(rest))
            .unwrap_or_else(|| PathBuf::from(path)),
        None => PathBuf::from(path),
    }
}

/// Connects, authenticates and opens an interactive shell channel.
///
/// Only the channel comes back: every ssh2 handle shares one reference-counted
/// session inner, so holding the channel holds the connection open. The old
/// code held the session in a struct that was bound to `_ssh_handle` and
/// dropped at the end of the statement, which is why nothing could be typed.
fn connect_shell(config: &SshConfig, cols: u16, rows: u16) -> Result<Channel> {
    let tcp = TcpStream::connect((config.host.as_str(), config.port))
        .context("Failed to connect to SSH server")?;
    // The session takes ownership of one dup of the socket. This second handle
    // exists only to flip O_NONBLOCK below, which dup'd descriptors share.
    let socket = tcp.try_clone().context("Failed to duplicate SSH socket")?;

    let mut session = Session::new().context("Failed to create SSH session")?;
    session.set_tcp_stream(tcp);
    session.handshake().context("SSH handshake failed")?;

    match auth_method(config)? {
        Auth::Password(password) => session
            .userauth_password(&config.username, password)
            .context("SSH password authentication failed")?,
        Auth::Key { path, passphrase } => session
            .userauth_pubkey_file(&config.username, None, &expand_tilde(path), passphrase)
            .context("SSH key authentication failed")?,
    }

    if !session.authenticated() {
        return Err(anyhow::anyhow!("SSH authentication failed"));
    }

    let mut channel = session
        .channel_session()
        .context("Failed to open SSH channel")?;
    channel
        .request_pty(
            "xterm-256color",
            None,
            Some((cols as u32, rows as u32, 0, 0)),
        )
        .context("Failed to request a remote PTY")?;
    channel.shell().context("Failed to start the remote shell")?;

    // Non-blocking from here on. In blocking mode the reader would sit inside
    // recv() holding the session mutex, so a keystroke could not be written
    // until the server happened to say something first.
    socket
        .set_nonblocking(true)
        .context("Failed to set the SSH socket non-blocking")?;
    session.set_blocking(false);

    Ok(channel)
}

/// Streams channel output to the frontend as `ssh-output-<id>`, mirroring the
/// `terminal-output:<id>` contract in pty.rs (base64 so multi-byte UTF-8 and
/// raw escape bytes survive the JSON hop).
///
/// Its own OS thread, never the tokio pool: the lock is a std mutex held across
/// libssh2 calls, and parking async workers on terminal I/O is what caused the
/// multi-second keystroke stalls fixed in b528872.
fn spawn_ssh_reader(app: AppHandle, session_id: String, channel: Arc<Mutex<Channel>>) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            let mut pending: Vec<u8> = Vec::new();
            let mut closed = false;
            loop {
                let read = channel.lock().unwrap().read(&mut buffer);
                match read {
                    Ok(0) => {
                        closed = true;
                        break;
                    }
                    Ok(n) => pending.extend_from_slice(&buffer[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        eprintln!("SSH read failed on {session_id}: {e}");
                        closed = true;
                        break;
                    }
                }
            }

            if !pending.is_empty() {
                let _ = app.emit(
                    &format!("ssh-output-{session_id}"),
                    serde_json::json!({
                        "sessionId": session_id,
                        "data": STANDARD.encode(&pending),
                    }),
                );
            }

            if closed {
                // The far end is gone; forget the session so the profile list
                // offers Connect again instead of a dead Disconnect.
                if let Some(state) = app.try_state::<AppState>() {
                    tauri::async_runtime::block_on(async {
                        state.ssh_sessions.lock().await.remove(&session_id);
                    });
                }
                let _ = app.emit(
                    &format!("ssh-closed-{session_id}"),
                    serde_json::json!({ "sessionId": session_id }),
                );
                break;
            }

            std::thread::sleep(READ_POLL);
        }
    });
}

/// Reads and parses the SSH config file
pub fn read_ssh_config() -> Result<Vec<SshHostConfig>> {
    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;

    let config_path = home_dir.join(".ssh").join("config");

    println!("🔍 Looking for SSH config at: {:?}", config_path);

    if !config_path.exists() {
        println!("⚠️ SSH config file not found");
        return Ok(Vec::new()); // Return empty list if no config file
    }

    let content = fs::read_to_string(&config_path).context("Failed to read SSH config file")?;

    println!("📄 Read SSH config file, {} bytes", content.len());

    let hosts = parse_ssh_config(&content, &home_dir)?;
    println!("✅ Parsed {} SSH hosts", hosts.len());
    for host in &hosts {
        println!(
            "  - {} ({})",
            host.name,
            host.hostname.as_deref().unwrap_or("no hostname")
        );
    }

    Ok(hosts)
}

/// Parses SSH config file content
fn parse_ssh_config(content: &str, home_dir: &Path) -> Result<Vec<SshHostConfig>> {
    let mut hosts = Vec::new();
    let mut current_host: Option<SshHostConfig> = None;

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let keyword = parts[0].to_lowercase();

        match keyword.as_str() {
            "host" if parts.len() > 1 => {
                // Save previous host if exists
                if let Some(host) = current_host.take() {
                    hosts.push(host);
                }
                // Start new host (skip wildcards like *)
                let host_name = parts[1];
                if !host_name.contains('*') && !host_name.contains('?') {
                    current_host = Some(SshHostConfig {
                        name: host_name.to_string(),
                        hostname: None,
                        user: None,
                        port: None,
                        identity_file: None,
                    });
                }
            }
            "hostname" if parts.len() > 1 => {
                if let Some(ref mut host) = current_host {
                    host.hostname = Some(parts[1].to_string());
                }
            }
            "user" if parts.len() > 1 => {
                if let Some(ref mut host) = current_host {
                    host.user = Some(parts[1].to_string());
                }
            }
            "port" if parts.len() > 1 => {
                if let Some(ref mut host) = current_host {
                    if let Ok(port) = parts[1].parse::<u16>() {
                        host.port = Some(port);
                    }
                }
            }
            "identityfile" if parts.len() > 1 => {
                if let Some(ref mut host) = current_host {
                    let path = parts[1].replace("~", &home_dir.to_string_lossy());
                    host.identity_file = Some(path);
                }
            }
            _ => {}
        }
    }

    // Don't forget the last host
    if let Some(host) = current_host {
        hosts.push(host);
    }

    Ok(hosts)
}

/// Creates an SSH session, opens its shell, and stores it in app state.
pub async fn create_ssh_session(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    config: SshConfig,
) -> Result<String> {
    let session_id = AppState::generate_session_id();

    // Connecting blocks on DNS, TCP and key exchange. Off the async worker it
    // would otherwise occupy for the length of a handshake.
    let connect_config = config.clone();
    let channel = tauri::async_runtime::spawn_blocking(move || {
        // 80x24 is a placeholder: the frontend calls resize_ssh once xterm has
        // measured the pane, which is the only point the real size is known.
        connect_shell(&connect_config, 80, 24)
    })
    .await
    .context("SSH connect task failed")??;

    let channel = Arc::new(Mutex::new(channel));

    let ssh_session = SshSession {
        _id: session_id.clone(),
        host: config.host.clone(),
        username: config.username.clone(),
        connected_at: std::time::SystemTime::now(),
        channel: Arc::clone(&channel),
    };

    state
        .ssh_sessions
        .lock()
        .await
        .insert(session_id.clone(), ssh_session);

    spawn_ssh_reader(app.clone(), session_id.clone(), channel);

    // Emit connection success event
    let _ = app.emit(
        "ssh-connected",
        serde_json::json!({
            "sessionId": session_id,
            "host": config.host,
        }),
    );

    Ok(session_id)
}

/// Looks up a live session's channel.
async fn channel_for(
    state: &tauri::State<'_, AppState>,
    session_id: &str,
) -> Result<Arc<Mutex<Channel>>> {
    let sessions = state.ssh_sessions.lock().await;
    let session = sessions
        .get(session_id)
        .context("SSH session not found")?;
    Ok(Arc::clone(&session.channel))
}

/// Sends keystrokes to the remote shell.
///
/// Off the async worker: a wedged link makes this spin for IO_BUDGET, and
/// parking tokio workers on terminal I/O is the freeze this codebase already
/// paid for twice (b528872).
pub async fn write_to_ssh(
    state: tauri::State<'_, AppState>,
    session_id: String,
    data: &[u8],
) -> Result<()> {
    let channel = channel_for(&state, &session_id).await?;
    let data = data.to_vec();
    tauri::async_runtime::spawn_blocking(move || -> io::Result<()> {
        let mut channel = channel.lock().unwrap();
        write_all_retrying(&mut *channel, &data, IO_BUDGET)?;
        retry_would_block(IO_BUDGET, || channel.flush())
    })
    .await
    .context("SSH write task failed")?
    .context("Failed to write to SSH session")?;
    Ok(())
}

/// Tells the remote PTY how big the pane actually is.
/// Without this the remote shell keeps wrapping at the placeholder 80 columns.
pub async fn resize_ssh(
    state: tauri::State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let channel = channel_for(&state, &session_id).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut channel = channel.lock().unwrap();
        retry_would_block(IO_BUDGET, || {
            channel
                .request_pty_size(cols as u32, rows as u32, None, None)
                .map_err(io::Error::from)
        })
    })
    .await
    .context("SSH resize task failed")?
    .context("Failed to resize the remote PTY")?;
    Ok(())
}

/// Closes an SSH session
pub async fn close_ssh_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<()> {
    let mut sessions = state.ssh_sessions.lock().await;

    if let Some(session) = sessions.remove(&session_id) {
        // Tell the far end before dropping. Both calls can report EAGAIN on a
        // non-blocking session and neither is worth failing a disconnect over:
        // dropping the last handle frees the channel either way.
        if let Ok(mut channel) = session.channel.lock() {
            let _ = channel.send_eof();
            let _ = channel.close();
        }
        Ok(())
    } else {
        Err(anyhow::anyhow!("SSH session not found"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_ssh_config_default() {
        let config = SshConfig::default();
        assert_eq!(config.port, 22);
        assert!(config.password.is_none());
    }

    #[test]
    fn test_parse_ssh_config() {
        let config_content = r#"
Host myserver
    HostName example.com
    User ubuntu
    Port 2222
    IdentityFile ~/.ssh/id_rsa

Host production
    HostName 192.168.1.100
    User admin

Host *
    ServerAliveInterval 60
"#;
        let home_dir = PathBuf::from("/home/test");
        let hosts = parse_ssh_config(config_content, &home_dir).unwrap();

        assert_eq!(hosts.len(), 2);

        assert_eq!(hosts[0].name, "myserver");
        assert_eq!(hosts[0].hostname, Some("example.com".to_string()));
        assert_eq!(hosts[0].user, Some("ubuntu".to_string()));
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(
            hosts[0].identity_file,
            Some("/home/test/.ssh/id_rsa".to_string())
        );

        assert_eq!(hosts[1].name, "production");
        assert_eq!(hosts[1].hostname, Some("192.168.1.100".to_string()));
        assert_eq!(hosts[1].user, Some("admin".to_string()));
    }

    /// The shipped bug: the profile editor sent `privateKey`, this struct read
    /// `key_path`, serde dropped the unknown field, and the connect failed with
    /// "No authentication method provided" as if the profile were empty.
    #[test]
    fn a_key_profile_from_the_editor_resolves_to_key_auth() {
        let payload = serde_json::json!({
            "host": "cosmos.example",
            "port": 22,
            "username": "andre",
            "password": null,
            "keyPath": "/home/andre/.ssh/id_ed25519",
        });
        let config: SshConfig = serde_json::from_value(payload).expect("editor payload parses");
        assert_eq!(config.key_path.as_deref(), Some("/home/andre/.ssh/id_ed25519"));
        assert_eq!(
            auth_method(&config).unwrap(),
            Auth::Key {
                path: "/home/andre/.ssh/id_ed25519",
                passphrase: None
            }
        );
    }

    #[test]
    fn a_password_profile_prefers_the_password() {
        let config = SshConfig {
            password: Some("hunter2".into()),
            key_path: Some("/tmp/key".into()),
            ..SshConfig::default()
        };
        assert_eq!(auth_method(&config).unwrap(), Auth::Password("hunter2"));
    }

    #[test]
    fn a_profile_with_no_credential_is_rejected() {
        let config = SshConfig {
            password: Some(String::new()),
            key_path: Some("   ".into()),
            ..SshConfig::default()
        };
        assert!(auth_method(&config).is_err(), "blank fields are not credentials");
    }

    /// A non-blocking channel takes what it feels like and says EAGAIN for the
    /// rest. Losing the offset here would resend a paste from the start.
    #[derive(Default)]
    struct StubbornSink {
        accepted: Vec<u8>,
        /// Consumed in order: None is EAGAIN, Some(n) accepts at most n bytes.
        script: std::collections::VecDeque<Option<usize>>,
    }

    impl Write for StubbornSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            match self.script.pop_front() {
                Some(None) => Err(io::Error::new(io::ErrorKind::WouldBlock, "eagain")),
                Some(Some(max)) => {
                    let take = max.min(buf.len());
                    self.accepted.extend_from_slice(&buf[..take]);
                    Ok(take)
                }
                None => {
                    self.accepted.extend_from_slice(buf);
                    Ok(buf.len())
                }
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_write_survives_eagain_and_short_writes() {
        let mut sink = StubbornSink {
            script: vec![None, Some(3), None, Some(2)].into(),
            ..Default::default()
        };
        write_all_retrying(&mut sink, b"echo hello\r", IO_BUDGET).expect("write completes");
        assert_eq!(sink.accepted, b"echo hello\r", "every byte, once, in order");
    }

    #[test]
    fn a_channel_that_never_drains_gives_up() {
        let mut sink = StubbornSink {
            script: std::iter::repeat(None).take(10_000).collect(),
            ..Default::default()
        };
        let err = write_all_retrying(&mut sink, b"x", Duration::from_millis(20))
            .expect_err("a permanently blocked channel must surface");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    }

    /// The whole loop against a real server: key auth, a PTY, a shell, a
    /// keystroke going out and the shell's answer coming back.
    ///
    /// Ignored because it needs a server. Everything above this proves a piece;
    /// only this proves the pieces are connected, which is exactly what was
    /// wrong (XNAUT-200). Start any sshd and run:
    ///
    ///   XNAUT_SSH_TEST='me@127.0.0.1:2222:/path/to/key' \
    ///     cargo test --bin xnaut -- --ignored a_real_shell
    #[test]
    #[ignore]
    fn a_real_shell_runs_what_is_typed_into_it() {
        let spec = std::env::var("XNAUT_SSH_TEST")
            .expect("XNAUT_SSH_TEST=user@host:port:/path/to/key");
        let (username, rest) = spec.split_once('@').expect("user@host:port:key");
        let mut parts = rest.splitn(3, ':');
        let host = parts.next().expect("host").to_string();
        let port: u16 = parts.next().expect("port").parse().expect("numeric port");
        let key_path = parts.next().expect("key path").to_string();

        let config = SshConfig {
            host,
            port,
            username: username.to_string(),
            key_path: Some(key_path),
            ..SshConfig::default()
        };
        let mut channel = connect_shell(&config, 80, 24).expect("shell opens");

        // Arithmetic the shell has to evaluate: the echoed command line carries
        // the expression, so only a shell that actually ran it produces 200.
        write_all_retrying(&mut channel, b"echo xnaut-$((100+100))\n", IO_BUDGET)
            .expect("keystrokes reach the channel");
        retry_would_block(IO_BUDGET, || channel.flush()).expect("flush");

        let deadline = Instant::now() + Duration::from_secs(15);
        let mut seen = Vec::new();
        let mut buffer = [0u8; 8192];
        while Instant::now() < deadline {
            match channel.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => seen.extend_from_slice(&buffer[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(READ_POLL)
                }
                Err(e) => panic!("read failed: {e}"),
            }
            if String::from_utf8_lossy(&seen).contains("xnaut-200") {
                return;
            }
        }
        panic!(
            "the remote shell never answered. Saw: {:?}",
            String::from_utf8_lossy(&seen)
        );
    }
}
