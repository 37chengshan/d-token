use quote::ToTokens;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

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
    Php,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeCodecAvailability {
    Disabled,
    EvaluationOnly,
    ProductionSafe,
}

pub const TREE_SITTER_SEMANTIC_REVISION: &str = "tree-sitter-ast-equality@0.24.7";
pub const RUST_PRODUCTION_PARSER_REVISION: &str = "syn@2.0.0";
pub const RUST_PRODUCTION_COMPILER_REVISION: &str = "rustc@1.95.0";
pub const RUST_PRODUCTION_CORPUS_DIGEST: &str =
    "b09043c3a9d01b334d89d443663230fffca5aebb3f7f637e801e538ee2f42cc6";
pub const TYPESCRIPT_PRODUCTION_PARSER_REVISION: &str = "tree-sitter-typescript@0.23.2";
pub const TYPESCRIPT_PRODUCTION_CORPUS_DIGEST: &str =
    "f83782e172cb09b6c96941113d34afc82f37780a883b106b5a7a09f93a0eb320";
pub const JAVASCRIPT_PRODUCTION_PARSER_REVISION: &str = "tree-sitter-javascript@0.23.1";
pub const JAVASCRIPT_PRODUCTION_CORPUS_DIGEST: &str =
    "066305465330680975adfb97f3e84e8a055e45edcb0a9f3b5b2eb2a8fdaad649";
pub const PYTHON_PRODUCTION_PARSER_REVISION: &str = "tree-sitter-python@0.23.6";
pub const PYTHON_PRODUCTION_CORPUS_DIGEST: &str =
    "4cb8f0a553746a83fa759e7328c417f161cc1fa02e5264dd03b47ca3a19acdcf";
pub const GO_PRODUCTION_PARSER_REVISION: &str = "tree-sitter-go@0.23.4";
pub const GO_PRODUCTION_CORPUS_DIGEST: &str =
    "3c41964598f7ff38187c1ec6cca06c9b65f549e68804964e9fbf8586076c222e";
pub const JAVA_PRODUCTION_PARSER_REVISION: &str = "tree-sitter-java@0.23.5";
pub const JAVA_PRODUCTION_CORPUS_DIGEST: &str =
    "6092369ae3978c1e1b39cd58df4eec096f169032bb349effec18b5440ddc4832";
pub const C_PRODUCTION_PARSER_REVISION: &str = "tree-sitter-c@0.23.4";
pub const C_PRODUCTION_CORPUS_DIGEST: &str =
    "423ea3018e998cd88187071cc6eeb106d2e038d6166f3349404d3654649a6b70";
pub const CPP_PRODUCTION_PARSER_REVISION: &str = "tree-sitter-cpp@0.23.4";
pub const CPP_PRODUCTION_CORPUS_DIGEST: &str =
    "1347ab5763ba864490fc13392b1660619699e4d916402339dfb2b80f942608aa";
pub const PHP_PRODUCTION_PARSER_REVISION: &str = "tree-sitter-php@0.23.11";
pub const PHP_PRODUCTION_CORPUS_DIGEST: &str =
    "775dbd1067214b9a946dc0cedba65dab5af6d444dd5cdbb43382187aca0fb931";

const TREE_SITTER_TIMEOUT_US: u64 = 50_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeCodecGateEvidence {
    pub language: CodeLanguage,
    pub parser_revision: String,
    /// Rust uses a real compiler revision. Tree-sitter-backed languages pin
    /// the structural-equivalence validator contract here instead of
    /// pretending to have native compiler evidence.
    pub compiler_revision: String,
    pub corpus_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeCodecGateError {
    UnknownLanguage,
    InvalidParserRevision,
    InvalidCompilerRevision,
    InvalidCorpusDigest,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeCodecProductionError {
    GateNotSatisfied,
    LanguageNotImplemented,
    Cancelled,
    TimedOut,
    SecretLikeContent,
    LineSensitiveContent,
    MalformedFence,
    ParseFailed,
    SemanticVerificationFailed,
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

#[derive(Clone, PartialEq, Eq)]
pub struct CodeCodecProductionCandidate {
    language: CodeLanguage,
    parser_revision: String,
    compiler_revision: String,
    corpus_digest: String,
    original_digest: String,
    candidate_digest: String,
    original_bytes: usize,
    candidate_source: String,
    fence_label: String,
    newline_delimiter: String,
    leading_trimmed_lines: Vec<String>,
    trailing_trimmed_lines: Vec<String>,
}

impl std::fmt::Debug for CodeCodecProductionCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodeCodecProductionCandidate")
            .field("language", &self.language)
            .field("parser_revision", &self.parser_revision)
            .field("compiler_revision", &self.compiler_revision)
            .field("corpus_digest", &self.corpus_digest)
            .field("original_digest", &self.original_digest)
            .field("candidate_digest", &self.candidate_digest)
            .field("original_bytes", &self.original_bytes)
            .field("candidate_bytes", &self.candidate_source.len())
            .field("transform", &"code-fence-envelope-trim-v1")
            .field(
                "leading_trimmed_line_count",
                &self.leading_trimmed_lines.len(),
            )
            .field(
                "trailing_trimmed_line_count",
                &self.trailing_trimmed_lines.len(),
            )
            .finish()
    }
}

impl CodeCodecProductionCandidate {
    pub fn language(&self) -> CodeLanguage {
        self.language
    }

    pub fn parser_revision(&self) -> &str {
        &self.parser_revision
    }

    pub fn compiler_revision(&self) -> &str {
        &self.compiler_revision
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

    pub fn fence_label(&self) -> &str {
        &self.fence_label
    }

    pub fn newline_delimiter(&self) -> &str {
        &self.newline_delimiter
    }

    pub fn leading_trimmed_lines(&self) -> &[String] {
        &self.leading_trimmed_lines
    }

    pub fn trailing_trimmed_lines(&self) -> &[String] {
        &self.trailing_trimmed_lines
    }

    pub fn reconstruct_source(&self) -> String {
        let body = reconstruct_body(
            &self.candidate_source,
            &self.fence_label,
            &self.newline_delimiter,
            &self.leading_trimmed_lines,
            &self.trailing_trimmed_lines,
        );
        format!(
            "```{}{}{}{}```",
            self.fence_label, self.newline_delimiter, body, self.newline_delimiter
        )
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

    pub fn reason_code(&self) -> &'static str {
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
            Some(evidence) if production_gate_satisfied(evidence) => {
                match self.prepare_production_candidate(source) {
                    Ok(_) => CodeCodecDecision {
                        language,
                        availability: CodeCodecAvailability::ProductionSafe,
                        reason_code: "code-fence-envelope-trim-v1",
                        parser_revision: Some(evidence.parser_revision.clone()),
                        corpus_digest: Some(evidence.corpus_digest.clone()),
                    },
                    Err(error) => CodeCodecDecision {
                        language,
                        availability: CodeCodecAvailability::EvaluationOnly,
                        reason_code: reason_code_for_production_error(&error),
                        parser_revision: Some(evidence.parser_revision.clone()),
                        corpus_digest: Some(evidence.corpus_digest.clone()),
                    },
                }
            }
            Some(evidence) => CodeCodecDecision {
                language,
                availability: CodeCodecAvailability::EvaluationOnly,
                reason_code: "production-evidence-incomplete",
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

    pub fn prepare_production_candidate(
        &self,
        source: &str,
    ) -> Result<CodeCodecProductionCandidate, CodeCodecProductionError> {
        self.prepare_production_candidate_with_cancel(source, || false)
    }

    pub fn prepare_production_candidate_with_cancel<F>(
        &self,
        source: &str,
        mut is_cancelled: F,
    ) -> Result<CodeCodecProductionCandidate, CodeCodecProductionError>
    where
        F: FnMut() -> bool,
    {
        if is_cancelled() {
            return Err(CodeCodecProductionError::Cancelled);
        }
        let language = detect_language(source);
        let evidence = self
            .evaluated
            .get(&language)
            .ok_or(CodeCodecProductionError::GateNotSatisfied)?;
        if !production_gate_satisfied(evidence) {
            return Err(CodeCodecProductionError::GateNotSatisfied);
        }
        if contains_secret_like_content(source) {
            return Err(CodeCodecProductionError::SecretLikeContent);
        }
        let fenced = strict_fenced_source(source, language)?;
        if contains_line_sensitive_content(language, fenced.body) {
            return Err(CodeCodecProductionError::LineSensitiveContent);
        }
        let trim = trim_blank_line_envelope(fenced.body, fenced.newline_delimiter);
        if trim.trimmed_body == fenced.body {
            return Err(CodeCodecProductionError::NonBeneficial);
        }
        if is_cancelled() {
            return Err(CodeCodecProductionError::Cancelled);
        }
        verify_candidate_semantics(language, fenced.body, &trim.trimmed_body, &mut is_cancelled)?;
        let candidate_source = fenced.render_candidate(&trim.trimmed_body);
        if candidate_source.len() >= source.len() {
            return Err(CodeCodecProductionError::NonBeneficial);
        }
        Ok(CodeCodecProductionCandidate {
            language,
            parser_revision: evidence.parser_revision.clone(),
            compiler_revision: evidence.compiler_revision.clone(),
            corpus_digest: evidence.corpus_digest.clone(),
            original_digest: sha256_hex(source.as_bytes()),
            candidate_digest: sha256_hex(candidate_source.as_bytes()),
            original_bytes: source.len(),
            candidate_source,
            fence_label: fenced.fence_label.to_string(),
            newline_delimiter: fenced.newline_delimiter.to_string(),
            leading_trimmed_lines: trim.leading_trimmed_lines,
            trailing_trimmed_lines: trim.trailing_trimmed_lines,
        })
    }
}

static PRODUCTION_REGISTRY: OnceLock<CodeCodecRegistry> = OnceLock::new();

pub fn production_registry() -> &'static CodeCodecRegistry {
    PRODUCTION_REGISTRY.get_or_init(|| {
        CodeCodecRegistry::from_evidence([
            rust_production_evidence(),
            tree_sitter_production_evidence(
                CodeLanguage::TypeScript,
                TYPESCRIPT_PRODUCTION_PARSER_REVISION,
                TYPESCRIPT_PRODUCTION_CORPUS_DIGEST,
            ),
            tree_sitter_production_evidence(
                CodeLanguage::JavaScript,
                JAVASCRIPT_PRODUCTION_PARSER_REVISION,
                JAVASCRIPT_PRODUCTION_CORPUS_DIGEST,
            ),
            tree_sitter_production_evidence(
                CodeLanguage::Python,
                PYTHON_PRODUCTION_PARSER_REVISION,
                PYTHON_PRODUCTION_CORPUS_DIGEST,
            ),
            tree_sitter_production_evidence(
                CodeLanguage::Go,
                GO_PRODUCTION_PARSER_REVISION,
                GO_PRODUCTION_CORPUS_DIGEST,
            ),
            tree_sitter_production_evidence(
                CodeLanguage::Java,
                JAVA_PRODUCTION_PARSER_REVISION,
                JAVA_PRODUCTION_CORPUS_DIGEST,
            ),
            tree_sitter_production_evidence(
                CodeLanguage::C,
                C_PRODUCTION_PARSER_REVISION,
                C_PRODUCTION_CORPUS_DIGEST,
            ),
            tree_sitter_production_evidence(
                CodeLanguage::Cpp,
                CPP_PRODUCTION_PARSER_REVISION,
                CPP_PRODUCTION_CORPUS_DIGEST,
            ),
            tree_sitter_production_evidence(
                CodeLanguage::Php,
                PHP_PRODUCTION_PARSER_REVISION,
                PHP_PRODUCTION_CORPUS_DIGEST,
            ),
        ])
        .expect("pinned production evidence must remain internally valid")
    })
}

fn rust_production_evidence() -> CodeCodecGateEvidence {
    CodeCodecGateEvidence {
        language: CodeLanguage::Rust,
        parser_revision: RUST_PRODUCTION_PARSER_REVISION.to_string(),
        compiler_revision: RUST_PRODUCTION_COMPILER_REVISION.to_string(),
        corpus_digest: RUST_PRODUCTION_CORPUS_DIGEST.to_string(),
    }
}

fn tree_sitter_production_evidence(
    language: CodeLanguage,
    parser_revision: &'static str,
    corpus_digest: &'static str,
) -> CodeCodecGateEvidence {
    CodeCodecGateEvidence {
        language,
        parser_revision: parser_revision.to_string(),
        compiler_revision: TREE_SITTER_SEMANTIC_REVISION.to_string(),
        corpus_digest: corpus_digest.to_string(),
    }
}

fn production_gate_satisfied(evidence: &CodeCodecGateEvidence) -> bool {
    match evidence.language {
        CodeLanguage::Rust => {
            evidence.parser_revision == RUST_PRODUCTION_PARSER_REVISION
                && evidence.compiler_revision == RUST_PRODUCTION_COMPILER_REVISION
                && evidence.corpus_digest == RUST_PRODUCTION_CORPUS_DIGEST
        }
        CodeLanguage::TypeScript => tree_sitter_gate_matches(
            evidence,
            TYPESCRIPT_PRODUCTION_PARSER_REVISION,
            TYPESCRIPT_PRODUCTION_CORPUS_DIGEST,
        ),
        CodeLanguage::JavaScript => tree_sitter_gate_matches(
            evidence,
            JAVASCRIPT_PRODUCTION_PARSER_REVISION,
            JAVASCRIPT_PRODUCTION_CORPUS_DIGEST,
        ),
        CodeLanguage::Python => tree_sitter_gate_matches(
            evidence,
            PYTHON_PRODUCTION_PARSER_REVISION,
            PYTHON_PRODUCTION_CORPUS_DIGEST,
        ),
        CodeLanguage::Go => tree_sitter_gate_matches(
            evidence,
            GO_PRODUCTION_PARSER_REVISION,
            GO_PRODUCTION_CORPUS_DIGEST,
        ),
        CodeLanguage::Java => tree_sitter_gate_matches(
            evidence,
            JAVA_PRODUCTION_PARSER_REVISION,
            JAVA_PRODUCTION_CORPUS_DIGEST,
        ),
        CodeLanguage::C => tree_sitter_gate_matches(
            evidence,
            C_PRODUCTION_PARSER_REVISION,
            C_PRODUCTION_CORPUS_DIGEST,
        ),
        CodeLanguage::Cpp => tree_sitter_gate_matches(
            evidence,
            CPP_PRODUCTION_PARSER_REVISION,
            CPP_PRODUCTION_CORPUS_DIGEST,
        ),
        CodeLanguage::Php => tree_sitter_gate_matches(
            evidence,
            PHP_PRODUCTION_PARSER_REVISION,
            PHP_PRODUCTION_CORPUS_DIGEST,
        ),
        CodeLanguage::Unknown => false,
    }
}

fn tree_sitter_gate_matches(
    evidence: &CodeCodecGateEvidence,
    parser_revision: &'static str,
    corpus_digest: &'static str,
) -> bool {
    evidence.parser_revision == parser_revision
        && evidence.compiler_revision == TREE_SITTER_SEMANTIC_REVISION
        && evidence.corpus_digest == corpus_digest
}

fn reason_code_for_production_error(error: &CodeCodecProductionError) -> &'static str {
    match error {
        CodeCodecProductionError::GateNotSatisfied => "production-evidence-incomplete",
        CodeCodecProductionError::LanguageNotImplemented => "code-language-not-implemented",
        CodeCodecProductionError::Cancelled => "code-transform-cancelled",
        CodeCodecProductionError::TimedOut => "code-transform-timeout",
        CodeCodecProductionError::SecretLikeContent => "secret-preserve-code",
        CodeCodecProductionError::LineSensitiveContent => "line-sensitive-preserve-code",
        CodeCodecProductionError::MalformedFence => "strict-fence-required",
        CodeCodecProductionError::ParseFailed => "source-parse-failed",
        CodeCodecProductionError::SemanticVerificationFailed => {
            "code-ast-equivalence-verification-failed"
        }
        CodeCodecProductionError::NonBeneficial => "source-not-production-eligible",
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

#[derive(Debug, Clone, Copy)]
struct StrictFencedSource<'a> {
    fence_label: &'a str,
    newline_delimiter: &'a str,
    body: &'a str,
}

impl StrictFencedSource<'_> {
    fn render_candidate(&self, candidate_body: &str) -> String {
        format!(
            "```{}{}{}{}```",
            self.fence_label, self.newline_delimiter, candidate_body, self.newline_delimiter
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnvelopeTrimResult {
    trimmed_body: String,
    leading_trimmed_lines: Vec<String>,
    trailing_trimmed_lines: Vec<String>,
}

fn strict_fenced_source(
    source: &str,
    expected_language: CodeLanguage,
) -> Result<StrictFencedSource<'_>, CodeCodecProductionError> {
    let header = source
        .strip_prefix("```")
        .ok_or(CodeCodecProductionError::MalformedFence)?;
    let Some(newline_index) = header.find('\n') else {
        return Err(CodeCodecProductionError::MalformedFence);
    };
    let header_line = &header[..newline_index];
    let (fence_label, newline_delimiter) = if let Some(stripped) = header_line.strip_suffix('\r') {
        (stripped.trim(), "\r\n")
    } else {
        (header_line.trim(), "\n")
    };
    if fence_label.is_empty() || language_for_label(fence_label) != Some(expected_language) {
        return Err(CodeCodecProductionError::MalformedFence);
    }
    let body_start = 3 + newline_index + 1;
    let closing = format!("{newline_delimiter}```");
    let body = source
        .strip_suffix(&closing)
        .and_then(|prefix| prefix.get(body_start..))
        .ok_or(CodeCodecProductionError::MalformedFence)?;
    if body.contains(&closing) || has_mixed_newlines(body, newline_delimiter) {
        return Err(CodeCodecProductionError::MalformedFence);
    }
    Ok(StrictFencedSource {
        fence_label,
        newline_delimiter,
        body,
    })
}

fn has_mixed_newlines(body: &str, newline_delimiter: &str) -> bool {
    match newline_delimiter {
        "\n" => body.contains('\r'),
        "\r\n" => body.contains('\r') && body.replace("\r\n", "").contains('\r'),
        _ => true,
    }
}

fn trim_blank_line_envelope(body: &str, newline_delimiter: &str) -> EnvelopeTrimResult {
    let lines = if body.is_empty() {
        Vec::new()
    } else {
        body.split(newline_delimiter).collect::<Vec<_>>()
    };
    let mut start = 0usize;
    let mut end = lines.len();
    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    EnvelopeTrimResult {
        trimmed_body: lines[start..end].join(newline_delimiter),
        leading_trimmed_lines: lines[..start]
            .iter()
            .map(|line| (*line).to_string())
            .collect(),
        trailing_trimmed_lines: lines[end..]
            .iter()
            .map(|line| (*line).to_string())
            .collect(),
    }
}

fn reconstruct_body(
    candidate_source: &str,
    fence_label: &str,
    newline_delimiter: &str,
    leading_trimmed_lines: &[String],
    trailing_trimmed_lines: &[String],
) -> String {
    let candidate_body = candidate_source
        .strip_prefix(&format!("```{fence_label}{newline_delimiter}"))
        .and_then(|value| value.strip_suffix(&format!("{newline_delimiter}```")))
        .unwrap_or_default();
    let mut lines = Vec::new();
    lines.extend(leading_trimmed_lines.iter().cloned());
    if !candidate_body.is_empty() {
        lines.extend(
            candidate_body
                .split(newline_delimiter)
                .map(ToOwned::to_owned),
        );
    }
    lines.extend(trailing_trimmed_lines.iter().cloned());
    lines.join(newline_delimiter)
}

fn verify_candidate_semantics<F>(
    language: CodeLanguage,
    original_body: &str,
    candidate_body: &str,
    is_cancelled: &mut F,
) -> Result<(), CodeCodecProductionError>
where
    F: FnMut() -> bool,
{
    if is_cancelled() {
        return Err(CodeCodecProductionError::Cancelled);
    }
    match language {
        CodeLanguage::Rust => verify_rust_equivalence(original_body, candidate_body),
        CodeLanguage::TypeScript
        | CodeLanguage::JavaScript
        | CodeLanguage::Python
        | CodeLanguage::Go
        | CodeLanguage::Java
        | CodeLanguage::C
        | CodeLanguage::Cpp
        | CodeLanguage::Php => {
            verify_tree_sitter_equivalence(language, original_body, candidate_body, is_cancelled)
        }
        CodeLanguage::Unknown => Err(CodeCodecProductionError::LanguageNotImplemented),
    }
}

fn verify_rust_equivalence(
    original_body: &str,
    candidate_body: &str,
) -> Result<(), CodeCodecProductionError> {
    let original =
        syn::parse_file(original_body).map_err(|_| CodeCodecProductionError::ParseFailed)?;
    let candidate =
        syn::parse_file(candidate_body).map_err(|_| CodeCodecProductionError::ParseFailed)?;
    if token_signature(&original) != token_signature(&candidate) {
        return Err(CodeCodecProductionError::SemanticVerificationFailed);
    }
    Ok(())
}

fn verify_tree_sitter_equivalence<F>(
    language: CodeLanguage,
    original_body: &str,
    candidate_body: &str,
    is_cancelled: &mut F,
) -> Result<(), CodeCodecProductionError>
where
    F: FnMut() -> bool,
{
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_language(language)?)
        .map_err(|_| CodeCodecProductionError::ParseFailed)?;
    parser.set_timeout_micros(TREE_SITTER_TIMEOUT_US);
    let original_tree = parser
        .parse(original_body, None)
        .ok_or(CodeCodecProductionError::TimedOut)?;
    if has_tree_sitter_error(original_tree.root_node()) {
        return Err(CodeCodecProductionError::ParseFailed);
    }
    if is_cancelled() {
        return Err(CodeCodecProductionError::Cancelled);
    }
    let candidate_tree = parser
        .parse(candidate_body, None)
        .ok_or(CodeCodecProductionError::TimedOut)?;
    if has_tree_sitter_error(candidate_tree.root_node()) {
        return Err(CodeCodecProductionError::ParseFailed);
    }
    if is_cancelled() {
        return Err(CodeCodecProductionError::Cancelled);
    }
    if original_tree.root_node().to_sexp() != candidate_tree.root_node().to_sexp() {
        return Err(CodeCodecProductionError::SemanticVerificationFailed);
    }
    Ok(())
}

fn tree_sitter_language(
    language: CodeLanguage,
) -> Result<tree_sitter::Language, CodeCodecProductionError> {
    match language {
        CodeLanguage::TypeScript => Ok(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        CodeLanguage::JavaScript => Ok(tree_sitter_javascript::LANGUAGE.into()),
        CodeLanguage::Python => Ok(tree_sitter_python::LANGUAGE.into()),
        CodeLanguage::Go => Ok(tree_sitter_go::LANGUAGE.into()),
        CodeLanguage::Java => Ok(tree_sitter_java::LANGUAGE.into()),
        CodeLanguage::C => Ok(tree_sitter_c::LANGUAGE.into()),
        CodeLanguage::Cpp => Ok(tree_sitter_cpp::LANGUAGE.into()),
        CodeLanguage::Php => Ok(tree_sitter_php::LANGUAGE_PHP.into()),
        _ => Err(CodeCodecProductionError::LanguageNotImplemented),
    }
}

fn has_tree_sitter_error(node: tree_sitter::Node<'_>) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    for index in 0..node.child_count() {
        if let Some(child) = node.child(index) {
            if has_tree_sitter_error(child) {
                return true;
            }
        }
    }
    false
}

fn token_signature(file: &syn::File) -> String {
    file.into_token_stream().to_string()
}

fn contains_secret_like_content(source: &str) -> bool {
    let normalized = source.to_ascii_lowercase();
    [
        "authorization:",
        "x-api-key:",
        "api_key",
        "api-key",
        "-----begin",
        "private key-----",
        "sk-",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn contains_line_sensitive_content(language: CodeLanguage, body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    match language {
        CodeLanguage::Rust => ["line!(", "column!(", "file!("]
            .iter()
            .any(|marker| body.contains(marker)),
        CodeLanguage::TypeScript | CodeLanguage::JavaScript => [
            "sourcemappingurl=",
            "sourceurl=",
            "//# sourceMappingURL",
            "//@ sourceMappingURL",
        ]
        .iter()
        .any(|marker| lower.contains(&marker.to_ascii_lowercase())),
        CodeLanguage::Python => lower.contains("traceback (most recent call last):"),
        CodeLanguage::Go => body.lines().any(|line| line.trim_start().starts_with("//line ")),
        CodeLanguage::Java => {
            lower.contains("exception in thread ") || body.lines().any(is_java_stacktrace_line)
        }
        CodeLanguage::C | CodeLanguage::Cpp => {
            body.contains("__LINE__")
                || body.contains("__FILE__")
                || body
                    .lines()
                    .any(|line| matches!(line.trim_start(), value if value.starts_with("#line") || value.starts_with("# line")))
        }
        CodeLanguage::Php => {
            lower.contains("__line__")
                || lower.contains("__file__")
                || body.lines().any(|line| {
                    matches!(line.trim_start(), value if value.starts_with("//# sourceMappingURL"))
                })
        }
        CodeLanguage::Unknown => true,
    }
}

fn is_java_stacktrace_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("at ") && trimmed.contains(".java:")
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
    if evidence.compiler_revision.trim().is_empty()
        || !evidence.compiler_revision.contains('@')
        || evidence.compiler_revision.chars().any(char::is_whitespace)
    {
        return Err(CodeCodecGateError::InvalidCompilerRevision);
    }
    if evidence.corpus_digest.len() != 64
        || !evidence
            .corpus_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(CodeCodecGateError::InvalidCorpusDigest);
    }
    Ok(())
}

fn detect_language(source: &str) -> CodeLanguage {
    let first = source.lines().next().unwrap_or_default().trim();
    match first.strip_prefix("```").unwrap_or_default().trim() {
        "rust" | "rs" => CodeLanguage::Rust,
        "typescript" | "ts" | "tsx" => CodeLanguage::TypeScript,
        "javascript" | "js" | "jsx" | "mjs" | "cjs" => CodeLanguage::JavaScript,
        "python" | "py" => CodeLanguage::Python,
        "go" | "golang" => CodeLanguage::Go,
        "java" => CodeLanguage::Java,
        "c" => CodeLanguage::C,
        "cpp" | "c++" | "cc" | "cxx" => CodeLanguage::Cpp,
        "php" | "phtml" | "php5" | "php7" | "php8" => CodeLanguage::Php,
        _ => detect_unfenced_language(source),
    }
}

fn language_for_label(label: &str) -> Option<CodeLanguage> {
    match label {
        "rust" | "rs" => Some(CodeLanguage::Rust),
        "typescript" | "ts" | "tsx" => Some(CodeLanguage::TypeScript),
        "javascript" | "js" | "jsx" | "mjs" | "cjs" => Some(CodeLanguage::JavaScript),
        "python" | "py" => Some(CodeLanguage::Python),
        "go" | "golang" => Some(CodeLanguage::Go),
        "java" => Some(CodeLanguage::Java),
        "c" => Some(CodeLanguage::C),
        "cpp" | "c++" | "cc" | "cxx" => Some(CodeLanguage::Cpp),
        "php" | "phtml" | "php5" | "php7" | "php8" => Some(CodeLanguage::Php),
        _ => None,
    }
}

fn detect_unfenced_language(source: &str) -> CodeLanguage {
    let trimmed = source.trim_start();
    if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
        CodeLanguage::Rust
    } else if trimmed.starts_with("def ") || (trimmed.starts_with("class ") && source.contains(':'))
    {
        CodeLanguage::Python
    } else if trimmed.starts_with("package ") && source.contains("func ") {
        CodeLanguage::Go
    } else if source.contains("interface ") && source.contains(':') {
        CodeLanguage::TypeScript
    } else if source.contains("function ") || source.contains("const ") {
        CodeLanguage::JavaScript
    } else if trimmed.starts_with("<?php") || source.contains("<?php ") {
        CodeLanguage::Php
    } else {
        CodeLanguage::Unknown
    }
}
