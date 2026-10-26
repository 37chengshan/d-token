use dtoken_compress::context_pipeline::{
    plan_context, plan_context_with_cache_evidence, prepare_context_transform,
    prepare_context_transform_with_cache_evidence, validate_context_transform, CacheEvidenceStatus,
    ContextTransformError, ProviderCacheEvidence, ProviderCacheStrategy, ZoneDisposition,
};
use dtoken_compress::optical::risk::{classify_block, RiskTier};
use dtoken_compress::{CompressionEngine, ResolvedOptimizationContext};
use dtoken_types::{CompressionPolicy, OpticalEndpoint, Protocol};
use serde_json::{json, Value};

fn context() -> ResolvedOptimizationContext {
    ResolvedOptimizationContext {
        route_id: "route-primary".to_string(),
        provider_id: "provider-primary".to_string(),
        model: "model-exact-revision".to_string(),
        input_protocol: Protocol::Anthropic,
        output_protocol: Protocol::Anthropic,
        endpoint: OpticalEndpoint::AnthropicMessages,
    }
}

fn openai_context(endpoint: OpticalEndpoint) -> ResolvedOptimizationContext {
    ResolvedOptimizationContext {
        route_id: "route-openai".to_string(),
        provider_id: "provider-openai".to_string(),
        model: "model-openai-revision".to_string(),
        input_protocol: Protocol::OpenAi,
        output_protocol: Protocol::OpenAi,
        endpoint,
    }
}

fn request() -> Value {
    let log = ["INFO build complete"; 12].join("\n");
    let mut messages = vec![
        json!({"role":"assistant","content":[{"type":"tool_use","id":"old-call","name":"build","input":{}}]}),
        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"old-call","content":log}]}),
    ];
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
    messages.push(json!({"role":"user","content":[{"type":"text","text":"current user"}]}));
    json!({
        "model": "model-exact-revision",
        "system": "system remains exact",
        "messages": messages,
        "extension": {"must_remain":"exact"}
    })
}

fn code_fixture(language: &str) -> &'static str {
    match language {
        "rust" => include_str!("fixtures/code_codec/rust/async_and_generics.rs"),
        "typescript" => include_str!("fixtures/code_codec/typescript/sample.ts"),
        "javascript" => include_str!("fixtures/code_codec/javascript/sample.js"),
        "python" => include_str!("fixtures/code_codec/python/sample.py"),
        "go" => include_str!("fixtures/code_codec/go/sample.go"),
        "java" => include_str!("fixtures/code_codec/java/Sample.java"),
        "c" => include_str!("fixtures/code_codec/c/sample.c"),
        "cpp" => include_str!("fixtures/code_codec/cpp/sample.cpp"),
        _ => unreachable!(),
    }
    .trim_end_matches('\n')
}

#[test]
fn cache_breakpoint_separates_frozen_prefix_transformable_window_and_protected_tail() {
    let body = request();
    let plan = plan_context(&context(), &body).expect("context plan");

    assert_eq!(plan.transformable_zones().len(), 1);
    assert_eq!(
        plan.transformable_zones()[0].pointer(),
        "/messages/1/content/0/content"
    );
    assert_eq!(
        plan.transformable_zones()[0].disposition(),
        ZoneDisposition::TransformableWindow
    );
    assert!(plan
        .zones()
        .iter()
        .take_while(|zone| zone.pointer() != "/messages/1/content/0/content")
        .all(|zone| zone.disposition() == ZoneDisposition::FrozenPrefix));
    assert!(plan
        .zones()
        .iter()
        .skip_while(|zone| zone.pointer() != "/messages/1/content/0/content")
        .skip(1)
        .all(|zone| zone.disposition() == ZoneDisposition::ProtectedTail));
    assert!(plan.cache_alignment().stable_prefix_digest().is_some());
}

#[test]
fn changing_only_transformable_window_does_not_change_frozen_prefix_identity() {
    let body = request();
    let first = plan_context(&context(), &body).expect("first plan");
    let mut changed = body.clone();
    *changed
        .pointer_mut("/messages/1/content/0/content")
        .expect("live target") = Value::String("INFO changed\nINFO changed".to_string());
    let second = plan_context(&context(), &changed).expect("second plan");

    assert_eq!(
        first.cache_alignment().stable_prefix_digest(),
        second.cache_alignment().stable_prefix_digest()
    );
}

#[test]
fn cache_identity_is_bound_to_route_provider_model_and_stable_prefix() {
    let body = request();
    let first_context = context();
    let mut second_context = context();
    second_context.model = "different-model-revision".to_string();
    let first = plan_context(&first_context, &body).expect("first plan");
    let second = plan_context(&second_context, &body).expect("second plan");

    assert_eq!(
        first.cache_alignment().stable_prefix_digest(),
        second.cache_alignment().stable_prefix_digest()
    );
    assert_ne!(
        first.cache_alignment().cache_key_digest(),
        second.cache_alignment().cache_key_digest()
    );
    assert_ne!(
        first.cache_alignment().resolved_identity_digest(),
        second.cache_alignment().resolved_identity_digest()
    );
}

#[test]
fn provider_cache_evidence_must_match_exact_route_provider_and_model() {
    let body = request();
    let context = context();
    let evidence = ProviderCacheEvidence {
        route_id: context.route_id.clone(),
        provider_id: context.provider_id.clone(),
        model: context.model.clone(),
        strategy: ProviderCacheStrategy::AnthropicExplicit,
        ttl_seconds: Some(300),
        min_prefix_bytes: 1,
        source_revision: "b".repeat(64),
    };
    let plan = plan_context_with_cache_evidence(&context, &body, Some(&evidence))
        .expect("verified cache evidence");
    assert_eq!(
        plan.cache_alignment().evidence_status(),
        CacheEvidenceStatus::VerifiedEligible
    );
    assert_eq!(
        plan.cache_alignment().provider_strategy(),
        Some(ProviderCacheStrategy::AnthropicExplicit)
    );

    let mut mismatch = evidence;
    mismatch.model = "other-model".to_string();
    assert_eq!(
        plan_context_with_cache_evidence(&context, &body, Some(&mismatch))
            .expect_err("mismatch must fail"),
        ContextTransformError::ProviderCacheEvidenceMismatch
    );
    let fallback = CompressionEngine::new()
        .compress_resolved_with_cache_evidence(
            &context,
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&mismatch),
            None,
        )
        .expect("invalid cache evidence preserves the request");
    assert_eq!(fallback.body, body);
    assert_eq!(fallback.tokens_saved, 0);
    assert_eq!(
        fallback.fallback_reason_code.as_deref(),
        Some("compression_cache_evidence_mismatch")
    );
    assert_eq!(
        plan_context(&context, &body)
            .expect("default plan")
            .cache_alignment()
            .evidence_status(),
        CacheEvidenceStatus::Unavailable
    );
}

#[test]
fn verified_cache_evidence_is_carried_into_the_pre_send_transform_plan() {
    let body = request();
    let context = context();
    let evidence = ProviderCacheEvidence {
        route_id: context.route_id.clone(),
        provider_id: context.provider_id.clone(),
        model: context.model.clone(),
        strategy: ProviderCacheStrategy::AnthropicExplicit,
        ttl_seconds: Some(300),
        min_prefix_bytes: 1,
        source_revision: "c".repeat(64),
    };
    let prepared = prepare_context_transform_with_cache_evidence(
        &context,
        CompressionPolicy::Balanced,
        body,
        Some(&evidence),
        None,
    )
    .expect("prepare")
    .expect("candidate");
    assert_eq!(
        prepared.plan().cache_alignment().evidence_status(),
        CacheEvidenceStatus::VerifiedEligible
    );
    assert_eq!(
        prepared.plan().cache_alignment().evidence_revision(),
        Some(evidence.source_revision.as_str())
    );
    prepared.validate().expect("validated candidate");

    let mut invalid = evidence;
    invalid.source_revision = "invalid".to_string();
    assert_eq!(
        plan_context_with_cache_evidence(&context, &request(), Some(&invalid))
            .expect_err("invalid evidence"),
        ContextTransformError::InvalidProviderCacheEvidence
    );
}

#[test]
fn manifest_validator_accepts_only_declared_target_changes() {
    let body = request();
    let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, body)
        .expect("prepare")
        .expect("candidate");
    assert_eq!(prepared.manifest().entries().len(), 1);
    assert!(prepared.manifest().bytes_saved() > 0);

    let validated = prepared.validate().expect("valid candidate");
    assert_eq!(validated.manifest().entries().len(), 1);
    assert!(validated
        .body()
        .pointer("/messages/1/content/0/content")
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains("[repeated x12]")));
}

#[test]
fn validator_rejects_mutation_outside_manifest_and_engine_falls_back() {
    let body = request();
    let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, body.clone())
        .expect("prepare")
        .expect("candidate");
    let mut tampered = prepared.candidate_body().clone();
    if let Some(value) = tampered.pointer_mut("/extension/must_remain") {
        *value = Value::String("tampered".to_string());
    }

    assert_eq!(
        validate_context_transform(prepared.original_body(), &tampered, prepared.manifest())
            .expect_err("tamper must fail"),
        ContextTransformError::MutationOutsideManifest
    );

    let result = CompressionEngine::new()
        .compress_resolved(&context(), CompressionPolicy::Balanced, body.clone())
        .expect("engine fallback");
    assert_ne!(result.body, Value::Null);
    assert_eq!(
        result.body.pointer("/extension/must_remain"),
        body.pointer("/extension/must_remain")
    );
}

#[test]
fn exact_path_search_results_remain_native_text() {
    let mut body = request();
    let search = [
        "src/a.rs:10 result",
        "src/a.rs:10 result",
        "src/b.rs:20 other",
        "src/c.rs:30 final",
    ]
    .join("\n");
    assert_eq!(classify_block(&search).tier, RiskTier::BlockUnsafe);
    *body
        .pointer_mut("/messages/1/content/0/content")
        .expect("historical result") = Value::String(search);

    let prepared =
        prepare_context_transform(&context(), CompressionPolicy::Balanced, body).expect("prepare");
    assert!(prepared.is_none(), "exact paths must not enter a codec");
}

#[test]
fn build_output_fold_retains_every_distinct_status_line() {
    let mut body = request();
    let build = [
        "Compiling alpha",
        "Compiling alpha",
        "Running alpha tests",
        "Finished test target",
    ]
    .join("\n");
    *body
        .pointer_mut("/messages/1/content/0/content")
        .expect("historical result") = Value::String(build);

    let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, body)
        .expect("prepare")
        .expect("build candidate");
    assert_eq!(
        prepared.manifest().entries()[0].codec_revision(),
        "build_fold_v1"
    );
    let validated = prepared.validate().expect("validated build");
    let text = validated
        .body()
        .pointer("/messages/1/content/0/content")
        .and_then(Value::as_str)
        .expect("build text");
    assert!(text.contains("Compiling alpha [repeated x2]"));
    assert!(text.contains("Running alpha tests"));
    assert!(text.contains("Finished test target"));
}

#[test]
fn strictly_reversible_unified_diff_context_repeat_enters_manifest() {
    let mut body = request();
    let diff = [
        "diff --git a/src/a.rs b/src/a.rs",
        "--- a/src/a.rs",
        "+++ b/src/a.rs",
        "@@ -1,4 +1,4 @@",
        " unchanged",
        " unchanged",
        " unchanged",
        "-old exact",
        "+new exact",
    ]
    .join("\n");
    *body
        .pointer_mut("/messages/1/content/0/content")
        .expect("historical result") = Value::String(diff);

    let prepared =
        prepare_context_transform(&context(), CompressionPolicy::Balanced, body).expect("prepare");
    let prepared = prepared.expect("strictly reversible diff candidate");
    assert_eq!(
        prepared.manifest().entries()[0].codec_revision(),
        "diff_fold_v1"
    );
    let validated = prepared.validate().expect("validated diff");
    let text = validated
        .body()
        .pointer("/messages/1/content/0/content")
        .and_then(Value::as_str)
        .expect("diff text");
    assert!(text.contains(" unchanged [repeated x3]"));
}

#[test]
fn multi_language_code_tool_results_use_production_codec() {
    for language in [
        "rust",
        "typescript",
        "javascript",
        "python",
        "go",
        "java",
        "c",
        "cpp",
    ] {
        let source = format!("```{language}\n\n{}\n\n```", code_fixture(language));
        let expected = format!("```{language}\n{}\n```", code_fixture(language));
        let mut body = request();
        *body
            .pointer_mut("/messages/1/content/0/content")
            .expect("historical result") = Value::String(source.clone());

        let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, body)
            .expect("prepare")
            .unwrap_or_else(|| panic!("code candidate: {language}"));
        assert_eq!(
            prepared.manifest().entries()[0].codec_revision(),
            "code_fence_envelope_trim_v1"
        );
        let validated = prepared.validate().expect("validated code");
        let code = validated
            .body()
            .pointer("/messages/1/content/0/content")
            .and_then(Value::as_str)
            .expect("code");
        assert_eq!(code, expected, "{language}");
    }
}

#[test]
fn secret_malformed_or_nonbeneficial_code_stays_passthrough() {
    for source in [
        "```python\n\nconst TOKEN = \"sk-not-for-codec\";\n\n```",
        "```typescript\n\n//# sourceMappingURL=sample.js.map\nconst value: number = 1;\n\n```",
        "```javascript\n\n//# sourceMappingURL=sample.js.map\nconst value = 1;\n\n```",
        "```go\n\n//line generated.go:7\npackage main\nfunc main() {}\n\n```",
        "```java\n\nException in thread \"main\" java.lang.RuntimeException\n\tat codec.Sample.add(Sample.java:7)\n\n```",
        "```c\n\n#line 42 \"generated.c\"\nint main(void) { return __LINE__; }\n\n```",
        "```cpp\n\n#line 42 \"generated.cpp\"\nint main() { return __LINE__; }\n\n```",
        "```rust\npub fn add() {}\n```",
        "```rust\n\nconst TOKEN: &str = \"sk-not-for-codec\";\n\n```",
        "```python\n\npass\n```extra",
    ] {
        let mut body = request();
        *body
            .pointer_mut("/messages/1/content/0/content")
            .expect("historical result") = Value::String(source.to_string());

        let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, body)
            .expect("rejection is not an error");
        assert!(prepared.is_none(), "code must stay byte-exact: {source}");
    }
}

#[test]
fn prepared_and_validated_debug_output_never_contains_request_content() {
    let body = request();
    let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, body)
        .expect("prepare")
        .expect("candidate");
    let prepared_debug = format!("{prepared:?}");
    assert!(!prepared_debug.contains("system remains exact"));
    assert!(!prepared_debug.contains("INFO build complete"));

    let validated = prepared.validate().expect("validated");
    let validated_debug = format!("{validated:?}");
    assert!(!validated_debug.contains("system remains exact"));
    assert!(!validated_debug.contains("INFO build complete"));
}

#[test]
fn repeated_plain_text_uses_reversible_text_codec_in_production_path() {
    let mut body = request();
    let text = [
        "The build completed successfully.",
        "The build completed successfully.",
        "No additional action is required.",
    ]
    .join("\n");
    *body
        .pointer_mut("/messages/1/content/0/content")
        .expect("historical result") = Value::String(text);

    let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, body)
        .expect("prepare")
        .expect("text candidate");
    assert_eq!(
        prepared.manifest().entries()[0].codec_revision(),
        "text_repeat_fold_v1"
    );
    let validated = prepared.validate().expect("validated text");
    let text = validated
        .body()
        .pointer("/messages/1/content/0/content")
        .and_then(Value::as_str)
        .expect("text");
    assert!(text.contains("The build completed successfully. [repeated x2]"));
    assert!(text.contains("No additional action is required."));
}

#[test]
fn safe_html_repeat_fold_is_reversible_in_production_path() {
    let mut body = request();
    let html = [
        "<!doctype html>",
        "<ul>",
        "<li>same</li>",
        "<li>same</li>",
        "<li>same</li>",
        "<li>same</li>",
        "</ul>",
    ]
    .join("\n");
    *body
        .pointer_mut("/messages/1/content/0/content")
        .expect("historical result") = Value::String(html);

    let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, body)
        .expect("prepare")
        .expect("html candidate");
    assert_eq!(
        prepared.manifest().entries()[0].codec_revision(),
        "html_repeat_fold_v1"
    );
    let validated = prepared.validate().expect("validated html");
    let text = validated
        .body()
        .pointer("/messages/1/content/0/content")
        .and_then(Value::as_str)
        .expect("html");
    assert!(text.contains("<li>same</li> [repeated x4]"));
}

#[test]
fn openai_chat_uses_the_same_manifest_validator_contract() {
    let repeated = ["WARN compile retry"; 12].join("\n");
    let mut messages = vec![
        json!({"role":"assistant","tool_calls":[{"id":"call-1","type":"function","function":{"name":"compile","arguments":"{\"path\":\"src/lib.rs\"}"}}]}),
        json!({"role":"tool","tool_call_id":"call-1","content":repeated}),
    ];
    for index in 0..6 {
        messages.push(json!({
            "role":"assistant",
            "tool_calls":[{"id":format!("recent-{index}"),"type":"function","function":{"name":"read","arguments":"{}"}}]
        }));
        messages.push(json!({
            "role":"tool",
            "tool_call_id":format!("recent-{index}"),
            "content":"recent"
        }));
    }
    messages.push(json!({"role":"user","content":"current"}));
    let body = json!({
        "model":"model-openai-revision",
        "tools":[{"type":"function","function":{"name":"compile","parameters":{"type":"object"}}}],
        "messages":messages
    });
    let prepared = prepare_context_transform(
        &openai_context(OpticalEndpoint::OpenAiChatCompletions),
        CompressionPolicy::Balanced,
        body,
    )
    .expect("prepare")
    .expect("OpenAI candidate");
    assert_eq!(prepared.manifest().entries().len(), 1);
    assert_eq!(
        prepared.manifest().entries()[0].pointer(),
        "/messages/1/content"
    );
    prepared.validate().expect("validated OpenAI candidate");
}

#[test]
fn openai_responses_text_codec_transforms_closed_historical_tool_result() {
    // R3.C1: the native Responses wire now runs the same resolved safe-text
    // pipeline as Chat. A closed historical low-risk tool result (outside the
    // recent six completed tool pairs) is the approved candidate; every other
    // field stays byte-faithful per the Codex schema contract.
    let repeated = ["INFO result"; 12].join("\n");
    let mut input = vec![
        json!({"type":"function_call","id":"fc-1","call_id":"call-1","name":"run","arguments":"{}"}),
        json!({"type":"function_call_output","call_id":"call-1","output":repeated}),
    ];
    for index in 0..6 {
        input.push(json!({
            "type":"function_call","id":format!("fc-recent-{index}"),"call_id":format!("call-recent-{index}"),
            "name":"read","arguments":"{}"
        }));
        input.push(json!({
            "type":"function_call_output","call_id":format!("call-recent-{index}"),
            "output":format!("recent output {index}")
        }));
    }
    input.push(
        json!({"type":"message","role":"user","content":[{"type":"input_text","text":"continue"}]}),
    );
    let body = json!({
        "model":"model-openai-revision",
        "instructions":"stay exact",
        "input": input
    });
    let prepared = prepare_context_transform(
        &openai_context(OpticalEndpoint::OpenAiResponses),
        CompressionPolicy::Balanced,
        body.clone(),
    )
    .expect("Responses preparation")
    .expect("historical closed tool result yields a Responses candidate");
    assert_eq!(prepared.manifest().entries().len(), 1);
    assert_eq!(
        prepared.manifest().entries()[0].pointer(),
        "/input/1/output",
        "the patch targets the exact function_call_output string field"
    );
    let validated = prepared.validate().expect("validated Responses candidate");
    let (transformed, _manifest) = validated.into_parts();
    // Byte-faithful outside the single approved field.
    assert_eq!(transformed["model"], body["model"]);
    assert_eq!(transformed["instructions"], body["instructions"]);
    assert_eq!(transformed["input"][0], body["input"][0]);
    assert_eq!(transformed["input"][2], body["input"][2]);
    assert_ne!(
        transformed["input"][1]["output"], body["input"][1]["output"],
        "the repeated historical output is folded"
    );
    // The folded value must still round-trip to the original through the
    // manifest (rollback evidence is part of the contract).
    assert!(transformed["input"][1]["output"].as_str().is_some());
}

#[test]
fn openai_responses_recent_tail_tool_result_stays_untransformed() {
    // A single tool result inside the recent tail never becomes a candidate:
    // the fail-closed protection is identical to the Chat wire.
    let repeated = ["INFO result"; 12].join("\n");
    let body = json!({
        "model":"model-openai-revision",
        "input":[
            {"type":"function_call","id":"fc-1","call_id":"call-1","name":"run","arguments":"{}"},
            {"type":"function_call_output","call_id":"call-1","output":repeated},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"continue"}]}
        ]
    });
    let prepared = prepare_context_transform(
        &openai_context(OpticalEndpoint::OpenAiResponses),
        CompressionPolicy::Balanced,
        body,
    )
    .expect("Responses preparation");
    assert!(prepared.is_none(), "recent-tail output stays untouched");
}

#[test]
fn newline_and_crlf_repeat_folds_preserve_exact_line_endings() {
    let line = "The exact repeated line must preserve its line ending.";
    for (source, line_ending) in [
        (format!("{line}\n{line}\n{line}\n{line}\n"), "\n"),
        (format!("{line}\r\n{line}\r\n{line}\r\n{line}\r\n"), "\r\n"),
    ] {
        let mut body = request();
        *body
            .pointer_mut("/messages/1/content/0/content")
            .expect("historical result") = Value::String(source);
        let result = CompressionEngine::new()
            .compress_resolved(&context(), CompressionPolicy::Balanced, body.clone())
            .expect("line-ending-aware repeat fold");
        assert!(result
            .transforms_applied
            .iter()
            .any(|item| item == "text_repeat_fold_v1"));
        let candidate = result
            .body
            .pointer("/messages/1/content/0/content")
            .and_then(Value::as_str)
            .expect("folded historical result");
        assert!(candidate.contains("[repeated x4]"));
        assert!(candidate.ends_with(line_ending));
        assert!(result.compressed_tokens < result.original_tokens);
    }
}

#[test]
fn frozen_message_count_follows_cache_control_markers() {
    use dtoken_compress::context_pipeline::compute_frozen_message_count;

    let no_markers = json!({"messages": [{"role": "user", "content": "hi"}]});
    assert_eq!(compute_frozen_message_count(&no_markers), 0);

    let marker_at_one = json!({
        "messages": [
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": [
                {"type": "text", "text": "x", "cache_control": {"type": "ephemeral"}}
            ]}
        ]
    });
    assert_eq!(compute_frozen_message_count(&marker_at_one), 2);

    let highest_marker_wins = json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "a", "cache_control": {"type": "ephemeral"}}
            ]},
            {"role": "user", "content": "b"},
            {"role": "assistant", "content": [
                {"type": "text", "text": "c", "cache_control": {"type": "ephemeral", "ttl": "1h"}}
            ]}
        ]
    });
    assert_eq!(compute_frozen_message_count(&highest_marker_wins), 3);

    // Markers in system / tools never bump the message floor.
    let system_marker = json!({
        "system": [{"type": "text", "text": "sys", "cache_control": {"type": "ephemeral"}}],
        "tools": [{"name": "t", "cache_control": {"type": "ephemeral"}, "input_schema": {}}],
        "messages": [{"role": "user", "content": "hi"}]
    });
    assert_eq!(compute_frozen_message_count(&system_marker), 0);

    // String content carries no block list, hence no markers.
    let string_content = json!({
        "messages": [{"role": "user", "content": "cache_control is not a marker here"}]
    });
    assert_eq!(compute_frozen_message_count(&string_content), 0);
}

#[test]
fn cache_control_frozen_floor_keeps_below_floor_messages_in_frozen_prefix() {
    let body = request();
    // Marker on message 1 (the historical tool result that is normally the
    // single transformable window): floor becomes 2, so that message and
    // everything before it must be frozen.
    let mut marked = body.clone();
    marked["messages"][1]["content"][0]["cache_control"] = json!({"type": "ephemeral"});
    let plan = plan_context(&context(), &marked).expect("marked plan");
    assert_eq!(plan.cache_alignment().frozen_message_floor(), 2);
    assert!(plan.transformable_zones().is_empty());
    assert!(
        plan.zones()
            .iter()
            .filter(|zone| zone.reason_code() == "cache-control-frozen-message")
            .count()
            >= 2
    );
    assert!(plan
        .zones()
        .iter()
        .all(|zone| { zone.disposition() != ZoneDisposition::TransformableWindow }));

    // Compression must be a no-op when the only transformable node is
    // below the frozen floor.
    let result = CompressionEngine::new()
        .compress_resolved(&context(), CompressionPolicy::Balanced, marked.clone())
        .expect("frozen compression");
    assert!(result.transforms_applied.is_empty());
    assert_eq!(result.body, marked);
}

#[test]
fn thinking_blocks_are_explicitly_protected_and_never_transformed() {
    use dtoken_compress::{HybridNodeKind, HybridProtection, HybridProtectionReason};

    let body = json!({
        "model": "model-exact-revision",
        "system": "system",
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "build it"}]},
            {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "let me plan the build steps", "signature": "sig1"},
                {"type": "tool_use", "id": "t1", "name": "build", "input": {}}
            ]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "done"}]}
        ]
    });
    let ir = dtoken_compress::lower_hybrid_context(&context(), &body).expect("lowered context");
    let thinking = ir
        .nodes()
        .iter()
        .find(|node| node.kind() == HybridNodeKind::Thinking)
        .expect("thinking node present");
    assert_eq!(thinking.protection(), HybridProtection::AlwaysText);
    assert_eq!(thinking.reason(), HybridProtectionReason::Thinking);
    assert_eq!(thinking.source_path(), "/messages/1/content/0");

    let result = CompressionEngine::new()
        .compress_resolved(&context(), CompressionPolicy::Balanced, body.clone())
        .expect("thinking compression");
    assert!(result.transforms_applied.is_empty());
    assert_eq!(
        result.body.pointer("/messages/1/content/0/thinking"),
        Some(&json!("let me plan the build steps"))
    );
    assert_eq!(result.body, body);
}

#[test]
fn redacted_thinking_blocks_are_equally_protected() {
    use dtoken_compress::HybridNodeKind;

    let body = json!({
        "model": "model-exact-revision",
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "go"}]},
            {"role": "assistant", "content": [
                {"type": "redacted_thinking", "data": "redacted-blob", "signature": "sig2"}
            ]}
        ]
    });
    let ir = dtoken_compress::lower_hybrid_context(&context(), &body).expect("lowered context");
    assert!(ir
        .nodes()
        .iter()
        .any(|node| node.kind() == HybridNodeKind::Thinking));
    let result = CompressionEngine::new()
        .compress_resolved(&context(), CompressionPolicy::Balanced, body.clone())
        .expect("redacted thinking compression");
    assert!(result.transforms_applied.is_empty());
    assert_eq!(result.body, body);
}

#[test]
fn structured_config_tool_result_uses_the_config_codec_in_production_path() {
    let config = [
        "[server]",
        "host = \"aa.example.internal\"",
        "host = \"aa.example.internal\"",
        "zone = \"primary\"",
        "mode = \"fast\"",
    ]
    .join("\n");
    let mut messages = vec![
        json!({"role":"assistant","content":[{"type":"tool_use","id":"old-call","name":"config","input":{}}]}),
        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"old-call","content":config}]}),
    ];
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
    let body = json!({
        "model": "model-exact-revision",
        "system": "system remains exact",
        "messages": messages
    });

    let result = CompressionEngine::new()
        .compress_resolved(&context(), CompressionPolicy::Balanced, body)
        .expect("config compression");
    assert!(
        result
            .transforms_applied
            .iter()
            .any(|item| item == "content_router:config"),
        "{result:#?}"
    );
    assert!(result
        .transforms_applied
        .iter()
        .any(|item| item == "config_repeat_fold_v1"));
    let candidate = result
        .body
        .pointer("/messages/1/content/0/content")
        .and_then(Value::as_str)
        .expect("compressed config");
    assert!(candidate.contains("host = \"aa.example.internal\" [repeated x2]"));
    assert!(candidate.contains("zone = \"primary\""));
    // The fold is byte-exact reversible.
    assert_eq!(
        dtoken_compress::content_router::expand_exact_folded_lines_losslessly(candidate).as_deref(),
        Some(config.as_str())
    );
}

#[test]
fn anthropic_qualified_ccr_retrieve_result_is_preserved_verbatim() {
    let retrieved = [
        "[server]",
        "host = \"aa.example.internal\"",
        "host = \"aa.example.internal\"",
        "zone = \"primary\"",
        "mode = \"fast\"",
    ]
    .join("\n");
    let mut messages = vec![
        json!({"role":"assistant","content":[{"type":"tool_use","id":"retrieve-call","name":"mcp__dtoken__headroom_retrieve","input":{}}]}),
        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"retrieve-call","content":retrieved}]}),
    ];
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
    let body = json!({
        "model": "model-exact-revision",
        "system": "system remains exact",
        "messages": messages
    });

    let result = CompressionEngine::new()
        .compress_resolved(&context(), CompressionPolicy::Balanced, body.clone())
        .expect("CCR retrieve passthrough");

    assert_eq!(result.body, body);
    assert!(result.transforms_applied.is_empty());
}

#[test]
fn openai_ccr_retrieve_result_is_preserved_verbatim() {
    let retrieved = [
        "[server]",
        "host = \"aa.example.internal\"",
        "host = \"aa.example.internal\"",
        "zone = \"primary\"",
        "mode = \"fast\"",
    ]
    .join("\n");
    let mut messages = vec![
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"retrieve-call","type":"function","function":{"name":"headroom_retrieve","arguments":"{}"}}]}),
        json!({"role":"tool","tool_call_id":"retrieve-call","content":retrieved}),
    ];
    for index in 0..6 {
        messages.push(json!({
            "role":"assistant",
            "content":null,
            "tool_calls":[{"id":format!("recent-{index}"),"type":"function","function":{"name":"read","arguments":"{}"}}]
        }));
        messages.push(json!({
            "role":"tool",
            "tool_call_id":format!("recent-{index}"),
            "content":"recent"
        }));
    }
    messages.push(json!({"role":"user","content":"current"}));
    let body = json!({
        "model": "model-openai-revision",
        "messages": messages
    });
    let openai = openai_context(OpticalEndpoint::OpenAiChatCompletions);

    let result = CompressionEngine::new()
        .compress_resolved(&openai, CompressionPolicy::Balanced, body.clone())
        .expect("CCR retrieve passthrough");

    assert_eq!(result.body, body);
    assert!(result.transforms_applied.is_empty());
}

#[test]
fn mixed_tool_result_uses_the_section_router_in_production_path() {
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
    assert!(dtoken_compress::content_router::is_safe_mixed_candidate(
        &mixed
    ));
    let mut messages = vec![
        json!({"role":"assistant","content":[{"type":"tool_use","id":"old-call","name":"mixed","input":{}}]}),
        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"old-call","content":mixed}]}),
    ];
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
    let body = json!({
        "model": "model-exact-revision",
        "system": "system remains exact",
        "messages": messages
    });

    let result = CompressionEngine::new()
        .compress_resolved(&context(), CompressionPolicy::Balanced, body)
        .expect("mixed compression");
    assert!(result
        .transforms_applied
        .iter()
        .any(|item| item == "content_router:mixed"));
    assert!(result
        .transforms_applied
        .iter()
        .any(|item| item == "mixed_section_route_v1"));
    let candidate = result
        .body
        .pointer("/messages/1/content/0/content")
        .and_then(Value::as_str)
        .expect("mixed candidate");
    // Code fence trimmed and JSON sections minified.
    assert!(candidate.contains("fn main() {}"));
    assert!(candidate.contains("{\"items\":[1,2,3]}"));
    assert!(candidate.len() < mixed.len());
    assert_eq!(
        result.body["system"].as_str().unwrap(),
        "system remains exact"
    );
}

#[test]
fn frozen_message_count_override_wins_over_marker_floor_and_none_falls_back() {
    use dtoken_compress::context_pipeline::resolve_frozen_message_count;

    // Marker on messages[2] bumps the marker-derived floor to 3.
    let marked = json!({
        "model": "model-exact-revision",
        "messages": [
            {"role": "user", "content": "first"},
            {"role": "user", "content": "second"},
            {"role": "user", "content": [{"type": "text", "text": "third", "cache_control": {"type": "ephemeral"}}]},
            {"role": "user", "content": "fourth"}
        ]
    });
    assert_eq!(resolve_frozen_message_count(&marked, None), 3);
    // A request-level override wins over the marker-derived floor: the
    // caller owns the cache-prefix contract for this request.
    assert_eq!(resolve_frozen_message_count(&marked, Some(1)), 1);
    assert_eq!(resolve_frozen_message_count(&marked, Some(0)), 0);
    assert_eq!(resolve_frozen_message_count(&marked, Some(4)), 4);

    // Marker in `system` must NOT bump the message-index floor.
    let system_marked = json!({
        "model": "model-exact-revision",
        "system": [{"type": "text", "text": "sys", "cache_control": {"type": "ephemeral"}}],
        "messages": [{"role": "user", "content": "only"}]
    });
    assert_eq!(resolve_frozen_message_count(&system_marked, None), 0);

    // No messages / no markers -> 0; an override still applies.
    assert_eq!(resolve_frozen_message_count(&json!({}), None), 0);
    assert_eq!(resolve_frozen_message_count(&json!({}), Some(2)), 2);
}

#[test]
fn frozen_override_flows_into_prepared_plan_floor() {
    // Use `request()` as-is (proven to produce a candidate). Add cache_control
    // to messages[0] (assistant tool_use "old-call") → marker-derived floor = 1.
    // The first tool result (index 1, turn_index=1) is NOT frozen at floor 1,
    // so the pipeline can still produce a candidate.
    let mut body = request();
    if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
        if !messages.is_empty() {
            if let Some(content) = messages[0].get_mut("content").and_then(Value::as_array_mut) {
                if let Some(first) = content.first_mut() {
                    if let Some(obj) = first.as_object_mut() {
                        obj.insert("cache_control".to_string(), json!({"type": "ephemeral"}));
                    }
                }
            }
        }
    }
    // Marker-derived floor is 1 (index 0 + 1).  A request-level override of 0
    // wins: no frozen prefix.
    let overridden = prepare_context_transform_with_cache_evidence(
        &context(),
        CompressionPolicy::Balanced,
        body.clone(),
        None,
        Some(0),
    )
    .expect("prepare")
    .expect("candidate: override=Some(0) should produce a candidate");
    assert_eq!(
        overridden.plan().cache_alignment().frozen_message_floor(),
        0
    );

    // `None` keeps the marker-derived floor (upstream #2718 parity).
    let fallback = prepare_context_transform_with_cache_evidence(
        &context(),
        CompressionPolicy::Balanced,
        body,
        None,
        None,
    )
    .expect("prepare")
    .expect("candidate: override=None should produce a candidate");
    assert_eq!(fallback.plan().cache_alignment().frozen_message_floor(), 1);
}
