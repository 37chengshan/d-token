// RiskPartitionPlanner：把文本 span 分类到 risk tier，构建 AnchorMap。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §15。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskTier {
    RenderSafe,
    ExactTextRequired,
    SecretNoDuplicate,
    StructuralUnsafe,
    BlockUnsafe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarPolicy {
    None,
    TextAdjacent,
    PreserveWholeBlockText,
    SkipBlock,
}

#[derive(Debug, Clone)]
pub struct ExactAnchor {
    pub anchor_id: String,
    pub risk_kind: RiskTier,
    pub placeholder: String,
    pub span_chars: usize,
}

#[derive(Debug, Clone)]
pub struct AnchorMap {
    pub anchors: Vec<ExactAnchor>,
    pub sidecar_policy: SidecarPolicy,
    pub anchor_count: usize,
    pub exact_chars: usize,
    pub total_chars: usize,
}

#[derive(Debug, Clone)]
pub struct RiskPartitionPlan {
    pub tier: RiskTier,
    pub anchors: Option<AnchorMap>,
    pub skip_reason: Option<String>,
}

fn is_uuid_like(text: &str) -> bool {
    let t = text.trim();
    t.len() == 36 && t.chars().filter(|c| *c == '-').count() == 4
}
fn is_hash_like(text: &str) -> bool {
    let t = text.trim();
    t.len() >= 7 && t.chars().all(|c| c.is_ascii_hexdigit())
}
fn is_path_like(text: &str) -> bool {
    let t = text.trim();
    t.contains('/') || t.contains('\\') || t.starts_with("./") || t.starts_with("../")
}
fn is_bearer_token(text: &str) -> bool {
    text.trim().to_ascii_lowercase().starts_with("bearer ")
}
fn is_api_key_like(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("sk-") || t.starts_with("sk-ant-") || t.contains("api_key")
}
fn is_cookie(text: &str) -> bool {
    text.trim().to_ascii_lowercase().starts_with("cookie:")
}
fn is_diff_patch(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("---") || t.starts_with("+++") || t.starts_with("@@") || t.starts_with("diff ")
}

/// 对单个 block 文本做 risk 分类。
pub fn classify_block(text: &str) -> RiskPartitionPlan {
    let total_chars = text.chars().count();
    if total_chars == 0 {
        return RiskPartitionPlan {
            tier: RiskTier::RenderSafe,
            anchors: None,
            skip_reason: Some("empty".to_string()),
        };
    }
    if is_bearer_token(text) || is_api_key_like(text) || is_cookie(text) {
        return RiskPartitionPlan {
            tier: RiskTier::SecretNoDuplicate,
            anchors: None,
            skip_reason: Some("secret_no_duplicate".to_string()),
        };
    }
    if is_diff_patch(text) {
        return RiskPartitionPlan {
            tier: RiskTier::StructuralUnsafe,
            anchors: None,
            skip_reason: Some("diff_patch_structural".to_string()),
        };
    }
    let mut anchors = Vec::new();
    for (idx, word) in text.split_whitespace().enumerate() {
        if is_uuid_like(word) || is_hash_like(word) || is_path_like(word) {
            anchors.push(ExactAnchor {
                anchor_id: format!("anchor-{}", idx),
                risk_kind: RiskTier::ExactTextRequired,
                placeholder: format!("«§{}»", idx),
                span_chars: word.chars().count(),
            });
        }
    }
    let exact_chars: usize = anchors.iter().map(|a| a.span_chars).sum();
    let anchor_count = anchors.len();
    // dense exact spans only when multiple anchors dominate the text
    if anchor_count > 1 && total_chars > 0 && exact_chars as f64 / total_chars as f64 > 0.15 {
        return RiskPartitionPlan {
            tier: RiskTier::BlockUnsafe,
            anchors: Some(AnchorMap {
                anchors,
                sidecar_policy: SidecarPolicy::SkipBlock,
                anchor_count,
                exact_chars,
                total_chars,
            }),
            skip_reason: Some("dense_exact_spans".to_string()),
        };
    }
    if anchor_count > 100 {
        return RiskPartitionPlan {
            tier: RiskTier::BlockUnsafe,
            anchors: Some(AnchorMap {
                anchors,
                sidecar_policy: SidecarPolicy::SkipBlock,
                anchor_count,
                exact_chars,
                total_chars,
            }),
            skip_reason: Some("anchor_count_over_budget".to_string()),
        };
    }
    if !anchors.is_empty() {
        return RiskPartitionPlan {
            tier: RiskTier::ExactTextRequired,
            anchors: Some(AnchorMap {
                anchors,
                sidecar_policy: SidecarPolicy::TextAdjacent,
                anchor_count,
                exact_chars,
                total_chars,
            }),
            skip_reason: None,
        };
    }
    RiskPartitionPlan {
        tier: RiskTier::RenderSafe,
        anchors: None,
        skip_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_hash_path_become_exact_text_required() {
        assert_eq!(
            classify_block("550e8400-e29b-41d4-a716-446655440000").tier,
            RiskTier::ExactTextRequired
        );
        assert_eq!(
            classify_block("a1b2c3d4e5f6").tier,
            RiskTier::ExactTextRequired
        );
        assert_eq!(
            classify_block("/usr/local/bin/node").tier,
            RiskTier::ExactTextRequired
        );
    }

    #[test]
    fn bearer_cookie_api_key_become_secret_no_duplicate() {
        assert_eq!(
            classify_block("Bearer abc123token").tier,
            RiskTier::SecretNoDuplicate
        );
        assert_eq!(
            classify_block("sk-ant-test-key").tier,
            RiskTier::SecretNoDuplicate
        );
        assert_eq!(
            classify_block("Cookie: session=abc").tier,
            RiskTier::SecretNoDuplicate
        );
    }

    #[test]
    fn diff_becomes_structural_unsafe() {
        assert_eq!(
            classify_block("--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,3 @@").tier,
            RiskTier::StructuralUnsafe
        );
    }

    #[test]
    fn dense_exact_spans_skip_block() {
        let dense = "a1b2c3d4e5f6 a1b2c3d4e5f6 a1b2c3d4e5f6 a1b2c3d4e5f6";
        let plan = classify_block(dense);
        assert_eq!(plan.tier, RiskTier::BlockUnsafe);
        assert_eq!(plan.skip_reason.as_deref(), Some("dense_exact_spans"));
    }
}
