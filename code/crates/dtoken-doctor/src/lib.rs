use dtoken_types::{DoctorCheck, DoctorReport};
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};

pub const DOCTOR_V3_SCHEMA_VERSION: u16 = 3;

/// Stable product directory for Doctor v3 modules.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DoctorModuleDirectory {
    Core,
    Agent,
    Optical,
    Platform,
    Recovery,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DoctorModuleId {
    CoreServiceLifecycle,
    CorePortProcessIdentity,
    CoreRouteVerification,
    AgentDiscovery,
    AgentTransport,
    AgentInstructions,
    AgentHookPlugin,
    AgentSkill,
    AgentMetadataMcp,
    AgentCcSwitchSync,
    OpticalObservability,
    OpticalShadowPolicy,
    PlatformAutostart,
    PlatformRuntimePackageIdentity,
    RecoveryRollbackEvidence,
    RecoverySpecializedTransactionRollback,
}

impl DoctorModuleId {
    fn as_str(self) -> &'static str {
        match self {
            Self::CoreServiceLifecycle => "core_service_lifecycle",
            Self::CorePortProcessIdentity => "core_port_process_identity",
            Self::CoreRouteVerification => "core_route_verification",
            Self::AgentDiscovery => "agent_discovery",
            Self::AgentTransport => "agent_transport",
            Self::AgentInstructions => "agent_instructions",
            Self::AgentHookPlugin => "agent_hook_plugin",
            Self::AgentSkill => "agent_skill",
            Self::AgentMetadataMcp => "agent_metadata_mcp",
            Self::AgentCcSwitchSync => "agent_cc_switch_sync",
            Self::OpticalObservability => "optical_observability",
            Self::OpticalShadowPolicy => "optical_shadow_policy",
            Self::PlatformAutostart => "platform_autostart",
            Self::PlatformRuntimePackageIdentity => "platform_runtime_package_identity",
            Self::RecoveryRollbackEvidence => "recovery_rollback_evidence",
            Self::RecoverySpecializedTransactionRollback => {
                "recovery_specialized_transaction_rollback"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorRepairClass {
    ScanOnly,
    ReadOnlyRefresh,
    ConfirmedSafe,
    SpecializedConfirmation,
}

impl DoctorRepairClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::ScanOnly => "scan_only",
            Self::ReadOnlyRefresh => "read_only_refresh",
            Self::ConfirmedSafe => "confirmed_safe",
            Self::SpecializedConfirmation => "specialized_confirmation",
        }
    }

    fn requires_confirmation(self) -> bool {
        matches!(self, Self::ConfirmedSafe | Self::SpecializedConfirmation)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorRepairRisk {
    None,
    Low,
    Medium,
    High,
}

impl DoctorRepairRisk {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorSelectionRequirement {
    None,
    Agent,
    Route,
    AgentAndRoute,
}

impl DoctorSelectionRequirement {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Agent => "agent",
            Self::Route => "route",
            Self::AgentAndRoute => "agent_and_route",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorRepairScope {
    Global,
    Project,
}

impl DoctorRepairScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorSnapshotSource {
    Runtime,
    DesktopBridge,
    Combined,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorSnapshotFreshness {
    Fresh,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorEvidenceKind {
    Availability,
    Identity,
    Ownership,
    Configuration,
    Transport,
    Policy,
    Verification,
    Rollback,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorEvidenceState {
    Healthy,
    Attention,
    Blocked,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorRepairOutcome {
    Applied,
    NoChange,
    Cancelled,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorVerificationState {
    Verified,
    Partial,
    Failed,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorDigestAlgorithm {
    Fnv1a64V1,
}

impl DoctorDigestAlgorithm {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fnv1a64V1 => "fnv1a64_v1",
        }
    }
}

/// Static policy metadata. It intentionally contains no command or filesystem path.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorRepairModule {
    pub id: DoctorModuleId,
    pub directory: DoctorModuleDirectory,
    pub repair_class: DoctorRepairClass,
    pub risk: DoctorRepairRisk,
    pub selection_requirement: DoctorSelectionRequirement,
    pub confirmation_required: bool,
}

const fn module(
    id: DoctorModuleId,
    directory: DoctorModuleDirectory,
    repair_class: DoctorRepairClass,
    risk: DoctorRepairRisk,
    selection_requirement: DoctorSelectionRequirement,
) -> DoctorRepairModule {
    DoctorRepairModule {
        id,
        directory,
        repair_class,
        risk,
        selection_requirement,
        confirmation_required: matches!(
            repair_class,
            DoctorRepairClass::ConfirmedSafe | DoctorRepairClass::SpecializedConfirmation
        ),
    }
}

pub const DOCTOR_REPAIR_MODULES: &[DoctorRepairModule] = &[
    module(
        DoctorModuleId::CoreServiceLifecycle,
        DoctorModuleDirectory::Core,
        DoctorRepairClass::ConfirmedSafe,
        DoctorRepairRisk::Low,
        DoctorSelectionRequirement::None,
    ),
    module(
        DoctorModuleId::CorePortProcessIdentity,
        DoctorModuleDirectory::Core,
        DoctorRepairClass::ScanOnly,
        DoctorRepairRisk::None,
        DoctorSelectionRequirement::None,
    ),
    module(
        DoctorModuleId::CoreRouteVerification,
        DoctorModuleDirectory::Core,
        DoctorRepairClass::ReadOnlyRefresh,
        DoctorRepairRisk::None,
        DoctorSelectionRequirement::Route,
    ),
    module(
        DoctorModuleId::AgentDiscovery,
        DoctorModuleDirectory::Agent,
        DoctorRepairClass::ReadOnlyRefresh,
        DoctorRepairRisk::None,
        DoctorSelectionRequirement::None,
    ),
    module(
        DoctorModuleId::AgentTransport,
        DoctorModuleDirectory::Agent,
        DoctorRepairClass::ConfirmedSafe,
        DoctorRepairRisk::Low,
        DoctorSelectionRequirement::AgentAndRoute,
    ),
    module(
        DoctorModuleId::AgentInstructions,
        DoctorModuleDirectory::Agent,
        DoctorRepairClass::ConfirmedSafe,
        DoctorRepairRisk::Low,
        DoctorSelectionRequirement::Agent,
    ),
    module(
        DoctorModuleId::AgentHookPlugin,
        DoctorModuleDirectory::Agent,
        DoctorRepairClass::SpecializedConfirmation,
        DoctorRepairRisk::Medium,
        DoctorSelectionRequirement::Agent,
    ),
    module(
        DoctorModuleId::AgentSkill,
        DoctorModuleDirectory::Agent,
        DoctorRepairClass::ConfirmedSafe,
        DoctorRepairRisk::Low,
        DoctorSelectionRequirement::Agent,
    ),
    module(
        DoctorModuleId::AgentMetadataMcp,
        DoctorModuleDirectory::Agent,
        DoctorRepairClass::SpecializedConfirmation,
        DoctorRepairRisk::Medium,
        DoctorSelectionRequirement::Agent,
    ),
    module(
        DoctorModuleId::AgentCcSwitchSync,
        DoctorModuleDirectory::Agent,
        DoctorRepairClass::SpecializedConfirmation,
        DoctorRepairRisk::Medium,
        DoctorSelectionRequirement::AgentAndRoute,
    ),
    module(
        DoctorModuleId::OpticalObservability,
        DoctorModuleDirectory::Optical,
        DoctorRepairClass::ReadOnlyRefresh,
        DoctorRepairRisk::None,
        DoctorSelectionRequirement::None,
    ),
    module(
        DoctorModuleId::OpticalShadowPolicy,
        DoctorModuleDirectory::Optical,
        DoctorRepairClass::ConfirmedSafe,
        DoctorRepairRisk::Low,
        DoctorSelectionRequirement::None,
    ),
    module(
        DoctorModuleId::PlatformAutostart,
        DoctorModuleDirectory::Platform,
        DoctorRepairClass::SpecializedConfirmation,
        DoctorRepairRisk::Medium,
        DoctorSelectionRequirement::None,
    ),
    module(
        DoctorModuleId::PlatformRuntimePackageIdentity,
        DoctorModuleDirectory::Platform,
        DoctorRepairClass::ReadOnlyRefresh,
        DoctorRepairRisk::None,
        DoctorSelectionRequirement::None,
    ),
    module(
        DoctorModuleId::RecoveryRollbackEvidence,
        DoctorModuleDirectory::Recovery,
        DoctorRepairClass::ScanOnly,
        DoctorRepairRisk::None,
        DoctorSelectionRequirement::None,
    ),
    module(
        DoctorModuleId::RecoverySpecializedTransactionRollback,
        DoctorModuleDirectory::Recovery,
        DoctorRepairClass::SpecializedConfirmation,
        DoctorRepairRisk::High,
        DoctorSelectionRequirement::None,
    ),
];

pub fn doctor_repair_modules() -> &'static [DoctorRepairModule] {
    DOCTOR_REPAIR_MODULES
}

pub fn doctor_repair_module(id: DoctorModuleId) -> Option<&'static DoctorRepairModule> {
    DOCTOR_REPAIR_MODULES
        .iter()
        .find(|candidate| candidate.id == id)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorEvidence {
    pub kind: DoctorEvidenceKind,
    pub state: DoctorEvidenceState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorModuleEvidence {
    pub module_id: DoctorModuleId,
    pub state: DoctorEvidenceState,
    pub observed_at_ms: u64,
    pub evidence: Vec<DoctorEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorSnapshotV3 {
    pub schema_version: u16,
    pub revision: String,
    pub source: DoctorSnapshotSource,
    pub collected_at_ms: u64,
    pub ttl_ms: u64,
    pub overall_state: DoctorEvidenceState,
    pub modules: Vec<DoctorModuleEvidence>,
}

impl DoctorSnapshotV3 {
    pub fn new(
        revision: impl Into<String>,
        source: DoctorSnapshotSource,
        collected_at_ms: u64,
        ttl_ms: u64,
        overall_state: DoctorEvidenceState,
        modules: Vec<DoctorModuleEvidence>,
    ) -> Result<Self, DoctorContractError> {
        let revision = revision.into();
        validate_identifier(&revision, DoctorIdentifierField::SnapshotRevision)?;

        let mut module_ids = modules
            .iter()
            .map(|item| item.module_id)
            .collect::<Vec<_>>();
        module_ids.sort_unstable();
        if module_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(DoctorContractError::DuplicateModuleEvidence);
        }

        Ok(Self {
            schema_version: DOCTOR_V3_SCHEMA_VERSION,
            revision,
            source,
            collected_at_ms,
            ttl_ms,
            overall_state,
            modules,
        })
    }

    pub fn freshness_at(&self, now_ms: u64) -> DoctorSnapshotFreshness {
        if self.schema_version != DOCTOR_V3_SCHEMA_VERSION
            || self.collected_at_ms == 0
            || self.ttl_ms == 0
            || now_ms < self.collected_at_ms
        {
            return DoctorSnapshotFreshness::Unknown;
        }

        match self.collected_at_ms.checked_add(self.ttl_ms) {
            Some(expires_at_ms) if now_ms <= expires_at_ms => DoctorSnapshotFreshness::Fresh,
            Some(_) => DoctorSnapshotFreshness::Stale,
            None => DoctorSnapshotFreshness::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorRepairSelection {
    pub scope: DoctorRepairScope,
    pub agent_ids: Vec<String>,
    pub route_ids: Vec<String>,
}

impl DoctorRepairSelection {
    pub fn canonicalized(&self) -> Result<Self, DoctorContractError> {
        let mut selection = self.clone();
        canonicalize_identifiers(&mut selection.agent_ids, DoctorIdentifierField::AgentId)?;
        canonicalize_identifiers(&mut selection.route_ids, DoctorIdentifierField::RouteId)?;
        Ok(selection)
    }

    fn validate_for(
        &self,
        requirement: DoctorSelectionRequirement,
    ) -> Result<(), DoctorContractError> {
        let has_agents = !self.agent_ids.is_empty();
        let has_routes = !self.route_ids.is_empty();
        let valid = match requirement {
            DoctorSelectionRequirement::None => !has_agents && !has_routes,
            DoctorSelectionRequirement::Agent => has_agents && !has_routes,
            DoctorSelectionRequirement::Route => !has_agents && has_routes,
            DoctorSelectionRequirement::AgentAndRoute => has_agents && has_routes,
        };

        if valid {
            Ok(())
        } else {
            Err(DoctorContractError::SelectionMismatch { requirement })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorRepairPlan {
    pub schema_version: u16,
    pub plan_id: String,
    pub snapshot_revision: String,
    pub module_id: DoctorModuleId,
    pub selection: DoctorRepairSelection,
    pub repair_class: DoctorRepairClass,
    pub risk: DoctorRepairRisk,
    pub confirmation_required: bool,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub digest_algorithm: DoctorDigestAlgorithm,
    pub review_digest: String,
}

impl DoctorRepairPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plan_id: impl Into<String>,
        snapshot_revision: impl Into<String>,
        module_id: DoctorModuleId,
        selection: DoctorRepairSelection,
        created_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self, DoctorContractError> {
        let descriptor =
            doctor_repair_module(module_id).ok_or(DoctorContractError::UnknownModule)?;
        if descriptor.repair_class == DoctorRepairClass::ScanOnly {
            return Err(DoctorContractError::ScanOnlyModule);
        }

        let plan_id = plan_id.into();
        let snapshot_revision = snapshot_revision.into();
        validate_identifier(&plan_id, DoctorIdentifierField::PlanId)?;
        validate_identifier(&snapshot_revision, DoctorIdentifierField::SnapshotRevision)?;
        validate_time_window(created_at_ms, expires_at_ms)?;

        let selection = selection.canonicalized()?;
        selection.validate_for(descriptor.selection_requirement)?;

        let mut plan = Self {
            schema_version: DOCTOR_V3_SCHEMA_VERSION,
            plan_id,
            snapshot_revision,
            module_id,
            selection,
            repair_class: descriptor.repair_class,
            risk: descriptor.risk,
            confirmation_required: descriptor.confirmation_required,
            created_at_ms,
            expires_at_ms,
            digest_algorithm: DoctorDigestAlgorithm::Fnv1a64V1,
            review_digest: String::new(),
        };
        plan.review_digest = plan.compute_review_digest();
        Ok(plan)
    }

    pub fn validate_at(
        &self,
        now_ms: u64,
        expected_snapshot_revision: &str,
    ) -> Result<(), DoctorContractError> {
        if self.schema_version != DOCTOR_V3_SCHEMA_VERSION {
            return Err(DoctorContractError::UnsupportedSchemaVersion {
                actual: self.schema_version,
            });
        }
        validate_identifier(&self.plan_id, DoctorIdentifierField::PlanId)?;
        validate_identifier(
            &self.snapshot_revision,
            DoctorIdentifierField::SnapshotRevision,
        )?;
        validate_identifier(
            expected_snapshot_revision,
            DoctorIdentifierField::SnapshotRevision,
        )?;
        validate_time_window(self.created_at_ms, self.expires_at_ms)?;

        if self.snapshot_revision != expected_snapshot_revision {
            return Err(DoctorContractError::SnapshotRevisionMismatch);
        }
        if now_ms < self.created_at_ms {
            return Err(DoctorContractError::PlanNotYetValid);
        }
        if now_ms >= self.expires_at_ms {
            return Err(DoctorContractError::PlanExpired);
        }
        if self.digest_algorithm != DoctorDigestAlgorithm::Fnv1a64V1 {
            return Err(DoctorContractError::DigestAlgorithmMismatch);
        }

        let descriptor =
            doctor_repair_module(self.module_id).ok_or(DoctorContractError::UnknownModule)?;
        if descriptor.repair_class == DoctorRepairClass::ScanOnly {
            return Err(DoctorContractError::ScanOnlyModule);
        }
        if self.repair_class != descriptor.repair_class
            || self.risk != descriptor.risk
            || self.confirmation_required != descriptor.confirmation_required
            || self.confirmation_required != self.repair_class.requires_confirmation()
        {
            return Err(DoctorContractError::ModuleContractMismatch);
        }

        let canonical_selection = self.selection.canonicalized()?;
        canonical_selection.validate_for(descriptor.selection_requirement)?;
        if self.review_digest != self.compute_review_digest() {
            return Err(DoctorContractError::DigestMismatch);
        }
        Ok(())
    }

    /// FNV-1a is used only as a deterministic stale-review marker, not as a security boundary.
    fn compute_review_digest(&self) -> String {
        let canonical_selection = self
            .selection
            .canonicalized()
            .unwrap_or_else(|_| self.selection.clone());
        let mut digest = Fnv1a64::new();
        digest.part(self.digest_algorithm.as_str());
        digest.part(&self.schema_version.to_string());
        digest.part(&self.plan_id);
        digest.part(&self.snapshot_revision);
        digest.part(self.module_id.as_str());
        digest.part(self.repair_class.as_str());
        digest.part(self.risk.as_str());
        digest.part(if self.confirmation_required {
            "confirmation_required"
        } else {
            "confirmation_not_required"
        });
        digest.part(canonical_selection.scope.as_str());
        for agent_id in &canonical_selection.agent_ids {
            digest.part("agent");
            digest.part(agent_id);
        }
        for route_id in &canonical_selection.route_ids {
            digest.part("route");
            digest.part(route_id);
        }
        digest.part(&self.created_at_ms.to_string());
        digest.part(&self.expires_at_ms.to_string());
        format!("fnv1a64_v1:{:016x}", digest.finish())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorRepairReceipt {
    pub schema_version: u16,
    pub operation_id: String,
    pub plan_id: String,
    pub plan_review_digest: String,
    pub snapshot_revision: String,
    pub module_id: DoctorModuleId,
    pub outcome: DoctorRepairOutcome,
    pub post_verification: DoctorVerificationState,
    pub post_snapshot_revision: Option<String>,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
}

impl DoctorRepairReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn from_plan(
        plan: &DoctorRepairPlan,
        operation_id: impl Into<String>,
        outcome: DoctorRepairOutcome,
        post_verification: DoctorVerificationState,
        post_snapshot_revision: Option<String>,
        started_at_ms: u64,
        completed_at_ms: u64,
    ) -> Result<Self, DoctorContractError> {
        plan.validate_at(started_at_ms, &plan.snapshot_revision)?;
        let operation_id = operation_id.into();
        validate_identifier(&operation_id, DoctorIdentifierField::OperationId)?;
        if completed_at_ms < started_at_ms {
            return Err(DoctorContractError::InvalidReceiptTimeWindow);
        }
        if let Some(revision) = post_snapshot_revision.as_deref() {
            validate_identifier(revision, DoctorIdentifierField::SnapshotRevision)?;
        }
        if post_verification == DoctorVerificationState::Verified
            && post_snapshot_revision.is_none()
        {
            return Err(DoctorContractError::MissingPostVerificationRevision);
        }

        Ok(Self {
            schema_version: DOCTOR_V3_SCHEMA_VERSION,
            operation_id,
            plan_id: plan.plan_id.clone(),
            plan_review_digest: plan.review_digest.clone(),
            snapshot_revision: plan.snapshot_revision.clone(),
            module_id: plan.module_id,
            outcome,
            post_verification,
            post_snapshot_revision,
            started_at_ms,
            completed_at_ms,
        })
    }

    pub fn validate_against(&self, plan: &DoctorRepairPlan) -> Result<(), DoctorContractError> {
        if self.schema_version != DOCTOR_V3_SCHEMA_VERSION {
            return Err(DoctorContractError::UnsupportedSchemaVersion {
                actual: self.schema_version,
            });
        }
        validate_identifier(&self.operation_id, DoctorIdentifierField::OperationId)?;
        if self.completed_at_ms < self.started_at_ms {
            return Err(DoctorContractError::InvalidReceiptTimeWindow);
        }
        if let Some(revision) = self.post_snapshot_revision.as_deref() {
            validate_identifier(revision, DoctorIdentifierField::SnapshotRevision)?;
        }
        if self.post_verification == DoctorVerificationState::Verified
            && self.post_snapshot_revision.is_none()
        {
            return Err(DoctorContractError::MissingPostVerificationRevision);
        }
        if self.plan_id != plan.plan_id
            || self.plan_review_digest != plan.review_digest
            || self.snapshot_revision != plan.snapshot_revision
            || self.module_id != plan.module_id
        {
            return Err(DoctorContractError::ReceiptPlanMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorIdentifierField {
    PlanId,
    OperationId,
    SnapshotRevision,
    AgentId,
    RouteId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorContractError {
    UnsupportedSchemaVersion {
        actual: u16,
    },
    UnknownModule,
    ScanOnlyModule,
    InvalidIdentifier {
        field: DoctorIdentifierField,
    },
    DuplicateModuleEvidence,
    InvalidPlanTimeWindow,
    InvalidReceiptTimeWindow,
    SnapshotRevisionMismatch,
    PlanNotYetValid,
    PlanExpired,
    DigestAlgorithmMismatch,
    DigestMismatch,
    ModuleContractMismatch,
    SelectionMismatch {
        requirement: DoctorSelectionRequirement,
    },
    MissingPostVerificationRevision,
    ReceiptPlanMismatch,
}

impl fmt::Display for DoctorContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { actual } => {
                write!(formatter, "unsupported Doctor schema version {actual}")
            }
            Self::UnknownModule => formatter.write_str("unknown Doctor module"),
            Self::ScanOnlyModule => formatter.write_str("scan-only module has no repair plan"),
            Self::InvalidIdentifier { field } => {
                write!(formatter, "invalid Doctor identifier for {field:?}")
            }
            Self::DuplicateModuleEvidence => {
                formatter.write_str("snapshot contains duplicate module evidence")
            }
            Self::InvalidPlanTimeWindow => formatter.write_str("invalid repair plan time window"),
            Self::InvalidReceiptTimeWindow => {
                formatter.write_str("invalid repair receipt time window")
            }
            Self::SnapshotRevisionMismatch => {
                formatter.write_str("repair plan snapshot revision is stale")
            }
            Self::PlanNotYetValid => formatter.write_str("repair plan is not yet valid"),
            Self::PlanExpired => formatter.write_str("repair plan has expired"),
            Self::DigestAlgorithmMismatch => {
                formatter.write_str("repair plan digest algorithm is unsupported")
            }
            Self::DigestMismatch => formatter.write_str("repair plan review digest does not match"),
            Self::ModuleContractMismatch => {
                formatter.write_str("repair plan module policy does not match the registry")
            }
            Self::SelectionMismatch { requirement } => write!(
                formatter,
                "repair selection does not satisfy {}",
                requirement.as_str()
            ),
            Self::MissingPostVerificationRevision => {
                formatter.write_str("verified receipt requires a post-snapshot revision")
            }
            Self::ReceiptPlanMismatch => {
                formatter.write_str("repair receipt is not bound to the supplied plan")
            }
        }
    }
}

impl std::error::Error for DoctorContractError {}

fn validate_identifier(
    value: &str,
    field: DoctorIdentifierField,
) -> Result<(), DoctorContractError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if valid {
        Ok(())
    } else {
        Err(DoctorContractError::InvalidIdentifier { field })
    }
}

fn canonicalize_identifiers(
    values: &mut Vec<String>,
    field: DoctorIdentifierField,
) -> Result<(), DoctorContractError> {
    for value in values.iter() {
        validate_identifier(value, field)?;
    }
    values.sort_unstable();
    values.dedup();
    Ok(())
}

fn validate_time_window(created_at_ms: u64, expires_at_ms: u64) -> Result<(), DoctorContractError> {
    if created_at_ms == 0 || expires_at_ms <= created_at_ms {
        Err(DoctorContractError::InvalidPlanTimeWindow)
    } else {
        Ok(())
    }
}

struct Fnv1a64(u64);

impl Fnv1a64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x00000100000001b3;

    fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    fn part(&mut self, value: &str) {
        for byte in (value.len() as u64)
            .to_le_bytes()
            .iter()
            .chain(value.as_bytes())
        {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

pub fn run_basic_doctor(port: u16) -> DoctorReport {
    let bind_check = check_local_port(port);
    let ok = bind_check.ok;

    DoctorReport {
        ok,
        checks: vec![
            DoctorCheck {
                name: "local-only-bind".to_string(),
                ok: true,
                message: "M1 requires local services to bind to 127.0.0.1 by default".to_string(),
            },
            bind_check,
        ],
    }
}

pub fn run_dev_no_touch_doctor() -> DoctorReport {
    let profile = env::var("DTOKEN_PROFILE").unwrap_or_else(|_| "fixture".to_string());
    let temp_home = env::var("DTOKEN_TEST_HOME").ok();
    let fake_path = env::var("DTOKEN_FAKE_BIN").ok();

    let profile_ok = matches!(
        profile.as_str(),
        "fixture" | "fake-binary" | "fake_binary" | "shadow" | "external"
    );
    let no_real_binary_by_default =
        profile == "fixture" || fake_path.is_some() || profile == "shadow" || profile == "external";
    let home_ok =
        profile == "fixture" || temp_home.is_some() || profile == "shadow" || profile == "external";

    let checks = vec![
        DoctorCheck {
            name: "dev-profile".to_string(),
            ok: profile_ok,
            message: format!("development profile is {profile}"),
        },
        DoctorCheck {
            name: "no-real-binary-default".to_string(),
            ok: no_real_binary_by_default,
            message: "fixture mode does not call real RTK or Headroom binaries".to_string(),
        },
        DoctorCheck {
            name: "temporary-home-for-command-tests".to_string(),
            ok: home_ok,
            message: "fake-binary tests must set DTOKEN_TEST_HOME before command execution"
                .to_string(),
        },
        DoctorCheck {
            name: "no-config-apply-default".to_string(),
            ok: true,
            message: "ConfigPatcher exposes plan/backup/apply, but API defaults to plan-only"
                .to_string(),
        },
    ];

    DoctorReport {
        ok: checks.iter().all(|check| check.ok),
        checks,
    }
}

fn check_local_port(port: u16) -> DoctorCheck {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    match TcpListener::bind(addr) {
        Ok(_) => DoctorCheck {
            name: "port-available".to_string(),
            ok: true,
            message: format!("127.0.0.1:{port} is available"),
        },
        Err(err) => DoctorCheck {
            name: "port-available".to_string(),
            ok: false,
            message: format!("127.0.0.1:{port} is not available: {err}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn selection(
        scope: DoctorRepairScope,
        agent_ids: &[&str],
        route_ids: &[&str],
    ) -> DoctorRepairSelection {
        DoctorRepairSelection {
            scope,
            agent_ids: agent_ids.iter().map(|value| (*value).to_string()).collect(),
            route_ids: route_ids.iter().map(|value| (*value).to_string()).collect(),
        }
    }

    fn transport_plan() -> DoctorRepairPlan {
        DoctorRepairPlan::new(
            "plan-1",
            "snapshot-1",
            DoctorModuleId::AgentTransport,
            selection(
                DoctorRepairScope::Project,
                &["agent-b", "agent-a", "agent-a"],
                &["route-a"],
            ),
            1_000,
            2_000,
        )
        .expect("transport plan should be valid")
    }

    #[test]
    fn fixture_profile_passes_no_touch_doctor_without_real_services() {
        env::remove_var("DTOKEN_PROFILE");
        env::remove_var("DTOKEN_FAKE_BIN");
        env::remove_var("DTOKEN_TEST_HOME");

        let report = run_dev_no_touch_doctor();

        assert!(report.ok);
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "no-real-binary-default" && check.ok));
    }

    #[test]
    fn registry_covers_every_product_directory_with_unique_module_ids() {
        let directories = doctor_repair_modules()
            .iter()
            .map(|entry| entry.directory)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            directories,
            BTreeSet::from([
                DoctorModuleDirectory::Core,
                DoctorModuleDirectory::Agent,
                DoctorModuleDirectory::Optical,
                DoctorModuleDirectory::Platform,
                DoctorModuleDirectory::Recovery,
            ])
        );

        let unique_ids = doctor_repair_modules()
            .iter()
            .map(|entry| entry.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(unique_ids.len(), doctor_repair_modules().len());
        assert!(doctor_repair_modules().iter().all(|entry| {
            entry.confirmation_required == entry.repair_class.requires_confirmation()
        }));
    }

    #[test]
    fn snapshot_freshness_uses_ttl_boundary_and_rejects_invalid_time_evidence() {
        let snapshot = DoctorSnapshotV3::new(
            "snapshot-1",
            DoctorSnapshotSource::Runtime,
            1_000,
            100,
            DoctorEvidenceState::Healthy,
            vec![],
        )
        .expect("snapshot should be valid");

        assert_eq!(snapshot.freshness_at(1_100), DoctorSnapshotFreshness::Fresh);
        assert_eq!(snapshot.freshness_at(1_101), DoctorSnapshotFreshness::Stale);
        assert_eq!(snapshot.freshness_at(999), DoctorSnapshotFreshness::Unknown);

        let mut invalid = snapshot;
        invalid.ttl_ms = 0;
        assert_eq!(
            invalid.freshness_at(1_000),
            DoctorSnapshotFreshness::Unknown
        );
        invalid.ttl_ms = u64::MAX;
        assert_eq!(
            invalid.freshness_at(1_000),
            DoctorSnapshotFreshness::Unknown
        );
    }

    #[test]
    fn plan_digest_is_stable_for_canonical_selection() {
        let first = transport_plan();
        let second = DoctorRepairPlan::new(
            "plan-1",
            "snapshot-1",
            DoctorModuleId::AgentTransport,
            selection(
                DoctorRepairScope::Project,
                &["agent-a", "agent-b"],
                &["route-a", "route-a"],
            ),
            1_000,
            2_000,
        )
        .expect("equivalent plan should be valid");

        assert_eq!(first.selection, second.selection);
        assert_eq!(first.review_digest, second.review_digest);
        assert!(first.review_digest.starts_with("fnv1a64_v1:"));
    }

    #[test]
    fn plan_digest_detects_semantic_mutation() {
        let mut plan = transport_plan();
        plan.selection.agent_ids.push("agent-c".to_string());

        assert_eq!(
            plan.validate_at(1_500, "snapshot-1"),
            Err(DoctorContractError::DigestMismatch)
        );
    }

    #[test]
    fn plan_rejects_stale_snapshot_revision_and_expiry_boundary() {
        let plan = transport_plan();
        assert_eq!(
            plan.validate_at(1_500, "snapshot-2"),
            Err(DoctorContractError::SnapshotRevisionMismatch)
        );
        assert_eq!(
            plan.validate_at(2_000, "snapshot-1"),
            Err(DoctorContractError::PlanExpired)
        );
    }

    #[test]
    fn agent_module_requires_explicit_agent_selection() {
        let error = DoctorRepairPlan::new(
            "plan-1",
            "snapshot-1",
            DoctorModuleId::AgentInstructions,
            selection(DoctorRepairScope::Project, &[], &[]),
            1_000,
            2_000,
        )
        .expect_err("agent module must require an explicit Agent ID");

        assert_eq!(
            error,
            DoctorContractError::SelectionMismatch {
                requirement: DoctorSelectionRequirement::Agent,
            }
        );
    }

    #[test]
    fn scan_only_module_cannot_create_a_repair_plan() {
        let error = DoctorRepairPlan::new(
            "plan-1",
            "snapshot-1",
            DoctorModuleId::CorePortProcessIdentity,
            selection(DoctorRepairScope::Global, &[], &[]),
            1_000,
            2_000,
        )
        .expect_err("scan-only module must not create repair work");

        assert_eq!(error, DoctorContractError::ScanOnlyModule);
    }

    #[test]
    fn identifiers_cannot_smuggle_paths_or_commands() {
        let error = DoctorRepairPlan::new(
            "../../plan",
            "snapshot-1",
            DoctorModuleId::CoreServiceLifecycle,
            selection(DoctorRepairScope::Global, &[], &[]),
            1_000,
            2_000,
        )
        .expect_err("path-shaped plan ID must be rejected");

        assert_eq!(
            error,
            DoctorContractError::InvalidIdentifier {
                field: DoctorIdentifierField::PlanId,
            }
        );
    }

    #[test]
    fn serde_contract_is_snake_case_and_round_trips() {
        let plan = transport_plan();
        let value = serde_json::to_value(&plan).expect("plan should serialize");
        assert_eq!(value["module_id"], "agent_transport");
        assert_eq!(value["repair_class"], "confirmed_safe");
        assert_eq!(value["selection"]["scope"], "project");
        assert_eq!(value["digest_algorithm"], "fnv1a64_v1");

        let decoded: DoctorRepairPlan =
            serde_json::from_value(value).expect("plan should deserialize");
        assert_eq!(decoded, plan);
        assert_eq!(decoded.validate_at(1_500, "snapshot-1"), Ok(()));
    }

    #[test]
    fn receipt_is_bound_to_plan_and_post_verification_revision() {
        let plan = transport_plan();
        let receipt = DoctorRepairReceipt::from_plan(
            &plan,
            "operation-1",
            DoctorRepairOutcome::Applied,
            DoctorVerificationState::Verified,
            Some("snapshot-2".to_string()),
            1_500,
            1_600,
        )
        .expect("receipt should bind to plan");
        assert_eq!(receipt.validate_against(&plan), Ok(()));

        let mut other_plan = plan;
        other_plan.plan_id = "plan-2".to_string();
        assert_eq!(
            receipt.validate_against(&other_plan),
            Err(DoctorContractError::ReceiptPlanMismatch)
        );
    }
}
