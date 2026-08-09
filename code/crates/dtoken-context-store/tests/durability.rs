//! PR-C6B projection rebuild: the JSONL journal is the durable source of
//! truth, the SQLite projections are always rebuildable views of it, and a
//! crash mid-rebuild leaves either the old view or the new view — never a
//! partial one (unique safe terminal state across JSONL + SQLite).

use dtoken_context_store::{
    CcrMetadataProjection, ContextStore, RequestProjection,
};
use dtoken_event_ledger::JsonlStore;
use uuid::Uuid;

fn temp_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("dtoken-store-{name}-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create isolated test directory");
    root
}

fn request_projection(request_id: &str, status: &str, observed_at_ms: u64) -> RequestProjection {
    RequestProjection {
        request_id: request_id.to_string(),
        route_id: "route-a".to_string(),
        provider_id: "provider-a".to_string(),
        agent_id: Some("claude-code".to_string()),
        project_id: None,
        status: status.to_string(),
        input_units: Some(10),
        output_units: Some(4),
        saved_units: Some(2),
        observed_at_ms,
    }
}

fn ccr_projection(
    recovery_ref: &str,
    request_id: &str,
    state: &str,
    updated_at_ms: u64,
) -> CcrMetadataProjection {
    CcrMetadataProjection {
        recovery_ref: recovery_ref.to_string(),
        request_id: request_id.to_string(),
        expires_at_ms: Some(updated_at_ms + 1_000),
        plaintext_bytes: Some(4096),
        state: state.to_string(),
        updated_at_ms,
    }
}

#[test]
fn request_projection_rebuilds_from_journal_after_wiping_the_view() {
    let root = temp_root("request-rebuild");
    let db_path = root.join("runtime-state.sqlite3");
    let journal_path = root.join("request-journal.jsonl");
    let journal = JsonlStore::new(&journal_path);

    let mut store = ContextStore::open(&db_path).expect("open store");
    for request_id in ["req-1", "req-2", "req-3"] {
        journal
            .append(&request_projection(request_id, "ok", 10))
            .expect("journal append");
    }
    // Journal is the durable source of truth; rebuild the disposable view.
    store
        .rebuild_request_projections_from_journal(&journal)
        .expect("rebuild projections from journal");
    assert_eq!(
        store.projection_counts().expect("counts").0,
        3,
        "all journaled requests must be projected"
    );

    // A later rebuild from a stale/partial journal replaces the view
    // atomically — it never merges.
    let journal = JsonlStore::new(&journal_path);
    store
        .rebuild_request_projections_from_journal(&journal)
        .expect("re-rebuild");
    assert_eq!(store.projection_counts().expect("counts").0, 3);

    drop(store);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn ccr_projection_rebuild_keeps_latest_state_per_recovery_reference() {
    let root = temp_root("ccr-rebuild");
    let db_path = root.join("runtime-state.sqlite3");
    let journal_path = root.join("ccr-journal.jsonl");
    let journal = JsonlStore::new(&journal_path);

    let mut store = ContextStore::open(&db_path).expect("open store");
    // Append-only journal holds the lifecycle: prepared → send-committed.
    journal
        .append(&ccr_projection("ccr:one", "req-1", "prepared", 10))
        .expect("journal append");
    journal
        .append(&ccr_projection("ccr:one", "req-1", "send_committed", 20))
        .expect("journal append");
    journal
        .append(&ccr_projection("ccr:two", "req-2", "send_committed", 30))
        .expect("journal append");

    store
        .rebuild_ccr_metadata_from_journal(&journal)
        .expect("rebuild ccr metadata from journal");
    assert_eq!(
        store
            .ccr_metadata_state("ccr:one")
            .expect("read ccr state")
            .as_deref(),
        Some("send_committed"),
        "latest journal state must win per recovery reference"
    );
    assert_eq!(
        store
            .ccr_metadata_state("ccr:two")
            .expect("read ccr state")
            .as_deref(),
        Some("send_committed")
    );
    assert_eq!(store.projection_counts().expect("counts").1, 2);

    drop(store);
    let _ = std::fs::remove_dir_all(&root);
}

/// A torn journal tail (crash before the record completed) must not poison a
/// rebuild: `visit_all` streams only complete records, so the projection
/// equals the durable prefix, and the next journal append isolates the tail.
#[test]
fn rebuild_ignores_torn_journal_tail_and_recovers_on_next_append() {
    let root = temp_root("torn-journal");
    let db_path = root.join("runtime-state.sqlite3");
    let journal_path = root.join("request-journal.jsonl");
    let journal = JsonlStore::new(&journal_path);

    let mut store = ContextStore::open(&db_path).expect("open store");
    journal
        .append(&request_projection("req-1", "ok", 10))
        .expect("journal append");

    // Crash mid-append: torn tail.
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal");
    file.write_all(b"{\"request_id\":\"req-2\"")
        .expect("inject torn tail");
    file.flush().expect("flush torn tail");
    drop(file);

    store
        .rebuild_request_projections_from_journal(&journal)
        .expect("rebuild must ignore the torn tail");
    assert_eq!(
        store.projection_counts().expect("counts").0,
        1,
        "only complete journal records may be projected"
    );

    journal
        .append(&request_projection("req-3", "ok", 30))
        .expect("append isolates the torn tail");
    store
        .rebuild_request_projections_from_journal(&journal)
        .expect("rebuild after recovery");
    assert_eq!(store.projection_counts().expect("counts").0, 2);

    drop(store);
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(feature = "failpoints")]
mod fault_matrix {
    use super::*;
    use dtoken_event_ledger::failpoints::{arm, clear_all, disarm};

    static FAILPOINT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn crash_before_projection_replace_commit_keeps_the_previous_view() {
        let _guard = FAILPOINT_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        clear_all();
        let root = temp_root("crash-replace");
        let db_path = root.join("runtime-state.sqlite3");
        let journal_path = root.join("request-journal.jsonl");
        let journal = JsonlStore::new(&journal_path);

        let mut store = ContextStore::open(&db_path).expect("open store");
        for request_id in ["req-1", "req-2"] {
            journal
                .append(&request_projection(request_id, "ok", 10))
                .expect("journal append");
        }
        store
            .rebuild_request_projections_from_journal(&journal)
            .expect("initial rebuild");
        assert_eq!(store.projection_counts().expect("counts").0, 2);

        // Journal grows; the rebuild crashes right before commit.
        journal
            .append(&request_projection("req-3", "ok", 30))
            .expect("journal append");
        arm("context_store:crash_before_projection_replace_commit", 1);
        assert!(
            store
                .rebuild_request_projections_from_journal(&journal)
                .is_err(),
            "armed failpoint must abort the rebuild"
        );
        assert_eq!(
            store.projection_counts().expect("counts").0,
            2,
            "a crashed rebuild must leave the previous view intact, never a partial one"
        );

        // Recovery: the same journal rebuilds to the full view.
        store
            .rebuild_request_projections_from_journal(&journal)
            .expect("rebuild after crash");
        assert_eq!(store.projection_counts().expect("counts").0, 3);

        drop(store);
        clear_all();
        disarm("context_store:crash_before_projection_replace_commit");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn crash_before_ccr_replace_commit_keeps_the_previous_view() {
        let _guard = FAILPOINT_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        clear_all();
        let root = temp_root("crash-ccr");
        let db_path = root.join("runtime-state.sqlite3");
        let journal_path = root.join("ccr-journal.jsonl");
        let journal = JsonlStore::new(&journal_path);

        let mut store = ContextStore::open(&db_path).expect("open store");
        journal
            .append(&ccr_projection("ccr:one", "req-1", "send_committed", 10))
            .expect("journal append");
        store
            .rebuild_ccr_metadata_from_journal(&journal)
            .expect("initial rebuild");

        journal
            .append(&ccr_projection("ccr:two", "req-2", "send_committed", 20))
            .expect("journal append");
        arm("context_store:crash_before_ccr_replace_commit", 1);
        assert!(
            store
                .rebuild_ccr_metadata_from_journal(&journal)
                .is_err(),
            "armed failpoint must abort the ccr rebuild"
        );
        assert_eq!(
            store.projection_counts().expect("counts").1,
            1,
            "a crashed ccr rebuild must leave the previous view intact"
        );

        store
            .rebuild_ccr_metadata_from_journal(&journal)
            .expect("rebuild after crash");
        assert_eq!(store.projection_counts().expect("counts").1, 2);

        drop(store);
        clear_all();
        let _ = std::fs::remove_dir_all(&root);
    }
}
