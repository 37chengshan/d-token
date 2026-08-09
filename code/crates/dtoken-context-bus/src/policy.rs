//! Cache policy — 从 `dtoken-compress/src/context_pipeline.rs` 搬移（单一 authority）。
//!
//! 原实现定义 `ProviderCacheEvidence` / `ProviderCacheStrategy` / `CacheEvidenceStatus`
//! 及其校验规则（`validate_provider_cache_evidence` / `strategy_matches_protocol`）。
//! 本 crate 是这些 policy 的唯一归属；`dtoken-compress` 通过 `pub use` re-export
//! 保持公开路径不变，不再本地持有定义。

use crate::signature::is_hex64;
use dtoken_types::Protocol;

/// Provider cache 策略（搬移自 compress `ProviderCacheStrategy`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCacheStrategy {
    AnthropicExplicit,
    OpenAiAutomatic,
    GoogleExplicit,
}

/// cache evidence 状态（搬移自 compress `CacheEvidenceStatus`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEvidenceStatus {
    Unavailable,
    VerifiedBelowMinimum,
    VerifiedEligible,
}

/// Provider cache evidence（搬移自 compress `ProviderCacheEvidence`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCacheEvidence {
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub strategy: ProviderCacheStrategy,
    pub ttl_seconds: Option<u64>,
    pub min_prefix_bytes: usize,
    pub source_revision: String,
}

/// Cache policy 校验错误（搬移自 compress `ContextTransformError` 的两个变体语义）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachePolicyError {
    ProviderCacheEvidenceMismatch,
    InvalidProviderCacheEvidence,
}

impl std::fmt::Display for CachePolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProviderCacheEvidenceMismatch => {
                formatter.write_str("provider cache evidence mismatch")
            }
            Self::InvalidProviderCacheEvidence => {
                formatter.write_str("invalid provider cache evidence")
            }
        }
    }
}

impl std::error::Error for CachePolicyError {}

/// 搬移自 compress `validate_provider_cache_evidence`（去掉对压缩上下文的依赖，
/// 纯 policy 校验，route/provider/model/protocol 匹配 + 字段形态校验）。
pub fn validate_provider_cache_evidence(
    route_id: &str,
    provider_id: &str,
    model: &str,
    input_protocol: &Protocol,
    evidence: &ProviderCacheEvidence,
) -> Result<(), CachePolicyError> {
    if evidence.route_id != route_id
        || evidence.provider_id != provider_id
        || evidence.model != model
        || !strategy_matches_protocol(evidence.strategy, input_protocol)
    {
        return Err(CachePolicyError::ProviderCacheEvidenceMismatch);
    }
    if evidence.min_prefix_bytes == 0
        || evidence.ttl_seconds == Some(0)
        || !is_hex64(&evidence.source_revision)
    {
        return Err(CachePolicyError::InvalidProviderCacheEvidence);
    }
    Ok(())
}

/// 搬移自 compress `strategy_matches_protocol`。
pub fn strategy_matches_protocol(strategy: ProviderCacheStrategy, protocol: &Protocol) -> bool {
    matches!(
        (strategy, protocol),
        (ProviderCacheStrategy::AnthropicExplicit, Protocol::Anthropic)
            | (ProviderCacheStrategy::OpenAiAutomatic, Protocol::OpenAi)
            | (ProviderCacheStrategy::GoogleExplicit, Protocol::OpenAi)
    )
}

/// freshness policy（搬移自 core-api `configured_provider_cache_evidence_at` 的
/// TTL 判定：now 在 [observed_at, observed_at + ttl] 内为 fresh）。
pub fn is_fresh(observed_at_ms: u64, ttl_seconds: Option<u64>, now_ms: u64) -> bool {
    let Some(ttl) = ttl_seconds else {
        return now_ms >= observed_at_ms;
    };
    let Some(expires_at_ms) = observed_at_ms.checked_add(ttl.checked_mul(1_000).unwrap_or(u64::MAX))
    else {
        return now_ms >= observed_at_ms;
    };
    now_ms >= observed_at_ms && now_ms <= expires_at_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(route: &str, source: &str) -> ProviderCacheEvidence {
        ProviderCacheEvidence {
            route_id: route.to_string(),
            provider_id: "provider-a".to_string(),
            model: "model-a".to_string(),
            strategy: ProviderCacheStrategy::AnthropicExplicit,
            ttl_seconds: Some(3600),
            min_prefix_bytes: 1024,
            source_revision: source.to_string(),
        }
    }

    #[test]
    fn provider_cache_evidence_validation_matches_compress_rules() {
        let source = "a".repeat(64);
        let valid = evidence("route-1", &source);
        assert_eq!(
            validate_provider_cache_evidence("route-1", "provider-a", "model-a", &Protocol::Anthropic, &valid),
            Ok(())
        );
        // route/provider/model/protocol 不匹配 → mismatch。
        assert_eq!(
            validate_provider_cache_evidence("route-x", "provider-a", "model-a", &Protocol::Anthropic, &valid),
            Err(CachePolicyError::ProviderCacheEvidenceMismatch)
        );
        assert_eq!(
            validate_provider_cache_evidence("route-1", "provider-a", "model-a", &Protocol::OpenAi, &valid),
            Err(CachePolicyError::ProviderCacheEvidenceMismatch)
        );
        // 形态非法 → invalid。
        let bad_revision = {
            let mut bad = valid.clone();
            bad.source_revision = "short".to_string();
            bad
        };
        assert_eq!(
            validate_provider_cache_evidence("route-1", "provider-a", "model-a", &Protocol::Anthropic, &bad_revision),
            Err(CachePolicyError::InvalidProviderCacheEvidence)
        );
        let zero_ttl = {
            let mut bad = valid.clone();
            bad.ttl_seconds = Some(0);
            bad
        };
        assert_eq!(
            validate_provider_cache_evidence("route-1", "provider-a", "model-a", &Protocol::Anthropic, &zero_ttl),
            Err(CachePolicyError::InvalidProviderCacheEvidence)
        );
    }

    #[test]
    fn strategy_protocol_matrix() {
        assert!(strategy_matches_protocol(ProviderCacheStrategy::AnthropicExplicit, &Protocol::Anthropic));
        assert!(strategy_matches_protocol(ProviderCacheStrategy::OpenAiAutomatic, &Protocol::OpenAi));
        assert!(strategy_matches_protocol(ProviderCacheStrategy::GoogleExplicit, &Protocol::OpenAi));
        assert!(!strategy_matches_protocol(ProviderCacheStrategy::AnthropicExplicit, &Protocol::OpenAi));
        assert!(!strategy_matches_protocol(ProviderCacheStrategy::OpenAiAutomatic, &Protocol::Anthropic));
    }

    #[test]
    fn ttl_freshness_policy() {
        // 无 TTL：仅拒绝早于观察时间的时钟回拨。
        assert!(is_fresh(100, None, 100));
        assert!(is_fresh(100, None, 999_999));
        assert!(!is_fresh(100, None, 99));
        // 有 TTL：过期即 stale。
        assert!(is_fresh(1_000, Some(1), 1_000));
        assert!(is_fresh(1_000, Some(1), 2_000));
        assert!(!is_fresh(1_000, Some(1), 2_001));
        assert!(!is_fresh(1_000, Some(1), 999));
    }
}
