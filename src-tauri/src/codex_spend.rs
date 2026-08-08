// What a Codex session actually consumed.
//
// We already read Codex's `rate_limits` (usage.rs::codex_usage) — the percentage
// of plan consumed. That answers "how close am I to the ceiling" and says
// nothing about what a given piece of work cost, which is the question that
// matters once the provider is selectable per session.
//
// Idea from cdknorow/coral (Apache 2.0), `internal/background/token_poller.go`,
// which polls Codex rollout transcripts for usage. We depart from it in one
// place, and the departure is the interesting part:
//
//   They poll and DIFF cumulative usage against the previous reading, because
//   they are building a time series of deltas. We want a per-session total, and
//   Codex already writes `total_token_usage` cumulatively — so the LAST
//   occurrence in a file IS the session total. Taking the last value is exact;
//   summing occurrences would multiply-count the whole session, which is the
//   trap the diffing exists to avoid in the first place.
//
// Verified against real transcripts at ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl:
//   payload.info.total_token_usage = { input_tokens, cached_input_tokens,
//     cache_write_input_tokens, output_tokens, reasoning_output_tokens,
//     total_tokens }
//   payload.info.last_token_usage  = the same shape, for the most recent turn

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct CodexSpend {
    /// Session id, taken from the rollout filename.
    pub session_id: String,
    pub model: String,
    /// First timestamp seen in the file, RFC3339.
    pub started: String,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    /// Estimated USD **at API list prices**. `None` when the model has no known
    /// price — an unpriced model must read as "unknown", never as free.
    ///
    /// This is a notional value, not a bill. On a subscription plan the marginal
    /// cost of a session is zero and this number is instead "what this work
    /// would have cost on the API" — useful for comparing sessions and for
    /// quoting client work, misleading if read as money that left an account.
    /// Any UI showing it must say which of the two it means.
    pub cost_usd: Option<f64>,
    /// Absolute path, so a caller can open the transcript.
    pub path: String,
}

/// Per-million-token prices, input and output.
///
/// Cached input is billed at a fraction of input; Codex reports it separately
/// and it dominates long sessions, so ignoring the distinction would overstate
/// cost badly — one real transcript here shows 64.3M cached against 66.3M total
/// input, so treating cached as full price would be roughly 10x wrong.
fn price_per_mtok(model: &str) -> Option<(f64, f64, f64)> {
    let m = model.to_lowercase();
    // (input, cached_input, output)
    if m.starts_with("gpt-5") || m.starts_with("o4") {
        return Some((1.25, 0.125, 10.0));
    }
    if m.starts_with("gpt-4.1") {
        return Some((2.0, 0.5, 8.0));
    }
    if m.starts_with("gpt-4o") {
        return Some((2.5, 1.25, 10.0));
    }
    None
}

fn estimate_cost(model: &str, input: u64, cached: u64, output: u64) -> Option<f64> {
    let (p_in, p_cached, p_out) = price_per_mtok(model)?;
    // Codex reports cached_input_tokens as a SUBSET of input_tokens, so the
    // uncached portion is the difference. Saturating: a malformed file must not
    // underflow into a colossal number.
    let uncached = input.saturating_sub(cached);
    let m = 1_000_000.0;
    Some((uncached as f64 / m) * p_in + (cached as f64 / m) * p_cached + (output as f64 / m) * p_out)
}

/// Pull the first string value for any of `keys`, anywhere in the tree.
fn first_str(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for k in keys {
                if let Some(Value::String(s)) = map.get(*k) {
                    if !s.is_empty() {
                        return Some(s.clone());
                    }
                }
            }
            map.values().find_map(|v| first_str(v, keys))
        }
        Value::Array(arr) => arr.iter().find_map(|v| first_str(v, keys)),
        _ => None,
    }
}

fn find_obj<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .filter(|v| v.is_object())
            .or_else(|| map.values().find_map(|v| find_obj(v, key))),
        Value::Array(arr) => arr.iter().find_map(|v| find_obj(v, key)),
        _ => None,
    }
}

fn u64_at(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Parses one rollout transcript. Public for testing against fixtures.
pub fn parse_rollout(session_id: &str, path: &str, body: &str) -> Option<CodexSpend> {
    let mut model = String::new();
    let mut started = String::new();
    let mut last_total: Option<Value> = None;

    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue; // a truncated final line is normal on a live session
        };
        if started.is_empty() {
            if let Some(ts) = first_str(&value, &["timestamp"]) {
                started = ts;
            }
        }
        if model.is_empty() {
            if let Some(m) = first_str(&value, &["model", "model_slug", "model_name"]) {
                model = m;
            }
        }
        // Cumulative — keep overwriting, so the last one wins.
        if let Some(t) = find_obj(&value, "total_token_usage") {
            last_total = Some(t.clone());
        }
    }

    let total = last_total?;
    let input = u64_at(&total, "input_tokens");
    let cached = u64_at(&total, "cached_input_tokens");
    let output = u64_at(&total, "output_tokens");
    Some(CodexSpend {
        session_id: session_id.to_string(),
        cost_usd: estimate_cost(&model, input, cached, output),
        model,
        started,
        input_tokens: input,
        cached_input_tokens: cached,
        output_tokens: output,
        reasoning_output_tokens: u64_at(&total, "reasoning_output_tokens"),
        total_tokens: u64_at(&total, "total_tokens"),
        path: path.to_string(),
    })
}

/// `rollout-2026-07-29T22-16-56-019faf85-….jsonl` → the uuid tail.
fn session_id_from(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
    // The id is the last 5 dash-separated groups of a uuid; taking everything
    // after the timestamp is more robust than counting dashes.
    stem.strip_prefix("rollout-")
        .and_then(|rest| rest.get(20..))
        .unwrap_or(&stem)
        .to_string()
}

fn collect_rollouts(dir: &Path, out: &mut Vec<(std::time::SystemTime, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rollouts(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
            && path
                .file_name()
                .map(|n| n.to_string_lossy().starts_with("rollout-"))
                .unwrap_or(false)
        {
            if let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) {
                out.push((mtime, path));
            }
        }
    }
}

/// Recent Codex sessions with what each consumed, newest first.
#[tauri::command]
pub fn codex_spend(limit: Option<usize>) -> Result<Vec<CodexSpend>, String> {
    let dir = dirs::home_dir().ok_or("no home dir")?.join(".codex").join("sessions");
    let mut files = Vec::new();
    collect_rollouts(&dir, &mut files);
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.truncate(limit.unwrap_or(25));

    let mut out = Vec::new();
    for (_, path) in files {
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let id = session_id_from(&path);
        if let Some(s) = parse_rollout(&id, &path.to_string_lossy(), &body) {
            out.push(s);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(total: (u64, u64, u64, u64, u64)) -> String {
        format!(
            r#"{{"payload":{{"info":{{"total_token_usage":{{"input_tokens":{},"cached_input_tokens":{},"cache_write_input_tokens":0,"output_tokens":{},"reasoning_output_tokens":{},"total_tokens":{}}}}}}}}}"#,
            total.0, total.1, total.2, total.3, total.4
        )
    }

    #[test]
    fn the_last_cumulative_reading_wins() {
        // The trap: these are cumulative, so summing them would report 3x the
        // real session. Only the final line is the session total.
        let body = [line((100, 0, 10, 0, 110)), line((200, 0, 20, 0, 220)), line((300, 50, 30, 5, 330))].join("\n");
        let s = parse_rollout("abc", "/tmp/x.jsonl", &body).unwrap();
        assert_eq!(s.input_tokens, 300);
        assert_eq!(s.output_tokens, 30);
        assert_eq!(s.total_tokens, 330);
    }

    #[test]
    fn model_and_timestamp_come_from_the_transcript() {
        let body = format!(
            "{{\"timestamp\":\"2026-07-29T20:17:11.671Z\",\"payload\":{{\"model\":\"gpt-5.6-sol\"}}}}\n{}",
            line((10, 0, 1, 0, 11))
        );
        let s = parse_rollout("abc", "/tmp/x.jsonl", &body).unwrap();
        assert_eq!(s.model, "gpt-5.6-sol");
        assert_eq!(s.started, "2026-07-29T20:17:11.671Z");
    }

    #[test]
    fn cached_input_is_not_billed_at_full_price() {
        // A real transcript here showed 64.3M cached of 66.3M input. Charging
        // cached at full rate would overstate that session roughly tenfold.
        let full = estimate_cost("gpt-5.6-sol", 1_000_000, 0, 0).unwrap();
        let cached = estimate_cost("gpt-5.6-sol", 1_000_000, 1_000_000, 0).unwrap();
        assert!(cached < full / 5.0, "cached {cached} should be far below {full}");
    }

    #[test]
    fn an_unpriced_model_is_unknown_not_free() {
        let body = format!(
            "{{\"payload\":{{\"model\":\"some-future-model\"}}}}\n{}",
            line((1_000_000, 0, 1_000_000, 0, 2_000_000))
        );
        let s = parse_rollout("abc", "/tmp/x.jsonl", &body).unwrap();
        assert_eq!(s.cost_usd, None, "unknown price must not read as zero cost");
        assert_eq!(s.total_tokens, 2_000_000, "tokens are still reported");
    }

    #[test]
    fn cost_matches_a_hand_calculation() {
        // 1M uncached in @1.25 + 1M cached @0.125 + 1M out @10 = 11.375
        let c = estimate_cost("gpt-5-codex", 2_000_000, 1_000_000, 1_000_000).unwrap();
        assert!((c - 11.375).abs() < 1e-9, "got {c}");
    }

    #[test]
    fn a_truncated_final_line_does_not_lose_the_session() {
        // Live sessions are read mid-write; the last line is often half-flushed.
        let body = format!("{}\n{{\"payload\":{{\"info\":{{\"total_to", line((7, 1, 2, 0, 9)));
        let s = parse_rollout("abc", "/tmp/x.jsonl", &body).unwrap();
        assert_eq!(s.total_tokens, 9);
    }

    #[test]
    fn a_transcript_with_no_usage_yields_nothing() {
        assert!(parse_rollout("abc", "/tmp/x.jsonl", "{\"payload\":{}}\n").is_none());
    }

    /// Against the real transcripts on this machine, not fixtures. Ignored by
    /// default because it depends on local files; run with
    /// `cargo test real_transcripts -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_transcripts() {
        let sessions = codex_spend(Some(50)).expect("should read ~/.codex/sessions");
        println!("\n{} Codex sessions:", sessions.len());
        let mut priced = 0.0;
        let mut unknown = 0;
        for s in &sessions {
            match s.cost_usd {
                Some(c) => { priced += c; }
                None => unknown += 1,
            }
            println!(
                "  {:<14} {:>12} tok  ({:>11} cached)  {}",
                if s.model.is_empty() { "?" } else { &s.model },
                s.total_tokens, s.cached_input_tokens,
                s.cost_usd.map(|c| format!("${c:.2}")).unwrap_or_else(|| "price unknown".into()),
            );
            assert!(s.total_tokens >= s.output_tokens, "totals must be coherent");
            assert!(s.cached_input_tokens <= s.input_tokens, "cached is a subset of input");
        }
        println!("  total priced: ${priced:.2}   ({unknown} of unknown price)");
    }

    #[test]
    fn session_id_is_taken_from_the_filename() {
        let p = PathBuf::from("/x/rollout-2026-07-29T22-16-56-019faf85-e921-7653-98f5-92402c71f46a.jsonl");
        assert_eq!(session_id_from(&p), "019faf85-e921-7653-98f5-92402c71f46a");
    }
}
