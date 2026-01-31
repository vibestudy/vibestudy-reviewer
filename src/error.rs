use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("review not found: {0}")]
    NotFound(String),

    #[error("invalid request: {0}")]
    BadRequest(String),

    #[error("git error: {0}")]
    GitError(String),

    #[error("checker error: {0}")]
    CheckerError(String),

    #[error("internal error: {0}")]
    InternalError(String),
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("rate limit exceeded: retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("context window exceeded: {used} tokens used, {limit} limit")]
    ContextExceeded { used: u64, limit: u64 },

    #[error("content filtered: {reason}")]
    ContentFiltered { reason: String },

    #[error("model not found: {model}")]
    ModelNotFound { model: String },

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("stream error: {0}")]
    StreamError(String),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("provider unavailable: {provider}")]
    Unavailable { provider: String },

    #[error("token expired")]
    TokenExpired,
}

impl LlmError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Network(_) | Self::Unavailable { .. }
        )
    }

    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_ms } => Some(*retry_after_ms),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required config: {0}")]
    MissingRequired(String),

    #[error("invalid value for {0}")]
    InvalidValue(String),
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::GitError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::CheckerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let code = match self {
            ApiError::NotFound(_) => "NOT_FOUND",
            ApiError::BadRequest(_) => "BAD_REQUEST",
            ApiError::GitError(_) => "GIT_ERROR",
            ApiError::CheckerError(_) => "CHECKER_ERROR",
            ApiError::InternalError(_) => "INTERNAL_ERROR",
        };
        HttpResponse::build(self.status_code()).json(ErrorResponse {
            error: self.to_string(),
            code: code.to_string(),
            details: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_error_retryable() {
        let rate_limited = LlmError::RateLimited {
            retry_after_ms: 1000,
        };
        assert!(rate_limited.is_retryable());
        assert_eq!(rate_limited.retry_after_ms(), Some(1000));

        let auth_failed = LlmError::AuthenticationFailed("bad token".to_string());
        assert!(!auth_failed.is_retryable());
        assert_eq!(auth_failed.retry_after_ms(), None);
    }

    #[test]
    fn test_api_error_status_codes() {
        use actix_web::ResponseError;

        let not_found = ApiError::NotFound("review_123".to_string());
        assert_eq!(not_found.status_code(), StatusCode::NOT_FOUND);

        let bad_request = ApiError::BadRequest("missing field".to_string());
        assert_eq!(bad_request.status_code(), StatusCode::BAD_REQUEST);
    }
}
