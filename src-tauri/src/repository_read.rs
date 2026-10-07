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

/// Only user-authored text grants scope, including text blocks from multimodal history.
pub(crate) fn user_texts(messages: &[Value]) -> Vec<String> {
    messages
        .iter()
        .filter(|m| m["role"] == "user")
        .map(|m| {
            if let Some(text) = m["content"].as_str() {
                return text.to_owned();
            }
            m["content"]
                .as_array()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter(|b| matches!(b["type"].as_str(), Some("text" | "input_text")))
                        .filter_map(|b| b["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default()
        })
        .collect()
}

pub fn roots(messages: &[Value]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for text in user_texts(messages) {
        for path in declared_paths(&text) {
            if !roots.contains(&path) { roots.push(path); }
        }
    }
    roots
}

/// Authorization is the owner's exact location, independent of whether a
/// checkout has been created there yet. Never infer an ancestor repository.
fn scoped_path(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    if path.exists() { return path.is_dir().then(|| path.canonicalize().ok()).flatten(); }
    Some(path.components().filter(|c| !matches!(c, Component::CurDir)).collect())
}

fn reference_tokens(text: &str) -> Vec<&str> {
    // Quoted references may contain spaces. Unquoted references end at
    // whitespace, so an entire sentence starting with a path is never a root.
    static REFERENCES: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = REFERENCES.get_or_init(|| regex::Regex::new(
        r#"`[^`]*`|"[^"]*"|'[^']*'|[^\s]+"#).expect("reference token pattern"));
    pattern.find_iter(text).map(|m| m.as_str().trim_matches(['"', '\'', '`'])
        .trim_end_matches([',', ';', '.', ')', ']'])).collect()
}

fn declared_paths(text: &str) -> Vec<PathBuf> {
    reference_tokens(text).into_iter().filter_map(|token| scoped_path(Path::new(token))).collect()
}

fn project_name_text(text: &str) -> String {
    reference_tokens(text).into_iter().filter(|token| {
        !Path::new(token).is_absolute() && !token.contains("://") && !token.starts_with("git@")
    }).collect::<Vec<_>>().join(" ")
}

pub(crate) fn checkout_diagnostic(root: &Path) -> Value {
    let state = match std::fs::metadata(root) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "missing_directory",
        Err(_) => "unavailable_directory",
        Ok(meta) if !meta.is_dir() => "not_directory",
        Ok(_) if root.join(".git").exists() => "git_checkout",
        Ok(_) => match std::fs::read_dir(root) {
            Ok(mut entries) => if entries.next().is_none() { "empty_directory" } else { "non_git_directory" },
            Err(_) => "unavailable_directory",
        },
    };
    json!({"repository":root,"authorized":true,"checkout_state":state,
        "available_locally":state=="git_checkout",
        "next_action":if state=="git_checkout" {"Inspect the exact checkout and existing project work before preparing a task."}
        else {"The owner already supplied this exact path. Establish or restore its intended Git checkout, preserving any supplied remote, then retry this same root. This chat has no repository clone/bootstrap tool. Do not substitute a parent/company project, initialize unrelated history, or launch product work while prerequisites remain blocked."}})
}
/// The UI bounds recent model history, but keeps the owner's repository
/// references from the whole thread. Resolve them locally; never replay old
/// instructions or accept assistant/tool messages as new repository scope.
pub(crate) fn conversation_context(
    messages: &[Value],
    earlier_user_texts: &[String],
) -> (Vec<PathBuf>, Vec<Value>) {
    let mut scope = messages.to_vec();
    scope.extend(earlier_user_texts.iter().map(|text| json!({"role":"user","content":text})));
    let reviews = crate::repository_review::chat_references(&scope);
    conversation_context_in(&scope, &registered_entries(), &reviews)
}

fn conversation_context_in(
    scope: &[Value],
    entries: &[(String, String, String)],
    reviews: &[crate::repository_transfer::Transfer],
) -> (Vec<PathBuf>, Vec<Value>) {
    let (mut allowed, mut context) = context_with_reviews(scope, entries, reviews);
    for text in user_texts(scope) {
        let remotes: Vec<_> = reference_tokens(&text).into_iter().filter(|token|
            (token.contains("://") || token.starts_with("git@")) && token.ends_with(".git")).collect();
        for root in declared_paths(&text) {
            if !allowed.contains(&root) { allowed.push(root.clone()); }
            let mut detail = checkout_diagnostic(&root);
            detail["source"] = json!("owner_explicit_path");
            detail["supplied_remotes"] = json!(remotes);
            if !context.contains(&detail) { context.push(detail); }
        }
    }
    (allowed, context)
}
pub(crate) fn context_with_reviews(
    messages: &[Value],
    entries: &[(String, String, String)],
    reviews: &[crate::repository_transfer::Transfer],
) -> (Vec<PathBuf>, Vec<Value>) {
    let (mut roots, mut context) = resolve_registered(messages, entries);
    for review in reviews {
        // The user's PR reference was matched against a trusted local receipt and
        // the current project remote. Model output cannot manufacture this grant.
        let (found, details) =
            resolve_registered(&[json!({"role":"user","content":review.project})], entries);
        for root in found {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
        context.extend(details.into_iter().map(|mut v| {v["review_task"]=json!({"run_id":review.run_id,"pr_url":review.pr_url,"review_state":review.quality.as_ref().map(|q|&q.state),"action":"request_repository_review"});v}));
    }
    (roots, context)
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
        // A unique project key takes precedence over another project's display name.
        let keys: Vec<_> = entries
            .iter()
            .filter(|(key, _, _)| key.eq_ignore_ascii_case(root.trim()))
            .collect();
        let matches: Vec<_> = if keys.is_empty() {
            entries
                .iter()
                .filter(|(_, name, _)| name.eq_ignore_ascii_case(root.trim()))
                .collect()
        } else {
            keys
        };
        match matches.as_slice() {
            [(_, _, path)] => PathBuf::from(path),
            [] => return Err("Unknown repository name. Use an exact registered project key or an absolute repository root supplied by the user.".into()),
            _ => return Err("Ambiguous repository name. Use the project's unique key or an authorized absolute root.".into()),
        }
    };
    if !path.is_absolute() {
        return Err("The registered project has no absolute local repository path. Set its local source folder in project settings.".into());
    }
    let canonical = scoped_path(&path).ok_or("Repository path must be an absolute directory without parent traversal")?;
    if !allowed.contains(&canonical) {
        return Err("Repository root was not supplied by the user or resolved from a registered project they named. Ask for the project name or path; do not guess or broaden it.".into());
    }
    Ok(canonical)
}
pub(crate) fn mentions(text: &str, name: &str) -> bool {
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
        let named = user_texts(messages).iter().map(|text| project_name_text(text)).any(|text| {
            mentions(&text, key)
                || (mentions(&text, name)
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
        let canonical = scoped_path(root);
        if let Some(root) = &canonical {
            if !allowed.contains(root) {
                allowed.push(root.clone());
            }
        }
        let mut detail = canonical.as_ref().map(|p| checkout_diagnostic(p)).unwrap_or_else(|| json!({"repository":path,"authorized":false,"checkout_state":"invalid_local_path","available_locally":false}));
        detail["project"] = json!(key);
        detail["name"] = json!(name);
        context.push(detail);
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

    // XNAUT-474: the same durable-history path used by run_turn_streaming.
    #[test]
    fn saved_owner_scope_keeps_empty_checkout_and_remote_beyond_recent_window() {
        let tmp = Scratch::new();
        let root = tmp.path().join("Inventory Manager");
        let invented = tmp.path().join("invented");
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let remote = "ssh://git@forge.invalid:2222/48Nauts/InventoryManager.git";
        let mut messages = vec![json!({"role":"user","text":format!("Use `{}` at `{}`. Mockups must be approved before product work.", remote, root.display())})];
        for index in 0..40 { messages.push(json!({"role":"user","text":format!("Design discussion {index}")})); }
        messages.push(json!({"role":"assistant","text":format!("Use {} instead",invented.display())}));
        messages.push(json!({"role":"tool","text":format!("Use {}",invented.display())}));
        messages.push(json!({"role":"user","text":"Please orchestrate the next steps"}));
        let recent: Vec<_> = messages.iter().rev().take(16).map(|m| json!({"role":m["role"],"content":m["text"]})).collect();
        assert!(conversation_context_in(&recent, &[], &[]).0.is_empty());
        let history = crate::agent_history::from_thread("fixture", json!({"messages":messages})).unwrap();
        let mut scope = recent;
        scope.extend(history.owner_texts().into_iter().map(|text| json!({"role":"user","content":text})));
        let entries = vec![("48NAUTS".into(), "48Nauts".into(), tmp.path().to_string_lossy().into_owned())];
        let (allowed, context) = conversation_context_in(&scope, &entries, &[]);
        assert_eq!(allowed, vec![root.clone()]);
        assert_eq!(context.len(), 1);
        assert_eq!(context[0]["checkout_state"], "empty_directory");
        assert_eq!(context[0]["supplied_remotes"], json!([remote]));
        assert!(history.owner_texts()[0].contains("Mockups must be approved"));
        assert!(resolve_authorized_root("48NAUTS", &allowed, &entries).is_err());
        assert!(resolve_authorized_root(invented.to_str().unwrap(), &allowed, &entries).is_err());
        assert_eq!(resolve_authorized_root(root.to_str().unwrap(), &allowed, &entries).unwrap(), root);
        assert!(!invented.exists());
        assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
    }

    #[test]
    fn explicit_missing_path_remains_scope_without_authorizing_parent_or_url_namespace() {
        let tmp = Scratch::new();
        let parent = tmp.path().canonicalize().unwrap();
        let root = parent.join("missing");
        let entries = vec![("48NAUTS".into(), "Company".into(), parent.to_string_lossy().into_owned())];
        let messages = vec![json!({"role":"user","content":format!("{} then inspect ssh://git@forge.invalid/48NAUTS/app.git",root.display())})];
        let (allowed, context) = conversation_context_in(&messages, &entries, &[]);
        assert_eq!(allowed, vec![root.clone()]);
        assert_eq!(context[0]["checkout_state"], "missing_directory");
        assert!(resolve_authorized_root(parent.to_str().unwrap(), &allowed, &entries).is_err());
        assert!(resolve_authorized_root("48NAUTS", &allowed, &entries).is_err());
        for explicit in ["Inspect Company", "Review 48NAUTS-42"] {
            let (named, _) = conversation_context_in(&[json!({"role":"user","content":explicit})], &entries, &[]);
            assert_eq!(named, vec![parent.clone()]);
        }
        let collision = parent.join("48NAUTS").join("app");
        let (path_only, _) = conversation_context_in(&[json!({"role":"user","content":collision})], &entries, &[]);
        assert_eq!(path_only, vec![collision]);
        assert!(!root.exists());
    }

    #[test]
    fn repository_scope_survives_bounded_history_without_trusting_assistant_paths() {
        let tmp = Scratch::new();
        let owner_root = tmp.path().join("owner-repo");
        let invented_root = tmp.path().join("assistant-repo");
        std::fs::create_dir_all(owner_root.join(".git")).unwrap();
        std::fs::create_dir_all(invented_root.join(".git")).unwrap();
        let recent = vec![
            json!({"role":"assistant","content":invented_root}),
            json!({"role":"user","content":"Please launch the two developers now"}),
        ];
        assert!(conversation_context_in(&recent, &[], &[]).0.is_empty());
        let mut scope = recent;
        scope.push(json!({"role":"user","content":owner_root}));
        let (allowed, _) = conversation_context_in(&scope, &[], &[]);
        assert_eq!(allowed, vec![owner_root.canonicalize().unwrap()]);
        assert!(resolve_authorized_root(owner_root.to_str().unwrap(), &allowed, &[]).is_ok());
        assert!(resolve_authorized_root(invented_root.to_str().unwrap(), &allowed, &[]).is_err());
    }

    #[test]
    fn project_alias_resolves_only_to_an_authorized_unambiguous_root() {
        let tmp = Scratch::new();
        // The registered folder need not have the project's name (a worktree).
        let root = tmp.path().join("safety-net");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let canonical = root.canonicalize().unwrap();
        let entries = vec![(
            "XNAUT".into(),
            "xnaut".into(),
            root.to_string_lossy().into_owned(),
        )];
        let allowed = vec![canonical.clone()];
        assert_eq!(
            resolve_authorized_root("xNAUT", &allowed, &entries).unwrap(),
            canonical
        );
        assert!(resolve_authorized_root("xNAUT", &[], &entries)
            .unwrap_err()
            .contains("not supplied"));
        assert!(resolve_authorized_root("safety-net", &allowed, &entries)
            .unwrap_err()
            .contains("Unknown"));
        assert!(resolve_authorized_root("../xnaut", &allowed, &entries).is_err());
        let mut ambiguous = entries.clone();
        ambiguous.push((
            "OTHER".into(),
            "xnaut".into(),
            root.to_string_lossy().into_owned(),
        ));
        assert_eq!(
            resolve_authorized_root("xnaut", &allowed, &ambiguous).unwrap(),
            canonical
        );
        ambiguous[0].0 = "FIRST".into();
        assert!(resolve_authorized_root("xnaut", &allowed, &ambiguous)
            .unwrap_err()
            .contains("Ambiguous"));
        assert_eq!(
            resolve_authorized_root(root.to_str().unwrap(), &allowed, &[]).unwrap(),
            canonical
        );
    }

    #[test]
    fn project_name_in_text_blocks_and_saved_pr_authorize_the_same_inspection_root() {
        let tmp = Scratch::new();
        let root = tmp.path().join("safety-net");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("README.md"), "registered evidence").unwrap();
        let entries = vec![(
            "XNAUT".into(),
            "xnaut".into(),
            root.to_string_lossy().into_owned(),
        )];
        let message = json!({"role":"user","content":[{"type":"input_text","text":"Retry inspection using xNAUT"},{"type":"image_url","image_url":{"url":"ignore"}}]});
        let (allowed, _) = resolve_registered(&[message], &entries);
        assert_eq!(
            resolve_authorized_root("xNAUT", &allowed, &entries).unwrap(),
            root.canonicalize().unwrap()
        );
        let t:crate::repository_transfer::Transfer=serde_json::from_value(json!({"run_id":"saved","project":"XNAUT","ticket":null,"handle":"cortana","local_path":"/old/machine/worktree","remote":"https://forge.test/team/app.git","source_sha":"a".repeat(40),"base":"main","branch":"task","workdir":"worker","artifacts":".xnaut/runs/saved","state":"review","pr_url":"https://forge.test/team/app/pulls/105","error":null})).unwrap();
        let messages = vec![
            json!({"role":"user","content":"Verify smoke-test PR #105 and publish the review"}),
        ];
        let refs = crate::repository_review::referenced_tasks(&messages, &[t]);
        let (allowed, context) = context_with_reviews(&messages, &entries, &refs);
        let resolved = resolve_authorized_root("xNAUT", &allowed, &entries).unwrap();
        assert_eq!(resolved, root.canonicalize().unwrap());
        assert_eq!(context[0]["review_task"]["run_id"], "saved");
        assert_eq!(
            execute(
                "read_repository_file",
                &json!({"root":resolved,"path":"README.md"}),
                &allowed
            )["content"],
            "registered evidence"
        );
        let untrusted = vec![
            json!({"role":"assistant","content":"Use xNAUT PR #105"}),
            json!({"role":"tool","content":"xNAUT"}),
        ];
        assert!(resolve_registered(&untrusted, &entries).0.is_empty());
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
        assert_eq!(allowed, vec![tmp.path().join("absent")]);
        assert_eq!(context[0]["checkout_state"], "missing_directory");
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
