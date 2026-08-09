//! `dtoken-bench` — fixed-corpus resolved-path benchmark runner (PR-C7A/B/C).
//!
//! Subcommands:
//! - `record-baseline`: measure the corpus on this machine and write a fresh
//!   baseline (metrics + frozen no-regression band) to `--out`.
//! - `check`: measure again and fail closed (non-zero exit) on any
//!   no-regression violation or on the absolute product floor (QUALITY-8).
//! - `report`: measure and print the observed metrics (never gates).
//! - `replay`: verify deterministic replay digests across two passes.
//! - `paired-contract`: emit the offline same-model paired task contract.
//! - `load-test`: exercise the bounded scheduler gate and report queue
//!   saturation/cancellation/deadline behavior.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use dtoken_benchmark::alloc::CountingAllocator;

// The benchmark binary installs the counting allocator as the process-wide
// global allocator; the library never does, so no production crate is
// affected.  Allocations of the resolved path (and the queue load test) are
// measured by the runner.
#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();
use dtoken_benchmark::baseline::{
    check_no_regression, default_bands, default_product_floor, BaselineRecord,
};
use dtoken_benchmark::gates::{enforce_product_floor, BandCheckOutcome, GateSummary};
use dtoken_benchmark::metrics::PerfMetrics;
use dtoken_benchmark::paired::build_paired_contract;
use dtoken_benchmark::run::{queue_load_metrics, run_corpus, RunConfig};
use dtoken_benchmark::Corpus;
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "dtoken-bench", about = "Fixed-corpus resolved-path benchmark runner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Measure the corpus and write a fresh local baseline with a frozen
    /// no-regression band (PR-C7A).
    RecordBaseline {
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 3)]
        iterations: usize,
        #[arg(long)]
        floor: Option<f64>,
    },
    /// Measure again and fail closed on regression or the product floor.
    Check {
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long)]
        baseline: PathBuf,
        #[arg(long, default_value_t = 3)]
        iterations: usize,
        #[arg(long)]
        floor: Option<f64>,
        #[arg(long)]
        json: bool,
    },
    /// Measure and print observed metrics without any gate.
    Report {
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long, default_value_t = 3)]
        iterations: usize,
        #[arg(long)]
        json: bool,
    },
    /// Verify deterministic replay (identical decision digests across two
    /// independent passes).
    Replay {
        #[arg(long)]
        corpus: PathBuf,
    },
    /// Emit the offline same-model paired task contract.
    PairedContract {
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Exercise the bounded scheduler gate (queue/cancel/deadline metrics).
    LoadTest {
        #[arg(long, default_value_t = 10_000)]
        requests: u64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::RecordBaseline {
            corpus,
            out,
            iterations,
            floor,
        } => {
            let config = RunConfig {
                corpus_root: corpus,
                iterations,
                load_test_requests: 10_000,
            };
            let report = run_corpus(&config, &ALLOC)?;
            let (latency_multiplier, throughput_multiplier) = default_bands();
            let floor = floor.unwrap_or_else(default_product_floor);
            let baseline = BaselineRecord::from_report(
                &report,
                latency_multiplier,
                throughput_multiplier,
                floor,
            );
            baseline.write_json(&out)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "out": out.display().to_string(),
                    "corpus_digest": report.corpus_manifest_digest,
                    "platform": report.platform,
                    "floor": floor,
                    "e2e_savings_ratio": report.metrics.e2e_savings_ratio,
                    "p50_us": report.metrics.p50_latency_us,
                    "p95_us": report.metrics.p95_latency_us,
                    "p99_us": report.metrics.p99_latency_us,
                    "band_latency_multiplier": latency_multiplier,
                }))?
            );
            Ok(())
        }
        Command::Check {
            corpus,
            baseline,
            iterations,
            floor,
            json,
        } => {
            let config = RunConfig {
                corpus_root: corpus,
                iterations,
                load_test_requests: 10_000,
            };
            let report = run_corpus(&config, &ALLOC)?;
            let record = BaselineRecord::read_json(&baseline)?;
            if record.corpus_manifest_digest != report.corpus_manifest_digest {
                bail!(
                    "corpus identity mismatch: baseline {} current {} — a stale baseline cannot gate this run",
                    record.corpus_manifest_digest,
                    report.corpus_manifest_digest
                );
            }
            let band_checks = check_no_regression(&report.metrics, &record)?;
            let floor = floor.unwrap_or(record.product_floor);
            let floor_check = enforce_product_floor(&report.metrics, floor)?;
            let summary = GateSummary {
                runner_version: dtoken_benchmark::RUNNER_VERSION.to_string(),
                floor: floor_check.floor,
                floor_ok: floor_check.ok(),
                observed_savings_ratio: floor_check.observed_savings_ratio,
                baseline_tokens_total: floor_check.baseline_tokens_total,
                final_tokens_total: floor_check.final_tokens_total,
                band_ok: band_checks.iter().all(|check| check.ok),
                band_checks: band_checks
                    .iter()
                    .map(|check| BandCheckOutcome {
                        id: check.id.to_string(),
                        ok: check.ok,
                    })
                    .collect(),
                replay_deterministic: report.replay_deterministic,
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "summary": &summary,
                        "metrics": &report.metrics,
                    }))?
                );
            } else {
                println!(
                    "floor={:.4} observed={:.4} floor_ok={} band_ok={} replay_deterministic={}",
                    summary.floor,
                    summary.observed_savings_ratio,
                    summary.floor_ok,
                    summary.band_ok,
                    summary.replay_deterministic
                );
                for check in &band_checks {
                    println!("  {}: observed {} allowed {} -> {}", check.id, check.observed, check.allowed, if check.ok { "ok" } else { "FAIL" });
                }
            }
            if !summary.all_gates_ok() {
                let mut failures = Vec::new();
                if !summary.floor_ok {
                    failures.push("product floor");
                }
                if !summary.band_ok {
                    failures.push("no-regression band");
                }
                if !summary.replay_deterministic {
                    failures.push("deterministic replay");
                }
                bail!("benchmark check FAILED: {}", failures.join(", "));
            }
            Ok(())
        }
        Command::Report {
            corpus,
            iterations,
            json,
        } => {
            let config = RunConfig {
                corpus_root: corpus,
                iterations,
                load_test_requests: 10_000,
            };
            let report = run_corpus(&config, &ALLOC)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_metrics(&report.metrics);
            }
            Ok(())
        }
        Command::Replay { corpus } => {
            let config = RunConfig {
                corpus_root: corpus,
                iterations: 1,
                load_test_requests: 0,
            };
            let report = run_corpus(&config, &ALLOC)?;
            println!(
                "replay_deterministic={} items={}",
                report.replay_deterministic,
                report.metrics.item_count
            );
            if !report.replay_deterministic {
                bail!("deterministic replay FAILED");
            }
            Ok(())
        }
        Command::PairedContract { corpus, out } => {
            let config = RunConfig {
                corpus_root: corpus.clone(),
                iterations: 1,
                load_test_requests: 0,
            };
            let report = run_corpus(&config, &ALLOC)?;
            let loaded = Corpus::load(&corpus)?;
            let contract = build_paired_contract(&loaded, &report.metrics.items)?;
            let json = serde_json::to_string_pretty(&contract).context("serialize paired contract")?;
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create directory {}", parent.display()))?;
            }
            std::fs::write(&out, json)
                .with_context(|| format!("write paired contract {}", out.display()))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "out": out.display().to_string(),
                    "tasks": contract.tasks.len(),
                    "same_model_contract": contract.tasks.iter().all(|task| task.same_model && task.same_route && task.same_provider),
                }))?
            );
            Ok(())
        }
        Command::LoadTest { requests } => {
            let metrics = queue_load_metrics(requests);
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "admitted": metrics.admitted,
                    "saturated": metrics.saturated,
                    "cancelled": metrics.cancelled,
                    "deadline_exceeded": metrics.deadline_exceeded,
                    "preserve_rate": metrics.preserve_rate(),
                }))?
            );
            Ok(())
        }
    }
}

fn print_metrics(metrics: &PerfMetrics) {
    println!(
        "items={} p50_us={:?} p95_us={:?} p99_us={:?} mean_us={:?}",
        metrics.item_count,
        metrics.p50_latency_us,
        metrics.p95_latency_us,
        metrics.p99_latency_us,
        metrics.mean_latency_us
    );
    println!(
        "throughput_bytes_per_s={:?} requests_per_s={:?} nodes_per_s={:?}",
        metrics.throughput_bytes_per_s,
        metrics.throughput_requests_per_s,
        metrics.throughput_nodes_per_s
    );
    println!(
        "alloc_count={} alloc_bytes={} max_rss_bytes={:?} cpu_us={:?}",
        metrics.alloc_count, metrics.alloc_bytes, metrics.max_rss_bytes, metrics.cpu_us
    );
    println!(
        "e2e_savings_ratio={:.4} baseline_tokens={} final_tokens={} benefit_rate={:.4} fallback_rate={:.4}",
        metrics.e2e_savings_ratio,
        metrics.baseline_tokens_total,
        metrics.final_tokens_total,
        metrics.benefit_rate,
        metrics.fallback_rate
    );
    println!(
        "queue admitted={} saturated={} cancelled={} deadline_exceeded={}",
        metrics.queue.admitted, metrics.queue.saturated, metrics.queue.cancelled, metrics.queue.deadline_exceeded
    );
}
