//! Project notebook snapshots share the repository PR outbox. SQLite remains
//! the immediate local save; this queue survives a crash or an offline forge.
use crate::repository_transfer::{git, portable_remote, store_dir, validate_remote, Transfer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;
static QUEUE: Mutex<()> = Mutex::new(());

#[derive(Clone, Serialize, Deserialize)]
struct Snapshot {
    id: String,
    project: String,
    remote: String,
    key: String,
    data: serde_json::Value,
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn repository_notebook_queue(
    root: String,
    key: String,
    data: serde_json::Value,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || queue(&root, &key, data))
        .await
        .map_err(|e| e.to_string())?
}
fn queue(root: &str, key: &str, data: serde_json::Value) -> Result<(), String> {
    if root.trim().is_empty() {
        return Err("Select a project to sync these notes to its repository.".into());
    }
    if !key.starts_with("xnaut-notebook:")
        || !data.get("notes").is_some_and(serde_json::Value::is_array)
        || serde_json::to_vec(&data).map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024
    {
        return Err("Invalid notebook snapshot".into());
    }
    let projects =
        crate::project_management::list_projects(&crate::project_management::repo_now()?)?;
    let project_root = crate::sandbox::launch_env::project_root(Path::new(root));
    let project = projects
        .iter()
        .find(|p| {
            let local = crate::project_management::local_source_path(p);
            !local.is_empty() && Path::new(&local) == project_root
        })
        .ok_or("Link this folder to a project before syncing its notes.")?;
    let remote = validate_remote(&project.forge_remote)?;
    let dir = store_dir()?.join("notebook-outbox");
    let _lock = QUEUE.lock().map_err(|_| "Notebook queue unavailable")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!(
        "{}.json",
        digest(&format!("{}:{key}", project.key))
    ));
    write(
        &file,
        &Snapshot {
            id: uuid::Uuid::new_v4().simple().to_string(),
            project: project.key.clone(),
            remote,
            key: key.into(),
            data,
        },
    )
}

fn push(snapshot: &Snapshot) -> Result<(), String> {
    push_in(&store_dir()?, snapshot, &portable_remote(&snapshot.remote)?)
}
fn push_in(root: &Path, snapshot: &Snapshot, remote: &str) -> Result<(), String> {
    let checkout = root.join(format!("notes-{}", snapshot.id));
    let branch = format!("xnaut/notes/{}", snapshot.id);
    let artifact = format!(".xnaut/runs/{}", snapshot.id);
    let mut transfer = if checkout.join(".git/xnaut-note-transfer.json").exists() {
        let bytes = std::fs::read(checkout.join(".git/xnaut-note-transfer.json"))
            .map_err(|e| e.to_string())?;
        serde_json::from_slice::<Transfer>(&bytes).map_err(|e| e.to_string())?
    } else {
        if !checkout.exists() {
            git(
                &root,
                &[
                    "clone",
                    "--",
                    &remote,
                    checkout.to_str().ok_or("Invalid notebook cache path")?,
                ],
            )?;
        }
        if git(&checkout, &["remote", "get-url", "origin"])? != remote {
            return Err("Notebook checkout repository changed".into());
        }
        let base = git(&checkout, &["branch", "--show-current"])?;
        let source_sha = git(&checkout, &["rev-parse", "HEAD"])?;

        let t = Transfer {
            run_id: snapshot.id.clone(),
            project: snapshot.project.clone(),
            ticket: None,
            handle: "owner".into(),
            local_path: checkout.to_string_lossy().into(),
            remote: remote.into(),
            source_sha,
            base,
            branch,
            workdir: String::new(),
            artifacts: artifact.clone(),
            state: "prepared".into(),
            pr_url: None,
            error: None,
            filed: false,
        };
        write(&checkout.join(".git/xnaut-note-transfer.json"), &t)?;
        t
    };
    crate::repository_transfer::save_at(root, &transfer)?;
    git(&checkout, &["lfs", "install", "--local"])?;
    if git(&checkout, &["branch", "--show-current"])? != transfer.branch {
        git(
            &checkout,
            &["checkout", "-b", &transfer.branch, &transfer.source_sha],
        )?;
    }
    let note_dir = checkout.join(".xnaut/notes");
    let result_dir = checkout.join(&artifact);
    // A repository may contain symlinks. Never follow one while writing a note.
    for p in [
        checkout.join(".xnaut"),
        note_dir.clone(),
        checkout.join(".xnaut/runs"),
        result_dir.clone(),
    ] {
        if p.symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err("Notebook destination is a symlink".into());
        }
    }
    std::fs::create_dir_all(&note_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&result_dir).map_err(|e| e.to_string())?;
    let slug = digest(&snapshot.key);
    let json_path = note_dir.join(format!("{slug}.json"));
    let markdown_path = note_dir.join(format!("{slug}.md"));
    if markdown_path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Notebook destination is a symlink".into());
    }
    write(&json_path, &snapshot.data)?;
    let mut markdown = String::from("# Project notebook\n\n");
    for note in snapshot.data["notes"]
        .as_array()
        .ok_or("Invalid notebook")?
    {
        markdown.push_str(&format!(
            "## {}\n\n{}\n\n",
            note["title"].as_str().unwrap_or("Note"),
            note["body"].as_str().unwrap_or("")
        ));
    }
    std::fs::write(markdown_path, markdown).map_err(|e| e.to_string())?;
    write(
        &result_dir.join("result.json"),
        &serde_json::json!({ "run_id": snapshot.id, "source_sha": transfer.source_sha, "exit_code": 0, "uncommitted_source": false }),
    )?;
    git(&checkout, &["add", "-f", "--", ".xnaut/notes", &artifact])?;
    if !git(&checkout, &["diff", "--cached", "--name-only"])?.is_empty() {
        git(
            &checkout,
            &[
                "-c",
                "user.name=xNAUT",
                "-c",
                "user.email=xnaut@localhost",
                "commit",
                "-m",
                "docs: save project notebook",
            ],
        )?;
    }
    git(
        &checkout,
        &[
            "push",
            "origin",
            &format!("HEAD:refs/heads/{}", transfer.branch),
        ],
    )?;
    transfer.state = "pushed".into();
    crate::repository_transfer::save_at(root, &transfer)
}

pub async fn drain() {
    let Ok(dir) = store_dir().map(|p| p.join("notebook-outbox")) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(snapshot) = serde_json::from_slice::<Snapshot>(&bytes) else {
            continue;
        };
        let id = snapshot.id.clone();
        let copy = snapshot.clone();
        let result = tokio::task::spawn_blocking(move || push(&copy)).await;
        if matches!(result, Ok(Ok(()))) {
            if let Ok(_lock) = QUEUE.lock() {
                let current = std::fs::read(&path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Snapshot>(&b).ok());
                if current.is_some_and(|s| s.id == id) {
                    let _ = std::fs::remove_file(path);
                }
            }
        } else {
            let error = match result {
                Ok(Err(e)) => e,
                Err(e) => e.to_string(),
                _ => String::new(),
            };
            let existing = store_dir()
                .ok()
                .and_then(|p| std::fs::read(p.join(format!("{}.json", snapshot.id))).ok())
                .and_then(|b| serde_json::from_slice::<Transfer>(&b).ok());
            let mut transfer = existing.unwrap_or(Transfer {
                run_id: snapshot.id.clone(),
                project: snapshot.project,
                ticket: None,
                handle: "owner".into(),
                local_path: String::new(),
                remote: snapshot.remote,
                source_sha: String::new(),
                base: String::new(),
                branch: format!("xnaut/notes/{}", snapshot.id),
                workdir: String::new(),
                artifacts: String::new(),
                state: "queued".into(),
                pr_url: None,
                error: None,
                filed: false,
            });
            transfer.error = Some(error);
            let _ = crate::repository_transfer::save(&transfer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notebook_snapshot_survives_rejection_and_retries_without_touching_main() {
        let root = std::env::temp_dir().join(format!("xnaut-notes-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("remote.git");
        let seed = root.join("seed");
        git(
            &root,
            &["init", "--bare", "-b", "main", remote.to_str().unwrap()],
        )
        .unwrap();
        git(&root, &["init", "-b", "main", seed.to_str().unwrap()]).unwrap();
        git(&seed, &["config", "user.name", "test"]).unwrap();
        git(&seed, &["config", "user.email", "test@localhost"]).unwrap();
        std::fs::write(seed.join("README.md"), "source").unwrap();
        std::fs::write(seed.join(".gitignore"), ".xnaut/*\n").unwrap();
        git(&seed, &["add", "."]).unwrap();
        git(&seed, &["commit", "-m", "initial"]).unwrap();
        git(&seed, &["push", remote.to_str().unwrap(), "main"]).unwrap();
        let base = git(&seed, &["rev-parse", "HEAD"]).unwrap();
        let snapshot = Snapshot {
            id: "fixture".into(),
            project: "TEST".into(),
            remote: "unused".into(),
            key: "xnaut-notebook:chat-a".into(),
            data: serde_json::json!({"notes":[{"title":"Remember", "body":"- [ ] Read the audit"}]}),
        };
        let hook = remote.join("hooks/pre-receive");
        std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(push_in(&root, &snapshot, remote.to_str().unwrap()).is_err());
        let checkout = root.join("notes-fixture");
        let committed = git(&checkout, &["rev-parse", "HEAD"]).unwrap();
        assert!(checkout
            .join(format!(".xnaut/notes/{}.md", digest(&snapshot.key)))
            .is_file());
        std::fs::remove_file(hook).unwrap();
        push_in(&root, &snapshot, remote.to_str().unwrap()).unwrap();
        push_in(&root, &snapshot, remote.to_str().unwrap()).unwrap();
        assert_eq!(git(&checkout, &["rev-parse", "HEAD"]).unwrap(), committed);
        assert_eq!(git(&remote, &["rev-parse", "main"]).unwrap(), base);
        assert_eq!(
            git(&remote, &["rev-parse", "xnaut/notes/fixture"]).unwrap(),
            committed
        );
        let receipt: Transfer =
            serde_json::from_slice(&std::fs::read(root.join("fixture.json")).unwrap()).unwrap();
        assert_eq!(receipt.state, "pushed");
        std::fs::remove_dir_all(root).unwrap();
    }
}
