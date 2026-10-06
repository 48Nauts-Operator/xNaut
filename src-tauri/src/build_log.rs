// Build log — one durable, append-only record per build.
//
// "Every session is written to one master log you can always go back to."
//
// This is deliberately NOT Dozzle's model. Dozzle is the reference for the
// VIEWER — split sources, live tail, level filters, regex search — but its
// README is explicit that it "doesn't store any log files", and it points you at
// Loggly or Kibana for history. The requirement here is the opposite: the log
// must survive the run.
//
// It also cannot be built on the agent CLI's own transcripts. On 2026-08-09 every
// Guardian agent ran with CLAUDE_CODE_CHILD_SESSION=1 inherited from the app, so
// Claude Code disabled transcript saving and `~/.claude/projects` held nothing at
// all for that build. Whether history exists must not depend on how the app
// happened to be launched — so we write our own.
//
// JSONL, append-only, same discipline as audit.rs: a log that is read, mutated
// and written back can lose everything to one bad parse, whereas appending a line
// can cost at most the line being written. Reading skips what it cannot parse.
//
// What this replaces: `managerSay()` assigned a single string that the next event
// overwrote. The build's entire decision history existed for a few seconds and
// was then destroyed — which is why the planner failures were so hard to
// diagnose, and why three agents could be killed with no record of why.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

/// Severity. Kept to four so the filter pills stay countable and meaningful;
/// anything richer belongs in `kind`.
pub const LEVELS: [&str; 4] = ["debug", "info", "warn", "error"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Line number in the file, 0-based. Assigned on read, not stored — the file
    /// is append-only, so position IS identity, and it gives the UI a cursor to
    /// tail from without re-reading what it already has.
    #[serde(default)]
    pub seq: usize,
    /// RFC3339, UTC.
    pub ts: String,
    /// Epoch millis, so the UI can sort and diff without parsing dates.
    #[serde(default)]
    pub t: i64,
    /// One of LEVELS. Normalised on write; unknown input becomes "info".
    pub level: String,
    /// Who emitted it: "manager", "planner", "gate", "probe", or a slice id.
    pub source: String,
    /// Dotted semantic name, e.g. "slice.done", "worktree.escape". Lets the UI
    /// style an event without pattern-matching the prose.
    #[serde(default)]
    pub kind: String,
    /// The human-readable line.
    pub event: String,
    /// Structured payload; shape varies per kind.
    #[serde(default)]
    pub data: serde_json::Value,
}

/// Per-level totals for the filter pills. Counted over everything in the file,
/// NOT over the filtered slice — a pill that only counted what is already shown
/// would always read the same and could never tell you an error exists.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LevelCounts {
    pub debug: usize,
    pub info: usize,
    pub warn: usize,
    pub error: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogPage {
    pub events: Vec<LogEvent>,
    pub counts: LevelCounts,
    /// Every source seen in the file, sorted — the rail is built from this, so it
    /// must list a slice even when the current filter hides all of its lines.
    pub sources: Vec<String>,
    /// Total lines in the file, including ones this page filtered out.
    pub total: usize,
    /// Path on disk. Surfaced in the UI footer: a log you cannot find and grep
    /// is not really durable.
    pub path: String,
}

fn logs_dir() -> Option<PathBuf> {
    Some(
        crate::loop_acceptance::platform_config_dir()?
            .join("xnaut")
            .join("looms")
            .join("logs"),
    )
}

/// Build ids come from the UI, so they are treated as untrusted path input:
/// anything outside a conservative set is replaced, which makes traversal
/// (`../../`) impossible rather than merely unlikely.
fn safe_id(build_id: &str) -> String {
    let s: String = build_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .collect();
    let s = s.trim_matches(['-', '.']).to_string();
    if s.is_empty() { "build".into() } else { s }
}

fn log_path(build_id: &str) -> Option<PathBuf> {
    Some(logs_dir()?.join(format!("{}.jsonl", safe_id(build_id))))
}

fn normalise_level(level: &str) -> String {
    let l = level.trim().to_ascii_lowercase();
    if LEVELS.contains(&l.as_str()) { l } else { "info".into() }
}

/// Append one event. Never returns an error: logging must not be able to fail the
/// thing it is recording — the same rule audit.rs follows.
#[tauri::command]
pub fn build_log_append(
    build_id: String,
    level: String,
    source: String,
    event: String,
    kind: Option<String>,
    data: Option<serde_json::Value>,
) {
    let Some(path) = log_path(&build_id) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let now = chrono::Utc::now();
    let e = LogEvent {
        seq: 0,
        ts: now.to_rfc3339(),
        t: now.timestamp_millis(),
        level: normalise_level(&level),
        source: if source.trim().is_empty() { "manager".into() } else { source },
        kind: kind.unwrap_or_default(),
        event,
        data: data.unwrap_or(serde_json::Value::Null),
    };
    // Newlines would split one event into several unparseable lines.
    let Ok(line) = serde_json::to_string(&e) else { return };
    let line = line.replace('\n', " ");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

/// Parse a whole file into events, skipping unparseable lines.
///
/// Split out from the command so the filtering can be tested against fixture text
/// without touching the filesystem.
fn parse(body: &str) -> Vec<LogEvent> {
    body.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let l = l.trim();
            if l.is_empty() { return None; }
            let mut e: LogEvent = serde_json::from_str(l).ok()?;
            e.seq = i;
            Some(e)
        })
        .collect()
}

/// Every event in a log, oldest first. `build_log_read` above is the paged
/// reader for the viewer; callers that want the whole file (decisions.rs) use
/// this. A missing file is an empty log, not an error: nothing has happened yet.
pub fn read_events(build_id: &str) -> Vec<LogEvent> {
    let Some(path) = log_path(build_id) else {
        return Vec::new();
    };
    parse(&std::fs::read_to_string(path).unwrap_or_default())
}

fn count_levels(all: &[LogEvent]) -> LevelCounts {
    let mut c = LevelCounts { total: all.len(), ..Default::default() };
    for e in all {
        match e.level.as_str() {
            "debug" => c.debug += 1,
            "warn" => c.warn += 1,
            "error" => c.error += 1,
            _ => c.info += 1,
        }
    }
    c
}

/// Apply the viewer's filters. `query` is a plain case-insensitive substring —
/// regex is the UI's job to offer, but a bad pattern must not be able to break
/// reading the log, so the backend stays literal.
fn apply(
    all: Vec<LogEvent>,
    levels: &[String],
    source: Option<&str>,
    query: Option<&str>,
    after_seq: Option<usize>,
    limit: usize,
) -> Vec<LogEvent> {
    let want: Vec<String> = levels.iter().map(|l| l.to_ascii_lowercase()).collect();
    let q = query.map(|s| s.to_ascii_lowercase());
    let mut out: Vec<LogEvent> = all
        .into_iter()
        .filter(|e| after_seq.is_none_or(|a| e.seq > a))
        .filter(|e| want.is_empty() || want.contains(&e.level))
        .filter(|e| source.is_none_or(|s| s.is_empty() || e.source == s))
        .filter(|e| {
            q.as_ref().is_none_or(|q| {
                e.event.to_ascii_lowercase().contains(q) || e.source.to_ascii_lowercase().contains(q)
            })
        })
        .collect();
    // Keep the TAIL, not the head: a live viewer wants the newest, and truncating
    // from the front would hide the event you opened the log to see.
    if out.len() > limit {
        out = out.split_off(out.len() - limit);
    }
    out
}

/// Read one build's log. Counts and sources always describe the WHOLE file so the
/// filter UI can show you what you are currently hiding.
#[tauri::command]
pub fn build_log_read(
    build_id: String,
    levels: Option<Vec<String>>,
    source: Option<String>,
    query: Option<String>,
    after_seq: Option<usize>,
    limit: Option<usize>,
) -> Result<LogPage, String> {
    let path = log_path(&build_id).ok_or("no config dir")?;
    let body = std::fs::read_to_string(&path).unwrap_or_default();
    let all = parse(&body);
    let counts = count_levels(&all);
    let mut sources: Vec<String> = all.iter().map(|e| e.source.clone()).collect();
    sources.sort();
    sources.dedup();
    let events = apply(
        all,
        &levels.unwrap_or_default(),
        source.as_deref(),
        query.as_deref(),
        after_seq,
        limit.unwrap_or(500),
    );
    let total = counts.total;
    Ok(LogPage { events, counts, sources, total, path: path.to_string_lossy().into_owned() })
}

#[derive(Debug, Clone, Serialize)]
pub struct LogSummary {
    pub build_id: String,
    pub path: String,
    pub bytes: u64,
    pub modified_ms: i64,
}

/// Every build log on disk, newest first — this is the "go back to" half of the
/// requirement. Without it the master log is only useful for the run in front of
/// you, which is the failure mode of a live-only viewer.
#[tauri::command]
pub fn build_log_list() -> Result<Vec<LogSummary>, String> {
    let Some(dir) = logs_dir() else { return Ok(vec![]) };
    let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(vec![]) };
    let mut out: Vec<LogSummary> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| {
            let path = e.path();
            let md = e.metadata().ok()?;
            let modified_ms = md
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Some(LogSummary {
                build_id: path.file_stem()?.to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                bytes: md.len(),
                modified_ms,
            })
        })
        .collect();
    out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(seq: usize, level: &str, source: &str, event: &str) -> String {
        serde_json::to_string(&LogEvent {
            seq,
            ts: "2026-08-09T00:57:09Z".into(),
            t: 1_754_698_629_000 + seq as i64,
            level: level.into(),
            source: source.into(),
            kind: String::new(),
            event: event.into(),
            data: serde_json::Value::Null,
        })
        .unwrap()
    }

    fn fixture() -> String {
        [
            ev(0, "info", "planner", "plan parsed after 47s"),
            ev(1, "debug", "manager", "dag_step ready[3] waiting[1]"),
            ev(2, "warn", "wt-2", "wrote OUTSIDE its worktree"),
            ev(3, "debug", "gate", "gate_score_run 31/35"),
            ev(4, "error", "manager", "re-attach recovered 3 of 4 slices"),
        ]
        .join("\n")
    }

    #[test]
    fn a_malformed_line_costs_only_itself() {
        // The whole reason for JSONL over a rewritten document.
        let body = format!("{}\n{{ this is not json\n{}", ev(0, "info", "a", "first"), ev(2, "warn", "b", "third"));
        let all = parse(&body);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].event, "first");
        assert_eq!(all[1].event, "third");
        // seq is the line number, so the bad line leaves a visible gap rather
        // than silently renumbering everything after it.
        assert_eq!(all[1].seq, 2);
    }

    #[test]
    fn counts_describe_the_whole_file_not_the_filtered_view() {
        let all = parse(&fixture());
        let c = count_levels(&all);
        assert_eq!((c.debug, c.info, c.warn, c.error, c.total), (2, 1, 1, 1, 5));
        // Filtering to errors must not change what the pills report.
        let only_errors = apply(all.clone(), &["error".into()], None, None, None, 500);
        assert_eq!(only_errors.len(), 1);
        assert_eq!(count_levels(&all).total, 5);
    }

    #[test]
    fn filters_compose() {
        let all = parse(&fixture());
        assert_eq!(apply(all.clone(), &["debug".into()], None, None, None, 500).len(), 2);
        assert_eq!(apply(all.clone(), &[], Some("manager"), None, None, 500).len(), 2);
        assert_eq!(apply(all.clone(), &[], None, Some("WORKTREE"), None, 500).len(), 1); // case-insensitive
        assert_eq!(apply(all.clone(), &["debug".into()], Some("gate"), None, None, 500).len(), 1);
        // An empty source string means "no source filter", not "match nothing".
        assert_eq!(apply(all, &[], Some(""), None, None, 500).len(), 5);
    }

    #[test]
    fn after_seq_tails_without_re_reading() {
        let all = parse(&fixture());
        let tail = apply(all, &[], None, None, Some(2), 500);
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].seq, 3);
    }

    #[test]
    fn the_limit_keeps_the_newest_events() {
        // Truncating from the front would hide the error you opened the log for.
        let all = parse(&fixture());
        let page = apply(all, &[], None, None, None, 2);
        assert_eq!(page.len(), 2);
        assert_eq!(page[1].event, "re-attach recovered 3 of 4 slices");
    }

    #[test]
    fn unknown_levels_become_info_rather_than_a_new_pill() {
        assert_eq!(normalise_level("WARN"), "warn");
        assert_eq!(normalise_level("trace"), "info");
        assert_eq!(normalise_level(""), "info");
    }

    #[test]
    fn build_ids_cannot_escape_the_logs_directory() {
        assert_eq!(safe_id("../../etc/passwd"), "etc-passwd");
        assert_eq!(safe_id("build-guardian-1754698629"), "build-guardian-1754698629");
        assert_eq!(safe_id(""), "build");
        assert_eq!(safe_id("..."), "build");
    }
}
