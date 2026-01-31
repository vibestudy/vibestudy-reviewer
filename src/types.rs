use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReviewEvent {
    ReviewStarted {
        review_id: String,
        repo_url: String,
    },
    CheckStarted {
        check_type: CheckType,
    },
    CheckCompleted {
        check_type: CheckType,
        diagnostics: Vec<Diagnostic>,
        duration_ms: u64,
    },
    CheckFailed {
        check_type: CheckType,
        error: String,
    },
    ValidationStarted {
        validator: String,
    },
    ValidationCompleted {
        validator: String,
        results: Vec<Diagnostic>,
    },
    ReviewerStarted {
        reviewer: String,
    },
    ReviewerCompleted {
        reviewer: String,
        suggestions: Vec<Suggestion>,
    },
    ReviewCompleted {
        summary: ReviewSummary,
    },
    ReviewFailed {
        error: String,
    },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSummary {
    pub total_diagnostics: usize,
    pub by_severity: SeverityCounts,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityCounts {
    pub error: usize,
    pub warning: usize,
    pub info: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub category: SuggestionCategory,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub priority: Priority,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionCategory {
    Architecture,
    Performance,
    Security,
    CodeQuality,
    ProductIdea,
    Hardening,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Cloning,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub rule: String,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    Lint,
    Comments,
    Typos,
    Format,
    AiCode,
    AiProduct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub repo_url: String,
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReviewResponse {
    pub review_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResponse {
    pub id: String,
    pub status: ReviewStatus,
    pub repo_url: String,
    pub results: Vec<Diagnostic>,
    pub suggestions: Vec<Suggestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_event_serialization() {
        let event = ReviewEvent::ReviewStarted {
            review_id: "test-123".to_string(),
            repo_url: "https://github.com/test/repo".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("review_started"));
        assert!(json.contains("test-123"));
    }

    #[test]
    fn test_diagnostic_serialization() {
        let diagnostic = Diagnostic {
            file: "src/main.rs".to_string(),
            line: 10,
            column: 5,
            message: "unused variable".to_string(),
            rule: "unused_variables".to_string(),
            severity: Severity::Warning,
            suggestion: Some("remove or use the variable".to_string()),
        };
        let json = serde_json::to_string(&diagnostic).unwrap();
        assert!(json.contains("warning"));
        assert!(json.contains("src/main.rs"));
    }

    #[test]
    fn test_check_type_serialization() {
        let check = CheckType::Lint;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"lint\"");
    }
}
