// OpenAI-compatible LLM client for the chat panel (v1.6). Streams completions
// over SSE as chat://chunk events; optionally injects Engram memories as context.

use serde::{Deserialize, Serialize};
use tauri::Emitter;

const CHAT_MAX_TOKENS: u32 = 1024;
const CHAT_STREAM_IDLE_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// "system" | "user" | "assistant"
    pub role: String,
    pub content: String,
}

/// Joins the configured endpoint (with or without trailing slash, with or
/// without /v1) and an API path, e.g. "http://localhost:8090/v1/" + "chat/completions".
fn join_endpoint(endpoint: &str, path: &str) -> String {
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

fn streaming_request_body(model: &str, messages: Vec<ChatMessage>) -> serde_json::Value {
    let messages = normalize_messages_for_model_template(model, messages);
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "max_tokens": CHAT_MAX_TOKENS,
    });
    if should_disable_reasoning_for_chat(model) {
        body["reasoning_effort"] = serde_json::json!("none");
    }
    body
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

fn apply_auth(req: reqwest::RequestBuilder, api_key: &Option<String>) -> reqwest::RequestBuilder {
    match api_key {
        Some(key) if !key.is_empty() => req.bearer_auth(key),
        _ => req,
    }
}

/// One-shot completion, non-streaming. Used by AI commit messages and PR title/body.
pub async fn complete_oneshot(
    llm: &crate::settings::LlmSettings,
    system: Option<&str>,
    user: &str,
) -> Result<String, String> {
    let mut messages = Vec::new();
    if let Some(sys) = system {
        messages.push(serde_json::json!({"role": "system", "content": sys}));
    }
    messages.push(serde_json::json!({"role": "user", "content": user}));

    // Non-streaming, but full-document generation on a local model is slow —
    // generous overall cap, fail fast only on an unreachable endpoint.
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;

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
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| {
            format!(
                "LLM response missing choices[0].message.content: {}",
                body_excerpt(&body)
            )
        })
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn chat_send(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    request_id: String,
    messages: Vec<ChatMessage>,
) -> Result<String, String> {
    let settings = state.settings.lock().await.clone();
    let llm = &settings.llm;

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
                crate::engram::search(&settings.engram.url, &last_user.content, 8).await
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
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;

    let url = join_endpoint(&llm.endpoint, "chat/completions");
    let req = apply_auth(client.post(&url), &llm.api_key)
        .json(&streaming_request_body(&llm.model, outgoing));

    let resp = req
        .send()
        .await
        .map_err(|e| format!("LLM request to {url} failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "LLM request failed ({status}): {}",
            body_excerpt(&body)
        ));
    }

    // Parse the SSE stream. Events can span chunk boundaries, so keep a byte
    // buffer and only consume complete newline-terminated lines.
    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut full = String::new();
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
) -> Result<bool, String> {
    let llm = state.settings.lock().await.llm.clone();

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
        let body = streaming_request_body("qwen/qwen3.6-35b-a3b", Vec::new());
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], CHAT_MAX_TOKENS);
    }

    #[test]
    fn streaming_request_body_disables_reasoning_tokens_for_chat_actions() {
        let body = streaming_request_body("qwen/qwen3.6-35b-a3b", Vec::new());
        assert_eq!(body["reasoning_effort"], "none");
    }

    #[test]
    fn streaming_request_body_does_not_send_qwen_reasoning_flag_to_other_models() {
        let body = streaming_request_body("google/gemma-4-12b-qat", Vec::new());
        assert!(body.get("reasoning_effort").is_none());
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
