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
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpticalApplicabilityReason {
    Eligible,
    GlobalDisabled,
    RouteDisabled,
    UnsupportedMethod,
    UnsupportedPath,
    EmptyBody,
    UnsupportedProtocol,
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
pub struct OpticalGlobalPolicy {
    pub enabled: bool,
    pub default_mode: OpticalMode,
    pub allow_active: bool,
    pub max_images_per_request: usize,
    pub max_candidate_chars_per_request: usize,
    pub max_plan_time_ms: u64,
    pub max_render_time_ms: u64,
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
            public_summary_only: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
