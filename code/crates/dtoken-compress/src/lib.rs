use anyhow::Result;
use dtoken_types::{CompressionPolicy, CompressionResult};
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct CompressionEngine;

impl CompressionEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn compress(&self, policy: CompressionPolicy, body: Value) -> Result<CompressionResult> {
        let rough_tokens_before = estimate_tokens(&body);

        let transforms_applied = match policy {
            CompressionPolicy::Off => vec![],
            CompressionPolicy::Safe => vec!["m1-safe-placeholder".to_string()],
            CompressionPolicy::Balanced => vec!["m1-balanced-placeholder".to_string()],
            CompressionPolicy::Aggressive => vec!["m1-aggressive-placeholder".to_string()],
        };

        Ok(CompressionResult {
            original_tokens: rough_tokens_before,
            compressed_tokens: rough_tokens_before,
            tokens_saved: 0,
            transforms_applied,
            body,
        })
    }
}

fn estimate_tokens(value: &Value) -> usize {
    value.to_string().chars().count().div_ceil(4)
}
