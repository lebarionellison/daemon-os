use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use daemon_core::telemetry::{SystemSnapshot, Telemetry};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct AppState {
    telemetry: Arc<Mutex<Telemetry>>,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        telemetry: Arc::new(Mutex::new(Telemetry::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/telemetry", get(telemetry))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8787")
        .await
        .expect("failed to bind API server");

    println!("Daemon API listening on http://0.0.0.0:8787");

    axum::serve(listener, app)
        .await
        .expect("API server failed");
}

async fn health() -> &'static str {
    "Daemon OS API online"
}

async fn telemetry(
    State(state): State<AppState>,
) -> Json<SystemSnapshot> {
    let mut telemetry = state.telemetry.lock().unwrap();

    Json(telemetry.snapshot())
}