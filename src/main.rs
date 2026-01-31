use actix_web::{App, HttpServer, middleware, web};
use api_server::api;
use api_server::config::AppConfig;
use api_server::db::{GradeRepository, MongoClient};
use api_server::grade_orchestrator::GradeStore;
use api_server::orchestrator::ReviewStore;
use api_server::shutdown::shutdown_signal;
use api_server::types::GradeConfig;
use secrecy::ExposeSecret;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_server=info,actix_web=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env().expect("Failed to load configuration");
    let review_store = ReviewStore::new(config.review.review_ttl_secs, Some(config.providers.clone()));

    let grade_repo = if let Some(ref mongodb_url) = config.mongo.mongodb_url {
        match MongoClient::new(mongodb_url.expose_secret(), &config.mongo.mongodb_db_name).await {
            Ok(client) => {
                tracing::info!("MongoDB connected for grade persistence");
                Some(Arc::new(GradeRepository::new(client)))
            }
            Err(e) => {
                tracing::warn!("Failed to connect to MongoDB: {}. Grade persistence disabled.", e);
                None
            }
        }
    } else {
        tracing::info!("MongoDB not configured. Grade persistence disabled.");
        None
    };

    let grade_store = GradeStore::new(
        config.review.review_ttl_secs,
        Some(config.providers.clone()),
        GradeConfig::default(),
        grade_repo,
    );

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!("Starting server at http://{}", bind_addr);

    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            .app_data(web::Data::new(review_store.clone()))
            .app_data(web::Data::new(grade_store.clone()))
            .configure(api::configure)
    })
    .bind(&bind_addr)?
    .run();

    tokio::select! {
        result = server => result,
        _ = shutdown_signal() => {
            tracing::info!("Shutting down gracefully");
            Ok(())
        }
    }
}
