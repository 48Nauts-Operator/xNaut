// Who is editing what, and when two agents pick the same file (XNAUT-190).
//
// The message vocabulary is from ECC (github.com/affaan-m/ecc, MIT),
// `ecc2/src/comms/mod.rs`, which names the five things agents say to each
// other: TaskHandoff, Query, Response, Completed{summary, files_changed} and
// Conflict{file, description}. We already had ways to hand off, ask and answer.
// The two we did not have are the two this file is about.
//
// We depart from the source in where the signal comes from. ECC has agents
// declare a conflict to each other; nothing detects one. Here it falls out of
// the veto: every tool call already passes through /v1/veto with the tool, the
// arguments and the agent that is calling, so the file a Write is about to
// touch is known BEFORE it is written, and the second agent to reach for it can
// be told while both are still running. A conflict reported at the end of a run
// is a merge to resolve; a conflict reported at the write is a conversation.
//
// It never blocks. A collision is worth knowing about and is not evidence that
// the call is wrong: two agents may be editing the same file for good reason.
// The veto decides; this only speaks.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a touch counts as "still working on it". Beyond this the other
/// agent has almost certainly moved on and the warning would be noise.
const WINDOW: Duration = Duration::from_secs(30 * 60);

/// ponytail: one global map, not per-project. A file path is already unique
/// across worktrees, and the whole table is a few hundred entries on the worst
/// day. Shard by project if that ever stops being true.
static TOUCHES: Mutex<Option<HashMap<String, Touch>>> = Mutex::new(None);

#[derive(Debug, Clone)]
struct Touch {
    agent: String,
    at: Instant,
}

/// Two agents reaching for one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub file: String,
    /// The agent that got there first and may still be working.
    pub other: String,
    pub description: String,
}

/// Record that `agent` is about to touch whatever this call names, and say so
/// if somebody else touched it first.
///
/// Returns None for every call that names no file, which is most of them.
pub fn note(agent: &str, tool: &str, input: &serde_json::Value) -> Option<Conflict> {
    let agent = agent.trim();
    if agent.is_empty() {
        // An unattributed call cannot conflict with anyone: there is nobody to
        // name in the warning, and guessing would be worse than silence.
        return None;
    }
    let file = target_file(tool, input)?;
    note_at(agent, &file, Instant::now())
}

/// Split out so the tests can control time without sleeping.
fn note_at(agent: &str, file: &str, now: Instant) -> Option<Conflict> {
    let mut guard = TOUCHES.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    map.retain(|_, touch| now.duration_since(touch.at) < WINDOW);

    let previous = map.insert(
        file.to_string(),
        Touch { agent: agent.to_string(), at: now },
    );

    let previous = previous?;
    if previous.agent.eq_ignore_ascii_case(agent) {
        return None;
    }
    if now.duration_since(previous.at) >= WINDOW {
        return None;
    }
    Some(Conflict {
        file: file.to_string(),
        other: previous.agent.clone(),
        description: format!(
            "@{} is editing this file too, as of {} ago.",
            previous.agent,
            humanise(now.duration_since(previous.at))
        ),
    })
}

/// The file a tool call is about, when it names one outright.
///
/// Only the explicit parameters. A shell command that happens to mention a path
/// is not the same as a write, and treating it as one would warn about every
/// `cat` and `grep` until nobody read the warnings.
fn target_file(tool: &str, input: &serde_json::Value) -> Option<String> {
    const WRITING_TOOLS: &[&str] = &["write", "edit", "multiedit", "notebookedit"];
    if !WRITING_TOOLS.contains(&tool.trim().to_ascii_lowercase().as_str()) {
        return None;
    }
    for key in ["file_path", "filePath", "path", "notebook_path"] {
        if let Some(value) = input.get(key).and_then(serde_json::Value::as_str) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn humanise(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 90 {
        return format!("{seconds}s");
    }
    format!("{}m", seconds / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        *TOUCHES.lock().unwrap() = Some(HashMap::new());
    }

    #[test]
    fn a_second_agent_on_the_same_file_is_a_conflict() {
        reset();
        let now = Instant::now();
        assert_eq!(note_at("rudi", "/tmp/a.rs", now), None, "the first touch is never a conflict");
        let conflict = note_at("cortana", "/tmp/a.rs", now + Duration::from_secs(60))
            .expect("the second agent must be told");
        assert_eq!(conflict.other, "rudi");
        assert_eq!(conflict.file, "/tmp/a.rs");
        assert!(conflict.description.contains("@rudi"), "{}", conflict.description);
    }

    #[test]
    fn one_agent_editing_its_own_file_repeatedly_is_not_a_conflict() {
        reset();
        let now = Instant::now();
        note_at("rudi", "/tmp/b.rs", now);
        assert_eq!(note_at("rudi", "/tmp/b.rs", now + Duration::from_secs(5)), None);
        // Case is not identity: @Rudi is rudi.
        assert_eq!(note_at("Rudi", "/tmp/b.rs", now + Duration::from_secs(6)), None);
    }

    #[test]
    /// The warning is only useful while the other agent is plausibly still
    /// working. Past the window it is archaeology, and noise teaches people to
    /// ignore the channel it arrives on.
    fn a_stale_touch_does_not_warn() {
        reset();
        let now = Instant::now();
        note_at("rudi", "/tmp/c.rs", now);
        assert_eq!(note_at("cortana", "/tmp/c.rs", now + WINDOW + Duration::from_secs(1)), None);
    }

    #[test]
    fn only_calls_that_name_a_file_are_tracked() {
        // A shell command that mentions a path is not a write.
        assert_eq!(
            target_file("Bash", &serde_json::json!({"command": "cat /tmp/d.rs"})),
            None
        );
        assert_eq!(
            target_file("Write", &serde_json::json!({"file_path": "/tmp/d.rs"})),
            Some("/tmp/d.rs".to_string())
        );
        assert_eq!(
            target_file("Edit", &serde_json::json!({"file_path": "  "})),
            None,
            "a blank path names nothing"
        );
        assert_eq!(
            target_file("Read", &serde_json::json!({"file_path": "/tmp/d.rs"})),
            None,
            "reading the same file as someone else is not a conflict"
        );
    }

    #[test]
    fn an_unattributed_call_cannot_conflict() {
        reset();
        assert_eq!(note("", "Write", &serde_json::json!({"file_path": "/tmp/e.rs"})), None);
        assert_eq!(note("  ", "Write", &serde_json::json!({"file_path": "/tmp/e.rs"})), None);
    }
}
