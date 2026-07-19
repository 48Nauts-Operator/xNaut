// Session transcript lib — finds and reads coding-agent session logs on disk.
// One small brain over two on-disk formats:
//   • Claude Code — ~/.claude/projects/<sanitized-cwd>/<session>.jsonl
//                   lines: {type:"user"|"assistant", message:{content:[…]}}
//   • Codex       — ~/.codex/sessions/YYYY/MM/DD/rollout-*-<id>.jsonl
//                   line0 session_meta{payload:{id,cwd}}, msgs response_item{payload:{role,content:[{text}]}}
//
// Everything the Workspace does (Type-1 capture, backscan, session picker,
// and later Type-2 brainstorm) reads through here. Consumers get a normalized
// SessionMeta + a clean CaptureItem list; the format differences stay in here.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// One discoverable agent session for a project.
#[derive(Debug, Clone, Serialize)]
pub struct SessionMeta {
    pub id: String,
    pub agent: String, // "claude" | "codex"
    pub path: String,
    pub cwd: String,
    /// Unix-ms mtime for sorting (newest first).
    pub updated_ms: u64,
    /// Best-effort human title (first real user message).
    pub title: String,
}

/// A clean capture — the only things worth keeping from a session.
#[derive(Debug, Clone, Serialize)]
pub struct CaptureItem {
    pub kind: String, // "artifact" (a link) | "document" (a created file)
    pub label: String,
    pub target: String,
    pub agent: String,
    pub ts: Option<String>,
}

// ---- path helpers ---------------------------------------------------------

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Claude sanitizes the cwd into a dir name: every non-alphanumeric char
/// (`/`, `.`, `_`, …) becomes '-' — mirrors Claude Code's `replace(/[^a-zA-Z0-9]/g,'-')`.
fn claude_dir(project_path: &str) -> Option<PathBuf> {
    let san: String = project_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Some(home()?.join(".claude").join("projects").join(san))
}

fn mtime_ms(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>().trim())
    }
}

// ---- discovery ------------------------------------------------------------

fn claude_title(file: &Path) -> String {
    if let Ok(content) = std::fs::read_to_string(file) {
        for line in content.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if v.get("type").and_then(|t| t.as_str()) == Some("user") {
                if let Some(arr) = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                {
                    for b in arr {
                        if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                            if !t.trim().is_empty() {
                                return truncate(t, 60);
                            }
                        }
                    }
                } else if let Some(t) = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                {
                    if !t.trim().is_empty() {
                        return truncate(t, 60);
                    }
                }
            }
        }
    }
    "Session".into()
}

fn list_claude(project_path: &str) -> Vec<SessionMeta> {
    let Some(dir) = claude_dir(project_path) else {
        return vec![];
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    rd.filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "jsonl").unwrap_or(false))
        .map(|p| SessionMeta {
            id: p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string(),
            agent: "claude".into(),
            title: claude_title(&p),
            cwd: project_path.to_string(),
            updated_ms: mtime_ms(&p),
            path: p.to_string_lossy().into_owned(),
        })
        .collect()
}

/// Read the codex session_meta (first line) → (id, cwd), plus first real user msg as title.
fn codex_meta(file: &Path) -> Option<(String, String, String)> {
    let content = std::fs::read_to_string(file).ok()?;
    let mut lines = content.lines();
    let first = lines.next()?;
    let v: serde_json::Value = serde_json::from_str(first).ok()?;
    if v.get("type").and_then(|t| t.as_str()) != Some("session_meta") {
        return None;
    }
    let p = v.get("payload")?;
    let id = p.get("id").and_then(|x| x.as_str())?.to_string();
    let cwd = p
        .get("cwd")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    // title = first user response_item whose text isn't an env/permissions preamble
    let title = "Codex session".to_string();
    for line in lines {
        let Ok(o) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if o.get("type").and_then(|t| t.as_str()) != Some("response_item") {
            continue;
        }
        let pl = o.get("payload");
        if pl.and_then(|x| x.get("role")).and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if let Some(arr) = pl.and_then(|x| x.get("content")).and_then(|c| c.as_array()) {
            for b in arr {
                if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                    let tt = t.trim_start();
                    if tt.starts_with('<') {
                        continue;
                    } // <environment_context> / <permissions …>
                    if !tt.is_empty() {
                        return Some((id, cwd, truncate(tt, 60)));
                    }
                }
            }
        }
    }
    Some((id, cwd, title))
}

fn list_codex(project_path: &str) -> Vec<SessionMeta> {
    let Some(root) = home().map(|h| h.join(".codex").join("sessions")) else {
        return vec![];
    };
    let mut out = Vec::new();
    walk_jsonl(&root, &mut |p| {
        if let Some((id, cwd, title)) = codex_meta(p) {
            // Match sessions run in (or under) the project.
            if cwd == project_path || cwd.starts_with(&format!("{project_path}/")) {
                out.push(SessionMeta {
                    id,
                    agent: "codex".into(),
                    title,
                    cwd,
                    updated_ms: mtime_ms(p),
                    path: p.to_string_lossy().into_owned(),
                });
            }
        }
    });
    out
}

/// Recursively visit every *.jsonl under `dir`, calling `f` per file.
fn walk_jsonl(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            walk_jsonl(&p, f);
        } else if p.extension().map(|x| x == "jsonl").unwrap_or(false) {
            f(&p);
        }
    }
}

/// All sessions (both agents) for a project, newest first.
pub fn list_sessions(project_path: &str) -> Vec<SessionMeta> {
    let mut all = list_claude(project_path);
    all.extend(list_codex(project_path));
    all.sort_by_key(|s| std::cmp::Reverse(s.updated_ms));
    all
}

pub fn newest_session(project_path: &str) -> Option<SessionMeta> {
    list_sessions(project_path).into_iter().next()
}

pub fn find_session(project_path: &str, id: &str) -> Option<SessionMeta> {
    list_sessions(project_path).into_iter().find(|s| s.id == id)
}

// ---- capture extraction ---------------------------------------------------

fn is_artifact_url(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    u.contains("/artifact/") || u.contains("claude.ai/code") || {
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
    let no_scheme = url.split_once("://").map(|x| x.1).unwrap_or(url);
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

fn scan_text(
    text: &str,
    agent: &str,
    cwd: &str,
    ts: &Option<String>,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Vec<CaptureItem>,
) {
    // 1. artifact / served-HTML links
    let mut url_spans: Vec<(usize, usize)> = Vec::new();
    for m in artifact_url_re().find_iter(text) {
        url_spans.push((m.start(), m.end()));
        let url = m.as_str().trim_end_matches(&['.', ',', ')', ';', ':'][..]);
        if is_artifact_url(url) && seen.insert(url.to_string()) {
            out.push(CaptureItem {
                kind: "artifact".into(),
                label: artifact_label(url),
                target: url.to_string(),
                agent: agent.to_string(),
                ts: ts.clone(),
            });
        }
    }
    // 2. document file-paths mentioned in prose (created reports/docs the agent
    //    made via Bash/generators, not the Write tool). Resolve to an openable path.
    for m in doc_path_re().find_iter(text) {
        if url_spans
            .iter()
            .any(|&(s, e)| m.start() >= s && m.start() < e)
        {
            continue; // inside a URL we already handled
        }
        let raw = m.as_str().trim_end_matches(&['.', ',', ')', ';', ':'][..]);
        if let Some(abs) = resolve_doc(raw, cwd) {
            if seen.insert(abs.clone()) {
                out.push(CaptureItem {
                    kind: "document".into(),
                    label: basename(raw),
                    target: abs,
                    agent: agent.to_string(),
                    ts: ts.clone(),
                });
            }
        }
    }
}

fn artifact_url_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r#"(?:https?|file)://[^\s<>()\[\]"`']+"#).expect("valid url regex")
    })
}

fn doc_path_re() -> &'static regex::Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    // path-ish token ending in a document extension (name char right before the dot)
    RE.get_or_init(|| {
        regex::Regex::new(r"[~\w./+-]*[\w-]\.(?:md|html?|pdf|txt|csv|docx)\b")
            .expect("valid doc-path regex")
    })
}

/// Resolve a prose-mentioned doc path to an EXISTING absolute path, or None.
/// Requiring the file to exist filters out placeholders (e.g. "file.md"),
/// hypothetical mentions, and docs the agent only referenced but didn't make.
fn resolve_doc(raw: &str, cwd: &str) -> Option<String> {
    let cands: Vec<PathBuf> = if raw.starts_with('/') {
        vec![PathBuf::from(raw)]
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home().map(|h| vec![h.join(rest)]).unwrap_or_default()
    } else {
        let mut v = vec![Path::new(cwd).join(raw)];
        if let Some(h) = home() {
            v.push(h.join(raw));
        }
        v
    };
    cands
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Clean captures from a session: artifact links (both agents) + created
/// documents (Claude Write tool). Tool mechanics, thinking, code files, and
/// plain URLs are excluded.
pub fn extract_captures(session: &SessionMeta) -> Vec<CaptureItem> {
    let Ok(content) = std::fs::read_to_string(&session.path) else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match session.agent.as_str() {
            "claude" => {
                if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
                    continue;
                }
                let ts = v
                    .get("timestamp")
                    .and_then(|t| t.as_str())
                    .map(str::to_string);
                let Some(blocks) = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                else {
                    continue;
                };
                for b in blocks {
                    match b.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            let t = b.get("text").and_then(|t| t.as_str()).unwrap_or("");
                            scan_text(t, "claude", &session.cwd, &ts, &mut seen, &mut out);
                        }
                        Some("tool_use")
                            if b.get("name").and_then(|n| n.as_str()) == Some("Write") =>
                        {
                            if let Some(fp) = b
                                .get("input")
                                .and_then(|i| i.get("file_path"))
                                .and_then(|p| p.as_str())
                            {
                                if is_document_path(fp) && seen.insert(fp.to_string()) {
                                    out.push(CaptureItem {
                                        kind: "document".into(),
                                        label: basename(fp),
                                        target: fp.to_string(),
                                        agent: "claude".into(),
                                        ts: ts.clone(),
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "codex" => {
                if v.get("type").and_then(|t| t.as_str()) != Some("response_item") {
                    continue;
                }
                let ts = v
                    .get("timestamp")
                    .and_then(|t| t.as_str())
                    .map(str::to_string);
                let p = v.get("payload");
                if p.and_then(|x| x.get("role")).and_then(|r| r.as_str()) != Some("assistant") {
                    continue;
                }
                if let Some(arr) = p.and_then(|x| x.get("content")).and_then(|c| c.as_array()) {
                    for b in arr {
                        if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                            scan_text(t, "codex", &session.cwd, &ts, &mut seen, &mut out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_artifacts_and_docs() {
        assert!(is_artifact_url("https://claude.ai/code/artifact/abc"));
        assert!(is_artifact_url("http://h/report.html?v=1"));
        assert!(!is_artifact_url("https://github.com/o/r"));
        assert!(is_document_path("/x/a.md"));
        assert!(!is_document_path("/x/a.rs"));
    }

    #[test]
    fn claude_capture_is_clean() {
        let dir = std::env::temp_dir().join(format!("tx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.jsonl");
        std::fs::write(&f, [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"a https://claude.ai/code/artifact/z and https://github.com/o/r"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Write","input":{"file_path":"/p/report.md"}}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Write","input":{"file_path":"/p/x.rs"}}]}}"#,
        ].join("\n")).unwrap();
        let s = SessionMeta {
            id: "s".into(),
            agent: "claude".into(),
            path: f.to_string_lossy().into(),
            cwd: "/p".into(),
            updated_ms: 0,
            title: "t".into(),
        };
        let items = extract_captures(&s);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "artifact");
        assert_eq!(items[1].label, "report.md");
    }

    #[test]
    fn codex_artifact_capture() {
        let dir = std::env::temp_dir().join(format!("txc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("c.jsonl");
        std::fs::write(&f, [
            r#"{"type":"session_meta","payload":{"id":"cid","cwd":"/proj"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"see http://host/out.html done"}]}}"#,
        ].join("\n")).unwrap();
        let s = SessionMeta {
            id: "cid".into(),
            agent: "codex".into(),
            path: f.to_string_lossy().into(),
            cwd: "/proj".into(),
            updated_ms: 0,
            title: "t".into(),
        };
        let items = extract_captures(&s);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "artifact");
        assert_eq!(items[0].agent, "codex");
    }
}
