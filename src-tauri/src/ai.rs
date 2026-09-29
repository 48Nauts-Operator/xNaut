// ABOUTME: AI integration module for terminal assistance using LLM APIs (OpenAI, Anthropic, etc.).
// ABOUTME: Provides command suggestions, error analysis, and natural language terminal interaction.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// AI provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub provider: AiProvider,
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,
}

/// Supported AI providers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    OpenAi,
    Anthropic,
    Custom,
}

/// AI request for terminal assistance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub prompt: String,
    pub context: Option<String>,
    pub terminal_output: Option<String>,
    pub system_info: Option<SystemInfo>,
    /// Replaces the terminal-assistant system message for callers that are not
    /// asking about a terminal. The Researcher (XNAUT-356) asks Perplexity for
    /// precedent and market context, and "you are an expert terminal assistant"
    /// is the wrong instruction for that question. `None` keeps the old text, so
    /// every existing caller is unchanged.
    #[serde(default)]
    pub system: Option<String>,
}

/// System information for context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub shell: String,
    pub working_directory: String,
}

/// One external source an answer stood on, as the provider reported it.
///
/// A research answer without its sources is an assertion, which is the thing
/// the Researcher exists not to produce (XNAUT-356). `title` may be empty —
/// Perplexity's older `citations` array is bare URLs — and the URL is what is
/// load-bearing, so a source with no URL is dropped rather than rendered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    #[serde(default)]
    pub title: String,
    pub url: String,
}

/// AI response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub response: String,
    pub suggestions: Vec<String>,
    pub commands: Vec<String>,
    pub confidence: f32,
    /// Sources the provider says it read. Empty for providers that do not
    /// search (OpenAI, Anthropic); populated for Perplexity.
    #[serde(default)]
    pub citations: Vec<Citation>,
}

/// AI service client
pub struct AiClient {
    config: AiConfig,
    http_client: reqwest::Client,
}

impl AiClient {
    /// Creates a new AI client with configuration
    pub fn new(config: AiConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).connect_timeout(std::time::Duration::from_secs(15)).timeout(std::time::Duration::from_secs(300)).build().expect("HTTP client"),
        }
    }

    /// Sends a request to the AI provider
    pub async fn ask(&self, request: AiRequest) -> Result<AiResponse> {
        let config = route_config(&crate::settings::load_or_default(), &self.config)?;
        let routed = Self { config, http_client: self.http_client.clone() };
        if crate::responses::required(&routed.config.model) {
            return routed.ask_responses(request).await;
        }
        match routed.config.provider {
            AiProvider::OpenAi => routed.ask_openai(request).await,
            AiProvider::Anthropic => routed.ask_anthropic(request).await,
            AiProvider::Custom => routed.ask_custom(request).await,
        }
    }

    async fn ask_responses(&self, request: AiRequest) -> Result<AiResponse> {
        anyhow::ensure!(!matches!(self.config.provider, AiProvider::Anthropic), "Astra requires a Responses-compatible provider; select OpenAI or enable NautGate");
        let endpoint = self.config.base_url.as_deref().unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/').trim_end_matches("/chat/completions");
        let llm = crate::settings::LlmSettings { endpoint:endpoint.into(), model:self.config.model.clone(), api_key:Some(self.config.api_key.clone()), ..Default::default() };
        let user = request.context.map(|context| format!("{}\n\nContext: {context}", request.prompt)).unwrap_or(request.prompt);
        let input = vec![serde_json::json!({"role":"system","content":request.system.unwrap_or_else(default_system_prompt)}),serde_json::json!({"role":"user","content":user})];
        let answer = crate::responses::Session::default().request(&self.http_client,&llm,&llm.model,&input,&[],None,8192,None).await.map_err(anyhow::Error::msg)?;
        let content = answer.message["content"].as_str().unwrap_or("").to_string();
        Ok(AiResponse { suggestions:extract_suggestions(&content), commands:extract_commands(&content), response:content, confidence:0.8, citations:Vec::new() })
    }

    /// Analyzes terminal output for errors and suggestions
    #[allow(dead_code)]
    pub async fn analyze_output(&self, output: &str) -> Result<AiResponse> {
        let request = AiRequest {
            prompt: format!(
                "Analyze this terminal output and provide suggestions:\n\n{}",
                output
            ),
            context: Some("Terminal output analysis".to_string()),
            terminal_output: Some(output.to_string()),
            system_info: None,
            system: None,
        };

        self.ask(request).await
    }

    /// Suggests commands based on natural language
    #[allow(dead_code)]
    pub async fn suggest_command(
        &self,
        intent: &str,
        context: Option<String>,
    ) -> Result<AiResponse> {
        let request = AiRequest {
            prompt: format!("Suggest a terminal command for: {}", intent),
            context,
            terminal_output: None,
            system_info: Self::get_system_info(),
            system: None,
        };

        self.ask(request).await
    }

    /// OpenAI API implementation
    async fn ask_openai(&self, request: AiRequest) -> Result<AiResponse> {
        let url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com/v1/chat/completions");

        let system_message = request.system.clone().unwrap_or_else(default_system_prompt);

        let user_message = if let Some(context) = request.context {
            format!("{}\n\nContext: {}", request.prompt, context)
        } else {
            request.prompt.clone()
        };

        let response = self
            .http_client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.config.model,
                "messages": [
                    {"role": "system", "content": system_message},
                    {"role": "user", "content": user_message}
                ],
                "temperature": 0.7,
            }))
            .send()
            .await
            .context("Failed to send request to OpenAI")?;

        let response_data: serde_json::Value = response
            .error_for_status().context("Model endpoint rejected the request")?
            .json()
            .await
            .context("Failed to parse OpenAI response")?;

        let content = response_data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(AiResponse {
            response: content.clone(),
            suggestions: extract_suggestions(&content),
            commands: extract_commands(&content),
            confidence: 0.8,
            citations: Vec::new(),
        })
    }

    /// Anthropic API implementation
    async fn ask_anthropic(&self, request: AiRequest) -> Result<AiResponse> {
        let url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com/v1/messages");

        let system_prompt = request.system.clone().unwrap_or_else(default_system_prompt);

        let user_message = if let Some(context) = request.context {
            format!("{}\n\nContext: {}", request.prompt, context)
        } else {
            request.prompt.clone()
        };

        let response = self
            .http_client
            .post(url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.config.model,
                "system": system_prompt,
                "messages": [
                    {"role": "user", "content": user_message}
                ],
                "max_tokens": 1024,
            }))
            .send()
            .await
            .context("Failed to send request to Anthropic")?;

        let response_data: serde_json::Value = response
            .error_for_status().context("Model endpoint rejected the request")?
            .json()
            .await
            .context("Failed to parse Anthropic response")?;

        let content = response_data["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(AiResponse {
            response: content.clone(),
            suggestions: extract_suggestions(&content),
            commands: extract_commands(&content),
            confidence: 0.8,
            citations: Vec::new(),
        })
    }

    /// Custom API implementation (OpenAI-compatible format for Perplexity, OpenRouter)
    async fn ask_custom(&self, request: AiRequest) -> Result<AiResponse> {
        let url = self
            .config
            .base_url
            .as_ref()
            .context("Custom provider requires base_url")?;

        let system_message = request.system.clone().unwrap_or_else(default_system_prompt);

        let user_message = if let Some(context) = request.context {
            format!("{}\n\nContext: {}", request.prompt, context)
        } else {
            request.prompt.clone()
        };

        // Use OpenAI-compatible format (Perplexity, OpenRouter support this)
        let response = self
            .http_client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.config.model,
                "messages": [
                    {"role": "system", "content": system_message},
                    {"role": "user", "content": user_message}
                ],
                "temperature": 0.7,
            }))
            .send()
            .await
            .context("Failed to send request to custom provider")?;

        let response_data: serde_json::Value = response
            .error_for_status().context("Model endpoint rejected the request")?
            .json()
            .await
            .context("Failed to parse custom provider response")?;

        // Parse OpenAI-compatible response
        let content = response_data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(AiResponse {
            response: content.clone(),
            suggestions: extract_suggestions(&content),
            commands: extract_commands(&content),
            confidence: 0.8,
            citations: extract_citations(&response_data),
        })
    }

    /// Gets current system information
    #[allow(dead_code)]
    fn get_system_info() -> Option<SystemInfo> {
        Some(SystemInfo {
            os: std::env::consts::OS.to_string(),
            shell: std::env::var("SHELL").unwrap_or_else(|_| "unknown".to_string()),
            working_directory: std::env::current_dir().ok()?.to_string_lossy().to_string(),
        })
    }
}

fn route_config(settings: &crate::settings::Settings, config: &AiConfig) -> Result<AiConfig> {
    if !crate::chat::gateway_enabled(settings) { return Ok(config.clone()); }
    let gateway = crate::chat::selected_llm(settings, "nautgate").map_err(anyhow::Error::msg)?;
    Ok(AiConfig { provider:AiProvider::Custom, model:config.model.clone(), api_key:gateway.api_key.unwrap_or_default(), base_url:Some(crate::chat::join_endpoint(&gateway.endpoint,"chat/completions")) })
}

fn default_system_prompt() -> String {
    "You are an expert terminal assistant. Provide helpful, accurate command suggestions and error analysis.".to_string()
}

/// Pulls the sources out of an OpenAI-compatible response body.
///
/// Perplexity answers in two shapes and has shipped both at once: the newer
/// `search_results` carries `{title, url, date}` per source, the older
/// `citations` is a bare array of URLs. Titles are what make a Sources section
/// readable, so `search_results` wins where both are present, and `citations`
/// fills in the rest rather than being ignored — an install pinned to an older
/// model would otherwise cite nothing and look like a key problem.
///
/// Deduplicated by URL, because the two arrays overlap by design. A source with
/// no URL is dropped: it cannot be checked, and an uncheckable citation is
/// worse than none.
fn extract_citations(body: &serde_json::Value) -> Vec<Citation> {
    let mut out: Vec<Citation> = Vec::new();
    let mut push = |title: &str, url: &str| {
        let url = url.trim();
        if url.is_empty() || out.iter().any(|existing| existing.url == url) {
            return;
        }
        out.push(Citation {
            title: title.trim().to_string(),
            url: url.to_string(),
        });
    };

    if let Some(results) = body["search_results"].as_array() {
        for result in results {
            push(
                result["title"].as_str().unwrap_or(""),
                result["url"].as_str().unwrap_or(""),
            );
        }
    }
    if let Some(citations) = body["citations"].as_array() {
        for citation in citations {
            match citation {
                serde_json::Value::String(url) => push("", url),
                other => push(
                    other["title"].as_str().unwrap_or(""),
                    other["url"].as_str().unwrap_or(""),
                ),
            }
        }
    }
    out
}

/// Extracts command suggestions from AI response
fn extract_suggestions(text: &str) -> Vec<String> {
    // Simple extraction - look for numbered lists or bullet points
    text.lines()
        .filter(|line| {
            line.trim().starts_with('-')
                || line.trim().starts_with('•')
                || line.starts_with(|c: char| c.is_numeric())
        })
        .map(|line| {
            line.trim()
                .trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '-' || c == '•')
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Extracts shell commands from AI response (commands in backticks)
fn extract_commands(text: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_code_block = false;
    let mut current_command = String::new();

    for line in text.lines() {
        if line.trim().starts_with("```") {
            if in_code_block && !current_command.is_empty() {
                commands.push(current_command.trim().to_string());
                current_command.clear();
            }
            in_code_block = !in_code_block;
        } else if in_code_block {
            current_command.push_str(line);
            current_command.push('\n');
        } else if line.contains('`') {
            // Extract inline code
            for part in line.split('`').skip(1).step_by(2) {
                if !part.trim().is_empty() {
                    commands.push(part.trim().to_string());
                }
            }
        }
    }

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn responses_rejects_anthropic_credentials_before_any_request() {
        let client=AiClient::new(AiConfig{provider:AiProvider::Anthropic,model:"gpt-6-astra".into(),api_key:"anthropic-fixture".into(),base_url:None});
        let error=client.ask_responses(AiRequest{prompt:"Hello".into(),context:None,terminal_output:None,system_info:None,system:None}).await.unwrap_err();
        assert!(error.to_string().contains("Responses-compatible provider"));
    }

    #[test]
    fn gateway_also_owns_terminal_and_research_requests_without_direct_key_reuse() {
        let config=AiConfig{provider:AiProvider::Custom,model:"sonar-pro".into(),api_key:"direct-key".into(),base_url:Some("https://direct.invalid/chat/completions".into())};
        let mut settings=crate::settings::Settings::default();
        settings.llm_providers.push(crate::settings::LlmProviderSettings{name:"nautgate".into(),endpoint:"http://gateway.invalid/v1".into(),api_key:Some("gateway-key".into()),enabled:true});
        let routed=route_config(&settings,&config).unwrap();
        assert_eq!(routed.api_key,"gateway-key");assert_eq!(routed.base_url.as_deref(),Some("http://gateway.invalid/v1/chat/completions"));assert_eq!(routed.model,"sonar-pro");
        settings.llm_providers[0].enabled=false;
        let direct=route_config(&settings,&config).unwrap();assert_eq!(direct.api_key,"direct-key");assert_eq!(direct.base_url,config.base_url);
    }

    #[test]
    fn test_extract_commands() {
        let text = "You can use `ls -la` to list files or run:\n```\nfind . -name '*.rs'\n```";
        let commands = extract_commands(text);
        assert_eq!(commands.len(), 2);
        assert!(commands.contains(&"ls -la".to_string()));
    }

    #[test]
    fn test_extract_suggestions() {
        let text = "Here are suggestions:\n- First suggestion\n- Second suggestion\n• Third one";
        let suggestions = extract_suggestions(text);
        assert_eq!(suggestions.len(), 3);
    }

    /// Both of Perplexity's shapes, in one body, as it actually answers.
    #[test]
    fn sources_come_back_titled_and_deduplicated() {
        let body = serde_json::json!({
            "choices": [{"message": {"content": "…"}}],
            "search_results": [
                {"title": "BMAD method", "url": "https://example.com/bmad", "date": "2026-01-02"},
                {"title": "", "url": "  "},
            ],
            "citations": ["https://example.com/bmad", "https://example.com/precedent"],
        });
        assert_eq!(
            extract_citations(&body),
            vec![
                Citation {
                    title: "BMAD method".into(),
                    url: "https://example.com/bmad".into()
                },
                Citation {
                    title: String::new(),
                    url: "https://example.com/precedent".into()
                },
            ]
        );
    }

    /// A provider that does not search says nothing about sources, and that is
    /// not an error — it is how the Researcher tells "no sources" from "wrong
    /// provider".
    #[test]
    fn a_body_with_no_sources_cites_nothing() {
        let body = serde_json::json!({"choices": [{"message": {"content": "hi"}}]});
        assert!(extract_citations(&body).is_empty());
    }
}
