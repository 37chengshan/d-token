//! PR-C6C contract: pre-commit candidate failure sends baseline once,
//! post-commit never retries or switches provider, one terminal receipt per
//! request. Receipts are sealed; a transport can only mint them by consuming
//! the sendable unit at the observable commit point.

use dtoken_runtime_pipeline::{
    PipelineError, PipelineInput, RecoveryPhase, RecoveryProofAuthority, RecoveryReceipt,
    RecoveryTracker, RecoveryTransitionError, SendCommitState, SentPayloadVariant,
    TransformArtifact,
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

fn prepared_request() -> (Uuid, dtoken_runtime_pipeline::PreparedRequest) {
    let input = PipelineInput::new(scope(), br#"{"messages":["baseline"]}"#.to_vec(), 1024)
        .expect("bounded input");
    let prepared = input.prepare();
    let request_id = prepared.scope().request_id;
    (request_id, prepared)
}

/// Test stand-in for the production store authority.
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

fn recovery_for(prepared: &dtoken_runtime_pipeline::PreparedRequest) -> RecoveryReceipt {
    let artifact = TransformArtifact::new(
        prepared.baseline_digest(),
        br#"{"messages":["candidate"]}"#.to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    let proof = authority()
        .issue("ccr:durable-ref-1", 1_000_000_000_000)
        .expect("store-issued proof");
    let candidate = prepared
        .clone()
        .with_durable_candidate(artifact, &decision_for(prepared), proof, &authority())
        .expect("valid durable recovery receipt");
    candidate.recovery_receipt().clone()
}

#[test]
fn pre_commit_candidate_failure_sends_baseline_exactly_once() {
    let (request_id, prepared) = prepared_request();
    let mut tracker = RecoveryTracker::new(request_id);

    // Candidate path: artifact fails source-digest validation.
    let artifact = TransformArtifact::new(
        "wrong-digest",
        b"candidate".to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    assert_eq!(
        prepared.clone().with_candidate(artifact, None).unwrap_err(),
        PipelineError::SourceDigestMismatch
    );

    // Recovery: baseline dispatched once.
    assert_eq!(
        tracker.record_baseline_dispatch(),
        Ok(RecoveryPhase::BaselineDispatched)
    );
    assert_eq!(
        tracker.record_baseline_dispatch().unwrap_err(),
        RecoveryTransitionError::BaselineAlreadyDispatched,
        "a fault loop must never re-send the baseline"
    );

    // The baseline unit itself is the only sendable output of this request.
    let ready = prepared.into_baseline_ready("candidate_failed_preserve");
    assert_eq!(ready.variant(), SentPayloadVariant::Original);
    let receipt = ready.mint_send_commit(50);
    assert_eq!(receipt.state(), SendCommitState::SendCommitted);
    assert_eq!(receipt.reason_code(), "candidate_failed_preserve");
    assert_eq!(
        tracker.record_terminal(&receipt),
        Ok(RecoveryPhase::Terminal)
    );
}

#[test]
fn post_commit_is_terminal_no_retry_no_second_receipt() {
    let (request_id, prepared) = prepared_request();
    let mut tracker = RecoveryTracker::new(request_id);

    let artifact = TransformArtifact::new(
        prepared.baseline_digest(),
        br#"{"messages":["candidate"]}"#.to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    let recovery = recovery_for(&prepared);
    let ready = prepared
        .with_candidate(artifact, Some(recovery))
        .unwrap()
        .validate(|baseline, transformed| baseline != transformed)
        .unwrap()
        .into_ready_to_send();
    assert_eq!(
        tracker.record_candidate_dispatch(),
        Ok(RecoveryPhase::CandidateDispatched)
    );

    // Commit at the observable transport point: one terminal receipt, then
    // the machine is absorbing.
    let receipt = ready.mint_send_commit(101);
    assert_eq!(receipt.state(), SendCommitState::SendCommitted);
    assert_eq!(
        tracker.record_terminal(&receipt),
        Ok(RecoveryPhase::Terminal)
    );

    // Post-commit retry attempts are all rejected: no second baseline, no
    // second candidate dispatch, no second terminal receipt.
    assert_eq!(
        tracker.record_baseline_dispatch().unwrap_err(),
        RecoveryTransitionError::AlreadyTerminal
    );
    assert_eq!(
        tracker.record_candidate_dispatch().unwrap_err(),
        RecoveryTransitionError::AlreadyTerminal
    );
    // A second transport that somehow produced another receipt for the same
    // request identity is still rejected: one terminal receipt per request.
    let duplicate_scope = {
        let mut scope = scope();
        scope.request_id = request_id;
        scope
    };
    let duplicate = PipelineInput::new(duplicate_scope, b"duplicate".to_vec(), 64)
        .unwrap()
        .prepare()
        .into_baseline_ready("duplicate")
        .mint_send_commit(102);
    assert_eq!(
        tracker.record_terminal(&duplicate).unwrap_err(),
        RecoveryTransitionError::AlreadyTerminal
    );
    assert_eq!(tracker.phase(), RecoveryPhase::Terminal);
}

#[test]
fn foreign_receipt_cannot_settle_a_request_identity() {
    let (request_id, _) = prepared_request();
    let mut tracker = RecoveryTracker::new(request_id);

    let other_scope = {
        let mut scope = scope();
        scope.request_id = Uuid::new_v4();
        scope
    };
    let foreign = PipelineInput::new(other_scope, b"other".to_vec(), 64)
        .unwrap()
        .prepare()
        .into_baseline_ready("foreign")
        .mint_send_commit(1);
    assert_eq!(
        tracker.record_terminal(&foreign).unwrap_err(),
        RecoveryTransitionError::RequestIdentityMismatch
    );
    assert_eq!(tracker.phase(), RecoveryPhase::Preparing);
}

#[test]
fn provider_identity_is_bound_at_ingress_and_never_switchable_after_commit() {
    let ingress_scope = scope();
    let input = PipelineInput::new(
        ingress_scope.clone(),
        br#"{"messages":["baseline"]}"#.to_vec(),
        1024,
    )
    .unwrap();
    let prepared = input.prepare();
    let artifact = TransformArtifact::new(
        prepared.baseline_digest(),
        br#"{"messages":["candidate"]}"#.to_vec(),
        vec!["safe-text".to_string()],
    )
    .unwrap();
    let recovery = recovery_for(&prepared);
    let receipt = prepared
        .with_candidate(artifact, Some(recovery))
        .unwrap()
        .validate(|baseline, transformed| baseline != transformed)
        .unwrap()
        .into_ready_to_send()
        .mint_send_commit(101);

    // The receipt is bound to the exact ingress identity; there is no setter
    // on any public type, so a post-commit provider switch is impossible.
    assert_eq!(receipt.route_id(), ingress_scope.route_id);
    assert_eq!(receipt.provider_id(), ingress_scope.provider_id);
    assert_eq!(receipt.model(), ingress_scope.model);
    assert_eq!(receipt.scope().request_id, ingress_scope.request_id);
}

#[test]
fn pre_send_fault_is_terminal_without_a_send() {
    let (request_id, prepared) = prepared_request();
    let mut tracker = RecoveryTracker::new(request_id);
    assert_eq!(
        tracker.record_baseline_dispatch(),
        Ok(RecoveryPhase::BaselineDispatched)
    );
    let receipt = prepared
        .into_baseline_ready("transport_unavailable")
        .mint_send_commit(0);
    assert_eq!(receipt.state(), SendCommitState::FailedBeforeCommit);
    assert_eq!(receipt.committed_at_ms(), None);
    assert_eq!(
        tracker.record_terminal(&receipt),
        Ok(RecoveryPhase::Terminal)
    );
    assert_eq!(tracker.phase(), RecoveryPhase::Terminal);
}

#[test]
fn saturated_and_cancelled_requests_preserve_and_still_commit() {
    let (request_id, prepared) = prepared_request();
    let mut tracker = RecoveryTracker::new(request_id);
    assert_eq!(
        tracker.record_baseline_dispatch(),
        Ok(RecoveryPhase::BaselineDispatched)
    );
    // Saturation Preserve (PR-C6A) still yields one sendable unit and one
    // terminal receipt with the evidence reason code.
    let ready = prepared.into_preserved(dtoken_runtime_pipeline::PreserveReason::QueueSaturated);
    assert_eq!(ready.variant(), SentPayloadVariant::Original);
    assert!(ready.reason_code().contains("saturated"));
    let receipt = ready.mint_send_commit(7);
    assert_eq!(receipt.state(), SendCommitState::SendCommitted);
    assert_eq!(
        tracker.record_terminal(&receipt),
        Ok(RecoveryPhase::Terminal)
    );

    let (request_id, prepared) = prepared_request();
    let mut tracker = RecoveryTracker::new(request_id);
    assert_eq!(
        tracker.record_baseline_dispatch(),
        Ok(RecoveryPhase::BaselineDispatched)
    );
    let ready = prepared.into_preserved(dtoken_runtime_pipeline::PreserveReason::Cancelled);
    let receipt = ready.mint_send_commit(8);
    assert_eq!(receipt.state(), SendCommitState::SendCommitted);
    assert!(receipt.reason_code().contains("cancelled"));
    assert_eq!(
        tracker.record_terminal(&receipt),
        Ok(RecoveryPhase::Terminal)
    );
}
