use actix_web::{web, Responder};
use actix_web_lab::sse::{self, Event, Sse};
use futures::stream::StreamExt;
use std::time::Duration;

use crate::error::ApiError;
use crate::grade_orchestrator::GradeStore;
use crate::orchestrator::ReviewStore;
use crate::types::{
    CreateGradeResponse, CreateReviewResponse, GradeRequest, GradeResponse, GradeStatus,
    ReviewRequest, ReviewResponse,
};

pub async fn health() -> impl Responder {
    web::Json(serde_json::json!({"status": "ok"}))
}

pub async fn create_review(
    body: web::Json<ReviewRequest>,
    store: web::Data<ReviewStore>,
) -> Result<impl Responder, ApiError> {
    let request = body.into_inner();

    if request.repo_url.is_empty() {
        return Err(ApiError::BadRequest("repo_url is required".to_string()));
    }

    let review_id = store.create_review(request.repo_url.clone()).await;

    let store_clone = store.get_ref().clone();
    let review_id_clone = review_id.clone();
    tokio::spawn(async move {
        if let Err(e) = store_clone.run_review(&review_id_clone).await {
            tracing::error!("Review {} failed: {}", review_id_clone, e);
            store_clone.mark_failed(&review_id_clone, e.to_string()).await;
        }
    });

    Ok(web::Json(CreateReviewResponse { review_id }))
}

pub async fn get_review(
    path: web::Path<String>,
    store: web::Data<ReviewStore>,
) -> Result<impl Responder, ApiError> {
    let review_id = path.into_inner();

    let state = store
        .get_review(&review_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Review {} not found", review_id)))?;

    Ok(web::Json(ReviewResponse {
        id: state.id.clone(),
        status: state.status,
        repo_url: state.repo_url.clone(),
        results: state.results.clone(),
        suggestions: state.suggestions.clone(),
        error: state.error.clone(),
    }))
}

pub async fn stream_review(
    path: web::Path<String>,
    store: web::Data<ReviewStore>,
) -> Result<impl Responder, ApiError> {
    let review_id = path.into_inner();

    let receiver = store
        .subscribe(&review_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Review {} not found", review_id)))?;

    let stream = tokio_stream::wrappers::BroadcastStream::new(receiver).filter_map(|result| async move {
        match result {
            Ok(event) => {
                let data = serde_json::to_string(&event).ok()?;
                Some(Ok::<_, std::convert::Infallible>(Event::Data(
                    sse::Data::new(data),
                )))
            }
            Err(_) => None,
        }
    });

    Ok(Sse::from_stream(stream).with_keep_alive(Duration::from_secs(15)))
}

pub async fn create_grade(
    body: web::Json<GradeRequest>,
    store: web::Data<GradeStore>,
) -> Result<impl Responder, ApiError> {
    let request = body.into_inner();

    if request.repo_url.is_empty() {
        return Err(ApiError::BadRequest("repo_url is required".to_string()));
    }

    if request.tasks.is_empty() {
        return Err(ApiError::BadRequest("tasks cannot be empty".to_string()));
    }

    let grade_id = store.create_grade(request.clone()).await;

    let store_clone = store.get_ref().clone();
    let grade_id_clone = grade_id.clone();
    tokio::spawn(async move {
        if let Err(e) = store_clone.run_grade(&grade_id_clone, request).await {
            tracing::error!("Grade {} failed: {}", grade_id_clone, e);
        }
    });

    Ok(web::Json(CreateGradeResponse {
        grade_id,
        status: GradeStatus::Pending,
    }))
}

pub async fn get_grade(
    path: web::Path<String>,
    store: web::Data<GradeStore>,
) -> Result<impl Responder, ApiError> {
    let grade_id = path.into_inner();

    let report = store
        .get_grade(&grade_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Grade {} not found", grade_id)))?;

    Ok(web::Json(GradeResponse {
        id: report.id,
        status: report.status,
        repo_url: report.repo_url,
        overall_score: report.overall_score,
        percentage: report.percentage,
        grade: report.grade,
        tasks: report.tasks,
        summary: report.summary,
        error: report.error,
    }))
}

pub async fn stream_grade(
    path: web::Path<String>,
    store: web::Data<GradeStore>,
) -> Result<impl Responder, ApiError> {
    let grade_id = path.into_inner();

    let receiver = store
        .subscribe(&grade_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("Grade {} not found", grade_id)))?;

    let stream = tokio_stream::wrappers::BroadcastStream::new(receiver).filter_map(|result| async move {
        match result {
            Ok(event) => {
                let data = serde_json::to_string(&event).ok()?;
                Some(Ok::<_, std::convert::Infallible>(Event::Data(
                    sse::Data::new(data),
                )))
            }
            Err(_) => None,
        }
    });

    Ok(Sse::from_stream(stream).with_keep_alive(Duration::from_secs(15)))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/health", web::get().to(health))
            .route("/review", web::post().to(create_review))
            .route("/review/{id}", web::get().to(get_review))
            .route("/review/{id}/stream", web::get().to(stream_review))
            .route("/grade", web::post().to(create_grade))
            .route("/grade/{id}", web::get().to(get_grade))
            .route("/grade/{id}/stream", web::get().to(stream_grade)),
    );
}
