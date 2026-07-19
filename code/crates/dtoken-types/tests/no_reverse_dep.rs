use dtoken_types::agent::{AgentDiscoverySnapshot, DetectedAgent};

#[test]
fn agent_discovery_types_are_owned_by_dtoken_types() {
    let snapshot = AgentDiscoverySnapshot {
        observed_at_ms: 0,
        evidence_revision: "test".to_string(),
        agents: Vec::<DetectedAgent>::new(),
    };

    assert!(snapshot.agents.is_empty());
}
