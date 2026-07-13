use dtoken_types::{
    CapabilityStatus, OpticalApplicabilityDecision, OpticalApplicabilityInput,
    OpticalApplicabilityReason, OpticalEndpoint, OpticalGlobalPolicy, OpticalMode, Protocol,
    RouteOpticalPolicy,
};

use super::capability::{
    normalize_model_base, resolve_model_capability_record_for_route_with_registry,
    OpticalCapabilityRegistry,
};

pub fn resolve_endpoint(path: Option<&str>) -> OpticalEndpoint {
    match path.map(str::trim) {
        Some("/v1/messages") | Some("/anthropic/v1/messages") | Some("/anthropic/messages") => {
            OpticalEndpoint::AnthropicMessages
        }
        Some("/v1/chat/completions") => OpticalEndpoint::OpenAiChatCompletions,
        Some("/v1/responses") => OpticalEndpoint::OpenAiResponses,
        _ => OpticalEndpoint::Unsupported,
    }
}

pub fn evaluate_applicability(input: &OpticalApplicabilityInput) -> OpticalApplicabilityDecision {
    evaluate_applicability_with_registry(input, &OpticalCapabilityRegistry::default())
}

pub fn evaluate_applicability_with_registry(
    input: &OpticalApplicabilityInput,
    registry: &OpticalCapabilityRegistry,
) -> OpticalApplicabilityDecision {
    let endpoint = resolve_endpoint(input.endpoint_path.as_deref());
    let normalized_model_base = input.model.as_deref().map(normalize_model_base);

    if input.body_bytes.unwrap_or_default() == 0 {
        return decision(
            false,
            false,
            OpticalApplicabilityReason::EmptyBody,
            normalized_model_base,
            endpoint,
        );
    }

    if !matches!(input.method.as_deref(), Some(method) if method.eq_ignore_ascii_case("POST")) {
        return decision(
            false,
            false,
            OpticalApplicabilityReason::UnsupportedMethod,
            normalized_model_base,
            endpoint,
        );
    }

    if endpoint == OpticalEndpoint::Unsupported {
        return decision(
            false,
            false,
            OpticalApplicabilityReason::UnsupportedPath,
            normalized_model_base,
            endpoint,
        );
    }

    if !protocol_matches_endpoint(input.protocol.as_ref(), &endpoint) {
        return decision(
            false,
            false,
            OpticalApplicabilityReason::ProtocolEndpointMismatch,
            normalized_model_base,
            endpoint,
        );
    }

    if !input.global_enabled {
        return decision(
            true,
            false,
            OpticalApplicabilityReason::GlobalDisabled,
            normalized_model_base,
            endpoint,
        );
    }

    if !input.route_enabled {
        return decision(
            true,
            false,
            OpticalApplicabilityReason::RouteDisabled,
            normalized_model_base,
            endpoint,
        );
    }

    let capability = resolve_model_capability_record_for_route_with_registry(
        input.protocol.as_ref(),
        input.provider_id.as_deref(),
        input.route_id.as_deref(),
        input.model.as_deref(),
        endpoint.clone(),
        registry,
    )
    .capability;

    match capability.status {
        CapabilityStatus::Supported => decision(
            true,
            true,
            OpticalApplicabilityReason::Eligible,
            normalized_model_base,
            endpoint,
        ),
        CapabilityStatus::Limited | CapabilityStatus::ShadowOnly => decision(
            true,
            false,
            OpticalApplicabilityReason::UnknownModelShadowOnly,
            normalized_model_base,
            endpoint,
        ),
        CapabilityStatus::Blocked => decision(
            false,
            false,
            OpticalApplicabilityReason::UnsupportedProtocol,
            normalized_model_base,
            endpoint,
        ),
        CapabilityStatus::Unknown => decision(
            true,
            false,
            OpticalApplicabilityReason::UnknownModelShadowOnly,
            normalized_model_base,
            endpoint,
        ),
    }
}

pub fn evaluate_applicability_with_policies(
    input: &OpticalApplicabilityInput,
    global_policy: &OpticalGlobalPolicy,
    route_policy: &RouteOpticalPolicy,
) -> OpticalApplicabilityDecision {
    evaluate_applicability_with_policies_and_registry(
        input,
        global_policy,
        route_policy,
        &OpticalCapabilityRegistry::default(),
    )
}

pub fn evaluate_applicability_with_policies_and_registry(
    input: &OpticalApplicabilityInput,
    global_policy: &OpticalGlobalPolicy,
    route_policy: &RouteOpticalPolicy,
    registry: &OpticalCapabilityRegistry,
) -> OpticalApplicabilityDecision {
    let mut result = evaluate_applicability_with_registry(input, registry);
    if !result.eligible_for_shadow {
        return result;
    }
    if matches!(
        result.reason,
        OpticalApplicabilityReason::GlobalDisabled | OpticalApplicabilityReason::RouteDisabled
    ) {
        return result;
    }
    if !global_policy.enabled {
        result.eligible_for_active = false;
        result.reason = OpticalApplicabilityReason::GlobalDisabled;
        return result;
    }
    if !route_policy.enabled {
        result.eligible_for_active = false;
        result.reason = OpticalApplicabilityReason::RouteDisabled;
        return result;
    }

    // body_bytes is a conservative upper bound until the inventory provides exact candidate chars.
    if input.body_bytes.unwrap_or_default() > global_policy.max_candidate_chars_per_request {
        result.eligible_for_active = false;
        result.reason = OpticalApplicabilityReason::OverBudget;
        return result;
    }

    if !global_policy.allow_active {
        result.eligible_for_active = false;
        result.reason = OpticalApplicabilityReason::ActiveDisabled;
        return result;
    }

    if !route_policy.confirmed_experimental_lossy {
        result.eligible_for_active = false;
        result.reason = OpticalApplicabilityReason::ExperimentalConfirmationRequired;
        return result;
    }

    if route_policy.mode == OpticalMode::Off || !route_policy.allow_tool_result {
        result.eligible_for_active = false;
        result.reason = OpticalApplicabilityReason::RouteModeDisabled;
    }

    result
}

fn protocol_matches_endpoint(protocol: Option<&Protocol>, endpoint: &OpticalEndpoint) -> bool {
    matches!(
        (protocol, endpoint),
        (
            Some(Protocol::Anthropic),
            OpticalEndpoint::AnthropicMessages
        ) | (
            Some(Protocol::OpenAi),
            OpticalEndpoint::OpenAiChatCompletions
        ) | (Some(Protocol::OpenAi), OpticalEndpoint::OpenAiResponses)
    )
}

fn decision(
    eligible_for_shadow: bool,
    eligible_for_active: bool,
    reason: OpticalApplicabilityReason,
    normalized_model_base: Option<String>,
    endpoint: OpticalEndpoint,
) -> OpticalApplicabilityDecision {
    OpticalApplicabilityDecision {
        eligible_for_shadow,
        eligible_for_active,
        reason,
        normalized_model_base,
        endpoint: Some(endpoint),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_types::Protocol;

    fn base_input() -> OpticalApplicabilityInput {
        OpticalApplicabilityInput {
            method: Some("POST".to_string()),
            endpoint_path: Some("/v1/messages".to_string()),
            body_bytes: Some(128),
            protocol: Some(Protocol::Anthropic),
            provider_id: Some("anthropic-main".to_string()),
            route_id: Some("route-main".to_string()),
            model: Some("claude-fable-5".to_string()),
            global_enabled: true,
            route_enabled: true,
        }
    }

    fn active_registry() -> OpticalCapabilityRegistry {
        let mut registry = OpticalCapabilityRegistry::new();
        registry.register_verified(
            crate::optical::capability::experimental_anthropic_active_capability("claude-fable-5"),
        );
        registry
    }

    #[test]
    fn default_registry_is_shadow_only() {
        let decision = evaluate_applicability(&base_input());

        assert!(decision.eligible_for_shadow);
        assert!(!decision.eligible_for_active);
        assert_eq!(
            decision.reason,
            OpticalApplicabilityReason::UnknownModelShadowOnly
        );
    }

    #[test]
    fn verified_registry_can_make_supported_model_active_eligible() {
        let registry = active_registry();
        let decision = evaluate_applicability_with_registry(&base_input(), &registry);

        assert!(decision.eligible_for_shadow);
        assert!(decision.eligible_for_active);
        assert_eq!(decision.reason, OpticalApplicabilityReason::Eligible);
        assert_eq!(decision.endpoint, Some(OpticalEndpoint::AnthropicMessages));
    }

    #[test]
    fn get_request_is_ineligible() {
        let mut input = base_input();
        input.method = Some("GET".to_string());
        let decision = evaluate_applicability(&input);

        assert!(!decision.eligible_for_shadow);
        assert!(!decision.eligible_for_active);
        assert_eq!(
            decision.reason,
            OpticalApplicabilityReason::UnsupportedMethod
        );
    }

    #[test]
    fn empty_body_is_ineligible() {
        let mut input = base_input();
        input.body_bytes = Some(0);
        let decision = evaluate_applicability(&input);

        assert!(!decision.eligible_for_shadow);
        assert!(!decision.eligible_for_active);
        assert_eq!(decision.reason, OpticalApplicabilityReason::EmptyBody);
    }

    #[test]
    fn unknown_model_is_shadow_only() {
        let mut input = base_input();
        input.model = Some("unknown-model".to_string());
        let decision = evaluate_applicability(&input);

        assert!(decision.eligible_for_shadow);
        assert!(!decision.eligible_for_active);
        assert_eq!(
            decision.reason,
            OpticalApplicabilityReason::UnknownModelShadowOnly
        );
    }

    #[test]
    fn unsupported_path_is_ineligible() {
        let mut input = base_input();
        input.endpoint_path = Some("/v1/messages/count_tokens".to_string());
        let decision = evaluate_applicability(&input);

        assert!(!decision.eligible_for_shadow);
        assert!(!decision.eligible_for_active);
        assert_eq!(decision.reason, OpticalApplicabilityReason::UnsupportedPath);
    }

    #[test]
    fn protocol_endpoint_mismatch_is_ineligible() {
        let mut input = base_input();
        input.protocol = Some(Protocol::OpenAi);
        let decision = evaluate_applicability(&input);

        assert!(!decision.eligible_for_shadow);
        assert!(!decision.eligible_for_active);
        assert_eq!(
            decision.reason,
            OpticalApplicabilityReason::ProtocolEndpointMismatch
        );
    }

    #[test]
    fn active_requires_explicit_experimental_confirmation() {
        let input = base_input();
        let global = OpticalGlobalPolicy {
            enabled: true,
            allow_active: true,
            ..Default::default()
        };
        let mut route = RouteOpticalPolicy {
            enabled: true,
            mode: OpticalMode::ToolResultsOnly,
            allow_tool_result: true,
            ..Default::default()
        };
        let registry = active_registry();

        let unconfirmed =
            evaluate_applicability_with_policies_and_registry(&input, &global, &route, &registry);
        assert!(unconfirmed.eligible_for_shadow);
        assert!(!unconfirmed.eligible_for_active);
        assert_eq!(
            unconfirmed.reason,
            OpticalApplicabilityReason::ExperimentalConfirmationRequired
        );

        route.confirmed_experimental_lossy = true;
        let confirmed =
            evaluate_applicability_with_policies_and_registry(&input, &global, &route, &registry);
        assert!(confirmed.eligible_for_active);
    }

    #[test]
    fn over_budget_request_is_never_active() {
        let mut input = base_input();
        input.body_bytes = Some(600_000);
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
            ..Default::default()
        };

        let decision = evaluate_applicability_with_policies(&input, &global, &route);
        assert!(decision.eligible_for_shadow);
        assert!(!decision.eligible_for_active);
        assert_eq!(decision.reason, OpticalApplicabilityReason::OverBudget);
    }
}
