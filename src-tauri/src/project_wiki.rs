//! XNAUT-455: the project Wiki is a view of canonical Vault Markdown and
//! existing receipts, not a second editable knowledge store. History files
//! are immutable snapshots; optimistic hashes protect edits across app windows.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

pub(crate) mod journal;

const MAX_DOC: u64 = 2 * 1024 * 1024;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn stamp(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|v| v.to_rfc3339())
        .unwrap_or_default()
}
fn human() -> String {
    format!(
        "{} (person)",
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "Local user".into())
    )
}
fn read(path: &Path) -> Result<String, String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_DOC {
        return Err("Document exceeds the 2 MB reading limit".into());
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}
fn atomic(path: &Path, body: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().ok_or("Missing parent")?).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}
#[derive(Clone, Serialize)]
pub struct Project {
    pub key: String,
    pub name: String,
    pub root: String,
    pub purpose: String,
    pub vault_path: String,
}
fn projects() -> Result<Vec<Project>, String> {
    let repo = crate::project_management::repo_now()?;
    let vault = crate::vault_tools::vault_root()?;
    crate::project_management::list_projects(&repo)?
        .into_iter()
        .map(|p| {
            component(&p.name)?;
            Ok(Project {
                key: p.key.clone(),
                root: crate::project_management::local_source_path(&p),
                vault_path: vault.join("work").join(&p.name).to_string_lossy().into(),
                name: p.name,
                purpose: p.purpose,
            })
        })
        .collect()
}
fn component(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.starts_with('.') || value.contains(['/', '\\']) {
        return Err("Invalid project name".into());
    }
    Ok(())
}
fn project(key: &str) -> Result<Project, String> {
    let all = projects()?;
    if let Some(p) = all.iter().find(|p| p.key == key || p.root == key) {
        return Ok(p.clone());
    }
    // A project's ordinary checkout and isolated worktrees share one Git identity.
    fn common(root: &str) -> Option<PathBuf> {
        if !Path::new(root).is_dir() {
            return None;
        }
        let out = std::process::Command::new("git")
            .args([
                "-C",
                root,
                "rev-parse",
                "--path-format=absolute",
                "--git-common-dir",
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
            .canonicalize()
            .ok()
    }
    if let Some(identity) = common(key) {
        if let Some(p) = all
            .into_iter()
            .find(|p| common(&p.root).as_ref() == Some(&identity))
        {
            return Ok(p);
        }
    }
    Err("Select a registered project to open its Wiki".into())
}
// Reject symlink ancestors as well as traversal, including for a new file.
fn safe(root: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() || Path::new(rel).is_absolute() || rel.contains('\\') {
        return Err("Use a project-relative document path".into());
    }
    let mut p = root.to_path_buf();
    for c in root.ancestors() {
        if std::fs::symlink_metadata(c).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Symlinked project folders are not supported".into());
        }
    }
    for c in Path::new(rel).components() {
        let Component::Normal(n) = c else {
            return Err("Document path cannot leave this project".into());
        };
        if n.to_string_lossy().starts_with('.') {
            return Err("Hidden paths are not Wiki documents".into());
        }
        p.push(n);
        if std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Wiki paths cannot follow symlinks".into());
        }
    }
    Ok(p)
}
fn markdown_path(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let p = safe(root, rel)?;
    if p.extension().and_then(|v| v.to_str()) != Some("md") {
        return Err("Wiki pages use Markdown (.md)".into());
    }
    Ok(p)
}
// OS advisory locks release on process exit, including crashes. No stale
// lock-file deletion or PID guessing can let two writers enter together.
struct Lock(std::fs::File);
fn lock(root: &Path) -> Result<Lock, String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    if std::fs::symlink_metadata(root.join(".wiki-history"))
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("Unsafe Wiki history path".into());
    }
    let path = root.join(".wiki-write.lock");
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Unsafe Wiki lock path".into());
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.try_lock()
        .map_err(|_| "Another Wiki save is in progress. Retry after it finishes.".to_string())?;
    Ok(Lock(file))
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Revision {
    pub number: u64,
    pub hash: String,
    pub at: String,
    pub actor: String,
    pub summary: String,
    pub content: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct History {
    id: String,
    created_at: String,
    created_by: String,
    revisions: Vec<Revision>,
}
fn history_path(root: &Path, rel: &str) -> PathBuf {
    root.join(".wiki-history")
        .join(format!("{}.json", hash(rel.as_bytes())))
}
fn front(content: &str, key: &str) -> String {
    if !content.starts_with("---\n") {
        return String::new();
    }
    content[4..]
        .split("\n---")
        .next()
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.strip_prefix(&format!("{key}:")).map(|s| {
                serde_json::from_str::<String>(s.trim()).unwrap_or_else(|_| s.trim().into())
            })
        })
        .unwrap_or_default()
}
fn body(content: &str) -> &str {
    if content.starts_with("---\n") {
        if let Some((_, b)) = content[4..].split_once("\n---") {
            return b.trim_start_matches('\n');
        }
    }
    content
}
fn history(root: &Path, rel: &str, content: &str) -> Result<History, String> {
    let p = history_path(root, rel);
    if std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Unsafe Wiki history file".into());
    }
    if p.exists() {
        serde_json::from_str(&std::fs::read_to_string(&p).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Revision history is unreadable; it was preserved: {e}"))
    } else {
        Ok(History {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: front(content, "Created"),
            created_by: {
                let a = front(content, "Author");
                if a.is_empty() {
                    "Unknown (legacy document)".into()
                } else {
                    a
                }
            },
            revisions: vec![],
        })
    }
}
fn snapshot(h: &mut History, content: &str, actor: &str, summary: &str) {
    let digest = hash(content.as_bytes());
    if h.revisions.last().is_some_and(|r| r.hash == digest) {
        return;
    }
    h.revisions.push(Revision {
        number: h.revisions.len() as u64 + 1,
        hash: digest,
        at: now(),
        actor: actor.into(),
        summary: summary.into(),
        content: content.into(),
    });
}
#[derive(Serialize)]
pub struct Document {
    pub path: String,
    pub title: String,
    pub content: String,
    pub hash: String,
    pub id: String,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
    pub updated_by: String,
    pub revision: u64,
    pub history: Vec<Revision>,
}
fn title(content: &str, rel: &str) -> String {
    body(content)
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or_else(|| {
            rel.rsplit('/')
                .next()
                .unwrap_or(rel)
                .trim_end_matches(".md")
        })
        .to_string()
}
fn doc_in(root: &Path, rel: &str) -> Result<Document, String> {
    let content = read(&markdown_path(root, rel)?)?;
    let _guard = lock(root)?;
    let mut h = history(root, rel, &content)?;
    snapshot(
        &mut h,
        &content,
        "External / legacy editor (not recorded)",
        "Observed Vault content; exact edit time and editor not recorded",
    );
    atomic(
        &history_path(root, rel),
        &serde_json::to_vec(&h).map_err(|e| e.to_string())?,
    )?;
    let last = h.revisions.last().ok_or("No revision")?;
    Ok(Document {
        path: rel.into(),
        title: title(&content, rel),
        hash: hash(content.as_bytes()),
        content,
        id: h.id,
        created_at: h.created_at,
        created_by: h.created_by,
        updated_at: last.at.clone(),
        updated_by: last.actor.clone(),
        revision: last.number,
        history: h.revisions,
    })
}
#[derive(Deserialize)]
pub struct SaveRequest {
    pub project: String,
    pub path: String,
    pub content: String,
    pub expected_hash: Option<String>,
    pub summary: String,
}
fn save_in(
    root: &Path,
    rel: &str,
    content: &str,
    expected: Option<&str>,
    actor: &str,
    summary: &str,
) -> Result<(), String> {
    if content.len() as u64 > MAX_DOC {
        return Err("Page exceeds 2 MB".into());
    }
    if summary.trim().is_empty() {
        return Err("Describe the change before saving".into());
    }
    let guard = lock(root)?;
    save_locked_in(root, rel, content, expected, actor, summary, &guard)
}
// Journal capture holds this same lock across its read/append/write cycle.
// Wiki edits retain their optimistic revision check under the lock.
fn save_locked_in(
    root: &Path, rel: &str, content: &str, expected: Option<&str>,
    actor: &str, summary: &str, _guard: &Lock,
) -> Result<(), String> {
    if content.len() as u64 > MAX_DOC { return Err("Page exceeds 2 MB".into()); }
    let path = markdown_path(root, rel)?;
    let old = if path.exists() {
        Some(read(&path)?)
    } else {
        None
    };
    if old.as_ref().map(|s| hash(s.as_bytes())).as_deref() != expected {
        return Err("This page changed since you opened it. Your draft is preserved. Reload the current page before merging your changes.".into());
    }
    let mut h = history(root, rel, old.as_deref().unwrap_or_default())?;
    if let Some(ref text) = old {
        snapshot(
            &mut h,
            text,
            "External / legacy editor (not recorded)",
            "Content before Wiki edit",
        );
    } else {
        h.created_at = now();
        h.created_by = actor.into();
    }
    let text = if old.is_none() && !content.starts_with("---\n") {
        format!(
            "---\nAuthor: {}\nCreated: {}\n---\n\n{}",
            serde_json::to_string(actor).unwrap(),
            serde_json::to_string(&h.created_at).unwrap(),
            content
        )
    } else {
        content.into()
    };
    atomic(
        &history_path(root, rel),
        &serde_json::to_vec(&h).map_err(|e| e.to_string())?,
    )?;
    atomic(&path, text.as_bytes())?;
    snapshot(&mut h, &text, actor, summary);
    atomic(
        &history_path(root, rel),
        &serde_json::to_vec(&h).map_err(|e| e.to_string())?,
    )
}
fn walk(root: &Path, dir: &Path, out: &mut Vec<Value>) {
    if out.len() >= 20000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if e.file_name().to_string_lossy().starts_with('.')
            || e.file_type().is_ok_and(|t| t.is_symlink())
        {
            continue;
        }
        if p.is_dir() {
            walk(root, &p, out)
        } else if p.extension().is_some_and(|e| e == "md") {
            if let (Ok(rel), Ok(content)) = (p.strip_prefix(root), read(&p)) {
                let rel = rel.to_string_lossy();
                out.push(json!({"path":rel,"title":title(&content,&rel),"section":rel.split('/').nth(1).unwrap_or("Documents")}));
            }
        }
        if out.len() >= 20000 {
            break;
        }
    }
}
#[tauri::command]
pub fn project_wiki_projects() -> Result<Vec<Project>, String> {
    projects()
}
#[tauri::command]
pub fn project_wiki_read(project: String, path: String) -> Result<Document, String> {
    let p = self::project(&project)?;
    doc_in(Path::new(&p.vault_path), &path)
}
#[tauri::command]
pub fn project_wiki_save(request: SaveRequest) -> Result<Document, String> {
    let p = project(&request.project)?;
    let root = Path::new(&p.vault_path);
    save_in(
        root,
        &request.path,
        &request.content,
        request.expected_hash.as_deref(),
        &human(),
        &request.summary,
    )?;
    doc_in(root, &request.path)
}

fn runs(key: &str) -> Vec<crate::run_control::RunManifest> {
    let Ok(dir) = crate::agents::registry_dir() else {
        return vec![];
    };
    crate::run_control::list_ids_in(&dir)
        .unwrap_or_default()
        .iter()
        .filter_map(|id| crate::run_control::load_manifest_in(&dir, id).ok())
        .filter(|r| r.project == key)
        .collect()
}
pub(crate) fn redact(text: &str) -> String {
    static RULES: std::sync::OnceLock<Vec<regex::Regex>> = std::sync::OnceLock::new();
    let rules = RULES.get_or_init(|| {
        [
        r#"(?i)(authorization["']?\s*[:=]\s*["']?(?:bearer\s+|token\s+)?)[^\s"']+"#,
        r#"(?i)((?:api[_-]?key|access[_-]?token|password|secret)["']?\s*[=:]\s*["']?)[^\s,;"']+"#,
        r"\b(?:sk-[a-zA-Z0-9_-]{12,}|gh[pousr]_[a-zA-Z0-9]{15,})\b",
    ].into_iter().map(|p| regex::Regex::new(p).expect("static redaction pattern")).collect()
    });
    let mut out = text.to_string();
    for rule in rules {
        out = rule.replace_all(&out, "${1}[redacted]").into_owned();
    }
    out
}

fn lines(path: &Path, max: usize) -> Vec<Value> {
    let Ok(f) = std::fs::File::open(path) else {
        return vec![];
    };
    // Keep a bounded tail without making the dashboard wait for an unbounded log.
    let mut f = f;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let offset = len.saturating_sub(8 * 1024 * 1024);
    let _ = f.seek(SeekFrom::Start(offset));
    let mut reader = std::io::BufReader::new(f);
    if offset > 0 {
        let mut partial = String::new();
        let _ = reader.read_line(&mut partial);
    }
    let mut rows: std::collections::VecDeque<Value> = std::collections::VecDeque::new();
    for line in reader.lines().map_while(Result::ok) {
        if let Ok(v) = serde_json::from_str(&line) {
            rows.push_back(v);
            if rows.len() > max {
                rows.pop_front();
            }
        }
    }
    rows.into()
}
fn ticket(p: &Project, id: &str) -> Result<Value, String> {
    if !id.starts_with(&format!("{}-", p.key))
        || !id[p.key.len() + 1..].chars().all(|c| c.is_ascii_digit())
    {
        return Err("Ticket is outside this project".into());
    }
    let path = crate::project_management::repo_now()?
        .join("projects")
        .join(&p.key)
        .join("tickets")
        .join(format!("{id}.json"));
    serde_json::from_str(&read(&path)?).map_err(|e| e.to_string())
}
fn handoff_body(run: &crate::run_control::RunManifest, t: &Value) -> String {
    let state = serde_json::to_value(run.state).unwrap_or_default();
    let evidence = t
        .get("handback")
        .filter(|v| v["run_id"].as_str() == Some(run.run_id.as_str()));
    format!("# Handoff · {}\n\n> Factual checkpoint from saved run records. Runtime state is not proof that implementation is complete.\n\n## Goal\n{}\n\n## People and execution\n- Initiator: {}\n- Agent: {}\n- Runtime: {}\n- Model: {}\n- Run: `{}`\n- Ticket: {}\n- Started: {}\n- Last observed: {}\n- Runtime state: {}\n- Last signal: {}\n\n## Changes and evidence\n- Branch: `{}`\n- Last recorded commit: `{}`\n- Worktree: `{}`\n\n{}\n\n## Where to continue\n{}\n\nRead the linked run, ticket and conversation evidence in the project Wiki before resuming. A failed or cancelled runtime can still have commits or a PR.\n",
        run.ticket.as_deref().unwrap_or(&run.run_id),t["title"].as_str().unwrap_or("Scope not recorded on a linked ticket"),
        if run.initiated_by.is_empty(){"Not recorded in this legacy run"}else{&run.initiated_by},run.agent_handle,run.runtime_id,run.model.as_deref().unwrap_or("Not recorded"),run.run_id,run.ticket.as_deref().unwrap_or("Not recorded"),stamp(run.started_at),stamp(run.last_seen_at),state.as_str().unwrap_or("unknown"),redact(&run.last_signal),run.branch,run.last_commit,run.worktree_path,
        evidence.map(|e|format!("### Saved handback\n```json\n{}\n```",redact(&serde_json::to_string_pretty(e).unwrap_or_default()))).unwrap_or_else(||"No structured handback is linked to this exact run. Summary pending: changes, tests and remaining requirements need verification. This factual checkpoint remains available without a summary service.".into()),
        run.waiting_on.as_deref().unwrap_or("Check the recorded outcome and unresolved ticket requirements; preserve any existing implementation."))
}
pub(crate) fn capture_stop(run: &crate::run_control::RunManifest) -> Result<(), String> {
    use crate::run_control::RunState;
    if !run.state.terminal() && run.state != RunState::Blocked {
        return Ok(());
    }
    let p = project(&run.project)?;
    capture_in(
        Path::new(&p.vault_path),
        run,
        &run.ticket
            .as_ref()
            .and_then(|id| ticket(&p, id).ok())
            .unwrap_or(Value::Null),
    )
}
fn capture_in(root: &Path, run: &crate::run_control::RunManifest, t: &Value) -> Result<(), String> {
    let rel = format!(
        "Development/handoffs/{}-{}.md",
        run.run_id,
        serde_json::to_value(run.state)
            .unwrap()
            .as_str()
            .unwrap_or("stopped")
    );
    let path = markdown_path(root, &rel)?;
    // Keep the original checkpoint. A later, exactly bound handback is a new
    // evidence page, never an overwrite of the original or a human revision.
    if path.exists() {
        if let Some(h) = t
            .get("handback")
            .filter(|v| v["run_id"].as_str() == Some(run.run_id.as_str()))
        {
            let encoded = redact(&serde_json::to_string_pretty(h).unwrap_or_default());
            if !read(&path)?.contains(&encoded) {
                let followup = format!(
                    "Development/handoffs/{}-handback-{}.md",
                    run.run_id,
                    &hash(encoded.as_bytes())[..16]
                );
                if !markdown_path(root, &followup)?.exists() {
                    save_in(
                        root,
                        &followup,
                        &handoff_body(run, t),
                        None,
                        "xNAUT recorded checkpoint",
                        "Link later structured handback to its exact run",
                    )?;
                }
            }
        }
        return Ok(());
    }
    save_in(
        root,
        &rel,
        &handoff_body(run, t),
        None,
        "xNAUT recorded checkpoint",
        "Preserve stopped work and its evidence",
    )
}
#[tauri::command]
pub async fn project_wiki_overview(project: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || overview(project))
        .await
        .map_err(|e| e.to_string())?
}
fn overview(project: String) -> Result<Value, String> {
    let p = self::project(&project)?;
    let root = Path::new(&p.vault_path);
    let mut warnings = Vec::new();
    let mut run_rows = runs(&p.key);
    run_rows.sort_by_key(|r| std::cmp::Reverse(r.started_at));
    for run in &run_rows {
        if let Err(e) = capture_stop(run) {
            warnings.push(format!("Handoff {}: {e}", run.run_id));
        }
    }
    let mut docs = Vec::new();
    walk(root, root, &mut docs);
    docs.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
    if docs.len() >= 20000 {
        warnings.push("Showing the first 20,000 Vault documents".into());
    }
    let mut activity = Vec::new();
    for document in &docs {
        if let Some(path) = document["path"].as_str() {
            let hp = history_path(root, path);
            if !std::fs::symlink_metadata(&hp).is_ok_and(|m| m.file_type().is_symlink()) {
                if let Ok(h) = std::fs::read_to_string(hp).and_then(|s| {
                    serde_json::from_str::<History>(&s).map_err(std::io::Error::other)
                }) {
                    for r in h.revisions {
                        activity.push(json!({"id":format!("document:{}:{}",h.id,r.number),"at":r.at,"kind":"document.revised","actor":r.actor,"path":path,"detail":r.summary,"source":"document"}));
                    }
                }
            }
        }
    }
    if let Ok(repo) = crate::project_management::repo_now() {
        if let Ok(entries) = std::fs::read_dir(repo.join("events")) {
            for entry in entries.flatten() {
                if let Ok(v) = read(&entry.path())
                    .and_then(|s| serde_json::from_str::<Value>(&s).map_err(|e| e.to_string()))
                {
                    let subject = v["subject"].as_str().unwrap_or_default();
                    if subject == p.key || subject.starts_with(&format!("{}-", p.key)) {
                        activity.push(json!({"id":entry.file_name().to_string_lossy(),"at":v["timestamp"],"kind":v["event"],"actor":v["details"]["caller"].as_str().or(v["details"]["actor"].as_str()).unwrap_or("Not recorded"),"ticket":subject,"detail":redact(&v["details"].to_string()),"source":"ticket"}));
                    }
                }
            }
        }
    }
    for v in lines(&crate::ledger::path(), 10000) {
        if v["ticket"]
            .as_str()
            .is_some_and(|s| s.starts_with(&format!("{}-", p.key)))
            || run_rows
                .iter()
                .any(|r| Some(r.run_id.as_str()) == v["run_id"].as_str())
        {
            activity.push(json!({"id":hash(v.to_string().as_bytes()),"at":v["at"],"kind":v["kind"],"actor":v["agent"],"ticket":v["ticket"],"run_id":v["run_id"],"detail":redact(v["detail"].as_str().unwrap_or_default()),"source":"run"}));
        }
    }
    for r in &run_rows {
        activity.push(json!({"id":format!("run:{}",r.run_id),"at":stamp(r.last_seen_at),"kind":format!("run {}",serde_json::to_value(r.state).unwrap().as_str().unwrap()),"actor":r.agent_handle,"ticket":r.ticket,"run_id":r.run_id,"detail":redact(&r.last_signal),"source":"run"}));
    }
    activity.sort_by(|a, b| b["at"].as_str().cmp(&a["at"].as_str()));
    let total = activity.len();
    activity.truncate(1000);
    Ok(
        json!({"project":p,"documents":docs,"runs":run_rows,"activity":activity,"activity_total":total,"observed_at":now(),"warnings":warnings}),
    )
}
#[tauri::command]
pub fn project_wiki_source(project: String, kind: String, id: String) -> Result<Value, String> {
    let p = self::project(&project)?;
    if kind == "ticket" {
        return Ok(
            json!({"title":id,"text":redact(&serde_json::to_string_pretty(&ticket(&p,&id)?).unwrap_or_default())}),
        );
    }
    let run = runs(&p.key)
        .into_iter()
        .find(|r| r.run_id == id)
        .ok_or("Run is unavailable or belongs to another project")?;
    if kind == "context" {
        return context_for_run(&run);
    }
    if kind == "log" {
        let path = run
            .output_path
            .ok_or("No output log was recorded for this run")?;
        let mut file = std::fs::File::open(&path)
            .map_err(|_| "The saved output log is no longer available")?;
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        let from = len.saturating_sub(128 * 1024);
        file.seek(SeekFrom::Start(from))
            .map_err(|e| e.to_string())?;
        let mut text = Vec::new();
        file.take(128 * 1024)
            .read_to_end(&mut text)
            .map_err(|e| e.to_string())?;
        return Ok(
            json!({"title":format!("Run log · {}",id),"text":redact(&String::from_utf8_lossy(&text)),"truncated":from>0}),
        );
    }
    Ok(
        json!({"title":format!("Run · {}",id),"text":redact(&serde_json::to_string_pretty(&run).unwrap_or_default())}),
    )
}
fn context_for_run(run: &crate::run_control::RunManifest) -> Result<Value, String> {
    let db = crate::conversation_store::root()?.join("conversations.sqlite");
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let raw: String = conn
        .query_row(
            "SELECT value FROM conversations WHERE key='xnaut-agent-threads:v1'",
            [],
            |r| r.get(0),
        )
        .map_err(|_| "Saved conversation context is unavailable")?;
    let all: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    for (agent, threads) in all.as_object().into_iter().flatten() {
        for thread in threads.as_array().into_iter().flatten() {
            let messages = thread["messages"].as_array().cloned().unwrap_or_default();
            for (i, m) in messages.iter().enumerate() {
                let receipt = &m["executionReceipt"];
                if receipt.to_string().contains(&run.run_id)
                    && receipt["launch"]["run_id"]
                        .as_str()
                        .or(receipt["run_id"].as_str())
                        == Some(&run.run_id)
                {
                    let scope = receipt["task"]
                        .as_str()
                        .unwrap_or("Task scope not recorded");
                    return Ok(
                        json!({"title":format!("Saved context · {}",thread["title"].as_str().unwrap_or("Agent conversation")),"text":redact(&format!("Agent: {agent}\nThread: {}\nMessage: {i}\n\n{scope}\n\nConversation through launch:\n{}\n\nLaunch receipt:\n{}",thread["id"],serde_json::to_string_pretty(&messages[..=i]).unwrap_or_default(),serde_json::to_string_pretty(receipt).unwrap_or_default())),"thread_id":thread["id"],"agent":agent,"message_index":i}),
                    );
                }
            }
        }
    }
    Err("No exact saved conversation receipt is linked to this run; attribution has not been guessed".into())
}
// New stop events are written immediately; this loop also reconciles crashes
// and older run records without depending on an open Wiki window.
pub(crate) fn reconcile() {
    if let Ok(ps) = projects() {
        for p in ps {
            for r in runs(&p.key) {
                let _ = capture_stop(&r);
            }
        }
    }
}

pub(crate) fn agent_tool(name: &str, args: &Value, actor: &str) -> Value {
    let result = (|| -> Result<Value, String> {
        let key = args["project"]
            .as_str()
            .ok_or("A project is required")?
            .to_string();
        match name {
            "project_wiki_journal_read" | "project_wiki_journal_append" => journal::tool(name, args, actor),
            "project_wiki_list" => overview(key),
            "project_wiki_read" => serde_json::to_value(project_wiki_read(
                key,
                args["path"].as_str().ok_or("A path is required")?.into(),
            )?)
            .map_err(|e| e.to_string()),
            "project_wiki_write" => {
                let request: SaveRequest =
                    serde_json::from_value(args.clone()).map_err(|e| e.to_string())?;
                let p = project(&key)?;
                let ticket_id = args["ticket"]
                    .as_str()
                    .ok_or("An existing project ticket is required for Agent Wiki edits")?;
                ticket(&p, ticket_id)?;
                save_in(
                    Path::new(&p.vault_path),
                    &request.path,
                    &request.content,
                    request.expected_hash.as_deref(),
                    actor,
                    &request.summary,
                )?;
                Ok(
                    json!({"ok":true,"path":request.path,"note":"Saved to the project Vault with revision history"}),
                )
            }
            _ => Err("Unknown Wiki tool".into()),
        }
    })();
    result.unwrap_or_else(|e| json!({"ok":false,"error":e}))
}
/// Local feature build: same renderer and commands, no production boot loops,
/// session restore, mobile bridge, scheduler, updater or worker dispatch.
pub(crate) fn preview() {
    use tauri::Manager;
    let app = tauri::Builder::default()
        .setup(|app| {
            let window = match app.get_webview_window("main") {
                Some(window) => window,
                None => tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("wiki-preview.html".into()),
                )
                .title("xNAUT · Wiki Preview")
                .inner_size(1400.0, 900.0)
                .build()?,
            };
            window.show()?;
            window.set_focus()?;
            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            project_wiki_projects,
            project_wiki_overview,
            project_wiki_read,
            project_wiki_save,
            project_wiki_source,
            project_wiki_asset,
            project_wiki_attach
        ])
        .build(tauri::generate_context!())
        .expect("Wiki preview failed to start");
    app.run(|_, _| {});
}

#[tauri::command]
pub fn project_wiki_asset(project: String, path: String) -> Result<String, String> {
    let p = self::project(&project)?;
    let root = Path::new(&p.vault_path);
    let file = safe(root, &path)?;
    let canonical = file
        .canonicalize()
        .map_err(|_| "Attachment is unavailable")?;
    if !canonical.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
        return Err("Attachment is outside the project Vault".into());
    }
    if !["png", "jpg", "jpeg", "gif", "webp", "mp4", "webm"].contains(
        &file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default(),
    ) {
        return Err("Unsupported media type".into());
    }
    Ok(canonical.to_string_lossy().into())
}
#[tauri::command]
pub fn project_wiki_attach(project: String, name: String, bytes: Vec<u8>) -> Result<Value, String> {
    if bytes.is_empty() || bytes.len() > 10 * 1024 * 1024 {
        return Err("Attachments must be between 1 byte and 10 MB".into());
    }
    let ext = Path::new(&name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let valid = match ext.as_str() {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "gif" => bytes.starts_with(b"GIF8"),
        "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        "mp4" => bytes.get(4..8) == Some(b"ftyp"),
        "webm" => bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]),
        _ => false,
    };
    if !valid {
        return Err("The file does not match a supported image/video format".into());
    }
    let p = self::project(&project)?;
    let rel = format!("Development/wiki/assets/{}.{}", uuid::Uuid::new_v4(), ext);
    let file = safe(Path::new(&p.vault_path), &rel)?;
    atomic(&file, &bytes)?;
    Ok(json!({"path":rel}))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("xnaut-wiki-455-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p.canonicalize().unwrap())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn manual_and_agent_edits_keep_creator_and_every_revision() {
        let f = Fixture::new();
        let rel = "Development/wiki/recon.md";
        save_in(
            &f.0,
            rel,
            "# Recon\n\nHuman observation",
            None,
            "Andre",
            "Initial recon",
        )
        .unwrap();
        let first = doc_in(&f.0, rel).unwrap();
        assert_eq!(first.created_by, "Andre");
        assert_eq!(first.history.len(), 1);
        save_in(
            &f.0,
            rel,
            &format!("{}\n\nAgent evidence", first.content),
            Some(&first.hash),
            "NautBot",
            "Append evidence",
        )
        .unwrap();
        let second = doc_in(&f.0, rel).unwrap();
        assert_eq!(second.created_by, "Andre");
        assert_eq!(second.created_at, first.created_at);
        assert_eq!(second.id, first.id);
        assert_eq!(second.updated_by, "NautBot");
        assert_eq!(second.history.len(), 2);
        assert!(second.content.contains("Human observation"));
        assert!(second.history[0].content.contains("Human observation"));
    }
    #[test]
    fn external_vault_edits_are_visible_and_stale_writers_refused() {
        let f = Fixture::new();
        let rel = "notes.md";
        save_in(&f.0, rel, "# Original", None, "Person", "Create").unwrap();
        let first = doc_in(&f.0, rel).unwrap();
        std::fs::write(f.0.join(rel), "# Edited in Vault\nA human change").unwrap();
        assert!(save_in(
            &f.0,
            rel,
            "# Agent overwrite",
            Some(&first.hash),
            "Agent",
            "Overwrite"
        )
        .unwrap_err()
        .contains("changed since"));
        let current = doc_in(&f.0, rel).unwrap();
        assert!(current.content.contains("human change"));
        assert_eq!(current.created_by, "Person");
        assert_eq!(current.history.len(), 2);
        assert!(current.updated_by.contains("not recorded"));
    }
    #[test]
    fn paths_cannot_escape_project_or_touch_hidden_storage() {
        let f = Fixture::new();
        for p in [
            "../other.md",
            "/tmp/other.md",
            "a/../../other.md",
            ".wiki-history/a.md",
            "a\\other.md",
        ] {
            assert!(markdown_path(&f.0, p).is_err(), "{p}");
        }
        assert!(markdown_path(&f.0, "page.txt").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_documents_and_parent_folders_are_refused() {
        let f = Fixture::new();
        let other = Fixture::new();
        std::os::unix::fs::symlink(&other.0, f.0.join("outside")).unwrap();
        assert!(save_in(&f.0, "outside/a.md", "bad", None, "A", "Create").is_err());
        assert!(!other.0.join("a.md").exists());
        std::os::unix::fs::symlink(&other.0, f.0.join(".wiki-history")).unwrap();
        assert!(save_in(&f.0, "a.md", "bad", None, "A", "Create").is_err());
    }
    #[test]
    fn locks_release_and_duplicate_creates_do_not_overwrite() {
        let f = Fixture::new();
        {
            let _lock = lock(&f.0).unwrap();
            assert!(lock(&f.0).is_err());
        }
        assert!(lock(&f.0).is_ok());
        save_in(&f.0, "a.md", "# First", None, "A", "Create").unwrap();
        assert!(save_in(&f.0, "a.md", "# Second", None, "B", "Create").is_err());
        assert!(read(&f.0.join("a.md")).unwrap().contains("First"));
    }
    #[test]
    fn identical_retry_does_not_invent_an_extra_revision() {
        let f = Fixture::new();
        save_in(&f.0, "a.md", "# A", None, "A", "Create").unwrap();
        let one = doc_in(&f.0, "a.md").unwrap();
        let two = doc_in(&f.0, "a.md").unwrap();
        assert_eq!(one.revision, two.revision);
        assert_eq!(one.updated_at, two.updated_at);
    }
    #[test]
    fn legacy_import_does_not_invent_creation_time() {
        let f = Fixture::new();
        std::fs::write(f.0.join("a.md"), "# Old notes").unwrap();
        let d = doc_in(&f.0, "a.md").unwrap();
        assert!(d.created_at.is_empty());
        assert!(d.created_by.contains("Unknown"));
    }
    #[test]
    fn cancelled_failed_and_completed_runs_get_durable_truthful_handoffs() {
        let f = Fixture::new();
        let mut run = crate::run_control::tests::run();
        run.state = crate::run_control::RunState::Failed;
        run.last_signal = "cancelled by owner".into();
        run.last_commit = "abc123".into();
        run.initiated_by = "Andre".into();
        let t = json!({"title":"Upgrade reviewer","handback":{"summary":"Code written, validation incomplete","checks":[]}});
        capture_in(&f.0, &run, &t).unwrap();
        capture_in(&f.0, &run, &t).unwrap();
        let mut docs = vec![];
        walk(&f.0, &f.0, &mut docs);
        assert_eq!(docs.len(), 1);
        let d = doc_in(&f.0, docs[0]["path"].as_str().unwrap()).unwrap();
        assert!(d.content.contains("abc123"));
        assert!(d.content.contains("cancelled by owner"));
        assert!(d.content.contains("not proof"));
        assert!(d.content.contains("Andre"));
        assert_eq!(d.revision, 1);
        run.state = crate::run_control::RunState::Done;
        capture_in(&f.0, &run, &Value::Null).unwrap();
        docs.clear();
        walk(&f.0, &f.0, &mut docs);
        assert_eq!(docs.len(), 2);
    }
    #[test]
    fn handbacks_are_bound_to_exact_runs_and_late_evidence_is_preserved_once() {
        let f = Fixture::new();
        let mut run = crate::run_control::tests::run();
        run.state = crate::run_control::RunState::Done;
        let wrong = json!({"handback":{"run_id":"another-run","summary":"Unrelated success"}});
        assert!(!handoff_body(&run, &wrong).contains("Unrelated success"));
        capture_in(&f.0, &run, &wrong).unwrap();
        let right = json!({"handback":{"run_id":run.run_id,"summary":"Actual outcome"}});
        capture_in(&f.0, &run, &right).unwrap();
        capture_in(&f.0, &run, &right).unwrap();
        let mut docs = vec![];
        walk(&f.0, &f.0, &mut docs);
        assert_eq!(docs.len(), 2);
    }
    #[test]
    fn secrets_are_removed_from_displayed_logs() {
        let s =
            redact("Authorization: Bearer abc123\napi_key=secret-value\npassword=pwd\ncommit=abc");
        assert!(!s.contains("abc123"));
        assert!(!s.contains("secret-value"));
        assert!(!s.contains("pwd"));
        assert!(s.contains("commit=abc"));
    }
    #[test]
    fn verification_dates_remain_unchanged_on_edit() {
        let f = Fixture::new();
        save_in(
            &f.0,
            "a.md",
            "# Findings\nVerified: 2026-09-01",
            None,
            "A",
            "Create",
        )
        .unwrap();
        let a = doc_in(&f.0, "a.md").unwrap();
        save_in(
            &f.0,
            "a.md",
            &(a.content + "\nAnother note"),
            Some(&a.hash),
            "B",
            "Add note",
        )
        .unwrap();
        assert!(doc_in(&f.0, "a.md")
            .unwrap()
            .content
            .contains("Verified: 2026-09-01"));
    }
}
