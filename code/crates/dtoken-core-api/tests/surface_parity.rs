use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
    Router,
};
use dtoken_config::DtokenConfig;
use dtoken_context_store::{
    MemoryAccessScope, MemoryCapabilityPermission, MemoryCapabilityRouteBinding,
    ResolvedMemoryCapability,
};
use dtoken_core_api::{
    build_router, mcp::McpServer, openai_chat_to_responses_response, responses_to_openai_chat_body,
    AppState, CompressionDecisionError, CompressionDecisionRequest,
};
use dtoken_types::{
    AvailabilityState, ChainMode, CompressionPolicy, Protocol, ProviderKind, ProviderProfile,
    RouteProfile,
};
use serde_json::{json, Value};
use tower::util::ServiceExt;

/// Opens a Streamable-HTTP session over POST /mcp and returns the
/// `Mcp-Session-Id` header that later requests must carry.
async fn initialize_http_session(app: &Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "initialize",
                        "params": {"protocolVersion": "2024-11-05"}
                    }))
                    .expect("initialize JSON"),
                ))
                .expect("initialize request"),
        )
        .await
        .expect("initialize response");
    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get(dtoken_core_api::mcp::MCP_SESSION_HEADER)
        .expect("Mcp-Session-Id header")
        .to_str()
        .expect("session id is utf-8")
        .to_string()
}

#[tokio::test]
async fn compression_decision_matches_http_and_mcp_surfaces() {
    let fixture = fixture_request();
    let state = AppState::new(surface_test_config());
    let library_decision = state
        .compression_decision(fixture.clone())
        .expect("library surface should resolve");
    let library_json = serde_json::to_value(&library_decision).expect("decision JSON");
    let library_execution = state
        .compression_execution(fixture.clone())
        .expect("library gateway surface should resolve");
    let library_execution_json = serde_json::to_value(&library_execution).expect("execution JSON");

    let http_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/compression/decision")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(fixture_bytes()))
                .expect("HTTP request"),
        )
        .await
        .expect("HTTP response");
    assert_eq!(http_response.status(), StatusCode::OK);
    let http_json = response_json(http_response).await;
    assert_eq!(
        strip_timing(http_json.clone()),
        strip_timing(library_json.clone())
    );

    let gateway_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/compress")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(fixture_bytes()))
                .expect("gateway request"),
        )
        .await
        .expect("gateway response");
    assert_eq!(gateway_response.status(), StatusCode::OK);
    let gateway_json = response_json(gateway_response).await;
    assert_eq!(
        strip_timing(gateway_json),
        strip_timing(library_execution_json.clone())
    );

    let mcp_response = McpServer::new(state.clone())
        .handle_jsonrpc(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "headroom_compress",
                "arguments": {
                    "policy": "balanced",
                    "route_ref": {
                        "route_id": fixture.route_id,
                        "input_protocol": fixture.input_protocol,
                        "endpoint": fixture.endpoint
                    },
                    "body": fixture.body
                }
            }
        }))
        .expect("MCP response");
    let mcp_json = mcp_tool_payload(mcp_response);
    assert_eq!(
        strip_timing(mcp_json.clone()),
        strip_timing(library_execution_json.clone())
    );
    assert_eq!(mcp_json["decision"], library_json);
    assert_eq!(
        mcp_json["decision"]["reason_code"],
        http_json["reason_code"]
    );
    assert_eq!(mcp_json["decision"]["artifact"], http_json["artifact"]);
    assert_eq!(
        mcp_json["fallback_reason_code"],
        library_json["reason_code"]
    );
}

#[tokio::test]
async fn http_json_rpc_mcp_matches_stdio_and_library_surfaces() {
    let fixture = fixture_request();
    let state = AppState::new(surface_test_config());
    let library_execution = state
        .compression_execution(fixture.clone())
        .expect("library gateway surface should resolve");
    let library_json = serde_json::to_value(&library_execution).expect("execution JSON");

    let mcp_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "headroom_compress",
            "arguments": {
                "policy": "balanced",
                "route_ref": {
                    "route_id": fixture.route_id,
                    "input_protocol": fixture.input_protocol,
                    "endpoint": fixture.endpoint
                },
                "body": fixture.body
            }
        }
    });

    // Stdio transport.
    let stdio_response = McpServer::new(state.clone())
        .handle_jsonrpc(mcp_body.clone())
        .expect("stdio MCP response");
    let stdio_payload = mcp_tool_payload(stdio_response.clone());
    assert_eq!(
        strip_timing(stdio_payload.clone()),
        strip_timing(library_json)
    );

    // Streamable-HTTP JSON mode answers with the identical payload. The
    // session is opened by `initialize`; the tools/call request must carry
    // the returned `Mcp-Session-Id` header.
    let app = build_router(state.clone());
    let session_id = initialize_http_session(&app).await;
    let http_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(CONTENT_TYPE, "application/json")
                .header(
                    dtoken_core_api::mcp::MCP_SESSION_HEADER,
                    session_id.as_str(),
                )
                .header("accept", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&mcp_body).expect("MCP body JSON"),
                ))
                .expect("HTTP MCP request"),
        )
        .await
        .expect("HTTP MCP response");
    assert_eq!(http_response.status(), StatusCode::OK);
    let http_json = response_json(http_response).await;
    assert_eq!(http_json["jsonrpc"], "2.0");
    assert_eq!(http_json["id"], 1);
    let http_payload: Value = serde_json::from_str(
        http_json["result"]["content"][0]["text"]
            .as_str()
            .expect("text"),
    )
    .expect("HTTP payload JSON");
    assert_eq!(strip_timing(http_payload), strip_timing(stdio_payload));
}

#[tokio::test]
async fn http_json_rpc_mcp_rejects_oversized_frames_and_parse_errors_without_echoing() {
    let state = AppState::new(surface_test_config());
    let secret = "HTTP_MCP_SECRET_MUST_NOT_ECHO";

    // Oversized frame -> 413 with a JSON-RPC error, input never echoed.
    let oversized = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(vec![
                    b'x';
                    dtoken_core_api::mcp::MAX_MCP_MESSAGE_BYTES
                        + 1
                ]))
                .expect("oversized HTTP MCP request"),
        )
        .await
        .expect("oversized response");
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!response_json(oversized).await.to_string().contains(secret));

    // Parse error -> 400 with a JSON-RPC error, input never echoed.
    let malformed = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(format!("not-json {secret}")))
                .expect("malformed HTTP MCP request"),
        )
        .await
        .expect("malformed response");
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let malformed_json = response_json(malformed).await;
    assert_eq!(malformed_json["error"]["code"], -32700);
    assert!(!malformed_json.to_string().contains(secret));
}

#[tokio::test]
async fn http_json_rpc_mcp_preserves_cancellation_state_across_posts() {
    let app = build_router(AppState::new(surface_test_config()));
    let session_id = initialize_http_session(&app).await;
    let cancellation = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(CONTENT_TYPE, "application/json")
                .header(
                    dtoken_core_api::mcp::MCP_SESSION_HEADER,
                    session_id.as_str(),
                )
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/cancelled",
                        "params": {"requestId": 99}
                    }))
                    .expect("cancellation JSON"),
                ))
                .expect("cancellation request"),
        )
        .await
        .expect("cancellation response");
    assert_eq!(cancellation.status(), StatusCode::ACCEPTED);

    let cancelled_request = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(CONTENT_TYPE, "application/json")
                .header(
                    dtoken_core_api::mcp::MCP_SESSION_HEADER,
                    session_id.as_str(),
                )
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "jsonrpc": "2.0",
                        "id": 99,
                        "method": "ping"
                    }))
                    .expect("cancelled request JSON"),
                ))
                .expect("cancelled request"),
        )
        .await
        .expect("cancelled response");
    assert_eq!(cancelled_request.status(), StatusCode::OK);
    let payload = response_json(cancelled_request).await;
    assert_eq!(payload["error"]["code"], -32800);
    assert_eq!(payload["error"]["message"], "Request cancelled");
}

#[tokio::test]
async fn route_context_mismatch_uses_stable_error_codes_across_surfaces() {
    let mut payload = fixture_value();
    payload["endpoint"] = json!("open_ai_responses");
    let request: CompressionDecisionRequest =
        serde_json::from_value(payload.clone()).expect("request fixture");
    let state = AppState::new(surface_test_config());

    let library_error = state
        .compression_decision(request)
        .expect_err("library surface should fail closed");
    assert!(matches!(
        library_error,
        CompressionDecisionError::RouteContextMismatch
    ));

    let http_response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/compression/decision")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&payload).expect("HTTP mismatch payload"),
                ))
                .expect("HTTP request"),
        )
        .await
        .expect("HTTP response");
    assert_eq!(http_response.status(), StatusCode::BAD_REQUEST);
    let http_json = response_json(http_response).await;
    assert_eq!(http_json["error"]["code"], "route_context_mismatch");

    let mcp_response = McpServer::new(state)
        .handle_jsonrpc(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "headroom_compress",
                "arguments": {
                    "route_ref": {
                        "route_id": "surface-route",
                        "input_protocol": "open_ai",
                        "endpoint": "open_ai_responses"
                    },
                    "body": fixture_request().body
                }
            }
        }))
        .expect("MCP mismatch response");
    let error_text = mcp_tool_text(mcp_response);
    assert!(error_text.contains("route_context_mismatch"));
}

#[tokio::test]
async fn read_surfaces_share_stable_body_free_runtime_summaries() {
    let state = AppState::new(surface_test_config());
    let fixture_marker = fixture_request().body["messages"][0]["content"]
        .as_str()
        .expect("fixture marker")
        .to_string();

    let stats = get_json(&state, "/stats").await;
    let headroom = get_json(&state, "/headroom/stats").await;
    let ccr = get_json(&state, "/ccr/summary").await;
    let memory = get_json(&state, "/memory/summary").await;
    let optical_status = get_json(&state, "/compression/optical/status").await;
    let optical_capabilities = get_json(&state, "/compression/optical/capabilities").await;
    let metrics = get_text(&state, "/metrics").await;

    let mcp = McpServer::new(state.clone());
    let mcp_stats = mcp_tool_payload(mcp_call(&mcp, 10, "headroom_stats", json!({})));
    assert_eq!(mcp_stats["stats"], stats);
    assert_eq!(headroom["stats"], stats);
    assert_eq!(mcp_stats["cache_summary"], ccr);
    assert_eq!(headroom["cache_summary"], ccr);
    assert_eq!(headroom["metrics"]["prometheus_text"], metrics);
    assert_eq!(mcp_stats["raw_available"], false);
    assert_eq!(ccr["raw_available"], false);

    assert_eq!(memory["state"], "scope_required");
    assert_eq!(
        memory["reason_code"],
        "memory_summary_authorization_unavailable"
    );
    assert_eq!(memory["raw_available"], false);
    let memory_export = mcp_tool_payload(mcp_call(&mcp, 11, "headroom_memory_export", json!({})));
    assert_eq!(memory_export["code"], "memory_authorization_unavailable");

    assert_eq!(optical_status["fail_closed"], true);
    assert_eq!(optical_capabilities["fail_closed"], true);
    assert_eq!(optical_status["public_summary_only"], true);
    assert_eq!(optical_capabilities["public_summary_only"], true);

    for (id, missing_tool) in [
        (12, "headroom_metrics"),
        (13, "headroom_optical_status"),
        (14, "headroom_optical_capabilities"),
    ] {
        let unavailable = mcp_tool_payload(mcp_call(&mcp, id, missing_tool, json!({})));
        assert_eq!(unavailable["code"], "unknown_tool");
        assert_eq!(
            unavailable["message"],
            "The requested d-token MCP tool is not available."
        );
    }

    let public_projection = json!({
        "stats": stats,
        "headroom": headroom,
        "ccr": ccr,
        "memory": memory,
        "optical_status": optical_status,
        "optical_capabilities": optical_capabilities,
        "metrics": metrics,
    });
    assert!(!public_projection.to_string().contains(&fixture_marker));
}

#[tokio::test]
async fn ccr_retrieve_and_delete_expose_stable_authorization_semantics() {
    let state = AppState::new(surface_test_config());
    let recovery_ref = "ccr:surface-parity-missing";
    let capability = "c".repeat(48);
    let wrong_capability = "x".repeat(48);

    let metadata_only = get_json(&state, &format!("/ccr/retrieve/{recovery_ref}")).await;
    assert_eq!(metadata_only["raw_available"], false);
    assert_eq!(metadata_only["error"]["type"], "ccr_metadata_not_found");

    let unavailable = McpServer::new(state.clone());
    for (id, tool) in [(20, "headroom_retrieve"), (21, "headroom_delete")] {
        let payload = mcp_tool_payload(mcp_call(
            &unavailable,
            id,
            tool,
            json!({"recovery_ref": recovery_ref, "authorization": capability}),
        ));
        assert_eq!(payload["code"], "retrieve_authorization_unavailable");
        assert!(!payload.to_string().contains(&capability));
    }

    let authorized = McpServer::new_with_retrieve_capability(state, Some(capability.clone()));
    assert!(authorized
        .available_tool_names()
        .contains(&"headroom_retrieve"));
    assert!(authorized
        .available_tool_names()
        .contains(&"headroom_delete"));

    for (id, tool) in [(22, "headroom_retrieve"), (23, "headroom_delete")] {
        let denied = mcp_tool_payload(mcp_call(
            &authorized,
            id,
            tool,
            json!({"recovery_ref": recovery_ref, "authorization": wrong_capability}),
        ));
        assert_eq!(denied["code"], "retrieve_authorization_denied");
        assert!(!denied.to_string().contains(&wrong_capability));

        let missing = mcp_tool_payload(mcp_call(
            &authorized,
            id + 10,
            tool,
            json!({"recovery_ref": recovery_ref, "authorization": capability}),
        ));
        assert_eq!(missing["code"], "recovery_unavailable");
        assert!(!missing.to_string().contains(&capability));
    }
}

#[tokio::test]
async fn memory_export_is_scope_bound_while_http_remains_summary_only() {
    let directory = std::env::temp_dir().join(format!(
        "dtoken-surface-parity-memory-{}",
        uuid::Uuid::new_v4()
    ));
    let state = AppState::new_persistent(surface_test_config(), &directory);

    let http_summary = get_json(&state, "/memory/summary").await;
    assert_eq!(http_summary["state"], "scope_required");
    assert_eq!(
        http_summary["reason_code"],
        "memory_summary_authorization_unavailable"
    );
    assert_eq!(http_summary["raw_available"], false);

    let scoped = McpServer::new_with_managed_memory_capability_for_test(
        state,
        ResolvedMemoryCapability {
            transaction_id: format!("memcap-{}", uuid::Uuid::new_v4()),
            scope: MemoryAccessScope {
                project_id: "surface-project".to_string(),
                agent_id: "surface-agent".to_string(),
                session_id: Some("surface-session".to_string()),
            },
            route_binding: MemoryCapabilityRouteBinding {
                route_id: "surface-route".to_string(),
                provider_id: "surface-provider".to_string(),
                model: "surface-model".to_string(),
                config_revision: "a".repeat(64),
            },
            permissions: vec![MemoryCapabilityPermission::Export],
            source_revision: "surface-parity".to_string(),
            expires_at_ms: dtoken_types::time::now_ms() + 60_000,
        },
    );
    assert!(scoped
        .available_tool_names()
        .contains(&"headroom_memory_export"));

    let denied = mcp_tool_payload(mcp_call(&scoped, 40, "headroom_memory_list", json!({})));
    assert_eq!(denied["code"], "memory_access_denied");

    let exported = mcp_tool_payload(mcp_call(&scoped, 41, "headroom_memory_export", json!({})));
    assert_eq!(exported["source"], "dtoken-owned-runtime");
    assert_eq!(exported["state"], "exported");
    assert_eq!(exported["count"], 0);
    assert_eq!(exported["raw_available"], false);

    drop(scoped);
    let _ = std::fs::remove_dir_all(directory);
}

#[tokio::test]
async fn capability_registry_marks_public_surfaces_ready() {
    let response = build_router(AppState::new(surface_test_config()))
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/capabilities")
                .body(Body::empty())
                .expect("capabilities request"),
        )
        .await
        .expect("capabilities response");
    assert_eq!(response.status(), StatusCode::OK);

    let registry = response_json(response).await;
    let capabilities = registry["capabilities"]
        .as_array()
        .expect("capabilities array");

    let rust_library = capabilities
        .iter()
        .find(|entry| entry["id"] == "rust-library")
        .expect("rust-library capability");
    assert_eq!(rust_library["state"], json!(AvailabilityState::Ready));
    assert_eq!(
        rust_library["reason_codes"],
        json!([
            "public_compression_decision_contract_available",
            "route_bound_gateway_compress_available"
        ])
    );

    let typescript_sdk = capabilities
        .iter()
        .find(|entry| entry["id"] == "typescript-sdk")
        .expect("typescript-sdk capability");
    assert_eq!(typescript_sdk["state"], json!(AvailabilityState::Ready));
    assert_eq!(
        typescript_sdk["reason_codes"],
        json!([
            "typescript_sdk_http_binding_available",
            "typescript_sdk_gateway_compress_available"
        ])
    );

    let compressor_registry = capabilities
        .iter()
        .find(|entry| entry["id"] == "compressor-registry")
        .expect("compressor registry capability");
    assert_eq!(
        compressor_registry["state"],
        json!(AvailabilityState::Ready)
    );
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

fn fixture_request() -> CompressionDecisionRequest {
    serde_json::from_value(fixture_value()).expect("fixture request")
}

fn fixture_value() -> Value {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = root.join("../../packages/sdk/testdata/compression-decision.request.json");
    serde_json::from_slice(
        &std::fs::read(&fixture_path).expect("surface parity fixture should exist"),
    )
    .expect("surface parity fixture JSON")
}

fn fixture_bytes() -> Vec<u8> {
    serde_json::to_vec(&fixture_value()).expect("fixture bytes")
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    serde_json::from_slice(&bytes).expect("response JSON")
}

/// `CompressionExecution.elapsed_us` is a live measurement, so it can never
/// be byte-identical across two independent library/HTTP/MCP invocations.
/// Parity covers the decision/reason/artifact contract, which is deterministic;
/// timing is stripped before comparison.
fn strip_timing(value: Value) -> Value {
    match value {
        Value::Object(mut object) => {
            object.remove("elapsed_us");
            Value::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, strip_timing(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.into_iter().map(strip_timing).collect()),
        other => other,
    }
}

async fn get_json(state: &AppState, uri: &str) -> Value {
    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .expect("GET request"),
        )
        .await
        .expect("GET response");
    assert_eq!(response.status(), StatusCode::OK, "GET {uri}");
    response_json(response).await
}

async fn get_text(state: &AppState, uri: &str) -> String {
    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .expect("GET request"),
        )
        .await
        .expect("GET response");
    assert_eq!(response.status(), StatusCode::OK, "GET {uri}");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    String::from_utf8(bytes.to_vec()).expect("UTF-8 response")
}

fn mcp_call(server: &McpServer, id: u64, name: &str, arguments: Value) -> Value {
    server
        .handle_jsonrpc(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        }))
        .expect("MCP response")
}

fn mcp_tool_payload(response: Value) -> Value {
    serde_json::from_str(&mcp_tool_text(response)).expect("MCP payload JSON")
}

fn mcp_tool_text(response: Value) -> String {
    response["result"]["content"][0]["text"]
        .as_str()
        .expect("MCP text content")
        .to_string()
}

// === OpenAI Responses API semantic bridge regression tests ===
//
// Library-surface tests exercise the pure conversion functions directly;
// HTTP-surface tests exercise /v1/responses end to end against a loopback
// mock upstream. Unsupported Responses semantics must fail closed on both
// surfaces: an explicit error, never a silent drop or guess.

fn responses_test_config(base_url: String) -> DtokenConfig {
    DtokenConfig {
        providers: vec![ProviderProfile {
            id: "responses-provider".to_string(),
            name: "Responses provider".to_string(),
            kind: ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url,
            api_key_ref: None,
            models: vec!["responses-model".to_string()],
            timeout_ms: 5_000,
            healthcheck_path: None,
        }],
        routes: vec![RouteProfile {
            id: "responses-route".to_string(),
            name: "Responses route".to_string(),
            input_protocol: Protocol::OpenAi,
            instance_id: "responses-instance".to_string(),
            target_provider_id: "responses-provider".to_string(),
            fallback_provider_id: None,
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Off,
            bind_host: "127.0.0.1".to_string(),
            local_port: 18082,
        }],
        instances: Vec::new(),
        provider_cache_policies: Vec::new(),
        output_shaping_policies: Vec::new(),
        optical: Default::default(),
    }
}

/// Accept one request on a loopback listener and answer it with a fixed
/// `status` / `content_type` / `body`. Returns the provider base URL.
async fn spawn_upstream_response(
    status: &'static str,
    content_type: &'static str,
    body: &'static str,
) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock upstream");
    let addr = listener.local_addr().expect("mock upstream address");
    tokio::spawn(async move {
        if let Ok((mut socket, _peer)) = listener.accept().await {
            let mut buffer = [0_u8; 16 * 1024];
            let _ = socket.read(&mut buffer).await;
            let headers = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut out = headers.into_bytes();
            out.extend_from_slice(body.as_bytes());
            let _ = socket.write_all(&out).await;
        }
    });
    format!("http://{addr}/v1")
}

async fn post_responses(state: &AppState, body: Value) -> axum::response::Response {
    build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/responses")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).expect("request body")))
                .expect("request"),
        )
        .await
        .expect("response")
}

#[test]
fn responses_bridge_string_input_and_instructions_become_chat_messages() {
    let converted = responses_to_openai_chat_body(&json!({
        "model": "responses-model",
        "instructions": "Be concise.",
        "input": "hello world",
        "max_output_tokens": 64,
        "temperature": 0.2
    }))
    .expect("string input conversion");
    assert_eq!(converted["model"], "responses-model");
    assert_eq!(converted["max_tokens"], 64);
    assert_eq!(converted["temperature"], 0.2);
    assert_eq!(converted["stream"], false);
    let messages = converted["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0],
        json!({"role": "system", "content": "Be concise."})
    );
    assert_eq!(
        messages[1],
        json!({"role": "user", "content": "hello world"})
    );
}

#[test]
fn responses_bridge_message_function_call_and_output_items_become_chat_messages() {
    let converted = responses_to_openai_chat_body(&json!({
        "input": [
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "What is 2+2?"}]},
            {"type": "function_call", "call_id": "call_1", "name": "add", "arguments": "{\"a\":2,\"b\":2}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "4"}
        ]
    }))
    .expect("item conversion");
    let messages = converted["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages[0],
        json!({"role": "user", "content": "What is 2+2?"})
    );
    assert_eq!(
        messages[1],
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {"name": "add", "arguments": "{\"a\":2,\"b\":2}"}
            }]
        })
    );
    assert_eq!(
        messages[2],
        json!({"role": "tool", "tool_call_id": "call_1", "content": "4"})
    );
}

#[test]
fn responses_bridge_converts_function_tools_and_fails_closed_on_hosted() {
    let converted = responses_to_openai_chat_body(&json!({
        "input": "test",
        "tools": [
            {
                "type": "function",
                "name": "lookup",
                "description": "Look things up",
                "parameters": {"type": "object"},
                "strict": true
            }
        ]
    }))
    .expect("function tool conversion");
    assert_eq!(
        converted["tools"],
        json!([{
            "type": "function",
            "function": {
                "name": "lookup",
                "description": "Look things up",
                "parameters": {"type": "object"},
                "strict": true
            }
        }])
    );

    let error = responses_to_openai_chat_body(&json!({
        "input": "test",
        "tools": [{"type": "hosted", "name": "web_search"}]
    }))
    .expect_err("hosted tool must fail closed");
    assert!(error.contains("hosted"), "unexpected error: {error}");
}

#[test]
fn responses_bridge_validates_tool_choice_and_required_function_fields() {
    for valid_choice in [
        json!("auto"),
        json!("none"),
        json!("required"),
        json!({"type": "function", "name": "lookup"}),
    ] {
        let converted = responses_to_openai_chat_body(&json!({
            "input": "test",
            "tool_choice": valid_choice
        }))
        .expect("valid tool choice");
        assert!(converted.get("tool_choice").is_some());
    }

    for (body, needle) in [
        (
            json!({"input": "test", "tool_choice": "mystery"}),
            "tool_choice",
        ),
        (
            json!({"input": "test", "tool_choice": {"type": "function"}}),
            "name",
        ),
        (
            json!({"input": [{"type": "function_call", "name": "lookup", "arguments": "{}"}]}),
            "call_id",
        ),
        (
            json!({"input": [{"type": "function_call", "call_id": "call_1", "arguments": "{}"}]}),
            "name",
        ),
        (
            json!({"input": [{"type": "function_call_output", "output": "ok"}]}),
            "call_id",
        ),
        (
            json!({"input": [{"type": "function_call", "call_id": "call_1", "name": "lookup"}]}),
            "arguments",
        ),
        (
            json!({"input": [{"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "not-json"}]}),
            "valid JSON",
        ),
        (
            json!({"input": [{"type": "function_call_output", "call_id": "call_1"}]}),
            "output",
        ),
        (
            json!({"input": [{"type": "message", "role": "tool", "content": "no"}]}),
            "role",
        ),
        (
            json!({"input": "test", "tools": [{"type": "function", "name": "lookup", "parameters": []}]}),
            "parameters",
        ),
        (
            json!({"input": "test", "tools": [{"type": "function", "name": "lookup", "description": [], "parameters": {}}]}),
            "description",
        ),
        (
            json!({"input": "test", "tools": [{"type": "function", "name": "lookup", "parameters": {}, "strict": "yes"}]}),
            "strict",
        ),
    ] {
        let error = responses_to_openai_chat_body(&body).expect_err("invalid bridge input");
        assert!(
            error.contains(needle),
            "expected `{needle}` in error message, got: {error}"
        );
    }
}

#[test]
fn responses_bridge_fails_closed_on_reasoning_item_reference_and_unknown_items() {
    for (input, needle) in [
        (json!([{"type": "reasoning"}]), "reasoning"),
        (
            json!([{"type": "item_reference", "id": "item_1"}]),
            "item_reference",
        ),
        (json!([{"type": "mystery_item"}]), "mystery_item"),
        (
            json!([{"type": "message", "role": "user", "content": [{"type": "input_image"}]}]),
            "input_image",
        ),
    ] {
        let error =
            responses_to_openai_chat_body(&json!({"input": input})).expect_err("must fail closed");
        assert!(
            error.contains(needle),
            "expected `{needle}` in error message, got: {error}"
        );
    }
    let error = responses_to_openai_chat_body(&json!({"messages": []}))
        .expect_err("missing input must fail closed");
    assert!(error.contains("input"), "unexpected error: {error}");
}

#[test]
fn responses_bridge_chat_output_back_with_text_tool_calls_and_usage() {
    let upstream = json!({
        "id": "chatcmpl-1",
        "model": "upstream-model",
        "choices": [{
            "index": 0,
            "finish_reason": "tool_calls",
            "message": {
                "role": "assistant",
                "content": "Let me check.",
                "tool_calls": [{
                    "id": "call_9",
                    "type": "function",
                    "function": {"name": "add", "arguments": "{\"a\":2,\"b\":2}"}
                }]
            }
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    });
    let response =
        openai_chat_to_responses_response(&upstream, Some("responses-model"), "resp-test-1");
    assert_eq!(response["object"], "response");
    assert_eq!(response["status"], "completed");
    assert_eq!(response["id"], "resp-test-1");
    let output = response["output"].as_array().expect("output array");
    assert_eq!(output.len(), 2);
    assert_eq!(output[0]["type"], "message");
    assert_eq!(output[0]["content"][0]["type"], "output_text");
    assert_eq!(output[0]["content"][0]["text"], "Let me check.");
    assert_eq!(output[1]["type"], "function_call");
    assert_eq!(output[1]["call_id"], "call_9");
    assert_eq!(output[1]["name"], "add");
    assert_eq!(output[1]["arguments"], "{\"a\":2,\"b\":2}");
    assert_eq!(response["usage"]["input_tokens"], 10);
    assert_eq!(response["usage"]["output_tokens"], 5);
    assert_eq!(response["usage"]["total_tokens"], 15);

    let truncated = json!({
        "choices": [{
            "index": 0,
            "finish_reason": "length",
            "message": {"role": "assistant", "content": "partial"}
        }]
    });
    let response = openai_chat_to_responses_response(&truncated, None, "resp-test-2");
    assert_eq!(response["status"], "incomplete");
}

#[tokio::test]
async fn responses_http_fails_closed_on_reasoning_item_before_route_selection() {
    let state = AppState::new(DtokenConfig::empty());
    let response = post_responses(
        &state,
        json!({
            "model": "responses-model",
            "input": [{"type": "reasoning"}]
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(payload["error"]["type"], "unsupported_responses_input");
    assert!(
        payload["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("reasoning"),
        "unexpected error: {payload}"
    );
}

#[tokio::test]
async fn responses_http_non_stream_returns_responses_output_from_chat_upstream() {
    let base_url = spawn_upstream_response(
        "200 OK",
        "application/json",
        r#"{"id":"chatcmpl-1","object":"chat.completion","model":"upstream-model","choices":[{"index":0,"finish_reason":"tool_calls","message":{"role":"assistant","content":"Let me check.","tool_calls":[{"id":"call_9","type":"function","function":{"name":"add","arguments":"{\"a\":2,\"b\":2}"}}]}}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#,
    )
    .await;
    let state = AppState::new(responses_test_config(base_url));
    let response = post_responses(
        &state,
        json!({
            "model": "responses-model",
            "input": [
                {"type": "function_call", "call_id": "call_9", "name": "add", "arguments": "{}"}
            ],
            "tools": [{"type": "function", "name": "add", "parameters": {"type": "object"}}]
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["object"], "response");
    assert_eq!(payload["output"][0]["type"], "message");
    assert_eq!(payload["output"][0]["content"][0]["text"], "Let me check.");
    assert_eq!(payload["output"][1]["type"], "function_call");
    assert_eq!(payload["output"][1]["call_id"], "call_9");
    assert_eq!(payload["usage"]["input_tokens"], 10);
    assert_eq!(payload["usage"]["output_tokens"], 5);
    assert!(payload.get("dtoken").is_some());
    assert_eq!(payload["dtoken"]["fallback"], "disabled");
}

#[tokio::test]
async fn responses_http_stream_converts_chat_sse_to_responses_sse() {
    let sse = concat!(
        "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":2,\"total_tokens\":4}}\n\n",
        "data: [DONE]\n\n"
    );
    let base_url = spawn_upstream_response("200 OK", "text/event-stream", sse).await;
    let state = AppState::new(responses_test_config(base_url));
    let response = post_responses(
        &state,
        json!({"model": "responses-model", "input": "hi", "stream": true}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert_eq!(content_type, "text/event-stream");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("SSE body");
    let converted = String::from_utf8(bytes.to_vec()).expect("UTF-8 SSE");
    assert!(
        converted.contains("event: response.created"),
        "missing response.created:\n{converted}"
    );
    assert!(
        converted.contains("event: response.output_text.delta"),
        "missing output_text.delta:\n{converted}"
    );
    assert!(
        converted.contains("\"delta\":\"Hel\""),
        "missing first delta payload:\n{converted}"
    );
    assert!(
        converted.contains("\"delta\":\"lo\""),
        "missing second delta payload:\n{converted}"
    );
    assert!(
        converted.contains("event: response.output_item.done"),
        "missing output_item.done:\n{converted}"
    );
    assert!(
        converted.contains("event: response.completed"),
        "missing response.completed:\n{converted}"
    );
    assert!(
        converted.contains("\"status\":\"completed\""),
        "missing completed status:\n{converted}"
    );
    assert!(
        converted.contains("\"text\":\"Hello\""),
        "missing accumulated text:\n{converted}"
    );
    assert!(
        converted.contains("\"input_tokens\":2"),
        "missing mapped usage:\n{converted}"
    );
    assert!(
        !converted.contains("response.failed"),
        "unexpected failure:\n{converted}"
    );
}
