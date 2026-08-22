// Shared agent notes — so parallel agents can read each other's findings.
//
// Idea from CORAL (Apache 2.0): `.coral/public/` is symlinked into every agent
// worktree, so agents see each other's work while they work. Today NautFlow's
// worktree agents are blind to each other, and three of them can independently
// discover the same broken assumption and each pay for it.
//
// One deliberate departure from the original. CORAL's notes are scoped to a RUN
// and die with it, which is right for a research sweep that exists to produce
// one score. Ours are scoped to a PROJECT and are permanent, because the second
// consumer of this directory is a human reading it weeks later (XNAUT-91: the
// right pane as a durable note stream). Putting them in the vault gets that,
// plus Obsidian visibility on the phone and a git history, for free:
//
//     ~/.xnaut-vault/work/<project>/Development/notes/
//
// and each worktree gets `.nf-shared/notes` pointing at it.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Where a project's notes live. One directory per project, permanent.
pub fn notes_dir(project: &str) -> Result<PathBuf, String> {
    let slug = slugify(project);
    if slug.is_empty() {
        return Err("a project name is required".into());
    }
    Ok(vault_work_root()?
        .join(slug)
        .join("Development")
        .join("notes"))
}

/// The work vault, honouring XNAUT_VAULT_ROOT.
///
/// The override exists because the tests below write real note trees, and
/// without it they wrote them into the owner's actual vault: 487 empty
/// `xnaut-notes-test-*` folders accumulated there, one per test run, because
/// the cleanup line only ran on the happy path and a panic skipped it. A test
/// that pollutes the thing it is testing is a test people stop running.
pub fn vault_work_root() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("XNAUT_VAULT_ROOT") {
        return Ok(PathBuf::from(root).join("work"));
    }
    let home = dirs::home_dir().ok_or("no home directory")?;
    Ok(home.join(".xnaut-vault").join("work"))
}

/// Path-safe, and stable across calls so the same project always resolves to the
/// same directory. Spaces become dashes; anything else outside [A-Za-z0-9._-] is
/// dropped rather than escaped, since these become real directory names.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    // Never let a slug escape the vault.
    out.trim_matches(['.', '-']).to_string()
}

/// Creates the project's notes directory and links it into a worktree.
///
/// Returns the notes directory. Safe to call repeatedly: an existing correct
/// link is left alone, and a stale one (from a moved vault) is replaced rather
/// than silently pointing somewhere wrong.
#[tauri::command]
pub fn shared_notes_link(project: String, worktree_path: String) -> Result<String, String> {
    let target = notes_dir(&project)?;
    std::fs::create_dir_all(&target).map_err(|e| format!("creating {}: {e}", target.display()))?;

    let wt = PathBuf::from(&worktree_path);
    if !wt.is_dir() {
        return Err(format!("{} is not a directory", wt.display()));
    }
    let shared = wt.join(".nf-shared");
    std::fs::create_dir_all(&shared).map_err(|e| format!("creating {}: {e}", shared.display()))?;
    let link = shared.join("notes");

    match std::fs::read_link(&link) {
        Ok(existing) if existing == target => {}
        Ok(_) => {
            // Points somewhere else — replace it. A stale link is worse than no
            // link: the agent writes notes that nobody reads.
            let _ = std::fs::remove_file(&link);
            symlink(&target, &link)?;
        }
        Err(_) => {
            if link.exists() {
                // A real directory where the link should be: keep whatever is in
                // it by moving the files across, then link.
                if link.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&link) {
                        for entry in entries.flatten() {
                            let dest = target.join(entry.file_name());
                            let _ = std::fs::rename(entry.path(), dest);
                        }
                    }
                    let _ = std::fs::remove_dir_all(&link);
                } else {
                    let _ = std::fs::remove_file(&link);
                }
            }
            symlink(&target, &link)?;
        }
    }

    // Keep it out of the agent's commits without touching the repo's tracked
    // .gitignore — info/exclude is local to this worktree.
    exclude_locally(&wt, ".nf-shared/");
    Ok(target.to_string_lossy().to_string())
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link)
        .map_err(|e| format!("linking {} -> {}: {e}", link.display(), target.display()))
}

#[cfg(not(unix))]
fn symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_dir(target, link)
        .map_err(|e| format!("linking {} -> {}: {e}", link.display(), target.display()))
}

/// Appends a pattern to a worktree's local excludes, once.
fn exclude_locally(worktree: &Path, pattern: &str) {
    // A linked worktree's .git is a FILE pointing at the real gitdir, so the
    // exclude file is not always <worktree>/.git/info/exclude.
    let gitdir = match std::fs::read_to_string(worktree.join(".git")) {
        Ok(body) => body
            .lines()
            .find_map(|l| l.strip_prefix("gitdir:"))
            .map(|p| PathBuf::from(p.trim()))
            .unwrap_or_else(|| worktree.join(".git")),
        Err(_) => worktree.join(".git"),
    };
    let info = gitdir.join("info");
    if std::fs::create_dir_all(&info).is_err() {
        return;
    }
    let exclude = info.join("exclude");
    let current = std::fs::read_to_string(&exclude).unwrap_or_default();
    if current.lines().any(|l| l.trim() == pattern) {
        return;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude)
    {
        let _ = writeln!(f, "{pattern}");
    }
}

/// One note, as the pane will want it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Note {
    pub slug: String,
    pub title: String,
    pub agent: String,
    pub created: String,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub body: String,
}

/// Splits YAML frontmatter from the body. Deliberately lenient: these files are
/// written by models, and a note with a slightly malformed header is still worth
/// showing. Anything unparseable is treated as a bodyless-header note rather
/// than being dropped.
fn parse_note(slug: &str, text: &str) -> Note {
    let mut title = String::new();
    let mut agent = String::new();
    let mut created = String::new();
    let mut tags = Vec::new();
    let mut links = Vec::new();
    let mut body = text.trim().to_string();

    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let front = &rest[..end];
            body = rest[end + 4..].trim().to_string();
            for line in front.lines() {
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let key = key.trim().to_lowercase();
                let value = value.trim().trim_matches('"').trim_matches('\'');
                match key.as_str() {
                    "title" => title = value.to_string(),
                    "agent" => agent = value.to_string(),
                    "created" => created = value.to_string(),
                    "tags" => tags = parse_list(value),
                    "links" => links = parse_list(value),
                    _ => {}
                }
            }
        }
    }
    if title.is_empty() {
        // Fall back to the first heading or the first line, so a note without a
        // title is still identifiable in a list.
        title = body
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim_start_matches('#').trim().to_string())
            .unwrap_or_else(|| slug.to_string());
    }
    Note {
        slug: slug.to_string(),
        title,
        agent,
        created,
        tags,
        links,
        body,
    }
}

/// `[a, b]` or `a, b` — both appear in practice.
fn parse_list(value: &str) -> Vec<String> {
    value
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A project's notes, newest first. Empty when the project has none — an absent
/// directory is "nothing written yet", not an error.
#[tauri::command]
pub fn shared_notes_list(project: String, limit: Option<usize>) -> Result<Vec<Note>, String> {
    let dir = notes_dir(&project)?;
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut notes: Vec<(std::time::SystemTime, Note)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let slug = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        notes.push((mtime, parse_note(&slug, &text)));
    }
    // Sort by the frontmatter timestamp where there is one, mtime otherwise —
    // a note edited later should not jump ahead of one written later.
    notes.sort_by(|a, b| {
        let ka = if a.1.created.is_empty() {
            None
        } else {
            Some(&a.1.created)
        };
        let kb = if b.1.created.is_empty() {
            None
        } else {
            Some(&b.1.created)
        };
        match (ka, kb) {
            (Some(x), Some(y)) => y.cmp(x),
            _ => b.0.cmp(&a.0),
        }
    });
    let mut out: Vec<Note> = notes.into_iter().map(|(_, n)| n).collect();
    if let Some(n) = limit {
        out.truncate(n);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_cannot_escape_the_vault() {
        assert_eq!(slugify("../../etc"), "etc");
        assert_eq!(slugify("My Project"), "My-Project");
        assert_eq!(slugify("xnaut"), "xnaut");
        assert!(slugify("///").is_empty());
    }

    #[test]
    fn parses_the_memory_shaped_frontmatter() {
        let text = "---\ntitle: Redis was the wrong call\nagent: dev-2\n\
                    created: 2026-08-08T01:12:00Z\ntags: [decision, dead-end]\n\
                    links: [\"https://example.com/x\"]\n---\n\nBody here. See [[other-note]].\n";
        let n = parse_note("redis", text);
        assert_eq!(n.title, "Redis was the wrong call");
        assert_eq!(n.agent, "dev-2");
        assert_eq!(n.tags, vec!["decision", "dead-end"]);
        assert_eq!(n.links, vec!["https://example.com/x"]);
        assert!(n.body.starts_with("Body here."));
    }

    #[test]
    fn a_note_without_frontmatter_is_kept_not_dropped() {
        // Written by a model, so it will happen. A malformed header must not
        // cost the note.
        let n = parse_note("stray", "# Found the race\n\nIt is in the poller.\n");
        assert_eq!(n.title, "Found the race");
        assert!(n.body.contains("poller"));
    }

    #[test]
    fn listing_an_unwritten_project_is_empty_not_an_error() {
        let out = shared_notes_list("no-such-project-xnaut-test".into(), None).unwrap();
        assert!(out.is_empty());
    }

    /// Everything this test writes lives under one temp root, including the
    /// vault. Dropping it removes the lot, panic or not — the previous version
    /// cleaned up on its last line, so a failed assertion left the project
    /// folder behind in the owner's real vault, forever, once per run.
    struct TempVault(PathBuf);
    impl Drop for TempVault {
        fn drop(&mut self) {
            std::env::remove_var("XNAUT_VAULT_ROOT");
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A test must never write into the owner's real vault.
    ///
    /// This one did, and the damage was invisible per-run: one empty
    /// `xnaut-notes-test-<pid>` folder per execution, cleaned up only if every
    /// assertion passed. 487 of them accumulated in the work vault before
    /// anyone looked. The guard is that notes_dir is reachable from a temp
    /// root, so the fix cannot be undone by someone reinstating the home path.
    #[test]
    fn the_notes_dir_can_be_pointed_away_from_the_real_vault() {
        let tmp = std::env::temp_dir().join(format!("xnaut-vault-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("XNAUT_VAULT_ROOT", &tmp);
        let dir = notes_dir("some-project").unwrap();
        std::env::remove_var("XNAUT_VAULT_ROOT");
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(
            dir.starts_with(&tmp),
            "notes_dir ignored XNAUT_VAULT_ROOT and resolved to {} — tests will pollute the real vault",
            dir.display()
        );
        let home = dirs::home_dir().unwrap().join(".xnaut-vault");
        assert!(!dir.starts_with(home), "notes_dir still resolves into the home vault");
    }

    #[test]
    fn linking_creates_the_dir_the_link_and_the_exclude() {
        let tmp = std::env::temp_dir().join(format!("xnaut-notes-it-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(".git/info")).unwrap();
        // The vault goes inside the same temp root the guard owns.
        std::env::set_var("XNAUT_VAULT_ROOT", tmp.join("vault"));
        let _guard = TempVault(tmp.clone());
        let project = format!("xnaut-notes-test-{}", std::process::id());

        let target = shared_notes_link(project.clone(), tmp.to_string_lossy().into()).unwrap();
        let link = tmp.join(".nf-shared/notes");
        assert!(link.exists(), "link not created");
        assert_eq!(std::fs::read_link(&link).unwrap(), PathBuf::from(&target));

        // Idempotent: a second call must not fail or duplicate the exclude line.
        shared_notes_link(project.clone(), tmp.to_string_lossy().into()).unwrap();
        let excl = std::fs::read_to_string(tmp.join(".git/info/exclude")).unwrap();
        assert_eq!(excl.matches(".nf-shared/").count(), 1, "exclude duplicated");

        // A note written through the link is readable through the project.
        std::fs::write(
            link.join("one.md"),
            "---\ntitle: T\nagent: a\n---\nbody\n",
        )
        .unwrap();
        let notes = shared_notes_list(project.clone(), None).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "T");

        // No manual cleanup: TempVault owns it and runs on unwind too.
    }
}
