//! Development probe: load the corpus, run the resolved path, and print
//! per-item savings.  Not part of the release runner contract; used to
//! validate corpus content.

use anyhow::Result;
use dtoken_benchmark::alloc::CountingAllocator;
use dtoken_benchmark::run::{run_item, scope_for, RunConfig};
use dtoken_benchmark::Corpus;
use dtoken_compress::CompressionEngine;
use serde_json::Value;

fn main() -> Result<()> {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "corpus".to_string());
    let allocator = CountingAllocator::default();
    let config = RunConfig {
        corpus_root: std::path::PathBuf::from(&root),
        iterations: 1,
        load_test_requests: 0,
    };
    let report = dtoken_benchmark::run::run_corpus(&config, &allocator)?;
    let corpus = Corpus::load(&root)?;
    let engine = CompressionEngine::new();
    let mut total_baseline = 0usize;
    let mut total_final = 0usize;
    for item in corpus.items() {
        let body: Value =
            serde_json::from_slice(corpus.bodies.get(&item.id).expect("body"))?;
        let context = scope_for(item)?;
        let metric = run_item(&engine, item, &body, &context)?;
        total_baseline += metric.baseline_tokens;
        total_final += metric.final_tokens;
        println!(
            "{:32} kind={:12} tokens {:6}->{:6} saved={:6} ratio={:6.3} nodes={:3} transforms={:?}",
            item.id,
            item.kind,
            metric.baseline_tokens,
            metric.final_tokens,
            metric.saved_tokens,
            metric.savings_ratio(),
            metric.ir_nodes,
            metric.transforms,
        );
    }
    println!(
        "TOTAL e2e savings = {:.4} ({} -> {})",
        (total_baseline.saturating_sub(total_final)) as f64 / total_baseline.max(1) as f64,
        total_baseline,
        total_final
    );
    println!(
        "aggregate e2e_savings_ratio = {:.4}, benefit_rate = {:.4}, fallback_rate = {:.4}",
        report.metrics.e2e_savings_ratio,
        report.metrics.benefit_rate,
        report.metrics.fallback_rate
    );
    Ok(())
}
