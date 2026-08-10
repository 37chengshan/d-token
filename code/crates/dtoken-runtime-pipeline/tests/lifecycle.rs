//! T-C0B / RUNTIME-8 lifecycle contract tests.
//!
//! The pipeline types are sealed: an optimized payload cannot become
//! sendable without validation and a durable recovery receipt, and a
//! transport can mint at most one terminal SendCommitReceipt by consuming
//! the sendable unit. These behavioral negatives prove the state machine
//! cannot be forged by callers.

use dtoken_runtime_pipeline::{
    PipelineError, PipelineInput, RecoveryProof, RecoveryProofAuthority, RecoveryReceipt,
    SendCommitState, SentPayloadVariant, TransformArtifact, ValidatedRequest,
};
use dtoken_types::{
    ChainMode, CompressionPolicy, EvidenceSource, EvidenceValue, Protocol, RequestScope,
    RouteDecision,
};
use uuid::Uuid;

fn scope() -> RequestScope {
    RequestScope::new(
        Uuid::new_v4(),
        EvidenceValue::known(
            "claude-code".to_string(),
            EvidenceSource::ConfiguredInstance,
        ),
        EvidenceValue::unknown("project_identity_unavailable"),
        EvidenceValue::unknown("session_identity_unavailable"),
        EvidenceValue::known("instance-a".to_string(), EvidenceSource::ConfiguredRoute),
        "route-a",
        "provider-a",
        "model-a",
        Protocol::Anthropic,
        Protocol::Anthropic,
        "config-revision",
    )
    .unwrap()
}

/// Test stand-in for the production store authority.  Only the CCR store
/// holds the key in production; tests stand in for the store issuance side.
fn authority() -> RecoveryProofAuthority {
    RecoveryProofAuthority::new([7_u8; 32])
}

/// Active route decision for the prepared request.
fn decision_for(prepared: &dtoken_runtime_pipeline::PreparedRequest) -> RouteDecision {
    let scope = prepared.scope();
    RouteDecision {
        request_id: scope.request_id,
        route_id: scope.route_id.clone(),
        provider_id: scope.provider_id.clone(),
        chain_mode: ChainMode::Direct,
        compression_policy: CompressionPolicy::Balanced,
        reason: "test decision".to_string(),
        input_protocol: Some(scope.input_protocol.clone()),
        output_protocol: Some(scope.output_protocol.clone()),
    }
}

/// Store-issued proof for the durable test reference.
fn proof() -> RecoveryProof {
    authority()
        .issue("ccr:durable-ref-1", 1_000_000_000_000)
        .expect("store-issued proof")
}

fn recovery_for(prepared: &dtoken_runtime_pipeline::PreparedRequest) -> RecoveryReceipt {
    let artifact = TransformArtifact::new(
        prepared.baseline_digest(),
        br#"{"messages":["candidate"]}"#.to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    let candidate = prepared
        .clone()
        .with_durable_candidate(artifact, &decision_for(prepared), proof(), &authority())
        .expect("valid durable recovery receipt");
    candidate.recovery_receipt().clone()
}

fn prepared_with_candidate(
) -> (dtoken_runtime_pipeline::PreparedRequest, dtoken_runtime_pipeline::CandidateRequest) {
    let input = PipelineInput::new(scope(), br#"{"messages":["baseline"]}"#.to_vec(), 1024)
        .expect("bounded input");
    let prepared = input.prepare();
    let artifact = TransformArtifact::new(
        prepared.baseline_digest(),
        br#"{"messages":["candidate"]}"#.to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    let candidate = prepared
        .clone()
        .with_durable_candidate(artifact, &decision_for(&prepared), proof(), &authority())
        .expect("candidate bound");
    (prepared, candidate)
}

#[test]
fn optimized_send_requires_matching_source_validation_and_recovery_receipt() {
    let input = PipelineInput::new(scope(), br#"{"messages":["baseline"]}"#.to_vec(), 1024)
        .expect("bounded input");
    let prepared = input.prepare();
    let mismatched = TransformArtifact::new(
        "wrong-digest",
        br#"{"messages":["candidate"]}"#.to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    assert_eq!(
        prepared
            .clone()
            .with_candidate(mismatched, None)
            .unwrap_err(),
        PipelineError::SourceDigestMismatch
    );

    let artifact = TransformArtifact::new(
        prepared.baseline_digest(),
        br#"{"messages":["candidate"]}"#.to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    assert_eq!(
        prepared
            .clone()
            .with_candidate(artifact.clone(), None)
            .unwrap_err(),
        PipelineError::MissingRecoveryReceipt
    );

    let recovery = recovery_for(&prepared);
    let candidate = prepared
        .with_candidate(artifact, Some(recovery))
        .expect("candidate bound");
    let validated = candidate
        .validate(|baseline, transformed| baseline != transformed)
        .expect("validator accepts changed candidate");
    let ready = validated.into_ready_to_send();
    assert_eq!(ready.variant(), SentPayloadVariant::Optimized);
    let receipt = ready.mint_send_commit(101);
    assert_eq!(receipt.state(), SendCommitState::SendCommitted);
}

#[test]
fn validation_receipt_is_minted_only_after_validation_and_binds_the_validated_digest() {
    let (_prepared, candidate) = prepared_with_candidate();
    let validated = candidate
        .validate(|baseline, transformed| baseline != transformed)
        .unwrap();
    let receipt = validated.mint_validation_receipt(42);
    assert_eq!(receipt.request_id(), validated.scope().request_id);
    assert_eq!(receipt.source_digest(), validated.source_digest());
    // The validated digest is the sha256 of the exact validated candidate
    // bytes: {"messages":["candidate"]}.
    assert_eq!(
        receipt.validated_digest(),
        "8f9394ff25f7ede3e0a84aafcd3801bde800ac72ef8dc6388e3851098ea97bb9"
    );
    assert_eq!(receipt.variant(), SentPayloadVariant::Optimized);

    // A post-validation mutation changes the digest and is structurally
    // excluded: the receipt keeps the digest of the validated bytes.
    assert_ne!(
        receipt.validated_digest(),
        PipelineInput::new(scope(), br#"{"messages":["MUTATED"]}"#.to_vec(), 1024)
            .unwrap()
            .prepare()
            .into_baseline_ready("mutated")
            .body_digest()
    );
}

#[test]
fn send_commit_receipt_binds_the_exact_sent_body_digest() {
    let (_prepared, candidate) = prepared_with_candidate();
    let ready = candidate
        .validate(|baseline, transformed| baseline != transformed)
        .unwrap()
        .into_ready_to_send();
    let expected = ready.body_digest();
    let receipt = ready.mint_send_commit(101);
    assert_eq!(receipt.sent_digest(), &expected);
    assert_eq!(receipt.variant(), SentPayloadVariant::Optimized);
    assert_eq!(receipt.recovery().unwrap().recovery_ref(), "ccr:durable-ref-1");
    assert_eq!(receipt.transforms(), &["safe-text".to_string()]);
}

#[test]
fn recovery_receipt_rejects_non_durable_refs_and_forged_bindings() {
    let input = PipelineInput::new(scope(), br#"{"messages":["baseline"]}"#.to_vec(), 1024)
        .expect("bounded input");
    let prepared = input.prepare();
    let artifact = |digest: &str| {
        TransformArtifact::new(
            digest,
            br#"{"messages":["candidate"]}"#.to_vec(),
            vec!["safe-text".to_string()],
        )
        .unwrap()
    };

    // Issuance is the single gate (PIPELINE-RECEIPT-AUTHENTICITY): the
    // store authority refuses free-form strings, bare prefixes and zero
    // expiry, so an invented `ccr:` string can never become a proof.
    assert_eq!(
        authority().issue("plain-ref", 1_000).unwrap_err(),
        PipelineError::InvalidRecoveryReceipt
    );
    assert_eq!(
        authority().issue("ccr:", 1_000).unwrap_err(),
        PipelineError::InvalidRecoveryReceipt,
        "bare prefix with no ref is not durable"
    );
    assert_eq!(
        authority().issue("ccr:no-expiry", 0).unwrap_err(),
        PipelineError::InvalidRecoveryReceipt
    );

    // A proof minted under a different store key (a caller that does not
    // hold the store authority) must fail closed at candidate binding.
    let forged = RecoveryProofAuthority::new([9_u8; 32])
        .issue("ccr:forged", 1_000_000_000_000)
        .expect("foreign-key proof");
    assert_eq!(
        prepared
            .clone()
            .with_durable_candidate(
                artifact(prepared.baseline_digest()),
                &decision_for(&prepared),
                forged,
                &authority(),
            )
            .unwrap_err(),
        PipelineError::InvalidRecoveryReceipt,
        "a proof from a foreign authority is not store-issued"
    );

    // A receipt minted for a different prepared request (different source
    // digest) must never bind to this request. Receipts can only be obtained
    // through the pipeline flow, so the foreign receipt comes from another
    // prepared request's flow and is rejected on binding here.
    let other = PipelineInput::new(scope(), b"other-baseline".to_vec(), 64)
        .unwrap()
        .prepare();
    let other_artifact = TransformArtifact::new(
        other.baseline_digest(),
        b"candidate".to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    let foreign_candidate = other
        .clone()
        .with_durable_candidate(other_artifact, &decision_for(&other), proof(), &authority())
        .expect("foreign request binds its own receipt");
    let foreign = foreign_candidate.recovery_receipt().clone();
    assert!(!foreign.is_valid_for(&prepared, 50));
    let bound = prepared
        .clone()
        .with_candidate(artifact(prepared.baseline_digest()), Some(foreign));
    assert_eq!(
        bound.unwrap_err(),
        PipelineError::ReceiptIdentityMismatch
    );
}

#[test]
fn recovery_receipt_expiry_is_enforced() {
    let input = PipelineInput::new(scope(), b"baseline".to_vec(), 64).unwrap();
    let prepared = input.prepare();
    let artifact = TransformArtifact::new(
        prepared.baseline_digest(),
        b"candidate".to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();

    // Expired store proof fails closed at binding time: the pipeline reads
    // its own trusted clock, so a caller cannot revive an expired proof by
    // claiming an old time (RUNTIME-8).
    let expired = authority()
        .issue("ccr:expired", 1)
        .expect("store-issued proof");
    assert_eq!(
        prepared
            .clone()
            .with_durable_candidate(artifact.clone(), &decision_for(&prepared), expired, &authority())
            .unwrap_err(),
        PipelineError::InvalidRecoveryReceipt,
        "an expired proof must fail closed"
    );

    // A live proof binds and the receipt enforces the same boundary.
    let candidate = prepared
        .clone()
        .with_durable_candidate(
            artifact,
            &decision_for(&prepared),
            authority()
                .issue("ccr:expiring", 1_000_000_000_000)
                .expect("store-issued proof"),
            &authority(),
        )
        .unwrap();
    let receipt = candidate.recovery_receipt();
    assert!(receipt.is_valid_for(&prepared, 50));
    assert!(!receipt.is_valid_for(&prepared, 1_000_000_000_001));
}

#[test]
fn recovery_receipt_rejects_inactive_route_decisions() {
    let input = PipelineInput::new(scope(), br#"{"messages":["baseline"]}"#.to_vec(), 1024)
        .expect("bounded input");
    let prepared = input.prepare();
    let artifact = TransformArtifact::new(
        prepared.baseline_digest(),
        b"candidate".to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    let live_proof = || {
        authority()
            .issue("ccr:bound", 1_000_000_000_000)
            .expect("store-issued proof")
    };

    // Decision for a different request identity must never bind.
    let foreign_decision = {
        let mut decision = decision_for(&prepared);
        decision.request_id = Uuid::new_v4();
        decision
    };
    assert_eq!(
        prepared
            .clone()
            .with_durable_candidate(artifact.clone(), &foreign_decision, live_proof(), &authority())
            .unwrap_err(),
        PipelineError::RouteDecisionMismatch,
        "a decision for another request is not active"
    );

    // Decision for the same request but a different active route/provider.
    let wrong_route = {
        let mut decision = decision_for(&prepared);
        decision.route_id = "route-forged".to_string();
        decision
    };
    assert_eq!(
        prepared
            .clone()
            .with_durable_candidate(artifact.clone(), &wrong_route, live_proof(), &authority())
            .unwrap_err(),
        PipelineError::RouteDecisionMismatch,
        "a mutated route identity is not active"
    );
    let wrong_provider = {
        let mut decision = decision_for(&prepared);
        decision.provider_id = "provider-forged".to_string();
        decision
    };
    assert_eq!(
        prepared
            .clone()
            .with_durable_candidate(artifact, &wrong_provider, live_proof(), &authority())
            .unwrap_err(),
        PipelineError::RouteDecisionMismatch,
        "a mutated provider identity is not active"
    );
}

#[test]
fn baseline_is_sendable_without_recovery_and_keeps_reason() {
    let ready = PipelineInput::new(scope(), b"baseline".to_vec(), 32)
        .unwrap()
        .prepare()
        .into_baseline_ready("codec_not_applicable");
    assert_eq!(ready.variant(), SentPayloadVariant::Original);
    assert_eq!(ready.reason_code(), "codec_not_applicable");
    let receipt = ready.mint_send_commit(1);
    assert_eq!(receipt.variant(), SentPayloadVariant::Original);
    assert!(receipt.recovery().is_none());
}

#[test]
fn context_envelope_is_bounded_and_is_the_only_typed_pipeline_ingress() {
    let input = PipelineInput::new(scope(), Vec::new(), 32);
    assert_eq!(input.unwrap_err(), PipelineError::EmptyIngress);
    let input = PipelineInput::new(scope(), vec![0; 33], 32);
    assert_eq!(input.unwrap_err(), PipelineError::IngressTooLarge);
}

#[test]
fn transform_artifact_rejects_empty_source_candidate_or_transform_list() {
    assert_eq!(
        TransformArtifact::new("", b"candidate".to_vec(), vec!["safe-text".to_string()])
            .unwrap_err(),
        PipelineError::EmptySourceDigest
    );
    assert_eq!(
        TransformArtifact::new("digest", Vec::new(), vec!["safe-text".to_string()]).unwrap_err(),
        PipelineError::EmptyCandidate
    );
    assert_eq!(
        TransformArtifact::new("digest", b"candidate".to_vec(), Vec::new()).unwrap_err(),
        PipelineError::EmptyTransformList
    );
    assert_eq!(
        TransformArtifact::new(
            "digest",
            b"candidate".to_vec(),
            vec!["safe-text".to_string(), "  ".to_string()],
        )
        .unwrap_err(),
        PipelineError::EmptyTransformList
    );
}

#[test]
fn validator_rejection_never_reaches_a_sendable_candidate() {
    let (_prepared, candidate) = prepared_with_candidate();
    assert_eq!(
        candidate.validate(|_, _| false).unwrap_err(),
        PipelineError::ValidatorRejected
    );
}

#[test]
fn pre_send_fault_marks_failed_before_commit_and_keeps_the_original_variant() {
    let ready = PipelineInput::new(scope(), b"baseline".to_vec(), 32)
        .unwrap()
        .prepare()
        .into_baseline_ready("transport_unavailable");
    let receipt = ready.mint_send_commit(0);
    assert_eq!(receipt.state(), SendCommitState::FailedBeforeCommit);
    assert_eq!(receipt.committed_at_ms(), None);
    assert_eq!(receipt.variant(), SentPayloadVariant::Original);
    assert_eq!(receipt.transforms(), &[] as &[String]);
}

#[test]
fn post_commit_receipt_is_terminal_and_carries_the_optimized_evidence() {
    let (_prepared, candidate) = prepared_with_candidate();
    let ready = candidate
        .validate(|baseline, transformed| baseline != transformed)
        .unwrap()
        .into_ready_to_send();
    let receipt = ready.mint_send_commit(101);
    assert_eq!(receipt.state(), SendCommitState::SendCommitted);
    assert_eq!(receipt.committed_at_ms(), Some(101));
    assert_eq!(receipt.variant(), SentPayloadVariant::Optimized);
    assert_eq!(receipt.transforms(), &["safe-text".to_string()]);
    assert_eq!(receipt.recovery().unwrap().recovery_ref(), "ccr:durable-ref-1");
    assert!(receipt.is_terminal());
}

#[test]
fn validation_and_commit_receipts_carry_the_same_sent_digest_for_optimized_units() {
    let (_prepared, candidate) = prepared_with_candidate();
    let validated: ValidatedRequest = candidate
        .validate(|baseline, transformed| baseline != transformed)
        .unwrap();
    let validation = validated.mint_validation_receipt(42);
    let ready = validated.into_ready_to_send();
    let commit = ready.mint_send_commit(101);
    assert_eq!(
        validation.validated_digest(),
        commit.sent_digest(),
        "production send body must equal the validated body (T-C1B)"
    );
}
