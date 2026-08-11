use dtoken_compress::{
    content_router::{minify_json_losslessly, route_content, CodecKind},
    context_pipeline::{plan_context, prepare_context_transform, ZoneDisposition},
    ResolvedOptimizationContext,
};
use dtoken_types::{CompressionPolicy, OpticalEndpoint, Protocol};
use proptest::prelude::*;
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

fn request(tool_result: String) -> Value {
    let mut messages = vec![
        json!({"role":"assistant","content":[{"type":"tool_use","id":"old-call","name":"build","input":{}}]}),
        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"old-call","content":tool_result}]}),
    ];
    for index in 0..6 {
        messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":format!("recent-{index}"),"name":"read","input":{}}]}));
        messages.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":format!("recent-{index}"),"content":"recent"}]}));
    }
    messages.push(json!({"role":"user","content":[{"type":"text","text":"current user"}]}));
    json!({"model":"model-exact-revision","system":"system remains exact","messages":messages,"extension":{"must_remain":"exact"}})
}

proptest! {
    #[test]
    fn prop_frozen_prefix_byte_identical(fragment in "[a-zA-Z0-9 ]{1,32}") {
        let tool_result = std::iter::repeat_n(format!("INFO {fragment}"), 12)
            .collect::<Vec<_>>()
            .join("\n");
        let original = request(tool_result);
        let plan = plan_context(&context(), &original).expect("supported context");
        let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, original.clone())
            .expect("supported context");
        let candidate = prepared.as_ref().map(|item| item.candidate_body()).unwrap_or(&original);
        for zone in plan.zones().iter().filter(|zone| zone.disposition() == ZoneDisposition::FrozenPrefix) {
            prop_assert_eq!(original.pointer(zone.pointer()), candidate.pointer(zone.pointer()));
        }
    }

    #[test]
    fn prop_secret_never_transformed(suffix in "[a-zA-Z0-9]{24,48}") {
        let text = format!("Authorization: Bearer sk-{suffix}");
        let route = route_content(&text);
        prop_assert_eq!(route.codec, CodecKind::Passthrough);
        prop_assert_eq!(route.reason_code, "secret-preserve-text");
    }

    #[test]
    fn prop_json_minify_semantic_equivalent(value in "[a-zA-Z0-9 _-]{0,64}") {
        let original = serde_json::to_string_pretty(&json!({"value": value, "items": [1, 2, 3]})).expect("json");
        let minified = minify_json_losslessly(&original).expect("pretty JSON is minifiable");
        let before: Value = serde_json::from_str(&original).expect("original JSON");
        let after: Value = serde_json::from_str(&minified).expect("minified JSON");
        prop_assert_eq!(before, after);
    }

    #[test]
    fn prop_each_ir_node_has_exactly_one_exclusive_action(fragment in "[a-zA-Z0-9 ]{1,32}") {
        let tool_result = std::iter::repeat_n(format!("INFO {fragment}"), 12)
            .collect::<Vec<_>>()
            .join("\n");
        let original = request(tool_result);
        let plan = plan_context(&context(), &original).expect("supported context");
        let prepared = prepare_context_transform(&context(), CompressionPolicy::Balanced, original)
            .expect("supported context");
        // Every manifest entry is a distinct IR node: no node is transformed
        // twice, no protected/frozen node enters the manifest, and every
        // transformed node lies inside the transformable window.
        let entries = prepared
            .as_ref()
            .map(|item| item.manifest().entries())
            .unwrap_or_default();
        let mut pointers = entries
            .iter()
            .map(|entry| entry.pointer())
            .collect::<Vec<_>>();
        pointers.sort_unstable();
        pointers.dedup();
        prop_assert_eq!(pointers.len(), entries.len(), "each node has one action");
        for entry in entries {
            prop_assert!(
                plan.transformable_zones()
                    .iter()
                    .any(|zone| zone.pointer() == entry.pointer()),
                "transformed node must sit inside the transformable window"
            );
        }
    }
}
