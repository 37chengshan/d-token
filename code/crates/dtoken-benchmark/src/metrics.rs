//! Aggregated metrics over a corpus run: percentiles, throughput, allocation
//! and RSS instrumentation, queue/cancel behavior, benefit/fallback rates,
//! and the end-to-end product savings ratio.

use serde::{Deserialize, Serialize};

/// Nearest-rank percentile (R-7).  `q` is in `(0, 1]`; `0.95` and `0.99`
/// follow the standard benchmark convention of the nearest-rank position.
pub fn percentile(mut sorted: Vec<u64>, q: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_unstable();
    rank(&sorted, q)
}

/// Nearest-rank percentile over an already-sorted slice.
fn rank(sorted: &[u64], q: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    Some(sorted[index - 1])
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct QueueMetrics {
    pub admitted: u64,
    pub saturated: u64,
    pub cancelled: u64,
    pub deadline_exceeded: u64,
}

impl QueueMetrics {
    pub fn total(&self) -> u64 {
        self.admitted + self.saturated + self.cancelled + self.deadline_exceeded
    }

    /// Fraction of requests that could not run candidate work and therefore
    /// took the Preserve path (saturation, cancellation, deadline).
    pub fn preserve_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        (self.saturated + self.cancelled + self.deadline_exceeded) as f64 / total as f64
    }
}

/// Per-item measurement through the resolved production path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemMetric {
    pub id: String,
    pub kind: String,
    pub partition: String,
    pub input_bytes: usize,
    pub ir_nodes: usize,
    pub baseline_tokens: usize,
    pub final_tokens: usize,
    pub saved_tokens: usize,
    pub latency_us: u64,
    pub transforms: Vec<String>,
    pub original_body_digest: String,
    pub final_body_digest: String,
    pub decision_digest: String,
    /// `true` when the item went out unchanged (Preserve/fallback).
    pub fallback: bool,
    /// `true` when the item produced a positive token saving.
    pub positive_benefit: bool,
}

impl ItemMetric {
    pub fn savings_ratio(&self) -> f64 {
        if self.baseline_tokens == 0 {
            return 0.0;
        }
        self.saved_tokens as f64 / self.baseline_tokens as f64
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PerfMetrics {
    pub item_count: usize,
    pub p50_latency_us: Option<u64>,
    pub p95_latency_us: Option<u64>,
    pub p99_latency_us: Option<u64>,
    pub mean_latency_us: Option<u64>,
    pub throughput_bytes_per_s: Option<f64>,
    pub throughput_requests_per_s: Option<f64>,
    pub throughput_nodes_per_s: Option<f64>,
    pub alloc_count: u64,
    pub alloc_bytes: u64,
    pub max_rss_bytes: Option<u64>,
    pub cpu_us: Option<u64>,
    /// End-to-end savings over the whole product-level corpus:
    /// (Σ baseline tokens − Σ final tokens) / Σ baseline tokens.
    pub e2e_savings_ratio: f64,
    pub baseline_tokens_total: usize,
    pub final_tokens_total: usize,
    /// Fraction of items with a positive token saving.
    pub benefit_rate: f64,
    /// Fraction of items that went out unchanged.
    pub fallback_rate: f64,
    pub queue: QueueMetrics,
    pub items: Vec<ItemMetric>,
}

impl PerfMetrics {
    pub fn aggregate(items: Vec<ItemMetric>, queue: QueueMetrics) -> Self {
        let item_count = items.len();
        let mut latencies = items.iter().map(|item| item.latency_us).collect::<Vec<_>>();
        let mean_latency_us = if latencies.is_empty() {
            None
        } else {
            Some(
                (latencies.iter().map(|value| *value as u128).sum::<u128>()
                    / latencies.len() as u128) as u64,
            )
        };
        latencies.sort_unstable();
        let p50 = rank(&latencies, 0.50);
        let p95 = rank(&latencies, 0.95);
        let p99 = rank(&latencies, 0.99);

        let baseline_tokens_total = items.iter().map(|item| item.baseline_tokens).sum::<usize>();
        let final_tokens_total = items.iter().map(|item| item.final_tokens).sum::<usize>();
        let e2e_savings_ratio = if baseline_tokens_total == 0 {
            0.0
        } else {
            baseline_tokens_total.saturating_sub(final_tokens_total) as f64
                / baseline_tokens_total as f64
        };
        let benefit_count = items.iter().filter(|item| item.positive_benefit).count();
        let fallback_count = items.iter().filter(|item| item.fallback).count();
        let benefit_rate = if item_count == 0 {
            0.0
        } else {
            benefit_count as f64 / item_count as f64
        };
        let fallback_rate = if item_count == 0 {
            0.0
        } else {
            fallback_count as f64 / item_count as f64
        };

        let input_bytes_total = items.iter().map(|item| item.input_bytes).sum::<usize>();
        let nodes_total = items.iter().map(|item| item.ir_nodes).sum::<usize>();
        let total_latency_us = items.iter().map(|item| item.latency_us as u128).sum::<u128>();

        Self {
            item_count,
            p50_latency_us: p50,
            p95_latency_us: p95,
            p99_latency_us: p99,
            mean_latency_us,
            throughput_bytes_per_s: throughput(total_latency_us, input_bytes_total),
            throughput_requests_per_s: throughput(total_latency_us, item_count),
            throughput_nodes_per_s: throughput(total_latency_us, nodes_total),
            alloc_count: 0,
            alloc_bytes: 0,
            max_rss_bytes: None,
            cpu_us: None,
            e2e_savings_ratio,
            baseline_tokens_total,
            final_tokens_total,
            benefit_rate,
            fallback_rate,
            queue,
            items,
        }
    }

    pub fn with_instrumentation(
        mut self,
        alloc_count: u64,
        alloc_bytes: u64,
        max_rss_bytes: Option<u64>,
        cpu_us: Option<u64>,
    ) -> Self {
        self.alloc_count = alloc_count;
        self.alloc_bytes = alloc_bytes;
        self.max_rss_bytes = max_rss_bytes;
        self.cpu_us = cpu_us;
        self
    }
}

fn throughput(total_latency_us: u128, units: usize) -> Option<f64> {
    if total_latency_us == 0 {
        return None;
    }
    Some(units as f64 / (total_latency_us as f64 / 1_000_000.0))
}
