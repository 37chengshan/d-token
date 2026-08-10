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

    /// Canonical route-selection authority (T-C1A).
    ///
    /// Every surface (HTTP Chat/Responses/Anthropic, MCP, TS SDK, Proxy, Rust
    /// callers) must obtain its [`RouteDecision`] from here — there is no
    /// other production constructor. Ambiguity fails closed.
    pub fn decide(&self, instance_id: &str, input_protocol: Protocol) -> Result<RouteDecision> {
        self.decide_for_request(Uuid::new_v4(), instance_id, input_protocol)
    }

    /// Canonical route-selection authority with an explicit request identity.
    pub fn decide_for_request(
        &self,
        request_id: Uuid,
        instance_id: &str,
        input_protocol: Protocol,
    ) -> Result<RouteDecision> {
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
            request_id,
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
