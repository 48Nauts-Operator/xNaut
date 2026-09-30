// OpenAI-compatible LLM client for the chat panel (v1.6). Streams completions
// over SSE as chat://chunk events; optionally injects Engram memories as context.

use serde::{Deserialize, Serialize};
use tauri::Emitter;

const CHAT_MAX_TOKENS: u32 = 1024;
const CHAT_DOCUMENT_MAX_TOKENS: u32 = 8192;
const CHAT_STREAM_IDLE_TIMEOUT_SECS: u64 = 120;
const NAUTGATE_DEFAULT_ENDPOINT: &str = "http://localhost:8090/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// "system" | "user" | "assistant"
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    pub provider: String,
    pub model: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct CompletionResult {
    pub content: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// The compact receipt NautGate delivers with a response. NautGate owns the
/// durable record; xNAUT only carries its identifier and makes a mismatch or
/// failure visible at the point where the user is already looking.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NautGateReceipt {
    pub decision_id: Option<String>,
    /// The id the gateway's own audit trail is keyed by. Different row from the
    /// decision (`route_decisions d ON d.id = r.decision_id` in NautGate's
    /// queries.py), and the one `GET /v1/audit/receipts/{id}/bundle` wants, so
    /// an export can put the gateway's signed account beside ours (XNAUT-216).
    pub receipt_id: Option<String>,
    pub requested_model: Option<String>,
    pub selected_model: Option<String>,
    pub observed_model: Option<String>,
    pub substituted: bool,
    pub upstream_status: Option<String>,
}

impl NautGateReceipt {
    pub(crate) fn from_headers(headers: &reqwest::header::HeaderMap) -> Self {
        let value = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        Self {
            decision_id: value("x-nautgate-decision-id"),
            receipt_id: value("x-nautgate-receipt-id"),
            requested_model: value("x-nautgate-requested-model"),
            selected_model: value("x-nautgate-model"),
            observed_model: value("x-nautgate-observed-model"),
            substituted: value("x-nautgate-substituted").as_deref() == Some("true"),
            upstream_status: value("x-nautgate-upstream-status"),
        }
    }

    /// The ids as an evidence record body, or None when no gateway answered.
    ///
    /// Nested under one key so a third id later costs no schema version, and
    /// absent entirely when nothing was routed: a record with an empty
    /// `nautgate` object would claim a gateway was involved.
    pub(crate) fn evidence_body(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        let mut ids = serde_json::Map::new();
        for (key, value) in [
            ("decision_id", &self.decision_id),
            ("receipt_id", &self.receipt_id),
        ] {
            if let Some(value) = value {
                ids.insert(key.into(), serde_json::Value::String(value.clone()));
            }
        }
        if ids.is_empty() {
            return None;
        }
        let mut body = serde_json::Map::new();
        body.insert("nautgate".into(), serde_json::Value::Object(ids));
        Some(body)
    }

    pub(crate) fn substitution_notice(&self) -> Option<String> {
        if !self.substituted {
            return None;
        }
        let requested = self.requested_model.as_deref().unwrap_or("requested route");
        let applied = self
            .observed_model
            .as_deref()
            .or(self.selected_model.as_deref())
            .unwrap_or("another route");
        Some(format!(
            "⚠ NautGate routed `{requested}` → `{applied}`{}",
            self.decision_suffix()
        ))
    }

    pub(crate) fn error_suffix(&self) -> String {
        let upstream = self
            .upstream_status
            .as_deref()
            .map(|status| format!("; upstream {status}"))
            .unwrap_or_default();
        format!("{}{upstream}", self.decision_suffix())
    }

    fn decision_suffix(&self) -> String {
        self.decision_id
            .as_deref()
            .map(|id| format!(" · decision `{id}`"))
            .unwrap_or_default()
    }
}

/// Joins the configured endpoint (with or without trailing slash, with or
/// without /v1) and an API path, e.g. "http://localhost:8090/v1/" + "chat/completions".
pub(crate) fn join_endpoint(endpoint: &str, path: &str) -> String {
    format!("{}/{}", endpoint.trim_end_matches('/'), path)
}

/// Strips the "data: " prefix from an SSE line. Returns None for non-data
/// lines (comments, blank lines, "event:" fields).
fn sse_data(line: &str) -> Option<&str> {
    line.strip_prefix("data: ").map(str::trim)
}

/// Extracts choices[0].delta.content from a streaming chunk JSON payload.
fn delta_content(chunk_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(chunk_json).ok()?;
    v["choices"][0]["delta"]["content"]
        .as_str()
        .map(str::to_string)
}

/// Truncated body excerpt for error messages — keeps Err(String)s readable.
fn body_excerpt(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.len() <= 300 {
        return trimmed.to_string();
    }
    let mut end = 300;
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &trimmed[..end])
}

/// Retry only explicit parameter rejection, before any tool can execute.
/// Providers can wrap the same validation message in HTTP 400 or 502.
pub(crate) fn adapt_rejected_request(body: &mut serde_json::Value, error: &str) -> bool {
    let reason = error.to_ascii_lowercase();
    let Some(object) = body.as_object_mut() else { return false; };
    if crate::agent_tools::reasoning_override_rejected(error) && object.remove("reasoning_effort").is_some() { return true; }
    if reason.contains("unsupported") || reason.contains("not supported") || reason.contains("does not support") {
        if reason.contains("'max_tokens'") && reason.contains("max_completion_tokens") {
            if let Some(value) = object.remove("max_tokens") { object.insert("max_completion_tokens".into(),value); return true; }
        } else if reason.contains("max_completion_tokens") {
            if let Some(value) = object.remove("max_completion_tokens") { object.insert("max_tokens".into(),value); return true; }
        }
    }
    false
}

fn streaming_request_body(
    model: &str,
    messages: Vec<ChatMessage>,
    reasoning_effort: Option<&str>,
) -> serde_json::Value {
    let max_tokens = completion_token_budget(&messages);
    let messages = normalize_messages_for_model_template(model, messages);
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "max_tokens": max_tokens,
    });
    if should_disable_reasoning_for_chat(model) {
        body["reasoning_effort"] = serde_json::json!("none");
    } else if let Some(effort) = reasoning_effort
        .map(str::trim)
        .filter(|effort| matches!(*effort, "none" | "low" | "medium" | "high" | "xhigh"))
    {
        body["reasoning_effort"] = serde_json::json!(effort);
    }
    body
}

fn completion_token_budget(messages: &[ChatMessage]) -> u32 {
    let document_action = messages.iter().any(|message| {
        let content = message.content.to_ascii_lowercase();
        content.contains("\"action\":\"vault_write\"")
            || content.contains("\"action\": \"vault_write\"")
            || content.contains("complete document")
            || content.contains("required vault tool json")
    });
    if document_action {
        CHAT_DOCUMENT_MAX_TOKENS
    } else {
        CHAT_MAX_TOKENS
    }
}

fn should_disable_reasoning_for_chat(model: &str) -> bool {
    model.to_ascii_lowercase().contains("qwen")
}

fn normalize_messages_for_model_template(
    model: &str,
    mut messages: Vec<ChatMessage>,
) -> Vec<ChatMessage> {
    if !should_disable_reasoning_for_chat(model) {
        return messages;
    }
    if messages.last().map(|m| m.role.as_str()) != Some("user") {
        messages.push(ChatMessage {
            role: "user".into(),
            content: "Continue from the context above. Reply briefly with the requested answer or action result.".into(),
        });
    }
    messages
}

fn chat_stream_idle_timeout_error() -> String {
    format!("LLM stream timed out after {CHAT_STREAM_IDLE_TIMEOUT_SECS}s without a response chunk")
}

pub(crate) fn apply_auth(req: reqwest::RequestBuilder, api_key: &Option<String>) -> reqwest::RequestBuilder {
    match api_key {
        Some(key) if !key.is_empty() => req.bearer_auth(key),
        _ => req,
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Resolve a chat provider through the same NautGate environment used by the
/// agent runtimes. Older installs may have a working NAUTGATE_API_KEY and the
/// seeded NautGate base URL without a corresponding `llm_providers` row; that
/// must not make the built-in NautBot report "provider not configured".
pub(crate) fn provider_llm(
    settings: &crate::settings::Settings,
    provider: &str,
) -> Option<crate::settings::LlmSettings> {
    let provider = provider.trim();
    if provider.is_empty() {
        return None;
    }

    if settings.llm_providers.iter().any(|p| p.name.eq_ignore_ascii_case(provider) && !p.enabled) {
        return None;
    }
    let configured = settings
        .llm_providers
        .iter()
        .find(|item| item.enabled && item.name.eq_ignore_ascii_case(provider));
    let primary = settings
        .llm
        .provider
        .eq_ignore_ascii_case(provider)
        .then_some(&settings.llm);

    if configured.is_none() && primary.is_none() && !provider.eq_ignore_ascii_case("nautgate") {
        return None;
    }

    let endpoint = configured
        .map(|item| item.endpoint.trim())
        .filter(|endpoint| !endpoint.is_empty())
        .map(str::to_string)
        .or_else(|| {
            primary
                .map(|llm| llm.endpoint.trim())
                .filter(|endpoint| !endpoint.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            provider.eq_ignore_ascii_case("nautgate").then(|| {
                non_empty_env("NAUTGATE_BASE_URL")
                    .or_else(|| non_empty_env("NAUTGATE_URL"))
                    .unwrap_or_else(|| NAUTGATE_DEFAULT_ENDPOINT.to_string())
            })
        })?;

    let configured_key = configured
        .and_then(|item| item.api_key.clone())
        .filter(|key| !key.trim().is_empty());
    let primary_key = primary
        .and_then(|llm| llm.api_key.clone())
        .filter(|key| !key.trim().is_empty());
    let api_key = configured_key.or(primary_key).or_else(|| {
        provider
            .eq_ignore_ascii_case("nautgate")
            .then(|| non_empty_env("NAUTGATE_API_KEY"))
            .flatten()
    });

    Some(crate::settings::LlmSettings {
        provider: provider.to_ascii_lowercase(),
        endpoint,
        model: primary.map(|llm| llm.model.clone()).unwrap_or_default(),
        api_key,
        system_prompt: primary.and_then(|llm| llm.system_prompt.clone()),
        harness_local: primary.is_some_and(|llm| llm.harness_local),
    })
}

/// An explicit registry switch wins over legacy primary-provider settings.
pub(crate) fn gateway_enabled(settings: &crate::settings::Settings) -> bool {
    settings.llm_providers.iter().find(|p| p.name.eq_ignore_ascii_case("nautgate"))
        .map(|p| p.enabled).unwrap_or_else(|| settings.llm.provider.eq_ignore_ascii_case("nautgate"))
}

/// Apply transport policy without changing the requested model or prompt.
/// No network fallback is permitted: a gateway failure stays a gateway failure.
pub(crate) fn route_llm(settings: &crate::settings::Settings, requested: &crate::settings::LlmSettings)
    -> Result<crate::settings::LlmSettings, String> {
    if gateway_enabled(settings) {
        let mut route = provider_llm(settings, "nautgate").ok_or("NautGate is enabled but not configured")?;
        route.model = requested.model.clone();
        route.system_prompt = requested.system_prompt.clone();
        route.harness_local = false;
        return Ok(route);
    }
    if settings.llm_providers.iter().any(|p| p.name.eq_ignore_ascii_case(&requested.provider) && !p.enabled) {
        return Err(format!("Provider {} is disabled. Select a configured direct provider in Settings.", requested.provider));
    }
    Ok(requested.clone())
}

pub(crate) fn selected_llm(settings: &crate::settings::Settings, provider: &str)
    -> Result<crate::settings::LlmSettings, String> {
    if gateway_enabled(settings) {
        // The provider need not have local credentials when the gateway owns them.
        return route_llm(settings, &settings.llm);
    }
    let requested = if provider.trim().is_empty() || provider == "global" {
        settings.llm.clone()
    } else { provider_llm(settings, provider).ok_or_else(|| format!("LLM provider is not configured or is disabled: {provider}"))? };
    route_llm(settings, &requested)
}

/// One-shot completion, non-streaming. Used by AI commit messages and PR title/body.
pub async fn complete_oneshot(
    llm: &crate::settings::LlmSettings,
    system: Option<&str>,
    user: &str,
) -> Result<String, String> {
    Ok(complete_oneshot_with_usage(llm, system, user)
        .await?
        .content)
}

pub async fn complete_oneshot_with_usage(
    llm: &crate::settings::LlmSettings,
    system: Option<&str>,
    user: &str,
) -> Result<CompletionResult, String> {
    let routed = route_llm(&crate::settings::load_or_default(), llm)?;
    let llm = &routed;
    let mut messages = Vec::new();
    if let Some(sys) = system {
        messages.push(serde_json::json!({"role": "system", "content": sys}));
    }
    messages.push(serde_json::json!({"role": "user", "content": user}));

    // Non-streaming, but full-document generation on a local model is slow —
    // generous overall cap, fail fast only on an unreachable endpoint.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;

    if crate::responses::required(&llm.model) {
        let answer = crate::responses::Session::default().request(&client, llm, &llm.model, &messages, &[], None, 8192, None).await?;
        return Ok(CompletionResult {
            content: answer.message["content"].as_str().unwrap_or("").trim().into(),
            input_tokens: answer.usage["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: answer.usage["output_tokens"].as_u64().unwrap_or(0),
        });
    }

    let url = join_endpoint(&llm.endpoint, "chat/completions");
    let req = apply_auth(client.post(&url), &llm.api_key).json(&serde_json::json!({
        "model": llm.model,
        "messages": messages,
        "stream": false,
    }));

    let resp = req
        .send()
        .await
        .map_err(|e| format!("LLM request to {url} failed: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("failed to read LLM response body: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "LLM request failed ({status}): {}",
            body_excerpt(&body)
        ));
    }

    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("invalid JSON from LLM ({e}): {}", body_excerpt(&body)))?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| {
            format!(
                "LLM response missing choices[0].message.content: {}",
                body_excerpt(&body)
            )
        })?;
    Ok(CompletionResult {
        content,
        input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
    })
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn chat_send(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    request_id: String,
    messages: Vec<ChatMessage>,
    reasoning_effort: Option<String>,
) -> Result<String, String> {
    let settings = state.settings.lock().await.clone();
    chat_send_with_settings(app, settings, request_id, messages, None, reasoning_effort).await
}

#[tauri::command]
pub async fn chat_send_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    request_id: String,
    model: String,
    messages: Vec<ChatMessage>,
    reasoning_effort: Option<String>,
) -> Result<String, String> {
    let settings = state.settings.lock().await.clone();
    let model = model.trim().to_string();
    let model_override = if model.is_empty() { None } else { Some(model) };
    chat_send_with_settings(
        app,
        settings,
        request_id,
        messages,
        model_override,
        reasoning_effort,
    )
    .await
}

#[tauri::command]
pub async fn chat_send_provider(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    request_id: String,
    provider: String,
    model: String,
    messages: Vec<ChatMessage>,
    reasoning_effort: Option<String>,
) -> Result<String, String> {
    let mut settings = state.settings.lock().await.clone();
    let provider = provider.trim();
    let configured = selected_llm(&settings, provider)?;
    settings.llm.provider = configured.provider;
    settings.llm.endpoint = configured.endpoint;
    settings.llm.api_key = configured.api_key;
    settings.llm.model = model.trim().to_string();
    if settings.llm.model.is_empty() {
        return Err("model is required".into());
    }
    chat_send_with_settings(app, settings, request_id, messages, None, reasoning_effort).await
}

/// Resolve the caller's provider and pin it into `settings`.
///
/// Pinning matters more than resolving. `chat_send_tools` has two
/// plain-completion fallbacks and both read `settings.llm`, so returning the
/// provider without writing it back sent every fallback to the global default.
/// A pane showing "nautgate . auto" timed out against LM Studio on :1238 and
/// named a model the owner never picked. Same shape as `chat_send_provider`.
pub(crate) fn pin_provider(
    settings: &mut crate::settings::Settings,
    provider_name: &str,
) -> Result<crate::settings::LlmSettings, String> {
    let llm = selected_llm(settings, provider_name)?;
    settings.llm = llm.clone();
    Ok(llm)
}

/// The chat pane's turn, with xNAUT's tools attached (XNAUT-194).
///
/// The pane called `chat_send`, which posts messages and nothing else, while
/// Agent Space went through `agent_tools::run_turn` and got the whole tool
/// list. Same agent, two surfaces, different abilities — and the model reports
/// that as "the tool isn't available", which reads as a missing feature. It is
/// the same shape as the dictation microphone existing in only one composer.
///
/// Three paths, in order:
///   * a route already known not to carry tool calls goes straight to the
///     streaming completion, because a round trip we know will 502 is worse
///     than no round trip (XNAUT-196 remembers the answer);
///   * otherwise the tool loop runs;
///   * and if it fails anyway, the streaming completion answers and the reply
///     SAYS the tools were lost. Silence there cost four days (XNAUT-195).
///
/// All three paths stream now, on the same `chat://chunk` event, so the pane
/// paints a tool turn the way it paints any other one (XNAUT-159). The text a
/// tool round emits before it calls anything shows while the tools run; the
/// returned string is still the authoritative answer.
#[tauri::command]
pub async fn chat_send_tools(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    request_id: String,
    messages: Vec<ChatMessage>,
    provider: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    // chat_key is the CONVERSATION's identity, not this turn's. The canvas is
    // keyed by it, so a diagram drawn on Monday is the one the agent reads on
    // Tuesday. Optional so older callers keep working; they fall back to the
    // request id and get the old orphaning behaviour rather than an error.
    chat_key: Option<String>,
) -> Result<String, String> {
    let mut settings = state.settings.lock().await.clone();
    let provider_name = provider.clone().unwrap_or_default();
    let llm = pin_provider(&mut settings, &provider_name)?;
    let chosen = model
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| llm.model.clone());

    let known_bad = crate::tool_support::known(&llm.provider, &chosen)
        .filter(|support| !support.supported)
        .map(|support| support.reason);

    if (known_bad.is_none() || crate::responses::required(&chosen)) && !chosen.is_empty() {
        let mut with_model = llm.clone();
        with_model.model = chosen.clone();
        let history: Vec<serde_json::Value> = messages
            .iter()
            .map(|message| serde_json::json!({ "role": message.role, "content": message.content }))
            .collect();
        // The canvas key must outlive the request. `request_id` is a fresh UUID
        // per turn, so keying the canvas by it wrote every drawing to a file
        // nobody could ever open again — the agent reported success, the owner
        // saw nothing, and `canvas_get(handle)` found an empty graph. The
        // conversation key is the stable identity the frontend already uses for
        // history; the canvas belongs to the same conversation.
        let canvas_key = chat_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&request_id);
        // Same `chat://chunk` the plain completion below emits, so the panel
        // paints a tool turn exactly as it paints an ordinary one (XNAUT-159).
        match crate::agent_tools::run_turn_streaming(
            &with_model,
            &chosen,
            history,
            reasoning_effort.as_deref(),
            &[],
            canvas_key,
            Some((&app, &request_id)),
        )
        .await
        {
            Ok(outcome) => return Ok(outcome.text),
            Err(error) => {
                if crate::responses::required(&chosen) { return Err(error); }
                let _ = crate::debug_log::debug_log_append(vec![format!(
                    "[chat_send_tools] tool loop unavailable, falling back to a plain completion: {error}"
                )]);
                let reply = chat_send_with_settings(
                    app,
                    settings,
                    request_id,
                    messages,
                    Some(chosen.clone()),
                    reasoning_effort,
                )
                .await?;
                return Ok(format!(
                    "{reply}{}",
                    crate::agent_profiles::tool_failure_notice(&chosen, &error)
                ));
            }
        }
    }

    let reply = chat_send_with_settings(
        app,
        settings,
        request_id,
        messages,
        Some(chosen.clone()),
        reasoning_effort,
    )
    .await?;
    Ok(match known_bad {
        Some(reason) => format!(
            "{reply}{}",
            crate::agent_profiles::tool_failure_notice(&chosen, &reason)
        ),
        None => reply,
    })
}

async fn chat_send_with_settings(
    app: tauri::AppHandle,
    settings: crate::settings::Settings,
    request_id: String,
    messages: Vec<ChatMessage>,
    model_override: Option<String>,
    reasoning_effort: Option<String>,
) -> Result<String, String> {
    let routed = route_llm(&settings, &settings.llm)?;
    let llm = &routed;
    let model = model_override.as_deref().unwrap_or(&llm.model);

    // Build the outgoing message list: [system_prompt?, brain?, ...messages].
    let mut outgoing: Vec<ChatMessage> = Vec::new();
    if let Some(sys) = &llm.system_prompt {
        if !sys.is_empty() {
            outgoing.push(ChatMessage {
                role: "system".into(),
                content: sys.clone(),
            });
        }
    }

    // Engram brain: search long-term memories on the last user message.
    // Failures fall back silently — chat must work without the brain.
    let mut brain_count: Option<usize> = None;
    if settings.engram.enabled && !settings.engram.url.is_empty() {
        if let Some(last_user) = messages.iter().rev().find(|m| m.role == "user") {
            if let Ok(memories) =
                crate::engram::search(&settings.engram.url, &last_user.content, 8, None).await
            {
                if !memories.is_empty() {
                    let bullets = memories
                        .iter()
                        .map(|m| format!("- {}", m.content))
                        .collect::<Vec<_>>()
                        .join("\n");
                    outgoing.push(ChatMessage {
                        role: "system".into(),
                        content: format!("Relevant long-term memories (Engram):\n{bullets}"),
                    });
                    brain_count = Some(memories.len());
                }
            }
        }
    }
    outgoing.extend(messages);

    // Streaming: no overall timeout (a slow local model generating a long
    // reply can take minutes). Fail fast only if the endpoint is unreachable.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;

    if crate::responses::required(model) {
        let input: Vec<_> = outgoing.iter().map(|m| serde_json::json!({"role":m.role,"content":m.content})).collect();
        let answer = crate::responses::Session::default().request(&client, llm, model, &input, &[], reasoning_effort.as_deref(), 8192, Some((&app, &request_id))).await?;
        let mut text = answer.message["content"].as_str().unwrap_or("").to_string();
        if let Some(notice) = answer.receipt.substitution_notice() { text = format!("{notice}\n\n{text}"); }
        if let Some(count) = brain_count { let _ = app.emit("chat://brain", serde_json::json!({"requestId":request_id,"count":count})); }
        let _ = app.emit("chat://done", serde_json::json!({"requestId":request_id}));
        return Ok(text);
    }

    let url = join_endpoint(&llm.endpoint, "chat/completions");
    let mut body = streaming_request_body(model, outgoing, reasoning_effort.as_deref());
    let mut attempt = 0;
    let (resp, receipt) = loop {
        let resp = apply_auth(client.post(&url), &llm.api_key).json(&body).send().await
            .map_err(|e| format!("LLM request to {url} failed: {e}"))?;
        let status = resp.status();
        let receipt = NautGateReceipt::from_headers(resp.headers());
        if status.is_success() { break (resp, receipt); }
        let error = resp.text().await.unwrap_or_default();
        if attempt < 2 && adapt_rejected_request(&mut body, &error) { attempt += 1; continue; }
        return Err(format!("LLM request failed ({status}): {}{}",body_excerpt(&error),receipt.error_suffix()));
    };

    // Parse the SSE stream. Events can span chunk boundaries, so keep a byte
    // buffer and only consume complete newline-terminated lines.
    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut full = String::new();
    if let Some(notice) = receipt.substitution_notice() {
        full.push_str(&notice);
        full.push_str("\n\n");
        app.emit(
            "chat://chunk",
            serde_json::json!({"requestId": request_id, "delta": format!("{notice}\n\n")}),
        )
        .map_err(|e| format!("failed to emit NautGate receipt: {e}"))?;
    }
    let mut done = false;

    loop {
        let Some(chunk) = tokio::time::timeout(
            std::time::Duration::from_secs(CHAT_STREAM_IDLE_TIMEOUT_SECS),
            stream.next(),
        )
        .await
        .map_err(|_| chat_stream_idle_timeout_error())?
        else {
            break;
        };
        let chunk = chunk.map_err(|e| format!("LLM stream error: {e}"))?;
        buf.extend_from_slice(&chunk);

        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            let line = line.trim_end_matches(['\n', '\r']);
            let Some(data) = sse_data(line) else { continue };
            if data == "[DONE]" {
                done = true;
                break;
            }
            if let Some(delta) = delta_content(data) {
                if !delta.is_empty() {
                    full.push_str(&delta);
                    app.emit(
                        "chat://chunk",
                        serde_json::json!({"requestId": request_id, "delta": delta}),
                    )
                    .map_err(|e| format!("failed to emit chat://chunk: {e}"))?;
                }
            }
        }
        if done {
            break;
        }
    }

    if let Some(count) = brain_count {
        let _ = app.emit(
            "chat://brain",
            serde_json::json!({"requestId": request_id, "count": count}),
        );
    }
    app.emit("chat://done", serde_json::json!({"requestId": request_id}))
        .map_err(|e| format!("failed to emit chat://done: {e}"))?;

    Ok(full)
}

#[tauri::command]
pub async fn chat_check_endpoint(
    state: tauri::State<'_, crate::state::AppState>,
    provider: Option<String>,
) -> Result<bool, String> {
    let settings = state.settings.lock().await.clone();
    let llm = selected_llm(&settings, provider.as_deref().unwrap_or(""))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;

    let url = join_endpoint(&llm.endpoint, "models");
    match apply_auth(client.get(&url), &llm.api_key).send().await {
        Ok(resp) => Ok(resp.status().is_success()),
        // Connection refused / timeout = endpoint down, not an app error.
        Err(_) => Ok(false),
    }
}

#[tauri::command]
pub async fn chat_list_models(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<String>, String> {
    let settings = state.settings.lock().await.clone();
    let llm = selected_llm(&settings, "")?;
    list_models_for(&llm).await
}

async fn list_models_for(llm: &crate::settings::LlmSettings) -> Result<Vec<String>, String> {
    Ok(list_model_entries_for(llm).await?.into_iter().map(|entry| entry.model).collect())
}

fn model_entries(value: &serde_json::Value, provider: &str) -> Result<Vec<ProviderModel>, String> {
    let data = value["data"].as_array().ok_or("Model catalog response has no data array")?;
    let mut models: Vec<_> = data.iter().filter_map(|item| {
        let id = item["id"].as_str()?.trim();
        if id.is_empty() { return None; }
        let label = ["nautgate_display", "name", "display_name"].iter()
            .filter_map(|key| item[*key].as_str().map(str::trim))
            .find(|name| !name.is_empty()).unwrap_or(id);
        Some(ProviderModel { provider: provider.into(), model: id.into(), label: label.into() })
    }).collect();
    models.sort_by(|a, b| a.model.cmp(&b.model));
    models.dedup_by(|a, b| a.model == b.model);
    Ok(models)
}

async fn list_model_entries_for(llm: &crate::settings::LlmSettings) -> Result<Vec<ProviderModel>, String> {
    if llm.endpoint.trim().is_empty() {
        return Ok(Vec::new());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;
    let url = join_endpoint(&llm.endpoint, "models");
    let response = apply_auth(client.get(&url), &llm.api_key)
        .send()
        .await
        .map_err(|e| format!("model list request to {url} failed: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("failed to read model list: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "model list request failed ({status}): {}",
            body_excerpt(&body)
        ));
    }
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("invalid model list JSON ({e}): {}", body_excerpt(&body)))?;
    model_entries(&value, &llm.provider)
}

#[tauri::command]
pub async fn chat_list_provider_models(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<ProviderModel>, String> {
    let settings = state.settings.lock().await.clone();
    let mut providers = settings
        .llm_providers
        .iter()
        .cloned()
        .filter(|provider| provider.enabled && !provider.endpoint.trim().is_empty())
        .collect::<Vec<_>>();
    if !settings.llm.provider.trim().is_empty()
        && !providers
            .iter()
            .any(|item| item.name == settings.llm.provider)
    {
        providers.push(crate::settings::LlmProviderSettings {
            name: settings.llm.provider.clone(),
            endpoint: settings.llm.endpoint.clone(),
            api_key: settings.llm.api_key.clone(),
            enabled: true,
        });
    }
    providers.retain(|p| !settings.llm_providers.iter().any(|r| r.name.eq_ignore_ascii_case(&p.name) && !r.enabled));
    if gateway_enabled(&settings) {
        let gateway = selected_llm(&settings, "nautgate")?;
        providers = vec![crate::settings::LlmProviderSettings { name: gateway.provider, endpoint: gateway.endpoint, api_key: gateway.api_key, enabled: true }];
    }

    let requests = providers.into_iter().map(|provider| async move {
        let name = provider.name;
        let llm = crate::settings::LlmSettings {
            provider: name.clone(),
            endpoint: provider.endpoint,
            model: String::new(),
            api_key: provider.api_key,
            system_prompt: None,
            harness_local: false,
        };
        (name, list_model_entries_for(&llm).await)
    });
    let mut result = Vec::new();
    let mut failed = Vec::new();
    let mut succeeded = 0;
    for (name, models) in futures_util::future::join_all(requests).await {
        match models {
            Ok(models) => { succeeded += 1; result.extend(models); }
            Err(_) => failed.push(name),
        }
    }
    // An outage is not an authoritative empty catalog. Let the frontend retain
    // its last successful result and timestamp instead of marking it fresh.
    if succeeded == 0 && !failed.is_empty() {
        return Err(format!("Model catalog refresh failed for {}", failed.join(", ")));
    }
    result.sort_by(|a, b| (&a.provider, &a.model).cmp(&(&b.provider, &b.model)));
    result.dedup_by(|a, b| a.provider == b.provider && a.model == b.model);
    Ok(result)
}

/// Generic localhost-service reachability probe for the settings Test buttons.
/// Runs from Rust so webview CORS policies (LM Studio, Ollama) can't mask a
/// healthy server as unreachable. Restricted to loopback hosts on purpose.
#[tauri::command]
pub async fn net_probe(url: String) -> Result<bool, String> {
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("invalid url: {e}"))?;
    match parsed.host_str() {
        Some("localhost") | Some("127.0.0.1") | Some("0.0.0.0") => {}
        _ => return Err("net_probe only accepts localhost URLs".into()),
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;
    match client.get(parsed).send().await {
        Ok(resp) => Ok(resp.status().is_success()),
        Err(_) => Ok(false),
    }
}

/// Localhost-only JSON fetch for the legacy AI panel + model dropdowns.
/// Same rationale as `net_probe`: the webview's CSP/CORS can't veto Rust,
/// so local providers (LM Studio, Ollama) are reachable from settings UI.
#[tauri::command]
pub async fn net_fetch_json(
    url: String,
    method: Option<String>,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("invalid url: {e}"))?;
    match parsed.host_str() {
        Some("localhost") | Some("127.0.0.1") | Some("0.0.0.0") => {}
        _ => return Err("net_fetch_json only accepts localhost URLs".into()),
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;
    let req = if method
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("post"))
    {
        client
            .post(parsed)
            .json(&body.unwrap_or(serde_json::Value::Null))
    } else {
        client.get(parsed)
    };
    let resp = req
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("read failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("{status}: {}", &text[..text.len().min(300)]));
    }
    serde_json::from_str(&text).map_err(|e| format!("invalid JSON response: {e}"))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_catalog_preserves_provider_display_names_and_exact_request_ids() {
        let entries = model_entries(&serde_json::json!({"data":[
            {"id":"vendor/model-v2", "name":"Vendor Model Two"},
            {"id":"gateway/model", "nautgate_display":"Gateway Model", "name":"Ignored"},
            {"id":"plain-model"}, {"id":" "}
        ]}), "fixture").unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].model, "gateway/model");
        assert_eq!(entries[0].label, "Gateway Model");
        assert_eq!(entries[1].label, "plain-model");
        assert_eq!(entries[2].label, "Vendor Model Two");
        assert!(model_entries(&serde_json::json!({"error":"unavailable"}), "fixture").is_err());
        assert!(model_entries(&serde_json::json!({"data":[]}), "fixture").unwrap().is_empty());
    }

    /// The screenshot that started this: a composer labelled "nautgate . auto"
    /// timing out against http://localhost:1238 with a model nobody selected.
    /// The provider was resolved and then thrown away, so both of
    /// `chat_send_tools`'s fallbacks answered from the global default.
    #[test]
    fn gateway_policy_preserves_selection_and_never_borrows_direct_credentials() {
        let mut settings = crate::settings::Settings::default();
        settings.llm = crate::settings::LlmSettings { provider:"openai".into(), endpoint:"https://direct.invalid/v1".into(), api_key:Some("direct-key".into()), model:"gpt-6-astra".into(), ..Default::default() };
        settings.llm_providers.push(crate::settings::LlmProviderSettings { name:"nautgate".into(), endpoint:"http://gateway.invalid/v1".into(), api_key:Some("gateway-key".into()), enabled:true });
        let routed = route_llm(&settings, &settings.llm).unwrap();
        assert_eq!(routed.endpoint,"http://gateway.invalid/v1");
        assert_eq!(routed.api_key.as_deref(),Some("gateway-key"));assert_eq!(routed.model,"gpt-6-astra");
        assert_eq!(selected_llm(&settings,"unconfigured-cloud-account").unwrap().provider,"nautgate");
        settings.llm_providers[0].enabled=false;
        let direct=selected_llm(&settings,"openai").unwrap();
        assert_eq!(direct.endpoint,"https://direct.invalid/v1");assert_eq!(direct.api_key.as_deref(),Some("direct-key"));
        assert!(provider_llm(&settings,"nautgate").is_none());
        assert!(selected_llm(&settings,"nautgate").is_err());
        settings.llm.provider="nautgate".into();
        assert!(!gateway_enabled(&settings));assert!(selected_llm(&settings,"global").is_err());
    }

    #[test]
    fn a_provider_override_survives_into_the_fallback_settings() {
        let mut settings = crate::settings::Settings::default();
        settings.llm.provider = "lmstudio".into();
        settings.llm.endpoint = "http://localhost:1238/v1".into();
        settings.llm.model = "google/gemma-4-e4b".into();
        settings.llm_providers.push(crate::settings::LlmProviderSettings {
            name: "nautgate".into(),
            endpoint: "http://localhost:8090/v1".into(),
            api_key: Some("ng_test".into()),
            enabled: true,
        });

        let picked = pin_provider(&mut settings, "nautgate").expect("nautgate is configured");
        assert_eq!(picked.endpoint, "http://localhost:8090/v1");
        assert_eq!(
            settings.llm.endpoint, "http://localhost:8090/v1",
            "the fallback completion reads settings.llm, so leaving it on the global endpoint sends the retry to LM Studio"
        );

        let mut untouched = settings.clone();
        untouched.llm.endpoint = "http://localhost:8090/v1".into();
        let global = pin_provider(&mut untouched, "").expect("an empty provider means the global one");
        assert_eq!(global.endpoint, "http://localhost:8090/v1");

        assert_eq!(pin_provider(&mut settings, "nope").unwrap().provider, "nautgate");
        settings.llm_providers[0].enabled = false;
        assert!(pin_provider(&mut settings, "nope").is_err());
    }

    #[test]
    fn joins_endpoint_with_and_without_trailing_slash() {
        assert_eq!(
            join_endpoint("http://localhost:8090/v1/", "chat/completions"),
            "http://localhost:8090/v1/chat/completions"
        );
        assert_eq!(
            join_endpoint("http://localhost:8090/v1", "chat/completions"),
            "http://localhost:8090/v1/chat/completions"
        );
        assert_eq!(
            join_endpoint("http://localhost:11434/v1", "models"),
            "http://localhost:11434/v1/models"
        );
    }

    #[test]
    fn nautgate_receipt_turns_substitution_headers_into_a_thread_notice() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-nautgate-decision-id", "dec-123".parse().unwrap());
        headers.insert("x-nautgate-requested-model", "claude-opus-5".parse().unwrap());
        headers.insert("x-nautgate-model", "claude-sonnet-4-6".parse().unwrap());
        headers.insert("x-nautgate-observed-model", "claude-haiku-4-5".parse().unwrap());
        headers.insert("x-nautgate-substituted", "true".parse().unwrap());

        let receipt = NautGateReceipt::from_headers(&headers);
        let notice = receipt.substitution_notice().unwrap();
        assert!(notice.contains("claude-opus-5"), "{notice}");
        assert!(notice.contains("claude-haiku-4-5"), "{notice}");
        assert!(notice.contains("dec-123"), "{notice}");
    }

    #[test]
    fn nautgate_error_context_keeps_decision_and_upstream_status() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-nautgate-decision-id", "dec-400".parse().unwrap());
        headers.insert("x-nautgate-upstream-status", "400".parse().unwrap());
        let suffix = NautGateReceipt::from_headers(&headers).error_suffix();
        assert!(suffix.contains("dec-400"), "{suffix}");
        assert!(suffix.contains("upstream 400"), "{suffix}");
    }

    #[test]
    fn nautgate_receipt_carries_both_ids_and_only_records_when_a_gateway_answered() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-nautgate-decision-id", "dec-7".parse().unwrap());
        headers.insert("x-nautgate-receipt-id", "rcpt-7".parse().unwrap());
        let receipt = NautGateReceipt::from_headers(&headers);
        assert_eq!(receipt.decision_id.as_deref(), Some("dec-7"));
        assert_eq!(receipt.receipt_id.as_deref(), Some("rcpt-7"));

        let body = receipt.evidence_body().expect("both ids belong in the record");
        assert_eq!(body["nautgate"]["decision_id"], "dec-7");
        assert_eq!(body["nautgate"]["receipt_id"], "rcpt-7");

        // No gateway on the wire is not an empty nautgate object: that would
        // claim one was involved.
        let bare = NautGateReceipt::from_headers(&reqwest::header::HeaderMap::new());
        assert!(bare.evidence_body().is_none());
    }

    #[test]
    fn nautgate_is_a_builtin_provider_without_a_persisted_registry_row() {
        let settings = crate::settings::Settings::default();
        let nautgate = provider_llm(&settings, "nautgate").unwrap();
        assert_eq!(nautgate.provider, "nautgate");
        assert!(!nautgate.endpoint.is_empty());
    }

    #[test]
    fn configured_nautgate_takes_precedence_over_the_builtin_endpoint() {
        let mut settings = crate::settings::Settings::default();
        settings
            .llm_providers
            .push(crate::settings::LlmProviderSettings {
                name: "NautGate".into(),
                endpoint: "http://127.0.0.1:9999/v1".into(),
                api_key: Some("configured-key".into()),
                enabled: true,
            });
        let nautgate = provider_llm(&settings, "nautgate").unwrap();
        assert_eq!(nautgate.endpoint, "http://127.0.0.1:9999/v1");
        assert_eq!(nautgate.api_key.as_deref(), Some("configured-key"));
    }

    #[test]
    fn sse_data_strips_prefix_and_ignores_other_lines() {
        assert_eq!(sse_data("data: {\"x\":1}"), Some("{\"x\":1}"));
        assert_eq!(sse_data("data: [DONE]"), Some("[DONE]"));
        assert_eq!(sse_data(": keep-alive comment"), None);
        assert_eq!(sse_data("event: message"), None);
        assert_eq!(sse_data(""), None);
    }

    #[test]
    fn extracts_delta_content_from_stream_chunk() {
        let chunk = r#"{"id":"x","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        assert_eq!(delta_content(chunk), Some("Hello".to_string()));

        // Role-only first chunk and finish chunk carry no content.
        let role_only = r#"{"choices":[{"delta":{"role":"assistant"}}]}"#;
        assert_eq!(delta_content(role_only), None);
        let finish = r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        assert_eq!(delta_content(finish), None);
        assert_eq!(delta_content("not json"), None);
    }

    #[test]
    fn body_excerpt_truncates_long_bodies() {
        let long = "x".repeat(1000);
        let out = body_excerpt(&long);
        assert!(out.len() <= 304); // 300 bytes + multi-byte ellipsis
        assert!(out.ends_with('…'));
        assert_eq!(body_excerpt("  short  "), "short");
    }

    #[test]
    fn streaming_request_body_caps_completion_tokens() {
        let body = streaming_request_body("qwen/qwen3.6-35b-a3b", Vec::new(), None);
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], CHAT_MAX_TOKENS);
    }

    #[test]
    fn streaming_request_body_allows_complete_vault_documents() {
        let body = streaming_request_body(
            "openai/gpt-4o",
            vec![ChatMessage {
                role: "system".into(),
                content: r#"Reply with {"action":"vault_write","rel":"doc.md","content":"COMPLETE DOCUMENT"}."#.into(),
            }],
            None,
        );
        assert_eq!(body["max_tokens"], CHAT_DOCUMENT_MAX_TOKENS);
    }

    #[test]
    fn streaming_request_body_disables_reasoning_tokens_for_chat_actions() {
        let body = streaming_request_body("qwen/qwen3.6-35b-a3b", Vec::new(), Some("high"));
        assert_eq!(body["reasoning_effort"], "none");
    }

    #[test]
    fn streaming_request_body_does_not_send_qwen_reasoning_flag_to_other_models() {
        let body = streaming_request_body("google/gemma-4-12b-qat", Vec::new(), None);
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn streaming_request_body_forwards_requested_reasoning_effort() {
        let body = streaming_request_body("gpt-5.6-sol", Vec::new(), Some("high"));
        assert_eq!(body["reasoning_effort"], "high");
    }

    #[test]
    fn qwen_streaming_request_body_appends_user_query_after_trailing_system_message() {
        let body = streaming_request_body(
            "qwen/qwen3.6-35b-a3b",
            vec![
                ChatMessage {
                    role: "user".into(),
                    content: "create a note".into(),
                },
                ChatMessage {
                    role: "system".into(),
                    content: "VAULT TOOL RESULTS:\nCREATED Templates/Test.md.".into(),
                },
            ],
            None,
        );
        let messages = body["messages"].as_array().unwrap();
        let last = messages.last().unwrap();
        assert_eq!(last["role"], "user");
        assert!(last["content"].as_str().unwrap().contains("Continue"));
    }

    #[test]
    fn non_qwen_streaming_request_body_keeps_original_message_order() {
        let body = streaming_request_body(
            "google/gemma-4-12b-qat",
            vec![ChatMessage {
                role: "system".into(),
                content: "context only".into(),
            }],
            None,
        );
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "system");
    }

    #[test]
    fn chat_stream_timeout_error_mentions_idle_limit() {
        let err = chat_stream_idle_timeout_error();
        assert!(err.contains("timed out"));
        assert!(err.contains(&CHAT_STREAM_IDLE_TIMEOUT_SECS.to_string()));
    }
}
