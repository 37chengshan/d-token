use serde_json::{json, Value};
use std::io;

#[allow(dead_code, unused_imports)]
#[path = "../src/agent_registry.rs"]
mod agent_registry;
#[allow(dead_code, unused_imports)]
#[path = "../src/upstream_discovery.rs"]
mod upstream_discovery;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let discovery = upstream_discovery::discover();
    let cc_switch = discovery
        .pointer("/external_config_snapshot/cc_switch")
        .cloned()
        .ok_or_else(|| io::Error::other("d-token discovery did not return a cc-switch snapshot"))?;
    let source_state = cc_switch
        .get("source_state")
        .and_then(Value::as_str)
        .unwrap_or("metadata_unavailable");
    if source_state != "loaded" {
        return Err(io::Error::other(format!(
            "cc-switch snapshot is not safely loaded: {source_state}"
        ))
        .into());
    }

    let bindings = discovery
        .get("agent_bindings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(redacted_cc_switch_binding)
        .collect::<Vec<_>>();

    println!(
        "{}",
        json!({
            "source": "dtoken-desktop-read-only-discovery",
            "revision": discovery.get("revision"),
            "cc_switch": cc_switch,
            "cc_switch_sync": discovery.get("cc_switch_sync"),
            "agent_bindings": bindings,
            "writes_performed": false,
            "sqlite_rows_returned": false,
            "credential_values_read": false,
            "credential_values_returned": false,
        })
    );
    Ok(())
}

fn redacted_cc_switch_binding(binding: &Value) -> Option<Value> {
    let candidates = binding
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|candidate| {
            candidate
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|source| source.starts_with("cc-switch:"))
        })
        .map(redacted_candidate)
        .collect::<Vec<_>>();
    let resolved = binding
        .get("resolved_candidate")
        .filter(|candidate| {
            candidate
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|source| source.starts_with("cc-switch:"))
        })
        .map(redacted_candidate);
    if candidates.is_empty() && resolved.is_none() {
        return None;
    }
    Some(json!({
        "agent_id": binding.get("agent_id"),
        "installed": binding.get("installed"),
        "resolution": binding.get("resolution"),
        "confidence": binding.get("confidence"),
        "capability": binding.get("capability"),
        "can_configure": binding.get("can_configure"),
        "can_clear": binding.get("can_clear"),
        "transport_supported": binding.get("transport_supported"),
        "transport_status": binding.get("transport_status"),
        "already_connected": binding.get("already_connected"),
        "candidates": candidates,
        "resolved_candidate": resolved,
    }))
}

fn redacted_candidate(candidate: &Value) -> Value {
    json!({
        "id": candidate.get("id"),
        "protocol": candidate.get("protocol"),
        "base_url": candidate.get("base_url"),
        "models": candidate.get("models"),
        "source": candidate.get("source"),
        "auth_mode": candidate.get("auth_mode"),
        "active": candidate.get("active"),
        "port": candidate.get("port"),
        "state": candidate.get("state"),
        "agent_ids": candidate.get("agent_ids"),
        "confidence": candidate.get("confidence"),
    })
}
