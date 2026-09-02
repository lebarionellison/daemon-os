use axum::{
    extract::State,
    http::{HeaderValue, Method},
    routing::get,
    Json, Router,
};
use daemon_core::telemetry::{SystemSnapshot, Telemetry};
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;

type AppState = Arc<Mutex<Telemetry>>;

#[tokio::main]
async fn main() {
    let state: AppState = Arc::new(Mutex::new(Telemetry::new()));

    let cors = CorsLayer::new()
        .allow_origin([
            "http://127.0.0.1:8080".parse::<HeaderValue>().unwrap(),
            "http://localhost:8080".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([Method::GET]);

    let app = Router::new()
        .route("/health", get(health))
        .route("/telemetry", get(get_telemetry))
        .layer(cors)
        .with_state(state);

    let address = "0.0.0.0:8787";

    println!("Daemon OS API listening on http://{address}");

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind API listener");

    axum::serve(listener, app)
        .await
        .expect("API server failed");
}

async fn health() -> &'static str {
    "ok"
}

async fn get_telemetry(
    State(state): State<AppState>,
) -> Json<SystemSnapshot> {
    let mut telemetry = state
        .lock()
        .expect("telemetry mutex poisoned");

    Json(telemetry.snapshot())
}