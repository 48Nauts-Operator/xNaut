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
}

/// System information for context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub shell: String,
    pub working_directory: String,
}

/// AI response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub response: String,
    pub suggestions: Vec<String>,
    pub commands: Vec<String>,
    pub confidence: f32,
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
            http_client: reqwest::Client::new(),
        }
    }

    /// Sends a request to the AI provider
    pub async fn ask(&self, request: AiRequest) -> Result<AiResponse> {
        match self.config.provider {
            AiProvider::OpenAi => self.ask_openai(request).await,
            AiProvider::Anthropic => self.ask_anthropic(request).await,
            AiProvider::Custom => self.ask_custom(request).await,
        }
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

        let system_message = "You are an expert terminal assistant. Provide helpful, accurate command suggestions and error analysis.";

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
        })
    }

    /// Anthropic API implementation
    async fn ask_anthropic(&self, request: AiRequest) -> Result<AiResponse> {
        let url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com/v1/messages");

        let system_prompt = "You are an expert terminal assistant. Provide helpful, accurate command suggestions and error analysis.";

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
        })
    }

    /// Custom API implementation (OpenAI-compatible format for Perplexity, OpenRouter)
    async fn ask_custom(&self, request: AiRequest) -> Result<AiResponse> {
        let url = self
            .config
            .base_url
            .as_ref()
            .context("Custom provider requires base_url")?;

        let system_message = "You are an expert terminal assistant. Provide helpful, accurate command suggestions and error analysis.";

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
}
