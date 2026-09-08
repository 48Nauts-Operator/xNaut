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
    Some(
        dirs::home_dir()?
            .join(".config")
            .join("xnaut")
            .join("looms"),
    )
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "weave".into()
    } else {
        s
    }
}

fn validate(w: &Weave) -> Result<(), String> {
    if w.spec != SPEC {
        return Err(format!("unsupported spec {:?} (expected {SPEC})", w.spec));
    }
    if w.kind != "Weave" {
        return Err(format!(
            "unsupported kind {:?} (expected \"Weave\")",
            w.kind
        ));
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
    let Some(dir) = looms_dir() else {
        return Ok(vec![]);
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Ok(vec![]);
    };
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
    out.sort_by_key(|a| a.name.to_lowercase());
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
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .any(|e| e.path().to_string_lossy().ends_with(".loom.json"))
        })
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
    #[serde(default)]
    pub model: String, // executor model/CLI ("claude-fable-5", "codex", …)
    #[serde(default)]
    pub cwd: String, // project dir or worktree the run executes in
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
    model: Option<String>,
    cwd: Option<String>,
) -> Result<RunRecord, String> {
    let dir = looms_dir().ok_or("no home dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let started = now_ms();
    let id = run_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("run-{started}"));
    // Same sanitization as loom_run, so the recorded log path matches the file
    // loom_run actually writes (an id with a space/underscore would otherwise
    // point the UI's log view at a file that doesn't exist).
    let rid: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let log = dir
        .join("runs")
        .join(format!("{rid}.log"))
        .to_string_lossy()
        .to_string();
    let rec = RunRecord {
        id,
        weave,
        goal,
        provider,
        started_ms: started,
        status: "started".into(),
        pid: pid.unwrap_or(0),
        log,
        model: model.unwrap_or_default(),
        cwd: cwd.unwrap_or_default(),
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
    // Ungated: refusing to start a run because the evidence disk is full would
    // be the right posture, but this is not the choke point that can enforce it
    // (the UI records the run after spawning the process). The tool calls the
    // run then makes are gated in veto.rs, so the block still happens, one step
    // later and where it can actually stop something.
    let _ = crate::evidence::record(
        "session_start",
        &rec.id,
        crate::evidence::fields(&[
            ("weave", rec.weave.as_str()),
            ("goal", rec.goal.as_str()),
            ("provider", rec.provider.as_str()),
            ("model", rec.model.as_str()),
            ("cwd", rec.cwd.as_str()),
            ("cwd_hash", &crate::evidence::path_hash(&rec.cwd)),
        ]),
    );
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
    out.sort_by_key(|r| std::cmp::Reverse(r.started_ms));
    out.truncate(limit);
    out
}

/// List recent runs, newest first (default 20). Later status wins per id.
#[tauri::command]
pub fn loom_runs_list(limit: Option<usize>) -> Result<Vec<RunRecord>, String> {
    let Some(path) = runs_path() else {
        return Ok(vec![]);
    };
    let Ok(body) = std::fs::read_to_string(&path) else {
        return Ok(vec![]);
    };
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
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    f.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    // Closes the session's chain. The record's own prev_hash IS the final head,
    // so nothing needs to carry it separately, and a run that ends without one
    // of these is visible as a chain with no terminator rather than as silence.
    //
    // mark is called more than once for some runs (the UI marks failed, then
    // cancelled). Each is recorded; the chain shows what was said and when,
    // which is more honest than collapsing them here.
    let mut body = crate::evidence::fields(&[("status", rec.status.as_str())]);
    let elapsed = now_ms().saturating_sub(rec.started_ms);
    body.insert("duration_ms".into(), serde_json::Value::from(elapsed));
    let _ = crate::evidence::record("session_end", &rec.id, body);
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
//
// WHAT THIS NO LONGER DECIDES (XNAUT-266): which agent binary to run, and what
// flags it needs to run unattended. It had its own `case "$MODEL" in codex*)`,
// which was a second answer to a question `agents::headless_command` already
// answers for the fleet — and a second answer is how a runtime ends up launched
// one way here and another way there. The line now arrives in
// .loom-agent-cmd.txt, and the session name in .loom-session.txt, both built by
// the same code the one launcher uses. What is left here is genuinely this
// runner's own: the live stream-json formatter, the redaction, and the desktop
// window.
const AGENT_RUNNER: &str = r#"#!/usr/bin/env bash
cd /workspace 2>/dev/null || cd .
: > .agent.log
# stream-json emits one JSON event per message/tool-call AS IT HAPPENS — unlike
# `--verbose` alone, which prints only the final result at the very end (so the
# log looked dead for the whole run). A tiny jq formatter turns each event into a
# readable line; raw JSON if jq is missing (still proves the agent is alive).
cat > .agent-fmt.sh <<'FMT'
#!/usr/bin/env bash
# Redact secrets BEFORE anything hits the log: credentials in URLs
# (scheme://user:pass@host) and KEY=value / "key": "value" shapes for
# password/secret/token/api-key names. The log is persisted + streamed to the UI.
redact() {
  sed -Eu \
    -e 's#(://[^/:@[:space:]]+:)[^@[:space:]]+@#\1*****@#g' \
    -e 's#(([Pp]assword|PASSWORD|[Pp]asswd|[Ss]ecret|SECRET|[Tt]oken|TOKEN|[Aa]pi[_-]?[Kk]ey|API[_-]?KEY|[Aa]ccess[_-]?[Kk]ey)["'"'"']?[[:space:]]*[=:][[:space:]]*["'"'"']?)[^[:space:]"'"'"']+#\1*****#g'
}
if [ "$1" != "raw" ] && command -v jq >/dev/null 2>&1; then
  jq -rc 'if .type=="assistant" then (.message.content[]? | if .type=="text" then .text elif .type=="tool_use" then "· "+.name+"  "+((.input.command // .input.file_path // .input.pattern // .input.description // "")|tostring|.[0:140]) else empty end) elif .type=="result" then "[done] "+((.num_turns//0)|tostring)+" turns · "+(((.duration_ms//0)/1000)|floor|tostring)+"s" else empty end' 2>/dev/null | redact
else
  redact
fi
FMT
chmod +x .agent-fmt.sh
# The agent command line and the session name are built by the one launcher
# (agents::headless_command, launch_env::session_name) and staged next to the
# goal. A missing file is a bug in loom_run, not something to guess around.
AGENT="$(cat .loom-agent-cmd.txt 2>/dev/null)"
SESSION="$(cat .loom-session.txt 2>/dev/null | tr -d '[:space:]')"
[ -n "$AGENT" ] || { echo "xNAUT: no agent command was staged for this run" >&2; exit 1; }
[ -n "$SESSION" ] || SESSION=nautloom
if ! command -v tmux >/dev/null 2>&1; then sudo apt-get install -y -q tmux >/dev/null 2>&1 || true; fi
if command -v tmux >/dev/null 2>&1; then
  tmux kill-session -t "$SESSION" 2>/dev/null || true
  tmux new-session -d -s "$SESSION" "cd /workspace && $AGENT | tee -a .agent.log; echo __AGENT_DONE__ >> .agent.log"
  DISPLAY=:0 setsid xfce4-terminal --maximize --title 'NautLoom agent' --command "tmux attach -t '$SESSION'" >/dev/null 2>&1 &
  echo "tmux: $SESSION  ·  attach: gitvm ssh, then  tmux attach -t $SESSION"
else
  printf '%s\n' "cd /workspace && $AGENT" > .agent-cmd.sh
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
pub async fn loom_run(
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
        let same =
            cwd_p == home || std::fs::canonicalize(cwd_p).ok() == std::fs::canonicalize(&home).ok();
        if same {
            return Err("refusing to run from your home directory — open a project first".into());
        }
    }
    let dir = looms_dir().ok_or("no home dir")?.join("runs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let rid: String = run_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let log_path = dir.join(format!("{rid}.log"));
    let script_path = dir.join(format!("{rid}.sh"));
    // Every agent run funnels through here, so this is the one place that can put
    // past learnings in front of every agent. Prepended rather than appended: the
    // goal ends with the instruction the agent acts on, and that should stay last.
    let project = crate::engram::project_from_cwd(&cwd);
    let goal = format!(
        "{}{goal}",
        crate::engram::recall_block(&project, &goal, 6).await
    );
    // Goal file in the project dir → synced into the sandbox by `gitvm run`.
    let _ = std::fs::write(
        std::path::Path::new(&cwd).join(".loom-goal.txt"),
        goal.as_bytes(),
    );
    // Model file → the runner injects `claude --model <it>`. Empty file = CLI default.
    //
    // "local" is special: it means the user's own LLM server (Settings → AI
    // Providers) rather than a named cloud model. Claude Code has no --model for
    // that, so the model file is left empty and the endpoint is passed as env
    // instead — LM Studio serves Anthropic's /v1/messages natively, so the
    // harness runs against it unchanged. Verified end to end against qwen.
    let model_val = model.unwrap_or_default();
    let local_harness = model_val.trim().eq_ignore_ascii_case("local");
    let local_env = if local_harness {
        let s = crate::settings::load_or_default();
        let base = crate::agents::anthropic_base(&s.llm.endpoint);
        if base.is_empty() {
            return Err(
                "No local model configured — set an endpoint under Settings → AI Providers".into(),
            );
        }
        Some((base, s.llm.model))
    } else {
        None
    };
    let _ = std::fs::write(
        std::path::Path::new(&cwd).join(".loom-model.txt"),
        if local_harness {
            b"".as_slice()
        } else {
            model_val.trim().as_bytes()
        },
    );
    // The agent command line, from the ONE place that knows how to run an agent
    // CLI headless (XNAUT-266). The runner used to work this out itself in
    // bash; a second copy of "which binary, which flags" is how a runtime gets
    // launched one way by the fleet and another way by a loom.
    //
    // The local-harness case deliberately passes no model, matching the empty
    // model file above: Claude Code has no flag for "the user's own LLM server"
    // and the endpoint travels as env instead.
    let effective_model = if local_harness { "" } else { model_val.trim() };
    let agent_command = crate::agents::headless_command(&crate::agents::Headless {
        model: effective_model,
        goal_file: ".loom-goal.txt",
        ..Default::default()
    })?;
    // codex prints plain text already; only the stream-json runtimes need the
    // live formatter, and `raw` still redacts.
    let (runtime, _) = crate::agents::runtime_for_model(effective_model);
    let formatter = if runtime == "claude" { "" } else { " raw" };
    let _ = std::fs::write(
        std::path::Path::new(&cwd).join(".loom-agent-cmd.txt"),
        format!("{agent_command} 2>&1 | ./.agent-fmt.sh{formatter}"),
    );
    // The sandbox session name is DERIVED the same way every other environment
    // derives one, so a loom agent in a sandbox is found again by the same
    // adoption path as a fleet agent (`cli::live_sessions_for`) instead of by a
    // hardcoded word only this file knew.
    let _ = std::fs::write(
        std::path::Path::new(&cwd).join(".loom-session.txt"),
        crate::sandbox::launch_env::session_name("nautloom", &rid),
    );
    // Agent runner (synced into the sandbox): runs claude in a tmux session you
    // can attach to, or falls back to a setsid-detached agent shown in an
    // xfce4-terminal on :0. Either way it's visible on the desktop, survives the
    // local driver dying, and streams its log to stdout (→ the Looms OUTPUT).
    let _ = std::fs::write(
        std::path::Path::new(&cwd).join(".loom-agent.sh"),
        AGENT_RUNNER,
    );
    // rm: don't litter the project dir (or the Obsidian vault, for persona runs)
    // with the per-run control files. A killed run still leaves them — acceptable.
    let full = format!(
        "#!/usr/bin/env bash\nset +e\ncd {}\n{}\ncode=$?\nrm -f .loom-goal.txt .loom-model.txt .loom-agent.sh .loom-agent-cmd.txt .loom-session.txt\necho \"__LOOM_DONE__ $code\"\n",
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
    if let Some((base, model_id)) = local_env {
        // ANTHROPIC_API_KEY is required as *an* auth source; the local server
        // ignores its value. Without ANTHROPIC_MODEL the CLI asks for a claude-*
        // model the local server cannot serve.
        cmd.env("ANTHROPIC_BASE_URL", base);
        cmd.env("ANTHROPIC_API_KEY", "local");
        if !model_id.trim().is_empty() {
            cmd.env("ANTHROPIC_MODEL", model_id);
        }
    }
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
        let quiet = |args: &[&str]| {
            let _ = std::process::Command::new("kill")
                .args(args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        };
        // `--` before the negative pid: Linux procps kill reads `-123` as an
        // option and delivers nothing (same bug as designer_local::stop).
        quiet(&["-TERM", "--", &format!("-{pid}")]);
        quiet(&["-TERM", &pid.to_string()]);
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }
    Ok(())
}

/// True if a coding agent (claude/codex/pi) is running WITH `cwd` as its working
/// directory — the Build manager's "is my developer actually alive" check. A
/// Zellij session outlives its agent (`claudeps …; exec zsh` decays to a bare
/// shell), so session liveness alone is not enough.
///
/// FAIL-SAFE, deliberately: every inconclusive answer is `true` ("assume alive").
/// The caller's response to `false` is to force-kill the Zellij session and
/// restart the agent, so a wrong `false` DESTROYS live work. A wrong `true` costs
/// a stall instead, which is cheaper but NOT free: it is only cheap while the
/// next check can still come back `false`. An inconclusive answer that can never
/// resolve is a permanent lie, which is how XNAUT-270 happened; keep the fail-safe
/// pointed at conditions that actually clear.
///
/// This was not hypothetical. The probe used to shell out to `pgrep -f …`, which
/// on this machine fails for EVERY pattern — even `pgrep -f xnaut`:
///
///     pgrep: Regular expression evaluation error (illegal byte sequence)
///
/// Not the regex; the LOCALE. Under en_US.UTF-8 some running process carries
/// argv bytes that are not valid UTF-8, and BSD pgrep aborts the whole scan
/// rather than skipping that one process. (`LC_ALL=C pgrep -f …` works, which is
/// how it was isolated.) It exits 3 with EMPTY stdout while the spawn itself
/// succeeds — so the `else` branch never fired, the pid loop had nothing to
/// iterate, and every call returned `false`. Every agent looked dead. On
/// 2026-08-08 that force-killed three healthy Guardian agents three times each
/// until all three slices went red.
///
/// `ps` + basename matching avoids it outright: no regex, no locale sensitivity
/// (from_utf8_lossy absorbs the bad bytes instead of aborting), and the parsing
/// is unit-testable without spawning processes.
///
/// ONLY THIS USER'S PROCESSES ARE CANDIDATES (XNAUT-270). The scan used to end on
/// a single machine-wide `unreadable` latch: if the cwd of ANY agent-named process
/// on the box could not be read, every directory reported alive. Measured on
/// macOS: `lsof -a -p <pid> -d cwd -Fn` against a process owned by another user
/// exits 1 with EMPTY stdout, which is exactly that branch; pid 1 and five other
/// root-owned pids all returned `rc=1, len=0`. So one root-owned process named
/// `claude`, anywhere on the machine, pinned every liveness answer for every
/// directory to "alive" permanently, with no recovery. The Build manager then
/// believed a dead developer was still working, forever.
///
/// xNAUT launches its agents as the current user, so a process owned by anyone
/// else is not our agent and gets to decide nothing. Filtering on uid removes the
/// root-owned processes before `lsof` is ever asked about them. What remains is a
/// per-process judgement: readable and matching is alive, readable and elsewhere
/// is not our agent, unreadable is unknown ABOUT THAT ONE PROCESS.
///
/// Async + `spawn_blocking`: a *sync* Tauri command runs on the main thread, and
/// this one spawns `ps` plus one `lsof` per agent and waits for each. Same reason
/// as `agents::agent_session_alive`.
#[tauri::command]
pub async fn agent_alive_in(cwd: String) -> bool {
    tokio::task::spawn_blocking(move || alive_in(&cwd))
        .await
        .unwrap_or(true) // cannot tell: never the destructive answer
}

fn alive_in(cwd: &str) -> bool {
    let Ok(ps) = std::process::Command::new("ps")
        .args(["-Ao", "pid=,uid=,comm="])
        .output()
    else {
        return true; // cannot tell, never the destructive answer
    };
    let table = String::from_utf8_lossy(&ps.stdout);
    // Our own uid comes out of the same table, which keeps the comparison in one
    // column's units and costs no extra process. Our row is always present; if it
    // is not, the table is not trustworthy enough to kill an agent over.
    let Some(uid) = self_uid(&table, std::process::id()) else {
        return true;
    };
    let pids = agent_pids(&table, uid);
    if pids.is_empty() {
        return false; // ps worked and no agent of ours is running anywhere: down
    }
    let want = std::fs::canonicalize(cwd).unwrap_or_else(|_| std::path::PathBuf::from(cwd));
    any_agent_in(&pids, &want, read_cwd)
}

/// A running process's working directory, or `None` when it could not be read.
fn read_cwd(pid: &str) -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("lsof")
        .args(["-a", "-p", pid, "-d", "cwd", "-Fn"])
        .output()
        .ok()?;
    // No `n` line means lsof would not tell us, not that the process has no cwd.
    // A refusal is exit 1 with EMPTY stdout (measured against pid 1 and five other
    // root-owned pids), and a process that vanished between `ps` and here looks the
    // same. Both read as unknown, which is right for both now that only our own
    // processes are ever asked.
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix('n').map(std::path::PathBuf::from))
}

/// Is one of `pids` sitting in `want`?
///
/// Decided PER PROCESS, which is the whole of XNAUT-270: a process whose cwd we
/// cannot read says nothing about the others, so it never ends the scan early and
/// never counts as a match. Only when every candidate has been asked, and none of
/// them is in `want`, does an unknown one still tip the answer to "alive" (see the
/// FAIL-SAFE note above). That residue is now bounded to our own processes.
// ponytail: an unreadable own-process is still treated as maybe-alive rather than
// probed further (re-checking whether the pid still exists would separate "it
// exited" from "lsof refused"). Not worth the extra process until it is measured
// to happen, since same-user lsof does not refuse.
fn any_agent_in(
    pids: &[String],
    want: &std::path::Path,
    read_cwd: impl Fn(&str) -> Option<std::path::PathBuf>,
) -> bool {
    let mut unknown = false;
    for pid in pids {
        match read_cwd(pid) {
            Some(dir) if dir == want => return true,
            Some(_) => {} // readable and elsewhere: definitively not our agent
            None => unknown = true,
        }
    }
    unknown
}

/// `(pid, uid, command)` from one `ps -Ao pid=,uid=,comm=` row. The command can
/// contain spaces, so only the first two fields are split off.
fn ps_row(line: &str) -> Option<(&str, &str, &str)> {
    let (pid, rest) = line.trim().split_once(char::is_whitespace)?;
    let (uid, cmd) = rest.trim_start().split_once(char::is_whitespace)?;
    Some((pid, uid, cmd.trim()))
}

/// The uid `ps` reports for our own process, read back out of its own output.
fn self_uid(ps_output: &str, self_pid: u32) -> Option<&str> {
    let me = self_pid.to_string();
    ps_output.lines().find_map(|line| {
        ps_row(line)
            .filter(|(pid, ..)| *pid == me)
            .map(|(_, uid, _)| uid)
    })
}

/// PIDs of `uid`'s running coding agents, from `ps -Ao pid=,uid=,comm=` output.
///
/// Matches on the executable's BASENAME so an absolute path
/// (`/Users/x/.local/bin/claude`) and a bare `claude` both count, while
/// `claude-agent-acp`, `pip` or any path merely CONTAINING the word do not.
/// Split out from the command purely so it can be tested against fixture text.
fn agent_pids(ps_output: &str, uid: &str) -> Vec<String> {
    ps_output
        .lines()
        .filter_map(|line| {
            let (pid, owner, cmd) = ps_row(line)?;
            if owner != uid {
                return None; // another user's process is never an agent of ours
            }
            let base = cmd.rsplit('/').next()?;
            matches!(base, "claude" | "codex" | "pi").then(|| pid.to_string())
        })
        .collect()
}

#[cfg(test)]
mod agent_alive_tests {
    use super::{agent_pids, any_agent_in, self_uid};
    use std::path::{Path, PathBuf};

    const ME: u32 = 900;

    #[test]
    fn matches_absolute_paths_and_bare_names() {
        let ps = "  101 501 /Users/cand0rian/.local/bin/claude\n 102 501 codex\n103 501 /opt/pi\n 900 501 xnaut\n";
        assert_eq!(agent_pids(ps, "501"), vec!["101", "102", "103"]);
    }

    #[test]
    fn rejects_lookalikes() {
        // claude-agent-acp is a DIFFERENT program that happens to start with the
        // same word; `pip`/`python` merely contain the letters of `pi`.
        let ps = "\
 201 501 node
 202 501 /Users/x/Library/Application Support/Buzz/node-tools/bin/claude-agent-acp
 203 501 /usr/bin/pip
 204 501 python3
 205 501 /Users/x/claude/cli.js
";
        assert!(agent_pids(ps, "501").is_empty());
    }

    #[test]
    fn tolerates_junk_lines() {
        // A header row or a blank line must not panic or produce a bogus pid.
        let ps = "  PID   UID COMM\n\n   \n 301 501 /usr/local/bin/claude\n";
        assert_eq!(agent_pids(ps, "501"), vec!["301"]);
    }

    /// XNAUT-270, the headline. A root-owned `claude` whose cwd `lsof` refuses to
    /// read (measured: `rc=1`, empty stdout, for pid 1 and five other root pids)
    /// used to pin EVERY directory to "alive" forever. It is not our process, so
    /// it must not answer for a directory that has no agent in it at all.
    #[test]
    fn a_foreign_unreadable_agent_does_not_report_an_empty_directory_alive() {
        let ps = "    1 0 /usr/local/bin/claude\n  900 501 xnaut\n";
        let uid = self_uid(ps, ME).expect("our own row is in the table");
        let pids = agent_pids(ps, uid);
        assert!(pids.is_empty(), "root's claude is not an agent of ours");
        assert!(!any_agent_in(&pids, Path::new("/tmp/wt"), |_| None));
    }

    /// And it must not mask a real, readable "that agent is somewhere else"
    /// either: the Build manager has to be able to see a dead developer.
    #[test]
    fn a_foreign_unreadable_agent_does_not_mask_our_agent_being_elsewhere() {
        let ps = "    1 0 claude\n    5 501 /usr/local/bin/claude\n  900 501 xnaut\n";
        let uid = self_uid(ps, ME).unwrap();
        let pids = agent_pids(ps, uid);
        assert_eq!(pids, vec!["5"]);
        assert!(!any_agent_in(&pids, Path::new("/tmp/a"), |_| Some(
            PathBuf::from("/tmp/b")
        )));
    }

    #[test]
    fn our_agent_in_that_directory_is_alive() {
        let ps = "    5 501 /usr/local/bin/claude\n  900 501 xnaut\n";
        let pids = agent_pids(ps, self_uid(ps, ME).unwrap());
        assert!(any_agent_in(&pids, Path::new("/tmp/wt"), |_| Some(
            PathBuf::from("/tmp/wt")
        )));
    }

    /// One unreadable process must not end the scan: the readable agent behind it
    /// is still the right answer.
    #[test]
    fn an_unreadable_process_does_not_stop_the_scan() {
        let pids = vec!["5".to_string(), "6".to_string()];
        let found = any_agent_in(&pids, Path::new("/tmp/wt"), |pid| {
            (pid == "6").then(|| PathBuf::from("/tmp/wt"))
        });
        assert!(found);
    }

    /// The deliberate fail-safe, kept: a wrong "dead" makes the Build manager
    /// force-kill a working developer, so our OWN unreadable agent still reads as
    /// alive rather than as absent.
    #[test]
    fn our_own_unreadable_agent_still_fails_safe_to_alive() {
        let ps = "    5 501 claude\n  900 501 xnaut\n";
        let pids = agent_pids(ps, self_uid(ps, ME).unwrap());
        assert_eq!(pids, vec!["5"]);
        assert!(any_agent_in(&pids, Path::new("/tmp/wt"), |_| None));
    }

    #[test]
    fn a_table_without_our_own_row_yields_no_uid() {
        // alive_in turns this into "cannot tell", never into a kill.
        assert!(self_uid("    1 0 /sbin/launchd\n", ME).is_none());
    }
}

/// Is a run's driver process still alive? (used to re-attach / detect a dead run)
#[tauri::command]
pub fn loom_run_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
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
        if fresh {
            std::fs::read_to_string(&p).unwrap_or_default()
        } else {
            String::new()
        }
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
            let meta = match e.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
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
    Ok(LoomReport {
        report_md: read("report.md"),
        status_json: read("status.json"),
        media,
    })
}

// ---- ship: branch + commit + push the agent's returned code ------------------
// After a green run the pulled /workspace diffs are shipped onto their own
// branch and pushed, so a PR can carry the review — main is never committed to.

#[derive(Debug, Clone, Serialize)]
pub struct ShipResult {
    pub branch: String,
    pub previous_branch: String,
    pub org_repo: String,
    pub commit: String,
}

fn parse_org_repo(url: &str) -> String {
    // forgejo:org/name.git · git@host:org/name.git · ssh://git@host:port/org/name.git · https://host/org/name
    let s = url.trim().trim_end_matches(".git");
    let tail = if let Some(i) = s.rfind(':') {
        let after = &s[i + 1..];
        if after.contains('/') && !after.starts_with("//") {
            after
        } else {
            s
        }
    } else {
        s
    };
    let parts: Vec<&str> = tail.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 {
        format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        String::new()
    }
}

#[tauri::command]
pub fn loom_ship(cwd: String, branch: String, message: String) -> Result<ShipResult, String> {
    let git = |args: &[&str]| -> Result<String, String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&cwd)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    };
    let branch: String = branch
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '/' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if branch.trim().is_empty() {
        return Err("branch name is required".into());
    }
    if git(&["status", "--porcelain"])?.trim().is_empty() {
        return Err("nothing to ship — working tree is clean".into());
    }
    let previous = git(&["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    git(&["checkout", "-B", &branch])?;
    // Stage everything EXCEPT run leftovers — works even in repos whose
    // .gitignore doesn't know about the loom files.
    git(&[
        "add",
        "-A",
        "--",
        ".",
        ":(exclude)artifacts",
        ":(exclude).loom-goal.txt",
        ":(exclude).loom-agent.sh",
        ":(exclude).loom-model.txt",
        ":(exclude).loom-agent-cmd.txt",
        ":(exclude).loom-session.txt",
        ":(exclude).agent-cmd.sh",
        ":(exclude).agent-fmt.sh",
        ":(exclude).agent.log",
        ":(exclude).gitvm",
    ])?;
    git(&["commit", "-m", &message])?;
    let commit = git(&["rev-parse", "--short", "HEAD"])?.trim().to_string();
    git(&["push", "-u", "origin", &branch])?;
    let url = git(&["remote", "get-url", "origin"])?.trim().to_string();
    Ok(ShipResult {
        branch,
        previous_branch: previous,
        org_repo: parse_org_repo(&url),
        commit,
    })
}

// ---- sandbox resource stats ---------------------------------------------------
// Polls the live sandbox over the same jump-host ssh the gitvm CLI uses (raw ssh,
// NO rsync — `gitvm run` would --delete the agent's work). CPU% from two
// /proc/stat samples 1s apart; mem from free; disk from df /workspace.
//
// The connection details and the ssh flags come from `sandbox::cli` (XNAUT-266):
// this function used to parse `.gitvm/state.json` and spell out the flags
// itself, which made three copies of one command line in the app, each free to
// drift on the timeout and on the default jump host.

#[derive(Debug, Clone, Serialize)]
pub struct SandboxStats {
    pub cpu_pct: f32,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub disk_pct: u32,
    pub cores: u32,
}

#[tauri::command]
pub async fn loom_sandbox_stats(cwd: String) -> Result<SandboxStats, String> {
    let guest = crate::sandbox::cli::guest(std::path::Path::new(&cwd))?;
    let script = "head -1 /proc/stat; sleep 1; head -1 /proc/stat; free -m | awk 'NR==2{print \"MEM\",$2,$3}'; df -m /workspace 2>/dev/null | awk 'NR==2{print \"DISK\",$5}'; echo CORES $(nproc)";
    let mut args = crate::sandbox::cli::ssh_opts(&guest);
    args.push(format!("root@{}", guest.ip));
    args.push(script.to_string());
    let out = tokio::process::Command::new("ssh")
        .args(&args)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "ssh failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut cpu_lines = Vec::new();
    let (mut mem_t, mut mem_u, mut disk, mut cores) = (0u64, 0u64, 0u32, 0u32);
    for line in text.lines() {
        if line.starts_with("cpu ") {
            let v: Vec<u64> = line
                .split_whitespace()
                .skip(1)
                .filter_map(|x| x.parse().ok())
                .collect();
            cpu_lines.push(v);
        } else if let Some(rest) = line.strip_prefix("MEM ") {
            let p: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            if p.len() >= 2 {
                mem_t = p[0];
                mem_u = p[1];
            }
        } else if let Some(rest) = line.strip_prefix("DISK ") {
            disk = rest.trim().trim_end_matches('%').parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("CORES ") {
            cores = rest.trim().parse().unwrap_or(0);
        }
    }
    let cpu_pct = if cpu_lines.len() >= 2 && cpu_lines[0].len() >= 5 {
        let (a, b) = (&cpu_lines[0], &cpu_lines[1]);
        let tot_a: u64 = a.iter().sum();
        let tot_b: u64 = b.iter().sum();
        let idle_a = a[3] + a.get(4).copied().unwrap_or(0);
        let idle_b = b[3] + b.get(4).copied().unwrap_or(0);
        let dt = tot_b.saturating_sub(tot_a);
        if dt > 0 {
            100.0 * (1.0 - (idle_b.saturating_sub(idle_a)) as f32 / dt as f32)
        } else {
            0.0
        }
    } else {
        0.0
    };
    Ok(SandboxStats {
        cpu_pct,
        mem_used_mb: mem_u,
        mem_total_mb: mem_t,
        disk_pct: disk,
        cores,
    })
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

// ---- static mock preview server -----------------------------------------------
// Serves a directory of self-contained design mocks over 127.0.0.1 so they can be
// opened clickable in the browser pane (real navigation between screens). One
// server per directory, reused across calls while the app lives.
#[tauri::command]
pub async fn static_serve(dir: String) -> Result<String, String> {
    let root = std::path::PathBuf::from(&dir);
    if !root.is_dir() {
        return Err(format!("not a directory: {}", dir));
    }
    static SERVERS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, u16>>> =
        std::sync::OnceLock::new();
    let servers = SERVERS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Some(p) = servers.lock().unwrap().get(&dir).copied() {
        if std::net::TcpStream::connect(("127.0.0.1", p)).is_ok() {
            return Ok(format!("http://127.0.0.1:{}", p));
        }
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    servers.lock().unwrap().insert(dir.clone(), port);
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            let root = root.clone();
            tauri::async_runtime::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/");
                let path = path.split(['?', '#']).next().unwrap_or("/");
                let mut rel = path.trim_start_matches('/');
                if rel.is_empty() {
                    rel = "screen-1.html";
                }
                // ponytail: flat mock dir — any ".." is rejected outright
                let body = if rel.contains("..") {
                    None
                } else {
                    tokio::fs::read(root.join(rel)).await.ok()
                };
                let resp = match body {
                    Some(b) => {
                        let mime = match rel.rsplit('.').next().unwrap_or("") {
                            "html" => "text/html; charset=utf-8",
                            "css" => "text/css",
                            "js" => "text/javascript",
                            "svg" => "image/svg+xml",
                            "png" => "image/png",
                            "jpg" | "jpeg" => "image/jpeg",
                            _ => "application/octet-stream",
                        };
                        let mut r = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                            mime,
                            b.len()
                        )
                        .into_bytes();
                        r.extend_from_slice(&b);
                        r
                    }
                    None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot found".to_vec(),
                };
                let _ = sock.write_all(&resp).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    Ok(format!("http://127.0.0.1:{}", port))
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
