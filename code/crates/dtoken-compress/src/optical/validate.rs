// PlanValidator：校验 shadow/active plan。任何失败 fail closed 到原始 body。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §22。

use dtoken_types::{CapabilityStatus, OpticalMode, OpticalPlanStatus};

#[derive(Debug, Clone)]
pub struct PlanValidationResult {
    pub valid: bool,
    pub failures: Vec<String>,
    pub mode: OpticalMode,
}

/// Shadow validator：body 不变、public summary 无 raw text、所有 candidate 有 action 和 reason、
/// current user turn AlwaysText、recent tail AlwaysText、unknown model active disabled、geometry exists。
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
        failures.push("candidate_missing_action".to_string());
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

/// Active validator：在 shadow 之上，拒绝 unknown model、cache-unstable、secret sidecar duplication、
/// patch path outside candidate。
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

/// 由 validation 结果决定最终状态。
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
}
