// Notes storage + file watcher for Phase 8 (diff viewer with annotations).
//
// Annotations live at `<worktree>/.xnaut/notes.json` in the same shape hunk
// uses for its agent-context.json (so we can swap data sources without
// touching the frontend). The struct is range-on-both-sides (oldRange +
// newRange) — one note renders correctly in split or unified view because
// both anchors are present.
//
// File watching uses the `notify` crate to emit an event whenever notes.json
// changes on disk — the frontend's diff pane subscribes and re-renders
// without a full reload. This is the broker-less integration path: agents
// can edit notes.json directly (CLI tools, sandboxed processes) and xNaut
// hot-reloads. The HTTP broker (Phase 8b) layers on top of this primitive.

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Annotation {
    #[serde(default)]
    pub id: Option<String>,
    /// Inclusive line range on the pre-image side (1-indexed).
    #[serde(rename = "oldRange", default)]
    pub old_range: Option<[u32; 2]>,
    /// Inclusive line range on the post-image side (1-indexed).
    #[serde(rename = "newRange", default)]
    pub new_range: Option<[u32; 2]>,
    /// Headline body — always present.
    pub summary: String,
    /// Muted secondary body for the "why" of a note.
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// "low" / "medium" / "high" — kept loose so hunk-style values round-trip.
    #[serde(default)]
    pub confidence: Option<String>,
    /// "ai" / "agent" / "user" / a custom string.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// ISO 8601 timestamp set by whoever creates the note.
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<String>,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub editable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileNotes {
    pub path: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesDoc {
    /// Schema version. Currently 1 — bump when shape changes.
    pub version: u32,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub files: Vec<FileNotes>,
}

impl Default for NotesDoc {
    fn default() -> Self {
        Self {
            version: 1,
            summary: None,
            files: Vec::new(),
        }
    }
}

fn notes_dir(worktree: &Path) -> PathBuf {
    worktree.join(".xnaut")
}

fn notes_path(worktree: &Path) -> PathBuf {
    notes_dir(worktree).join("notes.json")
}

/// Reads the notes for a worktree; returns an empty doc if the file doesn't exist.
pub fn read_notes(worktree: &Path) -> Result<NotesDoc, String> {
    let path = notes_path(worktree);
    if !path.exists() {
        return Ok(NotesDoc::default());
    }
    let body = std::fs::read_to_string(&path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str::<NotesDoc>(&body)
        .map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Writes the notes for a worktree, creating the .xnaut directory if needed.
/// Pretty-printed because users may edit this file by hand.
pub fn write_notes(worktree: &Path, doc: &NotesDoc) -> Result<(), String> {
    let dir = notes_dir(worktree);
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let path = notes_path(worktree);
    let body = serde_json::to_string_pretty(doc)
        .map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

// ─── Watcher: emit `notes-changed` when the file mutates on disk ─────────────

#[derive(Default)]
pub struct NotesWatcher {
    inner: Mutex<Option<RecommendedWatcher>>,
    watching: Mutex<Option<PathBuf>>,
}

impl NotesWatcher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

/// Begin watching a worktree's notes.json. Replaces any existing watch.
/// Emits `notes-changed` with the worktree path on every change.
pub async fn start_watch(
    state: &Arc<NotesWatcher>,
    app: &AppHandle,
    worktree: PathBuf,
) -> Result<(), String> {
    // Drop the previous watcher first (notify is single-watch per instance).
    {
        let mut inner = state.inner.lock().await;
        *inner = None;
    }

    let dir = notes_dir(&worktree);
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;

    let app_clone = app.clone();
    let worktree_clone = worktree.clone();

    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let _ = app_clone.emit(
                        "notes-changed",
                        serde_json::json!({
                            "worktree": worktree_clone.to_string_lossy(),
                        }),
                    );
                }
            }
        })
        .map_err(|e| format!("create watcher: {e}"))?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| format!("watch {}: {e}", dir.display()))?;

    {
        let mut inner = state.inner.lock().await;
        *inner = Some(watcher);
        let mut path = state.watching.lock().await;
        *path = Some(worktree);
    }
    Ok(())
}

/// Stop the active watcher. Safe to call multiple times.
pub async fn stop_watch(state: &Arc<NotesWatcher>) {
    let mut inner = state.inner.lock().await;
    *inner = None;
    let mut path = state.watching.lock().await;
    *path = None;
}

// ─── Mutators used by both the Tauri commands and the Phase 8b broker ──────

pub fn add_note(
    worktree: &Path,
    file_path: &str,
    note: Annotation,
) -> Result<NotesDoc, String> {
    let mut doc = read_notes(worktree)?;
    let entry = doc
        .files
        .iter_mut()
        .find(|f| f.path == file_path);
    let target = match entry {
        Some(e) => e,
        None => {
            doc.files.push(FileNotes {
                path: file_path.to_string(),
                summary: None,
                annotations: Vec::new(),
            });
            doc.files.last_mut().unwrap()
        }
    };
    let mut n = note;
    if n.id.is_none() {
        n.id = Some(uuid::Uuid::new_v4().to_string());
    }
    target.annotations.push(n);
    write_notes(worktree, &doc)?;
    Ok(doc)
}

pub fn remove_note(worktree: &Path, note_id: &str) -> Result<NotesDoc, String> {
    let mut doc = read_notes(worktree)?;
    for f in doc.files.iter_mut() {
        f.annotations.retain(|a| a.id.as_deref() != Some(note_id));
    }
    write_notes(worktree, &doc)?;
    Ok(doc)
}

pub fn clear_notes(
    worktree: &Path,
    file_path: Option<&str>,
    include_user: bool,
) -> Result<NotesDoc, String> {
    let mut doc = read_notes(worktree)?;
    for f in doc.files.iter_mut() {
        if let Some(fp) = file_path {
            if f.path != fp {
                continue;
            }
        }
        f.annotations.retain(|a| {
            if include_user {
                false
            } else {
                a.source.as_deref() == Some("user")
            }
        });
    }
    write_notes(worktree, &doc)?;
    Ok(doc)
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn notes_read(worktree: String) -> Result<NotesDoc, String> {
    read_notes(Path::new(&worktree))
}

#[tauri::command]
pub fn notes_write(worktree: String, doc: NotesDoc) -> Result<(), String> {
    write_notes(Path::new(&worktree), &doc)
}

#[tauri::command]
pub fn notes_add(
    worktree: String,
    file_path: String,
    note: Annotation,
) -> Result<NotesDoc, String> {
    add_note(Path::new(&worktree), &file_path, note)
}

#[tauri::command]
pub fn notes_remove(worktree: String, note_id: String) -> Result<NotesDoc, String> {
    remove_note(Path::new(&worktree), &note_id)
}

#[tauri::command]
pub fn notes_clear(
    worktree: String,
    file_path: Option<String>,
    include_user: bool,
) -> Result<NotesDoc, String> {
    clear_notes(Path::new(&worktree), file_path.as_deref(), include_user)
}

#[tauri::command]
pub async fn notes_watch_start(
    app: AppHandle,
    worktree: String,
) -> Result<(), String> {
    let state = app.state::<Arc<NotesWatcher>>().inner().clone();
    start_watch(&state, &app, PathBuf::from(worktree)).await
}

#[tauri::command]
pub async fn notes_watch_stop(app: AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<NotesWatcher>>().inner().clone();
    stop_watch(&state).await;
    Ok(())
}

// We use a small sleep helper in tests; keep deps minimal here.
#[allow(dead_code)]
async fn brief_sleep() {
    tokio::time::sleep(Duration::from_millis(80)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("xnaut-notes-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn empty_read_returns_default_doc() {
        let d = tmpdir();
        let doc = read_notes(&d).unwrap();
        assert_eq!(doc.version, 1);
        assert!(doc.files.is_empty());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn round_trip_persists_annotation() {
        let d = tmpdir();
        let mut doc = NotesDoc::default();
        doc.files.push(FileNotes {
            path: "src/lib.rs".into(),
            summary: Some("tightened the parser".into()),
            annotations: vec![Annotation {
                id: Some("a1".into()),
                old_range: None,
                new_range: Some([42, 42]),
                summary: "watch the unwrap".into(),
                rationale: Some("crashes on EOF".into()),
                tags: vec!["correctness".into()],
                confidence: Some("high".into()),
                source: Some("agent".into()),
                author: Some("claude".into()),
                created_at: None,
                updated_at: None,
                editable: Some(true),
            }],
        });
        write_notes(&d, &doc).unwrap();
        let back = read_notes(&d).unwrap();
        assert_eq!(back.files.len(), 1);
        assert_eq!(back.files[0].annotations.len(), 1);
        assert_eq!(back.files[0].annotations[0].summary, "watch the unwrap");
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn add_note_creates_file_entry_if_absent() {
        let d = tmpdir();
        let n = Annotation {
            summary: "test".into(),
            ..Default::default()
        };
        let doc = add_note(&d, "newfile.rs", n).unwrap();
        assert_eq!(doc.files.len(), 1);
        assert_eq!(doc.files[0].path, "newfile.rs");
        assert!(doc.files[0].annotations[0].id.is_some());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn remove_note_drops_matching_id() {
        let d = tmpdir();
        let mut n = Annotation {
            id: Some("keep".into()),
            summary: "k".into(),
            ..Default::default()
        };
        add_note(&d, "f.rs", n.clone()).unwrap();
        n.id = Some("kill".into());
        n.summary = "drop me".into();
        add_note(&d, "f.rs", n).unwrap();
        let doc = remove_note(&d, "kill").unwrap();
        assert_eq!(doc.files[0].annotations.len(), 1);
        assert_eq!(doc.files[0].annotations[0].id.as_deref(), Some("keep"));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn clear_notes_with_include_user_drops_all() {
        let d = tmpdir();
        let mut user_note = Annotation {
            summary: "u".into(),
            source: Some("user".into()),
            ..Default::default()
        };
        let agent_note = Annotation {
            summary: "a".into(),
            source: Some("agent".into()),
            ..Default::default()
        };
        add_note(&d, "f.rs", user_note.clone()).unwrap();
        add_note(&d, "f.rs", agent_note).unwrap();
        let doc = clear_notes(&d, None, true).unwrap();
        assert!(doc.files[0].annotations.is_empty());
        // without include_user, the user note survives
        add_note(&d, "f.rs", user_note).unwrap();
        add_note(
            &d,
            "f.rs",
            Annotation {
                summary: "a2".into(),
                source: Some("agent".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let doc = clear_notes(&d, None, false).unwrap();
        assert_eq!(doc.files[0].annotations.len(), 1);
        assert_eq!(doc.files[0].annotations[0].source.as_deref(), Some("user"));
        std::fs::remove_dir_all(d).ok();
    }
}
