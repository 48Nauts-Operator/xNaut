// ABOUTME: Ralph Ultra integration commands for AI agent orchestration.
// ABOUTME: Provides PRD management, CLI detection, AC test execution, config persistence, and temp file management.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Command;

// ==================== PRD Management ====================

/// Reads and parses prd.json from a project directory
#[tauri::command]
pub async fn ralph_read_prd(project_path: String) -> Result<serde_json::Value, String> {
    let prd_path = Path::new(&project_path).join("prd.json");

    let content = tokio::fs::read_to_string(&prd_path)
        .await
        .map_err(|e| format!("Failed to read prd.json: {}", e))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse prd.json: {}", e))
}

/// Atomically writes prd.json to a project directory (temp file → rename)
#[tauri::command]
pub async fn ralph_write_prd(
    project_path: String,
    prd: serde_json::Value,
) -> Result<(), String> {
    let prd_path = Path::new(&project_path).join("prd.json");
    let tmp_path = Path::new(&project_path).join(".prd.json.tmp");

    let content = serde_json::to_string_pretty(&prd)
        .map_err(|e| format!("Failed to serialize PRD: {}", e))?;

    // Write to temp file first
    tokio::fs::write(&tmp_path, &content)
        .await
        .map_err(|e| format!("Failed to write temp file: {}", e))?;

    // Atomic rename
    tokio::fs::rename(&tmp_path, &prd_path)
        .await
        .map_err(|e| {
            // Clean up temp file on rename failure
            let _ = std::fs::remove_file(&tmp_path);
            format!("Failed to rename temp file: {}", e)
        })
}

/// Creates a timestamped backup of prd.json, pruning to max 20 backups
#[tauri::command]
pub async fn ralph_backup_prd(project_path: String) -> Result<String, String> {
    let prd_path = Path::new(&project_path).join("prd.json");
    let backup_dir = Path::new(&project_path).join(".ralph-backups");

    // Ensure backup directory exists
    tokio::fs::create_dir_all(&backup_dir)
        .await
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    // Generate timestamped filename
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_filename = format!("prd_{}.json", timestamp);
    let backup_path = backup_dir.join(&backup_filename);

    // Copy prd.json to backup
    tokio::fs::copy(&prd_path, &backup_path)
        .await
        .map_err(|e| format!("Failed to create backup: {}", e))?;

    // Prune old backups (keep newest 20)
    prune_backups(&backup_dir, 20).await;

    Ok(backup_path.to_string_lossy().to_string())
}

/// Lists all PRD backups with timestamps
#[tauri::command]
pub async fn ralph_list_backups(
    project_path: String,
) -> Result<Vec<serde_json::Value>, String> {
    let backup_dir = Path::new(&project_path).join(".ralph-backups");

    if !backup_dir.exists() {
        return Ok(vec![]);
    }

    let mut entries = tokio::fs::read_dir(&backup_dir)
        .await
        .map_err(|e| format!("Failed to read backup directory: {}", e))?;

    let mut backups = Vec::new();

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read entry: {}", e))?
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| format!("Failed to read metadata: {}", e))?;

            let modified = metadata
                .modified()
                .unwrap_or(UNIX_EPOCH)
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            backups.push(serde_json::json!({
                "path": path.to_string_lossy(),
                "filename": path.file_name().unwrap_or_default().to_string_lossy(),
                "modified": modified,
                "size": metadata.len(),
            }));
        }
    }

    // Sort by modified time, newest first
    backups.sort_by(|a, b| {
        let a_mod = a["modified"].as_u64().unwrap_or(0);
        let b_mod = b["modified"].as_u64().unwrap_or(0);
        b_mod.cmp(&a_mod)
    });

    Ok(backups)
}

/// Restores a PRD backup over the current prd.json
#[tauri::command]
pub async fn ralph_restore_backup(
    project_path: String,
    backup_path: String,
) -> Result<(), String> {
    let prd_path = Path::new(&project_path).join("prd.json");
    let backup = Path::new(&backup_path);

    if !backup.exists() {
        return Err("Backup file does not exist".to_string());
    }

    // Validate the backup is valid JSON before restoring
    let content = tokio::fs::read_to_string(backup)
        .await
        .map_err(|e| format!("Failed to read backup: {}", e))?;

    let _: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Backup contains invalid JSON: {}", e))?;

    // Atomic write via temp file
    let tmp_path = Path::new(&project_path).join(".prd.json.tmp");
    tokio::fs::write(&tmp_path, &content)
        .await
        .map_err(|e| format!("Failed to write temp file: {}", e))?;

    tokio::fs::rename(&tmp_path, &prd_path)
        .await
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            format!("Failed to restore backup: {}", e)
        })
}

/// Prune backup directory to keep only the newest `max` entries
async fn prune_backups(backup_dir: &Path, max: usize) {
    let Ok(mut entries) = tokio::fs::read_dir(backup_dir).await else {
        return;
    };

    let mut files: Vec<(PathBuf, u64)> = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let modified = entry
                .metadata()
                .await
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            files.push((path, modified));
        }
    }

    if files.len() <= max {
        return;
    }

    // Sort oldest first
    files.sort_by_key(|(_, modified)| *modified);

    // Remove oldest entries beyond max
    let to_remove = files.len() - max;
    for (path, _) in files.into_iter().take(to_remove) {
        let _ = tokio::fs::remove_file(path).await;
    }
}

// ==================== CLI Detection ====================

/// Known AI CLIs to detect
const KNOWN_CLIS: &[&str] = &["claude", "opencode", "codex", "gemini", "aider", "cody"];

/// Detects installed AI CLIs and their versions
#[tauri::command]
pub async fn ralph_detect_clis() -> Result<Vec<serde_json::Value>, String> {
    let mut results = Vec::new();

    for cli_name in KNOWN_CLIS {
        let info = detect_single_cli(cli_name).await;
        results.push(info);
    }

    Ok(results)
}

/// Checks if a specific CLI is healthy (responds to --version within 3s)
#[tauri::command]
pub async fn ralph_check_cli_health(cli: String) -> Result<bool, String> {
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        Command::new(&cli).arg("--version").output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => Ok(output.status.success()),
        _ => Ok(false),
    }
}

/// Detect a single CLI: find path via `which`, get version via `--version`
async fn detect_single_cli(name: &str) -> serde_json::Value {
    // Find path via `which`
    let which_result = Command::new("which").arg(name).output().await;

    let path = match which_result {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            return serde_json::json!({
                "name": name,
                "path": null,
                "version": null,
                "healthy": false,
            });
        }
    };

    // Get version with 3s timeout
    let version_result = tokio::time::timeout(
        Duration::from_secs(3),
        Command::new(name).arg("--version").output(),
    )
    .await;

    let (version, healthy) = match version_result {
        Ok(Ok(output)) if output.status.success() => {
            let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Extract version-like pattern from output
            let version = extract_version(&raw).unwrap_or(raw);
            (Some(version), true)
        }
        Ok(Ok(output)) => {
            // Command ran but returned non-zero; try stderr
            let raw = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let version = extract_version(&raw);
            (version, false)
        }
        _ => (None, false),
    };

    serde_json::json!({
        "name": name,
        "path": path,
        "version": version,
        "healthy": healthy,
    })
}

/// Extract a semver-like version string from CLI output
fn extract_version(text: &str) -> Option<String> {
    // Match patterns like "1.2.3", "v1.2.3", "1.2.3-beta"
    let re = regex::Regex::new(r"v?(\d+\.\d+(?:\.\d+)?(?:-[\w.]+)?)").ok()?;
    re.captures(text)
        .and_then(|c| c.get(0))
        .map(|m| m.as_str().to_string())
}

// ==================== AC Test Execution ====================

/// Result of running an acceptance criteria test
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcTestResult {
    pub passes: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Runs a shell command as an AC test and captures the result
#[tauri::command]
pub async fn ralph_run_ac_test(
    command: String,
    cwd: String,
    timeout_ms: u64,
) -> Result<AcTestResult, String> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    let result = tokio::time::timeout(
        timeout,
        Command::new("sh")
            .args(["-c", &command])
            .current_dir(&cwd)
            .output(),
    )
    .await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            Ok(AcTestResult {
                passes: exit_code == 0,
                exit_code,
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                duration_ms,
            })
        }
        Ok(Err(e)) => Err(format!("Failed to execute command: {}", e)),
        Err(_) => Ok(AcTestResult {
            passes: false,
            exit_code: -1,
            stdout: String::new(),
            stderr: format!("Command timed out after {}ms", timeout_ms),
            duration_ms,
        }),
    }
}

// ==================== Config Persistence ====================

/// Returns the Ralph Ultra config directory (~/.config/ralph-ultra/)
fn config_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Failed to get home directory")?;
    Ok(home.join(".config").join("ralph-ultra"))
}

/// Reads a JSON config file from ~/.config/ralph-ultra/
#[tauri::command]
pub async fn ralph_read_config(filename: String) -> Result<serde_json::Value, String> {
    let dir = config_dir()?;
    let path = dir.join(&filename);

    if !path.exists() {
        return Ok(serde_json::json!({}));
    }

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read config: {}", e))?;

    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse config: {}", e))
}

/// Writes a JSON config file to ~/.config/ralph-ultra/
#[tauri::command]
pub async fn ralph_write_config(
    filename: String,
    data: serde_json::Value,
) -> Result<(), String> {
    let dir = config_dir()?;

    // Ensure config directory exists
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    let path = dir.join(&filename);
    let content = serde_json::to_string_pretty(&data)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Failed to write config: {}", e))
}

// ==================== Temp File Management ====================

/// Writes content to a temporary file with a given prefix
#[tauri::command]
pub async fn ralph_write_temp_file(
    content: String,
    prefix: String,
) -> Result<String, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let filename = format!("{}-{}.txt", prefix, timestamp);
    let path = std::env::temp_dir().join(filename);

    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Failed to write temp file: {}", e))?;

    Ok(path.to_string_lossy().to_string())
}

/// Deletes a temporary file
#[tauri::command]
pub async fn ralph_cleanup_temp_file(path: String) -> Result<(), String> {
    let path = Path::new(&path);

    // Safety: only delete files in the system temp directory
    let temp_dir = std::env::temp_dir();
    if !path.starts_with(&temp_dir) {
        return Err("Can only delete files in the system temp directory".to_string());
    }

    if path.exists() {
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| format!("Failed to delete temp file: {}", e))?;
    }

    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version() {
        assert_eq!(
            extract_version("claude 1.2.3"),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            extract_version("v2.0.1-beta"),
            Some("v2.0.1-beta".to_string())
        );
        assert_eq!(
            extract_version("aider version 0.42.0"),
            Some("0.42.0".to_string())
        );
        assert_eq!(extract_version("no version here"), None);
    }

    #[test]
    fn test_config_dir() {
        let dir = config_dir();
        assert!(dir.is_ok());
        let dir = dir.unwrap();
        assert!(dir.to_string_lossy().contains("ralph-ultra"));
    }
}
