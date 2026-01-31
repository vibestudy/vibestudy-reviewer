use actix_web::{web, Responder};
use actix_web_lab::sse::{self, Event, Sse};
use futures::stream::StreamExt;
use std::time::Duration;

use crate::error::ApiError;
use crate::orchestrator::ReviewStore;
use crate::types::{CreateReviewResponse, ReviewRequest, ReviewResponse};

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

    let review_id = store.create_review(request.repo_url.clone(), request.include_ai).await;

    let store_clone = store.get_ref().clone();
    let review_id_clone = review_id.clone();
    tokio::spawn(async move {
        if let Err(e) = store_clone.run_review(&review_id_clone).await {
            tracing::error!("Review {} failed: {}", review_id_clone, e);
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

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/health", web::get().to(health))
            .route("/review", web::post().to(create_review))
            .route("/review/{id}", web::get().to(get_review))
            .route("/review/{id}/stream", web::get().to(stream_review)),
    );
}
