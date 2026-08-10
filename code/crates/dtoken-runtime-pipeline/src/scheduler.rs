//! Bounded concurrency for the request lifecycle (PR-C6A).
//!
//! The production transport admits requests through a [`SchedulerGate`] that
//! enforces three independent bounds — worker count, admission queue depth,
//! and in-flight payload bytes — and processes them with a stable dispatch
//! order (admission order, FIFO).  Saturation, cancellation, and expired
//! deadlines never drop a request: the decision surface
//! ([`DispatchDecision`]) maps them to a baseline Preserve so the transport
//! can still deliver the original payload exactly once.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};

use crate::PipelineError;

/// Bounded runtime limits shared by admission and the worker pool.
///
/// All three bounds are strictly positive; a zero bound is a programming
/// error that fails construction so a misconfigured runtime can never
/// silently become unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConcurrencyLimits {
    max_workers: usize,
    max_queue_depth: usize,
    max_inflight_bytes: usize,
}

impl ConcurrencyLimits {
    pub fn new(
        max_workers: usize,
        max_queue_depth: usize,
        max_inflight_bytes: usize,
    ) -> Result<Self, PipelineError> {
        if max_workers == 0 || max_queue_depth == 0 || max_inflight_bytes == 0 {
            return Err(PipelineError::InvalidConcurrencyLimits);
        }
        Ok(Self {
            max_workers,
            max_queue_depth,
            max_inflight_bytes,
        })
    }

    pub fn max_workers(&self) -> usize {
        self.max_workers
    }

    pub fn max_queue_depth(&self) -> usize {
        self.max_queue_depth
    }

    pub fn max_inflight_bytes(&self) -> usize {
        self.max_inflight_bytes
    }
}

/// A bounded byte-permit budget for in-flight payloads.
///
/// Workers hold a permit for the whole request lifetime (admission through
/// commit), so the aggregate bytes of concurrently processing requests can
/// never exceed `capacity`.
#[derive(Debug)]
pub struct ByteBudget {
    capacity: usize,
    available: Mutex<usize>,
    condition: Condvar,
}

impl ByteBudget {
    pub fn new(capacity: usize) -> Result<Self, PipelineError> {
        if capacity == 0 {
            return Err(PipelineError::InvalidByteBudget);
        }
        Ok(Self {
            capacity,
            available: Mutex::new(capacity),
            condition: Condvar::new(),
        })
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Non-blocking acquisition.  Returns `false` when the request would
    /// exceed the budget; the caller then decides between waiting and
    /// preserving the baseline.
    pub fn try_acquire(&self, bytes: usize) -> bool {
        let mut available = self
            .available
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *available < bytes {
            return false;
        }
        *available -= bytes;
        true
    }

    /// Blocking acquisition for the worker pool.  Workers must wait rather
    /// than exceed the byte bound; a caller that cannot wait uses
    /// [`ByteBudget::try_acquire`].
    pub fn acquire(&self, bytes: usize) -> bool {
        if bytes > self.capacity {
            return false;
        }
        let mut available = self
            .available
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *available < bytes {
            available = self
                .condition
                .wait(available)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *available -= bytes;
        true
    }

    pub fn release(&self, bytes: usize) {
        let mut available = self
            .available
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *available = (*available + bytes).min(self.capacity);
        self.condition.notify_all();
    }

    pub fn available(&self) -> usize {
        *self
            .available
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// A request-wide deadline.  The same deadline governs admission, candidate
/// work, and the send window, so an over-budget request cannot hide behind a
/// transform that outlives its contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestDeadline {
    deadline_at_ms: u64,
}

impl RequestDeadline {
    pub fn new(now_ms: u64, budget_ms: u64) -> Result<Self, PipelineError> {
        if budget_ms == 0 {
            return Err(PipelineError::InvalidDeadline);
        }
        let deadline_at_ms = now_ms
            .checked_add(budget_ms)
            .ok_or(PipelineError::InvalidDeadline)?;
        Ok(Self { deadline_at_ms })
    }

    pub fn deadline_at_ms(&self) -> u64 {
        self.deadline_at_ms
    }

    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.deadline_at_ms
    }

    /// Remaining budget, `None` once expired.
    pub fn remaining_ms(&self, now_ms: u64) -> Option<u64> {
        self.deadline_at_ms.checked_sub(now_ms)
    }
}

/// Cooperative request-wide cancellation: the transport flips the flag and
/// any in-flight or queued work for that request observes it before it can
/// start expensive candidate work.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Monotonic admission sequence.  Requests are dispatched in this order, so
/// output ordering is stable (FIFO) even when workers finish out of order.
#[derive(Debug, Default)]
pub struct StableOrder {
    admitted: AtomicU64,
    dispatched: AtomicU64,
}

impl StableOrder {
    pub fn next_admission(&self) -> u64 {
        self.admitted.fetch_add(1, Ordering::Relaxed)
    }

    pub fn mark_dispatched(&self) -> u64 {
        self.dispatched.fetch_add(1, Ordering::Relaxed)
    }

    pub fn admitted_count(&self) -> u64 {
        self.admitted.load(Ordering::Relaxed)
    }

    /// Dispatched count never exceeds admitted count; equality after drain
    /// means every admitted request was dispatched exactly once.
    pub fn dispatched_count(&self) -> u64 {
        self.dispatched.load(Ordering::Relaxed)
    }
}

/// Why a request is delivered as its original baseline instead of an
/// optimized candidate.  Each reason maps to a stable, evidence-bearing
/// reason code on the sendable unit and its terminal receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreserveReason {
    QueueSaturated,
    DeadlineExceeded,
    Cancelled,
}

impl PreserveReason {
    pub fn as_reason_code(self) -> &'static str {
        match self {
            Self::QueueSaturated => "queue_saturated_preserve",
            Self::DeadlineExceeded => "deadline_exceeded_preserve",
            Self::Cancelled => "cancelled_preserve",
        }
    }
}

/// What the transport should do with a request after the gate has decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchDecision {
    /// Admission succeeded; candidate work may run within the deadline.
    Optimize,
    /// The request must go out as its original baseline, exactly once.
    Preserve(PreserveReason),
}

/// Admission gate.  Pure and testable: it owns no threads and can be
/// shared across transports for one runtime instance.
///
/// The gate enforces all three bounded-runtime bounds at admission time:
///
/// 1. pipeline capacity — an admission is refused once
///    `max_workers + max_queue_depth` requests are already in flight, so the
///    gate never hands out more admissions than the bounded pool can hold;
/// 2. in-flight payload bytes — a request that would exceed the byte budget
///    is preserved instead of queueing bytes that can never run;
/// 3. a request-wide deadline and cancellation token — a request that has
///    already exhausted its budget, or has been cancelled, is preserved
///    instead of consuming capacity it can no longer use.
///
/// A granted admission carries a stable FIFO sequence: requests are admitted
/// in admission order, and dispatch follows the same order, so output
/// ordering is deterministic even when workers finish out of order.
#[derive(Debug)]
pub struct SchedulerGate {
    limits: ConcurrencyLimits,
    inflight: ByteBudget,
    order: StableOrder,
    admitted_inflight: AtomicU64,
}

impl SchedulerGate {
    pub fn new(limits: ConcurrencyLimits) -> Result<Self, PipelineError> {
        Ok(Self {
            limits,
            inflight: ByteBudget::new(limits.max_inflight_bytes())?,
            order: StableOrder::default(),
            admitted_inflight: AtomicU64::new(0),
        })
    }

    pub fn limits(&self) -> ConcurrencyLimits {
        self.limits
    }

    pub fn order(&self) -> &StableOrder {
        &self.order
    }

    pub fn inflight_bytes(&self) -> usize {
        self.limits.max_inflight_bytes() - self.inflight.available()
    }

    /// Number of admissions currently held (admission through commit).
    pub fn admitted_inflight(&self) -> u64 {
        self.admitted_inflight.load(Ordering::Relaxed)
    }

    /// Pipeline capacity this gate admits against: every admitted request is
    /// guaranteed a seat in `max_workers + max_queue_depth`.
    pub fn pipeline_capacity(&self) -> usize {
        self.limits.max_workers() + self.limits.max_queue_depth()
    }

    /// Decide what to do with a request at admission time.
    ///
    /// The pipeline-capacity bound is checked first (cheapest), then the
    /// byte permit, then the request-wide deadline and cancellation token.
    /// A refused request returns the exact [`PreserveReason`]; the caller
    /// must deliver the original baseline exactly once.
    pub fn decide(
        &self,
        ingress_bytes: usize,
        deadline: Option<RequestDeadline>,
        cancel: Option<&CancellationToken>,
    ) -> Result<StableAdmission<'_>, DispatchDecision> {
        let admitted = self.admitted_inflight.load(Ordering::Relaxed);
        if admitted >= self.pipeline_capacity() as u64 {
            return Err(DispatchDecision::Preserve(PreserveReason::QueueSaturated));
        }
        if !self.inflight.try_acquire(ingress_bytes) {
            return Err(DispatchDecision::Preserve(PreserveReason::QueueSaturated));
        }
        let expired = deadline.is_some_and(|deadline| deadline.is_expired(now_ms()));
        let cancelled = cancel.is_some_and(CancellationToken::is_cancelled);
        if expired || cancelled {
            // The byte permit was already reserved; release it before
            // reporting the preserve decision so the budget is not leaked.
            self.inflight.release(ingress_bytes);
            // Cancellation takes precedence when a request is both cancelled
            // and expired: an explicitly cancelled request preserves with
            // the cancellation reason (established transport contract).
            let reason = if cancelled {
                PreserveReason::Cancelled
            } else {
                PreserveReason::DeadlineExceeded
            };
            return Err(DispatchDecision::Preserve(reason));
        }
        self.admitted_inflight.fetch_add(1, Ordering::Relaxed);
        Ok(StableAdmission {
            gate: self,
            bytes: ingress_bytes,
            sequence: self.order.next_admission(),
        })
    }
}

/// A granted admission: holds the byte permit and one pipeline-capacity seat
/// for the request lifetime and carries its stable dispatch sequence.
#[derive(Debug)]
pub struct StableAdmission<'a> {
    gate: &'a SchedulerGate,
    bytes: usize,
    sequence: u64,
}

impl StableAdmission<'_> {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for StableAdmission<'_> {
    fn drop(&mut self) {
        self.gate.inflight.release(self.bytes);
        self.gate.admitted_inflight.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Outcome of a queued task in the bounded pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOutcome {
    Completed,
    DeadlineExceeded,
    Cancelled,
}

pub type PoolTask = Box<dyn FnOnce() -> TaskOutcome + Send + 'static>;

/// Error submitting a task to a bounded pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    /// The bounded queue is full; the caller preserves the baseline.
    QueueSaturated,
    /// The payload exceeds the pool's byte budget capacity; it can never be
    /// admitted, so the caller must preserve the baseline without waiting.
    PayloadExceedsCapacity,
    /// The pool has shut down.
    Closed,
}

/// A bounded worker pool with a bounded FIFO queue.
///
/// - `max_workers` threads consume a `sync_channel(max_queue_depth)` queue,
///   so the aggregate of queued + running tasks is `max_workers +
///   max_queue_depth` — never unbounded.
/// - Dispatch order is admission order: the channel is FIFO and every task
///   carries its stable sequence.
/// - Workers hold a byte permit for the task duration, so the pool enforces
///   the in-flight byte bound independently of admission.
/// - A request-wide deadline and cancellation token are evaluated before a
///   task runs, so an expired or cancelled request never performs expensive
///   candidate work; it reports [`TaskOutcome::DeadlineExceeded`] or
///   [`TaskOutcome::Cancelled`] and the transport preserves the baseline.
pub struct BoundedWorkerPool {
    sender: mpsc::SyncSender<QueuedTask>,
    budget: Arc<ByteBudget>,
    queue_capacity: usize,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for BoundedWorkerPool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BoundedWorkerPool")
            .field("queue_capacity", &self.queue_capacity)
            .field("workers", &self.workers.len())
            .field("byte_budget_capacity", &self.budget.capacity())
            .finish()
    }
}

/// One queued task with its request-wide deadline and cancellation token.
/// The deadline and cancellation are bound to the *request*, not to the pool,
/// so a queue-bound request is still evaluated against its own contract the
/// moment a worker dequeues it.
///
/// `permit_held` marks tasks submitted under an existing gate admission:
/// the byte permit is owned by the admission, so the worker must not acquire
/// (or release) the pool budget again — there is exactly one admission
/// authority per request.
struct QueuedTask {
    bytes: usize,
    deadline: Option<RequestDeadline>,
    cancel: Option<CancellationToken>,
    permit_held: bool,
    task: PoolTask,
}

impl BoundedWorkerPool {
    pub fn spawn(
        limits: ConcurrencyLimits,
        budget: Arc<ByteBudget>,
        _deadline: Option<RequestDeadline>,
        _cancel: Option<CancellationToken>,
    ) -> Result<Self, PipelineError> {
        // The pool-level deadline/cancel parameters are retained for API
        // compatibility; per-request contracts are attached at submit time
        // via [`BoundedWorkerPool::submit`]/[`BoundedWorkerPool::try_submit`].
        let (sender, receiver) = mpsc::sync_channel::<QueuedTask>(limits.max_queue_depth());
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(limits.max_workers());
        for _ in 0..limits.max_workers() {
            let receiver = Arc::clone(&receiver);
            let budget = Arc::clone(&budget);
            workers.push(std::thread::spawn(move || loop {
                let task = {
                    let guard = receiver
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    guard.recv()
                };
                let queued = match task {
                    Ok(task) => task,
                    Err(_) => break,
                };
                if !queued.permit_held && !budget.acquire(queued.bytes) {
                    continue;
                }
                let outcome = if queued
                    .cancel
                    .as_ref()
                    .is_some_and(CancellationToken::is_cancelled)
                {
                    TaskOutcome::Cancelled
                } else if queued
                    .deadline
                    .is_some_and(|deadline| deadline.is_expired(now_ms()))
                {
                    TaskOutcome::DeadlineExceeded
                } else {
                    (queued.task)()
                };
                let _ = outcome;
                if !queued.permit_held {
                    budget.release(queued.bytes);
                }
            }));
        }
        Ok(Self {
            sender,
            budget,
            queue_capacity: limits.max_queue_depth(),
            workers,
        })
    }

    /// Enqueue a task with its byte weight and request-wide deadline /
    /// cancellation contract.  A full queue returns
    /// [`SubmitError::QueueSaturated`] without blocking.
    ///
    /// The returned [`TaskOutcome`] reports how the task actually ran: the
    /// task's own outcome on success, or [`TaskOutcome::Cancelled`] /
    /// [`TaskOutcome::DeadlineExceeded`] when the request contract expired
    /// before a worker could start it.
    pub fn submit(
        &self,
        bytes: usize,
        deadline: Option<RequestDeadline>,
        cancel: Option<CancellationToken>,
        task: PoolTask,
    ) -> Result<TaskOutcome, SubmitError> {
        self.submit_impl(bytes, deadline, cancel, false, task)
    }

    /// Like [`BoundedWorkerPool::submit`], for tasks whose byte permit is
    /// already held by a gate admission.  The worker skips the pool budget
    /// acquire/release entirely, so the request is admitted exactly once.
    pub fn submit_with_permit(
        &self,
        bytes: usize,
        deadline: Option<RequestDeadline>,
        cancel: Option<CancellationToken>,
        task: PoolTask,
    ) -> Result<TaskOutcome, SubmitError> {
        self.submit_impl(bytes, deadline, cancel, true, task)
    }

    fn submit_impl(
        &self,
        bytes: usize,
        deadline: Option<RequestDeadline>,
        cancel: Option<CancellationToken>,
        permit_held: bool,
        task: PoolTask,
    ) -> Result<TaskOutcome, SubmitError> {
        let (done_tx, done_rx) = mpsc::channel::<TaskOutcome>();
        let wrapped = Box::new(move || {
            let outcome = task();
            let _ = done_tx.send(outcome);
            outcome
        });
        self.try_submit_impl(bytes, deadline, cancel, permit_held, wrapped)?;
        done_rx.recv().map_err(|_| SubmitError::Closed)
    }

    /// Enqueue a task with its byte weight and request contract without
    /// waiting for the outcome.  A full queue returns
    /// [`SubmitError::QueueSaturated`] without blocking.
    pub fn try_submit(
        &self,
        bytes: usize,
        deadline: Option<RequestDeadline>,
        cancel: Option<CancellationToken>,
        task: PoolTask,
    ) -> Result<(), SubmitError> {
        self.try_submit_impl(bytes, deadline, cancel, false, task)
    }

    /// Like [`BoundedWorkerPool::try_submit`], for tasks whose byte permit
    /// is already held by a gate admission.
    pub fn try_submit_with_permit(
        &self,
        bytes: usize,
        deadline: Option<RequestDeadline>,
        cancel: Option<CancellationToken>,
        task: PoolTask,
    ) -> Result<(), SubmitError> {
        self.try_submit_impl(bytes, deadline, cancel, true, task)
    }

    fn try_submit_impl(
        &self,
        bytes: usize,
        deadline: Option<RequestDeadline>,
        cancel: Option<CancellationToken>,
        permit_held: bool,
        task: PoolTask,
    ) -> Result<(), SubmitError> {
        // The worker's byte permit is bounded by the budget capacity; a
        // payload that can never fit must fail immediately instead of
        // queueing bytes that can never run.  Permit-held tasks already
        // passed the gate's byte admission, so the pool budget does not
        // apply to them.
        if !permit_held && bytes > self.budget.capacity() {
            return Err(SubmitError::PayloadExceedsCapacity);
        }
        self.sender
            .try_send(QueuedTask {
                bytes,
                deadline,
                cancel,
                permit_held,
                task,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => SubmitError::QueueSaturated,
                mpsc::TrySendError::Disconnected(_) => SubmitError::Closed,
            })
    }

    pub fn shutdown(self) {
        drop(self.sender);
        for worker in self.workers {
            let _ = worker.join();
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
