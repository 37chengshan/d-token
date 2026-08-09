//! 签名模型 — map-signature 复用与 source fingerprint。
//!
//! map-signature：对规范化后的 map 内容（原始 body 的 canonical JSON）做 SHA-256，
//! 等价内容得到相同签名 → 可复用；文件/source 变化 → 签名变化 → miss。
//! source fingerprint：source_revision（64 hex）+ 可选 file digest，任何变化即 miss。
//! 本模块同时承载 secret-shape gate（搬移自 store `is_secret_shaped_failure_metadata`
//! 并扩展 token 形态），保证 bus 不持久化 raw/secret。

use sha2::{Digest, Sha256};

/// 规范化的 map 内容 → map signature（64 hex）。
pub fn map_signature(project_id: &str, canonical_content: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"dtoken-context-bus/map/v1");
    digest.update([0]);
    digest.update(project_id.as_bytes());
    digest.update([0]);
    digest.update(canonical_content.as_bytes());
    format!("{:x}", digest.finalize())
}

/// source fingerprint：source_revision + 可选 file digest 的签名。
pub fn source_fingerprint(source_revision: &str, file_digest: Option<&str>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"dtoken-context-bus/source/v1");
    digest.update([0]);
    digest.update(source_revision.as_bytes());
    digest.update([0]);
    digest.update(file_digest.unwrap_or_default().as_bytes());
    format!("{:x}", digest.finalize())
}

/// 64 hex 校验（source_revision 形态），搬移自 compress `validate_provider_cache_evidence`。
pub fn is_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// query tokens 规范化（与 store `normalized_keywords` 同规则：小写、排序、去重）。
pub fn normalize_query_tokens(tokens: &[String]) -> Vec<String> {
    let mut normalized = Vec::with_capacity(tokens.len());
    for token in tokens {
        let token = token.trim().to_lowercase();
        if !token.is_empty() && token.chars().count() <= 64 && !token.chars().any(char::is_control)
        {
            normalized.push(token);
        }
    }
    normalized.sort();
    normalized.dedup();
    normalized
}

/// secret-shape gate：任何形如 secret 的内容不得进入 bus 存储。
/// 搬移自 `dtoken-context-store::is_secret_shaped_failure_metadata` 并扩展
/// JWT/credential 形态，覆盖 SECURITY-1 的共享分类器核心形态。
pub fn is_secret_shaped(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("xox")
        || [
            "authorization",
            "bearer",
            "api_key",
            "apikey",
            "cookie",
            "password",
            "secret",
            "token=",
            "private key",
            "BEGIN RSA PRIVATE KEY",
            "BEGIN PRIVATE KEY",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return true;
    }
    // JWT 形态（header.payload.signature）。
    if value.split('.').count() == 3
        && value
            .split('.')
            .all(|part| part.len() >= 4 && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_signature_is_deterministic_and_content_sensitive() {
        let signature = map_signature("proj", r#"{"a":1}"#);
        assert_eq!(signature.len(), 64);
        assert_eq!(signature, map_signature("proj", r#"{"a":1}"#));
        // 内容变化 → 签名变化；项目不同 → 签名不同（防跨项目串用）。
        assert_ne!(signature, map_signature("proj", r#"{"a":2}"#));
        assert_ne!(signature, map_signature("other", r#"{"a":1}"#));
    }

    #[test]
    fn source_fingerprint_changes_with_revision_or_file_digest() {
        let fingerprint = source_fingerprint("a".repeat(64).as_str(), None);
        assert_eq!(fingerprint, source_fingerprint("a".repeat(64).as_str(), None));
        assert_ne!(fingerprint, source_fingerprint("b".repeat(64).as_str(), None));
        assert_ne!(
            fingerprint,
            source_fingerprint("a".repeat(64).as_str(), Some("file-digest"))
        );
    }

    #[test]
    fn hex64_validation() {
        assert!(is_hex64(&"a".repeat(64)));
        assert!(!is_hex64(&"a".repeat(63)));
        assert!(!is_hex64(&"g".repeat(64)));
    }

    #[test]
    fn secret_shape_gate_covers_store_markers_and_jwt() {
        for secret in [
            "sk-abc123",
            "ghp_xYZ",
            "Authorization: Bearer token",
            "api_key=secret",
            "password=hunter2",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
        ] {
            assert!(is_secret_shaped(secret), "expected secret-shape: {secret}");
        }
        // 普通内容不是 secret。
        for plain in [
            "The quick brown fox",
            "fn main() { println!(\"hi\"); }",
            "project convention: prefer explicit routes",
        ] {
            assert!(!is_secret_shaped(plain), "expected plain: {plain}");
        }
    }

    #[test]
    fn query_tokens_are_normalized_deduped_and_sorted() {
        let tokens = normalize_query_tokens(&[
            "  Route ".to_string(),
            "route".to_string(),
            "Error".to_string(),
            "".to_string(),
        ]);
        assert_eq!(tokens, vec!["error".to_string(), "route".to_string()]);
    }
}
