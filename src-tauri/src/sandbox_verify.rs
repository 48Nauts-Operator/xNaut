// ABOUTME: Sandbox Verify orchestration (XNAUT-19). Resolve what to run for a
// ABOUTME: target repo (`.xnaut/verify.json`, else Node auto-detect), run it in
// ABOUTME: a fresh GitVM sandbox, record every step, and on green append the
// ABOUTME: proof to the ticket. Steps 4-7 of the Phase 1 plan.
#![allow(dead_code)] // video proof (step 9) still consumes more of this.

use crate::sandbox::cli as gvm;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::Emitter;

fn default_template() -> String {
    "pi-dev".into()
}
fn default_retries() -> u32 {
    1
}

/// Shape of `.xnaut/verify.json` (all command fields optional).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyConfig {
    /// Where the steps run: "gitvm" (default, fresh sandbox per run) or
    /// "exe-dev" (one persistent VM, warm caches). Per-repo so switching is
    /// one line in `.xnaut/verify.json` and nothing else (XNAUT-252).
    #[serde(default)]
    pub provider: Option<String>,
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
            provider: None,
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
    // An unknown provider is a typo, not a default. `"provider": "local"` was
    // written into a rig board on 2026-09-01, fell through to gitvm, and five
    // verifications failed on a machine with no gitvm installed while the
    // config looked deliberate. Silent fallback is the failure mode this whole
    // sprint exists to remove, so name the valid options and stop.
    if let Some(provider) = config.provider.as_deref() {
        if !matches!(provider.trim(), "" | "gitvm" | "exe-dev") {
            return Err(format!(
                "unknown verify provider {provider:?} in .xnaut/verify.json;                  valid providers are \"gitvm\" and \"exe-dev\""
            ));
        }
    }
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

// ─── Which tree gets verified (XNAUT-276) ───────────────────────────────────

/// Does this branch name name this ticket?
///
/// The convention is `feat/xnaut-276-<slug>` / `fix/xnaut-276-<slug>`, and
/// agents also use `agent/<who>/xnaut-276-<slug>`, so the test is containment
/// of the ticket id rather than a prefix. The boundary check is the whole
/// point: a bare `contains` makes `XNAUT-27` match `feat/xnaut-276-…`, which
/// would verify a neighbouring ticket's tree and call it proof.
fn branch_names_ticket(branch: &str, ticket_id: &str) -> bool {
    let needle = ticket_id.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return false;
    }
    let hay = branch.to_ascii_lowercase();
    let bytes = hay.as_bytes();
    let mut from = 0;
    while let Some(offset) = hay[from..].find(&needle) {
        let start = from + offset;
        let end = start + needle.len();
        let clean_start = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let clean_end = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if clean_start && clean_end {
            return true;
        }
        from = start + 1;
    }
    false
}

/// The directory a verification for `ticket_id` belongs in.
///
/// THE BUG THIS EXISTS TO KILL (XNAUT-276): this used to be the project's
/// `source_path` and nothing else, so every verification for every ticket in a
/// project ran one fixed directory. XNAUT's `source_path` pointed at
/// `.worktrees/done-nudge`, a tree unrelated to anything in flight; a green run
/// there proved that `done-nudge` compiles and then closed the ticket at
/// `complete` on the strength of it. Nineteen XNAUT tickets sat in `done` at
/// the time, and eighteen of them would have been settled that way.
///
/// So: if some worktree's branch names the ticket, that worktree IS the tree
/// under test. Otherwise fall back to `source_path`; a ticket with no worktree
/// of its own is being worked in the project checkout, which is still the
/// honest answer.
///
/// PURE over the listing on purpose. The git call belongs to the caller so the
/// decision itself can be asserted without a repo on disk.
///
/// Ties are broken by branch name so the choice is deterministic. Two worktrees
/// claiming one ticket is a workspace the owner has to sort out; picking
/// arbitrarily between them would make the record unreproducible on top of it.
pub fn resolve_verify_dir(
    ticket_id: &str,
    worktrees: &[crate::worktree::WorktreeInfo],
    source_path: &str,
) -> String {
    let mut claimed: Vec<(&str, &str)> = worktrees
        .iter()
        .filter(|w| !w.is_bare)
        .filter_map(|w| w.branch.as_deref().map(|b| (b, w.path.as_str())))
        .filter(|(branch, _)| branch_names_ticket(branch, ticket_id))
        .collect();
    claimed.sort_unstable();
    match claimed.first() {
        Some((_, path)) => (*path).to_string(),
        None => source_path.to_string(),
    }
}

/// HEAD of a checkout, or empty when it cannot be read.
///
/// Empty is a real answer here (an unpacked tarball is verifiable and has no
/// sha), and a run must not be refused for it. It is written to the record as
/// "unknown" and read as such.
fn head_sha(dir: &Path) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
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
    /// The directory that was actually verified. This is the record's answer to
    /// "what did this run prove?", so it is resolved per TICKET
    /// (`resolve_verify_dir`), not per project (XNAUT-276).
    pub repo_path: String,
    /// HEAD of `repo_path` at the moment the run started.
    ///
    /// Without it a green record names a directory whose contents have since
    /// moved on, and it can be read as evidence for work it never contained.
    /// Every record written before 2026-09-05 lacks it, so `serde(default)`
    /// leaves it empty rather than failing the load; an empty sha means
    /// "unknown", never "clean".
    #[serde(default)]
    pub commit_sha: String,
    pub provider_kind: String,
    pub sandbox_id: String,
    pub public_url: String,
    /// Why a run failed, when it failed before any step could produce an exit
    /// code. Without it the rig's five failed RIG-2 records all read
    /// `exit_code: None, log_tail: ""`, and nothing on disk said the machine
    /// simply had no gitvm on it (2026-09-01). serde(default) so records
    /// written before this field still load.
    #[serde(default)]
    pub error: String,
    /// running | passed | failed | cancelled | orphaned
    ///
    /// `orphaned` is written at startup for a run whose app died mid-flight
    /// (XNAUT-264). Without it a killed verification sat at `running`
    /// forever: the pill never resolved, the timeline showed work in
    /// progress, and the ticket could never be verified again.
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

/// Which backend runs the steps. Resolved once from the config; everything
/// downstream matches on this instead of re-reading strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runner {
    GitVm,
    ExeDev,
}

fn runner_for(config: &VerifyConfig) -> Runner {
    match config.provider.as_deref() {
        Some("exe-dev") => Runner::ExeDev,
        _ => Runner::GitVm,
    }
}

/// The record as it exists before a single step has run: what is about to be
/// verified, and where.
///
/// Split out of `run_verify` so the identifying half can be asserted without a
/// sandbox (XNAUT-276). `run_verify` needs GitVM or an exe.dev VM to reach its
/// first line of output, which is exactly why the record's provenance fields
/// went untested long enough for the wrong directory to reach production.
fn opening_record(
    repo_dir: &Path,
    ticket_id: &str,
    project: &str,
    run_id: &str,
    runner: Runner,
    steps: &[PlannedStep],
) -> VerifyRecord {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    VerifyRecord {
        id: id.clone(),
        run_id: run_id.into(),
        ticket_id: ticket_id.into(),
        project: project.into(),
        repo_path: repo_dir.to_string_lossy().into_owned(),
        // Read from `repo_dir` rather than passed in, so the sha belongs to the
        // directory this run actually opened, whichever caller chose it.
        commit_sha: head_sha(repo_dir),
        provider_kind: match runner {
            Runner::GitVm => "gitvm-cli".into(),
            Runner::ExeDev => "exe-ssh".into(),
        },
        sandbox_id: String::new(),
        public_url: String::new(),
        error: String::new(),
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
    }
}

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
///
/// `app` is optional because the engine has no business needing a window: the
/// handle is used for nothing but progress emission. Passing `None` is what
/// lets `live_exe_dev_run_goes_green` drive a real verification from `cargo
/// test`, which is how this path finally got exercised at all.
pub async fn run_verify(
    app: Option<&tauri::AppHandle>,
    repo_dir: &Path,
    ticket_id: &str,
    project: &str,
    run_id: &str,
    config: &VerifyConfig,
    steps: &[PlannedStep],
) -> Result<VerifyRecord, String> {
    let runner = runner_for(config);
    let mut record = opening_record(repo_dir, ticket_id, project, run_id, runner, steps);
    write_verify_record(&record)?;
    emit(app, &record);

    let dir = repo_dir.to_path_buf();
    let warm_project = record.project.clone();
    // Whether another verification is live in this same directory. gitvm is
    // directory-scoped, one sandbox per directory, so if nobody else is
    // running here then whatever sandbox the directory holds belongs to a run
    // that no longer exists.
    let dir_is_free = !another_run_is_live_in(&dir, &record.id);
    let warmed = tokio::task::spawn_blocking(move || match runner {
        Runner::GitVm => {
            // A reaped VM leaves .gitvm/state.json behind and warm-up refuses
            // while it exists, wedging the directory. Clear it only when the
            // control plane agrees the sandbox is really gone.
            if gvm::state_is_stale(&dir) {
                let _ = gvm::stop(&dir);
            }
            // The other way a directory wedges (2026-09-05): the app quit
            // mid-verification, the run was orphaned, and its sandbox stayed
            // RUNNING. Not stale by the check above, owned by nobody, and every
            // later verification here died at "gitvm: already warm". Four in a
            // row did, on two directories. If no other verification is live in
            // this directory, the sandbox is ours to replace.
            if dir_is_free && dir.join(".gitvm/state.json").is_file() {
                let _ = gvm::stop(&dir);
            }
            gvm::warm_up(&dir)?;
            gvm::public_url(&dir)
        }
        Runner::ExeDev => {
            let url = crate::sandbox::exe::ensure()?;
            crate::sandbox::exe::push(&dir, &warm_project)?;
            Ok(url)
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    match warmed {
        Ok(url) => {
            record.public_url = url;
            record.sandbox_id = match runner {
                Runner::GitVm => "gitvm".into(),
                Runner::ExeDev => crate::sandbox::exe::VM.into(),
            };
            let _ = write_verify_record(&record);
        }
        Err(error) => return Err(fail(app, &mut record, error)),
    }

    let result = run_steps(app, repo_dir, config, steps, runner, &mut record).await;

    // Pull BEFORE stop — teardown destroys /workspace (XNAUT-40) and the gate
    // may have written reports we want. Both are best-effort: the verdict is
    // already recorded and the sandbox self-destructs at its TTL regardless.
    // exe.dev skips both on purpose: the VM is persistent, so nothing is
    // destroyed and the warm target/ cache IS the reason it was chosen.
    if runner == Runner::GitVm {
        let dir = repo_dir.to_path_buf();
        let _ = tokio::task::spawn_blocking(move || {
            let _ = gvm::pull(&dir);
            gvm::stop(&dir)
        })
        .await;
    }

    settle(&mut record, &result);
    write_verify_record(&record)?;
    emit(app, &record);
    result.map(|_| record)
}

/// Stamp the run's terminal state onto its record.
///
/// The `Err` arm carrying `error` is the fix. A runner that dies mid-flight
/// (ssh dropped, the VM stopped under us, a workdir that could not be named)
/// leaves no exit code on any step, so without this the record reads
/// `status: "failed", error: "", exit_code: null` and says nothing whatsoever
/// about why. That is the exact shape of the rig's unreadable failures, and the
/// every-failure-carries-its-reason contract has to hold here too, not only for
/// the pre-run refusals that already had it.
fn settle(record: &mut VerifyRecord, result: &Result<bool, String>) {
    record.status = match result {
        Ok(true) => "passed",
        _ => "failed",
    }
    .into();
    if let Err(error) = result {
        record.error = error.clone();
    }
    record.updated_at = chrono::Utc::now().to_rfc3339();
}

fn emit(app: Option<&tauri::AppHandle>, record: &VerifyRecord) {
    if let Some(app) = app {
        let _ = app.emit("sandbox-verify-changed", record);
    }
}

fn fail(app: Option<&tauri::AppHandle>, record: &mut VerifyRecord, error: String) -> String {
    record.status = "failed".into();
    record.error = error.clone();
    record.updated_at = chrono::Utc::now().to_rfc3339();
    let _ = write_verify_record(record);
    emit(app, record);
    error
}

async fn run_steps(
    app: Option<&tauri::AppHandle>,
    repo_dir: &Path,
    config: &VerifyConfig,
    steps: &[PlannedStep],
    runner: Runner,
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
            let project = record.project.clone();
            let step_command = step.command.clone();
            let out = tokio::task::spawn_blocking(move || match runner {
                Runner::GitVm => {
                    gvm::run(&dir, &format!("cd /workspace && {step_command}"))
                }
                // exe::run cds into the project's own dir on the shared VM.
                Runner::ExeDev => crate::sandbox::exe::run(&project, &step_command),
            })
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

/// Where a green run leaves the ticket: the end of the review rail.
///
/// This used to be `review`, and that left the rail OPEN. Nothing anywhere in
/// the tree ever wrote `complete` on its own: the only writers are NautBot's
/// chat tool (agent_tools.rs) and the MCP tool (agent_hooks.rs), both of which
/// need a live model turn, plus the owner's own UI. So a ticket could pass a
/// real verification and still never finish.
///
/// Worse, `review` is one of the two statuses `sweep::awaits_review` treats as
/// a handback, so a green run put the ticket straight back in the queue it had
/// just come out of, to be re-verified every cooldown, forever. The terminus
/// has to be a status the sweep does not offer, and `complete` is the only one
/// that means finished.
///
/// The sweep still never marks its own homework: it is the VERIFIER that
/// writes this, and only against a record whose steps actually exited 0 on a
/// real machine.
///
/// ponytail: green IS the approval. There is no human check and no merge check
/// between a passing record and `complete`; the ticket can complete while its
/// branch is still unmerged. Gate this on `merge_gate` when a wrong auto-complete
/// actually costs something.
/// `pub(crate)` so `sweep::fleet_report` can ask the one question that makes a
/// fleet run readable: is this ticket closed, and did a green run close it?
pub(crate) const PASSED_STATUS: &str = "complete";

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
    let _ = state;
    sandbox_verify_start_inner(app, ticket_id, project).await
}

/// The command's body, callable without a `State` extractor.
///
/// The durable sweep (XNAUT-239) starts verifications from a background task
/// where no command invocation exists, so the work lives here and the command
/// is a thin wrapper.
pub async fn sandbox_verify_start_inner(
    app: tauri::AppHandle,
    ticket_id: String,
    project: String,
) -> Result<(), String> {
    // A refusal BEFORE any step runs still has to leave a record.
    //
    // Everything that can go wrong here (no repo path, a missing directory, an
    // unknown provider, an empty plan) used to return Err with no record on
    // disk. The sweep derives its failure count from records, so those failures
    // incremented nothing and three strikes was never reached: the rig measured
    // nine identical refusals in 27 minutes with no backoff at all, and again
    // four in a row on the next build (2026-09-01, round 14).
    //
    // Worse, refusing an unknown provider by name was itself a fix landed the
    // same day. Before it, a bad provider fell through to gitvm and produced a
    // record the counter could see. Two individually correct fixes made a new
    // unbounded loop between them, which is the whole argument for recording
    // the refusal rather than only returning it.
    match plan_run(&app, &ticket_id, &project).await {
        Ok(started) => Ok(started),
        Err(error) => {
            record_refusal(Some(&app), &ticket_id, &project, &error);
            Err(error)
        }
    }
}

/// Writes the record a pre-run refusal would otherwise never leave, so it is
/// countable and it says why. Best effort: a refusal that cannot be written is
/// still a refusal, and the caller still gets the error.
fn record_refusal(app: Option<&tauri::AppHandle>, ticket_id: &str, project: &str, error: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();
    let mut record = VerifyRecord {
        id: id.clone(),
        run_id: id.clone(),
        ticket_id: ticket_id.to_string(),
        project: project.to_string(),
        repo_path: String::new(),
        commit_sha: String::new(),
        provider_kind: "none".into(),
        sandbox_id: String::new(),
        public_url: String::new(),
        error: error.to_string(),
        status: "failed".into(),
        steps: Vec::new(),
        log_dir: records_dir().join(&id).to_string_lossy().into_owned(),
        video_path: None,
        created_at: now.clone(),
        updated_at: now,
    };
    let _ = write_verify_record(&record);
    emit(app, &record);
    record.status = "failed".into();
}

async fn plan_run(
    app: &tauri::AppHandle,
    ticket_id: &str,
    project: &str,
) -> Result<(), String> {
    let (app, ticket_id, project) = (app.clone(), ticket_id.to_string(), project.to_string());
    let state = tauri::Manager::state::<crate::state::AppState>(&app);
    let projects = crate::project_management::pm_project_list(state).await?;
    let source_path = projects
        .iter()
        .find(|p| p.key == project)
        .map(|p| p.source_path.clone())
        .filter(|p| !p.is_empty())
        .ok_or_else(|| format!("project {project} has no local repo path set"))?;
    // The project only says where the repo IS. What gets verified is the
    // TICKET's own tree when it has one (XNAUT-276). A listing we cannot read
    // (source_path is not a git checkout) simply yields no candidates, and the
    // fallback is the old behaviour rather than a refusal.
    let worktrees = crate::worktree::list_worktrees(Path::new(&source_path)).unwrap_or_default();
    let repo_dir = PathBuf::from(resolve_verify_dir(&ticket_id, &worktrees, &source_path));
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
            Some(&app),
            &repo_dir,
            &ticket_id,
            &project,
            &run_id,
            &config,
            &steps,
        )
        .await;
        if let Ok(record) = outcome {
            if let Err(error) = settle_ticket(&record) {
                eprintln!("sandbox verify: ticket not updated: {error}");
            }
        }
    });
    Ok(())
}

/// What a finished verification does to its ticket, against the configured
/// control repo.
fn settle_ticket(record: &VerifyRecord) -> Result<Option<()>, String> {
    let repo = crate::project_management::repo_now()?;
    settle_ticket_in(&repo, record).map(|moved| moved.map(|_| ()))
}

/// The last link of the review rail: a green run appends its proof to the
/// ticket and closes it at `PASSED_STATUS`; anything else leaves the ticket
/// exactly where it is. Returns the ticket it moved, or `None` for a run that
/// moved nothing.
///
/// The "only green moves it" rule lives HERE, inside one tested function,
/// rather than in the `tokio::spawn` closure that used to hold it. That closure
/// runs in a background task inside a Tauri command and no test could ever
/// reach it, so the half of the rail that matters most was asserted by nobody:
/// a RED run must not advance a ticket.
///
/// Takes a repo path rather than an `AppHandle` for the same reason `run_verify`
/// takes an optional one: nothing about deciding a ticket's fate needs a window.
///
/// The revision is re-read here rather than taken from the caller: a verify run
/// is minutes long and any copy from before it started is stale.
pub fn settle_ticket_in(
    repo: &Path,
    record: &VerifyRecord,
) -> Result<Option<crate::project_management::TicketRecord>, String> {
    // A red run is already fully described by its own record; appending
    // failures to the ticket body is noise, and moving it would be a lie.
    if record.status != "passed" {
        return Ok(None);
    }
    let ticket = crate::project_management::ticket_list_in(repo, Some(record.project.clone()))?
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
    // Name the tree in the ticket, not only in the record. This paragraph is
    // the thing a human reads when deciding whether a `complete` is trustworthy,
    // and until XNAUT-276 it could not say which directory produced the green.
    proof.push_str(&format!(
        "\nTree: `{}` @ `{}`\n",
        if record.repo_path.is_empty() {
            "unknown"
        } else {
            &record.repo_path
        },
        if record.commit_sha.is_empty() {
            "unknown"
        } else {
            &record.commit_sha
        },
    ));
    proof.push_str(&format!("\nRecord: `{}`\n", record.id));

    crate::project_management::ticket_update_in(
        repo,
        crate::project_management::TicketUpdateRequest {
            // Attributed to NautBot, not left unattributed. `complete` is
            // NautBot's word in this product's vocabulary, and saying so sends
            // the write THROUGH `foreign_complete_refusal` instead of around
            // it: if that gate ever stopped admitting NautBot, the rail would
            // go red here rather than quietly stop closing tickets.
            caller: Some(crate::agent_profiles::RESERVED_NAUTBOT_HANDLE.to_string()),
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
    .map(Some)
}

/// Adopt the wreckage of the last run at startup, then hand the tickets back
/// to the sweep.
///
/// A verification lives inside the app process, so quitting mid-run leaves a
/// record claiming `running` with nobody behind it — observed 2026-08-31,
/// record b55a35f0, still "running" hours later. Two things have to happen
/// on the way back up, and neither used to:
///
///   1. The lie is corrected: the record becomes `orphaned`, with the step it
///      died on preserved, so every surface stops showing phantom work.
///   2. The ticket is RETURNED, not forgotten. The ids are handed to the
///      sweep, which retries them before it looks for anything new.
///
/// A grace window keeps this honest across a fast restart: a record touched
/// in the last two minutes might belong to an app that is still running (a
/// second window, a relaunch racing the old process), so it is left alone.
pub fn reap_orphaned_runs() -> Vec<String> {
    const GRACE_SECS: i64 = 120;
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(GRACE_SECS);
    let mut tickets = Vec::new();
    let dir = records_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return tickets;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut record) = serde_json::from_str::<VerifyRecord>(&body) else {
            continue;
        };
        if record.status != "running" {
            continue;
        }
        let touched = chrono::DateTime::parse_from_rfc3339(&record.updated_at)
            .map(|at| at.with_timezone(&chrono::Utc));
        if let Ok(at) = touched {
            if at > cutoff {
                continue; // too fresh to call dead
            }
        }
        // Say which step it died on rather than blanking the run: that is the
        // one piece of evidence a crashed verification leaves behind.
        let died_on = record
            .steps
            .iter()
            .find(|step| step.exit_code.is_none())
            .map(|step| step.name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        record.status = "orphaned".into();
        record.updated_at = chrono::Utc::now().to_rfc3339();
        if let Some(step) = record.steps.iter_mut().find(|s| s.exit_code.is_none()) {
            step.log_tail = format!(
                "{}\n[xnaut] the app exited during this step; the run was orphaned and re-queued",
                step.log_tail
            );
        }
        let _ = write_verify_record(&record);
        crate::ledger::record(
            "verify_orphaned",
            "nautbot",
            &record.ticket_id,
            &format!("app exited during `{died_on}`; re-queued"),
        );
        if !record.ticket_id.trim().is_empty() && !tickets.contains(&record.ticket_id) {
            tickets.push(record.ticket_id.clone());
        }
    }
    tickets
}

/// Every recorded verify run, newest first.
/// Is some OTHER verification record `running` for this directory? Read from
/// the records on disk, the same source the sweep budgets from.
fn another_run_is_live_in(dir: &Path, own_id: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(records_dir()) else {
        return false;
    };
    let records: Vec<VerifyRecord> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|body| serde_json::from_str::<VerifyRecord>(&body).ok())
        .collect();
    another_run_is_live(&records, dir, own_id)
}

/// Pure over the listing, so the rule can be asserted without a records dir.
fn another_run_is_live(records: &[VerifyRecord], dir: &Path, own_id: &str) -> bool {
    let want = dir.to_string_lossy();
    records
        .iter()
        .any(|r| r.status == "running" && r.id != own_id && r.repo_path == want)
}

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
pub(crate) fn verify_outcome(status: &str) -> &'static str {
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
        Some(&app),
        &repo_dir,
        &ticket_id,
        &project,
        &run_id,
        &config,
        &steps,
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

    /// A directory with an ownerless running sandbox is wedged for every later
    /// verification (2026-09-05, four in a row on two directories). Replacing
    /// that sandbox is only safe when no OTHER run is live in the directory:
    /// our own record is always `running` at this point and must not count.
    #[test]
    fn a_directory_is_free_unless_another_run_is_live_in_it() {
        let dir = std::path::Path::new("/tmp/verify-me");
        let steps = vec![PlannedStep { name: "test".into(), command: "true".into() }];
        let mut mine = opening_record(dir, "XNAUT-1", "XNAUT", "run-1", Runner::GitVm, &steps);
        mine.id = "me".into();
        let mut other_here = opening_record(dir, "XNAUT-2", "XNAUT", "run-2", Runner::GitVm, &steps);
        other_here.id = "other".into();
        let mut other_elsewhere =
            opening_record(std::path::Path::new("/tmp/elsewhere"), "XNAUT-3", "XNAUT", "run-3", Runner::GitVm, &steps);
        other_elsewhere.id = "far".into();
        let mut finished_here = other_here.clone();
        finished_here.id = "done".into();
        finished_here.status = "failed".into();

        assert!(!another_run_is_live(&[mine.clone()], dir, "me"), "my own record is not another run");
        assert!(!another_run_is_live(&[mine.clone(), other_elsewhere], dir, "me"));
        assert!(!another_run_is_live(&[mine.clone(), finished_here], dir, "me"));
        assert!(another_run_is_live(&[mine, other_here], dir, "me"), "a live run here holds the sandbox");
    }
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

    // ─── Which tree gets verified (XNAUT-276) ───────────────────────────────

    fn wt(branch: &str, path: &str) -> crate::worktree::WorktreeInfo {
        crate::worktree::WorktreeInfo {
            path: path.into(),
            branch: Some(branch.into()),
            head: None,
            is_bare: false,
            is_detached: false,
            is_locked: false,
            is_prunable: false,
        }
    }

    /// A git repo with one commit, so `head_sha` has something real to read.
    fn repo_with_a_commit(name: &str) -> std::path::PathBuf {
        let dir = tmpdir().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .expect("git is on PATH");
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("f.txt"), name).unwrap();
        git(&["add", "-A"]);
        // Identity passed per-invocation: the suite must not depend on, or
        // touch, the machine's git config.
        git(&[
            "-c",
            "user.name=xnaut-test",
            "-c",
            "user.email=test@xnaut.invalid",
            "commit",
            "-qm",
            "seed",
        ]);
        dir
    }

    /// The bug, stated as a test. XNAUT's `source_path` pointed at
    /// `.worktrees/done-nudge` while XNAUT-276 was being worked in its own
    /// worktree; the resolver must pick the ticket's tree, not the project's.
    ///
    /// Return `source_path.to_string()` unconditionally from
    /// `resolve_verify_dir` and this goes red, which is the original bug
    /// exactly.
    #[test]
    fn the_tickets_own_worktree_wins_over_the_project_path() {
        let trees = vec![
            wt("feat/done-nudges-nautbot", "/repo/.worktrees/done-nudge"),
            wt("fix/xnaut-276-verify-the-ticket", "/repo/.worktrees/x276"),
            wt("feat/xnaut-264-orphan-reap", "/repo/.worktrees/safety-net"),
        ];
        assert_eq!(
            resolve_verify_dir("XNAUT-276", &trees, "/repo/.worktrees/done-nudge"),
            "/repo/.worktrees/x276",
            "a verification proves the ticket's tree, not whatever the project points at"
        );
        // `feat/` is the same convention, and so is an agent-prefixed branch.
        assert_eq!(
            resolve_verify_dir("XNAUT-264", &trees, "/repo/.worktrees/done-nudge"),
            "/repo/.worktrees/safety-net"
        );
        let agent = vec![wt("agent/claude/xnaut-232-writer-lease", "/repo/wt/232")];
        assert_eq!(
            resolve_verify_dir("XNAUT-232", &agent, "/repo"),
            "/repo/wt/232"
        );
    }

    /// No worktree names the ticket: the project checkout is still the honest
    /// answer, and the fallback must not be lost while fixing the bug.
    #[test]
    fn with_no_worktree_for_the_ticket_the_project_path_stands() {
        let trees = vec![
            wt("main", "/repo"),
            wt("feat/xnaut-264-orphan-reap", "/repo/.worktrees/safety-net"),
        ];
        assert_eq!(resolve_verify_dir("XNAUT-999", &trees, "/repo"), "/repo");
        assert_eq!(resolve_verify_dir("XNAUT-999", &[], "/repo"), "/repo");
        // A detached or branchless worktree can never name a ticket.
        let detached = vec![crate::worktree::WorktreeInfo {
            branch: None,
            is_detached: true,
            ..wt("unused", "/repo/.worktrees/detached")
        }];
        assert_eq!(resolve_verify_dir("XNAUT-276", &detached, "/repo"), "/repo");
    }

    /// The boundary check, which is the whole reason this is not a `contains`.
    /// `XNAUT-27` must not claim `feat/xnaut-276-…`, or a verification proves a
    /// neighbouring ticket's tree and closes the wrong ticket on it.
    #[test]
    fn a_ticket_id_does_not_match_a_longer_neighbour() {
        let trees = vec![wt("fix/xnaut-276-verify-the-ticket", "/repo/wt/276")];
        assert_eq!(resolve_verify_dir("XNAUT-27", &trees, "/repo"), "/repo");
        assert_eq!(resolve_verify_dir("XNAUT-2", &trees, "/repo"), "/repo");
        assert_eq!(
            resolve_verify_dir("XNAUT-276", &trees, "/repo"),
            "/repo/wt/276"
        );
        // Nor may a longer id borrow a shorter branch's name.
        let short = vec![wt("fix/xnaut-27-thing", "/repo/wt/27")];
        assert_eq!(resolve_verify_dir("XNAUT-276", &short, "/repo"), "/repo");
        // And an empty ticket id claims nothing at all.
        assert_eq!(resolve_verify_dir("", &trees, "/repo"), "/repo");
    }

    /// Two worktrees claiming one ticket is a mess the owner has to sort out,
    /// but the record still has to be reproducible: same listing, same answer.
    #[test]
    fn a_contested_ticket_resolves_deterministically() {
        let a = wt("feat/xnaut-276-verify", "/repo/wt/a");
        let b = wt("fix/xnaut-276-verify", "/repo/wt/b");
        let forward = resolve_verify_dir("XNAUT-276", &[a.clone(), b.clone()], "/repo");
        let reversed = resolve_verify_dir("XNAUT-276", &[b, a], "/repo");
        assert_eq!(
            forward, reversed,
            "listing order must not change the answer"
        );
    }

    /// The record has to name the commit it verified, and it has to be the
    /// commit of the tree that was chosen, not the project's.
    ///
    /// `run_verify` builds its record with exactly this call, so this covers
    /// the wiring and not just the helper. Point `opening_record`'s
    /// `commit_sha` at anything other than `repo_dir` and this goes red.
    #[test]
    fn the_recorded_sha_is_the_sha_of_the_tree_actually_verified() {
        let project = repo_with_a_commit("project-checkout");
        let ticket_tree = repo_with_a_commit("ticket-worktree");
        let project_sha = head_sha(&project);
        let ticket_sha = head_sha(&ticket_tree);
        assert_eq!(ticket_sha.len(), 40, "a real sha: {ticket_sha}");
        assert_ne!(project_sha, ticket_sha, "the two trees differ");

        let trees = vec![wt(
            "fix/xnaut-276-verify-the-ticket",
            &ticket_tree.to_string_lossy(),
        )];
        let chosen = resolve_verify_dir("XNAUT-276", &trees, &project.to_string_lossy());
        let record = opening_record(
            Path::new(&chosen),
            "XNAUT-276",
            "XNAUT",
            "run",
            Runner::GitVm,
            &[],
        );

        assert_eq!(record.repo_path, ticket_tree.to_string_lossy());
        assert_eq!(
            record.commit_sha, ticket_sha,
            "the record names the commit under test"
        );
        assert_ne!(
            record.commit_sha, project_sha,
            "and never the project's, which is the misreading this field exists to stop"
        );

        // A directory that is not a checkout is still verifiable; the sha is
        // simply unknown, and the run must not be refused for it.
        let bare = tmpdir();
        assert_eq!(
            opening_record(&bare, "X-1", "X", "run", Runner::GitVm, &[]).commit_sha,
            ""
        );

        let _ = std::fs::remove_dir_all(project.parent().unwrap());
        let _ = std::fs::remove_dir_all(ticket_tree.parent().unwrap());
        let _ = std::fs::remove_dir_all(&bare);
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
            commit_sha: String::new(),
            provider_kind: "gitvm-cli".into(),
            sandbox_id: String::new(),
            public_url: String::new(),
            error: String::new(),
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

    fn blank_record() -> VerifyRecord {
        VerifyRecord {
            id: "r".into(),
            run_id: "r".into(),
            ticket_id: "RIG-1".into(),
            project: "RIG".into(),
            repo_path: String::new(),
            commit_sha: String::new(),
            provider_kind: "exe-ssh".into(),
            sandbox_id: String::new(),
            public_url: String::new(),
            error: String::new(),
            status: "running".into(),
            steps: vec![],
            log_dir: String::new(),
            video_path: None,
            created_at: "2026-09-03T00:00:00Z".into(),
            updated_at: "2026-09-03T00:00:00Z".into(),
        }
    }

    /// A runner that dies before any step can report an exit code must still
    /// leave its reason on the record. Drop the `error` assignment in `settle`
    /// and the record is `failed` with an empty `error` and no step data: a
    /// failure nobody can read, which is what 158 rig records looked like.
    #[test]
    fn a_runner_that_dies_mid_run_writes_why_onto_the_record() {
        let mut record = blank_record();
        settle(&mut record, &Err("ssh: connect to host … timed out".into()));
        assert_eq!(record.status, "failed");
        assert!(
            record.error.contains("timed out"),
            "the reason has to survive onto the record, got {:?}",
            record.error
        );
    }

    #[test]
    fn settle_maps_the_two_step_verdicts() {
        let mut green = blank_record();
        settle(&mut green, &Ok(true));
        assert_eq!(green.status, "passed");
        // A red STEP is already fully described by its exit code and log tail,
        // so `error` stays empty and only the status carries the verdict.
        let mut red = blank_record();
        settle(&mut red, &Ok(false));
        assert_eq!(red.status, "failed");
        assert_eq!(red.error, "");
    }

    /// The whole rail against the real exe.dev VM: warm a stopped VM, rsync a
    /// repo whose test makes a genuine assertion, run it, and come back green.
    ///
    /// Ignored because it needs the owner's registered exe.dev ssh key and a
    /// network. Run it with:
    ///   cargo test --bin xnaut live_exe_dev_run_goes_green -- --ignored --nocapture
    ///
    /// The second half is the part that makes the first half mean anything: the
    /// same repo with the assertion broken must come back `failed` with the
    /// failing exit code recorded. A green run that cannot go red proves only
    /// that something ran.
    #[tokio::test]
    #[ignore]
    async fn live_exe_dev_run_goes_green() {
        let dir = tmpdir();
        std::fs::create_dir_all(dir.join(".xnaut")).unwrap();
        std::fs::write(
            dir.join(".xnaut/verify.json"),
            r#"{"provider":"exe-dev","install":"chmod +x sum.sh test.sh","test":"sh ./test.sh"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("sum.sh"), "#!/bin/sh\necho $(( $1 + $2 ))\n").unwrap();
        let test_sh = |expected: &str| {
            format!(
                "#!/bin/sh\nset -e\ngot=$(sh ./sum.sh 2 3)\nif [ \"$got\" != \"{expected}\" ]; then\n  echo \"FAIL: sum.sh 2 3 = $got, want {expected}\"\n  exit 1\nfi\necho \"PASS: sum.sh 2 3 = 5\"\n"
            )
        };
        std::fs::write(dir.join("test.sh"), test_sh("5")).unwrap();

        let (config, steps) = load_verify_plan(&dir).unwrap();
        assert_eq!(runner_for(&config), Runner::ExeDev, "provider is honoured");

        let green = run_verify(None, &dir, "RIG-EXE", "rigexe", "live", &config, &steps)
            .await
            .expect("the run produced a verdict");
        println!("{}", serde_json::to_string_pretty(&green).unwrap());
        assert_eq!(green.status, "passed", "error={:?}", green.error);
        assert_eq!(green.provider_kind, "exe-ssh");
        assert!(
            green.steps.iter().all(|s| s.exit_code == Some(0)),
            "every step exited 0: {:?}",
            green.steps
        );

        // Break the assertion; the same rail must go red on the same VM.
        std::fs::write(dir.join("test.sh"), test_sh("6")).unwrap();
        let red = run_verify(None, &dir, "RIG-EXE", "rigexe", "live", &config, &steps)
            .await
            .expect("the run produced a verdict");
        println!("{}", serde_json::to_string_pretty(&red).unwrap());
        assert_eq!(red.status, "failed", "a broken assertion must be red");
        let test_step = red.steps.iter().find(|s| s.name == "test").unwrap();
        assert_eq!(test_step.exit_code, Some(1), "the real exit code, recorded");
        assert!(
            test_step.log_tail.contains("FAIL: sum.sh 2 3 = 5, want 6"),
            "the assertion's own message is the evidence: {}",
            test_step.log_tail
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── The review rail, end to end ────────────────────────────────────────
    //
    // A scratch control repo, built the way the real one is (projects/<KEY>/
    // tickets + events + git), because every write below goes through
    // `record_mutation` and commits. NEVER ~/.xnaut-control: that is the
    // owner's real board.

    fn scratch_board(project: &str, ticket: &str, status: &str) -> std::path::PathBuf {
        let root = tmpdir().join("board");
        std::fs::create_dir_all(root.join(format!("projects/{project}/tickets"))).unwrap();
        std::fs::create_dir_all(root.join("events")).unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            std::process::Command::new("git")
                .args(&args)
                .current_dir(&root)
                .output()
                .unwrap();
        }
        let body = serde_json::json!({
            "id": ticket, "project": project, "title": ticket, "type": "task",
            "status": status, "priority": "medium", "owner": "nautbot",
            "documentation": [], "body": "the work", "source_id": "",
            "revision": 1, "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
        });
        std::fs::write(
            root.join(format!("projects/{project}/tickets/{ticket}.json")),
            serde_json::to_string_pretty(&body).unwrap(),
        )
        .unwrap();
        root
    }

    fn on_board(
        repo: &Path,
        project: &str,
        ticket: &str,
    ) -> crate::project_management::TicketRecord {
        crate::project_management::ticket_list_in(repo, Some(project.into()))
            .unwrap()
            .into_iter()
            .find(|t| t.id == ticket)
            .expect("the ticket is on the board")
    }

    fn verdict(project: &str, ticket: &str, status: &str, exit: i32) -> VerifyRecord {
        VerifyRecord {
            steps: vec![VerifyStep {
                name: "test".into(),
                command: "sh ./test.sh".into(),
                exit_code: Some(exit),
                log_tail: "PASS: sum.sh 2 3 = 5".into(),
            }],
            ticket_id: ticket.into(),
            project: project.into(),
            status: status.into(),
            ..blank_record()
        }
    }

    /// The half that matters most: a RED verification must leave the ticket
    /// exactly where the agent put it.
    ///
    /// This rule used to live in a `tokio::spawn` closure inside a Tauri
    /// command (`plan_run`), which no test could reach, so nothing anywhere
    /// asserted it. Delete the `record.status != "passed"` guard in
    /// `settle_ticket_in` and this goes red: a failing run would close the
    /// ticket it just failed.
    #[test]
    fn a_red_verification_does_not_move_the_ticket() {
        let repo = scratch_board("RAIL", "RAIL-1", "done");
        let before = on_board(&repo, "RAIL", "RAIL-1");

        let moved = settle_ticket_in(&repo, &verdict("RAIL", "RAIL-1", "failed", 1)).unwrap();
        assert!(moved.is_none(), "a red run moves nothing");

        let after = on_board(&repo, "RAIL", "RAIL-1");
        assert_eq!(after.status, "done", "the agent's word stands");
        assert_eq!(after.revision, before.revision, "not even a write happened");
        assert_eq!(after.body, before.body, "no proof is appended to a failure");

        // Nor does any other non-green verdict. `orphaned` is the one that
        // would hurt: a run whose app died says nothing about the work.
        for status in ["running", "cancelled", "orphaned"] {
            assert!(
                settle_ticket_in(&repo, &verdict("RAIL", "RAIL-1", status, 0))
                    .unwrap()
                    .is_none()
            );
            assert_eq!(on_board(&repo, "RAIL", "RAIL-1").status, "done", "{status}");
        }
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    /// The green half: a passing record closes the ticket at `complete`,
    /// written by the verifier and carrying its evidence.
    #[test]
    fn a_green_verification_closes_the_ticket_at_complete() {
        let repo = scratch_board("RAIL", "RAIL-1", "done");
        let green = verdict("RAIL", "RAIL-1", "passed", 0);

        let moved = settle_ticket_in(&repo, &green)
            .unwrap()
            .expect("a green run moves the ticket");
        assert_eq!(moved.status, "complete");

        // Read it back off the board rather than trusting the return value:
        // the write is what the next sweep and the owner's panel both read.
        let after = on_board(&repo, "RAIL", "RAIL-1");
        assert_eq!(after.status, "complete", "the rail reaches its terminus");
        assert!(
            after.body.contains(&format!("Record: `{}`", green.id)),
            "the ticket carries the evidence that closed it: {}",
            after.body
        );
        assert!(
            after.body.contains("`test` — `sh ./test.sh` — exit 0"),
            "and the step that produced it: {}",
            after.body
        );
        assert!(
            after.body.starts_with("the work"),
            "the body is appended to, never overwritten"
        );
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    /// The proof appended to the ticket names the tree that produced it.
    ///
    /// The record is where a machine looks; this paragraph is where a human
    /// looks when deciding whether to trust a `complete`. Drop the `Tree:` line
    /// from `settle_ticket_in` and the ticket once again says only that
    /// something, somewhere, went green.
    #[test]
    fn the_proof_on_the_ticket_names_the_tree_and_the_commit() {
        let repo = scratch_board("RAIL", "RAIL-1", "done");
        let green = VerifyRecord {
            repo_path: "/repo/.worktrees/x276".into(),
            commit_sha: "abc123def456".into(),
            ..verdict("RAIL", "RAIL-1", "passed", 0)
        };
        settle_ticket_in(&repo, &green)
            .unwrap()
            .expect("green moves it");

        let body = on_board(&repo, "RAIL", "RAIL-1").body;
        assert!(
            body.contains("/repo/.worktrees/x276"),
            "the ticket says which directory was verified: {body}"
        );
        assert!(body.contains("abc123def456"), "and at which commit: {body}");
        let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    }

    /// The terminus has to be a status the sweep will not offer again.
    ///
    /// This is the link that made the rail a LOOP rather than a rail. The old
    /// terminus was `review`, and `sweep::awaits_review` counts `review` as a
    /// handback, so every green run put the ticket straight back in the queue
    /// it had just come out of, to be re-verified every cooldown forever. Point
    /// `PASSED_STATUS` back at `review` or `done` and this turns red.
    #[test]
    fn the_terminus_is_not_a_status_the_sweep_offers_again() {
        assert!(
            !crate::sweep::awaits_review(PASSED_STATUS),
            "a green run must not feed the ticket back to the sweep, got {PASSED_STATUS:?}"
        );
        // And it must be a real status, or every write of it is refused.
        assert!(crate::project_management::TICKET_STATUSES.contains(&PASSED_STATUS));
    }

    /// `complete` is NautBot's word and the write is attributed to NautBot, so
    /// it passes THROUGH the gate rather than around it. Attribute it to
    /// anything else and `foreign_complete_refusal` refuses: the rail would
    /// stop closing tickets with only an eprintln to say so.
    #[test]
    fn the_verifier_speaks_as_nautbot_and_the_gate_admits_it() {
        assert!(
            crate::project_management::foreign_complete_refusal(
                Some(crate::agent_profiles::RESERVED_NAUTBOT_HANDLE),
                Some(PASSED_STATUS),
            )
            .is_none(),
            "the gate must admit the caller the verifier actually uses"
        );
        assert!(
            crate::project_management::foreign_complete_refusal(
                Some("claude"),
                Some(PASSED_STATUS)
            )
            .is_some(),
            "and must still refuse everyone else"
        );
    }

    /// The whole rail against the real exe.dev VM, from an agent's `done` to
    /// NautBot's `complete`, on the strength of a verification that really ran.
    ///
    /// Ignored because it needs the owner's registered exe.dev ssh key and a
    /// network. Run it with:
    ///   cargo test --bin xnaut live_rail_closes_from_done_to_complete -- --ignored --nocapture
    ///
    /// The second half is what makes the first mean anything: the same repo
    /// with its assertion broken must come back red and leave the ticket in
    /// `done`. A rail that cannot refuse proves only that something ran.
    #[tokio::test]
    #[ignore]
    async fn live_rail_closes_from_done_to_complete() {
        let dir = tmpdir();
        std::fs::create_dir_all(dir.join(".xnaut")).unwrap();
        std::fs::write(
            dir.join(".xnaut/verify.json"),
            r#"{"provider":"exe-dev","install":"chmod +x sum.sh test.sh","test":"sh ./test.sh"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("sum.sh"), "#!/bin/sh\necho $(( $1 + $2 ))\n").unwrap();
        let test_sh = |expected: &str| {
            format!(
                "#!/bin/sh\nset -e\ngot=$(sh ./sum.sh 2 3)\nif [ \"$got\" != \"{expected}\" ]; then\n  echo \"FAIL: sum.sh 2 3 = $got, want {expected}\"\n  exit 1\nfi\necho \"PASS: sum.sh 2 3 = 5\"\n"
            )
        };
        std::fs::write(dir.join("test.sh"), test_sh("5")).unwrap();
        let (config, steps) = load_verify_plan(&dir).unwrap();

        // An agent finished RAIL-1 and handed it back. Nobody has looked.
        let board = scratch_board("RAIL", "RAIL-1", "done");

        let green = run_verify(None, &dir, "RAIL-1", "RAIL", "live", &config, &steps)
            .await
            .expect("the run produced a verdict");
        println!(
            "green record: {}",
            serde_json::to_string_pretty(&green).unwrap()
        );
        assert_eq!(green.status, "passed", "error={:?}", green.error);
        assert!(green.steps.iter().all(|s| s.exit_code == Some(0)));

        settle_ticket_in(&board, &green)
            .unwrap()
            .expect("a green run closes the ticket");
        let closed = on_board(&board, "RAIL", "RAIL-1");
        assert_eq!(
            closed.status, "complete",
            "the rail closed on a real green run"
        );
        assert!(closed.body.contains(&format!("Record: `{}`", green.id)));

        // Now the refusal, on a second ticket so the first one's verdict is
        // not what is being re-read: break the assertion, same VM, same rail.
        let red_board = scratch_board("RAIL", "RAIL-2", "done");
        std::fs::write(dir.join("test.sh"), test_sh("6")).unwrap();
        let red = run_verify(None, &dir, "RAIL-2", "RAIL", "live", &config, &steps)
            .await
            .expect("the run produced a verdict");
        println!(
            "red record: {}",
            serde_json::to_string_pretty(&red).unwrap()
        );
        assert_eq!(red.status, "failed", "a broken assertion must be red");
        assert!(settle_ticket_in(&red_board, &red).unwrap().is_none());
        assert_eq!(
            on_board(&red_board, "RAIL", "RAIL-2").status,
            "done",
            "a red run leaves the ticket where the agent put it"
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(board.parent().unwrap());
        let _ = std::fs::remove_dir_all(red_board.parent().unwrap());
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
