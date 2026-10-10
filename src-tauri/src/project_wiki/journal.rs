//! XNAUT-455: continuously saved working documents. Capture is native and
//! independent of which project's pane is visible. Markdown in the Vault is
//! canonical; deterministic entry IDs and Wiki CAS protect concurrent writers.
use super::*;
mod activity;
mod console;
const MARK: &str = "<!-- xnaut-journal-entry ";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub at: String,
    pub actor: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub ticket: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub source_at: String,
    #[serde(default)]
    pub content: String,
}
#[derive(Deserialize)]
pub struct Request {
    pub project: String,
    pub ticket: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub source_id: String,
}
fn day(at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(at)
        .map(|d| d.with_timezone(&chrono::Utc).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| chrono::Utc::now().format("%Y-%m-%d").to_string())
}
fn path(at: &str) -> String {
    format!("Development/journal/{}.md", day(at))
}
fn entries(text: &str) -> Vec<Entry> {
    text.split(MARK)
        .skip(1)
        .filter_map(|block| {
            let (meta, body) = block.split_once(" -->\n")?;
            let mut e: Entry = serde_json::from_str(meta).ok()?;
            e.content = body.trim().to_string();
            Some(e)
        })
        .collect()
}
fn append_in(root: &Path, name: &str, e: &Entry, context: &str) -> Result<(), String> {
    let rel = path(&e.at);
    for _ in 0..8 {
        let target = markdown_path(root, &rel)?;
        let old = if target.exists() {
            Some(read(&target)?)
        } else {
            None
        };
        if old
            .as_ref()
            .is_some_and(|s| entries(s).iter().any(|x| x.id == e.id))
        {
            return Ok(());
        }
        let mut text=old.clone().unwrap_or_else(||format!("# {name} · Live Journal · {}\n\n## Where we stand\n\n{context}\n\n## Working notes\n\n",day(&e.at)));
        let mut meta = e.clone();
        meta.content.clear();
        let content = redact(&e.content).replace(MARK, "&lt;!-- xnaut-journal-entry ");
        text.push_str(&format!(
            "\n{MARK}{} -->\n### {}\n\n*{} · {} · {}*\n\n{}\n\n",
            serde_json::to_string(&meta)
                .map_err(|e| e.to_string())?
                .replace('<', "\\u003c"),
            e.title.replace(['\n', '\r'], " "),
            e.at,
            e.actor,
            e.kind,
            content
        ));
        match save_in(
            root,
            &rel,
            &text,
            old.as_ref().map(|s| hash(s.as_bytes())).as_deref(),
            &e.actor,
            "Capture Live Journal entry",
        ) {
            Ok(()) => return Ok(()),
            Err(err)
                if err.contains("changed since")
                    || err.contains("busy")
                    || err.contains("locked")
                    || err.contains("save is in progress") =>
            {
                std::thread::sleep(std::time::Duration::from_millis(30))
            }
            Err(err) => return Err(err),
        }
    }
    Err("Journal is busy; the source remains saved and capture will retry.".into())
}
fn historical(p: &Project) -> Vec<Value> {
    let root = Path::new(&p.vault_path);
    let mut docs = Vec::new();
    if let Ok(dir) = safe(root, "Development/handoffs") {
        walk(root, &dir, &mut docs);
    }
    docs.sort_by(|a, b| b["path"].as_str().cmp(&a["path"].as_str()));
    let mut seen = std::collections::HashSet::new();
    docs.into_iter().filter(|v|seen.insert(v["title"].as_str().unwrap_or("").to_string())).take(3).filter_map(|v| {
        let rel=v["path"].as_str()?;
        let d=doc_in(root,rel).ok()?;
        Some(json!({"title":d.title,"path":rel,"content":body(&d.content),"updated_at":d.updated_at}))
    }).collect()
}
fn excerpt(text: &str, limit: usize) -> String {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= limit {
        return clean;
    }
    let short = clean.chars().take(limit).collect::<String>();
    format!(
        "{}…",
        short.rsplit_once(' ').map(|(s, _)| s).unwrap_or(&short)
    )
}
fn signal_summary(signal: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(signal.trim()) else {
        return excerpt(signal, 240);
    };
    let review = &value["review"];
    let mut parts = Vec::new();
    if let Some(decision) = review["decision"].as_str().or(value["decision"].as_str()) {
        parts.push(format!("Review: {}.", decision.replace('_', " ")));
    }
    if let Some(summary) = value["summary"].as_str().or(value["outcome"].as_str()) {
        parts.push(excerpt(summary, 220));
    }
    if let Some(reason) = review["reasons"]
        .as_array()
        .and_then(|v| v.first())
        .and_then(Value::as_str)
        .or(value["reason"].as_str())
    {
        parts.push(format!("Finding: {}", excerpt(reason, 220)));
    }
    if parts.is_empty() {
        "A structured result was recorded. Open the evidence for details.".into()
    } else {
        parts.join(" ")
    }
}
fn run_preview(e: &Entry) -> String {
    let section = |heading: &str| {
        e.content
            .split_once(&format!("## {heading}\n"))
            .map(|(_, s)| s.split("\n## ").next().unwrap_or(s).trim())
            .unwrap_or("")
    };
    let goal = section("Goal");
    let signal = e
        .content
        .lines()
        .find_map(|l| l.strip_prefix("- Last signal: "))
        .unwrap_or("");
    let next = section("Where to continue")
        .split("\n\nRead the linked")
        .next()
        .unwrap_or("");
    format!(
        "### {}\n\n{}\n\n{}\n\n{}",
        e.title,
        excerpt(goal, 260),
        signal_summary(signal),
        if next.is_empty() {
            String::new()
        } else {
            format!("**Next:** {}", excerpt(next, 220))
        }
    )
}
fn context(p: &Project) -> String {
    let root = Path::new(&p.vault_path);
    let mut out=format!("Saved context assembled {}. Earlier reports retain their original dates and are not fresh verification.\n\n",now());
    // Carry the previous working document forward, including human decisions.
    // The existing day's opening is immutable once saved; refresh never rewrites it.
    let previous = documents(p)
        .into_iter()
        .find(|d| d["path"].as_str() != Some(path(&now()).as_str()));
    if let Some(d) = previous {
        let rel = d["path"].as_str().unwrap_or("");
        if let Ok(text) = markdown_path(root, rel).and_then(|path| read(&path)) {
            let selected: Vec<_> = entries(&text)
                .into_iter()
                .rev()
                .filter(|e| {
                    e.run_id.is_empty()
                        && ["summary", "decision", "question"].contains(&e.kind.as_str())
                })
                .take(3)
                .collect();
            out.push_str(&format!("### [Previous Journal](../../{rel})\n\n"));
            if selected.is_empty() {
                out.push_str(
                    "The previous working notes remain available in the linked Journal.\n\n",
                );
            }
            for e in selected.into_iter().rev() {
                out.push_str(&format!(
                    "**{} · {} · {}**\n\n{}\n\n",
                    e.kind,
                    e.actor,
                    e.at,
                    e.content.chars().take(2200).collect::<String>()
                ));
            }
        }
    }
    let prior = historical(p);
    if prior.is_empty() {
        out.push_str("No previous run handoff is recorded. Establish the current state before making changes.");
    }
    for d in prior {
        let content = d["content"].as_str().unwrap_or("");
        let section = |heading: &str| {
            content
                .split_once(&format!("## {heading}\n"))
                .map(|(_, s)| s.split("\n## ").next().unwrap_or(s).trim())
                .unwrap_or("")
        };
        let goal = section("Goal");
        let state = content
            .lines()
            .find_map(|l| l.strip_prefix("- Runtime state: "))
            .unwrap_or("See saved evidence");
        let signal = content
            .lines()
            .find_map(|l| l.strip_prefix("- Last signal: "))
            .unwrap_or("");
        let next = section("Where to continue")
            .split("\n\nRead the linked")
            .next()
            .unwrap_or("");
        out.push_str(&format!(
            "### [{}](../../{})\n\n{}\n\n**Execution:** {}. {}\n\n**Next:** {}\n\n",
            d["title"].as_str().unwrap_or("Handoff"),
            d["path"].as_str().unwrap_or(""),
            goal,
            state,
            signal_summary(signal),
            excerpt(next, 220)
        ));
    }
    out
}

fn append(p: &Project, e: &Entry) -> Result<(), String> {
    let root = Path::new(&p.vault_path);
    let ctx = if markdown_path(root, &path(&e.at))?.exists() {
        String::new()
    } else {
        context(p)
    };
    append_in(root, &p.name, e, &ctx)
}
fn documents(p: &Project) -> Vec<Value> {
    let root = Path::new(&p.vault_path);
    let mut docs = Vec::new();
    if let Ok(dir) = safe(root, "Development/journal") {
        walk(root, &dir, &mut docs);
    }
    docs.sort_by(|a, b| b["path"].as_str().cmp(&a["path"].as_str()));
    docs
}
/// A small owner-facing view of existing durable groups. Approval scope and
/// ticket bodies remain in the native coordination store, not the Journal UI.
fn group_views(registry: &Path, project: &str) -> Result<Value, String> {
    use crate::swarm_plan::MemberState;
    let groups = crate::swarm_plan::groups_in(registry, Some(project))?;
    let scoped = |ticket: &str| {
        ticket
            .strip_prefix(&format!("{project}-"))
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    if groups.iter().any(|g| {
        g.members.iter().any(|m| !scoped(&m.ticket))
            || g.plan.runs.iter().any(|r| !scoped(&r.ticket))
    }) {
        return Err("A saved group includes tickets outside this project; inspect its scope before stopping it.".into());
    }
    Ok(Value::Array(groups.into_iter().filter(|g| g.approved_at.is_some()).map(|g| {
        let count=|state: fn(&MemberState)->bool| g.members.iter().filter(|m|state(&m.state)).count();
        json!({"id":g.plan.id,"project":g.plan.project,"approved_at":g.approved_at,"stopped_at":g.stopped_at,
            "counts":{"queued":count(|s|matches!(s,MemberState::Queued)),"running":count(|s|matches!(s,MemberState::Starting|MemberState::Tracking)),
                "blocked":count(|s|matches!(s,MemberState::Blocked)),"verified":count(|s|matches!(s,MemberState::Verified))},
            "members":g.members.iter().map(|m|json!({"ticket":m.ticket,"state":m.state,"run_id":m.run_id,"reason":redact(&m.reason)})).collect::<Vec<_>>()})
    }).collect()))
}
pub fn read_journal(key: &str, selected: Option<&str>) -> Result<Value, String> {
    read_journal_page(key, selected, None)
}
fn read_journal_page(key: &str, selected: Option<&str>, action_before: Option<usize>) -> Result<Value, String> {
    let p = project(key)?;
    let docs = documents(&p);
    let rel = selected.map(String::from).unwrap_or_else(|| path(&now()));
    if !rel.starts_with("Development/journal/") {
        return Err("Select a Journal document".into());
    }
    let target = markdown_path(Path::new(&p.vault_path), &rel)?;
    let text = if target.exists() {
        read(&target)?
    } else {
        String::new()
    };
    let rows: Vec<_> = entries(&text)
        .into_iter()
        .map(|e| {
            let mut value = serde_json::to_value(&e).unwrap();
            if !e.run_id.is_empty() {
                value["preview"] = json!(if e.id.starts_with("activity:") {
                    format!(
                        "### {}\n\n{}",
                        e.title,
                        e.content
                            .split("\n\n[Source evidence]")
                            .next()
                            .unwrap_or(&e.content)
                    )
                } else {
                    run_preview(&e)
                });
            }
            value
        })
        .collect();
    let opening = if text.is_empty() {
        context(&p)
    } else {
        text.split(MARK)
            .next()
            .unwrap_or("")
            .split_once("## Where we stand\n\n")
            .map(|(_, s)| s.split("## Working notes").next().unwrap_or(s))
            .unwrap_or("")
            .to_string()
    };
    // Older preview documents retain their evidence, but do not dump a JSON
    // receipt into the human-readable opening when displaying that document.
    let opening = opening
        .lines()
        .map(|line| {
            if line.starts_with("**Recorded outcome:**") || line.starts_with("**Execution:**") {
                if let Some(index) = line.find('{') {
                    return format!("{}{}", &line[..index], signal_summary(&line[index..]));
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let all_runs = runs(&p.key);
    let console = console::read(&p, &rel, &all_runs, action_before);
    let current: Vec<_> = all_runs
        .into_iter()
        .filter(|r| !r.state.terminal())
        .collect();
    // Reconcile on every read, including reopen and historical date selection.
    // This is a read-only projection, never a replacement for the saved opening.
    let (continuity, continuity_error) = match crate::project_continuity::snapshot(&p.key) {
        Ok(snapshot) => (
            serde_json::to_value(snapshot).map_err(|e| e.to_string())?,
            None,
        ),
        Err(error) => (Value::Null, Some(error)),
    };
    let (groups, groups_error) =
        match crate::agents::registry_dir().and_then(|dir| group_views(&dir, &p.key)) {
            Ok(groups) => (groups, None::<String>),
            Err(_) => (
                json!([]),
                Some("Approved groups are unavailable; refresh to retry.".into()),
            ),
        };
    Ok(
        json!({"console":console,"groups":groups,"groups_error":groups_error,"project":p,"path":rel,"documents":docs,"opening":opening,"entries":rows,"runs":current,"continuity":continuity,"continuity_error":continuity_error,"observed_at":now(),"warning":CAPTURE_WARNING.lock().map(|s|s.clone()).unwrap_or_default()}),
    )
}
#[tauri::command]
pub async fn project_journal_read(project: String, path: Option<String>, action_before: Option<usize>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || read_journal_page(&project, path.as_deref(), action_before))
        .await
        .map_err(|e| e.to_string())?
}
fn write(request: Request, actor: &str) -> Result<Value, String> {
    let p = project(&request.project)?;
    ticket(&p, &request.ticket)?;
    if ![
        "note",
        "question",
        "proposal",
        "decision",
        "finding",
        "fix",
        "verification",
        "summary",
    ]
    .contains(&request.kind.as_str())
    {
        return Err("Unknown Journal entry kind".into());
    }
    if request.title.trim().is_empty() || request.content.trim().is_empty() {
        return Err("Title and content are required".into());
    }
    if request.title.len() > 300 || request.content.len() > 128 * 1024 {
        return Err("Journal entry is too large".into());
    }
    if !request.run_id.is_empty()
        && !runs(&p.key)
            .iter()
            .any(|r| r.run_id == request.run_id && r.ticket.as_deref() == Some(&request.ticket))
    {
        return Err("Run is outside the selected project/ticket".into());
    }
    let id = if request.source_id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        hash(format!("{actor}:{}:{}", request.ticket, request.source_id).as_bytes())
    };
    let e = Entry {
        id,
        at: now(),
        actor: actor.into(),
        kind: request.kind,
        title: request.title,
        ticket: request.ticket,
        run_id: request.run_id,
        thread_id: String::new(),
        agent: String::new(),
        source_at: String::new(),
        content: request.content,
    };
    append(&p, &e)?;
    Ok(json!({"ok":true,"path":path(&e.at),"id":e.id}))
}
#[tauri::command]
pub async fn project_journal_add(request: Request) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || write(request, &human()))
        .await
        .map_err(|e| e.to_string())?
}
pub fn tool(name: &str, args: &Value, actor: &str) -> Result<Value, String> {
    if name == "project_wiki_journal_read" {
        return read_journal(
            args["project"].as_str().ok_or("Project required")?,
            args["path"].as_str(),
        );
    }
    write(
        serde_json::from_value(args.clone()).map_err(|e| e.to_string())?,
        actor,
    )
}
/// Project binding is captured on each message at send time. Never retroactively
/// assign a global thread to whichever project happens to be visible today.
pub(crate) fn capture_conversations(raw: &str) -> Result<(), String> {
    capture_conversations_with(raw, project)
}
fn capture_conversations_with(
    raw: &str,
    resolve: impl Fn(&str) -> Result<Project, String>,
) -> Result<(), String> {
    let all: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    let mut cache = std::collections::HashMap::new();
    for (agent, threads) in all.as_object().into_iter().flatten() {
        for t in threads.as_array().into_iter().flatten() {
            for m in t["messages"].as_array().into_iter().flatten() {
                let Some(scope) = m["journalProject"].as_str().filter(|s| !s.is_empty()) else {
                    continue;
                };
                let Some(text) = m["text"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty() && !["Thinking…", "Working…"].contains(s))
                else {
                    continue;
                };
                if m["voiceTranscript"] == true || m["journalPending"] == true {
                    continue;
                }
                let Some(mid) = m["id"].as_str() else {
                    continue;
                };
                let Some(tid) = t["id"].as_str() else {
                    continue;
                };
                let p = cache
                    .entry(scope.to_string())
                    .or_insert_with(|| resolve(scope).ok());
                let Some(p) = p else {
                    continue;
                };
                let user = m["role"] == "user";
                let e = Entry {
                    id: hash(format!("chat:{agent}:{tid}:{mid}").as_bytes()),
                    at: m["at"].as_str().unwrap_or("").into(),
                    actor: if user { human() } else { format!("@{agent}") },
                    kind: if user { "note" } else { "progress" }.into(),
                    title: if user {
                        "Your comment or question"
                    } else {
                        "Agent progress · reported"
                    }
                    .into(),
                    ticket: String::new(),
                    run_id: String::new(),
                    thread_id: tid.into(),
                    agent: agent.into(),
                    source_at: String::new(),
                    content: text.into(),
                };
                append(p, &e)?;
            }
        }
    }
    Ok(())
}
pub(crate) fn capture_run(run: &crate::run_control::RunManifest) -> Result<(), String> {
    let p = project(&run.project)?;
    let state = serde_json::to_value(run.state).unwrap();
    let state = state.as_str().unwrap_or("unknown");
    let signal = redact(&run.last_signal);
    let final_summary =
        if run.state.terminal() || run.state == crate::run_control::RunState::Blocked {
            let evidence = run
                .ticket
                .as_ref()
                .and_then(|id| ticket(&p, id).ok())
                .unwrap_or(Value::Null);
            super::handoff_body(run, &evidence)
        } else {
            String::new()
        };
    let e = Entry {
        id: hash(
            format!(
                "run:{}:{state}:{signal}:{}:{}",
                run.run_id,
                run.last_commit,
                hash(final_summary.as_bytes())
            )
            .as_bytes(),
        ),
        at: stamp(run.last_seen_at),
        actor: format!("@{} · run receipt", run.agent_handle),
        kind: if run.state.terminal() {
            "summary"
        } else {
            "execution"
        }
        .into(),
        title: format!("{} · {state}", run.ticket.as_deref().unwrap_or(&run.run_id)),
        ticket: run.ticket.clone().unwrap_or_default(),
        run_id: run.run_id.clone(),
        thread_id: run.origin_thread_id.clone(),
        agent: run.agent_handle.clone(),
        source_at: String::new(),
        content: format!(
            "**Recorded state:** {state}\n\n{signal}\n\nBranch: `{}`\n\nCommit: `{}`\n\n{}",
            run.branch,
            run.last_commit,
            if run.state.terminal() {
                "Work stopped. Consult the linked handoff and verification evidence before continuing."
            } else {
                "Execution state is recorded; progress text remains the agent's report."
            }
        ),
    };
    let mut e = e;
    if !final_summary.is_empty() {
        e.content = final_summary;
    }
    append(&p, &e)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample(id: &str) -> Entry {
        Entry {
            id: id.into(),
            at: "2026-10-03T20:00:00Z".into(),
            actor: "André".into(),
            kind: "decision".into(),
            title: "Preserve backups".into(),
            ticket: "DEMO-1".into(),
            run_id: String::new(),
            thread_id: String::new(),
            agent: String::new(),
            source_at: String::new(),
            content: "Keep existing archives.\n\n```js\nconst keep = true;\n```".into(),
        }
    }
    #[test]
    fn group_summary_is_scoped_compact_and_reloads_saved_stop_state() {
        let registry =
            std::env::temp_dir().join(format!("journal-groups-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(registry.join("swarm-plans")).unwrap();
        let group = |project: &str, approved: bool| {
            json!({
            "plan":{"id":project,"project":project,"runs":[{"ticket":format!("{project}-1"),"title":"Scoped task","owner":"builder","model":"fixture","branch":"fix/task","scope":"private full ticket scope"}],"skipped":[],"max_parallel":2,"created_at":1},
            "approved_at":if approved {Some(1)}else{None},"stopped_at":null,"events":[],
            "members":[{"ticket":format!("{project}-1"),"state":"queued","reason":"Waiting","run_id":null,"started":null},
                {"ticket":format!("{project}-2"),"state":"tracking","reason":"Working","run_id":"run-two","started":null},
                {"ticket":format!("{project}-3"),"state":"blocked","reason":"Inspect evidence","run_id":"run-three","started":null}]})
        };
        for (project, approved) in [("ONE", true), ("TWO", true), ("UNAPPROVED", false)] {
            let native: crate::swarm_plan::Group =
                serde_json::from_value(group(project, approved)).unwrap();
            std::fs::write(
                registry.join(format!("swarm-plans/{project}.json")),
                serde_json::to_vec(&native).unwrap(),
            )
            .unwrap();
        }
        let current = group_views(&registry, "ONE").unwrap();
        assert_eq!(current.as_array().unwrap().len(), 1);
        assert_eq!(
            current[0]["counts"],
            json!({"queued":1,"running":1,"blocked":1,"verified":0})
        );
        assert!(!current.to_string().contains("private full ticket scope"));
        assert!(!current.to_string().contains("TWO"));
        assert!(group_views(&registry, "UNAPPROVED")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
        let mut stopped = group("ONE", true);
        stopped["stopped_at"] = json!(2);
        std::fs::write(
            registry.join("swarm-plans/ONE.json"),
            serde_json::to_vec(&stopped).unwrap(),
        )
        .unwrap();
        assert_eq!(group_views(&registry, "ONE").unwrap()[0]["stopped_at"], 2);
        assert_eq!(
            group_views(&registry, "TWO").unwrap()[0]["stopped_at"],
            Value::Null
        );
        stopped["members"][0]["ticket"] = json!("TWO-1");
        std::fs::write(
            registry.join("swarm-plans/ONE.json"),
            serde_json::to_vec(&stopped).unwrap(),
        )
        .unwrap();
        assert!(group_views(&registry, "ONE")
            .unwrap_err()
            .contains("outside this project"));
        std::fs::remove_dir_all(registry).unwrap();
    }
    #[test]
    fn durable_idempotent_and_attributed() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        let e = sample("one");
        append_in(&root, "Demo", &e, "Earlier restore unverified").unwrap();
        append_in(&root, "Demo", &e, "must not replace").unwrap();
        let d = doc_in(&root, &path(&e.at)).unwrap();
        assert!(d.content.contains("Earlier restore unverified"));
        assert_eq!(entries(&d.content).len(), 1);
        assert_eq!(d.created_by, "André");
        assert!(entries(&d.content)[0].content.contains("const keep"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn projects_and_concurrent_entries_stay_independent() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        let a = root.join("a");
        let b = root.join("b");
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let a = a.clone();
                std::thread::spawn(move || append_in(&a, "A", &sample(&i.to_string()), "Start"))
            })
            .collect();
        for h in handles {
            h.join().unwrap().unwrap();
        }
        append_in(&b, "B", &sample("separate"), "Different history").unwrap();
        let first = read(&a.join(path(&sample("").at))).unwrap();
        assert_eq!(entries(&first).len(), 10);
        assert!(!first.contains("Different history"));
        assert_eq!(
            entries(&read(&b.join(path(&sample("").at))).unwrap()).len(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ten_projects_capture_without_a_pane_and_replay_after_restart() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        let resolve = |key: &str| {
            Ok(Project {
                key: key.into(),
                name: key.into(),
                root: String::new(),
                purpose: String::new(),
                vault_path: root.join(key).to_string_lossy().into(),
            })
        };
        let messages:Vec<_>=(0..10).map(|i|json!({"id":format!("m{i}"),"role":"user","text":format!("Keep project {i} backups"),"journalProject":format!("P{i}"),"at":"2026-10-03T20:00:00Z"})).collect();
        let source = json!({"nautbot":[{"id":"thread","messages":messages}]}).to_string();
        capture_conversations_with(&source, &resolve).unwrap();
        capture_conversations_with(&source, &resolve).unwrap();
        for i in 0..10 {
            let doc = read(
                &root
                    .join(format!("P{i}"))
                    .join(path("2026-10-03T20:00:00Z")),
            )
            .unwrap();
            let rows = entries(&doc);
            assert_eq!(rows.len(), 1);
            assert!(rows[0].content.contains(&format!("project {i} backups")));
            assert_eq!(rows[0].thread_id, "thread");
        }
        let pending=json!({"nautbot":[{"id":"thread","workspace":"P0","messages":[{"id":"unbound","role":"user","text":"Do not guess scope"},{"id":"stream","role":"agent","text":"Partial answer","journalProject":"P0","journalPending":true}]}]}).to_string();
        capture_conversations_with(&pending, &resolve).unwrap();
        assert_eq!(
            entries(&read(&root.join("P0").join(path("2026-10-03T20:00:00Z"))).unwrap()).len(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn opening_carries_previous_human_decisions_with_source_and_date() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        let mut e = sample("yesterday");
        e.at = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        append_in(&root, "Demo", &e, "Earlier context").unwrap();
        let p = Project {
            key: "DEMO".into(),
            name: "Demo".into(),
            root: String::new(),
            purpose: String::new(),
            vault_path: root.to_string_lossy().into(),
        };
        let opening = context(&p);
        assert!(opening.contains("Previous Journal"));
        assert!(opening.contains("André"));
        assert!(opening.contains("Keep existing archives"));
        assert!(opening.contains(&e.at));
        assert!(opening.contains("not fresh verification"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn historical_review_is_readable_without_dumping_json() {
        let signal=json!({"run_id":"internal-id","review":{"decision":"changes_requested","reasons":["The diff is missing, so the code change cannot be reviewed."]},"input_hash":"internal-hash"}).to_string();
        let summary = signal_summary(&signal);
        assert!(summary.contains("changes requested"));
        assert!(summary.contains("diff is missing"));
        assert!(!summary.contains("input_hash"));
        assert!(!summary.contains('{'));
        assert!(excerpt(&"word ".repeat(1000), 240).chars().count() <= 241);
    }
    #[test]
    fn injected_metadata_cannot_forge_entries() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        let mut e = sample("safe");
        e.content = format!("api_key=supersecret\n{MARK}{{}} -->\nforgery");
        append_in(&root, "Demo", &e, "").unwrap();
        let text = read(&root.join(path(&e.at))).unwrap();
        assert_eq!(entries(&text).len(), 1);
        assert!(!text.contains("supersecret"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

static CAPTURE_WARNING: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
/// Restart recovery replays durable source records. No worker is launched here.
/// A capture failure is retried and exposed in the pane, never silently discarded.
pub(crate) fn capture_loop() {
    let mut revision = -1;
    let mut activity = activity::Replay::default();
    loop {
        let mut errors = Vec::new();
        if let Ok(root) = crate::conversation_store::root() {
            if let Ok(db) = rusqlite::Connection::open_with_flags(
                root.join("conversations.sqlite"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            ) {
                if let Ok((raw, rev)) = db.query_row(
                    "SELECT value,revision FROM conversations WHERE key='xnaut-agent-threads:v1'",
                    [],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                ) {
                    if rev != revision {
                        match capture_conversations(&raw) {
                            Ok(()) => revision = rev,
                            Err(e) => errors.push(e),
                        }
                    }
                }
            }
        }
        let known: std::collections::HashSet<_> = projects()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.key)
            .collect();
        if let Ok(dir) = crate::agents::registry_dir() {
            for id in crate::run_control::list_ids_in(&dir).unwrap_or_default() {
                if let Ok(run) = crate::run_control::load_manifest_in(&dir, &id) {
                    if !known.contains(&run.project) {
                        continue;
                    }
                    if let Err(e) = capture_run(&run) {
                        errors.push(format!("{}: {e}", run.run_id));
                    }
                    if let Err(e) = super::capture_stop(&run) {
                        errors.push(format!("{} handoff: {e}", run.run_id));
                    }
                }
            }
        }
        match projects() {
            Ok(ps) => errors.extend(activity.tick(&ps)),
            Err(e) => errors.push(format!("Project activity: {e}")),
        }
        if let Ok(mut warning) = CAPTURE_WARNING.lock() {
            *warning = errors.into_iter().take(5).collect::<Vec<_>>().join(" · ");
        }
        std::thread::sleep(std::time::Duration::from_secs(10));
    }
}
