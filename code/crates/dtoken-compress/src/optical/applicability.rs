use dtoken_types::{
    CapabilityStatus, OpticalApplicabilityDecision, OpticalApplicabilityInput,
    OpticalApplicabilityReason, OpticalEndpoint,
};

use super::capability::{normalize_model_base, resolve_model_capability};

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

    let capability = resolve_model_capability(
        input.protocol.as_ref(),
        input.provider_id.as_deref(),
        input.model.as_deref(),
        endpoint.clone(),
    );

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

    #[test]
    fn post_anthropic_messages_can_be_active_when_supported() {
        let decision = evaluate_applicability(&base_input());

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
}
