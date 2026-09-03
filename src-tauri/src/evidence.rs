// The execution record: what an agent did, in bytes that hash the same
// everywhere (XNAUT-213, phase 1).
//
// Before this, xNAUT had every capture point and connected none of them. The
// claim "every action lands in a tamper-evident record" was not weakly true, it
// was false: `audit.jsonl` had two entries from 9 August, `agent-ledger.jsonl`
// stopped on 17 August, and its three surviving writers wrote kinds that appear
// nowhere in the file. Zero chained records had ever been written.
//
// Deliberate asymmetry with `veto.rs`, and it is the interesting part of this
// module. Veto fails OPEN everywhere: a broken guard must not stop work. This
// fails CLOSED: a broken recorder that lets work proceed unattested is exactly
// the failure the product exists to prevent. Two modules, opposite postures,
// each correct for its job.
//
// Phase 1 is the spine only. No signing, no HSM, no batching, no outbox. A
// record is canonical, domain-separated, chained and appended; sealing is
// phase 3 and it hangs off `record_hash` without changing anything here.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const SCHEMA: &str = "xnaut.execution-record/v1";

/// Domain separation, byte-prefixed before hashing, never omitted.
///
/// Without it a signature over a record can be replayed as a signature over a
/// checkpoint, or over any other document that happens to canonicalize to the
/// same bytes. `securosys-attest.py::link_hash` has no prefix at all; that is a
/// known defect and it is fixed when the signer moves onto this module.
///
/// The SCREAMING-KEBAB shape is NautGate's, from `core/app/audit_evidence.py`
/// (`NAUTGATE-DECISION-RECEIPT-V1\0`, same project, 48Nauts). Its trail shipped
/// first and the two are meant to verify side by side in one exported bundle,
/// so one convention wins and it is not this one's. Free to change today
/// because nothing is sealed yet; a format break the moment a checkpoint is.
const RECORD_DOMAIN: &[u8] = b"XNAUT-EXECUTION-RECORD-V1\x00";
const REDACTED_DOMAIN: &[u8] = b"XNAUT-REDACTED-VALUE-V1\x00";
const PATH_DOMAIN: &[u8] = b"XNAUT-PATH-V1\x00";
const ARGS_DOMAIN: &[u8] = b"XNAUT-TOOL-ARGUMENTS-V1\x00";

// ---- where it lives ---------------------------------------------------------

pub fn dir() -> PathBuf {
    // Tests must not append to the owner's real chain, same reasoning as
    // XNAUT_KEYCHAIN_SERVICE in secrets.rs.
    if let Some(over) = std::env::var("XNAUT_EVIDENCE_DIR").ok().filter(|v| !v.trim().is_empty()) {
        return PathBuf::from(over);
    }
    dirs::config_dir().unwrap_or_default().join("xnaut").join("evidence")
}

pub fn log_path() -> PathBuf {
    dir().join("execution.jsonl")
}

/// Does a failed record stop the work it was recording?
///
/// Defaults to yes, per the plan's failure table: silent unattested work is the
/// one outcome worse than a stopped agent. The escape hatch exists because a
/// full disk should be recoverable without rebuilding the app.
pub fn blocks() -> bool {
    !matches!(std::env::var("XNAUT_EVIDENCE_OPTIONAL").as_deref(), Ok("1") | Ok("true"))
}

// ---- RFC 8785 JCS -----------------------------------------------------------

/// Canonical JSON per RFC 8785, restricted to integers.
///
/// ponytail: floats are REFUSED rather than serialised. JCS number output is
/// ECMAScript `Number::toString`, which is shortest-round-trip Grisu with its
/// own exponent rules, and getting it subtly wrong is worse than not having it:
/// the chain still verifies on this machine and fails on the auditor's. No v1
/// record kind carries a float. If one ever does, pull in `ryu` and port
/// section 3.2.2.3 properly, with the Appendix B table as the test.
pub fn jcs(value: &Value) -> Result<String, String> {
    let mut out = String::new();
    write_jcs(value, &mut out)?;
    Ok(out)
}

fn write_jcs(value: &Value, out: &mut String) -> Result<(), String> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => match (n.as_u64(), n.as_i64()) {
            (Some(u), _) => out.push_str(&u.to_string()),
            (_, Some(i)) => out.push_str(&i.to_string()),
            _ => return Err(format!("evidence records carry no floats, got {n}")),
        },
        // serde_json's string escaping is already ECMAScript's: the \b \t \n \f
        // \r \" \\ shortcuts, \u00xx lowercase for the rest of C0, and no escape
        // for / or for anything non-ASCII. That is exactly JCS 3.2.2.2.
        Value::String(s) => out.push_str(&serde_json::to_string(s).map_err(|e| e.to_string())?),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_jcs(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            // Sorted by UTF-16 code unit, not by code point. The two orders
            // differ for astral-plane keys: an emoji sorts BEFORE U+FB33 by code
            // point and AFTER it by UTF-16, because its lead surrogate is
            // U+D83D. serde_json::Map's own iteration order is insertion or
            // BTree depending on features, so never rely on it here.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).map_err(|e| e.to_string())?);
                out.push(':');
                write_jcs(&map[*key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

// ---- hashing ----------------------------------------------------------------

/// `sha256:<lowercase hex>` over `domain || bytes`. Never bare hex: a hash
/// whose algorithm is implied cannot be migrated.
pub fn digest(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    let mut out = String::from("sha256:");
    for byte in hasher.finalize() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The hash of a record as a verifier recomputes it: canonical bytes of every
/// field except `hash` itself, domain-prefixed.
pub fn record_hash(record: &Map<String, Value>) -> Result<String, String> {
    let mut body = record.clone();
    body.remove("hash");
    Ok(digest(RECORD_DOMAIN, jcs(&Value::Object(body))?.as_bytes()))
}

/// A path as its hash, so an exported bundle can drop the plaintext beside it
/// and still verify. Directory layouts are customer information too.
pub fn path_hash(path: &str) -> String {
    digest(PATH_DOMAIN, path.as_bytes())
}

// ---- redaction --------------------------------------------------------------

/// Replace credential-shaped values with their hash, in place. True if anything
/// was replaced, which the caller records as `redacted` INSIDE the signed bytes
/// so the redaction is itself attested and cannot be claimed afterwards.
fn redact(value: &mut Value) -> bool {
    match value {
        Value::Object(map) => {
            let mut hit = false;
            for (key, child) in map.iter_mut() {
                match child.as_str() {
                    Some(text) if crate::secrets::is_secret_key(key) && !text.is_empty() => {
                        *child = Value::String(digest(REDACTED_DOMAIN, text.as_bytes()));
                        hit = true;
                    }
                    _ => hit |= redact(child),
                }
            }
            hit
        }
        Value::Array(items) => items.iter_mut().fold(false, |acc, item| acc | redact(item)),
        _ => false,
    }
}

/// Describe a tool's arguments for the record: hash, size, and the body written
/// to a content-addressed blob beside the log.
///
/// The body never goes inside the record. That is what makes an exported bundle
/// redactable: drop the blobs and every hash in the chain still resolves, so a
/// customer can prove what happened without handing over their source.
///
/// No plaintext goes inside the record, not even a preview. A 200-character
/// preview shipped until phase 4 and it quietly defeated the whole point of
/// `evidence_shred`: for a shell command 200 characters is usually the entire
/// command, so a shredded session still published its arguments in the log.
/// It also made a fully redacted bundle impossible, since removing a field the
/// record hashes over breaks the record. Deleting it fixed both. The UI reads
/// arguments through `evidence_arguments`, which works exactly while the
/// session's key exists, which is the correct behaviour.
pub fn arguments(session: &str, input: &Value) -> Map<String, Value> {
    let mut copy = input.clone();
    let redacted = redact(&mut copy);
    let canon = jcs(&copy).unwrap_or_else(|_| copy.to_string());
    let hash = digest(ARGS_DOMAIN, canon.as_bytes());
    let blob = blob_path(session, &hash);
    if !blob.exists() {
        let _ = crate::seal::write_blob(session, &blob, canon.as_bytes());
    }
    let mut out = Map::new();
    out.insert("args_hash".into(), Value::String(hash));
    out.insert("args_size".into(), Value::from(canon.len() as u64));
    if redacted {
        out.insert("redacted".into(), Value::Bool(true));
    }
    out
}

/// One session, as the Evidence panel lists it.
#[derive(serde::Serialize)]
pub struct SessionRow {
    pub session_id: String,
    pub records: usize,
    pub first_at: String,
    pub last_at: String,
    /// The KEK the session's key is wrapped under; empty when never sealed.
    pub kek: String,
    pub sealed: bool,
    pub shredded: bool,
    /// Who did the work, distinct and sorted. Empty when no record named an
    /// actor, which is itself worth showing rather than papering over.
    pub agents: Vec<String>,
    /// How many of the records are refusals. The most interesting row in a
    /// session is usually the one where something was stopped.
    pub refused: usize,
}

/// Running totals for one session while the log is being walked.
#[derive(Default)]
struct Tally {
    records: usize,
    first_at: String,
    last_at: String,
    agents: std::collections::BTreeSet<String>,
    refused: usize,
}

fn top(row: &Map<String, Value>, key: &str) -> String {
    row.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn nested(row: &Map<String, Value>, parent: &str, child: &str) -> String {
    row.get(parent)
        .and_then(Value::as_object)
        .and_then(|m| m.get(child))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Every session in the log, newest activity first.
///
/// Reads the file rather than the in-memory chain: the chain only knows the
/// sessions this process has appended to, and the point of the panel is to
/// shred a session recorded weeks ago. A corrupt line is skipped rather than
/// fatal, because refusing to list anything would take away the one control
/// that makes a bad record disposable.
#[tauri::command]
pub fn evidence_sessions() -> Result<Vec<SessionRow>, String> {
    let body = match std::fs::read_to_string(log_path()) {
        Ok(body) => body,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut seen: HashMap<String, Tally> = HashMap::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(row) = serde_json::from_str::<Map<String, Value>>(line) else { continue };
        let session = top(&row, "session_id");
        if session.is_empty() {
            continue;
        }
        let at = top(&row, "recorded_at");
        let entry = seen
            .entry(session)
            .or_insert_with(|| Tally { first_at: at.clone(), last_at: at.clone(), ..Default::default() });
        entry.records += 1;
        if !at.is_empty() && (entry.first_at.is_empty() || at < entry.first_at) {
            entry.first_at = at.clone();
        }
        if at > entry.last_at {
            entry.last_at = at;
        }
        let agent = nested(&row, "actor", "agent");
        if !agent.is_empty() {
            entry.agents.insert(agent);
        }
        if top(&row, "kind") == "tool_refused" {
            entry.refused += 1;
        }
    }
    let mut rows: Vec<SessionRow> = seen
        .into_iter()
        .map(|(session_id, tally)| {
            let (kek, shredded) = crate::seal::state(&session_id);
            SessionRow {
                session_id,
                records: tally.records,
                first_at: tally.first_at,
                last_at: tally.last_at,
                sealed: kek.is_some(),
                kek: kek.unwrap_or_default(),
                shredded,
                agents: tally.agents.into_iter().collect(),
                refused: tally.refused,
            }
        })
        .collect();
    // Newest first, then by id: two sessions recorded in the same millisecond
    // are a tie, and a list that reshuffles on every refresh is a list nobody
    // can click a destructive button in.
    rows.sort_by(|a, b| b.last_at.cmp(&a.last_at).then(a.session_id.cmp(&b.session_id)));
    Ok(rows)
}

/// One record, flattened into the fields a person actually reads.
///
/// The arguments come back as TEXT, not as a hash. `args_hash:
/// sha256:7578f1f2…` answers nothing about what happened, and the blob beside
/// the log has held the full command since phase 4 with nothing reading it:
/// `evidence_arguments` shipped, was permitted, and had zero callers.
#[derive(serde::Serialize)]
pub struct RecordRow {
    pub seq: u64,
    pub at: String,
    pub kind: String,
    /// `actor.agent`, empty when the record named none.
    pub agent: String,
    pub tool: String,
    pub model: String,
    /// `outcome.decision`: allow, deny.
    pub decision: String,
    /// The rule that refused it, when one did. This is the sentence a person
    /// came to the panel to find.
    pub rule: String,
    pub cwd: String,
    pub args_hash: String,
    pub args_size: u64,
    /// The full arguments as recorded. Already redacted at write time, so
    /// showing them cannot leak a credential the log did not already hold.
    pub args: String,
    /// One line for the row: the command itself, never a digest of it.
    pub summary: String,
    /// Why `args` is empty, when it is. A blank with no reason beside it is
    /// exactly the failure this panel exists to end.
    pub args_error: String,
}

/// The one line that stands in for a hash in a list.
///
/// ponytail: a fixed field order rather than a per-tool table. A tool nobody
/// has taught it still shows its real arguments, just as raw JSON instead of
/// a sentence. Add a key here rather than a match arm.
fn headline(args: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(args) else {
        return args.trim().to_string();
    };
    for key in ["command", "file_path", "path", "query", "pattern", "url", "prompt", "description"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return text.trim().to_string();
            }
        }
    }
    args.trim().to_string()
}

fn record_row(session: &str, row: &Map<String, Value>) -> RecordRow {
    let args_hash = nested(row, "tool", "args_hash");
    let (args, args_error) = if args_hash.is_empty() {
        (String::new(), String::new())
    } else {
        match crate::seal::read_blob(session, &blob_path(session, &args_hash)) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => (text, String::new()),
                Err(_) => (String::new(), "the blob is sealed and its key is gone".to_string()),
            },
            // A shredded session, or a bundle whose blobs were dropped before
            // export. Both are correct states, and both must say so in words
            // rather than render as an empty row.
            Err(why) => (String::new(), why),
        }
    };
    let model = top(row, "model");
    let summary = if !args.is_empty() {
        headline(&args)
    } else if !model.is_empty() {
        model.clone()
    } else {
        String::new()
    };
    RecordRow {
        seq: row.get("seq").and_then(Value::as_u64).unwrap_or(0),
        at: top(row, "recorded_at"),
        kind: top(row, "kind"),
        agent: nested(row, "actor", "agent"),
        tool: nested(row, "tool", "name"),
        model,
        decision: nested(row, "outcome", "decision"),
        rule: nested(row, "outcome", "rule"),
        cwd: nested(row, "context", "cwd"),
        args_size: row
            .get("tool")
            .and_then(Value::as_object)
            .and_then(|m| m.get("args_size"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        args_hash,
        args,
        summary,
        args_error,
    }
}

/// Every record in one session, oldest first, with its arguments resolved.
///
/// Oldest first on purpose: sessions are read as a story of what was done, and
/// a story told backwards is not one. The session LIST is newest first, which
/// is the opposite question ("which run") and takes the opposite order.
///
/// ponytail: re-reads the whole log per session, like `evidence_sessions`
/// does. 648 records is under a millisecond; index it when a scan stops being
/// free, and note that the blob reads, not the scan, are the cost here.
#[tauri::command]
pub fn evidence_records(session: String) -> Result<Vec<RecordRow>, String> {
    let body = match std::fs::read_to_string(log_path()) {
        Ok(body) => body,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut rows = Vec::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(row) = serde_json::from_str::<Map<String, Value>>(line) else { continue };
        if top(&row, "session_id") != session {
            continue;
        }
        rows.push(record_row(&session, &row));
    }
    rows.sort_by_key(|r| r.seq);
    Ok(rows)
}

// ---- the join: a ticket, and the commands run for it ------------------------

/// One command an agent ran, as the answer to "what did it actually do".
#[derive(serde::Serialize, Debug)]
pub struct TicketCommand {
    pub session: String,
    pub seq: u64,
    pub at: String,
    pub agent: String,
    pub tool: String,
    /// allow or deny. A refusal is usually the most interesting line here.
    pub decision: String,
    /// The command itself, resolved from the content-addressed blob. Empty
    /// only when `error` says why.
    pub command: String,
    /// Why `command` is empty, when it is. A shredded session and a session
    /// that ran nothing are opposite facts and must not render alike.
    pub error: String,
}

/// What the record can and cannot say about one ticket.
///
/// Every field that could be empty is paired with something that explains the
/// emptiness. This project's recurring bug is a confident empty result, so a
/// caller here is given no way to render one by accident.
#[derive(serde::Serialize, Debug)]
pub struct TicketEvidence {
    pub ticket: String,
    /// How many ledger rows the ticket has at all. Zero means the ledger has
    /// never heard of it, which is a different answer from "it ran nothing".
    pub ledger_rows: usize,
    /// The sessions its rows named, distinct and sorted.
    pub sessions: Vec<String>,
    /// Ledger rows that carried no session id. These are the work this ticket
    /// did that cannot be traced to commands, counted rather than dropped.
    pub unattributed_rows: usize,
    pub commands: Vec<TicketCommand>,
    /// Populated whenever `commands` is empty, and empty whenever it is not.
    /// The sentence a person needs in order to trust the blank.
    pub unjoinable: String,
}

/// From a ticket id to the commands an agent ran for it.
///
/// Follows ONLY session ids a ledger row explicitly recorded next to this
/// ticket. Nothing here matches on time, on the agent handle or on the working
/// directory; see the note at the top of `ledger.rs` for the two same-handle
/// same-directory runs on this machine that such a match would get wrong.
///
/// ponytail: re-reads the ledger and the whole evidence log per call, like
/// `evidence_records` does. 648 records and 60 ledger lines is under a
/// millisecond; index both when a scan stops being free.
#[tauri::command]
pub fn ticket_evidence(ticket: String) -> Result<TicketEvidence, String> {
    let wanted = ticket.trim().to_uppercase();
    let rows = crate::ledger::rows_for_ticket(&wanted);
    let sessions: Vec<String> = rows
        .iter()
        .filter(|row| !row.session.is_empty())
        .map(|row| row.session.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let unattributed_rows = rows.iter().filter(|row| row.session.is_empty()).count();

    let mut out = TicketEvidence {
        ticket: wanted.clone(),
        ledger_rows: rows.len(),
        sessions: sessions.clone(),
        unattributed_rows,
        commands: Vec::new(),
        unjoinable: String::new(),
    };
    if wanted.is_empty() {
        out.unjoinable = "no ticket id was given".to_string();
        return Ok(out);
    }

    for session in &sessions {
        for row in evidence_records(session.clone())? {
            // A model call is not something the agent DID to the machine, and
            // the question being asked is what it did.
            if row.kind == "model_call" {
                continue;
            }
            out.commands.push(TicketCommand {
                session: session.clone(),
                seq: row.seq,
                at: row.at,
                agent: row.agent,
                tool: row.tool,
                decision: row.decision,
                command: row.summary,
                error: row.args_error,
            });
        }
    }
    out.commands.sort_by(|a, b| a.at.cmp(&b.at).then(a.seq.cmp(&b.seq)));

    // Every way this can come back empty, said in words. The order matters:
    // each case is a different thing for the reader to go and do.
    if out.commands.is_empty() {
        out.unjoinable = if out.ledger_rows == 0 {
            format!("the ledger has no row for {wanted}, so nothing was ever dispatched under that id")
        } else if sessions.is_empty() {
            format!(
                "{} ledger rows for {wanted}, none of which recorded an evidence session, so its commands cannot be reached from here",
                out.ledger_rows
            )
        } else {
            format!(
                "{} attributed to {wanted}, and the evidence log holds no tool call for them",
                if sessions.len() == 1 {
                    format!("session {} is", sessions[0])
                } else {
                    format!("{} sessions are", sessions.len())
                }
            )
        };
    }
    Ok(out)
}

/// One evidence session no ticket has claimed.
#[derive(serde::Serialize)]
pub struct UnattributedSession {
    pub session_id: String,
    pub records: usize,
    pub first_at: String,
    pub last_at: String,
    pub agents: Vec<String>,
    /// Always populated. The other half of the honest-blank rule: work that
    /// happened with no ticket behind it is a finding, not an absence.
    pub why: String,
}

/// Evidence sessions that no ticket points at.
///
/// The mirror of `ticket_evidence`, and the reason the pair is worth having:
/// a fleet run's `unverified_close` is answered by the first, and work nobody
/// can attribute to any ticket is answered by this one. Neither may be
/// reported as an empty screen.
#[tauri::command]
pub fn unattributed_sessions() -> Result<Vec<UnattributedSession>, String> {
    let claimed = crate::ledger::claimed_sessions();
    Ok(evidence_sessions()?
        .into_iter()
        .filter(|row| !claimed.contains(&row.session_id))
        .map(|row| UnattributedSession {
            why: format!(
                "{} records under {}, and no ledger row names it, so the ticket this work belongs to is unknown",
                row.records, row.session_id
            ),
            session_id: row.session_id,
            records: row.records,
            first_at: row.first_at,
            last_at: row.last_at,
            agents: row.agents,
        })
        .collect())
}

/// Move every session key onto a new KEK, and say how many moved.
///
/// The new key has to exist in the HSM first; this app deliberately cannot
/// create one.
#[tauri::command]
pub fn evidence_rotate_kek(new_label: String) -> Result<usize, String> {
    crate::seal::rotate_kek(&new_label)
}

/// The KEK new sessions are sealed under today.
#[tauri::command]
pub fn evidence_kek_label() -> String {
    crate::seal::kek_label().to_string()
}

/// The arguments behind a record. Sealed blobs come back only while the
/// session's key exists; after a shred this is the error, which is the point.
#[tauri::command]
pub fn evidence_arguments(session: String, args_hash: String) -> Result<String, String> {
    let bytes = crate::seal::read_blob(&session, &blob_path(&session, &args_hash))?;
    String::from_utf8(bytes).map_err(|_| "the blob is sealed and its key is gone".to_string())
}

/// Destroy one session's sealing key. The records stay where they are and still
/// verify; their arguments become unreadable by everyone, us included.
#[tauri::command]
pub fn evidence_shred(session: String) -> Result<(), String> {
    crate::seal::shred(&session)
}

/// Where one session's blob for `hash` lives.
///
/// Per session, not one flat store, because the sealing key is per session:
/// a blob shared between two sessions could not be shredded with either of
/// them without breaking the other. Blobs are small; correctness is not.
pub fn blob_path(session: &str, hash: &str) -> PathBuf {
    dir().join("blobs").join(session).join(hash.replace("sha256:", ""))
}

// ---- the chain --------------------------------------------------------------

/// Next seq and current head, per session. The mutex also serialises the append
/// itself: two agents recording at the same instant must not compute the same
/// `prev_hash`, or one of them writes a link nobody can reproduce.
fn chain() -> &'static Mutex<HashMap<String, (u64, String)>> {
    static CHAIN: OnceLock<Mutex<HashMap<String, (u64, String)>>> = OnceLock::new();
    CHAIN.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Where a session's chain stands on disk. Scanned once per session per process.
///
/// ponytail: a full file read on first touch of a session. At a few thousand
/// records that is milliseconds and it happens once; index it in `evidence.db`
/// when the log outgrows a scan.
fn head_on_disk(path: &Path, session: &str) -> (u64, Value) {
    let Ok(body) = std::fs::read_to_string(path) else {
        return (0, Value::Null);
    };
    let mut found: Option<(u64, Value)> = None;
    for line in body.lines() {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue; // A corrupt line costs one line. The verifier reports it.
        };
        if row.get("session_id").and_then(Value::as_str) != Some(session) {
            continue;
        }
        let seq = row.get("seq").and_then(Value::as_u64).unwrap_or(0);
        if let Some(hash) = row.get("hash").cloned() {
            found = Some((seq + 1, hash));
        }
    }
    found.unwrap_or((0, Value::Null))
}

fn executor_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let host = std::process::Command::new("hostname")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into());
        format!("xnaut:{host}")
    })
}

/// Append one record and return its hash.
///
/// `body` is the kind's own fields; the envelope is added here so no call site
/// can forget the version, the chain or the clock.
pub fn record(kind: &str, session: &str, body: Map<String, Value>) -> Result<String, String> {
    let session = if session.trim().is_empty() { "unattributed" } else { session.trim() };
    let path = log_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("evidence dir: {e}"))?;
    }

    let mut state = chain().lock().map_err(|_| "evidence chain poisoned".to_string())?;
    let (seq, prev) = match state.get(session) {
        Some((seq, head)) => (*seq, Value::String(head.clone())),
        None => head_on_disk(&path, session),
    };

    let mut rec = Map::new();
    rec.insert("schema_version".into(), Value::String(SCHEMA.into()));
    rec.insert("record_id".into(), Value::String(uuid::Uuid::new_v4().to_string()));
    rec.insert("session_id".into(), Value::String(session.into()));
    rec.insert("seq".into(), Value::from(seq));
    rec.insert("prev_hash".into(), prev);
    rec.insert(
        "recorded_at".into(),
        Value::String(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
    );
    rec.insert("executor_id".into(), Value::String(executor_id().into()));
    rec.insert("executor_version".into(), Value::String(env!("CARGO_PKG_VERSION").into()));
    rec.insert("kind".into(), Value::String(kind.into()));
    for (key, value) in body {
        rec.insert(key, value);
    }

    let hash = record_hash(&rec)?;
    rec.insert("hash".into(), Value::String(hash.clone()));
    let line = serde_json::to_string(&Value::Object(rec)).map_err(|e| e.to_string())? + "\n";

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("evidence log: {e}"))?;
    file.write_all(line.as_bytes()).map_err(|e| format!("evidence write: {e}"))?;
    // Only now is the record real. Advancing the head before the write lands
    // would chain the next record onto something no reader can find.
    state.insert(session.to_string(), (seq + 1, hash.clone()));
    Ok(hash)
}

/// Record, or tell the caller to stop.
///
/// Ok(()) means either it was recorded or the operator opted out of blocking.
/// Err means the work must not proceed, and the string says why in words a
/// person can act on.
pub fn gate(kind: &str, session: &str, body: Map<String, Value>) -> Result<(), String> {
    match record(kind, session, body) {
        Ok(_) => Ok(()),
        Err(why) if blocks() => Err(format!("xNAUT could not record this action, so it was not run: {why}")),
        Err(why) => {
            eprintln!("evidence: {why} (XNAUT_EVIDENCE_OPTIONAL is set, continuing unattested)");
            Ok(())
        }
    }
}

/// Convenience for the common shape: a handful of string fields.
pub fn fields(pairs: &[(&str, &str)]) -> Map<String, Value> {
    pairs
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| ((*k).to_string(), Value::String((*v).to_string())))
        .collect()
}

// ---- verification -----------------------------------------------------------

/// What a verification actually established.
///
/// A struct rather than a bool, because the panel must not be the thing that
/// decides what "verified" means. Everything it needs to render an honest
/// verdict, including the case where there is nothing to verify, is decided
/// here and carried across.
#[derive(serde::Serialize)]
pub struct VerifyReport {
    /// The file that was read, spelled out. An empty panel that does not name
    /// the file it looked at is indistinguishable from a broken one, so this
    /// is populated even when nothing was found.
    pub path: String,
    pub exists: bool,
    pub records: usize,
    pub sessions: usize,
    /// True ONLY when at least one record was checked and every check passed.
    ///
    /// An empty chain is not verified. There is nothing there to verify, and a
    /// green tick over zero records is the precise lie this field exists to
    /// refuse: it is indistinguishable, to the reader, from a chain of a
    /// thousand records that all held.
    pub ok: bool,
    /// Empty while ok. Otherwise the first failure, naming its line.
    pub broken: String,
    /// What was checked, in words, so the verdict is never a bare tick. Only
    /// populated when something actually was checked.
    pub checked: Vec<String>,
}

/// Walk the log, recompute every hash and follow every link, and report.
///
/// Stops at the first break: past a broken link nothing downstream means
/// anything, and a count that kept going would read like partial success.
pub fn verify_report(path: &Path) -> VerifyReport {
    let mut report = VerifyReport {
        path: path.display().to_string(),
        exists: path.exists(),
        records: 0,
        sessions: 0,
        ok: false,
        broken: String::new(),
        checked: Vec::new(),
    };
    let body = match std::fs::read_to_string(path) {
        Ok(body) => body,
        Err(e) => {
            report.broken = format!("cannot read {}: {e}", path.display());
            return report;
        }
    };
    let mut expected: HashMap<String, (u64, Value)> = HashMap::new();
    for (index, line) in body.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let row: Map<String, Value> = match serde_json::from_str(line) {
            Ok(row) => row,
            Err(e) => {
                report.broken = format!("line {}: corrupt, gap here: {e}", index + 1);
                break;
            }
        };
        let claimed = row.get("hash").and_then(Value::as_str).unwrap_or("").to_string();
        match record_hash(&row) {
            Ok(computed) if computed == claimed => {}
            Ok(_) => {
                report.broken = format!("line {}: record does not hash to its own hash", index + 1);
                break;
            }
            Err(why) => {
                report.broken = format!("line {}: {why}", index + 1);
                break;
            }
        }
        let session = row.get("session_id").and_then(Value::as_str).unwrap_or("").to_string();
        let (seq, prev) = expected.get(&session).cloned().unwrap_or((0, Value::Null));
        if row.get("seq").and_then(Value::as_u64) != Some(seq) {
            report.broken = format!("line {}: session {session} expected seq {seq}", index + 1);
            break;
        }
        if row.get("prev_hash") != Some(&prev) {
            report.broken =
                format!("line {}: session {session} does not link to the record before it", index + 1);
            break;
        }
        expected.insert(session, (seq + 1, Value::String(claimed)));
        report.records += 1;
    }
    report.sessions = expected.len();
    report.ok = report.broken.is_empty() && report.records > 0;
    if report.ok {
        report.checked = vec![
            format!(
                "{} records re-hashed from their own canonical bytes; each matched the hash it carries",
                report.records
            ),
            format!(
                "{} prev_hash links followed back to the record before them, across {} sessions",
                report.records, report.sessions
            ),
            format!("seq runs contiguously from 0 in every one of the {} sessions", report.sessions),
        ];
    }
    report
}

/// How many records verified, or the first break.
///
/// The thin shape, kept because a caller that only wants a verdict should not
/// have to reason about a report.
pub fn verify(path: &Path) -> Result<usize, String> {
    let report = verify_report(path);
    if report.broken.is_empty() {
        Ok(report.records)
    } else {
        Err(report.broken)
    }
}

/// Verify the chain on this machine and say what was checked.
///
/// Never Err on a broken chain: a break is a RESULT, and one that arrives as
/// an error is one the panel renders in the same red box it uses for "the
/// backend is down". They are opposite facts and must not look alike.
#[tauri::command]
pub fn evidence_verify() -> VerifyReport {
    verify_report(&log_path())
}

/// XNAUT_EVIDENCE_DIR is process-global, so tests that set it take turns.
/// `seal` writes into the same tree and has to queue behind the same lock.
#[cfg(test)]
pub(crate) static DIR_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_panel_lists_a_session_it_can_shred() {
        let _guard = DIR_LOCK.lock().unwrap();
        let scratch = std::env::temp_dir().join(format!("xnaut-ev-{}", uuid::Uuid::new_v4()));
        std::env::set_var("XNAUT_EVIDENCE_DIR", &scratch);
        let old = format!("old-{}", uuid::Uuid::new_v4());
        let new = format!("new-{}", uuid::Uuid::new_v4());
        record("tool_call", &old, fields(&[("tool", "Read")])).unwrap();
        for tool in ["Read", "Edit"] {
            record("tool_call", &new, fields(&[("tool", tool)])).unwrap();
        }

        let rows = evidence_sessions().unwrap();
        // Newest first, or the session someone wants to shred is the one they
        // have to scroll for. Asserted as the property, not as an identity:
        // both sessions can land in the same millisecond, which used to make
        // this test fail about one run in three.
        assert!(rows.windows(2).all(|w| w[0].last_at >= w[1].last_at), "not sorted newest first");
        let newest = rows.iter().find(|r| r.session_id == new).unwrap();
        assert_eq!(newest.records, 2);
        assert!(newest.first_at <= newest.last_at);
        assert_eq!(rows.iter().find(|r| r.session_id == old).unwrap().records, 1);
        assert!(rows.iter().all(|r| !r.sealed && !r.shredded), "nothing was sealed here");
        assert!(rows.iter().all(|r| r.kek.is_empty()));

        // A corrupt line must not take the whole list down with it.
        let mut body = std::fs::read_to_string(log_path()).unwrap();
        body.push_str("{not json\n");
        std::fs::write(log_path(), body).unwrap();
        assert_eq!(evidence_sessions().unwrap().len(), 2);

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    // ---- the ticket-to-commands join ---------------------------------------

    /// A scratch evidence tree plus a scratch ledger, both redirected.
    ///
    /// Both are env-var globals, so the two locks are always taken in THIS
    /// order and nowhere else in the tree takes both; taking them in the other
    /// order somewhere would deadlock the suite.
    #[cfg(test)]
    fn scratch_join(
        name: &str,
    ) -> (std::sync::MutexGuard<'static, ()>, std::sync::MutexGuard<'static, ()>, PathBuf) {
        let dir_guard = DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let scratch = std::env::temp_dir().join(format!("xnaut-join-{}-{name}", uuid::Uuid::new_v4()));
        std::env::set_var("XNAUT_EVIDENCE_DIR", &scratch);
        let ledger_guard = crate::ledger::scratch(name);
        (dir_guard, ledger_guard, scratch)
    }

    /// Record one tool call the way `veto.rs` does, blob and all.
    fn tool_call(session: &str, agent: &str, command: &str) {
        let mut tool = arguments(session, &json!({ "command": command }));
        tool.insert("name".into(), Value::String("Bash".into()));
        let mut body = Map::new();
        body.insert("actor".into(), Value::Object(fields(&[("agent", agent)])));
        body.insert("tool".into(), Value::Object(tool));
        body.insert("outcome".into(), Value::Object(fields(&[("decision", "allow")])));
        record("tool_call", session, body).unwrap();
    }

    #[test]
    fn a_ticket_leads_to_the_commands_its_agent_ran() {
        // THE question the two logs could not answer together: start at a
        // ticket id, end at the actual shell commands the agent ran for it.
        //
        // Built fixture rather than the owner's live tree, so it runs on any
        // machine; the same walk over a copy of his real 648-record log is
        // `a_real_session_resolves_to_the_real_commands_it_ran` below.
        let (_dir, _ledger, scratch) = scratch_join("join");
        let session = uuid::Uuid::new_v4().to_string();
        crate::ledger::record_in_session(
            "dispatched",
            "claude",
            "XNAUT-58",
            "Start with XNAUT-58",
            &session,
        );
        tool_call(&session, "claude", "cargo test --bin xnaut");
        tool_call(&session, "claude", "git commit -m 'fix the thing'");
        // A second ticket, a second session, same agent: the join must not
        // bleed one into the other. This is the real 2026-08-31 shape.
        let other = uuid::Uuid::new_v4().to_string();
        crate::ledger::record_in_session("dispatched", "claude", "XNAUT-73", "and 73", &other);
        tool_call(&other, "claude", "cargo build --release");

        let found = ticket_evidence("XNAUT-58".into()).unwrap();
        assert_eq!(found.sessions, vec![session.clone()]);
        let ran: Vec<&str> = found.commands.iter().map(|c| c.command.as_str()).collect();
        assert_eq!(ran, vec!["cargo test --bin xnaut", "git commit -m 'fix the thing'"]);
        assert!(found.unjoinable.is_empty(), "a joined ticket explains nothing: {found:?}",);
        assert!(
            found.commands.iter().all(|c| c.agent == "claude" && c.tool == "Bash"),
            "the actor and the tool travel with the command"
        );
        // Lower case in, same answer out: the ledger stores ids upper-cased.
        assert_eq!(ticket_evidence("xnaut-58".into()).unwrap().commands.len(), 2);
        // And the other ticket kept its own single command.
        let sibling = ticket_evidence("XNAUT-73".into()).unwrap();
        assert_eq!(
            sibling.commands.iter().map(|c| c.command.as_str()).collect::<Vec<_>>(),
            vec!["cargo build --release"],
            "one agent, two tickets, two sessions: they must not merge"
        );

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_ticket_nobody_attributed_says_so_rather_than_coming_back_empty() {
        // The recurring bug this project pays for is a confident empty result.
        // A ticket the ledger knows, with rows that never named a session, is
        // NOT "this agent ran nothing"; it is "this cannot be reached from
        // here", and the two must not render alike.
        let (_dir, _ledger, scratch) = scratch_join("unjoinable");
        crate::ledger::record("dispatched", "claude", "XNAUT-99", "no session in hand");
        crate::ledger::record("nudged", "claude", "XNAUT-99", "again");

        let found = ticket_evidence("XNAUT-99".into()).unwrap();
        assert!(found.commands.is_empty());
        assert_eq!(found.ledger_rows, 2, "the rows are real and are counted");
        assert_eq!(found.unattributed_rows, 2);
        assert!(
            found.unjoinable.contains("none of which recorded an evidence session"),
            "the blank has to explain itself: {:?}",
            found.unjoinable
        );

        // A ticket the ledger has never heard of is a DIFFERENT sentence.
        let never = ticket_evidence("XNAUT-4242".into()).unwrap();
        assert_eq!(never.ledger_rows, 0);
        assert!(
            never.unjoinable.contains("no row for XNAUT-4242"),
            "unknown and unattributed are different facts: {:?}",
            never.unjoinable
        );
        assert_ne!(
            never.unjoinable, found.unjoinable,
            "two different blanks must not read the same"
        );

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn evidence_no_ticket_claims_is_listed_not_hidden() {
        // The other direction of the same rule. Work that ran with no ticket
        // behind it is a finding, and a fleet report that quietly omits it is
        // the "it looks like it worked" failure wearing a different hat.
        let (_dir, _ledger, scratch) = scratch_join("orphan-evidence");
        let claimed = uuid::Uuid::new_v4().to_string();
        let orphan = uuid::Uuid::new_v4().to_string();
        crate::ledger::record_in_session("dispatched", "claude", "XNAUT-1", "go", &claimed);
        tool_call(&claimed, "claude", "ls");
        tool_call(&orphan, "claude", "rm -rf /tmp/whatever");

        let loose = unattributed_sessions().unwrap();
        let ids: Vec<&str> = loose.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, vec![orphan.as_str()], "only the unclaimed session is loose: {ids:?}");
        assert!(
            loose[0].why.contains("no ledger row names it"),
            "it has to say why it is here: {:?}",
            loose[0].why
        );
        assert_eq!(loose[0].records, 1);

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_shredded_command_says_it_is_gone_instead_of_reading_as_silence() {
        // A ticket whose evidence was shredded must not look like a ticket
        // that ran nothing. The command is unreadable BY DESIGN, and the row
        // has to carry that sentence or the shred becomes indistinguishable
        // from an empty run.
        let (_dir, _ledger, scratch) = scratch_join("shredded");
        let session = uuid::Uuid::new_v4().to_string();
        crate::ledger::record_in_session("dispatched", "claude", "XNAUT-7", "go", &session);
        tool_call(&session, "claude", "cargo test");
        // Drop the blob the way an exported bundle does.
        let _ = std::fs::remove_dir_all(dir().join("blobs").join(&session));

        let found = ticket_evidence("XNAUT-7".into()).unwrap();
        assert_eq!(found.commands.len(), 1, "the RECORD survives; only its argument body is gone");
        assert!(found.commands[0].command.is_empty());
        assert!(
            !found.commands[0].error.is_empty(),
            "an unreadable command must name its reason, not render blank"
        );

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A record written before the join existed, byte for byte off André's
    /// machine (session 9487b4a3, the XNAUT-58 run of 2026-08-31 11:18).
    ///
    /// The join deliberately added NO field to the execution record, because
    /// this side is hash-linked and the ledger is not. This is the tripwire
    /// for that promise: if anything ever changes what a record hashes over,
    /// this line stops verifying and all 648 of his stop with it.
    const REAL_RECORD_2026_08_31: &str = r#"{"actor":{"agent":"claude"},"context":{"cwd":"/Users/cand0rian/Library/Application Support/xnaut/agent-workspaces/claude","cwd_hash":"sha256:4006cd4e74d5e35d4d09598cfc703f4453537f524099ebcd4212589f4d481fe3"},"executor_id":"xnaut:Cand0riacStudio.candoo","executor_version":"1.25.2","hash":"sha256:3949ec97fbbce1edaba09c6cc9e384ce0be37f7c0f3cce7c9eff4c8f4464ed8e","kind":"tool_call","outcome":{"decision":"allow"},"prev_hash":null,"record_id":"a5539fe8-3a6d-4982-ad83-473d186d6fbc","recorded_at":"2026-08-31T11:18:27.472Z","schema_version":"xnaut.execution-record/v1","seq":0,"session_id":"9487b4a3-b360-47c5-ab28-2943fa27d8e0","tool":{"args_hash":"sha256:a709894f37af7d08a2c22e1f5c33386dcd029b6e9c2251c7568170717635ad52","args_size":159,"name":"Bash"}}"#;

    /// The same walk, over the owner's REAL chain rather than a fixture.
    ///
    /// Copied into scratch first, never read in place: pointing the global
    /// XNAUT_EVIDENCE_DIR at his live tree would leave one panicking test able
    /// to append to a 648-record chain that is the whole point of the feature.
    ///
    /// Skips, loudly, on a machine without that log — it is his data, not the
    /// repo's. The hermetic proof of the same behaviour is
    /// `a_ticket_leads_to_the_commands_its_agent_ran`.
    #[test]
    fn a_real_session_resolves_to_the_real_commands_it_ran() {
        let live = dirs::config_dir().unwrap_or_default().join("xnaut").join("evidence");
        if !live.join("execution.jsonl").exists() {
            eprintln!("SKIPPED: no live evidence log at {}", live.display());
            return;
        }
        let (_dir, _ledger, scratch) = scratch_join("real");
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::copy(live.join("execution.jsonl"), scratch.join("execution.jsonl")).unwrap();
        let _ = std::process::Command::new("cp")
            .arg("-R")
            .arg(live.join("blobs"))
            .arg(scratch.join("blobs"))
            .status();

        // The real session that ran on 2026-08-31 at 11:18, five seconds after
        // the real `dispatched` row whose ticket field was left empty. The
        // attribution is the one thing that was missing; supply it and the
        // question answers itself.
        let session = "9487b4a3-b360-47c5-ab28-2943fa27d8e0";
        crate::ledger::record_in_session("dispatched", "claude", "XNAUT-58", "go", session);

        let found = ticket_evidence("XNAUT-58".into()).unwrap();
        assert!(
            found.commands.len() > 10,
            "the real session holds 34 records; got {}",
            found.commands.len()
        );
        assert!(found.unjoinable.is_empty());
        let readable: Vec<&TicketCommand> =
            found.commands.iter().filter(|c| !c.command.is_empty()).collect();
        assert!(
            !readable.is_empty(),
            "every command came back empty, so the blobs did not resolve: {:?}",
            &found.commands[..3.min(found.commands.len())]
        );
        eprintln!(
            "real data: XNAUT-58 -> {} records, first command: {:?}",
            found.commands.len(),
            readable[0].command
        );

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_record_written_before_the_join_still_verifies() {
        let row: Map<String, Value> = serde_json::from_str(REAL_RECORD_2026_08_31).unwrap();
        let claimed = row.get("hash").and_then(Value::as_str).unwrap();
        assert_eq!(
            record_hash(&row).unwrap(),
            claimed,
            "adding the join must not change what an execution record hashes over"
        );
        assert!(
            !row.contains_key("ticket"),
            "the link lives on the ledger row; putting it here would re-canonicalize every record"
        );
    }

    #[test]
    fn keys_sort_by_utf16_code_unit_not_code_point() {
        // RFC 8785 section 3.2.3, verbatim. The emoji is the whole point: by
        // code point it sorts before U+FB33, by UTF-16 it sorts after, because
        // its lead surrogate is U+D83D. Getting this wrong is invisible until
        // someone names a field with an emoji, and then only for them.
        let input = json!({
            "\u{20ac}": "Euro Sign",
            "\r": "Carriage Return",
            "\u{fb33}": "Hebrew Letter Dalet With Dagesh",
            "1": "One",
            "\u{1f600}": "Emoji: Grinning Face",
            "\u{80}": "Control",
            "\u{f6}": "Latin Small Letter O With Diaeresis",
        });
        let canon = jcs(&input).unwrap();
        let order: Vec<&str> = ["Carriage Return", "One", "Control",
            "Latin Small Letter O With Diaeresis", "Euro Sign",
            "Emoji: Grinning Face", "Hebrew Letter Dalet With Dagesh"].into();
        let mut at = 0;
        for value in order {
            let found = canon[at..].find(value).unwrap_or_else(|| panic!("{value} out of order in {canon}"));
            at += found;
        }
    }

    #[test]
    fn strings_escape_the_way_the_rfc_says() {
        // RFC 8785 section 3.2.2's string, and its canonical form byte for byte.
        let input = json!({ "string": "\u{20ac}$\u{0f}\nA'B\"\\\\\"/" });
        assert_eq!(jcs(&input).unwrap(), "{\"string\":\"€$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}");
    }

    #[test]
    fn a_float_is_refused_rather_than_guessed_at() {
        assert!(jcs(&json!({ "val": 3.5 })).is_err());
        assert_eq!(jcs(&json!({ "big": "055", "val": 12 })).unwrap(), "{\"big\":\"055\",\"val\":12}");
    }

    /// The one vector both implementations are pinned to.
    ///
    /// `mcp/xnaut_evidence.py` asserts the same record hashes to the same
    /// string in its `--selftest`. Two canonicalizers that agree today drift
    /// apart at the first edge case nobody tested; this is the tripwire, and
    /// it fires on either side changing alone.
    #[test]
    fn the_python_side_computes_the_same_hash_for_the_same_record() {
        let record: Map<String, Value> = serde_json::from_str(
            r#"{"schema_version":"xnaut.execution-record/v1","record_id":"a",
                "session_id":"s","seq":0,"prev_hash":null,
                "recorded_at":"2026-08-20T00:00:00.000Z","executor_id":"xnaut:test",
                "executor_version":"1.19.0","kind":"tool_call"}"#,
        )
        .unwrap();
        assert_eq!(
            record_hash(&record).unwrap(),
            "sha256:c6f9bdb5259ad1e23a9fa9a137cbefcad4e83a8cd849531686a3bf5491ac2b52"
        );
    }

    #[test]
    fn the_domain_prefix_separates_identical_bytes() {
        let body = b"{}";
        assert_ne!(digest(RECORD_DOMAIN, body), digest(b"XNAUT-AUDIT-CHECKPOINT-V1\x00", body));
        assert_ne!(digest(RECORD_DOMAIN, body), digest(b"", body));
    }

    #[test]
    fn a_secret_shaped_argument_is_hashed_before_it_reaches_the_blob() {
        let _guard = DIR_LOCK.lock().unwrap();
        let scratch = std::env::temp_dir().join(format!("xnaut-ev-{}", uuid::Uuid::new_v4()));
        std::env::set_var("XNAUT_EVIDENCE_DIR", &scratch);
        let described = arguments("sess", &json!({
            "command": "curl -H auth",
            "env": { "GITEA_ACCESS_TOKEN": "tok_abcdef", "TSB_URL": "https://tsb.example" },
        }));
        assert_eq!(described["redacted"], json!(true));
        let body = serde_json::to_string(&Value::Object(described.clone())).unwrap();
        assert!(!body.contains("tok_abcdef"), "the secret reached the record: {body}");
        assert!(!body.contains("https://tsb.example"), "no plaintext in a record at all: {body}");
        let blob = std::fs::read_to_string(
            blob_path("sess", described["args_hash"].as_str().unwrap())).unwrap();
        assert!(!blob.contains("tok_abcdef"));
        assert!(blob.contains("https://tsb.example"), "a non-secret must survive into the blob");
        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Registering a command in main.rs is HALF of adding one.
    ///
    /// main.rs's `acl_audit` checks that every permission BLOCK is granted. It
    /// cannot see a command that is in no block at all, and that is the gap
    /// this whole surface was built out of: `allow-evidence` was granted
    /// nowhere, so these were the only 5 blocked commands of 349 while
    /// delivery-panel.js rendered a tab calling 4 of them. Every call failed at
    /// the ACL, which is silent by design. A new command added here without a
    /// line in default.toml lands exactly as dead, with no compile error and no
    /// runtime log; this is the only thing that says so.
    #[test]
    fn every_command_here_is_both_registered_and_permitted() {
        let source = include_str!("evidence.rs");
        let toml = include_str!("../permissions/default.toml");
        let main = include_str!("main.rs");
        let lines: Vec<&str> = source.lines().collect();
        let mut names = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if line.trim() != "#[tauri::command]" {
                continue;
            }
            let sig = lines.get(i + 1).copied().unwrap_or("");
            if let Some(name) = sig.split("fn ").nth(1).and_then(|rest| rest.split('(').next()) {
                names.push(name.to_string());
            }
        }
        assert!(names.len() >= 7, "found only {names:?}; the scraper stopped matching");
        let unpermitted: Vec<&String> =
            names.iter().filter(|n| !toml.contains(&format!("\"{n}\""))).collect();
        assert!(
            unpermitted.is_empty(),
            "in no permissions/default.toml block, so the ACL refuses them at runtime \
             with nothing to see: {unpermitted:?}"
        );
        let unregistered: Vec<&String> =
            names.iter().filter(|n| !main.contains(&format!("evidence::{n},"))).collect();
        assert!(
            unregistered.is_empty(),
            "not in main.rs's invoke_handler, so the frontend cannot call them: {unregistered:?}"
        );
    }

    /// The point of the whole panel: a row reads as a command, not a digest.
    #[test]
    fn a_record_comes_back_as_its_command_text_not_its_hash() {
        let _guard = DIR_LOCK.lock().unwrap();
        let scratch = std::env::temp_dir().join(format!("xnaut-ev-{}", uuid::Uuid::new_v4()));
        std::env::set_var("XNAUT_EVIDENCE_DIR", &scratch);
        let session = format!("run-{}", uuid::Uuid::new_v4());

        let mut described = arguments(&session, &json!({
            "command": "cargo tauri build --release", "description": "Build it",
        }));
        described.insert("name".into(), Value::String("Bash".into()));
        let mut body = Map::new();
        body.insert("tool".into(), Value::Object(described));
        body.insert("actor".into(), Value::Object(fields(&[("agent", "claude")])));
        body.insert("outcome".into(), Value::Object(fields(&[("decision", "allow")])));
        record("tool_call", &session, body).unwrap();

        let rows = evidence_records(session.clone()).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        // The command, verbatim. Not the hash, not a truncation of it.
        assert_eq!(row.summary, "cargo tauri build --release");
        assert!(row.args.contains("cargo tauri build --release"), "full args missing: {}", row.args);
        assert_eq!(row.agent, "claude");
        assert_eq!(row.tool, "Bash");
        assert_eq!(row.decision, "allow");
        assert!(row.args_error.is_empty());
        assert!(row.summary != row.args_hash && !row.summary.starts_with("sha256:"));

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// When the arguments cannot be read, the row says so in words.
    ///
    /// A shredded session and a bundle exported without its blobs both land
    /// here. Rendering either as a blank line would make a destroyed key look
    /// exactly like a tool that took no arguments.
    #[test]
    fn an_unreadable_blob_gives_a_reason_not_a_blank_row() {
        let _guard = DIR_LOCK.lock().unwrap();
        let scratch = std::env::temp_dir().join(format!("xnaut-ev-{}", uuid::Uuid::new_v4()));
        std::env::set_var("XNAUT_EVIDENCE_DIR", &scratch);
        let session = format!("run-{}", uuid::Uuid::new_v4());

        let mut described = arguments(&session, &json!({ "command": "rm -rf /tmp/thing" }));
        let hash = described["args_hash"].as_str().unwrap().to_string();
        described.insert("name".into(), Value::String("Bash".into()));
        let mut body = Map::new();
        body.insert("tool".into(), Value::Object(described));
        record("tool_call", &session, body).unwrap();
        std::fs::remove_file(blob_path(&session, &hash)).unwrap();

        let rows = evidence_records(session.clone()).unwrap();
        assert!(rows[0].args.is_empty());
        assert!(!rows[0].args_error.is_empty(), "a blank row with no reason is the bug");
        assert!(rows[0].args_error.contains("could not read"), "{}", rows[0].args_error);
        // And the record itself still verifies: the chain does not depend on
        // the blob, which is exactly what makes a redacted bundle possible.
        assert_eq!(verify(&log_path()).unwrap(), 1);

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// An empty chain is never reported as verified.
    ///
    /// A green tick over zero records is indistinguishable, to a reader, from
    /// a green tick over a thousand that all held. That exact bug was fixed in
    /// the work report days ago and it must not grow back here.
    #[test]
    fn nothing_recorded_is_not_the_same_as_verified() {
        let _guard = DIR_LOCK.lock().unwrap();
        let scratch = std::env::temp_dir().join(format!("xnaut-ev-{}", uuid::Uuid::new_v4()));
        std::env::set_var("XNAUT_EVIDENCE_DIR", &scratch);

        // No file at all.
        let absent = evidence_verify();
        assert!(!absent.ok, "a missing log must never report ok");
        assert!(!absent.exists);
        assert_eq!(absent.records, 0);
        assert!(absent.checked.is_empty(), "nothing was checked, so nothing may be claimed");
        // The panel's empty state quotes this; without it the reader has no
        // file to go and look at.
        assert!(absent.path.ends_with("execution.jsonl"), "{}", absent.path);

        // Present and empty.
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(log_path(), "").unwrap();
        let empty = evidence_verify();
        assert!(empty.exists);
        assert!(!empty.ok, "an empty log must never report ok");
        assert!(empty.checked.is_empty());
        assert!(empty.broken.is_empty(), "empty is not broken; they are different sentences");

        // One real record, and only now does it verify.
        record("tool_call", "s", fields(&[("tool", "Read")])).unwrap();
        let full = evidence_verify();
        assert!(full.ok);
        assert_eq!(full.records, 1);
        assert_eq!(full.sessions, 1);
        assert!(!full.checked.is_empty(), "a tick must say what it checked");
        assert!(full.checked.iter().any(|c| c.contains("prev_hash")), "{:?}", full.checked);

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A broken chain reports where, and does not claim ok on the way past.
    #[test]
    fn a_broken_chain_reports_the_line_and_never_reports_ok() {
        let _guard = DIR_LOCK.lock().unwrap();
        let scratch = std::env::temp_dir().join(format!("xnaut-ev-{}", uuid::Uuid::new_v4()));
        std::env::set_var("XNAUT_EVIDENCE_DIR", &scratch);
        let session = format!("run-{}", uuid::Uuid::new_v4());
        for tool in ["Read", "Edit", "Bash"] {
            record("tool_call", &session, fields(&[("tool", tool)])).unwrap();
        }
        assert!(verify_report(&log_path()).ok);

        let body = std::fs::read_to_string(log_path()).unwrap();
        let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
        lines[1] = lines[1].replace("\"Edit\"", "\"Read\"");
        std::fs::write(log_path(), lines.join("\n") + "\n").unwrap();

        let report = verify_report(&log_path());
        assert!(!report.ok, "a tampered chain must never report ok");
        assert!(report.broken.contains("line 2"), "{}", report.broken);
        assert_eq!(report.records, 1, "only what verified before the break counts");
        assert!(report.checked.is_empty(), "a broken chain claims nothing");

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The session list says who did the work and what was refused.
    #[test]
    fn the_session_list_names_the_agent_and_counts_refusals() {
        let _guard = DIR_LOCK.lock().unwrap();
        let scratch = std::env::temp_dir().join(format!("xnaut-ev-{}", uuid::Uuid::new_v4()));
        std::env::set_var("XNAUT_EVIDENCE_DIR", &scratch);
        let session = format!("run-{}", uuid::Uuid::new_v4());
        for (kind, agent) in [("tool_call", "claude"), ("tool_refused", "claude"), ("tool_call", "codex")] {
            let mut body = Map::new();
            body.insert("actor".into(), Value::Object(fields(&[("agent", agent)])));
            record(kind, &session, body).unwrap();
        }
        let rows = evidence_sessions().unwrap();
        let row = rows.iter().find(|r| r.session_id == session).unwrap();
        assert_eq!(row.agents, vec!["claude".to_string(), "codex".to_string()], "sorted and deduped");
        assert_eq!(row.refused, 1);
        assert_eq!(row.records, 3);

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn mutating_one_record_breaks_every_link_after_it() {
        let _guard = DIR_LOCK.lock().unwrap();
        let scratch = std::env::temp_dir().join(format!("xnaut-ev-{}", uuid::Uuid::new_v4()));
        std::env::set_var("XNAUT_EVIDENCE_DIR", &scratch);
        let session = format!("run-{}", uuid::Uuid::new_v4());
        for tool in ["Read", "Edit", "Bash"] {
            record("tool_call", &session, fields(&[("tool", tool)])).unwrap();
        }
        let path = log_path();
        assert_eq!(verify(&path).unwrap(), 3);

        // Change one field in the middle record and leave its hash alone: the
        // edit anybody tampering would actually make.
        let body = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
        lines[1] = lines[1].replace("\"Edit\"", "\"Read\"");
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        assert!(verify(&path).unwrap_err().contains("line 2"));

        // And rehashing the forged record does not save it: the record after it
        // still links to the hash the original had.
        let mut forged: Map<String, Value> = serde_json::from_str(&lines[1]).unwrap();
        forged.insert("hash".into(), Value::String(record_hash(&forged).unwrap()));
        lines[1] = serde_json::to_string(&Value::Object(forged)).unwrap();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        assert!(verify(&path).unwrap_err().contains("line 3"));

        std::env::remove_var("XNAUT_EVIDENCE_DIR");
        let _ = std::fs::remove_dir_all(&scratch);
    }
}

#[cfg(test)]
mod live {
    use super::*;
    use serde_json::json;

    /// Read the real chain on this machine and print what the panel shows.
    ///
    /// Read-only: it opens the owner's log and blobs and writes nothing. This
    /// is how the Evidence tab is checked against real data rather than a
    /// fixture, because a fixture cannot tell you the log has 26 sessions of
    /// which 15 have no blobs at all. Run:
    /// `cargo test --bin xnaut -- --ignored --nocapture reads_the_real_chain`.
    #[test]
    #[ignore = "reads the owner's real evidence log; run with --ignored"]
    fn reads_the_real_chain() {
        let report = verify_report(&log_path());
        println!("VERIFY {}", serde_json::to_string(&report).unwrap());
        let mut sessions = evidence_sessions().unwrap();
        sessions.truncate(6);
        println!("SESSIONS {}", serde_json::to_string(&sessions).unwrap());
        if let Some(first) = sessions.iter().find(|s| s.records > 3) {
            let mut rows = evidence_records(first.session_id.clone()).unwrap();
            rows.truncate(8);
            println!("RECORDS {}", serde_json::to_string(&rows).unwrap());
        }
        // A refusal is the row a person came here for, so surface one whether
        // or not it landed in the sessions above.
        let refused: Vec<RecordRow> = evidence_sessions()
            .unwrap()
            .iter()
            .filter(|s| s.refused > 0)
            .flat_map(|s| evidence_records(s.session_id.clone()).unwrap())
            .filter(|r| r.kind == "tool_refused")
            .take(3)
            .collect();
        println!("REFUSED {}", serde_json::to_string(&refused).unwrap());
    }

    /// Writes a real session the end-to-end driver can seal, checkpoint,
    /// export and shred. `XNAUT_SEAL=1 XNAUT_EVIDENCE_DIR=<dir> cargo test
    /// --bin xnaut -- --ignored writes_a_real_sealed_session`.
    #[test]
    #[ignore = "leaves a session on disk for the end-to-end driver; run with --ignored"]
    fn writes_a_real_sealed_session() {
        let _lock = DIR_LOCK.lock();
        let session = std::env::var("XNAUT_LIVE_SESSION").unwrap_or_else(|_| "live-e2e".into());
        for (tool, input) in [
            ("Bash", json!({"command": "cargo tauri build --release"})),
            ("Edit", json!({"file_path": "/Users/andre/secret/app.rs", "old": "a", "new": "b"})),
            ("Bash", json!({"command": "curl -H 'Authorization: Bearer tok'",
                            "env": {"GITEA_ACCESS_TOKEN": "tok_should_not_appear"}})),
        ] {
            let mut described = arguments(&session, &input);
            described.insert("name".into(), Value::String(tool.into()));
            let mut body = Map::new();
            body.insert("tool".into(), Value::Object(described));
            body.insert("outcome".into(), Value::Object(fields(&[("decision", "allow")])));
            record("tool_call", &session, body).expect("record");
        }
        let log = std::fs::read_to_string(log_path()).unwrap();
        assert!(!log.contains("tok_should_not_appear"), "a secret reached the log");
        assert!(!log.contains("cargo tauri build"), "plaintext arguments reached the log");
        println!("session={session} dir={}", dir().display());
    }
}
