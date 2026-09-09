// App-wide debug log. The frontend forwards every console.{log,warn,error,info}
// plus uncaught errors / promise rejections here; the backend appends them to
// ~/Library/Application Support/xnaut/debug.log (size-capped). This gives a
// single readable file for diagnosing issues without opening DevTools.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

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

/// How this platform's file manager is asked to show the log, as a pure argv
/// builder (XNAUT-75).
///
/// Pure so it can be asserted without spawning anything. The failure this
/// guards is not a crash: a reveal that points at the wrong thing opens a
/// window on the home directory, or on nothing, and reads to the user as "the
/// button does nothing" — which is exactly the class of silent failure this
/// ticket exists to end. Only one of the three branches is ever compiled on a
/// given machine, so the other two are never exercised by hand.
fn reveal_argv(path: &Path) -> (&'static str, Vec<String>) {
    let file = path.to_string_lossy().into_owned();
    if cfg!(target_os = "macos") {
        // -R selects the file in Finder rather than opening it in whatever
        // app claims .log — on this machine that is Xcode.
        ("open", vec!["-R".into(), file])
    } else if cfg!(target_os = "windows") {
        // No space after the comma: explorer treats "/select, path" as two
        // arguments and silently opens Documents instead.
        ("explorer", vec![format!("/select,{file}")])
    } else {
        // xdg-open has no selection primitive, so the containing directory is
        // the most it can honestly do.
        let dir = path
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or(file);
        ("xdg-open", vec![dir])
    }
}

/// Show debug.log in the platform's file manager and answer with its path.
#[tauri::command]
pub fn debug_log_reveal() -> Result<String, String> {
    let path = log_path();
    // A reveal of a file that does not exist yet opens a window on nothing,
    // which is indistinguishable from a broken button. An empty log is a
    // truthful answer to "where is it", so create one.
    if !path.exists() {
        let dir = config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        std::fs::write(&path, "").map_err(|e| format!("create debug.log: {e}"))?;
    }
    let (program, args) = reveal_argv(&path);
    std::process::Command::new(program)
        .args(&args)
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

/// The last `lines` whole lines of `body`.
///
/// Whole lines matter because the caller has already cut the file at a byte
/// offset: the first line it holds is usually a fragment, and showing half an
/// entry as if it were an entry is how a reader mis-attributes a stack.
fn tail_lines(body: &str, lines: usize) -> String {
    if lines == 0 {
        return String::new();
    }
    let all: Vec<&str> = body.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

/// Read at most `cap` bytes from the END of a file.
///
/// The log is capped at 2 MB, so reading it whole to show two hundred lines
/// would push megabytes across the IPC boundary on every click. A byte cut can
/// land mid-character, so the bytes are decoded lossily and the leading
/// fragment is dropped by `tail_lines`.
fn read_tail(path: &Path, cap: u64) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("open debug.log: {e}"))?;
    let len = f
        .metadata()
        .map_err(|e| format!("stat debug.log: {e}"))?
        .len();
    if len > cap {
        f.seek(SeekFrom::Start(len - cap))
            .map_err(|e| format!("seek debug.log: {e}"))?;
    }
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)
        .map_err(|e| format!("read debug.log: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The tail of debug.log, so the truth it captures is readable inside the app
/// rather than only by someone who knows the file exists.
#[tauri::command]
pub fn debug_log_tail(lines: Option<usize>) -> Result<String, String> {
    let path = log_path();
    if !path.exists() {
        return Ok(String::new());
    }
    let body = read_tail(&path, 256_000)?;
    Ok(tail_lines(&body, lines.unwrap_or(200)))
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

    // --- XNAUT-75: the log has to be reachable from inside the app ---------

    /// The reveal must point at the LOG, not at a directory and not at the
    /// wrong thing. Only one branch compiles per platform, so without this the
    /// other two ship unexercised.
    #[test]
    fn reveal_points_the_file_manager_at_the_log_itself() {
        let path = PathBuf::from("/Users/x/Library/Application Support/xnaut/debug.log");
        let (program, args) = reveal_argv(&path);
        assert!(!program.is_empty(), "no file manager named");
        let line = args.join(" ");
        if cfg!(target_os = "linux") {
            // xdg-open cannot select, so the directory is the honest answer.
            assert_eq!(program, "xdg-open");
            assert!(
                line.ends_with("/xnaut") && !line.ends_with("debug.log"),
                "xdg-open was handed something other than the containing dir: {line}"
            );
        } else {
            assert!(
                line.contains("debug.log"),
                "{program} was not pointed at debug.log: {line}"
            );
        }
        if cfg!(target_os = "macos") {
            // Without -R this OPENS the log, which on this machine means Xcode.
            assert_eq!(args.first().map(String::as_str), Some("-R"));
        }
        if cfg!(target_os = "windows") {
            // "/select, path" is two arguments and silently opens Documents.
            assert!(line.starts_with("/select,") && !line.starts_with("/select, "));
        }
    }

    #[test]
    fn tail_returns_the_last_whole_lines_and_never_a_fragment() {
        let body = "alpha\nbeta\ngamma\ndelta\n";
        assert_eq!(tail_lines(body, 2), "gamma\ndelta");
        // Asking for more than exists is not an error, it is the whole file.
        assert_eq!(tail_lines(body, 99), "alpha\nbeta\ngamma\ndelta");
        assert_eq!(tail_lines(body, 0), "");
    }

    #[test]
    fn tail_reads_only_the_end_of_a_large_log() {
        let mut p = std::env::temp_dir();
        p.push(format!("xnaut-debuglog-tail-{}.log", std::process::id()));
        let mut body = String::new();
        for i in 0..50_000 {
            body.push_str(&format!("{i} 📥 line with multi-byte content ▒▒▒\n"));
        }
        std::fs::write(&p, &body).unwrap();

        // Cut small enough that the window certainly lands mid-line and
        // mid-character — the case a naive String slice panics on.
        let read = read_tail(&p, 4_000).unwrap();
        assert!(read.len() <= 4_000 + 8, "read more than the cap: {}", read.len());
        let out = tail_lines(&read, 3);
        assert_eq!(out.lines().count(), 3);
        assert!(
            out.lines().all(|l| l.ends_with('▒')),
            "a truncated fragment was served as an entry: {out}"
        );
        assert!(out.ends_with("49999 📥 line with multi-byte content ▒▒▒"));
        let _ = std::fs::remove_file(&p);
    }
}
