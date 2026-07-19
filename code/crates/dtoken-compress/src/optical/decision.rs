// OpticalTransformPlan builder：组装 shadow-only plan，产出 metadata-only public summary。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §20-22。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::content_router::{route_content, ContentKind};
use crate::optical::applicability::evaluate_applicability_with_policies_and_registry;
use crate::optical::capability::OpticalCapabilityRegistry;
use crate::optical::context_graph::{build_context_graph, ContextGraph};
use crate::optical::economics::{evaluate_block_economics, BlockEconomicsPlan};
use crate::optical::inventory::{
    build_anthropic_inventory_with_arena, build_openai_chat_inventory_with_arena,
    build_openai_responses_inventory_with_arena, BlockKind, BlockRef, InitialProtection,
};
use crate::optical::plan_ir::{ContextOptimizationPlan, PlanCandidateEvidence};
use crate::optical::render_geometry::{
    anthropic_dense_v2, geometry_for_capability, images_required,
};
use crate::optical::risk::{classify_block, RiskPartitionPlan, RiskTier};
use crate::optical::semantic_priority::plan_semantic_priority;
use crate::optical::text_arena::TextArena;
use crate::optical::validate::{
    decide_final_status, validate_shadow_plan_input, CandidateValidationEvidence,
    ShadowPlanValidationInput,
};
use crate::optical::volatility::plan_volatility;
use dtoken_types::{
    CachePublicSummary, CapabilityStatus, OpticalApplicabilityInput, OpticalBucket,
    OpticalBucketSummary, OpticalEconomicsPublicSummary, OpticalGateDecision, OpticalGlobalPolicy,
    OpticalMode, OpticalPlanCounters, OpticalPlanPublicSummary, OpticalPlanStatus,
    RouteOpticalPolicy,
};

#[derive(Debug, Clone)]
pub struct CandidateDecision {
    pub block_id: String,
    pub source_path: String,
    pub bucket: OpticalBucket,
    pub action: CandidateAction,
    pub reason: String,
    pub chars: usize,
    pub estimated_images: usize,
    pub economics: Option<BlockEconomicsPlan>,
    pub content_kind: ContentKind,
    pub risk_tier: Option<RiskTier>,
    pub cache_stable: bool,
    pub stable_digest_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateAction {
    PreserveText,
    ShadowEstimateOnly,
    SkipTooSmall,
    SkipUnprofitable,
    SkipExactnessRisk,
    SkipDecisionSensitive,
    SkipSecretRisk,
    SkipStructuralRisk,
    SkipCacheUnstable,
    SkipUnsupportedModel,
    SkipOverBudget,
}

impl CandidateAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreserveText => "preserve_text",
            Self::ShadowEstimateOnly => "shadow_estimate_only",
            Self::SkipTooSmall => "skip_too_small",
            Self::SkipUnprofitable => "skip_unprofitable",
            Self::SkipExactnessRisk => "skip_exactness_risk",
            Self::SkipDecisionSensitive => "skip_decision_sensitive",
            Self::SkipSecretRisk => "skip_secret_risk",
            Self::SkipStructuralRisk => "skip_structural_risk",
            Self::SkipCacheUnstable => "skip_cache_unstable",
            Self::SkipUnsupportedModel => "skip_unsupported_model",
            Self::SkipOverBudget => "skip_over_budget",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShadowPlanBuild {
    pub summary: OpticalPlanPublicSummary,
    pub candidates: Vec<CandidateDecision>,
    pub plan: ContextOptimizationPlan,
}

#[derive(Default)]
struct EconomicsAccumulator {
    text_tokens_before: f64,
    image_tokens_after: f64,
    sidecar_tokens: f64,
    cache_burn_tokens: f64,
    net_saved_tokens: f64,
    conservative_net_saved_tokens: f64,
    optimistic_net_saved_tokens: f64,
}

pub fn build_shadow_plan(
    request_id: &str,
    body: &serde_json::Value,
    global_policy: &OpticalGlobalPolicy,
    route_policy: &RouteOpticalPolicy,
    applicability_input: &OpticalApplicabilityInput,
) -> OpticalPlanPublicSummary {
    build_shadow_plan_detailed(
        request_id,
        body,
        global_policy,
        route_policy,
        applicability_input,
    )
    .summary
}

pub fn build_shadow_plan_detailed(
    request_id: &str,
    body: &serde_json::Value,
    global_policy: &OpticalGlobalPolicy,
    route_policy: &RouteOpticalPolicy,
    applicability_input: &OpticalApplicabilityInput,
) -> ShadowPlanBuild {
    let registry = OpticalCapabilityRegistry::default();
    build_shadow_plan_detailed_with_registry(
        request_id,
        body,
        global_policy,
        route_policy,
        applicability_input,
        &registry,
    )
}

pub fn build_shadow_plan_detailed_with_registry(
    request_id: &str,
    body: &serde_json::Value,
    global_policy: &OpticalGlobalPolicy,
    route_policy: &RouteOpticalPolicy,
    applicability_input: &OpticalApplicabilityInput,
    registry: &OpticalCapabilityRegistry,
) -> ShadowPlanBuild {
    let plan_started = Instant::now();
    let plan_deadline = Duration::from_millis(global_policy.max_plan_time_ms.max(1));
    let applicability = evaluate_applicability_with_policies_and_registry(
        applicability_input,
        global_policy,
        route_policy,
        registry,
    );
    let mut gates = vec![OpticalGateDecision {
        name: "applicability".into(),
        passed: applicability.eligible_for_shadow,
        reason: format!("{:?}", applicability.reason),
    }];
    if !applicability.eligible_for_shadow {
        let summary = shadow_summary(
            request_id,
            OpticalPlanStatus::Ineligible,
            gates,
            Default::default(),
            Vec::new(),
            Default::default(),
            Vec::new(),
        );
        let plan = ContextOptimizationPlan::new(
            request_id,
            body,
            body,
            None,
            None,
            Default::default(),
            Vec::new(),
            summary.clone(),
        );
        return ShadowPlanBuild {
            summary,
            candidates: Vec::new(),
            plan,
        };
    }

    let endpoint = applicability.endpoint.clone().unwrap_or_default();
    let resolved_capability =
        super::capability::resolve_model_capability_record_for_route_with_registry(
            applicability_input.protocol.as_ref(),
            applicability_input.provider_id.as_deref(),
            applicability_input.route_id.as_deref(),
            applicability_input.model.as_deref(),
            endpoint.clone(),
            registry,
        );
    let capability = resolved_capability.capability.clone();
    gates.push(OpticalGateDecision {
        name: "capability".into(),
        passed: capability.status != CapabilityStatus::Blocked,
        reason: match (
            &resolved_capability.snapshot_id,
            resolved_capability.revision,
        ) {
            (Some(snapshot_id), Some(revision)) => {
                format!("{:?}:{snapshot_id}@{revision}", capability.status)
            }
            _ => format!("{:?}:unverified", capability.status),
        },
    });

    let mut warnings = Vec::new();
    let geometry = geometry_for_capability(&capability).or_else(|| {
        if capability.supports_anthropic_image_block {
            warnings.push("geometry_fallback_profile".to_string());
            Some(anthropic_dense_v2())
        } else {
            None
        }
    });
    let mut arena = TextArena::new(request_id);
    let inventory = match endpoint {
        dtoken_types::OpticalEndpoint::AnthropicMessages => {
            build_anthropic_inventory_with_arena(body, &mut arena)
        }
        dtoken_types::OpticalEndpoint::OpenAiChatCompletions => {
            build_openai_chat_inventory_with_arena(body, &mut arena)
        }
        dtoken_types::OpticalEndpoint::OpenAiResponses => {
            build_openai_responses_inventory_with_arena(body, &mut arena)
        }
        dtoken_types::OpticalEndpoint::Unsupported => {
            build_anthropic_inventory_with_arena(body, &mut arena)
        }
    };
    let graph = build_context_graph(&inventory);
    let semantic = plan_semantic_priority(&inventory);
    let mut plan_budget_exceeded = plan_started.elapsed() > plan_deadline;
    if plan_budget_exceeded {
        warnings.push("plan_deadline_exceeded".to_string());
    }

    let mut counters = OpticalPlanCounters::default();
    let mut bucket_totals: HashMap<OpticalBucket, (usize, usize, isize)> = HashMap::new();
    let mut economics_totals = EconomicsAccumulator::default();
    let mut candidates = Vec::new();

    for block in &inventory.blocks {
        if plan_started.elapsed() > plan_deadline {
            plan_budget_exceeded = true;
            warnings.push("plan_deadline_exceeded".to_string());
            break;
        }
        if block.protection != InitialProtection::Candidate {
            continue;
        }
        counters.candidate_blocks += 1;

        let raw_text = block
            .text_ref
            .as_ref()
            .and_then(|text_ref| arena.get(text_ref));
        let bucket = bucket_for_block(block, raw_text);
        let risk = raw_text.map(classify_block);
        let content_kind = raw_text
            .map(route_content)
            .map_or(ContentKind::Unknown, |route| route.content_kind);
        let volatility = raw_text.map(plan_volatility);
        let cache_stable = volatility
            .as_ref()
            .is_some_and(|plan| plan.stable_digest.is_some() && plan.volatile_chars == 0);
        let estimated_images = geometry
            .as_ref()
            .and_then(|profile| images_required(profile, block.chars))
            .unwrap_or(0);
        let economics = geometry.as_ref().and_then(|profile| {
            (estimated_images > 0).then(|| {
                evaluate_block_economics(
                    bucket.clone(),
                    block.chars,
                    estimated_images,
                    profile,
                    0.0,
                    1.25,
                    0.1,
                )
            })
        });

        let (action, reason) = decide_candidate(
            block,
            raw_text,
            risk.as_ref(),
            cache_stable,
            economics.as_ref(),
            estimated_images,
            &capability,
            &graph,
            global_policy,
            route_policy,
            inventory.current_user_turn,
            inventory.recent_tail_start,
        );

        let selected_for_shadow = action == CandidateAction::ShadowEstimateOnly;
        match action {
            CandidateAction::PreserveText | CandidateAction::ShadowEstimateOnly => {
                counters.preserved_blocks += 1;
            }
            _ => counters.skipped_blocks += 1,
        }
        if let Some(risk) = &risk {
            if matches!(risk.tier, RiskTier::ExactTextRequired) {
                counters.exact_spans += 1;
            }
            if matches!(risk.tier, RiskTier::SecretNoDuplicate) {
                counters.secret_spans += 1;
            }
        }
        if selected_for_shadow {
            counters.estimated_images = counters.estimated_images.saturating_add(estimated_images);
        }

        let net_saved = if selected_for_shadow {
            economics
                .as_ref()
                .map(|plan| f64_to_isize(plan.net_saved_tokens))
                .unwrap_or(0)
        } else {
            0
        };
        let entry = bucket_totals.entry(bucket.clone()).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 = entry.1.saturating_add(block.chars);
        entry.2 = entry.2.saturating_add(net_saved);

        if selected_for_shadow {
            if let Some(plan) = &economics {
                economics_totals.text_tokens_before += plan.text_tokens_before;
                economics_totals.image_tokens_after += plan.image_tokens_after;
                economics_totals.sidecar_tokens += plan.sidecar_tokens;
                economics_totals.cache_burn_tokens += plan.cache_burn_image_side;
                economics_totals.net_saved_tokens += plan.net_saved_tokens;
                economics_totals.conservative_net_saved_tokens +=
                    plan.estimate_range.conservative_net_saved_tokens;
                economics_totals.optimistic_net_saved_tokens +=
                    plan.estimate_range.optimistic_net_saved_tokens;
            }
        }

        candidates.push(CandidateDecision {
            block_id: block.block_id.clone(),
            source_path: block.source_path.clone(),
            bucket,
            action,
            reason,
            chars: block.chars,
            estimated_images,
            economics,
            content_kind,
            risk_tier: risk.as_ref().map(|plan| plan.tier.clone()),
            cache_stable,
            stable_digest_present: volatility
                .as_ref()
                .is_some_and(|plan| plan.stable_digest.is_some()),
        });
    }

    let buckets = bucket_totals
        .into_iter()
        .map(
            |(bucket, (blocks, chars, estimated_net_saved_tokens))| OpticalBucketSummary {
                bucket,
                blocks,
                chars,
                estimated_net_saved_tokens,
            },
        )
        .collect::<Vec<_>>();
    gates.push(OpticalGateDecision {
        name: "plan_budget".to_string(),
        passed: !plan_budget_exceeded,
        reason: if plan_budget_exceeded {
            "plan_deadline_exceeded".to_string()
        } else {
            "within_plan_deadline".to_string()
        },
    });
    let economics_summary = OpticalEconomicsPublicSummary {
        text_tokens_before: f64_to_usize(economics_totals.text_tokens_before),
        image_tokens_after: f64_to_usize(economics_totals.image_tokens_after),
        sidecar_tokens: f64_to_usize(economics_totals.sidecar_tokens),
        cache_burn_tokens: f64_to_usize(economics_totals.cache_burn_tokens),
        net_saved_tokens: f64_to_isize(economics_totals.net_saved_tokens),
        conservative_net_saved_tokens: f64_to_isize(economics_totals.conservative_net_saved_tokens),
        expected_net_saved_tokens: f64_to_isize(economics_totals.net_saved_tokens),
        optimistic_net_saved_tokens: f64_to_isize(economics_totals.optimistic_net_saved_tokens),
    };
    let geometry_is_sufficient = geometry.is_some()
        || candidates.iter().all(|candidate| {
            candidate.action != CandidateAction::ShadowEstimateOnly
                && candidate.estimated_images == 0
        });
    gates.push(OpticalGateDecision {
        name: "geometry".to_string(),
        passed: geometry_is_sufficient,
        reason: geometry
            .as_ref()
            .map(|profile| profile.profile_id.clone())
            .unwrap_or_else(|| {
                if geometry_is_sufficient {
                    "not_required_for_selected_actions".to_string()
                } else {
                    "geometry_unavailable".to_string()
                }
            }),
    });

    let mut summary = shadow_summary(
        request_id,
        OpticalPlanStatus::Disabled,
        gates,
        counters,
        buckets,
        economics_summary,
        warnings,
    );
    let recent_tail_always_text = inventory
        .blocks
        .iter()
        .filter(|block| {
            block
                .turn_index
                .map(|turn| turn >= inventory.recent_tail_start)
                .unwrap_or(false)
                && block.protection == InitialProtection::Candidate
        })
        .all(|block| {
            candidates
                .iter()
                .find(|candidate| candidate.block_id == block.block_id)
                .map(|candidate| candidate.action == CandidateAction::PreserveText)
                .unwrap_or(false)
        });
    let evidence = candidates
        .iter()
        .map(|candidate| CandidateValidationEvidence {
            action: candidate.action.as_str(),
            reason: candidate.reason.as_str(),
        })
        .collect::<Vec<_>>();
    let validation = validate_shadow_plan_input(&ShadowPlanValidationInput {
        original_body: body,
        planned_body: body,
        public_summary: &summary,
        candidates: &evidence,
        current_user_always_text: semantic.current_user_always_text,
        recent_tail_always_text,
        geometry_exists: geometry_is_sufficient,
    });
    summary.final_status = if plan_budget_exceeded {
        OpticalPlanStatus::FailedClosed
    } else {
        decide_final_status(validation.valid, false)
    };
    summary.gates.push(OpticalGateDecision {
        name: "shadow_validation".to_string(),
        passed: validation.valid,
        reason: if validation.valid {
            "validated_shadow_plan".to_string()
        } else {
            validation.failures.join(",")
        },
    });

    let plan_candidates = candidates
        .iter()
        .map(|candidate| PlanCandidateEvidence {
            block_id: candidate.block_id.clone(),
            source_path: candidate.source_path.clone(),
            action: candidate.action.as_str().to_string(),
            reason: candidate.reason.clone(),
            chars: candidate.chars,
            estimated_images: candidate.estimated_images,
            content_kind: candidate.content_kind,
            risk_tier: candidate.risk_tier.clone(),
            cache_stable: candidate.cache_stable,
            stable_digest_present: candidate.stable_digest_present,
        })
        .collect();
    let plan = ContextOptimizationPlan::new(
        request_id,
        body,
        body,
        resolved_capability.snapshot_id,
        resolved_capability.revision,
        graph.summary(),
        plan_candidates,
        summary.clone(),
    );

    ShadowPlanBuild {
        summary,
        candidates,
        plan,
    }
}

fn bucket_for_block(block: &BlockRef, raw_text: Option<&str>) -> OpticalBucket {
    if block.kind != BlockKind::ToolResult {
        return OpticalBucket::ToolResultProse;
    }
    let Some(text) = raw_text else {
        return OpticalBucket::ToolResultProse;
    };
    if serde_json::from_str::<serde_json::Value>(text).is_ok() {
        return OpticalBucket::ToolResultJson;
    }
    let line_count = text.lines().count();
    let table_lines = text
        .lines()
        .filter(|line| line.matches('|').count() >= 2)
        .count();
    if table_lines >= 3 {
        return OpticalBucket::ToolResultTable;
    }
    if line_count >= 8
        || text.contains("INFO ")
        || text.contains("WARN ")
        || text.contains("ERROR ")
    {
        return OpticalBucket::ToolResultLog;
    }
    OpticalBucket::ToolResultProse
}

#[allow(clippy::too_many_arguments)]
fn decide_candidate(
    block: &BlockRef,
    raw_text: Option<&str>,
    risk: Option<&RiskPartitionPlan>,
    cache_stable: bool,
    economics: Option<&BlockEconomicsPlan>,
    estimated_images: usize,
    capability: &dtoken_types::OpticalModelCapability,
    graph: &ContextGraph,
    global: &OpticalGlobalPolicy,
    route: &RouteOpticalPolicy,
    current_user_turn: Option<usize>,
    recent_tail_start: usize,
) -> (CandidateAction, String) {
    if block.kind != BlockKind::ToolResult {
        return (
            CandidateAction::PreserveText,
            "phase_a_tool_result_only".to_string(),
        );
    }
    if !graph.is_closed_tool_result(&block.block_id) {
        return (
            CandidateAction::SkipStructuralRisk,
            if graph.is_orphan_tool_result(&block.block_id) {
                "orphan_tool_result".to_string()
            } else {
                "tool_dependency_not_closed".to_string()
            },
        );
    }
    if !graph.is_closed_historical_tool_result(&block.block_id, recent_tail_start) {
        return (
            CandidateAction::PreserveText,
            "tool_result_not_historical".to_string(),
        );
    }
    if let Some(turn_idx) = block.turn_index {
        if Some(turn_idx) == current_user_turn {
            return (
                CandidateAction::PreserveText,
                "current_user_always_text".to_string(),
            );
        }
        if turn_idx >= recent_tail_start {
            return (
                CandidateAction::PreserveText,
                "recent_tail_always_text".to_string(),
            );
        }
    }
    if raw_text.is_none() || risk.is_none() {
        return (
            CandidateAction::SkipStructuralRisk,
            "missing_request_local_text".to_string(),
        );
    }
    if block.chars < 2_000 {
        return (CandidateAction::SkipTooSmall, "block_too_small".into());
    }
    let risk = risk.expect("checked above");
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
            risk.skip_reason
                .clone()
                .unwrap_or_else(|| "structural_unsafe".to_string()),
        );
    }
    if matches!(risk.tier, RiskTier::ExactTextRequired) {
        return (
            CandidateAction::SkipExactnessRisk,
            "exact_text_required".into(),
        );
    }
    if matches!(risk.tier, RiskTier::DecisionSensitive) {
        return (
            CandidateAction::SkipDecisionSensitive,
            "decision_sensitive".into(),
        );
    }
    if !cache_stable {
        return (
            CandidateAction::SkipCacheUnstable,
            "volatile_or_unstable_source".into(),
        );
    }
    if matches!(
        capability.status,
        CapabilityStatus::Unknown | CapabilityStatus::Blocked | CapabilityStatus::ShadowOnly
    ) {
        return (
            CandidateAction::SkipUnsupportedModel,
            "capability_not_verified_for_optical_estimation".into(),
        );
    }
    let capability_image_limit = if capability.max_images_per_request == 0 {
        usize::MAX
    } else {
        capability.max_images_per_request
    };
    let image_limit = global.max_images_per_request.min(capability_image_limit);
    if estimated_images == 0 || estimated_images > image_limit {
        return (
            CandidateAction::SkipOverBudget,
            "image_budget_exceeded".into(),
        );
    }
    let Some(economics) = economics else {
        return (
            CandidateAction::SkipUnprofitable,
            "economics_unavailable".into(),
        );
    };
    if !economics.profitable || economics.net_saved_tokens < route.min_net_saved_tokens as f64 {
        return (
            CandidateAction::SkipUnprofitable,
            format!("{:?}", economics.reason),
        );
    }
    (
        CandidateAction::ShadowEstimateOnly,
        if capability.status == CapabilityStatus::Supported {
            "eligible_shadow_supported_model".into()
        } else {
            "eligible_shadow_non_active_model".into()
        },
    )
}

fn f64_to_usize(value: f64) -> usize {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.round().min(usize::MAX as f64) as usize
    }
}

fn f64_to_isize(value: f64) -> isize {
    if !value.is_finite() {
        0
    } else {
        value.round().clamp(isize::MIN as f64, isize::MAX as f64) as isize
    }
}

fn shadow_summary(
    plan_id: &str,
    status: OpticalPlanStatus,
    gates: Vec<OpticalGateDecision>,
    counters: OpticalPlanCounters,
    buckets: Vec<OpticalBucketSummary>,
    economics: OpticalEconomicsPublicSummary,
    warnings: Vec<String>,
) -> OpticalPlanPublicSummary {
    OpticalPlanPublicSummary {
        plan_id: plan_id.into(),
        mode: OpticalMode::ShadowPreview,
        final_status: status,
        gates,
        counters,
        buckets,
        cache: CachePublicSummary::default(),
        economics,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_types::Protocol;
    use serde_json::json;

    fn make_input(body: &serde_json::Value) -> OpticalApplicabilityInput {
        OpticalApplicabilityInput {
            method: Some("POST".into()),
            endpoint_path: Some("/v1/messages".into()),
            body_bytes: Some(serde_json::to_vec(body).unwrap().len()),
            protocol: Some(Protocol::Anthropic),
            provider_id: Some("anthropic-main".into()),
            route_id: Some("route-main".into()),
            model: Some("claude-fable-5".into()),
            global_enabled: true,
            route_enabled: true,
        }
    }

    fn shadow_policies() -> (OpticalGlobalPolicy, RouteOpticalPolicy) {
        (
            OpticalGlobalPolicy {
                enabled: true,
                ..Default::default()
            },
            RouteOpticalPolicy {
                enabled: true,
                mode: OpticalMode::ShadowPreview,
                allow_tool_result: true,
                min_net_saved_tokens: 64,
                ..Default::default()
            },
        )
    }

    #[test]
    fn shadow_plan_body_unchanged_and_redacted() {
        let body = json!({"messages":[{"role":"user","content":"hello world"}]});
        let (global, route) = shadow_policies();
        let summary = build_shadow_plan("plan-1", &body, &global, &route, &make_input(&body));
        let encoded = serde_json::to_string(&summary).unwrap();
        assert!(!encoded.contains("raw_prompt"));
        assert!(!encoded.contains("request_body"));
        assert!(!encoded.contains("base64"));
        assert_eq!(summary.final_status, OpticalPlanStatus::ValidatedShadowPlan);
    }

    #[test]
    fn responses_shadow_plan_uses_responses_inventory_without_active_patch_targets() {
        let output = "INFO stable repeated line\n".repeat(100);
        let body = json!({"instructions":"keep exact rules","input":[
            {"type":"function_call","id":"fc-1","call_id":"call-1","name":"read","arguments":"{}"},
            {"type":"function_call_output","call_id":"call-1","output":output},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"continue"}]},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"working"}]}
        ]});
        let input = OpticalApplicabilityInput {
            method: Some("POST".into()),
            endpoint_path: Some("/v1/responses".into()),
            body_bytes: Some(serde_json::to_vec(&body).expect("body").len()),
            protocol: Some(Protocol::OpenAi),
            provider_id: Some("openai-main".into()),
            route_id: Some("responses-route".into()),
            model: Some("gpt-5.6-sol".into()),
            global_enabled: true,
            route_enabled: true,
        };
        let (global, route) = shadow_policies();
        let build = build_shadow_plan_detailed("responses-shadow", &body, &global, &route, &input);

        assert!(build.plan.body_unchanged());
        assert_eq!(
            build.plan.graph_summary().state_machine,
            crate::optical::context_graph::ProviderToolStateMachine::OpenAiResponsesV1
        );
        assert!(build
            .candidates
            .iter()
            .all(|candidate| candidate.action != CandidateAction::ShadowEstimateOnly));
    }

    #[test]
    fn real_secret_tool_result_is_skipped() {
        let secret = format!("Bearer {}", "credential-material ".repeat(180));
        let mut messages = vec![
            json!({"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"read","input":{}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":secret}]}),
        ];
        for index in 0..6 {
            messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":format!("recent-{index}"),"name":"read","input":{}}]}));
            messages.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":format!("recent-{index}"),"content":"recent"}]}));
        }
        messages.push(json!({"role":"user","content":"next task"}));
        let body = json!({"messages": messages});
        let (global, route) = shadow_policies();
        let build =
            build_shadow_plan_detailed("plan-secret", &body, &global, &route, &make_input(&body));
        assert!(build.candidates.iter().any(|candidate| {
            candidate.action == CandidateAction::SkipSecretRisk
                && candidate.reason == "secret_no_duplicate"
        }));
        assert_eq!(build.summary.economics.text_tokens_before, 0);
        assert_eq!(build.summary.economics.image_tokens_after, 0);
        assert_eq!(build.summary.economics.net_saved_tokens, 0);
        assert_eq!(build.summary.counters.estimated_images, 0);
        let encoded = serde_json::to_string(&build.summary).unwrap();
        assert!(!encoded.contains("credential-material"));
    }

    #[test]
    fn dense_log_populates_economics_summary() {
        let log = "INFO operation completed successfully\n".repeat(1_000);
        let mut messages = vec![
            json!({"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"read","input":{}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":log}]}),
        ];
        for index in 0..6 {
            messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":format!("recent-{index}"),"name":"read","input":{}}]}));
            messages.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":format!("recent-{index}"),"content":"recent"}]}));
        }
        messages.push(json!({"role":"user","content":"next task"}));
        let body = json!({"messages": messages});
        let (global, route) = shadow_policies();
        let build = build_shadow_plan_detailed(
            "plan-economics",
            &body,
            &global,
            &route,
            &make_input(&body),
        );
        assert!(build.summary.economics.text_tokens_before > 0);
        assert!(build.summary.economics.image_tokens_after > 0);
        assert!(build.summary.economics.net_saved_tokens > 0);
        assert!(build
            .candidates
            .iter()
            .any(|candidate| candidate.action == CandidateAction::ShadowEstimateOnly));
    }

    #[test]
    fn ineligible_request_returns_ineligible_status() {
        let body = json!({});
        let (global, route) = shadow_policies();
        let mut input = make_input(&body);
        input.body_bytes = Some(0);
        let summary = build_shadow_plan("plan-2", &body, &global, &route, &input);
        assert_eq!(summary.final_status, OpticalPlanStatus::Ineligible);
    }
}
