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
