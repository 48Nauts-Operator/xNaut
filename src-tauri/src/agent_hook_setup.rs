// Per-agent hook-script writers (XNAUT-25). Closes the status-hook loop: the
// listener (agent_hooks.rs) and env injection (agents.rs) already ship, but
// nothing makes agents CALL the hook. Here we (a) locate the bundled
// scripts/hooks/xnaut-hook.sh and (b) merge Stop/Notification hooks into a
// launched Claude worktree's .claude/settings.local.json so it fires
// `done`/`permission` states instead of relying on the 2s silence heuristic.
//
// settings.local.json (not settings.json) on purpose: it is the machine-local,
// git-ignored layer, so we never pollute the user's committed project config.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Locate the bundled hook script. Mirrors skills.rs resolution: walk up from
/// the exe for a `scripts/hooks/xnaut-hook.sh` (dev tree) or the same under a
/// `.app/Contents/Resources` layout, then fall back to cwd.
pub fn hook_script_path() -> Option<PathBuf> {
    let rel = Path::new("scripts").join("hooks").join("xnaut-hook.sh");
    let mut p = std::env::current_exe().ok()?;
    for _ in 0..6 {
        p.pop();
        let direct = p.join(&rel);
        if direct.is_file() {
            return Some(direct);
        }
        let in_resources = p.join("Resources").join(&rel);
        if in_resources.is_file() {
            return Some(in_resources);
        }
    }
    let cwd = std::env::current_dir().ok()?.join(&rel);
    cwd.is_file().then_some(cwd)
}

fn xnaut_hook_group(command: &str) -> Value {
    json!({ "hooks": [ { "type": "command", "command": command } ] })
}

/// True if a command line invokes any hook script of ours.
///
/// Matched by shape — `xnaut-<something>.sh` — rather than by a list of names.
/// The list version had to be edited every time a script was added, and
/// forgetting meant the new hook was re-appended on every launch: a settings
/// file that grows a duplicate per session. The brief script (SessionStart) is
/// the third, and was exactly that bug until this became a pattern.
fn is_xnaut_script(command: &str) -> bool {
    command.match_indices("xnaut-").any(|(i, _)| {
        command[i..]
            .split_whitespace()
            .next()
            // The command is a shell line, so the path is usually quoted:
            // `sh "…/xnaut-hook.sh" done`. Without trimming the closing quote
            // the token ends in `"` and never matches, which would make the
            // marker answer false for every real hook we have ever written.
            .map(|w| w.trim_end_matches(['"', '\'']))
            .is_some_and(|w| w.ends_with(".sh"))
    })
}

/// True if any hook group in `arr` already invokes one of our scripts
/// (idempotency marker).
fn has_xnaut_hook(arr: &[Value]) -> bool {
    arr.iter().any(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .map(|hs| {
                hs.iter().any(|h| {
                    h.get("command").and_then(Value::as_str).is_some_and(is_xnaut_script)
                })
            })
            .unwrap_or(false)
    })
}

/// Merge our Stop→`done` and Notification→`permission` hooks into a Claude
/// settings JSON tree. Never clobbers existing keys or other hooks; skips any
/// event that already has an xnaut hook. Returns true if anything changed.
/// Pure — unit-tested.
/// The veto script, beside the status hook (XNAUT-132).
pub fn veto_script_path() -> Option<PathBuf> {
    hook_script_path().map(|path| path.with_file_name("xnaut-veto.sh")).filter(|path| path.is_file())
}

/// The brief script, beside the others. SessionStart is the only hook whose
/// stdout becomes context, which is what makes it the delivery mechanism for
/// the project brief (flow_context).
pub fn brief_script_path() -> Option<PathBuf> {
    hook_script_path()
        .map(|path| path.with_file_name("xnaut-brief.sh"))
        .filter(|path| path.is_file())
}

pub fn merge_claude_hooks(root: &mut Value, script: &str) -> bool {
    if !root.is_object() {
        *root = json!({});
    }
    let obj = root.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        // A non-object `hooks` is the user's; don't touch it.
        return false;
    };

    let mut changed = false;

    // PreToolUse is the one hook that can refuse (XNAUT-132). It is added only
    // when the veto script is actually on disk: a PreToolUse entry pointing at
    // a missing file is a hook that fails on every call, which is the failure
    // this whole design is built to avoid.
    if let Some(veto) = veto_script_path() {
        let entry = hooks.entry("PreToolUse".to_string()).or_insert_with(|| json!([]));
        if let Some(arr) = entry.as_array_mut() {
            if !has_xnaut_hook(arr) {
                arr.push(xnaut_hook_group(&format!("sh \"{}\"", veto.display())));
                changed = true;
            }
        }
    }

    // SessionStart carries the project brief. Same on-disk guard as the veto: a
    // hook pointing at a missing script fires on every session and fails on
    // every session. Unlike the others it takes no state argument — its whole
    // output IS the payload, so anything printed after the command would land
    // in the agent's context as though the project had said it.
    if let Some(brief) = brief_script_path() {
        let entry = hooks.entry("SessionStart".to_string()).or_insert_with(|| json!([]));
        if let Some(arr) = entry.as_array_mut() {
            if !has_xnaut_hook(arr) {
                arr.push(xnaut_hook_group(&format!("sh \"{}\"", brief.display())));
                changed = true;
            }
        }
    }

    for (event, state) in [("Stop", "done"), ("Notification", "permission")] {
        let entry = hooks.entry(event.to_string()).or_insert_with(|| json!([]));
        let Some(arr) = entry.as_array_mut() else {
            continue;
        };
        if has_xnaut_hook(arr) {
            continue;
        }
        arr.push(xnaut_hook_group(&format!("sh \"{script}\" {state}")));
        changed = true;
    }
    changed
}

/// Write the merged hooks into `<worktree>/.claude/settings.local.json`.
/// Returns true if the file was created/updated, false if already configured.
pub fn write_claude_hooks(worktree: &str, script: &Path) -> Result<bool, String> {
    let dir = Path::new(worktree).join(".claude");
    let file = dir.join("settings.local.json");
    let mut root: Value = if file.exists() {
        let raw =
            std::fs::read_to_string(&file).map_err(|e| format!("read {}: {e}", file.display()))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            json!({})
        } else {
            serde_json::from_str(trimmed).map_err(|e| format!("parse {}: {e}", file.display()))?
        }
    } else {
        json!({})
    };

    if !merge_claude_hooks(&mut root, &script.to_string_lossy()) {
        return Ok(false);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let pretty = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    std::fs::write(&file, format!("{pretty}\n"))
        .map_err(|e| format!("write {}: {e}", file.display()))?;
    Ok(true)
}

/// Dispatch per-agent hook setup at launch. Called only when the hook server is
/// live (so XNAUT_HOOK_URL is in the agent env). Best-effort: logs and moves on,
/// never fails the launch. Agents without a writer keep the silence fallback.
pub fn apply_agent_setup(agent_binary: &str, worktree: &str) {
    if agent_binary != "claude" {
        // codex/gemini/others: no notify-hook writer yet — silence detection stays.
        return;
    }
    let Some(script) = hook_script_path() else {
        eprintln!(
            "[agent_hooks] xnaut-hook.sh not found (dev tree or bundle Resources) — skipping claude hook setup for {worktree}"
        );
        return;
    };
    match write_claude_hooks(worktree, &script) {
        Ok(true) => eprintln!(
            "[agent_hooks] wrote Stop/Notification hooks into {worktree}/.claude/settings.local.json"
        ),
        Ok(false) => {} // already configured — idempotent
        Err(e) => eprintln!("[agent_hooks] claude hook setup failed for {worktree}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "/Applications/xNAUT.app/Contents/Resources/scripts/hooks/xnaut-hook.sh";

    /// The idempotency marker must recognise a script by SHAPE, not by a list
    /// of names someone has to remember to extend. Every failure of this kind
    /// looks the same from outside: a settings file that grows one duplicate
    /// hook per agent launch, forever.
    #[test]
    fn the_marker_recognises_any_xnaut_script_including_ones_not_written_yet() {
        for name in ["xnaut-hook.sh", "xnaut-veto.sh", "xnaut-brief.sh", "xnaut-not-invented-yet.sh"] {
            let cmd = format!("sh \"/opt/xnaut/scripts/hooks/{name}\" done");
            assert!(is_xnaut_script(&cmd), "{name} was not recognised as ours");
        }
        // Somebody else's hook must never be mistaken for ours, or we would
        // skip installing our own and silently ship no hooks at all.
        assert!(!is_xnaut_script("sh /usr/local/bin/their-hook.sh"));
        assert!(!is_xnaut_script("echo xnaut-hook"));
    }

    #[test]
    fn adds_both_hooks_to_empty() {
        let mut root = json!({});
        assert!(merge_claude_hooks(&mut root, SCRIPT));
        let stop = &root["hooks"]["Stop"][0]["hooks"][0]["command"];
        let notif = &root["hooks"]["Notification"][0]["hooks"][0]["command"];
        assert!(stop.as_str().unwrap().contains("xnaut-hook.sh"));
        assert!(stop.as_str().unwrap().ends_with(" done"));
        assert!(notif.as_str().unwrap().ends_with(" permission"));
    }

    /// The veto is the only hook that can refuse, so it has to be installed
    /// exactly once and only when its script exists. A PreToolUse entry
    /// pointing at a missing file fails on every call, which is the failure
    /// this design exists to avoid.
    #[test]
    fn the_veto_hook_installs_once_and_only_when_present() {
        let mut root = json!({});
        merge_claude_hooks(&mut root, SCRIPT);
        let installed = root["hooks"]["PreToolUse"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(installed <= 1, "the veto was installed {installed} times");

        // Whatever the first run decided, a second run changes nothing.
        let before = root.clone();
        assert!(!merge_claude_hooks(&mut root, SCRIPT));
        assert_eq!(root, before, "a second run rewrote the hooks");

        if let Some(path) = veto_script_path() {
            assert_eq!(installed, 1, "the script is at {} but no hook was installed", path.display());
            let command = root["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str().unwrap();
            assert!(command.contains("xnaut-veto.sh"), "{command}");
        }
    }

    #[test]
    fn idempotent_second_run() {
        let mut root = json!({});
        assert!(merge_claude_hooks(&mut root, SCRIPT));
        // Second run must be a no-op and must not duplicate.
        assert!(!merge_claude_hooks(&mut root, SCRIPT));
        assert_eq!(root["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(root["hooks"]["Notification"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn preserves_existing_user_config_and_hooks() {
        let mut root = json!({
            "model": "claude-opus-4-8",
            "hooks": {
                "PreToolUse": [{ "hooks": [{ "type": "command", "command": "echo pre" }] }],
                "Stop": [{ "hooks": [{ "type": "command", "command": "echo user-stop" }] }]
            }
        });
        assert!(merge_claude_hooks(&mut root, SCRIPT));
        // Untouched user keys/hooks survive.
        assert_eq!(root["model"], "claude-opus-4-8");
        // The user's own PreToolUse hook survives at its position. Ours is
        // appended beside it when the veto script exists (XNAUT-132), the same
        // way Stop works; a policy that silently replaced the user's gating
        // hook would be worse than having none.
        let pre = root["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre[0]["hooks"][0]["command"], "echo pre");
        assert_eq!(pre.len(), if veto_script_path().is_some() { 2 } else { 1 }, "{pre:?}");
        // Our Stop hook is appended alongside the user's, not replacing it.
        let stop = root["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "echo user-stop");
        assert!(stop[1]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("xnaut-hook.sh"));
    }

    #[test]
    fn does_not_touch_malformed_hooks() {
        let mut root = json!({ "hooks": "not-an-object" });
        assert!(!merge_claude_hooks(&mut root, SCRIPT));
        assert_eq!(root["hooks"], "not-an-object");
    }

    #[test]
    fn write_creates_then_is_idempotent() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let wt = std::env::temp_dir().join(format!(
            "xnaut-hooktest-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&wt);
        std::fs::create_dir_all(&wt).unwrap();
        let script = Path::new(SCRIPT);

        // First write creates the file with our hooks.
        assert!(write_claude_hooks(wt.to_str().unwrap(), script).unwrap());
        let file = wt.join(".claude").join("settings.local.json");
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(v["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("xnaut-hook.sh"));

        // Second write is a no-op (idempotent) and leaves one entry each.
        assert!(!write_claude_hooks(wt.to_str().unwrap(), script).unwrap());
        let v2: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v2["hooks"]["Stop"].as_array().unwrap().len(), 1);

        std::fs::remove_dir_all(&wt).unwrap();
    }
}
