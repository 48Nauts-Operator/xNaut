// The agent's canvas: a diagram the agent draws and the owner watches.
//
// Ported from Cockpit (48Nauts, private), `src/lib/concept-canvas.ts`,
// `src/lib/concept-realtime-tools.ts` and `src/app/api/concepts/tools/route.ts`.
// Cockpit drives its canvas with OpenAI Realtime function calls and renders it
// with React Flow; the contract is what travels, not the transport or the
// renderer.
//
// What we kept, because it is the reason Cockpit's canvas feels good to use:
//
//   * The agent sends the COMPLETE graph, never a patch. "Send the whole
//     desired graph, not only the changed box" removes a whole class of
//     half-applied edits, and a model is far better at restating a small
//     graph than at describing a diff.
//   * Positions are preserved BY ID across updates, and only new nodes are
//     laid out. Without this, every edit shuffles the drawing and the owner
//     loses the map he had in his head.
//   * The previous graph is snapshotted before each write, so an agent that
//     flattens the diagram is one step from undone.
//
// Where we departed: this is keyed by AGENT, not by conversation — an agent
// here outlives any one thread — and the tools are xNAUT tools rather than
// Realtime function calls, so the same canvas works with any provider. And no
// React Flow: xNAUT's frontend has no bundler, so the pane draws SVG directly.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What a box on the canvas IS. Same vocabulary as Cockpit's, because it is a
/// good one: it covers architecture, process and trust boundaries without
/// becoming a taxonomy nobody remembers.
pub const NODE_KINDS: &[&str] = &[
    "actor",
    "component",
    "service",
    "agent",
    "data-store",
    "external-system",
    "decision",
    "document",
    "note",
    "trust-boundary",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasNode {
    pub id: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// Set by the layout when the node is new, and preserved from then on.
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

fn default_kind() -> String {
    "component".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Canvas {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub nodes: Vec<CanvasNode>,
    #[serde(default)]
    pub edges: Vec<CanvasEdge>,
    /// The graph as it was before the last update. One step of undo, which is
    /// what an agent that flattens a diagram costs you without it.
    #[serde(default)]
    pub previous: Option<Box<Canvas>>,
    #[serde(default)]
    pub updated_at: String,
}

fn canvas_dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "could not resolve the config directory".to_string())?
        .join("xnaut")
        .join("canvases");
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    Ok(dir)
}

fn safe_key(key: &str) -> String {
    key.trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_lowercase()
}

fn canvas_path(key: &str) -> Result<PathBuf, String> {
    let key = safe_key(key);
    if key.is_empty() {
        return Err("which canvas?".into());
    }
    Ok(canvas_dir()?.join(format!("{key}.json")))
}

pub fn load(key: &str) -> Canvas {
    canvas_path(key)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Place the nodes that have no position yet, leaving the rest alone.
///
/// A grid, not a graph layout: it is honest about being a starting point, and
/// anything cleverer would move boxes the owner has already arranged.
fn place_new(nodes: &mut [CanvasNode], known: &[CanvasNode]) {
    const COLUMN: f64 = 260.0;
    const ROW: f64 = 150.0;
    let mut slot = known.len();
    for node in nodes.iter_mut() {
        if let Some(existing) = known.iter().find(|item| item.id == node.id) {
            node.x = existing.x;
            node.y = existing.y;
            continue;
        }
        if node.x != 0.0 || node.y != 0.0 {
            continue; // the caller placed it deliberately
        }
        node.x = 60.0 + (slot % 4) as f64 * COLUMN;
        node.y = 60.0 + (slot / 4) as f64 * ROW;
        slot += 1;
    }
}

/// Replace the graph, keeping what the owner has arranged.
pub fn update(key: &str, mut next: Canvas, now: String) -> Result<Canvas, String> {
    let current = load(key);
    next.nodes.retain(|node| !node.id.trim().is_empty());
    place_new(&mut next.nodes, &current.nodes);
    // An edge to a box that does not exist draws nothing and confuses the
    // model on the next read, so drop it and say nothing — the agent sends the
    // whole graph each time and will not miss it.
    let ids: Vec<&str> = next.nodes.iter().map(|node| node.id.as_str()).collect();
    next.edges
        .retain(|edge| ids.contains(&edge.source.as_str()) && ids.contains(&edge.target.as_str()));
    if next.title.trim().is_empty() {
        next.title = current.title.clone();
    }
    next.updated_at = now;
    next.previous = Some(Box::new(Canvas {
        previous: None,
        ..current
    }));

    let path = canvas_path(key)?;
    let text = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(next)
}

#[tauri::command]
pub fn canvas_get(key: String) -> Result<Canvas, String> {
    Ok(load(&key))
}

#[tauri::command]
pub fn canvas_set(app: tauri::AppHandle, key: String, canvas: Canvas) -> Result<Canvas, String> {
    let saved = update(&key, canvas, now_iso())?;
    let _ = tauri::Emitter::emit(&app, "canvas-changed", serde_json::json!({ "key": key }));
    Ok(saved)
}

/// Step back to the graph before the last update.
#[tauri::command]
pub fn canvas_undo(app: tauri::AppHandle, key: String) -> Result<Canvas, String> {
    let current = load(&key);
    let Some(previous) = current.previous else {
        return Err("nothing to undo on this canvas".into());
    };
    let restored = update(&key, *previous, now_iso())?;
    let _ = tauri::Emitter::emit(&app, "canvas-changed", serde_json::json!({ "key": key }));
    Ok(restored)
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// The graph as Mermaid, for pasting into a document. Cockpit's
/// `conceptCanvasToMarkdown` does the same thing and for the same reason: a
/// diagram nobody can paste anywhere is a diagram trapped in one app.
pub fn to_mermaid(canvas: &Canvas) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    for node in &canvas.nodes {
        let shape = match node.kind.as_str() {
            "decision" => format!("{{{}}}", node.label),
            "data-store" => format!("[({})]", node.label),
            "actor" | "external-system" => format!("([{}])", node.label),
            _ => format!("[{}]", node.label),
        };
        lines.push(format!("  {}{}", safe_key(&node.id), shape));
    }
    for edge in &canvas.edges {
        let arrow = if edge.label.trim().is_empty() {
            "-->".to_string()
        } else {
            format!("-- {} -->", edge.label.trim())
        };
        lines.push(format!(
            "  {} {arrow} {}",
            safe_key(&edge.source),
            safe_key(&edge.target)
        ));
    }
    lines.join("\n")
}

// ── Documents ───────────────────────────────────────────────────────────────
//
// The other half of Cockpit's split screen: `write_concept_document` puts a
// Markdown document beside the conversation, with Preview and Code. Same
// contract as the canvas — the agent sends the whole document, the previous
// one is kept for a single undo — because a report is edited by rewriting it,
// not by describing a diff.

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Document {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub previous: Option<Box<Document>>,
    #[serde(default)]
    pub updated_at: String,
}

fn document_path(key: &str) -> Result<PathBuf, String> {
    let key = safe_key(key);
    if key.is_empty() {
        return Err("which document?".into());
    }
    Ok(canvas_dir()?.join(format!("{key}.doc.json")))
}

pub fn load_document(key: &str) -> Document {
    document_path(key)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn write_document(key: &str, mut next: Document, now: String) -> Result<Document, String> {
    let current = load_document(key);
    if next.title.trim().is_empty() {
        next.title = current.title.clone();
    }
    next.updated_at = now;
    next.previous = Some(Box::new(Document { previous: None, ..current }));
    let path = document_path(key)?;
    let text = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(next)
}

#[tauri::command]
pub fn document_get(key: String) -> Result<Document, String> {
    Ok(load_document(&key))
}

#[tauri::command]
pub fn document_set(app: tauri::AppHandle, key: String, document: Document) -> Result<Document, String> {
    let saved = write_document(&key, document, now_iso())?;
    let _ = tauri::Emitter::emit(&app, "document-changed", serde_json::json!({ "key": key }));
    Ok(saved)
}

/// Put the document where documents live here: the work vault, under the
/// project, with the frontmatter every doc in it carries. An artifact that
/// only exists inside the app is an artifact that gets lost.
#[tauri::command]
pub fn document_save_to_vault(key: String, project: String, author: String) -> Result<String, String> {
    let document = load_document(&key);
    if document.content.trim().is_empty() {
        return Err("there is nothing written yet".into());
    }
    let project = if project.trim().is_empty() { "xNAUT".to_string() } else { project.trim().to_string() };
    let dir = dirs::home_dir()
        .ok_or_else(|| "could not resolve the home directory".to_string())?
        .join(".xnaut-vault")
        .join("work")
        .join(&project)
        .join("Development")
        .join("features");
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M %Z").to_string();
    let title = if document.title.trim().is_empty() { "Untitled".to_string() } else { document.title.trim().to_string() };
    let slug: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let path = dir.join(format!("{today}_{slug}.md"));
    let author = if author.trim().is_empty() { "xNAUT agent".to_string() } else { author.trim().to_string() };
    let body = format!(
        "---\nAuthor: {author}\nLast modified: {stamp}\n---\n\n# {title}\n\n{}\n",
        document.content.trim()
    );
    std::fs::write(&path, body).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, x: f64, y: f64) -> CanvasNode {
        CanvasNode {
            id: id.into(),
            kind: "component".into(),
            label: id.into(),
            description: String::new(),
            x,
            y,
        }
    }

    #[test]
    fn an_arranged_box_does_not_move_when_the_graph_is_redrawn() {
        // The reason Cockpit's canvas is usable: the agent restates the whole
        // graph every time, and the owner's arrangement survives it.
        let known = vec![node("api", 900.0, 400.0)];
        let mut next = vec![node("api", 0.0, 0.0), node("worker", 0.0, 0.0)];
        place_new(&mut next, &known);
        assert_eq!((next[0].x, next[0].y), (900.0, 400.0), "an existing box moved");
        assert!(next[1].x != 0.0 || next[1].y != 0.0, "a new box was never placed");
    }

    #[test]
    fn a_deliberate_position_is_respected() {
        let mut next = vec![node("api", 120.0, 240.0)];
        place_new(&mut next, &[]);
        assert_eq!((next[0].x, next[0].y), (120.0, 240.0));
    }

    #[test]
    fn an_edge_to_nowhere_is_dropped_rather_than_drawn() {
        let canvas = Canvas {
            nodes: vec![node("a", 0.0, 0.0)],
            edges: vec![CanvasEdge {
                id: "e1".into(),
                source: "a".into(),
                target: "ghost".into(),
                label: String::new(),
            }],
            ..Canvas::default()
        };
        let key = format!("test-edges-{}", std::process::id());
        let saved = update(&key, canvas, "now".into()).unwrap();
        let _ = std::fs::remove_file(canvas_path(&key).unwrap());
        assert!(saved.edges.is_empty(), "an edge to a missing box survived");
    }

    #[test]
    fn the_previous_graph_is_kept_for_one_step_of_undo() {
        let key = format!("test-undo-{}", std::process::id());
        let first = Canvas { nodes: vec![node("a", 0.0, 0.0)], ..Canvas::default() };
        update(&key, first, "t1".into()).unwrap();
        let second = Canvas { nodes: vec![node("b", 0.0, 0.0)], ..Canvas::default() };
        let saved = update(&key, second, "t2".into()).unwrap();
        let _ = std::fs::remove_file(canvas_path(&key).unwrap());

        let previous = saved.previous.expect("no snapshot was kept");
        assert_eq!(previous.nodes.len(), 1);
        assert_eq!(previous.nodes[0].id, "a");
        // And the snapshot does not carry its own snapshot, or the file grows
        // by a whole history on every edit.
        assert!(previous.previous.is_none());
    }

    #[test]
    fn a_canvas_can_leave_as_mermaid() {
        let canvas = Canvas {
            nodes: vec![
                CanvasNode { kind: "decision".into(), ..node("gate", 0.0, 0.0) },
                node("build", 0.0, 0.0),
            ],
            edges: vec![CanvasEdge {
                id: "e".into(),
                source: "gate".into(),
                target: "build".into(),
                label: "approved".into(),
            }],
            ..Canvas::default()
        };
        let mermaid = to_mermaid(&canvas);
        assert!(mermaid.contains("flowchart LR"));
        assert!(mermaid.contains("gate{gate}"), "{mermaid}");
        assert!(mermaid.contains("gate -- approved --> build"), "{mermaid}");
    }

    #[test]
    fn a_document_keeps_one_step_of_undo_and_its_title() {
        let key = format!("test-doc-{}", std::process::id());
        write_document(&key, Document { title: "Release notes".into(), content: "first".into(), ..Document::default() }, "t1".into()).unwrap();
        // A rewrite with no title keeps the one it had — a model that omits
        // the title should not blank it.
        let saved = write_document(&key, Document { content: "second".into(), ..Document::default() }, "t2".into()).unwrap();
        let _ = std::fs::remove_file(document_path(&key).unwrap());
        assert_eq!(saved.title, "Release notes");
        assert_eq!(saved.previous.unwrap().content, "first");
    }

    /// A canvas keyed by the request id is a canvas nobody can reopen.
    ///
    /// The chat panel sends a fresh UUID per turn. Keying the canvas by it made
    /// every diagram drawn from chat land in a file that existed for one
    /// request: the agent reported "it is on your canvas now", the owner saw an
    /// empty pane, and the graph sat under a UUID in the canvases directory.
    /// Found with 19 nodes of real architecture stranded that way.
    #[test]
    fn the_chat_canvas_is_keyed_by_the_conversation_not_the_request() {
        let chat = include_str!("chat.rs");
        assert!(
            chat.contains("chat_key"),
            "chat_send_tools no longer takes the conversation key"
        );
        let call = chat
            .split("agent_tools::run_turn")
            .nth(1)
            .expect("run_turn is no longer called from chat.rs");
        assert!(
            !call.starts_with("(&with_model, &chosen, history, None, &[], &request_id)"),
            "the canvas is keyed by the request id again — every drawing will be orphaned"
        );

        let panel = include_str!("../../src/js/chat-panel.js");
        assert!(
            panel.contains("chatKey: entry.chatKey"),
            "the frontend stopped sending chatKey, so the backend falls back to the request id"
        );
    }

    #[test]
    fn a_canvas_key_cannot_escape_its_directory() {
        assert!(canvas_path("../../etc/passwd").unwrap().starts_with(canvas_dir().unwrap()));
        assert!(canvas_path("  ").is_err());
    }
}
