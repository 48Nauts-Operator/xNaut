//! Read-only project inspection for chat. A repository root must have been
//! named by the user, not invented by a model or supplied by a tool result.
use serde_json::{json, Value};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};

pub fn specs() -> Vec<Value> {
    [
        ("list_repository_files", "List files in a local repository directory named by the user. Use this to find documentation/source before reviewing; no worktree or shell required. Paginated, read-only.", false),
        ("read_repository_file", "Read a UTF-8 documentation or source file from a local repository named by the user. Use relative paths returned by list_repository_files. Read-only; excludes private files and cannot escape the repository.", true),
    ].into_iter().map(|(name, description, read)| json!({"type":"function","function":{
        "name":name,"description":description,"parameters":{"type":"object","properties":{
            "root":{"type":"string","description":"Absolute authorized repository root, or the exact registered project key/name named by the user."},
            "path":{"type":"string","description":if read {"Relative file path."} else {"Relative directory, defaults to ."}},
            "offset":{"type":"integer","minimum":0,"description":if read {"Zero-based line offset."} else {"Zero-based entry offset."}},
            "limit":{"type":"integer","minimum":1,"maximum":200}
        },"required":if read {vec!["root","path"]} else {vec!["root"]}}
    }})).collect()
}
pub fn is_tool(name: &str) -> bool {
    matches!(name, "list_repository_files" | "read_repository_file")
}

pub fn roots(messages: &[Value]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for message in messages.iter().filter(|m| m["role"] == "user") {
        let Some(text) = message["content"].as_str() else {
            continue;
        };
        // Both whitespace-delimited paths and quoted paths with spaces.
        let parts = text.split(['"', '\'', '`']).chain(text.split_whitespace());
        for part in parts {
            let path = Path::new(part.trim().trim_end_matches([',', ';', '.']));
            if !path.is_absolute() || !path.join(".git").exists() {
                continue;
            }
            if let Ok(path) = path.canonicalize() {
                if path.is_dir() && !roots.contains(&path) {
                    roots.push(path);
                }
            }
        }
    }
    roots
}
/// Resolve user-named projects from xNaut's authoritative local registry.
/// Assistant/tool text cannot grant access, and ambiguous names grant none.
pub fn registered_context(messages: &[Value]) -> (Vec<PathBuf>, Vec<Value>) {
    resolve_registered(messages, &registered_entries())
}

fn registered_entries() -> Vec<(String, String, String)> {
    let projects = crate::project_management::repo_now()
        .and_then(|repo| crate::project_management::list_projects(&repo))
        .unwrap_or_default();
    projects
        .iter()
        .map(|p| {
            (
                p.key.clone(),
                p.name.clone(),
                crate::project_management::local_source_path(p),
            )
        })
        .collect()
}

/// Interpret names through the registry, never relative to the app's working
/// directory. Resolving a name cannot grant access beyond the turn's roots.
pub(crate) fn authorized_root(root: &str, allowed: &[PathBuf]) -> Result<PathBuf, String> {
    resolve_authorized_root(root, allowed, &registered_entries())
}

fn resolve_authorized_root(
    root: &str,
    allowed: &[PathBuf],
    entries: &[(String, String, String)],
) -> Result<PathBuf, String> {
    let requested = Path::new(root);
    let path = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        let matches: Vec<_> = entries.iter().filter(|(key, name, _)| {
            key.eq_ignore_ascii_case(root) || name.eq_ignore_ascii_case(root)
        }).collect();
        match matches.as_slice() {
            [(_, _, path)] => PathBuf::from(path),
            [] => return Err("Unknown repository name. Use an exact registered project key or an absolute repository root supplied by the user.".into()),
            _ => return Err("Ambiguous repository name. Use the project's unique key or an authorized absolute root.".into()),
        }
    };
    if !path.is_absolute() {
        return Err("The registered project has no absolute local repository path. Set its local source folder in project settings.".into());
    }
    let canonical = path.canonicalize().map_err(|e| format!("Repository unavailable: {e}"))?;
    if !allowed.contains(&canonical) {
        return Err("Repository root was not supplied by the user or resolved from a registered project they named. Ask for the project name or path; do not guess or broaden it.".into());
    }
    Ok(canonical)
}
fn mentions(text: &str, name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    let text = text.to_lowercase();
    let name = name.to_lowercase();
    text.match_indices(&name).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + name.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}
fn resolve_registered(
    messages: &[Value],
    entries: &[(String, String, String)],
) -> (Vec<PathBuf>, Vec<Value>) {
    let mut allowed = Vec::new();
    let mut context = Vec::new();
    for (key, name, path) in entries {
        let named = messages
            .iter()
            .filter(|m| m["role"] == "user")
            .filter_map(|m| m["content"].as_str())
            .any(|text| {
                mentions(text, key)
                    || (mentions(text, name)
                        && entries
                            .iter()
                            .filter(|(_, other, _)| other.eq_ignore_ascii_case(name))
                            .count()
                            == 1)
            });
        if !named {
            continue;
        }
        let root = Path::new(path);
        let valid = root.is_absolute() && root.join(".git").exists();
        let canonical = valid
            .then(|| root.canonicalize().ok())
            .flatten()
            .filter(|p| p.is_dir());
        if let Some(root) = &canonical {
            if !allowed.contains(root) {
                allowed.push(root.clone());
            }
        }
        context.push(json!({"project":key,"name":name,"repository":canonical.as_ref().map(|p|p.to_string_lossy().to_string()).unwrap_or_else(||path.clone()),"available_locally":canonical.is_some()}));
    }
    (allowed, context)
}

fn excluded(path: &Path) -> bool {
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy().to_lowercase();
        matches!(
            name.as_str(),
            ".git"
                | ".ssh"
                | ".aws"
                | ".gnupg"
                | "node_modules"
                | "target"
                | ".worktrees"
                | "id_rsa"
                | "id_ed25519"
                | "credentials"
                | "credentials.json"
        ) || name.starts_with(".env")
            || name.ends_with(".pem")
            || name.ends_with(".key")
            || name.ends_with(".p12")
    })
}
fn inspect(name: &str, args: &Value, allowed: &[PathBuf]) -> Result<Value, String> {
    let root = authorized_root(args["root"].as_str().ok_or("root is required")?, allowed)?;
    let relative = Path::new(args["path"].as_str().unwrap_or("."));
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        || excluded(relative)
    {
        return Err("Use a relative project path; parent traversal, secret files and generated directories are excluded.".into());
    }
    let mut path = root.clone();
    for component in relative.components() {
        path.push(component);
        if std::fs::symlink_metadata(&path)
            .map_err(|e| format!("Cannot inspect path: {e}"))?
            .file_type()
            .is_symlink()
        {
            return Err("Symbolic links are not followed by repository chat tools.".into());
        }
    }
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&root) {
        return Err("Path escapes the supplied repository.".into());
    }
    let offset = args["offset"].as_u64().unwrap_or(0).min(usize::MAX as u64) as usize;
    let limit = args["limit"].as_u64().unwrap_or(100).clamp(1, 200) as usize;
    if name == "list_repository_files" {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&canonical)
            .map_err(|e| format!("Cannot list directory: {e}"))?
            .take(10001)
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let relative = entry
                .path()
                .strip_prefix(&root)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            if excluded(&relative) {
                continue;
            }
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            entries.push(json!({"path":relative.to_string_lossy(),"kind":if kind.is_symlink() {"symlink (not followed)"} else if kind.is_dir() {"directory"} else {"file"}}));
        }
        if entries.len() > 10000 {
            return Err("Directory too large; inspect a narrower subdirectory.".into());
        }
        entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
        let total = entries.len();
        Ok(
            json!({"ok":true,"root":root,"entries":entries.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"total":total,"next_offset": if offset.saturating_add(limit)<total {Some(offset+limit)} else {None}}),
        )
    } else {
        let metadata = std::fs::metadata(&canonical).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > 1024 * 1024 {
            return Err("Choose a regular text file no larger than 1 MiB.".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&canonical)
            .map_err(|e| format!("Cannot read file: {e}"))?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 || bytes.contains(&0) {
            return Err("File is too large or binary.".into());
        }
        let content = String::from_utf8(bytes).map_err(|_| "File is not UTF-8 text")?;
        let lines: Vec<_> = content.lines().collect();
        let mut page = Vec::new();
        let mut size = 0;
        for line in lines.iter().skip(offset).take(limit) {
            if size + line.len() > 64000 {
                if page.is_empty() {
                    return Err("A line exceeds the 64 KB response limit.".into());
                }
                break;
            }
            page.push(*line);
            size += line.len() + 1;
        }
        let next = offset.saturating_add(page.len());
        Ok(
            json!({"ok":true,"root":root,"path":relative,"content":page.join("\n"),"start_line":offset.saturating_add(1),"total_lines":lines.len(),"next_offset":if next<lines.len(){Some(next)}else{None}}),
        )
    }
}
pub fn execute(name: &str, args: &Value, allowed: &[PathBuf]) -> Value {
    match inspect(name, args, allowed) {
        Ok(value) => value,
        Err(error) => json!({"ok":false,"error":error}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "xnaut-history-repo-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn project_alias_resolves_only_to_an_authorized_unambiguous_root() {
        let tmp = Scratch::new();
        // The registered folder need not have the project's name (a worktree).
        let root = tmp.path().join("safety-net");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let canonical = root.canonicalize().unwrap();
        let entries = vec![("XNAUT".into(), "xnaut".into(), root.to_string_lossy().into_owned())];
        let allowed = vec![canonical.clone()];
        assert_eq!(resolve_authorized_root("xNAUT", &allowed, &entries).unwrap(), canonical);
        assert!(resolve_authorized_root("xNAUT", &[], &entries).unwrap_err().contains("not supplied"));
        assert!(resolve_authorized_root("safety-net", &allowed, &entries).unwrap_err().contains("Unknown"));
        assert!(resolve_authorized_root("../xnaut", &allowed, &entries).is_err());
        let mut ambiguous = entries.clone();
        ambiguous.push(("OTHER".into(), "xnaut".into(), root.to_string_lossy().into_owned()));
        assert!(resolve_authorized_root("xnaut", &allowed, &ambiguous).unwrap_err().contains("Ambiguous"));
        assert_eq!(resolve_authorized_root(root.to_str().unwrap(), &allowed, &[]).unwrap(), canonical);
    }

    #[test]
    fn registered_projects_resolve_only_explicit_unambiguous_user_mentions() {
        let tmp = Scratch::new();
        let root = tmp.path().join("JobUp");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("README.md"), "evidence").unwrap();
        let entries = vec![(
            "JOBUP".into(),
            "JobUp".into(),
            root.to_string_lossy().to_string(),
        )];
        let (allowed, context) = resolve_registered(
            &[json!({"role":"user","content":"Run a security check on JobUp"})],
            &entries,
        );
        assert_eq!(allowed, vec![root.canonicalize().unwrap()]);
        assert_eq!(context[0]["available_locally"], true);
        assert_eq!(
            execute(
                "read_repository_file",
                &json!({"root":root,"path":"README.md"}),
                &allowed
            )["content"],
            "evidence"
        );
        for role in ["assistant", "tool", "system"] {
            assert!(
                resolve_registered(&[json!({"role":role,"content":"JobUp"})], &entries)
                    .0
                    .is_empty()
            );
        }
        assert!(
            resolve_registered(&[json!({"role":"user","content":"JobUpper"})], &entries)
                .0
                .is_empty()
        );
        assert_eq!(
            resolve_registered(
                &[json!({"role":"user","content":"Check JOBUP-11"})],
                &entries
            )
            .0
            .len(),
            1
        );
        let ambiguous = vec![
            (
                "ONE".into(),
                "Shared".into(),
                root.to_string_lossy().to_string(),
            ),
            (
                "TWO".into(),
                "Shared".into(),
                root.to_string_lossy().to_string(),
            ),
        ];
        assert!(resolve_registered(
            &[json!({"role":"user","content":"Review Shared"})],
            &ambiguous
        )
        .0
        .is_empty());
        let missing = vec![(
            "MISSING".into(),
            "Missing".into(),
            tmp.path().join("absent").to_string_lossy().to_string(),
        )];
        let (allowed, context) = resolve_registered(
            &[json!({"role":"user","content":"Review Missing"})],
            &missing,
        );
        assert!(allowed.is_empty());
        assert_eq!(context[0]["available_locally"], false);
    }

    #[test]
    fn reads_named_repo_and_rejects_untrusted_roots_traversal_and_secrets() {
        let tmp = Scratch::new();
        let root = tmp.path().join("project with spaces");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/README.md"), "one\ntwo\nthree").unwrap();
        std::fs::write(root.join(".env"), "secret").unwrap();
        let user = json!({"role":"user","content":format!("Review \"{}\"",root.display())});
        let allowed = roots(&[user]);
        assert_eq!(allowed.len(), 1);
        assert!(roots(&[json!({"role":"tool","content":root.to_string_lossy()})]).is_empty());
        let args = json!({"root":root,"path":"docs/README.md","offset":1,"limit":1});
        assert_eq!(
            execute("read_repository_file", &args, &allowed)["content"],
            "two"
        );
        assert_eq!(
            execute("read_repository_file", &args, &allowed)["next_offset"],
            2
        );
        assert_eq!(execute("read_repository_file", &args, &[])["ok"], false);
        for path in ["../outside", ".env", "/etc/passwd", ".git/config"] {
            assert_eq!(
                execute(
                    "read_repository_file",
                    &json!({"root":root,"path":path}),
                    &allowed
                )["ok"],
                false
            );
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc", root.join("escape")).unwrap();
            assert_eq!(
                execute(
                    "read_repository_file",
                    &json!({"root":root,"path":"escape/passwd"}),
                    &allowed
                )["ok"],
                false
            );
        }
        assert_eq!(
            execute("list_repository_files", &json!({"root":root}), &allowed)["entries"][0]["path"],
            "docs"
        );
    }
}
