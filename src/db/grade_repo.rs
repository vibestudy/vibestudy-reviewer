use bson::{doc, oid::ObjectId, DateTime as BsonDateTime};
use mongodb::Collection;
use serde::{Deserialize, Serialize};

use crate::{
    db::MongoClient,
    types::{GradeReport, GradeRequest, GradeStatus},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradeJob {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub curriculum_id: Option<String>,
    pub task_id: Option<String>,
    pub repo_url: String,
    pub branch: Option<String>,
    pub status: GradeStatus,
    pub request: bson::Document,
    pub result: Option<bson::Document>,
    pub error: Option<String>,
    pub created_at: BsonDateTime,
    pub completed_at: Option<BsonDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGradeUpdate {
    pub grade_job_id: String,
    pub score: f32,
    pub percentage: u32,
    pub grade: String,
    pub criteria_results: Vec<bson::Document>,
    pub repo_url: String,
    pub graded_at: BsonDateTime,
}

pub struct GradeRepository {
    client: MongoClient,
}

impl GradeRepository {
    pub fn new(client: MongoClient) -> Self {
        Self { client }
    }

    fn grade_jobs_collection(&self) -> Collection<GradeJob> {
        self.client.database().collection("grade_jobs")
    }

    fn tasks_collection(&self) -> Collection<bson::Document> {
        self.client.database().collection("tasks")
    }

    pub async fn save_grade_job(
        &self,
        request: &GradeRequest,
        curriculum_id: Option<String>,
        task_id: Option<String>,
    ) -> Result<String, mongodb::error::Error> {
        let request_doc = bson::to_document(request)
            .map_err(|e| mongodb::error::Error::custom(format!("Failed to serialize request: {}", e)))?;

        let job = GradeJob {
            id: None,
            curriculum_id,
            task_id,
            repo_url: request.repo_url.clone(),
            branch: request.branch.clone(),
            status: GradeStatus::Pending,
            request: request_doc,
            result: None,
            error: None,
            created_at: BsonDateTime::now(),
            completed_at: None,
        };

        let result = self.grade_jobs_collection().insert_one(job).await?;
        Ok(result.inserted_id.as_object_id().unwrap().to_hex())
    }

    pub async fn update_grade_job(
        &self,
        id: &str,
        report: &GradeReport,
    ) -> Result<(), mongodb::error::Error> {
        let oid = ObjectId::parse_str(id)
            .map_err(|e| mongodb::error::Error::custom(format!("Invalid ObjectId: {}", e)))?;

        let result_doc = bson::to_document(report)
            .map_err(|e| mongodb::error::Error::custom(format!("Failed to serialize report: {}", e)))?;

        self.grade_jobs_collection()
            .update_one(
                doc! { "_id": oid },
                doc! {
                    "$set": {
                        "status": bson::to_bson(&report.status).unwrap(),
                        "result": result_doc,
                        "error": &report.error,
                        "completed_at": BsonDateTime::now(),
                    }
                },
            )
            .await?;

        Ok(())
    }

    pub async fn get_grade_job(
        &self,
        id: &str,
    ) -> Result<Option<GradeJob>, mongodb::error::Error> {
        let oid = ObjectId::parse_str(id)
            .map_err(|e| mongodb::error::Error::custom(format!("Invalid ObjectId: {}", e)))?;

        self.grade_jobs_collection()
            .find_one(doc! { "_id": oid })
            .await
    }

    pub async fn update_task_grade(
        &self,
        curriculum_id: &str,
        task_id: &str,
        report: &GradeReport,
    ) -> Result<(), mongodb::error::Error> {
        let curriculum_oid = ObjectId::parse_str(curriculum_id)
            .map_err(|e| mongodb::error::Error::custom(format!("Invalid curriculum_id: {}", e)))?;

        let grade_result = TaskGradeUpdate {
            grade_job_id: report.id.clone(),
            score: report.overall_score,
            percentage: report.percentage,
            grade: report.grade.clone(),
            criteria_results: report
                .tasks
                .iter()
                .flat_map(|t| {
                    t.criteria_results
                        .iter()
                        .filter_map(|c| bson::to_document(c).ok())
                })
                .collect(),
            repo_url: report.repo_url.clone(),
            graded_at: BsonDateTime::now(),
        };

        let grade_result_doc = bson::to_document(&grade_result)
            .map_err(|e| mongodb::error::Error::custom(format!("Failed to serialize grade_result: {}", e)))?;

        let status = if report.overall_score >= 0.9 {
            "passed"
        } else if report.overall_score >= 0.4 {
            "partial"
        } else {
            "failed"
        };

        self.tasks_collection()
            .update_one(
                doc! {
                    "_id": task_id,
                    "curriculum_id": curriculum_oid,
                },
                doc! {
                    "$set": {
                        "status": status,
                        "grade_result": grade_result_doc,
                        "updated_at": BsonDateTime::now(),
                    }
                },
            )
            .await?;

        Ok(())
    }
}
