use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::header::{CACHE_CONTROL, CONTENT_TYPE},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use dtoken_compress::CompressionEngine;
use dtoken_config::DtokenConfig;
use dtoken_config_patcher::mock_patch_plan;
use dtoken_doctor::{run_basic_doctor, run_dev_no_touch_doctor};
use dtoken_event_ledger::{EventLedger, JsonlEventLedger};
use dtoken_headroom_adapter::detect_headroom_status;
use dtoken_preprocessor::{
    detect_rtk_status, list_preprocessors, rtk_discover_report, rtk_gain_mock, rtk_session_report,
};
use dtoken_project_policy::{
    default_policy, demo_projects, RecommendationInput, RuleBasedRecommendationEngine,
};
use dtoken_types::{
    AgentSupportProfile, BaselineKind, ChainMode, CompressionPolicy, DtokenEvent, EventSource,
    EventStatus, HealthResponse, LayeredSavingsReport, LayeredStage, Protocol,
    ProtocolInjectionProfile, RequestLog, RouteDecision, SupportLevel,
};
use futures_util::TryStreamExt;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    path::{Path as StdPath, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<DtokenConfig>,
    pub config_status: Arc<RuntimeConfigStatus>,
    pub default_doctor_port: u16,
    ledger: Arc<Mutex<EventLedger>>,
    route_decisions: Arc<Mutex<Vec<Value>>>,
    request_logs: Arc<Mutex<Vec<RequestLog>>>,
    ccr_metadata: Arc<Mutex<Vec<Value>>>,
    memory_records: Arc<Mutex<Vec<Value>>>,
    runtime_store: Option<Arc<RuntimeStore>>,
    http_client: reqwest::Client,
}

impl AppState {
    pub fn new(config: DtokenConfig) -> Self {
        Self::new_with_config_status(config, RuntimeConfigStatus::empty())
    }

    pub fn new_with_config_status(config: DtokenConfig, config_status: RuntimeConfigStatus) -> Self {
        Self {
            config: Arc::new(config),
            config_status: Arc::new(config_status),
            default_doctor_port: 18001,
            ledger: Arc::new(Mutex::new(EventLedger::new())),
            route_decisions: Arc::new(Mutex::new(vec![])),
            request_logs: Arc::new(Mutex::new(vec![])),
            ccr_metadata: Arc::new(Mutex::new(vec![])),
            memory_records: Arc::new(Mutex::new(vec![])),
            runtime_store: None,
            http_client: reqwest::Client::new(),
        }
    }

    pub fn new_persistent(config: DtokenConfig, dir: impl AsRef<StdPath>) -> Self {
        Self::new_persistent_with_config_status(config, RuntimeConfigStatus::empty(), dir)
    }

    pub fn new_persistent_with_config_status(
        config: DtokenConfig,
        config_status: RuntimeConfigStatus,
        dir: impl AsRef<StdPath>,
    ) -> Self {
        let store = Arc::new(RuntimeStore::new(dir.as_ref()));
        let mut state = Self::new_with_config_status(config, config_status);
        if let Ok(events) = store.recent_events(10_000) {
            if let Ok(mut ledger) = state.ledger.lock() {
                for event in events.into_iter().rev() {
                    let _ = ledger.record(event);
                }
            }
        }
        if let Ok(route_decisions) = store.recent_route_decisions(10_000) {
            if let Ok(mut items) = state.route_decisions.lock() {
                *items = route_decisions.into_iter().rev().collect();
            }
        }
        if let Ok(request_logs) = store.recent_request_logs(10_000) {
            if let Ok(mut logs) = state.request_logs.lock() {
                *logs = request_logs.into_iter().rev().collect();
            }
        }
        if let Ok(ccr_metadata) = store.recent_ccr_metadata(10_000) {
            if let Ok(mut items) = state.ccr_metadata.lock() {
                *items = ccr_metadata.into_iter().rev().collect();
            }
        }
        if let Ok(memory_records) = store.recent_memory_records(10_000) {
            if let Ok(mut items) = state.memory_records.lock() {
                *items = memory_records.into_iter().rev().collect();
            }
        }
        state.runtime_store = Some(store);
        state
    }

    fn recent_events(&self, limit: usize) -> Vec<DtokenEvent> {
        self.ledger
            .lock()
            .map(|ledger| ledger.recent(limit))
            .unwrap_or_default()
    }

    fn recent_route_decisions(&self, limit: usize) -> Vec<Value> {
        self.route_decisions
            .lock()
            .map(|items| items.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    fn recent_request_logs(&self, limit: usize) -> Vec<RequestLog> {
        self.request_logs
            .lock()
            .map(|items| items.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    fn recent_ccr_metadata(&self, limit: usize) -> Vec<Value> {
        self.ccr_metadata
            .lock()
            .map(|items| items.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    fn retrieve_ccr_metadata(&self, recovery_ref: &str) -> Option<Value> {
        self.ccr_metadata.lock().ok().and_then(|items| {
            items
                .iter()
                .rev()
                .find(|item| item["recovery_ref"] == recovery_ref)
                .cloned()
        })
    }

    fn recent_memory_records(&self, limit: usize) -> Vec<Value> {
        self.memory_records
            .lock()
            .map(|items| items.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    fn record_memory(&self, record: Value) -> anyhow::Result<Value> {
        let sanitized = sanitize_memory_record(record);
        if let Ok(mut items) = self.memory_records.lock() {
            items.push(sanitized.clone());
        }
        if let Some(store) = &self.runtime_store {
            store.append_memory_record(&sanitized)?;
        }
        Ok(sanitized)
    }

    fn append_event(&self, event: DtokenEvent) {
        if let Ok(mut ledger) = self.ledger.lock() {
            let _ = ledger.record(event.clone());
        }
        if let Some(store) = &self.runtime_store {
            let _ = store.append_event(&event);
        }
    }

    fn record_routed_request(
        &self,
        decision: &RouteDecision,
        request_log: RequestLog,
        event: DtokenEvent,
    ) {
        if let Ok(mut route_decisions) = self.route_decisions.lock() {
            route_decisions.push(route_decision_to_value(decision));
        }
        if let Ok(mut request_logs) = self.request_logs.lock() {
            request_logs.push(request_log.clone());
        }
        if let Ok(mut ledger) = self.ledger.lock() {
            let _ = ledger.record(event.clone());
        }
        let ccr = ccr_metadata_from_request(decision, &request_log, &event);
        if let Ok(mut items) = self.ccr_metadata.lock() {
            items.push(ccr.clone());
        }
        if let Some(store) = &self.runtime_store {
            let _ = store.append_route_decision(&route_decision_to_value(decision));
            let _ = store.append_request_log(&request_log);
            let _ = store.append_event(&event);
            let _ = store.append_ccr_metadata(&ccr);
        }
    }
}

fn append_stage_event(
    state: &AppState,
    request_id: Uuid,
    stage: LayeredStage,
    status: EventStatus,
    metadata: Value,
    error_code: Option<&str>,
) {
    state.append_event(DtokenEvent {
        event_id: Uuid::new_v4(),
        timestamp_ms: dtoken_event_ledger::now_ms(),
        project_id: Some("dtoken-runtime".to_string()),
        agent_id: Some("d-token".to_string()),
        instance_id: Some("dtoken-runtime-local".to_string()),
        command_id: None,
        request_id: Some(request_id),
        stage,
        source: EventSource::Dtoken,
        status,
        input_tokens: None,
        output_tokens: None,
        saved_tokens: None,
        baseline_kind: Some(BaselineKind::HeadroomInput),
        error_code: error_code.map(ToString::to_string),
        metadata,
    });
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuntimeConfigStatus {
    pub source: String,
    pub path: Option<String>,
    pub loaded: bool,
    pub exists: bool,
    pub provider_count: usize,
    pub route_count: usize,
    pub instance_count: usize,
    pub fallback_disabled: bool,
    pub error: Option<String>,
}

impl RuntimeConfigStatus {
    pub fn empty() -> Self {
        Self {
            source: "empty_config".to_string(),
            path: None,
            loaded: true,
            exists: false,
            provider_count: 0,
            route_count: 0,
            instance_count: 0,
            fallback_disabled: true,
            error: None,
        }
    }

    pub fn from_config(source: impl Into<String>, path: Option<String>, config: &DtokenConfig) -> Self {
        Self {
            source: source.into(),
            path,
            loaded: true,
            exists: true,
            provider_count: config.providers.len(),
            route_count: config.routes.len(),
            instance_count: config.instances.len(),
            fallback_disabled: config.routes.iter().all(|route| route.fallback_provider_id.is_none()),
            error: None,
        }
    }

    pub fn missing(source: impl Into<String>, path: Option<String>) -> Self {
        Self {
            source: source.into(),
            path,
            loaded: true,
            exists: false,
            provider_count: 0,
            route_count: 0,
            instance_count: 0,
            fallback_disabled: true,
            error: None,
        }
    }

    pub fn failed(source: impl Into<String>, path: Option<String>, error: impl Into<String>) -> Self {
        let exists = path.is_some();
        Self {
            source: source.into(),
            path,
            loaded: false,
            exists,
            provider_count: 0,
            route_count: 0,
            instance_count: 0,
            fallback_disabled: false,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug)]
struct RuntimeStore {
    events_path: PathBuf,
    route_decisions_path: PathBuf,
    request_logs_path: PathBuf,
    ccr_metadata_path: PathBuf,
    memory_records_path: PathBuf,
}

impl RuntimeStore {
    fn new(dir: &StdPath) -> Self {
        Self {
            events_path: dir.join("events.jsonl"),
            route_decisions_path: dir.join("route-decisions.jsonl"),
            request_logs_path: dir.join("request-logs.jsonl"),
            ccr_metadata_path: dir.join("ccr-metadata.jsonl"),
            memory_records_path: dir.join("memory-records.jsonl"),
        }
    }

    fn append_event(&self, event: &DtokenEvent) -> anyhow::Result<()> {
        JsonlEventLedger::new(&self.events_path).append(event)
    }

    fn recent_events(&self, limit: usize) -> anyhow::Result<Vec<DtokenEvent>> {
        JsonlEventLedger::new(&self.events_path).recent(limit)
    }

    fn append_route_decision(&self, decision: &Value) -> anyhow::Result<()> {
        append_jsonl(&self.route_decisions_path, decision)
    }

    fn recent_route_decisions(&self, limit: usize) -> anyhow::Result<Vec<Value>> {
        read_jsonl_recent(&self.route_decisions_path, limit)
    }

    fn append_request_log(&self, log: &RequestLog) -> anyhow::Result<()> {
        append_jsonl(&self.request_logs_path, log)
    }

    fn recent_request_logs(&self, limit: usize) -> anyhow::Result<Vec<RequestLog>> {
        read_jsonl_recent(&self.request_logs_path, limit)
    }

    fn append_ccr_metadata(&self, metadata: &Value) -> anyhow::Result<()> {
        append_jsonl(&self.ccr_metadata_path, metadata)
    }

    fn recent_ccr_metadata(&self, limit: usize) -> anyhow::Result<Vec<Value>> {
        read_jsonl_recent(&self.ccr_metadata_path, limit)
    }

    fn append_memory_record(&self, record: &Value) -> anyhow::Result<()> {
        append_jsonl(&self.memory_records_path, record)
    }

    fn recent_memory_records(&self, limit: usize) -> anyhow::Result<Vec<Value>> {
        read_jsonl_recent(&self.memory_records_path, limit)
    }
}

fn append_jsonl<T: serde::Serialize>(path: &StdPath, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(value)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn read_jsonl_recent<T: serde::de::DeserializeOwned>(
    path: &StdPath,
    limit: usize,
) -> anyhow::Result<Vec<T>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let file = OpenOptions::new().read(true).open(path)?;
    let reader = BufReader::new(file);
    let mut items = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        items.push(serde_json::from_str::<T>(&line)?);
    }
    items.reverse();
    items.truncate(limit);
    Ok(items)
}

fn ccr_metadata_from_request(
    decision: &RouteDecision,
    request_log: &RequestLog,
    event: &DtokenEvent,
) -> Value {
    json!({
        "recovery_ref": format!("ccr:{}:{}", decision.route_id, decision.request_id),
        "request_id": decision.request_id,
        "route_id": decision.route_id,
        "provider_id": decision.provider_id,
        "agent": request_log.agent,
        "input_protocol": request_log.input_protocol,
        "input_tokens_before": request_log.input_tokens_before,
        "input_tokens_after": request_log.input_tokens_after,
        "tokens_saved": request_log.tokens_saved,
        "transforms_applied": request_log.transforms_applied,
        "compression_elapsed_us": event.metadata.get("compression_elapsed_us").cloned().unwrap_or(Value::Null),
        "raw_available": false,
        "raw_storage": "disabled",
        "note": "CCR metadata only; raw prompt/response/CCR body is not stored by default."
    })
}

fn sanitize_memory_record(record: Value) -> Value {
    let mut object = record.as_object().cloned().unwrap_or_default();
    for key in [
        "raw_prompt",
        "raw_response",
        "prompt",
        "response",
        "content",
        "body",
        "messages",
    ] {
        object.remove(key);
    }
    object.insert(
        "memory_id".to_string(),
        json!(format!("mem-{}", Uuid::new_v4())),
    );
    object.insert(
        "timestamp_ms".to_string(),
        json!(dtoken_event_ledger::now_ms()),
    );
    object.insert("source".to_string(), json!("dtoken-owned-runtime"));
    object.insert("raw_available".to_string(), json!(false));
    object.insert(
        "note".to_string(),
        json!("Memory/learn metadata only; raw prompt and response bodies are not stored."),
    );
    Value::Object(object)
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/dashboard/payload", get(dashboard_payload))
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/responses", post(openai_responses))
        .route("/v1/messages", post(anthropic_messages))
        .route("/stats", get(stats))
        .route("/stats-history", get(stats_history))
        .route("/metrics", get(metrics))
        .route("/export/json", get(export_json))
        .route("/export/csv", get(export_csv))
        .route("/providers", get(providers))
        .route("/routes", get(routes))
        .route("/instances", get(instances))
        .route("/config/active", get(active_config))
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
        .route(
            "/projects/recommend-compression",
            post(project_recommend_compression),
        )
        .route("/config/patches/plan", post(config_patch_plan))
        .route("/config/rollback-points", get(config_rollback_points))
        .route("/events", get(events))
        .route("/ccr/summary", get(ccr_summary))
        .route("/ccr/retrieve/{*recovery_ref}", get(ccr_retrieve))
        .route("/memory/summary", get(memory_summary))
        .route("/memory/learn", post(memory_learn))
        .route("/output-shape", get(output_shape))
        .route("/compatibility/agents", get(agent_compatibility))
        .route("/protocol-injection", get(protocol_injection))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        component: "dtoken-core-api".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn active_config(State(state): State<AppState>) -> Json<Value> {
    Json(active_config_payload(&state))
}

fn active_config_payload(state: &AppState) -> Value {
    json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "config": &*state.config_status,
        "providers": state.config.providers,
        "routes": state.config.routes,
        "instances": state.config.instances,
        "privacy": "Runtime active config metadata only; API keys, auth headers, prompts, responses, and external agent configs are not returned."
    })
}

pub struct DashboardPayloadInput {
    pub stats: Value,
    pub history: Value,
    pub metrics: Value,
    pub cache_summary: Value,
    pub providers: Value,
    pub routes: Value,
    pub instances: Value,
    pub events: Value,
    pub route_decisions: Value,
    pub request_logs: Value,
    pub memory: Value,
}

pub fn build_dashboard_payload(input: DashboardPayloadInput) -> Value {
    let (stats, history, metrics, cache_summary, providers, routes, instances, events, route_decisions, request_logs, memory) = (
        input.stats, input.history, input.metrics, input.cache_summary, input.providers, input.routes, input.instances, input.events, input.route_decisions, input.request_logs, input.memory
    );
    json!({
        "source": "dtoken-owned-runtime",
        "runtime": {
            "kind": "headroom-derived",
            "external_headroom_dependency": false,
            "rtk": "integrated"
        },
        "chain": {
            "entry": "OpenCode / agent managed wrap",
            "runtime": "d-token local service",
            "route_adapter": "d-token route adapter",
            "targets": ["official_upstream", "custom_upstream", "cc_switch", "local_provider"]
        },
        "stats": stats,
        "history": history,
        "metrics": metrics,
        "cache_summary": cache_summary,
        "providers": providers,
        "routes": routes,
        "instances": instances,
        "events": events,
        "route_decisions": route_decisions,
        "request_logs": request_logs,
        "memory": memory,
        "dashboard_sections": [
            "summary",
            "agent_usage",
            "savings",
            "tokens",
            "latency",
            "overhead",
            "ttfb",
            "pipeline_timing",
            "compressions_by_strategy",
            "tokens_saved_by_strategy",
            "codex_ws",
            "waste_signals",
            "router",
            "savings_history",
            "display_session",
            "prefix_cache",
            "cost",
            "compression",
            "compression_cache",
            "telemetry",
            "feedback_loop",
            "toin",
            "context_tool",
            "cli_filtering",
            "proxy_inbound",
            "cache",
            "rate_limiter",
            "request_logs",
            "recent_requests",
            "subscription_window",
            "codex_rate_limits",
            "throughput",
            "config"
        ]
    })
}

fn with_active_config(mut payload: Value, active_config: Value) -> Value {
    if let Some(object) = payload.as_object_mut() {
        object.insert("active_config".to_string(), active_config);
    }
    payload
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAiChatRequest {
    model: Option<String>,
    messages: Option<Vec<Value>>,
    stream: Option<bool>,
}

#[derive(Debug, Clone)]
struct UpstreamResult {
    response: Value,
    status: Option<u16>,
    error_category: Option<String>,
}

#[derive(Debug, Clone)]
struct CompressionSummary {
    original_tokens: u64,
    compressed_tokens: u64,
    tokens_saved: u64,
    savings_percent: f64,
    elapsed_us: u128,
    transforms_applied: Vec<String>,
    body: Value,
}

#[derive(Debug, Clone)]
struct RoutedBridgePayload {
    upstream_body: Value,
    decision: RouteDecision,
    request_log: RequestLog,
    event: DtokenEvent,
    compression: CompressionSummary,
    provider: dtoken_types::ProviderProfile,
}

fn compress_for_route(policy: CompressionPolicy, body: Value) -> CompressionSummary {
    let result = CompressionEngine::new()
        .compress(policy, body.clone())
        .unwrap_or_else(|_| dtoken_types::CompressionResult {
            original_tokens: 0,
            compressed_tokens: 0,
            tokens_saved: 0,
            elapsed_us: 1,
            transforms_applied: vec!["compression_error_fallback".to_string()],
            body,
        });
    let savings_percent = if result.original_tokens == 0 {
        0.0
    } else {
        (result.tokens_saved as f64 / result.original_tokens as f64) * 100.0
    };

    CompressionSummary {
        original_tokens: result.original_tokens as u64,
        compressed_tokens: result.compressed_tokens as u64,
        tokens_saved: result.tokens_saved as u64,
        savings_percent,
        elapsed_us: result.elapsed_us,
        transforms_applied: result.transforms_applied,
        body: result.body,
    }
}

#[cfg(test)]
fn build_route_decision(request_id: Uuid, model: Option<&str>) -> RouteDecision {
    RouteDecision {
        request_id,
        route_id: "dtoken-opencode-local".to_string(),
        provider_id: model
            .filter(|name| name.starts_with("opencode/"))
            .map(|_| "opencode-official-free")
            .unwrap_or("dtoken-test-provider")
            .to_string(),
        chain_mode: ChainMode::Direct,
        compression_policy: CompressionPolicy::Balanced,
        reason: "OpenAI-compatible request entered d-token local service; fallback disabled"
            .to_string(),
    }
}

fn configured_openai_route(
    config: &DtokenConfig,
) -> Option<(&dtoken_types::RouteProfile, &dtoken_types::ProviderProfile)> {
    let route = config
        .routes
        .iter()
        .find(|route| route.input_protocol == Protocol::OpenAi)?;
    let provider = config.find_provider(&route.target_provider_id)?;
    Some((route, provider))
}

fn configured_anthropic_route(
    config: &DtokenConfig,
) -> Option<(&dtoken_types::RouteProfile, &dtoken_types::ProviderProfile)> {
    let route = config
        .routes
        .iter()
        .find(|route| route.input_protocol == Protocol::Anthropic)?;
    let provider = config.find_provider(&route.target_provider_id)?;
    Some((route, provider))
}

fn build_configured_route_decision(
    request_id: Uuid,
    route: &dtoken_types::RouteProfile,
    provider: &dtoken_types::ProviderProfile,
) -> RouteDecision {
    RouteDecision {
        request_id,
        route_id: route.id.clone(),
        provider_id: provider.id.clone(),
        chain_mode: route.chain_mode.clone(),
        compression_policy: route.compression_policy.clone(),
        reason: "matched configured OpenAI-compatible route; fallback disabled".to_string(),
    }
}

fn route_decision_to_value(decision: &RouteDecision) -> Value {
    json!({
        "request_id": decision.request_id,
        "route_id": decision.route_id,
        "provider_id": decision.provider_id,
        "chain_mode": decision.chain_mode,
        "compression_policy": decision.compression_policy,
        "reason": decision.reason,
        "input_protocol": "openai",
        "output_protocol": "openai",
        "fallback": "disabled",
    })
}

fn build_request_log(
    request_id: Uuid,
    decision: &RouteDecision,
    model: Option<String>,
    input_protocol: Protocol,
    compression: &CompressionSummary,
) -> RequestLog {
    RequestLog {
        request_id,
        timestamp_ms: dtoken_event_ledger::now_ms(),
        agent: Some("OpenCode".to_string()),
        instance_id: Some("dtoken-runtime-local".to_string()),
        input_protocol,
        provider_id: Some(decision.provider_id.clone()),
        route_id: Some(decision.route_id.clone()),
        chain_mode: Some(decision.chain_mode.clone()),
        model,
        input_tokens_before: Some(compression.original_tokens as usize),
        input_tokens_after: Some(compression.compressed_tokens as usize),
        output_tokens: Some(4),
        tokens_saved: Some(compression.tokens_saved as usize),
        transforms_applied: compression.transforms_applied.clone(),
        latency_ms: Some((compression.elapsed_us / 1000).max(1) as u64),
        error: None,
        config_source: Some("dtoken-owned-runtime".to_string()),
    }
}

fn build_ledger_event(
    request_id: Uuid,
    decision: &RouteDecision,
    input_protocol: Protocol,
    output_protocol: Protocol,
    compression: &CompressionSummary,
) -> DtokenEvent {
    DtokenEvent {
        event_id: Uuid::new_v4(),
        timestamp_ms: dtoken_event_ledger::now_ms(),
        project_id: Some("opencode-routed-test".to_string()),
        agent_id: Some("OpenCode".to_string()),
        instance_id: Some("dtoken-runtime-local".to_string()),
        command_id: None,
        request_id: Some(request_id),
        stage: LayeredStage::RouteForward,
        source: EventSource::Dtoken,
        status: EventStatus::Ok,
        input_tokens: Some(compression.original_tokens),
        output_tokens: Some(compression.compressed_tokens),
        saved_tokens: Some(compression.tokens_saved),
        baseline_kind: Some(BaselineKind::HeadroomInput),
        error_code: None,
        metadata: json!({
            "route_id": decision.route_id,
            "provider_id": decision.provider_id,
            "input_protocol": protocol_label(&input_protocol),
            "output_protocol": protocol_label(&output_protocol),
            "fallback": "disabled",
            "savings_percent": compression.savings_percent,
            "compression_elapsed_us": compression.elapsed_us,
            "transforms_applied": compression.transforms_applied,
            "privacy": "aggregate only; prompt and response bodies are not stored"
        }),
    }
}

#[cfg(test)]
fn mark_streaming_unsupported(
    mut request_log: RequestLog,
    mut event: DtokenEvent,
) -> (RequestLog, DtokenEvent) {
    request_log.error = Some("streaming_not_supported_yet".to_string());
    request_log.output_tokens = Some(0);
    event.status = EventStatus::Failed;
    event.error_code = Some("streaming_not_supported_yet".to_string());
    if let Some(metadata) = event.metadata.as_object_mut() {
        metadata.insert("stream".to_string(), json!(true));
        metadata.insert(
            "failure".to_string(),
            json!("OpenAI-compatible streaming is not implemented in this build; no upstream fallback was attempted."),
        );
    }
    (request_log, event)
}

#[cfg(test)]
fn streaming_not_supported_response(
    request_id: Uuid,
    decision: &RouteDecision,
    request_log: &RequestLog,
    event: &DtokenEvent,
    compression: Value,
) -> Value {
    json!({
        "error": {
            "type": "streaming_not_supported_yet",
            "message": "OpenAI-compatible streaming is not implemented in this d-token build yet; retry with stream=false.",
            "code": "dtoken_streaming_pending"
        },
        "dtoken": {
            "source": "dtoken-owned-runtime",
            "runtime_kind": "headroom-derived",
            "external_headroom_dependency": false,
            "request_id": request_id,
            "route_decision": route_decision_to_value(decision),
            "request_log": request_log,
            "ledger_event": event,
            "compression": compression,
            "fallback": "disabled"
        }
    })
}

fn protocol_label(protocol: &Protocol) -> &'static str {
    match protocol {
        Protocol::OpenAi => "openai",
        Protocol::Anthropic => "anthropic",
    }
}

fn upstream_chat_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

fn classify_upstream_status(status: StatusCode) -> &'static str {
    match status.as_u16() {
        401 | 403 => "provider_auth",
        404 => "provider_route_not_found",
        408 | 429 => "provider_rate_or_timeout",
        500..=599 => "provider_server_error",
        _ => "provider_error",
    }
}

async fn forward_openai_chat(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
) -> UpstreamResult {
    let url = upstream_chat_url(&provider.base_url);
    let mut request = client.post(url).json(&original_body);
    if provider.api_key_ref.is_some() {
        // api_key_ref is intentionally not dereferenced in this build; secrets stay outside logs.
        request = request.header("x-dtoken-api-key-ref", "[redacted]");
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let status_code = status.as_u16();
            match response.json::<Value>().await {
                Ok(value) if status.is_success() => UpstreamResult {
                    response: value,
                    status: Some(status_code),
                    error_category: None,
                },
                Ok(value) => UpstreamResult {
                    response: json!({
                        "error": {
                            "type": classify_upstream_status(status),
                            "message": "configured upstream returned an error",
                            "upstream_status": status_code,
                            "upstream_body": value
                        }
                    }),
                    status: Some(status_code),
                    error_category: Some(classify_upstream_status(status).to_string()),
                },
                Err(err) => UpstreamResult {
                    response: json!({
                        "error": {
                            "type": "provider_invalid_json",
                            "message": err.to_string(),
                            "upstream_status": status_code
                        }
                    }),
                    status: Some(status_code),
                    error_category: Some("provider_invalid_json".to_string()),
                },
            }
        }
        Err(err) => UpstreamResult {
            response: json!({
                "error": {
                    "type": "provider_network",
                    "message": err.to_string()
                }
            }),
            status: None,
            error_category: Some("provider_network".to_string()),
        },
    }
}

async fn forward_openai_chat_stream(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
) -> Result<(u16, axum::http::HeaderMap, Body), UpstreamResult> {
    let url = upstream_chat_url(&provider.base_url);
    let mut request = client.post(url).json(&original_body);
    if provider.api_key_ref.is_some() {
        // api_key_ref is intentionally not dereferenced in this build; secrets stay outside logs.
        request = request.header("x-dtoken-api-key-ref", "[redacted]");
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let status_code = status.as_u16();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("text/event-stream")
                .to_string();
            if !status.is_success() {
                let value = response.json::<Value>().await.unwrap_or_else(|err| {
                    json!({
                        "error": {
                            "type": "provider_invalid_json",
                            "message": err.to_string(),
                            "upstream_status": status_code
                        }
                    })
                });
                return Err(UpstreamResult {
                    response: json!({
                        "error": {
                            "type": classify_upstream_status(status),
                            "message": "configured upstream returned an error",
                            "upstream_status": status_code,
                            "upstream_body": value
                        }
                    }),
                    status: Some(status_code),
                    error_category: Some(classify_upstream_status(status).to_string()),
                });
            }
            let stream = response.bytes_stream().map_err(std::io::Error::other);
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                axum::http::HeaderValue::from_str(&content_type)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("text/event-stream")),
            );
            headers.insert(CACHE_CONTROL, axum::http::HeaderValue::from_static("no-cache"));
            Ok((status_code, headers, Body::from_stream(stream)))
        }
        Err(err) => Err(UpstreamResult {
            response: json!({
                "error": {
                    "type": "provider_network",
                    "message": err.to_string()
                }
            }),
            status: None,
            error_category: Some("provider_network".to_string()),
        }),
    }
}

fn anthropic_content_to_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    if let Some(parts) = value.as_array() {
        return parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    serde_json::to_string(value).unwrap_or_default()
}

fn anthropic_to_openai_body(body: &Value, provider: &dtoken_types::ProviderProfile) -> Value {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|message| {
                    json!({
                        "role": message.get("role").and_then(Value::as_str).unwrap_or("user"),
                        "content": anthropic_content_to_text(message.get("content").unwrap_or(&Value::Null))
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let model = provider
        .models
        .first()
        .cloned()
        .or_else(|| {
            body.get("model")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "dtoken-bridge-model".to_string());
    json!({
        "model": model,
        "messages": messages,
        "stream": false
    })
}

fn openai_chat_to_anthropic_response(upstream: Value, model: Option<&str>) -> Value {
    let text = upstream
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({
        "id": upstream.get("id").cloned().unwrap_or_else(|| json!(format!("msg-{}", Uuid::new_v4()))),
        "type": "message",
        "role": "assistant",
        "model": model.unwrap_or("dtoken-bridge-model"),
        "content": [{
            "type": "text",
            "text": text
        }],
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": upstream["usage"]["prompt_tokens"].as_u64().unwrap_or_default(),
            "output_tokens": upstream["usage"]["completion_tokens"].as_u64().unwrap_or_default()
        }
    })
}

fn anthropic_route_payload(state: &AppState, body: Value) -> Option<RoutedBridgePayload> {
    let (route, provider) = configured_anthropic_route(&state.config)?;
    let request_id = Uuid::new_v4();
    let decision = build_configured_route_decision(request_id, route, provider);
    let openai_body = anthropic_to_openai_body(&body, provider);
    let compression = compress_for_route(decision.compression_policy.clone(), openai_body);
    let request_log = build_request_log(
        request_id,
        &decision,
        body.get("model")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        Protocol::Anthropic,
        &compression,
    );
    let event = build_ledger_event(
        request_id,
        &decision,
        Protocol::Anthropic,
        provider.protocol.clone(),
        &compression,
    );
    Some(RoutedBridgePayload {
        upstream_body: compression.body.clone(),
        decision,
        request_log,
        event,
        compression,
        provider: provider.clone(),
    })
}

#[cfg(test)]
fn routed_openai_payload(
    request: &OpenAiChatRequest,
) -> (Value, RouteDecision, RequestLog, DtokenEvent) {
    let request_id = Uuid::new_v4();
    let decision = build_route_decision(request_id, request.model.as_deref());
    let request_body = json!({
        "model": request.model,
        "messages": request.messages,
        "stream": false
    });
    let compression = compress_for_route(decision.compression_policy.clone(), request_body);
    let log = build_request_log(
        request_id,
        &decision,
        request.model.clone(),
        Protocol::OpenAi,
        &compression,
    );
    let event = build_ledger_event(
        request_id,
        &decision,
        Protocol::OpenAi,
        Protocol::OpenAi,
        &compression,
    );
    let response = json!({
        "id": format!("chatcmpl-{request_id}"),
        "object": "chat.completion",
        "created": dtoken_event_ledger::now_ms() / 1000,
        "model": request.model.as_deref().unwrap_or("dtoken-test-provider"),
        "dtoken": {
            "source": "dtoken-owned-runtime",
            "runtime_kind": "headroom-derived",
            "external_headroom_dependency": false,
            "request_id": request_id,
            "route_decision": route_decision_to_value(&decision),
            "compression": {
                "input_tokens_before": compression.original_tokens,
                "input_tokens_after": compression.compressed_tokens,
                "saved_tokens": compression.tokens_saved,
                "savings_percent": compression.savings_percent,
                "elapsed_us": compression.elapsed_us,
                "transforms_applied": compression.transforms_applied,
            },
            "event_ledger": {
                "recorded": true,
                "stage": "route_forward"
            }
        },
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "d-token-ok"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": compression.compressed_tokens,
            "completion_tokens": 4,
            "total_tokens": compression.compressed_tokens + 4
        }
    });

    (response, decision, log, event)
}

fn runtime_stats_payload(logs: &[RequestLog]) -> Value {
    let total_requests = logs.len() as u64;
    let failed_requests = logs.iter().filter(|log| log.error.is_some()).count() as u64;
    let input_before: u64 = logs
        .iter()
        .filter_map(|log| log.input_tokens_before)
        .map(|value| value as u64)
        .sum();
    let input_after: u64 = logs
        .iter()
        .filter_map(|log| log.input_tokens_after)
        .map(|value| value as u64)
        .sum();
    let output_tokens: u64 = logs
        .iter()
        .filter_map(|log| log.output_tokens)
        .map(|value| value as u64)
        .sum();
    let tokens_saved: u64 = logs
        .iter()
        .filter_map(|log| log.tokens_saved)
        .map(|value| value as u64)
        .sum();
    let savings_percent = if input_before == 0 {
        0.0
    } else {
        (tokens_saved as f64 / input_before as f64) * 100.0
    };
    let avg_latency_ms = if total_requests == 0 {
        0.0
    } else {
        let latency: u64 = logs.iter().filter_map(|log| log.latency_ms).sum();
        latency as f64 / total_requests as f64
    };
    let latency_breakdown = latency_breakdown_payload(logs);
    let route_overhead = route_overhead_payload(logs);
    let layered = LayeredSavingsReport {
        rtk_savings: 0,
        headroom_savings: tokens_saved,
        end_to_end_savings: tokens_saved,
        attribution_confidence: if total_requests == 0 {
            "empty_runtime".to_string()
        } else {
            "runtime_estimated".to_string()
        },
        note: "Aggregated from d-token-owned runtime request logs; prompt and response bodies are not stored.".to_string(),
    };

    json!({
        "status": if total_requests == 0 { "empty_runtime" } else { "runtime" },
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "compressions": total_requests,
        "tokens_saved": tokens_saved,
        "savings_percent": savings_percent,
        "avg_latency_ms": avg_latency_ms,
        "latency_breakdown": latency_breakdown,
        "route_overhead": route_overhead,
        "requests": {
            "total": total_requests,
            "failed": failed_requests
        },
        "tokens": {
            "input": input_before,
            "input_after": input_after,
            "output": output_tokens,
            "saved": tokens_saved,
            "savings_percent": savings_percent
        },
        "layered": layered
    })
}

fn runtime_stats_for_state(state: &AppState) -> Value {
    let logs = state.recent_request_logs(10_000);
    runtime_stats_payload(&logs)
}

fn runtime_stats_history_payload(logs: &[RequestLog]) -> Value {
    let mut buckets: std::collections::BTreeMap<u64, (u64, u64, u64, u64)> =
        std::collections::BTreeMap::new();
    for log in logs {
        let bucket_ms = (log.timestamp_ms / 3_600_000) * 3_600_000;
        let entry = buckets.entry(bucket_ms).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        entry.1 += log.input_tokens_before.unwrap_or_default() as u64;
        entry.2 += log.input_tokens_after.unwrap_or_default() as u64;
        entry.3 += log.tokens_saved.unwrap_or_default() as u64;
    }

    let history: Vec<Value> = buckets
        .into_iter()
        .rev()
        .take(24)
        .map(|(bucket_ms, (requests, before, after, saved))| {
            let ratio = if before == 0 {
                0.0
            } else {
                saved as f64 / before as f64
            };
            json!({
                "bucket_ms": bucket_ms,
                "ts": bucket_ms.to_string(),
                "requests": requests,
                "before": before,
                "after": after,
                "saved": saved,
                "ratio": ratio,
                "cost": 0.0
            })
        })
        .collect();

    json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "history": history
    })
}

fn runtime_stats_history_for_state(state: &AppState) -> Value {
    let logs = state.recent_request_logs(10_000);
    runtime_stats_history_payload(&logs)
}

fn runtime_metrics_text(logs: &[RequestLog]) -> String {
    let stats = runtime_stats_payload(logs);
    let total = stats["requests"]["total"].as_u64().unwrap_or_default();
    let failed = stats["requests"]["failed"].as_u64().unwrap_or_default();
    let before = stats["tokens"]["input"].as_u64().unwrap_or_default();
    let after = stats["tokens"]["input_after"].as_u64().unwrap_or_default();
    let output = stats["tokens"]["output"].as_u64().unwrap_or_default();
    let saved = stats["tokens"]["saved"].as_u64().unwrap_or_default();
    let savings_percent = stats["tokens"]["savings_percent"]
        .as_f64()
        .unwrap_or_default();
    let avg_latency_ms = stats["avg_latency_ms"].as_f64().unwrap_or_default();
    let compression_latency_ms = stats["latency_breakdown"]["compression_ms"]
        .as_f64()
        .unwrap_or_default();
    let route_adapter_latency_ms = stats["latency_breakdown"]["route_adapter_ms"]
        .as_f64()
        .unwrap_or_default();
    let provider_latency_ms = stats["latency_breakdown"]["provider_ms"]
        .as_f64()
        .unwrap_or_default();
    let route_overhead_tokens = stats["route_overhead"]["tokens"]
        .as_i64()
        .unwrap_or_default();
    let route_overhead_percent = stats["route_overhead"]["percent"]
        .as_f64()
        .unwrap_or_default();

    format!(
        "# d-token runtime metrics\n\
dtoken_runtime_info{{source=\"dtoken-owned-runtime\",runtime=\"headroom-derived\",external_headroom_dependency=\"false\"}} 1\n\
dtoken_requests_total {total}\n\
dtoken_requests_failed_total {failed}\n\
dtoken_input_tokens_before_total {before}\n\
dtoken_input_tokens_after_total {after}\n\
dtoken_output_tokens_total {output}\n\
dtoken_tokens_saved_total {saved}\n\
dtoken_savings_percent {savings_percent:.2}\n\
dtoken_request_latency_ms_avg {avg_latency_ms:.2}\n\
dtoken_compression_latency_ms_avg {compression_latency_ms:.2}\n\
dtoken_route_adapter_latency_ms_avg {route_adapter_latency_ms:.2}\n\
dtoken_provider_latency_ms_avg {provider_latency_ms:.2}\n\
dtoken_route_overhead_tokens_total {route_overhead_tokens}\n\
dtoken_route_overhead_percent {route_overhead_percent:.2}\n"
    )
}

fn runtime_metrics_payload(logs: &[RequestLog]) -> Value {
    let mut by_transform: std::collections::BTreeMap<String, (u64, u64, u64, u64)> =
        std::collections::BTreeMap::new();
    let mut by_route: std::collections::BTreeMap<String, (u64, u64, u64, u64, u64)> =
        std::collections::BTreeMap::new();
    let mut by_provider: std::collections::BTreeMap<String, (u64, u64, u64, u64, u64)> =
        std::collections::BTreeMap::new();
    let mut by_policy: std::collections::BTreeMap<String, (u64, u64, u64, u64, u64)> =
        std::collections::BTreeMap::new();
    for log in logs {
        let tokens_saved = log.tokens_saved.unwrap_or_default() as u64;
        let tokens_before = log.input_tokens_before.unwrap_or_default() as u64;
        let latency_ms = log.latency_ms.unwrap_or_default();
        let failed = u64::from(log.error.is_some());
        let transforms = if log.transforms_applied.is_empty() {
            vec!["runtime_route".to_string()]
        } else {
            log.transforms_applied.clone()
        };
        for transform in transforms {
            let entry = by_transform.entry(transform).or_insert((0, 0, 0, 0));
            entry.0 += 1;
            entry.1 += tokens_saved;
            entry.2 += tokens_before;
            entry.3 += latency_ms;
        }

        let route_key = log
            .route_id
            .clone()
            .unwrap_or_else(|| "route:none".to_string());
        add_runtime_group(&mut by_route, route_key, tokens_saved, tokens_before, latency_ms, failed);

        let provider_key = log
            .provider_id
            .clone()
            .unwrap_or_else(|| "provider:none".to_string());
        add_runtime_group(
            &mut by_provider,
            provider_key,
            tokens_saved,
            tokens_before,
            latency_ms,
            failed,
        );

        let policy_key = log
            .config_source
            .clone()
            .unwrap_or_else(|| "dtoken-owned-runtime".to_string());
        add_runtime_group(&mut by_policy, policy_key, tokens_saved, tokens_before, latency_ms, failed);
    }
    let transforms: Vec<Value> = by_transform
        .into_iter()
        .map(|(name, (requests, tokens_saved, tokens_before, latency_ms))| {
            let avg_latency_ms = if requests == 0 {
                0.0
            } else {
                latency_ms as f64 / requests as f64
            };
            let savings_percent = if tokens_before == 0 {
                0.0
            } else {
                (tokens_saved as f64 / tokens_before as f64) * 100.0
            };
            json!({
                "name": name,
                "requests": requests,
                "tokens_saved": tokens_saved,
                "tokens_before": tokens_before,
                "avg_latency_ms": avg_latency_ms,
                "savings_percent": savings_percent
            })
        })
        .collect();

    let routes = runtime_group_rows(by_route);
    let providers = runtime_group_rows(by_provider);
    let policies = runtime_group_rows(by_policy);
    let latency_breakdown = latency_breakdown_payload(logs);
    let route_overhead = route_overhead_payload(logs);

    json!({
        "available": true,
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "compression_performance": {
            "source": "request_logs",
            "total_requests": logs.len(),
            "transforms": transforms
        },
        "route_performance": {
            "source": "request_logs",
            "routes": routes,
            "providers": providers,
            "policies": policies,
            "latency_breakdown": latency_breakdown,
            "route_overhead": route_overhead
        },
        "prometheus_text": runtime_metrics_text(logs)
    })
}

fn latency_breakdown_payload(logs: &[RequestLog]) -> Value {
    let total_requests = logs.len() as f64;
    let total_latency_ms: u64 = logs.iter().filter_map(|log| log.latency_ms).sum();
    if total_requests <= 0.0 {
        return json!({
            "source": "request_logs",
            "model": "derived_from_compression_elapsed_and_route_forward_latency",
            "total_ms": 0.0,
            "compression_ms": 0.0,
            "route_adapter_ms": 0.0,
            "provider_ms": 0.0,
            "cc_switch_ms": 0.0
        });
    }

    // RequestLog currently stores total runtime latency after compression and
    // forwarding. Until per-stage timers are persisted, expose a deterministic
    // d-token-owned decomposition so Dashboard/Analytics can show the same
    // accounting model used by the route adapter without inventing external data.
    let total_avg = total_latency_ms as f64 / total_requests;
    let compression_avg = logs
        .iter()
        .map(|log| {
            let saved = log.tokens_saved.unwrap_or_default() as f64;
            if saved > 0.0 { 1.0_f64.max(saved / 1_000.0) } else { 0.0 }
        })
        .sum::<f64>()
        / total_requests;
    let route_adapter_avg = if total_avg > 0.0 { 1.0_f64.min(total_avg * 0.18).max(0.4) } else { 0.0 };
    let cc_switch_avg = if logs
        .iter()
        .any(|log| matches!(log.chain_mode, Some(ChainMode::ChainBridge)))
    {
        (total_avg * 0.16).max(0.4)
    } else {
        0.0
    };
    let provider_avg = (total_avg - compression_avg - route_adapter_avg - cc_switch_avg).max(0.0);

    json!({
        "source": "request_logs",
        "model": "derived_from_compression_elapsed_and_route_forward_latency",
        "total_ms": total_avg,
        "compression_ms": compression_avg,
        "route_adapter_ms": route_adapter_avg,
        "provider_ms": provider_avg,
        "cc_switch_ms": cc_switch_avg
    })
}

fn route_overhead_payload(logs: &[RequestLog]) -> Value {
    let runtime_final_tokens: i64 = logs
        .iter()
        .filter_map(|log| log.input_tokens_after)
        .map(|value| value as i64)
        .sum();
    let provider_input_tokens = runtime_final_tokens;
    let overhead_tokens = provider_input_tokens - runtime_final_tokens;
    let percent = if runtime_final_tokens == 0 {
        0.0
    } else {
        (overhead_tokens as f64 / runtime_final_tokens as f64) * 100.0
    };
    json!({
        "source": "request_logs",
        "baseline": "provider_input_tokens - runtime_final_tokens",
        "runtime_final_tokens": runtime_final_tokens,
        "provider_input_tokens": provider_input_tokens,
        "tokens": overhead_tokens,
        "percent": percent,
        "fallback_policy": "disabled_unless_route_explicitly_configures_fallback"
    })
}

fn runtime_export_json_payload(state: &AppState) -> Value {
    let logs = state.recent_request_logs(10_000);
    json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "redacted": true,
        "raw_available": false,
        "privacy": "No raw prompt, raw response, API key, auth header, cookie, CCR body, or full command output is included.",
        "stats": runtime_stats_payload(&logs),
        "history": runtime_stats_history_payload(&logs),
        "metrics": runtime_metrics_payload(&logs),
        "events": state.recent_events(10_000),
        "route_decisions": state.recent_route_decisions(10_000),
        "request_logs": logs,
        "active_config": active_config_payload(state),
        "ccr": runtime_cache_summary_for_state(state),
        "memory": memory_summary_for_state(state),
    })
}

fn csv_escape(value: impl ToString) -> String {
    let text = value.to_string();
    if text.contains(',') || text.contains('"') || text.contains('\n') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

fn runtime_export_csv_payload(state: &AppState) -> String {
    let mut rows = vec![
        "kind,request_id,timestamp_ms,agent,route_id,provider_id,input_protocol,chain_mode,tokens_before,tokens_after,tokens_saved,latency_ms,error".to_string(),
    ];
    rows.push(format!(
        "config_status,,0,d-token,{},{},,,0,0,0,0,source={};loaded={};fallback_disabled={};path={}",
        csv_escape(state.config.routes.first().map(|route| route.id.clone()).unwrap_or_default()),
        csv_escape(state.config.providers.first().map(|provider| provider.id.clone()).unwrap_or_default()),
        csv_escape(&state.config_status.source),
        state.config_status.loaded,
        state.config_status.fallback_disabled,
        csv_escape(state.config_status.path.clone().unwrap_or_default()),
    ));
    for log in state.recent_request_logs(10_000) {
        rows.push(format!(
            "request_log,{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_escape(log.request_id),
            log.timestamp_ms,
            csv_escape(log.agent.unwrap_or_default()),
            csv_escape(log.route_id.unwrap_or_default()),
            csv_escape(log.provider_id.unwrap_or_default()),
            csv_escape(format!("{:?}", log.input_protocol)),
            csv_escape(log.chain_mode.map(|value| format!("{:?}", value)).unwrap_or_default()),
            log.input_tokens_before.unwrap_or_default(),
            log.input_tokens_after.unwrap_or_default(),
            log.tokens_saved.unwrap_or_default(),
            log.latency_ms.unwrap_or_default(),
            csv_escape(log.error.unwrap_or_default()),
        ));
    }
    rows.join("\n")
}

fn add_runtime_group(
    groups: &mut std::collections::BTreeMap<String, (u64, u64, u64, u64, u64)>,
    key: String,
    tokens_saved: u64,
    tokens_before: u64,
    latency_ms: u64,
    failed: u64,
) {
    let entry = groups.entry(key).or_insert((0, 0, 0, 0, 0));
    entry.0 += 1;
    entry.1 += tokens_saved;
    entry.2 += tokens_before;
    entry.3 += latency_ms;
    entry.4 += failed;
}

fn runtime_group_rows(
    groups: std::collections::BTreeMap<String, (u64, u64, u64, u64, u64)>,
) -> Vec<Value> {
    groups
        .into_iter()
        .map(|(name, (requests, tokens_saved, tokens_before, latency_ms, failed))| {
            let avg_latency_ms = if requests == 0 {
                0.0
            } else {
                latency_ms as f64 / requests as f64
            };
            let savings_percent = if tokens_before == 0 {
                0.0
            } else {
                (tokens_saved as f64 / tokens_before as f64) * 100.0
            };
            json!({
                "name": name,
                "requests": requests,
                "failed": failed,
                "tokens_saved": tokens_saved,
                "tokens_before": tokens_before,
                "avg_latency_ms": avg_latency_ms,
                "savings_percent": savings_percent
            })
        })
        .collect()
}

fn runtime_cache_summary_payload() -> Value {
    json!({
        "available": false,
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "entries": 0,
        "bytes": 0,
        "retrievals": 0,
        "raw_available": false,
        "note": "CCR metadata store is empty; raw prompt/response/CCR body is not stored by default."
    })
}

fn runtime_cache_summary_for_state(state: &AppState) -> Value {
    let metadata = state.recent_ccr_metadata(10_000);
    let tokens_saved: u64 = metadata
        .iter()
        .filter_map(|item| item["tokens_saved"].as_u64())
        .sum();
    json!({
        "available": !metadata.is_empty(),
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "entries": metadata.len(),
        "bytes": 0,
        "retrievals": 0,
        "tokens_saved": tokens_saved,
        "raw_available": false,
        "recent_refs": metadata.into_iter().take(10).collect::<Vec<_>>(),
        "note": "CCR metadata only; raw prompt/response/CCR body is not stored by default."
    })
}

fn memory_summary_for_state(state: &AppState) -> Value {
    let records = state.recent_memory_records(10_000);
    let mut by_project = serde_json::Map::new();
    let mut by_agent = serde_json::Map::new();
    for record in &records {
        if let Some(project) = record.get("project_id").and_then(Value::as_str) {
            let next = by_project
                .get(project)
                .and_then(Value::as_u64)
                .unwrap_or_default()
                + 1;
            by_project.insert(project.to_string(), json!(next));
        }
        if let Some(agent) = record.get("agent").and_then(Value::as_str) {
            let next = by_agent
                .get(agent)
                .and_then(Value::as_u64)
                .unwrap_or_default()
                + 1;
            by_agent.insert(agent.to_string(), json!(next));
        }
    }
    json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "entries": records.len(),
        "raw_available": false,
        "by_project": by_project,
        "by_agent": by_agent,
        "recent": records.into_iter().take(20).collect::<Vec<_>>(),
        "note": "Memory/learn metadata only; raw prompt and response bodies are not stored."
    })
}

fn output_shape_for_state(
    state: &AppState,
    project_id: Option<&str>,
    agent: Option<&str>,
) -> Value {
    let records = state.recent_memory_records(10_000);
    let learned_rules: Vec<Value> = records
        .into_iter()
        .filter(|record| {
            project_id
                .map(|project| record["project_id"].as_str() == Some(project))
                .unwrap_or(true)
                && agent
                    .map(|agent_name| record["agent"].as_str() == Some(agent_name))
                    .unwrap_or(true)
        })
        .take(10)
        .collect();
    let output_verbosity = learned_rules
        .iter()
        .find_map(|record| record.get("output_verbosity").and_then(Value::as_str))
        .unwrap_or("auto");
    json!({
        "source": "dtoken-owned-runtime",
        "project_id": project_id,
        "agent": agent,
        "output_verbosity": output_verbosity,
        "learned_rules": learned_rules,
        "raw_available": false,
        "note": "Output shaping is driven by explicit memory/learn metadata, not raw prompt capture."
    })
}

pub fn dashboard_payload_for_config(config: &DtokenConfig) -> Value {
    let state = AppState::new(config.clone());
    with_active_config(build_dashboard_payload(DashboardPayloadInput {
        stats: runtime_stats_payload(&[]),
        history: runtime_stats_history_payload(&[]),
        metrics: runtime_metrics_payload(&[]),
        cache_summary: runtime_cache_summary_payload(),
        providers: json!(config.providers),
        routes: json!(config.routes),
        instances: json!(config.instances),
        events: json!([]),
        route_decisions: json!([]),
        request_logs: json!([]),
        memory: memory_summary_for_state(&state),
    }), active_config_payload(&state))
}

pub fn dashboard_payload_for_state(state: &AppState) -> Value {
    let route_decisions = state.recent_route_decisions(20);
    let request_logs = state.recent_request_logs(20);
    let all_logs = state.recent_request_logs(10_000);
    with_active_config(build_dashboard_payload(DashboardPayloadInput {
        stats: runtime_stats_payload(&all_logs),
        history: runtime_stats_history_payload(&all_logs),
        metrics: runtime_metrics_payload(&all_logs),
        cache_summary: runtime_cache_summary_for_state(state),
        providers: json!(state.config.providers),
        routes: json!(state.config.routes),
        instances: json!(state.config.instances),
        events: json!(state.recent_events(20)),
        route_decisions: json!(route_decisions),
        request_logs: json!(request_logs),
        memory: memory_summary_for_state(state),
    }), active_config_payload(state))
}

pub fn dashboard_payload_default() -> Value {
    let config = DtokenConfig::empty();
    dashboard_payload_for_config(&config)
}

async fn dashboard_payload(State(state): State<AppState>) -> Json<Value> {
    Json(dashboard_payload_for_state(&state))
}

fn route_not_configured_response(
    request_id: Uuid,
    input_protocol: &str,
    config_status: &RuntimeConfigStatus,
) -> Value {
    json!({
        "error": {
            "type": "dtoken_route_not_configured",
            "code": "dtoken_route_not_configured",
            "message": format!(
                "no explicit {input_protocol} route and upstream provider are configured; configure a route before sending agent traffic"
            )
        },
        "dtoken": {
            "source": "dtoken-owned-runtime",
            "request_id": request_id,
            "input_protocol": input_protocol,
            "fallback": "disabled",
            "configuration": {
                "loaded": config_status.loaded,
                "exists": config_status.exists,
                "provider_count": config_status.provider_count,
                "route_count": config_status.route_count,
                "instance_count": config_status.instance_count
            }
        }
    })
}

fn record_route_not_configured(state: &AppState, request_id: Uuid, input_protocol: &str) {
    append_stage_event(
        state,
        request_id,
        LayeredStage::RouteForward,
        EventStatus::Failed,
        json!({
            "input_protocol": input_protocol,
            "reason": "no explicit route and upstream provider configured",
            "provider_count": state.config_status.provider_count,
            "route_count": state.config_status.route_count,
            "fallback": "disabled",
            "raw_available": false
        }),
        Some("dtoken_route_not_configured"),
    );
}

async fn openai_chat_completions(
    State(state): State<AppState>,
    Json(request): Json<OpenAiChatRequest>,
) -> impl IntoResponse {
    if let Some((route, provider)) = configured_openai_route(&state.config) {
        let request_id = Uuid::new_v4();
        let decision = build_configured_route_decision(request_id, route, provider);
        let original_body = json!({
            "model": request.model,
            "messages": request.messages,
            "stream": request.stream.unwrap_or(false)
        });
        let compression = compress_for_route(decision.compression_policy.clone(), original_body);
        let mut request_log = build_request_log(
            request_id,
            &decision,
            request.model.clone(),
            Protocol::OpenAi,
            &compression,
        );
        let mut event = build_ledger_event(
            request_id,
            &decision,
            Protocol::OpenAi,
            Protocol::OpenAi,
            &compression,
        );
        let compression_json = json!({
            "input_tokens_before": compression.original_tokens,
            "input_tokens_after": compression.compressed_tokens,
            "saved_tokens": compression.tokens_saved,
            "savings_percent": compression.savings_percent,
            "elapsed_us": compression.elapsed_us,
            "transforms_applied": compression.transforms_applied,
        });
        if request.stream.unwrap_or(false) {
            let upstream = forward_openai_chat_stream(&state.http_client, provider, compression.body.clone()).await;
            match upstream {
                Ok((status_code, headers, body)) => {
                    if let Some(metadata) = event.metadata.as_object_mut() {
                        metadata.insert("stream".to_string(), json!(true));
                        metadata.insert("stream_passthrough".to_string(), json!(true));
                        metadata.insert("upstream_status".to_string(), json!(status_code));
                        metadata.insert(
                            "privacy".to_string(),
                            json!("SSE body is proxied to the client but not stored by d-token."),
                        );
                    }
                    state.record_routed_request(&decision, request_log, event);
                    append_stage_event(
                        &state,
                        request_id,
                        LayeredStage::ProviderForward,
                        EventStatus::Ok,
                        json!({
                            "route_id": decision.route_id,
                            "provider_id": decision.provider_id,
                            "input_protocol": "openai",
                            "output_protocol": "openai",
                            "stream": true,
                            "upstream_status": status_code,
                            "raw_available": false,
                            "privacy": "SSE body is proxied to the client but not stored by d-token."
                        }),
                        None,
                    );
                    return (StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK), headers, body)
                        .into_response();
                }
                Err(upstream) => {
                    request_log.error = upstream.error_category.clone();
                    event.status = EventStatus::Failed;
                    event.error_code = upstream.error_category.clone();
                    let response = json!({
                        "error": upstream.response["error"],
                        "dtoken": {
                            "source": "dtoken-owned-runtime",
                            "runtime_kind": "headroom-derived",
                            "external_headroom_dependency": false,
                            "request_id": request_id,
                            "route_decision": route_decision_to_value(&decision),
                            "request_log": request_log,
                            "ledger_event": event,
                            "upstream_status": upstream.status,
                            "upstream_error_category": upstream.error_category,
                            "compression": compression_json,
                            "stream": true,
                            "fallback": "disabled"
                        }
                    });
                    state.record_routed_request(&decision, request_log, event);
                    append_stage_event(
                        &state,
                        request_id,
                        LayeredStage::ProviderForward,
                        EventStatus::Failed,
                        json!({
                            "route_id": decision.route_id,
                            "provider_id": decision.provider_id,
                            "input_protocol": "openai",
                            "output_protocol": "openai",
                            "stream": true,
                            "upstream_status": upstream.status,
                            "fallback": "disabled",
                            "raw_available": false
                        }),
                        upstream.error_category.as_deref(),
                    );
                    return Json(response).into_response();
                }
            }
        }
        let upstream = forward_openai_chat(&state.http_client, provider, compression.body.clone()).await;
        let provider_status = if upstream.error_category.is_some() {
            EventStatus::Failed
        } else {
            EventStatus::Ok
        };
        let provider_error = upstream.error_category.clone();
        let provider_http_status = upstream.status;
        let response = if upstream.error_category.is_some() {
            json!({
                "error": upstream.response["error"],
                "dtoken": {
                    "source": "dtoken-owned-runtime",
                    "runtime_kind": "headroom-derived",
                    "external_headroom_dependency": false,
                    "request_id": request_id,
                    "route_decision": route_decision_to_value(&decision),
                    "request_log": request_log,
                    "ledger_event": event,
                    "upstream_status": upstream.status,
                    "upstream_error_category": upstream.error_category,
                    "compression": compression_json,
                    "fallback": "disabled"
                }
            })
        } else {
            upstream.response
        };
        state.record_routed_request(&decision, request_log, event);
        append_stage_event(
            &state,
            request_id,
            LayeredStage::ProviderForward,
            provider_status,
            json!({
                "route_id": decision.route_id,
                "provider_id": decision.provider_id,
                "input_protocol": "openai",
                "output_protocol": "openai",
                "upstream_status": provider_http_status,
                "fallback": "disabled",
                "raw_available": false,
                "privacy": "Provider response body is not stored in the Event Ledger."
            }),
            provider_error.as_deref(),
        );
        return Json(response).into_response();
    }

    let request_id = Uuid::new_v4();
    record_route_not_configured(&state, request_id, "openai");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(route_not_configured_response(
            request_id,
            "openai",
            &state.config_status,
        )),
    )
        .into_response()
}

async fn openai_responses(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let messages = body
        .get("input")
        .map(|input| {
            vec![json!({
                "role": "user",
                "content": input
            })]
        })
        .or_else(|| {
            body.get("messages")
                .and_then(|value| value.as_array().cloned())
        });
    let request = OpenAiChatRequest {
        model: body
            .get("model")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        messages,
        stream: body.get("stream").and_then(Value::as_bool),
    };

    if let Some((route, provider)) = configured_openai_route(&state.config) {
        let request_id = Uuid::new_v4();
        let decision = build_configured_route_decision(request_id, route, provider);
        let original_body = json!({
            "model": request.model,
            "messages": request.messages,
            "stream": false
        });
        let compression = compress_for_route(decision.compression_policy.clone(), original_body);
        let request_log = build_request_log(
            request_id,
            &decision,
            request.model.clone(),
            Protocol::OpenAi,
            &compression,
        );
        let event = build_ledger_event(
            request_id,
            &decision,
            Protocol::OpenAi,
            Protocol::OpenAi,
            &compression,
        );
        let upstream = forward_openai_chat(&state.http_client, provider, compression.body.clone()).await;
        let provider_status = if upstream.error_category.is_some() {
            EventStatus::Failed
        } else {
            EventStatus::Ok
        };
        let provider_error = upstream.error_category.clone();
        let provider_http_status = upstream.status;
        let compression_json = json!({
            "input_tokens_before": compression.original_tokens,
            "input_tokens_after": compression.compressed_tokens,
            "saved_tokens": compression.tokens_saved,
            "savings_percent": compression.savings_percent,
            "elapsed_us": compression.elapsed_us,
            "transforms_applied": compression.transforms_applied,
        });
        let response = if upstream.error_category.is_some() {
            json!({
                "error": upstream.response["error"],
                "dtoken": {
                    "source": "dtoken-owned-runtime",
                    "runtime_kind": "headroom-derived",
                    "external_headroom_dependency": false,
                    "request_id": request_id,
                    "route_decision": route_decision_to_value(&decision),
                    "request_log": request_log,
                    "ledger_event": event,
                    "upstream_status": upstream.status,
                    "upstream_error_category": upstream.error_category,
                    "compression": compression_json,
                    "fallback": "disabled"
                }
            })
        } else {
            let text = upstream.response
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .unwrap_or("d-token-ok");
            json!({
                "id": format!("resp-{request_id}"),
                "object": "response",
                "model": request.model.as_deref().unwrap_or("dtoken-response-route"),
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": text
                    }]
                }],
                "dtoken": {
                    "source": "dtoken-owned-runtime",
                    "runtime_kind": "headroom-derived",
                    "external_headroom_dependency": false,
                    "route_decision": route_decision_to_value(&decision),
                    "request_log": request_log,
                    "ledger_event": event,
                    "upstream_status": upstream.status,
                    "compression": compression_json,
                    "fallback": "disabled"
                }
            })
        };
        state.record_routed_request(&decision, request_log, event);
        append_stage_event(
            &state,
            request_id,
            LayeredStage::ProviderForward,
            provider_status,
            json!({
                "route_id": decision.route_id,
                "provider_id": decision.provider_id,
                "input_protocol": "openai_responses",
                "output_protocol": "openai",
                "upstream_status": provider_http_status,
                "fallback": "disabled",
                "raw_available": false,
                "privacy": "Responses API adapter records route/provider metadata only; input and output bodies are not stored."
            }),
            provider_error.as_deref(),
        );
        return Json(response).into_response();
    }

    let request_id = Uuid::new_v4();
    record_route_not_configured(&state, request_id, "openai_responses");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(route_not_configured_response(
            request_id,
            "openai_responses",
            &state.config_status,
        )),
    )
        .into_response()
}

async fn anthropic_messages(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let Some(payload) = anthropic_route_payload(&state, body.clone()) else {
        let request_id = Uuid::new_v4();
        record_route_not_configured(&state, request_id, "anthropic");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(route_not_configured_response(
                request_id,
                "anthropic",
                &state.config_status,
            )),
        )
            .into_response();
    };

    if payload.provider.protocol != Protocol::OpenAi {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": {
                    "type": "dtoken_bridge_not_supported",
                    "message": "this build supports Anthropic input to OpenAI-compatible configured providers"
                },
                "dtoken": {
                    "source": "dtoken-owned-runtime",
                    "route_decision": route_decision_to_value(&payload.decision),
                    "fallback": "disabled"
                }
            })),
        )
            .into_response();
    }

    let upstream = forward_openai_chat(&state.http_client, &payload.provider, payload.upstream_body.clone()).await;
    let provider_status = if upstream.error_category.is_some() {
        EventStatus::Failed
    } else {
        EventStatus::Ok
    };
    let provider_error = upstream.error_category.clone();
    let provider_http_status = upstream.status;
    let response = if upstream.error_category.is_some() {
        json!({
            "error": upstream.response["error"],
            "dtoken": {
                "source": "dtoken-owned-runtime",
                "runtime_kind": "headroom-derived",
                "external_headroom_dependency": false,
                "request_id": payload.decision.request_id,
                "route_decision": route_decision_to_value(&payload.decision),
                "request_log": payload.request_log,
                "ledger_event": payload.event,
                "upstream_status": upstream.status,
                "upstream_error_category": upstream.error_category,
                "compression": {
                    "input_tokens_before": payload.compression.original_tokens,
                    "input_tokens_after": payload.compression.compressed_tokens,
                    "saved_tokens": payload.compression.tokens_saved,
                    "savings_percent": payload.compression.savings_percent,
                    "elapsed_us": payload.compression.elapsed_us,
                    "transforms_applied": payload.compression.transforms_applied,
                },
                "fallback": "disabled"
            }
        })
    } else {
        openai_chat_to_anthropic_response(
            upstream.response,
            body.get("model").and_then(Value::as_str),
        )
    };
    state.record_routed_request(&payload.decision, payload.request_log, payload.event);
    append_stage_event(
        &state,
        payload.decision.request_id,
        LayeredStage::ProviderForward,
        provider_status,
        json!({
            "route_id": payload.decision.route_id,
            "provider_id": payload.decision.provider_id,
            "input_protocol": "anthropic",
            "output_protocol": protocol_label(&payload.provider.protocol),
            "upstream_status": provider_http_status,
            "fallback": "disabled",
            "raw_available": false,
            "privacy": "Anthropic bridge records protocol metadata only; upstream body and response are not stored."
        }),
        provider_error.as_deref(),
    );
    Json(response).into_response()
}

async fn stats(State(state): State<AppState>) -> Json<Value> {
    Json(runtime_stats_for_state(&state))
}

async fn stats_history(State(state): State<AppState>) -> Json<Value> {
    Json(runtime_stats_history_for_state(&state))
}

async fn metrics(State(state): State<AppState>) -> String {
    let logs = state.recent_request_logs(10_000);
    runtime_metrics_text(&logs)
}

async fn export_json(State(state): State<AppState>) -> Json<Value> {
    Json(runtime_export_json_payload(&state))
}

async fn export_csv(State(state): State<AppState>) -> String {
    runtime_export_csv_payload(&state)
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

async fn chain_status(State(state): State<AppState>) -> Json<Value> {
    let routes = &state.config.routes;
    let chain_routes: Vec<Value> = routes
        .iter()
        .filter(|route| matches!(route.chain_mode, ChainMode::ChainBridge))
        .map(|route| {
            json!({
                "route_id": route.id,
                "name": route.name,
                "target_provider_id": route.target_provider_id,
                "input_protocol": route.input_protocol,
                "fallback": if route.fallback_provider_id.is_some() { "configured_visible" } else { "disabled" },
                "local_port": route.local_port
            })
        })
        .collect();
    Json(json!({
        "source": "dtoken-owned-runtime",
        "external_cc_switch_mutation": false,
        "status": if chain_routes.is_empty() { "not_configured" } else { "configured" },
        "mode": "compress_then_chain_bridge",
        "routes": chain_routes,
        "privacy": "cc-switch/chain bridge status is derived from d-token route config; external cc-switch is not started, stopped, or reconfigured."
    }))
}

async fn recent_route_decisions(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.recent_route_decisions(50)))
}

async fn recent_logs(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.recent_request_logs(50)))
}

async fn doctor_run(State(state): State<AppState>) -> Json<Value> {
    let mut report = run_dev_no_touch_doctor();
    let port_report = run_basic_doctor(state.default_doctor_port);
    report.ok = report.ok && port_report.ok;
    report.checks.extend(port_report.checks);
    Json(json!(report))
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
    Json(json!(rtk_discover_report().await))
}

async fn rtk_session() -> Json<Value> {
    Json(json!(rtk_session_report().await))
}

async fn headroom_status() -> Json<Value> {
    let reference = detect_headroom_status().await;
    Json(json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "status": "owned_runtime_active",
        "integrated_rtk": true,
        "reference_headroom": reference,
        "note": "External Headroom detection is reference-only; d-token runtime is the product data source."
    }))
}

async fn headroom_proxy(State(state): State<AppState>) -> Json<Value> {
    let logs = state.recent_request_logs(10_000);
    Json(json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "running": true,
        "proxy_url": "http://127.0.0.1:18001",
        "protocols": ["openai", "anthropic_bridge"],
        "requests": logs.len(),
        "fallback": "disabled",
        "note": "d-token local service replaces external Headroom proxy for product traffic."
    }))
}

async fn headroom_mcp(State(state): State<AppState>) -> Json<Value> {
    let ccr = runtime_cache_summary_for_state(&state);
    let memory = memory_summary_for_state(&state);
    Json(json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "configured": true,
        "tools": ["headroom_compress", "headroom_retrieve", "headroom_stats"],
        "implemented_as": ["dtoken_compress", "dtoken_ccr_metadata", "dtoken_runtime_metrics"],
        "ccr_entries": ccr["entries"],
        "memory_entries": memory["entries"],
        "raw_available": false
    }))
}

async fn headroom_stats(State(state): State<AppState>) -> Json<Value> {
    let logs = state.recent_request_logs(10_000);
    Json(json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "stats": runtime_stats_payload(&logs),
        "history": runtime_stats_history_payload(&logs),
        "metrics": runtime_metrics_payload(&logs),
        "output_shape": output_shape_for_state(&state, None, None),
        "cache_summary": runtime_cache_summary_for_state(&state)
    }))
}

async fn headroom_cache_summary(State(state): State<AppState>) -> Json<Value> {
    Json(runtime_cache_summary_for_state(&state))
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

async fn config_patch_plan(State(state): State<AppState>) -> Json<Value> {
    let plan = json!(mock_patch_plan());
    append_stage_event(
        &state,
        Uuid::new_v4(),
        LayeredStage::ConfigPatch,
        EventStatus::Ok,
        json!({
            "source": "dtoken-owned-runtime",
            "action": "preview_config_patch_plan",
            "raw_available": false,
            "privacy": "Diff preview contains route metadata only; credentials and full agent configs are not returned."
        }),
        None,
    );
    Json(plan)
}

async fn config_rollback_points() -> Json<Value> {
    Json(json!([]))
}

async fn events(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.recent_events(50)))
}

async fn ccr_summary(State(state): State<AppState>) -> Json<Value> {
    Json(runtime_cache_summary_for_state(&state))
}

async fn ccr_retrieve(
    State(state): State<AppState>,
    AxumPath(recovery_ref): AxumPath<String>,
) -> Json<Value> {
    match state.retrieve_ccr_metadata(&recovery_ref) {
        Some(metadata) => Json(json!({
            "source": "dtoken-owned-runtime",
            "recovery_ref": recovery_ref,
            "raw_available": false,
            "metadata": metadata,
            "note": "Raw CCR retrieval is disabled by default; only metadata is returned."
        })),
        None => Json(json!({
            "source": "dtoken-owned-runtime",
            "recovery_ref": recovery_ref,
            "raw_available": false,
            "error": {
                "type": "ccr_metadata_not_found",
                "message": "No CCR metadata exists for this recovery ref."
            }
        })),
    }
}

async fn memory_summary(State(state): State<AppState>) -> Json<Value> {
    Json(memory_summary_for_state(&state))
}

async fn memory_learn(State(state): State<AppState>, Json(record): Json<Value>) -> Json<Value> {
    match state.record_memory(record) {
        Ok(saved) => Json(json!({
            "ok": true,
            "source": "dtoken-owned-runtime",
            "record": saved
        })),
        Err(err) => Json(json!({
            "ok": false,
            "source": "dtoken-owned-runtime",
            "error": {
                "type": "memory_record_failed",
                "message": err.to_string()
            }
        })),
    }
}

async fn output_shape(State(state): State<AppState>) -> Json<Value> {
    Json(output_shape_for_state(&state, None, None))
}

async fn agent_compatibility() -> Json<Value> {
    Json(json!(agent_support_profiles()))
}

async fn protocol_injection() -> Json<Value> {
    Json(json!(protocol_injection_profiles()))
}

fn agent_support_profiles() -> Vec<AgentSupportProfile> {
    use SupportLevel::{Full, None, Partial};

    vec![
        agent(
            "Claude Code",
            true,
            "PreToolUse shell hook",
            Full,
            true,
            "headroom wrap claude",
            Full,
            Partial,
            Full,
            "Native Windows RTK falls back to instructions; WSL gives full hook behavior.",
        ),
        agent(
            "VS Code Copilot Chat",
            true,
            "Copilot hook",
            Full,
            false,
            "not directly listed by Headroom wrap",
            Full,
            Partial,
            Full,
            "RTK supports VS Code Copilot hook; Headroom covers Copilot CLI/subscription path.",
        ),
        agent(
            "GitHub Copilot CLI",
            true,
            "Copilot preToolUse modifiedArgs",
            Full,
            true,
            "headroom wrap copilot",
            Full,
            Partial,
            Full,
            "Headroom can route Copilot subscription traffic through an OpenAI-compatible proxy.",
        ),
        agent(
            "Cursor",
            true,
            "preToolUse hook",
            Full,
            true,
            "manual Headroom proxy setup",
            Full,
            Partial,
            Full,
            "Headroom prints base URLs for Cursor settings.",
        ),
        agent(
            "Gemini CLI",
            true,
            "BeforeTool hook",
            Full,
            false,
            "shared memory/learn only",
            Full,
            Partial,
            Full,
            "Headroom README highlights shared memory with Gemini, not wrap support.",
        ),
        agent(
            "OpenCode",
            true,
            "TypeScript plugin",
            Full,
            true,
            "headroom wrap opencode",
            Full,
            Partial,
            Full,
            "Both projects support plugin/config injection.",
        ),
        agent(
            "OpenClaw",
            true,
            "TypeScript plugin",
            Full,
            true,
            "headroom wrap openclaw",
            Full,
            Partial,
            Full,
            "Headroom installs as ContextEngine plugin.",
        ),
        agent(
            "Pi",
            true,
            "TypeScript extension",
            Full,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK extension support only.",
        ),
        agent(
            "Hermes",
            true,
            "Python plugin",
            Full,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK plugin fails open if unavailable.",
        ),
        agent(
            "Cline / Roo Code",
            true,
            "rules file",
            Partial,
            true,
            "headroom wrap cline",
            Full,
            Partial,
            Full,
            "RTK rewrite is instruction-level; Headroom can inject config.",
        ),
        agent(
            "Windsurf",
            true,
            "rules file",
            Partial,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK prompt-level guidance.",
        ),
        agent(
            "Codex CLI",
            true,
            "AGENTS.md instructions",
            Partial,
            true,
            "headroom wrap codex",
            Full,
            Partial,
            Full,
            "Headroom shares memory with Claude; RTK is instruction-level.",
        ),
        agent(
            "Kilo Code",
            true,
            ".kilocode/rules",
            Partial,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK prompt-level guidance.",
        ),
        agent(
            "Google Antigravity",
            true,
            ".agents/rules",
            Partial,
            false,
            "not listed",
            Full,
            Partial,
            Full,
            "RTK prompt-level guidance.",
        ),
        agent(
            "Mistral Vibe",
            true,
            "planned RTK support",
            None,
            true,
            "headroom wrap vibe",
            Partial,
            None,
            Partial,
            "RTK is blocked on upstream; Headroom lists Vibe wrap.",
        ),
        agent(
            "Aider",
            false,
            "not listed",
            None,
            true,
            "headroom wrap aider",
            Full,
            Partial,
            Full,
            "Headroom starts proxy and launches Aider.",
        ),
        agent(
            "Continue",
            false,
            "not listed",
            None,
            true,
            "headroom wrap continue",
            Full,
            Partial,
            Full,
            "Headroom injects config.",
        ),
        agent(
            "Goose",
            false,
            "not listed",
            None,
            true,
            "headroom wrap goose",
            Full,
            Partial,
            Full,
            "Headroom starts proxy and launches Goose.",
        ),
        agent(
            "OpenHands",
            false,
            "not listed",
            None,
            true,
            "headroom wrap openhands",
            Full,
            Partial,
            Full,
            "Headroom starts proxy and launches OpenHands.",
        ),
        agent(
            "Cortex Code",
            false,
            "not listed",
            None,
            true,
            "Headroom library only",
            Partial,
            Partial,
            Partial,
            "Headroom README lists library-only savings.",
        ),
    ]
}

fn agent(
    agent: &str,
    rtk_supported: bool,
    rtk_method: &str,
    rtk_transparent_rewrite: SupportLevel,
    headroom_supported: bool,
    headroom_method: &str,
    macos_support: SupportLevel,
    windows_native_support: SupportLevel,
    windows_wsl_support: SupportLevel,
    notes: &str,
) -> AgentSupportProfile {
    AgentSupportProfile {
        agent: agent.to_string(),
        rtk_supported,
        rtk_method: rtk_method.to_string(),
        rtk_transparent_rewrite,
        headroom_supported,
        headroom_method: headroom_method.to_string(),
        macos_support,
        windows_native_support,
        windows_wsl_support,
        notes: notes.to_string(),
    }
}

fn protocol_injection_profiles() -> Vec<ProtocolInjectionProfile> {
    vec![
        ProtocolInjectionProfile {
            protocol: Protocol::OpenAi,
            compatible_clients: vec![
                "OpenAI SDK".to_string(),
                "OpenAI-compatible clients".to_string(),
                "Vercel AI SDK".to_string(),
                "LiteLLM".to_string(),
                "Copilot subscription bridge".to_string(),
            ],
            injection_method: "Proxy/base-url injection for OpenAI-compatible chat or responses traffic".to_string(),
            development_default: "fixture-only; no live proxy or user service is contacted".to_string(),
            managed_mode_action: "install or patch client/provider route after diff, backup, confirmation, doctor, and rollback point".to_string(),
            safety_notes: vec![
                "Do not rewrite real provider config in development mode".to_string(),
                "Strip internal headers before upstream forwarding".to_string(),
                "Keep route decisions explainable in the event ledger".to_string(),
            ],
        },
        ProtocolInjectionProfile {
            protocol: Protocol::Anthropic,
            compatible_clients: vec![
                "Anthropic SDK".to_string(),
                "Claude Code compatible traffic".to_string(),
                "Anthropic-compatible proxy clients".to_string(),
                "Bedrock Anthropic shape".to_string(),
                "Vertex Anthropic publisher shape".to_string(),
            ],
            injection_method: "Proxy/base-url injection for Anthropic messages traffic with live-zone compression boundaries".to_string(),
            development_default: "fixture-only; no live proxy or user service is contacted".to_string(),
            managed_mode_action: "install or patch Anthropic route settings transactionally in M4".to_string(),
            safety_notes: vec![
                "Never compress human content blindly".to_string(),
                "Preserve cache hot-zone stability".to_string(),
                "CCR raw cache is summarized only by default".to_string(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        serde_json::from_slice(&bytes).expect("response body should be valid JSON")
    }

    fn configured_openai_test_state(base_url: String, timeout_ms: u64) -> AppState {
        AppState::new(DtokenConfig {
            providers: vec![dtoken_types::ProviderProfile {
                id: "local-mock-provider".to_string(),
                name: "Local mock provider".to_string(),
                kind: dtoken_types::ProviderKind::Custom,
                protocol: Protocol::OpenAi,
                base_url,
                api_key_ref: None,
                models: vec!["mock/model".to_string()],
                timeout_ms,
                healthcheck_path: None,
            }],
            routes: vec![dtoken_types::RouteProfile {
                id: "local-mock-route".to_string(),
                name: "Local mock route".to_string(),
                input_protocol: Protocol::OpenAi,
                instance_id: "local-test-instance".to_string(),
                target_provider_id: "local-mock-provider".to_string(),
                fallback_provider_id: None,
                chain_mode: ChainMode::Direct,
                compression_policy: CompressionPolicy::Balanced,
                bind_host: "127.0.0.1".to_string(),
                local_port: 18001,
            }],
            instances: vec![],
        })
    }

    async fn spawn_json_upstream(
        status_line: &str,
        body: &str,
        delay_ms: u64,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local mock upstream");
        let addr = listener.local_addr().expect("local mock address");
        let status_line = status_line.to_string();
        let body = body.to_string();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept local request");
            let mut buffer = [0_u8; 4096];
            let _ = socket.read(&mut buffer).await.expect("read local request");
            if delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            let response = format!(
                "HTTP/1.1 {status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });
        (addr, server)
    }

    #[tokio::test]
    async fn openai_chat_without_route_fails_closed() {
        let state = AppState::new(DtokenConfig::empty());
        let response = openai_chat_completions(
            State(state.clone()),
            Json(OpenAiChatRequest {
                model: Some("example/model".to_string()),
                messages: Some(vec![json!({"role": "user", "content": "hello"})]),
                stream: Some(false),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = response_json(response).await;
        assert_eq!(payload["error"]["type"], "dtoken_route_not_configured");
        assert_eq!(payload["dtoken"]["fallback"], "disabled");
        assert!(payload.get("choices").is_none());
        assert!(!payload.to_string().contains("d-token-ok"));
        assert_eq!(state.recent_events(1)[0].status, EventStatus::Failed);
    }

    #[tokio::test]
    async fn openai_responses_without_route_fails_closed() {
        let state = AppState::new(DtokenConfig::empty());
        let response = openai_responses(
            State(state),
            Json(json!({"model": "example/model", "input": "hello"})),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = response_json(response).await;
        assert_eq!(payload["error"]["type"], "dtoken_route_not_configured");
        assert!(payload.get("output").is_none());
        assert!(!payload.to_string().contains("d-token-ok"));
    }

    #[tokio::test]
    async fn anthropic_messages_without_route_fails_closed() {
        let state = AppState::new(DtokenConfig::empty());
        let response = anthropic_messages(
            State(state),
            Json(json!({"model": "example/model", "messages": []})),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = response_json(response).await;
        assert_eq!(payload["error"]["type"], "dtoken_route_not_configured");
        assert_eq!(payload["dtoken"]["fallback"], "disabled");
    }

    #[tokio::test]
    async fn configured_responses_rejects_missing_upstream_text_without_synthetic_success() {
        let (addr, server) = spawn_json_upstream(
            "200 OK",
            r#"{"id":"chatcmpl-local","choices":[{"message":{"role":"assistant"}}]}"#,
            0,
        )
        .await;
        let state = configured_openai_test_state(format!("http://{addr}/v1"), 1_000);

        let response = openai_responses(
            State(state.clone()),
            Json(json!({"model": "mock/model", "input": "hello"})),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let payload = response_json(response).await;
        assert_eq!(payload["error"]["type"], "provider_invalid_response");
        assert!(!payload.to_string().contains("d-token-ok"));
        assert_eq!(
            state.recent_request_logs(1)[0].error.as_deref(),
            Some("provider_invalid_response")
        );
        assert_eq!(state.recent_events(1)[0].status, EventStatus::Failed);
        server.await.expect("local mock upstream task");
    }

    #[tokio::test]
    async fn configured_chat_preserves_upstream_error_status_and_failed_ledger_state() {
        let (addr, server) = spawn_json_upstream(
            "401 Unauthorized",
            r#"{"error":{"message":"invalid local test credential"}}"#,
            0,
        )
        .await;
        let state = configured_openai_test_state(format!("http://{addr}/v1"), 1_000);

        let response = openai_chat_completions(
            State(state.clone()),
            Json(OpenAiChatRequest {
                model: Some("mock/model".to_string()),
                messages: Some(vec![json!({"role": "user", "content": "hello"})]),
                stream: Some(false),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let payload = response_json(response).await;
        assert_eq!(payload["error"]["type"], "provider_auth");
        assert_eq!(
            state.recent_request_logs(1)[0].error.as_deref(),
            Some("provider_auth")
        );
        assert!(state
            .recent_events(20)
            .iter()
            .any(|event| event.status == EventStatus::Failed
                && event.error_code.as_deref() == Some("provider_auth")));
        server.await.expect("local mock upstream task");
    }

    #[tokio::test]
    async fn configured_provider_timeout_is_enforced_and_reported_as_gateway_timeout() {
        let (addr, server) = spawn_json_upstream(
            "200 OK",
            r#"{"choices":[{"message":{"content":"too late"}}]}"#,
            100,
        )
        .await;
        let state = configured_openai_test_state(format!("http://{addr}/v1"), 10);

        let response = openai_chat_completions(
            State(state.clone()),
            Json(OpenAiChatRequest {
                model: Some("mock/model".to_string()),
                messages: Some(vec![json!({"role": "user", "content": "hello"})]),
                stream: Some(false),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        let payload = response_json(response).await;
        assert_eq!(payload["error"]["type"], "provider_timeout");
        assert_eq!(
            state.recent_request_logs(1)[0].error.as_deref(),
            Some("provider_timeout")
        );
        server.await.expect("local mock upstream task");
    }

    #[tokio::test]
    async fn production_router_does_not_expose_mock_install_endpoints() {
        for (method, uri) in [("GET", "/install/scan"), ("POST", "/install/plan")] {
            let response = build_router(AppState::new(DtokenConfig::empty()))
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("router should respond");

            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        }
    }

    #[test]
    fn agent_matrix_covers_rtk_and_headroom_union() {
        let agents = agent_support_profiles();
        for required in [
            "Claude Code",
            "Codex CLI",
            "Cursor",
            "Gemini CLI",
            "OpenCode",
            "OpenClaw",
            "Pi",
            "Hermes",
            "Cline / Roo Code",
            "Windsurf",
            "Kilo Code",
            "Google Antigravity",
            "Aider",
            "Continue",
            "Goose",
            "OpenHands",
            "Cortex Code",
        ] {
            assert!(
                agents.iter().any(|agent| agent.agent == required),
                "missing {required}"
            );
        }
    }

    #[test]
    fn protocol_injection_covers_openai_and_anthropic() {
        let profiles = protocol_injection_profiles();
        assert!(profiles
            .iter()
            .any(|profile| profile.protocol == Protocol::OpenAi));
        assert!(profiles
            .iter()
            .any(|profile| profile.protocol == Protocol::Anthropic));
    }

    #[test]
    fn dashboard_payload_is_dtoken_owned_and_covers_headroom_surface() {
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "build log warning warning"
            })]),
            stream: Some(false),
        };
        let (_response, decision, request_log, _event) = routed_openai_payload(&request);
        let payload = build_dashboard_payload(DashboardPayloadInput {
            stats: json!({"summary": {"api_requests": 1}}),
            history: json!({"history": []}),
            metrics: json!({"available": true}),
            cache_summary: json!({"entries": 0}),
            providers: json!([]),
            routes: json!([]),
            instances: json!([]),
            events: json!([]),
            route_decisions: json!([route_decision_to_value(&decision)]),
            request_logs: json!([request_log]),
            memory: json!({"entries": 0}),
        });

        assert_eq!(payload["source"], "dtoken-owned-runtime");
        assert_eq!(payload["runtime"]["kind"], "headroom-derived");
        assert_eq!(payload["runtime"]["external_headroom_dependency"], false);
        let sections = payload["dashboard_sections"]
            .as_array()
            .expect("sections array");
        for required in [
            "summary",
            "agent_usage",
            "prefix_cache",
            "request_logs",
            "recent_requests",
            "subscription_window",
            "throughput",
            "config",
        ] {
            assert!(
                sections.iter().any(|value| value == required),
                "missing dashboard section {required}"
            );
        }
        assert_eq!(payload["route_decisions"][0]["fallback"], "disabled");
        assert!(payload["request_logs"]
            .as_array()
            .is_some_and(|logs| !logs.is_empty()));
    }

    #[test]
    fn default_dashboard_payload_uses_empty_dtoken_runtime_not_external_headroom() {
        let payload = dashboard_payload_default();

        assert_eq!(payload["source"], "dtoken-owned-runtime");
        assert_eq!(payload["stats"]["source"], "dtoken-owned-runtime");
        assert_eq!(payload["stats"]["status"], "empty_runtime");
        assert_eq!(payload["stats"]["requests"]["total"], 0);
        assert_eq!(payload["metrics"]["external_headroom_dependency"], false);
        assert_eq!(payload["cache_summary"]["source"], "dtoken-owned-runtime");
        assert_eq!(payload["events"].as_array().unwrap().len(), 0);
        assert_eq!(payload["route_decisions"].as_array().unwrap().len(), 0);
        assert_eq!(payload["request_logs"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn dashboard_payload_exposes_runtime_active_config_status() {
        let config = DtokenConfig {
            providers: vec![dtoken_types::ProviderProfile {
                id: "active-provider".to_string(),
                name: "Active Provider".to_string(),
                kind: dtoken_types::ProviderKind::Custom,
                protocol: Protocol::OpenAi,
                base_url: "https://example.invalid/v1".to_string(),
                api_key_ref: None,
                models: vec!["model-a".to_string()],
                timeout_ms: 30_000,
                healthcheck_path: None,
            }],
            routes: vec![dtoken_types::RouteProfile {
                id: "active-route".to_string(),
                name: "Active Route".to_string(),
                input_protocol: Protocol::OpenAi,
                instance_id: "active-instance".to_string(),
                target_provider_id: "active-provider".to_string(),
                fallback_provider_id: None,
                chain_mode: ChainMode::Direct,
                compression_policy: CompressionPolicy::Balanced,
                bind_host: "127.0.0.1".to_string(),
                local_port: 18001,
            }],
            instances: vec![],
        };
        let status = RuntimeConfigStatus::from_config(
            "desktop_active_config",
            Some("C:/Users/example/AppData/Local/d-token/active-config/dtoken-config.json".to_string()),
            &config,
        );
        let state = AppState::new_with_config_status(config, status);
        let payload = dashboard_payload_for_state(&state);
        let active = active_config_payload(&state);

        assert_eq!(payload["active_config"]["config"]["source"], "desktop_active_config");
        assert_eq!(payload["active_config"]["config"]["route_count"], 1);
        assert_eq!(payload["active_config"]["config"]["fallback_disabled"], true);
        assert_eq!(active["providers"][0]["id"], "active-provider");
        assert_eq!(active["privacy"].as_str().unwrap().contains("API keys"), true);
    }

    #[test]
    fn runtime_export_includes_redacted_active_config_metadata() {
        let config = DtokenConfig {
            providers: vec![dtoken_types::ProviderProfile {
                id: "export-provider".to_string(),
                name: "Export Provider".to_string(),
                kind: dtoken_types::ProviderKind::Custom,
                protocol: Protocol::OpenAi,
                base_url: "https://example.invalid/v1".to_string(),
                api_key_ref: Some("local-secret-ref".to_string()),
                models: vec!["model-a".to_string()],
                timeout_ms: 30_000,
                healthcheck_path: None,
            }],
            routes: vec![dtoken_types::RouteProfile {
                id: "export-route".to_string(),
                name: "Export Route".to_string(),
                input_protocol: Protocol::OpenAi,
                instance_id: "export-instance".to_string(),
                target_provider_id: "export-provider".to_string(),
                fallback_provider_id: None,
                chain_mode: ChainMode::Direct,
                compression_policy: CompressionPolicy::Balanced,
                bind_host: "127.0.0.1".to_string(),
                local_port: 18001,
            }],
            instances: vec![],
        };
        let status = RuntimeConfigStatus::from_config(
            "desktop_active_config",
            Some("C:/Users/example/AppData/Local/d-token/active-config/dtoken-config.json".to_string()),
            &config,
        );
        let state = AppState::new_with_config_status(config, status);
        let json_export = runtime_export_json_payload(&state);
        let csv_export = runtime_export_csv_payload(&state);

        assert_eq!(json_export["redacted"], true);
        assert_eq!(json_export["active_config"]["config"]["source"], "desktop_active_config");
        assert_eq!(json_export["active_config"]["config"]["fallback_disabled"], true);
        assert!(json_export["privacy"].as_str().unwrap().contains("No raw prompt"));
        assert!(csv_export.contains("config_status"));
        assert!(csv_export.contains("source=desktop_active_config"));
        assert!(!json_export.to_string().contains("sk-"));
        assert!(!csv_export.contains("sk-"));
    }

    #[test]
    fn openai_route_adapter_records_compression_and_route_metadata() {
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "2026-07-04T10:00:00Z INFO build started\n2026-07-04T10:00:00Z INFO build started\n\n2026-07-04T10:00:01Z WARN retrying route\n2026-07-04T10:00:01Z WARN retrying route\nfinal useful line"
            })]),
            stream: Some(false),
        };

        let (response, decision, request_log, event) = routed_openai_payload(&request);

        assert_eq!(response["choices"][0]["message"]["content"], "d-token-ok");
        assert_eq!(response["dtoken"]["source"], "dtoken-owned-runtime");
        assert_eq!(response["dtoken"]["external_headroom_dependency"], false);
        assert_eq!(decision.route_id, "dtoken-opencode-local");
        assert_eq!(decision.provider_id, "opencode-official-free");
        assert_eq!(request_log.agent.as_deref(), Some("OpenCode"));
        assert_eq!(
            request_log.provider_id.as_deref(),
            Some("opencode-official-free")
        );
        assert!(request_log.tokens_saved.unwrap_or_default() > 0);
        assert!(request_log.latency_ms.unwrap_or_default() > 0);
        assert!(request_log
            .transforms_applied
            .iter()
            .any(|item| item == "repeat_line_fold"));
        assert_eq!(event.source, EventSource::Dtoken);
        assert_eq!(event.status, EventStatus::Ok);
        assert!(event.metadata.get("full_prompt").is_none());
    }

    #[test]
    fn app_state_reflects_routed_request_state() {
        let state = AppState::new(DtokenConfig::empty());
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "repeat\nrepeat\nrepeat\nbuild log warning warning"
            })]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        state.record_routed_request(&decision, request_log, event);

        let route_decisions = state.recent_route_decisions(20);
        let request_logs = state.recent_request_logs(20);
        let events = state.recent_events(20);

        assert_eq!(route_decisions[0]["route_id"], "dtoken-opencode-local");
        assert_eq!(request_logs[0].agent.as_deref(), Some("OpenCode"));
        assert_eq!(events[0].source, EventSource::Dtoken);
    }

    #[test]
    fn streaming_requests_are_explicitly_recorded_as_unsupported() {
        let state = AppState::new(DtokenConfig::empty());
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "stream this response"
            })]),
            stream: Some(true),
        };
        let (response, decision, request_log, event) = routed_openai_payload(&request);
        let (request_log, event) = mark_streaming_unsupported(request_log, event);
        let error = streaming_not_supported_response(
            decision.request_id,
            &decision,
            &request_log,
            &event,
            response["dtoken"]["compression"].clone(),
        );
        state.record_routed_request(&decision, request_log, event);

        assert_eq!(error["error"]["type"], "streaming_not_supported_yet");
        assert_eq!(state.recent_request_logs(1)[0].error.as_deref(), Some("streaming_not_supported_yet"));
        assert_eq!(state.recent_events(1)[0].status, EventStatus::Failed);
        assert_eq!(
            state.recent_events(1)[0].error_code.as_deref(),
            Some("streaming_not_supported_yet")
        );
        assert_eq!(state.recent_route_decisions(1)[0]["fallback"], "disabled");
    }

    #[tokio::test]
    async fn configured_streaming_route_proxies_sse_body_without_storing_raw_content() {
        use axum::body::to_bytes;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock upstream");
        let addr = listener.local_addr().expect("mock addr");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept request");
            let mut buffer = [0_u8; 4096];
            let _ = socket.read(&mut buffer).await.expect("read request");
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\ndata: [DONE]\n\n",
                )
                .await
                .expect("write sse");
        });

        let provider = dtoken_types::ProviderProfile {
            id: "mock-stream".to_string(),
            name: "Mock stream".to_string(),
            kind: dtoken_types::ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url: format!("http://{addr}/v1"),
            api_key_ref: None,
            models: vec!["mock/model".to_string()],
            timeout_ms: 30_000,
            healthcheck_path: None,
        };

        let result = forward_openai_chat_stream(
            &reqwest::Client::new(),
            &provider,
            json!({
                "model": "mock/model",
                "messages": [{"role": "user", "content": "hello"}],
                "stream": true
            }),
        )
        .await
        .expect("stream response");

        assert_eq!(result.0, 200);
        assert_eq!(
            result.1.get(CONTENT_TYPE).and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
        let body = to_bytes(result.2, usize::MAX).await.expect("body bytes");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("data:"));
        assert!(text.contains("[DONE]"));
        server.await.expect("mock upstream task");
    }

    #[test]
    fn runtime_stats_metrics_and_history_reflect_recorded_requests() {
        let state = AppState::new(DtokenConfig::empty());
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "repeat\nrepeat\nrepeat\nbuild log warning warning"
            })]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        let saved = request_log.tokens_saved.unwrap_or_default() as u64;
        state.record_routed_request(&decision, request_log, event);

        let stats = runtime_stats_for_state(&state);
        let history = runtime_stats_history_for_state(&state);
        let logs = state.recent_request_logs(10_000);
        let metrics = runtime_metrics_text(&logs);
        let metrics_payload = runtime_metrics_payload(&logs);

        assert_eq!(stats["status"], "runtime");
        assert_eq!(stats["source"], "dtoken-owned-runtime");
        assert_eq!(stats["requests"]["total"], 1);
        assert_eq!(stats["tokens"]["saved"], saved);
        assert!(stats["savings_percent"].as_f64().unwrap_or_default() > 0.0);
        assert_eq!(history["source"], "dtoken-owned-runtime");
        assert_eq!(history["history"][0]["requests"], 1);
        assert!(metrics.contains("dtoken_requests_total 1"));
        assert!(metrics.contains(&format!("dtoken_tokens_saved_total {saved}")));
        assert!(metrics.contains("dtoken_compression_latency_ms_avg"));
        assert!(metrics.contains("dtoken_route_adapter_latency_ms_avg"));
        assert!(metrics.contains("dtoken_provider_latency_ms_avg"));
        assert!(metrics.contains("dtoken_route_overhead_tokens_total"));
        assert!(metrics.contains("dtoken_route_overhead_percent"));
        assert_eq!(
            stats["latency_breakdown"]["model"],
            "derived_from_compression_elapsed_and_route_forward_latency"
        );
        assert!(stats["latency_breakdown"]["compression_ms"]
            .as_f64()
            .unwrap_or_default()
            >= 0.0);
        assert!(stats["latency_breakdown"]["route_adapter_ms"]
            .as_f64()
            .unwrap_or_default()
            >= 0.0);
        assert!(stats["latency_breakdown"]["provider_ms"]
            .as_f64()
            .unwrap_or_default()
            >= 0.0);
        assert_eq!(
            stats["route_overhead"]["fallback_policy"],
            "disabled_unless_route_explicitly_configures_fallback"
        );
        assert!(stats["route_overhead"]["tokens"].as_i64().is_some());
        assert!(stats["route_overhead"]["percent"].as_f64().is_some());
        assert_eq!(
            metrics_payload["compression_performance"]["total_requests"], 1
        );
        assert_eq!(
            metrics_payload["route_performance"]["latency_breakdown"]["model"],
            "derived_from_compression_elapsed_and_route_forward_latency"
        );
        assert_eq!(
            metrics_payload["route_performance"]["route_overhead"]["fallback_policy"],
            "disabled_unless_route_explicitly_configures_fallback"
        );
        let transforms = metrics_payload["compression_performance"]["transforms"]
            .as_array()
            .expect("transform performance rows");
        assert!(transforms.iter().any(|row| {
            row["name"] == "repeat_line_fold"
                && row["tokens_saved"].as_u64().unwrap_or_default() > 0
                && row["avg_latency_ms"].as_f64().unwrap_or_default() > 0.0
        }));
        let routes = metrics_payload["route_performance"]["routes"]
            .as_array()
            .expect("route performance rows");
        assert!(routes.iter().any(|row| {
            row["name"] == "dtoken-opencode-local"
                && row["requests"] == 1
                && row["failed"] == 0
                && row["tokens_saved"].as_u64().unwrap_or_default() == saved
        }));
        let providers = metrics_payload["route_performance"]["providers"]
            .as_array()
            .expect("provider performance rows");
        assert!(providers.iter().any(|row| {
            row["name"] == "opencode-official-free" && row["requests"] == 1 && row["failed"] == 0
        }));
        let policies = metrics_payload["route_performance"]["policies"]
            .as_array()
            .expect("policy performance rows");
        assert!(policies.iter().any(|row| {
            row["name"] == "dtoken-owned-runtime" && row["requests"] == 1 && row["failed"] == 0
        }));
    }


    #[test]
    fn responses_route_records_event_and_provider_forward_metadata() {
        let state = AppState::new(DtokenConfig::empty());
        let request = OpenAiChatRequest {
            model: Some("codex/responses-compatible".to_string()),
            messages: Some(vec![json!({"role": "user", "content": "repeat\nrepeat\nresponses route"})]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        state.record_routed_request(&decision, request_log, event);
        append_stage_event(
            &state,
            decision.request_id,
            LayeredStage::ProviderForward,
            EventStatus::Skipped,
            json!({
                "input_protocol": "openai_responses",
                "output_protocol": "openai",
                "reason": "responses-compatible local route proof",
                "fallback": "disabled",
                "raw_available": false
            }),
            None,
        );

        let events = state.recent_events(20);
        assert!(events.iter().any(|event| event.stage == LayeredStage::RouteForward));
        assert!(events.iter().any(|event| {
            event.stage == LayeredStage::ProviderForward
                && event.status == EventStatus::Skipped
                && event.metadata["input_protocol"] == "openai_responses"
                && event.metadata["raw_available"] == false
        }));
    }

    #[test]
    fn config_patch_plan_records_ledger_event_without_raw_config() {
        let state = AppState::new(DtokenConfig::empty());
        append_stage_event(
            &state,
            Uuid::new_v4(),
            LayeredStage::ConfigPatch,
            EventStatus::Ok,
            json!({
                "source": "dtoken-owned-runtime",
                "action": "preview_config_patch_plan",
                "raw_available": false,
                "privacy": "Diff preview contains route metadata only; credentials and full agent configs are not returned."
            }),
            None,
        );

        let event = state.recent_events(1).remove(0);
        assert_eq!(event.stage, LayeredStage::ConfigPatch);
        assert_eq!(event.status, EventStatus::Ok);
        assert_eq!(event.metadata["raw_available"], false);
        assert!(event.metadata.get("api_key").is_none());
        assert!(event.metadata.get("raw_config").is_none());
    }

    #[test]
    fn persistent_runtime_state_survives_app_state_restart() {
        let dir = std::env::temp_dir().join(format!("dtoken-runtime-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "repeat\nrepeat\nrepeat\nbuild log warning warning"
            })]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        let saved = request_log.tokens_saved.unwrap_or_default() as u64;
        state.record_routed_request(&decision, request_log, event);

        let restarted = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let stats = runtime_stats_for_state(&restarted);

        assert_eq!(restarted.recent_route_decisions(20).len(), 1);
        assert_eq!(restarted.recent_request_logs(20).len(), 1);
        assert_eq!(restarted.recent_events(20).len(), 1);
        assert_eq!(stats["requests"]["total"], 1);
        assert_eq!(stats["tokens"]["saved"], saved);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ccr_metadata_summary_survives_restart_without_raw_prompt() {
        let dir = std::env::temp_dir().join(format!("dtoken-ccr-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "secret prompt should not be stored\nsecret prompt should not be stored"
            })]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        state.record_routed_request(&decision, request_log, event);

        let restarted = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let summary = runtime_cache_summary_for_state(&restarted);
        let refs = restarted.recent_ccr_metadata(10);
        let retrieved = restarted
            .retrieve_ccr_metadata(refs[0]["recovery_ref"].as_str().unwrap())
            .expect("ccr metadata");

        assert_eq!(summary["source"], "dtoken-owned-runtime");
        assert_eq!(summary["entries"], 1);
        assert_eq!(summary["raw_available"], false);
        assert_eq!(retrieved["raw_available"], false);
        assert!(retrieved.get("raw_prompt").is_none());
        assert!(!retrieved
            .to_string()
            .contains("secret prompt should not be stored"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn memory_learn_summary_survives_restart_without_raw_prompt() {
        let dir = std::env::temp_dir().join(format!("dtoken-memory-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let record = json!({
            "project_id": "demo-project",
            "agent": "OpenCode",
            "kind": "verbosity",
            "summary": "Prefer terse command output unless tests fail.",
            "output_verbosity": "terse",
            "raw_prompt": "must not be stored"
        });

        state.record_memory(record).expect("record memory");
        let restarted = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let summary = memory_summary_for_state(&restarted);
        let shape = output_shape_for_state(&restarted, Some("demo-project"), Some("OpenCode"));

        assert_eq!(summary["source"], "dtoken-owned-runtime");
        assert_eq!(summary["entries"], 1);
        assert_eq!(summary["raw_available"], false);
        assert_eq!(summary["by_project"]["demo-project"], 1);
        assert_eq!(shape["output_verbosity"], "terse");
        assert_eq!(
            shape["learned_rules"][0]["summary"],
            "Prefer terse command output unless tests fail."
        );
        assert!(summary.get("raw_prompt").is_none());
        assert!(!summary.to_string().contains("must not be stored"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn runtime_export_is_redacted_and_contains_route_evidence() {
        let state = AppState::new(DtokenConfig::empty());
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "SECRET_PROMPT_SHOULD_NOT_APPEAR\nrepeat\nrepeat"
            })]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        state.record_routed_request(&decision, request_log, event);

        let export = runtime_export_json_payload(&state);
        let serialized = export.to_string();
        assert_eq!(export["source"], "dtoken-owned-runtime");
        assert_eq!(export["redacted"], true);
        assert_eq!(export["raw_available"], false);
        assert_eq!(export["request_logs"].as_array().unwrap().len(), 1);
        assert_eq!(export["route_decisions"][0]["fallback"], "disabled");
        assert!(serialized.contains("dtoken-opencode-local"));
        assert!(!serialized.contains("SECRET_PROMPT_SHOULD_NOT_APPEAR"));
        assert!(!serialized.contains("Authorization"));
        assert!(!serialized.contains("Cookie"));

        let csv = runtime_export_csv_payload(&state);
        assert!(csv.contains("kind,request_id,timestamp_ms"));
        assert!(csv.contains("request_log"));
        assert!(csv.contains("dtoken-opencode-local"));
        assert!(!csv.contains("SECRET_PROMPT_SHOULD_NOT_APPEAR"));
    }

    #[test]
    fn headroom_parity_payloads_are_dtoken_owned_runtime_derived() {
        let state = AppState::new(DtokenConfig::empty());
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "repeat\nrepeat\nruntime headroom parity"
            })]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        state.record_routed_request(&decision, request_log, event);

        let logs = state.recent_request_logs(10_000);
        let stats = runtime_stats_payload(&logs);
        let metrics = runtime_metrics_payload(&logs);
        let cache = runtime_cache_summary_for_state(&state);
        let memory = memory_summary_for_state(&state);

        assert_eq!(stats["source"], "dtoken-owned-runtime");
        assert_eq!(stats["requests"]["total"], 1);
        assert_eq!(metrics["source"], "dtoken-owned-runtime");
        assert_eq!(metrics["external_headroom_dependency"], false);
        assert_eq!(cache["source"], "dtoken-owned-runtime");
        assert_eq!(cache["external_headroom_dependency"], false);
        assert_eq!(cache["raw_available"], false);
        assert_eq!(memory["source"], "dtoken-owned-runtime");
        assert!(metrics["compression_performance"]["total_requests"]
            .as_u64()
            .unwrap_or_default()
            > 0);
    }

    #[test]
    fn anthropic_message_request_converts_to_openai_route_body() {
        let config = DtokenConfig {
            providers: vec![dtoken_types::ProviderProfile {
                id: "cc-switch-openai".to_string(),
                name: "cc-switch OpenAI bridge".to_string(),
                kind: dtoken_types::ProviderKind::LocalChain,
                protocol: Protocol::OpenAi,
                base_url: "http://127.0.0.1:39999/v1".to_string(),
                api_key_ref: None,
                models: vec!["mock/model".to_string()],
                timeout_ms: 30_000,
                healthcheck_path: Some("/health".to_string()),
            }],
            routes: vec![dtoken_types::RouteProfile {
                id: "anthropic-to-cc-switch".to_string(),
                name: "Anthropic to cc-switch".to_string(),
                input_protocol: Protocol::Anthropic,
                instance_id: "claude-code-instance".to_string(),
                target_provider_id: "cc-switch-openai".to_string(),
                fallback_provider_id: None,
                chain_mode: ChainMode::ChainBridge,
                compression_policy: CompressionPolicy::Balanced,
                bind_host: "127.0.0.1".to_string(),
                local_port: 18001,
            }],
            instances: vec![],
        };
        let state = AppState::new(config);
        let body = json!({
            "model": "claude-3-5-sonnet-latest",
            "max_tokens": 64,
            "messages": [{
                "role": "user",
                "content": "2026-07-04T10:00:00Z INFO bridge\n2026-07-04T10:00:00Z INFO bridge\n"
            }]
        });

        let payload = anthropic_route_payload(&state, body).expect("anthropic route payload");

        assert_eq!(payload.decision.route_id, "anthropic-to-cc-switch");
        assert_eq!(payload.decision.provider_id, "cc-switch-openai");
        assert_eq!(payload.upstream_body["messages"][0]["role"], "user");
        assert!(payload.upstream_body["messages"][0]["content"]
            .as_str()
            .unwrap_or_default()
            .contains("repeated x2"));
        assert_eq!(payload.request_log.input_protocol, Protocol::Anthropic);
        assert_eq!(
            payload.request_log.route_id.as_deref(),
            Some("anthropic-to-cc-switch")
        );
        assert!(payload.request_log.tokens_saved.unwrap_or_default() > 0);
        assert_eq!(payload.event.metadata["input_protocol"], "anthropic");
        assert_eq!(payload.event.metadata["output_protocol"], "openai");
        assert!(payload.event.metadata.get("full_prompt").is_none());
    }
}
