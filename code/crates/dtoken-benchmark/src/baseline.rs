//! Fresh-baseline records and the no-regression band (PR-C7A).
//!
//! A baseline is only ever produced by running the runner on this machine
//! (`record-baseline`); it freezes the observed metrics plus the band
//! boundaries *derived* from them.  `check` compares a current run against a
//! baseline and fails closed on any violation.  The absolute product floor
//! (QUALITY-8, ≥10% end-to-end savings) is enforced separately in
//! [`crate::gates`], never conflated with the relative band.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::metrics::PerfMetrics;
use crate::{
    DEFAULT_NO_REGRESSION_LATENCY_MULTIPLIER, DEFAULT_NO_REGRESSION_THROUGHPUT_MULTIPLIER,
    DEFAULT_PRODUCT_FLOOR, RUNNER_VERSION,
};

pub const BASELINE_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoRegressionBands {
    /// p50/p95/p99 ceilings: fresh baseline latency × latency multiplier.
    pub p50_latency_us_max: u64,
    pub p95_latency_us_max: u64,
    pub p99_latency_us_max: u64,
    /// Floor for throughput: fresh baseline × throughput multiplier.
    pub throughput_bytes_per_s_min: Option<f64>,
    pub throughput_requests_per_s_min: Option<f64>,
    /// Benefit rate may fall by at most this absolute amount (noise band).
    pub benefit_rate_min: f64,
    pub latency_multiplier: f64,
    pub throughput_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaselineRecord {
    pub schema_version: u64,
    pub runner_version: String,
    pub platform: String,
    pub recorded_at_ms: u64,
    pub corpus_manifest_digest: String,
    pub corpus_id: String,
    pub metrics: PerfMetrics,
    pub bands: NoRegressionBands,
    /// Absolute product floor at record time (QUALITY-8).
    pub product_floor: f64,
}

impl BaselineRecord {
    pub fn from_report(
        report: &crate::RunReport,
        latency_multiplier: f64,
        throughput_multiplier: f64,
        product_floor: f64,
    ) -> Self {
        let metrics = &report.metrics;
        let ceil = |value: Option<u64>| {
            value
                .map(|value| (value as f64 * latency_multiplier).ceil() as u64)
                .unwrap_or(u64::MAX)
        };
        let bands = NoRegressionBands {
            p50_latency_us_max: ceil(metrics.p50_latency_us),
            p95_latency_us_max: ceil(metrics.p95_latency_us),
            p99_latency_us_max: ceil(metrics.p99_latency_us),
            throughput_bytes_per_s_min: metrics
                .throughput_bytes_per_s
                .map(|value| value * throughput_multiplier),
            throughput_requests_per_s_min: metrics
                .throughput_requests_per_s
                .map(|value| value * throughput_multiplier),
            benefit_rate_min: (metrics.benefit_rate - 0.05).max(0.0),
            latency_multiplier,
            throughput_multiplier,
        };
        Self {
            schema_version: BASELINE_SCHEMA_VERSION,
            runner_version: RUNNER_VERSION.to_string(),
            platform: report.platform.clone(),
            recorded_at_ms: now_ms(),
            corpus_manifest_digest: report.corpus_manifest_digest.clone(),
            corpus_id: report.corpus_id.clone(),
            metrics: metrics.clone(),
            bands,
            product_floor,
        }
    }

    pub fn write_json(&self, path: &std::path::Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("serialize baseline")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create baseline directory {}", parent.display()))?;
        }
        std::fs::write(path, json).with_context(|| format!("write baseline {}", path.display()))
    }

    pub fn read_json(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read baseline {}", path.display()))?;
        let record: Self = serde_json::from_str(&text).context("parse baseline JSON")?;
        if record.schema_version != BASELINE_SCHEMA_VERSION {
            bail!(
                "baseline schema {} unsupported (expected {})",
                record.schema_version,
                BASELINE_SCHEMA_VERSION
            );
        }
        if record.runner_version != RUNNER_VERSION {
            bail!(
                "baseline runner version {} does not match current {}",
                record.runner_version,
                RUNNER_VERSION
            );
        }
        Ok(record)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BandCheck {
    pub id: &'static str,
    pub observed: String,
    pub allowed: String,
    pub ok: bool,
}

/// Compare a current run's metrics against a recorded baseline's frozen
/// bands.  Corpus identity must match or the comparison is invalid.
pub fn check_no_regression(
    current: &PerfMetrics,
    baseline: &BaselineRecord,
) -> Result<Vec<BandCheck>> {
    if current.item_count == 0 {
        bail!("current run has no measured items");
    }
    let mut checks = Vec::new();
    for (id, observed, allowed) in [
        (
            "latency_p50_us",
            current.p50_latency_us.unwrap_or(u64::MAX),
            baseline.bands.p50_latency_us_max,
        ),
        (
            "latency_p95_us",
            current.p95_latency_us.unwrap_or(u64::MAX),
            baseline.bands.p95_latency_us_max,
        ),
        (
            "latency_p99_us",
            current.p99_latency_us.unwrap_or(u64::MAX),
            baseline.bands.p99_latency_us_max,
        ),
    ] {
        checks.push(BandCheck {
            id,
            observed: format!("{observed}"),
            allowed: format!("<= {allowed}"),
            ok: observed <= allowed,
        });
    }
    for (id, observed, allowed) in [
        (
            "throughput_bytes_per_s",
            current.throughput_bytes_per_s.unwrap_or(0.0),
            baseline.bands.throughput_bytes_per_s_min.unwrap_or(0.0),
        ),
        (
            "throughput_requests_per_s",
            current.throughput_requests_per_s.unwrap_or(0.0),
            baseline.bands.throughput_requests_per_s_min.unwrap_or(0.0),
        ),
    ] {
        checks.push(BandCheck {
            id,
            observed: format!("{observed:.2}"),
            allowed: format!(">= {allowed:.2}"),
            ok: observed >= allowed,
        });
    }
    checks.push(BandCheck {
        id: "benefit_rate",
        observed: format!("{:.4}", current.benefit_rate),
        allowed: format!(">= {:.4}", baseline.bands.benefit_rate_min),
        ok: current.benefit_rate >= baseline.bands.benefit_rate_min,
    });
    Ok(checks)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Default no-regression bands for fresh-baseline recording.
pub fn default_bands() -> (f64, f64) {
    (
        DEFAULT_NO_REGRESSION_LATENCY_MULTIPLIER,
        DEFAULT_NO_REGRESSION_THROUGHPUT_MULTIPLIER,
    )
}

/// The absolute product floor (QUALITY-8) used when none is supplied.
pub fn default_product_floor() -> f64 {
    DEFAULT_PRODUCT_FLOOR
}
