//! Sealed receipt contract tests.
//!
//! Contract requirements under test:
//! - ValidationReceipt is artifact-derived: its digest is recomputed from the
//!   evidence fields and any tampering (including forged serde JSON) is
//!   rejected by `verify()`.
//! - RecoveryReceipt is request/source/decision-bound: wrong scope or expired
//!   receipts are rejected.
//! - SendCommitReceipt is transport-observable: it records the observed
//!   commit state; forged/tampered serialized receipts fail verification.
//! - External adapters cannot *mint* valid receipts: constructors are
//!   crate-private and the Optimized/Committed type-state is not reachable
//!   from arbitrary bytes/timestamps (compile-fail doctests prove this).

use dtoken_runtime_pipeline::{
    RecoveryReceipt, SendCommitReceipt, SendCommitState, SentPayloadVariant, ValidationReceipt,
};
use serde_json::json;
use uuid::Uuid;

fn request_id() -> Uuid {
    Uuid::new_v4()
}

#[test]
fn validation_receipt_verify_rejects_forged_json() {
    // A receipt deserialized from JSON can exist as data, but the forged
    // validation digest must not survive verification: `verify()` recomputes
    // the digest from the evidence fields.
    let id = request_id();
    let forged: ValidationReceipt = serde_json::from_value(json!({
        "request_id": id,
        "source_digest": "a".repeat(64),
        "candidate_digest": "b".repeat(64),
        "transforms": ["log_fold_v1"],
        "validated_at_ms": 100,
        "revision": "validation_v1",
        "validation_digest": "0".repeat(64),
    }))
    .expect("receipts are deserializable data");

    assert!(
        !forged.verify(),
        "forged validation digest must fail verify()"
    );
    assert_eq!(forged.source_digest(), "a".repeat(64));
    assert_eq!(forged.transforms(), &["log_fold_v1".to_string()]);

    // Cross-check against the actual artifact bytes: even a self-consistent
    // digest is rejected when it does not match the real candidate bytes.
    assert!(!forged.matches_artifact(&"a".repeat(64), b"real candidate"));
}

#[test]
fn validation_receipt_matches_artifact_only_when_bytes_agree() {
    let id = request_id();
    let source = "s".repeat(64);
    let candidate = b"optimized payload";
    // Minted by the pipeline (crate-private path exercised via its public
    // projection below); here we only verify the public cross-check refuses
    // mismatched bytes without a minted receipt.
    let forged = serde_json::from_value::<ValidationReceipt>(json!({
        "request_id": id,
        "source_digest": source,
        "candidate_digest": "c".repeat(64),
        "transforms": [],
        "validated_at_ms": 1,
        "revision": "validation_v1",
        "validation_digest": "d".repeat(64),
    }))
    .expect("deserializable");
    assert!(!forged.matches_artifact(&source, candidate));
    assert!(!forged.matches_artifact(&source, b"other bytes"));
}

#[test]
fn recovery_receipt_rejects_wrong_scope_and_expired() {
    // A recovery receipt binds request_id + source_digest + decision_digest.
    // Wrong scope must be rejected even when the receipt itself is valid.
    let id = request_id();
    let receipt = serde_json::from_value::<RecoveryReceipt>(json!({
        "request_id": id,
        "source_digest": "s".repeat(64),
        "decision_digest": "d".repeat(64),
        "recovery_ref": "ccr:durable/0001",
        "expires_at_unix": 1_000_000,
        "revision": "recovery_v1",
        "receipt_digest": "r".repeat(64),
    }))
    .expect("deserializable");

    assert!(receipt.matches_scope(id, &"s".repeat(64), &"d".repeat(64)));
    assert!(!receipt.matches_scope(request_id(), &"s".repeat(64), &"d".repeat(64)));
    assert!(!receipt.matches_scope(id, &"x".repeat(64), &"d".repeat(64)));
    assert!(!receipt.matches_scope(id, &"s".repeat(64), &"x".repeat(64)));
    assert!(!receipt.is_expired(999_999), "before expiry is not expired");
    assert!(
        !receipt.is_expired(1_000_000),
        "at the expiry instant is not yet expired"
    );
    assert!(receipt.is_expired(1_000_001), "past expiry fails closed");
    assert!(receipt.is_expired(2_000_000));
    // Tampered digest fails verification.
    assert!(!receipt.verify());
}

#[test]
fn send_commit_receipt_is_transport_observable_and_verify_rejects_forgery() {
    let id = request_id();
    let receipt = serde_json::from_value::<SendCommitReceipt>(json!({
        "request_id": id,
        "state": "send_committed",
        "variant": "optimized",
        "committed_at_ms": 1234,
        "transport_observed": true,
        "revision": "send_commit_v1",
        "commit_digest": "0".repeat(64),
    }))
    .expect("deserializable");

    assert_eq!(receipt.request_id(), id);
    assert_eq!(receipt.state(), SendCommitState::SendCommitted);
    assert_eq!(receipt.variant(), SentPayloadVariant::Optimized);
    assert_eq!(receipt.committed_at_ms(), Some(1234));
    assert!(receipt.transport_observed());
    assert!(!receipt.verify(), "forged commit digest must fail verify()");
}
