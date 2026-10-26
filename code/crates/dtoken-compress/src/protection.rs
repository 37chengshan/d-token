//! Generic, image-independent protection primitives.
//!
//! These types deliberately operate on normalized context units rather than a
//! provider wire format.  They are used to decide which units may be handed to
//! a lossless text codec.  Binary and image units are always opaque and are
//! never decoded or rewritten by this module.

// Risk classification predates this provider-neutral module.  Re-exporting
// it here keeps the new import path independent of Optical terminology while
// the legacy implementation is migrated in a later phase.
pub use crate::optical::risk::{
    classify_block, AnchorMap, ExactAnchor, RiskPartitionPlan, RiskTier, SidecarPolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextUnitKind {
    Prose,
    SystemInstruction,
    ToolSchema,
    ToolCall,
    ToolResult,
    Code,
    Diff,
    Error,
    Path,
    Identifier,
    Citation,
    Image,
    Opaque,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextUnit {
    id: String,
    kind: ContextUnitKind,
    bytes: Vec<u8>,
    protected: bool,
}

impl ContextUnit {
    pub fn new(id: impl Into<String>, kind: ContextUnitKind, bytes: impl Into<Vec<u8>>) -> Self {
        let bytes = bytes.into();
        let protected = !matches!(kind, ContextUnitKind::Prose) || contains_code_fence(&bytes);
        Self {
            id: id.into(),
            kind,
            bytes,
            protected,
        }
    }

    pub fn prose(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(id, ContextUnitKind::Prose, text.into().into_bytes())
    }

    pub fn protected(mut self, protected: bool) -> Self {
        self.protected = protected;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn kind(&self) -> ContextUnitKind {
        self.kind
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn is_protected(&self) -> bool {
        self.protected
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtectedKind {
    SecretLike,
    SystemInstruction,
    ToolPair,
    ToolSchema,
    Code,
    Diff,
    UnresolvedError,
    PathOrIdentifier,
    Citation,
    Image,
    Opaque,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedSpan {
    unit_id: String,
    kind: ProtectedKind,
    start: usize,
    end: usize,
    reason: &'static str,
}

impl ProtectedSpan {
    pub fn unit_id(&self) -> &str {
        &self.unit_id
    }

    pub fn kind(&self) -> ProtectedKind {
        self.kind
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn reason(&self) -> &str {
        self.reason
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextInventory {
    units: Vec<ContextInventoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextInventoryEntry {
    unit_id: String,
    kind: ContextUnitKind,
    bytes: usize,
    protected: bool,
}

impl ContextInventoryEntry {
    pub fn unit_id(&self) -> &str {
        &self.unit_id
    }

    pub fn kind(&self) -> ContextUnitKind {
        self.kind
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn is_protected(&self) -> bool {
        self.protected
    }
}

impl ContextInventory {
    pub fn units(&self) -> &[ContextInventoryEntry] {
        &self.units
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    pub fn total_bytes(&self) -> usize {
        self.units.iter().map(|unit| unit.bytes).sum()
    }

    pub fn protected_bytes(&self) -> usize {
        self.units
            .iter()
            .filter(|unit| unit.protected)
            .map(|unit| unit.bytes)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProtectedSpanSet {
    spans: Vec<ProtectedSpan>,
}

impl ProtectedSpanSet {
    pub fn spans(&self) -> &[ProtectedSpan] {
        &self.spans
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    pub fn contains_kind(&self, kind: ProtectedKind) -> bool {
        self.spans.iter().any(|span| span.kind == kind)
    }

    pub fn overlaps(&self, unit_id: &str, start: usize, end: usize) -> bool {
        self.spans
            .iter()
            .any(|span| span.unit_id == unit_id && start < span.end && end > span.start)
    }
}

/// Build a stable inventory in input order.  IDs are preserved verbatim so
/// callers can use them as manifest pointers without exposing content.
pub fn inventory_context(input: &[ContextUnit]) -> ContextInventory {
    ContextInventory {
        units: input
            .iter()
            .map(|unit| ContextInventoryEntry {
                unit_id: unit.id.clone(),
                kind: unit.kind,
                bytes: unit.bytes.len(),
                protected: unit.protected || !is_codec_candidate(unit),
            })
            .collect(),
    }
}

/// Return whole-unit protected spans.  The local engine may still apply a
/// lossless codec to a separate prose unit; it can never cross one of these
/// boundaries or mutate the bytes covered by a span.
pub fn protected_spans(input: &[ContextUnit], _inventory: &ContextInventory) -> ProtectedSpanSet {
    let mut spans = Vec::new();
    let mut tool_ids = Vec::new();
    for unit in input {
        let (kind, reason) = if contains_secret_like_bytes(unit.bytes()) {
            (ProtectedKind::SecretLike, "secret-like-content")
        } else {
            match unit.kind {
                ContextUnitKind::SystemInstruction => {
                    (ProtectedKind::SystemInstruction, "system-instruction")
                }
                ContextUnitKind::ToolSchema => (ProtectedKind::ToolSchema, "tool-schema"),
                ContextUnitKind::ToolCall | ContextUnitKind::ToolResult => {
                    tool_ids.push(unit.id.clone());
                    (ProtectedKind::ToolPair, "tool-pair")
                }
                ContextUnitKind::Code => (ProtectedKind::Code, "code-block"),
                ContextUnitKind::Diff => (ProtectedKind::Diff, "diff-block"),
                ContextUnitKind::Error => (ProtectedKind::UnresolvedError, "unresolved-error"),
                ContextUnitKind::Path | ContextUnitKind::Identifier => {
                    (ProtectedKind::PathOrIdentifier, "path-or-identifier")
                }
                ContextUnitKind::Citation => (ProtectedKind::Citation, "citation"),
                ContextUnitKind::Image => (ProtectedKind::Image, "image-opaque"),
                ContextUnitKind::Opaque => (ProtectedKind::Opaque, "opaque-bytes"),
                ContextUnitKind::Prose if contains_code_fence(unit.bytes()) => {
                    (ProtectedKind::Code, "code-fence")
                }
                ContextUnitKind::Prose if unit.protected => {
                    (ProtectedKind::Opaque, "caller-protected")
                }
                ContextUnitKind::Prose => continue,
            }
        };
        spans.push(ProtectedSpan {
            unit_id: unit.id.clone(),
            kind,
            start: 0,
            end: unit.bytes.len(),
            reason,
        });
    }

    // A tool call/result pair is represented as two spans so each source unit
    // stays independently addressable.  The local engine uses this marker to
    // reject a candidate that drops one side of a pair.
    if tool_ids.len() > 1 {
        spans.sort_by(|left, right| left.unit_id.cmp(&right.unit_id));
    }
    ProtectedSpanSet { spans }
}

pub fn contains_secret_like_bytes(bytes: &[u8]) -> bool {
    crate::secret::contains_secret_like(&bytes)
}

fn is_codec_candidate(unit: &ContextUnit) -> bool {
    matches!(unit.kind, ContextUnitKind::Prose)
        && !unit.protected
        && !contains_secret_like_bytes(unit.bytes())
}

fn contains_code_fence(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    text.contains("```") || text.contains("~~~")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_sensitive_units_without_touching_prose() {
        let input = vec![
            ContextUnit::new("system", ContextUnitKind::SystemInstruction, "follow rules"),
            ContextUnit::new(
                "secret",
                ContextUnitKind::Prose,
                "Authorization: Bearer hidden",
            ),
            ContextUnit::prose("prose", "repeated prose"),
            ContextUnit::new("image", ContextUnitKind::Image, [0, 1, 2, 3]),
        ];
        let inventory = inventory_context(&input);
        let spans = protected_spans(&input, &inventory);
        assert!(spans.contains_kind(ProtectedKind::SystemInstruction));
        assert!(spans.contains_kind(ProtectedKind::SecretLike));
        assert!(spans.contains_kind(ProtectedKind::Image));
        assert!(!spans.overlaps("prose", 0, 6));
    }

    #[test]
    fn fenced_prose_is_a_code_protection_boundary() {
        let input = vec![ContextUnit::prose("fence", "```rust\nlet x = 1;\n```")];
        let inventory = inventory_context(&input);
        let spans = protected_spans(&input, &inventory);
        assert!(spans.contains_kind(ProtectedKind::Code));
        assert!(spans.overlaps("fence", 0, input[0].bytes().len()));
    }
}
