// ABOUTME: Work session logger with Merkle tree proof for verifiable work documentation.
// ABOUTME: Records terminal commands with timestamps, chains them cryptographically,
// ABOUTME: and generates QR codes for tamper-evident verification.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkLogEntry {
    pub timestamp: String,
    pub command: String,
    pub directory: String,
    pub output_summary: Option<String>,
    pub duration_ms: Option<u64>,
    pub hash: String,
    pub prev_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSession {
    pub id: String,
    pub client: String,
    pub project: String,
    pub started: String,
    pub ended: Option<String>,
    pub entries: Vec<WorkLogEntry>,
    pub merkle_root: Option<String>,
    pub active: bool,
}

impl WorkSession {
    pub fn new(client: &str, project: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            client: client.to_string(),
            project: project.to_string(),
            started: chrono::Utc::now().to_rfc3339(),
            ended: None,
            entries: Vec::new(),
            merkle_root: None,
            active: true,
        }
    }

    pub fn add_entry(&mut self, command: &str, directory: &str, output_summary: Option<&str>) {
        let prev_hash = self
            .entries
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_else(|| "genesis".to_string());

        // Calculate duration since last command
        let now = chrono::Utc::now();
        let timestamp = now.to_rfc3339();
        let duration_ms = self.entries.last().and_then(|prev| {
            chrono::DateTime::parse_from_rfc3339(&prev.timestamp)
                .ok()
                .map(|prev_time| (now - prev_time.with_timezone(&chrono::Utc)).num_milliseconds() as u64)
        });

        let hash_input = format!("{}|{}|{}|{}", timestamp, command, directory, prev_hash);
        let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

        self.entries.push(WorkLogEntry {
            timestamp,
            command: command.to_string(),
            directory: directory.to_string(),
            output_summary: output_summary.map(|s| s.to_string()),
            duration_ms,
            hash,
            prev_hash,
        });
    }

    pub fn finalize(&mut self) {
        self.active = false;
        self.ended = Some(chrono::Utc::now().to_rfc3339());
        self.merkle_root = Some(self.compute_merkle_root());
    }

    fn compute_merkle_root(&self) -> String {
        if self.entries.is_empty() {
            return "empty".to_string();
        }

        let mut hashes: Vec<String> = self.entries.iter().map(|e| e.hash.clone()).collect();

        while hashes.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in hashes.chunks(2) {
                let combined = if chunk.len() == 2 {
                    format!("{}{}", chunk[0], chunk[1])
                } else {
                    format!("{}{}", chunk[0], chunk[0])
                };
                next_level.push(format!("{:x}", Sha256::digest(combined.as_bytes())));
            }
            hashes = next_level;
        }

        hashes[0].clone()
    }

    pub fn verify(&self) -> bool {
        let mut prev_hash = "genesis".to_string();
        for entry in &self.entries {
            if entry.prev_hash != prev_hash {
                return false;
            }
            let expected = format!(
                "{}|{}|{}|{}",
                entry.timestamp, entry.command, entry.directory, entry.prev_hash
            );
            let expected_hash = format!("{:x}", Sha256::digest(expected.as_bytes()));
            if entry.hash != expected_hash {
                return false;
            }
            prev_hash = entry.hash.clone();
        }
        true
    }

    pub fn generate_qr_data(&self) -> String {
        serde_json::json!({
            "session": self.id,
            "client": self.client,
            "project": self.project,
            "started": self.started,
            "ended": self.ended,
            "commands": self.entries.len(),
            "merkle_root": self.merkle_root,
            "verified": self.verify()
        })
        .to_string()
    }

    pub fn generate_qr_svg(&self) -> String {
        use qrcode::QrCode;
        let data = self.generate_qr_data();
        let code = QrCode::new(data.as_bytes()).unwrap_or_else(|_| QrCode::new(b"error").unwrap());
        let svg = code
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(200, 200)
            .build();
        svg
    }

    pub fn generate_summary(&self) -> String {
        let duration = if let Some(ref ended) = self.ended {
            if let (Ok(start), Ok(end)) = (
                chrono::DateTime::parse_from_rfc3339(&self.started),
                chrono::DateTime::parse_from_rfc3339(ended),
            ) {
                let dur = end.signed_duration_since(start);
                let hours = dur.num_hours();
                let mins = dur.num_minutes() % 60;
                format!("{}h {}m", hours, mins)
            } else {
                "unknown".to_string()
            }
        } else {
            "ongoing".to_string()
        };

        let mut summary = format!(
            "# Work Session: {} — {}\n\n",
            self.project, self.client
        );
        summary += &format!("**Date:** {}\n", &self.started[..10]);
        summary += &format!("**Duration:** {}\n", duration);
        summary += &format!("**Commands:** {}\n", self.entries.len());
        summary += &format!(
            "**Verified:** {}\n",
            if self.verify() {
                "✅ Integrity intact"
            } else {
                "❌ Tampered"
            }
        );
        if let Some(ref root) = self.merkle_root {
            summary += &format!("**Merkle Root:** `{}`\n", &root[..16]);
        }
        summary += "\n## Actions\n\n";

        for (i, entry) in self.entries.iter().enumerate() {
            let time = &entry.timestamp[11..19]; // HH:MM:SS
            summary += &format!("{}. `{}` — `{}` ({})\n", i + 1, time, entry.command, entry.directory);
            if let Some(ref out) = entry.output_summary {
                summary += &format!("   _{}_\n", out);
            }
        }

        summary
    }
}

fn worklog_dir() -> PathBuf {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".xnaut")
        .join("worklogs");
    let _ = fs::create_dir_all(&dir);
    dir
}

impl WorkSession {
    pub fn generate_html_report(&self) -> String {
        let duration = if let Some(ref ended) = self.ended {
            if let (Ok(start), Ok(end)) = (
                chrono::DateTime::parse_from_rfc3339(&self.started),
                chrono::DateTime::parse_from_rfc3339(ended),
            ) {
                let dur = end.signed_duration_since(start);
                format!("{}h {}m", dur.num_hours(), dur.num_minutes() % 60)
            } else {
                "unknown".to_string()
            }
        } else {
            "ongoing".to_string()
        };

        let date = if self.started.len() >= 10 { &self.started[..10] } else { &self.started };
        let verified = self.verify();
        let merkle = self.merkle_root.as_deref().unwrap_or("N/A");
        let qr_svg = self.generate_qr_svg();

        // Tool detection and grouping
        let known_tools = vec![
            ("besen", "Besen"),
            ("antbot", "AntBot"),
            ("claude", "Claude Code"),
            ("codex", "OpenAI Codex"),
            ("aider", "Aider"),
            ("docker", "Docker"),
            ("terraform", "Terraform"),
            ("kubectl", "Kubernetes"),
            ("helm", "Helm"),
            ("aws", "AWS CLI"),
            ("gcloud", "Google Cloud"),
            ("git", "Git"),
            ("npm", "npm"),
            ("cargo", "Cargo"),
            ("pip", "pip"),
        ];

        // Calculate tool usage
        let mut tool_usage: std::collections::HashMap<String, (u64, usize)> = std::collections::HashMap::new();
        let mut manual_duration: u64 = 0;
        let mut manual_count: usize = 0;

        for entry in &self.entries {
            let cmd_lower = entry.command.to_lowercase();
            let first_word = cmd_lower.split_whitespace().next().unwrap_or("");
            let dur = entry.duration_ms.unwrap_or(0);

            let mut matched = false;
            for (pattern, name) in &known_tools {
                if first_word == *pattern || first_word.contains(pattern) {
                    let entry_data = tool_usage.entry(name.to_string()).or_insert((0, 0));
                    entry_data.0 += dur;
                    entry_data.1 += 1;
                    matched = true;
                    break;
                }
            }
            if !matched {
                manual_duration += dur;
                manual_count += 1;
            }
        }

        // Sort tools by duration (most used first)
        let mut sorted_tools: Vec<(String, u64, usize)> = tool_usage
            .into_iter()
            .map(|(name, (dur, count))| (name, dur, count))
            .collect();
        sorted_tools.sort_by(|a, b| b.1.cmp(&a.1));

        // Generate tool summary rows
        let mut tool_rows = String::new();
        for (name, dur, count) in &sorted_tools {
            let dur_str = if *dur >= 3600000 {
                format!("{}h {}m", dur / 3600000, (dur % 3600000) / 60000)
            } else if *dur >= 60000 {
                format!("{}m {}s", dur / 60000, (dur % 60000) / 1000)
            } else {
                format!("{:.1}s", *dur as f64 / 1000.0)
            };
            tool_rows += &format!(
                "<tr><td><strong>{}</strong></td><td>{}</td><td>{}</td></tr>\n",
                name, dur_str, count
            );
        }
        if manual_count > 0 {
            let dur_str = if manual_duration >= 60000 {
                format!("{}m {}s", manual_duration / 60000, (manual_duration % 60000) / 1000)
            } else {
                format!("{:.1}s", manual_duration as f64 / 1000.0)
            };
            tool_rows += &format!(
                "<tr><td>Manual commands</td><td>{}</td><td>{}</td></tr>\n",
                dur_str, manual_count
            );
        }

        // Generate command log rows
        let mut rows = String::new();
        for (i, entry) in self.entries.iter().enumerate() {
            let time = if entry.timestamp.len() >= 19 { &entry.timestamp[11..19] } else { &entry.timestamp };
            let dur = match entry.duration_ms {
                Some(ms) if ms >= 60000 => format!("{}m {}s", ms / 60000, (ms % 60000) / 1000),
                Some(ms) if ms >= 1000 => format!("{:.1}s", ms as f64 / 1000.0),
                Some(ms) => format!("{}ms", ms),
                None => "—".to_string(),
            };

            // Detect tool for row highlighting
            let cmd_lower = entry.command.to_lowercase();
            let first_word = cmd_lower.split_whitespace().next().unwrap_or("");
            let tool_name = known_tools.iter()
                .find(|(p, _)| first_word == *p || first_word.contains(p))
                .map(|(_, n)| *n);
            let tool_badge = tool_name
                .map(|n| format!("<span style='font-size:9px; padding:1px 4px; border-radius:2px; background:#3b82f6; color:white; margin-right:4px;'>{}</span>", n))
                .unwrap_or_default();

            rows += &format!(
                "<tr><td>{}</td><td>{}</td><td>{}<code>{}</code></td><td>{}</td><td>{}</td><td><code style='font-size:9px;color:#888;'>{}</code></td></tr>\n",
                i + 1,
                time,
                tool_badge,
                entry.command.replace('<', "&lt;").replace('>', "&gt;"),
                entry.directory,
                dur,
                &entry.hash[..12]
            );
        }

        format!(r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Work Report — {} — {}</title>
<style>
  @page {{ margin: 20mm; }}
  body {{ font-family: -apple-system, 'Helvetica Neue', Arial, sans-serif; color: #1a1a1a; max-width: 800px; margin: 0 auto; padding: 40px; }}
  h1 {{ font-size: 24px; margin-bottom: 4px; }}
  .subtitle {{ color: #666; font-size: 14px; margin-bottom: 24px; }}
  .meta {{ display: flex; gap: 32px; margin-bottom: 24px; padding: 16px; background: #f8f8f8; border-radius: 8px; }}
  .meta-item {{ }}
  .meta-label {{ font-size: 11px; color: #888; text-transform: uppercase; letter-spacing: 0.5px; }}
  .meta-value {{ font-size: 16px; font-weight: 600; }}
  table {{ width: 100%; border-collapse: collapse; margin: 16px 0; font-size: 13px; }}
  th {{ text-align: left; padding: 8px 12px; background: #f0f0f0; border-bottom: 2px solid #ddd; font-size: 11px; text-transform: uppercase; color: #666; }}
  td {{ padding: 6px 12px; border-bottom: 1px solid #eee; }}
  tr:hover {{ background: #fafafa; }}
  code {{ font-family: 'JetBrains Mono', 'SF Mono', monospace; font-size: 12px; }}
  .verification {{ margin-top: 32px; padding: 20px; border: 1px solid #e0e0e0; border-radius: 8px; text-align: center; }}
  .verification h3 {{ margin-bottom: 8px; }}
  .qr {{ background: white; display: inline-block; padding: 12px; margin: 8px 0; }}
  .hash {{ font-family: monospace; font-size: 10px; color: #888; word-break: break-all; }}
  .verified {{ color: #10b981; font-weight: 600; }}
  .tampered {{ color: #ef4444; font-weight: 600; }}
  .footer {{ margin-top: 32px; font-size: 11px; color: #aaa; text-align: center; border-top: 1px solid #eee; padding-top: 16px; }}
</style>
</head>
<body>
<h1>Work Session Report</h1>
<div class="subtitle">{} — {}</div>

<div class="meta">
  <div class="meta-item"><div class="meta-label">Date</div><div class="meta-value">{}</div></div>
  <div class="meta-item"><div class="meta-label">Duration</div><div class="meta-value">{}</div></div>
  <div class="meta-item"><div class="meta-label">Commands</div><div class="meta-value">{}</div></div>
  <div class="meta-item"><div class="meta-label">Status</div><div class="meta-value {}">{}</div></div>
</div>

<h2>Tool Usage Summary</h2>
<table>
<tr><th>Tool</th><th>Duration</th><th>Commands</th></tr>
{}
</table>

<h2>Command Log</h2>
<table>
<tr><th>#</th><th>Time</th><th>Command</th><th>Directory</th><th>Duration</th><th>Hash</th></tr>
{}
</table>

<div class="verification">
  <h3>Verification</h3>
  <p>This work session is cryptographically signed using a SHA-256 Merkle tree.</p>
  <div class="qr">{}</div>
  <div class="hash">Merkle Root: {}</div>
  <p class="{}">Integrity: {}</p>
</div>

<div class="footer">
  Generated by xNAUT — AI-Powered Terminal<br>
  github.com/48Nauts-Operator/xNaut
</div>
</body>
</html>"#,
            self.project, self.client,
            self.project, self.client,
            date, duration, self.entries.len(),
            if verified { "verified" } else { "tampered" },
            if verified { "✓ Verified" } else { "✗ Tampered" },
            tool_rows,
            rows,
            qr_svg,
            merkle,
            if verified { "verified" } else { "tampered" },
            if verified { "Chain integrity intact — no modifications detected" } else { "WARNING: Log has been modified" },
        )
    }
}

fn save_session(session: &WorkSession) {
    let path = worklog_dir().join(format!("{}.json", session.id));
    let _ = fs::write(path, serde_json::to_string_pretty(session).unwrap_or_default());
}

// ==================== Tauri Commands ====================

#[tauri::command]
pub async fn worklog_start(
    state: State<'_, AppState>,
    client: String,
    project: String,
) -> Result<WorkSession, String> {
    let session = WorkSession::new(&client, &project);
    let mut active = state.active_worklog.lock().await;
    *active = Some(session.clone());
    save_session(&session);
    Ok(session)
}

#[tauri::command]
pub async fn worklog_log(
    state: State<'_, AppState>,
    command: String,
    directory: String,
    output_summary: Option<String>,
) -> Result<(), String> {
    let mut active = state.active_worklog.lock().await;
    if let Some(ref mut session) = *active {
        session.add_entry(&command, &directory, output_summary.as_deref());
        save_session(session);
        Ok(())
    } else {
        Err("No active work session".to_string())
    }
}

#[tauri::command]
pub async fn worklog_stop(state: State<'_, AppState>) -> Result<WorkSession, String> {
    let mut active = state.active_worklog.lock().await;
    if let Some(ref mut session) = *active {
        session.finalize();
        save_session(session);
        let result = session.clone();
        *active = None;
        Ok(result)
    } else {
        Err("No active work session".to_string())
    }
}

#[tauri::command]
pub async fn worklog_status(state: State<'_, AppState>) -> Result<Option<WorkSession>, String> {
    let active = state.active_worklog.lock().await;
    Ok(active.clone())
}

#[tauri::command]
pub async fn worklog_summary(state: State<'_, AppState>) -> Result<String, String> {
    let active = state.active_worklog.lock().await;
    if let Some(ref session) = *active {
        Ok(session.generate_summary())
    } else {
        Err("No active work session".to_string())
    }
}

#[tauri::command]
pub async fn worklog_qr(state: State<'_, AppState>) -> Result<String, String> {
    let active = state.active_worklog.lock().await;
    if let Some(ref session) = *active {
        Ok(session.generate_qr_svg())
    } else {
        Err("No active work session".to_string())
    }
}

#[tauri::command]
pub async fn worklog_verify(session_id: String) -> Result<serde_json::Value, String> {
    let path = worklog_dir().join(format!("{}.json", session_id));
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let session: WorkSession = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "verified": session.verify(),
        "session": session,
    }))
}

#[tauri::command]
pub async fn worklog_list() -> Result<Vec<serde_json::Value>, String> {
    let dir = worklog_dir();
    let mut sessions = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(session) = serde_json::from_str::<WorkSession>(&content) {
                        sessions.push(serde_json::json!({
                            "id": session.id,
                            "client": session.client,
                            "project": session.project,
                            "started": session.started,
                            "ended": session.ended,
                            "commands": session.entries.len(),
                            "active": session.active,
                            "verified": session.verify(),
                        }));
                    }
                }
            }
        }
    }
    sessions.sort_by(|a, b| {
        b["started"].as_str().cmp(&a["started"].as_str())
    });
    Ok(sessions)
}

#[tauri::command]
pub async fn worklog_export_html(state: State<'_, AppState>) -> Result<String, String> {
    let active = state.active_worklog.lock().await;
    // Try active session first, then last saved
    if let Some(ref session) = *active {
        return Ok(session.generate_html_report());
    }
    drop(active);

    // Find most recent session
    let dir = worklog_dir();
    let mut latest: Option<(String, WorkSession)> = None;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(session) = serde_json::from_str::<WorkSession>(&content) {
                    if latest.is_none() || session.started > latest.as_ref().unwrap().0 {
                        latest = Some((session.started.clone(), session));
                    }
                }
            }
        }
    }

    if let Some((_, session)) = latest {
        Ok(session.generate_html_report())
    } else {
        Err("No work sessions found".to_string())
    }
}

#[tauri::command]
pub async fn worklog_save_report(state: State<'_, AppState>) -> Result<String, String> {
    let html = worklog_export_html(state).await?;
    let dir = worklog_dir();
    let filename = format!("report-{}.html", chrono::Utc::now().format("%Y-%m-%d-%H%M%S"));
    let path = dir.join(&filename);
    fs::write(&path, &html).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}
