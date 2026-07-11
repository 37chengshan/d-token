// ConversationInventory：把 Anthropic 请求降级为统一 conversation/turn/block 结构。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §11。

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
    let total_turns = turns.len();
    let recent_tail_start = total_turns.saturating_sub(RECENT_TAIL_TURNS);
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
                    text_ref: None,
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
    let recent_tail_start = turns.len().saturating_sub(RECENT_TAIL_TURNS);
    ConversationInventory {
        protocol: Protocol::OpenAi,
        endpoint: OpticalEndpoint::OpenAiChatCompletions,
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
}
