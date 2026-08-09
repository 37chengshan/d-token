//! Quality gates (PR-C7A/C, QUALITY-8): the absolute product floor and the
//! runner's fail-closed contract.  The floor is an absolute product-level
//! end-to-end savings gate (≥10% by default) and is enforced separately from
//! the no-regression band — a run can be "no worse than baseline" and still
//! fail because the product is below the floor.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::metrics::PerfMetrics;
use crate::{DEFAULT_PRODUCT_FLOOR, RUNNER_VERSION};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct FloorCheck {
    pub floor: f64,
    pub observed_savings_ratio: f64,
    pub baseline_tokens_total: usize,
    pub final_tokens_total: usize,
}

impl FloorCheck {
    pub fn ok(self) -> bool {
        self.observed_savings_ratio >= self.floor
    }
}

/// Enforce the absolute product compression floor on a measured run.
/// Fails (returns `Err`) whenever end-to-end savings fall below the floor —
/// "no regression vs baseline" is never an acceptable substitute (QUALITY-8 /
/// RISKS_DECISIONS D14).
pub fn enforce_product_floor(metrics: &PerfMetrics, floor: f64) -> Result<FloorCheck> {
    if floor <= 0.0 {
        bail!("product floor must be strictly positive, got {floor}");
    }
    let check = FloorCheck {
        floor,
        observed_savings_ratio: metrics.e2e_savings_ratio,
        baseline_tokens_total: metrics.baseline_tokens_total,
        final_tokens_total: metrics.final_tokens_total,
    };
    if !check.ok() {
        bail!(
            "PRODUCT FLOOR VIOLATION: end-to-end savings {:.4} < floor {:.4} \
             (baseline tokens {} -> final tokens {}). The product-level corpus \
             must save at least {}% end-to-end; a no-regression band is not a substitute.",
            check.observed_savings_ratio,
            check.floor,
            check.baseline_tokens_total,
            check.final_tokens_total,
            (check.floor * 100.0).round() as u64,
        );
    }
    Ok(check)
}

/// Machine-readable gate summary for the runner's JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateSummary {
    pub runner_version: String,
    pub floor: f64,
    pub floor_ok: bool,
    pub observed_savings_ratio: f64,
    pub baseline_tokens_total: usize,
    pub final_tokens_total: usize,
    pub band_ok: bool,
    pub band_checks: Vec<BandCheckOutcome>,
    pub replay_deterministic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandCheckOutcome {
    pub id: String,
    pub ok: bool,
}

impl GateSummary {
    pub fn all_gates_ok(&self) -> bool {
        self.floor_ok && self.band_ok && self.replay_deterministic
    }
}

pub fn default_floor() -> f64 {
    DEFAULT_PRODUCT_FLOOR
}

pub fn runner_version() -> String {
    RUNNER_VERSION.to_string()
}
