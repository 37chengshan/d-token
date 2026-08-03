use dtoken_compress::code_codec::{
    production_registry, CodeCodecAvailability, CodeCodecEvaluationError, CodeCodecGateEvidence,
    CodeCodecProductionError, CodeCodecRegistry, CodeLanguage, CPP_PRODUCTION_CORPUS_DIGEST,
    CPP_PRODUCTION_PARSER_REVISION, C_PRODUCTION_CORPUS_DIGEST, C_PRODUCTION_PARSER_REVISION,
    GO_PRODUCTION_CORPUS_DIGEST, GO_PRODUCTION_PARSER_REVISION,
    JAVASCRIPT_PRODUCTION_CORPUS_DIGEST, JAVASCRIPT_PRODUCTION_PARSER_REVISION,
    JAVA_PRODUCTION_CORPUS_DIGEST, JAVA_PRODUCTION_PARSER_REVISION,
    PHP_PRODUCTION_CORPUS_DIGEST, PHP_PRODUCTION_PARSER_REVISION,
    PYTHON_PRODUCTION_CORPUS_DIGEST, PYTHON_PRODUCTION_PARSER_REVISION,
    RUST_PRODUCTION_COMPILER_REVISION, RUST_PRODUCTION_CORPUS_DIGEST,
    RUST_PRODUCTION_PARSER_REVISION, TREE_SITTER_SEMANTIC_REVISION,
    TYPESCRIPT_PRODUCTION_CORPUS_DIGEST, TYPESCRIPT_PRODUCTION_PARSER_REVISION,
};
use proptest::prelude::*;
use sha2::{Digest, Sha256};
use std::{fs, process::Command};

#[derive(Clone, Copy)]
struct LangCase {
    language: CodeLanguage,
    label: &'static str,
    parser_revision: &'static str,
    corpus_digest: &'static str,
}

fn language_cases() -> [LangCase; 9] {
    [
        LangCase {
            language: CodeLanguage::Rust,
            label: "rust",
            parser_revision: RUST_PRODUCTION_PARSER_REVISION,
            corpus_digest: RUST_PRODUCTION_CORPUS_DIGEST,
        },
        LangCase {
            language: CodeLanguage::TypeScript,
            label: "typescript",
            parser_revision: TYPESCRIPT_PRODUCTION_PARSER_REVISION,
            corpus_digest: TYPESCRIPT_PRODUCTION_CORPUS_DIGEST,
        },
        LangCase {
            language: CodeLanguage::JavaScript,
            label: "javascript",
            parser_revision: JAVASCRIPT_PRODUCTION_PARSER_REVISION,
            corpus_digest: JAVASCRIPT_PRODUCTION_CORPUS_DIGEST,
        },
        LangCase {
            language: CodeLanguage::Python,
            label: "python",
            parser_revision: PYTHON_PRODUCTION_PARSER_REVISION,
            corpus_digest: PYTHON_PRODUCTION_CORPUS_DIGEST,
        },
        LangCase {
            language: CodeLanguage::Go,
            label: "go",
            parser_revision: GO_PRODUCTION_PARSER_REVISION,
            corpus_digest: GO_PRODUCTION_CORPUS_DIGEST,
        },
        LangCase {
            language: CodeLanguage::Java,
            label: "java",
            parser_revision: JAVA_PRODUCTION_PARSER_REVISION,
            corpus_digest: JAVA_PRODUCTION_CORPUS_DIGEST,
        },
        LangCase {
            language: CodeLanguage::C,
            label: "c",
            parser_revision: C_PRODUCTION_PARSER_REVISION,
            corpus_digest: C_PRODUCTION_CORPUS_DIGEST,
        },
        LangCase {
            language: CodeLanguage::Cpp,
            label: "cpp",
            parser_revision: CPP_PRODUCTION_PARSER_REVISION,
            corpus_digest: CPP_PRODUCTION_CORPUS_DIGEST,
        },
        LangCase {
            language: CodeLanguage::Php,
            label: "php",
            parser_revision: PHP_PRODUCTION_PARSER_REVISION,
            corpus_digest: PHP_PRODUCTION_CORPUS_DIGEST,
        },
    ]
}

fn rust_evidence() -> CodeCodecGateEvidence {
    CodeCodecGateEvidence {
        language: CodeLanguage::Rust,
        parser_revision: RUST_PRODUCTION_PARSER_REVISION.to_string(),
        compiler_revision: RUST_PRODUCTION_COMPILER_REVISION.to_string(),
        corpus_digest: RUST_PRODUCTION_CORPUS_DIGEST.to_string(),
    }
}

fn tree_sitter_evidence(case: LangCase) -> CodeCodecGateEvidence {
    CodeCodecGateEvidence {
        language: case.language,
        parser_revision: case.parser_revision.to_string(),
        compiler_revision: TREE_SITTER_SEMANTIC_REVISION.to_string(),
        corpus_digest: case.corpus_digest.to_string(),
    }
}

fn fixture_digest(case: LangCase) -> String {
    let mut digest = Sha256::new();
    match case.language {
        CodeLanguage::Rust => {
            let manifest = include_str!("fixtures/code_codec/rust/manifest.txt");
            digest.update(manifest.as_bytes());
            for source in [
                include_str!("fixtures/code_codec/rust/async_and_generics.rs"),
                include_str!("fixtures/code_codec/rust/attributes_and_docs.rs"),
                include_str!("fixtures/code_codec/rust/macros.rs"),
            ] {
                digest.update(source.as_bytes());
            }
        }
        _ => {
            let manifest = match case.language {
                CodeLanguage::TypeScript => {
                    include_str!("fixtures/code_codec/typescript/manifest.txt")
                }
                CodeLanguage::JavaScript => {
                    include_str!("fixtures/code_codec/javascript/manifest.txt")
                }
                CodeLanguage::Python => include_str!("fixtures/code_codec/python/manifest.txt"),
                CodeLanguage::Go => include_str!("fixtures/code_codec/go/manifest.txt"),
                CodeLanguage::Java => include_str!("fixtures/code_codec/java/manifest.txt"),
                CodeLanguage::C => include_str!("fixtures/code_codec/c/manifest.txt"),
                CodeLanguage::Cpp => include_str!("fixtures/code_codec/cpp/manifest.txt"),
                CodeLanguage::Php => include_str!("fixtures/code_codec/php/manifest.txt"),
                CodeLanguage::Rust | CodeLanguage::Unknown => unreachable!(),
            };
            digest.update(manifest.as_bytes());
            digest.update(raw_fixture_body(case.language).as_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

fn fixture_body(language: CodeLanguage) -> &'static str {
    match language {
        CodeLanguage::Rust => include_str!("fixtures/code_codec/rust/async_and_generics.rs"),
        CodeLanguage::TypeScript => include_str!("fixtures/code_codec/typescript/sample.ts"),
        CodeLanguage::JavaScript => include_str!("fixtures/code_codec/javascript/sample.js"),
        CodeLanguage::Python => include_str!("fixtures/code_codec/python/sample.py"),
        CodeLanguage::Go => include_str!("fixtures/code_codec/go/sample.go"),
        CodeLanguage::Java => include_str!("fixtures/code_codec/java/Sample.java"),
        CodeLanguage::C => include_str!("fixtures/code_codec/c/sample.c"),
        CodeLanguage::Cpp => include_str!("fixtures/code_codec/cpp/sample.cpp"),
        CodeLanguage::Php => include_str!("fixtures/code_codec/php/sample.php"),
        CodeLanguage::Unknown => "",
    }
    .trim_end_matches('\n')
}

fn raw_fixture_body(language: CodeLanguage) -> &'static str {
    match language {
        CodeLanguage::Rust => include_str!("fixtures/code_codec/rust/async_and_generics.rs"),
        CodeLanguage::TypeScript => include_str!("fixtures/code_codec/typescript/sample.ts"),
        CodeLanguage::JavaScript => include_str!("fixtures/code_codec/javascript/sample.js"),
        CodeLanguage::Python => include_str!("fixtures/code_codec/python/sample.py"),
        CodeLanguage::Go => include_str!("fixtures/code_codec/go/sample.go"),
        CodeLanguage::Java => include_str!("fixtures/code_codec/java/Sample.java"),
        CodeLanguage::C => include_str!("fixtures/code_codec/c/sample.c"),
        CodeLanguage::Cpp => include_str!("fixtures/code_codec/cpp/sample.cpp"),
        CodeLanguage::Php => include_str!("fixtures/code_codec/php/sample.php"),
        CodeLanguage::Unknown => "",
    }
}

fn beneficial_source(case: LangCase) -> String {
    format!(
        "```{}\n\n{}\n\n```",
        case.label,
        fixture_body(case.language)
    )
}

fn nonbeneficial_source(case: LangCase) -> String {
    format!("```{}\n{}\n```", case.label, fixture_body(case.language))
}

fn secret_source(case: LangCase) -> String {
    format!(
        "```{}\n\nconst TOKEN = \"sk-not-for-codec\";\n\n```",
        case.label
    )
}

fn line_sensitive_source(case: LangCase) -> String {
    match case.language {
        CodeLanguage::Rust => "```rust\n\nfn current_line() -> u32 {\n    line!()\n}\n\n```",
        CodeLanguage::TypeScript => {
            "```typescript\n\n//# sourceMappingURL=sample.js.map\nconst value: number = 1;\n\n```"
        }
        CodeLanguage::JavaScript => {
            "```javascript\n\n//# sourceMappingURL=sample.js.map\nconst value = 1;\n\n```"
        }
        CodeLanguage::Python => {
            "```python\n\nTraceback (most recent call last):\n  File \"demo.py\", line 1, in <module>\n\n```"
        }
        CodeLanguage::Go => {
            "```go\n\n//line generated.go:7\npackage main\nfunc main() {}\n\n```"
        }
        CodeLanguage::Java => {
            "```java\n\nException in thread \"main\" java.lang.RuntimeException\n\tat codec.Sample.add(Sample.java:7)\n\n```"
        }
        CodeLanguage::C => {
            "```c\n\n#line 42 \"generated.c\"\nint main(void) { return __LINE__; }\n\n```"
        }
        CodeLanguage::Cpp => {
            "```cpp\n\n#line 42 \"generated.cpp\"\nint main() { return __LINE__; }\n\n```"
        }
        CodeLanguage::Php => {
            "```php\n\n<?php\necho __LINE__;\n\n```"
        }
        CodeLanguage::Unknown => unreachable!(),
    }
    .to_string()
}

#[test]
fn supported_languages_are_detected_but_default_to_disabled() {
    let registry = CodeCodecRegistry::default();
    for case in language_cases() {
        let decision = registry.decision_for(&nonbeneficial_source(case));
        assert_eq!(decision.language(), case.language);
        assert_eq!(decision.availability(), CodeCodecAvailability::Disabled);
        assert_eq!(decision.reason_code(), "parser-eval-gate-missing");
    }
}

#[test]
fn production_registry_enables_all_supported_languages_only_for_beneficial_fenced_sources() {
    let registry = production_registry();
    for case in language_cases() {
        let decision = registry.decision_for(&beneficial_source(case));
        assert_eq!(decision.language(), case.language);
        assert_eq!(
            decision.availability(),
            CodeCodecAvailability::ProductionSafe
        );
        assert_eq!(decision.reason_code(), "code-fence-envelope-trim-v1");

        let candidate = registry
            .prepare_production_candidate(&beneficial_source(case))
            .expect("production candidate");
        assert_eq!(candidate.language(), case.language);
        assert_eq!(candidate.parser_revision(), case.parser_revision);
        assert_eq!(candidate.corpus_digest(), case.corpus_digest);
        assert_eq!(candidate.fence_label(), case.label);
        assert_eq!(candidate.newline_delimiter(), "\n");
        assert_eq!(candidate.leading_trimmed_lines(), &vec![String::new()]);
        assert_eq!(candidate.trailing_trimmed_lines(), &vec![String::new()]);
        assert_eq!(candidate.reconstruct_source(), beneficial_source(case));
        assert!(candidate.candidate_source().len() < beneficial_source(case).len());
    }
}

#[test]
fn production_codec_fails_closed_for_line_sensitive_secret_cancelled_and_nonbeneficial_inputs() {
    let registry = production_registry();
    for case in language_cases() {
        assert_eq!(
            registry.prepare_production_candidate(&line_sensitive_source(case)),
            Err(CodeCodecProductionError::LineSensitiveContent),
            "line sensitive: {}",
            case.label
        );
        assert_eq!(
            registry.prepare_production_candidate(&secret_source(case)),
            Err(CodeCodecProductionError::SecretLikeContent),
            "secret: {}",
            case.label
        );
        assert_eq!(
            registry.prepare_production_candidate(&nonbeneficial_source(case)),
            Err(CodeCodecProductionError::NonBeneficial),
            "nonbeneficial: {}",
            case.label
        );
        assert_eq!(
            registry.prepare_production_candidate_with_cancel(&beneficial_source(case), || true),
            Err(CodeCodecProductionError::Cancelled),
            "cancelled: {}",
            case.label
        );
        assert_eq!(
            registry
                .decision_for(&line_sensitive_source(case))
                .availability(),
            CodeCodecAvailability::EvaluationOnly
        );
    }
}

#[test]
fn production_codec_fails_closed_for_incomplete_evidence_and_malformed_or_parse_broken_input() {
    let mut incomplete = rust_evidence();
    incomplete.corpus_digest = "b".repeat(64);
    let registry = CodeCodecRegistry::from_evidence([incomplete]).unwrap();
    assert_eq!(
        registry.prepare_production_candidate("```rust\n\nfn main() {}\n\n```"),
        Err(CodeCodecProductionError::GateNotSatisfied)
    );
    assert_eq!(
        registry
            .decision_for("```rust\n\nfn main() {}\n\n```")
            .availability(),
        CodeCodecAvailability::EvaluationOnly
    );

    let mut incomplete_ts = tree_sitter_evidence(language_cases()[1]);
    incomplete_ts.compiler_revision = "tree-sitter-ast-equality@broken".to_string();
    let ts_registry = CodeCodecRegistry::from_evidence([incomplete_ts]).unwrap();
    assert_eq!(
        ts_registry
            .decision_for("```typescript\n\nconst value: number = 1;\n\n```")
            .availability(),
        CodeCodecAvailability::EvaluationOnly
    );

    let registry = production_registry();
    assert_eq!(
        registry.prepare_production_candidate("```rust\n\nfn broken( {\n\n```"),
        Err(CodeCodecProductionError::ParseFailed)
    );
    assert_eq!(
        registry.prepare_production_candidate("```typescript\n\nconst broken = (: number);\n\n```"),
        Err(CodeCodecProductionError::ParseFailed)
    );
    assert_eq!(
        registry.prepare_production_candidate("```python\n\npass\n```extra"),
        Err(CodeCodecProductionError::MalformedFence)
    );
}

#[test]
fn evaluation_candidate_remains_rust_only_and_debug_output_is_metadata_only() {
    let registry = CodeCodecRegistry::from_evidence([rust_evidence()]).unwrap();
    let candidate = registry
        .prepare_evaluation_candidate(
            "```rust\n// evaluation-only source must stay private\n\npub fn secret(\n) -> &'static str {\n    \"do-not-log\"\n}\n```",
        )
        .unwrap();
    let debug = format!("{candidate:?}");
    assert!(!debug.contains("do-not-log"));
    assert!(!debug.contains("pub fn secret"));
    assert_eq!(
        registry.prepare_evaluation_candidate("```typescript\nconst x = 1;\n```"),
        Err(CodeCodecEvaluationError::GateNotSatisfied)
    );
    assert_eq!(
        registry.prepare_evaluation_candidate("```rust\nfn broken( {\n```"),
        Err(CodeCodecEvaluationError::ParseFailed)
    );
}

#[test]
fn production_candidate_debug_is_metadata_only() {
    let registry = production_registry();
    for case in language_cases() {
        let candidate = registry
            .prepare_production_candidate(&beneficial_source(case))
            .unwrap();
        let debug = format!("{candidate:?}");
        assert!(!debug.contains("return left + right"));
        assert!(!debug.contains("Point"));
        assert!(!debug.contains("Counter"));
    }
}

proptest! {
    #[test]
    fn fenced_envelope_trim_reconstructs_exact_original_for_all_languages(
        selector in 0usize..9,
        leading in 1usize..4,
        trailing in 1usize..4,
        ws in "[ \\t]{0,3}"
    ) {
        let case = language_cases()[selector];
        let body = fixture_body(case.language);
        let lead = std::iter::repeat_n(ws.clone(), leading)
            .collect::<Vec<_>>()
            .join("\n");
        let tail = std::iter::repeat_n(ws.clone(), trailing)
            .collect::<Vec<_>>()
            .join("\n");
        let source = format!("```{}\n{}\n{}\n{}\n```", case.label, lead, body, tail);
        let candidate = production_registry()
            .prepare_production_candidate(&source)
            .expect("safe candidate");
        prop_assert_eq!(candidate.reconstruct_source(), source.clone());
        prop_assert!(candidate.candidate_source().len() < source.len());
    }
}

#[test]
fn exact_corpus_digests_are_reproducible() {
    for case in language_cases() {
        assert_eq!(fixture_digest(case), case.corpus_digest);
    }
}

#[test]
fn fixture_syntax_checks_remain_reproducible_for_supported_external_toolchains() {
    let output_dir = std::env::temp_dir().join(format!(
        "dtoken-code-codec-fixtures-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&output_dir).expect("temporary output");

    let rust = format!(
        "{}/tests/fixtures/code_codec/rust/async_and_generics.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let rust_status = Command::new("rustc")
        .args(["--edition", "2021", "--crate-type", "lib", &rust, "-o"])
        .arg(output_dir.join("rust.rlib"))
        .status()
        .expect("rustc available");
    assert!(rust_status.success());

    let js = format!(
        "{}/tests/fixtures/code_codec/javascript/sample.js",
        env!("CARGO_MANIFEST_DIR")
    );
    let js_status = Command::new("node")
        .args(["--check", &js])
        .status()
        .expect("node available");
    assert!(js_status.success());

    let py = format!(
        "{}/tests/fixtures/code_codec/python/sample.py",
        env!("CARGO_MANIFEST_DIR")
    );
    let py_status = Command::new("python3")
        .args(["-m", "py_compile", &py])
        .status()
        .expect("python3 available");
    assert!(py_status.success());

    let go = format!(
        "{}/tests/fixtures/code_codec/go/sample.go",
        env!("CARGO_MANIFEST_DIR")
    );
    let go_status = Command::new("go")
        .args(["tool", "compile", "-o"])
        .arg(output_dir.join("go.o"))
        .arg(&go)
        .status()
        .expect("go available");
    assert!(go_status.success());

    let java = format!(
        "{}/tests/fixtures/code_codec/java/Sample.java",
        env!("CARGO_MANIFEST_DIR")
    );
    let java_status = Command::new("javac")
        .arg("-d")
        .arg(output_dir.join("java"))
        .arg(&java)
        .status()
        .expect("javac available");
    assert!(java_status.success());

    let c = format!(
        "{}/tests/fixtures/code_codec/c/sample.c",
        env!("CARGO_MANIFEST_DIR")
    );
    let c_status = Command::new("cc")
        .args(["-fsyntax-only", &c])
        .status()
        .expect("cc available");
    assert!(c_status.success());

    let cpp = format!(
        "{}/tests/fixtures/code_codec/cpp/sample.cpp",
        env!("CARGO_MANIFEST_DIR")
    );
    let cpp_status = Command::new("c++")
        .args(["-fsyntax-only", &cpp])
        .status()
        .expect("c++ available");
    assert!(cpp_status.success());

    fs::remove_dir_all(output_dir).expect("remove temporary output");
}
