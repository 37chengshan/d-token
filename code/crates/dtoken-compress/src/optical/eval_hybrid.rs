use super::secret::contains_secret_like;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalHybridBlockKind {
    SystemDeveloper,
    CurrentUser,
    RecentTail,
    ToolSchema,
    ToolIdentifier,
    DiffPatch,
    ExactIdentifier,
    LowRiskHistory,
    ArchitectureBackground,
    ClosedToolPair,
    OpenToolState,
    OrphanToolState,
    MalformedToolState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalHybridBlock {
    pub block_id: String,
    pub kind: EvalHybridBlockKind,
    pub text: String,
    pub sequence: usize,
    pub stable_and_economic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalHybridDecision {
    pub block_id: String,
    pub keep_text: bool,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalHybridPartition {
    pub image_block_ids: Vec<String>,
    pub text_block_ids: Vec<String>,
    pub decisions: Vec<EvalHybridDecision>,
}

pub fn partition_eval_hybrid(blocks: &[EvalHybridBlock]) -> EvalHybridPartition {
    let mut closed_sequences = blocks
        .iter()
        .filter(|block| block.kind == EvalHybridBlockKind::ClosedToolPair)
        .map(|block| block.sequence)
        .collect::<Vec<_>>();
    closed_sequences.sort_unstable();
    let recent_closed_floor = closed_sequences
        .len()
        .checked_sub(6)
        .and_then(|index| closed_sequences.get(index).copied());

    let mut image_block_ids = Vec::new();
    let mut text_block_ids = Vec::new();
    let mut decisions = Vec::new();
    for block in blocks {
        let (keep_text, reason_code) = classify(block, recent_closed_floor);
        if keep_text {
            text_block_ids.push(block.block_id.clone());
        } else {
            image_block_ids.push(block.block_id.clone());
        }
        decisions.push(EvalHybridDecision {
            block_id: block.block_id.clone(),
            keep_text,
            reason_code: reason_code.to_string(),
        });
    }
    EvalHybridPartition {
        image_block_ids,
        text_block_ids,
        decisions,
    }
}

fn classify(block: &EvalHybridBlock, recent_closed_floor: Option<usize>) -> (bool, &'static str) {
    if contains_secret_like(&block.text) {
        return (true, "secret_first_text");
    }
    match block.kind {
        EvalHybridBlockKind::SystemDeveloper => (true, "system_developer_text"),
        EvalHybridBlockKind::CurrentUser => (true, "current_user_text"),
        EvalHybridBlockKind::RecentTail => (true, "recent_tail_text"),
        EvalHybridBlockKind::ToolSchema => (true, "tool_schema_text"),
        EvalHybridBlockKind::ToolIdentifier => (true, "tool_identifier_text"),
        EvalHybridBlockKind::DiffPatch => (true, "diff_patch_text"),
        EvalHybridBlockKind::ExactIdentifier => (true, "exact_identifier_text"),
        EvalHybridBlockKind::OpenToolState => (true, "open_tool_state_text"),
        EvalHybridBlockKind::OrphanToolState => (true, "orphan_tool_state_text"),
        EvalHybridBlockKind::MalformedToolState => (true, "malformed_tool_state_text"),
        EvalHybridBlockKind::ClosedToolPair
            if recent_closed_floor.is_some_and(|floor| block.sequence >= floor) =>
        {
            (true, "recent_six_closed_tool_pairs_text")
        }
        EvalHybridBlockKind::LowRiskHistory
        | EvalHybridBlockKind::ArchitectureBackground
        | EvalHybridBlockKind::ClosedToolPair
            if block.stable_and_economic =>
        {
            (false, "stable_low_risk_image_candidate")
        }
        _ => (true, "economics_or_stability_not_proven"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: &str, kind: EvalHybridBlockKind, sequence: usize) -> EvalHybridBlock {
        EvalHybridBlock {
            block_id: id.to_string(),
            kind,
            text: format!("public synthetic {id}"),
            sequence,
            stable_and_economic: true,
        }
    }

    #[test]
    fn preserves_recent_six_and_all_incomplete_tool_states() {
        let mut blocks = (0..8)
            .map(|index| {
                block(
                    &format!("closed-{index}"),
                    EvalHybridBlockKind::ClosedToolPair,
                    index,
                )
            })
            .collect::<Vec<_>>();
        blocks.push(block("open", EvalHybridBlockKind::OpenToolState, 20));
        blocks.push(block("orphan", EvalHybridBlockKind::OrphanToolState, 21));
        blocks.push(block(
            "malformed",
            EvalHybridBlockKind::MalformedToolState,
            22,
        ));
        let partition = partition_eval_hybrid(&blocks);
        assert_eq!(partition.image_block_ids, vec!["closed-0", "closed-1"]);
        for id in ["closed-2", "closed-7", "open", "orphan", "malformed"] {
            assert!(partition.text_block_ids.iter().any(|value| value == id));
        }
    }

    #[test]
    fn secret_and_exact_material_never_enters_image_partition() {
        let mut secret = block("secret", EvalHybridBlockKind::LowRiskHistory, 0);
        secret.text = "api_key=sk-example".to_string();
        let exact = block("hash", EvalHybridBlockKind::ExactIdentifier, 1);
        let safe = block("history", EvalHybridBlockKind::LowRiskHistory, 2);
        let partition = partition_eval_hybrid(&[secret, exact, safe]);
        assert_eq!(partition.image_block_ids, vec!["history"]);
        assert_eq!(partition.text_block_ids, vec!["secret", "hash"]);
    }
}
