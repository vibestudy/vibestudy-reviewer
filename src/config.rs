use crate::error::ConfigError;
use secrecy::SecretString;

#[derive(Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub providers: ProvidersConfig,
    pub review: ReviewConfig,
}

#[derive(Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub cors_origins: Vec<String>,
}

#[derive(Clone)]
pub struct ProvidersConfig {
    pub openai_api_key: Option<SecretString>,
    pub anthropic_api_key: Option<SecretString>,
    pub opencode_api_key: Option<SecretString>,
    pub opencode_base_url: Option<String>,
    pub default_timeout_secs: u64,
}

#[derive(Clone)]
pub struct ReviewConfig {
    pub max_concurrent_checks: usize,
    pub review_ttl_secs: u64,
    pub max_repo_size_mb: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            server: ServerConfig {
                host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
                port: std::env::var("PORT")
                    .unwrap_or_else(|_| "8080".to_string())
                    .parse()
                    .map_err(|_| ConfigError::InvalidValue("PORT".into()))?,
                cors_origins: std::env::var("CORS_ORIGINS")
                    .unwrap_or_else(|_| "*".to_string())
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect(),
            },
            providers: ProvidersConfig {
                openai_api_key: std::env::var("OPENAI_API_KEY").ok().map(SecretString::from),
                anthropic_api_key: std::env::var("ANTHROPIC_API_KEY")
                    .ok()
                    .map(SecretString::from),
                opencode_api_key: std::env::var("OPENCODE_API_KEY")
                    .ok()
                    .map(SecretString::from),
                opencode_base_url: std::env::var("OPENCODE_BASE_URL").ok(),
                default_timeout_secs: std::env::var("LLM_TIMEOUT_SECS")
                    .unwrap_or_else(|_| "120".to_string())
                    .parse()
                    .unwrap_or(120),
            },
            review: ReviewConfig {
                max_concurrent_checks: std::env::var("MAX_CONCURRENT_CHECKS")
                    .unwrap_or_else(|_| "4".to_string())
                    .parse()
                    .unwrap_or(4),
                review_ttl_secs: std::env::var("REVIEW_TTL_SECS")
                    .unwrap_or_else(|_| "3600".to_string())
                    .parse()
                    .unwrap_or(3600),
                max_repo_size_mb: 100,
            },
        })
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            cors_origins: vec!["*".to_string()],
        }
    }
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            openai_api_key: None,
            anthropic_api_key: None,
            opencode_api_key: None,
            opencode_base_url: None,
            default_timeout_secs: 120,
        }
    }
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            max_concurrent_checks: 4,
            review_ttl_secs: 3600,
            max_repo_size_mb: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let server = ServerConfig::default();
        assert_eq!(server.port, 8080);
        assert_eq!(server.host, "0.0.0.0");
    }

    #[test]
    fn test_providers_config_default() {
        let providers = ProvidersConfig::default();
        assert!(providers.openai_api_key.is_none());
        assert_eq!(providers.default_timeout_secs, 120);
    }
}
