//! Native Responses transport, shared by direct providers and gateways.
//! Never downgrade to Chat Completions or change credentials on failure.
use futures_util::StreamExt;
use serde_json::{json, Value};

pub fn required(model: &str) -> bool {
    let name = model.trim().rsplit('/').next().unwrap_or("");
    name == "gpt-6-astra" || name.starts_with("gpt-6-astra-")
}
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Session {
    previous: Option<String>,
    sent: usize,
}
pub struct Answer {
    pub message: Value,
    pub usage: Value,
    pub receipt: crate::chat::NautGateReceipt,
    pub transport: Option<String>,
}
fn input(messages: &[Value], continuation: bool) -> Result<Vec<Value>, String> {
    messages.iter().filter(|m| !(continuation && m["role"]=="assistant")).map(|m|{
        if m["role"]=="tool" {
            let id=m["tool_call_id"].as_str().filter(|s|!s.is_empty()).ok_or("Tool result is missing its call ID")?;
            Ok(json!({"type":"function_call_output","call_id":id,"output":m["content"].as_str().unwrap_or("")}))
        }else if m.get("tool_calls").is_some(){Err("Unexpected historical tool calls without native Responses continuation".into())}
        else{Ok(json!({"role":m["role"],"content":m["content"]}))}
    }).collect()
}
fn tools(specs: &[Value]) -> Result<Vec<Value>, String> {
    specs
        .iter()
        .map(|s| {
            let f = s
                .get("function")
                .filter(|f| f.is_object())
                .ok_or("Responses requires function tool schemas")?;
            let mut f = f.clone();
            f["type"] = json!("function");
            // Preserve optional arguments. Responses otherwise normalizes schemas to
            // strict mode, making optional fields unexpectedly required.
            if f.get("strict").is_none() {
                f["strict"] = json!(false);
            }
            Ok(f)
        })
        .collect()
}
fn detail(v: &Value) -> String {
    v.pointer("/error/message")
        .or_else(|| v.pointer("/response/error/message"))
        .or_else(|| v.get("message"))
        .or_else(|| v.get("detail"))
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_else(|| v.to_string())
        .chars()
        .take(1600)
        .collect()
}
fn completed(response: &Value) -> Result<Value, String> {
    if response["status"] != "completed" {
        return Err(format!(
            "Responses did not complete ({}): {}",
            response["status"],
            detail(response)
        ));
    }
    let output = response["output"]
        .as_array()
        .ok_or("Completed Responses payload has no output items")?;
    let mut text = String::new();
    let mut calls = Vec::new();
    let mut seen = std::collections::HashMap::new();
    for item in output {
        match item["type"].as_str().unwrap_or("") {
            "message" => {
                for content in item["content"].as_array().into_iter().flatten() {
                    if let Some(part) = content["text"]
                        .as_str()
                        .or_else(|| content["refusal"].as_str())
                    {
                        text.push_str(part);
                    }
                }
            }
            "function_call" => {
                let id = item["call_id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Responses function call has no call_id")?;
                let name = item["name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Responses function call has no name")?;
                let args = item["arguments"]
                    .as_str()
                    .ok_or("Responses function call has no arguments")?;
                let parsed: Value = serde_json::from_str(args)
                    .map_err(|_| "Responses function arguments are incomplete or invalid JSON")?;
                if !parsed.is_object() {
                    return Err("Responses function arguments must be an object".into());
                }
                if let Some(previous) = seen.insert(id, (name, args)) {
                    if previous != (name, args) {
                        return Err("Conflicting duplicate Responses call ID".into());
                    }
                    continue;
                }
                calls.push(
                    json!({"id":id,"type":"function","function":{"name":name,"arguments":args}}),
                );
            }
            "reasoning" => {} // preserved by previous_response_id; never displayed as speech
            kind => return Err(format!("Unsupported Responses output item: {kind}")),
        }
    }
    if text.is_empty() && calls.is_empty() {
        return Err("Responses completed without text or function calls".into());
    }
    let mut message = json!({"role":"assistant","content":text});
    if !calls.is_empty() {
        message["tool_calls"] = json!(calls);
    }
    Ok(message)
}
#[derive(Default)]
struct Events {
    final_response: Option<Value>,
    text: String,
}
impl Events {
    fn accept(&mut self, data: &str) -> Result<Option<String>, String> {
        if data == "[DONE]" {
            return Ok(None);
        }
        let v: Value =
            serde_json::from_str(data).map_err(|e| format!("Invalid Responses SSE event: {e}"))?;
        if v.get("error").is_some_and(|e| !e.is_null())
            || matches!(
                v["type"].as_str(),
                Some("error" | "response.failed" | "response.incomplete")
            )
        {
            return Err(format!("Responses stream failed: {}", detail(&v)));
        }
        match v["type"].as_str() {
            Some("response.output_text.delta" | "response.refusal.delta") => {
                let delta = v["delta"]
                    .as_str()
                    .ok_or("Responses text delta is not text")?;
                self.text.push_str(delta);
                Ok(Some(delta.into()))
            }
            Some("response.completed") => {
                completed(&v["response"])?;
                self.final_response = Some(v["response"].clone());
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
async fn bounded_json(response: reqwest::Response) -> Result<Value, String> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = tokio::time::timeout(std::time::Duration::from_secs(120), stream.next())
        .await
        .map_err(|_| "Responses body timed out")?
    {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err("Responses payload exceeds 8 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Responses JSON: {e}"))
}
impl Session {
    pub async fn request(
        &mut self,
        client: &reqwest::Client,
        llm: &crate::settings::LlmSettings,
        model: &str,
        messages: &[Value],
        specs: &[Value],
        effort: Option<&str>,
        max_tokens: u32,
        stream_to: Option<(&tauri::AppHandle, &str)>,
    ) -> Result<Answer, String> {
        let since = if self.previous.is_some() {
            self.sent
        } else {
            0
        };
        let pending = messages
            .get(since..)
            .ok_or("Responses conversation changed during a turn")?;
        let mut body = json!({"model":model,"input":input(pending,self.previous.is_some())?,"tools":tools(specs)?,"stream":true,"max_output_tokens":max_tokens});
        let effort = effort
            .filter(|s| !s.is_empty() && *s != "none")
            .unwrap_or("low");
        body["reasoning"] = json!({"effort":effort});
        if let Some(previous) = &self.previous {
            body["previous_response_id"] = json!(previous);
        }
        let url = crate::chat::join_endpoint(&llm.endpoint, "responses");
        let response = crate::chat::apply_auth(client.post(&url), &llm.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Responses request to {url} failed: {e}"))?;
        let status = response.status();
        let receipt = crate::chat::NautGateReceipt::from_headers(response.headers());
        let transport = response
            .headers()
            .get("x-nautgate-transport")
            .and_then(|h| h.to_str().ok())
            .map(str::to_string);
        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|s| s.contains("text/event-stream"));
        if !status.is_success() {
            let payload = bounded_json(response)
                .await
                .unwrap_or(json!({"message":"The endpoint returned a non-JSON error"}));
            return Err(format!("Native Responses request failed ({status}): {}{}. The selected endpoint must support /responses; no provider fallback was attempted.",detail(&payload),receipt.error_suffix()));
        }
        let value = if is_sse {
            let mut stream = response.bytes_stream();
            let mut buffer = Vec::new();
            let mut event = String::new();
            let mut events = Events::default();
            let mut total = 0usize;
            'read: loop {
                let next = tokio::time::timeout(std::time::Duration::from_secs(120), stream.next())
                    .await
                    .map_err(|_| "Responses stream timed out waiting for an event")?;
                let Some(chunk) = next else { break };
                let chunk = chunk.map_err(|e| format!("Responses stream disconnected: {e}"))?;
                total += chunk.len();
                if total > 32 * 1024 * 1024 {
                    return Err("Responses stream exceeds 32 MiB".into());
                }
                buffer.extend_from_slice(&chunk);
                if buffer.len() > 4 * 1024 * 1024 {
                    return Err("Responses event exceeds 4 MiB".into());
                }
                while let Some(at) = buffer.iter().position(|b| *b == b'\n') {
                    let bytes: Vec<_> = buffer.drain(..=at).collect();
                    let line = std::str::from_utf8(&bytes)
                        .map_err(|_| "Responses event contains invalid UTF-8")?
                        .trim_end_matches(['\n', '\r']);
                    if line.is_empty() {
                        if !event.is_empty() {
                            if let Some(delta) = events.accept(event.trim_end_matches('\n'))? {
                                if let Some((app, id)) = stream_to {
                                    crate::durable_turn::emit_chunk(app, id, &delta)?;
                                }
                            }
                            event.clear();
                            if events.final_response.is_some() {
                                break 'read;
                            }
                        }
                    } else if let Some(data) = line.strip_prefix("data:") {
                        event.push_str(data.strip_prefix(' ').unwrap_or(data));
                        event.push('\n');
                        if event.len() > 4 * 1024 * 1024 {
                            return Err("Responses event exceeds 4 MiB".into());
                        }
                    }
                }
            }
            events.final_response.ok_or("Responses stream ended before response.completed; no partial function call was executed")?
        } else {
            bounded_json(response).await?
        };
        let message = completed(&value)?;
        let id = value["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Completed Responses payload has no response ID")?;
        self.previous = Some(id.into());
        self.sent = messages.len();
        if !is_sse {
            if let Some((app, id)) = stream_to {
                crate::durable_turn::emit_chunk(app, id, message["content"].as_str().unwrap_or(""))?;
            }
        }
        Ok(Answer {
            message,
            usage: value["usage"].clone(),
            receipt,
            transport,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    pub(super) async fn read_request(socket: &mut tokio::net::TcpStream) -> (String, Value) {
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0u8; 8192];
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..at]).to_string();
                let size: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= at + 4 + size {
                    return (
                        headers,
                        serde_json::from_slice(&bytes[at + 4..at + 4 + size]).unwrap(),
                    );
                }
            }
        }
    }
    fn response(id: &str, output: Value) -> Value {
        json!({"id":id,"status":"completed","output":output,"usage":{"input_tokens":12,"output_tokens":7}})
    }
    fn call() -> Value {
        json!({"type":"function_call","id":"item_1","call_id":"call_1","name":"ping","arguments":"{}"})
    }
    fn text_item(text: &str) -> Value {
        json!({"type":"message","content":[{"type":"output_text","text":text}]})
    }
    fn tool() -> Value {
        json!({"type":"function","function":{"name":"ping","description":"Ping","parameters":{"type":"object","properties":{}}}})
    }

    #[test]
    fn canonical_output_validates_whole_batch_and_deduplicates() {
        let out = completed(&response(
            "r1",
            json!([{"type":"reasoning","id":"reason"},call(),call()]),
        ))
        .unwrap();
        assert_eq!(out["tool_calls"].as_array().unwrap().len(), 1);
        let mut conflicting = call();
        conflicting["arguments"] = json!("{\"x\":1}");
        assert!(completed(&response("r1", json!([call(), conflicting])))
            .unwrap_err()
            .contains("duplicate"));
        let mut invalid = call();
        invalid["arguments"] = json!("{");
        assert!(completed(&response("r1", json!([call(), invalid]))).is_err());
        assert!(completed(&json!({"status":"incomplete","output":[call()]})).is_err());
        let flat = tools(&[tool()]).unwrap();
        assert_eq!(flat[0]["name"], "ping");
        assert_eq!(flat[0]["strict"], false);
        assert!(flat[0].get("function").is_none());
    }

    #[tokio::test]
    async fn native_wire_preserves_reasoning_call_ids_usage_and_split_utf8() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for step in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let (headers, body) = read_request(&mut socket).await;
                assert!(headers.starts_with("POST /v1/responses "));
                assert!(headers
                    .to_lowercase()
                    .contains("authorization: bearer fixture-only"));
                assert_eq!(body["model"], "gpt-6-astra");
                assert_eq!(body["reasoning"]["effort"], "low");
                assert!(body.get("messages").is_none());
                assert!(body.get("reasoning_effort").is_none());
                assert!(body.get("max_tokens").is_none());
                assert_eq!(body["max_output_tokens"], 4096);
                assert_eq!(body["tools"][0]["name"], "ping");
                let result = if step == 0 {
                    assert!(body.get("previous_response_id").is_none());
                    response("r1", json!([{"type":"reasoning","id":"reason"},call()]))
                } else {
                    assert_eq!(body["previous_response_id"], "r1");
                    assert_eq!(
                        body["input"],
                        json!([{"type":"function_call_output","call_id":"call_1","output":"pong"}])
                    );
                    response("r2", json!([text_item("Grüezi 🟢")]))
                };
                let sse = format!(
                    "event: response.completed\r\ndata: {}\r\n\r\n",
                    json!({"type":"response.completed","response":result})
                );
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nX-NautGate-Transport: openai-responses\r\nX-NautGate-Decision-Id: decision-fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",sse.len()).as_bytes()).await.unwrap();
                for chunk in sse.as_bytes().chunks(3) {
                    socket.write_all(chunk).await.unwrap();
                    tokio::task::yield_now().await;
                }
            }
        });
        let llm = crate::settings::LlmSettings {
            endpoint: format!("http://{addr}/v1"),
            api_key: Some("fixture-only".into()),
            ..Default::default()
        };
        let client = reqwest::Client::new();
        let mut session = Session::default();
        let mut messages = vec![json!({"role":"user","content":"Call ping"})];
        let first = session
            .request(
                &client,
                &llm,
                "gpt-6-astra",
                &messages,
                &[tool()],
                Some("none"),
                4096,
                None,
            )
            .await
            .unwrap();
        assert_eq!(first.transport.as_deref(), Some("openai-responses"));
        assert_eq!(
            first.receipt.decision_id.as_deref(),
            Some("decision-fixture")
        );
        messages.push(first.message);
        messages.push(json!({"role":"tool","tool_call_id":"call_1","content":"pong"}));
        let second = session
            .request(
                &client,
                &llm,
                "gpt-6-astra",
                &messages,
                &[tool()],
                None,
                4096,
                None,
            )
            .await
            .unwrap();
        assert_eq!(second.message["content"], "Grüezi 🟢");
        assert_eq!(second.usage["input_tokens"], 12);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn incomplete_failed_and_http_errors_never_retry_or_downgrade() {
        for (status,body) in [
            (200,format!("data: {}\n\ndata: [DONE]\n\n",json!({"type":"response.output_item.done","item":call()}))),
            (200,"data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"fixture failure\"}}}\n\n".into()),
            (502,"{\"error\":{\"message\":\"upstream unavailable\"}}".into()),
        ] {
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();
            let server=tokio::spawn(async move{
                let (mut socket,_)=listener.accept().await.unwrap();let (headers,_)=read_request(&mut socket).await;
                assert!(headers.starts_with("POST /v1/responses "));
                let mime=if status==200{"text/event-stream"}else{"application/json"};
                socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                drop(socket);
                assert!(tokio::time::timeout(std::time::Duration::from_millis(100),listener.accept()).await.is_err());
            });
            let llm=crate::settings::LlmSettings{endpoint:format!("http://{addr}/v1"),..Default::default()};
            let error=Session::default().request(&reqwest::Client::new(),&llm,"gpt-6-astra",&[json!({"role":"user","content":"ping"})],&[tool()],None,4096,None).await.err().expect("must fail closed");
            assert!(error.contains("Responses"));server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn enabled_gateway_failure_never_contacts_direct_provider() {
        let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let direct = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut settings = crate::settings::Settings::default();
        settings.llm = crate::settings::LlmSettings {
            provider: "openai".into(),
            endpoint: format!("http://{}/v1", direct.local_addr().unwrap()),
            api_key: Some("direct-secret".into()),
            model: "gpt-6-astra".into(),
            ..Default::default()
        };
        settings
            .llm_providers
            .push(crate::settings::LlmProviderSettings {
                name: "nautgate".into(),
                endpoint: format!("http://{}/v1", gateway.local_addr().unwrap()),
                api_key: Some("gateway-secret".into()),
                enabled: true,
            });
        let redirect = format!("{}/responses", settings.llm.endpoint);
        let server = tokio::spawn(async move {
            let (mut socket, _) = gateway.accept().await.unwrap();
            let (headers, _) = read_request(&mut socket).await;
            assert!(headers
                .to_lowercase()
                .contains("authorization: bearer gateway-secret"));
            assert!(!headers.contains("direct-secret"));
            socket.write_all(format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {redirect}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        });
        let llm = crate::chat::selected_llm(&settings, "openai").unwrap();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        assert!(Session::default()
            .request(
                &client,
                &llm,
                &llm.model,
                &[json!({"role":"user","content":"Hello"})],
                &[],
                None,
                4096,
                None
            )
            .await
            .is_err());
        server.await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), direct.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "explicit paid native Astra live tool round-trip; synthetic ping only"]
    async fn live_gateway_native_tool_round_trip() {
        let settings = crate::settings::load_or_default();
        let llm = crate::chat::provider_llm(&settings, "nautgate").expect("configured gateway");
        let model = "gpt-6-astra";
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .unwrap();
        let mut session = Session::default();
        let mut messages = vec![
            json!({"role":"user","content":"Call the ping tool exactly once with no arguments. After receiving pong, answer exactly PONG VERIFIED."}),
        ];
        let first = session
            .request(
                &client,
                &llm,
                model,
                &messages,
                &[tool()],
                Some("low"),
                4096,
                None,
            )
            .await
            .unwrap();
        assert_eq!(first.transport.as_deref(), Some("openai-responses"));
        let calls = first.message["tool_calls"].as_array().expect("ping call");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["function"]["name"], "ping");
        let id = calls[0]["id"].as_str().unwrap().to_string();
        println!(
            "LIVE first decision={:?} receipt={:?} usage={}",
            first.receipt.decision_id, first.receipt.receipt_id, first.usage
        );
        messages.push(first.message);
        messages.push(json!({"role":"tool","tool_call_id":id,"content":"pong"}));
        let second = session
            .request(
                &client,
                &llm,
                model,
                &messages,
                &[tool()],
                Some("low"),
                4096,
                None,
            )
            .await
            .unwrap();
        assert_eq!(second.transport.as_deref(), Some("openai-responses"));
        assert!(second.message["content"]
            .as_str()
            .unwrap_or("")
            .contains("PONG VERIFIED"));
        println!(
            "LIVE continuation decision={:?} receipt={:?} usage={} answer={}",
            second.receipt.decision_id,
            second.receipt.receipt_id,
            second.usage,
            second.message["content"]
        );
    }
}
