#[cfg(test)]
pub(crate) use dtoken_config_patcher::agent::KNOWN_AGENT_ADAPTERS as AGENT_REGISTRY;
pub(crate) use dtoken_config_patcher::agent::{
    agent_adapter as agent_descriptor, scan_current_process_agents,
    AgentAdapterDescriptor as AgentDescriptor, AgentEvidenceKind, AgentInstallCapability,
    AgentSessionCapability,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_bridge_projects_the_backend_registry_without_a_second_catalog() {
        let mut ids = AGENT_REGISTRY
            .iter()
            .map(|agent| agent.id)
            .collect::<Vec<_>>();
        let original_len = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), original_len);
        assert!(AGENT_REGISTRY.len() >= 20);
        assert_eq!(
            agent_descriptor("opencode").unwrap().session_capability,
            AgentSessionCapability::Metadata
        );
    }
}
