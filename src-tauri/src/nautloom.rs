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
    pub status: String, // "started" | "done" | "failed" | "cancelled"
    #[serde(default)]
    pub pid: u32, // driver pid (0 if unknown) — lets the UI tell active from stale
    #[serde(default)]
    pub log: String, // absolute path to runs/<id>.log
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

/// Record a run start. `run_id` links the record to its runs/<id>.log; `pid` is
/// the driver process so the UI can tell an active session from a stale one.
#[tauri::command]
pub fn loom_run_record(
    run_id: Option<String>,
    weave: String,
    goal: String,
    provider: String,
    pid: Option<u32>,
) -> Result<RunRecord, String> {
    let dir = looms_dir().ok_or("no home dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let started = now_ms();
    let id = run_id.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| format!("run-{started}"));
    let log = dir.join("runs").join(format!("{id}.log")).to_string_lossy().to_string();
    let rec = RunRecord {
        id,
        weave,
        goal,
        provider,
        started_ms: started,
        status: "started".into(),
        pid: pid.unwrap_or(0),
        log,
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

// ---- executor ---------------------------------------------------------------
// Run a resolved loom script in the project dir, streaming to a log file the UI
// tails via read_file. The run's goal is written to .loom-goal.txt in cwd so the
// agent step reads it (and `gitvm run` rsyncs it into the sandbox) — no need to
// CLI-quote a ticket-sized prompt. Detached; returns pid + log path.

#[derive(Debug, Clone, Serialize)]
pub struct RunProc {
    pub pid: u32,
    pub log: String,
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// Runs the agent visibly + survivably in the sandbox: a tmux session you can
// attach to (falls back to setsid + an xfce4-terminal on :0 if tmux is absent),
// streaming its log to stdout until it finishes. Reads the enriched brief from
// .loom-goal.txt (goal + acceptance + iterate-until-green).
const AGENT_RUNNER: &str = r#"#!/usr/bin/env bash
cd /workspace 2>/dev/null || cd .
: > .agent.log
# stream-json emits one JSON event per message/tool-call AS IT HAPPENS — unlike
# `--verbose` alone, which prints only the final result at the very end (so the
# log looked dead for the whole run). A tiny jq formatter turns each event into a
# readable line; raw JSON if jq is missing (still proves the agent is alive).
cat > .agent-fmt.sh <<'FMT'
#!/usr/bin/env bash
if command -v jq >/dev/null 2>&1; then
  jq -rc 'if .type=="assistant" then (.message.content[]? | if .type=="text" then .text elif .type=="tool_use" then "· "+.name+"  "+((.input.command // .input.file_path // .input.pattern // .input.description // "")|tostring|.[0:140]) else empty end) elif .type=="result" then "[done] "+((.num_turns//0)|tostring)+" turns · "+(((.duration_ms//0)/1000)|floor|tostring)+"s" else empty end' 2>/dev/null
else
  cat
fi
FMT
chmod +x .agent-fmt.sh
MODEL="$(cat .loom-model.txt 2>/dev/null | tr -d '[:space:]')"; MF=""; [ -n "$MODEL" ] && MF="--model $MODEL"
CLAUDE="claude -p --verbose --output-format stream-json $MF --dangerously-skip-permissions \"\$(cat .loom-goal.txt)\""
if ! command -v tmux >/dev/null 2>&1; then sudo apt-get install -y -q tmux >/dev/null 2>&1 || true; fi
if command -v tmux >/dev/null 2>&1; then
  tmux kill-session -t nautloom 2>/dev/null || true
  tmux new-session -d -s nautloom "cd /workspace && $CLAUDE 2>&1 | ./.agent-fmt.sh | tee -a .agent.log; echo __AGENT_DONE__ >> .agent.log"
  DISPLAY=:0 setsid xfce4-terminal --maximize --title 'NautLoom agent' --command 'tmux attach -t nautloom' >/dev/null 2>&1 &
  echo "tmux: nautloom  ·  attach: gitvm ssh, then  tmux attach -t nautloom"
else
  printf '%s\n' "cd /workspace && $CLAUDE 2>&1 | ./.agent-fmt.sh" > .agent-cmd.sh
  DISPLAY=:0 setsid xfce4-terminal --maximize --title 'NautLoom agent' --command "bash -lc 'tail -f /workspace/.agent.log'" >/dev/null 2>&1 &
  # PTY via `script` so the pipe stays line-buffered and streams live.
  setsid bash -c 'script -qefc "bash /workspace/.agent-cmd.sh" /dev/null >> /workspace/.agent.log 2>&1; echo __AGENT_DONE__ >> /workspace/.agent.log' >/dev/null 2>&1 &
  echo "watch: gitvm ssh, then  tail -f /workspace/.agent.log"
fi
tail -f .agent.log &
TP=$!
for _ in $(seq 1 5400); do grep -q __AGENT_DONE__ .agent.log && break; sleep 1; done
kill $TP 2>/dev/null
echo "[agent step complete]"
"#;

#[tauri::command]
pub fn loom_run(
    run_id: String,
    script: String,
    goal: String,
    cwd: String,
    model: Option<String>,
) -> Result<RunProc, String> {
    if cwd.trim().is_empty() {
        return Err("no project directory — open a project first".into());
    }
    // Never run from $HOME: a gitvm loom would rsync your entire home folder.
    if let Some(home) = dirs::home_dir() {
        let cwd_p = std::path::Path::new(&cwd);
        let same = cwd_p == home
            || std::fs::canonicalize(cwd_p).ok() == std::fs::canonicalize(&home).ok();
        if same {
            return Err("refusing to run from your home directory — open a project first".into());
        }
    }
    let dir = looms_dir().ok_or("no home dir")?.join("runs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let rid: String = run_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    let log_path = dir.join(format!("{rid}.log"));
    let script_path = dir.join(format!("{rid}.sh"));
    // Goal file in the project dir → synced into the sandbox by `gitvm run`.
    let _ = std::fs::write(std::path::Path::new(&cwd).join(".loom-goal.txt"), goal.as_bytes());
    // Model file → the runner injects `claude --model <it>`. Empty file = CLI default.
    let model_val = model.unwrap_or_default();
    let _ = std::fs::write(
        std::path::Path::new(&cwd).join(".loom-model.txt"),
        model_val.trim().as_bytes(),
    );
    // Agent runner (synced into the sandbox): runs claude in a tmux session you
    // can attach to, or falls back to a setsid-detached agent shown in an
    // xfce4-terminal on :0. Either way it's visible on the desktop, survives the
    // local driver dying, and streams its log to stdout (→ the Looms OUTPUT).
    let _ = std::fs::write(std::path::Path::new(&cwd).join(".loom-agent.sh"), AGENT_RUNNER);
    let full = format!(
        "#!/usr/bin/env bash\nset +e\ncd {}\n{}\ncode=$?\necho \"__LOOM_DONE__ $code\"\n",
        shell_quote(&cwd),
        script
    );
    std::fs::write(&script_path, full).map_err(|e| e.to_string())?;
    let logf = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let logf2 = logf.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("bash");
    cmd.arg(&script_path)
        .current_dir(&cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(logf))
        .stderr(std::process::Stdio::from(logf2));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // own group so Stop can kill the whole pipeline
    }
    let child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    Ok(RunProc {
        pid: child.id(),
        log: log_path.to_string_lossy().into_owned(),
    })
}

/// Stop a running loom by killing its process group.
#[tauri::command]
pub fn loom_run_stop(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill").arg("-TERM").arg(format!("-{pid}")).status();
        let _ = std::process::Command::new("kill").arg("-TERM").arg(pid.to_string()).status();
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }
    Ok(())
}

/// Is a run's driver process still alive? (used to re-attach / detect a dead run)
#[tauri::command]
pub fn loom_run_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

// ---- run report -------------------------------------------------------------
// The agent writes /artifacts/report.md + status.json in the sandbox; the loom
// pulls them to <cwd>/artifacts/. This gathers them for the Output report card.
// `since_ms` filters media by mtime so stale files from earlier runs (rsync has
// no --delete on artifacts pull) never show up in the wrong report.

#[derive(Debug, Clone, Serialize)]
pub struct ReportMedia {
    pub name: String,
    pub path: String,
    pub kind: String, // "image" | "video"
    pub modified_ms: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoomReport {
    pub report_md: String,
    pub status_json: String,
    pub media: Vec<ReportMedia>,
}

#[tauri::command]
pub fn loom_report(cwd: String, since_ms: Option<u64>) -> Result<LoomReport, String> {
    let dir = std::path::Path::new(&cwd).join("artifacts");
    let since = since_ms.unwrap_or(0);
    // Only surface report/status written during THIS run (mtime >= since).
    let read = |n: &str| -> String {
        let p = dir.join(n);
        let fresh = std::fs::metadata(&p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| (d.as_millis() as u64) >= since)
            .unwrap_or(false);
        if fresh { std::fs::read_to_string(&p).unwrap_or_default() } else { String::new() }
    };
    let mut media = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let kind = match p.extension().and_then(|x| x.to_str()) {
                Some("png") | Some("jpg") | Some("jpeg") | Some("webp") => "image",
                Some("mp4") | Some("webm") | Some("mov") => "video",
                _ => continue,
            };
            let meta = match e.metadata() { Ok(m) => m, Err(_) => continue };
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            if modified_ms < since || meta.len() < 1024 {
                continue; // stale (earlier run) or empty stub
            }
            media.push(ReportMedia {
                name,
                path: p.to_string_lossy().to_string(),
                kind: kind.into(),
                modified_ms,
                size: meta.len(),
            });
        }
    }
    media.sort_by_key(|m| m.modified_ms);
    Ok(LoomReport { report_md: read("report.md"), status_json: read("status.json"), media })
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
