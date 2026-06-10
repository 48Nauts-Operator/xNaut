// Thin opt-in client for Engram-OSS ("the Brain") — long-term memory API,
// e.g. http://stargate.tail138398.ts.net:8085. Endpoints: /health, /memories/search.

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub content: String,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub id: Option<String>,
}

/// Search memories. `url` is the API base (no trailing slash needed).
///
/// POSTs `{url}/memories/search` with `{"query": ..., "limit": ...}` (10s timeout).
/// Tolerates the three response shapes Engram deployments use: a bare array,
/// `{"results": [...]}`, or `{"memories": [...]}`.
pub async fn search(url: &str, query: &str, limit: usize) -> Result<Vec<Memory>, String> {
    let endpoint = format!("{}/memories/search", url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let resp = client
        .post(&endpoint)
        .json(&serde_json::json!({ "query": query, "limit": limit }))
        .send()
        .await
        .map_err(|e| format!("engram search request failed: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("engram search: failed to read body: {e}"))?;

    if !status.is_success() {
        let excerpt: String = body.chars().take(200).collect();
        return Err(format!("engram search returned {status}: {excerpt}"));
    }

    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("engram search: invalid JSON response: {e}"))?;
    Ok(parse_memories(&value))
}

/// True if `GET {url}/health` returns 2xx within 3s.
pub async fn health(url: &str) -> bool {
    let endpoint = format!("{}/health", url.trim_end_matches('/'));
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    else {
        return false;
    };
    match client.get(&endpoint).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Extracts memories from a search response. Handles a bare array or an object
/// wrapping the array under "results" or "memories". Entries without a content
/// field ("content" / "text" / "memory") are skipped.
fn parse_memories(value: &serde_json::Value) -> Vec<Memory> {
    let items = match value {
        serde_json::Value::Array(items) => items.as_slice(),
        serde_json::Value::Object(map) => map
            .get("results")
            .or_else(|| map.get("memories"))
            .and_then(|v| v.as_array())
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
        _ => &[],
    };

    items
        .iter()
        .filter_map(|item| {
            let content = item
                .get("content")
                .or_else(|| item.get("text"))
                .or_else(|| item.get("memory"))
                .and_then(|v| v.as_str())?
                .to_string();
            let score = item
                .get("score")
                .or_else(|| item.get("similarity"))
                .and_then(|v| v.as_f64());
            let id = item.get("id").and_then(|v| v.as_str()).map(String::from);
            Some(Memory { content, score, id })
        })
        .collect()
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn engram_status(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<serde_json::Value, String> {
    let engram = state.settings.lock().await.engram.clone();
    let reachable = if engram.enabled && !engram.url.is_empty() {
        health(&engram.url).await
    } else {
        false
    };
    Ok(serde_json::json!({
        "enabled": engram.enabled,
        "url": engram.url,
        "reachable": reachable,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_bare_array() {
        let v = json!([
            {"content": "first", "score": 0.9, "id": "m1"},
            {"text": "second", "similarity": 0.5},
        ]);
        let mems = parse_memories(&v);
        assert_eq!(mems.len(), 2);
        assert_eq!(mems[0].content, "first");
        assert_eq!(mems[0].score, Some(0.9));
        assert_eq!(mems[0].id.as_deref(), Some("m1"));
        assert_eq!(mems[1].content, "second");
        assert_eq!(mems[1].score, Some(0.5));
        assert_eq!(mems[1].id, None);
    }

    #[test]
    fn parses_results_wrapper() {
        let v = json!({"results": [{"memory": "wrapped", "score": 0.1}]});
        let mems = parse_memories(&v);
        assert_eq!(mems.len(), 1);
        assert_eq!(mems[0].content, "wrapped");
        assert_eq!(mems[0].score, Some(0.1));
    }

    #[test]
    fn parses_memories_wrapper() {
        let v = json!({"memories": [{"content": "from memories"}]});
        let mems = parse_memories(&v);
        assert_eq!(mems.len(), 1);
        assert_eq!(mems[0].content, "from memories");
        assert_eq!(mems[0].score, None);
    }

    #[test]
    fn skips_entries_without_content() {
        let v = json!([{"score": 0.7}, {"content": "kept"}]);
        let mems = parse_memories(&v);
        assert_eq!(mems.len(), 1);
        assert_eq!(mems[0].content, "kept");
    }

    #[test]
    fn unknown_shapes_yield_empty() {
        assert!(parse_memories(&json!({"data": []})).is_empty());
        assert!(parse_memories(&json!("nope")).is_empty());
    }
}
