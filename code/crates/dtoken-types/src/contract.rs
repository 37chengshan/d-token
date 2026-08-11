//! Canonical unified-compression contracts (T-C0A).
//!
//! This module is the single frozen authority for the unified-compression IR
//! vocabulary, the global safety policy, and the four-dimension accounting
//! buckets. Every stage slice consumes this module; no adapter may redefine
//! its own [`NodeAction`], [`SafetyPolicy`], or accounting buckets.
//!
//! All constructors validate their inputs and every field is private: the
//! contract types are immutable evidence, never ad-hoc mutable state.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::{AccountingBucket, NodeAction};

/// Contract construction/validation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractError {
    EmptyNodeId,
    DuplicateNodeId,
    EmptyNodes,
    EmptySourceDigest,
    EmptyScopeIdentity,
    EmptyTransform,
    UnknownNodeId,
    MismatchedRequestId,
}

/// Global safety policy (PRODUCT-8). This is not an engine chooser: the
/// default is `safe_lossless`, and enhanced/lossy policies must never
/// execute without explicit confirmation. Automatic stage selection is
/// decided only by the canonical planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SafetyPolicy {
    /// Default: lossless transforms only; safe fallback is always allowed.
    #[default]
    SafeLossless,
    /// Enhanced transforms require explicit confirmation before execution.
    Enhanced,
    /// Lossy transforms require explicit confirmation before execution.
    Lossy,
}

impl SafetyPolicy {
    /// The global default (PRODUCT-8).
    pub const fn default() -> Self {
        Self::SafeLossless
    }

    /// Explicitly confirmed enhanced policy.
    pub const fn enhanced() -> Self {
        Self::Enhanced
    }

    /// Explicitly confirmed lossy policy.
    pub const fn lossy() -> Self {
        Self::Lossy
    }

    /// Enhanced and lossy policies never execute without explicit
    /// confirmation (PRODUCT-8: 未确认有损永不执行).
    pub const fn requires_explicit_confirmation(self) -> bool {
        matches!(self, Self::Enhanced | Self::Lossy)
    }

    /// Safe-lossless policies may always fall back to the baseline.
    pub const fn is_safe_lossless(self) -> bool {
        matches!(self, Self::SafeLossless)
    }

    /// Canonical mapping from a route compression policy to the global
    /// safety classification. The route intent is classified once here so
    /// every surface shares one policy authority.
    pub const fn from_compression_policy(policy: &crate::CompressionPolicy) -> Self {
        match policy {
            crate::CompressionPolicy::Off
            | crate::CompressionPolicy::Safe
            | crate::CompressionPolicy::Balanced => Self::SafeLossless,
            crate::CompressionPolicy::Aggressive => Self::Enhanced,
            crate::CompressionPolicy::Max => Self::Lossy,
        }
    }
}

/// Four-dimension savings accounting authority (RUNTIME-5/9, CONTEXT-3).
///
/// Entries in different buckets may never share a source digest, and this
/// type deliberately exposes **no** total accessor: the four dimensions are
/// independent and are never summed into one savings pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountingBuckets {
    compression: u64,
    command_output: u64,
    provider_cache: u64,
    relay: u64,
}

impl AccountingBuckets {
    pub const fn new(
        compression: u64,
        command_output: u64,
        provider_cache: u64,
        relay: u64,
    ) -> Self {
        Self {
            compression,
            command_output,
            provider_cache,
            relay,
        }
    }

    pub const fn zero() -> Self {
        Self::new(0, 0, 0, 0)
    }

    /// Read one dimension by its canonical bucket.
    pub const fn bucket(self, bucket: AccountingBucket) -> u64 {
        match bucket {
            AccountingBucket::Compression => self.compression,
            AccountingBucket::CommandOutput => self.command_output,
            AccountingBucket::ProviderCache => self.provider_cache,
            AccountingBucket::Relay => self.relay,
        }
    }

    pub const fn compression(self) -> u64 {
        self.compression
    }

    pub const fn command_output(self) -> u64 {
        self.command_output
    }

    pub const fn provider_cache(self) -> u64 {
        self.provider_cache
    }

    pub const fn relay(self) -> u64 {
        self.relay
    }

    /// Attribute saved bytes to exactly one bucket. The caller must
    /// guarantee exclusive attribution: the same source digest may never be
    /// recorded into two buckets (no double count).
    pub fn record(&mut self, bucket: AccountingBucket, saved_bytes: u64) {
        match bucket {
            AccountingBucket::Compression => self.compression += saved_bytes,
            AccountingBucket::CommandOutput => self.command_output += saved_bytes,
            AccountingBucket::ProviderCache => self.provider_cache += saved_bytes,
            AccountingBucket::Relay => self.relay += saved_bytes,
        }
    }
}

/// One canonical IR node: a context node assigned exactly one exclusive
/// planner action (RUNTIME-7). A node with `RtkCommandOutput` is never also
/// `HeadroomSemantic`/`Optical`/`Reuse`, so its byte savings cannot be
/// double counted into another dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrNode {
    node_id: String,
    action: NodeAction,
    source_digest: String,
}

impl IrNode {
    pub fn new(
        node_id: impl Into<String>,
        action: NodeAction,
        source_digest: impl Into<String>,
    ) -> Result<Self, ContractError> {
        let node_id = node_id.into();
        let source_digest = source_digest.into();
        if node_id.trim().is_empty() {
            return Err(ContractError::EmptyNodeId);
        }
        if source_digest.trim().is_empty() {
            return Err(ContractError::EmptySourceDigest);
        }
        Ok(Self {
            node_id,
            action,
            source_digest,
        })
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn action(&self) -> NodeAction {
        self.action
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }
}

/// A request bound to its exact ingress identity and source digest.
///
/// Binding is exclusive: one request id maps to one source digest. The bound
/// request is the canonical validated unit that receipts reference; it
/// carries no body bytes (INV-7 / RUNTIME-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundRequest {
    request_id: Uuid,
    source_digest: String,
    scope_identity: String,
}

impl BoundRequest {
    pub fn new(
        request_id: Uuid,
        source_digest: impl Into<String>,
        scope_identity: impl Into<String>,
    ) -> Result<Self, ContractError> {
        let source_digest = source_digest.into();
        let scope_identity = scope_identity.into();
        if source_digest.trim().is_empty() {
            return Err(ContractError::EmptySourceDigest);
        }
        if scope_identity.trim().is_empty() {
            return Err(ContractError::EmptyScopeIdentity);
        }
        Ok(Self {
            request_id,
            source_digest,
            scope_identity,
        })
    }

    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    pub fn scope_identity(&self) -> &str {
        &self.scope_identity
    }
}

/// Ordered, exclusive-action plan for one request's canonical IR (T-C0A).
///
/// Each node carries exactly one action and node ids are unique within the
/// plan, so a node can never be planned twice with conflicting actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePlan {
    request_id: Uuid,
    nodes: Vec<IrNode>,
}

impl NodePlan {
    pub fn new(request_id: Uuid, nodes: Vec<IrNode>) -> Result<Self, ContractError> {
        if nodes.is_empty() {
            return Err(ContractError::EmptyNodes);
        }
        let mut seen = std::collections::HashSet::new();
        for node in &nodes {
            if !seen.insert(node.node_id()) {
                return Err(ContractError::DuplicateNodeId);
            }
        }
        Ok(Self { request_id, nodes })
    }

    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    pub fn nodes(&self) -> &[IrNode] {
        &self.nodes
    }
}

/// One stage's transform artifact, bound to its node and exclusive action.
///
/// `output_digest` proves the exact bytes the stage produced, so a stage can
/// never claim an artifact it did not produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageArtifact {
    node_id: String,
    action: NodeAction,
    transform: String,
    output_digest: String,
}

impl StageArtifact {
    pub fn new(
        node_id: impl Into<String>,
        action: NodeAction,
        transform: impl Into<String>,
        output_digest: impl Into<String>,
    ) -> Result<Self, ContractError> {
        let node_id = node_id.into();
        let transform = transform.into();
        let output_digest = output_digest.into();
        if node_id.trim().is_empty() {
            return Err(ContractError::EmptyNodeId);
        }
        if transform.trim().is_empty() {
            return Err(ContractError::EmptyTransform);
        }
        if output_digest.trim().is_empty() {
            return Err(ContractError::EmptySourceDigest);
        }
        Ok(Self {
            node_id,
            action,
            transform,
            output_digest,
        })
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn action(&self) -> NodeAction {
        self.action
    }

    pub fn transform(&self) -> &str {
        &self.transform
    }

    pub fn output_digest(&self) -> &str {
        &self.output_digest
    }
}

/// The frozen per-request optimization manifest (T-C0A).
///
/// Binds the request to its source digest, its exclusive-action plan, and
/// the stage artifacts that actually ran. Only the canonical planner may
/// produce it; it is the exclusive evidence for what was planned and what
/// stages executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizationManifest {
    request_id: Uuid,
    source_digest: String,
    plan: NodePlan,
    stages: Vec<StageArtifact>,
}

impl OptimizationManifest {
    pub fn new(
        request_id: Uuid,
        source_digest: impl Into<String>,
        plan: NodePlan,
        stages: Vec<StageArtifact>,
    ) -> Result<Self, ContractError> {
        let source_digest = source_digest.into();
        if source_digest.trim().is_empty() {
            return Err(ContractError::EmptySourceDigest);
        }
        if plan.request_id() != request_id {
            return Err(ContractError::MismatchedRequestId);
        }
        for stage in &stages {
            if !plan
                .nodes()
                .iter()
                .any(|node| node.node_id() == stage.node_id())
            {
                return Err(ContractError::UnknownNodeId);
            }
        }
        Ok(Self {
            request_id,
            source_digest,
            plan,
            stages,
        })
    }

    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    pub fn plan(&self) -> &NodePlan {
        &self.plan
    }

    pub fn stages(&self) -> &[StageArtifact] {
        &self.stages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChainMode, CompressionPolicy, Protocol, RouteDecision};
    use uuid::Uuid;

    fn decision() -> RouteDecision {
        RouteDecision {
            request_id: Uuid::new_v4(),
            route_id: "route-a".to_string(),
            provider_id: "provider-a".to_string(),
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Balanced,
            reason: "reason".to_string(),
            input_protocol: Some(Protocol::OpenAi),
            output_protocol: Some(Protocol::OpenAi),
        }
    }

    #[test]
    fn safety_policy_default_is_safe_lossless_and_never_requires_confirmation() {
        assert_eq!(SafetyPolicy::default(), SafetyPolicy::SafeLossless);
        assert!(!SafetyPolicy::default().requires_explicit_confirmation());
        assert!(SafetyPolicy::default().is_safe_lossless());
        assert!(SafetyPolicy::enhanced().requires_explicit_confirmation());
        assert!(SafetyPolicy::lossy().requires_explicit_confirmation());
        assert!(!SafetyPolicy::enhanced().is_safe_lossless());
    }

    #[test]
    fn safety_policy_classifies_compression_policy_once() {
        assert_eq!(
            SafetyPolicy::from_compression_policy(&CompressionPolicy::Off),
            SafetyPolicy::SafeLossless
        );
        assert_eq!(
            SafetyPolicy::from_compression_policy(&CompressionPolicy::Safe),
            SafetyPolicy::SafeLossless
        );
        assert_eq!(
            SafetyPolicy::from_compression_policy(&CompressionPolicy::Balanced),
            SafetyPolicy::SafeLossless
        );
        assert_eq!(
            SafetyPolicy::from_compression_policy(&CompressionPolicy::Aggressive),
            SafetyPolicy::Enhanced
        );
        assert_eq!(
            SafetyPolicy::from_compression_policy(&CompressionPolicy::Max),
            SafetyPolicy::Lossy
        );
    }

    #[test]
    fn accounting_buckets_are_independent_and_never_summed() {
        let mut buckets = AccountingBuckets::zero();
        buckets.record(AccountingBucket::Compression, 10);
        buckets.record(AccountingBucket::CommandOutput, 20);
        buckets.record(AccountingBucket::ProviderCache, 30);
        buckets.record(AccountingBucket::Relay, 40);
        assert_eq!(buckets.compression(), 10);
        assert_eq!(buckets.command_output(), 20);
        assert_eq!(buckets.provider_cache(), 30);
        assert_eq!(buckets.relay(), 40);
        assert_eq!(
            buckets.bucket(AccountingBucket::Compression),
            10,
            "per-dimension read only"
        );
        // The type exposes no total; dimensions stay independently
        // attributable (RUNTIME-9: 四维不可直接相加).
        let serialized = serde_json::to_value(buckets).unwrap();
        assert_eq!(serialized["compression"], 10);
    }

    #[test]
    fn ir_node_rejects_empty_ids_and_digests() {
        assert_eq!(
            IrNode::new("", NodeAction::Preserve, "digest").unwrap_err(),
            ContractError::EmptyNodeId
        );
        assert_eq!(
            IrNode::new("node-1", NodeAction::Preserve, "").unwrap_err(),
            ContractError::EmptySourceDigest
        );
    }

    #[test]
    fn node_plan_rejects_empty_and_duplicate_nodes() {
        assert_eq!(
            NodePlan::new(Uuid::new_v4(), vec![]).unwrap_err(),
            ContractError::EmptyNodes
        );
        let duplicate = vec![
            IrNode::new("node-1", NodeAction::Preserve, "d").unwrap(),
            IrNode::new("node-1", NodeAction::Reuse, "d").unwrap(),
        ];
        assert_eq!(
            NodePlan::new(Uuid::new_v4(), duplicate).unwrap_err(),
            ContractError::DuplicateNodeId
        );
    }

    #[test]
    fn each_ir_node_carries_exactly_one_exclusive_action() {
        let node = IrNode::new("node-1", NodeAction::RtkCommandOutput, "d").unwrap();
        assert_eq!(node.action(), NodeAction::RtkCommandOutput);
        // A node has a single action field: it can never be both
        // RtkCommandOutput and HeadroomSemantic/Optical/Reuse.
        let serialized = serde_json::to_value(&node).unwrap();
        assert_eq!(serialized["action"], "rtk_command_output");
        assert!(serialized.get("actions").is_none());
    }

    #[test]
    fn optimization_manifest_binds_request_plan_and_stages() {
        let request_id = Uuid::new_v4();
        let node = IrNode::new("node-1", NodeAction::Preserve, "source").unwrap();
        let plan = NodePlan::new(request_id, vec![node]).unwrap();
        let stage = StageArtifact::new("node-1", NodeAction::Preserve, "noop", "out").unwrap();
        let manifest =
            OptimizationManifest::new(request_id, "source", plan.clone(), vec![stage]).unwrap();
        assert_eq!(manifest.request_id(), request_id);
        assert_eq!(manifest.source_digest(), "source");
        assert_eq!(manifest.plan(), &plan);
        assert_eq!(manifest.stages().len(), 1);

        let foreign = OptimizationManifest::new(
            Uuid::new_v4(),
            "source",
            plan,
            vec![StageArtifact::new("node-2", NodeAction::Reuse, "reuse", "out").unwrap()],
        );
        assert_eq!(foreign.unwrap_err(), ContractError::MismatchedRequestId);
    }

    #[test]
    fn stage_artifact_must_belong_to_a_planned_node() {
        let request_id = Uuid::new_v4();
        let node = IrNode::new("node-1", NodeAction::Preserve, "source").unwrap();
        let plan = NodePlan::new(request_id, vec![node]).unwrap();
        let stray = StageArtifact::new("node-9", NodeAction::Reuse, "reuse", "out").unwrap();
        assert_eq!(
            OptimizationManifest::new(request_id, "source", plan, vec![stray]).unwrap_err(),
            ContractError::UnknownNodeId
        );
    }

    #[test]
    fn route_decision_stable_digest_is_deterministic_and_excludes_reason() {
        let first = decision();
        let mut second = first.clone();
        second.reason = "different human prose".to_string();
        assert_eq!(first.stable_digest(), second.stable_digest());
        let mut third = first.clone();
        third.provider_id = "provider-b".to_string();
        assert_ne!(first.stable_digest(), third.stable_digest());
        assert_eq!(first.stable_digest().len(), 64, "sha256 hex digest");
        // The digest is stable across processes: same fields, same bytes.
        assert_eq!(first.stable_digest(), first.clone().stable_digest());
    }
}
