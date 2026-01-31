mod retry;
mod tokens;

pub mod anthropic;
pub mod openai;
pub mod opencode;

pub use retry::{with_retry, RetryConfig};
pub use tokens::OAuthTokens;

use crate::error::LlmError;
use async_trait::async_trait;

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn chat(&self, messages: &[Message], system: Option<&str>) -> Result<String, LlmError>;
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }
}
