use dtoken_project_policy::{RecommendationInput, RuleBasedRecommendationEngine};
use dtoken_types::CompressionPolicy;

#[test]
fn recommends_safe_for_high_risk_task() {
    let engine = RuleBasedRecommendationEngine::default();
    let result = engine.recommend(RecommendationInput {
        project_id: "p1".to_string(),
        language_stack: vec!["rust".to_string()],
        avg_request_tokens_7d: 100_000,
        avg_cli_noise_ratio_7d: 0.9,
        retrieve_rate_7d: 0.0,
        failure_rate_7d: 0.0,
        high_risk_task: true,
        manual_override: false,
    });
    assert_eq!(result.recommended_level, CompressionPolicy::Safe);
}

#[test]
fn recommends_aggressive_for_large_noisy_context() {
    let engine = RuleBasedRecommendationEngine::default();
    let result = engine.recommend(RecommendationInput {
        project_id: "p2".to_string(),
        language_stack: vec!["typescript".to_string()],
        avg_request_tokens_7d: 80_000,
        avg_cli_noise_ratio_7d: 0.7,
        retrieve_rate_7d: 0.0,
        failure_rate_7d: 0.0,
        high_risk_task: false,
        manual_override: false,
    });
    assert_eq!(result.recommended_level, CompressionPolicy::Aggressive);
}
