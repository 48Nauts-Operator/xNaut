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
    // Mutable receipts can retain a source ID while gaining checks or artifacts.
    // Bind the original time and complete saved version so each change is an
    // immutable observation, including after restart or across UTC day rollover.
    let version = hash(&serde_json::to_vec(v).map_err(|e| e.to_string())?);
    let identity = hash(format!("{project}\0{source}\0{id}\0{at}\0{version}").as_bytes());
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
            run_id: redact(run),
            thread_id: String::new(),
            agent: redact(actor.trim_start_matches('@')),
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
/// Operator attribution remains the saved coordinator. The linked worker is a
/// separate role: repair states refer to the author successor; review states to
/// the reviewer. A blocked repair retains its preceding phase's identity.
fn phase_run<'a>(v: &'a Value, original: &'a str, previous_state: &str) -> (&'a str, &'static str) {
    let state = v["state"].as_str().or(v["kind"].as_str()).unwrap_or("");
    let review = string(v, "review_child");
    let author = string(v, "author_child");
    let author_phase = state.starts_with("repair")
        || (state == "pending" && review.is_empty() && !author.is_empty())
        || (state == "blocked" && previous_state.starts_with("repair"));
    let (worker, role) = if author_phase && !author.is_empty() {
        (author, "Author repair")
    } else if !review.is_empty() {
        (review, "Independent review")
    } else if !author.is_empty() {
        (author, "Author repair")
    } else {
        (original, "Original assignment")
    };
    (
        v["run_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(worker),
        role,
    )
}
fn history(
    project: &str,
    ticket: &str,
    run: &str,
    source: &str,
    values: &Value,
) -> Vec<Result<Activity, String>> {
    let mut previous_state = String::new();
    values
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            let ticket = v["ticket"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(ticket);
            let original_run = run;
            if v["project"].as_str().is_some_and(|p| p != project) {
                return Err("Activity history conflicts with its project scope".into());
            }
            let state = v["state"].as_str().or(v["kind"].as_str()).unwrap_or("");
            let (run, role) = phase_run(v, original_run, &previous_state);
            previous_state = state.into();
            let mut detail = format!(
                "**Recorded transition:** {}\n\n{}\n\n{}",
                state.replace('_', " "),
                v["reason"].as_str().or(v["detail"].as_str()).unwrap_or(""),
                if string(v, "head").is_empty() {
                    String::new()
                } else {
                    format!("Revision: `{}`", string(v, "head"))
                }
            );
            if source == "review" {
                detail.push_str(&format!("\n\n**Linked role:** {role}\n\n"));
                for (label, key) in [
                    ("Independent review run", "review_child"),
                    ("Author repair run", "author_child"),
                    ("Predecessor run", "predecessor_run_id"),
                ] {
                    if !string(v, key).is_empty() {
                        detail.push_str(&format!("{label}: `{}`\n\n", string(v, key)));
                    }
                }
            }
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
        let sources = (|| {
            Ok::<_, String>(Sources {
                groups: crate::agents::registry_dir()?.join("swarm-plans"),
                transfers: crate::repository_transfer::store_dir()?,
                triage: crate::ticket_triage::triage_root()?.join("records"),
                verification: crate::sandbox_verify::records_dir(),
            })
        })();
        match sources {
            Ok(sources) => self.capture(ps, collect(ps, &sources)),
            Err(_) => {
                vec!["Project activity source locations are unavailable; capture will retry".into()]
            }
        }
    }
}

/// Production readers and fixture replay share this exact path. No global
/// settings, workers, network clients, or selected UI project are consulted.
struct Sources {
    groups: PathBuf,
    transfers: PathBuf,
    triage: PathBuf,
    verification: PathBuf,
}
fn source_rows(root: &Path, label: &str) -> Vec<Result<Value, String>> {
    let files = match std::fs::read_dir(root) {
        Ok(files) => files,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return vec![],
        Err(_) => {
            return vec![Err(format!(
                "{label} history is unavailable; capture will retry"
            ))]
        }
    };
    let mut paths = Vec::new();
    let mut rows = Vec::new();
    for file in files {
        match file {
            Ok(file) if file.path().extension().is_some_and(|e| e == "json") => {
                if file.file_type().is_ok_and(|t| t.is_file()) {
                    paths.push(file.path());
                } else {
                    rows.push(Err(format!("A {label} history record is not a readable regular file; some activity may be missing")));
                }
            }
            Ok(_) => (),
            Err(_) => rows.push(Err(format!(
                "A {label} history entry is unavailable; some activity may be missing"
            ))),
        }
    }
    paths.sort();
    for path in paths {
        rows.push(
            read(&path)
                .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
                .map_err(|_| {
                    format!("A {label} history record is unreadable; some activity may be missing")
                }),
        );
    }
    rows
}
fn collect(ps: &[Project], sources: &Sources) -> Vec<Result<Activity, String>> {
    let known: HashSet<_> = ps.iter().map(|p| p.key.as_str()).collect();
    let mut rows = Vec::new();
    for (source, root) in [
        ("swarm", &sources.groups),
        ("review", &sources.transfers),
        ("triage", &sources.triage),
        ("verification", &sources.verification),
    ] {
        for raw in source_rows(root, source) {
            let v = match raw {
                Ok(v) => v,
                Err(e) => {
                    rows.push(Err(e));
                    continue;
                }
            };
            let project = if source == "swarm" {
                v["plan"]["project"].as_str()
            } else {
                v["project"].as_str()
            };
            let Some(project) = project.filter(|p| !p.is_empty()) else {
                rows.push(Err(format!(
                    "A {source} history record has no project binding; capture deferred"
                )));
                continue;
            };
            // Foreign records are never copied or diagnosed by filename/content.
            if !known.contains(project) {
                continue;
            }
            let valid = match source {
                "swarm" => serde_json::from_value::<crate::swarm_plan::Group>(v.clone()).is_ok(),
                "review" => {
                    serde_json::from_value::<crate::repository_transfer::Transfer>(v.clone())
                        .is_ok()
                }
                "triage" => {
                    serde_json::from_value::<crate::ticket_triage::TriageRecord>(v.clone()).is_ok()
                }
                _ => {
                    serde_json::from_value::<crate::sandbox_verify::VerifyRecord>(v.clone()).is_ok()
                }
            };
            if !valid {
                rows.push(Err(format!(
                    "{project}: {source} history has an invalid saved record; capture deferred"
                )));
                continue;
            }
            match source {
                "swarm" => rows.extend(history(project, "", "", source, &v["events"])),
                "review" => rows.extend(history(
                    project,
                    string(&v, "ticket"),
                    string(&v, "run_id"),
                    source,
                    &v["quality"]["events"],
                )),
                "triage" => {
                    let ticket = v["binding"]["ticket"].as_str().unwrap_or("");
                    if v["events"].as_array().is_some_and(|a| !a.is_empty()) {
                        rows.extend(history(
                            project,
                            ticket,
                            string(&v, "run_id"),
                            source,
                            &v["events"],
                        ));
                    } else {
                        let identity =
                            format!("{}:{}", string(&v, "fingerprint"), string(&v, "status"));
                        rows.push(event(project,ticket,string(&v,"run_id"),source,&identity,&v,"","decision",
                            format!("Finding #{} · {}",v["issue_number"],string(&v,"status").replace('_'," ")),
                            format!("**Triage state:** {}\n\n{}\n\nOriginal finding: {}\n\nThis records the saved triage decision; a duplicate suggestion is not a confirmed shared cause.",string(&v,"status"),string(&v,"issue_title"),string(&v,"issue_url"))));
                    }
                }
                _ => {
                    let ticket = string(&v, "ticket_id");
                    let status = string(&v, "status");
                    let detail = format!(
                        "**Verification record:** {status}\n\nRevision: `{}`\n\n{}\n\n{}",
                        string(&v, "commit_sha"),
                        if v["not_evidence"] == true {
                            "This run is explicitly marked as not evidence for the ticket."
                        } else {
                            "These checks apply to the recorded revision only; independent review and completion remain separate."
                        },
                        string(&v, "error")
                    );
                    rows.push(event(
                        project,
                        ticket,
                        string(&v, "run_id"),
                        source,
                        string(&v, "id"),
                        &v,
                        "xNAUT verifier",
                        "check",
                        format!("{ticket} · verification {status}"),
                        detail,
                    ));
                }
            }
        }
    }
    rows.sort_by(|a, b| match (a, b) {
        (Ok(a), Ok(b)) => a
            .entry
            .at
            .cmp(&b.entry.at)
            .then(a.entry.id.cmp(&b.entry.id)),
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    });
    rows
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
    fn sources(root: &Path) -> Sources {
        Sources {
            groups: root.join("groups"),
            transfers: root.join("transfers"),
            triage: root.join("triage"),
            verification: root.join("verification"),
        }
    }
    fn persist<T: serde::de::DeserializeOwned + serde::Serialize>(
        root: &Path,
        name: &str,
        value: Value,
    ) {
        let record: T = serde_json::from_value(value).unwrap();
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(
            root.join(format!("{name}.json")),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
    }
    fn verify_fixture(project: &str) -> Value {
        json!({"id":format!("verify-{project}"),"run_id":"run-one","ticket_id":format!("{project}-1"),"project":project,
            "repo_path":"fixture","commit_sha":"abc","provider_kind":"local","sandbox_id":"sandbox","public_url":"",
            "error":"","status":"running","steps":[],"log_dir":"fixture/logs","video_path":null,
            "created_at":"2026-10-06T12:00:00Z","updated_at":"2026-10-06T12:00:00Z"})
    }
    #[test]
    fn production_sources_replay_actual_records_for_every_project_and_consume_updates() {
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        let paths = sources(&dir.join("sources"));
        let ps = vec![project(&dir, "ONE"), project(&dir, "TWO")];
        for p in ["ONE", "TWO", "FOREIGN"] {
            let ticket = format!("{p}-1");
            persist::<crate::swarm_plan::Group>(
                &paths.groups,
                p,
                json!({
                "plan":{"id":p,"project":p,"runs":[],"skipped":[],"max_parallel":2,"created_at":1791288000000i64},
                "approved_at":1791288000000i64,"members":[],"events":[{"id":"group-start","project":p,"ticket":ticket,
                    "actor":"nautbot","source":format!("swarm-plans/{p}.json"),"at_ms":1791288000000i64,"state":"tracking","reason":"Started approved scope","run_id":"run-one"}]}),
            );
            // Serialize the native Review through Transfer so schema changes
            // cannot make a JSON-only fixture pass while production drops events.
            persist::<crate::repository_transfer::Transfer>(
                &paths.transfers,
                p,
                json!({"run_id":"run-one","project":p,"ticket":ticket,"handle":"builder",
                "local_path":"fixture","remote":"https://forge.test/team/app.git","source_sha":"abc","base":"main","branch":"fix",
                "workdir":"fixture","artifacts":"fixture","state":"published","pr_url":null,"error":null,
                "quality":{"state":"findings","reviewer":"reviewer","head":"abc","base":"base","worktree":"fixture","child":"review-one","attempts":1,
                    "message":"Missing boundary check","report":null,"jev":null,"comment_url":null,
                    "events":[{"id":"review-event","at_ms":1791288060000i64,"state":"findings","reason":"Missing boundary check","head":"abc","base":"base",
                        "review_child":"review-one","author_child":null,"predecessor_run_id":"run-one","actor":"xNAUT coordinator"}]}}),
            );
            persist::<crate::ticket_triage::TriageRecord>(
                &paths.triage,
                p,
                json!({"fingerprint":p,"source_id":"forgejo:team/app#7",
                "binding":{"ticket":ticket,"project":p,"source_id":"forgejo:team/app#7","scope_hash":"fixture"},
                "run_id":"triage-one","forge_index":0,"forge_kind":"forgejo","owner":"team","repo":"app","issue_number":7,
                "issue_url":"https://forge.test/issues/7","project":p,"provider":"local","model":"fixture","classification":"confirmed","confidence":0.9,
                "status":"approved","comment_url":"","created_at":"2026-10-06T12:00:00Z","updated_at":"2026-10-06T12:02:00Z",
                "events":[{"id":"triage-event","run_id":"triage-one","at":"2026-10-06T12:02:00Z","kind":"triage.approved","actor":"Owner",
                    "project":p,"ticket":ticket,"source":"https://forge.test/issues/7","detail":"Approved bounded finding"}]}),
            );
            persist::<crate::sandbox_verify::VerifyRecord>(
                &paths.verification,
                p,
                verify_fixture(p),
            );
        }
        let rows = collect(&ps, &paths);
        assert_eq!(
            rows.iter().filter(|r| r.is_ok()).count(),
            8,
            "each real source must project for both registered projects"
        );
        assert!(Replay::default().capture(&ps, rows).is_empty());
        let journal = dir
            .join("ONE")
            .join(super::super::path("2026-10-06T12:00:00Z"));
        let saved = read(&journal).unwrap();
        let records = entries(&saved);
        assert_eq!(records.len(), 4);
        assert!(records
            .iter()
            .any(|e| e.actor == "nautbot" && e.content.contains("Started approved scope")));
        assert!(records.iter().any(
            |e| e.actor == "xNAUT coordinator" && e.content.contains("Missing boundary check")
        ));
        assert!(records
            .iter()
            .any(|e| e.actor == "Owner" && e.content.contains("Approved bounded finding")));
        assert!(!saved.contains("TWO-1"));
        assert!(!dir.join("FOREIGN").exists());
        std::fs::write(
            &journal,
            format!("{saved}\nOwner note: preserve compatibility.\n"),
        )
        .unwrap();
        let mut newer = verify_fixture("ONE");
        newer["updated_at"] = json!("2026-10-07T00:01:00Z");
        newer["error"] = json!("New check evidence arrived while still running");
        persist::<crate::sandbox_verify::VerifyRecord>(&paths.verification, "ONE", newer);
        assert!(Replay::default()
            .capture(&ps, collect(&ps, &paths))
            .is_empty());
        assert!(Replay::default()
            .capture(&ps, collect(&ps, &paths))
            .is_empty());
        let original = read(&journal).unwrap();
        assert_eq!(entries(&original).len(), 4);
        assert!(original.contains("Owner note: preserve compatibility."));
        let next = read(
            &dir.join("ONE")
                .join(super::super::path("2026-10-07T00:01:00Z")),
        )
        .unwrap();
        assert_eq!(entries(&next).len(), 1);
        assert!(next.contains("New check evidence arrived"));
        let evidence_dir = dir.join("ONE/Development/evidence/journal");
        assert_eq!(std::fs::read_dir(&evidence_dir).unwrap().count(), 5);
        assert!(std::fs::read_dir(&evidence_dir)
            .unwrap()
            .flatten()
            .any(|f| read(&f.path())
                .unwrap()
                .contains("New check evidence arrived")));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn source_errors_are_visible_without_foreign_content_or_filename_leaks() {
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        let paths = sources(&dir);
        std::fs::create_dir_all(&paths.groups).unwrap();
        std::fs::write(
            paths.groups.join("private-foreign-name.json"),
            "private-foreign-content",
        )
        .unwrap();
        std::fs::write(
            paths.groups.join("foreign.json"),
            r#"{"plan":{"project":"FOREIGN"},"private":"foreign secret"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(&paths.transfers).unwrap();
        std::fs::write(
            paths.transfers.join("bad.json"),
            r#"{"project":"ONE","private":"known secret"}"#,
        )
        .unwrap();
        std::fs::write(&paths.verification, "not a directory").unwrap();
        let ps = vec![project(&dir, "ONE")];
        let errors = Replay::default().capture(&ps, collect(&ps, &paths));
        assert_eq!(errors.len(), 3);
        let text = errors.join(" ");
        assert!(text.contains("swarm history record is unreadable"));
        assert!(text.contains("ONE: review history"));
        assert!(text.contains("verification history is unavailable"));
        for secret in [
            "private-foreign-name",
            "private-foreign-content",
            "foreign secret",
            "known secret",
        ] {
            assert!(!text.contains(secret));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn saved_activity_metadata_and_evidence_redact_actor_credentials() {
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(uuid::Uuid::new_v4().to_string());
        let ps = vec![project(&dir, "ONE")];
        let value = json!({"id":"secret","at":"2026-10-06T12:00:00Z","state":"tracking","actor":"nautbot api_key=do-not-preserve","reason":"password=also-private"});
        assert!(Replay::default()
            .capture(
                &ps,
                history("ONE", "ONE-1", "run", "swarm", &json!([value]))
            )
            .is_empty());
        let journal = read(
            &dir.join("ONE")
                .join(super::super::path("2026-10-06T12:00:00Z")),
        )
        .unwrap();
        assert!(!journal.contains("do-not-preserve"));
        assert!(!journal.contains("also-private"));
        assert!(journal.contains("[redacted]"));
        for entry in std::fs::read_dir(dir.join("ONE/Development/evidence/journal"))
            .unwrap()
            .flatten()
        {
            let evidence = read(&entry.path()).unwrap();
            assert!(!evidence.contains("do-not-preserve"));
            assert!(!evidence.contains("also-private"));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn phase_links_follow_review_and_author_successors_without_changing_operator() {
        let values = json!([
            {"id":"finding","at_ms":1791288000000i64,"state":"changes_requested","actor":"xNAUT coordinator","review_child":"review-one","predecessor_run_id":"author-original"},
            {"id":"repair","at_ms":1791288001000i64,"state":"repair_running","actor":"xNAUT coordinator","review_child":"review-one","author_child":"repair-one","predecessor_run_id":"author-original"},
            {"id":"blocked","at_ms":1791288002000i64,"state":"blocked","actor":"xNAUT coordinator","review_child":"review-one","author_child":"repair-one","predecessor_run_id":"author-original"},
            {"id":"published","at_ms":1791288003000i64,"state":"pending","actor":"xNAUT coordinator","author_child":"repair-one","predecessor_run_id":"repair-one"},
            {"id":"review","at_ms":1791288004000i64,"state":"running","actor":"xNAUT coordinator","review_child":"review-two","author_child":"repair-one","predecessor_run_id":"repair-one"},
            {"id":"ready","at_ms":1791288005000i64,"state":"ready","actor":"xNAUT coordinator","review_child":"review-two","author_child":"repair-one","predecessor_run_id":"repair-one"}
        ]);
        let rows = history("ONE", "ONE-1", "author-original", "review", &values)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.entry.run_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "review-one",
                "repair-one",
                "repair-one",
                "repair-one",
                "review-two",
                "review-two"
            ]
        );
        assert!(rows.iter().all(|r| r.entry.actor == "xNAUT coordinator"));
        assert!(rows[1]
            .entry
            .content
            .contains("Linked role:** Author repair"));
        assert!(rows[1]
            .entry
            .content
            .contains("Predecessor run: `author-original`"));
        assert!(rows[4]
            .entry
            .content
            .contains("Linked role:** Independent review"));
        assert_eq!(rows[4].evidence["author_child"], "repair-one");
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
