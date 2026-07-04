// Vault: Obsidian-style markdown vault - parsing, index, CRUD, backlinks,
// tags, search, rename-with-rewrite, watcher, MinIO sync. One brain: UI
// panels and the librarian agent's tools are all thin clients of this module.

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct NoteMeta {
    pub rel: String,
    pub title: String,
    pub stem_key: String,
    pub tags: Vec<String>,
    pub links_out: Vec<String>,
    pub mtime: u64,
    pub size: u64,
}

pub struct VaultIndex {
    pub root: PathBuf,
    pub notes: HashMap<String, NoteMeta>,
    pub by_stem: HashMap<String, String>,
    pub backlinks: HashMap<String, Vec<String>>,
    pub tags: HashMap<String, Vec<String>>,
}

#[derive(Default)]
pub struct VaultManager {
    pub indexes: std::sync::Mutex<HashMap<String, VaultIndex>>,
    #[allow(dead_code)]
    pub watchers: std::sync::Mutex<HashMap<String, notify::RecommendedWatcher>>,
}

const MAX_FILES: usize = 20_000; // ponytail: walk cap, personal vaults are thousands not millions

fn skip_dir(name: &str) -> bool {
    name.starts_with('.')
}

fn walk(dir: &Path, exts_md_only: bool, files: &mut Vec<PathBuf>, dirs: &mut Vec<PathBuf>) {
    if files.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let p = e.path();
        if p.is_dir() {
            if skip_dir(&name) {
                continue;
            }
            dirs.push(p.clone());
            walk(&p, exts_md_only, files, dirs);
        } else if !exts_md_only || p.extension().map(|x| x == "md").unwrap_or(false) {
            files.push(p);
        }
    }
}

impl VaultIndex {
    pub fn build(root: PathBuf) -> Self {
        let mut idx = Self {
            root,
            notes: HashMap::new(),
            by_stem: HashMap::new(),
            backlinks: HashMap::new(),
            tags: HashMap::new(),
        };
        let (mut files, mut dirs) = (Vec::new(), Vec::new());
        walk(&idx.root.clone(), true, &mut files, &mut dirs);
        for f in &files {
            idx.index_one(f);
        }
        idx.rebuild_derived();
        idx
    }

    fn rel_of(&self, abs: &Path) -> Option<String> {
        abs.strip_prefix(&self.root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    }

    fn index_one(&mut self, abs: &Path) {
        let Some(rel) = self.rel_of(abs) else {
            return;
        };
        let stem = abs
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(body) = std::fs::read_to_string(abs) else {
            return;
        };
        let (title, tags, links_out) = parse_note(&body, &stem);
        let md = std::fs::metadata(abs).ok();
        self.notes.insert(
            rel.clone(),
            NoteMeta {
                rel,
                title,
                stem_key: stem.to_lowercase(),
                tags,
                links_out,
                mtime: md
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                size: md.map(|m| m.len()).unwrap_or(0),
            },
        );
    }

    /// Recompute by_stem / backlinks / tags from `notes`. O(notes) - cheap
    /// enough to run after every mutation. ponytail: full derived rebuild
    /// instead of surgical updates; revisit if vaults exceed ~20k notes.
    pub fn rebuild_derived(&mut self) {
        self.by_stem.clear();
        self.backlinks.clear();
        self.tags.clear();
        let mut rels: Vec<String> = self.notes.keys().cloned().collect();
        rels.sort();
        for rel in &rels {
            let key = self.notes[rel].stem_key.clone();
            self.by_stem.entry(key).or_insert_with(|| rel.clone());
        }
        for rel in &rels {
            let meta = self.notes[rel].clone();
            for l in &meta.links_out {
                if let Some(target) = self.by_stem.get(l) {
                    if target != rel {
                        self.backlinks
                            .entry(target.clone())
                            .or_default()
                            .push(rel.clone());
                    }
                }
            }
            for t in &meta.tags {
                self.tags.entry(t.clone()).or_default().push(rel.clone());
            }
        }
    }

    /// Index the file if it exists, drop it if it doesn't, refresh derived maps.
    pub fn reindex_path(&mut self, abs: &Path) {
        if abs.exists() {
            self.index_one(abs);
        } else if let Some(rel) = self.rel_of(abs) {
            self.notes.remove(&rel);
        }
        self.rebuild_derived();
    }

    /// All directories (incl. empty project folders), vault-relative.
    pub fn dirs(&self) -> Vec<String> {
        let (mut files, mut dirs) = (Vec::new(), Vec::new());
        walk(&self.root.clone(), true, &mut files, &mut dirs);
        dirs.iter()
            .filter_map(|d| {
                d.strip_prefix(&self.root)
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
            })
            .collect()
    }
}

pub fn vault_root(vault: &str) -> Result<PathBuf, String> {
    if vault != "work" && vault != "personal" {
        return Err(format!("unknown vault: {vault}"));
    }
    Ok(dirs::home_dir()
        .ok_or("no home dir")?
        .join(".xnaut-vault")
        .join(vault))
}

/// Join a client-supplied rel path under root, refusing traversal. Every
/// command that takes a rel path MUST go through this.
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|c| c == "..") {
        return Err(format!("invalid path: {rel}"));
    }
    Ok(root.join(rel))
}

#[tauri::command]
pub fn vault_init() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("no home dir")?;
    let root = home.join(".xnaut-vault");
    for v in ["work", "personal"] {
        std::fs::create_dir_all(root.join(v).join("_inbox")).map_err(|e| e.to_string())?;
    }
    if let Ok(tasks) = crate::tasks::tasks_list() {
        for t in tasks.iter().filter(|t| t.kind == "project") {
            let safe = t.name.replace('/', "-");
            let _ = std::fs::create_dir_all(root.join("work").join(&safe));
        }
    }
    Ok(root.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn vault_open(
    state: State<'_, VaultManager>,
    vault: String,
) -> Result<serde_json::Value, String> {
    let root = vault_root(&vault)?;
    let idx = VaultIndex::build(root);
    let (nc, tc) = (idx.notes.len(), idx.tags.len());
    state.indexes.lock().unwrap().insert(vault, idx);
    Ok(serde_json::json!({ "note_count": nc, "tag_count": tc }))
}

#[tauri::command]
pub fn vault_tree(
    state: State<'_, VaultManager>,
    vault: String,
) -> Result<serde_json::Value, String> {
    let map = state.indexes.lock().unwrap();
    let idx = map.get(&vault).ok_or("vault not open")?;
    let mut notes: Vec<&NoteMeta> = idx.notes.values().collect();
    notes.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(serde_json::json!({ "dirs": idx.dirs(), "notes": notes }))
}

pub fn crud_write(idx: &mut VaultIndex, rel: &str, content: &str) -> Result<(), String> {
    let abs = safe_join(&idx.root, rel)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&abs, content).map_err(|e| e.to_string())?;
    idx.reindex_path(&abs);
    Ok(())
}

pub fn crud_move(idx: &mut VaultIndex, from_rel: &str, to_rel: &str) -> Result<(), String> {
    let from = safe_join(&idx.root, from_rel)?;
    let to = safe_join(&idx.root, to_rel)?;
    if to.exists() {
        return Err(format!("target exists: {to_rel}"));
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&from, &to).map_err(|e| e.to_string())?;
    idx.reindex_path(&from);
    idx.reindex_path(&to);
    Ok(())
}

pub fn crud_trash(idx: &mut VaultIndex, rel: &str) -> Result<(), String> {
    let abs = safe_join(&idx.root, rel)?;
    let trash = idx.root.join(".trash");
    std::fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let flat = rel.replace('/', "__");
    std::fs::rename(&abs, trash.join(format!("{epoch}-{flat}"))).map_err(|e| e.to_string())?;
    idx.reindex_path(&abs);
    Ok(())
}

#[tauri::command]
pub fn vault_note_read(
    state: State<'_, VaultManager>,
    vault: String,
    rel: String,
) -> Result<String, String> {
    let map = state.indexes.lock().unwrap();
    let idx = map.get(&vault).ok_or("vault not open")?;
    let abs = safe_join(&idx.root, &rel)?;
    std::fs::read_to_string(abs).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vault_note_write(
    state: State<'_, VaultManager>,
    vault: String,
    rel: String,
    content: String,
) -> Result<(), String> {
    let mut map = state.indexes.lock().unwrap();
    let idx = map.get_mut(&vault).ok_or("vault not open")?;
    crud_write(idx, &rel, &content)
}

#[tauri::command]
pub fn vault_note_create(
    state: State<'_, VaultManager>,
    vault: String,
    rel: String,
    content: Option<String>,
) -> Result<(), String> {
    let mut map = state.indexes.lock().unwrap();
    let idx = map.get_mut(&vault).ok_or("vault not open")?;
    let abs = safe_join(&idx.root, &rel)?;
    if abs.exists() {
        return Err(format!("note exists: {rel}"));
    }
    let stem = abs
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    crud_write(idx, &rel, &content.unwrap_or(format!("# {stem}\n")))
}

#[tauri::command]
pub fn vault_note_move(
    state: State<'_, VaultManager>,
    vault: String,
    from_rel: String,
    to_rel: String,
) -> Result<(), String> {
    let mut map = state.indexes.lock().unwrap();
    let idx = map.get_mut(&vault).ok_or("vault not open")?;
    crud_move(idx, &from_rel, &to_rel)
}

#[tauri::command]
pub fn vault_note_delete(
    state: State<'_, VaultManager>,
    vault: String,
    rel: String,
) -> Result<(), String> {
    let mut map = state.indexes.lock().unwrap();
    let idx = map.get_mut(&vault).ok_or("vault not open")?;
    crud_trash(idx, &rel)
}

fn push_tag(tags: &mut Vec<String>, raw: &str) {
    let t = raw
        .trim()
        .trim_matches('"')
        .trim_start_matches('#')
        .to_lowercase();
    if !t.is_empty() && !tags.contains(&t) {
        tags.push(t);
    }
}

/// Parse one note body into (title, tags, outgoing link stem-keys).
/// Frontmatter: leading `---\n...\n---` block; only `tags:` is interpreted
/// (inline `[a, b]` or dash list). Inline `#tags` and `[[wikilinks]]` come
/// from the body. ponytail: tag/link scan is code-fence-blind - a #tag inside
/// a code block gets indexed; harmless for personal notes.
pub fn parse_note(body: &str, stem: &str) -> (String, Vec<String>, Vec<String>) {
    let mut tags: Vec<String> = Vec::new();
    let mut rest = body;
    if let Some(stripped) = body.strip_prefix("---\n") {
        if let Some(end) = stripped.find("\n---") {
            let fm = &stripped[..end];
            rest = stripped[end + 4..].trim_start_matches('\n');
            let mut in_tags = false;
            for line in fm.lines() {
                let t = line.trim();
                if let Some(v) = t.strip_prefix("tags:") {
                    let v = v.trim();
                    if v.starts_with('[') {
                        for item in v.trim_start_matches('[').trim_end_matches(']').split(',') {
                            push_tag(&mut tags, item);
                        }
                        in_tags = false;
                    } else if v.is_empty() {
                        in_tags = true;
                    } else {
                        push_tag(&mut tags, v);
                        in_tags = false;
                    }
                } else if in_tags {
                    if let Some(item) = t.strip_prefix("- ") {
                        push_tag(&mut tags, item);
                    } else {
                        in_tags = false;
                    }
                }
            }
        }
    }
    let tag_re = regex::Regex::new(r"(?m)(^|\s)#([A-Za-z][A-Za-z0-9_/-]*)").unwrap();
    for cap in tag_re.captures_iter(rest) {
        push_tag(&mut tags, &cap[2]);
    }
    let link_re = regex::Regex::new(r"\[\[([^\]\|#]+)").unwrap();
    let mut links = Vec::new();
    for cap in link_re.captures_iter(rest) {
        let raw = cap[1].trim();
        let k = raw.rsplit('/').next().unwrap_or(raw).trim().to_lowercase();
        if !k.is_empty() && !links.contains(&k) {
            links.push(k);
        }
    }
    let title = rest
        .lines()
        .find_map(|l| l.strip_prefix("# ").map(|s| s.trim().to_string()))
        .unwrap_or_else(|| stem.to_string());
    (title, tags, links)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_vault(name: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("xnaut-vault-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_frontmatter_inline_tags_links_title() {
        let body = "---\ntags: [Idea, naut-gate]\ncreated: 2026-07-04\n---\n# My Title\ntext #inline and [[Other Note|alias]] plus [[folder/Deep#h]] and #inline again\n";
        let (title, tags, links) = parse_note(body, "my-note");
        assert_eq!(title, "My Title");
        assert_eq!(tags, vec!["idea", "naut-gate", "inline"]);
        assert_eq!(links, vec!["other note", "deep"]);
    }

    #[test]
    fn parses_dash_list_tags_and_falls_back_to_stem_title() {
        let body = "---\ntags:\n  - alpha\n  - beta\n---\nno heading here\n";
        let (title, tags, links) = parse_note(body, "some-note");
        assert_eq!(title, "some-note");
        assert_eq!(tags, vec!["alpha", "beta"]);
        assert!(links.is_empty());
    }

    #[test]
    fn no_frontmatter_is_fine() {
        let (title, tags, links) = parse_note("# T\nplain", "x");
        assert_eq!(title, "T");
        assert!(tags.is_empty() && links.is_empty());
    }

    #[test]
    fn builds_index_with_backlinks_and_tags() {
        let dir = tmp_vault("idx");
        std::fs::create_dir_all(dir.join("proj")).unwrap();
        std::fs::create_dir_all(dir.join(".trash")).unwrap();
        std::fs::write(dir.join(".trash/gone.md"), "[[Alpha]]").unwrap();
        std::fs::write(dir.join("Alpha.md"), "# Alpha\nsee [[Beta]] #core\n").unwrap();
        std::fs::write(dir.join("proj/Beta.md"), "back [[Alpha]] #core #proj\n").unwrap();
        let idx = VaultIndex::build(dir.clone());
        assert_eq!(idx.notes.len(), 2);
        assert_eq!(idx.by_stem["beta"], "proj/Beta.md");
        assert_eq!(idx.backlinks["Alpha.md"], vec!["proj/Beta.md"]);
        assert_eq!(idx.backlinks["proj/Beta.md"], vec!["Alpha.md"]);
        assert_eq!(idx.tags["core"].len(), 2);
        assert!(idx.dirs().contains(&"proj".to_string()));
        std::fs::remove_file(dir.join("proj/Beta.md")).unwrap();
        let abs = dir.join("proj/Beta.md");
        let mut idx = idx;
        idx.reindex_path(&abs);
        assert_eq!(idx.notes.len(), 1);
        assert!(idx.backlinks.get("Alpha.md").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let root = std::path::Path::new("/tmp/v");
        assert!(safe_join(root, "a/b.md").is_ok());
        assert!(safe_join(root, "../evil.md").is_err());
        assert!(safe_join(root, "a/../../evil.md").is_err());
        assert!(safe_join(root, "/abs.md").is_err());
        assert!(safe_join(root, "").is_err());
        assert!(vault_root("nope").is_err());
    }

    #[test]
    fn crud_roundtrip_with_trash() {
        let dir = tmp_vault("crud");
        let mut idx = VaultIndex::build(dir.clone());
        crud_write(&mut idx, "proj/new.md", "# New\n[[other]]").unwrap();
        assert!(dir.join("proj/new.md").exists());
        assert!(idx.notes.contains_key("proj/new.md"));
        crud_move(&mut idx, "proj/new.md", "proj/renamed.md").unwrap();
        assert!(!dir.join("proj/new.md").exists() && dir.join("proj/renamed.md").exists());
        crud_trash(&mut idx, "proj/renamed.md").unwrap();
        assert!(!idx.notes.contains_key("proj/renamed.md"));
        let trash: Vec<_> = std::fs::read_dir(dir.join(".trash")).unwrap().collect();
        assert_eq!(trash.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
