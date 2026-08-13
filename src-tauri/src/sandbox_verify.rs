// ABOUTME: Sandbox Verify orchestration (XNAUT-19). Resolve what to run for a
// ABOUTME: target repo (`.xnaut/verify.json`, else Node auto-detect), run it in
// ABOUTME: a fresh GitVM sandbox, record every step, and on green append the
// ABOUTME: proof to the ticket. Steps 4-7 of the Phase 1 plan.
#![allow(dead_code)] // video proof (step 9) still consumes more of this.

use crate::sandbox::cli as gvm;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager};

fn default_template() -> String {
    "pi-dev".into()
}
fn default_retries() -> u32 {
    1
}

/// Shape of `.xnaut/verify.json` (all command fields optional).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyConfig {
    #[serde(default = "default_template")]
    pub template: String,
    #[serde(default)]
    pub install: Option<String>,
    #[serde(default)]
    pub build: Option<String>,
    #[serde(default)]
    pub test: Option<String>,
    /// Times to re-run the test step for flakes before failing. Default 1.
    #[serde(default = "default_retries")]
    pub retries: u32,
    /// Optional: command to start the app (for web targets / video proof).
    #[serde(default)]
    pub start: Option<String>,
    /// Optional: HTTP path to poll for 200 once started.
    #[serde(default)]
    pub health_path: Option<String>,
    /// Optional: env file to ship into the sandbox.
    #[serde(default)]
    pub env_file: Option<String>,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            template: default_template(),
            install: None,
            build: None,
            test: None,
            retries: default_retries(),
            start: None,
            health_path: None,
            env_file: None,
        }
    }
}

/// One command to run in the sandbox, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedStep {
    pub name: String,
    pub command: String,
}

/// Resolve the verify plan for a repo: explicit `.xnaut/verify.json` wins;
/// else Node auto-detect from package.json; else a clear error.
pub fn load_verify_plan(repo_dir: &Path) -> Result<(VerifyConfig, Vec<PlannedStep>), String> {
    let explicit = repo_dir.join(".xnaut").join("verify.json");
    let config = if explicit.is_file() {
        let body = std::fs::read_to_string(&explicit)
            .map_err(|e| format!("reading {}: {e}", explicit.display()))?;
        serde_json::from_str::<VerifyConfig>(&body)
            .map_err(|e| format!("parsing .xnaut/verify.json: {e}"))?
    } else if repo_dir.join("package.json").is_file() {
        node_autodetect(repo_dir)?
    } else {
        return Err(
            "no .xnaut/verify.json and no package.json — add .xnaut/verify.json describing how to install/build/test"
                .into(),
        );
    };
    let mut steps = config_to_steps(&config);
    if let Some(gate) = find_build_gate(repo_dir) {
        steps.push(PlannedStep {
            name: "gate".into(),
            // uv run: the Validator writes it with a PEP 723 header, so its deps
            // are declared inline and there is nothing to install first.
            command: format!("uv run {gate} || python3 {gate}"),
        });
    }
    Ok((config.clone(), steps))
}

/// The Validator's acceptance gate, if this repo has one.
///
/// NautFlow's Validator writes `95-Build-Gate.py` next to the stage documents:
/// concrete behavioural checks against the BUILT product — files exist with real
/// content, commands exit 0, HTTP endpoints answer, pages contain what the spec
/// demands — one `PASS:`/`FAIL:` line each, exit 0 only if all pass. Until now
/// nothing executed it: the only two references in the tree were the prompt that
/// creates it and the reset that deletes it (XNAUT-64).
///
/// Looked up beside the repo and one level down, because the gate lives with the
/// NAUT-Flow documents rather than at the product root.
fn find_build_gate(repo_dir: &Path) -> Option<String> {
    const GATE: &str = "95-Build-Gate.py";
    if repo_dir.join(GATE).is_file() {
        return Some(GATE.to_string());
    }
    let entries = std::fs::read_dir(repo_dir).ok()?;
    for entry in entries.flatten() {
        if entry.path().is_dir() && entry.path().join(GATE).is_file() {
            let dir = entry.file_name().to_string_lossy().into_owned();
            // Shell-safe: only accept a plain directory name.
            if dir
                .chars()
                .all(|c| c.is_alphanumeric() || "-_.".contains(c))
            {
                return Some(format!("{dir}/{GATE}"));
            }
        }
    }
    None
}

/// Node defaults, gated on which scripts actually exist in package.json:
/// `npm ci`, then `npm run build` / `npm test` only if those scripts are defined.
fn node_autodetect(repo_dir: &Path) -> Result<VerifyConfig, String> {
    let pkg_body = std::fs::read_to_string(repo_dir.join("package.json"))
        .map_err(|e| format!("reading package.json: {e}"))?;
    let pkg: serde_json::Value =
        serde_json::from_str(&pkg_body).map_err(|e| format!("parsing package.json: {e}"))?;
    let has_script = |name: &str| {
        pkg.get("scripts")
            .and_then(|s| s.get(name))
            .and_then(serde_json::Value::as_str)
            .is_some()
    };
    Ok(VerifyConfig {
        install: Some("npm ci".into()),
        build: if has_script("build") {
            Some("npm run build".into())
        } else {
            None
        },
        test: if has_script("test") {
            Some("npm test".into())
        } else {
            None
        },
        ..VerifyConfig::default()
    })
}

fn config_to_steps(config: &VerifyConfig) -> Vec<PlannedStep> {
    let mut steps = Vec::new();
    let mut push = |name: &str, cmd: &Option<String>| {
        if let Some(command) = cmd {
            if !command.trim().is_empty() {
                steps.push(PlannedStep {
                    name: name.into(),
                    command: command.clone(),
                });
            }
        }
    };
    push("install", &config.install);
    push("build", &config.build);
    push("test", &config.test);
    steps
}

// ─── Verify engine (step 5a) ────────────────────────────────────────────────

/// One step's result inside a verify run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyStep {
    pub name: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub log_tail: String,
}

/// Persisted record of one verification run (one JSON per run).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyRecord {
    pub id: String,
    pub run_id: String,
    pub ticket_id: String,
    pub project: String,
    pub repo_path: String,
    pub provider_kind: String,
    pub sandbox_id: String,
    pub public_url: String,
    /// running | passed | failed | cancelled
    pub status: String,
    pub steps: Vec<VerifyStep>,
    pub log_dir: String,
    pub video_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn records_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut").join("sandbox-verify").join("records"))
        .unwrap_or_else(|| PathBuf::from(".xnaut-sandbox-verify"))
}

fn write_verify_record(record: &VerifyRecord) -> Result<(), String> {
    let dir = records_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create records dir: {e}"))?;
    let body = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{}.json", record.id)), body)
        .map_err(|e| format!("write record: {e}"))
}

/// Keep the last `max` chars of a (possibly huge) log, char-boundary safe.
fn tail_of(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let tail: String = {
        let mut v: Vec<char> = s.chars().rev().take(max).collect();
        v.reverse();
        v.into_iter().collect()
    };
    format!("…{tail}")
}

const LOG_TAIL_CHARS: usize = 4000;

/// Run a verification: warm the directory's sandbox → run the steps → record →
/// pull the work back → destroy. Returns the final record (passed/failed) and
/// never leaves a sandbox running.
///
/// Uses `sandbox::cli`, the one GitVM entry point (XNAUT-62), not the HTTP
/// SandboxDriver: that seam needs a provider in `settings.sandboxes`, which is
/// empty on a normal install, and nothing in production ever constructed one —
/// `SandboxDriver::for_settings` appears only in this crate's tests. That is why
/// this module has never run.
///
/// The CLI is directory-scoped and `gitvm run` rsyncs the directory into
/// /workspace itself, so the base64 tarball shipping this used to do is gone
/// rather than ported.
pub async fn run_verify(
    app: &tauri::AppHandle,
    repo_dir: &Path,
    ticket_id: &str,
    project: &str,
    run_id: &str,
    config: &VerifyConfig,
    steps: &[PlannedStep],
) -> Result<VerifyRecord, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let mut record = VerifyRecord {
        id: id.clone(),
        run_id: run_id.into(),
        ticket_id: ticket_id.into(),
        project: project.into(),
        repo_path: repo_dir.to_string_lossy().into_owned(),
        provider_kind: "gitvm-cli".into(),
        sandbox_id: String::new(),
        public_url: String::new(),
        status: "running".into(),
        steps: steps
            .iter()
            .map(|s| VerifyStep {
                name: s.name.clone(),
                command: s.command.clone(),
                exit_code: None,
                log_tail: String::new(),
            })
            .collect(),
        log_dir: records_dir().join(&id).to_string_lossy().into_owned(),
        video_path: None,
        created_at: now.clone(),
        updated_at: now,
    };
    write_verify_record(&record)?;
    emit(app, &record);

    let dir = repo_dir.to_path_buf();
    let warmed = tokio::task::spawn_blocking(move || {
        // A reaped VM leaves .gitvm/state.json behind and warm-up refuses while
        // it exists, wedging the directory. Clear it only when the control plane
        // agrees the sandbox is really gone.
        if gvm::state_is_stale(&dir) {
            let _ = gvm::stop(&dir);
        }
        gvm::warm_up(&dir)?;
        gvm::public_url(&dir)
    })
    .await
    .map_err(|e| e.to_string())?;
    match warmed {
        Ok(url) => {
            record.public_url = url;
            record.sandbox_id = "gitvm".into();
            let _ = write_verify_record(&record);
        }
        Err(error) => return Err(fail(app, &mut record, error)),
    }

    let result = run_steps(app, repo_dir, config, steps, &mut record).await;

    // Pull BEFORE stop — teardown destroys /workspace (XNAUT-40) and the gate
    // may have written reports we want. Both are best-effort: the verdict is
    // already recorded and the sandbox self-destructs at its TTL regardless.
    let dir = repo_dir.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        let _ = gvm::pull(&dir);
        gvm::stop(&dir)
    })
    .await;

    record.status = match &result {
        Ok(true) => "passed",
        Ok(false) => "failed",
        Err(_) => "failed",
    }
    .into();
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_verify_record(&record)?;
    emit(app, &record);
    result.map(|_| record)
}

fn emit(app: &tauri::AppHandle, record: &VerifyRecord) {
    let _ = app.emit("sandbox-verify-changed", record);
}

fn fail(app: &tauri::AppHandle, record: &mut VerifyRecord, error: String) -> String {
    record.status = "failed".into();
    record.updated_at = chrono::Utc::now().to_rfc3339();
    let _ = write_verify_record(record);
    emit(app, record);
    error
}

async fn run_steps(
    app: &tauri::AppHandle,
    repo_dir: &Path,
    config: &VerifyConfig,
    steps: &[PlannedStep],
    record: &mut VerifyRecord,
) -> Result<bool, String> {
    let mut all_ok = true;
    for (index, step) in steps.iter().enumerate() {
        // Only the test step retries (flake tolerance); install/build run once.
        let attempts = if step.name == "test" {
            config.retries.max(1)
        } else {
            1
        };
        let mut last: Option<(i32, String)> = None;
        for _ in 0..attempts {
            let dir = repo_dir.to_path_buf();
            let command = format!("cd /workspace && {}", step.command);
            let out = tokio::task::spawn_blocking(move || gvm::run(&dir, &command))
                .await
                .map_err(|e| e.to_string())??;
            let code = out.status.code().unwrap_or(-1);
            let text = gvm::text(&out);
            let ok = code == 0;
            last = Some((code, text));
            if ok {
                break;
            }
        }
        let (code, text) = last.expect("attempts >= 1");
        record.steps[index].exit_code = Some(code);
        record.steps[index].log_tail = tail_of(&text, LOG_TAIL_CHARS);
        record.updated_at = chrono::Utc::now().to_rfc3339();
        let _ = write_verify_record(record);
        emit(app, record);
        if code != 0 {
            all_ok = false;
            break; // stop at the first red step
        }
    }
    Ok(all_ok)
}

// ─── Commands (steps 6-7) ───────────────────────────────────────────────────

/// Where a green run leaves the ticket.
///
/// The design doc recommends `done`. André's status semantics say otherwise:
/// `review` = work done, awaiting his verification; `done` = verified by him.
/// A sandbox proving the tests pass is the first of those, not the second.
const PASSED_STATUS: &str = "review";

/// Kick off a verification for a ticket. Validates synchronously (the two
/// things that fail for user-fixable reasons — unknown repo, no plan — surface
/// in the caller's catch) then runs in the background, reporting progress via
/// `sandbox-verify-changed`.
#[tauri::command]
pub async fn sandbox_verify_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    ticket_id: String,
    project: String,
) -> Result<(), String> {
    let projects = crate::project_management::pm_project_list(state).await?;
    let repo = projects
        .iter()
        .find(|p| p.key == project)
        .map(|p| p.source_path.clone())
        .filter(|p| !p.is_empty())
        .ok_or_else(|| format!("project {project} has no local repo path set"))?;
    let repo_dir = PathBuf::from(repo);
    if !repo_dir.is_dir() {
        return Err(format!("repo path does not exist: {}", repo_dir.display()));
    }
    let (config, steps) = load_verify_plan(&repo_dir)?;
    if steps.is_empty() {
        return Err("verify plan has no steps to run".into());
    }

    let run_id = uuid::Uuid::new_v4().to_string();
    tokio::spawn(async move {
        let outcome = run_verify(
            &app, &repo_dir, &ticket_id, &project, &run_id, &config, &steps,
        )
        .await;
        // Only a green run touches the ticket. A red one is already fully
        // described by its record; appending failures to the body is noise.
        if let Ok(record) = outcome {
            if record.status == "passed" {
                if let Err(error) = mark_ticket_verified(&app, &record).await {
                    eprintln!("sandbox verify: ticket not updated: {error}");
                }
            }
        }
    });
    Ok(())
}

/// Append the proof to the ticket and move it to `PASSED_STATUS`.
///
/// The revision is re-read here rather than taken from the caller: a verify run
/// is minutes long and the panel's copy is stale by the time it finishes.
async fn mark_ticket_verified(app: &tauri::AppHandle, record: &VerifyRecord) -> Result<(), String> {
    let state = app.state::<crate::state::AppState>();
    let tickets =
        crate::project_management::pm_ticket_list(state, Some(record.project.clone())).await?;
    let ticket = tickets
        .into_iter()
        .find(|t| t.id == record.ticket_id)
        .ok_or_else(|| format!("ticket {} not found", record.ticket_id))?;

    let mut proof = format!(
        "\n\n## Sandbox verify — passed {}\n\n",
        &record.updated_at[..10.min(record.updated_at.len())]
    );
    for step in &record.steps {
        proof.push_str(&format!(
            "- `{}` — `{}` — exit {}\n",
            step.name,
            step.command,
            step.exit_code.unwrap_or(-1)
        ));
    }
    proof.push_str(&format!("\nRecord: `{}`\n", record.id));

    let state = app.state::<crate::state::AppState>();
    crate::project_management::pm_ticket_update(
        state,
        crate::project_management::TicketUpdateRequest {
            id: ticket.id.clone(),
            expected_revision: ticket.revision,
            title: None,
            ticket_type: None,
            status: Some(PASSED_STATUS.into()),
            priority: None,
            owner: None,
            clear_owner: false,
            documentation: None,
            body: Some(format!("{}{proof}", ticket.body)),
        },
    )
    .await
    .map(|_| ())
}

/// Every recorded verify run, newest first.
#[tauri::command]
pub async fn sandbox_verify_records() -> Result<Vec<VerifyRecord>, String> {
    let dir = records_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<VerifyRecord> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|body| serde_json::from_str(&body).ok())
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

// ─── Loops → GitVM bridge (XNAUT-38 Phase 3) ─────────────────────────────────

/// The port a finished verify leaves the node through. Workflow edges are wired
/// on `success` / `error` (every seeded definition in `loops.rs` names its ports
/// that way), and `schedule_downstream` matches outcomes against `from_port` by
/// string. A node completed with the record's own `passed`/`failed` vocabulary
/// would match no edge and silently schedule nothing.
fn verify_outcome(status: &str) -> &'static str {
    if status == "passed" {
        "success"
    } else {
        "error"
    }
}

/// Runs one workflow node's work in a GitVM sandbox: claim the node, resolve the
/// repo's verify plan, ship/run/record/destroy, then complete the node with the
/// record as its output.
///
/// This is the bridge that was missing. `loops.rs` is a durable state machine
/// and never executed anything; `run_verify` had no caller outside
/// `sandbox_verify_start`. Neither half needed changing.
///
/// A red verify *completes* the node down the `error` edge rather than failing
/// the run — that is what lets a workflow route to a fix-and-retry branch.
/// `loops_run_fail_node` is for the run never producing a verdict at all.
/// Iterate-until-green is not new code either: `run_steps` already retries the
/// test step `retries` times from `.xnaut/verify.json`.
///
/// ponytail: `repo_path` is an argument, not read out of the node's config. The
/// caller already knows the checkout it is verifying. Move it into
/// `node.config.repo_path` when a workflow needs to verify a repo its caller
/// cannot name.
#[tauri::command]
pub async fn loops_run_sandbox_node(
    app: tauri::AppHandle,
    run_id: String,
    node_id: String,
    repo_path: String,
) -> Result<VerifyRecord, String> {
    let repo_dir = PathBuf::from(&repo_path);
    if !repo_dir.is_dir() {
        return Err(format!("repo path does not exist: {repo_path}"));
    }
    // Resolve the plan before claiming: a repo with no verify plan is a caller
    // error, and claiming first would leave the node stuck in `running`.
    let (config, steps) = load_verify_plan(&repo_dir)?;
    if steps.is_empty() {
        return Err("verify plan has no steps to run".into());
    }

    let run = crate::loops::loops_run_claim_node(app.clone(), run_id.clone(), node_id.clone())?;
    let project = run.project.clone().unwrap_or_default();
    let ticket_id = run
        .input
        .get("ticket_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let record = match run_verify(
        &app, &repo_dir, &ticket_id, &project, &run_id, &config, &steps,
    )
    .await
    {
        Ok(record) => record,
        Err(error) => {
            let _ = crate::loops::loops_run_fail_node(
                app,
                crate::loops::FailNodeRequest {
                    run_id,
                    node_id,
                    error: error.clone(),
                },
            );
            return Err(error);
        }
    };

    crate::loops::loops_run_complete_node(
        app,
        crate::loops::CompleteNodeRequest {
            run_id,
            node_id,
            output: serde_json::to_value(&record).map_err(|e| e.to_string())?,
            outcomes: vec![verify_outcome(&record.status).into()],
            usage: None,
        },
    )?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("xnaut-verify-test-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn explicit_verify_json_wins() {
        let dir = tmpdir();
        std::fs::create_dir_all(dir.join(".xnaut")).unwrap();
        std::fs::write(
            dir.join(".xnaut/verify.json"),
            r#"{"install":"pnpm i","build":"pnpm build","test":"pnpm test","retries":2}"#,
        )
        .unwrap();
        // package.json present too — the explicit file must still win.
        std::fs::write(dir.join("package.json"), r#"{"scripts":{"test":"jest"}}"#).unwrap();
        let (cfg, steps) = load_verify_plan(&dir).unwrap();
        assert_eq!(cfg.retries, 2);
        assert_eq!(
            steps.iter().map(|s| s.command.as_str()).collect::<Vec<_>>(),
            vec!["pnpm i", "pnpm build", "pnpm test"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn node_fallback_uses_only_present_scripts() {
        let dir = tmpdir();
        // build script present, test present, no verify.json.
        std::fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"build":"next build","test":"vitest"}}"#,
        )
        .unwrap();
        let (_cfg, steps) = load_verify_plan(&dir).unwrap();
        let cmds: Vec<_> = steps.iter().map(|s| s.command.as_str()).collect();
        assert_eq!(cmds, vec!["npm ci", "npm run build", "npm test"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn node_fallback_skips_absent_scripts() {
        let dir = tmpdir();
        // no build/test scripts → only install.
        std::fs::write(dir.join("package.json"), r#"{"name":"x"}"#).unwrap();
        let (_cfg, steps) = load_verify_plan(&dir).unwrap();
        assert_eq!(
            steps.iter().map(|s| s.command.as_str()).collect::<Vec<_>>(),
            vec!["npm ci"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn neither_file_is_an_error() {
        let dir = tmpdir();
        let err = load_verify_plan(&dir).unwrap_err();
        assert!(
            err.contains(".xnaut/verify.json"),
            "err should guide the fix: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_gate_is_appended_as_the_last_step() {
        let dir = tmpdir();
        std::fs::write(dir.join("package.json"), r#"{"scripts":{"test":"vitest"}}"#).unwrap();
        // No gate yet: the plan is just what package.json declares.
        let (_, steps) = load_verify_plan(&dir).unwrap();
        assert!(
            !steps.iter().any(|s| s.name == "gate"),
            "no gate file, no gate step"
        );
        // The Validator writes it beside the stage documents, one level down.
        let flow = dir.join("NAUT-Flow");
        std::fs::create_dir_all(&flow).unwrap();
        std::fs::write(flow.join("95-Build-Gate.py"), "# checks\n").unwrap();
        let (_, steps) = load_verify_plan(&dir).unwrap();
        let last = steps.last().expect("at least one step");
        assert_eq!(last.name, "gate", "the gate runs last, after build/test");
        assert!(
            last.command.contains("NAUT-Flow/95-Build-Gate.py"),
            "gate command points at the file it found: {}",
            last.command
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn records_round_trip_newest_first() {
        // Shares the real records dir with any other run; the assertions only
        // look at the two ids this test wrote, so it does not need isolation.
        let mk = |id: &str, created: &str| VerifyRecord {
            id: id.into(),
            run_id: "r".into(),
            ticket_id: "XNAUT-19".into(),
            project: "XNAUT".into(),
            repo_path: String::new(),
            provider_kind: "gitvm-cli".into(),
            sandbox_id: String::new(),
            public_url: String::new(),
            status: "passed".into(),
            steps: vec![],
            log_dir: String::new(),
            video_path: None,
            created_at: created.into(),
            updated_at: created.into(),
        };
        let old = format!("test-old-{}", std::process::id());
        let new = format!("test-new-{}", std::process::id());
        write_verify_record(&mk(&old, "2020-01-01T00:00:00Z")).unwrap();
        write_verify_record(&mk(&new, "2030-01-01T00:00:00Z")).unwrap();

        let all = sandbox_verify_records().await.unwrap();
        let ours: Vec<&str> = all
            .iter()
            .map(|r| r.id.as_str())
            .filter(|id| *id == old || *id == new)
            .collect();
        assert_eq!(ours, vec![new.as_str(), old.as_str()], "newest first");

        let _ = std::fs::remove_file(records_dir().join(format!("{old}.json")));
        let _ = std::fs::remove_file(records_dir().join(format!("{new}.json")));
    }

    #[test]
    fn tail_of_keeps_end_and_is_char_safe() {
        assert_eq!(tail_of("short", 100), "short");
        let long: String = "é".repeat(50);
        let t = tail_of(&long, 10);
        assert!(t.starts_with('…'), "truncation marker");
        assert_eq!(
            t.chars().filter(|c| *c == 'é').count(),
            10,
            "keeps last 10 chars"
        );
    }

    /// The one branch in the bridge. `schedule_downstream` matches a node's
    /// outcomes against connection `from_port` by string, and every workflow in
    /// `loops.rs` wires its ports as success/error. Return this module's own
    /// `passed`/`failed` vocabulary instead and the run stalls with no error:
    /// no edge matches, so nothing downstream is ever scheduled.
    #[test]
    fn verify_outcome_maps_onto_workflow_ports() {
        assert_eq!(verify_outcome("passed"), "success");
        assert_eq!(verify_outcome("failed"), "error");
        // Anything that is not a green verdict routes down the error edge.
        assert_eq!(verify_outcome("cancelled"), "error");
        assert_eq!(verify_outcome("running"), "error");
    }
}
