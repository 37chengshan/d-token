//! C5 exact active contract (PR-C5C/PR-C5D) exercised through the
//! production-shaped config load path with the tracked signed fixtures in
//! `support/fixtures/optical/`:
//!
//! - A test-owned, Ed25519-signed supported route (Anthropic Messages,
//!   claude-sonnet-4-5) loaded through `OpticalRuntimeConfig` +
//!   `OpticalCapabilityRegistry::from_runtime_config` must reach automatic
//!   `Active` with all gates green and no UI switch (OPTICAL-1/5).
//! - Mutation, secret, quality, economics, cache/billing-header and resource
//!   negatives must all preserve the original bytes (OPTICAL-2/4/5/6).
//! - History collapse (OPTICAL-6) wires the d-token-owned planner into the
//!   active path: enabled routes scope candidates to the planner's approved
//!   frozen-history blocks and bind the plan digest to the report.
//!
//! The fixtures are produced by `examples/optical_capability_signer.rs` (the
//! tracked Ed25519 capability/eval authority lifecycle tooling); the demo
//! seed is a fixed, clearly labeled fixture value.

use std::path::PathBuf;

use dtoken_compress::optical::{
    eval_artifact_digest, HybridContextEngine, HybridPreparationKind, OpticalCapabilityRegistry,
    RenderCancellation,
};
use dtoken_compress::ResolvedOptimizationContext;
use dtoken_types::{
    CompressionPolicy, OpticalCapabilitySnapshot, OpticalEvalArtifact, OpticalMode,
    OpticalRuntimeConfig, OpticalTrustedAuthority, Protocol, RouteOpticalPolicy,
    RouteOpticalPolicyEntry,
};
use serde_json::{json, Value};

/// Digest of `anthropic-eval-claude-sonnet-4-5.json` produced by the signer
/// example; a signer/atlas/contract drift breaks this pin and the test.
const PINNED_EVAL_DIGEST: &str = "b0d248bff195081863faa0eaecf58b3d996b8ba41c0ccf3799a75a9245a56740";
/// The unified optical corpus digest (support/fixtures/unified-corpus/
/// manifest.json corpora.optical.digest) bound into the signed eval artifact.
const OPTICAL_CORPUS_DIGEST: &str =
    "45dcd9d54b285c6e71ba37127656898a5ac9217fdfb21e7d1cc08b7af9ec0bbe";


fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../support/fixtures/optical")
        .join(name)
}

fn load_fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let raw = std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|error| panic!("missing optical fixture {name}: {error}"));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("invalid optical fixture {name}: {error}"))
}

fn signed_snapshot() -> OpticalCapabilitySnapshot {
    load_fixture("anthropic-snapshot-claude-sonnet-4-5-signed.json")
}

fn demo_authority() -> OpticalTrustedAuthority {
    load_fixture("demo-authority.json")
}

fn bound_eval() -> OpticalEvalArtifact {
    let eval: OpticalEvalArtifact = load_fixture("anthropic-eval-claude-sonnet-4-5.json");
    assert_eq!(
        eval_artifact_digest(&eval),
        PINNED_EVAL_DIGEST,
        "signed eval artifact drifted from the pinned digest"
    );
    assert_eq!(
        eval.corpus_digest, OPTICAL_CORPUS_DIGEST,
        "eval must bind the canonical unified optical corpus digest"
    );
    assert!(eval.passes_derived_quality_gate());
    eval
}

fn context() -> ResolvedOptimizationContext {
    ResolvedOptimizationContext {
        route_id: "route-anthropic-main".to_string(),
        provider_id: "anthropic-main".to_string(),
        model: "claude-sonnet-4-5".to_string(),
        input_protocol: Protocol::Anthropic,
        output_protocol: Protocol::Anthropic,
        endpoint: dtoken_types::OpticalEndpoint::AnthropicMessages,
    }
}

fn active_global() -> dtoken_types::OpticalGlobalPolicy {
    dtoken_types::OpticalGlobalPolicy {
        enabled: true,
        allow_active: true,
        max_plan_time_ms: 5_000,
        max_render_time_ms: 5_000,
        ..Default::default()
    }
}

fn active_route(allow_history: bool) -> RouteOpticalPolicy {
    RouteOpticalPolicy {
        enabled: true,
        mode: OpticalMode::ToolResultsOnly,
        allow_tool_result: true,
        allow_history,
        min_net_saved_tokens: 1,
        confirmed_experimental_lossy: true,
        ..Default::default()
    }
}

fn signed_runtime_config(
    snapshot: OpticalCapabilitySnapshot,
    eval: OpticalEvalArtifact,
    global: dtoken_types::OpticalGlobalPolicy,
    route: RouteOpticalPolicy,
) -> OpticalRuntimeConfig {
    OpticalRuntimeConfig {
        schema_version: 1,
        global_policy: global,
        route_policies: vec![RouteOpticalPolicyEntry {
            route_id: "route-anthropic-main".to_string(),
            policy: route,
        }],
        capability_snapshots: vec![snapshot],
        trusted_authorities: vec![demo_authority()],
        eval_artifacts: vec![eval],
    }
}

/// The exact supported Anthropic route request: one dense closed historical
/// tool result (turn 0), a recent tail, then the current turn. Mirrors the
/// engine unit fixtures so the Active path is exercised end to end.
fn exact_request_body() -> Value {
    let dense_line = format!("INFO {}\n", "historical operation completed ".repeat(16));
    let log = dense_line.repeat(40);
    let mut messages = vec![
        json!({"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"read","input":{"path":"/tmp/a"}}]}),
        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":log}]}),
    ];
    for index in 0..6 {
        messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":format!("recent-{index}"),"name":"read","input":{}}]}));
        messages.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":format!("recent-{index}"),"content":"recent"}]}));
    }
    messages.push(json!({"role":"user","content":[{"type":"text","text":"current turn"}]}));
    json!({
        "model": "claude-sonnet-4-5",
        "system": "protected system instruction",
        "tools": [{"name":"read","input_schema":{"type":"object"}}],
        "messages": messages
    })
}

fn prepare(
    global: &dtoken_types::OpticalGlobalPolicy,
    route: &RouteOpticalPolicy,
    registry: &OpticalCapabilityRegistry,
    body: Value,
) -> dtoken_compress::optical::PreparedHybridRequest {
    let raw = serde_json::to_vec(&body).expect("fixture serializes");
    HybridContextEngine::new(global).prepare(
        "c5-contract-request",
        &context(),
        CompressionPolicy::Balanced,
        body.clone(),
        Some(&raw),
        global,
        route,
        registry,
        &RenderCancellation::default(),
    )
}

#[test]
fn signed_supported_route_reaches_automatic_active_with_all_gates_green() {
    // Acceptance: a test-owned signed supported route (Ed25519 capability +
    // bound passing eval + attested economics, loaded exactly like a
    // production runtime config) makes the real active path reachable, with
    // protected text preserved byte-for-byte.
    let eval = bound_eval();
    let snapshot = signed_snapshot();
    let config = signed_runtime_config(
        snapshot.clone(),
        eval,
        active_global(),
        active_route(false),
    );
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &config,
        now_ms(),
    );
    assert_eq!(registry.verified_rule_count(), 1, "signed demo capability must verify");
    assert_eq!(registry.rejected_rule_count(), 0);

    let body = exact_request_body();
    let prepared = prepare(&config.global_policy, &config.route_policies[0].policy, &registry, body.clone());
    assert_eq!(
        prepared.kind(),
        HybridPreparationKind::Active,
        "{}",
        prepared.report().reason_code
    );
    assert_eq!(prepared.report().capability_revision, Some(1));
    assert!(prepared.report().artifact_validated);
    assert!(prepared.report().image_count > 0);
    assert!(prepared.report().png_bytes > 0);
    assert!(prepared.report().estimated_signed_input_savings > 0);
    assert_eq!(
        prepared.accounting().disposition,
        dtoken_types::OptimizationDisposition::Active
    );
    assert_eq!(
        prepared.accounting().sent_payload,
        dtoken_types::SentPayloadVariant::Optimized
    );

    // Protected text: system, tools, first (tool_use) message, recent tail
    // and current turn stay byte-identical; only the approved historical
    // tool result becomes the typed text+image capsule.
    let patched = prepared.body();
    assert_eq!(patched["model"], body["model"]);
    assert_eq!(patched["system"], body["system"]);
    assert_eq!(patched["tools"], body["tools"]);
    assert_eq!(patched["messages"][0], body["messages"][0]);
    let patched_content = patched
        .pointer("/messages/1/content/0/content")
        .and_then(Value::as_array)
        .expect("approved tool result is replaced by typed content blocks");
    assert_eq!(patched_content[0]["type"], "text");
    assert_eq!(patched_content[1]["type"], "image");
    for index in 2..body["messages"].as_array().expect("messages").len() {
        assert_eq!(patched["messages"][index], body["messages"][index]);
    }
}

#[test]
fn mutated_snapshot_identity_preserves_original_bytes() {
    // Tampering with any signature-bound field breaks the Ed25519
    // attestation; the registry rejects the rule and the request keeps the
    // original body.
    let eval = bound_eval();
    let mut snapshot = signed_snapshot();
    snapshot.model_base = "claude-opus-4-6".to_string();
    let config = signed_runtime_config(snapshot, eval, active_global(), active_route(false));
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &config,
        now_ms(),
    );
    assert_eq!(registry.verified_rule_count(), 0);
    assert_eq!(registry.rejected_rule_count(), 1);

    let body = exact_request_body();
    let prepared = prepare(&config.global_policy, &config.route_policies[0].policy, &registry, body.clone());
    assert_ne!(prepared.kind(), HybridPreparationKind::Active);
    assert_eq!(prepared.body(), &body);
    assert_eq!(
        prepared.accounting().sent_payload,
        dtoken_types::SentPayloadVariant::Original
    );
}

#[test]
fn secret_tool_result_preserves_original_bytes() {
    // OPTICAL-4/6: secret-shaped content is pinned by the shared classifier;
    // it never becomes an approved candidate, so the request stays
    // byte-identical even with a fully signed active lane.
    let eval = bound_eval();
    let snapshot = signed_snapshot();
    let config = signed_runtime_config(snapshot, eval, active_global(), active_route(false));
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &config,
        now_ms(),
    );
    assert_eq!(registry.verified_rule_count(), 1);

    let mut body = exact_request_body();
    body["messages"][1]["content"][0]["content"] = json!(
        "Authorization: Bearer sk-ant-fixture-0123456789abcdef0123456789abcdef\nINFO historical operation completed\n".repeat(40)
    );
    let prepared = prepare(&config.global_policy, &config.route_policies[0].policy, &registry, body.clone());
    assert_ne!(prepared.kind(), HybridPreparationKind::Active);
    assert_eq!(prepared.body(), &body);
    assert_eq!(
        prepared.accounting().sent_payload,
        dtoken_types::SentPayloadVariant::Original
    );
}

#[test]
fn failing_bound_eval_preserves_original_bytes() {
    // OPTICAL-5: a bound eval that no longer passes the derived quality gate
    // (e.g. candidate accuracy regression) keeps the lane shadow even though
    // the signature still verifies.
    let mut eval = bound_eval();
    eval.candidate_passes = 19;
    eval.quality_delta = -0.05;
    eval.passed = true; // self-reported pass must not override derivation
    let snapshot = signed_snapshot();
    let config = signed_runtime_config(snapshot, eval, active_global(), active_route(false));
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &config,
        now_ms(),
    );
    assert_eq!(registry.verified_rule_count(), 0);
    assert_eq!(registry.rejected_rule_count(), 1);

    let body = exact_request_body();
    let prepared = prepare(&config.global_policy, &config.route_policies[0].policy, &registry, body.clone());
    assert_ne!(prepared.kind(), HybridPreparationKind::Active);
    assert_eq!(prepared.body(), &body);
    assert_eq!(
        prepared.accounting().sent_payload,
        dtoken_types::SentPayloadVariant::Original
    );
}

#[test]
fn missing_attested_economics_preserves_original_bytes() {
    // OPTICAL-5: without the signature-bound economics contract the rule is
    // rejected at registry load (economics contract missing), so the request
    // cannot reach active; the original bytes reach the wire unchanged.
    // Depending on where the gate closes (registry rejection vs. the active
    // economics gate) the kind is Shadow or FallbackPreSend — never Active.
    let eval = bound_eval();
    let mut snapshot = signed_snapshot();
    snapshot.economics = None;
    let config = signed_runtime_config(snapshot, eval, active_global(), active_route(false));
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &config,
        now_ms(),
    );
    assert_eq!(registry.verified_rule_count(), 0);
    assert_eq!(registry.rejected_rule_count(), 1);

    let body = exact_request_body();
    let prepared = prepare(&config.global_policy, &config.route_policies[0].policy, &registry, body.clone());
    assert_ne!(prepared.kind(), HybridPreparationKind::Active);
    assert_eq!(prepared.body(), &body);
    assert_eq!(
        prepared.accounting().sent_payload,
        dtoken_types::SentPayloadVariant::Original
    );
}

#[test]
fn volatile_billing_header_candidate_preserves_original_bytes() {
    // OPTICAL-6: a billing header / per-turn volatile block is never
    // cache-stable, so it cannot become an active candidate; the request
    // stays byte-identical.
    let eval = bound_eval();
    let snapshot = signed_snapshot();
    let config = signed_runtime_config(snapshot, eval, active_global(), active_route(false));
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &config,
        now_ms(),
    );
    assert_eq!(registry.verified_rule_count(), 1);

    let mut body = exact_request_body();
    body["messages"][1]["content"][0]["content"] = json!(
        "X-Billing-Usage: 1234\nDate: 2026-07-08\nINFO historical operation completed\n".repeat(40)
    );
    let prepared = prepare(&config.global_policy, &config.route_policies[0].policy, &registry, body.clone());
    assert_ne!(prepared.kind(), HybridPreparationKind::Active);
    assert_eq!(prepared.body(), &body);
    assert_eq!(
        prepared.accounting().sent_payload,
        dtoken_types::SentPayloadVariant::Original
    );
}

#[test]
fn render_resource_limit_preserves_original_bytes() {
    // OPTICAL-4: resource budgets are fail-closed. A PNG byte budget that
    // cannot hold the rendered capsule falls back pre-send with the original
    // bytes.
    let eval = bound_eval();
    let snapshot = signed_snapshot();
    let global = dtoken_types::OpticalGlobalPolicy {
        max_png_bytes: 1_024,
        ..active_global()
    };
    let config = signed_runtime_config(snapshot, eval, global, active_route(false));
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &config,
        now_ms(),
    );
    assert_eq!(registry.verified_rule_count(), 1);

    let body = exact_request_body();
    let prepared = prepare(&config.global_policy, &config.route_policies[0].policy, &registry, body.clone());
    assert_eq!(prepared.kind(), HybridPreparationKind::FallbackPreSend);
    assert_eq!(prepared.body(), &body);
    assert_eq!(
        prepared.accounting().sent_payload,
        dtoken_types::SentPayloadVariant::Original
    );
}

#[test]
fn history_collapse_scopes_active_candidates_and_binds_plan_digest() {
    // OPTICAL-6: with allow_history, the active candidate set is scoped by
    // the d-token-owned planner (static slab + freeze grid + cache anchor +
    // billing boundary). The plan digest is bound to the report and the
    // protected fields stay byte-identical.
    let eval = bound_eval();
    let snapshot = signed_snapshot();
    let config = signed_runtime_config(snapshot, eval, active_global(), active_route(true));
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &config,
        now_ms(),
    );
    assert_eq!(registry.verified_rule_count(), 1);

    let body = exact_request_body();
    let prepared = prepare(&config.global_policy, &config.route_policies[0].policy, &registry, body.clone());
    assert_eq!(
        prepared.kind(),
        HybridPreparationKind::Active,
        "{}",
        prepared.report().reason_code
    );
    let digest = prepared
        .report()
        .history_collapse_digest
        .as_deref()
        .expect("history collapse plan digest must be bound on the active path");
    assert_eq!(digest.len(), 64);
    assert!(prepared
        .report()
        .transforms_applied
        .iter()
        .any(|transform| transform == "history_collapse_v1"));
    assert_eq!(prepared.body()["system"], body["system"]);
    assert_eq!(prepared.body()["tools"], body["tools"]);
    assert_eq!(prepared.body()["messages"][0], body["messages"][0]);
    for index in 2..body["messages"].as_array().expect("messages").len() {
        assert_eq!(prepared.body()["messages"][index], body["messages"][index]);
    }
}

#[test]
fn history_collapse_without_approved_frozen_history_preserves_original_bytes() {
    // OPTICAL-6 fail-closed: allow_history with no frozen-history candidate
    // (only the recent tail is collapseable) must not collapse anything; the
    // request preserves the original bytes.
    let eval = bound_eval();
    let snapshot = signed_snapshot();
    let config = signed_runtime_config(snapshot, eval, active_global(), active_route(true));
    let registry = OpticalCapabilityRegistry::from_runtime_config(
        &config,
        now_ms(),
    );
    assert_eq!(registry.verified_rule_count(), 1);

    // Move the dense tool result into the recent tail: no frozen history
    // exists below the tail, so the planner approves nothing.
    let mut body = exact_request_body();
    let dense = body["messages"][1].clone();
    body["messages"][1] = json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"recent"}]});
    body["messages"].as_array_mut().expect("messages").push(
        json!({"role":"assistant","content":[{"type":"tool_use","id":"tool-recent","name":"read","input":{}}]}),
    );
    body["messages"].as_array_mut().expect("messages").push(dense);
    let prepared = prepare(&config.global_policy, &config.route_policies[0].policy, &registry, body.clone());
    assert_ne!(prepared.kind(), HybridPreparationKind::Active);
    assert_eq!(prepared.body(), &body);
    assert!(prepared.report().history_collapse_digest.is_none());
    assert_eq!(
        prepared.accounting().sent_payload,
        dtoken_types::SentPayloadVariant::Original
    );
}
