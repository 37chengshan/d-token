//! PR-C7A corpus contract: the tracked fixed product-level corpus parses,
//! digests match, partitions are valid, and the resolved-path runner really
//! compresses compressible items (never the no-op `compress()` path).

use dtoken_benchmark::alloc::CountingAllocator;
use dtoken_benchmark::run::{run_corpus, run_item, scope_for, RunConfig};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

static BENCH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run_serialized(config: RunConfig) -> dtoken_benchmark::RunReport {
    let _guard = BENCH_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    run_corpus(&config, &ALLOC).unwrap()
}
use dtoken_benchmark::{Corpus, DEFAULT_PRODUCT_FLOOR};
use dtoken_compress::CompressionEngine;
use serde_json::Value;

fn corpus_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

#[test]
fn tracked_corpus_manifest_is_valid_and_digests_match() {
    let corpus = Corpus::load(corpus_root()).expect("corpus must load and verify");
    assert_eq!(corpus.manifest.schema_version, 1);
    assert_eq!(corpus.manifest.id, "product-level-corpus-v1");
    assert!(
        corpus.items().len() >= 10,
        "product corpus must be substantial"
    );
    assert!(!corpus.manifest_digest.is_empty());

    let partitions = corpus
        .items()
        .iter()
        .map(|item| item.partition.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(partitions.contains("train"));
    assert!(partitions.contains("replay"));
    assert!(
        partitions.contains("holdout"),
        "holdout partition must exist"
    );

    // Corpus digest is stable across manifest item ordering.
    let mut reordered = corpus.manifest.clone();
    reordered.items.reverse();
    assert_eq!(reordered.corpus_digest(), corpus.manifest_digest);
}

#[test]
fn product_corpus_is_resolved_path_not_the_noop_compress() {
    let corpus = Corpus::load(corpus_root()).unwrap();
    let engine = CompressionEngine::new();
    let mut transformed_items = 0usize;
    for item in corpus.items() {
        let body: Value = serde_json::from_slice(corpus.bodies.get(&item.id).unwrap()).unwrap();
        let context = scope_for(item).unwrap();
        let metric = run_item(&engine, item, &body, &context).unwrap();
        if metric.saved_tokens > 0 {
            transformed_items += 1;
            // The resolved path reports real codec transforms; the no-op
            // `compress()` would report zero transforms and zero savings.
            assert!(
                metric
                    .transforms
                    .iter()
                    .any(|transform| transform.ends_with("_fold_v1")
                        || transform == "json_minify_v1"),
                "item {} must carry a real codec transform, got {:?}",
                item.id,
                metric.transforms
            );
        }
    }
    assert!(
        transformed_items >= 5,
        "the product corpus must contain at least 5 items the resolved path compresses"
    );
}

#[test]
fn floor_gate_math_is_exact_and_fails_below_the_product_floor() {
    // The floor is an absolute end-to-end ratio over the whole corpus.
    let metrics = dtoken_benchmark::metrics::PerfMetrics {
        e2e_savings_ratio: 0.0999,
        baseline_tokens_total: 10_000,
        final_tokens_total: 9_001,
        ..Default::default()
    };
    assert!(
        dtoken_benchmark::gates::enforce_product_floor(&metrics, DEFAULT_PRODUCT_FLOOR).is_err(),
        "savings below the 10% floor must fail even with no regression"
    );
    let metrics = dtoken_benchmark::metrics::PerfMetrics {
        e2e_savings_ratio: 0.25,
        baseline_tokens_total: 10_000,
        final_tokens_total: 7_500,
        ..Default::default()
    };
    let check = dtoken_benchmark::gates::enforce_product_floor(&metrics, DEFAULT_PRODUCT_FLOOR)
        .expect("savings above the floor must pass");
    assert_eq!(check.observed_savings_ratio, 0.25);
    assert_eq!(check.baseline_tokens_total, 10_000);
    assert_eq!(check.final_tokens_total, 7_500);
}

#[test]
fn floor_metric_matches_manual_sum_over_the_tracked_corpus() {
    // The runner's e2e ratio must equal (Σ baseline − Σ final) / Σ baseline
    // computed independently over the raw per-item numbers.
    let config = RunConfig {
        corpus_root: corpus_root(),
        iterations: 1,
        load_test_requests: 0,
    };
    let report = run_serialized(config);
    let baseline_total = report
        .metrics
        .items
        .iter()
        .map(|item| item.baseline_tokens)
        .sum::<usize>();
    let final_total = report
        .metrics
        .items
        .iter()
        .map(|item| item.final_tokens)
        .sum::<usize>();
    let manual = (baseline_total.saturating_sub(final_total)) as f64 / baseline_total as f64;
    assert!(
        (manual - report.metrics.e2e_savings_ratio).abs() < 1e-9,
        "aggregate ratio must match the manual computation"
    );
    assert!(
        report.metrics.e2e_savings_ratio >= DEFAULT_PRODUCT_FLOOR,
        "the tracked product corpus must clear the 10% floor on this platform"
    );
}

#[test]
fn corpus_tampering_is_detected() {
    // Mutating a corpus item must be caught by the manifest digest check.
    let root = corpus_root();
    let first_item = Corpus::load(&root).unwrap().items()[0].clone();
    let item_path = root.join(&first_item.path);
    let original = std::fs::read(&item_path).unwrap();
    let mut tampered = original.clone();
    tampered[0] ^= 0x01;
    std::fs::write(&item_path, &tampered).unwrap();
    let result = Corpus::load(&root);
    std::fs::write(&item_path, &original).unwrap();
    assert!(
        result.is_err(),
        "a mutated corpus item must fail verification"
    );
}
