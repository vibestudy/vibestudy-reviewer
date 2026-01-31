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

// ============================================================================
// GRADING TYPES
// ============================================================================

fn default_weight() -> f32 {
    1.0
}

/// Single acceptance criterion from planner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    /// Unique identifier (optional, for tracking)
    #[serde(default)]
    pub id: Option<String>,
    /// Human-readable criterion description
    /// Example: "코드가 에러 없이 컴파일됨"
    pub description: String,
    /// Optional weight for weighted scoring (default: 1.0)
    #[serde(default = "default_weight")]
    pub weight: f32,
}

/// Task from planner containing acceptance criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradeTask {
    /// Task title
    /// Example: "환경 설정 및 준비"
    pub title: String,
    /// Optional task description
    #[serde(default)]
    pub description: Option<String>,
    /// List of acceptance criteria to check
    pub acceptance_criteria: Vec<Criterion>,
    /// Expected time in minutes (for reference only)
    #[serde(default)]
    pub estimated_minutes: Option<u32>,
}

/// Grading configuration (can be passed in request or use defaults)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradeConfig {
    /// Max concurrent tasks being graded
    #[serde(default = "default_max_parallel_tasks")]
    pub max_parallel_tasks: usize,
    /// Max concurrent criteria checks per task
    #[serde(default = "default_max_parallel_criteria")]
    pub max_parallel_criteria: usize,
    /// Timeout for single criterion check (seconds)
    #[serde(default = "default_criterion_timeout")]
    pub criterion_timeout_secs: u64,
    /// Max files to include in context
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    /// Max chars per file
    #[serde(default = "default_max_chars_per_file")]
    pub max_chars_per_file: usize,
}

fn default_max_parallel_tasks() -> usize {
    5
}
fn default_max_parallel_criteria() -> usize {
    10
}
fn default_criterion_timeout() -> u64 {
    60
}
fn default_max_files() -> usize {
    30
}
fn default_max_chars_per_file() -> usize {
    5000
}

impl Default for GradeConfig {
    fn default() -> Self {
        Self {
            max_parallel_tasks: 5,
            max_parallel_criteria: 10,
            criterion_timeout_secs: 60,
            max_files: 30,
            max_chars_per_file: 5000,
        }
    }
}

/// Optional metadata for grading request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GradeMetadata {
    /// Session ID from planner
    #[serde(default)]
    pub session_id: Option<String>,
    /// Course title
    #[serde(default)]
    pub course_title: Option<String>,
    /// Student identifier
    #[serde(default)]
    pub student_id: Option<String>,
}

/// Grading request from external system (planner)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradeRequest {
    /// Student's submission repository URL
    pub repo_url: String,
    /// Optional branch to grade (default: main/master)
    #[serde(default)]
    pub branch: Option<String>,
    /// Tasks with acceptance criteria
    pub tasks: Vec<GradeTask>,
    /// Optional grading configuration
    #[serde(default)]
    pub config: Option<GradeConfig>,
    /// Optional metadata
    #[serde(default)]
    pub metadata: Option<GradeMetadata>,
    /// Optional curriculum ID for linked grading
    #[serde(default)]
    pub curriculum_id: Option<String>,
    /// Optional task ID for linked grading
    #[serde(default)]
    pub task_id: Option<String>,
}

// ----------------------------------------------------------------------------
// Results
// ----------------------------------------------------------------------------

/// Code reference pointing to specific lines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRef {
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Result of checking a single criterion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionResult {
    /// Original criterion description
    pub criterion: String,
    /// Whether the criterion is satisfied
    pub passed: bool,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// LLM's reasoning/evidence
    pub evidence: String,
    /// Code locations that support the decision
    #[serde(default)]
    pub code_references: Vec<CodeRef>,
    /// Weight used for scoring
    pub weight: f32,
}

/// Task grading status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// All criteria passed
    Passed,
    /// Some criteria passed (score > 0, < 1)
    Partial,
    /// No criteria passed
    Failed,
}

/// Result of grading a single task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGradeResult {
    pub task_title: String,
    /// Weighted score (0.0 - 1.0)
    pub score: f32,
    pub status: TaskStatus,
    pub criteria_results: Vec<CriterionResult>,
    /// Number of criteria passed
    pub passed_count: usize,
    /// Total number of criteria
    pub total_count: usize,
}

/// Overall grading status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradeStatus {
    Pending,
    Cloning,
    Analyzing,
    Grading,
    Completed,
    Failed,
}

/// Complete grading report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradeReport {
    pub id: String,
    pub repo_url: String,
    pub status: GradeStatus,
    /// Overall weighted score (0.0 - 1.0)
    pub overall_score: f32,
    /// Overall percentage (0 - 100)
    pub percentage: u32,
    /// Human-readable grade (우수/양호/보통/미흡/불합격)
    pub grade: String,
    pub tasks: Vec<TaskGradeResult>,
    pub summary: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<GradeMetadata>,
}

// ----------------------------------------------------------------------------
// SSE Events
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GradeEvent {
    /// Grading job started
    GradeStarted {
        grade_id: String,
        repo_url: String,
        task_count: usize,
        total_criteria: usize,
    },
    /// Repository cloning started
    CloningStarted,
    /// Repository cloning completed
    CloningCompleted { duration_ms: u64 },
    /// Code analysis started
    AnalysisStarted,
    /// Code analysis completed
    AnalysisCompleted {
        file_count: usize,
        total_lines: usize,
    },
    /// Task grading started
    TaskStarted {
        task_index: usize,
        task_title: String,
        criteria_count: usize,
    },
    /// Single criterion check completed
    CriterionChecked {
        task_index: usize,
        criterion_index: usize,
        criterion: String,
        passed: bool,
        confidence: f32,
    },
    /// Task grading completed
    TaskCompleted {
        task_index: usize,
        task_title: String,
        score: f32,
        status: TaskStatus,
        passed_count: usize,
        total_count: usize,
    },
    /// All grading completed
    GradeCompleted {
        overall_score: f32,
        percentage: u32,
        grade: String,
        summary: String,
        duration_ms: u64,
    },
    /// Grading failed
    GradeFailed { error: String, recoverable: bool },
    /// Keep-alive ping
    Ping,
}

// ----------------------------------------------------------------------------
// API Responses
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGradeResponse {
    pub grade_id: String,
    pub status: GradeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradeResponse {
    pub id: String,
    pub status: GradeStatus,
    pub repo_url: String,
    pub overall_score: f32,
    pub percentage: u32,
    pub grade: String,
    pub tasks: Vec<TaskGradeResult>,
    pub summary: String,
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

    #[test]
    fn test_grade_event_serialization() {
        let event = GradeEvent::GradeStarted {
            grade_id: "grade-123".to_string(),
            repo_url: "https://github.com/test/repo".to_string(),
            task_count: 3,
            total_criteria: 10,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("grade_started"));
        assert!(json.contains("grade-123"));
    }

    #[test]
    fn test_criterion_result_serialization() {
        let result = CriterionResult {
            criterion: "코드가 실행됨".to_string(),
            passed: true,
            confidence: 0.95,
            evidence: "package.json exists".to_string(),
            code_references: vec![CodeRef {
                file: "package.json".to_string(),
                line_start: 1,
                line_end: 10,
                snippet: None,
            }],
            weight: 1.0,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("코드가 실행됨"));
        assert!(json.contains("package.json"));
    }

    #[test]
    fn test_grade_config_default() {
        let config = GradeConfig::default();
        assert_eq!(config.max_parallel_tasks, 5);
        assert_eq!(config.max_parallel_criteria, 10);
        assert_eq!(config.criterion_timeout_secs, 60);
    }
}
