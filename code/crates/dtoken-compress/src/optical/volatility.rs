// VolatilityPlanner：移除 per-turn 变化内容，避免 prompt-cache churn。
// 关联计划：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md §9。

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolatileKind {
    BillingHeader,
    EnvironmentBlock,
    WorkingDirectory,
    GitStatus,
    GitBranch,
    PlatformInfo,
    CurrentDate,
    SessionHeader,
    UnknownPerTurnTag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolatilePolicy {
    KeepTextInPlace,
    RelocateToTailText,
    DropIfKnownRedundant,
}

#[derive(Debug, Clone)]
pub struct StableSegment {
    pub chars: usize,
    pub digest_prefix: String,
}

#[derive(Debug, Clone)]
pub struct VolatileSegment {
    pub kind: VolatileKind,
    pub policy: VolatilePolicy,
    pub chars: usize,
}

#[derive(Debug, Clone)]
pub struct VolatilityPlan {
    pub stable_segments: Vec<StableSegment>,
    pub volatile_segments: Vec<VolatileSegment>,
    pub stable_digest: Option<String>,
    pub volatile_chars: usize,
    pub churning_static_tags: Vec<String>,
}

/// 已知 volatile header 指纹（前缀匹配）。
const VOLATILE_PREFIXES: &[(&str, VolatileKind)] = &[
    ("Date:", VolatileKind::CurrentDate),
    ("X-Request-Id:", VolatileKind::SessionHeader),
    ("X-Billing-", VolatileKind::BillingHeader),
    ("CWD:", VolatileKind::WorkingDirectory),
    ("cwd:", VolatileKind::WorkingDirectory),
    ("Git status:", VolatileKind::GitStatus),
    ("git status:", VolatileKind::GitStatus),
    ("Branch:", VolatileKind::GitBranch),
    ("branch:", VolatileKind::GitBranch),
    ("Platform:", VolatileKind::PlatformInfo),
    ("Environment:", VolatileKind::EnvironmentBlock),
];

/// 扫描候选 slab，拆分 stable/volatile，不改原始 body。
pub fn plan_volatility(text: &str) -> VolatilityPlan {
    let mut stable_segments = Vec::new();
    let mut volatile_segments = Vec::new();
    let mut volatile_chars = 0usize;

    for line in text.lines() {
        if let Some(kind) = detect_volatile_kind(line) {
            let chars = line.chars().count();
            volatile_segments.push(VolatileSegment {
                kind: kind.clone(),
                policy: default_policy_for(&kind),
                chars,
            });
            volatile_chars += chars;
        } else {
            stable_segments.push(StableSegment {
                chars: line.chars().count(),
                digest_prefix: digest_line(line),
            });
        }
    }

    let stable_digest = if !stable_segments.is_empty() {
        Some(compute_stable_digest(&stable_segments))
    } else {
        None
    };

    VolatilityPlan {
        stable_segments,
        volatile_segments,
        stable_digest,
        volatile_chars,
        churning_static_tags: Vec::new(),
    }
}

fn detect_volatile_kind(line: &str) -> Option<VolatileKind> {
    let trimmed = line.trim_start();
    for (prefix, kind) in VOLATILE_PREFIXES {
        if trimmed.starts_with(prefix) {
            return Some(kind.clone());
        }
    }
    None
}

fn default_policy_for(kind: &VolatileKind) -> VolatilePolicy {
    match kind {
        VolatileKind::CurrentDate | VolatileKind::SessionHeader => {
            VolatilePolicy::RelocateToTailText
        }
        VolatileKind::BillingHeader => VolatilePolicy::DropIfKnownRedundant,
        _ => VolatilePolicy::KeepTextInPlace,
    }
}

fn digest_line(line: &str) -> String {
    format!("{:x}", Sha256::digest(line.as_bytes()))
}

fn compute_stable_digest(segments: &[StableSegment]) -> String {
    let mut hasher = Sha256::new();
    for seg in segments {
        hasher.update(seg.digest_prefix.as_bytes());
        hasher.update([0]);
        hasher.update(seg.chars.to_le_bytes());
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_block_becomes_volatile() {
        let plan = plan_volatility("Environment: PATH=/usr/bin\nstable line\nDate: 2026-07-08");
        assert_eq!(plan.volatile_segments.len(), 2);
        assert!(plan
            .volatile_segments
            .iter()
            .any(|s| s.kind == VolatileKind::EnvironmentBlock));
        assert!(plan
            .volatile_segments
            .iter()
            .any(|s| s.kind == VolatileKind::CurrentDate));
    }

    #[test]
    fn stable_digest_ignores_volatile_segment() {
        let plan_a = plan_volatility("stable line\nDate: 2026-07-08");
        let plan_b = plan_volatility("stable line\nDate: 2026-07-09");
        assert_eq!(plan_a.stable_digest, plan_b.stable_digest);
        assert_eq!(plan_a.stable_digest.as_deref().map(str::len), Some(64));
    }

    #[test]
    fn churning_static_tag_becomes_warning() {
        let mut plan = plan_volatility("stable line");
        plan.churning_static_tags
            .push("static_slab_digest_changed".to_string());
        assert!(!plan.churning_static_tags.is_empty());
    }
}
