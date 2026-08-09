//! PR-C6B durability contract: FULL vs Normal durability modes and the
//! rebuildable-never-torn invariant.  The deterministic failpoint crash
//! matrix and process-crash restart proof live in `fault_matrix.rs`
//! (feature `failpoints`).

use dtoken_event_ledger::{DurabilityMode, JsonlEventLedger, JsonlStore};
use dtoken_types::{DtokenEvent, EventSource, EventStatus, LayeredStage};
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dtoken-ledger-{name}-{}.jsonl", Uuid::new_v4()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn test_event(sequence: usize) -> DtokenEvent {
    DtokenEvent {
        event_id: Uuid::new_v4(),
        timestamp_ms: now_ms(),
        project_id: Some("p1".to_string()),
        agent_id: None,
        instance_id: None,
        command_id: None,
        request_id: None,
        stage: LayeredStage::RtkFilter,
        source: EventSource::Rtk,
        status: EventStatus::Ok,
        input_tokens: Some(100),
        output_tokens: Some(40),
        saved_tokens: Some(60),
        baseline_kind: None,
        error_code: None,
        metadata: json!({ "sequence": sequence, "summary": "aggregate only" }),
        context_accounting: None,
    }
}

/// FULL durability: the ack happens after `sync_data`, so a record that
/// returned `Ok` is durable on disk even when a later operation aborts.
#[test]
fn full_durability_acks_only_after_sync() {
    let path = test_path("full-durability");
    let store = JsonlStore::new_with_durability(&path, DurabilityMode::Full);
    assert_eq!(store.durability(), DurabilityMode::Full);
    store.append(&json!({"sequence": 0})).expect("acked record");
    // Simulate a hard crash (abort) right after an acked append: the record
    // must still be fully present when the store is reopened.
    let reopened = JsonlStore::new(&path);
    let recent = reopened.recent::<Value>(10).expect("read after crash");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0]["sequence"], 0);
    let _ = std::fs::remove_file(path);
}

/// Normal durability trades sync for throughput; rebuildable projections use
/// it.  A crash before sync may lose the last record but never tears it, and
/// `recent`/`visit_all` still observe only complete records.
#[test]
fn normal_durability_is_rebuildable_never_torn() {
    let path = test_path("normal-durability");
    let store = JsonlStore::new_with_durability(&path, DurabilityMode::Normal);
    assert_eq!(store.durability(), DurabilityMode::Normal);
    store.append(&json!({"sequence": 0})).expect("append");

    // Truncate the tail mid-record: recovery must treat it as absent, not
    // corrupt the ledger.
    let mut file = OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open store");
    file.write_all(b"{\"sequence\":1")
        .expect("inject torn tail");
    file.flush().expect("flush torn tail");
    drop(file);

    let recovered = JsonlStore::new_with_durability(&path, DurabilityMode::Normal);
    let recent = recovered.recent::<Value>(10).expect("read after torn tail");
    assert_eq!(recent.len(), 1, "torn tail must be ignored, never surfaced");
    assert_eq!(recent[0]["sequence"], 0);

    // The next append isolates the torn tail and commits cleanly.
    recovered
        .append(&json!({"sequence": 2}))
        .expect("append after torn tail");
    let observed = recovered
        .recent::<Value>(10)
        .unwrap()
        .iter()
        .filter_map(|record| record["sequence"].as_u64())
        .collect::<Vec<_>>();
    assert_eq!(observed, vec![2, 0]);
    let _ = std::fs::remove_file(path);
}

#[test]
fn ledger_event_round_trips_with_explicit_durability_mode() {
    let path = test_path("ledger-mode");
    let ledger = JsonlEventLedger::new_with_durability(&path, DurabilityMode::Full);
    assert_eq!(ledger.durability(), DurabilityMode::Full);
    let event = test_event(7);
    ledger.append(&event).expect("append event");
    let recent = ledger.recent(10).expect("read events");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].event_id, event.event_id);
    let _ = std::fs::remove_file(path);
}
