use crate::ai::{CodeContext, Reviewer};
use crate::error::LlmError;
use crate::llm::{Message, ModelClient};
use crate::types::{Priority, Suggestion, SuggestionCategory};
use async_trait::async_trait;
use serde::Deserialize;

pub struct CodeOracle;

impl CodeOracle {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CodeOracle {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Reviewer for CodeOracle {
    async fn review(
        &self,
        client: &dyn ModelClient,
        context: &CodeContext,
    ) -> Result<Vec<Suggestion>, LlmError> {
        if context.files.is_empty() {
            return Ok(Vec::new());
        }

        let files_content = context
            .files
            .iter()
            .take(10)
            .map(|(path, content)| {
                let preview = if content.len() > 2000 {
                    format!("{}...(truncated)", &content[..2000])
                } else {
                    content.clone()
                };
                format!("=== {} ===\n{}", path, preview)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Analyze this codebase and provide architectural and code quality suggestions.\n\n\
             {}\n\n\
             Provide suggestions in this JSON format:\n\
             [{{\n\
               \"category\": \"architecture\"|\"performance\"|\"security\"|\"code_quality\",\n\
               \"title\": \"Brief title\",\n\
               \"description\": \"Detailed description\",\n\
               \"file\": \"path/to/file.rs\" (optional),\n\
               \"line\": 42 (optional),\n\
               \"priority\": \"high\"|\"medium\"|\"low\",\n\
               \"rationale\": \"Why this matters\"\n\
             }}]\n\n\
             Focus on:\n\
             - Architectural patterns and anti-patterns\n\
             - Error handling improvements\n\
             - Performance optimizations\n\
             - Security concerns\n\
             - Code organization\n\n\
             Return ONLY the JSON array.",
            files_content
        );

        let messages = vec![Message::user(prompt)];
        let response = client.chat(&messages, Some(CODE_ORACLE_SYSTEM)).await?;

        parse_suggestions(&response)
    }

    fn name(&self) -> &'static str {
        "code_oracle"
    }
}

pub struct ProductIdeasReviewer;

impl ProductIdeasReviewer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProductIdeasReviewer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Reviewer for ProductIdeasReviewer {
    async fn review(
        &self,
        client: &dyn ModelClient,
        context: &CodeContext,
    ) -> Result<Vec<Suggestion>, LlmError> {
        if context.files.is_empty() {
            return Ok(Vec::new());
        }

        let summary = context.summary();
        let diagnostics_summary = if context.diagnostics.is_empty() {
            "No issues detected.".to_string()
        } else {
            format!("{} issues found.", context.diagnostics.len())
        };

        let prompt = format!(
            "Analyze this codebase from a PRODUCT perspective.\n\n\
             {}\n\n\
             Current issues: {}\n\n\
             Provide suggestions in this JSON format:\n\
             [{{\n\
               \"category\": \"product_idea\"|\"hardening\",\n\
               \"title\": \"Brief title\",\n\
               \"description\": \"Detailed description\",\n\
               \"priority\": \"high\"|\"medium\"|\"low\",\n\
               \"rationale\": \"Why this matters for the product\"\n\
             }}]\n\n\
             Focus on:\n\
             - Feature suggestions based on code structure\n\
             - Production hardening (logging, monitoring, error recovery)\n\
             - Deployment considerations\n\
             - User experience improvements\n\
             - Reliability and resilience\n\n\
             Return ONLY the JSON array.",
            summary, diagnostics_summary
        );

        let messages = vec![Message::user(prompt)];
        let response = client.chat(&messages, Some(PRODUCT_REVIEWER_SYSTEM)).await?;

        parse_suggestions(&response)
    }

    fn name(&self) -> &'static str {
        "product_ideas_reviewer"
    }
}

const CODE_ORACLE_SYSTEM: &str = "You are a senior software architect reviewing code. \
    Focus on actionable improvements. Respond ONLY with JSON. \
    All text content (title, description, rationale) MUST be written in Korean.";

const PRODUCT_REVIEWER_SYSTEM: &str = "You are a product engineer reviewing code for production readiness. \
    Focus on reliability, user experience, and operational excellence. Respond ONLY with JSON. \
    All text content (title, description, rationale) MUST be written in Korean.";

#[derive(Debug, Deserialize)]
struct RawSuggestion {
    category: String,
    title: String,
    description: String,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    line: Option<u32>,
    priority: String,
    rationale: String,
}

fn parse_suggestions(response: &str) -> Result<Vec<Suggestion>, LlmError> {
    let trimmed = response.trim();
    let json_str = if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    let raw: Vec<RawSuggestion> = serde_json::from_str(json_str).map_err(|e| {
        LlmError::InvalidResponse(format!("Failed to parse suggestions: {} - Response: {}", e, json_str))
    })?;

    Ok(raw
        .into_iter()
        .map(|r| Suggestion {
            category: parse_category(&r.category),
            title: r.title,
            description: r.description,
            file: r.file,
            line: r.line,
            priority: parse_priority(&r.priority),
            rationale: r.rationale,
        })
        .collect())
}

fn parse_category(s: &str) -> SuggestionCategory {
    match s.to_lowercase().as_str() {
        "architecture" => SuggestionCategory::Architecture,
        "performance" => SuggestionCategory::Performance,
        "security" => SuggestionCategory::Security,
        "code_quality" => SuggestionCategory::CodeQuality,
        "product_idea" => SuggestionCategory::ProductIdea,
        "hardening" => SuggestionCategory::Hardening,
        _ => SuggestionCategory::CodeQuality,
    }
}

fn parse_priority(s: &str) -> Priority {
    match s.to_lowercase().as_str() {
        "high" => Priority::High,
        "medium" => Priority::Medium,
        "low" => Priority::Low,
        _ => Priority::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_suggestions() {
        let response = r#"[
            {
                "category": "architecture",
                "title": "Add caching layer",
                "description": "Consider adding Redis for caching",
                "file": "src/api.rs",
                "line": 42,
                "priority": "high",
                "rationale": "Reduces database load"
            }
        ]"#;

        let suggestions = parse_suggestions(response).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert!(matches!(
            suggestions[0].category,
            SuggestionCategory::Architecture
        ));
        assert_eq!(suggestions[0].title, "Add caching layer");
        assert!(matches!(suggestions[0].priority, Priority::High));
    }

    #[test]
    fn test_parse_category() {
        assert!(matches!(
            parse_category("architecture"),
            SuggestionCategory::Architecture
        ));
        assert!(matches!(
            parse_category("PERFORMANCE"),
            SuggestionCategory::Performance
        ));
        assert!(matches!(
            parse_category("unknown"),
            SuggestionCategory::CodeQuality
        ));
    }

    #[test]
    fn test_parse_priority() {
        assert!(matches!(parse_priority("high"), Priority::High));
        assert!(matches!(parse_priority("LOW"), Priority::Low));
        assert!(matches!(parse_priority("unknown"), Priority::Medium));
    }
}
