use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::ApprovedHybridTarget;

use super::renderer::RenderedArtifact;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchError {
    EmptyPatchSet,
    MissingTarget,
    PreconditionMismatch,
    TargetIsNotString,
    MissingRenderedPage,
    UnsupportedTargetPath,
    PatchWriteFailed,
}

#[derive(Clone)]
pub struct OpticalPatchOperation {
    node_id: String,
    path: String,
    precondition_digest: String,
    artifact: RenderedArtifact,
}

impl std::fmt::Debug for OpticalPatchOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpticalPatchOperation")
            .field("node_id", &self.node_id)
            .field("path", &self.path)
            .field("precondition_digest", &self.precondition_digest)
            .field("page_count", &self.artifact.pages().len())
            .field("source_digest", &self.artifact.source_digest())
            .finish()
    }
}

impl OpticalPatchOperation {
    pub(crate) fn from_rendered(
        target: &ApprovedHybridTarget,
        artifact: RenderedArtifact,
    ) -> Result<Self, PatchError> {
        if artifact.pages().is_empty() {
            return Err(PatchError::MissingRenderedPage);
        }
        if !target.patch_path().starts_with("/messages/")
            || !target.patch_path().ends_with("/content")
        {
            return Err(PatchError::UnsupportedTargetPath);
        }
        Ok(Self {
            node_id: target.node_id().to_string(),
            path: target.patch_path().to_string(),
            precondition_digest: target.precondition_digest().to_string(),
            artifact,
        })
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn artifact(&self) -> &RenderedArtifact {
        &self.artifact
    }
}

#[derive(Debug, Clone)]
pub struct OpticalPatchSet {
    operations: Vec<OpticalPatchOperation>,
}

impl OpticalPatchSet {
    pub(crate) fn from_operations(
        operations: Vec<OpticalPatchOperation>,
    ) -> Result<Self, PatchError> {
        if operations.is_empty() {
            return Err(PatchError::EmptyPatchSet);
        }
        Ok(Self { operations })
    }

    pub fn operations(&self) -> &[OpticalPatchOperation] {
        &self.operations
    }

    pub fn image_count(&self) -> usize {
        self.operations
            .iter()
            .map(|operation| operation.artifact.pages().len())
            .sum()
    }

    pub fn png_bytes(&self) -> u64 {
        self.operations
            .iter()
            .map(|operation| operation.artifact.png_bytes())
            .sum()
    }

    pub fn base64_bytes(&self) -> u64 {
        self.operations
            .iter()
            .map(|operation| operation.artifact.base64_bytes())
            .sum()
    }

    pub(crate) fn apply(&self, mut body: Value) -> Result<PatchedRequest, PatchError> {
        for operation in &self.operations {
            let current = body
                .pointer(operation.path())
                .ok_or(PatchError::MissingTarget)?;
            if json_value_digest(current) != operation.precondition_digest {
                return Err(PatchError::PreconditionMismatch);
            }
            if !current.is_string() {
                return Err(PatchError::TargetIsNotString);
            }

            let mut content = vec![json!({
                "type": "text",
                "text": format!(
                    "[d-token optical capsule: {} deterministic PNG page(s); low-risk historical tool output only]",
                    operation.artifact.pages().len()
                )
            })];
            content.extend(operation.artifact.pages().iter().map(|page| {
                json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": page.base64()
                    }
                })
            }));
            let slot = body
                .pointer_mut(operation.path())
                .ok_or(PatchError::PatchWriteFailed)?;
            *slot = Value::Array(content);
        }

        Ok(PatchedRequest {
            body,
            patch_set: self.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PatchedRequest {
    body: Value,
    patch_set: OpticalPatchSet,
}

impl PatchedRequest {
    pub(crate) fn body(&self) -> &Value {
        &self.body
    }

    pub(crate) fn patch_set(&self) -> &OpticalPatchSet {
        &self.patch_set
    }

    #[cfg(test)]
    pub(crate) fn body_mut(&mut self) -> &mut Value {
        &mut self.body
    }
}

pub(crate) fn json_value_digest(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).unwrap_or_default())
    )
}
