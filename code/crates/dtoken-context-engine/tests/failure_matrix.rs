use dtoken_context_engine::{validate_json_candidate, ContextBudget};

#[test]
fn validator_rejects_expansion_and_invalid_json() {
    let original = br#"{"text":"small"}"#;
    let expanded = br#"{"text":"this candidate is longer than source"}"#;
    assert_eq!(
        validate_json_candidate(original, expanded, 1024, 1).unwrap_err(),
        "candidate_expansion"
    );
    assert_eq!(
        validate_json_candidate(original, br#"not-json"#, 1024, 1).unwrap_err(),
        "codec_output_invalid"
    );
}

#[test]
fn budget_tokens_are_deterministic_bytes() {
    assert_eq!(ContextBudget::tokens(100).max_bytes, 400);
    assert_eq!(ContextBudget::bytes(100).max_bytes, 100);
}
