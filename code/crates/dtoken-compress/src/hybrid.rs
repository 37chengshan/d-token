use dtoken_types::{OpticalEndpoint, Protocol};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::content_router::{route_content, ContentKind};
use crate::optical::context_graph::{build_context_graph, ContextGraph};
use crate::optical::inventory::{
    build_anthropic_inventory, build_openai_chat_inventory, build_openai_responses_inventory,
    BlockKind, ConversationInventory, InitialProtection, RoleKind,
};
use crate::optical::risk::{classify_block, RiskTier};
use crate::optical::volatility::plan_volatility;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOptimizationContext {
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub input_protocol: Protocol,
    pub output_protocol: Protocol,
    pub endpoint: OpticalEndpoint,
}

impl ResolvedOptimizationContext {
    pub fn supports_inventory(&self) -> bool {
        if self.route_id.trim().is_empty()
            || self.provider_id.trim().is_empty()
            || self.model.trim().is_empty()
        {
            return false;
        }
        matches!(
            (&self.input_protocol, &self.output_protocol, &self.endpoint),
            (
                Protocol::Anthropic,
                Protocol::Anthropic,
                OpticalEndpoint::AnthropicMessages
            ) | (
                Protocol::OpenAi,
                Protocol::OpenAi,
                OpticalEndpoint::OpenAiChatCompletions | OpticalEndpoint::OpenAiResponses
            )
        )
    }

    pub fn supports_safe_text_codec(&self) -> bool {
        if !self.supports_inventory() {
            return false;
        }

        matches!(
            (&self.input_protocol, &self.output_protocol, &self.endpoint),
            (
                Protocol::Anthropic,
                Protocol::Anthropic,
                OpticalEndpoint::AnthropicMessages
            ) | (
                Protocol::OpenAi,
                Protocol::OpenAi,
                OpticalEndpoint::OpenAiChatCompletions
            )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HybridNodeKind {
    SystemPrompt,
    ToolDefinition,
    ToolSchema,
    ToolUse,
    ToolArguments,
    ToolResult,
    Identifier,
    Text,
    Image,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HybridRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HybridContentProvenance {
    SystemInstruction,
    DeveloperInstruction,
    ToolSchema,
    ToolInvocation,
    ToolOutput,
    UserMessage,
    AssistantMessage,
    OpaqueInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HybridProtection {
    AlwaysText,
    Exact,
    Secret,
    Opaque,
    Candidate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HybridProtectionReason {
    System,
    Developer,
    CurrentTurn,
    RecentTail,
    OpenToolState,
    StructuralToolState,
    ToolSchema,
    ToolArguments,
    CodeOrDiff,
    Secret,
    Exact,
    ExistingImage,
    Identifier,
    UnknownNode,
    GeneralPreserve,
    ClosedHistoricalLowRiskToolResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridContextNode {
    node_id: String,
    source_path: String,
    kind: HybridNodeKind,
    role: HybridRole,
    provenance: HybridContentProvenance,
    protection: HybridProtection,
    reason: HybridProtectionReason,
    turn_index: Option<usize>,
    tool_correlation_digest: Option<String>,
}

impl HybridContextNode {
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn source_path(&self) -> &str {
        &self.source_path
    }

    pub fn kind(&self) -> HybridNodeKind {
        self.kind.clone()
    }

    pub fn role(&self) -> HybridRole {
        self.role.clone()
    }

    pub fn provenance(&self) -> HybridContentProvenance {
        self.provenance.clone()
    }

    pub fn protection(&self) -> HybridProtection {
        self.protection.clone()
    }

    pub fn reason(&self) -> HybridProtectionReason {
        self.reason.clone()
    }

    pub fn turn_index(&self) -> Option<usize> {
        self.turn_index
    }

    pub fn tool_correlation_digest(&self) -> Option<&str> {
        self.tool_correlation_digest.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovedHybridTargetKind {
    ClosedHistoricalLowRiskToolResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovedHybridAction {
    SafeTextCodec,
    OpticalImage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedHybridTarget {
    node_id: String,
    patch_path: String,
    precondition_digest: String,
    kind: ApprovedHybridTargetKind,
    action: ApprovedHybridAction,
    content_kind: ContentKind,
    risk_tier: RiskTier,
    reason_code: String,
    stable_digest: Option<String>,
    stable_chars: usize,
    volatile_chars: usize,
}

impl ApprovedHybridTarget {
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn patch_path(&self) -> &str {
        &self.patch_path
    }

    pub fn precondition_digest(&self) -> &str {
        &self.precondition_digest
    }

    pub fn kind(&self) -> ApprovedHybridTargetKind {
        self.kind.clone()
    }

    pub fn action(&self) -> ApprovedHybridAction {
        self.action
    }

    pub fn content_kind(&self) -> ContentKind {
        self.content_kind
    }

    pub fn risk_tier(&self) -> RiskTier {
        self.risk_tier.clone()
    }

    pub fn reason_code(&self) -> &str {
        &self.reason_code
    }

    pub fn stable_digest(&self) -> Option<&str> {
        self.stable_digest.as_deref()
    }

    pub fn stable_chars(&self) -> usize {
        self.stable_chars
    }

    pub fn volatile_chars(&self) -> usize {
        self.volatile_chars
    }

    pub fn cache_stable(&self) -> bool {
        self.stable_digest.is_some() && self.volatile_chars == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridContextIr {
    endpoint: OpticalEndpoint,
    input_protocol: Protocol,
    resolved_identity_digest: String,
    current_user_turn: Option<usize>,
    recent_tail_start: usize,
    nodes: Vec<HybridContextNode>,
    approved_text_targets: Vec<ApprovedHybridTarget>,
}

impl HybridContextIr {
    pub fn endpoint(&self) -> OpticalEndpoint {
        self.endpoint.clone()
    }

    pub fn input_protocol(&self) -> Protocol {
        self.input_protocol.clone()
    }

    pub fn resolved_identity_digest(&self) -> &str {
        &self.resolved_identity_digest
    }

    pub fn current_user_turn(&self) -> Option<usize> {
        self.current_user_turn
    }

    pub fn recent_tail_start(&self) -> usize {
        self.recent_tail_start
    }

    pub fn nodes(&self) -> &[HybridContextNode] {
        &self.nodes
    }

    pub fn node_at(&self, source_path: &str) -> Option<&HybridContextNode> {
        self.nodes
            .iter()
            .find(|node| node.source_path == source_path)
    }

    pub fn approved_text_targets(&self) -> &[ApprovedHybridTarget] {
        &self.approved_text_targets
    }

    pub fn representation_targets(&self) -> &[ApprovedHybridTarget] {
        &self.approved_text_targets
    }
}

struct LoweredContext {
    ir: HybridContextIr,
}

pub fn lower_hybrid_context(
    context: &ResolvedOptimizationContext,
    body: &Value,
) -> Option<HybridContextIr> {
    lower_context(context, body).map(|lowered| lowered.ir)
}

fn lower_context(context: &ResolvedOptimizationContext, body: &Value) -> Option<LoweredContext> {
    if !context.supports_inventory() {
        return None;
    }

    let inventory = match (&context.input_protocol, &context.endpoint) {
        (Protocol::Anthropic, OpticalEndpoint::AnthropicMessages) => {
            build_anthropic_inventory(body)
        }
        (Protocol::OpenAi, OpticalEndpoint::OpenAiChatCompletions) => {
            build_openai_chat_inventory(body)
        }
        (Protocol::OpenAi, OpticalEndpoint::OpenAiResponses) => {
            build_openai_responses_inventory(body)
        }
        _ => return None,
    };
    if inventory.protocol != context.input_protocol || inventory.endpoint != context.endpoint {
        return None;
    }

    let graph = build_context_graph(&inventory);
    let mut nodes = Vec::with_capacity(inventory.blocks.len());
    let mut approved_text_targets = Vec::new();
    for block in &inventory.blocks {
        let (node, target) = lower_node(context, body, &inventory, &graph, block);
        nodes.push(node);
        if let Some(target) = target {
            approved_text_targets.push(target);
        }
    }
    append_explicit_protected_nodes(context, body, &mut nodes);

    let resolved_identity_digest = digest_bytes(
        format!(
            "{}\0{}\0{}\0{:?}\0{:?}\0{:?}",
            context.route_id,
            context.provider_id,
            context.model,
            context.input_protocol,
            context.output_protocol,
            context.endpoint
        )
        .as_bytes(),
    );

    Some(LoweredContext {
        ir: HybridContextIr {
            endpoint: context.endpoint.clone(),
            input_protocol: context.input_protocol.clone(),
            resolved_identity_digest,
            current_user_turn: inventory.current_user_turn,
            recent_tail_start: inventory.recent_tail_start,
            nodes,
            approved_text_targets,
        },
    })
}

fn lower_node(
    context: &ResolvedOptimizationContext,
    body: &Value,
    inventory: &ConversationInventory,
    graph: &ContextGraph,
    block: &crate::optical::inventory::BlockRef,
) -> (HybridContextNode, Option<ApprovedHybridTarget>) {
    let kind = map_kind(&block.kind);
    let role = map_role(&block.role);
    let provenance = provenance_for(&kind, &role);
    let patch_path = patch_path_for(context, body, block);
    let patch_value = patch_path
        .as_deref()
        .and_then(|path| body.pointer(path))
        .or_else(|| body.pointer(&block.source_path));
    let text = patch_value.and_then(Value::as_str);
    let content_kind = text
        .map(route_content)
        .map_or(ContentKind::Unknown, |decision| decision.content_kind);
    let risk_plan = text.map(classify_block);
    let volatility = text.map(plan_volatility);

    let (mut protection, mut reason) = baseline_protection(block, inventory);
    let mut approved_action = None;
    if block.kind == BlockKind::ToolResult {
        if graph.is_closed_historical_tool_result(&block.block_id, inventory.recent_tail_start) {
            (protection, reason) = match risk_plan.clone() {
                Some(plan) => match plan.tier {
                    RiskTier::SecretNoDuplicate => {
                        (HybridProtection::Secret, HybridProtectionReason::Secret)
                    }
                    RiskTier::ExactTextRequired
                        if text.is_some_and(|text| {
                            crate::content_router::is_safe_lossless_candidate(text)
                                || crate::content_router::is_safe_search_codec_candidate(text)
                                || crate::content_router::is_safe_build_codec_candidate(text)
                                || crate::content_router::is_safe_code_codec_candidate(text)
                                || crate::content_router::is_safe_html_repeat_candidate(text)
                        }) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::BlockUnsafe
                        if text.is_some_and(|text| {
                            crate::content_router::is_safe_lossless_candidate(text)
                                || crate::content_router::is_safe_search_codec_candidate(text)
                                || crate::content_router::is_safe_build_codec_candidate(text)
                                || crate::content_router::is_safe_code_codec_candidate(text)
                                || crate::content_router::is_safe_html_repeat_candidate(text)
                        }) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::ExactTextRequired => {
                        (HybridProtection::Exact, HybridProtectionReason::Exact)
                    }
                    RiskTier::DecisionSensitive => (
                        HybridProtection::AlwaysText,
                        HybridProtectionReason::GeneralPreserve,
                    ),
                    RiskTier::StructuralUnsafe
                        if text
                            .is_some_and(crate::content_router::is_safe_code_codec_candidate) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::StructuralUnsafe
                        if text
                            .is_some_and(crate::content_router::is_safe_diff_codec_candidate) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::StructuralUnsafe => {
                        (HybridProtection::Opaque, HybridProtectionReason::CodeOrDiff)
                    }
                    RiskTier::BlockUnsafe => {
                        (HybridProtection::Opaque, HybridProtectionReason::Exact)
                    }
                    RiskTier::RenderSafe
                        if text
                            .is_some_and(crate::content_router::is_safe_code_codec_candidate) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::RenderSafe
                        if text.is_some_and(|text| {
                            crate::content_router::is_safe_lossless_candidate(text)
                        }) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::RenderSafe
                        if text.is_some_and(crate::content_router::is_safe_log_codec_candidate) =>
                    {
                        approved_action = Some(
                            if text.is_some_and(|text| {
                                is_low_risk_repetitive_log(text)
                                    && !crate::content_router::contains_decision_signal(text)
                            }) {
                                ApprovedHybridAction::OpticalImage
                            } else {
                                ApprovedHybridAction::SafeTextCodec
                            },
                        );
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::RenderSafe
                        if text
                            .is_some_and(crate::content_router::is_safe_search_codec_candidate) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::RenderSafe
                        if text
                            .is_some_and(crate::content_router::is_safe_build_codec_candidate) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::RenderSafe
                        if text
                            .is_some_and(crate::content_router::is_safe_text_repeat_candidate) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::RenderSafe
                        if text
                            .is_some_and(crate::content_router::is_safe_html_repeat_candidate) =>
                    {
                        approved_action = Some(ApprovedHybridAction::SafeTextCodec);
                        (
                            HybridProtection::Candidate,
                            HybridProtectionReason::ClosedHistoricalLowRiskToolResult,
                        )
                    }
                    RiskTier::RenderSafe => (
                        HybridProtection::AlwaysText,
                        HybridProtectionReason::GeneralPreserve,
                    ),
                },
                None => (
                    HybridProtection::Opaque,
                    HybridProtectionReason::UnknownNode,
                ),
            };
        } else if block
            .turn_index
            .is_some_and(|turn| turn >= inventory.recent_tail_start)
        {
            protection = HybridProtection::AlwaysText;
            reason = HybridProtectionReason::RecentTail;
        } else {
            protection = HybridProtection::AlwaysText;
            reason = HybridProtectionReason::StructuralToolState;
        }
    }

    let node_id = node_id_for(
        context,
        &block.source_path,
        &kind,
        &role,
        block.tool_id.as_deref(),
    );
    let tool_correlation_digest = block
        .tool_id
        .as_deref()
        .map(|tool_id| digest_bytes(tool_id.as_bytes()));
    let target = if protection == HybridProtection::Candidate {
        match (patch_path, patch_value, approved_action) {
            (Some(patch_path), Some(value), Some(action)) => Some(ApprovedHybridTarget {
                node_id: node_id.clone(),
                patch_path,
                precondition_digest: digest_json_value(value),
                kind: ApprovedHybridTargetKind::ClosedHistoricalLowRiskToolResult,
                action,
                content_kind,
                risk_tier: risk_plan
                    .as_ref()
                    .map_or(RiskTier::BlockUnsafe, |plan| plan.tier.clone()),
                reason_code: match action {
                    ApprovedHybridAction::SafeTextCodec => "safe_text_codec",
                    ApprovedHybridAction::OpticalImage => "optical_image_candidate",
                }
                .to_string(),
                stable_digest: volatility
                    .as_ref()
                    .and_then(|plan| plan.stable_digest.clone()),
                stable_chars: volatility.as_ref().map_or(0, |plan| {
                    plan.stable_segments
                        .iter()
                        .map(|segment| segment.chars)
                        .sum()
                }),
                volatile_chars: volatility.as_ref().map_or(0, |plan| plan.volatile_chars),
            }),
            _ => None,
        }
    } else {
        None
    };

    (
        HybridContextNode {
            node_id,
            source_path: block.source_path.clone(),
            kind,
            role,
            provenance,
            protection,
            reason,
            turn_index: block.turn_index,
            tool_correlation_digest,
        },
        target,
    )
}

fn baseline_protection(
    block: &crate::optical::inventory::BlockRef,
    inventory: &ConversationInventory,
) -> (HybridProtection, HybridProtectionReason) {
    if block.kind == BlockKind::Unknown || block.role == RoleKind::Unknown {
        return (
            HybridProtection::Opaque,
            HybridProtectionReason::UnknownNode,
        );
    }
    if block.kind == BlockKind::Image || block.protection == InitialProtection::Opaque {
        return (
            HybridProtection::Opaque,
            HybridProtectionReason::ExistingImage,
        );
    }
    if block.role == RoleKind::Developer {
        return (
            HybridProtection::AlwaysText,
            HybridProtectionReason::Developer,
        );
    }
    if block.kind == BlockKind::SystemPrompt {
        return (HybridProtection::AlwaysText, HybridProtectionReason::System);
    }
    if block.kind == BlockKind::ToolDefinition {
        return (
            HybridProtection::AlwaysText,
            HybridProtectionReason::ToolSchema,
        );
    }
    if block.kind == BlockKind::ToolUse {
        return (
            HybridProtection::AlwaysText,
            HybridProtectionReason::OpenToolState,
        );
    }
    if block.turn_index == inventory.current_user_turn {
        return (
            HybridProtection::AlwaysText,
            HybridProtectionReason::CurrentTurn,
        );
    }
    if block
        .turn_index
        .is_some_and(|turn| turn >= inventory.recent_tail_start)
    {
        return (
            HybridProtection::AlwaysText,
            HybridProtectionReason::RecentTail,
        );
    }
    (
        HybridProtection::AlwaysText,
        HybridProtectionReason::GeneralPreserve,
    )
}

fn patch_path_for(
    context: &ResolvedOptimizationContext,
    body: &Value,
    block: &crate::optical::inventory::BlockRef,
) -> Option<String> {
    if block.kind != BlockKind::ToolResult {
        return None;
    }
    let path = match context.endpoint {
        OpticalEndpoint::AnthropicMessages | OpticalEndpoint::OpenAiChatCompletions => {
            format!("{}/content", block.source_path)
        }
        _ => return None,
    };
    body.pointer(&path).and_then(Value::as_str)?;
    Some(path)
}

fn append_explicit_protected_nodes(
    context: &ResolvedOptimizationContext,
    body: &Value,
    nodes: &mut Vec<HybridContextNode>,
) {
    match context.endpoint {
        OpticalEndpoint::AnthropicMessages => {
            if let Some(tools) = body.get("tools").and_then(Value::as_array) {
                for tool_index in 0..tools.len() {
                    push_explicit_node(
                        context,
                        body,
                        nodes,
                        format!("/tools/{tool_index}/input_schema"),
                        HybridNodeKind::ToolSchema,
                        HybridRole::System,
                        HybridContentProvenance::ToolSchema,
                        HybridProtection::AlwaysText,
                        HybridProtectionReason::ToolSchema,
                        None,
                    );
                    push_explicit_node(
                        context,
                        body,
                        nodes,
                        format!("/tools/{tool_index}/name"),
                        HybridNodeKind::Identifier,
                        HybridRole::System,
                        HybridContentProvenance::ToolSchema,
                        HybridProtection::Exact,
                        HybridProtectionReason::Identifier,
                        None,
                    );
                }
            }
            if let Some(messages) = body.get("messages").and_then(Value::as_array) {
                for (turn_index, message) in messages.iter().enumerate() {
                    let role = hybrid_role_from_value(message.get("role"));
                    let Some(content) = message.get("content").and_then(Value::as_array) else {
                        continue;
                    };
                    for (content_index, item) in content.iter().enumerate() {
                        let base = format!("/messages/{turn_index}/content/{content_index}");
                        match item.get("type").and_then(Value::as_str) {
                            Some("tool_use") => {
                                push_explicit_node(
                                    context,
                                    body,
                                    nodes,
                                    format!("{base}/id"),
                                    HybridNodeKind::Identifier,
                                    role.clone(),
                                    HybridContentProvenance::ToolInvocation,
                                    HybridProtection::Exact,
                                    HybridProtectionReason::Identifier,
                                    Some(turn_index),
                                );
                                push_explicit_node(
                                    context,
                                    body,
                                    nodes,
                                    format!("{base}/name"),
                                    HybridNodeKind::Identifier,
                                    role.clone(),
                                    HybridContentProvenance::ToolInvocation,
                                    HybridProtection::Exact,
                                    HybridProtectionReason::Identifier,
                                    Some(turn_index),
                                );
                                push_explicit_node(
                                    context,
                                    body,
                                    nodes,
                                    format!("{base}/input"),
                                    HybridNodeKind::ToolArguments,
                                    role.clone(),
                                    HybridContentProvenance::ToolInvocation,
                                    HybridProtection::Exact,
                                    HybridProtectionReason::ToolArguments,
                                    Some(turn_index),
                                );
                            }
                            Some("tool_result") => push_explicit_node(
                                context,
                                body,
                                nodes,
                                format!("{base}/tool_use_id"),
                                HybridNodeKind::Identifier,
                                role.clone(),
                                HybridContentProvenance::ToolOutput,
                                HybridProtection::Exact,
                                HybridProtectionReason::Identifier,
                                Some(turn_index),
                            ),
                            _ => {}
                        }
                    }
                }
            }
        }
        OpticalEndpoint::OpenAiChatCompletions => {
            if let Some(tools) = body.get("tools").and_then(Value::as_array) {
                for tool_index in 0..tools.len() {
                    push_explicit_node(
                        context,
                        body,
                        nodes,
                        format!("/tools/{tool_index}/function/parameters"),
                        HybridNodeKind::ToolSchema,
                        HybridRole::System,
                        HybridContentProvenance::ToolSchema,
                        HybridProtection::AlwaysText,
                        HybridProtectionReason::ToolSchema,
                        None,
                    );
                    push_explicit_node(
                        context,
                        body,
                        nodes,
                        format!("/tools/{tool_index}/function/name"),
                        HybridNodeKind::Identifier,
                        HybridRole::System,
                        HybridContentProvenance::ToolSchema,
                        HybridProtection::Exact,
                        HybridProtectionReason::Identifier,
                        None,
                    );
                }
            }
            if let Some(messages) = body.get("messages").and_then(Value::as_array) {
                for (turn_index, message) in messages.iter().enumerate() {
                    let role = hybrid_role_from_value(message.get("role"));
                    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                        for call_index in 0..calls.len() {
                            let base = format!("/messages/{turn_index}/tool_calls/{call_index}");
                            for (suffix, kind, reason) in [
                                (
                                    "id",
                                    HybridNodeKind::Identifier,
                                    HybridProtectionReason::Identifier,
                                ),
                                (
                                    "function/name",
                                    HybridNodeKind::Identifier,
                                    HybridProtectionReason::Identifier,
                                ),
                                (
                                    "function/arguments",
                                    HybridNodeKind::ToolArguments,
                                    HybridProtectionReason::ToolArguments,
                                ),
                            ] {
                                push_explicit_node(
                                    context,
                                    body,
                                    nodes,
                                    format!("{base}/{suffix}"),
                                    kind,
                                    role.clone(),
                                    HybridContentProvenance::ToolInvocation,
                                    HybridProtection::Exact,
                                    reason,
                                    Some(turn_index),
                                );
                            }
                        }
                    }
                    if message.get("role").and_then(Value::as_str) == Some("tool") {
                        push_explicit_node(
                            context,
                            body,
                            nodes,
                            format!("/messages/{turn_index}/tool_call_id"),
                            HybridNodeKind::Identifier,
                            HybridRole::Tool,
                            HybridContentProvenance::ToolOutput,
                            HybridProtection::Exact,
                            HybridProtectionReason::Identifier,
                            Some(turn_index),
                        );
                    }
                }
            }
        }
        OpticalEndpoint::OpenAiResponses => {
            if let Some(tools) = body.get("tools").and_then(Value::as_array) {
                for tool_index in 0..tools.len() {
                    for (suffix, kind, reason) in [
                        (
                            "parameters",
                            HybridNodeKind::ToolSchema,
                            HybridProtectionReason::ToolSchema,
                        ),
                        (
                            "name",
                            HybridNodeKind::Identifier,
                            HybridProtectionReason::Identifier,
                        ),
                    ] {
                        push_explicit_node(
                            context,
                            body,
                            nodes,
                            format!("/tools/{tool_index}/{suffix}"),
                            kind,
                            HybridRole::System,
                            HybridContentProvenance::ToolSchema,
                            HybridProtection::AlwaysText,
                            reason,
                            None,
                        );
                    }
                }
            }
            if let Some(input) = body.get("input").and_then(Value::as_array) {
                for (turn_index, item) in input.iter().enumerate() {
                    let item_type = item.get("type").and_then(Value::as_str);
                    if item_type == Some("function_call") {
                        for (suffix, kind, reason) in [
                            (
                                "call_id",
                                HybridNodeKind::Identifier,
                                HybridProtectionReason::Identifier,
                            ),
                            (
                                "name",
                                HybridNodeKind::Identifier,
                                HybridProtectionReason::Identifier,
                            ),
                            (
                                "arguments",
                                HybridNodeKind::ToolArguments,
                                HybridProtectionReason::ToolArguments,
                            ),
                        ] {
                            push_explicit_node(
                                context,
                                body,
                                nodes,
                                format!("/input/{turn_index}/{suffix}"),
                                kind,
                                HybridRole::Assistant,
                                HybridContentProvenance::ToolInvocation,
                                HybridProtection::Exact,
                                reason,
                                Some(turn_index),
                            );
                        }
                    } else if item_type == Some("function_call_output") {
                        push_explicit_node(
                            context,
                            body,
                            nodes,
                            format!("/input/{turn_index}/call_id"),
                            HybridNodeKind::Identifier,
                            HybridRole::Tool,
                            HybridContentProvenance::ToolOutput,
                            HybridProtection::Exact,
                            HybridProtectionReason::Identifier,
                            Some(turn_index),
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn push_explicit_node(
    context: &ResolvedOptimizationContext,
    body: &Value,
    nodes: &mut Vec<HybridContextNode>,
    source_path: String,
    kind: HybridNodeKind,
    role: HybridRole,
    provenance: HybridContentProvenance,
    protection: HybridProtection,
    reason: HybridProtectionReason,
    turn_index: Option<usize>,
) {
    let Some(value) = body.pointer(&source_path) else {
        return;
    };
    if nodes.iter().any(|node| node.source_path == source_path) {
        return;
    }
    let tool_correlation_digest = if kind == HybridNodeKind::Identifier {
        Some(digest_json_value(value))
    } else {
        None
    };
    let node_id = node_id_for(context, &source_path, &kind, &role, None);
    nodes.push(HybridContextNode {
        node_id,
        source_path,
        kind,
        role,
        provenance,
        protection,
        reason,
        turn_index,
        tool_correlation_digest,
    });
}

fn node_id_for(
    context: &ResolvedOptimizationContext,
    source_path: &str,
    kind: &HybridNodeKind,
    role: &HybridRole,
    tool_id: Option<&str>,
) -> String {
    digest_bytes(
        format!(
            "{:?}\0{}\0{:?}\0{:?}\0{}",
            context.endpoint,
            source_path,
            kind,
            role,
            tool_id.unwrap_or("")
        )
        .as_bytes(),
    )
}

fn hybrid_role_from_value(role: Option<&Value>) -> HybridRole {
    match role.and_then(Value::as_str) {
        Some("system") => HybridRole::System,
        Some("developer") => HybridRole::Developer,
        Some("user") => HybridRole::User,
        Some("assistant") => HybridRole::Assistant,
        Some("tool") => HybridRole::Tool,
        _ => HybridRole::Unknown,
    }
}

fn map_kind(kind: &BlockKind) -> HybridNodeKind {
    match kind {
        BlockKind::SystemPrompt => HybridNodeKind::SystemPrompt,
        BlockKind::ToolDefinition => HybridNodeKind::ToolDefinition,
        BlockKind::ToolUse => HybridNodeKind::ToolUse,
        BlockKind::ToolResult => HybridNodeKind::ToolResult,
        BlockKind::Text => HybridNodeKind::Text,
        BlockKind::Image => HybridNodeKind::Image,
        BlockKind::Unknown => HybridNodeKind::Unknown,
    }
}

fn map_role(role: &RoleKind) -> HybridRole {
    match role {
        RoleKind::System => HybridRole::System,
        RoleKind::Developer => HybridRole::Developer,
        RoleKind::User => HybridRole::User,
        RoleKind::Assistant => HybridRole::Assistant,
        RoleKind::Tool => HybridRole::Tool,
        RoleKind::Unknown => HybridRole::Unknown,
    }
}

fn provenance_for(kind: &HybridNodeKind, role: &HybridRole) -> HybridContentProvenance {
    match kind {
        HybridNodeKind::SystemPrompt => match role {
            HybridRole::Developer => HybridContentProvenance::DeveloperInstruction,
            _ => HybridContentProvenance::SystemInstruction,
        },
        HybridNodeKind::ToolDefinition | HybridNodeKind::ToolSchema => {
            HybridContentProvenance::ToolSchema
        }
        HybridNodeKind::ToolUse | HybridNodeKind::ToolArguments | HybridNodeKind::Identifier => {
            HybridContentProvenance::ToolInvocation
        }
        HybridNodeKind::ToolResult => HybridContentProvenance::ToolOutput,
        HybridNodeKind::Image | HybridNodeKind::Unknown => HybridContentProvenance::OpaqueInput,
        HybridNodeKind::Text => match role {
            HybridRole::User => HybridContentProvenance::UserMessage,
            HybridRole::Assistant => HybridContentProvenance::AssistantMessage,
            HybridRole::System => HybridContentProvenance::SystemInstruction,
            HybridRole::Developer => HybridContentProvenance::DeveloperInstruction,
            HybridRole::Tool => HybridContentProvenance::ToolOutput,
            HybridRole::Unknown => HybridContentProvenance::OpaqueInput,
        },
    }
}

fn is_low_risk_repetitive_log(text: &str) -> bool {
    if text.len() > 256 * 1024 || text.contains('\0') {
        return false;
    }
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() < 8 {
        return false;
    }

    let mut has_repeat = false;
    for window in lines.windows(2) {
        has_repeat |= window[0] == window[1];
    }
    has_repeat
        && lines.iter().all(|line| {
            let payload = line
                .strip_prefix("INFO ")
                .or_else(|| line.strip_prefix("WARN "))
                .or_else(|| line.strip_prefix("DEBUG "));
            payload.is_some_and(|payload| {
                !payload.is_empty()
                    && payload.chars().all(|character| {
                        character.is_ascii_alphabetic() || character == ' ' || character == '='
                    })
            })
        })
}

fn digest_json_value(value: &Value) -> String {
    digest_bytes(&serde_json::to_vec(value).unwrap_or_default())
}

fn digest_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn apply_safe_text_codec(
    context: &ResolvedOptimizationContext,
    policy: dtoken_types::CompressionPolicy,
    mut body: Value,
    transforms: &mut Vec<String>,
) -> Value {
    if matches!(policy, dtoken_types::CompressionPolicy::Off) {
        return body;
    }
    let Some(lowered) = lower_context(context, &body) else {
        return body;
    };

    for target in lowered.ir.approved_text_targets() {
        if target.kind != ApprovedHybridTargetKind::ClosedHistoricalLowRiskToolResult {
            continue;
        }
        let Some(current) = body.pointer(&target.patch_path) else {
            continue;
        };
        if digest_json_value(current) != target.precondition_digest {
            continue;
        }
        let Some(text) = current.as_str() else {
            continue;
        };
        let compressed = crate::compressor_registry::production_compressor_registry().compress(
            policy.clone(),
            text,
            transforms,
        );
        if compressed == text {
            continue;
        }
        let Some(patch_value) = body.pointer_mut(&target.patch_path) else {
            continue;
        };
        *patch_value = Value::String(compressed);
        crate::push_transform(transforms, "semantic_closed_tool_result");
    }

    body
}
