use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use dtoken_config::DtokenConfig;
use dtoken_core_api::{build_router, AppState};
use dtoken_types::{
    BaselineKind, DtokenEvent, EventSource, EventStatus, LayeredStage, OpticalCapabilitySnapshot,
    OpticalMode,
};
use serde_json::{json, Value};
use std::fs;
use tower::ServiceExt;
use uuid::Uuid;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid JSON")
}

fn app() -> axum::Router {
    build_router(AppState::new(DtokenConfig::empty()))
}

fn unsigned_active_snapshot(id: &str, revision: u64) -> OpticalCapabilitySnapshot {
    serde_json::from_value(json!({
        "schema_version": 1,
        "snapshot_id": id,
        "revision": revision,
        "enabled": true,
        "lifecycle": "active",
        "revoked_at_ms": null,
        "revocation_reason": null,
        "supersedes_snapshot_id": null,
        "provider_family": "anthropic",
        "provider_id": null,
        "route_id": null,
        "model_base": "claude-fable-5",
        "endpoint": "anthropic_messages",
        "capability": {
            "provider_family": "anthropic",
            "provider_id": null,
            "model_base": "claude-fable-5",
            "endpoint": "anthropic_messages",
            "status": "supported",
            "max_mode": "tool_results_only",
            "supports_image_input": true,
            "supports_anthropic_image_block": true,
            "supports_openai_image_url": false,
            "supports_openai_input_image": false,
            "supports_data_url": true,
            "max_images_per_request": 10,
            "max_image_width_px": 1568,
            "max_image_height_px": 728,
            "vision_token_formula": {"patch_grid": {"patch_size_px": 28, "max_tokens": 1568, "safety_margin_percent": 105.0}},
            "exactness_class": "technical_readable"
        },
        "verified_at_ms": 100,
        "source": "test-fixture",
        "notes": "unsigned test fixture",
        "attestation": null
    }))
    .expect("unsigned capability fixture")
}

#[tokio::test]
async fn optical_status_defaults_off_and_shadow_only() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/compression/optical/status")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["enabled"], false);
    assert_eq!(payload["default_mode"], "shadow_preview");
    assert_eq!(payload["allow_active"], false);
    assert_eq!(payload["public_summary_only"], true);
    assert_eq!(payload["fail_closed"], true);
}

#[tokio::test]
async fn optical_capabilities_are_redacted_and_fail_closed() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/compression/optical/capabilities")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["active_transform_enabled"], false);
    assert_eq!(payload["max_mode"], "shadow_preview");
    assert_eq!(payload["unknown_model_status"], "shadow_only");
    assert_eq!(payload["unsupported_path_status"], "blocked");
    assert_eq!(payload["public_summary_only"], true);
    assert_eq!(payload["fail_closed"], true);

    let encoded = serde_json::to_string(&payload).expect("capabilities should serialize");
    for forbidden in [
        "provider_id",
        "model_base",
        "vision_token_formula",
        "max_image_width_px",
        "max_image_height_px",
        "pixels_per_token",
        "base64",
    ] {
        assert!(!encoded.contains(forbidden), "leaked field: {forbidden}");
    }
}

#[tokio::test]
async fn optical_preview_returns_public_shadow_summary_without_raw_body() {
    let secret_marker = "raw-optical-preview-secret";
    let request_body = json!({
        "endpoint_path": "/v1/messages",
        "protocol": "anthropic",
        "provider_id": "private-provider",
        "route_id": "private-route",
        "model": "unknown-private-model",
        "body": {
            "messages": [{
                "role": "user",
                "content": secret_marker
            }]
        }
    });
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/compression/optical/preview")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(request_body.to_string()))
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["mode"], "shadow_preview");
    assert_eq!(payload["final_status"], "validated_shadow_plan");

    let encoded = serde_json::to_string(&payload).expect("preview should serialize");
    for forbidden in [
        secret_marker,
        "private-provider",
        "private-route",
        "unknown-private-model",
        "raw_prompt",
        "request_body",
        "data:image",
        "image/png",
        "base64",
    ] {
        assert!(!encoded.contains(forbidden), "leaked value: {forbidden}");
    }
}

#[tokio::test]
async fn optical_preview_rejects_unsupported_path_in_plan() {
    let request_body = json!({
        "endpoint_path": "/unsupported/path",
        "protocol": "anthropic",
        "provider_id": null,
        "route_id": null,
        "model": "claude-fable-5",
        "body": {"messages": [{"role": "user", "content": "hello"}]}
    });
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/compression/optical/preview")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(request_body.to_string()))
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["mode"], "shadow_preview");
    assert_eq!(payload["final_status"], "ineligible");
    assert_eq!(payload["gates"][0]["passed"], false);
    assert_eq!(payload["gates"][0]["reason"], "UnsupportedPath");
}

#[tokio::test]
async fn optical_preview_records_metadata_only_event() {
    let private_marker = "private-marker-should-not-leak";
    let router = app();
    let request_body = json!({
        "endpoint_path": "/v1/messages",
        "protocol": "anthropic",
        "provider_id": "private-provider",
        "route_id": "private-route",
        "model": "unknown-private-model",
        "body": {"messages": [{"role": "user", "content": private_marker}]}
    });
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/compression/optical/preview")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(request_body.to_string()))
                .expect("request should build"),
        )
        .await
        .expect("router should respond");
    assert_eq!(response.status(), StatusCode::OK);

    let events_response = router
        .oneshot(
            Request::builder()
                .uri("/compression/optical/events/recent")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");
    assert_eq!(events_response.status(), StatusCode::OK);
    let events = response_json(events_response).await;
    let event = events
        .as_array()
        .and_then(|items| items.first())
        .expect("one optical preview event");
    assert_eq!(event["stage"], "optical_preview");
    assert_eq!(event["metadata"]["kind"], "optical_preview");
    assert_eq!(event["metadata"]["route_policy_found"], false);
    assert_eq!(event["metadata"]["final_status"], "validated_shadow_plan");
    assert_eq!(event["metadata"]["mode"], "shadow_preview");
    assert_eq!(event["metadata"]["configured_rules"], 0);
    assert_eq!(event["metadata"]["verified_rules"], 0);
    assert_eq!(event["metadata"]["rejected_rules"], 0);
    assert_eq!(event["metadata"]["snapshot_revision_present"], false);
    assert!(event["metadata"]["graph_nodes"].as_u64().is_some());
    assert!(event["metadata"]["graph_edges"].as_u64().is_some());
    assert!(event["metadata"].get("economics").is_none());
    assert!(event["metadata"]["net_saved_tokens"].as_i64().is_some());
    assert_eq!(event["metadata"]["raw_available"], false);

    let encoded = serde_json::to_string(event).expect("event should serialize");
    for forbidden in [
        private_marker,
        "private-provider",
        "private-route",
        "unknown-private-model",
        "request_body",
        "base64",
    ] {
        assert!(!encoded.contains(forbidden), "leaked value: {forbidden}");
    }
}

#[tokio::test]
async fn configured_active_intent_remains_gated_without_route_and_attestation() {
    let mut config = DtokenConfig::empty();
    config.optical.global_policy.enabled = true;
    config.optical.global_policy.allow_active = true;
    config.optical.global_policy.default_mode = OpticalMode::FullExperimental;
    config.optical.global_policy.public_summary_only = false;
    config
        .optical
        .capability_snapshots
        .push(unsigned_active_snapshot("verified-shadow-estimator", 1));
    let router = build_router(AppState::new(config));

    for (path, expected_field) in [
        ("/compression/optical/status", "allow_active"),
        (
            "/compression/optical/capabilities",
            "active_transform_enabled",
        ),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        assert_eq!(payload[expected_field], false);
        if path.ends_with("/status") {
            assert_eq!(payload["configured_mode"], "full_experimental");
            assert_eq!(payload["effective_mode"], "shadow_preview");
            assert_eq!(payload["default_mode"], "shadow_preview");
            assert_eq!(payload["public_summary_only"], true);
        } else {
            assert_eq!(payload["max_mode"], "shadow_preview");
            assert_eq!(payload["public_summary_only"], true);
        }
    }
}

#[tokio::test]
async fn lifecycle_rejects_unsigned_runtime_snapshots() {
    let valid = unsigned_active_snapshot("valid-snapshot", 1);
    let mut invalid = unsigned_active_snapshot("invalid-snapshot", 2);
    invalid.capability.supports_image_input = false;
    let mut config = DtokenConfig::empty();
    config.optical.capability_snapshots = vec![valid, invalid];

    let response = build_router(AppState::new(config))
        .oneshot(
            Request::builder()
                .uri("/compression/optical/capabilities/lifecycle")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["configured_snapshots"], 2);
    assert_eq!(payload["active_snapshots"], 0);
    assert_eq!(payload["verified_rules"], 0);
    assert_eq!(payload["rejected_rules"], 2);
}

#[tokio::test]
async fn recent_optical_events_redact_legacy_or_corrupt_metadata_at_read_boundary() {
    let temp_dir = std::env::temp_dir().join(format!(
        "dtoken-optical-recent-redaction-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_dir).expect("temp runtime directory");
    let secret = "persisted-raw-secret-must-never-leave-api";
    let event = DtokenEvent {
        event_id: Uuid::new_v4(),
        timestamp_ms: 123,
        project_id: Some(secret.to_string()),
        agent_id: Some("private-agent".to_string()),
        instance_id: Some("private-instance".to_string()),
        command_id: Some("private-command".to_string()),
        request_id: Some(Uuid::new_v4()),
        stage: LayeredStage::OpticalPreview,
        source: EventSource::Dtoken,
        status: EventStatus::Ok,
        input_tokens: Some(123),
        output_tokens: Some(456),
        saved_tokens: Some(78),
        baseline_kind: Some(BaselineKind::HeadroomInput),
        error_code: Some("private-error".to_string()),
        metadata: json!({
            "kind": "tampered-kind",
            "final_status": "validated_active_plan",
            "mode": "full_experimental",
            "candidate_blocks": 4,
            "economics": {"net_saved_tokens": 42, "raw": secret},
            "raw_prompt": secret,
            "request_body": {"secret": secret},
            "provider_id": "private-provider",
            "base64": secret
        }),
        context_accounting: None,
    };
    let encoded = serde_json::to_string(&event).expect("event serializes");
    fs::write(temp_dir.join("events.jsonl"), format!("{encoded}\n"))
        .expect("write persisted event");

    let response = build_router(AppState::new_persistent(DtokenConfig::empty(), &temp_dir))
        .oneshot(
            Request::builder()
                .uri("/compression/optical/events/recent")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let event = payload
        .as_array()
        .and_then(|items| items.first())
        .expect("one redacted event");
    assert_eq!(event["metadata"]["kind"], "optical_preview");
    assert_eq!(event["metadata"]["mode"], "shadow_preview");
    assert_eq!(event["metadata"]["final_status"], "failed_closed");
    assert_eq!(event["metadata"]["candidate_blocks"], 4);
    assert_eq!(event["metadata"]["net_saved_tokens"], 42);
    assert_eq!(event["metadata"]["raw_available"], false);

    let public = serde_json::to_string(&payload).expect("public payload serializes");
    for forbidden in [
        secret,
        "raw_prompt",
        "request_body",
        "provider_id",
        "base64",
        "private-agent",
        "private-instance",
        "private-command",
        "private-error",
        "request_id",
        "input_tokens",
        "output_tokens",
    ] {
        assert!(
            !public.contains(forbidden),
            "leaked persisted value: {forbidden}"
        );
    }

    fs::remove_dir_all(&temp_dir).expect("remove temp runtime directory");
}
#[tokio::test]
async fn optical_capability_lifecycle_is_metadata_only() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/compression/optical/capabilities/lifecycle")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["configured_snapshots"], 0);
    assert_eq!(payload["active_snapshots"], 0);
    assert_eq!(payload["verified_rules"], 0);
    let encoded = serde_json::to_string(&payload).expect("lifecycle should serialize");
    assert!(!encoded.contains("provider_id"));
    assert!(!encoded.contains("model_base"));
}
