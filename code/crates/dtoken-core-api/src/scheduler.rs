//! Production transport scheduler wiring (PR-C6A).
//!
//! The HTTP send surfaces (OpenAI Chat, native Responses, Anthropic
//! Messages) admit every routed request through one process-wide
//! [`SchedulerGate`] backed by one bounded worker pool.  The gate is the
//! single admission authority: it enforces pipeline capacity (workers +
//! queue depth), in-flight payload bytes, and a request-wide
//! deadline/cancellation token at admission time, and the held admission is
//! the byte permit for the whole request lifetime (admission through send
//! commit).
//!
//! Candidate work (the resolved compression/plan) runs on the bounded pool
//! *under that same admission* (`submit_with_permit`), so the pool boundary
//! never double-acquires the byte budget or halves the admission capacity.
//! Saturation, cancellation, and expired deadlines never drop a request:
//! they map to an exact baseline Preserve with an evidence-bearing reason
//! code, so the wire body is the original payload delivered exactly once.

use std::sync::{mpsc, Arc, LazyLock};

use dtoken_compress::context_pipeline::ProviderCacheEvidence;
use dtoken_runtime_pipeline::scheduler::PoolTask;
use dtoken_runtime_pipeline::{
    BoundedWorkerPool, ByteBudget, CancellationToken, ConcurrencyLimits, DispatchDecision,
    PreserveReason, RequestDeadline, SchedulerGate, StableAdmission, SubmitError, TaskOutcome,
};

use super::*;

/// Process-wide production bounds.  All strictly positive; a zero bound is a
/// programming error that fails construction so a misconfigured runtime can
/// never silently become unbounded.
const PRODUCTION_MAX_WORKERS: usize = 4;
const PRODUCTION_MAX_QUEUE_DEPTH: usize = 64;
const PRODUCTION_MAX_INFLIGHT_BYTES: usize = 16 * 1024 * 1024;

/// Default request-wide deadline for routed sends (admission → commit).
const DEFAULT_REQUEST_DEADLINE_MS: u64 = 30_000;

/// The shared production scheduler.  Created once per process; the gate and
/// pool are `Sync`, so every transport surface admits through the same
/// bounds.
struct ProductionScheduler {
    gate: SchedulerGate,
    pool: Arc<BoundedWorkerPool>,
}

impl ProductionScheduler {
    fn new() -> Self {
        let limits = ConcurrencyLimits::new(
            PRODUCTION_MAX_WORKERS,
            PRODUCTION_MAX_QUEUE_DEPTH,
            PRODUCTION_MAX_INFLIGHT_BYTES,
        )
        .expect("static production limits must be valid");
        let gate = SchedulerGate::new(limits).expect("static production gate must construct");
        let budget =
            Arc::new(ByteBudget::new(limits.max_inflight_bytes()).expect("static byte budget"));
        let pool =
            BoundedWorkerPool::spawn(limits, budget, None, None).expect("static production pool");
        ProductionScheduler {
            gate,
            pool: Arc::new(pool),
        }
    }
}

static PRODUCTION_SCHEDULER: LazyLock<ProductionScheduler> =
    LazyLock::new(ProductionScheduler::new);

pub(crate) fn production_gate() -> &'static SchedulerGate {
    &PRODUCTION_SCHEDULER.gate
}

pub(crate) fn production_pool() -> &'static BoundedWorkerPool {
    &PRODUCTION_SCHEDULER.pool
}

/// A fresh request-wide deadline for a routed send.
pub(crate) fn request_deadline() -> RequestDeadline {
    RequestDeadline::new(dtoken_event_ledger::now_ms(), DEFAULT_REQUEST_DEADLINE_MS)
        .expect("valid default deadline")
}

/// Outcome of gating one routed request: the compression summary to send,
/// the exact preserve reason when the request could not run candidate work,
/// and the held admission (released when the request commits or is dropped).
pub(crate) struct GatedCompression {
    pub summary: CompressionSummary,
    pub preserve_reason: Option<PreserveReason>,
    /// Held until the send window commits; dropping it releases the byte
    /// permit and the pipeline-capacity seat.
    pub _admission: Option<StableAdmission<'static>>,
    /// Cooperative cancellation for the request lifetime.
    pub cancel: CancellationToken,
    /// The request-wide deadline governing admission, candidate work, and
    /// the upstream send window.  The transport must re-check
    /// [`RequestDeadline::is_expired`] before every upstream send/stream so
    /// total request time stays within one budget.
    pub deadline: RequestDeadline,
}

/// Terminal guard for the upstream send window (PR-C6A security): the
/// request-wide deadline bounds the total request time, so before any
/// upstream send/stream the transport must re-check it.  When expired, the
/// request is NOT forwarded — it terminates with the deadline reason and a
/// `FailedBeforeCommit` send state, keeping total time within one budget.
///
/// Returns `Some(response)` when the deadline expired (caller must return it
/// immediately without touching the transport), `None` otherwise.
pub(crate) fn scheduler_deadline_guard(
    state: &AppState,
    request_id: Uuid,
    deadline: RequestDeadline,
) -> Option<Response> {
    if !deadline.is_expired(dtoken_event_ledger::now_ms()) {
        return None;
    }
    append_stage_event(
        state,
        request_id,
        LayeredStage::ProviderForward,
        EventStatus::Failed,
        json!({
            "reason_code": "deadline_exceeded_preserve",
            "send_state": "failed_before_commit",
            "upstream_send_attempted": false,
            "raw_available": false,
            "privacy": "request-wide deadline expired before the upstream send window; nothing was forwarded"
        }),
        Some("deadline_exceeded_preserve"),
    );
    Some(
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": {
                    "type": "request_timeout",
                    "message": "request-wide deadline expired before the upstream send window"
                },
                "dtoken": {
                    "source": "dtoken-owned-runtime",
                    "request_id": request_id,
                    "reason_code": "deadline_exceeded_preserve",
                    "upstream_send_attempted": false
                }
            })),
        )
            .into_response(),
    )
}

/// A baseline (body untouched, zero transforms) summary carrying the
/// preserve reason as the evidence-bearing fallback reason code.
pub(crate) fn preserved_compression_summary(
    policy: CompressionPolicy,
    body: Value,
    reason: PreserveReason,
) -> CompressionSummary {
    let mut summary = compress_for_route(None, policy, body);
    summary.fallback_reason_code = Some(reason.as_reason_code().to_string());
    summary
}

/// A sealed baseline pipeline payload for a Preserve decision: the exact
/// original body, `SentPayloadVariant::Original`, with the evidence-bearing
/// reason code.
pub(crate) fn preserved_pipeline_payload(
    scope: RequestScope,
    baseline: &Value,
    reason: PreserveReason,
) -> PreparedPipelinePayload {
    let ready = PipelineInput::new(
        scope.clone(),
        serde_json::to_vec(baseline).unwrap_or_default(),
        MAX_REQUEST_BODY_BYTES,
    )
    .map(|input| input.prepare().into_preserved(reason))
    .unwrap_or_else(|_| {
        PipelineInput::new(scope, b"{}".to_vec(), MAX_REQUEST_BODY_BYTES)
            .expect("static JSON baseline is valid")
            .prepare()
            .into_preserved(reason)
    });
    PreparedPipelinePayload {
        ready,
        fallback_reason: Some(reason.as_reason_code()),
    }
}

/// Run the resolved production compression through the scheduler gate.
///
/// Admission is decided first (capacity, byte permit, deadline, cancel);
/// when the gate refuses, the request is preserved as its exact baseline
/// without running any candidate work.  When admitted, the candidate work is
/// dispatched to the bounded worker pool *under the same admission*
/// (`submit_with_permit`): the pool queue is also bounded by the gate's
/// pipeline capacity, and in-flight bytes never exceed the single budget.
pub(crate) async fn run_gated_compression(
    state: &AppState,
    scope: RequestScope,
    context: Option<ResolvedRouteOptimizationContext>,
    policy: CompressionPolicy,
    body: Value,
    cache_evidence: Option<ProviderCacheEvidence>,
    frozen_message_count: Option<usize>,
    ir: Option<dtoken_compress::HybridContextIr>,
    deadline: RequestDeadline,
) -> GatedCompression {
    let bytes = serde_json::to_vec(&body).map(|bytes| bytes.len()).unwrap_or(0);
    let cancel = CancellationToken::new();
    let admission = match production_gate().decide(bytes, Some(deadline), Some(&cancel)) {
        Ok(admission) => admission,
        Err(DispatchDecision::Preserve(reason)) => {
            return GatedCompression {
                summary: preserved_compression_summary(policy, body, reason),
                preserve_reason: Some(reason),
                _admission: None,
                cancel,
                deadline,
            };
        }
        Err(DispatchDecision::Optimize) => unreachable!("gate never returns Optimize"),
    };

    let (summary_tx, summary_rx) = mpsc::channel::<CompressionSummary>();
    let owned_state = state.clone();
    let owned_cancel = cancel.clone();
    let owned_policy = policy.clone();
    let owned_body = body.clone();
    let task: PoolTask = Box::new(move || {
        let summary = compress_for_route_with_relay(
            &owned_state,
            &scope,
            context.as_ref(),
            owned_policy,
            owned_body,
            cache_evidence.as_ref(),
            frozen_message_count,
            ir.as_ref(),
        );
        let _ = summary_tx.send(summary);
        TaskOutcome::Completed
    });

    let admission_sequence = admission.sequence();
    let outcome = tokio::task::spawn_blocking(move || {
        production_pool().submit_with_permit(
            bytes,
            Some(deadline),
            Some(owned_cancel),
            admission_sequence,
            task,
        )
    })
    .await;

    match outcome {
        Ok(Ok(TaskOutcome::Completed)) => {
            let summary = summary_rx
                .recv()
                .unwrap_or_else(|_| preserved_compression_summary(policy, body, PreserveReason::QueueSaturated));
            GatedCompression {
                summary,
                preserve_reason: None,
                _admission: Some(admission),
                cancel,
                deadline,
            }
        }
        Ok(Ok(TaskOutcome::Cancelled)) => {
            let _ = summary_rx.try_recv();
            GatedCompression {
                summary: preserved_compression_summary(policy, body, PreserveReason::Cancelled),
                preserve_reason: Some(PreserveReason::Cancelled),
                _admission: None,
                cancel,
                deadline,
            }
        }
        Ok(Ok(TaskOutcome::DeadlineExceeded)) => {
            let _ = summary_rx.try_recv();
            GatedCompression {
                summary: preserved_compression_summary(policy, body, PreserveReason::DeadlineExceeded),
                preserve_reason: Some(PreserveReason::DeadlineExceeded),
                _admission: None,
                cancel,
                deadline,
            }
        }
        // Queue saturation or a lost worker: the request still goes out
        // exactly once as its baseline, with an evidence-bearing reason.
        Ok(Err(SubmitError::QueueSaturated))
        | Ok(Err(SubmitError::PayloadExceedsCapacity))
        | Ok(Err(SubmitError::Closed))
        | Err(_) => GatedCompression {
            summary: preserved_compression_summary(policy, body, PreserveReason::QueueSaturated),
            preserve_reason: Some(PreserveReason::QueueSaturated),
            _admission: None,
            cancel,
            deadline,
        },
    }
}
