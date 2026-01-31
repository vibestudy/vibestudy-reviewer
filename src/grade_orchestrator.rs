use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock, Semaphore};
use tokio::time::{interval, Duration};

use crate::ai::graders::{CriteriaChecker, GradeContext, Grader};
use crate::config::ProvidersConfig;
use crate::error::ApiError;
use crate::git::ClonedRepo;
use crate::llm::anthropic::AnthropicClient;
use crate::llm::openai::OpenAIClient;
use crate::llm::opencode::OpenCodeClient;
use crate::llm::ModelClient;
use crate::types::{
    CriterionResult, GradeConfig, GradeEvent, GradeMetadata, GradeReport, GradeRequest,
    GradeStatus, GradeTask, TaskGradeResult, TaskStatus,
};
use secrecy::ExposeSecret;

pub struct GradeState {
    pub id: String,
    pub status: GradeStatus,
    pub repo_url: String,
    pub tasks: Vec<GradeTask>,
    pub task_results: Vec<TaskGradeResult>,
    pub overall_score: f32,
    pub percentage: u32,
    pub grade: String,
    pub summary: String,
    pub error: Option<String>,
    pub metadata: Option<GradeMetadata>,
    pub created_at: u64,
    pub duration_ms: u64,
    event_sender: broadcast::Sender<GradeEvent>,
}

impl GradeState {
    pub fn new(id: String, request: &GradeRequest) -> Self {
        let (event_sender, _) = broadcast::channel(100);
        Self {
            id,
            status: GradeStatus::Pending,
            repo_url: request.repo_url.clone(),
            tasks: request.tasks.clone(),
            task_results: Vec::new(),
            overall_score: 0.0,
            percentage: 0,
            grade: String::new(),
            summary: String::new(),
            error: None,
            metadata: request.metadata.clone(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            duration_ms: 0,
            event_sender,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GradeEvent> {
        self.event_sender.subscribe()
    }

    pub fn emit(&self, event: GradeEvent) {
        let _ = self.event_sender.send(event);
    }

    pub fn to_report(&self) -> GradeReport {
        GradeReport {
            id: self.id.clone(),
            repo_url: self.repo_url.clone(),
            status: self.status,
            overall_score: self.overall_score,
            percentage: self.percentage,
            grade: self.grade.clone(),
            tasks: self.task_results.clone(),
            summary: self.summary.clone(),
            duration_ms: self.duration_ms,
            error: self.error.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

#[derive(Clone)]
pub struct GradeStore {
    grades: Arc<RwLock<HashMap<String, GradeState>>>,
    ttl_secs: u64,
    providers_config: Option<ProvidersConfig>,
    default_config: GradeConfig,
}

impl GradeStore {
    pub fn new(
        ttl_secs: u64,
        providers_config: Option<ProvidersConfig>,
        default_config: GradeConfig,
    ) -> Self {
        let store = Self {
            grades: Arc::new(RwLock::new(HashMap::new())),
            ttl_secs,
            providers_config,
            default_config,
        };

        Self::spawn_cleanup_task(store.grades.clone(), ttl_secs);
        store
    }

    fn spawn_cleanup_task(grades: Arc<RwLock<HashMap<String, GradeState>>>, ttl_secs: u64) {
        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(60));
            loop {
                cleanup_interval.tick().await;
                Self::cleanup_expired(&grades, ttl_secs).await;
            }
        });
    }

    async fn cleanup_expired(grades: &Arc<RwLock<HashMap<String, GradeState>>>, ttl_secs: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut grades = grades.write().await;
        grades.retain(|_, state| now - state.created_at < ttl_secs);
    }

    fn create_llm_client(&self) -> Option<Box<dyn ModelClient>> {
        let config = self.providers_config.as_ref()?;

        // Priority: Anthropic > OpenAI > OpenCode
        if let Some(ref api_key) = config.anthropic_api_key {
            return Some(Box::new(AnthropicClient::with_api_key(
                api_key.expose_secret(),
            )));
        }

        if let Some(ref api_key) = config.openai_api_key {
            return Some(Box::new(OpenAIClient::with_api_key(
                api_key.expose_secret(),
            )));
        }

        if let Some(ref api_key) = config.opencode_api_key {
            let base_url = config.opencode_base_url.clone();
            return Some(Box::new(OpenCodeClient::new(
                base_url,
                Some(api_key.expose_secret().to_string()),
            )));
        }

        None
    }

    pub async fn create_grade(&self, request: GradeRequest) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let state = GradeState::new(id.clone(), &request);

        let total_criteria: usize = request
            .tasks
            .iter()
            .map(|t| t.acceptance_criteria.len())
            .sum();

        state.emit(GradeEvent::GradeStarted {
            grade_id: id.clone(),
            repo_url: request.repo_url.clone(),
            task_count: request.tasks.len(),
            total_criteria,
        });

        let mut grades = self.grades.write().await;
        grades.insert(id.clone(), state);

        id
    }

    pub async fn get_grade(&self, id: &str) -> Option<GradeReport> {
        let grades = self.grades.read().await;
        grades.get(id).map(|state| state.to_report())
    }

    pub async fn subscribe(&self, id: &str) -> Option<broadcast::Receiver<GradeEvent>> {
        let grades = self.grades.read().await;
        grades.get(id).map(|state| state.subscribe())
    }

    pub async fn run_grade(&self, id: &str, request: GradeRequest) -> Result<(), ApiError> {
        let start = Instant::now();
        let config = request.config.clone().unwrap_or(self.default_config.clone());

        {
            let mut grades = self.grades.write().await;
            if let Some(state) = grades.get_mut(id) {
                state.status = GradeStatus::Cloning;
                state.emit(GradeEvent::CloningStarted);
            } else {
                return Err(ApiError::NotFound(format!("Grade {} not found", id)));
            }
        }

        let clone_start = Instant::now();
        let cloned_repo = ClonedRepo::from_url(&request.repo_url).await?;
        let repo_path = cloned_repo.path.clone();

        {
            let mut grades = self.grades.write().await;
            if let Some(state) = grades.get_mut(id) {
                state.emit(GradeEvent::CloningCompleted {
                    duration_ms: clone_start.elapsed().as_millis() as u64,
                });
                state.status = GradeStatus::Analyzing;
                state.emit(GradeEvent::AnalysisStarted);
            }
        }

        let files = Self::read_source_files(&repo_path, config.max_files);
        let total_lines: usize = files.iter().map(|(_, c)| c.lines().count()).sum();

        {
            let mut grades = self.grades.write().await;
            if let Some(state) = grades.get_mut(id) {
                state.emit(GradeEvent::AnalysisCompleted {
                    file_count: files.len(),
                    total_lines,
                });
                state.status = GradeStatus::Grading;
            }
        }

        let llm_client = self.create_llm_client().ok_or_else(|| {
            ApiError::InternalError("No LLM provider configured".to_string())
        })?;

        let grader = CriteriaChecker::with_limits(config.max_files, config.max_chars_per_file);
        let task_results = self
            .process_tasks_parallel(
                id,
                &request.tasks,
                &files,
                &request.repo_url,
                llm_client.as_ref(),
                &grader,
                &config,
            )
            .await;

        let (overall_score, percentage, grade, summary) =
            Self::calculate_final_score(&task_results);

        {
            let mut grades = self.grades.write().await;
            if let Some(state) = grades.get_mut(id) {
                state.task_results = task_results;
                state.overall_score = overall_score;
                state.percentage = percentage;
                state.grade = grade.clone();
                state.summary = summary.clone();
                state.status = GradeStatus::Completed;
                state.duration_ms = start.elapsed().as_millis() as u64;

                state.emit(GradeEvent::GradeCompleted {
                    overall_score,
                    percentage,
                    grade,
                    summary,
                    duration_ms: state.duration_ms,
                });
            }
        }

        Ok(())
    }

    async fn process_tasks_parallel(
        &self,
        grade_id: &str,
        tasks: &[GradeTask],
        files: &[(String, String)],
        repo_url: &str,
        client: &dyn ModelClient,
        grader: &CriteriaChecker,
        config: &GradeConfig,
    ) -> Vec<TaskGradeResult> {
        let task_semaphore = Arc::new(Semaphore::new(config.max_parallel_tasks));
        let criteria_semaphore = Arc::new(Semaphore::new(config.max_parallel_criteria));

        let mut task_results = Vec::with_capacity(tasks.len());

        for (task_index, task) in tasks.iter().enumerate() {
            let _permit = task_semaphore.acquire().await.unwrap();

            {
                let grades = self.grades.read().await;
                if let Some(state) = grades.get(grade_id) {
                    state.emit(GradeEvent::TaskStarted {
                        task_index,
                        task_title: task.title.clone(),
                        criteria_count: task.acceptance_criteria.len(),
                    });
                }
            }

            let context = GradeContext::new(repo_url.to_string(), task.clone())
                .with_files(files.to_vec());
            let criteria_results = self
                .process_criteria_parallel(
                    grade_id,
                    task_index,
                    task,
                    &context,
                    client,
                    grader,
                    &criteria_semaphore,
                )
                .await;

            let (score, status, passed_count) = Self::calculate_task_score(&criteria_results);

            let task_result = TaskGradeResult {
                task_title: task.title.clone(),
                score,
                status,
                criteria_results,
                passed_count,
                total_count: task.acceptance_criteria.len(),
            };

            {
                let grades = self.grades.read().await;
                if let Some(state) = grades.get(grade_id) {
                    state.emit(GradeEvent::TaskCompleted {
                        task_index,
                        task_title: task.title.clone(),
                        score,
                        status,
                        passed_count,
                        total_count: task.acceptance_criteria.len(),
                    });
                }
            }

            task_results.push(task_result);
        }

        task_results
    }

    async fn process_criteria_parallel(
        &self,
        grade_id: &str,
        task_index: usize,
        task: &GradeTask,
        context: &GradeContext,
        client: &dyn ModelClient,
        grader: &CriteriaChecker,
        semaphore: &Arc<Semaphore>,
    ) -> Vec<CriterionResult> {
        let mut results = Vec::with_capacity(task.acceptance_criteria.len());

        for (criterion_index, criterion) in task.acceptance_criteria.iter().enumerate() {
            let _permit = semaphore.acquire().await.unwrap();

            let result = match grader.check_criterion(client, context, criterion).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::warn!(
                        "Failed to check criterion '{}': {}",
                        criterion.description,
                        e
                    );
                    CriterionResult {
                        criterion: criterion.description.clone(),
                        passed: false,
                        confidence: 0.0,
                        evidence: format!("Error checking criterion: {}", e),
                        code_references: vec![],
                        weight: criterion.weight,
                    }
                }
            };

            {
                let grades = self.grades.read().await;
                if let Some(state) = grades.get(grade_id) {
                    state.emit(GradeEvent::CriterionChecked {
                        task_index,
                        criterion_index,
                        criterion: criterion.description.clone(),
                        passed: result.passed,
                        confidence: result.confidence,
                    });
                }
            }

            results.push(result);
        }

        results
    }

    fn calculate_task_score(criteria_results: &[CriterionResult]) -> (f32, TaskStatus, usize) {
        if criteria_results.is_empty() {
            return (0.0, TaskStatus::Failed, 0);
        }

        let total_weight: f32 = criteria_results.iter().map(|r| r.weight).sum();
        let passed_weight: f32 = criteria_results
            .iter()
            .filter(|r| r.passed)
            .map(|r| r.weight)
            .sum();

        let score = if total_weight > 0.0 {
            passed_weight / total_weight
        } else {
            0.0
        };

        let passed_count = criteria_results.iter().filter(|r| r.passed).count();

        let status = if score >= 1.0 {
            TaskStatus::Passed
        } else if score > 0.0 {
            TaskStatus::Partial
        } else {
            TaskStatus::Failed
        };

        (score, status, passed_count)
    }

    fn calculate_final_score(task_results: &[TaskGradeResult]) -> (f32, u32, String, String) {
        if task_results.is_empty() {
            return (0.0, 0, "N/A".to_string(), "No tasks to grade".to_string());
        }

        let overall_score: f32 =
            task_results.iter().map(|t| t.score).sum::<f32>() / task_results.len() as f32;
        let percentage = (overall_score * 100.0).round() as u32;

        let grade = match percentage {
            90..=100 => "우수",
            75..=89 => "양호",
            60..=74 => "보통",
            40..=59 => "미흡",
            _ => "불합격",
        }
        .to_string();

        let passed_tasks = task_results
            .iter()
            .filter(|t| t.status == TaskStatus::Passed)
            .count();
        let total_tasks = task_results.len();
        let total_criteria: usize = task_results.iter().map(|t| t.total_count).sum();
        let passed_criteria: usize = task_results.iter().map(|t| t.passed_count).sum();

        let summary = format!(
            "전체 점수: {}점 ({}) - 과제 {}/{} 완료, 기준 {}/{} 충족",
            percentage, grade, passed_tasks, total_tasks, passed_criteria, total_criteria
        );

        (overall_score, percentage, grade, summary)
    }

    fn read_source_files(repo_path: &Path, max_files: usize) -> Vec<(String, String)> {
        let mut files = Vec::new();
        let extensions = [
            "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "kt", "swift", "c", "cpp", "h",
            "hpp", "cs", "rb", "php", "html", "css", "json", "yaml", "yml", "toml", "md",
        ];

        for entry in walkdir::WalkDir::new(repo_path)
            .max_depth(10)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                let path = e.path();
                // Skip hidden files and common non-source directories
                !path
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
                    && !path
                        .components()
                        .any(|c| ["node_modules", "target", "dist", "build", "__pycache__"].contains(&c.as_os_str().to_str().unwrap_or("")))
            })
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| extensions.contains(&ext))
                    .unwrap_or(false)
            })
            .take(max_files)
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
    use crate::types::Criterion;

    #[tokio::test]
    async fn test_create_and_get_grade() {
        let store = GradeStore::new(3600, None, GradeConfig::default());
        let request = GradeRequest {
            repo_url: "https://github.com/test/repo".to_string(),
            branch: None,
            tasks: vec![GradeTask {
                title: "Test Task".to_string(),
                description: None,
                acceptance_criteria: vec![Criterion {
                    id: None,
                    description: "Test criterion".to_string(),
                    weight: 1.0,
                }],
                estimated_minutes: None,
            }],
            config: None,
            metadata: None,
        };

        let id = store.create_grade(request).await;
        let report = store.get_grade(&id).await;

        assert!(report.is_some());
        let report = report.unwrap();
        assert_eq!(report.repo_url, "https://github.com/test/repo");
        assert_eq!(report.status, GradeStatus::Pending);
    }

    #[tokio::test]
    async fn test_subscribe() {
        let store = GradeStore::new(3600, None, GradeConfig::default());
        let request = GradeRequest {
            repo_url: "https://github.com/test/repo".to_string(),
            branch: None,
            tasks: vec![],
            config: None,
            metadata: None,
        };

        let id = store.create_grade(request).await;
        let receiver = store.subscribe(&id).await;

        assert!(receiver.is_some());
    }

    #[test]
    fn test_calculate_task_score() {
        let results = vec![
            CriterionResult {
                criterion: "A".to_string(),
                passed: true,
                confidence: 0.9,
                evidence: "".to_string(),
                code_references: vec![],
                weight: 1.0,
            },
            CriterionResult {
                criterion: "B".to_string(),
                passed: false,
                confidence: 0.8,
                evidence: "".to_string(),
                code_references: vec![],
                weight: 1.0,
            },
        ];

        let (score, status, passed_count) = GradeStore::calculate_task_score(&results);
        assert!((score - 0.5).abs() < 0.01);
        assert_eq!(status, TaskStatus::Partial);
        assert_eq!(passed_count, 1);
    }

    #[test]
    fn test_calculate_final_score() {
        let task_results = vec![
            TaskGradeResult {
                task_title: "Task 1".to_string(),
                score: 1.0,
                status: TaskStatus::Passed,
                criteria_results: vec![],
                passed_count: 2,
                total_count: 2,
            },
            TaskGradeResult {
                task_title: "Task 2".to_string(),
                score: 0.5,
                status: TaskStatus::Partial,
                criteria_results: vec![],
                passed_count: 1,
                total_count: 2,
            },
        ];

        let (score, percentage, grade, _summary) = GradeStore::calculate_final_score(&task_results);
        assert!((score - 0.75).abs() < 0.01);
        assert_eq!(percentage, 75);
        assert_eq!(grade, "양호");
    }
}
