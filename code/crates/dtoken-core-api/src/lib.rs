use axum::{extract::State, routing::{get, post}, Json, Router};
use dtoken_config::DtokenConfig;
use dtoken_config_patcher::mock_patch_plan;
use dtoken_doctor::run_basic_doctor;
use dtoken_event_ledger::EventLedger;
use dtoken_headroom_adapter::{detect_headroom_status, headroom_cache_summary_mock, headroom_stats_mock};
use dtoken_preprocessor::{detect_rtk_status, list_preprocessors, rtk_gain_mock};
use dtoken_project_policy::{default_policy, demo_projects, RecommendationInput, RuleBasedRecommendationEngine};
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
        .route("/preprocessors", get(preprocessors))
        .route("/rtk/status", get(rtk_status))
        .route("/rtk/gain", get(rtk_gain))
        .route("/rtk/discover", get(rtk_discover))
        .route("/rtk/session", get(rtk_session))
        .route("/headroom/status", get(headroom_status))
        .route("/headroom/proxy", get(headroom_proxy))
        .route("/headroom/mcp", get(headroom_mcp))
        .route("/headroom/stats", get(headroom_stats))
        .route("/headroom/cache-summary", get(headroom_cache_summary))
        .route("/projects", get(projects))
        .route("/projects/default-policy", get(project_default_policy))
        .route("/projects/recommend-compression", post(project_recommend_compression))
        .route("/config/patches/plan", post(config_patch_plan))
        .route("/config/rollback-points", get(config_rollback_points))
        .route("/events", get(events))
        .route("/install/scan", get(install_scan))
        .route("/install/plan", post(install_plan))
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
        "status": "m1_mock",
        "requests": { "total": 0, "failed": 0 },
        "tokens": { "input": 0, "output": 0, "saved": 0, "savings_percent": 0.0 },
        "layered": {
            "rtk_savings": 0,
            "headroom_savings": 0,
            "end_to_end_savings": 0,
            "attribution_confidence": "m1_mock"
        }
    }))
}

async fn stats_history() -> Json<Value> {
    Json(json!({
        "status": "m1_mock",
        "history": [],
        "series": { "hourly": [], "daily": [], "weekly": [], "monthly": [] }
    }))
}

async fn metrics() -> String {
    "# d-token M1 metrics placeholder\ndtoken_requests_total 0\ndtoken_rtk_saved_tokens 0\ndtoken_headroom_saved_tokens 0\n".to_string()
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

async fn preprocessors() -> Json<Value> {
    Json(json!(list_preprocessors().await))
}

async fn rtk_status() -> Json<Value> {
    Json(json!(detect_rtk_status().await))
}

async fn rtk_gain() -> Json<Value> {
    Json(json!(rtk_gain_mock().await))
}

async fn rtk_discover() -> Json<Value> {
    Json(json!({
        "available": false,
        "source": "m1_mock",
        "missed_opportunities": []
    }))
}

async fn rtk_session() -> Json<Value> {
    Json(json!({
        "available": false,
        "source": "m1_mock",
        "sessions": []
    }))
}

async fn headroom_status() -> Json<Value> {
    Json(json!(detect_headroom_status().await))
}

async fn headroom_proxy() -> Json<Value> {
    Json(json!({
        "running": false,
        "proxy_url": "http://127.0.0.1:8787",
        "source": "m1_mock"
    }))
}

async fn headroom_mcp() -> Json<Value> {
    Json(json!({
        "configured": false,
        "tools": ["headroom_compress", "headroom_retrieve", "headroom_stats"],
        "source": "m1_mock"
    }))
}

async fn headroom_stats() -> Json<Value> {
    Json(json!(headroom_stats_mock().await))
}

async fn headroom_cache_summary() -> Json<Value> {
    Json(json!(headroom_cache_summary_mock()))
}

async fn projects() -> Json<Value> {
    Json(json!(demo_projects()))
}

async fn project_default_policy() -> Json<Value> {
    Json(json!(default_policy("demo-project")))
}

async fn project_recommend_compression() -> Json<Value> {
    let engine = RuleBasedRecommendationEngine::default();
    let input = RecommendationInput {
        project_id: "demo-project".to_string(),
        language_stack: vec!["rust".to_string(), "typescript".to_string()],
        avg_request_tokens_7d: 20_000,
        avg_cli_noise_ratio_7d: 0.25,
        retrieve_rate_7d: 0.02,
        failure_rate_7d: 0.01,
        high_risk_task: false,
        manual_override: false,
    };
    Json(json!(engine.recommend(input)))
}

async fn config_patch_plan() -> Json<Value> {
    Json(json!(mock_patch_plan()))
}

async fn config_rollback_points() -> Json<Value> {
    Json(json!([]))
}

async fn events() -> Json<Value> {
    Json(json!(EventLedger::mock_m1().recent(20)))
}

async fn install_scan() -> Json<Value> {
    Json(json!({
        "status": "m1_mock",
        "checks": [
            { "name": "dtoken-core", "ok": true },
            { "name": "rtk", "ok": false, "note": "detected dynamically via /rtk/status" },
            { "name": "headroom", "ok": false, "note": "detected dynamically via /headroom/status" }
        ]
    }))
}

async fn install_plan() -> Json<Value> {
    Json(json!({
        "status": "m1_mock",
        "transaction": {
            "id": "m1-install-plan",
            "steps": ["scan", "select_tools", "show_diff", "backup", "apply", "doctor"],
            "requires_user_confirmation": true
        }
    }))
}
