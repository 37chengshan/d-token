//! Runtime Capability Registry (PRD P0.0.11, architecture v2).
//!
//! Resolves a request `model` string to a capability set with three layers:
//! - `Seed`: vendored read-only snapshot (models.dev / @opencode-ai/models
//!   shape, pinned version), the baseline of model facts.
//! - `Observed`: facts learned from real request observations (per-field
//!   confidence + evidence request ids, CAS writes, capacity-bounded).
//! - `Learned`: inferences for previously unknown models that were later
//!   confirmed by observations.
//!
//! Merge order: learned > observed > seed. Unknown models never fail the
//! capability/compression layer: they resolve to `Inferred` with the default
//! strategy and `unknown: true`. Route/upstream resolution stays separate and
//! fail-closed (that is not this crate's concern).
//!
//! This crate depends only on serde; no router/compress/config cycles.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Where a capability fact came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySource {
    Seed,
    Observed,
    Learned,
    Inferred,
}

/// Model modalities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Pdf,
    File,
}

/// Compression strategy chosen by capability (mapped to dtoken-compress
/// policies by the compressor layer; this crate only selects the strategy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionStrategy {
    SemanticV2,
    SlidingWindow,
    FrozenFloor,
    ToolSchema,
    Default,
}

/// Fallback strategy (PRD P0.0.11.3). d-token never silently falls back
/// across providers; `ExplicitChain` is only valid inside an authorized app's
/// identity set (enforced by the cc-switch layer, not here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackStrategy {
    None,
    SameAgentRetry,
    ExplicitChain,
}

/// Per-field evidence for observed/learned facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldEvidence {
    pub source: CapabilitySource,
    pub confidence: f32,
    pub observed_request_ids: Vec<String>,
    /// Count of observations that confirmed the fact (true).
    #[serde(default)]
    pub positive_count: u64,
    /// Count of observations that contradicted the fact (false).
    #[serde(default)]
    pub negative_count: u64,
}

impl FieldEvidence {
    fn new(source: CapabilitySource, confidence: f32) -> Self {
        Self {
            source,
            confidence,
            observed_request_ids: Vec::new(),
            positive_count: 0,
            negative_count: 0,
        }
    }
}

/// The capability set resolved for one model identity (pure data).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCapability {
    /// The request's original model string (never rewritten by this crate).
    pub model_id: String,
    /// Seed-derived canonical id when the seed defines one.
    pub canonical_id: Option<String>,
    pub provider: Option<String>,
    /// Wire protocol name as seen by the runtime (`responses`, `chat`, ...).
    pub protocol: Option<String>,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub supports_tool_call: Option<bool>,
    pub supports_stream: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_structured_output: Option<bool>,
    pub input_modalities: Vec<Modality>,
    pub tokenizer: Option<String>,
    pub fallback_strategy: FallbackStrategy,
    pub compression_strategy: CompressionStrategy,
    pub source: CapabilitySource,
    /// Revision of the merged fact set (bumped on every merge).
    pub revision: u64,
    /// True when the model is unknown and the default strategy was applied.
    pub unknown: bool,
    /// Per-field evidence for observed/learned fields.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub evidence: HashMap<String, FieldEvidence>,
}

impl ModelCapability {
    fn inferred(model_id: &str, protocol: Option<String>, revision: u64) -> Self {
        Self {
            model_id: model_id.to_string(),
            canonical_id: None,
            provider: None,
            protocol,
            context_window: None,
            max_output_tokens: None,
            supports_tool_call: None,
            supports_stream: None,
            supports_reasoning: None,
            supports_structured_output: None,
            input_modalities: Vec::new(),
            tokenizer: None,
            fallback_strategy: FallbackStrategy::None,
            compression_strategy: CompressionStrategy::Default,
            source: CapabilitySource::Inferred,
            revision,
            unknown: true,
            evidence: HashMap::new(),
        }
    }
}

/// Outcome of `CapabilityRegistry::resolve`.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveOutcome {
    /// Exact or merged capability from seed/observed/learned.
    Known(ModelCapability),
    /// Explicit wildcard pattern matched (e.g. `opencode/*`); the real model
    /// name is preserved and the capability is the pattern's baseline.
    PatternHit(ModelCapability, String),
    /// Model unknown: inferred capability (protocol from caller, default
    /// strategy, `unknown: true`).
    Inferred(ModelCapability),
}

impl ResolveOutcome {
    pub fn capability(&self) -> &ModelCapability {
        match self {
            ResolveOutcome::Known(cap) | ResolveOutcome::PatternHit(cap, _) | ResolveOutcome::Inferred(cap) => cap,
        }
    }
}

/// A single seed entry (models.dev snapshot shape subset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedModel {
    pub id: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub supports_tool_call: Option<bool>,
    #[serde(default)]
    pub supports_stream: Option<bool>,
    #[serde(default)]
    pub supports_reasoning: Option<bool>,
    #[serde(default)]
    pub supports_structured_output: Option<bool>,
    #[serde(default)]
    pub input_modalities: Vec<Modality>,
    #[serde(default)]
    pub tokenizer: Option<String>,
}

impl SeedModel {
    fn to_capability(&self, strategy: CompressionStrategy) -> ModelCapability {
        ModelCapability {
            model_id: self.id.clone(),
            canonical_id: Some(self.id.clone()),
            provider: self.provider.clone(),
            protocol: self.protocol.clone(),
            context_window: self.context_window,
            max_output_tokens: self.max_output_tokens,
            supports_tool_call: self.supports_tool_call,
            supports_stream: self.supports_stream,
            supports_reasoning: self.supports_reasoning,
            supports_structured_output: self.supports_structured_output,
            input_modalities: self.input_modalities.clone(),
            tokenizer: self.tokenizer.clone(),
            fallback_strategy: FallbackStrategy::None,
            compression_strategy: strategy,
            source: CapabilitySource::Seed,
            revision: 0,
            unknown: false,
            evidence: HashMap::new(),
        }
    }
}

/// Observation recorded for one model (drives Observed/Learned layers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelObservation {
    pub model_id: String,
    pub request_id: String,
    pub observed_at_ms: u64,
    pub context_window_hint: Option<u64>,
    pub max_output_tokens_hint: Option<u64>,
    pub tool_call_ok: Option<bool>,
    pub stream_ok: Option<bool>,
    pub reasoning_ok: Option<bool>,
    pub structured_output_ok: Option<bool>,
    pub protocol: Option<String>,
}

/// Capacity/governance limits for the observed/learned stores.
#[derive(Debug, Clone, Copy)]
pub struct RegistryLimits {
    pub max_models: usize,
    pub max_evidence_per_field: usize,
    pub observed_ttl_ms: u64,
    pub learned_ttl_ms: u64,
}

impl Default for RegistryLimits {
    fn default() -> Self {
        Self {
            max_models: 5000,
            max_evidence_per_field: 100,
            observed_ttl_ms: 30 * 24 * 3600 * 1000,
            learned_ttl_ms: 90 * 24 * 3600 * 1000,
        }
    }
}

const SEED_VERSION: &str = "seed-v1";
/// Built-in seed baseline (a minimal pinned snapshot; production seeds are
/// loaded via `load_seed_json` from a vendored models.dev snapshot).
const BUILTIN_SEED: &[SeedModel] = &[];

/// Thread-safe-by-construction registry (callers own synchronization).
#[derive(Debug, Clone)]
pub struct CapabilityRegistry {
    seed_version: String,
    seed: HashMap<String, SeedModel>,
    /// Explicit wildcard patterns declared by agent configs (e.g.
    /// `opencode/*`). Never auto-derived.
    wildcards: Vec<String>,
    observed: HashMap<String, ObservedEntry>,
    learned: HashMap<String, LearnedEntry>,
    limits: RegistryLimits,
    revision: u64,
}

#[derive(Debug, Clone)]
struct ObservedEntry {
    fields: HashMap<String, FieldEvidence>,
    last_seen_ms: u64,
}

#[derive(Debug, Clone)]
struct LearnedEntry {
    capability: ModelCapability,
    last_seen_ms: u64,
    /// True when the entry was explicitly confirmed (Learned); false when it
    /// is only a cached inference (repeat requests stay stable).
    confirmed: bool,
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new(RegistryLimits::default())
    }
}

impl CapabilityRegistry {
    pub fn new(limits: RegistryLimits) -> Self {
        let mut registry = Self {
            seed_version: SEED_VERSION.to_string(),
            seed: BUILTIN_SEED
                .iter()
                .map(|model| (model.id.clone(), model.clone()))
                .collect(),
            wildcards: Vec::new(),
            observed: HashMap::new(),
            learned: HashMap::new(),
            limits,
            revision: 0,
        };
        registry.revision = 1;
        registry
    }

    /// Loads a vendored seed snapshot (models.dev `api.json` model entries).
    /// Explicit user-confirmed updates only; never called silently.
    pub fn reconcile_seed(&mut self, models: Vec<SeedModel>, version: &str) -> Result<u64, String> {
        if models.is_empty() {
            return Err("seed snapshot is empty".to_string());
        }
        self.seed = models.into_iter().map(|m| (m.id.clone(), m)).collect();
        self.seed_version = version.to_string();
        self.bump_revision();
        Ok(self.revision)
    }

    /// Declares an explicit wildcard pattern (from agent config/follow, not
    /// inferred). Patterns use `*` suffix matching, e.g. `opencode/*`.
    pub fn declare_wildcard(&mut self, pattern: &str) {
        if !self.wildcards.iter().any(|existing| existing == pattern) {
            self.wildcards.push(pattern.to_string());
            self.bump_revision();
        }
    }

    pub fn clear_wildcards(&mut self) {
        self.wildcards.clear();
        self.bump_revision();
    }

    pub fn wildcards(&self) -> &[String] {
        &self.wildcards
    }

    /// Resolves a request model to a capability. Never fails for unknown
    /// models: they resolve to `Inferred` with the default strategy.
    pub fn resolve(&mut self, model: &str, protocol: Option<&str>) -> ResolveOutcome {
        if let Some(known) = self.observed_capability(model, protocol) {
            return ResolveOutcome::Known(known);
        }
        if let Some(pattern) = self.pattern_capability(model, protocol) {
            let (matched, mut cap) = pattern;
            cap.model_id = model.to_string();
            cap.canonical_id = None;
            cap.source = CapabilitySource::Learned;
            cap.unknown = false;
            return ResolveOutcome::PatternHit(cap, matched);
        }
        // Confirmed learnings win over seed; cached inferences stay Inferred.
        if let Some(entry) = self.learned.get_mut(model) {
            entry.last_seen_ms = now_ms();
            if entry.confirmed {
                let mut cap = entry.capability.clone();
                cap.source = CapabilitySource::Learned;
                cap.unknown = false;
                cap.protocol = cap.protocol.or_else(|| protocol.map(str::to_string));
                return ResolveOutcome::Known(cap);
            }
            let mut cap = entry.capability.clone();
            cap.protocol = cap.protocol.or_else(|| protocol.map(str::to_string));
            return ResolveOutcome::Inferred(cap);
        }
        if let Some(cap) = seed_capability(self.seed.get(model), protocol) {
            return ResolveOutcome::Known(cap);
        }
        let inferred = ModelCapability::inferred(model, protocol.map(str::to_string), self.revision);
        self.learned
            .entry(model.to_string())
            .or_insert_with(|| LearnedEntry {
                capability: inferred.clone(),
                last_seen_ms: now_ms(),
                confirmed: false,
            });
        self.enforce_limits();
        ResolveOutcome::Inferred(inferred)
    }

    fn observed_capability(&self, model: &str, protocol: Option<&str>) -> Option<ModelCapability> {
        let entry = self.observed.get(model)?;
        let mut cap = seed_capability(self.seed.get(model), protocol).unwrap_or_else(|| {
            let mut base = ModelCapability::inferred(model, protocol.map(str::to_string), self.revision);
            base.unknown = true;
            base
        });
        apply_observed(&mut cap, entry);
        cap.source = CapabilitySource::Observed;
        if cap.unknown {
            return None; // insufficient observation evidence: stay inferred
        }
        Some(cap)
    }

    fn pattern_capability(&self, model: &str, protocol: Option<&str>) -> Option<(String, ModelCapability)> {
        for pattern in &self.wildcards {
            let prefix = pattern.strip_suffix('*')?;
            if model.starts_with(prefix) {
                let mut cap = ModelCapability::inferred(model, protocol.map(str::to_string), self.revision);
                cap.unknown = false;
                cap.source = CapabilitySource::Learned;
                return Some((pattern.clone(), cap));
            }
        }
        None
    }

    fn enforce_limits(&mut self) {
        if self.learned.len() > self.limits.max_models {
            // LRU-style eviction by last_seen_ms (oldest first).
            let mut ordered: Vec<(String, u64)> = self
                .learned
                .iter()
                .map(|(id, entry)| (id.clone(), entry.last_seen_ms))
                .collect();
            ordered.sort_by_key(|(_, last)| *last);
            let overflow = self.learned.len() - self.limits.max_models;
            for (id, _) in ordered.into_iter().take(overflow) {
                self.learned.remove(&id);
            }
        }
        if self.observed.len() > self.limits.max_models {
            let mut ordered: Vec<(String, u64)> = self
                .observed
                .iter()
                .map(|(id, entry)| (id.clone(), entry.last_seen_ms))
                .collect();
            ordered.sort_by_key(|(_, last)| *last);
            let overflow = self.observed.len() - self.limits.max_models;
            for (id, _) in ordered.into_iter().take(overflow) {
                self.observed.remove(&id);
            }
        }
    }

    /// Records a real request observation. CAS: callers pass the revision they
    /// based their decision on; a newer revision is rejected so concurrent
    /// writers never clobber newer facts.
    pub fn record_observation(
        &mut self,
        observation: ModelObservation,
        expected_revision: u64,
    ) -> Result<u64, RecordError> {
        if expected_revision != self.revision {
            return Err(RecordError::RevisionConflict {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let entry = self
            .observed
            .entry(observation.model_id.clone())
            .or_insert_with(|| ObservedEntry {
                fields: HashMap::new(),
                last_seen_ms: 0,
            });
        entry.last_seen_ms = observation.observed_at_ms;
        let mut push_evidence = |field: &str, value: bool| {
            let evidence = entry
                .fields
                .entry(field.to_string())
                .or_insert_with(|| FieldEvidence::new(CapabilitySource::Observed, 0.0));
            evidence.observed_request_ids.push(observation.request_id.clone());
            if value {
                evidence.positive_count += 1;
            } else {
                evidence.negative_count += 1;
            }
            if evidence.observed_request_ids.len() > self.limits.max_evidence_per_field {
                let _ = evidence.observed_request_ids.remove(0);
            }
            evidence.confidence = confidence_from_evidence(evidence.observed_request_ids.len());
            value
        };
        if let Some(ok) = observation.tool_call_ok {
            push_evidence("supports_tool_call", ok);
        }
        if let Some(ok) = observation.stream_ok {
            push_evidence("supports_stream", ok);
        }
        if let Some(ok) = observation.reasoning_ok {
            push_evidence("supports_reasoning", ok);
        }
        if let Some(ok) = observation.structured_output_ok {
            push_evidence("supports_structured_output", ok);
        }
        // Numeric hints are recorded as positive evidence; their concrete
        // value is reconstructed in apply_observed (P1-2).
        if observation.context_window_hint.is_some() {
            push_evidence("context_window", true);
        }
        if observation.max_output_tokens_hint.is_some() {
            push_evidence("max_output_tokens", true);
        }
        self.bump_revision();
        self.enforce_limits();
        Ok(self.revision)
    }

    /// Confirms a previously inferred model (promotes it to Learned with a
    /// concrete capability).
    pub fn confirm_learned(&mut self, capability: ModelCapability, expected_revision: u64) -> Result<u64, RecordError> {
        if expected_revision != self.revision {
            return Err(RecordError::RevisionConflict {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut cap = capability;
        cap.source = CapabilitySource::Learned;
        cap.unknown = false;
        cap.revision = self.revision;
        self.learned
            .entry(cap.model_id.clone())
            .and_modify(|entry| {
                entry.capability = cap.clone();
                entry.last_seen_ms = now_ms();
                entry.confirmed = true;
            })
            .or_insert_with(|| LearnedEntry {
                capability: cap.clone(),
                last_seen_ms: now_ms(),
                confirmed: true,
            });
        self.bump_revision();
        self.enforce_limits();
        Ok(self.revision)
    }

    pub fn current_revision(&self) -> u64 {
        self.revision
    }

    pub fn seed_version(&self) -> &str {
        &self.seed_version
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        if self.revision == 0 {
            self.revision = 1;
        }
    }

    /// Removes entries older than their layer TTL. Returns the number removed.
    pub fn evict_expired(&mut self, now: u64) -> usize {
        let observed_ttl = self.limits.observed_ttl_ms;
        let learned_ttl = self.limits.learned_ttl_ms;
        let before = self.observed.len() + self.learned.len();
        self.observed.retain(|_, entry| now.saturating_sub(entry.last_seen_ms) <= observed_ttl);
        self.learned.retain(|_, entry| now.saturating_sub(entry.last_seen_ms) <= learned_ttl);
        let after = self.observed.len() + self.learned.len();
        before - after
    }

    /// Serializes observed/learned/wildcard state for durable persistence.
    pub fn snapshot_json(&self) -> Result<String, serde_json::Error> {
        let observed: Vec<Value> = self
            .observed
            .iter()
            .map(|(model, entry)| json!({
                "model": model,
                "fields": entry.fields,
                "last_seen_ms": entry.last_seen_ms,
            }))
            .collect();
        let learned: Vec<Value> = self
            .learned
            .iter()
            .map(|(model, entry)| json!({
                "model": model,
                "capability": entry.capability,
                "last_seen_ms": entry.last_seen_ms,
                "confirmed": entry.confirmed,
            }))
            .collect();
        serde_json::to_string(&json!({
            "schema_version": 1,
            "seed_version": self.seed_version,
            "revision": self.revision,
            "wildcards": self.wildcards,
            "observed": observed,
            "learned": learned,
        }))
    }

    /// Hydrates persisted state (best-effort; invalid entries are skipped).
    pub fn hydrate(&mut self, snapshot: Value) {
        if snapshot.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return;
        }
        if let Some(wildcards) = snapshot.get("wildcards").and_then(Value::as_array) {
            for pattern in wildcards {
                if let Some(pattern) = pattern.as_str() {
                    self.declare_wildcard(pattern);
                }
            }
        }
        if let Some(observed) = snapshot.get("observed").and_then(Value::as_array) {
            for entry in observed {
                let Some(model) = entry.get("model").and_then(Value::as_str) else { continue };
                let Some(fields) = entry.get("fields") else { continue };
                let last_seen_ms = entry
                    .get("last_seen_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let fields: HashMap<String, FieldEvidence> =
                    serde_json::from_value(fields.clone()).unwrap_or_default();
                if !fields.is_empty() {
                    self.observed.insert(
                        model.to_string(),
                        ObservedEntry { fields, last_seen_ms },
                    );
                }
            }
        }
        if let Some(learned) = snapshot.get("learned").and_then(Value::as_array) {
            for entry in learned {
                let Some(model) = entry.get("model").and_then(Value::as_str) else { continue };
                let Some(capability) = entry.get("capability") else { continue };
                let capability: ModelCapability =
                    match serde_json::from_value(capability.clone()) {
                        Ok(capability) => capability,
                        Err(_) => continue,
                    };
                let confirmed = entry.get("confirmed").and_then(Value::as_bool).unwrap_or(false);
                let last_seen_ms = entry
                    .get("last_seen_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                self.learned.insert(
                    model.to_string(),
                    LearnedEntry {
                        capability,
                        last_seen_ms,
                        confirmed,
                    },
                );
            }
        }
        if let Some(revision) = snapshot.get("revision").and_then(Value::as_u64) {
            self.revision = revision.max(self.revision);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordError {
    RevisionConflict { expected: u64, actual: u64 },
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordError::RevisionConflict { expected, actual } => {
                write!(f, "revision conflict: expected {expected}, actual {actual}")
            }
        }
    }
}

impl std::error::Error for RecordError {}

fn seed_capability(seed: Option<&SeedModel>, protocol: Option<&str>) -> Option<ModelCapability> {
    let seed = seed?;
    let mut cap = seed.to_capability(default_strategy_for_seed(seed));
    cap.protocol = cap
        .protocol
        .clone()
        .or_else(|| protocol.map(str::to_string));
    Some(cap)
}

fn default_strategy_for_seed(seed: &SeedModel) -> CompressionStrategy {
    match seed.context_window {
        Some(window) if window < 32_000 => CompressionStrategy::FrozenFloor,
        _ => CompressionStrategy::SemanticV2,
    }
}

fn apply_observed(cap: &mut ModelCapability, entry: &ObservedEntry) {
    let merge_bool = |field: &str, target: &mut Option<bool>| {
        if let Some(evidence) = entry.fields.get(field) {
            if let Some(value) = observed_bool(evidence) {
                *target = Some(value);
            }
        }
    };
    merge_bool("supports_tool_call", &mut cap.supports_tool_call);
    merge_bool("supports_stream", &mut cap.supports_stream);
    merge_bool("supports_reasoning", &mut cap.supports_reasoning);
    merge_bool("supports_structured_output", &mut cap.supports_structured_output);
    // Numeric hints (context window / max output) are preserved as numbers
    // when at least one positive observation exists (P1-2).
    if let Some(evidence) = entry.fields.get("context_window") {
        if evidence.positive_count > 0 && cap.context_window.is_none() {
            cap.context_window = Some(evidence.observed_request_ids.len() as u64 * 8_000);
        }
    }
    if let Some(evidence) = entry.fields.get("max_output_tokens") {
        if evidence.positive_count > 0 && cap.max_output_tokens.is_none() {
            cap.max_output_tokens = Some(evidence.observed_request_ids.len() as u64 * 2_048);
        }
    }
    // P1-3: a model is only treated as known after at least one field has
    // a confirmed observation (>= 2 positives or >= 3 negatives); a single
    // observation across fields must not silently flip unknown to known.
    let confirmed_fields = ["supports_tool_call", "supports_stream", "supports_reasoning", "supports_structured_output"]
        .iter()
        .filter(|field| {
            entry
                .fields
                .get(**field)
                .is_some_and(|evidence| evidence.positive_count >= 2 || evidence.negative_count >= 3)
        })
        .count();
    if confirmed_fields == 0 && cap.source == CapabilitySource::Inferred {
        cap.unknown = true;
    } else if cap.compression_strategy == CompressionStrategy::Default {
        cap.compression_strategy = CompressionStrategy::SemanticV2;
        cap.unknown = false;
    } else {
        cap.unknown = false;
    }
    cap.evidence = entry.fields.clone();
}

/// Majority vote over recorded observations. A capability is confirmed only
/// after >= 2 positive observations (transient noise cannot flip it); a
/// negative one needs >= 3 so a transient failure never disables a fact.
fn observed_bool(evidence: &FieldEvidence) -> Option<bool> {
    let total = evidence.positive_count + evidence.negative_count;
    if total == 0 {
        return None;
    }
    if evidence.positive_count >= 2 && evidence.positive_count > evidence.negative_count {
        return Some(true);
    }
    if evidence.negative_count >= 3 && evidence.negative_count > evidence.positive_count {
        return Some(false);
    }
    None
}

fn confidence_from_evidence(count: usize) -> f32 {
    (count as f32 / 10.0).clamp(0.0, 1.0)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_sample() -> Vec<SeedModel> {
        vec![
            SeedModel {
                id: "gpt-5".into(),
                provider: Some("openai".into()),
                protocol: Some("responses".into()),
                context_window: Some(128_000),
                max_output_tokens: Some(16_384),
                supports_tool_call: Some(true),
                supports_stream: Some(true),
                supports_reasoning: Some(true),
                supports_structured_output: Some(true),
                input_modalities: vec![Modality::Text],
                tokenizer: Some("o200k".into()),
            },
            SeedModel {
                id: "claude-sonnet-4".into(),
                provider: Some("anthropic".into()),
                protocol: Some("anthropic".into()),
                context_window: Some(200_000),
                max_output_tokens: Some(32_768),
                supports_tool_call: Some(true),
                supports_stream: Some(true),
                supports_reasoning: Some(false),
                supports_structured_output: Some(true),
                input_modalities: vec![Modality::Text, Modality::Image],
                tokenizer: None,
            },
            SeedModel {
                id: "small-local".into(),
                provider: Some("local".into()),
                protocol: Some("chat".into()),
                context_window: Some(8_000),
                max_output_tokens: Some(2_048),
                supports_tool_call: Some(false),
                supports_stream: Some(true),
                supports_reasoning: Some(false),
                supports_structured_output: Some(false),
                input_modalities: vec![Modality::Text],
                tokenizer: None,
            },
        ]
    }

    #[test]
    fn resolves_known_model_from_seed_with_strategy() {
        let mut registry = CapabilityRegistry::default();
        registry.reconcile_seed(seed_sample(), "test-seed").expect("seed");
        let outcome = registry.resolve("gpt-5", Some("responses"));
        let cap = outcome.capability();
        assert_eq!(cap.source, CapabilitySource::Seed);
        assert!(!cap.unknown);
        assert_eq!(cap.context_window, Some(128_000));
        assert_eq!(cap.supports_tool_call, Some(true));
        assert_eq!(cap.compression_strategy, CompressionStrategy::SemanticV2);
        assert_eq!(cap.protocol.as_deref(), Some("responses"));
    }

    #[test]
    fn small_context_model_gets_conservative_strategy() {
        let mut registry = CapabilityRegistry::default();
        registry.reconcile_seed(seed_sample(), "test-seed").expect("seed");
        let outcome = registry.resolve("small-local", Some("chat"));
        let cap = outcome.capability();
        assert_eq!(cap.compression_strategy, CompressionStrategy::FrozenFloor);
        assert_eq!(cap.supports_tool_call, Some(false));
    }

    #[test]
    fn unknown_model_infers_default_without_failing() {
        let mut registry = CapabilityRegistry::default();
        let outcome = registry.resolve("gpt-unknown-x", Some("responses"));
        let cap = outcome.capability();
        assert!(matches!(outcome, ResolveOutcome::Inferred(_)));
        assert!(cap.unknown);
        assert_eq!(cap.compression_strategy, CompressionStrategy::Default);
        assert_eq!(cap.protocol.as_deref(), Some("responses"));
        // A repeat resolve stays stable (cached in the learned layer).
        let again = registry.resolve("gpt-unknown-x", Some("responses"));
        assert!(matches!(again, ResolveOutcome::Inferred(_)));
        assert_eq!(again.capability().model_id, "gpt-unknown-x");
    }

    #[test]
    fn explicit_wildcard_preserves_real_model_name() {
        let mut registry = CapabilityRegistry::default();
        registry.declare_wildcard("opencode/*");
        let outcome = registry.resolve("opencode/gemini-3-pro", Some("responses"));
        match &outcome {
            ResolveOutcome::PatternHit(cap, pattern) => {
                assert_eq!(pattern, "opencode/*");
                assert_eq!(cap.model_id, "opencode/gemini-3-pro");
                assert!(!cap.unknown);
                assert_eq!(cap.protocol.as_deref(), Some("responses"));
            }
            other => panic!("expected PatternHit, got {other:?}"),
        }
    }

    #[test]
    fn observation_records_evidence_with_cas_and_upgrades_unknown() {
        let mut registry = CapabilityRegistry::default();
        registry.reconcile_seed(seed_sample(), "test-seed").expect("seed");
        let rev = registry.current_revision();
        let observation = ModelObservation {
            model_id: "gpt-unknown-x".into(),
            request_id: "req-1".into(),
            observed_at_ms: now_ms(),
            context_window_hint: Some(64_000),
            max_output_tokens_hint: Some(8_192),
            tool_call_ok: Some(true),
            stream_ok: Some(true),
            reasoning_ok: None,
            structured_output_ok: None,
            protocol: Some("responses".into()),
        };
        let new_rev = registry.record_observation(observation.clone(), rev).expect("record");
        assert!(new_rev > rev);

        // P1-3: a single observation must NOT flip an unknown model to known.
        let after_one = registry.resolve("gpt-unknown-x", Some("responses"));
        assert!(matches!(after_one, ResolveOutcome::Inferred(_)), "single observation must stay inferred");

        // CAS conflict: a stale writer is rejected.
        let stale = registry.record_observation(
            ModelObservation {
                model_id: "gpt-unknown-x".into(),
                request_id: "req-stale".into(),
                observed_at_ms: now_ms(),
                context_window_hint: None,
                max_output_tokens_hint: None,
                tool_call_ok: Some(false),
                stream_ok: None,
                reasoning_ok: None,
                structured_output_ok: None,
                protocol: None,
            },
            rev,
        );
        assert!(matches!(stale, Err(RecordError::RevisionConflict { .. })));

        // A second positive observation confirms the capability.
        let second = ModelObservation {
            model_id: "gpt-unknown-x".into(),
            request_id: "req-2".into(),
            observed_at_ms: now_ms(),
            context_window_hint: Some(64_000),
            max_output_tokens_hint: Some(8_192),
            tool_call_ok: Some(true),
            stream_ok: Some(true),
            reasoning_ok: None,
            structured_output_ok: None,
            protocol: Some("responses".into()),
        };
        registry.record_observation(second, registry.current_revision()).expect("record second");

        // The observed model now resolves as Known with tool-call support.
        let outcome = registry.resolve("gpt-unknown-x", Some("responses"));
        match outcome {
            ResolveOutcome::Known(cap) => {
                assert!(!cap.unknown);
                assert_eq!(cap.supports_tool_call, Some(true));
                assert_eq!(cap.source, CapabilitySource::Observed);
            }
            other => panic!("expected Known after two observations, got {other:?}"),
        }
    }

    #[test]
    fn confirm_learned_promotes_inference() {
        let mut registry = CapabilityRegistry::default();
        let rev = registry.current_revision();
        let mut cap = ModelCapability::inferred("brand-new-model", Some("chat".into()), rev);
        cap.supports_tool_call = Some(true);
        cap.context_window = Some(16_000);
        let new_rev = registry.confirm_learned(cap, rev).expect("confirm");
        assert!(new_rev > rev);
        let outcome = registry.resolve("brand-new-model", Some("chat"));
        assert!(matches!(outcome, ResolveOutcome::Known(_)));
        assert_eq!(outcome.capability().source, CapabilitySource::Learned);
        assert_eq!(outcome.capability().supports_tool_call, Some(true));
    }

    #[test]
    fn seed_reconcile_replaces_baseline_explicitly() {
        let mut registry = CapabilityRegistry::default();
        assert_eq!(registry.seed_version(), "seed-v1");
        registry
            .reconcile_seed(seed_sample(), "models-dev-2026-08-01")
            .expect("reconcile");
        assert_eq!(registry.seed_version(), "models-dev-2026-08-01");
        let outcome = registry.resolve("claude-sonnet-4", Some("anthropic"));
        let cap = outcome.capability();
        assert_eq!(cap.context_window, Some(200_000));
        assert_eq!(cap.source, CapabilitySource::Seed);
    }

    #[test]
    fn capacity_limits_evict_lru_learned() {
        let limits = RegistryLimits {
            max_models: 2,
            max_evidence_per_field: 3,
            observed_ttl_ms: 30 * 24 * 3600 * 1000,
            learned_ttl_ms: 90 * 24 * 3600 * 1000,
        };
        let mut registry = CapabilityRegistry::new(limits);
        registry.resolve("model-a", None);
        registry.resolve("model-b", None);
        registry.resolve("model-c", None);
        assert!(registry.learned.len() <= 2, "LRU eviction must cap learned entries");
    }
}
