pub mod reviewers;
pub mod validators;

use crate::error::LlmError;
use crate::llm::ModelClient;
use crate::types::{Diagnostic, Suggestion};
use async_trait::async_trait;

/// Trait for AI validators that filter/validate rule-based checker results
#[async_trait]
pub trait Validator: Send + Sync {
    /// Validate diagnostics, returning only those that pass AI validation
    async fn validate(
        &self,
        client: &dyn ModelClient,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Vec<Diagnostic>, LlmError>;

    /// Name of this validator for logging/events
    fn name(&self) -> &'static str;
}

/// Trait for AI reviewers that generate suggestions from code analysis
#[async_trait]
pub trait Reviewer: Send + Sync {
    /// Review code and generate suggestions
    async fn review(
        &self,
        client: &dyn ModelClient,
        code_context: &CodeContext,
    ) -> Result<Vec<Suggestion>, LlmError>;

    /// Name of this reviewer for logging/events
    fn name(&self) -> &'static str;
}

/// Context provided to reviewers for code analysis
#[derive(Debug, Clone)]
pub struct CodeContext {
    /// Repository URL or path
    pub repo_url: String,
    /// List of files with their contents (path, content)
    pub files: Vec<(String, String)>,
    /// Diagnostics from rule-based checkers (for context)
    pub diagnostics: Vec<Diagnostic>,
}

impl CodeContext {
    pub fn new(repo_url: String) -> Self {
        Self {
            repo_url,
            files: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn with_files(mut self, files: Vec<(String, String)>) -> Self {
        self.files = files;
        self
    }

    pub fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    /// Get a summary of the codebase for prompts
    pub fn summary(&self) -> String {
        let file_list: Vec<_> = self.files.iter().map(|(path, _)| path.as_str()).collect();
        format!(
            "Repository: {}\nFiles ({}):\n- {}",
            self.repo_url,
            self.files.len(),
            file_list.join("\n- ")
        )
    }
}
