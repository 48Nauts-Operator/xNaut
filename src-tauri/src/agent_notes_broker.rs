// Phase 8b — the agent-facing HTTP broker for diff-note actions.
//
// Mounted on the existing Phase 5 agent_hooks listener (same port, same
// rate/size limits, same 127.0.0.1 binding). Action vocabulary mirrors
// hunk's session-broker: list / get / review / comment-add / comment-apply
// / comment-list / comment-rm / comment-clear. The wire-format payloads
// match hunk's JSON-RPC-ish single-endpoint pattern so agents that know
// hunk can talk to xNaut with no client changes.
//
// Authentication: loopback-only. Hunk uses no token; we don't either for
// now — the existing X-Xnaut-Session token check is for the Phase 5
// status hook, where the writer is a child process of an xNaut-spawned
// agent. Notes-from-agents may be CLI tools the user runs from a separate
// terminal, so requiring the per-session token would break the workflow.
// Loopback binding + Host-header check (from agent_hooks) covers DNS
// rebinding; broader auth lands when a real threat model emerges.

use crate::notes::{
    add_note, clear_notes, read_notes, remove_note, write_notes, Annotation,
};
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use std::path::Path;
use tauri::Emitter;

// Reuse ServerCtx from agent_hooks for the AppHandle.
use crate::agent_hooks::ServerCtx;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum NotesRequest {
    /// List all known worktrees that have a notes.json. Returns empty for now —
    /// xNaut doesn't track this globally; clients should know their own paths.
    List,
    /// Get the full notes doc for a worktree.
    Get { worktree: String },
    /// Get the diff + notes together (mirror of hunk's "review" verb).
    Review {
        worktree: String,
        #[serde(default)]
        include_patch: bool,
        #[serde(default = "default_true")]
        include_notes: bool,
    },
    /// Add a single annotation to a file.
    CommentAdd {
        worktree: String,
        #[serde(rename = "filePath")]
        file_path: String,
        side: String, // "old" | "new"
        line: u32,
        summary: String,
        #[serde(default)]
        rationale: Option<String>,
        #[serde(default)]
        author: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        confidence: Option<String>,
        #[serde(default)]
        source: Option<String>,
        #[serde(default = "default_true")]
        reveal: bool,
    },
    /// Apply a batch of comments transactionally — validates all first, then mutates.
    CommentApply {
        worktree: String,
        comments: Vec<CommentApplyItem>,
        #[serde(default)]
        #[serde(rename = "revealMode")]
        reveal_mode: Option<String>, // "none" | "first"
    },
    /// List comments for a worktree, optionally filtered by file.
    CommentList {
        worktree: String,
        #[serde(default)]
        #[serde(rename = "filePath")]
        file_path: Option<String>,
    },
    /// Remove a single comment by id.
    CommentRm {
        worktree: String,
        #[serde(rename = "commentId")]
        comment_id: String,
    },
    /// Clear comments. file_path optional; include_user controls user-authored protection.
    CommentClear {
        worktree: String,
        #[serde(default)]
        #[serde(rename = "filePath")]
        file_path: Option<String>,
        #[serde(default)]
        #[serde(rename = "includeUser")]
        include_user: bool,
    },
}

fn default_true() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct CommentApplyItem {
    #[serde(rename = "filePath")]
    pub file_path: String,
    pub side: String,
    pub line: u32,
    pub summary: String,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub confidence: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

fn make_annotation(
    file_path: &str,
    side: &str,
    line: u32,
    summary: String,
    rationale: Option<String>,
    author: Option<String>,
    tags: Vec<String>,
    confidence: Option<String>,
    source: Option<String>,
) -> (String, Annotation) {
    let _ = file_path;
    let mut a = Annotation::default();
    a.summary = summary;
    a.rationale = rationale;
    a.tags = tags;
    a.confidence = confidence;
    a.source = source.or_else(|| Some("agent".into()));
    a.author = author;
    a.created_at = Some(chrono::Utc::now().to_rfc3339());
    if side == "old" {
        a.old_range = Some([line, line]);
    } else {
        a.new_range = Some([line, line]);
    }
    (side.to_string(), a)
}

pub async fn handle_notes(
    State(ctx): State<ServerCtx>,
    Json(req): Json<NotesRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use serde_json::json;

    let result: serde_json::Value = match req {
        NotesRequest::List => json!({ "sessions": [] }),
        NotesRequest::Get { worktree } => {
            let doc = read_notes(Path::new(&worktree))
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            serde_json::to_value(doc).unwrap_or(json!({}))
        }
        NotesRequest::Review {
            worktree,
            include_patch,
            include_notes,
        } => {
            let mut out = json!({});
            if include_notes {
                let doc = read_notes(Path::new(&worktree))
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
                out["notes"] = serde_json::to_value(doc).unwrap_or(json!({}));
            }
            if include_patch {
                match crate::diff::diff_for_worktree(worktree.clone()) {
                    Ok(d) => { out["diff"] = serde_json::to_value(d).unwrap_or(json!({})); }
                    Err(e) => { out["diff_error"] = json!(e); }
                }
            }
            out
        }
        NotesRequest::CommentAdd {
            worktree, file_path, side, line, summary, rationale, author, tags, confidence, source, reveal,
        } => {
            let (_, ann) = make_annotation(&file_path, &side, line, summary, rationale, author, tags, confidence, source);
            let doc = add_note(Path::new(&worktree), &file_path, ann)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            let _ = ctx.app.emit("notes-changed", json!({ "worktree": worktree }));
            if reveal {
                let _ = ctx.app.emit(
                    "diff-reveal",
                    json!({ "worktree": worktree, "filePath": file_path, "side": side, "line": line }),
                );
            }
            serde_json::to_value(doc).unwrap_or(json!({}))
        }
        NotesRequest::CommentApply { worktree, comments, reveal_mode } => {
            // Validate all first (we already trust the types via serde — keep the batch
            // semantics: build all annotations, then commit one mutation pass).
            let mut annotations = Vec::new();
            for c in &comments {
                let (_, ann) = make_annotation(
                    &c.file_path,
                    &c.side,
                    c.line,
                    c.summary.clone(),
                    c.rationale.clone(),
                    c.author.clone(),
                    c.tags.clone(),
                    c.confidence.clone(),
                    c.source.clone(),
                );
                annotations.push((c.file_path.clone(), ann));
            }
            let mut doc = read_notes(Path::new(&worktree))
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            for (path, ann) in annotations {
                let entry = doc.files.iter_mut().find(|f| f.path == path);
                let target = match entry {
                    Some(e) => e,
                    None => {
                        doc.files.push(crate::notes::FileNotes {
                            path,
                            summary: None,
                            annotations: Vec::new(),
                        });
                        doc.files.last_mut().unwrap()
                    }
                };
                let mut a = ann;
                if a.id.is_none() {
                    a.id = Some(uuid::Uuid::new_v4().to_string());
                }
                target.annotations.push(a);
            }
            write_notes(Path::new(&worktree), &doc)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            let _ = ctx.app.emit("notes-changed", json!({ "worktree": worktree }));
            // Reveal the first new comment if requested
            if reveal_mode.as_deref() == Some("first") {
                if let Some(c) = comments.first() {
                    let _ = ctx.app.emit(
                        "diff-reveal",
                        json!({ "worktree": worktree, "filePath": c.file_path, "side": c.side, "line": c.line }),
                    );
                }
            }
            serde_json::to_value(doc).unwrap_or(json!({}))
        }
        NotesRequest::CommentList { worktree, file_path } => {
            let doc = read_notes(Path::new(&worktree))
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            let out: Vec<&Annotation> = doc
                .files
                .iter()
                .filter(|f| file_path.as_deref().map_or(true, |fp| f.path == fp))
                .flat_map(|f| f.annotations.iter())
                .collect();
            serde_json::to_value(out).unwrap_or(json!([]))
        }
        NotesRequest::CommentRm { worktree, comment_id } => {
            let doc = remove_note(Path::new(&worktree), &comment_id)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            let _ = ctx.app.emit("notes-changed", json!({ "worktree": worktree }));
            serde_json::to_value(doc).unwrap_or(json!({}))
        }
        NotesRequest::CommentClear { worktree, file_path, include_user } => {
            let doc = clear_notes(Path::new(&worktree), file_path.as_deref(), include_user)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            let _ = ctx.app.emit("notes-changed", json!({ "worktree": worktree }));
            serde_json::to_value(doc).unwrap_or(json!({}))
        }
    };

    Ok(Json(serde_json::json!({ "ok": true, "result": result })))
}
