//! Capability-driven compression strategy mapping (R2.E).
//!
//! The capability registry picks a `CompressionStrategy` per model identity;
//! this module maps it onto the concrete `CompressionPolicy` the compressor
//! executes. Unknown capability resolves to the conservative default (Safe);
//! a capability known to be small-context or tool-call-only stays
//! conservative. Never fails: every strategy has a mapping.

use dtoken_types::CompressionPolicy;

/// Maps a capability-resolved strategy hint to a concrete policy.
/// Unknown/absent hints fall back to `Safe` (conservative, never off).
pub fn compression_strategy_to_policy(hint: Option<&str>) -> CompressionPolicy {
    match hint {
        Some("semantic_v2") | Some("sliding_window") => CompressionPolicy::Balanced,
        Some("frozen_floor") | Some("tool_schema") => CompressionPolicy::Safe,
        Some("default") | Some("passthrough") => CompressionPolicy::Safe,
        Some("off") => CompressionPolicy::Off,
        _ => CompressionPolicy::Safe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_semantic_strategy_maps_to_balanced() {
        assert_eq!(
            compression_strategy_to_policy(Some("semantic_v2")),
            CompressionPolicy::Balanced
        );
        assert_eq!(
            compression_strategy_to_policy(Some("sliding_window")),
            CompressionPolicy::Balanced
        );
    }

    #[test]
    fn conservative_strategies_map_to_safe() {
        assert_eq!(
            compression_strategy_to_policy(Some("frozen_floor")),
            CompressionPolicy::Safe
        );
        assert_eq!(
            compression_strategy_to_policy(Some("tool_schema")),
            CompressionPolicy::Safe
        );
        assert_eq!(
            compression_strategy_to_policy(Some("default")),
            CompressionPolicy::Safe
        );
    }

    #[test]
    fn unknown_hint_never_fails_and_stays_conservative() {
        assert_eq!(
            compression_strategy_to_policy(None),
            CompressionPolicy::Safe
        );
        assert_eq!(
            compression_strategy_to_policy(Some("some-future-strategy")),
            CompressionPolicy::Safe
        );
    }

    #[test]
    fn explicit_off_is_respected() {
        assert_eq!(
            compression_strategy_to_policy(Some("off")),
            CompressionPolicy::Off
        );
    }
}
