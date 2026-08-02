// ABOUTME: Sandbox Verify orchestration (XNAUT-19). Step 4: resolve what to run
// ABOUTME: for a target repo — an explicit `.xnaut/verify.json`, else Node
// ABOUTME: auto-detect from package.json. Pure planning logic; the run loop
// ABOUTME: (workflow + GitVM exec) lands in step 5.
#![allow(dead_code)] // run loop + commands consume this in later Phase-1 steps.

use crate::sandbox::cli as gvm;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
            if dir.chars().all(|c| c.is_alphanumeric() || "-_.".contains(c)) {
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
        Err(error) => return Err(fail(&mut record, error)),
    }

    let result = run_steps(repo_dir, config, steps, &mut record).await;

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
    result.map(|_| record)
}

fn fail(record: &mut VerifyRecord, error: String) -> String {
    record.status = "failed".into();
    record.updated_at = chrono::Utc::now().to_rfc3339();
    let _ = write_verify_record(record);
    error
}

async fn run_steps(
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
        let _ = write_verify_record(record);
        if code != 0 {
            all_ok = false;
            break; // stop at the first red step
        }
    }
    Ok(all_ok)
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

}
