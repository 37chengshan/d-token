//! RouteDecision stable digest tests.
//!
//! Canonical-digest requirements under test:
//! - the digest is deterministic: identical decisions produce identical
//!   digests;
//! - any field change changes the digest (every serialized field binds);
//! - the digest is canonical (sorted keys, no whitespace), so it is stable
//!   across processes and serde versions.

use dtoken_types::{ChainMode, CompressionPolicy, Protocol, RouteDecision};
use uuid::Uuid;

fn decision() -> RouteDecision {
    RouteDecision {
        request_id: Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef),
        route_id: "route-primary".to_string(),
        provider_id: "provider-primary".to_string(),
        chain_mode: ChainMode::Direct,
        compression_policy: CompressionPolicy::Balanced,
        reason: "matched instance id and input protocol".to_string(),
        input_protocol: Some(Protocol::OpenAi),
        output_protocol: Some(Protocol::OpenAi),
    }
}

#[test]
fn stable_digest_is_deterministic_for_identical_decisions() {
    assert_eq!(decision().stable_digest(), decision().stable_digest());
    assert_eq!(decision().stable_digest().len(), 64, "sha256 hex");
}

#[test]
fn any_field_change_changes_the_digest() {
    let baseline = decision().stable_digest();

    let mut route_id = decision();
    route_id.route_id = "route-secondary".to_string();
    assert_ne!(baseline, route_id.stable_digest(), "route_id binds");

    let mut provider_id = decision();
    provider_id.provider_id = "provider-secondary".to_string();
    assert_ne!(baseline, provider_id.stable_digest(), "provider_id binds");

    let mut chain_mode = decision();
    chain_mode.chain_mode = ChainMode::ChainBridge;
    assert_ne!(baseline, chain_mode.stable_digest(), "chain_mode binds");

    let mut policy = decision();
    policy.compression_policy = CompressionPolicy::Aggressive;
    assert_ne!(baseline, policy.stable_digest(), "compression_policy binds");

    let mut reason = decision();
    reason.reason = "different reason".to_string();
    assert_ne!(baseline, reason.stable_digest(), "reason binds");

    let mut input_protocol = decision();
    input_protocol.input_protocol = Some(Protocol::Anthropic);
    assert_ne!(
        baseline,
        input_protocol.stable_digest(),
        "input_protocol binds"
    );

    let mut output_protocol = decision();
    output_protocol.output_protocol = None;
    assert_ne!(
        baseline,
        output_protocol.stable_digest(),
        "output_protocol binds"
    );
}

#[test]
fn digest_is_stable_across_round_trips() {
    // Canonical serialization means a deserialized decision digests identically.
    let original = decision();
    let value = serde_json::to_value(&original).unwrap();
    let restored: RouteDecision = serde_json::from_value(value).unwrap();
    assert_eq!(original.stable_digest(), restored.stable_digest());
}
