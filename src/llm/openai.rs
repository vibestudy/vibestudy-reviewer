use crate::error::LlmError;
use crate::llm::{Message, ModelClient, Role};
use async_trait::async_trait;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";
const OPENAI_OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const TOKEN_REFRESH_BUFFER_SECS: i64 = 300;

enum CredentialSource {
    ApiKey(SecretString),
    OAuth {
        access_token: SecretString,
        refresh_token: Option<SecretString>,
        expires_at: Option<i64>,
    },
}

pub struct OpenAIClient {
    client: Client,
    credential: Arc<RwLock<CredentialSource>>,
    refresh_lock: Arc<Mutex<()>>,
    model: String,
}

impl OpenAIClient {
    pub fn with_api_key(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            credential: Arc::new(RwLock::new(CredentialSource::ApiKey(SecretString::from(
                api_key.into(),
            )))),
            refresh_lock: Arc::new(Mutex::new(())),
            model: "gpt-4o".to_string(),
        }
    }

    pub fn with_oauth(
        access_token: impl Into<String>,
        refresh_token: Option<String>,
        expires_at: Option<i64>,
    ) -> Self {
        Self {
            client: Client::new(),
            credential: Arc::new(RwLock::new(CredentialSource::OAuth {
                access_token: SecretString::from(access_token.into()),
                refresh_token: refresh_token.map(SecretString::from),
                expires_at,
            })),
            refresh_lock: Arc::new(Mutex::new(())),
            model: "gpt-4o".to_string(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    fn is_token_expired(expires_at: Option<i64>) -> bool {
        match expires_at {
            Some(exp) => {
                let now = chrono::Utc::now().timestamp();
                now >= (exp - TOKEN_REFRESH_BUFFER_SECS)
            }
            None => false,
        }
    }

    async fn refresh_token_if_needed(&self) -> Result<(), LlmError> {
        let might_need_refresh = {
            let credential = self.credential.read().await;
            if let CredentialSource::OAuth { expires_at, .. } = &*credential {
                Self::is_token_expired(*expires_at)
            } else {
                false
            }
        };

        if !might_need_refresh {
            return Ok(());
        }

        let _guard = self.refresh_lock.lock().await;

        let (needs_refresh, refresh_token_opt) = {
            let credential = self.credential.read().await;
            if let CredentialSource::OAuth {
                refresh_token,
                expires_at,
                ..
            } = &*credential
            {
                (Self::is_token_expired(*expires_at), refresh_token.clone())
            } else {
                (false, None)
            }
        };

        if needs_refresh {
            if let Some(refresh_token) = refresh_token_opt {
                self.do_token_refresh(&refresh_token).await?;
            } else {
                return Err(LlmError::TokenExpired);
            }
        }

        Ok(())
    }

    async fn do_token_refresh(&self, refresh_token: &SecretString) -> Result<(), LlmError> {
        let response = self
            .client
            .post(OPENAI_OAUTH_TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.expose_secret()),
                ("client_id", CODEX_CLIENT_ID),
            ])
            .send()
            .await
            .map_err(LlmError::Network)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::AuthenticationFailed(format!(
                "Token refresh failed ({}): {}",
                status, body
            )));
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: Option<i64>,
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("Invalid token response: {}", e)))?;

        let expires_at = token_response
            .expires_in
            .map(|exp| chrono::Utc::now().timestamp() + exp);

        let mut credential = self.credential.write().await;
        *credential = CredentialSource::OAuth {
            access_token: SecretString::from(token_response.access_token),
            refresh_token: token_response
                .refresh_token
                .map(SecretString::from)
                .or_else(|| Some(refresh_token.clone())),
            expires_at,
        };

        Ok(())
    }

    async fn get_auth_header(&self) -> Result<String, LlmError> {
        let credential = self.credential.read().await;
        match &*credential {
            CredentialSource::ApiKey(key) => Ok(format!("Bearer {}", key.expose_secret())),
            CredentialSource::OAuth { access_token, .. } => {
                Ok(format!("Bearer {}", access_token.expose_secret()))
            }
        }
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
impl ModelClient for OpenAIClient {
    async fn chat(&self, messages: &[Message], system: Option<&str>) -> Result<String, LlmError> {
        self.refresh_token_if_needed().await?;

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

        let auth_header = self.get_auth_header().await?;

        let response = self
            .client
            .post(OPENAI_API_URL)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(LlmError::Network)?;

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
