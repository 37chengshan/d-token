use dtoken_types::{OpticalEndpoint, OpticalEvalArtifact};
use sha2::{Digest, Sha256};

use super::renderer::{font_digest, profile_digest};
use super::RenderGeometryProfile;

#[derive(Debug, Clone, PartialEq)]
pub struct EvalThresholds {
    pub minimum_samples: u64,
    pub maximum_quality_regression: f64,
    pub maximum_latency_p95_delta_ms: i64,
    pub require_positive_input_savings: bool,
}

impl Default for EvalThresholds {
    fn default() -> Self {
        Self {
            minimum_samples: 20,
            maximum_quality_regression: 0.0,
            maximum_latency_p95_delta_ms: 2_000,
            require_positive_input_savings: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalSampleObservation {
    pub baseline_passed: bool,
    pub candidate_passed: bool,
    pub exact_failure: bool,
    pub unsafe_failure: bool,
    pub baseline_input_tokens: u64,
    pub candidate_input_tokens: u64,
    pub baseline_cost_microusd: u64,
    pub candidate_cost_microusd: u64,
    pub baseline_latency_ms: u64,
    pub candidate_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalBuildError {
    EmptyArtifactId,
    EmptyModel,
    EmptyCorpusDigest,
    InvalidTimestamp,
    NoSamples,
}

pub struct OpticalEvalBuilder {
    artifact_id: String,
    model_base: String,
    endpoint: OpticalEndpoint,
    renderer_profile_digest: String,
    font_digest: String,
    corpus_digest: String,
    evaluated_at_ms: u64,
    thresholds: EvalThresholds,
    samples: Vec<EvalSampleObservation>,
}

impl OpticalEvalBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        artifact_id: impl Into<String>,
        model_base: impl Into<String>,
        endpoint: OpticalEndpoint,
        profile: &RenderGeometryProfile,
        corpus_digest: impl Into<String>,
        evaluated_at_ms: u64,
        thresholds: EvalThresholds,
    ) -> Self {
        Self {
            artifact_id: artifact_id.into(),
            model_base: model_base.into(),
            endpoint,
            renderer_profile_digest: profile_digest(profile),
            font_digest: font_digest(),
            corpus_digest: corpus_digest.into(),
            evaluated_at_ms,
            thresholds,
            samples: Vec::new(),
        }
    }

    pub fn record(&mut self, observation: EvalSampleObservation) {
        self.samples.push(observation);
    }

    pub fn finalize(self) -> Result<OpticalEvalArtifact, EvalBuildError> {
        if self.artifact_id.trim().is_empty() {
            return Err(EvalBuildError::EmptyArtifactId);
        }
        if self.model_base.trim().is_empty() {
            return Err(EvalBuildError::EmptyModel);
        }
        if self.corpus_digest.trim().is_empty() {
            return Err(EvalBuildError::EmptyCorpusDigest);
        }
        if self.evaluated_at_ms == 0 {
            return Err(EvalBuildError::InvalidTimestamp);
        }
        if self.samples.is_empty() {
            return Err(EvalBuildError::NoSamples);
        }

        let sample_count = u64::try_from(self.samples.len()).unwrap_or(u64::MAX);
        let baseline_passes = self
            .samples
            .iter()
            .filter(|sample| sample.baseline_passed)
            .count() as u64;
        let candidate_passes = self
            .samples
            .iter()
            .filter(|sample| sample.candidate_passed)
            .count() as u64;
        let exact_failures = self
            .samples
            .iter()
            .filter(|sample| sample.exact_failure)
            .count() as u64;
        let unsafe_failures = self
            .samples
            .iter()
            .filter(|sample| sample.unsafe_failure)
            .count() as u64;
        let quality_delta = if sample_count == 0 {
            0.0
        } else {
            (candidate_passes as f64 - baseline_passes as f64) / sample_count as f64
        };
        let baseline_tokens = self
            .samples
            .iter()
            .map(|sample| sample.baseline_input_tokens)
            .fold(0u64, u64::saturating_add);
        let candidate_tokens = self
            .samples
            .iter()
            .map(|sample| sample.candidate_input_tokens)
            .fold(0u64, u64::saturating_add);
        let baseline_cost = self
            .samples
            .iter()
            .map(|sample| sample.baseline_cost_microusd)
            .fold(0u64, u64::saturating_add);
        let candidate_cost = self
            .samples
            .iter()
            .map(|sample| sample.candidate_cost_microusd)
            .fold(0u64, u64::saturating_add);
        let latency_delta = p95(self
            .samples
            .iter()
            .map(|sample| sample.candidate_latency_ms)
            .collect()) as i128
            - p95(self
                .samples
                .iter()
                .map(|sample| sample.baseline_latency_ms)
                .collect()) as i128;
        let signed_input_token_delta = saturated_difference(baseline_tokens, candidate_tokens);
        let signed_cost_delta_microusd = saturated_difference(baseline_cost, candidate_cost);
        let latency_p95_delta_ms = latency_delta.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        let passed = sample_count >= self.thresholds.minimum_samples
            && exact_failures == 0
            && unsafe_failures == 0
            && quality_delta >= -self.thresholds.maximum_quality_regression
            && latency_p95_delta_ms <= self.thresholds.maximum_latency_p95_delta_ms
            && (!self.thresholds.require_positive_input_savings || signed_input_token_delta > 0);

        Ok(OpticalEvalArtifact {
            schema_version: 1,
            artifact_id: self.artifact_id,
            model_base: self.model_base.trim().to_ascii_lowercase(),
            endpoint: self.endpoint,
            renderer_profile_digest: self.renderer_profile_digest,
            font_digest: self.font_digest,
            corpus_digest: self.corpus_digest,
            evaluated_at_ms: self.evaluated_at_ms,
            sample_count,
            exact_failures,
            unsafe_failures,
            baseline_passes,
            candidate_passes,
            quality_delta,
            signed_input_token_delta,
            signed_cost_delta_microusd,
            latency_p95_delta_ms,
            passed,
        })
    }
}

pub fn eval_corpus_digest(case_ids: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let mut ids = case_ids
        .into_iter()
        .map(|id| id.as_ref().trim().to_string())
        .collect::<Vec<_>>();
    ids.sort();
    format!("{:x}", Sha256::digest(ids.join("\n").as_bytes()))
}

fn p95(mut values: Vec<u64>) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let index = values
        .len()
        .saturating_mul(95)
        .div_ceil(100)
        .saturating_sub(1);
    values[index.min(values.len() - 1)]
}

fn saturated_difference(baseline: u64, candidate: u64) -> i64 {
    let difference = i128::from(baseline) - i128::from(candidate);
    difference.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}
