// Phase 8b — the agent-facing HTTP broker for diff-note actions.
//
// Mounted on the existing Phase 5 agent_hooks listener (same port, same
// rate/size limits, same 127.0.0.1 binding). Action vocabulary mirrors
// hunk's session-broker: list / get / review / comment-add / comment-apply
// / comment-list / comment-rm / comment-clear. The wire-format payloads
// match hunk's JSON-RPC-ish single-endpoint pattern so agents that know
// hunk can talk to xNaut with no client changes.
//
// Authentication: the same credential every other route on this listener
// takes (XNAUT-350). Before that this broker took none at all, on the
// reasoning that loopback binding plus "a Host-header check (from
// agent_hooks)" was enough. That Host check has never existed, and the actions
// here are not read-only trivia: `review` runs a diff in any directory the body
// names, and every comment verb creates and writes `<worktree>/.xnaut/notes.json`
// wherever it is pointed. Any local process could do both, unidentified.
//
// The workflow the old comment protected still works: a CLI the user runs in
// their own terminal sends `Authorization: Bearer <MCP token>`, which is what
// `inbox::authorize` accepts from scripts and sandboxes, and an agent xNaut
// launched already has X-Xnaut-Session in its environment.

use crate::notes::{add_note, clear_notes, read_notes, remove_note, write_notes, Annotation};
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

fn default_true() -> bool {
    true
}

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

#[allow(clippy::too_many_arguments)]
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
    fn build_annotation(
        summary: String,
        rationale: Option<String>,
        author: Option<String>,
        tags: Vec<String>,
        confidence: Option<String>,
        source: Option<String>,
    ) -> Annotation {
        Annotation {
            summary,
            rationale,
            tags,
            confidence,
            source: source.or_else(|| Some("agent".into())),
            author,
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }

    let mut a = build_annotation(summary, rationale, author, tags, confidence, source);
    if side == "old" {
        a.old_range = Some([line, line]);
    } else {
        a.new_range = Some([line, line]);
    }
    (side.to_string(), a)
}

/// Who a note is attributed to.
///
/// `verified` is the handle of the session behind the request, `None` when the
/// caller authenticated with the MCP bearer instead. A verified session is the
/// author, full stop: an annotation signed by a name the caller chose is worth
/// less than an unsigned one, because it reads as evidence. A caller with no
/// session identity keeps what it sent, having nothing verified to overrule it.
fn note_author(verified: Option<&str>, claimed: Option<String>) -> Option<String> {
    match verified {
        Some(handle) if !handle.trim().is_empty() => Some(handle.trim().to_string()),
        Some(_) => None,
        None => claimed,
    }
}

pub async fn handle_notes(
    State(ctx): State<ServerCtx>,
    headers: axum::http::HeaderMap,
    Json(req): Json<NotesRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use serde_json::json;

    // Refuse first: a caller we cannot identify gets nothing, not even a read.
    let session = crate::inbox::authorize(&ctx, &headers).await?;
    let verified = match session {
        Some(_) => Some(
            crate::agent_hooks::session_handle(&ctx, &headers)
                .await
                .unwrap_or_default(),
        ),
        None => None,
    };
    let verified = verified.as_deref();

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
                    Ok(d) => {
                        out["diff"] = serde_json::to_value(d).unwrap_or(json!({}));
                    }
                    Err(e) => {
                        out["diff_error"] = json!(e);
                    }
                }
            }
            out
        }
        NotesRequest::CommentAdd {
            worktree,
            file_path,
            side,
            line,
            summary,
            rationale,
            author,
            tags,
            confidence,
            source,
            reveal,
        } => {
            let (_, ann) = make_annotation(
                &file_path,
                &side,
                line,
                summary,
                rationale,
                note_author(verified, author),
                tags,
                confidence,
                source,
            );
            let doc = add_note(Path::new(&worktree), &file_path, ann)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            let _ = ctx
                .app
                .emit("notes-changed", json!({ "worktree": worktree }));
            if reveal {
                let _ = ctx.app.emit(
                    "diff-reveal",
                    json!({ "worktree": worktree, "filePath": file_path, "side": side, "line": line }),
                );
            }
            serde_json::to_value(doc).unwrap_or(json!({}))
        }
        NotesRequest::CommentApply {
            worktree,
            comments,
            reveal_mode,
        } => {
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
                    note_author(verified, c.author.clone()),
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
            let _ = ctx
                .app
                .emit("notes-changed", json!({ "worktree": worktree }));
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
        NotesRequest::CommentList {
            worktree,
            file_path,
        } => {
            let doc = read_notes(Path::new(&worktree))
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            let out: Vec<&Annotation> = doc
                .files
                .iter()
                .filter(|f| file_path.as_deref().is_none_or(|fp| f.path == fp))
                .flat_map(|f| f.annotations.iter())
                .collect();
            serde_json::to_value(out).unwrap_or(json!([]))
        }
        NotesRequest::CommentRm {
            worktree,
            comment_id,
        } => {
            let doc = remove_note(Path::new(&worktree), &comment_id)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            let _ = ctx
                .app
                .emit("notes-changed", json!({ "worktree": worktree }));
            serde_json::to_value(doc).unwrap_or(json!({}))
        }
        NotesRequest::CommentClear {
            worktree,
            file_path,
            include_user,
        } => {
            let doc = clear_notes(Path::new(&worktree), file_path.as_deref(), include_user)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            let _ = ctx
                .app
                .emit("notes-changed", json!({ "worktree": worktree }));
            serde_json::to_value(doc).unwrap_or(json!({}))
        }
    };

    Ok(Json(serde_json::json!({ "ok": true, "result": result })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// XNAUT-350. A note is evidence in a review, so the name on it comes from
    /// the session that wrote it and not from the JSON that carried it.
    #[test]
    fn a_note_is_signed_by_the_session_not_by_the_body() {
        assert_eq!(
            note_author(Some("atlas"), Some("nautbot".into())),
            Some("atlas".into())
        );
        assert_eq!(note_author(Some("atlas"), None), Some("atlas".into()));
        // A session we cannot name signs nothing, rather than signing what the
        // body asked for.
        assert_eq!(note_author(Some(""), Some("nautbot".into())), None);
        // No session behind the request is the owner's own tooling over the
        // MCP bearer, which keeps the author it sent.
        assert_eq!(
            note_author(None, Some("hunk-cli".into())),
            Some("hunk-cli".into())
        );
        assert_eq!(note_author(None, None), None);
    }

    /// The broker used to serve every verb to anyone who could reach the port.
    ///
    /// A unit test cannot call the handler (it needs a running app), so prove
    /// the gate the only other way that fails when it is removed: the refusal
    /// stands before the first action is dispatched, and the route is not on
    /// the anonymous list.
    #[test]
    fn the_notes_route_takes_a_credential() {
        assert!(
            !crate::agent_hooks::anonymous_allowed("POST /v1/notes"),
            "the notes broker must never be an anonymous route"
        );
        let source = include_str!("agent_notes_broker.rs");
        let handler = source
            .split_once("pub async fn handle_notes(")
            .expect("handle_notes moved")
            .1;
        let before_dispatch = handler
            .split_once("let result: serde_json::Value = match req {")
            .expect("the dispatch moved")
            .0;
        assert!(
            before_dispatch.contains("crate::inbox::authorize(&ctx, &headers).await?"),
            "handle_notes must refuse an unidentified caller before it acts"
        );
    }
}
