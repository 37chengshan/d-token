use dtoken_project_policy::{default_policy, RecommendationInput, RuleBasedRecommendationEngine};
use dtoken_types::{CompressionPolicy, OutputVerbosity};

fn assert_savings_are_unmeasured(result: &dtoken_types::CompressionRecommendation) {
    assert_eq!(result.expected_savings_min, 0.0);
    assert_eq!(result.expected_savings_max, 0.0);
    assert!(result
        .risks
        .iter()
        .any(|risk| risk.contains("routed runtime evidence")));
}

#[test]
fn recommends_safe_for_high_risk_task() {
    let engine = RuleBasedRecommendationEngine;
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
    assert_savings_are_unmeasured(&result);
}

#[test]
fn recommends_aggressive_for_large_noisy_context() {
    let engine = RuleBasedRecommendationEngine;
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
    assert_savings_are_unmeasured(&result);
    assert!(!result
        .risks
        .iter()
        .any(|risk| risk.contains("CCR") || risk.contains("raw recovery")));
}

#[test]
fn default_policy_disables_unverified_headroom_capabilities() {
    let policy = default_policy("p3");

    assert_eq!(policy.level, CompressionPolicy::Safe);
    assert!(!policy.enable_ccr);
    assert!(!policy.enable_memory);
    assert!(!policy.enable_learn);
    assert!(!policy.enable_mcp);
    assert!(!policy.enable_output_shaper);
    assert!(!policy.enable_cache_aligner);
    assert!(!policy.enable_code_compressor);
    assert!(!policy.enable_json_smartcrusher);
    assert!(!policy.enable_text_kompress);
    assert_eq!(policy.output_verbosity, OutputVerbosity::Auto);
    assert_eq!(policy.holdout_ratio, None);
    assert!(policy
        .notes
        .as_deref()
        .is_some_and(|notes| notes.contains("Fail-closed default")));
}

#[test]
fn normal_workload_does_not_receive_an_invented_savings_range() {
    let result = RuleBasedRecommendationEngine.recommend(RecommendationInput {
        project_id: "p4".to_string(),
        language_stack: vec!["rust".to_string()],
        avg_request_tokens_7d: 10_000,
        avg_cli_noise_ratio_7d: 0.2,
        retrieve_rate_7d: 0.0,
        failure_rate_7d: 0.0,
        high_risk_task: false,
        manual_override: false,
    });

    assert_eq!(result.recommended_level, CompressionPolicy::Balanced);
    assert_savings_are_unmeasured(&result);
    assert!(result
        .reasons
        .iter()
        .any(|reason| reason.contains("unknown until")));
}
