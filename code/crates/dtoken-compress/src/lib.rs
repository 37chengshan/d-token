pub mod code_codec;
pub mod codec_evidence;
pub mod compressor_registry;
pub mod content_router;
pub mod context_pipeline;
pub mod diff_codec;
pub mod hybrid;
pub mod optical;
pub mod output_shaper;
pub mod text_codec;
pub mod tool_schema;

use anyhow::Result;
use dtoken_types::{CompressionPolicy, CompressionResult};
use serde_json::Value;
use std::time::Instant;

pub use hybrid::{
    lower_hybrid_context, ApprovedHybridAction, ApprovedHybridTarget, ApprovedHybridTargetKind,
    HybridContentProvenance, HybridContextIr, HybridContextNode, HybridNodeKind, HybridProtection,
    HybridProtectionReason, HybridRole, ResolvedOptimizationContext,
};

#[derive(Debug, Clone, Default)]
pub struct CompressionEngine;

impl CompressionEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn compress(&self, _policy: CompressionPolicy, body: Value) -> Result<CompressionResult> {
        let started = Instant::now();
        let original_tokens = estimate_tokens(&body);
        Ok(CompressionResult {
            original_tokens,
            compressed_tokens: original_tokens,
            tokens_saved: 0,
            elapsed_us: started.elapsed().as_micros().max(1),
            transforms_applied: Vec::new(),
            body,
            context_accounting: None,
        })
    }

    pub fn compress_resolved(
        &self,
        context: &ResolvedOptimizationContext,
        policy: CompressionPolicy,
        body: Value,
    ) -> Result<CompressionResult> {
        self.compress_resolved_with_cache_evidence(context, policy, body, None, None)
    }

    pub fn compress_resolved_with_cache_evidence(
        &self,
        context: &ResolvedOptimizationContext,
        policy: CompressionPolicy,
        body: Value,
        cache_evidence: Option<&context_pipeline::ProviderCacheEvidence>,
        frozen_message_count: Option<usize>,
    ) -> Result<CompressionResult> {
        let started = Instant::now();
        let original_tokens = estimate_tokens(&body);
        if policy == CompressionPolicy::Off || !context.supports_safe_text_codec() {
            return Ok(CompressionResult {
                original_tokens,
                compressed_tokens: original_tokens,
                tokens_saved: 0,
                elapsed_us: started.elapsed().as_micros().max(1),
                transforms_applied: Vec::new(),
                body,
                context_accounting: None,
            });
        }

        // PR-E1/PR-E2 tool-schema byte stabilization (see [`crate::tool_schema`]).
        // E1 sorts `tools[]`/`functions[]` by name so identical logical tool
        // sets serialize to identical bytes regardless of client collection
        // order; it is skipped when any tool carries a customer `cache_control`
        // marker (position-sensitive).  E2 canonicalizes schema key order.
        // PR-E3 automatic `cache_control` placement is intentionally NOT
        // performed here: it changes customer cache intent and the d-token
        // proxy layer gates it on fresh, exact provider cache evidence in
        // dtoken-core-api.  The normalization is idempotent, reversible, and
        // never touches tool descriptions, parameter values, or any string
        // content.
        let mut body = body;
        let tool_schema_outcome = crate::tool_schema::normalize_tool_definitions(&mut body);
        let mut transforms_applied = Vec::new();
        if tool_schema_outcome.changed {
            push_transform(&mut transforms_applied, "tool_schema:normalize");
            push_transform(&mut transforms_applied, "tool_def_normalize_v1");
        }
        // The tool-schema pass reorders bytes, so token accounting measures
        // from the normalized baseline the pipeline actually transforms.
        let original_tokens = estimate_tokens(&body);
        let original_body = body.clone();
        let (compressed_body, pipeline_transforms) =
            match context_pipeline::prepare_context_transform_with_cache_evidence(
                context,
                policy,
                body,
                cache_evidence,
                frozen_message_count,
            ) {
                Ok(Some(prepared)) => match prepared.validate() {
                    Ok(validated) => {
                        let (body, manifest) = validated.into_parts();
                        (body, manifest.transforms().to_vec())
                    }
                    Err(_) => (original_body.clone(), Vec::new()),
                },
                Ok(None) | Err(_) => (original_body.clone(), Vec::new()),
            };
        for transform in pipeline_transforms {
            push_transform(&mut transforms_applied, &transform);
        }
        let compressed_tokens = estimate_tokens(&compressed_body);
        let tokens_saved = original_tokens.saturating_sub(compressed_tokens);
        let context_accounting = (!transforms_applied.is_empty()).then(|| {
            semantic_context_accounting(
                original_tokens,
                compressed_tokens,
                &original_body,
                &compressed_body,
            )
        });

        Ok(CompressionResult {
            original_tokens,
            compressed_tokens,
            tokens_saved,
            elapsed_us: started.elapsed().as_micros().max(1),
            transforms_applied,
            body: compressed_body,
            context_accounting,
        })
    }

    /// Route a single text payload through the IR-safe mixed-content
    /// section router.  Standalone and evidence-bearing: returns the
    /// candidate, the applied transforms, and the per-section outcome.
    /// Never throws — negative-benefit, secret, marker, or low-confidence
    /// content passes through verbatim.
    pub fn route_mixed(&self, policy: CompressionPolicy, text: &str) -> MixedTextResult {
        let mut transforms = Vec::new();
        let (candidate, outcome, records) =
            crate::content_router::route_mixed_content(policy, text, &mut transforms);
        MixedTextResult {
            candidate,
            transforms,
            outcome,
            records,
        }
    }

    /// Normalize tool definitions for byte-stable prompt-cache keys
    /// (upstream Phase E, PR-E1/PR-E2 subset).  Standalone, idempotent,
    /// reversible; reports exactly what moved.  PR-E3 (auto cache_control
    /// placement) is intentionally absent — it needs proxy-layer auth-mode
    /// gating.
    pub fn normalize_tool_definitions(&self, mut body: Value) -> ToolSchemaResult {
        let mut transforms = Vec::new();
        let outcome = crate::tool_schema::normalize_tool_definitions(&mut body);
        if outcome.changed {
            crate::push_transform(&mut transforms, "tool_schema:normalize");
            crate::push_transform(&mut transforms, "tool_def_normalize_v1");
        }
        ToolSchemaResult {
            body,
            transforms,
            outcome,
        }
    }
}

/// Evidence-bearing result of [`CompressionEngine::route_mixed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixedTextResult {
    pub candidate: String,
    pub transforms: Vec<String>,
    pub outcome: crate::content_router::MixedRouteOutcome,
    pub records: Vec<crate::content_router::SectionRouteRecord>,
}

/// Evidence-bearing result of [`CompressionEngine::normalize_tool_definitions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSchemaResult {
    pub body: Value,
    pub transforms: Vec<String>,
    pub outcome: crate::tool_schema::ToolSchemaNormalizeOutcome,
}

fn semantic_context_accounting(
    original_tokens: usize,
    compressed_tokens: usize,
    original_body: &Value,
    compressed_body: &Value,
) -> dtoken_types::HybridContextAccounting {
    let measurement = |tokens: usize, body: &Value| dtoken_types::UsageMeasurement {
        evidence: dtoken_types::UsageEvidence {
            kind: dtoken_types::UsageEvidenceKind::LocalEstimate,
            source_revision: "semantic-safe-codec-v1".to_string(),
        },
        usage: dtoken_types::UsageVector {
            input: dtoken_types::InputTokenUsage {
                normalized_total: Some(u64::try_from(tokens).unwrap_or(u64::MAX)),
                modality: dtoken_types::ModalityTokens {
                    text: Some(u64::try_from(tokens).unwrap_or(u64::MAX)),
                    ..Default::default()
                },
                cache: dtoken_types::CacheTokens {
                    uncached: Some(u64::try_from(tokens).unwrap_or(u64::MAX)),
                    ..Default::default()
                },
            },
            output_tokens: None,
            bytes: dtoken_types::PayloadByteUsage {
                request_json: Some(
                    u64::try_from(serde_json::to_vec(body).unwrap_or_default().len())
                        .unwrap_or(u64::MAX),
                ),
                wire_body: Some(
                    u64::try_from(serde_json::to_vec(body).unwrap_or_default().len())
                        .unwrap_or(u64::MAX),
                ),
                ..Default::default()
            },
            cost_microusd: None,
        },
    };
    let estimated = dtoken_types::UsageComparison::from_measurements(
        measurement(original_tokens, original_body),
        measurement(compressed_tokens, compressed_body),
    );
    dtoken_types::HybridContextAccounting {
        schema_version: 1,
        disposition: dtoken_types::OptimizationDisposition::SemanticActive,
        sent_payload: dtoken_types::SentPayloadVariant::Optimized,
        upstream_state: dtoken_types::UpstreamSendState::NotAttempted,
        estimated: Some(estimated),
        provider_counterfactual_baseline: None,
        provider_actual_sent: dtoken_types::ProviderActualUsage::default(),
        provider_reconciled_input_savings: None,
        paired_actual: None,
        attribution_basis: dtoken_types::SavingsAttribution::None,
        fallback_reason_code: None,
    }
}

fn compress_text(policy: CompressionPolicy, text: &str, transforms: &mut Vec<String>) -> String {
    let mut lines = Vec::new();
    let mut last_line: Option<String> = None;
    let mut repeat_count = 0usize;
    let mut removed_blank = false;
    let mut normalized_log_prefix = false;

    for line in text.lines() {
        let mut current = line.trim().to_string();
        if current.is_empty() {
            removed_blank = true;
            continue;
        }
        if matches!(
            policy,
            CompressionPolicy::Balanced | CompressionPolicy::Aggressive | CompressionPolicy::Max
        ) {
            let normalized = normalize_log_prefix(&current);
            if normalized != current {
                normalized_log_prefix = true;
                current = normalized;
            }
        }

        if last_line.as_deref() == Some(current.as_str()) {
            repeat_count += 1;
            continue;
        }
        flush_repeated_line(&mut lines, &mut last_line, &mut repeat_count);
        last_line = Some(current);
    }
    flush_repeated_line(&mut lines, &mut last_line, &mut repeat_count);

    if removed_blank {
        push_transform(transforms, "blank_line_trim");
    }
    if lines.iter().any(|line| line.contains("[repeated x")) {
        push_transform(transforms, "repeat_line_fold");
    }
    if normalized_log_prefix {
        push_transform(transforms, "log_prefix_normalize");
    }

    let mut compressed = lines.join("\n");
    if matches!(
        policy,
        CompressionPolicy::Aggressive | CompressionPolicy::Max
    ) {
        let next = collapse_long_text(&compressed);
        if next != compressed {
            push_transform(transforms, "long_text_summarize");
            compressed = next;
        }
    }
    compressed
}

fn flush_repeated_line(
    lines: &mut Vec<String>,
    last_line: &mut Option<String>,
    repeat_count: &mut usize,
) {
    if let Some(line) = last_line.take() {
        if *repeat_count > 0 {
            lines.push(format!("{line} [repeated x{}]", *repeat_count + 1));
        } else {
            lines.push(line);
        }
    }
    if *repeat_count > 0 {
        *repeat_count = 0;
    }
}

fn normalize_log_prefix(line: &str) -> String {
    let bytes = line.as_bytes();
    if bytes.len() >= 20
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
    {
        return format!("<ts> {}", line[20..].trim_start());
    }
    line.to_string()
}

fn collapse_long_text(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= 160 {
        return text.to_string();
    }
    let head = words.iter().take(80).copied().collect::<Vec<_>>().join(" ");
    let tail = words
        .iter()
        .rev()
        .take(40)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{head}\n[... d-token summarized {} middle words ...]\n{tail}",
        words.len().saturating_sub(120)
    )
}

fn push_transform(transforms: &mut Vec<String>, name: &str) {
    if !transforms.iter().any(|item| item == name) {
        transforms.push(name.to_string());
    }
}

fn estimate_tokens(value: &Value) -> usize {
    estimate_text_tokens(&serde_json::to_string(value).unwrap_or_default())
}

fn estimate_text_tokens(text: &str) -> usize {
    if text.trim().is_empty() {
        0
    } else {
        text.split_whitespace()
            .count()
            .max(text.chars().count().div_ceil(4))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn followed_by_six_recent_closed_pairs(mut messages: Vec<Value>) -> Vec<Value> {
        for index in 0..6 {
            messages.push(json!({
                "role":"assistant",
                "content":[{"type":"tool_use","id":format!("recent-{index}"),"name":"read","input":{}}]
            }));
            messages.push(json!({
                "role":"user",
                "content":[{"type":"tool_result","tool_use_id":format!("recent-{index}"),"content":"recent"}]
            }));
        }
        messages.push(json!({"role":"user","content":[{"type":"text","text":"current"}]}));
        messages
    }

    #[test]
    fn protocol_unknown_max_policy_fails_safe() {
        let body = json!({ "large": "fixture payload" });
        let result = CompressionEngine::new()
            .compress(CompressionPolicy::Max, body.clone())
            .expect("max compression policy should be accepted");

        assert_eq!(result.body, body);
        assert_eq!(result.tokens_saved, 0);
        assert!(result.transforms_applied.is_empty());
    }

    #[test]
    fn resolved_anthropic_policy_reduces_only_historical_closed_logs() {
        let repeated_log = ["INFO build started"; 12].join("\n");
        let messages = followed_by_six_recent_closed_pairs(vec![
            json!({"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"build","input":{}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":repeated_log}]}),
        ]);
        let body = json!({
            "messages": messages
        });
        let context = ResolvedOptimizationContext {
            route_id: "r".into(),
            provider_id: "p".into(),
            model: "m".into(),
            input_protocol: dtoken_types::Protocol::Anthropic,
            output_protocol: dtoken_types::Protocol::Anthropic,
            endpoint: dtoken_types::OpticalEndpoint::AnthropicMessages,
        };

        let result = CompressionEngine::new()
            .compress_resolved(&context, CompressionPolicy::Balanced, body)
            .expect("balanced compression should succeed");

        assert!(result.tokens_saved > 0);
        assert!(result.compressed_tokens < result.original_tokens);
        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "repeat_line_fold"));
        assert!(result.elapsed_us > 0);
        assert!(result.body.to_string().contains("repeated x12"));
    }

    #[test]
    fn resolved_pipeline_routes_historical_json_to_lossless_minifier() {
        let json_output = "{\n  \"items\": [\n    {\"name\": \"alpha\", \"ok\": true},\n    {\"name\": \"beta\", \"ok\": false}\n  ]\n}";
        let messages = followed_by_six_recent_closed_pairs(vec![
            json!({"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"query","input":{}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":json_output}]}),
        ]);
        let body = json!({
            "messages": messages
        });
        let context = ResolvedOptimizationContext {
            route_id: "r".into(),
            provider_id: "p".into(),
            model: "m".into(),
            input_protocol: dtoken_types::Protocol::Anthropic,
            output_protocol: dtoken_types::Protocol::Anthropic,
            endpoint: dtoken_types::OpticalEndpoint::AnthropicMessages,
        };

        let result = CompressionEngine::new()
            .compress_resolved(&context, CompressionPolicy::Balanced, body)
            .expect("resolved JSON compression should succeed");

        let compressed = result
            .body
            .pointer("/messages/1/content/0/content")
            .and_then(Value::as_str)
            .expect("compressed JSON string");
        assert_eq!(
            serde_json::from_str::<Value>(compressed).unwrap(),
            serde_json::from_str::<Value>(json_output).unwrap()
        );
        assert!(compressed.len() < json_output.len());
        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "content_router:json"));
        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "json_minify_v1"));
        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "semantic_closed_tool_result"));
    }

    #[test]
    fn off_policy_preserves_payload_and_reports_zero_savings() {
        let body = json!({
            "messages": [{
                "role": "user",
                "content": "do not compress this"
            }]
        });

        let result = CompressionEngine::new()
            .compress(CompressionPolicy::Off, body.clone())
            .expect("off compression should succeed");

        assert_eq!(result.tokens_saved, 0);
        assert_eq!(result.original_tokens, result.compressed_tokens);
        assert_eq!(result.transforms_applied, Vec::<String>::new());
        assert_eq!(result.body, body);
        assert!(result.elapsed_us > 0);
    }

    #[test]
    fn route_mixed_returns_evidence_and_passthrough_on_negative_benefit() {
        let engine = CompressionEngine::new();
        let mixed = [
            "```rust",
            "",
            "fn main() {}",
            "",
            "```",
            "{\"items\": [1, 2, 3]}",
            "{\"items\": [1, 2, 3]}",
        ]
        .join("\n");
        let result = engine.route_mixed(CompressionPolicy::Balanced, &mixed);
        assert!(result.outcome.applied);
        assert!(result.outcome.sections_changed >= 1);
        assert!(result.candidate.len() < mixed.len());
        assert!(result
            .transforms
            .iter()
            .any(|item| item == "content_router:mixed"));
        assert!(result.records.iter().any(|record| record.changed));

        // Negative-benefit plain text passes through with no transforms.
        let plain = "one line only";
        let passthrough = engine.route_mixed(CompressionPolicy::Balanced, plain);
        assert!(!passthrough.outcome.applied);
        assert_eq!(passthrough.candidate, plain);
        assert!(passthrough.transforms.is_empty());
    }

    #[test]
    fn normalize_tool_definitions_reports_evidence_and_is_idempotent() {
        let engine = CompressionEngine::new();
        let body = json!({
            "tools": [
                {"type": "function", "function": {"name": "b", "parameters": {"type": "object"}}},
                {"type": "function", "function": {"name": "a", "parameters": {"type": "object"}}}
            ]
        });
        let first = engine.normalize_tool_definitions(body);
        assert!(first.outcome.changed);
        assert!(first.outcome.tools_reordered);
        assert_eq!(
            first.body["tools"][0]["function"]["name"].as_str().unwrap(),
            "a"
        );
        assert!(first
            .transforms
            .iter()
            .any(|item| item == "tool_schema:normalize"));
        assert!(first
            .transforms
            .iter()
            .any(|item| item == "tool_def_normalize_v1"));

        let second = engine.normalize_tool_definitions(first.body);
        assert!(!second.outcome.changed);
        assert!(second.transforms.is_empty());
    }

    #[test]
    fn compress_resolved_normalizes_tool_schemas_before_transforming() {
        let repeated_log = ["INFO build started"; 12].join("\n");
        let messages = followed_by_six_recent_closed_pairs(vec![
            json!({"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"build","input":{}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":repeated_log}]}),
        ]);
        let body = json!({
            "model": "m",
            "system": "system remains exact",
            "tools": [
                {"name": "zeta", "input_schema": {"type": "object"}},
                {"name": "alpha", "input_schema": {"type": "object"}}
            ],
            "messages": messages
        });
        let context = ResolvedOptimizationContext {
            route_id: "r".into(),
            provider_id: "p".into(),
            model: "m".into(),
            input_protocol: dtoken_types::Protocol::Anthropic,
            output_protocol: dtoken_types::Protocol::Anthropic,
            endpoint: dtoken_types::OpticalEndpoint::AnthropicMessages,
        };

        let result = CompressionEngine::new()
            .compress_resolved(&context, CompressionPolicy::Balanced, body)
            .expect("resolved compression");

        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "tool_schema:normalize"));
        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "tool_def_normalize_v1"));
        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "repeat_line_fold"));
        // E1 sorted the tools array by name and the tool section is exact.
        let tools = result.body["tools"].as_array().expect("tools array");
        assert_eq!(tools[0]["name"].as_str().unwrap(), "alpha");
        assert_eq!(tools[1]["name"].as_str().unwrap(), "zeta");
        assert_eq!(
            result.body["system"].as_str().unwrap(),
            "system remains exact"
        );
    }

    #[test]
    fn compress_resolved_preserves_customer_cache_control_marker_order() {
        let repeated_log = ["INFO build started"; 12].join("\n");
        let messages = followed_by_six_recent_closed_pairs(vec![
            json!({"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"build","input":{}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":repeated_log}]}),
        ]);
        let body = json!({
            "model": "m",
            "system": "system remains exact",
            "tools": [
                {"name": "zeta", "cache_control": {"type": "ephemeral"}, "input_schema": {"type": "object"}},
                {"name": "alpha", "input_schema": {"type": "object"}}
            ],
            "messages": messages
        });
        let context = ResolvedOptimizationContext {
            route_id: "r".into(),
            provider_id: "p".into(),
            model: "m".into(),
            input_protocol: dtoken_types::Protocol::Anthropic,
            output_protocol: dtoken_types::Protocol::Anthropic,
            endpoint: dtoken_types::OpticalEndpoint::AnthropicMessages,
        };

        let result = CompressionEngine::new()
            .compress_resolved(&context, CompressionPolicy::Balanced, body)
            .expect("resolved compression");

        // The customer's cache_control marker is position-sensitive: E1 must
        // not reorder, so no tool-schema transform is recorded.
        assert!(!result
            .transforms_applied
            .iter()
            .any(|item| item == "tool_schema:normalize"));
        let tools = result.body["tools"].as_array().expect("tools array");
        assert_eq!(tools[0]["name"].as_str().unwrap(), "zeta");
        assert_eq!(tools[1]["name"].as_str().unwrap(), "alpha");
        assert_eq!(
            tools[0]["cache_control"]["type"].as_str().unwrap(),
            "ephemeral"
        );
    }
}
