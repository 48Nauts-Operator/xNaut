// Layer kill-switches (XNAUT-231): the firewall's master switches, not
// per-rule policy. One flag drops a whole enforcement layer, every flip is
// audited, and lifting is the owner's move — there is deliberately no agent
// tool that touches these. The enforcement points read the file at the
// moment of the action, so a flip needs no restart and no running frontend.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct KillSwitches {
    /// merge_ticket refuses everything, everywhere. The overnight switch.
    /// unmerge_ticket stays live: the safety valve is never frozen.
    #[serde(default)]
    pub freeze_merges: bool,
    /// Every write tool refuses; list/read still work. Looking without
    /// anything moving.
    #[serde(default)]
    pub read_only: bool,
    /// The risk threshold drops to zero: every merge parks in the Mesh for
    /// the owner, whatever its score.
    #[serde(default)]
    pub approve_everything: bool,
    /// Quarantined agent handles: their nudges are dead, tickets cannot be
    /// assigned to them, and /v1/tickets/mine answers them empty.
    #[serde(default)]
    pub quarantined: Vec<String>,
}

fn normalize(handle: &str) -> String {
    let trimmed = handle.trim();
    trimmed
        .strip_prefix('@')
        .unwrap_or(trimmed)
        .to_ascii_lowercase()
}

impl KillSwitches {
    /// Project-level automatic merge permission is not a per-merge owner
    /// approval. The explicit merge_ticket approval flow remains separate.
    pub(crate) fn automatic_merge_hold(&self) -> Option<&'static str> {
        if self.read_only {
            Some("Automatic merge paused: read_only kill-switch engaged")
        } else if self.freeze_merges {
            Some("Automatic merge paused: freeze_merges kill-switch engaged")
        } else if self.approve_everything {
            Some("Automatic merge awaits explicit owner approval: approve_everything engaged")
        } else {
            None
        }
    }

    pub fn is_quarantined(&self, handle: &str) -> bool {
        let handle = normalize(handle);
        self.quarantined.iter().any(|q| normalize(q) == handle)
    }
}

// Tests that exercise process-wide switch readers bind actual files to their
// own thread. This changes no production path or admission rule; unlike a
// process environment variable, one spending fixture cannot pause another test.
#[cfg(test)]
thread_local! { static TEST_ROOT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(crate) struct TestScope {
    previous: Option<PathBuf>,
    root: PathBuf,
    remove: bool,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
#[cfg(test)]
impl TestScope {
    pub(crate) fn in_dir(root: PathBuf, switches: KillSwitches) -> Self {
        std::fs::create_dir_all(&root).unwrap();
        store_at(&root.join("kill-switches.json"), &root.join("kill-switches.log"), &switches).unwrap();
        let previous = TEST_ROOT.with(|v| v.replace(Some(root.clone())));
        Self { previous, root, remove: false, _thread: std::marker::PhantomData }
    }
    pub(crate) fn unpaused(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("xnaut-switch-fixture-{name}-{}", uuid::Uuid::new_v4()));
        let mut scope = Self::in_dir(root, KillSwitches::default());
        scope.remove = true;
        scope
    }
}
#[cfg(test)]
impl Drop for TestScope {
    fn drop(&mut self) {
        TEST_ROOT.with(|v| *v.borrow_mut() = self.previous.take());
        if self.remove { let _ = std::fs::remove_dir_all(&self.root); }
    }
}

fn config_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(root) = TEST_ROOT.with(|v| v.borrow().clone()) { return root; }
    // Test redirect, same pattern as XNAUT_LEASE_DIR / XNAUT_SPEND_DIR: a
    // test that engages a switch must never write the OWNER'S real file — a
    // spend-ceiling test once flipped the real read_only overnight.
    if let Some(root) = std::env::var_os("XNAUT_SWITCHES_DIR") {
        return PathBuf::from(root);
    }
    crate::loop_acceptance::platform_config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn store_path() -> PathBuf {
    config_dir().join("kill-switches.json")
}

fn audit_path() -> PathBuf {
    config_dir().join("kill-switches.log")
}

/// Missing or unreadable file means every layer is up — the switches can
/// only ever be flipped deliberately, never by a lost file.
pub fn load() -> KillSwitches {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Automatic merge and PM writers use the strict reader. A genuinely absent file keeps
/// the established defaults; existing but unavailable/corrupt authority is a hold.
fn load_strict_at(path: &Path) -> Result<KillSwitches, String> {
    let body = match std::fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match path.symlink_metadata() {
                Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => Ok(KillSwitches::default()),
                _ => Err("Automatic merge paused: kill-switch file exists but cannot be read".into()),
            };
        }
        Err(error) => return Err(format!("Automatic merge paused: cannot read kill-switch file: {error}")),
    };
    serde_json::from_str(&body)
        .map_err(|error| format!("Automatic merge paused: invalid kill-switch file: {error}"))
}

pub(crate) fn automatic_merge_hold_strict() -> Option<String> {
    automatic_merge_hold_at(&store_path())
}
pub(crate) fn automatic_merge_hold_at(path: &Path) -> Option<String> {
    match load_strict_at(path) {
        Ok(switches) => switches.automatic_merge_hold().map(str::to_owned),
        Err(reason) => Some(reason),
    }
}

/// PM automation fails closed on unreadable authority; direct owner commands
/// retain their explicit-action contract and do not use this background gate.
pub(crate) fn automatic_pm_write_hold_strict() -> Option<String> {
    match load_strict_at(&store_path()) {
        Ok(switches) if switches.read_only => Some("Automatic PM write paused: read_only kill-switch engaged; evidence retained for retry".into()),
        Ok(_) => None,
        Err(reason) => Some(reason.replace("Automatic merge", "Automatic PM write")),
    }
}

/// Persists the switches and appends one audit line naming what changed.
/// The audit is not optional: a kill-switch nobody can trace is how "why
/// were merges off all week" happens.
pub fn store(next: &KillSwitches) -> Result<(), String> {
    store_at(&store_path(), &audit_path(), next)
}

fn store_at(path: &Path, audit: &Path, next: &KillSwitches) -> Result<(), String> {
    use std::io::Write;
    let previous = std::fs::read_to_string(path).ok()
        .and_then(|s| serde_json::from_str::<KillSwitches>(&s).ok()).unwrap_or_default();
    let dir = path.parent().ok_or("Kill-switch path has no directory")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("create config dir: {e}"))?;
    let body = serde_json::to_vec_pretty(next).map_err(|e| e.to_string())?;
    // One complete version replaces the old one. Unique sibling files prevent
    // concurrent app instances from sharing a partial staging file.
    let temp = dir.join(format!(".kill-switches-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true)
            .open(&temp).map_err(|e| format!("stage switches: {e}"))?;
        if let Ok(metadata) = std::fs::metadata(path) {
            file.set_permissions(metadata.permissions()).map_err(|e| format!("switch permissions: {e}"))?;
        }
        file.write_all(&body).and_then(|_| file.sync_all())
            .map_err(|e| format!("write switches: {e}"))?;
        std::fs::rename(&temp, path).map_err(|e| format!("publish switches: {e}"))
    })();
    if result.is_err() { let _ = std::fs::remove_file(&temp); }
    result?;
    if previous != *next {
        let line = serde_json::json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "from": previous,
            "to": next,
        });
        let mut log = std::fs::OpenOptions::new()
            .create(true).append(true).open(audit)
            .map_err(|e| format!("open audit log: {e}"))?;
        writeln!(log, "{line}").map_err(|e| format!("append audit: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn kill_switches_get() -> KillSwitches {
    load()
}

#[tauri::command]
pub fn kill_switches_set(switches: KillSwitches) -> Result<KillSwitches, String> {
    store(&switches)?;
    Ok(load())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_restores_outer_actual_switch_file() {
        let outer = TestScope::unpaused("outer");
        let mut paused = KillSwitches::default(); paused.read_only = true;
        store(&paused).unwrap();
        assert!(automatic_pm_write_hold_strict().unwrap().contains("read_only"));
        {
            let _inner = TestScope::unpaused("inner");
            assert!(!load().read_only);
            assert!(automatic_pm_write_hold_strict().is_none());
        }
        assert!(load().read_only);
        assert_eq!(store_path(), outer.root.join("kill-switches.json"));
    }

    #[test]
    fn strict_merge_switch_reader_distinguishes_absence_from_invalid_authority() {
        let dir = std::env::temp_dir().join(format!("xnaut-strict-switches-{}", uuid::Uuid::new_v4()));
        let path = dir.join("kill-switches.json");
        assert!(automatic_merge_hold_at(&path).is_none());
        assert!(!dir.exists(), "read must not create owner state");
        std::fs::create_dir_all(&dir).unwrap();
        for invalid in ["{", "", r#"{"freeze_merges":"yes"}"#] {
            std::fs::write(&path, invalid).unwrap();
            assert!(automatic_merge_hold_at(&path).unwrap().contains("invalid kill-switch"));
        }
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(automatic_merge_hold_at(&path).unwrap().contains("cannot read"));
        std::fs::remove_dir(&path).unwrap();
        #[cfg(unix)] {
            std::os::unix::fs::symlink(dir.join("missing-target"), &path).unwrap();
            assert!(automatic_merge_hold_at(&path).is_some(), "dangling file is not absent authority");
            std::fs::remove_file(&path).unwrap();
        }
        std::fs::write(&path, "{}").unwrap();
        assert!(automatic_merge_hold_at(&path).is_none(), "valid legacy defaults remain supported");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn switch_update_atomically_replaces_complete_version_and_preserves_audit() {
        use std::io::Read;
        let dir = std::env::temp_dir().join(format!("xnaut-atomic-switches-{}", uuid::Uuid::new_v4()));
        let path = dir.join("kill-switches.json");
        let audit = dir.join("kill-switches.log");
        let before = KillSwitches { read_only: true, ..Default::default() };
        let after = KillSwitches { freeze_merges: true, quarantined: vec!["fixture".into()], ..Default::default() };
        store_at(&path, &audit, &before).unwrap();
        let mut old_reader = std::fs::File::open(&path).unwrap();
        store_at(&path, &audit, &after).unwrap();
        // Existing readers retain the complete old inode; a truncating write
        // would instead expose the new body (or a partial one) through this FD.
        let mut old_body = String::new();
        old_reader.read_to_string(&mut old_body).unwrap();
        assert_eq!(serde_json::from_str::<KillSwitches>(&old_body).unwrap(), before);
        assert_eq!(load_strict_at(&path).unwrap(), after);
        assert!(automatic_merge_hold_at(&path).unwrap().contains("freeze_merges"));
        let entries: Vec<serde_json::Value> = std::fs::read_to_string(&audit).unwrap().lines()
            .map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1]["from"], serde_json::to_value(&before).unwrap());
        assert_eq!(entries[1]["to"], serde_json::to_value(&after).unwrap());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "no staging files remain");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn defaults_leave_every_layer_up() {
        let sw = KillSwitches::default();
        assert!(!sw.freeze_merges && !sw.read_only && !sw.approve_everything);
        assert!(sw.quarantined.is_empty());
    }

    #[test]
    fn quarantine_ignores_case_and_the_at_prefix() {
        let sw = KillSwitches {
            quarantined: vec!["@Claudi".into()],
            ..Default::default()
        };
        assert!(sw.is_quarantined("claudi"));
        assert!(sw.is_quarantined("@CLAUDI"));
        assert!(!sw.is_quarantined("codex"));
    }

    #[test]
    fn a_garbled_file_fails_up_not_open() {
        // load() on unparseable content must mean "all layers up", never a
        // panic and never some layers down.
        let parsed: Result<KillSwitches, _> = serde_json::from_str("{nonsense");
        assert!(parsed.is_err());
        assert_eq!(
            serde_json::from_str::<KillSwitches>("{}").unwrap(),
            KillSwitches::default()
        );
    }
}
