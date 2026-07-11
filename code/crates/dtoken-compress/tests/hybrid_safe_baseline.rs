use dtoken_compress::{
    lower_hybrid_context, CompressionEngine, HybridContentProvenance, HybridNodeKind,
    HybridProtection, HybridProtectionReason, HybridRole, ResolvedOptimizationContext,
};
use dtoken_types::{CompressionPolicy, OpticalEndpoint, Protocol};
use serde_json::{json, Value};

fn resolved_context(
    input_protocol: Protocol,
    output_protocol: Protocol,
    endpoint: OpticalEndpoint,
) -> ResolvedOptimizationContext {
    ResolvedOptimizationContext {
        route_id: "route-primary".to_string(),
        provider_id: "provider-primary".to_string(),
        model: "model-exact-revision".to_string(),
        input_protocol,
        output_protocol,
        endpoint,
    }
}

fn assert_only_paths_changed(original: &Value, optimized: &Value, allowed: &[&str]) {
    fn walk(path: &str, original: &Value, optimized: &Value, changed: &mut Vec<String>) {
        if original == optimized {
            return;
        }
        match (original, optimized) {
            (Value::Object(before), Value::Object(after)) => {
                let mut keys = before.keys().chain(after.keys()).collect::<Vec<_>>();
                keys.sort();
                keys.dedup();
                for key in keys {
                    let next = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                    walk(
                        &next,
                        before.get(key).unwrap_or(&Value::Null),
                        after.get(key).unwrap_or(&Value::Null),
                        changed,
                    );
                }
            }
            (Value::Array(before), Value::Array(after)) if before.len() == after.len() => {
                for (index, (before, after)) in before.iter().zip(after).enumerate() {
                    walk(&format!("{path}/{index}"), before, after, changed);
                }
            }
            _ => changed.push(path.to_string()),
        }
    }

    let mut changed = Vec::new();
    walk("", original, optimized, &mut changed);
    assert_eq!(changed, allowed);
}

#[test]
fn protocol_unknown_legacy_entry_is_fail_safe_passthrough() {
    let body = json!({
        "model": "exact-model-id",
        "system": "  system must stay exact\n\n  system must stay exact  ",
        "messages": [{
            "role": "user",
            "content": "2026-07-11T21:00:00Z INFO repeated\n2026-07-11T21:00:00Z INFO repeated"
        }]
    });

    let result = CompressionEngine::new()
        .compress(CompressionPolicy::Balanced, body.clone())
        .expect("legacy compression should fail safe");

    assert_eq!(result.body, body);
    assert_eq!(result.tokens_saved, 0);
    assert!(result.transforms_applied.is_empty());
}

#[test]
fn anthropic_hybrid_only_mutates_approved_historical_closed_tool_result() {
    let repeated_log = ["INFO build step complete"; 12].join("\n");
    let body = json!({
        "model": "claude-exact-model-revision",
        "system": [{"type":"text","text":"  exact system\n\nkeep whitespace  ","cache_control":{"type":"ephemeral"}}],
        "tools": [{
            "name": "build_project",
            "description": "  exact tool description  ",
            "input_schema": {
                "type": "object",
                "properties": {"target": {"type":"string", "description":" exact schema text "}},
                "required": ["target"]
            }
        }],
        "stop_sequences": ["END  EXACT"],
        "metadata": {"user_id":"user-0123456789abcdef"},
        "messages": [
            {"role":"assistant","content":[{"type":"tool_use","id":"toolu_exact_1","name":"build_project","input":{"target":"  ./src EXACT  "}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_exact_1","content":repeated_log}]},
            {"role":"assistant","content":[{"type":"text","text":"old assistant exact"}]},
            {"role":"user","content":[{"type":"text","text":"recent user exact\n\nkeep"}]},
            {"role":"assistant","content":[{"type":"text","text":"recent assistant exact"}]},
            {"role":"user","content":[{"type":"text","text":"current user exact\n\nkeep"}]}
        ],
        "unknown_extension": {"preserve":"  exact unknown  "}
    });

    let result = CompressionEngine::new()
        .compress_resolved(
            &resolved_context(
                Protocol::Anthropic,
                Protocol::Anthropic,
                OpticalEndpoint::AnthropicMessages,
            ),
            CompressionPolicy::Balanced,
            body.clone(),
        )
        .expect("resolved Anthropic compression should succeed");

    assert_only_paths_changed(&body, &result.body, &["/messages/1/content/0/content"]);
    assert_eq!(result.body["system"], body["system"]);
    assert_eq!(result.body["tools"], body["tools"]);
    assert_eq!(result.body["messages"][0], body["messages"][0]);
    assert_eq!(result.body["messages"][3], body["messages"][3]);
    assert_eq!(result.body["messages"][5], body["messages"][5]);
    assert_eq!(result.body["unknown_extension"], body["unknown_extension"]);
    assert!(result
        .transforms_applied
        .iter()
        .any(|transform| transform == "semantic_closed_tool_result"));
}

#[test]
fn exact_or_secret_historical_tool_results_are_preserved() {
    let body = json!({
        "messages": [
            {"role":"assistant","content":[
                {"type":"tool_use","id":"toolu_hash","name":"read_file","input":{"path":"/tmp/exact"}},
                {"type":"tool_use","id":"toolu_secret","name":"env","input":{}}
            ]},
            {"role":"user","content":[
                {"type":"tool_result","tool_use_id":"toolu_hash","content":"commit 0123456789abcdef0123456789abcdef"},
                {"type":"tool_result","tool_use_id":"toolu_secret","content":"Authorization: Bearer highly-sensitive-value"}
            ]},
            {"role":"assistant","content":[{"type":"text","text":"old"}]},
            {"role":"user","content":[{"type":"text","text":"recent"}]},
            {"role":"assistant","content":[{"type":"text","text":"recent"}]},
            {"role":"user","content":[{"type":"text","text":"current"}]}
        ]
    });

    let result = CompressionEngine::new()
        .compress_resolved(
            &resolved_context(
                Protocol::Anthropic,
                Protocol::Anthropic,
                OpticalEndpoint::AnthropicMessages,
            ),
            CompressionPolicy::Max,
            body.clone(),
        )
        .expect("unsafe candidates should be preserved, not rejected globally");

    assert_eq!(result.body, body);
    assert_eq!(result.tokens_saved, 0);
}

#[test]
fn openai_tool_arguments_and_protocol_fields_remain_identical() {
    let repeated_log = ["WARN compile retry"; 12].join("\n");
    let body = json!({
        "model":"gpt-exact-model",
        "stream":true,
        "tools":[{"type":"function","function":{"name":"compile","description":" exact ","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}],
        "messages":[
            {"role":"system","content":"  exact system  "},
            {"role":"assistant","tool_calls":[{"id":"call_exact_1","type":"function","function":{"name":"compile","arguments":"{\"path\":\"  ./src EXACT  \"}"}}]},
            {"role":"tool","tool_call_id":"call_exact_1","content":repeated_log},
            {"role":"assistant","content":"old"},
            {"role":"user","content":"recent exact"},
            {"role":"assistant","content":"recent exact"},
            {"role":"user","content":"current exact"}
        ],
        "unknown_control":{"preserve":" exact "}
    });

    let result = CompressionEngine::new()
        .compress_resolved(
            &resolved_context(
                Protocol::OpenAi,
                Protocol::OpenAi,
                OpticalEndpoint::OpenAiChatCompletions,
            ),
            CompressionPolicy::Balanced,
            body.clone(),
        )
        .expect("resolved OpenAI Chat compression should succeed");

    assert_only_paths_changed(&body, &result.body, &["/messages/2/content"]);
    assert_eq!(result.body["messages"][1], body["messages"][1]);
    assert_eq!(result.body["tools"], body["tools"]);
    assert_eq!(result.body["stream"], body["stream"]);
    assert_eq!(result.body["unknown_control"], body["unknown_control"]);
}

#[test]
fn mismatched_or_unsupported_resolved_context_is_passthrough() {
    let body = json!({"messages":[{"role":"user","content":"INFO x\nINFO x"}]});
    let context = resolved_context(
        Protocol::Anthropic,
        Protocol::OpenAi,
        OpticalEndpoint::OpenAiResponses,
    );

    let result = CompressionEngine::new()
        .compress_resolved(&context, CompressionPolicy::Max, body.clone())
        .expect("unsupported context should fail safe");

    assert_eq!(result.body, body);
    assert_eq!(result.tokens_saved, 0);
    assert!(result.transforms_applied.is_empty());
}

#[test]
fn unknown_anthropic_role_and_block_are_opaque() {
    let body = json!({
        "messages": [{
            "role": "future_operator",
            "content": [{"type":"future_tool_state","payload":"opaque"}]
        }]
    });
    let context = resolved_context(
        Protocol::Anthropic,
        Protocol::Anthropic,
        OpticalEndpoint::AnthropicMessages,
    );

    let ir = lower_hybrid_context(&context, &body).expect("Anthropic IR");
    let node = ir
        .node_at("/messages/0/content/0")
        .expect("unknown block should remain represented");

    assert_eq!(node.kind(), HybridNodeKind::Unknown);
    assert_eq!(node.role(), HybridRole::Unknown);
    assert_eq!(node.provenance(), HybridContentProvenance::OpaqueInput);
    assert_eq!(node.protection(), HybridProtection::Opaque);
    assert_eq!(node.reason(), HybridProtectionReason::UnknownNode);
    assert!(ir.approved_text_targets().is_empty());
}

#[test]
fn openai_tools_and_developer_instructions_are_explicitly_inventoried() {
    let body = json!({
        "tools": [{
            "type":"function",
            "function": {
                "name":"run",
                "description":"exact tool",
                "parameters":{"type":"object"}
            }
        }],
        "messages": [
            {"role":"system","content":"system"},
            {"role":"developer","content":"developer"}
        ]
    });
    let context = resolved_context(
        Protocol::OpenAi,
        Protocol::OpenAi,
        OpticalEndpoint::OpenAiChatCompletions,
    );

    let ir = lower_hybrid_context(&context, &body).expect("OpenAI Chat IR");
    let tool = ir.node_at("/tools/0").expect("top-level tool node");
    let system = ir.node_at("/messages/0/content").expect("system node");
    let developer = ir.node_at("/messages/1/content").expect("developer node");

    assert_eq!(tool.kind(), HybridNodeKind::ToolDefinition);
    assert_eq!(tool.provenance(), HybridContentProvenance::ToolSchema);
    assert_eq!(tool.protection(), HybridProtection::AlwaysText);
    assert_eq!(developer.role(), HybridRole::Developer);
    assert_eq!(
        developer.provenance(),
        HybridContentProvenance::DeveloperInstruction
    );
    assert_ne!(system.provenance(), developer.provenance());
}

#[test]
fn schema_arguments_and_identifiers_have_explicit_protected_nodes() {
    let anthropic = json!({
        "tools":[{"name":"run","input_schema":{"type":"object"}}],
        "messages":[
            {"role":"assistant","content":[{
                "type":"tool_use","id":"tool-1","name":"run","input":{"path":"src/main.rs"}
            }]},
            {"role":"user","content":[{
                "type":"tool_result","tool_use_id":"tool-1","content":"done"
            }]}
        ]
    });
    let openai = json!({
        "tools":[{"type":"function","function":{"name":"run","parameters":{"type":"object"}}}],
        "messages":[
            {"role":"assistant","tool_calls":[{
                "id":"call-1","type":"function","function":{"name":"run","arguments":"{\"path\":\"src/main.rs\"}"}
            }]},
            {"role":"tool","tool_call_id":"call-1","content":"done"}
        ]
    });
    let anthropic_ir = lower_hybrid_context(
        &resolved_context(
            Protocol::Anthropic,
            Protocol::Anthropic,
            OpticalEndpoint::AnthropicMessages,
        ),
        &anthropic,
    )
    .expect("Anthropic IR");
    let openai_ir = lower_hybrid_context(
        &resolved_context(
            Protocol::OpenAi,
            Protocol::OpenAi,
            OpticalEndpoint::OpenAiChatCompletions,
        ),
        &openai,
    )
    .expect("OpenAI IR");

    for path in [
        "/tools/0/input_schema",
        "/messages/0/content/0/id",
        "/messages/0/content/0/input",
        "/messages/1/content/0/tool_use_id",
    ] {
        assert!(anthropic_ir.node_at(path).is_some(), "missing {path}");
    }
    for path in [
        "/tools/0/function/parameters",
        "/messages/0/tool_calls/0/id",
        "/messages/0/tool_calls/0/function/arguments",
        "/messages/1/tool_call_id",
    ] {
        assert!(openai_ir.node_at(path).is_some(), "missing {path}");
    }

    for ir in [&anthropic_ir, &openai_ir] {
        for node in ir.nodes() {
            if matches!(
                node.kind(),
                HybridNodeKind::ToolSchema
                    | HybridNodeKind::ToolArguments
                    | HybridNodeKind::Identifier
            ) {
                assert_ne!(node.protection(), HybridProtection::Candidate);
            }
        }
    }
}

#[test]
fn approved_targets_are_sealed_and_recent_or_orphan_results_never_appear() {
    let repeated_log = ["INFO safe repeated line"; 12].join("\n");
    let body = json!({
        "messages": [
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"orphan","content":repeated_log.clone()}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"valid","name":"build","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"valid","content":repeated_log.clone()}]},
            {"role":"assistant","content":[{"type":"text","text":"old"}]},
            {"role":"user","content":[{"type":"text","text":"recent"}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"recent","name":"build","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"recent","content":repeated_log}]}
        ]
    });
    let context = resolved_context(
        Protocol::Anthropic,
        Protocol::Anthropic,
        OpticalEndpoint::AnthropicMessages,
    );
    let ir = lower_hybrid_context(&context, &body).expect("Anthropic IR");

    assert_eq!(ir.current_user_turn(), Some(6));
    assert_eq!(ir.approved_text_targets().len(), 1);
    assert_eq!(
        ir.approved_text_targets()[0].patch_path(),
        "/messages/2/content/0/content"
    );
    assert!(!ir.approved_text_targets()[0].node_id().is_empty());
    assert!(!ir.approved_text_targets()[0]
        .precondition_digest()
        .is_empty());
}
