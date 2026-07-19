pub mod ccr_store;

use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path as AxumPath, Query, State},
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
use dtoken_config_patcher::{
    agent::{scan_current_process_agents, AgentDiscoverySnapshot},
    mock_patch_plan, stable_hash,
};
use dtoken_context_store::{CcrMetadataProjection, ContextStore, RequestProjection};
use dtoken_doctor::{run_basic_doctor, run_dev_no_touch_doctor};
use dtoken_event_ledger::{EventLedger, JsonlEventLedger, JsonlStore};
use dtoken_headroom_adapter::detect_headroom_status;
use dtoken_preprocessor::{
    detect_rtk_status, list_preprocessors, rtk_discover_report, rtk_gain_mock, rtk_session_report,
};
use dtoken_project_policy::{default_policy, RecommendationInput, RuleBasedRecommendationEngine};
use dtoken_runtime_pipeline::{
    PipelineInput, ReadyToSend, RecoveryPrepareReceipt, TransformArtifact,
};
use dtoken_types::{
    AgentSupportProfile, AvailabilityState, BaselineKind, CacheTokens, CapabilityEvidence,
    CapabilityRegistrySnapshot, ChainMode, CompressionPolicy, DtokenEvent, EffectiveOpticalState,
    EventSource, EventStatus, EvidenceFreshness, EvidenceSource, EvidenceValue, HealthResponse,
    InputTokenUsage, LayeredSavingsReport, LayeredStage, ModalityTokens, OpticalApplicabilityInput,
    OpticalCapabilitiesPublicSummary, OpticalCapabilityLifecyclePublicSummary, OpticalEndpoint,
    OpticalEventMetadataPublicSummary, OpticalEventPublicSummary, OpticalMode,
    OpticalPlanPublicSummary, OpticalPlanStatus, OpticalPreviewRequest, OpticalStatusPublicSummary,
    PayloadByteUsage, Protocol, ProtocolInjectionProfile, RequestLog, RequestScope,
    RequestScopeError, RouteDecision, SupportLevel, UpstreamSendState, UsageEvidence,
    UsageEvidenceKind, UsageMeasurement, UsageVector,
};
use futures_util::{Stream, TryStreamExt};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    env,
    path::Path as StdPath,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    task::{Context, Poll},
    time::{Duration, Instant},
};
use uuid::Uuid;

use ccr_store::{CcrStore, CcrStoreReceipt};

#[derive(Debug)]
struct PreparedCcr {
    receipt: Option<CcrStoreReceipt>,
    store_configured: bool,
}

#[derive(Debug)]
struct PreparedPipelinePayload {
    ready: ReadyToSend,
    fallback_reason: Option<&'static str>,
}

type RuntimeTransformGroup = (u64, u64, u64, u64);
type RuntimeDimensionGroup = (u64, u64, u64, u64, u64);
type RuntimeHistoryBucket = (u64, u64, u64, u64);
const MAX_RUNTIME_PERF_SAMPLES: usize = 256;

#[derive(Clone, Default)]
struct RuntimePerfSeries {
    durations_us: VecDeque<u64>,
    failures: u64,
    sync_count: u64,
}

impl RuntimePerfSeries {
    fn record(&mut self, duration_us: u64, ok: bool, sync_count: u64) {
        if self.durations_us.len() == MAX_RUNTIME_PERF_SAMPLES {
            self.durations_us.pop_front();
        }
        self.durations_us.push_back(duration_us);
        self.failures = self.failures.saturating_add(u64::from(!ok));
        self.sync_count = self.sync_count.saturating_add(sync_count);
    }

    fn payload(&self) -> Value {
        let mut sorted = self.durations_us.iter().copied().collect::<Vec<_>>();
        sorted.sort_unstable();
        json!({
            "samples": sorted.len(),
            "failures": self.failures,
            "sync_count": self.sync_count,
            "p50_us": percentile(&sorted, 50),
            "p95_us": percentile(&sorted, 95),
            "p99_us": percentile(&sorted, 99),
            "max_us": sorted.last().copied().unwrap_or_default()
        })
    }
}

#[derive(Clone, Default)]
struct RuntimePerformanceState {
    stages: BTreeMap<&'static str, RuntimePerfSeries>,
}

impl RuntimePerformanceState {
    fn record(&mut self, stage: &'static str, started_at: Instant, ok: bool, sync_count: u64) {
        self.stages.entry(stage).or_default().record(
            u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX),
            ok,
            sync_count,
        );
    }

    fn payload(&self) -> Value {
        Value::Object(
            self.stages
                .iter()
                .map(|(stage, series)| ((*stage).to_string(), series.payload()))
                .collect(),
        )
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = percentile.saturating_mul(sorted.len()).saturating_add(99) / 100;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

#[derive(Clone, Default)]
struct RuntimeSummaryReadModel {
    revision: u64,
    total_requests: u64,
    failed_requests: u64,
    input_before: u64,
    input_after: u64,
    output_tokens: u64,
    tokens_saved: u64,
    signed_net_tokens: i64,
    positive_saved_tokens: u64,
    penalty_tokens: u64,
    total_latency_ms: u64,
    latency_samples: u64,
    history: BTreeMap<u64, RuntimeHistoryBucket>,
    analytics_daily: BTreeMap<u64, RuntimeHistoryBucket>,
    by_transform: BTreeMap<String, RuntimeTransformGroup>,
    by_route: BTreeMap<String, RuntimeDimensionGroup>,
    by_provider: BTreeMap<String, RuntimeDimensionGroup>,
    by_policy: BTreeMap<String, RuntimeDimensionGroup>,
    by_agent: BTreeMap<String, RuntimeDimensionGroup>,
    by_model: BTreeMap<String, RuntimeDimensionGroup>,
    by_protocol: BTreeMap<String, RuntimeDimensionGroup>,
    by_chain: BTreeMap<String, RuntimeDimensionGroup>,
    by_error: BTreeMap<String, RuntimeDimensionGroup>,
    ccr_entries: u64,
    ccr_tokens_saved: u64,
    recent_ccr: VecDeque<Value>,
    latest_account_quotas: BTreeMap<String, Value>,
}

impl RuntimeSummaryReadModel {
    fn from_records(logs: &[RequestLog], ccr_metadata: &[Value]) -> Self {
        let mut model = Self::default();
        for log in logs {
            model.record_request(log);
        }
        for metadata in ccr_metadata {
            model.record_ccr(metadata);
        }
        model
    }

    fn record_request(&mut self, log: &RequestLog) {
        self.revision = self.revision.saturating_add(1);
        let tokens_before = usize_to_u64(log.input_tokens_before.unwrap_or_default());
        let tokens_after = usize_to_u64(log.input_tokens_after.unwrap_or_default());
        let output_tokens = usize_to_u64(log.output_tokens.unwrap_or_default());
        let tokens_saved = usize_to_u64(log.tokens_saved.unwrap_or_default());
        let latency_ms = log.latency_ms.unwrap_or_default();
        let failed = u64::from(log.error.is_some());
        let signed_savings = signed_context_savings(log);

        self.total_requests = self.total_requests.saturating_add(1);
        self.failed_requests = self.failed_requests.saturating_add(failed);
        self.input_before = self.input_before.saturating_add(tokens_before);
        self.input_after = self.input_after.saturating_add(tokens_after);
        self.output_tokens = self.output_tokens.saturating_add(output_tokens);
        self.tokens_saved = self.tokens_saved.saturating_add(tokens_saved);
        self.signed_net_tokens = self.signed_net_tokens.saturating_add(signed_savings);
        if signed_savings > 0 {
            self.positive_saved_tokens = self
                .positive_saved_tokens
                .saturating_add(signed_savings as u64);
        } else if signed_savings < 0 {
            self.penalty_tokens = self
                .penalty_tokens
                .saturating_add(signed_savings.unsigned_abs());
        }
        if let Some(observed_latency_ms) = log.latency_ms {
            self.total_latency_ms = self.total_latency_ms.saturating_add(observed_latency_ms);
            self.latency_samples = self.latency_samples.saturating_add(1);
        }
        if let Some(quota) = log.quota_metadata.as_ref() {
            if let Some(provider) = quota.get("provider").and_then(Value::as_str) {
                self.latest_account_quotas
                    .insert(provider.to_string(), quota.clone());
            }
        }

        let bucket_ms = (log.timestamp_ms / 3_600_000) * 3_600_000;
        let history = self.history.entry(bucket_ms).or_default();
        history.0 = history.0.saturating_add(1);
        history.1 = history.1.saturating_add(tokens_before);
        history.2 = history.2.saturating_add(tokens_after);
        history.3 = history.3.saturating_add(tokens_saved);
        while self.history.len() > 24 {
            self.history.pop_first();
        }
        let day_ms = (log.timestamp_ms / 86_400_000) * 86_400_000;
        let daily = self.analytics_daily.entry(day_ms).or_default();
        daily.0 = daily.0.saturating_add(1);
        daily.1 = daily.1.saturating_add(tokens_before);
        daily.2 = daily.2.saturating_add(tokens_after);
        daily.3 = daily.3.saturating_add(tokens_saved);
        while self.analytics_daily.len() > 30 {
            self.analytics_daily.pop_first();
        }

        let transform = if log.transforms_applied.is_empty() {
            "runtime_route".to_string()
        } else {
            log.transforms_applied.join("+")
        };
        let transform_group = self.by_transform.entry(transform).or_default();
        transform_group.0 = transform_group.0.saturating_add(1);
        transform_group.1 = transform_group.1.saturating_add(tokens_saved);
        transform_group.2 = transform_group.2.saturating_add(tokens_before);
        transform_group.3 = transform_group.3.saturating_add(latency_ms);

        add_runtime_group(
            &mut self.by_route,
            log.route_id
                .clone()
                .unwrap_or_else(|| "route:none".to_string()),
            tokens_saved,
            tokens_before,
            latency_ms,
            failed,
        );
        add_runtime_group(
            &mut self.by_provider,
            log.provider_id
                .clone()
                .unwrap_or_else(|| "provider:none".to_string()),
            tokens_saved,
            tokens_before,
            latency_ms,
            failed,
        );
        add_runtime_group(
            &mut self.by_policy,
            log.config_source
                .clone()
                .unwrap_or_else(|| "dtoken-owned-runtime".to_string()),
            tokens_saved,
            tokens_before,
            latency_ms,
            failed,
        );
        for (groups, label) in [
            (
                &mut self.by_agent,
                log.agent.clone().unwrap_or_else(|| "unknown".to_string()),
            ),
            (
                &mut self.by_model,
                log.model.clone().unwrap_or_else(|| "unknown".to_string()),
            ),
            (
                &mut self.by_protocol,
                protocol_label(&log.input_protocol).to_string(),
            ),
            (
                &mut self.by_chain,
                match log.chain_mode {
                    Some(ChainMode::Direct) => "direct".to_string(),
                    Some(ChainMode::ChainBridge) => "chain_bridge".to_string(),
                    None => "unknown".to_string(),
                },
            ),
        ] {
            add_runtime_group(
                groups,
                label,
                tokens_saved,
                tokens_before,
                latency_ms,
                failed,
            );
        }
        if let Some(error_type) = analytics_error_type(log.error.as_deref()) {
            add_runtime_group(
                &mut self.by_error,
                error_type,
                tokens_saved,
                tokens_before,
                latency_ms,
                1,
            );
        }
    }

    fn record_ccr(&mut self, metadata: &Value) {
        self.revision = self.revision.saturating_add(1);
        self.ccr_entries = self.ccr_entries.saturating_add(1);
        self.ccr_tokens_saved = self
            .ccr_tokens_saved
            .saturating_add(metadata["tokens_saved"].as_u64().unwrap_or_default());
        self.recent_ccr.push_front(metadata.clone());
        self.recent_ccr.truncate(10);
    }

    fn latency_breakdown(&self) -> Value {
        let compression_avg = (self.latency_samples > 0)
            .then(|| self.total_latency_ms as f64 / self.latency_samples as f64);
        json!({
            "source": "request_logs",
            "status": if compression_avg.is_some() { "partial" } else { "unavailable" },
            "model": "compression_only",
            "total_ms": null,
            "compression_ms": compression_avg,
            "route_adapter_ms": null,
            "provider_ms": null,
            "cc_switch_ms": null
        })
    }

    fn route_overhead(&self) -> Value {
        json!({
            "source": "unavailable",
            "status": "unavailable",
            "baseline": "provider_input_tokens - runtime_final_tokens",
            "runtime_final_tokens": self.input_after,
            "provider_input_tokens": null,
            "tokens": null,
            "percent": null,
            "fallback_policy": "disabled_unless_route_explicitly_configures_fallback"
        })
    }

    fn stats(&self) -> Value {
        let savings_percent = percent(self.tokens_saved, self.input_before);
        let avg_latency_ms = (self.latency_samples > 0)
            .then(|| self.total_latency_ms as f64 / self.latency_samples as f64);
        let latency_breakdown = self.latency_breakdown();
        let route_overhead = self.route_overhead();
        let layered = LayeredSavingsReport {
            rtk_savings: 0,
            headroom_savings: self.tokens_saved,
            end_to_end_savings: self.tokens_saved,
            attribution_confidence: if self.total_requests == 0 {
                "empty_runtime".to_string()
            } else {
                "runtime_estimated".to_string()
            },
            note: "Aggregated from d-token-owned runtime request logs; prompt and response bodies are not stored.".to_string(),
        };
        let analytics_daily = self
            .analytics_daily
            .iter()
            .map(|(bucket_ms, (requests, before, after, saved))| {
                json!({
                    "bucket_ms": bucket_ms,
                    "requests": requests,
                    "input_tokens": before,
                    "input_tokens_after": after,
                    "tokens_saved": saved
                })
            })
            .collect::<Vec<_>>();
        json!({
            "status": if self.total_requests == 0 { "empty_runtime" } else { "runtime" },
            "source": "dtoken-owned-runtime",
            "runtime_kind": "headroom-derived",
            "external_headroom_dependency": false,
            "compressions": self.total_requests,
            "tokens_saved": self.tokens_saved,
            "signed_net_tokens": self.signed_net_tokens,
            "positive_saved_tokens": self.positive_saved_tokens,
            "penalty_tokens": self.penalty_tokens,
            "savings_percent": savings_percent,
            "avg_latency_ms": avg_latency_ms,
            "avg_latency_semantics": "compression_only",
            "latency_breakdown": latency_breakdown,
            "route_overhead": route_overhead,
            "requests": { "total": self.total_requests, "failed": self.failed_requests },
            "tokens": {
                "input": self.input_before,
                "input_after": self.input_after,
                "output": self.output_tokens,
                "saved": self.tokens_saved,
                "signed_net": self.signed_net_tokens,
                "positive_saved": self.positive_saved_tokens,
                "penalty": self.penalty_tokens,
                "savings_percent": savings_percent
            },
            "layered": layered,
            "account_quotas": self.latest_account_quotas,
            "analytics": {
                "source": "dtoken-owned-runtime",
                "revision": self.total_requests,
                "coverage": { "loaded_limit": 10_000, "complete": self.total_requests < 10_000 },
                "agents": runtime_group_rows(self.by_agent.clone()),
                "models": runtime_group_rows(self.by_model.clone()),
                "protocols": runtime_group_rows(self.by_protocol.clone()),
                "chains": runtime_group_rows(self.by_chain.clone()),
                "errors": runtime_group_rows(self.by_error.clone()),
                "daily": analytics_daily
            }
        })
    }

    fn history(&self) -> Value {
        let history = self
            .history
            .iter()
            .rev()
            .map(|(bucket_ms, (requests, before, after, saved))| {
                json!({
                    "bucket_ms": bucket_ms,
                    "ts": bucket_ms.to_string(),
                    "requests": requests,
                    "before": before,
                    "after": after,
                    "saved": saved,
                    "ratio": if *before == 0 { 0.0 } else { *saved as f64 / *before as f64 },
                    "cost": 0.0
                })
            })
            .collect::<Vec<_>>();
        json!({
            "source": "dtoken-owned-runtime",
            "runtime_kind": "headroom-derived",
            "external_headroom_dependency": false,
            "history": history
        })
    }

    fn metrics(&self, stats: &Value) -> Value {
        let transforms = self
            .by_transform
            .iter()
            .map(|(name, (requests, saved, before, latency))| {
                json!({
                    "name": name,
                    "requests": requests,
                    "tokens_saved": saved,
                    "tokens_before": before,
                    "avg_latency_ms": if *requests == 0 { 0.0 } else { *latency as f64 / *requests as f64 },
                    "savings_percent": percent(*saved, *before)
                })
            })
            .collect::<Vec<_>>();
        json!({
            "available": true,
            "source": "dtoken-owned-runtime",
            "runtime_kind": "headroom-derived",
            "external_headroom_dependency": false,
            "compression_performance": {
                "source": "request_logs",
                "total_requests": self.total_requests,
                "transforms": transforms,
                "signed_net_tokens": self.signed_net_tokens,
                "positive_saved_tokens": self.positive_saved_tokens,
                "penalty_tokens": self.penalty_tokens
            },
            "route_performance": {
                "source": "request_logs",
                "routes": runtime_group_rows(self.by_route.clone()),
                "providers": runtime_group_rows(self.by_provider.clone()),
                "policies": runtime_group_rows(self.by_policy.clone()),
                "latency_breakdown": self.latency_breakdown(),
                "route_overhead": self.route_overhead()
            },
            "prometheus_text": runtime_metrics_text_from_stats(stats)
        })
    }
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn percent(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        (numerator as f64 / denominator as f64) * 100.0
    }
}

fn prepare_json_pipeline_payload(
    scope: RequestScope,
    baseline: &Value,
    candidate: &Value,
    transforms: &[String],
    prepared_ccr: &PreparedCcr,
) -> anyhow::Result<PreparedPipelinePayload> {
    let baseline_bytes = serde_json::to_vec(baseline)?;
    let candidate_bytes = serde_json::to_vec(candidate)?;
    let prepared = PipelineInput::new(scope, baseline_bytes, MAX_REQUEST_BODY_BYTES)
        .map_err(|error| anyhow::anyhow!("invalid pipeline input: {error:?}"))?
        .prepare();
    if transforms.is_empty() {
        return Ok(PreparedPipelinePayload {
            ready: prepared.into_baseline_ready("no_transform_selected"),
            fallback_reason: None,
        });
    }
    let recovery = prepared_ccr.receipt.as_ref().and_then(|receipt| {
        RecoveryPrepareReceipt::new(receipt.recovery_ref.clone(), receipt.expires_at_unix).ok()
    });
    let Some(recovery) = recovery else {
        return Ok(PreparedPipelinePayload {
            ready: prepared.into_baseline_ready("pipeline_recovery_unavailable"),
            fallback_reason: Some("pipeline_recovery_unavailable"),
        });
    };
    let artifact = match TransformArtifact::new(
        prepared.baseline_digest(),
        candidate_bytes,
        transforms.to_vec(),
    ) {
        Ok(artifact) => artifact,
        Err(_) => {
            return Ok(PreparedPipelinePayload {
                ready: prepared.into_baseline_ready("pipeline_artifact_invalid"),
                fallback_reason: Some("pipeline_artifact_invalid"),
            });
        }
    };
    match prepared
        .clone()
        .with_candidate(artifact, Some(recovery))
        .and_then(|candidate| candidate.validate(valid_json_pipeline_transition))
    {
        Ok(validated) => Ok(PreparedPipelinePayload {
            ready: validated.into_ready_to_send(),
            fallback_reason: None,
        }),
        Err(_) => Ok(PreparedPipelinePayload {
            ready: prepared.into_baseline_ready("pipeline_validation_failed"),
            fallback_reason: Some("pipeline_validation_failed"),
        }),
    }
}

fn valid_json_pipeline_transition(baseline: &[u8], candidate: &[u8]) -> bool {
    let Ok(baseline) = serde_json::from_slice::<Value>(baseline) else {
        return false;
    };
    let Ok(candidate) = serde_json::from_slice::<Value>(candidate) else {
        return false;
    };
    baseline.is_object()
        && candidate.is_object()
        && baseline.get("model").and_then(Value::as_str)
            == candidate.get("model").and_then(Value::as_str)
}

/// Bounds all JSON extraction before LLM or Optical request parsing/allocation.
pub const MAX_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
const BACKEND_CAPABILITY_REVISION: &str = "headroom-backend-capabilities-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextStoreCapabilityState {
    NotConfigured,
    Ready { schema_version: i64 },
    Failed,
}

fn backend_capability_registry(
    agent_discovery: Option<&AgentDiscoverySnapshot>,
    context_store: ContextStoreCapabilityState,
) -> CapabilityRegistrySnapshot {
    let mut capabilities = [
        ("cache-aligner", "provider_cache_plan_not_implemented"),
        ("ccr-pre-send", "pre_send_recovery_protocol_not_implemented"),
        ("code-compressor", "language_quality_gates_incomplete"),
        ("content-router", "full_codec_routing_incomplete"),
        ("diff-codec", "diff_codec_not_implemented"),
        ("html-codec", "html_codec_not_implemented"),
        (
            "input-image-compressor",
            "input_image_codec_not_implemented",
        ),
        ("integrated-rtk", "native_rtk_pipeline_not_implemented"),
        ("json-codec", "json_quality_gates_incomplete"),
        ("log-codec", "log_quality_gates_incomplete"),
        ("mcp", "mcp_transport_not_implemented"),
        ("memory-store", "typed_project_memory_not_implemented"),
        ("optical-active", "active_release_gates_incomplete"),
        ("output-shaping", "output_quality_gates_not_implemented"),
        ("rust-library", "public_runtime_library_not_implemented"),
        ("search-codec", "search_codec_not_implemented"),
        ("smart-learning", "learning_backend_contract_incomplete"),
        ("text-codec", "text_quality_gates_incomplete"),
        ("typescript-sdk", "typescript_sdk_not_implemented"),
    ]
    .into_iter()
    .map(|(id, reason)| {
        CapabilityEvidence::unavailable(id, BACKEND_CAPABILITY_REVISION, reason)
            .expect("static capability evidence must be valid")
    })
    .collect::<Vec<_>>();

    capabilities.push(match agent_discovery {
        Some(snapshot) => CapabilityEvidence::new(
            "agent-registry",
            AvailabilityState::Ready,
            BACKEND_CAPABILITY_REVISION,
            None,
            Some(snapshot.evidence_revision.clone()),
            EvidenceFreshness::Live,
            vec![if snapshot.agents.is_empty() {
                "agent_scan_complete_no_agents_detected".to_string()
            } else {
                "agent_scan_complete".to_string()
            }],
            Some(snapshot.observed_at_ms),
        )
        .expect("validated Agent discovery must produce valid capability evidence"),
        None => CapabilityEvidence::unavailable(
            "agent-registry",
            BACKEND_CAPABILITY_REVISION,
            "agent_scan_environment_unavailable",
        )
        .expect("static capability evidence must be valid"),
    });
    capabilities.push(match context_store {
        ContextStoreCapabilityState::Ready { schema_version } => CapabilityEvidence::new(
            "context-store",
            AvailabilityState::Ready,
            BACKEND_CAPABILITY_REVISION,
            None,
            Some(format!("context-store-schema-v{schema_version}")),
            EvidenceFreshness::Live,
            vec!["context_store_open".to_string()],
            Some(dtoken_event_ledger::now_ms().max(1)),
        )
        .expect("validated ContextStore evidence must be valid"),
        ContextStoreCapabilityState::NotConfigured => CapabilityEvidence::unavailable(
            "context-store",
            BACKEND_CAPABILITY_REVISION,
            "context_store_not_configured",
        )
        .expect("static capability evidence must be valid"),
        ContextStoreCapabilityState::Failed => CapabilityEvidence::unavailable(
            "context-store",
            BACKEND_CAPABILITY_REVISION,
            "context_store_open_failed",
        )
        .expect("static capability evidence must be valid"),
    });

    CapabilityRegistrySnapshot::new(
        BACKEND_CAPABILITY_REVISION,
        dtoken_event_ledger::now_ms().max(1),
        capabilities,
    )
    .expect("static backend capability registry must be valid")
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<DtokenConfig>,
    pub config_status: Arc<RuntimeConfigStatus>,
    pub default_doctor_port: u16,
    ledger: Arc<Mutex<EventLedger>>,
    route_decisions: Arc<Mutex<Vec<Value>>>,
    request_logs: Arc<Mutex<Vec<RequestLog>>>,
    ccr_metadata: Arc<Mutex<Vec<Value>>>,
    ccr_private_store: Option<Arc<CcrStore>>,
    memory_records: Arc<Mutex<Vec<Value>>>,
    runtime_summary: Arc<Mutex<RuntimeSummaryReadModel>>,
    runtime_performance: Arc<Mutex<RuntimePerformanceState>>,
    runtime_store: Option<Arc<RuntimeStore>>,
    context_store: Option<Arc<Mutex<ContextStore>>>,
    agent_discovery: Arc<Option<AgentDiscoverySnapshot>>,
    http_client: reqwest::Client,
    hybrid_engine: Arc<HybridContextEngine>,
    optical_config_issues: Arc<Vec<dtoken_config::OpticalConfigIssue>>,
    capabilities: Arc<CapabilityRegistrySnapshot>,
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
        let agent_discovery = scan_current_process_agents().ok();
        let capabilities = Arc::new(backend_capability_registry(
            agent_discovery.as_ref(),
            ContextStoreCapabilityState::NotConfigured,
        ));
        Self {
            config: Arc::new(config),
            config_status: Arc::new(config_status),
            default_doctor_port: 18001,
            ledger: Arc::new(Mutex::new(EventLedger::new())),
            route_decisions: Arc::new(Mutex::new(vec![])),
            request_logs: Arc::new(Mutex::new(vec![])),
            ccr_metadata: Arc::new(Mutex::new(vec![])),
            ccr_private_store: None,
            memory_records: Arc::new(Mutex::new(vec![])),
            runtime_summary: Arc::new(Mutex::new(RuntimeSummaryReadModel::default())),
            runtime_performance: Arc::new(Mutex::new(RuntimePerformanceState::default())),
            runtime_store: None,
            context_store: None,
            agent_discovery: Arc::new(agent_discovery),
            http_client: reqwest::Client::new(),
            hybrid_engine,
            optical_config_issues,
            capabilities,
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
        let runtime_dir = dir.as_ref();
        let store = Arc::new(RuntimeStore::new(runtime_dir));
        let mut state = Self::new_with_config_status(config, config_status);
        state.ccr_private_store = configured_ccr_private_store(runtime_dir);
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
        state.rebuild_runtime_summary();
        match ContextStore::open(runtime_dir.join("runtime-state.sqlite3")) {
            Ok(mut context_store) => {
                let schema_version = context_store.schema_version().unwrap_or_default();
                let capabilities = Arc::new(backend_capability_registry(
                    state.agent_discovery.as_ref().as_ref(),
                    ContextStoreCapabilityState::Ready { schema_version },
                ));
                if persist_context_store_evidence(
                    &mut context_store,
                    &capabilities,
                    state.agent_discovery.as_ref().as_ref(),
                )
                .is_ok()
                {
                    state.capabilities = capabilities;
                    state.context_store = Some(Arc::new(Mutex::new(context_store)));
                } else {
                    state.capabilities = Arc::new(backend_capability_registry(
                        state.agent_discovery.as_ref().as_ref(),
                        ContextStoreCapabilityState::Failed,
                    ));
                }
            }
            Err(_) => {
                state.capabilities = Arc::new(backend_capability_registry(
                    state.agent_discovery.as_ref().as_ref(),
                    ContextStoreCapabilityState::Failed,
                ));
            }
        }
        state.runtime_store = Some(store);
        state
    }

    pub fn new_persistent_with_ccr_key(
        config: DtokenConfig,
        dir: impl AsRef<StdPath>,
        key: [u8; 32],
    ) -> anyhow::Result<Self> {
        let runtime_dir = dir.as_ref();
        let mut state = Self::new_persistent(config, runtime_dir);
        state.ccr_private_store = Some(Arc::new(CcrStore::new(
            runtime_dir.join("ccr-private"),
            key,
        )?));
        Ok(state)
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

    pub fn retrieve_ccr_original(&self, recovery_ref: &str) -> anyhow::Result<Vec<u8>> {
        self.ccr_private_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("CCR private storage is disabled"))?
            .retrieve(recovery_ref)
    }

    pub fn delete_ccr_original(&self, recovery_ref: &str) -> anyhow::Result<bool> {
        self.ccr_private_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("CCR private storage is disabled"))?
            .delete(recovery_ref)
    }

    fn recent_memory_records(&self, limit: usize) -> Vec<Value> {
        self.memory_records
            .lock()
            .map(|items| items.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    fn rebuild_runtime_summary(&self) {
        let logs = self
            .request_logs
            .lock()
            .map(|items| items.clone())
            .unwrap_or_default();
        let ccr_metadata = self
            .ccr_metadata
            .lock()
            .map(|items| items.clone())
            .unwrap_or_default();
        if let Ok(mut summary) = self.runtime_summary.lock() {
            *summary = RuntimeSummaryReadModel::from_records(&logs, &ccr_metadata);
        }
    }

    fn record_runtime_performance(
        &self,
        stage: &'static str,
        started_at: Instant,
        ok: bool,
        sync_count: u64,
    ) {
        if let Ok(mut performance) = self.runtime_performance.lock() {
            performance.record(stage, started_at, ok, sync_count);
        }
    }

    fn runtime_performance_payload(&self) -> Value {
        self.runtime_performance
            .lock()
            .map(|performance| performance.payload())
            .unwrap_or_else(|_| json!({}))
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
        if let Some(store) = &self.runtime_store {
            store.append_event(&event)?;
        }
        self.ledger
            .lock()
            .map_err(|_| anyhow::anyhow!("event ledger lock poisoned"))?
            .record(event)?;
        Ok(())
    }

    #[cfg(test)]
    fn record_routed_request(
        &self,
        decision: &RouteDecision,
        request_log: RequestLog,
        event: DtokenEvent,
    ) {
        self.record_routed_request_inner(decision, request_log, event, None, false);
    }

    fn prepare_ccr_for_request(
        &self,
        decision: &RouteDecision,
        request_log: &RequestLog,
        original_body: &[u8],
    ) -> anyhow::Result<PreparedCcr> {
        let store_configured = self.ccr_private_store.is_some();
        let required = !request_log.transforms_applied.is_empty();
        if !required {
            return Ok(PreparedCcr {
                receipt: None,
                store_configured,
            });
        }
        let store = self.ccr_private_store.as_ref().ok_or_else(|| {
            anyhow::anyhow!("CCR private storage is required before optimized send")
        })?;
        let recovery_ref = format!("ccr:{}", decision.request_id);
        let receipt = store.store(&recovery_ref, original_body)?;
        Ok(PreparedCcr {
            receipt: Some(receipt),
            store_configured,
        })
    }

    fn record_routed_request_with_ccr_receipt(
        &self,
        decision: &RouteDecision,
        request_log: RequestLog,
        event: DtokenEvent,
        prepared: &PreparedCcr,
    ) {
        self.record_routed_request_inner(
            decision,
            request_log,
            event,
            prepared.receipt.as_ref(),
            prepared.store_configured,
        );
    }

    fn record_routed_request_inner(
        &self,
        decision: &RouteDecision,
        request_log: RequestLog,
        event: DtokenEvent,
        ccr_receipt: Option<&CcrStoreReceipt>,
        ccr_store_configured: bool,
    ) {
        let terminal_started_at = Instant::now();
        let ccr = ccr_metadata_from_request(
            decision,
            &request_log,
            &event,
            ccr_receipt,
            ccr_store_configured,
        );
        let route_decision = route_decision_to_value(decision);
        let mut sync_count = 0_u64;
        if let Some(store) = &self.runtime_store {
            let started_at = Instant::now();
            let result = store.append_event(&event);
            self.record_runtime_performance(
                "terminal_event_sync",
                started_at,
                result.is_ok(),
                u64::from(result.is_ok()),
            );
            if result.is_err() {
                self.record_runtime_performance(
                    "terminal_commit_total",
                    terminal_started_at,
                    false,
                    sync_count,
                );
                return;
            }
            sync_count += 1;

            let started_at = Instant::now();
            let result = store.append_route_decision(&route_decision);
            self.record_runtime_performance(
                "terminal_route_sync",
                started_at,
                result.is_ok(),
                u64::from(result.is_ok()),
            );
            if result.is_err() {
                self.record_runtime_performance(
                    "terminal_commit_total",
                    terminal_started_at,
                    false,
                    sync_count,
                );
                return;
            }
            sync_count += 1;

            let started_at = Instant::now();
            let result = store.append_request_log(&request_log);
            self.record_runtime_performance(
                "terminal_request_log_sync",
                started_at,
                result.is_ok(),
                u64::from(result.is_ok()),
            );
            if result.is_err() {
                self.record_runtime_performance(
                    "terminal_commit_total",
                    terminal_started_at,
                    false,
                    sync_count,
                );
                return;
            }
            sync_count += 1;

            let started_at = Instant::now();
            let result = store.append_ccr_metadata(&ccr);
            self.record_runtime_performance(
                "terminal_ccr_sync",
                started_at,
                result.is_ok(),
                u64::from(result.is_ok()),
            );
            if result.is_err() {
                self.record_runtime_performance(
                    "terminal_commit_total",
                    terminal_started_at,
                    false,
                    sync_count,
                );
                return;
            }
            sync_count += 1;
        }
        if let Ok(mut route_decisions) = self.route_decisions.lock() {
            route_decisions.push(route_decision);
        }
        if let Ok(mut request_logs) = self.request_logs.lock() {
            request_logs.push(request_log.clone());
        }
        if let Ok(mut ledger) = self.ledger.lock() {
            let _ = ledger.record(event.clone());
        }
        if let Ok(mut items) = self.ccr_metadata.lock() {
            items.push(ccr.clone());
        }
        if let Ok(mut summary) = self.runtime_summary.lock() {
            summary.record_request(&request_log);
            summary.record_ccr(&ccr);
        }
        if let Some(store) = &self.context_store {
            let lock_started_at = Instant::now();
            if let Ok(store) = store.lock() {
                self.record_runtime_performance(
                    "context_store_lock_wait",
                    lock_started_at,
                    true,
                    0,
                );
                let projection_started_at = Instant::now();
                let request_result = store.upsert_request(&RequestProjection {
                    request_id: decision.request_id.to_string(),
                    route_id: decision.route_id.clone(),
                    provider_id: decision.provider_id.clone(),
                    agent_id: request_log.agent.clone(),
                    project_id: event.project_id.clone(),
                    status: event_status_label(&event.status).to_string(),
                    input_units: request_log
                        .input_tokens_before
                        .and_then(|value| u64::try_from(value).ok()),
                    output_units: request_log
                        .output_tokens
                        .and_then(|value| u64::try_from(value).ok()),
                    saved_units: request_log
                        .tokens_saved
                        .and_then(|value| u64::try_from(value).ok()),
                    observed_at_ms: event.timestamp_ms,
                });
                let ccr_result = store.upsert_ccr_metadata(&CcrMetadataProjection {
                    recovery_ref: format!("ccr:{}", decision.request_id),
                    request_id: decision.request_id.to_string(),
                    expires_at_ms: ccr_receipt
                        .map(|receipt| receipt.expires_at_unix.saturating_mul(1_000)),
                    plaintext_bytes: ccr_receipt
                        .and_then(|receipt| u64::try_from(receipt.plaintext_bytes).ok()),
                    state: if ccr_receipt.is_some() {
                        "prepared".to_string()
                    } else if request_log.transforms_applied.is_empty() {
                        "not_required".to_string()
                    } else {
                        "unavailable".to_string()
                    },
                    updated_at_ms: event.timestamp_ms,
                });
                self.record_runtime_performance(
                    "context_store_projection",
                    projection_started_at,
                    request_result.is_ok() && ccr_result.is_ok(),
                    0,
                );
            } else {
                self.record_runtime_performance(
                    "context_store_lock_wait",
                    lock_started_at,
                    false,
                    0,
                );
            }
        }
        self.record_runtime_performance(
            "terminal_commit_total",
            terminal_started_at,
            true,
            sync_count,
        );
    }
}

fn event_status_label(status: &EventStatus) -> &'static str {
    match status {
        EventStatus::Ok => "ok",
        EventStatus::Failed => "failed",
        EventStatus::Skipped => "skipped",
        EventStatus::Passthrough => "passthrough",
        EventStatus::Disabled => "disabled",
    }
}

fn persist_context_store_evidence(
    context_store: &mut ContextStore,
    capabilities: &CapabilityRegistrySnapshot,
    agent_discovery: Option<&AgentDiscoverySnapshot>,
) -> anyhow::Result<()> {
    for capability in &capabilities.capabilities {
        context_store.upsert_capability(capability)?;
    }
    if let Some(snapshot) = agent_discovery {
        context_store.replace_agent_snapshot(snapshot)?;
    }
    Ok(())
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

fn configured_ccr_private_store(runtime_dir: &StdPath) -> Option<Arc<CcrStore>> {
    if env::var("DTOKEN_CCR_ENABLED").ok().as_deref() != Some("1") {
        return None;
    }
    let key = load_or_create_ccr_key(runtime_dir).ok()?;
    CcrStore::new(runtime_dir.join("ccr-private"), key)
        .ok()
        .map(Arc::new)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn load_or_create_ccr_key(runtime_dir: &StdPath) -> anyhow::Result<[u8; 32]> {
    use ring::rand::{SecureRandom, SystemRandom};

    let identity = stable_hash(&runtime_dir.to_string_lossy());
    let account = format!("runtime-{}", &identity[..24]);
    let entry = keyring::Entry::new("d-token.ccr", &account)?;
    match entry.get_password() {
        Ok(encoded) => decode_ccr_key(&encoded),
        Err(keyring::Error::NoEntry) => {
            let mut key = [0_u8; 32];
            SystemRandom::new()
                .fill(&mut key)
                .map_err(|_| anyhow::anyhow!("CCR key generation failed"))?;
            entry.set_password(&encode_ccr_key(&key))?;
            Ok(key)
        }
        Err(error) => Err(anyhow::anyhow!("CCR credential lookup failed: {error}")),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn load_or_create_ccr_key(_runtime_dir: &StdPath) -> anyhow::Result<[u8; 32]> {
    Err(anyhow::anyhow!(
        "CCR private storage requires a supported OS credential store"
    ))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn encode_ccr_key(key: &[u8; 32]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn decode_ccr_key(encoded: &str) -> anyhow::Result<[u8; 32]> {
    if encoded.len() != 64
        || !encoded
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(anyhow::anyhow!("CCR credential has an invalid encoding"));
    }
    let mut key = [0_u8; 32];
    for (index, slot) in key.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16)
            .map_err(|_| anyhow::anyhow!("CCR credential has an invalid encoding"))?;
    }
    Ok(key)
}

fn ccr_metadata_from_request(
    decision: &RouteDecision,
    request_log: &RequestLog,
    event: &DtokenEvent,
    receipt: Option<&CcrStoreReceipt>,
    private_store_configured: bool,
) -> Value {
    json!({
        "recovery_ref": format!("ccr:{}", decision.request_id),
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
        "raw_available": receipt.is_some(),
        "raw_storage": if receipt.is_some() { "encrypted-private-store" } else if private_store_configured { "not-stored-or-failed" } else { "disabled" },
        "raw_bytes": receipt.map(|item| item.plaintext_bytes),
        "expires_at_unix": receipt.map(|item| item.expires_at_unix),
        "note": if receipt.is_some() { "Encrypted CCR original is available only through the in-process authorized retrieval boundary; it is excluded from ordinary HTTP responses, logs, Ledger, and exports." } else { "CCR metadata only; raw prompt/response/CCR body is not stored by default." }
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
        .route("/dashboard/summary", get(dashboard_summary))
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
        .route("/capabilities", get(backend_capabilities))
        .route("/config/active", get(active_config))
        .route("/chain/status", get(chain_status))
        .route("/route-decisions/recent", get(recent_route_decisions))
        .route("/logs/recent", get(recent_logs))
        .route("/analytics/requests", get(analytics_requests))
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
    let effective = effective_optical_state(&state);
    Json(OpticalStatusPublicSummary {
        schema_version: 3,
        enabled: effective.enabled,
        configured_mode: effective.configured_mode,
        effective_mode: effective.effective_mode.clone(),
        default_mode: effective.effective_mode,
        allow_active: effective.active_reachable,
        active_reachable: effective.active_reachable,
        active_execution_observed: effective.active_execution_observed,
        public_summary_only: effective.public_summary_only,
        fail_closed: true,
        renderer_ready: true,
        patcher_ready: true,
        post_validator_ready: true,
        signed_accounting_ready: true,
        representation_plan_ready: true,
        responses_shadow_inventory_ready: true,
        dynamic_page_accounting_ready: true,
        active_direct_routes: effective.active_direct_routes,
        verified_attested_capabilities: effective.verified_attested_capabilities,
        rejected_capabilities: effective.rejected_capabilities,
        config_issue_count: effective.config_issue_count,
    })
}

fn effective_optical_state(state: &AppState) -> EffectiveOpticalState {
    let policy = state.config.optical.global_policy.clone();
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &state.config.optical,
        dtoken_event_ledger::now_ms(),
    );
    let active_route_count = active_direct_anthropic_route_count(&state.config, &registry);
    let active_reachable = policy.allow_active && active_route_count > 0;
    let effective_mode = if active_reachable {
        OpticalMode::ToolResultsOnly
    } else {
        OpticalMode::ShadowPreview
    };
    let active_execution_observed = state.runtime_store.is_some()
        && state.recent_events(10_000).iter().any(|event| {
            event.status == EventStatus::Ok
                && event.context_accounting.as_ref().is_some_and(|accounting| {
                    accounting.disposition == dtoken_types::OptimizationDisposition::Active
                        && accounting.upstream_state == UpstreamSendState::Completed
                })
        });
    EffectiveOpticalState {
        enabled: policy.enabled,
        configured_mode: policy.default_mode,
        effective_mode,
        active_reachable,
        active_execution_observed,
        public_summary_only: policy.public_summary_only || !active_reachable,
        active_direct_routes: active_route_count,
        verified_attested_capabilities: registry.verified_rule_count(),
        rejected_capabilities: registry.rejected_rule_count(),
        config_issue_count: state.optical_config_issues.len(),
    }
}

async fn optical_capabilities(
    State(state): State<AppState>,
) -> Json<OpticalCapabilitiesPublicSummary> {
    let effective = effective_optical_state(&state);
    let active = effective.active_reachable;
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
            .filter(|artifact| artifact.passes_derived_quality_gate())
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
    let mut by_request = std::collections::HashMap::<Uuid, DtokenEvent>::new();
    let mut unscoped = Vec::new();
    for event in state.recent_events(200).into_iter().filter(|event| {
        matches!(
            &event.stage,
            LayeredStage::OpticalPreview
                | LayeredStage::OpticalPlan
                | LayeredStage::OpticalRender
                | LayeredStage::OpticalPatch
                | LayeredStage::OpticalValidate
                | LayeredStage::OpticalFallback
                | LayeredStage::OpticalComplete
                | LayeredStage::UpstreamSendCommitted
        ) || event.metadata.get("optical_phase").is_some()
    }) {
        if let Some(request_id) = event.request_id {
            match by_request.get(&request_id) {
                Some(current)
                    if optical_event_priority(current) > optical_event_priority(&event)
                        || (optical_event_priority(current) == optical_event_priority(&event)
                            && current.timestamp_ms >= event.timestamp_ms) => {}
                _ => {
                    by_request.insert(request_id, event);
                }
            }
        } else {
            unscoped.push(event);
        }
    }
    let mut selected = by_request.into_values().chain(unscoped).collect::<Vec<_>>();
    selected.sort_by_key(|event| std::cmp::Reverse(event.timestamp_ms));
    let events = selected
        .into_iter()
        .map(optical_event_public_summary)
        .collect();
    Json(events)
}

fn optical_event_priority(event: &DtokenEvent) -> u8 {
    let phase = event
        .metadata
        .get("optical_phase")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let terminal = event
        .metadata
        .get("terminal")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if terminal && matches!(phase, "upstream_completed" | "upstream_failed" | "fallback") {
        50
    } else if matches!(phase, "send_committed")
        || event.stage == LayeredStage::UpstreamSendCommitted
    {
        40
    } else if matches!(phase, "artifact_validated") || event.stage == LayeredStage::OpticalValidate
    {
        30
    } else if event.stage == LayeredStage::OpticalFallback {
        25
    } else {
        10
    }
}

fn optical_event_public_summary(event: DtokenEvent) -> OpticalEventPublicSummary {
    let metadata = &event.metadata;
    let accounting = event.context_accounting.clone().or_else(|| {
        metadata
            .get("accounting")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
    });
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
        LayeredStage::UpstreamSendCommitted => "upstream_send_committed",
        _ => "optical_runtime",
    }
    .to_string();
    let claimed_status = metadata
        .get("final_status")
        .cloned()
        .and_then(|value| serde_json::from_value::<OpticalPlanStatus>(value).ok());
    let terminal = metadata
        .get("terminal")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let phase = metadata
        .get("optical_phase")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let active_completion = stored_kind == "active"
        && event.status == EventStatus::Ok
        && terminal
        && phase == "upstream_completed"
        && accounting.as_ref().is_some_and(|accounting| {
            accounting.disposition == dtoken_types::OptimizationDisposition::Active
                && accounting.upstream_state == UpstreamSendState::Completed
        });
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
    public_metadata.attribution_basis = accounting
        .as_ref()
        .map(|accounting| accounting.attribution_basis);
    public_metadata.upstream_state = accounting
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
        runtime_identity: "dtoken-runtime-local".to_string(),
        runtime_instance_nonce: std::env::var("DTOKEN_RUNTIME_INSTANCE_NONCE")
            .ok()
            .filter(|value| !value.trim().is_empty()),
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
        "optical": {
            "schema_version": state.config.optical.schema_version,
            "global_policy": state.config.optical.global_policy,
            "route_policies": state.config.optical.route_policies,
        },
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
    let account_quotas = stats
        .get("account_quotas")
        .cloned()
        .unwrap_or_else(|| json!({}));
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
        "account_quotas": account_quotas,
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
    quota_metadata: Option<Value>,
}

#[derive(Debug, Clone)]
struct CompressionSummary {
    original_tokens: u64,
    compressed_tokens: u64,
    tokens_saved: u64,
    savings_percent: f64,
    elapsed_us: u128,
    transforms_applied: Vec<String>,
    fallback_reason_code: Option<String>,
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
    scope: RequestScope,
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
    compress_for_route_with_cache_evidence(context, policy, body, None)
}

fn compress_for_route_with_cache_evidence(
    context: Option<&ResolvedRouteOptimizationContext>,
    policy: CompressionPolicy,
    body: Value,
    cache_evidence: Option<&dtoken_compress::context_pipeline::ProviderCacheEvidence>,
) -> CompressionSummary {
    let result = context
        .map(|context| {
            CompressionEngine::new().compress_resolved_with_cache_evidence(
                &context.compression_context,
                policy.clone(),
                body.clone(),
                cache_evidence,
            )
        })
        .unwrap_or_else(|| CompressionEngine::new().compress(policy, body.clone()))
        .map(|result| (result, None))
        .unwrap_or_else(|_| {
            (
                dtoken_types::CompressionResult {
                    original_tokens: 0,
                    compressed_tokens: 0,
                    tokens_saved: 0,
                    elapsed_us: 1,
                    transforms_applied: Vec::new(),
                    body,
                    context_accounting: None,
                },
                Some("compression_error_fallback".to_string()),
            )
        });
    let (result, fallback_reason_code) = result;
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
        fallback_reason_code,
        body: result.body,
        context_accounting: result.context_accounting,
    }
}

fn compression_inventory_diagnostics(
    context: &dtoken_compress::ResolvedOptimizationContext,
    body: &Value,
) -> Value {
    let conversation_items = body
        .get("messages")
        .or_else(|| body.get("input"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let role_sequence = conversation_items
        .iter()
        .map(|item| {
            item.get("role")
                .and_then(Value::as_str)
                .or_else(|| match item.get("type").and_then(Value::as_str) {
                    Some("function_call") => Some("assistant"),
                    Some("function_call_output") => Some("tool"),
                    _ => None,
                })
                .unwrap_or("unknown")
        })
        .collect::<Vec<_>>();
    let content_kinds = conversation_items
        .iter()
        .map(|item| {
            json_value_kind(
                item.get("content")
                    .or_else(|| item.get("output"))
                    .unwrap_or(&Value::Null),
            )
        })
        .collect::<Vec<_>>();

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RequestAttribution {
    agent: Option<String>,
    instance_id: Option<String>,
    project_id: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn resolve_configured_request_scope(
    config: &DtokenConfig,
    route: &dtoken_types::RouteProfile,
    provider: &dtoken_types::ProviderProfile,
    request_id: Uuid,
    model: Option<&str>,
    input_protocol: Protocol,
    output_protocol: Protocol,
) -> Result<RequestScope, RequestScopeError> {
    let config_json =
        serde_json::to_string(config).map_err(|_| RequestScopeError::ConfigSerialization)?;
    let configured_instance = config
        .find_instance(&route.instance_id)
        .filter(|instance| instance.route_id == route.id);
    let agent = configured_instance
        .map(|instance| {
            EvidenceValue::known(instance.agent.clone(), EvidenceSource::ConfiguredInstance)
        })
        .unwrap_or_else(|| EvidenceValue::unknown("agent_identity_unavailable"));
    let instance = if route.instance_id.trim().is_empty() {
        EvidenceValue::unknown("instance_identity_unavailable")
    } else {
        EvidenceValue::known(route.instance_id.clone(), EvidenceSource::ConfiguredRoute)
    };
    let effective_model = model
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            provider
                .models
                .iter()
                .find(|value| !value.trim().is_empty())
                .map(String::as_str)
        })
        .unwrap_or_default();

    RequestScope::new(
        request_id,
        agent,
        EvidenceValue::unknown("project_identity_unavailable"),
        EvidenceValue::unknown("session_identity_unavailable"),
        instance,
        route.id.clone(),
        provider.id.clone(),
        effective_model,
        input_protocol,
        output_protocol,
        stable_hash(&config_json),
    )
}

fn request_attribution_from_scope(scope: &RequestScope) -> RequestAttribution {
    RequestAttribution {
        agent: scope.agent.value.clone(),
        instance_id: scope.instance.value.clone(),
        project_id: scope.project.value.clone(),
    }
}

fn attach_request_scope(event: &mut DtokenEvent, scope: &RequestScope) {
    if let (Some(metadata), Ok(scope)) =
        (event.metadata.as_object_mut(), serde_json::to_value(scope))
    {
        metadata.insert("request_scope".to_string(), scope);
    }
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
    attribution: &RequestAttribution,
) -> RequestLog {
    RequestLog {
        request_id,
        timestamp_ms: dtoken_event_ledger::now_ms(),
        agent: attribution.agent.clone(),
        instance_id: attribution.instance_id.clone(),
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
        quota_metadata: None,
        context_accounting: compression.context_accounting.clone(),
    }
}

fn build_ledger_event(
    request_id: Uuid,
    decision: &RouteDecision,
    input_protocol: Protocol,
    output_protocol: Protocol,
    compression: &CompressionSummary,
    attribution: &RequestAttribution,
) -> DtokenEvent {
    DtokenEvent {
        event_id: Uuid::new_v4(),
        timestamp_ms: dtoken_event_ledger::now_ms(),
        project_id: attribution.project_id.clone(),
        agent_id: attribution.agent.clone(),
        instance_id: attribution.instance_id.clone(),
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
            "compression_fallback_reason": compression.fallback_reason_code,
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
        quota_metadata: None,
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
    request_log.quota_metadata = upstream.quota_metadata.clone();

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
    if let Some(metadata) = event.metadata.as_object_mut() {
        let phase = match upstream.send_state {
            UpstreamSendState::Completed => "upstream_completed",
            UpstreamSendState::FailedBeforeCommit | UpstreamSendState::FailedAfterCommit => {
                "upstream_failed"
            }
            UpstreamSendState::SendCommitted
            | UpstreamSendState::ResponseStarted
            | UpstreamSendState::StreamStarted => "send_committed",
            UpstreamSendState::NotAttempted => "planned",
        };
        metadata.insert("optical_phase".to_string(), json!(phase));
        metadata.insert(
            "terminal".to_string(),
            json!(matches!(
                upstream.send_state,
                UpstreamSendState::Completed
                    | UpstreamSendState::FailedBeforeCommit
                    | UpstreamSendState::FailedAfterCommit
            )),
        );
        let kind = metadata
            .get("hybrid_kind")
            .cloned()
            .unwrap_or_else(|| json!("passthrough"));
        metadata.insert("kind".to_string(), kind);
        if let Some(accounting) = request_log.context_accounting.as_ref() {
            metadata.insert("disposition".to_string(), json!(accounting.disposition));
            metadata.insert("accounting".to_string(), json!(accounting));
        }
    }

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
        })
        .or_else(|| {
            usage
                .pointer("/input_tokens_details/cached_tokens")
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

fn provider_cache_observation_from_response(
    scope: &RequestScope,
    response: &Value,
    kind: dtoken_types::ProviderCacheObservationKind,
    observed_at_ms: u64,
) -> Option<dtoken_types::ProviderCacheObservation> {
    let usage = response.get("usage")?;
    let (read, creation, creation_5m, creation_1h) = match kind {
        dtoken_types::ProviderCacheObservationKind::AnthropicUsage => (
            usage.get("cache_read_input_tokens").and_then(Value::as_u64),
            usage
                .get("cache_creation_input_tokens")
                .and_then(Value::as_u64),
            usage
                .pointer("/cache_creation/ephemeral_5m_input_tokens")
                .and_then(Value::as_u64),
            usage
                .pointer("/cache_creation/ephemeral_1h_input_tokens")
                .and_then(Value::as_u64),
        ),
        dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage => (
            usage
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(Value::as_u64),
            None,
            None,
            None,
        ),
        dtoken_types::ProviderCacheObservationKind::OpenAiResponsesUsage => (
            usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(Value::as_u64),
            None,
            None,
            None,
        ),
    };
    dtoken_types::ProviderCacheObservation::new(
        scope.route_id.clone(),
        scope.provider_id.clone(),
        scope.model.clone(),
        scope.output_protocol.clone(),
        scope.config_revision.clone(),
        kind,
        read,
        creation,
        creation_5m,
        creation_1h,
        "provider-cache-observation-v1",
        observed_at_ms,
    )
    .ok()
}

fn persist_provider_cache_observation(
    state: &AppState,
    scope: &RequestScope,
    upstream: &UpstreamResult,
    kind: dtoken_types::ProviderCacheObservationKind,
) {
    if upstream.error_category.is_some() || upstream.send_state != UpstreamSendState::Completed {
        return;
    }
    let Some(observation) = provider_cache_observation_from_response(
        scope,
        &upstream.response,
        kind,
        dtoken_event_ledger::now_ms().max(1),
    ) else {
        return;
    };
    if let Some(store) = &state.context_store {
        if let Ok(store) = store.lock() {
            let _ = store.upsert_provider_cache_observation(&observation);
        }
    }
}

fn configured_provider_cache_evidence(
    state: &AppState,
    scope: &RequestScope,
) -> Option<dtoken_compress::context_pipeline::ProviderCacheEvidence> {
    configured_provider_cache_evidence_at(state, scope, dtoken_event_ledger::now_ms().max(1))
}

fn configured_provider_cache_evidence_at(
    state: &AppState,
    scope: &RequestScope,
    now_ms: u64,
) -> Option<dtoken_compress::context_pipeline::ProviderCacheEvidence> {
    let policy = state.config.find_provider_cache_policy(
        &scope.route_id,
        &scope.provider_id,
        &scope.model,
    )?;
    if !policy.is_well_formed() {
        return None;
    }
    let observation = state
        .context_store
        .as_ref()?
        .lock()
        .ok()?
        .provider_cache_observation(
            &scope.route_id,
            &scope.provider_id,
            &scope.model,
            &scope.output_protocol,
            &scope.config_revision,
        )
        .ok()??;
    let expires_at_ms = observation
        .observed_at_ms
        .checked_add(policy.ttl_seconds.checked_mul(1_000)?)?;
    if now_ms < observation.observed_at_ms || now_ms > expires_at_ms {
        return None;
    }
    let strategy = match policy.strategy {
        dtoken_types::ProviderCachePolicyStrategy::AnthropicExplicit
            if observation.kind == dtoken_types::ProviderCacheObservationKind::AnthropicUsage =>
        {
            dtoken_compress::context_pipeline::ProviderCacheStrategy::AnthropicExplicit
        }
        dtoken_types::ProviderCachePolicyStrategy::OpenAiAutomatic
            if matches!(
                observation.kind,
                dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage
                    | dtoken_types::ProviderCacheObservationKind::OpenAiResponsesUsage
            ) =>
        {
            dtoken_compress::context_pipeline::ProviderCacheStrategy::OpenAiAutomatic
        }
        dtoken_types::ProviderCachePolicyStrategy::GoogleExplicit
            if matches!(
                observation.kind,
                dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage
                    | dtoken_types::ProviderCacheObservationKind::OpenAiResponsesUsage
            ) =>
        {
            dtoken_compress::context_pipeline::ProviderCacheStrategy::GoogleExplicit
        }
        _ => return None,
    };
    let source = serde_json::to_string(&(policy, &observation)).ok()?;
    Some(dtoken_compress::context_pipeline::ProviderCacheEvidence {
        route_id: scope.route_id.clone(),
        provider_id: scope.provider_id.clone(),
        model: scope.model.clone(),
        strategy,
        ttl_seconds: Some(policy.ttl_seconds),
        min_prefix_bytes: usize::try_from(policy.min_prefix_bytes).ok()?,
        source_revision: stable_hash(&source),
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
        quota_metadata: upstream.quota_metadata,
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
        quota_metadata: upstream.quota_metadata,
    }
}

fn codex_quota_metadata(headers: &reqwest::header::HeaderMap) -> Option<Value> {
    fn number(headers: &reqwest::header::HeaderMap, name: &str) -> Option<f64> {
        headers
            .get(name)?
            .to_str()
            .ok()?
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
    }
    fn window(headers: &reqwest::header::HeaderMap, prefix: &str) -> Option<Value> {
        let used = number(headers, &format!("x-codex-{prefix}-used-percent"))?;
        let minutes = number(headers, &format!("x-codex-{prefix}-window-minutes"))?;
        if !(0.0..=100.0).contains(&used) || minutes <= 0.0 {
            return None;
        }
        let reset_at = number(headers, &format!("x-codex-{prefix}-reset-at"));
        Some(json!({
            "used_percent": used,
            "window_minutes": minutes,
            "reset_at": reset_at
        }))
    }
    let primary = window(headers, "primary");
    let secondary = window(headers, "secondary");
    if primary.is_none() && secondary.is_none() {
        return None;
    }
    Some(json!({
        "provider": "codex",
        "source": "codex-response-headers",
        "captured_at": dtoken_event_ledger::now_ms().max(1),
        "stale_after_ms": 900_000_u64,
        "primary": primary,
        "secondary": secondary,
        "privacy": "Allowlisted numeric quota metadata only; raw headers and account identifiers are not stored."
    }))
}

#[derive(Default)]
struct CodexQuotaCacheEntry {
    payload: Option<Value>,
    captured_at: Option<Instant>,
    last_attempt: Option<Instant>,
    in_flight: bool,
}

static CODEX_QUOTA_CACHE: OnceLock<Mutex<HashMap<String, CodexQuotaCacheEntry>>> = OnceLock::new();
static CLAUDE_QUOTA_CACHE: OnceLock<Mutex<HashMap<String, CodexQuotaCacheEntry>>> = OnceLock::new();
static ACTIVE_CODEX_QUOTA_KEY: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static ACTIVE_CLAUDE_QUOTA_KEY: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[derive(Default)]
struct QuotaGlobalGate {
    last_attempt: Option<Instant>,
    in_flight: bool,
}

static CODEX_QUOTA_GLOBAL_GATE: OnceLock<Mutex<QuotaGlobalGate>> = OnceLock::new();
static CLAUDE_QUOTA_GLOBAL_GATE: OnceLock<Mutex<QuotaGlobalGate>> = OnceLock::new();
const MAX_QUOTA_CACHE_ACCOUNTS: usize = 8;

fn set_active_quota_key(target: &OnceLock<Mutex<Option<String>>>, key: Option<String>) {
    if let Ok(mut active) = target.get_or_init(|| Mutex::new(None)).lock() {
        *active = key;
    }
}

fn active_quota_key(target: &OnceLock<Mutex<Option<String>>>) -> Option<String> {
    target.get_or_init(|| Mutex::new(None)).lock().ok()?.clone()
}

fn quota_cache_entry_mut<'a>(
    cache: &'a mut HashMap<String, CodexQuotaCacheEntry>,
    key: &str,
) -> &'a mut CodexQuotaCacheEntry {
    if !cache.contains_key(key) && cache.len() >= MAX_QUOTA_CACHE_ACCOUNTS {
        if let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_attempt.or(entry.captured_at))
            .map(|(key, _)| key.clone())
        {
            cache.remove(&oldest);
        }
    }
    cache.entry(key.to_string()).or_default()
}

fn active_cached_quota(
    cache: &HashMap<String, CodexQuotaCacheEntry>,
    key: Option<&str>,
    maximum_age: Duration,
) -> Option<Value> {
    let entry = cache.get(key?)?;
    entry
        .captured_at
        .filter(|captured_at| captured_at.elapsed() <= maximum_age)?;
    entry.payload.clone()
}

fn claim_quota_global_gate(
    target: &OnceLock<Mutex<QuotaGlobalGate>>,
    minimum_interval: Duration,
) -> bool {
    target
        .get_or_init(|| Mutex::new(QuotaGlobalGate::default()))
        .lock()
        .ok()
        .is_some_and(|mut gate| {
            let due = gate
                .last_attempt
                .is_none_or(|last_attempt| last_attempt.elapsed() >= minimum_interval);
            if gate.in_flight || !due {
                return false;
            }
            gate.in_flight = true;
            gate.last_attempt = Some(Instant::now());
            true
        })
}

fn release_quota_global_gate(target: &OnceLock<Mutex<QuotaGlobalGate>>) {
    if let Ok(mut gate) = target
        .get_or_init(|| Mutex::new(QuotaGlobalGate::default()))
        .lock()
    {
        gate.in_flight = false;
    }
}

fn codex_wham_quota_metadata(body: &Value, account_ref: &str) -> Option<Value> {
    fn window(body: &Value, pointer: &str) -> Option<Value> {
        let value = body.pointer(pointer)?;
        let used = value.get("used_percent")?.as_f64()?;
        let seconds = value.get("limit_window_seconds")?.as_f64()?;
        if !(0.0..=100.0).contains(&used) || seconds <= 0.0 {
            return None;
        }
        Some(json!({
            "used_percent": used,
            "window_minutes": seconds / 60.0,
            "reset_at": value.get("reset_at").and_then(Value::as_f64)
        }))
    }
    let primary = window(body, "/rate_limit/primary_window");
    let secondary = window(body, "/rate_limit/secondary_window");
    if primary.is_none() && secondary.is_none() {
        return None;
    }
    Some(json!({
        "provider": "codex",
        "source": "codex-wham-usage",
        "captured_at": dtoken_event_ledger::now_ms().max(1),
        "stale_after_ms": 900_000_u64,
        "account_ref": account_ref,
        "primary": primary,
        "secondary": secondary,
        "privacy": "Allowlisted numeric quota metadata only; the raw usage response, credential, and account identifier are not stored."
    }))
}

fn cached_or_schedule_codex_wham_quota(
    client: &reqwest::Client,
    credential: Option<&str>,
    account_id: Option<&str>,
) -> Option<Value> {
    set_active_quota_key(&ACTIVE_CODEX_QUOTA_KEY, None);
    let credential = credential
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let account_id = account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let account_ref = opaque_codex_account_ref_value(account_id);
    let cache_key = codex_quota_cache_key(credential, account_id);
    set_active_quota_key(&ACTIVE_CODEX_QUOTA_KEY, Some(cache_key.clone()));
    let cache = CODEX_QUOTA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let (cached, refresh_due) = cache.lock().ok().map_or((None, false), |mut cache| {
        let entry = quota_cache_entry_mut(&mut cache, &cache_key);
        let cached = entry
            .captured_at
            .filter(|captured_at| captured_at.elapsed() <= Duration::from_secs(15 * 60))
            .and_then(|_| entry.payload.clone());
        let refresh_due = entry
            .last_attempt
            .is_none_or(|last_attempt| last_attempt.elapsed() >= Duration::from_secs(60));
        (cached, refresh_due && !entry.in_flight)
    });
    let gate_claimed =
        refresh_due && claim_quota_global_gate(&CODEX_QUOTA_GLOBAL_GATE, Duration::from_secs(5));
    let should_schedule = gate_claimed
        && cache.lock().ok().is_some_and(|mut cache| {
            let entry = quota_cache_entry_mut(&mut cache, &cache_key);
            if entry.in_flight {
                return false;
            }
            entry.in_flight = true;
            entry.last_attempt = Some(Instant::now());
            true
        });
    if gate_claimed && !should_schedule {
        release_quota_global_gate(&CODEX_QUOTA_GLOBAL_GATE);
    }
    if should_schedule {
        let client = client.clone();
        let credential = credential.to_string();
        let account_id = account_id.to_string();
        let account_ref_for_task = account_ref.clone();
        let cache_key_for_task = cache_key.clone();
        tokio::spawn(async move {
            let authorization = provider_authorization_header(&credential).ok();
            let payload = match authorization {
                Some(authorization) => client
                    .get("https://chatgpt.com/backend-api/wham/usage")
                    .timeout(Duration::from_secs(3))
                    .header(reqwest::header::AUTHORIZATION, authorization)
                    .header("chatgpt-account-id", &account_id)
                    .send()
                    .await
                    .ok()
                    .filter(|response| response.status().is_success()),
                None => None,
            };
            let payload = match payload {
                Some(response) => response
                    .json::<Value>()
                    .await
                    .ok()
                    .and_then(|body| codex_wham_quota_metadata(&body, &account_ref_for_task)),
                None => None,
            };
            if let Ok(mut cache) = CODEX_QUOTA_CACHE
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
            {
                let entry = quota_cache_entry_mut(&mut cache, &cache_key_for_task);
                entry.in_flight = false;
                if let Some(payload) = payload {
                    entry.payload = Some(payload);
                    entry.captured_at = Some(Instant::now());
                }
            }
            release_quota_global_gate(&CODEX_QUOTA_GLOBAL_GATE);
        });
    }
    cached
}

fn claude_usage_metadata(body: &Value, account_ref: &str) -> Option<Value> {
    fn window(body: &Value, key: &str) -> Option<Value> {
        let value = body.get(key)?;
        let utilization = value.get("utilization")?.as_f64()?;
        if !(0.0..=100.0).contains(&utilization) {
            return None;
        }
        Some(json!({
            "utilization_pct": utilization,
            "resets_at": value.get("resets_at").and_then(Value::as_str)
        }))
    }
    let five_hour = window(body, "five_hour");
    let seven_day = window(body, "seven_day");
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }
    Some(json!({
        "provider": "claude",
        "source": "claude-usage-metadata",
        "account_ref": account_ref,
        "captured_at": dtoken_event_ledger::now_ms().max(1),
        "stale_after_ms": 900_000_u64,
        "five_hour": five_hour,
        "seven_day": seven_day,
        "privacy": "Allowlisted numeric quota metadata only; the raw usage response and OAuth credential are not stored."
    }))
}

fn schedule_claude_usage_refresh(client: &reqwest::Client, credential: Option<&str>) {
    set_active_quota_key(&ACTIVE_CLAUDE_QUOTA_KEY, None);
    let Some(credential) = credential
        .map(str::trim)
        .filter(|value| value.starts_with("sk-ant-oat"))
    else {
        return;
    };
    let cache_key = opaque_codex_account_ref_value(credential);
    set_active_quota_key(&ACTIVE_CLAUDE_QUOTA_KEY, Some(cache_key.clone()));
    let cache = CLAUDE_QUOTA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let refresh_due = cache.lock().ok().is_some_and(|mut cache| {
        let entry = quota_cache_entry_mut(&mut cache, &cache_key);
        entry
            .last_attempt
            .is_none_or(|last_attempt| last_attempt.elapsed() >= Duration::from_secs(5 * 60))
            && !entry.in_flight
    });
    let gate_claimed =
        refresh_due && claim_quota_global_gate(&CLAUDE_QUOTA_GLOBAL_GATE, Duration::from_secs(30));
    let should_schedule = gate_claimed
        && cache.lock().ok().is_some_and(|mut cache| {
            let entry = quota_cache_entry_mut(&mut cache, &cache_key);
            if entry.in_flight {
                return false;
            }
            entry.in_flight = true;
            entry.last_attempt = Some(Instant::now());
            true
        });
    if gate_claimed && !should_schedule {
        release_quota_global_gate(&CLAUDE_QUOTA_GLOBAL_GATE);
    }
    if !should_schedule {
        return;
    }
    let client = client.clone();
    let credential = credential.to_string();
    let cache_key_for_task = cache_key.clone();
    let account_ref_for_task = cache_key;
    tokio::spawn(async move {
        let authorization = provider_authorization_header(&credential).ok();
        let payload = match authorization {
            Some(authorization) => client
                .get("https://api.anthropic.com/api/oauth/usage")
                .timeout(Duration::from_secs(3))
                .header(reqwest::header::AUTHORIZATION, authorization)
                .header("anthropic-beta", "oauth-2025-04-20")
                .send()
                .await
                .ok()
                .filter(|response| response.status().is_success()),
            None => None,
        };
        let payload = match payload {
            Some(response) => response
                .json::<Value>()
                .await
                .ok()
                .and_then(|body| claude_usage_metadata(&body, &account_ref_for_task)),
            None => None,
        };
        if let Ok(mut cache) = CLAUDE_QUOTA_CACHE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
        {
            let entry = quota_cache_entry_mut(&mut cache, &cache_key_for_task);
            entry.in_flight = false;
            if let Some(payload) = payload {
                entry.payload = Some(payload);
                entry.captured_at = Some(Instant::now());
            }
        }
        release_quota_global_gate(&CLAUDE_QUOTA_GLOBAL_GATE);
    });
}

fn active_quota_snapshot(
    active_target: &OnceLock<Mutex<Option<String>>>,
    cache_target: &OnceLock<Mutex<HashMap<String, CodexQuotaCacheEntry>>>,
) -> (Option<String>, Option<Value>) {
    let key = active_quota_key(active_target);
    let payload = cache_target
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|cache| {
            active_cached_quota(&cache, key.as_deref(), Duration::from_secs(15 * 60))
        });
    (key, payload)
}

fn quota_payload_matches_active_account(payload: &Value, account_ref: Option<&str>) -> bool {
    let Some(account_ref) = account_ref else {
        return false;
    };
    if payload.get("account_ref").and_then(Value::as_str) != Some(account_ref) {
        return false;
    }
    let Some(captured_at) = payload.get("captured_at").and_then(Value::as_u64) else {
        return false;
    };
    let stale_after_ms = payload
        .get("stale_after_ms")
        .and_then(Value::as_u64)
        .unwrap_or(900_000)
        .min(900_000);
    dtoken_event_ledger::now_ms()
        .checked_sub(captured_at)
        .is_some_and(|age| age <= stale_after_ms)
}

fn quota_payload_matches_active_identity(
    payload: &Value,
    account_ref: Option<&str>,
    account_context_ref: Option<&str>,
) -> bool {
    let identity_matches = match account_context_ref {
        Some(context_ref) => {
            payload.get("account_context_ref").and_then(Value::as_str) == Some(context_ref)
        }
        None => quota_payload_matches_active_account(payload, account_ref),
    };
    if !identity_matches {
        return false;
    }
    let Some(captured_at) = payload.get("captured_at").and_then(Value::as_u64) else {
        return false;
    };
    let stale_after_ms = payload
        .get("stale_after_ms")
        .and_then(Value::as_u64)
        .unwrap_or(900_000)
        .min(900_000);
    dtoken_event_ledger::now_ms()
        .checked_sub(captured_at)
        .is_some_and(|age| age <= stale_after_ms)
}

fn reconcile_provider_quota(
    account_quotas: &mut serde_json::Map<String, Value>,
    provider: &str,
    active_account_ref: Option<&str>,
    active_account_context_ref: Option<&str>,
    cached: Option<&Value>,
) {
    if let Some(cached) = cached {
        account_quotas.insert(provider.to_string(), cached.clone());
        return;
    }
    let keep_historical = account_quotas.get(provider).is_some_and(|quota| {
        quota_payload_matches_active_identity(quota, active_account_ref, active_account_context_ref)
    });
    if !keep_historical {
        account_quotas.remove(provider);
    }
}

fn with_cached_account_quota(mut payload: Value) -> Value {
    let (codex_key, codex) = active_quota_snapshot(&ACTIVE_CODEX_QUOTA_KEY, &CODEX_QUOTA_CACHE);
    let codex_account_ref = codex_key
        .as_deref()
        .and_then(|key| key.rsplit_once(':').map(|(_, account_ref)| account_ref));
    let codex_account_context_ref = codex_key.as_deref().map(opaque_quota_context_ref);
    let (claude_key, claude) = active_quota_snapshot(&ACTIVE_CLAUDE_QUOTA_KEY, &CLAUDE_QUOTA_CACHE);
    let Some(object) = payload.as_object_mut() else {
        return payload;
    };
    if let Some(account_quotas) = object
        .entry("account_quotas")
        .or_insert_with(|| json!({}))
        .as_object_mut()
    {
        reconcile_provider_quota(
            account_quotas,
            "codex",
            codex_account_ref,
            codex_account_context_ref.as_deref(),
            codex.as_ref(),
        );
        reconcile_provider_quota(
            account_quotas,
            "claude",
            claude_key.as_deref(),
            None,
            claude.as_ref(),
        );
    }
    if let Some(stats) = object.get_mut("stats").and_then(Value::as_object_mut) {
        if let Some(account_quotas) = stats
            .entry("account_quotas")
            .or_insert_with(|| json!({}))
            .as_object_mut()
        {
            reconcile_provider_quota(
                account_quotas,
                "codex",
                codex_account_ref,
                codex_account_context_ref.as_deref(),
                codex.as_ref(),
            );
            reconcile_provider_quota(
                account_quotas,
                "claude",
                claude_key.as_deref(),
                None,
                claude.as_ref(),
            );
        }
    }
    payload
}

fn opaque_codex_account_ref(headers: &HeaderMap) -> Option<String> {
    let account_id = headers.get("chatgpt-account-id")?.to_str().ok()?.trim();
    if account_id.is_empty() {
        return None;
    }
    Some(opaque_codex_account_ref_value(account_id))
}

fn opaque_codex_account_ref_value(account_id: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, account_id.as_bytes());
    let short = digest
        .as_ref()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("codex-{short}")
}

fn codex_quota_cache_key(credential: &str, account_id: &str) -> String {
    format!(
        "{}:{}",
        opaque_codex_account_ref_value(credential),
        opaque_codex_account_ref_value(account_id)
    )
}

fn opaque_quota_context_ref(cache_key: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, cache_key.as_bytes());
    let short = digest
        .as_ref()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("quota-context-{short}")
}

fn chatgpt_account_id(headers: Option<&HeaderMap>) -> Option<&str> {
    headers?
        .get("chatgpt-account-id")?
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn attach_quota_account_ref(metadata: &mut Option<Value>, account_ref: Option<&str>) {
    let (Some(metadata), Some(account_ref)) = (metadata.as_mut(), account_ref) else {
        return;
    };
    if let Some(object) = metadata.as_object_mut() {
        object.insert("account_ref".to_string(), json!(account_ref));
    }
}

fn attach_quota_context_ref(metadata: &mut Option<Value>, context_ref: Option<&str>) {
    let (Some(metadata), Some(context_ref)) = (metadata.as_mut(), context_ref) else {
        return;
    };
    if let Some(object) = metadata.as_object_mut() {
        object.insert("account_context_ref".to_string(), json!(context_ref));
    }
}

fn is_chatgpt_upstream(base_url: &str) -> bool {
    url::Url::parse(base_url)
        .ok()
        .and_then(|url| {
            url.host_str().map(|host| {
                host.eq_ignore_ascii_case("chatgpt.com") || host.ends_with(".chatgpt.com")
            })
        })
        .unwrap_or(false)
}

fn is_anthropic_upstream(base_url: &str) -> bool {
    url::Url::parse(base_url)
        .ok()
        .and_then(|url| {
            url.host_str().map(|host| {
                host.eq_ignore_ascii_case("api.anthropic.com") || host.ends_with(".anthropic.com")
            })
        })
        .unwrap_or(false)
}

fn anthropic_oauth_token<'a>(
    credential: Option<&'a str>,
    headers: &'a HeaderMap,
) -> Option<&'a str> {
    credential
        .map(str::trim)
        .filter(|value| value.starts_with("sk-ant-oat"))
        .or_else(|| {
            headers
                .get(reqwest::header::AUTHORIZATION)?
                .to_str()
                .ok()?
                .trim()
                .strip_prefix("Bearer ")
                .map(str::trim)
                .filter(|value| value.starts_with("sk-ant-oat"))
        })
}

async fn forward_openai_chat(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
    inbound_headers: Option<&HeaderMap>,
) -> UpstreamResult {
    let credential = match resolve_provider_credential(provider).await {
        Ok(credential) => credential,
        Err(error) => return error,
    };
    let quota_context_ref = credential
        .as_deref()
        .zip(chatgpt_account_id(inbound_headers))
        .map(|(credential, account_id)| {
            opaque_quota_context_ref(&codex_quota_cache_key(credential, account_id))
        });
    let wham_quota = is_chatgpt_upstream(&provider.base_url)
        .then(|| {
            cached_or_schedule_codex_wham_quota(
                client,
                credential.as_deref(),
                chatgpt_account_id(inbound_headers),
            )
        })
        .flatten();
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
    if is_chatgpt_upstream(&provider.base_url) {
        if let Some(account_id) = inbound_headers
            .and_then(|headers| headers.get("chatgpt-account-id"))
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.trim().is_empty())
        {
            request = request.header("chatgpt-account-id", account_id);
        }
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let status_code = status.as_u16();
            let mut quota_metadata = is_chatgpt_upstream(&provider.base_url)
                .then(|| codex_quota_metadata(response.headers()))
                .flatten()
                .or(wham_quota);
            attach_quota_context_ref(&mut quota_metadata, quota_context_ref.as_deref());
            match response.json::<Value>().await {
                Ok(value) if status.is_success() => UpstreamResult {
                    response: value,
                    status: Some(status_code),
                    error_category: None,
                    send_state: UpstreamSendState::Completed,
                    quota_metadata,
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
                    quota_metadata,
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
                    quota_metadata,
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
                quota_metadata: None,
            }
        }
    }
}

async fn forward_openai_chat_stream(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
    inbound_headers: Option<&HeaderMap>,
) -> Result<(u16, axum::http::HeaderMap, Body, Option<Value>), UpstreamResult> {
    let credential = resolve_provider_credential(provider).await?;
    let quota_context_ref = credential
        .as_deref()
        .zip(chatgpt_account_id(inbound_headers))
        .map(|(credential, account_id)| {
            opaque_quota_context_ref(&codex_quota_cache_key(credential, account_id))
        });
    let wham_quota = is_chatgpt_upstream(&provider.base_url)
        .then(|| {
            cached_or_schedule_codex_wham_quota(
                client,
                credential.as_deref(),
                chatgpt_account_id(inbound_headers),
            )
        })
        .flatten();
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
    if is_chatgpt_upstream(&provider.base_url) {
        if let Some(account_id) = inbound_headers
            .and_then(|headers| headers.get("chatgpt-account-id"))
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.trim().is_empty())
        {
            request = request.header("chatgpt-account-id", account_id);
        }
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let status_code = status.as_u16();
            let mut quota_metadata = is_chatgpt_upstream(&provider.base_url)
                .then(|| codex_quota_metadata(response.headers()))
                .flatten()
                .or(wham_quota);
            attach_quota_context_ref(&mut quota_metadata, quota_context_ref.as_deref());
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
                    quota_metadata,
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
            Ok((
                status_code,
                headers,
                Body::from_stream(stream),
                quota_metadata,
            ))
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
                quota_metadata: None,
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
    if is_anthropic_upstream(&provider.base_url) {
        schedule_claude_usage_refresh(
            client,
            anthropic_oauth_token(credential.as_deref(), inbound_headers),
        );
    }
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
                    quota_metadata: None,
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
                    quota_metadata: None,
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
                    quota_metadata: None,
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
                quota_metadata: None,
            }
        }
    }
}

type UpstreamByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;

struct StreamTerminalLedgerGuard {
    state: AppState,
    request_id: Uuid,
    provider_id: String,
    upstream_status: u16,
    kind: &'static str,
    accounting: Option<dtoken_types::HybridContextAccounting>,
    terminal: bool,
}

impl StreamTerminalLedgerGuard {
    fn finish(mut self, status: EventStatus, phase: &'static str, error_code: Option<&str>) {
        self.terminal = true;
        if let Some(accounting) = self.accounting.as_mut() {
            accounting.upstream_state = if status == EventStatus::Ok {
                UpstreamSendState::Completed
            } else {
                UpstreamSendState::FailedAfterCommit
            };
        }
        append_stage_event(
            &self.state,
            self.request_id,
            LayeredStage::ProviderForward,
            status,
            json!({
                "provider_id": self.provider_id,
                "protocol": "anthropic",
                "stream": true,
                "upstream_status": self.upstream_status,
                "optical_phase": phase,
                "terminal": true,
                "kind": self.kind,
                "disposition": self.accounting.as_ref().map(|accounting| accounting.disposition),
                "accounting": self.accounting,
                "automatic_retry": false,
                "raw_available": false
            }),
            error_code,
        );
    }
}

impl Drop for StreamTerminalLedgerGuard {
    fn drop(&mut self) {
        if self.terminal {
            return;
        }
        self.terminal = true;
        if let Some(accounting) = self.accounting.as_mut() {
            accounting.upstream_state = UpstreamSendState::FailedAfterCommit;
        }
        append_stage_event(
            &self.state,
            self.request_id,
            LayeredStage::ProviderForward,
            EventStatus::Failed,
            json!({
                "provider_id": self.provider_id,
                "protocol": "anthropic",
                "stream": true,
                "upstream_status": self.upstream_status,
                "optical_phase": "upstream_failed",
                "terminal": true,
                "kind": self.kind,
                "disposition": self.accounting.as_ref().map(|accounting| accounting.disposition),
                "accounting": self.accounting,
                "automatic_retry": false,
                "raw_available": false,
                "stream_end": "body_dropped"
            }),
            Some("provider_stream_dropped"),
        );
    }
}

struct TerminalEventStream {
    inner: UpstreamByteStream,
    guard: Option<StreamTerminalLedgerGuard>,
}

impl TerminalEventStream {
    fn new(inner: UpstreamByteStream, guard: StreamTerminalLedgerGuard) -> Self {
        Self {
            inner,
            guard: Some(guard),
        }
    }
}

impl Stream for TerminalEventStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.as_mut().poll_next(context) {
            Poll::Ready(None) => {
                if let Some(guard) = self.guard.take() {
                    guard.finish(EventStatus::Ok, "upstream_completed", None);
                }
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                if let Some(guard) = self.guard.take() {
                    guard.finish(
                        EventStatus::Failed,
                        "upstream_failed",
                        Some("provider_stream_error"),
                    );
                }
                Poll::Ready(Some(Err(error)))
            }
            other => other,
        }
    }
}

async fn forward_anthropic_messages_stream(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    body: Value,
    raw_body: Option<&[u8]>,
    inbound_headers: &HeaderMap,
) -> Result<(u16, axum::http::HeaderMap, UpstreamByteStream), UpstreamResult> {
    let credential = resolve_provider_credential(provider).await?;
    if is_anthropic_upstream(&provider.base_url) {
        schedule_claude_usage_refresh(
            client,
            anthropic_oauth_token(credential.as_deref(), inbound_headers),
        );
    }
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
                    quota_metadata: None,
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
            Ok((status_code, headers, Box::pin(stream)))
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
                quota_metadata: None,
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
    let scope = resolve_configured_request_scope(
        &state.config,
        route,
        provider,
        request_id,
        upstream_body.get("model").and_then(Value::as_str),
        Protocol::Anthropic,
        provider.protocol.clone(),
    )
    .map_err(|_| RouteSelectionError::InvalidOptimizationContext)?;
    let compression = if provider.protocol == Protocol::Anthropic {
        // Direct Anthropic routes are planned by HybridContextEngine in the handler.
        // Keeping this preliminary body untouched prevents semantic+optical stacking.
        compress_for_route(None, CompressionPolicy::Off, upstream_body)
    } else {
        let cache_evidence = configured_provider_cache_evidence(state, &scope);
        compress_for_route_with_cache_evidence(
            Some(&optimization_context),
            decision.compression_policy.clone(),
            upstream_body,
            cache_evidence.as_ref(),
        )
    };
    let attribution = request_attribution_from_scope(&scope);
    let request_log = build_request_log(
        request_id,
        &decision,
        body.get("model")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        Protocol::Anthropic,
        &compression,
        &attribution,
    );
    let mut event = build_ledger_event(
        request_id,
        &decision,
        Protocol::Anthropic,
        provider.protocol.clone(),
        &compression,
        &attribution,
    );
    attach_request_scope(&mut event, &scope);
    Ok(RoutedBridgePayload {
        scope,
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
    let cache_evidence = configured_provider_cache_evidence(state, &payload.scope);
    let cancellation = RenderCancellation::default();
    let worker_cancellation = cancellation.clone();
    let mut cancel_guard = RenderCancelGuard(Some(cancellation));
    let result = tokio::task::spawn_blocking(move || {
        engine.prepare_with_cache_evidence(
            &request_id,
            &context,
            compression_policy,
            body,
            Some(&raw_body),
            &global,
            &route,
            &registry,
            &worker_cancellation,
            cache_evidence.as_ref(),
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
    let common = |phase: &str| {
        json!({
            "optical_phase": phase,
            "kind": hybrid_kind_label(report.kind),
            "disposition": prepared.accounting().disposition,
            "accounting": prepared.accounting(),
            "reason_code": report.reason_code,
            "candidate_count": report.candidate_count,
            "safe_text_candidate_count": report.safe_text_candidate_count,
            "optical_candidate_count": report.optical_candidate_count,
            "cache_stable_candidate_count": report.cache_stable_candidate_count,
            "exact_preserved_count": report.exact_preserved_count,
            "decision_preserved_count": report.decision_preserved_count,
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
            "terminal": false,
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
                common("planned"),
                None,
            )?;
            try_append_stage_event(
                state,
                request_id,
                LayeredStage::OpticalRender,
                EventStatus::Ok,
                common("planned"),
                None,
            )?;
            try_append_stage_event(
                state,
                request_id,
                LayeredStage::OpticalPatch,
                EventStatus::Ok,
                common("planned"),
                None,
            )?;
            try_append_stage_event(
                state,
                request_id,
                LayeredStage::OpticalValidate,
                EventStatus::Ok,
                common("artifact_validated"),
                None,
            )?;
        }
        HybridPreparationKind::Shadow => try_append_stage_event(
            state,
            request_id,
            LayeredStage::OpticalPlan,
            EventStatus::Passthrough,
            common("planned"),
            None,
        )?,
        HybridPreparationKind::Semantic => try_append_stage_event(
            state,
            request_id,
            LayeredStage::HeadroomCompress,
            EventStatus::Ok,
            common("artifact_validated"),
            None,
        )?,
        HybridPreparationKind::FallbackPreSend => try_append_stage_event(
            state,
            request_id,
            LayeredStage::OpticalFallback,
            EventStatus::Failed,
            common("fallback"),
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

fn force_payload_ccr_pre_send_baseline(
    payload: &mut RoutedBridgePayload,
    original_body: &Value,
    reason_code: &str,
) {
    let scope = payload.event.metadata.get("request_scope").cloned();
    let attribution = RequestAttribution {
        agent: payload.request_log.agent.clone(),
        instance_id: payload.request_log.instance_id.clone(),
        project_id: payload.event.project_id.clone(),
    };
    let upstream_body = match payload.provider.protocol {
        Protocol::Anthropic => original_body.clone(),
        Protocol::OpenAi => anthropic_to_openai_body(original_body, &payload.provider),
    };
    let compression = compress_for_route(None, CompressionPolicy::Off, upstream_body);
    let request_id = payload.decision.request_id;
    let model = original_body
        .get("model")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let request_log = build_request_log(
        request_id,
        &payload.decision,
        model,
        Protocol::Anthropic,
        &compression,
        &attribution,
    );
    let mut event = build_ledger_event(
        request_id,
        &payload.decision,
        Protocol::Anthropic,
        payload.provider.protocol.clone(),
        &compression,
        &attribution,
    );
    if let Some(metadata) = event.metadata.as_object_mut() {
        metadata.insert("ccr_pre_send".to_string(), json!(reason_code));
        if let Some(scope) = scope {
            metadata.insert("request_scope".to_string(), scope);
        }
    }
    payload.upstream_body = compression.body.clone();
    payload.compression = compression;
    payload.request_log = request_log;
    payload.event = event;
}

fn apply_routed_pipeline_payload(
    payload: &mut RoutedBridgePayload,
    original_anthropic_body: &Value,
    prepared_ccr: &PreparedCcr,
) -> anyhow::Result<dtoken_types::SentPayloadVariant> {
    let protocol_baseline = match payload.provider.protocol {
        Protocol::Anthropic => original_anthropic_body.clone(),
        Protocol::OpenAi => anthropic_to_openai_body(original_anthropic_body, &payload.provider),
    };
    let mut selection = prepare_json_pipeline_payload(
        payload.scope.clone(),
        &protocol_baseline,
        &payload.upstream_body,
        &payload.request_log.transforms_applied,
        prepared_ccr,
    )?;
    if let Some(reason) = selection.fallback_reason {
        force_payload_ccr_pre_send_baseline(payload, original_anthropic_body, reason);
        selection = prepare_json_pipeline_payload(
            payload.scope.clone(),
            &protocol_baseline,
            &payload.upstream_body,
            &[],
            prepared_ccr,
        )?;
        if let Some(metadata) = payload.event.metadata.as_object_mut() {
            metadata.insert("pipeline_fallback_reason".to_string(), json!(reason));
        }
    }
    payload.upstream_body = serde_json::from_slice(selection.ready.body())?;
    let variant = selection.ready.variant();
    if let Some(metadata) = payload.event.metadata.as_object_mut() {
        metadata.insert("pipeline_payload_variant".to_string(), json!(variant));
        metadata.insert(
            "pipeline_reason_code".to_string(),
            json!(selection.ready.reason_code()),
        );
    }
    Ok(variant)
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
    let attribution = RequestAttribution::default();
    let log = build_request_log(
        request_id,
        &decision,
        request.model.clone(),
        Protocol::OpenAi,
        &compression,
        &attribution,
    );
    let event = build_ledger_event(
        request_id,
        &decision,
        Protocol::OpenAi,
        Protocol::OpenAi,
        &compression,
        &attribution,
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
    let latency_samples = logs
        .iter()
        .filter_map(|log| log.latency_ms)
        .collect::<Vec<_>>();
    let avg_latency_ms = (!latency_samples.is_empty()).then(|| {
        latency_samples.iter().copied().sum::<u64>() as f64 / latency_samples.len() as f64
    });
    let latency_breakdown = latency_breakdown_payload(logs);
    let route_overhead = route_overhead_payload(logs);
    let mut account_quotas = BTreeMap::new();
    for log in logs {
        let Some(quota) = log.quota_metadata.as_ref() else {
            continue;
        };
        let Some(provider) = quota.get("provider").and_then(Value::as_str) else {
            continue;
        };
        account_quotas
            .entry(provider.to_string())
            .or_insert_with(|| quota.clone());
    }
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
    let analytics = RuntimeSummaryReadModel::from_records(logs, &[]).stats()["analytics"].clone();

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
        "avg_latency_semantics": "compression_only",
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
        "layered": layered,
        "account_quotas": account_quotas,
        "analytics": analytics
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
    runtime_metrics_text_from_stats(&stats)
}

fn runtime_metrics_text_from_stats(stats: &Value) -> String {
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
    let compression_latency_ms = stats["latency_breakdown"]["compression_ms"].as_f64();

    let mut metrics = format!(
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
dtoken_savings_percent {savings_percent:.2}\n"
    );
    if let Some(compression_latency_ms) = compression_latency_ms {
        metrics.push_str(&format!(
            "dtoken_compression_latency_ms_avg {compression_latency_ms:.2}\n"
        ));
    }
    metrics
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
    let latency_samples = logs
        .iter()
        .filter_map(|log| log.latency_ms)
        .collect::<Vec<_>>();
    let compression_avg = (!latency_samples.is_empty()).then(|| {
        latency_samples.iter().copied().sum::<u64>() as f64 / latency_samples.len() as f64
    });

    json!({
        "source": "request_logs",
        "status": if compression_avg.is_some() { "partial" } else { "unavailable" },
        "model": "compression_only",
        "total_ms": null,
        "compression_ms": compression_avg,
        "route_adapter_ms": null,
        "provider_ms": null,
        "cc_switch_ms": null
    })
}

fn route_overhead_payload(logs: &[RequestLog]) -> Value {
    let runtime_final_tokens: u64 = logs
        .iter()
        .filter_map(|log| log.input_tokens_after)
        .map(usize_to_u64)
        .fold(0_u64, u64::saturating_add);
    json!({
        "source": "unavailable",
        "status": "unavailable",
        "baseline": "provider_input_tokens - runtime_final_tokens",
        "runtime_final_tokens": runtime_final_tokens,
        "provider_input_tokens": null,
        "tokens": null,
        "percent": null,
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
    let private_store_configured = state.ccr_private_store.is_some();
    let private_summary = state
        .ccr_private_store
        .as_ref()
        .and_then(|store| store.summary().ok());
    let private_entries = private_summary
        .as_ref()
        .map(|summary| summary.entries)
        .unwrap_or_default();
    let encrypted_bytes = private_summary
        .as_ref()
        .map(|summary| summary.encrypted_bytes)
        .unwrap_or_default();
    json!({
        "available": !metadata.is_empty() || private_entries > 0,
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "entries": metadata.len(),
        "private_entries": private_entries,
        "bytes": encrypted_bytes,
        "retrievals": 0,
        "tokens_saved": tokens_saved,
        "raw_available": private_entries > 0,
        "private_store_configured": private_store_configured,
        "raw_access": if private_store_configured { "in-process-authorized-only" } else { "disabled" },
        "recent_refs": metadata.into_iter().take(10).collect::<Vec<_>>(),
        "note": if private_store_configured {
            "CCR originals are encrypted with an OS-credential-store-backed key, expire automatically, and are excluded from ordinary HTTP responses, logs, Ledger, and exports."
        } else {
            "CCR private storage is disabled by default; metadata never contains raw prompt or response bodies."
        }
    })
}

fn runtime_cache_summary_from_read_model(
    state: &AppState,
    model: &RuntimeSummaryReadModel,
) -> Value {
    let private_store_configured = state.ccr_private_store.is_some();
    let private_summary = state
        .ccr_private_store
        .as_ref()
        .and_then(|store| store.summary().ok());
    let private_entries = private_summary
        .as_ref()
        .map(|summary| summary.entries)
        .unwrap_or_default();
    let encrypted_bytes = private_summary
        .as_ref()
        .map(|summary| summary.encrypted_bytes)
        .unwrap_or_default();
    json!({
        "available": model.ccr_entries > 0 || private_entries > 0,
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "entries": model.ccr_entries,
        "private_entries": private_entries,
        "bytes": encrypted_bytes,
        "retrievals": 0,
        "tokens_saved": model.ccr_tokens_saved,
        "raw_available": private_entries > 0,
        "private_store_configured": private_store_configured,
        "raw_access": if private_store_configured { "in-process-authorized-only" } else { "disabled" },
        "recent_refs": model.recent_ccr.iter().cloned().collect::<Vec<_>>(),
        "note": if private_store_configured {
            "CCR originals are encrypted with an OS-credential-store-backed key, expire automatically, and are excluded from ordinary HTTP responses, logs, Ledger, and exports."
        } else {
            "CCR private storage is disabled by default; metadata never contains raw prompt or response bodies."
        }
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
        "state": "metadata-only",
        "active": false,
        "project_id": project_id,
        "agent": agent,
        "output_verbosity": output_verbosity,
        "learned_rules": learned_rules,
        "applied_requests": 0,
        "accounting_evidence": "none",
        "raw_available": false,
        "note": "Output shaping is not yet applied to outbound requests; this endpoint only reports explicit memory/learn metadata."
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
    with_cached_account_quota(with_active_config(
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
    ))
}

pub fn dashboard_summary_for_state(state: &AppState) -> Value {
    let summary_started_at = Instant::now();
    let model = state
        .runtime_summary
        .lock()
        .map(|model| model.clone())
        .unwrap_or_default();
    let stats = model.stats();
    let mut payload = with_cached_account_quota(with_active_config(
        build_dashboard_payload(DashboardPayloadInput {
            history: model.history(),
            metrics: model.metrics(&stats),
            stats,
            cache_summary: runtime_cache_summary_from_read_model(state, &model),
            providers: provider_metadata_payload(&state.config.providers),
            routes: json!(state.config.routes),
            instances: json!(state.config.instances),
            events: json!([]),
            route_decisions: json!(state.recent_route_decisions(20)),
            request_logs: json!(state.recent_request_logs(20)),
            memory: Value::Null,
        }),
        active_config_payload(state),
    ));
    let Some(object) = payload.as_object_mut() else {
        state.record_runtime_performance("dashboard_summary_build", summary_started_at, false, 0);
        return payload;
    };
    object.insert("payload_kind".to_string(), json!("summary"));
    object.insert(
        "slice_revisions".to_string(),
        json!({
            "runtime_summary": model.revision,
            "request_logs": model.total_requests,
            "history": model.revision,
            "cache_summary": model.ccr_entries
        }),
    );
    object.remove("events");
    object.remove("memory");
    if let Some(logs) = object.get_mut("request_logs").and_then(Value::as_array_mut) {
        for log in logs {
            let source = log.as_object().cloned().unwrap_or_default();
            let mut compact = serde_json::Map::new();
            for key in [
                "timestamp_ms",
                "agent",
                "input_tokens_before",
                "input_tokens_after",
                "output_tokens",
                "tokens_saved",
                "transforms_applied",
                "latency_ms",
                "route_id",
                "provider_id",
                "model",
                "input_protocol",
                "chain_mode",
                "config_source",
            ] {
                if let Some(value) = source.get(key) {
                    compact.insert(key.to_string(), value.clone());
                }
            }
            if let Some(error_type) =
                analytics_error_type(source.get("error").and_then(Value::as_str))
            {
                compact.insert("error_type".to_string(), json!(error_type));
            }
            *log = Value::Object(compact);
        }
    }
    if let Some(decisions) = object
        .get_mut("route_decisions")
        .and_then(Value::as_array_mut)
    {
        for decision in decisions {
            let source = decision.as_object().cloned().unwrap_or_default();
            let mut compact = serde_json::Map::new();
            for key in [
                "timestamp_ms",
                "agent",
                "route_id",
                "provider_id",
                "chain_mode",
                "fallback",
            ] {
                if let Some(value) = source.get(key) {
                    compact.insert(key.to_string(), value.clone());
                }
            }
            *decision = Value::Object(compact);
        }
    }
    state.record_runtime_performance("dashboard_summary_build", summary_started_at, true, 0);
    object.insert(
        "performance".to_string(),
        json!({
            "schema_version": 1,
            "source": "d-token-runtime-performance",
            "stages": state.runtime_performance_payload(),
            "privacy": "Metadata-only allowlisted duration and count histograms."
        }),
    );
    payload
}

pub fn dashboard_payload_default() -> Value {
    let config = DtokenConfig::empty();
    dashboard_payload_for_config(&config)
}

async fn dashboard_payload(State(state): State<AppState>) -> Json<Value> {
    Json(dashboard_payload_for_state(&state))
}

async fn dashboard_summary(State(state): State<AppState>) -> Json<Value> {
    Json(dashboard_summary_for_state(&state))
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
    body_bytes: Bytes,
) -> impl IntoResponse {
    let original_body: Value = match serde_json::from_slice(&body_bytes) {
        Ok(body) => body,
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
    let ccr_original_body = body_bytes.to_vec();
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
        let scope = match resolve_configured_request_scope(
            &state.config,
            route,
            provider,
            request_id,
            request.model.as_deref(),
            Protocol::OpenAi,
            Protocol::OpenAi,
        ) {
            Ok(scope) => scope,
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
        let baseline_body = original_body.clone();
        let cache_evidence = configured_provider_cache_evidence(&state, &scope);
        let mut compression = compress_for_route_with_cache_evidence(
            Some(&optimization_context),
            decision.compression_policy.clone(),
            original_body,
            cache_evidence.as_ref(),
        );
        let attribution = request_attribution_from_scope(&scope);
        let mut request_log = build_request_log(
            request_id,
            &decision,
            request.model.clone(),
            Protocol::OpenAi,
            &compression,
            &attribution,
        );
        let mut event = build_ledger_event(
            request_id,
            &decision,
            Protocol::OpenAi,
            Protocol::OpenAi,
            &compression,
            &attribution,
        );
        attach_request_scope(&mut event, &scope);
        if let Some(metadata) = event.metadata.as_object_mut() {
            metadata.insert(
                "compression_inventory".to_string(),
                inventory_diagnostics.clone(),
            );
        }
        let prepared_ccr =
            match state.prepare_ccr_for_request(&decision, &request_log, &ccr_original_body) {
                Ok(prepared) => prepared,
                Err(_) => {
                    compression =
                        compress_for_route(None, CompressionPolicy::Off, baseline_body.clone());
                    request_log = build_request_log(
                        request_id,
                        &decision,
                        request.model.clone(),
                        Protocol::OpenAi,
                        &compression,
                        &attribution,
                    );
                    event = build_ledger_event(
                        request_id,
                        &decision,
                        Protocol::OpenAi,
                        Protocol::OpenAi,
                        &compression,
                        &attribution,
                    );
                    attach_request_scope(&mut event, &scope);
                    if let Some(metadata) = event.metadata.as_object_mut() {
                        metadata.insert(
                            "ccr_pre_send".to_string(),
                            json!("baseline_fallback_store_unavailable"),
                        );
                    }
                    state
                        .prepare_ccr_for_request(&decision, &request_log, &ccr_original_body)
                        .expect("baseline request does not require CCR storage")
                }
            };
        let pipeline_payload = match prepare_json_pipeline_payload(
            scope.clone(),
            &baseline_body,
            &compression.body,
            &request_log.transforms_applied,
            &prepared_ccr,
        ) {
            Ok(payload) => payload,
            Err(_) => PreparedPipelinePayload {
                ready: PipelineInput::new(
                    scope.clone(),
                    serde_json::to_vec(&baseline_body).unwrap_or_default(),
                    MAX_REQUEST_BODY_BYTES,
                )
                .map(|input| {
                    input
                        .prepare()
                        .into_baseline_ready("pipeline_input_invalid")
                })
                .unwrap_or_else(|_| {
                    PipelineInput::new(scope.clone(), b"{}".to_vec(), MAX_REQUEST_BODY_BYTES)
                        .expect("static JSON baseline is valid")
                        .prepare()
                        .into_baseline_ready("pipeline_input_invalid")
                }),
                fallback_reason: Some("pipeline_input_invalid"),
            },
        };
        if let Some(reason) = pipeline_payload.fallback_reason {
            compression = compress_for_route(None, CompressionPolicy::Off, baseline_body.clone());
            request_log = build_request_log(
                request_id,
                &decision,
                request.model.clone(),
                Protocol::OpenAi,
                &compression,
                &attribution,
            );
            event = build_ledger_event(
                request_id,
                &decision,
                Protocol::OpenAi,
                Protocol::OpenAi,
                &compression,
                &attribution,
            );
            attach_request_scope(&mut event, &scope);
            if let Some(metadata) = event.metadata.as_object_mut() {
                metadata.insert("compression_inventory".to_string(), inventory_diagnostics);
                metadata.insert("pipeline_fallback_reason".to_string(), json!(reason));
            }
        }
        let upstream_body = serde_json::from_slice::<Value>(pipeline_payload.ready.body())
            .unwrap_or_else(|_| baseline_body.clone());
        if let Some(metadata) = event.metadata.as_object_mut() {
            metadata.insert(
                "pipeline_payload_variant".to_string(),
                json!(pipeline_payload.ready.variant()),
            );
            metadata.insert(
                "pipeline_reason_code".to_string(),
                json!(pipeline_payload.ready.reason_code()),
            );
        }
        drop(ccr_original_body);
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
            let quota_account_ref = opaque_codex_account_ref(&headers);
            let upstream = forward_openai_chat_stream(
                &state.http_client,
                provider,
                upstream_body.clone(),
                Some(&headers),
            )
            .await;
            match upstream {
                Ok((status_code, response_headers, body, mut quota_metadata)) => {
                    attach_quota_account_ref(&mut quota_metadata, quota_account_ref.as_deref());
                    request_log.quota_metadata = quota_metadata;
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
                    state.record_routed_request_with_ccr_receipt(
                        &decision,
                        request_log,
                        event,
                        &prepared_ccr,
                    );
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
                        response_headers,
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
                    state.record_routed_request_with_ccr_receipt(
                        &decision,
                        request_log,
                        event,
                        &prepared_ccr,
                    );
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
        let mut upstream =
            forward_openai_chat(&state.http_client, provider, upstream_body, Some(&headers)).await;
        attach_quota_account_ref(
            &mut upstream.quota_metadata,
            opaque_codex_account_ref(&headers).as_deref(),
        );
        mark_upstream_outcome(&upstream, &mut request_log, &mut event);
        persist_provider_cache_observation(
            &state,
            &scope,
            &upstream,
            dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
        );
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
        state.record_routed_request_with_ccr_receipt(&decision, request_log, event, &prepared_ccr);
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
    body_bytes: Bytes,
) -> impl IntoResponse {
    let body: Value = match serde_json::from_slice(&body_bytes) {
        Ok(body) => body,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "type": "invalid_request",
                        "message": format!("invalid OpenAI Responses request: {error}")
                    }
                })),
            )
                .into_response();
        }
    };
    let ccr_original_body = body_bytes.to_vec();
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
        let scope = match resolve_configured_request_scope(
            &state.config,
            route,
            provider,
            request_id,
            request.model.as_deref(),
            Protocol::OpenAi,
            Protocol::OpenAi,
        ) {
            Ok(scope) => scope,
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
        let responses_shadow_context = ResolvedOptimizationContext {
            route_id: decision.route_id.clone(),
            provider_id: decision.provider_id.clone(),
            model: optimization_context.effective_model.clone(),
            input_protocol: Protocol::OpenAi,
            output_protocol: Protocol::OpenAi,
            endpoint: OpticalEndpoint::OpenAiResponses,
        };
        let inventory_diagnostics =
            compression_inventory_diagnostics(&responses_shadow_context, &body);
        let baseline_body = original_body.clone();
        let cache_evidence = configured_provider_cache_evidence(&state, &scope);
        let mut compression = compress_for_route_with_cache_evidence(
            Some(&optimization_context),
            decision.compression_policy.clone(),
            original_body,
            cache_evidence.as_ref(),
        );
        let attribution = request_attribution_from_scope(&scope);
        let mut request_log = build_request_log(
            request_id,
            &decision,
            request.model.clone(),
            Protocol::OpenAi,
            &compression,
            &attribution,
        );
        let mut event = build_ledger_event(
            request_id,
            &decision,
            Protocol::OpenAi,
            Protocol::OpenAi,
            &compression,
            &attribution,
        );
        attach_request_scope(&mut event, &scope);
        if let Some(metadata) = event.metadata.as_object_mut() {
            metadata.insert(
                "responses_shadow_inventory".to_string(),
                inventory_diagnostics.clone(),
            );
        }
        let prepared_ccr =
            match state.prepare_ccr_for_request(&decision, &request_log, &ccr_original_body) {
                Ok(prepared) => prepared,
                Err(_) => {
                    compression =
                        compress_for_route(None, CompressionPolicy::Off, baseline_body.clone());
                    request_log = build_request_log(
                        request_id,
                        &decision,
                        request.model.clone(),
                        Protocol::OpenAi,
                        &compression,
                        &attribution,
                    );
                    event = build_ledger_event(
                        request_id,
                        &decision,
                        Protocol::OpenAi,
                        Protocol::OpenAi,
                        &compression,
                        &attribution,
                    );
                    attach_request_scope(&mut event, &scope);
                    if let Some(metadata) = event.metadata.as_object_mut() {
                        metadata.insert(
                            "responses_shadow_inventory".to_string(),
                            inventory_diagnostics.clone(),
                        );
                        metadata.insert(
                            "ccr_pre_send".to_string(),
                            json!("baseline_fallback_store_unavailable"),
                        );
                    }
                    state
                        .prepare_ccr_for_request(&decision, &request_log, &ccr_original_body)
                        .expect("baseline request does not require CCR storage")
                }
            };
        let pipeline_payload = match prepare_json_pipeline_payload(
            scope.clone(),
            &baseline_body,
            &compression.body,
            &request_log.transforms_applied,
            &prepared_ccr,
        ) {
            Ok(payload) => payload,
            Err(_) => PreparedPipelinePayload {
                ready: PipelineInput::new(
                    scope.clone(),
                    serde_json::to_vec(&baseline_body).unwrap_or_default(),
                    MAX_REQUEST_BODY_BYTES,
                )
                .map(|input| {
                    input
                        .prepare()
                        .into_baseline_ready("pipeline_input_invalid")
                })
                .unwrap_or_else(|_| {
                    PipelineInput::new(scope.clone(), b"{}".to_vec(), MAX_REQUEST_BODY_BYTES)
                        .expect("static JSON baseline is valid")
                        .prepare()
                        .into_baseline_ready("pipeline_input_invalid")
                }),
                fallback_reason: Some("pipeline_input_invalid"),
            },
        };
        if let Some(reason) = pipeline_payload.fallback_reason {
            compression = compress_for_route(None, CompressionPolicy::Off, baseline_body.clone());
            request_log = build_request_log(
                request_id,
                &decision,
                request.model.clone(),
                Protocol::OpenAi,
                &compression,
                &attribution,
            );
            event = build_ledger_event(
                request_id,
                &decision,
                Protocol::OpenAi,
                Protocol::OpenAi,
                &compression,
                &attribution,
            );
            attach_request_scope(&mut event, &scope);
            if let Some(metadata) = event.metadata.as_object_mut() {
                metadata.insert(
                    "responses_shadow_inventory".to_string(),
                    inventory_diagnostics,
                );
                metadata.insert("pipeline_fallback_reason".to_string(), json!(reason));
            }
        }
        let upstream_body = serde_json::from_slice::<Value>(pipeline_payload.ready.body())
            .unwrap_or_else(|_| baseline_body.clone());
        if let Some(metadata) = event.metadata.as_object_mut() {
            metadata.insert(
                "pipeline_payload_variant".to_string(),
                json!(pipeline_payload.ready.variant()),
            );
            metadata.insert(
                "pipeline_reason_code".to_string(),
                json!(pipeline_payload.ready.reason_code()),
            );
        }
        drop(ccr_original_body);
        let mut upstream = require_openai_message(
            forward_openai_chat(&state.http_client, provider, upstream_body, Some(&headers)).await,
            "OpenAI Responses",
        );
        attach_quota_account_ref(
            &mut upstream.quota_metadata,
            opaque_codex_account_ref(&headers).as_deref(),
        );
        mark_upstream_outcome(&upstream, &mut request_log, &mut event);
        persist_provider_cache_observation(
            &state,
            &scope,
            &upstream,
            dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
        );
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
        state.record_routed_request_with_ccr_receipt(&decision, request_log, event, &prepared_ccr);
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
    let ccr_original_body = raw_body.clone();
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
        // Active preparation owns the request-wide byte lease. Keep it alive
        // until the upstream operation reaches a terminal return.
        let mut _retained_hybrid_request = None;
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
            apply_hybrid_preparation(&mut payload, &prepared);
            _retained_hybrid_request = Some(prepared);
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

        let prepared_ccr = match state.prepare_ccr_for_request(
            &payload.decision,
            &payload.request_log,
            &ccr_original_body,
        ) {
            Ok(prepared) => prepared,
            Err(_) => {
                force_payload_ccr_pre_send_baseline(
                    &mut payload,
                    &body,
                    "baseline_fallback_store_unavailable",
                );
                state
                    .prepare_ccr_for_request(
                        &payload.decision,
                        &payload.request_log,
                        &ccr_original_body,
                    )
                    .expect("baseline request does not require CCR storage")
            }
        };
        let pipeline_variant =
            match apply_routed_pipeline_payload(&mut payload, &body, &prepared_ccr) {
                Ok(variant) => variant,
                Err(_) => {
                    force_payload_ccr_pre_send_baseline(
                        &mut payload,
                        &body,
                        "pipeline_input_invalid",
                    );
                    if let Some(metadata) = payload.event.metadata.as_object_mut() {
                        metadata.insert(
                            "pipeline_fallback_reason".to_string(),
                            json!("pipeline_input_invalid"),
                        );
                        metadata.insert(
                            "pipeline_payload_variant".to_string(),
                            json!(dtoken_types::SentPayloadVariant::Original),
                        );
                    }
                    dtoken_types::SentPayloadVariant::Original
                }
            };
        let send_raw_body = pipeline_variant == dtoken_types::SentPayloadVariant::Original;
        drop(ccr_original_body);

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
                Ok((status_code, response_headers, response_stream)) => {
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
                    let stream_accounting = payload.request_log.context_accounting.clone();
                    let stream_kind = match stream_accounting
                        .as_ref()
                        .map(|accounting| accounting.disposition)
                    {
                        Some(dtoken_types::OptimizationDisposition::Active) => "active",
                        Some(dtoken_types::OptimizationDisposition::SemanticActive) => "semantic",
                        Some(dtoken_types::OptimizationDisposition::Shadow) => "shadow",
                        Some(dtoken_types::OptimizationDisposition::FallbackPreSend)
                        | Some(dtoken_types::OptimizationDisposition::FallbackOriginalSent) => {
                            "fallback"
                        }
                        _ => "passthrough",
                    };
                    state.record_routed_request_with_ccr_receipt(
                        &payload.decision,
                        payload.request_log,
                        payload.event,
                        &prepared_ccr,
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
                            "optical_phase": "send_committed",
                            "terminal": false,
                            "kind": stream_kind,
                            "disposition": stream_accounting.as_ref().map(|accounting| accounting.disposition),
                            "accounting": stream_accounting,
                            "automatic_retry": false,
                            "raw_available": false
                        }),
                        None,
                    );
                    let response_body = Body::from_stream(TerminalEventStream::new(
                        response_stream,
                        StreamTerminalLedgerGuard {
                            state: state.clone(),
                            request_id: payload.decision.request_id,
                            provider_id: payload.decision.provider_id.clone(),
                            upstream_status: status_code,
                            kind: stream_kind,
                            accounting: stream_accounting,
                            terminal: false,
                        },
                    ));
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
                    state.record_routed_request_with_ccr_receipt(
                        &payload.decision,
                        payload.request_log,
                        payload.event,
                        &prepared_ccr,
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
        persist_provider_cache_observation(
            &state,
            &payload.scope,
            &upstream,
            dtoken_types::ProviderCacheObservationKind::AnthropicUsage,
        );
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
        let terminal_accounting = payload.request_log.context_accounting.clone();
        let terminal_kind = payload
            .event
            .metadata
            .get("hybrid_kind")
            .cloned()
            .unwrap_or_else(|| json!("passthrough"));
        state.record_routed_request_with_ccr_receipt(
            &payload.decision,
            payload.request_log,
            payload.event,
            &prepared_ccr,
        );
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
                "optical_phase": if upstream.send_state == UpstreamSendState::Completed { "upstream_completed" } else { "upstream_failed" },
                "terminal": true,
                "kind": terminal_kind,
                "disposition": terminal_accounting.as_ref().map(|accounting| accounting.disposition),
                "accounting": terminal_accounting,
                "automatic_retry": false,
                "raw_available": false,
                "privacy": "request and response bodies are never persisted"
            }),
            upstream.error_category.as_deref(),
        );
        return (response_status, Json(response)).into_response();
    }

    let prepared_ccr = match state.prepare_ccr_for_request(
        &payload.decision,
        &payload.request_log,
        &ccr_original_body,
    ) {
        Ok(prepared) => prepared,
        Err(_) => {
            force_payload_ccr_pre_send_baseline(
                &mut payload,
                &body,
                "baseline_fallback_store_unavailable",
            );
            state
                .prepare_ccr_for_request(
                    &payload.decision,
                    &payload.request_log,
                    &ccr_original_body,
                )
                .expect("baseline request does not require CCR storage")
        }
    };
    if apply_routed_pipeline_payload(&mut payload, &body, &prepared_ccr).is_err() {
        force_payload_ccr_pre_send_baseline(&mut payload, &body, "pipeline_input_invalid");
        if let Some(metadata) = payload.event.metadata.as_object_mut() {
            metadata.insert(
                "pipeline_fallback_reason".to_string(),
                json!("pipeline_input_invalid"),
            );
            metadata.insert(
                "pipeline_payload_variant".to_string(),
                json!(dtoken_types::SentPayloadVariant::Original),
            );
        }
    }
    drop(ccr_original_body);
    let mut upstream = require_openai_message(
        forward_openai_chat(
            &state.http_client,
            &payload.provider,
            payload.upstream_body.clone(),
            Some(&headers),
        )
        .await,
        "Anthropic bridge",
    );
    attach_quota_account_ref(
        &mut upstream.quota_metadata,
        opaque_codex_account_ref(&headers).as_deref(),
    );
    mark_upstream_outcome(&upstream, &mut payload.request_log, &mut payload.event);
    persist_provider_cache_observation(
        &state,
        &payload.scope,
        &upstream,
        dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
    );
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
    state.record_routed_request_with_ccr_receipt(
        &payload.decision,
        payload.request_log,
        payload.event,
        &prepared_ccr,
    );
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

#[derive(Debug, Default, Deserialize)]
struct AnalyticsRequestsQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

fn analytics_cursor(log: &RequestLog) -> String {
    format!("{}:{}", log.timestamp_ms, log.request_id)
}

fn analytics_error_type(error: Option<&str>) -> Option<String> {
    error.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()
            && value.len() <= 64
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            }))
        .then(|| value.to_string())
    })
}

fn analytics_request_row(log: &RequestLog) -> Value {
    json!({
        "request_id": log.request_id,
        "timestamp_ms": log.timestamp_ms,
        "agent": log.agent,
        "instance_id": log.instance_id,
        "input_protocol": log.input_protocol,
        "provider_id": log.provider_id,
        "route_id": log.route_id,
        "chain_mode": log.chain_mode,
        "model": log.model,
        "input_tokens_before": log.input_tokens_before,
        "input_tokens_after": log.input_tokens_after,
        "output_tokens": log.output_tokens,
        "tokens_saved": log.tokens_saved,
        "transforms_applied": log.transforms_applied,
        "compression_ms": log.latency_ms,
        "error_type": analytics_error_type(log.error.as_deref()),
        "config_source": log.config_source,
    })
}

async fn analytics_requests(
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRequestsQuery>,
) -> Json<Value> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let logs = state.recent_request_logs(10_000);
    let total_count = logs.len();
    let start = match query.cursor.as_deref() {
        None => Some(0),
        Some(cursor) => logs
            .iter()
            .position(|log| analytics_cursor(log) == cursor)
            .map(|index| index + 1),
    };
    let Some(start) = start else {
        return Json(json!({
            "schema_version": 1,
            "source": "dtoken-owned-runtime",
            "freshness": "fresh",
            "revision": state.runtime_summary.lock().map(|model| model.revision).unwrap_or_default(),
            "items": [],
            "returned_count": 0,
            "total_count": total_count,
            "has_more": false,
            "next_cursor": null,
            "warning": "invalid_cursor"
        }));
    };
    let page = logs.iter().skip(start).take(limit).collect::<Vec<_>>();
    let has_more = start.saturating_add(page.len()) < total_count;
    let returned_count = page.len();
    let next_cursor = has_more
        .then(|| page.last().map(|log| analytics_cursor(log)))
        .flatten();
    Json(json!({
        "schema_version": 1,
        "source": "dtoken-owned-runtime",
        "freshness": "fresh",
        "revision": state.runtime_summary.lock().map(|model| model.revision).unwrap_or_default(),
        "items": page.into_iter().map(analytics_request_row).collect::<Vec<_>>(),
        "returned_count": returned_count,
        "total_count": total_count,
        "has_more": has_more,
        "next_cursor": next_cursor,
        "coverage": { "loaded_limit": 10_000, "complete": total_count < 10_000 }
    }))
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
        "integrated_rtk": false,
        "integrated_rtk_state": "partial-external-path-adapter",
        "reference_headroom": reference,
        "note": "External Headroom detection is reference-only; d-token runtime is the product data source. Integrated RTK binary management and invocation evidence are not complete."
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
    let capability = state
        .capabilities
        .find("mcp")
        .expect("backend capability registry must contain MCP");
    Json(json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "state": capability.state,
        "configured": capability.is_available(),
        "capability_revision": state.capabilities.revision,
        "capability_collected_at_ms": state.capabilities.collected_at_ms,
        "implementation_revision": capability.implementation_revision,
        "evidence_revision": capability.evidence_revision,
        "freshness": capability.freshness,
        "reason_codes": capability.reason_codes,
        "transport": null,
        "tools": [],
        "planned_tools": ["headroom_compress", "headroom_retrieve", "headroom_stats"],
        "implemented_as": [],
        "ccr_entries": ccr["entries"],
        "memory_entries": memory["entries"],
        "raw_available": false,
        "reason": "MCP transport and tool handlers have not been implemented; HTTP metadata endpoints are not MCP tools."
    }))
}

async fn backend_capabilities(State(state): State<AppState>) -> Json<CapabilityRegistrySnapshot> {
    Json((*state.capabilities).clone())
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
    Json(json!({
        "source": "dtoken-owned-runtime",
        "state": "unavailable",
        "projects": [],
        "freshness": null,
        "reason": "The core runtime does not own a verified project registry yet; desktop Smart Learning discovery is reported through its native project-discovery command."
    }))
}

#[derive(Debug, Deserialize)]
struct ProjectPolicyQuery {
    project_id: Option<String>,
}

async fn project_default_policy(Query(query): Query<ProjectPolicyQuery>) -> impl IntoResponse {
    let Some(project_id) = validated_project_id(query.project_id.as_deref()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "source": "dtoken-owned-runtime",
                "state": "blocked",
                "error": {
                    "type": "project_id_required",
                    "message": "A non-empty project_id query parameter is required."
                }
            })),
        )
            .into_response();
    };
    Json(json!({
        "source": "dtoken-owned-runtime",
        "state": "template",
        "policy": default_policy(project_id),
        "note": "This fail-closed template is not an applied project policy."
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct ProjectRecommendationRequest {
    project_id: String,
    #[serde(default)]
    language_stack: Vec<String>,
    #[serde(default)]
    avg_request_tokens_7d: u64,
    #[serde(default)]
    avg_cli_noise_ratio_7d: f32,
    #[serde(default)]
    retrieve_rate_7d: f32,
    #[serde(default)]
    failure_rate_7d: f32,
    #[serde(default)]
    high_risk_task: bool,
    #[serde(default)]
    manual_override: bool,
}

async fn project_recommend_compression(
    Json(request): Json<ProjectRecommendationRequest>,
) -> impl IntoResponse {
    let Some(project_id) = validated_project_id(Some(&request.project_id)) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "source": "dtoken-owned-runtime",
                "state": "blocked",
                "error": {
                    "type": "project_id_required",
                    "message": "A non-empty project_id is required."
                }
            })),
        )
            .into_response();
    };
    let engine = RuleBasedRecommendationEngine;
    let input = RecommendationInput {
        project_id: project_id.to_string(),
        language_stack: request.language_stack,
        avg_request_tokens_7d: request.avg_request_tokens_7d,
        avg_cli_noise_ratio_7d: request.avg_cli_noise_ratio_7d.clamp(0.0, 1.0),
        retrieve_rate_7d: request.retrieve_rate_7d.clamp(0.0, 1.0),
        failure_rate_7d: request.failure_rate_7d.clamp(0.0, 1.0),
        high_risk_task: request.high_risk_task,
        manual_override: request.manual_override,
    };
    Json(json!({
        "source": "dtoken-owned-runtime",
        "state": "recommendation",
        "evidence": "caller-supplied-aggregate-metadata",
        "recommendation": engine.recommend(input)
    }))
    .into_response()
}

fn validated_project_id(value: Option<&str>) -> Option<&str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
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
        Some(metadata) => {
            let raw_available = metadata["raw_available"].as_bool().unwrap_or(false);
            Json(json!({
                "source": "dtoken-owned-runtime",
                "recovery_ref": recovery_ref,
                "raw_available": raw_available,
                "raw_returned": false,
                "retrieval_boundary": if raw_available { "in-process-authorized-only" } else { "disabled" },
                "metadata": metadata,
                "note": "This unauthenticated loopback metadata endpoint never returns CCR originals."
            }))
        }
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

    fn optical_test_event(
        stage: LayeredStage,
        status: EventStatus,
        timestamp_ms: u64,
        metadata: Value,
        context_accounting: Option<dtoken_types::HybridContextAccounting>,
    ) -> DtokenEvent {
        DtokenEvent {
            event_id: Uuid::new_v4(),
            timestamp_ms,
            project_id: None,
            agent_id: None,
            instance_id: None,
            command_id: None,
            request_id: Some(Uuid::new_v4()),
            stage,
            source: EventSource::Dtoken,
            status,
            input_tokens: None,
            output_tokens: None,
            saved_tokens: None,
            baseline_kind: Some(BaselineKind::HeadroomInput),
            error_code: None,
            metadata,
            context_accounting,
        }
    }

    fn completed_active_accounting() -> dtoken_types::HybridContextAccounting {
        dtoken_types::HybridContextAccounting {
            schema_version: 1,
            disposition: dtoken_types::OptimizationDisposition::Active,
            sent_payload: dtoken_types::SentPayloadVariant::Optimized,
            upstream_state: UpstreamSendState::Completed,
            estimated: None,
            provider_counterfactual_baseline: None,
            provider_actual_sent: Default::default(),
            provider_reconciled_input_savings: None,
            paired_actual: None,
            attribution_basis: dtoken_types::SavingsAttribution::None,
            fallback_reason_code: None,
        }
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        serde_json::from_slice(&bytes).expect("response body should be valid JSON")
    }

    #[test]
    fn provider_terminal_event_survives_missing_optical_complete_append() {
        let event = optical_test_event(
            LayeredStage::ProviderForward,
            EventStatus::Ok,
            100,
            json!({
                "kind": "active",
                "optical_phase": "upstream_completed",
                "terminal": true,
                "raw_available": false
            }),
            Some(completed_active_accounting()),
        );

        let summary = optical_event_public_summary(event);
        assert_eq!(
            summary.metadata.final_status,
            OpticalPlanStatus::ValidatedActivePlan
        );
        assert_eq!(summary.metadata.mode, OpticalMode::ToolResultsOnly);
    }

    #[test]
    fn terminal_event_outranks_newer_pre_send_validation() {
        let terminal = optical_test_event(
            LayeredStage::ProviderForward,
            EventStatus::Ok,
            100,
            json!({"kind":"active","optical_phase":"upstream_completed","terminal":true}),
            Some(completed_active_accounting()),
        );
        let validation = optical_test_event(
            LayeredStage::OpticalValidate,
            EventStatus::Ok,
            200,
            json!({"kind":"active","optical_phase":"artifact_validated","terminal":false}),
            None,
        );

        assert!(optical_event_priority(&terminal) > optical_event_priority(&validation));
    }

    #[test]
    fn pre_send_validation_cannot_claim_active_completion() {
        let event = optical_test_event(
            LayeredStage::OpticalValidate,
            EventStatus::Ok,
            100,
            json!({
                "kind": "active",
                "final_status": "validated_active_plan",
                "optical_phase": "artifact_validated",
                "terminal": false
            }),
            Some(completed_active_accounting()),
        );

        let summary = optical_event_public_summary(event);
        assert_eq!(summary.metadata.final_status, OpticalPlanStatus::Disabled);
        assert_eq!(summary.metadata.mode, OpticalMode::ShadowPreview);
    }

    #[test]
    fn terminal_fallback_outranks_send_committed() {
        let fallback = optical_test_event(
            LayeredStage::OpticalFallback,
            EventStatus::Failed,
            100,
            json!({
                "kind": "fallback",
                "optical_phase": "fallback",
                "terminal": true
            }),
            None,
        );
        let committed = optical_test_event(
            LayeredStage::UpstreamSendCommitted,
            EventStatus::Ok,
            200,
            json!({
                "kind": "active",
                "optical_phase": "send_committed",
                "terminal": false
            }),
            None,
        );

        assert!(optical_event_priority(&fallback) > optical_event_priority(&committed));
        assert_eq!(
            optical_event_public_summary(fallback).metadata.final_status,
            OpticalPlanStatus::FailedClosed
        );
    }

    #[test]
    fn stream_send_committed_is_not_reported_as_active_completion() {
        let mut accounting = completed_active_accounting();
        accounting.upstream_state = UpstreamSendState::StreamStarted;
        let event = optical_test_event(
            LayeredStage::UpstreamSendCommitted,
            EventStatus::Ok,
            100,
            json!({
                "kind": "active",
                "optical_phase": "send_committed",
                "terminal": false,
                "stream": true
            }),
            Some(accounting),
        );

        let summary = optical_event_public_summary(event);
        assert_eq!(summary.metadata.final_status, OpticalPlanStatus::Disabled);
        assert_ne!(summary.metadata.mode, OpticalMode::ToolResultsOnly);
    }

    fn request_bytes(value: Value) -> Bytes {
        Bytes::from(serde_json::to_vec(&value).expect("request fixture should serialize"))
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
            provider_cache_policies: vec![],
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
            provider_cache_policies: vec![],
            optical: Default::default(),
        }
    }

    fn cache_test_scope(protocol: Protocol) -> RequestScope {
        RequestScope::new(
            Uuid::new_v4(),
            EvidenceValue::unknown("agent-unavailable"),
            EvidenceValue::unknown("project-unavailable"),
            EvidenceValue::unknown("session-unavailable"),
            EvidenceValue::known("instance-a".to_string(), EvidenceSource::ConfiguredRoute),
            "route-a",
            "provider-a",
            "model-a",
            protocol.clone(),
            protocol,
            "config-a",
        )
        .unwrap()
    }

    #[test]
    fn provider_cache_observation_uses_only_protocol_specific_usage_fields() {
        let anthropic = provider_cache_observation_from_response(
            &cache_test_scope(Protocol::Anthropic),
            &json!({
                "usage": {
                    "input_tokens": 100,
                    "cache_read_input_tokens": 21,
                    "cache_creation_input_tokens": 8,
                    "cache_creation": {
                        "ephemeral_5m_input_tokens": 5,
                        "ephemeral_1h_input_tokens": 3
                    },
                    "ttl_seconds": 999999
                },
                "raw_prompt": "must never be persisted"
            }),
            dtoken_types::ProviderCacheObservationKind::AnthropicUsage,
            42,
        )
        .expect("Anthropic observation");
        assert_eq!(anthropic.read_units, Some(21));
        assert_eq!(anthropic.creation_units, Some(8));
        assert_eq!(anthropic.creation_5m_units, Some(5));
        assert_eq!(anthropic.creation_1h_units, Some(3));
        let debug = format!("{anthropic:?}");
        assert!(!debug.contains("must never be persisted"));
        assert!(!debug.contains("999999"));

        let responses = provider_cache_observation_from_response(
            &cache_test_scope(Protocol::OpenAi),
            &json!({
                "usage": {
                    "input_tokens": 100,
                    "input_tokens_details": {"cached_tokens": 34}
                }
            }),
            dtoken_types::ProviderCacheObservationKind::OpenAiResponsesUsage,
            84,
        )
        .expect("Responses observation");
        assert_eq!(responses.read_units, Some(34));
        assert_eq!(responses.creation_units, None);
    }

    #[test]
    fn provider_cache_observation_does_not_cross_accept_fields_or_infer_capability() {
        let scope = cache_test_scope(Protocol::OpenAi);
        assert!(provider_cache_observation_from_response(
            &scope,
            &json!({"usage": {"input_tokens_details": {"cached_tokens": 9}}}),
            dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
            42,
        )
        .is_none());
        assert!(provider_cache_observation_from_response(
            &scope,
            &json!({"usage": {"prompt_tokens": 10}}),
            dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
            42,
        )
        .is_none());
        assert!(provider_cache_observation_from_response(
            &scope,
            &json!({"usage": {"prompt_tokens_details": {"cached_tokens": -1}}}),
            dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
            42,
        )
        .is_none());
    }

    #[test]
    fn completed_provider_cache_observation_is_persisted_but_failure_is_not() {
        let dir = std::env::temp_dir().join(format!(
            "dtoken-provider-cache-observation-{}",
            Uuid::new_v4()
        ));
        let state = AppState::new_persistent(route_test_config(), &dir);
        let scope = cache_test_scope(Protocol::OpenAi);
        let completed = UpstreamResult {
            response: json!({
                "usage": {"prompt_tokens_details": {"cached_tokens": 13}}
            }),
            status: Some(200),
            error_category: None,
            send_state: UpstreamSendState::Completed,
            quota_metadata: None,
        };
        persist_provider_cache_observation(
            &state,
            &scope,
            &completed,
            dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
        );
        let stored = state
            .context_store
            .as_ref()
            .expect("persistent context store")
            .lock()
            .unwrap()
            .provider_cache_observation(
                &scope.route_id,
                &scope.provider_id,
                &scope.model,
                &scope.output_protocol,
                &scope.config_revision,
            )
            .unwrap()
            .expect("cache observation");
        assert_eq!(stored.read_units, Some(13));

        let failed_scope = RequestScope::new(
            Uuid::new_v4(),
            scope.agent.clone(),
            scope.project.clone(),
            scope.session.clone(),
            scope.instance.clone(),
            "route-a",
            "provider-a",
            "failed-model",
            Protocol::OpenAi,
            Protocol::OpenAi,
            "config-a",
        )
        .unwrap();
        let failed = UpstreamResult {
            response: json!({
                "usage": {"prompt_tokens_details": {"cached_tokens": 99}}
            }),
            status: Some(500),
            error_category: Some("provider_server_error".to_string()),
            send_state: UpstreamSendState::FailedAfterCommit,
            quota_metadata: None,
        };
        persist_provider_cache_observation(
            &state,
            &failed_scope,
            &failed,
            dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
        );
        assert!(state
            .context_store
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .provider_cache_observation(
                &failed_scope.route_id,
                &failed_scope.provider_id,
                &failed_scope.model,
                &failed_scope.output_protocol,
                &failed_scope.config_revision,
            )
            .unwrap()
            .is_none());
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn configured_cache_evidence_requires_exact_fresh_policy_and_observation() {
        let dir =
            std::env::temp_dir().join(format!("dtoken-provider-cache-evidence-{}", Uuid::new_v4()));
        let mut config = route_test_config();
        config
            .provider_cache_policies
            .push(dtoken_types::ProviderCachePolicy {
                route_id: "route-a".to_string(),
                provider_id: "provider-a".to_string(),
                model: "model-a".to_string(),
                strategy: dtoken_types::ProviderCachePolicyStrategy::OpenAiAutomatic,
                ttl_seconds: 300,
                min_prefix_bytes: 1024,
            });
        let route = config.find_route("route-a").unwrap();
        let provider = config.find_provider("provider-a").unwrap();
        let scope = resolve_configured_request_scope(
            &config,
            route,
            provider,
            Uuid::new_v4(),
            Some("model-a"),
            Protocol::OpenAi,
            Protocol::OpenAi,
        )
        .unwrap();
        let state = AppState::new_persistent(config, &dir);
        assert!(configured_provider_cache_evidence_at(&state, &scope, 1_000).is_none());
        let observation = dtoken_types::ProviderCacheObservation::new(
            &scope.route_id,
            &scope.provider_id,
            &scope.model,
            scope.output_protocol.clone(),
            &scope.config_revision,
            dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
            Some(0),
            None,
            None,
            None,
            "provider-cache-observation-v1",
            1_000,
        )
        .unwrap();
        state
            .context_store
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .upsert_provider_cache_observation(&observation)
            .unwrap();

        let evidence = configured_provider_cache_evidence_at(&state, &scope, 1_001)
            .expect("fresh exact policy plus observation");
        assert_eq!(
            evidence.strategy,
            dtoken_compress::context_pipeline::ProviderCacheStrategy::OpenAiAutomatic
        );
        assert_eq!(evidence.ttl_seconds, Some(300));
        assert_eq!(evidence.min_prefix_bytes, 1024);
        assert_eq!(evidence.source_revision.len(), 64);
        assert!(configured_provider_cache_evidence_at(&state, &scope, 301_001).is_none());

        let mut mismatch = scope.clone();
        mismatch.config_revision = "other-config".to_string();
        assert!(configured_provider_cache_evidence_at(&state, &mismatch, 1_001).is_none());
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
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
    fn responses_inventory_diagnostics_are_shadow_only_and_metadata_only() {
        let mut input = Vec::new();
        for index in 0..8 {
            input.push(json!({
                "type":"function_call",
                "id":format!("fc-{index}"),
                "call_id":format!("call-{index}"),
                "name":"read",
                "arguments":"{}"
            }));
            input.push(json!({
                "type":"function_call_output",
                "call_id":format!("call-{index}"),
                "output":"INFO private payload"
            }));
        }
        input.push(json!({"type":"message","role":"user","content":[{"type":"input_text","text":"continue"}]}));
        let body = json!({"model":"mock/model","input":input});
        let context = ResolvedOptimizationContext {
            route_id: "local-mock-route".to_string(),
            provider_id: "local-mock-provider".to_string(),
            model: "mock/model".to_string(),
            input_protocol: Protocol::OpenAi,
            output_protocol: Protocol::OpenAi,
            endpoint: OpticalEndpoint::OpenAiResponses,
        };

        let diagnostics = compression_inventory_diagnostics(&context, &body);
        assert_eq!(diagnostics["state"], "resolved");
        assert_eq!(diagnostics["message_count"], 17);
        assert_eq!(diagnostics["recent_tail_start"], 5);
        assert_eq!(diagnostics["approved_target_count"], 0);
        assert_eq!(diagnostics["raw_available"], false);
        assert!(!diagnostics.to_string().contains("private payload"));
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

    #[tokio::test]
    async fn optical_status_reports_hybrid_backend_evidence_without_enabling_active() {
        let app = build_router(AppState::new(DtokenConfig::empty()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/compression/optical/status")
                    .header("host", "127.0.0.1:18001")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        assert_eq!(payload["schema_version"], 3);
        assert_eq!(payload["representation_plan_ready"], true);
        assert_eq!(payload["responses_shadow_inventory_ready"], true);
        assert_eq!(payload["dynamic_page_accounting_ready"], true);
        assert_eq!(payload["allow_active"], false);
        assert_eq!(payload["active_direct_routes"], 0);
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
            fallback_reason_code: None,
            body: json!({}),
            context_accounting: None,
        };
        let log = build_request_log(
            decision.request_id,
            &decision,
            Some("model-a".to_string()),
            Protocol::OpenAi,
            &compression,
            &RequestAttribution::default(),
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

    #[test]
    fn configured_request_scope_binds_route_config_and_unknown_identity_evidence() {
        let mut config = route_test_config();
        config.instances.push(dtoken_types::InstanceProfile {
            id: "instance-a".to_string(),
            name: "Claude Code".to_string(),
            agent: "claude-code".to_string(),
            route_id: "route-a".to_string(),
            local_port: 18001,
            process_status: dtoken_types::ProcessStatus::Stopped,
            log_path: String::new(),
        });
        let scope = resolve_configured_request_scope(
            &config,
            &config.routes[0],
            &config.providers[0],
            Uuid::new_v4(),
            Some("model-a"),
            Protocol::OpenAi,
            Protocol::OpenAi,
        )
        .expect("configured route should produce a scope");

        assert_eq!(scope.agent.value.as_deref(), Some("claude-code"));
        assert_eq!(scope.project.value, None);
        assert_eq!(
            scope.project.reason_code.as_deref(),
            Some("project_identity_unavailable")
        );
        assert_eq!(scope.route_id, "route-a");
        assert_eq!(scope.provider_id, "provider-a");
        assert_eq!(scope.config_revision.len(), 64);
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
            request_bytes(json!({
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
            request_bytes(json!({"model": "example/model", "input": "hello"})),
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
            request_bytes(json!({"model": "mock/model", "input": "hello"})),
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
            request_bytes(json!({
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
        let events = state.recent_events(20);
        let scope = events
            .iter()
            .find_map(|event| event.metadata.get("request_scope"))
            .expect("configured route event must carry sanitized RequestScope");
        assert_eq!(scope["instance"]["value"], "local-test-instance");
        assert_eq!(scope["agent"]["value"], Value::Null);
        assert_eq!(scope["agent"]["reason_code"], "agent_identity_unavailable");
        assert_eq!(scope["config_revision"].as_str().map(str::len), Some(64));
        server.await.expect("local mock upstream task");
    }

    #[tokio::test]
    async fn configured_chat_falls_back_to_baseline_before_send_when_ccr_store_is_unavailable() {
        let (addr, server) = spawn_json_upstream(
            "200 OK",
            r#"{"id":"chatcmpl-local","choices":[{"message":{"role":"assistant","content":"ok"}}]}"#,
            0,
        )
        .await;
        let state = configured_openai_test_state(format!("http://{addr}/v1"), 1_000);
        let log = "INFO build stable route healthy tests passed\n".repeat(420);
        let mut messages = Vec::new();
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
        messages.push(json!({"role": "user", "content": "Summarize verified state."}));

        let response = openai_chat_completions(
            State(state.clone()),
            HeaderMap::new(),
            request_bytes(json!({
                "model": "mock/model",
                "messages": messages,
                "stream": false
            })),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(state.recent_request_logs(1)[0]
            .transforms_applied
            .is_empty());
        assert!(state.recent_events(20).iter().any(|event| {
            event.metadata["ccr_pre_send"] == "baseline_fallback_store_unavailable"
        }));
        assert!(state.recent_events(20).iter().any(|event| {
            event.metadata["pipeline_payload_variant"] == "original"
                && event.metadata["pipeline_reason_code"] == "no_transform_selected"
        }));
        server.await.expect("local mock upstream task");
    }

    #[tokio::test]
    async fn configured_chat_sends_pipeline_optimized_variant_only_after_ccr_prepare() {
        let (addr, server) = spawn_json_upstream(
            "200 OK",
            r#"{"id":"chatcmpl-local","choices":[{"message":{"role":"assistant","content":"ok"}}]}"#,
            0,
        )
        .await;
        let dir = std::env::temp_dir().join(format!("dtoken-pipeline-{}", Uuid::new_v4()));
        let mut state = configured_openai_test_state(format!("http://{addr}/v1"), 1_000);
        state.ccr_private_store = Some(Arc::new(
            CcrStore::new(dir.join("ccr-private"), [19_u8; 32]).expect("CCR store"),
        ));
        let log = "INFO build stable route healthy tests passed\n".repeat(420);
        let mut messages = Vec::new();
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
        messages.push(json!({"role": "user", "content": "Summarize verified state."}));
        let request = json!({
            "model": "mock/model",
            "messages": messages,
            "stream": false
        });
        let original = serde_json::to_vec(&request).expect("request bytes");

        let response = openai_chat_completions(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from(original.clone()),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(!state.recent_request_logs(1)[0]
            .transforms_applied
            .is_empty());
        assert!(state.recent_events(20).iter().any(|event| {
            event.metadata["pipeline_payload_variant"] == "optimized"
                && event.metadata["pipeline_reason_code"] == "validated_optimized_candidate"
        }));
        let recovery_ref = state.recent_ccr_metadata(1)[0]["recovery_ref"]
            .as_str()
            .expect("recovery ref")
            .to_string();
        assert_eq!(
            state
                .retrieve_ccr_original(&recovery_ref)
                .expect("recover original"),
            original
        );
        server.await.expect("local mock upstream task");
        let _ = std::fs::remove_dir_all(dir);
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
            request_bytes(json!({
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
    fn dashboard_summary_omits_heavy_event_and_private_log_fields() {
        let state = AppState::new(DtokenConfig::empty());
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({"role": "user", "content": "repeat\nrepeat"})]),
            stream: Some(false),
        };
        let (_response, decision, mut request_log, event) = routed_openai_payload(&request);
        request_log.quota_metadata = Some(json!({
            "provider": "codex",
            "source": "codex-response-headers",
            "captured_at": 1_700_000_000_000_u64,
            "stale_after_ms": 900_000_u64,
            "primary": { "used_percent": 20.0, "window_minutes": 300.0 },
            "secondary": { "used_percent": 35.0, "window_minutes": 10080.0 }
        }));
        state.record_routed_request(&decision, request_log, event);

        let detail = dashboard_payload_for_state(&state);
        let summary = dashboard_summary_for_state(&state);

        assert_eq!(summary["source"], "dtoken-owned-runtime");
        assert_eq!(summary["payload_kind"], "summary");
        for key in ["stats", "history", "metrics", "cache_summary"] {
            assert_eq!(summary[key], detail[key], "summary slice drifted for {key}");
        }
        assert!(summary.get("events").is_none());
        assert!(summary.get("memory").is_none());
        assert!(summary["account_quotas"].get("codex").is_none());
        assert_eq!(
            summary["stats"]["analytics"]["models"][0]["name"],
            "opencode/deepseek-v4-flash-free"
        );
        assert_eq!(
            summary["stats"]["analytics"]["protocols"][0]["name"],
            "openai"
        );
        assert_eq!(summary["stats"]["analytics"]["daily"][0]["requests"], 1);
        assert_eq!(summary["stats"]["latency_breakdown"]["status"], "partial");
        assert_eq!(
            summary["stats"]["latency_breakdown"]["total_ms"],
            Value::Null
        );
        assert_eq!(
            summary["stats"]["latency_breakdown"]["route_adapter_ms"],
            Value::Null
        );
        assert_eq!(
            summary["stats"]["latency_breakdown"]["provider_ms"],
            Value::Null
        );
        assert_eq!(summary["stats"]["route_overhead"]["status"], "unavailable");
        assert_eq!(summary["stats"]["route_overhead"]["tokens"], Value::Null);
        let log = summary["request_logs"][0]
            .as_object()
            .expect("compact request log");
        assert!(log.contains_key("timestamp_ms"));
        assert!(log.contains_key("input_tokens_before"));
        assert!(log.contains_key("input_protocol"));
        assert!(log.contains_key("chain_mode"));
        assert!(log.contains_key("config_source"));
        assert!(!log.contains_key("request_id"));
        assert!(!log.contains_key("context_accounting"));
    }

    #[test]
    fn codex_quota_headers_are_reduced_to_allowlisted_numeric_metadata() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-codex-primary-used-percent", "20".parse().unwrap());
        headers.insert("x-codex-primary-window-minutes", "300".parse().unwrap());
        headers.insert("x-codex-primary-reset-at", "1800000000".parse().unwrap());
        headers.insert("x-codex-secondary-used-percent", "35".parse().unwrap());
        headers.insert("x-codex-secondary-window-minutes", "10080".parse().unwrap());
        headers.insert("authorization", "Bearer must-not-escape".parse().unwrap());

        let metadata = codex_quota_metadata(&headers).expect("codex quota metadata");
        assert_eq!(metadata["source"], "codex-response-headers");
        assert_eq!(metadata["primary"]["used_percent"], 20.0);
        assert_eq!(metadata["secondary"]["window_minutes"], 10080.0);
        let serialized = serde_json::to_string(&metadata).unwrap();
        assert!(!serialized.contains("authorization"));
        assert!(!serialized.contains("must-not-escape"));
        assert!(is_chatgpt_upstream("https://chatgpt.com/backend-api"));
        assert!(!is_chatgpt_upstream("https://custom.example/v1"));

        let wham = codex_wham_quota_metadata(&json!({
            "rate_limit": {
                "primary_window": {"used_percent": 21.0, "limit_window_seconds": 18_000, "reset_at": 1_800_000_000},
                "secondary_window": {"used_percent": 36.0, "limit_window_seconds": 604_800, "reset_at": 1_800_000_001}
            },
            "credential": "must-not-escape"
        }), "codex-opaque").expect("wham quota");
        assert_eq!(wham["primary"]["window_minutes"], 300.0);
        assert_eq!(wham["secondary"]["window_minutes"], 10080.0);
        assert!(!serde_json::to_string(&wham)
            .unwrap()
            .contains("must-not-escape"));
        let context_ref = opaque_quota_context_ref(&codex_quota_cache_key(
            "credential-must-not-escape",
            "account-must-not-escape",
        ));
        let mut context_bound = Some(wham.clone());
        attach_quota_context_ref(&mut context_bound, Some(&context_ref));
        let serialized = serde_json::to_string(&context_bound).unwrap();
        assert!(serialized.contains("quota-context-"));
        assert!(!serialized.contains("credential-must-not-escape"));
        assert!(!serialized.contains("account-must-not-escape"));

        let claude = claude_usage_metadata(
            &json!({
                "five_hour": {"utilization": 42.0, "resets_at": "2026-07-16T12:00:00Z"},
                "seven_day": {"utilization": 65.0, "resets_at": "2026-07-20T12:00:00Z"},
                "token": "must-not-escape"
            }),
            "claude-opaque",
        )
        .expect("Claude usage metadata");
        assert_eq!(claude["five_hour"]["utilization_pct"], 42.0);
        assert_eq!(claude["seven_day"]["utilization_pct"], 65.0);
        assert_eq!(claude["account_ref"], "claude-opaque");
        assert!(!serde_json::to_string(&claude)
            .unwrap()
            .contains("must-not-escape"));
    }

    #[test]
    fn quota_cache_is_bounded_and_never_falls_back_across_accounts() {
        let mut cache = HashMap::new();
        for index in 0..=MAX_QUOTA_CACHE_ACCOUNTS {
            let key = format!("account-{index}");
            let entry = quota_cache_entry_mut(&mut cache, &key);
            entry.payload = Some(json!({"account_ref": key}));
            entry.captured_at = Some(Instant::now());
            entry.last_attempt = Some(Instant::now());
        }

        assert_eq!(cache.len(), MAX_QUOTA_CACHE_ACCOUNTS);
        assert_eq!(
            active_cached_quota(
                &cache,
                Some(&format!("account-{MAX_QUOTA_CACHE_ACCOUNTS}")),
                Duration::from_secs(15 * 60),
            )
            .expect("active quota")["account_ref"],
            format!("account-{MAX_QUOTA_CACHE_ACCOUNTS}")
        );
        assert!(active_cached_quota(&cache, None, Duration::from_secs(15 * 60)).is_none());
        assert!(active_cached_quota(
            &cache,
            Some("account-not-present"),
            Duration::from_secs(15 * 60)
        )
        .is_none());

        let stale_key = format!("account-{MAX_QUOTA_CACHE_ACCOUNTS}");
        cache.get_mut(&stale_key).unwrap().captured_at =
            Some(Instant::now() - Duration::from_secs(16 * 60));
        assert!(
            active_cached_quota(&cache, Some(&stale_key), Duration::from_secs(15 * 60)).is_none()
        );
    }

    #[test]
    fn historical_quota_is_kept_only_for_the_current_fresh_account() {
        let now = dtoken_event_ledger::now_ms();
        let mut account_quotas = json!({
            "codex": {
                "account_ref": "account-a",
                "captured_at": now,
                "stale_after_ms": 900_000
            }
        })
        .as_object()
        .unwrap()
        .clone();

        reconcile_provider_quota(&mut account_quotas, "codex", Some("account-b"), None, None);
        assert!(account_quotas.get("codex").is_none());

        account_quotas.insert(
            "codex".to_string(),
            json!({
                "account_ref": "account-b",
                "captured_at": now.saturating_sub(900_001),
                "stale_after_ms": 900_000
            }),
        );
        reconcile_provider_quota(&mut account_quotas, "codex", Some("account-b"), None, None);
        assert!(account_quotas.get("codex").is_none());

        let current = json!({
            "account_ref": "account-b",
            "captured_at": now,
            "stale_after_ms": 900_000
        });
        account_quotas.insert("codex".to_string(), current.clone());
        reconcile_provider_quota(&mut account_quotas, "codex", Some("account-b"), None, None);
        assert_eq!(account_quotas.get("codex"), Some(&current));

        account_quotas.insert(
            "codex".to_string(),
            json!({
                "account_ref": "account-b",
                "account_context_ref": "context-a",
                "captured_at": now,
                "stale_after_ms": 900_000
            }),
        );
        reconcile_provider_quota(
            &mut account_quotas,
            "codex",
            Some("account-b"),
            Some("context-b"),
            None,
        );
        assert!(account_quotas.get("codex").is_none());

        let context_bound = json!({
            "account_ref": "account-b",
            "account_context_ref": "context-b",
            "captured_at": now,
            "stale_after_ms": 900_000
        });
        account_quotas.insert("codex".to_string(), context_bound.clone());
        reconcile_provider_quota(
            &mut account_quotas,
            "codex",
            Some("account-b"),
            Some("context-b"),
            None,
        );
        assert_eq!(account_quotas.get("codex"), Some(&context_bound));

        let refreshed = json!({"account_ref": "account-c", "captured_at": now});
        reconcile_provider_quota(
            &mut account_quotas,
            "codex",
            Some("account-c"),
            Some("context-c"),
            Some(&refreshed),
        );
        assert_eq!(account_quotas.get("codex"), Some(&refreshed));
    }

    #[test]
    fn quota_global_gate_is_single_flight_and_respects_attempt_floor() {
        let gate = OnceLock::new();

        assert!(claim_quota_global_gate(&gate, Duration::from_secs(30)));
        assert!(!claim_quota_global_gate(&gate, Duration::ZERO));
        release_quota_global_gate(&gate);
        assert!(!claim_quota_global_gate(&gate, Duration::from_secs(30)));

        let immediate_gate = OnceLock::new();
        assert!(claim_quota_global_gate(&immediate_gate, Duration::ZERO));
        release_quota_global_gate(&immediate_gate);
        assert!(claim_quota_global_gate(&immediate_gate, Duration::ZERO));
        release_quota_global_gate(&immediate_gate);
    }

    #[test]
    fn active_quota_key_can_be_cleared_when_account_context_disappears() {
        let active = OnceLock::new();

        set_active_quota_key(&active, Some("opaque-account".to_string()));
        assert_eq!(active_quota_key(&active).as_deref(), Some("opaque-account"));
        set_active_quota_key(&active, None);
        assert!(active_quota_key(&active).is_none());
    }

    #[tokio::test]
    async fn analytics_request_cursor_returns_stable_bounded_pages() {
        let state = AppState::new(DtokenConfig::empty());
        for index in 0..12 {
            let request = OpenAiChatRequest {
                model: Some(format!("model-{index}")),
                messages: Some(vec![json!({"role": "user", "content": "private"})]),
                stream: Some(false),
            };
            let (_response, decision, request_log, event) = routed_openai_payload(&request);
            state.record_routed_request(&decision, request_log, event);
        }

        let Json(first) = analytics_requests(
            State(state.clone()),
            Query(AnalyticsRequestsQuery {
                cursor: None,
                limit: Some(5),
            }),
        )
        .await;
        assert_eq!(first["returned_count"], 5);
        assert_eq!(first["total_count"], 12);
        assert_eq!(first["has_more"], true);
        let cursor = first["next_cursor"].as_str().unwrap().to_string();

        let Json(second) = analytics_requests(
            State(state),
            Query(AnalyticsRequestsQuery {
                cursor: Some(cursor),
                limit: Some(5),
            }),
        )
        .await;
        assert_eq!(second["returned_count"], 5);
        assert_eq!(second["has_more"], true);
        let serialized = serde_json::to_string(&second).unwrap();
        assert!(!serialized.contains("private"));
    }

    #[test]
    fn terminal_persistence_performance_reports_four_durable_boundaries_without_content() {
        let runtime_dir =
            std::env::temp_dir().join(format!("dtoken-runtime-performance-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&runtime_dir).expect("create runtime performance fixture");
        let state = AppState::new_persistent(DtokenConfig::empty(), &runtime_dir);
        let request = OpenAiChatRequest {
            model: Some("fixture/model".to_string()),
            messages: Some(vec![json!({"role": "user", "content": "private fixture"})]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        state.record_routed_request(&decision, request_log, event);

        let summary = dashboard_summary_for_state(&state);
        let stages = &summary["performance"]["stages"];
        assert_eq!(stages["terminal_commit_total"]["samples"], 1);
        assert_eq!(stages["terminal_commit_total"]["sync_count"], 4);
        for stage in [
            "terminal_event_sync",
            "terminal_route_sync",
            "terminal_request_log_sync",
            "terminal_ccr_sync",
        ] {
            assert_eq!(stages[stage]["samples"], 1, "missing {stage}");
            assert_eq!(
                stages[stage]["sync_count"], 1,
                "wrong sync count for {stage}"
            );
        }
        let serialized = summary["performance"].to_string();
        assert!(!serialized.contains("private fixture"));
        assert!(!serialized.contains("fixture/model"));
        assert!(!serialized.contains(runtime_dir.to_string_lossy().as_ref()));
        drop(state);
        std::fs::remove_dir_all(runtime_dir).expect("remove runtime performance fixture");
    }

    #[test]
    #[ignore = "release-profile performance baseline"]
    fn dashboard_summary_performance_is_history_size_independent() {
        let request = OpenAiChatRequest {
            model: Some("fixture/model".to_string()),
            messages: Some(vec![json!({"role": "user", "content": "fixture"})]),
            stream: Some(false),
        };
        let (_response, _decision, base_log, _event) = routed_openai_payload(&request);

        for history_size in [0_usize, 1_000, 10_000, 100_000] {
            let state = AppState::new(DtokenConfig::empty());
            let logs = (0..history_size)
                .map(|index| {
                    let mut log = base_log.clone();
                    log.timestamp_ms = 1_700_000_000_000
                        + u64::try_from(index % 24).unwrap_or_default() * 3_600_000;
                    log.latency_ms = Some(u64::try_from(index % 25).unwrap_or_default());
                    log
                })
                .collect::<Vec<_>>();
            *state
                .request_logs
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = logs;
            state.rebuild_runtime_summary();
            let _ = dashboard_summary_for_state(&state);

            let mut samples = Vec::with_capacity(200);
            for _ in 0..200 {
                let started_at = Instant::now();
                let summary = dashboard_summary_for_state(&state);
                assert_eq!(
                    summary["stats"]["requests"]["total"],
                    u64::try_from(history_size).unwrap_or(u64::MAX)
                );
                samples.push(u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX));
            }
            samples.sort_unstable();
            let p95_us = percentile(&samples, 95);
            eprintln!("dashboard_summary history={history_size} p95_us={p95_us}");
            assert!(
                p95_us <= 25_000,
                "summary p95 {p95_us}us exceeded 25ms for {history_size} records"
            );
        }
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
            provider_cache_policies: vec![],
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
            provider_cache_policies: vec![],
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
        assert_eq!(request_log.agent, None);
        assert_eq!(request_log.instance_id, None);
        assert_eq!(event.agent_id, None);
        assert_eq!(event.project_id, None);
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
            provider_cache_policies: vec![],
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
        assert_eq!(request_logs[0].agent, None);
        assert_eq!(events[0].project_id, None);
        assert_eq!(events[0].source, EventSource::Dtoken);
    }

    #[test]
    fn durable_event_failure_never_creates_in_memory_success_terminal() {
        let dir = std::env::temp_dir().join(format!("dtoken-durable-first-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let events_path = dir.join("events.jsonl");
        let _ = std::fs::remove_file(&events_path);
        std::fs::create_dir_all(&events_path).expect("fault-injection directory");
        let request = OpenAiChatRequest {
            model: Some("test-model".to_string()),
            messages: Some(vec![json!({"role": "user", "content": "hello"})]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        state.record_routed_request(&decision, request_log, event);
        assert!(state.recent_events(10).is_empty());
        assert!(state.recent_request_logs(10).is_empty());
        assert!(state.recent_route_decisions(10).is_empty());
        let _ = std::fs::remove_dir_all(dir);
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
            None,
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
        assert!(!metrics.contains("dtoken_route_adapter_latency_ms_avg"));
        assert!(!metrics.contains("dtoken_provider_latency_ms_avg"));
        assert!(!metrics.contains("dtoken_route_overhead_tokens_total"));
        assert!(!metrics.contains("dtoken_route_overhead_percent"));
        assert_eq!(stats["latency_breakdown"]["model"], "compression_only");
        assert_eq!(stats["latency_breakdown"]["status"], "partial");
        assert!(
            stats["latency_breakdown"]["compression_ms"]
                .as_f64()
                .unwrap_or_default()
                > 0.0
        );
        assert_eq!(stats["latency_breakdown"]["route_adapter_ms"], Value::Null);
        assert_eq!(stats["latency_breakdown"]["provider_ms"], Value::Null);
        assert_eq!(
            stats["route_overhead"]["fallback_policy"],
            "disabled_unless_route_explicitly_configures_fallback"
        );
        assert_eq!(stats["route_overhead"]["status"], "unavailable");
        assert_eq!(stats["route_overhead"]["tokens"], Value::Null);
        assert_eq!(stats["route_overhead"]["percent"], Value::Null);
        assert_eq!(
            metrics_payload["compression_performance"]["total_requests"],
            1
        );
        assert_eq!(
            metrics_payload["route_performance"]["latency_breakdown"]["model"],
            "compression_only"
        );
        assert_eq!(
            metrics_payload["route_performance"]["route_overhead"]["status"],
            "unavailable"
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
    fn ccr_private_store_recovers_exact_original_without_leaking_to_metadata_or_export() {
        let dir = std::env::temp_dir().join(format!("dtoken-ccr-private-{}", Uuid::new_v4()));
        let key = [31_u8; 32];
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("private CCR state");
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "CCR_PRIVATE_SECRET must remain outside metadata"
            })]),
            stream: Some(false),
        };
        let (_response, decision, mut request_log, mut event) = routed_openai_payload(&request);
        request_log.tokens_saved = Some(17);
        request_log.transforms_applied = vec!["content_router:json".to_string()];
        event.saved_tokens = Some(17);
        let original = br#"{  "model":"opencode/deepseek-v4-flash-free", "messages":[{"role":"user","content":"CCR_PRIVATE_SECRET must remain outside metadata"}] }"#;

        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, original)
            .expect("CCR must be durable before upstream send");
        let recovery_ref = format!("ccr:{}", decision.request_id);
        assert_eq!(
            state
                .retrieve_ccr_original(&recovery_ref)
                .expect("prepared CCR must already be retrievable"),
            original
        );
        state.record_routed_request_with_ccr_receipt(&decision, request_log, event, &prepared);
        let projections = ContextStore::open(dir.join("runtime-state.sqlite3"))
            .expect("open runtime projection store");
        assert_eq!(projections.projection_counts().unwrap(), (1, 1));
        let metadata = state
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("CCR metadata");
        let summary = runtime_cache_summary_for_state(&state);
        let export = runtime_export_json_payload(&state).to_string();

        assert_eq!(metadata["raw_available"], true);
        assert_eq!(metadata["raw_storage"], "encrypted-private-store");
        assert_eq!(summary["private_entries"], 1);
        assert_eq!(summary["raw_available"], true);
        assert!(!metadata.to_string().contains("CCR_PRIVATE_SECRET"));
        assert!(!export.contains("CCR_PRIVATE_SECRET"));

        let restarted = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("restarted private CCR state");
        assert_eq!(
            restarted
                .retrieve_ccr_original(&recovery_ref)
                .expect("exact CCR original"),
            original
        );
        assert!(restarted
            .delete_ccr_original(&recovery_ref)
            .expect("delete private CCR original"));
        assert!(restarted.retrieve_ccr_original(&recovery_ref).is_err());

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
        assert_eq!(shape["state"], "metadata-only");
        assert_eq!(shape["active"], false);
        assert_eq!(shape["applied_requests"], 0);
        assert_eq!(shape["accounting_evidence"], "none");
        assert_eq!(
            shape["learned_rules"][0]["summary"],
            "Prefer terse command output unless tests fail."
        );
        assert!(summary.get("raw_prompt").is_none());
        assert!(!summary.to_string().contains("must not be stored"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn headroom_mcp_status_fails_closed_until_transport_and_handlers_exist() {
        let state = AppState::new(DtokenConfig::empty());
        let Json(payload) = headroom_mcp(State(state)).await;

        assert_eq!(payload["state"], "unavailable");
        assert_eq!(payload["configured"], false);
        assert_eq!(payload["freshness"], "unknown");
        assert_eq!(payload["capability_revision"], BACKEND_CAPABILITY_REVISION);
        assert_eq!(
            payload["reason_codes"],
            json!(["mcp_transport_not_implemented"])
        );
        assert_eq!(payload["transport"], Value::Null);
        assert_eq!(payload["tools"], json!([]));
        assert_eq!(
            payload["planned_tools"],
            json!(["headroom_compress", "headroom_retrieve", "headroom_stats"])
        );
    }

    #[tokio::test]
    async fn backend_capability_registry_is_the_mcp_status_source() {
        let state = AppState::new(DtokenConfig::empty());
        let Json(registry) = backend_capabilities(State(state.clone())).await;
        let Json(mcp) = headroom_mcp(State(state)).await;
        let capability = registry.find("mcp").expect("MCP registry entry");

        assert_eq!(
            capability.state,
            dtoken_types::AvailabilityState::Unavailable
        );
        assert_eq!(mcp["state"], json!(capability.state));
        assert_eq!(mcp["freshness"], json!(capability.freshness));
        assert_eq!(mcp["reason_codes"], json!(capability.reason_codes));
        assert_eq!(mcp["capability_revision"], registry.revision);
    }

    #[test]
    fn successful_agent_scan_produces_ready_capability_evidence() {
        let snapshot = AgentDiscoverySnapshot {
            observed_at_ms: 42,
            evidence_revision: "agent-scan-42".to_string(),
            agents: vec![],
        };

        let registry = backend_capability_registry(
            Some(&snapshot),
            ContextStoreCapabilityState::NotConfigured,
        );
        let capability = registry
            .find("agent-registry")
            .expect("Agent registry capability");

        assert_eq!(capability.state, AvailabilityState::Ready);
        assert_eq!(capability.freshness, EvidenceFreshness::Live);
        assert_eq!(
            capability.evidence_revision.as_deref(),
            Some("agent-scan-42")
        );
        assert_eq!(
            capability.reason_codes,
            vec!["agent_scan_complete_no_agents_detected"]
        );
    }

    #[test]
    fn runtime_pipeline_selects_optimized_json_only_with_a_durable_ccr_receipt() {
        let scope = RequestScope::new(
            Uuid::new_v4(),
            EvidenceValue::unknown("agent_unavailable"),
            EvidenceValue::unknown("project_unavailable"),
            EvidenceValue::unknown("session_unavailable"),
            EvidenceValue::known("instance-a".to_string(), EvidenceSource::ConfiguredRoute),
            "route-a",
            "provider-a",
            "model-a",
            Protocol::OpenAi,
            Protocol::OpenAi,
            "config-a",
        )
        .expect("scope");
        let baseline =
            json!({"model":"model-a","messages":[{"role":"user","content":"repeat repeat"}]});
        let candidate = json!({"model":"model-a","messages":[{"role":"user","content":"repeat"}]});
        let transforms = vec!["semantic_text".to_string()];
        let missing = PreparedCcr {
            receipt: None,
            store_configured: false,
        };

        let fallback = prepare_json_pipeline_payload(
            scope.clone(),
            &baseline,
            &candidate,
            &transforms,
            &missing,
        )
        .expect("baseline fallback");
        assert_eq!(
            fallback.ready.variant(),
            dtoken_types::SentPayloadVariant::Original
        );
        assert_eq!(
            fallback.fallback_reason,
            Some("pipeline_recovery_unavailable")
        );

        let prepared = PreparedCcr {
            receipt: Some(CcrStoreReceipt {
                recovery_ref: format!("ccr:{}", scope.request_id),
                expires_at_unix: 100,
                plaintext_bytes: 64,
            }),
            store_configured: true,
        };
        let optimized =
            prepare_json_pipeline_payload(scope, &baseline, &candidate, &transforms, &prepared)
                .expect("optimized payload");
        assert_eq!(
            optimized.ready.variant(),
            dtoken_types::SentPayloadVariant::Optimized
        );
        assert_eq!(optimized.fallback_reason, None);
        assert_eq!(
            serde_json::from_slice::<Value>(optimized.ready.body()).unwrap(),
            candidate
        );
    }

    #[test]
    fn context_store_persists_the_same_agent_snapshot_as_capability_evidence() {
        let dir = std::env::temp_dir().join(format!("dtoken-agent-store-{}", Uuid::new_v4()));
        let snapshot = AgentDiscoverySnapshot {
            observed_at_ms: 84,
            evidence_revision: "agent-scan-84".to_string(),
            agents: vec![],
        };
        let registry = backend_capability_registry(
            Some(&snapshot),
            ContextStoreCapabilityState::Ready { schema_version: 2 },
        );
        let mut store =
            ContextStore::open(dir.join("runtime-state.sqlite3")).expect("open context store");
        persist_context_store_evidence(&mut store, &registry, Some(&snapshot))
            .expect("persist context evidence");

        assert_eq!(
            store.agent_snapshot().expect("Agent snapshot"),
            Some(snapshot)
        );
        assert_eq!(
            store
                .capability("agent-registry")
                .expect("Agent capability")
                .expect("stored Agent capability")
                .evidence_revision
                .as_deref(),
            Some("agent-scan-84")
        );
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn persistent_context_store_capability_is_ready_and_open_failure_is_honest() {
        let ready_dir =
            std::env::temp_dir().join(format!("dtoken-context-ready-{}", Uuid::new_v4()));
        let ready = AppState::new_persistent(DtokenConfig::empty(), &ready_dir);
        let ready_capability = ready
            .capabilities
            .find("context-store")
            .expect("context-store capability");
        assert_eq!(ready_capability.state, AvailabilityState::Ready);
        assert_eq!(
            ready_capability.evidence_revision.as_deref(),
            Some("context-store-schema-v4")
        );

        let blocked_root =
            std::env::temp_dir().join(format!("dtoken-context-blocked-{}", Uuid::new_v4()));
        std::fs::write(&blocked_root, b"not a directory").expect("blocked runtime root");
        let failed = AppState::new_persistent(DtokenConfig::empty(), &blocked_root);
        let failed_capability = failed
            .capabilities
            .find("context-store")
            .expect("failed context-store capability");
        assert_eq!(failed_capability.state, AvailabilityState::Unavailable);
        assert_eq!(
            failed_capability.reason_codes,
            vec!["context_store_open_failed"]
        );
        assert!(failed.context_store.is_none());

        let _ = std::fs::remove_dir_all(ready_dir);
        let _ = std::fs::remove_file(blocked_root);
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
            provider_cache_policies: vec![],
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
            quota_metadata: None,
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
        let response = to_bytes(Body::from_stream(body), usize::MAX)
            .await
            .expect("read SSE body");
        let response = String::from_utf8_lossy(&response);
        assert!(response.contains("event: message_start"));
        assert!(response.contains("event: message_stop"));
        server.await.expect("mock upstream task");
    }

    #[tokio::test]
    async fn terminal_event_stream_records_completed_and_failed_terminal_events() {
        use axum::body::to_bytes;
        use futures_util::stream;

        let completed_state = AppState::new(DtokenConfig::default());
        let completed_request_id = Uuid::new_v4();
        let completed_stream: UpstreamByteStream = Box::pin(stream::iter(vec![Ok(
            Bytes::from_static(b"event: message_stop\n\n"),
        )]));
        let completed_body = Body::from_stream(TerminalEventStream::new(
            completed_stream,
            StreamTerminalLedgerGuard {
                state: completed_state.clone(),
                request_id: completed_request_id,
                provider_id: "fixture-provider".to_string(),
                upstream_status: 200,
                kind: "passthrough",
                accounting: None,
                terminal: false,
            },
        ));
        to_bytes(completed_body, usize::MAX)
            .await
            .expect("completed stream body");
        let completed = completed_state
            .recent_events(10)
            .into_iter()
            .find(|event| event.request_id == Some(completed_request_id))
            .expect("completed terminal event");
        assert_eq!(completed.status, EventStatus::Ok);
        assert_eq!(completed.metadata["optical_phase"], "upstream_completed");
        assert_eq!(completed.metadata["terminal"], true);

        let failed_state = AppState::new(DtokenConfig::default());
        let failed_request_id = Uuid::new_v4();
        let failed_stream: UpstreamByteStream = Box::pin(stream::iter(vec![Err(
            std::io::Error::other("fixture stream failure"),
        )]));
        let failed_body = Body::from_stream(TerminalEventStream::new(
            failed_stream,
            StreamTerminalLedgerGuard {
                state: failed_state.clone(),
                request_id: failed_request_id,
                provider_id: "fixture-provider".to_string(),
                upstream_status: 200,
                kind: "semantic",
                accounting: None,
                terminal: false,
            },
        ));
        assert!(to_bytes(failed_body, usize::MAX).await.is_err());
        let failed = failed_state
            .recent_events(10)
            .into_iter()
            .find(|event| event.request_id == Some(failed_request_id))
            .expect("failed terminal event");
        assert_eq!(failed.status, EventStatus::Failed);
        assert_eq!(failed.metadata["optical_phase"], "upstream_failed");
        assert_eq!(failed.metadata["terminal"], true);
        assert_eq!(failed.metadata["kind"], "semantic");
        assert_eq!(failed.error_code.as_deref(), Some("provider_stream_error"));
    }
}
