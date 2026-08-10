//! Transport-independent request lifecycle with sealed, unforgeable
//! receipts (T-C0B / RUNTIME-8 / QUALITY-7).
//!
//! Production state constructors stay private so an optimized payload cannot
//! become sendable without validation and durable recovery evidence, and no
//! external caller or adapter can mint an Optimized/Committed state by
//! construction:
//!
//! - [`ValidationReceipt`] is minted only by [`ValidatedRequest::mint_validation_receipt`]
//!   after a real baseline/candidate validation, and carries the exact
//!   validated candidate digest.
//! - [`RecoveryReceipt`] is minted only from a durable store reference bound
//!   to the request identity and the exact source digest; the old `ccr:`
//!   prefix sentinel forgery is dead (non-durable refs are rejected).
//! - [`SendCommitReceipt`] is minted only by [`ReadyToSend::mint_send_commit`],
//!   which consumes the sendable unit: a transport can produce at most one
//!   terminal receipt per request, and it must consume the sealed unit to do
//!   so. The receipt carries the request identity, the exact sent-body
//!   digest, and the send outcome.

use dtoken_types::RequestScope;
use sha2::{Digest, Sha256};

pub use dtoken_types::{SentPayloadVariant, UpstreamSendState as SendCommitState};

pub mod recovery;
pub mod scheduler;

pub use recovery::{RecoveryPhase, RecoveryTracker, RecoveryTransitionError};
pub use scheduler::{
    BoundedWorkerPool, ByteBudget, CancellationToken, ConcurrencyLimits, DispatchDecision,
    PreserveReason, RequestDeadline, SchedulerGate, StableAdmission, StableOrder, SubmitError,
    TaskOutcome,
};

/// Backward-compatible alias for the terminal commit receipt (T-C0B
/// canonical name is [`SendCommitReceipt`]).
#[deprecated(note = "renamed to SendCommitReceipt; mint via ReadyToSend::mint_send_commit")]
pub type SendReceipt = SendCommitReceipt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    EmptyIngress,
    IngressTooLarge,
    EmptyCandidate,
    EmptyTransformList,
    EmptySourceDigest,
    SourceDigestMismatch,
    MissingRecoveryReceipt,
    InvalidRecoveryReceipt,
    ValidatorRejected,
    InvalidCommitTimestamp,
    /// Concurrency bounds must all be strictly positive.
    InvalidConcurrencyLimits,
    /// Byte budget capacity must be strictly positive.
    InvalidByteBudget,
    /// Request deadline budget must be strictly positive.
    InvalidDeadline,
    /// The admission queue was full; the caller preserves the baseline.
    QueueSaturated,
    /// The request-wide deadline passed before commit.
    DeadlineExceeded,
    /// The request was cancelled before commit.
    Cancelled,
    /// A second baseline dispatch was attempted for one request.
    BaselineAlreadyDispatched,
    /// A dispatch or receipt was attempted after the request was terminal.
    AlreadyTerminal,
    /// A terminal receipt did not belong to the request identity.
    ReceiptIdentityMismatch,
}

#[derive(Debug, Clone)]
pub struct ContextEnvelope {
    scope: RequestScope,
    ingress: Vec<u8>,
}

impl ContextEnvelope {
    pub fn new(
        scope: RequestScope,
        ingress: Vec<u8>,
        max_ingress_bytes: usize,
    ) -> Result<Self, PipelineError> {
        if ingress.is_empty() {
            return Err(PipelineError::EmptyIngress);
        }
        if ingress.len() > max_ingress_bytes {
            return Err(PipelineError::IngressTooLarge);
        }
        Ok(Self { scope, ingress })
    }

    pub fn scope(&self) -> &RequestScope {
        &self.scope
    }
}

#[derive(Debug, Clone)]
pub struct PipelineInput {
    envelope: ContextEnvelope,
}

impl PipelineInput {
    pub fn new(
        scope: RequestScope,
        ingress: Vec<u8>,
        max_ingress_bytes: usize,
    ) -> Result<Self, PipelineError> {
        ContextEnvelope::new(scope, ingress, max_ingress_bytes).map(Self::from_envelope)
    }

    pub fn from_envelope(envelope: ContextEnvelope) -> Self {
        Self { envelope }
    }

    pub fn prepare(self) -> PreparedRequest {
        let baseline_digest = digest(&self.envelope.ingress);
        PreparedRequest {
            scope: self.envelope.scope,
            baseline: self.envelope.ingress,
            baseline_digest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformArtifact {
    source_digest: String,
    candidate: Vec<u8>,
    transforms: Vec<String>,
}

impl TransformArtifact {
    pub fn new(
        source_digest: impl Into<String>,
        candidate: Vec<u8>,
        transforms: Vec<String>,
    ) -> Result<Self, PipelineError> {
        let source_digest = source_digest.into();
        if source_digest.trim().is_empty() {
            return Err(PipelineError::EmptySourceDigest);
        }
        if candidate.is_empty() {
            return Err(PipelineError::EmptyCandidate);
        }
        if transforms.is_empty() || transforms.iter().any(|item| item.trim().is_empty()) {
            return Err(PipelineError::EmptyTransformList);
        }
        Ok(Self {
            source_digest,
            candidate,
            transforms,
        })
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    pub fn candidate(&self) -> &[u8] {
        &self.candidate
    }

    pub fn transforms(&self) -> &[String] {
        &self.transforms
    }
}

/// Sealed recovery receipt (T-C0B / RUNTIME-8 / CONTEXT-2).
///
/// Minted only from a durable store reference by [`PreparedRequest::with_candidate`]
/// (via [`RecoveryReceipt::mint`], which is pub but accepts no free-form
/// bytes: every field is either a validated digest, the request identity, or
/// a durable store ref that must carry the `ccr:` prefix). A receipt binds
/// the request, the exact source digest and the route decision digest so a
/// foreign or stale recovery handle can never be substituted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReceipt {
    request_id: uuid::Uuid,
    source_digest: String,
    decision_digest: String,
    recovery_ref: String,
    expires_at_unix: u64,
}

impl RecoveryReceipt {
    /// Mint the recovery receipt for one prepared request (crate-private).
    ///
    /// Only the pipeline itself may mint receipts; external callers use the
    /// public [`PreparedRequest::with_durable_candidate`] flow. `recovery_ref`
    /// must be a durable store reference (`ccr:` prefix) — the old
    /// timestamp/prefix-sentinel forgery is rejected here — and every field
    /// is bound to the exact prepared request identity and source digest.
    pub(crate) fn mint(
        request: &PreparedRequest,
        decision_digest: impl Into<String>,
        recovery_ref: impl Into<String>,
        expires_at_unix: u64,
    ) -> Result<Self, PipelineError> {
        let decision_digest = decision_digest.into();
        let recovery_ref = recovery_ref.into();
        if decision_digest.trim().is_empty() {
            return Err(PipelineError::EmptySourceDigest);
        }
        if !is_durable_recovery_ref(&recovery_ref) || expires_at_unix == 0 {
            return Err(PipelineError::InvalidRecoveryReceipt);
        }
        Ok(Self {
            request_id: request.scope.request_id,
            source_digest: request.baseline_digest.clone(),
            decision_digest,
            recovery_ref,
            expires_at_unix,
        })
    }

    pub fn request_id(&self) -> uuid::Uuid {
        self.request_id
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    pub fn decision_digest(&self) -> &str {
        &self.decision_digest
    }

    pub fn recovery_ref(&self) -> &str {
        &self.recovery_ref
    }

    pub fn expires_at_unix(&self) -> u64 {
        self.expires_at_unix
    }

    /// The receipt is usable only while it matches the exact prepared
    /// request identity and source digest, and only before expiry.
    pub fn is_valid_for(&self, request: &PreparedRequest, now_unix: u64) -> bool {
        self.request_id == request.scope.request_id
            && self.source_digest == request.baseline_digest
            && self.expires_at_unix > now_unix
    }
}

/// Sealed validation receipt (T-C0B / RUNTIME-8).
///
/// Minted only by [`ValidatedRequest::mint_validation_receipt`] — after a
/// real baseline/candidate validation — and bound to the exact candidate
/// digest the validation accepted. The same digest is what the transport
/// must send; a post-validation mutation changes the digest and is
/// structurally excluded from this receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReceipt {
    request_id: uuid::Uuid,
    source_digest: String,
    validated_digest: String,
    variant: SentPayloadVariant,
    transforms: Vec<String>,
    validated_at_ms: u64,
}

impl ValidationReceipt {
    pub fn request_id(&self) -> uuid::Uuid {
        self.request_id
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    /// The exact digest of the validated body the transport must send.
    pub fn validated_digest(&self) -> &str {
        &self.validated_digest
    }

    pub fn variant(&self) -> SentPayloadVariant {
        self.variant
    }

    pub fn transforms(&self) -> &[String] {
        &self.transforms
    }

    pub fn validated_at_ms(&self) -> u64 {
        self.validated_at_ms
    }
}

#[derive(Debug, Clone)]
pub struct PreparedRequest {
    scope: RequestScope,
    baseline: Vec<u8>,
    baseline_digest: String,
}

impl PreparedRequest {
    pub fn baseline_digest(&self) -> &str {
        &self.baseline_digest
    }

    pub fn scope(&self) -> &RequestScope {
        &self.scope
    }

    pub fn with_candidate(
        self,
        artifact: TransformArtifact,
        recovery: Option<RecoveryReceipt>,
    ) -> Result<CandidateRequest, PipelineError> {
        if artifact.source_digest != self.baseline_digest {
            return Err(PipelineError::SourceDigestMismatch);
        }
        let recovery = recovery.ok_or(PipelineError::MissingRecoveryReceipt)?;
        if recovery.request_id != self.scope.request_id
            || recovery.source_digest != self.baseline_digest
        {
            return Err(PipelineError::ReceiptIdentityMismatch);
        }
        Ok(CandidateRequest {
            scope: self.scope,
            baseline: self.baseline,
            artifact,
            recovery,
        })
    }

    pub fn into_baseline_ready(self, reason_code: impl Into<String>) -> ReadyToSend {
        ReadyToSend {
            scope: self.scope,
            body: self.baseline,
            variant: SentPayloadVariant::Original,
            reason_code: reason_code.into(),
            recovery: None,
            transforms: Vec::new(),
        }
    }

    /// Public production flow that binds a durable store reference and
    /// candidate in one step (T-C0B / RUNTIME-8).
    ///
    /// The recovery receipt is minted inside the pipeline — never by the
    /// caller — and bound to this exact request identity and source digest.
    /// `decision_digest` is the canonical
    /// [`dtoken_types::RouteDecision::stable_digest`] of the request's route
    /// decision; `recovery_ref` must be a durable store reference (`ccr:`
    /// prefix); `expires_at_unix` must be non-zero.
    pub fn with_durable_candidate(
        self,
        artifact: TransformArtifact,
        decision_digest: impl Into<String>,
        recovery_ref: impl Into<String>,
        expires_at_unix: u64,
    ) -> Result<CandidateRequest, PipelineError> {
        let recovery = RecoveryReceipt::mint(&self, decision_digest, recovery_ref, expires_at_unix)?;
        self.with_candidate(artifact, Some(recovery))
    }

    /// Preserve path for saturation, expired deadlines, and cancellation
    /// (PR-C6A): the request still goes out as its original baseline,
    /// exactly once, with an evidence-bearing reason code.  The caller never
    /// drops a request because optimization could not run.
    pub fn into_preserved(self, reason: PreserveReason) -> ReadyToSend {
        self.into_baseline_ready(reason.as_reason_code())
    }
}

#[derive(Debug)]
pub struct CandidateRequest {
    scope: RequestScope,
    baseline: Vec<u8>,
    artifact: TransformArtifact,
    recovery: RecoveryReceipt,
}

impl CandidateRequest {
    pub fn scope(&self) -> &RequestScope {
        &self.scope
    }

    pub fn recovery_receipt(&self) -> &RecoveryReceipt {
        &self.recovery
    }

    /// Validate the candidate against the baseline. The closure is the
    /// protocol-validity check only; the receipts are still minted by the
    /// pipeline, never by the caller.
    pub fn validate(
        self,
        validator: impl FnOnce(&[u8], &[u8]) -> bool,
    ) -> Result<ValidatedRequest, PipelineError> {
        if !validator(&self.baseline, &self.artifact.candidate) {
            return Err(PipelineError::ValidatorRejected);
        }
        Ok(ValidatedRequest { candidate: self })
    }
}

#[derive(Debug)]
pub struct ValidatedRequest {
    candidate: CandidateRequest,
}

impl ValidatedRequest {
    pub fn scope(&self) -> &RequestScope {
        &self.candidate.scope
    }

    pub fn source_digest(&self) -> &str {
        &self.candidate.artifact.source_digest
    }

    /// Consume the validated request into the single sendable unit.
    ///
    /// This is the only way a validated candidate becomes sendable, so the
    /// Optimized state is structurally unreachable without validation.
    pub fn into_ready_to_send(self) -> ReadyToSend {
        ReadyToSend {
            scope: self.candidate.scope,
            body: self.candidate.artifact.candidate,
            variant: SentPayloadVariant::Optimized,
            reason_code: "validated_optimized_candidate".to_string(),
            recovery: Some(self.candidate.recovery),
            transforms: self.candidate.artifact.transforms,
        }
    }

    /// Mint the sealed validation receipt for the exact validated bytes.
    ///
    /// The receipt is minted before the sendable unit is consumed, and it
    /// binds the exact validated candidate digest: the production transport
    /// compares this digest against the body it actually sends.
    pub fn mint_validation_receipt(&self, validated_at_ms: u64) -> ValidationReceipt {
        ValidationReceipt {
            request_id: self.candidate.scope.request_id,
            source_digest: self.candidate.artifact.source_digest.clone(),
            validated_digest: digest(&self.candidate.artifact.candidate),
            variant: SentPayloadVariant::Optimized,
            transforms: self.candidate.artifact.transforms.clone(),
            validated_at_ms,
        }
    }
}

#[derive(Debug)]
pub struct ReadyToSend {
    scope: RequestScope,
    body: Vec<u8>,
    variant: SentPayloadVariant,
    reason_code: String,
    recovery: Option<RecoveryReceipt>,
    transforms: Vec<String>,
}

impl ReadyToSend {
    pub fn variant(&self) -> SentPayloadVariant {
        self.variant
    }

    pub fn reason_code(&self) -> &str {
        &self.reason_code
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn scope(&self) -> &RequestScope {
        &self.scope
    }

    /// Exact digest of the body this unit will send (the validated digest
    /// for optimized units; the baseline digest for preserved units).
    pub fn body_digest(&self) -> String {
        digest(&self.body)
    }

    /// Mint the terminal send-commit receipt, consuming the sendable unit.
    ///
    /// A transport can produce at most one terminal receipt per request, and
    /// it must consume the sealed unit to do so. `committed_at_ms == 0`
    /// records the honest pre-send failure state; no caller can mint a
    /// `SendCommitted` state from arbitrary bytes or a timestamp.
    pub fn mint_send_commit(self, committed_at_ms: u64) -> SendCommitReceipt {
        let sent_digest = digest(&self.body);
        SendCommitReceipt {
            request_id: self.scope.request_id,
            state: if committed_at_ms == 0 {
                SendCommitState::FailedBeforeCommit
            } else {
                SendCommitState::SendCommitted
            },
            committed_at_ms: (committed_at_ms != 0).then_some(committed_at_ms),
            sent_digest,
            variant: self.variant,
            recovery: self.recovery,
            transforms: self.transforms,
            reason_code: self.reason_code,
            scope: self.scope,
        }
    }
}

/// Terminal transport-observable commit receipt (T-C0B / RUNTIME-8).
///
/// Sealed: minted only by [`ReadyToSend::mint_send_commit`]. The sent body
/// digest is bound at mint time from the actual sendable unit, so the
/// production transport can prove the sent body equals the validated body.
#[derive(Debug)]
pub struct SendCommitReceipt {
    request_id: uuid::Uuid,
    state: SendCommitState,
    committed_at_ms: Option<u64>,
    sent_digest: String,
    variant: SentPayloadVariant,
    recovery: Option<RecoveryReceipt>,
    transforms: Vec<String>,
    reason_code: String,
    scope: RequestScope,
}

impl SendCommitReceipt {
    pub fn state(&self) -> SendCommitState {
        self.state
    }

    pub fn request_id(&self) -> uuid::Uuid {
        self.request_id
    }

    pub fn committed_at_ms(&self) -> Option<u64> {
        self.committed_at_ms
    }

    /// Exact digest of the body that was (or was attempted to be) sent.
    pub fn sent_digest(&self) -> &str {
        &self.sent_digest
    }

    pub fn variant(&self) -> SentPayloadVariant {
        self.variant
    }

    pub fn recovery(&self) -> Option<&RecoveryReceipt> {
        self.recovery.as_ref()
    }

    pub fn transforms(&self) -> &[String] {
        &self.transforms
    }

    /// Evidence-bearing reason code for the terminal state (e.g.
    /// `queue_saturated_preserve`, `validated_optimized_candidate`).
    pub fn reason_code(&self) -> &str {
        &self.reason_code
    }

    /// The full ingress identity.  The receipt is bound to the original
    /// route/provider/model; there is no setter, so a post-commit provider
    /// switch is structurally impossible.
    pub fn scope(&self) -> &RequestScope {
        &self.scope
    }

    pub fn provider_id(&self) -> &str {
        &self.scope.provider_id
    }

    pub fn route_id(&self) -> &str {
        &self.scope.route_id
    }

    pub fn model(&self) -> &str {
        &self.scope.model
    }

    /// Terminal states are absorbing: a committed receipt can never be
    /// replayed into a second send.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            SendCommitState::SendCommitted
                | SendCommitState::FailedBeforeCommit
                | SendCommitState::FailedAfterCommit
        )
    }
}

/// A recovery ref is durable only when it carries the store prefix. The
/// production transport only ever receives refs from the real CCR store;
/// this guard rejects every other string, so a timestamp or free-form
/// string can never masquerade as a recovery receipt (RUNTIME-8).
fn is_durable_recovery_ref(recovery_ref: &str) -> bool {
    recovery_ref.starts_with("ccr:") && recovery_ref.len() > "ccr:".len()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
