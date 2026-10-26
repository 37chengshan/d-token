use dtoken_context_store::{ContextStore, DecisionProjection, StickyDecisionProjection};
use dtoken_types::{
    ConfidenceBand, ContextDecisionReceipt, DecisionFallback, DecisionSource, DecisionWorkflow,
};
use uuid::Uuid;

fn temp_db() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("dtoken-decision-store-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create test directory");
    root.join("runtime-state.sqlite3")
}

fn receipt(request_id: &str) -> ContextDecisionReceipt {
    ContextDecisionReceipt::new(
        request_id,
        DecisionWorkflow::ToolOutputRetention,
        DecisionSource::Local,
        ConfidenceBand::High,
        14,
    )
    .with_digests(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()),
    )
    .with_counts(12, 2)
    .with_fallback(DecisionFallback::NotRequired)
    .with_reason_code("local_policy_match")
}

#[test]
fn decision_projection_round_trips_across_restart_without_raw_content() {
    let path = temp_db();
    let projection = DecisionProjection::from_receipt(&receipt("req-1"), 42).unwrap();
    {
        let store = ContextStore::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), 8);
        store.record_decision(&projection).unwrap();
        // A retry is idempotent and does not create a duplicate row.
        store.record_decision(&projection).unwrap();
        assert_eq!(
            store.decision_for("req-1", projection.workflow).unwrap(),
            Some(projection)
        );
    }
    let store = ContextStore::open(&path).unwrap();
    let restored = store
        .decision_for("req-1", DecisionWorkflow::ToolOutputRetention)
        .unwrap()
        .unwrap();
    assert_eq!(
        restored.input_digest,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        restored.candidate_digest.as_deref(),
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    let raw: String = connection
        .query_row(
            "SELECT request_id || workflow || input_digest || COALESCE(candidate_digest, '') || reason_code
             FROM decision_receipts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!raw.contains("secret output"));
    assert!(!raw.contains("prompt body"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn sticky_decision_is_lease_bound_and_expiry_is_purgeable() {
    let path = temp_db();
    let store = ContextStore::open(&path).unwrap();
    let projection = StickyDecisionProjection {
        scope: "project-a:session-a".to_string(),
        workflow: DecisionWorkflow::ArrivalSpanRetention,
        source_digest: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        decision_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_string(),
        expires_at_ms: 200,
        created_at_ms: 100,
    };
    store.put_sticky_decision(&projection).unwrap();
    assert_eq!(
        store
            .get_live_sticky_decision(
                "project-a:session-a",
                DecisionWorkflow::ArrivalSpanRetention,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                150,
            )
            .unwrap(),
        Some(projection.clone())
    );
    assert!(store
        .get_live_sticky_decision(
            "project-a:session-a",
            DecisionWorkflow::ArrivalSpanRetention,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            200,
        )
        .unwrap()
        .is_none());
    assert_eq!(store.purge_expired_sticky_decisions(200).unwrap(), 1);
    assert!(store
        .get_live_sticky_decision(
            "project-a:session-a",
            DecisionWorkflow::ArrivalSpanRetention,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            201,
        )
        .unwrap()
        .is_none());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn invalid_decision_projection_is_rejected_before_sqlite_write() {
    let path = temp_db();
    let store = ContextStore::open(&path).unwrap();
    let mut projection = DecisionProjection::from_receipt(&receipt("req-2"), 42).unwrap();
    projection.input_digest.clear();
    assert!(store.record_decision(&projection).is_err());
    let mut sticky = StickyDecisionProjection {
        scope: "scope".to_string(),
        workflow: DecisionWorkflow::ContextRanking,
        source_digest: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            .to_string(),
        decision_digest: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
            .to_string(),
        expires_at_ms: 10,
        created_at_ms: 10,
    };
    assert!(store.put_sticky_decision(&sticky).is_err());
    sticky.expires_at_ms = 11;
    assert!(store.put_sticky_decision(&sticky).is_ok());
    drop(store);
    let _ = std::fs::remove_file(path);
}
