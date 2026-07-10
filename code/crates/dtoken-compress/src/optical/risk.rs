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

fn clean_token(token: &str) -> &str {
    token.trim_matches(|character: char| {
        matches!(
            character,
            ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\''
        )
    })
}

fn is_uuid_like(text: &str) -> bool {
    let token = clean_token(text);
    if token.len() != 36 {
        return false;
    }
    for (index, character) in token.chars().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if character != '-' {
                return false;
            }
        } else if !character.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn is_hash_like(text: &str) -> bool {
    let token = clean_token(text);
    (7..=128).contains(&token.len()) && token.chars().all(|character| character.is_ascii_hexdigit())
}

fn is_path_like(text: &str) -> bool {
    let token = clean_token(text);
    token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with("~/")
        || token.contains('\\')
        || (token.len() >= 3
            && token.as_bytes()[1] == b':'
            && matches!(token.as_bytes()[2], b'\\' | b'/'))
}

fn contains_secret_like(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let markers = [
        "authorization: bearer ",
        "authorization=bearer ",
        "bearer ",
        "cookie:",
        "set-cookie:",
        "x-api-key",
        "api_key",
        "api-key",
        "apikey",
        "sk-ant-",
    ];
    if markers.iter().any(|marker| lower.contains(marker)) {
        return true;
    }

    text.split_whitespace().any(|token| {
        let token = clean_token(token);
        token.starts_with("sk-") && token.len() >= 12
    })
}

fn contains_diff_patch(text: &str) -> bool {
    let mut has_old_header = false;
    let mut has_new_header = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("diff --git ") || trimmed.starts_with("@@ ") {
            return true;
        }
        has_old_header |= trimmed.starts_with("--- ");
        has_new_header |= trimmed.starts_with("+++ ");
    }
    has_old_header && has_new_header
}

/// 对单个 block 文本做保守风险分类。
/// Secret 和结构风险优先于 exact anchor；任何 secret-like 内容都会整块跳过。
pub fn classify_block(text: &str) -> RiskPartitionPlan {
    let total_chars = text.chars().count();
    if total_chars == 0 {
        return RiskPartitionPlan {
            tier: RiskTier::RenderSafe,
            anchors: None,
            skip_reason: Some("empty".to_string()),
        };
    }
    if contains_secret_like(text) {
        return RiskPartitionPlan {
            tier: RiskTier::SecretNoDuplicate,
            anchors: None,
            skip_reason: Some("secret_no_duplicate".to_string()),
        };
    }
    if contains_diff_patch(text) {
        return RiskPartitionPlan {
            tier: RiskTier::StructuralUnsafe,
            anchors: None,
            skip_reason: Some("diff_patch_structural".to_string()),
        };
    }

    let mut anchors = Vec::new();
    for (index, token) in text.split_whitespace().enumerate() {
        let cleaned = clean_token(token);
        if is_uuid_like(cleaned) || is_hash_like(cleaned) || is_path_like(cleaned) {
            anchors.push(ExactAnchor {
                anchor_id: format!("anchor-{index}"),
                risk_kind: RiskTier::ExactTextRequired,
                placeholder: format!("«§{index}»"),
                span_chars: cleaned.chars().count(),
            });
        }
    }
    let exact_chars: usize = anchors.iter().map(|anchor| anchor.span_chars).sum();
    let anchor_count = anchors.len();

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
    if anchor_count > 1 && exact_chars as f64 / total_chars as f64 > 0.15 {
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
            classify_block("(/usr/local/bin/node),").tier,
            RiskTier::ExactTextRequired
        );
    }

    #[test]
    fn embedded_secret_markers_skip_the_whole_block() {
        let text = "INFO request started\nAuthorization: Bearer credential-material\nINFO done";
        assert_eq!(classify_block(text).tier, RiskTier::SecretNoDuplicate);
        assert_eq!(
            classify_block("prefix Cookie: session=value suffix").tier,
            RiskTier::SecretNoDuplicate
        );
        assert_eq!(
            classify_block("config x-api-key=credential-material").tier,
            RiskTier::SecretNoDuplicate
        );
    }

    #[test]
    fn embedded_diff_becomes_structural_unsafe() {
        let text = "command output\n--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,3 @@";
        assert_eq!(classify_block(text).tier, RiskTier::StructuralUnsafe);
    }

    #[test]
    fn dense_exact_spans_skip_block() {
        let dense = "a1b2c3d4e5f6 a1b2c3d4e5f6 a1b2c3d4e5f6 a1b2c3d4e5f6";
        let plan = classify_block(dense);
        assert_eq!(plan.tier, RiskTier::BlockUnsafe);
        assert_eq!(plan.skip_reason.as_deref(), Some("dense_exact_spans"));
    }

    #[test]
    fn ordinary_log_remains_render_safe() {
        let text = "INFO operation completed\nWARN retry scheduled\nINFO finished";
        assert_eq!(classify_block(text).tier, RiskTier::RenderSafe);
    }
}
