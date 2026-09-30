//! Conversation storage independent of WKWebView's per-bundle profile.
//! Legacy snapshots are imported read-only and kept for recovery; writes use
//! optimistic revisions so two app instances cannot silently overwrite a chat.
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub value: Option<String>,
    pub revision: i64,
}
type Records = BTreeMap<String, Record>;
fn allowed(key: &str) -> bool {
    matches!(
        key,
        "xnaut-agent-threads:v1" | "xnaut-chat-sessions" | "xnaut-librarian-threads-migrated"
    ) || key.starts_with("xnaut-chat-history:")
        || key.starts_with("xnaut-chat-model:")
        || key.starts_with("xnaut-chat-title:")
        || key.starts_with("xnaut-notebook:")
}
fn root() -> Result<PathBuf, String> {
    Ok(dirs::config_dir()
        .ok_or("Configuration directory unavailable")?
        .join("xnaut"))
}
fn connect(root: &Path) -> Result<Connection, String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = root.join("conversations.sqlite");
    let db = Connection::open(&path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS conversations (key TEXT PRIMARY KEY, value TEXT, revision INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS legacy_snapshots (source TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY(source,key));").map_err(|e|e.to_string())?;
    Ok(db)
}
fn get(db: &Connection, key: &str) -> Result<Option<Record>, String> {
    db.query_row(
        "SELECT value,revision FROM conversations WHERE key=?",
        [key],
        |row| {
            Ok(Record {
                value: row.get(0)?,
                revision: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}
fn read_all(db: &Connection) -> Result<Records, String> {
    let mut stmt = db
        .prepare("SELECT key,value,revision FROM conversations")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                Record {
                    value: row.get(1)?,
                    revision: row.get(2)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}
// Distinct thread IDs are preserved. Same-ID conflicts remain in the immutable
// source snapshot; the newer thread wins. Never concatenate duplicate messages.
fn merge_threads(current: &str, incoming: &str) -> Result<String, String> {
    let mut a: Value = serde_json::from_str(current).map_err(|e| e.to_string())?;
    let b: Value = serde_json::from_str(incoming).map_err(|e| e.to_string())?;
    let map = a.as_object_mut().ok_or("Invalid saved threads")?;
    for (agent, threads) in b.as_object().ok_or("Invalid legacy threads")? {
        let list = map
            .entry(agent)
            .or_insert(serde_json::json!([]))
            .as_array_mut()
            .ok_or("Invalid agent threads")?;
        for thread in threads.as_array().ok_or("Invalid legacy thread list")? {
            let Some(id) = thread["id"].as_str() else {
                continue;
            };
            match list.iter().position(|t| t["id"].as_str() == Some(id)) {
                Some(i) if thread["updated_at"].as_str() > list[i]["updated_at"].as_str() => {
                    list[i] = thread.clone();
                }
                None => list.push(thread.clone()),
                _ => {}
            }
        }
        list.sort_by(|a, b| b["updated_at"].as_str().cmp(&a["updated_at"].as_str()));
    }
    serde_json::to_string(&a).map_err(|e| e.to_string())
}
fn import(db: &Connection, source: &str, key: &str, value: &str) -> Result<(), String> {
    if !allowed(key) || value.len() > 32 * 1024 * 1024 {
        return Ok(());
    }
    // Only conversation JSON (and the migration flag); no app credentials.
    if key != "xnaut-librarian-threads-migrated" && serde_json::from_str::<Value>(value).is_err() {
        return Ok(());
    }
    let inserted = db
        .execute(
            "INSERT OR IGNORE INTO legacy_snapshots VALUES (?,?,?)",
            params![source, key, value],
        )
        .map_err(|e| e.to_string())?;
    if inserted == 0 {
        return Ok(());
    }
    match get(db, key)? {
        None => {
            db.execute(
                "INSERT INTO conversations VALUES (?,?,1)",
                params![key, value],
            )
            .map_err(|e| e.to_string())?;
        }
        Some(old) if key == "xnaut-agent-threads:v1" && old.value.is_some() => {
            let merged = merge_threads(old.value.as_deref().unwrap(), value)?;
            if Some(&merged) != old.value.as_ref() {
                db.execute(
                    "UPDATE conversations SET value=?,revision=revision+1 WHERE key=?",
                    params![merged, key],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        // Conflicting ordinary chats get a separate, discoverable recovery key.
        Some(old)
            if key.starts_with("xnaut-chat-history:")
                && old.value.as_deref().is_some_and(|v| v != value) =>
        {
            use sha2::{Digest, Sha256};
            let digest = format!("{:x}", Sha256::digest(format!("{source}:{key}").as_bytes()));
            let recovered = format!("xnaut-chat-history:recovered-{}", &digest[..16]);
            db.execute(
                "INSERT OR IGNORE INTO conversations VALUES (?,?,1)",
                params![recovered, value],
            )
            .map_err(|e| e.to_string())?;
        }
        _ => {}
    }
    Ok(())
}
fn decode(value: rusqlite::types::Value) -> Option<String> {
    match value {
        rusqlite::types::Value::Text(s) => Some(s),
        rusqlite::types::Value::Blob(b) if b.len() % 2 == 0 => String::from_utf16(
            &b.chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect::<Vec<_>>(),
        )
        .ok(),
        _ => None,
    }
}
fn legacy_files(path: &Path, depth: u8, files: &mut Vec<PathBuf>) {
    if depth > 7 || files.len() > 100 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_file() && entry.file_name() == "localstorage.sqlite3" {
            files.push(entry.path());
        } else if kind.is_dir() && !entry.file_name().to_string_lossy().contains("backup") {
            legacy_files(&entry.path(), depth + 1, files);
        }
    }
}
fn import_webkit(db: &Connection) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let Some(home) = dirs::home_dir() else {
            return Ok(());
        };
        // Explicit xNaut profiles only. No unrelated browser/app data is read.
        for id in [
            "com.nautcode.xnaut",
            "com.nautcode.xnaut.voice-test",
            "com.nautcode.xnaut.fix-preview",
            "com.nautcode.xnaut.tool-catalog-preview",
            "com.nautcode.xnaut.decisions-preview",
            "com.nautcode.xnaut.test",
        ] {
            let mut paths = Vec::new();
            legacy_files(
                &home
                    .join("Library/WebKit")
                    .join(id)
                    .join("WebsiteData/Default"),
                0,
                &mut paths,
            );
            paths.sort();
            for path in paths {
                let old =
                    Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                        .map_err(|e| format!("Cannot recover {}: {e}", path.display()))?;
                let mut query = old
                    .prepare("SELECT key,value FROM ItemTable")
                    .map_err(|e| e.to_string())?;
                let rows = query
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, rusqlite::types::Value>(0)?,
                            row.get::<_, rusqlite::types::Value>(1)?,
                        ))
                    })
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    let (key, value) = row.map_err(|e| e.to_string())?;
                    if let (Some(key), Some(value)) = (decode(key), decode(value)) {
                        import(db, &path.to_string_lossy(), &key, &value)?;
                    }
                }
            }
        }
    }
    Ok(())
}
// Preserve an interrupted/stale write as a separate visible recovery chat.
// Never replace newer canonical text or resurrect a deleted canonical thread.
fn recover_pending(db: &Connection, key: &str, value: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(format!("{key}:{value}").as_bytes()));
    let source = format!("interrupted-{}", &digest[..16]);
    db.execute(
        "INSERT OR IGNORE INTO legacy_snapshots VALUES (?,?,?)",
        params![source, key, value],
    )
    .map_err(|e| e.to_string())?;
    let data: Value = serde_json::from_str(value).map_err(|e| e.to_string())?;
    let mut recoveries = Vec::new();
    if key == "xnaut-agent-threads:v1" {
        let old = get(db, key)?
            .and_then(|r| r.value)
            .and_then(|v| serde_json::from_str::<Value>(&v).ok())
            .unwrap_or(serde_json::json!({}));
        if let Some(agents) = data.as_object() {
            for (agent, threads) in agents {
                for thread in threads.as_array().into_iter().flatten() {
                    let matches = old[agent]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|t| t == thread);
                    if matches {
                        continue;
                    }
                    let messages:Vec<Value>=thread["messages"].as_array().into_iter().flatten().map(|m|serde_json::json!({"role":if m["role"]=="user" {"user"}else{"assistant"},"content":m["text"]})).collect();
                    recoveries.push((
                        format!("{}-{}", agent, thread["id"].as_str().unwrap_or("thread")),
                        serde_json::to_string(&messages).map_err(|e| e.to_string())?,
                    ));
                }
            }
        }
    } else if key.starts_with("xnaut-chat-history:") {
        recoveries.push(("chat".into(), value.into()));
    }
    if key.starts_with("xnaut-notebook:") {
        // Preserve interrupted edits separately, visible in the notebook's
        // recovery picker; never overwrite the newer canonical notebook.
        let recovery_key = format!("{key}:recovered:{source}");
        db.execute("INSERT OR IGNORE INTO conversations VALUES (?,?,1)",
            params![recovery_key, value]).map_err(|e| e.to_string())?;
    }
    for (label, body) in recoveries {
        let name = format!("xnaut-chat-history:recovered-{source}-{label}");
        db.execute(
            "INSERT OR IGNORE INTO conversations VALUES (?,?,1)",
            params![name, body],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub fn conversation_store_load(
    app: tauri::AppHandle,
    legacy: BTreeMap<String, String>,
    pending: Option<BTreeMap<String, String>>,
) -> Result<Records, String> {
    let mut db = connect(&root()?)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    import_webkit(&tx)?;
    for (key, value) in legacy {
        import(
            &tx,
            &format!("webview:{}", app.config().identifier),
            &key,
            &value,
        )?;
    }
    for (key, body) in pending.unwrap_or_default() {
        if !allowed(&key) {
            continue;
        }
        let item: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        if let Some(value) = item["value"].as_str() {
            if get(&tx, &key)?.and_then(|r| r.value).as_deref() != Some(value) {
                recover_pending(&tx, &key, value)?;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    read_all(&db)
}
fn put(
    db: &mut Connection,
    key: &str,
    value: Option<String>,
    revision: i64,
) -> Result<Record, String> {
    if !allowed(key) {
        return Err("Not a conversation storage key".into());
    }
    if value.as_ref().is_some_and(|s| s.len() > 32 * 1024 * 1024) {
        return Err(
            "Conversation storage entry exceeds 32 MiB; export or split this conversation.".into(),
        );
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let previous = get(&tx, key)?.map(|r| r.revision).unwrap_or(0);
    if previous != revision {
        return Err("Conversation changed in another window. Your local copy is retained; reopen the conversation before continuing.".into());
    }
    let next = Record {
        value,
        revision: previous + 1,
    };
    tx.execute("INSERT INTO conversations VALUES (?,?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value,revision=excluded.revision",params![key,next.value,next.revision]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(next)
}
#[tauri::command]
pub fn conversation_store_put(
    key: String,
    value: Option<String>,
    revision: i64,
) -> Result<Record, String> {
    put(&mut connect(&root()?)?, &key, value, revision)
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
    fn recovery_keeps_threads_and_tombstones_and_refuses_stale_writers() {
        let tmp = Scratch::new();
        let mut db = connect(tmp.path()).unwrap();
        let a = r#"{"stark":[{"id":"a","messages":[{"text":"remember me"}]}]}"#;
        let b = r#"{"stark":[{"id":"b","messages":[]}]}"#;
        import(&db, "old", "xnaut-agent-threads:v1", a).unwrap();
        import(&db, "new", "xnaut-agent-threads:v1", b).unwrap();
        let record = get(&db, "xnaut-agent-threads:v1").unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(record.value.as_ref().unwrap()).unwrap()["stark"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(put(&mut db, "xnaut-agent-threads:v1", Some(b.into()), 1).is_err());
        put(&mut db, "xnaut-chat-history:test", Some("[]".into()), 0).unwrap();
        put(&mut db, "xnaut-chat-history:test", None, 1).unwrap();
        import(
            &db,
            "old",
            "xnaut-chat-history:test",
            "[{\"content\":\"deleted\"}]",
        )
        .unwrap();
        assert!(get(&db, "xnaut-chat-history:test")
            .unwrap()
            .unwrap()
            .value
            .is_none());
        assert!(put(&mut db, "api_key", Some("secret".into()), 0).is_err());
        drop(db);
        let db = connect(tmp.path()).unwrap();
        assert!(read_all(&db)
            .unwrap()
            .contains_key("xnaut-agent-threads:v1"));
        assert_eq!(
            db.query_row("SELECT count(*) FROM legacy_snapshots", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
    // Explicit owner-data probe: reads legacy profiles into a temporary database,
    // never updates WebKit, the real conversation store, or running sessions.
    #[test]
    #[ignore]
    fn owner_legacy_recovery_probe() {
        let tmp=Scratch::new();let db=connect(tmp.path()).unwrap();
        import_webkit(&db).unwrap();
        let all=read_all(&db).unwrap();
        let threads:Value=serde_json::from_str(all["xnaut-agent-threads:v1"].value.as_ref().unwrap()).unwrap();
        let count:usize=threads.as_object().unwrap().values().map(|v|v.as_array().unwrap().len()).sum();
        let messages:usize=threads.as_object().unwrap().values().flat_map(|v|v.as_array().unwrap()).map(|t|t["messages"].as_array().map(Vec::len).unwrap_or(0)).sum();
        assert!(count>0);assert!(messages>0);
        println!("Recovered {} storage records, {} agent threads, {} retained messages into an isolated database",all.len(),count,messages);
        let repo=std::path::PathBuf::from(std::env::var("XNAUT_READ_PROBE_ROOT").expect("explicit project root"));
        let scope=crate::repository_read::roots(&[serde_json::json!({"role":"user","content":format!("Review {}",repo.display())})]);
        let read=crate::repository_read::execute("read_repository_file",&serde_json::json!({"root":repo,"path":"docs/updater.md"}),&scope);
        assert_eq!(read["ok"],true);assert!(!read["content"].as_str().unwrap().is_empty());
        println!("Read-only project probe passed: docs/updater.md, {} lines",read["total_lines"]);
    }

    #[test]
    fn interrupted_write_is_recovered_without_replacing_newer_history() {
        let tmp = Scratch::new();
        let mut db = connect(tmp.path()).unwrap();
        put(
            &mut db,
            "xnaut-chat-history:one",
            Some("[{\"content\":\"newer\"}]".into()),
            0,
        )
        .unwrap();
        recover_pending(
            &db,
            "xnaut-chat-history:one",
            "[{\"content\":\"interrupted\"}]",
        )
        .unwrap();
        assert!(get(&db, "xnaut-chat-history:one")
            .unwrap()
            .unwrap()
            .value
            .unwrap()
            .contains("newer"));
        assert!(read_all(&db).unwrap().values().any(|v| v
            .value
            .as_deref()
            .unwrap_or("")
            .contains("interrupted")));
    }

    #[test]
    fn notebook_edits_recover_without_overwriting_newer_notes() {
        let tmp = Scratch::new();
        let mut db = connect(tmp.path()).unwrap();
        let key = "xnaut-notebook:agent:cortana:one";
        put(&mut db, key, Some(r#"{"notes":[{"body":"newer"}]}"#.into()), 0).unwrap();
        assert!(put(&mut db, key, Some("{}".into()), 0).is_err());
        recover_pending(&db, key, r#"{"notes":[{"body":"interrupted"}]}"#).unwrap();
        let records = read_all(&db).unwrap();
        assert!(records[key].value.as_ref().unwrap().contains("newer"));
        assert!(records.iter().any(|(k,v)| k.starts_with(&format!("{key}:recovered:")) && v.value.as_ref().unwrap().contains("interrupted")));
    }

    #[test]
    fn conflicting_chat_is_recoverable_and_import_is_idempotent() {
        let tmp = Scratch::new();
        let db = connect(tmp.path()).unwrap();
        import(
            &db,
            "one",
            "xnaut-chat-history:test",
            "[{\"content\":\"first\"}]",
        )
        .unwrap();
        import(
            &db,
            "two",
            "xnaut-chat-history:test",
            "[{\"content\":\"second\"}]",
        )
        .unwrap();
        import(
            &db,
            "two",
            "xnaut-chat-history:test",
            "[{\"content\":\"second\"}]",
        )
        .unwrap();
        assert_eq!(read_all(&db).unwrap().len(), 2);
        assert_eq!(
            decode(rusqlite::types::Value::Blob(vec![65, 0, 66, 0])).unwrap(),
            "AB"
        );
    }
}
