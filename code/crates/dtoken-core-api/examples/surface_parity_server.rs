use axum::serve;
use dtoken_config::DtokenConfig;
use dtoken_core_api::{build_router, AppState};
use dtoken_types::{
    ChainMode, CompressionPolicy, Protocol, ProviderKind, ProviderProfile, RouteProfile,
};
use std::io::Write;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind surface parity server");
    let addr = listener.local_addr().expect("surface parity local addr");
    println!("listening http://{addr}");
    std::io::stdout()
        .flush()
        .expect("flush surface parity listener address");

    serve(listener, build_router(AppState::new(surface_test_config())))
        .await
        .expect("serve surface parity router");
}

fn surface_test_config() -> DtokenConfig {
    DtokenConfig {
        providers: vec![ProviderProfile {
            id: "surface-provider".to_string(),
            name: "Surface provider".to_string(),
            kind: ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url: "http://127.0.0.1:9/v1".to_string(),
            api_key_ref: None,
            models: vec!["surface-model".to_string()],
            timeout_ms: 100,
            healthcheck_path: None,
            wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
        }],
        routes: vec![RouteProfile {
            id: "surface-route".to_string(),
            name: "Surface route".to_string(),
            input_protocol: Protocol::OpenAi,
            instance_id: "surface-instance".to_string(),
            target_provider_id: "surface-provider".to_string(),
            fallback_provider_id: None,
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Balanced,
            bind_host: "127.0.0.1".to_string(),
            local_port: 18081,
        }],
        instances: Vec::new(),
        provider_cache_policies: Vec::new(),
        output_shaping_policies: Vec::new(),
        optical: Default::default(),
    }
}
