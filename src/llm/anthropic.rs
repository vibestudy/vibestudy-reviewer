use crate::error::LlmError;
use crate::llm::{Message, ModelClient, Role};
use async_trait::async_trait;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const OAUTH_BETA_FEATURES: &str = "oauth-2025-04-20,interleaved-thinking-2025-05-14";
const OAUTH_USER_AGENT: &str = "claude-cli/2.1.2 (external, cli)";
const TOOL_PREFIX: &str = "mcp_";
const CLAUDE_CODE_IDENTITY: &str = "You are Claude Code, Anthropic's official CLI for Claude.";

enum AuthMode {
    ApiKey(SecretString),
    OAuth { access_token: SecretString },
}

pub struct AnthropicClient {
    client: Client,
    auth: AuthMode,
    model: String,
    base_url: String,
}

impl AnthropicClient {
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            auth: AuthMode::ApiKey(SecretString::from(api_key.into())),
            model: "claude-sonnet-4-20250514".to_string(),
            base_url: ANTHROPIC_API_URL.to_string(),
        }
    }

    pub fn with_oauth(access_token: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            auth: AuthMode::OAuth {
                access_token: SecretString::from(access_token.into()),
            },
            model: "claude-sonnet-4-20250514".to_string(),
            base_url: ANTHROPIC_API_URL.to_string(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    fn is_oauth(&self) -> bool {
        matches!(self.auth, AuthMode::OAuth { .. })
    }

    fn get_endpoint(&self) -> String {
        if self.is_oauth() {
            format!("{}?beta=true", self.base_url)
        } else {
            self.base_url.clone()
        }
    }

    fn sanitize_for_oauth(text: &str) -> String {
        text.replace("OpenCode", "Claude Code")
            .replace("opencode", "Claude")
    }
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<SystemPrompt>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum SystemPrompt {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

#[derive(Serialize)]
struct SystemBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: String,
    cache_control: CacheControl,
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    cache_type: String,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
}

#[async_trait]
impl ModelClient for AnthropicClient {
    async fn chat(&self, messages: &[Message], system: Option<&str>) -> Result<String, LlmError> {
        let system_prompt = if self.is_oauth() {
            let mut blocks = vec![SystemBlock {
                block_type: "text".to_string(),
                text: CLAUDE_CODE_IDENTITY.to_string(),
                cache_control: CacheControl {
                    cache_type: "ephemeral".to_string(),
                },
            }];

            if let Some(sys) = system {
                let sanitized = Self::sanitize_for_oauth(sys);
                blocks.push(SystemBlock {
                    block_type: "text".to_string(),
                    text: sanitized,
                    cache_control: CacheControl {
                        cache_type: "ephemeral".to_string(),
                    },
                });
            }
            Some(SystemPrompt::Blocks(blocks))
        } else {
            system.map(|s| SystemPrompt::Text(s.to_string()))
        };

        let api_messages: Vec<ApiMessage> = messages
            .iter()
            .filter(|m| !matches!(m.role, Role::System))
            .map(|m| ApiMessage {
                role: match m.role {
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::System => "user".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let request = ApiRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            messages: api_messages,
            system: system_prompt,
        };

        let mut req_builder = self
            .client
            .post(self.get_endpoint())
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("Content-Type", "application/json");

        match &self.auth {
            AuthMode::ApiKey(key) => {
                req_builder = req_builder.header("x-api-key", key.expose_secret());
            }
            AuthMode::OAuth { access_token } => {
                req_builder = req_builder
                    .header(
                        "Authorization",
                        format!("Bearer {}", access_token.expose_secret()),
                    )
                    .header("anthropic-beta", OAUTH_BETA_FEATURES)
                    .header("anthropic-product", "claude-code")
                    .header("user-agent", OAUTH_USER_AGENT);
            }
        }

        let response = req_builder.json(&request).send().await.map_err(LlmError::Network)?;

        let status = response.status();
        if status.as_u16() == 429 {
            return Err(LlmError::RateLimited {
                retry_after_ms: 60000,
            });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::InvalidResponse(format!(
                "API error ({}): {}",
                status, body
            )));
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("Invalid response: {}", e)))?;

        let text = api_response
            .content
            .iter()
            .filter(|b| b.block_type == "text")
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("");

        if text.is_empty() {
            Err(LlmError::InvalidResponse(
                "No text content in response".to_string(),
            ))
        } else {
            Ok(text)
        }
    }
}

pub fn prefix_tool_name(name: &str) -> String {
    format!("{}{}", TOOL_PREFIX, name)
}

pub fn strip_tool_prefix(name: &str) -> String {
    name.strip_prefix(TOOL_PREFIX)
        .unwrap_or(name)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_for_oauth() {
        let input = "This is OpenCode running opencode commands";
        let output = AnthropicClient::sanitize_for_oauth(input);
        assert_eq!(output, "This is Claude Code running Claude commands");
    }

    #[test]
    fn test_tool_prefix() {
        assert_eq!(prefix_tool_name("read_file"), "mcp_read_file");
        assert_eq!(strip_tool_prefix("mcp_read_file"), "read_file");
        assert_eq!(strip_tool_prefix("read_file"), "read_file");
    }
}
