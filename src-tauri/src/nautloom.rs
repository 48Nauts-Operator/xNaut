// NautLoom — the open, provider-agnostic playbook standard for AI-agent runs.
// A "Weave" (*.loom.json, spec "nautloom/v1") describes a run in a fixed shape:
// runtime (Terraform-style env + provider seam) + intent (the human's goal) +
// steps (Ansible-style ordered actions) + acceptance + report. xNaut is the
// reference implementation. See work:xnaut/Development/features/2026-07-14_NautLoom-Standard.md
//
// Storage: Weaves are reusable templates, so they live in a GLOBAL library at
// ~/.config/xnaut/looms/*.loom.json (not per-project). The human fills `intent`
// at run time; the rest is the reusable template.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const SPEC: &str = "nautloom/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "one")]
    pub version: u32,
}
fn one() -> u32 {
    1
}

/// A full Weave. The flexible sub-trees (runtime/intent/steps/report) stay as
/// JSON so the format can evolve without breaking the reader.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weave {
    pub spec: String,
    pub kind: String,
    pub metadata: Metadata,
    #[serde(default)]
    pub runtime: Value,
    #[serde(default)]
    pub intent: Value,
    #[serde(default)]
    pub steps: Vec<Value>,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub report: Value,
}

/// Lightweight listing entry for the Playbooks tab.
#[derive(Debug, Clone, Serialize)]
pub struct WeaveMeta {
    pub name: String,
    pub description: String,
    pub provider: String,
    pub path: String,
}

fn looms_dir() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".config").join("xnaut").join("looms"))
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "weave".into() } else { s }
}

fn validate(w: &Weave) -> Result<(), String> {
    if w.spec != SPEC {
        return Err(format!("unsupported spec {:?} (expected {SPEC})", w.spec));
    }
    if w.kind != "Weave" {
        return Err(format!("unsupported kind {:?} (expected \"Weave\")", w.kind));
    }
    if w.metadata.name.trim().is_empty() {
        return Err("metadata.name is required".into());
    }
    Ok(())
}

// ---- commands -------------------------------------------------------------

/// List the Weaves in the global library, name-sorted.
#[tauri::command]
pub fn looms_list() -> Result<Vec<WeaveMeta>, String> {
    let Some(dir) = looms_dir() else { return Ok(vec![]) };
    let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(vec![]) };
    let mut out: Vec<WeaveMeta> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".loom.json"))
        .filter_map(|p| {
            let w: Weave = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
            Some(WeaveMeta {
                name: w.metadata.name,
                description: w.metadata.description,
                provider: w
                    .runtime
                    .get("provider")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                path: p.to_string_lossy().into_owned(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// Read one Weave by absolute path.
#[tauri::command]
pub fn loom_read(path: String) -> Result<Weave, String> {
    let body = std::fs::read_to_string(&path).map_err(|e| format!("read {path}: {e}"))?;
    let w: Weave = serde_json::from_str(&body).map_err(|e| format!("parse {path}: {e}"))?;
    validate(&w)?;
    Ok(w)
}

/// Write (create or overwrite) a Weave. Returns the file path.
#[tauri::command]
pub fn loom_write(weave: Weave) -> Result<String, String> {
    validate(&weave)?;
    let dir = looms_dir().ok_or("no home dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create looms dir: {e}"))?;
    let path = dir.join(format!("{}.loom.json", sanitize(&weave.metadata.name)));
    let body = serde_json::to_string_pretty(&weave).map_err(|e| e.to_string())?;
    std::fs::write(&path, body + "\n").map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

/// Seed the 5 starter Weaves if the library is empty. Returns how many written.
#[tauri::command]
pub fn looms_seed_defaults() -> Result<usize, String> {
    let dir = looms_dir().ok_or("no home dir")?;
    let has_any = std::fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok()).any(|e| e.path().to_string_lossy().ends_with(".loom.json")))
        .unwrap_or(false);
    if has_any {
        return Ok(0);
    }
    let mut n = 0;
    for w in default_weaves() {
        loom_write(w)?;
        n += 1;
    }
    Ok(n)
}

// ---- run log --------------------------------------------------------------
// xNaut pushes a composed Weave to the terminal agent; it can't observe the
// sandbox loop's completion in the PoC. So we log run STARTS honestly and let
// the human mark one done. Produced artifacts loop back via the History tab.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub weave: String,
    pub goal: String,
    #[serde(default)]
    pub provider: String,
    pub started_ms: u64,
    pub status: String, // "started" | "done" | "cancelled"
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn runs_path() -> Option<PathBuf> {
    Some(looms_dir()?.parent()?.join("looms").join("runs.jsonl"))
}

/// Record a run start. Returns the created record.
#[tauri::command]
pub fn loom_run_record(weave: String, goal: String, provider: String) -> Result<RunRecord, String> {
    let dir = looms_dir().ok_or("no home dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let started = now_ms();
    let rec = RunRecord {
        id: format!("run-{started}"),
        weave,
        goal,
        provider,
        started_ms: started,
        status: "started".into(),
    };
    let path = runs_path().ok_or("no runs path")?;
    let line = serde_json::to_string(&rec).map_err(|e| e.to_string())? + "\n";
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    f.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    Ok(rec)
}

/// Collapse an append-only runs.jsonl body to the latest record per id,
/// newest-started first, truncated to `limit`. (mark appends a fresh line.)
fn collapse_runs(body: &str, limit: usize) -> Vec<RunRecord> {
    let mut by_id: std::collections::HashMap<String, RunRecord> = std::collections::HashMap::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok(r) = serde_json::from_str::<RunRecord>(line) {
            by_id.insert(r.id.clone(), r); // later line wins
        }
    }
    let mut out: Vec<RunRecord> = by_id.into_values().collect();
    out.sort_by(|a, b| b.started_ms.cmp(&a.started_ms));
    out.truncate(limit);
    out
}

/// List recent runs, newest first (default 20). Later status wins per id.
#[tauri::command]
pub fn loom_runs_list(limit: Option<usize>) -> Result<Vec<RunRecord>, String> {
    let Some(path) = runs_path() else { return Ok(vec![]) };
    let Ok(body) = std::fs::read_to_string(&path) else { return Ok(vec![]) };
    Ok(collapse_runs(&body, limit.unwrap_or(20)))
}

/// Mark a run's status ("done" | "cancelled" | "started"). Appends a new line.
#[tauri::command]
pub fn loom_run_mark(id: String, status: String) -> Result<(), String> {
    let path = runs_path().ok_or("no runs path")?;
    let body = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut rec = body
        .lines()
        .filter_map(|l| serde_json::from_str::<RunRecord>(l).ok())
        .find(|r| r.id == id)
        .ok_or_else(|| format!("run {id} not found"))?;
    rec.status = status;
    let line = serde_json::to_string(&rec).map_err(|e| e.to_string())? + "\n";
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).map_err(|e| e.to_string())?;
    f.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

// ---- starter library ------------------------------------------------------

fn weave(name: &str, desc: &str, steps: Value, acceptance: Value, tools: Value) -> Weave {
    serde_json::from_value(json!({
        "spec": SPEC, "kind": "Weave",
        "metadata": { "name": name, "description": desc, "author": "48nauts", "version": 1 },
        "runtime": {
            "provider": "gitvm", "template": "agent-desktop",
            "resources": { "vcpus": 4, "memoryMB": 8192, "ttl": 21600 },
            "tools": tools
        },
        "intent": { "goal": "", "inputs": [] },
        "steps": steps,
        "acceptance": acceptance,
        "report": { "to": "agentic", "include": ["summary", "tests", "video"] }
    }))
    .expect("valid starter weave")
}

fn default_weaves() -> Vec<Weave> {
    vec![
        weave(
            "build-verify",
            "Warm up a GitVM sandbox, build + test until green, record a video, pull artifacts, report back.",
            json!([
                { "id": "warmup",  "action": "provision" },
                { "id": "sync",    "action": "sync" },
                { "id": "work",    "action": "agent", "with": { "agent": "claude", "goal": "$intent.goal" } },
                { "id": "verify",  "action": "test",  "loop": { "until": "pass", "max": 5 } },
                { "id": "record",  "action": "record" },
                { "id": "collect", "action": "artifacts" },
                { "id": "cleanup", "action": "teardown" }
            ]),
            json!(["tests green", "recording exists"]),
            json!(["playwright", "codex"]),
        ),
        weave(
            "quick-run",
            "Warm up, do the task in the sandbox, report. Minimal — no video.",
            json!([
                { "id": "warmup",  "action": "provision" },
                { "id": "work",    "action": "agent", "with": { "agent": "claude", "goal": "$intent.goal" } },
                { "id": "collect", "action": "artifacts" }
            ]),
            json!(["task done", "summary produced"]),
            json!([]),
        ),
        weave(
            "ui-record",
            "Spin up agent-desktop, run the UI/Playwright flow headed, screen-record it, report with the video.",
            json!([
                { "id": "warmup",  "action": "provision" },
                { "id": "work",    "action": "agent", "with": { "agent": "claude", "goal": "$intent.goal", "headed": true } },
                { "id": "record",  "action": "record" },
                { "id": "collect", "action": "artifacts" }
            ]),
            json!(["a non-empty recording exists"]),
            json!(["playwright"]),
        ),
        weave(
            "swarm",
            "Lead agent spawns specialist sub-agents in their own sandboxes, coordinating via the mesh, then synthesizes.",
            json!([
                { "id": "warmup",   "action": "provision" },
                { "id": "decompose","action": "agent", "with": { "agent": "claude", "goal": "$intent.goal", "role": "lead" } },
                { "id": "dispatch", "action": "spawn", "with": { "provider": "gitvm", "agents": ["codex"], "coordinate": "mesh" } },
                { "id": "synthesize","action": "agent", "with": { "agent": "claude", "role": "lead" } },
                { "id": "collect",  "action": "artifacts" },
                { "id": "cleanup",  "action": "teardown" }
            ]),
            json!(["synthesis produced", "sub-agent artifacts collected"]),
            json!(["codex"]),
        ),
        weave(
            "blank",
            "No template — the human's instructions are pushed verbatim to the agent.",
            json!([]),
            json!([]),
            json!([]),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_weaves_round_trip() {
        for w in default_weaves() {
            validate(&w).unwrap();
            let s = serde_json::to_string(&w).unwrap();
            let back: Weave = serde_json::from_str(&s).unwrap();
            assert_eq!(back.spec, SPEC);
            assert_eq!(back.kind, "Weave");
            assert!(!back.metadata.name.is_empty());
        }
    }

    #[test]
    fn validate_rejects_bad_spec_and_kind() {
        let mut w = default_weaves().remove(0);
        w.spec = "loop/v9".into();
        assert!(validate(&w).is_err());
        let mut w2 = default_weaves().remove(0);
        w2.kind = "Playbook".into();
        assert!(validate(&w2).is_err());
    }

    #[test]
    fn runs_collapse_latest_status_wins() {
        let body = "\
{\"id\":\"run-1\",\"weave\":\"build-verify\",\"goal\":\"a\",\"provider\":\"gitvm\",\"started_ms\":100,\"status\":\"started\"}
{\"id\":\"run-2\",\"weave\":\"quick-run\",\"goal\":\"b\",\"provider\":\"gitvm\",\"started_ms\":200,\"status\":\"started\"}
{\"id\":\"run-1\",\"weave\":\"build-verify\",\"goal\":\"a\",\"provider\":\"gitvm\",\"started_ms\":100,\"status\":\"done\"}
";
        let out = collapse_runs(body, 20);
        assert_eq!(out.len(), 2, "two distinct ids");
        assert_eq!(out[0].id, "run-2", "newest started first");
        let r1 = out.iter().find(|r| r.id == "run-1").unwrap();
        assert_eq!(r1.status, "done", "later mark wins");
        assert_eq!(collapse_runs(body, 1).len(), 1, "limit applied");
    }

    #[test]
    fn sanitize_name() {
        assert_eq!(sanitize("Build & Verify"), "Build---Verify");
        assert_eq!(sanitize("../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize(""), "weave");
    }
}
