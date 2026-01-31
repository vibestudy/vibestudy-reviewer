use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, Duration};

use crate::ai::validators::{CommentValidator, Prioritizer, TypoValidator};
use crate::ai::reviewers::{CodeOracle, ProductIdeasReviewer};
use crate::ai::{CodeContext, Reviewer, Validator};
use crate::checkers::run_all_checkers;
use crate::config::ProvidersConfig;
use crate::error::ApiError;
use crate::git::ClonedRepo;
use crate::llm::openai::OpenAIClient;
use crate::llm::anthropic::AnthropicClient;
use crate::llm::opencode::OpenCodeClient;
use crate::llm::ModelClient;
use crate::types::{Diagnostic, ReviewEvent, ReviewStatus, ReviewSummary, SeverityCounts, Suggestion};
use secrecy::ExposeSecret;

pub struct ReviewState {
    pub id: String,
    pub status: ReviewStatus,
    pub repo_url: String,
    pub results: Vec<Diagnostic>,
    pub suggestions: Vec<Suggestion>,
    pub created_at: u64,
    event_sender: broadcast::Sender<ReviewEvent>,
}

impl ReviewState {
    pub fn new(id: String, repo_url: String) -> Self {
        let (event_sender, _) = broadcast::channel(100);
        Self {
            id,
            status: ReviewStatus::Pending,
            repo_url,
            results: Vec::new(),
            suggestions: Vec::new(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            event_sender,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ReviewEvent> {
        self.event_sender.subscribe()
    }

    pub fn emit(&self, event: ReviewEvent) {
        let _ = self.event_sender.send(event);
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct ReviewStore {
    reviews: Arc<RwLock<HashMap<String, ReviewState>>>,
    ttl_secs: u64,
    providers_config: Option<ProvidersConfig>,
}

impl ReviewStore {
    pub fn new(ttl_secs: u64, providers_config: Option<ProvidersConfig>) -> Self {
        let store = Self {
            reviews: Arc::new(RwLock::new(HashMap::new())),
            ttl_secs,
            providers_config,
        };

        let reviews = store.reviews.clone();
        let ttl = ttl_secs;
        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(60));
            loop {
                cleanup_interval.tick().await;
                Self::cleanup_expired(&reviews, ttl).await;
            }
        });

        store
    }

    /// Create an LLM client based on available configuration
    fn create_llm_client(&self) -> Option<Box<dyn ModelClient>> {
        let config = self.providers_config.as_ref()?;

        // Priority: Anthropic > OpenAI > OpenCode
        if let Some(ref api_key) = config.anthropic_api_key {
            let key = api_key.expose_secret();
            // Detect OAuth token vs API key
            if key.starts_with("sk-ant-oat") {
                return Some(Box::new(AnthropicClient::with_oauth(key)));
            } else {
                return Some(Box::new(AnthropicClient::with_api_key(key)));
            }
        }

        if let Some(ref api_key) = config.openai_api_key {
            return Some(Box::new(OpenAIClient::with_api_key(api_key.expose_secret())));
        }

        if let Some(ref api_key) = config.opencode_api_key {
            let base_url = config.opencode_base_url.clone();
            return Some(Box::new(OpenCodeClient::new(base_url, Some(api_key.expose_secret().to_string()))));
        }

        None
    }

    async fn cleanup_expired(reviews: &Arc<RwLock<HashMap<String, ReviewState>>>, ttl_secs: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut reviews = reviews.write().await;
        reviews.retain(|_, state| now - state.created_at < ttl_secs);
    }

    pub async fn create_review(&self, repo_url: String) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let state = ReviewState::new(id.clone(), repo_url.clone());

        state.emit(ReviewEvent::ReviewStarted {
            review_id: id.clone(),
            repo_url,
        });

        let mut reviews = self.reviews.write().await;
        reviews.insert(id.clone(), state);

        id
    }

    pub async fn get_review(&self, id: &str) -> Option<ReviewState> {
        let reviews = self.reviews.read().await;
        reviews.get(id).map(|state| ReviewState {
            id: state.id.clone(),
            status: state.status,
            repo_url: state.repo_url.clone(),
            results: state.results.clone(),
            suggestions: state.suggestions.clone(),
            created_at: state.created_at,
            event_sender: state.event_sender.clone(),
        })
    }

    pub async fn subscribe(&self, id: &str) -> Option<broadcast::Receiver<ReviewEvent>> {
        let reviews = self.reviews.read().await;
        reviews.get(id).map(|state| state.subscribe())
    }

    pub async fn run_review(&self, id: &str) -> Result<(), ApiError> {
        let (repo_url, event_sender) = {
            let mut reviews = self.reviews.write().await;
            if let Some(state) = reviews.get_mut(id) {
                state.status = ReviewStatus::Cloning;
                (state.repo_url.clone(), state.event_sender.clone())
            } else {
                return Err(ApiError::NotFound(format!("Review {} not found", id)));
            }
        };

        let start = std::time::Instant::now();

        let cloned_repo = ClonedRepo::from_url(&repo_url).await?;
        let repo_path = cloned_repo.path.clone();

        {
            let mut reviews = self.reviews.write().await;
            if let Some(state) = reviews.get_mut(id) {
                state.status = ReviewStatus::Running;
            }
        }

        let mut all_diagnostics: Vec<Diagnostic> = Vec::new();

        let checker_results = tokio::task::spawn_blocking({
            let path = repo_path.clone();
            move || run_all_checkers(&path)
        })
        .await
        .map_err(|e| ApiError::InternalError(format!("Checker task failed: {}", e)))?;

        for (check_type, diagnostics) in checker_results {
            let check_start = std::time::Instant::now();
            let _ = event_sender.send(ReviewEvent::CheckStarted { check_type });

            let _ = event_sender.send(ReviewEvent::CheckCompleted {
                check_type,
                diagnostics: diagnostics.clone(),
                duration_ms: check_start.elapsed().as_millis() as u64,
            });

            all_diagnostics.extend(diagnostics);
        }

        let mut all_suggestions: Vec<Suggestion> = Vec::new();

        if let Some(llm_client) = self.create_llm_client() {
            let validated_diagnostics = self.run_ai_validators(
                llm_client.as_ref(),
                all_diagnostics.clone(),
                &event_sender,
            ).await;
            all_diagnostics = validated_diagnostics;

            let code_context = self.build_code_context(&repo_url, &repo_path, &all_diagnostics);
            let suggestions = self.run_ai_reviewers(
                llm_client.as_ref(),
                &code_context,
                &event_sender,
            ).await;
            all_suggestions = suggestions;
        }

        {
            let mut reviews = self.reviews.write().await;
            if let Some(state) = reviews.get_mut(id) {
                state.results = all_diagnostics.clone();
                state.suggestions = all_suggestions.clone();
                state.status = ReviewStatus::Completed;
                state.emit(ReviewEvent::ReviewCompleted {
                    summary: ReviewSummary {
                        total_diagnostics: all_diagnostics.len(),
                        by_severity: SeverityCounts {
                            error: all_diagnostics
                                .iter()
                                .filter(|d| d.severity == crate::types::Severity::Error)
                                .count(),
                            warning: all_diagnostics
                                .iter()
                                .filter(|d| d.severity == crate::types::Severity::Warning)
                                .count(),
                            info: all_diagnostics
                                .iter()
                                .filter(|d| d.severity == crate::types::Severity::Info)
                                .count(),
                        },
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                });
            }
        }

        Ok(())
    }

    async fn run_ai_validators(
        &self,
        client: &dyn ModelClient,
        mut diagnostics: Vec<Diagnostic>,
        event_sender: &broadcast::Sender<ReviewEvent>,
    ) -> Vec<Diagnostic> {
        let validators: Vec<Box<dyn Validator>> = vec![
            Box::new(TypoValidator::new()),
            Box::new(CommentValidator::new()),
            Box::new(Prioritizer::new()),
        ];

        for validator in validators {
            let _ = event_sender.send(ReviewEvent::ValidationStarted {
                validator: validator.name().to_string(),
            });

            match validator.validate(client, diagnostics.clone()).await {
                Ok(validated) => {
                    let _ = event_sender.send(ReviewEvent::ValidationCompleted {
                        validator: validator.name().to_string(),
                        results: validated.clone(),
                    });
                    diagnostics = validated;
                }
                Err(e) => {
                    tracing::warn!("Validator {} failed: {}", validator.name(), e);
                }
            }
        }

        diagnostics
    }

    async fn run_ai_reviewers(
        &self,
        client: &dyn ModelClient,
        context: &CodeContext,
        event_sender: &broadcast::Sender<ReviewEvent>,
    ) -> Vec<Suggestion> {
        let reviewers: Vec<Box<dyn Reviewer>> = vec![
            Box::new(CodeOracle::new()),
            Box::new(ProductIdeasReviewer::new()),
        ];

        let mut all_suggestions = Vec::new();

        for reviewer in reviewers {
            let _ = event_sender.send(ReviewEvent::ReviewerStarted {
                reviewer: reviewer.name().to_string(),
            });

            match reviewer.review(client, context).await {
                Ok(suggestions) => {
                    let _ = event_sender.send(ReviewEvent::ReviewerCompleted {
                        reviewer: reviewer.name().to_string(),
                        suggestions: suggestions.clone(),
                    });
                    all_suggestions.extend(suggestions);
                }
                Err(e) => {
                    tracing::warn!("Reviewer {} failed: {}", reviewer.name(), e);
                }
            }
        }

        all_suggestions
    }

    fn build_code_context(&self, repo_url: &str, repo_path: &Path, diagnostics: &[Diagnostic]) -> CodeContext {
        let files = Self::read_source_files(repo_path);
        CodeContext::new(repo_url.to_string())
            .with_files(files)
            .with_diagnostics(diagnostics.to_vec())
    }

    fn read_source_files(repo_path: &Path) -> Vec<(String, String)> {
        let mut files = Vec::new();
        let extensions = ["rs", "ts", "tsx", "js", "jsx", "py", "go", "java"];

        for entry in walkdir::WalkDir::new(repo_path)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| extensions.contains(&ext))
                    .unwrap_or(false)
            })
            .take(20)
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let relative_path = entry
                    .path()
                    .strip_prefix(repo_path)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .to_string();
                files.push((relative_path, content));
            }
        }

        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_review() {
        let store = ReviewStore::new(3600, None);
        let id = store
            .create_review("https://github.com/test/repo".to_string())
            .await;

        let state = store.get_review(&id).await;
        assert!(state.is_some());
        assert_eq!(state.unwrap().repo_url, "https://github.com/test/repo");
    }

    #[tokio::test]
    async fn test_subscribe() {
        let store = ReviewStore::new(3600, None);
        let id = store
            .create_review("https://github.com/test/repo".to_string())
            .await;

        let receiver = store.subscribe(&id).await;
        assert!(receiver.is_some());
    }
}
