// ABOUTME: Work session logger with Merkle tree proof for verifiable work documentation.
// ABOUTME: Records terminal commands with timestamps, chains them cryptographically,
// ABOUTME: and generates QR codes for tamper-evident verification.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use tauri::State;

use crate::state::AppState;
use crate::worklog_sources::{self, Query, Sources};

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
                .map(|prev_time| {
                    (now - prev_time.with_timezone(&chrono::Utc)).num_milliseconds() as u64
                })
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

        let mut summary = format!("# Work Session: {} — {}\n\n", self.project, self.client);
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
            summary += &format!(
                "{}. `{}` — `{}` ({})\n",
                i + 1,
                time,
                entry.command,
                entry.directory
            );
            if let Some(ref out) = entry.output_summary {
                summary += &format!("   _{}_\n", out);
            }
        }

        summary
    }
}

pub(crate) fn worklog_dir() -> PathBuf {
    // Overridable so the tests can stage sessions without writing into the real
    // ~/.xnaut/worklogs, the same trick ledger.rs uses.
    if let Ok(dir) = std::env::var("XNAUT_WORKLOG_DIR") {
        let dir = PathBuf::from(dir);
        let _ = fs::create_dir_all(&dir);
        return dir;
    }
    let dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".xnaut")
        .join("worklogs");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// The `<` and `>` an agent's detail line can easily contain, kept out of the
/// markup. The command log has done this inline since the beginning; the agent
/// rows carry far more untrusted text, so it gets a name.
fn esc(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl WorkSession {
    pub fn generate_html_report_with(&self, sources: &Sources) -> String {
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

        let date = if self.started.len() >= 10 {
            &self.started[..10]
        } else {
            &self.started
        };
        let verified = self.verify();
        let merkle = self.merkle_root.as_deref().unwrap_or("N/A");
        let qr_svg = self.generate_qr_svg();

        // Tool detection and grouping
        let known_tools = vec![
            ("besen", "Besen"),
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
        let mut tool_usage: std::collections::HashMap<String, (u64, usize)> =
            std::collections::HashMap::new();
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
        sorted_tools.sort_by_key(|tool| std::cmp::Reverse(tool.1));

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
                format!(
                    "{}m {}s",
                    manual_duration / 60000,
                    (manual_duration % 60000) / 1000
                )
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
            let time = if entry.timestamp.len() >= 19 {
                &entry.timestamp[11..19]
            } else {
                &entry.timestamp
            };
            let dur = match entry.duration_ms {
                Some(ms) if ms >= 60000 => format!("{}m {}s", ms / 60000, (ms % 60000) / 1000),
                Some(ms) if ms >= 1000 => format!("{:.1}s", ms as f64 / 1000.0),
                Some(ms) => format!("{}ms", ms),
                None => "—".to_string(),
            };

            // Detect tool for row highlighting
            let cmd_lower = entry.command.to_lowercase();
            let first_word = cmd_lower.split_whitespace().next().unwrap_or("");
            let tool_name = known_tools
                .iter()
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

        // Everything an agent did, from the files that already recorded it
        // (XNAUT-267). A typed command is one row source out of six now, not
        // the only one, which is why this table is usually the whole report.
        let mut agent_rows = String::new();
        for item in &sources.activity {
            let time = if item.at.len() >= 19 {
                &item.at[11..19]
            } else {
                &item.at
            };
            agent_rows += &format!(
                "<tr><td>{}</td><td><span style='font-size:9px; padding:1px 4px; border-radius:2px; background:#6b7280; color:white;'>{}</span></td><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td></tr>\n",
                time,
                esc(&item.source),
                esc(&item.actor),
                esc(&item.kind),
                esc(&item.ticket),
                esc(&item.detail),
            );
        }
        if agent_rows.is_empty() {
            // Never render blank. A report that shows nothing and says nothing
            // is indistinguishable from a quiet day, and that is exactly how
            // this feature shipped broken for weeks.
            agent_rows = "<tr><td colspan='6' style='color:#888;'>No agent activity recorded in this window. The Sources table below says which file each reader looked at and what it found.</td></tr>\n".to_string();
        }

        let mut source_rows = String::new();
        for note in &sources.notes {
            let verdict = if note.note.is_empty() {
                format!("{} event(s)", note.count)
            } else {
                esc(&note.note)
            };
            source_rows += &format!(
                "<tr><td><strong>{}</strong></td><td>{}</td></tr>\n",
                esc(&note.source),
                verdict
            );
        }
        if source_rows.is_empty() {
            source_rows = "<tr><td colspan='2' style='color:#888;'>This report was built without collecting sources, so only typed commands appear.</td></tr>\n".to_string();
        }

        format!(
            r#"<!DOCTYPE html>
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
  <div class="meta-item"><div class="meta-label">Agent events</div><div class="meta-value">{}</div></div>
  <div class="meta-item"><div class="meta-label">Status</div><div class="meta-value {}">{}</div></div>
</div>

<h2>Agent Activity</h2>
<table>
<tr><th>Time</th><th>Source</th><th>Actor</th><th>Event</th><th>Ticket</th><th>Detail</th></tr>
{}
</table>

<h2>Sources</h2>
<table>
<tr><th>Source</th><th>Found</th></tr>
{}
</table>

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
            self.project,
            self.client,
            self.project,
            self.client,
            date,
            duration,
            self.entries.len(),
            sources.activity.len(),
            if verified { "verified" } else { "tampered" },
            if verified {
                "✓ Verified"
            } else {
                "✗ Tampered"
            },
            agent_rows,
            source_rows,
            tool_rows,
            rows,
            qr_svg,
            merkle,
            if verified { "verified" } else { "tampered" },
            if verified {
                "Chain integrity intact — no modifications detected"
            } else {
                "WARNING: Log has been modified"
            },
        )
    }
}

fn save_session(session: &WorkSession) {
    let path = worklog_dir().join(format!("{}.json", session.id));
    let _ = fs::write(
        path,
        serde_json::to_string_pretty(session).unwrap_or_default(),
    );
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

// ==================== Sessions the app forgot ====================
//
// `active_worklog` lives in memory and dies with the process, while every start
// and every logged command writes the session to disk. So closing xNAUT with the
// work log running left a file marked `"active": true` that nothing ever read
// back: the monitor stopped recording, the session was never finalized, and the
// hours vanished from the PM Space dashboard. Reported 2026-08-13 (XNAUT-139),
// with two such files already on disk.
//
// Resuming is deliberately NOT automatic. Between the crash and the relaunch
// time passed that nobody worked, and silently adopting the newest of several
// orphans would quietly close whichever one was real. The app asks.

/// Sessions that were never stopped.
///
/// `finalize()` sets `active = false` and stamps `ended`, so an unstopped
/// session is unambiguous on disk. Kept separate from the directory read so the
/// rule can be tested without a home directory to stage.
fn unfinished(sessions: Vec<WorkSession>) -> Vec<WorkSession> {
    let mut out: Vec<WorkSession> = sessions
        .into_iter()
        .filter(|s| s.active && s.ended.is_none())
        .collect();
    // Newest first: the one worth resuming is almost always the last one open.
    out.sort_by(|a, b| b.started.cmp(&a.started));
    out
}

fn read_sessions() -> Vec<WorkSession> {
    let mut sessions = Vec::new();
    if let Ok(entries) = fs::read_dir(worklog_dir()) {
        for entry in entries.flatten() {
            if entry
                .path()
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
            {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(session) = serde_json::from_str::<WorkSession>(&content) {
                        sessions.push(session);
                    }
                }
            }
        }
    }
    sessions
}

fn load_session(id: &str) -> Result<WorkSession, String> {
    let path = worklog_dir().join(format!("{id}.json"));
    let content = fs::read_to_string(&path).map_err(|e| format!("no session {id}: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("session {id} is unreadable: {e}"))
}

/// Work logs that were running when the app last went away.
///
/// Empty while a session is already active: there is nothing to ask about, and
/// offering to resume over a running log is how you lose the running one.
#[tauri::command]
pub async fn worklog_orphans(state: State<'_, AppState>) -> Result<Vec<WorkSession>, String> {
    if state.active_worklog.lock().await.is_some() {
        return Ok(Vec::new());
    }
    Ok(unfinished(read_sessions()))
}

/// Pick up an orphaned session where it left off.
///
/// The gap is recorded rather than hidden. Its entries carry timestamps, so a
/// resumed session that swallowed six hours of downtime would silently inflate
/// the burn figure; a marker entry makes it visible in the log itself.
#[tauri::command]
pub async fn worklog_resume(state: State<'_, AppState>, id: String) -> Result<WorkSession, String> {
    let mut active = state.active_worklog.lock().await;
    if active.is_some() {
        return Err("a work log is already running".to_string());
    }
    let mut session = load_session(&id)?;
    if !session.active || session.ended.is_some() {
        return Err(format!("session {id} was already stopped"));
    }
    session.add_entry(
        "xnaut restarted, work log resumed",
        "",
        Some("the app was closed while this log was running"),
    );
    save_session(&session);
    *active = Some(session.clone());
    Ok(session)
}

/// Close out an orphan without resuming it, so it stops being offered.
#[tauri::command]
pub async fn worklog_discard(id: String) -> Result<WorkSession, String> {
    let mut session = load_session(&id)?;
    if !session.active || session.ended.is_some() {
        return Err(format!("session {id} was already stopped"));
    }
    session.finalize();
    save_session(&session);
    Ok(session)
}

/// The session a report is about: the running one, or a finished one by id.
///
/// Both readers below used to serve the ACTIVE session only, and the one moment
/// anybody wants a summary is straight after `worklog_stop`, which has already
/// cleared `active_worklog`. So every call errored, the caller fell back, and
/// the summary and the QR code the panel promises were unreachable from the app.
fn pick_session(active: Option<WorkSession>, id: Option<String>) -> Result<WorkSession, String> {
    match id.filter(|id| !id.trim().is_empty()) {
        Some(id) => load_session(id.trim()),
        None => active.ok_or_else(|| "No active work session".to_string()),
    }
}

#[tauri::command]
pub async fn worklog_summary(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<String, String> {
    let active = state.active_worklog.lock().await.clone();
    Ok(pick_session(active, session_id)?.generate_summary())
}

#[tauri::command]
pub async fn worklog_qr(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<String, String> {
    let active = state.active_worklog.lock().await.clone();
    Ok(pick_session(active, session_id)?.generate_qr_svg())
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
            if entry
                .path()
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
            {
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
    sessions.sort_by(|a, b| b["started"].as_str().cmp(&a["started"].as_str()));
    Ok(sessions)
}

/// The control repo, when Project Management is on. `None` otherwise, and that
/// is a note in the report rather than an error: a work report is still a work
/// report without ticket moves in it.
async fn pm_repo(state: &State<'_, AppState>) -> Option<PathBuf> {
    let settings = state.settings.lock().await.project_management.clone();
    crate::project_management::configured_repo(&settings).ok()
}

fn parse_window(value: &str, label: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|at| at.with_timezone(&chrono::Utc))
        .map_err(|e| format!("{label} is not an RFC3339 timestamp: {e}"))
}

#[tauri::command]
pub async fn worklog_export_html(state: State<'_, AppState>) -> Result<String, String> {
    let repo = pm_repo(&state).await;
    let now = chrono::Utc::now();

    let active = state.active_worklog.lock().await;
    // Try active session first, then last saved
    if let Some(ref session) = *active {
        let from = parse_window(&session.started, "session start").unwrap_or(now);
        let sources = worklog_sources::collect(&Query {
            from,
            to: now,
            pm_repo: repo,
            skip_session: Some(&session.id),
        });
        return Ok(session.generate_html_report_with(&sources));
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

    match latest {
        Some((_, session)) => {
            let from = parse_window(&session.started, "session start").unwrap_or(now);
            let to = session
                .ended
                .as_deref()
                .and_then(|ended| parse_window(ended, "session end").ok())
                .unwrap_or(now);
            let sources = worklog_sources::collect(&Query {
                from,
                to,
                pm_repo: repo,
                skip_session: Some(&session.id),
            });
            Ok(session.generate_html_report_with(&sources))
        }
        // No session was ever started, which used to be a dead end. The agents
        // still worked; their records are on disk. Report the last day of them
        // rather than refusing (XNAUT-267).
        None => build_range_report(now - chrono::Duration::hours(24), now, repo),
    }
}

/// A report over an arbitrary past window, saved and returned as a path.
///
/// This is the retroactive half of XNAUT-267: the five agent sources have been
/// accumulating for weeks, so a window that closed long before this code was
/// written still fills in. Returns a path rather than the markup because the
/// only thing to do with a report is open it, exactly like worklog_save_report.
#[tauri::command]
pub async fn worklog_report_range(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<String, String> {
    let start = parse_window(&from, "from")?;
    let end = parse_window(&to, "to")?;
    if end <= start {
        return Err("the window ends before it starts".into());
    }
    let repo = pm_repo(&state).await;
    let html = build_range_report(start, end, repo)?;
    save_report(&html)
}

fn save_report(html: &str) -> Result<String, String> {
    let dir = worklog_dir();
    let filename = format!(
        "report-{}.html",
        chrono::Utc::now().format("%Y-%m-%d-%H%M%S")
    );
    let path = dir.join(&filename);
    fs::write(&path, html).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

fn build_range_report(
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    repo: Option<PathBuf>,
) -> Result<String, String> {
    // A window, not a recorded session. `entries` stays empty on purpose: the
    // typed commands from that window arrive through the collector like every
    // other source, so the Merkle chain in the report is not asked to vouch for
    // rows it never hashed.
    let mut window = WorkSession::new("Unassigned", "Recorded work");
    window.started = from.to_rfc3339();
    window.ended = Some(to.to_rfc3339());
    window.active = false;
    window.merkle_root = Some(window.compute_merkle_root());

    let sources = worklog_sources::collect(&Query {
        from,
        to,
        pm_repo: repo,
        skip_session: None,
    });
    Ok(window.generate_html_report_with(&sources))
}

#[tauri::command]
pub async fn worklog_save_report(state: State<'_, AppState>) -> Result<String, String> {
    let html = worklog_export_html(state).await?;
    save_report(&html)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, started: &str) -> WorkSession {
        let mut s = WorkSession::new("Client", "Project");
        s.id = id.to_string();
        s.started = started.to_string();
        s
    }

    /// One writer of XNAUT_WORKLOG_DIR at a time: tests share a process, and a
    /// second test repointing the directory mid-read is a flake nobody enjoys.
    fn staged(name: &str) -> (std::sync::MutexGuard<'static, ()>, WorkSession) {
        // Shared with worklog_sources::tests, which moves the same variable.
        let guard = worklog_sources::env_guard();
        let dir = std::env::temp_dir().join(format!("xnaut-worklog-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        std::env::set_var("XNAUT_WORKLOG_DIR", &dir);
        let mut s = session("stopped-1", "2026-08-13T10:00:00Z");
        s.add_entry("cargo test", "/tmp", None);
        s.finalize();
        save_session(&s);
        (guard, s)
    }

    #[test]
    fn a_stopped_session_can_still_be_summarised() {
        // The shipped bug: worklog_stop clears active_worklog, so the summary
        // and the QR were read from a session that no longer existed. Every
        // call errored, silently, into a one-line fallback.
        let (_guard, staged) = staged("summary");
        let picked = pick_session(None, Some(staged.id.clone()))
            .expect("a session that just stopped must still be readable by id");
        let summary = picked.generate_summary();
        assert!(
            summary.contains("cargo test"),
            "the real summary lists the commands: {summary}"
        );
        assert!(
            summary.contains("Merkle Root"),
            "the summary carries the proof: {summary}"
        );
        assert!(
            !picked.generate_qr_svg().is_empty(),
            "the QR the panel promises must render"
        );
    }

    #[test]
    fn a_running_session_is_still_the_default() {
        let (_guard, _staged) = staged("active");
        let mut running = session("running-1", "2026-08-13T12:00:00Z");
        running.add_entry("git status", "/tmp", None);
        let picked = pick_session(Some(running.clone()), None).expect("the active session answers");
        assert_eq!(picked.id, "running-1");
        assert!(
            pick_session(None, None).is_err(),
            "no session and no id is an error, not a blank page"
        );
    }

    #[test]
    fn a_session_that_was_never_stopped_is_offered() {
        let out = unfinished(vec![session("a", "2026-08-13T10:00:00Z")]);
        assert_eq!(out.len(), 1, "an unstopped session must be offered back");
        assert_eq!(out[0].id, "a");
    }

    #[test]
    fn a_stopped_session_is_not_resurrected() {
        let mut s = session("a", "2026-08-13T10:00:00Z");
        s.finalize();
        assert!(
            unfinished(vec![s]).is_empty(),
            "finalize() ends a session; offering it again would reopen work that was done"
        );
    }

    // The two fields are set together by finalize(), but they arrive from disk
    // and a half-written file must not read as still running.
    #[test]
    fn an_end_timestamp_alone_is_enough_to_count_as_stopped() {
        let mut s = session("a", "2026-08-13T10:00:00Z");
        s.ended = Some("2026-08-13T11:00:00Z".to_string());
        assert!(unfinished(vec![s]).is_empty());
    }

    #[test]
    fn the_newest_orphan_is_offered_first() {
        let out = unfinished(vec![
            session("older", "2026-08-13T10:00:00Z"),
            session("newest", "2026-08-13T19:59:55Z"),
            session("middle", "2026-08-13T19:56:59Z"),
        ]);
        assert_eq!(
            out.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["newest", "middle", "older"],
            "the log worth resuming is almost always the last one left open"
        );
    }

    /// Stage one agent tool call in a scratch evidence chain and hand back the
    /// window it falls in. Nobody types anything; that is the point.
    fn agent_only_window(
        name: &str,
    ) -> (
        std::sync::MutexGuard<'static, ()>,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    ) {
        use std::io::Write;
        let guard = worklog_sources::env_guard();
        let dir =
            std::env::temp_dir().join(format!("xnaut-worklog-rep-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("evidence")).unwrap();
        std::env::set_var("XNAUT_WORKLOG_ROOT", &dir);
        std::env::set_var("XNAUT_WORKLOG_DIR", dir.join("worklogs"));
        std::env::set_var("XNAUT_EVIDENCE_DIR", dir.join("evidence"));

        let at = chrono::Utc::now() - chrono::Duration::minutes(30);
        let mut handle = fs::File::create(dir.join("evidence").join("execution.jsonl")).unwrap();
        writeln!(
            handle,
            r#"{{"recorded_at":"{}","kind":"tool_call","session_id":"s-1","actor":{{"agent":"claude"}},"tool":{{"name":"Bash"}}}}"#,
            at.to_rfc3339()
        )
        .unwrap();
        (
            guard,
            chrono::Utc::now() - chrono::Duration::hours(2),
            chrono::Utc::now(),
        )
    }

    #[test]
    fn the_report_shows_agent_work_when_nobody_typed_a_command() {
        // The owner's complaint, exactly: "I tested. the report is empty." It
        // was, because the only feed was a human pressing Enter with
        // autocomplete watching. The session below has zero entries, which is
        // what every saved session on his machine looks like.
        let (_guard, from, to) = agent_only_window("agent-rows");
        let html = build_range_report(from, to, None).expect("a window is enough to report on");

        assert!(
            html.contains("Agent Activity"),
            "the report needs somewhere to put agent work"
        );
        assert!(
            html.contains("receipts") && html.contains("Bash"),
            "the tool call an agent made must appear: {}",
            &html[..html.len().min(400)]
        );
        assert!(
            html.contains(
                "<div class=\"meta-label\">Agent events</div><div class=\"meta-value\">1</div>"
            ),
            "the count at the top has to agree with the table"
        );
    }

    #[test]
    fn a_report_with_nothing_in_it_says_which_source_found_nothing_and_why() {
        // Silence that looks like health is the failure this sprint removes. An
        // empty report has to name every reader and what it looked at.
        let (_guard, _from, _to) = agent_only_window("empty-reasons");
        let long_ago = chrono::Utc::now() - chrono::Duration::days(400);
        let html = build_range_report(long_ago, long_ago + chrono::Duration::hours(1), None)
            .expect("an empty window still renders");

        assert!(
            html.contains("No agent activity recorded in this window"),
            "a blank table is indistinguishable from a quiet day"
        );
        for source in ["ledger", "receipts", "runs", "verify", "pm", "commands"] {
            assert!(
                html.contains(&format!("<strong>{source}</strong>")),
                "{source} did not account for itself in the report"
            );
        }
        assert!(
            html.contains("Project Management is off"),
            "a source that is off says so rather than reading as empty"
        );
    }

    #[test]
    fn a_window_that_closed_before_the_report_existed_still_fills_in() {
        // The retroactive claim. The records were written by other features for
        // their own reasons and have been accumulating for weeks, so a report
        // can be asked for a window that ended long ago.
        let (_guard, _from, _to) = agent_only_window("retroactive");
        let from = chrono::Utc::now() - chrono::Duration::hours(1);
        let to = chrono::Utc::now() - chrono::Duration::minutes(20);
        let html = build_range_report(from, to, None).expect("a past window is a valid report");
        assert!(
            html.contains("Bash"),
            "work recorded 30 minutes ago belongs in a window that covers it"
        );
    }

    #[test]
    fn resuming_records_the_gap_rather_than_hiding_it() {
        let mut s = session("a", "2026-08-13T10:00:00Z");
        let before = s.entries.len();
        s.add_entry("xnaut restarted, work log resumed", "", None);
        assert_eq!(s.entries.len(), before + 1);
        assert!(
            s.entries.last().unwrap().command.contains("restarted"),
            "downtime that leaves no trace inflates the burn figure silently"
        );
    }
}
