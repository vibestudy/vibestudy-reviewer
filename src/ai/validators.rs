use crate::ai::Validator;
use crate::error::LlmError;
use crate::llm::{Message, ModelClient};
use crate::types::{Diagnostic, Severity};
use async_trait::async_trait;
use serde::Deserialize;

pub struct TypoValidator;

impl TypoValidator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TypoValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Validator for TypoValidator {
    async fn validate(
        &self,
        client: &dyn ModelClient,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Vec<Diagnostic>, LlmError> {
        if diagnostics.is_empty() {
            return Ok(Vec::new());
        }

        let typos_list: Vec<_> = diagnostics
            .iter()
            .enumerate()
            .map(|(i, d)| format!("{}. \"{}\" in {} (line {})", i + 1, d.message, d.file, d.line))
            .collect();

        let prompt = format!(
            "Review these potential typos and identify FALSE POSITIVES (valid technical terms, \
             abbreviations, or intentional spellings).\n\n\
             Typos:\n{}\n\n\
             Return ONLY a JSON array of indices (1-based) that are FALSE POSITIVES. \
             Example: [1, 3, 5]\n\
             If all are real typos, return: []",
            typos_list.join("\n")
        );

        let messages = vec![Message::user(prompt)];
        let response = client.chat(&messages, Some(VALIDATOR_SYSTEM_PROMPT)).await?;

        let false_positives = parse_index_array(&response);

        let validated: Vec<Diagnostic> = diagnostics
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !false_positives.contains(&(i + 1)))
            .map(|(_, d)| d)
            .collect();

        Ok(validated)
    }

    fn name(&self) -> &'static str {
        "typo_validator"
    }
}

pub struct CommentValidator;

impl CommentValidator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CommentValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Validator for CommentValidator {
    async fn validate(
        &self,
        client: &dyn ModelClient,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Vec<Diagnostic>, LlmError> {
        if diagnostics.is_empty() {
            return Ok(Vec::new());
        }

        let comments_list: Vec<_> = diagnostics
            .iter()
            .enumerate()
            .map(|(i, d)| format!("{}. {} in {} (line {})", i + 1, d.message, d.file, d.line))
            .collect();

        let prompt = format!(
            "Review these TODO/FIXME/HACK comments and identify which ones are:\n\
             - LOW PRIORITY (minor improvements, nice-to-have)\n\
             - Already completed but not removed\n\
             - Not actionable\n\n\
             Comments:\n{}\n\n\
             Return ONLY a JSON array of indices (1-based) to REMOVE. \
             Example: [2, 4]\n\
             If all are important, return: []",
            comments_list.join("\n")
        );

        let messages = vec![Message::user(prompt)];
        let response = client.chat(&messages, Some(VALIDATOR_SYSTEM_PROMPT)).await?;

        let to_remove = parse_index_array(&response);

        let validated: Vec<Diagnostic> = diagnostics
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !to_remove.contains(&(i + 1)))
            .map(|(_, d)| d)
            .collect();

        Ok(validated)
    }

    fn name(&self) -> &'static str {
        "comment_validator"
    }
}

pub struct Prioritizer;

impl Prioritizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Prioritizer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct PriorityItem {
    index: usize,
    priority: String,
}

#[async_trait]
impl Validator for Prioritizer {
    async fn validate(
        &self,
        client: &dyn ModelClient,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Vec<Diagnostic>, LlmError> {
        if diagnostics.is_empty() {
            return Ok(Vec::new());
        }

        let issues_list: Vec<_> = diagnostics
            .iter()
            .enumerate()
            .map(|(i, d)| {
                format!(
                    "{}. [{}] {} - {} ({}:{})",
                    i + 1,
                    severity_str(d.severity),
                    d.rule,
                    d.message,
                    d.file,
                    d.line
                )
            })
            .collect();

        let prompt = format!(
            "Prioritize these code issues by actual impact:\n\n\
             Issues:\n{}\n\n\
             Return a JSON array with priority adjustments:\n\
             [{{\n  \"index\": 1,\n  \"priority\": \"high\"|\"medium\"|\"low\"\n}}]\n\n\
             Consider:\n\
             - Security issues = high\n\
             - Bugs/crashes = high\n\
             - Performance = medium\n\
             - Style/formatting = low\n\n\
             Return ONLY the JSON array.",
            issues_list.join("\n")
        );

        let messages = vec![Message::user(prompt)];
        let response = client.chat(&messages, Some(VALIDATOR_SYSTEM_PROMPT)).await?;

        let priorities = parse_priorities(&response);

        let prioritized: Vec<Diagnostic> = diagnostics
            .into_iter()
            .enumerate()
            .map(|(i, mut d)| {
                if let Some(p) = priorities.get(&(i + 1)) {
                    d.severity = match p.as_str() {
                        "high" => Severity::Error,
                        "medium" => Severity::Warning,
                        "low" => Severity::Info,
                        _ => d.severity,
                    };
                }
                d
            })
            .collect();

        Ok(prioritized)
    }

    fn name(&self) -> &'static str {
        "prioritizer"
    }
}

const VALIDATOR_SYSTEM_PROMPT: &str = "You are a code review assistant. \
    Respond ONLY with the requested JSON format. No explanations. \
    All text content in the JSON (messages, descriptions, suggestions) MUST be written in Korean.";

fn severity_str(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "ERROR",
        Severity::Warning => "WARN",
        Severity::Info => "INFO",
    }
}

fn parse_index_array(response: &str) -> Vec<usize> {
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

    serde_json::from_str::<Vec<usize>>(json_str).unwrap_or_default()
}

fn parse_priorities(response: &str) -> std::collections::HashMap<usize, String> {
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

    let items: Vec<PriorityItem> = serde_json::from_str(json_str).unwrap_or_default();
    items.into_iter().map(|p| (p.index, p.priority)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_index_array() {
        assert_eq!(parse_index_array("[1, 3, 5]"), vec![1, 3, 5]);
        assert_eq!(parse_index_array("[]"), Vec::<usize>::new());
        assert_eq!(
            parse_index_array("Here is the result: [2, 4]"),
            vec![2, 4]
        );
        assert_eq!(parse_index_array("invalid"), Vec::<usize>::new());
    }

    #[test]
    fn test_parse_priorities() {
        let response = r#"[{"index": 1, "priority": "high"}, {"index": 2, "priority": "low"}]"#;
        let priorities = parse_priorities(response);
        assert_eq!(priorities.get(&1), Some(&"high".to_string()));
        assert_eq!(priorities.get(&2), Some(&"low".to_string()));
    }
}
