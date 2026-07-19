// ABOUTME: Sandbox Verify orchestration (XNAUT-19). Step 4: resolve what to run
// ABOUTME: for a target repo — an explicit `.xnaut/verify.json`, else Node
// ABOUTME: auto-detect from package.json. Pure planning logic; the run loop
// ABOUTME: (workflow + GitVM exec) lands in step 5.
#![allow(dead_code)] // run loop + commands consume this in later Phase-1 steps.

use crate::sandbox::{SandboxDriver, SandboxSpec};
use base64::{engine::general_purpose::STANDARD, Engine};
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
    Ok((config.clone(), config_to_steps(&config)))
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

/// tar+gzip the repo dir to bytes, excluding heavy/secret paths. Shells to system tar.
// ponytail: ships the whole tarball inline in one /exec (base64). Fine for small
// repos (build-port-pulse ships ~155KB this way); if a repo is large enough to
// blow the exec/ARG_MAX limit, switch to a chunked upload — not before.
fn tar_repo(repo_dir: &Path) -> Result<Vec<u8>, String> {
    let output = std::process::Command::new("tar")
        .arg("-czf")
        .arg("-")
        .args([
            "--exclude=./node_modules",
            "--exclude=./.next",
            "--exclude=./.git",
            "--exclude=./target",
            "--exclude=./.env",
            "--exclude=./.env.*",
        ])
        .arg("-C")
        .arg(repo_dir)
        .arg(".")
        .output()
        .map_err(|e| format!("tar: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "tar failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

const READY_TIMEOUT_SECS: u64 = 90;
const LOG_TAIL_CHARS: usize = 4000;

/// Run a verification: create sandbox → ship repo → run steps → record → destroy.
/// Returns the final record (status passed/failed). Never leaves a sandbox running.
#[allow(clippy::too_many_arguments)]
pub async fn run_verify(
    driver: &SandboxDriver,
    provider_kind: &str,
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
        provider_kind: provider_kind.into(),
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

    // Tar the repo before provisioning so a bad repo fails fast (no orphan sandbox).
    let tarball = match tar_repo(repo_dir) {
        Ok(bytes) => bytes,
        Err(error) => return Err(fail(&mut record, error)),
    };

    let spec = SandboxSpec {
        template: config.template.clone(),
        ..SandboxSpec::default()
    };
    let handle = match driver.create(&spec).await {
        Ok(handle) => handle,
        Err(error) => return Err(fail(&mut record, error)),
    };
    record.sandbox_id = handle.id.clone();
    record.public_url = handle.public_url.clone();
    let _ = write_verify_record(&record);

    let result = run_steps(driver, &handle.id, &tarball, config, steps, &mut record).await;

    // Teardown is best-effort — the sandbox also auto-destroys at its timeout.
    let _ = driver.destroy(&handle.id).await;

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
    driver: &SandboxDriver,
    sandbox_id: &str,
    tarball: &[u8],
    config: &VerifyConfig,
    steps: &[PlannedStep],
    record: &mut VerifyRecord,
) -> Result<bool, String> {
    driver.wait_ready(sandbox_id, READY_TIMEOUT_SECS).await?;

    // Ship: base64 the tarball, decode + extract into /app on the sandbox.
    let b64 = STANDARD.encode(tarball);
    let ship = driver
        .exec(
            sandbox_id,
            &format!(
                "mkdir -p /app && printf '%s' '{b64}' | base64 -d > /tmp/app.tgz && tar xzf /tmp/app.tgz -C /app && echo shipped"
            ),
        )
        .await?;
    if ship.exit_code != 0 {
        return Err(format!(
            "ship failed (exit {}): {}",
            ship.exit_code, ship.stderr
        ));
    }

    let mut all_ok = true;
    for (index, step) in steps.iter().enumerate() {
        // Only the test step retries (flake tolerance); install/build run once.
        let attempts = if step.name == "test" {
            config.retries.max(1)
        } else {
            1
        };
        let mut result = None;
        for _ in 0..attempts {
            let exec = driver
                .exec(sandbox_id, &format!("cd /app && {}", step.command))
                .await?;
            let ok = exec.exit_code == 0;
            result = Some(exec);
            if ok {
                break;
            }
        }
        let exec = result.expect("attempts >= 1");
        record.steps[index].exit_code = Some(exec.exit_code);
        record.steps[index].log_tail =
            tail_of(&format!("{}{}", exec.stdout, exec.stderr), LOG_TAIL_CHARS);
        let _ = write_verify_record(record);
        if exec.exit_code != 0 {
            all_ok = false;
            break; // stop at first red step
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

    #[test]
    fn tar_repo_produces_gzip_and_excludes_node_modules() {
        let src = tmpdir();
        std::fs::write(src.join("index.js"), "console.log(1)").unwrap();
        std::fs::create_dir_all(src.join("node_modules/x")).unwrap();
        std::fs::write(src.join("node_modules/x/big.js"), "x".repeat(1000)).unwrap();
        let bytes = tar_repo(&src).unwrap();
        assert!(
            bytes.len() > 2 && bytes[0..2] == [0x1f, 0x8b],
            "gzip magic bytes"
        );
        let out = tmpdir();
        let tgz = out.join("a.tgz");
        std::fs::write(&tgz, &bytes).unwrap();
        let status = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&tgz)
            .arg("-C")
            .arg(&out)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(out.join("index.js").exists(), "keeps source");
        assert!(!out.join("node_modules").exists(), "excludes node_modules");
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&out);
    }
}
