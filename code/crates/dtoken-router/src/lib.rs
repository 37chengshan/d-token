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

    /// Legacy contract preserved: instance + protocol matching only.
    pub fn decide(&self, instance_id: &str, input_protocol: Protocol) -> Result<RouteDecision> {
        self.decide_with_model(instance_id, input_protocol, None, None)
    }

    /// R2.E: route decision with an optional model identity and capability
    /// hint. The model dimension never changes route selection semantics —
    /// it is carried into the decision for capability-driven compression and
    /// receipt evidence. Route/upstream matching stays fail-closed: no route
    /// → error; ambiguous routes → error (never a guess).
    pub fn decide_with_model(
        &self,
        instance_id: &str,
        input_protocol: Protocol,
        model_identity: Option<&str>,
        compression_hint: Option<&str>,
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

        let mut decision = RouteDecision {
            request_id: Uuid::new_v4(),
            route_id: route.id.clone(),
            provider_id: provider.id.clone(),
            chain_mode: route.chain_mode.clone(),
            compression_policy: route.compression_policy.clone(),
            reason: "matched instance id and input protocol".to_string(),
            input_protocol: Some(input_protocol),
            output_protocol: Some(provider.protocol.clone()),
            model_identity: None,
            compression_hint: None,
        };
        if let Some(model) = model_identity {
            decision.model_identity = Some(model.to_string());
            decision.reason = format!(
                "matched instance id and input protocol; model '{}' carried for capability resolution",
                model
            );
        }
        if let Some(hint) = compression_hint {
            decision.compression_hint = Some(hint.to_string());
        }
        Ok(decision)
    }
}
