//! Public voice setup. The webview may submit a key but can never read it back.
use super::{connect, protocol, ClientEvent, Config, ServerEvent};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::{io::Write, path::Path, sync::Mutex, time::Duration};
use tokio_tungstenite::tungstenite::Message;

static SAVE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    configured: bool,
    model: String,
}

fn summary(config: Option<&Config>) -> VoiceSettings {
    VoiceSettings {
        configured: config.is_some(),
        model: config
            .and_then(|c| c.model.clone())
            .unwrap_or_else(|| protocol::DEFAULT_VOICE_MODEL.into()),
    }
}

#[tauri::command]
pub fn voice_live_settings_get() -> Result<VoiceSettings, String> {
    let path = Config::path()?;
    if !path.exists() {
        return Ok(summary(None));
    }
    Config::load_at(&path).map(|c| summary(Some(&c)))
}

fn save_at(path: &Path, api_key: Option<String>, model: String) -> Result<VoiceSettings, String> {
    let previous = Config::load_at(path).ok();
    let api_key = api_key
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| previous.as_ref().map(|c| c.api_key.clone()))
        .ok_or("Enter an API key to enable public voice.")?;
    let config = Config {
        api_key,
        // Preserve existing custom installations; normal setup uses the public route.
        endpoint: previous.and_then(|c| c.endpoint),
        model: Some(model.trim().to_owned()),
    };
    config.validate()?;
    let parent = path.parent().ok_or("invalid voice settings location")?;
    std::fs::create_dir_all(parent).map_err(|_| "cannot create voice settings directory")?;
    let temp = parent.join(format!(".voice-live-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<(), String> {
        let mut file = options
            .open(&temp)
            .map_err(|_| "cannot create private voice settings")?;
        let bytes =
            serde_json::to_vec_pretty(&config).map_err(|_| "cannot encode voice settings")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "cannot write voice settings")?;
        std::fs::rename(&temp, path).map_err(|_| "cannot replace voice settings")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result?;
    Ok(summary(Some(&config)))
}

#[tauri::command]
pub fn voice_live_settings_save(
    api_key: Option<String>,
    model: String,
) -> Result<VoiceSettings, String> {
    let _lock = SAVE_LOCK
        .lock()
        .map_err(|_| "voice settings are unavailable")?;
    save_at(&Config::path()?, api_key, model)
}

#[tauri::command]
pub async fn voice_live_settings_test() -> Result<(), String> {
    let config = Config::load()?;
    let mut socket = connect(&config).await?;
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        socket
            .send(Message::Text(
                ClientEvent::SessionStart {
                    model: config
                        .model
                        .unwrap_or_else(|| protocol::DEFAULT_VOICE_MODEL.into()),
                    instructions: "Connection test only. Do not speak unless asked.".into(),
                }
                .to_json()
                .to_string(),
            ))
            .await
            .map_err(|_| "could not start voice connection test")?;
        while let Some(frame) = socket.next().await {
            match frame.map_err(|_| "voice connection test failed")? {
                Message::Text(text) => {
                    let value = serde_json::from_str(&text)
                        .map_err(|_| "invalid voice service response")?;
                    match ServerEvent::decode(&value) {
                        ServerEvent::SessionStarted => return Ok(()),
                        ServerEvent::Error { .. } | ServerEvent::SessionClosed => return Err(
                            "Voice service rejected the session. Check your key and model access.",
                        ),
                        _ => {}
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        Err("Voice service closed before accepting the session.")
    })
    .await
    .map_err(|_| "voice connection test timed out".to_string())
    .and_then(|r| r.map_err(str::to_owned));
    let _ = tokio::time::timeout(Duration::from_secs(2), socket.close(None)).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_keep_keys_private_preserve_on_blank_and_reject_invalid_updates() {
        let dir =
            std::env::temp_dir().join(format!("xnaut-voice-settings-{}", uuid::Uuid::new_v4()));
        let path = dir.join("voice-live.json");
        assert!(save_at(&path, None, "gpt-live-1".into()).is_err());
        assert!(!path.exists());
        let view = save_at(&path, Some("test-secret-one".into()), "gpt-live-1".into()).unwrap();
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(!serialized.contains("test-secret"));
        assert!(!serialized.contains("apiKey"));
        assert!(view.configured);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        save_at(&path, Some(" ".into()), "gpt-live-1".into()).unwrap();
        assert_eq!(Config::load_at(&path).unwrap().api_key, "test-secret-one");
        assert!(save_at(&path, Some("replacement".into()), "".into()).is_err());
        assert_eq!(Config::load_at(&path).unwrap().api_key, "test-secret-one");
        save_at(&path, Some("test-secret-two".into()), "gpt-live-1".into()).unwrap();
        assert_eq!(Config::load_at(&path).unwrap().api_key, "test-secret-two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
