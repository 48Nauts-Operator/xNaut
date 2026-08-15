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

/// How many model→tool→model rounds one turn may take.
///
/// Measured, not guessed: repairing the Forgejo connector took eight —
/// connect, inspect, search, repair, connect, inspect, search, repair — and
/// the model had the right package (gitea-mcp) at exactly the round the old
/// cap of 8 cut it off, so the turn failed one step before succeeding. Enough
/// room for two full diagnose-and-retry cycles, and still bounded.
const MAX_ROUNDS: usize = 14;

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
                "name": "connect_plugin",
                "description": "Add a plugin: find any credential xNAUT already holds, switch it on, PROVE it starts, and hand it to an agent. Use this for 'add X' or 'connect X' — do not describe the menu. It only comes back with a question when a credential genuinely cannot be found.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "plugin": { "type": "string", "description": "Which plugin, by id or name — forgejo, Forgejo and Google Calendar all work." },
                        "agent_handle": { "type": "string", "description": "Agent to hand it to, without the @. Optional." }
                    },
                    "required": ["plugin"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "inspect_package",
                "description": "Look up an npm package before or after pointing a plugin at it: does it exist, does it ship an executable npx can run, what version, which repository. Use this when a connector fails with 'could not determine executable to run'.",
                "parameters": {
                    "type": "object",
                    "properties": { "name": { "type": "string", "description": "npm package name" } },
                    "required": ["name"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "search_packages",
                "description": "Search npm for an MCP server, with whether each result can actually be run by npx. Use it to find a working replacement when a plugin's package turns out to be unrunnable.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "limit": { "type": "integer" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "repair_plugin",
                "description": "Point a plugin at a different command and arguments, then connect it again. This is how you FIX a broken connector instead of reporting it: diagnose with inspect_package or search_packages, repair, and retry connect_plugin.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "plugin": { "type": "string" },
                        "command": { "type": "string", "description": "Usually npx" },
                        "args": { "type": "array", "items": { "type": "string" }, "description": "For example [\"-y\", \"gitea-mcp\"]" }
                    },
                    "required": ["plugin", "command", "args"]
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
pub async fn execute(name: &str, args: &Value) -> Value {
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
        "inspect_package" => {
            let name = args.get("name").and_then(Value::as_str).unwrap_or("").trim();
            match crate::plugins::npm_package(name).await {
                Ok(info) => info,
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "search_packages" => {
            let query = args.get("query").and_then(Value::as_str).unwrap_or("").trim();
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(5) as usize;
            match crate::plugins::npm_search(query, limit).await {
                Ok(results) => results,
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "repair_plugin" => {
            let named = args.get("plugin").and_then(Value::as_str).unwrap_or("");
            let command = args.get("command").and_then(Value::as_str).unwrap_or("npx").trim().to_string();
            let list: Vec<String> = args
                .get("args")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let Some(id) = crate::plugins::resolve_id(named) else {
                return json!({ "ok": false, "error": format!("no plugin called {named:?} in the library") });
            };
            match crate::plugins::set_command(&id, &command, list) {
                Ok(plugin) => json!({
                    "ok": true,
                    "id": plugin.id,
                    "command": format!("{} {}", plugin.command, plugin.args.join(" ")),
                    "next": "Call connect_plugin again to prove it starts."
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
        "connect_plugin" => {
            // A model reaches for whichever key name it remembers, and once
            // put the plugin's name in "handle". Take the plugin from any of
            // them and resolve it the way a person would say it, rather than
            // answering "no such plugin" to a request that was perfectly clear.
            let named = ["plugin", "id", "name", "handle"]
                .iter()
                .filter_map(|key| args.get(*key).and_then(Value::as_str))
                .map(str::trim)
                .find(|value| !value.is_empty())
                .unwrap_or("");
            let Some(id) = crate::plugins::resolve_id(named) else {
                return json!({ "ok": false, "error": format!("no plugin called {named:?} in the library") });
            };
            let handle = ["agent_handle", "handle", "agent"]
                .iter()
                .filter_map(|key| args.get(*key).and_then(Value::as_str))
                .map(str::trim)
                .find(|value| !value.is_empty() && crate::plugins::resolve_id(value).is_none())
                .unwrap_or("")
                .to_string();
            match crate::plugins::connect(&id).await {
                Ok(mut report) => {
                    if !handle.is_empty() {
                        match crate::agent_profiles::set_plugin_grant(&handle, &id, true) {
                            Ok(held) => {
                                report["handed_to"] = json!(handle);
                                report["agent_now_holds"] = json!(held);
                            }
                            Err(error) => report["handed_to_error"] = json!(error),
                        }
                    }
                    report
                }
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
    // Every call, not just the ones that worked: a turn that runs out of
    // rounds has to be able to say what it was busy doing.
    let mut attempted: Vec<String> = Vec::new();

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
            let result = execute(&name, &args).await;
            attempted.push(format!("{name} {}", args));
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
    Err(format!(
        "the agent kept calling tools without answering: {}",
        attempted.join(" | ")
    ))
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

    #[tokio::test]
    #[ignore = "talks to the live model and starts a real server; run with --ignored"]
    async fn the_agent_repairs_a_broken_connector_instead_of_reporting_it() {
        // André's exact failure: "Forgejo could not start: the MCP server
        // exited because npm could not determine the executable to run."
        // forgejo-mcp ships no bin. A person would look it up and find one
        // that does; so must the agent.
        let scratch = std::env::temp_dir().join("xnaut-plugins-repair-test.json");
        let _ = std::fs::remove_file(&scratch);
        std::env::set_var("XNAUT_PLUGINS_PATH", &scratch);
        // Break it the way it was broken, and make the breakage STICK: saving
        // through plugin_save marks the entry as the owner's, so the seed
        // refresh does not quietly heal it before the agent gets a look. That
        // refresh is a real feature — it is why this had to be forced.
        let mut broken = crate::plugins::catalog_snapshot()
            .into_iter()
            .find(|plugin| plugin["id"] == json!("forgejo"))
            .map(|_| crate::plugins::seed().into_iter().find(|p| p.id == "forgejo").unwrap())
            .expect("forgejo is seeded");
        broken.command = "npx".into();
        broken.args = vec!["-y".into(), "forgejo-mcp".into()];
        crate::plugins::plugin_save(broken).expect("save the broken entry");

        let settings = crate::settings::load_or_default();
        let llm = crate::chat::provider_llm(&settings, "nautgate").expect("nautgate configured");
        let system = format!(
            "You are NautBot (@nautbot), one of the agents in xNAUT.\n\n{}",
            crate::composer::CHAT_RULES
        );
        let messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": "Add the Forgejo plugin" }),
        ];
        let result = run_turn(&llm, "gpt-5.6-sol", messages, None).await;
        let store: Value = serde_json::from_str(&std::fs::read_to_string(&scratch).unwrap()).unwrap();
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);

        let (text, did) = result.expect("the turn should finish");
        println!("agent said: {text}\ntools run: {did:?}");
        let forgejo = store["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|plugin| plugin["id"] == json!("forgejo"))
            .unwrap()
            .clone();
        println!("forgejo now: {} {:?} enabled={}", forgejo["command"], forgejo["args"], forgejo["enabled"]);
        assert!(
            did.iter().any(|call| call.starts_with("repair_plugin")),
            "the agent never repaired anything: {did:?}"
        );
        assert_eq!(forgejo["enabled"], json!(true), "it did not end up connected");
    }

    #[tokio::test]
    async fn the_tool_specs_are_dumped_for_the_live_probe() {
        // Printed so a probe against the real model sends exactly what the app
        // sends. Hand-rewriting the schema for a probe is how a check passes
        // while the product still fails.
        if std::env::var("XNAUT_DUMP_TOOLS").is_ok() {
            println!("{}", serde_json::to_string(&tool_specs()).unwrap());
        }
    }

    #[tokio::test]
    async fn an_unknown_tool_answers_instead_of_failing_the_turn() {
        let result = execute("drop_everything", &json!({})).await;
        assert_eq!(result["ok"], json!(false));
        assert!(result["error"].as_str().unwrap().contains("no such tool"));
    }

    #[tokio::test]
    async fn enabling_a_plugin_that_cannot_run_reports_why() {
        // Notion needs NOTION_TOKEN. The refusal has to name it, because
        // "could not enable" sends the owner looking in the wrong place.
        //
        // Against a SCRATCH library: the first version of this test ran against
        // the real one and switched a plugin on in André's own config.
        let scratch = std::env::temp_dir().join(format!("xnaut-plugins-test-{}.json", std::process::id()));
        std::env::set_var("XNAUT_PLUGINS_PATH", &scratch);
        let result = execute("set_plugin_enabled", &json!({ "id": "notion", "enabled": true })).await;
        let unknown = execute("set_plugin_enabled", &json!({ "id": "nope", "enabled": true })).await;
        let listed = execute("list_plugins", &json!({})).await;
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

    #[tokio::test]
    async fn every_tool_name_in_the_spec_is_one_execute_knows() {
        // A spec entry with no implementation is a tool the model will call
        // once and never get an answer from.
        for spec in tool_specs() {
            let name = spec["function"]["name"].as_str().unwrap().to_string();
            let result = execute(&name, &json!({})).await;
            assert!(
                result.get("error").and_then(Value::as_str).map(|e| !e.contains("no such tool")).unwrap_or(true),
                "{name} is advertised but not implemented"
            );
        }
    }
}
