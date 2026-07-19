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
    Text,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind {
    JsonMinify,
    LogFold,
    SearchFold,
    BuildFold,
    DiffFold,
    TextRepeatFold,
    HtmlRepeatFold,
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
        return decision(
            ContentKind::Diff,
            CodecKind::Passthrough,
            if looks_like_valid_unified_diff(trimmed) {
                "unified-diff-preserve-text"
            } else {
                "malformed-diff-passthrough"
            },
            98,
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
    if looks_like_code(trimmed) {
        return decision(
            ContentKind::Code,
            CodecKind::Passthrough,
            "code-requires-ast-codec",
            95,
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
    if looks_like_search_results(trimmed) {
        return decision(
            ContentKind::SearchResults,
            if exact_preserve {
                CodecKind::Passthrough
            } else {
                CodecKind::SearchFold
            },
            if exact_preserve {
                "exact-anchor-preserve-text"
            } else {
                "path-line-search-results"
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
    let _ = text;
    false
}

pub fn is_safe_text_repeat_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::TextRepeatFold)
}

pub fn is_safe_html_repeat_candidate(text: &str) -> bool {
    matches!(route_content(text).codec, CodecKind::HtmlRepeatFold)
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
    let decision = route_content(text);
    match decision.codec {
        CodecKind::JsonMinify => {
            let Some(minified) = minify_json_losslessly(text) else {
                return text.to_string();
            };
            crate::push_transform(transforms, "content_router:json");
            crate::push_transform(transforms, "json_minify_v1");
            minified
        }
        CodecKind::LogFold => {
            // Log routing is deliberately bounded to stable normalization and
            // adjacent-repeat folding. Aggressive/Max's generic middle-text
            // collapse can discard a unique decision line from a long log.
            let _requested_policy = policy;
            let compressed = crate::compress_text(CompressionPolicy::Balanced, text, transforms);
            if compressed != text {
                crate::push_transform(transforms, "content_router:log");
                crate::push_transform(transforms, "log_fold_v1");
            }
            compressed
        }
        CodecKind::SearchFold => {
            let compressed = fold_exact_repeated_lines(text);
            if compressed.len() >= text.len() {
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
        CodecKind::DiffFold => text.to_string(),
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
        CodecKind::Passthrough => text.to_string(),
    }
}

pub fn minify_json_losslessly(text: &str) -> Option<String> {
    let value = parse_json_container(text.trim())?;
    let minified = serde_json::to_string(&value).ok()?;
    (minified.len() < text.len()).then_some(minified)
}

fn fold_exact_repeated_lines(text: &str) -> String {
    let mut output = Vec::new();
    let mut lines = text.lines();
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
    output.join("\n")
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
            .filter(|line| {
                let line = line.trim();
                line.starts_with("http://")
                    || line.starts_with("https://")
                    || line.split_once(':').is_some_and(|(path, suffix)| {
                        (path.contains('/') || path.contains('\\'))
                            && suffix.chars().next().is_some_and(|ch| ch.is_ascii_digit())
                    })
            })
            .count()
            * 2
            >= lines.len()
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
    fn oversized_and_code_content_fail_closed() {
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
    fn unified_diff_and_malformed_diff_both_preserve_exact_text() {
        let source = [
            "diff --git a/src/a.rs b/src/a.rs",
            "--- a/src/a.rs",
            "+++ b/src/a.rs",
            "@@ -1,7 +1,7 @@",
            " unchanged",
            " unchanged",
            " unchanged",
            "-old exact",
            "+new exact",
        ]
        .join("\n");
        let decision = route_content(&source);
        assert_eq!(decision.content_kind, ContentKind::Diff);
        assert_eq!(decision.codec, CodecKind::Passthrough);
        let mut transforms = Vec::new();
        let candidate = apply_routed_codec(CompressionPolicy::Balanced, &source, &mut transforms);
        assert_eq!(candidate, source);
        assert!(transforms.is_empty());

        let malformed = "diff --git a/a b/a\n-old\n+new";
        assert_eq!(route_content(malformed).content_kind, ContentKind::Diff);
        assert_eq!(route_content(malformed).codec, CodecKind::Passthrough);
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
