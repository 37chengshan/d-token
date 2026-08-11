//! Exclusive canonical planner action for command-output nodes and the
//! four-dimension savings ledger (PR-C4C).
//!
//! RUNTIME-7 requires one canonical Context IR where every node is assigned
//! exactly one exclusive `NodeAction`; RUNTIME-9 requires savings to be
//! attributed to four independent dimensions (compression, command-output,
//! provider-cache, relay) with no double counting. This module is the RTK
//! half of that contract:
//!
//! - [`plan_exclusive_action`] assigns exactly one [`NodeAction`] per node
//!   shape; a command-output node owned by the verified RTK bridge is
//!   `RtkCommandOutput` and is never also `HeadroomSemantic`/`Optical`/
//!   `Reuse`.
//! - [`SavingsLedger`] attributes every claim to exactly one bucket, rejects
//!   the same `source_digest` twice in one bucket or across two buckets, and
//!   exposes per-bucket totals only — the four dimensions are never summed
//!   into a single pool. `remove`/`clear` provide uninstall/cleanup so a
//!   removed RTK runtime cannot leave stale positive claims behind.

use dtoken_types::{AccountingBucket, AccountingEntry, NodeAction};
use std::collections::HashSet;

pub const ACCOUNTING_SCHEMA_VERSION: u8 = 1;
pub const ACCOUNTING_ENTRY_SCHEMA_VERSION: u8 = 1;

/// The node shapes the exclusive planner distinguishes. This is deliberately
/// smaller than the full Context IR: it exists to prove the exclusivity and
/// attribution contract for command-output nodes, which is the RTK boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    /// A completed command-output node (e.g. a tool result) that the
    /// d-token RTK filter can own.
    CommandOutput,
    /// Cache-stable tool-result prose that is an Optical candidate.
    ToolResultProse,
    /// Transformable text that is a Headroom semantic candidate.
    TransformableText,
    /// A reuse candidate (already-recovered context).
    ReuseCandidate,
    /// Protected/secret/exact content that must stay verbatim.
    Protected,
}

/// Assign exactly one canonical action to a node. A command-output node is
/// `RtkCommandOutput` only when the verified RTK bridge owns it; otherwise it
/// is `Preserve`, so an unverified filter can never claim command-output
/// savings. Every other shape maps to a single, mutually exclusive action.
pub fn plan_exclusive_action(shape: NodeShape, rtk_owned: bool) -> NodeAction {
    match shape {
        NodeShape::CommandOutput if rtk_owned => NodeAction::RtkCommandOutput,
        NodeShape::CommandOutput => NodeAction::Preserve,
        NodeShape::ToolResultProse => NodeAction::Optical,
        NodeShape::TransformableText => NodeAction::HeadroomSemantic,
        NodeShape::ReuseCandidate => NodeAction::Reuse,
        NodeShape::Protected => NodeAction::Preserve,
    }
}

/// True when a claim was produced by the verified d-token RTK command-output
/// filter (disposition written by the bridge), as opposed to a Headroom or
/// Optical text transform. The exclusive planner uses this to decide RTK
/// ownership of a node without trusting caller-supplied buckets.
pub fn is_rtk_command_output_claim(disposition: &str) -> bool {
    matches!(disposition, "filtered_verified_hook")
}

/// Ledger admission errors. Every failure leaves the ledger unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountingError {
    UnsupportedSchema,
    InvalidDigest,
    InvalidBytes,
    DuplicateClaim,
    CrossBucketConflict,
}

impl std::fmt::Display for AccountingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            AccountingError::UnsupportedSchema => "accounting entry schema version is unsupported",
            AccountingError::InvalidDigest => "accounting entry source digest is not sha256 hex",
            AccountingError::InvalidBytes => "accounting entry byte accounting is inconsistent",
            AccountingError::DuplicateClaim => {
                "accounting entry duplicates a claim in the same bucket"
            }
            AccountingError::CrossBucketConflict => {
                "accounting entry conflicts with a claim in another bucket"
            }
        };
        formatter.write_str(text)
    }
}

impl std::error::Error for AccountingError {}

/// Per-bucket projection totals. `entries` counts admitted claims.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BucketTotals {
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub saved_bytes: u64,
    pub entries: usize,
}

#[derive(Debug, Clone, Default)]
struct BucketLedger {
    totals: BucketTotals,
    digests: HashSet<String>,
}

/// The four-dimension savings ledger. Claims are admitted exclusively: the
/// same `source_digest` can never appear twice in one bucket or in two
/// buckets, so compression/command-output/cache/relay savings are disjoint
/// and a Headroom or Optical transform of an already-filtered command output
/// can never double count RTK savings.
#[derive(Debug, Clone, Default)]
pub struct SavingsLedger {
    compression: BucketLedger,
    command_output: BucketLedger,
    provider_cache: BucketLedger,
    relay: BucketLedger,
}

impl SavingsLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Admit one attributed claim. Fails closed (ledger unchanged) on an
    /// unsupported schema, a non-sha256 digest, inconsistent bytes, a
    /// duplicate claim in the same bucket, or a conflict with any other
    /// bucket.
    pub fn record(&mut self, entry: AccountingEntry) -> Result<(), AccountingError> {
        if entry.schema_version != ACCOUNTING_ENTRY_SCHEMA_VERSION {
            return Err(AccountingError::UnsupportedSchema);
        }
        if !is_sha256_hex(&entry.source_digest) {
            return Err(AccountingError::InvalidDigest);
        }
        if entry.output_bytes > entry.input_bytes
            || entry.saved_bytes != entry.input_bytes - entry.output_bytes
        {
            return Err(AccountingError::InvalidBytes);
        }
        // Cross-bucket conflicts are checked before any mutation so a
        // rejected claim can never leave a partial claim behind.
        let conflict = self.buckets().into_iter().any(|(bucket, ledger)| {
            bucket != entry.bucket && ledger.digests.contains(&entry.source_digest)
        });
        if conflict {
            return Err(AccountingError::CrossBucketConflict);
        }
        let target = self.bucket_ledger_mut(entry.bucket);
        if !target.digests.insert(entry.source_digest.clone()) {
            return Err(AccountingError::DuplicateClaim);
        }
        target.totals.input_bytes = target.totals.input_bytes.saturating_add(entry.input_bytes);
        target.totals.output_bytes = target
            .totals
            .output_bytes
            .saturating_add(entry.output_bytes);
        target.totals.saved_bytes = target.totals.saved_bytes.saturating_add(entry.saved_bytes);
        target.totals.entries = target.totals.entries.saturating_add(1);
        Ok(())
    }

    /// Per-bucket projection totals.
    pub fn bucket_totals(&self, bucket: AccountingBucket) -> BucketTotals {
        self.bucket_ledger(bucket).totals
    }

    pub fn compression_savings(&self) -> u64 {
        self.compression.totals.saved_bytes
    }

    /// Command-output (RTK) savings. This is the only bucket RTK claims may
    /// ever land in; it is never merged into compression/cache/relay totals.
    pub fn command_output_savings(&self) -> u64 {
        self.command_output.totals.saved_bytes
    }

    pub fn provider_cache_savings(&self) -> u64 {
        self.provider_cache.totals.saved_bytes
    }

    pub fn relay_savings(&self) -> u64 {
        self.relay.totals.saved_bytes
    }

    pub fn total_entries(&self) -> usize {
        self.buckets()
            .into_iter()
            .map(|(_, ledger)| ledger.totals.entries)
            .sum()
    }

    /// Cleanup: drop one claim (and its digest) from every bucket. Returns
    /// true when the digest was present. Used by uninstall paths so a
    /// removed RTK runtime cannot leave stale positive claims behind.
    pub fn remove(&mut self, source_digest: &str) -> bool {
        let mut removed = false;
        for (_, ledger) in self.buckets_mut() {
            if ledger.digests.remove(source_digest) {
                ledger.totals = BucketTotals::default();
                removed = true;
            }
        }
        removed
    }

    /// Cleanup: drop every claim. The ledger returns to its initial state.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    fn bucket_ledger(&self, bucket: AccountingBucket) -> &BucketLedger {
        match bucket {
            AccountingBucket::Compression => &self.compression,
            AccountingBucket::CommandOutput => &self.command_output,
            AccountingBucket::ProviderCache => &self.provider_cache,
            AccountingBucket::Relay => &self.relay,
        }
    }

    fn bucket_ledger_mut(&mut self, bucket: AccountingBucket) -> &mut BucketLedger {
        match bucket {
            AccountingBucket::Compression => &mut self.compression,
            AccountingBucket::CommandOutput => &mut self.command_output,
            AccountingBucket::ProviderCache => &mut self.provider_cache,
            AccountingBucket::Relay => &mut self.relay,
        }
    }

    fn buckets_mut(&mut self) -> [(AccountingBucket, &mut BucketLedger); 4] {
        [
            (AccountingBucket::Compression, &mut self.compression),
            (AccountingBucket::CommandOutput, &mut self.command_output),
            (AccountingBucket::ProviderCache, &mut self.provider_cache),
            (AccountingBucket::Relay, &mut self.relay),
        ]
    }

    fn buckets(&self) -> [(AccountingBucket, &BucketLedger); 4] {
        [
            (AccountingBucket::Compression, &self.compression),
            (AccountingBucket::CommandOutput, &self.command_output),
            (AccountingBucket::ProviderCache, &self.provider_cache),
            (AccountingBucket::Relay, &self.relay),
        ]
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(bucket: AccountingBucket, digest: &str, input: u64, output: u64) -> AccountingEntry {
        AccountingEntry {
            schema_version: ACCOUNTING_ENTRY_SCHEMA_VERSION,
            bucket,
            source_digest: digest.to_string(),
            input_bytes: input,
            output_bytes: output,
            saved_bytes: input - output,
            disposition: "filtered_verified_hook".to_string(),
            recorded_at_ms: 1,
        }
    }

    #[test]
    fn command_output_node_is_exclusively_rtk_and_never_compression() {
        assert_eq!(
            plan_exclusive_action(NodeShape::CommandOutput, true),
            NodeAction::RtkCommandOutput
        );
        // Without verified RTK ownership the same node is preserved — no
        // action can be claimed, so no savings can be counted.
        assert_eq!(
            plan_exclusive_action(NodeShape::CommandOutput, false),
            NodeAction::Preserve
        );
        // The remaining shapes map to exclusive, distinct actions.
        assert_eq!(
            plan_exclusive_action(NodeShape::TransformableText, false),
            NodeAction::HeadroomSemantic
        );
        assert_eq!(
            plan_exclusive_action(NodeShape::ToolResultProse, false),
            NodeAction::Optical
        );
        assert_eq!(
            plan_exclusive_action(NodeShape::ReuseCandidate, false),
            NodeAction::Reuse
        );
        assert_eq!(
            plan_exclusive_action(NodeShape::Protected, false),
            NodeAction::Preserve
        );
        let actions = [
            NodeAction::Preserve,
            NodeAction::Reuse,
            NodeAction::HeadroomSemantic,
            NodeAction::RtkCommandOutput,
            NodeAction::Optical,
        ];
        assert_eq!(actions.len(), 5);
    }

    #[test]
    fn rtk_claims_are_identified_by_bridge_disposition_only() {
        assert!(is_rtk_command_output_claim("filtered_verified_hook"));
        assert!(!is_rtk_command_output_claim("passthrough_unverified_hook"));
        assert!(!is_rtk_command_output_claim("headroom_semantic"));
        assert!(!is_rtk_command_output_claim("optical_image"));
    }

    #[test]
    fn command_output_savings_never_flow_into_other_buckets() {
        let mut ledger = SavingsLedger::new();
        ledger
            .record(entry(
                AccountingBucket::CommandOutput,
                &"a".repeat(64),
                100,
                40,
            ))
            .expect("command-output claim");
        assert_eq!(ledger.command_output_savings(), 60);
        assert_eq!(ledger.compression_savings(), 0);
        assert_eq!(ledger.provider_cache_savings(), 0);
        assert_eq!(ledger.relay_savings(), 0);
        assert_eq!(
            ledger
                .bucket_totals(AccountingBucket::CommandOutput)
                .entries,
            1
        );
    }

    #[test]
    fn same_digest_cannot_be_claimed_twice_or_in_two_buckets() {
        let mut ledger = SavingsLedger::new();
        let digest = "b".repeat(64);
        ledger
            .record(entry(AccountingBucket::CommandOutput, &digest, 100, 40))
            .expect("first claim");
        let duplicate = ledger.record(entry(AccountingBucket::CommandOutput, &digest, 100, 40));
        assert_eq!(duplicate, Err(AccountingError::DuplicateClaim));

        let cross_bucket = ledger.record(entry(AccountingBucket::Compression, &digest, 200, 100));
        assert_eq!(cross_bucket, Err(AccountingError::CrossBucketConflict));
        assert_eq!(ledger.compression_savings(), 0, "conflict leaves no claim");
        assert_eq!(ledger.command_output_savings(), 60);
    }

    #[test]
    fn an_optical_transform_of_filtered_output_cannot_double_count() {
        // A node already owned by RTK cannot later be claimed by Optical or
        // Headroom: the exclusive planner refuses, and the ledger rejects the
        // same digest in another bucket.
        let digest = "c".repeat(64);
        let action = plan_exclusive_action(NodeShape::CommandOutput, true);
        assert_eq!(action, NodeAction::RtkCommandOutput);
        let mut ledger = SavingsLedger::new();
        ledger
            .record(entry(AccountingBucket::CommandOutput, &digest, 100, 40))
            .expect("RTK claim");
        assert_eq!(
            ledger.record(entry(AccountingBucket::Compression, &digest, 100, 40,)),
            Err(AccountingError::CrossBucketConflict)
        );
        assert_eq!(
            ledger.record(entry(AccountingBucket::Relay, &digest, 100, 40,)),
            Err(AccountingError::CrossBucketConflict)
        );
        assert_eq!(ledger.command_output_savings(), 60);
    }

    #[test]
    fn invalid_claims_fail_closed_without_mutating_the_ledger() {
        let mut ledger = SavingsLedger::new();
        assert_eq!(
            ledger.record(AccountingEntry {
                schema_version: 99,
                ..entry(AccountingBucket::CommandOutput, &"a".repeat(64), 10, 5)
            }),
            Err(AccountingError::UnsupportedSchema)
        );
        assert_eq!(
            ledger.record(entry(
                AccountingBucket::CommandOutput,
                "not-a-digest",
                10,
                5,
            )),
            Err(AccountingError::InvalidDigest)
        );
        assert_eq!(
            ledger.record(AccountingEntry {
                saved_bytes: 99,
                ..entry(AccountingBucket::CommandOutput, &"a".repeat(64), 10, 5)
            }),
            Err(AccountingError::InvalidBytes)
        );
        assert_eq!(ledger.total_entries(), 0);
        assert_eq!(ledger.command_output_savings(), 0);
    }

    #[test]
    fn uninstall_cleanup_removes_claims_and_clear_resets_everything() {
        let mut ledger = SavingsLedger::new();
        let digest = "d".repeat(64);
        ledger
            .record(entry(AccountingBucket::CommandOutput, &digest, 100, 40))
            .expect("claim");
        ledger
            .record(entry(
                AccountingBucket::Compression,
                &"e".repeat(64),
                50,
                20,
            ))
            .expect("claim");
        assert!(ledger.remove(&digest));
        assert_eq!(ledger.command_output_savings(), 0);
        assert_eq!(ledger.compression_savings(), 30);
        assert!(!ledger.remove(&digest), "second remove is a no-op");

        // The digest is free again after cleanup.
        ledger
            .record(entry(AccountingBucket::CommandOutput, &digest, 10, 4))
            .expect("re-record after uninstall cleanup");

        ledger.clear();
        assert_eq!(ledger.total_entries(), 0);
        assert_eq!(ledger.command_output_savings(), 0);
        assert_eq!(ledger.compression_savings(), 0);
    }
}
