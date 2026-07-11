use crate::settings::McpServerSettings;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

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
