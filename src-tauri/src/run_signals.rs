// Runtime notices for the run registry (XNAUT-296). Detector only: retirement
// policy and writer transfer live in run_control and sweep. No provider quality
// ordering is inferred. Real ANSI capture excerpts are in tests/fixtures.
use crate::run_control::{self, RunState};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Match notice lines, not a mention of an error inside a ticket or tool log.
/// Horizontal cursor positioning separates words in Claude's incremental ANSI
/// output; deleting those escapes would turn `expired Please` into one word.
pub fn notice(capture: &str, requirement: &str) -> Option<String> {
    let ansi =
        regex::Regex::new(r"\x1b\][^\x07]*(?:\x07)|\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b[()][A-Z0-9]")
            .unwrap();
    let clean = ansi.replace_all(capture, |c: &regex::Captures| {
        if c[0].ends_with('G') || c[0].ends_with('C') {
            " "
        } else if c[0].ends_with(['H', 'B', 'A']) {
            "\n"
        } else {
            ""
        }
    });
    let clean: String = clean
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    let problems = regex::Regex::new(concat!(
        r"(?i)^(?:login expired(?:\s*·?\s*please\s+run\s+/login)?",
        r"|not logged in\s*·?\s*run\s+/login",
        r"|not authenticated(?:\s*·?\s*run\s+/login)?",
        r"|rate limit(?:ed| exceeded| reached)?(?:\s*[:·].*)?",
        r"|do you trust (?:the files in this folder|this folder)\?",
        r"|trust this folder\?",
        r"|(?:allow|approve) (?:this command|this tool call|this action)\?",
        r"|do you want to proceed\?|would you like to run the following command\?)$"
    ))
    .unwrap();
    for line in clean.lines().rev() {
        let line = line
            .trim()
            .trim_start_matches(['⏺', '●', '│', '┃'])
            .trim()
            .trim_end_matches(['│', '┃'])
            .trim();
        if problems.is_match(line) {
            return Some(format!("runtime notice: {line}"));
        }
        let lower = line.to_ascii_lowercase();
        let model = lower
            .strip_prefix("model:")
            .or_else(|| lower.strip_prefix("current model:"))
            .or_else(|| lower.strip_prefix("switched to model:"));
        if let Some(model) = model {
            let model = model.trim();
            if !model.is_empty()
                && !model.contains(['`', '"'])
                && !run_control::model_meets(model, requirement)
            {
                return Some(format!("runtime model {model} does not meet {requirement}"));
            }
        }
        // Claude's status header, e.g. "Opus 4.8 · Claude Max". Translate its
        // display spelling into its model id; this is identity, not ranking.
        if let Some((display, _)) = lower.split_once(" · ") {
            if ["opus ", "sonnet ", "haiku "]
                .iter()
                .any(|p| display.starts_with(p))
                && display.split_whitespace().count() == 2
            {
                let model = format!("claude-{}", display.replace([' ', '.'], "-"));
                if !run_control::model_meets(&model, requirement) {
                    return Some(format!("runtime model {model} does not meet {requirement}"));
                }
            }
        }
    }
    None
}

pub fn capture_notice(path: &Path, requirement: &str) -> Result<Option<String>, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let start = size.saturating_sub(16 * 1024);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(16 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    // A partial first line must not turn a quoted sentence into a notice.
    let text = if start > 0 {
        text.split_once('\n').map(|(_, t)| t).unwrap_or("")
    } else {
        &text
    };
    Ok(notice(text, requirement))
}

/// Bind a reported run id to the authenticated session before accepting it.
/// Older hooks can omit run_id; the session still supplies the identity.
pub fn hook_model_in(
    dir: &Path,
    session: &str,
    run_id: Option<&str>,
    model: &str,
    tickets: &[crate::project_management::TicketRecord],
) -> Result<(), String> {
    for id in run_control::list_ids_in(dir)? {
        let run = run_control::load_manifest_in(dir, &id)?;
        if run_id.is_some_and(|wanted| wanted != id)
            || (run.pty_session.as_deref() != Some(session)
                && run.zellij_session.as_deref() != Some(session))
        {
            continue;
        }
        if run.state.terminal() || matches!(run.state, RunState::Retiring | RunState::Undead) {
            continue;
        }
        let requirement = tickets
            .iter()
            .find(|t| Some(t.id.as_str()) == run.ticket.as_deref())
            .map(|t| t.model_requirement.as_str())
            .unwrap_or_default();
        run_control::update_in(dir, &id, |r| {
            if r.state.terminal() || matches!(r.state, RunState::Retiring | RunState::Undead) {
                return;
            }
            r.model = Some(model.trim().into());
            if !run_control::model_meets(model, requirement) {
                r.state = RunState::Degraded;
                r.last_signal = format!(
                    "hook model {} does not meet {}",
                    model.trim(),
                    requirement.trim()
                );
            }
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_capture_notices_and_quoted_prose() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/runtime-notices.json")).unwrap();
        for f in fixtures.as_array().unwrap() {
            assert!(
                notice(f["capture"].as_str().unwrap(), "").is_some(),
                "{}",
                f["source"]
            );
        }
        for text in [
            "The ticket says 'Login expired', 'Not logged in', 'rate limit'.",
            "because a claude session reports \"Not logged in · Run /login\" while another works",
            "Model: `old-model` is an example in this task.",
            "Please detect this: Do you trust the files in this folder?",
        ] {
            assert_eq!(notice(text, "required"), None, "{text}");
        }
        for text in [
            "Rate limit exceeded",
            "Allow this command?",
            "Approve this tool call?",
            "Do you want to proceed?",
            "Would you like to run the following command?",
        ] {
            assert!(notice(text, "").is_some(), "{text}");
        }
        assert!(notice("Model: old-model", "required").is_some());
        assert!(notice("Opus 4.8 · Claude Max", "claude-opus-5").is_some());
        assert_eq!(notice("Opus 5 · Claude Max", "claude-opus-5"), None);
        assert_eq!(notice("Model: old-model", ""), None);
        assert_eq!(notice("Model: REQUIRED", " required "), None);
    }
    #[test]
    fn hook_requires_session_binding_and_default_empty_never_degrades() {
        let dir = run_control::tests::directory("model-hook");
        let run = run_control::request_in(&dir, run_control::tests::run(), || Ok(())).unwrap();
        let mut ticket: crate::project_management::TicketRecord = serde_json::from_value(serde_json::json!({
            "id":"XNAUT-900", "project":"XNAUT", "title":"test", "type":"bug", "status":"in_progress",
            "priority":"high", "revision":1, "created_at":"", "updated_at":""
        })).unwrap();
        assert_eq!(ticket.model_requirement, "");
        hook_model_in(
            &dir,
            "test-session",
            Some(&run.run_id),
            "old",
            &[ticket.clone()],
        )
        .unwrap();
        assert_ne!(
            run_control::load_manifest_in(&dir, &run.run_id)
                .unwrap()
                .state,
            RunState::Degraded
        );
        ticket.model_requirement = "required".into();
        hook_model_in(
            &dir,
            "somebody-else",
            Some(&run.run_id),
            "old",
            &[ticket.clone()],
        )
        .unwrap();
        hook_model_in(
            &dir,
            "test-session",
            Some("another-run"),
            "old",
            &[ticket.clone()],
        )
        .unwrap();
        assert_ne!(
            run_control::load_manifest_in(&dir, &run.run_id)
                .unwrap()
                .state,
            RunState::Degraded
        );
        hook_model_in(&dir, "test-session", Some(&run.run_id), "old", &[ticket]).unwrap();
        run_control::signal_session_in(&dir, "test-session", Some(RunState::Running), None, 3000)
            .unwrap();
        run_control::reconcile_in(&dir, 4000, |_| run_control::tests::proof()).unwrap();
        let run = run_control::load_manifest_in(&dir, &run.run_id).unwrap();
        assert_eq!(run.state, RunState::Degraded);
        assert!(run.last_signal.contains("old does not meet required"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
