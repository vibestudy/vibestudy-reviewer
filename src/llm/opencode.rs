use crate::error::LlmError;
use crate::llm::{Message, ModelClient, Role};
use async_trait::async_trait;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

const DEFAULT_OPENCODE_URL: &str = "http://localhost:8000/v1/chat/completions";

pub struct OpenCodeClient {
    client: Client,
    api_key: Option<SecretString>,
    base_url: String,
    model: String,
}

impl OpenCodeClient {
    pub fn new(base_url: Option<String>, api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.map(SecretString::from),
            base_url: base_url.unwrap_or_else(|| DEFAULT_OPENCODE_URL.to_string()),
            model: "default".to_string(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

#[async_trait]
impl ModelClient for OpenCodeClient {
    async fn chat(&self, messages: &[Message], system: Option<&str>) -> Result<String, LlmError> {
        let mut chat_messages: Vec<ChatMessage> = Vec::new();

        if let Some(sys) = system {
            chat_messages.push(ChatMessage {
                role: "system".to_string(),
                content: sys.to_string(),
            });
        }

        for msg in messages {
            let role = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            chat_messages.push(ChatMessage {
                role: role.to_string(),
                content: msg.content.clone(),
            });
        }

        let request = ChatRequest {
            model: self.model.clone(),
            messages: chat_messages,
        };

        let mut req_builder = self
            .client
            .post(&self.base_url)
            .header("Content-Type", "application/json");

        if let Some(api_key) = &self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key.expose_secret()));
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

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("Invalid response: {}", e)))?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| LlmError::InvalidResponse("No choices in response".to_string()))
    }
}
