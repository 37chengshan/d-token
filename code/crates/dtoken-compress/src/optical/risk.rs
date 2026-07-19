// RiskPartitionPlanner：把文本 span 分类到 risk tier，构建 AnchorMap。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §15。

use super::secret::contains_secret_like;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskTier {
    RenderSafe,
    ExactTextRequired,
    DecisionSensitive,
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
        || (token.contains('/') && token.contains('.'))
        || (token.len() >= 3
            && token.as_bytes()[1] == b':'
            && matches!(token.as_bytes()[2], b'\\' | b'/'))
}

fn is_url_like(text: &str) -> bool {
    let token = clean_token(text);
    token.starts_with("http://") || token.starts_with("https://")
}

fn is_numeric_like(text: &str) -> bool {
    let token = clean_token(text).trim_matches(|character: char| {
        matches!(character, '+' | '-' | '_' | ',' | '.' | '%' | 'm' | 's')
    });
    !token.is_empty()
        && token.chars().any(|character| character.is_ascii_digit())
        && token.chars().all(|character| character.is_ascii_digit())
}

fn is_version_like(text: &str) -> bool {
    let token = clean_token(text)
        .strip_prefix('v')
        .unwrap_or_else(|| clean_token(text));
    let pieces = token.split('.').collect::<Vec<_>>();
    pieces.len() >= 2
        && pieces
            .iter()
            .all(|piece| !piece.is_empty() && piece.chars().all(|ch| ch.is_ascii_digit()))
}

fn is_cli_flag_like(text: &str) -> bool {
    let token = clean_token(text);
    token.starts_with('-')
        && token.len() >= 2
        && token
            .trim_start_matches('-')
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn is_constant_like(text: &str) -> bool {
    let token = clean_token(text);
    token.len() >= 3
        && token.contains('_')
        && token.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

fn is_assignment_like(text: &str) -> bool {
    let token = clean_token(text);
    let Some((label, value)) = token.split_once('=') else {
        return false;
    };
    label.len() >= 2
        && !value.is_empty()
        && label.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

fn is_camel_case_like(text: &str) -> bool {
    let token = clean_token(text);
    token.len() >= 6
        && token
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
        && token
            .chars()
            .any(|character| character.is_ascii_lowercase())
        && token
            .chars()
            .skip(1)
            .any(|character| character.is_ascii_uppercase())
}

fn is_ticket_like(text: &str) -> bool {
    let token = clean_token(text);
    token.contains('-')
        && token.chars().any(|character| character.is_ascii_digit())
        && token.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
        })
}

fn is_exact_token(text: &str) -> bool {
    is_uuid_like(text)
        || is_hash_like(text)
        || is_path_like(text)
        || is_url_like(text)
        || is_numeric_like(text)
        || is_version_like(text)
        || is_cli_flag_like(text)
        || is_constant_like(text)
        || is_assignment_like(text)
        || is_camel_case_like(text)
        || is_ticket_like(text)
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
    if !text.is_ascii() {
        return RiskPartitionPlan {
            tier: RiskTier::StructuralUnsafe,
            anchors: None,
            skip_reason: Some("unsupported_render_script".to_string()),
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
        let is_html_tag_token = token.contains('<') && token.contains('>');
        if !is_html_tag_token && is_exact_token(cleaned) {
            anchors.push(ExactAnchor {
                anchor_id: format!("anchor-{index}"),
                risk_kind: RiskTier::ExactTextRequired,
                placeholder: format!("«§{index}»"),
                span_chars: cleaned.chars().count(),
            });
        }
        // HTML tag syntax contains slash-prefixed closing tags (for example
        // `</li>`). Those are structure, not filesystem paths. The HTML
        // router preserves tag attributes while folding only identical safe
        // lines, so treating tag fragments as exact anchors creates false
        // positives without adding protection.
        if is_html_tag_token {
            continue;
        }
        for (fragment_index, fragment) in token
            .split(|character: char| {
                character.is_whitespace()
                    || matches!(
                        character,
                        ',' | ';'
                            | ':'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '<'
                            | '>'
                            | '"'
                            | '\''
                            | '='
                    )
            })
            .filter(|fragment| !fragment.is_empty())
            .enumerate()
        {
            if fragment != cleaned && is_exact_token(fragment) {
                anchors.push(ExactAnchor {
                    anchor_id: format!("anchor-{index}-{fragment_index}"),
                    risk_kind: RiskTier::ExactTextRequired,
                    placeholder: format!("«§{index}.{fragment_index}»"),
                    span_chars: fragment.chars().count(),
                });
            }
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

    if crate::content_router::contains_decision_signal(text) {
        return RiskPartitionPlan {
            tier: RiskTier::DecisionSensitive,
            anchors: None,
            skip_reason: Some("decision_sensitive".to_string()),
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
            classify_block("INFO token=credentialmaterial").tier,
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

    #[test]
    fn precision_critical_shapes_require_text() {
        for value in [
            "https://example.test/a/b",
            "src/lib/router.rs",
            "v5.6.1",
            "--max-parallel",
            "CACHE_V3",
            "ACTIVE_MODEL=gpt-5.6-sol",
            "tokenLedgerShard",
            "PROJ-1482",
            "port 47821",
            "{\"id\":\"a1b2c3d4e5f6\",\"dur_ms\":731}",
        ] {
            assert_ne!(classify_block(value).tier, RiskTier::RenderSafe, "{value}");
        }
    }

    #[test]
    fn final_decision_without_exact_tokens_is_decision_sensitive() {
        let plan = classify_block("INFO final decision use floor division");
        assert_eq!(plan.tier, RiskTier::DecisionSensitive);
        assert_eq!(plan.skip_reason.as_deref(), Some("decision_sensitive"));
    }
}
