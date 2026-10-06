// Layer kill-switches (XNAUT-231): the firewall's master switches, not
// per-rule policy. One flag drops a whole enforcement layer, every flip is
// audited, and lifting is the owner's move — there is deliberately no agent
// tool that touches these. The enforcement points read the file at the
// moment of the action, so a flip needs no restart and no running frontend.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

fn config_dir() -> PathBuf {
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

/// Persists the switches and appends one audit line naming what changed.
/// The audit is not optional: a kill-switch nobody can trace is how "why
/// were merges off all week" happens.
pub fn store(next: &KillSwitches) -> Result<(), String> {
    let previous = load();
    std::fs::create_dir_all(config_dir()).map_err(|e| format!("create config dir: {e}"))?;
    let body = serde_json::to_string_pretty(next).map_err(|e| e.to_string())?;
    std::fs::write(store_path(), body).map_err(|e| format!("write switches: {e}"))?;
    if previous != *next {
        let line = serde_json::json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "from": previous,
            "to": next,
        });
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(audit_path())
            .map_err(|e| format!("open audit log: {e}"))?;
        use std::io::Write;
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
