use dtoken_runtime_pipeline::{
    ContextEnvelope, PipelineError, PipelineInput, RecoveryPrepareReceipt, SendCommitState,
    SentPayloadVariant, TransformArtifact,
};
use dtoken_types::{EvidenceSource, EvidenceValue, Protocol, RequestScope};
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

    let candidate = prepared
        .with_candidate(
            artifact,
            Some(RecoveryPrepareReceipt::new("ccr:prepared", 100).unwrap()),
        )
        .unwrap();
    let validated = candidate
        .validate(|baseline, transformed| baseline != transformed)
        .expect("validator accepts changed candidate");
    let ready = validated.into_ready_to_send();
    assert_eq!(ready.variant(), SentPayloadVariant::Optimized);
    let receipt = ready.mark_committed(101);
    assert_eq!(receipt.state(), SendCommitState::SendCommitted);
}

#[test]
fn baseline_is_sendable_without_recovery_and_keeps_reason() {
    let ready = PipelineInput::new(scope(), b"baseline".to_vec(), 32)
        .unwrap()
        .prepare()
        .into_baseline_ready("codec_not_applicable");
    assert_eq!(ready.variant(), SentPayloadVariant::Original);
    assert_eq!(ready.reason_code(), "codec_not_applicable");
}

#[test]
fn context_envelope_is_bounded_and_is_the_only_typed_pipeline_ingress() {
    assert_eq!(
        ContextEnvelope::new(scope(), Vec::new(), 32).unwrap_err(),
        PipelineError::EmptyIngress
    );
    assert_eq!(
        ContextEnvelope::new(scope(), vec![0; 33], 32).unwrap_err(),
        PipelineError::IngressTooLarge
    );

    let envelope = ContextEnvelope::new(scope(), b"bounded".to_vec(), 32).unwrap();
    let ready = PipelineInput::from_envelope(envelope)
        .prepare()
        .into_baseline_ready("envelope_validated");
    assert_eq!(ready.reason_code(), "envelope_validated");
}
