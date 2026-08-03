use crate::code_codec::{production_registry, CodeCodecAvailability};
use dtoken_types::CompressionPolicy;
use serde_json::Value;

const MAX_ROUTABLE_TEXT_BYTES: usize = 256 * 1024;
const MIN_ROUTING_DEADLINE_US: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Json,
    Log,
    SearchResults,
    Diff,
    Html,
    BuildOutput,
    Code,
    Config,
    Text,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind {
    JsonMinify,
    CodeFenceEnvelopeTrim,
    LogFold,
    SearchFold,
    BuildFold,
    DiffFold,
    TextRepeatFold,
    HtmlRepeatFold,
    ConfigFold,
    Passthrough,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentRouteDecision {
    pub content_kind: ContentKind,
    pub codec: CodecKind,
    pub reason_code: &'static str,
    pub confidence: u8,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutingBudget {
    pub max_input_bytes: usize,
    pub deadline_remaining_us: Option<u64>,
}

impl Default for RoutingBudget {
    fn default() -> Self {
        Self {
            max_input_bytes: MAX_ROUTABLE_TEXT_BYTES,
            deadline_remaining_us: None,
        }
    }
}

pub fn route_content(text: &str) -> ContentRouteDecision {
    route_content_with_budget(text, RoutingBudget::default())
}

pub fn route_content_with_budget(text: &str, budget: RoutingBudget) -> ContentRouteDecision {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return decision(
            ContentKind::Unknown,
            CodecKind::Passthrough,
            "empty",
            100,
            text.len(),
        );
    }
    if text.len() > budget.max_input_bytes {
        return decision(
            ContentKind::Unknown,
            CodecKind::Passthrough,
            "size-limit",
            100,
            text.len(),
        );
    }
    if budget
        .deadline_remaining_us
        .is_some_and(|remaining| remaining < MIN_ROUTING_DEADLINE_US)
    {
        return decision(
            ContentKind::Unknown,
            CodecKind::Passthrough,
            "deadline-budget",
            100,
            text.len(),
        );
    }
    if crate::optical::secret::contains_secret_like(text) {
        return decision(
            ContentKind::Unknown,
            CodecKind::Passthrough,
            "secret-preserve-text",
            100,
            text.len(),
        );
    }
    if text.lines().any(has_repeat_marker) {
        return decision(
            ContentKind::Unknown,
            CodecKind::Passthrough,
            "codec-marker-preserve-text",
            100,
            text.len(),
        );
    }
    if is_already_compressed(text) {
        return decision(
            ContentKind::Unknown,
            CodecKind::Passthrough,
            "ccr-retrieval-marker-preserve",
            100,
            text.len(),
        );
    }
    let risk = crate::optical::risk::classify_block(text);
    let exact_preserve = risk.tier == crate::optical::risk::RiskTier::ExactTextRequired
        || risk
            .anchors
            .as_ref()
            .is_some_and(|anchors| anchors.anchor_count > 0);
    if parse_json_container(trimmed).is_some() {
        return decision(
            ContentKind::Json,
            CodecKind::JsonMinify,
            "json-container",
            100,
            text.len(),
        );
    }
    if looks_like_diff(trimmed) {
        let safe_diff = unified_diff_repeat_fold_is_safe(trimmed);
        return decision(
            ContentKind::Diff,
            if safe_diff {
                CodecKind::DiffFold
            } else {
                CodecKind::Passthrough
            },
            if safe_diff {
                "unified-diff-context-repeat-fold"
            } else if looks_like_valid_unified_diff(trimmed) {
                "unified-diff-preserve-text"
            } else {
                "malformed-diff-passthrough"
            },
            98,
            text.len(),
        );
    }
    if looks_like_code(trimmed) {
        let code_decision = production_registry().decision_for(text);
        return decision(
            ContentKind::Code,
            if code_decision.availability() == CodeCodecAvailability::ProductionSafe {
                CodecKind::CodeFenceEnvelopeTrim
            } else {
                CodecKind::Passthrough
            },
            if code_decision.availability() == CodeCodecAvailability::ProductionSafe {
                "code-fence-envelope-trim-v1"
            } else {
                code_decision.reason_code()
            },
            95,
            text.len(),
        );
    }
    if looks_like_search_results(trimmed) {
        let safe_repeat = search_repeat_fold_is_safe(text);
        return decision(
            ContentKind::SearchResults,
            if safe_repeat {
                CodecKind::SearchFold
            } else {
                CodecKind::Passthrough
            },
            if safe_repeat {
                "search-body-repeat-fold"
            } else {
                "exact-anchor-preserve-text"
            },
            100,
            text.len(),
        );
    }
    if risk.tier == crate::optical::risk::RiskTier::StructuralUnsafe {
        return decision(
            ContentKind::Unknown,
            CodecKind::Passthrough,
            "structural-preserve-text",
            100,
            text.len(),
        );
    }
    if looks_like_html(trimmed) {
        if exact_preserve {
            return decision(
                ContentKind::Html,
                CodecKind::Passthrough,
                "exact-anchor-preserve-text",
                100,
                text.len(),
            );
        }
        let safe_repeat = html_repeat_fold_is_safe(trimmed);
        return decision(
            ContentKind::Html,
            if safe_repeat {
                CodecKind::HtmlRepeatFold
            } else {
                CodecKind::Passthrough
            },
            if safe_repeat {
                "safe-html-repeat-fold"
            } else {
                "html-safe-parser-required"
            },
            95,
            text.len(),
        );
    }
    if looks_like_build_output(trimmed) {
        return decision(
            ContentKind::BuildOutput,
            if exact_preserve {
                CodecKind::Passthrough
            } else {
                CodecKind::BuildFold
            },
            if exact_preserve {
                "exact-anchor-preserve-text"
            } else {
                "structured-build-output"
            },
            if exact_preserve { 100 } else { 90 },
            text.len(),
        );
    }
    if looks_like_log(trimmed) {
        return decision(
            ContentKind::Log,
            if exact_preserve {
                CodecKind::Passthrough
            } else {
                CodecKind::LogFold
            },
            if exact_preserve {
                "exact-anchor-preserve-text"
            } else {
                "structured-or-repeated-log"
            },
            if exact_preserve { 100 } else { 90 },
            text.len(),
        );
    }
    if looks_like_structured_config(trimmed) {
        return decision(
            ContentKind::Config,
            if exact_preserve {
                CodecKind::Passthrough
            } else {
                CodecKind::ConfigFold
            },
            if exact_preserve {
                "exact-anchor-preserve-text"
            } else {
                "structured-config-repeat-fold"
            },
            if exact_preserve { 100 } else { 92 },
            text.len(),
        );
    }
    if exact_preserve {
        return decision(
            ContentKind::Unknown,
            CodecKind::Passthrough,
            "exact-anchor-preserve-text",
            100,
            text.len(),
        );
    }
    if has_adjacent_exact_repeat(trimmed) {
        return decision(
            ContentKind::Text,
            if contains_decision_signal(trimmed) {
                CodecKind::Passthrough
            } else {
                CodecKind::TextRepeatFold
            },
            if contains_decision_signal(trimmed) {
                "decision-sensitive-text"
            } else {
                "repeated-plain-text"
            },
            85,
            text.len(),
        );
    }
    decision(
        ContentKind::Text,
        CodecKind::Passthrough,
        "text-quality-gate-required",
        50,
        text.len(),
    )
}

fn decision(
    content_kind: ContentKind,
    codec: CodecKind,
    reason_code: &'static str,
    confidence: u8,
    size_bytes: usize,
) -> ContentRouteDecision {
    ContentRouteDecision {
        content_kind,
        codec,
        reason_code,
        confidence,
        size_bytes,
    }
}

pub fn is_safe_lossless_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::JsonMinify)
}

pub fn is_safe_log_codec_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::LogFold)
}

pub fn is_safe_search_codec_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::SearchFold)
}

pub fn is_safe_build_codec_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::BuildFold)
}

pub fn is_safe_diff_codec_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::DiffFold)
}

pub fn is_safe_text_repeat_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::TextRepeatFold)
}

pub fn is_safe_html_repeat_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::HtmlRepeatFold)
}

pub fn is_safe_code_codec_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::CodeFenceEnvelopeTrim)
}

pub fn contains_decision_signal(text: &str) -> bool {
    text.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        [
            "final ",
            "final decision",
            "final policy",
            "final invariant",
            "final value",
            "decision:",
            "decided ",
            "selected ",
            "approved ",
            "locked ",
            "locked value",
            "production decision",
            "root cause",
            "must use",
            "must not",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    })
}

pub(crate) fn apply_routed_codec(
    policy: CompressionPolicy,
    text: &str,
    transforms: &mut Vec<String>,
) -> String {
    apply_routed_codec_with_registry(
        crate::compressor_registry::production_compressor_registry(),
        policy,
        text,
        transforms,
    )
}

pub(crate) fn apply_routed_codec_with_registry(
    registry: crate::compressor_registry::ProductionCompressorRegistry,
    policy: CompressionPolicy,
    text: &str,
    transforms: &mut Vec<String>,
) -> String {
    let Some(selection) = registry.select(text, RoutingBudget::default()) else {
        return text.to_string();
    };
    match selection.descriptor.codec {
        CodecKind::JsonMinify => {
            let Some(minified) = minify_json_losslessly(text) else {
                return text.to_string();
            };
            crate::push_transform(transforms, "content_router:json");
            crate::push_transform(transforms, "json_minify_v1");
            minified
        }
        CodecKind::CodeFenceEnvelopeTrim => {
            let Ok(candidate) = production_registry().prepare_production_candidate(text) else {
                return text.to_string();
            };
            crate::push_transform(transforms, "content_router:code");
            crate::push_transform(transforms, "code_fence_envelope_trim_v1");
            candidate.candidate_source().to_string()
        }
        CodecKind::LogFold => {
            // Log routing is deliberately bounded to stable normalization and
            // adjacent-repeat folding. Aggressive/Max's generic middle-text
            // collapse can discard a unique decision line from a long log.
            let _requested_policy = policy;
            let mut codec_transforms = Vec::new();
            let compressed =
                crate::compress_text(CompressionPolicy::Balanced, text, &mut codec_transforms);
            if compressed.len() >= text.len() {
                return text.to_string();
            }
            transforms.extend(codec_transforms);
            crate::push_transform(transforms, "content_router:log");
            crate::push_transform(transforms, "log_fold_v1");
            compressed
        }
        CodecKind::SearchFold => {
            let compressed = fold_search_result_repeats(text);
            if compressed.len() >= text.len()
                || expand_exact_folded_lines_losslessly(&compressed).as_deref() != Some(text)
            {
                return text.to_string();
            }
            crate::push_transform(transforms, "content_router:search");
            crate::push_transform(transforms, "search_fold_v1");
            compressed
        }
        CodecKind::BuildFold => {
            let compressed = fold_exact_repeated_lines(text);
            if compressed.len() >= text.len() {
                return text.to_string();
            }
            crate::push_transform(transforms, "content_router:build");
            crate::push_transform(transforms, "build_fold_v1");
            compressed
        }
        CodecKind::DiffFold => {
            let compressed = fold_unified_diff_context_repeats(text);
            if compressed.len() >= text.len()
                || !unified_diff_repeat_fold_is_safe(text)
                || expand_folded_diff_context_lines(&compressed).as_deref() != Some(text)
            {
                return text.to_string();
            }
            crate::push_transform(transforms, "content_router:diff");
            crate::push_transform(transforms, "diff_context_repeat_fold_v1");
            compressed
        }
        CodecKind::TextRepeatFold => {
            let compressed = fold_exact_repeated_lines(text);
            if compressed.len() >= text.len() {
                return text.to_string();
            }
            crate::push_transform(transforms, "content_router:text");
            crate::push_transform(transforms, "text_repeat_fold_v1");
            compressed
        }
        CodecKind::HtmlRepeatFold => {
            let compressed = fold_exact_repeated_lines(text);
            if compressed.len() >= text.len() {
                return text.to_string();
            }
            crate::push_transform(transforms, "content_router:html");
            crate::push_transform(transforms, "html_repeat_fold_v1");
            compressed
        }
        CodecKind::ConfigFold => {
            let compressed = fold_exact_repeated_lines(text);
            if compressed.len() >= text.len()
                || expand_exact_folded_lines_losslessly(&compressed).as_deref() != Some(text)
            {
                return text.to_string();
            }
            crate::push_transform(transforms, "content_router:config");
            crate::push_transform(transforms, "config_repeat_fold_v1");
            compressed
        }
        CodecKind::Passthrough => text.to_string(),
    }
}

/// IR-safe recursive content routing for mixed/multi-type tool output.
///
/// Splits the text into typed sections, routes each section through the
/// same registry-driven codec decisions as a single-type payload, and
/// reassembles the result.  Every section must round-trip losslessly
/// (its codec candidate must verify against the source); any section that
/// fails verification, is secret-like, carries a CCR retrieval marker, or
/// has no positive benefit is passed through verbatim.  If no section
/// changed, the whole payload passes through.
///
/// The result is evidence-bearing: `transforms` records each applied
/// section codec and the per-section outcome list exposes the routing
/// decision for every section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixedRouteOutcome {
    pub section_count: usize,
    pub sections_changed: usize,
    pub original_bytes: usize,
    pub candidate_bytes: usize,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionRouteRecord {
    pub index: usize,
    pub kind: ContentKind,
    pub codec: CodecKind,
    pub reason_code: &'static str,
    pub original_bytes: usize,
    pub candidate_bytes: usize,
    pub changed: bool,
}

pub fn route_mixed_content(
    policy: CompressionPolicy,
    text: &str,
    transforms: &mut Vec<String>,
) -> (String, Vec<SectionRouteRecord>) {
    if !is_mixed_content(text) || is_already_compressed(text) {
        return (text.to_string(), Vec::new());
    }
    let sections = split_into_sections(text);
    if sections.len() < 2 {
        return (text.to_string(), Vec::new());
    }
    let mut records = Vec::with_capacity(sections.len());
    let mut output = Vec::with_capacity(sections.len());
    let mut sections_changed = 0usize;
    let mut original_bytes = 0usize;
    let mut candidate_bytes = 0usize;
    for (index, section) in sections.iter().enumerate() {
        let section_bytes = section.body.len();
        original_bytes += section_bytes;
        let (candidate, codec, reason) = route_single_section(&policy, section);
        let changed = candidate != section.body;
        if changed {
            sections_changed += 1;
        }
        candidate_bytes += candidate.len();
        records.push(SectionRouteRecord {
            index,
            kind: section.kind,
            codec,
            reason_code: reason,
            original_bytes: section_bytes,
            candidate_bytes: candidate.len(),
            changed,
        });
        output.push(candidate);
    }
    if sections_changed == 0 {
        return (text.to_string(), records);
    }
    crate::push_transform(transforms, "content_router:mixed");
    crate::push_transform(transforms, "mixed_section_route_v1");
    (
        output.join("\n"),
        records,
    )
}

/// Route a single section through the codec decision, with all guards
/// (secret, marker, negative-benefit) enforced per section.
fn route_single_section(
    policy: &CompressionPolicy,
    section: &ContentSection,
) -> (String, CodecKind, &'static str) {
    if section.kind == ContentKind::Code {
        // Rebuild the fenced form so the code codec sees its expected
        // envelope; the fence label is preserved verbatim.
        let label = section.fence_label.as_deref().unwrap_or("");
        let fenced = if section.body.is_empty() {
            format!("```{label}\n\n```")
        } else {
            format!("```{label}\n{}\n```", section.body)
        };
        let mut codec_transforms = Vec::new();
        let candidate = apply_routed_codec(*policy, &fenced, &mut codec_transforms);
        let codec = route_content(&fenced).codec;
        let reason = route_content(&fenced).reason_code;
        let changed = candidate != fenced;
        let body_candidate = if changed {
            strip_fence(&candidate, label)
        } else {
            section.body.clone()
        };
        (body_candidate, codec, reason)
    } else {
        let mut codec_transforms = Vec::new();
        let candidate = apply_routed_codec(*policy, &section.body, &mut codec_transforms);
        let decision = route_content(&section.body);
        (candidate, decision.codec, decision.reason_code)
    }
}

fn strip_fence(candidate: &str, label: &str) -> String {
    let prefix = format!("```{label}\n");
    let suffix = "\n```";
    candidate
        .strip_prefix(&prefix)
        .and_then(|value| value.strip_suffix(suffix))
        .unwrap_or(candidate)
        .to_string()
}

pub fn minify_json_losslessly(text: &str) -> Option<String> {
    let value = parse_json_container(text.trim())?;
    let minified = serde_json::to_string(&value).ok()?;
    (minified.len() < text.len()).then_some(minified)
}

fn fold_exact_repeated_lines(text: &str) -> String {
    let Some(line_ending) = uniform_line_ending(text) else {
        return text.to_string();
    };
    let trailing_line_ending = text.ends_with(line_ending);
    let body = if trailing_line_ending {
        &text[..text.len() - line_ending.len()]
    } else {
        text
    };
    let mut output = Vec::new();
    let mut lines = body.split(line_ending);
    let Some(mut current) = lines.next() else {
        return String::new();
    };
    let mut count = 1usize;
    for line in lines {
        if line == current {
            count += 1;
            continue;
        }
        if count > 1 {
            output.push(format!("{current} [repeated x{count}]"));
        } else {
            output.push(current.to_string());
        }
        current = line;
        count = 1;
    }
    if count > 1 {
        output.push(format!("{current} [repeated x{count}]"));
    } else {
        output.push(current.to_string());
    }
    let mut folded = output.join(line_ending);
    if trailing_line_ending {
        folded.push_str(line_ending);
    }
    folded
}

fn fold_search_result_repeats(text: &str) -> String {
    let Some(line_ending) = uniform_line_ending(text) else {
        return text.to_string();
    };
    let trailing_line_ending = text.ends_with(line_ending);
    let body = if trailing_line_ending {
        &text[..text.len() - line_ending.len()]
    } else {
        text
    };
    let lines = body.split(line_ending).collect::<Vec<_>>();
    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if is_search_anchor_line(line) || has_repeat_marker(line) {
            output.push(line.to_string());
            index += 1;
            continue;
        }
        let mut count = 1usize;
        while index + count < lines.len()
            && lines[index + count] == line
            && !is_search_anchor_line(lines[index + count])
            && !has_repeat_marker(lines[index + count])
        {
            count += 1;
        }
        if count > 1 {
            output.push(format!("{line} [repeated x{count}]"));
        } else {
            output.push(line.to_string());
        }
        index += count;
    }
    let mut folded = output.join(line_ending);
    if trailing_line_ending {
        folded.push_str(line_ending);
    }
    folded
}

pub(crate) fn expand_exact_folded_lines_losslessly(candidate: &str) -> Option<String> {
    let line_ending = uniform_line_ending(candidate)?;
    let trailing_line_ending = candidate.ends_with(line_ending);
    let body = if trailing_line_ending {
        &candidate[..candidate.len() - line_ending.len()]
    } else {
        candidate
    };
    let mut expanded = Vec::new();
    for line in body.split(line_ending) {
        let Some((base, suffix)) = line.rsplit_once(" [repeated x") else {
            expanded.push(line.to_string());
            continue;
        };
        let Some(count) = suffix
            .strip_suffix(']')
            .and_then(|value| value.parse::<usize>().ok())
        else {
            expanded.push(line.to_string());
            continue;
        };
        if !(2..=1_000_000).contains(&count) {
            return None;
        }
        expanded.extend(std::iter::repeat_n(base.to_string(), count));
    }
    let mut source = expanded.join(line_ending);
    if trailing_line_ending {
        source.push_str(line_ending);
    }
    Some(source)
}

fn uniform_line_ending(text: &str) -> Option<&'static str> {
    let bytes = text.as_bytes();
    let mut saw_lf = false;
    let mut saw_crlf = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                if bytes.get(index + 1) != Some(&b'\n') {
                    return None;
                }
                saw_crlf = true;
                index += 2;
            }
            b'\n' => {
                saw_lf = true;
                index += 1;
            }
            _ => index += 1,
        }
        if saw_lf && saw_crlf {
            return None;
        }
    }
    Some(if saw_crlf { "\r\n" } else { "\n" })
}

/// Fold only adjacent identical context lines in a single, fully counted
/// unified diff.  The emitted marker is deliberately a d-token context
/// representation, not an applyable patch; the pipeline must prove exact
/// reconstruction before it may send it upstream.
fn fold_unified_diff_context_repeats(text: &str) -> String {
    let mut output = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if !is_foldable_diff_context_line(line) {
            output.push(line.to_string());
            continue;
        }
        let mut count = 1usize;
        while lines
            .peek()
            .is_some_and(|next| *next == line && is_foldable_diff_context_line(next))
        {
            let _ = lines.next();
            count += 1;
        }
        if count > 1 {
            output.push(format!("{line} [repeated x{count}]"));
        } else {
            output.push(line.to_string());
        }
    }
    output.join("\n")
}

fn is_foldable_diff_context_line(line: &str) -> bool {
    line.starts_with(' ') && !has_repeat_marker(line)
}

fn has_repeat_marker(line: &str) -> bool {
    line.rsplit_once(" [repeated x")
        .and_then(|(_, suffix)| suffix.strip_suffix(']'))
        .is_some_and(|count| count.parse::<usize>().is_ok())
}

fn parse_json_container(text: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    matches!(value, Value::Array(_) | Value::Object(_)).then_some(value)
}

fn looks_like_code(text: &str) -> bool {
    text.starts_with("```")
        || text.lines().take(24).any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("function ")
                || trimmed.starts_with("import ")
                || trimmed.starts_with("use ")
        })
}

fn looks_like_diff(text: &str) -> bool {
    text.starts_with("diff --git ")
        || (text.lines().any(|line| line.starts_with("@@ "))
            && text.lines().any(|line| {
                (line.starts_with('+') && !line.starts_with("+++"))
                    || (line.starts_with('-') && !line.starts_with("---"))
            }))
}

fn looks_like_valid_unified_diff(text: &str) -> bool {
    let mut has_old_header = false;
    let mut has_new_header = false;
    let mut has_hunk = false;
    let mut has_change = false;
    for line in text.lines().take(4096) {
        has_old_header |= line.starts_with("--- ");
        has_new_header |= line.starts_with("+++ ");
        has_hunk |= line.starts_with("@@ ");
        has_change |= (line.starts_with('+') && !line.starts_with("+++"))
            || (line.starts_with('-') && !line.starts_with("---"));
    }
    text.starts_with("diff --git ") && has_old_header && has_new_header && has_hunk && has_change
}

fn unified_diff_repeat_fold_is_safe(text: &str) -> bool {
    if text.ends_with('\n')
        || text.matches("diff --git ").count() != 1
        || text.lines().any(has_repeat_marker)
        || !looks_like_valid_unified_diff(text)
    {
        return false;
    }

    let lines = text.lines().collect::<Vec<_>>();
    let Some(old_header) = lines.iter().position(|line| line.starts_with("--- ")) else {
        return false;
    };
    let Some(new_header) = lines.iter().position(|line| line.starts_with("+++ ")) else {
        return false;
    };
    if new_header != old_header + 1 || old_header == 0 {
        return false;
    }

    let mut index = new_header + 1;
    let mut has_foldable_repeat = false;
    while index < lines.len() {
        let Some((old_count, new_count)) = parse_unified_hunk_counts(lines[index]) else {
            return false;
        };
        index += 1;
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;
        while index < lines.len() && !lines[index].starts_with("@@ ") {
            let line = lines[index];
            if line == "\\ No newline at end of file" {
                index += 1;
                continue;
            }
            match line.as_bytes().first() {
                Some(b' ') => {
                    old_seen += 1;
                    new_seen += 1;
                    if index + 1 < lines.len() && lines[index + 1] == line {
                        has_foldable_repeat = true;
                    }
                }
                Some(b'-') => old_seen += 1,
                Some(b'+') => new_seen += 1,
                _ => return false,
            }
            index += 1;
        }
        if old_seen != old_count || new_seen != new_count {
            return false;
        }
    }
    has_foldable_repeat
}

fn parse_unified_hunk_counts(line: &str) -> Option<(usize, usize)> {
    let body = line.strip_prefix("@@ ")?.strip_suffix(" @@")?;
    let (old, new) = body.split_once(' ')?;
    Some((
        parse_unified_range(old, '-')?,
        parse_unified_range(new, '+')?,
    ))
}

fn parse_unified_range(range: &str, prefix: char) -> Option<usize> {
    let range = range.strip_prefix(prefix)?;
    let (start, count) = range.split_once(',').unwrap_or((range, "1"));
    let start = start.parse::<usize>().ok()?;
    let count = count.parse::<usize>().ok()?;
    (start > 0 || count == 0).then_some(count)
}

fn expand_folded_diff_context_lines(candidate: &str) -> Option<String> {
    let mut expanded = Vec::new();
    for line in candidate.lines() {
        let Some((base, suffix)) = line.rsplit_once(" [repeated x") else {
            expanded.push(line.to_string());
            continue;
        };
        let count = suffix.strip_suffix(']')?.parse::<usize>().ok()?;
        if !(2..=1_000_000).contains(&count) || !base.starts_with(' ') {
            return None;
        }
        expanded.extend(std::iter::repeat_n(base.to_string(), count));
    }
    Some(expanded.join("\n"))
}

fn looks_like_html(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.starts_with("<!doctype html")
        || lower.starts_with("<html")
        || ["<script", "<style", "<pre", "<textarea"]
            .iter()
            .any(|tag| lower.starts_with(tag))
        || (lower.contains("<body") && lower.contains("</body>"))
}

fn html_repeat_fold_is_safe(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    has_adjacent_exact_repeat(text)
        && !["<script", "<style", "<pre", "<textarea"]
            .iter()
            .any(|tag| lower.contains(tag))
}

fn looks_like_build_output(text: &str) -> bool {
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(64)
        .collect::<Vec<_>>();
    if lines.len() < 2 {
        return false;
    }
    let marked = lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim_start();
            [
                "Compiling ",
                "Finished ",
                "Running ",
                "test result:",
                "npm ERR!",
                "PASS ",
                "FAIL ",
                "FAILED ",
            ]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
        })
        .count();
    marked * 2 >= lines.len()
}

fn looks_like_log(text: &str) -> bool {
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(64)
        .collect::<Vec<_>>();
    if lines.len() < 2 {
        return false;
    }
    let marked = lines
        .iter()
        .filter(|line| {
            let upper = line.trim_start().to_ascii_uppercase();
            ["INFO ", "WARN ", "WARNING ", "ERROR ", "DEBUG ", "TRACE "]
                .iter()
                .any(|prefix| upper.starts_with(prefix))
                || looks_like_iso_timestamp(line.trim_start())
        })
        .count();
    marked * 2 >= lines.len()
}

fn has_adjacent_exact_repeat(text: &str) -> bool {
    let lines = text.lines().collect::<Vec<_>>();
    lines.windows(2).any(|pair| pair[0] == pair[1])
}

fn looks_like_iso_timestamp(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() >= 20
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
}

fn looks_like_search_results(text: &str) -> bool {
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(64)
        .collect::<Vec<_>>();
    lines.len() >= 3
        && lines
            .iter()
            .filter(|line| is_search_anchor_line(line))
            .count()
            * 2
            >= lines.len()
}

fn is_search_anchor_line(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("http://")
        || line.starts_with("https://")
        || line.split_once(':').is_some_and(|(path, suffix)| {
            (path.contains('/') || path.contains('\\'))
                && suffix.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        })
}

fn search_repeat_fold_is_safe(text: &str) -> bool {
    if text.lines().any(has_repeat_marker) {
        return false;
    }
    let candidate = fold_search_result_repeats(text);
    candidate.len() < text.len()
        && expand_exact_folded_lines_losslessly(&candidate).as_deref() == Some(text)
        && text
            .lines()
            .filter(|line| is_search_anchor_line(line))
            .eq(candidate.lines().filter(|line| is_search_anchor_line(line)))
}

/// CCR retrieval markers that mean "these bytes are already compressed and
/// the real content lives in the reversible store".  Re-compressing such a
/// block treats compressed text as source and can make the retrieval
/// handle unrecoverable.  This is the d-token side of upstream #2694:
/// every marker shape the runtime can emit (`<<ccr:...>>` opaque-blob
/// forms plus the SmartCrusher row-drop/read-lifecycle bracket forms) is
/// preserved verbatim before any codec decision.
pub fn is_already_compressed(text: &str) -> bool {
    text.contains("<<ccr:")
        || text.contains("Retrieve more: hash=")
        || text.contains("Retrieve original: hash=")
}

/// YAML/TOML/INI structured-config detection (upstream content_detector
/// parity, heuristic-only — no third-party parsers).  A config is a small
/// set of section headers / key-value assignments with comment lines.
/// Prose and log lines are guarded out so config never steals text/log
/// routing.
fn looks_like_structured_config(text: &str) -> bool {
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(96)
        .collect::<Vec<_>>();
    if lines.len() < 3 {
        return false;
    }
    if looks_like_log(text) || looks_like_build_output(text) || looks_like_code(text) {
        return false;
    }
    let section = |line: &str| {
        let trimmed = line.trim();
        (trimmed.starts_with('[') && trimmed.ends_with(']'))
            || trimmed == "---"
            || trimmed == "..."
    };
    let assignment = |line: &str| {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.starts_with(';') {
            return false;
        }
        // TOML/INI: `key = value` / `key: value`.  Colon form is also
        // YAML's mapping separator, so it needs the value to be a plain
        // scalar and the line to be short — prose with a colon ("Note:")
        // must not qualify.
        if let Some((key, value)) = trimmed.split_once('=') {
            return !key.trim().is_empty() && !value.trim().is_empty() && value.trim().len() < 200;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            return !key.is_empty()
                && !value.is_empty()
                && value.len() < 200
                && !value.contains(' ')
                && !key.contains(' ');
        }
        // YAML list item: `- value`.
        trimmed.starts_with("- ")
    };
    let config_like = lines
        .iter()
        .filter(|line| section(line) || assignment(line))
        .count();
    config_like * 2 >= lines.len()
}

/// A typed section of mixed content (upstream `mixed_content.ContentSection`
/// parity).  Sections are line-based so reconstruction is exact: joining
/// section bodies with `\n` reproduces the original byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSection {
    pub kind: ContentKind,
    pub body: String,
    /// Code fence label when `kind == ContentKind::Code`, if any.
    pub fence_label: Option<String>,
}

/// Detect whether the text mixes multiple content types (upstream
/// `is_mixed_content` parity).  At least two distinct signals must fire.
pub fn is_mixed_content(text: &str) -> bool {
    let mut indicators = 0usize;
    if text.lines().any(|line| line.trim_start().starts_with("```")) {
        indicators += 1;
    }
    if text
        .lines()
        .any(|line| line.trim_start().starts_with('{') || line.trim_start().starts_with('['))
    {
        indicators += 1;
    }
    if text.lines().any(is_search_anchor_line) {
        indicators += 1;
    }
    if looks_like_structured_config(text) {
        indicators += 1;
    }
    // Prose signal: several multi-word sentence-ish lines.
    let prose = text
        .lines()
        .filter(|line| {
            let words = line.split_whitespace().count();
            words >= 4
        })
        .count();
    if prose > 3 {
        indicators += 1;
    }
    indicators >= 2
}

/// Split mixed content into typed sections.  Line-based and lossless:
/// every section body round-trips to the exact original bytes.
pub fn split_into_sections(text: &str) -> Vec<ContentSection> {
    let mut sections = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        // Code fence: consume until the closing fence.
        if line.trim_start().starts_with("```") {
            let label = line
                .trim_start()
                .trim_start_matches('`')
                .trim()
                .split_whitespace()
                .next()
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let mut body = Vec::new();
            while let Some(next) = lines.next() {
                if next.trim_start().starts_with("```") {
                    break;
                }
                body.push(next);
            }
            sections.push(ContentSection {
                kind: ContentKind::Code,
                body: body.join("\n"),
                fence_label: label,
            });
            continue;
        }
        // JSON block: collect a balanced {…}/[…].
        if line.trim_start().starts_with('{') || line.trim_start().starts_with('[') {
            let mut block = vec![line];
            let mut depth = json_balance_delta(line);
            while depth > 0 {
                let Some(next) = lines.next() else { break };
                depth += json_balance_delta(next);
                block.push(next);
            }
            if serde_json::from_str::<Value>(&block.join("\n")).is_ok() {
                sections.push(ContentSection {
                    kind: ContentKind::Json,
                    body: block.join("\n"),
                    fence_label: None,
                });
                continue;
            }
            // Not a valid JSON block — rewind the consumed lines.
            for rewinded in block.into_iter().rev().skip(1) {
                sections.push(ContentSection {
                    kind: ContentKind::Text,
                    body: rewinded.to_string(),
                    fence_label: None,
                });
            }
            continue;
        }
        // Search-anchor line run.
        if is_search_anchor_line(line) {
            let mut run = vec![line];
            while lines.peek().is_some_and(|next| is_search_anchor_line(next)) {
                run.push(lines.next().expect("peeked line"));
            }
            sections.push(ContentSection {
                kind: ContentKind::SearchResults,
                body: run.join("\n"),
                fence_label: None,
            });
            continue;
        }
        // Prose/text run until the next section boundary.
        let mut run = vec![line];
        while lines.peek().is_some_and(|next| {
            !next.trim_start().starts_with("```")
                && !next.trim_start().starts_with('{')
                && !next.trim_start().starts_with('[')
                && !is_search_anchor_line(next)
        }) {
            run.push(lines.next().expect("peeked line"));
        }
        sections.push(ContentSection {
            kind: ContentKind::Text,
            body: run.join("\n"),
            fence_label: None,
        });
    }
    sections
}

/// `+1` for an opening bracket, `-1` for a closing one, tracking string
/// literals so a `]` inside a JSON string doesn't unbalance the walk.
fn json_balance_delta(line: &str) -> isize {
    let mut delta = 0isize;
    let mut in_string = false;
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '[' | '{' if !in_string => delta += 1,
            ']' | '}' if !in_string => delta -= 1,
            _ => {}
        }
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_json_log_search_code_and_text_without_guessing_a_codec() {
        assert_eq!(
            route_content("{ \"items\": [1, 2] }").codec,
            CodecKind::JsonMinify
        );
        assert_eq!(
            route_content("INFO build\nINFO build").codec,
            CodecKind::LogFold
        );
        assert_eq!(
            route_content("src/a.rs:10 result\nsrc/b.rs:20 result\nsrc/c.rs:30 result")
                .content_kind,
            ContentKind::SearchResults
        );
        assert_eq!(
            route_content("fn main() { println!(\"x\"); }").content_kind,
            ContentKind::Code
        );
        assert_eq!(
            route_content("A short project explanation.").content_kind,
            ContentKind::Text
        );
    }

    #[test]
    fn json_codec_is_semantically_lossless_and_only_runs_when_smaller() {
        let source = "{\n  \"items\": [1, 2],\n  \"ok\": true\n}";
        let mut transforms = Vec::new();
        let compressed = apply_routed_codec(CompressionPolicy::Balanced, source, &mut transforms);
        assert_eq!(
            serde_json::from_str::<Value>(&compressed).unwrap(),
            serde_json::from_str::<Value>(source).unwrap()
        );
        assert!(compressed.len() < source.len());
        assert_eq!(transforms, vec!["content_router:json", "json_minify_v1"]);
    }

    #[test]
    fn oversized_and_nonbeneficial_code_content_fail_closed() {
        let oversized = "x".repeat(MAX_ROUTABLE_TEXT_BYTES + 1);
        assert_eq!(route_content(&oversized).reason_code, "size-limit");
        let code = "```rust\nfn main() {}\n```";
        let mut transforms = Vec::new();
        assert_eq!(
            apply_routed_codec(CompressionPolicy::Max, code, &mut transforms),
            code
        );
        assert!(transforms.is_empty());
    }

    #[test]
    fn code_fence_codec_handles_beneficial_sources_and_fails_closed_when_required() {
        for (source, expected) in [
            (
                "```rust\n\npub fn add(left: usize, right: usize) -> usize {\n    left + right\n}\n\n```",
                "```rust\npub fn add(left: usize, right: usize) -> usize {\n    left + right\n}\n```",
            ),
            (
                "```python\n\ndef add(left: int, right: int) -> int:\n    return left + right\n\n```",
                "```python\ndef add(left: int, right: int) -> int:\n    return left + right\n```",
            ),
            (
                "```typescript\n\ninterface User {\n  id: number;\n}\n\nconst value: User = { id: 1 };\n\n```",
                "```typescript\ninterface User {\n  id: number;\n}\n\nconst value: User = { id: 1 };\n```",
            ),
        ] {
            let decision = route_content(source);
            assert_eq!(decision.content_kind, ContentKind::Code);
            assert_eq!(decision.codec, CodecKind::CodeFenceEnvelopeTrim);
            assert_eq!(decision.reason_code, "code-fence-envelope-trim-v1");

            let mut transforms = Vec::new();
            assert_eq!(
                apply_routed_codec(CompressionPolicy::Balanced, source, &mut transforms),
                expected
            );
            assert_eq!(
                transforms,
                vec!["content_router:code", "code_fence_envelope_trim_v1"]
            );
        }

        for source in [
            "```rust\npub fn add() {}\n```",
            "```typescript\n\n//# sourceMappingURL=sample.js.map\nconst value: number = 1;\n\n```",
            "```python\n\nTraceback (most recent call last):\n  File \"demo.py\", line 1, in <module>\n\n```",
        ] {
            let decision = route_content(source);
            assert_eq!(decision.content_kind, ContentKind::Code);
            assert_eq!(decision.codec, CodecKind::Passthrough);

            let mut transforms = Vec::new();
            assert_eq!(
                apply_routed_codec(CompressionPolicy::Balanced, source, &mut transforms),
                source
            );
            assert!(transforms.is_empty());
        }

        let secret = "```rust\n\nconst TOKEN: &str = \"sk-not-for-codec\";\n\n```";
        let mut transforms = Vec::new();
        assert_eq!(route_content(secret).codec, CodecKind::Passthrough);
        assert_eq!(
            apply_routed_codec(CompressionPolicy::Balanced, secret, &mut transforms),
            secret
        );
        assert!(transforms.is_empty());
    }

    #[test]
    fn decision_signal_blocks_optical_image_routing_but_not_safe_log_folding() {
        let source = [
            "INFO build complete",
            "INFO build complete",
            "INFO final decision use floor division",
            "INFO build complete",
        ]
        .join("\n");
        assert!(is_safe_log_codec_candidate(&source));
        assert!(contains_decision_signal(&source));

        let mut transforms = Vec::new();
        let compressed = apply_routed_codec(CompressionPolicy::Max, &source, &mut transforms);
        assert!(compressed.contains("final decision use floor division"));
        assert!(!transforms.iter().any(|item| item == "long_text_summarize"));
    }

    #[test]
    fn classifies_diff_html_and_build_output_without_unsafe_codec_guessing() {
        let diff = "diff --git a/src/a.rs b/src/a.rs\n@@ -1 +1 @@\n-old\n+new";
        assert_eq!(route_content(diff).content_kind, ContentKind::Diff);
        assert_eq!(route_content(diff).codec, CodecKind::Passthrough);

        let html = "<!doctype html><html><body><p>result</p></body></html>";
        assert_eq!(route_content(html).content_kind, ContentKind::Html);
        assert_eq!(route_content(html).codec, CodecKind::Passthrough);

        let build = "Compiling alpha v0.1.0\nCompiling alpha v0.1.0\nFinished dev target";
        assert_eq!(route_content(build).codec, CodecKind::Passthrough);
        assert_eq!(
            route_content(build).reason_code,
            "exact-anchor-preserve-text"
        );
    }

    #[test]
    fn search_paths_are_preserved_as_native_text() {
        let source = [
            "src/a.rs:10 result",
            "src/a.rs:10 result",
            "src/b.rs:20 other",
            "src/c.rs:30 final",
        ]
        .join("\n");
        let decision = route_content(&source);
        assert_eq!(decision.codec, CodecKind::Passthrough);
        assert_eq!(decision.reason_code, "exact-anchor-preserve-text");

        let mut transforms = Vec::new();
        let compressed = apply_routed_codec(CompressionPolicy::Balanced, &source, &mut transforms);
        assert_eq!(compressed, source);
        assert!(compressed.contains("src/b.rs:20 other"));
        assert!(compressed.contains("src/c.rs:30 final"));
        assert!(transforms.is_empty());
    }

    #[test]
    fn mixed_or_low_confidence_content_stays_passthrough() {
        let mixed = "```rust\nfn main() {}\n```\nsrc/a.rs:10 result\nhttps://example.com";
        let decision = route_content(mixed);
        assert_eq!(decision.content_kind, ContentKind::Code);
        assert_eq!(decision.codec, CodecKind::Passthrough);
        assert!(decision.confidence >= 90);
    }

    #[test]
    fn strictly_counted_single_file_diff_folds_only_repeated_context_lines() {
        let source = [
            "diff --git a/src/a.rs b/src/a.rs",
            "--- a/src/a.rs",
            "+++ b/src/a.rs",
            "@@ -1,4 +1,4 @@",
            " unchanged",
            " unchanged",
            " unchanged",
            "-old exact",
            "+new exact",
        ]
        .join("\n");
        let decision = route_content(&source);
        assert_eq!(decision.content_kind, ContentKind::Diff);
        assert_eq!(decision.codec, CodecKind::DiffFold);
        let mut transforms = Vec::new();
        let candidate = apply_routed_codec(CompressionPolicy::Balanced, &source, &mut transforms);
        assert_ne!(candidate, source);
        assert!(candidate.contains(" unchanged [repeated x3]"));
        assert_eq!(
            expand_folded_diff_context_lines(&candidate).as_deref(),
            Some(source.as_str())
        );
        assert_eq!(
            transforms,
            ["content_router:diff", "diff_context_repeat_fold_v1"]
        );

        let malformed = "diff --git a/a b/a\n-old\n+new";
        assert_eq!(route_content(malformed).content_kind, ContentKind::Diff);
        assert_eq!(route_content(malformed).codec, CodecKind::Passthrough);
    }

    #[test]
    fn diff_codec_rejects_multi_file_bad_counts_markers_secrets_and_negative_benefit() {
        let valid_without_repeats = [
            "diff --git a/a.rs b/a.rs",
            "--- a/a.rs",
            "+++ b/a.rs",
            "@@ -1 +1 @@",
            "-old",
            "+new",
        ]
        .join("\n");
        assert_eq!(
            route_content(&valid_without_repeats).codec,
            CodecKind::Passthrough
        );

        let malformed_counts = valid_without_repeats.replace("@@ -1 +1 @@", "@@ -1,2 +1 @@");
        assert_eq!(
            route_content(&malformed_counts).codec,
            CodecKind::Passthrough
        );

        let multi_file = format!("{valid_without_repeats}\n{valid_without_repeats}");
        assert_eq!(route_content(&multi_file).codec, CodecKind::Passthrough);

        let marker = [
            "diff --git a/a.rs b/a.rs",
            "--- a/a.rs",
            "+++ b/a.rs",
            "@@ -1,3 +1,3 @@",
            " keep [repeated x2]",
            " keep [repeated x2]",
            " unchanged",
        ]
        .join("\n");
        assert_eq!(route_content(&marker).codec, CodecKind::Passthrough);

        let secret = marker.replace("unchanged", "Authorization: Bearer sk-live-secret");
        assert_eq!(route_content(&secret).codec, CodecKind::Passthrough);
    }

    #[test]
    fn secret_like_content_is_preserved_before_codec_selection() {
        let source = "INFO Authorization: Bearer sk-live-secret-material\nINFO done";
        let decision = route_content(source);
        assert_eq!(decision.codec, CodecKind::Passthrough);
        assert_eq!(decision.reason_code, "secret-preserve-text");
    }

    #[test]
    fn routing_budget_fails_closed_on_size_or_deadline_pressure() {
        let json = "{ \"items\": [1, 2, 3] }";
        let size_limited = route_content_with_budget(
            json,
            RoutingBudget {
                max_input_bytes: json.len() - 1,
                deadline_remaining_us: None,
            },
        );
        assert_eq!(size_limited.codec, CodecKind::Passthrough);
        assert_eq!(size_limited.reason_code, "size-limit");

        let deadline_limited = route_content_with_budget(
            json,
            RoutingBudget {
                max_input_bytes: MAX_ROUTABLE_TEXT_BYTES,
                deadline_remaining_us: Some(100),
            },
        );
        assert_eq!(deadline_limited.codec, CodecKind::Passthrough);
        assert_eq!(deadline_limited.reason_code, "deadline-budget");
    }

    #[test]
    fn repeated_plain_text_uses_exact_fold_but_decisions_stay_passthrough() {
        let text = [
            "The build completed successfully.",
            "The build completed successfully.",
            "No additional action is required.",
        ]
        .join("\n");
        let decision = route_content(&text);
        assert_eq!(decision.content_kind, ContentKind::Text);
        assert_eq!(decision.codec, CodecKind::TextRepeatFold);
        let mut transforms = Vec::new();
        let candidate = apply_routed_codec(CompressionPolicy::Balanced, &text, &mut transforms);
        assert!(candidate.contains("The build completed successfully. [repeated x2]"));
        assert!(candidate.contains("No additional action is required."));

        let decision_text = "final decision use model alpha\nfinal decision use model alpha";
        assert_eq!(route_content(decision_text).codec, CodecKind::Passthrough);
        assert_eq!(
            route_content(decision_text).reason_code,
            "decision-sensitive-text"
        );
    }

    #[test]
    fn html_repeat_codec_excludes_script_style_pre_and_textarea() {
        let html = [
            "<!doctype html>",
            "<ul>",
            "<li>same</li>",
            "<li>same</li>",
            "<li>same</li>",
            "<li>same</li>",
            "</ul>",
        ]
        .join("\n");
        let decision = route_content(&html);
        assert_eq!(decision.content_kind, ContentKind::Html);
        assert_eq!(decision.codec, CodecKind::HtmlRepeatFold);
        let mut transforms = Vec::new();
        let candidate = apply_routed_codec(CompressionPolicy::Balanced, &html, &mut transforms);
        assert!(candidate.contains("<li>same</li> [repeated x4]"));

        for blocked in [
            "<script>\nrun();\nrun();\n</script>",
            "<style>\n.a{}\n.a{}\n</style>",
            "<pre>\nexact\nexact\n</pre>",
            "<textarea>\nexact\nexact\n</textarea>",
        ] {
            assert_eq!(route_content(blocked).content_kind, ContentKind::Html);
            assert_eq!(route_content(blocked).codec, CodecKind::Passthrough);
        }
    }
}
