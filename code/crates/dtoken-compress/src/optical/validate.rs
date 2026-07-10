// PlanValidator：校验 shadow/active plan。任何失败 fail closed 到原始 body。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §22。

use dtoken_types::{
    CapabilityStatus, OpticalGlobalPolicy, OpticalMode, OpticalModelCapability,
    OpticalPlanPublicSummary, OpticalPlanStatus, RouteOpticalPolicy,
};

#[derive(Debug, Clone)]
pub struct PlanValidationResult {
    pub valid: bool,
    pub failures: Vec<String>,
    pub mode: OpticalMode,
}

#[derive(Debug, Clone)]
pub struct CandidateValidationEvidence<'a> {
    pub action: &'a str,
    pub reason: &'a str,
}

pub struct ShadowPlanValidationInput<'a> {
    pub original_body: &'a serde_json::Value,
    pub planned_body: &'a serde_json::Value,
    pub public_summary: &'a OpticalPlanPublicSummary,
    pub candidates: &'a [CandidateValidationEvidence<'a>],
    pub current_user_always_text: bool,
    pub recent_tail_always_text: bool,
    pub geometry_exists: bool,
}

pub struct ActivePlanValidationInput<'a> {
    pub shadow_validation: &'a PlanValidationResult,
    pub global_policy: &'a OpticalGlobalPolicy,
    pub route_policy: &'a RouteOpticalPolicy,
    pub capability: &'a OpticalModelCapability,
    pub cache_stable: bool,
    pub secret_sidecar_duplication: bool,
    pub patch_paths_inside_candidates: bool,
    pub image_count: usize,
    pub net_saved_tokens: isize,
}

pub fn validate_shadow_plan_input(input: &ShadowPlanValidationInput<'_>) -> PlanValidationResult {
    let summary_json = serde_json::to_string(input.public_summary).unwrap_or_default();
    let forbidden_markers = [
        "raw_prompt",
        "request_body",
        "response_body",
        "rendered_text",
        "png_bytes",
        "base64",
        "secret_value",
        "exact_value",
    ];
    let summary_has_raw_text = forbidden_markers
        .iter()
        .any(|marker| summary_json.contains(marker));
    let all_candidates_have_action = input
        .candidates
        .iter()
        .all(|candidate| !candidate.action.is_empty() && !candidate.reason.is_empty());

    validate_shadow_plan(
        input.original_body == input.planned_body,
        summary_has_raw_text,
        all_candidates_have_action,
        input.current_user_always_text,
        input.recent_tail_always_text,
        input.geometry_exists,
    )
}

/// Compatibility helper for callers that already computed validation evidence.
pub fn validate_shadow_plan(
    body_unchanged: bool,
    summary_has_raw_text: bool,
    all_candidates_have_action: bool,
    current_user_always_text: bool,
    recent_tail_always_text: bool,
    geometry_exists: bool,
) -> PlanValidationResult {
    let mut failures = Vec::new();
    if !body_unchanged {
        failures.push("body_changed".to_string());
    }
    if summary_has_raw_text {
        failures.push("summary_contains_raw_text".to_string());
    }
    if !all_candidates_have_action {
        failures.push("candidate_missing_action_or_reason".to_string());
    }
    if !current_user_always_text {
        failures.push("current_user_not_always_text".to_string());
    }
    if !recent_tail_always_text {
        failures.push("recent_tail_not_always_text".to_string());
    }
    if !geometry_exists {
        failures.push("geometry_missing".to_string());
    }
    PlanValidationResult {
        valid: failures.is_empty(),
        failures,
        mode: OpticalMode::ShadowPreview,
    }
}

pub fn validate_active_plan_input(input: &ActivePlanValidationInput<'_>) -> PlanValidationResult {
    let mut failures = input.shadow_validation.failures.clone();
    if !input.shadow_validation.valid {
        failures.push("shadow_plan_invalid".to_string());
    }
    if !input.global_policy.enabled || !input.global_policy.allow_active {
        failures.push("active_globally_disabled".to_string());
    }
    if !input.route_policy.enabled
        || input.route_policy.mode == OpticalMode::Off
        || !input.route_policy.allow_tool_result
    {
        failures.push("active_route_disabled".to_string());
    }
    if !input.route_policy.confirmed_experimental_lossy {
        failures.push("active_confirmation_missing".to_string());
    }
    if input.capability.status != CapabilityStatus::Supported {
        failures.push("active_rejects_unsupported_capability".to_string());
    }
    if !input.cache_stable {
        failures.push("active_rejects_cache_unstable".to_string());
    }
    if input.secret_sidecar_duplication {
        failures.push("active_rejects_secret_sidecar_duplication".to_string());
    }
    if !input.patch_paths_inside_candidates {
        failures.push("patch_path_outside_candidate".to_string());
    }
    let capability_limit = if input.capability.max_images_per_request == 0 {
        usize::MAX
    } else {
        input.capability.max_images_per_request
    };
    let image_limit = input
        .global_policy
        .max_images_per_request
        .min(capability_limit);
    if input.image_count > image_limit {
        failures.push("active_image_budget_exceeded".to_string());
    }
    if input.net_saved_tokens < input.route_policy.min_net_saved_tokens {
        failures.push("active_net_savings_below_threshold".to_string());
    }

    PlanValidationResult {
        valid: failures.is_empty(),
        failures,
        mode: OpticalMode::ToolResultsOnly,
    }
}

/// Compatibility helper retained for existing tests and callers.
pub fn validate_active_plan(
    shadow_valid: &PlanValidationResult,
    capability_status: CapabilityStatus,
    cache_stable: bool,
    secret_sidecar_duplication: bool,
    patch_path_inside_candidate: bool,
) -> PlanValidationResult {
    let mut failures = shadow_valid.failures.clone();
    if !shadow_valid.valid {
        failures.push("shadow_plan_invalid".to_string());
    }
    if matches!(
        capability_status,
        CapabilityStatus::Unknown | CapabilityStatus::Blocked | CapabilityStatus::ShadowOnly
    ) {
        failures.push("active_rejects_unknown_model".to_string());
    }
    if !cache_stable {
        failures.push("active_rejects_cache_unstable".to_string());
    }
    if secret_sidecar_duplication {
        failures.push("active_rejects_secret_sidecar_duplication".to_string());
    }
    if !patch_path_inside_candidate {
        failures.push("patch_path_outside_candidate".to_string());
    }
    PlanValidationResult {
        valid: failures.is_empty(),
        failures,
        mode: OpticalMode::ToolResultsOnly,
    }
}

pub fn decide_final_status(valid: bool, is_active: bool) -> OpticalPlanStatus {
    if !valid {
        return OpticalPlanStatus::FailedClosed;
    }
    if is_active {
        OpticalPlanStatus::ValidatedActivePlan
    } else {
        OpticalPlanStatus::ValidatedShadowPlan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_types::{
        CachePublicSummary, OpticalEconomicsPublicSummary, OpticalGateDecision, OpticalPlanCounters,
    };
    use serde_json::json;

    fn summary() -> OpticalPlanPublicSummary {
        OpticalPlanPublicSummary {
            plan_id: "plan-1".to_string(),
            mode: OpticalMode::ShadowPreview,
            final_status: OpticalPlanStatus::ValidatedShadowPlan,
            gates: vec![OpticalGateDecision {
                name: "test".to_string(),
                passed: true,
                reason: "ok".to_string(),
            }],
            counters: OpticalPlanCounters::default(),
            buckets: Vec::new(),
            cache: CachePublicSummary::default(),
            economics: OpticalEconomicsPublicSummary::default(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn structured_shadow_validation_checks_actual_body() {
        let original = json!({"messages": []});
        let changed = json!({"messages": [{"role": "user"}]});
        let candidates = [CandidateValidationEvidence {
            action: "preserve_text",
            reason: "protected",
        }];
        let result = validate_shadow_plan_input(&ShadowPlanValidationInput {
            original_body: &original,
            planned_body: &changed,
            public_summary: &summary(),
            candidates: &candidates,
            current_user_always_text: true,
            recent_tail_always_text: true,
            geometry_exists: true,
        });
        assert!(!result.valid);
        assert!(result.failures.contains(&"body_changed".to_string()));
    }

    #[test]
    fn shadow_body_unchanged_valid() {
        let result = validate_shadow_plan(true, false, true, true, true, true);
        assert!(result.valid);
    }

    #[test]
    fn shadow_body_changed_invalid() {
        let result = validate_shadow_plan(false, false, true, true, true, true);
        assert!(!result.valid);
        assert!(result.failures.contains(&"body_changed".to_string()));
    }

    #[test]
    fn active_rejects_unknown_model() {
        let shadow = validate_shadow_plan(true, false, true, true, true, true);
        let active = validate_active_plan(&shadow, CapabilityStatus::Unknown, true, false, true);
        assert!(!active.valid);
        assert!(active
            .failures
            .contains(&"active_rejects_unknown_model".to_string()));
    }

    #[test]
    fn active_rejects_cache_unstable() {
        let shadow = validate_shadow_plan(true, false, true, true, true, true);
        let active = validate_active_plan(&shadow, CapabilityStatus::Supported, false, false, true);
        assert!(!active.valid);
        assert!(active
            .failures
            .contains(&"active_rejects_cache_unstable".to_string()));
    }

    #[test]
    fn active_rejects_secret_sidecar_duplication() {
        let shadow = validate_shadow_plan(true, false, true, true, true, true);
        let active = validate_active_plan(&shadow, CapabilityStatus::Supported, true, true, true);
        assert!(!active.valid);
        assert!(active
            .failures
            .contains(&"active_rejects_secret_sidecar_duplication".to_string()));
    }

    #[test]
    fn patch_path_outside_candidate_rejected() {
        let shadow = validate_shadow_plan(true, false, true, true, true, true);
        let active = validate_active_plan(&shadow, CapabilityStatus::Supported, true, false, false);
        assert!(!active.valid);
        assert!(active
            .failures
            .contains(&"patch_path_outside_candidate".to_string()));
    }

    #[test]
    fn structured_active_validation_requires_all_release_gates() {
        let shadow = validate_shadow_plan(true, false, true, true, true, true);
        let global = OpticalGlobalPolicy {
            enabled: true,
            allow_active: true,
            ..Default::default()
        };
        let route = RouteOpticalPolicy {
            enabled: true,
            mode: OpticalMode::ToolResultsOnly,
            allow_tool_result: true,
            confirmed_experimental_lossy: true,
            min_net_saved_tokens: 100,
            ..Default::default()
        };
        let capability = OpticalModelCapability {
            status: CapabilityStatus::Supported,
            max_images_per_request: 4,
            ..Default::default()
        };
        let result = validate_active_plan_input(&ActivePlanValidationInput {
            shadow_validation: &shadow,
            global_policy: &global,
            route_policy: &route,
            capability: &capability,
            cache_stable: true,
            secret_sidecar_duplication: false,
            patch_paths_inside_candidates: true,
            image_count: 2,
            net_saved_tokens: 200,
        });
        assert!(result.valid, "{:?}", result.failures);
    }
}
