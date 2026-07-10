// ConversationInventory：把 Anthropic 请求降级为统一 conversation/turn/block 结构。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §11。

use dtoken_types::{OpticalEndpoint, Protocol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleKind {
    System,
    User,
    Assistant,
    Tool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Text,
    ToolUse,
    ToolResult,
    Image,
    SystemPrompt,
    ToolDefinition,
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
    pub kind: BlockKind,
    pub role: RoleKind,
    pub turn_index: Option<usize>,
    pub chars: usize,
    pub protection: InitialProtection,
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

pub fn build_anthropic_inventory(body: &serde_json::Value) -> ConversationInventory {
    let mut blocks = Vec::new();
    let mut turns = Vec::new();
    let mut block_seq = 0usize;
    if let Some(system) = body.get("system") {
        blocks.push(BlockRef {
            block_id: format!("blk-{}", block_seq),
            kind: BlockKind::SystemPrompt,
            role: RoleKind::System,
            turn_index: None,
            chars: text_chars(system),
            protection: InitialProtection::AlwaysText,
        });
        block_seq += 1;
    }
    if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
        for tool in tools {
            let chars = serde_json::to_string(tool)
                .unwrap_or_default()
                .chars()
                .count();
            blocks.push(BlockRef {
                block_id: format!("blk-{}", block_seq),
                kind: BlockKind::ToolDefinition,
                role: RoleKind::System,
                turn_index: None,
                chars,
                protection: InitialProtection::AlwaysText,
            });
            block_seq += 1;
        }
    }
    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        for (turn_idx, msg) in messages.iter().enumerate() {
            let role = match msg.get("role").and_then(|r| r.as_str()) {
                Some("user") => RoleKind::User,
                Some("assistant") => RoleKind::Assistant,
                _ => RoleKind::User,
            };
            let mut turn_blocks = Vec::new();
            let mut opens_tool_ids = Vec::new();
            let mut closes_tool_ids = Vec::new();
            let mut contains_image = false;
            let mut typed_user_text_chars = 0usize;
            if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                for item in content {
                    let kind_str = item.get("type").and_then(|t| t.as_str()).unwrap_or("text");
                    let (kind, protection) = match kind_str {
                        "tool_use" => (BlockKind::ToolUse, InitialProtection::AlwaysText),
                        "tool_result" => (BlockKind::ToolResult, InitialProtection::Candidate),
                        "image" => (BlockKind::Image, InitialProtection::Opaque),
                        _ => (
                            BlockKind::Text,
                            if role == RoleKind::User {
                                InitialProtection::Candidate
                            } else {
                                InitialProtection::AlwaysText
                            },
                        ),
                    };
                    if kind == BlockKind::Image {
                        contains_image = true;
                    }
                    if kind == BlockKind::ToolUse {
                        if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                            opens_tool_ids.push(id.to_string());
                        }
                    }
                    if kind == BlockKind::ToolResult {
                        if let Some(id) = item.get("tool_use_id").and_then(|i| i.as_str()) {
                            closes_tool_ids.push(id.to_string());
                        }
                    }
                    let chars = content_item_chars(item);
                    if kind == BlockKind::Text && role == RoleKind::User {
                        typed_user_text_chars += chars;
                    }
                    let bid = format!("blk-{}", block_seq);
                    blocks.push(BlockRef {
                        block_id: bid.clone(),
                        kind,
                        role: role.clone(),
                        turn_index: Some(turn_idx),
                        chars,
                        protection,
                    });
                    turn_blocks.push(bid);
                    block_seq += 1;
                }
            } else if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                let chars = text.chars().count();
                let protection = if role == RoleKind::User {
                    InitialProtection::Candidate
                } else {
                    InitialProtection::AlwaysText
                };
                if role == RoleKind::User {
                    typed_user_text_chars += chars;
                }
                let bid = format!("blk-{}", block_seq);
                blocks.push(BlockRef {
                    block_id: bid.clone(),
                    kind: BlockKind::Text,
                    role: role.clone(),
                    turn_index: Some(turn_idx),
                    chars,
                    protection,
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
}
