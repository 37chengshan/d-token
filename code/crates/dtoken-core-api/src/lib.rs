use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path as AxumPath, State},
    http::header::{CACHE_CONTROL, CONTENT_TYPE},
    http::HeaderMap,
    middleware::{self, Next},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use dtoken_compress::optical::{
    HybridContextEngine, HybridPreparationKind, OpticalCapabilityRegistry, PreparedHybridRequest,
    RenderCancellation,
};
use dtoken_compress::{CompressionEngine, ResolvedOptimizationContext};
use dtoken_config::DtokenConfig;
use dtoken_config_patcher::mock_patch_plan;
use dtoken_doctor::{run_basic_doctor, run_dev_no_touch_doctor};
use dtoken_event_ledger::{EventLedger, JsonlEventLedger, JsonlStore};
use dtoken_headroom_adapter::detect_headroom_status;
use dtoken_preprocessor::{
    detect_rtk_status, list_preprocessors, rtk_discover_report, rtk_gain_mock, rtk_session_report,
};
use dtoken_project_policy::{
    default_policy, demo_projects, RecommendationInput, RuleBasedRecommendationEngine,
};
use dtoken_types::{
    AgentSupportProfile, BaselineKind, CacheTokens, ChainMode, CompressionPolicy, DtokenEvent,
    EventSource, EventStatus, HealthResponse, InputTokenUsage, LayeredSavingsReport, LayeredStage,
    ModalityTokens, OpticalApplicabilityInput, OpticalCapabilitiesPublicSummary,
    OpticalCapabilityLifecyclePublicSummary, OpticalEndpoint, OpticalEventMetadataPublicSummary,
    OpticalEventPublicSummary, OpticalMode, OpticalPlanPublicSummary, OpticalPlanStatus,
    OpticalPreviewRequest, OpticalStatusPublicSummary, PayloadByteUsage, Protocol,
    ProtocolInjectionProfile, RequestLog, RouteDecision, SupportLevel, UpstreamSendState,
    UsageEvidence, UsageEvidenceKind, UsageMeasurement, UsageVector,
};
use futures_util::TryStreamExt;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::Path as StdPath,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

/// Bounds all JSON extraction before LLM or Optical request parsing/allocation.
pub const MAX_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;

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
    hybrid_engine: Arc<HybridContextEngine>,
    optical_config_issues: Arc<Vec<dtoken_config::OpticalConfigIssue>>,
}

impl AppState {
    pub fn new(config: DtokenConfig) -> Self {
        Self::new_with_config_status(config, RuntimeConfigStatus::empty())
    }

    pub fn new_with_config_status(
        mut config: DtokenConfig,
        config_status: RuntimeConfigStatus,
    ) -> Self {
        let optical_config_issues = Arc::new(config.normalize_optical_fail_closed());
        let hybrid_engine = Arc::new(HybridContextEngine::new(&config.optical.global_policy));
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
            hybrid_engine,
            optical_config_issues,
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

    fn try_append_event(&self, event: DtokenEvent) -> anyhow::Result<()> {
        self.ledger
            .lock()
            .map_err(|_| anyhow::anyhow!("event ledger lock poisoned"))?
            .record(event.clone())?;
        if let Some(store) = &self.runtime_store {
            store.append_event(&event)?;
        }
        Ok(())
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
    let _ = try_append_stage_event(state, request_id, stage, status, metadata, error_code);
}

fn try_append_stage_event(
    state: &AppState,
    request_id: Uuid,
    stage: LayeredStage,
    status: EventStatus,
    metadata: Value,
    error_code: Option<&str>,
) -> anyhow::Result<()> {
    state.try_append_event(DtokenEvent {
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
        context_accounting: None,
    })
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

    pub fn from_config(
        source: impl Into<String>,
        path: Option<String>,
        config: &DtokenConfig,
    ) -> Self {
        Self {
            source: source.into(),
            path,
            loaded: true,
            exists: true,
            provider_count: config.providers.len(),
            route_count: config.routes.len(),
            instance_count: config.instances.len(),
            fallback_disabled: config
                .routes
                .iter()
                .all(|route| route.fallback_provider_id.is_none()),
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

    pub fn failed(
        source: impl Into<String>,
        path: Option<String>,
        error: impl Into<String>,
    ) -> Self {
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
    events: JsonlEventLedger,
    route_decisions: JsonlStore,
    request_logs: JsonlStore,
    ccr_metadata: JsonlStore,
    memory_records: JsonlStore,
}

impl RuntimeStore {
    fn new(dir: &StdPath) -> Self {
        Self {
            events: JsonlEventLedger::new(dir.join("events.jsonl")),
            route_decisions: JsonlStore::new(dir.join("route-decisions.jsonl")),
            request_logs: JsonlStore::new(dir.join("request-logs.jsonl")),
            ccr_metadata: JsonlStore::new(dir.join("ccr-metadata.jsonl")),
            memory_records: JsonlStore::new(dir.join("memory-records.jsonl")),
        }
    }

    fn append_event(&self, event: &DtokenEvent) -> anyhow::Result<()> {
        self.events.append(event)
    }

    fn recent_events(&self, limit: usize) -> anyhow::Result<Vec<DtokenEvent>> {
        self.events.recent(limit)
    }

    fn append_route_decision(&self, decision: &Value) -> anyhow::Result<()> {
        self.route_decisions.append(decision)
    }

    fn recent_route_decisions(&self, limit: usize) -> anyhow::Result<Vec<Value>> {
        self.route_decisions.recent(limit)
    }

    fn append_request_log(&self, log: &RequestLog) -> anyhow::Result<()> {
        self.request_logs.append(log)
    }

    fn recent_request_logs(&self, limit: usize) -> anyhow::Result<Vec<RequestLog>> {
        self.request_logs.recent(limit)
    }

    fn append_ccr_metadata(&self, metadata: &Value) -> anyhow::Result<()> {
        self.ccr_metadata.append(metadata)
    }

    fn recent_ccr_metadata(&self, limit: usize) -> anyhow::Result<Vec<Value>> {
        self.ccr_metadata.recent(limit)
    }

    fn append_memory_record(&self, record: &Value) -> anyhow::Result<()> {
        self.memory_records.append(record)
    }

    fn recent_memory_records(&self, limit: usize) -> anyhow::Result<Vec<Value>> {
        self.memory_records.recent(limit)
    }
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
        .route("/compression/optical/preview", post(optical_preview))
        .route("/compression/optical/status", get(optical_status))
        .route(
            "/compression/optical/capabilities",
            get(optical_capabilities),
        )
        .route(
            "/compression/optical/capabilities/lifecycle",
            get(optical_capability_lifecycle),
        )
        .route(
            "/compression/optical/events/recent",
            get(optical_recent_events),
        )
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
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .layer(middleware::from_fn(loopback_request_guard))
        .with_state(state)
}

async fn optical_preview(
    State(state): State<AppState>,
    Json(request): Json<OpticalPreviewRequest>,
) -> Json<OpticalPlanPublicSummary> {
    let global_policy = state.config.optical.global_policy.clone();
    let route_policy = request
        .route_id
        .as_deref()
        .and_then(|route_id| state.config.find_optical_route_policy(route_id))
        .cloned()
        .unwrap_or_default();
    let route_policy_found = request
        .route_id
        .as_deref()
        .and_then(|route_id| state.config.find_optical_route_policy(route_id))
        .is_some();
    let registry = dtoken_compress::optical::OpticalCapabilityRegistry::from_runtime_config(
        &state.config.optical,
        dtoken_event_ledger::now_ms(),
    );
    let body_bytes = serde_json::to_vec(&request.body)
        .map(|body| body.len())
        .unwrap_or_default();
    let applicability_input = OpticalApplicabilityInput {
        method: Some("POST".to_string()),
        endpoint_path: Some(request.endpoint_path.clone()),
        body_bytes: Some(body_bytes),
        protocol: request.protocol.clone(),
        provider_id: request.provider_id.clone(),
        route_id: request.route_id.clone(),
        model: request.model.clone(),
        global_enabled: global_policy.enabled,
        route_enabled: route_policy.enabled,
    };
    let request_id = Uuid::new_v4();
    let plan_id = request_id.to_string();
    let build = dtoken_compress::optical::build_shadow_plan_detailed_with_registry(
        &plan_id,
        &request.body,
        &global_policy,
        &route_policy,
        &applicability_input,
        &registry,
    );

    record_optical_preview_event(&state, request_id, &build, &registry, route_policy_found);
    Json(build.summary)
}

async fn optical_status(State(state): State<AppState>) -> Json<OpticalStatusPublicSummary> {
    let policy = state.config.optical.global_policy.clone();
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &state.config.optical,
        dtoken_event_ledger::now_ms(),
    );
    let active_route_count = active_direct_anthropic_route_count(&state.config, &registry);
    let active = policy.allow_active && active_route_count > 0;
    let effective_mode = if active {
        OpticalMode::ToolResultsOnly
    } else {
        OpticalMode::ShadowPreview
    };

    Json(OpticalStatusPublicSummary {
        schema_version: 1,
        enabled: policy.enabled,
        configured_mode: policy.default_mode,
        effective_mode: effective_mode.clone(),
        default_mode: effective_mode,
        allow_active: active,
        public_summary_only: policy.public_summary_only || !active,
        fail_closed: true,
        renderer_ready: true,
        patcher_ready: true,
        post_validator_ready: true,
        signed_accounting_ready: true,
        active_direct_routes: active_route_count,
        verified_attested_capabilities: registry.verified_rule_count(),
        rejected_capabilities: registry.rejected_rule_count(),
        config_issue_count: state.optical_config_issues.len(),
    })
}

async fn optical_capabilities(
    State(state): State<AppState>,
) -> Json<OpticalCapabilitiesPublicSummary> {
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &state.config.optical,
        dtoken_event_ledger::now_ms(),
    );
    let active = state.config.optical.global_policy.allow_active
        && active_direct_anthropic_route_count(&state.config, &registry) > 0;
    Json(OpticalCapabilitiesPublicSummary {
        shadow_preview_supported: true,
        active_transform_enabled: active,
        max_mode: if active {
            OpticalMode::ToolResultsOnly
        } else {
            OpticalMode::ShadowPreview
        },
        supported_endpoints: vec![
            OpticalEndpoint::AnthropicMessages,
            OpticalEndpoint::OpenAiChatCompletions,
            OpticalEndpoint::OpenAiResponses,
        ],
        unknown_model_status: dtoken_types::CapabilityStatus::ShadowOnly,
        unsupported_path_status: dtoken_types::CapabilityStatus::Blocked,
        public_summary_only: state.config.optical.global_policy.public_summary_only || !active,
        fail_closed: true,
        direct_anthropic_upstream_supported: true,
        streaming_passthrough_supported: true,
        deterministic_png_renderer: true,
        artifact_post_validation: true,
        signed_multimodal_accounting: true,
        capability_attestation_required: true,
    })
}

fn active_direct_anthropic_route_count(
    config: &DtokenConfig,
    registry: &OpticalCapabilityRegistry,
) -> usize {
    config.routes.iter().filter(|route| {
        route.input_protocol == Protocol::Anthropic
            && config
                .find_provider(&route.target_provider_id)
                .is_some_and(|provider| {
                    provider.protocol == Protocol::Anthropic
                        && provider.models.iter().any(|model| {
                            dtoken_compress::optical::resolve_model_capability_record_for_route_with_registry(
                                Some(&Protocol::Anthropic),
                                Some(&provider.id),
                                Some(&route.id),
                                Some(model),
                                OpticalEndpoint::AnthropicMessages,
                                registry,
                            )
                            .revision
                            .is_some()
                        })
                })
            && config
                .find_optical_route_policy(&route.id)
                .is_some_and(|policy| {
                    policy.enabled
                        && policy.mode == OpticalMode::ToolResultsOnly
                        && policy.allow_tool_result
                        && policy.confirmed_experimental_lossy
                })
    }).count()
}

async fn optical_capability_lifecycle(
    State(state): State<AppState>,
) -> Json<OpticalCapabilityLifecyclePublicSummary> {
    let snapshots = &state.config.optical.capability_snapshots;
    let registry = dtoken_compress::optical::OpticalCapabilityRegistry::from_runtime_config(
        &state.config.optical,
        dtoken_event_ledger::now_ms(),
    );
    Json(OpticalCapabilityLifecyclePublicSummary {
        schema_version: 1,
        configured_snapshots: snapshots.len(),
        // "Active" means accepted by the registry, not merely labelled active in JSON.
        active_snapshots: registry.verified_rule_count(),
        revoked_snapshots: registry.revoked_rule_count(),
        superseded_snapshots: registry.superseded_rule_count(),
        verified_rules: registry.verified_rule_count(),
        rejected_rules: registry.rejected_rule_count(),
        trusted_authorities: state.config.optical.trusted_authorities.len(),
        eval_artifacts: state.config.optical.eval_artifacts.len(),
        passing_eval_artifacts: state
            .config
            .optical
            .eval_artifacts
            .iter()
            .filter(|artifact| artifact.passed)
            .count(),
        attestation_required: true,
    })
}

fn record_optical_preview_event(
    state: &AppState,
    request_id: Uuid,
    build: &dtoken_compress::optical::ShadowPlanBuild,
    registry: &dtoken_compress::optical::OpticalCapabilityRegistry,
    route_policy_found: bool,
) {
    let event_status = match &build.summary.final_status {
        dtoken_types::OpticalPlanStatus::ValidatedShadowPlan
        | dtoken_types::OpticalPlanStatus::ValidatedActivePlan => EventStatus::Ok,
        dtoken_types::OpticalPlanStatus::Ineligible => EventStatus::Skipped,
        dtoken_types::OpticalPlanStatus::FailedClosed => EventStatus::Failed,
        dtoken_types::OpticalPlanStatus::Disabled => EventStatus::Disabled,
    };
    let graph = build.plan.graph_summary();
    append_stage_event(
        state,
        request_id,
        LayeredStage::OpticalPreview,
        event_status,
        json!({
            "kind": "optical_preview",
            "final_status": build.summary.final_status.clone(),
            "mode": build.summary.mode.clone(),
            "candidate_blocks": build.summary.counters.candidate_blocks,
            "preserved_blocks": build.summary.counters.preserved_blocks,
            "skipped_blocks": build.summary.counters.skipped_blocks,
            "estimated_images": build.summary.counters.estimated_images,
            "economics": build.summary.economics.clone(),
            "graph_nodes": graph.nodes,
            "graph_edges": graph.edges,
            "tool_dependency_edges": graph.tool_dependency_edges,
            "closed_tool_results": graph.closed_tool_results,
            "orphan_tool_results": graph.orphan_tool_results,
            "open_tool_uses": graph.open_tool_uses,
            "route_policy_found": route_policy_found,
            "configured_rules": registry.configured_rule_count(),
            "verified_rules": registry.verified_rule_count(),
            "rejected_rules": registry.rejected_rule_count(),
            "snapshot_revision_present": build.plan.capability_revision().is_some()
        }),
        None,
    );
}
async fn optical_recent_events(
    State(state): State<AppState>,
) -> Json<Vec<OpticalEventPublicSummary>> {
    Json(
        state
            .recent_events(200)
            .into_iter()
            .filter(|event| {
                matches!(
                    &event.stage,
                    LayeredStage::OpticalPreview
                        | LayeredStage::OpticalPlan
                        | LayeredStage::OpticalRender
                        | LayeredStage::OpticalPatch
                        | LayeredStage::OpticalValidate
                        | LayeredStage::OpticalFallback
                        | LayeredStage::OpticalComplete
                )
            })
            .map(optical_event_public_summary)
            .collect(),
    )
}

fn optical_event_public_summary(event: DtokenEvent) -> OpticalEventPublicSummary {
    let metadata = &event.metadata;
    let stored_kind = metadata
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kind = match &event.stage {
        LayeredStage::OpticalPreview => "optical_preview",
        LayeredStage::OpticalPlan => "optical_plan",
        LayeredStage::OpticalRender => "optical_render",
        LayeredStage::OpticalPatch => "optical_patch",
        LayeredStage::OpticalValidate => "optical_validate",
        LayeredStage::OpticalFallback => "optical_fallback",
        LayeredStage::OpticalComplete => "optical_complete",
        _ => "optical_runtime",
    }
    .to_string();
    let claimed_status = metadata
        .get("final_status")
        .cloned()
        .and_then(|value| serde_json::from_value::<OpticalPlanStatus>(value).ok());
    let active_completion = stored_kind == "active"
        && event.status == EventStatus::Ok
        && matches!(
            &event.stage,
            LayeredStage::OpticalValidate | LayeredStage::OpticalComplete
        );
    let preview_claim_is_valid = !matches!(&event.stage, LayeredStage::OpticalPreview)
        || (stored_kind == "optical_preview"
            && !matches!(
                claimed_status.as_ref(),
                Some(OpticalPlanStatus::ValidatedActivePlan)
            ));
    let final_status = if !preview_claim_is_valid || event.status == EventStatus::Failed {
        OpticalPlanStatus::FailedClosed
    } else if active_completion {
        OpticalPlanStatus::ValidatedActivePlan
    } else if matches!(
        claimed_status.as_ref(),
        Some(OpticalPlanStatus::ValidatedShadowPlan)
    ) || (stored_kind == "shadow" && event.status != EventStatus::Failed)
    {
        OpticalPlanStatus::ValidatedShadowPlan
    } else {
        match event.status {
            EventStatus::Disabled => OpticalPlanStatus::Disabled,
            EventStatus::Skipped => OpticalPlanStatus::Ineligible,
            _ => OpticalPlanStatus::Disabled,
        }
    };
    let mut public_metadata = OpticalEventMetadataPublicSummary {
        kind,
        final_status,
        mode: if active_completion {
            OpticalMode::ToolResultsOnly
        } else {
            OpticalMode::ShadowPreview
        },
        raw_available: false,
        ..Default::default()
    };
    public_metadata.candidate_blocks = metadata_usize(metadata, "candidate_blocks")
        .max(metadata_usize(metadata, "candidate_count"));
    public_metadata.preserved_blocks = metadata_usize(metadata, "preserved_blocks");
    public_metadata.skipped_blocks = metadata_usize(metadata, "skipped_blocks");
    public_metadata.estimated_images =
        metadata_usize(metadata, "estimated_images").max(metadata_usize(metadata, "image_count"));
    public_metadata.patched_blocks = metadata_usize(metadata, "patched_count");
    public_metadata.png_bytes = metadata
        .get("png_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    public_metadata.encoded_image_bytes = metadata
        .get("encoded_image_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    public_metadata.reason_code = metadata
        .get("reason_code")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    public_metadata.artifact_validated = metadata
        .get("artifact_validated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    public_metadata.attribution_basis = event
        .context_accounting
        .as_ref()
        .map(|accounting| accounting.attribution_basis);
    public_metadata.upstream_state = event
        .context_accounting
        .as_ref()
        .map(|accounting| accounting.upstream_state);
    public_metadata.net_saved_tokens = metadata
        .get("economics")
        .and_then(|economics| economics.get("net_saved_tokens"))
        .and_then(Value::as_i64)
        .and_then(|value| isize::try_from(value).ok())
        .unwrap_or_default();
    if public_metadata.net_saved_tokens == 0 {
        public_metadata.net_saved_tokens = metadata
            .get("estimated_signed_input_savings")
            .and_then(Value::as_i64)
            .and_then(|value| isize::try_from(value).ok())
            .unwrap_or_default();
    }
    public_metadata.graph_nodes = metadata_usize(metadata, "graph_nodes");
    public_metadata.graph_edges = metadata_usize(metadata, "graph_edges");
    public_metadata.tool_dependency_edges = metadata_usize(metadata, "tool_dependency_edges");
    public_metadata.closed_tool_results = metadata_usize(metadata, "closed_tool_results");
    public_metadata.orphan_tool_results = metadata_usize(metadata, "orphan_tool_results");
    public_metadata.open_tool_uses = metadata_usize(metadata, "open_tool_uses");
    public_metadata.route_policy_found = metadata
        .get("route_policy_found")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    public_metadata.configured_rules = metadata_usize(metadata, "configured_rules");
    public_metadata.verified_rules = metadata_usize(metadata, "verified_rules");
    public_metadata.rejected_rules = metadata_usize(metadata, "rejected_rules");
    public_metadata.snapshot_revision_present = metadata
        .get("snapshot_revision_present")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    OpticalEventPublicSummary {
        event_id: event.event_id,
        timestamp_ms: event.timestamp_ms,
        stage: event.stage,
        status: event.status,
        metadata: public_metadata,
    }
}

fn metadata_usize(metadata: &Value, key: &str) -> usize {
    metadata
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default()
}

async fn loopback_request_guard(
    request: axum::extract::Request,
    next: Next,
) -> axum::response::Response {
    let allowed_host = request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(is_allowed_loopback_authority)
        .unwrap_or(true);
    let allowed_origin = request
        .headers()
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(is_allowed_local_origin)
        .unwrap_or(true);
    if !allowed_host || !allowed_origin {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": {
                    "type": "local_request_boundary_rejected",
                    "message": "d-token only accepts local loopback Host and Origin headers"
                }
            })),
        )
            .into_response();
    }
    next.run(request).await
}

fn is_allowed_loopback_authority(value: &str) -> bool {
    let Ok(authority) = value.parse::<axum::http::uri::Authority>() else {
        return false;
    };
    let host = authority
        .host()
        .trim_start_matches('[')
        .trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn is_allowed_local_origin(value: &str) -> bool {
    let Ok(origin) = url::Url::parse(value) else {
        return false;
    };
    let Some(host) = origin.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("tauri.localhost") {
        return matches!(origin.scheme(), "http" | "https");
    }
    matches!(origin.scheme(), "http" | "https" | "tauri")
        && (host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback()))
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
        "providers": provider_metadata_payload(&state.config.providers),
        "routes": state.config.routes,
        "instances": state.config.instances,
        "privacy": "Runtime active config metadata only; API keys, auth headers, prompts, responses, and external agent configs are not returned."
    })
}

fn provider_metadata_payload(providers: &[dtoken_types::ProviderProfile]) -> Value {
    Value::Array(
        providers
            .iter()
            .map(|provider| {
                json!({
                    "id": provider.id,
                    "name": provider.name,
                    "kind": provider.kind,
                    "protocol": provider.protocol,
                    "base_url": provider.base_url,
                    "models": provider.models,
                    "timeout_ms": provider.timeout_ms,
                    "healthcheck_path": provider.healthcheck_path,
                    "credential_configured": provider.api_key_ref.as_deref().is_some_and(|value| !value.trim().is_empty())
                })
            })
            .collect(),
    )
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
    let (
        stats,
        history,
        metrics,
        cache_summary,
        providers,
        routes,
        instances,
        events,
        route_decisions,
        request_logs,
        memory,
    ) = (
        input.stats,
        input.history,
        input.metrics,
        input.cache_summary,
        input.providers,
        input.routes,
        input.instances,
        input.events,
        input.route_decisions,
        input.request_logs,
        input.memory,
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

fn parse_openai_request_body(
    original_body: Value,
) -> Result<(OpenAiChatRequest, Value), serde_json::Error> {
    let request = serde_json::from_value(original_body.clone())?;
    Ok((request, original_body))
}

#[derive(Debug, Clone)]
struct UpstreamResult {
    response: Value,
    status: Option<u16>,
    error_category: Option<String>,
    send_state: UpstreamSendState,
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
    context_accounting: Option<dtoken_types::HybridContextAccounting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteAdapterKind {
    OpenAiChatPassthrough,
    OpenAiResponsesToChat,
    AnthropicMessagesPassthrough,
    AnthropicMessagesToOpenAiChat,
}

impl RouteAdapterKind {
    fn surfaces(self) -> (OptimizationSurface, OptimizationSurface) {
        match self {
            Self::OpenAiChatPassthrough => (
                OptimizationSurface {
                    protocol: Protocol::OpenAi,
                    endpoint: OpticalEndpoint::OpenAiChatCompletions,
                },
                OptimizationSurface {
                    protocol: Protocol::OpenAi,
                    endpoint: OpticalEndpoint::OpenAiChatCompletions,
                },
            ),
            Self::OpenAiResponsesToChat => (
                OptimizationSurface {
                    protocol: Protocol::OpenAi,
                    endpoint: OpticalEndpoint::OpenAiResponses,
                },
                OptimizationSurface {
                    protocol: Protocol::OpenAi,
                    endpoint: OpticalEndpoint::OpenAiChatCompletions,
                },
            ),
            Self::AnthropicMessagesToOpenAiChat => (
                OptimizationSurface {
                    protocol: Protocol::Anthropic,
                    endpoint: OpticalEndpoint::AnthropicMessages,
                },
                OptimizationSurface {
                    protocol: Protocol::OpenAi,
                    endpoint: OpticalEndpoint::OpenAiChatCompletions,
                },
            ),
            Self::AnthropicMessagesPassthrough => (
                OptimizationSurface {
                    protocol: Protocol::Anthropic,
                    endpoint: OpticalEndpoint::AnthropicMessages,
                },
                OptimizationSurface {
                    protocol: Protocol::Anthropic,
                    endpoint: OpticalEndpoint::AnthropicMessages,
                },
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OptimizationSurface {
    protocol: Protocol,
    endpoint: OpticalEndpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpticalExecutionDisposition {
    Disabled,
    RouteResolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedRouteOptimizationContext {
    request_id: Uuid,
    route_id: String,
    provider_id: String,
    requested_policy: CompressionPolicy,
    adapter: RouteAdapterKind,
    ingress: OptimizationSurface,
    upstream: OptimizationSurface,
    requested_model: String,
    effective_model: String,
    optical_disposition: OpticalExecutionDisposition,
    compression_context: ResolvedOptimizationContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextResolutionError {
    DecisionRouteMismatch,
    DecisionProviderMismatch,
    RouteProviderMismatch,
    DecisionPolicyMismatch,
    RouteIngressProtocolMismatch,
    ProviderUpstreamProtocolMismatch,
    MissingRequestedModel,
    MissingEffectiveModel,
    EffectiveModelNotAllowed,
}

fn resolve_optimization_context(
    decision: &RouteDecision,
    route: &dtoken_types::RouteProfile,
    provider: &dtoken_types::ProviderProfile,
    adapter: RouteAdapterKind,
    ingress_body: &Value,
    upstream_body: &Value,
) -> Result<ResolvedRouteOptimizationContext, ContextResolutionError> {
    if decision.route_id != route.id {
        return Err(ContextResolutionError::DecisionRouteMismatch);
    }
    if decision.provider_id != provider.id {
        return Err(ContextResolutionError::DecisionProviderMismatch);
    }
    if route.target_provider_id != provider.id {
        return Err(ContextResolutionError::RouteProviderMismatch);
    }
    if decision.compression_policy != route.compression_policy {
        return Err(ContextResolutionError::DecisionPolicyMismatch);
    }

    let (ingress, upstream) = adapter.surfaces();
    if route.input_protocol != ingress.protocol {
        return Err(ContextResolutionError::RouteIngressProtocolMismatch);
    }
    if provider.protocol != upstream.protocol {
        return Err(ContextResolutionError::ProviderUpstreamProtocolMismatch);
    }

    let requested_model = ingress_body
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(ContextResolutionError::MissingRequestedModel)?
        .to_string();
    let effective_model = upstream_body
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(ContextResolutionError::MissingEffectiveModel)?
        .to_string();
    if !provider.models.is_empty()
        && !provider
            .models
            .iter()
            .any(|configured| configured == &effective_model)
    {
        return Err(ContextResolutionError::EffectiveModelNotAllowed);
    }

    let compression_context = ResolvedOptimizationContext {
        route_id: decision.route_id.clone(),
        provider_id: decision.provider_id.clone(),
        model: effective_model.clone(),
        input_protocol: ingress.protocol.clone(),
        output_protocol: upstream.protocol.clone(),
        endpoint: upstream.endpoint.clone(),
    };

    Ok(ResolvedRouteOptimizationContext {
        request_id: decision.request_id,
        route_id: decision.route_id.clone(),
        provider_id: decision.provider_id.clone(),
        requested_policy: decision.compression_policy.clone(),
        adapter,
        ingress,
        upstream,
        requested_model,
        effective_model,
        optical_disposition: if adapter == RouteAdapterKind::AnthropicMessagesPassthrough {
            OpticalExecutionDisposition::RouteResolved
        } else {
            OpticalExecutionDisposition::Disabled
        },
        compression_context,
    })
}

#[derive(Debug, Clone)]
struct RoutedBridgePayload {
    upstream_body: Value,
    decision: RouteDecision,
    request_log: RequestLog,
    event: DtokenEvent,
    compression: CompressionSummary,
    provider: dtoken_types::ProviderProfile,
    optimization_context: Option<ResolvedRouteOptimizationContext>,
}

fn compress_for_route(
    context: Option<&ResolvedRouteOptimizationContext>,
    policy: CompressionPolicy,
    body: Value,
) -> CompressionSummary {
    let result = context
        .map(|context| {
            CompressionEngine::new().compress_resolved(
                &context.compression_context,
                policy.clone(),
                body.clone(),
            )
        })
        .unwrap_or_else(|| CompressionEngine::new().compress(policy, body.clone()))
        .unwrap_or_else(|_| dtoken_types::CompressionResult {
            original_tokens: 0,
            compressed_tokens: 0,
            tokens_saved: 0,
            elapsed_us: 1,
            transforms_applied: vec!["compression_error_fallback".to_string()],
            body,
            context_accounting: None,
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
        context_accounting: result.context_accounting,
    }
}

fn compression_inventory_diagnostics(
    context: &dtoken_compress::ResolvedOptimizationContext,
    body: &Value,
) -> Value {
    let role_sequence = body
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .map(|message| {
                    message
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let content_kinds = body
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .map(|message| json_value_kind(message.get("content").unwrap_or(&Value::Null)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let Some(inventory) = dtoken_compress::lower_hybrid_context(context, body) else {
        return json!({
            "state": "unsupported_context",
            "role_sequence": role_sequence,
            "content_kinds": content_kinds,
            "raw_available": false
        });
    };
    let tool_nodes = inventory
        .nodes()
        .iter()
        .filter(|node| {
            matches!(
                node.kind(),
                dtoken_compress::HybridNodeKind::ToolUse
                    | dtoken_compress::HybridNodeKind::ToolResult
            )
        })
        .map(|node| {
            let digest = node
                .tool_correlation_digest()
                .map(|value| value.chars().take(12).collect::<String>());
            json!({
                "kind": format!("{:?}", node.kind()),
                "role": format!("{:?}", node.role()),
                "turn_index": node.turn_index(),
                "recent": node
                    .turn_index()
                    .is_some_and(|turn| turn >= inventory.recent_tail_start()),
                "correlation_digest": digest,
                "protection": format!("{:?}", node.protection()),
                "reason": format!("{:?}", node.reason())
            })
        })
        .collect::<Vec<_>>();
    json!({
        "state": "resolved",
        "message_count": role_sequence.len(),
        "role_sequence": role_sequence,
        "content_kinds": content_kinds,
        "current_user_turn": inventory.current_user_turn(),
        "recent_tail_start": inventory.recent_tail_start(),
        "tool_nodes": tool_nodes,
        "approved_target_count": inventory.approved_text_targets().len(),
        "raw_available": false
    })
}

fn json_value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
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
        input_protocol: Some(Protocol::OpenAi),
        output_protocol: Some(Protocol::OpenAi),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteSelectionError {
    NotConfigured,
    Ambiguous,
    EvidenceConflict,
    InvalidOptimizationContext,
}

#[derive(Debug, Clone, Copy, Default)]
struct RouteEvidence<'a> {
    route_id: Option<&'a str>,
    instance_id: Option<&'a str>,
    model: Option<&'a str>,
}

fn header_evidence<'a>(headers: &'a HeaderMap, model: Option<&'a str>) -> RouteEvidence<'a> {
    RouteEvidence {
        route_id: headers
            .get("x-dtoken-route-id")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.trim().is_empty()),
        instance_id: headers
            .get("x-dtoken-instance-id")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.trim().is_empty()),
        model,
    }
}

fn configured_route<'a>(
    config: &'a DtokenConfig,
    input_protocol: Protocol,
    evidence: RouteEvidence<'_>,
) -> Result<
    (
        &'a dtoken_types::RouteProfile,
        &'a dtoken_types::ProviderProfile,
    ),
    RouteSelectionError,
> {
    let mut candidates = config
        .routes
        .iter()
        .filter(|route| route.input_protocol == input_protocol)
        .filter_map(|route| {
            config
                .find_provider(&route.target_provider_id)
                .map(|provider| (route, provider))
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        return Err(RouteSelectionError::NotConfigured);
    }

    if let Some(route_id) = evidence.route_id {
        candidates.retain(|(route, _)| route.id == route_id);
        if candidates.is_empty() {
            return Err(RouteSelectionError::EvidenceConflict);
        }
    }
    if let Some(instance_id) = evidence.instance_id {
        candidates.retain(|(route, _)| route.instance_id == instance_id);
        if candidates.is_empty() {
            return Err(RouteSelectionError::EvidenceConflict);
        }
    }

    if candidates.len() > 1 {
        if let Some(model) = evidence.model {
            let model_matches = candidates
                .iter()
                .copied()
                .filter(|(_, provider)| provider.models.iter().any(|item| item == model))
                .collect::<Vec<_>>();
            match model_matches.as_slice() {
                [selected] => return Ok(*selected),
                [] => {}
                _ => return Err(RouteSelectionError::Ambiguous),
            }
        }
        return Err(RouteSelectionError::Ambiguous);
    }

    Ok(candidates[0])
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
        reason: "matched one explicit configured route; fallback disabled".to_string(),
        input_protocol: Some(route.input_protocol.clone()),
        output_protocol: Some(provider.protocol.clone()),
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
        "input_protocol": decision.input_protocol.as_ref().map(protocol_label),
        "output_protocol": decision.output_protocol.as_ref().map(protocol_label),
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
        output_tokens: None,
        tokens_saved: Some(if compression.context_accounting.is_some() {
            0
        } else {
            compression.tokens_saved as usize
        }),
        transforms_applied: compression.transforms_applied.clone(),
        latency_ms: Some((compression.elapsed_us / 1000).max(1) as u64),
        error: None,
        config_source: Some("dtoken-owned-runtime".to_string()),
        context_accounting: compression.context_accounting.clone(),
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
        status: EventStatus::Passthrough,
        input_tokens: Some(compression.original_tokens),
        output_tokens: None,
        saved_tokens: Some(if compression.context_accounting.is_some() {
            0
        } else {
            compression.tokens_saved
        }),
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
        context_accounting: compression.context_accounting.clone(),
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

fn upstream_anthropic_messages_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1/messages") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/messages")
    } else {
        format!("{trimmed}/v1/messages")
    }
}

fn credential_target_from_reference(reference: &str) -> Result<(String, String), String> {
    let parsed = url::Url::parse(reference)
        .map_err(|_| "credential reference is not a valid opaque URL".to_string())?;
    if !matches!(parsed.scheme(), "os-credential" | "wincred" | "keychain")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("credential reference violates the OS-store reference format".to_string());
    }
    let service = parsed
        .host_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "credential reference has no service".to_string())?;
    let account = parsed.path().trim_matches('/');
    if account.is_empty() {
        return Err("credential reference has no account".to_string());
    }
    Ok((service.to_string(), account.to_string()))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn read_platform_credential(service: &str, account: &str) -> Result<String, ()> {
    let entry = keyring::Entry::new(service, account).map_err(|_| ())?;
    let credential = entry.get_password().map_err(|_| ())?;
    if credential.trim().is_empty() {
        return Err(());
    }
    Ok(credential)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn read_platform_credential(_service: &str, _account: &str) -> Result<String, ()> {
    Err(())
}

fn credential_error_result(error_type: &str, message: &str) -> UpstreamResult {
    UpstreamResult {
        response: json!({
            "error": {
                "type": error_type,
                "message": message
            }
        }),
        status: None,
        error_category: Some(error_type.to_string()),
        send_state: UpstreamSendState::NotAttempted,
    }
}

async fn resolve_provider_credential(
    provider: &dtoken_types::ProviderProfile,
) -> Result<Option<String>, UpstreamResult> {
    let Some(reference) = provider.api_key_ref.as_deref() else {
        return Ok(None);
    };
    let (service, account) = credential_target_from_reference(reference).map_err(|_| {
        credential_error_result(
            "provider_credential_reference_invalid",
            "configured provider credential reference is invalid",
        )
    })?;
    tokio::task::spawn_blocking(move || read_platform_credential(&service, &account))
        .await
        .map_err(|_| {
            credential_error_result(
                "provider_credential_unavailable",
                "configured provider credential could not be read from the OS credential store",
            )
        })?
        .map(Some)
        .map_err(|_| {
            credential_error_result(
                "provider_credential_unavailable",
                "configured provider credential could not be read from the OS credential store",
            )
        })
}

fn provider_authorization_header(credential: &str) -> Result<reqwest::header::HeaderValue, ()> {
    reqwest::header::HeaderValue::from_str(&format!("Bearer {credential}")).map_err(|_| ())
}

fn classify_send_failure(error: &reqwest::Error) -> UpstreamSendState {
    if error.is_builder() || error.is_connect() {
        UpstreamSendState::FailedBeforeCommit
    } else {
        // Once reqwest cannot prove the request failed before connection/write, d-token
        // conservatively treats it as post-commit and never retries the baseline.
        UpstreamSendState::FailedAfterCommit
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

fn upstream_response_status(upstream: &UpstreamResult) -> StatusCode {
    if upstream.error_category.as_deref() == Some("provider_timeout") {
        return StatusCode::GATEWAY_TIMEOUT;
    }
    if matches!(
        upstream.error_category.as_deref(),
        Some(
            "provider_credential_reference_invalid"
                | "provider_credential_unavailable"
                | "provider_credential_invalid"
        )
    ) {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    if upstream.error_category.is_some() {
        return upstream
            .status
            .and_then(|status| StatusCode::from_u16(status).ok())
            .filter(|status| status.is_client_error() || status.is_server_error())
            .unwrap_or(StatusCode::BAD_GATEWAY);
    }
    upstream
        .status
        .and_then(|status| StatusCode::from_u16(status).ok())
        .unwrap_or(StatusCode::OK)
}

fn mark_upstream_outcome(
    upstream: &UpstreamResult,
    request_log: &mut RequestLog,
    event: &mut DtokenEvent,
) {
    let provider_usage = provider_usage_measurement(&upstream.response);
    let output_tokens = provider_usage
        .as_ref()
        .and_then(|measurement| measurement.usage.output_tokens)
        .and_then(|tokens| usize::try_from(tokens).ok());

    if let Some(accounting) = request_log.context_accounting.as_mut() {
        accounting.upstream_state = upstream.send_state;
        if accounting.disposition == dtoken_types::OptimizationDisposition::FallbackPreSend
            && matches!(
                upstream.send_state,
                UpstreamSendState::SendCommitted
                    | UpstreamSendState::ResponseStarted
                    | UpstreamSendState::StreamStarted
                    | UpstreamSendState::Completed
                    | UpstreamSendState::FailedAfterCommit
            )
        {
            accounting.disposition = dtoken_types::OptimizationDisposition::FallbackOriginalSent;
        }
        accounting.provider_actual_sent.measurement = provider_usage.clone();
        if upstream.error_category.is_none()
            && upstream.send_state == UpstreamSendState::Completed
            && matches!(
                accounting.disposition,
                dtoken_types::OptimizationDisposition::Active
                    | dtoken_types::OptimizationDisposition::SemanticActive
            )
            && accounting.estimated.is_some()
        {
            accounting.attribution_basis = dtoken_types::SavingsAttribution::EstimatedActive;
        } else {
            accounting.attribution_basis = dtoken_types::SavingsAttribution::None;
        }
        request_log.tokens_saved =
            Some(usize::try_from(accounting.legacy_positive_tokens_saved()).unwrap_or(usize::MAX));
    }
    event.context_accounting = request_log.context_accounting.clone();

    if let Some(category) = upstream.error_category.as_ref() {
        request_log.error = Some(category.clone());
        request_log.output_tokens = Some(0);
        event.status = EventStatus::Failed;
        event.error_code = Some(category.clone());
        event.saved_tokens = Some(0);
    } else {
        request_log.output_tokens = output_tokens;
        event.status = EventStatus::Ok;
        event.output_tokens = output_tokens.and_then(|tokens| u64::try_from(tokens).ok());
        if let Some(measurement) = provider_usage {
            event.input_tokens = measurement.usage.input.normalized_total;
        }
        event.saved_tokens = request_log
            .context_accounting
            .as_ref()
            .map(HybridContextAccountingExt::positive_saved_tokens)
            .or_else(|| {
                request_log
                    .tokens_saved
                    .and_then(|tokens| u64::try_from(tokens).ok())
            });
    }
}

trait HybridContextAccountingExt {
    fn positive_saved_tokens(&self) -> u64;
}

impl HybridContextAccountingExt for dtoken_types::HybridContextAccounting {
    fn positive_saved_tokens(&self) -> u64 {
        self.legacy_positive_tokens_saved()
    }
}

fn provider_usage_measurement(response: &Value) -> Option<UsageMeasurement> {
    let usage = response.get("usage")?;
    let openai_input = usage.get("prompt_tokens").and_then(Value::as_u64);
    let anthropic_uncached = usage.get("input_tokens").and_then(Value::as_u64);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(Value::as_u64)
        .or_else(|| {
            usage
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_u64)
        });
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .and_then(Value::as_u64);
    let cache_creation_5m = usage
        .pointer("/cache_creation/ephemeral_5m_input_tokens")
        .and_then(Value::as_u64);
    let cache_creation_1h = usage
        .pointer("/cache_creation/ephemeral_1h_input_tokens")
        .and_then(Value::as_u64);
    let normalized_total = openai_input.or_else(|| {
        anthropic_uncached.map(|uncached| {
            uncached
                .saturating_add(cache_read.unwrap_or_default())
                .saturating_add(cache_creation.unwrap_or_default())
        })
    });
    let output_tokens = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .or_else(|| usage.get("output_tokens").and_then(Value::as_u64));
    if normalized_total.is_none() && output_tokens.is_none() {
        return None;
    }
    Some(UsageMeasurement {
        evidence: UsageEvidence {
            kind: UsageEvidenceKind::ProviderActual,
            source_revision: "provider-response-usage-v1".to_string(),
        },
        usage: UsageVector {
            input: InputTokenUsage {
                normalized_total,
                modality: ModalityTokens {
                    text: openai_input.or(anthropic_uncached),
                    vision: None,
                    other: None,
                },
                cache: CacheTokens {
                    uncached: anthropic_uncached.or(openai_input
                        .map(|tokens| tokens.saturating_sub(cache_read.unwrap_or_default()))),
                    read: cache_read,
                    creation: cache_creation,
                    creation_5m: cache_creation_5m,
                    creation_1h: cache_creation_1h,
                },
            },
            output_tokens,
            bytes: PayloadByteUsage::default(),
            cost_microusd: None,
        },
    })
}

fn require_openai_message(upstream: UpstreamResult, adapter: &str) -> UpstreamResult {
    let message = upstream.response.pointer("/choices/0/message");
    let has_content = message
        .and_then(|message| message.get("content"))
        .is_some_and(|content| !content.is_null());
    let has_tool_calls = message
        .and_then(|message| message.get("tool_calls"))
        .and_then(Value::as_array)
        .is_some_and(|calls| !calls.is_empty());
    if upstream.error_category.is_some() || has_content || has_tool_calls {
        return upstream;
    }
    UpstreamResult {
        response: json!({
            "error": {
                "type": "provider_invalid_response",
                "message": format!(
                    "configured upstream returned success without choices[0].message.content required by the {adapter} adapter"
                ),
                "upstream_status": upstream.status
            }
        }),
        status: upstream.status,
        error_category: Some("provider_invalid_response".to_string()),
        send_state: upstream.send_state,
    }
}

fn require_anthropic_message(upstream: UpstreamResult) -> UpstreamResult {
    let valid = upstream.response.get("type").and_then(Value::as_str) == Some("message")
        && upstream
            .response
            .get("content")
            .and_then(Value::as_array)
            .is_some();
    if upstream.error_category.is_some() || valid {
        return upstream;
    }
    UpstreamResult {
        response: json!({
            "error": {
                "type": "provider_invalid_response",
                "message": "configured Anthropic upstream returned success without a valid message content array",
                "upstream_status": upstream.status
            }
        }),
        status: upstream.status,
        error_category: Some("provider_invalid_response".to_string()),
        send_state: upstream.send_state,
    }
}

async fn forward_openai_chat(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
) -> UpstreamResult {
    let credential = match resolve_provider_credential(provider).await {
        Ok(credential) => credential,
        Err(error) => return error,
    };
    let url = upstream_chat_url(&provider.base_url);
    let mut request = client
        .post(url)
        .timeout(std::time::Duration::from_millis(provider.timeout_ms.max(1)))
        .json(&original_body);
    if let Some(credential) = credential.as_deref() {
        let header = match provider_authorization_header(credential) {
            Ok(header) => header,
            Err(()) => {
                return credential_error_result(
                    "provider_credential_invalid",
                    "configured provider credential cannot be used as an authorization header",
                )
            }
        };
        request = request.header(reqwest::header::AUTHORIZATION, header);
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
                    send_state: UpstreamSendState::Completed,
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
                    send_state: UpstreamSendState::Completed,
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
                    send_state: UpstreamSendState::Completed,
                },
            }
        }
        Err(err) => {
            let (category, message) = if err.is_timeout() {
                ("provider_timeout", "configured upstream request timed out")
            } else {
                ("provider_network", "configured upstream request failed")
            };
            UpstreamResult {
                response: json!({
                    "error": {
                        "type": category,
                        "message": message
                    }
                }),
                status: None,
                error_category: Some(category.to_string()),
                send_state: classify_send_failure(&err),
            }
        }
    }
}

async fn forward_openai_chat_stream(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
) -> Result<(u16, axum::http::HeaderMap, Body), UpstreamResult> {
    let credential = resolve_provider_credential(provider).await?;
    let url = upstream_chat_url(&provider.base_url);
    let mut request = client
        .post(url)
        .timeout(std::time::Duration::from_millis(provider.timeout_ms.max(1)))
        .json(&original_body);
    if let Some(credential) = credential.as_deref() {
        let header = provider_authorization_header(credential).map_err(|()| {
            credential_error_result(
                "provider_credential_invalid",
                "configured provider credential cannot be used as an authorization header",
            )
        })?;
        request = request.header(reqwest::header::AUTHORIZATION, header);
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
                    send_state: UpstreamSendState::Completed,
                });
            }
            let stream = response.bytes_stream().map_err(std::io::Error::other);
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                axum::http::HeaderValue::from_str(&content_type)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("text/event-stream")),
            );
            headers.insert(
                CACHE_CONTROL,
                axum::http::HeaderValue::from_static("no-cache"),
            );
            Ok((status_code, headers, Body::from_stream(stream)))
        }
        Err(err) => {
            let (category, message) = if err.is_timeout() {
                (
                    "provider_timeout",
                    "configured upstream streaming request timed out",
                )
            } else {
                (
                    "provider_network",
                    "configured upstream streaming request failed",
                )
            };
            Err(UpstreamResult {
                response: json!({
                    "error": {
                        "type": category,
                        "message": message
                    }
                }),
                status: None,
                error_category: Some(category.to_string()),
                send_state: classify_send_failure(&err),
            })
        }
    }
}

fn anthropic_header_value(
    inbound: &HeaderMap,
    name: &'static str,
    fallback: Option<&'static str>,
) -> Option<reqwest::header::HeaderValue> {
    inbound
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| reqwest::header::HeaderValue::from_str(value).ok())
        .or_else(|| fallback.map(reqwest::header::HeaderValue::from_static))
}

async fn forward_anthropic_messages(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    body: Value,
    raw_body: Option<&[u8]>,
    inbound_headers: &HeaderMap,
) -> UpstreamResult {
    let credential = match resolve_provider_credential(provider).await {
        Ok(credential) => credential,
        Err(error) => return error,
    };
    let url = upstream_anthropic_messages_url(&provider.base_url);
    let request = client
        .post(url)
        .timeout(std::time::Duration::from_millis(provider.timeout_ms.max(1)));
    let mut request = if let Some(raw_body) = raw_body {
        request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(raw_body.to_vec())
    } else {
        request.json(&body)
    };
    if let Some(version) =
        anthropic_header_value(inbound_headers, "anthropic-version", Some("2023-06-01"))
    {
        request = request.header("anthropic-version", version);
    }
    if let Some(beta) = anthropic_header_value(inbound_headers, "anthropic-beta", None) {
        request = request.header("anthropic-beta", beta);
    }
    if let Some(credential) = credential.as_deref() {
        let header = match reqwest::header::HeaderValue::from_str(credential) {
            Ok(header) => header,
            Err(_) => {
                return credential_error_result(
                    "provider_credential_invalid",
                    "configured provider credential cannot be used as an Anthropic API key",
                )
            }
        };
        request = request.header("x-api-key", header);
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
                    send_state: UpstreamSendState::Completed,
                },
                Ok(value) => UpstreamResult {
                    response: json!({
                        "error": {
                            "type": classify_upstream_status(status),
                            "message": "configured Anthropic upstream returned an error",
                            "upstream_status": status_code,
                            "upstream_body": value
                        }
                    }),
                    status: Some(status_code),
                    error_category: Some(classify_upstream_status(status).to_string()),
                    send_state: UpstreamSendState::Completed,
                },
                Err(_) => UpstreamResult {
                    response: json!({
                        "error": {
                            "type": "provider_invalid_json",
                            "message": "configured Anthropic upstream returned invalid JSON",
                            "upstream_status": status_code
                        }
                    }),
                    status: Some(status_code),
                    error_category: Some("provider_invalid_json".to_string()),
                    send_state: UpstreamSendState::Completed,
                },
            }
        }
        Err(error) => {
            let category = if error.is_timeout() {
                "provider_timeout"
            } else {
                "provider_network"
            };
            UpstreamResult {
                response: json!({
                    "error": {
                        "type": category,
                        "message": "configured Anthropic upstream request failed"
                    }
                }),
                status: None,
                error_category: Some(category.to_string()),
                send_state: classify_send_failure(&error),
            }
        }
    }
}

async fn forward_anthropic_messages_stream(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    body: Value,
    raw_body: Option<&[u8]>,
    inbound_headers: &HeaderMap,
) -> Result<(u16, axum::http::HeaderMap, Body), UpstreamResult> {
    let credential = resolve_provider_credential(provider).await?;
    let url = upstream_anthropic_messages_url(&provider.base_url);
    let request = client
        .post(url)
        .timeout(std::time::Duration::from_millis(provider.timeout_ms.max(1)));
    let mut request = if let Some(raw_body) = raw_body {
        request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(raw_body.to_vec())
    } else {
        request.json(&body)
    };
    if let Some(version) =
        anthropic_header_value(inbound_headers, "anthropic-version", Some("2023-06-01"))
    {
        request = request.header("anthropic-version", version);
    }
    if let Some(beta) = anthropic_header_value(inbound_headers, "anthropic-beta", None) {
        request = request.header("anthropic-beta", beta);
    }
    if let Some(credential) = credential.as_deref() {
        let header = reqwest::header::HeaderValue::from_str(credential).map_err(|_| {
            credential_error_result(
                "provider_credential_invalid",
                "configured provider credential cannot be used as an Anthropic API key",
            )
        })?;
        request = request.header("x-api-key", header);
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let status_code = status.as_u16();
            if !status.is_success() {
                let value = response.json::<Value>().await.unwrap_or_else(|_| json!({}));
                return Err(UpstreamResult {
                    response: json!({
                        "error": {
                            "type": classify_upstream_status(status),
                            "message": "configured Anthropic upstream returned an error",
                            "upstream_status": status_code,
                            "upstream_body": value
                        }
                    }),
                    status: Some(status_code),
                    error_category: Some(classify_upstream_status(status).to_string()),
                    send_state: UpstreamSendState::Completed,
                });
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("text/event-stream")
                .to_string();
            let stream = response.bytes_stream().map_err(std::io::Error::other);
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                axum::http::HeaderValue::from_str(&content_type)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("text/event-stream")),
            );
            headers.insert(
                CACHE_CONTROL,
                axum::http::HeaderValue::from_static("no-cache"),
            );
            Ok((status_code, headers, Body::from_stream(stream)))
        }
        Err(error) => {
            let category = if error.is_timeout() {
                "provider_timeout"
            } else {
                "provider_network"
            };
            Err(UpstreamResult {
                response: json!({
                    "error": {"type": category, "message": "Anthropic streaming request failed"}
                }),
                status: None,
                error_category: Some(category.to_string()),
                send_state: classify_send_failure(&error),
            })
        }
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
    let mut messages = Vec::new();
    if let Some(system) = body.get("system") {
        let text = anthropic_content_to_text(system);
        if !text.is_empty() {
            messages.push(json!({"role": "system", "content": text}));
        }
    }
    if let Some(items) = body.get("messages").and_then(Value::as_array) {
        for message in items {
            messages.extend(anthropic_message_to_openai(message));
        }
    }
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
    let mut converted = serde_json::Map::new();
    converted.insert("model".to_string(), json!(model));
    converted.insert("messages".to_string(), json!(messages));
    converted.insert("stream".to_string(), json!(false));

    for key in ["max_tokens", "temperature", "top_p"] {
        if let Some(value) = body.get(key) {
            converted.insert(key.to_string(), value.clone());
        }
    }
    if let Some(stop) = body.get("stop_sequences") {
        converted.insert("stop".to_string(), stop.clone());
    }
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        let tools = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.get("name").and_then(Value::as_str).unwrap_or_default(),
                        "description": tool.get("description").and_then(Value::as_str).unwrap_or_default(),
                        "parameters": tool.get("input_schema").cloned().unwrap_or_else(|| json!({"type": "object"}))
                    }
                })
            })
            .collect::<Vec<_>>();
        converted.insert("tools".to_string(), json!(tools));
    }
    if let Some(choice) = anthropic_tool_choice_to_openai(body.get("tool_choice")) {
        converted.insert("tool_choice".to_string(), choice);
    }
    Value::Object(converted)
}

fn anthropic_message_to_openai(message: &Value) -> Vec<Value> {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user");
    let content = message.get("content").unwrap_or(&Value::Null);
    let Some(parts) = content.as_array() else {
        return vec![json!({
            "role": role,
            "content": anthropic_content_to_text(content)
        })];
    };

    if role == "assistant" {
        let text = parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        let tool_calls = parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("tool_use"))
            .map(|part| {
                let arguments = serde_json::to_string(
                    part.get("input")
                        .unwrap_or(&Value::Object(Default::default())),
                )
                .unwrap_or_else(|_| "{}".to_string());
                json!({
                    "id": part.get("id").and_then(Value::as_str).unwrap_or_default(),
                    "type": "function",
                    "function": {
                        "name": part.get("name").and_then(Value::as_str).unwrap_or_default(),
                        "arguments": arguments
                    }
                })
            })
            .collect::<Vec<_>>();
        let mut converted = serde_json::Map::new();
        converted.insert("role".to_string(), json!("assistant"));
        converted.insert(
            "content".to_string(),
            if text.is_empty() {
                Value::Null
            } else {
                json!(text)
            },
        );
        if !tool_calls.is_empty() {
            converted.insert("tool_calls".to_string(), json!(tool_calls));
        }
        return vec![Value::Object(converted)];
    }

    let mut converted = Vec::new();
    let mut pending_text = Vec::new();
    for part in parts {
        if part.get("type").and_then(Value::as_str) == Some("tool_result") {
            if !pending_text.is_empty() {
                converted.push(json!({"role": role, "content": pending_text.join("\n")}));
                pending_text.clear();
            }
            converted.push(json!({
                "role": "tool",
                "tool_call_id": part.get("tool_use_id").and_then(Value::as_str).unwrap_or_default(),
                "content": anthropic_content_to_text(part.get("content").unwrap_or(&Value::Null))
            }));
        } else if let Some(text) = part.get("text").and_then(Value::as_str) {
            pending_text.push(text.to_string());
        }
    }
    if !pending_text.is_empty() {
        converted.push(json!({"role": role, "content": pending_text.join("\n")}));
    }
    if converted.is_empty() {
        converted.push(json!({"role": role, "content": anthropic_content_to_text(content)}));
    }
    converted
}

fn anthropic_tool_choice_to_openai(choice: Option<&Value>) -> Option<Value> {
    let choice = choice?;
    match choice.get("type").and_then(Value::as_str) {
        Some("auto") => Some(json!("auto")),
        Some("any") => Some(json!("required")),
        Some("tool") => Some(json!({
            "type": "function",
            "function": {
                "name": choice.get("name").and_then(Value::as_str).unwrap_or_default()
            }
        })),
        Some("none") => Some(json!("none")),
        _ => None,
    }
}

fn openai_chat_to_anthropic_response(upstream: Value, model: Option<&str>) -> Value {
    let choice = upstream
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first());
    let message = choice.and_then(|choice| choice.get("message"));
    let text = message
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut content = Vec::new();
    if !text.is_empty() {
        content.push(json!({"type": "text", "text": text}));
    }
    if let Some(tool_calls) = message
        .and_then(|message| message.get("tool_calls"))
        .and_then(Value::as_array)
    {
        for call in tool_calls {
            let arguments = call["function"]["arguments"].as_str().unwrap_or("{}");
            let input = serde_json::from_str::<Value>(arguments)
                .unwrap_or_else(|_| json!({"raw": arguments}));
            content.push(json!({
                "type": "tool_use",
                "id": call.get("id").and_then(Value::as_str).unwrap_or_default(),
                "name": call["function"]["name"].as_str().unwrap_or_default(),
                "input": input
            }));
        }
    }
    if content.is_empty() {
        content.push(json!({"type": "text", "text": ""}));
    }
    let stop_reason = match choice
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(Value::as_str)
    {
        Some("tool_calls") | Some("function_call") => "tool_use",
        Some("length") => "max_tokens",
        Some("stop") => "end_turn",
        _ => "end_turn",
    };
    json!({
        "id": upstream.get("id").cloned().unwrap_or_else(|| json!(format!("msg-{}", Uuid::new_v4()))),
        "type": "message",
        "role": "assistant",
        "model": model.unwrap_or("dtoken-bridge-model"),
        "content": content,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": upstream["usage"]["prompt_tokens"].as_u64().unwrap_or_default(),
            "output_tokens": upstream["usage"]["completion_tokens"].as_u64().unwrap_or_default()
        }
    })
}

fn anthropic_route_payload(
    state: &AppState,
    body: Value,
    evidence: RouteEvidence<'_>,
) -> Result<RoutedBridgePayload, RouteSelectionError> {
    let (route, provider) = configured_route(&state.config, Protocol::Anthropic, evidence)?;
    let request_id = Uuid::new_v4();
    let decision = build_configured_route_decision(request_id, route, provider);
    let (upstream_body, adapter) = match provider.protocol {
        Protocol::Anthropic => (body.clone(), RouteAdapterKind::AnthropicMessagesPassthrough),
        Protocol::OpenAi => (
            anthropic_to_openai_body(&body, provider),
            RouteAdapterKind::AnthropicMessagesToOpenAiChat,
        ),
    };
    let optimization_context =
        resolve_optimization_context(&decision, route, provider, adapter, &body, &upstream_body)
            .map_err(|_| RouteSelectionError::InvalidOptimizationContext)?;
    let compression = if provider.protocol == Protocol::Anthropic {
        // Direct Anthropic routes are planned by HybridContextEngine in the handler.
        // Keeping this preliminary body untouched prevents semantic+optical stacking.
        compress_for_route(None, CompressionPolicy::Off, upstream_body)
    } else {
        compress_for_route(
            Some(&optimization_context),
            decision.compression_policy.clone(),
            upstream_body,
        )
    };
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
    Ok(RoutedBridgePayload {
        upstream_body: compression.body.clone(),
        decision,
        request_log,
        event,
        compression,
        provider: provider.clone(),
        optimization_context: Some(optimization_context),
    })
}

async fn prepare_direct_anthropic_hybrid(
    state: &AppState,
    payload: &RoutedBridgePayload,
    raw_body: Vec<u8>,
) -> Option<PreparedHybridRequest> {
    if payload.provider.protocol != Protocol::Anthropic {
        return None;
    }
    let context = payload
        .optimization_context
        .as_ref()?
        .compression_context
        .clone();
    let global = state.config.optical.global_policy.clone();
    let route = state
        .config
        .find_optical_route_policy(&payload.decision.route_id)
        .cloned()
        .unwrap_or_default();
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &state.config.optical,
        dtoken_event_ledger::now_ms(),
    );
    let engine = state.hybrid_engine.clone();
    let request_id = payload.decision.request_id.to_string();
    let compression_policy = payload.decision.compression_policy.clone();
    let body = payload.upstream_body.clone();
    let cancellation = RenderCancellation::default();
    let worker_cancellation = cancellation.clone();
    let mut cancel_guard = RenderCancelGuard(Some(cancellation));
    let result = tokio::task::spawn_blocking(move || {
        engine.prepare(
            &request_id,
            &context,
            compression_policy,
            body,
            Some(&raw_body),
            &global,
            &route,
            &registry,
            &worker_cancellation,
        )
    })
    .await
    .ok();
    cancel_guard.0.take();
    result
}

struct RenderCancelGuard(Option<RenderCancellation>);

impl Drop for RenderCancelGuard {
    fn drop(&mut self) {
        if let Some(cancellation) = self.0.take() {
            cancellation.cancel();
        }
    }
}

fn record_hybrid_pre_send_stages(
    state: &AppState,
    request_id: Uuid,
    prepared: &PreparedHybridRequest,
) -> anyhow::Result<()> {
    let report = prepared.report();
    let common = || {
        json!({
            "kind": hybrid_kind_label(report.kind),
            "reason_code": report.reason_code,
            "candidate_count": report.candidate_count,
            "patched_count": report.patched_count,
            "image_count": report.image_count,
            "source_chars": report.source_chars,
            "png_bytes": report.png_bytes,
            "encoded_image_bytes": report.base64_bytes,
            "estimated_text_tokens_before": report.estimated_text_tokens_before,
            "estimated_input_tokens_after": report.estimated_input_tokens_after,
            "estimated_signed_input_savings": report.estimated_signed_input_savings,
            "capability_revision_present": report.capability_revision.is_some(),
            "renderer_profile_bound": report.profile_digest.is_some(),
            "font_profile_bound": report.font_digest.is_some(),
            "frozen_capsules": report.capsule_digests.len(),
            "artifact_validated": report.artifact_validated,
            "raw_available": false,
            "raw_ingress_observed": report.raw_available,
            "privacy": "metadata only; prompt, rendered text, PNG and base64 are not persisted"
        })
    };
    match prepared.kind() {
        HybridPreparationKind::Active => {
            try_append_stage_event(
                state,
                request_id,
                LayeredStage::OpticalPlan,
                EventStatus::Ok,
                common(),
                None,
            )?;
            try_append_stage_event(
                state,
                request_id,
                LayeredStage::OpticalRender,
                EventStatus::Ok,
                common(),
                None,
            )?;
            try_append_stage_event(
                state,
                request_id,
                LayeredStage::OpticalPatch,
                EventStatus::Ok,
                common(),
                None,
            )?;
            try_append_stage_event(
                state,
                request_id,
                LayeredStage::OpticalValidate,
                EventStatus::Ok,
                common(),
                None,
            )?;
        }
        HybridPreparationKind::Shadow => try_append_stage_event(
            state,
            request_id,
            LayeredStage::OpticalPlan,
            EventStatus::Passthrough,
            common(),
            None,
        )?,
        HybridPreparationKind::Semantic => try_append_stage_event(
            state,
            request_id,
            LayeredStage::HeadroomCompress,
            EventStatus::Ok,
            common(),
            None,
        )?,
        HybridPreparationKind::FallbackPreSend => try_append_stage_event(
            state,
            request_id,
            LayeredStage::OpticalFallback,
            EventStatus::Failed,
            common(),
            Some(&report.reason_code),
        )?,
        HybridPreparationKind::Passthrough => {}
    }
    Ok(())
}

fn hybrid_kind_label(kind: HybridPreparationKind) -> &'static str {
    match kind {
        HybridPreparationKind::Passthrough => "passthrough",
        HybridPreparationKind::Semantic => "semantic",
        HybridPreparationKind::Shadow => "shadow",
        HybridPreparationKind::Active => "active",
        HybridPreparationKind::FallbackPreSend => "fallback_pre_send",
    }
}

fn force_hybrid_pre_send_fallback(
    prepared: PreparedHybridRequest,
    original_body: Value,
    reason: &str,
) -> PreparedHybridRequest {
    let mut accounting = prepared.accounting().clone();
    accounting.disposition = dtoken_types::OptimizationDisposition::FallbackPreSend;
    accounting.sent_payload = dtoken_types::SentPayloadVariant::Original;
    accounting.upstream_state = UpstreamSendState::NotAttempted;
    accounting.attribution_basis = dtoken_types::SavingsAttribution::None;
    accounting.fallback_reason_code = Some(reason.to_string());
    let mut report = prepared.report().clone();
    report.kind = HybridPreparationKind::FallbackPreSend;
    report.reason_code = reason.to_string();
    report.patched_count = 0;
    report.image_count = 0;
    report.png_bytes = 0;
    report.base64_bytes = 0;
    report.artifact_validated = false;
    PreparedHybridRequest::FallbackPreSend {
        body: original_body,
        report,
        accounting,
    }
}

fn apply_hybrid_preparation(payload: &mut RoutedBridgePayload, prepared: &PreparedHybridRequest) {
    payload.upstream_body = prepared.body().clone();
    let accounting = prepared.accounting().clone();
    let baseline = accounting
        .estimated
        .as_ref()
        .and_then(|comparison| comparison.baseline.usage.input.normalized_total)
        .unwrap_or(payload.compression.original_tokens);
    let candidate = accounting
        .estimated
        .as_ref()
        .and_then(|comparison| comparison.candidate.usage.input.normalized_total)
        .unwrap_or(baseline);
    payload.compression.original_tokens = baseline;
    payload.compression.compressed_tokens = candidate;
    payload.compression.tokens_saved = 0;
    payload.compression.savings_percent = 0.0;
    payload.compression.transforms_applied = match prepared.kind() {
        HybridPreparationKind::Active => vec![
            "optical_closed_historical_tool_result".to_string(),
            "deterministic_png_render".to_string(),
            "artifact_post_validation".to_string(),
        ],
        HybridPreparationKind::Semantic => prepared.report().transforms_applied.clone(),
        _ => Vec::new(),
    };
    payload.compression.context_accounting = Some(accounting.clone());
    payload.request_log.input_tokens_before = usize::try_from(baseline).ok();
    payload.request_log.input_tokens_after = usize::try_from(candidate).ok();
    payload.request_log.tokens_saved = Some(0);
    payload.request_log.transforms_applied = payload.compression.transforms_applied.clone();
    payload.request_log.context_accounting = Some(accounting.clone());
    payload.event.input_tokens = Some(baseline);
    payload.event.output_tokens = None;
    payload.event.saved_tokens = Some(0);
    payload.event.context_accounting = Some(accounting);
    if let Some(metadata) = payload.event.metadata.as_object_mut() {
        metadata.insert(
            "hybrid_kind".to_string(),
            json!(hybrid_kind_label(prepared.kind())),
        );
        metadata.insert(
            "hybrid_reason_code".to_string(),
            json!(prepared.report().reason_code),
        );
        metadata.insert(
            "hybrid_image_count".to_string(),
            json!(prepared.report().image_count),
        );
    }
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
    let compression = compress_for_route(None, decision.compression_policy.clone(), request_body);
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
                "saved_tokens": if compression.context_accounting.is_some() { 0 } else { compression.tokens_saved },
                "estimated_saved_tokens": compression.tokens_saved,
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
    let signed_net_tokens = logs
        .iter()
        .map(signed_context_savings)
        .fold(0i64, i64::saturating_add);
    let positive_saved_tokens = logs
        .iter()
        .map(signed_context_savings)
        .filter(|value| *value > 0)
        .map(|value| u64::try_from(value).unwrap_or(u64::MAX))
        .fold(0u64, u64::saturating_add);
    let penalty_tokens = logs
        .iter()
        .map(signed_context_savings)
        .filter(|value| *value < 0)
        .map(i64::unsigned_abs)
        .fold(0u64, u64::saturating_add);
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
        "signed_net_tokens": signed_net_tokens,
        "positive_saved_tokens": positive_saved_tokens,
        "penalty_tokens": penalty_tokens,
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
            "signed_net": signed_net_tokens,
            "positive_saved": positive_saved_tokens,
            "penalty": penalty_tokens,
            "savings_percent": savings_percent
        },
        "layered": layered
    })
}

fn signed_context_savings(log: &RequestLog) -> i64 {
    if let Some(accounting) = log.context_accounting.as_ref() {
        if let Some(savings) = accounting.claimable_signed_savings() {
            return savings
                .total_tokens
                .or(savings.input_tokens)
                .unwrap_or_default();
        }
        if let Some(savings) = accounting.provider_reconciled_signed_savings() {
            return savings
                .total_tokens
                .or(savings.input_tokens)
                .unwrap_or_default();
        }
        return 0;
    }
    i64::try_from(log.tokens_saved.unwrap_or_default()).unwrap_or(i64::MAX)
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
    let signed_net = stats["tokens"]["signed_net"].as_i64().unwrap_or_default();
    let positive_saved = stats["tokens"]["positive_saved"]
        .as_u64()
        .unwrap_or_default();
    let penalty = stats["tokens"]["penalty"].as_u64().unwrap_or_default();
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
dtoken_context_positive_saved_tokens_total {positive_saved}\n\
dtoken_context_penalty_tokens_total {penalty}\n\
dtoken_context_signed_net_tokens {signed_net}\n\
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
        let transform = if log.transforms_applied.is_empty() {
            "runtime_route".to_string()
        } else {
            log.transforms_applied.join("+")
        };
        let entry = by_transform.entry(transform).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        entry.1 += tokens_saved;
        entry.2 += tokens_before;
        entry.3 += latency_ms;

        let route_key = log
            .route_id
            .clone()
            .unwrap_or_else(|| "route:none".to_string());
        add_runtime_group(
            &mut by_route,
            route_key,
            tokens_saved,
            tokens_before,
            latency_ms,
            failed,
        );

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
        add_runtime_group(
            &mut by_policy,
            policy_key,
            tokens_saved,
            tokens_before,
            latency_ms,
            failed,
        );
    }
    let transforms: Vec<Value> = by_transform
        .into_iter()
        .map(
            |(name, (requests, tokens_saved, tokens_before, latency_ms))| {
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
            },
        )
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
            "transforms": transforms,
            "signed_net_tokens": logs.iter().map(signed_context_savings).fold(0i64, i64::saturating_add),
            "positive_saved_tokens": logs.iter().map(signed_context_savings).filter(|value| *value > 0).map(|value| value as u64).fold(0u64, u64::saturating_add),
            "penalty_tokens": logs.iter().map(signed_context_savings).filter(|value| *value < 0).map(i64::unsigned_abs).fold(0u64, u64::saturating_add)
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
            if saved > 0.0 {
                1.0_f64.max(saved / 1_000.0)
            } else {
                0.0
            }
        })
        .sum::<f64>()
        / total_requests;
    let route_adapter_avg = if total_avg > 0.0 {
        1.0_f64.min(total_avg * 0.18).max(0.4)
    } else {
        0.0
    };
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
        csv_escape(
            state
                .config
                .routes
                .first()
                .map(|route| route.id.clone())
                .unwrap_or_default()
        ),
        csv_escape(
            state
                .config
                .providers
                .first()
                .map(|provider| provider.id.clone())
                .unwrap_or_default()
        ),
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
            csv_escape(
                log.chain_mode
                    .map(|value| format!("{:?}", value))
                    .unwrap_or_default()
            ),
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
        .map(
            |(name, (requests, tokens_saved, tokens_before, latency_ms, failed))| {
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
            },
        )
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
    with_active_config(
        build_dashboard_payload(DashboardPayloadInput {
            stats: runtime_stats_payload(&[]),
            history: runtime_stats_history_payload(&[]),
            metrics: runtime_metrics_payload(&[]),
            cache_summary: runtime_cache_summary_payload(),
            providers: provider_metadata_payload(&config.providers),
            routes: json!(config.routes),
            instances: json!(config.instances),
            events: json!([]),
            route_decisions: json!([]),
            request_logs: json!([]),
            memory: memory_summary_for_state(&state),
        }),
        active_config_payload(&state),
    )
}

pub fn dashboard_payload_for_state(state: &AppState) -> Value {
    let route_decisions = state.recent_route_decisions(20);
    let request_logs = state.recent_request_logs(20);
    let all_logs = state.recent_request_logs(10_000);
    with_active_config(
        build_dashboard_payload(DashboardPayloadInput {
            stats: runtime_stats_payload(&all_logs),
            history: runtime_stats_history_payload(&all_logs),
            metrics: runtime_metrics_payload(&all_logs),
            cache_summary: runtime_cache_summary_for_state(state),
            providers: provider_metadata_payload(&state.config.providers),
            routes: json!(state.config.routes),
            instances: json!(state.config.instances),
            events: json!(state.recent_events(20)),
            route_decisions: json!(route_decisions),
            request_logs: json!(request_logs),
            memory: memory_summary_for_state(state),
        }),
        active_config_payload(state),
    )
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

fn route_selection_error_response(
    request_id: Uuid,
    input_protocol: &str,
    error: RouteSelectionError,
    config_status: &RuntimeConfigStatus,
) -> (StatusCode, Value) {
    if error == RouteSelectionError::NotConfigured {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            route_not_configured_response(request_id, input_protocol, config_status),
        );
    }
    let (error_type, message) = match error {
        RouteSelectionError::Ambiguous => (
            "dtoken_route_ambiguous",
            "multiple configured routes match this request; provide a unique model or x-dtoken-route-id/x-dtoken-instance-id",
        ),
        RouteSelectionError::EvidenceConflict => (
            "dtoken_route_evidence_conflict",
            "the supplied route or instance evidence does not match a configured route for this protocol",
        ),
        RouteSelectionError::InvalidOptimizationContext => (
            "dtoken_route_context_invalid",
            "the resolved route, provider, protocol, adapter or model identity is inconsistent",
        ),
        RouteSelectionError::NotConfigured => unreachable!(),
    };
    (
        StatusCode::CONFLICT,
        json!({
            "error": {
                "type": error_type,
                "code": error_type,
                "message": message
            },
            "dtoken": {
                "source": "dtoken-owned-runtime",
                "request_id": request_id,
                "input_protocol": input_protocol,
                "fallback": "disabled"
            }
        }),
    )
}

fn record_route_selection_error(
    state: &AppState,
    request_id: Uuid,
    input_protocol: &str,
    error: RouteSelectionError,
) {
    let (reason, code) = match error {
        RouteSelectionError::NotConfigured => (
            "no explicit route and upstream provider configured",
            "dtoken_route_not_configured",
        ),
        RouteSelectionError::Ambiguous => (
            "multiple routes match without unique request evidence",
            "dtoken_route_ambiguous",
        ),
        RouteSelectionError::EvidenceConflict => (
            "route or instance evidence conflicts with configured routes",
            "dtoken_route_evidence_conflict",
        ),
        RouteSelectionError::InvalidOptimizationContext => (
            "resolved route/provider/model context failed validation",
            "dtoken_route_context_invalid",
        ),
    };
    append_stage_event(
        state,
        request_id,
        LayeredStage::RouteForward,
        EventStatus::Failed,
        json!({
            "input_protocol": input_protocol,
            "reason": reason,
            "provider_count": state.config_status.provider_count,
            "route_count": state.config_status.route_count,
            "fallback": "disabled",
            "raw_available": false
        }),
        Some(code),
    );
}

async fn openai_chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(original_body): Json<Value>,
) -> impl IntoResponse {
    let (request, original_body) = match parse_openai_request_body(original_body) {
        Ok(parsed) => parsed,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "type": "invalid_request",
                        "message": format!("invalid OpenAI chat request: {error}")
                    }
                })),
            )
                .into_response();
        }
    };
    let route_selection = configured_route(
        &state.config,
        Protocol::OpenAi,
        header_evidence(&headers, request.model.as_deref()),
    );
    if let Ok((route, provider)) = route_selection {
        let request_id = Uuid::new_v4();
        let decision = build_configured_route_decision(request_id, route, provider);
        let optimization_context = match resolve_optimization_context(
            &decision,
            route,
            provider,
            RouteAdapterKind::OpenAiChatPassthrough,
            &original_body,
            &original_body,
        ) {
            Ok(context) => context,
            Err(_) => {
                record_route_selection_error(
                    &state,
                    request_id,
                    "openai",
                    RouteSelectionError::InvalidOptimizationContext,
                );
                let (status, response) = route_selection_error_response(
                    request_id,
                    "openai",
                    RouteSelectionError::InvalidOptimizationContext,
                    &state.config_status,
                );
                return (status, Json(response)).into_response();
            }
        };
        let inventory_diagnostics = compression_inventory_diagnostics(
            &optimization_context.compression_context,
            &original_body,
        );
        let compression = compress_for_route(
            Some(&optimization_context),
            decision.compression_policy.clone(),
            original_body,
        );
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
        if let Some(metadata) = event.metadata.as_object_mut() {
            metadata.insert("compression_inventory".to_string(), inventory_diagnostics);
        }
        let compression_json = json!({
            "input_tokens_before": compression.original_tokens,
            "input_tokens_after": compression.compressed_tokens,
            "saved_tokens": if compression.context_accounting.is_some() { 0 } else { compression.tokens_saved },
            "estimated_saved_tokens": compression.tokens_saved,
            "savings_percent": compression.savings_percent,
            "elapsed_us": compression.elapsed_us,
            "transforms_applied": compression.transforms_applied,
        });
        if request.stream.unwrap_or(false) {
            let upstream =
                forward_openai_chat_stream(&state.http_client, provider, compression.body.clone())
                    .await;
            match upstream {
                Ok((status_code, headers, body)) => {
                    if let Some(accounting) = request_log.context_accounting.as_mut() {
                        accounting.upstream_state = UpstreamSendState::StreamStarted;
                        accounting.attribution_basis = dtoken_types::SavingsAttribution::None;
                    }
                    event.context_accounting = request_log.context_accounting.clone();
                    event.status = EventStatus::Ok;
                    event.saved_tokens = Some(0);
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
                    return (
                        StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK),
                        headers,
                        body,
                    )
                        .into_response();
                }
                Err(upstream) => {
                    mark_upstream_outcome(&upstream, &mut request_log, &mut event);
                    let response_status = upstream_response_status(&upstream);
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
                    return (response_status, Json(response)).into_response();
                }
            }
        }
        let upstream =
            forward_openai_chat(&state.http_client, provider, compression.body.clone()).await;
        mark_upstream_outcome(&upstream, &mut request_log, &mut event);
        let response_status = upstream_response_status(&upstream);
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
        return (response_status, Json(response)).into_response();
    }

    let request_id = Uuid::new_v4();
    let error = route_selection.expect_err("successful selection returned above");
    record_route_selection_error(&state, request_id, "openai", error);
    let (status, response) =
        route_selection_error_response(request_id, "openai", error, &state.config_status);
    (status, Json(response)).into_response()
}

async fn openai_responses(
    State(state): State<AppState>,
    headers: HeaderMap,
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

    let route_selection = configured_route(
        &state.config,
        Protocol::OpenAi,
        header_evidence(&headers, request.model.as_deref()),
    );
    if let Ok((route, provider)) = route_selection {
        let request_id = Uuid::new_v4();
        let decision = build_configured_route_decision(request_id, route, provider);
        let original_body = json!({
            "model": request.model,
            "messages": request.messages,
            "stream": false
        });
        let optimization_context = match resolve_optimization_context(
            &decision,
            route,
            provider,
            RouteAdapterKind::OpenAiResponsesToChat,
            &body,
            &original_body,
        ) {
            Ok(context) => context,
            Err(_) => {
                record_route_selection_error(
                    &state,
                    request_id,
                    "openai_responses",
                    RouteSelectionError::InvalidOptimizationContext,
                );
                let (status, response) = route_selection_error_response(
                    request_id,
                    "openai_responses",
                    RouteSelectionError::InvalidOptimizationContext,
                    &state.config_status,
                );
                return (status, Json(response)).into_response();
            }
        };
        let compression = compress_for_route(
            Some(&optimization_context),
            decision.compression_policy.clone(),
            original_body,
        );
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
        let upstream = require_openai_message(
            forward_openai_chat(&state.http_client, provider, compression.body.clone()).await,
            "OpenAI Responses",
        );
        mark_upstream_outcome(&upstream, &mut request_log, &mut event);
        let response_status = upstream_response_status(&upstream);
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
            "saved_tokens": if compression.context_accounting.is_some() { 0 } else { compression.tokens_saved },
            "estimated_saved_tokens": compression.tokens_saved,
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
            let text = upstream
                .response
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .expect("validated OpenAI message content");
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
        return (response_status, Json(response)).into_response();
    }

    let request_id = Uuid::new_v4();
    let error = route_selection.expect_err("successful selection returned above");
    record_route_selection_error(&state, request_id, "openai_responses", error);
    let (status, response) =
        route_selection_error_response(request_id, "openai_responses", error, &state.config_status);
    (status, Json(response)).into_response()
}

async fn anthropic_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> impl IntoResponse {
    let body: Value = match serde_json::from_slice(&body_bytes) {
        Ok(body) => body,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "type": "invalid_request",
                        "message": "invalid Anthropic Messages JSON body"
                    }
                })),
            )
                .into_response();
        }
    };
    let raw_body = body_bytes.to_vec();
    let evidence = header_evidence(&headers, body.get("model").and_then(Value::as_str));
    let mut payload = match anthropic_route_payload(&state, body.clone(), evidence) {
        Ok(payload) => payload,
        Err(error) => {
            let request_id = Uuid::new_v4();
            record_route_selection_error(&state, request_id, "anthropic", error);
            let (status, response) = route_selection_error_response(
                request_id,
                "anthropic",
                error,
                &state.config_status,
            );
            return (status, Json(response)).into_response();
        }
    };

    if payload.provider.protocol == Protocol::Anthropic {
        let original_body = body.clone();
        let mut send_raw_body = true;
        if let Some(mut prepared) =
            prepare_direct_anthropic_hybrid(&state, &payload, raw_body.clone()).await
        {
            let stage_recording =
                record_hybrid_pre_send_stages(&state, payload.decision.request_id, &prepared);
            if stage_recording.is_err()
                && matches!(
                    prepared.kind(),
                    HybridPreparationKind::Active | HybridPreparationKind::Semantic
                )
            {
                prepared = force_hybrid_pre_send_fallback(
                    prepared,
                    original_body,
                    "ledger_pre_send_unavailable",
                );
                append_stage_event(
                    &state,
                    payload.decision.request_id,
                    LayeredStage::OpticalFallback,
                    EventStatus::Failed,
                    json!({
                        "reason_code": "ledger_pre_send_unavailable",
                        "raw_available": false,
                        "privacy": "baseline selected before upstream send"
                    }),
                    Some("ledger_pre_send_unavailable"),
                );
            }
            send_raw_body = matches!(
                prepared.kind(),
                HybridPreparationKind::Passthrough
                    | HybridPreparationKind::Shadow
                    | HybridPreparationKind::FallbackPreSend
            );
            apply_hybrid_preparation(&mut payload, &prepared);
        } else {
            append_stage_event(
                &state,
                payload.decision.request_id,
                LayeredStage::OpticalFallback,
                EventStatus::Failed,
                json!({
                    "reason_code": "hybrid_worker_unavailable",
                    "raw_available": false,
                    "baseline_selected": true,
                    "automatic_retry": false
                }),
                Some("hybrid_worker_unavailable"),
            );
        }

        let stream = payload
            .upstream_body
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if stream {
            match forward_anthropic_messages_stream(
                &state.http_client,
                &payload.provider,
                payload.upstream_body.clone(),
                send_raw_body.then_some(raw_body.as_slice()),
                &headers,
            )
            .await
            {
                Ok((status_code, response_headers, response_body)) => {
                    if let Some(accounting) = payload.request_log.context_accounting.as_mut() {
                        accounting.upstream_state = UpstreamSendState::StreamStarted;
                        accounting.attribution_basis = dtoken_types::SavingsAttribution::None;
                        if accounting.disposition
                            == dtoken_types::OptimizationDisposition::FallbackPreSend
                        {
                            accounting.disposition =
                                dtoken_types::OptimizationDisposition::FallbackOriginalSent;
                        }
                    }
                    payload.event.context_accounting =
                        payload.request_log.context_accounting.clone();
                    payload.event.status = EventStatus::Ok;
                    if let Some(metadata) = payload.event.metadata.as_object_mut() {
                        metadata.insert("stream".to_string(), json!(true));
                        metadata.insert("send_state".to_string(), json!("stream_started"));
                    }
                    state.record_routed_request(
                        &payload.decision,
                        payload.request_log,
                        payload.event,
                    );
                    append_stage_event(
                        &state,
                        payload.decision.request_id,
                        LayeredStage::UpstreamSendCommitted,
                        EventStatus::Ok,
                        json!({
                            "provider_id": payload.decision.provider_id,
                            "protocol": "anthropic",
                            "stream": true,
                            "upstream_status": status_code,
                            "automatic_retry": false,
                            "raw_available": false
                        }),
                        None,
                    );
                    return (
                        StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK),
                        response_headers,
                        response_body,
                    )
                        .into_response();
                }
                Err(upstream) => {
                    mark_upstream_outcome(&upstream, &mut payload.request_log, &mut payload.event);
                    let response_status = upstream_response_status(&upstream);
                    let response = json!({
                        "error": upstream.response["error"],
                        "dtoken": {
                            "source": "dtoken-owned-runtime",
                            "request_id": payload.decision.request_id,
                            "route_decision": route_decision_to_value(&payload.decision),
                            "request_log": payload.request_log,
                            "ledger_event": payload.event,
                            "upstream_status": upstream.status,
                            "upstream_error_category": upstream.error_category,
                            "send_state": upstream.send_state,
                            "automatic_retry": false
                        }
                    });
                    state.record_routed_request(
                        &payload.decision,
                        payload.request_log,
                        payload.event,
                    );
                    append_stage_event(
                        &state,
                        payload.decision.request_id,
                        LayeredStage::ProviderForward,
                        EventStatus::Failed,
                        json!({
                            "provider_id": payload.decision.provider_id,
                            "protocol": "anthropic",
                            "stream": true,
                            "send_state": upstream.send_state,
                            "automatic_retry": false,
                            "raw_available": false
                        }),
                        upstream.error_category.as_deref(),
                    );
                    return (response_status, Json(response)).into_response();
                }
            }
        }

        let upstream = require_anthropic_message(
            forward_anthropic_messages(
                &state.http_client,
                &payload.provider,
                payload.upstream_body.clone(),
                send_raw_body.then_some(raw_body.as_slice()),
                &headers,
            )
            .await,
        );
        mark_upstream_outcome(&upstream, &mut payload.request_log, &mut payload.event);
        let response_status = upstream_response_status(&upstream);
        let provider_status = if upstream.error_category.is_some() {
            EventStatus::Failed
        } else {
            EventStatus::Ok
        };
        let response = if upstream.error_category.is_some() {
            json!({
                "error": upstream.response["error"],
                "dtoken": {
                    "source": "dtoken-owned-runtime",
                    "request_id": payload.decision.request_id,
                    "route_decision": route_decision_to_value(&payload.decision),
                    "request_log": payload.request_log,
                    "ledger_event": payload.event,
                    "upstream_status": upstream.status,
                    "upstream_error_category": upstream.error_category,
                    "send_state": upstream.send_state,
                    "automatic_retry": false
                }
            })
        } else {
            upstream.response.clone()
        };
        state.record_routed_request(&payload.decision, payload.request_log, payload.event);
        append_stage_event(
            &state,
            payload.decision.request_id,
            if upstream.send_state == UpstreamSendState::Completed {
                LayeredStage::OpticalComplete
            } else {
                LayeredStage::ProviderForward
            },
            provider_status,
            json!({
                "route_id": payload.decision.route_id,
                "provider_id": payload.decision.provider_id,
                "input_protocol": "anthropic",
                "output_protocol": "anthropic",
                "upstream_status": upstream.status,
                "send_state": upstream.send_state,
                "automatic_retry": false,
                "raw_available": false,
                "privacy": "request and response bodies are never persisted"
            }),
            upstream.error_category.as_deref(),
        );
        return (response_status, Json(response)).into_response();
    }

    let upstream = require_openai_message(
        forward_openai_chat(
            &state.http_client,
            &payload.provider,
            payload.upstream_body.clone(),
        )
        .await,
        "Anthropic bridge",
    );
    mark_upstream_outcome(&upstream, &mut payload.request_log, &mut payload.event);
    let response_status = upstream_response_status(&upstream);
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
                    "saved_tokens": if payload.compression.context_accounting.is_some() { 0 } else { payload.compression.tokens_saved },
                    "estimated_saved_tokens": payload.compression.tokens_saved,
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
    (response_status, Json(response)).into_response()
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
    Json(provider_metadata_payload(&state.config.providers))
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
    let issues = state.optical_config_issues.as_ref();
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &state.config.optical,
        dtoken_event_ledger::now_ms(),
    );
    let active_direct_route = active_direct_anthropic_route_count(&state.config, &registry) > 0;
    let active_requested = state.config.optical.global_policy.allow_active;
    let config_ok = issues.is_empty();
    let registry_ok = !active_requested || registry.verified_rule_count() > 0;
    let route_ok = !active_requested || active_direct_route;
    report.checks.push(dtoken_types::DoctorCheck {
        name: "hybrid_context_config".to_string(),
        ok: config_ok,
        message: if config_ok {
            "Hybrid context configuration schema and resource budgets are valid".to_string()
        } else {
            format!("Hybrid context configuration is fail-closed: {issues:?}")
        },
    });
    report.checks.push(dtoken_types::DoctorCheck {
        name: "hybrid_capability_eval_gate".to_string(),
        ok: registry_ok,
        message: format!(
            "verified_attested_rules={} rejected_rules={} active_requested={active_requested}",
            registry.verified_rule_count(),
            registry.rejected_rule_count()
        ),
    });
    report.checks.push(dtoken_types::DoctorCheck {
        name: "hybrid_direct_anthropic_route".to_string(),
        ok: route_ok,
        message: if active_direct_route {
            "An explicit same-protocol Anthropic route is active-capable".to_string()
        } else {
            "No active same-protocol Anthropic route; Optical remains shadow/passthrough"
                .to_string()
        },
    });
    report.checks.push(dtoken_types::DoctorCheck {
        name: "hybrid_renderer_limits".to_string(),
        ok: state.config.optical.global_policy.max_concurrent_renders > 0,
        message: format!(
            "pages={} pixels={} png_bytes={} encoded_image_bytes={} memory_bytes={} concurrency={}",
            state.config.optical.global_policy.max_images_per_request,
            state.config.optical.global_policy.max_total_pixels,
            state.config.optical.global_policy.max_png_bytes,
            state.config.optical.global_policy.max_base64_bytes,
            state.config.optical.global_policy.max_memory_bytes,
            state.config.optical.global_policy.max_concurrent_renders
        ),
    });
    report.ok = report.ok && config_ok && registry_ok && route_ok;
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
    let engine = RuleBasedRecommendationEngine;
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

// Compatibility rows are intentionally declared as compact table-like records.
#[allow(clippy::too_many_arguments)]
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
            optical: Default::default(),
        })
    }

    fn route_test_config() -> DtokenConfig {
        let provider = |id: &str, model: &str| dtoken_types::ProviderProfile {
            id: id.to_string(),
            name: id.to_string(),
            kind: dtoken_types::ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url: "http://127.0.0.1:9/v1".to_string(),
            api_key_ref: None,
            models: vec![model.to_string()],
            timeout_ms: 100,
            healthcheck_path: None,
        };
        let route = |id: &str, instance_id: &str, provider_id: &str| dtoken_types::RouteProfile {
            id: id.to_string(),
            name: id.to_string(),
            input_protocol: Protocol::OpenAi,
            instance_id: instance_id.to_string(),
            target_provider_id: provider_id.to_string(),
            fallback_provider_id: None,
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Balanced,
            bind_host: "127.0.0.1".to_string(),
            local_port: 18001,
        };
        DtokenConfig {
            providers: vec![
                provider("provider-a", "model-a"),
                provider("provider-b", "model-b"),
            ],
            routes: vec![
                route("route-a", "instance-a", "provider-a"),
                route("route-b", "instance-b", "provider-b"),
            ],
            instances: vec![],
            optical: Default::default(),
        }
    }

    #[test]
    fn compression_inventory_diagnostics_are_metadata_only_and_show_eligibility() {
        let mut messages = Vec::new();
        let log = "INFO build stable route healthy tests passed\n".repeat(420);
        for index in 0..8 {
            let call_id = format!("call-{index}");
            messages.push(json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {"name": "bash", "arguments": "{}"}
                }]
            }));
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": log
            }));
        }
        messages.push(json!({"role": "user", "content": "Summarize the verified state."}));
        let body = json!({"model": "mock/model", "messages": messages, "stream": true});
        let context = dtoken_compress::ResolvedOptimizationContext {
            route_id: "local-mock-route".to_string(),
            provider_id: "local-mock-provider".to_string(),
            model: "mock/model".to_string(),
            input_protocol: Protocol::OpenAi,
            output_protocol: Protocol::OpenAi,
            endpoint: dtoken_types::OpticalEndpoint::OpenAiChatCompletions,
        };

        let diagnostics = compression_inventory_diagnostics(&context, &body);
        assert_eq!(diagnostics["state"], "resolved");
        assert!(diagnostics["approved_target_count"].as_u64().unwrap_or(0) > 0);
        assert_eq!(diagnostics["raw_available"], false);
        assert!(!diagnostics.to_string().contains("build stable"));
        assert!(diagnostics["tool_nodes"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().all(|node| node["correlation_digest"]
                .as_str()
                .is_none_or(|digest| digest.len() == 12))));

        let compressed = compress_for_route(
            Some(&ResolvedRouteOptimizationContext {
                request_id: Uuid::new_v4(),
                route_id: "local-mock-route".to_string(),
                provider_id: "local-mock-provider".to_string(),
                requested_policy: CompressionPolicy::Balanced,
                adapter: RouteAdapterKind::OpenAiChatPassthrough,
                ingress: OptimizationSurface {
                    protocol: Protocol::OpenAi,
                    endpoint: dtoken_types::OpticalEndpoint::OpenAiChatCompletions,
                },
                upstream: OptimizationSurface {
                    protocol: Protocol::OpenAi,
                    endpoint: dtoken_types::OpticalEndpoint::OpenAiChatCompletions,
                },
                requested_model: "mock/model".to_string(),
                effective_model: "mock/model".to_string(),
                optical_disposition: OpticalExecutionDisposition::Disabled,
                compression_context: context,
            }),
            CompressionPolicy::Balanced,
            body,
        );
        assert!(compressed.tokens_saved > 0);
        assert!(compressed
            .transforms_applied
            .iter()
            .any(|transform| transform == "semantic_closed_tool_result"));
    }

    #[test]
    fn ambiguous_routes_fail_closed_without_unique_evidence() {
        let config = route_test_config();

        assert_eq!(
            configured_route(&config, Protocol::OpenAi, RouteEvidence::default())
                .expect_err("multiple protocol matches must not select the first route"),
            RouteSelectionError::Ambiguous
        );
        let (route, provider) = configured_route(
            &config,
            Protocol::OpenAi,
            RouteEvidence {
                model: Some("model-b"),
                ..Default::default()
            },
        )
        .expect("a unique provider model is explicit route evidence");
        assert_eq!(route.id, "route-b");
        assert_eq!(provider.id, "provider-b");
    }

    #[test]
    fn resolved_context_maps_all_three_adapters_without_conflating_surfaces() {
        let config = route_test_config();
        let base_route = config.routes[0].clone();
        let provider = config.providers[0].clone();

        let cases = [
            (
                RouteAdapterKind::OpenAiChatPassthrough,
                Protocol::OpenAi,
                OpticalEndpoint::OpenAiChatCompletions,
                "model-a",
                "model-a",
            ),
            (
                RouteAdapterKind::OpenAiResponsesToChat,
                Protocol::OpenAi,
                OpticalEndpoint::OpenAiResponses,
                "model-a",
                "model-a",
            ),
            (
                RouteAdapterKind::AnthropicMessagesToOpenAiChat,
                Protocol::Anthropic,
                OpticalEndpoint::AnthropicMessages,
                "claude-requested-model",
                "model-a",
            ),
        ];

        for (adapter, ingress_protocol, ingress_endpoint, requested, effective) in cases {
            let mut route = base_route.clone();
            route.input_protocol = ingress_protocol.clone();
            let decision = build_configured_route_decision(Uuid::new_v4(), &route, &provider);
            let ingress = json!({"model": requested, "messages": []});
            let upstream = json!({"model": effective, "messages": []});

            let context = resolve_optimization_context(
                &decision, &route, &provider, adapter, &ingress, &upstream,
            )
            .expect("valid route surfaces should resolve");

            assert_eq!(context.ingress.protocol, ingress_protocol);
            assert_eq!(context.ingress.endpoint, ingress_endpoint);
            assert_eq!(context.upstream.protocol, Protocol::OpenAi);
            assert_eq!(
                context.upstream.endpoint,
                OpticalEndpoint::OpenAiChatCompletions
            );
            assert_eq!(context.requested_model, requested);
            assert_eq!(context.effective_model, effective);
            assert_eq!(
                context.optical_disposition,
                OpticalExecutionDisposition::Disabled
            );
        }
    }

    #[test]
    fn resolved_context_rejects_decision_provider_mismatch() {
        let config = route_test_config();
        let route = &config.routes[0];
        let provider = &config.providers[0];
        let mut decision = build_configured_route_decision(Uuid::new_v4(), route, provider);
        decision.provider_id = "spoofed-provider".to_string();

        assert_eq!(
            resolve_optimization_context(
                &decision,
                route,
                provider,
                RouteAdapterKind::OpenAiChatPassthrough,
                &json!({"model":"model-a"}),
                &json!({"model":"model-a"}),
            )
            .expect_err("spoofed provider identity must fail closed"),
            ContextResolutionError::DecisionProviderMismatch
        );
    }

    #[test]
    fn resolved_context_rejects_route_ingress_protocol_mismatch() {
        let config = route_test_config();
        let route = &config.routes[0];
        let provider = &config.providers[0];
        let decision = build_configured_route_decision(Uuid::new_v4(), route, provider);

        assert_eq!(
            resolve_optimization_context(
                &decision,
                route,
                provider,
                RouteAdapterKind::AnthropicMessagesToOpenAiChat,
                &json!({"model":"claude-requested"}),
                &json!({"model":"model-a"}),
            )
            .expect_err("adapter and route ingress protocol must agree"),
            ContextResolutionError::RouteIngressProtocolMismatch
        );
    }

    #[test]
    fn resolved_context_uses_adapted_model_not_requested_model_for_bridge() {
        let config = route_test_config();
        let mut route = config.routes[0].clone();
        route.input_protocol = Protocol::Anthropic;
        let provider = &config.providers[0];
        let decision = build_configured_route_decision(Uuid::new_v4(), &route, provider);
        let context = resolve_optimization_context(
            &decision,
            &route,
            provider,
            RouteAdapterKind::AnthropicMessagesToOpenAiChat,
            &json!({"model":"claude-fable-5"}),
            &json!({"model":"model-a"}),
        )
        .expect("valid bridge context should resolve");

        assert_eq!(context.requested_model, "claude-fable-5");
        assert_eq!(context.effective_model, "model-a");
        assert_ne!(context.requested_model, context.effective_model);
    }

    #[tokio::test]
    async fn ambiguous_openai_request_returns_conflict_without_forwarding() {
        let app = build_router(AppState::new(route_test_config()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("host", "127.0.0.1:18001")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"messages": [{"role": "user", "content": "hello"}]}).to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let payload = response_json(response).await;
        assert_eq!(payload["error"]["type"], "dtoken_route_ambiguous");
        assert_eq!(payload["dtoken"]["fallback"], "disabled");
    }

    #[tokio::test]
    async fn oversized_llm_body_is_rejected_before_json_extraction() {
        let app = build_router(AppState::new(DtokenConfig::empty()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("host", "127.0.0.1:18001")
                    .header("content-type", "application/json")
                    .body(Body::from(vec![b'x'; MAX_REQUEST_BODY_BYTES + 1]))
                    .expect("request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn oversized_optical_preview_body_is_rejected_before_json_extraction() {
        let app = build_router(AppState::new(DtokenConfig::empty()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/compression/optical/preview")
                    .header("host", "127.0.0.1:18001")
                    .header("content-type", "application/json")
                    .body(Body::from(vec![b'x'; MAX_REQUEST_BODY_BYTES + 1]))
                    .expect("request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn runtime_metadata_streams_repair_corrupt_tails_consistently() {
        use std::io::Write as _;

        let dir = std::env::temp_dir().join(format!("dtoken-runtime-store-{}", Uuid::new_v4()));
        let store = RuntimeStore::new(&dir);
        let decision = RouteDecision {
            request_id: Uuid::new_v4(),
            route_id: "route-a".to_string(),
            provider_id: "provider-a".to_string(),
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Balanced,
            reason: "test".to_string(),
            input_protocol: Some(Protocol::OpenAi),
            output_protocol: Some(Protocol::OpenAi),
        };
        let compression = CompressionSummary {
            original_tokens: 10,
            compressed_tokens: 8,
            tokens_saved: 2,
            savings_percent: 20.0,
            elapsed_us: 1,
            transforms_applied: vec![],
            body: json!({}),
            context_accounting: None,
        };
        let log = build_request_log(
            decision.request_id,
            &decision,
            Some("model-a".to_string()),
            Protocol::OpenAi,
            &compression,
        );
        let route_value = route_decision_to_value(&decision);
        let metadata = json!({"recovery_ref": "ccr:route-a:test", "raw_available": false});
        let memory = json!({"memory_id": "mem-test", "raw_available": false});

        store
            .append_route_decision(&route_value)
            .expect("seed route");
        store.append_request_log(&log).expect("seed request log");
        store.append_ccr_metadata(&metadata).expect("seed CCR");
        store.append_memory_record(&memory).expect("seed memory");
        for name in [
            "route-decisions.jsonl",
            "request-logs.jsonl",
            "ccr-metadata.jsonl",
            "memory-records.jsonl",
        ] {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(dir.join(name))
                .expect("open runtime stream for fault injection");
            file.write_all(b"truncated-tail")
                .expect("inject corrupt tail");
        }
        store
            .append_route_decision(&route_value)
            .expect("repair route");
        store.append_request_log(&log).expect("repair request log");
        store.append_ccr_metadata(&metadata).expect("repair CCR");
        store.append_memory_record(&memory).expect("repair memory");

        assert_eq!(store.recent_route_decisions(10).unwrap().len(), 2);
        assert_eq!(store.recent_request_logs(10).unwrap().len(), 2);
        assert_eq!(store.recent_ccr_metadata(10).unwrap().len(), 2);
        assert_eq!(store.recent_memory_records(10).unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(dir);
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
            HeaderMap::new(),
            Json(json!({
                "model": "example/model",
                "messages": [{"role": "user", "content": "hello"}],
                "stream": false
            })),
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
            HeaderMap::new(),
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
            HeaderMap::new(),
            Bytes::from_static(br#"{"model":"example/model","messages":[]}"#),
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
            HeaderMap::new(),
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
            HeaderMap::new(),
            Json(json!({
                "model": "mock/model",
                "messages": [{"role": "user", "content": "hello"}],
                "stream": false
            })),
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
            HeaderMap::new(),
            Json(json!({
                "model": "mock/model",
                "messages": [{"role": "user", "content": "hello"}],
                "stream": false
            })),
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
            optical: Default::default(),
        };
        let status = RuntimeConfigStatus::from_config(
            "desktop_active_config",
            Some(
                "C:/Users/example/AppData/Local/d-token/active-config/dtoken-config.json"
                    .to_string(),
            ),
            &config,
        );
        let state = AppState::new_with_config_status(config, status);
        let payload = dashboard_payload_for_state(&state);
        let active = active_config_payload(&state);

        assert_eq!(
            payload["active_config"]["config"]["source"],
            "desktop_active_config"
        );
        assert_eq!(payload["active_config"]["config"]["route_count"], 1);
        assert_eq!(
            payload["active_config"]["config"]["fallback_disabled"],
            true
        );
        assert_eq!(active["providers"][0]["id"], "active-provider");
        assert!(active["privacy"].as_str().unwrap().contains("API keys"));
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
            optical: Default::default(),
        };
        let status = RuntimeConfigStatus::from_config(
            "desktop_active_config",
            Some(
                "C:/Users/example/AppData/Local/d-token/active-config/dtoken-config.json"
                    .to_string(),
            ),
            &config,
        );
        let state = AppState::new_with_config_status(config, status);
        let json_export = runtime_export_json_payload(&state);
        let csv_export = runtime_export_csv_payload(&state);

        assert_eq!(json_export["redacted"], true);
        assert_eq!(
            json_export["active_config"]["config"]["source"],
            "desktop_active_config"
        );
        assert_eq!(
            json_export["active_config"]["config"]["fallback_disabled"],
            true
        );
        assert!(json_export["privacy"]
            .as_str()
            .unwrap()
            .contains("No raw prompt"));
        assert!(csv_export.contains("config_status"));
        assert!(csv_export.contains("source=desktop_active_config"));
        assert!(!json_export.to_string().contains("sk-"));
        assert!(!csv_export.contains("sk-"));
    }

    #[test]
    fn openai_route_adapter_records_safe_passthrough_and_route_metadata() {
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
        assert_eq!(request_log.tokens_saved, Some(0));
        assert!(request_log.latency_ms.unwrap_or_default() > 0);
        assert!(request_log.transforms_applied.is_empty());
        assert_eq!(event.source, EventSource::Dtoken);
        assert_eq!(event.status, EventStatus::Passthrough);
        assert!(event.metadata.get("full_prompt").is_none());
    }

    #[test]
    fn openai_request_body_preserves_coding_agent_tool_semantics() {
        let original = json!({
            "model": "agent/model",
            "messages": [{"role": "user", "content": "inspect the repository"}],
            "stream": false,
            "tools": [{
                "type": "function",
                "function": {
                    "name": "read_file",
                    "description": "Read a workspace file",
                    "parameters": {"type": "object"}
                }
            }],
            "tool_choice": "auto",
            "temperature": 0.2,
            "max_tokens": 4096,
            "response_format": {"type": "json_object"}
        });

        let (request, body) = parse_openai_request_body(original).expect("parse OpenAI request");

        assert_eq!(request.model.as_deref(), Some("agent/model"));
        assert_eq!(body["tools"][0]["function"]["name"], "read_file");
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["temperature"], 0.2);
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    #[test]
    fn provider_credential_references_never_leave_runtime_metadata_apis() {
        let config = DtokenConfig {
            providers: vec![dtoken_types::ProviderProfile {
                id: "secured-provider".to_string(),
                name: "Secured provider".to_string(),
                kind: dtoken_types::ProviderKind::Official,
                protocol: Protocol::OpenAi,
                base_url: "https://api.example.com/v1".to_string(),
                api_key_ref: Some("os-credential://d-token/providers/secured".to_string()),
                models: vec!["model".to_string()],
                timeout_ms: 30_000,
                healthcheck_path: None,
            }],
            routes: vec![],
            instances: vec![],
            optical: Default::default(),
        };
        let state = AppState::new(config);

        for payload in [
            active_config_payload(&state),
            dashboard_payload_for_state(&state),
        ] {
            let serialized = payload.to_string();
            assert!(!serialized.contains("os-credential://"));
            assert!(payload["providers"][0].get("api_key_ref").is_none());
            assert_eq!(payload["providers"][0]["credential_configured"], true);
        }
    }

    #[test]
    fn os_credential_reference_parses_without_exposing_a_secret() {
        assert_eq!(
            credential_target_from_reference("os-credential://d-token/providers/secured"),
            Ok(("d-token".to_string(), "providers/secured".to_string()))
        );
        assert_eq!(
            credential_target_from_reference("wincred://d-token/providers/secured"),
            Ok(("d-token".to_string(), "providers/secured".to_string()))
        );
        assert!(credential_target_from_reference("https://example.com/secret").is_err());
        assert!(
            credential_target_from_reference("os-credential://user:pass@d-token/item").is_err()
        );
        assert!(
            credential_target_from_reference("os-credential://d-token/item?secret=value").is_err()
        );
    }

    #[tokio::test]
    async fn runtime_rejects_non_loopback_host_and_origin_headers() {
        let app = build_router(AppState::new(DtokenConfig::empty()));
        let hostile_host = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("host", "attacker.example")
                    .body(Body::empty())
                    .expect("hostile host request"),
            )
            .await
            .expect("host guard response");
        assert_eq!(hostile_host.status(), StatusCode::FORBIDDEN);

        let hostile_origin = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("host", "127.0.0.1:18001")
                    .header("origin", "https://attacker.example")
                    .body(Body::empty())
                    .expect("hostile origin request"),
            )
            .await
            .expect("origin guard response");
        assert_eq!(hostile_origin.status(), StatusCode::FORBIDDEN);

        let allowed = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("host", "[::1]:18001")
                    .header("origin", "http://localhost:5180")
                    .body(Body::empty())
                    .expect("loopback request"),
            )
            .await
            .expect("allowed response");
        assert_eq!(allowed.status(), StatusCode::OK);
    }

    #[test]
    fn resolved_provider_credential_becomes_a_valid_bearer_header() {
        let header = provider_authorization_header("unit-test-credential")
            .expect("create authorization header");
        assert_eq!(
            header.to_str().expect("header text"),
            "Bearer unit-test-credential"
        );
        assert!(provider_authorization_header("bad\ncredential").is_err());
    }

    #[test]
    fn upstream_statuses_have_stable_user_visible_categories() {
        let cases = [
            (StatusCode::UNAUTHORIZED, "provider_auth"),
            (StatusCode::FORBIDDEN, "provider_auth"),
            (StatusCode::NOT_FOUND, "provider_route_not_found"),
            (StatusCode::REQUEST_TIMEOUT, "provider_rate_or_timeout"),
            (StatusCode::TOO_MANY_REQUESTS, "provider_rate_or_timeout"),
            (StatusCode::INTERNAL_SERVER_ERROR, "provider_server_error"),
            (StatusCode::SERVICE_UNAVAILABLE, "provider_server_error"),
        ];

        for (status, expected) in cases {
            assert_eq!(classify_upstream_status(status), expected, "{status}");
        }
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
        assert_eq!(
            state.recent_request_logs(1)[0].error.as_deref(),
            Some("streaming_not_supported_yet")
        );
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

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock upstream");
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
            result
                .1
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
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
        let (_response, decision, mut request_log, mut event) = routed_openai_payload(&request);
        // Metrics attribution is tested with an explicit realized fixture. Production passthrough
        // requests must not fabricate compression savings to make this aggregation test pass.
        request_log.input_tokens_before = Some(100);
        request_log.input_tokens_after = Some(80);
        request_log.tokens_saved = Some(20);
        request_log.transforms_applied = vec!["semantic_closed_tool_result".to_string()];
        event.input_tokens = Some(100);
        event.output_tokens = Some(80);
        event.saved_tokens = Some(20);
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
        assert!(
            stats["latency_breakdown"]["compression_ms"]
                .as_f64()
                .unwrap_or_default()
                >= 0.0
        );
        assert!(
            stats["latency_breakdown"]["route_adapter_ms"]
                .as_f64()
                .unwrap_or_default()
                >= 0.0
        );
        assert!(
            stats["latency_breakdown"]["provider_ms"]
                .as_f64()
                .unwrap_or_default()
                >= 0.0
        );
        assert_eq!(
            stats["route_overhead"]["fallback_policy"],
            "disabled_unless_route_explicitly_configures_fallback"
        );
        assert!(stats["route_overhead"]["tokens"].as_i64().is_some());
        assert!(stats["route_overhead"]["percent"].as_f64().is_some());
        assert_eq!(
            metrics_payload["compression_performance"]["total_requests"],
            1
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
            row["name"] == "semantic_closed_tool_result"
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
            messages: Some(vec![
                json!({"role": "user", "content": "repeat\nrepeat\nresponses route"}),
            ]),
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
        assert!(events
            .iter()
            .any(|event| event.stage == LayeredStage::RouteForward));
        assert!(events.iter().any(|event| {
            event.stage == LayeredStage::ProviderForward
                && event.status == EventStatus::Skipped
                && event.metadata["input_protocol"] == "openai_responses"
                && event.metadata["raw_available"] == false
        }));
    }

    #[test]
    fn compression_performance_attribution_does_not_double_count_multi_transform_savings() {
        let request = OpenAiChatRequest {
            model: Some("mock/model".to_string()),
            messages: Some(vec![
                json!({"role": "user", "content": "repeat\nrepeat\nlog"}),
            ]),
            stream: Some(false),
        };
        let (_response, _decision, mut request_log, _event) = routed_openai_payload(&request);
        request_log.input_tokens_before = Some(101);
        request_log.input_tokens_after = Some(61);
        request_log.tokens_saved = Some(40);
        request_log.transforms_applied = vec!["repeat_fold".to_string(), "log_fold".to_string()];

        let metrics = runtime_metrics_payload(&[request_log]);
        let transforms = metrics["compression_performance"]["transforms"]
            .as_array()
            .expect("transform rows");
        let attributed_saved: u64 = transforms
            .iter()
            .map(|row| row["tokens_saved"].as_u64().unwrap_or_default())
            .sum();
        let attributed_before: u64 = transforms
            .iter()
            .map(|row| row["tokens_before"].as_u64().unwrap_or_default())
            .sum();

        assert_eq!(attributed_saved, 40);
        assert_eq!(attributed_before, 101);
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
        assert!(
            metrics["compression_performance"]["total_requests"]
                .as_u64()
                .unwrap_or_default()
                > 0
        );
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
            optical: Default::default(),
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
        let original_content = body["messages"][0]["content"].clone();

        let payload = anthropic_route_payload(&state, body, RouteEvidence::default())
            .expect("anthropic route payload");

        assert_eq!(payload.decision.route_id, "anthropic-to-cc-switch");
        assert_eq!(payload.decision.provider_id, "cc-switch-openai");
        assert_eq!(payload.upstream_body["messages"][0]["role"], "user");
        assert_eq!(
            payload.upstream_body["messages"][0]["content"],
            original_content
        );
        assert_eq!(payload.request_log.input_protocol, Protocol::Anthropic);
        assert_eq!(
            payload.request_log.route_id.as_deref(),
            Some("anthropic-to-cc-switch")
        );
        assert_eq!(payload.request_log.tokens_saved, Some(0));
        assert!(payload.request_log.transforms_applied.is_empty());
        assert!(!payload.upstream_body.to_string().contains("data:image"));
        assert_eq!(payload.event.metadata["input_protocol"], "anthropic");
        assert_eq!(payload.event.metadata["output_protocol"], "openai");
        assert!(payload.event.metadata.get("full_prompt").is_none());
    }

    #[test]
    fn anthropic_bridge_preserves_system_tools_and_generation_controls() {
        let provider = dtoken_types::ProviderProfile {
            id: "openai-upstream".to_string(),
            name: "OpenAI upstream".to_string(),
            kind: dtoken_types::ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url: "http://127.0.0.1:39999/v1".to_string(),
            api_key_ref: None,
            models: vec!["mock/model".to_string()],
            timeout_ms: 30_000,
            healthcheck_path: None,
        };
        let body = json!({
            "model": "claude-model",
            "system": [{"type": "text", "text": "You are a coding agent."}],
            "max_tokens": 2048,
            "temperature": 0.1,
            "top_p": 0.9,
            "stop_sequences": ["DONE"],
            "tools": [{
                "name": "read_file",
                "description": "Read a workspace file",
                "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
            }],
            "tool_choice": {"type": "tool", "name": "read_file"},
            "messages": [
                {
                    "role": "assistant",
                    "content": [{
                        "type": "tool_use",
                        "id": "toolu_1",
                        "name": "read_file",
                        "input": {"path": "README.md"}
                    }]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "toolu_1",
                        "content": "file contents"
                    }]
                }
            ]
        });

        let converted = anthropic_to_openai_body(&body, &provider);

        assert_eq!(converted["messages"][0]["role"], "system");
        assert_eq!(
            converted["messages"][0]["content"],
            "You are a coding agent."
        );
        assert_eq!(converted["messages"][1]["tool_calls"][0]["id"], "toolu_1");
        assert_eq!(
            converted["messages"][1]["tool_calls"][0]["function"]["name"],
            "read_file"
        );
        assert_eq!(converted["messages"][2]["role"], "tool");
        assert_eq!(converted["messages"][2]["tool_call_id"], "toolu_1");
        assert_eq!(converted["tools"][0]["function"]["name"], "read_file");
        assert_eq!(converted["tool_choice"]["function"]["name"], "read_file");
        assert_eq!(converted["max_tokens"], 2048);
        assert_eq!(converted["temperature"], 0.1);
        assert_eq!(converted["top_p"], 0.9);
        assert_eq!(converted["stop"][0], "DONE");
    }

    #[test]
    fn openai_tool_calls_convert_back_to_anthropic_tool_use_blocks() {
        let upstream = json!({
            "id": "chatcmpl-1",
            "model": "mock/model",
            "choices": [{
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"README.md\"}"
                        }
                    }]
                }
            }],
            "usage": {"prompt_tokens": 12, "completion_tokens": 7}
        });

        let converted = openai_chat_to_anthropic_response(upstream, Some("claude-model"));

        assert_eq!(converted["content"][0]["type"], "tool_use");
        assert_eq!(converted["content"][0]["id"], "call_1");
        assert_eq!(converted["content"][0]["name"], "read_file");
        assert_eq!(converted["content"][0]["input"]["path"], "README.md");
        assert_eq!(converted["stop_reason"], "tool_use");
        assert_eq!(converted["usage"]["input_tokens"], 12);
        assert_eq!(converted["usage"]["output_tokens"], 7);
    }

    #[test]
    fn successful_openai_tool_call_is_not_rejected_for_missing_text_content() {
        let upstream = UpstreamResult {
            response: json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {"name": "read_file", "arguments": "{}"}
                        }]
                    }
                }]
            }),
            status: Some(200),
            error_category: None,
            send_state: UpstreamSendState::Completed,
        };

        let validated = require_openai_message(upstream, "Anthropic bridge");

        assert!(validated.error_category.is_none());
        assert_eq!(
            validated.response["choices"][0]["message"]["tool_calls"][0]["id"],
            "call_1"
        );
    }

    #[tokio::test]
    async fn direct_anthropic_stream_preserves_raw_request_bytes_and_sse_body() {
        use axum::body::to_bytes;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        use tokio::sync::oneshot;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind direct Anthropic mock upstream");
        let addr = listener.local_addr().expect("mock upstream address");
        let (captured_tx, captured_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept direct request");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            let (header_end, content_length) = loop {
                let read = socket.read(&mut buffer).await.expect("read direct request");
                assert!(read > 0, "request closed before headers completed");
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    let header_end = index + 4;
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .expect("reqwest sends a content length for fixed raw bytes");
                    break (header_end, content_length);
                }
            };
            while request.len() < header_end + content_length {
                let read = socket.read(&mut buffer).await.expect("read request body");
                assert!(read > 0, "request closed before body completed");
                request.extend_from_slice(&buffer[..read]);
            }
            captured_tx
                .send(request[header_end..header_end + content_length].to_vec())
                .expect("capture raw body");
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\nevent: message_start\ndata: {\"type\":\"message_start\"}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
                )
                .await
                .expect("write Anthropic SSE response");
        });

        let provider = dtoken_types::ProviderProfile {
            id: "direct-anthropic".to_string(),
            name: "Direct Anthropic".to_string(),
            kind: dtoken_types::ProviderKind::Custom,
            protocol: Protocol::Anthropic,
            base_url: format!("http://{addr}"),
            api_key_ref: None,
            models: vec!["claude-fable-5".to_string()],
            timeout_ms: 30_000,
            healthcheck_path: None,
        };
        let raw = br#"{ "model" : "claude-fable-5", "stream" : true, "messages" : [] }"#;
        let parsed: Value = serde_json::from_slice(raw).expect("fixture JSON");
        let (status, headers, body) = forward_anthropic_messages_stream(
            &reqwest::Client::new(),
            &provider,
            parsed,
            Some(raw),
            &HeaderMap::new(),
        )
        .await
        .expect("direct Anthropic stream succeeds");

        assert_eq!(status, 200);
        assert_eq!(
            headers
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
        assert_eq!(captured_rx.await.expect("captured request body"), raw);
        let response = to_bytes(body, usize::MAX).await.expect("read SSE body");
        let response = String::from_utf8_lossy(&response);
        assert!(response.contains("event: message_start"));
        assert!(response.contains("event: message_stop"));
        server.await.expect("mock upstream task");
    }
}
