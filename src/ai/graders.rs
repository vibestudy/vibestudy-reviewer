use async_trait::async_trait;
use serde::Deserialize;

use crate::error::LlmError;
use crate::llm::{Message, ModelClient};
use crate::types::{CodeRef, Criterion, CriterionResult, GradeTask};

#[derive(Debug, Clone)]
pub struct GradeContext {
    pub repo_url: String,
    pub task: GradeTask,
    pub files: Vec<(String, String)>,
}

impl GradeContext {
    pub fn new(repo_url: String, task: GradeTask) -> Self {
        Self {
            repo_url,
            task,
            files: Vec::new(),
        }
    }

    pub fn with_files(mut self, files: Vec<(String, String)>) -> Self {
        self.files = files;
        self
    }

    pub fn code_summary(&self, max_files: usize, max_chars_per_file: usize) -> String {
        self.files
            .iter()
            .take(max_files)
            .map(|(path, content)| {
                let truncated = if content.len() > max_chars_per_file {
                    format!(
                        "{}...\n[truncated, {} more chars]",
                        &content[..max_chars_per_file],
                        content.len() - max_chars_per_file
                    )
                } else {
                    content.clone()
                };
                format!("=== {} ===\n{}", path, truncated)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[async_trait]
pub trait Grader: Send + Sync {
    async fn check_criterion(
        &self,
        client: &dyn ModelClient,
        context: &GradeContext,
        criterion: &Criterion,
    ) -> Result<CriterionResult, LlmError>;

    fn name(&self) -> &'static str;
}

pub struct CriteriaChecker {
    max_files: usize,
    max_chars_per_file: usize,
}

impl CriteriaChecker {
    pub fn new() -> Self {
        Self {
            max_files: 20,
            max_chars_per_file: 4000,
        }
    }

    pub fn with_limits(max_files: usize, max_chars_per_file: usize) -> Self {
        Self {
            max_files,
            max_chars_per_file,
        }
    }
}

impl Default for CriteriaChecker {
    fn default() -> Self {
        Self::new()
    }
}

const GRADER_SYSTEM_PROMPT: &str = r#"You are a code grader evaluating student submissions against acceptance criteria.

## Your Role
Determine if the submitted code satisfies a specific acceptance criterion.

## Evaluation Guidelines
1. Be Fair: Give credit for working implementations, even if imperfect
2. Be Thorough: Check for actual implementation, not just presence of code
3. Be Specific: Cite exact file and line numbers as evidence
4. Consider Intent: Partial implementations may still satisfy criteria

## Scoring Rules
- passed: true - Criterion is clearly satisfied
- passed: false - Criterion is NOT satisfied or insufficient evidence
- confidence: Your certainty (0.0 = guess, 1.0 = certain)

## Response Format
Respond ONLY with valid JSON (no markdown, no explanation):
{
    "passed": true|false,
    "confidence": 0.0-1.0,
    "evidence": "Detailed explanation with code references",
    "code_references": [
        {"file": "path/to/file", "line_start": 10, "line_end": 20, "snippet": "optional"}
    ]
}"#;

#[derive(Debug, Deserialize)]
struct GraderResponse {
    passed: bool,
    confidence: f32,
    evidence: String,
    #[serde(default)]
    code_references: Vec<RawCodeRef>,
}

#[derive(Debug, Deserialize)]
struct RawCodeRef {
    file: String,
    line_start: u32,
    line_end: u32,
    #[serde(default)]
    snippet: Option<String>,
}

#[async_trait]
impl Grader for CriteriaChecker {
    async fn check_criterion(
        &self,
        client: &dyn ModelClient,
        context: &GradeContext,
        criterion: &Criterion,
    ) -> Result<CriterionResult, LlmError> {
        let code_summary = context.code_summary(self.max_files, self.max_chars_per_file);

        let prompt = format!(
            r#"## Task
{task_title}
{task_desc}

## Acceptance Criterion to Check
{criterion}

## Submitted Code
{code}

Evaluate if this criterion is satisfied. Return JSON only."#,
            task_title = context.task.title,
            task_desc = context.task.description.as_deref().unwrap_or(""),
            criterion = criterion.description,
            code = code_summary
        );

        let messages = vec![Message::user(prompt)];
        let response = client.chat(&messages, Some(GRADER_SYSTEM_PROMPT)).await?;

        self.parse_response(&response, criterion)
    }

    fn name(&self) -> &'static str {
        "criteria_checker"
    }
}

impl CriteriaChecker {
    fn parse_response(
        &self,
        response: &str,
        criterion: &Criterion,
    ) -> Result<CriterionResult, LlmError> {
        let json_str = self.extract_json(response);

        let raw: GraderResponse = serde_json::from_str(&json_str).map_err(|e| {
            tracing::warn!("Failed to parse grader response: {} - Raw: {}", e, response);
            LlmError::InvalidResponse(format!("JSON parse error: {}", e))
        })?;

        Ok(CriterionResult {
            criterion: criterion.description.clone(),
            passed: raw.passed,
            confidence: raw.confidence.clamp(0.0, 1.0),
            evidence: raw.evidence,
            code_references: raw
                .code_references
                .into_iter()
                .map(|r| CodeRef {
                    file: r.file,
                    line_start: r.line_start,
                    line_end: r.line_end,
                    snippet: r.snippet,
                })
                .collect(),
            weight: criterion.weight,
        })
    }

    fn extract_json(&self, response: &str) -> String {
        let trimmed = response.trim();

        // Try to find JSON in markdown code block
        if let Some(start) = trimmed.find("```json") {
            if let Some(end) = trimmed[start + 7..].find("```") {
                return trimmed[start + 7..start + 7 + end].trim().to_string();
            }
        }

        // Try to find raw JSON object
        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                return trimmed[start..=end].to_string();
            }
        }

        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_raw() {
        let checker = CriteriaChecker::new();
        let input = r#"{"passed": true, "confidence": 0.9, "evidence": "test", "code_references": []}"#;
        assert_eq!(checker.extract_json(input), input);
    }

    #[test]
    fn test_extract_json_markdown() {
        let checker = CriteriaChecker::new();
        let input = r#"Here is my analysis:
```json
{"passed": true, "confidence": 0.9, "evidence": "test", "code_references": []}
```
"#;
        let result = checker.extract_json(input);
        assert!(result.contains("\"passed\": true"));
    }

    #[test]
    fn test_code_summary_truncation() {
        let task = GradeTask {
            title: "Test".to_string(),
            description: None,
            acceptance_criteria: vec![],
            estimated_minutes: None,
        };
        let ctx = GradeContext::new("https://example.com".to_string(), task)
            .with_files(vec![
                ("file1.rs".to_string(), "a".repeat(10000)),
                ("file2.rs".to_string(), "short".to_string()),
            ]);

        let summary = ctx.code_summary(2, 100);
        assert!(summary.contains("[truncated"));
        assert!(summary.contains("file2.rs"));
    }
}
