pub mod hybrid;
pub mod optical;

use anyhow::Result;
use dtoken_types::{CompressionPolicy, CompressionResult};
use serde_json::Value;
use std::time::Instant;

pub use hybrid::{
    lower_hybrid_context, ApprovedHybridTarget, ApprovedHybridTargetKind, HybridContentProvenance,
    HybridContextIr, HybridContextNode, HybridNodeKind, HybridProtection, HybridProtectionReason,
    HybridRole, ResolvedOptimizationContext,
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
        })
    }

    pub fn compress_resolved(
        &self,
        context: &ResolvedOptimizationContext,
        policy: CompressionPolicy,
        body: Value,
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
            });
        }

        let mut transforms_applied = Vec::new();
        let compressed_body =
            hybrid::apply_safe_text_codec(context, policy, body, &mut transforms_applied);
        let compressed_tokens = estimate_tokens(&compressed_body);
        let tokens_saved = original_tokens.saturating_sub(compressed_tokens);

        Ok(CompressionResult {
            original_tokens,
            compressed_tokens,
            tokens_saved,
            elapsed_us: started.elapsed().as_micros().max(1),
            transforms_applied,
            body: compressed_body,
        })
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
        let body = json!({
            "messages": [
                {"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"build","input":{}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":repeated_log}]},
                {"role":"assistant","content":[{"type":"text","text":"old"}]},
                {"role":"user","content":[{"type":"text","text":"recent"}]},
                {"role":"assistant","content":[{"type":"text","text":"recent"}]},
                {"role":"user","content":[{"type":"text","text":"current"}]}
            ]
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
}
