//! XNAUT-466: project-scoped replay of durable coordinator and review evidence.
//! Saved source timestamps/identities survive restarts; capture is not verification.
use super::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
struct Activity {
    project: String,
    source: String,
    entry: Entry,
    evidence: Value,
}
fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn timestamp(v: &Value) -> Option<String> {
    ["at", "timestamp", "updated_at", "created_at"]
        .iter()
        .filter_map(|k| v[k].as_str())
        .find_map(|at| {
            chrono::DateTime::parse_from_rfc3339(at)
                .ok()
                .map(|d| d.to_rfc3339())
        })
        .or_else(|| {
            ["at_ms", "source_at_ms"].iter().find_map(|k| {
                chrono::DateTime::<chrono::Utc>::from_timestamp_millis(v[*k].as_i64()?)
                    .map(|d| d.to_rfc3339())
            })
        })
}
fn ticket_in(project: &str, ticket: &str) -> bool {
    ticket
        .strip_prefix(&format!("{project}-"))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}
fn event(
    project: &str,
    ticket: &str,
    run: &str,
    source: &str,
    id: &str,
    v: &Value,
    actor: &str,
    kind: &str,
    title: String,
    detail: String,
) -> Result<Activity, String> {
    if project.is_empty() || (!ticket.is_empty() && !ticket_in(project, ticket)) {
        return Err("Activity has inconsistent project/ticket scope".into());
    }
    let at = timestamp(v).ok_or("Activity has no valid original timestamp; capture deferred")?;
    if id.is_empty() {
        return Err("Activity has no durable source identity".into());
    }
    let identity = hash(format!("{project}\0{source}\0{id}").as_bytes());
    Ok(Activity {
        project: project.into(),
        source: source.into(),
        evidence: v.clone(),
        entry: Entry {
            id: format!("activity:{identity}"),
            at: at.clone(),
            source_at: at,
            actor: if actor.is_empty() {
                "Not recorded".into()
            } else {
                redact(actor)
            },
            kind: kind.into(),
            title: redact(&title),
            ticket: ticket.into(),
            run_id: run.into(),
            thread_id: String::new(),
            agent: actor.trim_start_matches('@').into(),
            content: redact(&detail),
        },
    })
}
fn phase_kind(state: &str) -> &'static str {
    if state.contains("block")
        || state.contains("fail")
        || state.contains("changes_requested")
        || state == "findings"
    {
        "finding"
    } else if state.contains("fix") || state.contains("repair") {
        "fix"
    } else if state.contains("verif") || state.contains("review") {
        "check"
    } else {
        "execution"
    }
}
fn history(
    project: &str,
    ticket: &str,
    run: &str,
    source: &str,
    values: &Value,
) -> Vec<Result<Activity, String>> {
    values
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            let ticket = v["ticket"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(ticket);
            let run = v["run_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(run);
            if v["project"].as_str().is_some_and(|p| p != project) {
                return Err("Activity history conflicts with its project scope".into());
            }
            let state = v["state"].as_str().or(v["kind"].as_str()).unwrap_or("");
            let detail = format!(
                "**Recorded transition:** {}\n\n{}\n\n{}",
                state.replace('_', " "),
                v["reason"].as_str().or(v["detail"].as_str()).unwrap_or(""),
                if string(v, "head").is_empty() {
                    String::new()
                } else {
                    format!("Revision: `{}`", string(v, "head"))
                }
            );
            event(
                project,
                ticket,
                run,
                source,
                string(v, "id"),
                v,
                string(v, "actor"),
                phase_kind(state),
                format!(
                    "{} · {}",
                    if ticket.is_empty() { project } else { ticket },
                    state.replace('_', " ")
                ),
                detail,
            )
        })
        .collect()
}
fn capture_one(root: &Path, name: &str, activity: &Activity) -> Result<(), String> {
    let key = activity
        .entry
        .id
        .strip_prefix("activity:")
        .ok_or("Invalid activity identity")?;
    let rel = format!("Development/evidence/journal/{key}.md");
    let evidence_path = markdown_path(root, &rel)?;
    if !evidence_path.exists() {
        let body = format!("# {}\n\nSource: `{}`\n\nOriginal time: {}\n\nActor: {}\n\n> Saved source evidence. A recorded action or agent claim does not itself prove completion.\n\n```json\n{}\n```\n",
            activity.entry.title, activity.source, activity.entry.at, activity.entry.actor,
            redact(&serde_json::to_string_pretty(&activity.evidence).map_err(|e|e.to_string())?));
        // save_in uses the Wiki's exclusive lock/CAS and preserves manual revisions.
        match save_in(
            root,
            &rel,
            &body,
            None,
            &activity.entry.actor,
            "Preserve loop evidence",
        ) {
            Ok(()) => (),
            Err(_) if evidence_path.exists() => (),
            Err(e) => return Err(e),
        }
    }
    let mut entry = activity.entry.clone();
    entry.content.push_str(&format!("\n\n[Source evidence](../../{rel})\n\n*Recorded from saved project evidence; consult its revision and checks before treating work as verified.*"));
    append_in(root,name,&entry,"Recorded project activity. Earlier evidence retains its original time; use the current project summary above for present status.")
}

/// In-memory fast path only. The canonical Journal entries are the durable
/// deduplication index, so losing this cache cannot duplicate an entry.
#[derive(Default)]
pub(super) struct Replay {
    captured: HashSet<String>,
}
impl Replay {
    fn capture(&mut self, ps: &[Project], rows: Vec<Result<Activity, String>>) -> Vec<String> {
        let projects: HashMap<_, _> = ps.iter().map(|p| (p.key.as_str(), p)).collect();
        let mut errors = Vec::new();
        for row in rows {
            let activity = match row {
                Ok(a) => a,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let Some(p) = projects.get(activity.project.as_str()) else {
                continue;
            };
            // A remapped Vault must receive its own replay even in the same process.
            let cache_key = format!("{}:{}", p.vault_path, activity.entry.id);
            if self.captured.contains(&cache_key) {
                continue;
            }
            match capture_one(Path::new(&p.vault_path), &p.name, &activity) {
                Ok(()) => {
                    self.captured.insert(cache_key);
                }
                Err(e) => errors.push(format!("{} activity: {e}", p.key)),
            }
        }
        errors
    }
    pub(super) fn tick(&mut self, ps: &[Project]) -> Vec<String> {
        let known: HashSet<_> = ps.iter().map(|p| p.key.as_str()).collect();
        let mut rows = Vec::new();
        // Group histories are native saved approval/dispatch events. Reading
        // them does not admit work or expand the set of authorized tickets.
        match crate::agents::registry_dir() {
            Ok(root) => {
                let dir = root.join("swarm-plans");
                if dir.exists() {
                    match std::fs::read_dir(dir) {
                        Ok(files) => {
                            for file in files
                                .flatten()
                                .filter(|f| f.path().extension().is_some_and(|x| x == "json"))
                            {
                                match read(&file.path()).and_then(|s|serde_json::from_str::<Value>(&s).map_err(|e|e.to_string())) {
                                Ok(v)=>{
                                    let p=v["plan"]["project"].as_str().or(v["project"].as_str()).unwrap_or("");
                                    if known.contains(p) { rows.extend(history(p,"","","swarm",&v["events"])); }
                                },
                                Err(_)=>rows.push(Err("A coordinator history could not be read; some activity may be missing".into())),
                            }
                            }
                        }
                        Err(_) => rows.push(Err("Coordinator history is unavailable".into())),
                    }
                }
            }
            Err(e) => rows.push(Err(e)),
        }
        match crate::repository_transfer::list() {
            Ok(transfers) => {
                for t in transfers
                    .into_iter()
                    .filter(|t| known.contains(t.project.as_str()))
                {
                    if let Ok(v) = serde_json::to_value(&t) {
                        rows.extend(history(
                            &t.project,
                            t.ticket.as_deref().unwrap_or(""),
                            &t.run_id,
                            "review",
                            &v["quality"]["events"],
                        ));
                    }
                }
            }
            Err(_) => rows.push(Err("Review histories are unavailable".into())),
        }
        match crate::ticket_triage::ticket_triage_records() {
            Ok(records) => {
                for r in records {
                    let Some(p) = r.project.as_deref().filter(|p| known.contains(p)) else {
                        continue;
                    };
                    if let Ok(v) = serde_json::to_value(&r) {
                        let ticket = v["ticket_id"]
                            .as_str()
                            .or(v["binding"]["ticket_id"].as_str())
                            .or(v["binding"]["ticket"].as_str())
                            .unwrap_or("");
                        if v["events"].as_array().is_some_and(|a| !a.is_empty()) {
                            rows.extend(history(p, ticket, "", "triage", &v["events"]));
                        } else {
                            let identity =
                                format!("{}:{}:{}", r.fingerprint, r.status, r.updated_at);
                            rows.push(event(p,ticket,"","triage",&identity,&v,"","decision",
                            format!("Finding #{} · {}",r.issue_number,r.status.replace('_'," ")),
                            format!("**Triage state:** {}\n\n{}\n\nOriginal finding: {}\n\nThis records the saved triage decision; a duplicate suggestion is not a confirmed shared cause.",r.status,r.issue_title,r.issue_url)));
                        }
                    }
                }
            }
            Err(_) => rows.push(Err("Findings triage history is unavailable".into())),
        }
        match tauri::async_runtime::block_on(crate::sandbox_verify::sandbox_verify_records()) {
            Ok(records) => {
                for r in records
                    .into_iter()
                    .filter(|r| known.contains(r.project.as_str()))
                {
                    if let Ok(v) = serde_json::to_value(&r) {
                        let identity = format!("{}:{}:{}", r.id, r.status, r.commit_sha);
                        let detail = format!(
                            "**Verification record:** {}\n\nRevision: `{}`\n\n{}\n\n{}",
                            r.status,
                            r.commit_sha,
                            if r.not_evidence {
                                "This run is explicitly marked as not evidence for the ticket."
                            } else {
                                "These checks apply to the recorded revision only; independent review and completion remain separate."
                            },
                            r.error
                        );
                        rows.push(event(
                            &r.project,
                            &r.ticket_id,
                            &r.run_id,
                            "verification",
                            &identity,
                            &v,
                            "xNAUT verifier",
                            "check",
                            format!("{} · verification {}", r.ticket_id, r.status),
                            detail,
                        ));
                    }
                }
            }
            Err(_) => rows.push(Err("Verification history is unavailable".into())),
        }
        self.capture(ps, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project(root: &Path, key: &str) -> Project {
        Project {
            key: key.into(),
            name: key.into(),
            root: String::new(),
            purpose: String::new(),
            vault_path: root.join(key).to_string_lossy().into(),
        }
    }
    #[test]
    fn repair_history_replays_once_preserving_owner_notes_and_projects() {
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        let ps = vec![project(&dir, "ONE"), project(&dir, "TWO")];
        let values = json!([
            {"id":"review-1","at":"2026-10-06T12:00:00Z","state":"findings","actor":"reviewer","reason":"Parser loses the last item","head":"abc"},
            {"id":"repair-1","at":"2026-10-06T12:05:00Z","state":"repair_published","actor":"builder","reason":"Preserved the last item","head":"def"},
            {"id":"verify-1","at":"2026-10-06T12:10:00Z","state":"verified","actor":"reviewer","reason":"Regression passed at revised head","head":"def"}
        ]);
        let rows = || {
            let mut r = history("ONE", "ONE-1", "worker", "review", &values);
            r.extend(history("TWO", "TWO-2", "worker2", "review", &values));
            r
        };
        assert!(Replay::default().capture(&ps, rows()).is_empty());
        let path = dir
            .join("ONE")
            .join(super::super::path("2026-10-06T12:00:00Z"));
        let mut saved = read(&path).unwrap();
        saved.push_str("\nOwner note: keep the compatibility branch.\n");
        std::fs::write(&path, &saved).unwrap();
        assert!(
            Replay::default().capture(&ps, rows()).is_empty(),
            "new process replays sources"
        );
        let text = read(&path).unwrap();
        let recorded = entries(&text);
        assert_eq!(recorded.len(), 3);
        assert!(text.contains("Owner note: keep"));
        assert!(!text.contains("TWO-2"));
        assert_eq!(recorded[0].actor, "reviewer");
        assert_eq!(recorded[1].actor, "builder");
        assert_eq!(recorded[0].source_at, "2026-10-06T12:00:00+00:00");
        assert!(recorded[0].content.contains("Source evidence"));
        assert_eq!(
            entries(
                &read(
                    &dir.join("TWO")
                        .join(super::super::path("2026-10-06T12:00:00Z"))
                )
                .unwrap()
            )
            .len(),
            3
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unknown_timestamps_and_cross_project_tickets_are_not_guessed() {
        let v = json!({"id":"x","state":"verified"});
        assert!(event(
            "ONE",
            "ONE-1",
            "",
            "review",
            "x",
            &v,
            "",
            "check",
            "x".into(),
            "x".into()
        )
        .is_err());
        let v = json!({"id":"x","at_ms":1791288000000i64,"state":"verified"});
        assert!(event(
            "ONE",
            "TWO-1",
            "",
            "review",
            "x",
            &v,
            "",
            "check",
            "x".into(),
            "x".into()
        )
        .is_err());
        let a = event(
            "ONE",
            "ONE-1",
            "",
            "review",
            "x",
            &v,
            "",
            "check",
            "x".into(),
            "x".into(),
        )
        .unwrap();
        assert_eq!(a.entry.actor, "Not recorded");
    }
}
