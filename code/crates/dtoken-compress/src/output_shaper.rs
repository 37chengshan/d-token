//! Route-resolved output shaping (收口计划 B3 item 4d).
//!
//! The production caller must supply an exact route/provider/model policy.
//! Missing authorization, invalid holdouts, risk signals, and non-positive
//! deltas are all explicit passthroughs. Evidence contains aggregate byte
//! counts only; never response text.

use dtoken_types::{CompressionPolicy, OutputShapingRoutePolicy, OutputVerbosity, RouteProfile};
use sha2::{Digest, Sha256};

/// Effort budget a resolved route is willing to spend on shaping. Higher
/// compression policy lowers the effort budget (more aggressive shaping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffortLevel {
    Minimal,
    Standard,
    High,
}

impl EffortLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Standard => "standard",
            Self::High => "high",
        }
    }
}

/// Map a resolved route's compression policy to a conservative effort level.
/// `Off`/`Safe` keep full effort (Detailed); `Balanced` is Standard;
/// `Aggressive`/`Max` drop to Minimal.
pub fn route_effort_routing(route: &RouteProfile) -> EffortLevel {
    match route.compression_policy {
        CompressionPolicy::Max | CompressionPolicy::Aggressive => EffortLevel::Minimal,
        CompressionPolicy::Balanced => EffortLevel::Standard,
        CompressionPolicy::Safe | CompressionPolicy::Off => EffortLevel::High,
    }
}

/// Resolve the effective output verbosity. `Auto` defers to the route effort:
/// Minimal -> Terse, Standard -> Normal, High -> Detailed. An explicit
/// verbosity is always honored over the route-derived default.
pub fn resolve_output_verbosity(
    declared: OutputVerbosity,
    route: &RouteProfile,
) -> OutputVerbosity {
    match declared {
        OutputVerbosity::Auto => match route_effort_routing(route) {
            EffortLevel::Minimal => OutputVerbosity::Terse,
            EffortLevel::Standard => OutputVerbosity::Normal,
            EffortLevel::High => OutputVerbosity::Detailed,
        },
        other => other,
    }
}

/// The outcome of shaping a text body for a resolved route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapedOutput {
    pub body: String,
    pub original_bytes: usize,
    pub shaped_bytes: usize,
    pub verbosity: OutputVerbosity,
    pub effort: EffortLevel,
    pub transforms: Vec<&'static str>,
    pub applied: bool,
}

/// The only response classes accepted by the production adapter. A response
/// with a tool call, code block, opaque content, streaming body, or error is
/// not safe to shape and must be passed through by its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputSafety {
    PlainText,
    Stream,
    Error,
    ToolOrStructuredContent,
    CodeOrOpaqueContent,
    Unknown,
}

impl OutputSafety {
    pub const fn reason_code(self) -> &'static str {
        match self {
            Self::PlainText => "plain_text",
            Self::Stream => "stream_passthrough",
            Self::Error => "error_response_passthrough",
            Self::ToolOrStructuredContent => "tool_or_structured_response_passthrough",
            Self::CodeOrOpaqueContent => "code_or_opaque_response_passthrough",
            Self::Unknown => "unknown_response_shape_passthrough",
        }
    }
}

/// Accounting is deliberately dimensioned in outbound UTF-8 bytes. Provider
/// output-token counters describe generation, not this local response rewrite,
/// so they must not be claimed as measured shaping savings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputShapingAccounting {
    Measured {
        original_bytes: usize,
        shaped_bytes: usize,
        saved_bytes: usize,
    },
    HoldoutEstimated {
        original_bytes: usize,
        candidate_bytes: usize,
        estimated_saved_bytes: usize,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputShapingDisposition {
    Applied,
    Holdout,
    Passthrough,
}

impl OutputShapingDisposition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Holdout => "holdout",
            Self::Passthrough => "passthrough",
        }
    }
}

/// A decision contains no raw response body in its evidence-facing fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputShapingResult {
    pub body: String,
    pub disposition: OutputShapingDisposition,
    pub reason_code: &'static str,
    pub verbosity: OutputVerbosity,
    pub effort: EffortLevel,
    pub transforms: Vec<&'static str>,
    pub accounting: OutputShapingAccounting,
}

/// Select holdout deterministically from the request id and the exact bound
/// route. A retry of the same routed request cannot silently move cohorts.
pub fn is_deterministic_holdout(request_id: &str, route_id: &str, ratio: f32) -> bool {
    if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) || ratio <= 0.0 {
        return false;
    }
    if ratio >= 1.0 {
        return true;
    }
    let mut hasher = Sha256::new();
    hasher.update(b"dtoken-output-shaper-holdout-v1\0");
    hasher.update(route_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(request_id.as_bytes());
    let digest = hasher.finalize();
    let bucket = u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 prefix"));
    (bucket as f64 / u64::MAX as f64) < ratio as f64
}

/// Shape one response text value under an already resolved policy. The route,
/// provider, and model binding is validated by the configuration layer before
/// this function is called.
pub fn shape_output_for_policy(
    route: &RouteProfile,
    policy: &OutputShapingRoutePolicy,
    request_id: &str,
    safety: OutputSafety,
    body: &str,
) -> OutputShapingResult {
    let verbosity = resolve_output_verbosity(policy.output_verbosity.clone(), route);
    let effort = route_effort_routing(route);
    let passthrough = |reason_code, accounting| OutputShapingResult {
        body: body.to_string(),
        disposition: OutputShapingDisposition::Passthrough,
        reason_code,
        verbosity: verbosity.clone(),
        effort,
        transforms: Vec::new(),
        accounting,
    };
    if !policy.enabled {
        return passthrough("output_shaping_disabled", OutputShapingAccounting::Unknown);
    }
    if safety != OutputSafety::PlainText {
        return passthrough(safety.reason_code(), OutputShapingAccounting::Unknown);
    }
    let holdout_ratio = policy.holdout_ratio.unwrap_or(0.0);
    if !holdout_ratio.is_finite() || !(0.0..=1.0).contains(&holdout_ratio) {
        return passthrough("invalid_holdout_ratio", OutputShapingAccounting::Unknown);
    }
    let shaped = shape_text_output(body, verbosity.clone(), effort);
    let saved_bytes = shaped.original_bytes.saturating_sub(shaped.shaped_bytes);
    if saved_bytes == 0 {
        return passthrough("non_positive_byte_delta", OutputShapingAccounting::Unknown);
    }
    if is_deterministic_holdout(request_id, &route.id, holdout_ratio) {
        return OutputShapingResult {
            body: body.to_string(),
            disposition: OutputShapingDisposition::Holdout,
            reason_code: "deterministic_holdout",
            verbosity,
            effort,
            transforms: shaped.transforms,
            accounting: OutputShapingAccounting::HoldoutEstimated {
                original_bytes: shaped.original_bytes,
                candidate_bytes: shaped.shaped_bytes,
                estimated_saved_bytes: saved_bytes,
            },
        };
    }
    OutputShapingResult {
        body: shaped.body,
        disposition: OutputShapingDisposition::Applied,
        reason_code: "output_shaping_applied",
        verbosity,
        effort,
        transforms: shaped.transforms,
        accounting: OutputShapingAccounting::Measured {
            original_bytes: shaped.original_bytes,
            shaped_bytes: shaped.shaped_bytes,
            saved_bytes,
        },
    }
}

/// Apply route-resolved output shaping to a text body. The transform is
/// whitespace-only: `Terse` collapses runs of two or more blank lines to one
/// and trims trailing blank lines; `Normal` only trims trailing blank lines;
/// `Detailed` preserves the body verbatim. Non-blank content is never removed,
/// so the transform is content-preserving (lossless for semantic payload).
pub fn shape_text_output(
    body: &str,
    verbosity: OutputVerbosity,
    effort: EffortLevel,
) -> ShapedOutput {
    let original_bytes = body.len();
    let mut transforms = Vec::new();
    let shaped = match verbosity {
        OutputVerbosity::Terse => {
            let folded = collapse_blank_runs(body);
            if folded != body {
                transforms.push("terse_blank_collapse");
            }
            let trimmed = trim_trailing_blank_lines(&folded);
            if trimmed != folded {
                transforms.push("terse_trailing_blank_trim");
            }
            trimmed
        }
        OutputVerbosity::Normal => {
            let trimmed = trim_trailing_blank_lines(body);
            if trimmed != body {
                transforms.push("normal_trailing_blank_trim");
            }
            trimmed
        }
        OutputVerbosity::Auto | OutputVerbosity::Detailed => body.to_string(),
    };
    let shaped_bytes = shaped.len();
    let applied = shaped_bytes != original_bytes;
    ShapedOutput {
        body: shaped,
        original_bytes,
        shaped_bytes,
        verbosity,
        effort,
        transforms,
        applied,
    }
}

/// Convenience: shape a body for a resolved route and a declared verbosity.
/// Returns the shaped body plus metadata. Callers must gate this on the
/// route/project `enable_output_shaper` flag before applying upstream.
pub fn shape_output_for_route(
    route: &RouteProfile,
    declared_verbosity: OutputVerbosity,
    body: &str,
) -> ShapedOutput {
    let verbosity = resolve_output_verbosity(declared_verbosity, route);
    let effort = route_effort_routing(route);
    shape_text_output(body, verbosity, effort)
}

fn collapse_blank_runs(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut blank_run = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run == 1 {
                out.push(line);
            }
            continue;
        }
        blank_run = 0;
        out.push(line);
    }
    out.join("\n")
}

fn trim_trailing_blank_lines(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    while matches!(lines.last(), Some(last) if last.trim().is_empty()) {
        lines.pop();
    }
    let joined = lines.join("\n");
    if text.ends_with('\n') && !joined.is_empty() {
        format!("{joined}\n")
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_types::{ChainMode, OutputShapingRoutePolicy, Protocol};

    fn route(policy: CompressionPolicy) -> RouteProfile {
        RouteProfile {
            id: "r1".into(),
            name: "route".into(),
            input_protocol: Protocol::OpenAi,
            instance_id: "i1".into(),
            target_provider_id: "p1".into(),
            fallback_provider_id: None,
            chain_mode: ChainMode::Direct,
            compression_policy: policy,
            bind_host: "127.0.0.1".into(),
            local_port: 8787,
        }
    }

    fn shaping_policy(
        enabled: bool,
        verbosity: OutputVerbosity,
        holdout_ratio: Option<f32>,
    ) -> OutputShapingRoutePolicy {
        OutputShapingRoutePolicy {
            route_id: "r1".into(),
            provider_id: "p1".into(),
            model: "model-1".into(),
            enabled,
            output_verbosity: verbosity,
            holdout_ratio,
        }
    }

    #[test]
    fn effort_routing_maps_compression_policy() {
        assert_eq!(
            route_effort_routing(&route(CompressionPolicy::Off)),
            EffortLevel::High
        );
        assert_eq!(
            route_effort_routing(&route(CompressionPolicy::Safe)),
            EffortLevel::High
        );
        assert_eq!(
            route_effort_routing(&route(CompressionPolicy::Balanced)),
            EffortLevel::Standard
        );
        assert_eq!(
            route_effort_routing(&route(CompressionPolicy::Aggressive)),
            EffortLevel::Minimal
        );
        assert_eq!(
            route_effort_routing(&route(CompressionPolicy::Max)),
            EffortLevel::Minimal
        );
    }

    #[test]
    fn auto_verbosity_defers_to_effort() {
        assert_eq!(
            resolve_output_verbosity(OutputVerbosity::Auto, &route(CompressionPolicy::Max)),
            OutputVerbosity::Terse
        );
        assert_eq!(
            resolve_output_verbosity(OutputVerbosity::Auto, &route(CompressionPolicy::Balanced)),
            OutputVerbosity::Normal
        );
        assert_eq!(
            resolve_output_verbosity(OutputVerbosity::Auto, &route(CompressionPolicy::Off)),
            OutputVerbosity::Detailed
        );
    }

    #[test]
    fn explicit_verbosity_overrides_route() {
        assert_eq!(
            resolve_output_verbosity(OutputVerbosity::Terse, &route(CompressionPolicy::Off)),
            OutputVerbosity::Terse
        );
    }

    #[test]
    fn terse_collapses_blank_runs_without_losing_content() {
        let body = "fn a() {}\n\n\n\nfn b() {}\n\n";
        let shaped = shape_text_output(body, OutputVerbosity::Terse, EffortLevel::Minimal);
        assert!(shaped.applied);
        assert!(shaped.body.contains("fn a() {}"));
        assert!(shaped.body.contains("fn b() {}"));
        assert!(!shaped.body.contains("\n\n\n"));
    }

    #[test]
    fn detailed_preserves_body_verbatim() {
        let body = "keep\n\n\nme\n";
        let shaped = shape_text_output(body, OutputVerbosity::Detailed, EffortLevel::High);
        assert!(!shaped.applied);
        assert_eq!(shaped.body, body);
        assert!(shaped.transforms.is_empty());
    }

    #[test]
    fn shape_for_route_gates_on_verbosity() {
        let r = route(CompressionPolicy::Aggressive);
        let body = "a\n\n\n\nb";
        let shaped = shape_output_for_route(&r, OutputVerbosity::Auto, body);
        assert_eq!(shaped.verbosity, OutputVerbosity::Terse);
        assert!(shaped.applied);
        assert_eq!(shaped.body, "a\n\nb");
    }

    #[test]
    fn policy_requires_explicit_enablement_and_safe_plain_text() {
        let body = "a\n\n\n\nb\n\n";
        let disabled = shape_output_for_policy(
            &route(CompressionPolicy::Max),
            &shaping_policy(false, OutputVerbosity::Terse, None),
            "request-1",
            OutputSafety::PlainText,
            body,
        );
        assert_eq!(disabled.disposition, OutputShapingDisposition::Passthrough);
        assert_eq!(disabled.reason_code, "output_shaping_disabled");
        assert_eq!(disabled.body, body);

        let risky = shape_output_for_policy(
            &route(CompressionPolicy::Max),
            &shaping_policy(true, OutputVerbosity::Terse, None),
            "request-1",
            OutputSafety::ToolOrStructuredContent,
            body,
        );
        assert_eq!(risky.disposition, OutputShapingDisposition::Passthrough);
        assert_eq!(risky.reason_code, "tool_or_structured_response_passthrough");
        assert_eq!(risky.body, body);
        assert_eq!(risky.accounting, OutputShapingAccounting::Unknown);
    }

    #[test]
    fn holdout_is_deterministic_and_keeps_estimate_separate_from_measurement() {
        let body = "a\n\n\n\nb\n\n";
        let policy = shaping_policy(true, OutputVerbosity::Terse, Some(1.0));
        let first = shape_output_for_policy(
            &route(CompressionPolicy::Max),
            &policy,
            "request-1",
            OutputSafety::PlainText,
            body,
        );
        let second = shape_output_for_policy(
            &route(CompressionPolicy::Max),
            &policy,
            "request-1",
            OutputSafety::PlainText,
            body,
        );
        assert_eq!(first, second);
        assert_eq!(first.disposition, OutputShapingDisposition::Holdout);
        assert_eq!(first.body, body);
        assert!(matches!(
            first.accounting,
            OutputShapingAccounting::HoldoutEstimated { .. }
        ));
        assert!(!is_deterministic_holdout("request-1", "r1", 0.0));
        assert!(is_deterministic_holdout("request-1", "r1", 1.0));
    }

    #[test]
    fn applied_result_has_measured_bytes_only_after_a_positive_delta() {
        let body = "a\n\n\n\nb\n\n";
        let result = shape_output_for_policy(
            &route(CompressionPolicy::Max),
            &shaping_policy(true, OutputVerbosity::Terse, Some(0.0)),
            "request-1",
            OutputSafety::PlainText,
            body,
        );
        assert_eq!(result.disposition, OutputShapingDisposition::Applied);
        assert_eq!(result.body, "a\n\nb\n");
        assert!(matches!(
            result.accounting,
            OutputShapingAccounting::Measured { saved_bytes, .. } if saved_bytes > 0
        ));

        let no_gain = shape_output_for_policy(
            &route(CompressionPolicy::Max),
            &shaping_policy(true, OutputVerbosity::Terse, None),
            "request-2",
            OutputSafety::PlainText,
            "already compact",
        );
        assert_eq!(no_gain.disposition, OutputShapingDisposition::Passthrough);
        assert_eq!(no_gain.reason_code, "non_positive_byte_delta");
        assert_eq!(no_gain.accounting, OutputShapingAccounting::Unknown);
    }
}
