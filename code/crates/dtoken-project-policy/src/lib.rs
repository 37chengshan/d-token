use dtoken_types::{
    CompressionPolicy, CompressionRecommendation, ProjectCompressionPolicy, ProjectProfile,
    RecommendationConfidence, OutputVerbosity,
};
use uuid::Uuid;

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
                risks: vec!["Compression should be conservative for this project/session".to_string()],
                expected_savings_min: 0.10,
                expected_savings_max: 0.35,
            };
        }

        if input.failure_rate_7d > 0.05 || input.retrieve_rate_7d > 0.15 {
            return CompressionRecommendation {
                project_id: input.project_id,
                recommended_level: CompressionPolicy::Safe,
                confidence: RecommendationConfidence::Medium,
                reasons: vec!["Recent failure or retrieve rate is elevated".to_string()],
                risks: vec!["Aggressive compression may hide required details".to_string()],
                expected_savings_min: 0.10,
                expected_savings_max: 0.40,
            };
        }

        if input.avg_request_tokens_7d > 50_000 || input.avg_cli_noise_ratio_7d > 0.60 {
            return CompressionRecommendation {
                project_id: input.project_id,
                recommended_level: CompressionPolicy::Aggressive,
                confidence: RecommendationConfidence::Medium,
                reasons: vec!["Large context or noisy CLI output detected".to_string()],
                risks: vec!["Review details via CCR/raw recovery when needed".to_string()],
                expected_savings_min: 0.45,
                expected_savings_max: 0.80,
            };
        }

        CompressionRecommendation {
            project_id: input.project_id,
            recommended_level: CompressionPolicy::Balanced,
            confidence: RecommendationConfidence::Medium,
            reasons: vec!["Default recommendation for normal coding workload".to_string()],
            risks: vec![],
            expected_savings_min: 0.25,
            expected_savings_max: 0.60,
        }
    }
}

pub fn default_project(name: &str, root_path: &str) -> ProjectProfile {
    let now = "m1-placeholder".to_string();
    ProjectProfile {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        root_path: root_path.to_string(),
        language_stack: vec![],
        agents: vec![],
        default_instance_id: None,
        headroom_policy_id: None,
        rtk_profile_id: None,
        provider_route_id: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

pub fn default_policy(project_id: &str) -> ProjectCompressionPolicy {
    ProjectCompressionPolicy {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        level: CompressionPolicy::Safe,
        recommended_level: CompressionPolicy::Safe,
        recommendation_confidence: RecommendationConfidence::Medium,
        enable_ccr: true,
        enable_memory: false,
        enable_learn: false,
        enable_mcp: false,
        enable_output_shaper: false,
        enable_cache_aligner: true,
        enable_code_compressor: true,
        enable_json_smartcrusher: true,
        enable_text_kompress: true,
        output_verbosity: OutputVerbosity::Auto,
        holdout_ratio: Some(0.05),
        notes: Some("M1 default safe policy".to_string()),
    }
}

pub fn demo_projects() -> Vec<ProjectProfile> {
    vec![default_project("Demo Project", ".")]
}
