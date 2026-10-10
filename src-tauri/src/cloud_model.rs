//! Phase 1: one explicit model/connection for remote agent launches.
//! Execution providers only transport this configuration; they never choose a model.
use crate::{agent_profiles::AgentProfile, settings::Settings};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    /// Optional address of the same provider reachable from workers.
    #[serde(default)]
    pub worker_endpoint: String,
}

/// Non-secret approval/receipt data. Credentials are resolved separately.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
}

#[derive(Clone)]
pub struct Resolved {
    pub pin: Pin,
    pub api_key: String,
}

pub fn resolve(settings: &Settings) -> Result<Option<Resolved>, String> {
    let selected = &settings.cloud_agent_model;
    if selected.model.trim().is_empty()
        && selected.provider.trim().is_empty()
        && selected.worker_endpoint.trim().is_empty()
    {
        return Ok(None);
    }
    if selected.model.trim().is_empty() || selected.provider.trim().is_empty() {
        return Err(
            "Choose both a provider connection and a cloud agent model in Settings.".into(),
        );
    }
    // Deliberately do not call route_llm: automatic NautGate model routing is Phase 2.
    let provider = selected.provider.trim().to_ascii_lowercase();
    let connection = crate::chat::provider_llm(settings, &provider)
        .ok_or("The cloud model's provider connection is disabled or missing in Settings.")?;
    let endpoint = if selected.worker_endpoint.trim().is_empty() {
        connection.endpoint.trim()
    } else {
        selected.worker_endpoint.trim()
    }
    .trim_end_matches('/');
    let url = reqwest::Url::parse(endpoint)
        .map_err(|_| "The cloud model endpoint must be an HTTP(S) URL.")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("The cloud model endpoint must be an HTTP(S) URL without credentials, query or fragment.".into());
    }
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".localhost")
        || host == "0.0.0.0"
        || host == "::"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
    {
        return Err("The cloud model endpoint points to localhost. Set its worker-reachable address in Settings.".into());
    }
    Ok(Some(Resolved {
        pin: Pin {
            provider,
            model: selected.model.trim().into(),
            endpoint: endpoint.into(),
        },
        // Keyless OpenAI-compatible servers still need an auth placeholder in Pi.
        api_key: connection
            .api_key
            .filter(|key| !key.trim().is_empty())
            .unwrap_or_else(|| "xnaut-keyless".into()),
    }))
}

pub fn apply(
    settings: &Settings,
    profile: &AgentProfile,
    environment: &str,
) -> Result<(AgentProfile, Option<Resolved>), String> {
    let mut effective = profile.clone();
    if environment == "local" {
        return Ok((effective, None));
    }
    let selected = resolve(settings)?;
    if let Some(selection) = &selected {
        validate_connection_for_harness(&profile.runtime_id, &selection.pin)?;
        effective.model = selection.pin.model.clone();
    }
    Ok((effective, selected))
}

fn validate_connection_for_harness(runtime: &str, connection: &Pin) -> Result<(), String> {
    if !matches!(runtime, "pi" | "codex" | "claude") {
        return Err(format!("The shared cloud model has no adapter for the {runtime} harness. Configure a supported harness; xNaut will not switch it silently."));
    }
    // A model's name does not determine its protocol. Claude Code can use a
    // non-Anthropic model behind a Messages-compatible gateway, and Codex can
    // use a custom model behind a Responses-compatible endpoint. Refuse known
    // incompatible direct APIs, not arbitrary model IDs or provider labels.
    let url =
        reqwest::Url::parse(&connection.endpoint).map_err(|_| "Invalid cloud model endpoint")?;
    let required = match (runtime, url.host_str().unwrap_or_default()) {
        ("claude", "api.openai.com") => Some("Anthropic Messages"),
        ("codex", "api.anthropic.com") => Some("OpenAI Responses"),
        _ => None,
    };
    if let Some(api) = required {
        return Err(format!("The {runtime} harness requires the {api} API. Select a compatible connection or change the harness; xNaut will not switch it silently."));
    }
    Ok(())
}

pub fn runtime(
    cfg: &crate::agents::AgentConfig,
    selected: Option<&Resolved>,
) -> Result<crate::agents::AgentConfig, String> {
    let mut cfg = cfg.clone();
    let Some(selected) = selected else {
        return Ok(cfg);
    };
    validate_connection_for_harness(&cfg.id, &selected.pin)?;
    if cfg.launch_cmd != cfg.id {
        return Err("The shared cloud model requires a standard pi, codex or claude command; custom runtime wrappers must declare their own model configuration.".into());
    }
    cfg.env.retain(|key, _| {
        !key.starts_with("ANTHROPIC_")
            && !key.starts_with("OPENAI_")
            && !key.starts_with("CLAUDE_CODE_USE_")
            && key != "CLAUDE_CODE_OAUTH_TOKEN"
            && key != "PI_CODING_AGENT_DIR"
            && key != "XNAUT_CLOUD_API_KEY"
    });
    // Remove old explicit selectors so the shared choice has one meaning.
    let mut args = cfg.extra_args.into_iter();
    cfg.extra_args = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--model" || arg == "-m" || (cfg.id == "pi" && arg == "--provider") {
            args.next();
        } else if !arg.starts_with("--model=")
            && !(cfg.id == "pi" && arg.starts_with("--provider="))
        {
            cfg.extra_args.push(arg);
        }
    }
    match cfg.id.as_str() {
        "pi" => cfg
            .extra_args
            .extend(["--provider".into(), "xnaut-cloud".into()]),
        "codex" => {
            let endpoint =
                serde_json::to_string(&selected.pin.endpoint).map_err(|e| e.to_string())?;
            cfg.extra_args.extend(["-c".into(), "model_provider=\"xnaut-cloud\"".into(), "-c".into(),
                format!("model_providers.xnaut-cloud={{name=\"xNaut Cloud\",base_url={endpoint},env_key=\"XNAUT_CLOUD_API_KEY\",wire_api=\"responses\"}}")]);
        }
        _ => {}
    }
    Ok(cfg)
}

pub fn command(command: String, selected: Option<&Resolved>, run_id: &str) -> String {
    if selected.is_none() {
        return command;
    }
    // Only the opaque run ID travels in argv. The worker wrapper reads its
    // private configuration and injects credentials into the child environment.
    let script = format!(
        "{}\nlaunch(sys.argv[1], sys.argv[2:])",
        include_str!("worker_model.py")
    );
    format!(
        "python3 -c {} {} sh -c {}",
        crate::sandbox::exe::shell_single_quote(&script),
        crate::sandbox::exe::shell_single_quote(run_id),
        crate::sandbox::exe::shell_single_quote(&command)
    )
}

/// If staging/admission fails, discard only this unstarted run's credentials.
/// Once the worker may have started, its wrapper owns cleanup. SSH never runs
/// on the UI thread, including this error path.
pub struct Preparation {
    target: crate::worker_bootstrap::Target,
    id: String,
    pending: bool,
}
impl Preparation {
    pub fn new(target: crate::worker_bootstrap::Target, id: &str, enabled: bool) -> Self {
        Self {
            target,
            id: id.into(),
            pending: enabled,
        }
    }
    pub fn handed_off(&mut self) {
        self.pending = false;
    }
}
impl Drop for Preparation {
    fn drop(&mut self) {
        if !self.pending {
            return;
        }
        let target = self.target.clone();
        let id = self.id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let script = format!(
                "{}\nshutil.rmtree(cloud_directory(sys.argv[1]), ignore_errors=True)",
                include_str!("worker_model.py")
            );
            let command = format!(
                "python3 -c {} {}",
                crate::sandbox::exe::shell_single_quote(&script),
                crate::sandbox::exe::shell_single_quote(&id)
            );
            let _ = target.exchange(&command, &[], 15);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(unix)]
    fn worker_model_protocol_suite() {
        let output = std::process::Command::new("python3")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../tests/repository-transfer/test_worker_model.py"),
            )
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn settings() -> Settings {
        let mut settings = Settings::default();
        settings.llm_providers = vec![crate::settings::LlmProviderSettings {
            name: "test-provider".into(),
            endpoint: "https://models.example/v1".into(),
            api_key: Some("private-fixture-key".into()),
            enabled: true,
        }];
        settings.cloud_agent_model = Selection {
            provider: "test-provider".into(),
            model: "gpt-cloud-test".into(),
            worker_endpoint: String::new(),
        };
        settings
    }
    fn profile(runtime: &str) -> AgentProfile {
        serde_json::from_value(serde_json::json!({"handle":"test", "display_name":"Test",
            "tagline":"", "purpose":"test", "runtime_id":runtime, "provider":"profile-provider",
            "model":"profile-model", "role":"builder", "default_project":null}))
        .unwrap()
    }
    #[test]
    fn shared_cloud_choice_is_identical_across_destinations_and_preserves_local_profiles() {
        let settings = settings();
        let original = profile("pi");
        for destination in ["exe-dev", "gitvm", "future-cloud-driver"] {
            let (effective, route) = apply(&settings, &original, destination).unwrap();
            assert_eq!(effective.model, "gpt-cloud-test");
            assert_eq!(effective.runtime_id, "pi");
            assert_eq!(route.unwrap().pin.endpoint, "https://models.example/v1");
        }
        let (local, route) = apply(&settings, &original, "local").unwrap();
        assert_eq!(local, original);
        assert!(route.is_none());
        assert_eq!(original.model, "profile-model");
    }
    #[test]
    fn cloud_settings_round_trip_and_legacy_settings_preserve_unknown_keys() {
        let mut settings = settings();
        settings
            .extra
            .insert("future-setting".into(), serde_json::json!({"keep":42}));
        let saved = serde_json::to_value(settings).unwrap();
        let restored: Settings = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(
            resolve(&restored).unwrap().unwrap().pin.model,
            "gpt-cloud-test"
        );
        assert_eq!(restored.extra["future-setting"]["keep"], 42);
        let mut legacy = saved;
        legacy.as_object_mut().unwrap().remove("cloud_agent_model");
        assert!(resolve(&serde_json::from_value(legacy).unwrap())
            .unwrap()
            .is_none());
    }
    #[test]
    fn invalid_or_disabled_cloud_connections_refuse_without_fallback() {
        let mut settings = settings();
        for endpoint in [
            "http://localhost:8090/v1",
            "http://127.1/v1",
            "http://[::1]/v1",
            "https://user:secret@example.com/v1",
            "https://example.com/v1?key=secret",
        ] {
            settings.cloud_agent_model.worker_endpoint = endpoint.into();
            assert!(resolve(&settings).is_err(), "accepted {endpoint}");
        }
        settings.cloud_agent_model.worker_endpoint = "https://worker-reachable.example/v1".into();
        assert_eq!(
            resolve(&settings).unwrap().unwrap().pin.endpoint,
            "https://worker-reachable.example/v1"
        );
        settings.llm_providers[0].enabled = false;
        assert!(resolve(&settings).is_err());
        settings.llm_providers[0].enabled = true;
        settings.cloud_agent_model.model.clear();
        assert!(resolve(&settings).is_err());
    }
    #[test]
    fn shared_cloud_choice_does_not_silently_replace_an_incompatible_harness() {
        let mut settings = settings();
        settings.cloud_agent_model.worker_endpoint = "https://api.openai.com/v1".into();
        assert!(apply(&settings, &profile("claude"), "exe-dev").is_err());
        assert!(apply(&settings, &profile("unsupported-wrapper"), "gitvm").is_err());
        assert!(apply(&settings, &profile("codex"), "exe-dev").is_ok());
        settings.cloud_agent_model.worker_endpoint = "https://api.anthropic.com/v1".into();
        settings.cloud_agent_model.model = "claude-test".into();
        assert!(apply(&settings, &profile("codex"), "gitvm").is_err());
        assert!(apply(&settings, &profile("claude"), "gitvm").is_ok());
        settings.cloud_agent_model.worker_endpoint = "https://custom-gateway.example/v1".into();
        settings.cloud_agent_model.model = "vendor/custom-model".into();
        for harness in ["claude", "codex", "pi"] {
            let (resolved, _) = apply(&settings, &profile(harness), "exe-dev").unwrap();
            assert_eq!(resolved.model, "vendor/custom-model");
            assert_eq!(
                resolved.runtime_id, harness,
                "a gateway model must not switch the assigned harness"
            );
        }
    }
    #[test]
    fn cloud_launch_overrides_worker_defaults_without_putting_credentials_in_argv() {
        let selection = resolve(&settings()).unwrap().unwrap();
        let cfg: crate::agents::AgentConfig = serde_json::from_value(serde_json::json!({
            "id":"pi", "label":"Pi", "detect_cmd":"pi", "launch_cmd":"pi",
            "extra_args":["--provider","old", "--model=old-model"], "expected_process":"pi",
            "prompt_injection_mode":"argv", "env":{"OPENAI_BASE_URL":"http://localhost:8090/v1"}
        }))
        .unwrap();
        let resolved = runtime(&cfg, Some(&selection)).unwrap();
        let (argv, _) =
            crate::agents::build_launch(&resolved, Some("task"), Some(&selection.pin.model));
        assert_eq!(
            argv,
            [
                "pi",
                "--provider",
                "xnaut-cloud",
                "--model",
                "gpt-cloud-test",
                "task"
            ]
        );
        assert!(!resolved.env.contains_key("OPENAI_BASE_URL"));
        let command = command(
            "pi --model gpt-cloud-test".into(),
            Some(&selection),
            "test-run",
        );
        assert!(!command.contains(&selection.api_key));
        assert!(!serde_json::to_string(&selection.pin)
            .unwrap()
            .contains(&selection.api_key));
        assert_eq!(cfg.extra_args[1], "old");
    }
}
