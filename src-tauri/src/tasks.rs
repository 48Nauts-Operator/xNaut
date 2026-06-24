// Persistent registry of Tasks Mode sessions (v1.6) — every task/project maps
// to a Zellij session. Stored as a JSON Vec at ~/.config/xnaut/tasks.json.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One registered task or project. `kind` is "project" (real folder under
/// project_root, usually with a forge repo) or "task" (lightweight scratch dir).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSession {
    /// Stable uuid — registry key.
    pub id: String,
    pub name: String,
    /// "project" | "task"
    pub kind: String,
    /// Absolute folder path the Zellij session runs in.
    pub path: String,
    pub zellij_session: String,
    pub agent_id: Option<String>,
    /// RFC3339 creation timestamp.
    pub created: String,
    /// Category label for projects (e.g. "Development").
    #[serde(default)]
    pub project_type: Option<String>,
    /// Clone URL when a forge repo exists.
    #[serde(default)]
    pub forge_remote: Option<String>,
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn tasks_path() -> PathBuf {
    config_dir().join("tasks.json")
}

/// Loads the task registry. A missing file or parse error yields an empty list
/// (parse errors are logged, matching the settings.rs pattern).
pub fn load_tasks() -> Vec<TaskSession> {
    let path = tasks_path();
    match std::fs::read_to_string(&path) {
        Ok(body) => serde_json::from_str(&body).unwrap_or_else(|e| {
            eprintln!(
                "[tasks] parse error in {}: {e} — starting with empty registry",
                path.display()
            );
            Vec::new()
        }),
        Err(_) => Vec::new(),
    }
}

/// Writes the full registry to ~/.config/xnaut/tasks.json, creating the dir.
pub fn save_tasks(tasks: &[TaskSession]) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let path = tasks_path();
    let body = serde_json::to_string_pretty(tasks).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

/// Pure upsert: replace the entry with a matching id, else append.
fn upsert_into(tasks: &mut Vec<TaskSession>, t: TaskSession) {
    if let Some(existing) = tasks.iter_mut().find(|x| x.id == t.id) {
        *existing = t;
    } else {
        tasks.push(t);
    }
}

/// Inserts or replaces (by id) a task in the persisted registry.
pub fn upsert_task(t: TaskSession) -> Result<(), String> {
    let mut tasks = load_tasks();
    upsert_into(&mut tasks, t);
    save_tasks(&tasks)
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

/// Lists all registered tasks/projects.
#[tauri::command]
pub fn tasks_list() -> Result<Vec<TaskSession>, String> {
    Ok(load_tasks())
}

/// Registers a lightweight project entry (no zellij/git/forge — that's what
/// scaffold_init_project is for). Used by the PM intake modal so a client
/// project can be linked without going through the chat scaffold flow.
/// `path` is optional; when blank it defaults to `<project_root>/<sanitized-name>`
/// and the folder is created if missing.
#[tauri::command]
pub fn tasks_create_project(name: String, path: Option<String>) -> Result<TaskSession, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("project name is required".into());
    }

    // Dedupe by name: a second click (or re-create) returns the existing
    // project instead of spawning a twin.
    if let Some(existing) = load_tasks()
        .into_iter()
        .find(|t| t.kind == "project" && t.name.eq_ignore_ascii_case(name))
    {
        return Ok(existing);
    }

    let path = match path.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            // Default location: <project_root>/<Development category folder>/<name>
            // — same place the chat scaffold flow puts new projects.
            let settings = crate::settings::load_or_default();
            let root = settings.project_root;
            if root.trim().is_empty() {
                return Err("set a project root in Settings → Tasks Mode, or enter a path".into());
            }
            let category_folder = settings
                .categories
                .iter()
                .find(|c| c.label.eq_ignore_ascii_case("Development"))
                .map(|c| c.folder.clone())
                .unwrap_or_else(|| "02-Development".to_string());
            PathBuf::from(root)
                .join(category_folder)
                .join(sanitize_folder(name))
        }
    };
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("failed to create {}: {e}", path.display()))?;

    let task = TaskSession {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        kind: "project".into(),
        path: path.to_string_lossy().into_owned(),
        zellij_session: String::new(),
        agent_id: None,
        created: chrono::Utc::now().to_rfc3339(),
        project_type: None,
        forge_remote: None,
    };
    upsert_task(task.clone())?;
    Ok(task)
}

/// Keeps alphanumerics, dashes and underscores; spaces become dashes.
fn sanitize_folder(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            c if c.is_alphanumeric() || c == '-' || c == '_' => c,
            ' ' => '-',
            _ => '-',
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Removes a registry entry by id. Never touches folders on disk.
#[tauri::command]
pub fn task_remove(id: String) -> Result<(), String> {
    let mut tasks = load_tasks();
    let before = tasks.len();
    tasks.retain(|t| t.id != id);
    if tasks.len() == before {
        return Err(format!("no task with id {id}"));
    }
    save_tasks(&tasks)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, name: &str) -> TaskSession {
        TaskSession {
            id: id.into(),
            name: name.into(),
            kind: "task".into(),
            path: format!("/tmp/{name}"),
            zellij_session: name.into(),
            agent_id: None,
            created: "2026-06-10T00:00:00Z".into(),
            project_type: None,
            forge_remote: None,
        }
    }

    #[test]
    fn upsert_appends_when_id_is_new() {
        let mut tasks = vec![task("a", "one")];
        upsert_into(&mut tasks, task("b", "two"));
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[1].id, "b");
    }

    #[test]
    fn upsert_replaces_in_place_when_id_exists() {
        let mut tasks = vec![task("a", "one"), task("b", "two")];
        let mut updated = task("a", "one-renamed");
        updated.kind = "project".into();
        upsert_into(&mut tasks, updated);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].name, "one-renamed");
        assert_eq!(tasks[0].kind, "project");
        assert_eq!(tasks[1].id, "b");
    }
}
