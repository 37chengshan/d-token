use axum::{extract::State, routing::{get, post}, Json, Router};
use dtoken_config::DtokenConfig;
use dtoken_doctor::run_basic_doctor;
use dtoken_types::HealthResponse;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<DtokenConfig>,
    pub default_doctor_port: u16,
}

impl AppState {
    pub fn new(config: DtokenConfig) -> Self {
        Self {
            config: Arc::new(config),
            default_doctor_port: 18001,
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats))
        .route("/stats-history", get(stats_history))
        .route("/metrics", get(metrics))
        .route("/providers", get(providers))
        .route("/routes", get(routes))
        .route("/instances", get(instances))
        .route("/chain/status", get(chain_status))
        .route("/route-decisions/recent", get(recent_route_decisions))
        .route("/logs/recent", get(recent_logs))
        .route("/doctor/run", post(doctor_run))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        component: "dtoken-core-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn stats() -> Json<Value> {
    Json(json!({
        "status": "m1_placeholder",
        "requests": { "total": 0, "failed": 0 },
        "tokens": { "input": 0, "output": 0, "saved": 0, "savings_percent": 0.0 },
        "note": "Dashboard-compatible stats schema will be filled during M2/M4"
    }))
}

async fn stats_history() -> Json<Value> {
    Json(json!({
        "status": "m1_placeholder",
        "history": [],
        "series": { "hourly": [], "daily": [], "weekly": [], "monthly": [] }
    }))
}

async fn metrics() -> String {
    "# d-token M1 metrics placeholder\ndtoken_requests_total 0\n".to_string()
}

async fn providers(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.config.providers))
}

async fn routes(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.config.routes))
}

async fn instances(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.config.instances))
}

async fn chain_status() -> Json<Value> {
    Json(json!({ "status": "not_connected", "mode": "m1_placeholder" }))
}

async fn recent_route_decisions() -> Json<Value> {
    Json(json!([]))
}

async fn recent_logs() -> Json<Value> {
    Json(json!([]))
}

async fn doctor_run(State(state): State<AppState>) -> Json<Value> {
    Json(json!(run_basic_doctor(state.default_doctor_port)))
}
