// Thin opt-in client for Engram-OSS ("the Brain") — long-term memory API,
// e.g. http://stargate.tail138398.ts.net:8085. Endpoints: /health, /memories/search.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub content: String,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketLearning {
    pub agent_id: String,
    pub repository: String,
    pub item_type: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub analysis: String,
}

fn http_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))
}

async fn post_json(url: &str, path: &str, body: &Value) -> Result<Value, String> {
    let endpoint = format!("{}{}", url.trim_end_matches('/'), path);
    let resp = http_client(30)?
        .post(&endpoint)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Engram {path} request failed: {e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Engram {path}: failed to read body: {e}"))?;
    if !status.is_success() {
        let excerpt: String = text.chars().take(300).collect();
        return Err(format!("Engram {path} returned {status}: {excerpt}"));
    }
    if text.trim().is_empty() {
        return Ok(json!({ "ok": true }));
    }
    serde_json::from_str(&text).or_else(|_| Ok(json!({ "ok": true, "response": text })))
}

fn redact_secrets(content: &str) -> String {
    let secret = regex::Regex::new(
        r"(?i)(api[_-]?key|access[_-]?token|auth[_-]?token|password|secret)(\s*[:=]\s*)([^\s,;]+)",
    )
    .expect("secret redaction regex");
    secret.replace_all(content, "$1$2[REDACTED]").into_owned()
}

fn learning_source_id(learning: &TicketLearning, analysis: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(learning.repository.as_bytes());
    hash.update(learning.item_type.as_bytes());
    hash.update(learning.number.to_le_bytes());
    hash.update(analysis.as_bytes());
    format!("xnaut-ticket-{:x}", hash.finalize())
}

async fn store_ticket_learning(url: &str, learning: &TicketLearning) -> Result<Value, String> {
    let analysis = redact_secrets(learning.analysis.trim());
    if analysis.is_empty() {
        return Err("learning analysis is empty".into());
    }
    let analysis: String = analysis.chars().take(12_000).collect();
    let source_id = learning_source_id(learning, &analysis);
    let content = format!(
        "Verified ticket learning: {} #{} - {}\n\n{}\n\nSource: {}",
        learning.repository, learning.number, learning.title, analysis, learning.url
    );
    post_json(
        url,
        "/memories",
        &json!({
            "agent_id": learning.agent_id.trim().to_string(),
            "content": content,
            "category": "insight",
            "importance": 0.9,
            "metadata": {
                "source": "xnaut-forge-review",
                "source_id": source_id,
                "confirmed_by_user": true,
                "repository": learning.repository,
                "item_type": learning.item_type,
                "ticket_number": learning.number,
                "ticket_url": learning.url,
            }
        }),
    )
    .await
}

async fn run_consolidation(url: &str) -> Result<Value, String> {
    post_json(
        url,
        "/consolidation/run",
        &json!({ "hours": 24, "dry_run": false }),
    )
    .await
}

fn learning_state_path() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut").join("engram-learning.json"))
        .unwrap_or_else(|| PathBuf::from(".xnaut/engram-learning.json"))
}

fn last_learning_date() -> Option<String> {
    let value: Value =
        serde_json::from_str(&std::fs::read_to_string(learning_state_path()).ok()?).ok()?;
    value
        .get("last_successful_date")?
        .as_str()
        .map(str::to_string)
}

fn write_learning_date(date: &str) -> Result<(), String> {
    let path = learning_state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({ "last_successful_date": date })).unwrap(),
    )
    .map_err(|e| format!("failed to write {}: {e}", path.display()))
}

fn daily_learning_due(last: Option<&str>, today: &str) -> bool {
    last != Some(today)
}

pub fn spawn_daily_learning_task(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if let Some(state) = app.try_state::<crate::state::AppState>() {
                let settings = state.settings.lock().await.engram.clone();
                let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                if settings.enabled
                    && !settings.url.trim().is_empty()
                    && daily_learning_due(last_learning_date().as_deref(), &today)
                {
                    match run_consolidation(&settings.url).await {
                        Ok(_) => {
                            if let Err(e) = write_learning_date(&today) {
                                eprintln!("[engram] daily learning state failed: {e}");
                            }
                        }
                        Err(e) => eprintln!("[engram] daily learning pass failed: {e}"),
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
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

#[tauri::command]
pub async fn engram_store_learning(
    state: tauri::State<'_, crate::state::AppState>,
    learning: TicketLearning,
) -> Result<Value, String> {
    let engram = state.settings.lock().await.engram.clone();
    if !engram.enabled || engram.url.trim().is_empty() {
        return Err(
            "Engram is disabled. Enable it and set its URL in Settings > Tasks Mode.".into(),
        );
    }
    store_ticket_learning(&engram.url, &learning).await
}

#[tauri::command]
pub async fn engram_run_learning_loop(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Value, String> {
    let engram = state.settings.lock().await.engram.clone();
    if !engram.enabled || engram.url.trim().is_empty() {
        return Err(
            "Engram is disabled. Enable it and set its URL in Settings > Tasks Mode.".into(),
        );
    }
    let result = run_consolidation(&engram.url).await?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    write_learning_date(&today)?;
    Ok(result)
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

    #[test]
    fn secret_values_are_redacted_before_storage() {
        let redacted = redact_secrets("api_key=abc123 password: hunter2 safe text");
        assert_eq!(
            redacted,
            "api_key=[REDACTED] password: [REDACTED] safe text"
        );
    }

    #[test]
    fn daily_learning_runs_once_per_date() {
        assert!(daily_learning_due(None, "2026-07-10"));
        assert!(daily_learning_due(Some("2026-07-09"), "2026-07-10"));
        assert!(!daily_learning_due(Some("2026-07-10"), "2026-07-10"));
    }
}
