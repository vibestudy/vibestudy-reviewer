use crate::error::LlmError;
use std::time::Duration;
use tokio::time::sleep;

const MAX_RETRIES: u32 = 3;
const BASE_DELAY_MS: u64 = 1000;
const MAX_DELAY_MS: u64 = 60000;

pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: MAX_RETRIES,
            base_delay_ms: BASE_DELAY_MS,
            max_delay_ms: MAX_DELAY_MS,
        }
    }
}

pub async fn with_retry<T, F, Fut>(config: &RetryConfig, mut operation: F) -> Result<T, LlmError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, LlmError>>,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if e.is_retryable() && attempt < config.max_retries => {
                attempt += 1;

                let delay_ms = e.retry_after_ms().unwrap_or_else(|| {
                    std::cmp::min(
                        config.base_delay_ms * 2u64.pow(attempt - 1),
                        config.max_delay_ms,
                    )
                });

                tracing::warn!(
                    "Request failed (attempt {}/{}), retrying in {}ms: {}",
                    attempt,
                    config.max_retries,
                    delay_ms,
                    e
                );

                sleep(Duration::from_millis(delay_ms)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_succeeds_first_try() {
        let config = RetryConfig::default();
        let result = with_retry(&config, || async { Ok::<_, LlmError>("success".to_string()) }).await;
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let config = RetryConfig {
            max_retries: 3,
            base_delay_ms: 10,
            max_delay_ms: 100,
        };
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = with_retry(&config, || {
            let attempts = attempts_clone.clone();
            async move {
                let count = attempts.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(LlmError::RateLimited {
                        retry_after_ms: 10,
                    })
                } else {
                    Ok("success".to_string())
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let config = RetryConfig {
            max_retries: 2,
            base_delay_ms: 10,
            max_delay_ms: 100,
        };

        let result: Result<String, LlmError> = with_retry(&config, || async {
            Err(LlmError::RateLimited {
                retry_after_ms: 10,
            })
        })
        .await;

        assert!(result.is_err());
    }
}
