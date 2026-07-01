use anyhow::{anyhow, Result};
use dtoken_config::DtokenConfig;
use dtoken_types::{Protocol, RouteDecision};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Router {
    config: DtokenConfig,
}

impl Router {
    pub fn new(config: DtokenConfig) -> Self {
        Self { config }
    }

    pub fn decide(&self, instance_id: &str, input_protocol: Protocol) -> Result<RouteDecision> {
        let route = self
            .config
            .routes
            .iter()
            .find(|route| route.instance_id == instance_id && route.input_protocol == input_protocol)
            .ok_or_else(|| anyhow!("no matching route for instance '{instance_id}'"))?;

        let provider = self
            .config
            .find_provider(&route.target_provider_id)
            .ok_or_else(|| anyhow!("target provider '{}' is not configured", route.target_provider_id))?;

        Ok(RouteDecision {
            request_id: Uuid::new_v4(),
            route_id: route.id.clone(),
            provider_id: provider.id.clone(),
            chain_mode: route.chain_mode.clone(),
            compression_policy: route.compression_policy.clone(),
            reason: "matched instance id and input protocol".to_string(),
        })
    }
}
