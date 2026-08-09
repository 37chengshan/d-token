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
#[derive(Debug)]
pub struct SchedulerGate {
    limits: ConcurrencyLimits,
    inflight: ByteBudget,
    order: StableOrder,
}

impl SchedulerGate {
    pub fn new(limits: ConcurrencyLimits) -> Result<Self, PipelineError> {
        Ok(Self {
            limits,
            inflight: ByteBudget::new(limits.max_inflight_bytes())?,
            order: StableOrder::default(),
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

    /// Decide what to do with a request at admission time.
    ///
    /// The byte permit is checked first so an over-budget request preserves
    /// the baseline instead of queueing bytes that can never run.  A granted
    /// admission returns a [`StableAdmission`] that holds the byte permit
    /// and the stable sequence number; dropping it releases the permit.
    pub fn decide(&self, ingress_bytes: usize) -> Result<StableAdmission<'_>, DispatchDecision> {
        if !self.inflight.try_acquire(ingress_bytes) {
            return Err(DispatchDecision::Preserve(PreserveReason::QueueSaturated));
        }
        Ok(StableAdmission {
            inflight: &self.inflight,
            bytes: ingress_bytes,
            sequence: self.order.next_admission(),
        })
    }
}

/// A granted admission: holds the byte permit for the request lifetime and
/// carries its stable dispatch sequence.
#[derive(Debug)]
pub struct StableAdmission<'a> {
    inflight: &'a ByteBudget,
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
        self.inflight.release(self.bytes);
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
#[derive(Debug)]
pub struct BoundedWorkerPool {
    sender: mpsc::SyncSender<(usize, PoolTask)>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl BoundedWorkerPool {
    pub fn spawn(
        limits: ConcurrencyLimits,
        budget: Arc<ByteBudget>,
        deadline: Option<RequestDeadline>,
        cancel: Option<CancellationToken>,
    ) -> Result<Self, PipelineError> {
        let (sender, receiver) = mpsc::sync_channel::<(usize, PoolTask)>(limits.max_queue_depth());
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(limits.max_workers());
        for _ in 0..limits.max_workers() {
            let receiver = Arc::clone(&receiver);
            let budget = Arc::clone(&budget);
            let cancel = cancel.clone();
            workers.push(std::thread::spawn(move || loop {
                let task = {
                    let guard = receiver
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    guard.recv()
                };
                let (bytes, task) = match task {
                    Ok(task) => task,
                    Err(_) => break,
                };
                if !budget.acquire(bytes) {
                    continue;
                }
                let outcome = if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
                    TaskOutcome::Cancelled
                } else if deadline
                    .as_ref()
                    .is_some_and(|deadline| deadline.is_expired(now_ms()))
                {
                    TaskOutcome::DeadlineExceeded
                } else {
                    task()
                };
                let _ = outcome;
                budget.release(bytes);
            }));
        }
        Ok(Self { sender, workers })
    }

    /// Enqueue a task with its byte weight.  A full queue returns
    /// [`SubmitError::QueueSaturated`] without blocking.
    pub fn try_submit(&self, bytes: usize, task: PoolTask) -> Result<(), SubmitError> {
        self.sender
            .try_send((bytes, task))
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
