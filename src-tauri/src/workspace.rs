// Workspace Agentic capture — XNAUT-38 Phase 2 (Type 1: artifacts + created docs).
//
// Reads the coding agent's on-disk transcript for a project and extracts only
// CLEAR DATA — artifact/served-HTML links and documents/reports the agent
// created. Tool mechanics, thinking, and prose rambling are noise and are not
// captured. (Type 2 — brainstorm/idea threads — is the LLM Summarize pass,
// layered on top later.)

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct CaptureItem {
    /// "artifact" (a link) or "document" (a file the agent created).
    pub kind: String,
    /// Human label — artifact host/slug, or the document's basename.
    pub label: String,
    /// The URL (artifact) or absolute file path (document).
    pub target: String,
    /// ISO timestamp from the transcript line, if present.
    pub ts: Option<String>,
}

/// Claude Code stores transcripts under ~/.claude/projects/<sanitized-cwd>/*.jsonl
/// where the dir name is the cwd with every '/' and '.' replaced by '-'.
fn claude_transcript_dir(project_path: &str) -> Option<PathBuf> {
    let san: String = project_path
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    Some(dirs::home_dir()?.join(".claude").join("projects").join(san))
}

/// Newest .jsonl in a dir (best-effort "this session"), by mtime.
fn newest_jsonl(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "jsonl").unwrap_or(false))
        .max_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
        })
}

fn is_artifact_url(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    u.contains("/artifact/") || u.contains("claude.ai/code") || {
        // served HTML page (strip query/fragment first)
        let base = u.split(['?', '#']).next().unwrap_or(&u);
        base.ends_with(".html")
    }
}

fn is_document_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [".md", ".html", ".pdf", ".txt", ".csv", ".docx"]
        .iter()
        .any(|ext| p.ends_with(ext))
}

fn artifact_label(url: &str) -> String {
    // Prefer the last path segment (slug/uuid) with the host as context.
    let no_scheme = url.splitn(2, "://").nth(1).unwrap_or(url);
    let host = no_scheme.split('/').next().unwrap_or("");
    let slug = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    if slug.is_empty() || slug == host {
        host.to_string()
    } else {
        format!("{host} · {slug}")
    }
}

fn basename(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

/// Extract capture items from a single transcript file.
fn extract_from_transcript(file: &Path) -> Vec<CaptureItem> {
    let url_re = regex::Regex::new(r#"https?://[^\s<>()\[\]"`']+"#).expect("valid url regex");
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut items: Vec<CaptureItem> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in content.lines() {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if val.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let ts = val
            .get("timestamp")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        let Some(blocks) = val
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            continue;
        };
        for b in blocks {
            match b.get("type").and_then(|t| t.as_str()) {
                // Artifact / served-HTML links inside assistant prose.
                Some("text") => {
                    let text = b.get("text").and_then(|t| t.as_str()).unwrap_or("");
                    for m in url_re.find_iter(text) {
                        let url = m.as_str().trim_end_matches(&['.', ',', ')', ';', ':'][..]);
                        if is_artifact_url(url) && seen.insert(url.to_string()) {
                            items.push(CaptureItem {
                                kind: "artifact".into(),
                                label: artifact_label(url),
                                target: url.to_string(),
                                ts: ts.clone(),
                            });
                        }
                    }
                }
                // Documents/reports the agent created (Write only — Edit modifies).
                Some("tool_use") => {
                    if b.get("name").and_then(|n| n.as_str()) == Some("Write") {
                        if let Some(fp) = b
                            .get("input")
                            .and_then(|i| i.get("file_path"))
                            .and_then(|p| p.as_str())
                        {
                            if is_document_path(fp) && seen.insert(fp.to_string()) {
                                items.push(CaptureItem {
                                    kind: "document".into(),
                                    label: basename(fp),
                                    target: fp.to_string(),
                                    ts: ts.clone(),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    items
}

/// Frontend entry point: clean capture feed for a project's latest session.
#[tauri::command]
pub fn workspace_agentic_items(project_path: String) -> Result<Vec<CaptureItem>, String> {
    let Some(dir) = claude_transcript_dir(&project_path) else {
        return Ok(Vec::new());
    };
    let Some(file) = newest_jsonl(&dir) else {
        return Ok(Vec::new());
    };
    Ok(extract_from_transcript(&file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_and_document_detection() {
        assert!(is_artifact_url("https://claude.ai/code/artifact/abc-123"));
        assert!(is_artifact_url("http://host/preview/report.html"));
        assert!(is_artifact_url("http://host/report.html?v=2"));
        assert!(!is_artifact_url("https://github.com/org/repo"));
        assert!(is_document_path("/x/report.md"));
        assert!(is_document_path("/x/a.HTML"));
        assert!(!is_document_path("/x/main.rs"));
    }

    #[test]
    fn extracts_clean_items_no_noise() {
        let dir = std::env::temp_dir().join(format!("ws-cap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.jsonl");
        // assistant with an artifact link, a Write of a doc, a Write of code (noise),
        // a Bash tool_use (noise), and an ordinary URL (noise).
        let lines = [
            r#"{"type":"assistant","timestamp":"2026-07-14T10:00:00Z","message":{"content":[{"type":"text","text":"Here: https://claude.ai/code/artifact/xy-9 and see https://github.com/org/repo"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Write","input":{"file_path":"/p/report.md","content":"x"}}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Write","input":{"file_path":"/p/app.rs","content":"x"}}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#,
        ];
        std::fs::write(&f, lines.join("\n")).unwrap();
        let items = extract_from_transcript(&f);
        std::fs::remove_dir_all(&dir).ok();
        // exactly: 1 artifact + 1 document; github link, app.rs, and Bash are noise.
        assert_eq!(items.len(), 2, "got {items:?}");
        assert_eq!(items[0].kind, "artifact");
        assert_eq!(items[1].kind, "document");
        assert_eq!(items[1].label, "report.md");
    }
}
