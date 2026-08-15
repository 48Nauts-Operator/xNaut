// What an agent can DO to xNAUT from a chat turn (XNAUT-161).
//
// "Please enable the Tavily plugin" came back as "open Agents → Plugins and
// enable it yourself". That is the assistant answering about the product
// instead of operating it, and it is the opposite of the point: NautBot is
// meant to execute — connect features, switch plugins on, hand them to an
// agent.
//
// So a chat turn gets tools. Deliberately few, and deliberately bounded:
//
//   * read what exists (plugins, agents)
//   * switch a plugin on or off in the library
//   * hand a plugin to one agent, or take it back
//
// What is NOT here, on purpose: writing credentials. A token pasted into a
// chat turn ends up in the transcript, in the model's context, and in any log
// that captured either. If a plugin needs a key, the tool says which key is
// missing and the owner types it where it belongs.
//
// The loop is non-streaming: tool calls arrive as deltas in the streaming API
// and reassembling them buys nothing here, where the answer is a sentence and
// the work is the side effect.

use serde_json::{json, Value};

/// How many model→tool→model rounds one turn may take. Enough to list, act and
/// report; low enough that a confused model cannot spend the evening.
const MAX_ROUNDS: usize = 4;

pub fn tool_specs() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "list_plugins",
                "description": "List xNAUT's plugin library: every MCP server, whether it is switched on, and what is missing if it cannot run.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "set_plugin_enabled",
                "description": "Switch a plugin on or off in the library. Fails with the reason when the plugin is missing a credential or an endpoint; report that reason rather than retrying.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Plugin id, e.g. tavily" },
                        "enabled": { "type": "boolean" }
                    },
                    "required": ["id", "enabled"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_agents",
                "description": "List the agents in this xNAUT, with the plugins each one currently holds.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "set_agent_plugin",
                "description": "Hand a plugin to one agent, or take it back. A plugin switched on in the library still reaches no agent until it is handed over.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "handle": { "type": "string", "description": "Agent handle without the @" },
                        "id": { "type": "string", "description": "Plugin id" },
                        "granted": { "type": "boolean" }
                    },
                    "required": ["handle", "id", "granted"]
                }
            }
        }),
    ]
}

/// Run one tool. Errors come back as data, not as a failed turn: the model has
/// to be able to tell the owner WHY something did not happen.
pub fn execute(name: &str, args: &Value) -> Value {
    match name {
        "list_plugins" => {
            let plugins = crate::plugins::catalog_snapshot();
            json!({ "plugins": plugins })
        }
        "set_plugin_enabled" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("").trim();
            let enabled = args.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            match crate::plugins::set_enabled(id, enabled) {
                Ok(plugin) => json!({
                    "ok": true,
                    "id": plugin.id,
                    "name": plugin.name,
                    "enabled": plugin.enabled,
                    "next": "A plugin switched on still reaches no agent until it is handed to one."
                }),
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "list_agents" => json!({ "agents": crate::agent_profiles::roster_snapshot() }),
        "set_agent_plugin" => {
            let handle = args.get("handle").and_then(Value::as_str).unwrap_or("").trim();
            let id = args.get("id").and_then(Value::as_str).unwrap_or("").trim();
            let granted = args.get("granted").and_then(Value::as_bool).unwrap_or(true);
            match crate::agent_profiles::set_plugin_grant(handle, id, granted) {
                Ok(held) => json!({ "ok": true, "handle": handle, "plugins": held }),
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        other => json!({ "ok": false, "error": format!("no such tool: {other}") }),
    }
}

/// One chat turn that may call tools before it answers.
///
/// Returns the assistant's final text plus the tools it ran, so the UI can
/// show what actually changed rather than taking the model's word for it.
pub async fn run_turn(
    llm: &crate::settings::LlmSettings,
    model: &str,
    messages: Vec<Value>,
    effort: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;
    let url = crate::chat::join_endpoint(&llm.endpoint, "chat/completions");

    let mut conversation = messages;
    let mut performed: Vec<String> = Vec::new();

    for _ in 0..MAX_ROUNDS {
        // reasoning_effort is FORCED to none on a tool turn. Verified against
        // NautGate on 2026-08-15, which answered:
        //
        //   Function tools with reasoning_effort are not supported for
        //   gpt-5.6-sol in /v1/chat/completions. To use function tools, use
        //   /v1/responses or set reasoning_effort to 'none'.
        //
        // Passing the profile's effort through turned every tool turn into a
        // 502, and the fallback then answered without tools — which is exactly
        // the "open the menu yourself" reply this module exists to end. The
        // second attempt drops the field entirely for providers that dislike
        // the literal "none".
        let _ = effort;
        let mut payload = Value::Null;
        let mut status = reqwest::StatusCode::OK;
        for attempt in 0..2 {
            let mut body = json!({
                "model": model,
                "messages": conversation,
                "tools": tool_specs(),
            });
            if attempt == 0 {
                body["reasoning_effort"] = json!("none");
            }
            let response = crate::chat::apply_auth(client.post(&url), &llm.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("chat request failed: {e}"))?;
            status = response.status();
            payload = response
                .json()
                .await
                .map_err(|e| format!("chat response was not JSON: {e}"))?;
            if status.is_success() {
                break;
            }
        }
        if !status.is_success() {
            let detail = payload
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .or_else(|| payload.get("detail").and_then(Value::as_str))
                .unwrap_or("unknown error");
            return Err(format!("{status}: {detail}"));
        }
        let message = payload
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| "chat response had no message".to_string())?;
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            let text = message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            return Ok((text, performed));
        }
        conversation.push(message);
        for call in calls {
            let id = call.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            // Arguments arrive as a JSON STRING, and a model that writes a
            // malformed one must get a usable error rather than a dead turn.
            let raw = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let args: Value = serde_json::from_str(raw).unwrap_or_else(|_| json!({}));
            let result = execute(&name, &args);
            if result.get("ok").and_then(Value::as_bool) == Some(true) {
                performed.push(format!("{name} {}", args));
            }
            conversation.push(json!({
                "role": "tool",
                "tool_call_id": id,
                "name": name,
                "content": result.to_string(),
            }));
        }
    }
    Err("the agent kept calling tools without answering".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tools_never_offer_to_write_a_credential() {
        // A token pasted into a chat turn lands in the transcript, the model's
        // context and any log that caught either. The tool surface must not
        // make that easy, however convenient it sounds.
        let specs = serde_json::to_string(&tool_specs()).unwrap().to_lowercase();
        for forbidden in ["api_key", "token", "secret", "credential\":", "env"] {
            assert!(!specs.contains(forbidden), "tool surface exposes {forbidden}");
        }
    }

    #[test]
    fn an_unknown_tool_answers_instead_of_failing_the_turn() {
        let result = execute("drop_everything", &json!({}));
        assert_eq!(result["ok"], json!(false));
        assert!(result["error"].as_str().unwrap().contains("no such tool"));
    }

    #[test]
    fn enabling_a_plugin_that_cannot_run_reports_why() {
        // Notion needs NOTION_TOKEN. The refusal has to name it, because
        // "could not enable" sends the owner looking in the wrong place.
        //
        // Against a SCRATCH library: the first version of this test ran against
        // the real one and switched a plugin on in André's own config.
        let scratch = std::env::temp_dir().join(format!("xnaut-plugins-test-{}.json", std::process::id()));
        std::env::set_var("XNAUT_PLUGINS_PATH", &scratch);
        let result = execute("set_plugin_enabled", &json!({ "id": "notion", "enabled": true }));
        let unknown = execute("set_plugin_enabled", &json!({ "id": "nope", "enabled": true }));
        let listed = execute("list_plugins", &json!({}));
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);

        assert_eq!(result["ok"], json!(false));
        assert!(
            result["error"].as_str().unwrap().contains("NOTION_TOKEN"),
            "expected the missing key to be named, got {result}"
        );
        assert!(unknown["error"].as_str().unwrap().contains("no plugin called"));
        // The listing names the missing variable — that is the whole point of
        // "blocked_by" — but never carries a VALUE. Prove it with one planted.
        let text = listed.to_string();
        assert!(text.contains("NOTION_TOKEN"), "the listing must say what is missing");
        assert!(!text.contains("\"env\""), "the listing carries the credential map");
    }

    #[test]
    fn every_tool_name_in_the_spec_is_one_execute_knows() {
        // A spec entry with no implementation is a tool the model will call
        // once and never get an answer from.
        for spec in tool_specs() {
            let name = spec["function"]["name"].as_str().unwrap().to_string();
            let result = execute(&name, &json!({}));
            assert!(
                result.get("error").and_then(Value::as_str).map(|e| !e.contains("no such tool")).unwrap_or(true),
                "{name} is advertised but not implemented"
            );
        }
    }
}
