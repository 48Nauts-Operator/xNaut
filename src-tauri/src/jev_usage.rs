//! xNaut-only Jev receipts. No account-wide balance, external demo logs, prompts
//! or credentials are exposed. Native decision receipts and legacy xNaut usage
//! are combined here; unknown usage is shown separately instead of priced at zero.
//! Pricing: https://docs.typesafe.ai/models.md (verified 2026-09-29).
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Receipt {
    pub request_id: String,
    pub at: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    configured: bool,
    month: String,
    calls: usize,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    unpriced_calls: usize,
    unknown_calls: usize,
}
fn summary(receipts: Vec<Receipt>, month: &str, configured: bool) -> Summary {
    let mut result = Summary { configured, month: month.into(), calls: 0,
        input_tokens: 0, output_tokens: 0, cost_usd: 0.0, unpriced_calls: 0, unknown_calls: 0 };
    let mut seen = std::collections::HashSet::new();
    for r in receipts {
        if !seen.insert(r.request_id) || !r.at.starts_with(&format!("{month}-")) { continue; }
        result.calls += 1;
        result.input_tokens += r.input_tokens;
        result.output_tokens += r.output_tokens;
        if r.model == "jev-1.13.0" {
            result.cost_usd += r.input_tokens as f64 * 0.042 / 1_000_000.0;
        } else { result.unpriced_calls += 1; }
    }
    result
}
fn read(path: &Path) -> Result<Vec<Receipt>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "Jev usage records could not be read".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err("Jev usage records could not be read".into()),
    }
}
#[tauri::command]
pub async fn jev_usage() -> Result<Summary, String> {
    tokio::task::spawn_blocking(|| {
        let root = dirs::config_dir().ok_or("configuration directory unavailable")?.join("xnaut");
        let configured = crate::secrets::load("plugin/typesafe/TYPESAFE_API_KEY").is_some()
            || crate::plugins::plugin_env("typesafe").is_some_and(|env| env.get("TYPESAFE_API_KEY").is_some_and(|v| !v.trim().is_empty()));
        // This file is reserved for xNaut's native Jev caller. Do not import
        // account totals: other tools may share the same credential.
        let mut receipts = read(&root.join("jev-usage.json"))?;
        let (native, unknown) = crate::jev_decisions::usage_records()?;
        receipts.extend(native);
        let mut result = summary(receipts, &chrono::Utc::now().format("%Y-%m").to_string(), configured);
        result.unknown_calls = unknown;
        result.calls += unknown;
        Ok(result)
    }).await.map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_cost_counts_input_only_and_deduplicates() {
        let make = |id: &str, at: &str, model: &str| Receipt { request_id: id.into(), at: at.into(), model: model.into(), input_tokens: 1_000_000, output_tokens: 500_000 };
        let s = summary(vec![make("a", "2026-09-29", "jev-1.13.0"), make("a", "2026-09-29", "jev-1.13.0"), make("b", "2026-08-31", "jev-1.13.0"), make("c", "2026-09-29", "future")], "2026-09", true);
        assert_eq!(s.calls, 2); assert_eq!(s.input_tokens, 2_000_000);
        assert_eq!(s.cost_usd, 0.042); assert_eq!(s.unpriced_calls, 1);
    }
    #[test]
    fn missing_and_corrupt_receipts_are_distinct() {
        let dir = std::env::temp_dir().join(format!("jev-usage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap(); let path = dir.join("usage.json");
        assert!(read(&path).unwrap().is_empty());
        std::fs::write(&path, b"broken").unwrap(); assert!(read(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
