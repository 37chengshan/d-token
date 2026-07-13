use std::collections::{HashMap, HashSet};

use dtoken_types::Protocol;

use super::inventory::{BlockKind, ConversationInventory, InitialProtection, RoleKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderToolStateMachine {
    AnthropicMessagesV1,
    OpenAiChatV1,
    #[default]
    UnsupportedFailClosed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextEdgeKind {
    Sequence,
    ToolUseToResult,
}

#[derive(Debug, Clone)]
pub struct ContextNode {
    pub block_id: String,
    pub source_path: String,
    pub kind: BlockKind,
    pub role: RoleKind,
    pub turn_index: Option<usize>,
    pub tool_id: Option<String>,
    pub chars: usize,
    pub protection: InitialProtection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEdge {
    pub from_block_id: String,
    pub to_block_id: String,
    pub kind: ContextEdgeKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextGraphSummary {
    pub state_machine: ProviderToolStateMachine,
    pub state_machine_version: u32,
    pub nodes: usize,
    pub edges: usize,
    pub tool_dependency_edges: usize,
    pub closed_tool_results: usize,
    pub orphan_tool_results: usize,
    pub open_tool_uses: usize,
}

#[derive(Debug, Clone)]
pub struct ContextGraph {
    pub state_machine: ProviderToolStateMachine,
    pub nodes: Vec<ContextNode>,
    pub edges: Vec<ContextEdge>,
    closed_tool_results: HashSet<String>,
    orphan_tool_results: HashSet<String>,
    open_tool_uses: HashSet<String>,
    turn_by_block_id: HashMap<String, Option<usize>>,
}

impl ContextGraph {
    pub fn is_closed_tool_result(&self, block_id: &str) -> bool {
        self.closed_tool_results.contains(block_id)
    }

    pub fn is_closed_historical_tool_result(
        &self,
        block_id: &str,
        recent_tail_start: usize,
    ) -> bool {
        self.is_closed_tool_result(block_id)
            && self
                .turn_by_block_id
                .get(block_id)
                .copied()
                .flatten()
                .map(|turn| turn < recent_tail_start)
                .unwrap_or(false)
    }

    pub fn is_orphan_tool_result(&self, block_id: &str) -> bool {
        self.orphan_tool_results.contains(block_id)
    }

    pub fn summary(&self) -> ContextGraphSummary {
        ContextGraphSummary {
            state_machine: self.state_machine,
            state_machine_version: 1,
            nodes: self.nodes.len(),
            edges: self.edges.len(),
            tool_dependency_edges: self
                .edges
                .iter()
                .filter(|edge| edge.kind == ContextEdgeKind::ToolUseToResult)
                .count(),
            closed_tool_results: self.closed_tool_results.len(),
            orphan_tool_results: self.orphan_tool_results.len(),
            open_tool_uses: self.open_tool_uses.len(),
        }
    }
}

pub fn build_context_graph(inventory: &ConversationInventory) -> ContextGraph {
    let state_machine = match &inventory.protocol {
        Protocol::Anthropic => ProviderToolStateMachine::AnthropicMessagesV1,
        Protocol::OpenAi => ProviderToolStateMachine::OpenAiChatV1,
    };
    let nodes = inventory
        .blocks
        .iter()
        .map(|block| ContextNode {
            block_id: block.block_id.clone(),
            source_path: block.source_path.clone(),
            kind: block.kind.clone(),
            role: block.role.clone(),
            turn_index: block.turn_index,
            tool_id: block.tool_id.clone(),
            chars: block.chars,
            protection: block.protection.clone(),
        })
        .collect::<Vec<_>>();

    let mut edges = nodes
        .windows(2)
        .map(|pair| ContextEdge {
            from_block_id: pair[0].block_id.clone(),
            to_block_id: pair[1].block_id.clone(),
            kind: ContextEdgeKind::Sequence,
        })
        .collect::<Vec<_>>();

    let mut tool_uses = HashMap::<String, (String, Option<usize>)>::new();
    let mut ambiguous_tool_ids = HashSet::new();
    let mut open_tool_uses = HashSet::new();
    for node in &nodes {
        if node.kind == BlockKind::ToolUse {
            if let Some(tool_id) = &node.tool_id {
                if tool_uses
                    .insert(tool_id.clone(), (node.block_id.clone(), node.turn_index))
                    .is_some()
                {
                    ambiguous_tool_ids.insert(tool_id.clone());
                }
                open_tool_uses.insert(tool_id.clone());
            }
        }
    }

    let mut closed_tool_results = HashSet::new();
    let mut closed_result_by_tool_id = HashMap::<String, String>::new();
    let mut orphan_tool_results = HashSet::new();
    let mut invalidated_dependency_results = HashSet::new();
    for node in &nodes {
        if node.kind != BlockKind::ToolResult {
            continue;
        }
        let Some(tool_id) = &node.tool_id else {
            orphan_tool_results.insert(node.block_id.clone());
            continue;
        };
        if ambiguous_tool_ids.contains(tool_id) {
            orphan_tool_results.insert(node.block_id.clone());
            continue;
        }
        let Some((tool_use_block_id, tool_use_turn)) = tool_uses.get(tool_id) else {
            orphan_tool_results.insert(node.block_id.clone());
            continue;
        };
        let dependency_is_ordered = match (*tool_use_turn, node.turn_index) {
            (Some(open_turn), Some(close_turn)) => open_turn < close_turn,
            _ => false,
        };
        if !dependency_is_ordered {
            orphan_tool_results.insert(node.block_id.clone());
            continue;
        }
        if let Some(previous_result) =
            closed_result_by_tool_id.insert(tool_id.clone(), node.block_id.clone())
        {
            closed_tool_results.remove(&previous_result);
            orphan_tool_results.insert(previous_result.clone());
            orphan_tool_results.insert(node.block_id.clone());
            invalidated_dependency_results.insert(previous_result);
            invalidated_dependency_results.insert(node.block_id.clone());
            open_tool_uses.insert(tool_id.clone());
            continue;
        }
        edges.push(ContextEdge {
            from_block_id: tool_use_block_id.clone(),
            to_block_id: node.block_id.clone(),
            kind: ContextEdgeKind::ToolUseToResult,
        });
        closed_tool_results.insert(node.block_id.clone());
        open_tool_uses.remove(tool_id);
    }
    if !invalidated_dependency_results.is_empty() {
        edges.retain(|edge| {
            edge.kind != ContextEdgeKind::ToolUseToResult
                || !invalidated_dependency_results.contains(&edge.to_block_id)
        });
    }
    let turn_by_block_id = nodes
        .iter()
        .map(|node| (node.block_id.clone(), node.turn_index))
        .collect();

    ContextGraph {
        state_machine,
        nodes,
        edges,
        closed_tool_results,
        orphan_tool_results,
        open_tool_uses,
        turn_by_block_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optical::inventory::build_anthropic_inventory;
    use serde_json::json;

    #[test]
    fn matching_tool_use_and_result_create_dependency_edge() {
        let body = json!({"messages":[
            {"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"read","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"done"}]},
            {"role":"assistant","content":"ack"},
            {"role":"user","content":"next"},
            {"role":"assistant","content":"working"}
        ]});
        let inventory = build_anthropic_inventory(&body);
        let graph = build_context_graph(&inventory);
        let result = inventory
            .blocks
            .iter()
            .find(|block| block.kind == BlockKind::ToolResult)
            .expect("tool result");

        assert!(graph.is_closed_tool_result(&result.block_id));
        assert!(
            graph.is_closed_historical_tool_result(&result.block_id, inventory.recent_tail_start)
        );
        assert_eq!(graph.summary().tool_dependency_edges, 1);
        assert_eq!(graph.summary().open_tool_uses, 0);
    }

    #[test]
    fn orphan_tool_result_is_not_closed() {
        let body = json!({"messages":[
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"missing","content":"done"}]}
        ]});
        let inventory = build_anthropic_inventory(&body);
        let graph = build_context_graph(&inventory);
        let result = inventory
            .blocks
            .iter()
            .find(|block| block.kind == BlockKind::ToolResult)
            .expect("tool result");

        assert!(!graph.is_closed_tool_result(&result.block_id));
        assert!(graph.is_orphan_tool_result(&result.block_id));
        assert_eq!(graph.summary().orphan_tool_results, 1);
    }

    #[test]
    fn duplicate_tool_ids_make_result_ambiguous_and_orphaned() {
        let body = json!({"messages":[
            {"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"read","input":{}}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"write","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"done"}]}
        ]});
        let inventory = build_anthropic_inventory(&body);
        let graph = build_context_graph(&inventory);
        let result = inventory
            .blocks
            .iter()
            .find(|block| block.kind == BlockKind::ToolResult)
            .expect("tool result");

        assert!(!graph.is_closed_tool_result(&result.block_id));
        assert!(graph.is_orphan_tool_result(&result.block_id));
    }

    #[test]
    fn duplicate_results_for_one_tool_use_fail_closed() {
        let body = json!({"messages":[
            {"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"read","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"first"}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"second"}]}
        ]});
        let inventory = build_anthropic_inventory(&body);
        let graph = build_context_graph(&inventory);
        let results = inventory
            .blocks
            .iter()
            .filter(|block| block.kind == BlockKind::ToolResult)
            .collect::<Vec<_>>();

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .all(|result| graph.is_orphan_tool_result(&result.block_id)));
        assert_eq!(graph.summary().closed_tool_results, 0);
        assert_eq!(graph.summary().tool_dependency_edges, 0);
    }

    #[test]
    fn recent_closed_tool_result_is_not_historical() {
        let body = json!({"messages":[
            {"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"read","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":"done"}]}
        ]});
        let inventory = build_anthropic_inventory(&body);
        let graph = build_context_graph(&inventory);
        let result = inventory
            .blocks
            .iter()
            .find(|block| block.kind == BlockKind::ToolResult)
            .expect("tool result");

        assert!(graph.is_closed_tool_result(&result.block_id));
        assert!(
            !graph.is_closed_historical_tool_result(&result.block_id, inventory.recent_tail_start)
        );
    }

    #[test]
    fn anthropic_parallel_results_can_close_out_of_order() {
        let body = json!({"messages":[
            {"role":"assistant","content":[
                {"type":"tool_use","id":"tool-1","name":"read","input":{}},
                {"type":"tool_use","id":"tool-2","name":"search","input":{}}
            ]},
            {"role":"user","content":[
                {"type":"tool_result","tool_use_id":"tool-2","content":"second"},
                {"type":"tool_result","tool_use_id":"tool-1","content":"first"}
            ]},
            {"role":"assistant","content":"ack"},
            {"role":"user","content":"next"},
            {"role":"assistant","content":"working"}
        ]});
        let inventory = build_anthropic_inventory(&body);
        let graph = build_context_graph(&inventory);
        assert_eq!(graph.summary().closed_tool_results, 2);
        assert_eq!(graph.summary().tool_dependency_edges, 2);
        assert_eq!(
            graph.summary().state_machine,
            ProviderToolStateMachine::AnthropicMessagesV1
        );
    }

    #[test]
    fn openai_parallel_tool_calls_use_openai_state_machine() {
        let body = json!({"messages":[
            {"role":"assistant","content":null,"tool_calls":[
                {"id":"call-1","type":"function","function":{"name":"read","arguments":"{}"}},
                {"id":"call-2","type":"function","function":{"name":"search","arguments":"{}"}}
            ]},
            {"role":"tool","tool_call_id":"call-2","content":"second"},
            {"role":"tool","tool_call_id":"call-1","content":"first"}
        ]});
        let inventory = crate::optical::inventory::build_openai_chat_inventory(&body);
        let graph = build_context_graph(&inventory);
        assert_eq!(
            graph.summary().state_machine,
            ProviderToolStateMachine::OpenAiChatV1
        );
        assert_eq!(graph.summary().closed_tool_results, 2);
        assert_eq!(graph.summary().open_tool_uses, 0);
    }
}
