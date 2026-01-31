use chrono::Utc;
use serde::{Deserialize, Serialize};

const TOKEN_REFRESH_BUFFER_SECS: i64 = 300;

#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub token_type: String,
}

impl OAuthTokens {
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(exp) => {
                let now = Utc::now().timestamp();
                now >= (exp - TOKEN_REFRESH_BUFFER_SECS)
            }
            None => false,
        }
    }

    pub fn new(
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<i64>,
    ) -> Self {
        let expires_at = expires_in.map(|exp| Utc::now().timestamp() + exp);
        Self {
            access_token,
            refresh_token,
            expires_at,
            token_type: "Bearer".to_string(),
        }
    }
}

impl std::fmt::Debug for OAuthTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthTokens")
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("token_type", &self.token_type)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_not_expired() {
        let token = OAuthTokens::new("test".to_string(), None, Some(3600));
        assert!(!token.is_expired());
    }

    #[test]
    fn test_token_expired() {
        let mut token = OAuthTokens::new("test".to_string(), None, Some(3600));
        token.expires_at = Some(Utc::now().timestamp() - 100);
        assert!(token.is_expired());
    }

    #[test]
    fn test_token_no_expiry() {
        let token = OAuthTokens {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
        };
        assert!(!token.is_expired());
    }
}
