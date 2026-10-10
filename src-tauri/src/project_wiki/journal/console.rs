//! XNAUT-489: read-only Journal console; joins use project/ticket/run identity.
//! No new lifecycle store, remote probes, or admission decisions live here.
use super::super::*;
use crate::run_control::{RunManifest, RunState};
use std::collections::{HashSet, VecDeque};
use std::io::BufRead;

const ACTION_LIMIT: usize = 500;

fn actions(
    input: impl BufRead, project: &str, day: &str, runs: &[RunManifest], before: Option<usize>,
) -> Result<Value, String> {
    let ids: HashSet<_> = runs.iter().filter(|r| r.project == project).map(|r| r.run_id.as_str()).collect();
    let prefix = format!("{project}-");
    let mut rows = VecDeque::new();
    let mut total = 0;
    let mut eligible = 0;
    let mut dates = std::collections::BTreeSet::new();
    for (index, line) in input.lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let Ok(mut e) = serde_json::from_str::<crate::ledger::Entry>(&line) else { continue; };
        // An explicit ticket takes precedence over a conflicting run reference.
        let belongs = if e.ticket.is_empty() { ids.contains(e.run_id.as_str()) }
            else { e.ticket.strip_prefix(&prefix).is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit())) };
        if !belongs { continue; }
        let Ok(at) = chrono::DateTime::parse_from_rfc3339(&e.at) else { continue; };
        let date = at.with_timezone(&chrono::Utc).format("%Y-%m-%d").to_string();
        dates.insert(date.clone());
        if date != day { continue; }
        total += 1;
        if before.is_some_and(|cursor| index >= cursor) { continue; }
        eligible += 1;
        e.detail = redact(&e.detail);
        rows.push_back((index, e));
        if rows.len() > ACTION_LIMIT { rows.pop_front(); }
    }
    let next_before = if eligible > ACTION_LIMIT { rows.front().map(|(index,_)| *index) } else { None };
    Ok(json!({"entries":rows.into_iter().rev().map(|(_,e)|e).collect::<Vec<_>>(),"total":total,"limit":ACTION_LIMIT,"dates":dates,"next_before":next_before,"before":before}))
}

fn deployed(project: &str, runs: &[RunManifest]) -> Vec<Value> {
    runs.iter().filter(|r| r.project == project && !r.user_conversation
        && !r.state.terminal() && r.state != RunState::Requested && !r.admission_refused)
        .map(|r| json!({"run_id":r.run_id,"ticket":r.ticket,"agent":r.agent_handle,
            "destination":r.remote_env.as_deref().unwrap_or("local"),"machine":r.machine,
            "state":r.state,"pty_session":r.pty_session,"zellij_session":r.zellij_session,
            "observed_at":r.last_seen_at,"signal":redact(&super::signal_summary(&r.last_signal))}))
        .collect()
}

pub(super) fn read(p: &Project, rel: &str, runs: &[RunManifest], before: Option<usize>) -> Value {
    let day = rel.rsplit('/').next().unwrap_or("").trim_end_matches(".md");
    let (activity, error) = match std::fs::File::open(crate::ledger::path()) {
        Ok(file) => match actions(std::io::BufReader::new(file), &p.key, day, runs, before) {
            Ok(value) => (value, None),
            Err(_) => (Value::Null, Some("Project actions could not be read.")),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (json!({"entries":[],"total":0,"dates":[]}), None),
        Err(_) => (Value::Null, Some("Project actions could not be read.")),
    };
    let deployed = deployed(&p.key, runs);
    // Metadata only: opening a handoff uses the existing Wiki reader on demand.
    let root = Path::new(&p.vault_path);
    let mut handoffs = Vec::new();
    if let Ok(dir) = safe(root, "Development/handoffs") { walk(root, &dir, &mut handoffs); }
    handoffs.sort_by(|a,b| b["path"].as_str().cmp(&a["path"].as_str()));
    json!({"project":p.key,"activity":activity,"error":error,"deployed":deployed,"handoffs":handoffs})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(project: &str, id: &str) -> RunManifest {
        let mut r = RunManifest::requested("pi", "pi", "/fixture", Some(format!("{project}-1")), None, &[], 1);
        r.run_id = id.into(); r.state = RunState::Running;
        r.remote_env = Some("exe-dev".into()); r.pty_session = Some("exact-session".into());
        r.last_signal = "api_key=private-value".into();
        r
    }
    #[test]
    fn journal_workers_require_project_identity_and_exclude_unlaunched_or_finished_work() {
        let active = run("DEMO", "live");
        let mut pending = run("DEMO", "pending"); pending.state = RunState::Requested;
        let mut failed = run("DEMO", "failed"); failed.state = RunState::Failed;
        let mut refused = run("DEMO", "refused"); refused.admission_refused = true;
        let mut conversation = run("DEMO", "chat"); conversation.user_conversation = true;
        let rows = deployed("DEMO", &[active,pending,failed,refused,conversation,run("OTHER","foreign")]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["run_id"], "live");
        assert_eq!(rows[0]["pty_session"], "exact-session");
        assert_eq!(rows[0]["destination"], "exe-dev");
        assert!(!rows[0].to_string().contains("private-value"));
    }
    #[test]
    fn journal_actions_never_infer_a_project_from_an_agent_or_override_a_foreign_ticket() {
        let rows = [
            json!({"run_id":"known","ticket":"","kind":"adopted","agent":"pi","at":"2026-10-10T00:00:00Z"}),
            json!({"run_id":"known","ticket":"OTHER-1","kind":"adopted","agent":"pi","at":"2026-10-10T00:00:00Z"}),
            json!({"run_id":"unknown","ticket":"","kind":"adopted","agent":"pi","at":"2026-10-10T00:00:00Z"}),
        ];
        let text = rows.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
        let value = actions(text.as_bytes(), "DEMO", "2026-10-10", &[run("DEMO","known")], None).unwrap();
        assert_eq!(value["total"], 1);
        assert_eq!(value["entries"][0]["run_id"], "known");
    }
    #[test]
    fn journal_actions_scope_before_limit_and_keep_old_dates() {
        let mut lines = Vec::new();
        for i in 0..600 {
            lines.push(json!({"at":"2026-10-09T12:00:00Z","kind":"dispatched","agent":"pi","ticket":"DEMO-1","detail":format!("row {i}")}));
        }
        lines.push(json!({"at":"2026-10-10T12:00:00Z","kind":"blocked","agent":"pi","ticket":"DEMO-2","detail":"api_key=secret-value"}));
        for ticket in ["OTHER-1", "DEMO-X", "DEMOX-1", ""] {
            lines.push(json!({"at":"2026-10-09T12:00:00Z","kind":"blocked","agent":"pi","ticket":ticket,"detail":"foreign"}));
        }
        let text = lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
        let old = actions(text.as_bytes(), "DEMO", "2026-10-09", &[], None).unwrap();
        assert_eq!(old["total"],600);
        let older = actions(text.as_bytes(), "DEMO", "2026-10-09", &[], Some(old["next_before"].as_u64().unwrap() as usize)).unwrap();
        assert_eq!(older["entries"].as_array().unwrap().len(),100);
        assert_eq!(older["entries"][0]["detail"],"row 99");
        assert!(older["next_before"].is_null());
        assert_eq!(old["entries"].as_array().unwrap().len(),500);
        assert_eq!(old["entries"][0]["detail"],"row 599");
        assert_eq!(old["dates"].as_array().unwrap().len(),2);
        assert!(!old.to_string().contains("foreign"));
        let today = actions(text.as_bytes(), "DEMO", "2026-10-10", &[], None).unwrap();
        assert_eq!(today["total"],1);
        assert!(!today.to_string().contains("secret-value"));
    }
}
