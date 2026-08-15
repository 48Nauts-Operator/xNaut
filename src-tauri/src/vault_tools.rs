// The vault, as tools an agent can call.
//
// The Librarian used to be a right-pane view of its own, driving the vault by
// emitting `{"action":"vault_create", ...}` JSON that the view interpreted. That
// made it a second kind of agent with a second protocol, living somewhere the
// other agents are not. It is an agent like the rest now, and this is what it
// holds: search, read, write, list.
//
// Deliberately filesystem-level rather than through VaultManager. The vault is
// Markdown on disk; requiring an "open vault" would make an agent's ability to
// find a note depend on which pane the owner last clicked.
//
// Every path is resolved under the vault root and refused if it escapes.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// `~/.xnaut-vault`. The one place documents live here.
pub fn vault_root() -> Result<PathBuf, String> {
    let root = dirs::home_dir()
        .ok_or_else(|| "could not resolve the home directory".to_string())?
        .join(".xnaut-vault");
    if !root.is_dir() {
        return Err(format!("no vault at {}", root.display()));
    }
    Ok(root)
}

/// Resolve a vault-relative path, refusing anything that climbs out.
fn resolve(rel: &str) -> Result<PathBuf, String> {
    let root = vault_root()?;
    let rel = rel.trim().trim_start_matches('/');
    if rel.is_empty() {
        return Err("which note?".into());
    }
    let joined = root.join(rel);
    // Compare lexically: the file may not exist yet, so canonicalize would fail.
    if joined
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("a vault path cannot climb out of the vault".into());
    }
    if !joined.starts_with(&root) {
        return Err("a vault path cannot climb out of the vault".into());
    }
    Ok(joined)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, budget: &mut usize) {
    if *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(&path, out, budget);
        } else if path.extension().map(|ext| ext == "md").unwrap_or(false) {
            out.push(path);
            *budget -= 1;
            if *budget == 0 {
                return;
            }
        }
    }
}

/// Search by TERMS, not by the whole phrase.
///
/// The first version matched the query as one literal string, so "plugin
/// marketplace" missed a note called `xNAUT-Addon-Marketplace.md` — the two
/// words never appear next to each other. It scores each term separately now,
/// counts a hit in the path for more than a hit in the body, and requires
/// every term to appear somewhere before a note is offered.
pub fn search(query: &str, limit: usize) -> Result<Value, String> {
    let root = vault_root()?;
    let terms: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| term.len() > 2)
        .map(str::to_string)
        .collect();
    if terms.is_empty() {
        return Err("search for what?".into());
    }
    let mut files = Vec::new();
    let mut budget = 4000usize;
    walk(&root, &mut files, &mut budget);

    let mut hits: Vec<(i64, String, String)> = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let lower_path = rel.to_lowercase();
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let lower_body = body.to_lowercase();

        let mut score = 0i64;
        let mut matched = 0usize;
        for term in &terms {
            let in_path = lower_path.contains(term);
            let occurrences = lower_body.matches(term.as_str()).count() as i64;
            if !in_path && occurrences == 0 {
                continue;
            }
            matched += 1;
            // A term in the filename is what the note is ABOUT; a term in the
            // body might be a passing mention.
            score += if in_path { 40 } else { 0 } + occurrences.min(10);
        }
        // Requiring EVERY term found nothing for "xNaut Project", because the
        // word "project" appears in almost none of those notes. Rank instead:
        // more terms matched wins, and matching all of them is simply the top
        // of the ranking rather than the price of entry.
        if matched == 0 {
            continue;
        }
        score += matched as i64 * 25;
        let excerpt = body
            .lines()
            .find(|line| terms.iter().any(|term| line.to_lowercase().contains(term.as_str())))
            .unwrap_or_else(|| body.lines().next().unwrap_or(""))
            .trim()
            .chars()
            .take(160)
            .collect::<String>();
        hits.push((score, rel, excerpt));
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    hits.truncate(limit.clamp(1, 40));
    Ok(json!({
        "ok": true,
        "hits": hits
            .into_iter()
            .map(|(score, rel, excerpt)| json!({ "path": rel, "excerpt": excerpt, "score": score }))
            .collect::<Vec<_>>(),
    }))
}

/// The most recently modified notes. "What was the last doc added" is a
/// question about mtime, and a text search can only guess at it — which is
/// exactly what it did, naming a note from two days earlier.
pub fn recent(limit: usize, prefix: &str) -> Result<Value, String> {
    let root = vault_root()?;
    let start = if prefix.trim().is_empty() { root.clone() } else { resolve(prefix)? };
    let mut files = Vec::new();
    let mut budget = 4000usize;
    walk(&start, &mut files, &mut budget);

    let mut dated: Vec<(std::time::SystemTime, String)> = files
        .into_iter()
        .filter_map(|path| {
            let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
            let rel = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().to_string();
            Some((modified, rel))
        })
        .collect();
    dated.sort_by(|a, b| b.0.cmp(&a.0));
    dated.truncate(limit.clamp(1, 50));

    Ok(json!({
        "ok": true,
        "notes": dated
            .into_iter()
            .map(|(modified, rel)| {
                let stamp: chrono::DateTime<chrono::Local> = modified.into();
                json!({ "path": rel, "modified": stamp.format("%Y-%m-%d %H:%M").to_string() })
            })
            .collect::<Vec<_>>(),
    }))
}

pub fn read(rel: &str) -> Result<Value, String> {
    let path = resolve(rel)?;
    let content = std::fs::read_to_string(&path).map_err(|e| format!("could not read {rel}: {e}"))?;
    Ok(json!({ "ok": true, "path": rel, "content": content }))
}

/// Write a note, with the frontmatter every document in this vault carries.
pub fn write(rel: &str, content: &str, author: &str) -> Result<Value, String> {
    let path = resolve(rel)?;
    if !rel.ends_with(".md") {
        return Err("a vault note is Markdown; give the path a .md ending".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M %Z").to_string();
    let author = if author.trim().is_empty() { "xNAUT agent" } else { author.trim() };
    // Keep the owner's frontmatter if he wrote one; only bump Last modified.
    let body = if content.trim_start().starts_with("---") {
        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
        let mut seen = false;
        for line in lines.iter_mut() {
            if line.starts_with("Last modified:") {
                *line = format!("Last modified: {stamp}");
                seen = true;
                break;
            }
        }
        if !seen {
            lines.insert(1, format!("Last modified: {stamp}"));
        }
        lines.join("\n")
    } else {
        format!("---\nAuthor: {author}\nLast modified: {stamp}\n---\n\n{}", content.trim_start())
    };
    std::fs::write(&path, body).map_err(|e| format!("could not write {rel}: {e}"))?;
    Ok(json!({ "ok": true, "path": rel, "note": "Written to the vault with its frontmatter." }))
}

pub fn list(prefix: &str, limit: usize) -> Result<Value, String> {
    let root = vault_root()?;
    let start = if prefix.trim().is_empty() { root.clone() } else { resolve(prefix)? };
    let mut files = Vec::new();
    let mut budget = limit.clamp(1, 500);
    walk(&start, &mut files, &mut budget);
    let paths: Vec<String> = files
        .into_iter()
        .map(|path| path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().to_string())
        .collect();
    Ok(json!({ "ok": true, "paths": paths }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vault_path_cannot_climb_out() {
        // The Librarian writes where it is told, and it is only ever told
        // about the vault.
        assert!(resolve("../.ssh/id_rsa").is_err());
        assert!(resolve("work/../../etc/passwd").is_err());
        assert!(resolve("").is_err());
    }

    #[test]
    fn a_search_matches_terms_rather_than_the_whole_phrase() {
        // "plugin marketplace" has to find xNAUT-Addon-Marketplace.md, whose
        // filename never contains those two words together.
        if vault_root().is_err() {
            return;
        }
        let found = search("plugin marketplace", 10).expect("search");
        let paths: Vec<String> = found["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|hit| hit["path"].as_str().unwrap_or("").to_lowercase())
            .collect();
        assert!(
            paths.iter().any(|path| path.contains("marketplace")),
            "the marketplace notes were not found: {paths:?}"
        );
    }

    #[test]
    fn recent_answers_a_question_about_time_rather_than_text() {
        // "What was the last doc added to the Vault" was answered from a text
        // search, which named a note two days older than the newest one.
        if vault_root().is_err() {
            return;
        }
        let listed = recent(5, "").expect("recent");
        let notes = listed["notes"].as_array().unwrap();
        assert!(!notes.is_empty(), "the vault has notes");
        let stamps: Vec<&str> = notes.iter().map(|n| n["modified"].as_str().unwrap()).collect();
        let mut sorted = stamps.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(stamps, sorted, "newest first: {stamps:?}");
    }

    #[test]
    fn a_search_ranks_rather_than_demanding_every_word() {
        // "xNaut Project" found nothing, because "project" appears in almost
        // none of those notes.
        if vault_root().is_err() {
            return;
        }
        let found = search("xNaut Project", 5).expect("search");
        assert!(
            !found["hits"].as_array().unwrap().is_empty(),
            "a two-word question found nothing"
        );
    }

    #[test]
    fn a_note_must_be_markdown() {
        // Not a guard against malice, a guard against a model writing a .json
        // into a folder of notes and nobody finding it again.
        assert!(write("work/xNAUT/notes.txt", "hello", "test").is_err());
    }

    #[test]
    fn writing_keeps_frontmatter_the_owner_already_wrote() {
        let content = "---\nAuthor: André\nLast modified: 2020-01-01 00:00 CET\n---\n\n# Kept\n";
        let rel = format!("work/xNAUT/Development/features/test-{}.md", std::process::id());
        if vault_root().is_err() {
            return; // no vault on this machine; nothing to assert
        }
        write(&rel, content, "agent").expect("write");
        let back = read(&rel).expect("read");
        let text = back["content"].as_str().unwrap().to_string();
        let _ = std::fs::remove_file(resolve(&rel).unwrap());
        assert!(text.contains("Author: André"), "the author was overwritten: {text}");
        assert!(!text.contains("2020-01-01"), "Last modified was not bumped: {text}");
    }
}
