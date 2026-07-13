use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    OpenAi,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Official,
    Custom,
    LocalChain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChainMode {
    Direct,
    ChainBridge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompressionPolicy {
    Off,
    Safe,
    Balanced,
    Aggressive,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationConfidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputVerbosity {
    Auto,
    Terse,
    Normal,
    Detailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStatus {
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Found,
    Missing,
    WrongPackage,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    Installed,
    Missing,
    Partial,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityState {
    Verified,
    Tampered,
    NoBaseline,
    NotInstalled,
    OrphanedHash,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryStatus {
    Disabled,
    Enabled,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreprocessorImplementation {
    RtkExternal,
    RtkManaged,
    DtokenBuiltin,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayeredStage {
    RtkRewrite,
    RtkFilter,
    HeadroomRoute,
    HeadroomCompress,
    HeadroomRetrieve,
    RouteForward,
    ProviderForward,
    OpticalPreview,
    OpticalPlan,
    OpticalRender,
    OpticalPatch,
    OpticalValidate,
    OpticalFallback,
    UpstreamSendCommitted,
    OpticalComplete,
    InstallStep,
    ConfigPatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Rtk,
    Headroom,
    Dtoken,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Ok,
    Passthrough,
    Failed,
    Disabled,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaselineKind {
    RawCli,
    RtkCleaned,
    HeadroomInput,
    FinalLlmInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigToolKind {
    Rtk,
    Headroom,
    Dtoken,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Fixture,
    FakeBinary,
    Shadow,
    External,
    Managed,
    BuiltinFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityStatus {
    Untested,
    Pass,
    Fail,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Full,
    Partial,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub protocol: Protocol,
    pub base_url: String,
    pub api_key_ref: Option<String>,
    pub models: Vec<String>,
    pub timeout_ms: u64,
    pub healthcheck_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfile {
    pub id: String,
    pub name: String,
    pub input_protocol: Protocol,
    pub instance_id: String,
    pub target_provider_id: String,
    pub fallback_provider_id: Option<String>,
    pub chain_mode: ChainMode,
    pub compression_policy: CompressionPolicy,
    pub bind_host: String,
    pub local_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceProfile {
    pub id: String,
    pub name: String,
    pub agent: String,
    pub route_id: String,
    pub local_port: u16,
    pub process_status: ProcessStatus,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProfile {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub language_stack: Vec<String>,
    pub agents: Vec<String>,
    pub default_instance_id: Option<String>,
    pub headroom_policy_id: Option<String>,
    pub rtk_profile_id: Option<String>,
    pub provider_route_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCompressionPolicy {
    pub id: String,
    pub project_id: String,
    pub level: CompressionPolicy,
    pub recommended_level: CompressionPolicy,
    pub recommendation_confidence: RecommendationConfidence,
    pub enable_ccr: bool,
    pub enable_memory: bool,
    pub enable_learn: bool,
    pub enable_mcp: bool,
    pub enable_output_shaper: bool,
    pub enable_cache_aligner: bool,
    pub enable_code_compressor: bool,
    pub enable_json_smartcrusher: bool,
    pub enable_text_kompress: bool,
    pub output_verbosity: OutputVerbosity,
    pub holdout_ratio: Option<f32>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionRecommendation {
    pub project_id: String,
    pub recommended_level: CompressionPolicy,
    pub confidence: RecommendationConfidence,
    pub reasons: Vec<String>,
    pub risks: Vec<String>,
    pub expected_savings_min: f32,
    pub expected_savings_max: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorProfile {
    pub id: String,
    pub name: String,
    pub implementation: PreprocessorImplementation,
    pub binary_path: Option<String>,
    pub version: Option<String>,
    pub agent_hook_status: HookStatus,
    pub telemetry_status: TelemetryStatus,
    pub config_path: Option<String>,
    pub tracking_db_path: Option<String>,
    pub tee_dir: Option<String>,
    pub supported_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtkStatus {
    pub status: ToolStatus,
    pub binary_path: Option<String>,
    pub version: Option<String>,
    pub install_method: Option<String>,
    pub hook_status: HookStatus,
    pub integrity_state: IntegrityState,
    pub telemetry_status: TelemetryStatus,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtkGainReport {
    pub available: bool,
    pub total_commands: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub saved_tokens: u64,
    pub savings_percent: f32,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtkDiscoverReport {
    pub available: bool,
    pub source: String,
    pub missed_opportunities: Vec<String>,
    pub raw_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtkSessionReport {
    pub available: bool,
    pub source: String,
    pub sessions: Vec<String>,
    pub raw_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadroomStatus {
    pub status: ToolStatus,
    pub binary_path: Option<String>,
    pub version: Option<String>,
    pub python_runtime: Option<String>,
    pub proxy_running: bool,
    pub mcp_configured: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadroomStats {
    pub available: bool,
    pub compressions: u64,
    pub retrievals: u64,
    pub tokens_saved: u64,
    pub savings_percent: f32,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CcrCacheSummary {
    pub available: bool,
    pub entries: u64,
    pub bytes: u64,
    pub retrievals: u64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadroomMetricsReport {
    pub available: bool,
    pub source: String,
    pub prometheus_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayeredSavingsReport {
    pub rtk_savings: u64,
    pub headroom_savings: u64,
    pub end_to_end_savings: u64,
    pub attribution_confidence: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityCase {
    pub id: String,
    pub os: String,
    pub arch: String,
    pub shell: String,
    pub agent: String,
    pub rtk_mode: RuntimeMode,
    pub headroom_mode: RuntimeMode,
    pub expected_hook_support: SupportLevel,
    pub expected_proxy_support: SupportLevel,
    pub status: CompatibilityStatus,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSupportProfile {
    pub agent: String,
    pub rtk_supported: bool,
    pub rtk_method: String,
    pub rtk_transparent_rewrite: SupportLevel,
    pub headroom_supported: bool,
    pub headroom_method: String,
    pub macos_support: SupportLevel,
    pub windows_native_support: SupportLevel,
    pub windows_wsl_support: SupportLevel,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolInjectionProfile {
    pub protocol: Protocol,
    pub compatible_clients: Vec<String>,
    pub injection_method: String,
    pub development_default: String,
    pub managed_mode_action: String,
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayeredCompressionStage {
    pub request_id: Option<Uuid>,
    pub command_id: Option<String>,
    pub project_id: Option<String>,
    pub stage: LayeredStage,
    pub source: EventSource,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub tokens_saved: Option<u64>,
    pub savings_percent: Option<f32>,
    pub transforms_applied: Vec<String>,
    pub raw_recovery_ref: Option<String>,
    pub status: EventStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DtokenEvent {
    pub event_id: Uuid,
    pub timestamp_ms: u64,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub instance_id: Option<String>,
    pub command_id: Option<String>,
    pub request_id: Option<Uuid>,
    pub stage: LayeredStage,
    pub source: EventSource,
    pub status: EventStatus,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub saved_tokens: Option<u64>,
    pub baseline_kind: Option<BaselineKind>,
    pub error_code: Option<String>,
    pub metadata: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_accounting: Option<HybridContextAccounting>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPatchPlan {
    pub id: String,
    pub target_file: String,
    pub tool: ConfigToolKind,
    pub agent: Option<String>,
    pub before_hash: String,
    pub after_hash: String,
    pub diff: String,
    pub risk_level: RiskLevel,
    pub requires_restart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPoint {
    pub id: String,
    pub created_at: String,
    pub backup_dir: String,
    pub target_files: Vec<String>,
    pub patch_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    pub request_id: Uuid,
    pub route_id: String,
    pub provider_id: String,
    pub chain_mode: ChainMode,
    pub compression_policy: CompressionPolicy,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_protocol: Option<Protocol>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_protocol: Option<Protocol>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub tokens_saved: usize,
    #[serde(default)]
    pub elapsed_us: u128,
    pub transforms_applied: Vec<String>,
    pub body: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_accounting: Option<HybridContextAccounting>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageEvidenceKind {
    LocalEstimate,
    ProviderActual,
    PairedActual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageEvidence {
    pub kind: UsageEvidenceKind,
    pub source_revision: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ModalityTokens {
    pub text: Option<u64>,
    pub vision: Option<u64>,
    pub other: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CacheTokens {
    pub uncached: Option<u64>,
    pub read: Option<u64>,
    pub creation: Option<u64>,
    pub creation_5m: Option<u64>,
    pub creation_1h: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct InputTokenUsage {
    pub normalized_total: Option<u64>,
    pub modality: ModalityTokens,
    pub cache: CacheTokens,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PayloadByteUsage {
    pub request_json: Option<u64>,
    pub png_binary: Option<u64>,
    pub base64_encoded: Option<u64>,
    pub wire_body: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UsageVector {
    pub input: InputTokenUsage,
    pub output_tokens: Option<u64>,
    pub bytes: PayloadByteUsage,
    pub cost_microusd: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageMeasurement {
    pub evidence: UsageEvidence,
    pub usage: UsageVector,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SignedSavings {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub request_bytes: Option<i64>,
    pub png_bytes: Option<i64>,
    pub base64_bytes: Option<i64>,
    pub cost_microusd: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageComparison {
    pub baseline: UsageMeasurement,
    pub candidate: UsageMeasurement,
    pub signed_savings: SignedSavings,
}

impl UsageComparison {
    pub fn from_measurements(baseline: UsageMeasurement, candidate: UsageMeasurement) -> Self {
        let baseline_input = baseline.usage.input.normalized_total;
        let candidate_input = candidate.usage.input.normalized_total;
        let baseline_output = baseline.usage.output_tokens;
        let candidate_output = candidate.usage.output_tokens;
        let baseline_bytes = outbound_request_bytes(&baseline.usage.bytes);
        let candidate_bytes = outbound_request_bytes(&candidate.usage.bytes);

        Self {
            signed_savings: SignedSavings {
                input_tokens: signed_difference(baseline_input, candidate_input),
                output_tokens: signed_difference(baseline_output, candidate_output),
                total_tokens: signed_total_difference(
                    baseline_input,
                    baseline_output,
                    candidate_input,
                    candidate_output,
                ),
                request_bytes: signed_difference(baseline_bytes, candidate_bytes),
                png_bytes: signed_difference(
                    baseline.usage.bytes.png_binary,
                    candidate.usage.bytes.png_binary,
                ),
                base64_bytes: signed_difference(
                    baseline.usage.bytes.base64_encoded,
                    candidate.usage.bytes.base64_encoded,
                ),
                cost_microusd: signed_difference(
                    baseline.usage.cost_microusd,
                    candidate.usage.cost_microusd,
                ),
            },
            baseline,
            candidate,
        }
    }
}

fn outbound_request_bytes(bytes: &PayloadByteUsage) -> Option<u64> {
    bytes.wire_body.or(bytes.request_json)
}

fn signed_difference(baseline: Option<u64>, candidate: Option<u64>) -> Option<i64> {
    Some(saturating_i64_difference(baseline?, candidate?))
}

fn signed_total_difference(
    baseline_input: Option<u64>,
    baseline_output: Option<u64>,
    candidate_input: Option<u64>,
    candidate_output: Option<u64>,
) -> Option<i64> {
    let baseline = u128::from(baseline_input?) + u128::from(baseline_output?);
    let candidate = u128::from(candidate_input?) + u128::from(candidate_output?);
    Some(saturating_i64_difference_u128(baseline, candidate))
}

fn saturating_i64_difference(baseline: u64, candidate: u64) -> i64 {
    saturating_i64_difference_u128(u128::from(baseline), u128::from(candidate))
}

fn saturating_i64_difference_u128(baseline: u128, candidate: u128) -> i64 {
    if baseline >= candidate {
        i64::try_from(baseline - candidate).unwrap_or(i64::MAX)
    } else {
        i64::try_from(candidate - baseline)
            .map(|value| -value)
            .unwrap_or(i64::MIN)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationDisposition {
    Passthrough,
    Shadow,
    Active,
    SemanticActive,
    FallbackPreSend,
    FallbackOriginalSent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SentPayloadVariant {
    Original,
    Optimized,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamSendState {
    NotAttempted,
    SendCommitted,
    ResponseStarted,
    StreamStarted,
    Completed,
    FailedBeforeCommit,
    FailedAfterCommit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SavingsAttribution {
    None,
    EstimatedActive,
    ProviderReconciledActual,
    PairedActual,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderActualUsage {
    pub measurement: Option<UsageMeasurement>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountingValidationError {
    UnsupportedSchemaVersion,
    PostCommitCannotBePreSendFallback,
    NonActiveDispositionCannotClaimSavings,
    ActiveDispositionMustSendOptimizedPayload,
    NonActiveDispositionMustSendOriginalPayload,
    SavingsClaimRequiresCompletedSend,
    EstimatedAttributionRequiresEstimate,
    ProviderAttributionRequiresCounterfactualAndActual,
    PairedAttributionRequiresPairedActual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HybridContextAccounting {
    pub schema_version: u32,
    pub disposition: OptimizationDisposition,
    pub sent_payload: SentPayloadVariant,
    pub upstream_state: UpstreamSendState,
    pub estimated: Option<UsageComparison>,
    pub provider_counterfactual_baseline: Option<UsageMeasurement>,
    pub provider_actual_sent: ProviderActualUsage,
    pub provider_reconciled_input_savings: Option<i64>,
    pub paired_actual: Option<UsageComparison>,
    pub attribution_basis: SavingsAttribution,
    pub fallback_reason_code: Option<String>,
}

impl HybridContextAccounting {
    pub fn validate(&self) -> Result<(), AccountingValidationError> {
        if self.schema_version != 1 {
            return Err(AccountingValidationError::UnsupportedSchemaVersion);
        }
        if self.disposition == OptimizationDisposition::FallbackPreSend
            && self.upstream_state == UpstreamSendState::FailedAfterCommit
        {
            return Err(AccountingValidationError::PostCommitCannotBePreSendFallback);
        }
        if matches!(
            self.disposition,
            OptimizationDisposition::Active | OptimizationDisposition::SemanticActive
        ) {
            if self.sent_payload != SentPayloadVariant::Optimized {
                return Err(AccountingValidationError::ActiveDispositionMustSendOptimizedPayload);
            }
        } else {
            if self.sent_payload != SentPayloadVariant::Original {
                return Err(AccountingValidationError::NonActiveDispositionMustSendOriginalPayload);
            }
            if self.attribution_basis != SavingsAttribution::None {
                return Err(AccountingValidationError::NonActiveDispositionCannotClaimSavings);
            }
        }

        if self.attribution_basis != SavingsAttribution::None
            && self.upstream_state != UpstreamSendState::Completed
        {
            return Err(AccountingValidationError::SavingsClaimRequiresCompletedSend);
        }
        match self.attribution_basis {
            SavingsAttribution::None => Ok(()),
            SavingsAttribution::EstimatedActive if self.estimated.is_some() => Ok(()),
            SavingsAttribution::EstimatedActive => {
                Err(AccountingValidationError::EstimatedAttributionRequiresEstimate)
            }
            SavingsAttribution::ProviderReconciledActual
                if self.provider_counterfactual_baseline.is_some()
                    && self.provider_actual_sent.measurement.is_some()
                    && self.provider_reconciled_input_savings.is_some() =>
            {
                Ok(())
            }
            SavingsAttribution::ProviderReconciledActual => {
                Err(AccountingValidationError::ProviderAttributionRequiresCounterfactualAndActual)
            }
            SavingsAttribution::PairedActual if self.paired_actual.is_some() => Ok(()),
            SavingsAttribution::PairedActual => {
                Err(AccountingValidationError::PairedAttributionRequiresPairedActual)
            }
        }
    }

    pub fn claimable_signed_savings(&self) -> Option<&SignedSavings> {
        if self.validate().is_err()
            || !matches!(
                self.disposition,
                OptimizationDisposition::Active | OptimizationDisposition::SemanticActive
            )
            || self.sent_payload != SentPayloadVariant::Optimized
            || self.upstream_state != UpstreamSendState::Completed
        {
            return None;
        }
        match self.attribution_basis {
            SavingsAttribution::EstimatedActive => self
                .estimated
                .as_ref()
                .map(|comparison| &comparison.signed_savings),
            SavingsAttribution::PairedActual => self
                .paired_actual
                .as_ref()
                .map(|comparison| &comparison.signed_savings),
            SavingsAttribution::None | SavingsAttribution::ProviderReconciledActual => None,
        }
    }

    pub fn provider_reconciled_signed_savings(&self) -> Option<SignedSavings> {
        if self.validate().is_err()
            || self.attribution_basis != SavingsAttribution::ProviderReconciledActual
        {
            return None;
        }
        let baseline = self.provider_counterfactual_baseline.clone()?;
        let actual = self.provider_actual_sent.measurement.clone()?;
        Some(UsageComparison::from_measurements(baseline, actual).signed_savings)
    }

    pub fn legacy_positive_tokens_saved(&self) -> u64 {
        self.claimable_signed_savings()
            .and_then(|savings| savings.total_tokens.or(savings.input_tokens))
            .filter(|tokens| *tokens > 0)
            .and_then(|tokens| u64::try_from(tokens).ok())
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLog {
    pub request_id: Uuid,
    pub timestamp_ms: u64,
    pub agent: Option<String>,
    pub instance_id: Option<String>,
    pub input_protocol: Protocol,
    pub provider_id: Option<String>,
    pub route_id: Option<String>,
    pub chain_mode: Option<ChainMode>,
    pub model: Option<String>,
    pub input_tokens_before: Option<usize>,
    pub input_tokens_after: Option<usize>,
    pub output_tokens: Option<usize>,
    pub tokens_saved: Option<usize>,
    pub transforms_applied: Vec<String>,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    pub config_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_accounting: Option<HybridContextAccounting>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub component: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
}

// Optical Compression Types

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpticalMode {
    Off,
    #[default]
    ShadowPreview,
    ToolResultsOnly,
    HistoryOnly,
    FullExperimental,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFamily {
    Anthropic,
    OpenAi,
    #[default]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpticalEndpoint {
    AnthropicMessages,
    OpenAiChatCompletions,
    OpenAiResponses,
    #[default]
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Supported,
    Limited,
    ShadowOnly,
    Blocked,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VisionExactnessClass {
    SummaryOnly,
    TechnicalReadable,
    ExactStringUnsafe,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VisionTokenFormula {
    PixelAreaDivisor {
        pixels_per_token: f64,
        safety_margin_percent: f64,
    },
    PatchGrid {
        patch_size_px: u32,
        max_tokens: usize,
        safety_margin_percent: f64,
    },
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpticalApplicabilityReason {
    Eligible,
    GlobalDisabled,
    RouteDisabled,
    ActiveDisabled,
    ExperimentalConfirmationRequired,
    RouteModeDisabled,
    UnsupportedMethod,
    UnsupportedPath,
    EmptyBody,
    UnsupportedProtocol,
    ProtocolEndpointMismatch,
    UnsupportedModel,
    UnknownModelShadowOnly,
    OverBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpticalPlanStatus {
    #[default]
    Disabled,
    Ineligible,
    ValidatedShadowPlan,
    ValidatedActivePlan,
    FailedClosed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OpticalBucket {
    StaticSlab,
    Reminder,
    ToolResultJson,
    ToolResultLog,
    ToolResultProse,
    ToolResultTable,
    AnthropicHistory,
    GptHistory,
    SchemaAnnotation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalModelCapability {
    pub provider_family: ProviderFamily,
    pub provider_id: Option<String>,
    pub model_base: Option<String>,
    pub endpoint: OpticalEndpoint,
    pub status: CapabilityStatus,
    pub max_mode: OpticalMode,
    pub supports_image_input: bool,
    pub supports_anthropic_image_block: bool,
    pub supports_openai_image_url: bool,
    pub supports_openai_input_image: bool,
    pub supports_data_url: bool,
    pub max_images_per_request: usize,
    pub max_image_width_px: usize,
    pub max_image_height_px: usize,
    pub vision_token_formula: VisionTokenFormula,
    pub exactness_class: VisionExactnessClass,
}

impl Default for OpticalModelCapability {
    fn default() -> Self {
        Self {
            provider_family: ProviderFamily::Other,
            provider_id: None,
            model_base: None,
            endpoint: OpticalEndpoint::Unsupported,
            status: CapabilityStatus::Unknown,
            max_mode: OpticalMode::Off,
            supports_image_input: false,
            supports_anthropic_image_block: false,
            supports_openai_image_url: false,
            supports_openai_input_image: false,
            supports_data_url: false,
            max_images_per_request: 0,
            max_image_width_px: 0,
            max_image_height_px: 0,
            vision_token_formula: VisionTokenFormula::Unknown,
            exactness_class: VisionExactnessClass::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalApplicabilityInput {
    pub method: Option<String>,
    pub endpoint_path: Option<String>,
    pub body_bytes: Option<usize>,
    pub protocol: Option<Protocol>,
    pub provider_id: Option<String>,
    pub route_id: Option<String>,
    pub model: Option<String>,
    pub global_enabled: bool,
    pub route_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalApplicabilityDecision {
    pub eligible_for_shadow: bool,
    pub eligible_for_active: bool,
    pub reason: OpticalApplicabilityReason,
    pub normalized_model_base: Option<String>,
    pub endpoint: Option<OpticalEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpticalGlobalPolicy {
    pub enabled: bool,
    pub default_mode: OpticalMode,
    pub allow_active: bool,
    pub max_images_per_request: usize,
    pub max_candidate_chars_per_request: usize,
    pub max_plan_time_ms: u64,
    pub max_render_time_ms: u64,
    pub max_total_pixels: u64,
    pub max_png_bytes: u64,
    pub max_base64_bytes: u64,
    pub max_outbound_body_bytes: u64,
    pub max_memory_bytes: u64,
    pub max_concurrent_renders: usize,
    pub render_queue_timeout_ms: u64,
    pub public_summary_only: bool,
}

impl Default for OpticalGlobalPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            default_mode: OpticalMode::ShadowPreview,
            allow_active: false,
            max_images_per_request: 10,
            max_candidate_chars_per_request: 512000,
            max_plan_time_ms: 50,
            max_render_time_ms: 1500,
            max_total_pixels: 16_000_000,
            max_png_bytes: 32 * 1024 * 1024,
            max_base64_bytes: 48 * 1024 * 1024,
            max_outbound_body_bytes: 64 * 1024 * 1024,
            max_memory_bytes: 128 * 1024 * 1024,
            max_concurrent_renders: 2,
            render_queue_timeout_ms: 100,
            public_summary_only: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RouteOpticalPolicy {
    pub enabled: bool,
    pub mode: OpticalMode,
    pub allow_tool_result: bool,
    pub allow_history: bool,
    pub allow_schema_annotations: bool,
    pub allow_system_static_slab: bool,
    pub min_net_saved_tokens: isize,
    pub confirmed_experimental_lossy: bool,
}

impl Default for RouteOpticalPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: OpticalMode::Off,
            allow_tool_result: false,
            allow_history: false,
            allow_schema_annotations: false,
            allow_system_static_slab: false,
            min_net_saved_tokens: 1024,
            confirmed_experimental_lossy: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpticalCapabilityLifecycle {
    #[default]
    Active,
    Revoked,
    Superseded,
}

fn default_optical_snapshot_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalCapabilitySnapshot {
    #[serde(default = "default_optical_snapshot_schema_version")]
    pub schema_version: u32,
    pub snapshot_id: String,
    pub revision: u64,
    pub enabled: bool,
    #[serde(default)]
    pub lifecycle: OpticalCapabilityLifecycle,
    #[serde(default)]
    pub revoked_at_ms: Option<u64>,
    #[serde(default)]
    pub revocation_reason: Option<String>,
    #[serde(default)]
    pub supersedes_snapshot_id: Option<String>,
    pub provider_family: ProviderFamily,
    pub provider_id: Option<String>,
    #[serde(default)]
    pub route_id: Option<String>,
    pub model_base: String,
    pub endpoint: OpticalEndpoint,
    pub capability: OpticalModelCapability,
    pub verified_at_ms: u64,
    pub source: String,
    pub notes: Option<String>,
    #[serde(default)]
    pub attestation: Option<OpticalCapabilityAttestation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpticalCapabilityAttestation {
    pub schema_version: u32,
    pub algorithm: String,
    pub authority_id: String,
    pub key_id: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub renderer_profile_digest: String,
    pub font_digest: String,
    pub eval_artifact_digest: String,
    pub signature_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpticalTrustedAuthority {
    pub authority_id: String,
    pub key_id: String,
    pub public_key_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpticalEvalArtifact {
    pub schema_version: u32,
    pub artifact_id: String,
    pub model_base: String,
    pub endpoint: OpticalEndpoint,
    pub renderer_profile_digest: String,
    pub font_digest: String,
    pub corpus_digest: String,
    pub evaluated_at_ms: u64,
    pub sample_count: u64,
    pub exact_failures: u64,
    pub unsafe_failures: u64,
    pub baseline_passes: u64,
    pub candidate_passes: u64,
    pub quality_delta: f64,
    pub signed_input_token_delta: i64,
    pub signed_cost_delta_microusd: i64,
    pub latency_p95_delta_ms: i64,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteOpticalPolicyEntry {
    pub route_id: String,
    pub policy: RouteOpticalPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpticalRuntimeConfig {
    #[serde(default = "default_optical_runtime_schema_version")]
    pub schema_version: u32,
    pub global_policy: OpticalGlobalPolicy,
    pub route_policies: Vec<RouteOpticalPolicyEntry>,
    pub capability_snapshots: Vec<OpticalCapabilitySnapshot>,
    pub trusted_authorities: Vec<OpticalTrustedAuthority>,
    pub eval_artifacts: Vec<OpticalEvalArtifact>,
}

fn default_optical_runtime_schema_version() -> u32 {
    1
}

impl Default for OpticalRuntimeConfig {
    fn default() -> Self {
        Self {
            schema_version: default_optical_runtime_schema_version(),
            global_policy: OpticalGlobalPolicy::default(),
            route_policies: Vec::new(),
            capability_snapshots: Vec::new(),
            trusted_authorities: Vec::new(),
            eval_artifacts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalGateDecision {
    pub name: String,
    pub passed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpticalPlanCounters {
    pub candidate_blocks: usize,
    pub preserved_blocks: usize,
    pub skipped_blocks: usize,
    pub exact_spans: usize,
    pub secret_spans: usize,
    pub estimated_images: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalBucketSummary {
    pub bucket: OpticalBucket,
    pub blocks: usize,
    pub chars: usize,
    pub estimated_net_saved_tokens: isize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachePublicSummary {
    pub stable_prefix_digest_present: bool,
    pub frozen_sections: usize,
    pub cache_tokens_at_risk: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpticalEconomicsPublicSummary {
    pub text_tokens_before: usize,
    pub image_tokens_after: usize,
    pub sidecar_tokens: usize,
    pub cache_burn_tokens: usize,
    pub net_saved_tokens: isize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalPlanPublicSummary {
    pub plan_id: String,
    pub mode: OpticalMode,
    pub final_status: OpticalPlanStatus,
    pub gates: Vec<OpticalGateDecision>,
    pub counters: OpticalPlanCounters,
    pub buckets: Vec<OpticalBucketSummary>,
    pub cache: CachePublicSummary,
    pub economics: OpticalEconomicsPublicSummary,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalPreviewRequest {
    pub endpoint_path: String,
    pub protocol: Option<Protocol>,
    pub provider_id: Option<String>,
    pub route_id: Option<String>,
    pub model: Option<String>,
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalStatusPublicSummary {
    pub schema_version: u32,
    pub enabled: bool,
    /// Configured operator intent. This does not by itself authorize active execution.
    pub configured_mode: OpticalMode,
    /// Effective mode after route, attestation, evaluation, and runtime gates.
    pub effective_mode: OpticalMode,
    /// Backward-compatible alias for `effective_mode`.
    pub default_mode: OpticalMode,
    pub allow_active: bool,
    pub public_summary_only: bool,
    pub fail_closed: bool,
    pub renderer_ready: bool,
    pub patcher_ready: bool,
    pub post_validator_ready: bool,
    pub signed_accounting_ready: bool,
    pub active_direct_routes: usize,
    pub verified_attested_capabilities: usize,
    pub rejected_capabilities: usize,
    pub config_issue_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalCapabilitiesPublicSummary {
    pub shadow_preview_supported: bool,
    pub active_transform_enabled: bool,
    pub max_mode: OpticalMode,
    pub supported_endpoints: Vec<OpticalEndpoint>,
    pub unknown_model_status: CapabilityStatus,
    pub unsupported_path_status: CapabilityStatus,
    pub public_summary_only: bool,
    pub fail_closed: bool,
    pub direct_anthropic_upstream_supported: bool,
    pub streaming_passthrough_supported: bool,
    pub deterministic_png_renderer: bool,
    pub artifact_post_validation: bool,
    pub signed_multimodal_accounting: bool,
    pub capability_attestation_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpticalCapabilityLifecyclePublicSummary {
    pub schema_version: u32,
    pub configured_snapshots: usize,
    pub active_snapshots: usize,
    pub revoked_snapshots: usize,
    pub superseded_snapshots: usize,
    pub verified_rules: usize,
    pub rejected_rules: usize,
    pub trusted_authorities: usize,
    pub eval_artifacts: usize,
    pub passing_eval_artifacts: usize,
    pub attestation_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpticalEventMetadataPublicSummary {
    pub kind: String,
    pub reason_code: Option<String>,
    pub final_status: OpticalPlanStatus,
    pub mode: OpticalMode,
    pub candidate_blocks: usize,
    pub preserved_blocks: usize,
    pub skipped_blocks: usize,
    pub estimated_images: usize,
    pub patched_blocks: usize,
    pub png_bytes: u64,
    pub encoded_image_bytes: u64,
    pub net_saved_tokens: isize,
    pub artifact_validated: bool,
    pub attribution_basis: Option<SavingsAttribution>,
    pub upstream_state: Option<UpstreamSendState>,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub tool_dependency_edges: usize,
    pub closed_tool_results: usize,
    pub orphan_tool_results: usize,
    pub open_tool_uses: usize,
    pub route_policy_found: bool,
    pub configured_rules: usize,
    pub verified_rules: usize,
    pub rejected_rules: usize,
    pub snapshot_revision_present: bool,
    pub raw_available: bool,
}

impl Default for OpticalEventMetadataPublicSummary {
    fn default() -> Self {
        Self {
            kind: String::new(),
            reason_code: None,
            final_status: OpticalPlanStatus::Disabled,
            mode: OpticalMode::Off,
            candidate_blocks: 0,
            preserved_blocks: 0,
            skipped_blocks: 0,
            estimated_images: 0,
            patched_blocks: 0,
            png_bytes: 0,
            encoded_image_bytes: 0,
            net_saved_tokens: 0,
            artifact_validated: false,
            attribution_basis: None,
            upstream_state: None,
            graph_nodes: 0,
            graph_edges: 0,
            tool_dependency_edges: 0,
            closed_tool_results: 0,
            orphan_tool_results: 0,
            open_tool_uses: 0,
            route_policy_found: false,
            configured_rules: 0,
            verified_rules: 0,
            rejected_rules: 0,
            snapshot_revision_present: false,
            raw_available: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalEventPublicSummary {
    pub event_id: Uuid,
    pub timestamp_ms: u64,
    pub stage: LayeredStage,
    pub status: EventStatus,
    pub metadata: OpticalEventMetadataPublicSummary,
}
