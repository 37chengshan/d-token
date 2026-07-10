// OpticalTransformPlan builder：组装 shadow-only plan，产出 public summary。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §20-21。

use crate::optical::applicability::evaluate_applicability;
use crate::optical::capability::resolve_model_capability;
use crate::optical::economics::BlockEconomicsPlan;
use crate::optical::inventory::{
    build_anthropic_inventory, BlockKind, BlockRef, InitialProtection,
};
use crate::optical::render_geometry::{anthropic_dense_v1, geometry_for_capability};
use crate::optical::risk::{classify_block, RiskPartitionPlan, RiskTier};
use crate::optical::semantic_priority::plan_semantic_priority;
use crate::optical::validate::{decide_final_status, validate_shadow_plan};
use dtoken_types::{
    CachePublicSummary, OpticalApplicabilityInput, OpticalBucket, OpticalEconomicsPublicSummary,
    OpticalGateDecision, OpticalGlobalPolicy, OpticalMode, OpticalPlanCounters,
    OpticalPlanPublicSummary, OpticalPlanStatus, RouteOpticalPolicy,
};

#[derive(Debug, Clone)]
pub struct CandidateDecision {
    pub block_id: String,
    pub bucket: OpticalBucket,
    pub action: CandidateAction,
    pub reason: String,
    pub chars: usize,
    pub economics: Option<BlockEconomicsPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateAction {
    PreserveText,
    ShadowEstimateOnly,
    SkipTooSmall,
    SkipUnprofitable,
    SkipExactnessRisk,
    SkipSecretRisk,
    SkipStructuralRisk,
    SkipCacheUnstable,
}

pub fn build_shadow_plan(
    request_id: &str,
    body: &serde_json::Value,
    global_policy: &OpticalGlobalPolicy,
    route_policy: &RouteOpticalPolicy,
    applicability_input: &OpticalApplicabilityInput,
) -> OpticalPlanPublicSummary {
    let applicability = evaluate_applicability(applicability_input);
    let mut gates = vec![OpticalGateDecision {
        name: "applicability".into(),
        passed: applicability.eligible_for_shadow,
        reason: format!("{:?}", applicability.reason),
    }];
    if !applicability.eligible_for_shadow {
        return shadow_summary(
            request_id,
            OpticalPlanStatus::Ineligible,
            gates,
            Default::default(),
            Vec::new(),
        );
    }
    let endpoint = applicability.endpoint.clone().unwrap_or_default();
    let capability = resolve_model_capability(
        applicability_input.protocol.as_ref(),
        applicability_input.provider_id.as_deref(),
        applicability_input.model.as_deref(),
        endpoint,
    );
    gates.push(OpticalGateDecision {
        name: "capability".into(),
        passed: !matches!(capability.status, dtoken_types::CapabilityStatus::Blocked),
        reason: format!("{:?}", capability.status),
    });
    let inventory = build_anthropic_inventory(body);
    let semantic = plan_semantic_priority(&inventory);
    let _profile = geometry_for_capability(&capability).unwrap_or_else(anthropic_dense_v1);
    let mut counters = OpticalPlanCounters::default();
    let mut bucket_chars: std::collections::HashMap<OpticalBucket, (usize, usize)> =
        std::collections::HashMap::new();
    for block in &inventory.blocks {
        if block.protection != InitialProtection::Candidate {
            continue;
        }
        counters.candidate_blocks += 1;
        let bucket = bucket_for_block(block);
        let risk = classify_block(&format!("block-{}-chars-{}", block.block_id, block.chars));
        let (action, _reason) = decide_candidate(
            block,
            &risk,
            &capability,
            global_policy,
            route_policy,
            inventory.current_user_turn,
            inventory.recent_tail_start,
        );
        if action == CandidateAction::ShadowEstimateOnly {
            counters.preserved_blocks += 1;
        } else {
            counters.skipped_blocks += 1;
        }
        if matches!(risk.tier, RiskTier::ExactTextRequired) {
            counters.exact_spans += 1;
        }
        if matches!(risk.tier, RiskTier::SecretNoDuplicate) {
            counters.secret_spans += 1;
        }
        let e = bucket_chars.entry(bucket).or_insert((0, 0));
        e.0 += 1;
        e.1 += block.chars;
    }
    let validation = validate_shadow_plan(
        true,
        false,
        true,
        semantic.current_user_always_text,
        true,
        true,
    );
    let final_status = decide_final_status(validation.valid, false);
    let buckets = bucket_chars
        .into_iter()
        .map(|(b, (n, c))| dtoken_types::OpticalBucketSummary {
            bucket: b,
            blocks: n,
            chars: c,
            estimated_net_saved_tokens: 0,
        })
        .collect();
    shadow_summary(request_id, final_status, gates, counters, buckets)
}

fn bucket_for_block(block: &BlockRef) -> OpticalBucket {
    match block.kind {
        BlockKind::ToolResult => OpticalBucket::ToolResultLog,
        BlockKind::Text => OpticalBucket::ToolResultProse,
        BlockKind::SystemPrompt => OpticalBucket::StaticSlab,
        BlockKind::ToolDefinition => OpticalBucket::SchemaAnnotation,
        _ => OpticalBucket::ToolResultProse,
    }
}

fn decide_candidate(
    block: &BlockRef,
    risk: &RiskPartitionPlan,
    capability: &dtoken_types::OpticalModelCapability,
    global: &OpticalGlobalPolicy,
    route: &RouteOpticalPolicy,
    current_user_turn: Option<usize>,
    recent_tail_start: usize,
) -> (CandidateAction, String) {
    if block.chars < 2000 {
        return (CandidateAction::SkipTooSmall, "block_too_small".into());
    }
    if matches!(risk.tier, RiskTier::SecretNoDuplicate) {
        return (
            CandidateAction::SkipSecretRisk,
            "secret_no_duplicate".into(),
        );
    }
    if matches!(
        risk.tier,
        RiskTier::StructuralUnsafe | RiskTier::BlockUnsafe
    ) {
        return (
            CandidateAction::SkipStructuralRisk,
            risk.skip_reason.clone().unwrap_or_default(),
        );
    }
    if matches!(risk.tier, RiskTier::ExactTextRequired) {
        return (
            CandidateAction::SkipExactnessRisk,
            "exact_text_required".into(),
        );
    }
    if !global.enabled || !route.enabled {
        return (CandidateAction::PreserveText, "optical_disabled".into());
    }
    if let Some(turn_idx) = block.turn_index {
        if Some(turn_idx) == current_user_turn {
            return (
                CandidateAction::PreserveText,
                "current_user_always_text".into(),
            );
        }
        if turn_idx >= recent_tail_start {
            return (
                CandidateAction::PreserveText,
                "recent_tail_always_text".into(),
            );
        }
    }
    if matches!(
        capability.status,
        dtoken_types::CapabilityStatus::Unknown
            | dtoken_types::CapabilityStatus::Blocked
            | dtoken_types::CapabilityStatus::ShadowOnly
    ) {
        return (
            CandidateAction::ShadowEstimateOnly,
            "shadow_estimate_only".into(),
        );
    }
    (
        CandidateAction::ShadowEstimateOnly,
        "eligible_shadow".into(),
    )
}

fn shadow_summary(
    plan_id: &str,
    status: OpticalPlanStatus,
    gates: Vec<OpticalGateDecision>,
    counters: OpticalPlanCounters,
    buckets: Vec<dtoken_types::OpticalBucketSummary>,
) -> OpticalPlanPublicSummary {
    OpticalPlanPublicSummary {
        plan_id: plan_id.into(),
        mode: OpticalMode::ShadowPreview,
        final_status: status,
        gates,
        counters,
        buckets,
        cache: CachePublicSummary::default(),
        economics: OpticalEconomicsPublicSummary::default(),
        warnings: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_types::Protocol;
    use serde_json::json;
    fn make_input() -> OpticalApplicabilityInput {
        OpticalApplicabilityInput {
            method: Some("POST".into()),
            endpoint_path: Some("/v1/messages".into()),
            body_bytes: Some(256),
            protocol: Some(Protocol::Anthropic),
            provider_id: Some("anthropic-main".into()),
            route_id: Some("route-main".into()),
            model: Some("claude-fable-5".into()),
            global_enabled: true,
            route_enabled: true,
        }
    }
    #[test]
    fn shadow_plan_body_unchanged_and_redacted() {
        let body = json!({"messages":[{"role":"user","content":"hello world this is a test message that is long enough"}]});
        let global = OpticalGlobalPolicy::default();
        let route = RouteOpticalPolicy {
            enabled: true,
            mode: OpticalMode::ShadowPreview,
            ..Default::default()
        };
        let summary = build_shadow_plan("plan-1", &body, &global, &route, &make_input());
        let encoded = serde_json::to_string(&summary).unwrap();
        assert!(!encoded.contains("raw_prompt"));
        assert!(!encoded.contains("request_body"));
        assert!(!encoded.contains("base64"));
    }
    #[test]
    fn ineligible_request_returns_ineligible_status() {
        let body = json!({});
        let global = OpticalGlobalPolicy::default();
        let route = RouteOpticalPolicy::default();
        let mut input = make_input();
        input.body_bytes = Some(0);
        let summary = build_shadow_plan("plan-2", &body, &global, &route, &input);
        assert_eq!(summary.final_status, OpticalPlanStatus::Ineligible);
    }
}
