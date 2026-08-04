// SchemaSlimPlanner：分离 validator 结构与 annotation 文本。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §10。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaAnnotationPolicy {
    KeepText,
    ShadowOnlyCandidate,
    FullExperimentalCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaWalkContext {
    SchemaNode,
    PropertiesMap,
    PatternPropertiesMap,
    DefinitionsMap,
    DefsMap,
    RequiredArray,
    EnumArray,
    ConstValue,
    RefValue,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct SchemaAnnotationBlock {
    pub key: String,
    pub chars: usize,
    pub policy: SchemaAnnotationPolicy,
}

#[derive(Debug, Clone)]
pub struct SchemaSlimSkip {
    pub key: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct SchemaSlimPlan {
    pub tool_count: usize,
    pub schemas_considered: usize,
    pub schemas_slimmed: usize,
    pub annotations_moved_chars: usize,
    pub validator_chars_before: usize,
    pub validator_chars_after: usize,
    pub annotation_blocks: Vec<SchemaAnnotationBlock>,
    pub skipped: Vec<SchemaSlimSkip>,
}

const ANNOTATION_KEYS: &[&str] = &[
    "description",
    "title",
    "examples",
    "default",
    "$schema",
    "$id",
    "$comment",
];
const VALIDATOR_KEYS: &[&str] = &[
    "required",
    "enum",
    "const",
    "type",
    "$ref",
    "properties",
    "patternProperties",
    "definitions",
    "$defs",
    "items",
    "additionalProperties",
    "oneOf",
    "anyOf",
    "allOf",
];

fn is_annotation_key(key: &str) -> bool {
    ANNOTATION_KEYS.contains(&key)
}
fn is_validator_key(key: &str) -> bool {
    VALIDATOR_KEYS.contains(&key)
}

pub fn plan_schema_slim(schema: &serde_json::Value) -> SchemaSlimPlan {
    let validator_chars_before = serde_json::to_string(schema)
        .unwrap_or_default()
        .chars()
        .count();
    let mut annotation_blocks = Vec::new();
    let mut skipped = Vec::new();
    let mut annotations_moved_chars = 0usize;
    walk_schema(
        schema,
        SchemaWalkContext::SchemaNode,
        &mut annotation_blocks,
        &mut skipped,
        &mut annotations_moved_chars,
    );
    let validator_chars_after = validator_chars_before.saturating_sub(annotations_moved_chars);
    let schemas_slimmed = if !annotation_blocks.is_empty() { 1 } else { 0 };
    SchemaSlimPlan {
        tool_count: 0,
        schemas_considered: 1,
        schemas_slimmed,
        annotations_moved_chars,
        validator_chars_before,
        validator_chars_after,
        annotation_blocks,
        skipped,
    }
}

fn walk_schema(
    value: &serde_json::Value,
    _ctx: SchemaWalkContext,
    annotation_blocks: &mut Vec<SchemaAnnotationBlock>,
    skipped: &mut Vec<SchemaSlimSkip>,
    annotations_moved_chars: &mut usize,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                if is_annotation_key(key) {
                    let chars = serde_json::to_string(val)
                        .unwrap_or_default()
                        .chars()
                        .count();
                    annotation_blocks.push(SchemaAnnotationBlock {
                        key: key.clone(),
                        chars,
                        policy: SchemaAnnotationPolicy::ShadowOnlyCandidate,
                    });
                    *annotations_moved_chars += chars;
                } else if is_validator_key(key) {
                    walk_schema(
                        val,
                        SchemaWalkContext::SchemaNode,
                        annotation_blocks,
                        skipped,
                        annotations_moved_chars,
                    );
                } else {
                    skipped.push(SchemaSlimSkip {
                        key: key.clone(),
                        reason: "unknown_context_preserved".to_string(),
                    });
                    walk_schema(
                        val,
                        SchemaWalkContext::Unknown,
                        annotation_blocks,
                        skipped,
                        annotations_moved_chars,
                    );
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                walk_schema(
                    item,
                    SchemaWalkContext::Unknown,
                    annotation_blocks,
                    skipped,
                    annotations_moved_chars,
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn property_named_description_is_preserved() {
        let schema = json!({"type":"object","properties":{"name":{"type":"string","description":"user name"}},"required":["name"]});
        let plan = plan_schema_slim(&schema);
        assert!(plan
            .annotation_blocks
            .iter()
            .any(|b| b.key == "description"));
    }
    #[test]
    fn required_remains_valid() {
        let schema = json!({"type":"object","required":["id","name"]});
        let plan = plan_schema_slim(&schema);
        assert!(!plan.annotation_blocks.iter().any(|b| b.key == "required"));
    }
    #[test]
    fn enum_const_type_ref_preserved() {
        let schema = json!({"enum":["a","b"],"const":"x","type":"string","$ref":"#/$defs/x"});
        let plan = plan_schema_slim(&schema);
        assert!(!plan
            .annotation_blocks
            .iter()
            .any(|b| b.key == "enum" || b.key == "const" || b.key == "type" || b.key == "$ref"));
    }
    #[test]
    fn unknown_schema_context_preserves_text() {
        let schema = json!({"custom_field":"value"});
        let plan = plan_schema_slim(&schema);
        assert!(plan.skipped.iter().any(|s| s.key == "custom_field"));
    }
}
