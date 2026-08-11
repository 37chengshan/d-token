use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use dtoken_types::{
    CapabilityStatus, OpticalCapabilityLifecycle, OpticalCapabilitySnapshot,
    OpticalEconomicsAttestation, OpticalEndpoint, OpticalEvalArtifact, OpticalMode,
    OpticalModelCapability, OpticalRuntimeConfig, OpticalTrustedAuthority, Protocol,
    ProviderFamily, VisionExactnessClass, VisionTokenFormula,
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use super::contract::OpticalTransformContractV2;
use super::economics::economics_contract_from_attested;
use super::render_geometry::geometry_for_capability;
use super::renderer::{font_digest_for, production_render_contract, render_contract_digest};

#[cfg(test)]
use ed25519_dalek::Signer;

#[derive(Debug, Clone)]
pub struct ResolvedOpticalCapability {
    pub capability: OpticalModelCapability,
    pub snapshot_id: Option<String>,
    pub revision: Option<u64>,
    /// Attested economics bound by the resolved snapshot, when present.
    pub economics: Option<OpticalEconomicsAttestation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityRegistryError {
    DisabledSnapshot,
    UnsupportedSchemaVersion,
    DuplicateSnapshotId,
    RevokedSnapshot,
    SupersededSnapshot,
    InvalidLifecycleMetadata,
    InvalidIdentity,
    WildcardIdentity,
    InvalidRevision,
    CapabilityNotSupported,
    CapabilityIdentityMismatch,
    CapabilityPolicyMismatch,
    MissingAttestation,
    UnsupportedAttestationVersion,
    UnknownAuthority,
    InvalidAuthorityKey,
    InvalidSignature,
    AttestationExpired,
    AttestationNotYetValid,
    RendererProfileMismatch,
    FontMismatch,
    EvalArtifactMissing,
    EvalArtifactRejected,
    EconomicsContractMissing,
}

#[derive(Debug, Clone, Default)]
pub struct OpticalCapabilityRegistry {
    verified_rules: Vec<OpticalCapabilitySnapshot>,
    snapshot_ids: HashSet<String>,
    configured_rules: usize,
    rejected_rules: usize,
    revoked_rules: usize,
    superseded_rules: usize,
    blocked_through_revision: HashMap<String, u64>,
}

impl OpticalCapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn from_snapshots(
        snapshots: impl IntoIterator<Item = OpticalCapabilitySnapshot>,
    ) -> Self {
        let mut registry = Self::new();
        for snapshot in snapshots {
            registry.configured_rules = registry.configured_rules.saturating_add(1);
            let lifecycle_metadata_valid = lifecycle_metadata_is_valid(&snapshot);
            let lifecycle_blocks_older = lifecycle_can_block(&snapshot);
            if lifecycle_blocks_older {
                let key = snapshot_identity_key(&snapshot);
                registry
                    .blocked_through_revision
                    .entry(key)
                    .and_modify(|revision| *revision = (*revision).max(snapshot.revision))
                    .or_insert(snapshot.revision);
            }
            match (snapshot.lifecycle, lifecycle_metadata_valid) {
                (OpticalCapabilityLifecycle::Revoked, true) => {
                    registry.revoked_rules = registry.revoked_rules.saturating_add(1);
                }
                (OpticalCapabilityLifecycle::Superseded, true) => {
                    registry.superseded_rules = registry.superseded_rules.saturating_add(1);
                }
                _ => {}
            }
            if registry.register_snapshot(snapshot).is_err() {
                registry.rejected_rules = registry.rejected_rules.saturating_add(1);
            }
        }
        registry
    }

    pub fn from_runtime_config(config: &OpticalRuntimeConfig, now_ms: u64) -> Self {
        let mut registry = Self::new();
        let authorities = config
            .trusted_authorities
            .iter()
            .map(|authority| {
                (
                    format!("{}|{}", authority.authority_id, authority.key_id),
                    authority,
                )
            })
            .collect::<HashMap<_, _>>();
        let evals = config
            .eval_artifacts
            .iter()
            .map(|artifact| (eval_artifact_digest(artifact), artifact))
            .collect::<HashMap<_, _>>();

        for snapshot in config.capability_snapshots.iter().cloned() {
            registry.configured_rules = registry.configured_rules.saturating_add(1);
            if lifecycle_can_block(&snapshot) {
                let key = snapshot_identity_key(&snapshot);
                registry
                    .blocked_through_revision
                    .entry(key)
                    .and_modify(|revision| *revision = (*revision).max(snapshot.revision))
                    .or_insert(snapshot.revision);
            }
            match snapshot.lifecycle {
                OpticalCapabilityLifecycle::Revoked => {
                    registry.revoked_rules = registry.revoked_rules.saturating_add(1)
                }
                OpticalCapabilityLifecycle::Superseded => {
                    registry.superseded_rules = registry.superseded_rules.saturating_add(1)
                }
                OpticalCapabilityLifecycle::Active => {}
            }
            if registry
                .register_attested_snapshot(snapshot, &authorities, &evals, now_ms)
                .is_err()
            {
                registry.rejected_rules = registry.rejected_rules.saturating_add(1);
            }
        }
        registry
    }

    pub fn configured_rule_count(&self) -> usize {
        self.configured_rules
    }

    pub fn verified_rule_count(&self) -> usize {
        self.verified_rules.len()
    }

    pub fn rejected_rule_count(&self) -> usize {
        self.rejected_rules
    }

    pub fn revoked_rule_count(&self) -> usize {
        self.revoked_rules
    }

    pub fn superseded_rule_count(&self) -> usize {
        self.superseded_rules
    }

    /// Returns the verified snapshot for `snapshot_id`, when the registry
    /// accepted it after full attestation/eval/economics validation.
    pub fn verified_snapshot(&self, snapshot_id: &str) -> Option<&OpticalCapabilitySnapshot> {
        self.verified_rules
            .iter()
            .find(|snapshot| snapshot.snapshot_id == snapshot_id)
    }

    pub fn verified_snapshots(&self) -> impl Iterator<Item = &OpticalCapabilitySnapshot> {
        self.verified_rules.iter()
    }

    pub(crate) fn register_snapshot(
        &mut self,
        mut snapshot: OpticalCapabilitySnapshot,
    ) -> Result<(), CapabilityRegistryError> {
        if !snapshot.enabled {
            return Err(CapabilityRegistryError::DisabledSnapshot);
        }
        if snapshot.schema_version != 1 {
            return Err(CapabilityRegistryError::UnsupportedSchemaVersion);
        }
        snapshot.snapshot_id = snapshot.snapshot_id.trim().to_string();
        if self.snapshot_ids.contains(&snapshot.snapshot_id) {
            return Err(CapabilityRegistryError::DuplicateSnapshotId);
        }
        match snapshot.lifecycle {
            OpticalCapabilityLifecycle::Revoked => {
                if snapshot.revoked_at_ms.is_none()
                    || snapshot
                        .revocation_reason
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .is_empty()
                {
                    return Err(CapabilityRegistryError::InvalidLifecycleMetadata);
                }
                return Err(CapabilityRegistryError::RevokedSnapshot);
            }
            OpticalCapabilityLifecycle::Superseded => {
                if snapshot
                    .supersedes_snapshot_id
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .is_empty()
                {
                    return Err(CapabilityRegistryError::InvalidLifecycleMetadata);
                }
                return Err(CapabilityRegistryError::SupersededSnapshot);
            }
            OpticalCapabilityLifecycle::Active => {
                if snapshot.revoked_at_ms.is_some() || snapshot.revocation_reason.is_some() {
                    return Err(CapabilityRegistryError::InvalidLifecycleMetadata);
                }
            }
        }
        snapshot.model_base = normalize_model_base(&snapshot.model_base);
        snapshot.source = snapshot.source.trim().to_string();
        if snapshot.snapshot_id.is_empty()
            || snapshot.model_base.is_empty()
            || snapshot.source.is_empty()
        {
            return Err(CapabilityRegistryError::InvalidIdentity);
        }
        if identity_contains_wildcard(&snapshot.model_base)
            || snapshot
                .provider_id
                .as_deref()
                .is_some_and(identity_contains_wildcard)
            || snapshot
                .route_id
                .as_deref()
                .is_some_and(identity_contains_wildcard)
        {
            return Err(CapabilityRegistryError::WildcardIdentity);
        }
        if snapshot.revision == 0 || snapshot.verified_at_ms == 0 {
            return Err(CapabilityRegistryError::InvalidRevision);
        }
        if snapshot.capability.status != CapabilityStatus::Supported {
            return Err(CapabilityRegistryError::CapabilityNotSupported);
        }
        if snapshot.capability.provider_family != snapshot.provider_family
            || snapshot.capability.provider_id != snapshot.provider_id
            || snapshot.capability.endpoint != snapshot.endpoint
            || snapshot.capability.model_base.as_deref() != Some(snapshot.model_base.as_str())
        {
            return Err(CapabilityRegistryError::CapabilityIdentityMismatch);
        }
        if !capability_policy_is_verified(&snapshot.capability) {
            return Err(CapabilityRegistryError::CapabilityPolicyMismatch);
        }
        self.snapshot_ids.insert(snapshot.snapshot_id.clone());
        self.verified_rules.push(snapshot);
        Ok(())
    }

    fn register_attested_snapshot(
        &mut self,
        snapshot: OpticalCapabilitySnapshot,
        authorities: &HashMap<String, &OpticalTrustedAuthority>,
        evals: &HashMap<String, &OpticalEvalArtifact>,
        now_ms: u64,
    ) -> Result<(), CapabilityRegistryError> {
        verify_snapshot_attestation(&snapshot, authorities, evals, now_ms)?;
        self.register_snapshot(snapshot)
    }

    #[cfg(test)]
    pub(crate) fn register_verified(&mut self, capability: OpticalModelCapability) {
        let model_base = capability.model_base.clone().unwrap_or_default();
        let snapshot = OpticalCapabilitySnapshot {
            schema_version: 1,
            snapshot_id: format!("inline-verified-{}", self.verified_rules.len() + 1),
            revision: 1,
            enabled: true,
            lifecycle: OpticalCapabilityLifecycle::Active,
            revoked_at_ms: None,
            revocation_reason: None,
            supersedes_snapshot_id: None,
            provider_family: capability.provider_family.clone(),
            provider_id: capability.provider_id.clone(),
            route_id: None,
            model_base,
            endpoint: capability.endpoint.clone(),
            capability,
            verified_at_ms: 1,
            source: "runtime-inline-test-rule".to_string(),
            notes: Some(
                "compatibility helper; production should load versioned snapshots".to_string(),
            ),
            attestation: None,
            economics: None,
        };
        let _ = self.register_snapshot(snapshot);
    }

    fn resolve(
        &self,
        provider_family: &ProviderFamily,
        provider_id: Option<&str>,
        route_id: Option<&str>,
        endpoint: &OpticalEndpoint,
        model_base: &str,
    ) -> Option<&OpticalCapabilitySnapshot> {
        let blocked_through = [
            capability_identity_key(provider_family, provider_id, route_id, endpoint, model_base),
            capability_identity_key(provider_family, None, route_id, endpoint, model_base),
            capability_identity_key(provider_family, provider_id, None, endpoint, model_base),
            capability_identity_key(provider_family, None, None, endpoint, model_base),
        ]
        .into_iter()
        .filter_map(|identity| self.blocked_through_revision.get(&identity).copied())
        .max()
        .unwrap_or_default();
        self.verified_rules
            .iter()
            .filter(|snapshot| {
                snapshot.enabled
                    && snapshot.lifecycle == OpticalCapabilityLifecycle::Active
                    && snapshot.revision > blocked_through
                    && snapshot.provider_family == *provider_family
                    && snapshot.endpoint == *endpoint
                    && snapshot.model_base == model_base
                    && snapshot
                        .provider_id
                        .as_deref()
                        .map(|expected| Some(expected) == provider_id)
                        .unwrap_or(true)
                    && snapshot
                        .route_id
                        .as_deref()
                        .map(|expected| Some(expected) == route_id)
                        .unwrap_or(true)
                    && snapshot.capability.status == CapabilityStatus::Supported
            })
            .max_by_key(|snapshot| snapshot.revision)
    }
}

fn capability_identity_key(
    provider_family: &ProviderFamily,
    provider_id: Option<&str>,
    route_id: Option<&str>,
    endpoint: &OpticalEndpoint,
    model_base: &str,
) -> String {
    format!(
        "{:?}|{}|{}|{:?}|{}",
        provider_family,
        provider_id.unwrap_or("*"),
        route_id.unwrap_or("*"),
        endpoint,
        normalize_model_base(model_base)
    )
}

pub(crate) fn identity_contains_wildcard(value: &str) -> bool {
    value.contains('*')
}

fn snapshot_identity_key(snapshot: &OpticalCapabilitySnapshot) -> String {
    capability_identity_key(
        &snapshot.provider_family,
        snapshot.provider_id.as_deref(),
        snapshot.route_id.as_deref(),
        &snapshot.endpoint,
        &snapshot.model_base,
    )
}

#[cfg(test)]
fn lifecycle_metadata_is_valid(snapshot: &OpticalCapabilitySnapshot) -> bool {
    if !snapshot.enabled
        || snapshot.schema_version != 1
        || snapshot.revision == 0
        || snapshot.verified_at_ms == 0
    {
        return false;
    }
    match snapshot.lifecycle {
        OpticalCapabilityLifecycle::Active => {
            snapshot.revoked_at_ms.is_none() && snapshot.revocation_reason.is_none()
        }
        OpticalCapabilityLifecycle::Revoked => {
            snapshot.revoked_at_ms.is_some()
                && snapshot
                    .revocation_reason
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|reason| !reason.is_empty())
        }
        OpticalCapabilityLifecycle::Superseded => snapshot
            .supersedes_snapshot_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|snapshot_id| !snapshot_id.is_empty()),
    }
}

fn lifecycle_can_block(snapshot: &OpticalCapabilitySnapshot) -> bool {
    snapshot.enabled
        && snapshot.schema_version == 1
        && snapshot.revision > 0
        && snapshot.verified_at_ms > 0
        && !snapshot.model_base.trim().is_empty()
        && matches!(
            snapshot.lifecycle,
            OpticalCapabilityLifecycle::Revoked | OpticalCapabilityLifecycle::Superseded
        )
}

fn capability_policy_is_verified(capability: &OpticalModelCapability) -> bool {
    let endpoint_encoding_supported = match capability.endpoint {
        OpticalEndpoint::AnthropicMessages => capability.supports_anthropic_image_block,
        OpticalEndpoint::OpenAiChatCompletions => capability.supports_openai_image_url,
        OpticalEndpoint::OpenAiResponses => capability.supports_openai_input_image,
        OpticalEndpoint::Unsupported => false,
    };
    let token_formula_is_verified = matches!(
        capability.vision_token_formula,
        VisionTokenFormula::PixelAreaDivisor {
            pixels_per_token,
            safety_margin_percent,
        } if pixels_per_token.is_finite()
            && pixels_per_token > 0.0
            && safety_margin_percent.is_finite()
            && safety_margin_percent >= 100.0
    ) || matches!(
        capability.vision_token_formula,
        VisionTokenFormula::PatchGrid {
            patch_size_px,
            max_tokens,
            safety_margin_percent,
        } if patch_size_px > 0
            && max_tokens > 0
            && safety_margin_percent.is_finite()
            && safety_margin_percent >= 100.0
    );

    capability.max_mode == OpticalMode::ToolResultsOnly
        && capability.supports_image_input
        && endpoint_encoding_supported
        && capability.supports_data_url
        && capability.max_images_per_request > 0
        && capability.max_image_width_px > 0
        && capability.max_image_height_px > 0
        && token_formula_is_verified
        && capability.exactness_class == VisionExactnessClass::TechnicalReadable
}

/// Builds an explicit experimental rule. Registering this rule is a configuration decision;
/// no model is active-capable by default.
#[cfg(test)]
pub(crate) fn experimental_anthropic_active_capability(model_base: &str) -> OpticalModelCapability {
    OpticalModelCapability {
        provider_family: ProviderFamily::Anthropic,
        provider_id: None,
        model_base: Some(normalize_model_base(model_base)),
        endpoint: OpticalEndpoint::AnthropicMessages,
        status: CapabilityStatus::Supported,
        max_mode: OpticalMode::ToolResultsOnly,
        supports_image_input: true,
        supports_anthropic_image_block: true,
        supports_openai_image_url: false,
        supports_openai_input_image: false,
        supports_data_url: true,
        max_images_per_request: 10,
        max_image_width_px: 1_568,
        max_image_height_px: 728,
        vision_token_formula: VisionTokenFormula::PatchGrid {
            patch_size_px: 28,
            max_tokens: 1_568,
            safety_margin_percent: 105.0,
        },
        exactness_class: VisionExactnessClass::TechnicalReadable,
    }
}

#[cfg(test)]
pub(crate) fn experimental_anthropic_active_snapshot(
    snapshot_id: &str,
    revision: u64,
    model_base: &str,
    verified_at_ms: u64,
    source: &str,
) -> OpticalCapabilitySnapshot {
    let capability = experimental_anthropic_active_capability(model_base);
    OpticalCapabilitySnapshot {
        schema_version: 1,
        snapshot_id: snapshot_id.to_string(),
        revision,
        enabled: true,
        lifecycle: OpticalCapabilityLifecycle::Active,
        revoked_at_ms: None,
        revocation_reason: None,
        supersedes_snapshot_id: None,
        provider_family: ProviderFamily::Anthropic,
        provider_id: None,
        route_id: None,
        model_base: normalize_model_base(model_base),
        endpoint: OpticalEndpoint::AnthropicMessages,
        capability,
        verified_at_ms,
        source: source.to_string(),
        notes: Some("experimental; requires explicit route confirmation".to_string()),
        attestation: None,
        economics: Some(fixture_economics()),
    }
}

/// Fixture economics matching the historical unit contract
/// (`test-char4-tokenizer-v1` / `test-anthropic-cache-pricing-v1`).
#[cfg(test)]
pub(crate) fn fixture_economics() -> OpticalEconomicsAttestation {
    OpticalEconomicsAttestation {
        schema_version: 1,
        tokenizer_revision: "test-char4-tokenizer-v1".to_string(),
        pricing_revision: "test-anthropic-cache-pricing-v1".to_string(),
        cache_policy_revision: "cold-warm-unknown-max-burn-v1".to_string(),
        chars_per_text_token: 4.0,
        cache_create_rate: 1.25,
        cache_read_rate: 0.1,
    }
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn experimental_anthropic_active_snapshot_for_route(
    snapshot_id: &str,
    revision: u64,
    provider_id: &str,
    route_id: &str,
    model_base: &str,
    verified_at_ms: u64,
    source: &str,
) -> OpticalCapabilitySnapshot {
    let mut snapshot = experimental_anthropic_active_snapshot(
        snapshot_id,
        revision,
        model_base,
        verified_at_ms,
        source,
    );
    let provider_id = provider_id.trim().to_string();
    snapshot.provider_id = Some(provider_id.clone());
    snapshot.route_id = Some(route_id.trim().to_string());
    snapshot.capability.provider_id = Some(provider_id);
    snapshot
}

pub fn capability_attestation_payload(snapshot: &OpticalCapabilitySnapshot) -> Vec<u8> {
    let Some(attestation) = snapshot.attestation.as_ref() else {
        return Vec::new();
    };
    serde_json::to_vec(&serde_json::json!({
        "schema_version": snapshot.schema_version,
        "snapshot_id": snapshot.snapshot_id,
        "revision": snapshot.revision,
        "provider_family": snapshot.provider_family,
        "provider_id": snapshot.provider_id,
        "route_id": snapshot.route_id,
        "model_base": snapshot.model_base,
        "endpoint": snapshot.endpoint,
        "capability": snapshot.capability,
        "verified_at_ms": snapshot.verified_at_ms,
        "source": snapshot.source,
        "economics": snapshot.economics,
        "attestation": {
            "schema_version": attestation.schema_version,
            "algorithm": attestation.algorithm,
            "authority_id": attestation.authority_id,
            "key_id": attestation.key_id,
            "issued_at_ms": attestation.issued_at_ms,
            "expires_at_ms": attestation.expires_at_ms,
            "renderer_profile_digest": attestation.renderer_profile_digest,
            "font_digest": attestation.font_digest,
            "transform_contract_digest": attestation.transform_contract_digest,
            "eval_artifact_digest": attestation.eval_artifact_digest
        }
    }))
    .unwrap_or_default()
}

pub fn eval_artifact_digest(artifact: &OpticalEvalArtifact) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(artifact).unwrap_or_default())
    )
}

/// Produces a signed, production-loadable capability snapshot for an exact
/// route/provider/model/API identity. The authority's private key signs the
/// snapshot payload, which binds the renderer/font/transform/eval digests and
/// the attested economics. `expires_at_ms` drives the expiry lifecycle; a
/// revocation or a higher superseded revision later blocks this snapshot.
pub fn sign_capability_snapshot(
    snapshot: &mut OpticalCapabilitySnapshot,
    signing_key_bytes: &[u8; 32],
    authority_id: &str,
    key_id: &str,
    issued_at_ms: u64,
    expires_at_ms: u64,
    eval_artifact_digest: &str,
) -> Result<(), CapabilityRegistryError> {
    if identity_contains_wildcard(&snapshot.model_base)
        || snapshot
            .provider_id
            .as_deref()
            .is_some_and(identity_contains_wildcard)
        || snapshot
            .route_id
            .as_deref()
            .is_some_and(identity_contains_wildcard)
    {
        return Err(CapabilityRegistryError::WildcardIdentity);
    }
    if snapshot
        .provider_id
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty)
        || snapshot
            .route_id
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
    {
        return Err(CapabilityRegistryError::InvalidIdentity);
    }
    let geometry = geometry_for_capability(&snapshot.capability)
        .ok_or(CapabilityRegistryError::RendererProfileMismatch)?;
    let render_contract = production_render_contract(&geometry);
    let renderer_profile_digest = render_contract_digest(&render_contract);
    let economics_contract = snapshot
        .economics
        .as_ref()
        .and_then(economics_contract_from_attested)
        .ok_or(CapabilityRegistryError::EconomicsContractMissing)?;
    let transform_contract_digest = OpticalTransformContractV2::current_with_economics(
        &renderer_profile_digest,
        &economics_contract.tokenizer_revision,
        &economics_contract.pricing_revision,
        &economics_contract.cache_policy_revision,
    )
    .digest();
    if expires_at_ms <= issued_at_ms {
        return Err(CapabilityRegistryError::AttestationExpired);
    }
    let attestation = dtoken_types::OpticalCapabilityAttestation {
        schema_version: 2,
        algorithm: "ed25519".to_string(),
        authority_id: authority_id.trim().to_string(),
        key_id: key_id.trim().to_string(),
        issued_at_ms,
        expires_at_ms,
        renderer_profile_digest,
        font_digest: font_digest_for(render_contract.font),
        transform_contract_digest,
        eval_artifact_digest: eval_artifact_digest.to_string(),
        signature_base64: String::new(),
    };
    snapshot.attestation = Some(attestation);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(signing_key_bytes);
    let signature =
        ed25519_dalek::Signer::sign(&signing_key, &capability_attestation_payload(snapshot));
    snapshot
        .attestation
        .as_mut()
        .expect("attestation was just set")
        .signature_base64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
    Ok(())
}

fn verify_snapshot_attestation(
    snapshot: &OpticalCapabilitySnapshot,
    authorities: &HashMap<String, &OpticalTrustedAuthority>,
    evals: &HashMap<String, &OpticalEvalArtifact>,
    now_ms: u64,
) -> Result<(), CapabilityRegistryError> {
    let attestation = snapshot
        .attestation
        .as_ref()
        .ok_or(CapabilityRegistryError::MissingAttestation)?;
    if snapshot
        .provider_id
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty)
        || snapshot
            .route_id
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
    {
        return Err(CapabilityRegistryError::InvalidIdentity);
    }
    // A wildcard-bound capability must never authorize active execution on a
    // request: exact route/provider/model identity is a release gate.
    if identity_contains_wildcard(&snapshot.model_base)
        || snapshot
            .provider_id
            .as_deref()
            .is_some_and(identity_contains_wildcard)
        || snapshot
            .route_id
            .as_deref()
            .is_some_and(identity_contains_wildcard)
    {
        return Err(CapabilityRegistryError::WildcardIdentity);
    }
    if attestation.schema_version != 2 {
        return Err(CapabilityRegistryError::UnsupportedAttestationVersion);
    }
    if attestation.algorithm != "ed25519" {
        return Err(CapabilityRegistryError::InvalidSignature);
    }
    if attestation.issued_at_ms > now_ms {
        return Err(CapabilityRegistryError::AttestationNotYetValid);
    }
    if snapshot.verified_at_ms > attestation.issued_at_ms {
        return Err(CapabilityRegistryError::AttestationNotYetValid);
    }
    if attestation.expires_at_ms <= now_ms || attestation.expires_at_ms <= attestation.issued_at_ms
    {
        return Err(CapabilityRegistryError::AttestationExpired);
    }
    let authority = authorities
        .get(&format!(
            "{}|{}",
            attestation.authority_id, attestation.key_id
        ))
        .ok_or(CapabilityRegistryError::UnknownAuthority)?;
    let key = base64::engine::general_purpose::STANDARD
        .decode(&authority.public_key_base64)
        .map_err(|_| CapabilityRegistryError::InvalidAuthorityKey)?;
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| CapabilityRegistryError::InvalidAuthorityKey)?;
    let verifying_key =
        VerifyingKey::from_bytes(&key).map_err(|_| CapabilityRegistryError::InvalidAuthorityKey)?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(&attestation.signature_base64)
        .map_err(|_| CapabilityRegistryError::InvalidSignature)?;
    let signature =
        Signature::from_slice(&signature).map_err(|_| CapabilityRegistryError::InvalidSignature)?;
    verifying_key
        .verify(&capability_attestation_payload(snapshot), &signature)
        .map_err(|_| CapabilityRegistryError::InvalidSignature)?;

    let geometry = geometry_for_capability(&snapshot.capability)
        .ok_or(CapabilityRegistryError::RendererProfileMismatch)?;
    let render_contract = production_render_contract(&geometry);
    let renderer_profile_digest = render_contract_digest(&render_contract);
    // The economics contract must be carried by the snapshot itself and bound
    // by the signature; an unattested or malformed contract can never resolve.
    let economics_contract = snapshot
        .economics
        .as_ref()
        .and_then(economics_contract_from_attested)
        .ok_or(CapabilityRegistryError::EconomicsContractMissing)?;
    let transform_contract_digest = OpticalTransformContractV2::current_with_economics(
        &renderer_profile_digest,
        &economics_contract.tokenizer_revision,
        &economics_contract.pricing_revision,
        &economics_contract.cache_policy_revision,
    )
    .digest();
    if attestation.renderer_profile_digest != renderer_profile_digest {
        return Err(CapabilityRegistryError::RendererProfileMismatch);
    }
    if attestation.font_digest != font_digest_for(render_contract.font) {
        return Err(CapabilityRegistryError::FontMismatch);
    }
    let eval = evals
        .get(&attestation.eval_artifact_digest)
        .ok_or(CapabilityRegistryError::EvalArtifactMissing)?;
    if eval.corpus_digest.trim().is_empty() {
        return Err(CapabilityRegistryError::EvalArtifactRejected);
    }
    if !eval.passes_derived_quality_gate()
        || eval.evaluated_at_ms == 0
        || eval.evaluated_at_ms > attestation.issued_at_ms
        || normalize_model_base(&eval.model_base) != normalize_model_base(&snapshot.model_base)
        || eval.endpoint != snapshot.endpoint
        || eval.renderer_profile_digest != attestation.renderer_profile_digest
        || eval.font_digest != attestation.font_digest
        || eval.transform_contract_digest != transform_contract_digest
        || attestation.transform_contract_digest != transform_contract_digest
    {
        return Err(CapabilityRegistryError::EvalArtifactRejected);
    }
    Ok(())
}

pub fn normalize_model_base(model: &str) -> String {
    let mut normalized = model.trim().to_ascii_lowercase();
    if let Some((head, _)) = normalized.split_once('[') {
        normalized = head.trim().to_string();
    }
    if let Some((head, _)) = normalized.split_once('(') {
        normalized = head.trim().to_string();
    }
    normalized
}

pub fn provider_family_for_protocol(protocol: Option<&Protocol>) -> ProviderFamily {
    match protocol {
        Some(Protocol::Anthropic) => ProviderFamily::Anthropic,
        Some(Protocol::OpenAi) => ProviderFamily::OpenAi,
        None => ProviderFamily::Other,
    }
}

pub fn resolve_model_capability(
    protocol: Option<&Protocol>,
    provider_id: Option<&str>,
    model: Option<&str>,
    endpoint: OpticalEndpoint,
) -> OpticalModelCapability {
    resolve_model_capability_record_with_registry(
        protocol,
        provider_id,
        model,
        endpoint,
        &OpticalCapabilityRegistry::default(),
    )
    .capability
}

pub fn resolve_model_capability_with_registry(
    protocol: Option<&Protocol>,
    provider_id: Option<&str>,
    model: Option<&str>,
    endpoint: OpticalEndpoint,
    registry: &OpticalCapabilityRegistry,
) -> OpticalModelCapability {
    resolve_model_capability_record_with_registry(protocol, provider_id, model, endpoint, registry)
        .capability
}

pub fn resolve_model_capability_record_with_registry(
    protocol: Option<&Protocol>,
    provider_id: Option<&str>,
    model: Option<&str>,
    endpoint: OpticalEndpoint,
    registry: &OpticalCapabilityRegistry,
) -> ResolvedOpticalCapability {
    resolve_model_capability_record_for_route_with_registry(
        protocol,
        provider_id,
        None,
        model,
        endpoint,
        registry,
    )
}

pub fn resolve_model_capability_record_for_route_with_registry(
    protocol: Option<&Protocol>,
    provider_id: Option<&str>,
    route_id: Option<&str>,
    model: Option<&str>,
    endpoint: OpticalEndpoint,
    registry: &OpticalCapabilityRegistry,
) -> ResolvedOpticalCapability {
    let provider_family = provider_family_for_protocol(protocol);
    let Some(model) = model else {
        return ResolvedOpticalCapability {
            capability: OpticalModelCapability {
                provider_family,
                provider_id: provider_id.map(ToOwned::to_owned),
                endpoint,
                ..OpticalModelCapability::default()
            },
            snapshot_id: None,
            revision: None,
            economics: None,
        };
    };

    let model_base = normalize_model_base(model);
    if let Some(snapshot) = registry.resolve(
        &provider_family,
        provider_id,
        route_id,
        &endpoint,
        &model_base,
    ) {
        let mut capability = snapshot.capability.clone();
        capability.provider_id = provider_id.map(ToOwned::to_owned);
        capability.model_base = Some(model_base);
        return ResolvedOpticalCapability {
            capability,
            snapshot_id: Some(snapshot.snapshot_id.clone()),
            revision: Some(snapshot.revision),
            economics: snapshot.economics.clone(),
        };
    }

    let mut capability = OpticalModelCapability {
        provider_family: provider_family.clone(),
        provider_id: provider_id.map(ToOwned::to_owned),
        model_base: Some(model_base.clone()),
        endpoint: endpoint.clone(),
        ..OpticalModelCapability::default()
    };

    match (provider_family, endpoint) {
        (ProviderFamily::Anthropic, OpticalEndpoint::AnthropicMessages)
            if model_base.starts_with("claude-") =>
        {
            capability.status = CapabilityStatus::Limited;
            capability.max_mode = OpticalMode::ShadowPreview;
            capability.supports_image_input = true;
            capability.supports_anthropic_image_block = true;
            capability.supports_data_url = true;
            capability.max_images_per_request = 10;
            capability.exactness_class = VisionExactnessClass::Unknown;
        }
        (ProviderFamily::Anthropic, OpticalEndpoint::AnthropicMessages) => {
            capability.status = CapabilityStatus::Unknown;
        }
        (ProviderFamily::OpenAi, OpticalEndpoint::OpenAiChatCompletions)
        | (ProviderFamily::OpenAi, OpticalEndpoint::OpenAiResponses) => {
            capability.status = CapabilityStatus::ShadowOnly;
            capability.max_mode = OpticalMode::ShadowPreview;
            capability.supports_image_input = false;
            capability.exactness_class = VisionExactnessClass::Unknown;
        }
        (_, OpticalEndpoint::Unsupported) => {
            capability.status = CapabilityStatus::Blocked;
            capability.max_mode = OpticalMode::Off;
        }
        _ => {
            capability.status = CapabilityStatus::Unknown;
        }
    }

    ResolvedOpticalCapability {
        capability,
        snapshot_id: None,
        revision: None,
        economics: None,
    }
}

/// Builds a fully signed fixture runtime config (Ed25519 attestation + passing
/// bound eval + attested economics) that the registry accepts while fresh.
#[cfg(test)]
pub(crate) fn signed_runtime_config_fixture() -> OpticalRuntimeConfig {
    signed_runtime_config()
}

#[cfg(test)]
fn signed_runtime_config() -> OpticalRuntimeConfig {
    let mut snapshot = experimental_anthropic_active_snapshot_for_route(
        "signed-route-capability-v1",
        1,
        "anthropic-main",
        "route-main",
        "claude-fable-5",
        100,
        "fixture-eval",
    );
    let geometry = geometry_for_capability(&snapshot.capability)
        .expect("experimental capability must have geometry");
    let render_contract = production_render_contract(&geometry);
    let renderer_digest = render_contract_digest(&render_contract);
    let economics = snapshot
        .economics
        .as_ref()
        .and_then(economics_contract_from_attested)
        .expect("fixture snapshot must carry attested economics");
    let transform_contract_digest = OpticalTransformContractV2::current_with_economics(
        &renderer_digest,
        &economics.tokenizer_revision,
        &economics.pricing_revision,
        &economics.cache_policy_revision,
    )
    .digest();
    let eval = OpticalEvalArtifact {
        schema_version: 2,
        artifact_id: "eval-v1".to_string(),
        model_base: "claude-fable-5".to_string(),
        endpoint: OpticalEndpoint::AnthropicMessages,
        renderer_profile_digest: renderer_digest.clone(),
        font_digest: font_digest_for(render_contract.font),
        transform_contract_digest: transform_contract_digest.clone(),
        corpus_digest: "fixture-corpus-digest".to_string(),
        evaluated_at_ms: 90,
        sample_count: 20,
        exact_failures: 0,
        unsafe_failures: 0,
        baseline_passes: 20,
        candidate_passes: 20,
        quality_delta: 0.0,
        signed_input_token_delta: 10_000,
        signed_cost_delta_microusd: 5_000,
        latency_p95_delta_ms: 100,
        passed: true,
    };
    sign_capability_snapshot(
        &mut snapshot,
        &[7u8; 32],
        "fixture-authority",
        "fixture-key-v1",
        150,
        1_000,
        &eval_artifact_digest(&eval),
    )
    .expect("fixture snapshot must sign");
    let _ = (renderer_digest, transform_contract_digest);

    OpticalRuntimeConfig {
        capability_snapshots: vec![snapshot],
        trusted_authorities: vec![OpticalTrustedAuthority {
            authority_id: "fixture-authority".to_string(),
            key_id: "fixture-key-v1".to_string(),
            public_key_base64: base64::engine::general_purpose::STANDARD.encode(
                ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
                    .verifying_key()
                    .as_bytes(),
            ),
        }],
        eval_artifacts: vec![eval],
        ..Default::default()
    }
}

/// Re-signs the fixture attestation after mutating the snapshot/eval so the
/// remaining gates are evaluated on their own merit.
#[cfg(test)]
pub(crate) fn resign_runtime_config_fixture(config: &mut OpticalRuntimeConfig) {
    let digest = eval_artifact_digest(&config.eval_artifacts[0]);
    config.capability_snapshots[0]
        .attestation
        .as_mut()
        .expect("attestation")
        .eval_artifact_digest = digest;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let signature = signing_key.sign(&capability_attestation_payload(
        &config.capability_snapshots[0],
    ));
    config.capability_snapshots[0]
        .attestation
        .as_mut()
        .expect("attestation")
        .signature_base64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn normalize_model_base_strips_variant_tags() {
        assert_eq!(
            normalize_model_base(" Claude-Fable-5 [beta] "),
            "claude-fable-5"
        );
        assert_eq!(
            normalize_model_base("claude-fable-5(test)"),
            "claude-fable-5"
        );
    }

    #[test]
    fn default_registry_never_grants_active_capability() {
        let capability = resolve_model_capability(
            Some(&Protocol::Anthropic),
            Some("anthropic-main"),
            Some("claude-fable-5"),
            OpticalEndpoint::AnthropicMessages,
        );

        assert_eq!(capability.status, CapabilityStatus::Limited);
        assert_eq!(capability.max_mode, OpticalMode::ShadowPreview);
        assert!(capability.supports_anthropic_image_block);
    }

    #[test]
    fn self_reported_pass_cannot_override_derived_quality_or_contract_version() {
        let mut degraded = signed_runtime_config();
        degraded.eval_artifacts[0].candidate_passes = 19;
        degraded.eval_artifacts[0].quality_delta = -0.05;
        degraded.eval_artifacts[0].passed = true;
        resign_runtime_config_fixture(&mut degraded);
        let registry = OpticalCapabilityRegistry::from_runtime_config(&degraded, 200);
        assert_eq!(registry.verified_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);

        let mut legacy = signed_runtime_config();
        legacy.eval_artifacts[0].schema_version = 1;
        resign_runtime_config_fixture(&mut legacy);
        let registry = OpticalCapabilityRegistry::from_runtime_config(&legacy, 200);
        assert_eq!(registry.verified_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);
    }

    #[test]
    fn signed_zero_of_twenty_quality_artifact_is_rejected() {
        let mut config = signed_runtime_config();
        config.eval_artifacts[0].baseline_passes = 0;
        config.eval_artifacts[0].candidate_passes = 0;
        config.eval_artifacts[0].quality_delta = 0.0;
        config.eval_artifacts[0].passed = true;
        resign_runtime_config_fixture(&mut config);

        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
        assert_eq!(registry.verified_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);
    }

    #[test]
    fn signed_capability_without_attested_economics_contract_is_rejected() {
        // Removing the attested economics leaves the transform contract
        // unbound, so the registry must reject the snapshot (OPTICAL-5).
        let mut config = signed_runtime_config();
        config.capability_snapshots[0].economics = None;
        resign_runtime_config_fixture(&mut config);

        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
        assert_eq!(registry.verified_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);
    }

    #[test]
    fn signed_capability_with_malformed_economics_contract_is_rejected() {
        let mut config = signed_runtime_config();
        let mut economics = config.capability_snapshots[0]
            .economics
            .clone()
            .expect("fixture economics");
        economics.cache_policy_revision.clear();
        config.capability_snapshots[0].economics = Some(economics);
        resign_runtime_config_fixture(&mut config);

        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
        assert_eq!(registry.verified_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);
    }

    #[test]
    fn signed_wildcard_identity_is_rejected_and_never_activates() {
        for field in ["provider", "route", "model"] {
            let mut config = signed_runtime_config();
            match field {
                "provider" => {
                    config.capability_snapshots[0].provider_id = Some("*".to_string());
                }
                "route" => {
                    config.capability_snapshots[0].route_id = Some("*".to_string());
                }
                _ => {
                    config.capability_snapshots[0].model_base = "*".to_string();
                }
            }
            resign_runtime_config_fixture(&mut config);
            let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
            assert_eq!(
                registry.verified_rule_count(),
                0,
                "wildcard {field} must not verify"
            );
            assert_eq!(registry.rejected_rule_count(), 1, "wildcard {field}");
        }
    }

    #[test]
    fn expired_attestation_is_rejected_after_expiry() {
        let config = signed_runtime_config();
        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
        assert_eq!(registry.verified_rule_count(), 1);

        // The fixture attestation expires at 1_000ms; after that the same
        // snapshot must fail closed (expiry lifecycle).
        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 1_001);
        assert_eq!(registry.verified_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);
    }

    #[test]
    fn not_yet_valid_attestation_is_rejected() {
        let mut config = signed_runtime_config();
        config.capability_snapshots[0]
            .attestation
            .as_mut()
            .expect("attestation")
            .issued_at_ms = 500;
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let signature = signing_key.sign(&capability_attestation_payload(
            &config.capability_snapshots[0],
        ));
        config.capability_snapshots[0]
            .attestation
            .as_mut()
            .expect("attestation")
            .signature_base64 =
            base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
        assert_eq!(registry.verified_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);
    }

    #[test]
    fn explicit_verified_registry_can_allow_tool_result_active() {
        let mut registry = OpticalCapabilityRegistry::new();
        registry.register_verified(experimental_anthropic_active_capability("claude-fable-5"));
        let capability = resolve_model_capability_with_registry(
            Some(&Protocol::Anthropic),
            Some("anthropic-main"),
            Some("Claude-Fable-5 [beta]"),
            OpticalEndpoint::AnthropicMessages,
            &registry,
        );

        assert_eq!(capability.status, CapabilityStatus::Supported);
        assert_eq!(capability.max_mode, OpticalMode::ToolResultsOnly);
        assert!(capability.supports_image_input);
        assert!(capability.supports_anthropic_image_block);
    }

    #[test]
    fn registry_prefers_highest_verified_revision() {
        let mut registry = OpticalCapabilityRegistry::new();
        registry
            .register_snapshot(experimental_anthropic_active_snapshot(
                "claude-fable-v1",
                1,
                "claude-fable-5",
                100,
                "fixture-v1",
            ))
            .expect("v1 snapshot");
        registry
            .register_snapshot(experimental_anthropic_active_snapshot(
                "claude-fable-v2",
                2,
                "claude-fable-5",
                200,
                "fixture-v2",
            ))
            .expect("v2 snapshot");

        let resolved = resolve_model_capability_record_with_registry(
            Some(&Protocol::Anthropic),
            None,
            Some("claude-fable-5"),
            OpticalEndpoint::AnthropicMessages,
            &registry,
        );
        assert_eq!(resolved.snapshot_id.as_deref(), Some("claude-fable-v2"));
        assert_eq!(resolved.revision, Some(2));
        assert_eq!(resolved.capability.status, CapabilityStatus::Supported);
    }

    #[test]
    fn invalid_snapshot_is_rejected() {
        let mut registry = OpticalCapabilityRegistry::new();
        let mut snapshot =
            experimental_anthropic_active_snapshot("invalid", 0, "claude-fable-5", 0, "fixture");
        snapshot.capability.status = CapabilityStatus::Limited;
        assert!(registry.register_snapshot(snapshot).is_err());
    }

    #[test]
    fn snapshot_identity_includes_provider_scope() {
        let mut registry = OpticalCapabilityRegistry::new();
        let mut snapshot = experimental_anthropic_active_snapshot(
            "provider-scoped",
            1,
            "claude-fable-5",
            100,
            "fixture",
        );
        snapshot.provider_id = Some("provider-a".to_string());

        assert_eq!(
            registry.register_snapshot(snapshot),
            Err(CapabilityRegistryError::CapabilityIdentityMismatch)
        );
        assert_eq!(registry.verified_rule_count(), 0);
    }

    #[test]
    fn supported_label_without_verified_render_policy_is_rejected() {
        let mut registry = OpticalCapabilityRegistry::new();
        let mut snapshot = experimental_anthropic_active_snapshot(
            "unsafe-geometry",
            1,
            "claude-fable-5",
            100,
            "fixture",
        );
        snapshot.capability.vision_token_formula = VisionTokenFormula::PixelAreaDivisor {
            pixels_per_token: 0.0,
            safety_margin_percent: 99.0,
        };

        assert_eq!(
            registry.register_snapshot(snapshot),
            Err(CapabilityRegistryError::CapabilityPolicyMismatch)
        );
        assert_eq!(registry.verified_rule_count(), 0);
    }

    #[test]
    fn from_snapshots_counts_rejected_rules_without_granting_capability() {
        let mut invalid = experimental_anthropic_active_snapshot(
            "invalid-policy",
            1,
            "claude-fable-5",
            100,
            "fixture",
        );
        invalid.capability.supports_image_input = false;
        let registry = OpticalCapabilityRegistry::from_snapshots([invalid]);

        assert_eq!(registry.verified_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);
        let resolved = resolve_model_capability_record_with_registry(
            Some(&Protocol::Anthropic),
            None,
            Some("claude-fable-5"),
            OpticalEndpoint::AnthropicMessages,
            &registry,
        );
        assert_eq!(resolved.capability.max_mode, OpticalMode::ShadowPreview);
        assert!(resolved.snapshot_id.is_none());
    }

    #[test]
    fn duplicate_snapshot_id_is_rejected() {
        let mut registry = OpticalCapabilityRegistry::new();
        let first = experimental_anthropic_active_snapshot(
            "duplicate-id",
            1,
            "claude-fable-5",
            100,
            "fixture-v1",
        );
        let second = experimental_anthropic_active_snapshot(
            "duplicate-id",
            2,
            "claude-fable-5",
            200,
            "fixture-v2",
        );
        registry.register_snapshot(first).expect("first snapshot");
        assert_eq!(
            registry.register_snapshot(second),
            Err(CapabilityRegistryError::DuplicateSnapshotId)
        );
    }

    #[test]
    fn revoked_higher_revision_blocks_older_active_snapshot() {
        let active = experimental_anthropic_active_snapshot(
            "active-v1",
            1,
            "claude-fable-5",
            100,
            "fixture-v1",
        );
        let mut revoked = experimental_anthropic_active_snapshot(
            "revoked-v2",
            2,
            "claude-fable-5",
            200,
            "fixture-v2",
        );
        revoked.lifecycle = OpticalCapabilityLifecycle::Revoked;
        revoked.revoked_at_ms = Some(250);
        revoked.revocation_reason = Some("provider behavior changed".to_string());
        let registry = OpticalCapabilityRegistry::from_snapshots([active, revoked]);

        let resolved = resolve_model_capability_record_with_registry(
            Some(&Protocol::Anthropic),
            None,
            Some("claude-fable-5"),
            OpticalEndpoint::AnthropicMessages,
            &registry,
        );
        assert_eq!(resolved.capability.status, CapabilityStatus::Limited);
        assert_eq!(resolved.snapshot_id, None);
        assert_eq!(registry.revoked_rule_count(), 1);
    }

    #[test]
    fn malformed_revocation_fails_closed_without_being_counted_as_valid_lifecycle() {
        let active = experimental_anthropic_active_snapshot(
            "active-v1",
            1,
            "claude-fable-5",
            100,
            "fixture-v1",
        );
        let mut malformed_revocation = experimental_anthropic_active_snapshot(
            "revoked-v2",
            2,
            "claude-fable-5",
            200,
            "fixture-v2",
        );
        malformed_revocation.lifecycle = OpticalCapabilityLifecycle::Revoked;
        malformed_revocation.revoked_at_ms = Some(250);
        malformed_revocation.revocation_reason = None;
        let registry = OpticalCapabilityRegistry::from_snapshots([active, malformed_revocation]);

        let resolved = resolve_model_capability_record_with_registry(
            Some(&Protocol::Anthropic),
            None,
            Some("claude-fable-5"),
            OpticalEndpoint::AnthropicMessages,
            &registry,
        );
        assert_eq!(resolved.capability.status, CapabilityStatus::Limited);
        assert_eq!(resolved.snapshot_id, None);
        assert_eq!(registry.revoked_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);
    }

    #[test]
    fn newer_active_revision_can_restore_after_revocation() {
        let active_v1 = experimental_anthropic_active_snapshot(
            "active-v1",
            1,
            "claude-fable-5",
            100,
            "fixture-v1",
        );
        let mut revoked_v2 = experimental_anthropic_active_snapshot(
            "revoked-v2",
            2,
            "claude-fable-5",
            200,
            "fixture-v2",
        );
        revoked_v2.lifecycle = OpticalCapabilityLifecycle::Revoked;
        revoked_v2.revoked_at_ms = Some(250);
        revoked_v2.revocation_reason = Some("temporary disable".to_string());
        let active_v3 = experimental_anthropic_active_snapshot(
            "active-v3",
            3,
            "claude-fable-5",
            300,
            "fixture-v3",
        );
        let registry =
            OpticalCapabilityRegistry::from_snapshots([active_v1, revoked_v2, active_v3]);

        let resolved = resolve_model_capability_record_with_registry(
            Some(&Protocol::Anthropic),
            None,
            Some("claude-fable-5"),
            OpticalEndpoint::AnthropicMessages,
            &registry,
        );
        assert_eq!(resolved.snapshot_id.as_deref(), Some("active-v3"));
        assert_eq!(resolved.revision, Some(3));
    }

    #[test]
    fn unknown_model_cannot_active_transform() {
        let capability = resolve_model_capability(
            Some(&Protocol::Anthropic),
            None,
            Some("unknown-model"),
            OpticalEndpoint::AnthropicMessages,
        );

        assert_eq!(capability.status, CapabilityStatus::Unknown);
        assert!(!capability.supports_image_input);
    }

    #[test]
    fn production_registry_requires_valid_route_bound_attestation_and_eval() {
        let valid_config = signed_runtime_config();
        let registry = OpticalCapabilityRegistry::from_runtime_config(&valid_config, 200);
        assert_eq!(registry.configured_rule_count(), 1);
        assert_eq!(registry.verified_rule_count(), 1);
        assert_eq!(registry.rejected_rule_count(), 0);

        let resolved = resolve_model_capability_record_for_route_with_registry(
            Some(&Protocol::Anthropic),
            Some("anthropic-main"),
            Some("route-main"),
            Some("claude-fable-5"),
            OpticalEndpoint::AnthropicMessages,
            &registry,
        );
        assert_eq!(resolved.revision, Some(1));
        assert_eq!(resolved.capability.status, CapabilityStatus::Supported);

        let mut unsigned_config = valid_config.clone();
        unsigned_config.capability_snapshots[0].attestation = None;
        let unsigned = OpticalCapabilityRegistry::from_runtime_config(&unsigned_config, 200);
        assert_eq!(unsigned.verified_rule_count(), 0);
        assert_eq!(unsigned.rejected_rule_count(), 1);

        let mut tampered_config = valid_config;
        tampered_config.capability_snapshots[0].route_id = Some("route-other".to_string());
        let tampered = OpticalCapabilityRegistry::from_runtime_config(&tampered_config, 200);
        assert_eq!(tampered.verified_rule_count(), 0);
        assert_eq!(tampered.rejected_rule_count(), 1);
    }
}
