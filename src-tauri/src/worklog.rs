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

        let timestamp = chrono::Utc::now().to_rfc3339();
        let hash_input = format!("{}|{}|{}|{}", timestamp, command, directory, prev_hash);
        let hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

        self.entries.push(WorkLogEntry {
            timestamp,
            command: command.to_string(),
            directory: directory.to_string(),
            output_summary: output_summary.map(|s| s.to_string()),
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
