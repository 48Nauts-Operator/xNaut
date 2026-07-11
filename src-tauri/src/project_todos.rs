// Per-project to-do / reminder store. Central JSON keyed by task_id at
// ~/Library/Application Support/xnaut/project-todos.json — never writes into the
// project folders. Shape: { "<task_id>": [ { id, text, done, created } ] }.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub done: bool,
    pub created: String,
}

type Store = HashMap<String, Vec<Todo>>;

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn store_path() -> PathBuf {
    config_dir().join("project-todos.json")
}

fn load() -> Store {
    match std::fs::read_to_string(store_path()) {
        Ok(body) => serde_json::from_str(&body).unwrap_or_default(),
        Err(_) => Store::new(),
    }
}

pub fn load_all() -> HashMap<String, Vec<Todo>> {
    load()
}

fn save(store: &Store) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let body = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(store_path(), body).map_err(|e| format!("failed to write todos: {e}"))
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn project_todos_list(task_id: String) -> Result<Vec<Todo>, String> {
    Ok(load().get(&task_id).cloned().unwrap_or_default())
}

#[tauri::command]
pub fn project_todos_add(task_id: String, text: String) -> Result<Vec<Todo>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("task text is required".into());
    }
    let mut store = load();
    let list = store.entry(task_id.clone()).or_default();
    list.push(Todo {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.to_string(),
        done: false,
        created: chrono::Utc::now().to_rfc3339(),
    });
    let out = list.clone();
    save(&store)?;
    Ok(out)
}

#[tauri::command]
pub fn project_todos_toggle(task_id: String, todo_id: String) -> Result<Vec<Todo>, String> {
    let mut store = load();
    let list = store.get_mut(&task_id).ok_or("no todos for project")?;
    let t = list
        .iter_mut()
        .find(|t| t.id == todo_id)
        .ok_or("todo not found")?;
    t.done = !t.done;
    let out = list.clone();
    save(&store)?;
    Ok(out)
}

#[tauri::command]
pub fn project_todos_remove(task_id: String, todo_id: String) -> Result<Vec<Todo>, String> {
    let mut store = load();
    let list = store.get_mut(&task_id).ok_or("no todos for project")?;
    list.retain(|t| t.id != todo_id);
    let out = list.clone();
    save(&store)?;
    Ok(out)
}
