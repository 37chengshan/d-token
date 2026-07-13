use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{lower_hybrid_context, ResolvedOptimizationContext};

use super::patcher::{OpticalPatchSet, PatchedRequest};
use super::renderer::validate_png;

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

#[derive(Clone)]
pub struct ValidatedActiveRequest {
    body: Value,
    report: PostValidationReport,
    patch_set: OpticalPatchSet,
    body_digest: String,
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
        let expected_sidecar = format!(
            "[d-token optical capsule: {} deterministic PNG page(s); low-risk historical tool output only]",
            operation.artifact().pages().len()
        );
        if content.len() != operation.artifact().pages().len().saturating_add(1)
            || content
                .first()
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str)
                != Some("text")
            || content
                .first()
                .and_then(|item| item.get("text"))
                .and_then(Value::as_str)
                != Some(expected_sidecar.as_str())
        {
            return Err(PostValidationError::InvalidPatchedContent);
        }
        let images = content
            .iter()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("image"))
            .collect::<Vec<_>>();
        if images.len() != operation.artifact().pages().len() {
            return Err(PostValidationError::PageCountMismatch);
        }
        for (image, page) in images.into_iter().zip(operation.artifact().pages()) {
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
    Ok(ValidatedActiveRequest {
        body: patched.body().clone(),
        report,
        patch_set: patched.patch_set().clone(),
        body_digest,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optical::{
        experimental_anthropic_active_capability, geometry_for_capability,
        DeterministicPngRenderer, OpticalPatchOperation, RenderCancellation, RenderLimits,
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
        let log = "INFO historical operation completed successfully\n".repeat(600);
        json!({
            "model": "claude-fable-5",
            "system": "protected system instruction",
            "tools": [{"name":"read","input_schema":{"type":"object"}}],
            "messages": [
                {"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"read","input":{"path":"/tmp/a"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":log}]},
                {"role":"assistant","content":[{"type":"text","text":"done"}]},
                {"role":"user","content":[{"type":"text","text":"next task"}]},
                {"role":"assistant","content":[{"type":"text","text":"working"}]},
                {"role":"user","content":[{"type":"text","text":"current turn"}]}
            ]
        })
    }

    fn patched_fixture(original: &Value) -> PatchedRequest {
        let context = context();
        let ir = lower_hybrid_context(&context, original).expect("valid Anthropic context");
        let target = ir
            .approved_text_targets()
            .first()
            .expect("historical closed tool result is approved");
        let text = original
            .pointer(target.patch_path())
            .and_then(Value::as_str)
            .expect("approved target is text");
        let capability = experimental_anthropic_active_capability("claude-fable-5");
        let geometry = geometry_for_capability(&capability).expect("fixture geometry");
        let renderer = DeterministicPngRenderer::new(1, Duration::from_millis(50));
        let limits = RenderLimits::from_policy(&OpticalGlobalPolicy {
            enabled: true,
            ..Default::default()
        });
        let artifact = renderer
            .render(text, &geometry, &limits, &RenderCancellation::default())
            .expect("fixture renders");
        let operation = OpticalPatchOperation::from_rendered(target, artifact)
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
}
