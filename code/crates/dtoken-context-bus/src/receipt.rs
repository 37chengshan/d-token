//! Receipts — RelayHit / CacheReuse 与 compression savings 严格分离（九边界之一，CONTEXT-3）。
//!
//! Relay/Cache 命中节省的是「避免重传」的字节（avoided bytes），**不是**
//! compression savings（token 压缩分账）。本模块的 receipt 只记录命中来源与
//! 避免的字节，永远不计入 `CompressionResult.tokens_saved`；UI/export 按四维
//! 独立呈现（compression / cache / Relay / RTK），不得直接求和（REQUIREMENTS
//! RUNTIME-5、CONTEXT-3）。

/// 节省维度：只可能是 relay 或 cache，绝不可能是 compression。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusSavingsKind {
    /// Context Relay 命中：跨 Agent 复用避免重传。
    RelayAvoidedRetransmission,
    /// Session Cache 命中：会话内复用避免重传。
    CacheReuse,
}

impl BusSavingsKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RelayAvoidedRetransmission => "relay_avoided_retransmission",
            Self::CacheReuse => "cache_reuse",
        }
    }
}

/// Relay 命中 receipt（跨 Agent）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayHitReceipt {
    pub project_id: String,
    pub owner_agent_id: String,
    pub grantee_agent_id: String,
    pub session_id: String,
    pub map_signature: String,
    pub source_fingerprint: String,
    pub avoided_bytes: usize,
    pub hit_at_ms: u64,
}

/// Session Cache 命中 receipt（会话内复用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheReuseReceipt {
    pub project_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub map_signature: String,
    pub source_fingerprint: String,
    pub reused_bytes: usize,
    pub hit_at_ms: u64,
}

/// 统一的 bus receipt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusReceipt {
    RelayHit(RelayHitReceipt),
    CacheReuse(CacheReuseReceipt),
}

impl BusReceipt {
    pub fn savings_kind(&self) -> BusSavingsKind {
        match self {
            Self::RelayHit(_) => BusSavingsKind::RelayAvoidedRetransmission,
            Self::CacheReuse(_) => BusSavingsKind::CacheReuse,
        }
    }

    /// 避免重传的字节（与 compression savings 完全无关）。
    pub fn avoided_bytes(&self) -> usize {
        match self {
            Self::RelayHit(receipt) => receipt.avoided_bytes,
            Self::CacheReuse(receipt) => receipt.reused_bytes,
        }
    }

    pub fn map_signature(&self) -> &str {
        match self {
            Self::RelayHit(receipt) => &receipt.map_signature,
            Self::CacheReuse(receipt) => &receipt.map_signature,
        }
    }

    pub fn project_id(&self) -> &str {
        match self {
            Self::RelayHit(receipt) => &receipt.project_id,
            Self::CacheReuse(receipt) => &receipt.project_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_hit_receipt_never_counts_as_compression() {
        let receipt = BusReceipt::RelayHit(RelayHitReceipt {
            project_id: "proj".to_string(),
            owner_agent_id: "agent-a".to_string(),
            grantee_agent_id: "agent-b".to_string(),
            session_id: "sess-1".to_string(),
            map_signature: "a".repeat(64),
            source_fingerprint: "b".repeat(64),
            avoided_bytes: 4096,
            hit_at_ms: 1_000,
        });
        assert_eq!(receipt.savings_kind(), BusSavingsKind::RelayAvoidedRetransmission);
        assert_eq!(receipt.savings_kind().as_str(), "relay_avoided_retransmission");
        assert_eq!(receipt.avoided_bytes(), 4096);
        // 明确断言：receipt 不携带任何 compression 字段。
        assert_ne!(receipt.savings_kind().as_str(), "compression");
    }

    #[test]
    fn cache_reuse_receipt_kind_is_cache() {
        let receipt = BusReceipt::CacheReuse(CacheReuseReceipt {
            project_id: "proj".to_string(),
            agent_id: "agent-a".to_string(),
            session_id: "sess-1".to_string(),
            map_signature: "a".repeat(64),
            source_fingerprint: "b".repeat(64),
            reused_bytes: 1024,
            hit_at_ms: 1_000,
        });
        assert_eq!(receipt.savings_kind(), BusSavingsKind::CacheReuse);
        assert_eq!(receipt.savings_kind().as_str(), "cache_reuse");
        assert_eq!(receipt.avoided_bytes(), 1024);
    }
}
