//! Tool-schema deterministic normalization (upstream Phase E, PR-E1/PR-E2).
//!
//! Cache savings without touching prompt content: providers key prompt
//! caches on byte-identical prefixes, so the same logical tool set
//! serializes to different bytes depending on the client's tool-collection
//! order and JSON Schema key order.  These two passes make the tool section
//! byte-stable:
//!
//! - **PR-E1** — sort `tools[]` alphabetically by tool name (Anthropic
//!   `tool["name"]`, OpenAI `tool["function"]["name"]`).  Skipped entirely
//!   when any tool carries a top-level `cache_control` marker, because that
//!   marker is position-sensitive: reordering would shift the cache
//!   breakpoint the customer explicitly placed.
//! - **PR-E2** — recursively sort JSON object keys inside every tool's
//!   `input_schema` / `parameters` / `function.parameters`.  JSON Schema
//!   permits arbitrary key order and arrays keep their order; this walker
//!   only canonicalizes object-key order.
//!
//! Both passes are **idempotent** (re-running yields byte-identical output),
//! **reversible** (they never drop or rewrite content, only order), and
//! **secret-safe** (they never touch tool descriptions, parameter values, or
//! any string content).  The outcome records exactly what moved so the
//! caller can attach evidence.
//!
//! Upstream's PR-E3 (auto-placing a `cache_control` marker on the last
//! tool) is deliberately NOT implemented here: it changes customer cache
//! intent and upstream gates it on PAYG auth mode, which is a proxy-layer
//! concern outside this crate's ownership.  Skipping E3 keeps this module a
//! safe, reversible normalization subset.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Evidence for one tool-definition normalization pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSchemaNormalizeOutcome {
    /// Total tool definitions inspected across all supported shapes
    /// (`tools[]` on Anthropic, `tools[]`/`functions[]` on OpenAI).
    pub tools_considered: usize,
    /// PR-E1 fired: at least two tools swapped positions.
    pub tools_reordered: bool,
    /// PR-E2 fired: at least one JSON Schema object had its keys reordered.
    pub schemas_key_sorted: bool,
    /// PR-E4 fired: at least one byte-identical duplicate definition dropped.
    pub tools_deduped: usize,
    /// Any byte change happened (either pass fired).
    pub changed: bool,
}

/// PR-E4: drop byte-identical duplicate tool definitions (graph dedup).
///
/// Two tool definitions are duplicates when their canonical JSON is exactly
/// equal (same name + same schema).  Keeping only the first is lossless:
/// identical bytes mean identical semantics for every provider.  When any
/// tool carries a `cache_control` marker the duplicates are still removed
/// (dedup never reorders, so cache breakpoints are untouched).  Returns the
/// number of removed tools.
pub fn dedupe_tools(tools: &mut Vec<Value>) -> usize {
    let mut seen: Vec<Vec<u8>> = Vec::with_capacity(tools.len());
    let mut kept: Vec<Value> = Vec::with_capacity(tools.len());
    let mut removed = 0;
    for tool in tools.drain(..) {
        // A serialization failure means we cannot prove byte identity — the
        // tool is kept untouched (never treated as a duplicate of an empty
        // key, which would corrupt distinct definitions).
        let Ok(canonical) = serde_json::to_vec(&tool) else {
            kept.push(tool);
            continue;
        };
        if seen.iter().any(|existing| existing == &canonical) {
            removed += 1;
            continue;
        }
        seen.push(canonical);
        kept.push(tool);
    }
    *tools = kept;
    removed
}

/// Sort `tools[]` deterministically by name (PR-E1).
///
/// Returns `true` if the order changed.  Stable: equal keys preserve
/// original relative order.  Unnamed tools fall back to a SHA-256 digest of
/// their canonical JSON as the sort key so they never compare equal by
/// accident.
pub fn sort_tools_deterministically(tools: &mut [Value]) -> bool {
    let before: Vec<String> = tools.iter().map(sort_key).collect();
    tools.sort_by_key(sort_key);
    let after: Vec<String> = tools.iter().map(sort_key).collect();
    before != after
}

/// Build the deterministic sort key for a tool: `tool["name"]` (Anthropic),
/// `tool["function"]["name"]` (OpenAI), else SHA-256 of canonical JSON.
fn sort_key(tool: &Value) -> String {
    if let Some(name) = tool.get("name").and_then(Value::as_str) {
        return name.to_string();
    }
    if let Some(name) = tool
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(Value::as_str)
    {
        return name.to_string();
    }
    sha256_hex_bytes(&serde_json::to_vec(tool).unwrap_or_default())
}

fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Return `true` if any tool object carries a top-level `cache_control`
/// field.  When present, the tool's position is customer intent and the
/// sort must be skipped (upstream `any_tool_has_cache_control`).
pub fn any_tool_has_cache_control(tools: &[Value]) -> bool {
    tools.iter().any(|tool| tool.get("cache_control").is_some())
}

/// Recursively sort JSON object keys in place (PR-E2).
///
/// Returns `true` if any object's key order changed.  Arrays are preserved
/// in order (JSON Schema arrays are ordered: `oneOf`, `anyOf`, `enum`,
/// `prefixItems`); only object keys inside them are canonicalized.  Under a
/// `BTreeMap`-backed `serde_json::Map` (this workspace's default, no
/// `preserve_order` feature) the reorder is a no-op, which is exactly the
/// byte-stability we want; the pass remains as a defense and for evidence
/// parity if `preserve_order` is ever enabled.
pub fn sort_schema_keys_recursive(value: &mut Value) -> bool {
    match value {
        Value::Object(map) => {
            let mut changed = false;
            for (_key, child) in map.iter_mut() {
                changed |= sort_schema_keys_recursive(child);
            }
            // `serde_json::Map` does not expose an in-place key reorder;
            // rebuild it in sorted order.  With the default BTreeMap backing
            // this is a no-op clone; with `preserve_order` it canonicalizes.
            let mut entries: Vec<(String, Value)> = map
                .iter()
                .map(|(key, child)| (key.clone(), child.clone()))
                .collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let sorted: Vec<String> = entries.iter().map(|(key, _)| key.clone()).collect();
            let before: Vec<String> = map.keys().cloned().collect();
            if before != sorted {
                map.clear();
                for (key, child) in entries {
                    map.insert(key, child);
                }
                changed = true;
            }
            changed
        }
        Value::Array(items) => {
            let mut changed = false;
            for item in items.iter_mut() {
                changed |= sort_schema_keys_recursive(item);
            }
            changed
        }
        _ => false,
    }
}

/// Normalize every tool definition in a request body (PR-E1 + PR-E2).
///
/// Handles the three shapes d-token routes:
/// - Anthropic `/v1/messages`: `body.tools[*]` with `input_schema`.
/// - OpenAI Chat Completions: `body.tools[*].function.parameters`.
/// - OpenAI legacy: `body.functions[*].parameters`.
///
/// The body is mutated in place; the outcome reports what moved.  The
/// `tools` key serves both Anthropic and OpenAI Chat shapes — the sort key
/// reads `tool["name"]` first, then `tool["function"]["name"]`, so one
/// pass normalizes both.
pub fn normalize_tool_definitions(body: &mut Value) -> ToolSchemaNormalizeOutcome {
    let mut outcome = ToolSchemaNormalizeOutcome {
        tools_considered: 0,
        tools_reordered: false,
        schemas_key_sorted: false,
        tools_deduped: 0,
        changed: false,
    };
    let Value::Object(map) = body else {
        return outcome;
    };

    // `tools[]` — Anthropic (`input_schema`) and OpenAI Chat
    // (`function.parameters`) share this key.
    if let Some(tools) = map.get_mut("tools").and_then(Value::as_array_mut) {
        outcome.tools_considered += tools.len();
        // PR-E4 dedup first (never reorders, so cache breakpoints are safe),
        // then PR-E1 sort and PR-E2 key sorting.
        outcome.tools_deduped += dedupe_tools(tools);
        if !any_tool_has_cache_control(tools) {
            outcome.tools_reordered |= sort_tools_deterministically(tools);
        }
        for tool in tools.iter_mut() {
            if let Some(schema) = tool.get_mut("input_schema") {
                outcome.schemas_key_sorted |= sort_schema_keys_recursive(schema);
            }
            if let Some(parameters) = tool
                .get_mut("function")
                .and_then(|function| function.get_mut("parameters"))
            {
                outcome.schemas_key_sorted |= sort_schema_keys_recursive(parameters);
            }
        }
    }

    // OpenAI legacy shape: `functions[*].parameters`.
    if let Some(functions) = map.get_mut("functions").and_then(Value::as_array_mut) {
        outcome.tools_considered += functions.len();
        outcome.tools_deduped += dedupe_tools(functions);
        if !any_tool_has_cache_control(functions) {
            outcome.tools_reordered |= sort_tools_deterministically(functions);
        }
        for function in functions.iter_mut() {
            if let Some(parameters) = function.get_mut("parameters") {
                outcome.schemas_key_sorted |= sort_schema_keys_recursive(parameters);
            }
        }
    }

    outcome.changed = outcome.tools_reordered
        || outcome.schemas_key_sorted
        || outcome.tools_deduped > 0;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_anthropic_tools_alphabetically_and_reports_reorder() {
        let mut tools = vec![
            json!({"name": "zeta", "input_schema": {"type": "object"}}),
            json!({"name": "alpha", "input_schema": {"type": "object"}}),
            json!({"name": "mid", "input_schema": {"type": "object"}}),
        ];
        let changed = sort_tools_deterministically(&mut tools);
        assert!(changed);
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["alpha", "mid", "zeta"]);
    }

    #[test]
    fn idempotent_resort_is_no_change() {
        let mut tools = vec![
            json!({"name": "a"}),
            json!({"name": "b"}),
            json!({"name": "c"}),
        ];
        assert!(!sort_tools_deterministically(&mut tools));
    }

    #[test]
    fn openai_function_name_is_the_sort_key() {
        let mut tools = vec![
            json!({"type": "function", "function": {"name": "b", "parameters": {}}}),
            json!({"type": "function", "function": {"name": "a", "parameters": {}}}),
        ];
        assert!(sort_tools_deterministically(&mut tools));
        assert_eq!(tools[0]["function"]["name"].as_str().unwrap(), "a");
    }

    #[test]
    fn cache_control_marker_blocks_reorder() {
        let tools = vec![
            json!({"name": "b", "cache_control": {"type": "ephemeral"}, "input_schema": {}}),
            json!({"name": "a", "input_schema": {}}),
        ];
        assert!(any_tool_has_cache_control(&tools));
        let mut body = json!({"tools": tools});
        let outcome = normalize_tool_definitions(&mut body);
        assert!(!outcome.tools_reordered);
        assert_eq!(body["tools"][0]["name"].as_str().unwrap(), "b");
    }

    #[test]
    fn schema_keys_are_recursively_sorted_arrays_keep_order() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "z": {"type": "string"},
                "a": {"type": "integer"}
            },
            "oneOf": [
                {"required": ["b"], "title": "second"},
                {"required": ["a"], "title": "first"}
            ]
        });
        sort_schema_keys_recursive(&mut schema);
        // Object keys sorted at every level.
        let properties = schema["properties"].as_object().unwrap();
        let keys: Vec<&String> = properties.keys().collect();
        assert_eq!(keys, vec!["a", "z"]);
        // oneOf array order preserved.
        assert_eq!(schema["oneOf"][0]["title"].as_str().unwrap(), "second");
        assert_eq!(schema["oneOf"][1]["title"].as_str().unwrap(), "first");
    }

    #[test]
    fn normalize_handles_all_three_shapes_and_reports_changed() {
        let mut body = json!({
            "tools": [
                {"type": "function", "function": {"name": "b", "parameters": {"properties": {"z": {}, "a": {}}}}},
                {"type": "function", "function": {"name": "a", "parameters": {"type": "object"}}}
            ],
            "functions": [
                {"name": "legacy-b", "parameters": {}},
                {"name": "legacy-a", "parameters": {}}
            ]
        });
        let outcome = normalize_tool_definitions(&mut body);
        assert!(outcome.tools_reordered);
        assert!(outcome.changed);
        assert_eq!(outcome.tools_considered, 4);
        assert_eq!(body["tools"][0]["function"]["name"].as_str().unwrap(), "a");
        assert_eq!(body["functions"][0]["name"].as_str().unwrap(), "legacy-a");
    }

    #[test]
    fn already_normalized_body_is_a_noop() {
        let mut body = json!({
            "tools": [
                {"name": "a", "input_schema": {"type": "object"}},
                {"name": "b", "input_schema": {"type": "object"}}
            ]
        });
        let first = normalize_tool_definitions(&mut body);
        assert!(!first.changed);
        let snapshot = body.clone();
        let second = normalize_tool_definitions(&mut body);
        assert!(!second.changed);
        assert_eq!(body, snapshot);
    }

    #[test]
    fn non_object_body_is_a_noop() {
        let mut body = json!([1, 2, 3]);
        let outcome = normalize_tool_definitions(&mut body);
        assert_eq!(outcome.tools_considered, 0);
        assert!(!outcome.changed);
    }

    #[test]
    fn dedupe_drops_byte_identical_tool_definitions_losslessly() {
        let mut tools = vec![
            json!({"name": "alpha", "input_schema": {"type": "object", "properties": {"a": {"type": "string"}}}}),
            json!({"name": "alpha", "input_schema": {"type": "object", "properties": {"a": {"type": "string"}}}}),
            json!({"name": "beta", "input_schema": {"type": "object"}}),
        ];
        let removed = dedupe_tools(&mut tools);
        assert_eq!(removed, 1);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"].as_str().unwrap(), "alpha");
        assert_eq!(tools[1]["name"].as_str().unwrap(), "beta");
    }

    #[test]
    fn dedupe_keeps_distinct_schemas_with_same_name() {
        // Same tool name but different schema is NOT a duplicate.
        let mut tools = vec![
            json!({"name": "alpha", "input_schema": {"type": "object", "properties": {"a": {}}}}),
            json!({"name": "alpha", "input_schema": {"type": "object", "properties": {"b": {}}}}),
        ];
        let removed = dedupe_tools(&mut tools);
        assert_eq!(removed, 0);
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn normalize_reports_deduped_count_and_changed() {
        let mut body = json!({
            "tools": [
                {"name": "a", "input_schema": {"type": "object"}},
                {"name": "a", "input_schema": {"type": "object"}},
                {"name": "b", "input_schema": {"type": "object"}}
            ]
        });
        let outcome = normalize_tool_definitions(&mut body);
        assert_eq!(outcome.tools_deduped, 1);
        assert!(outcome.changed);
        assert_eq!(body["tools"].as_array().unwrap().len(), 2);
    }
}
