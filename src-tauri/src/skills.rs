// Skill library (XNAUT-158). Skills are plain markdown: a folder with a
// SKILL.md carrying YAML frontmatter (name, description) and instructions
// below. Deliberately the same shape Claude Code uses, so a skill written for
// one works in the other and nobody has to port a library into a bespoke
// format.
//
// Four roots, in precedence order — the first hit wins, so a user copy can
// shadow a bundled one without editing anything we ship:
//   project   <project>/.xnaut/skills      travels with the repo
//   user      ~/…/xnaut/skills             yours, editable, where downloads land
//   claude    ~/.claude/skills             read so an existing library works today
//   bundled   next to the binary           ships with xNAUT, read-only
//
// Skills are INSTRUCTIONS, not payload: the composer lists enabled names with
// one-line descriptions and the agent reads the file on demand. Preloading
// bodies would spend the context window before the task is read (the tier
// rule from the 2026-08-10 knowledge doc).

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    Project,
    User,
    Claude,
    Bundled,
}

#[derive(Clone, Debug, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: String,
    pub source: SkillSource,
    /// Only the user root is writable from the app; the rest are read-only so
    /// an update or a reinstall never silently discards an edit.
    pub editable: bool,
}

fn bundled_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut p = exe.clone();
    for _ in 0..6 {
        p.pop();
        let candidate = p.join("skills");
        if candidate.is_dir() {
            return Some(candidate);
        }
        let candidate2 = p.join("Resources").join("skills");
        if candidate2.is_dir() {
            return Some(candidate2);
        }
    }
    let cwd = std::env::current_dir().ok()?;
    let cwd_skills = cwd.join("skills");
    if cwd_skills.is_dir() {
        return Some(cwd_skills);
    }
    None
}

pub fn user_root() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut").join("skills"))
        .unwrap_or_else(|| PathBuf::from(".xnaut/skills"))
}

fn claude_root() -> Option<PathBuf> {
    dirs::home_dir()
        .map(|p| p.join(".claude").join("skills"))
        .filter(|p| p.is_dir())
}

fn project_root(project: Option<&str>) -> Option<PathBuf> {
    let project = project?.trim();
    if project.is_empty() {
        return None;
    }
    let path = Path::new(project).join(".xnaut").join("skills");
    path.is_dir().then_some(path)
}

/// Name and description from the frontmatter. A skill without frontmatter is
/// still listed — a folder with instructions is more useful than an error —
/// it simply falls back to the directory name.
pub fn parse_frontmatter(contents: &str, fallback_name: &str) -> (String, String) {
    let mut name = fallback_name.to_string();
    let mut description = String::new();
    let trimmed = contents.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let value = value.trim().trim_matches('"').trim_matches('\'');
                match key.trim() {
                    "name" if !value.is_empty() => name = value.to_string(),
                    "description" if !value.is_empty() => description = value.to_string(),
                    _ => {}
                }
            }
        }
    }
    (name, description)
}

fn read_root(root: &Path, source: SkillSource, out: &mut Vec<Skill>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let manifest = dir.join("SKILL.md");
        if !dir.is_dir() || !manifest.is_file() {
            continue;
        }
        let Some(folder) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        // First root wins: a user copy shadows the bundled one of the same name.
        if out.iter().any(|skill| skill.name == folder) {
            continue;
        }
        let contents = std::fs::read_to_string(&manifest).unwrap_or_default();
        let (_, description) = parse_frontmatter(&contents, &folder);
        out.push(Skill {
            name: folder,
            description,
            path: manifest.to_string_lossy().into_owned(),
            source,
            editable: source == SkillSource::User,
        });
    }
}

/// Every skill visible to this project, nearest root first.
#[tauri::command]
pub fn skill_catalog(project: Option<String>) -> Result<Vec<Skill>, String> {
    let mut out: Vec<Skill> = Vec::new();
    if let Some(root) = project_root(project.as_deref()) {
        read_root(&root, SkillSource::Project, &mut out);
    }
    read_root(&user_root(), SkillSource::User, &mut out);
    if let Some(root) = claude_root() {
        read_root(&root, SkillSource::Claude, &mut out);
    }
    if let Some(root) = bundled_root() {
        read_root(&root, SkillSource::Bundled, &mut out);
    }
    out.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    Ok(out)
}

/// Folder names only: no separators, no traversal. A downloaded skill must
/// never be able to write outside the skills directory.
fn safe_skill_name(name: &str) -> Result<String, String> {
    let cleaned = name.trim();
    if cleaned.is_empty() {
        return Err("a skill needs a name".to_string());
    }
    if cleaned.contains('/') || cleaned.contains('\\') || cleaned.contains("..") {
        return Err("a skill name cannot contain a path".to_string());
    }
    if !cleaned
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("a skill name may use letters, numbers, - and _ only".to_string());
    }
    Ok(cleaned.to_ascii_lowercase())
}

const SKILL_TEMPLATE: &str = r#"---
name: {name}
description: One line on when an agent should reach for this.
---

# {name}

## When to use

## Steps

1.
"#;

/// Create or overwrite a skill in the user root. Markdown in, nothing else.
#[tauri::command]
pub fn skill_write(name: String, contents: Option<String>) -> Result<Skill, String> {
    let name = safe_skill_name(&name)?;
    let dir = user_root().join(&name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create the skill folder: {e}"))?;
    let manifest = dir.join("SKILL.md");
    let body = contents
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| SKILL_TEMPLATE.replace("{name}", &name));
    std::fs::write(&manifest, &body).map_err(|e| format!("could not write the skill: {e}"))?;
    let (_, description) = parse_frontmatter(&body, &name);
    Ok(Skill {
        name,
        description,
        path: manifest.to_string_lossy().into_owned(),
        source: SkillSource::User,
        editable: true,
    })
}

/// Import a skill from a folder (with SKILL.md) or a single markdown file.
/// Copies into the user root so the original can move or vanish.
#[tauri::command]
pub fn skill_import(source_path: String, name: Option<String>) -> Result<Skill, String> {
    let source = PathBuf::from(source_path.trim());
    if !source.exists() {
        return Err(format!("nothing found at {}", source.display()));
    }
    let (manifest, default_name) = if source.is_dir() {
        let manifest = source.join("SKILL.md");
        if !manifest.is_file() {
            return Err("that folder has no SKILL.md".to_string());
        }
        let folder = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("skill")
            .to_string();
        (manifest, folder)
    } else {
        let stem = source
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("skill")
            .to_string();
        (source.clone(), stem)
    };
    let contents = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("could not read the skill: {e}"))?;
    let chosen = name.unwrap_or(default_name);
    skill_write(chosen, Some(contents))
}

#[tauri::command]
pub fn skill_delete(name: String) -> Result<(), String> {
    let name = safe_skill_name(&name)?;
    let dir = user_root().join(&name);
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("could not remove the skill: {e}"))
}

#[tauri::command]
pub fn skill_read(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("could not read {path}: {e}"))
}

// ---- kept for existing callers -----------------------------------------

#[tauri::command]
pub fn skill_path(name: String) -> Result<String, String> {
    let name = safe_skill_name(&name)?;
    for skill in skill_catalog(None)? {
        if skill.name == name {
            return Ok(skill.path);
        }
    }
    Err(format!("skill not found: {name}"))
}

#[tauri::command]
pub fn skill_list() -> Result<Vec<String>, String> {
    Ok(skill_catalog(None)?
        .into_iter()
        .map(|skill| skill.name)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_gives_the_description_the_library_shows() {
        let (name, description) = parse_frontmatter(
            "---\nname: prove-it\ndescription: Verify before claiming done.\n---\n\n# body\n",
            "folder-name",
        );
        assert_eq!(name, "prove-it");
        assert_eq!(description, "Verify before claiming done.");
    }

    #[test]
    fn a_skill_without_frontmatter_is_still_listed() {
        // A folder of instructions is more useful than an error.
        let (name, description) = parse_frontmatter("# Just instructions\n", "release-check");
        assert_eq!(name, "release-check");
        assert!(description.is_empty());
    }

    #[test]
    fn a_downloaded_skill_cannot_escape_the_skills_directory() {
        for attempt in ["../../etc/passwd", "a/b", "..", "with space"] {
            assert!(safe_skill_name(attempt).is_err(), "{attempt} was accepted");
        }
        assert_eq!(safe_skill_name(" Prove-It ").unwrap(), "prove-it");
    }

    #[test]
    fn the_first_root_wins_so_a_user_copy_shadows_a_bundled_one() {
        let dir = std::env::temp_dir().join(format!("xnaut-skills-{}", uuid::Uuid::new_v4()));
        let skill = dir.join("demo");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "---\ndescription: mine\n---\n").unwrap();
        let mut out = Vec::new();
        read_root(&dir, SkillSource::User, &mut out);
        read_root(&dir, SkillSource::Bundled, &mut out);
        assert_eq!(out.len(), 1, "the same name must not appear twice");
        assert_eq!(out[0].source, SkillSource::User);
        assert!(out[0].editable);
        std::fs::remove_dir_all(&dir).ok();
    }
}
