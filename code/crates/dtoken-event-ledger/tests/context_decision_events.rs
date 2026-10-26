use dtoken_event_ledger::{ContextDecisionRecorded, EventLedger, JsonlDecisionLedger};
use dtoken_types::{
    ConfidenceBand, ContextDecisionReceipt, DecisionFallback, DecisionSource, DecisionWorkflow,
    ValidationStatus,
};
use uuid::Uuid;

fn receipt() -> ContextDecisionReceipt {
    ContextDecisionReceipt::new(
        "req-1",
        DecisionWorkflow::ToolOutputRetention,
        DecisionSource::Jev,
        ConfidenceBand::High,
        14,
    )
    .with_digests(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
    )
    .with_counts(12, 2)
    .with_fallback(DecisionFallback::NotRequired)
    .with_reason_code("jev_selected")
}

#[test]
fn decision_event_is_redacted_and_round_trips() {
    let event = ContextDecisionRecorded::from_receipt(&receipt())
        .with_validation_status(ValidationStatus::Passed);
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains(r#""workflow":"tool_output_retention""#));
    assert!(json.contains(r#""source":"jev""#));
    assert!(!json.contains("sentinel raw tool output"));
    assert!(!json.contains("prompt body"));
    assert_eq!(
        serde_json::from_str::<ContextDecisionRecorded>(&json).unwrap(),
        event
    );
    assert!(event.validate());
}

#[test]
fn event_ledger_rejects_invalid_decision_and_keeps_legacy_events_separate() {
    let mut ledger = EventLedger::new();
    let event = ContextDecisionRecorded::from_receipt(&receipt());
    ledger.record_context_decision(event.clone()).unwrap();
    assert_eq!(ledger.recent_context_decisions(1), vec![event]);

    let mut invalid = ContextDecisionRecorded::from_receipt(&receipt());
    invalid.input_digest = "raw tool output".to_string();
    assert!(ledger.record_context_decision(invalid).is_err());
    assert!(ledger.recent(1).is_empty());
}

#[test]
fn decision_jsonl_survives_restart_without_mixing_legacy_stream() {
    let path =
        std::env::temp_dir().join(format!("dtoken-decision-ledger-{}.jsonl", Uuid::new_v4()));
    let event = ContextDecisionRecorded::from_receipt(&receipt());
    {
        let ledger = JsonlDecisionLedger::new(&path);
        ledger.append(&event).unwrap();
    }
    let ledger = JsonlDecisionLedger::new(&path);
    assert_eq!(ledger.recent(10).unwrap(), vec![event]);
    std::fs::remove_file(path).unwrap();
}
