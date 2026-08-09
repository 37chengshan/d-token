// Bounded, non-sensitive exact-route evidence for the N10 U7 gate.
//
// This module turns the configured optical attestation surface into a
// machine-readable evidence list: route/provider/model/API identity, the
// renderer/font/profile/corpus/tokenizer/pricing/cache/eval digests bound by
// the Ed25519 attestation, lifecycle times, and the paired fidelity/cost/
// latency eval numbers. It never emits signatures, request bodies, source
// text, PNG/base64 bytes, credentials or secrets.

use dtoken_types::{
    OpticalCapabilityLifecycle, OpticalEconomicsAttestation, OpticalEndpoint, OpticalEvalArtifact,
    OpticalExactRouteEvidence, OpticalRuntimeConfig,
};
use sha2::Digest;

use super::capability::{identity_contains_wildcard, OpticalCapabilityRegistry};

const EXACT_EVIDENCE_SCHEMA_VERSION: u32 = 1;

fn api_revision_for_endpoint(endpoint: &OpticalEndpoint) -> String {
    match endpoint {
        OpticalEndpoint::AnthropicMessages => "anthropic-messages-v1".to_string(),
        OpticalEndpoint::OpenAiChatCompletions => "openai-chat-completions-v1".to_string(),
        OpticalEndpoint::OpenAiResponses => "openai-responses-v1".to_string(),
        OpticalEndpoint::Unsupported => "unsupported".to_string(),
    }
}

fn bound_eval<'a>(
    config: &'a OpticalRuntimeConfig,
    expected_digest: &str,
) -> Option<&'a OpticalEvalArtifact> {
    config
        .eval_artifacts
        .iter()
        .find(|artifact| eval_artifact_digest(artifact) == expected_digest)
}

fn economics_bound(economics: &Option<OpticalEconomicsAttestation>) -> bool {
    economics
        .as_ref()
        .is_some_and(OpticalEconomicsAttestation::is_well_formed)
}

fn exact_identity(
    provider_id: &Option<String>,
    route_id: &Option<String>,
    model_base: &str,
) -> bool {
    !identity_contains_wildcard(model_base)
        && provider_id
            .as_deref()
            .is_none_or(|id| !identity_contains_wildcard(id) && !id.trim().is_empty())
        && route_id
            .as_deref()
            .is_none_or(|id| !identity_contains_wildcard(id) && !id.trim().is_empty())
}

/// Emits bounded non-sensitive exact-route evidence for every configured
/// capability snapshot. `active_reachable` is true only when the registry
/// verified the snapshot (Ed25519 attestation + bound passing eval + attested
/// economics) and the lifecycle/identity gates are green.
pub fn optical_exact_route_evidence(
    config: &OpticalRuntimeConfig,
    registry: &OpticalCapabilityRegistry,
    now_ms: u64,
) -> Vec<OpticalExactRouteEvidence> {
    config
        .capability_snapshots
        .iter()
        .map(|snapshot| {
            let attestation = snapshot.attestation.as_ref();
            let verified = registry.verified_snapshot(&snapshot.snapshot_id).is_some();
            let eval = attestation
                .and_then(|attestation| bound_eval(config, &attestation.eval_artifact_digest));
            let eval_gate_passed = eval.is_some_and(|artifact| {
                artifact.passes_derived_quality_gate() && !artifact.corpus_digest.trim().is_empty()
            });
            let economics_present = economics_bound(&snapshot.economics);
            let identity_exact = exact_identity(
                &snapshot.provider_id,
                &snapshot.route_id,
                &snapshot.model_base,
            );
            let not_expired =
                attestation.is_none_or(|attestation| attestation.expires_at_ms > now_ms);
            let active_reachable = verified
                && eval_gate_passed
                && economics_present
                && identity_exact
                && not_expired
                && snapshot.lifecycle == OpticalCapabilityLifecycle::Active;

            OpticalExactRouteEvidence {
                schema_version: EXACT_EVIDENCE_SCHEMA_VERSION,
                snapshot_id: snapshot.snapshot_id.clone(),
                revision: snapshot.revision,
                lifecycle: snapshot.lifecycle,
                provider_family: snapshot.provider_family.clone(),
                provider_id: snapshot
                    .provider_id
                    .clone()
                    .unwrap_or_else(|| "*".to_string()),
                route_id: snapshot.route_id.clone().unwrap_or_else(|| "*".to_string()),
                model_base: snapshot.model_base.clone(),
                endpoint: snapshot.endpoint.clone(),
                api_revision: api_revision_for_endpoint(&snapshot.endpoint),
                verified_at_ms: snapshot.verified_at_ms,
                issued_at_ms: attestation.map(|attestation| attestation.issued_at_ms),
                expires_at_ms: attestation.map(|attestation| attestation.expires_at_ms),
                renderer_profile_digest: attestation
                    .map(|attestation| attestation.renderer_profile_digest.clone())
                    .unwrap_or_default(),
                font_digest: attestation
                    .map(|attestation| attestation.font_digest.clone())
                    .unwrap_or_default(),
                transform_contract_digest: attestation
                    .map(|attestation| attestation.transform_contract_digest.clone())
                    .unwrap_or_default(),
                tokenizer_revision: snapshot
                    .economics
                    .as_ref()
                    .map(|economics| economics.tokenizer_revision.clone())
                    .unwrap_or_default(),
                pricing_revision: snapshot
                    .economics
                    .as_ref()
                    .map(|economics| economics.pricing_revision.clone())
                    .unwrap_or_default(),
                cache_policy_revision: snapshot
                    .economics
                    .as_ref()
                    .map(|economics| economics.cache_policy_revision.clone())
                    .unwrap_or_default(),
                corpus_digest: eval
                    .map(|artifact| artifact.corpus_digest.clone())
                    .unwrap_or_default(),
                eval_artifact_digest: attestation
                    .map(|attestation| attestation.eval_artifact_digest.clone())
                    .unwrap_or_default(),
                eval_sample_count: eval.map(|artifact| artifact.sample_count).unwrap_or(0),
                eval_exact_failures: eval.map(|artifact| artifact.exact_failures).unwrap_or(0),
                eval_unsafe_failures: eval.map(|artifact| artifact.unsafe_failures).unwrap_or(0),
                eval_baseline_passes: eval.map(|artifact| artifact.baseline_passes).unwrap_or(0),
                eval_candidate_passes: eval.map(|artifact| artifact.candidate_passes).unwrap_or(0),
                eval_quality_delta: eval
                    .map(|artifact| artifact.quality_delta)
                    .unwrap_or_default(),
                paired_input_token_delta: eval
                    .map(|artifact| artifact.signed_input_token_delta)
                    .unwrap_or_default(),
                paired_cost_delta_microusd: eval
                    .map(|artifact| artifact.signed_cost_delta_microusd)
                    .unwrap_or_default(),
                paired_latency_p95_delta_ms: eval
                    .map(|artifact| artifact.latency_p95_delta_ms)
                    .unwrap_or_default(),
                attested: attestation.is_some(),
                eval_gate_passed,
                economics_bound: economics_present,
                exact_identity: identity_exact,
                active_reachable,
            }
        })
        .collect()
}

pub(crate) fn eval_artifact_digest(artifact: &OpticalEvalArtifact) -> String {
    format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_vec(artifact).unwrap_or_default())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optical::capability::signed_runtime_config_fixture;

    #[test]
    fn green_exact_route_emits_active_reachable_evidence() {
        let config = signed_runtime_config_fixture();
        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
        assert_eq!(registry.verified_rule_count(), 1);

        let evidence = optical_exact_route_evidence(&config, &registry, 200);
        assert_eq!(evidence.len(), 1);
        let entry = &evidence[0];
        assert!(entry.attested);
        assert!(entry.eval_gate_passed);
        assert!(entry.economics_bound);
        assert!(entry.exact_identity);
        assert!(entry.active_reachable);
        assert_eq!(entry.provider_id, "anthropic-main");
        assert_eq!(entry.route_id, "route-main");
        assert_eq!(entry.model_base, "claude-fable-5");
        assert_eq!(entry.api_revision, "anthropic-messages-v1");
        assert!(entry.paired_input_token_delta > 0);
        assert!(entry.paired_cost_delta_microusd > 0);
        assert!(entry.paired_latency_p95_delta_ms <= 2_000);
        assert!(!entry.renderer_profile_digest.is_empty());
        assert!(!entry.font_digest.is_empty());
        assert!(!entry.transform_contract_digest.is_empty());
        assert!(!entry.corpus_digest.is_empty());
        assert!(!entry.tokenizer_revision.is_empty());
        assert!(!entry.pricing_revision.is_empty());
        assert!(!entry.cache_policy_revision.is_empty());
    }

    #[test]
    fn stale_or_revoked_or_wildcard_evidence_is_never_active_reachable() {
        let mut config = signed_runtime_config_fixture();
        let expired = config.clone();
        let now = 1_001;
        let registry = OpticalCapabilityRegistry::from_runtime_config(&expired, now);
        let evidence = optical_exact_route_evidence(&expired, &registry, now);
        assert!(!evidence[0].active_reachable);

        let mut revoked = config.clone();
        revoked.capability_snapshots[0].lifecycle = OpticalCapabilityLifecycle::Revoked;
        revoked.capability_snapshots[0].revoked_at_ms = Some(250);
        revoked.capability_snapshots[0].revocation_reason = Some("fixture".to_string());
        let registry = OpticalCapabilityRegistry::from_runtime_config(&revoked, 200);
        let evidence = optical_exact_route_evidence(&revoked, &registry, 200);
        assert!(!evidence[0].active_reachable);

        config.capability_snapshots[0].route_id = Some("*".to_string());
        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
        let evidence = optical_exact_route_evidence(&config, &registry, 200);
        assert!(!evidence[0].active_reachable);
        assert!(!evidence[0].exact_identity);
    }

    #[test]
    fn evidence_never_leaks_sensitive_content() {
        let config = signed_runtime_config_fixture();
        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
        let evidence = optical_exact_route_evidence(&config, &registry, 200);
        let encoded = serde_json::to_string(&evidence).expect("evidence serializes");

        assert!(!encoded.contains("signature"));
        assert!(!encoded.contains("secret"));
        assert!(!encoded.contains("base64"));
        assert!(!encoded.contains("png"));
        assert!(!encoded.contains("prompt"));
        assert!(!encoded.contains("renderer bytes"));
    }
}
