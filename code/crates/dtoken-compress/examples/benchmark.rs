use anyhow::Result;
use dtoken_compress::CompressionEngine;
use dtoken_types::CompressionPolicy;
use serde_json::json;
use std::time::Instant;

fn main() -> Result<()> {
    let engine = CompressionEngine::new();
    let cases = [
        (
            "logs",
            "2026-07-04T10:00:00Z INFO build started\n2026-07-04T10:00:00Z INFO build started\n\n2026-07-04T10:00:01Z WARN retrying route\n2026-07-04T10:00:01Z WARN retrying route\nfinal useful line",
        ),
        (
            "code",
            "fn main() {\n    println!(\"hello\");\n}\n\nfn main() {\n    println!(\"hello\");\n}\n// route adapter test output follows\n// route adapter test output follows",
        ),
        (
            "json",
            "{\n  \"route\": \"opencode-real-routed\",\n  \"provider\": \"opencode-zen\",\n  \"events\": [1, 1, 1],\n  \"events\": [1, 1, 1]\n}",
        ),
        (
            "long_text",
            &((0..260)
                .map(|index| format!("diagnostic-token-{index}"))
                .collect::<Vec<_>>()
                .join(" ")),
        ),
    ];

    let mut rows = Vec::new();
    for (name, content) in cases {
        for (policy_name, policy) in [
            ("balanced", CompressionPolicy::Balanced),
            ("aggressive", CompressionPolicy::Aggressive),
        ] {
            let body = json!({
                "messages": [{
                    "role": "user",
                    "content": content
                }]
            });
            let started = Instant::now();
            let result = engine.compress(policy, body)?;
            let wall_elapsed_us = started.elapsed().as_micros().max(1);
            let ratio = if result.original_tokens == 0 {
                0.0
            } else {
                result.tokens_saved as f64 / result.original_tokens as f64
            };
            rows.push(json!({
                "case": name,
                "policy": policy_name,
                "original_tokens": result.original_tokens,
                "compressed_tokens": result.compressed_tokens,
                "tokens_saved": result.tokens_saved,
                "compression_ratio": ratio,
                "engine_elapsed_us": result.elapsed_us,
                "wall_elapsed_us": wall_elapsed_us,
                "transforms": result.transforms_applied
            }));
        }
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "cases": rows }))?
    );
    Ok(())
}
