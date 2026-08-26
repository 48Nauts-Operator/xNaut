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
                "name": "start_local_service",
                "description": "Start a plugin's local server when its endpoint is unreachable — today that is Excalidraw, whose canvas and MCP endpoint run on 127.0.0.1:3001. Use it instead of reporting that a local plugin will not connect.",
                "parameters": {
                    "type": "object",
                    "properties": { "plugin": { "type": "string" } },
                    "required": ["plugin"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_canvas",
                "description": "Read your canvas: the diagram the owner is looking at, as nodes and edges. The visible graph is authoritative — read it before changing it.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "update_canvas",
                "description": "Draw on your canvas. Send the COMPLETE graph you want, not only the changed box — boxes the owner has moved keep their positions, and new ones are placed for you. This is how you answer 'draw me a diagram': no repository, no build.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "nodes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string", "description": "Stable id — reuse it and the box keeps its place." },
                                    "kind": { "type": "string", "enum": ["actor", "component", "service", "agent", "data-store", "external-system", "decision", "document", "note", "trust-boundary"] },
                                    "label": { "type": "string" },
                                    "description": { "type": "string" }
                                },
                                "required": ["id", "label"]
                            }
                        },
                        "edges": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "source": { "type": "string" },
                                    "target": { "type": "string" },
                                    "label": { "type": "string" }
                                },
                                "required": ["id", "source", "target"]
                            }
                        }
                    },
                    "required": ["nodes"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "write_document",
                "description": "Write a document — a report, a spec, release notes, a plan — beside the conversation. Markdown. Send the COMPLETE document, not a fragment: this replaces what is there, and the previous version is kept for one undo. Use it instead of pasting a long answer into the chat.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "content": { "type": "string", "description": "Markdown body, without the title heading." }
                    },
                    "required": ["content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_document",
                "description": "Read the document currently beside the conversation, so an edit rewrites what is actually there.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "vault_search",
                "description": "Search the work vault (~/.xnaut-vault) for notes. Titles rank above bodies. Use this before writing, so an answer builds on what is already written down.",
                "parameters": {
                    "type": "object",
                    "properties": { "query": { "type": "string" }, "limit": { "type": "integer" } },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "vault_read",
                "description": "Read one note from the vault by its path, as returned by vault_search.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "vault_write",
                "description": "Write a note into the vault. Markdown only, path relative to the vault root, e.g. work/xNAUT/Development/features/2026-08-16_Title.md. Frontmatter is added or its Last modified bumped for you.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" }, "content": { "type": "string" } },
                    "required": ["path", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "vault_recent",
                "description": "The most recently modified notes in the vault, newest first. Use this for 'what was the last document added', which is a question about time and cannot be answered by searching text.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer" },
                        "prefix": { "type": "string", "description": "Narrow to a folder, e.g. work/xnaut" }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "show_note",
                "description": "Open a vault note in the owner's split pane so he can read it beside the conversation. Use this instead of pasting a long note into the chat.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string", "description": "Vault-relative path, as returned by vault_search or vault_recent" } },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "open_graph",
                "description": "Open the knowledge-graph view — the vault's notes and the links between them — in a tab. Use it when asked to show the graph.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "create_agent",
                "description": "Create a new agent in this xNAUT: a handle, a name, and what it is for. This is an xNAUT action, not a coding task — never open a repository or a worktree to do it.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "handle": { "type": "string", "description": "Without the @, e.g. fronti" },
                        "display_name": { "type": "string", "description": "e.g. Frontend Developer" },
                        "tagline": { "type": "string", "description": "One line shown under the name." },
                        "purpose": { "type": "string", "description": "Its instructions: what it is for and how it should work." },
                        "runtime": { "type": "string", "description": "claude, codex, gemini… Omit to use the same runtime as NautBot." },
                        "model": { "type": "string" }
                    },
                    "required": ["handle", "display_name"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_sessions",
                "description": "The live zellij sessions on this machine, including ones started outside xNAUT. Use it to find work that is already running before starting anything new.",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "attach_session",
                "description": "Attach to a live zellij session by name and open it as a tab, so its work continues in view. This ATTACHES a viewport; it does not type into the session.",
                "parameters": {
                    "type": "object",
                    "properties": { "name": { "type": "string" } },
                    "required": ["name"]
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
        json!({
            "type": "function",
            "function": {
                "name": "list_tickets",
                "description": "List Project Management tickets, newest change first. Use this before touching one: it is how you learn a ticket's real status, owner and body rather than guessing.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "project": { "type": "string", "description": "Project key such as XNAUT. Omit for every project." },
                        "status": { "type": "string", "description": "Only tickets in this status: inbox, ready, in_progress, review, blocked, done or complete." },
                        "limit": { "type": "integer", "description": "Default 20." }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "create_ticket",
                "description": "File a new ticket. Writes the ticket JSON, an event and a git commit, exactly as the app does. Use it for work that outlives this conversation; do not file a duplicate of something list_tickets already shows.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "project": { "type": "string", "description": "Project key such as XNAUT" },
                        "title": { "type": "string" },
                        "body": { "type": "string", "description": "What the work is, why, and how to tell it is done." },
                        "ticket_type": { "type": "string", "description": "idea, feature, bug, incident or task. Default feature." },
                        "priority": { "type": "string", "description": "low, medium, high or critical. Default medium." },
                        "status": { "type": "string", "description": "Default inbox." }
                    },
                    "required": ["project", "title"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "update_ticket",
                "description": "Change a ticket's status, priority or owner, and append to its body. The body is only ever APPENDED to, so a ticket's history cannot be overwritten. Set done when the work is actually finished: the ticket goes back to NautBot, who tests it and is the only one who can set complete.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Ticket id such as XNAUT-173" },
                        "status": { "type": "string", "description": "inbox, ready, in_progress, review, blocked or done. Set done when the work is finished, which hands the ticket back to NautBot. Only NautBot can set complete (tested, checked, approved)." },
                        "priority": { "type": "string" },
                        "owner": { "type": "string", "description": "Agent handle or name taking the ticket." },
                        "append_body": { "type": "string", "description": "Appended under the existing body, never replacing it." }
                    },
                    "required": ["id"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "wake_agent",
                "description": "Nudge an agent to check its assigned tickets. Types a short wake-up line into that agent's idle session; the tickets themselves carry the work. Only NautBot wakes agents. Assign the ticket first (update_ticket with owner and status ready), then wake.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "handle": { "type": "string", "description": "Agent handle such as claudi." },
                        "message": { "type": "string", "description": "Optional wake-up line. Default: Check your tickets." }
                    },
                    "required": ["handle"]
                }
            }
        }),
    ]
}

/// A ticket body with something added under it.
///
/// An agent gets APPEND, never replace: a ticket carries the history of the
/// decision, and a model asked to "update the body" will happily hand back a
/// tidied version with everything before it gone.
fn appended_body(current: &str, added: &str) -> String {
    if current.trim().is_empty() {
        added.to_string()
    } else {
        format!("{}\n\n{}", current.trim_end(), added)
    }
}

/// Run one tool. Errors come back as data, not as a failed turn: the model has
/// to be able to tell the owner WHY something did not happen.
pub async fn execute(name: &str, args: &Value, canvas_key: &str) -> Value {
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
        "start_local_service" => {
            let named = args.get("plugin").and_then(Value::as_str).unwrap_or("");
            match crate::plugins::resolve_id(named).as_deref() {
                Some("excalidraw") => match crate::mcp::mcp_start_local_excalidraw().await {
                    Ok(message) => json!({ "ok": true, "message": message, "canvas": "http://127.0.0.1:3001/" }),
                    Err(error) => json!({ "ok": false, "error": error }),
                },
                Some(other) => json!({
                    "ok": false,
                    "error": format!("{other} has no local server xNAUT can start; it runs wherever its URL points")
                }),
                None => json!({ "ok": false, "error": format!("no plugin called {named:?} in the library") }),
            }
        }
        "read_canvas" => {
            let canvas = crate::canvas::load(canvas_key);
            json!({ "ok": true, "title": canvas.title, "nodes": canvas.nodes, "edges": canvas.edges })
        }
        "update_canvas" => {
            let next: crate::canvas::Canvas = match serde_json::from_value(args.clone()) {
                Ok(canvas) => canvas,
                Err(error) => return json!({ "ok": false, "error": format!("that is not a graph I can draw: {error}") }),
            };
            match crate::canvas::update(canvas_key, next, crate::canvas::now_iso()) {
                Ok(saved) => json!({
                    "ok": true,
                    "drawn": saved.nodes.len(),
                    "edges": saved.edges.len(),
                    "note": "It is on the owner's canvas now. Say what you drew in one line."
                }),
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "read_document" => {
            let document = crate::canvas::load_document(canvas_key);
            json!({ "ok": true, "title": document.title, "content": document.content })
        }
        "write_document" => {
            let document = crate::canvas::Document {
                title: args.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
                content: args.get("content").and_then(Value::as_str).unwrap_or("").to_string(),
                ..Default::default()
            };
            if document.content.trim().is_empty() {
                return json!({ "ok": false, "error": "a document needs content" });
            }
            match crate::canvas::write_document(canvas_key, document, crate::canvas::now_iso()) {
                Ok(saved) => json!({
                    "ok": true,
                    "title": saved.title,
                    "words": saved.content.split_whitespace().count(),
                    "note": "It is open beside the conversation. Say what you wrote in one line."
                }),
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "vault_search" => {
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(8) as usize;
            match crate::vault_tools::search(query, limit) {
                Ok(value) => value,
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "vault_read" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            match crate::vault_tools::read(path) {
                Ok(value) => value,
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "vault_write" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            let content = args.get("content").and_then(Value::as_str).unwrap_or("");
            match crate::vault_tools::write(path, content, canvas_key) {
                Ok(value) => value,
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "vault_recent" => {
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize;
            let prefix = args.get("prefix").and_then(Value::as_str).unwrap_or("");
            match crate::vault_tools::recent(limit, prefix) {
                Ok(value) => value,
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "show_note" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            match crate::vault_tools::read(path) {
                Ok(note) => {
                    let content = note["content"].as_str().unwrap_or("").to_string();
                    let title = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md").to_string();
                    match crate::canvas::write_document(
                        canvas_key,
                        crate::canvas::Document { title, content, ..Default::default() },
                        crate::canvas::now_iso(),
                    ) {
                        Ok(saved) => json!({ "ok": true, "shown": path, "title": saved.title }),
                        Err(error) => json!({ "ok": false, "error": error }),
                    }
                }
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "open_graph" => json!({ "ok": true, "opened": "graph", "note": "The graph view is open in a tab." }),
        "create_agent" => {
            let handle = args.get("handle").and_then(Value::as_str).unwrap_or("");
            let display_name = args.get("display_name").and_then(Value::as_str).unwrap_or(handle);
            let tagline = args.get("tagline").and_then(Value::as_str).unwrap_or("");
            let purpose = args.get("purpose").and_then(Value::as_str).unwrap_or("");
            let runtime = args.get("runtime").and_then(Value::as_str);
            let model = args.get("model").and_then(Value::as_str);
            match crate::agent_profiles::create_profile_from(handle, display_name, tagline, purpose, runtime, model) {
                Ok(profile) => json!({
                    "ok": true,
                    "handle": profile.handle,
                    "name": profile.display_name,
                    "runtime": profile.runtime_id,
                    "note": "It is in the roster now. Say so in one line."
                }),
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "list_sessions" => {
            let sessions = tokio::task::spawn_blocking(crate::zellij::list_live_sessions)
                .await
                .unwrap_or_default();
            json!({
                "ok": true,
                "sessions": sessions
                    .iter()
                    .map(|name| json!({
                        "name": name,
                        "started_by_xnaut": name.starts_with("xnaut-"),
                    }))
                    .collect::<Vec<_>>(),
            })
        }
        "attach_session" => {
            let name = args.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
            let probe = name.clone();
            let live = tokio::task::spawn_blocking(move || {
                crate::zellij::list_live_sessions().iter().any(|item| item == &probe)
            })
            .await
            .unwrap_or(false);
            if !live {
                return json!({ "ok": false, "error": format!("no live session called {name:?}") });
            }
            json!({ "ok": true, "attach": name, "note": "Opening it as a tab. Watch it; do not type into a session the owner is using." })
        }
        // --- Project Management (XNAUT-173 step 1) ---------------------------
        // The agent writes tickets through the same functions the app's own
        // commands use, so a ticket it files is indistinguishable from one a
        // person filed: JSON + event + commit, via record_mutation.
        "list_tickets" => {
            let repo = match crate::project_management::repo_now() {
                Ok(repo) => repo,
                Err(error) => return json!({ "ok": false, "error": error }),
            };
            let project = args.get("project").and_then(Value::as_str).map(str::to_string);
            let wanted = args.get("status").and_then(Value::as_str).unwrap_or("").trim().to_string();
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
            match crate::project_management::ticket_list_in(&repo, project) {
                Ok(tickets) => {
                    let rows: Vec<Value> = tickets
                        .into_iter()
                        .filter(|t| wanted.is_empty() || t.status == wanted)
                        .take(limit)
                        // The body is the expensive field and most of a listing
                        // is scanned, not read. Ask for one ticket by id to see it.
                        .map(|t| json!({
                            "id": t.id,
                            "title": t.title,
                            "status": t.status,
                            "priority": t.priority,
                            "type": t.ticket_type,
                            "owner": t.owner,
                            "updated_at": t.updated_at,
                            "body": t.body,
                        }))
                        .collect();
                    json!({ "ok": true, "count": rows.len(), "tickets": rows })
                }
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "create_ticket" => {
            let repo = match crate::project_management::repo_now() {
                Ok(repo) => repo,
                Err(error) => return json!({ "ok": false, "error": error }),
            };
            let request = crate::project_management::TicketCreateRequest {
                project: args.get("project").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                title: args.get("title").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                ticket_type: args.get("ticket_type").and_then(Value::as_str).unwrap_or("feature").to_string(),
                status: args.get("status").and_then(Value::as_str).unwrap_or("inbox").to_string(),
                priority: args.get("priority").and_then(Value::as_str).unwrap_or("medium").to_string(),
                owner: args.get("owner").and_then(Value::as_str).map(str::to_string),
                documentation: Vec::new(),
                body: args.get("body").and_then(Value::as_str).unwrap_or("").to_string(),
            };
            match crate::project_management::ticket_create_in(&repo, request) {
                Ok(ticket) => json!({ "ok": true, "id": ticket.id, "status": ticket.status, "title": ticket.title }),
                Err(error) => json!({ "ok": false, "error": error }),
            }
        }
        "wake_agent" => {
            // The mirror of `complete`: waking workers is the orchestrator's
            // move. An agent that could wake other agents is a loop with no
            // human in it.
            let is_nautbot = canvas_key
                .trim()
                .eq_ignore_ascii_case(crate::agent_profiles::RESERVED_NAUTBOT_HANDLE);
            if !is_nautbot {
                return json!({
                    "ok": false,
                    "error": "only NautBot wakes agents. Finish your own tickets and set them to done; NautBot picks it up from there."
                });
            }
            let handle = args.get("handle").and_then(Value::as_str).unwrap_or("").trim();
            if handle.is_empty() {
                return json!({ "ok": false, "error": "handle is required" });
            }
            let message = args
                .get("message")
                .and_then(Value::as_str)
                .filter(|m| !m.trim().is_empty())
                .unwrap_or("Check your tickets.");
            match crate::nudge::app() {
                None => json!({ "ok": false, "error": "the app is not running, so no session can be nudged" }),
                Some(app) => match crate::nudge::nudge_agent(app, handle, message).await {
                    Ok(result) => result,
                    Err(error) => json!({ "ok": false, "error": error }),
                },
            }
        }
        "update_ticket" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("").trim().to_string();
            let status = args.get("status").and_then(Value::as_str).map(|s| s.trim().to_string());
            // XNAUT-175: two words, two different claims. `done` is the
            // agent's, "I finished the work", and any agent may say it. It
            // hands the ticket straight back to NautBot. `complete` means
            // tested, checked and approved, and only NautBot says that.
            //
            // `canvas_key` is the calling agent's handle on the agent-chat
            // path (agent_profiles.rs passes `&profile.handle`), so the check
            // needs no new plumbing. Anywhere else it is a conversation key,
            // which is simply not NautBot: this fails closed.
            let is_nautbot = canvas_key
                .trim()
                .eq_ignore_ascii_case(crate::agent_profiles::RESERVED_NAUTBOT_HANDLE);
            if status.as_deref() == Some("complete") && !is_nautbot {
                return json!({
                    "ok": false,
                    "error": "only NautBot can set a ticket to complete. Set it to done and it goes back to NautBot, who tests and approves it."
                });
            }
            let repo = match crate::project_management::repo_now() {
                Ok(repo) => repo,
                Err(error) => return json!({ "ok": false, "error": error }),
            };
            // ponytail: read-then-write under the PM mutation lock instead of
            // making the model carry expected_revision. The read is fresh and
            // the lock is what actually serialises writers; a revision the
            // model remembered from earlier in the turn is the likelier bug.
            let current = match crate::project_management::ticket_list_in(&repo, None) {
                Ok(tickets) => tickets.into_iter().find(|t| t.id == id),
                Err(error) => return json!({ "ok": false, "error": error }),
            };
            let Some(current) = current else {
                return json!({ "ok": false, "error": format!("no ticket called {id:?}") });
            };
            let status_is_done = status.as_deref() == Some("done");
            let body = args
                .get("append_body")
                .and_then(Value::as_str)
                .map(|added| appended_body(&current.body, added));
            let request = crate::project_management::TicketUpdateRequest {
                id: id.clone(),
                expected_revision: current.revision,
                title: None,
                ticket_type: None,
                status,
                priority: args.get("priority").and_then(Value::as_str).map(str::to_string),
                // Handing it back IS what done means, so the reassignment is
                // not something the model has to remember to do.
                owner: if status_is_done {
                    Some(Some(crate::agent_profiles::RESERVED_NAUTBOT_HANDLE.to_string()))
                } else {
                    args.get("owner").and_then(Value::as_str).map(|o| Some(o.to_string()))
                },
                clear_owner: false,
                documentation: None,
                body,
            };
            match crate::project_management::ticket_update_in(&repo, request) {
                Ok(ticket) => json!({ "ok": true, "id": ticket.id, "status": ticket.status, "revision": ticket.revision }),
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
/// A LOCAL page a tool handed back — a canvas, a preview, a dev server.
///
/// Only 127.0.0.1/localhost: a remote URL in a tool result is usually just a
/// link in some data (a repository, an issue), and opening those would be
/// noise. A local one is the thing the agent just made, and it belongs on
/// screen next to the conversation.
fn local_surface(value: &Value) -> Option<String> {
    let text = value.to_string();
    let start = text.find("http://127.0.0.1").or_else(|| text.find("http://localhost"))?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\\' || c == '\'')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// What one turn produced: the answer, what it ran, a local page worth
/// showing, and a sign-in card the chat should render.
pub struct TurnOutcome {
    pub text: String,
    pub performed: Vec<String>,
    pub surface: Option<String>,
    pub needs_auth: Option<Value>,
    /// The turn asked for the knowledge graph; the app opens it in a tab.
    pub open_graph: bool,
    /// The turn put a note in the split pane.
    pub wrote_document: bool,
    /// A zellij session the turn asked to watch.
    pub attach_session: Option<String>,
}

/// Did this turn write a document? The pane opens on the strength of it.
pub fn wrote_document(performed: &[String]) -> bool {
    performed.iter().any(|call| call.starts_with("write_document"))
}

/// Does this URL actually serve something? A tool can hand back an endpoint
/// that speaks a protocol rather than HTML, and opening that shows an error
/// page where a canvas was promised.
async fn answers_as_a_page(url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
    else {
        return false;
    };
    match client.get(url).send().await {
        Ok(response) => {
            let is_html = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.contains("html"))
                .unwrap_or(false);
            response.status().is_success() && is_html
        }
        Err(_) => false,
    }
}

/// A history that ends with an assistant message is a PREFILL, and the
/// Anthropic lane refuses one outright: 400 "This model does not support
/// assistant message prefill". The tool loop then dies and the caller falls
/// back to a plain completion, so the agent answers without tools and reads
/// like a broken feature rather than a broken request.
///
/// Agent Space sent one every single turn: it pushes a "Thinking…" placeholder
/// into the thread and then serialises the thread, placeholder included. Rather
/// than trust every caller to get the tail right, drop it here.
fn without_trailing_assistant(mut messages: Vec<Value>) -> Vec<Value> {
    while messages
        .last()
        .and_then(|message| message["role"].as_str())
        .is_some_and(|role| role == "assistant")
    {
        messages.pop();
    }
    messages
}

pub async fn run_turn(
    llm: &crate::settings::LlmSettings,
    model: &str,
    messages: Vec<Value>,
    effort: Option<&str>,
    capabilities: &[String],
    canvas_key: &str,
) -> Result<TurnOutcome, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("failed to build http client: {e}"))?;
    let url = crate::chat::join_endpoint(&llm.endpoint, "chat/completions");

    // The plugins this agent holds are OPENED for the turn, and their tools
    // sit next to xNAUT's own. Without this, "Forgejo connected" was followed
    // by "I can't query Forgejo in this chat", which is a fair thing to swear
    // about: connecting something has to change what the agent can do.
    let (mut sessions, plugin_tools, problems) = crate::mcp_client::open_for(capabilities).await;
    let mut conversation = without_trailing_assistant(messages);
    if !problems.is_empty() {
        // Say it in-band: a server that would not start is something the agent
        // should mention rather than silently work without.
        conversation.push(json!({
            "role": "system",
            "content": format!("These plugins could not be opened for this turn: {}", problems.join("; ")),
        }));
    }
    // The vault document chat keys its conversation "vault-document:...". In
    // that mode diagrams belong in the open note, so the canvas tools are
    // withheld (see the tool assembly below).
    let document_mode = canvas_key.starts_with("vault-document");
    let mut performed: Vec<String> = Vec::new();
    let mut surface: Option<String> = None;
    let mut needs_auth: Option<Value> = None;
    let mut wants_graph = false;
    let mut attach: Option<String> = None;
    let mut wrote_note = false;
    // Every call, not just the ones that worked: a turn that runs out of
    // rounds has to be able to say what it was busy doing.
    let mut attempted: Vec<String> = Vec::new();
    let mut routing_notices: Vec<String> = Vec::new();

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
        for attempt in 0..2 {
            let mut tools = tool_specs();
            // The vault document chat draws INTO the open note as ```mermaid```,
            // not onto a separate canvas file the workspace never shows. The
            // canvas tool's own description says "this is how you draw a
            // diagram", so leaving it in wins over the persona every time; drop
            // it here so the model embeds the diagram in the document instead.
            if document_mode {
                tools.retain(|tool| {
                    let name = tool
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    name != "read_canvas" && name != "update_canvas"
                });
            }
            tools.extend(plugin_tools.iter().cloned());
            let mut body = json!({
                "model": model,
                "messages": conversation,
                "tools": tools,
            });
            if attempt == 0 {
                body["reasoning_effort"] = json!("none");
            }
            let response = crate::chat::apply_auth(client.post(&url), &llm.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("chat request failed: {e}"))?;
            let status = response.status();
            let receipt = crate::chat::NautGateReceipt::from_headers(response.headers());
            // The gateway's ids go into our own chain so an export can ask
            // NautGate for its signed account of the same call (XNAUT-216).
            // Best effort on purpose: a gateway that answered is not a reason
            // to fail the turn, and no gateway means no record at all.
            if let Some(mut body) = receipt.evidence_body() {
                body.insert("model".into(), json!(model));
                let _ = crate::evidence::record("model_call", canvas_key, body);
            }
            if let Some(notice) = receipt.substitution_notice() {
                if !routing_notices.contains(&notice) {
                    routing_notices.push(notice);
                }
            }
            payload = response
                .json()
                .await
                .map_err(|e| format!("chat response was not JSON: {e}"))?;
            if status.is_success() {
                break;
            }
            if attempt == 1 {
                let detail = payload
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .or_else(|| payload.get("detail").and_then(Value::as_str))
                    .unwrap_or("unknown error");
                return Err(format!("{status}: {detail}{}", receipt.error_suffix()));
            }
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
            let text = if routing_notices.is_empty() {
                text
            } else {
                format!("{}\n\n{text}", routing_notices.join("\n"))
            };
            for session in sessions {
                session.close().await;
            }
            let document_written = wrote_note || wrote_document(&performed);
            return Ok(TurnOutcome {
                text,
                performed,
                surface,
                needs_auth,
                open_graph: wants_graph,
                wrote_document: document_written,
                attach_session: attach,
            });
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
            // "<plugin>__<tool>" belongs to an MCP server; anything else is
            // xNAUT's own.
            let result = match name.split_once("__") {
                Some((prefix, tool)) => {
                    let wanted = prefix.replace('_', "-");
                    match sessions
                        .iter_mut()
                        .find(|session| session.plugin_id.replace('-', "_") == prefix || session.plugin_id == wanted)
                    {
                        Some(session) => match session.call(tool, &args).await {
                            Ok(value) => value,
                            Err(error) => json!({ "ok": false, "error": error }),
                        },
                        None => json!({ "ok": false, "error": format!("{prefix} is not connected to this agent") }),
                    }
                }
                None => execute(&name, &args, canvas_key).await,
            };
            attempted.push(format!("{name} {}", args));
            // A plugin's own tools answer in MCP's shape, not ours, so "did
            // something happen" is: it is a plugin call that did not error.
            let worked = result.get("ok").and_then(Value::as_bool) == Some(true)
                || (name.contains("__") && result.get("error").is_none());
            if worked {
                performed.push(format!("{name} {}", args));
                if surface.is_none() {
                    surface = local_surface(&result);
                }
                // Verified before it is shown: the Excalidraw MCP server has
                // no page at its root, so surfacing its URL put "Cannot GET /"
                // in front of him. A surface has to be a page.
                if let Some(url) = surface.clone() {
                    if !answers_as_a_page(&url).await {
                        surface = None;
                    }
                }
                if name == "open_graph" {
                    wants_graph = true;
                }
                if name == "attach_session" {
                    attach = result.get("attach").and_then(Value::as_str).map(str::to_string);
                }
                if name == "show_note" {
                    wrote_note = true;
                }
            } else if needs_auth.is_none() {
                // A refusal that names the missing credential becomes a card.
                needs_auth = result
                    .get("error")
                    .and_then(Value::as_str)
                    .and_then(|error| serde_json::from_str::<Value>(error).ok())
                    .and_then(|parsed| parsed.get("needs_auth").cloned());
            }
            conversation.push(json!({
                "role": "tool",
                "tool_call_id": id,
                "name": name,
                "content": result.to_string(),
            }));
        }
    }
    for session in sessions {
        session.close().await;
    }
    Err(format!(
        "the agent kept calling tools without answering: {}",
        attempted.join(" | ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "I cannot create charts." Three diagram requests in a row failed in the
    /// vault composer, so this asks the real route to draw one and then reads
    /// the canvas back. Ignored because it costs a request; run it with
    /// `cargo test --bin xnaut -- --ignored draws_a_diagram`.
    #[tokio::test]
    #[ignore]
    async fn the_live_route_draws_a_diagram_on_the_canvas() {
        let settings = crate::settings::load_or_default();
        let llm = crate::chat::provider_llm(&settings, "nautgate").expect("nautgate configured");
        let key = "livecheck-diagram";
        let _ = crate::canvas::update(key, crate::canvas::Canvas::default(), crate::canvas::now_iso());

        let outcome = run_turn(
            &llm,
            "auto",
            vec![serde_json::json!({
                "role": "user",
                "content": "Draw a diagram of a two-tier app: a Frontend box and a Backend box, one edge from Frontend to Backend. Use update_canvas.",
            })],
            None,
            &[],
            key,
        )
        .await
        .expect("the tool loop reached the route");

        let canvas = crate::canvas::load(key);
        assert!(
            canvas.nodes.len() >= 2,
            "the route answered but drew nothing, so 'create a diagram' is still prose: {}",
            outcome.text
        );
    }

    #[test]
    fn a_turn_never_ends_on_the_agents_own_voice() {
        // Agent Space serialises its "Thinking…" placeholder into the history,
        // so every turn arrived as a prefill and the Anthropic lane 400'd the
        // whole request. The agent then answered with no tools at all, which
        // reads as a missing feature (XNAUT-217).
        let history = vec![
            json!({ "role": "system", "content": "you are an agent" }),
            json!({ "role": "user", "content": "file the ticket" }),
            json!({ "role": "assistant", "content": "Thinking…" }),
        ];
        let trimmed = without_trailing_assistant(history);
        assert_eq!(trimmed.len(), 2);
        assert_eq!(trimmed.last().unwrap()["role"], "user");
        // A history that already ends on the user is untouched.
        let clean = vec![json!({ "role": "user", "content": "hi" })];
        assert_eq!(without_trailing_assistant(clean.clone()), clean);
    }

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
        let (_store_lock, scratch) = crate::plugins::scratch_store("repair");
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
        let result = run_turn(&llm, "gpt-5.6-sol", messages, None, &[], "test").await;
        let store: Value = serde_json::from_str(&std::fs::read_to_string(&scratch).unwrap()).unwrap();
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);

        let TurnOutcome { text, performed: did, .. } = result.expect("the turn should finish");
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
    #[ignore = "talks to the live model and a real MCP server; run with --ignored"]
    async fn a_connected_plugin_can_actually_answer_the_question() {
        // "Forgejo connected." / "how many repos do we have?" / "I can't query
        // Forgejo in this chat". Connecting something has to change what the
        // agent can answer, or it was theatre.
        let (_store_lock, scratch) = crate::plugins::scratch_store("repair");
        let connected = crate::plugins::connect("forgejo").await;
        assert!(connected.is_ok(), "forgejo should connect from local credentials: {connected:?}");

        let settings = crate::settings::load_or_default();
        let llm = crate::chat::provider_llm(&settings, "nautgate").expect("nautgate configured");
        let system = format!(
            "You are NautBot (@nautbot), one of the agents in xNAUT.\n\n{}",
            crate::composer::CHAT_RULES
        );
        let messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": "How many repositories do we have on Forgejo? Use your tools and give me the number." }),
        ];
        let result = run_turn(&llm, "gpt-5.6-sol", messages, None, &["plugin:forgejo".to_string()], "test").await;
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);

        let TurnOutcome { text, performed: did, .. } = result.expect("the turn should finish");
        println!("agent said: {text}\ntools run: {did:?}");
        assert!(
            did.iter().any(|call| call.starts_with("forgejo__")),
            "the agent never used the connected plugin: {did:?}"
        );
        assert!(
            text.chars().any(|c| c.is_ascii_digit()),
            "an answer about how many repos should contain a number: {text}"
        );
    }

    #[test]
    fn only_a_local_page_is_offered_to_the_screen() {
        // A repository URL in a Forgejo result is a link in data; a canvas on
        // 127.0.0.1 is the thing the agent just made.
        assert_eq!(
            local_surface(&json!({ "canvas": "http://127.0.0.1:3001/" })).as_deref(),
            Some("http://127.0.0.1:3001/")
        );
        assert!(local_surface(&json!({ "html_url": "https://cosmos/48Nauts/xNaut" })).is_none());
    }

    /// The phase-5 join, against a real gateway rather than a stub: point
    /// `XNAUT_LIVE_GATE` at a NautGate with the verified audit trail on, and
    /// this makes one real model call through it and checks that the routing
    /// ids came back on the wire and reached our own chain (XNAUT-216).
    ///
    /// `XNAUT_LIVE_GATE=http://127.0.0.1:8099/v1 XNAUT_LIVE_GATE_KEY=ng_… \
    ///  XNAUT_LIVE_GATE_MODEL=… XNAUT_EVIDENCE_DIR=<dir> cargo test --bin xnaut \
    ///  -- --ignored the_gateway_ids_reach_our_own_chain --nocapture`
    #[tokio::test]
    #[ignore = "needs a NautGate with the audit trail on; run with --ignored"]
    async fn the_gateway_ids_reach_our_own_chain() {
        let endpoint = std::env::var("XNAUT_LIVE_GATE").expect("XNAUT_LIVE_GATE");
        let model = std::env::var("XNAUT_LIVE_GATE_MODEL").expect("XNAUT_LIVE_GATE_MODEL");
        let key = format!("gatejoin-{}", std::process::id());
        let llm = crate::settings::LlmSettings {
            provider: "nautgate".into(),
            endpoint,
            model: model.clone(),
            api_key: std::env::var("XNAUT_LIVE_GATE_KEY").ok(),
            system_prompt: None,
            harness_local: false,
        };
        let messages = vec![json!({ "role": "user", "content": "Reply with the single word: ok" })];
        let outcome = run_turn(&llm, &model, messages, None, &[], &key).await;
        let text = outcome.expect("the turn should finish").text;

        let log = std::fs::read_to_string(crate::evidence::log_path()).expect("execution log");
        let record: Value = log
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|row| row["kind"] == "model_call" && row["session_id"] == key.as_str())
            .next_back()
            .expect("the turn recorded a model call");
        println!("model said: {text}\nrecord: {record}");
        assert_eq!(record["model"], model.as_str());
        assert!(
            record["nautgate"]["receipt_id"].is_string(),
            "no receipt id: the gateway did not send one, or we dropped it again"
        );
    }

    #[tokio::test]
    #[ignore = "talks to the live model; run with --ignored"]
    async fn a_diagram_is_drawn_on_the_canvas_not_sent_to_a_worktree() {
        // "Why do we need to make the fuss and open an agent?" — we do not.
        // A diagram is a canvas update, and the canvas is beside the chat.
        let key = format!("livetest-{}", std::process::id());
        let settings = crate::settings::load_or_default();
        let llm = crate::chat::provider_llm(&settings, "nautgate").expect("nautgate configured");
        let system = format!(
            "You are NautBot (@nautbot), one of the agents in xNAUT.\n\n{}",
            crate::composer::CHAT_RULES
        );
        let messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": "Can you please create for me a diagram to explain how Agentic Loops work?" }),
        ];
        let outcome = run_turn(&llm, "gpt-5.6-sol", messages, None, &[], &key).await;
        let canvas = crate::canvas::load(&key);
        let _ = std::fs::remove_file(
            dirs::config_dir().unwrap().join("xnaut").join("canvases").join(format!("{key}.json")),
        );

        let TurnOutcome { text, performed, .. } = outcome.expect("the turn should finish");
        println!("agent said: {text}\ntools: {performed:?}\nnodes: {:?}", 
            canvas.nodes.iter().map(|n| (&n.id, &n.kind, n.x, n.y)).collect::<Vec<_>>());
        assert!(
            performed.iter().any(|call| call.starts_with("update_canvas")),
            "nothing was drawn: {performed:?}"
        );
        assert!(canvas.nodes.len() >= 3, "a loop needs more than {} boxes", canvas.nodes.len());
        assert!(!canvas.edges.is_empty(), "a loop with no arrows is not a loop");
        // Every box got a place, or the drawing is a pile in the corner.
        assert!(canvas.nodes.iter().any(|node| node.x > 0.0 || node.y > 0.0));
    }

    #[tokio::test]
    #[ignore = "talks to the live model and reads the real vault; run with --ignored"]
    async fn the_librarian_finds_what_is_already_written() {
        // The Librarian was a pane with its own JSON protocol. As an agent it
        // has to actually search the vault and answer from it.
        let settings = crate::settings::load_or_default();
        let llm = crate::chat::provider_llm(&settings, "nautgate").expect("nautgate configured");
        let system = format!(
            "You are Librarian (@librarian), one of the agents in xNAUT.\n\nKeeps the vault.\n\n{}",
            crate::composer::CHAT_RULES
        );
        let messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": "What do we have written down about the plugin marketplace? Give me the note paths." }),
        ];
        let outcome = run_turn(&llm, "gpt-5.6-sol", messages, None, &[], "librarian").await;
        let TurnOutcome { text, performed, .. } = outcome.expect("the turn should finish");
        println!("librarian said: {text}\ntools: {performed:?}");
        assert!(
            performed.iter().any(|call| call.starts_with("vault_search")),
            "it never looked in the vault: {performed:?}"
        );
        assert!(text.contains(".md"), "an answer about notes should name one: {text}");
    }

    #[tokio::test]
    #[ignore = "talks to the live model and reads the real vault; run with --ignored"]
    async fn the_librarian_answers_the_three_questions_it_failed_on() {
        // From the transcript he sent: the newest document, the project's
        // notes, and opening one in the pane. All three were "no".
        let settings = crate::settings::load_or_default();
        let llm = crate::chat::provider_llm(&settings, "nautgate").expect("nautgate configured");
        let system = format!(
            "You are Librarian (@librarian), one of the agents in xNAUT.\n\nKeeps the vault.\n\n{}",
            crate::composer::CHAT_RULES
        );
        let ask = |question: &'static str| {
            let llm = llm.clone();
            let system = system.clone();
            async move {
                let messages = vec![
                    json!({ "role": "system", "content": system }),
                    json!({ "role": "user", "content": question }),
                ];
                run_turn(&llm, "gpt-5.6-sol", messages, None, &[], "librariantest").await
            }
        };

        let newest = ask("What was the last document added to the vault?").await.expect("turn");
        println!("newest: {}\n  tools {:?}", newest.text, newest.performed);
        assert!(
            newest.performed.iter().any(|call| call.starts_with("vault_recent")),
            "it guessed instead of asking for the newest: {:?}",
            newest.performed
        );

        let project = ask("What notes do we have for the xNaut project?").await.expect("turn");
        println!("project: {}\n  tools {:?}", project.text, project.performed);
        // It may name paths or summarise them by group; what must be true is
        // that it looked, and came back with something rather than "no
        // matching notes" — which is what it said before.
        assert!(
            project.performed.iter().any(|call| call.starts_with("vault_search")),
            "it never searched: {:?}",
            project.performed
        );
        assert!(
            !project.text.to_lowercase().contains("no matching"),
            "it still found nothing: {}",
            project.text
        );

        let shown = ask("Open the newest xNAUT note in the split screen.").await.expect("turn");
        println!("shown: {}\n  tools {:?}", shown.text, shown.performed);
        assert!(
            shown.performed.iter().any(|call| call.starts_with("show_note")),
            "it did not put anything in the pane: {:?}",
            shown.performed
        );
        assert!(shown.wrote_document, "the pane was never told to open");
    }

    #[tokio::test]
    #[ignore = "talks to the live model and writes a profile; run with --ignored"]
    async fn creating_an_agent_is_one_call_not_a_worktree() {
        // His words, and the screenshot: "Can you create me a new Agent? I
        // would like to call him the Frontend Developer short Fronti" opened a
        // worktree in 05-DevOps and started reading repository guides.
        let settings = crate::settings::load_or_default();
        let llm = crate::chat::provider_llm(&settings, "nautgate").expect("nautgate configured");
        let system = format!(
            "You are NautBot (@nautbot), one of the agents in xNAUT.\n\n{}",
            crate::composer::CHAT_RULES
        );
        let messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": "Can you create me a new Agent? I would like to call him the Frontend Developer, short Fronti" }),
        ];
        let outcome = run_turn(&llm, "gpt-5.6-sol", messages, None, &[], "nautbot").await.expect("turn");
        println!("said: {}\ntools: {:?}", outcome.text, outcome.performed);

        // Clean up whatever it made before asserting, so a failure does not
        // leave a stray agent in his roster.
        let made = crate::agent_profiles::roster_snapshot()
            .into_iter()
            .any(|agent| agent["handle"] == json!("fronti"));
        if made {
            let _ = crate::agent_profiles::delete_profile_for_test("fronti");
        }
        assert!(
            outcome.performed.iter().any(|call| call.starts_with("create_agent")),
            "it did not create an agent: {:?}",
            outcome.performed
        );
        assert!(made, "the roster never gained @fronti");
        assert!(
            !outcome.performed.iter().any(|call| call.contains("BUILD")),
            "it tried to build something: {:?}",
            outcome.performed
        );
    }

    #[tokio::test]
    #[ignore = "talks to the live model and lists real sessions; run with --ignored"]
    async fn an_agent_can_find_and_watch_a_session_that_is_already_running() {
        let settings = crate::settings::load_or_default();
        let llm = crate::chat::provider_llm(&settings, "nautgate").expect("nautgate configured");
        let system = format!(
            "You are NautBot (@nautbot), one of the agents in xNAUT.\n\n{}",
            crate::composer::CHAT_RULES
        );
        let messages = vec![
            json!({ "role": "system", "content": system }),
            json!({ "role": "user", "content": "Which zellij sessions are running? Attach to the Cockpit one so I can watch it." }),
        ];
        let outcome = run_turn(&llm, "gpt-5.6-sol", messages, None, &[], "nautbot").await.expect("turn");
        println!("said: {}\ntools: {:?}\nattach: {:?}", outcome.text, outcome.performed, outcome.attach_session);
        assert!(
            outcome.performed.iter().any(|call| call.starts_with("list_sessions")),
            "it never looked: {:?}",
            outcome.performed
        );
        assert!(outcome.attach_session.is_some(), "nothing was attached: {:?}", outcome.performed);
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
    async fn done_is_the_agents_word_and_complete_is_nautbots() {
        // Andre's rule: an agent marks a ticket done, that hands it back to
        // NautBot, and only NautBot can call it complete (tested, checked,
        // approved). Refused before the repo is opened, so this is the same
        // answer on a machine with no PM repo.
        let refused = execute(
            "update_ticket",
            &json!({ "id": "XNAUT-1", "status": "complete" }),
            "librarian",
        )
        .await;
        assert_eq!(refused["ok"], json!(false));
        assert!(
            refused["error"].as_str().unwrap().contains("only NautBot"),
            "the refusal has to say who CAN approve it, got {refused}"
        );

        // NautBot itself, and any agent reporting done, must get past the
        // guard and fail for the ordinary reason instead.
        for (caller, status) in [("nautbot", "complete"), ("librarian", "done")] {
            let allowed = execute(
                "update_ticket",
                &json!({ "id": "xnaut-no-such-ticket", "status": status }),
                caller,
            )
            .await;
            assert!(
                !allowed["error"].as_str().unwrap_or("").contains("only NautBot"),
                "{caller} setting {status} must not hit the guard, got {allowed}"
            );
        }

        // complete has to actually exist as a status, or NautBot's half of the
        // rule is a refusal on both sides.
        assert!(crate::project_management::TICKET_STATUSES.contains(&"complete"));

        // A ticket's body is appended to, never replaced: a model asked to
        // "update the body" will hand back a tidied version with the history
        // gone.
        assert_eq!(appended_body("first", "second"), "first\n\nsecond");
        assert_eq!(appended_body("   ", "only"), "only");
        assert!(appended_body("history", "new").starts_with("history"));
    }

    #[tokio::test]
    async fn an_unknown_tool_answers_instead_of_failing_the_turn() {
        let result = execute("drop_everything", &json!({}), "test").await;
        assert_eq!(result["ok"], json!(false));
        assert!(result["error"].as_str().unwrap().contains("no such tool"));
    }

    /// A value no real credential would be, planted so a leak is unambiguous.
    const PLANTED_SECRET: &str = "xnaut-planted-secret-9f13";

    #[tokio::test]
    async fn enabling_a_plugin_that_cannot_run_reports_why() {
        // Notion needs NOTION_TOKEN. The refusal has to name it, because
        // "could not enable" sends the owner looking in the wrong place.
        //
        // Against a SCRATCH library: the first version of this test ran against
        // the real one and switched a plugin on in André's own config.
        let scratch = std::env::temp_dir().join(format!("xnaut-plugins-test-{}.json", std::process::id()));
        std::env::set_var("XNAUT_PLUGINS_PATH", &scratch);
        // Plant a credential so the listing can be checked for it.
        let mut notion = crate::plugins::seed().into_iter().find(|p| p.id == "notion").unwrap();
        notion.env.insert("NOTION_TOKEN".into(), PLANTED_SECRET.into());
        crate::plugins::plugin_save(notion).expect("plant");
        let result = execute("set_plugin_enabled", &json!({ "id": "obsidian", "enabled": true }), "test").await;
        let unknown = execute("set_plugin_enabled", &json!({ "id": "nope", "enabled": true }), "test").await;
        let listed = execute("list_plugins", &json!({}), "test").await;
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);

        assert_eq!(result["ok"], json!(false));
        assert!(
            result["error"].as_str().unwrap().contains("OBSIDIAN_API_KEY"),
            "expected the missing key to be named, got {result}"
        );
        assert!(unknown["error"].as_str().unwrap().contains("no plugin called"));
        // The listing names the missing variable — that is the whole point of
        // "blocked_by" — but never carries a VALUE. Proven with one planted,
        // rather than by looking for the string "env": a plugin in the
        // compiled catalog ships a skill named exactly that, and the crude
        // check failed on it.
        let text = listed.to_string();
        assert!(text.contains("OBSIDIAN_API_KEY"), "the listing must say what is missing");
        assert!(!text.contains(PLANTED_SECRET), "the listing leaks a credential value");
    }

    #[tokio::test]
    async fn every_tool_name_in_the_spec_is_one_execute_knows() {
        // A spec entry with no implementation is a tool the model will call
        // once and never get an answer from.
        for spec in tool_specs() {
            let name = spec["function"]["name"].as_str().unwrap().to_string();
            let result = execute(&name, &json!({}), "test").await;
            assert!(
                result.get("error").and_then(Value::as_str).map(|e| !e.contains("no such tool")).unwrap_or(true),
                "{name} is advertised but not implemented"
            );
        }
    }
}

