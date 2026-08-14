//! R2.E: process-wide Runtime Capability Registry bridge for the request
//! pipeline.
//!
//! Kept out of `AppState` to avoid changing the public state shape; a single
//! process-wide registry accumulates seed/observed/learned facts for the
//! lifetime of the runtime. The request pipeline only reads capability
//! summaries (never blocking on learning) and attaches them to request logs
//! and ledger events as receipt evidence.

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use dtoken_capability_registry::{
    CapabilityRegistry, ModelObservation, RegistryLimits, ResolveOutcome,
};
use serde_json::{json, Value};

/// Optional persistence path for observed/learned facts. When set, the
/// registry is hydrated at first access and snapshotted on mutations so
/// learning survives process restarts (PRD P0.0.11.3 runtime facts).
fn registry_store_path() -> Option<PathBuf> {
    std::env::var_os("DTOKEN_CAPABILITY_REGISTRY_STORE").map(PathBuf::from)
}

fn registry() -> &'static Mutex<CapabilityRegistry> {
    static REGISTRY: LazyLock<Mutex<CapabilityRegistry>> = LazyLock::new(|| {
        let mut registry = CapabilityRegistry::new(RegistryLimits::default());
        if let Some(path) = registry_store_path() {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<Value>(&raw) {
                    registry.hydrate(json);
                }
            }
        }
        Mutex::new(registry)
    });
    &REGISTRY
}

fn persist() {
    if let Some(path) = registry_store_path() {
        if let Ok(serialized) = registry()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot_json()
        {
            let _ = std::fs::write(path, serialized);
        }
    }
}

/// Records a completed request observation (P0.0.11 learning loop).
/// Best-effort: never fails the request path; revision conflicts are ignored
/// (a newer concurrent write wins) and persistence failures are dropped.
pub fn record_observation(observation: ModelObservation) {
    let revision = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .current_revision();
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if registry.record_observation(observation, revision).is_ok() {
        drop(registry);
        persist();
    }
}

/// Confirms a previously inferred capability (promotes to Learned).
pub fn confirm_learned(capability: dtoken_capability_registry::ModelCapability) {
    let revision = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .current_revision();
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if registry.confirm_learned(capability, revision).is_ok() {
        drop(registry);
        persist();
    }
}

/// Declares an explicit wildcard pattern from agent config/follow.
pub fn declare_wildcard(pattern: &str) {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .declare_wildcard(pattern);
    persist();
}

/// Resolves a request model to a capability summary for receipt evidence.
/// Never fails: unknown models yield an `inferred` summary with the default
/// strategy (`unknown: true`), matching PRD P0.0.11.4.
pub fn resolve_model_capability_summary(model: &str, protocol: Option<&str>) -> Value {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let outcome = registry.resolve(model, protocol);
    let cap = outcome.capability();
    let pattern = match &outcome {
        ResolveOutcome::PatternHit(_, pattern) => Some(pattern.as_str()),
        _ => None,
    };
    json!({
        "model_id": cap.model_id,
        "source": match cap.source {
            dtoken_capability_registry::CapabilitySource::Seed => "seed",
            dtoken_capability_registry::CapabilitySource::Observed => "observed",
            dtoken_capability_registry::CapabilitySource::Learned => "learned",
            dtoken_capability_registry::CapabilitySource::Inferred => "inferred",
        },
        "unknown": cap.unknown,
        "wildcard_pattern": pattern,
        "compression_strategy": match cap.compression_strategy {
            dtoken_capability_registry::CompressionStrategy::SemanticV2 => "semantic_v2",
            dtoken_capability_registry::CompressionStrategy::SlidingWindow => "sliding_window",
            dtoken_capability_registry::CompressionStrategy::FrozenFloor => "frozen_floor",
            dtoken_capability_registry::CompressionStrategy::ToolSchema => "tool_schema",
            dtoken_capability_registry::CompressionStrategy::Default => "default",
        },
        "protocol": cap.protocol,
        "context_window": cap.context_window,
        "supports_tool_call": cap.supports_tool_call,
        "revision": cap.revision,
    })
}

/// Compression policy hint derived from the capability (mapped by
/// dtoken-compress). Returns None when the capability is unknown (route policy
/// governs); callers must not block on it.
pub fn compression_hint_for(model: &str, protocol: Option<&str>) -> Option<String> {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cap = registry.resolve(model, protocol).capability().clone();
    if cap.unknown {
        return None;
    }
    Some(
        match cap.compression_strategy {
            dtoken_capability_registry::CompressionStrategy::SemanticV2 => "semantic_v2",
            dtoken_capability_registry::CompressionStrategy::SlidingWindow => "sliding_window",
            dtoken_capability_registry::CompressionStrategy::FrozenFloor => "frozen_floor",
            dtoken_capability_registry::CompressionStrategy::ToolSchema => "tool_schema",
            dtoken_capability_registry::CompressionStrategy::Default => "default",
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_model_resolves_to_inferred_without_failing() {
        let summary = resolve_model_capability_summary("brand-new-model-x", Some("responses"));
        assert_eq!(summary["unknown"], true);
        assert_eq!(summary["source"], "inferred");
        assert_eq!(summary["compression_strategy"], "default");
        assert_eq!(summary["model_id"], "brand-new-model-x");
    }

    #[test]
    fn capability_hint_is_none_for_unknown_model() {
        assert_eq!(compression_hint_for("brand-new-model-x", Some("responses")), None);
    }
}
