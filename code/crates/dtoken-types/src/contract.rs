//! Canonical request-bound contract types.
//!
//! Freezes the canonical request-bound IR surface: `BoundRequest`,
//! `IrNode`/`NodeAction` (exactly one exclusive action per node),
//! `NodePlan`, `StageArtifact` and `OptimizationManifest`, together with the
//! global `SafetyPolicy` (default `safe_lossless`) and the four-dimension
//! `AccountingBuckets` (compression / command-output / provider-cache /
//! relay; never summed naively, unknown stays `None`).
//!
//! Contract invariants:
//! - digests are derived from bytes or canonical serialization, never
//!   supplied by callers;
//! - every node carries exactly one `NodeAction` (an enum, so no
//!   combination is representable);
//! - enhanced/lossy policies require an explicit `confirmation_revision`
//!   (no silent escalation, no engine chooser);
//! - manifest digest covers every field except itself and is re-verifiable.

use crate::{OpticalBucket, Protocol, RouteDecision};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use uuid::Uuid;

/// Canonical contract schema revision. Any breaking change bumps this and
/// requires re-verification of every canonical-digest consumer.
pub const CANONICAL_SCHEMA_VERSION: u32 = 1;

/// Contract construction/validation errors. All variants are stable and
/// machine-readable; adapters must fail closed on any error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    EmptyIngress,
    EmptyDecisionDigest,
    EmptyNodeId,
    EmptySourceRef,
    DuplicateNodeId,
    EmptyNodePlan,
    EmptyStageId,
    EmptySourceDigest,
    EmptyBody,
    MissingConfirmationRevision,
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            ContractError::EmptyIngress => "request ingress is empty",
            ContractError::EmptyDecisionDigest => "decision digest is empty",
            ContractError::EmptyNodeId => "IR node id is empty",
            ContractError::EmptySourceRef => "IR node source ref is empty",
            ContractError::DuplicateNodeId => "IR node ids must be unique within a plan",
            ContractError::EmptyNodePlan => "node plan must contain at least one node",
            ContractError::EmptyStageId => "stage artifact stage id is empty",
            ContractError::EmptySourceDigest => "stage artifact source digest is empty",
            ContractError::EmptyBody => "stage artifact body is empty",
            ContractError::MissingConfirmationRevision => {
                "enhanced/lossy safety policies require an explicit confirmation revision"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ContractError {}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Canonical SHA-256 of a serializable value: object keys are sorted
/// (serde_json default `Map`), no extra whitespace — deterministic across
/// processes and serde versions.
fn canonical_sha256<T: Serialize>(value: &T) -> String {
    let value = serde_json::to_value(value).expect("contract serialization is infallible");
    let canonical = serde_json::to_string(&value).expect("canonical serialization is infallible");
    sha256_hex(canonical.as_bytes())
}

impl RouteDecision {
    /// Stable digest over the full decision: canonical sorted-key
    /// JSON, SHA-256 hex. Every serialized field participates, so any field
    /// change changes the digest. Used to bind requests and receipts to the
    /// exact decision that authorized them.
    pub fn stable_digest(&self) -> String {
        canonical_sha256(self)
    }
}

/// A request bound to its identity evidence, ingress and route decision.
///
/// `ingress_digest` is always derived from the request bytes; callers can
/// never supply it. `decision_digest` binds the route decision that
/// authorized this request (the stable RouteDecision digest).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundRequest {
    pub request_id: Uuid,
    pub protocol: Protocol,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_target_model: Option<String>,
    pub ingress_digest: String,
    pub decision_digest: String,
}

impl BoundRequest {
    pub fn new(
        request_id: Uuid,
        protocol: Protocol,
        project_scope: Option<String>,
        user_target_model: Option<String>,
        ingress: &[u8],
        decision_digest: impl Into<String>,
    ) -> Result<Self, ContractError> {
        if ingress.is_empty() {
            return Err(ContractError::EmptyIngress);
        }
        let decision_digest = decision_digest.into();
        if decision_digest.trim().is_empty() {
            return Err(ContractError::EmptyDecisionDigest);
        }
        Ok(Self {
            request_id,
            protocol,
            project_scope,
            user_target_model,
            ingress_digest: sha256_hex(ingress),
            decision_digest,
        })
    }
}

/// The exclusive action applied to one canonical IR node. Exactly one action
/// per node: this is an enum, so Preserve/Reuse/HeadroomSemantic/
/// RtkCommandOutput/Optical can never be combined on a single node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeAction {
    Preserve { reason: String },
    Reuse { cache_identity: String },
    HeadroomSemantic { codec_id: String },
    RtkCommandOutput { command_id: String },
    Optical { bucket: OpticalBucket },
}

impl NodeAction {
    /// Stable snake_case kind used by planners, receipts and projections.
    pub fn kind(&self) -> &'static str {
        match self {
            NodeAction::Preserve { .. } => "preserve",
            NodeAction::Reuse { .. } => "reuse",
            NodeAction::HeadroomSemantic { .. } => "headroom_semantic",
            NodeAction::RtkCommandOutput { .. } => "rtk_command_output",
            NodeAction::Optical { .. } => "optical",
        }
    }
}

/// One canonical IR node: a source reference plus exactly one `NodeAction`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrNode {
    pub id: String,
    pub source_ref: String,
    pub action: NodeAction,
}

impl IrNode {
    pub fn new(id: String, source_ref: String, action: NodeAction) -> Result<Self, ContractError> {
        if id.trim().is_empty() {
            return Err(ContractError::EmptyNodeId);
        }
        if source_ref.trim().is_empty() {
            return Err(ContractError::EmptySourceRef);
        }
        Ok(Self {
            id,
            source_ref,
            action,
        })
    }

    pub fn action(&self) -> &NodeAction {
        &self.action
    }

    pub fn action_kind(&self) -> &'static str {
        self.action.kind()
    }
}

/// Ordered canonical plan over the request. `plan_digest` is derived from
/// the canonical serialization of the nodes and changes when any node,
/// action, id or source ref changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodePlan {
    pub schema_version: u32,
    pub nodes: Vec<IrNode>,
    pub plan_digest: String,
}

impl NodePlan {
    pub fn new(nodes: Vec<IrNode>) -> Result<Self, ContractError> {
        if nodes.is_empty() {
            return Err(ContractError::EmptyNodePlan);
        }
        let mut seen = HashSet::new();
        for node in &nodes {
            if !seen.insert(node.id.as_str()) {
                return Err(ContractError::DuplicateNodeId);
            }
        }
        let plan_digest = canonical_sha256(&nodes);
        Ok(Self {
            schema_version: CANONICAL_SCHEMA_VERSION,
            nodes,
            plan_digest,
        })
    }
}

/// Output of one pipeline stage. `body_digest` is derived from the stage's
/// produced bytes; `transforms` records the ordered transform ids applied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageArtifact {
    pub stage_id: String,
    pub source_digest: String,
    pub body_digest: String,
    pub transforms: Vec<String>,
}

impl StageArtifact {
    pub fn new(
        stage_id: String,
        source_digest: String,
        body: &[u8],
        transforms: Vec<String>,
    ) -> Result<Self, ContractError> {
        if stage_id.trim().is_empty() {
            return Err(ContractError::EmptyStageId);
        }
        if source_digest.trim().is_empty() {
            return Err(ContractError::EmptySourceDigest);
        }
        if body.is_empty() {
            return Err(ContractError::EmptyBody);
        }
        Ok(Self {
            stage_id,
            source_digest,
            body_digest: sha256_hex(body),
            transforms,
        })
    }
}

/// Global safety policy. Default is `safe_lossless`; enhanced and
/// lossy policies are only constructible with an explicit, non-empty
/// `confirmation_revision`. This is a policy, not an engine chooser: it never
/// selects an engine and never escalates silently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SafetyPolicy {
    #[default]
    SafeLossless,
    Enhanced {
        confirmation_revision: String,
    },
    Lossy {
        confirmation_revision: String,
    },
}

impl SafetyPolicy {
    pub fn enhanced(confirmation_revision: impl Into<String>) -> Result<Self, ContractError> {
        let revision = confirmation_revision.into();
        if revision.trim().is_empty() {
            return Err(ContractError::MissingConfirmationRevision);
        }
        Ok(SafetyPolicy::Enhanced {
            confirmation_revision: revision,
        })
    }

    pub fn lossy(confirmation_revision: impl Into<String>) -> Result<Self, ContractError> {
        let revision = confirmation_revision.into();
        if revision.trim().is_empty() {
            return Err(ContractError::MissingConfirmationRevision);
        }
        Ok(SafetyPolicy::Lossy {
            confirmation_revision: revision,
        })
    }

    pub fn is_lossless(&self) -> bool {
        matches!(self, SafetyPolicy::SafeLossless)
    }

    pub fn confirmation_revision(&self) -> Option<&str> {
        match self {
            SafetyPolicy::SafeLossless => None,
            SafetyPolicy::Enhanced {
                confirmation_revision,
            }
            | SafetyPolicy::Lossy {
                confirmation_revision,
            } => Some(confirmation_revision),
        }
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        match self {
            SafetyPolicy::SafeLossless => Ok(()),
            SafetyPolicy::Enhanced {
                confirmation_revision,
            }
            | SafetyPolicy::Lossy {
                confirmation_revision,
            } => {
                if confirmation_revision.trim().is_empty() {
                    Err(ContractError::MissingConfirmationRevision)
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Four independent accounting dimensions: compression,
/// command-output, provider-cache and relay. They must never be summed
/// naively — the only aggregation is `total_if_all_known`, which returns
/// `None` until every dimension is known (unknown is never coerced to 0).
/// Unknown fields serialize as explicit `null`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AccountingBuckets {
    pub compression: Option<i64>,
    pub command_output: Option<i64>,
    pub provider_cache: Option<i64>,
    pub relay: Option<i64>,
}

impl AccountingBuckets {
    /// Aggregated total only when every dimension is known; `None` otherwise.
    /// This is the single sanctioned aggregation and is never used for
    /// claimable savings — claimable requires paired Provider evidence.
    pub fn total_if_all_known(&self) -> Option<i64> {
        match (
            self.compression,
            self.command_output,
            self.provider_cache,
            self.relay,
        ) {
            (Some(compression), Some(command_output), Some(provider_cache), Some(relay)) => {
                Some(compression + command_output + provider_cache + relay)
            }
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct ManifestDigestInput<'a> {
    schema_version: u32,
    bound_request: &'a BoundRequest,
    plan: &'a NodePlan,
    stages: &'a [StageArtifact],
    policy: &'a SafetyPolicy,
    policy_confirmation_revision: Option<&'a str>,
    buckets: &'a AccountingBuckets,
}

/// Complete optimization manifest binding request, plan, stage artifacts,
/// safety policy evidence and accounting buckets. `manifest_digest` covers
/// every other field (self-excluding) and can be re-verified with
/// [`OptimizationManifest::verify_manifest_digest`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptimizationManifest {
    pub schema_version: u32,
    pub bound_request: BoundRequest,
    pub plan: NodePlan,
    pub stages: Vec<StageArtifact>,
    pub policy: SafetyPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_confirmation_revision: Option<String>,
    pub buckets: AccountingBuckets,
    pub manifest_digest: String,
}

impl OptimizationManifest {
    pub fn new(
        bound_request: BoundRequest,
        plan: NodePlan,
        stages: Vec<StageArtifact>,
        policy: SafetyPolicy,
        buckets: AccountingBuckets,
    ) -> Result<Self, ContractError> {
        policy.validate()?;
        let policy_confirmation_revision = policy.confirmation_revision().map(ToOwned::to_owned);
        let digest_input = ManifestDigestInput {
            schema_version: CANONICAL_SCHEMA_VERSION,
            bound_request: &bound_request,
            plan: &plan,
            stages: &stages,
            policy: &policy,
            policy_confirmation_revision: policy_confirmation_revision.as_deref(),
            buckets: &buckets,
        };
        let manifest_digest = canonical_sha256(&digest_input);
        Ok(Self {
            schema_version: CANONICAL_SCHEMA_VERSION,
            bound_request,
            plan,
            stages,
            policy,
            policy_confirmation_revision,
            buckets,
            manifest_digest,
        })
    }

    /// Recomputed self-excluding digest equals the stored one.
    pub fn verify_manifest_digest(&self) -> bool {
        let digest_input = ManifestDigestInput {
            schema_version: self.schema_version,
            bound_request: &self.bound_request,
            plan: &self.plan,
            stages: &self.stages,
            policy: &self.policy,
            policy_confirmation_revision: self.policy_confirmation_revision.as_deref(),
            buckets: &self.buckets,
        };
        canonical_sha256(&digest_input) == self.manifest_digest
    }
}
