//! d-token fixed-corpus benchmark harness (PR-C7A/B/C).
//!
//! Measures the *resolved* production path — `CompressionEngine::
//! compress_resolved_with_ir`, the same entry the canonical pipeline uses —
//! over a tracked, fixed, product-level corpus.  It never touches the no-op
//! `compress()` fallback, so every reported saving is a real end-to-end
//! (inbound → plan → send) token reduction.

pub mod alloc;
pub mod baseline;
pub mod corpus;
pub mod gates;
pub mod metrics;
pub mod paired;
pub mod run;
pub mod system;

pub use corpus::Corpus;
pub use run::{RunConfig, RunReport};

/// Runner schema/behavior version.  Bumped whenever the measurement
/// contract changes so stale baselines cannot be compared silently.
pub const RUNNER_VERSION: &str = "bench-v1";

/// Default product floor (QUALITY-8): absolute end-to-end compression
/// savings must be at least this fraction of baseline tokens.  This is a
/// product gate, separate from the no-regression band.
pub const DEFAULT_PRODUCT_FLOOR: f64 = 0.10;

/// Default no-regression latency multiplier applied to the fresh baseline's
/// p50/p95/p99.  The band is *derived* from the baseline recorded on this
/// hardware; the multiplier is the band definition, not a magic threshold.
pub const DEFAULT_NO_REGRESSION_LATENCY_MULTIPLIER: f64 = 1.25;

/// Throughput is allowed to drop this fraction below the fresh baseline
/// before the run is a regression (noise band on the downside).
pub const DEFAULT_NO_REGRESSION_THROUGHPUT_MULTIPLIER: f64 = 0.8;
