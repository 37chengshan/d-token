use dtoken_context_engine::{
    ContextBudget, ContextEngine, ContextInput, ContextOutcome, ContextUnit, ContextUnitKind,
    LocalContextConfig, ProtectedKind,
};

#[test]
fn local_engine_reduces_redundant_prose_and_is_deterministic() {
    let repeated = "same line\n".repeat(20);
    let units = vec![
        ContextUnit::new(
            "system",
            ContextUnitKind::SystemInstruction,
            "keep this exact",
        ),
        ContextUnit::prose("history", repeated),
        ContextUnit::new("path", ContextUnitKind::Path, "/repo/src/main.rs"),
    ];
    let input = ContextInput::new("req", "project", ContextBudget::bytes(16_000), units);
    let engine = ContextEngine::local(LocalContextConfig::default());
    let first = engine.prepare(input.clone());
    let second = engine.prepare(input);
    match (&first, &second) {
        (ContextOutcome::Candidate(left), ContextOutcome::Candidate(right)) => {
            assert_eq!(left.candidate_digest, right.candidate_digest);
            assert!(left.validation.is_valid());
            assert!(!left.transforms.is_empty());
            assert_eq!(left.source, "local");
        }
        other => panic!("expected deterministic candidate, got {other:?}"),
    }
}

#[test]
fn all_protected_input_returns_original_baseline() {
    let input = ContextInput::new(
        "req",
        "project",
        ContextBudget::bytes(1_000),
        vec![
            ContextUnit::new(
                "secret",
                ContextUnitKind::Prose,
                "Authorization: Bearer secret",
            ),
            ContextUnit::new("image", ContextUnitKind::Image, [1, 2, 3]),
            ContextUnit::new("error", ContextUnitKind::Error, "unresolved failure"),
        ],
    );
    let outcome = ContextEngine::local(LocalContextConfig::default()).prepare(input);
    assert!(
        matches!(outcome, ContextOutcome::Baseline(baseline) if baseline.reason_code == "no_safe_candidate")
    );
}

#[test]
fn image_only_is_byte_preserving_baseline() {
    let image = vec![0, 159, 146, 150, 1, 2];
    let input = ContextInput::new(
        "req",
        "project",
        ContextBudget::bytes(1_000),
        vec![ContextUnit::new(
            "image",
            ContextUnitKind::Image,
            image.clone(),
        )],
    );
    let outcome = ContextEngine::local(LocalContextConfig::default()).prepare(input);
    let ContextOutcome::Baseline(baseline) = outcome else {
        panic!("image-only input cannot produce a candidate")
    };
    let bytes = baseline.body[0]["bytes"].as_array().expect("byte array");
    assert_eq!(bytes.len(), image.len());
    assert_eq!(bytes[1].as_u64(), Some(159));
    assert_eq!(baseline.reason_code, "no_safe_candidate");
}

#[test]
fn mixed_image_text_does_not_modify_image_unit() {
    let input = ContextInput::new(
        "req",
        "project",
        ContextBudget::bytes(4_000),
        vec![
            ContextUnit::new("image", ContextUnitKind::Image, [0, 1, 2, 3]),
            ContextUnit::prose("text", "repeat\n".repeat(20)),
        ],
    );
    let outcome = ContextEngine::local(LocalContextConfig::default()).prepare(input);
    let ContextOutcome::Candidate(candidate) = outcome else {
        panic!("repeated prose should remain a candidate next to an image")
    };
    assert_eq!(candidate.body[0]["bytes"], serde_json::json!([0, 1, 2, 3]));
    assert!(candidate.validation.changed_units > 0);
}

#[test]
fn protected_kinds_are_exposed_for_callers() {
    let unit = ContextUnit::new("diff", ContextUnitKind::Diff, "@@ -1 +1 @@");
    let inventory = dtoken_context_engine::inventory_context(std::slice::from_ref(&unit));
    let spans = dtoken_context_engine::protected_spans(std::slice::from_ref(&unit), &inventory);
    assert!(spans.contains_kind(ProtectedKind::Diff));
}

#[test]
fn invalid_recovery_receipt_is_a_byte_preserving_baseline() {
    let units = vec![ContextUnit::prose("history", "repeat\n".repeat(20))];
    let expected = serde_json::json!([{
        "id": "history",
        "kind": "Prose",
        "bytes": units[0].bytes(),
    }]);
    let engine = ContextEngine::local(LocalContextConfig {
        recovery: Some(dtoken_context_engine::RecoveryInput::new("not-a-ccr", 1)),
        ..Default::default()
    });
    let outcome = engine.prepare(ContextInput::new(
        "req",
        "project",
        ContextBudget::bytes(4_000),
        units,
    ));
    assert!(
        matches!(outcome, ContextOutcome::Baseline(ref value) if value.reason_code == "recovery_prepare_failed" && value.body == expected)
    );
}
