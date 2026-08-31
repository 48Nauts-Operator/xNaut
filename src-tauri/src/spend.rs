// The spend ceiling (XNAUT-245 item 2): unattended launches get a budget.
//
// 64 ready tickets on frontier models with nobody watching had no stop at
// all — the kill-switches stop merges, not money. Three limits, smallest
// that closes the hole:
//
//   1. A CONCURRENT cap: at most N live agent sessions at once. Default 2,
//      the number the validation stampede taught us.
//   2. A DAILY launch cap: at most M fresh launches per calendar day.
//   3. A HARD STOP: crossing the daily cap engages the existing `read_only`
//      kill-switch rather than inventing a second halt — every write tool
//      then refuses, the audit log names the flip, and lifting it is the
//      owner's move like any other switch.
//
// Conversation-mode launches are NOT counted: a chat with an agent is the
// owner interacting, and the ceiling exists for the unattended fleet, not
// for him. The enforcement point is agent_profile_launch, which every
// launch path (cold launch included) already funnels through.
//
// Same file pattern as switches.rs: missing or unreadable config means the
// DEFAULTS apply — the ceiling can be raised deliberately, never lost.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpendCeiling {
    /// Live agent sessions allowed at once (working/blocked/waiting/permission).
    #[serde(default = "default_concurrent")]
    pub max_concurrent: u32,
    /// Fresh (non-conversation) launches allowed per calendar day, UTC.
    #[serde(default = "default_daily")]
    pub max_daily_launches: u32,
}

fn default_concurrent() -> u32 {
    2
}
fn default_daily() -> u32 {
    20
}

impl Default for SpendCeiling {
    fn default() -> Self {
        Self {
            max_concurrent: default_concurrent(),
            max_daily_launches: default_daily(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DayCounter {
    day: String,
    launches: u32,
}

fn config_dir() -> PathBuf {
    if let Some(root) = std::env::var_os("XNAUT_SPEND_DIR") {
        return PathBuf::from(root);
    }
    dirs::config_dir()
        .map(|p| p.join("xnaut"))
        .unwrap_or_else(|| PathBuf::from(".xnaut"))
}

fn ceiling_path() -> PathBuf {
    config_dir().join("spend-ceiling.json")
}

fn counter_path() -> PathBuf {
    config_dir().join("spend-launches.json")
}

pub fn load_ceiling() -> SpendCeiling {
    std::fs::read_to_string(ceiling_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn spend_ceiling_get() -> SpendCeiling {
    load_ceiling()
}

#[tauri::command]
pub fn spend_ceiling_set(ceiling: SpendCeiling) -> Result<SpendCeiling, String> {
    std::fs::create_dir_all(config_dir()).map_err(|e| format!("create config dir: {e}"))?;
    let body = serde_json::to_string_pretty(&ceiling).map_err(|e| e.to_string())?;
    std::fs::write(ceiling_path(), body).map_err(|e| format!("write spend ceiling: {e}"))?;
    Ok(load_ceiling())
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn load_counter() -> DayCounter {
    let counter: DayCounter = std::fs::read_to_string(counter_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if counter.day == today() {
        counter
    } else {
        DayCounter {
            day: today(),
            launches: 0,
        }
    }
}

fn store_counter(counter: &DayCounter) -> Result<(), String> {
    std::fs::create_dir_all(config_dir()).map_err(|e| format!("create config dir: {e}"))?;
    let body = serde_json::to_string_pretty(counter).map_err(|e| e.to_string())?;
    std::fs::write(counter_path(), body).map_err(|e| format!("write launch counter: {e}"))
}

/// Admit one fresh launch, or say exactly why not.
///
/// `live_sessions` is the caller's count of sessions in a live status; the
/// caller has the state lock, this module deliberately does not.
pub fn admit_launch(live_sessions: usize) -> Result<(), String> {
    let ceiling = load_ceiling();
    if live_sessions >= ceiling.max_concurrent as usize {
        return Err(format!(
            "spend ceiling: {live_sessions} agent sessions are already live and the concurrent \
             cap is {}. Wait for one to finish, or raise the cap (spend-ceiling.json).",
            ceiling.max_concurrent
        ));
    }
    let mut counter = load_counter();
    if counter.launches >= ceiling.max_daily_launches {
        // The hard stop: engage the EXISTING read_only switch. One halt
        // mechanism in the whole app, one audit trail, one place to lift it.
        let mut switches = crate::switches::load();
        if !switches.read_only {
            switches.read_only = true;
            let _ = crate::switches::store(&switches);
        }
        return Err(format!(
            "spend ceiling: {} launches today reached the daily cap of {}. The read_only \
             kill-switch is now engaged; lifting it and raising the cap are the owner's moves.",
            counter.launches, ceiling.max_daily_launches
        ));
    }
    counter.launches += 1;
    store_counter(&counter)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// XNAUT_SPEND_DIR is process-global and tests run in parallel, so every
    /// test that touches the store serializes on this lock and uses its own
    /// scratch directory.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn scratch(name: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = std::env::temp_dir().join(format!("xnaut-spend-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XNAUT_SPEND_DIR", &dir);
        // The hard stop writes through switches::store; keep that INSIDE the
        // scratch dir. Flipping the owner's real read_only from a unit test
        // happened once and poisoned every later PM test in the process.
        std::env::set_var("XNAUT_SWITCHES_DIR", &dir);
        (guard, dir)
    }

    #[test]
    fn defaults_hold_when_no_file_exists() {
        let (_g, _d) = scratch("defaults");
        let ceiling = load_ceiling();
        assert_eq!(ceiling.max_concurrent, 2);
        assert_eq!(ceiling.max_daily_launches, 20);
    }

    #[test]
    fn the_concurrent_cap_refuses_the_third_agent() {
        let (_g, _d) = scratch("concurrent");
        assert!(admit_launch(0).is_ok());
        assert!(admit_launch(1).is_ok());
        let refused = admit_launch(2).unwrap_err();
        assert!(refused.contains("concurrent cap is 2"), "{refused}");
    }

    #[test]
    fn the_daily_cap_engages_read_only_and_counts_across_calls() {
        let (_g, dir) = scratch("daily");
        spend_ceiling_set(SpendCeiling {
            max_concurrent: 10,
            max_daily_launches: 3,
        })
        .unwrap();
        for _ in 0..3 {
            admit_launch(0).unwrap();
        }
        let refused = admit_launch(0).unwrap_err();
        assert!(refused.contains("daily cap of 3"), "{refused}");
        // The hard stop really engaged the switch — in the SCRATCH dir,
        // which XNAUT_SWITCHES_DIR guarantees.
        assert!(refused.contains("read_only"), "{refused}");
        let switches: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("kill-switches.json")).unwrap())
                .unwrap();
        assert_eq!(switches["read_only"], serde_json::json!(true));
        let again = admit_launch(0).unwrap_err();
        assert!(again.contains("daily cap"), "{again}");
        // Counter survives a reload from disk.
        let on_disk: DayCounter =
            serde_json::from_str(&std::fs::read_to_string(dir.join("spend-launches.json")).unwrap())
                .unwrap();
        assert_eq!(on_disk.launches, 3);
    }

    #[test]
    fn the_counter_resets_on_a_new_day() {
        let (_g, dir) = scratch("newday");
        admit_launch(0).unwrap();
        let stale = DayCounter {
            day: "2020-01-01".into(),
            launches: 999,
        };
        std::fs::write(
            dir.join("spend-launches.json"),
            serde_json::to_string(&stale).unwrap(),
        )
        .unwrap();
        assert!(admit_launch(0).is_ok(), "a new day starts at zero");
    }
}
