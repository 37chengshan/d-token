use std::collections::{HashMap, HashSet};

use dtoken_types::{
    CapabilityStatus, OpticalCapabilityLifecycle, OpticalCapabilitySnapshot, OpticalEndpoint,
    OpticalMode, OpticalModelCapability, Protocol, ProviderFamily, VisionExactnessClass,
    VisionTokenFormula,
};

#[derive(Debug, Clone)]
pub struct ResolvedOpticalCapability {
    pub capability: OpticalModelCapability,
    pub snapshot_id: Option<String>,
    pub revision: Option<u64>,
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
    InvalidRevision,
    CapabilityNotSupported,
    CapabilityIdentityMismatch,
    CapabilityPolicyMismatch,
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

    pub fn from_snapshots(snapshots: impl IntoIterator<Item = OpticalCapabilitySnapshot>) -> Self {
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

    pub fn register_snapshot(
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

    pub fn register_verified(&mut self, capability: OpticalModelCapability) {
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
            model_base,
            endpoint: capability.endpoint.clone(),
            capability,
            verified_at_ms: 1,
            source: "runtime-inline-test-rule".to_string(),
            notes: Some(
                "compatibility helper; production should load versioned snapshots".to_string(),
            ),
        };
        let _ = self.register_snapshot(snapshot);
    }

    fn resolve(
        &self,
        provider_family: &ProviderFamily,
        provider_id: Option<&str>,
        endpoint: &OpticalEndpoint,
        model_base: &str,
    ) -> Option<&OpticalCapabilitySnapshot> {
        let exact_identity =
            capability_identity_key(provider_family, provider_id, endpoint, model_base);
        let wildcard_identity =
            capability_identity_key(provider_family, None, endpoint, model_base);
        let blocked_through = self
            .blocked_through_revision
            .get(&exact_identity)
            .copied()
            .unwrap_or_default()
            .max(
                self.blocked_through_revision
                    .get(&wildcard_identity)
                    .copied()
                    .unwrap_or_default(),
            );
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
                    && snapshot.capability.status == CapabilityStatus::Supported
            })
            .max_by_key(|snapshot| snapshot.revision)
    }
}

fn capability_identity_key(
    provider_family: &ProviderFamily,
    provider_id: Option<&str>,
    endpoint: &OpticalEndpoint,
    model_base: &str,
) -> String {
    format!(
        "{:?}|{}|{:?}|{}",
        provider_family,
        provider_id.unwrap_or("*"),
        endpoint,
        normalize_model_base(model_base)
    )
}

fn snapshot_identity_key(snapshot: &OpticalCapabilitySnapshot) -> String {
    capability_identity_key(
        &snapshot.provider_family,
        snapshot.provider_id.as_deref(),
        &snapshot.endpoint,
        &snapshot.model_base,
    )
}

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
pub fn experimental_anthropic_active_capability(model_base: &str) -> OpticalModelCapability {
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
        vision_token_formula: VisionTokenFormula::PixelAreaDivisor {
            pixels_per_token: 750.0,
            safety_margin_percent: 120.0,
        },
        exactness_class: VisionExactnessClass::TechnicalReadable,
    }
}

pub fn experimental_anthropic_active_snapshot(
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
        model_base: normalize_model_base(model_base),
        endpoint: OpticalEndpoint::AnthropicMessages,
        capability,
        verified_at_ms,
        source: source.to_string(),
        notes: Some("experimental; requires explicit route confirmation".to_string()),
    }
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
        };
    };

    let model_base = normalize_model_base(model);
    if let Some(snapshot) = registry.resolve(&provider_family, provider_id, &endpoint, &model_base)
    {
        let mut capability = snapshot.capability.clone();
        capability.provider_id = provider_id.map(ToOwned::to_owned);
        capability.model_base = Some(model_base);
        return ResolvedOpticalCapability {
            capability,
            snapshot_id: Some(snapshot.snapshot_id.clone()),
            revision: Some(snapshot.revision),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
