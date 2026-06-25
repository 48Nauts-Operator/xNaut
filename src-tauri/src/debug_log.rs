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
    // Operate on bytes — slicing a String at a raw byte offset panics when it
    // lands mid-UTF-8-char (terminal output has multi-byte chars/emoji), which
    // with panic=abort would crash the whole app.
    if let Ok(bytes) = std::fs::read(path) {
        let keep = bytes.len().saturating_sub(1_000_000);
        // start just after the next newline so we keep whole lines
        let start = bytes[keep..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| keep + i + 1)
            .unwrap_or(keep);
        let _ = std::fs::write(path, &bytes[start..]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_does_not_panic_on_multibyte_boundary() {
        let mut p = std::env::temp_dir();
        p.push(format!("xnaut-debuglog-test-{}.log", std::process::id()));
        // > CAP_BYTES of multi-byte content so the byte cut at len-1MB lands
        // mid-character — the old String-slice version panicked here.
        let mut body = String::new();
        while (body.len() as u64) < CAP_BYTES + 100_000 {
            body.push_str("📥 terminal output ▒▒▒\n");
        }
        std::fs::write(&p, &body).unwrap();
        trim_if_large(&p); // must not panic
        let after = std::fs::metadata(&p).unwrap().len();
        assert!(after <= CAP_BYTES);
        let _ = std::fs::remove_file(&p);
    }
}
