//! Open-source, local recovery for native Agent Space turns. No paid Registry.
//! Inspired by Pi Durable (MIT), packages/durable/README.md: persisted submission
//! identity, task intent before effects, committed results, and explicit safe replay.
//! https://github.com/earendil-works/pi/tree/main/packages/durable
//! This is a native Rust implementation, not a port or a JavaScript dependency.
//! Unlike Pi's general task graph, this store checkpoints xNAUT's existing bounded
//! model/tool loop. Completed steps reconstruct its state without rerunning effects.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::Manager;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub handle: String,
    pub messages: Vec<crate::chat::ChatMessage>,
    pub repository_context: Option<Vec<String>>,
    pub thread_id: Option<String>,
    pub project_scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    pub request_id: String,
    pub handle: String,
    pub thread_id: Option<String>,
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub partial: String,
    pub outcome: Option<Value>,
    pub execution_receipts: Vec<Value>,
}

#[derive(Clone)]
struct Context {
    root: PathBuf,
    id: String,
    recovering: bool,
    stream: Arc<Mutex<(String, Instant)>>,
}
tokio::task_local! { static CURRENT: Context; }
const FORMAT: i64 = 1;

fn error(e: impl std::fmt::Display) -> String {
    format!("Durable execution: {e}")
}
fn db(root: &Path) -> Result<Connection, String> {
    let db = crate::conversation_store::connect(root).map_err(error)?;
    db.execute_batch(
        "PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS agent_turns (
            id TEXT PRIMARY KEY, scope TEXT NOT NULL, request TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending', result TEXT, error TEXT,
            partial TEXT NOT NULL DEFAULT '', acknowledged INTEGER NOT NULL DEFAULT 0,
            format INTEGER NOT NULL DEFAULT 1);
        CREATE TABLE IF NOT EXISTS agent_turn_checkpoints (
            turn_id TEXT NOT NULL, name TEXT NOT NULL, value TEXT NOT NULL,
            PRIMARY KEY(turn_id,name));
        CREATE TABLE IF NOT EXISTS agent_turn_steps (
            turn_id TEXT NOT NULL, name TEXT NOT NULL, input TEXT NOT NULL,
            replay_safe INTEGER NOT NULL, result TEXT, PRIMARY KEY(turn_id,name));",
    )
    .map_err(error)?;
    Ok(db)
}
fn scope(id: &str, request: &Request) -> String {
    serde_json::to_string(&(&request.handle, request.thread_id.as_deref().unwrap_or(id))).unwrap()
}
fn admit(root: &Path, id: &str, request: &Request) -> Result<(), String> {
    if id.is_empty() || id.len() > 200 {
        return Err(error("Invalid request identity"));
    }
    let input = serde_json::to_string(request).map_err(error)?;
    let db = db(root)?;
    db.execute(
        "INSERT OR IGNORE INTO agent_turns(id,scope,request,format) VALUES(?,?,?,?)",
        params![id, scope(id, request), input, FORMAT],
    )
    .map_err(error)?;
    let existing: String = db
        .query_row("SELECT request FROM agent_turns WHERE id=?", [id], |r| {
            r.get(0)
        })
        .map_err(error)?;
    if existing != input {
        return Err(error(
            "Request ID already belongs to different input or scope",
        ));
    }
    Ok(())
}
fn request(root: &Path, id: &str) -> Result<Request, String> {
    let (value, format): (String, i64) = db(root)?
        .query_row(
            "SELECT request,format FROM agent_turns WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(error)?;
    if format != FORMAT {
        return Err(error(
            "Unsupported execution checkpoint version; migration is required before resuming",
        ));
    }
    serde_json::from_str(&value).map_err(error)
}
fn checkpoint_in(root: &Path, id: &str, name: &str) -> Result<Option<Value>, String> {
    let value: Option<String> = db(root)?
        .query_row(
            "SELECT value FROM agent_turn_checkpoints WHERE turn_id=? AND name=?",
            params![id, name],
            |r| r.get(0),
        )
        .optional()
        .map_err(error)?;
    value
        .map(|v| serde_json::from_str(&v).map_err(error))
        .transpose()
}
fn save_in(root: &Path, id: &str, name: &str, value: &Value) -> Result<(), String> {
    db(root)?.execute("INSERT INTO agent_turn_checkpoints VALUES(?,?,?) ON CONFLICT(turn_id,name) DO UPDATE SET value=excluded.value",
        params![id,name,value.to_string()]).map_err(error)?;
    Ok(())
}
pub fn checkpoint(name: &str) -> Result<Option<Value>, String> {
    CURRENT
        .try_with(|c| checkpoint_in(&c.root, &c.id, name))
        .unwrap_or(Ok(None))
}
pub fn recovering() -> bool {
    CURRENT.try_with(|c| c.recovering).unwrap_or(false)
}
pub fn save(name: &str, value: &Value) -> Result<(), String> {
    CURRENT
        .try_with(|c| save_in(&c.root, &c.id, name, value))
        .unwrap_or(Ok(()))
}
pub fn bind(name: &str, value: &Value) -> Result<(), String> {
    match checkpoint(name)? {
        Some(previous) if previous != *value => Err(error(format!("{name} changed; interrupted turn remains recorded and was not resumed under a different scope"))),
        Some(_) => Ok(()),
        None => save(name,value),
    }
}

// None means the intent has been committed and this owner may execute it.
// Some is a committed result, including an explicit interrupted outcome.
fn begin_in(
    root: &Path,
    id: &str,
    name: &str,
    input: &Value,
    safe: bool,
) -> Result<Option<Value>, String> {
    let mut db = db(root)?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(error)?;
    let existing: Option<(String, bool, Option<String>)> = tx
        .query_row(
            "SELECT input,replay_safe,result FROM agent_turn_steps WHERE turn_id=? AND name=?",
            params![id, name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(error)?;
    let result = match existing {
        Some((old, _, _)) if old != input.to_string() => {
            return Err(error(
                "Checkpoint input changed; refusing a different effect under the same identity",
            ))
        }
        Some((_, _, Some(result))) => Some(serde_json::from_str(&result).map_err(error)?),
        Some((_, was_safe, None)) if !(safe && was_safe) => {
            let result = json!({"ok":false,"interrupted":true,"operation_id":format!("{id}/{name}"),
                "error":"Execution was interrupted before its result was committed. The action may already have succeeded. It was not repeated. Inspect its native receipt or external state before any further mutation; do not infer failure or repeat the write."});
            tx.execute(
                "UPDATE agent_turn_steps SET result=? WHERE turn_id=? AND name=?",
                params![result.to_string(), id, name],
            )
            .map_err(error)?;
            Some(result)
        }
        Some(_) => None,
        None => {
            tx.execute(
                "INSERT INTO agent_turn_steps(turn_id,name,input,replay_safe) VALUES(?,?,?,?)",
                params![id, name, input.to_string(), safe],
            )
            .map_err(error)?;
            None
        }
    };
    tx.commit().map_err(error)?;
    Ok(result)
}
pub fn begin(name: &str, input: &Value, safe: bool) -> Result<Option<Value>, String> {
    CURRENT
        .try_with(|c| {
            // Prepared context plus committed responses/results reconstruct model
            // requests. Retain their digest instead of another full transcript copy
            // at every round; keep tool arguments directly inspectable.
            let digest;
            let input = if name.starts_with("model:") {
                digest =
                    json!({"sha256":format!("{:x}",Sha256::digest(input.to_string().as_bytes()))});
                &digest
            } else {
                input
            };
            begin_in(&c.root, &c.id, name, input, safe)
        })
        .unwrap_or(Ok(None))
}
fn finish_in(root: &Path, id: &str, name: &str, result: &Value) -> Result<(), String> {
    let changed = db(root)?
        .execute(
            "UPDATE agent_turn_steps SET result=? WHERE turn_id=? AND name=? AND result IS NULL",
            params![result.to_string(), id, name],
        )
        .map_err(error)?;
    if changed != 1 {
        return Err(error("Missing or already completed operation intent"));
    }
    Ok(())
}
pub fn finish(name: &str, result: &Value) -> Result<(), String> {
    CURRENT
        .try_with(|c| finish_in(&c.root, &c.id, name, result))
        .unwrap_or(Ok(()))
}
pub fn has_tool_intents() -> bool {
    CURRENT
        .try_with(|c| {
            db(&c.root).and_then(|db| {
                db.query_row(
        "SELECT EXISTS(SELECT 1 FROM agent_turn_steps WHERE turn_id=? AND name LIKE 'tool:%')",
        [&c.id], |r| r.get(0)).map_err(error)
            })
        })
        .unwrap_or(Ok(false))
        .unwrap_or(true)
}
// This allowlist is deliberately about implementations, never name prefixes or
// an MCP server's unverified readOnlyHint. Writes use their existing receipts for
// reconciliation; they are not automatically repeated by this layer.
pub fn replay_safe(name: &str) -> bool {
    matches!(
        name,
        "list_repository_files"
            | "read_repository_file"
            | "read_conversation_history"
            | "read_conversation_tasks"
            | "read_project_work"
            | "xnaut_search_tools"
            | "xnaut_load_tools"
    )
}

// Batch live text, committing before emission. The uncommitted tail is never
// displayed. A recovered model request starts a fresh visible stream, while its
// interrupted partial remains in the durable checkpoint for inspection.
pub fn emit_chunk(app: &tauri::AppHandle, id: &str, delta: &str) -> Result<(), String> {
    let chunk = CURRENT
        .try_with(|c| -> Result<Option<String>, String> {
            let mut stream = c.stream.lock().map_err(error)?;
            stream.0.push_str(delta);
            if stream.1.elapsed() < Duration::from_millis(100) {
                return Ok(None);
            }
            let value = std::mem::take(&mut stream.0);
            db(&c.root)?
                .execute(
                    "UPDATE agent_turns SET partial=partial || ? WHERE id=?",
                    params![value, c.id],
                )
                .map_err(error)?;
            stream.1 = Instant::now();
            Ok(Some(value))
        })
        .unwrap_or_else(|_| Ok(Some(delta.to_string())))?;
    if let Some(delta) = chunk {
        let _ = tauri::Emitter::emit(app, "chat://chunk", json!({"requestId":id,"delta":delta}));
    }
    Ok(())
}

fn lease(root: &Path, name: &str) -> Result<Option<File>, String> {
    let dir = root.join("agent-turn-locks");
    std::fs::create_dir_all(&dir).map_err(error)?;
    let path = dir.join(format!("{:x}.lock", Sha256::digest(name.as_bytes())));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(error)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(e) => Err(error(e)),
    }
}
fn pending(root: &Path) -> Result<Vec<(String, String)>, String> {
    let db = db(root)?;
    let mut q = db
        .prepare(
            "SELECT id,scope FROM agent_turns WHERE status IN ('pending','running') ORDER BY rowid",
        )
        .map_err(error)?;
    let rows = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(error)?;
    rows.collect::<Result<_, _>>().map_err(error)
}
fn view(root: &Path, id: &str) -> Result<View, String> {
    let db = db(root)?;
    let (raw, status, result, error, partial): (
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    ) = db
        .query_row(
            "SELECT request,status,result,error,partial FROM agent_turns WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(error)?;
    let req: Request = serde_json::from_str(&raw).map_err(self::error)?;
    let mut query = db.prepare("SELECT input,result FROM agent_turn_steps WHERE turn_id=? AND name LIKE 'tool:%' AND result IS NOT NULL ORDER BY rowid").map_err(self::error)?;
    let rows = query
        .query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(self::error)?;
    let mut execution_receipts = Vec::new();
    for row in rows {
        let (input, result) = row.map_err(self::error)?;
        let input: Value = serde_json::from_str(&input).map_err(self::error)?;
        if input["name"] == "start_repository_task" {
            let result: Value = serde_json::from_str(&result).map_err(self::error)?;
            if result["ok"] == true {
                execution_receipts.push(result);
            }
        }
    }
    Ok(View {
        request_id: id.into(),
        handle: req.handle,
        thread_id: req.thread_id,
        status,
        result,
        error,
        partial,
        outcome: checkpoint_in(root, id, "outcome")?,
        execution_receipts,
    })
}
fn settle(root: &Path, id: &str, result: &Result<String, String>) -> Result<(), String> {
    let (status, text, error) = match result {
        Ok(text) => ("completed", Some(text.as_str()), None),
        Err(e) => ("failed", None, Some(e.as_str())),
    };
    db(root)?
        .execute(
            "UPDATE agent_turns SET status=?,result=?,error=? WHERE id=?",
            params![status, text, error, id],
        )
        .map_err(self::error)?;
    Ok(())
}
fn kick(app: tauri::AppHandle, root: PathBuf, owner_scope: String) {
    tauri::async_runtime::spawn(async move {
        let _lease = match lease(&root, &owner_scope) {
            Ok(Some(v)) => v,
            Ok(None) => return,
            Err(e) => {
                log_error(&e);
                return;
            }
        };
        loop {
            let id = match pending(&root) {
                Ok(rows) => match rows.into_iter().find(|(_, s)| s == &owner_scope) {
                    Some((id, _)) => id,
                    None => return,
                },
                Err(e) => {
                    log_error(&e);
                    return;
                }
            };
            let req = match request(&root, &id) {
                Ok(v) => v,
                Err(e) => {
                    log_error(&e);
                    let _ = settle(&root, &id, &Err(e));
                    return;
                }
            };
            let recovering = match view(&root, &id) {
                Ok(v) => v.status == "running",
                Err(e) => {
                    log_error(&e);
                    return;
                }
            };
            let ctx = Context {
                root: root.clone(),
                id: id.clone(),
                recovering,
                stream: Arc::new(Mutex::new((
                    String::new(),
                    Instant::now() - Duration::from_secs(1),
                ))),
            };
            if let Err(e) = db(&root).and_then(|db| {
                db.execute("UPDATE agent_turns SET status='running' WHERE id=?", [&id])
                    .map_err(error)
            }) {
                log_error(&e);
                return;
            }
            let result = CURRENT
                .scope(ctx, async {
                    crate::agent_profiles::agent_chat_turn_inner(
                        app.clone(),
                        app.state(),
                        req.handle,
                        id.clone(),
                        req.messages,
                        req.repository_context,
                        req.thread_id,
                        req.project_scope,
                    )
                    .await
                })
                .await;
            if let Err(e) = settle(&root, &id, &result) {
                log_error(&e);
                return;
            }
            let _ =
                tauri::Emitter::emit(&app, "durable-agent-turn-changed", json!({"requestId":id}));
        }
    });
}
fn log_error(message: &str) {
    let _ = crate::debug_log::debug_log_append(vec![format!("[durable-turn] {message}")]);
}
pub async fn submit(app: tauri::AppHandle, id: String, request: Request) -> Result<String, String> {
    let root = crate::conversation_store::root()?;
    admit(&root, &id, &request)?;
    kick(app, root.clone(), scope(&id, &request));
    loop {
        let current = view(&root, &id)?;
        match current.status.as_str() {
            "completed" => return Ok(current.result.unwrap_or_default()),
            "failed" => return Err(current.error.unwrap_or_else(|| error("Turn failed"))),
            _ => tokio::time::sleep(Duration::from_millis(150)).await,
        }
    }
}
pub fn spawn_recovery(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            match crate::conversation_store::root()
                .and_then(|root| pending(&root).map(|rows| (root, rows)))
            {
                Ok((root, rows)) => {
                    let scopes: std::collections::BTreeSet<_> =
                        rows.into_iter().map(|(_, s)| s).collect();
                    for scope in scopes {
                        kick(app.clone(), root.clone(), scope);
                    }
                }
                Err(e) => log_error(&e),
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}
#[tauri::command]
pub async fn durable_agent_turns() -> Result<Vec<View>, String> {
    tauri::async_runtime::spawn_blocking(list_views)
        .await
        .map_err(error)?
}
fn list_views() -> Result<Vec<View>, String> {
    let root = crate::conversation_store::root()?;
    let db = db(&root)?;
    let mut q = db
        .prepare("SELECT id FROM agent_turns WHERE acknowledged=0 ORDER BY rowid")
        .map_err(error)?;
    let ids = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    ids.iter().map(|id| view(&root, id)).collect()
}
#[tauri::command]
pub async fn durable_agent_turn_ack(request_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || acknowledge(&request_id))
        .await
        .map_err(error)?
}
fn acknowledge(request_id: &str) -> Result<(), String> {
    db(&crate::conversation_store::root()?)?
        .execute(
            "UPDATE agent_turns SET acknowledged=1 WHERE id=? AND status IN ('completed','failed')",
            [request_id],
        )
        .map_err(error)?;
    Ok(())
}

#[cfg(test)]
pub(crate) async fn test_scope<T>(
    root: &Path,
    id: &str,
    future: impl std::future::Future<Output = T>,
) -> T {
    CURRENT
        .scope(
            Context {
                root: root.into(),
                id: id.into(),
                recovering: true,
                stream: Arc::new(Mutex::new((String::new(), Instant::now()))),
            },
            future,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("xnaut-durable-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn input() -> Request {
        Request {
            handle: "nautbot".into(),
            messages: vec![crate::chat::ChatMessage {
                role: "user".into(),
                content: "Inspect my project".into(),
            }],
            repository_context: Some(vec!["/test/project".into()]),
            thread_id: Some("original-thread".into()),
            project_scope: Some("TEST".into()),
        }
    }
    #[test]
    fn submissions_reopen_with_the_same_identity_and_reject_scope_changes() {
        let tmp = Scratch::new();
        let req = input();
        admit(&tmp.0, "request-1", &req).unwrap();
        admit(&tmp.0, "request-1", &req).unwrap();
        assert_eq!(pending(&tmp.0).unwrap().len(), 1);
        let mut foreign = input();
        foreign.project_scope = Some("OTHER".into());
        assert!(admit(&tmp.0, "request-1", &foreign)
            .unwrap_err()
            .contains("different input or scope"));
        settle(&tmp.0, "request-1", &Ok("Saved answer".into())).unwrap();
        admit(&tmp.0, "request-1", &req).unwrap();
        let result = view(&tmp.0, "request-1").unwrap();
        assert_eq!(result.result.as_deref(), Some("Saved answer"));
        assert_eq!(result.thread_id.as_deref(), Some("original-thread"));
        assert!(pending(&tmp.0).unwrap().is_empty());
    }
    #[tokio::test]
    async fn changed_provider_repository_and_tools_fail_before_replay() {
        let tmp = Scratch::new();
        admit(&tmp.0, "one", &input()).unwrap();
        test_scope(&tmp.0, "one", async {
            bind(
                "scope",
                &json!({"endpoint":"original","repo":"/first","tools":["read_repository_file"]}),
            )
            .unwrap();
        })
        .await;
        test_scope(&tmp.0, "one", async {
            assert!(bind(
                "scope",
                &json!({"endpoint":"other","repo":"/first","tools":["read_repository_file"]})
            )
            .is_err());
            assert!(bind(
                "scope",
                &json!({"endpoint":"original","repo":"/other","tools":["read_repository_file"]})
            )
            .is_err());
            assert!(bind(
                "scope",
                &json!({"endpoint":"original","repo":"/first","tools":["delete_repository"]})
            )
            .is_err());
        })
        .await;
    }
    #[test]
    fn committed_effect_results_are_immutable_and_uncertain_writes_are_not_repeated() {
        let tmp = Scratch::new();
        let input = json!({"name":"update_ticket","args":{"body":"append this once"}});
        assert!(begin_in(&tmp.0, "one", "tool:0", &input, false)
            .unwrap()
            .is_none());
        let interrupted = begin_in(&tmp.0, "one", "tool:0", &input, false)
            .unwrap()
            .unwrap();
        assert_eq!(interrupted["interrupted"], true);
        assert_eq!(
            begin_in(&tmp.0, "one", "tool:0", &input, false)
                .unwrap()
                .unwrap(),
            interrupted
        );
        assert!(finish_in(&tmp.0, "one", "tool:0", &json!({"ok":true})).is_err());
        assert!(begin_in(&tmp.0, "one", "tool:0", &json!({"different":true}), false).is_err());
        assert!(!replay_safe("mcp__read_and_delete"));
        assert!(!replay_safe("start_repository_task"));
        assert!(replay_safe("read_repository_file"));
    }
    #[test]
    fn failed_result_commit_leaves_recoverable_uncertainty() {
        let tmp = Scratch::new();
        let input = json!({"write":"once"});
        begin_in(&tmp.0, "one", "tool:0", &input, false).unwrap();
        db(&tmp.0).unwrap().execute_batch("CREATE TRIGGER fail_result BEFORE UPDATE OF result ON agent_turn_steps BEGIN SELECT RAISE(ABORT,'simulated disk write failure'); END;").unwrap();
        assert!(finish_in(&tmp.0, "one", "tool:0", &json!({"ok":true})).is_err());
        db(&tmp.0)
            .unwrap()
            .execute_batch("DROP TRIGGER fail_result")
            .unwrap();
        assert_eq!(
            begin_in(&tmp.0, "one", "tool:0", &input, false)
                .unwrap()
                .unwrap()["interrupted"],
            true
        );
    }
    // Invoked in a fresh process by the test below. The parent kills this exact
    // test process, never an installed xNAUT or user agent. The external effect
    // is a real append+fsync outside SQLite, so crash recovery cannot hide a
    // duplicate behind a mocked return value.
    #[test]
    fn crash_fixture() {
        let Some(root) = std::env::var_os("XNAUT_DURABLE_CRASH_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let phase = std::env::var("XNAUT_DURABLE_CRASH_PHASE").unwrap();
        let safe = std::env::var("XNAUT_DURABLE_CRASH_SAFE").unwrap() == "yes";
        let req = input();
        admit(&root, "one", &req).unwrap();
        let _owner = lease(&root, &scope("one", &req))
            .unwrap()
            .expect("exclusive owner");
        let stop = |at: &str| {
            if phase == at {
                std::fs::write(root.join("ready"), at).unwrap();
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        };
        stop("before-intent");
        let result = match begin_in(
            &root,
            "one",
            "operation",
            &json!({"destination":"original"}),
            safe,
        )
        .unwrap()
        {
            Some(result) => result,
            None => {
                stop("after-intent");
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(root.join("external-effects"))
                    .unwrap();
                writeln!(file, "effect").unwrap();
                file.sync_all().unwrap();
                stop("after-effect");
                let result = json!({"ok":true,"receipt":"same-external-action"});
                finish_in(&root, "one", "operation", &result).unwrap();
                stop("after-result");
                result
            }
        };
        settle(&root, "one", &Ok(result.to_string())).unwrap();
    }
    #[test]
    fn process_kill_at_every_effect_boundary_preserves_results_and_prevents_duplicate_writes() {
        for (phase, safe, expected, interrupted) in [
            ("before-intent", false, 1, false),
            ("after-intent", false, 0, true),
            ("after-effect", false, 1, true),
            ("after-result", false, 1, false),
            ("after-effect", true, 2, false),
            ("after-result", true, 1, false),
        ] {
            let tmp = Scratch::new();
            let command = |phase: &str| {
                let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
                cmd.args([
                    "--exact",
                    "durable_turn::tests::crash_fixture",
                    "--nocapture",
                ])
                .env("XNAUT_DURABLE_CRASH_ROOT", &tmp.0)
                .env("XNAUT_DURABLE_CRASH_PHASE", phase)
                .env("XNAUT_DURABLE_CRASH_SAFE", if safe { "yes" } else { "no" });
                cmd
            };
            let mut child = command(phase)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !tmp.0.join("ready").exists() {
                if start.elapsed() > Duration::from_secs(20) {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("crash fixture did not reach {phase}");
                }
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "fixture exited before {phase}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(lease(&tmp.0, &scope("one", &input())).unwrap().is_none());
            child.kill().unwrap();
            child.wait().unwrap();
            assert!(command("resume")
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap()
                .success());
            let effects = std::fs::read_to_string(tmp.0.join("external-effects"))
                .unwrap_or_default()
                .lines()
                .count();
            assert_eq!(effects, expected, "phase={phase},safe={safe}");
            let result: Value =
                serde_json::from_str(view(&tmp.0, "one").unwrap().result.as_deref().unwrap())
                    .unwrap();
            assert_eq!(
                result["interrupted"] == true,
                interrupted,
                "phase={phase},safe={safe}"
            );
        }
    }
}
