// ABOUTME: Claude MAX-plan usage for the footer strip (XNAUT-24). Reads the OAuth
// ABOUTME: token from the macOS Keychain (service "Claude Code-credentials") and
// ABOUTME: queries api.anthropic.com/api/oauth/usage — the same endpoint /usage uses.
// ABOUTME: Returns 5-hour %, weekly %, and per-model % (e.g. Fable).

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct ModelUsage {
    pub name: String,
    pub percent: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaxUsage {
    pub five_hour_pct: f64,
    pub seven_day_pct: f64,
    pub five_hour_resets_at: Option<String>,
    pub seven_day_resets_at: Option<String>,
    pub per_model: Vec<ModelUsage>,
    /// Worst severity across buckets: normal | warning | critical (best-effort).
    pub severity: String,
}

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// Read the Claude Code OAuth access token from the macOS Keychain.
fn read_oauth_token(account: Option<&str>) -> Result<String, String> {
    let mut cmd = std::process::Command::new("security");
    cmd.args(["find-generic-password", "-s", KEYCHAIN_SERVICE]);
    if let Some(acct) = account {
        cmd.args(["-a", acct]);
    }
    cmd.arg("-w"); // print only the password (the creds blob)
    let out = cmd
        .output()
        .map_err(|e| format!("keychain read failed: {e}"))?;
    if !out.status.success() {
        return Err("could not read the Claude OAuth token from the Keychain".into());
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let creds: Value = serde_json::from_str(raw.trim())
        .map_err(|e| format!("keychain creds are not JSON: {e}"))?;
    creds
        .pointer("/claudeAiOauth/accessToken")
        .or_else(|| creds.get("accessToken"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| "no accessToken in Keychain credentials".into())
}

/// Parse the /api/oauth/usage response into the footer's shape. Pure — tested.
pub fn parse_usage(value: &Value) -> MaxUsage {
    let util = |bucket: &str| -> f64 {
        value
            .pointer(&format!("/{bucket}/utilization"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let resets = |bucket: &str| -> Option<String> {
        value
            .pointer(&format!("/{bucket}/resets_at"))
            .and_then(Value::as_str)
            .map(String::from)
    };
    let per_model = value
        .get("limits")
        .and_then(Value::as_array)
        .map(|limits| {
            limits
                .iter()
                .filter(|limit| limit.get("kind").and_then(Value::as_str) == Some("weekly_scoped"))
                .filter_map(|limit| {
                    let name = limit
                        .pointer("/scope/model/display_name")
                        .and_then(Value::as_str)?
                        .to_string();
                    Some(ModelUsage {
                        name,
                        percent: limit.get("percent").and_then(Value::as_f64).unwrap_or(0.0),
                        resets_at: limit
                            .get("resets_at")
                            .and_then(Value::as_str)
                            .map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let severity = value
        .get("limits")
        .and_then(Value::as_array)
        .and_then(|limits| {
            let rank = |s: &str| match s {
                "critical" => 3,
                "warning" => 2,
                "normal" => 1,
                _ => 0,
            };
            limits
                .iter()
                .filter_map(|l| l.get("severity").and_then(Value::as_str))
                .max_by_key(|s| rank(s))
                .map(String::from)
        })
        .unwrap_or_else(|| "normal".into());
    MaxUsage {
        five_hour_pct: util("five_hour"),
        seven_day_pct: util("seven_day"),
        five_hour_resets_at: resets("five_hour"),
        seven_day_resets_at: resets("seven_day"),
        per_model,
        severity,
    }
}

/// Fetch current MAX-plan usage. Never blocks the UI on failure — the caller shows "—".
#[tauri::command]
pub async fn max_usage(account: Option<String>) -> Result<MaxUsage, String> {
    let token = read_oauth_token(account.as_deref())?;
    let resp = reqwest::Client::new()
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .await
        .map_err(|e| format!("usage request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        // 401 → token likely expired; Claude Code refreshes it on next use. Surface
        // a clean message so the footer degrades to "—" rather than crashing.
        return Err(format!("usage endpoint returned HTTP {status}"));
    }
    let value: Value = resp
        .json()
        .await
        .map_err(|e| format!("usage response not JSON: {e}"))?;
    Ok(parse_usage(&value))
}

// ─── Codex (ChatGPT/GPT) usage ──────────────────────────────────────────────
// Codex writes rate limits into its session logs (~/.codex/sessions/**/*.jsonl),
// so this is offline — but only as fresh as the last codex run.

#[derive(Debug, Clone, Serialize)]
pub struct CodexWindow {
    pub used_percent: f64,
    pub window_label: String,
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexUsage {
    pub primary: Option<CodexWindow>,
    pub secondary: Option<CodexWindow>,
    pub plan_type: Option<String>,
}

fn window_label(minutes: f64) -> String {
    let m = minutes.round() as i64;
    if m == 10080 {
        "wk".into()
    } else if m > 0 && m % 60 == 0 {
        format!("{}h", m / 60)
    } else {
        format!("{}m", m)
    }
}

fn codex_window(value: &Value) -> Option<CodexWindow> {
    let used = value.get("used_percent").and_then(Value::as_f64)?;
    Some(CodexWindow {
        used_percent: used,
        window_label: window_label(
            value
                .get("window_minutes")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        ),
        resets_at: value.get("resets_at").and_then(Value::as_i64),
    })
}

/// Parse a codex `rate_limits` object into the footer shape. Pure — tested.
pub fn parse_codex_rate_limits(rl: &Value) -> CodexUsage {
    CodexUsage {
        primary: rl.get("primary").and_then(codex_window),
        secondary: rl.get("secondary").and_then(codex_window),
        plan_type: rl
            .get("plan_type")
            .and_then(Value::as_str)
            .map(String::from),
    }
}

/// Recursively find the first value for `key` anywhere in a JSON tree.
fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|v| find_key(v, key))),
        Value::Array(arr) => arr.iter().find_map(|v| find_key(v, key)),
        _ => None,
    }
}

fn collect_jsonl(dir: &Path, out: &mut Vec<(std::time::SystemTime, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            if let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) {
                out.push((mtime, path));
            }
        }
    }
}

/// Latest Codex usage from its session logs (newest file, last rate_limits entry).
#[tauri::command]
pub fn codex_usage() -> Result<CodexUsage, String> {
    let dir = dirs::home_dir()
        .ok_or("no home dir")?
        .join(".codex")
        .join("sessions");
    let mut files = Vec::new();
    collect_jsonl(&dir, &mut files);
    files.sort_by_key(|f| std::cmp::Reverse(f.0)); // newest first
    for (_, path) in files.iter().take(30) {
        let Ok(body) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in body.lines().rev() {
            if !line.contains("\"rate_limits\"") {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                if let Some(rl) = find_key(&value, "rate_limits") {
                    if !rl.is_null() {
                        return Ok(parse_codex_rate_limits(rl));
                    }
                }
            }
        }
    }
    Err("no Codex rate-limit data in session logs".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_verified_response_shape() {
        // Trimmed real shape from GET /api/oauth/usage.
        let raw = serde_json::json!({
            "five_hour": { "utilization": 4.0, "resets_at": "2026-07-13T20:20:00Z" },
            "seven_day": { "utilization": 31.0, "resets_at": "2026-07-15T06:00:00Z" },
            "limits": [
                { "kind": "session", "percent": 4, "severity": "normal" },
                { "kind": "weekly_all", "percent": 31, "severity": "warning" },
                { "kind": "weekly_scoped", "percent": 23, "severity": "normal",
                  "resets_at": "2026-07-15T06:00:00Z",
                  "scope": { "model": { "display_name": "Fable" } } }
            ]
        });
        let u = parse_usage(&raw);
        assert_eq!(u.five_hour_pct, 4.0);
        assert_eq!(u.seven_day_pct, 31.0);
        assert_eq!(u.per_model.len(), 1);
        assert_eq!(u.per_model[0].name, "Fable");
        assert_eq!(u.per_model[0].percent, 23.0);
        assert_eq!(u.severity, "warning"); // worst across buckets
    }

    #[test]
    fn tolerates_missing_fields() {
        let u = parse_usage(&serde_json::json!({}));
        assert_eq!(u.five_hour_pct, 0.0);
        assert_eq!(u.seven_day_pct, 0.0);
        assert!(u.per_model.is_empty());
        assert_eq!(u.severity, "normal");
    }

    #[test]
    fn parses_codex_rate_limits_shape() {
        let rl = serde_json::json!({
            "limit_id": "codex",
            "primary": { "used_percent": 2.0, "window_minutes": 10080, "resets_at": 1784489192_i64 },
            "secondary": null,
            "plan_type": "pro"
        });
        let u = parse_codex_rate_limits(&rl);
        let p = u.primary.expect("primary");
        assert_eq!(p.used_percent, 2.0);
        assert_eq!(p.window_label, "wk");
        assert_eq!(p.resets_at, Some(1784489192));
        assert!(u.secondary.is_none());
        assert_eq!(u.plan_type.as_deref(), Some("pro"));
    }

    #[test]
    fn codex_window_labels() {
        assert_eq!(window_label(10080.0), "wk");
        assert_eq!(window_label(300.0), "5h");
        assert_eq!(window_label(60.0), "1h");
        assert_eq!(window_label(45.0), "45m");
    }

    #[test]
    fn find_key_reaches_nested() {
        let v = serde_json::json!({"a":{"b":{"rate_limits":{"x":1}}}});
        assert!(find_key(&v, "rate_limits").is_some());
        assert!(find_key(&v, "nope").is_none());
    }
}
