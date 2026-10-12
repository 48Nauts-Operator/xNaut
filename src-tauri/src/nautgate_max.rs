//! Shared Claude Max launch contract for local and sandbox runtimes.
//! NautGate registration uses its own key; inference uses the owner's Max
//! access token. Never substitute the gateway key for subscription auth.
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize)]
struct MaxLaunchRegistration<'a> {
    app: &'static str,
    project: &'a str,
    native_session: &'a str,
    run_id: &'a str,
    ttl_seconds: u32,
    credential: &'a Credential,
}

#[derive(Deserialize)]
struct MaxLaunchRegistrationResponse {
    base_url: String,
    #[serde(default)]
    renewable: bool,
}

pub async fn register(
    route: &crate::settings::LlmSettings,
    project: &str,
    native_session: &str,
    run_id: &str,
) -> Result<String, String> {
    let token = route
        .api_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "NautGate Max launch binding requires the configured NautGate key".to_string()
        })?;
    let origin = route
        .endpoint
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or(route.endpoint.trim_end_matches('/'));
    let owned_route = route.clone();
    let credential = tokio::task::spawn_blocking(move || credential_for_route(&owned_route))
        .await
        .map_err(|_| "Claude Max credential maintenance failed")??;
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("could not create NautGate launch client: {error}"))?
        .post(format!("{origin}/v1/max/launches"))
        .bearer_auth(token)
        .json(&MaxLaunchRegistration {
            app: "xnaut",
            project,
            native_session,
            run_id,
            ttl_seconds: 86_400,
            credential: &credential,
        })
        .send()
        .await
        .map_err(|error| format!("NautGate Max launch registration failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "NautGate rejected Max launch registration ({})",
            response.status()
        ));
    }
    let registered: MaxLaunchRegistrationResponse = response
        .json()
        .await
        .map_err(|error| format!("invalid NautGate Max launch response: {error}"))?;
    if !registered.renewable {
        return Err(
            "NautGate requires the durable Max launch update before this worker can start".into(),
        );
    }
    bound_url(origin, &registered.base_url)
}

fn bound_url(origin: &str, registered: &str) -> Result<String, String> {
    let returned =
        reqwest::Url::parse(registered).map_err(|_| "Invalid NautGate Max launch URL")?;
    let path = returned.path();
    let token = path
        .strip_prefix("/v1/max/launches/")
        .ok_or("Invalid NautGate Max launch path")?;
    if token.is_empty()
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err("Invalid NautGate Max launch capability".into());
    }
    // Ingress may report the container's origin. Retain the configured origin;
    // never send a subscription credential to a host from a response body.
    Ok(format!("{}{path}", origin.trim_end_matches('/')))
}

#[derive(Clone, Serialize)]
pub struct Credential {
    pub access_token: String,
    pub expires_at_ms: u64,
    pub subscription_type: &'static str,
}

fn parse_credential(raw: &[u8], now_ms: u64) -> Result<Credential, String> {
    let value: serde_json::Value =
        serde_json::from_slice(raw).map_err(|_| "Claude Max credentials are invalid")?;
    let oauth = &value["claudeAiOauth"];
    let token = oauth["accessToken"]
        .as_str()
        .filter(|s| s.starts_with("sk-ant-oat01-"))
        .ok_or("Claude Max access token is missing; sign in to Claude Code on this Mac")?;
    if oauth["subscriptionType"].as_str() != Some("max") {
        return Err("This sandbox requires the configured Claude Max subscription".into());
    }
    let expires = oauth["expiresAt"]
        .as_u64()
        .ok_or("Claude Max credential expiry is missing")?;
    if expires <= now_ms.saturating_add(60_000) {
        return Err("Claude Max credential has expired or is expiring; refresh the Claude Code login before retrying. No paid API fallback was used".into());
    }
    Ok(Credential {
        access_token: token.into(),
        expires_at_ms: expires,
        subscription_type: "max",
    })
}

fn read_credential() -> Result<Vec<u8>, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Stdio;
        let mut child = std::process::Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "Claude Code-credentials",
                "-w",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Cannot read Claude Max credentials from the owner keychain")?;
        let start = std::time::Instant::now();
        loop {
            if child
                .try_wait()
                .map_err(|_| "Cannot observe owner keychain access")?
                .is_some()
            {
                break;
            }
            if start.elapsed() > Duration::from_secs(8) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Owner keychain is locked or unavailable; Max launch paused without a paid fallback".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let output = child
            .wait_with_output()
            .map_err(|_| "Cannot read owner keychain response")?;
        if output.status.success() {
            return Ok(output.stdout);
        }
    }
    let config = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|p| p.join(".claude")))
        .ok_or("Owner Claude configuration directory is unavailable")?;
    std::fs::read(config.join(".credentials.json")).map_err(|_| {
        "Claude Max credentials are unavailable; sign in to Claude Code on this computer".into()
    })
}

pub fn credential() -> Result<Credential, String> {
    parse_credential(&read_credential()?, crate::run_control::now_ms() as u64)
}

pub fn default_model() -> Result<String, String> {
    let profile = crate::agent_profiles::agent_profile_get("claude".into())?;
    explicit_model(&profile.model)
}

fn explicit_model(model: &str) -> Result<String, String> {
    let model = model.trim();
    if !model.starts_with("claude-") {
        return Err("Select a concrete Claude model in the Claude profile for Max workers; auto and other-provider defaults cannot be passed through".into());
    }
    Ok(model.into())
}

fn needs_renewal(raw: &[u8], now: u64) -> Result<bool, String> {
    let value: serde_json::Value =
        serde_json::from_slice(raw).map_err(|_| "Invalid Claude credentials")?;
    let oauth = &value["claudeAiOauth"];
    if oauth["subscriptionType"] != "max"
        || !oauth["accessToken"]
            .as_str()
            .is_some_and(|s| s.starts_with("sk-ant-oat01-"))
    {
        return Err("Claude Max login is required; no paid fallback was used".into());
    }
    let expires = oauth["expiresAt"]
        .as_u64()
        .ok_or("Claude Max credential expiry is missing")?;
    Ok(expires <= now.saturating_add(120_000))
}

fn credential_for_route(route: &crate::settings::LlmSettings) -> Result<Credential, String> {
    // Let the installed Claude client own refresh-token rotation and its native
    // credential-store locking. Never copy refresh credentials to a worker or
    // gateway. A minimal, tool-free native turn only runs near access expiry.
    static REFRESH: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _lock = REFRESH
        .lock()
        .map_err(|_| "Claude credential maintenance lock failed")?;
    let raw = read_credential()?;
    if needs_renewal(&raw, crate::run_control::now_ms() as u64)? {
        use std::process::Stdio;
        let binary = dirs::home_dir()
            .map(|p| p.join(".local/bin/claude"))
            .filter(|p| p.is_file())
            .unwrap_or_else(|| std::path::PathBuf::from("claude"));
        let mut cmd = std::process::Command::new(binary);
        for (key, _) in std::env::vars() {
            if key.starts_with("ANTHROPIC_")
                || key.starts_with("CLAUDE_CODE_USE_")
                || ["CLAUDE_CODE_OAUTH_TOKEN", "CLAUDECODE"].contains(&key.as_str())
            {
                cmd.env_remove(key);
            }
        }
        cmd.env(
            "ANTHROPIC_BASE_URL",
            crate::agents::anthropic_base(&route.endpoint),
        );
        cmd.args([
            "-p",
            "--model",
            &default_model()?,
            "--max-turns",
            "1",
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--setting-sources",
            "",
            "--no-session-persistence",
            "Reply READY.",
        ])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        let mut child = cmd
            .spawn()
            .map_err(|_| "Cannot start native Claude credential renewal")?;
        let started = std::time::Instant::now();
        loop {
            if let Some(_status) = child
                .try_wait()
                .map_err(|_| "Cannot observe Claude credential renewal")?
            {
                break;
            }
            if started.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    credential()
}

pub async fn renew_active() -> Result<(), String> {
    let registry = crate::agents::registry_dir()?;
    let active = crate::run_control::list_ids_in(&registry)?
        .into_iter()
        .any(|id| {
            crate::run_control::load_manifest_in(&registry, &id)
                .is_ok_and(|r| r.runtime_id == "claude" && !r.state.terminal())
        });
    if !active {
        return Ok(());
    }
    let settings = crate::settings::load_or_default();
    if !crate::chat::gateway_enabled(&settings) {
        return Ok(());
    }
    let route =
        crate::chat::provider_llm(&settings, "nautgate").ok_or("NautGate connection missing")?;
    let owned = route.clone();
    let credential = tokio::task::spawn_blocking(move || credential_for_route(&owned))
        .await
        .map_err(|_| "Claude Max renewal task failed")??;
    let origin = crate::agents::anthropic_base(&route.endpoint);
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Max renewal client failed")?
        .put(format!("{origin}/v1/max/credentials"))
        .bearer_auth(route.api_key.as_deref().ok_or("NautGate key missing")?)
        .json(&credential)
        .send()
        .await
        .map_err(|_| "NautGate Max renewal transport failed")?;
    if !response.status().is_success() {
        return Err(format!(
            "NautGate Max renewal refused ({})",
            response.status()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renewal_only_uses_valid_max_identity_and_starts_before_expiry() {
        let mut value = serde_json::json!({"claudeAiOauth":{"accessToken":"sk-ant-oat01-fixture", "expiresAt":180001, "subscriptionType":"max"}});
        assert!(!needs_renewal(&serde_json::to_vec(&value).unwrap(), 60000).unwrap());
        for expires in [180000, 120000, 0] {
            value["claudeAiOauth"]["expiresAt"] = expires.into();
            assert!(needs_renewal(&serde_json::to_vec(&value).unwrap(), 60000).unwrap());
            assert!(
                parse_credential(&serde_json::to_vec(&value).unwrap(), expires).is_err(),
                "unchanged expired credentials remain refused after renewal"
            );
        }
        value["claudeAiOauth"]["subscriptionType"] = "api".into();
        assert!(needs_renewal(&serde_json::to_vec(&value).unwrap(), 60000).is_err());
    }
    #[test]
    fn subscription_model_never_inherits_auto_or_another_provider_default() {
        assert_eq!(
            explicit_model("claude-fable-5-1").unwrap(),
            "claude-fable-5-1"
        );
        for wrong in ["", "auto", "qwen-default", "gpt-6"] {
            assert!(explicit_model(wrong).is_err());
        }
    }
    #[test]
    fn ingress_cannot_redirect_subscription_credentials() {
        assert_eq!(
            bound_url(
                "https://gate.example",
                "http://internal:8090/v1/max/launches/opaque-token"
            )
            .unwrap(),
            "https://gate.example/v1/max/launches/opaque-token"
        );
        for bad in [
            "https://other.example/v1/messages",
            "https://other.example/v1/max/launches/",
            "https://other.example/v1/max/launches/token/extra",
        ] {
            assert!(bound_url("https://gate.example", bad).is_err());
        }
    }
    #[test]
    fn max_credentials_are_explicit_and_fail_closed_without_secrets_in_errors() {
        let mut value = serde_json::json!({"claudeAiOauth":{"accessToken":"sk-ant-oat01-fixture-secret", "expiresAt":120001, "subscriptionType":"max"}});
        assert!(parse_credential(&serde_json::to_vec(&value).unwrap(), 60000).is_ok());
        for expiry in [0, 119999, 120000] {
            value["claudeAiOauth"]["expiresAt"] = expiry.into();
            let error = parse_credential(&serde_json::to_vec(&value).unwrap(), 60000)
                .err()
                .unwrap();
            assert!(!error.contains("fixture-secret"));
        }
        value["claudeAiOauth"]["expiresAt"] = 240000.into();
        value["claudeAiOauth"]["subscriptionType"] = "pro".into();
        assert!(parse_credential(&serde_json::to_vec(&value).unwrap(), 60000).is_err());
    }
}
