use dtoken_compress::code_codec::{
    CodeCodecAvailability, CodeCodecEvaluationError, CodeCodecGateEvidence, CodeCodecRegistry,
    CodeLanguage,
};

fn rust_evidence() -> CodeCodecGateEvidence {
    CodeCodecGateEvidence {
        language: CodeLanguage::Rust,
        parser_revision: "syn@2.0".to_string(),
        corpus_digest: "a".repeat(64),
        syntax_cases: 500,
        semantic_cases: 100,
        syntax_failures: 0,
        semantic_failures: 0,
    }
}

#[test]
fn supported_languages_are_detected_but_default_to_disabled() {
    let registry = CodeCodecRegistry::default();
    for (source, language) in [
        ("```rust\nfn main() {}\n```", CodeLanguage::Rust),
        (
            "```typescript\nconst x: number = 1;\n```",
            CodeLanguage::TypeScript,
        ),
        ("```javascript\nconst x = 1;\n```", CodeLanguage::JavaScript),
        ("```python\ndef run():\n    pass\n```", CodeLanguage::Python),
        ("```go\npackage main\nfunc main() {}\n```", CodeLanguage::Go),
        ("```java\nclass Main {}\n```", CodeLanguage::Java),
        ("```c\nint main(void) {}\n```", CodeLanguage::C),
        ("```cpp\nint main() {}\n```", CodeLanguage::Cpp),
    ] {
        let decision = registry.decision_for(source);
        assert_eq!(decision.language(), language);
        assert_eq!(decision.availability(), CodeCodecAvailability::Disabled);
        assert_eq!(decision.reason_code(), "parser-eval-gate-missing");
    }
}

#[test]
fn evaluation_evidence_is_strict_and_never_enables_active_without_an_implementation() {
    let invalid = CodeCodecGateEvidence {
        language: CodeLanguage::Rust,
        parser_revision: "tree-sitter-rust@x".to_string(),
        corpus_digest: "short".to_string(),
        syntax_cases: 500,
        semantic_cases: 100,
        syntax_failures: 0,
        semantic_failures: 0,
    };
    assert!(CodeCodecRegistry::from_evidence([invalid]).is_err());

    let valid = rust_evidence();
    let registry = CodeCodecRegistry::from_evidence([valid]).expect("validated evidence");
    let decision = registry.decision_for("```rust\nfn main() {}\n```");
    assert_eq!(
        decision.availability(),
        CodeCodecAvailability::EvaluationOnly
    );
    assert_eq!(decision.reason_code(), "implementation-not-active");
}

#[test]
fn rust_parser_builds_evaluation_only_candidate_without_enabling_active() {
    let registry = CodeCodecRegistry::from_evidence([rust_evidence()]).unwrap();
    let source = "```rust\n\n// evaluation comment\npub fn add(\n    left: usize,\n    right: usize,\n) -> usize {\n    left + right\n}\n```";
    let candidate = registry
        .prepare_evaluation_candidate(source)
        .expect("parser-backed evaluation candidate");
    assert_eq!(candidate.language(), CodeLanguage::Rust);
    assert_eq!(candidate.parser_revision(), "syn@2.0");
    assert_eq!(candidate.corpus_digest(), "a".repeat(64));
    assert_ne!(candidate.original_digest(), candidate.candidate_digest());
    assert!(candidate.candidate_source().len() < source.len());
    assert_eq!(
        registry.decision_for(source).availability(),
        CodeCodecAvailability::EvaluationOnly
    );
}

#[test]
fn evaluation_candidate_rejects_invalid_or_unimplemented_language() {
    let registry = CodeCodecRegistry::from_evidence([rust_evidence()]).unwrap();
    assert_eq!(
        registry.prepare_evaluation_candidate("```rust\nfn broken( {\n```"),
        Err(CodeCodecEvaluationError::ParseFailed)
    );
    assert_eq!(
        registry.prepare_evaluation_candidate("```typescript\nconst x = 1;\n```"),
        Err(CodeCodecEvaluationError::GateNotSatisfied)
    );
}

#[test]
fn evaluation_candidate_debug_is_metadata_only() {
    let registry = CodeCodecRegistry::from_evidence([rust_evidence()]).unwrap();
    let candidate = registry
        .prepare_evaluation_candidate(
            "```rust\n// evaluation-only source must stay private\n\npub fn secret(\n) -> &'static str {\n    \"do-not-log\"\n}\n```",
        )
        .unwrap();
    let debug = format!("{candidate:?}");
    assert!(!debug.contains("do-not-log"));
    assert!(!debug.contains("pub fn secret"));
}

#[test]
fn code_gate_debug_output_never_contains_source_code() {
    let registry = CodeCodecRegistry::default();
    let decision = registry.decision_for("```rust\nconst SECRET: &str = \"raw\";\n```");
    let debug = format!("{decision:?}");
    assert!(!debug.contains("const SECRET"));
    assert!(!debug.contains("raw"));
}
