use dtoken_compress::protection::{
    inventory_context, protected_spans, ContextUnit, ContextUnitKind, ProtectedKind,
};

#[test]
fn protection_keeps_sensitive_context_out_of_candidate_space() {
    let input = vec![
        ContextUnit::new("system", ContextUnitKind::SystemInstruction, "follow policy"),
        ContextUnit::new("call", ContextUnitKind::ToolCall, "tool call"),
        ContextUnit::new("result", ContextUnitKind::ToolResult, "tool result"),
        ContextUnit::prose("fence", "```rust\nlet exact = true;\n```"),
        ContextUnit::new("error", ContextUnitKind::Error, "unresolved failure"),
        ContextUnit::new("path", ContextUnitKind::Path, "/repo/src/main.rs"),
        ContextUnit::new("citation", ContextUnitKind::Citation, "[1] source"),
        ContextUnit::new(
            "secret",
            ContextUnitKind::Prose,
            "Authorization: Bearer deliberately-not-a-real-token",
        ),
        ContextUnit::prose("prose", "redundant prose may be considered separately"),
    ];

    let inventory = inventory_context(&input);
    let protected = protected_spans(&input, &inventory);

    for kind in [
        ProtectedKind::SystemInstruction,
        ProtectedKind::ToolPair,
        ProtectedKind::Code,
        ProtectedKind::UnresolvedError,
        ProtectedKind::PathOrIdentifier,
        ProtectedKind::Citation,
        ProtectedKind::SecretLike,
    ] {
        assert!(protected.contains_kind(kind), "missing protection for {kind:?}");
    }
    assert!(!protected.overlaps("prose", 0, input[8].bytes().len()));
}

#[test]
fn image_and_invalid_utf8_are_preserved_as_non_candidates() {
    let image = vec![0, 159, 146, 150];
    let opaque = vec![0xff, 0xfe, 0xfd];
    let input = vec![
        ContextUnit::new("image", ContextUnitKind::Image, image.clone()),
        ContextUnit::new("opaque", ContextUnitKind::Opaque, opaque.clone()),
    ];
    let inventory = inventory_context(&input);
    let protected = protected_spans(&input, &inventory);

    assert_eq!(input[0].bytes(), image);
    assert_eq!(input[1].bytes(), opaque);
    assert!(protected.contains_kind(ProtectedKind::Image));
    assert!(protected.contains_kind(ProtectedKind::Opaque));
    assert_eq!(inventory.protected_bytes(), image.len() + opaque.len());
}
