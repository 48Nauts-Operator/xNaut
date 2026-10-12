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
}

#[derive(Deserialize)]
struct MaxLaunchRegistrationResponse {
    base_url: String,
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
            ttl_seconds: 21_600,
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
    bound_url(origin, &registered.base_url)
}


fn bound_url(origin: &str, registered: &str) -> Result<String, String> {
    let returned = reqwest::Url::parse(registered).map_err(|_| "Invalid NautGate Max launch URL")?;
    let path = returned.path();
    let token = path.strip_prefix("/v1/max/launches/").ok_or("Invalid NautGate Max launch path")?;
    if token.is_empty() || !token.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
        return Err("Invalid NautGate Max launch capability".into());
    }
    // Ingress may report the container's origin. Retain the configured origin;
    // never send a subscription credential to a host from a response body.
    Ok(format!("{}{path}", origin.trim_end_matches('/')))
}

#[derive(Serialize)]
pub struct Credential {
    pub access_token: String,
    pub expires_at_ms: u64,
}

fn parse_credential(raw: &[u8], now_ms: u64) -> Result<Credential, String> {
    let value: serde_json::Value = serde_json::from_slice(raw).map_err(|_| "Claude Max credentials are invalid")?;
    let oauth = &value["claudeAiOauth"];
    let token = oauth["accessToken"].as_str().filter(|s| s.starts_with("sk-ant-oat01-"))
        .ok_or("Claude Max access token is missing; sign in to Claude Code on this Mac")?;
    if oauth["subscriptionType"].as_str() != Some("max") {
        return Err("This sandbox requires the configured Claude Max subscription".into());
    }
    let expires = oauth["expiresAt"].as_u64().ok_or("Claude Max credential expiry is missing")?;
    if expires <= now_ms.saturating_add(60_000) {
        return Err("Claude Max credential has expired or is expiring; refresh the Claude Code login before retrying. No paid API fallback was used".into());
    }
    Ok(Credential { access_token: token.into(), expires_at_ms: expires })
}

pub fn credential() -> Result<Credential, String> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output().map_err(|_| "Cannot read Claude Max credentials from the owner keychain")?;
    if !output.status.success() {
        return Err("Claude Max credentials are unavailable; sign in to Claude Code on this Mac".into());
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "System clock is before the Unix epoch")?.as_millis() as u64;
    parse_credential(&output.stdout, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ingress_cannot_redirect_subscription_credentials() {
        assert_eq!(bound_url("https://gate.example", "http://internal:8090/v1/max/launches/opaque-token").unwrap(),
            "https://gate.example/v1/max/launches/opaque-token");
        for bad in ["https://other.example/v1/messages", "https://other.example/v1/max/launches/", "https://other.example/v1/max/launches/token/extra"] {
            assert!(bound_url("https://gate.example", bad).is_err());
        }
    }
    #[test]
    fn max_credentials_are_explicit_and_fail_closed_without_secrets_in_errors() {
        let mut value = serde_json::json!({"claudeAiOauth":{"accessToken":"sk-ant-oat01-fixture-secret", "expiresAt":120001, "subscriptionType":"max"}});
        assert!(parse_credential(&serde_json::to_vec(&value).unwrap(), 60000).is_ok());
        for expiry in [0, 119999, 120000] {
            value["claudeAiOauth"]["expiresAt"] = expiry.into();
            let error = parse_credential(&serde_json::to_vec(&value).unwrap(), 60000).err().unwrap();
            assert!(!error.contains("fixture-secret"));
        }
        value["claudeAiOauth"]["expiresAt"] = 240000.into();
        value["claudeAiOauth"]["subscriptionType"] = "pro".into();
        assert!(parse_credential(&serde_json::to_vec(&value).unwrap(), 60000).is_err());
    }
}
