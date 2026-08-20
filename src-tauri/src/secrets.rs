// Secrets out of the config files and into the macOS keychain (XNAUT-213).
//
// `plugins.json` held a TSB JWT and an API key in a world-readable file. The
// fix is not "chmod it" — that leaves the secret one careless `cp` from a
// backup, a screenshot or a support bundle. The value moves into the login
// keychain and the file keeps only a pointer, `keychain:<account>`.
//
// The subprocess still receives the plaintext, because an MCP server needs the
// real token; the point is that it is materialised at launch and never at
// rest. Argv exposure for codex is a separate, filed problem (XNAUT-206).
//
// The `security(1)` CLI rather than a crate: `keyring` is not a dependency and
// this is thirty lines. `-w` with no value makes it read the password from
// stdin twice, so nothing ever lands in argv.

use std::path::Path;
use std::process::{Command, Stdio};

/// Marker for a value that lives in the keychain, not in the file.
pub const PREFIX: &str = "keychain:";

fn service() -> String {
    // Tests must not write into the owner's real login keychain — the same
    // reasoning as XNAUT_PLUGINS_PATH.
    std::env::var("XNAUT_KEYCHAIN_SERVICE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        // A test that forgets to set the variable used to fall through to the
        // owner's real login keychain and write a fake token into it. Under
        // cfg(test) there is no such fallback.
        .unwrap_or_else(|| {
            if cfg!(test) { "xnaut-test-fallback".to_string() } else { "xnaut".to_string() }
        })
}

/// Does this config key name a credential? Name-shaped, deliberately: a
/// hardcoded list of the five SECUROSYS_* keys would miss the next plugin's
/// token, and that is how the first one ended up in plaintext.
pub fn is_secret_key(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["TOKEN", "SECRET", "PASSWORD", "PASSWD", "JWT", "API_KEY", "APIKEY", "AUTHORIZATION", "CREDENTIAL"]
        .iter()
        .any(|needle| upper.contains(needle))
}

/// Put `value` under `account`, replacing whatever was there.
pub fn store(account: &str, value: &str) -> Result<(), String> {
    if value.contains('\n') {
        // `security -w` reads the password as a line, so an embedded newline
        // would silently truncate it. No credential we handle has one.
        return Err("a multi-line value cannot be stored in the keychain".into());
    }
    // `-U` alone is not enough: over an existing item `security` still exits 45
    // (errSecDuplicateItem, -25299), `store` returns an error and the CALLER
    // keeps the plaintext in the file. Re-saving a credential is the common
    // case, so delete first and let the add be the only write.
    let _ = Command::new("security")
        .args(["delete-generic-password", "-s", &service(), "-a", account])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let mut child = Command::new("security")
        .args(["add-generic-password", "-U", "-s", &service(), "-a", account, "-w"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not run security: {e}"))?;
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().ok_or("security took no stdin")?;
        // It prompts for the password and then for a confirmation.
        stdin
            .write_all(format!("{value}\n{value}\n").as_bytes())
            .map_err(|e| format!("could not hand security the value: {e}"))?;
    }
    let status = child.wait().map_err(|e| format!("security failed: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("security refused the item ({status})"))
    }
}

/// The value behind `account`, or None when the keychain has no such item.
pub fn load(account: &str) -> Option<String> {
    let out = Command::new("security")
        .args(["find-generic-password", "-s", &service(), "-a", account, "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8(out.stdout).ok()?;
    let value = value.trim_end_matches('\n').to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
pub fn forget(account: &str) {
    let _ = Command::new("security")
        .args(["delete-generic-password", "-s", &service(), "-a", account])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Move `value` into the keychain and return the sentinel to store instead.
///
/// Returns None when the keychain would not take it, and the CALLER keeps the
/// plaintext. Losing someone's credential to a keychain hiccup is worse than
/// leaving it in a file that is already mode 600.
pub fn stash(account: &str, value: &str) -> Option<String> {
    if value.starts_with(PREFIX) || value.trim().is_empty() {
        return None;
    }
    match store(account, value) {
        Ok(()) => Some(format!("{PREFIX}{account}")),
        Err(reason) => {
            eprintln!("keychain: keeping {account} in the file: {reason}");
            None
        }
    }
}

/// A stored value as the process needs it: sentinels resolved, anything else
/// passed through. A sentinel whose item has gone resolves to empty, which
/// `blocker` then reports as a missing credential rather than launching a
/// server with the literal string "keychain:...".
pub fn resolve(value: &str) -> String {
    match value.strip_prefix(PREFIX) {
        Some(account) => load(account).unwrap_or_default(),
        None => value.to_string(),
    }
}

/// Take away group and world access from everything under the config
/// directory. Cheap, idempotent, and it covers files written before this
/// existed as well as ones other tools drop in later.
#[cfg(unix)]
pub fn harden(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            harden(&path);
        } else {
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
}

#[cfg(not(unix))]
pub fn harden(_dir: &Path) {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// XNAUT_KEYCHAIN_SERVICE is process-global; two tests with different
    /// services would race the same way XNAUT_PLUGINS_PATH did.
    pub(crate) static KEYCHAIN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// True when this machine's keychain will actually accept an item.
    ///
    /// Over ssh the login keychain is locked and `security` refuses every write
    /// with "user interaction is not allowed" (exit 36), so on the headless test
    /// machine these tests assert a property the environment cannot provide.
    /// The probe is a real store, because the only reliable way to know whether
    /// `security` will take an item is to hand it one.
    ///
    /// Skipping loudly beats a red suite that says nothing about the code. It
    /// also beats asserting the fallback, which is a different claim: xNAUT
    /// deliberately keeps the plaintext when the keychain refuses, since losing
    /// someone's credential is worse than a mode-600 file.
    pub(crate) fn keychain_usable() -> bool {
        let probe = "plugin/probe/KEYCHAIN_PROBE";
        let ok = store(probe, "probe").is_ok();
        forget(probe);
        if !ok {
            eprintln!("skipping: this keychain refuses items (no logged-in GUI session?)");
        }
        ok
    }

    #[test]
    fn a_secret_survives_the_round_trip_and_the_name_test() {
        let _guard = KEYCHAIN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::set_var("XNAUT_KEYCHAIN_SERVICE", "xnaut-test-roundtrip");
        if !keychain_usable() {
            std::env::remove_var("XNAUT_KEYCHAIN_SERVICE");
            return;
        }
        forget("plugin/demo/SECUROSYS_JWT");

        let sentinel = stash("plugin/demo/SECUROSYS_JWT", "eyJ0eXAi.header.sig").unwrap();
        assert_eq!(sentinel, "keychain:plugin/demo/SECUROSYS_JWT");
        assert_eq!(resolve(&sentinel), "eyJ0eXAi.header.sig");
        // Not a sentinel: passed through untouched.
        assert_eq!(resolve("https://tsb.example"), "https://tsb.example");
        // Storing over an existing item must win, not fail: that failure left
        // the plaintext in plugins.json and it was invisible.
        assert!(store("plugin/demo/SECUROSYS_JWT", "eyJ0eXAi.second.sig").is_ok(), "re-store failed");
        assert_eq!(resolve(&sentinel), "eyJ0eXAi.second.sig");

        // A sentinel with nothing behind it must read empty, never literal.
        forget("plugin/demo/SECUROSYS_JWT");
        assert_eq!(resolve(&sentinel), "");

        assert!(is_secret_key("SECUROSYS_JWT"));
        assert!(is_secret_key("SECUROSYS_API_KEY"));
        assert!(is_secret_key("GITEA_ACCESS_TOKEN"));
        assert!(is_secret_key("Authorization"));
        assert!(!is_secret_key("SECUROSYS_TSB_URL"));
        assert!(!is_secret_key("SECUROSYS_KEY_NAME"));
        assert!(!is_secret_key("SECUROSYS_PUBLISH_DIR"));
        std::env::remove_var("XNAUT_KEYCHAIN_SERVICE");
    }

    #[test]
    #[cfg(unix)]
    fn hardening_closes_a_world_readable_config_directory() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("xnaut-harden-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("looms")).unwrap();
        let file = dir.join("looms").join("plugins.json");
        std::fs::write(&file, "{}").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        harden(&dir);
        let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "the directory itself must stop being traversable");
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "a nested file must be closed too");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
