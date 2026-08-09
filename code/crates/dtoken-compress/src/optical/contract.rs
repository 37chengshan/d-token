use sha2::{Digest, Sha256};

use super::secret::SECRET_CLASSIFIER_REVISION;

pub const OPTICAL_TRANSFORM_CONTRACT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpticalTransformContractV2 {
    pub renderer_contract_digest: String,
    pub secret_classifier_revision: &'static str,
    pub recent_tail_revision: &'static str,
    pub content_router_revision: &'static str,
    pub factsheet_revision: &'static str,
    pub pointer_guard_revision: &'static str,
    pub selector_revision: &'static str,
    pub patcher_revision: &'static str,
    pub post_validator_revision: &'static str,
    pub tokenizer_revision: String,
    pub pricing_revision: String,
    pub cache_policy_revision: String,
}

impl OpticalTransformContractV2 {
    pub fn current(renderer_contract_digest: impl Into<String>) -> Self {
        Self {
            renderer_contract_digest: renderer_contract_digest.into(),
            secret_classifier_revision: SECRET_CLASSIFIER_REVISION,
            recent_tail_revision: "recent-six-closed-tool-pairs-v3",
            content_router_revision: "safe-content-router-v2-diff-preserve",
            factsheet_revision: "typed-exact-factsheet-v3",
            pointer_guard_revision: "historical-pointer-guard-v2",
            selector_revision: "hybrid-selector-v3-single-page-active",
            patcher_revision: "sealed-anthropic-patcher-v2",
            post_validator_revision: "artifact-post-validator-v2",
            tokenizer_revision: "unbound-tokenizer".to_string(),
            pricing_revision: "unbound-pricing".to_string(),
            cache_policy_revision: "cold-warm-unknown-max-burn-v1".to_string(),
        }
    }

    pub fn current_with_economics(
        renderer_contract_digest: impl Into<String>,
        tokenizer_revision: impl Into<String>,
        pricing_revision: impl Into<String>,
        cache_policy_revision: impl Into<String>,
    ) -> Self {
        Self {
            tokenizer_revision: tokenizer_revision.into(),
            pricing_revision: pricing_revision.into(),
            cache_policy_revision: cache_policy_revision.into(),
            ..Self::current(renderer_contract_digest)
        }
    }

    pub fn digest(&self) -> String {
        let payload = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            OPTICAL_TRANSFORM_CONTRACT_SCHEMA_VERSION,
            self.renderer_contract_digest,
            self.secret_classifier_revision,
            self.recent_tail_revision,
            self.content_router_revision,
            self.factsheet_revision,
            self.pointer_guard_revision,
            self.selector_revision,
            self.patcher_revision,
            self.post_validator_revision,
            self.tokenizer_revision,
            self.pricing_revision,
            self.cache_policy_revision,
        );
        format!("{:x}", Sha256::digest(payload.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_digest_is_stable_and_renderer_bound() {
        let first = OpticalTransformContractV2::current("renderer-a").digest();
        assert_eq!(
            first,
            OpticalTransformContractV2::current("renderer-a").digest()
        );
        assert_ne!(
            first,
            OpticalTransformContractV2::current("renderer-b").digest()
        );
        assert_ne!(
            first,
            OpticalTransformContractV2::current_with_economics(
                "renderer-a",
                "tokenizer-v1",
                "pricing-v1",
                "cache-v1",
            )
            .digest()
        );
    }
}
