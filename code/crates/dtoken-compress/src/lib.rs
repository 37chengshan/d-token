pub mod optical;

use anyhow::Result;
use dtoken_types::{CompressionPolicy, CompressionResult};
use serde_json::Value;
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct CompressionEngine;

impl CompressionEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn compress(&self, policy: CompressionPolicy, body: Value) -> Result<CompressionResult> {
        let started = Instant::now();
        let original_tokens = estimate_tokens(&body);
        if policy == CompressionPolicy::Off {
            return Ok(CompressionResult {
                original_tokens,
                compressed_tokens: original_tokens,
                tokens_saved: 0,
                elapsed_us: started.elapsed().as_micros().max(1),
                transforms_applied: vec![],
                body,
            });
        }

        let mut transforms_applied = Vec::new();
        let compressed_body = compress_value(policy, body, &mut transforms_applied);
        if compact_json_saved_tokens(&compressed_body) > 0 {
            transforms_applied.push("json_compact_measurement".to_string());
        }
        let compressed_tokens = estimate_tokens(&compressed_body).min(original_tokens);
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

fn compress_value(policy: CompressionPolicy, value: Value, transforms: &mut Vec<String>) -> Value {
    match value {
        Value::String(text) => Value::String(compress_text(policy, &text, transforms)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| compress_value(policy.clone(), item, transforms))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, compress_value(policy.clone(), value, transforms)))
                .collect(),
        ),
        other => other,
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

fn compact_json_saved_tokens(value: &Value) -> usize {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
    let compact = serde_json::to_string(value).unwrap_or_default();
    estimate_text_tokens(&pretty).saturating_sub(estimate_text_tokens(&compact))
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
    fn max_policy_is_explicitly_supported() {
        let result = CompressionEngine::new()
            .compress(
                CompressionPolicy::Max,
                json!({ "large": "fixture payload" }),
            )
            .expect("max compression policy should be accepted");

        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "json_compact_measurement"));
    }

    #[test]
    fn balanced_policy_reduces_repeated_logs_and_records_elapsed_time() {
        let body = json!({
            "messages": [{
                "role": "user",
                "content": "2026-07-04T10:00:00Z INFO build started\n2026-07-04T10:00:00Z INFO build started\n\n2026-07-04T10:00:01Z WARN retrying route\n2026-07-04T10:00:01Z WARN retrying route\nfinal useful line"
            }]
        });

        let result = CompressionEngine::new()
            .compress(CompressionPolicy::Balanced, body)
            .expect("balanced compression should succeed");

        assert!(result.tokens_saved > 0);
        assert!(result.compressed_tokens < result.original_tokens);
        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "blank_line_trim"));
        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "repeat_line_fold"));
        assert!(result.elapsed_us > 0);
        assert!(result.body.to_string().contains("repeated x2"));
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
