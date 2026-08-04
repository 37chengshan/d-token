use dtoken_types::{
    CompressionPolicy, CompressionRecommendation, OutputVerbosity, ProjectCompressionPolicy,
    RecommendationConfidence,
};
use uuid::Uuid;

pub const PROJECT_POLICY_CAPABILITY_REVISION: &str = "dtoken-project-policy-v1";

const UNMEASURED_SAVINGS_REASON: &str =
    "Expected savings are unknown until this project, route, and compressor are measured";
const UNMEASURED_SAVINGS_RISK: &str =
    "Do not present a savings range until routed runtime evidence is available";

#[derive(Debug, Clone)]
pub struct RecommendationInput {
    pub project_id: String,
    pub language_stack: Vec<String>,
    pub avg_request_tokens_7d: u64,
    pub avg_cli_noise_ratio_7d: f32,
    pub retrieve_rate_7d: f32,
    pub failure_rate_7d: f32,
    pub high_risk_task: bool,
    pub manual_override: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RuleBasedRecommendationEngine;

impl RuleBasedRecommendationEngine {
    pub fn recommend(&self, input: RecommendationInput) -> CompressionRecommendation {
        if input.high_risk_task || input.manual_override {
            return CompressionRecommendation {
                project_id: input.project_id,
                recommended_level: CompressionPolicy::Safe,
                confidence: RecommendationConfidence::High,
                reasons: vec!["High-risk task or manual override detected".to_string()],
                risks: vec![
                    "Compression should be conservative for this project/session".to_string(),
                    UNMEASURED_SAVINGS_RISK.to_string(),
                ],
                expected_savings_min: 0.0,
                expected_savings_max: 0.0,
            };
        }

        if input.failure_rate_7d > 0.05 || input.retrieve_rate_7d > 0.15 {
            return CompressionRecommendation {
                project_id: input.project_id,
                recommended_level: CompressionPolicy::Safe,
                confidence: RecommendationConfidence::Medium,
                reasons: vec![
                    "Recent failure or retrieval-metadata rate is elevated".to_string(),
                    UNMEASURED_SAVINGS_REASON.to_string(),
                ],
                risks: vec![
                    "Aggressive compression may hide required details".to_string(),
                    UNMEASURED_SAVINGS_RISK.to_string(),
                ],
                expected_savings_min: 0.0,
                expected_savings_max: 0.0,
            };
        }

        if input.avg_request_tokens_7d > 50_000 || input.avg_cli_noise_ratio_7d > 0.60 {
            return CompressionRecommendation {
                project_id: input.project_id,
                recommended_level: CompressionPolicy::Aggressive,
                confidence: RecommendationConfidence::Medium,
                reasons: vec![
                    "Large context or noisy CLI output detected".to_string(),
                    UNMEASURED_SAVINGS_REASON.to_string(),
                ],
                risks: vec![
                    "Aggressive compression requires project-specific quality and fallback validation"
                        .to_string(),
                    UNMEASURED_SAVINGS_RISK.to_string(),
                ],
                expected_savings_min: 0.0,
                expected_savings_max: 0.0,
            };
        }

        CompressionRecommendation {
            project_id: input.project_id,
            recommended_level: CompressionPolicy::Balanced,
            confidence: RecommendationConfidence::Medium,
            reasons: vec![
                "Default recommendation for normal coding workload".to_string(),
                UNMEASURED_SAVINGS_REASON.to_string(),
            ],
            risks: vec![UNMEASURED_SAVINGS_RISK.to_string()],
            expected_savings_min: 0.0,
            expected_savings_max: 0.0,
        }
    }
}

pub fn default_policy(project_id: &str) -> ProjectCompressionPolicy {
    ProjectCompressionPolicy {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        level: CompressionPolicy::Safe,
        recommended_level: CompressionPolicy::Safe,
        recommendation_confidence: RecommendationConfidence::Medium,
        enable_ccr: false,
        enable_memory: false,
        enable_learn: false,
        enable_mcp: false,
        enable_output_shaper: false,
        enable_cache_aligner: false,
        enable_code_compressor: false,
        enable_json_smartcrusher: false,
        enable_text_kompress: false,
        output_verbosity: OutputVerbosity::Auto,
        holdout_ratio: None,
        notes: Some(
            "Fail-closed default: Headroom-derived capabilities stay disabled until the runtime reports verified support and the user explicitly enables them"
                .to_string(),
        ),
    }
}
