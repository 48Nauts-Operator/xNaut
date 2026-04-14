// ABOUTME: Trigger system for pattern matching terminal output and executing actions.
// ABOUTME: Supports regex patterns with notifications, command execution, and AI assistance triggers.

use crate::state::{AppState, Trigger, TriggerAction};
use anyhow::Result;
use regex::Regex;
use tauri::{AppHandle, Emitter};

/// Processes terminal output against all active triggers
pub async fn process_output(
    app: AppHandle,
    state: &AppState,
    session_id: &str,
    output: &str,
) -> Result<()> {
    let triggers = state.triggers.lock().await;

    for trigger in triggers.values() {
        if !trigger.enabled {
            continue;
        }

        // Try to match pattern
        if let Ok(regex) = Regex::new(&trigger.pattern) {
            if regex.is_match(output) {
                // Execute trigger action
                execute_trigger_action(
                    app.clone(),
                    session_id,
                    trigger,
                    output,
                )
                .await?;
            }
        }
    }

    Ok(())
}

/// Executes a trigger action
async fn execute_trigger_action(
    app: AppHandle,
    session_id: &str,
    trigger: &Trigger,
    matched_output: &str,
) -> Result<()> {
    match &trigger.action {
        TriggerAction::Notify { message } => {
            // Send notification to frontend
            let _ = app.emit(
                "trigger-notification",
                serde_json::json!({
                    "triggerId": trigger.id,
                    "sessionId": session_id,
                    "message": message,
                    "matchedOutput": matched_output,
                }),
            );
        }
        TriggerAction::RunCommand { command } => {
            // Emit command execution request
            let _ = app.emit(
                "trigger-command",
                serde_json::json!({
                    "triggerId": trigger.id,
                    "sessionId": session_id,
                    "command": command,
                }),
            );
        }
        TriggerAction::AiAssist { prompt } => {
            // Request AI assistance
            let _ = app.emit(
                "trigger-ai-assist",
                serde_json::json!({
                    "triggerId": trigger.id,
                    "sessionId": session_id,
                    "prompt": prompt,
                    "context": matched_output,
                }),
            );
        }
    }

    Ok(())
}

/// Common trigger patterns
pub mod patterns {
    pub const ERROR: &str = r"(?i)(error|fail|failed|exception)";
    pub const WARNING: &str = r"(?i)(warning|warn)";
    pub const SUCCESS: &str = r"(?i)(success|successful|completed|done)";
    pub const PERMISSION_DENIED: &str = r"(?i)permission denied";
    pub const NOT_FOUND: &str = r"(?i)(not found|no such file)";
    pub const CONNECTION_ERROR: &str = r"(?i)(connection refused|timeout|unreachable)";
    pub const SYNTAX_ERROR: &str = r"(?i)syntax error";
    pub const SEGFAULT: &str = r"(?i)segmentation fault";
}

/// Creates common helpful triggers
pub fn create_default_triggers() -> Vec<Trigger> {
    vec![
        Trigger {
            id: "error-notifier".to_string(),
            pattern: patterns::ERROR.to_string(),
            action: TriggerAction::Notify {
                message: "Error detected in terminal output".to_string(),
            },
            enabled: true,
        },
        Trigger {
            id: "permission-helper".to_string(),
            pattern: patterns::PERMISSION_DENIED.to_string(),
            action: TriggerAction::AiAssist {
                prompt: "How do I fix this permission error?".to_string(),
            },
            enabled: true,
        },
        Trigger {
            id: "success-notifier".to_string(),
            pattern: patterns::SUCCESS.to_string(),
            action: TriggerAction::Notify {
                message: "Command completed successfully".to_string(),
            },
            enabled: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_pattern_matching() {
        let regex = Regex::new(patterns::ERROR).unwrap();
        assert!(regex.is_match("Error: file not found"));
        assert!(regex.is_match("Operation failed"));
        assert!(!regex.is_match("Everything is fine"));
    }

    #[test]
    fn test_permission_pattern() {
        let regex = Regex::new(patterns::PERMISSION_DENIED).unwrap();
        assert!(regex.is_match("Permission denied: /etc/passwd"));
        assert!(!regex.is_match("Access granted"));
    }

    #[test]
    fn test_default_triggers_creation() {
        let triggers = create_default_triggers();
        assert!(triggers.len() > 0);
        assert!(triggers.iter().any(|t| t.id == "error-notifier"));
    }
}
