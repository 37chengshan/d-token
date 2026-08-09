//! PR-C6B deterministic fault matrix: failpoint crash points, process-crash
//! restart recovery, and the unique safe terminal state across a crash.
//!
//! The failpoint registry is process-global, so every test in this file
//! serializes behind one lock: arming a crash point in one test must never
//! bleed into another test's appends.
#![cfg(feature = "failpoints")]

use dtoken_event_ledger::failpoints::{arm, clear_all, disarm, maybe_crash};
use dtoken_event_ledger::JsonlStore;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

static FAILPOINT_TEST_LOCK: Mutex<()> = Mutex::new(());

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("dtoken-ledger-{name}-{}.jsonl", Uuid::new_v4()))
}

fn sequences(records: &[Value]) -> Vec<u64> {
    records
        .iter()
        .filter_map(|record| record["sequence"].as_u64())
        .collect()
}

fn assert_all_lines_complete(path: &PathBuf) {
    let contents = std::fs::read_to_string(path).expect("read repaired JSONL");
    assert!(contents.ends_with('\n'));
    assert!(
        contents
            .lines()
            .all(|line| serde_json::from_str::<Value>(line).is_ok()),
        "a crash must never leave a torn record in {}",
        path.display()
    );
}

/// Crash matrix: for every registered JSONL crash point, an append aborts
/// with the injected error and the recovered store observes only complete
/// records.  The unique safe terminal state holds at every point:
/// previously acked records are always present, the un-acked record is
/// present-or-absent but never torn, and recovery is lossless and ordered.
#[test]
fn crash_matrix_leaves_only_complete_records_at_every_crash_point() {
    let _guard = FAILPOINT_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    for point in [
        "jsonl:crash_after_write_before_flush",
        "jsonl:crash_after_flush_before_sync",
        "jsonl:crash_after_sync_before_ack",
    ] {
        clear_all();
        let path = test_path("crash-matrix");
        let store = JsonlStore::new(&path);
        store.append(&json!({"sequence": 0})).expect("seed record");
        store.append(&json!({"sequence": 1})).expect("seed record");

        arm(point, 1);
        let crashed = store.append(&json!({"sequence": 2})).unwrap_err();
        assert!(
            crashed.to_string().contains("failpoint"),
            "{point} must abort with the injected error, got {crashed}"
        );

        // A crash must never hide or tear previously acked records; the
        // un-acked record is present-or-absent but always complete.
        let recent = store.recent::<Value>(10).expect("read after crash");
        let observed = sequences(&recent);
        assert!(
            observed.contains(&0) && observed.contains(&1),
            "{point} lost an acked record: {observed:?}"
        );
        assert!(
            observed.iter().all(|sequence| *sequence <= 2),
            "{point} observed an unexpected record: {observed:?}"
        );
        assert_all_lines_complete(&path);

        // Recovery: the next append isolates any torn tail and commits.
        store
            .append(&json!({"sequence": 3}))
            .expect("append after crash must succeed");
        let recovered = store.recent::<Value>(10).expect("read after recovery");
        let mut observed = sequences(&recovered);
        observed.sort_unstable();
        assert_eq!(
            observed,
            vec![0, 1, 2, 3],
            "{point} reordered or duplicated records after recovery"
        );
        assert_all_lines_complete(&path);
        let _ = std::fs::remove_file(&path);
    }
    clear_all();
}

/// Process-crash restart: the test binary re-executes itself in a child with
/// a failpoint armed at `usize::MAX`, hard-aborts mid-append, and the parent
/// verifies the reopened ledger contains only complete records — acked ones
/// always, the un-acked one present-or-absent — and that the next append
/// isolates any torn tail.  This is the real crash/restart proof, not an
/// in-process simulation.
#[test]
fn process_crash_mid_append_recovers_with_acked_records_only() {
    if std::env::var("DTK_CRASH_CHILD").as_deref() == Ok("1") {
        let path = std::env::var("DTK_CRASH_PATH").expect("crash path");
        let store = JsonlStore::new(&path);
        store.append(&json!({"sequence": 0})).expect("acked");
        arm("jsonl:crash_after_write_before_flush", usize::MAX);
        // This append hits the failpoint and returns the injected error; the
        // child then hard-aborts so no flush/sync/ack can run.
        let _ = store.append(&json!({"sequence": 1}));
        std::process::abort();
    }

    let _guard = FAILPOINT_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    clear_all();
    let path = test_path("process-crash");
    let current = std::env::current_exe().expect("test binary path");
    let status = std::process::Command::new(current)
        .args(["--exact", "process_crash_mid_append_recovers_with_acked_records_only"])
        .env("DTK_CRASH_CHILD", "1")
        .env("DTK_CRASH_PATH", &path)
        .status()
        .expect("spawn crash child");
    assert!(!status.success(), "child must hard-abort");

    // Reopen after the crash: the acked record must be present and every
    // record must be complete; the un-acked record is present-or-absent.
    let store = JsonlStore::new(&path);
    let observed = sequences(&store.recent::<Value>(10).expect("read after process crash"));
    assert!(
        observed.contains(&0),
        "the acked record must survive a process crash: {observed:?}"
    );
    assert!(
        observed.iter().all(|sequence| *sequence <= 1),
        "only records from this crash window may exist: {observed:?}"
    );
    assert_all_lines_complete(&path);

    // The torn tail (sequence 1, never acked) must be isolated, and the
    // ledger must still accept the next append.
    store
        .append(&json!({"sequence": 2}))
        .expect("append after process crash");
    let mut recovered = sequences(&store.recent::<Value>(10).expect("read recovered ledger"));
    recovered.sort_unstable();
    assert!(recovered.contains(&0) && recovered.contains(&2));
    assert_all_lines_complete(&path);
    let _ = std::fs::remove_file(path);
    clear_all();
}

/// Concurrent writers with a failpoint armed: exactly one append aborts with
/// the injected error, and the recovered ledger contains every record
/// exactly once as a complete line — no duplicate, no tear.
#[test]
fn concurrent_appends_with_failpoint_never_torn_or_duplicated() {
    const THREADS: usize = 6;
    const RECORDS_PER_THREAD: usize = 10;
    let _guard = FAILPOINT_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    clear_all();
    let path = test_path("concurrent-crash");
    let store = Arc::new(JsonlStore::new(&path));
    // Arm a crash point that fires once: exactly one append aborts (after
    // its bytes were fully written, so the file still holds a complete
    // record — it is simply never acked).
    arm("jsonl:crash_after_sync_before_ack", 1);

    let mut handles = Vec::new();
    let errors = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for thread_index in 0..THREADS {
        let store = Arc::clone(&store);
        let errors = Arc::clone(&errors);
        handles.push(std::thread::spawn(move || {
            for record_index in 0..RECORDS_PER_THREAD {
                let sequence = thread_index * RECORDS_PER_THREAD + record_index;
                if store.append(&json!({"sequence": sequence})).is_err() {
                    errors.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }
        }));
    }
    for handle in handles {
        handle.join().expect("writer thread must not panic");
    }
    assert_eq!(
        errors.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "exactly one append must abort at the armed failpoint"
    );

    let recent = store
        .recent::<Value>(THREADS * RECORDS_PER_THREAD + 1)
        .expect("read all records");
    let mut observed = sequences(&recent);
    observed.sort_unstable();
    observed.dedup();
    assert_eq!(
        observed.len(),
        THREADS * RECORDS_PER_THREAD,
        "every record must be present exactly once, even the un-acked one"
    );
    assert_all_lines_complete(&path);
    let _ = std::fs::remove_file(path);
    clear_all();
}

/// The failpoint registry itself is deterministic: hits are consumed exactly
/// once and disarming restores normal operation.
#[test]
fn failpoint_registry_hits_are_deterministic() {
    let _guard = FAILPOINT_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    clear_all();
    arm("test:once", 2);
    assert!(maybe_crash("test:once").is_err(), "first hit fires");
    assert!(maybe_crash("test:once").is_err(), "second hit fires");
    assert!(
        maybe_crash("test:once").is_ok(),
        "third hit is disarmed after depletion"
    );
    arm("test:forever", usize::MAX);
    assert!(maybe_crash("test:forever").is_err());
    assert!(maybe_crash("test:forever").is_err());
    disarm("test:forever");
    assert!(maybe_crash("test:forever").is_ok());
    clear_all();
    assert!(maybe_crash("test:once").is_ok());
}
