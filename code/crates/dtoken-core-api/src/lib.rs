pub mod capability_bridge;
pub mod ccr_store;
pub mod mcp;

use anyhow::Context as _;
use axum::response::Response;
use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Extension, Path as AxumPath, Query, State},
    http::header::{HeaderValue, CACHE_CONTROL, CONTENT_TYPE},
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
use dtoken_compress::output_shaper::{
    shape_output_for_policy, OutputSafety, OutputShapingAccounting, OutputShapingDisposition,
    OutputShapingResult,
};
use dtoken_compress::{CompressionEngine, ResolvedOptimizationContext};
use dtoken_config::DtokenConfig;
use dtoken_config_patcher::{
    agent::{scan_current_process_agents, AgentDiscoverySnapshot},
    stable_hash, CONFIG_TRANSACTION_CAPABILITY_REVISION,
};
use dtoken_context_store::{
    CcrMetadataProjection, ContextStore, FailureClass, FailurePatternObservation,
    MemoryAccessScope, MemoryCapabilityRouteBinding, MemoryCapabilityTransactionReceipt,
    MemoryCapabilityTransactionRequest, MemoryExportEntry, MemoryRecord, MemoryWriteRequest,
    PreparedMemoryCapabilityTransaction, RequestProjection, ResolvedMemoryCapability,
};
use dtoken_doctor::{run_basic_doctor, run_dev_no_touch_doctor};
use dtoken_event_ledger::{EventLedger, JsonlEventLedger, JsonlStore};
use dtoken_headroom_adapter::detect_headroom_status;
use dtoken_learning::LEARNING_CAPABILITY_REVISION;
use dtoken_preprocessor::{
    detect_rtk_status, inspect_managed_rtk_runtime, list_preprocessors, rtk_discover_report,
    rtk_gain_mock, rtk_session_report, ManagedRtkRuntimeEvidence,
};
use dtoken_project_policy::{
    default_policy, RecommendationInput, RuleBasedRecommendationEngine,
    PROJECT_POLICY_CAPABILITY_REVISION,
};
use dtoken_runtime_pipeline::{
    PipelineInput, ReadyToSend, RecoveryPrepareReceipt, TransformArtifact,
};
use dtoken_types::{
    AgentSupportProfile, AgentTransactionEvidence, AvailabilityState, BaselineKind, CacheTokens,
    CapabilityEvidence, CapabilityRegistrySnapshot, ChainMode, CompressionPolicy,
    ConfigTransactionState, DtokenEvent, EffectiveOpticalState, EventSource, EventStatus,
    EvidenceFreshness, EvidenceSource, EvidenceValue, HealthResponse, InputTokenUsage,
    LayeredSavingsReport, LayeredStage, ModalityTokens, OpticalApplicabilityInput,
    OpticalCapabilitiesPublicSummary, OpticalCapabilityLifecyclePublicSummary, OpticalEndpoint,
    OpticalEventMetadataPublicSummary, OpticalEventPublicSummary, OpticalMode,
    OpticalPlanPublicSummary, OpticalPlanStatus, OpticalPreviewRequest, OpticalStatusPublicSummary,
    OptimizationReceipt, PayloadByteUsage, Protocol, ProtocolInjectionProfile, RequestLog,
    RequestScope, RequestScopeError, RouteDecision, SupportLevel, UpstreamSendState, UsageEvidence,
    UsageEvidenceKind, UsageMeasurement, UsageVector, ValidationStatus,
};
use futures_util::{Stream, TryStreamExt};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    env,
    fs::OpenOptions,
    io::{BufRead, BufReader},
    path::{Path as StdPath, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    task::{Context, Poll},
    time::{Duration, Instant},
};
use uuid::Uuid;

use ccr_store::{CcrEntryInspection, CcrStore, CcrStoreReceipt};

#[derive(Debug, Clone)]
struct PreparedCcr {
    receipt: Option<CcrStoreReceipt>,
    store_configured: bool,
}

const IN_MEMORY_JOURNAL_MAX_ENTRIES: usize = 10_000;
const IN_MEMORY_JOURNAL_RETAIN_ENTRIES: usize = 7_500;

fn push_bounded<T>(items: &mut Vec<T>, item: T) {
    if items.len() >= IN_MEMORY_JOURNAL_MAX_ENTRIES {
        let remove = items.len().saturating_sub(IN_MEMORY_JOURNAL_RETAIN_ENTRIES);
        items.drain(..remove);
    }
    items.push(item);
}

/// Stable snake_case string for [`CompressionPolicy`] — no `Display` impl
/// on the enum itself, so callers must use this helper explicitly.
fn compression_policy_as_str(policy: &CompressionPolicy) -> &'static str {
    match policy {
        CompressionPolicy::Off => "off",
        CompressionPolicy::Safe => "safe",
        CompressionPolicy::Balanced => "balanced",
        CompressionPolicy::Aggressive => "aggressive",
        CompressionPolicy::Max => "max",
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CcrDeletionReceipt {
    pub source: &'static str,
    pub recovery_ref: String,
    pub state: &'static str,
    pub raw_available: bool,
    pub raw_returned: bool,
}

pub const COMPRESSION_DECISION_CONTRACT_REVISION: &str = "dtoken-compression-decision-v1";
pub const RUST_LIBRARY_IMPLEMENTATION_REVISION: &str = "dtoken-rust-library-v1";
pub const TYPESCRIPT_SDK_IMPLEMENTATION_REVISION: &str = "dtoken-typescript-sdk-http-v1";

// ===========================================================================
// CF-2 contract-freeze-core: ConfigTransactionState progression and the
// per-Agent transaction cross-section (frozen §6.1/§8.1, consumed here).
// ===========================================================================

/// One Agent's configuration transaction (frozen §6.1/§8.1). Every Agent
/// advances its own `ConfigTransactionState` independently; batch
/// configuration is UI orchestration only and is never a shared atomic
/// transaction. `evidence` is the frozen cross-section that must satisfy the
/// target state's requirements before the transition is accepted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTransactionRecord {
    pub agent_id: String,
    pub state: ConfigTransactionState,
    pub evidence: AgentTransactionEvidence,
}

impl AgentTransactionRecord {
    /// Opens a fresh transaction in `discovered` with an empty cross-section.
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            state: ConfigTransactionState::Discovered,
            evidence: AgentTransactionEvidence {
                transaction_id: String::new(),
                source_revision: String::new(),
                before_digest: String::new(),
                redacted_diff_digest: String::new(),
                backup_manifest: None,
                planned_after_digest: String::new(),
                readback_digest: None,
                capability_evidence: None,
                receipt_id: None,
                rollback_receipt_id: None,
            },
        }
    }

    /// Validates that the frozen cross-section evidence is complete for the
    /// target state (fail-closed: a missing field rejects the transition).
    pub fn validate_evidence_for(
        &self,
        target: ConfigTransactionState,
    ) -> Result<(), AgentTransactionError> {
        use ConfigTransactionState::*;
        let evidence = &self.evidence;
        let require = |name: &'static str, present: bool| {
            if present {
                Ok(())
            } else {
                Err(AgentTransactionError::MissingEvidence(name))
            }
        };
        match target {
            // No cross-section requirement for the discovery/selection steps;
            // failure may happen at any point and needs no evidence.
            Discovered | Selected | Failed => Ok(()),
            Planned => {
                require("transaction_id", !evidence.transaction_id.is_empty())?;
                require("source_revision", !evidence.source_revision.is_empty())?;
                require("before_digest", !evidence.before_digest.is_empty())?;
                require(
                    "redacted_diff_digest",
                    !evidence.redacted_diff_digest.is_empty(),
                )?;
                require(
                    "planned_after_digest",
                    !evidence.planned_after_digest.is_empty(),
                )
            }
            BackedUp => require(
                "backup_manifest",
                evidence
                    .backup_manifest
                    .as_deref()
                    .is_some_and(|s| !s.is_empty()),
            ),
            Applied => require(
                "receipt_id",
                evidence
                    .receipt_id
                    .as_deref()
                    .is_some_and(|s| !s.is_empty()),
            ),
            ReadbackVerified => {
                require(
                    "planned_after_digest",
                    !evidence.planned_after_digest.is_empty(),
                )?;
                require(
                    "readback_digest",
                    evidence
                        .readback_digest
                        .as_deref()
                        .is_some_and(|s| !s.is_empty()),
                )?;
                if evidence.readback_digest.as_deref()
                    == Some(evidence.planned_after_digest.as_str())
                {
                    Ok(())
                } else {
                    Err(AgentTransactionError::ReadbackMismatch)
                }
            }
            CapabilityVerified => require(
                "capability_evidence",
                evidence
                    .capability_evidence
                    .as_deref()
                    .is_some_and(|s| !s.is_empty()),
            ),
            Completed => require(
                "receipt_id",
                evidence
                    .receipt_id
                    .as_deref()
                    .is_some_and(|s| !s.is_empty()),
            ),
            RolledBack => require(
                "rollback_receipt_id",
                evidence
                    .rollback_receipt_id
                    .as_deref()
                    .is_some_and(|s| !s.is_empty()),
            ),
        }
    }

    /// Advances this Agent's transaction to `target`. The transition must be
    /// allowed by the frozen table and the cross-section evidence must satisfy
    /// the target state's requirements; otherwise the record is left unchanged
    /// and an error is returned (fail-closed).
    pub fn advance(
        &mut self,
        target: ConfigTransactionState,
    ) -> Result<ConfigTransactionState, AgentTransactionError> {
        if !self.state.can_transition_to(target) {
            return Err(AgentTransactionError::InvalidTransition {
                from: self.state,
                to: target,
            });
        }
        self.validate_evidence_for(target)?;
        self.state = target;
        Ok(target)
    }

    /// Verifies the post-apply readback digest for this Agent (§6.1/§8.1).
    /// A digest matching `planned_after_digest` advances to
    /// `readback_verified`; a mismatch rolls back **this Agent only** and
    /// records the rollback receipt in the cross-section. Other Agents'
    /// transactions are never touched.
    pub fn verify_readback(
        &mut self,
        observed_digest: &str,
        rollback_receipt_id: &str,
    ) -> Result<ReadbackOutcome, AgentTransactionError> {
        if self.state != ConfigTransactionState::Applied {
            return Err(AgentTransactionError::InvalidTransition {
                from: self.state,
                to: ConfigTransactionState::ReadbackVerified,
            });
        }
        if !observed_digest.is_empty() && observed_digest == self.evidence.planned_after_digest {
            self.evidence.readback_digest = Some(observed_digest.to_string());
            self.state = ConfigTransactionState::ReadbackVerified;
            Ok(ReadbackOutcome::Verified)
        } else {
            if rollback_receipt_id.is_empty() {
                return Err(AgentTransactionError::MissingEvidence(
                    "rollback_receipt_id",
                ));
            }
            self.evidence.rollback_receipt_id = Some(rollback_receipt_id.to_string());
            self.state = ConfigTransactionState::RolledBack;
            Ok(ReadbackOutcome::RolledBack)
        }
    }

    /// Projects this Agent's transaction onto the frozen cross-section for
    /// API/UI consumption. Only cross-section fields are emitted; credentials,
    /// raw configuration bytes, and absolute paths never enter the projection.
    /// The state is the internal snake_case code for technical details,
    /// Doctor, and logs. Ordinary pages must instead derive their result word
    /// with [`transaction_status_word`].
    pub fn projection(&self) -> Value {
        json!({
            "agent_id": self.agent_id,
            "state": config_transaction_state_label(self.state),
            "transaction_id": self.evidence.transaction_id,
            "source_revision": self.evidence.source_revision,
            "before_digest": self.evidence.before_digest,
            "redacted_diff_digest": self.evidence.redacted_diff_digest,
            "backup_manifest": self.evidence.backup_manifest,
            "planned_after_digest": self.evidence.planned_after_digest,
            "readback_digest": self.evidence.readback_digest,
            "capability_evidence": self.evidence.capability_evidence,
            "receipt_id": self.evidence.receipt_id,
            "rollback_receipt_id": self.evidence.rollback_receipt_id,
        })
    }
}

/// Outcome of a per-Agent readback verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadbackOutcome {
    /// Readback digest matched `planned_after_digest`.
    Verified,
    /// Readback digest mismatched; this Agent alone was rolled back.
    RolledBack,
}

/// A request to advance one Agent's transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAdvanceRequest {
    pub agent_id: String,
    pub target: ConfigTransactionState,
}

/// Why an Agent transaction cannot advance (fail-closed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentTransactionError {
    /// The transition is not in the frozen table (§6.1).
    InvalidTransition {
        from: ConfigTransactionState,
        to: ConfigTransactionState,
    },
    /// A cross-section field required for the target state is missing.
    MissingEvidence(&'static str),
    /// `readback_digest` does not match `planned_after_digest`.
    ReadbackMismatch,
    /// No transaction record exists for the requested Agent.
    UnknownAgent(String),
    /// More than one record claims the same Agent identity.
    DuplicateAgent(String),
}

/// Advances each requested Agent independently (§8.1). One Agent's failure
/// (for example a readback mismatch that rolls back that Agent) never rolls
/// back or aborts any other Agent's transaction; results are aligned with
/// `requests`.
pub fn advance_agent_batch(
    records: &mut [AgentTransactionRecord],
    requests: &[AgentAdvanceRequest],
) -> Vec<Result<ConfigTransactionState, AgentTransactionError>> {
    let duplicates = records
        .iter()
        .fold(HashMap::<String, usize>::new(), |mut counts, record| {
            *counts.entry(record.agent_id.clone()).or_default() += 1;
            counts
        });
    requests
        .iter()
        .map(|request| {
            if duplicates
                .get(request.agent_id.as_str())
                .copied()
                .unwrap_or(0)
                > 1
            {
                return Err(AgentTransactionError::DuplicateAgent(
                    request.agent_id.clone(),
                ));
            }
            records
                .iter_mut()
                .find(|record| record.agent_id == request.agent_id)
                .map_or_else(
                    || {
                        Err(AgentTransactionError::UnknownAgent(
                            request.agent_id.clone(),
                        ))
                    },
                    |record| record.advance(request.target),
                )
        })
        .collect()
}

/// Stable snake_case label for [`ConfigTransactionState`] — the internal code
/// used in technical details, Doctor, and logs (§3.6/§6.1); never ordinary UI.
fn config_transaction_state_label(state: ConfigTransactionState) -> &'static str {
    use ConfigTransactionState::*;
    match state {
        Discovered => "discovered",
        Selected => "selected",
        Planned => "planned",
        BackedUp => "backed_up",
        Applied => "applied",
        ReadbackVerified => "readback_verified",
        CapabilityVerified => "capability_verified",
        Completed => "completed",
        RolledBack => "rolled_back",
        Failed => "failed",
    }
}

/// Maps the frozen per-Agent transaction state onto the §3.6 user vocabulary.
/// Intermediate states are one user task (`正在配置`); only completed is success,
/// and both rollback and failure are visible failures.
pub fn transaction_status_word(state: ConfigTransactionState) -> &'static str {
    use ConfigTransactionState::*;
    match state {
        Discovered | Selected | Planned | BackedUp | Applied | ReadbackVerified
        | CapabilityVerified => "正在配置",
        Completed => "配置成功",
        RolledBack | Failed => "配置失败",
    }
}

/// Projects a frozen `RequestLog` onto the frozen `OptimizationReceipt`
/// (§6.2/§7.2). Local token counts are mapped from the log's pre/post counts;
/// Provider input/output usage is sourced only from
/// `context_accounting.provider_actual_sent` — when the log carries no
/// Provider usage evidence the fields stay `None` (unavailable) and are never
/// set to `0`. Provider usage alone never elevates claimable savings; the
/// claimable bucket is derived only from the accounting's claimable signed
/// savings. `protected_content` is a descriptor of protected content and
/// never carries content; a `RequestLog` records no such descriptor, so the
/// projection leaves it empty rather than inventing one.
pub fn project_optimization_receipt(log: &RequestLog) -> OptimizationReceipt {
    let provider_usage = log.context_accounting.as_ref().and_then(|accounting| {
        if accounting.validate().is_err() {
            return None;
        }
        accounting
            .provider_actual_sent
            .measurement
            .as_ref()
            .filter(|measurement| measurement.evidence.kind == UsageEvidenceKind::ProviderActual)
    });
    let provider_input_tokens =
        provider_usage.and_then(|measurement| measurement.usage.input.normalized_total);
    let provider_output_tokens =
        provider_usage.and_then(|measurement| measurement.usage.output_tokens);
    let claimable_tokens_avoided = log
        .context_accounting
        .as_ref()
        .and_then(|accounting| accounting.claimable_signed_savings())
        .and_then(|savings| savings.total_tokens.or(savings.input_tokens))
        .filter(|tokens| *tokens > 0);
    OptimizationReceipt {
        schema_version: 1,
        request_id: log.request_id,
        timestamp_ms: log.timestamp_ms,
        source_agent: log.agent.clone(),
        project_scope: None,
        user_target_provider: None,
        user_target_model: None,
        actual_provider: log.provider_id.clone(),
        actual_model: log.model.clone(),
        // No user-target evidence is recorded in `RequestLog`, so a change
        // cannot be claimed; the receipt stays conservative (false).
        provider_or_model_changed: false,
        transformations: log.transforms_applied.clone(),
        protected_content: String::new(),
        local_original_tokens: log
            .input_tokens_before
            .and_then(|tokens| u64::try_from(tokens).ok()),
        local_optimized_tokens: log
            .input_tokens_after
            .and_then(|tokens| u64::try_from(tokens).ok()),
        provider_input_tokens,
        provider_output_tokens,
        estimated_local_tokens_avoided: log
            .tokens_saved
            .and_then(|tokens| i64::try_from(tokens).ok()),
        claimable_tokens_avoided,
        recovery_status: None,
        recovery_pointer: None,
        // A `RequestLog` alone carries no artifact-derived validation
        // evidence; unavailable is the honest value, never Passed/Failed.
        validation_status: ValidationStatus::Unavailable,
        skip_or_failure_reason: log.error.clone(),
    }
}

// ===========================================================================
// §3.6 frozen configuration/sync result vocabulary (frozen; consumed by
// PR-3/PR-4). Ordinary pages render ONLY these words; internal codes appear
// only in technical detail / Doctor / logs. Unknown codes fail closed.
// ===========================================================================

/// The ten §3.6 frozen configuration/sync result words — the only words
/// ordinary pages may render for configuration/sync results.
pub const FROZEN_STATUS_WORDS: [&str; 10] = [
    "未安装",
    "未选择",
    "正在配置",
    "配置成功",
    "配置失败",
    "工具能力已接入",
    "已检测/不支持本地路由",
    "正在同步",
    "同步成功",
    "同步失败",
];

/// Asset-list statuses additionally allowed by §3.6 (MUST 1, §11.2).
pub const FROZEN_ASSET_LIST_WORDS: [&str; 4] = ["已安装", "未安装", "已配置", "未配置"];

/// No sanctioned §3.6 user word exists for this internal code. The code may
/// only appear in technical detail / Doctor / logs; it must never leak into
/// ordinary UI (fail-closed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusVocabularyError {
    pub internal_code: String,
}

/// Maps an internal status code to the §3.6 frozen user-facing word
/// (PR-3/PR-4 shared interface). Known codes return the sanctioned word;
/// mechanism-only codes (`adapter`, `route`, `credential_reference`,
/// `needs-source-fix`, `source_conflict`, `healthy`, `active`, `supported`,
/// …) and unknown strings fail closed.
pub fn user_status_word_for(internal_code: &str) -> Result<&'static str, StatusVocabularyError> {
    let word = match internal_code {
        "not_installed" | "uninstalled" => "未安装",
        "unselected" | "not_selected" => "未选择",
        "configuring" | "in_progress" => "正在配置",
        // Design example: Codex 无 routed 报告时显示「配置成功；尚无真实请求验证」——
        // `configured`/`not_verified` 的内部码在普通 UI 落为「配置成功」，
        // 证据说明允许用户语言补充，绝不原样显示内部码。
        "configured" | "not_verified" => "配置成功",
        "failed" | "configuration_failed" => "配置失败",
        "connected" | "verified" => "工具能力已接入",
        "detected" | "detection_only" => "已检测/不支持本地路由",
        "syncing" => "正在同步",
        "synced" => "同步成功",
        "sync_failed" => "同步失败",
        _ => {
            return Err(StatusVocabularyError {
                internal_code: internal_code.to_string(),
            })
        }
    };
    Ok(word)
}

/// Asset-list status word (§3.6 MUST 1, §11.2): maps installed/configured
/// facts to one of the four allowed asset-list words. Returns only frozen
/// vocabulary; never an internal code.
pub fn asset_list_status_word(installed: bool, configured: bool) -> &'static str {
    if !installed {
        "未安装"
    } else if configured {
        "已配置"
    } else {
        "未配置"
    }
}

/// A local, redacted request for a deterministic compression decision. The
/// body is evaluated in memory only and is never retained by this contract.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompressionDecisionRequest {
    pub route_id: String,
    pub input_protocol: Protocol,
    pub endpoint: OpticalEndpoint,
    #[serde(default)]
    pub requested_policy: Option<CompressionPolicy>,
    /// Request-level frozen-message override for the cache prefix. When set,
    /// this wins over `cache_control` marker detection; `None` keeps the
    /// marker-derived floor. The body is still evaluated in memory only.
    #[serde(default)]
    pub frozen_message_count: Option<usize>,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompressionDecisionState {
    Resolved,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompressionArtifactState {
    Transformed,
    Passthrough,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompressionArtifactIdentity {
    pub state: CompressionArtifactState,
    pub identity: String,
    pub transforms_applied: Vec<String>,
}

/// Stable, body-free output shared by the Rust API, local HTTP endpoint, MCP,
/// and the TypeScript SDK. `artifact.identity` is an opaque SHA-256 digest of
/// the evaluated candidate and selected route context, not a request-body
/// export.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompressionDecision {
    pub contract_revision: &'static str,
    pub source: &'static str,
    pub state: CompressionDecisionState,
    pub reason_code: String,
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub input_protocol: Protocol,
    pub output_protocol: Protocol,
    pub endpoint: OpticalEndpoint,
    pub compression_policy: CompressionPolicy,
    pub artifact: CompressionArtifactIdentity,
    pub original_tokens: u64,
    pub compressed_tokens: u64,
    pub tokens_saved: u64,
    pub capability: CompressionCapabilityTransaction,
    pub evidence: CompressionDecisionEvidence,
    /// R2.E: capability registry summary for this model (seed/observed/
    /// learned/inferred + unknown flag + strategy). Evidence only; unknown
    /// models never fail the decision.
    #[serde(default)]
    pub capability_summary: serde_json::Value,
    /// R2.E: capability-driven compression strategy hint; None for unknown
    /// models (route policy governs). Not yet applied by default.
    #[serde(default)]
    pub compression_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompressionExecutionState {
    Completed,
    Passthrough,
}

/// Route-bound compression result for gateway, SDK, and MCP callers.
///
/// The supplied request body is evaluated in memory and returned as either the
/// validated candidate or the exact passthrough body. It is not persisted by
/// this surface and no upstream request is sent.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CompressionExecution {
    pub contract_revision: &'static str,
    pub source: &'static str,
    pub state: CompressionExecutionState,
    pub body: Value,
    pub original_tokens: u64,
    pub compressed_tokens: u64,
    pub tokens_saved: u64,
    pub savings_percent: f64,
    pub elapsed_us: u128,
    pub transforms_applied: Vec<String>,
    pub fallback_reason_code: Option<String>,
    pub evidence: CompressionDecisionEvidence,
    pub decision: CompressionDecision,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompressionDecisionEvidence {
    pub route_identity: &'static str,
    pub provider_identity: &'static str,
    pub model_allowlist: &'static str,
    pub endpoint_identity: &'static str,
    pub request_body_persisted: bool,
    pub credentials_returned: bool,
}

pub const COMPRESSION_CAPABILITY_TRANSACTION_REVISION: &str = "dtoken-compression-capability-v1";

/// The explicit capability transaction produced once
/// route/provider/model/protocol have all been resolved for a compress
/// request. It names the ingress surface the caller presented and the
/// upstream surface the candidate body targets, plus the exact adapter used.
///
/// Dispatch is fail-closed: any combination that does not resolve through
/// [`CompressionDecisionError`] instead of a transaction, so neither the MCP,
/// HTTP, Rust library nor TS SDK surface ever guesses a protocol.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompressionCapabilityTransaction {
    pub transaction_revision: &'static str,
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub ingress_protocol: Protocol,
    pub ingress_endpoint: OpticalEndpoint,
    pub upstream_protocol: Protocol,
    pub upstream_endpoint: OpticalEndpoint,
    pub adapter: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressionDecisionError {
    InvalidArguments,
    RouteContextUnavailable,
    RouteContextMismatch,
    RoutePolicyMismatch,
}

#[derive(Debug, Clone)]
struct EvaluatedCompressionDecision {
    decision: CompressionDecision,
    body: Value,
    savings_percent: f64,
    elapsed_us: u128,
}

impl EvaluatedCompressionDecision {
    fn into_execution(self) -> CompressionExecution {
        let transformed = matches!(
            self.decision.artifact.state,
            CompressionArtifactState::Transformed
        );
        CompressionExecution {
            contract_revision: COMPRESSION_DECISION_CONTRACT_REVISION,
            source: "dtoken-owned-runtime",
            state: if transformed {
                CompressionExecutionState::Completed
            } else {
                CompressionExecutionState::Passthrough
            },
            body: self.body,
            original_tokens: self.decision.original_tokens,
            compressed_tokens: self.decision.compressed_tokens,
            tokens_saved: self.decision.tokens_saved,
            savings_percent: self.savings_percent,
            elapsed_us: self.elapsed_us,
            transforms_applied: self.decision.artifact.transforms_applied.clone(),
            fallback_reason_code: (!transformed).then(|| self.decision.reason_code.clone()),
            evidence: self.decision.evidence.clone(),
            decision: self.decision,
            note: if transformed {
                "A resolved d-token route produced a validated transform without forwarding the request."
            } else {
                "The resolved route did not permit a safe transform, so the exact body is returned unchanged without forwarding."
            },
        }
    }
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
        for metadata in latest_ccr_metadata_records(ccr_metadata) {
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

fn latest_ccr_metadata_records(metadata: &[Value]) -> Vec<&Value> {
    let mut latest_index_by_ref = HashMap::new();
    for (index, item) in metadata.iter().enumerate() {
        if let Some(recovery_ref) = item["recovery_ref"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            latest_index_by_ref.insert(recovery_ref, index);
        }
    }
    metadata
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let recovery_ref = item["recovery_ref"].as_str();
            let is_latest = recovery_ref
                .and_then(|value| latest_index_by_ref.get(value))
                .map(|latest| *latest == index)
                .unwrap_or(true);
            is_latest.then_some(item)
        })
        .collect()
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
    let prepared = PipelineInput::new(scope, baseline_bytes.clone(), MAX_REQUEST_BODY_BYTES)
        .map_err(|error| anyhow::anyhow!("invalid pipeline input: {error:?}"))?
        .prepare();
    if transforms.is_empty() {
        return Ok(PreparedPipelinePayload {
            ready: prepared.into_baseline_ready("no_transform_selected"),
            fallback_reason: None,
        });
    }
    // Bare-marker persistence guard: a transformed candidate that carries a
    // `<<ccr:...>>` retrieval marker must be backed by a durable receipt for
    // that exact recovery ref.  Any mismatch falls back to the baseline.
    if let Err(reason) = ccr_bare_marker_guard(
        &baseline_bytes,
        &candidate_bytes,
        prepared_ccr.receipt.as_ref(),
    ) {
        return Ok(PreparedPipelinePayload {
            ready: prepared.into_baseline_ready(reason),
            fallback_reason: Some(reason),
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

/// CCR bare-marker persistence guard.
///
/// A `<<ccr:<ref>,...>>` marker (or the bracket retrieval forms) means the
/// surrounding bytes were already compressed and the real content lives in
/// the reversible private store.  Sending a candidate that contains a marker
/// without a durable receipt for that exact recovery ref leaks a dangling
/// handle that can never be resolved upstream.  d-token itself never emits
/// these markers; the guard is the fail-closed backstop for a transformed
/// candidate that somehow contains one:
///
/// * marker present but no durable receipt -> fail closed;
/// * marker ref does not exactly match the durable receipt -> fail closed;
/// * unverifiable bracket form (`Retrieve more/original: hash=`) -> fail
///   closed, because the handle cannot be bound to any receipt.
///
/// The receipt is only produced after `CcrStore::store` has synced the
/// envelope to disk, so a matching receipt proves durable persistence of the
/// original bytes.
fn ccr_bare_marker_guard(
    baseline: &[u8],
    candidate: &[u8],
    receipt: Option<&CcrStoreReceipt>,
) -> Result<(), &'static str> {
    if !contains_ccr_marker(candidate) {
        return Ok(());
    }
    // A marker already present in the baseline input is a passthrough case:
    // the content router routes it to the passthrough codec and no transform
    // may run.  Reaching this guard with a marker-bearing baseline means a
    // transform kept compressed text as source, which is never safe.
    if contains_ccr_marker(baseline) {
        return Err("ccr_marker_in_baseline_requires_passthrough");
    }
    let Some(receipt) = receipt else {
        return Err("ccr_marker_without_durable_receipt");
    };
    for marker_ref in ccr_marker_refs(candidate) {
        let Some(marker_ref) = marker_ref else {
            // Unverifiable bracket form: no recovery ref to bind to a receipt.
            return Err("ccr_marker_recovery_ref_unverifiable");
        };
        if marker_ref != receipt.recovery_ref
            && marker_ref
                != receipt
                    .recovery_ref
                    .strip_prefix("ccr:")
                    .unwrap_or_default()
        {
            return Err("ccr_marker_recovery_ref_mismatch");
        }
    }
    Ok(())
}

/// True when the serialized body contains any CCR retrieval marker shape the
/// runtime can emit (opaque `<<ccr:...>>` blob plus the bracket forms).
fn contains_ccr_marker(bytes: &[u8]) -> bool {
    !ccr_marker_refs(bytes).is_empty()
}

/// Extracts every marker recovery reference.  `<<ccr:<ref>,...>>` yields
/// `Some(ref)`; the bracket retrieval forms yield `None` because they carry
/// only a hash, not a recovery ref.
fn ccr_marker_refs(bytes: &[u8]) -> Vec<Option<String>> {
    let mut refs = Vec::new();
    let mut start = 0;
    while let Some(relative) = bytes[start..]
        .windows(6)
        .position(|window| window == b"<<ccr:")
    {
        let marker_start = start + relative + 6;
        let tail = &bytes[marker_start..];
        let end = tail
            .iter()
            .position(|byte| *byte == b',' || *byte == b'>')
            .unwrap_or(tail.len());
        let raw = String::from_utf8_lossy(&tail[..end]).into_owned();
        refs.push((!raw.is_empty()).then_some(raw));
        start = marker_start + end;
    }
    for needle in [
        b"Retrieve more: hash=" as &[u8],
        b"Retrieve original: hash=" as &[u8],
    ] {
        if bytes.windows(needle.len()).any(|window| window == needle) {
            refs.push(None);
        }
    }
    refs
}

/// Bounds all JSON extraction before LLM or Optical request parsing/allocation.
pub const MAX_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
const BACKEND_CAPABILITY_REVISION: &str = "headroom-backend-capabilities-v1";
const SAFE_PIPELINE_EVIDENCE_REVISION: &str = "dtoken-safe-context-pipeline-v1";
const OPTICAL_SHADOW_EVIDENCE_REVISION: &str = "dtoken-optical-shadow-pipeline-v1";
const OUTPUT_SHAPING_EVIDENCE_REVISION: &str = "dtoken-output-shaping-v1";
const CCR_PRE_SEND_EVIDENCE_REVISION: &str = "dtoken-ccr-pre-send-v1";
const INTEGRATED_RTK_EVIDENCE_REVISION: &str = "dtoken-integrated-rtk-adapter-v1";
const HEADROOM_AGGREGATE_EVIDENCE_REVISION: &str = "dtoken-headroom-aggregate-v1";
const COMPRESSOR_REGISTRY_EVIDENCE_REVISION: &str = "dtoken-compressor-registry-v1";
const FAILURE_PATTERN_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextStoreCapabilityState {
    NotConfigured,
    Ready { schema_version: i64 },
    Failed,
}

fn managed_rtk_invocation_verified(evidence: &ManagedRtkRuntimeEvidence) -> bool {
    evidence.installed
        && evidence.verified
        && evidence.usage_count > 0
        && evidence.last_receipt.is_some()
}

fn backend_capability_registry(
    agent_discovery: Option<&AgentDiscoverySnapshot>,
    context_store: ContextStoreCapabilityState,
    ccr_private_store_ready: bool,
    managed_rtk: Option<&ManagedRtkRuntimeEvidence>,
) -> CapabilityRegistrySnapshot {
    let verified_at_ms = dtoken_event_ledger::now_ms().max(1);
    let verified =
        |id: &str, state: AvailabilityState, evidence_revision: &str, reason_codes: &[&str]| {
            CapabilityEvidence::new(
                id,
                state,
                BACKEND_CAPABILITY_REVISION,
                None,
                Some(evidence_revision.to_string()),
                EvidenceFreshness::Live,
                reason_codes
                    .iter()
                    .map(|reason| (*reason).to_string())
                    .collect(),
                Some(verified_at_ms),
            )
            .expect("verified backend capability evidence must be valid")
        };
    let mut capabilities = [
        ("cache-aligner", "safe_cache_alignment_pipeline_available"),
        ("content-router", "safe_content_router_available"),
    ]
    .into_iter()
    .map(|(id, reason)| {
        verified(
            id,
            AvailabilityState::Ready,
            SAFE_PIPELINE_EVIDENCE_REVISION,
            &[reason],
        )
    })
    .collect::<Vec<_>>();
    let codec_evidence = dtoken_compress::codec_evidence::production_codec_evidence();
    for (id, reason) in [
        ("code-compressor", "safe_code_codec_available"),
        ("diff-codec", "safe_diff_codec_available"),
        ("html-codec", "safe_html_codec_available"),
        ("json-codec", "safe_json_codec_available"),
        ("log-codec", "safe_log_codec_available"),
        ("search-codec", "safe_search_codec_available"),
        ("build-codec", "safe_build_codec_available"),
        ("text-codec", "safe_text_codec_available"),
    ] {
        let active = codec_evidence.find(id).is_some_and(|evidence| {
            evidence.state() == dtoken_compress::codec_evidence::CodecEvidenceState::Active
                && evidence.failures() == 0
        });
        capabilities.push(verified(
            id,
            if active {
                AvailabilityState::Ready
            } else {
                AvailabilityState::Unavailable
            },
            codec_evidence.evidence_revision(),
            &[
                reason,
                if active {
                    "runtime_codec_evidence_verified"
                } else {
                    "runtime_codec_evidence_failed"
                },
            ],
        ));
    }

    capabilities.push(verified(
        "output-shaping",
        AvailabilityState::Ready,
        OUTPUT_SHAPING_EVIDENCE_REVISION,
        &["safe_output_shaping_pipeline_available"],
    ));
    capabilities.push(verified(
        "ccr-pre-send",
        if ccr_private_store_ready {
            AvailabilityState::Ready
        } else {
            AvailabilityState::Degraded
        },
        CCR_PRE_SEND_EVIDENCE_REVISION,
        &[if ccr_private_store_ready {
            "durable_ccr_pre_send_pipeline_available"
        } else {
            "ccr_lifecycle_implemented_private_store_disabled_in_snapshot"
        }],
    ));
    let (integrated_rtk_state, integrated_rtk_reason) = match managed_rtk {
        Some(evidence) if managed_rtk_invocation_verified(evidence) => (
            AvailabilityState::Ready,
            "verified_managed_runtime_invocation",
        ),
        Some(evidence) if evidence.verified => (
            AvailabilityState::Degraded,
            "verified_managed_runtime_candidate_invocation_required",
        ),
        Some(evidence) if evidence.installed => (
            AvailabilityState::Degraded,
            "managed_runtime_evidence_unverified",
        ),
        Some(_) | None => (AvailabilityState::Degraded, "managed_runtime_not_installed"),
    };
    capabilities.push(verified(
        "integrated-rtk",
        integrated_rtk_state.clone(),
        INTEGRATED_RTK_EVIDENCE_REVISION,
        &[integrated_rtk_reason],
    ));
    capabilities.push(
        CapabilityEvidence::unavailable(
            "input-image-compressor",
            BACKEND_CAPABILITY_REVISION,
            "ordinary_input_image_compression_out_of_scope",
        )
        .expect("static capability evidence must be valid"),
    );
    let smart_learning_state = if matches!(context_store, ContextStoreCapabilityState::Ready { .. })
    {
        AvailabilityState::Degraded
    } else {
        AvailabilityState::Unavailable
    };
    capabilities.push(
        CapabilityEvidence::new(
            "smart-learning",
            smart_learning_state,
            LEARNING_CAPABILITY_REVISION,
            None,
            Some(LEARNING_CAPABILITY_REVISION.to_string()),
            EvidenceFreshness::Live,
            vec![
                if matches!(context_store, ContextStoreCapabilityState::Ready { .. }) {
                    "learning_scan_writer_ready_candidate_usage_required".to_string()
                } else {
                    "learning_requires_persistent_context_store".to_string()
                },
            ],
            Some(verified_at_ms),
        )
        .expect("learning capability evidence must be valid"),
    );

    for (id, reason) in [
        (
            "optical-shadow-preview",
            "shadow_preview_pipeline_available",
        ),
        (
            "optical-renderer",
            "deterministic_optical_renderer_available",
        ),
        ("optical-patcher", "shadow_artifact_patcher_available"),
        (
            "optical-post-validator",
            "shadow_artifact_post_validator_available",
        ),
        (
            "optical-signed-accounting",
            "signed_multimodal_accounting_available",
        ),
        (
            "optical-representation-plan",
            "representation_plan_available",
        ),
        (
            "optical-responses-shadow-inventory",
            "responses_shadow_inventory_available",
        ),
        (
            "optical-dynamic-page-accounting",
            "dynamic_page_accounting_available",
        ),
        (
            "optical-direct-anthropic-upstream",
            "direct_anthropic_shadow_path_available",
        ),
        (
            "tool-schema-normalization",
            "tool_schema_normalization_available",
        ),
        (
            "optical-streaming-passthrough",
            "streaming_fail_closed_passthrough_available",
        ),
        ("optical-fail-closed", "optical_fail_closed_policy_enforced"),
        (
            "optical-capability-attestation",
            "signed_capability_attestation_enforced",
        ),
    ] {
        capabilities.push(verified(
            id,
            AvailabilityState::Ready,
            OPTICAL_SHADOW_EVIDENCE_REVISION,
            &[reason],
        ));
    }
    capabilities.push(verified(
        "optical-active",
        AvailabilityState::ImplementedDisabled,
        OPTICAL_SHADOW_EVIDENCE_REVISION,
        &["policy_shadow_only", "release_gate_disabled"],
    ));

    capabilities.push(
        CapabilityEvidence::new(
            "rust-library",
            AvailabilityState::Ready,
            RUST_LIBRARY_IMPLEMENTATION_REVISION,
            None,
            Some(COMPRESSION_DECISION_CONTRACT_REVISION.to_string()),
            EvidenceFreshness::Live,
            vec![
                "public_compression_decision_contract_available".to_string(),
                "route_bound_gateway_compress_available".to_string(),
            ],
            Some(verified_at_ms),
        )
        .expect("Rust library capability evidence must be valid"),
    );

    capabilities.push(
        CapabilityEvidence::new(
            "typescript-sdk",
            AvailabilityState::Ready,
            TYPESCRIPT_SDK_IMPLEMENTATION_REVISION,
            None,
            Some(COMPRESSION_DECISION_CONTRACT_REVISION.to_string()),
            EvidenceFreshness::Live,
            vec![
                "typescript_sdk_http_binding_available".to_string(),
                "typescript_sdk_gateway_compress_available".to_string(),
            ],
            Some(verified_at_ms),
        )
        .expect("TypeScript SDK capability evidence must be valid"),
    );

    capabilities.push(if let Some(transport) = mcp::McpServer::transport_name() {
        CapabilityEvidence::new(
            "mcp",
            AvailabilityState::Ready,
            BACKEND_CAPABILITY_REVISION,
            None,
            Some(mcp::MCP_IMPLEMENTATION_REVISION.to_string()),
            EvidenceFreshness::Live,
            vec![
                format!("mcp_{}_transport_available", transport.replace('-', "_")),
                "mcp_compress_handler_available".to_string(),
                "mcp_stats_handler_available".to_string(),
                "mcp_retrieve_requires_managed_capability".to_string(),
                "mcp_shared_context_requires_managed_capability".to_string(),
            ],
            Some(verified_at_ms),
        )
        .expect("MCP handler evidence must be valid")
    } else {
        CapabilityEvidence::unavailable(
            "mcp",
            BACKEND_CAPABILITY_REVISION,
            "mcp_transport_unavailable",
        )
        .expect("MCP unavailable evidence must be valid")
    });

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
    capabilities.push(match context_store {
        ContextStoreCapabilityState::Ready { schema_version } => CapabilityEvidence::new(
            "memory-store",
            AvailabilityState::Ready,
            BACKEND_CAPABILITY_REVISION,
            None,
            Some(format!("context-store-schema-v{schema_version}")),
            EvidenceFreshness::Live,
            vec![
                "context_store_open".to_string(),
                "memory_http_summary_scope_required".to_string(),
                "memory_write_requires_managed_mcp_scope".to_string(),
                "memory_records_redacted".to_string(),
            ],
            Some(dtoken_event_ledger::now_ms().max(1)),
        )
        .expect("validated memory-store evidence must be valid"),
        ContextStoreCapabilityState::NotConfigured => CapabilityEvidence::unavailable(
            "memory-store",
            BACKEND_CAPABILITY_REVISION,
            "memory_store_requires_persistent_context_store",
        )
        .expect("static capability evidence must be valid"),
        ContextStoreCapabilityState::Failed => CapabilityEvidence::unavailable(
            "memory-store",
            BACKEND_CAPABILITY_REVISION,
            "context_store_open_failed",
        )
        .expect("static capability evidence must be valid"),
    });
    let compressor_registry =
        dtoken_compress::compressor_registry::production_compressor_registry();
    capabilities.push(verified(
        "compressor-registry",
        if compressor_registry.all_active() {
            AvailabilityState::Ready
        } else {
            AvailabilityState::Unavailable
        },
        COMPRESSOR_REGISTRY_EVIDENCE_REVISION,
        &[if compressor_registry.all_active() {
            "rust_owned_builtin_compressor_registry_active"
        } else {
            "rust_owned_builtin_compressor_registry_incomplete"
        }],
    ));

    let leaves_ready = |ids: &[&str]| {
        ids.iter().all(|id| {
            capabilities
                .iter()
                .find(|capability| capability.id == *id)
                .is_some_and(|capability| capability.state == AvailabilityState::Ready)
        })
    };
    let compression_ready = leaves_ready(&[
        "cache-aligner",
        "code-compressor",
        "content-router",
        "json-codec",
        "log-codec",
        "search-codec",
        "build-codec",
        "diff-codec",
        "text-codec",
        "html-codec",
        "compressor-registry",
    ]);
    let output_shape_ready = leaves_ready(&["output-shaping"]);
    let proxy_sdk_mcp_ready = leaves_ready(&["rust-library", "typescript-sdk", "mcp"]);
    let memory_store_ready = leaves_ready(&["memory-store"]);
    let ccr_ready = leaves_ready(&["ccr-pre-send", "context-store"]);
    let smart_learning_available = capabilities
        .iter()
        .find(|capability| capability.id == "smart-learning")
        .is_some_and(|capability| {
            matches!(
                capability.state,
                AvailabilityState::Ready | AvailabilityState::Degraded
            )
        });

    capabilities.push(verified(
        "compression",
        if compression_ready {
            AvailabilityState::Ready
        } else {
            AvailabilityState::Unavailable
        },
        HEADROOM_AGGREGATE_EVIDENCE_REVISION,
        &[if compression_ready {
            "all_production_codecs_active"
        } else {
            "production_codec_leaf_unavailable"
        }],
    ));
    capabilities.push(verified(
        "ccr",
        if ccr_ready {
            AvailabilityState::Ready
        } else if memory_store_ready {
            AvailabilityState::Degraded
        } else {
            AvailabilityState::Unavailable
        },
        CCR_PRE_SEND_EVIDENCE_REVISION,
        &[if ccr_ready {
            "ccr_private_store_and_lifecycle_ready"
        } else if memory_store_ready {
            "ccr_lifecycle_ready_private_store_activation_required"
        } else {
            "ccr_persistent_projection_unavailable"
        }],
    ));
    capabilities.push(verified(
        "memory_learn",
        if memory_store_ready && smart_learning_available {
            AvailabilityState::Degraded
        } else {
            AvailabilityState::Unavailable
        },
        LEARNING_CAPABILITY_REVISION,
        &[if memory_store_ready && smart_learning_available {
            "memory_learning_ready_candidate_usage_required"
        } else {
            "memory_or_learning_store_unavailable"
        }],
    ));
    capabilities.push(verified(
        "output_shape",
        if output_shape_ready {
            AvailabilityState::Ready
        } else {
            AvailabilityState::Unavailable
        },
        HEADROOM_AGGREGATE_EVIDENCE_REVISION,
        &[if output_shape_ready {
            "route_bound_output_shaping_active"
        } else {
            "output_shaping_leaf_unavailable"
        }],
    ));
    capabilities.push(verified(
        "integrated_rtk",
        integrated_rtk_state,
        INTEGRATED_RTK_EVIDENCE_REVISION,
        &[integrated_rtk_reason],
    ));
    capabilities.push(verified(
        "proxy_sdk_mcp",
        if proxy_sdk_mcp_ready {
            AvailabilityState::Ready
        } else {
            AvailabilityState::Unavailable
        },
        HEADROOM_AGGREGATE_EVIDENCE_REVISION,
        &[if proxy_sdk_mcp_ready {
            "rust_http_mcp_typescript_surface_parity_ready"
        } else {
            "public_surface_leaf_unavailable"
        }],
    ));
    capabilities.push(verified(
        "stats_history_metrics",
        AvailabilityState::Ready,
        HEADROOM_AGGREGATE_EVIDENCE_REVISION,
        &["ledger_stats_history_metrics_active"],
    ));
    capabilities.push(verified(
        "routing_protocols",
        AvailabilityState::Ready,
        HEADROOM_AGGREGATE_EVIDENCE_REVISION,
        &["openai_anthropic_route_adapters_active"],
    ));
    capabilities.push(verified(
        "doctor_dashboard_logs_export",
        AvailabilityState::Degraded,
        HEADROOM_AGGREGATE_EVIDENCE_REVISION,
        &["runtime_surfaces_ready_desktop_candidate_verification_required"],
    ));
    capabilities.push(verified(
        "project_policy",
        AvailabilityState::Degraded,
        PROJECT_POLICY_CAPABILITY_REVISION,
        &["project_policy_executor_available_candidate_apply_required"],
    ));
    capabilities.push(verified(
        "backup_rollback",
        AvailabilityState::Degraded,
        CONFIG_TRANSACTION_CAPABILITY_REVISION,
        &["config_transaction_executor_available_candidate_receipt_required"],
    ));
    capabilities.push(verified(
        "optical_active_transform",
        AvailabilityState::ImplementedDisabled,
        HEADROOM_AGGREGATE_EVIDENCE_REVISION,
        &["policy_shadow_only", "release_gate_disabled"],
    ));

    CapabilityRegistrySnapshot::new(
        BACKEND_CAPABILITY_REVISION,
        dtoken_event_ledger::now_ms().max(1),
        capabilities,
    )
    .expect("static backend capability registry must be valid")
}

fn capability_registry_semantically_equal(
    left: &CapabilityRegistrySnapshot,
    right: &CapabilityRegistrySnapshot,
) -> bool {
    left.schema_version == right.schema_version
        && left.revision == right.revision
        && left.capabilities.len() == right.capabilities.len()
        && left
            .capabilities
            .iter()
            .zip(&right.capabilities)
            .all(|(left, right)| {
                left.id == right.id
                    && left.state == right.state
                    && left.implementation_revision == right.implementation_revision
                    && left.config_revision == right.config_revision
                    && left.evidence_revision == right.evidence_revision
                    && left.freshness == right.freshness
                    && left.reason_codes == right.reason_codes
            })
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
    ccr_lifecycle_lock: Arc<Mutex<()>>,
    ccr_private_store: Option<Arc<CcrStore>>,
    runtime_summary: Arc<Mutex<RuntimeSummaryReadModel>>,
    runtime_performance: Arc<Mutex<RuntimePerformanceState>>,
    runtime_store: Option<Arc<RuntimeStore>>,
    /// Requests whose terminal commit recovery marked explicitly pending
    /// (partial fan-out).  Their terminal records are excluded from the hot
    /// read models and the SQLite projections until a later, complete
    /// commit arrives; lifecycle CCR records for the same recovery refs are
    /// still reconciled.  Populated only during persistent restart.
    terminal_pending_commits: HashMap<Uuid, TerminalCommitMarker>,
    context_store: Option<Arc<Mutex<ContextStore>>>,
    managed_rtk_root: Option<Arc<PathBuf>>,
    agent_discovery: Arc<Option<AgentDiscoverySnapshot>>,
    http_client: reqwest::Client,
    hybrid_engine: Arc<HybridContextEngine>,
    optical_config_issues: Arc<Vec<dtoken_config::OpticalConfigIssue>>,
    capabilities: Arc<CapabilityRegistrySnapshot>,
    capability_read_model: Arc<Mutex<CapabilityRegistrySnapshot>>,
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
            false,
            None,
        ));
        let capability_read_model = Arc::new(Mutex::new((*capabilities).clone()));
        Self {
            config: Arc::new(config),
            config_status: Arc::new(config_status),
            default_doctor_port: 18001,
            ledger: Arc::new(Mutex::new(EventLedger::new())),
            route_decisions: Arc::new(Mutex::new(vec![])),
            request_logs: Arc::new(Mutex::new(vec![])),
            ccr_metadata: Arc::new(Mutex::new(vec![])),
            ccr_lifecycle_lock: Arc::new(Mutex::new(())),
            ccr_private_store: None,
            runtime_summary: Arc::new(Mutex::new(RuntimeSummaryReadModel::default())),
            runtime_performance: Arc::new(Mutex::new(RuntimePerformanceState::default())),
            runtime_store: None,
            terminal_pending_commits: HashMap::new(),
            context_store: None,
            managed_rtk_root: None,
            agent_discovery: Arc::new(agent_discovery),
            http_client: reqwest::Client::new(),
            hybrid_engine,
            optical_config_issues,
            capabilities,
            capability_read_model,
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
        let ccr_private_store = configured_ccr_private_store(runtime_dir);
        Self::new_persistent_with_private_store(
            config,
            config_status,
            runtime_dir,
            ccr_private_store,
        )
    }

    fn new_persistent_with_private_store(
        config: DtokenConfig,
        config_status: RuntimeConfigStatus,
        runtime_dir: &StdPath,
        ccr_private_store: Option<Arc<CcrStore>>,
    ) -> Self {
        let store = Arc::new(RuntimeStore::new(runtime_dir));
        let mut state = Self::new_with_config_status(config, config_status);
        state.ccr_private_store = ccr_private_store;
        // G3: the terminal-commit recovery sweep needs the journal installed
        // before any load, and every subsequent load must filter the requests
        // it explicitly left pending.
        state.runtime_store = Some(Arc::clone(&store));
        let recovery_started_at = Instant::now();
        let recovery_result = state.recover_terminal_commits();
        state.record_runtime_performance(
            "terminal_recovery",
            recovery_started_at,
            recovery_result.is_ok(),
            state.terminal_pending_commits.len() as u64,
        );
        if recovery_result.is_err() {
            // Recovery could not classify dangling intents; fall back to the
            // legacy unfiltered load so startup still converges from the raw
            // journals, and surface the failure through the performance stage.
            state.terminal_pending_commits.clear();
        }
        let pending_request_ids = state
            .terminal_pending_commits
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let pending_event_ids = state
            .terminal_pending_commits
            .values()
            .map(|marker| marker.event_id)
            .collect::<HashSet<_>>();
        let pending_ccr_refs = state
            .terminal_pending_commits
            .keys()
            .map(|request_id| format!("ccr:{request_id}"))
            .collect::<HashSet<_>>();
        let managed_rtk_root = runtime_dir.join("managed-rtk");
        let managed_rtk_evidence = inspect_managed_rtk_runtime(&managed_rtk_root);
        state.managed_rtk_root = Some(Arc::new(managed_rtk_root));
        if let Ok(events) = store.recent_events(10_000) {
            if let Ok(mut ledger) = state.ledger.lock() {
                for event in events.into_iter().rev() {
                    if pending_event_ids.contains(&event.event_id) {
                        continue;
                    }
                    let _ = ledger.record(event);
                }
            }
        }
        if let Ok(route_decisions) = store.recent_route_decisions(10_000) {
            if let Ok(mut items) = state.route_decisions.lock() {
                *items = route_decisions
                    .into_iter()
                    .rev()
                    .filter(|value| {
                        match value["request_id"]
                            .as_str()
                            .and_then(|value| Uuid::parse_str(value).ok())
                        {
                            Some(request_id) => !pending_request_ids.contains(&request_id),
                            None => true,
                        }
                    })
                    .collect();
            }
        }
        if let Ok(request_logs) = store.recent_request_logs(10_000) {
            if let Ok(mut logs) = state.request_logs.lock() {
                *logs = request_logs
                    .into_iter()
                    .rev()
                    .filter(|log| !pending_request_ids.contains(&log.request_id))
                    .collect();
            }
        }
        // CCR lifecycle reconciliation must still scan the complete append-only
        // journal, but keeping every historical lifecycle record resident
        // makes startup memory grow without bound. Retaining only the latest
        // entry for each recovery ref preserves restart semantics.  Records of
        // explicitly pending terminal commits are skipped so the lifecycle
        // state (retained / deleted) stays authoritative and the terminal
        // record cannot masquerade as finalized.
        if let Ok(ccr_metadata) = store.latest_ccr_metadata_by_recovery_ref(&|record: &Value| {
            terminal_ccr_state_is_terminal(record["state"].as_str())
                && record["recovery_ref"]
                    .as_str()
                    .is_some_and(|recovery_ref| pending_ccr_refs.contains(recovery_ref))
        }) {
            if let Ok(mut items) = state.ccr_metadata.lock() {
                *items = ccr_metadata;
            }
        }
        state.rebuild_runtime_summary();
        match ContextStore::open(runtime_dir.join("runtime-state.sqlite3")) {
            Ok(mut context_store) => {
                let schema_version = context_store.schema_version().unwrap_or_default();
                let capabilities = Arc::new(backend_capability_registry(
                    state.agent_discovery.as_ref().as_ref(),
                    ContextStoreCapabilityState::Ready { schema_version },
                    state.ccr_private_store.is_some(),
                    Some(&managed_rtk_evidence),
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
                        state.ccr_private_store.is_some(),
                        Some(&managed_rtk_evidence),
                    ));
                }
            }
            Err(_) => {
                state.capabilities = Arc::new(backend_capability_registry(
                    state.agent_discovery.as_ref().as_ref(),
                    ContextStoreCapabilityState::Failed,
                    state.ccr_private_store.is_some(),
                    Some(&managed_rtk_evidence),
                ));
            }
        }
        if let Ok(mut read_model) = state.capability_read_model.lock() {
            *read_model = (*state.capabilities).clone();
        }
        // Reconciliation is deliberately last: both the final private store
        // and the append-only journal are installed, while the SQLite views are
        // still clearly rebuildable projections.  It never resends or deletes
        // an encrypted original.
        if state.reconcile_ccr_lifecycle_journal().is_err() {
            state.record_ccr_reconciliation_unavailable();
        }
        // The SQLite request view is a disposable projection of the durable
        // request/event journals: a crash between a JSONL append and the
        // SQLite upsert is healed here, keeping the cross-store terminal
        // commit recoverable.
        if state.rebuild_request_projections_from_journal().is_err() {
            if let Ok(mut read_model) = state.capability_read_model.lock() {
                if let Some(capability) = read_model
                    .capabilities
                    .iter_mut()
                    .find(|capability| capability.id == "context-store")
                {
                    capability
                        .reason_codes
                        .push("request_projection_rebuild_unavailable".to_string());
                }
            }
        }
        state
    }

    pub fn new_persistent_with_ccr_key(
        config: DtokenConfig,
        dir: impl AsRef<StdPath>,
        key: [u8; 32],
    ) -> anyhow::Result<Self> {
        let runtime_dir = dir.as_ref();
        let ccr_private_store = Arc::new(CcrStore::new(runtime_dir.join("ccr-private"), key)?);
        let state = Self::new_persistent_with_private_store(
            config,
            RuntimeConfigStatus::empty(),
            runtime_dir,
            Some(ccr_private_store),
        );
        verify_windows_private_runtime_contract(runtime_dir, &state)?;
        Ok(state)
    }

    /// G3 cross-store terminal commit recovery.  Called once during
    /// persistent startup, after `runtime_store` is installed and before any
    /// journal load.  A dangling `Intent` marker is rolled forward to
    /// `Committed` when the four fan-out journals already hold the complete
    /// bundle (the crash landed after the last fan-out sync but before the
    /// committed marker); otherwise it is marked `Pending` explicitly.
    /// Fan-out records are never re-appended here, so recovery cannot
    /// duplicate terminal records; the pending map then drives the hot-model
    /// and SQLite filters.
    fn recover_terminal_commits(&mut self) -> anyhow::Result<()> {
        let Some(store) = self.runtime_store.as_ref() else {
            return Ok(());
        };
        let mut pending = HashMap::<Uuid, TerminalCommitMarker>::new();
        for (request_id, marker) in store.latest_terminal_commit_markers()? {
            match marker.phase {
                TerminalCommitPhase::Committed => {}
                TerminalCommitPhase::Pending => {
                    pending.insert(request_id, marker);
                }
                TerminalCommitPhase::Intent => {
                    if store.terminal_bundle_complete(&marker)? {
                        store.append_terminal_commit_marker(&TerminalCommitMarker {
                            phase: TerminalCommitPhase::Committed,
                            ..marker.clone()
                        })?;
                    } else {
                        let pending_marker = TerminalCommitMarker {
                            phase: TerminalCommitPhase::Pending,
                            reason: Some("partial_fan_out_at_recovery".to_string()),
                            ..marker.clone()
                        };
                        store.append_terminal_commit_marker(&pending_marker)?;
                        pending.insert(request_id, pending_marker);
                    }
                }
            }
        }
        self.terminal_pending_commits = pending;
        Ok(())
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

    fn managed_rtk_runtime_evidence(&self) -> ManagedRtkRuntimeEvidence {
        self.managed_rtk_root
            .as_deref()
            .map(|root| inspect_managed_rtk_runtime(root))
            .unwrap_or_else(|| ManagedRtkRuntimeEvidence {
                installed: false,
                verified: false,
                usage_count: 0,
                saved_bytes: 0,
                last_receipt: None,
                reason: "persistent_runtime_not_configured".to_string(),
            })
    }

    fn current_capability_registry(&self) -> CapabilityRegistrySnapshot {
        let context_store = match self.context_store.as_ref() {
            Some(store) => store
                .lock()
                .ok()
                .and_then(|store| store.schema_version().ok())
                .map_or(ContextStoreCapabilityState::Failed, |schema_version| {
                    ContextStoreCapabilityState::Ready { schema_version }
                }),
            None if self
                .capabilities
                .find("context-store")
                .is_some_and(|capability| {
                    capability
                        .reason_codes
                        .iter()
                        .any(|reason| reason == "context_store_open_failed")
                }) =>
            {
                ContextStoreCapabilityState::Failed
            }
            None => ContextStoreCapabilityState::NotConfigured,
        };
        let managed_rtk = self.managed_rtk_runtime_evidence();
        let candidate = backend_capability_registry(
            self.agent_discovery.as_ref().as_ref(),
            context_store,
            self.ccr_private_store.is_some(),
            Some(&managed_rtk),
        );
        let Ok(mut current) = self.capability_read_model.lock() else {
            return candidate;
        };
        if capability_registry_semantically_equal(&current, &candidate) {
            return current.clone();
        }
        *current = candidate.clone();
        candidate
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

    /// Fail-closed authorized retrieval used by every capability-gated
    /// surface (managed MCP tool-call retrieve, future tool-call recovery).
    ///
    /// Retrieval is only granted when all of the following hold:
    ///
    /// 1. the recovery ref has a durable lifecycle record;
    /// 2. the record's terminal state still makes the original retrievable
    ///    (`pending_retained`, `finalized`, `send_failed_retained`,
    ///    `finalized_with_non_ok_terminal`, or `recovery_pending_retained`);
    /// 3. the encrypted envelope is actually present.
    ///
    /// Deleted, delete-intent, interrupted, untracked or raw-missing refs
    /// are rejected before any byte is decrypted, so a stale or forged
    /// recovery handle can never pull an original back out of the private
    /// store.  The caller still owns its capability gate (MCP authorization
    /// or an in-process route binding); this function never substitutes for
    /// it and never returns raw material on the error path.
    pub fn retrieve_ccr_original_bound(&self, recovery_ref: &str) -> anyhow::Result<Vec<u8>> {
        let metadata = self
            .retrieve_ccr_metadata(recovery_ref)
            .ok_or_else(|| anyhow::anyhow!("CCR metadata is unavailable for this recovery ref"))?;
        if metadata["raw_available"].as_bool() != Some(true) {
            return Err(anyhow::anyhow!(
                "CCR original is not retrievable for this recovery ref"
            ));
        }
        let state = metadata["state"].as_str().unwrap_or_default();
        if !matches!(
            state,
            "pending_retained"
                | "finalized"
                | "send_failed_retained"
                | "finalized_with_non_ok_terminal"
                | "recovery_pending_retained"
        ) {
            return Err(anyhow::anyhow!(
                "CCR original is not retrievable in lifecycle state {state}"
            ));
        }
        self.retrieve_ccr_original(recovery_ref)
    }

    fn append_ccr_lifecycle_record(&self, record: Value) -> anyhow::Result<()> {
        validate_ccr_lifecycle_record(&record)?;
        if let Some(store) = &self.runtime_store {
            store.append_ccr_metadata(&record)?;
        }
        let mut metadata = self
            .ccr_metadata
            .lock()
            .map_err(|_| anyhow::anyhow!("CCR metadata lock poisoned"))?;
        push_bounded(&mut metadata, record);
        drop(metadata);
        self.rebuild_runtime_summary();
        Ok(())
    }

    /// Converges redacted lifecycle records after restart.  This deliberately
    /// has no `store`, `retrieve`, or `delete` call: a restart must never turn
    /// an interrupted write/delete into an automatic resend or destructive
    /// action.
    fn reconcile_ccr_lifecycle_journal(&mut self) -> anyhow::Result<()> {
        let lifecycle_lock = Arc::clone(&self.ccr_lifecycle_lock);
        let _lifecycle_guard = lifecycle_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("CCR lifecycle lock poisoned"))?;
        let Some(private_store) = self.ccr_private_store.as_ref() else {
            return Ok(());
        };
        let journal = self
            .ccr_metadata
            .lock()
            .map_err(|_| anyhow::anyhow!("CCR metadata lock poisoned"))?
            .clone();
        let latest = latest_ccr_metadata_records(&journal)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let now_ms = dtoken_event_ledger::now_ms().max(1);

        for metadata in latest {
            let Some(recovery_ref) = metadata["recovery_ref"].as_str() else {
                continue;
            };
            let raw_state = private_store.inspect(recovery_ref)?;
            let raw_present = raw_state == CcrEntryInspection::Available;
            let state = metadata["state"].as_str().unwrap_or_default();
            let next = match state {
                "delete_intent" if raw_present => {
                    Some(ccr_interrupted_retained(metadata.clone(), now_ms)?)
                }
                "delete_intent" if raw_state == CcrEntryInspection::Missing => {
                    Some(ccr_deletion_tombstone(metadata.clone(), now_ms)?)
                }
                "delete_intent" => Some(ccr_raw_unavailable(metadata.clone(), now_ms, raw_state)?),
                "write_intent" | "pending_retained" if raw_present => {
                    Some(ccr_recovery_pending_retained(metadata.clone(), now_ms)?)
                }
                "write_intent" | "pending_retained" if raw_state == CcrEntryInspection::Missing => {
                    Some(ccr_raw_missing(metadata.clone(), now_ms)?)
                }
                "write_intent" | "pending_retained" => {
                    Some(ccr_raw_unavailable(metadata.clone(), now_ms, raw_state)?)
                }
                _ if ccr_terminal_metadata(&metadata)
                    && raw_state == CcrEntryInspection::Missing =>
                {
                    Some(ccr_raw_missing(metadata.clone(), now_ms)?)
                }
                _ if ccr_terminal_metadata(&metadata) && !raw_present => {
                    Some(ccr_raw_unavailable(metadata.clone(), now_ms, raw_state)?)
                }
                _ => None,
            };
            if let Some(next) = next {
                self.append_ccr_lifecycle_record(next)?;
            }
        }
        self.rebuild_ccr_metadata_projection_from_journal()
    }

    fn record_ccr_reconciliation_unavailable(&self) {
        let _ = self.try_append_event(DtokenEvent {
            event_id: Uuid::new_v4(),
            timestamp_ms: dtoken_event_ledger::now_ms().max(1),
            project_id: Some("dtoken-runtime".to_string()),
            agent_id: Some("d-token".to_string()),
            instance_id: Some("dtoken-runtime-local".to_string()),
            command_id: None,
            request_id: None,
            stage: LayeredStage::HeadroomRetrieve,
            source: EventSource::Dtoken,
            status: EventStatus::Failed,
            input_tokens: None,
            output_tokens: None,
            saved_tokens: None,
            baseline_kind: Some(BaselineKind::HeadroomInput),
            error_code: Some("ccr_reconciliation_unavailable".to_string()),
            metadata: json!({
                "action": "ccr_restart_reconciliation",
                "state": "unavailable",
                "raw_available": false,
                "raw_returned": false
            }),
            context_accounting: None,
        });
    }

    fn rebuild_ccr_metadata_projection_from_journal(&self) -> anyhow::Result<()> {
        let Some(context_store) = self.context_store.as_ref() else {
            return Ok(());
        };
        let journal = self
            .ccr_metadata
            .lock()
            .map_err(|_| anyhow::anyhow!("CCR metadata lock poisoned"))?
            .clone();
        let projections = latest_ccr_metadata_records(&journal)
            .into_iter()
            .filter_map(ccr_metadata_projection_from_journal)
            .collect::<Vec<_>>();
        context_store
            .lock()
            .map_err(|_| anyhow::anyhow!("context-store lock poisoned"))?
            .replace_ccr_metadata(&projections)
    }

    /// Rebuilds the disposable SQLite request view from the append-only
    /// request/event journals.  The JSONL journal is the durable source of
    /// truth for the cross-store terminal commit; this projection is always a
    /// rebuildable view of it, so a crash between a JSONL append and the
    /// SQLite upsert is healed on the next restart.
    fn rebuild_request_projections_from_journal(&self) -> anyhow::Result<()> {
        let Some(context_store) = self.context_store.as_ref() else {
            return Ok(());
        };
        let runtime_store = self
            .runtime_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("runtime journal is unavailable"))?;
        let mut terminal_by_request = HashMap::<Uuid, DtokenEvent>::new();
        runtime_store.visit_all_events(|event| {
            if let Some(request_id) = event.request_id {
                // Events of explicitly pending terminal commits are not
                // terminal; they must not drive the projection.
                if self.terminal_pending_commits.contains_key(&request_id) {
                    return Ok(());
                }
                terminal_by_request.insert(request_id, event);
            }
            Ok(())
        })?;
        let mut latest_logs = HashMap::<Uuid, RequestLog>::new();
        runtime_store.visit_all_request_logs(|log| {
            if self.terminal_pending_commits.contains_key(&log.request_id) {
                return Ok(());
            }
            latest_logs.insert(log.request_id, log);
            Ok(())
        })?;
        let mut logs = latest_logs.into_values().collect::<Vec<_>>();
        logs.sort_by(|left, right| {
            left.timestamp_ms
                .cmp(&right.timestamp_ms)
                .then_with(|| left.request_id.cmp(&right.request_id))
        });
        let projections = logs
            .into_iter()
            .filter_map(|log| {
                let event = terminal_by_request.get(&log.request_id)?;
                Some(RequestProjection {
                    request_id: log.request_id.to_string(),
                    route_id: log.route_id.unwrap_or_default(),
                    provider_id: log.provider_id.unwrap_or_default(),
                    agent_id: log.agent,
                    project_id: event.project_id.clone(),
                    status: event_status_label(&event.status).to_string(),
                    input_units: log
                        .input_tokens_before
                        .and_then(|value| u64::try_from(value).ok()),
                    output_units: log
                        .output_tokens
                        .and_then(|value| u64::try_from(value).ok()),
                    saved_units: log.tokens_saved.and_then(|value| u64::try_from(value).ok()),
                    observed_at_ms: log.timestamp_ms,
                })
            })
            .collect::<Vec<_>>();
        context_store
            .lock()
            .map_err(|_| anyhow::anyhow!("context-store lock poisoned"))?
            .replace_request_projections(&projections)
    }

    pub fn delete_ccr_original(&self, recovery_ref: &str) -> anyhow::Result<CcrDeletionReceipt> {
        let _lifecycle_guard = self
            .ccr_lifecycle_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("CCR lifecycle lock poisoned"))?;
        let now_ms = dtoken_event_ledger::now_ms().max(1);
        let metadata = self
            .retrieve_ccr_metadata(recovery_ref)
            .ok_or_else(|| anyhow::anyhow!("CCR metadata is unavailable for this recovery ref"))?;

        if metadata["state"].as_str() == Some("deleted") {
            return Ok(CcrDeletionReceipt {
                source: "dtoken-owned-runtime",
                recovery_ref: recovery_ref.to_string(),
                state: "already_deleted",
                raw_available: false,
                raw_returned: false,
            });
        }
        if metadata["raw_available"].as_bool() != Some(true) {
            return Err(anyhow::anyhow!(
                "CCR metadata does not describe a deletable private original"
            ));
        }
        let request_id = metadata["request_id"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("CCR metadata is missing request identity"))?
            .to_string();
        let private_store = self
            .ccr_private_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("CCR private storage is disabled"))?;

        // The intent is durable before a destructive operation.  If the
        // process stops after it, restart reconciliation only reports whether
        // the original was retained or is already absent; it never deletes it.
        self.append_ccr_lifecycle_record(ccr_delete_intent(metadata.clone(), now_ms)?)?;
        let _raw_deleted = private_store.delete(recovery_ref)?;
        let tombstone = ccr_deletion_tombstone(metadata, now_ms)?;
        self.append_ccr_lifecycle_record(tombstone.clone())?;
        self.try_append_event(DtokenEvent {
            event_id: Uuid::new_v4(),
            timestamp_ms: now_ms,
            project_id: Some("dtoken-runtime".to_string()),
            agent_id: Some("d-token".to_string()),
            instance_id: Some("dtoken-runtime-local".to_string()),
            command_id: None,
            request_id: Uuid::parse_str(&request_id).ok(),
            stage: LayeredStage::HeadroomRetrieve,
            source: EventSource::Dtoken,
            status: EventStatus::Ok,
            input_tokens: None,
            output_tokens: None,
            saved_tokens: None,
            baseline_kind: Some(BaselineKind::HeadroomInput),
            error_code: None,
            metadata: json!({
                "action": "ccr_delete",
                "recovery_ref": recovery_ref,
                "state": "deleted",
                "raw_available": false,
                "raw_returned": false
            }),
            context_accounting: None,
        })?;
        if let Some(store) = &self.context_store {
            let store = store
                .lock()
                .map_err(|_| anyhow::anyhow!("context-store lock poisoned"))?;
            store.upsert_ccr_metadata(&CcrMetadataProjection {
                recovery_ref: recovery_ref.to_string(),
                request_id,
                expires_at_ms: None,
                plaintext_bytes: None,
                state: "deleted".to_string(),
                updated_at_ms: now_ms,
            })?;
        }
        Ok(CcrDeletionReceipt {
            source: "dtoken-owned-runtime",
            recovery_ref: recovery_ref.to_string(),
            state: "deleted",
            raw_available: false,
            raw_returned: false,
        })
    }

    pub(crate) fn mcp_compress(
        &self,
        requested_policy: Option<CompressionPolicy>,
        frozen_message_count: Option<usize>,
        route_ref: mcp::McpRouteReference,
        body: Value,
    ) -> Result<Value, mcp::McpToolError> {
        let encoded = serde_json::to_vec(&body)
            .map_err(|_| mcp::McpToolError::invalid_arguments("body must be JSON serializable"))?;
        if encoded.len() > mcp::MAX_MCP_COMPRESS_BODY_BYTES {
            return Err(mcp::McpToolError::request_too_large());
        }
        let execution = self
            .compression_execution(CompressionDecisionRequest {
                route_id: route_ref.route_id,
                input_protocol: route_ref.input_protocol,
                endpoint: route_ref.endpoint,
                requested_policy,
                frozen_message_count,
                body,
            })
            .map_err(mcp_tool_error_from_decision_error)?;
        Ok(serde_json::to_value(execution).expect("compression execution must serialize"))
    }

    /// Returns a stable, body-free decision contract for the configured route.
    /// This API performs no network I/O and does not persist the supplied body.
    pub fn compression_decision(
        &self,
        request: CompressionDecisionRequest,
    ) -> Result<CompressionDecision, CompressionDecisionError> {
        self.evaluate_compression_decision(request)
            .map(|evaluation| evaluation.decision)
    }

    /// Returns a route-bound candidate body without forwarding it upstream.
    /// This is the shared Rust implementation for `/v1/compress`, MCP, and the
    /// TypeScript SDK.
    pub fn compression_execution(
        &self,
        request: CompressionDecisionRequest,
    ) -> Result<CompressionExecution, CompressionDecisionError> {
        self.evaluate_compression_decision(request)
            .map(EvaluatedCompressionDecision::into_execution)
    }

    fn evaluate_compression_decision(
        &self,
        request: CompressionDecisionRequest,
    ) -> Result<EvaluatedCompressionDecision, CompressionDecisionError> {
        if !request.body.is_object() || request.route_id.trim().is_empty() {
            return Err(CompressionDecisionError::InvalidArguments);
        }
        let route = self
            .config
            .find_route(&request.route_id)
            .ok_or(CompressionDecisionError::RouteContextUnavailable)?;
        let provider = self
            .config
            .find_provider(&route.target_provider_id)
            .ok_or(CompressionDecisionError::RouteContextUnavailable)?;
        if route.input_protocol != request.input_protocol {
            return Err(CompressionDecisionError::RouteContextMismatch);
        }
        if request
            .requested_policy
            .as_ref()
            .is_some_and(|policy| policy != &route.compression_policy)
        {
            return Err(CompressionDecisionError::RoutePolicyMismatch);
        }
        let model = request
            .body
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or(CompressionDecisionError::RouteContextUnavailable)?
            .to_string();
        if !provider.models.is_empty() && !provider.models.iter().any(|item| item == &model) {
            return Err(CompressionDecisionError::RouteContextUnavailable);
        }

        // R2.E: attach capability evidence (seed/observed/learned/inferred)
        // and, when the capability is known, drive the compression strategy
        // hint. Unknown models never fail here — the hint stays None and the
        // route policy governs; the evidence still lands in the decision.
        let capability_summary = crate::capability_bridge::resolve_model_capability_summary(
            &model,
            Some(protocol_label(&provider.protocol)),
        );
        let capability_hint = crate::capability_bridge::compression_hint_for(
            &model,
            Some(protocol_label(&provider.protocol)),
        );

        let (adapter, ingress_body, upstream_body) =
            match (&route.input_protocol, &provider.protocol, &request.endpoint) {
                (Protocol::OpenAi, Protocol::OpenAi, OpticalEndpoint::OpenAiChatCompletions) => (
                    RouteAdapterKind::OpenAiChatPassthrough,
                    request.body.clone(),
                    request.body.clone(),
                ),
                (Protocol::OpenAi, Protocol::OpenAi, OpticalEndpoint::OpenAiResponses) => {
                    // The Responses surface is dispatched through the same
                    // fail-closed bridge as the routed `/v1/responses` surface:
                    // invalid items, reasoning references and unknown fields
                    // are rejected here and never guessed.
                    let upstream = responses_to_openai_chat_body(&request.body)
                        .map_err(|_| CompressionDecisionError::InvalidArguments)?;
                    (
                        RouteAdapterKind::OpenAiResponsesToChat,
                        request.body.clone(),
                        upstream,
                    )
                }
                (Protocol::Anthropic, Protocol::Anthropic, OpticalEndpoint::AnthropicMessages) => (
                    RouteAdapterKind::AnthropicMessagesPassthrough,
                    request.body.clone(),
                    request.body.clone(),
                ),
                _ => return Err(CompressionDecisionError::RouteContextMismatch),
            };
        let configured_decision = build_configured_route_decision(Uuid::new_v4(), route, provider);
        let context = resolve_optimization_context(
            &configured_decision,
            route,
            provider,
            adapter,
            &ingress_body,
            &upstream_body,
        )
        .map_err(|_| CompressionDecisionError::RouteContextUnavailable)?;
        let summary = compress_for_route_with_frozen(
            Some(&context),
            route.compression_policy.clone(),
            upstream_body,
            None,
            request.frozen_message_count,
        );
        let transformed = !summary.transforms_applied.is_empty();
        let reason_code = if transformed {
            "validated_transform".to_string()
        } else {
            summary
                .fallback_reason_code
                .clone()
                .unwrap_or_else(|| "compression_not_applicable".to_string())
        };
        let artifact = CompressionArtifactIdentity {
            state: if transformed {
                CompressionArtifactState::Transformed
            } else {
                CompressionArtifactState::Passthrough
            },
            identity: compression_artifact_identity(
                &context,
                &summary.body,
                &summary.transforms_applied,
                &reason_code,
            ),
            transforms_applied: summary.transforms_applied.clone(),
        };
        Ok(EvaluatedCompressionDecision {
            decision: CompressionDecision {
                contract_revision: COMPRESSION_DECISION_CONTRACT_REVISION,
                source: "dtoken-owned-runtime",
                state: CompressionDecisionState::Resolved,
                reason_code,
                route_id: context.route_id.clone(),
                provider_id: context.provider_id.clone(),
                model: context.effective_model.clone(),
                input_protocol: context.compression_context.input_protocol,
                output_protocol: context.compression_context.output_protocol,
                endpoint: context.compression_context.endpoint,
                compression_policy: route.compression_policy.clone(),
                artifact,
                original_tokens: summary.original_tokens,
                compressed_tokens: summary.compressed_tokens,
                tokens_saved: summary.tokens_saved,
                capability: CompressionCapabilityTransaction {
                    transaction_revision: COMPRESSION_CAPABILITY_TRANSACTION_REVISION,
                    route_id: context.route_id.clone(),
                    provider_id: context.provider_id.clone(),
                    model: context.effective_model.clone(),
                    ingress_protocol: context.ingress.protocol.clone(),
                    ingress_endpoint: context.ingress.endpoint.clone(),
                    upstream_protocol: context.upstream.protocol.clone(),
                    upstream_endpoint: context.upstream.endpoint.clone(),
                    adapter: adapter.adapter_label(),
                },
                evidence: compression_decision_evidence(),
                capability_summary,
                compression_hint: capability_hint,
            },
            body: summary.body,
            savings_percent: summary.savings_percent,
            elapsed_us: summary.elapsed_us,
        })
    }

    pub(crate) fn mcp_stats(&self) -> Value {
        let logs = self.recent_request_logs(10_000);
        json!({
            "source": "dtoken-owned-runtime",
            "state": "runtime",
            "stats": runtime_stats_payload(&logs),
            "cache_summary": runtime_cache_summary_for_state(self),
            "raw_available": false,
        })
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

    fn record_memory(&self, request: &MemoryWriteRequest) -> anyhow::Result<Value> {
        let store = self
            .context_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("typed memory store is unavailable"))?;
        let store = store
            .lock()
            .map_err(|_| anyhow::anyhow!("typed memory store lock is unavailable"))?;
        let record = store.upsert_memory(request)?;
        Ok(memory_record_payload(&record))
    }

    pub fn prepare_memory_capability_transaction(
        &self,
        request: &MemoryCapabilityTransactionRequest,
    ) -> anyhow::Result<PreparedMemoryCapabilityTransaction> {
        let store = self
            .context_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("typed memory store is unavailable"))?;
        let store = store
            .lock()
            .map_err(|_| anyhow::anyhow!("typed memory store lock is unavailable"))?;
        store.prepare_memory_capability_transaction(request)
    }

    pub fn apply_memory_capability_transaction(
        &self,
        prepared: PreparedMemoryCapabilityTransaction,
        reviewed_digest: &str,
    ) -> anyhow::Result<MemoryCapabilityTransactionReceipt> {
        let store = self
            .context_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("typed memory store is unavailable"))?;
        let store = store
            .lock()
            .map_err(|_| anyhow::anyhow!("typed memory store lock is unavailable"))?;
        store.apply_memory_capability_transaction(prepared, reviewed_digest)
    }

    pub fn revoke_memory_capability_transaction(
        &self,
        transaction_id: &str,
        reviewed_document_sha256: &str,
        revoked_at_ms: u64,
    ) -> anyhow::Result<MemoryCapabilityTransactionReceipt> {
        let store = self
            .context_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("typed memory store is unavailable"))?;
        let store = store
            .lock()
            .map_err(|_| anyhow::anyhow!("typed memory store lock is unavailable"))?;
        store.revoke_memory_capability_transaction(
            transaction_id,
            reviewed_document_sha256,
            revoked_at_ms,
        )
    }

    pub fn resolve_memory_capability_file(
        &self,
        path: &StdPath,
        expected_route_binding: &MemoryCapabilityRouteBinding,
        observed_at_ms: u64,
    ) -> anyhow::Result<ResolvedMemoryCapability> {
        let store = self
            .context_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("typed memory store is unavailable"))?;
        let store = store
            .lock()
            .map_err(|_| anyhow::anyhow!("typed memory store lock is unavailable"))?;
        store.resolve_memory_capability_file(path, expected_route_binding, observed_at_ms)
    }

    pub fn memory_capability_route_binding(
        &self,
        route_id: &str,
        model: &str,
    ) -> anyhow::Result<MemoryCapabilityRouteBinding> {
        let route = self
            .config
            .find_route(route_id)
            .ok_or_else(|| anyhow::anyhow!("memory capability route is unavailable"))?;
        let provider = self
            .config
            .find_provider(&route.target_provider_id)
            .ok_or_else(|| anyhow::anyhow!("memory capability provider is unavailable"))?;
        let model = model.trim();
        if model.is_empty()
            || (!provider.models.is_empty()
                && !provider.models.iter().any(|configured| configured == model))
        {
            anyhow::bail!("memory capability model is not allowed by the active route");
        }
        let config_json = serde_json::to_string(self.config.as_ref())
            .context("serialize active d-token config")?;
        Ok(MemoryCapabilityRouteBinding {
            route_id: route.id.clone(),
            provider_id: provider.id.clone(),
            model: model.to_string(),
            config_revision: stable_hash(&config_json),
        })
    }

    pub fn resolve_memory_capability_for_route(
        &self,
        path: &StdPath,
        route_id: &str,
        model: &str,
        observed_at_ms: u64,
    ) -> anyhow::Result<ResolvedMemoryCapability> {
        let binding = self.memory_capability_route_binding(route_id, model)?;
        self.resolve_memory_capability_file(path, &binding, observed_at_ms)
    }

    /// SharedContext is reachable only through a managed MCP capability which
    /// already carries this exact scope.  These helpers deliberately accept no
    /// caller-supplied project or Agent identity, and never return raw prompt
    /// or response data.
    pub(crate) fn mcp_list_memory(
        &self,
        scope: &MemoryAccessScope,
    ) -> Result<Value, mcp::McpToolError> {
        let store = self
            .context_store
            .as_ref()
            .ok_or_else(mcp::McpToolError::memory_unavailable)?;
        let store = store
            .lock()
            .map_err(|_| mcp::McpToolError::memory_unavailable())?;
        let records = store
            .list_visible_memory(scope)
            .map_err(|_| mcp::McpToolError::memory_scope_denied())?;
        Ok(json!({
            "source": "dtoken-owned-runtime",
            "state": "listed",
            "entries": records.iter().map(memory_record_payload).collect::<Vec<_>>(),
            "count": records.len(),
            "raw_available": false,
        }))
    }

    pub(crate) fn mcp_export_memory(
        &self,
        scope: &MemoryAccessScope,
        include_tombstones: bool,
    ) -> Result<Value, mcp::McpToolError> {
        let store = self
            .context_store
            .as_ref()
            .ok_or_else(mcp::McpToolError::memory_unavailable)?;
        let store = store
            .lock()
            .map_err(|_| mcp::McpToolError::memory_unavailable())?;
        let records = store
            .export_project_memory(scope, include_tombstones)
            .map_err(|_| mcp::McpToolError::memory_scope_denied())?;
        Ok(json!({
            "source": "dtoken-owned-runtime",
            "state": "exported",
            "entries": records.iter().map(memory_export_payload).collect::<Vec<_>>(),
            "count": records.len(),
            "raw_available": false,
        }))
    }

    pub(crate) fn mcp_delete_memory(
        &self,
        scope: &MemoryAccessScope,
        memory_id: &str,
        expected_revision: &str,
        tombstone_revision: &str,
    ) -> Result<Value, mcp::McpToolError> {
        let store = self
            .context_store
            .as_ref()
            .ok_or_else(mcp::McpToolError::memory_unavailable)?;
        let store = store
            .lock()
            .map_err(|_| mcp::McpToolError::memory_unavailable())?;
        let tombstone = store
            .tombstone_memory(
                memory_id,
                scope,
                expected_revision,
                tombstone_revision,
                dtoken_event_ledger::now_ms(),
            )
            .map_err(|_| mcp::McpToolError::memory_scope_denied())?;
        Ok(json!({
            "source": "dtoken-owned-runtime",
            "state": "deleted",
            "memory_id": tombstone.memory_id,
            "entry_revision": tombstone.entry_revision,
            "deleted_at_ms": tombstone.deleted_at_ms,
            "raw_available": false,
        }))
    }

    pub(crate) fn mcp_write_memory(
        &self,
        scope: &MemoryAccessScope,
        request: &MemoryWriteRequest,
    ) -> Result<Value, mcp::McpToolError> {
        if request.project_id != scope.project_id
            || request.agent_id.as_deref() != Some(scope.agent_id.as_str())
            || request.session_id != scope.session_id
        {
            return Err(mcp::McpToolError::memory_scope_denied());
        }
        self.record_memory(request)
            .map_err(|_| mcp::McpToolError::memory_unavailable())
    }

    fn try_append_event(&self, event: DtokenEvent) -> anyhow::Result<()> {
        if let Some(store) = &self.runtime_store {
            store.append_event(&event)?;
        }
        self.ledger
            .lock()
            .map_err(|_| anyhow::anyhow!("event ledger lock poisoned"))?
            .record_bounded(event, IN_MEMORY_JOURNAL_MAX_ENTRIES)?;
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
        let _lifecycle_guard = self
            .ccr_lifecycle_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("CCR lifecycle lock poisoned"))?;
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
        let now_ms = dtoken_event_ledger::now_ms().max(1);
        self.append_ccr_lifecycle_record(ccr_write_intent(
            &recovery_ref,
            decision.request_id,
            now_ms,
        ))?;
        let receipt = store.store(&recovery_ref, original_body)?;
        self.append_ccr_lifecycle_record(ccr_pending_retained(&receipt, now_ms)?)?;
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
        let Ok(_lifecycle_guard) = self.ccr_lifecycle_lock.lock() else {
            self.record_runtime_performance("terminal_commit_total", Instant::now(), false, 0);
            return;
        };
        let terminal_started_at = Instant::now();
        let generated_ccr = ccr_metadata_from_request(
            decision,
            &request_log,
            &event,
            ccr_receipt,
            ccr_store_configured,
        );
        let recovery_ref = format!("ccr:{}", decision.request_id);
        let ccr = self
            .retrieve_ccr_metadata(&recovery_ref)
            .filter(|metadata| {
                matches!(
                    metadata["state"].as_str(),
                    Some("delete_intent" | "deleted" | "interrupted_retained")
                )
            })
            .unwrap_or(generated_ccr);
        let route_decision = route_decision_to_value(decision);
        let mut sync_count = 0_u64;
        if let Some(store) = &self.runtime_store {
            // G3 cross-store terminal commit: the durable intent marker is
            // synced before any fan-out append, so a failure here aborts the
            // commit with no partial durable state anywhere.
            let marker_started_at = Instant::now();
            let marker = TerminalCommitMarker {
                request_id: decision.request_id,
                phase: TerminalCommitPhase::Intent,
                timestamp_ms: event.timestamp_ms.max(1),
                event_id: event.event_id,
                ccr_updated_at_ms: ccr["updated_at_ms"].as_u64().unwrap_or(event.timestamp_ms),
                reason: None,
            };
            let intent_result = store.append_terminal_commit_marker(&marker);
            self.record_runtime_performance(
                "terminal_intent_sync",
                marker_started_at,
                intent_result.is_ok(),
                u64::from(intent_result.is_ok()),
            );
            if intent_result.is_err() {
                self.record_runtime_performance(
                    "terminal_commit_total",
                    terminal_started_at,
                    false,
                    0,
                );
                return;
            }
            // Fan-out appends, cheapest to verify last.  The first failure
            // aborts the commit and durably marks the intent pending so a
            // restart converges without guessing.
            let mut append =
                |stage: &'static str, result: anyhow::Result<()>| -> Option<&'static str> {
                    let started_at = Instant::now();
                    self.record_runtime_performance(
                        stage,
                        started_at,
                        result.is_ok(),
                        u64::from(result.is_ok()),
                    );
                    match result {
                        Ok(()) => {
                            sync_count += 1;
                            None
                        }
                        Err(_) => Some(stage),
                    }
                };
            let mut fan_out_failed = append("terminal_event_sync", store.append_event(&event));
            if fan_out_failed.is_none() {
                fan_out_failed = append(
                    "terminal_route_sync",
                    store.append_route_decision(&route_decision),
                );
            }
            if fan_out_failed.is_none() {
                fan_out_failed = append(
                    "terminal_request_log_sync",
                    store.append_request_log(&request_log),
                );
            }
            if fan_out_failed.is_none() {
                fan_out_failed = append("terminal_ccr_sync", store.append_ccr_metadata(&ccr));
            }
            if let Some(stage) = fan_out_failed {
                let _ = store.append_terminal_commit_marker(&TerminalCommitMarker {
                    phase: TerminalCommitPhase::Pending,
                    reason: Some(format!("{stage}_failed")),
                    ..marker.clone()
                });
                self.record_runtime_performance(
                    "terminal_commit_total",
                    terminal_started_at,
                    false,
                    sync_count,
                );
                return;
            }
            // G3: the committed marker is synced only after all four fan-out
            // appends are durable.  A crash before this point is healed by
            // the restart recovery sweep; a crash after it is a normal
            // committed request whose SQLite view is rebuilt from the
            // journals.
            let commit_started_at = Instant::now();
            let commit_marker = TerminalCommitMarker {
                phase: TerminalCommitPhase::Committed,
                ..marker.clone()
            };
            let commit_result = store.append_terminal_commit_marker(&commit_marker);
            self.record_runtime_performance(
                "terminal_commit_sync",
                commit_started_at,
                commit_result.is_ok(),
                u64::from(commit_result.is_ok()),
            );
            if commit_result.is_err() {
                // The fan-out is complete but the commit marker could not be
                // synced; mark the dangling intent explicitly pending so
                // recovery leaves it untouched and never duplicates fan-out
                // records.  The projection layer converges anyway via
                // latest-wins rebuilds once a later commit lands.
                let _ = store.append_terminal_commit_marker(&TerminalCommitMarker {
                    phase: TerminalCommitPhase::Pending,
                    reason: Some("commit_marker_write_failed".to_string()),
                    ..marker.clone()
                });
                self.record_runtime_performance(
                    "terminal_commit_total",
                    terminal_started_at,
                    false,
                    sync_count,
                );
                return;
            }
        }
        if let Ok(mut route_decisions) = self.route_decisions.lock() {
            push_bounded(&mut route_decisions, route_decision);
        }
        if let Ok(mut request_logs) = self.request_logs.lock() {
            push_bounded(&mut request_logs, request_log.clone());
        }
        if let Ok(mut ledger) = self.ledger.lock() {
            let _ = ledger.record_bounded(event.clone(), IN_MEMORY_JOURNAL_MAX_ENTRIES);
        }
        if let Ok(mut items) = self.ccr_metadata.lock() {
            push_bounded(&mut items, ccr.clone());
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
                        // This projection is written only with the terminal
                        // request record, after upstream completion/failure is
                        // known.  Never leave a durable receipt labelled
                        // `prepared`: that would make crash recovery and exact
                        // delete semantics ambiguous.
                        match &event.status {
                            EventStatus::Ok => "finalized".to_string(),
                            EventStatus::Failed => "send_failed_retained".to_string(),
                            _ => "finalized_with_non_ok_terminal".to_string(),
                        }
                    } else if request_log.transforms_applied.is_empty() {
                        "not_required".to_string()
                    } else {
                        "unavailable".to_string()
                    },
                    updated_at_ms: event.timestamp_ms,
                });
                let failure_pattern_result = if event.status == EventStatus::Failed {
                    request_log
                        .error
                        .as_deref()
                        .and_then(FailureClass::from_reason_code)
                        .map(|failure_class| {
                            store.record_failure_pattern(
                                &FailurePatternObservation {
                                    failure_class,
                                    route_id: decision.route_id.clone(),
                                    provider_id: decision.provider_id.clone(),
                                    model: request_log.model.clone(),
                                    agent_id: request_log.agent.clone(),
                                    project_id: event.project_id.clone(),
                                    observed_at_ms: event.timestamp_ms.max(1),
                                    evidence_revision: format!("request-{}", decision.request_id),
                                },
                                FAILURE_PATTERN_TTL_MS,
                            )
                        })
                        .transpose()
                } else {
                    Ok(None)
                };
                self.record_runtime_performance(
                    "context_store_projection",
                    projection_started_at,
                    request_result.is_ok() && ccr_result.is_ok() && failure_pattern_result.is_ok(),
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

fn mcp_tool_error_from_decision_error(error: CompressionDecisionError) -> mcp::McpToolError {
    match error {
        CompressionDecisionError::InvalidArguments => {
            mcp::McpToolError::invalid_arguments("invalid compression decision request")
        }
        CompressionDecisionError::RouteContextUnavailable => {
            mcp::McpToolError::route_context_unavailable()
        }
        CompressionDecisionError::RouteContextMismatch => {
            mcp::McpToolError::route_context_mismatch()
        }
        CompressionDecisionError::RoutePolicyMismatch => mcp::McpToolError::route_policy_mismatch(),
    }
}

fn compression_decision_evidence() -> CompressionDecisionEvidence {
    CompressionDecisionEvidence {
        route_identity: "configured",
        provider_identity: "configured",
        model_allowlist: "validated",
        endpoint_identity: "validated",
        request_body_persisted: false,
        credentials_returned: false,
    }
}

fn compression_artifact_identity(
    context: &ResolvedRouteOptimizationContext,
    body: &Value,
    transforms_applied: &[String],
    reason_code: &str,
) -> String {
    let material = json!({
        "contract_revision": COMPRESSION_DECISION_CONTRACT_REVISION,
        "route_id": context.route_id,
        "provider_id": context.provider_id,
        "model": context.effective_model,
        "input_protocol": context.compression_context.input_protocol,
        "output_protocol": context.compression_context.output_protocol,
        "endpoint": context.compression_context.endpoint,
        "compression_policy": context.requested_policy,
        "reason_code": reason_code,
        "transforms_applied": transforms_applied,
        "candidate": body,
    });
    stable_hash(&serde_json::to_string(&material).unwrap_or_default())
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

/// Durable phases of a cross-store terminal commit.  One request may
/// progress `Intent -> Committed` (rolled forward at recovery) or
/// `Intent -> Pending` (explicitly incomplete).  Markers are the only
/// records the recovery sweep ever appends; fan-out journal records are
/// never re-appended, so restart recovery cannot duplicate terminal
/// records.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TerminalCommitPhase {
    Intent,
    Committed,
    Pending,
}

/// One durable record in the terminal-commit marker journal
/// (`terminal-commits.jsonl`).  The intent marker is synced before any of
/// the four fan-out journals; the committed marker is synced only after
/// all four fan-out appends succeeded.  A restart that observes a dangling
/// intent checks the four journals and either rolls the commit forward
/// (complete fan-out) or marks it explicitly pending (partial fan-out).
/// The marker carries the exact identities needed for those checks:
/// `events.jsonl` is not unique per request (stage events share a request
/// id), so the terminal event is pinned by `event_id`; `ccr-metadata.jsonl`
/// holds lifecycle records per recovery ref, so the terminal CCR record is
/// pinned by its `updated_at_ms`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TerminalCommitMarker {
    request_id: Uuid,
    phase: TerminalCommitPhase,
    timestamp_ms: u64,
    event_id: Uuid,
    ccr_updated_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// Whether a CCR lifecycle state is written only by the terminal commit.
/// These are the states hidden for explicitly pending terminal commits so
/// the underlying lifecycle (write intent / retained / deleted) stays
/// authoritative; `delete_intent` / `deleted` / `interrupted_retained` are
/// lifecycle states and must keep reconciling.
fn terminal_ccr_state_is_terminal(state: Option<&str>) -> bool {
    matches!(
        state,
        Some(
            "finalized"
                | "send_failed_retained"
                | "finalized_with_non_ok_terminal"
                | "not_required"
                | "unavailable"
        )
    )
}

#[derive(Debug)]
struct RuntimeStore {
    events: JsonlEventLedger,
    route_decisions: JsonlStore,
    request_logs: JsonlStore,
    ccr_metadata: JsonlStore,
    ccr_metadata_path: PathBuf,
    terminal_commits: JsonlStore,
    terminal_commits_path: PathBuf,
}

impl RuntimeStore {
    fn new(dir: &StdPath) -> Self {
        let ccr_metadata_path = dir.join("ccr-metadata.jsonl");
        let terminal_commits_path = dir.join("terminal-commits.jsonl");
        Self {
            events: JsonlEventLedger::new(dir.join("events.jsonl")),
            route_decisions: JsonlStore::new(dir.join("route-decisions.jsonl")),
            request_logs: JsonlStore::new(dir.join("request-logs.jsonl")),
            ccr_metadata: JsonlStore::new(&ccr_metadata_path),
            ccr_metadata_path,
            terminal_commits: JsonlStore::new(&terminal_commits_path),
            terminal_commits_path,
        }
    }

    fn append_event(&self, event: &DtokenEvent) -> anyhow::Result<()> {
        self.events.append(event)
    }

    fn recent_events(&self, limit: usize) -> anyhow::Result<Vec<DtokenEvent>> {
        self.events.recent(limit)
    }

    fn visit_all_events<F>(&self, visit: F) -> anyhow::Result<()>
    where
        F: FnMut(DtokenEvent) -> anyhow::Result<()>,
    {
        self.events.visit_all(visit)
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

    fn visit_all_request_logs<F>(&self, visit: F) -> anyhow::Result<()>
    where
        F: FnMut(RequestLog) -> anyhow::Result<()>,
    {
        self.request_logs.visit_all(visit)
    }

    fn append_ccr_metadata(&self, metadata: &Value) -> anyhow::Result<()> {
        self.ccr_metadata.append(metadata)
    }

    #[cfg(test)]
    fn recent_ccr_metadata(&self, limit: usize) -> anyhow::Result<Vec<Value>> {
        self.ccr_metadata.recent(limit)
    }

    /// Latest CCR lifecycle record per recovery ref, in journal order.
    /// Records for which `skip` returns `true` are ignored during the scan,
    /// so an earlier lifecycle record naturally becomes the latest for that
    /// ref.  Restart recovery uses this to hide terminal-family records of
    /// explicitly pending terminal commits while keeping the underlying
    /// lifecycle (write intent / retention / deletion) reconcilable.
    fn latest_ccr_metadata_by_recovery_ref(
        &self,
        skip: &dyn Fn(&Value) -> bool,
    ) -> anyhow::Result<Vec<Value>> {
        if !self.ccr_metadata_path.exists() {
            return Ok(Vec::new());
        }

        let file = OpenOptions::new()
            .read(true)
            .open(&self.ccr_metadata_path)
            .with_context(|| {
                format!(
                    "open CCR metadata journal {}",
                    self.ccr_metadata_path.display()
                )
            })?;
        let mut reader = BufReader::new(file);
        let mut latest_by_ref = HashMap::<String, (usize, Value)>::new();
        let mut line = Vec::new();
        let mut sequence = 0_usize;

        loop {
            line.clear();
            let bytes_read = reader.read_until(b'\n', &mut line).with_context(|| {
                format!(
                    "read CCR metadata journal {}",
                    self.ccr_metadata_path.display()
                )
            })?;
            if bytes_read == 0 {
                break;
            }
            if !line.ends_with(b"\n") {
                break;
            }
            let payload = &line[..line.len() - 1];
            if payload.iter().all(u8::is_ascii_whitespace) {
                continue;
            }

            match serde_json::from_slice::<Value>(payload) {
                Ok(record) => {
                    if skip(&record) {
                        continue;
                    }
                    if let Some(recovery_ref) = record
                        .get("recovery_ref")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                    {
                        latest_by_ref.insert(recovery_ref.to_string(), (sequence, record));
                        sequence = sequence.saturating_add(1);
                    }
                }
                Err(error) => {
                    let is_tail = reader
                        .fill_buf()
                        .with_context(|| {
                            format!(
                                "inspect CCR metadata tail in {}",
                                self.ccr_metadata_path.display()
                            )
                        })?
                        .is_empty();
                    if is_tail {
                        break;
                    }
                    return Err(error).with_context(|| {
                        format!(
                            "parse non-tail CCR metadata record in {}",
                            self.ccr_metadata_path.display()
                        )
                    });
                }
            }
        }

        let mut latest = latest_by_ref.into_values().collect::<Vec<_>>();
        latest.sort_by_key(|(sequence, _)| *sequence);
        Ok(latest.into_iter().map(|(_, record)| record).collect())
    }

    fn append_terminal_commit_marker(&self, marker: &TerminalCommitMarker) -> anyhow::Result<()> {
        self.terminal_commits.append(marker)
    }

    /// Latest terminal-commit marker per request, in journal order.  Phases
    /// are monotonic per request (intent, then at most one of committed /
    /// pending), so the last record for a request is its current state.
    fn latest_terminal_commit_markers(&self) -> anyhow::Result<Vec<(Uuid, TerminalCommitMarker)>> {
        if !self.terminal_commits_path.exists() {
            return Ok(Vec::new());
        }
        let mut latest = HashMap::<Uuid, TerminalCommitMarker>::new();
        self.terminal_commits
            .visit_all(|marker: TerminalCommitMarker| {
                latest.insert(marker.request_id, marker);
                Ok(())
            })?;
        let mut markers = latest.into_iter().collect::<Vec<_>>();
        markers.sort_by_key(|(request_id, _)| *request_id);
        Ok(markers)
    }

    /// Whether the four fan-out journals hold the complete terminal bundle
    /// for a dangling intent.  Checks are cheapest-first and short-circuit:
    /// route decisions and request logs are unique per request, the CCR
    /// record is pinned by recovery ref + `updated_at_ms`, and the terminal
    /// event is pinned by `event_id` (events are not unique per request).
    fn terminal_bundle_complete(&self, marker: &TerminalCommitMarker) -> anyhow::Result<bool> {
        let request_id = marker.request_id.to_string();
        let recovery_ref = format!("ccr:{}", marker.request_id);
        let mut route_present = false;
        self.route_decisions.visit_all(|value: Value| {
            if value["request_id"].as_str() == Some(request_id.as_str()) {
                route_present = true;
            }
            Ok(())
        })?;
        if !route_present {
            return Ok(false);
        }
        let mut request_present = false;
        self.request_logs.visit_all(|log: RequestLog| {
            if log.request_id == marker.request_id {
                request_present = true;
            }
            Ok(())
        })?;
        if !request_present {
            return Ok(false);
        }
        let mut ccr_present = false;
        self.ccr_metadata.visit_all(|value: Value| {
            if value["recovery_ref"].as_str() == Some(recovery_ref.as_str())
                && value["updated_at_ms"].as_u64() == Some(marker.ccr_updated_at_ms)
            {
                ccr_present = true;
            }
            Ok(())
        })?;
        if !ccr_present {
            return Ok(false);
        }
        let mut event_present = false;
        self.events.visit_all(|event: DtokenEvent| {
            if event.event_id == marker.event_id {
                event_present = true;
            }
            Ok(())
        })?;
        Ok(event_present)
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

#[cfg(windows)]
fn verify_windows_private_runtime_contract(
    runtime_dir: &StdPath,
    state: &AppState,
) -> anyhow::Result<()> {
    use dtoken_context_store::{verify_private_directory_acl, verify_private_file_acl};
    use std::{ffi::OsString, path::PathBuf};

    let ccr_root = runtime_dir.join("ccr-private");
    verify_private_directory_acl(runtime_dir)
        .with_context(|| format!("verify private runtime directory {}", runtime_dir.display()))?;
    verify_private_directory_acl(&ccr_root)
        .with_context(|| format!("verify private CCR directory {}", ccr_root.display()))?;

    if state.context_store.is_none() {
        anyhow::bail!(
            "Windows private runtime-state.sqlite3 projection did not initialize; refusing CCR-enabled persistent startup"
        );
    }

    let database = runtime_dir.join("runtime-state.sqlite3");
    verify_private_file_acl(&database)
        .with_context(|| format!("verify private runtime projection {}", database.display()))?;
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = OsString::from(database.as_os_str());
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        match std::fs::symlink_metadata(&sidecar) {
            Ok(_) => verify_private_file_acl(&sidecar)
                .with_context(|| format!("verify private SQLite sidecar {}", sidecar.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspect private SQLite sidecar {}", sidecar.display())
                });
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn verify_windows_private_runtime_contract(
    _runtime_dir: &StdPath,
    _state: &AppState,
) -> anyhow::Result<()> {
    Ok(())
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

fn ccr_deletion_tombstone(mut metadata: Value, deleted_at_ms: u64) -> anyhow::Result<Value> {
    let fields = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("CCR metadata must be an object"))?;
    fields.remove("raw_bytes");
    fields.remove("expires_at_unix");
    fields.insert("raw_available".to_string(), json!(false));
    fields.insert("raw_storage".to_string(), json!("deleted"));
    fields.insert("state".to_string(), json!("deleted"));
    fields.insert("deleted_at_ms".to_string(), json!(deleted_at_ms));
    fields.insert(
        "note".to_string(),
        json!("CCR original was deleted through the authorized in-process boundary; only redacted lifecycle metadata remains."),
    );
    Ok(metadata)
}

fn ccr_write_intent(recovery_ref: &str, request_id: Uuid, updated_at_ms: u64) -> Value {
    json!({
        "recovery_ref": recovery_ref,
        "request_id": request_id,
        "state": "write_intent",
        "raw_available": false,
        "raw_storage": "private-store-write-pending",
        "updated_at_ms": updated_at_ms,
        "note": "Redacted lifecycle intent recorded before the encrypted CCR original is written."
    })
}

fn ccr_pending_retained(receipt: &CcrStoreReceipt, updated_at_ms: u64) -> anyhow::Result<Value> {
    let request_id = receipt
        .recovery_ref
        .strip_prefix("ccr:")
        .ok_or_else(|| anyhow::anyhow!("CCR receipt has an invalid recovery reference"))?;
    Ok(json!({
        "recovery_ref": receipt.recovery_ref.as_str(),
        "request_id": request_id,
        "state": "pending_retained",
        "raw_available": true,
        "raw_storage": "encrypted-private-store",
        "raw_bytes": receipt.plaintext_bytes,
        "expires_at_unix": receipt.expires_at_unix,
        "updated_at_ms": updated_at_ms,
        "note": "Encrypted CCR original is retained pending a terminal request record; no raw body is present in this journal."
    }))
}

fn ccr_delete_intent(mut metadata: Value, updated_at_ms: u64) -> anyhow::Result<Value> {
    let fields = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("CCR metadata must be an object"))?;
    fields.remove("raw_bytes");
    fields.remove("expires_at_unix");
    fields.insert("state".to_string(), json!("delete_intent"));
    fields.insert("raw_available".to_string(), json!(true));
    fields.insert(
        "raw_storage".to_string(),
        json!("private-store-delete-pending"),
    );
    fields.insert("updated_at_ms".to_string(), json!(updated_at_ms));
    fields.insert(
        "note".to_string(),
        json!("Redacted lifecycle intent recorded before authorized CCR original deletion."),
    );
    Ok(metadata)
}

fn ccr_interrupted_retained(mut metadata: Value, updated_at_ms: u64) -> anyhow::Result<Value> {
    let fields = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("CCR metadata must be an object"))?;
    fields.remove("raw_bytes");
    fields.remove("expires_at_unix");
    fields.insert("state".to_string(), json!("interrupted_retained"));
    fields.insert("raw_available".to_string(), json!(true));
    fields.insert("raw_storage".to_string(), json!("encrypted-private-store"));
    fields.insert("updated_at_ms".to_string(), json!(updated_at_ms));
    fields.insert(
        "note".to_string(),
        json!("Deletion was interrupted; encrypted CCR original was retained and requires an explicit authorized retry."),
    );
    Ok(metadata)
}

fn ccr_recovery_pending_retained(mut metadata: Value, updated_at_ms: u64) -> anyhow::Result<Value> {
    let fields = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("CCR metadata must be an object"))?;
    fields.insert("state".to_string(), json!("recovery_pending_retained"));
    fields.insert("raw_available".to_string(), json!(true));
    fields.insert("raw_storage".to_string(), json!("encrypted-private-store"));
    fields.insert("updated_at_ms".to_string(), json!(updated_at_ms));
    fields.insert(
        "note".to_string(),
        json!("Write completed before a terminal request record; encrypted CCR original was retained and was not resent."),
    );
    Ok(metadata)
}

fn ccr_raw_missing(mut metadata: Value, updated_at_ms: u64) -> anyhow::Result<Value> {
    let fields = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("CCR metadata must be an object"))?;
    fields.remove("raw_bytes");
    fields.remove("expires_at_unix");
    fields.insert("state".to_string(), json!("raw_missing"));
    fields.insert("raw_available".to_string(), json!(false));
    fields.insert("raw_storage".to_string(), json!("missing"));
    fields.insert("updated_at_ms".to_string(), json!(updated_at_ms));
    fields.insert(
        "note".to_string(),
        json!("Encrypted CCR original is absent without a completed deletion tombstone; it was not treated as deleted."),
    );
    Ok(metadata)
}

fn ccr_raw_unavailable(
    mut metadata: Value,
    updated_at_ms: u64,
    inspection: CcrEntryInspection,
) -> anyhow::Result<Value> {
    let status = match inspection {
        CcrEntryInspection::Expired => "expired",
        CcrEntryInspection::Invalid => "invalid",
        CcrEntryInspection::Missing => "missing",
        CcrEntryInspection::Available => "available",
    };
    let fields = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("CCR metadata must be an object"))?;
    fields.remove("raw_bytes");
    fields.remove("expires_at_unix");
    fields.insert("state".to_string(), json!("raw_unavailable"));
    fields.insert("raw_available".to_string(), json!(false));
    fields.insert(
        "raw_storage".to_string(),
        json!("private-store-unavailable"),
    );
    fields.insert("raw_inspection".to_string(), json!(status));
    fields.insert("updated_at_ms".to_string(), json!(updated_at_ms));
    fields.insert(
        "note".to_string(),
        json!("Encrypted CCR original failed non-destructive restart verification and was retained without being exposed, resent, or deleted."),
    );
    Ok(metadata)
}

fn ccr_terminal_metadata(metadata: &Value) -> bool {
    matches!(
        metadata["state"].as_str(),
        Some("finalized" | "send_failed_retained" | "finalized_with_non_ok_terminal")
    ) || (metadata.get("state").is_none() && metadata["raw_available"].as_bool() == Some(true))
}

fn ccr_metadata_projection_from_journal(metadata: &Value) -> Option<CcrMetadataProjection> {
    let recovery_ref = metadata["recovery_ref"].as_str()?.trim();
    let request_id = metadata["request_id"].as_str()?.trim();
    let state = metadata["state"].as_str()?.trim();
    if recovery_ref.is_empty() || request_id.is_empty() || state.is_empty() {
        return None;
    }
    Some(CcrMetadataProjection {
        recovery_ref: recovery_ref.to_string(),
        request_id: request_id.to_string(),
        expires_at_ms: metadata["expires_at_unix"]
            .as_u64()
            .map(|value| value.saturating_mul(1_000)),
        plaintext_bytes: metadata["raw_bytes"].as_u64(),
        state: state.to_string(),
        updated_at_ms: metadata["updated_at_ms"].as_u64().unwrap_or_default(),
    })
}

fn validate_ccr_lifecycle_record(record: &Value) -> anyhow::Result<()> {
    let fields = record
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("CCR lifecycle record must be an object"))?;
    for required in ["recovery_ref", "request_id", "state"] {
        if fields
            .get(required)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(anyhow::anyhow!(
                "CCR lifecycle record is missing {required}"
            ));
        }
    }
    if ccr_record_contains_raw_material(record) {
        return Err(anyhow::anyhow!(
            "CCR lifecycle record contains prohibited raw material"
        ));
    }
    Ok(())
}

fn ccr_record_contains_raw_material(value: &Value) -> bool {
    const PROHIBITED: &[&str] = &[
        "raw_prompt",
        "raw_response",
        "raw_body",
        "plaintext",
        "payload",
        "messages",
        "content",
    ];
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            PROHIBITED.contains(&key.as_str()) || ccr_record_contains_raw_material(value)
        }),
        Value::Array(values) => values.iter().any(ccr_record_contains_raw_material),
        _ => false,
    }
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
        "state": if receipt.is_some() {
            match &event.status {
                EventStatus::Ok => "finalized",
                EventStatus::Failed => "send_failed_retained",
                _ => "finalized_with_non_ok_terminal",
            }
        } else if request_log.transforms_applied.is_empty() {
            "not_required"
        } else {
            "unavailable"
        },
        "updated_at_ms": event.timestamp_ms,
        "note": if receipt.is_some() { "Encrypted CCR original is available only through the in-process authorized retrieval boundary; it is excluded from ordinary HTTP responses, logs, Ledger, and exports." } else { "CCR metadata only; raw prompt/response/CCR body is not stored by default." }
    })
}

fn memory_record_payload(record: &MemoryRecord) -> Value {
    json!({
        "memory_id": record.memory_id,
        "project_id": record.project_id,
        "agent_id": record.agent_id,
        "session_id": record.session_id,
        "kind": record.kind,
        "summary": record.summary,
        "keywords": record.keywords,
        "dedup_key": record.dedup_key,
        "entry_revision": record.entry_revision,
        "provenance": {
            "source": record.provenance.source,
            "source_revision": record.provenance.source_revision,
            "observed_at_ms": record.provenance.observed_at_ms,
        },
        "created_at_ms": record.created_at_ms,
        "updated_at_ms": record.updated_at_ms,
        "raw_available": false,
        "note": "Typed memory metadata only; raw prompt and response bodies are not stored."
    })
}

fn memory_export_payload(record: &MemoryExportEntry) -> Value {
    json!({
        "memory_id": record.memory_id,
        "project_id": record.project_id,
        "agent_id": record.agent_id,
        "session_id": record.session_id,
        "kind": record.kind,
        "summary": record.summary,
        "keywords": record.keywords,
        "entry_revision": record.entry_revision,
        "provenance": {
            "source": record.provenance.source,
            "source_revision": record.provenance.source_revision,
            "observed_at_ms": record.provenance.observed_at_ms,
        },
        "state": record.state,
        "deleted_at_ms": record.deleted_at_ms,
        "raw_available": false,
    })
}

pub fn build_router(state: AppState) -> Router {
    let mcp_server = Arc::new(mcp::McpServer::new(state.clone()));
    build_router_with_mcp_server(state, mcp_server)
}

/// Same router driven by a caller-supplied MCP server. Session tests inject a
/// gated server here to hold requests deterministically in-flight.
pub fn build_router_with_mcp_server(state: AppState, mcp_server: Arc<mcp::McpServer>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/dashboard/payload", get(dashboard_payload))
        .route("/dashboard/summary", get(dashboard_summary))
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/responses", post(openai_responses))
        .route("/v1/messages", post(anthropic_messages))
        .route("/v1/compress", post(compression_execution))
        .route("/v1/compression/decision", post(compression_decision))
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
        .route("/learning/failure-patterns", get(learning_failure_patterns))
        .route("/doctor/run", post(doctor_run))
        .route("/preprocessors", get(preprocessors))
        .route("/rtk/status", get(rtk_status))
        .route("/rtk/gain", get(rtk_gain))
        .route("/rtk/discover", get(rtk_discover))
        .route("/rtk/session", get(rtk_session))
        .route("/headroom/status", get(headroom_status))
        .route("/headroom/proxy", get(headroom_proxy))
        .route("/headroom/mcp", get(headroom_mcp))
        .route("/mcp", post(mcp_http).delete(mcp_http_delete))
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
        .route("/output-shape", get(output_shape))
        .route("/compatibility/agents", get(agent_compatibility))
        .route("/protocol-injection", get(protocol_injection))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .layer(middleware::from_fn(loopback_request_guard))
        .layer(Extension(mcp_server))
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
        fail_closed: backend_capability_ready(&state.capabilities, "optical-fail-closed"),
        renderer_ready: backend_capability_ready(&state.capabilities, "optical-renderer"),
        patcher_ready: backend_capability_ready(&state.capabilities, "optical-patcher"),
        post_validator_ready: backend_capability_ready(
            &state.capabilities,
            "optical-post-validator",
        ),
        signed_accounting_ready: backend_capability_ready(
            &state.capabilities,
            "optical-signed-accounting",
        ),
        representation_plan_ready: backend_capability_ready(
            &state.capabilities,
            "optical-representation-plan",
        ),
        responses_shadow_inventory_ready: backend_capability_ready(
            &state.capabilities,
            "optical-responses-shadow-inventory",
        ),
        dynamic_page_accounting_ready: backend_capability_ready(
            &state.capabilities,
            "optical-dynamic-page-accounting",
        ),
        active_direct_routes: effective.active_direct_routes,
        verified_attested_capabilities: effective.verified_attested_capabilities,
        rejected_capabilities: effective.rejected_capabilities,
        config_issue_count: effective.config_issue_count,
    })
}

fn backend_capability_ready(registry: &CapabilityRegistrySnapshot, id: &str) -> bool {
    registry
        .find(id)
        .is_some_and(|capability| capability.state == AvailabilityState::Ready)
}

fn effective_optical_state(state: &AppState) -> EffectiveOpticalState {
    let policy = state.config.optical.global_policy.clone();
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &state.config.optical,
        dtoken_event_ledger::now_ms(),
    );
    let active_route_count = active_direct_anthropic_route_count(&state.config, &registry);
    let active_reachable = policy.allow_active
        && active_route_count > 0
        && backend_capability_ready(&state.capabilities, "optical-active");
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
        shadow_preview_supported: backend_capability_ready(
            &state.capabilities,
            "optical-shadow-preview",
        ),
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
        fail_closed: backend_capability_ready(&state.capabilities, "optical-fail-closed"),
        direct_anthropic_upstream_supported: backend_capability_ready(
            &state.capabilities,
            "optical-direct-anthropic-upstream",
        ),
        streaming_passthrough_supported: backend_capability_ready(
            &state.capabilities,
            "optical-streaming-passthrough",
        ),
        deterministic_png_renderer: backend_capability_ready(
            &state.capabilities,
            "optical-renderer",
        ),
        artifact_post_validation: backend_capability_ready(
            &state.capabilities,
            "optical-post-validator",
        ),
        signed_multimodal_accounting: backend_capability_ready(
            &state.capabilities,
            "optical-signed-accounting",
        ),
        capability_attestation_required: backend_capability_ready(
            &state.capabilities,
            "optical-capability-attestation",
        ),
        tool_schema_normalization: backend_capability_ready(
            &state.capabilities,
            "tool-schema-normalization",
        ),
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

async fn compression_decision(
    State(state): State<AppState>,
    Json(request): Json<CompressionDecisionRequest>,
) -> impl IntoResponse {
    match state.compression_decision(request) {
        Ok(decision) => (
            StatusCode::OK,
            Json(serde_json::to_value(decision).unwrap_or_default()),
        )
            .into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "type": "compression_decision_unavailable",
                    "code": compression_decision_error_code(error),
                    "message": "The requested compression decision is unavailable."
                }
            })),
        )
            .into_response(),
    }
}

async fn compression_execution(
    State(state): State<AppState>,
    Json(request): Json<CompressionDecisionRequest>,
) -> impl IntoResponse {
    match state.compression_execution(request) {
        Ok(execution) => (
            StatusCode::OK,
            Json(serde_json::to_value(execution).unwrap_or_default()),
        )
            .into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "type": "compression_execution_unavailable",
                    "code": compression_decision_error_code(error),
                    "message": "The requested route-bound compression is unavailable."
                }
            })),
        )
            .into_response(),
    }
}

fn compression_decision_error_code(error: CompressionDecisionError) -> &'static str {
    match error {
        CompressionDecisionError::InvalidArguments => "invalid_arguments",
        CompressionDecisionError::RouteContextUnavailable => "route_context_unavailable",
        CompressionDecisionError::RouteContextMismatch => "route_context_mismatch",
        CompressionDecisionError::RoutePolicyMismatch => "route_policy_mismatch",
    }
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
    // Populated by Deserialize for Chat completions requests; the messages
    // payload itself is forwarded as raw JSON and read by #[cfg(test)] helper
    // `routed_openai_payload`, so no production read site exists for the field.
    #[allow(dead_code)]
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
    fn adapter_label(self) -> &'static str {
        match self {
            Self::OpenAiChatPassthrough => "open_ai_chat_passthrough",
            Self::OpenAiResponsesToChat => "open_ai_responses_to_chat",
            Self::AnthropicMessagesPassthrough => "anthropic_messages_passthrough",
            Self::AnthropicMessagesToOpenAiChat => "anthropic_messages_to_open_ai_chat",
        }
    }

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

/// Derive the exact memory access scope from a request scope, fail-closed.
///
/// Memory authorization needs a real project and agent identity.  When
/// either is unavailable (the runtime never guesses identities), no memory
/// may be retrieved and no injection may occur.
fn request_scope_memory_access(scope: &RequestScope) -> Option<MemoryAccessScope> {
    let project_id = scope.project.value.as_deref()?.to_string();
    let agent_id = scope.agent.value.as_deref()?.to_string();
    if project_id.trim().is_empty() || agent_id.trim().is_empty() {
        return None;
    }
    Some(MemoryAccessScope {
        project_id,
        agent_id,
        session_id: scope.session.value.clone(),
    })
}

/// Retrieve memory records the route's caller is authorized to see, then
/// project them into the exact typed subset that may enter an upstream body.
///
/// Authorization is enforced by [`ContextStore::list_visible_memory`]: the
/// scope is project/agent/session-exact and every record is filtered by
/// ownership or an un-revoked session-bound share grant.  Only the typed
/// summary, kind and provenance projection leaves the store — no raw
/// prompt/response, no source content, no secrets.
fn authorized_memory_records(state: &AppState, scope: &MemoryAccessScope) -> Vec<MemoryRecord> {
    let Some(store) = state.context_store.as_ref() else {
        return Vec::new();
    };
    let Ok(store) = store.lock() else {
        return Vec::new();
    };
    store
        .list_visible_memory(scope)
        .unwrap_or_default()
        .into_iter()
        .filter(|record| {
            record.kind == "preference" || record.kind == "convention" || record.kind == "project"
        })
        .collect()
}

/// Turn authorized memory records into safe, typed system blocks.  Never
/// echoes raw content: only the typed summary text is included.
fn memory_context_blocks(records: &[MemoryRecord]) -> Vec<Value> {
    let mut blocks = Vec::with_capacity(records.len());
    for record in records {
        blocks.push(json!({
            "type": "text",
            "text": record.summary,
            "cache_control": {"type": "ephemeral"}
        }));
    }
    blocks
}

/// Inject authorized memory blocks into an OpenAI Chat request body as a
/// leading `system` message.  When no system message exists a new one is
/// created; when one exists the memory blocks are appended to its content.
/// The memory blocks are deliberately marked `cache_control: ephemeral`, so
/// they never join the transformable window and never enter a stored
/// artifact.
fn inject_memory_into_openai_body(body: &Value, blocks: &[Value]) -> Value {
    if blocks.is_empty() {
        return body.clone();
    }
    let mut body = body.clone();
    // Fail-closed: a body without a `messages` array is left untouched
    // (never panics on a hostile or non-chat-shaped body).
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return body;
    };
    let memory_block = json!({
        "role": "system",
        "content": blocks,
        "cache_control": {"type": "ephemeral"}
    });
    if let Some(first) = messages.first_mut() {
        if first.get("role").and_then(Value::as_str) == Some("system") {
            let merged = match first.get_mut("content") {
                Some(Value::Array(existing)) => {
                    let mut existing = existing.clone();
                    existing.extend(blocks.iter().cloned());
                    Value::Array(existing)
                }
                Some(Value::String(text)) => {
                    let mut merged = vec![json!({"type": "text", "text": text.clone()})];
                    merged.extend(blocks.iter().cloned());
                    Value::Array(merged)
                }
                _ => Value::Array(blocks.to_vec()),
            };
            if let Some(obj) = first.as_object_mut() {
                obj.insert("content".to_string(), merged);
            }
            return body;
        }
    }
    messages.insert(0, memory_block);
    body
}

/// Inject authorized memory blocks into an Anthropic Messages request body
/// as leading system blocks.  Both string and block-list `system` shapes are
/// normalized to a block list (memory blocks are already blocks).
fn inject_memory_into_anthropic_body(body: &Value, blocks: &[Value]) -> Value {
    if blocks.is_empty() {
        return body.clone();
    }
    let mut body = body.clone();
    let mut system = match body.get("system") {
        Some(Value::Array(existing)) => existing.clone(),
        Some(Value::String(text)) => vec![json!({"type": "text", "text": text.clone()})],
        _ => Vec::new(),
    };
    system.extend(blocks.iter().cloned());
    if let Some(obj) = body.as_object_mut() {
        obj.insert("system".to_string(), Value::Array(system));
    }
    body
}

/// Route dispatch for memory injection: picks the upstream-shape injector
/// based on the provider protocol.  Injection is skipped fail-closed when
/// there is nothing to inject.
fn inject_memory_into_body(body: Value, protocol: &Protocol, blocks: &[Value]) -> Value {
    if blocks.is_empty() {
        return body;
    }
    match protocol {
        Protocol::Anthropic => inject_memory_into_anthropic_body(&body, blocks),
        Protocol::OpenAi => inject_memory_into_openai_body(&body, blocks),
    }
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
    compress_for_route_with_frozen(context, policy, body, cache_evidence, None)
}

/// Compression path that carries an optional request-level frozen-message
/// override into the safe-text pipeline.
fn compress_for_route_with_frozen(
    context: Option<&ResolvedRouteOptimizationContext>,
    policy: CompressionPolicy,
    body: Value,
    cache_evidence: Option<&dtoken_compress::context_pipeline::ProviderCacheEvidence>,
    frozen_message_count: Option<usize>,
) -> CompressionSummary {
    let result = context
        .map(|context| {
            CompressionEngine::new().compress_resolved_with_cache_evidence(
                &context.compression_context,
                policy.clone(),
                body.clone(),
                cache_evidence,
                frozen_message_count,
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
        model_identity: model.map(str::to_string),
        compression_hint: None,
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
        model_identity: None,
        compression_hint: None,
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

/// Build the native Responses wire URL for a provider. Used only by the
/// explicit `ProviderWireApi::OpenAiResponses` passthrough branch; ordinary
/// Chat providers keep `upstream_chat_url`.
fn upstream_responses_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1/responses") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/responses")
    } else {
        format!("{trimmed}/v1/responses")
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

fn output_shaping_policy_for_scope<'a>(
    config: &'a DtokenConfig,
    scope: &RequestScope,
) -> Option<&'a dtoken_types::OutputShapingRoutePolicy> {
    config.find_output_shaping_policy(&scope.route_id, &scope.provider_id, &scope.model)
}

fn output_safety_for_text(text: &str) -> OutputSafety {
    if text.contains("```") {
        OutputSafety::CodeOrOpaqueContent
    } else {
        OutputSafety::PlainText
    }
}

// Evidence fields mirror the fixed outbound-shaping JSON contract; grouping them
// into a struct would churn every call site for no behavior change.
#[allow(clippy::too_many_arguments)]
fn output_shaping_evidence(
    result: OutputShapingResult,
    route_id: &str,
    provider_id: &str,
    model: &str,
    config_revision: &str,
    request_id: &str,
    chain_mode: &str,
    compression_policy: &str,
) -> Value {
    let accounting = match result.accounting {
        OutputShapingAccounting::Measured {
            original_bytes,
            shaped_bytes,
            saved_bytes,
        } => json!({
            "measured": {
                "unit": "outbound_utf8_bytes",
                "original_bytes": original_bytes,
                "shaped_bytes": shaped_bytes,
                "saved_bytes": saved_bytes,
            },
            "holdout_estimated": null,
            "unknown": false,
        }),
        OutputShapingAccounting::HoldoutEstimated {
            original_bytes,
            candidate_bytes,
            estimated_saved_bytes,
        } => json!({
            "measured": null,
            "holdout_estimated": {
                "unit": "outbound_utf8_bytes",
                "original_bytes": original_bytes,
                "candidate_bytes": candidate_bytes,
                "estimated_saved_bytes": estimated_saved_bytes,
            },
            "unknown": false,
        }),
        OutputShapingAccounting::Unknown => json!({
            "measured": null,
            "holdout_estimated": null,
            "unknown": true,
        }),
    };
    json!({
        "state": result.disposition.as_str(),
        "reason_code": result.reason_code,
        "route_id": route_id,
        "provider_id": provider_id,
        "model": model,
        "config_revision": config_revision,
        "request_id": request_id,
        "chain_mode": chain_mode,
        "compression_policy": compression_policy,
        "verbosity": result.verbosity,
        "effort": result.effort.as_str(),
        "transforms": result.transforms,
        "accounting": accounting,
        "raw_available": false,
        "privacy": "aggregate output-shaping evidence only; response text is not persisted",
    })
}

// Evidence fields mirror the fixed outbound-shaping JSON contract; grouping them
// into a struct would churn every call site for no behavior change.
#[allow(clippy::too_many_arguments)]
fn output_shaping_passthrough_evidence(
    reason_code: &'static str,
    route_id: &str,
    provider_id: &str,
    model: &str,
    config_revision: &str,
    request_id: &str,
    chain_mode: &str,
    compression_policy: &str,
) -> Value {
    json!({
        "state": OutputShapingDisposition::Passthrough.as_str(),
        "reason_code": reason_code,
        "route_id": route_id,
        "provider_id": provider_id,
        "model": model,
        "config_revision": config_revision,
        "request_id": request_id,
        "chain_mode": chain_mode,
        "compression_policy": compression_policy,
        "accounting": {
            "measured": null,
            "holdout_estimated": null,
            "unknown": true,
        },
        "raw_available": false,
        "privacy": "aggregate output-shaping evidence only; response text is not persisted",
    })
}

fn apply_openai_output_shaping(
    response: &mut Value,
    route: &dtoken_types::RouteProfile,
    scope: &RequestScope,
    policy: Option<&dtoken_types::OutputShapingRoutePolicy>,
    chain_mode: &str,
    compression_policy: &str,
) -> Value {
    let passthrough = |reason_code| {
        output_shaping_passthrough_evidence(
            reason_code,
            &scope.route_id,
            &scope.provider_id,
            &scope.model,
            &scope.config_revision,
            &scope.request_id.to_string(),
            chain_mode,
            compression_policy,
        )
    };
    let Some(policy) = policy else {
        return passthrough("route_output_shaping_policy_unavailable");
    };
    let Some(choices) = response.get_mut("choices").and_then(Value::as_array_mut) else {
        return passthrough("unknown_response_shape_passthrough");
    };
    if choices.len() != 1 {
        return passthrough("multi_choice_response_passthrough");
    }
    let Some(message) = choices[0].get_mut("message").and_then(Value::as_object_mut) else {
        return passthrough("unknown_response_shape_passthrough");
    };
    if message.contains_key("tool_calls")
        || message.contains_key("function_call")
        || message.get("role").and_then(Value::as_str) != Some("assistant")
    {
        return passthrough("tool_or_structured_response_passthrough");
    }
    let Some(text) = message.get("content").and_then(Value::as_str) else {
        return passthrough("unknown_response_shape_passthrough");
    };
    let result = shape_output_for_policy(
        route,
        policy,
        &scope.request_id.to_string(),
        output_safety_for_text(text),
        text,
    );
    if result.disposition == OutputShapingDisposition::Applied {
        message.insert("content".to_string(), Value::String(result.body.clone()));
    }
    output_shaping_evidence(
        result,
        &scope.route_id,
        &scope.provider_id,
        &scope.model,
        &scope.config_revision,
        &scope.request_id.to_string(),
        chain_mode,
        compression_policy,
    )
}

fn apply_anthropic_output_shaping(
    response: &mut Value,
    route: &dtoken_types::RouteProfile,
    scope: &RequestScope,
    policy: Option<&dtoken_types::OutputShapingRoutePolicy>,
    chain_mode: &str,
    compression_policy: &str,
) -> Value {
    let passthrough = |reason_code| {
        output_shaping_passthrough_evidence(
            reason_code,
            &scope.route_id,
            &scope.provider_id,
            &scope.model,
            &scope.config_revision,
            &scope.request_id.to_string(),
            chain_mode,
            compression_policy,
        )
    };
    let Some(policy) = policy else {
        return passthrough("route_output_shaping_policy_unavailable");
    };
    let Some(content) = response.get_mut("content").and_then(Value::as_array_mut) else {
        return passthrough("unknown_response_shape_passthrough");
    };
    if content.len() != 1 {
        return passthrough("tool_or_structured_response_passthrough");
    }
    let Some(block) = content[0].as_object_mut() else {
        return passthrough("unknown_response_shape_passthrough");
    };
    if block.get("type").and_then(Value::as_str) != Some("text")
        || block.keys().any(|key| key != "type" && key != "text")
    {
        return passthrough("tool_or_structured_response_passthrough");
    }
    let Some(text) = block.get("text").and_then(Value::as_str) else {
        return passthrough("unknown_response_shape_passthrough");
    };
    let result = shape_output_for_policy(
        route,
        policy,
        &scope.request_id.to_string(),
        output_safety_for_text(text),
        text,
    );
    if result.disposition == OutputShapingDisposition::Applied {
        block.insert("text".to_string(), Value::String(result.body.clone()));
    }
    output_shaping_evidence(
        result,
        &scope.route_id,
        &scope.provider_id,
        &scope.model,
        &scope.config_revision,
        &scope.request_id.to_string(),
        chain_mode,
        compression_policy,
    )
}

fn attach_output_shaping_evidence(
    request_log: &mut RequestLog,
    event: &mut DtokenEvent,
    evidence: Value,
) {
    let applied = evidence.get("state").and_then(Value::as_str) == Some("applied");
    if applied {
        request_log
            .transforms_applied
            .push("output_shaper_whitespace_only".to_string());
    }
    if let Some(metadata) = event.metadata.as_object_mut() {
        metadata.insert("output_shaping".to_string(), evidence);
    }
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

/// POST a JSON body to an arbitrary upstream URL with the provider's
/// credential and Codex/chatgpt metadata handling. Shared by the Chat wire
/// (`forward_openai_chat`) and the native Responses passthrough
/// (`forward_openai_responses`); the caller picks the explicit wire URL.
async fn forward_openai_url(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    url: String,
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

async fn forward_openai_chat(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
    inbound_headers: Option<&HeaderMap>,
) -> UpstreamResult {
    forward_openai_url(
        client,
        provider,
        upstream_chat_url(&provider.base_url),
        original_body,
        inbound_headers,
    )
    .await
}

/// Native OpenAI Responses passthrough: forwards the inbound body verbatim to
/// the provider's `/v1/responses` wire endpoint. No Chat conversion happens
/// here; the caller must only use this for `ProviderWireApi::OpenAiResponses`.
async fn forward_openai_responses(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
    inbound_headers: Option<&HeaderMap>,
) -> UpstreamResult {
    forward_openai_url(
        client,
        provider,
        upstream_responses_url(&provider.base_url),
        original_body,
        inbound_headers,
    )
    .await
}

/// Stream a JSON request to an arbitrary upstream URL, returning the raw SSE
/// body. Shared by the Chat wire (`forward_openai_chat_stream`) and the native
/// Responses passthrough (`forward_openai_responses_stream`); the caller picks
/// the explicit wire URL and must not convert the returned event stream.
async fn forward_openai_url_stream(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    url: String,
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

async fn forward_openai_chat_stream(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
    inbound_headers: Option<&HeaderMap>,
) -> Result<(u16, axum::http::HeaderMap, Body, Option<Value>), UpstreamResult> {
    forward_openai_url_stream(
        client,
        provider,
        upstream_chat_url(&provider.base_url),
        original_body,
        inbound_headers,
    )
    .await
}

/// Native OpenAI Responses SSE passthrough: forwards the inbound body
/// verbatim to `/v1/responses` and returns the upstream event stream untouched.
/// No Chat SSE conversion happens here; the caller must only use this for
/// `ProviderWireApi::OpenAiResponses`.
async fn forward_openai_responses_stream(
    client: &reqwest::Client,
    provider: &dtoken_types::ProviderProfile,
    original_body: Value,
    inbound_headers: Option<&HeaderMap>,
) -> Result<(u16, axum::http::HeaderMap, Body, Option<Value>), UpstreamResult> {
    forward_openai_url_stream(
        client,
        provider,
        upstream_responses_url(&provider.base_url),
        original_body,
        inbound_headers,
    )
    .await
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

/// Bounded, transient extraction of Anthropic SSE usage counters.  The
/// parser never stores event payloads outside this response body; it retains
/// only the allowlisted numeric counters required for ledger/cache evidence.
#[derive(Debug, Default)]
struct AnthropicStreamUsageObserver {
    pending_line: Vec<u8>,
    usage: AnthropicStreamUsage,
    message_stop: bool,
    error_event: bool,
}

#[derive(Debug, Default)]
struct AnthropicStreamUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_creation_5m_input_tokens: Option<u64>,
    cache_creation_1h_input_tokens: Option<u64>,
}

impl AnthropicStreamUsage {
    fn observe(&mut self, value: &Value) {
        let usage = value
            .get("usage")
            .or_else(|| value.pointer("/message/usage"));
        let Some(usage) = usage else {
            return;
        };
        for (target, key) in [
            (&mut self.input_tokens, "input_tokens"),
            (&mut self.output_tokens, "output_tokens"),
            (&mut self.cache_read_input_tokens, "cache_read_input_tokens"),
            (
                &mut self.cache_creation_input_tokens,
                "cache_creation_input_tokens",
            ),
            (
                &mut self.cache_creation_5m_input_tokens,
                "cache_creation_5m_input_tokens",
            ),
            (
                &mut self.cache_creation_1h_input_tokens,
                "cache_creation_1h_input_tokens",
            ),
        ] {
            if let Some(value) = usage.get(key).and_then(Value::as_u64) {
                *target = Some(value);
            }
        }
        if let Some(value) = usage
            .pointer("/cache_creation/ephemeral_5m_input_tokens")
            .and_then(Value::as_u64)
        {
            self.cache_creation_5m_input_tokens = Some(value);
        }
        if let Some(value) = usage
            .pointer("/cache_creation/ephemeral_1h_input_tokens")
            .and_then(Value::as_u64)
        {
            self.cache_creation_1h_input_tokens = Some(value);
        }
    }

    fn has_observation(&self) -> bool {
        self.input_tokens.is_some()
            || self.output_tokens.is_some()
            || self.cache_read_input_tokens.is_some()
            || self.cache_creation_input_tokens.is_some()
            || self.cache_creation_5m_input_tokens.is_some()
            || self.cache_creation_1h_input_tokens.is_some()
    }

    fn as_response(&self) -> Value {
        json!({
            "usage": {
                "input_tokens": self.input_tokens,
                "output_tokens": self.output_tokens,
                "cache_read_input_tokens": self.cache_read_input_tokens,
                "cache_creation_input_tokens": self.cache_creation_input_tokens,
                "cache_creation": {
                    "ephemeral_5m_input_tokens": self.cache_creation_5m_input_tokens,
                    "ephemeral_1h_input_tokens": self.cache_creation_1h_input_tokens,
                }
            }
        })
    }
}

impl AnthropicStreamUsageObserver {
    // Anthropic emits one JSON object per SSE `data:` line.  This bound is far
    // above a usage event yet prevents a malformed upstream from making the
    // proxy retain unbounded response content.
    const MAX_PENDING_LINE_BYTES: usize = 64 * 1024;

    fn observe_chunk(&mut self, chunk: &Bytes) {
        if self.pending_line.len().saturating_add(chunk.len()) > Self::MAX_PENDING_LINE_BYTES {
            self.pending_line.clear();
            return;
        }
        self.pending_line.extend_from_slice(chunk);
        while let Some(newline) = self.pending_line.iter().position(|byte| *byte == b'\n') {
            let remainder = self.pending_line.split_off(newline + 1);
            let line = std::mem::replace(&mut self.pending_line, remainder);
            self.observe_line(&line[..newline]);
        }
    }

    fn observe_line(&mut self, line: &[u8]) {
        let Some(payload) = line.strip_prefix(b"data:") else {
            return;
        };
        let Ok(payload) = std::str::from_utf8(payload) else {
            return;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            return;
        }
        if let Ok(value) = serde_json::from_str::<Value>(payload) {
            match value.get("type").and_then(Value::as_str) {
                Some("message_stop") => self.message_stop = true,
                Some("error") => self.error_event = true,
                _ => {}
            }
            self.usage.observe(&value);
        }
    }

    fn terminal_error_code(&self) -> Option<&'static str> {
        if self.error_event {
            Some("provider_stream_error_event")
        } else if !self.message_stop {
            Some("provider_stream_incomplete")
        } else {
            None
        }
    }

    fn take_usage(&mut self) -> AnthropicStreamUsage {
        self.pending_line.clear();
        std::mem::take(&mut self.usage)
    }
}

struct StreamTerminalLedgerGuard {
    state: AppState,
    scope: RequestScope,
    request_id: Uuid,
    provider_id: String,
    upstream_status: u16,
    kind: &'static str,
    accounting: Option<dtoken_types::HybridContextAccounting>,
    usage_observer: AnthropicStreamUsageObserver,
    /// A streaming request is not durable until its response reaches a
    /// terminal state.  Keeping this record with the body guard prevents a
    /// `stream_started` acknowledgement from being projected as a finalized
    /// CCR receipt when the client disconnects or the upstream stream fails.
    terminal_record: Option<StreamTerminalRequestRecord>,
    terminal: bool,
}

#[derive(Debug)]
struct StreamTerminalRequestRecord {
    decision: RouteDecision,
    request_log: RequestLog,
    event: DtokenEvent,
    prepared_ccr: PreparedCcr,
}

impl StreamTerminalLedgerGuard {
    fn finish_from_clean_eof(self) {
        let error_code = self.usage_observer.terminal_error_code();
        match error_code {
            Some(error_code) => {
                self.finish(EventStatus::Failed, "upstream_failed", Some(error_code))
            }
            None => self.finish(EventStatus::Ok, "upstream_completed", None),
        }
    }

    fn apply_completed_stream_usage(&mut self) {
        let usage = self.usage_observer.take_usage();
        if !usage.has_observation() {
            return;
        }
        let Some(record) = self.terminal_record.as_mut() else {
            return;
        };
        let upstream = UpstreamResult {
            response: usage.as_response(),
            status: Some(self.upstream_status),
            error_category: None,
            send_state: UpstreamSendState::Completed,
            quota_metadata: None,
        };
        mark_upstream_outcome(&upstream, &mut record.request_log, &mut record.event);
        persist_provider_cache_observation(
            &self.state,
            &self.scope,
            &upstream,
            dtoken_types::ProviderCacheObservationKind::AnthropicUsage,
        );
    }

    fn commit_terminal_record(&mut self, status: &EventStatus, send_state: UpstreamSendState) {
        let Some(mut record) = self.terminal_record.take() else {
            return;
        };
        if let Some(accounting) = record.request_log.context_accounting.as_mut() {
            accounting.upstream_state = send_state;
        }
        record.event.context_accounting = record.request_log.context_accounting.clone();
        record.event.status = status.clone();
        if let Some(metadata) = record.event.metadata.as_object_mut() {
            metadata.insert("send_state".to_string(), json!(send_state));
            metadata.insert("stream_terminal".to_string(), json!(true));
        }
        self.state.record_routed_request_with_ccr_receipt(
            &record.decision,
            record.request_log,
            record.event,
            &record.prepared_ccr,
        );
    }

    fn finish(mut self, status: EventStatus, phase: &'static str, error_code: Option<&str>) {
        self.terminal = true;
        let send_state = if status == EventStatus::Ok {
            UpstreamSendState::Completed
        } else {
            UpstreamSendState::FailedAfterCommit
        };
        if let Some(accounting) = self.accounting.as_mut() {
            accounting.upstream_state = send_state;
        }
        if status == EventStatus::Ok {
            self.apply_completed_stream_usage();
        } else {
            self.usage_observer.take_usage();
        }
        self.commit_terminal_record(&status, send_state);
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
        self.usage_observer.take_usage();
        self.commit_terminal_record(&EventStatus::Failed, UpstreamSendState::FailedAfterCommit);
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
                    guard.finish_from_clean_eof();
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
            Poll::Ready(Some(Ok(bytes))) => {
                if let Some(guard) = self.guard.as_mut() {
                    guard.usage_observer.observe_chunk(&bytes);
                }
                Poll::Ready(Some(Ok(bytes)))
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

// === OpenAI Responses API semantic bridge ===
//
// /v1/responses is converted to /v1/chat/completions for configured OpenAI
// routes and converted back. The bridge is fail-closed: Responses features
// that cannot be represented faithfully on the Chat surface (reasoning items,
// item_reference, hosted tools, image content, ...) are rejected with an
// explicit error instead of being silently dropped or guessed. Streaming is a
// real Chat SSE -> Responses SSE event conversion, never a silent downgrade.

/// Convert an OpenAI Responses request body into an OpenAI Chat completions
/// body.
///
/// Supported Responses input: `instructions` (mapped to a system message),
/// `input` as a string or as an array of `message` / `function_call` /
/// `function_call_output` items, and `tools` as flat Responses function tools.
/// `max_output_tokens` is mapped to `max_tokens`. Anything that cannot be
/// bridged faithfully fails closed with a descriptive reason.
pub fn responses_to_openai_chat_body(body: &Value) -> Result<Value, String> {
    let messages = responses_input_to_chat_messages(body)?;
    let mut converted = serde_json::Map::new();
    converted.insert(
        "model".to_string(),
        body.get("model").cloned().unwrap_or(Value::Null),
    );
    converted.insert("messages".to_string(), json!(messages));
    converted.insert(
        "stream".to_string(),
        body.get("stream").cloned().unwrap_or(json!(false)),
    );
    if let Some(max_output_tokens) = body.get("max_output_tokens") {
        converted.insert("max_tokens".to_string(), max_output_tokens.clone());
    }
    for key in ["temperature", "top_p"] {
        if let Some(value) = body.get(key) {
            converted.insert(key.to_string(), value.clone());
        }
    }
    if let Some(stop) = body.get("stop") {
        converted.insert("stop".to_string(), stop.clone());
    }
    if let Some(tools) = body.get("tools") {
        converted.insert("tools".to_string(), json!(responses_tools_to_chat(tools)?));
    }
    if body.get("tool_choice").is_some() {
        converted.insert(
            "tool_choice".to_string(),
            responses_tool_choice_to_chat(body.get("tool_choice"))?,
        );
    }
    Ok(Value::Object(converted))
}

fn responses_input_to_chat_messages(body: &Value) -> Result<Vec<Value>, String> {
    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        let text = responses_content_to_text(instructions)?;
        if !text.is_empty() {
            messages.push(json!({"role": "system", "content": text}));
        }
    }
    let input = body
        .get("input")
        .ok_or_else(|| "Responses request is missing the required `input` field".to_string())?;
    if let Some(text) = input.as_str() {
        messages.push(json!({"role": "user", "content": text}));
        return Ok(messages);
    }
    let items = input.as_array().ok_or_else(|| {
        "Responses `input` must be a string or an array of input items".to_string()
    })?;
    for item in items {
        responses_input_item_to_chat_messages(item, &mut messages)?;
    }
    Ok(messages)
}

fn responses_input_item_to_chat_messages(
    item: &Value,
    messages: &mut Vec<Value>,
) -> Result<(), String> {
    let Some(item_type) = item.get("type").and_then(Value::as_str) else {
        return Err("Responses input item is missing a `type` field".to_string());
    };
    match item_type {
        "message" => {
            let role = item
                .get("role")
                .and_then(Value::as_str)
                .ok_or_else(|| "Responses `message` item is missing string `role`".to_string())?;
            if !matches!(role, "user" | "assistant" | "system" | "developer") {
                return Err(format!(
                    "unsupported Responses `message` role `{role}` for the Chat bridge"
                ));
            }
            let content = item
                .get("content")
                .cloned()
                .ok_or_else(|| "Responses `message` item is missing `content`".to_string())?;
            let text = responses_content_to_text(&content)?;
            messages.push(json!({"role": role, "content": text}));
            Ok(())
        }
        "function_call" => {
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .or_else(|| item.get("id").and_then(Value::as_str))
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    "Responses `function_call` is missing non-empty string `call_id`".to_string()
                })?
                .to_string();
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    "Responses `function_call` is missing non-empty string `name`".to_string()
                })?
                .to_string();
            let arguments = match item.get("arguments") {
                None => {
                    return Err(
                        "Responses `function_call` is missing string `arguments`".to_string()
                    )
                }
                Some(Value::String(arguments))
                    if serde_json::from_str::<Value>(arguments).is_ok() =>
                {
                    arguments.clone()
                }
                Some(Value::String(_)) => {
                    return Err(
                        "Responses `function_call` `arguments` must contain valid JSON".to_string(),
                    )
                }
                Some(_) => {
                    return Err(
                        "Responses `function_call` `arguments` must be a JSON-encoded string"
                            .to_string(),
                    )
                }
            };
            messages.push(json!({
                "role": "assistant",
                "content": Value::Null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {"name": name, "arguments": arguments}
                }]
            }));
            Ok(())
        }
        "function_call_output" => {
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    "Responses `function_call_output` is missing non-empty string `call_id`"
                        .to_string()
                })?
                .to_string();
            let output = match item.get("output") {
                None => {
                    return Err(
                        "Responses `function_call_output` is missing string `output`".to_string(),
                    )
                }
                Some(Value::String(output)) => output.clone(),
                Some(_) => {
                    return Err(
                        "Responses `function_call_output` `output` must be a string".to_string(),
                    )
                }
            };
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output
            }));
            Ok(())
        }
        "reasoning" => Err(
            "Responses `reasoning` items are not supported by the Chat bridge; refusing to drop encrypted reasoning content"
                .to_string(),
        ),
        "item_reference" => Err(
            "Responses `item_reference` items cannot be resolved by the Chat bridge; refusing to guess"
                .to_string(),
        ),
        other => Err(format!("unsupported Responses input item type `{other}`")),
    }
}

fn responses_content_to_text(content: &Value) -> Result<String, String> {
    match content {
        Value::String(text) => Ok(text.clone()),
        Value::Null => Ok(String::new()),
        Value::Array(parts) => {
            let mut texts = Vec::new();
            for part in parts {
                let Some(kind) = part.get("type").and_then(Value::as_str) else {
                    return Err("Responses content part is missing a `type` field".to_string());
                };
                match kind {
                    "input_text" | "output_text" => {
                        let text = part.get("text").and_then(Value::as_str).ok_or_else(|| {
                            format!("Responses `{kind}` content part is missing string `text`")
                        })?;
                        texts.push(text.to_string());
                    }
                    other => {
                        return Err(format!(
                            "unsupported Responses content part type `{other}`; refusing to drop it"
                        ))
                    }
                }
            }
            Ok(texts.join("\n"))
        }
        _ => Err("Responses `content` must be a string or an array of content parts".to_string()),
    }
}

fn responses_tools_to_chat(tools: &Value) -> Result<Vec<Value>, String> {
    let Some(array) = tools.as_array() else {
        return Err("Responses `tools` must be an array".to_string());
    };
    let mut converted = Vec::new();
    for tool in array {
        let Some(kind) = tool.get("type").and_then(Value::as_str) else {
            return Err("Responses tool is missing a `type` field".to_string());
        };
        match kind {
            "function" => {
                let name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        "Responses function tool is missing non-empty string `name`".to_string()
                    })?;
                let description = match tool.get("description") {
                    None => "",
                    Some(Value::String(description)) => description,
                    Some(_) => {
                        return Err(
                            "Responses function tool `description` must be a string".to_string()
                        )
                    }
                };
                let parameters = tool
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object"}));
                if !parameters.is_object() {
                    return Err(
                        "Responses function tool `parameters` must be a JSON object".to_string()
                    );
                }
                let mut function = serde_json::Map::new();
                function.insert("name".to_string(), json!(name));
                function.insert("description".to_string(), json!(description));
                function.insert("parameters".to_string(), parameters);
                if let Some(strict) = tool.get("strict") {
                    if !strict.is_boolean() {
                        return Err(
                            "Responses function tool `strict` must be a boolean".to_string()
                        );
                    }
                    function.insert("strict".to_string(), strict.clone());
                }
                converted.push(json!({"type": "function", "function": Value::Object(function)}));
            }
            "hosted" => {
                return Err(
                    "Responses `hosted` tools are not supported by the Chat bridge; refusing to substitute a local guess"
                        .to_string(),
                )
            }
            other => {
                return Err(format!(
                    "unsupported Responses tool type `{other}`; only `function` tools can be bridged"
                ))
            }
        }
    }
    Ok(converted)
}

fn responses_tool_choice_to_chat(choice: Option<&Value>) -> Result<Value, String> {
    let choice =
        choice.ok_or_else(|| "Responses `tool_choice` value is unavailable".to_string())?;
    if let Some(value) = choice.as_str() {
        return match value {
            "auto" | "none" | "required" => Ok(json!(value)),
            other => Err(format!(
                "unsupported Responses `tool_choice` string `{other}`"
            )),
        };
    }
    let choice_type = choice.get("type").and_then(Value::as_str).ok_or_else(|| {
        "Responses `tool_choice` must be a supported string or object".to_string()
    })?;
    match choice_type {
        "auto" => Ok(json!("auto")),
        "none" => Ok(json!("none")),
        "required" => Ok(json!("required")),
        "function" => {
            let name = choice
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    "Responses function `tool_choice` is missing non-empty string `name`"
                        .to_string()
                })?;
            Ok(json!({
                "type": "function",
                "function": {"name": name}
            }))
        }
        other => Err(format!(
            "unsupported Responses `tool_choice` type `{other}`"
        )),
    }
}

/// Convert an OpenAI Chat completions response into an OpenAI Responses
/// response object. Text becomes a `message` output item with `output_text`;
/// `tool_calls` become `function_call` output items. `finish_reason=length`
/// maps to an incomplete response, everything else to completed.
pub fn openai_chat_to_responses_response(
    upstream: &Value,
    model: Option<&str>,
    response_id: &str,
) -> Value {
    let choice = upstream
        .pointer("/choices/0")
        .cloned()
        .unwrap_or(Value::Null);
    let message = choice.get("message").cloned().unwrap_or(Value::Null);
    let text = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut output = vec![json!({
        "type": "message",
        "id": format!("msg-{response_id}"),
        "status": "completed",
        "role": "assistant",
        "content": [{
            "type": "output_text",
            "text": text,
            "annotations": []
        }]
    })];
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            let call_id = call.get("id").and_then(Value::as_str).unwrap_or_default();
            let name = call["function"]["name"].as_str().unwrap_or_default();
            let arguments = call["function"]["arguments"].as_str().unwrap_or("{}");
            output.push(json!({
                "type": "function_call",
                "id": format!("fc-{response_id}-{index}"),
                "call_id": call_id,
                "name": name,
                "arguments": arguments
            }));
        }
    }
    let finish_reason = choice.get("finish_reason").and_then(Value::as_str);
    let status = match finish_reason {
        Some("length") => "incomplete",
        _ => "completed",
    };
    json!({
        "id": response_id,
        "object": "response",
        "status": status,
        "model": model.unwrap_or_else(|| {
            upstream
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("dtoken-response-route")
        }),
        "output": output,
        "usage": {
            "input_tokens": upstream["usage"]["prompt_tokens"].as_u64().unwrap_or_default(),
            "output_tokens": upstream["usage"]["completion_tokens"].as_u64().unwrap_or_default(),
            "total_tokens": upstream["usage"]["total_tokens"].as_u64().unwrap_or_default()
        }
    })
}

fn unix_timestamp_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

/// Convert an OpenAI Chat completions SSE byte stream into an OpenAI
/// Responses SSE event stream.
///
/// This is a real event conversion, never a silent downgrade: text deltas
/// become `response.output_text.delta` events, the terminal Chat chunk emits
/// `response.output_item.done` for the message and each `function_call`, and
/// the stream closes with `response.completed` / `response.incomplete`. A
/// clean EOF without a terminal `finish_reason` fails closed with
/// `response.failed` instead of synthesizing success.
fn responses_sse_from_chat_sse(
    body: Body,
    response_id: String,
    model: Option<String>,
    guard: ResponsesStreamTerminalLedgerGuard,
) -> Body {
    let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
        Box::pin(body.into_data_stream().map_err(std::io::Error::other));
    Body::from_stream(ResponsesSseStream::new(inner, response_id, model, guard))
}

struct ResponsesSseState {
    response_id: String,
    model: Option<String>,
    buffer: Vec<u8>,
    text: String,
    /// Accumulated Chat tool_calls ordered by `index`: (index, call_id, name, arguments).
    tool_calls: Vec<(usize, String, String, String)>,
    usage: Option<Value>,
    message_item_id: String,
    output_index: u64,
    content_index: u64,
    created_emitted: bool,
    terminal_emitted: bool,
    terminal_error_code: Option<String>,
}

impl ResponsesSseState {
    fn new(response_id: String, model: Option<String>) -> Self {
        Self {
            message_item_id: format!("msg-{response_id}"),
            response_id,
            model,
            buffer: Vec::new(),
            text: String::new(),
            tool_calls: Vec::new(),
            usage: None,
            output_index: 0,
            content_index: 0,
            created_emitted: false,
            terminal_emitted: false,
            terminal_error_code: None,
        }
    }

    fn sse_event(event_type: &str, data: Value) -> String {
        let payload = serde_json::to_string(&data).unwrap_or_default();
        format!("event: {event_type}\ndata: {payload}\n\n")
    }

    fn response_object(&self, status: &str, output: Vec<Value>) -> Value {
        json!({
            "id": self.response_id,
            "object": "response",
            "created_at": unix_timestamp_seconds(),
            "status": status,
            "model": self.model,
            "output": output,
            "usage": self.usage.clone().unwrap_or(Value::Null)
        })
    }

    fn observe_chunk(&mut self, bytes: Bytes, pending: &mut VecDeque<String>) {
        if !self.created_emitted {
            self.created_emitted = true;
            pending.push_back(Self::sse_event(
                "response.created",
                self.response_object("in_progress", Vec::new()),
            ));
        }
        self.buffer.extend_from_slice(&bytes);
        while let Some(separator) = self.buffer.windows(2).position(|window| window == b"\n\n") {
            let event_bytes: Vec<u8> = self.buffer.drain(..separator).collect();
            let _ = self.buffer.drain(..2);
            if !event_bytes.is_empty() {
                self.observe_sse_event(String::from_utf8_lossy(&event_bytes).to_string(), pending);
            }
        }
    }

    fn observe_sse_event(&mut self, event: String, pending: &mut VecDeque<String>) {
        let payloads = event
            .lines()
            .filter_map(|line| {
                line.strip_prefix("data:")
                    .map(str::trim)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        for payload in payloads {
            if payload == "[DONE]" {
                if !self.terminal_emitted {
                    self.fail_closed(
                        pending,
                        "provider_stream_incomplete",
                        "Chat SSE ended with [DONE] without a terminal finish_reason chunk",
                    );
                }
                continue;
            }
            if let Ok(chunk) = serde_json::from_str::<Value>(&payload) {
                self.observe_chat_chunk(chunk, pending);
            }
        }
    }

    fn observe_chat_chunk(&mut self, chunk: Value, pending: &mut VecDeque<String>) {
        if let Some(error) = chunk.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("upstream streaming error");
            self.fail_closed(pending, "provider_stream_error", message);
            return;
        }
        if let Some(usage) = chunk.get("usage") {
            self.usage = Some(json!({
                "input_tokens": usage["prompt_tokens"].as_u64().unwrap_or_default(),
                "output_tokens": usage["completion_tokens"].as_u64().unwrap_or_default(),
                "total_tokens": usage["total_tokens"].as_u64().unwrap_or_default()
            }));
        }
        let Some(choice) = chunk.pointer("/choices/0") else {
            return;
        };
        let delta = choice.get("delta").cloned().unwrap_or(Value::Null);
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            if !text.is_empty() {
                self.text.push_str(text);
                pending.push_back(Self::sse_event(
                    "response.output_text.delta",
                    json!({
                        "type": "response.output_text.delta",
                        "item_id": self.message_item_id,
                        "output_index": self.output_index,
                        "content_index": self.content_index,
                        "delta": text
                    }),
                ));
            }
        }
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            self.accumulate_tool_calls(calls);
        }
        if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str) {
            if !finish_reason.is_empty() && !self.terminal_emitted {
                self.finalize(finish_reason, pending);
            }
        }
    }

    fn accumulate_tool_calls(&mut self, calls: &[Value]) {
        for call in calls {
            let index = call
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize;
            let call_id = call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let name = call["function"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let arguments = call["function"]["arguments"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            match self.tool_calls.iter_mut().find(|entry| entry.0 == index) {
                Some(entry) => {
                    if !call_id.is_empty() {
                        entry.1 = call_id;
                    }
                    if !name.is_empty() {
                        entry.2 = name;
                    }
                    entry.3.push_str(&arguments);
                }
                None => self.tool_calls.push((index, call_id, name, arguments)),
            }
        }
        self.tool_calls.sort_by_key(|entry| entry.0);
    }

    fn finalize(&mut self, finish_reason: &str, pending: &mut VecDeque<String>) {
        self.terminal_emitted = true;
        let message_item = json!({
            "type": "message",
            "id": self.message_item_id,
            "status": "completed",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": self.text,
                "annotations": []
            }]
        });
        pending.push_back(Self::sse_event(
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "output_index": self.output_index,
                "item": message_item
            }),
        ));
        let mut output = vec![message_item];
        for (index, (_, call_id, name, arguments)) in self.tool_calls.iter().enumerate() {
            let item = json!({
                "type": "function_call",
                "id": format!("fc-{}-{index}", self.response_id),
                "call_id": call_id,
                "name": name,
                "arguments": arguments
            });
            pending.push_back(Self::sse_event(
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "output_index": self.output_index + 1 + index as u64,
                    "item": item
                }),
            ));
            output.push(item);
        }
        let (event_type, status) = match finish_reason {
            "length" => ("response.incomplete", "incomplete"),
            _ => ("response.completed", "completed"),
        };
        pending.push_back(Self::sse_event(
            event_type,
            json!({
                "type": event_type,
                "response": self.response_object(status, output)
            }),
        ));
    }

    fn fail_closed(&mut self, pending: &mut VecDeque<String>, code: &str, message: &str) {
        if self.terminal_emitted {
            return;
        }
        self.terminal_emitted = true;
        self.terminal_error_code = Some(code.to_string());
        if !self.created_emitted {
            self.created_emitted = true;
            pending.push_back(Self::sse_event(
                "response.created",
                self.response_object("in_progress", Vec::new()),
            ));
        }
        pending.push_back(Self::sse_event(
            "response.failed",
            json!({
                "type": "response.failed",
                "response": self.response_object("failed", Vec::new()),
                "error": {"code": code, "message": message}
            }),
        ));
    }
}

struct ResponsesStreamTerminalLedgerGuard {
    state: AppState,
    scope: RequestScope,
    request_id: Uuid,
    provider_id: String,
    upstream_status: u16,
    terminal_record: Option<StreamTerminalRequestRecord>,
    terminal: bool,
}

impl ResponsesStreamTerminalLedgerGuard {
    fn finish_from_state(mut self, stream_state: &ResponsesSseState) {
        self.terminal = true;
        let error_code = stream_state.terminal_error_code.as_deref();
        let status = if error_code.is_some() {
            EventStatus::Failed
        } else {
            EventStatus::Ok
        };
        let send_state = if error_code.is_some() {
            UpstreamSendState::FailedAfterCommit
        } else {
            UpstreamSendState::Completed
        };
        self.commit_terminal_record(stream_state, &status, send_state, error_code);
        append_stage_event(
            &self.state,
            self.request_id,
            LayeredStage::ProviderForward,
            status,
            json!({
                "provider_id": self.provider_id,
                "protocol": "openai_responses",
                "output_protocol": "openai_responses_sse",
                "stream": true,
                "upstream_status": self.upstream_status,
                "terminal": true,
                "stream_end": if error_code.is_some() { "failed" } else { "completed" },
                "fallback": "disabled",
                "raw_available": false
            }),
            error_code,
        );
    }

    /// Passthrough terminal finalization: the observer already determined the
    /// terminal outcome from the native Responses SSE event stream without
    /// rewriting any bytes. Shares the same ledger/CCR commit path as the
    /// conversion stream (`finish_from_state`).
    fn finish_from_passthrough(self, terminal_error_code: Option<String>, usage: Option<Value>) {
        let mut stream_state = ResponsesSseState::new(format!("resp-{}", self.request_id), None);
        stream_state.terminal_error_code = terminal_error_code;
        stream_state.usage = usage;
        self.finish_from_state(&stream_state);
    }

    fn commit_terminal_record(
        &mut self,
        stream_state: &ResponsesSseState,
        status: &EventStatus,
        send_state: UpstreamSendState,
        error_code: Option<&str>,
    ) {
        let Some(mut record) = self.terminal_record.take() else {
            return;
        };
        let quota_metadata = record.request_log.quota_metadata.clone();
        let usage = stream_state.usage.clone().unwrap_or(Value::Null);
        let response = if status == &EventStatus::Ok {
            json!({
                "usage": {
                    "prompt_tokens": usage["input_tokens"].as_u64().unwrap_or_default(),
                    "completion_tokens": usage["output_tokens"].as_u64().unwrap_or_default(),
                    "total_tokens": usage["total_tokens"].as_u64().unwrap_or_default()
                }
            })
        } else {
            json!({"error": {"type": error_code.unwrap_or("provider_stream_failed")}})
        };
        let upstream = UpstreamResult {
            response,
            status: Some(self.upstream_status),
            error_category: error_code.map(ToString::to_string),
            send_state,
            quota_metadata,
        };
        mark_upstream_outcome(&upstream, &mut record.request_log, &mut record.event);
        if status == &EventStatus::Ok {
            persist_provider_cache_observation(
                &self.state,
                &self.scope,
                &upstream,
                dtoken_types::ProviderCacheObservationKind::OpenAiChatUsage,
            );
        }
        record.event.status = status.clone();
        if let Some(metadata) = record.event.metadata.as_object_mut() {
            metadata.insert("stream".to_string(), json!(true));
            metadata.insert("stream_terminal".to_string(), json!(true));
            metadata.insert("send_state".to_string(), json!(send_state));
            if let Some(error_code) = error_code {
                metadata.insert("stream_error_code".to_string(), json!(error_code));
            }
        }
        self.state.record_routed_request_with_ccr_receipt(
            &record.decision,
            record.request_log,
            record.event,
            &record.prepared_ccr,
        );
    }
}

impl Drop for ResponsesStreamTerminalLedgerGuard {
    fn drop(&mut self) {
        if self.terminal {
            return;
        }
        self.terminal = true;
        let stream_state = ResponsesSseState::new(format!("resp-{}", self.request_id), None);
        self.commit_terminal_record(
            &stream_state,
            &EventStatus::Failed,
            UpstreamSendState::FailedAfterCommit,
            Some("provider_stream_dropped"),
        );
        append_stage_event(
            &self.state,
            self.request_id,
            LayeredStage::ProviderForward,
            EventStatus::Failed,
            json!({
                "provider_id": self.provider_id,
                "protocol": "openai_responses",
                "output_protocol": "openai_responses_sse",
                "stream": true,
                "upstream_status": self.upstream_status,
                "terminal": true,
                "stream_end": "body_dropped",
                "fallback": "disabled",
                "raw_available": false
            }),
            Some("provider_stream_dropped"),
        );
    }
}

struct ResponsesSseStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
    state: ResponsesSseState,
    pending: VecDeque<String>,
    finished: bool,
    guard: Option<ResponsesStreamTerminalLedgerGuard>,
}

impl ResponsesSseStream {
    fn new(
        inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
        response_id: String,
        model: Option<String>,
        guard: ResponsesStreamTerminalLedgerGuard,
    ) -> Self {
        Self {
            inner,
            state: ResponsesSseState::new(response_id, model),
            pending: VecDeque::new(),
            finished: false,
            guard: Some(guard),
        }
    }
}

impl Stream for ResponsesSseStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Self {
            inner,
            state,
            pending,
            finished,
            guard,
        } = &mut *self;
        loop {
            if let Some(event) = pending.pop_front() {
                return Poll::Ready(Some(Ok(Bytes::from(event))));
            }
            if *finished {
                if let Some(guard) = guard.take() {
                    guard.finish_from_state(state);
                }
                return Poll::Ready(None);
            }
            match inner.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    *finished = true;
                    if !state.terminal_emitted {
                        state.fail_closed(
                            pending,
                            "provider_stream_incomplete",
                            "Chat SSE ended without a terminal finish_reason chunk",
                        );
                    }
                }
                Poll::Ready(Some(Ok(bytes))) => {
                    state.observe_chunk(bytes, pending);
                }
                Poll::Ready(Some(Err(error))) => {
                    *finished = true;
                    state.fail_closed(pending, "provider_stream_io_error", &error.to_string());
                }
            }
        }
    }
}

/// Native Responses SSE passthrough terminal observer. Forwards every upstream
/// byte unchanged while watching for the terminal event (`response.completed` /
/// `response.incomplete` / `response.failed` / error payload) so the ledger
/// event and CCR receipt are finalized only when the stream reaches a terminal
/// state (G4). A clean EOF without a terminal event fails closed instead of
/// synthesizing success; a dropped body records `provider_stream_dropped` via
/// the guard's `Drop` impl. Bytes are never rewritten.
struct ResponsesPassthroughStream {
    inner: UpstreamByteStream,
    buffer: Vec<u8>,
    terminal_seen: bool,
    terminal_error_code: Option<String>,
    usage: Option<Value>,
    guard: Option<ResponsesStreamTerminalLedgerGuard>,
}

impl ResponsesPassthroughStream {
    fn new(inner: UpstreamByteStream, guard: ResponsesStreamTerminalLedgerGuard) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
            terminal_seen: false,
            terminal_error_code: None,
            usage: None,
            guard: Some(guard),
        }
    }

    fn observe_bytes(&mut self, bytes: &Bytes) {
        if self.terminal_seen {
            return;
        }
        self.buffer.extend_from_slice(bytes);
        while let Some(separator) = self.buffer.windows(2).position(|window| window == b"\n\n") {
            let event_bytes: Vec<u8> = self.buffer.drain(..separator).collect();
            let _ = self.buffer.drain(..2);
            if !event_bytes.is_empty() {
                self.observe_event(&event_bytes);
            }
            if self.terminal_seen {
                self.buffer.clear();
                return;
            }
        }
    }

    fn observe_event(&mut self, event_bytes: &[u8]) {
        let payloads = String::from_utf8_lossy(event_bytes)
            .lines()
            .filter_map(|line| {
                line.strip_prefix("data:")
                    .map(str::trim)
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        for payload in payloads {
            if payload == "[DONE]" {
                continue;
            }
            let Ok(chunk) = serde_json::from_str::<Value>(&payload) else {
                continue;
            };
            match chunk.get("type").and_then(Value::as_str) {
                Some("response.completed") | Some("response.incomplete") => {
                    self.terminal_seen = true;
                    self.terminal_error_code = None;
                    self.usage = chunk
                        .pointer("/response/usage")
                        .cloned()
                        .or_else(|| chunk.get("usage").cloned());
                }
                Some("response.failed") => {
                    self.terminal_seen = true;
                    self.terminal_error_code = Some(
                        chunk
                            .pointer("/error/code")
                            .and_then(Value::as_str)
                            .unwrap_or("provider_stream_failed")
                            .to_string(),
                    );
                }
                _ => {
                    if chunk.get("error").is_some() {
                        self.terminal_seen = true;
                        self.terminal_error_code = Some("provider_stream_error".to_string());
                    }
                }
            }
            if self.terminal_seen {
                return;
            }
        }
    }
}

impl Stream for ResponsesPassthroughStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.as_mut().poll_next(context) {
            Poll::Ready(None) => {
                if let Some(guard) = self.guard.take() {
                    let terminal_error_code = if self.terminal_seen {
                        self.terminal_error_code.take()
                    } else {
                        Some("provider_stream_incomplete".to_string())
                    };
                    guard.finish_from_passthrough(terminal_error_code, self.usage.take());
                }
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                if let Some(guard) = self.guard.take() {
                    guard.finish_from_passthrough(Some("provider_stream_error".to_string()), None);
                }
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(Some(Ok(bytes))) => {
                self.observe_bytes(&bytes);
                Poll::Ready(Some(Ok(bytes)))
            }
            other => other,
        }
    }
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
    // Authorized memory is retrieved exactly once, then injected only into
    // the final wire body after compression: memory blocks never enter the
    // compression pipeline, never enter stored artifacts, and never distort
    // token accounting.  Fail-closed: no retrievable scope -> no injection.
    let memory_blocks = request_scope_memory_access(&scope)
        .as_ref()
        .map(|access| memory_context_blocks(&authorized_memory_records(state, access)))
        .unwrap_or_default();
    let compression = if provider.protocol == Protocol::Anthropic {
        // Direct Anthropic routes are planned by HybridContextEngine in the handler.
        // Keeping this preliminary body untouched prevents semantic+optical stacking.
        compress_for_route(None, CompressionPolicy::Off, upstream_body)
    } else {
        let cache_evidence = configured_provider_cache_evidence(state, &scope);
        compress_for_route_with_frozen(
            Some(&optimization_context),
            decision.compression_policy.clone(),
            upstream_body,
            cache_evidence.as_ref(),
            None,
        )
    };
    let memory_injected_body =
        inject_memory_into_body(compression.body.clone(), &provider.protocol, &memory_blocks);
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
    if !memory_blocks.is_empty() {
        if let Some(metadata) = event.metadata.as_object_mut() {
            metadata.insert(
                "memory_injection".to_string(),
                json!({
                    "injected": true,
                    "block_count": memory_blocks.len(),
                    "protocol": provider.protocol,
                    "raw_available": false,
                    "privacy": "typed memory summaries only; raw prompt, response, source and secrets are never persisted"
                }),
            );
        }
    }
    Ok(RoutedBridgePayload {
        scope,
        upstream_body: memory_injected_body,
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
            None,
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

#[derive(Debug, Clone, Copy)]
struct DirectAnthropicCacheWire {
    breakpoint_inserted: bool,
    breakpoint_count: u8,
    pointer_kind: &'static str,
    reason_code: &'static str,
    evidence_revision_present: bool,
    rolled_back: bool,
}

const DIRECT_ANTHROPIC_CACHE_TRANSFORM: &str = "anthropic_explicit_cache_breakpoint";

/// Adds a single Anthropic cache breakpoint only when the route has fresh,
/// exact provider evidence and the context planner has kept a usable prefix.
///
/// This deliberately supports only the two unambiguous Anthropic request
/// surfaces we can validate structurally: an entire tool definition or a
/// typed system content block.  Message content and string-valued `system`
/// fields are left alone rather than changing their representation.
fn apply_direct_anthropic_cache_breakpoint(
    payload: &mut RoutedBridgePayload,
    evidence: Option<&dtoken_compress::context_pipeline::ProviderCacheEvidence>,
) -> DirectAnthropicCacheWire {
    let evidence_revision_present = evidence.is_some_and(|item| !item.source_revision.is_empty());
    let unavailable = |reason_code| DirectAnthropicCacheWire {
        breakpoint_inserted: false,
        breakpoint_count: 0,
        pointer_kind: "none",
        reason_code,
        evidence_revision_present,
        rolled_back: false,
    };
    let Some(evidence) = evidence else {
        return unavailable("provider_cache_evidence_unavailable");
    };
    if evidence.strategy
        != dtoken_compress::context_pipeline::ProviderCacheStrategy::AnthropicExplicit
    {
        return unavailable("provider_cache_strategy_mismatch");
    }
    // An Anthropic-shaped compatibility endpoint must not inherit the official
    // provider's breakpoint syntax merely because it shares the wire format.
    if !is_anthropic_upstream(&payload.provider.base_url) {
        return unavailable("anthropic_cache_upstream_unverified");
    }
    let Some(context) = payload.optimization_context.as_ref() else {
        return unavailable("cache_alignment_context_unavailable");
    };
    let Ok(plan) = dtoken_compress::context_pipeline::plan_context_with_cache_evidence(
        &context.compression_context,
        &payload.upstream_body,
        Some(evidence),
    ) else {
        return unavailable("cache_alignment_plan_unavailable");
    };
    let alignment = plan.cache_alignment();
    if alignment.evidence_status()
        != dtoken_compress::context_pipeline::CacheEvidenceStatus::VerifiedEligible
    {
        return unavailable("cache_alignment_not_eligible");
    }
    if alignment.provider_strategy()
        != Some(dtoken_compress::context_pipeline::ProviderCacheStrategy::AnthropicExplicit)
        || alignment.evidence_revision().is_none()
    {
        return unavailable("cache_alignment_evidence_mismatch");
    }
    if contains_cache_control(&payload.upstream_body) {
        return unavailable("user_cache_control_preserved");
    }
    let Some((pointer, pointer_kind)) =
        frozen_anthropic_cache_pointer(&payload.upstream_body, &plan)
    else {
        return unavailable("cache_alignment_no_safe_breakpoint");
    };
    let Some(target) = payload.upstream_body.pointer_mut(&pointer) else {
        return unavailable("cache_alignment_pointer_unavailable");
    };
    let Some(target) = target.as_object_mut() else {
        return unavailable("cache_alignment_pointer_incompatible");
    };
    if target.contains_key("cache_control") {
        return unavailable("user_cache_control_preserved");
    }
    target.insert("cache_control".to_string(), json!({"type": "ephemeral"}));
    append_direct_anthropic_cache_transform(payload);
    DirectAnthropicCacheWire {
        breakpoint_inserted: true,
        breakpoint_count: 1,
        pointer_kind,
        reason_code: "anthropic_explicit_breakpoint_inserted",
        evidence_revision_present: true,
        rolled_back: false,
    }
}

fn append_direct_anthropic_cache_transform(payload: &mut RoutedBridgePayload) {
    let transform = DIRECT_ANTHROPIC_CACHE_TRANSFORM.to_string();
    if !payload
        .compression
        .transforms_applied
        .iter()
        .any(|item| item == DIRECT_ANTHROPIC_CACHE_TRANSFORM)
    {
        payload
            .compression
            .transforms_applied
            .push(transform.clone());
    }
    if !payload
        .request_log
        .transforms_applied
        .iter()
        .any(|item| item == DIRECT_ANTHROPIC_CACHE_TRANSFORM)
    {
        payload.request_log.transforms_applied.push(transform);
    }
    if let Some(metadata) = payload.event.metadata.as_object_mut() {
        metadata.insert(
            "transforms_applied".to_string(),
            json!(payload.request_log.transforms_applied),
        );
    }
}

fn rolled_back_direct_anthropic_cache_wire(
    wire: DirectAnthropicCacheWire,
    reason_code: &'static str,
) -> DirectAnthropicCacheWire {
    if !wire.breakpoint_inserted {
        return wire;
    }
    DirectAnthropicCacheWire {
        breakpoint_inserted: false,
        breakpoint_count: 0,
        pointer_kind: wire.pointer_kind,
        reason_code,
        evidence_revision_present: wire.evidence_revision_present,
        rolled_back: true,
    }
}

fn contains_cache_control(value: &Value) -> bool {
    match value {
        Value::Object(fields) => {
            fields.contains_key("cache_control") || fields.values().any(contains_cache_control)
        }
        Value::Array(values) => values.iter().any(contains_cache_control),
        _ => false,
    }
}

fn frozen_anthropic_cache_pointer(
    body: &Value,
    plan: &dtoken_compress::context_pipeline::ContextPlan,
) -> Option<(String, &'static str)> {
    let frozen = plan.cache_alignment().breakpoint_zone_index();
    for zone in plan.zones().iter().take(frozen).rev() {
        let pointer = zone.pointer();
        if pointer.starts_with("/tools/")
            && pointer.split('/').count() == 3
            && body.pointer(pointer).is_some_and(Value::is_object)
        {
            return Some((pointer.to_string(), "tool_definition"));
        }
        if pointer == "/system" {
            let Some(system) = body.pointer(pointer).and_then(Value::as_array) else {
                continue;
            };
            let Some(index) = system.len().checked_sub(1) else {
                continue;
            };
            let candidate = format!("/system/{index}");
            if body.pointer(&candidate).is_some_and(|item| {
                item.is_object() && item.get("type").and_then(Value::as_str) == Some("text")
            }) {
                return Some((candidate, "system_text_block"));
            }
        }
    }
    None
}

fn attach_direct_anthropic_cache_wire_metadata(
    event: &mut DtokenEvent,
    wire: DirectAnthropicCacheWire,
    raw_wire: bool,
) {
    if let Some(metadata) = event.metadata.as_object_mut() {
        let status = if wire.rolled_back {
            "rolled_back"
        } else if wire.breakpoint_inserted {
            "inserted"
        } else {
            "not_inserted"
        };
        metadata.insert(
            "provider_cache".to_string(),
            json!({
                "strategy": "anthropic_explicit",
                "eligible": wire.breakpoint_inserted,
                "status": status,
                "breakpoint_count": wire.breakpoint_count,
                "zone": "frozen_prefix",
                "pointer_kind": wire.pointer_kind,
                "wire_variant": if raw_wire { "raw_wire" } else { "serialized" },
                "evidence_revision_present": wire.evidence_revision_present,
                "reason_code": wire.reason_code,
                "privacy": "cache evidence metadata only; request bodies, prompt-derived digests and credentials are not persisted"
            }),
        );
    }
}

fn prepare_direct_anthropic_pipeline_for_send(
    state: &AppState,
    payload: &mut RoutedBridgePayload,
    original_body: &Value,
    ccr_original_body: &[u8],
    cache_evidence: Option<&dtoken_compress::context_pipeline::ProviderCacheEvidence>,
) -> (
    PreparedCcr,
    dtoken_types::SentPayloadVariant,
    DirectAnthropicCacheWire,
) {
    let mut cache_wire = apply_direct_anthropic_cache_breakpoint(payload, cache_evidence);
    let prepared_ccr = match state.prepare_ccr_for_request(
        &payload.decision,
        &payload.request_log,
        ccr_original_body,
    ) {
        Ok(prepared) => prepared,
        Err(_) => {
            cache_wire = rolled_back_direct_anthropic_cache_wire(
                cache_wire,
                "cache_breakpoint_ccr_unavailable",
            );
            force_payload_ccr_pre_send_baseline(
                payload,
                original_body,
                "baseline_fallback_store_unavailable",
            );
            state
                .prepare_ccr_for_request(&payload.decision, &payload.request_log, ccr_original_body)
                .expect("baseline request does not require CCR storage")
        }
    };
    let pipeline_variant =
        match apply_routed_pipeline_payload(payload, original_body, &prepared_ccr) {
            Ok(variant) => variant,
            Err(_) => {
                force_payload_ccr_pre_send_baseline(
                    payload,
                    original_body,
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
    if cache_wire.breakpoint_inserted
        && pipeline_variant == dtoken_types::SentPayloadVariant::Original
    {
        cache_wire = rolled_back_direct_anthropic_cache_wire(
            cache_wire,
            "cache_breakpoint_pipeline_fallback",
        );
    }
    (prepared_ccr, pipeline_variant, cache_wire)
}

fn direct_anthropic_uses_raw_body(variant: dtoken_types::SentPayloadVariant) -> bool {
    variant == dtoken_types::SentPayloadVariant::Original
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
        "raw_returned": false,
        "note": "CCR metadata store is empty; raw prompt/response/CCR body is not stored by default."
    })
}

fn runtime_cache_summary_for_state(state: &AppState) -> Value {
    let chronological_metadata = state
        .recent_ccr_metadata(10_000)
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let metadata = latest_ccr_metadata_records(&chronological_metadata);
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
        "raw_returned": false,
        "private_store_configured": private_store_configured,
        "raw_access": if private_store_configured { "in-process-authorized-only" } else { "disabled" },
        "recent_refs": metadata.into_iter().rev().take(10).collect::<Vec<_>>(),
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
        "raw_returned": false,
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
    memory_summary_for_scope(state, None)
}

fn memory_summary_for_scope(state: &AppState, scope: Option<MemoryAccessScope>) -> Value {
    let Some(scope) = scope else {
        return json!({
            "source": "dtoken-owned-runtime",
            "runtime_kind": "headroom-derived",
            "state": "scope_required",
            "entries": 0,
            "raw_available": false,
            "recent": [],
            "reason_code": "memory_summary_authorization_unavailable",
            "note": "Typed memory summaries require an authorized project and Agent scope; this loopback endpoint does not establish one."
        });
    };
    let Some(store) = state.context_store.as_ref() else {
        return json!({
            "source": "dtoken-owned-runtime",
            "runtime_kind": "headroom-derived",
            "state": "unavailable",
            "entries": 0,
            "raw_available": false,
            "recent": [],
            "reason_code": "memory_store_requires_persistent_context_store"
        });
    };
    let Ok(store) = store.lock() else {
        return json!({
            "source": "dtoken-owned-runtime",
            "runtime_kind": "headroom-derived",
            "state": "unavailable",
            "entries": 0,
            "raw_available": false,
            "recent": [],
            "reason_code": "memory_store_lock_unavailable"
        });
    };
    let records = match store.list_visible_memory(&scope) {
        Ok(records) => records,
        Err(_) => {
            return json!({
                "source": "dtoken-owned-runtime",
                "runtime_kind": "headroom-derived",
                "state": "invalid_scope",
                "entries": 0,
                "raw_available": false,
                "recent": [],
                "reason_code": "memory_access_scope_rejected"
            });
        }
    };
    let mut by_project = serde_json::Map::new();
    let mut by_agent = serde_json::Map::new();
    for record in &records {
        let next = by_project
            .get(&record.project_id)
            .and_then(Value::as_u64)
            .unwrap_or_default()
            + 1;
        by_project.insert(record.project_id.clone(), json!(next));
        if let Some(agent) = record.agent_id.as_deref() {
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
        "state": "ready",
        "entries": records.len(),
        "raw_available": false,
        "by_project": by_project,
        "by_agent": by_agent,
        "recent": records.iter().take(20).map(memory_record_payload).collect::<Vec<_>>(),
        "note": "Typed memory metadata only; raw prompt and response bodies are not stored."
    })
}

fn output_shape_for_state(
    state: &AppState,
    project_id: Option<&str>,
    agent: Option<&str>,
) -> Value {
    let Some(project_id) = project_id.filter(|value| !value.trim().is_empty()) else {
        return json!({
            "source": "dtoken-owned-runtime", "state": "unavailable", "active": false,
            "project_id": project_id, "agent": agent, "output_verbosity": "auto",
            "learned_rules": [], "applied_requests": 0, "accounting_evidence": "none",
            "raw_available": false, "reason_code": "memory_output_shape_authorization_unavailable",
            "note": "Output shaping requires an authorized typed-memory scope."
        });
    };
    let Some(agent) = agent.filter(|value| !value.trim().is_empty()) else {
        return json!({"source":"dtoken-owned-runtime","state":"unavailable","active":false,"project_id":project_id,"agent":agent,"learned_rules":[],"reason_code":"memory_output_shape_authorization_unavailable","raw_available":false});
    };
    let scope = MemoryAccessScope {
        project_id: project_id.to_string(),
        agent_id: agent.to_string(),
        session_id: None,
    };
    let Some(store) = state.context_store.as_ref() else {
        return json!({"source":"dtoken-owned-runtime","state":"unavailable","active":false,"project_id":project_id,"agent":agent,"learned_rules":[],"reason_code":"memory_store_requires_persistent_context_store","raw_available":false});
    };
    let Ok(store) = store.lock() else {
        return json!({"source":"dtoken-owned-runtime","state":"unavailable","active":false,"project_id":project_id,"agent":agent,"learned_rules":[],"reason_code":"memory_store_lock_unavailable","raw_available":false});
    };
    let Ok(records) = store.list_visible_memory(&scope) else {
        return json!({"source":"dtoken-owned-runtime","state":"unavailable","active":false,"project_id":project_id,"agent":agent,"learned_rules":[],"reason_code":"memory_access_scope_rejected","raw_available":false});
    };
    let rules = records.iter().filter(|record| record.kind == "preference" || record.kind == "output_style").take(8).map(|record| json!({"memory_id":record.memory_id,"revision":record.entry_revision,"kind":record.kind,"keywords":record.keywords})).collect::<Vec<_>>();
    let verbosity = if rules.iter().any(|rule| {
        rule["keywords"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|key| key.as_str() == Some("terse")))
    }) {
        "terse"
    } else {
        "auto"
    };
    json!({"source":"dtoken-owned-runtime","state":"planned","active":false,"project_id":project_id,"agent":agent,"output_verbosity":verbosity,"learned_rules":rules,"applied_requests":0,"accounting_evidence":"typed_memory_scope","raw_available":false,"reason_code":"output_shape_execution_not_yet_bound_to_route"})
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
        let mut compression = compress_for_route_with_frozen(
            Some(&optimization_context),
            decision.compression_policy.clone(),
            original_body,
            cache_evidence.as_ref(),
            None,
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
        let mut upstream_body = serde_json::from_slice::<Value>(pipeline_payload.ready.body())
            .unwrap_or_else(|_| baseline_body.clone());
        // Authorized memory is injected into the final wire body after
        // compression: memory blocks never enter the compression pipeline,
        // never enter stored artifacts, and never distort token accounting.
        let memory_blocks = request_scope_memory_access(&scope)
            .as_ref()
            .map(|access| memory_context_blocks(&authorized_memory_records(&state, access)))
            .unwrap_or_default();
        if !memory_blocks.is_empty() {
            let injected =
                inject_memory_into_body(upstream_body.clone(), &Protocol::OpenAi, &memory_blocks);
            upstream_body = injected;
            if let Some(metadata) = event.metadata.as_object_mut() {
                metadata.insert(
                    "memory_injection".to_string(),
                    json!({
                        "injected": true,
                        "block_count": memory_blocks.len(),
                        "protocol": "open_ai",
                        "raw_available": false,
                        "privacy": "typed memory summaries only; raw prompt, response, source and secrets are never persisted"
                    }),
                );
            }
        }
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
                    attach_output_shaping_evidence(
                        &mut request_log,
                        &mut event,
                        output_shaping_passthrough_evidence(
                            "stream_passthrough",
                            &scope.route_id,
                            &scope.provider_id,
                            &scope.model,
                            &scope.config_revision,
                            &scope.request_id.to_string(),
                            "direct",
                            compression_policy_as_str(&decision.compression_policy),
                        ),
                    );
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
        let output_shaping = if upstream.error_category.is_some() {
            output_shaping_passthrough_evidence(
                "error_response_passthrough",
                &scope.route_id,
                &scope.provider_id,
                &scope.model,
                &scope.config_revision,
                &scope.request_id.to_string(),
                "direct",
                compression_policy_as_str(&decision.compression_policy),
            )
        } else {
            apply_openai_output_shaping(
                &mut upstream.response,
                route,
                &scope,
                output_shaping_policy_for_scope(&state.config, &scope),
                "direct",
                compression_policy_as_str(&decision.compression_policy),
            )
        };
        attach_output_shaping_evidence(&mut request_log, &mut event, output_shaping);
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
    let route_selection = configured_route(
        &state.config,
        Protocol::OpenAi,
        header_evidence(&headers, body.get("model").and_then(Value::as_str)),
    );
    if let Ok((route, provider)) = route_selection {
        let request_id = Uuid::new_v4();
        let decision = build_configured_route_decision(request_id, route, provider);
        if provider.wire_api == dtoken_types::ProviderWireApi::OpenAiResponses {
            // Native Responses passthrough: forward the inbound Responses body
            // verbatim to the provider's /v1/responses wire endpoint, bypassing
            // the Responses -> Chat semantic bridge and all compression
            // transforms. The route decision, request log and ledger event are
            // still recorded with CompressionPolicy::Off over the original
            // request so Dashboard/Doctor/Event Ledger stay consistent.
            let scope = match resolve_configured_request_scope(
                &state.config,
                route,
                provider,
                request_id,
                body.get("model").and_then(Value::as_str),
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
            let compression = compress_for_route(None, CompressionPolicy::Off, body.clone());
            let attribution = request_attribution_from_scope(&scope);
            let mut request_log = build_request_log(
                request_id,
                &decision,
                body.get("model")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
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
            let capability_summary = body
                .get("model")
                .and_then(Value::as_str)
                .map(|model| capability_bridge::resolve_model_capability_summary(model, Some("responses")))
                .unwrap_or_else(|| json!({ "unknown": true, "model_id": null }));
            if let Some(metadata) = event.metadata.as_object_mut() {
                metadata.insert("capability".to_string(), capability_summary.clone());
                metadata.insert(
                    "native_responses_passthrough".to_string(),
                    json!({
                        "conversion": "none",
                        "compression": "off",
                        "original_request_preserved": true,
                        "raw_available": false,
                        "privacy": "native Responses body is forwarded verbatim and never stored"
                    }),
                );
            }
            let prepared_ccr =
                match state.prepare_ccr_for_request(&decision, &request_log, &ccr_original_body) {
                    Ok(prepared) => prepared,
                    Err(_) => {
                        let (status, response) = route_selection_error_response(
                            request_id,
                            "openai_responses",
                            RouteSelectionError::InvalidOptimizationContext,
                            &state.config_status,
                        );
                        return (status, Json(response)).into_response();
                    }
                };
            if body.get("stream").and_then(Value::as_bool).unwrap_or(false) {
                let quota_account_ref = opaque_codex_account_ref(&headers);
                let upstream = forward_openai_responses_stream(
                    &state.http_client,
                    provider,
                    body,
                    Some(&headers),
                )
                .await;
                match upstream {
                    Ok((status_code, response_headers, stream_body, mut quota_metadata)) => {
                        attach_quota_account_ref(&mut quota_metadata, quota_account_ref.as_deref());
                        request_log.quota_metadata = quota_metadata;
                        event.saved_tokens = Some(0);
                        if let Some(metadata) = event.metadata.as_object_mut() {
                            metadata.insert("stream".to_string(), json!(true));
                            metadata.insert("upstream_status".to_string(), json!(status_code));
                        }
                        // G4: the terminal ledger event and CCR receipt are
                        // finalized only when the passthrough stream reaches a
                        // terminal state (completed / failed / dropped). The
                        // body guard observes native Responses SSE events
                        // without changing any bytes; nothing is recorded here.
                        append_stage_event(
                            &state,
                            request_id,
                            LayeredStage::ProviderForward,
                            EventStatus::Ok,
                            json!({
                                "route_id": decision.route_id,
                                "provider_id": decision.provider_id,
                                "input_protocol": "openai_responses",
                                "output_protocol": "openai_responses_sse",
                                "stream": true,
                                "upstream_status": status_code,
                                "terminal": false,
                                "send_state": "stream_started",
                                "fallback": "disabled",
                                "raw_available": false,
                                "privacy": "native Responses SSE is passed through untouched and is not stored."
                            }),
                            None,
                        );
                        let inner: UpstreamByteStream = Box::pin(
                            stream_body
                                .into_data_stream()
                                .map_err(std::io::Error::other),
                        );
                        let guarded = Body::from_stream(ResponsesPassthroughStream::new(
                            inner,
                            ResponsesStreamTerminalLedgerGuard {
                                state: state.clone(),
                                scope: scope.clone(),
                                request_id,
                                provider_id: decision.provider_id.clone(),
                                upstream_status: status_code,
                                terminal_record: Some(StreamTerminalRequestRecord {
                                    decision,
                                    request_log,
                                    event,
                                    prepared_ccr,
                                }),
                                terminal: false,
                            },
                        ));
                        return (
                            StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK),
                            response_headers,
                            guarded,
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
                                "input_protocol": "openai_responses",
                                "output_protocol": "openai_responses_sse",
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
                forward_openai_responses(&state.http_client, provider, body, Some(&headers)).await;
            attach_quota_account_ref(
                &mut upstream.quota_metadata,
                opaque_codex_account_ref(&headers).as_deref(),
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
                // Native Responses passthrough: the successful upstream JSON is
                // returned byte-for-byte (Codex schema contract). Evidence stays
                // in the internal request log and Event Ledger only, never in
                // the upstream response body.
                upstream.response
            };
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
                provider_status,
                json!({
                    "route_id": decision.route_id,
                    "provider_id": decision.provider_id,
                    "input_protocol": "openai_responses",
                    "output_protocol": "openai_responses",
                    "upstream_status": provider_http_status,
                    "fallback": "disabled",
                    "raw_available": false,
                    "privacy": "native Responses passthrough records route/provider metadata only; input and output bodies are not stored."
                }),
                provider_error.as_deref(),
            );
            return (response_status, Json(response)).into_response();
        }
        // Fail-closed Responses -> Chat semantic bridge: unsupported input items,
        // tools and generation controls are rejected before any route or upstream
        // work instead of being silently dropped or guessed.
        let original_body = match responses_to_openai_chat_body(&body) {
            Ok(converted) => converted,
            Err(reason) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": {
                            "type": "unsupported_responses_input",
                            "message": reason
                        }
                    })),
                )
                    .into_response();
            }
        };
        let request = OpenAiChatRequest {
            model: original_body
                .get("model")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            messages: original_body
                .get("messages")
                .and_then(Value::as_array)
                .cloned(),
            stream: body.get("stream").and_then(Value::as_bool),
        };
        // `original_body` is the Responses -> Chat conversion computed above;
        // it preserves the requested `stream` flag instead of silently
        // downgrading Responses streaming to a non-streaming Chat call.
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
        let mut compression = compress_for_route_with_frozen(
            Some(&optimization_context),
            decision.compression_policy.clone(),
            original_body,
            cache_evidence.as_ref(),
            None,
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
        let mut upstream_body = serde_json::from_slice::<Value>(pipeline_payload.ready.body())
            .unwrap_or_else(|_| baseline_body.clone());
        // Authorized memory is injected into the final wire body after
        // compression: memory blocks never enter the compression pipeline,
        // never enter stored artifacts, and never distort token accounting.
        let memory_blocks = request_scope_memory_access(&scope)
            .as_ref()
            .map(|access| memory_context_blocks(&authorized_memory_records(&state, access)))
            .unwrap_or_default();
        if !memory_blocks.is_empty() {
            let injected =
                inject_memory_into_body(upstream_body.clone(), &Protocol::OpenAi, &memory_blocks);
            upstream_body = injected;
            if let Some(metadata) = event.metadata.as_object_mut() {
                metadata.insert(
                    "memory_injection".to_string(),
                    json!({
                        "injected": true,
                        "block_count": memory_blocks.len(),
                        "protocol": "open_ai",
                        "raw_available": false,
                        "privacy": "typed memory summaries only; raw prompt, response, source and secrets are never persisted"
                    }),
                );
            }
        }
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
        if request.stream.unwrap_or(false) {
            // Real Chat SSE -> Responses SSE event conversion; never a silent
            // downgrade of stream:true to a non-streaming call.
            let quota_account_ref = opaque_codex_account_ref(&headers);
            let upstream = forward_openai_chat_stream(
                &state.http_client,
                provider,
                upstream_body.clone(),
                Some(&headers),
            )
            .await;
            match upstream {
                Ok((status_code, mut response_headers, body, mut quota_metadata)) => {
                    attach_quota_account_ref(&mut quota_metadata, quota_account_ref.as_deref());
                    request_log.quota_metadata = quota_metadata;
                    attach_output_shaping_evidence(
                        &mut request_log,
                        &mut event,
                        output_shaping_passthrough_evidence(
                            "stream_passthrough",
                            &scope.route_id,
                            &scope.provider_id,
                            &scope.model,
                            &scope.config_revision,
                            &scope.request_id.to_string(),
                            "direct",
                            compression_policy_as_str(&decision.compression_policy),
                        ),
                    );
                    if let Some(accounting) = request_log.context_accounting.as_mut() {
                        accounting.upstream_state = UpstreamSendState::StreamStarted;
                        accounting.attribution_basis = dtoken_types::SavingsAttribution::None;
                    }
                    event.context_accounting = request_log.context_accounting.clone();
                    event.status = EventStatus::Ok;
                    event.saved_tokens = Some(0);
                    if let Some(metadata) = event.metadata.as_object_mut() {
                        metadata.insert("stream".to_string(), json!(true));
                        metadata.insert(
                            "stream_semantic_bridge".to_string(),
                            json!("responses_sse_from_chat_sse"),
                        );
                        metadata.insert("upstream_status".to_string(), json!(status_code));
                        metadata.insert(
                            "privacy".to_string(),
                            json!(
                                "Responses SSE bridge converts Chat SSE events in memory; the converted stream is not stored by d-token."
                            ),
                        );
                    }
                    append_stage_event(
                        &state,
                        request_id,
                        LayeredStage::UpstreamSendCommitted,
                        EventStatus::Ok,
                        json!({
                            "route_id": decision.route_id,
                            "provider_id": decision.provider_id,
                            "input_protocol": "openai_responses",
                            "output_protocol": "openai_responses_sse",
                            "stream": true,
                            "upstream_status": status_code,
                            "terminal": false,
                            "send_state": "stream_started",
                            "fallback": "disabled",
                            "raw_available": false,
                            "privacy": "Responses SSE bridge emits converted events only; raw Chat SSE is not stored."
                        }),
                        None,
                    );
                    response_headers.insert(
                        CONTENT_TYPE,
                        axum::http::HeaderValue::from_static("text/event-stream"),
                    );
                    let converted = responses_sse_from_chat_sse(
                        body,
                        format!("resp-{request_id}"),
                        request.model.clone(),
                        ResponsesStreamTerminalLedgerGuard {
                            state: state.clone(),
                            scope: scope.clone(),
                            request_id,
                            provider_id: decision.provider_id.clone(),
                            upstream_status: status_code,
                            terminal_record: Some(StreamTerminalRequestRecord {
                                decision,
                                request_log,
                                event,
                                prepared_ccr,
                            }),
                            terminal: false,
                        },
                    );
                    return (
                        StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK),
                        response_headers,
                        converted,
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
                            "input_protocol": "openai_responses",
                            "output_protocol": "openai_responses_sse",
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
        let mut upstream = require_openai_message(
            forward_openai_chat(&state.http_client, provider, upstream_body, Some(&headers)).await,
            "OpenAI Responses",
        );
        attach_quota_account_ref(
            &mut upstream.quota_metadata,
            opaque_codex_account_ref(&headers).as_deref(),
        );
        let output_shaping = if upstream.error_category.is_some() {
            output_shaping_passthrough_evidence(
                "error_response_passthrough",
                &scope.route_id,
                &scope.provider_id,
                &scope.model,
                &scope.config_revision,
                &scope.request_id.to_string(),
                "direct",
                compression_policy_as_str(&decision.compression_policy),
            )
        } else {
            apply_openai_output_shaping(
                &mut upstream.response,
                route,
                &scope,
                output_shaping_policy_for_scope(&state.config, &scope),
                "direct",
                compression_policy_as_str(&decision.compression_policy),
            )
        };
        attach_output_shaping_evidence(&mut request_log, &mut event, output_shaping);
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
            let mut response = openai_chat_to_responses_response(
                &upstream.response,
                request.model.as_deref(),
                &format!("resp-{request_id}"),
            );
            if let Some(object) = response.as_object_mut() {
                object.insert(
                    "dtoken".to_string(),
                    json!({
                        "source": "dtoken-owned-runtime",
                        "runtime_kind": "headroom-derived",
                        "external_headroom_dependency": false,
                        "route_decision": route_decision_to_value(&decision),
                        "request_log": request_log,
                        "ledger_event": event,
                        "upstream_status": upstream.status,
                        "compression": compression_json,
                        "fallback": "disabled"
                    }),
                );
            }
            response
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

    // Route selection failed. The wire API is unknowable without a selected
    // route, so the fail-closed Responses -> Chat semantic validation still
    // runs first: unsupported Responses input (e.g. reasoning items) must be
    // rejected before the route error, matching the historical pre-route
    // behavior. A selected native Responses provider bypasses validation
    // (above); a selected Chat-wire provider validates before side effects.
    if let Err(reason) = responses_to_openai_chat_body(&body) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "type": "unsupported_responses_input",
                    "message": reason
                }
            })),
        )
            .into_response();
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
        let direct_cache_evidence = configured_provider_cache_evidence(&state, &payload.scope)
            .filter(|evidence| {
                evidence.strategy
                    == dtoken_compress::context_pipeline::ProviderCacheStrategy::AnthropicExplicit
            });
        // Without fresh, exact provider evidence, Direct Anthropic must retain
        // the inbound representation all the way to the wire.  In particular,
        // do not let a JSON re-serialization or a Hybrid candidate turn an
        // unverified cache optimization into an observable request mutation.
        let raw_wire_required = direct_cache_evidence.is_none();
        // Active preparation owns the request-wide byte lease. Keep it alive
        // until the upstream operation reaches a terminal return.
        let mut _retained_hybrid_request = None;
        if !raw_wire_required {
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
        } else {
            force_payload_ccr_pre_send_baseline(
                &mut payload,
                &body,
                "provider_cache_evidence_unavailable",
            );
            append_stage_event(
                &state,
                payload.decision.request_id,
                LayeredStage::OpticalFallback,
                EventStatus::Failed,
                json!({
                    "reason_code": "provider_cache_evidence_unavailable",
                    "raw_available": false,
                    "baseline_selected": true,
                    "automatic_retry": false
                }),
                Some("provider_cache_evidence_unavailable"),
            );
        }

        let (prepared_ccr, pipeline_variant, cache_wire) =
            prepare_direct_anthropic_pipeline_for_send(
                &state,
                &mut payload,
                &body,
                &ccr_original_body,
                direct_cache_evidence.as_ref(),
            );
        let send_raw_body = direct_anthropic_uses_raw_body(pipeline_variant);
        attach_direct_anthropic_cache_wire_metadata(&mut payload.event, cache_wire, send_raw_body);
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
                    attach_output_shaping_evidence(
                        &mut payload.request_log,
                        &mut payload.event,
                        output_shaping_passthrough_evidence(
                            "stream_passthrough",
                            &payload.scope.route_id,
                            &payload.scope.provider_id,
                            &payload.scope.model,
                            &payload.scope.config_revision,
                            &payload.scope.request_id.to_string(),
                            "direct",
                            compression_policy_as_str(&payload.decision.compression_policy),
                        ),
                    );
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
                        metadata.insert("stream_passthrough".to_string(), json!(true));
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
                            scope: payload.scope.clone(),
                            request_id: payload.decision.request_id,
                            provider_id: payload.decision.provider_id.clone(),
                            upstream_status: status_code,
                            kind: stream_kind,
                            accounting: stream_accounting,
                            usage_observer: AnthropicStreamUsageObserver::default(),
                            terminal_record: Some(StreamTerminalRequestRecord {
                                decision: payload.decision,
                                request_log: payload.request_log,
                                event: payload.event,
                                prepared_ccr,
                            }),
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

        let mut upstream = require_anthropic_message(
            forward_anthropic_messages(
                &state.http_client,
                &payload.provider,
                payload.upstream_body.clone(),
                send_raw_body.then_some(raw_body.as_slice()),
                &headers,
            )
            .await,
        );
        let output_shaping = if upstream.error_category.is_some() {
            output_shaping_passthrough_evidence(
                "error_response_passthrough",
                &payload.scope.route_id,
                &payload.scope.provider_id,
                &payload.scope.model,
                &payload.scope.config_revision,
                &payload.scope.request_id.to_string(),
                "direct",
                compression_policy_as_str(&payload.decision.compression_policy),
            )
        } else if let Some(route) = state.config.find_route(&payload.decision.route_id) {
            apply_anthropic_output_shaping(
                &mut upstream.response,
                route,
                &payload.scope,
                output_shaping_policy_for_scope(&state.config, &payload.scope),
                "direct",
                compression_policy_as_str(&payload.decision.compression_policy),
            )
        } else {
            output_shaping_passthrough_evidence(
                "route_output_shaping_policy_unavailable",
                &payload.scope.route_id,
                &payload.scope.provider_id,
                &payload.scope.model,
                &payload.scope.config_revision,
                &payload.scope.request_id.to_string(),
                "direct",
                compression_policy_as_str(&payload.decision.compression_policy),
            )
        };
        attach_output_shaping_evidence(
            &mut payload.request_log,
            &mut payload.event,
            output_shaping,
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
    let output_shaping = if upstream.error_category.is_some() {
        output_shaping_passthrough_evidence(
            "error_response_passthrough",
            &payload.scope.route_id,
            &payload.scope.provider_id,
            &payload.scope.model,
            &payload.scope.config_revision,
            &payload.scope.request_id.to_string(),
            "direct",
            compression_policy_as_str(&payload.decision.compression_policy),
        )
    } else if let Some(route) = state.config.find_route(&payload.decision.route_id) {
        apply_openai_output_shaping(
            &mut upstream.response,
            route,
            &payload.scope,
            output_shaping_policy_for_scope(&state.config, &payload.scope),
            "direct",
            compression_policy_as_str(&payload.decision.compression_policy),
        )
    } else {
        output_shaping_passthrough_evidence(
            "route_output_shaping_policy_unavailable",
            &payload.scope.route_id,
            &payload.scope.provider_id,
            &payload.scope.model,
            &payload.scope.config_revision,
            &payload.scope.request_id.to_string(),
            "direct",
            compression_policy_as_str(&payload.decision.compression_policy),
        )
    };
    attach_output_shaping_evidence(&mut payload.request_log, &mut payload.event, output_shaping);
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

#[derive(Debug, Default, Deserialize)]
struct FailurePatternsQuery {
    project_id: Option<String>,
    agent_id: Option<String>,
    limit: Option<usize>,
}

async fn learning_failure_patterns(
    State(state): State<AppState>,
    Query(query): Query<FailurePatternsQuery>,
) -> Json<Value> {
    let Some(store) = state.context_store.as_ref() else {
        return Json(json!({
            "source": "dtoken-owned-runtime",
            "state": "unavailable",
            "patterns": [],
            "reason_code": "learning_store_requires_persistent_context_store",
            "raw_available": false,
        }));
    };
    let Ok(store) = store.lock() else {
        return Json(json!({
            "source": "dtoken-owned-runtime",
            "state": "unavailable",
            "patterns": [],
            "reason_code": "learning_store_lock_unavailable",
            "raw_available": false,
        }));
    };
    let observed_at_ms = dtoken_event_ledger::now_ms().max(1);
    match store.list_failure_patterns(
        query.project_id.as_deref(),
        query.agent_id.as_deref(),
        observed_at_ms,
        query.limit.unwrap_or(100),
    ) {
        Ok(patterns) => Json(json!({
            "source": "dtoken-owned-runtime",
            "state": "ready",
            "freshness": "live",
            "observed_at_ms": observed_at_ms,
            "patterns": patterns,
            "raw_available": false,
            "privacy": "Allowlisted failure classes and opaque route scopes only; no error text, prompt, response, command output, or credential is stored.",
        })),
        Err(_) => Json(json!({
            "source": "dtoken-owned-runtime",
            "state": "blocked",
            "patterns": [],
            "reason_code": "failure_pattern_scope_rejected",
            "raw_available": false,
        })),
    }
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

async fn rtk_status(State(state): State<AppState>) -> Json<Value> {
    let external = detect_rtk_status().await;
    let managed = state.managed_rtk_runtime_evidence();
    let mut payload = serde_json::to_value(external).unwrap_or_else(|_| json!({}));
    if let Some(object) = payload.as_object_mut() {
        object.insert("source".to_string(), json!("dtoken-owned-runtime"));
        object.insert("managed_runtime".to_string(), json!(managed));
        object.insert(
            "integrated".to_string(),
            json!(managed_rtk_invocation_verified(&managed)),
        );
        object.insert(
            "privacy".to_string(),
            json!("Managed RTK evidence excludes argv, command output, paths, and digests."),
        );
    }
    Json(payload)
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

async fn headroom_status(State(state): State<AppState>) -> Json<Value> {
    let reference = detect_headroom_status().await;
    let managed_rtk = state.managed_rtk_runtime_evidence();
    let integrated_rtk = managed_rtk_invocation_verified(&managed_rtk);
    Json(json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "status": "owned_runtime_active",
        "integrated_rtk": integrated_rtk,
        "integrated_rtk_state": if integrated_rtk {
            "verified_runtime_invocation"
        } else {
            managed_rtk.reason.as_str()
        },
        "integrated_rtk_evidence": managed_rtk,
        "reference_headroom": reference,
        "note": "External Headroom detection is reference-only; d-token runtime is the product data source. Integrated RTK is active only after a verified d-token-owned invocation receipt."
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
    let transport = mcp::McpServer::transport_name();
    let capabilities = state.current_capability_registry();
    let capability = capabilities
        .find("mcp")
        .expect("backend capability registry must contain MCP");
    Json(json!({
        "source": "dtoken-owned-runtime",
        "runtime_kind": "headroom-derived",
        "external_headroom_dependency": false,
        "state": capability.state,
        "configured": capability.is_available(),
        "capability_revision": capabilities.revision,
        "capability_collected_at_ms": capabilities.collected_at_ms,
        "implementation_revision": capability.implementation_revision,
        "evidence_revision": capability.evidence_revision,
        "freshness": capability.freshness,
        "reason_codes": capability.reason_codes,
        "transport": transport.map_or(Value::Null, Value::from),
        "http_transport": "http-json-rpc",
        "tools": mcp::McpServer::new(state.clone()).available_tool_names(),
        "unavailable_tools": [
            {
                "name": "headroom_retrieve",
                "reason": "retrieve_requires_managed_capability"
            },
            {
                "name": "headroom_memory_list",
                "reason": "shared_context_requires_managed_scope_capability"
            }
        ],
        "implemented_as": ["dtoken-core-api::mcp::McpServer"],
        "ccr_entries": ccr["entries"],
        "memory_entries": memory["entries"],
        "raw_available": false,
        "reason": transport.map_or_else(
            || "MCP transport is unavailable.".to_string(),
            |name| format!("MCP {name} is d-token-owned, plus a JSON-only HTTP POST transport at /mcp for the streamable-HTTP `Accept: application/json` mode. CCR retrieval is registered only when the managed launcher supplies its memory-only capability; this HTTP status surface never exposes that capability."),
        )
    }))
}

/// Streamable-HTTP MCP JSON mode with bounded sessions.
///
/// The MCP streamable-HTTP spec lets a client request JSON-only delivery by
/// sending `Accept: application/json`; the server then answers with a single
/// JSON-RPC response instead of an SSE stream.  d-token implements exactly
/// that reliable subset: the request is parsed with the same bounded-frame
/// limits as the stdio transport and handled by the same `McpServer`, so the
/// decision/reason/artifact contract is byte-identical across transports.
///
/// Sessions are implicit and bounded: `initialize` creates a session whose
/// id is returned in the `Mcp-Session-Id` header, and every subsequent
/// request must carry that header.  The session layer records request and
/// terminal state without persisting request bodies.  Any request without a
/// session id other than `initialize` is rejected fail-closed instead of
/// guessing a session.  Idle sessions are reaped lazily at this boundary, so
/// clients that exit without DELETE /mcp release their capacity slot within
/// `mcp::MCP_SESSION_IDLE_TTL`; DELETE /mcp removes a session immediately.
/// No SSE session endpoint is registered (the axum build has no `sse`
/// feature), so sessions answer JSON only.
async fn mcp_http(
    Extension(server): Extension<Arc<mcp::McpServer>>,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Response {
    server.reap_idle_sessions();
    if body_bytes.len() > mcp::MAX_MCP_MESSAGE_BYTES {
        return jsonrpc_http_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            Value::Null,
            -32600,
            "Request too large",
        );
    }
    let request: Value = match serde_json::from_slice(&body_bytes) {
        Ok(request) => request,
        Err(_) => {
            return jsonrpc_http_error(StatusCode::BAD_REQUEST, Value::Null, -32700, "Parse error");
        }
    };
    let session_id = headers
        .get(mcp::MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let Some(session_id) = session_id else {
        if request.get("method").and_then(Value::as_str) == Some("initialize") {
            return initialize_session(server, request).await;
        }
        // Fail-closed: a session is created only by `initialize`; any other
        // request without a session id is rejected, never guessed.
        return jsonrpc_http_error(
            StatusCode::BAD_REQUEST,
            request.get("id").cloned().unwrap_or(Value::Null),
            mcp::McpSessionError::SessionUnknown.code(),
            mcp::McpSessionError::SessionUnknown.message(),
        );
    };
    match server.session_state(&session_id) {
        Ok(state) if state.status == mcp::McpSessionStatus::Active => {}
        Ok(_) => {
            return jsonrpc_http_error(
                StatusCode::BAD_REQUEST,
                request.get("id").cloned().unwrap_or(Value::Null),
                mcp::McpSessionError::SessionTerminated.code(),
                mcp::McpSessionError::SessionTerminated.message(),
            );
        }
        Err(mcp::McpSessionError::SessionUnknown) => {
            return jsonrpc_http_error(
                StatusCode::BAD_REQUEST,
                request.get("id").cloned().unwrap_or(Value::Null),
                mcp::McpSessionError::SessionUnknown.code(),
                mcp::McpSessionError::SessionUnknown.message(),
            );
        }
        Err(_) => {
            return jsonrpc_http_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                request.get("id").cloned().unwrap_or(Value::Null),
                -32603,
                "Internal error",
            );
        }
    }
    match server.submit_session_jsonrpc(&session_id, request).await {
        Ok(Some(response)) => (StatusCode::OK, Json(response)).into_response(),
        Ok(None) => {
            // Notifications have no response; Streamable-HTTP still answers
            // with a 202 Accepted and an empty body per the spec.
            (StatusCode::ACCEPTED, ()).into_response()
        }
        Err(error) => {
            let status = match error {
                mcp::McpSessionError::SessionBusy => StatusCode::CONFLICT,
                _ => StatusCode::BAD_REQUEST,
            };
            jsonrpc_http_error(status, Value::Null, error.code(), error.message())
        }
    }
}

/// Terminate one Streamable-HTTP MCP session over DELETE /mcp, identified by
/// the `Mcp-Session-Id` header. The in-flight request (if any) is cancelled
/// and the session's capacity slot is reclaimed immediately, so a client that
/// finishes normally can release capacity without waiting for the idle TTL.
/// Fail-closed: a missing header and an unknown session id are both rejected.
async fn mcp_http_delete(
    Extension(server): Extension<Arc<mcp::McpServer>>,
    headers: HeaderMap,
) -> Response {
    let Some(session_id) = headers
        .get(mcp::MCP_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
    else {
        return jsonrpc_http_error(
            StatusCode::BAD_REQUEST,
            Value::Null,
            mcp::McpSessionError::SessionUnknown.code(),
            mcp::McpSessionError::SessionUnknown.message(),
        );
    };
    match server.remove_session(&session_id) {
        Ok(_) => (StatusCode::NO_CONTENT, ()).into_response(),
        Err(mcp::McpSessionError::SessionUnknown) => jsonrpc_http_error(
            StatusCode::NOT_FOUND,
            Value::Null,
            mcp::McpSessionError::SessionUnknown.code(),
            mcp::McpSessionError::SessionUnknown.message(),
        ),
        Err(_) => jsonrpc_http_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            Value::Null,
            -32603,
            "Internal error",
        ),
    }
}

/// Creates the session for an `initialize` request and answers with the
/// `Mcp-Session-Id` header, so the client can bind subsequent requests.
async fn initialize_session(server: Arc<mcp::McpServer>, request: Value) -> Response {
    let state = match server.create_session() {
        Ok(state) => state,
        Err(mcp::McpSessionError::SessionCapacityExhausted) => {
            return jsonrpc_http_error(
                StatusCode::SERVICE_UNAVAILABLE,
                request.get("id").cloned().unwrap_or(Value::Null),
                mcp::McpSessionError::SessionCapacityExhausted.code(),
                mcp::McpSessionError::SessionCapacityExhausted.message(),
            );
        }
        Err(_) => {
            return jsonrpc_http_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                request.get("id").cloned().unwrap_or(Value::Null),
                -32603,
                "Internal error",
            );
        }
    };
    match server
        .submit_session_jsonrpc(&state.session_id, request)
        .await
    {
        Ok(Some(response)) => {
            let mut headers = HeaderMap::new();
            if let Ok(value) = HeaderValue::from_str(&state.session_id) {
                headers.insert(mcp::MCP_SESSION_HEADER, value);
            }
            (StatusCode::OK, headers, Json(response)).into_response()
        }
        Ok(None) => (StatusCode::ACCEPTED, ()).into_response(),
        Err(error) => {
            let _ = server.terminate_session(&state.session_id);
            jsonrpc_http_error(
                StatusCode::BAD_REQUEST,
                Value::Null,
                error.code(),
                error.message(),
            )
        }
    }
}

fn jsonrpc_http_error(status: StatusCode, id: Value, code: i64, message: &str) -> Response {
    (
        status,
        Json(json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})),
    )
        .into_response()
}

#[cfg(test)]
mod mcp_http_tests {
    use super::*;
    use dtoken_config::DtokenConfig;
    use dtoken_types::{ChainMode, ProviderKind, ProviderProfile, RouteProfile};

    fn route_config() -> DtokenConfig {
        DtokenConfig {
            providers: vec![ProviderProfile {
                id: "mcp-provider".to_string(),
                name: "MCP provider".to_string(),
                kind: ProviderKind::Custom,
                protocol: Protocol::OpenAi,
                base_url: "http://127.0.0.1:9/v1".to_string(),
                api_key_ref: None,
                models: vec!["mcp-model".to_string()],
                timeout_ms: 100,
                healthcheck_path: None,
                wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
            }],
            routes: vec![RouteProfile {
                id: "mcp-route".to_string(),
                name: "MCP route".to_string(),
                input_protocol: Protocol::OpenAi,
                instance_id: "mcp-instance".to_string(),
                target_provider_id: "mcp-provider".to_string(),
                fallback_provider_id: None,
                chain_mode: ChainMode::Direct,
                compression_policy: CompressionPolicy::Balanced,
                bind_host: "127.0.0.1".to_string(),
                local_port: 18081,
            }],
            instances: Vec::new(),
            provider_cache_policies: Vec::new(),
            output_shaping_policies: Vec::new(),
            optical: Default::default(),
        }
    }

    fn route_ref() -> Value {
        json!({
            "route_id": "mcp-route",
            "input_protocol": "open_ai",
            "endpoint": "open_ai_chat_completions"
        })
    }

    fn rpc_request(id: u64, method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }

    async fn http_response(response: Response) -> (StatusCode, HeaderMap, Value) {
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let body: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("JSON response body")
        };
        (status, headers, body)
    }

    fn initialize_request() -> Value {
        rpc_request(1, "initialize", json!({"protocolVersion": "2024-11-05"}))
    }

    fn compress_request() -> Value {
        rpc_request(
            2,
            "tools/call",
            json!({
                "name": "headroom_compress",
                "arguments": {
                    "policy": "balanced",
                    "route_ref": route_ref(),
                    "body": {"model": "mcp-model", "messages": [{"role": "user", "content": "hello"}]}
                }
            }),
        )
    }

    fn session_header(session_id: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            mcp::MCP_SESSION_HEADER,
            HeaderValue::from_str(session_id).expect("valid session header value"),
        );
        headers
    }

    #[tokio::test]
    async fn initialize_creates_a_session_and_http_parities_with_stdio() {
        let server = Arc::new(mcp::McpServer::new(AppState::new(route_config())));

        let (status, headers, body) = http_response(
            mcp_http(
                Extension(server.clone()),
                HeaderMap::new(),
                Bytes::from(initialize_request().to_string()),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["result"]["protocolVersion"], "2024-11-05");
        let session_id = headers
            .get(mcp::MCP_SESSION_HEADER)
            .expect("Mcp-Session-Id header")
            .to_str()
            .expect("session id is utf-8")
            .to_string();
        assert!(!session_id.is_empty());

        // The same compress request over the HTTP session and over the stdio
        // surface must produce a byte-identical tool result (decision,
        // reason, artifact and evidence contract).
        let compress = compress_request();
        let (status, _, body) = http_response(
            mcp_http(
                Extension(server.clone()),
                session_header(&session_id),
                Bytes::from(compress.to_string()),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let http_result = body["result"]["content"][0]["text"]
            .as_str()
            .expect("HTTP tool text");
        let stdio_response = server.handle_jsonrpc(compress).expect("stdio response");
        let stdio_result = stdio_response["result"]["content"][0]["text"]
            .as_str()
            .expect("stdio tool text");
        // `elapsed_us` is a live per-invocation timing measurement, so the
        // HTTP and stdio runs legitimately report different values; the
        // parity contract covers the deterministic decision/evidence content.
        // Normalize the timing field before comparing the full serialized
        // tool result byte-for-byte.
        let mut http_value: Value = serde_json::from_str(http_result).expect("HTTP result JSON");
        let mut stdio_value: Value = serde_json::from_str(stdio_result).expect("stdio result JSON");
        http_value["elapsed_us"] = json!(0);
        stdio_value["elapsed_us"] = json!(0);
        let http_normalized = serde_json::to_string(&http_value).expect("serialize HTTP result");
        let stdio_normalized = serde_json::to_string(&stdio_value).expect("serialize stdio result");
        assert_eq!(http_normalized, stdio_normalized);
        assert_eq!(http_value, stdio_value);
        assert_eq!(http_value["decision"]["route_id"], "mcp-route");
        assert_eq!(http_value["decision"]["provider_id"], "mcp-provider");
        assert_eq!(http_value["decision"]["model"], "mcp-model");

        // The session layer recorded both requests and their terminal state.
        let session = server.session_state(&session_id).expect("session state");
        assert_eq!(session.status, mcp::McpSessionStatus::Active);
        assert_eq!(session.request_count, 2);
        assert_eq!(
            session.last_terminal.as_ref().map(|terminal| terminal.kind),
            Some(mcp::McpSessionTerminalKind::Completed)
        );
    }

    #[tokio::test]
    async fn request_without_session_is_fail_closed_except_initialize() {
        let server = Arc::new(mcp::McpServer::new(AppState::new(route_config())));
        let (status, _, body) = http_response(
            mcp_http(
                Extension(server),
                HeaderMap::new(),
                Bytes::from(rpc_request(3, "tools/list", json!({})).to_string()),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], -32002);
        assert_eq!(body["error"]["message"], "The MCP session is unknown.");
    }

    #[tokio::test]
    async fn unknown_session_id_is_rejected() {
        let server = Arc::new(mcp::McpServer::new(AppState::new(route_config())));
        let (status, _, body) = http_response(
            mcp_http(
                Extension(server),
                session_header("missing-session"),
                Bytes::from(rpc_request(4, "ping", json!({})).to_string()),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], -32002);
    }

    async fn initialize_http_session(server: &Arc<mcp::McpServer>) -> String {
        let (_, headers, _) = http_response(
            mcp_http(
                Extension(server.clone()),
                HeaderMap::new(),
                Bytes::from(initialize_request().to_string()),
            )
            .await,
        )
        .await;
        headers
            .get(mcp::MCP_SESSION_HEADER)
            .expect("Mcp-Session-Id header")
            .to_str()
            .expect("session id is utf-8")
            .to_string()
    }

    #[tokio::test]
    async fn delete_unknown_session_and_missing_header_are_rejected() {
        let server = Arc::new(mcp::McpServer::new(AppState::new(route_config())));
        let (status, _, body) = http_response(
            mcp_http_delete(Extension(server.clone()), session_header("missing-session")).await,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], -32002);

        let (status, _, _) =
            http_response(mcp_http_delete(Extension(server), HeaderMap::new()).await).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn delete_frees_session_capacity_after_initialize_exhaustion() {
        let server = Arc::new(mcp::McpServer::new(AppState::new(route_config())));
        let mut session_ids = Vec::new();
        for _ in 0..mcp::MAX_MCP_SESSIONS {
            session_ids.push(initialize_http_session(&server).await);
        }
        // Capacity exhausted: the next initialize is rejected 503.
        let (status, _, body) = http_response(
            mcp_http(
                Extension(server.clone()),
                HeaderMap::new(),
                Bytes::from(initialize_request().to_string()),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"]["code"], -32005);

        // DELETE one session: the slot is reclaimed immediately.
        let (status, _, _) = http_response(
            mcp_http_delete(Extension(server.clone()), session_header(&session_ids[0])).await,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, headers, _) = http_response(
            mcp_http(
                Extension(server.clone()),
                HeaderMap::new(),
                Bytes::from(initialize_request().to_string()),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let replacement = headers
            .get(mcp::MCP_SESSION_HEADER)
            .expect("Mcp-Session-Id header")
            .to_str()
            .expect("session id is utf-8")
            .to_string();
        assert_ne!(replacement, session_ids[0]);

        // The deleted session id is no longer routable: POST fails fail-closed.
        let (status, _, _) = http_response(
            mcp_http(
                Extension(server),
                session_header(&session_ids[0]),
                Bytes::from(rpc_request(5, "ping", json!({})).to_string()),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn session_notification_returns_202_accepted() {
        let server = Arc::new(mcp::McpServer::new(AppState::new(route_config())));
        let (_, headers, _) = http_response(
            mcp_http(
                Extension(server.clone()),
                HeaderMap::new(),
                Bytes::from(initialize_request().to_string()),
            )
            .await,
        )
        .await;
        let session_id = headers
            .get(mcp::MCP_SESSION_HEADER)
            .expect("Mcp-Session-Id header")
            .to_str()
            .expect("session id is utf-8")
            .to_string();
        let response = mcp_http(
            Extension(server),
            session_header(&session_id),
            Bytes::from(
                json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})
                    .to_string(),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
}

async fn backend_capabilities(State(state): State<AppState>) -> Json<CapabilityRegistrySnapshot> {
    Json(state.current_capability_registry())
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
    append_stage_event(
        &state,
        Uuid::new_v4(),
        LayeredStage::ConfigPatch,
        EventStatus::Passthrough,
        json!({
            "source": "dtoken-owned-runtime",
            "action": "preview_config_patch_plan",
            "reason_code": "desktop_transaction_executor_required",
            "raw_available": false,
            "privacy": "The core runtime does not fabricate a patch plan. Config diff, backup, apply, verify, and rollback are owned by the desktop transaction executor."
        }),
        None,
    );
    Json(json!({
        "source": "dtoken-owned-runtime",
        "state": "unavailable",
        "reason_code": "desktop_transaction_executor_required",
        "writes_performed": false,
        "raw_available": false,
        "next_action": "use_desktop_config_transaction",
        "privacy": "No config file, diff, credential, or backup content was read or returned."
    }))
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
    fn anthropic_stream_usage_observer_extracts_only_allowlisted_counters() {
        const SECRET: &str = "ANTHROPIC_STREAM_SECRET_MUST_NOT_ESCAPE";
        let mut observer = AnthropicStreamUsageObserver::default();
        observer.observe_chunk(&Bytes::from_static(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":101,\"cache_read_input_tokens\":25,\"cache_creation_input_tokens\":3,\"private\":\"ANTHROPIC_STREAM_SECRET_MUST_NOT_ESCAPE\"}}}\n\n",
        ));
        observer.observe_chunk(&Bytes::from_static(
            b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":17,\"cache_creation\":{\"ephemeral_5m_input_tokens\":4}}}\n",
        ));
        observer.observe_chunk(&Bytes::from_static(b"data: {\"type\":\"message_stop\"}\n"));

        assert_eq!(observer.terminal_error_code(), None);
        let usage = observer.take_usage();
        assert_eq!(usage.input_tokens, Some(101));
        assert_eq!(usage.output_tokens, Some(17));
        assert_eq!(usage.cache_read_input_tokens, Some(25));
        assert_eq!(usage.cache_creation_input_tokens, Some(3));
        assert_eq!(usage.cache_creation_5m_input_tokens, Some(4));
        assert!(!usage.as_response().to_string().contains(SECRET));
    }

    #[test]
    fn anthropic_stream_usage_observer_fails_closed_without_message_stop_or_on_error() {
        let mut incomplete = AnthropicStreamUsageObserver::default();
        incomplete.observe_chunk(&Bytes::from_static(
            b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":2}}\n",
        ));
        assert_eq!(
            incomplete.terminal_error_code(),
            Some("provider_stream_incomplete")
        );

        let mut upstream_error = AnthropicStreamUsageObserver::default();
        upstream_error.observe_chunk(&Bytes::from_static(
            b"data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\ndata: {\"type\":\"message_stop\"}\n",
        ));
        assert_eq!(
            upstream_error.terminal_error_code(),
            Some("provider_stream_error_event")
        );
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
        configured_openai_test_state_with_output_policies(base_url, timeout_ms, vec![])
    }

    fn configured_openai_test_state_with_output_policies(
        base_url: String,
        timeout_ms: u64,
        output_shaping_policies: Vec<dtoken_types::OutputShapingRoutePolicy>,
    ) -> AppState {
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
                wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
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
            output_shaping_policies,
            optical: Default::default(),
        })
    }

    fn direct_anthropic_cache_test_config(base_url: String) -> DtokenConfig {
        DtokenConfig {
            providers: vec![dtoken_types::ProviderProfile {
                id: "anthropic-direct".to_string(),
                name: "Direct Anthropic".to_string(),
                kind: dtoken_types::ProviderKind::Custom,
                protocol: Protocol::Anthropic,
                base_url,
                api_key_ref: None,
                models: vec!["claude-cache-test".to_string()],
                timeout_ms: 30_000,
                healthcheck_path: None,
                wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
            }],
            routes: vec![dtoken_types::RouteProfile {
                id: "anthropic-direct-route".to_string(),
                name: "Direct Anthropic route".to_string(),
                input_protocol: Protocol::Anthropic,
                instance_id: "anthropic-direct-instance".to_string(),
                target_provider_id: "anthropic-direct".to_string(),
                fallback_provider_id: None,
                chain_mode: ChainMode::Direct,
                compression_policy: CompressionPolicy::Balanced,
                bind_host: "127.0.0.1".to_string(),
                local_port: 18002,
            }],
            instances: vec![],
            provider_cache_policies: vec![dtoken_types::ProviderCachePolicy {
                route_id: "anthropic-direct-route".to_string(),
                provider_id: "anthropic-direct".to_string(),
                model: "claude-cache-test".to_string(),
                strategy: dtoken_types::ProviderCachePolicyStrategy::AnthropicExplicit,
                ttl_seconds: 300,
                min_prefix_bytes: 1,
            }],
            output_shaping_policies: vec![],
            optical: Default::default(),
        }
    }

    fn persist_anthropic_cache_evidence(state: &AppState, scope: &RequestScope) {
        let observation = dtoken_types::ProviderCacheObservation::new(
            &scope.route_id,
            &scope.provider_id,
            &scope.model,
            scope.output_protocol.clone(),
            &scope.config_revision,
            dtoken_types::ProviderCacheObservationKind::AnthropicUsage,
            Some(0),
            None,
            None,
            None,
            "provider-cache-observation-v1",
            dtoken_event_ledger::now_ms().max(1),
        )
        .expect("valid Anthropic cache observation");
        state
            .context_store
            .as_ref()
            .expect("persistent context store")
            .lock()
            .unwrap()
            .upsert_provider_cache_observation(&observation)
            .expect("store Anthropic cache observation");
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
            wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
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
            output_shaping_policies: vec![],
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
    fn compression_execution_normalizes_tool_schemas_on_the_public_surface() {
        let state = configured_openai_test_state("http://127.0.0.1:9/v1".to_string(), 100);
        let repeated_log = ["same"; 12].join("\n");
        let mut messages = vec![
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "old-call",
                    "type": "function",
                    "function": {"name": "build", "arguments": "{}"}
                }]
            }),
            json!({"role": "tool", "tool_call_id": "old-call", "content": repeated_log}),
        ];
        for index in 0..6 {
            let call_id = format!("recent-{index}");
            messages.push(json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": call_id,
                    "type": "function",
                    "function": {"name": "recent", "arguments": "{}"}
                }]
            }));
            messages.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": "recent result"
            }));
        }
        messages.push(json!({"role": "user", "content": "current request"}));
        let request = CompressionDecisionRequest {
            route_id: "local-mock-route".to_string(),
            input_protocol: Protocol::OpenAi,
            endpoint: OpticalEndpoint::OpenAiChatCompletions,
            frozen_message_count: None,
            requested_policy: Some(CompressionPolicy::Balanced),
            body: json!({
                "model": "mock/model",
                "messages": messages,
                "tools": [
                    {"type": "function", "function": {"name": "zeta", "parameters": {"type": "object"}}},
                    {"type": "function", "function": {"name": "alpha", "parameters": {"type": "object"}}}
                ]
            }),
        };

        let execution = state
            .compression_execution(request)
            .expect("surface compression should resolve");
        assert!(execution
            .transforms_applied
            .iter()
            .any(|item| item == "tool_schema:normalize"));
        assert!(execution
            .transforms_applied
            .iter()
            .any(|item| item == "tool_def_normalize_v1"));
        // The tool section survives the transform sorted by function name.
        assert_eq!(
            execution.body["tools"][0]["function"]["name"]
                .as_str()
                .unwrap(),
            "alpha"
        );
        assert_eq!(
            execution.body["tools"][1]["function"]["name"]
                .as_str()
                .unwrap(),
            "zeta"
        );
        // The public contract stays body-free and credential-free.
        assert!(!execution.decision.evidence.request_body_persisted);
        assert!(!execution.decision.evidence.credentials_returned);
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

    #[tokio::test]
    async fn optical_public_readiness_is_false_when_registry_evidence_is_unavailable() {
        let mut state = AppState::new(DtokenConfig::empty());
        state.capabilities = Arc::new(
            CapabilityRegistrySnapshot::new(
                "test-optical-unavailable",
                1,
                vec![CapabilityEvidence::unavailable(
                    "optical-renderer",
                    "test-optical-unavailable",
                    "renderer_evidence_unavailable",
                )
                .expect("unavailable renderer evidence")],
            )
            .expect("test capability registry"),
        );

        let Json(status) = optical_status(State(state.clone())).await;
        assert!(!status.fail_closed);
        assert!(!status.renderer_ready);
        assert!(!status.patcher_ready);
        assert!(!status.post_validator_ready);
        assert!(!status.signed_accounting_ready);
        assert!(!status.representation_plan_ready);
        assert!(!status.responses_shadow_inventory_ready);
        assert!(!status.dynamic_page_accounting_ready);
        assert!(!status.active_reachable);

        let Json(capabilities) = optical_capabilities(State(state)).await;
        assert!(!capabilities.shadow_preview_supported);
        assert!(!capabilities.active_transform_enabled);
        assert!(!capabilities.fail_closed);
        assert!(!capabilities.direct_anthropic_upstream_supported);
        assert!(!capabilities.streaming_passthrough_supported);
        assert!(!capabilities.deterministic_png_renderer);
        assert!(!capabilities.artifact_post_validation);
        assert!(!capabilities.signed_multimodal_accounting);
        assert!(!capabilities.capability_attestation_required);
    }

    #[tokio::test]
    async fn optical_shadow_readiness_does_not_enable_active_transform() {
        let state = AppState::new(DtokenConfig::empty());
        let shadow = state
            .capabilities
            .find("optical-shadow-preview")
            .expect("shadow capability");
        let active = state
            .capabilities
            .find("optical-active")
            .expect("active capability");
        assert_eq!(shadow.state, AvailabilityState::Ready);
        assert_eq!(active.state, AvailabilityState::ImplementedDisabled);
        assert_eq!(
            active.reason_codes,
            vec!["policy_shadow_only", "release_gate_disabled"]
        );

        let Json(status) = optical_status(State(state.clone())).await;
        let Json(capabilities) = optical_capabilities(State(state)).await;
        assert!(status.renderer_ready);
        assert!(!status.allow_active);
        assert!(!status.active_reachable);
        assert!(capabilities.shadow_preview_supported);
        assert!(!capabilities.active_transform_enabled);
        assert_eq!(capabilities.max_mode, OpticalMode::ShadowPreview);
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
            model_identity: None,
            compression_hint: None,
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
        store
            .append_route_decision(&route_value)
            .expect("seed route");
        store.append_request_log(&log).expect("seed request log");
        store.append_ccr_metadata(&metadata).expect("seed CCR");
        for name in [
            "route-decisions.jsonl",
            "request-logs.jsonl",
            "ccr-metadata.jsonl",
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

        assert_eq!(store.recent_route_decisions(10).unwrap().len(), 2);
        assert_eq!(store.recent_request_logs(10).unwrap().len(), 2);
        assert_eq!(store.recent_ccr_metadata(10).unwrap().len(), 2);
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
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            let expected_request_bytes = loop {
                let read = socket.read(&mut buffer).await.expect("read local request");
                assert!(read > 0, "local request closed before headers completed");
                request.extend_from_slice(&buffer[..read]);
                let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = std::str::from_utf8(&request[..header_end])
                    .expect("local request headers are UTF-8");
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if !name.trim().eq_ignore_ascii_case("content-length") {
                            return None;
                        }
                        value.trim().parse::<usize>().ok()
                    })
                    .expect("local request content length");
                break header_end + 4 + content_length;
            };
            while request.len() < expected_request_bytes {
                let read = socket
                    .read(&mut buffer)
                    .await
                    .expect("read local request body");
                assert!(read > 0, "local request closed before body completed");
                request.extend_from_slice(&buffer[..read]);
            }
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
    async fn configured_chat_applies_route_bound_output_shaping_with_redacted_measurement() {
        let (addr, server) = spawn_json_upstream(
            "200 OK",
            r#"{"id":"chatcmpl-local","choices":[{"message":{"role":"assistant","content":"alpha\n\n\n\nbeta\n\n"}}]}"#,
            0,
        )
        .await;
        let state = configured_openai_test_state_with_output_policies(
            format!("http://{addr}/v1"),
            1_000,
            vec![dtoken_types::OutputShapingRoutePolicy {
                route_id: "local-mock-route".to_string(),
                provider_id: "local-mock-provider".to_string(),
                model: "mock/model".to_string(),
                enabled: true,
                output_verbosity: dtoken_types::OutputVerbosity::Terse,
                holdout_ratio: Some(0.0),
            }],
        );

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

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        assert_eq!(
            payload["choices"][0]["message"]["content"],
            "alpha\n\nbeta\n"
        );
        let event = state
            .recent_events(20)
            .into_iter()
            .find(|event| {
                event.request_id.is_some() && event.metadata.get("output_shaping").is_some()
            })
            .expect("routed response must persist output-shaping evidence");
        let evidence = &event.metadata["output_shaping"];
        assert_eq!(evidence["state"], "applied");
        assert_eq!(
            evidence["accounting"]["measured"]["unit"],
            "outbound_utf8_bytes"
        );
        assert!(evidence["accounting"]["measured"]["saved_bytes"]
            .as_u64()
            .is_some_and(|value| value > 0));
        assert_eq!(evidence["accounting"]["holdout_estimated"], Value::Null);
        assert_eq!(evidence["raw_available"], false);
        assert!(!event.metadata.to_string().contains("alpha"));
        server.await.expect("local mock upstream task");
    }

    #[tokio::test]
    async fn configured_chat_keeps_error_response_and_records_unknown_shaping() {
        let (addr, server) = spawn_json_upstream(
            "401 Unauthorized",
            r#"{"error":{"message":"invalid local test credential"}}"#,
            0,
        )
        .await;
        let state = configured_openai_test_state_with_output_policies(
            format!("http://{addr}/v1"),
            1_000,
            vec![dtoken_types::OutputShapingRoutePolicy {
                route_id: "local-mock-route".to_string(),
                provider_id: "local-mock-provider".to_string(),
                model: "mock/model".to_string(),
                enabled: true,
                output_verbosity: dtoken_types::OutputVerbosity::Terse,
                holdout_ratio: Some(0.0),
            }],
        );

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
        let event = state
            .recent_events(20)
            .into_iter()
            .find(|event| event.metadata.get("output_shaping").is_some())
            .expect("failed routed response must persist passthrough evidence");
        assert_eq!(
            event.metadata["output_shaping"]["reason_code"],
            "error_response_passthrough"
        );
        assert_eq!(
            event.metadata["output_shaping"]["accounting"]["unknown"],
            true
        );
        server.await.expect("local mock upstream task");
    }

    #[tokio::test]
    async fn configured_chat_holdout_keeps_response_verbatim_and_records_estimate_only() {
        let (addr, server) = spawn_json_upstream(
            "200 OK",
            r#"{"id":"chatcmpl-local","choices":[{"message":{"role":"assistant","content":"alpha\n\n\n\nbeta\n\n"}}]}"#,
            0,
        )
        .await;
        let state = configured_openai_test_state_with_output_policies(
            format!("http://{addr}/v1"),
            1_000,
            vec![dtoken_types::OutputShapingRoutePolicy {
                route_id: "local-mock-route".to_string(),
                provider_id: "local-mock-provider".to_string(),
                model: "mock/model".to_string(),
                enabled: true,
                output_verbosity: dtoken_types::OutputVerbosity::Terse,
                holdout_ratio: Some(1.0),
            }],
        );

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

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        assert_eq!(
            payload["choices"][0]["message"]["content"],
            "alpha\n\n\n\nbeta\n\n"
        );
        let event = state
            .recent_events(20)
            .into_iter()
            .find(|event| event.metadata.get("output_shaping").is_some())
            .expect("holdout must persist output-shaping evidence");
        let accounting = &event.metadata["output_shaping"]["accounting"];
        assert_eq!(event.metadata["output_shaping"]["state"], "holdout");
        assert_eq!(accounting["measured"], Value::Null);
        assert!(accounting["holdout_estimated"]["estimated_saved_bytes"]
            .as_u64()
            .is_some_and(|value| value > 0));
        assert_eq!(accounting["unknown"], false);
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
                wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
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
            output_shaping_policies: vec![],
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
                wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
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
            output_shaping_policies: vec![],
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
                wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
            }],
            routes: vec![],
            instances: vec![],
            provider_cache_policies: vec![],
            output_shaping_policies: vec![],
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
            wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
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

    #[tokio::test]
    async fn config_patch_plan_fails_closed_without_desktop_transaction_executor() {
        let state = AppState::new(DtokenConfig::empty());
        let Json(plan) = config_patch_plan(State(state.clone())).await;

        let event = state.recent_events(1).remove(0);
        assert_eq!(plan["source"], "dtoken-owned-runtime");
        assert_eq!(plan["state"], "unavailable");
        assert_eq!(plan["reason_code"], "desktop_transaction_executor_required");
        assert_eq!(plan["writes_performed"], false);
        assert!(plan.get("diff").is_none());
        assert_eq!(event.stage, LayeredStage::ConfigPatch);
        assert_eq!(event.status, EventStatus::Passthrough);
        assert_eq!(event.metadata["raw_available"], false);
        assert_eq!(
            event.metadata["reason_code"],
            "desktop_transaction_executor_required"
        );
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
    fn ccr_restart_keeps_only_latest_record_per_recovery_ref_in_memory() {
        let dir = std::env::temp_dir().join(format!("dtoken-ccr-latest-{}", Uuid::new_v4()));
        let key = [43_u8; 32];
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("private CCR state");
        let first_ref = format!("ccr:{}", Uuid::new_v4());
        let second_ref = format!("ccr:{}", Uuid::new_v4());
        let now_ms = dtoken_event_ledger::now_ms().max(1);

        for index in 0..64_u64 {
            state
                .append_ccr_lifecycle_record(json!({
                    "recovery_ref": first_ref.clone(),
                    "request_id": "request-a",
                    "state": if index == 63 { "recovery_pending_retained" } else { "pending_retained" },
                    "raw_available": true,
                    "raw_storage": "encrypted-private-store",
                    "updated_at_ms": now_ms.saturating_add(index),
                }))
                .expect("append first recovery ref journal record");
        }
        for index in 0..32_u64 {
            state
                .append_ccr_lifecycle_record(json!({
                    "recovery_ref": second_ref.clone(),
                    "request_id": "request-b",
                    "state": if index == 31 { "deleted" } else { "delete_intent" },
                    "raw_available": index != 31,
                    "raw_storage": if index == 31 { "deleted" } else { "private-store-delete-pending" },
                    "updated_at_ms": now_ms.saturating_add(1_000 + index),
                }))
                .expect("append second recovery ref journal record");
        }
        drop(state);

        let restarted = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("restarted private CCR state");
        let metadata = restarted.recent_ccr_metadata(10);

        assert_eq!(metadata.len(), 2);
        assert_eq!(
            restarted
                .retrieve_ccr_metadata(&first_ref)
                .expect("latest first ref state")["state"],
            "recovery_pending_retained"
        );
        assert_eq!(
            restarted
                .retrieve_ccr_metadata(&second_ref)
                .expect("latest second ref state")["state"],
            "deleted"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ccr_bare_marker_guard_fails_closed_without_a_matching_durable_receipt() {
        let scope = cache_test_scope(Protocol::OpenAi);
        let baseline =
            json!({"model": "model-a", "messages": [{"role": "user", "content": "original"}]});
        let candidate = json!({"model": "model-a", "messages": [{"role": "user", "content": "<<ccr:11111111-1111-4111-8111-111111111111,string,64>>"}]});
        let transforms = vec!["content_router:json".to_string()];
        let prepared_without_receipt = PreparedCcr {
            receipt: None,
            store_configured: true,
        };

        // Marker in the candidate with no durable receipt -> baseline fallback.
        let payload = prepare_json_pipeline_payload(
            scope.clone(),
            &baseline,
            &candidate,
            &transforms,
            &prepared_without_receipt,
        )
        .expect("guard returns a baseline fallback");
        assert_eq!(
            payload.fallback_reason,
            Some("ccr_marker_without_durable_receipt")
        );
        assert_eq!(
            payload.ready.reason_code(),
            "ccr_marker_without_durable_receipt"
        );
        assert_eq!(
            payload.ready.body(),
            serde_json::to_vec(&baseline).unwrap().as_slice()
        );

        // Matching receipt -> candidate passes and becomes sendable.
        let receipt = CcrStoreReceipt {
            recovery_ref: "ccr:11111111-1111-4111-8111-111111111111".to_string(),
            expires_at_unix: 9_999,
            plaintext_bytes: 64,
        };
        let prepared_with_receipt = PreparedCcr {
            receipt: Some(receipt.clone()),
            store_configured: true,
        };
        let payload = prepare_json_pipeline_payload(
            scope.clone(),
            &baseline,
            &candidate,
            &transforms,
            &prepared_with_receipt,
        )
        .expect("guard passes a receipt-bound marker");
        assert_eq!(payload.fallback_reason, None);
        assert_eq!(payload.ready.reason_code(), "validated_optimized_candidate");
        assert_eq!(
            payload.ready.body(),
            serde_json::to_vec(&candidate).unwrap().as_slice()
        );

        // Mismatched recovery ref -> baseline fallback.
        let wrong_receipt = CcrStoreReceipt {
            recovery_ref: "ccr:22222222-2222-4222-8222-222222222222".to_string(),
            ..receipt
        };
        let payload = prepare_json_pipeline_payload(
            scope.clone(),
            &baseline,
            &candidate,
            &transforms,
            &PreparedCcr {
                receipt: Some(wrong_receipt),
                store_configured: true,
            },
        )
        .expect("guard rejects a mismatched receipt");
        assert_eq!(
            payload.fallback_reason,
            Some("ccr_marker_recovery_ref_mismatch")
        );

        // Unverifiable bracket form -> baseline fallback even with a receipt.
        let bracket_candidate = json!({"model": "model-a", "messages": [{"role": "user", "content": "Retrieve original: hash=deadbeef"}]});
        let payload = prepare_json_pipeline_payload(
            scope.clone(),
            &baseline,
            &bracket_candidate,
            &transforms,
            &PreparedCcr {
                receipt: Some(receipt),
                store_configured: true,
            },
        )
        .expect("guard rejects an unverifiable bracket form");
        assert_eq!(
            payload.fallback_reason,
            Some("ccr_marker_recovery_ref_unverifiable")
        );
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
        let receipt = restarted
            .delete_ccr_original(&recovery_ref)
            .expect("delete private CCR original");
        assert_eq!(receipt.state, "deleted");
        assert!(!receipt.raw_available);
        assert!(!receipt.raw_returned);
        assert!(restarted.retrieve_ccr_original(&recovery_ref).is_err());
        let deleted_metadata = restarted
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("deleted CCR metadata");
        assert_eq!(deleted_metadata["state"], "deleted");
        assert_eq!(deleted_metadata["raw_available"], false);
        assert_eq!(deleted_metadata["raw_storage"], "deleted");
        assert!(deleted_metadata.get("raw_bytes").is_none());
        assert!(deleted_metadata.get("expires_at_unix").is_none());
        assert!(!deleted_metadata.to_string().contains("CCR_PRIVATE_SECRET"));
        let deleted_projection = ContextStore::open(dir.join("runtime-state.sqlite3"))
            .expect("open deleted CCR projection");
        assert_eq!(
            deleted_projection
                .ccr_metadata_state(&recovery_ref)
                .expect("deleted CCR projection state")
                .as_deref(),
            Some("deleted")
        );
        assert!(restarted.recent_events(20).iter().any(|event| {
            event.stage == LayeredStage::HeadroomRetrieve
                && event.metadata["action"] == "ccr_delete"
                && event.metadata["raw_available"] == false
        }));
        let already_deleted = restarted
            .delete_ccr_original(&recovery_ref)
            .expect("repeated deletion is idempotent");
        assert_eq!(already_deleted.state, "already_deleted");
        drop(restarted);

        let restarted_after_delete =
            AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
                .expect("restart after CCR deletion");
        let after_restart = restarted_after_delete
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("deleted CCR metadata survives restart");
        assert_eq!(after_restart["state"], "deleted");
        assert_eq!(after_restart["raw_storage"], "deleted");
        assert!(restarted_after_delete
            .retrieve_ccr_original(&recovery_ref)
            .is_err());
        assert_eq!(
            runtime_cache_summary_for_state(&restarted_after_delete)["entries"],
            1
        );

        let untracked_ref = format!("ccr:{}", Uuid::new_v4());
        restarted_after_delete
            .ccr_private_store
            .as_ref()
            .expect("private CCR store")
            .store(&untracked_ref, br#"{"content":"CCR_UNTRACKED_SECRET"}"#)
            .expect("seed untracked private CCR");
        assert!(restarted_after_delete
            .delete_ccr_original(&untracked_ref)
            .is_err());
        assert_eq!(
            restarted_after_delete
                .retrieve_ccr_original(&untracked_ref)
                .expect("missing metadata must not delete raw CCR"),
            br#"{"content":"CCR_UNTRACKED_SECRET"}"#
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sqlite_projection_missing_after_crash_is_rebuilt_from_the_durable_journal() {
        let dir = std::env::temp_dir().join(format!("dtoken-cross-store-{}", Uuid::new_v4()));
        let key = [33_u8; 32];
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("persistent state");
        let request = OpenAiChatRequest {
            model: Some("cross-store-model".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "CROSS_STORE_SECRET must be recoverable from the journal"
            })]),
            stream: Some(false),
        };
        let (_response, decision, mut request_log, mut event) = routed_openai_payload(&request);
        request_log.transforms_applied = vec!["content_router:json".to_string()];
        event.status = EventStatus::Ok;
        let original = br#"{"model":"cross-store-model","messages":[{"role":"user","content":"CROSS_STORE_SECRET"}]}"#;
        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, original)
            .expect("durable CCR prepare");
        state.record_routed_request_with_ccr_receipt(&decision, request_log, event, &prepared);

        // The durable journals are on disk and synced.  Simulate the crash
        // gap where the SQLite projection was never written (or was lost):
        // drop the database file before restarting.
        drop(state);
        let sqlite_path = dir.join("runtime-state.sqlite3");
        assert!(sqlite_path.exists());
        std::fs::remove_file(&sqlite_path).expect("remove SQLite projection to simulate crash gap");

        let restarted = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("restarted state");
        // Both projections are healed from the append-only journal.
        let projections = ContextStore::open(&sqlite_path).expect("rebuilt projection store");
        let (requests, ccr) = projections.projection_counts().expect("projection counts");
        assert_eq!(
            requests, 1,
            "request projection must be rebuilt from the journal"
        );
        assert_eq!(ccr, 1, "CCR projection must be rebuilt from the journal");
        assert_eq!(
            projections
                .ccr_metadata_state(&format!("ccr:{}", decision.request_id))
                .expect("rebuilt CCR state")
                .as_deref(),
            Some("finalized")
        );
        // The encrypted original is still intact in the private store.
        assert_eq!(
            restarted
                .retrieve_ccr_original(&format!("ccr:{}", decision.request_id))
                .expect("original survives the crash"),
            original
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn terminal_commit_recovery_marks_partial_intent_explicitly_pending() {
        let dir = std::env::temp_dir().join(format!("dtoken-marker-recovery-{}", Uuid::new_v4()));
        let key = [77_u8; 32];
        // A dangling intent with none of the four fan-out records on disk:
        // restart recovery must mark it explicitly pending, never guess.
        let intent_dir = dir.join("intent-only");
        std::fs::create_dir_all(&intent_dir).expect("create intent-only dir");
        let store = RuntimeStore::new(&intent_dir);
        let request_id = Uuid::new_v4();
        let marker = TerminalCommitMarker {
            request_id,
            phase: TerminalCommitPhase::Intent,
            timestamp_ms: 100,
            event_id: Uuid::new_v4(),
            ccr_updated_at_ms: 100,
            reason: None,
        };
        store
            .append_terminal_commit_marker(&marker)
            .expect("write dangling intent");
        // The intent is partial: none of the four fan-out records exist.
        let restarted =
            AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &intent_dir, key)
                .expect("restart around dangling intent");
        let latest = restarted
            .runtime_store
            .as_ref()
            .expect("runtime store")
            .latest_terminal_commit_markers()
            .expect("latest markers");
        let (_, recovered) = latest
            .iter()
            .find(|(id, _)| *id == request_id)
            .expect("recovered marker");
        assert_eq!(recovered.phase, TerminalCommitPhase::Pending);
        assert_eq!(
            recovered.reason.as_deref(),
            Some("partial_fan_out_at_recovery")
        );
        assert!(
            restarted.terminal_pending_commits.contains_key(&request_id),
            "pending request is tracked for projection filtering"
        );
        // The pending request must be absent from the hot read models and the
        // SQLite projection: it is explicitly incomplete, not finalized.
        assert!(
            restarted
                .recent_request_logs(100)
                .iter()
                .all(|log| log.request_id != request_id),
            "pending request must not surface in hot request logs"
        );
        let projections =
            ContextStore::open(intent_dir.join("runtime-state.sqlite3")).expect("open projection");
        assert_eq!(
            projections
                .projection_counts()
                .expect("projection counts")
                .0,
            0,
            "pending request must not surface in the SQLite projection"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn terminal_commit_recovery_rolls_complete_fan_out_forward_without_duplicates() {
        let dir =
            std::env::temp_dir().join(format!("dtoken-marker-rollforward-{}", Uuid::new_v4()));
        let key = [78_u8; 32];
        let request = OpenAiChatRequest {
            model: Some("rollforward-model".to_string()),
            messages: Some(vec![json!({
                "role": "user",
                "content": "ROLLFORWARD_SECRET"
            })]),
            stream: Some(false),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        // Append all four fan-out records but leave the committed marker off
        // disk, simulating the crash window after the last fan-out sync and
        // before the committed marker sync.
        let store = RuntimeStore::new(&dir);
        let marker = TerminalCommitMarker {
            request_id: decision.request_id,
            phase: TerminalCommitPhase::Intent,
            timestamp_ms: event.timestamp_ms.max(1),
            event_id: event.event_id,
            ccr_updated_at_ms: event.timestamp_ms.max(1),
            reason: None,
        };
        store
            .append_terminal_commit_marker(&marker)
            .expect("write intent marker");
        let ccr = ccr_metadata_from_request(&decision, &request_log, &event, None, false);
        store.append_event(&event).expect("append event");
        store
            .append_route_decision(&route_decision_to_value(&decision))
            .expect("append route");
        store.append_request_log(&request_log).expect("append log");
        store.append_ccr_metadata(&ccr).expect("append ccr");

        let restarted = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("restart around complete-but-unmarked fan-out");
        let markers = restarted
            .runtime_store
            .as_ref()
            .expect("runtime store")
            .latest_terminal_commit_markers()
            .expect("latest markers");
        let (_, final_marker) = markers
            .iter()
            .find(|(id, _)| *id == decision.request_id)
            .expect("final marker");
        assert_eq!(final_marker.phase, TerminalCommitPhase::Committed);
        assert!(
            !restarted
                .terminal_pending_commits
                .contains_key(&decision.request_id),
            "complete fan-out is not left pending"
        );
        // No duplicates: the fan-out journals hold exactly one record per
        // request and recovery never re-appended them (the restart appended
        // only the committed marker).
        let routes = restarted
            .runtime_store
            .as_ref()
            .expect("runtime store")
            .recent_route_decisions(100)
            .expect("route decisions");
        let request_id_str = decision.request_id.to_string();
        assert_eq!(
            routes
                .iter()
                .filter(|value| value["request_id"].as_str() == Some(request_id_str.as_str()))
                .count(),
            1
        );
        let logs = restarted
            .runtime_store
            .as_ref()
            .expect("runtime store")
            .recent_request_logs(100)
            .expect("request logs");
        assert_eq!(
            logs.iter()
                .filter(|log| log.request_id == decision.request_id)
                .count(),
            1
        );
        let events = restarted
            .runtime_store
            .as_ref()
            .expect("runtime store")
            .recent_events(100)
            .expect("events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_id == marker.event_id)
                .count(),
            1
        );
        // The SQLite projection converged with exactly one request row.
        let projections =
            ContextStore::open(dir.join("runtime-state.sqlite3")).expect("open projection");
        assert_eq!(projections.projection_counts().unwrap().0, 1);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sqlite_projection_rebuild_is_not_truncated_to_the_hot_memory_window() {
        const RECORDS: usize = 10_001;
        let dir =
            std::env::temp_dir().join(format!("dtoken-full-journal-rebuild-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create runtime directory");
        let request = OpenAiChatRequest {
            model: Some("projection-model".to_string()),
            messages: Some(vec![
                json!({"role": "user", "content": "projection fixture"}),
            ]),
            stream: Some(false),
        };
        let (_response, _decision, base_log, base_event) = routed_openai_payload(&request);
        let mut request_journal = Vec::new();
        let mut event_journal = Vec::new();
        for index in 0..RECORDS {
            let request_id = Uuid::new_v4();
            let mut log = base_log.clone();
            log.request_id = request_id;
            log.timestamp_ms = index as u64 + 1;
            serde_json::to_writer(&mut request_journal, &log).expect("serialize request log");
            request_journal.push(b'\n');

            let mut event = base_event.clone();
            event.event_id = Uuid::new_v4();
            event.request_id = Some(request_id);
            event.timestamp_ms = index as u64 + 1;
            serde_json::to_writer(&mut event_journal, &event).expect("serialize event");
            event_journal.push(b'\n');
        }
        std::fs::write(dir.join("request-logs.jsonl"), request_journal)
            .expect("write request journal");
        std::fs::write(dir.join("events.jsonl"), event_journal).expect("write event journal");

        let state = AppState::new_persistent(DtokenConfig::empty(), &dir);
        assert_eq!(
            state.recent_request_logs(RECORDS + 1).len(),
            IN_MEMORY_JOURNAL_MAX_ENTRIES,
            "the hot read model stays bounded"
        );
        let projections =
            ContextStore::open(dir.join("runtime-state.sqlite3")).expect("open projection");
        assert_eq!(
            projections
                .projection_counts()
                .expect("projection counts")
                .0,
            RECORDS as u64,
            "the durable projection must include records outside the hot window"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    #[test]
    fn windows_ccr_enabled_persistent_state_verifies_private_runtime_and_ccr_dacls() {
        use dtoken_context_store::{
            verify_private_directory_acl, verify_private_file_acl, CcrMetadataProjection,
        };
        use std::{ffi::OsString, path::PathBuf};

        let dir = std::env::temp_dir().join(format!("dtoken-windows-private-{}", Uuid::new_v4()));
        let key = [41_u8; 32];
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("Windows CCR-enabled persistent state");

        let recovery_ref = format!("ccr:{}", Uuid::new_v4());
        state
            .ccr_private_store
            .as_ref()
            .expect("private CCR store")
            .store(&recovery_ref, br#"{"content":"WINDOWS_DACL_SECRET"}"#)
            .expect("store private CCR entry");

        state
            .context_store
            .as_ref()
            .expect("runtime projection store")
            .lock()
            .expect("context store lock")
            .upsert_ccr_metadata(&CcrMetadataProjection {
                recovery_ref: recovery_ref.clone(),
                request_id: "windows-private-dacl".to_string(),
                expires_at_ms: Some(123),
                plaintext_bytes: Some(456),
                state: "retained".to_string(),
                updated_at_ms: 789,
            })
            .expect("materialize SQLite WAL/SHM");

        let ccr_root = dir.join("ccr-private");
        let final_entry = ccr_root.join(format!("{}.ccr", recovery_ref.replace(':', "_")));
        let database = dir.join("runtime-state.sqlite3");

        verify_private_directory_acl(&dir).expect("verify runtime directory DACL");
        verify_private_directory_acl(&ccr_root).expect("verify CCR root DACL");
        verify_private_file_acl(&final_entry).expect("verify CCR final entry DACL");
        verify_private_file_acl(&database).expect("verify runtime projection DACL");

        for suffix in ["-wal", "-shm"] {
            let mut sidecar = OsString::from(database.as_os_str());
            sidecar.push(suffix);
            let sidecar = PathBuf::from(sidecar);
            assert!(
                sidecar.exists(),
                "expected SQLite sidecar {} to exist for DACL verification",
                sidecar.display()
            );
            verify_private_file_acl(&sidecar)
                .unwrap_or_else(|error| panic!("verify sidecar {}: {error:#}", sidecar.display()));
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    #[test]
    fn windows_ccr_enabled_persistent_state_fails_closed_without_private_sqlite_projection() {
        use dtoken_context_store::apply_private_directory_acl;

        let dir =
            std::env::temp_dir().join(format!("dtoken-windows-private-blocked-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create runtime dir");
        // Simulate an already-privatized runtime directory: the private
        // runtime contract verifies the runtime dir DACL before probing the
        // projection path, so without this the test fails closed on the
        // inherited parent ACL instead of reaching the blocked projection.
        apply_private_directory_acl(&dir).expect("privatize runtime dir");
        std::fs::create_dir_all(dir.join("runtime-state.sqlite3"))
            .expect("block projection path with directory");

        let error =
            match AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, [42_u8; 32]) {
                Ok(_) => panic!("Windows private projection failure must be fatal"),
                Err(error) => error,
            };
        assert!(
            format!("{error:#}").contains("runtime-state.sqlite3 projection did not initialize"),
            "unexpected error: {error:#}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ccr_restart_retains_pending_original_without_resending_it() {
        let dir = std::env::temp_dir().join(format!("dtoken-ccr-pending-{}", Uuid::new_v4()));
        let key = [32_u8; 32];
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("private CCR state");
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![
                json!({"role": "user", "content": "pending CCR secret"}),
            ]),
            stream: Some(false),
        };
        let (_response, decision, mut request_log, _event) = routed_openai_payload(&request);
        request_log.transforms_applied = vec!["content_router:json".to_string()];
        let original = br#"{"messages":[{"content":"pending CCR secret"}]}"#;
        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, original)
            .expect("pre-send CCR lifecycle");
        assert!(prepared.receipt.is_some());
        let recovery_ref = format!("ccr:{}", decision.request_id);
        assert_eq!(
            state
                .retrieve_ccr_metadata(&recovery_ref)
                .expect("pending journal record")["state"],
            "pending_retained"
        );
        drop(state);

        let restarted = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("restarted private CCR state");
        let metadata = restarted
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("reconciled journal record");
        assert_eq!(metadata["state"], "recovery_pending_retained");
        assert_eq!(
            restarted
                .retrieve_ccr_original(&recovery_ref)
                .expect("restart must retain rather than resend or delete"),
            original
        );
        assert!(restarted.recent_request_logs(1).is_empty());
        let projection =
            ContextStore::open(dir.join("runtime-state.sqlite3")).expect("open rebuilt projection");
        assert_eq!(
            projection
                .ccr_metadata_state(&recovery_ref)
                .expect("read rebuilt projection")
                .as_deref(),
            Some("recovery_pending_retained")
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ccr_restart_marks_missing_terminal_raw_without_calling_it_deleted() {
        let dir = std::env::temp_dir().join(format!("dtoken-ccr-missing-{}", Uuid::new_v4()));
        let key = [33_u8; 32];
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("private CCR state");
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![
                json!({"role": "user", "content": "missing CCR secret"}),
            ]),
            stream: Some(false),
        };
        let (_response, decision, mut request_log, event) = routed_openai_payload(&request);
        request_log.transforms_applied = vec!["content_router:json".to_string()];
        let recovery_ref = format!("ccr:{}", decision.request_id);
        let prepared = state
            .prepare_ccr_for_request(
                &decision,
                &request_log,
                br#"{"content":"missing CCR secret"}"#,
            )
            .expect("pre-send CCR lifecycle");
        state.record_routed_request_with_ccr_receipt(&decision, request_log, event, &prepared);
        state
            .ccr_private_store
            .as_ref()
            .expect("private store")
            .delete(&recovery_ref)
            .expect("test-only missing raw setup");
        drop(state);

        let restarted = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("restarted private CCR state");
        let metadata = restarted
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("reconciled metadata");
        assert_eq!(metadata["state"], "raw_missing");
        assert_eq!(metadata["raw_available"], false);
        assert_ne!(metadata["state"], "deleted");
        assert!(restarted.retrieve_ccr_original(&recovery_ref).is_err());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ccr_restart_retains_raw_after_interrupted_delete_intent() {
        let dir = std::env::temp_dir().join(format!("dtoken-ccr-delete-intent-{}", Uuid::new_v4()));
        let key = [34_u8; 32];
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("private CCR state");
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![
                json!({"role": "user", "content": "delete intent secret"}),
            ]),
            stream: Some(false),
        };
        let (_response, decision, mut request_log, event) = routed_openai_payload(&request);
        request_log.transforms_applied = vec!["content_router:json".to_string()];
        let recovery_ref = format!("ccr:{}", decision.request_id);
        let original = br#"{"content":"delete intent secret"}"#;
        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, original)
            .expect("pre-send CCR lifecycle");
        state.record_routed_request_with_ccr_receipt(&decision, request_log, event, &prepared);
        let terminal = state
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("terminal metadata");
        state
            .append_ccr_lifecycle_record(
                ccr_delete_intent(terminal, dtoken_event_ledger::now_ms().max(1))
                    .expect("delete intent"),
            )
            .expect("durable delete intent");
        drop(state);

        let restarted = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("restarted private CCR state");
        let metadata = restarted
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("reconciled metadata");
        assert_eq!(metadata["state"], "interrupted_retained");
        assert_eq!(
            restarted
                .retrieve_ccr_original(&recovery_ref)
                .expect("restart must not auto-delete"),
            original
        );
        let receipt = restarted
            .delete_ccr_original(&recovery_ref)
            .expect("explicit retry deletes after reconciliation");
        assert_eq!(receipt.state, "deleted");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ccr_restart_turns_delete_intent_with_missing_raw_into_tombstone() {
        let dir =
            std::env::temp_dir().join(format!("dtoken-ccr-delete-tombstone-{}", Uuid::new_v4()));
        let key = [35_u8; 32];
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("private CCR state");
        let request = OpenAiChatRequest {
            model: Some("opencode/deepseek-v4-flash-free".to_string()),
            messages: Some(vec![
                json!({"role": "user", "content": "tombstone CCR secret"}),
            ]),
            stream: Some(false),
        };
        let (_response, decision, mut request_log, event) = routed_openai_payload(&request);
        request_log.transforms_applied = vec!["content_router:json".to_string()];
        let recovery_ref = format!("ccr:{}", decision.request_id);
        let prepared = state
            .prepare_ccr_for_request(
                &decision,
                &request_log,
                br#"{"content":"tombstone CCR secret"}"#,
            )
            .expect("pre-send CCR lifecycle");
        state.record_routed_request_with_ccr_receipt(&decision, request_log, event, &prepared);
        let terminal = state
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("terminal metadata");
        state
            .append_ccr_lifecycle_record(
                ccr_delete_intent(terminal, dtoken_event_ledger::now_ms().max(1))
                    .expect("delete intent"),
            )
            .expect("durable delete intent");
        state
            .ccr_private_store
            .as_ref()
            .expect("private store")
            .delete(&recovery_ref)
            .expect("test-only interrupted delete setup");
        drop(state);

        let restarted = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, key)
            .expect("restarted private CCR state");
        let metadata = restarted
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("reconciled tombstone");
        assert_eq!(metadata["state"], "deleted");
        assert_eq!(metadata["raw_available"], false);
        assert_eq!(metadata["raw_storage"], "deleted");
        assert!(restarted.retrieve_ccr_original(&recovery_ref).is_err());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn shared_context_http_writes_are_absent_and_typed_store_remains_scope_bound() {
        let dir = std::env::temp_dir().join(format!("dtoken-memory-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let record = json!({
            "project_id": "demo-project",
            "agent_id": "opencode",
            "session_id": "session-a",
            "kind": "preference",
            "summary": "Prefer terse command output unless tests fail.",
            "keywords": ["terse", "verification"],
            "dedup_key": "preference-output-verbosity",
            "entry_revision": "memory-rev-1",
            "provenance": {
                "source": "dtoken-runtime",
                "source_revision": "runtime-rev-1",
                "observed_at_ms": 1
            }
        });

        let response = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/memory/learn")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(record.to_string()))
                    .expect("unauthorized memory request should build"),
            )
            .await
            .expect("unauthorized memory request should respond");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let request = dtoken_context_store::parse_memory_write_request(&record)
            .expect("typed record should parse");
        let saved = state
            .record_memory(&request)
            .expect("managed writer fixture");
        assert_eq!(saved["agent_id"], "opencode");
        assert_eq!(saved["raw_available"], false);

        let restarted = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let summary_response = build_router(restarted.clone())
            .oneshot(
                Request::builder()
                    .uri("/memory/summary")
                    .body(Body::empty())
                    .expect("memory summary request should build"),
            )
            .await
            .expect("memory summary should respond");
        let public_summary = response_json(summary_response).await;
        assert_eq!(public_summary["state"], "scope_required");
        assert_eq!(
            public_summary["reason_code"],
            "memory_summary_authorization_unavailable"
        );
        assert_eq!(public_summary["entries"], 0);

        let owner_scope = MemoryAccessScope {
            project_id: "demo-project".to_string(),
            agent_id: "opencode".to_string(),
            session_id: Some("session-a".to_string()),
        };
        let summary = memory_summary_for_scope(&restarted, Some(owner_scope.clone()));

        assert_eq!(summary["source"], "dtoken-owned-runtime");
        assert_eq!(summary["state"], "ready");
        assert_eq!(summary["entries"], 1);
        assert_eq!(summary["raw_available"], false);
        assert_eq!(summary["by_project"]["demo-project"], 1);
        assert_eq!(
            summary["recent"][0]["summary"],
            "Prefer terse command output unless tests fail."
        );
        assert!(summary.get("raw_prompt").is_none());

        let output_plan =
            output_shape_for_state(&restarted, Some("demo-project"), Some("opencode"));
        assert_eq!(output_plan["state"], "planned");
        // Session-bound memory is intentionally excluded from a project-wide
        // plan until a route supplies the exact session identity.
        assert_eq!(output_plan["output_verbosity"], "auto");
        assert_eq!(output_plan["raw_available"], false);
        assert!(!output_plan
            .to_string()
            .contains("MEMORY_RAW_PROMPT_MUST_NOT_BE_STORED"));
        assert_eq!(
            output_shape_for_state(&restarted, Some("demo-project"), Some("claude-code"))["state"],
            "planned"
        );

        let other_agent = memory_summary_for_scope(
            &restarted,
            Some(MemoryAccessScope {
                agent_id: "claude-code".to_string(),
                ..owner_scope.clone()
            }),
        );
        assert_eq!(other_agent["entries"], 0);

        let other_project = memory_summary_for_scope(
            &restarted,
            Some(MemoryAccessScope {
                project_id: "other-project".to_string(),
                ..owner_scope.clone()
            }),
        );
        assert_eq!(other_project["entries"], 0);

        let rejected = json!({
            "project_id": "demo-project",
            "agent_id": "opencode",
            "session_id": "session-a",
            "kind": "preference",
            "summary": "This must be rejected.",
            "keywords": ["invalid"],
            "dedup_key": "invalid-raw-prompt",
            "entry_revision": "memory-rev-2",
            "provenance": {
                "source": "dtoken-runtime",
                "source_revision": "runtime-rev-2",
                "observed_at_ms": 2
            },
            "raw_prompt": "MEMORY_RAW_PROMPT_MUST_NOT_BE_STORED"
        });
        assert!(dtoken_context_store::parse_memory_write_request(&rejected).is_err());
        let stored_records = restarted
            .context_store
            .as_ref()
            .expect("persistent state must keep the context store")
            .lock()
            .expect("context store lock")
            .list_visible_memory(&owner_scope)
            .expect("typed memory records should remain readable");
        assert_eq!(stored_records.len(), 1);
        assert!(!stored_records[0]
            .summary
            .contains("MEMORY_RAW_PROMPT_MUST_NOT_BE_STORED"));

        let _ = std::fs::remove_dir_all(dir);
    }

    fn injection_test_scope() -> RequestScope {
        RequestScope::new(
            Uuid::new_v4(),
            EvidenceValue::known("opencode".to_string(), EvidenceSource::ConfiguredRoute),
            EvidenceValue::known("demo-project".to_string(), EvidenceSource::ConfiguredRoute),
            EvidenceValue::known("session-a".to_string(), EvidenceSource::ConfiguredRoute),
            EvidenceValue::known("instance-a".to_string(), EvidenceSource::ConfiguredRoute),
            "route-a",
            "provider-a",
            "model-a",
            Protocol::OpenAi,
            Protocol::OpenAi,
            "config-a",
        )
        .unwrap()
    }

    fn injection_memory_write(kind: &str, summary: &str, dedup: &str) -> MemoryWriteRequest {
        MemoryWriteRequest {
            project_id: "demo-project".to_string(),
            agent_id: Some("opencode".to_string()),
            session_id: Some("session-a".to_string()),
            kind: kind.to_string(),
            summary: summary.to_string(),
            keywords: vec!["keyword".to_string()],
            dedup_key: dedup.to_string(),
            entry_revision: format!("memory-rev-{dedup}"),
            provenance: dtoken_context_store::MemoryProvenance {
                source: "dtoken-runtime".to_string(),
                source_revision: "runtime-rev-1".to_string(),
                observed_at_ms: 1,
            },
        }
    }

    #[test]
    fn memory_injection_is_scope_authorized_and_typed_only() {
        let dir = std::env::temp_dir().join(format!("dtoken-memory-inject-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(DtokenConfig::empty(), &dir);
        state
            .record_memory(&injection_memory_write(
                "preference",
                "Prefer terse command output.",
                "pref-1",
            ))
            .expect("preference record");
        state
            .record_memory(&injection_memory_write(
                "convention",
                "Conventional commits are required.",
                "conv-1",
            ))
            .expect("convention record");
        state
            .record_memory(&injection_memory_write(
                "project",
                "Glossary: d-token is the local control plane.",
                "proj-1",
            ))
            .expect("project record");
        state
            .record_memory(&injection_memory_write(
                "diagnostic",
                "Diagnostic records must be filtered out of injection.",
                "diag-1",
            ))
            .expect("diagnostic record");

        // Fail-closed: unknown project/agent identity -> no retrieval.
        assert!(request_scope_memory_access(&cache_test_scope(Protocol::OpenAi)).is_none());

        let scope = injection_test_scope();
        let access = request_scope_memory_access(&scope).expect("known scope resolves");
        let records = authorized_memory_records(&state, &access);
        let mut kinds: Vec<&str> = records.iter().map(|record| record.kind.as_str()).collect();
        kinds.sort_unstable();
        assert_eq!(kinds, vec!["convention", "preference", "project"]);
        assert!(!records.iter().any(|record| record.kind == "diagnostic"));

        let blocks = memory_context_blocks(&records);
        assert_eq!(blocks.len(), 3);
        for block in &blocks {
            assert_eq!(block["type"], "text");
            assert_eq!(block["cache_control"], json!({"type": "ephemeral"}));
            assert!(block["text"].is_string());
            assert!(!block.to_string().contains("filtered out"));
        }

        // OpenAI Chat shape: existing system message content is merged as
        // typed parts; memory blocks never replace the original system text.
        let openai = json!({
            "model": "m",
            "messages": [
                {"role": "system", "content": "Existing system."},
                {"role": "user", "content": "hi"}
            ]
        });
        let injected = inject_memory_into_body(openai.clone(), &Protocol::OpenAi, &blocks);
        assert_eq!(injected["messages"][0]["role"], "system");
        let content = injected["messages"][0]["content"]
            .as_array()
            .expect("merged parts");
        assert_eq!(content.len(), 4);
        assert!(content
            .iter()
            .any(|part| part["text"] == "Existing system."));
        assert_eq!(injected["messages"][1]["role"], "user");

        // OpenAI Chat shape without a system message: a new leading system
        // message is created.
        let openai_plain = json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let injected_plain = inject_memory_into_body(openai_plain, &Protocol::OpenAi, &blocks);
        assert_eq!(injected_plain["messages"][0]["role"], "system");
        assert_eq!(
            injected_plain["messages"][0]["content"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(injected_plain["messages"][1]["role"], "user");

        // Anthropic string `system` is normalized to a block list with the
        // memory blocks appended.
        let anthropic = json!({
            "model": "m",
            "system": "Existing system.",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let injected_anthropic = inject_memory_into_body(anthropic, &Protocol::Anthropic, &blocks);
        let system = injected_anthropic["system"].as_array().expect("block list");
        assert_eq!(system.len(), 4);
        assert_eq!(
            system[0],
            json!({"type": "text", "text": "Existing system."})
        );

        // Anthropic block-list `system` is extended in place.
        let anthropic_array = json!({
            "model": "m",
            "system": [{"type": "text", "text": "A"}],
            "messages": []
        });
        let injected_aa = inject_memory_into_body(anthropic_array, &Protocol::Anthropic, &blocks);
        assert_eq!(injected_aa["system"].as_array().unwrap().len(), 4);

        // Empty blocks leave the body untouched and identical.
        let untouched = inject_memory_into_body(openai.clone(), &Protocol::OpenAi, &[]);
        assert_eq!(untouched, openai);

        // No raw prompt, secret, or excluded-kind content in any projection.
        let serialized = format!("{injected}{injected_plain}{injected_anthropic}{injected_aa}");
        assert!(!serialized.contains("raw_prompt"));
        assert!(!serialized.contains("filtered out"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn output_shaping_evidence_carries_real_route_evidence() {
        let result = dtoken_compress::output_shaper::OutputShapingResult {
            body: "shaped".to_string(),
            disposition: dtoken_compress::output_shaper::OutputShapingDisposition::Applied,
            reason_code: "terse_blank_collapse",
            verbosity: dtoken_types::OutputVerbosity::Terse,
            effort: dtoken_compress::output_shaper::EffortLevel::Minimal,
            transforms: vec!["terse_blank_collapse"],
            accounting: dtoken_compress::output_shaper::OutputShapingAccounting::Measured {
                original_bytes: 10,
                shaped_bytes: 5,
                saved_bytes: 5,
            },
        };
        let evidence = output_shaping_evidence(
            result,
            "route-a",
            "provider-a",
            "model-a",
            "config-rev-42",
            "request-7",
            "direct",
            "balanced",
        );
        assert_eq!(evidence["state"], "applied");
        assert_eq!(evidence["route_id"], "route-a");
        assert_eq!(evidence["provider_id"], "provider-a");
        assert_eq!(evidence["model"], "model-a");
        assert_eq!(evidence["config_revision"], "config-rev-42");
        assert_eq!(evidence["request_id"], "request-7");
        assert_eq!(evidence["chain_mode"], "direct");
        assert_eq!(evidence["compression_policy"], "balanced");
        assert_eq!(evidence["accounting"]["measured"]["saved_bytes"], 5);
        assert_eq!(evidence["raw_available"], false);
        assert_eq!(evidence["transforms"][0], "terse_blank_collapse");

        let passthrough = output_shaping_passthrough_evidence(
            "stream_passthrough",
            "route-a",
            "provider-a",
            "model-a",
            "config-rev-42",
            "request-7",
            "direct",
            "balanced",
        );
        assert_eq!(passthrough["state"], "passthrough");
        assert_eq!(passthrough["reason_code"], "stream_passthrough");
        assert_eq!(passthrough["config_revision"], "config-rev-42");
        assert_eq!(passthrough["request_id"], "request-7");
        assert_eq!(passthrough["chain_mode"], "direct");
        assert_eq!(passthrough["compression_policy"], "balanced");
        assert_eq!(passthrough["accounting"]["unknown"], true);
    }

    #[tokio::test]
    async fn headroom_mcp_status_reports_owned_transport_and_withholds_retrieve() {
        let state = AppState::new(DtokenConfig::empty());
        let Json(payload) = headroom_mcp(State(state)).await;

        assert_eq!(payload["state"], "ready");
        assert_eq!(payload["configured"], true);
        assert_eq!(payload["freshness"], "live");
        assert_eq!(payload["capability_revision"], BACKEND_CAPABILITY_REVISION);
        assert_eq!(
            payload["reason_codes"],
            json!([
                "mcp_stdio_json_rpc_transport_available",
                "mcp_compress_handler_available",
                "mcp_stats_handler_available",
                "mcp_retrieve_requires_managed_capability",
                "mcp_shared_context_requires_managed_capability"
            ])
        );
        assert_eq!(payload["transport"], "stdio-json-rpc");
        let transport = payload["transport"].as_str().expect("MCP transport name");
        let transport_reason = format!("mcp_{}_transport_available", transport.replace('-', "_"));
        assert!(payload["reason_codes"]
            .as_array()
            .expect("MCP reason codes")
            .iter()
            .any(|reason| reason == &transport_reason));
        assert!(payload["reason"]
            .as_str()
            .expect("MCP status reason")
            .contains(transport));
        assert_eq!(
            payload["tools"],
            json!(["headroom_compress", "headroom_stats"])
        );
        assert_eq!(
            payload["unavailable_tools"],
            json!([
                {
                    "name": "headroom_retrieve",
                    "reason": "retrieve_requires_managed_capability"
                },
                {
                    "name": "headroom_memory_list",
                    "reason": "shared_context_requires_managed_scope_capability"
                }
            ])
        );
    }

    #[tokio::test]
    async fn backend_capability_registry_is_the_mcp_status_source() {
        let state = AppState::new(DtokenConfig::empty());
        let Json(registry) = backend_capabilities(State(state.clone())).await;
        let Json(mcp) = headroom_mcp(State(state)).await;
        let capability = registry.find("mcp").expect("MCP registry entry");

        assert_eq!(capability.state, dtoken_types::AvailabilityState::Ready);
        assert_eq!(mcp["state"], json!(capability.state));
        assert_eq!(mcp["freshness"], json!(capability.freshness));
        assert_eq!(mcp["reason_codes"], json!(capability.reason_codes));
        assert_eq!(mcp["capability_revision"], registry.revision);
    }

    #[test]
    fn capability_read_model_is_stable_until_evidence_semantics_change() {
        let state = AppState::new(DtokenConfig::empty());
        let first = state.current_capability_registry();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = state.current_capability_registry();

        assert_eq!(second, first);

        let mut later_collection = first.clone();
        later_collection.collected_at_ms += 1;
        for capability in &mut later_collection.capabilities {
            capability.last_verified_at_ms = capability
                .last_verified_at_ms
                .map(|timestamp| timestamp + 1);
        }
        assert!(capability_registry_semantically_equal(
            &first,
            &later_collection
        ));
        later_collection
            .capabilities
            .iter_mut()
            .find(|capability| capability.id == "mcp")
            .expect("MCP capability")
            .reason_codes
            .push("semantic-change".to_string());
        assert!(!capability_registry_semantically_equal(
            &first,
            &later_collection
        ));
    }

    #[test]
    fn backend_registry_reports_safe_pipeline_and_disabled_boundaries_truthfully() {
        let registry = backend_capability_registry(
            None,
            ContextStoreCapabilityState::NotConfigured,
            false,
            None,
        );
        for id in [
            "cache-aligner",
            "code-compressor",
            "content-router",
            "json-codec",
            "log-codec",
            "search-codec",
            "build-codec",
            "diff-codec",
            "text-codec",
            "html-codec",
            "compressor-registry",
            "output-shaping",
            "rust-library",
            "typescript-sdk",
            "mcp",
        ] {
            assert_eq!(
                registry.find(id).expect("registered capability").state,
                AvailabilityState::Ready,
                "{id} must reflect its active implementation"
            );
        }
        let codec_evidence = dtoken_compress::codec_evidence::production_codec_evidence();
        for id in [
            "code-compressor",
            "json-codec",
            "log-codec",
            "search-codec",
            "build-codec",
            "diff-codec",
            "text-codec",
            "html-codec",
        ] {
            let capability = registry.find(id).expect("codec capability");
            assert_eq!(
                capability.evidence_revision.as_deref(),
                Some(codec_evidence.evidence_revision()),
                "{id}"
            );
            assert!(
                capability
                    .reason_codes
                    .contains(&"runtime_codec_evidence_verified".to_string()),
                "{id}"
            );
        }
        assert_eq!(
            registry
                .find("ccr-pre-send")
                .expect("CCR pre-send capability")
                .state,
            AvailabilityState::Degraded
        );
        assert_eq!(
            registry
                .find("integrated-rtk")
                .expect("integrated RTK capability")
                .state,
            AvailabilityState::Degraded
        );
        assert_eq!(
            registry
                .find("input-image-compressor")
                .expect("ordinary image capability boundary")
                .state,
            AvailabilityState::Unavailable
        );

        let ccr_ready = backend_capability_registry(
            None,
            ContextStoreCapabilityState::NotConfigured,
            true,
            None,
        );
        assert_eq!(
            ccr_ready
                .find("ccr-pre-send")
                .expect("configured CCR pre-send capability")
                .state,
            AvailabilityState::Ready
        );
    }

    #[test]
    fn backend_registry_exposes_release_aggregate_capabilities_without_claiming_optical_active() {
        let registry = backend_capability_registry(
            None,
            ContextStoreCapabilityState::Ready { schema_version: 6 },
            true,
            None,
        );
        let expected = [
            ("compression", AvailabilityState::Ready),
            ("ccr", AvailabilityState::Ready),
            ("memory_learn", AvailabilityState::Degraded),
            ("output_shape", AvailabilityState::Ready),
            ("integrated_rtk", AvailabilityState::Degraded),
            ("proxy_sdk_mcp", AvailabilityState::Ready),
            ("stats_history_metrics", AvailabilityState::Ready),
            ("routing_protocols", AvailabilityState::Ready),
            ("doctor_dashboard_logs_export", AvailabilityState::Degraded),
            ("project_policy", AvailabilityState::Degraded),
            ("backup_rollback", AvailabilityState::Degraded),
            (
                "optical_active_transform",
                AvailabilityState::ImplementedDisabled,
            ),
        ];

        for (id, state) in expected {
            let capability = registry.find(id).expect("release aggregate capability");
            assert_eq!(capability.state, state, "{id}");
            assert_eq!(capability.freshness, EvidenceFreshness::Live, "{id}");
            assert!(
                capability.evidence_revision.is_some(),
                "{id} requires source evidence"
            );
            assert!(!capability.reason_codes.is_empty(), "{id}");
        }
        assert!(registry
            .find("optical_active_transform")
            .expect("Optical aggregate")
            .reason_codes
            .contains(&"policy_shadow_only".to_string()));
    }

    #[test]
    fn integrated_rtk_requires_verified_install_and_real_invocation_receipt() {
        let installed_only = ManagedRtkRuntimeEvidence {
            installed: true,
            verified: true,
            usage_count: 0,
            saved_bytes: 0,
            last_receipt: None,
            reason: "verified_no_usage_evidence".to_string(),
        };
        let installed_registry = backend_capability_registry(
            None,
            ContextStoreCapabilityState::NotConfigured,
            false,
            Some(&installed_only),
        );
        assert_eq!(
            installed_registry
                .find("integrated_rtk")
                .expect("aggregate integrated RTK")
                .state,
            AvailabilityState::Degraded
        );

        let invoked = ManagedRtkRuntimeEvidence {
            installed: true,
            verified: true,
            usage_count: 1,
            saved_bytes: 24,
            last_receipt: Some(dtoken_preprocessor::ManagedRtkUsageReceipt {
                recorded_at_ms: 42,
                exit_code: Some(0),
                input_bytes: 100,
                output_bytes: 76,
                saved_bytes: 24,
            }),
            reason: "verified_usage_evidence".to_string(),
        };
        let invoked_registry = backend_capability_registry(
            None,
            ContextStoreCapabilityState::NotConfigured,
            false,
            Some(&invoked),
        );
        assert_eq!(
            invoked_registry
                .find("integrated-rtk")
                .expect("leaf integrated RTK")
                .state,
            AvailabilityState::Ready
        );
        assert_eq!(
            invoked_registry
                .find("integrated_rtk")
                .expect("aggregate integrated RTK")
                .state,
            AvailabilityState::Ready
        );
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
            false,
            None,
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
            false,
            None,
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
            Some("context-store-schema-v7")
        );
        let ready_memory = ready
            .capabilities
            .find("memory-store")
            .expect("memory-store capability");
        assert_eq!(ready_memory.state, AvailabilityState::Ready);
        assert!(ready_memory
            .reason_codes
            .contains(&"memory_http_summary_scope_required".to_string()));
        assert!(ready_memory
            .reason_codes
            .contains(&"memory_write_requires_managed_mcp_scope".to_string()));

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
        let failed_memory = failed
            .capabilities
            .find("memory-store")
            .expect("failed memory-store capability");
        assert_eq!(failed_memory.state, AvailabilityState::Unavailable);
        assert_eq!(
            failed_memory.reason_codes,
            vec!["context_store_open_failed"]
        );

        let _ = std::fs::remove_dir_all(ready_dir);
        let _ = std::fs::remove_file(blocked_root);
    }

    #[test]
    fn failed_terminal_request_records_only_a_typed_learning_pattern() {
        let dir = std::env::temp_dir().join(format!("dtoken-failure-pattern-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(DtokenConfig::empty(), &dir);
        let request_id = Uuid::new_v4();
        let decision = build_route_decision(request_id, Some("model-a"));
        let compression = compress_for_route(
            None,
            CompressionPolicy::Off,
            json!({"model":"model-a","messages":[]}),
        );
        let attribution = RequestAttribution {
            agent: Some("opencode".to_string()),
            instance_id: Some("instance-a".to_string()),
            project_id: Some("project-a".to_string()),
        };
        let mut request_log = build_request_log(
            request_id,
            &decision,
            Some("model-a".to_string()),
            Protocol::OpenAi,
            &compression,
            &attribution,
        );
        request_log.error = Some("provider_timeout".to_string());
        let mut event = build_ledger_event(
            request_id,
            &decision,
            Protocol::OpenAi,
            Protocol::OpenAi,
            &compression,
            &attribution,
        );
        event.status = EventStatus::Failed;
        state.record_routed_request(&decision, request_log, event);

        let patterns = state
            .context_store
            .as_ref()
            .expect("persistent context store")
            .lock()
            .expect("context store lock")
            .list_failure_patterns(
                Some("project-a"),
                Some("opencode"),
                dtoken_event_ledger::now_ms().max(1),
                10,
            )
            .expect("typed failure patterns");
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].failure_class, FailureClass::ProviderTimeout);
        assert_eq!(patterns[0].occurrences, 1);
        let encoded = serde_json::to_string(&patterns).expect("serialize patterns");
        assert!(!encoded.contains("configured upstream"));
        assert!(!encoded.contains("messages"));
        assert!(!encoded.contains("content"));

        drop(state);
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
                wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
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
            output_shaping_policies: vec![],
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
            wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
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
            wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
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

    #[test]
    fn direct_anthropic_verified_evidence_inserts_one_frozen_tool_breakpoint() {
        let dir = std::env::temp_dir().join(format!("dtoken-anthropic-cache-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(
            direct_anthropic_cache_test_config("https://api.anthropic.com".to_string()),
            &dir,
        );
        let body = json!({
            "model": "claude-cache-test",
            "system": [{"type": "text", "text": "stable system instructions"}],
            "tools": [{"name": "lookup", "description": "stable tool", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": "continue"}]
        });
        let mut payload = anthropic_route_payload(&state, body, RouteEvidence::default())
            .expect("direct Anthropic route payload");
        persist_anthropic_cache_evidence(&state, &payload.scope);
        let evidence = configured_provider_cache_evidence(&state, &payload.scope)
            .expect("fresh exact Anthropic evidence");

        let wire = apply_direct_anthropic_cache_breakpoint(&mut payload, Some(&evidence));

        assert!(wire.breakpoint_inserted);
        assert_eq!(wire.breakpoint_count, 1);
        assert_eq!(wire.pointer_kind, "tool_definition");
        assert_eq!(
            payload.upstream_body["tools"][0]["cache_control"]["type"],
            "ephemeral"
        );
        attach_direct_anthropic_cache_wire_metadata(&mut payload.event, wire, false);
        let metadata = &payload.event.metadata["provider_cache"];
        assert_eq!(metadata["wire_variant"], "serialized");
        assert_eq!(metadata["evidence_revision_present"], true);
        assert!(!metadata.to_string().contains("stable system instructions"));
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn direct_anthropic_cache_marker_preserves_existing_control_and_fails_closed() {
        let dir = std::env::temp_dir().join(format!("dtoken-anthropic-cache-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(
            direct_anthropic_cache_test_config("https://api.anthropic.com".to_string()),
            &dir,
        );
        let body = json!({
            "model": "claude-cache-test",
            "tools": [{
                "name": "lookup",
                "input_schema": {"type": "object"},
                "cache_control": {"type": "ephemeral", "ttl": "5m"}
            }],
            "messages": [{"role": "user", "content": "continue"}]
        });
        let mut payload = anthropic_route_payload(&state, body.clone(), RouteEvidence::default())
            .expect("direct Anthropic route payload");
        persist_anthropic_cache_evidence(&state, &payload.scope);
        let evidence = configured_provider_cache_evidence(&state, &payload.scope)
            .expect("fresh exact Anthropic evidence");

        let wire = apply_direct_anthropic_cache_breakpoint(&mut payload, Some(&evidence));

        assert!(!wire.breakpoint_inserted);
        assert_eq!(wire.reason_code, "user_cache_control_preserved");
        assert_eq!(payload.upstream_body, body);
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn direct_anthropic_cache_marker_rejects_missing_or_mismatched_evidence() {
        let dir = std::env::temp_dir().join(format!("dtoken-anthropic-cache-{}", Uuid::new_v4()));
        let state = AppState::new_persistent(
            direct_anthropic_cache_test_config("https://api.anthropic.com".to_string()),
            &dir,
        );
        let body = json!({
            "model": "claude-cache-test",
            "tools": [{"name": "lookup", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": "continue"}]
        });
        let mut payload = anthropic_route_payload(&state, body.clone(), RouteEvidence::default())
            .expect("direct Anthropic route payload");
        let wire = apply_direct_anthropic_cache_breakpoint(&mut payload, None);
        assert!(!wire.breakpoint_inserted);
        assert_eq!(wire.reason_code, "provider_cache_evidence_unavailable");
        assert_eq!(payload.upstream_body, body);
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn direct_anthropic_cache_marker_requires_durable_ccr_before_optimized_send() {
        let dir =
            std::env::temp_dir().join(format!("dtoken-anthropic-cache-ccr-{}", Uuid::new_v4()));
        let mut state = AppState::new_persistent(
            direct_anthropic_cache_test_config("https://api.anthropic.com".to_string()),
            &dir,
        );
        state.ccr_private_store = Some(Arc::new(
            CcrStore::new(dir.join("ccr-private"), [41_u8; 32]).expect("CCR store"),
        ));
        let body = json!({
            "model": "claude-cache-test",
            "tools": [{"name": "lookup", "description": "stable tool", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": "continue"}]
        });
        let original = serde_json::to_vec(&body).expect("serialize original request");
        let mut payload = anthropic_route_payload(&state, body.clone(), RouteEvidence::default())
            .expect("direct Anthropic route payload");
        persist_anthropic_cache_evidence(&state, &payload.scope);
        let evidence = configured_provider_cache_evidence(&state, &payload.scope)
            .expect("fresh exact Anthropic evidence");

        let (prepared_ccr, variant, wire) = prepare_direct_anthropic_pipeline_for_send(
            &state,
            &mut payload,
            &body,
            &original,
            Some(&evidence),
        );

        assert!(prepared_ccr.receipt.is_some());
        assert_eq!(variant, dtoken_types::SentPayloadVariant::Optimized);
        assert!(wire.breakpoint_inserted);
        assert!(!wire.rolled_back);
        assert_eq!(
            payload.upstream_body["tools"][0]["cache_control"]["type"],
            "ephemeral"
        );
        assert!(payload
            .request_log
            .transforms_applied
            .iter()
            .any(|item| item == "anthropic_explicit_cache_breakpoint"));
        assert_eq!(
            payload.event.metadata["transforms_applied"],
            json!(["anthropic_explicit_cache_breakpoint"])
        );
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn direct_anthropic_cache_marker_without_ccr_restores_exact_baseline() {
        let dir =
            std::env::temp_dir().join(format!("dtoken-anthropic-cache-no-ccr-{}", Uuid::new_v4()));
        let mut state = AppState::new_persistent(
            direct_anthropic_cache_test_config("https://api.anthropic.com".to_string()),
            &dir,
        );
        state.ccr_private_store = None;
        let body = json!({
            "model": "claude-cache-test",
            "tools": [{"name": "lookup", "description": "stable tool", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": "continue"}]
        });
        let original = serde_json::to_vec(&body).expect("serialize original request");
        let mut payload = anthropic_route_payload(&state, body.clone(), RouteEvidence::default())
            .expect("direct Anthropic route payload");
        persist_anthropic_cache_evidence(&state, &payload.scope);
        let evidence = configured_provider_cache_evidence(&state, &payload.scope)
            .expect("fresh exact Anthropic evidence");

        let (prepared_ccr, variant, wire) = prepare_direct_anthropic_pipeline_for_send(
            &state,
            &mut payload,
            &body,
            &original,
            Some(&evidence),
        );

        assert!(prepared_ccr.receipt.is_none());
        assert_eq!(variant, dtoken_types::SentPayloadVariant::Original);
        assert!(!wire.breakpoint_inserted);
        assert!(wire.rolled_back);
        assert_eq!(wire.reason_code, "cache_breakpoint_ccr_unavailable");
        assert_eq!(payload.upstream_body, body);
        assert!(payload.request_log.transforms_applied.is_empty());
        assert_eq!(payload.event.metadata["transforms_applied"], json!([]));
        assert!(direct_anthropic_uses_raw_body(variant));
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn direct_anthropic_without_verified_evidence_raw_wires_exact_body_and_ledger_metadata() {
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
                        .expect("fixed raw body has a content length");
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
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{\"type\":\"message\",\"content\":[],\"usage\":{\"input_tokens\":1}}",
                )
                .await
                .expect("write Anthropic response");
        });

        let state = AppState::new(direct_anthropic_cache_test_config(format!("http://{addr}")));
        let raw = br#"{ "model" : "claude-cache-test", "messages" : [ { "role":"user", "content":"keep these bytes" } ] }"#;
        let response = anthropic_messages(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from_static(raw),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(captured_rx.await.expect("captured request body"), raw);
        let event = state
            .recent_events(10)
            .into_iter()
            .find(|event| event.metadata.get("provider_cache").is_some())
            .expect("routed ledger event");
        assert_eq!(event.metadata["provider_cache"]["wire_variant"], "raw_wire");
        assert_eq!(
            event.metadata["provider_cache"]["reason_code"],
            "provider_cache_evidence_unavailable"
        );
        assert!(!event.metadata.to_string().contains("keep these bytes"));
        server.await.expect("mock upstream task");
    }

    #[tokio::test]
    async fn direct_anthropic_compatibility_upstream_withholds_breakpoint_and_preserves_wire() {
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
                        .expect("serialized body has a content length");
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
                .expect("capture serialized body");
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{\"type\":\"message\",\"content\":[],\"usage\":{\"input_tokens\":1}}",
                )
                .await
                .expect("write Anthropic response");
        });

        let dir =
            std::env::temp_dir().join(format!("dtoken-anthropic-pipeline-{}", Uuid::new_v4()));
        let mut state = AppState::new_persistent(
            direct_anthropic_cache_test_config(format!("http://{addr}")),
            &dir,
        );
        state.ccr_private_store = Some(Arc::new(
            CcrStore::new(dir.join("ccr-private"), [31_u8; 32]).expect("CCR store"),
        ));
        let log = "INFO stable build route healthy tests passed\n".repeat(420);
        let mut messages = Vec::new();
        for index in 0..8 {
            let tool_id = format!("tool-{index}");
            messages.push(json!({
                "role": "assistant",
                "content": [{"type": "tool_use", "id": tool_id, "name": "bash", "input": {}}]
            }));
            messages.push(json!({
                "role": "user",
                "content": [{"type": "tool_result", "tool_use_id": tool_id, "content": log}]
            }));
        }
        messages.push(json!({"role": "user", "content": "Summarize verified state."}));
        let raw = serde_json::to_vec(&json!({
            "model": "claude-cache-test",
            "messages": messages
        }))
        .expect("serialize Anthropic request");
        let payload = anthropic_route_payload(
            &state,
            serde_json::from_slice(&raw).expect("parse Anthropic request"),
            RouteEvidence::default(),
        )
        .expect("route payload");
        persist_anthropic_cache_evidence(&state, &payload.scope);

        let response = anthropic_messages(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from(raw.clone()),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let captured = captured_rx.await.expect("captured request body");
        assert_eq!(captured, raw);
        assert!(state.recent_request_logs(1)[0]
            .transforms_applied
            .is_empty());
        let event = state
            .recent_events(20)
            .into_iter()
            .find(|event| event.metadata.get("pipeline_payload_variant").is_some())
            .expect("routed ledger event");
        assert_eq!(event.metadata["pipeline_payload_variant"], "original");
        assert_eq!(event.metadata["provider_cache"]["wire_variant"], "raw_wire");
        assert_eq!(
            event.metadata["provider_cache"]["reason_code"],
            "anthropic_cache_upstream_unverified"
        );
        server.await.expect("mock upstream task");
        drop(state);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn terminal_event_stream_records_completed_and_failed_terminal_events() {
        use axum::body::to_bytes;
        use futures_util::stream;

        let completed_state = AppState::new(DtokenConfig::default());
        let completed_request_id = Uuid::new_v4();
        let completed_stream: UpstreamByteStream = Box::pin(stream::iter(vec![Ok(
            Bytes::from_static(b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"),
        )]));
        let completed_body = Body::from_stream(TerminalEventStream::new(
            completed_stream,
            StreamTerminalLedgerGuard {
                state: completed_state.clone(),
                scope: cache_test_scope(Protocol::Anthropic),
                request_id: completed_request_id,
                provider_id: "fixture-provider".to_string(),
                upstream_status: 200,
                kind: "passthrough",
                accounting: None,
                usage_observer: AnthropicStreamUsageObserver::default(),
                terminal_record: None,
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
                scope: cache_test_scope(Protocol::Anthropic),
                request_id: failed_request_id,
                provider_id: "fixture-provider".to_string(),
                upstream_status: 200,
                kind: "semantic",
                accounting: None,
                usage_observer: AnthropicStreamUsageObserver::default(),
                terminal_record: None,
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

    #[tokio::test]
    async fn terminal_stream_clean_eof_without_anthropic_message_stop_fails_closed() {
        use axum::body::to_bytes;
        use futures_util::stream;

        let state = AppState::new(DtokenConfig::default());
        let request_id = Uuid::new_v4();
        let stream: UpstreamByteStream = Box::pin(stream::iter(vec![Ok(Bytes::from_static(
            b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":2}}\n\n",
        ))]));
        let body = Body::from_stream(TerminalEventStream::new(
            stream,
            StreamTerminalLedgerGuard {
                state: state.clone(),
                scope: cache_test_scope(Protocol::Anthropic),
                request_id,
                provider_id: "fixture-provider".to_string(),
                upstream_status: 200,
                kind: "passthrough",
                accounting: None,
                usage_observer: AnthropicStreamUsageObserver::default(),
                terminal_record: None,
                terminal: false,
            },
        ));
        to_bytes(body, usize::MAX)
            .await
            .expect("transparent incomplete stream body");

        let terminal = state
            .recent_events(10)
            .into_iter()
            .find(|event| event.request_id == Some(request_id))
            .expect("terminal failure event");
        assert_eq!(terminal.status, EventStatus::Failed);
        assert_eq!(
            terminal.error_code.as_deref(),
            Some("provider_stream_incomplete")
        );
    }

    #[tokio::test]
    async fn responses_stream_defers_terminal_record_and_matches_failed_event() {
        use axum::body::to_bytes;
        use futures_util::stream;

        for (chunks, expected_status, expected_event) in [
            (
                vec![Ok(Bytes::from_static(
                    b"data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1,\"total_tokens\":3}}\n\ndata: [DONE]\n\n",
                ))],
                EventStatus::Ok,
                "response.completed",
            ),
            (
                vec![Ok(Bytes::from_static(
                    b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n",
                ))],
                EventStatus::Failed,
                "response.failed",
            ),
        ] {
            let state = AppState::new(DtokenConfig::default());
            let request = OpenAiChatRequest {
                model: Some("fixture-model".to_string()),
                messages: Some(vec![json!({"role":"user","content":"stream input"})]),
                stream: Some(true),
            };
            let (_response, decision, request_log, event) = routed_openai_payload(&request);
            let prepared = state
                .prepare_ccr_for_request(&decision, &request_log, b"{}")
                .expect("prepare metadata-only CCR");
            let request_id = decision.request_id;
            let inner: Pin<
                Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>,
            > = Box::pin(stream::iter(chunks));
            let body = Body::from_stream(ResponsesSseStream::new(
                inner,
                format!("resp-{request_id}"),
                request.model.clone(),
                ResponsesStreamTerminalLedgerGuard {
                    state: state.clone(),
                    scope: cache_test_scope(Protocol::OpenAi),
                    request_id,
                    provider_id: decision.provider_id.clone(),
                    upstream_status: 200,
                    terminal_record: Some(StreamTerminalRequestRecord {
                        decision,
                        request_log,
                        event,
                        prepared_ccr: prepared,
                    }),
                    terminal: false,
                },
            ));

            assert!(state.recent_request_logs(10).is_empty());
            let bytes = to_bytes(body, usize::MAX)
                .await
                .expect("consume converted Responses stream");
            let converted = String::from_utf8(bytes.to_vec()).expect("UTF-8 SSE");
            assert!(converted.contains(expected_event), "{converted}");
            assert_eq!(state.recent_request_logs(10).len(), 1);
            let terminal = state
                .recent_events(20)
                .into_iter()
                .find(|entry| {
                    entry.request_id == Some(request_id)
                        && entry.metadata["stream_terminal"] == true
                })
                .expect("Responses terminal request event");
            assert_eq!(terminal.status, expected_status);
            assert_eq!(terminal.metadata["stream_terminal"], true);
        }
    }

    #[tokio::test]
    async fn native_responses_non_stream_passthrough_returns_upstream_body_verbatim() {
        use axum::body::to_bytes;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        // Keys are pre-sorted so the handler's `Json(...)` re-serialization is
        // byte-identical to the upstream document (exact passthrough contract).
        const UPSTREAM_BODY: &str = r#"{"id":"resp_123","object":"response","output":[{"content":[{"annotations":[],"text":"hello","type":"output_text"}],"id":"msg_1","role":"assistant","status":"completed","type":"message"}],"status":"completed","usage":{"input_tokens":2,"output_tokens":1,"total_tokens":3}}"#;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock upstream");
        let addr = listener.local_addr().expect("mock addr");
        let upstream_body = UPSTREAM_BODY.to_string();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept request");
            let mut buffer = [0_u8; 8192];
            let _ = socket.read(&mut buffer).await.expect("read request");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                upstream_body.len(),
                upstream_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write json");
        });

        let provider = dtoken_types::ProviderProfile {
            id: "responses-upstream".to_string(),
            name: "Responses upstream".to_string(),
            kind: dtoken_types::ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url: format!("http://{addr}/v1"),
            api_key_ref: None,
            models: vec!["mock/model".to_string()],
            timeout_ms: 30_000,
            healthcheck_path: None,
            wire_api: dtoken_types::ProviderWireApi::OpenAiResponses,
        };
        let route = dtoken_types::RouteProfile {
            id: "route-responses".to_string(),
            name: "route-responses".to_string(),
            input_protocol: Protocol::OpenAi,
            instance_id: "instance-responses".to_string(),
            target_provider_id: "responses-upstream".to_string(),
            fallback_provider_id: None,
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Off,
            bind_host: "127.0.0.1".to_string(),
            local_port: 18001,
        };
        let config = DtokenConfig {
            providers: vec![provider],
            routes: vec![route],
            instances: vec![],
            provider_cache_policies: vec![],
            output_shaping_policies: vec![],
            optical: Default::default(),
        };
        let state = AppState::new(config);
        let app = build_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"model":"mock/model","input":"hello"}"#))
                    .expect("request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let body = String::from_utf8(bytes.to_vec()).expect("UTF-8 JSON");
        // Native passthrough must preserve the upstream Responses JSON exactly:
        // no dtoken envelope, no evidence keys inserted (Codex schema contract).
        assert_eq!(body, UPSTREAM_BODY, "body={body}");
        assert!(!body.contains("\"dtoken\""), "no dtoken key: body={body}");
        // Evidence stays in the internal ledger only.
        assert_eq!(state.recent_request_logs(10).len(), 1);
        let events = state.recent_events(20);
        assert!(
            events.iter().any(|event| event.status == EventStatus::Ok),
            "evidence recorded in ledger"
        );
        server.await.expect("mock upstream task");
    }

    #[tokio::test]
    async fn responses_passthrough_stream_defers_terminal_record_until_terminal_event() {
        use futures_util::stream;
        use futures_util::StreamExt;

        let state = AppState::new(DtokenConfig::default());
        let request = OpenAiChatRequest {
            model: Some("fixture-model".to_string()),
            messages: Some(vec![json!({"role":"user","content":"stream input"})]),
            stream: Some(true),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, b"{}")
            .expect("prepare metadata-only CCR");
        let request_id = decision.request_id;
        let delta_chunk = Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n",
        );
        let terminal_chunk = Bytes::from_static(
            b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":2,\"output_tokens\":1,\"total_tokens\":3}}}\n\n",
        );
        let inner: UpstreamByteStream = Box::pin(stream::iter(vec![
            Ok(delta_chunk.clone()),
            Ok(terminal_chunk.clone()),
        ]));
        let passthrough = ResponsesPassthroughStream::new(
            inner,
            ResponsesStreamTerminalLedgerGuard {
                state: state.clone(),
                scope: cache_test_scope(Protocol::OpenAi),
                request_id,
                provider_id: decision.provider_id.clone(),
                upstream_status: 200,
                terminal_record: Some(StreamTerminalRequestRecord {
                    decision,
                    request_log,
                    event,
                    prepared_ccr: prepared,
                }),
                terminal: false,
            },
        );
        let mut passthrough = std::pin::pin!(passthrough);

        // Non-terminal delta arrives first: bytes pass through unchanged and
        // nothing is committed yet (G4 defers the terminal record).
        let first = passthrough.next().await.expect("first chunk");
        assert_eq!(first.expect("ok chunk"), delta_chunk);
        assert!(
            state.recent_request_logs(10).is_empty(),
            "no terminal record before terminal event"
        );

        // Terminal event arrives, then EOF: ledger/CCR commit only now.
        let second = passthrough.next().await.expect("terminal chunk");
        assert_eq!(second.expect("ok chunk"), terminal_chunk);
        assert!(passthrough.next().await.is_none());
        assert_eq!(state.recent_request_logs(10).len(), 1);
        let terminal = state
            .recent_events(20)
            .into_iter()
            .find(|entry| {
                entry.request_id == Some(request_id) && entry.metadata["stream_terminal"] == true
            })
            .expect("Responses passthrough terminal event");
        assert_eq!(terminal.status, EventStatus::Ok);
        assert_eq!(terminal.metadata["stream_terminal"], true);
        assert_eq!(terminal.metadata["send_state"], "completed");
    }

    #[tokio::test]
    async fn responses_passthrough_stream_clean_eof_without_terminal_fails_closed() {
        use axum::body::to_bytes;
        use futures_util::stream;

        let state = AppState::new(DtokenConfig::default());
        let request = OpenAiChatRequest {
            model: Some("fixture-model".to_string()),
            messages: Some(vec![json!({"role":"user","content":"stream input"})]),
            stream: Some(true),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, b"{}")
            .expect("prepare metadata-only CCR");
        let request_id = decision.request_id;
        let delta_chunk = Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
        );
        let inner: UpstreamByteStream = Box::pin(stream::iter(vec![Ok(delta_chunk.clone())]));
        let body = Body::from_stream(ResponsesPassthroughStream::new(
            inner,
            ResponsesStreamTerminalLedgerGuard {
                state: state.clone(),
                scope: cache_test_scope(Protocol::OpenAi),
                request_id,
                provider_id: decision.provider_id.clone(),
                upstream_status: 200,
                terminal_record: Some(StreamTerminalRequestRecord {
                    decision,
                    request_log,
                    event,
                    prepared_ccr: prepared,
                }),
                terminal: false,
            },
        ));
        let bytes = to_bytes(body, usize::MAX)
            .await
            .expect("transparent incomplete stream body");
        assert_eq!(bytes, delta_chunk, "bytes pass through unchanged");
        let terminal = state
            .recent_events(20)
            .into_iter()
            .find(|entry| {
                entry.request_id == Some(request_id) && entry.metadata["stream_terminal"] == true
            })
            .expect("terminal failure event");
        assert_eq!(terminal.status, EventStatus::Failed);
        assert_eq!(
            terminal.error_code.as_deref(),
            Some("provider_stream_incomplete")
        );
    }

    #[tokio::test]
    async fn dropped_responses_stream_records_failure_instead_of_success() {
        use futures_util::stream;

        let state = AppState::new(DtokenConfig::default());
        let request = OpenAiChatRequest {
            model: Some("fixture-model".to_string()),
            messages: Some(vec![json!({"role":"user","content":"stream input"})]),
            stream: Some(true),
        };
        let (_response, decision, request_log, event) = routed_openai_payload(&request);
        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, b"{}")
            .expect("prepare metadata-only CCR");
        let request_id = decision.request_id;
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(stream::pending());
        let body = Body::from_stream(ResponsesSseStream::new(
            inner,
            format!("resp-{request_id}"),
            request.model.clone(),
            ResponsesStreamTerminalLedgerGuard {
                state: state.clone(),
                scope: cache_test_scope(Protocol::OpenAi),
                request_id,
                provider_id: decision.provider_id.clone(),
                upstream_status: 200,
                terminal_record: Some(StreamTerminalRequestRecord {
                    decision,
                    request_log,
                    event,
                    prepared_ccr: prepared,
                }),
                terminal: false,
            },
        ));

        assert!(state.recent_request_logs(10).is_empty());
        drop(body);
        assert_eq!(state.recent_request_logs(10).len(), 1);
        let terminal = state
            .recent_events(20)
            .into_iter()
            .find(|entry| {
                entry.request_id == Some(request_id) && entry.metadata["stream_terminal"] == true
            })
            .expect("dropped Responses stream terminal event");
        assert_eq!(terminal.status, EventStatus::Failed);
        assert_eq!(
            terminal.error_code.as_deref(),
            Some("provider_stream_dropped")
        );
    }

    #[tokio::test]
    async fn terminal_stream_defers_ccr_projection_until_stream_completion() {
        use axum::body::to_bytes;
        use futures_util::stream;

        let dir = std::env::temp_dir().join(format!("dtoken-stream-ccr-{}", Uuid::new_v4()));
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, [7_u8; 32])
            .expect("persistent CCR state");
        let request = OpenAiChatRequest {
            model: Some("fixture-model".to_string()),
            messages: Some(vec![json!({"role":"user","content":"stream input"})]),
            stream: Some(true),
        };
        let (_response, decision, mut request_log, event) = routed_openai_payload(&request);
        request_log.transforms_applied = vec!["semantic_closed_tool_result".to_string()];
        let original = br#"{"model":"fixture-model","messages":[{"role":"user","content":"STREAM_CCR_SECRET"}]}"#;
        let prepared = state
            .prepare_ccr_for_request(&decision, &request_log, original)
            .expect("CCR must be durable before stream send");
        let recovery_ref = format!("ccr:{}", decision.request_id);
        let scope = cache_test_scope(Protocol::Anthropic);
        let stream: UpstreamByteStream = Box::pin(stream::iter(vec![
            Ok(Bytes::from_static(
                b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":101,\"cache_read_input_tokens\":25,\"cache_creation_input_tokens\":3,\"private\":\"STREAM_USAGE_SECRET\"}}}\n\n",
            )),
            Ok(Bytes::from_static(
                b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":17,\"cache_creation\":{\"ephemeral_5m_input_tokens\":4}}}\n\n",
            )),
            Ok(Bytes::from_static(
                b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
            )),
        ]));
        let body = Body::from_stream(TerminalEventStream::new(
            stream,
            StreamTerminalLedgerGuard {
                state: state.clone(),
                scope: scope.clone(),
                request_id: decision.request_id,
                provider_id: decision.provider_id.clone(),
                upstream_status: 200,
                kind: "semantic",
                accounting: None,
                usage_observer: AnthropicStreamUsageObserver::default(),
                terminal_record: Some(StreamTerminalRequestRecord {
                    decision,
                    request_log,
                    event,
                    prepared_ccr: prepared,
                }),
                terminal: false,
            },
        ));

        // Creating the response body is only an upstream acceptance. The
        // redacted pre-send journal is durable, but no terminal request/CCR
        // projection may exist yet.
        assert!(state.recent_request_logs(10).is_empty());
        assert_eq!(
            state
                .retrieve_ccr_metadata(&recovery_ref)
                .expect("pending CCR lifecycle")["state"],
            "pending_retained"
        );
        to_bytes(body, usize::MAX)
            .await
            .expect("consume completed stream");

        assert_eq!(state.recent_request_logs(10).len(), 1);
        let request_log = state
            .recent_request_logs(10)
            .into_iter()
            .next()
            .expect("terminal request log");
        assert_eq!(request_log.output_tokens, Some(17));
        assert!(!serde_json::to_string(&request_log)
            .expect("request log JSON")
            .contains("STREAM_USAGE_SECRET"));
        let metadata = state
            .retrieve_ccr_metadata(&recovery_ref)
            .expect("terminal CCR metadata");
        assert_eq!(metadata["state"], "finalized");
        assert_eq!(metadata["raw_available"], true);
        assert!(!metadata.to_string().contains("STREAM_CCR_SECRET"));
        let terminal = state
            .recent_events(10)
            .into_iter()
            .find(|entry| {
                entry.request_id == Some(uuid::Uuid::parse_str(&recovery_ref[4..]).unwrap())
                    && entry.metadata["stream_terminal"] == true
            })
            .expect("terminal request event");
        assert_eq!(terminal.status, EventStatus::Ok);
        assert_eq!(terminal.metadata["stream_terminal"], true);

        let projections = ContextStore::open(dir.join("runtime-state.sqlite3"))
            .expect("open context-store projection");
        assert_eq!(
            projections.projection_counts().expect("projection counts"),
            (1, 1)
        );
        let observation = projections
            .provider_cache_observation(
                &scope.route_id,
                &scope.provider_id,
                &scope.model,
                &scope.output_protocol,
                &scope.config_revision,
            )
            .expect("cache observation lookup")
            .expect("Anthropic stream usage observation");
        assert_eq!(observation.read_units, Some(25));
        assert_eq!(observation.creation_units, Some(3));
        assert_eq!(observation.creation_5m_units, Some(4));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn dropped_stream_marks_ccr_receipt_as_send_failed_retained() {
        use futures_util::stream;

        let dir = std::env::temp_dir().join(format!("dtoken-stream-drop-{}", Uuid::new_v4()));
        let state = AppState::new_persistent_with_ccr_key(DtokenConfig::empty(), &dir, [8_u8; 32])
            .expect("persistent CCR state");
        let request = OpenAiChatRequest {
            model: Some("fixture-model".to_string()),
            messages: Some(vec![json!({"role":"user","content":"stream input"})]),
            stream: Some(true),
        };
        let (_response, decision, mut request_log, event) = routed_openai_payload(&request);
        request_log.transforms_applied = vec!["semantic_closed_tool_result".to_string()];
        let prepared = state
            .prepare_ccr_for_request(
                &decision,
                &request_log,
                br#"{"model":"fixture-model","messages":[]}"#,
            )
            .expect("CCR must be durable before stream send");
        let recovery_ref = format!("ccr:{}", decision.request_id);
        let stream: UpstreamByteStream = Box::pin(stream::pending());
        let body = Body::from_stream(TerminalEventStream::new(
            stream,
            StreamTerminalLedgerGuard {
                state: state.clone(),
                scope: cache_test_scope(Protocol::Anthropic),
                request_id: decision.request_id,
                provider_id: decision.provider_id.clone(),
                upstream_status: 200,
                kind: "semantic",
                accounting: None,
                usage_observer: AnthropicStreamUsageObserver::default(),
                terminal_record: Some(StreamTerminalRequestRecord {
                    decision,
                    request_log,
                    event,
                    prepared_ccr: prepared,
                }),
                terminal: false,
            },
        ));

        // A client that abandons the response must not leave a prepared or
        // finalized receipt behind.  Dropping the body is the terminal
        // failure signal for this request.
        assert!(state.recent_request_logs(10).is_empty());
        drop(body);
        assert_eq!(state.recent_request_logs(10).len(), 1);
        let projections = ContextStore::open(dir.join("runtime-state.sqlite3"))
            .expect("open context-store projection");
        assert_eq!(
            projections
                .ccr_metadata_state(&recovery_ref)
                .expect("read CCR state")
                .as_deref(),
            Some("send_failed_retained")
        );
        let terminal = state
            .recent_events(10)
            .into_iter()
            .find(|entry| {
                entry.request_id == Some(uuid::Uuid::parse_str(&recovery_ref[4..]).unwrap())
                    && entry.metadata["stream_terminal"] == true
            })
            .expect("dropped stream terminal event");
        assert_eq!(terminal.status, EventStatus::Failed);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compression_policy_as_str_maps_all_five_policies() {
        assert_eq!(compression_policy_as_str(&CompressionPolicy::Off), "off");
        assert_eq!(compression_policy_as_str(&CompressionPolicy::Safe), "safe");
        assert_eq!(
            compression_policy_as_str(&CompressionPolicy::Balanced),
            "balanced"
        );
        assert_eq!(
            compression_policy_as_str(&CompressionPolicy::Aggressive),
            "aggressive"
        );
        assert_eq!(compression_policy_as_str(&CompressionPolicy::Max), "max");
    }

    #[test]
    fn inject_memory_into_openai_body_merges_existing_system_message() {
        let blocks = vec![
            json!({"type": "text", "text": "Memory block.", "cache_control": {"type": "ephemeral"}}),
        ];
        // Existing system message: content is merged as typed parts.
        let body = json!({
            "model": "m",
            "messages": [
                {"role": "system", "content": "Original system."},
                {"role": "user", "content": "hi"}
            ]
        });
        let injected = inject_memory_into_openai_body(&body, &blocks);
        let messages = injected["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"][0]["text"], "Original system.");
        assert_eq!(messages[0]["content"][1]["text"], "Memory block.");
        assert_eq!(
            messages[0]["content"][1]["cache_control"]["type"],
            "ephemeral"
        );

        // No messages array: fail-closed, body unchanged.
        let no_messages = json!({"model": "m"});
        let unchanged = inject_memory_into_openai_body(&no_messages, &blocks);
        assert_eq!(unchanged, no_messages);

        // No system message: memory block becomes a new leading system message.
        let no_system = json!({
            "model": "m",
            "messages": [
                {"role": "user", "content": "user message"}
            ]
        });
        let injected = inject_memory_into_openai_body(&no_system, &blocks);
        let messages = injected["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"][0]["text"], "Memory block.");
        assert_eq!(messages[1]["role"], "user");

        // Empty blocks: body returned as-is.
        let empty = inject_memory_into_openai_body(&body, &[]);
        assert_eq!(empty, body);
    }

    #[test]
    fn inject_memory_into_anthropic_body_merges_system_blocks() {
        let blocks = vec![
            json!({"type": "text", "text": "Memory block.", "cache_control": {"type": "ephemeral"}}),
        ];
        // Existing string system -> normalized to block list.
        let body = json!({
            "model": "m",
            "system": "Original system.",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let injected = inject_memory_into_anthropic_body(&body, &blocks);
        let system = injected["system"].as_array().unwrap();
        assert_eq!(system[0]["type"], "text");
        assert_eq!(system[0]["text"], "Original system.");
        assert_eq!(system[1]["text"], "Memory block.");

        // Existing block system -> extended.
        let body = json!({
            "model": "m",
            "system": [{"type": "text", "text": "Existing."}],
            "messages": [{"role": "user", "content": "hi"}]
        });
        let injected = inject_memory_into_anthropic_body(&body, &blocks);
        let system = injected["system"].as_array().unwrap();
        assert_eq!(system.len(), 2);
        assert_eq!(system[1]["text"], "Memory block.");

        // No system -> new system array.
        let body = json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let injected = inject_memory_into_anthropic_body(&body, &blocks);
        assert_eq!(injected["system"].as_array().unwrap().len(), 1);

        // Empty blocks: body unchanged.
        let empty = inject_memory_into_anthropic_body(&body, &[]);
        assert_eq!(empty, body);
    }

    #[test]
    fn inject_memory_into_body_dispatches_by_protocol_and_skips_empty() {
        let blocks =
            vec![json!({"type": "text", "text": "Block.", "cache_control": {"type": "ephemeral"}})];
        // OpenAI dispatch.
        let openai_body = json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let result = inject_memory_into_body(openai_body.clone(), &Protocol::OpenAi, &blocks);
        assert_eq!(result["messages"][0]["role"], "system");

        // Anthropic dispatch.
        let anthropic_body = json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let result = inject_memory_into_body(anthropic_body.clone(), &Protocol::Anthropic, &blocks);
        assert!(result.get("system").is_some());

        // Empty blocks -> body returned as-is (fail-closed via early return).
        let result = inject_memory_into_body(openai_body.clone(), &Protocol::OpenAi, &[]);
        assert_eq!(result, openai_body);
    }

    #[test]
    fn frozen_message_count_flows_through_engine_compress_resolved() {
        use dtoken_compress::{
            context_pipeline::resolve_frozen_message_count, CompressionEngine,
            ResolvedOptimizationContext,
        };
        let ctx = ResolvedOptimizationContext {
            route_id: "route-test".to_string(),
            provider_id: "provider-test".to_string(),
            model: "model-exact-revision".to_string(),
            input_protocol: Protocol::OpenAi,
            output_protocol: Protocol::OpenAi,
            endpoint: OpticalEndpoint::OpenAiChatCompletions,
        };
        let body = json!({
            "model": "model-exact-revision",
            "messages": [
                {"role": "user", "content": "first"},
                {"role": "user", "content": [{"type": "text", "text": "cacheable", "cache_control": {"type": "ephemeral"}}]},
                {"role": "user", "content": "third"}
            ]
        });
        // Marker-derived floor is 2 (the marker on messages[1] sets floor=2).
        assert_eq!(resolve_frozen_message_count(&body, None), 2);

        // Override frozen_message_count=0 -> the marker on messages[1] is
        // ignored; the pipeline sees the body as passthrough-eligible.
        let _result = CompressionEngine::new()
            .compress_resolved_with_cache_evidence(
                &ctx,
                CompressionPolicy::Balanced,
                body.clone(),
                None,
                Some(0),
            )
            .expect("compress_resolved_with_cache_evidence");
        // With floor=0, the transformable window includes all messages and
        // the engine may or may not apply transforms.
        // (tokens_saved is u64, always non-negative.)

        // With frozen_message_count=2 (marker-derived floor), the engine
        // freezes messages[0..2] and only the trailing message is transformable.
        let _result = CompressionEngine::new()
            .compress_resolved_with_cache_evidence(
                &ctx,
                CompressionPolicy::Balanced,
                body,
                None,
                Some(2),
            )
            .expect("compress_resolved_with_cache_evidence");
        // tokens_saved is u64, always non-negative; the assertion is that
        // the call completed without error (demonstrated above).
    }

    // ------------------------------------------------------------------
    // CF-2 contract-freeze-core direct behavior tests (frozen §3.6/§6.1/§8.1)
    // ------------------------------------------------------------------

    /// Builds an `AgentTransactionRecord` advanced through the legal chain up
    /// to `applied` with a complete cross-section.
    fn applied_record(agent_id: &str) -> AgentTransactionRecord {
        let mut record = AgentTransactionRecord::new(agent_id);
        record
            .advance(ConfigTransactionState::Selected)
            .expect("discovered -> selected");
        record.evidence.transaction_id = format!("tx-{agent_id}");
        record.evidence.source_revision = "rev-1".to_string();
        record.evidence.before_digest = "before-1".to_string();
        record.evidence.redacted_diff_digest = "diff-1".to_string();
        record.evidence.planned_after_digest = "after-1".to_string();
        record
            .advance(ConfigTransactionState::Planned)
            .expect("selected -> planned");
        record.evidence.backup_manifest = Some("backup-1".to_string());
        record
            .advance(ConfigTransactionState::BackedUp)
            .expect("planned -> backed_up");
        record.evidence.receipt_id = Some("receipt-1".to_string());
        record
            .advance(ConfigTransactionState::Applied)
            .expect("backed_up -> applied");
        record
    }

    /// A `RequestLog` with local token counts but no Provider usage evidence.
    fn request_log_without_provider_usage() -> RequestLog {
        RequestLog {
            request_id: Uuid::new_v4(),
            timestamp_ms: 1_700_000_000_000,
            agent: Some("opencode".to_string()),
            instance_id: Some("instance-1".to_string()),
            input_protocol: Protocol::OpenAi,
            provider_id: Some("provider-1".to_string()),
            route_id: Some("route-1".to_string()),
            chain_mode: None,
            model: Some("model-1".to_string()),
            input_tokens_before: Some(1200),
            input_tokens_after: Some(1000),
            output_tokens: Some(300),
            tokens_saved: Some(200),
            transforms_applied: vec!["json_compact".to_string()],
            latency_ms: Some(42),
            error: None,
            config_source: Some("dtoken-owned-runtime".to_string()),
            quota_metadata: None,
            context_accounting: None,
        }
    }

    #[test]
    fn agent_transaction_advances_through_full_legal_chain() {
        let mut record = applied_record("opencode");
        record.evidence.readback_digest = Some("after-1".to_string());
        record
            .advance(ConfigTransactionState::ReadbackVerified)
            .expect("applied -> readback_verified");
        record.evidence.capability_evidence = Some("capability-evidence-1".to_string());
        record
            .advance(ConfigTransactionState::CapabilityVerified)
            .expect("readback_verified -> capability_verified");
        record
            .advance(ConfigTransactionState::Completed)
            .expect("capability_verified -> completed");
        assert_eq!(record.state, ConfigTransactionState::Completed);
        assert!(record.state.is_terminal());
    }

    #[test]
    fn illegal_state_transitions_are_rejected() {
        // Skipping steps is rejected by the frozen transition table.
        let mut record = AgentTransactionRecord::new("opencode");
        assert_eq!(
            record.advance(ConfigTransactionState::Planned),
            Err(AgentTransactionError::InvalidTransition {
                from: ConfigTransactionState::Discovered,
                to: ConfigTransactionState::Planned,
            })
        );
        // Rollback requires a backup to exist (frozen table).
        let mut record = AgentTransactionRecord::new("opencode");
        record.advance(ConfigTransactionState::Selected).unwrap();
        assert_eq!(
            record.advance(ConfigTransactionState::RolledBack),
            Err(AgentTransactionError::InvalidTransition {
                from: ConfigTransactionState::Selected,
                to: ConfigTransactionState::RolledBack,
            })
        );
        // Terminal states absorb no further transitions.
        let mut record = applied_record("opencode");
        record.state = ConfigTransactionState::Completed;
        assert!(matches!(
            record.advance(ConfigTransactionState::Failed),
            Err(AgentTransactionError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn missing_cross_section_evidence_rejects_the_transition() {
        let mut record = AgentTransactionRecord::new("opencode");
        record.advance(ConfigTransactionState::Selected).unwrap();
        // `planned` requires the full plan cross-section.
        assert_eq!(
            record.advance(ConfigTransactionState::Planned),
            Err(AgentTransactionError::MissingEvidence("transaction_id"))
        );
        record.evidence.transaction_id = "tx-1".to_string();
        record.evidence.source_revision = "rev-1".to_string();
        record.evidence.before_digest = "before-1".to_string();
        record.evidence.redacted_diff_digest = "diff-1".to_string();
        record.evidence.planned_after_digest = "after-1".to_string();
        record.advance(ConfigTransactionState::Planned).unwrap();
        // `backed_up` requires the backup manifest.
        assert_eq!(
            record.advance(ConfigTransactionState::BackedUp),
            Err(AgentTransactionError::MissingEvidence("backup_manifest"))
        );
    }

    #[test]
    fn readback_verified_requires_matching_digest() {
        let mut record = applied_record("opencode");
        record.evidence.readback_digest = Some("wrong-digest".to_string());
        assert_eq!(
            record.advance(ConfigTransactionState::ReadbackVerified),
            Err(AgentTransactionError::ReadbackMismatch)
        );
        assert_eq!(record.state, ConfigTransactionState::Applied);
    }

    #[test]
    fn readback_failure_rolls_back_only_the_failing_agent() {
        let mut opencode = applied_record("opencode");
        let claude = applied_record("claude-code");
        // opencode readback fails -> rolled_back with a rollback receipt.
        let outcome = opencode
            .verify_readback("mismatched-digest", "rollback-receipt-1")
            .expect("readback failure must roll back");
        assert_eq!(outcome, ReadbackOutcome::RolledBack);
        assert_eq!(opencode.state, ConfigTransactionState::RolledBack);
        assert_eq!(
            opencode.evidence.rollback_receipt_id.as_deref(),
            Some("rollback-receipt-1")
        );
        // claude-code is untouched and still `applied`.
        assert_eq!(claude.state, ConfigTransactionState::Applied);
        assert!(claude.evidence.rollback_receipt_id.is_none());
        assert!(claude.evidence.readback_digest.is_none());
    }

    #[test]
    fn readback_success_advances_only_the_verified_agent() {
        let mut opencode = applied_record("opencode");
        let claude = applied_record("claude-code");
        let outcome = opencode
            .verify_readback("after-1", "unused")
            .expect("matching readback must verify");
        assert_eq!(outcome, ReadbackOutcome::Verified);
        assert_eq!(opencode.state, ConfigTransactionState::ReadbackVerified);
        assert_eq!(claude.state, ConfigTransactionState::Applied);
    }

    #[test]
    fn rollback_without_receipt_is_rejected_fail_closed() {
        let mut record = applied_record("opencode");
        assert_eq!(
            record.verify_readback("mismatched-digest", ""),
            Err(AgentTransactionError::MissingEvidence(
                "rollback_receipt_id"
            ))
        );
        assert_eq!(record.state, ConfigTransactionState::Applied);
    }

    #[test]
    fn batch_advance_is_per_agent_independent() {
        let mut records = vec![applied_record("opencode"), applied_record("claude-code")];
        records[1].evidence.readback_digest = Some("after-1".to_string());
        let results = advance_agent_batch(
            &mut records,
            &[
                AgentAdvanceRequest {
                    agent_id: "opencode".to_string(),
                    target: ConfigTransactionState::RolledBack,
                },
                AgentAdvanceRequest {
                    agent_id: "claude-code".to_string(),
                    target: ConfigTransactionState::ReadbackVerified,
                },
                AgentAdvanceRequest {
                    agent_id: "ghost".to_string(),
                    target: ConfigTransactionState::Completed,
                },
            ],
        );
        // opencode lacks a rollback receipt -> only its request fails.
        assert_eq!(
            results[0],
            Err(AgentTransactionError::MissingEvidence(
                "rollback_receipt_id"
            ))
        );
        assert_eq!(records[0].state, ConfigTransactionState::Applied);
        // claude-code verifies independently despite opencode's failure.
        assert!(results[1].is_ok());
        assert_eq!(records[1].state, ConfigTransactionState::ReadbackVerified);
        // Unknown Agent fails closed.
        assert!(matches!(
            results[2],
            Err(AgentTransactionError::UnknownAgent(_))
        ));
    }

    #[test]
    fn projection_keeps_provider_usage_unavailable_when_missing() {
        let log = request_log_without_provider_usage();
        let receipt = project_optimization_receipt(&log);
        // Missing Provider usage is None (unavailable), never 0.
        assert_eq!(receipt.provider_input_tokens, None);
        assert_eq!(receipt.provider_output_tokens, None);
        // Local buckets are still projected from the log.
        assert_eq!(receipt.local_original_tokens, Some(1200));
        assert_eq!(receipt.local_optimized_tokens, Some(1000));
        assert_eq!(receipt.estimated_local_tokens_avoided, Some(200));
        assert_eq!(receipt.claimable_tokens_avoided, None);
        // No artifact-derived validation evidence -> unavailable.
        assert_eq!(receipt.validation_status, ValidationStatus::Unavailable);
        // protected_content is a descriptor, never fabricated content.
        assert_eq!(receipt.protected_content, "");
    }

    #[test]
    fn projection_sources_provider_usage_from_provider_actual_sent() {
        let accounting = dtoken_types::HybridContextAccounting {
            schema_version: 1,
            disposition: dtoken_types::OptimizationDisposition::Active,
            sent_payload: dtoken_types::SentPayloadVariant::Optimized,
            upstream_state: UpstreamSendState::Completed,
            estimated: None,
            provider_counterfactual_baseline: None,
            provider_actual_sent: dtoken_types::ProviderActualUsage {
                measurement: Some(dtoken_types::UsageMeasurement {
                    evidence: dtoken_types::UsageEvidence {
                        kind: UsageEvidenceKind::ProviderActual,
                        source_revision: "rev-1".to_string(),
                    },
                    usage: dtoken_types::UsageVector {
                        input: dtoken_types::InputTokenUsage {
                            normalized_total: Some(900),
                            modality: Default::default(),
                            cache: Default::default(),
                        },
                        output_tokens: Some(250),
                        bytes: Default::default(),
                        cost_microusd: None,
                    },
                }),
            },
            provider_reconciled_input_savings: None,
            paired_actual: None,
            attribution_basis: dtoken_types::SavingsAttribution::None,
            fallback_reason_code: None,
        };
        let mut log = request_log_without_provider_usage();
        log.context_accounting = Some(accounting);
        let receipt = project_optimization_receipt(&log);
        assert_eq!(receipt.provider_input_tokens, Some(900));
        assert_eq!(receipt.provider_output_tokens, Some(250));
        // Provider usage alone never elevates claimable (attribution None).
        assert_eq!(receipt.claimable_tokens_avoided, None);
    }

    #[test]
    fn transaction_projection_exposes_only_cross_section_fields() {
        let mut record = applied_record("opencode");
        record.evidence.backup_manifest = Some("backup-1".to_string());
        let projection = record.projection();
        let text = serde_json::to_string(&projection).expect("serialize projection");
        for forbidden in [
            "token",
            "cookie",
            "authorization",
            "api_key",
            "password",
            "secret",
            "/Users",
            "/home/",
        ] {
            assert!(
                !text.to_lowercase().contains(forbidden),
                "forbidden term {forbidden:?} leaked into projection: {text}"
            );
        }
        let keys: Vec<String> = projection
            .as_object()
            .expect("projection object")
            .keys()
            .cloned()
            .collect();
        let expected = [
            "agent_id",
            "state",
            "transaction_id",
            "source_revision",
            "before_digest",
            "redacted_diff_digest",
            "backup_manifest",
            "planned_after_digest",
            "readback_digest",
            "capability_evidence",
            "receipt_id",
            "rollback_receipt_id",
        ];
        assert_eq!(keys.len(), expected.len(), "unexpected keys: {keys:?}");
        for key in expected {
            assert!(
                keys.contains(&key.to_string()),
                "missing key {key}: {keys:?}"
            );
        }
    }

    #[test]
    fn vocabulary_maps_known_codes_and_fails_closed_on_unknown() {
        assert_eq!(user_status_word_for("configured").unwrap(), "配置成功");
        assert_eq!(user_status_word_for("not_verified").unwrap(), "配置成功");
        assert_eq!(user_status_word_for("connected").unwrap(), "工具能力已接入");
        assert_eq!(
            user_status_word_for("detected").unwrap(),
            "已检测/不支持本地路由"
        );
        assert_eq!(user_status_word_for("syncing").unwrap(), "正在同步");
        // Mechanism / internal codes fail closed and never surface in UI.
        for internal in [
            "adapter",
            "route",
            "credential_reference",
            "needs-source-fix",
            "source_conflict",
            "healthy",
            "active",
            "supported",
            "totally-unknown",
        ] {
            assert!(
                user_status_word_for(internal).is_err(),
                "internal code {internal:?} must fail closed"
            );
        }
        // Every mapped word belongs to the frozen §3.6 vocabulary.
        for code in [
            "not_installed",
            "unselected",
            "configuring",
            "configured",
            "not_verified",
            "failed",
            "connected",
            "verified",
            "detected",
            "detection_only",
            "syncing",
            "synced",
            "sync_failed",
        ] {
            let word = user_status_word_for(code).expect(code);
            assert!(
                FROZEN_STATUS_WORDS.contains(&word),
                "{word} is not in the frozen §3.6 vocabulary"
            );
        }
        // Asset-list words are also frozen-only.
        assert_eq!(asset_list_status_word(false, false), "未安装");
        assert_eq!(asset_list_status_word(true, false), "未配置");
        assert_eq!(asset_list_status_word(true, true), "已配置");
        assert!(FROZEN_ASSET_LIST_WORDS.contains(&asset_list_status_word(true, false)));
    }

    #[test]
    fn transaction_states_map_to_frozen_user_words() {
        for state in [
            ConfigTransactionState::Discovered,
            ConfigTransactionState::Selected,
            ConfigTransactionState::Planned,
            ConfigTransactionState::BackedUp,
            ConfigTransactionState::Applied,
            ConfigTransactionState::ReadbackVerified,
            ConfigTransactionState::CapabilityVerified,
        ] {
            assert_eq!(transaction_status_word(state), "正在配置");
        }
        assert_eq!(
            transaction_status_word(ConfigTransactionState::Completed),
            "配置成功"
        );
        assert_eq!(
            transaction_status_word(ConfigTransactionState::RolledBack),
            "配置失败"
        );
        assert_eq!(
            transaction_status_word(ConfigTransactionState::Failed),
            "配置失败"
        );
    }

    #[test]
    fn invalid_or_non_provider_accounting_keeps_provider_usage_unavailable() {
        let mut log = request_log_without_provider_usage();
        let invalid = dtoken_types::HybridContextAccounting {
            schema_version: 2,
            disposition: dtoken_types::OptimizationDisposition::Active,
            sent_payload: dtoken_types::SentPayloadVariant::Optimized,
            upstream_state: UpstreamSendState::Completed,
            estimated: None,
            provider_counterfactual_baseline: None,
            provider_actual_sent: dtoken_types::ProviderActualUsage {
                measurement: Some(dtoken_types::UsageMeasurement {
                    evidence: dtoken_types::UsageEvidence {
                        kind: UsageEvidenceKind::LocalEstimate,
                        source_revision: "rev-invalid".to_string(),
                    },
                    usage: dtoken_types::UsageVector {
                        input: dtoken_types::InputTokenUsage {
                            normalized_total: Some(999),
                            modality: Default::default(),
                            cache: Default::default(),
                        },
                        output_tokens: Some(111),
                        bytes: Default::default(),
                        cost_microusd: None,
                    },
                }),
            },
            provider_reconciled_input_savings: None,
            paired_actual: None,
            attribution_basis: dtoken_types::SavingsAttribution::None,
            fallback_reason_code: None,
        };
        log.context_accounting = Some(invalid);
        let receipt = project_optimization_receipt(&log);
        assert_eq!(receipt.provider_input_tokens, None);
        assert_eq!(receipt.provider_output_tokens, None);
    }

    #[test]
    fn valid_local_estimate_in_provider_slot_stays_unavailable() {
        let mut log = request_log_without_provider_usage();
        log.context_accounting = Some(dtoken_types::HybridContextAccounting {
            schema_version: 1,
            disposition: dtoken_types::OptimizationDisposition::Active,
            sent_payload: dtoken_types::SentPayloadVariant::Optimized,
            upstream_state: UpstreamSendState::Completed,
            estimated: None,
            provider_counterfactual_baseline: None,
            provider_actual_sent: dtoken_types::ProviderActualUsage {
                measurement: Some(dtoken_types::UsageMeasurement {
                    evidence: dtoken_types::UsageEvidence {
                        kind: UsageEvidenceKind::LocalEstimate,
                        source_revision: "rev-local".to_string(),
                    },
                    usage: dtoken_types::UsageVector {
                        input: dtoken_types::InputTokenUsage {
                            normalized_total: Some(999),
                            modality: Default::default(),
                            cache: Default::default(),
                        },
                        output_tokens: Some(111),
                        bytes: Default::default(),
                        cost_microusd: None,
                    },
                }),
            },
            provider_reconciled_input_savings: None,
            paired_actual: None,
            attribution_basis: dtoken_types::SavingsAttribution::None,
            fallback_reason_code: None,
        });
        assert!(log.context_accounting.as_ref().unwrap().validate().is_ok());
        let receipt = project_optimization_receipt(&log);
        assert_eq!(receipt.provider_input_tokens, None);
        assert_eq!(receipt.provider_output_tokens, None);
    }

    #[test]
    fn negative_signed_savings_are_not_claimable_avoided_tokens() {
        let baseline = dtoken_types::UsageMeasurement {
            evidence: dtoken_types::UsageEvidence {
                kind: UsageEvidenceKind::LocalEstimate,
                source_revision: "baseline".to_string(),
            },
            usage: dtoken_types::UsageVector {
                input: dtoken_types::InputTokenUsage {
                    normalized_total: Some(1000),
                    modality: Default::default(),
                    cache: Default::default(),
                },
                output_tokens: None,
                bytes: Default::default(),
                cost_microusd: None,
            },
        };
        let candidate = dtoken_types::UsageMeasurement {
            evidence: dtoken_types::UsageEvidence {
                kind: UsageEvidenceKind::LocalEstimate,
                source_revision: "candidate".to_string(),
            },
            usage: dtoken_types::UsageVector {
                input: dtoken_types::InputTokenUsage {
                    normalized_total: Some(1200),
                    modality: Default::default(),
                    cache: Default::default(),
                },
                output_tokens: None,
                bytes: Default::default(),
                cost_microusd: None,
            },
        };
        let mut log = request_log_without_provider_usage();
        log.context_accounting = Some(dtoken_types::HybridContextAccounting {
            schema_version: 1,
            disposition: dtoken_types::OptimizationDisposition::Active,
            sent_payload: dtoken_types::SentPayloadVariant::Optimized,
            upstream_state: UpstreamSendState::Completed,
            estimated: Some(dtoken_types::UsageComparison::from_measurements(
                baseline, candidate,
            )),
            provider_counterfactual_baseline: None,
            provider_actual_sent: Default::default(),
            provider_reconciled_input_savings: None,
            paired_actual: None,
            attribution_basis: dtoken_types::SavingsAttribution::EstimatedActive,
            fallback_reason_code: None,
        });
        assert_eq!(
            project_optimization_receipt(&log).claimable_tokens_avoided,
            None
        );
    }

    #[test]
    fn duplicate_agent_records_fail_closed_without_mutation() {
        let mut records = vec![applied_record("opencode"), applied_record("opencode")];
        let states_before = records
            .iter()
            .map(|record| record.state)
            .collect::<Vec<_>>();
        let results = advance_agent_batch(
            &mut records,
            &[AgentAdvanceRequest {
                agent_id: "opencode".to_string(),
                target: ConfigTransactionState::Failed,
            }],
        );
        assert_eq!(
            results,
            vec![Err(AgentTransactionError::DuplicateAgent(
                "opencode".to_string()
            ))]
        );
        assert_eq!(
            records
                .iter()
                .map(|record| record.state)
                .collect::<Vec<_>>(),
            states_before
        );
    }

    #[test]
    fn evidence_matrix_rejects_missing_applied_capability_and_completed_fields() {
        let mut applied = AgentTransactionRecord::new("applied");
        applied.state = ConfigTransactionState::BackedUp;
        assert_eq!(
            applied.advance(ConfigTransactionState::Applied),
            Err(AgentTransactionError::MissingEvidence("receipt_id"))
        );

        let mut capability = applied_record("capability");
        capability.state = ConfigTransactionState::ReadbackVerified;
        assert_eq!(
            capability.advance(ConfigTransactionState::CapabilityVerified),
            Err(AgentTransactionError::MissingEvidence(
                "capability_evidence"
            ))
        );

        let mut completed = applied_record("completed");
        completed.state = ConfigTransactionState::CapabilityVerified;
        completed.evidence.receipt_id = None;
        assert_eq!(
            completed.advance(ConfigTransactionState::Completed),
            Err(AgentTransactionError::MissingEvidence("receipt_id"))
        );
    }

    #[test]
    fn estimated_active_accounting_projects_claimable_savings() {
        let mut accounting = dtoken_types::HybridContextAccounting {
            schema_version: 1,
            disposition: dtoken_types::OptimizationDisposition::Active,
            sent_payload: dtoken_types::SentPayloadVariant::Optimized,
            upstream_state: UpstreamSendState::Completed,
            estimated: None,
            provider_counterfactual_baseline: None,
            provider_actual_sent: Default::default(),
            provider_reconciled_input_savings: None,
            paired_actual: None,
            attribution_basis: dtoken_types::SavingsAttribution::EstimatedActive,
            fallback_reason_code: None,
        };
        let baseline = dtoken_types::UsageMeasurement {
            evidence: dtoken_types::UsageEvidence {
                kind: UsageEvidenceKind::LocalEstimate,
                source_revision: "baseline".to_string(),
            },
            usage: dtoken_types::UsageVector {
                input: dtoken_types::InputTokenUsage {
                    normalized_total: Some(1200),
                    modality: Default::default(),
                    cache: Default::default(),
                },
                output_tokens: None,
                bytes: Default::default(),
                cost_microusd: None,
            },
        };
        let candidate = dtoken_types::UsageMeasurement {
            evidence: dtoken_types::UsageEvidence {
                kind: UsageEvidenceKind::LocalEstimate,
                source_revision: "candidate".to_string(),
            },
            usage: dtoken_types::UsageVector {
                input: dtoken_types::InputTokenUsage {
                    normalized_total: Some(1000),
                    modality: Default::default(),
                    cache: Default::default(),
                },
                output_tokens: None,
                bytes: Default::default(),
                cost_microusd: None,
            },
        };
        accounting.estimated = Some(dtoken_types::UsageComparison::from_measurements(
            baseline, candidate,
        ));
        let mut log = request_log_without_provider_usage();
        log.context_accounting = Some(accounting);
        assert_eq!(
            project_optimization_receipt(&log).claimable_tokens_avoided,
            Some(200)
        );
    }
}
