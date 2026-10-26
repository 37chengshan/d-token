use dtoken_compress::protection::{
    contains_secret_like_bytes, ContextUnit, ContextUnitKind, ProtectedKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitClassification {
    pub protected: bool,
    pub protected_kind: Option<ProtectedKind>,
    pub reason_code: &'static str,
}

pub fn classify_unit(unit: &ContextUnit) -> UnitClassification {
    if contains_secret_like_bytes(unit.bytes()) {
        return UnitClassification {
            protected: true,
            protected_kind: Some(ProtectedKind::SecretLike),
            reason_code: "secret-like-content",
        };
    }

    let (protected_kind, reason_code) = match unit.kind() {
        ContextUnitKind::Prose => (None, "candidate-prose"),
        ContextUnitKind::SystemInstruction => {
            (Some(ProtectedKind::SystemInstruction), "system-instruction")
        }
        ContextUnitKind::ToolSchema => (Some(ProtectedKind::ToolSchema), "tool-schema"),
        ContextUnitKind::ToolCall | ContextUnitKind::ToolResult => {
            (Some(ProtectedKind::ToolPair), "tool-pair")
        }
        ContextUnitKind::Code => (Some(ProtectedKind::Code), "code-block"),
        ContextUnitKind::Diff => (Some(ProtectedKind::Diff), "diff-block"),
        ContextUnitKind::Error => (Some(ProtectedKind::UnresolvedError), "unresolved-error"),
        ContextUnitKind::Path | ContextUnitKind::Identifier => {
            (Some(ProtectedKind::PathOrIdentifier), "path-or-identifier")
        }
        ContextUnitKind::Citation => (Some(ProtectedKind::Citation), "citation"),
        ContextUnitKind::Image => (Some(ProtectedKind::Image), "image-opaque"),
        ContextUnitKind::Opaque => (Some(ProtectedKind::Opaque), "opaque-bytes"),
    };

    UnitClassification {
        protected: unit.is_protected() || protected_kind.is_some(),
        protected_kind,
        reason_code,
    }
}
