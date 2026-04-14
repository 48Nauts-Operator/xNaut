// ABOUTME: SSH connection management using ssh2 crate for remote terminal sessions.
// ABOUTME: Supports password and key-based authentication with session lifecycle management.

use crate::state::{AppState, SshSession};
use anyhow::{Context, Result};
use ssh2::Session;
use std::fs;
use std::io::Read;
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

/// Configuration for SSH connection
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub key_passphrase: Option<String>,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            username: String::new(),
            password: None,
            key_path: None,
            key_passphrase: None,
        }
    }
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

/// SSH session wrapper with async support
#[allow(dead_code)]
pub struct SshSessionHandle {
    id: String,
    config: SshConfig,
    session: Arc<Mutex<Session>>,
}

impl SshSessionHandle {
    /// Creates a new SSH session and connects
    pub async fn connect(config: SshConfig) -> Result<Self> {
        let session_id = AppState::generate_session_id();

        // Connect to SSH server
        let tcp = TcpStream::connect(format!("{}:{}", config.host, config.port))
            .context("Failed to connect to SSH server")?;

        let mut session = Session::new().context("Failed to create SSH session")?;
        session.set_tcp_stream(tcp);
        session.handshake().context("SSH handshake failed")?;

        // Authenticate
        if let Some(ref password) = config.password {
            session
                .userauth_password(&config.username, password)
                .context("SSH password authentication failed")?;
        } else if let Some(ref key_path) = config.key_path {
            let passphrase = config.key_passphrase.as_deref();
            session
                .userauth_pubkey_file(&config.username, None, Path::new(key_path), passphrase)
                .context("SSH key authentication failed")?;
        } else {
            return Err(anyhow::anyhow!(
                "No authentication method provided (password or key required)"
            ));
        }

        if !session.authenticated() {
            return Err(anyhow::anyhow!("SSH authentication failed"));
        }

        Ok(Self {
            id: session_id,
            config,
            session: Arc::new(Mutex::new(session)),
        })
    }

    /// Opens a shell channel
    #[allow(dead_code)]
    pub async fn open_shell(&self) -> Result<()> {
        let session = self.session.lock().await;
        let mut channel = session
            .channel_session()
            .context("Failed to open channel")?;

        channel.request_pty("xterm", None, None)?;
        channel.shell()?;

        Ok(())
    }

    /// Executes a command over SSH
    #[allow(dead_code)]
    pub async fn execute_command(&self, command: &str) -> Result<String> {
        let session = self.session.lock().await;
        let mut channel = session.channel_session()?;

        channel.exec(command)?;

        let mut output = String::new();
        channel.read_to_string(&mut output)?;
        channel.wait_close()?;

        Ok(output)
    }
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

/// Creates an SSH session and stores it in app state
pub async fn create_ssh_session(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    config: SshConfig,
) -> Result<String> {
    let session_id = AppState::generate_session_id();

    // Create and connect SSH session
    let _ssh_handle = SshSessionHandle::connect(config.clone()).await?;

    // Store session metadata in state
    let ssh_session = SshSession {
        _id: session_id.clone(),
        host: config.host.clone(),
        username: config.username.clone(),
        connected_at: std::time::SystemTime::now(),
    };

    state
        .ssh_sessions
        .lock()
        .await
        .insert(session_id.clone(), ssh_session);

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

/// Closes an SSH session
#[allow(dead_code)]
pub async fn close_ssh_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<()> {
    let mut sessions = state.ssh_sessions.lock().await;

    if sessions.remove(&session_id).is_some() {
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
}
