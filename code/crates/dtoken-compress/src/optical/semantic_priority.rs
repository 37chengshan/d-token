// SemanticPriorityPlanner：分配 AlwaysText 给 current user turn 和 open tool state。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §12-13。

use crate::optical::inventory::{ConversationInventory, InitialProtection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticPriority {
    AlwaysText,
    CandidateForImage,
    FrozenHistoryCandidate,
}

#[derive(Debug, Clone)]
pub struct SemanticPriorityPlan {
    pub block_priorities: Vec<(String, SemanticPriority)>,
    pub current_user_always_text: bool,
    pub open_tool_state_always_text: bool,
    pub latest_collapsed_user_pointer: Option<String>,
}

/// 由 inventory 派生语义优先级。
pub fn plan_semantic_priority(inv: &ConversationInventory) -> SemanticPriorityPlan {
    let mut block_priorities = Vec::new();
    let mut current_user_always_text = false;
    let mut open_tool_state_always_text = false;

    // 检测 open tool state（有 tool_use 但没有对应 tool_result）
    let mut open_tool_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for turn in &inv.turns {
        for id in &turn.opens_tool_ids {
            open_tool_ids.insert(id.clone());
        }
        for id in &turn.closes_tool_ids {
            open_tool_ids.remove(id);
        }
    }
    if !open_tool_ids.is_empty() {
        open_tool_state_always_text = true;
    }

    for block in &inv.blocks {
        let priority = if let Some(turn_idx) = block.turn_index {
            if Some(turn_idx) == inv.current_user_turn {
                current_user_always_text = true;
                SemanticPriority::AlwaysText
            } else if turn_idx >= inv.recent_tail_start
                || matches!(
                    block.protection,
                    InitialProtection::AlwaysText | InitialProtection::Opaque
                )
            {
                SemanticPriority::AlwaysText
            } else {
                SemanticPriority::CandidateForImage
            }
        } else {
            SemanticPriority::AlwaysText // system/tools
        };
        block_priorities.push((block.block_id.clone(), priority));
    }

    // latest collapsed user pointer：如果历史被图片化，最新的 collapsed user 需要一个文本指针
    let latest_collapsed_user_pointer = if inv.turns.len() > 4 {
        Some(format!(
            "see-image-history-up-to-turn-{}",
            inv.recent_tail_start
        ))
    } else {
        None
    };

    SemanticPriorityPlan {
        block_priorities,
        current_user_always_text,
        open_tool_state_always_text,
        latest_collapsed_user_pointer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optical::inventory::build_anthropic_inventory;
    use serde_json::json;

    #[test]
    fn current_user_always_text() {
        let body = json!({"messages":[
            {"role":"user","content":"old message"},
            {"role":"assistant","content":"reply"},
            {"role":"user","content":"current question"}
        ]});
        let inv = build_anthropic_inventory(&body);
        let plan = plan_semantic_priority(&inv);
        assert!(plan.current_user_always_text);
    }

    #[test]
    fn open_tool_state_always_text() {
        let body = json!({"messages":[
            {"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"run","input":{}}]},
            {"role":"user","content":"follow up before result"}
        ]});
        let inv = build_anthropic_inventory(&body);
        let plan = plan_semantic_priority(&inv);
        assert!(plan.open_tool_state_always_text);
    }

    #[test]
    fn latest_collapsed_user_pointer_generated_for_imaged_history() {
        let mut msgs = Vec::new();
        for i in 0..6 {
            msgs.push(json!({"role":"user","content":format!("msg {}",i)}));
            msgs.push(json!({"role":"assistant","content":"reply"}));
        }
        let body = json!({"messages":msgs});
        let inv = build_anthropic_inventory(&body);
        let plan = plan_semantic_priority(&inv);
        assert!(plan.latest_collapsed_user_pointer.is_some());
    }
}
