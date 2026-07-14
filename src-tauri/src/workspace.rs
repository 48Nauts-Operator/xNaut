// Workspace commands — thin Tauri layer over the transcripts lib (XNAUT-38).
// All session discovery + capture extraction lives in `transcripts`; this file
// just exposes it to the frontend.

use crate::transcripts::{self, CaptureItem, SessionMeta};

/// List the coding-agent sessions (Claude + Codex) discoverable for a project,
/// newest first. Powers the Agentic session picker / backscan.
#[tauri::command]
pub fn workspace_sessions(project_path: String) -> Result<Vec<SessionMeta>, String> {
    Ok(transcripts::list_sessions(&project_path))
}

/// Clean captures (artifacts + created documents) for a project session.
/// `session` = a specific session id to backscan; omitted → newest session.
#[tauri::command]
pub fn workspace_agentic_items(
    project_path: String,
    session: Option<String>,
) -> Result<Vec<CaptureItem>, String> {
    let meta = match session {
        Some(id) if !id.is_empty() => transcripts::find_session(&project_path, &id),
        _ => transcripts::newest_session(&project_path),
    };
    Ok(meta.map(|m| transcripts::extract_captures(&m)).unwrap_or_default())
}
