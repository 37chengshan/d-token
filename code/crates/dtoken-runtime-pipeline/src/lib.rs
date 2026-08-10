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
//! - [`RecoveryReceipt`] is minted only from a store-issued opaque
//!   [`RecoveryProof`] bound to the request identity, the exact source
//!   digest and the active route decision; the old caller-supplied `ccr:`
//!   prefix sentinel forgery surface is gone (PIPELINE-RECEIPT-AUTHENTICITY).
//! - [`SendCommitReceipt`] is minted only by [`ReadyToSend::mint_send_commit`],
//!   which consumes the sendable unit: a transport can produce at most one
//!   terminal receipt per request, and it must consume the sealed unit to do
//!   so. The receipt carries the request identity, the exact sent-body
//!   digest, and the send outcome.

use dtoken_types::{RequestScope, RouteDecision};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

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
    /// The route decision is not the active decision for this request
    /// (request/route/provider identity mismatch); the candidate binding
    /// fails closed so a stale or foreign decision can never be paired
    /// with a receipt (PIPELINE-RECEIPT-BINDING).
    RouteDecisionMismatch,
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

/// Opaque store-issued recovery proof (PIPELINE-RECEIPT-AUTHENTICITY).
///
/// The only way to obtain a proof is [`RecoveryProofAuthority::issue`],
/// which only the production CCR store exercises with references it
/// actually stored.  Fields are private and the proof carries an
/// authority-keyed tag, so a caller can never inject an invented `ccr:`
/// string (or any other free-form bytes) into the public pipeline surface:
/// the old `impl Into<String>` forgery entry is gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryProof {
    recovery_ref: String,
    expires_at_unix: u64,
    tag: [u8; 32],
}

impl RecoveryProof {
    /// The durable store reference this proof was issued for.
    pub fn recovery_ref(&self) -> &str {
        &self.recovery_ref
    }

    /// Unix-seconds expiry of the underlying store entry.
    pub fn expires_at_unix(&self) -> u64 {
        self.expires_at_unix
    }
}

/// Authority that issues and verifies store recovery proofs.
///
/// Constructed only from the CCR store's private key material, so code
/// that does not own the store (adapters, transports, MCP surfaces) never
/// sees it.  A proof issued under any other key fails
/// [`RecoveryProofAuthority::verify`] and is rejected at candidate
/// binding, which also prevents cross-store replay of a proof issued by a
/// different store instance.
#[derive(Debug, Clone)]
pub struct RecoveryProofAuthority {
    key: [u8; 32],
}

impl RecoveryProofAuthority {
    /// Derive the proof authority from the store's private key material.
    ///
    /// Only the trusted runtime that owns the CCR store may call this;
    /// every public pipeline entry that consumes proofs verifies them
    /// against the authority the caller supplies, and a mismatched
    /// authority fails closed.
    pub fn new(store_key: [u8; 32]) -> Self {
        Self { key: store_key }
    }

    /// Issue a proof for one durable store reference.
    ///
    /// The reference must be durable (`ccr:` prefix) and carry a non-zero
    /// expiry; free-form strings and bare prefixes are rejected here, at
    /// the single issuance point.
    pub fn issue(
        &self,
        recovery_ref: impl Into<String>,
        expires_at_unix: u64,
    ) -> Result<RecoveryProof, PipelineError> {
        let recovery_ref = recovery_ref.into();
        if !is_durable_recovery_ref(&recovery_ref) || expires_at_unix == 0 {
            return Err(PipelineError::InvalidRecoveryReceipt);
        }
        Ok(RecoveryProof {
            tag: proof_tag(&self.key, &recovery_ref, expires_at_unix),
            recovery_ref,
            expires_at_unix,
        })
    }

    /// True when the proof was issued by an authority holding this exact
    /// key (i.e. the same store instance).
    pub fn verify(&self, proof: &RecoveryProof) -> bool {
        proof_tag(&self.key, &proof.recovery_ref, proof.expires_at_unix) == proof.tag
    }
}

/// Sealed recovery receipt (T-C0B / RUNTIME-8 / CONTEXT-2).
///
/// Minted only from a store-issued [`RecoveryProof`] by
/// [`PreparedRequest::with_durable_candidate`] (via
/// [`RecoveryReceipt::mint`], which is pub but accepts no free-form bytes:
/// every field is either a validated digest, the request identity, the
/// active route decision digest, or a store-issued opaque proof). A receipt
/// binds the request, the exact source digest and the route decision digest
/// so a foreign, forged or stale recovery handle can never be substituted.
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
    /// public [`PreparedRequest::with_durable_candidate`] flow.  The proof
    /// must verify under the supplied authority (store-issued), the decision
    /// must be the active decision for this request, and the proof must not
    /// be expired against the pipeline's own trusted clock — every check
    /// fails closed (PIPELINE-RECEIPT-AUTHENTICITY / BINDING).
    pub(crate) fn mint(
        request: &PreparedRequest,
        decision: &RouteDecision,
        proof: &RecoveryProof,
        authority: &RecoveryProofAuthority,
    ) -> Result<Self, PipelineError> {
        if !authority.verify(proof) {
            return Err(PipelineError::InvalidRecoveryReceipt);
        }
        if !is_durable_recovery_ref(&proof.recovery_ref) || proof.expires_at_unix == 0 {
            return Err(PipelineError::InvalidRecoveryReceipt);
        }
        if trusted_now_unix() >= proof.expires_at_unix {
            return Err(PipelineError::InvalidRecoveryReceipt);
        }
        if !decision_is_active_for(decision, request) {
            return Err(PipelineError::RouteDecisionMismatch);
        }
        Ok(Self {
            request_id: request.scope.request_id,
            source_digest: request.baseline_digest.clone(),
            decision_digest: decision.stable_digest(),
            recovery_ref: proof.recovery_ref.clone(),
            expires_at_unix: proof.expires_at_unix,
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
    /// request identity, source digest and active route decision (the
    /// stable digest is compared here, so a stale or mutated decision can
    /// never be paired with a receipt), and only before expiry.
    pub fn is_valid_for(
        &self,
        request: &PreparedRequest,
        decision: &RouteDecision,
        now_unix: u64,
    ) -> bool {
        self.request_id == request.scope.request_id
            && self.source_digest == request.baseline_digest
            && self.decision_digest == decision.stable_digest()
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

    /// Bind a recovery receipt to this prepared request and artifact.
    ///
    /// The receipt must match the exact request identity, source digest and
    /// the **current** active route decision: the stable digest of the
    /// supplied `decision` is compared against the receipt's bound digest,
    /// so a stale, mutated or foreign decision can never be paired with a
    /// candidate (PIPELINE-RECEIPT-BINDING).  Every mismatch fails closed.
    pub fn with_candidate(
        self,
        artifact: TransformArtifact,
        decision: &RouteDecision,
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
        if recovery.decision_digest != decision.stable_digest() {
            return Err(PipelineError::RouteDecisionMismatch);
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

    /// Public production flow that binds a store-issued recovery proof and
    /// candidate in one step (T-C0B / RUNTIME-8).
    ///
    /// The recovery receipt is minted inside the pipeline — never by the
    /// caller — and bound to this exact request identity and source digest.
    /// `decision` must be the active [`dtoken_types::RouteDecision`] for
    /// this request (identity is checked; the stable digest is computed
    /// inside the pipeline, never accepted from the caller); `proof` must be
    /// issued by the store authority; and expiry is checked against the
    /// pipeline's own trusted clock.  Every violation fails closed.
    pub fn with_durable_candidate(
        self,
        artifact: TransformArtifact,
        decision: &RouteDecision,
        proof: RecoveryProof,
        authority: &RecoveryProofAuthority,
    ) -> Result<CandidateRequest, PipelineError> {
        let recovery = RecoveryReceipt::mint(&self, decision, &proof, authority)?;
        self.with_candidate(artifact, decision, Some(recovery))
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

/// Authority-keyed tag over the proof binding.  Only an authority holding
/// the same store key can issue a proof that verifies, so invented refs and
/// cross-store replays fail closed at candidate binding.
fn proof_tag(key: &[u8; 32], recovery_ref: &str, expires_at_unix: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(b"dtoken-ccr-recovery-proof-v1\0");
    hasher.update(recovery_ref.as_bytes());
    hasher.update(expires_at_unix.to_le_bytes());
    hasher.finalize().into()
}

/// True when the decision is the active route decision for the request:
/// the identity (request/route/provider) must match the prepared scope, and
/// the stable digest is derived from those canonical fields inside the
/// pipeline.  A stale or foreign decision can never be paired with a
/// receipt (PIPELINE-RECEIPT-BINDING).
fn decision_is_active_for(decision: &RouteDecision, request: &PreparedRequest) -> bool {
    decision.request_id == request.scope.request_id
        && decision.route_id == request.scope.route_id
        && decision.provider_id == request.scope.provider_id
}

/// Trusted pipeline clock (unix seconds), read inside the pipeline and never
/// supplied by the caller, so an adapter cannot defeat the expiry gate by
/// claiming an old time (RUNTIME-8).
fn trusted_now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
