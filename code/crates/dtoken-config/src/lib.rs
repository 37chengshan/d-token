use anyhow::{Context, Result};
use dtoken_types::{
    InstanceProfile, OpticalEvalArtifact, OpticalRuntimeConfig, OutputShapingRoutePolicy,
    ProviderCachePolicy, ProviderProfile, RouteProfile,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpticalConfigIssue {
    UnsupportedSchemaVersion,
    InvalidResourceBudget,
    DuplicateRoutePolicy,
    UnknownRoutePolicy,
    DuplicateSnapshotId,
    DuplicateAuthority,
    DuplicateEvalArtifact,
    ActiveWithoutTrustedAuthority,
    ActiveWithoutPassingEval,
    ActiveWithoutAttestedCapability,
    ActiveWithoutExactVerifiedRoute,
    ActiveDisabledByReleasePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DtokenConfig {
    pub providers: Vec<ProviderProfile>,
    pub routes: Vec<RouteProfile>,
    pub instances: Vec<InstanceProfile>,
    #[serde(default)]
    pub provider_cache_policies: Vec<ProviderCachePolicy>,
    #[serde(default)]
    pub output_shaping_policies: Vec<OutputShapingRoutePolicy>,
    #[serde(default)]
    pub optical: OpticalRuntimeConfig,
}

impl DtokenConfig {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let mut config: Self = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;
        config.normalize_optical_fail_closed();
        Ok(config)
    }

    pub fn find_provider(&self, id: &str) -> Option<&ProviderProfile> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn find_route(&self, id: &str) -> Option<&RouteProfile> {
        self.routes.iter().find(|r| r.id == id)
    }

    pub fn find_instance(&self, id: &str) -> Option<&InstanceProfile> {
        self.instances.iter().find(|i| i.id == id)
    }

    pub fn find_provider_cache_policy(
        &self,
        route_id: &str,
        provider_id: &str,
        model: &str,
    ) -> Option<&ProviderCachePolicy> {
        let mut matches = self.provider_cache_policies.iter().filter(|policy| {
            policy.route_id == route_id
                && policy.provider_id == provider_id
                && policy.model == model
        });
        let policy = matches.next()?;
        matches.next().is_none().then_some(policy)
    }

    /// Returns a policy only when its route/provider/model binding is exact
    /// and unambiguous. Broad or duplicate policies never authorize shaping.
    pub fn find_output_shaping_policy(
        &self,
        route_id: &str,
        provider_id: &str,
        model: &str,
    ) -> Option<&OutputShapingRoutePolicy> {
        let mut matches = self.output_shaping_policies.iter().filter(|policy| {
            policy.route_id == route_id
                && policy.provider_id == provider_id
                && policy.model == model
        });
        let policy = matches.next()?;
        matches.next().is_none().then_some(policy)
    }

    pub fn find_optical_route_policy(
        &self,
        route_id: &str,
    ) -> Option<&dtoken_types::RouteOpticalPolicy> {
        self.optical
            .route_policies
            .iter()
            .find(|entry| entry.route_id == route_id)
            .map(|entry| &entry.policy)
    }

    pub fn optical_config_issues(&self) -> Vec<OpticalConfigIssue> {
        let mut issues = Vec::new();
        let optical = &self.optical;
        if optical.schema_version != 1 {
            issues.push(OpticalConfigIssue::UnsupportedSchemaVersion);
        }
        let global = &optical.global_policy;
        if global.max_images_per_request == 0
            || global.max_candidate_chars_per_request == 0
            || global.max_plan_time_ms == 0
            || global.max_render_time_ms == 0
            || global.max_total_pixels == 0
            || global.max_png_bytes == 0
            || global.max_base64_bytes < global.max_png_bytes
            || global.max_outbound_body_bytes < global.max_base64_bytes
            || global.max_memory_bytes < global.max_total_pixels
            || global.max_concurrent_renders == 0
            || global.render_queue_timeout_ms == 0
        {
            issues.push(OpticalConfigIssue::InvalidResourceBudget);
        }

        let route_ids = self
            .routes
            .iter()
            .map(|route| route.id.as_str())
            .collect::<HashSet<_>>();
        let mut optical_route_ids = HashSet::new();
        for entry in &optical.route_policies {
            if !optical_route_ids.insert(entry.route_id.as_str()) {
                issues.push(OpticalConfigIssue::DuplicateRoutePolicy);
            }
            if !route_ids.contains(entry.route_id.as_str()) {
                issues.push(OpticalConfigIssue::UnknownRoutePolicy);
            }
        }

        let mut snapshots = HashSet::new();
        for snapshot in &optical.capability_snapshots {
            if !snapshots.insert(snapshot.snapshot_id.as_str()) {
                issues.push(OpticalConfigIssue::DuplicateSnapshotId);
            }
        }
        let mut authorities = HashSet::new();
        for authority in &optical.trusted_authorities {
            if !authorities.insert((authority.authority_id.as_str(), authority.key_id.as_str())) {
                issues.push(OpticalConfigIssue::DuplicateAuthority);
            }
        }
        let mut artifacts = HashSet::new();
        for artifact in &optical.eval_artifacts {
            if !artifacts.insert(artifact.artifact_id.as_str()) {
                issues.push(OpticalConfigIssue::DuplicateEvalArtifact);
            }
        }

        if global.allow_active {
            if optical.trusted_authorities.is_empty() {
                issues.push(OpticalConfigIssue::ActiveWithoutTrustedAuthority);
            }
            if !optical
                .eval_artifacts
                .iter()
                .any(OpticalEvalArtifact::passes_derived_quality_gate)
            {
                issues.push(OpticalConfigIssue::ActiveWithoutPassingEval);
            }
            if !optical
                .capability_snapshots
                .iter()
                .any(|snapshot| snapshot.enabled && snapshot.attestation.is_some())
            {
                issues.push(OpticalConfigIssue::ActiveWithoutAttestedCapability);
            }
            if !self.has_exact_optical_active_route() {
                issues.push(OpticalConfigIssue::ActiveWithoutExactVerifiedRoute);
            }
        }
        issues
    }

    fn has_exact_optical_active_route(&self) -> bool {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default();
        let registry = dtoken_compress::optical::OpticalCapabilityRegistry::from_runtime_config(
            &self.optical,
            now_ms,
        );
        self.routes.iter().any(|route| {
            route.input_protocol == dtoken_types::Protocol::Anthropic
                && route.fallback_provider_id.is_none()
                && self
                    .find_provider(&route.target_provider_id)
                    .is_some_and(|provider| {
                        provider.protocol == dtoken_types::Protocol::Anthropic
                            && provider.models.iter().any(|model| {
                                dtoken_compress::optical::resolve_model_capability_record_for_route_with_registry(
                                    Some(&dtoken_types::Protocol::Anthropic),
                                    Some(&provider.id),
                                    Some(&route.id),
                                    Some(model),
                                    dtoken_types::OpticalEndpoint::AnthropicMessages,
                                    &registry,
                                )
                                .revision
                                .is_some()
                            })
                    })
                && self.find_optical_route_policy(&route.id).is_some_and(|policy| {
                    policy.enabled
                        && policy.mode == dtoken_types::OpticalMode::ToolResultsOnly
                        && policy.allow_tool_result
                        && policy.confirmed_experimental_lossy
                })
        })
    }

    pub fn normalize_optical_fail_closed(&mut self) -> Vec<OpticalConfigIssue> {
        let mut issues = self.optical_config_issues();
        let active_requested = self.optical.global_policy.allow_active;
        if issues.iter().any(|issue| {
            matches!(
                issue,
                OpticalConfigIssue::UnsupportedSchemaVersion
                    | OpticalConfigIssue::InvalidResourceBudget
                    | OpticalConfigIssue::DuplicateRoutePolicy
                    | OpticalConfigIssue::DuplicateSnapshotId
                    | OpticalConfigIssue::DuplicateAuthority
                    | OpticalConfigIssue::DuplicateEvalArtifact
                    | OpticalConfigIssue::ActiveWithoutTrustedAuthority
                    | OpticalConfigIssue::ActiveWithoutPassingEval
                    | OpticalConfigIssue::ActiveWithoutAttestedCapability
                    | OpticalConfigIssue::ActiveWithoutExactVerifiedRoute
            )
        }) {
            self.optical.global_policy.allow_active = false;
        }
        // Active is authorized by the attestation chain, not by a blanket
        // release flag: a fully green config (trusted authority + passing
        // eval + attested capability + sane budgets) keeps `allow_active`, so
        // the exact supported route can go automatic active with no UI switch
        // (OPTICAL-1). Any missing/stale/unknown gate still fails closed.
        if active_requested && !self.optical.global_policy.allow_active {
            issues.push(OpticalConfigIssue::ActiveDisabledByReleasePolicy);
        }
        let known_routes = self
            .routes
            .iter()
            .map(|route| route.id.clone())
            .collect::<HashSet<_>>();
        for entry in &mut self.optical.route_policies {
            if !known_routes.contains(&entry.route_id) {
                entry.policy.enabled = false;
                entry.policy.mode = dtoken_types::OpticalMode::Off;
            }
        }
        issues
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_types::{OpticalMode, RouteOpticalPolicy, RouteOpticalPolicyEntry};

    #[test]
    fn legacy_json_without_optical_section_loads_safe_defaults() {
        let config: DtokenConfig =
            serde_json::from_str(r#"{"providers":[],"routes":[],"instances":[]}"#)
                .expect("legacy config should load");

        assert!(!config.optical.global_policy.enabled);
        assert!(!config.optical.global_policy.allow_active);
        assert_eq!(
            config.optical.global_policy.default_mode,
            OpticalMode::ShadowPreview
        );
        assert!(config.optical.route_policies.is_empty());
        assert!(config.optical.capability_snapshots.is_empty());
    }

    #[test]
    fn partial_optical_config_backfills_safe_defaults() {
        let config: DtokenConfig = serde_json::from_str(
            r#"{
                "providers": [],
                "routes": [],
                "instances": [],
                "optical": {
                    "global_policy": { "enabled": true },
                    "route_policies": [{
                        "route_id": "route-a",
                        "policy": { "enabled": true }
                    }]
                }
            }"#,
        )
        .expect("partial optical config should load with safe defaults");

        assert!(config.optical.global_policy.enabled);
        assert!(!config.optical.global_policy.allow_active);
        assert_eq!(
            config.optical.global_policy.default_mode,
            OpticalMode::ShadowPreview
        );
        assert!(config.optical.global_policy.public_summary_only);
        assert!(config.optical.capability_snapshots.is_empty());

        let route = config
            .find_optical_route_policy("route-a")
            .expect("route policy should load");
        assert!(route.enabled);
        assert_eq!(route.mode, OpticalMode::Off);
        assert!(!route.allow_tool_result);
        assert!(!route.allow_history);
        assert!(!route.confirmed_experimental_lossy);
    }

    #[test]
    fn empty_optical_object_is_backward_compatible_and_fail_closed() {
        let config: DtokenConfig =
            serde_json::from_str(r#"{"providers":[],"routes":[],"instances":[],"optical":{}}"#)
                .expect("empty optical object should load");

        assert_eq!(
            config.optical.global_policy.default_mode,
            OpticalMode::ShadowPreview
        );
        assert!(!config.optical.global_policy.enabled);
        assert!(!config.optical.global_policy.allow_active);
        assert!(config.optical.route_policies.is_empty());
        assert!(config.optical.capability_snapshots.is_empty());
        assert!(config.provider_cache_policies.is_empty());
    }

    #[test]
    fn incomplete_gates_disable_requested_optical_active() {
        // allow_active with no authority/eval/attested snapshot must fail
        // closed and report the release-policy block.
        let mut config = DtokenConfig::default();
        config.optical.global_policy.allow_active = true;

        let issues = config.normalize_optical_fail_closed();

        assert!(!config.optical.global_policy.allow_active);
        assert!(issues.contains(&OpticalConfigIssue::ActiveWithoutTrustedAuthority));
        assert!(issues.contains(&OpticalConfigIssue::ActiveWithoutPassingEval));
        assert!(issues.contains(&OpticalConfigIssue::ActiveWithoutAttestedCapability));
        assert!(issues.contains(&OpticalConfigIssue::ActiveWithoutExactVerifiedRoute));
        assert!(issues.contains(&OpticalConfigIssue::ActiveDisabledByReleasePolicy));
    }


    #[test]
    fn active_requires_one_exact_route_bound_verified_attestation_chain() {
        let mut config = fully_green_optical_config();
        config.optical.capability_snapshots[0].route_id = Some("route-other".to_string());

        let issues = config.normalize_optical_fail_closed();

        assert!(!config.optical.global_policy.allow_active);
        assert!(issues.contains(&OpticalConfigIssue::ActiveWithoutExactVerifiedRoute));
    }

    #[test]
    fn exact_verified_optical_route_keeps_active_requested() {
        let mut config = fully_green_optical_config();

        let issues = config.normalize_optical_fail_closed();

        assert!(config.optical.global_policy.allow_active);
        assert!(issues.is_empty());
    }

    fn fully_green_optical_config() -> DtokenConfig {
        let mut config = DtokenConfig::default();
        config.providers.push(dtoken_types::ProviderProfile {
            id: "anthropic-main".to_string(),
            name: "Anthropic main".to_string(),
            kind: dtoken_types::ProviderKind::Custom,
            protocol: dtoken_types::Protocol::Anthropic,
            base_url: "https://api.anthropic.com".to_string(),
            api_key_ref: Some("keychain://optical-provider".to_string()),
            models: vec!["claude-sonnet-4-5".to_string()],
            timeout_ms: 30_000,
            healthcheck_path: None,
            wire_api: dtoken_types::ProviderWireApi::OpenAiChat,
        });
        config.routes.push(dtoken_types::RouteProfile {
            id: "route-anthropic-main".to_string(),
            name: "Optical route".to_string(),
            input_protocol: dtoken_types::Protocol::Anthropic,
            instance_id: "instance-main".to_string(),
            target_provider_id: "anthropic-main".to_string(),
            fallback_provider_id: None,
            chain_mode: dtoken_types::ChainMode::Direct,
            compression_policy: dtoken_types::CompressionPolicy::Balanced,
            bind_host: "127.0.0.1".to_string(),
            local_port: 18_001,
        });
        config.optical.global_policy.enabled = true;
        config.optical.global_policy.allow_active = true;
        config.optical.route_policies.push(RouteOpticalPolicyEntry {
            route_id: "route-anthropic-main".to_string(),
            policy: RouteOpticalPolicy {
                enabled: true,
                mode: OpticalMode::ToolResultsOnly,
                allow_tool_result: true,
                confirmed_experimental_lossy: true,
                ..Default::default()
            },
        });
        config.optical.trusted_authorities.push(
            serde_json::from_str(include_str!(
                "../../../../support/fixtures/optical/demo-verification-key.json"
            ))
            .expect("tracked authority fixture"),
        );
        config.optical.eval_artifacts.push(
            serde_json::from_str(include_str!(
                "../../../../support/fixtures/optical/anthropic-eval-claude-sonnet-4-5.json"
            ))
            .expect("tracked eval fixture"),
        );
        config.optical.capability_snapshots.push(
            serde_json::from_str(include_str!(
                "../../../../support/fixtures/optical/anthropic-snapshot-claude-sonnet-4-5-signed.json"
            ))
            .expect("tracked signed snapshot fixture"),
        );
        config
    }

    #[test]
    fn provider_cache_policy_lookup_requires_exact_identity() {
        let policy = dtoken_types::ProviderCachePolicy {
            route_id: "route-a".to_string(),
            provider_id: "provider-a".to_string(),
            model: "model-a".to_string(),
            strategy: dtoken_types::ProviderCachePolicyStrategy::AnthropicExplicit,
            ttl_seconds: 300,
            min_prefix_bytes: 1024,
        };
        let mut config = DtokenConfig {
            provider_cache_policies: vec![policy.clone()],
            ..Default::default()
        };
        assert!(config
            .find_provider_cache_policy("route-a", "provider-a", "model-a")
            .is_some_and(dtoken_types::ProviderCachePolicy::is_well_formed));
        assert!(config
            .find_provider_cache_policy("route-a", "provider-a", "other-model")
            .is_none());
        config.provider_cache_policies.push(policy);
        assert!(config
            .find_provider_cache_policy("route-a", "provider-a", "model-a")
            .is_none());
    }

    #[test]
    fn output_shaping_policy_lookup_requires_exact_unambiguous_route_binding() {
        let policy = OutputShapingRoutePolicy {
            route_id: "route-a".to_string(),
            provider_id: "provider-a".to_string(),
            model: "model-a".to_string(),
            enabled: true,
            output_verbosity: dtoken_types::OutputVerbosity::Terse,
            holdout_ratio: Some(0.2),
        };
        let mut config = DtokenConfig {
            output_shaping_policies: vec![policy.clone()],
            ..Default::default()
        };
        assert!(config
            .find_output_shaping_policy("route-a", "provider-a", "model-a")
            .is_some());
        assert!(config
            .find_output_shaping_policy("route-a", "provider-a", "model-b")
            .is_none());
        config.output_shaping_policies.push(policy);
        assert!(config
            .find_output_shaping_policy("route-a", "provider-a", "model-a")
            .is_none());
    }

    #[test]
    fn route_optical_policy_lookup_is_exact_and_fail_closed() {
        let config = DtokenConfig {
            optical: OpticalRuntimeConfig {
                route_policies: vec![RouteOpticalPolicyEntry {
                    route_id: "route-a".to_string(),
                    policy: RouteOpticalPolicy {
                        enabled: true,
                        mode: OpticalMode::ShadowPreview,
                        allow_tool_result: true,
                        ..Default::default()
                    },
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(config
            .find_optical_route_policy("route-a")
            .is_some_and(|policy| policy.enabled));
        assert!(config.find_optical_route_policy("route-b").is_none());
    }
}
