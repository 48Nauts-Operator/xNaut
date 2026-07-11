use crate::settings::McpServerSettings;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

fn local_excalidraw_process() -> &'static Mutex<Option<Child>> {
    static PROCESS: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
    PROCESS.get_or_init(|| Mutex::new(None))
}

fn command_path(name: &str) -> Result<String, String> {
    let output = Command::new("/bin/zsh")
        .args(["-lc", &format!("command -v {name}")])
        .output()
        .map_err(|error| format!("failed to locate {name}: {error}"))?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && !path.is_empty() {
        Ok(path)
    } else {
        Err(format!(
            "{name} is required to run the local Excalidraw MCP server"
        ))
    }
}

fn local_excalidraw_dir() -> Result<std::path::PathBuf, String> {
    dirs::data_local_dir()
        .map(|root| root.join("xnaut").join("mcp").join("excalidraw-mcp"))
        .ok_or_else(|| "local application data directory is unavailable".into())
}

fn run_checked(command: &str, args: &[&str], cwd: Option<&std::path::Path>) -> Result<(), String> {
    let mut process = Command::new(command);
    process.args(args);
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    let output = process
        .output()
        .map_err(|error| format!("failed to run {command}: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "{command} failed: {}",
        stderr.chars().take(500).collect::<String>()
    ))
}

fn install_local_excalidraw() -> Result<std::path::PathBuf, String> {
    let dir = local_excalidraw_dir()?;
    let entry = dir.join("dist").join("index.js");
    if entry.is_file() {
        let source = std::fs::read_to_string(dir.join("src").join("main.ts")).unwrap_or_default();
        if source.contains("app.listen(port, \"127.0.0.1\"") {
            return Ok(entry);
        }
    }
    if !dir.join(".git").is_dir() {
        if dir.exists() {
            return Err(format!(
                "local Excalidraw MCP directory is incomplete: {}",
                dir.display()
            ));
        }
        if let Some(parent) = dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let git = command_path("git")?;
        let target = dir.to_string_lossy().to_string();
        run_checked(
            &git,
            &[
                "clone",
                "--depth",
                "1",
                "https://github.com/excalidraw/excalidraw-mcp.git",
                &target,
            ],
            None,
        )?;
    }
    let main_source = dir.join("src").join("main.ts");
    let source = std::fs::read_to_string(&main_source)
        .map_err(|error| format!("failed to read {}: {error}", main_source.display()))?;
    let source = source
        .replace("host: \"0.0.0.0\"", "host: \"127.0.0.1\"")
        .replace(
            "app.listen(port, (err)",
            "app.listen(port, \"127.0.0.1\", (err)",
        );
    std::fs::write(&main_source, source)
        .map_err(|error| format!("failed to secure {}: {error}", main_source.display()))?;
    let pnpm = command_path("pnpm")?;
    run_checked(&pnpm, &["install", "--frozen-lockfile"], Some(&dir))?;
    run_checked(&pnpm, &["run", "build"], Some(&dir))?;
    if !entry.is_file() {
        return Err(format!(
            "Excalidraw MCP build did not create {}",
            entry.display()
        ));
    }
    Ok(entry)
}

#[tauri::command]
pub async fn mcp_start_local_excalidraw() -> Result<String, String> {
    {
        let mut process = local_excalidraw_process()
            .lock()
            .map_err(|_| "local MCP process lock is unavailable")?;
        if let Some(child) = process.as_mut() {
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                return Ok("Local Excalidraw MCP is already running".into());
            }
        }
    }
    let entry = tokio::task::spawn_blocking(install_local_excalidraw)
        .await
        .map_err(|error| format!("local Excalidraw MCP installer failed: {error}"))??;
    {
        let mut process = local_excalidraw_process()
            .lock()
            .map_err(|_| "local MCP process lock is unavailable")?;
        if let Some(child) = process.as_mut() {
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                return Ok("Local Excalidraw MCP is already running".into());
            }
        }
        let node = command_path("node")?;
        let child = Command::new(node)
            .arg(entry)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("failed to start local Excalidraw MCP: {error}"))?;
        *process = Some(child);
    }
    for _ in 0..30 {
        if tokio::net::TcpStream::connect("127.0.0.1:3001")
            .await
            .is_ok()
        {
            return Ok("Local Excalidraw MCP is running on 127.0.0.1:3001".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err("local Excalidraw MCP did not become ready within 15 seconds".into())
}

#[tauri::command]
pub fn mcp_stop_local_excalidraw() -> Result<(), String> {
    stop_local_excalidraw_process()
}

pub fn stop_local_excalidraw_process() -> Result<(), String> {
    let mut process = local_excalidraw_process()
        .lock()
        .map_err(|_| "local MCP process lock is unavailable")?;
    if let Some(mut child) = process.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

fn configured_server(
    settings: &crate::settings::Settings,
    name: &str,
) -> Result<McpServerSettings, String> {
    settings
        .mcp_servers
        .iter()
        .find(|server| server.name == name && server.enabled)
        .cloned()
        .ok_or_else(|| format!("MCP server is not enabled: {name}"))
}

fn parse_rpc_body(body: &str) -> Result<Value, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Ok(Value::Null);
    }
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }
    let mut last = None;
    for line in trimmed.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            if let Ok(value) = serde_json::from_str::<Value>(data.trim()) {
                last = Some(value);
            }
        }
    }
    last.ok_or_else(|| format!("invalid MCP response: {}", excerpt(trimmed)))
}

fn excerpt(value: &str) -> String {
    value.chars().take(300).collect()
}

async fn post_rpc(
    client: &reqwest::Client,
    server: &McpServerSettings,
    payload: &Value,
    session_id: Option<&str>,
) -> Result<(Value, Option<String>), String> {
    let mut request = client
        .post(server.url.trim())
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
        .json(payload);
    if let Some(key) = server
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        let token = key.strip_prefix("Bearer ").unwrap_or(key);
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(session) = session_id {
        request = request.header("Mcp-Session-Id", session);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("MCP request to {} failed: {error}", server.url))?;
    let status = response.status();
    let next_session = response
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response
        .text()
        .await
        .map_err(|error| format!("failed to read MCP response: {error}"))?;
    if !status.is_success() {
        return Err(format!("MCP request failed ({status}): {}", excerpt(&body)));
    }
    let value = parse_rpc_body(&body)?;
    if let Some(error) = value.get("error") {
        return Err(format!("MCP error: {error}"));
    }
    Ok((value, next_session))
}

async fn initialized_client(
    server: &McpServerSettings,
) -> Result<(reqwest::Client, Option<String>), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("failed to build MCP client: {error}"))?;
    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "xNAUT", "version": env!("CARGO_PKG_VERSION") }
        }
    });
    let (_, session) = post_rpc(&client, server, &initialize, None).await?;
    let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    let _ = post_rpc(&client, server, &initialized, session.as_deref()).await;
    Ok((client, session))
}

#[tauri::command]
pub async fn mcp_list_tools(
    state: tauri::State<'_, crate::state::AppState>,
    server: String,
) -> Result<Vec<Value>, String> {
    let settings = state.settings.lock().await.clone();
    let server = configured_server(&settings, server.trim())?;
    let (client, session) = initialized_client(&server).await?;
    let payload = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} });
    let (response, _) = post_rpc(&client, &server, &payload, session.as_deref()).await?;
    Ok(response["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default())
}

#[tauri::command]
pub async fn mcp_call_tool(
    state: tauri::State<'_, crate::state::AppState>,
    server: String,
    tool: String,
    arguments: Value,
) -> Result<Value, String> {
    let settings = state.settings.lock().await.clone();
    let server = configured_server(&settings, server.trim())?;
    let (client, session) = initialized_client(&server).await?;
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": tool.trim(), "arguments": arguments }
    });
    let (response, _) = post_rpc(&client, &server, &payload, session.as_deref()).await?;
    Ok(response["result"].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_and_sse_responses() {
        assert_eq!(
            parse_rpc_body(r#"{"jsonrpc":"2.0","id":1}"#).unwrap()["id"],
            1
        );
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2}\n\n";
        assert_eq!(parse_rpc_body(sse).unwrap()["id"], 2);
    }
}
