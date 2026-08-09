//! Transport-independent request lifecycle. Production state constructors stay private so an
//! optimized payload cannot become sendable without validation and durable recovery evidence.

use dtoken_types::RequestScope;
use sha2::{Digest, Sha256};

pub mod receipts;

pub use dtoken_types::{SentPayloadVariant, UpstreamSendState as SendCommitState};
pub use receipts::{RecoveryReceipt, SendCommitReceipt, ValidationReceipt};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryPrepareReceipt {
    recovery_ref: String,
    expires_at_unix: u64,
}

impl RecoveryPrepareReceipt {
    pub fn new(
        recovery_ref: impl Into<String>,
        expires_at_unix: u64,
    ) -> Result<Self, PipelineError> {
        let recovery_ref = recovery_ref.into();
        if !recovery_ref.starts_with("ccr:") || expires_at_unix == 0 {
            return Err(PipelineError::InvalidRecoveryReceipt);
        }
        Ok(Self {
            recovery_ref,
            expires_at_unix,
        })
    }

    pub fn recovery_ref(&self) -> &str {
        &self.recovery_ref
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

    pub fn with_candidate(
        self,
        artifact: TransformArtifact,
        recovery: Option<RecoveryPrepareReceipt>,
    ) -> Result<CandidateRequest, PipelineError> {
        if artifact.source_digest != self.baseline_digest {
            return Err(PipelineError::SourceDigestMismatch);
        }
        let recovery = recovery.ok_or(PipelineError::MissingRecoveryReceipt)?;
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
}

#[derive(Debug)]
pub struct CandidateRequest {
    scope: RequestScope,
    baseline: Vec<u8>,
    artifact: TransformArtifact,
    recovery: RecoveryPrepareReceipt,
}

impl CandidateRequest {
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
}

#[derive(Debug)]
pub struct ReadyToSend {
    scope: RequestScope,
    body: Vec<u8>,
    variant: SentPayloadVariant,
    reason_code: String,
    recovery: Option<RecoveryPrepareReceipt>,
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

    pub fn mark_committed(self, committed_at_ms: u64) -> SendReceipt {
        SendReceipt {
            request_id: self.scope.request_id,
            state: if committed_at_ms == 0 {
                SendCommitState::FailedBeforeCommit
            } else {
                SendCommitState::SendCommitted
            },
            committed_at_ms: (committed_at_ms != 0).then_some(committed_at_ms),
            variant: self.variant,
            recovery: self.recovery,
            transforms: self.transforms,
        }
    }
}

#[derive(Debug)]
pub struct SendReceipt {
    request_id: uuid::Uuid,
    state: SendCommitState,
    committed_at_ms: Option<u64>,
    variant: SentPayloadVariant,
    recovery: Option<RecoveryPrepareReceipt>,
    transforms: Vec<String>,
}

impl SendReceipt {
    pub fn state(&self) -> SendCommitState {
        self.state
    }

    pub fn request_id(&self) -> uuid::Uuid {
        self.request_id
    }

    pub fn committed_at_ms(&self) -> Option<u64> {
        self.committed_at_ms
    }

    pub fn variant(&self) -> SentPayloadVariant {
        self.variant
    }

    pub fn recovery(&self) -> Option<&RecoveryPrepareReceipt> {
        self.recovery.as_ref()
    }

    pub fn transforms(&self) -> &[String] {
        &self.transforms
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
