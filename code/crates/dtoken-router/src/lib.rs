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
        let matches = self
            .config
            .routes
            .iter()
            .filter(|route| {
                route.instance_id == instance_id && route.input_protocol == input_protocol
            })
            .collect::<Vec<_>>();
        let route = match matches.as_slice() {
            [] => return Err(anyhow!("no matching route for instance '{instance_id}'")),
            [route] => *route,
            _ => {
                return Err(anyhow!(
                    "ambiguous routes for instance '{instance_id}'; an explicit route is required"
                ))
            }
        };

        let provider = self
            .config
            .find_provider(&route.target_provider_id)
            .ok_or_else(|| {
                anyhow!(
                    "target provider '{}' is not configured",
                    route.target_provider_id
                )
            })?;

        Ok(RouteDecision {
            request_id: Uuid::new_v4(),
            route_id: route.id.clone(),
            provider_id: provider.id.clone(),
            chain_mode: route.chain_mode.clone(),
            compression_policy: route.compression_policy.clone(),
            reason: "matched instance id and input protocol".to_string(),
            input_protocol: Some(input_protocol),
            output_protocol: Some(provider.protocol.clone()),
        })
    }
}
