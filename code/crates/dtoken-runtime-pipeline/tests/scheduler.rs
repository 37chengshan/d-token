//! PR-C6A contract: bounded workers/queue/byte permits, request-wide
//! deadlines, stable order, cancellation and saturation Preserve.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use dtoken_runtime_pipeline::{
    BoundedWorkerPool, ByteBudget, CancellationToken, ConcurrencyLimits, DispatchDecision,
    PipelineError, PipelineInput, PreserveReason, RequestDeadline, SchedulerGate,
    SentPayloadVariant, StableAdmission, SubmitError, TaskOutcome,
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
fn zero_bounds_are_rejected_so_a_runtime_can_never_be_unbounded() {
    assert_eq!(
        ConcurrencyLimits::new(0, 1, 1024).unwrap_err(),
        PipelineError::InvalidConcurrencyLimits
    );
    assert_eq!(
        ConcurrencyLimits::new(1, 0, 1024).unwrap_err(),
        PipelineError::InvalidConcurrencyLimits
    );
    assert_eq!(
        ConcurrencyLimits::new(1, 1, 0).unwrap_err(),
        PipelineError::InvalidConcurrencyLimits
    );
    assert_eq!(
        ByteBudget::new(0).unwrap_err(),
        PipelineError::InvalidByteBudget
    );
    assert_eq!(
        RequestDeadline::new(100, 0).unwrap_err(),
        PipelineError::InvalidDeadline
    );
    assert!(ConcurrencyLimits::new(2, 4, 64 * 1024).is_ok());
}

#[test]
fn byte_budget_never_exceeds_capacity_and_blocks_until_release() {
    let budget = ByteBudget::new(100).unwrap();
    assert!(budget.try_acquire(60));
    assert!(!budget.try_acquire(60), "over-budget acquire must fail");
    assert!(!budget.try_acquire(50), "remaining 40 < 50");
    assert!(budget.try_acquire(40));
    assert_eq!(budget.available(), 0);

    budget.release(40);
    assert_eq!(budget.available(), 40);
    budget.release(60);
    assert_eq!(
        budget.available(),
        100,
        "release must clamp at capacity, never exceed"
    );

    // Blocking acquire waits for a release.
    let budget = Arc::new(ByteBudget::new(1).unwrap());
    assert!(budget.acquire(1));
    let blocked = {
        let budget = Arc::clone(&budget);
        std::thread::spawn(move || budget.acquire(1))
    };
    std::thread::sleep(std::time::Duration::from_millis(20));
    budget.release(1);
    assert!(blocked.join().unwrap(), "waiter must acquire after release");
}

#[test]
fn gate_admission_is_stable_and_saturation_preserves_the_baseline() {
    let limits = ConcurrencyLimits::new(2, 4, 100).unwrap();
    let gate = SchedulerGate::new(limits).unwrap();

    let first: StableAdmission<'_> = gate.decide(40, None, None).expect("admit within budget");
    let second = gate.decide(30, None, None).expect("admit within budget");
    assert!(
        first.sequence() < second.sequence(),
        "stable admission order"
    );

    // 40 + 30 leaves 30 free; a 40-byte request saturates.
    let saturated = gate.decide(40, None, None).unwrap_err();
    assert_eq!(
        saturated,
        DispatchDecision::Preserve(PreserveReason::QueueSaturated)
    );

    // The preserve decision maps to a baseline Preserve unit, exactly once.
    let ready = PipelineInput::new(scope(), b"original".to_vec(), 32)
        .unwrap()
        .prepare()
        .into_preserved(PreserveReason::QueueSaturated);
    assert_eq!(ready.variant(), SentPayloadVariant::Original);
    assert_eq!(ready.reason_code(), "queue_saturated_preserve");

    // Dropping admissions releases the byte permits.
    drop(second);
    assert_eq!(gate.inflight_bytes(), 40);
    drop(first);
    assert_eq!(gate.inflight_bytes(), 0);
}

#[test]
fn deadline_exceeded_and_cancelled_preserve_the_baseline_once() {
    let deadline = RequestDeadline::new(100, 50).unwrap();
    assert!(!deadline.is_expired(149));
    assert!(deadline.is_expired(150), "deadline boundary is inclusive");

    let ready = PipelineInput::new(scope(), b"original".to_vec(), 32)
        .unwrap()
        .prepare()
        .into_preserved(PreserveReason::DeadlineExceeded);
    assert_eq!(ready.reason_code(), "deadline_exceeded_preserve");

    let token = CancellationToken::new();
    assert!(!token.is_cancelled());
    token.cancel();
    assert!(token.is_cancelled());
    let ready = PipelineInput::new(scope(), b"original".to_vec(), 32)
        .unwrap()
        .prepare()
        .into_preserved(PreserveReason::Cancelled);
    assert_eq!(ready.variant(), SentPayloadVariant::Original);
    assert_eq!(ready.reason_code(), "cancelled_preserve");
}

#[test]
fn bounded_queue_is_fifo_and_saturation_is_reported_not_blocked() {
    let limits = ConcurrencyLimits::new(1, 1, 64 * 1024).unwrap();
    let budget = Arc::new(ByteBudget::new(limits.max_inflight_bytes()).unwrap());
    let pool = BoundedWorkerPool::spawn(limits, budget, None, None).unwrap();

    // One worker + queue depth 1: the first task runs (and blocks), the
    // second fills the queue, the third saturates.
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<u64>();

    let first = {
        let started_tx = started_tx.clone();
        Box::new(move || {
            started_tx.send(1).unwrap();
            let _ = release_rx.recv();
            TaskOutcome::Completed
        }) as Box<dyn FnOnce() -> TaskOutcome + Send>
    };
    pool.try_submit(16, None, None, first).expect("first task admitted");
    started_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("worker started");

    let second = Box::new(|| TaskOutcome::Completed);
    pool.try_submit(16, None, None, second).expect("second task queued");
    let third = Box::new(|| TaskOutcome::Completed);
    assert_eq!(
        pool.try_submit(16, None, None, third).unwrap_err(),
        SubmitError::QueueSaturated,
        "third submit must report saturation without blocking"
    );

    release_tx.send(()).unwrap();
    drop(release_tx);
    pool.shutdown();
}

#[test]
fn single_worker_pool_runs_tasks_in_stable_admission_order() {
    let limits = ConcurrencyLimits::new(1, 32, 64 * 1024).unwrap();
    let budget = Arc::new(ByteBudget::new(limits.max_inflight_bytes()).unwrap());
    let pool = BoundedWorkerPool::spawn(limits, budget, None, None).unwrap();

    let (done_tx, done_rx) = mpsc::channel::<usize>();
    let runs = Arc::new(AtomicUsize::new(0));
    for sequence in 0..8 {
        let done_tx = done_tx.clone();
        let runs = Arc::clone(&runs);
        pool.try_submit(
            8,
            None,
            None,
            Box::new(move || {
                let order = runs.fetch_add(1, Ordering::SeqCst);
                done_tx.send(sequence).unwrap();
                assert_eq!(order, sequence, "FIFO dispatch preserves admission order");
                TaskOutcome::Completed
            }),
        )
        .expect("task admitted");
    }
    drop(done_tx);
    let mut completed = Vec::new();
    while let Ok(sequence) = done_rx.recv() {
        completed.push(sequence);
    }
    assert_eq!(completed, (0..8).collect::<Vec<_>>());
    pool.shutdown();
}

#[test]
fn multi_worker_pool_completes_every_admitted_task_exactly_once() {
    const TASKS: usize = 40;
    let limits = ConcurrencyLimits::new(4, 64, 64 * 1024).unwrap();
    let budget = Arc::new(ByteBudget::new(limits.max_inflight_bytes()).unwrap());
    let pool = BoundedWorkerPool::spawn(limits, budget, None, None).unwrap();

    let (done_tx, done_rx) = mpsc::channel::<usize>();
    for sequence in 0..TASKS {
        let done_tx = done_tx.clone();
        pool.try_submit(
            8,
            None,
            None,
            Box::new(move || {
                done_tx.send(sequence).unwrap();
                TaskOutcome::Completed
            }),
        )
        .expect("task admitted");
    }
    drop(done_tx);
    let mut completed = Vec::new();
    while let Ok(sequence) = done_rx.recv() {
        completed.push(sequence);
    }
    completed.sort_unstable();
    assert_eq!(completed, (0..TASKS).collect::<Vec<_>>());
    pool.shutdown();
}

#[test]
fn byte_permits_cap_concurrency_below_worker_count() {
    // 2 workers but a byte budget that only fits one task at a time: the
    // second task cannot start until the first releases its permit.
    let limits = ConcurrencyLimits::new(2, 8, 1024).unwrap();
    let budget = Arc::new(ByteBudget::new(512).unwrap());
    let pool = BoundedWorkerPool::spawn(limits, budget, None, None).unwrap();

    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<u64>();
    let (second_started_tx, second_started_rx) = mpsc::channel::<()>();

    let first = {
        let started_tx = started_tx.clone();
        Box::new(move || {
            started_tx.send(1).unwrap();
            let _ = release_rx.recv();
            TaskOutcome::Completed
        }) as Box<dyn FnOnce() -> TaskOutcome + Send>
    };
    pool.try_submit(512, None, None, first)
        .expect("first task holds the byte budget");
    started_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("first task running");

    let second = Box::new(move || {
        second_started_tx.send(()).unwrap();
        TaskOutcome::Completed
    });
    pool.try_submit(512, None, None, second).expect("second task queued");

    // The second task must NOT start while the first holds the byte budget.
    assert!(
        second_started_rx
            .recv_timeout(std::time::Duration::from_millis(150))
            .is_err(),
        "second task must wait for the byte permit"
    );
    release_tx.send(()).unwrap();
    second_started_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("second task starts after permit release");
    pool.shutdown();
}

#[test]
fn pool_never_runs_expired_or_cancelled_tasks() {
    // Expired deadline: the wrapper checks before running the closure.
    let limits = ConcurrencyLimits::new(1, 4, 64 * 1024).unwrap();
    let budget = Arc::new(ByteBudget::new(limits.max_inflight_bytes()).unwrap());
    let expired = RequestDeadline::new(1, 1).unwrap(); // deadline in the far past
    let pool = BoundedWorkerPool::spawn(limits, budget, None, None).unwrap();
    let ran = Arc::new(AtomicUsize::new(0));
    let ran_flag = Arc::clone(&ran);
    pool.try_submit(
        8,
        Some(expired),
        None,
        Box::new(move || {
            ran_flag.fetch_add(1, Ordering::SeqCst);
            TaskOutcome::Completed
        }),
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert_eq!(ran.load(Ordering::SeqCst), 0, "expired task must not run");
    pool.shutdown();

    // Cancelled token: same guarantee.
    let token = CancellationToken::new();
    token.cancel();
    let limits = ConcurrencyLimits::new(1, 4, 64 * 1024).unwrap();
    let budget = Arc::new(ByteBudget::new(limits.max_inflight_bytes()).unwrap());
    let pool = BoundedWorkerPool::spawn(limits, budget, None, None).unwrap();
    let ran = Arc::new(AtomicUsize::new(0));
    let ran_flag = Arc::clone(&ran);
    pool.try_submit(
        8,
        None,
        Some(token),
        Box::new(move || {
            ran_flag.fetch_add(1, Ordering::SeqCst);
            TaskOutcome::Completed
        }),
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert_eq!(ran.load(Ordering::SeqCst), 0, "cancelled task must not run");
    pool.shutdown();
}

#[test]
fn concurrent_submission_never_inverts_stable_admission_order() {
    // Two submitters race: the second admission is enqueued strictly after
    // the first, so FIFO-by-sequence must keep dispatch order exactly
    // admission order even when enqueues interleave.
    let limits = ConcurrencyLimits::new(1, 256, 64 * 1024).unwrap();
    let budget = Arc::new(ByteBudget::new(limits.max_inflight_bytes()).unwrap());
    let pool = Arc::new(BoundedWorkerPool::spawn(limits, budget, None, None).unwrap());
    let gate = SchedulerGate::new(limits).unwrap();

    let (done_tx, done_rx) = mpsc::channel::<u64>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let release_rx = std::sync::Arc::new(std::sync::Mutex::new(release_rx));
    let barrier = Arc::new(std::sync::Barrier::new(3));

    let first_admission = gate.decide(8, None, None).expect("first admitted");
    let first_sequence = first_admission.sequence();
    let done_tx_first = done_tx.clone();
    let pool_first = Arc::clone(&pool);
    let release_rx_first = Arc::clone(&release_rx);
    let barrier_first = Arc::clone(&barrier);
    let first_handle = std::thread::spawn(move || {
        barrier_first.wait();
        pool_first
            .submit_with_permit(
                8,
                None,
                None,
                first_sequence,
                Box::new(move || {
                    let guard = release_rx_first
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let _ = guard.recv();
                    drop(guard);
                    done_tx_first.send(first_sequence).unwrap();
                    TaskOutcome::Completed
                }),
            )
            .expect("first task admitted");
    });

    let second_admission = gate.decide(8, None, None).expect("second admitted");
    let second_sequence = second_admission.sequence();
    assert!(first_sequence < second_sequence, "admission order");
    let done_tx_second = done_tx;
    let pool_second = Arc::clone(&pool);
    let barrier_second = Arc::clone(&barrier);
    let second_handle = std::thread::spawn(move || {
        barrier_second.wait();
        // Enqueued strictly after the first: sequence, not enqueue timing,
        // decides dispatch order.
        std::thread::sleep(std::time::Duration::from_millis(30));
        pool_second
            .submit_with_permit(
                8,
                None,
                None,
                second_sequence,
                Box::new(move || {
                    done_tx_second.send(second_sequence).unwrap();
                    TaskOutcome::Completed
                }),
            )
            .expect("second task admitted");
    });

    barrier.wait();
    std::thread::sleep(std::time::Duration::from_millis(80));
    release_tx.send(()).unwrap();
    first_handle.join().unwrap();
    second_handle.join().unwrap();

    let mut completed = Vec::new();
    while let Ok(sequence) = done_rx.recv() {
        completed.push(sequence);
    }
    assert_eq!(
        completed,
        vec![first_sequence, second_sequence],
        "dispatch must follow admission sequence, not enqueue timing"
    );
    Arc::try_unwrap(pool)
        .expect("pool uniquely owned after submitters finish")
        .shutdown();
}

#[test]
fn expired_deadline_rejects_before_transport_within_total_budget() {
    // A request whose deadline has already passed must never reach transport
    // work: the worker reports DeadlineExceeded and the closure never runs.
    // This bounds total request time to one request-wide budget.
    let limits = ConcurrencyLimits::new(1, 4, 64 * 1024).unwrap();
    let budget = Arc::new(ByteBudget::new(limits.max_inflight_bytes()).unwrap());
    let pool = BoundedWorkerPool::spawn(limits, budget, None, None).unwrap();

    let expired = RequestDeadline::new(1, 1).unwrap(); // far in the past
    let ran = Arc::new(AtomicUsize::new(0));
    let ran_flag = Arc::clone(&ran);
    let outcome = pool
        .submit(
            8,
            Some(expired),
            None,
            Box::new(move || {
                ran_flag.fetch_add(1, Ordering::SeqCst);
                TaskOutcome::Completed
            }),
        )
        .expect("expired task admitted to the bounded queue");
    assert_eq!(
        outcome,
        TaskOutcome::DeadlineExceeded,
        "expired request must be preserved before any transport work"
    );
    assert_eq!(
        ran.load(Ordering::SeqCst),
        0,
        "transport work must never run for an expired request"
    );
    pool.shutdown();
}
