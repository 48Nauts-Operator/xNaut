// App-wide debug log. The frontend forwards every console.{log,warn,error,info}
// plus uncaught errors / promise rejections here; the backend appends them to
// ~/Library/Application Support/xnaut/debug.log (size-capped). This gives a
// single readable file for diagnosing issues without opening DevTools.

use std::io::Write;
use std::path::PathBuf;

const CAP_BYTES: u64 = 2_000_000; // trim to last ~1 MB once we cross 2 MB

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn log_path() -> PathBuf {
    config_dir().join("debug.log")
}

fn trim_if_large(path: &PathBuf) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() <= CAP_BYTES {
        return;
    }
    if let Ok(body) = std::fs::read_to_string(path) {
        let keep = body.len().saturating_sub(1_000_000);
        // start at the next line boundary so we don't cut a line mid-way
        let start = body[keep..]
            .find('\n')
            .map(|i| keep + i + 1)
            .unwrap_or(keep);
        let _ = std::fs::write(path, &body[start..]);
    }
}

#[tauri::command]
pub fn debug_log_append(entries: Vec<String>) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = log_path();
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open debug.log: {e}"))?;
    for e in &entries {
        let _ = writeln!(f, "{e}");
    }
    drop(f);
    trim_if_large(&path);
    Ok(())
}

#[tauri::command]
pub fn debug_log_path() -> Result<String, String> {
    Ok(log_path().to_string_lossy().into_owned())
}

#[tauri::command]
pub fn debug_log_clear() -> Result<(), String> {
    let path = log_path();
    if path.exists() {
        std::fs::write(&path, "").map_err(|e| format!("clear debug.log: {e}"))?;
    }
    Ok(())
}
