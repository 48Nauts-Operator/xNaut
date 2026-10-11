//! XNAUT-494: bounded, read-only lifecycle projection. PM and run stores remain authoritative.
use super::*;
use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, OnceLock};
const FILE_LIMIT: usize = 2500;
const EVENT_LIMIT: usize = 1500;
#[derive(Default)]
struct Cache {
    root: PathBuf,
    files: BTreeMap<PathBuf, (std::time::SystemTime, Value)>,
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
fn event_rows(repo: &Path) -> Result<(Vec<Value>, bool, usize), String> {
    let mut paths: Vec<_> = std::fs::read_dir(repo.join("events"))
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|s| s == "json"))
        .collect();
    // Native event filenames begin with their timestamp. Legacy names remain
    // available as evidence, but the bounded window is never called complete.
    paths.sort_by_key(|p| {
        (
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with(|c: char| c.is_ascii_digit())),
            p.clone(),
        )
    });
    paths.reverse();
    let limited = paths.len() > FILE_LIMIT;
    paths.truncate(FILE_LIMIT);
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(Cache::default()))
        .lock()
        .map_err(|_| "Lifecycle cache unavailable")?;
    if cache.root != repo {
        cache.root = repo.into();
        cache.files.clear();
    }
    let keep: HashSet<_> = paths.iter().cloned().collect();
    cache.files.retain(|p, _| keep.contains(p));
    let mut errors = 0;
    for path in paths {
        let Ok(meta) = std::fs::metadata(&path) else {
            errors += 1;
            cache.files.remove(&path);
            continue;
        };
        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        if cache.files.get(&path).is_some_and(|(m, _)| *m == modified) {
            continue;
        }
        if meta.len() > 256 * 1024 {
            errors += 1;
            cache.files.remove(&path);
            continue;
        }
        match std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .filter(Value::is_object)
        {
            Some(v) => {
                cache.files.insert(path, (modified, v));
            }
            None => {
                cache.files.remove(&path);
                errors += 1;
            }
        }
    }
    Ok((
        cache
            .files
            .iter()
            .map(|(path, (_, v))| {
                let mut v = v.clone();
                v["source"] = json!(format!(
                    "events/{}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                v
            })
            .collect(),
        limited,
        errors,
    ))
}
fn project_events(project: &str, ids: &HashSet<String>, rows: Vec<Value>) -> Vec<Value> {
    let mut events: Vec<_>=rows.into_iter().filter(|v| v["subject"].as_str().is_some_and(|s| ids.contains(s)))
        .map(|v|json!({"id":v["source"],"source":v["source"],"project":project,"ticket":v["subject"],"at":v["timestamp"],"kind":v["event"],"state":v["details"]["status"],"actor":v["details"]["owner"],"detail":redact(&v["details"].to_string()),"authority":"Recorded PM event"})).collect();
    events.sort_by(|a, b| a["at"].as_str().cmp(&b["at"].as_str()));
    events
}
pub(super) fn read(
    project: &str,
    runs: &[crate::run_control::RunManifest],
) -> Result<Value, String> {
    let repo = crate::project_management::repo_now()?;
    let tickets = crate::project_management::ticket_list_in(&repo, Some(project.into()))?;
    let ids: HashSet<_> = tickets
        .iter()
        .filter(|t| t.project == project)
        .map(|t| t.id.clone())
        .collect();
    let (rows, limited, errors) = event_rows(&repo)?;
    let mut events = project_events(project, &ids, rows);
    let mut links = Vec::new();
    for t in &tickets {
        if let Some(parent) = t.parent.as_ref().filter(|p| ids.contains(*p)) {
            links.push(json!({"from":parent,"to":t.id,"kind":"parent"}));
        }
    }
    let run_ids: HashSet<_> = runs
        .iter()
        .filter(|r| r.project == project)
        .map(|r| r.run_id.as_str())
        .collect();
    for r in runs.iter().filter(|r| {
        r.project == project
            && !r.user_conversation
            && r.ticket.as_ref().is_some_and(|t| ids.contains(t))
    }) {
        // A manifest is an observation, never an invented stage duration or verified outcome.
        events.push(json!({"id":format!("run:{}:{}",r.run_id,r.last_seen_at),"project":project,"ticket":r.ticket,"at":stamp(r.last_seen_at),"kind":"run.observed","state":r.state,"actor":r.agent_handle,"run_id":r.run_id,"detail":redact(&signal_summary(&r.last_signal)),"source":format!("{}.run.json",r.run_id),"authority":"Recorded run observation"}));
        if let Some(previous) = r
            .previous_run_id
            .as_deref()
            .filter(|id| run_ids.contains(id))
        {
            links.push(
                json!({"from":previous,"to":r.run_id,"kind":"continuation","ticket":r.ticket}),
            );
        }
    }
    events.sort_by(|a, b| b["at"].as_str().cmp(&a["at"].as_str()));
    let truncated = events.len() > EVENT_LIMIT;
    events.truncate(EVENT_LIMIT);
    let metadata:Vec<_>=tickets.into_iter().filter(|t|t.project==project).map(|t|json!({"id":t.id,"title":t.title,"release":t.release,"parent":t.parent,"created_at":t.created_at,"status":t.status})).collect();
    Ok(
        json!({"project":project,"tickets":metadata,"events":events,"links":links,"limited":limited||truncated,"unreadable":errors,"coverage":"Recent recorded PM events and latest run observations; gaps are not inferred."}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_rejects_invalid_files_and_reconciles_removed_sources() {
        let dir = std::env::temp_dir().join(format!("journal-lifecycle-{}", uuid::Uuid::new_v4()));
        let events = dir.join("events");
        std::fs::create_dir_all(&events).unwrap();
        std::fs::write(events.join("20261011-good.json"), r#"{"subject":"ONE-1"}"#).unwrap();
        std::fs::write(events.join("20261011-array.json"), "[]").unwrap();
        std::fs::write(events.join("20261011-invalid.json"), "not json").unwrap();
        std::fs::write(
            events.join("20261011-large.json"),
            vec![b' '; 256 * 1024 + 1],
        )
        .unwrap();
        let (rows, limited, errors) = event_rows(&dir).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!limited);
        assert_eq!(errors, 3);
        std::fs::remove_file(events.join("20261011-good.json")).unwrap();
        assert!(event_rows(&dir).unwrap().0.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn lifecycle_scope_and_redaction_do_not_promote_claims_to_verification() {
        let ids = HashSet::from(["ONE-1".to_string()]);
        let rows = vec![
            json!({"subject":"ONE-1","timestamp":"2026-10-11T08:00:00Z","source":"a.json","event":"ticket.updated","details":{"status":"done","password":"hidden"}}),
            json!({"subject":"OTHER-1","source":"b.json"}),
        ];
        let result = project_events("ONE", &ids, rows);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["state"], "done");
        assert_eq!(result[0]["authority"], "Recorded PM event");
        assert!(!result[0]["detail"].as_str().unwrap().contains("hidden"));
    }
}
