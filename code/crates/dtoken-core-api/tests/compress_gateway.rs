use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use dtoken_config::DtokenConfig;
use dtoken_core_api::{build_router, AppState, CompressionDecisionRequest};
use dtoken_types::{
    ChainMode, CompressionPolicy, OpticalEndpoint, Protocol, ProviderKind, ProviderProfile,
    RouteProfile,
};
use serde_json::{json, Value};
use tower::util::ServiceExt;

#[tokio::test]
async fn gateway_compress_is_route_bound_and_returns_only_the_candidate_body() {
    let state = AppState::new(config());
    let request = request();
    let library = state
        .compression_execution(request.clone())
        .expect("gateway compression should resolve");
    assert_eq!(library.decision.provider_id, "relay-provider");
    assert!(!library.decision.evidence.request_body_persisted);
    assert!(
        library.body["messages"][1]["content"]
            .as_str()
            .expect("content")
            .contains("[repeated x"),
        "{library:#?}"
    );

    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/compress")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&request).expect("request JSON"),
                ))
                .expect("gateway request"),
        )
        .await
        .expect("gateway response");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["decision"]["provider_id"], "relay-provider");
    assert_eq!(payload["body"], library.body);
    assert_eq!(
        payload["decision"]["artifact"],
        serde_json::to_value(library.decision.artifact).unwrap()
    );
    assert_eq!(payload["evidence"]["request_body_persisted"], false);
    assert_eq!(payload["evidence"]["credentials_returned"], false);
}

#[tokio::test]
async fn gateway_compress_rejects_unknown_route_without_forwarding() {
    let state = AppState::new(config());
    let mut request = request();
    request.route_id = "missing-route".to_string();
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/compress")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&request).expect("request JSON"),
                ))
                .expect("gateway request"),
        )
        .await
        .expect("gateway response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(payload["error"]["code"], "route_context_unavailable");
}

fn request() -> CompressionDecisionRequest {
    let repeated_log = ["same"; 12].join("\n");
    let mut messages = vec![
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "old-call",
                "type": "function",
                "function": {"name": "build", "arguments": "{}"}
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "old-call",
            "content": repeated_log
        }),
    ];
    for index in 0..6 {
        let call_id = format!("recent-{index}");
        messages.push(json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": call_id,
                "type": "function",
                "function": {"name": "recent", "arguments": "{}"}
            }]
        }));
        messages.push(json!({
            "role": "tool",
            "tool_call_id": call_id,
            "content": "recent result"
        }));
    }
    messages.push(json!({"role": "user", "content": "current request"}));
    CompressionDecisionRequest {
        route_id: "relay-route".to_string(),
        input_protocol: Protocol::OpenAi,
        endpoint: OpticalEndpoint::OpenAiChatCompletions,
        frozen_message_count: None,
        requested_policy: Some(CompressionPolicy::Balanced),
        body: json!({
            "model": "relay-model",
            "messages": messages
        }),
    }
}

fn config() -> DtokenConfig {
    DtokenConfig {
        providers: vec![ProviderProfile {
            id: "relay-provider".to_string(),
            name: "Third-party relay".to_string(),
            kind: ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url: "https://relay.example.invalid/v1".to_string(),
            api_key_ref: None,
            models: vec!["relay-model".to_string()],
            timeout_ms: 500,
            healthcheck_path: None,
        }],
        routes: vec![RouteProfile {
            id: "relay-route".to_string(),
            name: "Third-party relay route".to_string(),
            input_protocol: Protocol::OpenAi,
            instance_id: "relay-instance".to_string(),
            target_provider_id: "relay-provider".to_string(),
            fallback_provider_id: None,
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Balanced,
            bind_host: "127.0.0.1".to_string(),
            local_port: 18101,
        }],
        instances: Vec::new(),
        provider_cache_policies: Vec::new(),
        output_shaping_policies: Vec::new(),
        optical: Default::default(),
    }
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    serde_json::from_slice(&bytes).expect("response JSON")
}
