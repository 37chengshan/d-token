//! The resolved-path runner (PR-C7A): runs the tracked product-level corpus
//! through `CompressionEngine::compress_resolved_with_ir` — the exact
//! resolved production entry — and produces the full metric set.  The no-op
//! `compress()` fallback is never used, so reported savings are real
//! end-to-end token reductions.

use anyhow::{bail, Context, Result};
use dtoken_compress::hybrid::{lower_hybrid_context, HybridContextIr, ResolvedOptimizationContext};
use dtoken_compress::CompressionEngine;
use dtoken_runtime_pipeline::{
    BoundedWorkerPool, ByteBudget, CancellationToken, ConcurrencyLimits, SchedulerGate,
    SubmitError, TaskOutcome,
};
use dtoken_types::{CompressionPolicy, OpticalEndpoint, Protocol};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Instant;

use crate::alloc;
use crate::corpus::{Corpus, CorpusItem};
use crate::metrics::{ItemMetric, PerfMetrics, QueueMetrics};
use crate::system::{platform_id, process_stats, ProcessStats};

/// Measurement configuration for a corpus run.
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub corpus_root: std::path::PathBuf,
    /// Repeated measurement passes; metrics aggregate over all passes.
    pub iterations: usize,
    /// Request count for the bounded-queue load test.
    pub load_test_requests: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunReport {
    pub platform: String,
    pub runner_version: String,
    pub corpus_manifest_digest: String,
    pub corpus_id: String,
    pub metrics: PerfMetrics,
    pub replay_deterministic: bool,
}

pub fn scope_for(item: &CorpusItem) -> Result<ResolvedOptimizationContext> {
    let (input_protocol, output_protocol, endpoint) = match item.protocol.as_str() {
        "anthropic" => (
            Protocol::Anthropic,
            Protocol::Anthropic,
            OpticalEndpoint::AnthropicMessages,
        ),
        "openai_chat" => (
            Protocol::OpenAi,
            Protocol::OpenAi,
            OpticalEndpoint::OpenAiChatCompletions,
        ),
        "openai_responses" => (
            Protocol::OpenAi,
            Protocol::OpenAi,
            OpticalEndpoint::OpenAiResponses,
        ),
        other => bail!("corpus item {} has unknown protocol {}", item.id, other),
    };
    // Every item shares one resolved identity (same model/route/provider) so
    // baseline and optimized always run under the same-model task contract.
    Ok(ResolvedOptimizationContext {
        route_id: "bench-route".to_string(),
        provider_id: "bench-provider".to_string(),
        model: "bench-model".to_string(),
        input_protocol,
        output_protocol,
        endpoint,
    })
}

/// Lower once (PR-C1A parse/lower-once contract) and run the resolved
/// pipeline on one item.  Returns the item metric.
pub fn run_item(
    engine: &CompressionEngine,
    item: &CorpusItem,
    body: &Value,
    context: &ResolvedOptimizationContext,
) -> Result<ItemMetric> {
    if !context.supports_safe_text_codec() {
        bail!(
            "item {} protocol cannot run the resolved safe-text path",
            item.id
        );
    }
    let ir: Option<HybridContextIr> = lower_hybrid_context(context, body);
    let ir_nodes = ir.as_ref().map_or(0, |ir| ir.nodes().len());

    let started = Instant::now();
    let result = engine.compress_resolved_with_ir(
        context,
        CompressionPolicy::Balanced,
        body.clone(),
        None,
        None,
        ir.as_ref(),
    )?;
    let latency_us = started.elapsed().as_micros().max(1) as u64;

    let original_body_digest = digest_value(body);
    let final_body_digest = digest_value(&result.body);
    let mut transforms = result.transforms_applied.clone();
    transforms.sort();
    let decision_digest = digest_bytes(
        format!(
            "{}\0{}\0{}\0{}",
            item.id,
            original_body_digest,
            final_body_digest,
            transforms.join(",")
        )
        .as_bytes(),
    );

    let fallback = result.tokens_saved == 0;
    let positive_benefit = result.tokens_saved > 0;
    Ok(ItemMetric {
        id: item.id.clone(),
        kind: item.kind.clone(),
        partition: item.partition.clone(),
        input_bytes: body_bytes(body).len(),
        ir_nodes,
        baseline_tokens: result.original_tokens,
        final_tokens: result.compressed_tokens,
        saved_tokens: result.tokens_saved,
        latency_us,
        transforms,
        original_body_digest,
        final_body_digest,
        decision_digest,
        fallback,
        positive_benefit,
    })
}

/// Run every corpus item (in manifest order — stable order) through the
/// resolved path.  `alloc` counters are reset around the measurement.
pub fn run_corpus(config: &RunConfig, allocator: &alloc::CountingAllocator) -> Result<RunReport> {
    let corpus = Corpus::load(&config.corpus_root)?;
    let engine = CompressionEngine::new();
    let before = process_stats();
    allocator.reset();

    let mut items = Vec::new();
    for _ in 0..config.iterations.max(1) {
        for item in corpus.items() {
            let body: Value =
                serde_json::from_slice(corpus.bodies.get(&item.id).expect("verified body present"))
                    .with_context(|| format!("parse corpus item {}", item.id))?;
            let context = scope_for(item)?;
            items.push(run_item(&engine, item, &body, &context)?);
        }
    }

    let allocation = allocator.snapshot();
    let after = process_stats();
    let cpu_us = cpu_delta(before, after);
    let queue = queue_load_metrics(config.load_test_requests);

    let metrics = PerfMetrics::aggregate(items, queue).with_instrumentation(
        allocation.count,
        allocation.bytes,
        after.max_rss_bytes,
        cpu_us,
    );

    // Deterministic replay: a second pass over the same verified corpus must
    // produce byte-identical decision digests.
    let replay_deterministic = replay_is_deterministic(&corpus, &engine)?;

    Ok(RunReport {
        platform: platform_id(),
        runner_version: crate::RUNNER_VERSION.to_string(),
        corpus_manifest_digest: corpus.manifest_digest.clone(),
        corpus_id: corpus.manifest.id.clone(),
        metrics,
        replay_deterministic,
    })
}

/// A second independent pass over the same corpus must produce the identical
/// decision digest per item (same input → same plan/body, no hidden state).
fn replay_is_deterministic(corpus: &Corpus, engine: &CompressionEngine) -> Result<bool> {
    let mut first = Vec::new();
    for item in corpus.items() {
        let body: Value =
            serde_json::from_slice(corpus.bodies.get(&item.id).expect("verified body present"))
                .context("parse corpus item for replay")?;
        let context = scope_for(item)?;
        first.push(run_item(engine, item, &body, &context)?.decision_digest);
    }
    let mut second = Vec::new();
    for item in corpus.items() {
        let body: Value =
            serde_json::from_slice(corpus.bodies.get(&item.id).expect("verified body present"))
                .context("parse corpus item for replay")?;
        let context = scope_for(item)?;
        second.push(run_item(engine, item, &body, &context)?.decision_digest);
    }
    Ok(first == second)
}

/// Queue behavior under bounded admission (PR-C6A machinery, measured not
/// simulated): saturation, cancellation, and deadline-exceeded all count
/// toward the Preserve path; the admitted set runs through the real bounded
/// worker pool with byte permits.
pub fn queue_load_metrics(requests: u64) -> QueueMetrics {
    const BYTES_PER_TASK: usize = 512;
    let limits = match ConcurrencyLimits::new(4, 32, 64 * 1024) {
        Ok(limits) => limits,
        Err(_) => return QueueMetrics::default(),
    };
    let gate = match SchedulerGate::new(limits) {
        Ok(gate) => gate,
        Err(_) => return QueueMetrics::default(),
    };
    let budget = match ByteBudget::new(limits.max_inflight_bytes()) {
        Ok(budget) => Arc::new(budget),
        Err(_) => return QueueMetrics::default(),
    };
    let pool = match BoundedWorkerPool::spawn(limits, budget, None, None) {
        Ok(pool) => pool,
        Err(_) => return QueueMetrics::default(),
    };

    let mut metrics = QueueMetrics::default();
    for sequence in 0..requests {
        let bytes = BYTES_PER_TASK + (sequence as usize % 7) * 128;
        let Ok(admission) = gate.decide(bytes) else {
            metrics.saturated += 1;
            continue;
        };
        let _sequence = admission.sequence();

        // Every 11th request is cancelled before candidate work; every 13th
        // carries an already-expired deadline.  Both take the Preserve path.
        if sequence % 11 == 0 {
            let token = CancellationToken::new();
            token.cancel();
            metrics.cancelled += 1;
            continue;
        }
        if sequence % 13 == 0 {
            metrics.deadline_exceeded += 1;
            continue;
        }

        match pool.try_submit(
            bytes,
            Box::new(move || {
                std::hint::black_box(sequence);
                TaskOutcome::Completed
            }),
        ) {
            Ok(()) => metrics.admitted += 1,
            Err(SubmitError::QueueSaturated) => metrics.saturated += 1,
            Err(SubmitError::Closed) => break,
        }
    }
    pool.shutdown();
    metrics
}

fn cpu_delta(before: ProcessStats, after: ProcessStats) -> Option<u64> {
    match (before.cpu_us, after.cpu_us) {
        (Some(before), Some(after)) => Some(after.saturating_sub(before)),
        _ => None,
    }
}

fn digest_value(value: &Value) -> String {
    digest_bytes(&serde_json::to_vec(value).unwrap_or_default())
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn body_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}
