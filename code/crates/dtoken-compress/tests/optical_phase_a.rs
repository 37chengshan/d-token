use dtoken_compress::optical::{
    anthropic_dense_v2, estimate_image_tokens, evaluate_applicability,
    evaluate_applicability_with_registry, normalize_model_base,
    resolve_model_capability_with_registry, OpticalCapabilityRegistry, TextArena, TextSensitivity,
};
use dtoken_types::{
    CapabilityStatus, OpticalApplicabilityInput, OpticalApplicabilityReason, OpticalEndpoint,
    OpticalMode, Protocol,
};

fn supported_input() -> OpticalApplicabilityInput {
    OpticalApplicabilityInput {
        method: Some("POST".to_string()),
        endpoint_path: Some("/v1/messages".to_string()),
        body_bytes: Some(256),
        protocol: Some(Protocol::Anthropic),
        provider_id: Some("anthropic-main".to_string()),
        route_id: Some("route-main".to_string()),
        model: Some("claude-fable-5".to_string()),
        global_enabled: true,
        route_enabled: true,
    }
}

#[test]
fn capability_normalizes_model_and_unsigned_registry_cannot_grant_active() {
    assert_eq!(
        normalize_model_base(" Claude-Fable-5 [beta] "),
        "claude-fable-5"
    );

    let registry = OpticalCapabilityRegistry::new();
    let capability = resolve_model_capability_with_registry(
        Some(&Protocol::Anthropic),
        Some("anthropic-main"),
        Some("Claude-Fable-5"),
        OpticalEndpoint::AnthropicMessages,
        &registry,
    );

    assert_eq!(capability.status, CapabilityStatus::Limited);
    assert_eq!(capability.max_mode, OpticalMode::ShadowPreview);
    assert!(capability.supports_anthropic_image_block);
}

#[test]
fn applicability_blocks_unsafe_or_unsupported_live_paths() {
    let registry = OpticalCapabilityRegistry::new();
    let supported = evaluate_applicability_with_registry(&supported_input(), &registry);
    assert!(supported.eligible_for_shadow);
    assert!(!supported.eligible_for_active);

    let mut get_request = supported_input();
    get_request.method = Some("GET".to_string());
    let blocked = evaluate_applicability(&get_request);
    assert!(!blocked.eligible_for_shadow);
    assert!(!blocked.eligible_for_active);
    assert_eq!(
        blocked.reason,
        OpticalApplicabilityReason::UnsupportedMethod
    );

    let mut unknown_model = supported_input();
    unknown_model.model = Some("unknown-model".to_string());
    let shadow_only = evaluate_applicability(&unknown_model);
    assert!(shadow_only.eligible_for_shadow);
    assert!(!shadow_only.eligible_for_active);
    assert_eq!(
        shadow_only.reason,
        OpticalApplicabilityReason::UnknownModelShadowOnly
    );
}

#[test]
fn render_geometry_is_profile_driven() {
    let profile = anthropic_dense_v2();
    assert_eq!(profile.profile_id, "anthropic_dense_v2");
    assert!(profile.shrink_last_page);
    assert_eq!(
        profile.readable_chars_per_image,
        profile.cols * profile.readable_rows
    );
    assert!(estimate_image_tokens(&profile, 1) > 0);

    assert_eq!(profile.width_px, 1_568);
}

#[test]
fn text_arena_keeps_raw_text_request_local_and_debug_redacted() {
    let mut arena = TextArena::new("request-1");
    let text_ref = arena.insert(
        "sensitive raw prompt",
        TextSensitivity::RenderCandidateInternal,
    );
    let debug = format!("{:?}", text_ref);

    assert_eq!(arena.get(&text_ref), Some("sensitive raw prompt"));
    assert!(!debug.contains("sensitive raw prompt"));
    assert!(debug.contains("digest_prefix"));
}
