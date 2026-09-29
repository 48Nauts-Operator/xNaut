//! Registered repositories are infrastructure even when clean and idle.
//! Recheck at deletion time; a scan is not authority to delete a newly registered base.
use std::path::{Path, PathBuf};
use serde_json::Value;

fn json(path: &Path) -> Result<Option<Value>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| format!("Cannot validate cleanup registrations in {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(format!("Cannot read cleanup registrations in {}", path.display())),
    }
}
fn resolve(path: &str) -> PathBuf {
    let p = if let Some(rest) = path.strip_prefix("~/") { dirs::home_dir().unwrap_or_default().join(rest) } else { PathBuf::from(path) };
    std::fs::canonicalize(&p).unwrap_or(p)
}
fn add(paths: &mut Vec<(PathBuf, String)>, path: &str, label: String) {
    if !path.trim().is_empty() { paths.push((resolve(path), label)); }
}
pub fn registered_in(config: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut paths = Vec::new();
    let overrides = json(&config.join("project-paths.json"))?.unwrap_or(Value::Null);
    if let Some(map) = overrides.as_object() {
        for (key, path) in map { if let Some(path) = path.as_str() { add(&mut paths, path, format!("project {key}")); } }
    }
    if let Some(settings) = json(&config.join("settings.json"))? {
        if let Some(root) = settings.pointer("/project_management/repo_path").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
            add(&mut paths, root, "project control repository".into());
            let dir = resolve(root).join("projects");
            // Configured but unreadable registries must not permit cleanup.
            for entry in std::fs::read_dir(&dir).map_err(|_| format!("Cannot check registered projects in {}", dir.display()))? {
                let entry = entry.map_err(|_| "Cannot read registered project entry")?;
                if !entry.file_type().map_err(|_| "Cannot inspect project registration")?.is_dir() { continue; }
                if let Some(project) = json(&entry.path().join("project.json"))? {
                    let key = project["key"].as_str().unwrap_or("unknown");
                    if let Some(path) = overrides.get(key).and_then(Value::as_str).or_else(|| project["source_path"].as_str()) {
                        add(&mut paths, path, format!("project {key}"));
                    }
                }
            }
        }
    }
    if let Some(environments) = json(&config.join("launch-environments.json"))? {
        if let Some(list) = environments["environments"].as_array() {
            for env in list { if let Some(dir) = env["dir"].as_str() { add(&mut paths, dir, "launch environment".into()); } }
        }
    }
    // Some installations also carry explicit base directories on agent profiles.
    let file = config.join("agent-profiles.toml");
    match std::fs::read_to_string(&file) {
        Ok(body) => {
            let value: toml::Value = toml::from_str(&body).map_err(|_| "Cannot validate agent workspace registrations")?;
            if let Some(profiles) = value.get("profiles").and_then(toml::Value::as_array) {
                for profile in profiles {
                    for key in ["workspace_path", "working_dir", "repo_path", "cwd"] {
                        if let Some(path) = profile.get(key).and_then(toml::Value::as_str) {
                            let name = profile.get("handle").and_then(toml::Value::as_str).unwrap_or("agent");
                            add(&mut paths, path, format!("agent @{name}"));
                        }
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(_) => return Err("Cannot read agent workspace registrations".into()),
    }
    Ok(paths)
}
pub fn registered() -> Result<Vec<(PathBuf, String)>, String> {
    registered_in(&dirs::config_dir().ok_or("No config directory")?.join("xnaut"))
}
pub fn reason(path: &Path, registrations: &[(PathBuf, String)]) -> Option<String> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    registrations.iter().find(|(base, _)| base.starts_with(&target))
        .map(|(_, label)| format!("registered workspace for {label}; update its registration before removing it"))
}
pub fn assert_removable(path: &Path) -> Result<(), String> {
    if let Some(reason) = reason(path, &registered()?) { Err(reason) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protects_project_and_agent_bases_and_parent_but_not_ticket_children() {
        let temp = std::env::temp_dir().join(format!("worktree-protection-{}", uuid::Uuid::new_v4()));
        let config = temp.join("config"); let board = temp.join("board"); let base = temp.join("safety-net");
        std::fs::create_dir_all(board.join("projects/XNAUT")).unwrap(); std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("settings.json"), serde_json::json!({"project_management":{"repo_path":board}}).to_string()).unwrap();
        std::fs::write(board.join("projects/XNAUT/project.json"), serde_json::json!({"key":"XNAUT","source_path":base}).to_string()).unwrap();
        let registrations = registered_in(&config).unwrap();
        assert!(reason(&base, &registrations).unwrap().contains("project XNAUT"));
        assert!(reason(&temp, &registrations).is_some());
        assert!(reason(&base.join("ticket-440"), &registrations).is_none());
        std::fs::write(config.join("agent-profiles.toml"), format!("[[profiles]]\nhandle='claudi'\nworkspace_path='{}'\n", temp.join("claudi").display())).unwrap();
        assert!(reason(&temp.join("claudi"), &registered_in(&config).unwrap()).unwrap().contains("@claudi"));
        std::fs::write(config.join("project-paths.json"), "broken").unwrap();
        assert!(registered_in(&config).is_err());
        std::fs::remove_dir_all(temp).unwrap();
    }
}
