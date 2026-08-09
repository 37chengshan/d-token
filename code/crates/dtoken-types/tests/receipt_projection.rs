//! T-PR1 OptimizationReceipt projection contract tests (PR-1 runtime-receipt).
//!
//! MUST (PRODUCT-1 / RUNTIME-1/4/9 / docs/releases/v0.0.1 T-PR1):
//! - the receipt is a minimal PRD projection and never copies the whole
//!   HybridContextAccounting (RUNTIME-4);
//! - Local estimate / Provider actual / claimable stay separated: provider
//!   usage alone or EstimatedActive never makes savings claimable;
//! - physical_tokens_saved is projected from local before/after measurements
//!   only when both exist and the change is non-negative;
//! - the receipt never carries prompt/response bodies, credentials or local
//!   paths (redaction);
//! - RequestLog field additions are serde-backward compatible.

use dtoken_types::contract::AccountingBuckets;
use dtoken_types::{
    CacheTokens, HybridContextAccounting, InputTokenUsage, ModalityTokens, OptimizationDisposition,
    OptimizationReceipt, PayloadByteUsage, ProviderActualUsage, RequestLog, SavingsAttribution,
    SentPayloadVariant, UpstreamSendState, UsageComparison, UsageEvidence, UsageEvidenceKind,
    UsageMeasurement, UsageVector,
};
use serde_json::json;
use uuid::Uuid;

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

fn request_log() -> RequestLog {
    RequestLog {
        request_id: Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef),
        timestamp_ms: 42,
        agent: Some("test-agent".to_string()),
        instance_id: None,
        project_id: Some("project-a".to_string()),
        user_target_model: Some("user-model".to_string()),
        input_protocol: dtoken_types::Protocol::OpenAi,
        provider_id: Some("provider-a".to_string()),
        route_id: Some("route-a".to_string()),
        chain_mode: Some(dtoken_types::ChainMode::Direct),
        model: Some("routed-model".to_string()),
        input_tokens_before: Some(1000),
        input_tokens_after: Some(400),
        output_tokens: None,
        tokens_saved: Some(0),
        provider_input_tokens: Some(900),
        provider_output_tokens: Some(120),
        transforms_applied: vec!["repeat_line_fold".to_string()],
        latency_ms: Some(5),
        error: None,
        config_source: Some("dtoken-owned-runtime".to_string()),
        quota_metadata: None,
        context_accounting: None,
    }
}

#[test]
fn receipt_never_copies_the_whole_hybrid_accounting() {
    // RUNTIME-4: the receipt is a minimal projection; it must not embed the
    // full HybridContextAccounting record.
    let mut log = request_log();
    log.context_accounting = Some(accounting(
        OptimizationDisposition::Active,
        SentPayloadVariant::Optimized,
        UpstreamSendState::Completed,
    ));
    let receipt = OptimizationReceipt::from_request_log(&log);
    let value = serde_json::to_value(&receipt).unwrap();
    assert!(
        value.get("context_accounting").is_none(),
        "receipt must not copy HybridContextAccounting"
    );
    assert!(
        value.get("provider_counterfactual_baseline").is_none(),
        "no internal accounting fields leak"
    );
}

#[test]
fn provider_usage_alone_never_makes_savings_claimable() {
    // PRODUCT-1: provider usage recorded without a complete declarable
    // attribution chain is display-only; claimable stays None.
    let log = request_log();
    let receipt = OptimizationReceipt::from_request_log(&log);
    assert_eq!(receipt.provider_input_tokens, Some(900));
    assert_eq!(receipt.provider_output_tokens, Some(120));
    assert_eq!(
        receipt.savings_claimable, None,
        "usage alone is not claimable"
    );
}

#[test]
fn estimated_active_is_display_only_and_never_claimable() {
    let mut log = request_log();
    let mut accounting = accounting(
        OptimizationDisposition::Active,
        SentPayloadVariant::Optimized,
        UpstreamSendState::Completed,
    );
    accounting.attribution_basis = SavingsAttribution::EstimatedActive;
    log.context_accounting = Some(accounting);
    let receipt = OptimizationReceipt::from_request_log(&log);
    assert_eq!(receipt.savings_estimated, Some(40));
    assert_eq!(
        receipt.savings_claimable, None,
        "EstimatedActive never claimable"
    );
}

#[test]
fn paired_actual_is_the_only_claimable_basis() {
    let mut log = request_log();
    let mut accounting = accounting(
        OptimizationDisposition::Active,
        SentPayloadVariant::Optimized,
        UpstreamSendState::Completed,
    );
    accounting.attribution_basis = SavingsAttribution::PairedActual;
    accounting.paired_actual = Some(estimated_gain());
    log.context_accounting = Some(accounting);
    let receipt = OptimizationReceipt::from_request_log(&log);
    assert_eq!(receipt.savings_claimable, Some(40));
    assert_eq!(receipt.savings_estimated, Some(40));
}

#[test]
fn physical_tokens_saved_is_local_and_non_negative_only() {
    // Physical savings project from local before/after when both exist and
    // the change is non-negative; growth or missing measurements stay None.
    let log = request_log();
    let receipt = OptimizationReceipt::from_request_log(&log);
    assert_eq!(receipt.physical_tokens_saved, Some(600));

    let mut grown = request_log();
    grown.input_tokens_before = Some(100);
    grown.input_tokens_after = Some(200);
    assert_eq!(
        OptimizationReceipt::from_request_log(&grown).physical_tokens_saved,
        None
    );

    let mut partial = request_log();
    partial.input_tokens_after = None;
    assert_eq!(
        OptimizationReceipt::from_request_log(&partial).physical_tokens_saved,
        None
    );
}

#[test]
fn provider_or_model_changed_is_only_provable_with_both_models() {
    let log = request_log();
    let receipt = OptimizationReceipt::from_request_log(&log);
    assert_eq!(receipt.provider_or_model_changed, Some(true));

    let mut same = request_log();
    same.user_target_model = Some("routed-model".to_string());
    assert_eq!(
        OptimizationReceipt::from_request_log(&same).provider_or_model_changed,
        Some(false)
    );

    let mut unknown = request_log();
    unknown.user_target_model = None;
    assert_eq!(
        OptimizationReceipt::from_request_log(&unknown).provider_or_model_changed,
        None
    );
}

#[test]
fn four_dimension_buckets_project_independently_with_unknown_none() {
    // RUNTIME-9 adaptation: the receipt exposes the four-dimension buckets
    // contract (compression/command-output/provider-cache/relay). Unknown
    // dimensions stay None; never coerced to 0, never summed naively here.
    let log = request_log();
    let receipt = OptimizationReceipt::from_request_log(&log);
    assert_eq!(
        receipt.buckets,
        Some(AccountingBuckets {
            compression: Some(600),
            command_output: None,
            provider_cache: None,
            relay: None,
        })
    );
    let value = serde_json::to_value(receipt.buckets.as_ref().unwrap()).unwrap();
    assert!(value["command_output"].is_null());
    assert!(value["provider_cache"].is_null());
    assert!(value["relay"].is_null());
}

#[test]
fn receipt_redaction_never_leaks_body_paths_or_secrets() {
    // Redaction contract: even a request log carrying sensitive-looking
    // quota metadata must not leak raw values into the receipt.
    let mut log = request_log();
    log.quota_metadata = Some(json!({
        "raw_header": "Bearer sk-placeholder-secret-0123456789abcdef",
        "local_path": "/Users/me/.config/dtoken/config.toml",
    }));
    log.context_accounting = Some(accounting(
        OptimizationDisposition::Active,
        SentPayloadVariant::Optimized,
        UpstreamSendState::Completed,
    ));
    let receipt = OptimizationReceipt::from_request_log(&log);
    let value = serde_json::to_value(&receipt).unwrap().to_string();
    assert!(
        !value.contains("sk-placeholder-secret"),
        "no secret in receipt"
    );
    assert!(
        !value.contains("/Users/me/.config"),
        "no local path in receipt"
    );
    assert!(!value.contains("Bearer"), "no raw header in receipt");
}

#[test]
fn request_log_additions_are_serde_backward_compatible() {
    // Old persisted JSON without the new fields must still deserialize.
    let legacy = json!({
        "request_id": Uuid::new_v4(),
        "timestamp_ms": 1,
        "agent": null,
        "instance_id": null,
        "input_protocol": "open_ai",
        "provider_id": null,
        "route_id": null,
        "chain_mode": null,
        "model": null,
        "input_tokens_before": null,
        "input_tokens_after": null,
        "output_tokens": null,
        "tokens_saved": null,
        "transforms_applied": [],
        "latency_ms": null,
        "error": null,
        "config_source": null,
        "quota_metadata": null,
        "context_accounting": null
    });
    let log: RequestLog = serde_json::from_value(legacy).expect("legacy row must migrate");
    assert_eq!(log.project_id, None);
    assert_eq!(log.user_target_model, None);
    assert_eq!(log.provider_input_tokens, None);
    assert_eq!(log.provider_output_tokens, None);
}
