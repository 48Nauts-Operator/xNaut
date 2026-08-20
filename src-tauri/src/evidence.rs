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

/// Walk the log and recompute every hash and every link.
///
/// Returns how many records verified. An Err names the first break and stops:
/// past a broken link nothing downstream means anything, and continuing would
/// report a count that reads like partial success.
pub fn verify(path: &Path) -> Result<usize, String> {
    let body = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut expected: HashMap<String, (u64, Value)> = HashMap::new();
    let mut count = 0usize;
    for (index, line) in body.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let row: Map<String, Value> = serde_json::from_str(line)
            .map_err(|e| format!("line {}: corrupt, gap here: {e}", index + 1))?;
        let claimed = row.get("hash").and_then(Value::as_str).unwrap_or("").to_string();
        if record_hash(&row)? != claimed {
            return Err(format!("line {}: record does not hash to its own hash", index + 1));
        }
        let session = row.get("session_id").and_then(Value::as_str).unwrap_or("").to_string();
        let (seq, prev) = expected.get(&session).cloned().unwrap_or((0, Value::Null));
        if row.get("seq").and_then(Value::as_u64) != Some(seq) {
            return Err(format!("line {}: session {session} expected seq {seq}", index + 1));
        }
        if row.get("prev_hash") != Some(&prev) {
            return Err(format!("line {}: session {session} does not link to the record before it", index + 1));
        }
        expected.insert(session, (seq + 1, Value::String(claimed)));
        count += 1;
    }
    Ok(count)
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
