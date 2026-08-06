pub mod agent;
pub mod time;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputVerbosity {
    #[default]
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

/// The only lifecycle actions a d-token-owned integrated RTK adapter may
/// request. These metadata-only operations deliberately do not carry agent
/// configuration bytes, commands, prompts, or credentials.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRtkOperation {
    Install,
    Configure,
    Verify,
    Rollback,
    Uninstall,
}

/// A managed RTK transaction is only successful after a versioned host hook
/// adapter has verified its own write and rollback. `Blocked` is an expected,
/// safe state while no such adapter is shipped for the host platform.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagedRtkTransactionState {
    Planned,
    Installed,
    Configured,
    Verified,
    RolledBack,
    Uninstalled,
    Blocked,
    Failed,
}

/// Platform capability for the managed RTK hook/wrap boundary. This is kept
/// separate from the builtin output filter: an available filter must never be
/// presented as an installed Agent hook.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedRtkPlatformCapability {
    pub platform: agent::HostPlatform,
    pub hook_status: HookStatus,
    pub can_modify_agent_config: bool,
    pub reason: String,
}

/// A redacted, hash-bound plan for a prospective managed RTK lifecycle.
/// `host_config_fingerprint` identifies only the reviewed host configuration;
/// the actual configuration and any credential material remain private to the
/// owning Agent adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedRtkTransactionPlan {
    pub schema_version: u8,
    pub transaction_id: String,
    pub platform: agent::HostPlatform,
    pub host_config_fingerprint: String,
    pub capability: ManagedRtkPlatformCapability,
    pub operations: Vec<ManagedRtkOperation>,
    pub requires_user_confirmation: bool,
}

/// A metadata-only lifecycle receipt. It is safe for the Event Ledger because
/// it records neither a command nor transformed command output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedRtkTransactionReceipt {
    pub schema_version: u8,
    pub transaction_id: String,
    pub operation: ManagedRtkOperation,
    pub state: ManagedRtkTransactionState,
    pub host_config_fingerprint: String,
    pub summary: String,
}

/// Byte-level usage evidence for one command result. The output itself is
/// intentionally absent: callers may deliver it directly to the Agent but
/// must not persist it as RTK telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedRtkUsageEvidence {
    pub schema_version: u8,
    pub transaction_id: String,
    pub exit_code: Option<i32>,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub saved_bytes: usize,
    pub transforms: Vec<String>,
    pub disposition: String,
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
    Native,
    Fixture,
    FakeBinary,
    Shadow,
    External,
    Managed,
    BuiltinFallback,
}

impl RuntimeMode {
    pub fn from_profile(profile: Option<&str>, allow_fixture: bool) -> Self {
        match profile {
            Some("fixture") if allow_fixture => Self::Fixture,
            Some("fake-binary") | Some("fake_binary") => Self::FakeBinary,
            Some("shadow") => Self::Shadow,
            Some("external") => Self::External,
            Some("managed") => Self::Managed,
            Some("builtin-fallback") | Some("builtin_fallback") => Self::BuiltinFallback,
            _ => Self::Native,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    ConfiguredRoute,
    ConfiguredInstance,
    SessionMetadata,
    Derived,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceValue<T> {
    pub value: Option<T>,
    pub source: EvidenceSource,
    pub reason_code: Option<String>,
}

impl<T> EvidenceValue<T> {
    pub fn known(value: T, source: EvidenceSource) -> Self {
        Self {
            value: Some(value),
            source,
            reason_code: None,
        }
    }

    pub fn unknown(reason_code: impl Into<String>) -> Self {
        Self {
            value: None,
            source: EvidenceSource::Unavailable,
            reason_code: Some(reason_code.into()),
        }
    }

    fn is_valid(&self) -> bool {
        match (&self.value, &self.source, &self.reason_code) {
            (Some(_), EvidenceSource::Unavailable, _) => false,
            (Some(_), _, None) => true,
            (None, EvidenceSource::Unavailable, Some(reason)) => !reason.trim().is_empty(),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequestScope {
    pub request_id: Uuid,
    pub agent: EvidenceValue<String>,
    pub project: EvidenceValue<String>,
    pub session: EvidenceValue<String>,
    pub instance: EvidenceValue<String>,
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub input_protocol: Protocol,
    pub output_protocol: Protocol,
    pub config_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestScopeError {
    InvalidIdentityEvidence(&'static str),
    EmptyRouteId,
    EmptyProviderId,
    EmptyModel,
    EmptyConfigRevision,
    ConfigSerialization,
}

impl RequestScope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: Uuid,
        agent: EvidenceValue<String>,
        project: EvidenceValue<String>,
        session: EvidenceValue<String>,
        instance: EvidenceValue<String>,
        route_id: impl Into<String>,
        provider_id: impl Into<String>,
        model: impl Into<String>,
        input_protocol: Protocol,
        output_protocol: Protocol,
        config_revision: impl Into<String>,
    ) -> Result<Self, RequestScopeError> {
        for (field, evidence) in [
            ("agent", &agent),
            ("project", &project),
            ("session", &session),
            ("instance", &instance),
        ] {
            if !evidence.is_valid() {
                return Err(RequestScopeError::InvalidIdentityEvidence(field));
            }
        }
        let route_id = route_id.into();
        let provider_id = provider_id.into();
        let model = model.into();
        let config_revision = config_revision.into();
        if route_id.trim().is_empty() {
            return Err(RequestScopeError::EmptyRouteId);
        }
        if provider_id.trim().is_empty() {
            return Err(RequestScopeError::EmptyProviderId);
        }
        if model.trim().is_empty() {
            return Err(RequestScopeError::EmptyModel);
        }
        if config_revision.trim().is_empty() {
            return Err(RequestScopeError::EmptyConfigRevision);
        }
        Ok(Self {
            request_id,
            agent,
            project,
            session,
            instance,
            route_id,
            provider_id,
            model,
            input_protocol,
            output_protocol,
            config_revision,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCacheObservationKind {
    AnthropicUsage,
    OpenAiChatUsage,
    OpenAiResponsesUsage,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCachePolicyStrategy {
    AnthropicExplicit,
    OpenAiAutomatic,
    GoogleExplicit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCachePolicy {
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub strategy: ProviderCachePolicyStrategy,
    pub ttl_seconds: u64,
    pub min_prefix_bytes: u64,
}

impl ProviderCachePolicy {
    pub fn is_well_formed(&self) -> bool {
        !self.route_id.trim().is_empty()
            && !self.provider_id.trim().is_empty()
            && !self.model.trim().is_empty()
            && self.ttl_seconds > 0
            && self.min_prefix_bytes > 0
            && self.min_prefix_bytes <= usize::MAX as u64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCacheObservation {
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub protocol: Protocol,
    pub config_revision: String,
    pub kind: ProviderCacheObservationKind,
    pub read_units: Option<u64>,
    pub creation_units: Option<u64>,
    pub creation_5m_units: Option<u64>,
    pub creation_1h_units: Option<u64>,
    pub source_revision: String,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCacheObservationError {
    EmptyIdentity,
    EmptyConfigRevision,
    EmptySourceRevision,
    MissingCounters,
    ProtocolMismatch,
    CounterOutOfRange,
    InvalidObservationTime,
}

impl ProviderCacheObservation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        route_id: impl Into<String>,
        provider_id: impl Into<String>,
        model: impl Into<String>,
        protocol: Protocol,
        config_revision: impl Into<String>,
        kind: ProviderCacheObservationKind,
        read_units: Option<u64>,
        creation_units: Option<u64>,
        creation_5m_units: Option<u64>,
        creation_1h_units: Option<u64>,
        source_revision: impl Into<String>,
        observed_at_ms: u64,
    ) -> Result<Self, ProviderCacheObservationError> {
        let observation = Self {
            route_id: route_id.into(),
            provider_id: provider_id.into(),
            model: model.into(),
            protocol,
            config_revision: config_revision.into(),
            kind,
            read_units,
            creation_units,
            creation_5m_units,
            creation_1h_units,
            source_revision: source_revision.into(),
            observed_at_ms,
        };
        observation.validate()?;
        Ok(observation)
    }

    pub fn validate(&self) -> Result<(), ProviderCacheObservationError> {
        if self.route_id.trim().is_empty()
            || self.provider_id.trim().is_empty()
            || self.model.trim().is_empty()
        {
            return Err(ProviderCacheObservationError::EmptyIdentity);
        }
        if self.config_revision.trim().is_empty() {
            return Err(ProviderCacheObservationError::EmptyConfigRevision);
        }
        if self.source_revision.trim().is_empty() {
            return Err(ProviderCacheObservationError::EmptySourceRevision);
        }
        if self.observed_at_ms == 0 || self.observed_at_ms > i64::MAX as u64 {
            return Err(ProviderCacheObservationError::InvalidObservationTime);
        }
        let counters = [
            self.read_units,
            self.creation_units,
            self.creation_5m_units,
            self.creation_1h_units,
        ];
        if counters.iter().all(Option::is_none) {
            return Err(ProviderCacheObservationError::MissingCounters);
        }
        if counters
            .into_iter()
            .flatten()
            .any(|value| value > i64::MAX as u64)
        {
            return Err(ProviderCacheObservationError::CounterOutOfRange);
        }
        let protocol_matches = matches!(
            (&self.protocol, &self.kind),
            (
                Protocol::Anthropic,
                ProviderCacheObservationKind::AnthropicUsage
            ) | (
                Protocol::OpenAi,
                ProviderCacheObservationKind::OpenAiChatUsage
            ) | (
                Protocol::OpenAi,
                ProviderCacheObservationKind::OpenAiResponsesUsage
            )
        );
        if !protocol_matches {
            return Err(ProviderCacheObservationError::ProtocolMismatch);
        }
        if matches!(
            self.kind,
            ProviderCacheObservationKind::OpenAiChatUsage
                | ProviderCacheObservationKind::OpenAiResponsesUsage
        ) && (self.read_units.is_none()
            || self.creation_units.is_some()
            || self.creation_5m_units.is_some()
            || self.creation_1h_units.is_some())
        {
            return Err(ProviderCacheObservationError::ProtocolMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityState {
    Unavailable,
    ImplementedDisabled,
    Ready,
    Degraded,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFreshness {
    Live,
    Recent,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityEvidence {
    pub id: String,
    pub state: AvailabilityState,
    pub implementation_revision: String,
    pub config_revision: Option<String>,
    pub evidence_revision: Option<String>,
    pub freshness: EvidenceFreshness,
    pub reason_codes: Vec<String>,
    pub last_verified_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityEvidenceError {
    EmptyCapabilityId,
    EmptyImplementationRevision,
    EmptyReasonCodes,
    InvalidReasonCode(String),
    DuplicateReasonCode(String),
    MissingEvidenceRevision,
    MissingVerificationTimestamp,
    MissingVerifiedFreshness,
    EmptyRegistryRevision,
    InvalidCollectionTimestamp,
    DuplicateCapabilityId(String),
}

impl CapabilityEvidence {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        state: AvailabilityState,
        implementation_revision: impl Into<String>,
        config_revision: Option<String>,
        evidence_revision: Option<String>,
        freshness: EvidenceFreshness,
        reason_codes: Vec<String>,
        last_verified_at_ms: Option<u64>,
    ) -> Result<Self, CapabilityEvidenceError> {
        let evidence = Self {
            id: id.into(),
            state,
            implementation_revision: implementation_revision.into(),
            config_revision,
            evidence_revision,
            freshness,
            reason_codes,
            last_verified_at_ms,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn unavailable(
        id: impl Into<String>,
        implementation_revision: impl Into<String>,
        reason_code: impl Into<String>,
    ) -> Result<Self, CapabilityEvidenceError> {
        Self::new(
            id,
            AvailabilityState::Unavailable,
            implementation_revision,
            None,
            None,
            EvidenceFreshness::Unknown,
            vec![reason_code.into()],
            None,
        )
    }

    pub fn is_available(&self) -> bool {
        matches!(
            self.state,
            AvailabilityState::Ready | AvailabilityState::Degraded
        )
    }

    fn validate(&self) -> Result<(), CapabilityEvidenceError> {
        if self.id.trim().is_empty() {
            return Err(CapabilityEvidenceError::EmptyCapabilityId);
        }
        if self.implementation_revision.trim().is_empty() {
            return Err(CapabilityEvidenceError::EmptyImplementationRevision);
        }
        if self.reason_codes.is_empty() {
            return Err(CapabilityEvidenceError::EmptyReasonCodes);
        }
        let mut seen_reason_codes = std::collections::BTreeSet::new();
        for reason in &self.reason_codes {
            if !valid_reason_code(reason) {
                return Err(CapabilityEvidenceError::InvalidReasonCode(reason.clone()));
            }
            if !seen_reason_codes.insert(reason.as_str()) {
                return Err(CapabilityEvidenceError::DuplicateReasonCode(reason.clone()));
            }
        }
        if matches!(
            self.state,
            AvailabilityState::ImplementedDisabled
                | AvailabilityState::Ready
                | AvailabilityState::Degraded
        ) {
            if self
                .evidence_revision
                .as_deref()
                .is_none_or(|revision| revision.trim().is_empty())
            {
                return Err(CapabilityEvidenceError::MissingEvidenceRevision);
            }
            if self
                .last_verified_at_ms
                .is_none_or(|timestamp| timestamp == 0)
            {
                return Err(CapabilityEvidenceError::MissingVerificationTimestamp);
            }
            if matches!(self.freshness, EvidenceFreshness::Unknown) {
                return Err(CapabilityEvidenceError::MissingVerifiedFreshness);
            }
        }
        Ok(())
    }
}

fn valid_reason_code(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes[1..].iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityRegistrySnapshot {
    pub schema_version: u32,
    pub revision: String,
    pub collected_at_ms: u64,
    pub capabilities: Vec<CapabilityEvidence>,
}

impl CapabilityRegistrySnapshot {
    pub fn new(
        revision: impl Into<String>,
        collected_at_ms: u64,
        mut capabilities: Vec<CapabilityEvidence>,
    ) -> Result<Self, CapabilityEvidenceError> {
        let revision = revision.into();
        if revision.trim().is_empty() {
            return Err(CapabilityEvidenceError::EmptyRegistryRevision);
        }
        if collected_at_ms == 0 {
            return Err(CapabilityEvidenceError::InvalidCollectionTimestamp);
        }
        capabilities.sort_by(|left, right| left.id.cmp(&right.id));
        let mut seen = std::collections::BTreeSet::new();
        for capability in &capabilities {
            capability.validate()?;
            if !seen.insert(capability.id.as_str()) {
                return Err(CapabilityEvidenceError::DuplicateCapabilityId(
                    capability.id.clone(),
                ));
            }
        }
        Ok(Self {
            schema_version: 1,
            revision,
            collected_at_ms,
            capabilities,
        })
    }

    pub fn find(&self, id: &str) -> Option<&CapabilityEvidence> {
        self.capabilities
            .binary_search_by(|candidate| candidate.id.as_str().cmp(id))
            .ok()
            .map(|index| &self.capabilities[index])
    }
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

/// Explicit, model-bound authorization for response-side output shaping.
///
/// This is intentionally separate from input compression. A route must name
/// the exact provider and model it permits, and an absent policy is disabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputShapingRoutePolicy {
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub enabled: bool,
    #[serde(default)]
    pub output_verbosity: OutputVerbosity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holdout_ratio: Option<f32>,
}

impl PartialEq for OutputShapingRoutePolicy {
    fn eq(&self, other: &Self) -> bool {
        self.route_id == other.route_id
            && self.provider_id == other.provider_id
            && self.model == other.model
            && self.enabled == other.enabled
            && self.output_verbosity == other.output_verbosity
            && self.holdout_ratio.map(f32::to_bits) == other.holdout_ratio.map(f32::to_bits)
    }
}

impl Eq for OutputShapingRoutePolicy {}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_protocol: Option<Protocol>,
    pub provider_id: Option<String>,
    pub route_id: Option<String>,
    pub chain_mode: Option<ChainMode>,
    pub model: Option<String>,
    pub input_tokens_before: Option<usize>,
    pub input_tokens_after: Option<usize>,
    pub output_tokens: Option<usize>,
    /// Provider-reported input tokens, only when the upstream response carries
    /// usage evidence; `null` (never a fabricated `0`) when usage is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_input_tokens: Option<usize>,
    pub tokens_saved: Option<usize>,
    pub transforms_applied: Vec<String>,
    /// Compression pipeline latency only; see `end_to_end_ms` for the full
    /// request observability window.
    pub latency_ms: Option<u64>,
    /// Terminal status classification aligned with the Event Ledger; `null`
    /// while the request is in flight or for legacy rows without a terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<EventStatus>,
    /// Wall-clock receipt of the request by the d-token runtime (handler entry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub received_at_ms: Option<u64>,
    /// Wall-clock terminal outcome (success or failure classification).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<u64>,
    /// End-to-end observability window: `completed_at_ms - received_at_ms`
    /// when both are available, otherwise `null`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_to_end_ms: Option<u64>,
    pub error: Option<String>,
    pub config_source: Option<String>,
    /// Allowlisted numeric quota metadata captured from an upstream response; never raw headers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_metadata: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_accounting: Option<HybridContextAccounting>,
}

impl RequestLog {
    /// End-to-end observability window when both wall-clock anchors exist.
    pub fn end_to_end(&self) -> Option<u64> {
        match (self.received_at_ms, self.completed_at_ms) {
            (Some(received), Some(completed)) if completed >= received => {
                Some(completed - received)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub component: String,
    pub version: String,
    pub runtime_identity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_instance_nonce: Option<String>,
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
    #[serde(default)]
    pub transform_contract_digest: String,
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
    #[serde(default)]
    pub transform_contract_digest: String,
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

impl OpticalEvalArtifact {
    /// Recomputes the release-critical quality result from the measured fields.
    /// The serialized `passed` member is retained for artifact compatibility and
    /// display only; callers must not use it as an authorization decision.
    pub fn passes_derived_quality_gate(&self) -> bool {
        if self.schema_version != 2
            || self.sample_count < 20
            || self.baseline_passes > self.sample_count
            || self.candidate_passes > self.sample_count
            || self.exact_failures != 0
            || self.unsafe_failures != 0
            || !self.quality_delta.is_finite()
            || self.signed_input_token_delta <= 0
            || self.signed_cost_delta_microusd <= 0
            || self.latency_p95_delta_ms > 2_000
        {
            return false;
        }
        let sample_count = self.sample_count as f64;
        let expected_quality_delta =
            (self.candidate_passes as f64 - self.baseline_passes as f64) / sample_count;
        let candidate_accuracy = self.candidate_passes as f64 / sample_count;
        candidate_accuracy >= 0.95
            && (self.quality_delta - expected_quality_delta).abs() <= f64::EPSILON
            && self.quality_delta >= 0.0
    }
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
    pub conservative_net_saved_tokens: isize,
    pub expected_net_saved_tokens: isize,
    pub optimistic_net_saved_tokens: isize,
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
    /// True only when current config, route, signed capability and runtime
    /// gates make Active reachable. It is not proof that an Active request ran.
    pub active_reachable: bool,
    /// Derived exclusively from a durable terminal Ledger event for a completed
    /// Active request.
    pub active_execution_observed: bool,
    pub public_summary_only: bool,
    pub fail_closed: bool,
    pub renderer_ready: bool,
    pub patcher_ready: bool,
    pub post_validator_ready: bool,
    pub signed_accounting_ready: bool,
    pub representation_plan_ready: bool,
    pub responses_shadow_inventory_ready: bool,
    pub dynamic_page_accounting_ready: bool,
    pub active_direct_routes: usize,
    pub verified_attested_capabilities: usize,
    pub rejected_capabilities: usize,
    pub config_issue_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveOpticalState {
    pub enabled: bool,
    pub configured_mode: OpticalMode,
    pub effective_mode: OpticalMode,
    pub active_reachable: bool,
    pub active_execution_observed: bool,
    pub public_summary_only: bool,
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

#[cfg(test)]
mod capability_contract_tests {
    use super::*;

    #[test]
    fn fixture_profile_is_rejected_when_debug_fixtures_are_not_allowed() {
        assert_eq!(
            RuntimeMode::from_profile(Some("fixture"), false),
            RuntimeMode::Native
        );
        assert_eq!(
            RuntimeMode::from_profile(Some("fixture"), true),
            RuntimeMode::Fixture
        );
        assert_eq!(
            RuntimeMode::from_profile(Some("unexpected-profile"), true),
            RuntimeMode::Native
        );
    }

    #[test]
    fn request_scope_preserves_unknown_identity_as_evidence_not_fake_defaults() {
        let request_id = Uuid::new_v4();
        let scope = RequestScope::new(
            request_id,
            EvidenceValue::known(
                "claude-code".to_string(),
                EvidenceSource::ConfiguredInstance,
            ),
            EvidenceValue::unknown("project_identity_unavailable"),
            EvidenceValue::unknown("session_identity_unavailable"),
            EvidenceValue::known(
                "instance-claude".to_string(),
                EvidenceSource::ConfiguredRoute,
            ),
            "route-claude",
            "provider-a",
            "claude-model",
            Protocol::Anthropic,
            Protocol::Anthropic,
            "config-sha256",
        )
        .expect("valid configured scope");

        assert_eq!(scope.request_id, request_id);
        assert_eq!(scope.agent.value.as_deref(), Some("claude-code"));
        assert_eq!(scope.project.value, None);
        assert_eq!(
            scope.project.reason_code.as_deref(),
            Some("project_identity_unavailable")
        );
        assert!(!serde_json::to_string(&scope)
            .expect("serialize scope")
            .contains("OpenCode"));
    }

    #[test]
    fn implemented_disabled_requires_verified_evidence() {
        let evidence = CapabilityEvidence::new(
            "code-compressor",
            AvailabilityState::ImplementedDisabled,
            "code-compressor-v1",
            None,
            None,
            EvidenceFreshness::Unknown,
            vec!["disabled_by_default".to_string()],
            None,
        );

        assert_eq!(
            evidence.expect_err("disabled implementation without evidence must fail"),
            CapabilityEvidenceError::MissingEvidenceRevision
        );
    }

    #[test]
    fn capability_reason_codes_are_unique_bounded_ascii_metadata() {
        let too_long = "a".repeat(65);
        assert_eq!(
            CapabilityEvidence::unavailable("memory", "memory-v1", too_long.clone())
                .expect_err("oversized reason codes must fail"),
            CapabilityEvidenceError::InvalidReasonCode(too_long)
        );

        let duplicate = CapabilityEvidence::new(
            "memory",
            AvailabilityState::Unavailable,
            "memory-v1",
            None,
            None,
            EvidenceFreshness::Unknown,
            vec!["scope_required".to_string(), "scope_required".to_string()],
            None,
        );
        assert_eq!(
            duplicate.expect_err("duplicate reason codes must fail"),
            CapabilityEvidenceError::DuplicateReasonCode("scope_required".to_string())
        );

        assert!(
            CapabilityEvidence::unavailable("memory", "memory-v1", "scope_required.v2-safe")
                .is_ok()
        );
    }

    #[test]
    fn registry_rejects_duplicate_capability_ids() {
        let unavailable = CapabilityEvidence::unavailable(
            "mcp",
            "mcp-schema-v1",
            "mcp_transport_not_implemented",
        )
        .expect("valid unavailable evidence");

        assert_eq!(
            CapabilityRegistrySnapshot::new(
                "runtime-capabilities-v1",
                1,
                vec![unavailable.clone(), unavailable],
            )
            .expect_err("duplicate IDs must fail"),
            CapabilityEvidenceError::DuplicateCapabilityId("mcp".to_string())
        );
    }

    #[test]
    fn optical_eval_quality_is_derived_from_measurements_not_passed_label() {
        let mut artifact = OpticalEvalArtifact {
            schema_version: 2,
            artifact_id: "eval-1".to_string(),
            model_base: "model".to_string(),
            endpoint: OpticalEndpoint::AnthropicMessages,
            renderer_profile_digest: "renderer".to_string(),
            font_digest: "font".to_string(),
            transform_contract_digest: "contract".to_string(),
            corpus_digest: "corpus".to_string(),
            evaluated_at_ms: 1,
            sample_count: 20,
            exact_failures: 0,
            unsafe_failures: 0,
            baseline_passes: 19,
            candidate_passes: 20,
            quality_delta: 0.05,
            signed_input_token_delta: 1,
            signed_cost_delta_microusd: 1,
            latency_p95_delta_ms: 0,
            passed: false,
        };

        assert!(artifact.passes_derived_quality_gate());
        artifact.candidate_passes = 0;
        artifact.baseline_passes = 0;
        artifact.quality_delta = 0.0;
        artifact.passed = true;
        assert!(!artifact.passes_derived_quality_gate());
    }
}
