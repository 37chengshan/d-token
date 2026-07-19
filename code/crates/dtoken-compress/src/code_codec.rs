use quote::ToTokens;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CodeLanguage {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    C,
    Cpp,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeCodecAvailability {
    Disabled,
    EvaluationOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeCodecGateEvidence {
    pub language: CodeLanguage,
    pub parser_revision: String,
    pub corpus_digest: String,
    pub syntax_cases: usize,
    pub semantic_cases: usize,
    pub syntax_failures: usize,
    pub semantic_failures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeCodecGateError {
    UnknownLanguage,
    InvalidParserRevision,
    InvalidCorpusDigest,
    InsufficientSyntaxCases,
    InsufficientSemanticCases,
    EvaluationFailures,
    DuplicateLanguage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeCodecEvaluationError {
    GateNotSatisfied,
    LanguageNotImplemented,
    ParserRevisionMismatch,
    ParseFailed,
    NonBeneficial,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CodeCodecEvaluationCandidate {
    language: CodeLanguage,
    parser_revision: String,
    corpus_digest: String,
    original_digest: String,
    candidate_digest: String,
    original_bytes: usize,
    candidate_source: String,
}

impl std::fmt::Debug for CodeCodecEvaluationCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodeCodecEvaluationCandidate")
            .field("language", &self.language)
            .field("parser_revision", &self.parser_revision)
            .field("corpus_digest", &self.corpus_digest)
            .field("original_digest", &self.original_digest)
            .field("candidate_digest", &self.candidate_digest)
            .field("original_bytes", &self.original_bytes)
            .field("candidate_bytes", &self.candidate_source.len())
            .field("semantic_risk", &"macro-token-stringification-not-proven")
            .finish()
    }
}

impl CodeCodecEvaluationCandidate {
    pub fn language(&self) -> CodeLanguage {
        self.language
    }

    pub fn parser_revision(&self) -> &str {
        &self.parser_revision
    }

    pub fn corpus_digest(&self) -> &str {
        &self.corpus_digest
    }

    pub fn original_digest(&self) -> &str {
        &self.original_digest
    }

    pub fn candidate_digest(&self) -> &str {
        &self.candidate_digest
    }

    pub fn candidate_source(&self) -> &str {
        &self.candidate_source
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeCodecDecision {
    language: CodeLanguage,
    availability: CodeCodecAvailability,
    reason_code: &'static str,
    parser_revision: Option<String>,
    corpus_digest: Option<String>,
}

impl CodeCodecDecision {
    pub fn language(&self) -> CodeLanguage {
        self.language
    }

    pub fn availability(&self) -> CodeCodecAvailability {
        self.availability
    }

    pub fn reason_code(&self) -> &str {
        self.reason_code
    }

    pub fn parser_revision(&self) -> Option<&str> {
        self.parser_revision.as_deref()
    }

    pub fn corpus_digest(&self) -> Option<&str> {
        self.corpus_digest.as_deref()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CodeCodecRegistry {
    evaluated: BTreeMap<CodeLanguage, CodeCodecGateEvidence>,
}

impl CodeCodecRegistry {
    pub fn from_evidence(
        evidence: impl IntoIterator<Item = CodeCodecGateEvidence>,
    ) -> Result<Self, CodeCodecGateError> {
        let mut evaluated = BTreeMap::new();
        for item in evidence {
            validate_evidence(&item)?;
            let language = item.language;
            if evaluated.insert(language, item).is_some() {
                return Err(CodeCodecGateError::DuplicateLanguage);
            }
        }
        Ok(Self { evaluated })
    }

    pub fn decision_for(&self, source: &str) -> CodeCodecDecision {
        let language = detect_language(source);
        if language == CodeLanguage::Unknown {
            return CodeCodecDecision {
                language,
                availability: CodeCodecAvailability::Disabled,
                reason_code: "language-unknown",
                parser_revision: None,
                corpus_digest: None,
            };
        }
        match self.evaluated.get(&language) {
            Some(evidence) => CodeCodecDecision {
                language,
                availability: CodeCodecAvailability::EvaluationOnly,
                reason_code: "implementation-not-active",
                parser_revision: Some(evidence.parser_revision.clone()),
                corpus_digest: Some(evidence.corpus_digest.clone()),
            },
            None => CodeCodecDecision {
                language,
                availability: CodeCodecAvailability::Disabled,
                reason_code: "parser-eval-gate-missing",
                parser_revision: None,
                corpus_digest: None,
            },
        }
    }

    pub fn prepare_evaluation_candidate(
        &self,
        source: &str,
    ) -> Result<CodeCodecEvaluationCandidate, CodeCodecEvaluationError> {
        let language = detect_language(source);
        let evidence = self
            .evaluated
            .get(&language)
            .ok_or(CodeCodecEvaluationError::GateNotSatisfied)?;
        if language != CodeLanguage::Rust {
            return Err(CodeCodecEvaluationError::LanguageNotImplemented);
        }
        if !evidence.parser_revision.starts_with("syn@2.") {
            return Err(CodeCodecEvaluationError::ParserRevisionMismatch);
        }
        let (body, fenced) = rust_source_body(source);
        let parsed = syn::parse_file(body).map_err(|_| CodeCodecEvaluationError::ParseFailed)?;
        let rendered = parsed.into_token_stream().to_string();
        syn::parse_file(&rendered).map_err(|_| CodeCodecEvaluationError::ParseFailed)?;
        let candidate_source = if fenced {
            format!("```rust\n{rendered}\n```")
        } else {
            rendered
        };
        if candidate_source.len() >= source.len() {
            return Err(CodeCodecEvaluationError::NonBeneficial);
        }
        Ok(CodeCodecEvaluationCandidate {
            language,
            parser_revision: evidence.parser_revision.clone(),
            corpus_digest: evidence.corpus_digest.clone(),
            original_digest: sha256_hex(source.as_bytes()),
            candidate_digest: sha256_hex(candidate_source.as_bytes()),
            original_bytes: source.len(),
            candidate_source,
        })
    }
}

fn rust_source_body(source: &str) -> (&str, bool) {
    let trimmed = source.trim();
    if trimmed.starts_with("```") {
        let body_start = trimmed.find('\n').map_or(trimmed.len(), |index| index + 1);
        let body_end = trimmed.rfind("```").unwrap_or(trimmed.len());
        return (&trimmed[body_start..body_end], true);
    }
    (source, false)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_evidence(evidence: &CodeCodecGateEvidence) -> Result<(), CodeCodecGateError> {
    if evidence.language == CodeLanguage::Unknown {
        return Err(CodeCodecGateError::UnknownLanguage);
    }
    if evidence.parser_revision.trim().is_empty()
        || !evidence.parser_revision.contains('@')
        || evidence.parser_revision.chars().any(char::is_whitespace)
    {
        return Err(CodeCodecGateError::InvalidParserRevision);
    }
    if evidence.corpus_digest.len() != 64
        || !evidence
            .corpus_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(CodeCodecGateError::InvalidCorpusDigest);
    }
    if evidence.syntax_cases < 500 {
        return Err(CodeCodecGateError::InsufficientSyntaxCases);
    }
    if evidence.semantic_cases < 100 {
        return Err(CodeCodecGateError::InsufficientSemanticCases);
    }
    if evidence.syntax_failures != 0 || evidence.semantic_failures != 0 {
        return Err(CodeCodecGateError::EvaluationFailures);
    }
    Ok(())
}

fn detect_language(source: &str) -> CodeLanguage {
    let first = source.lines().next().unwrap_or_default().trim();
    match first.strip_prefix("```").unwrap_or_default().trim() {
        "rust" | "rs" => CodeLanguage::Rust,
        "typescript" | "ts" | "tsx" => CodeLanguage::TypeScript,
        "javascript" | "js" | "jsx" => CodeLanguage::JavaScript,
        "python" | "py" => CodeLanguage::Python,
        "go" | "golang" => CodeLanguage::Go,
        "java" => CodeLanguage::Java,
        "c" => CodeLanguage::C,
        "cpp" | "c++" | "cc" | "cxx" => CodeLanguage::Cpp,
        _ => detect_unfenced_language(source),
    }
}

fn detect_unfenced_language(source: &str) -> CodeLanguage {
    let trimmed = source.trim_start();
    if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
        CodeLanguage::Rust
    } else if trimmed.starts_with("def ") || trimmed.starts_with("class ") && source.contains(':') {
        CodeLanguage::Python
    } else if trimmed.starts_with("package ") && source.contains("func ") {
        CodeLanguage::Go
    } else if source.contains("interface ") && source.contains(':') {
        CodeLanguage::TypeScript
    } else if source.contains("function ") || source.contains("const ") {
        CodeLanguage::JavaScript
    } else {
        CodeLanguage::Unknown
    }
}
