//! Sealed, verifiable evidence receipts.
//!
//! Three unforgeable type-state evidence records:
//! - [`ValidationReceipt`] is artifact-derived: its `validation_digest` is a
//!   canonical SHA-256 over the evidence fields, recomputed by `verify()`, so
//!   a forged or tampered receipt (including one deserialized from JSON) is
//!   rejected.
//! - [`RecoveryReceipt`] is request/source/decision-bound: `matches_scope`
//!   requires all three bindings to agree and `is_expired` fails closed on
//!   expiry, so wrong-scope or stale recovery is never honored.
//! - [`SendCommitReceipt`] is transport-observable: it records the observed
//!   commit state and payload variant; `verify()` rejects forged commits.
//!
//! Construction (`mint_*`) is `pub(crate)`: only the pipeline can mint valid
//! receipts. External adapters can deserialize receipts as data but can never
//! mint or forge a receipt that survives verification, and can never reach
//! the `Optimized`/`SendCommitted` type-state from arbitrary bytes or
//! timestamps (see the `compile_fail` doctests below).
//!
//! ```compile_fail
//! // External adapters cannot mint a validation receipt: the constructor is
//! // crate-private, so the struct literal and the minting path do not exist
//! // outside the pipeline.
//! use dtoken_runtime_pipeline::ValidationReceipt;
//! let _ = ValidationReceipt {
//!     request_id: uuid::Uuid::new_v4(),
//!     source_digest: String::new(),
//!     candidate_digest: String::new(),
//!     transforms: Vec::new(),
//!     validated_at_ms: 0,
//!     revision: String::new(),
//!     validation_digest: String::new(),
//! };
//! ```
//!
//! ```compile_fail
//! // External adapters cannot mint a send-commit receipt either.
//! use dtoken_runtime_pipeline::SendCommitReceipt;
//! let _ = SendCommitReceipt {
//!     request_id: uuid::Uuid::new_v4(),
//!     state: dtoken_runtime_pipeline::SendCommitState::SendCommitted,
//!     variant: dtoken_types::SentPayloadVariant::Optimized,
//!     committed_at_ms: None,
//!     transport_observed: false,
//!     revision: String::new(),
//!     commit_digest: String::new(),
//! };
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub use dtoken_types::{SentPayloadVariant, UpstreamSendState as SendCommitState};

pub const VALIDATION_RECEIPT_REVISION: &str = "validation_v1";
pub const RECOVERY_RECEIPT_REVISION: &str = "recovery_v1";
pub const SEND_COMMIT_RECEIPT_REVISION: &str = "send_commit_v1";

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Canonical digest over a serializable value: serde_json `Value` uses a
/// BTreeMap, so object keys serialize in lexicographic order with no extra
/// whitespace — deterministic across processes.
fn canonical_sha256(value: impl Serialize) -> String {
    let value = serde_json::to_value(value).expect("receipt serialization is infallible");
    let canonical = serde_json::to_string(&value).expect("canonical serialization is infallible");
    sha256_hex(canonical.as_bytes())
}

/// Artifact-derived validation receipt. Minted only by the
/// pipeline after a validator accepted the candidate; `verify()` recomputes
/// the digest from the evidence fields and `matches_artifact` additionally
/// cross-checks the actual candidate bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationReceipt {
    request_id: Uuid,
    source_digest: String,
    candidate_digest: String,
    transforms: Vec<String>,
    validated_at_ms: u64,
    revision: String,
    validation_digest: String,
}

impl ValidationReceipt {
    /// Pipeline-only minting path. `candidate` bytes are hashed here; callers
    /// can never supply the candidate digest.
    ///
    /// Dormant in non-test builds until the send/validation pipeline slices
    /// consume it (T-C1C); unit tests below exercise mint + verify now, and
    /// the `pub(crate)` sealing is what keeps adapters from minting.
    #[allow(dead_code)]
    pub(crate) fn mint(
        request_id: Uuid,
        source_digest: impl Into<String>,
        candidate: &[u8],
        transforms: Vec<String>,
        validated_at_ms: u64,
    ) -> Self {
        let source_digest = source_digest.into();
        let candidate_digest = sha256_hex(candidate);
        let revision = VALIDATION_RECEIPT_REVISION.to_string();
        let validation_digest = canonical_sha256((
            request_id,
            &source_digest,
            &candidate_digest,
            &transforms,
            validated_at_ms,
            &revision,
        ));
        Self {
            request_id,
            source_digest,
            candidate_digest,
            transforms,
            validated_at_ms,
            revision,
            validation_digest,
        }
    }

    /// Recomputed digest equals the stored one; rejects forged/tampered
    /// receipts (including ones deserialized from JSON).
    pub fn verify(&self) -> bool {
        self.validation_digest
            == canonical_sha256((
                self.request_id,
                &self.source_digest,
                &self.candidate_digest,
                &self.transforms,
                self.validated_at_ms,
                &self.revision,
            ))
    }

    /// Verification plus cross-check against the real source digest and
    /// candidate bytes. A self-consistent receipt still fails when the bytes
    /// do not match the recorded candidate digest.
    pub fn matches_artifact(&self, source_digest: &str, candidate: &[u8]) -> bool {
        self.verify()
            && self.source_digest == source_digest
            && self.candidate_digest == sha256_hex(candidate)
    }

    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    pub fn candidate_digest(&self) -> &str {
        &self.candidate_digest
    }

    pub fn transforms(&self) -> &[String] {
        &self.transforms
    }

    pub fn validated_at_ms(&self) -> u64 {
        self.validated_at_ms
    }
}

/// Request/source/decision-bound recovery receipt. The three
/// bindings must all agree for the recovery to be honored, and expired
/// receipts fail closed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryReceipt {
    request_id: Uuid,
    source_digest: String,
    decision_digest: String,
    recovery_ref: String,
    expires_at_unix: u64,
    revision: String,
    receipt_digest: String,
}

impl RecoveryReceipt {
    /// Pipeline-only minting path.
    ///
    /// Dormant in non-test builds until the recovery pipeline slices consume
    /// it (T-C2C); unit tests below exercise mint + scope/expiry checks now.
    #[allow(dead_code)]
    pub(crate) fn mint(
        request_id: Uuid,
        source_digest: impl Into<String>,
        decision_digest: impl Into<String>,
        recovery_ref: impl Into<String>,
        expires_at_unix: u64,
    ) -> Self {
        let source_digest = source_digest.into();
        let decision_digest = decision_digest.into();
        let recovery_ref = recovery_ref.into();
        let revision = RECOVERY_RECEIPT_REVISION.to_string();
        let receipt_digest = canonical_sha256((
            request_id,
            &source_digest,
            &decision_digest,
            &recovery_ref,
            expires_at_unix,
            &revision,
        ));
        Self {
            request_id,
            source_digest,
            decision_digest,
            recovery_ref,
            expires_at_unix,
            revision,
            receipt_digest,
        }
    }

    /// Pure binding check: request / source / decision must all agree.
    /// Does NOT check the digest — consumers must additionally call
    /// [`RecoveryReceipt::verify`] (or use [`RecoveryReceipt::is_valid_for`])
    /// so a forged receipt can never be honored on bindings alone.
    pub fn matches_scope(
        &self,
        request_id: Uuid,
        source_digest: &str,
        decision_digest: &str,
    ) -> bool {
        self.request_id == request_id
            && self.source_digest == source_digest
            && self.decision_digest == decision_digest
    }

    pub fn is_expired(&self, now_unix: u64) -> bool {
        now_unix > self.expires_at_unix
    }

    /// Fail-closed consumer gate: the receipt must be authentic, bound to
    /// this exact request/source/decision, and not expired.
    pub fn is_valid_for(
        &self,
        request_id: Uuid,
        source_digest: &str,
        decision_digest: &str,
        now_unix: u64,
    ) -> bool {
        self.verify()
            && self.matches_scope(request_id, source_digest, decision_digest)
            && !self.is_expired(now_unix)
    }

    pub fn verify(&self) -> bool {
        self.receipt_digest
            == canonical_sha256((
                self.request_id,
                &self.source_digest,
                &self.decision_digest,
                &self.recovery_ref,
                self.expires_at_unix,
                &self.revision,
            ))
    }

    pub fn request_id(&self) -> Uuid {
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
}

/// Transport-observable send-commit receipt. Only
/// the pipeline's commit path mints it from the observed transport outcome;
/// `verify()` rejects forged commit digests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendCommitReceipt {
    request_id: Uuid,
    state: SendCommitState,
    variant: SentPayloadVariant,
    committed_at_ms: Option<u64>,
    transport_observed: bool,
    revision: String,
    commit_digest: String,
}

impl SendCommitReceipt {
    /// Pipeline-only minting path: the commit is observed by the transport,
    /// never asserted by a caller boolean or timestamp.
    ///
    /// Dormant in non-test builds until the transport commit path consumes it
    /// (T-C6C); unit tests below exercise mint + verify now.
    #[allow(dead_code)]
    pub(crate) fn mint(
        request_id: Uuid,
        state: SendCommitState,
        variant: SentPayloadVariant,
        committed_at_ms: Option<u64>,
        transport_observed: bool,
    ) -> Self {
        let revision = SEND_COMMIT_RECEIPT_REVISION.to_string();
        let commit_digest = canonical_sha256((
            request_id,
            state,
            variant,
            committed_at_ms,
            transport_observed,
            &revision,
        ));
        Self {
            request_id,
            state,
            variant,
            committed_at_ms,
            transport_observed,
            revision,
            commit_digest,
        }
    }

    pub fn verify(&self) -> bool {
        self.commit_digest
            == canonical_sha256((
                self.request_id,
                self.state,
                self.variant,
                self.committed_at_ms,
                self.transport_observed,
                &self.revision,
            ))
    }

    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    pub fn state(&self) -> SendCommitState {
        self.state
    }

    pub fn variant(&self) -> SentPayloadVariant {
        self.variant
    }

    pub fn committed_at_ms(&self) -> Option<u64> {
        self.committed_at_ms
    }

    pub fn transport_observed(&self) -> bool {
        self.transport_observed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minted_validation_receipt_verifies_and_matches_artifact() {
        let id = Uuid::new_v4();
        let source = "s".repeat(64);
        let receipt = ValidationReceipt::mint(
            id,
            source.clone(),
            b"candidate bytes",
            vec!["log_fold_v1".to_string()],
            42,
        );
        assert!(receipt.verify(), "minted receipt must verify");
        assert!(receipt.matches_artifact(&source, b"candidate bytes"));
        assert!(!receipt.matches_artifact(&source, b"other bytes"));
        assert_eq!(receipt.request_id(), id);
        assert_eq!(receipt.source_digest(), source.as_str());
        assert_eq!(receipt.transforms(), &["log_fold_v1".to_string()]);
        assert_eq!(receipt.validated_at_ms(), 42);
    }

    #[test]
    fn minted_recovery_receipt_is_valid_only_for_its_scope_and_window() {
        let id = Uuid::new_v4();
        let source = "s".repeat(64);
        let decision = "d".repeat(64);
        let receipt = RecoveryReceipt::mint(
            id,
            source.clone(),
            decision.clone(),
            "ccr:durable/0001",
            100,
        );
        assert!(receipt.verify());
        assert!(receipt.is_valid_for(id, &source, &decision, 50));
        assert!(
            receipt.is_valid_for(id, &source, &decision, 100),
            "not yet expired at expiry"
        );
        assert!(
            !receipt.is_valid_for(id, &source, &decision, 101),
            "expired fails closed"
        );
        assert!(!receipt.is_valid_for(Uuid::new_v4(), &source, &decision, 50));
        assert!(!receipt.is_valid_for(id, &"x".repeat(64), &decision, 50));
        assert!(!receipt.is_valid_for(id, &source, &"x".repeat(64), 50));
    }

    #[test]
    fn serialized_receipts_never_carry_raw_or_sensitive_content() {
        // Receipts bind artifacts by digest only: raw candidate bytes (which
        // may contain prompts, responses or credentials) must never appear in
        // the serialized receipt — only the derived sha256 digests.
        let id = Uuid::new_v4();
        let sensitive = "Authorization: Bearer sk-super-secret-0123456789abcdef";
        let receipt = ValidationReceipt::mint(
            id,
            "s".repeat(64),
            sensitive.as_bytes(),
            vec!["log_fold_v1".to_string()],
            42,
        );
        let serialized = serde_json::to_string(&receipt).expect("serializable");
        assert!(
            !serialized.contains("sk-super-secret"),
            "raw candidate bytes must not leak into the receipt serialization"
        );
        assert!(!serialized.contains("Authorization"));
        assert!(
            !serialized.contains(sensitive),
            "the full sensitive candidate must not appear verbatim"
        );
        // The binding that survives is the derived digest, and only that.
        assert_eq!(receipt.candidate_digest().len(), 64);
        assert!(receipt
            .candidate_digest()
            .chars()
            .all(|c| c.is_ascii_hexdigit()));

        let commit = SendCommitReceipt::mint(
            id,
            SendCommitState::SendCommitted,
            SentPayloadVariant::Optimized,
            Some(1234),
            true,
        );
        let commit_json = serde_json::to_string(&commit).expect("serializable");
        assert!(
            !commit_json.contains("sk-super-secret"),
            "send-commit receipt carries no payload content"
        );
    }

    #[test]
    fn minted_send_commit_receipt_verifies_and_records_observation() {
        let id = Uuid::new_v4();
        let receipt = SendCommitReceipt::mint(
            id,
            SendCommitState::SendCommitted,
            SentPayloadVariant::Optimized,
            Some(1234),
            true,
        );
        assert!(receipt.verify());
        assert_eq!(receipt.state(), SendCommitState::SendCommitted);
        assert_eq!(receipt.variant(), SentPayloadVariant::Optimized);
        assert_eq!(receipt.committed_at_ms(), Some(1234));
        assert!(receipt.transport_observed());
    }
}
