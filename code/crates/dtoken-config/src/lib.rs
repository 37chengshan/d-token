use anyhow::{Context, Result};
use dtoken_types::{InstanceProfile, OpticalRuntimeConfig, ProviderProfile, RouteProfile};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DtokenConfig {
    pub providers: Vec<ProviderProfile>,
    pub routes: Vec<RouteProfile>,
    pub instances: Vec<InstanceProfile>,
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
        let config = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;
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
