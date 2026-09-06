use axum::{
    extract::{Path, State},
    http::{HeaderValue, Method, StatusCode},
    routing::{get, post},
    Json, Router,
};
use daemon_core::{
    config::{ConfigStore, DaemonConfig},
    event_engine::EventEngine,
    event_store::EventStore,
    events::{DaemonEvent, EventSource},
    history::{HistoryRecord, HistoryStore},
    intelligence::{IntelligenceEngine, IntelligenceResult},
    memory::MemoryEngine,
    security::SecuritySnapshot,
    telemetry::{SystemSnapshot, Telemetry},
};
use serde::Deserialize;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tower_http::cors::CorsLayer;

struct AppState {
    telemetry: Mutex<Telemetry>,
    history: HistoryStore,
    events: EventStore,
    config: ConfigStore,
    last_history_write: Mutex<u64>,
    event_sequence: AtomicU64,
}

type SharedState = Arc<AppState>;

#[derive(Debug, Deserialize)]
struct CreateEventRequest {
    name: String,
    #[serde(default)]
    description: String,
    source: EventSource,
    scheduled_at: Option<u64>,
    expected_duration_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MonitorEventRequest {
    signals: u32,
}

#[derive(Debug, Deserialize)]
struct VerifyEventRequest {
    outcome: String,
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct MessageRequest {
    message: String,
}

#[tokio::main]
async fn main() {
    let state: SharedState = Arc::new(AppState {
        telemetry: Mutex::new(Telemetry::new()),
        history: HistoryStore::new("data/history.jsonl"),
        events: EventStore::new("data/events.jsonl"),
        config: ConfigStore::new("data/config.json"),
        last_history_write: Mutex::new(0),
        event_sequence: AtomicU64::new(0),
    });

    let cors = CorsLayer::new()
        .allow_origin([
            "http://127.0.0.1:8080".parse::<HeaderValue>().unwrap(),
            "http://localhost:8080".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT])
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/telemetry", get(get_telemetry))
        .route("/security", get(get_security))
        .route("/intelligence", get(get_intelligence))
        .route("/history", get(get_history))
        .route("/memory", get(get_memory))
        .route("/events", get(get_events).post(create_event))
        .route("/events/{id}/start", post(start_event))
        .route("/events/{id}/monitor", post(monitor_event))
        .route(
            "/events/{id}/expected-completion",
            post(mark_expected_completion),
        )
        .route("/events/{id}/verify", post(verify_event))
        .route("/events/{id}/update", post(update_event))
        .route("/events/{id}/missed", post(mark_event_missed))
        .route("/events/{id}/fail", post(fail_event))
        .route("/config", get(get_config).put(put_config))
        .layer(cors)
        .with_state(Arc::clone(&state));

    let event_state = Arc::clone(&state);
    tokio::spawn(async move {
        let engine = EventEngine::new();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let mut events = match event_state.events.read_all() {
                Ok(events) => events,
                Err(error) => {
                    eprintln!("Event engine read error: {error}");
                    continue;
                }
            };
            let changed = engine.evaluate_all(&mut events, now);
            if changed > 0 {
                for event in &events {
                    if let Err(error) = event_state.events.upsert(event) {
                        eprintln!("Event engine persist error for {}: {error}", event.id);
                    }
                }
                println!("Daemon event engine updated {changed} event(s).");
            }
        }
    });

    let address = "0.0.0.0:8787";

    println!("Daemon OS API listening on http://{address}");

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind API listener");

    axum::serve(listener, app).await.expect("API server failed");
}

async fn health() -> &'static str {
    "ok"
}

async fn get_telemetry(
    State(state): State<SharedState>,
) -> Result<Json<SystemSnapshot>, (StatusCode, String)> {
    let config = state.config.load().map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load daemon configuration: {error}"),
        )
    })?;

    if !config.monitoring_enabled {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Daemon monitoring is disabled.".to_string(),
        ));
    }

    let mut telemetry = state.telemetry.lock().expect("telemetry mutex poisoned");

    Ok(Json(telemetry.snapshot()))
}

async fn get_security() -> Json<SecuritySnapshot> {
    Json(daemon_core::security::snapshot())
}

async fn get_intelligence(
    State(state): State<SharedState>,
) -> Result<Json<IntelligenceResult>, (StatusCode, String)> {
    let config = state.config.load().map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to load daemon configuration: {error}"),
        )
    })?;

    if !config.monitoring_enabled {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Daemon monitoring is disabled.".to_string(),
        ));
    }

    if !config.intelligence_enabled {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Daemon intelligence is disabled.".to_string(),
        ));
    }

    let snapshot = {
        let mut telemetry = state.telemetry.lock().expect("telemetry mutex poisoned");

        telemetry.snapshot()
    };

    let recent_history = state.history.read_recent(10).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to read intelligence history: {error}"),
        )
    })?;

    let previous_snapshot = recent_history.first().map(|record| &record.telemetry);

    let engine = IntelligenceEngine::new();

    let intelligence = engine.analyze(&snapshot, previous_snapshot, &config);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let should_persist = if !config.history_enabled {
        false
    } else {
        let mut last_write = state
            .last_history_write
            .lock()
            .expect("history timer mutex poisoned");

        if now.saturating_sub(*last_write) >= config.history_interval_seconds {
            *last_write = now;
            true
        } else {
            false
        }
    };

    if should_persist {
        state
            .history
            .append(snapshot, intelligence.clone())
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to persist intelligence history: {error}"),
                )
            })?;
    }

    Ok(Json(intelligence))
}

async fn get_history(State(state): State<SharedState>) -> Json<Vec<HistoryRecord>> {
    let records = state
        .history
        .read_recent(50)
        .expect("failed to read intelligence history");

    Json(records)
}

async fn get_memory(State(state): State<SharedState>) -> Json<daemon_core::memory::MemorySummary> {
    let records = state.history.read_recent(100).unwrap_or_default();

    let engine = MemoryEngine::new();

    Json(engine.analyze(&records))
}

async fn get_events(
    State(state): State<SharedState>,
) -> Result<Json<Vec<DaemonEvent>>, (StatusCode, String)> {
    let events = state.events.read_recent(100).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to read Daemon events: {error}"),
        )
    })?;

    Ok(Json(events))
}

async fn create_event(
    State(state): State<SharedState>,
    Json(request): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<DaemonEvent>), (StatusCode, String)> {
    let name = request.name.trim();

    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "event name cannot be empty.".to_string(),
        ));
    }

    if let Some(duration) = request.expected_duration_seconds {
        if duration == 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                "expected_duration_seconds must be greater than zero.".to_string(),
            ));
        }
    }

    if let (Some(scheduled_at), Some(duration)) =
        (request.scheduled_at, request.expected_duration_seconds)
    {
        if scheduled_at.saturating_add(duration) < scheduled_at {
            return Err((
                StatusCode::BAD_REQUEST,
                "event completion time overflowed.".to_string(),
            ));
        }
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let sequence = state.event_sequence.fetch_add(1, Ordering::Relaxed);

    let id = format!(
        "evt-{}-{}-{}",
        timestamp.as_secs(),
        timestamp.subsec_nanos(),
        sequence
    );

    let mut event = DaemonEvent::new(
        id,
        name.to_string(),
        request.source,
        request.scheduled_at,
        request.expected_duration_seconds,
    );

    event.description = request.description.trim().to_string();

    state.events.append(&event).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to persist Daemon event: {error}"),
        )
    })?;

    Ok((StatusCode::CREATED, Json(event)))
}

async fn start_event(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<DaemonEvent>, (StatusCode, String)> {
    let mut event = get_event_or_not_found(&state, &id)?;

    if event.completed_at.is_some()
        || matches!(
            event.status,
            daemon_core::events::EventStatus::Verified
                | daemon_core::events::EventStatus::Updated
                | daemon_core::events::EventStatus::Failed
                | daemon_core::events::EventStatus::Cancelled
        )
    {
        return Err((
            StatusCode::CONFLICT,
            "event has already reached a terminal state.".to_string(),
        ));
    }

    event.start();

    state.events.upsert(&event).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to persist event start: {error}"),
        )
    })?;

    Ok(Json(event))
}

async fn monitor_event(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<MonitorEventRequest>,
) -> Result<Json<DaemonEvent>, (StatusCode, String)> {
    let mut event = get_event_or_not_found(&state, &id)?;

    if event.completed_at.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "cannot monitor a completed event.".to_string(),
        ));
    }

    event.begin_monitoring(request.signals);

    state.events.upsert(&event).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to persist event monitoring state: {error}"),
        )
    })?;

    Ok(Json(event))
}

async fn mark_expected_completion(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<DaemonEvent>, (StatusCode, String)> {
    let mut event = get_event_or_not_found(&state, &id)?;

    if event.completed_at.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "event has already completed.".to_string(),
        ));
    }

    event.mark_expected_completion();

    state.events.upsert(&event).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to persist expected-completion state: {error}"),
        )
    })?;

    Ok(Json(event))
}

async fn verify_event(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<VerifyEventRequest>,
) -> Result<Json<DaemonEvent>, (StatusCode, String)> {
    let outcome = request.outcome.trim();

    if outcome.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "verification outcome cannot be empty.".to_string(),
        ));
    }

    if !request.confidence.is_finite() {
        return Err((
            StatusCode::BAD_REQUEST,
            "confidence must be a finite number.".to_string(),
        ));
    }

    let mut event = get_event_or_not_found(&state, &id)?;

    event.verify(outcome.to_string(), request.confidence);

    state.events.upsert(&event).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to persist event verification: {error}"),
        )
    })?;

    Ok(Json(event))
}

async fn update_event(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<MessageRequest>,
) -> Result<Json<DaemonEvent>, (StatusCode, String)> {
    let message = request.message.trim();

    if message.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "event update message cannot be empty.".to_string(),
        ));
    }

    let mut event = get_event_or_not_found(&state, &id)?;

    event.update(message.to_string());

    state.events.upsert(&event).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to persist event update: {error}"),
        )
    })?;

    Ok(Json(event))
}

async fn mark_event_missed(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<DaemonEvent>, (StatusCode, String)> {
    let mut event = get_event_or_not_found(&state, &id)?;

    if event.completed_at.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "completed events cannot be marked missed.".to_string(),
        ));
    }

    event.mark_missed();

    state.events.upsert(&event).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to persist missed state: {error}"),
        )
    })?;

    Ok(Json(event))
}

async fn fail_event(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(request): Json<MessageRequest>,
) -> Result<Json<DaemonEvent>, (StatusCode, String)> {
    let reason = request.message.trim();

    if reason.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "failure reason cannot be empty.".to_string(),
        ));
    }

    let mut event = get_event_or_not_found(&state, &id)?;

    event.mark_failed(reason.to_string());

    state.events.upsert(&event).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to persist event failure: {error}"),
        )
    })?;

    Ok(Json(event))
}

fn get_event_or_not_found(
    state: &SharedState,
    id: &str,
) -> Result<DaemonEvent, (StatusCode, String)> {
    state
        .events
        .get(id)
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to read event: {error}"),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("event '{id}' was not found."),
            )
        })
}

async fn get_config(State(state): State<SharedState>) -> Json<DaemonConfig> {
    let config = state
        .config
        .load()
        .expect("failed to load daemon configuration");

    Json(config)
}

async fn put_config(
    State(state): State<SharedState>,
    Json(config): Json<DaemonConfig>,
) -> Result<Json<DaemonConfig>, (StatusCode, String)> {
    if !config.monitoring_enabled && config.intelligence_enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            "Intelligence cannot be enabled while monitoring is disabled.".to_string(),
        ));
    }

    if config.history_interval_seconds == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "history_interval_seconds must be greater than zero.".to_string(),
        ));
    }

    let thresholds = [
        (
            "cpu",
            config.cpu_warning_percent,
            config.cpu_critical_percent,
        ),
        (
            "memory",
            config.memory_warning_percent,
            config.memory_critical_percent,
        ),
        (
            "disk",
            config.disk_warning_percent,
            config.disk_critical_percent,
        ),
    ];

    for (name, warning, critical) in thresholds {
        if !(0.0..=100.0).contains(&warning) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("{name}_warning_percent must be between 0 and 100."),
            ));
        }

        if !(0.0..=100.0).contains(&critical) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("{name}_critical_percent must be between 0 and 100."),
            ));
        }

        if warning >= critical {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("{name}_warning_percent must be lower than {name}_critical_percent."),
            ));
        }
    }

    state.config.save(&config).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to save Daemon configuration: {error}"),
        )
    })?;

    Ok(Json(config))
}
