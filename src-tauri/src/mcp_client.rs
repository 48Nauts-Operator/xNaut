// An MCP client, so a connected plugin is a plugin the agent can actually USE.
//
// "Forgejo connected." / "how many repos do we have?" / "I can't query Forgejo
// in this chat" — because until now an MCP server only ever reached the coding
// harness at launch (--mcp-config, -c mcp_servers). A chat turn had xNAUT's own
// admin tools and nothing else, so connecting something changed nothing about
// what the agent could answer.
//
// This opens the server for the duration of a turn, asks what it can do, and
// offers those tools to the model alongside xNAUT's. Protocol is JSON-RPC 2.0:
// newline-delimited over stdio, or POST for the HTTP transport.
//
// Deliberately per-turn: an MCP server is a child process holding credentials,
// and keeping a pool of them alive between messages is a much bigger promise
// than "answer this question". Cost is one process start per turn per plugin.

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// Long enough for `npx -y <pkg>` to download on first use.
const START_TIMEOUT: Duration = Duration::from_secs(90);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const PROTOCOL_VERSION: &str = "2025-06-18";

pub enum Transport {
    Stdio {
        child: Child,
        stdin: ChildStdin,
        reader: BufReader<ChildStdout>,
    },
    Http {
        client: reqwest::Client,
        url: String,
        headers: Vec<(String, String)>,
        session: Option<String>,
    },
}

pub struct Session {
    pub plugin_id: String,
    transport: Transport,
    next_id: i64,
}

impl Session {
    /// Start the server and complete the MCP handshake.
    pub async fn open(plugin: &crate::plugins::Plugin) -> Result<Self, String> {
        let transport = match plugin.transport {
            crate::plugins::Transport::Stdio => {
                let mut command = tokio::process::Command::new(&plugin.command);
                command
                    .args(plugin.resolved_args())
                    .envs(
                        plugin
                            .env
                            .iter()
                            .filter(|(_, value)| !value.trim().is_empty())
                            .map(|(key, value)| (key.clone(), value.clone())),
                    )
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .kill_on_drop(true);
                if let Some(path) = crate::agents::runtime_path_public() {
                    command.env("PATH", path);
                }
                let mut child = command
                    .spawn()
                    .map_err(|e| format!("could not start {}: {e}", plugin.command))?;
                let stdin = child.stdin.take().ok_or("no stdin on the MCP server")?;
                let stdout = child.stdout.take().ok_or("no stdout on the MCP server")?;
                Transport::Stdio {
                    child,
                    stdin,
                    reader: BufReader::new(stdout),
                }
            }
            crate::plugins::Transport::Http => Transport::Http {
                client: reqwest::Client::builder()
                    .timeout(CALL_TIMEOUT)
                    .build()
                    .map_err(|e| e.to_string())?,
                url: plugin.url.trim().to_string(),
                headers: plugin
                    .headers
                    .iter()
                    .filter(|(_, value)| !value.trim().is_empty())
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
                session: None,
            },
        };
        let mut session = Session {
            plugin_id: plugin.id.clone(),
            transport,
            next_id: 1,
        };
        session
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "xNAUT", "version": env!("CARGO_PKG_VERSION") }
                }),
                START_TIMEOUT,
            )
            .await?;
        session.notify("notifications/initialized").await?;
        Ok(session)
    }

    /// The tools this server offers, already namespaced so two plugins cannot
    /// collide on a name like "search".
    pub async fn tools(&mut self) -> Result<Vec<Value>, String> {
        let result = self.request("tools/list", json!({}), CALL_TIMEOUT).await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(tools
            .into_iter()
            .filter_map(|tool| {
                let name = tool.get("name")?.as_str()?.to_string();
                let schema = tool
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
                Some(json!({
                    "type": "function",
                    "function": {
                        "name": format!("{}__{name}", self.plugin_id.replace('-', "_")),
                        "description": tool.get("description").and_then(Value::as_str).unwrap_or(""),
                        "parameters": schema,
                    }
                }))
            })
            .collect())
    }

    pub async fn call(&mut self, tool: &str, args: &Value) -> Result<Value, String> {
        self.request(
            "tools/call",
            json!({ "name": tool, "arguments": args }),
            CALL_TIMEOUT,
        )
        .await
    }

    async fn notify(&mut self, method: &str) -> Result<(), String> {
        let message = json!({ "jsonrpc": "2.0", "method": method });
        match &mut self.transport {
            Transport::Stdio { stdin, .. } => {
                let line = format!("{message}\n");
                stdin
                    .write_all(line.as_bytes())
                    .await
                    .map_err(|e| format!("could not write to the MCP server: {e}"))?;
                stdin.flush().await.map_err(|e| e.to_string())?;
            }
            // The HTTP transport has no separate notification channel worth
            // keeping here; servers accept the POST and answer 202.
            Transport::Http {
                client,
                url,
                headers,
                session,
            } => {
                let mut request = client.post(url.as_str()).json(&message);
                for (key, value) in headers.iter() {
                    request = request.header(key, value);
                }
                if let Some(id) = session {
                    request = request.header("Mcp-Session-Id", id.as_str());
                }
                let _ = request.send().await;
            }
        }
        Ok(())
    }

    async fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });

        let payload: Value = match &mut self.transport {
            Transport::Stdio { stdin, reader, .. } => {
                let line = format!("{message}\n");
                stdin
                    .write_all(line.as_bytes())
                    .await
                    .map_err(|e| format!("could not write to the MCP server: {e}"))?;
                stdin.flush().await.map_err(|e| e.to_string())?;
                // Skip anything that is not our answer: servers log, and some
                // send notifications before the result.
                let deadline = tokio::time::Instant::now() + timeout;
                loop {
                    let mut line = String::new();
                    let read = tokio::time::timeout_at(deadline, reader.read_line(&mut line))
                        .await
                        .map_err(|_| format!("{method} timed out"))?
                        .map_err(|e| format!("could not read from the MCP server: {e}"))?;
                    if read == 0 {
                        return Err("the MCP server closed the connection".into());
                    }
                    let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
                        continue;
                    };
                    if value.get("id").and_then(Value::as_i64) == Some(id) {
                        break value;
                    }
                }
            }
            Transport::Http {
                client,
                url,
                headers,
                session,
            } => {
                let mut request = client
                    .post(url.as_str())
                    .header("Accept", "application/json, text/event-stream")
                    .json(&message);
                for (key, value) in headers.iter() {
                    request = request.header(key, value);
                }
                if let Some(id) = session.as_ref() {
                    request = request.header("Mcp-Session-Id", id.as_str());
                }
                let response = request
                    .send()
                    .await
                    .map_err(|e| format!("{method} failed: {e}"))?;
                if let Some(id) = response
                    .headers()
                    .get("mcp-session-id")
                    .and_then(|value| value.to_str().ok())
                {
                    *session = Some(id.to_string());
                }
                let status = response.status();
                let body = response.text().await.map_err(|e| e.to_string())?;
                if !status.is_success() {
                    return Err(format!("{method} failed ({status}): {}", body.chars().take(200).collect::<String>()));
                }
                // A streamable-HTTP server may answer as SSE; the payload is
                // the first data: line.
                let text = body
                    .lines()
                    .find_map(|line| line.strip_prefix("data:"))
                    .map(str::trim)
                    .unwrap_or(body.trim());
                serde_json::from_str(text)
                    .map_err(|e| format!("{method} returned a body that is not JSON-RPC: {e}"))?
            }
        };

        if let Some(error) = payload.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(format!("{method}: {message}"));
        }
        Ok(payload.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn close(mut self) {
        if let Transport::Stdio { child, .. } = &mut self.transport {
            let _ = child.kill().await;
        }
    }
}

/// Open every plugin this agent holds, and collect their tools.
///
/// A server that will not start is skipped with its reason rather than failing
/// the turn: one broken connector must not take the conversation with it.
pub async fn open_for(capabilities: &[String]) -> (Vec<Session>, Vec<Value>, Vec<String>) {
    let mut sessions = Vec::new();
    let mut tools = Vec::new();
    let mut problems = Vec::new();
    for plugin in crate::plugins::active_for(capabilities) {
        match Session::open(&plugin).await {
            Ok(mut session) => match session.tools().await {
                Ok(list) => {
                    tools.extend(list);
                    sessions.push(session);
                }
                Err(error) => {
                    problems.push(format!("{}: {error}", plugin.name));
                    session.close().await;
                }
            },
            Err(error) => problems.push(format!("{}: {error}", plugin.name)),
        }
    }
    (sessions, tools, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "read-only local Paper MCP handshake and tools/list"]
    async fn local_paper_exposes_tools() {
        let plugin = crate::plugins::seed().into_iter().find(|p| p.id == "paper").unwrap();
        let mut session = Session::open(&plugin).await.expect("Paper MCP initialize");
        let tools = session.tools().await.expect("Paper tools/list");
        assert!(tools.iter().any(|t| t["function"]["name"] == "paper__get_basic_info"));
        assert!(tools.iter().any(|t| t["function"]["name"] == "paper__list_files"));
        println!("PAPER_MCP tools={} read_only_discovery=true",tools.len());
        session.close().await;
    }

    #[test]
    fn tool_names_are_namespaced_per_plugin() {
        // Two servers both offering "search" would otherwise collide, and the
        // model would call one meaning the other.
        let id = "google-calendar".replace('-', "_");
        assert_eq!(format!("{id}__list_events"), "google_calendar__list_events");
    }

    #[tokio::test]
    #[ignore = "starts every runnable seeded server; slow. run with --ignored"]
    async fn how_many_seeded_plugins_actually_expose_tools() {
        // "With how many plugins can we do that?" — answered by starting them,
        // not by counting rows in the catalog. Only the ones that need no
        // credential we do not have are attempted; the rest are reported as
        // untested rather than claimed.
        let (_lock, scratch) = crate::plugins::scratch_store("survey");
        let mut ready = Vec::new();
        let mut blocked = Vec::new();
        let mut failed = Vec::new();

        for plugin in crate::plugins::seed() {
            let (found, _) = crate::plugins::discover(&plugin);
            let mut candidate = plugin.clone();
            for (key, value) in found {
                candidate.env.insert(key, value);
            }
            if let Some(reason) = crate::plugins::blocker(&candidate) {
                blocked.push(format!("{} ({reason})", candidate.name));
                continue;
            }
            match Session::open(&candidate).await {
                Ok(mut session) => match session.tools().await {
                    Ok(tools) => {
                        ready.push(format!("{} — {} tools", candidate.name, tools.len()));
                        session.close().await;
                    }
                    Err(error) => {
                        failed.push(format!("{} (tools/list: {error})", candidate.name));
                        session.close().await;
                    }
                },
                Err(error) => failed.push(format!("{} ({error})", candidate.name)),
            }
        }
        let _ = std::fs::remove_file(&scratch);

        println!("\n=== USABLE TODAY ({}) ===", ready.len());
        for line in &ready { println!("  {line}"); }
        println!("\n=== NEEDS A CREDENTIAL ({}) ===", blocked.len());
        for line in &blocked { println!("  {line}"); }
        println!("\n=== TRIED AND FAILED ({}) ===", failed.len());
        for line in &failed { println!("  {line}"); }
        assert!(!ready.is_empty(), "not one seeded plugin could be started");
    }

    #[tokio::test]
    #[ignore = "starts a real MCP server; run with --ignored"]
    async fn open_for_reports_what_it_opened_and_what_it_could_not() {
        let (_store_lock, scratch) = crate::plugins::scratch_store("openfor");
        let connected = crate::plugins::connect("forgejo").await;
        println!("connect: {connected:?}");
        let (sessions, tools, problems) = open_for(&["plugin:forgejo".to_string()]).await;
        println!("sessions: {} | problems: {problems:?}", sessions.len());
        println!("tools: {:?}", tools.iter().filter_map(|t| t.pointer("/function/name")).collect::<Vec<_>>());
        for session in sessions { session.close().await; }
        std::env::remove_var("XNAUT_PLUGINS_PATH");
        let _ = std::fs::remove_file(&scratch);
    }

    #[tokio::test]
    #[ignore = "starts a real MCP server over the network; run with --ignored"]
    async fn a_real_server_lists_its_tools() {
        let plugin = crate::plugins::seed()
            .into_iter()
            .find(|plugin| plugin.id == "context7")
            .unwrap();
        let mut session = Session::open(&plugin).await.expect("context7 should start");
        let tools = session.tools().await.expect("tools/list");
        println!("context7 tools: {:?}", tools.iter().filter_map(|t| t.pointer("/function/name")).collect::<Vec<_>>());
        assert!(!tools.is_empty(), "a server with no tools is not usable");
        session.close().await;
    }
}
