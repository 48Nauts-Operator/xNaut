// XNAUT-331. xNAUT's own memory, as files.
//
// The mission from 2026-09-08: the machine keeps a memory of everything that
// happened, locally, readable, and reads it before it asks, dispatches or
// escalates. File-based was the ask, and for good reasons: the work vault is
// where every other record lives, a memory is a note you can open in Obsidian
// and diff in git, and nautdocs already indexes the vault, so search across
// everything comes for free. Engram is a third-party service and a client of
// this; it is never the source. Two earlier pieces were built toward it and
// are absorbed here: the inbox's prior-ask check, and XNAUT-313's incident
// join at escalation time.
//
// One note per memory under `work/<project>/Memory/`. The filename ends in a
// hash of the memory's source record, so remembering the same fact twice
// writes one file, and a backfill over the older stores is safe at every
// start. Reads scan the folder; a few thousand small notes is nothing.

use std::path::{Path, PathBuf};

pub const KINDS: [&str; 5] = ["learning", "incident", "decision", "ask", "fix"];

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Memory {
    pub path: String,
    /// Epoch millis.
    pub at: i64,
    pub kind: String,
    pub project: String,
    pub ticket: String,
    pub run_id: String,
    pub files: Vec<String>,
    pub text: String,
    pub cause: String,
    pub fix: String,
    /// `<store>:<id>` of the record this came from. The idempotency key.
    pub source: String,
}

#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub at: Option<i64>,
    pub kind: String,
    pub project: String,
    pub ticket: String,
    pub run_id: String,
    pub files: Vec<String>,
    pub text: String,
    pub cause: String,
    pub fix: String,
    pub source: String,
}

/// The work vault, where every project's memory folder lives.
pub fn default_root() -> Result<PathBuf, String> {
    crate::vault::vault_root("work")
}

fn project_dir(root: &Path, project: &str) -> PathBuf {
    let key = project.trim().to_ascii_lowercase();
    root.join(if key.is_empty() { "_unfiled".to_string() } else { key }).join("Memory")
}

fn short_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::digest(s.trim().as_bytes());
    h.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

fn slug(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let c = if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' };
        if c == '-' && out.ends_with('-') { continue; }
        out.push(c);
        if out.len() >= max { break; }
    }
    out.trim_matches('-').to_string()
}

fn date_of(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".into())
}

fn stamp_of(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_default()
}

/// Store one memory as a note. Returns the note's path, or the path of the
/// note already written from the same source: the same fact twice is one
/// memory. A second occurrence from a different source is a second note, and
/// that is information.
pub fn remember(root: &Path, entry: &Entry) -> Result<PathBuf, String> {
    if entry.text.trim().is_empty() {
        return Err("a memory with nothing to say is not stored".into());
    }
    if entry.source.trim().is_empty() {
        return Err("a memory must name its source".into());
    }
    if !KINDS.contains(&entry.kind.as_str()) {
        return Err(format!("unknown memory kind: {}", entry.kind));
    }
    let dir = project_dir(root, &entry.project);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let hash = short_hash(&entry.source);
    if let Some(existing) = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&format!("_{hash}.md"))))
    {
        return Ok(existing);
    }
    let at = entry.at.unwrap_or_else(crate::run_control::now_ms);
    let first = entry.text.trim().lines().next().unwrap_or("").trim();
    let name = format!(
        "{}_{}_{}_{}.md",
        date_of(at),
        entry.kind,
        if entry.ticket.trim().is_empty() { slug(first, 40) } else { slug(entry.ticket.trim(), 20) },
        hash
    );
    let path = dir.join(name);
    let mut body = String::new();
    body.push_str("---\n");
    body.push_str("Author: xNAUT\n");
    body.push_str(&format!("Last modified: {}\n", stamp_of(at)));
    body.push_str(&format!("kind: {}\n", entry.kind));
    body.push_str(&format!("project: {}\n", entry.project.trim()));
    body.push_str(&format!("ticket: {}\n", entry.ticket.trim()));
    body.push_str(&format!("run_id: {}\n", entry.run_id.trim()));
    body.push_str(&format!("at: {at}\n"));
    body.push_str(&format!("source: {}\n", entry.source.trim()));
    body.push_str("files:\n");
    for f in entry.files.iter().map(|f| f.trim()).filter(|f| !f.is_empty()) {
        body.push_str(&format!("  - {f}\n"));
    }
    body.push_str("---\n\n");
    body.push_str(&format!("# {}\n\n{}\n", if first.is_empty() { "memory" } else { first }, entry.text.trim()));
    if !entry.cause.trim().is_empty() {
        body.push_str(&format!("\n## Cause\n\n{}\n", entry.cause.trim()));
    }
    if !entry.fix.trim().is_empty() {
        body.push_str(&format!("\n## Fix\n\n{}\n", entry.fix.trim()));
    }
    let tmp = path.with_extension("md.tmp");
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Parse one note back. `None` for a file that is not a memory note.
pub fn parse(path: &Path, raw: &str) -> Option<Memory> {
    let rest = raw.strip_prefix("---\n")?;
    let (front, body) = rest.split_once("\n---\n")?;
    let mut m = Memory {
        path: path.to_string_lossy().into_owned(),
        at: 0, kind: String::new(), project: String::new(), ticket: String::new(),
        run_id: String::new(), files: Vec::new(), text: String::new(),
        cause: String::new(), fix: String::new(), source: String::new(),
    };
    let mut in_files = false;
    for line in front.lines() {
        if in_files {
            if let Some(f) = line.trim().strip_prefix("- ") { m.files.push(f.trim().to_string()); continue; }
            in_files = false;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim();
        match k.trim() {
            "kind" => m.kind = v.into(),
            "project" => m.project = v.into(),
            "ticket" => m.ticket = v.into(),
            "run_id" => m.run_id = v.into(),
            "at" => m.at = v.parse().unwrap_or(0),
            "source" => m.source = v.into(),
            "files" => in_files = true,
            _ => {}
        }
    }
    if m.kind.is_empty() || m.source.is_empty() { return None; }
    // Body: the text is everything after the title until the first section.
    let body = body.trim_start();
    let body = body.strip_prefix('#').map(|b| b.split_once('\n').map(|(_, r)| r).unwrap_or("")).unwrap_or(body);
    let mut section = "text";
    for line in body.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            section = match h.trim() { "Cause" => "cause", "Fix" => "fix", _ => "other" };
            continue;
        }
        let target = match section { "text" => &mut m.text, "cause" => &mut m.cause, "fix" => &mut m.fix, _ => continue };
        target.push_str(line);
        target.push('\n');
    }
    for s in [&mut m.text, &mut m.cause, &mut m.fix] { *s = s.trim().to_string(); }
    Some(m)
}

/// Every memory in the vault, or in one project's folder, newest first.
pub fn load(root: &Path, project: Option<&str>) -> Result<Vec<Memory>, String> {
    let dirs: Vec<PathBuf> = match project {
        Some(p) => vec![project_dir(root, p)],
        None => std::fs::read_dir(root)
            .map(|rd| rd.flatten().map(|e| e.path().join("Memory")).filter(|p| p.is_dir()).collect())
            .unwrap_or_default(),
    };
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("md") { continue; }
            if let Ok(raw) = std::fs::read_to_string(&p) {
                if let Some(m) = parse(&p, &raw) { out.push(m); }
            }
        }
    }
    out.sort_by_key(|m| std::cmp::Reverse(m.at));
    Ok(out)
}

fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-')
        .map(|w| w.trim_matches(|c: char| c == '.' || c == '-').to_ascii_lowercase())
        .filter(|w| w.len() > 2)
        .collect()
}

/// Every word of the query must appear somewhere in the memory. Plain and
/// predictable; nautdocs does the clever search across the whole vault.
pub fn search<'a>(memories: &'a [Memory], query: &str, limit: usize) -> Vec<&'a Memory> {
    let q = words(query);
    if q.is_empty() { return Vec::new(); }
    memories
        .iter()
        .filter(|m| {
            let hay = words(&format!("{} {} {} {} {}", m.text, m.cause, m.fix, m.files.join(" "), m.ticket));
            q.iter().all(|w| hay.iter().any(|h| h == w || h.ends_with(&format!("/{w}"))))
        })
        .take(limit)
        .collect()
}

/// One ticket's story, oldest first.
pub fn for_ticket<'a>(memories: &'a [Memory], ticket: &str) -> Vec<&'a Memory> {
    let mut v: Vec<&Memory> = memories.iter().filter(|m| m.ticket == ticket).collect();
    v.sort_by_key(|m| m.at);
    v
}

/// The newest memories naming any of these files, exact path match.
pub fn for_files<'a>(memories: &'a [Memory], files: &[String], limit: usize) -> Vec<&'a Memory> {
    memories
        .iter()
        .filter(|m| m.files.iter().any(|f| files.iter().any(|x| x.trim() == f)))
        .take(limit)
        .collect()
}

/// The block a dispatched agent reads before it starts: the last few things
/// learned on the files and ticket it is about to touch. Empty when nothing
/// is known, so the prompt carries no empty heading.
pub fn recall_block(memories: &[Memory], ticket: &str, files: &[String], limit: usize) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    let story = for_ticket(memories, ticket);
    for m in story.iter().rev().copied().chain(for_files(memories, files, limit)) {
        if !seen.insert(m.path.clone()) || lines.len() >= limit { continue; }
        let what = if !m.fix.is_empty() {
            format!("{} Closed by: {}", m.text, m.fix)
        } else if !m.cause.is_empty() {
            format!("{} Cause: {}", m.text, m.cause)
        } else {
            m.text.clone()
        };
        let what: String = what.chars().take(240).collect::<String>().replace('\n', " ");
        lines.push(format!("- [{}{}] {what}", m.kind, if m.ticket.is_empty() { String::new() } else { format!(" {}", m.ticket) }));
    }
    if lines.is_empty() { return String::new(); }
    format!("## What xNAUT remembers about this area\n\n{}\n", lines.join("\n"))
}

/// Remember without ever failing the caller. The moments that write here
/// are the moments the machine must not stumble on: a handback, a jury
/// decision, a failed verify, an integration. A memory that could not be
/// written is logged and lost; the handback is not.
pub fn note(entry: Entry) {
    // A test build never writes into the real vault. Tests that want the
    // memory redirect the vault like every other vault test does.
    #[cfg(test)]
    if crate::vault::test_vault().is_none() {
        return;
    }
    match default_root().and_then(|root| remember(&root, &entry)) {
        Ok(_) => {}
        Err(e) => eprintln!("[memory] not stored ({}: {}): {e}", entry.kind, entry.source),
    }
}

/// Ingest what predates the memory: run failures and jury outcomes from the
/// registry (through XNAUT-313's join), and every ticket's handback. Keyed by
/// source, so it is safe to run at every start; it only ever adds what is
/// missing. Returns how many notes were new.
pub fn backfill(root: &Path, registry: &Path, tickets: &[crate::project_management::TicketRecord]) -> Result<usize, String> {
    let before: std::collections::HashSet<String> = load(root, None)?.into_iter().map(|m| m.source).collect();
    let project_of = |ticket: &str| tickets.iter().find(|t| t.id == ticket).map(|t| t.project.clone()).unwrap_or_default();
    let mut new = 0;
    for i in crate::incidents::all(registry, tickets)? {
        // An escalation retired as superseded was noise by definition: the
        // same shape stamped hundreds of times by a bug in the gate, not by
        // anything about the ticket. It is not a memory.
        if i.signal.trim_start().starts_with("Superseded") { continue; }
        let source = format!("incident:{}:{}", i.ticket, i.run_id.clone().unwrap_or_else(|| short_hash(&i.signal)));
        if before.contains(&source) { continue; }
        let kind = if i.fix.is_some() { "fix" } else { "incident" };
        if remember(root, &Entry {
            at: Some(if i.at > 10_000_000_000 { i.at } else { i.at * 1000 }),
            kind: kind.into(), project: project_of(&i.ticket), ticket: i.ticket.clone(),
            run_id: i.run_id.clone().unwrap_or_default(), text: i.signal.clone(),
            cause: i.cause.clone().unwrap_or_default(), fix: i.fix.clone().unwrap_or_default(),
            source, ..Default::default()
        }).is_ok() { new += 1; }
    }
    for t in tickets {
        let Some(h) = &t.handback else { continue };
        if h.summary.trim().is_empty() { continue; }
        let source = format!("handback:{}:{}", t.id, h.run_id.clone().unwrap_or_else(|| h.submitted_at.clone()));
        if before.contains(&source) { continue; }
        if remember(root, &Entry {
            at: chrono::DateTime::parse_from_rfc3339(&h.submitted_at).ok().map(|d| d.timestamp_millis()),
            kind: "learning".into(), project: t.project.clone(), ticket: t.id.clone(),
            run_id: h.run_id.clone().unwrap_or_default(), files: h.files_changed.clone(),
            text: h.summary.clone(),
            cause: h.not_finished.clone().filter(|n| !n.trim().eq_ignore_ascii_case("nothing")).unwrap_or_default(),
            fix: h.commits.join(", "), source,
        }).is_ok() { new += 1; }
    }
    Ok(new)
}

/// Backfill in the background at app start; one ledger line if anything was new.
pub fn spawn_backfill() {
    std::thread::spawn(|| {
        let outcome = (|| -> Result<usize, String> {
            let root = default_root()?;
            let registry = crate::agents::registry_dir()?;
            let repo = crate::project_management::repo_now()?;
            let tickets = crate::project_management::ticket_list_in(&repo, None)?;
            backfill(&root, &registry, &tickets)
        })();
        match outcome {
            Ok(n) if n > 0 => crate::ledger::record("memory_backfilled", "xnaut", "", &format!("{n} memories written from earlier stores")),
            Ok(_) => {}
            Err(e) => eprintln!("[memory] backfill skipped: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xnaut-memory-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn learning(source: &str, ticket: &str, files: &[&str], text: &str) -> Entry {
        Entry {
            kind: "learning".into(), project: "XNAUT".into(), ticket: ticket.into(),
            files: files.iter().map(|s| s.to_string()).collect(), text: text.into(),
            source: source.into(), at: Some(1_789_000_000_000), ..Default::default()
        }
    }


    #[test]
    fn the_backfill_writes_each_handback_once_and_only_adds() {
        let root = scratch("backfill");
        let registry = root.join("registry"); std::fs::create_dir_all(&registry).unwrap();
        let mut t: crate::project_management::TicketRecord = serde_json::from_value(serde_json::json!({
            "id":"XNAUT-1","project":"XNAUT","title":"t","type":"feature","status":"done","priority":"high",
            "documentation":[],"tags":[],"release":"","body":"","source_id":"","revision":1,"created_at":"","updated_at":""
        })).unwrap();
        t.handback = Some(crate::handback::Handback { ticket:"XNAUT-1".into(), summary:"learned a thing".into(), files_changed: vec!["src/x.rs".into()], commits: vec!["abc".into()], submitted_at: "2026-09-11T10:00:00+00:00".into(), ..Default::default() });
        assert_eq!(backfill(&root, &registry, &[t.clone()]).unwrap(), 1);
        assert_eq!(backfill(&root, &registry, &[t.clone()]).unwrap(), 0, "second pass adds nothing");
        let all = load(&root, Some("XNAUT")).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].files, vec!["src/x.rs"]);
        assert_eq!(all[0].fix, "abc");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_memory_is_a_readable_note_and_reads_back_as_itself() {
        let root = scratch("note");
        let mut e = learning("handback:XNAUT-316", "XNAUT-316", &["src-tauri/src/subdivide.rs"], "A parent's handback waits for its children.\nDepth is capped at two.");
        e.cause = "the upward flow of a planner tree".into();
        e.fix = "8502cb3".into();
        let path = remember(&root, &e).unwrap();
        assert!(path.starts_with(root.join("xnaut").join("Memory")), "{}", path.display());
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("---\nAuthor: xNAUT\nLast modified: "), "vault frontmatter first: {raw}");
        assert!(raw.contains("# A parent's handback waits for its children."));
        assert!(raw.contains("## Cause") && raw.contains("## Fix"));
        let back = parse(&path, &raw).expect("a memory note parses");
        assert_eq!(back.ticket, "XNAUT-316");
        assert_eq!(back.files, vec!["src-tauri/src/subdivide.rs"]);
        assert_eq!(back.text, "A parent's handback waits for its children.\nDepth is capped at two.");
        assert_eq!(back.cause, "the upward flow of a planner tree");
        assert_eq!(back.fix, "8502cb3");
        assert_eq!(back.source, "handback:XNAUT-316");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_same_fact_from_the_same_source_is_one_note() {
        let root = scratch("idempotent");
        let a = remember(&root, &learning("verify:abc", "XNAUT-1", &[], "the sandbox ran out of disk")).unwrap();
        let b = remember(&root, &learning("verify:abc", "XNAUT-1", &[], "the sandbox ran out of disk, again")).unwrap();
        assert_eq!(a, b);
        let notes = load(&root, Some("XNAUT")).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].text, "the sandbox ran out of disk", "the first record stands; a repeat does not rewrite it");
        remember(&root, &learning("verify:def", "XNAUT-2", &[], "the sandbox ran out of disk")).unwrap();
        assert_eq!(load(&root, Some("XNAUT")).unwrap().len(), 2, "a second occurrence is a second note");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_memory_needs_words_a_source_and_a_known_kind() {
        let root = scratch("refusals");
        assert!(remember(&root, &learning("s:1", "X", &[], "   ")).is_err());
        assert!(remember(&root, &learning("", "X", &[], "words")).is_err());
        let mut odd = learning("s:2", "X", &[], "words");
        odd.kind = "rumour".into();
        assert!(remember(&root, &odd).is_err());
        assert!(load(&root, None).unwrap().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_and_recall_name_the_files_area_and_the_ticket_and_nothing_else() {
        let root = scratch("recall");
        remember(&root, &learning("h:1", "XNAUT-10", &["src/a.rs"], "on a.rs the lock must be taken before the read")).unwrap();
        let mut fixed = learning("h:2", "XNAUT-11", &["src/b.rs"], "b.rs leaked a temp file");
        fixed.fix = "661c4d7".into();
        remember(&root, &fixed).unwrap();
        remember(&root, &learning("h:3", "XNAUT-12", &["src/c.rs"], "c.rs is unrelated")).unwrap();
        let all = load(&root, None).unwrap();
        assert_eq!(all.len(), 3);

        let hits = search(&all, "temp file", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].ticket, "XNAUT-11");
        assert_eq!(search(&all, "src/a.rs", 10).len(), 1, "a path is a term");
        assert!(search(&all, "quantum", 10).is_empty());
        assert!(search(&all, "", 10).is_empty());

        let block = recall_block(&all, "XNAUT-11", &["src/a.rs".into()], 5);
        assert!(block.contains("What xNAUT remembers"), "{block}");
        assert!(block.contains("a.rs the lock"), "the file's learning: {block}");
        assert!(block.contains("XNAUT-11") && block.contains("Closed by: 661c4d7"), "the ticket's story with its fix: {block}");
        assert!(!block.contains("c.rs is unrelated"), "nothing about other files: {block}");
        assert_eq!(recall_block(&all, "XNAUT-99", &["src/z.rs".into()], 5), "");
        std::fs::remove_dir_all(root).unwrap();
    }
}
