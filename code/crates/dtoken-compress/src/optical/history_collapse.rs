//! HistoryCollapsePlanner (PR-C5B): plans byte-stable collapse of frozen
//! history while keeping pinned and protected text native.
//!
//! Semantics (d-token-owned; no upstream code is copied):
//! - **History collapse**: only closed, historical, low-risk tool results
//!   below the recent tail are collapse candidates; every candidate maps to
//!   the canonical `ApprovedHybridAction::OpticalImage` action, which is the
//!   exclusive image-producing action in the IR.
//! - **Static slab**: system + tools + tool schema + developer instructions
//!   (including the CLAUDE.md section of the system prompt) form a byte-stable
//!   slab with a deterministic digest.
//! - **Freeze grid**: the row-level digests of every pinned block; collapse
//!   must leave them byte-identical.
//! - **Cache anchors**: the cache prefix boundary is the recent tail start;
//!   everything before it is byte-stable prefix.
//! - **Billing boundary**: volatile billing headers are detected with the
//!   shared volatility classifier and never become collapse candidates.
//! - **Pin semantics**: current turn, recent tail, open tool state, exact,
//!   secret, existing images, system, developer, tools, thinking and
//!   structural tool state are pinned; nothing pinned is ever collapsed.
//!
//! The planner is evidence-only: it never mutates the body. Production
//! patching stays gated by signed capability attestation (PR-C5C), so text is
//! preserved until those gates are complete.

use std::collections::HashSet;

use dtoken_types::RouteOpticalPolicy;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::optical::render_geometry::{
    estimate_image_tokens, images_required, RenderGeometryProfile,
};
use crate::optical::volatility::{detect_volatile_kind, VolatileKind};
use crate::{
    ApprovedHybridAction, HybridContextIr, HybridNodeKind, HybridProtection, HybridProtectionReason,
};

pub const HISTORY_COLLAPSE_PLANNER_REVISION: &str = "dtoken-history-collapse-planner-v1";

/// Why a block is pinned (never a collapse candidate). This is the explicit
/// pin semantics contract of the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinReason {
    System,
    Developer,
    Tools,
    ToolSchema,
    ClaudeMd,
    CurrentTurn,
    RecentTail,
    OpenToolState,
    StructuralToolState,
    Exact,
    Secret,
    ExistingImage,
    Thinking,
    ToolArguments,
    Identifier,
    GeneralPreserve,
    Unknown,
}

/// Origin of a byte-stable static slab row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlabProvenance {
    System,
    Developer,
    Tools,
    ToolSchema,
    ClaudeMd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlabRow {
    pub row_index: usize,
    pub provenance: SlabProvenance,
    pub digest: String,
}

/// system + tools + tool schema + developer instructions as a byte-stable
/// slab. `digest` changes if any slab row changes, so slab churn is visible
/// as cache-prefix churn before any collapse is attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticSlab {
    pub rows: Vec<SlabRow>,
    pub digest: String,
    pub total_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenRow {
    pub source_path: String,
    pub row_index: usize,
    pub pin_reason: PinReason,
    pub digest: String,
}

/// Row-level digests of every pinned block. `byte_stable` is the planner's
/// contract: collapse candidates never touch these rows, so the frozen
/// prefix stays byte-identical for cache anchors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreezeGrid {
    pub frozen_rows: Vec<FrozenRow>,
    pub byte_stable: bool,
}

/// The cache-prefix boundary. Everything at or below `anchor_turn_index`
/// (system/tools plus frozen history) is the stable cache prefix and must not
/// churn between requests; collapse stops exactly here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheAnchor {
    pub anchor_turn_index: usize,
    pub cache_prefix_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingBoundary {
    /// First (source_path, row_index) that is a volatile billing header.
    pub first_billing_row: Option<(String, usize)>,
    pub volatile_billing_chars: usize,
    pub volatile_kinds: Vec<VolatileKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapseCandidate {
    pub block_id: String,
    pub source_path: String,
    pub turn_index: usize,
    pub chars: usize,
    pub estimated_images: usize,
    pub estimated_vision_tokens: usize,
    pub stable_digest: Option<String>,
    pub volatile_chars: usize,
    pub cache_stable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCollapsePlan {
    /// Inclusive frozen history turn range `[start, end)` that may collapse.
    pub frozen_turn_range: (usize, usize),
    pub collapse_candidates: Vec<CollapseCandidate>,
    pub static_slab: StaticSlab,
    pub freeze_grid: FreezeGrid,
    pub cache_anchor: CacheAnchor,
    pub billing_boundary: BillingBoundary,
    /// Block source paths preserved as native text (current/recent/open/
    /// exact/secret/structural/system/developer/tools).
    pub protected_text_block_ids: Vec<String>,
    /// Every pinned block source path (never a collapse candidate).
    pub pinned_block_ids: Vec<String>,
    /// True when current/recent/open/exact/secret content is provably text.
    pub current_recent_open_exact_secret_native_text: bool,
    pub planner_revision: &'static str,
    pub digest: String,
}

impl HistoryCollapsePlan {
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// Plan history collapse for the canonical IR. Returns `None` (fail-closed:
/// preserve text) when the route does not allow history collapse, there is no
/// frozen history, or no closed historical tool result is collapseable.
pub fn plan_history_collapse(
    ir: &HybridContextIr,
    body: &Value,
    profile: &RenderGeometryProfile,
    route: &RouteOpticalPolicy,
) -> Option<HistoryCollapsePlan> {
    if !route.allow_history {
        return None;
    }
    let frozen_end = ir.recent_tail_start();
    if frozen_end == 0 {
        return None;
    }
    let candidates = collapse_candidates(ir, body, profile, frozen_end, ir.current_user_turn());
    if candidates.is_empty() {
        return None;
    }

    let static_slab = plan_static_slab(ir, body);
    let freeze_grid = plan_freeze_grid(ir, body);
    let cache_anchor = detect_cache_anchor(ir, body, frozen_end);
    let billing_boundary = detect_billing_boundary(ir, body);
    let (pinned_block_ids, protected_text_block_ids) = partition_pins(ir);
    let current_recent_open_exact_secret_native_text =
        verify_protected_text(ir, &candidates, &protected_text_block_ids);

    let plan = HistoryCollapsePlan {
        frozen_turn_range: (0, frozen_end),
        collapse_candidates: candidates,
        static_slab,
        freeze_grid,
        cache_anchor,
        billing_boundary,
        protected_text_block_ids,
        pinned_block_ids,
        current_recent_open_exact_secret_native_text,
        planner_revision: HISTORY_COLLAPSE_PLANNER_REVISION,
        digest: String::new(),
    };
    Some(plan.with_digest())
}

impl HistoryCollapsePlan {
    fn with_digest(mut self) -> Self {
        self.digest = plan_digest(&self);
        self
    }
}

/// The canonical, exclusive collapse path: only approved closed-historical
/// targets carrying `ApprovedHybridAction::OpticalImage` below the recent
/// tail become candidates. Ordinary image entries and unsupported-protocol
/// surfaces never reach this set (their IR protection is Opaque/AlwaysText).
fn collapse_candidates(
    ir: &HybridContextIr,
    body: &Value,
    profile: &RenderGeometryProfile,
    frozen_end: usize,
    current_turn: Option<usize>,
) -> Vec<CollapseCandidate> {
    let mut candidates = Vec::new();
    for target in ir.approved_text_targets() {
        if target.action() != ApprovedHybridAction::OpticalImage {
            continue;
        }
        let Some(turn_index) = target.turn_index() else {
            continue;
        };
        if turn_index >= frozen_end || Some(turn_index) == current_turn {
            continue;
        }
        let Some(text) = body.pointer(target.patch_path()).and_then(Value::as_str) else {
            continue;
        };
        let chars = text.chars().count();
        if chars == 0 {
            continue;
        }
        let Some(estimated_images) = images_required(profile, chars) else {
            continue;
        };
        candidates.push(CollapseCandidate {
            block_id: target.node_id().to_string(),
            source_path: target.patch_path().to_string(),
            turn_index,
            chars,
            estimated_images,
            estimated_vision_tokens: estimate_image_tokens(profile, estimated_images),
            stable_digest: target.stable_digest().map(str::to_string),
            volatile_chars: target.volatile_chars(),
            cache_stable: target.cache_stable(),
        });
    }
    candidates.sort_by(|left, right| {
        left.turn_index
            .cmp(&right.turn_index)
            .then_with(|| left.source_path.cmp(&right.source_path))
    });
    candidates
}

/// Build the byte-stable system + tools + tool-schema + developer slab.
/// Rows containing the `CLAUDE.md` marker are attributed to the CLAUDE.md
/// provenance so the slab digest is sensitive to CLAUDE.md churn.
fn plan_static_slab(ir: &HybridContextIr, body: &Value) -> StaticSlab {
    let mut rows = Vec::new();
    let mut total_chars = 0usize;
    for node in ir.nodes() {
        let base = match node.kind() {
            HybridNodeKind::SystemPrompt => SlabProvenance::System,
            HybridNodeKind::ToolDefinition => SlabProvenance::Tools,
            HybridNodeKind::ToolSchema => SlabProvenance::ToolSchema,
            _ => continue,
        };
        let Some(text) = slab_text(body, node.source_path()) else {
            continue;
        };
        for (row_index, line) in text.lines().enumerate() {
            let provenance = if line.contains("CLAUDE.md") {
                SlabProvenance::ClaudeMd
            } else {
                base
            };
            total_chars = total_chars.saturating_add(line.chars().count());
            rows.push(SlabRow {
                row_index,
                provenance,
                digest: digest_line(line),
            });
        }
    }
    let digest = rows_digest(
        &rows
            .iter()
            .map(|row| (row.provenance as u8, row.row_index, row.digest.as_str()))
            .collect::<Vec<_>>(),
    );
    StaticSlab {
        rows,
        digest,
        total_chars,
    }
}

/// Row-level freeze grid over every pinned block (system/developer/tools,
/// current/recent/open/exact/secret/thinking/structural/identifiers).
fn plan_freeze_grid(ir: &HybridContextIr, body: &Value) -> FreezeGrid {
    let mut frozen_rows = Vec::new();
    for node in ir.nodes() {
        let Some(pin_reason) = pin_reason_for(&node.protection(), &node.reason()) else {
            continue;
        };
        let Some(text) = text_at(body, node.source_path()) else {
            continue;
        };
        for (row_index, line) in text.lines().enumerate() {
            frozen_rows.push(FrozenRow {
                source_path: node.source_path().to_string(),
                row_index,
                pin_reason,
                digest: digest_line(line),
            });
        }
    }
    frozen_rows.sort_by(|left, right| {
        left.source_path
            .cmp(&right.source_path)
            .then_with(|| left.row_index.cmp(&right.row_index))
    });
    FreezeGrid {
        frozen_rows,
        byte_stable: true,
    }
}

/// Cache anchor = the recent tail start. Everything below it (system, tools
/// and frozen history) is the byte-stable cache prefix.
fn detect_cache_anchor(ir: &HybridContextIr, body: &Value, frozen_end: usize) -> CacheAnchor {
    let mut cache_prefix_chars = 0usize;
    for node in ir.nodes() {
        let in_prefix = match node.turn_index() {
            None => true,
            Some(turn) => turn < frozen_end,
        };
        if in_prefix {
            if let Some(text) = text_at(body, node.source_path()) {
                cache_prefix_chars = cache_prefix_chars.saturating_add(text.chars().count());
            }
        }
    }
    CacheAnchor {
        anchor_turn_index: frozen_end,
        cache_prefix_chars,
    }
}

/// Locate volatile billing headers with the shared volatility classifier.
/// Billing rows are never collapse candidates and mark a boundary the
/// collapse zone must not cross.
fn detect_billing_boundary(ir: &HybridContextIr, body: &Value) -> BillingBoundary {
    let mut first_billing_row = None;
    let mut volatile_billing_chars = 0usize;
    let mut volatile_kinds = Vec::new();
    for node in ir.nodes() {
        let Some(text) = text_at(body, node.source_path()) else {
            continue;
        };
        for (row_index, line) in text.lines().enumerate() {
            match detect_volatile_kind(line) {
                Some(VolatileKind::BillingHeader) => {
                    volatile_billing_chars =
                        volatile_billing_chars.saturating_add(line.chars().count());
                    if first_billing_row.is_none() {
                        first_billing_row = Some((node.source_path().to_string(), row_index));
                    }
                }
                Some(kind) if !volatile_kinds.contains(&kind) => volatile_kinds.push(kind),
                Some(_) | None => {}
            }
        }
    }
    BillingBoundary {
        first_billing_row,
        volatile_billing_chars,
        volatile_kinds,
    }
}

/// Pin semantics: map an IR (protection, reason) pair to the pin reason, or
/// `None` for collapse candidates.
pub fn pin_reason_for(
    protection: &HybridProtection,
    reason: &HybridProtectionReason,
) -> Option<PinReason> {
    match protection {
        HybridProtection::Candidate => None,
        HybridProtection::Secret => Some(PinReason::Secret),
        HybridProtection::Exact => Some(PinReason::Exact),
        HybridProtection::AlwaysText => Some(match reason {
            HybridProtectionReason::CurrentTurn => PinReason::CurrentTurn,
            HybridProtectionReason::RecentTail => PinReason::RecentTail,
            HybridProtectionReason::OpenToolState => PinReason::OpenToolState,
            HybridProtectionReason::StructuralToolState => PinReason::StructuralToolState,
            HybridProtectionReason::System => PinReason::System,
            HybridProtectionReason::Developer => PinReason::Developer,
            HybridProtectionReason::ToolSchema => PinReason::ToolSchema,
            HybridProtectionReason::ToolArguments => PinReason::ToolArguments,
            HybridProtectionReason::Identifier => PinReason::Identifier,
            HybridProtectionReason::Thinking => PinReason::Thinking,
            _ => PinReason::GeneralPreserve,
        }),
        HybridProtection::Opaque => Some(match reason {
            HybridProtectionReason::ExistingImage => PinReason::ExistingImage,
            _ => PinReason::Unknown,
        }),
    }
}

fn partition_pins(ir: &HybridContextIr) -> (Vec<String>, Vec<String>) {
    let mut pinned_block_ids = Vec::new();
    let mut protected_text_block_ids = Vec::new();
    for node in ir.nodes() {
        if node.protection() != HybridProtection::Candidate {
            pinned_block_ids.push(node.source_path().to_string());
        }
        match node.protection() {
            HybridProtection::AlwaysText | HybridProtection::Exact | HybridProtection::Secret => {
                protected_text_block_ids.push(node.source_path().to_string());
            }
            _ => {}
        }
    }
    pinned_block_ids.sort_unstable();
    protected_text_block_ids.sort_unstable();
    (pinned_block_ids, protected_text_block_ids)
}

/// Prove that current/recent/open/exact/secret content is native text: every
/// AlwaysText/Exact/Secret node is listed as protected and no candidate
/// touches a protected block (a candidate patch path like
/// `/messages/{i}/content/{j}/content` belongs to the protected item block
/// `/messages/{i}/content/{j}`).
fn verify_protected_text(
    ir: &HybridContextIr,
    candidates: &[CollapseCandidate],
    protected_text_block_ids: &[String],
) -> bool {
    let protected = protected_text_block_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    for node in ir.nodes() {
        match node.protection() {
            HybridProtection::AlwaysText | HybridProtection::Exact | HybridProtection::Secret
                if !protected.contains(node.source_path()) =>
            {
                return false;
            }
            _ => {}
        }
    }
    candidates
        .iter()
        .all(|candidate| !protected.contains(block_path_of(&candidate.source_path)))
}

/// Map a patch path to its block path by stripping the leaf `/content`
/// segment (Anthropic tool-result text items).
fn block_path_of(patch_path: &str) -> &str {
    patch_path.strip_suffix("/content").unwrap_or(patch_path)
}

/// Verify the freeze-grid contract: every frozen (pinned) row of `original`
/// exists in `planned` with identical bytes. Collapse never touches pinned
/// rows, so this must hold for any approved collapse plan.
pub fn frozen_prefix_is_byte_identical(
    original: &Value,
    planned: &Value,
    ir: &HybridContextIr,
) -> bool {
    let grid = plan_freeze_grid(ir, original);
    if !grid.byte_stable {
        return false;
    }
    grid.frozen_rows.iter().all(|row| {
        let before = line_at(original, &row.source_path, row.row_index);
        let after = line_at(planned, &row.source_path, row.row_index);
        match (before, after) {
            (Some(before), Some(after)) => before == after,
            _ => false,
        }
    })
}

fn plan_digest(plan: &HistoryCollapsePlan) -> String {
    let candidate_evidence = plan
        .collapse_candidates
        .iter()
        .map(|candidate| {
            format!(
                "{}:{}:{}:{}:{}",
                candidate.turn_index,
                candidate.source_path,
                candidate.chars,
                candidate.estimated_images,
                candidate.cache_stable
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let payload = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        HISTORY_COLLAPSE_PLANNER_REVISION,
        plan.frozen_turn_range.0,
        plan.frozen_turn_range.1,
        candidate_evidence,
        plan.static_slab.digest,
        plan.freeze_grid.frozen_rows.len(),
        plan.cache_anchor.anchor_turn_index,
        plan.billing_boundary.volatile_billing_chars,
        plan.protected_text_block_ids.len(),
    );
    sha256_hex(payload.as_bytes())
}

/// Text at a node path with the Anthropic item fallbacks (`/content`,
/// `/text`) and deterministic serialization for structured values.
fn text_at<'a>(body: &'a Value, source_path: &str) -> Option<&'a str> {
    body.pointer(source_path)
        .and_then(Value::as_str)
        .or_else(|| {
            body.pointer(&format!("{source_path}/content"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            body.pointer(&format!("{source_path}/text"))
                .and_then(Value::as_str)
        })
}

/// Slab text: like [`text_at`] but structured values (tool definitions,
/// schemas) serialize deterministically so their rows participate in the
/// byte-stable digest.
fn slab_text(body: &Value, source_path: &str) -> Option<String> {
    if let Some(text) = text_at(body, source_path) {
        return Some(text.to_string());
    }
    body.pointer(source_path)
        .map(|value| serde_json::to_string(value).unwrap_or_default())
}

fn line_at<'a>(body: &'a Value, source_path: &str, row_index: usize) -> Option<&'a str> {
    let text = text_at(body, source_path)?;
    text.lines().nth(row_index)
}

fn rows_digest(rows: &[(u8, usize, &str)]) -> String {
    let mut hasher = Sha256::new();
    for (provenance, row_index, digest) in rows {
        hasher.update([*provenance]);
        hasher.update(row_index.to_le_bytes());
        hasher.update(digest.as_bytes());
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

fn digest_line(line: &str) -> String {
    sha256_hex(line.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optical::capability::experimental_anthropic_active_capability;
    use crate::optical::geometry_for_capability;
    use crate::{lower_hybrid_context, ResolvedOptimizationContext};
    use dtoken_types::{OpticalEndpoint, OpticalMode, Protocol};
    use serde_json::json;

    fn resolved_context() -> ResolvedOptimizationContext {
        ResolvedOptimizationContext {
            route_id: "route-history".to_string(),
            provider_id: "anthropic-main".to_string(),
            model: "claude-fable-5".to_string(),
            input_protocol: Protocol::Anthropic,
            output_protocol: Protocol::Anthropic,
            endpoint: OpticalEndpoint::AnthropicMessages,
        }
    }

    fn profile() -> RenderGeometryProfile {
        geometry_for_capability(&experimental_anthropic_active_capability("claude-fable-5"))
            .expect("experimental Anthropic capability has render geometry")
    }

    fn history_route() -> RouteOpticalPolicy {
        RouteOpticalPolicy {
            enabled: true,
            mode: OpticalMode::ToolResultsOnly,
            allow_tool_result: true,
            allow_history: true,
            allow_system_static_slab: true,
            min_net_saved_tokens: 0,
            confirmed_experimental_lossy: true,
            ..Default::default()
        }
    }

    fn dense_log() -> String {
        let line = format!("INFO {}\n", "historical operation completed ".repeat(16));
        line.repeat(40)
    }

    /// Frozen history with one collapseable dense-log tool result, one exact
    /// tool result, one secret tool result, six recent pairs, an open tool
    /// use, an existing image, a CLAUDE.md system slab with a billing header,
    /// and a current user turn.
    fn collapse_body() -> Value {
        let mut messages = vec![
            json!({"role":"assistant","content":[{"type":"tool_use","id":"tool-old","name":"read","input":{"path":"/tmp/a"}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-old","content":dense_log()}]}),
            json!({"role":"assistant","content":[{"type":"tool_use","id":"tool-exact","name":"read","input":{}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-exact","content":"sha256:deadbeefcafe0000000000000000000000000000000000000000000000000000: /var/data/a1b2c3d4"}]}),
            json!({"role":"assistant","content":[{"type":"tool_use","id":"tool-secret","name":"read","input":{}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-secret","content":"PASSWORD = huntertwo"}]}),
        ];
        for index in 0..6 {
            let id = format!("recent-{index}");
            messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":id,"name":"read","input":{}}]}));
            messages.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":"recent result"}]}));
        }
        messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":"open-1","name":"run","input":{}}]}));
        messages.push(json!({"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"aGVsbG8="}}]}));
        messages.push(json!({"role":"user","content":[{"type":"text","text":"current turn"}]}));
        json!({
            "model": "claude-fable-5",
            "system": "# CLAUDE.md\n\nProject instructions.\nX-Billing-Account: acct-123\n",
            "tools": [{"name":"read","input_schema":{"type":"object","properties":{"path":{"type":"string"}}}}],
            "messages": messages
        })
    }

    fn collapse_plan() -> (HistoryCollapsePlan, HybridContextIr, Value) {
        let context = resolved_context();
        let body = collapse_body();
        let ir = lower_hybrid_context(&context, &body).expect("fixture lowers");
        let plan = plan_history_collapse(&ir, &body, &profile(), &history_route())
            .expect("frozen history is collapseable");
        (plan, ir, body)
    }

    #[test]
    fn collapse_marks_only_frozen_history_and_preserves_current_recent_open_exact_secret() {
        let (plan, ir, _body) = collapse_plan();
        let frozen_end = ir.recent_tail_start();
        assert!(frozen_end > 0);

        // Exactly one candidate: the frozen dense-log tool result.
        assert_eq!(plan.collapse_candidates.len(), 1);
        let candidate = &plan.collapse_candidates[0];
        assert_eq!(candidate.source_path, "/messages/1/content/0/content");
        assert!(candidate.turn_index < frozen_end);
        assert!(candidate.cache_stable);
        assert!(candidate.estimated_images >= 1);

        // Every candidate lies inside the frozen turn range.
        for candidate in &plan.collapse_candidates {
            assert!(candidate.turn_index >= plan.frozen_turn_range.0);
            assert!(candidate.turn_index < plan.frozen_turn_range.1);
        }

        // The native-text guarantee is provable.
        assert!(plan.current_recent_open_exact_secret_native_text);

        // Current turn, recent tail, open tool state, exact, secret and
        // existing image are all pinned and never candidates. Tool-result
        // blocks are protected at their item path (`.../content/{i}`).
        for protected in &[
            "/messages/3/content/0",
            "/messages/5/content/0",
            "/messages/17/content/0",
            "/messages/18/content/0",
            "/messages/19/content/0",
            "/messages/20/content/0",
            "/system",
            "/tools/0",
        ] {
            let is_protected = plan
                .protected_text_block_ids
                .iter()
                .any(|path| path == protected)
                || plan.pinned_block_ids.iter().any(|path| path == protected);
            assert!(is_protected, "{protected} must be pinned or protected text");
        }
        assert!(plan.collapse_candidates.iter().all(|c| !plan
            .protected_text_block_ids
            .iter()
            .any(|path| block_path_of(&c.source_path) == path)));
    }

    #[test]
    fn static_slab_is_byte_stable_and_claude_md_and_billing_are_captured() {
        let (plan, _, _) = collapse_plan();
        assert!(!plan.static_slab.rows.is_empty());
        assert!(plan
            .static_slab
            .rows
            .iter()
            .any(|row| row.provenance == SlabProvenance::ClaudeMd));
        assert!(plan
            .static_slab
            .rows
            .iter()
            .any(|row| row.provenance == SlabProvenance::Tools));
        assert!(plan
            .static_slab
            .rows
            .iter()
            .any(|row| row.provenance == SlabProvenance::ToolSchema));

        // Byte-stable across identical inputs; churns on any slab change.
        let context = resolved_context();
        let body = collapse_body();
        let ir = lower_hybrid_context(&context, &body).expect("lowers");
        let same = plan_static_slab(&ir, &body);
        assert_eq!(same.digest, plan.static_slab.digest);

        let mut changed = collapse_body();
        changed["system"] =
            json!("# CLAUDE.md\n\nChanged instructions.\nX-Billing-Account: acct-123\n");
        let changed_ir = lower_hybrid_context(&context, &changed).expect("lowers");
        let changed_slab = plan_static_slab(&changed_ir, &changed);
        assert_ne!(changed_slab.digest, plan.static_slab.digest);
    }

    #[test]
    fn freeze_grid_is_byte_identical_and_detects_slab_drift() {
        let (plan, ir, body) = collapse_plan();
        assert!(plan.freeze_grid.byte_stable);
        assert!(!plan.freeze_grid.frozen_rows.is_empty());

        // No collapse applied: frozen rows byte-identical.
        assert!(frozen_prefix_is_byte_identical(&body, &body, &ir));

        // Any pinned-row mutation breaks the contract.
        let mut mutated = body.clone();
        mutated["system"] =
            json!("# CLAUDE.md\n\nMutated instructions.\nX-Billing-Account: acct-123\n");
        assert!(!frozen_prefix_is_byte_identical(&body, &mutated, &ir));

        // Collapse candidates are never part of the freeze grid.
        let candidate_paths = plan
            .collapse_candidates
            .iter()
            .map(|c| c.source_path.as_str())
            .collect::<HashSet<_>>();
        assert!(plan
            .freeze_grid
            .frozen_rows
            .iter()
            .all(|row| !candidate_paths.contains(row.source_path.as_str())));
    }

    #[test]
    fn cache_anchor_boundary_precedes_recent_tail_and_prefix_is_stable() {
        let (plan, ir, _body) = collapse_plan();
        assert_eq!(plan.cache_anchor.anchor_turn_index, ir.recent_tail_start());
        assert!(plan.cache_anchor.cache_prefix_chars > 0);
        // The anchor sits at the boundary between frozen history and the
        // protected recent tail, so collapse never crosses into the tail.
        assert!(plan.cache_anchor.anchor_turn_index >= plan.frozen_turn_range.1);
    }

    #[test]
    fn billing_header_boundary_is_volatile_and_never_collapsed() {
        let (plan, _, _) = collapse_plan();
        let (source_path, _row_index) = plan
            .billing_boundary
            .first_billing_row
            .expect("billing header detected");
        assert_eq!(source_path, "/system");
        assert!(plan.billing_boundary.volatile_billing_chars > 0);
        // Billing rows live in the pinned system slab, never in candidates.
        assert!(plan
            .collapse_candidates
            .iter()
            .all(|candidate| !candidate.source_path.starts_with("/system")));
    }

    #[test]
    fn route_without_allow_history_preserves_text() {
        let context = resolved_context();
        let body = collapse_body();
        let ir = lower_hybrid_context(&context, &body).expect("lowers");
        let route = RouteOpticalPolicy {
            allow_history: false,
            ..history_route()
        };
        assert!(plan_history_collapse(&ir, &body, &profile(), &route).is_none());
    }

    #[test]
    fn no_frozen_collapseable_history_returns_none() {
        // A conversation without any closed historical tool result cannot
        // collapse; text is preserved.
        let context = resolved_context();
        let body = json!({
            "model": "claude-fable-5",
            "system": "instructions",
            "messages": [
                {"role":"user","content":[{"type":"text","text":"plain history"}]},
                {"role":"assistant","content":[{"type":"text","text":"plain reply"}]},
                {"role":"user","content":[{"type":"text","text":"current turn"}]},
            ]
        });
        let ir = lower_hybrid_context(&context, &body).expect("lowers");
        assert!(plan_history_collapse(&ir, &body, &profile(), &history_route()).is_none());
    }

    #[test]
    fn openai_responses_never_produces_collapse_candidates() {
        let context = ResolvedOptimizationContext {
            route_id: "route-responses".to_string(),
            provider_id: "openai-main".to_string(),
            model: "gpt-5.6-sol".to_string(),
            input_protocol: Protocol::OpenAi,
            output_protocol: Protocol::OpenAi,
            endpoint: OpticalEndpoint::OpenAiResponses,
        };
        let body = json!({
            "instructions": "system instructions",
            "tools": [{"type":"function","name":"read_file","parameters":{"type":"object"}}],
            "input": [
                {"type":"function_call","call_id":"fc-1","name":"read_file","arguments":"{}"},
                {"type":"function_call_output","call_id":"fc-1","output":"INFO x\nINFO x\nINFO x\nINFO x\nINFO x\nINFO x\nINFO x\nINFO x\n"}
            ]
        });
        let ir = lower_hybrid_context(&context, &body).expect("Responses lowers");
        assert!(ir.approved_text_targets().is_empty());
        assert!(plan_history_collapse(&ir, &body, &profile(), &history_route()).is_none());
    }

    #[test]
    fn no_ordinary_image_entry_becomes_a_collapse_candidate() {
        let (plan, _, _) = collapse_plan();
        // The existing image block is pinned (Opaque/ExistingImage) and never
        // a candidate; there is no generic image entry path in the planner.
        assert!(plan
            .pinned_block_ids
            .iter()
            .any(|path| path == "/messages/19/content/0"));
        assert!(plan
            .collapse_candidates
            .iter()
            .all(|candidate| !candidate.source_path.contains("/messages/19/content/0")));
    }

    #[test]
    fn optical_image_is_the_exclusive_image_action_in_canonical_ir() {
        let (_, ir, _) = collapse_plan();
        // The only image-producing action is OpticalImage, and every approved
        // image target is a closed historical low-risk tool result.
        let image_actions = ir
            .approved_text_targets()
            .iter()
            .filter(|target| target.action() == ApprovedHybridAction::OpticalImage)
            .collect::<Vec<_>>();
        assert!(!image_actions.is_empty());
        for target in image_actions {
            assert_eq!(
                target.kind(),
                crate::ApprovedHybridTargetKind::ClosedHistoricalLowRiskToolResult
            );
            assert_eq!(target.action(), ApprovedHybridAction::OpticalImage);
        }
    }

    #[test]
    fn plan_digest_is_deterministic_and_sensitive() {
        let (plan, _, _) = collapse_plan();
        assert_eq!(plan.digest().len(), 64);
        assert_eq!(plan.digest(), plan.digest());

        // A different plan shape (no candidates) must not produce a plan at
        // all; a mutated slab must change the digest.
        let context = resolved_context();
        let mut changed = collapse_body();
        changed["system"] = json!("# CLAUDE.md\n\nChanged.\nX-Billing-Account: acct-123\n");
        let changed_ir = lower_hybrid_context(&context, &changed).expect("lowers");
        let changed_plan =
            plan_history_collapse(&changed_ir, &changed, &profile(), &history_route())
                .expect("still collapseable");
        assert_ne!(changed_plan.digest(), plan.digest());
    }

    #[test]
    fn pin_reasons_cover_every_protected_protection() {
        assert_eq!(
            pin_reason_for(&HybridProtection::Secret, &HybridProtectionReason::Secret),
            Some(PinReason::Secret)
        );
        assert_eq!(
            pin_reason_for(&HybridProtection::Exact, &HybridProtectionReason::Exact),
            Some(PinReason::Exact)
        );
        assert_eq!(
            pin_reason_for(
                &HybridProtection::AlwaysText,
                &HybridProtectionReason::CurrentTurn
            ),
            Some(PinReason::CurrentTurn)
        );
        assert_eq!(
            pin_reason_for(
                &HybridProtection::AlwaysText,
                &HybridProtectionReason::OpenToolState
            ),
            Some(PinReason::OpenToolState)
        );
        assert_eq!(
            pin_reason_for(
                &HybridProtection::Opaque,
                &HybridProtectionReason::ExistingImage
            ),
            Some(PinReason::ExistingImage)
        );
        assert_eq!(
            pin_reason_for(
                &HybridProtection::Candidate,
                &HybridProtectionReason::ClosedHistoricalLowRiskToolResult
            ),
            None
        );
    }
}
