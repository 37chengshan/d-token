//! PR-C7A/B runner contract: percentiles, replay determinism, allocation
//! instrumentation, queue/cancel metrics, paired same-model contract, and
//! honest fallback measurement.

use dtoken_benchmark::alloc::CountingAllocator;
use dtoken_benchmark::baseline::{check_no_regression, BaselineRecord};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

/// The global allocator is process-wide, so corpus runs inside one test
/// binary must serialize or their reset/snapshot pairs interleave.
static BENCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run_serialized(config: RunConfig) -> dtoken_benchmark::RunReport {
    let _guard = BENCH_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    run_corpus(&config, &ALLOC).unwrap()
}
use dtoken_benchmark::metrics::{percentile, PerfMetrics, QueueMetrics};
use dtoken_benchmark::paired::build_paired_contract;
use dtoken_benchmark::run::{queue_load_metrics, run_corpus, RunConfig};
use dtoken_benchmark::Corpus;

fn corpus_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

#[test]
fn percentiles_use_nearest_rank() {
    let values = (1..=100).collect::<Vec<_>>();
    assert_eq!(percentile(values.clone(), 0.50), Some(50));
    assert_eq!(percentile(values.clone(), 0.95), Some(95));
    assert_eq!(percentile(values.clone(), 0.99), Some(99));
    assert_eq!(percentile(Vec::new(), 0.95), None);
    assert_eq!(percentile(values, 1.0), Some(100));
}

#[test]
fn replay_is_deterministic_across_independent_passes() {
        let config = RunConfig {
        corpus_root: corpus_root(),
        iterations: 1,
        load_test_requests: 0,
    };
    let first = run_serialized(config.clone());
    let second = run_serialized(config.clone());
    assert!(first.replay_deterministic);
    assert!(second.replay_deterministic);
    let digests = |report: &dtoken_benchmark::RunReport| {
        report
            .metrics
            .items
            .iter()
            .map(|item| item.decision_digest.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        digests(&first),
        digests(&second),
        "same input must produce identical decision digests"
    );
    assert_eq!(
        first.corpus_manifest_digest,
        second.corpus_manifest_digest,
        "corpus identity must be stable"
    );
}

#[test]
fn allocation_instrumentation_counts_real_work() {
    let config = RunConfig {
        corpus_root: corpus_root(),
        iterations: 1,
        load_test_requests: 0,
    };
    let report = run_serialized(config);
    // The runner resets the counters around the measured work itself.
    assert!(report.metrics.alloc_count > 0, "resolved path must allocate");
    assert!(report.metrics.alloc_bytes > 0);
    assert!(report.metrics.item_count >= 10);
    assert!(
        report.metrics.alloc_count <= 100_000_000,
        "resolved path must not balloon allocations"
    );
}

#[test]
fn queue_load_metrics_are_deterministic_and_honest() {
    // 100 requests: 10 cancelled (multiples of 11 incl. 0), 7
    // deadline-exceeded (multiples of 13 minus the already-cancelled 0),
    // the rest admitted or saturated.
    let metrics = queue_load_metrics(100);
    assert_eq!(metrics.cancelled, 10);
    assert_eq!(metrics.deadline_exceeded, 7);
    assert_eq!(metrics.total(), 100);
    assert_eq!(
        metrics.admitted + metrics.saturated,
        83,
        "every non-cancelled, non-expired request is admitted or saturated"
    );
    assert!(metrics.admitted > 0 && metrics.saturated > 0);
    assert!(
        metrics.preserve_rate() > 0.0,
        "saturation/cancel/deadline must measurably hit the Preserve path"
    );
}

#[test]
fn paired_contract_binds_one_model_route_and_provider() {
    let corpus = Corpus::load(corpus_root()).unwrap();
        let config = RunConfig {
        corpus_root: corpus_root(),
        iterations: 1,
        load_test_requests: 0,
    };
    let report = run_serialized(config);
    let contract = build_paired_contract(&corpus, &report.metrics.items).unwrap();
    assert_eq!(contract.schema_version, 1);
    assert_eq!(contract.corpus_manifest_digest, report.corpus_manifest_digest);
    assert!(contract.tasks.len() >= 5, "coding tasks must be paired");
    for task in &contract.tasks {
        assert!(task.same_model && task.same_route && task.same_provider);
        assert_eq!(task.model, "bench-model");
        assert_eq!(task.route_id, "bench-route");
        assert_eq!(task.provider_id, "bench-provider");
        // Offline contract: digests are byte-bound and stable.
        assert!(!task.baseline.body_digest.is_empty());
        assert!(!task.optimized.body_digest.is_empty());
    }
}

#[test]
fn fallback_measurement_is_honest_zero_savings_items_are_counted() {
        let config = RunConfig {
        corpus_root: corpus_root(),
        iterations: 1,
        load_test_requests: 0,
    };
    let report = run_serialized(config);
    let metrics = &report.metrics;
    assert!(metrics.fallback_rate > 0.0, "fallbacks must be measured, not hidden");
    assert!(metrics.benefit_rate > 0.0);
    for item in &metrics.items {
        assert_eq!(item.fallback, item.saved_tokens == 0);
        assert_eq!(item.positive_benefit, item.saved_tokens > 0);
    }
}

#[test]
fn baseline_self_check_passes_without_regression() {
        let config = RunConfig {
        corpus_root: corpus_root(),
        iterations: 1,
        load_test_requests: 0,
    };
    let report = run_serialized(config);
    let baseline = BaselineRecord::from_report(
        &report,
        1.25,
        0.8,
        dtoken_benchmark::DEFAULT_PRODUCT_FLOOR,
    );
    assert_eq!(baseline.corpus_manifest_digest, report.corpus_manifest_digest);
    let checks = check_no_regression(&report.metrics, &baseline).unwrap();
    let failed = checks
        .iter()
        .filter(|check| !check.ok)
        .map(|check| check.id)
        .collect::<Vec<_>>();
    assert!(failed.is_empty(), "self baseline must pass: {failed:?}");
    // The band is derived from the fresh baseline, not a magic constant
    // (ceil of baseline × multiplier).
    assert_eq!(
        baseline.bands.p50_latency_us_max,
        (baseline.metrics.p50_latency_us.unwrap() as f64 * 1.25).ceil() as u64
    );
}

#[test]
fn queue_metrics_aggregate_totals() {
    let metrics = QueueMetrics {
        admitted: 10,
        saturated: 2,
        cancelled: 1,
        deadline_exceeded: 1,
    };
    assert_eq!(metrics.total(), 14);
    assert!((metrics.preserve_rate() - (4.0 / 14.0)).abs() < 1e-9);
    let empty = QueueMetrics::default();
    assert_eq!(empty.preserve_rate(), 0.0);
}

#[test]
fn perf_metrics_aggregation_is_consistent() {
    let values = (1..=100).map(|index| index * 10).collect::<Vec<_>>();
    let items = values
        .iter()
        .enumerate()
        .map(|(index, latency)| dtoken_benchmark::metrics::ItemMetric {
            id: format!("item-{index}"),
            kind: "coding-task".to_string(),
            partition: "train".to_string(),
            input_bytes: 100,
            ir_nodes: 10,
            baseline_tokens: 100,
            final_tokens: 50,
            saved_tokens: 50,
            latency_us: *latency,
            transforms: vec!["log_fold_v1".to_string()],
            original_body_digest: "a".to_string(),
            final_body_digest: "b".to_string(),
            decision_digest: "c".to_string(),
            fallback: false,
            positive_benefit: true,
        })
        .collect::<Vec<_>>();
    let metrics = PerfMetrics::aggregate(items, QueueMetrics::default());
    assert_eq!(metrics.p50_latency_us, Some(500));
    assert_eq!(metrics.p95_latency_us, Some(950));
    assert_eq!(metrics.p99_latency_us, Some(990));
    assert_eq!(metrics.benefit_rate, 1.0);
    let json = serde_json::to_string(&metrics).unwrap();
    let parsed: PerfMetrics = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.p99_latency_us, Some(990));
}
