// ABOUTME: Collects the agent-work records xNAUT already writes to disk, so the
// ABOUTME: work report is built from what happened rather than from keystrokes.

// XNAUT-267. The owner tested the recorder and said "the report is empty". The
// rendering was fine; the sourcing was the whole bug. `worklog_log` has exactly
// one caller in the app, `worklogAutoLog` in app.js, reached only when a HUMAN
// presses Enter in a terminal that autocomplete is tracking. An agent never
// touches that path: the wake writes bracketed paste straight to the PTY, and a
// launched agent's own work is tool calls inside its own process, so a
// command-shaped recorder has nothing to record. Every saved session on the
// owner's machine says the same thing, `"entries": []` beside a window hours
// wide.
//
// So this is a re-source, not a repair. Everything the report wanted is already
// on disk in five other places, days of it, which is why the report also works
// backwards over windows that predate this code. Each reader reports either how
// much it found or WHY it found nothing: a source that is empty and silent
// looks exactly like a healthy one, and that confusion is the failure the whole
// sprint exists to remove.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One thing that happened, from whichever file recorded it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    /// RFC3339, UTC.
    pub at: String,
    /// ledger | receipts | runs | verify | pm | commands
    pub source: String,
    /// The agent handle, or "you" for a typed command.
    pub actor: String,
    pub kind: String,
    pub ticket: String,
    pub detail: String,
}

/// What one source contributed, and when it contributed nothing, why not.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceNote {
    pub source: String,
    pub count: usize,
    /// Empty while `count > 0`. Otherwise the reason, in words.
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sources {
    /// Newest last, so the report reads like a day.
    pub activity: Vec<Activity>,
    pub notes: Vec<SourceNote>,
    /// Everything the readers found BEFORE `MAX_ROWS` was applied. Equal to
    /// `activity.len()` unless the window was wide enough to trim, and the only
    /// place the true figure survives, so the report can state it.
    pub total: usize,
}

/// What to collect. Four fields rather than four arguments because every caller
/// sets all of them and three of them are easy to swap by accident.
pub struct Query<'a> {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    /// The PM control repo, when Project Management is on. `None` is normal.
    pub pm_repo: Option<PathBuf>,
    /// The session whose commands the report already lists in its own table;
    /// its entries are skipped here so nothing appears twice.
    pub skip_session: Option<&'a str>,
}

/// A wide window over a busy machine can hold tens of thousands of rows and no
/// one reads those. Newest kept.
const MAX_ROWS: usize = 1000;

// ---- where the sources live -------------------------------------------------

/// One override for the whole set, so a test can point the collector at a
/// scratch tree instead of the owner's real records.
fn root() -> PathBuf {
    if let Some(over) = std::env::var("XNAUT_WORKLOG_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return PathBuf::from(over);
    }
    crate::loop_acceptance::platform_config_dir().unwrap_or_default().join("xnaut")
}

fn overridden() -> bool {
    std::env::var("XNAUT_WORKLOG_ROOT")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

fn ledger_path() -> PathBuf {
    // The writer honours XNAUT_LEDGER_PATH, so the reader has to as well or the
    // report silently misses a relocated log.
    if !overridden() {
        if let Ok(path) = std::env::var("XNAUT_LEDGER_PATH") {
            return PathBuf::from(path);
        }
    }
    root().join("agent-ledger.jsonl")
}

/// Borrowed from `evidence`, so `XNAUT_EVIDENCE_DIR` keeps meaning one thing
/// and the blobs beside the log resolve with it.
fn receipts_path() -> PathBuf {
    crate::evidence::log_path()
}

fn verify_dir() -> PathBuf {
    root().join("sandbox-verify").join("records")
}

/// Run captures are the one source NOT under the app support dir: `script(1)`
/// writes them where `agents::run_dir` puts them, `~/.config/xnaut/agent-runs`,
/// which on macOS is a different directory from `crate::loop_acceptance::platform_config_dir()`.
fn runs_dir() -> PathBuf {
    if overridden() {
        return root().join("agent-runs");
    }
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("xnaut")
        .join("agent-runs")
}

/// The saved sessions, wherever the writer put them. Borrowed rather than
/// re-derived so `XNAUT_WORKLOG_DIR` keeps meaning one thing.
fn sessions_dir() -> PathBuf {
    crate::worklog::worklog_dir()
}

// ---- collection -------------------------------------------------------------

/// Read every source over the window. Never fails: a source that cannot be read
/// becomes a note saying so, because the alternative is an error page where a
/// partial report would have done.
pub fn collect(query: &Query) -> Sources {
    let mut sources = Sources::default();
    {
        let mut take = |name: &str, read: Result<Vec<Activity>, String>| match read {
            Ok(found) => {
                let note = if found.is_empty() {
                    format!("nothing recorded in this window ({})", describe(name))
                } else {
                    String::new()
                };
                sources.notes.push(SourceNote {
                    source: name.to_string(),
                    count: found.len(),
                    note,
                });
                sources.activity.extend(found);
            }
            Err(error) => sources.notes.push(SourceNote {
                source: name.to_string(),
                count: 0,
                note: format!("could not be read: {error}"),
            }),
        };

        take("ledger", ledger_activity(query));
        take("receipts", receipt_activity(query));
        take("runs", run_activity(query));
        take("verify", verify_activity(query));
        take("pm", pm_activity(query));
        take("commands", command_activity(query));
    }

    sources.activity.sort_by(|a, b| a.at.cmp(&b.at));
    sources.total = sources.activity.len();
    // The cap is reasonable; a 380 KB report is already large. Being quiet
    // about it is not. A rig window on 2026-08-31 held 1117 receipts + 288
    // ledger + 21 runs = 1426 records, rendered exactly 1000, dropped the
    // oldest 426, and the only trace was a row labelled "all" in the Sources
    // table that never named the true total. The tester grepped for a
    // truncation notice and found nothing he recognised as one.
    if sources.total > MAX_ROWS {
        let dropped = sources.total - MAX_ROWS;
        sources.activity.drain(..dropped);
        sources.notes.push(SourceNote {
            source: "all".to_string(),
            count: MAX_ROWS,
            note: format!(
                "showing the most recent {MAX_ROWS} of {} events; the oldest {dropped} are not in this report. Narrow the window to see them.",
                sources.total
            ),
        });
    }
    sources
}

/// What a reader looks at, said plainly, so an empty row is actionable rather
/// than just empty.
fn describe(source: &str) -> &'static str {
    match source {
        "ledger" => "agent-ledger.jsonl, dispatch lifecycle",
        "receipts" => "evidence/execution.jsonl, every tool call",
        "runs" => "agent-runs captures",
        "verify" => "sandbox-verify records",
        "pm" => "control repo events",
        _ => "recorded terminal commands",
    }
}

fn parse_at(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

fn in_window(query: &Query, at: &DateTime<Utc>) -> bool {
    *at >= query.from && *at <= query.to
}

fn short(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    trimmed.chars().take(max).collect::<String>() + "…"
}

/// A missing file is not a failure. Nothing has been written yet is a perfectly
/// ordinary state and it reads as "nothing recorded", not "unreadable".
fn read_optional(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(body) => Ok(body),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

// ---- the readers ------------------------------------------------------------

/// Dispatches, nudges, adoptions and sweeps: session lifecycle, NOT work.
///
/// A rig build on 2026-08-31 added forty-three tool calls to the evidence chain
/// and exactly zero lines here (240 before, 240 after), and 156 of those 240
/// are `sweep_failed`. So this is context for the timeline, what was asked of
/// whom and when; the work itself comes from `receipts`.
fn ledger_activity(query: &Query) -> Result<Vec<Activity>, String> {
    let body = read_optional(&ledger_path())?;
    Ok(body
        .lines()
        .filter_map(|line| serde_json::from_str::<crate::ledger::Entry>(line).ok())
        .filter_map(|entry| {
            let at = parse_at(&entry.at)?;
            in_window(query, &at).then(|| Activity {
                at: at.to_rfc3339(),
                source: "ledger".into(),
                actor: entry.agent.clone(),
                kind: entry.kind.clone(),
                ticket: entry.ticket.clone(),
                detail: short(&entry.detail, 160),
            })
        })
        .collect())
}

/// The chained execution record: one line per tool call and per model call,
/// written with no action from the agent. This is the richest source there is
/// and until now nothing surfaced it, which is the other half of why the report
/// looked empty. A rig session on 2026-08-31 produced 43 records covering four
/// commits and thirty-nine tests, all of it reconstructible from here alone.
///
/// The arguments are kept IN FULL in a content-addressed blob beside the log,
/// not just hashed, so the literal `git commit` line comes back. Reading them
/// is best-effort: a shredded session or a missing blob leaves the tool name,
/// which is still a real row.
fn receipt_activity(query: &Query) -> Result<Vec<Activity>, String> {
    let body = read_optional(&receipts_path())?;
    Ok(body
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|record| {
            let at = parse_at(record.get("recorded_at")?.as_str()?)?;
            if !in_window(query, &at) {
                return None;
            }
            let kind = record["kind"].as_str().unwrap_or("record").to_string();
            let actor = record["actor"]["agent"]
                .as_str()
                .or_else(|| record["executor_id"].as_str())
                .unwrap_or("unknown")
                .to_string();
            // A tool call names its tool; a model call names its model. Both
            // carry the decision they were allowed under, which is the part a
            // client asking "prove it" wants.
            let subject = record["tool"]["name"]
                .as_str()
                .or_else(|| record["model"].as_str())
                .unwrap_or("");
            let mut detail = subject.to_string();
            if let Some(args) = tool_arguments(&record) {
                detail = format!("{detail}: {args}");
            }
            let outcome = record["outcome"]["decision"].as_str().unwrap_or("");
            if !outcome.is_empty() {
                detail = format!("{detail} ({outcome})");
            }
            Some(Activity {
                at: at.to_rfc3339(),
                source: "receipts".into(),
                actor,
                kind,
                ticket: String::new(),
                detail: short(&detail, 200),
            })
        })
        .collect())
}

/// The one field of a tool's arguments a person would recognise it by.
///
/// Ordered, not merged: a `Bash` row wants its command, an `Edit` row wants the
/// file. Printing the whole argument object would put a file's entire new
/// contents in a table cell.
const ARGUMENT_KEYS: [&str; 8] = [
    "command",
    "file_path",
    "path",
    "pattern",
    "query",
    "prompt",
    "url",
    "description",
];

fn tool_arguments(record: &serde_json::Value) -> Option<String> {
    let session = record["session_id"].as_str()?;
    let hash = record["tool"]["args_hash"].as_str()?;
    let path = crate::evidence::blob_path(session, hash);
    let bytes = crate::seal::read_blob(session, &path).ok()?;
    let args: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    ARGUMENT_KEYS
        .iter()
        .find_map(|key| args.get(key).and_then(|value| value.as_str()))
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|value| !value.is_empty())
}

/// Every launched agent tees its session to a capture file, and the file is
/// evidence that a run existed. Its CONTENT is not read here and must not be:
/// despite the `.jsonl` name it is a raw ANSI screen-scrape of the pane, and a
/// tester on 2026-08-31 found `merkle_root` in one and briefly took it for a
/// work log. It was there because he had typed the word into a grep. Anything
/// matched in these bytes may simply be something an agent looked at.
///
// ponytail: span comes from the file's created/modified stamps, not from
// parsing the capture. A file copied or touched reports the wrong span. Parsing
// a 1.4 MB ANSI stream per run to do better buys nothing the evidence chain
// does not already give properly.
fn run_activity(query: &Query) -> Result<Vec<Activity>, String> {
    let dir = runs_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{}: {error}", dir.display())),
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let ended: DateTime<Utc> = match meta.modified() {
            Ok(stamp) => stamp.into(),
            Err(_) => continue,
        };
        let started: DateTime<Utc> = meta.created().map(Into::into).unwrap_or(ended);
        // A run that straddles the window still belongs in it; a report of a
        // morning must show the agent that was already going at nine.
        if ended < query.from || started > query.to {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("run");
        found.push(Activity {
            at: started.max(query.from).to_rfc3339(),
            source: "runs".into(),
            actor: agent_from_run_name(name),
            kind: "run".into(),
            ticket: String::new(),
            detail: format!(
                "{name}, {} captured, last wrote {}",
                human_bytes(meta.len()),
                ended.format("%H:%M:%S")
            ),
        });
    }
    Ok(found)
}

/// Capture files are named `xnaut-<agent>-<id>`; the handle is the middle.
fn agent_from_run_name(stem: &str) -> String {
    stem.strip_prefix("xnaut-")
        .and_then(|rest| rest.split('-').next())
        .unwrap_or(stem)
        .to_string()
}

fn human_bytes(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.0} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// What was proven, with exit codes. The only source here that carries a
/// verdict rather than an activity.
fn verify_activity(query: &Query) -> Result<Vec<Activity>, String> {
    let dir = verify_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{}: {error}", dir.display())),
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<crate::sandbox_verify::VerifyRecord>(&body) else {
            continue;
        };
        let Some(at) = parse_at(&record.created_at) else {
            continue;
        };
        if !in_window(query, &at) {
            continue;
        }
        let steps = record
            .steps
            .iter()
            .map(|step| match step.exit_code {
                Some(code) => format!("{}={code}", step.name),
                None => format!("{}=skipped", step.name),
            })
            .collect::<Vec<_>>()
            .join(", ");
        found.push(Activity {
            at: at.to_rfc3339(),
            source: "verify".into(),
            actor: record.provider_kind.clone(),
            kind: format!("verify:{}", record.status),
            ticket: record.ticket_id.clone(),
            detail: short(&steps, 160),
        });
    }
    Ok(found)
}

/// Ticket transitions, when Project Management is on.
///
/// Three shapes are in the control repo on the owner's machine: the app writes
/// `{event, ticket, fields}`, the ticket tooling writes
/// `{type, ticket_id, summary}`, and older hand edits mix the two. Read
/// whichever keys are present; failing the whole source over one unfamiliar
/// file is how a report goes quiet for no reason.
fn pm_activity(query: &Query) -> Result<Vec<Activity>, String> {
    let Some(repo) = query.pm_repo.as_ref() else {
        return Err("Project Management is off, so ticket moves are not a source".into());
    };
    let dir = repo.join("events");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{}: {error}", dir.display())),
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&body) else {
            continue;
        };
        let raw_at = event["at"].as_str().or_else(|| event["timestamp"].as_str());
        let Some(at) = raw_at.and_then(parse_at) else {
            continue;
        };
        if !in_window(query, &at) {
            continue;
        }
        let kind = event["event"]
            .as_str()
            .or_else(|| event["type"].as_str())
            .unwrap_or("ticket.event")
            .to_string();
        let ticket = event["ticket"]
            .as_str()
            .or_else(|| event["ticket_id"].as_str())
            .or_else(|| event["subject"].as_str())
            .unwrap_or("")
            .to_string();
        let detail = event["summary"]
            .as_str()
            .map(str::to_string)
            .or_else(|| {
                event["fields"].as_array().map(|fields| {
                    let names: Vec<&str> = fields.iter().filter_map(|f| f.as_str()).collect();
                    format!("changed {}", names.join(", "))
                })
            })
            .unwrap_or_default();
        found.push(Activity {
            at: at.to_rfc3339(),
            source: "pm".into(),
            actor: event["project"].as_str().unwrap_or("pm").to_string(),
            kind,
            ticket,
            detail: short(&detail, 160),
        });
    }
    Ok(found)
}

/// Typed commands: kept as one input among six rather than the only one.
///
/// Reads the saved sessions so a report over a past window still shows what the
/// owner typed then, not only what was typed since the app was last started.
fn command_activity(query: &Query) -> Result<Vec<Activity>, String> {
    let dir = sessions_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{}: {error}", dir.display())),
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(session) = serde_json::from_str::<crate::worklog::WorkSession>(&body) else {
            continue;
        };
        if query.skip_session == Some(session.id.as_str()) {
            continue;
        }
        for command in &session.entries {
            let Some(at) = parse_at(&command.timestamp) else {
                continue;
            };
            if !in_window(query, &at) {
                continue;
            }
            found.push(Activity {
                at: at.to_rfc3339(),
                source: "commands".into(),
                actor: "you".into(),
                kind: "command".into(),
                ticket: String::new(),
                detail: short(&format!("{} ({})", command.command, command.directory), 160),
            });
        }
    }
    Ok(found)
}

/// One lock for every test that repoints the worklog env vars, shared with
/// `worklog::tests`. Two locks would not have helped: both modules move
/// `XNAUT_WORKLOG_DIR`, and cargo runs them on threads of one process.
///
/// That argument has a third module in it. `worklog` also repoints
/// `XNAUT_EVIDENCE_DIR`, which `evidence` and `seal` already take turns over
/// behind `evidence::DIR_LOCK` — a *different* lock, so the two groups did not
/// exclude each other. `seal` clearing the variable mid-report is what made
/// `a_report_that_drops_rows_says_so_and_says_how_many_there_were` read the
/// real evidence dir and count the wrong number of rows (2 of 4 sandbox runs,
/// 2026-09-05). One variable, one lock: this now hands out that same lock.
#[cfg(test)]
pub(crate) fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    crate::evidence::DIR_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Serialised, because these tests move process-wide env vars.
    fn scratch(name: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        let guard = env_guard();
        let dir =
            std::env::temp_dir().join(format!("xnaut-worklog-src-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XNAUT_WORKLOG_ROOT", &dir);
        std::env::set_var("XNAUT_WORKLOG_DIR", dir.join("worklogs"));
        std::env::set_var("XNAUT_EVIDENCE_DIR", dir.join("evidence"));
        (guard, dir)
    }

    fn window() -> (DateTime<Utc>, DateTime<Utc>) {
        (
            Utc::now() - chrono::Duration::hours(4),
            Utc::now() + chrono::Duration::minutes(1),
        )
    }

    fn at(minutes_ago: i64) -> String {
        (Utc::now() - chrono::Duration::minutes(minutes_ago)).to_rfc3339()
    }

    fn append(path: &Path, line: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut handle = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(handle, "{line}").unwrap();
    }

    fn query<'a>(from: DateTime<Utc>, to: DateTime<Utc>, pm: Option<PathBuf>) -> Query<'a> {
        Query {
            from,
            to,
            pm_repo: pm,
            skip_session: None,
        }
    }

    #[test]
    fn agent_work_reaches_the_report_although_nobody_typed_a_command() {
        // The whole ticket in one assertion: five sources that a human never
        // touches, and every one of them lands in the timeline.
        let (_guard, dir) = scratch("all-sources");
        let (from, to) = window();

        append(
            &dir.join("agent-ledger.jsonl"),
            &format!(
                r#"{{"at":"{}","kind":"dispatched","agent":"claude","ticket":"XNAUT-267","detail":"re-source the report"}}"#,
                at(90)
            ),
        );
        append(
            &dir.join("evidence").join("execution.jsonl"),
            &format!(
                r#"{{"recorded_at":"{}","kind":"tool_call","session_id":"s-1","actor":{{"agent":"claude"}},"tool":{{"name":"Edit","args_hash":"sha256:aaa"}},"outcome":{{"decision":"allow"}}}}"#,
                at(80)
            ),
        );
        std::fs::create_dir_all(dir.join("agent-runs")).unwrap();
        std::fs::write(
            dir.join("agent-runs").join("xnaut-claude-abc123.jsonl"),
            "captured bytes",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("sandbox-verify").join("records")).unwrap();
        std::fs::write(
            dir.join("sandbox-verify")
                .join("records")
                .join("v1.json"),
            format!(
                r#"{{"id":"v1","run_id":"r1","ticket_id":"XNAUT-267","project":"XNAUT","repo_path":"/tmp","provider_kind":"gitvm-cli","sandbox_id":"gitvm","public_url":"","status":"passed","steps":[{{"name":"test","command":"cargo test","exit_code":0,"log_tail":""}}],"log_dir":"","video_path":null,"created_at":"{}","updated_at":"{}"}}"#,
                at(70),
                at(70)
            ),
        )
        .unwrap();
        let pm = dir.join("control");
        std::fs::create_dir_all(pm.join("events")).unwrap();
        std::fs::write(
            pm.join("events").join("t.json"),
            format!(
                r#"{{"type":"ticket.updated","ticket_id":"XNAUT-267","project":"XNAUT","at":"{}","summary":"moved to review"}}"#,
                at(60)
            ),
        )
        .unwrap();

        let sources = collect(&query(from, to, Some(pm)));
        let seen: Vec<&str> = sources
            .activity
            .iter()
            .map(|item| item.source.as_str())
            .collect();
        for wanted in ["ledger", "receipts", "runs", "verify", "pm"] {
            assert!(seen.contains(&wanted), "{wanted} missing from {seen:?}");
        }
        assert!(
            sources
                .activity
                .iter()
                .any(|item| item.detail.contains("Edit")),
            "the tool call itself should be legible: {:?}",
            sources.activity
        );
        assert!(
            sources
                .activity
                .iter()
                .any(|item| item.kind == "verify:passed" && item.detail.contains("test=0")),
            "a verification carries its exit codes: {:?}",
            sources.activity
        );
    }

    #[test]
    fn a_tool_call_reports_what_it_actually_ran_not_just_its_name() {
        // The evidence chain stores arguments IN FULL in a blob beside the log,
        // content-addressed by args_hash. A tester first assumed only the hash
        // was kept; it is not, and the literal `git commit` line is what makes
        // this source worth a report at all.
        let (_guard, dir) = scratch("blob");
        append(
            &dir.join("evidence").join("execution.jsonl"),
            &format!(
                r#"{{"recorded_at":"{}","kind":"tool_call","session_id":"s-9","actor":{{"agent":"claude"}},"tool":{{"name":"Bash","args_hash":"sha256:beef"}}}}"#,
                at(20)
            ),
        );
        let blob = crate::evidence::blob_path("s-9", "sha256:beef");
        std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
        std::fs::write(
            &blob,
            r#"{"command":"git commit -m \"fix the report\"","description":"commit"}"#,
        )
        .unwrap();

        let (from, to) = window();
        let sources = collect(&query(from, to, None));
        let row = sources
            .activity
            .iter()
            .find(|item| item.source == "receipts")
            .expect("the tool call should be there");
        assert!(
            row.detail.contains("git commit -m"),
            "the literal command has to survive into the report: {}",
            row.detail
        );
    }

    #[test]
    fn work_outside_the_window_stays_outside_it() {
        // A report for this morning that quietly includes last week is not a
        // report, and a client would be billed for it.
        let (_guard, dir) = scratch("window");
        append(
            &dir.join("agent-ledger.jsonl"),
            &format!(
                r#"{{"at":"{}","kind":"dispatched","agent":"claude","ticket":"XNAUT-1","detail":"inside"}}"#,
                at(30)
            ),
        );
        append(
            &dir.join("agent-ledger.jsonl"),
            &format!(
                r#"{{"at":"{}","kind":"dispatched","agent":"claude","ticket":"XNAUT-2","detail":"outside"}}"#,
                at(60 * 24 * 7)
            ),
        );

        let (from, to) = window();
        let sources = collect(&query(from, to, None));
        let details: Vec<&str> = sources
            .activity
            .iter()
            .map(|item| item.detail.as_str())
            .collect();
        assert_eq!(details, vec!["inside"], "{:?}", sources.activity);
    }

    #[test]
    fn an_empty_window_says_why_it_is_empty_source_by_source() {
        // Silence that looks like health is the failure being removed. Every
        // source has to account for itself.
        let (_guard, _dir) = scratch("empty");
        let (from, to) = window();
        let sources = collect(&query(from, to, None));

        assert!(sources.activity.is_empty());
        for wanted in ["ledger", "receipts", "runs", "verify", "pm", "commands"] {
            let note = sources
                .notes
                .iter()
                .find(|note| note.source == wanted)
                .unwrap_or_else(|| {
                    panic!("{wanted} did not account for itself: {:?}", sources.notes)
                });
            assert_eq!(note.count, 0);
            assert!(
                !note.note.trim().is_empty(),
                "{wanted} was empty AND silent"
            );
        }
        let pm = sources
            .notes
            .iter()
            .find(|note| note.source == "pm")
            .unwrap();
        assert!(
            pm.note.contains("Project Management is off"),
            "the reason has to be the real one: {}",
            pm.note
        );
    }

    #[test]
    fn a_source_that_cannot_be_read_says_so_rather_than_reading_as_empty() {
        // "No rows" and "the disk refused" must never render the same way.
        let (_guard, dir) = scratch("unreadable");
        // A file where the reader expects a directory: a read that fails for a
        // reason other than absence.
        std::fs::write(dir.join("agent-runs"), "not a directory").unwrap();

        let (from, to) = window();
        let sources = collect(&query(from, to, None));
        let runs = sources
            .notes
            .iter()
            .find(|note| note.source == "runs")
            .unwrap();
        assert!(
            runs.note.contains("could not be read"),
            "an unreadable source must not read as empty: {}",
            runs.note
        );
    }

    #[test]
    fn typed_commands_are_one_source_among_the_others() {
        // Kept, not replaced. A saved session's commands still show up, and a
        // report that already prints one session's chain can leave it out.
        let (_guard, dir) = scratch("commands");
        std::fs::create_dir_all(dir.join("worklogs")).unwrap();
        std::fs::write(
            dir.join("worklogs").join("s1.json"),
            format!(
                r#"{{"id":"s1","client":"ACME","project":"Ledger","started":"{}","ended":null,"entries":[{{"timestamp":"{}","command":"cargo test","directory":"/repo","output_summary":null,"duration_ms":null,"hash":"h","prev_hash":"genesis"}}],"merkle_root":null,"active":true}}"#,
                at(50),
                at(40)
            ),
        )
        .unwrap();

        let (from, to) = window();
        let sources = collect(&query(from, to, None));
        assert!(
            sources
                .activity
                .iter()
                .any(|item| item.source == "commands" && item.detail.contains("cargo test")),
            "{:?}",
            sources.activity
        );

        let skipped = collect(&Query {
            from,
            to,
            pm_repo: None,
            skip_session: Some("s1"),
        });
        assert!(
            !skipped
                .activity
                .iter()
                .any(|item| item.source == "commands"),
            "the session the report already prints must not be listed twice"
        );
    }
}
