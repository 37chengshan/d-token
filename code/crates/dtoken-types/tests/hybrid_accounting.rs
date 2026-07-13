use dtoken_types::{
    AccountingValidationError, CacheTokens, HybridContextAccounting, InputTokenUsage,
    ModalityTokens, OptimizationDisposition, PayloadByteUsage, ProviderActualUsage, RequestLog,
    SavingsAttribution, SentPayloadVariant, SignedSavings, UpstreamSendState, UsageComparison,
    UsageEvidence, UsageEvidenceKind, UsageMeasurement, UsageVector,
};

fn measurement(total: u64, output: u64, request_bytes: u64) -> UsageMeasurement {
    UsageMeasurement {
        evidence: UsageEvidence {
            kind: UsageEvidenceKind::LocalEstimate,
            source_revision: "test-profile-v1".to_string(),
        },
        usage: UsageVector {
            input: InputTokenUsage {
                normalized_total: Some(total),
                modality: ModalityTokens {
                    text: Some(total),
                    ..Default::default()
                },
                cache: CacheTokens::default(),
            },
            output_tokens: Some(output),
            bytes: PayloadByteUsage {
                request_json: Some(request_bytes),
                wire_body: Some(request_bytes),
                ..Default::default()
            },
            cost_microusd: None,
        },
    }
}

fn estimated_gain() -> UsageComparison {
    UsageComparison::from_measurements(measurement(100, 10, 1_000), measurement(60, 10, 800))
}

fn accounting(
    disposition: OptimizationDisposition,
    sent_payload: SentPayloadVariant,
    upstream_state: UpstreamSendState,
) -> HybridContextAccounting {
    HybridContextAccounting {
        schema_version: 1,
        disposition,
        sent_payload,
        upstream_state,
        estimated: Some(estimated_gain()),
        provider_counterfactual_baseline: None,
        provider_actual_sent: ProviderActualUsage::default(),
        provider_reconciled_input_savings: None,
        paired_actual: None,
        attribution_basis: SavingsAttribution::None,
        fallback_reason_code: None,
    }
}

#[test]
fn negative_multimodal_savings_round_trip_without_clamp() {
    let comparison = UsageComparison::from_measurements(
        measurement(100, 10, 1_000),
        measurement(140, 12, 3_000),
    );

    assert_eq!(comparison.signed_savings.input_tokens, Some(-40));
    assert_eq!(comparison.signed_savings.output_tokens, Some(-2));
    assert_eq!(comparison.signed_savings.total_tokens, Some(-42));
    assert_eq!(comparison.signed_savings.request_bytes, Some(-2_000));

    let encoded = serde_json::to_string(&comparison).expect("serialize accounting");
    let decoded: UsageComparison = serde_json::from_str(&encoded).expect("deserialize accounting");
    assert_eq!(decoded, comparison);
}

#[test]
fn shadow_and_pre_send_fallback_do_not_claim_realized_savings() {
    let shadow = accounting(
        OptimizationDisposition::Shadow,
        SentPayloadVariant::Original,
        UpstreamSendState::Completed,
    );
    let fallback = accounting(
        OptimizationDisposition::FallbackPreSend,
        SentPayloadVariant::Original,
        UpstreamSendState::Completed,
    );

    assert_eq!(shadow.claimable_signed_savings(), None);
    assert_eq!(fallback.claimable_signed_savings(), None);
    assert_eq!(shadow.legacy_positive_tokens_saved(), 0);
    assert_eq!(fallback.legacy_positive_tokens_saved(), 0);
    assert_eq!(shadow.validate(), Ok(()));
    assert_eq!(fallback.validate(), Ok(()));
}

#[test]
fn active_estimate_can_express_a_positive_legacy_projection() {
    let mut active = accounting(
        OptimizationDisposition::Active,
        SentPayloadVariant::Optimized,
        UpstreamSendState::Completed,
    );
    active.attribution_basis = SavingsAttribution::EstimatedActive;

    assert_eq!(
        active.claimable_signed_savings(),
        Some(&SignedSavings {
            input_tokens: Some(40),
            output_tokens: Some(0),
            total_tokens: Some(40),
            request_bytes: Some(200),
            png_bytes: None,
            base64_bytes: None,
            cost_microusd: None,
        })
    );
    assert_eq!(active.legacy_positive_tokens_saved(), 40);
    assert_eq!(active.validate(), Ok(()));
}

#[test]
fn post_commit_failure_cannot_be_recorded_as_pre_send_fallback() {
    let fallback = accounting(
        OptimizationDisposition::FallbackPreSend,
        SentPayloadVariant::Original,
        UpstreamSendState::FailedAfterCommit,
    );

    assert_eq!(
        fallback.validate(),
        Err(AccountingValidationError::PostCommitCannotBePreSendFallback)
    );
}

#[test]
fn failed_active_send_cannot_claim_estimated_savings() {
    let mut failed = accounting(
        OptimizationDisposition::Active,
        SentPayloadVariant::Optimized,
        UpstreamSendState::FailedAfterCommit,
    );
    failed.attribution_basis = SavingsAttribution::EstimatedActive;

    assert_eq!(failed.claimable_signed_savings(), None);
    assert_eq!(failed.legacy_positive_tokens_saved(), 0);
    assert_eq!(
        failed.validate(),
        Err(AccountingValidationError::SavingsClaimRequiresCompletedSend)
    );
}

#[test]
fn paired_actual_is_claimable_only_for_completed_active_payload() {
    let mut active = accounting(
        OptimizationDisposition::Active,
        SentPayloadVariant::Optimized,
        UpstreamSendState::Completed,
    );
    active.attribution_basis = SavingsAttribution::PairedActual;
    active.paired_actual = Some(UsageComparison::from_measurements(
        measurement(120, 20, 1_200),
        measurement(90, 18, 900),
    ));

    assert_eq!(active.validate(), Ok(()));
    assert_eq!(
        active.claimable_signed_savings(),
        active
            .paired_actual
            .as_ref()
            .map(|comparison| &comparison.signed_savings)
    );
    assert_eq!(active.legacy_positive_tokens_saved(), 32);
}

#[test]
fn provider_actual_without_counterfactual_is_not_actual_savings() {
    let mut active = accounting(
        OptimizationDisposition::Active,
        SentPayloadVariant::Optimized,
        UpstreamSendState::Completed,
    );
    active.estimated = None;
    active.provider_actual_sent.measurement = Some(measurement(80, 5, 900));
    active.attribution_basis = SavingsAttribution::None;

    assert_eq!(active.provider_reconciled_input_savings, None);
    assert_eq!(active.claimable_signed_savings(), None);
    assert_eq!(active.validate(), Ok(()));
}

#[test]
fn legacy_request_log_deserializes_without_context_accounting() {
    let legacy = serde_json::json!({
        "request_id":"00000000-0000-0000-0000-000000000001",
        "timestamp_ms":1,
        "agent":"OpenCode",
        "instance_id":"instance",
        "input_protocol":"open_ai",
        "provider_id":"provider",
        "route_id":"route",
        "chain_mode":"direct",
        "model":"model",
        "input_tokens_before":100,
        "input_tokens_after":80,
        "output_tokens":10,
        "tokens_saved":20,
        "transforms_applied":["legacy"],
        "latency_ms":5,
        "error":null,
        "config_source":"legacy"
    });

    let log: RequestLog = serde_json::from_value(legacy).expect("legacy row should migrate");
    assert!(log.context_accounting.is_none());
}
