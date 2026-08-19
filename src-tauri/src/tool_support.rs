// Can this model actually call a tool? (XNAUT-196)
//
// The model picker lists 491 models from the gateway and said nothing about
// which of them can run a tool call. Picking one was a coin flip, and losing
// looked like this: the agent answers in prose, says the tool "isn't
// available", and asks you to enable something that was never off.
//
// That took four days to find (XNAUT-195). The check that found it took one
// request: send a trivial `tools` payload and see whether it 200s. So the app
// does that itself now, and puts the answer next to the model's name.
//
// What this is NOT: a judgement about the model's ability. It is a probe of the
// ROUTE. The same model id answers differently through a subscription relay
// than through an API key, and it is the route that breaks.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A probe costs a request, so the answer is kept. Short enough that fixing a
/// billing page or an API key is visible without a restart, long enough that a
/// picker with 491 rows never storms the gateway.
const TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ToolSupport {
    pub provider: String,
    pub model: String,
    /// True only when the route answered 2xx to a request carrying tools.
    pub supported: bool,
    /// The upstream's own words when it did not. This is the sentence that
    /// tells you whether to open a billing page or change a key.
    pub reason: String,
}

static CACHE: Mutex<Option<HashMap<String, (ToolSupport, Instant)>>> = Mutex::new(None);

fn cache_key(provider: &str, model: &str) -> String {
    format!("{}::{}", provider.trim().to_lowercase(), model.trim())
}

fn cached(provider: &str, model: &str, now: Instant) -> Option<ToolSupport> {
    let mut guard = CACHE.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    map.retain(|_, (_, at)| now.duration_since(*at) < TTL);
    map.get(&cache_key(provider, model)).map(|(support, _)| support.clone())
}

fn remember(support: &ToolSupport, now: Instant) {
    if let Ok(mut guard) = CACHE.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(cache_key(&support.provider, &support.model), (support.clone(), now));
    }
}

/// Drop everything, so a fixed credential is visible without waiting out the TTL.
pub fn forget_all() {
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(HashMap::new());
    }
}

/// The smallest request that proves a route can carry tools.
///
/// One function, no arguments, one word of prompt, and `reasoning_effort:
/// "none"` because tools plus an effort setting is its own 502 on this gateway
/// (see agent_tools.rs). What matters is the status code, not the answer.
pub fn probe_body(model: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": "ping" }],
        "reasoning_effort": "none",
        "max_tokens": 16,
        "tools": [{
            "type": "function",
            "function": {
                "name": "ping",
                "description": "Answer the ping.",
                "parameters": { "type": "object", "properties": {} }
            }
        }]
    })
}

/// The sentence an upstream returned, dug out of whichever envelope it used.
///
/// Gateways disagree about where the message goes — `error.message`, `detail`,
/// or nothing at all — and the message is the whole value of the probe. A
/// status code alone tells you it failed; the sentence tells you it is a
/// billing page.
pub fn upstream_reason(status: u16, payload: &serde_json::Value) -> String {
    let detail = payload
        .pointer("/error/message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| payload.get("detail").and_then(serde_json::Value::as_str))
        .or_else(|| payload.get("message").and_then(serde_json::Value::as_str))
        .unwrap_or("")
        .trim();
    if detail.is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status}: {detail}")
    }
}

/// Probe one provider+model. Cached; pass `refresh` to ignore what is stored.
#[tauri::command]
pub async fn model_tool_support(
    state: tauri::State<'_, crate::state::AppState>,
    provider: Option<String>,
    model: String,
    refresh: Option<bool>,
) -> Result<ToolSupport, String> {
    let provider = provider.unwrap_or_default();
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("a model is required".to_string());
    }
    let now = Instant::now();
    if !refresh.unwrap_or(false) {
        if let Some(hit) = cached(&provider, &model, now) {
            return Ok(hit);
        }
    }

    let settings = state.settings.lock().await.clone();
    // An empty provider means "whatever the app is set to", which is what the
    // chat pane uses and therefore what has to be probeable.
    let llm = if provider.trim().is_empty() || provider == "global" {
        settings.llm.clone()
    } else {
        crate::chat::provider_llm(&settings, &provider)
            .ok_or_else(|| format!("no provider configured called {provider}"))?
    };
    let url = crate::chat::join_endpoint(&llm.endpoint, "chat/completions");
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|error| format!("failed to build http client: {error}"))?;

    let response = crate::chat::apply_auth(client.post(&url), &llm.api_key)
        .json(&probe_body(&model))
        .send()
        .await
        .map_err(|error| format!("probe request failed: {error}"))?;
    let status = response.status();
    let payload: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);

    let support = ToolSupport {
        provider: provider.clone(),
        model: model.clone(),
        supported: status.is_success(),
        reason: if status.is_success() {
            String::new()
        } else {
            upstream_reason(status.as_u16(), &payload)
        },
    };
    remember(&support, now);
    Ok(support)
}

/// Forget every probe, so a fixed key or a topped-up balance shows immediately.
#[tauri::command]
pub fn model_tool_support_reset() {
    forget_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// The probe has to CARRY tools, or it proves nothing. A payload that lost
    /// its tools array would answer 200 on exactly the routes this exists to
    /// catch, and every model would be labelled fine.
    fn the_probe_actually_asks_for_a_tool() {
        let body = probe_body("gpt-5.6-sol");
        let tools = body.get("tools").and_then(serde_json::Value::as_array).expect("tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(body.pointer("/tools/0/function/name").and_then(serde_json::Value::as_str), Some("ping"));
        // Tools plus an effort setting is its own 502 on this gateway.
        assert_eq!(body.get("reasoning_effort").and_then(serde_json::Value::as_str), Some("none"));
    }

    #[test]
    /// The status code says it failed; the sentence says whether the fix is a
    /// billing page, a key, or a different model. Losing it makes the whole
    /// feature a red dot.
    fn the_upstream_sentence_survives_whichever_envelope_it_arrives_in() {
        let openai = serde_json::json!({"error": {"message": "Your credit balance is too low"}});
        assert!(upstream_reason(400, &openai).contains("credit balance is too low"));

        let gateway = serde_json::json!({"detail": "upstream_failed (502): tool calls are not supported by this transport"});
        assert!(upstream_reason(502, &gateway).contains("not supported by this transport"));

        let plain = serde_json::json!({"message": "no working provider credential"});
        assert!(upstream_reason(502, &plain).contains("no working provider credential"));

        // Nothing usable: still say what happened rather than nothing.
        assert_eq!(upstream_reason(503, &serde_json::Value::Null), "HTTP 503");
    }

    #[test]
    fn the_cache_answers_within_the_window_and_forgets_on_reset() {
        forget_all();
        let now = Instant::now();
        let support = ToolSupport {
            provider: "nautgate".into(),
            model: "gpt-5.6-sol".into(),
            supported: false,
            reason: "HTTP 502: tool calls are not supported by this transport".into(),
        };
        remember(&support, now);
        assert_eq!(cached("nautgate", "gpt-5.6-sol", now), Some(support.clone()));
        // Case and padding must not miss the entry, or the picker probes twice.
        assert_eq!(cached("NautGate", " gpt-5.6-sol ", now), Some(support));
        // Past the window it is stale: a topped-up balance has to become visible.
        assert_eq!(cached("nautgate", "gpt-5.6-sol", now + TTL + Duration::from_secs(1)), None);

        forget_all();
        assert_eq!(cached("nautgate", "gpt-5.6-sol", now), None);
    }
}
