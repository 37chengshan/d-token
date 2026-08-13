use dtoken_config::DtokenConfig;
use dtoken_router::Router;
use dtoken_types::{
    ChainMode, CompressionPolicy, InstanceProfile, ProcessStatus, Protocol, ProviderKind,
    ProviderProfile, RouteProfile,
};

#[test]
fn decides_route_by_instance_and_protocol() {
    let config = DtokenConfig {
        providers: vec![ProviderProfile {
            id: "local-demo".to_string(),
            name: "Local Demo".to_string(),
            kind: ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url: "http://127.0.0.1:19001".to_string(),
            api_key_ref: None,
            models: vec!["demo-model".to_string()],
            timeout_ms: 120_000,
            healthcheck_path: Some("/health".to_string()),
            wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
        }],
        routes: vec![RouteProfile {
            id: "demo-route".to_string(),
            name: "Demo Route".to_string(),
            input_protocol: Protocol::OpenAi,
            instance_id: "demo-instance".to_string(),
            target_provider_id: "local-demo".to_string(),
            fallback_provider_id: None,
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Balanced,
            bind_host: "127.0.0.1".to_string(),
            local_port: 18001,
        }],
        instances: vec![InstanceProfile {
            id: "demo-instance".to_string(),
            name: "Demo Instance".to_string(),
            agent: "manual".to_string(),
            route_id: "demo-route".to_string(),
            local_port: 18001,
            process_status: ProcessStatus::Stopped,
            log_path: "./logs/demo-instance.jsonl".to_string(),
        }],
        provider_cache_policies: vec![],
        output_shaping_policies: vec![],
        optical: Default::default(),
    };

    let decision = Router::new(config)
        .decide("demo-instance", Protocol::OpenAi)
        .expect("route should match");

    assert_eq!(decision.route_id, "demo-route");
    assert_eq!(decision.provider_id, "local-demo");
    assert_eq!(decision.chain_mode, ChainMode::Direct);
    assert_eq!(decision.compression_policy, CompressionPolicy::Balanced);
}

#[test]
fn rejects_ambiguous_routes_instead_of_selecting_first() {
    let provider = ProviderProfile {
        id: "local-demo".to_string(),
        name: "Local Demo".to_string(),
        kind: ProviderKind::Custom,
        protocol: Protocol::OpenAi,
        base_url: "http://127.0.0.1:19001".to_string(),
        api_key_ref: None,
        models: vec!["demo-model".to_string()],
        timeout_ms: 120_000,
        healthcheck_path: None,
        wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
    };
    let route = |id: &str| RouteProfile {
        id: id.to_string(),
        name: id.to_string(),
        input_protocol: Protocol::OpenAi,
        instance_id: "demo-instance".to_string(),
        target_provider_id: "local-demo".to_string(),
        fallback_provider_id: None,
        chain_mode: ChainMode::Direct,
        compression_policy: CompressionPolicy::Balanced,
        bind_host: "127.0.0.1".to_string(),
        local_port: 18001,
    };
    let config = DtokenConfig {
        providers: vec![provider],
        routes: vec![route("route-a"), route("route-b")],
        instances: vec![],
        provider_cache_policies: vec![],
        output_shaping_policies: vec![],
        optical: Default::default(),
    };

    let error = Router::new(config)
        .decide("demo-instance", Protocol::OpenAi)
        .expect_err("ambiguous route must fail closed");
    assert!(error.to_string().contains("ambiguous routes"));
}

#[test]
fn decide_with_model_carries_identity_and_hint_without_changing_selection() {
    let config = DtokenConfig {
        providers: vec![ProviderProfile {
            id: "local-demo".to_string(),
            name: "Local Demo".to_string(),
            kind: ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url: "http://127.0.0.1:19001".to_string(),
            api_key_ref: None,
            models: vec!["demo-model".to_string()],
            timeout_ms: 120_000,
            healthcheck_path: None,
            wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
        }],
        routes: vec![RouteProfile {
            id: "demo-route".to_string(),
            name: "Demo Route".to_string(),
            input_protocol: Protocol::OpenAi,
            instance_id: "demo-instance".to_string(),
            target_provider_id: "local-demo".to_string(),
            fallback_provider_id: None,
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Balanced,
            bind_host: "127.0.0.1".to_string(),
            local_port: 18001,
        }],
        instances: vec![],
        provider_cache_policies: vec![],
        output_shaping_policies: vec![],
        optical: Default::default(),
    };
    let router = Router::new(config);

    let plain = router.decide("demo-instance", Protocol::OpenAi).expect("route");
    assert!(plain.model_identity.is_none());
    assert!(plain.compression_hint.is_none());

    let with_model = router
        .decide_with_model(
            "demo-instance",
            Protocol::OpenAi,
            Some("opencode/gemini-3-pro"),
            Some("default"),
        )
        .expect("route");
    // Selection is unchanged; the model identity and hint are carried.
    assert_eq!(with_model.route_id, plain.route_id);
    assert_eq!(with_model.provider_id, plain.provider_id);
    assert_eq!(with_model.compression_policy, plain.compression_policy);
    assert_eq!(with_model.model_identity.as_deref(), Some("opencode/gemini-3-pro"));
    assert_eq!(with_model.compression_hint.as_deref(), Some("default"));
}

#[test]
fn decide_with_model_still_fails_closed_on_unknown_instance() {
    let config = DtokenConfig {
        providers: vec![],
        routes: vec![],
        instances: vec![],
        provider_cache_policies: vec![],
        output_shaping_policies: vec![],
        optical: Default::default(),
    };
    let error = Router::new(config)
        .decide_with_model("missing", Protocol::OpenAi, Some("some-model"), None)
        .expect_err("unknown instance must fail closed");
    assert!(error.to_string().contains("no matching route"));
}
