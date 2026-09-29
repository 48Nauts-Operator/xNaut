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
            "root":{"type":"string","description":"Exact absolute repository root supplied by the user (quote paths containing spaces)."},
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
    let root = Path::new(args["root"].as_str().ok_or("root is required")?)
        .canonicalize()
        .map_err(|e| format!("Repository unavailable: {e}"))?;
    if !allowed.contains(&root) {
        return Err("Repository root was not supplied by the user in this conversation. Ask for the project path; do not guess or broaden it.".into());
    }
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
