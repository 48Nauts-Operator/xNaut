// Skill locator — returns the absolute path to bundled skill files so
// external agents can read them. Mirrors hunk's `hunk skill path` command.

use std::path::PathBuf;

fn skills_root() -> Option<PathBuf> {
    // Dev/build-from-source path: the skills/ directory next to src-tauri/.
    let exe = std::env::current_exe().ok()?;
    let mut p = exe.clone();
    // Walk up to find a parent that has a `skills` sibling. Handles both
    // `target/debug/xnaut` and inside the bundled .app/Contents/MacOS layout.
    for _ in 0..6 {
        p.pop();
        let candidate = p.join("skills");
        if candidate.is_dir() {
            return Some(candidate);
        }
        // Also try `../../skills` from the .app Resources dir
        let candidate2 = p.join("Resources").join("skills");
        if candidate2.is_dir() {
            return Some(candidate2);
        }
    }
    // Fall back to the project layout from cargo run
    let cwd = std::env::current_dir().ok()?;
    let cwd_skills = cwd.join("skills");
    if cwd_skills.is_dir() {
        return Some(cwd_skills);
    }
    None
}

#[tauri::command]
pub fn skill_path(name: String) -> Result<String, String> {
    let root = skills_root().ok_or_else(|| "skills directory not found".to_string())?;
    let path = root.join(&name).join("SKILL.md");
    if !path.exists() {
        return Err(format!("skill not found: {}", path.display()));
    }
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn skill_list() -> Result<Vec<String>, String> {
    let root = skills_root().ok_or_else(|| "skills directory not found".to_string())?;
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten() {
            if e.path().is_dir() && e.path().join("SKILL.md").is_file() {
                if let Some(name) = e.file_name().to_str() {
                    out.push(name.to_string());
                }
            }
        }
    }
    out.sort();
    Ok(out)
}
