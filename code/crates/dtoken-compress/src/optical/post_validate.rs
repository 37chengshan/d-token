use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{lower_hybrid_context, ApprovedHybridAction, ResolvedOptimizationContext};

use super::patcher::{OpticalPatchSet, PatchedRequest};
use super::renderer::{validate_png, RenderBytePermit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostValidationError {
    OriginalContextInvalid,
    PatchedContextInvalid,
    MutationOutsideApprovedTargets,
    MissingPatchedTarget,
    InvalidPatchedContent,
    PageCountMismatch,
    InvalidImageEnvelope,
    InvalidBase64,
    ArtifactDigestMismatch,
    InvalidPng,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostValidationReport {
    pub approved_patch_count: usize,
    pub image_count: usize,
    pub decoded_png_bytes: u64,
    pub protected_structure_unchanged: bool,
    pub artifact_evidence_derived: bool,
}

pub struct ValidatedActiveRequest {
    body: Value,
    report: PostValidationReport,
    patch_set: OpticalPatchSet,
    body_digest: String,
    _memory_lease: Option<RenderBytePermit>,
}

impl std::fmt::Debug for ValidatedActiveRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedActiveRequest")
            .field("report", &self.report)
            .field("body_digest", &self.body_digest)
            .finish()
    }
}

impl ValidatedActiveRequest {
    pub fn body(&self) -> &Value {
        &self.body
    }

    pub fn into_body(self) -> Value {
        self.body
    }

    pub fn report(&self) -> &PostValidationReport {
        &self.report
    }

    pub fn patch_set(&self) -> &OpticalPatchSet {
        &self.patch_set
    }

    pub fn body_digest(&self) -> &str {
        &self.body_digest
    }

    pub(crate) fn with_memory_lease(mut self, lease: RenderBytePermit) -> Self {
        self._memory_lease = Some(lease);
        self
    }
}

pub(crate) fn validate_patched_request(
    context: &ResolvedOptimizationContext,
    original: &Value,
    patched: PatchedRequest,
) -> Result<ValidatedActiveRequest, PostValidationError> {
    let original_ir = lower_hybrid_context(context, original)
        .ok_or(PostValidationError::OriginalContextInvalid)?;
    if original_ir.approved_text_targets().len() < patched.patch_set().operations().len() {
        return Err(PostValidationError::OriginalContextInvalid);
    }

    let mut original_scrubbed = original.clone();
    let mut patched_scrubbed = patched.body().clone();
    let mut decoded_png_bytes = 0u64;
    let mut image_count = 0usize;

    for operation in patched.patch_set().operations() {
        let approved_target = original_ir
            .approved_text_targets()
            .iter()
            .find(|target| {
                target.node_id() == operation.node_id()
                    && target.patch_path() == operation.path()
                    && target.action() == ApprovedHybridAction::OpticalImage
            })
            .ok_or(PostValidationError::OriginalContextInvalid)?;
        let original_value = original
            .pointer(approved_target.patch_path())
            .ok_or(PostValidationError::OriginalContextInvalid)?;
        if super::patcher::json_value_digest(original_value)
            != approved_target.precondition_digest()
        {
            return Err(PostValidationError::OriginalContextInvalid);
        }
        let original_slot = original_scrubbed
            .pointer_mut(operation.path())
            .ok_or(PostValidationError::MissingPatchedTarget)?;
        *original_slot = json!({"dtoken_approved_patch": operation.node_id()});

        let patched_value = patched
            .body()
            .pointer(operation.path())
            .ok_or(PostValidationError::MissingPatchedTarget)?;
        let content = patched_value
            .as_array()
            .ok_or(PostValidationError::InvalidPatchedContent)?;
        let expected_pointer = operation.pointer().text();
        let expected_guard = operation.history_guard().text();
        if content.len() != operation.expected_content_len()
            || content
                .first()
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str)
                != Some("text")
            || content
                .first()
                .and_then(|item| item.get("text"))
                .and_then(Value::as_str)
                != Some(expected_pointer.as_str())
            || content
                .last()
                .and_then(|item| item.get("text"))
                .and_then(Value::as_str)
                != Some(expected_guard.as_str())
        {
            return Err(PostValidationError::InvalidPatchedContent);
        }
        let image_end = operation.artifact().pages().len().saturating_add(1);
        let images = content
            .get(1..image_end)
            .ok_or(PostValidationError::InvalidPatchedContent)?;
        if images
            .iter()
            .any(|item| item.get("type").and_then(Value::as_str) != Some("image"))
        {
            return Err(PostValidationError::InvalidPatchedContent);
        }
        if images.len() != operation.artifact().pages().len() {
            return Err(PostValidationError::PageCountMismatch);
        }
        if let Some(expected_factsheet) = operation.exact_factsheet_text() {
            let factsheet = content
                .get(image_end)
                .ok_or(PostValidationError::InvalidPatchedContent)?;
            if factsheet.get("type").and_then(Value::as_str) != Some("text")
                || factsheet.get("text").and_then(Value::as_str)
                    != Some(expected_factsheet.as_str())
            {
                return Err(PostValidationError::InvalidPatchedContent);
            }
        }
        for (image, page) in images.iter().zip(operation.artifact().pages()) {
            let source = image
                .get("source")
                .and_then(Value::as_object)
                .ok_or(PostValidationError::InvalidImageEnvelope)?;
            if source.get("type").and_then(Value::as_str) != Some("base64")
                || source.get("media_type").and_then(Value::as_str) != Some("image/png")
            {
                return Err(PostValidationError::InvalidImageEnvelope);
            }
            let encoded = source
                .get("data")
                .and_then(Value::as_str)
                .ok_or(PostValidationError::InvalidImageEnvelope)?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| PostValidationError::InvalidBase64)?;
            if sha256_hex(&decoded) != page.digest() || decoded != page.png_bytes() {
                return Err(PostValidationError::ArtifactDigestMismatch);
            }
            validate_png(&decoded, page.width(), page.height())
                .map_err(|_| PostValidationError::InvalidPng)?;
            decoded_png_bytes =
                decoded_png_bytes.saturating_add(u64::try_from(decoded.len()).unwrap_or(u64::MAX));
            image_count = image_count.saturating_add(1);
        }

        let patched_slot = patched_scrubbed
            .pointer_mut(operation.path())
            .ok_or(PostValidationError::MissingPatchedTarget)?;
        *patched_slot = json!({"dtoken_approved_patch": operation.node_id()});
    }

    if original_scrubbed != patched_scrubbed {
        return Err(PostValidationError::MutationOutsideApprovedTargets);
    }
    if patched
        .body()
        .get("messages")
        .and_then(Value::as_array)
        .is_none()
    {
        return Err(PostValidationError::PatchedContextInvalid);
    }

    let body_digest = sha256_hex(&serde_json::to_vec(patched.body()).unwrap_or_default());
    let report = PostValidationReport {
        approved_patch_count: patched.patch_set().operations().len(),
        image_count,
        decoded_png_bytes,
        protected_structure_unchanged: true,
        artifact_evidence_derived: true,
    };
    let (body, patch_set) = patched.into_parts();
    Ok(ValidatedActiveRequest {
        body,
        report,
        patch_set,
        body_digest,
        _memory_lease: None,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optical::capability::experimental_anthropic_active_capability;
    use crate::optical::{
        geometry_for_capability, DeterministicPngRenderer, OpticalPatchOperation,
        RenderCancellation, RenderLimits,
    };
    use dtoken_types::{OpticalEndpoint, OpticalGlobalPolicy, Protocol};
    use std::time::Duration;

    fn context() -> ResolvedOptimizationContext {
        ResolvedOptimizationContext {
            route_id: "route-main".to_string(),
            provider_id: "anthropic-main".to_string(),
            model: "claude-fable-5".to_string(),
            input_protocol: Protocol::Anthropic,
            output_protocol: Protocol::Anthropic,
            endpoint: OpticalEndpoint::AnthropicMessages,
        }
    }

    fn request_body() -> Value {
        let log = "INFO historical operation completed successfully\n".repeat(300);
        let mut messages = vec![
            json!({"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"read","input":{"path":"/tmp/a"}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":log}]}),
        ];
        for index in 0..6 {
            messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":format!("recent-{index}"),"name":"read","input":{}}]}));
            messages.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":format!("recent-{index}"),"content":"recent"}]}));
        }
        messages.push(json!({"role":"user","content":[{"type":"text","text":"current turn"}]}));
        json!({
            "model": "claude-fable-5",
            "system": "protected system instruction",
            "tools": [{"name":"read","input_schema":{"type":"object"}}],
            "messages": messages
        })
    }

    fn patched_fixture(original: &Value) -> PatchedRequest {
        let context = context();
        let ir = lower_hybrid_context(&context, original).expect("valid Anthropic context");
        let target = ir
            .approved_text_targets()
            .first()
            .expect("historical closed tool result is approved");
        assert_eq!(target.action(), ApprovedHybridAction::OpticalImage);
        let text = original
            .pointer(target.patch_path())
            .and_then(Value::as_str)
            .expect("approved target is text");
        let capability = experimental_anthropic_active_capability("claude-fable-5");
        let geometry = geometry_for_capability(&capability).expect("fixture geometry");
        let renderer = DeterministicPngRenderer::new(1, Duration::from_millis(50));
        let limits = RenderLimits::from_policy(&OpticalGlobalPolicy {
            enabled: true,
            // These tests exercise patch validation; rendering deadlines have
            // their own tests and should not make fixture setup load-dependent.
            max_render_time_ms: 5_000,
            ..Default::default()
        });
        let artifact = renderer
            .render(text, &geometry, &limits, &RenderCancellation::default())
            .expect("fixture renders");
        let operation = OpticalPatchOperation::from_rendered(target, artifact, text)
            .expect("approved target accepts rendered artifact");
        OpticalPatchSet::from_operations(vec![operation])
            .expect("non-empty patch set")
            .apply(original.clone())
            .expect("patch precondition holds")
    }

    #[test]
    fn validator_accepts_only_approved_path_mutation_and_derives_artifact_evidence() {
        let original = request_body();
        let patched = patched_fixture(&original);
        let validated = validate_patched_request(&context(), &original, patched)
            .expect("approved optical patch validates");

        assert_eq!(validated.report().approved_patch_count, 1);
        assert!(validated.report().image_count > 0);
        assert!(validated.report().decoded_png_bytes > 0);
        assert!(validated.report().protected_structure_unchanged);
        assert!(validated.report().artifact_evidence_derived);
        assert_eq!(validated.body()["model"], original["model"]);
        assert_eq!(validated.body()["system"], original["system"]);
        assert_eq!(validated.body()["tools"], original["tools"]);
        assert_eq!(validated.body()["messages"][0], original["messages"][0]);
        assert!(validated.body()["messages"][1]["content"][0]["content"][0]
            .get("type")
            .is_some_and(|value| value == "text"));
        assert!(validated.body()["messages"][1]["content"][0]["content"][1]
            .get("type")
            .is_some_and(|value| value == "image"));
        let content = validated.body()["messages"][1]["content"][0]["content"]
            .as_array()
            .expect("patched content array");
        assert!(content
            .first()
            .and_then(|item| item.get("text"))
            .and_then(Value::as_str)
            .is_some_and(|text| text.starts_with("[d-token rendered context pointer]")));
        assert!(!content.iter().any(|item| item
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.starts_with("[d-token exact-value references"))));
        assert!(content
            .last()
            .and_then(|item| item.get("text"))
            .and_then(Value::as_str)
            .is_some_and(|text| text.starts_with("[d-token history boundary]")));
    }

    #[test]
    fn validator_rejects_any_mutation_outside_the_sealed_patch_path() {
        let original = request_body();
        let mut patched = patched_fixture(&original);
        patched.body_mut()["system"] = json!("tampered system instruction");

        assert!(matches!(
            validate_patched_request(&context(), &original, patched),
            Err(PostValidationError::MutationOutsideApprovedTargets)
        ));
    }

    #[test]
    fn validator_rejects_tampered_pointer_inside_approved_path() {
        let original = request_body();
        let mut patched = patched_fixture(&original);
        let content = patched.body_mut()["messages"][1]["content"][0]["content"]
            .as_array_mut()
            .expect("patched content array");
        content[0]["text"] = json!("tampered");
        assert!(matches!(
            validate_patched_request(&context(), &original, patched),
            Err(PostValidationError::InvalidPatchedContent)
        ));
    }
}
