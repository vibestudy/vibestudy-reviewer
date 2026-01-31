use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize, Deserialize)]
struct Message {
    id: u32,
    content: String,
}

async fn health() -> impl Responder {
    HttpResponse::Ok().json(HealthResponse { status: "ok" })
}

async fn get_messages() -> impl Responder {
    let messages = vec![
        Message { id: 1, content: "Hello, World!".to_string() },
        Message { id: 2, content: "Welcome to Actix Web".to_string() },
    ];
    HttpResponse::Ok().json(messages)
}

async fn get_message(path: web::Path<u32>) -> impl Responder {
    let id = path.into_inner();
    let message = Message {
        id,
        content: format!("Message with id: {}", id),
    };
    HttpResponse::Ok().json(message)
}

async fn create_message(body: web::Json<Message>) -> impl Responder {
    HttpResponse::Created().json(body.into_inner())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let port = 8080;
    println!("Starting server at http://localhost:{}", port);

    HttpServer::new(|| {
        App::new()
            .route("/health", web::get().to(health))
            .route("/messages", web::get().to(get_messages))
            .route("/messages/{id}", web::get().to(get_message))
            .route("/messages", web::post().to(create_message))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
