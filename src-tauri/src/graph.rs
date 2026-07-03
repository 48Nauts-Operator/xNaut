// Vault knowledge-graph: walk a folder's .md files, parse [[wikilinks]] into a
// {nodes, links} graph the frontend renders as an Obsidian-style orb.
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub val: u32,      // node size = degree + 1
    pub group: String, // top-level folder under the root (color bucket)
    pub ts: u64,       // file mtime (unix secs) — drives the timeline build-up
}

fn group_of(root: &Path, f: &Path) -> String {
    match f.strip_prefix(root) {
        Ok(rel) => {
            let comps: Vec<_> = rel.components().collect();
            if comps.len() >= 2 {
                comps[0].as_os_str().to_string_lossy().into_owned()
            } else {
                "root".into()
            }
        }
        Err(_) => "root".into(),
    }
}

fn mtime_of(f: &Path) -> u64 {
    std::fs::metadata(f)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
#[derive(Serialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
}
#[derive(Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

const MAX_FILES: usize = 6000; // ponytail: cap the walk; raise if a vault is bigger

fn walk_md(dir: &Path, out: &mut Vec<PathBuf>) {
    if out.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue; // skip .obsidian, .git, .trash, ...
        }
        let p = e.path();
        if p.is_dir() {
            walk_md(&p, out);
        } else if p.extension().map(|x| x == "md").unwrap_or(false) {
            out.push(p);
        }
        if out.len() >= MAX_FILES {
            return;
        }
    }
}

/// Build a note-link graph from a vault/folder. Notes are matched by filename
/// stem (case-insensitive), the way Obsidian resolves `[[Note]]`.
#[tauri::command]
pub fn graph_scan(path: String) -> Result<GraphData, String> {
    let expanded = if let Some(rest) = path.strip_prefix("~/") {
        std::env::var("HOME")
            .map(|h| format!("{h}/{rest}"))
            .unwrap_or(path.clone())
    } else {
        path.clone()
    };
    let root = Path::new(&expanded);
    if !root.is_dir() {
        return Err(format!("not a directory: {path}"));
    }
    let mut files = Vec::new();
    walk_md(root, &mut files);

    // key (lowercased stem) -> display label / group / mtime. First writer wins.
    let mut label_by_key: HashMap<String, String> = HashMap::new();
    let mut group_by_key: HashMap<String, String> = HashMap::new();
    let mut ts_by_key: HashMap<String, u64> = HashMap::new();
    for f in &files {
        if let Some(stem) = f.file_stem().and_then(|s| s.to_str()) {
            let key = stem.to_lowercase();
            if !label_by_key.contains_key(&key) {
                label_by_key.insert(key.clone(), stem.to_string());
                group_by_key.insert(key.clone(), group_of(root, f));
                ts_by_key.insert(key, mtime_of(f));
            }
        }
    }

    // [[Target]], [[Target|alias]], [[folder/Target#heading]] -> "target"
    let re = regex::Regex::new(r"\[\[([^\]\|#]+)").unwrap();
    let mut links: Vec<GraphLink> = Vec::new();
    let mut degree: HashMap<String, u32> = HashMap::new();
    for f in &files {
        let Some(stem) = f.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let src = stem.to_lowercase();
        let Ok(body) = std::fs::read_to_string(f) else {
            continue;
        };
        for cap in re.captures_iter(&body) {
            let raw = cap[1].trim();
            let target = raw.rsplit('/').next().unwrap_or(raw).trim().to_lowercase();
            if target.is_empty() || target == src || !label_by_key.contains_key(&target) {
                continue;
            }
            links.push(GraphLink {
                source: label_by_key[&src].clone(),
                target: label_by_key[&target].clone(),
            });
            *degree.entry(src.clone()).or_insert(0) += 1;
            *degree.entry(target.clone()).or_insert(0) += 1;
        }
    }

    let nodes = label_by_key
        .iter()
        .map(|(k, label)| GraphNode {
            id: label.clone(),
            label: label.clone(),
            val: degree.get(k).copied().unwrap_or(0) + 1,
            group: group_by_key
                .get(k)
                .cloned()
                .unwrap_or_else(|| "root".into()),
            ts: ts_by_key.get(k).copied().unwrap_or(0),
        })
        .collect();

    Ok(GraphData { nodes, links })
}

// ── Code dependency graph ────────────────────────────────────────────────
const CODE_EXTS: &[&str] = &[
    "js", "jsx", "ts", "tsx", "mjs", "cjs", "vue", "svelte", "py", "go", "c", "h", "hpp", "cpp",
    "cc", "css", "scss", "rb", "php", "java",
];
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    "vendor",
    "__pycache__",
    "coverage",
];
// extensions tried when resolving an extensionless relative import
const RESOLVE_EXTS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "vue", "svelte", "py", "go", "c", "h", "hpp", "cpp",
    "cc", "css", "scss", "rb", "php",
];

fn walk_code(dir: &Path, out: &mut Vec<PathBuf>) {
    if out.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            walk_code(&p, out);
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .map(|x| CODE_EXTS.contains(&x))
            .unwrap_or(false)
        {
            out.push(p);
        }
        if out.len() >= MAX_FILES {
            return;
        }
    }
}

/// Fold "./" and "../" against a base dir into a normalized `a/b/c` rel path.
fn normalize_join(base: &str, spec: &str) -> String {
    let mut comps: Vec<&str> = if base.is_empty() {
        vec![]
    } else {
        base.split('/').collect()
    };
    for seg in spec.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                comps.pop();
            }
            s => comps.push(s),
        }
    }
    comps.join("/")
}

/// Build a file-import graph from a codebase. Nodes are source files (by path
/// relative to root); edges are RELATIVE imports (`./`, `../`, `#include "…"`)
/// that resolve to another file in the tree. Bare package imports are ignored.
#[tauri::command]
pub fn code_scan(path: String) -> Result<GraphData, String> {
    let expanded = if let Some(rest) = path.strip_prefix("~/") {
        std::env::var("HOME")
            .map(|h| format!("{h}/{rest}"))
            .unwrap_or(path.clone())
    } else {
        path.clone()
    };
    let root = Path::new(&expanded);
    if !root.is_dir() {
        return Err(format!("not a directory: {path}"));
    }
    let mut files = Vec::new();
    walk_code(root, &mut files);

    // rel path (forward-slash) -> file, and a set for resolution lookups
    let rel = |p: &Path| -> String {
        p.strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let rel_set: std::collections::HashSet<String> = files.iter().map(|f| rel(f)).collect();

    // Quoted specifier after import/from/require/@import, or a #include "…".
    let re_spec = regex::Regex::new(
        r#"(?:\bfrom\s*|\brequire\(\s*|\bimport\s*|@import\s+)['"]([^'"]+)['"]|#include\s*"([^"]+)""#,
    )
    .unwrap();

    let mut links: Vec<GraphLink> = Vec::new();
    let mut degree: HashMap<String, u32> = HashMap::new();
    for f in &files {
        let src = rel(f);
        let base = Path::new(&src)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let Ok(body) = std::fs::read_to_string(f) else {
            continue;
        };
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for cap in re_spec.captures_iter(&body) {
            let spec = cap
                .get(1)
                .or_else(|| cap.get(2))
                .map(|m| m.as_str())
                .unwrap_or("");
            if !(spec.starts_with('.') || spec.starts_with('/')) {
                continue; // bare package import — skip
            }
            let joined = normalize_join(&base, spec);
            // try as-is, then with extensions, then /index.<ext>
            let mut target = None;
            if rel_set.contains(&joined) {
                target = Some(joined.clone());
            }
            if target.is_none() {
                for e in RESOLVE_EXTS {
                    let c = format!("{joined}.{e}");
                    if rel_set.contains(&c) {
                        target = Some(c);
                        break;
                    }
                }
            }
            if target.is_none() {
                for e in RESOLVE_EXTS {
                    let c = format!("{joined}/index.{e}");
                    if rel_set.contains(&c) {
                        target = Some(c);
                        break;
                    }
                }
            }
            if let Some(tgt) = target {
                if tgt == src || !seen.insert(tgt.clone()) {
                    continue;
                }
                links.push(GraphLink {
                    source: src.clone(),
                    target: tgt.clone(),
                });
                *degree.entry(src.clone()).or_insert(0) += 1;
                *degree.entry(tgt).or_insert(0) += 1;
            }
        }
    }

    let nodes = files
        .iter()
        .map(|f| {
            let id = rel(f);
            let label = f
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| id.clone());
            GraphNode {
                val: degree.get(&id).copied().unwrap_or(0) + 1,
                group: group_of(root, f),
                ts: mtime_of(f),
                label,
                id,
            }
        })
        .collect();

    Ok(GraphData { nodes, links })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wikilinks_into_graph() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("xnaut-graph-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".obsidian")).unwrap();
        std::fs::write(dir.join(".obsidian/app.json"), "{}").unwrap(); // must be ignored
        std::fs::write(dir.join("Alpha.md"), "see [[Beta]] and [[Gamma|g]]").unwrap();
        std::fs::write(dir.join("Beta.md"), "back to [[Alpha]]").unwrap();
        std::fs::write(dir.join("Gamma.md"), "no links here").unwrap();

        let g = graph_scan(dir.to_string_lossy().into()).unwrap();
        assert_eq!(g.nodes.len(), 3, "3 notes -> 3 nodes");
        // Alpha->Beta, Alpha->Gamma, Beta->Alpha
        assert_eq!(g.links.len(), 3);
        let alpha = g.nodes.iter().find(|n| n.id == "Alpha").unwrap();
        assert_eq!(alpha.val, 4, "Alpha degree 3 + 1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_relative_imports_into_code_graph() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("xnaut-code-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/util")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap(); // must be skipped
        std::fs::write(dir.join("node_modules/pkg.js"), "x").unwrap();
        std::fs::write(
            dir.join("src/app.js"),
            "import {a} from './util/helper'; import React from 'react';",
        )
        .unwrap();
        std::fs::write(dir.join("src/util/helper.js"), "export const a=1;").unwrap();

        let g = code_scan(dir.to_string_lossy().into()).unwrap();
        assert_eq!(g.nodes.len(), 2, "2 source files (node_modules skipped)");
        assert_eq!(
            g.links.len(),
            1,
            "only the relative import resolves; 'react' skipped"
        );
        assert_eq!(g.links[0].source, "src/app.js");
        assert_eq!(g.links[0].target, "src/util/helper.js");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
