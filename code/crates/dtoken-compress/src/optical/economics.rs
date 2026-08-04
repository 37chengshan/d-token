// EconomicsPlanner：评估 text/image/sidecar/cache-burn，判断是否经济划算。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §18。

use crate::optical::render_geometry::{estimate_image_tokens, RenderGeometryProfile};
use dtoken_types::OpticalBucket;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LatencyRiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconomicsReason {
    SparseProse,
    DenseContentProfitable,
    CacheBurnFlipped,
    HistoryAmortized,
    TooSmall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEvidenceState {
    Cold,
    Warm,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CostEstimateRange {
    pub estimated: bool,
    pub cache_state: CacheEvidenceState,
    pub conservative_input_tokens: f64,
    pub expected_input_tokens: f64,
    pub optimistic_input_tokens: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ActiveEconomicsContract {
    pub tokenizer_revision: &'static str,
    pub pricing_revision: &'static str,
    pub cache_policy_revision: &'static str,
    pub chars_per_text_token: f64,
    pub cache_create_rate: f64,
    pub cache_read_rate: f64,
}

/// Production contracts must be loaded from an attested, model-bound source.
/// No such source is shipped yet, so production Active remains fail-closed.
pub(crate) fn resolve_active_economics_contract(
    model: &str,
    capability_revision: Option<u64>,
) -> Option<ActiveEconomicsContract> {
    #[cfg(test)]
    if model == "claude-fable-5" && capability_revision.is_some() {
        return Some(ActiveEconomicsContract {
            tokenizer_revision: "test-char4-tokenizer-v1",
            pricing_revision: "test-anthropic-cache-pricing-v1",
            cache_policy_revision: "cold-warm-unknown-max-burn-v1",
            chars_per_text_token: 4.0,
            cache_create_rate: 1.25,
            cache_read_rate: 0.1,
        });
    }
    let _ = (model, capability_revision);
    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct EconomicsEstimateRange {
    pub conservative_net_saved_tokens: f64,
    pub expected_net_saved_tokens: f64,
    pub optimistic_net_saved_tokens: f64,
}

#[derive(Debug, Clone)]
pub struct BlockEconomicsPlan {
    pub bucket: OpticalBucket,
    pub text_tokens_before: f64,
    pub image_tokens_after: f64,
    pub sidecar_tokens: f64,
    pub cache_burn_image_side: f64,
    pub cache_burn_text_side: f64,
    pub output_risk_tokens: f64,
    pub latency_risk_level: LatencyRiskLevel,
    pub net_saved_tokens: f64,
    pub estimate_range: EconomicsEstimateRange,
    pub profitable: bool,
    pub reason: EconomicsReason,
}

const BUCKET_CHARS_PER_TOKEN: f64 = 4.0;
const SIDECAR_CHARS_PER_TOKEN: f64 = 4.0;
const OUTPUT_RISK_MULTIPLIER: f64 = 0.05;

/// 单请求经济性计算。
pub fn evaluate_block_economics(
    bucket: OpticalBucket,
    candidate_chars: usize,
    image_count: usize,
    profile: &RenderGeometryProfile,
    prior_warm_text_tokens: f64,
    cache_create_rate: f64,
    cache_read_rate: f64,
) -> BlockEconomicsPlan {
    let text_tokens_before = candidate_chars as f64 / BUCKET_CHARS_PER_TOKEN;
    let image_tokens_after = estimate_image_tokens(profile, image_count) as f64;
    let sidecar_tokens = (candidate_chars as f64 * 0.1) / SIDECAR_CHARS_PER_TOKEN; // 估算 sidecar ~10%
    let cache_burn_image_side =
        prior_warm_text_tokens * (cache_create_rate - cache_read_rate).max(0.0);
    let cache_burn_text_side = 0.0; // shadow 模式不渲染，无 image-side burn
    let output_risk_tokens = (image_tokens_after + sidecar_tokens) * OUTPUT_RISK_MULTIPLIER;

    let expected_net_saved = text_tokens_before + cache_burn_text_side
        - image_tokens_after
        - sidecar_tokens
        - cache_burn_image_side
        - output_risk_tokens;
    let conservative_net_saved = text_tokens_before + cache_burn_text_side
        - image_tokens_after * 1.10
        - sidecar_tokens * 1.50
        - cache_burn_image_side * 1.25
        - output_risk_tokens * 2.0;
    let optimistic_net_saved = text_tokens_before + cache_burn_text_side
        - image_tokens_after
        - sidecar_tokens * 0.75
        - cache_burn_image_side
        - output_risk_tokens * 0.5;
    let estimate_range = EconomicsEstimateRange {
        conservative_net_saved_tokens: conservative_net_saved,
        expected_net_saved_tokens: expected_net_saved,
        optimistic_net_saved_tokens: optimistic_net_saved,
    };

    let (profitable, reason) = if candidate_chars < 2000 {
        (false, EconomicsReason::TooSmall)
    } else if cache_burn_image_side > 0.0 && cache_burn_image_side > expected_net_saved.abs() {
        (false, EconomicsReason::CacheBurnFlipped)
    } else if conservative_net_saved < 64.0 {
        (false, EconomicsReason::SparseProse)
    } else {
        (true, EconomicsReason::DenseContentProfitable)
    };

    let latency_risk_level = if image_tokens_after > 2000.0 {
        LatencyRiskLevel::High
    } else if image_tokens_after > 500.0 {
        LatencyRiskLevel::Medium
    } else {
        LatencyRiskLevel::Low
    };

    BlockEconomicsPlan {
        bucket,
        text_tokens_before,
        image_tokens_after,
        sidecar_tokens,
        cache_burn_image_side,
        cache_burn_text_side,
        output_risk_tokens,
        latency_risk_level,
        net_saved_tokens: expected_net_saved,
        estimate_range,
        profitable,
        reason,
    }
}

/// Economics for a fully materialized capsule. `sidecar_tokens` must come from
/// the actual pointer/factsheet/guard serialization, rather than a per-page
/// constant. Unknown cache state is intentionally priced as the largest burn.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_materialized_capsule(
    text_tokens_before: f64,
    image_tokens_after: f64,
    sidecar_tokens: f64,
    prior_prefix_tokens: f64,
    cache_create_rate: f64,
    cache_read_rate: f64,
    cache_state: CacheEvidenceState,
) -> CostEstimateRange {
    let cold_burn = prior_prefix_tokens * cache_create_rate.max(0.0);
    let warm_burn = prior_prefix_tokens * cache_read_rate.max(0.0);
    let observed_burn = match cache_state {
        CacheEvidenceState::Cold => cold_burn,
        CacheEvidenceState::Warm => warm_burn,
        CacheEvidenceState::Unknown => cold_burn.max(warm_burn),
    };
    let expected = image_tokens_after + sidecar_tokens + observed_burn;
    CostEstimateRange {
        estimated: true,
        cache_state,
        conservative_input_tokens: expected + sidecar_tokens * 0.5 + image_tokens_after * 0.1,
        expected_input_tokens: expected,
        optimistic_input_tokens: (image_tokens_after + sidecar_tokens * 0.75 + warm_burn)
            .min(text_tokens_before.max(0.0)),
    }
}

/// 历史摊销经济性（简化版）。
pub fn history_amortized_profitable(
    image_tokens: f64,
    text_tokens: f64,
    cache_create_rate: f64,
    cache_read_rate: f64,
    turns: usize,
) -> bool {
    let n = turns.max(1) as f64;
    let image_lifetime = image_tokens * (cache_create_rate + cache_read_rate * (n - 1.0));
    let text_lifetime = text_tokens * cache_read_rate * n;
    image_lifetime < text_lifetime
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optical::render_geometry::anthropic_dense_v1;

    #[test]
    fn sparse_prose_unprofitable() {
        let profile = anthropic_dense_v1();
        let plan = evaluate_block_economics(
            OpticalBucket::ToolResultProse,
            500,
            1,
            &profile,
            0.0,
            1.25,
            0.1,
        );
        assert!(!plan.profitable);
        assert_eq!(plan.reason, EconomicsReason::TooSmall);
    }

    #[test]
    fn dense_log_profitable() {
        let profile = anthropic_dense_v1();
        let plan = evaluate_block_economics(
            OpticalBucket::ToolResultLog,
            20000,
            1,
            &profile,
            0.0,
            1.25,
            0.1,
        );
        assert!(plan.profitable);
        assert_eq!(plan.reason, EconomicsReason::DenseContentProfitable);
        assert!(
            plan.estimate_range.conservative_net_saved_tokens
                <= plan.estimate_range.expected_net_saved_tokens
        );
        assert!(
            plan.estimate_range.expected_net_saved_tokens
                <= plan.estimate_range.optimistic_net_saved_tokens
        );
    }

    #[test]
    fn cache_burn_can_flip_decision() {
        let profile = anthropic_dense_v1();
        let plan = evaluate_block_economics(
            OpticalBucket::ToolResultLog,
            20000,
            1,
            &profile,
            50000.0,
            1.25,
            0.1,
        );
        assert!(!plan.profitable);
        assert_eq!(plan.reason, EconomicsReason::CacheBurnFlipped);
    }

    #[test]
    fn history_uses_amortized_gate() {
        // image lifetime < text lifetime => profitable
        assert!(history_amortized_profitable(100.0, 500.0, 1.25, 0.1, 10));
        // image too expensive => not profitable
        assert!(!history_amortized_profitable(5000.0, 100.0, 1.25, 0.1, 2));
    }

    #[test]
    fn unknown_cache_uses_maximum_burn_and_materialized_sidecar() {
        let unknown = evaluate_materialized_capsule(
            10_000.0,
            1_464.0,
            211.0,
            5_000.0,
            1.25,
            0.1,
            CacheEvidenceState::Unknown,
        );
        let warm = evaluate_materialized_capsule(
            10_000.0,
            1_464.0,
            211.0,
            5_000.0,
            1.25,
            0.1,
            CacheEvidenceState::Warm,
        );
        assert!(unknown.expected_input_tokens > warm.expected_input_tokens);
        assert_eq!(unknown.cache_state, CacheEvidenceState::Unknown);
        assert!(unknown.estimated);
    }

    #[test]
    fn production_build_has_no_unattested_tokenizer_or_pricing_contract() {
        #[cfg(not(test))]
        assert!(resolve_active_economics_contract("claude-fable-5", Some(1)).is_none());
        #[cfg(test)]
        {
            assert!(resolve_active_economics_contract("unknown-model", Some(1)).is_none());
            let contract = resolve_active_economics_contract("claude-fable-5", Some(1))
                .expect("unit fixture contract");
            assert!(!contract.tokenizer_revision.is_empty());
            assert!(!contract.pricing_revision.is_empty());
        }
    }
}
