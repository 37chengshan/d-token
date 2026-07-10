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

    let net_saved = text_tokens_before + cache_burn_text_side
        - image_tokens_after
        - sidecar_tokens
        - cache_burn_image_side
        - output_risk_tokens;

    let (profitable, reason) = if candidate_chars < 2000 {
        (false, EconomicsReason::TooSmall)
    } else if cache_burn_image_side > 0.0 && cache_burn_image_side > net_saved.abs() {
        (false, EconomicsReason::CacheBurnFlipped)
    } else if net_saved < 64.0 {
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
        net_saved_tokens: net_saved,
        profitable,
        reason,
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
}
