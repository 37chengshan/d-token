//! Rust-owned compressor registry.
//!
//! Headroom v0.33 introduced a name-addressable compressor seam.  d-token
//! keeps that seam declarative and fail-closed: only codecs compiled into the
//! audited runtime can be selected, and third-party code is never loaded into
//! the request process.  A relay can still select a route and policy explicitly
//! and receive the same deterministic built-in codec decision.

use crate::content_router::{route_content_with_budget, CodecKind, ContentKind, RoutingBudget};
use dtoken_types::CompressionPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressorDescriptor {
    pub id: &'static str,
    pub content_kind: ContentKind,
    pub codec: CodecKind,
    pub revision: &'static str,
    pub lossless: bool,
    pub cost_tier: &'static str,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressorSelection {
    pub descriptor: CompressorDescriptor,
    pub route_reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProductionCompressorRegistry;

const DESCRIPTORS: [CompressorDescriptor; 9] = [
    CompressorDescriptor {
        id: "json-codec",
        content_kind: ContentKind::Json,
        codec: CodecKind::JsonMinify,
        revision: "json_minify_v1",
        lossless: true,
        cost_tier: "fast",
        active: true,
    },
    CompressorDescriptor {
        id: "log-codec",
        content_kind: ContentKind::Log,
        codec: CodecKind::LogFold,
        revision: "log_fold_v1",
        lossless: true,
        cost_tier: "fast",
        active: true,
    },
    CompressorDescriptor {
        id: "search-codec",
        content_kind: ContentKind::SearchResults,
        codec: CodecKind::SearchFold,
        revision: "search_fold_v1",
        lossless: true,
        cost_tier: "fast",
        active: true,
    },
    CompressorDescriptor {
        id: "build-codec",
        content_kind: ContentKind::BuildOutput,
        codec: CodecKind::BuildFold,
        revision: "build_fold_v1",
        lossless: true,
        cost_tier: "fast",
        active: true,
    },
    CompressorDescriptor {
        id: "diff-codec",
        content_kind: ContentKind::Diff,
        codec: CodecKind::DiffFold,
        revision: "diff_fold_v1",
        lossless: true,
        cost_tier: "fast",
        active: true,
    },
    CompressorDescriptor {
        id: "text-codec",
        content_kind: ContentKind::Text,
        codec: CodecKind::TextRepeatFold,
        revision: "text_repeat_fold_v1",
        lossless: true,
        cost_tier: "fast",
        active: true,
    },
    CompressorDescriptor {
        id: "html-codec",
        content_kind: ContentKind::Html,
        codec: CodecKind::HtmlRepeatFold,
        revision: "html_repeat_fold_v1",
        lossless: true,
        cost_tier: "fast",
        active: true,
    },
    CompressorDescriptor {
        id: "config-codec",
        content_kind: ContentKind::Config,
        codec: CodecKind::ConfigFold,
        revision: "config_repeat_fold_v1",
        lossless: true,
        cost_tier: "fast",
        active: true,
    },
    CompressorDescriptor {
        id: "code-compressor",
        content_kind: ContentKind::Code,
        codec: CodecKind::CodeFenceEnvelopeTrim,
        revision: "code_fence_envelope_trim_v1",
        lossless: true,
        cost_tier: "fast",
        active: true,
    },
];

/// Compile-time projection of the registry's descriptor ids, in registry
/// order.  The array length is pinned to `DESCRIPTORS.len()` at compile time:
/// adding or removing a descriptor changes this function's signature and the
/// release evidence list together, so the count can never drift.
pub const fn production_registry_codec_ids() -> [&'static str; 9] {
    [
        DESCRIPTORS[0].id,
        DESCRIPTORS[1].id,
        DESCRIPTORS[2].id,
        DESCRIPTORS[3].id,
        DESCRIPTORS[4].id,
        DESCRIPTORS[5].id,
        DESCRIPTORS[6].id,
        DESCRIPTORS[7].id,
        DESCRIPTORS[8].id,
    ]
}

impl ProductionCompressorRegistry {
    pub const fn descriptors(self) -> &'static [CompressorDescriptor; 9] {
        &DESCRIPTORS
    }

    pub fn descriptor_for(self, codec: CodecKind) -> Option<CompressorDescriptor> {
        self.descriptors()
            .iter()
            .copied()
            .find(|descriptor| descriptor.codec == codec)
    }

    pub fn select(self, text: &str, budget: RoutingBudget) -> Option<CompressorSelection> {
        let decision = route_content_with_budget(text, budget);
        let descriptor = self.descriptor_for(decision.codec)?;
        descriptor.active.then_some(CompressorSelection {
            descriptor,
            route_reason: decision.reason_code,
        })
    }

    pub fn all_active(self) -> bool {
        self.descriptors()
            .iter()
            .all(|descriptor| descriptor.active)
    }

    pub fn compress(
        self,
        policy: CompressionPolicy,
        text: &str,
        transforms: &mut Vec<String>,
    ) -> String {
        crate::content_router::apply_routed_codec_with_registry(self, policy, text, transforms)
    }
}

pub const fn production_compressor_registry() -> ProductionCompressorRegistry {
    ProductionCompressorRegistry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_registry_has_nine_active_lossless_codecs() {
        let registry = production_compressor_registry();
        assert_eq!(registry.descriptors().len(), 9);
        assert!(registry.all_active());
        assert!(registry.descriptors().iter().all(|descriptor| {
            descriptor.lossless && descriptor.cost_tier == "fast" && !descriptor.id.is_empty()
        }));
    }

    #[test]
    fn selection_is_route_content_driven_and_fail_closed() {
        let registry = production_compressor_registry();
        let selected = registry
            .select("same\nsame\nsame\n", RoutingBudget::default())
            .expect("text repeat codec should be selected");
        assert_eq!(selected.descriptor.codec, CodecKind::TextRepeatFold);
        assert!(registry
            .select("token=sk-test-1234567890", RoutingBudget::default())
            .is_none());
    }

    #[test]
    fn registry_dispatch_preserves_negative_benefit() {
        let registry = production_compressor_registry();
        let mut transforms = Vec::new();
        let original = "one line";
        assert_eq!(
            registry.compress(CompressionPolicy::Balanced, original, &mut transforms),
            original
        );
        assert!(transforms.is_empty());
    }
}
