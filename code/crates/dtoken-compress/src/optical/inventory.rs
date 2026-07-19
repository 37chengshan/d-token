// ConversationInventory：把 Anthropic 请求降级为统一 conversation/turn/block 结构。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §11。

use std::collections::HashMap;

use crate::optical::text_arena::{TextArena, TextRef, TextSensitivity};
use dtoken_types::{OpticalEndpoint, Protocol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleKind {
    System,
    Developer,
    User,
    Assistant,
    Tool,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Text,
    ToolUse,
    ToolResult,
    Image,
    SystemPrompt,
    ToolDefinition,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitialProtection {
    AlwaysText,
    Candidate,
    Opaque,
}

#[derive(Debug, Clone)]
pub struct BlockRef {
    pub block_id: String,
    pub source_path: String,
    pub kind: BlockKind,
    pub role: RoleKind,
    pub turn_index: Option<usize>,
    pub tool_id: Option<String>,
    pub chars: usize,
    pub protection: InitialProtection,
    pub text_ref: Option<TextRef>,
}

#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub index: usize,
    pub role: RoleKind,
    pub blocks: Vec<String>,
    pub opens_tool_ids: Vec<String>,
    pub closes_tool_ids: Vec<String>,
    pub contains_opaque_item: bool,
    pub contains_image: bool,
    pub typed_user_text_chars: usize,
}

#[derive(Debug, Clone)]
pub struct ConversationInventory {
    pub protocol: Protocol,
    pub endpoint: OpticalEndpoint,
    pub turns: Vec<ConversationTurn>,
    pub blocks: Vec<BlockRef>,
    pub current_user_turn: Option<usize>,
    pub recent_tail_start: usize,
}

const RECENT_TAIL_TURNS: usize = 2;
const RECENT_COMPLETED_TOOL_PAIRS: usize = 6;

fn protected_recent_tail_start(turns: &[ConversationTurn]) -> usize {
    let fallback = turns.len().saturating_sub(RECENT_TAIL_TURNS);
    let completed = validated_completed_tool_pair_close_turns(turns);
    completed
        .get(completed.len().saturating_sub(RECENT_COMPLETED_TOOL_PAIRS))
        .copied()
        .unwrap_or(fallback)
        .min(fallback)
}

fn validated_completed_tool_pair_close_turns(turns: &[ConversationTurn]) -> Vec<usize> {
    let mut opens = HashMap::<&str, Vec<usize>>::new();
    let mut closes = HashMap::<&str, Vec<usize>>::new();
    for turn in turns {
        for tool_id in &turn.opens_tool_ids {
            opens.entry(tool_id.as_str()).or_default().push(turn.index);
        }
        for tool_id in &turn.closes_tool_ids {
            closes.entry(tool_id.as_str()).or_default().push(turn.index);
        }
    }

    let mut completed = opens
        .into_iter()
        .filter_map(|(tool_id, open_turns)| {
            let close_turns = closes.get(tool_id)?;
            (open_turns.len() == 1 && close_turns.len() == 1 && open_turns[0] < close_turns[0])
                .then_some(close_turns[0])
        })
        .collect::<Vec<_>>();
    completed.sort_unstable();
    completed
}

fn text_chars(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(s) => s.chars().count(),
        _ => serde_json::to_string(value)
            .unwrap_or_default()
            .chars()
            .count(),
    }
}
fn content_item_chars(item: &serde_json::Value) -> usize {
    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
        return text.chars().count();
    }
    if let Some(content) = item.get("content") {
        return text_chars(content);
    }
    serde_json::to_string(item)
        .unwrap_or_default()
        .chars()
        .count()
}

fn content_item_text(item: &serde_json::Value) -> Option<String> {
    if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
        return Some(text.to_string());
    }
    if let Some(content) = item.get("content") {
        return match content {
            serde_json::Value::String(text) => Some(text.clone()),
            serde_json::Value::Null => None,
            other => serde_json::to_string(other).ok(),
        };
    }
    None
}

fn candidate_text_ref(
    arena: &mut Option<&mut TextArena>,
    text: Option<String>,
    protection: &InitialProtection,
) -> Option<TextRef> {
    if *protection != InitialProtection::Candidate {
        return None;
    }
    let text = text?;
    arena
        .as_deref_mut()
        .map(|arena| arena.insert(text, TextSensitivity::RenderCandidateInternal))
}

pub fn build_anthropic_inventory(body: &serde_json::Value) -> ConversationInventory {
    build_anthropic_inventory_internal(body, None)
}

pub fn build_anthropic_inventory_with_arena(
    body: &serde_json::Value,
    arena: &mut TextArena,
) -> ConversationInventory {
    build_anthropic_inventory_internal(body, Some(arena))
}

fn build_anthropic_inventory_internal(
    body: &serde_json::Value,
    mut arena: Option<&mut TextArena>,
) -> ConversationInventory {
    let mut blocks = Vec::new();
    let mut turns = Vec::new();
    let mut block_seq = 0usize;
    if let Some(system) = body.get("system") {
        blocks.push(BlockRef {
            block_id: format!("blk-{}", block_seq),
            source_path: "/system".to_string(),
            kind: BlockKind::SystemPrompt,
            role: RoleKind::System,
            turn_index: None,
            tool_id: None,
            chars: text_chars(system),
            protection: InitialProtection::AlwaysText,
            text_ref: None,
        });
        block_seq += 1;
    }
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        for (tool_index, tool) in tools.iter().enumerate() {
            let chars = serde_json::to_string(tool)
                .unwrap_or_default()
                .chars()
                .count();
            blocks.push(BlockRef {
                block_id: format!("blk-{}", block_seq),
                source_path: format!("/tools/{tool_index}"),
                kind: BlockKind::ToolDefinition,
                role: RoleKind::System,
                turn_index: None,
                tool_id: None,
                chars,
                protection: InitialProtection::AlwaysText,
                text_ref: None,
            });
            block_seq += 1;
        }
    }
    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        for (turn_idx, msg) in messages.iter().enumerate() {
            let role = match msg.get("role").and_then(|r| r.as_str()) {
                Some("user") => RoleKind::User,
                Some("assistant") => RoleKind::Assistant,
                _ => RoleKind::Unknown,
            };
            let mut turn_blocks = Vec::new();
            let mut opens_tool_ids = Vec::new();
            let mut closes_tool_ids = Vec::new();
            let mut contains_image = false;
            let mut typed_user_text_chars = 0usize;
            if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                for (content_index, item) in content.iter().enumerate() {
                    let kind_str = item.get("type").and_then(|t| t.as_str()).unwrap_or("text");
                    let (kind, mut protection) = match kind_str {
                        "tool_use" => (BlockKind::ToolUse, InitialProtection::AlwaysText),
                        "tool_result" => (BlockKind::ToolResult, InitialProtection::Candidate),
                        "image" => (BlockKind::Image, InitialProtection::Opaque),
                        "text" => (
                            BlockKind::Text,
                            if role == RoleKind::User {
                                InitialProtection::Candidate
                            } else {
                                InitialProtection::AlwaysText
                            },
                        ),
                        _ => (BlockKind::Unknown, InitialProtection::Opaque),
                    };
                    if role == RoleKind::Unknown {
                        protection = InitialProtection::Opaque;
                    }
                    if kind == BlockKind::Image {
                        contains_image = true;
                    }
                    let tool_id = match kind {
                        BlockKind::ToolUse => item
                            .get("id")
                            .and_then(|value| value.as_str())
                            .map(ToOwned::to_owned),
                        BlockKind::ToolResult => item
                            .get("tool_use_id")
                            .and_then(|value| value.as_str())
                            .map(ToOwned::to_owned),
                        _ => None,
                    };
                    if kind == BlockKind::ToolUse {
                        if let Some(id) = &tool_id {
                            opens_tool_ids.push(id.clone());
                        }
                    }
                    if kind == BlockKind::ToolResult {
                        if let Some(id) = &tool_id {
                            closes_tool_ids.push(id.clone());
                        }
                    }
                    let chars = content_item_chars(item);
                    if kind == BlockKind::Text && role == RoleKind::User {
                        typed_user_text_chars += chars;
                    }
                    let text_ref =
                        candidate_text_ref(&mut arena, content_item_text(item), &protection);
                    let bid = format!("blk-{}", block_seq);
                    blocks.push(BlockRef {
                        block_id: bid.clone(),
                        source_path: format!("/messages/{turn_idx}/content/{content_index}"),
                        kind,
                        role: role.clone(),
                        turn_index: Some(turn_idx),
                        tool_id,
                        chars,
                        protection,
                        text_ref,
                    });
                    turn_blocks.push(bid);
                    block_seq += 1;
                }
            } else if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                let chars = text.chars().count();
                let (kind, protection) = match role {
                    RoleKind::User => (BlockKind::Text, InitialProtection::Candidate),
                    RoleKind::Assistant => (BlockKind::Text, InitialProtection::AlwaysText),
                    _ => (BlockKind::Unknown, InitialProtection::Opaque),
                };
                if role == RoleKind::User {
                    typed_user_text_chars += chars;
                }
                let text_ref = candidate_text_ref(&mut arena, Some(text.to_string()), &protection);
                let bid = format!("blk-{}", block_seq);
                blocks.push(BlockRef {
                    block_id: bid.clone(),
                    source_path: format!("/messages/{turn_idx}/content"),
                    kind,
                    role: role.clone(),
                    turn_index: Some(turn_idx),
                    tool_id: None,
                    chars,
                    protection,
                    text_ref,
                });
                turn_blocks.push(bid);
                block_seq += 1;
            }
            turns.push(ConversationTurn {
                index: turn_idx,
                role,
                blocks: turn_blocks,
                opens_tool_ids,
                closes_tool_ids,
                contains_opaque_item: contains_image,
                contains_image,
                typed_user_text_chars,
            });
        }
    }
    let current_user_turn = turns.iter().rposition(|t| t.role == RoleKind::User);
    let recent_tail_start = protected_recent_tail_start(&turns);
    ConversationInventory {
        protocol: Protocol::Anthropic,
        endpoint: OpticalEndpoint::AnthropicMessages,
        turns,
        blocks,
        current_user_turn,
        recent_tail_start,
    }
}

pub fn build_openai_chat_inventory(body: &serde_json::Value) -> ConversationInventory {
    build_openai_chat_inventory_internal(body, None)
}

pub fn build_openai_chat_inventory_with_arena(
    body: &serde_json::Value,
    arena: &mut TextArena,
) -> ConversationInventory {
    build_openai_chat_inventory_internal(body, Some(arena))
}

fn build_openai_chat_inventory_internal(
    body: &serde_json::Value,
    mut arena: Option<&mut TextArena>,
) -> ConversationInventory {
    let mut blocks = Vec::new();
    let mut turns = Vec::new();
    let mut block_seq = 0usize;
    if let Some(tools) = body.get("tools").and_then(|value| value.as_array()) {
        for (tool_index, tool) in tools.iter().enumerate() {
            blocks.push(BlockRef {
                block_id: format!("blk-{block_seq}"),
                source_path: format!("/tools/{tool_index}"),
                kind: BlockKind::ToolDefinition,
                role: RoleKind::System,
                turn_index: None,
                tool_id: None,
                chars: text_chars(tool),
                protection: InitialProtection::AlwaysText,
                text_ref: None,
            });
            block_seq += 1;
        }
    }
    if let Some(messages) = body.get("messages").and_then(|value| value.as_array()) {
        for (turn_idx, message) in messages.iter().enumerate() {
            let role = match message.get("role").and_then(|value| value.as_str()) {
                Some("system") => RoleKind::System,
                Some("developer") => RoleKind::Developer,
                Some("assistant") => RoleKind::Assistant,
                Some("tool") => RoleKind::Tool,
                Some("user") => RoleKind::User,
                _ => RoleKind::Unknown,
            };
            let mut turn_blocks = Vec::new();
            let mut opens_tool_ids = Vec::new();
            let mut closes_tool_ids = Vec::new();
            if let Some(tool_calls) = message.get("tool_calls").and_then(|value| value.as_array()) {
                for (call_index, call) in tool_calls.iter().enumerate() {
                    let tool_id = call
                        .get("id")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned);
                    if let Some(id) = &tool_id {
                        opens_tool_ids.push(id.clone());
                    }
                    let block_id = format!("blk-{block_seq}");
                    blocks.push(BlockRef {
                        block_id: block_id.clone(),
                        source_path: format!("/messages/{turn_idx}/tool_calls/{call_index}"),
                        kind: BlockKind::ToolUse,
                        role: RoleKind::Assistant,
                        turn_index: Some(turn_idx),
                        tool_id,
                        chars: text_chars(call),
                        protection: InitialProtection::AlwaysText,
                        text_ref: None,
                    });
                    turn_blocks.push(block_id);
                    block_seq += 1;
                }
            }
            if role == RoleKind::Tool {
                let tool_id = message
                    .get("tool_call_id")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned);
                if let Some(id) = &tool_id {
                    closes_tool_ids.push(id.clone());
                }
                let content = message
                    .get("content")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let block_id = format!("blk-{block_seq}");
                blocks.push(BlockRef {
                    block_id: block_id.clone(),
                    source_path: format!("/messages/{turn_idx}"),
                    kind: BlockKind::ToolResult,
                    role: RoleKind::Tool,
                    turn_index: Some(turn_idx),
                    tool_id,
                    chars: text_chars(&content),
                    protection: InitialProtection::Candidate,
                    text_ref: candidate_text_ref(
                        &mut arena,
                        content.as_str().map(ToOwned::to_owned),
                        &InitialProtection::Candidate,
                    ),
                });
                turn_blocks.push(block_id);
                block_seq += 1;
            } else if let Some(content) = message.get("content") {
                if !content.is_null() {
                    let block_id = format!("blk-{block_seq}");
                    let (kind, protection) = match role {
                        RoleKind::User => (BlockKind::Text, InitialProtection::Candidate),
                        RoleKind::System | RoleKind::Developer => {
                            (BlockKind::SystemPrompt, InitialProtection::AlwaysText)
                        }
                        RoleKind::Assistant => (BlockKind::Text, InitialProtection::AlwaysText),
                        RoleKind::Unknown => (BlockKind::Unknown, InitialProtection::Opaque),
                        RoleKind::Tool => unreachable!("tool messages handled above"),
                    };
                    blocks.push(BlockRef {
                        block_id: block_id.clone(),
                        source_path: format!("/messages/{turn_idx}/content"),
                        kind,
                        role: role.clone(),
                        turn_index: Some(turn_idx),
                        tool_id: None,
                        chars: text_chars(content),
                        protection,
                        text_ref: None,
                    });
                    turn_blocks.push(block_id);
                    block_seq += 1;
                }
            }
            turns.push(ConversationTurn {
                index: turn_idx,
                role,
                blocks: turn_blocks,
                opens_tool_ids,
                closes_tool_ids,
                contains_opaque_item: false,
                contains_image: false,
                typed_user_text_chars: 0,
            });
        }
    }
    let current_user_turn = turns.iter().rposition(|turn| turn.role == RoleKind::User);
    let recent_tail_start = protected_recent_tail_start(&turns);
    ConversationInventory {
        protocol: Protocol::OpenAi,
        endpoint: OpticalEndpoint::OpenAiChatCompletions,
        turns,
        blocks,
        current_user_turn,
        recent_tail_start,
    }
}

pub fn build_openai_responses_inventory(body: &serde_json::Value) -> ConversationInventory {
    build_openai_responses_inventory_internal(body, None)
}

pub fn build_openai_responses_inventory_with_arena(
    body: &serde_json::Value,
    arena: &mut TextArena,
) -> ConversationInventory {
    build_openai_responses_inventory_internal(body, Some(arena))
}

fn build_openai_responses_inventory_internal(
    body: &serde_json::Value,
    mut arena: Option<&mut TextArena>,
) -> ConversationInventory {
    let mut blocks = Vec::new();
    let mut turns = Vec::new();
    let mut block_seq = 0usize;

    if let Some(instructions) = body.get("instructions") {
        blocks.push(BlockRef {
            block_id: format!("blk-{block_seq}"),
            source_path: "/instructions".to_string(),
            kind: BlockKind::SystemPrompt,
            role: RoleKind::Developer,
            turn_index: None,
            tool_id: None,
            chars: text_chars(instructions),
            protection: InitialProtection::AlwaysText,
            text_ref: None,
        });
        block_seq += 1;
    }
    if let Some(tools) = body.get("tools").and_then(serde_json::Value::as_array) {
        for (tool_index, tool) in tools.iter().enumerate() {
            blocks.push(BlockRef {
                block_id: format!("blk-{block_seq}"),
                source_path: format!("/tools/{tool_index}"),
                kind: BlockKind::ToolDefinition,
                role: RoleKind::System,
                turn_index: None,
                tool_id: None,
                chars: text_chars(tool),
                protection: InitialProtection::AlwaysText,
                text_ref: None,
            });
            block_seq += 1;
        }
    }

    if let Some(input) = body.get("input").and_then(serde_json::Value::as_array) {
        for (turn_idx, item) in input.iter().enumerate() {
            let item_type = item.get("type").and_then(serde_json::Value::as_str);
            let mut turn_blocks = Vec::new();
            let mut opens_tool_ids = Vec::new();
            let mut closes_tool_ids = Vec::new();
            let mut contains_opaque_item = false;
            let mut contains_image = false;
            let mut typed_user_text_chars = 0usize;
            let role = match item.get("role").and_then(serde_json::Value::as_str) {
                Some("system") => RoleKind::System,
                Some("developer") => RoleKind::Developer,
                Some("assistant") => RoleKind::Assistant,
                Some("user") => RoleKind::User,
                _ if item_type == Some("function_call") => RoleKind::Assistant,
                _ if item_type == Some("function_call_output") => RoleKind::Tool,
                _ => RoleKind::Unknown,
            };

            match item_type {
                Some("function_call") => {
                    let tool_id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned);
                    if let Some(id) = &tool_id {
                        opens_tool_ids.push(id.clone());
                    }
                    let block_id = format!("blk-{block_seq}");
                    blocks.push(BlockRef {
                        block_id: block_id.clone(),
                        source_path: format!("/input/{turn_idx}"),
                        kind: BlockKind::ToolUse,
                        role: RoleKind::Assistant,
                        turn_index: Some(turn_idx),
                        tool_id,
                        chars: text_chars(item),
                        protection: InitialProtection::AlwaysText,
                        text_ref: None,
                    });
                    turn_blocks.push(block_id);
                    block_seq += 1;
                }
                Some("function_call_output") => {
                    let tool_id = item
                        .get("call_id")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned);
                    if let Some(id) = &tool_id {
                        closes_tool_ids.push(id.clone());
                    }
                    let output = item
                        .get("output")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let protection = if output.is_string() {
                        InitialProtection::Candidate
                    } else {
                        InitialProtection::Opaque
                    };
                    contains_opaque_item = protection == InitialProtection::Opaque;
                    let block_id = format!("blk-{block_seq}");
                    blocks.push(BlockRef {
                        block_id: block_id.clone(),
                        source_path: format!("/input/{turn_idx}/output"),
                        kind: BlockKind::ToolResult,
                        role: RoleKind::Tool,
                        turn_index: Some(turn_idx),
                        tool_id,
                        chars: text_chars(&output),
                        protection: protection.clone(),
                        text_ref: candidate_text_ref(
                            &mut arena,
                            output.as_str().map(ToOwned::to_owned),
                            &protection,
                        ),
                    });
                    turn_blocks.push(block_id);
                    block_seq += 1;
                }
                Some("message") | None if item.get("role").is_some() => {
                    let content = item.get("content").unwrap_or(&serde_json::Value::Null);
                    let content_items = content
                        .as_array()
                        .cloned()
                        .unwrap_or_else(|| vec![content.clone()]);
                    for (content_index, content_item) in content_items.iter().enumerate() {
                        let content_type = content_item
                            .get("type")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("input_text");
                        let is_image = matches!(content_type, "input_image" | "image_url");
                        let (kind, protection) = if is_image {
                            contains_image = true;
                            contains_opaque_item = true;
                            (BlockKind::Image, InitialProtection::Opaque)
                        } else if matches!(content_type, "input_text" | "output_text" | "text") {
                            match role {
                                RoleKind::User => (BlockKind::Text, InitialProtection::Candidate),
                                RoleKind::System | RoleKind::Developer => {
                                    (BlockKind::SystemPrompt, InitialProtection::AlwaysText)
                                }
                                RoleKind::Assistant => {
                                    (BlockKind::Text, InitialProtection::AlwaysText)
                                }
                                _ => (BlockKind::Unknown, InitialProtection::Opaque),
                            }
                        } else {
                            contains_opaque_item = true;
                            (BlockKind::Unknown, InitialProtection::Opaque)
                        };
                        let text = if content_item.is_string() {
                            content_item.as_str().map(ToOwned::to_owned)
                        } else {
                            content_item_text(content_item)
                        };
                        let chars = text.as_deref().map_or_else(
                            || text_chars(content_item),
                            |value| value.chars().count(),
                        );
                        if kind == BlockKind::Text && role == RoleKind::User {
                            typed_user_text_chars = typed_user_text_chars.saturating_add(chars);
                        }
                        let block_id = format!("blk-{block_seq}");
                        let source_path = if content.is_array() {
                            format!("/input/{turn_idx}/content/{content_index}")
                        } else {
                            format!("/input/{turn_idx}/content")
                        };
                        blocks.push(BlockRef {
                            block_id: block_id.clone(),
                            source_path,
                            kind,
                            role: role.clone(),
                            turn_index: Some(turn_idx),
                            tool_id: None,
                            chars,
                            protection: protection.clone(),
                            text_ref: candidate_text_ref(&mut arena, text, &protection),
                        });
                        turn_blocks.push(block_id);
                        block_seq += 1;
                    }
                }
                _ => {
                    contains_opaque_item = true;
                    let block_id = format!("blk-{block_seq}");
                    blocks.push(BlockRef {
                        block_id: block_id.clone(),
                        source_path: format!("/input/{turn_idx}"),
                        kind: BlockKind::Unknown,
                        role: RoleKind::Unknown,
                        turn_index: Some(turn_idx),
                        tool_id: None,
                        chars: text_chars(item),
                        protection: InitialProtection::Opaque,
                        text_ref: None,
                    });
                    turn_blocks.push(block_id);
                    block_seq += 1;
                }
            }
            turns.push(ConversationTurn {
                index: turn_idx,
                role,
                blocks: turn_blocks,
                opens_tool_ids,
                closes_tool_ids,
                contains_opaque_item,
                contains_image,
                typed_user_text_chars,
            });
        }
    }

    let current_user_turn = turns.iter().rposition(|turn| turn.role == RoleKind::User);
    let recent_tail_start = protected_recent_tail_start(&turns);
    ConversationInventory {
        protocol: Protocol::OpenAi,
        endpoint: OpticalEndpoint::OpenAiResponses,
        turns,
        blocks,
        current_user_turn,
        recent_tail_start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anthropic_tool_result_candidate_detected() {
        let body = json!({"messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"long output".repeat(200)}]}]});
        let inv = build_anthropic_inventory(&body);
        assert!(inv.blocks.iter().any(
            |b| b.kind == BlockKind::ToolResult && b.protection == InitialProtection::Candidate
        ));
    }

    #[test]
    fn candidate_text_is_kept_only_in_request_local_arena() {
        let raw = "private tool output".repeat(200);
        let body = json!({"messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":raw}]}]});
        let mut arena = TextArena::new("request-1");
        let inv = build_anthropic_inventory_with_arena(&body, &mut arena);
        let block = inv
            .blocks
            .iter()
            .find(|block| block.kind == BlockKind::ToolResult)
            .expect("tool result should be inventoried");
        let text_ref = block
            .text_ref
            .as_ref()
            .expect("candidate should have text ref");

        assert_eq!(arena.get(text_ref), Some(raw.as_str()));
        assert!(!format!("{inv:?}").contains("private tool output"));
    }
    #[test]
    fn recent_tail_protected() {
        let body = json!({"messages":[{"role":"user","content":"old"},{"role":"assistant","content":"reply"},{"role":"user","content":"recent"},{"role":"assistant","content":"recent reply"}]});
        let inv = build_anthropic_inventory(&body);
        assert!(inv.recent_tail_start <= inv.turns.len());
    }

    #[test]
    fn anthropic_inventory_protects_recent_six_completed_tool_pairs() {
        let mut messages = Vec::new();
        for index in 0..8 {
            messages.push(json!({
                "role":"assistant",
                "content":[{"type":"tool_use","id":format!("tool-{index}"),"name":"read","input":{}}]
            }));
            messages.push(json!({
                "role":"user",
                "content":[{"type":"tool_result","tool_use_id":format!("tool-{index}"),"content":format!("result-{index}")}]
            }));
        }
        messages.push(json!({"role":"user","content":"continue"}));
        let inventory = build_anthropic_inventory(&json!({"messages":messages}));
        assert_eq!(inventory.recent_tail_start, 5);
    }

    #[test]
    fn orphan_closes_do_not_displace_recent_completed_tool_pairs() {
        let mut messages = Vec::new();
        for index in 0..8 {
            messages.push(json!({
                "role":"assistant",
                "content":[{"type":"tool_use","id":format!("tool-{index}"),"name":"read","input":{}}]
            }));
            messages.push(json!({
                "role":"user",
                "content":[{"type":"tool_result","tool_use_id":format!("tool-{index}"),"content":format!("result-{index}")}]
            }));
        }
        for index in 0..5 {
            messages.push(json!({
                "role":"user",
                "content":[{"type":"tool_result","tool_use_id":format!("orphan-{index}"),"content":"orphan"}]
            }));
        }
        messages.push(json!({"role":"user","content":"continue"}));

        let inventory = build_anthropic_inventory(&json!({"messages":messages}));
        assert_eq!(inventory.recent_tail_start, 5);
    }

    #[test]
    fn fewer_than_six_completed_pairs_remain_recent_despite_orphans_in_all_protocols() {
        let mut anthropic_messages = Vec::new();
        let mut chat_messages = Vec::new();
        let mut responses_input = Vec::new();
        for index in 0..3 {
            anthropic_messages.push(json!({
                "role":"assistant",
                "content":[{"type":"tool_use","id":format!("tool-{index}"),"name":"read","input":{}}]
            }));
            anthropic_messages.push(json!({
                "role":"user",
                "content":[{"type":"tool_result","tool_use_id":format!("tool-{index}"),"content":"closed"}]
            }));
            chat_messages.push(json!({
                "role":"assistant",
                "content":null,
                "tool_calls":[{"id":format!("call-{index}"),"type":"function","function":{"name":"read","arguments":"{}"}}]
            }));
            chat_messages.push(json!({
                "role":"tool",
                "tool_call_id":format!("call-{index}"),
                "content":"closed"
            }));
            responses_input.push(json!({
                "type":"function_call",
                "call_id":format!("response-call-{index}"),
                "name":"read",
                "arguments":"{}"
            }));
            responses_input.push(json!({
                "type":"function_call_output",
                "call_id":format!("response-call-{index}"),
                "output":"closed"
            }));
        }
        for index in 0..5 {
            anthropic_messages.push(json!({
                "role":"user",
                "content":[{"type":"tool_result","tool_use_id":format!("orphan-{index}"),"content":"orphan"}]
            }));
            chat_messages.push(json!({
                "role":"tool",
                "tool_call_id":format!("orphan-{index}"),
                "content":"orphan"
            }));
            responses_input.push(json!({
                "type":"function_call_output",
                "call_id":format!("orphan-{index}"),
                "output":"orphan"
            }));
        }
        anthropic_messages.push(json!({"role":"user","content":"continue"}));
        chat_messages.push(json!({"role":"user","content":"continue"}));
        responses_input.push(json!({
            "type":"message",
            "role":"user",
            "content":[{"type":"input_text","text":"continue"}]
        }));

        let anthropic = build_anthropic_inventory(&json!({"messages": anthropic_messages}));
        let chat = build_openai_chat_inventory(&json!({"messages": chat_messages}));
        let responses = build_openai_responses_inventory(&json!({"input": responses_input}));

        assert_eq!(anthropic.recent_tail_start, 1);
        assert_eq!(chat.recent_tail_start, 1);
        assert_eq!(responses.recent_tail_start, 1);
    }
    #[test]
    fn system_prompt_protected() {
        let body = json!({"system":"you are helpful","messages":[]});
        let inv = build_anthropic_inventory(&body);
        assert!(inv
            .blocks
            .iter()
            .any(|b| b.kind == BlockKind::SystemPrompt
                && b.protection == InitialProtection::AlwaysText));
    }
    #[test]
    fn existing_images_opaque() {
        let body = json!({"messages":[{"role":"user","content":[{"type":"image","source":{"type":"base64"}},{"type":"text","text":"describe"}]}]});
        let inv = build_anthropic_inventory(&body);
        assert!(inv
            .blocks
            .iter()
            .any(|b| b.kind == BlockKind::Image && b.protection == InitialProtection::Opaque));
    }

    #[test]
    fn openai_tool_calls_and_tool_results_are_lowered() {
        let body = json!({"messages":[
            {"role":"assistant","content":null,"tool_calls":[
                {"id":"call-1","type":"function","function":{"name":"read","arguments":"{}"}},
                {"id":"call-2","type":"function","function":{"name":"search","arguments":"{}"}}
            ]},
            {"role":"tool","tool_call_id":"call-2","content":"second"},
            {"role":"tool","tool_call_id":"call-1","content":"first"}
        ]});
        let inventory = build_openai_chat_inventory(&body);
        assert_eq!(inventory.protocol, Protocol::OpenAi);
        assert_eq!(
            inventory
                .blocks
                .iter()
                .filter(|block| block.kind == BlockKind::ToolUse)
                .count(),
            2
        );
        assert_eq!(
            inventory
                .blocks
                .iter()
                .filter(|block| block.kind == BlockKind::ToolResult)
                .count(),
            2
        );
    }

    #[test]
    fn openai_chat_inventory_protects_recent_six_completed_tool_pairs() {
        let mut messages = Vec::new();
        for index in 0..8 {
            messages.push(json!({
                "role":"assistant",
                "content":null,
                "tool_calls":[{"id":format!("call-{index}"),"type":"function","function":{"name":"read","arguments":"{}"}}]
            }));
            messages.push(json!({
                "role":"tool",
                "tool_call_id":format!("call-{index}"),
                "content":format!("result-{index}")
            }));
        }
        messages.push(json!({"role":"user","content":"continue"}));
        let inventory = build_openai_chat_inventory(&json!({"messages":messages}));
        assert_eq!(inventory.recent_tail_start, 5);
    }

    #[test]
    fn responses_inventory_uses_call_id_and_protects_recent_six_pairs() {
        let mut input = Vec::new();
        for index in 0..8 {
            input.push(json!({
                "type":"function_call",
                "id":format!("fc-{index}"),
                "call_id":format!("call-{index}"),
                "name":"read_file",
                "arguments":"{}"
            }));
            input.push(json!({
                "type":"function_call_output",
                "call_id":format!("call-{index}"),
                "output":format!("result-{index}")
            }));
        }
        input.push(json!({"type":"message","role":"user","content":[{"type":"input_text","text":"continue"}]}));
        let body = json!({"instructions":"system","tools":[{"type":"function","name":"read_file","parameters":{}}],"input":input});
        let inventory = build_openai_responses_inventory(&body);

        assert_eq!(inventory.endpoint, OpticalEndpoint::OpenAiResponses);
        assert_eq!(inventory.current_user_turn, Some(16));
        assert_eq!(inventory.recent_tail_start, 5);
        assert_eq!(
            inventory
                .blocks
                .iter()
                .filter(|block| block.kind == BlockKind::ToolResult)
                .count(),
            8
        );
        assert!(inventory.blocks.iter().any(|block| {
            block.kind == BlockKind::ToolResult
                && block.tool_id.as_deref() == Some("call-7")
                && block.source_path == "/input/15/output"
        }));
    }

    #[test]
    fn responses_non_string_tool_output_is_opaque() {
        let body = json!({"input":[
            {"type":"function_call","call_id":"call-1","name":"read","arguments":"{}"},
            {"type":"function_call_output","call_id":"call-1","output":[{"type":"input_image","image_url":"data:image/png;base64,AA=="}]}
        ]});
        let inventory = build_openai_responses_inventory(&body);
        let result = inventory
            .blocks
            .iter()
            .find(|block| block.kind == BlockKind::ToolResult)
            .expect("tool result");
        assert_eq!(result.protection, InitialProtection::Opaque);
    }
}
