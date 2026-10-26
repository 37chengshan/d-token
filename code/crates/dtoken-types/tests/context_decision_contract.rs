use dtoken_types::{
    ConfidenceBand, ContextDecisionReceipt, DecisionFallback, DecisionSource, DecisionWorkflow,
};

#[test]
fn context_decision_receipt_round_trips_without_content() {
    let receipt = ContextDecisionReceipt::new(
        "req-1",
        DecisionWorkflow::ToolOutputRetention,
        DecisionSource::Local,
        ConfidenceBand::High,
        14,
    )
    .with_digests(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        None,
    )
    .with_counts(12, 2)
    .with_fallback(DecisionFallback::NotRequired)
    .with_reason_code("local_policy_match");

    let json = serde_json::to_string(&receipt).unwrap();
    assert!(!json.contains("secret output"));
    assert!(!json.contains("prompt"));
    assert_eq!(
        serde_json::from_str::<ContextDecisionReceipt>(&json).unwrap(),
        receipt
    );
    assert!(receipt.is_valid());
}

#[test]
fn decision_contract_uses_explicit_snake_case_vocabulary() {
    let receipt = ContextDecisionReceipt::new(
        "req-2",
        DecisionWorkflow::RuleSkillSelection,
        DecisionSource::StickyReplay,
        ConfidenceBand::Medium,
        2,
    )
    .with_digests(
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string()),
    )
    .with_fallback(DecisionFallback::JevUnavailable)
    .with_reason_code("jev_unavailable");
    let value: serde_json::Value = serde_json::from_str(&serde_json::to_string(&receipt).unwrap())
        .expect("decision receipt JSON");
    assert_eq!(value["workflow"], "rule_skill_selection");
    assert_eq!(value["source"], "sticky_replay");
    assert_eq!(value["confidence_band"], "medium");
    assert_eq!(value["fallback"], "jev_unavailable");
    assert_eq!(value["schema_version"], 1);
}

#[test]
fn invalid_receipts_fail_closed() {
    let mut receipt = ContextDecisionReceipt::new(
        "req-3",
        DecisionWorkflow::ContextRanking,
        DecisionSource::Baseline,
        ConfidenceBand::Unknown,
        0,
    )
    .with_digests(
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        None,
    )
    .with_reason_code("baseline");
    assert!(receipt.is_valid());

    receipt.schema_version = 2;
    assert!(!receipt.is_valid());
    receipt.schema_version = 1;
    receipt.input_digest.clear();
    assert!(!receipt.is_valid());
}
