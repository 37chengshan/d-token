use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostPlatform {
    Macos,
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentInstallCapability {
    OneClick,
    ConfirmRequired,
    PromptOnly,
    ManualOnly,
}

impl AgentInstallCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OneClick => "one-click",
            Self::ConfirmRequired => "confirm-required",
            Self::PromptOnly => "prompt-only",
            Self::ManualOnly => "manual-only",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentSessionCapability {
    Metadata,
    DescriptorOnly,
    Unsupported,
}

impl AgentSessionCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::DescriptorOnly => "descriptor-only",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AgentEvidenceKind {
    Executable,
    AppBundle,
    Config,
    Extension,
    SessionRoot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentDetectionEvidence {
    pub kind: AgentEvidenceKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DetectedAgent {
    pub id: String,
    pub name: String,
    pub install_capability: AgentInstallCapability,
    pub session_capability: AgentSessionCapability,
    pub evidence: Vec<AgentDetectionEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentDiscoverySnapshot {
    pub observed_at_ms: u64,
    pub evidence_revision: String,
    pub agents: Vec<DetectedAgent>,
}

/// A redacted identity for the upstream currently selected by an Agent.
///
/// This deliberately carries no credential material: callers must never put
/// API keys, cookies, authorization headers, or raw configuration payloads in
/// any field of this contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpstreamSignature {
    pub provider_id: String,
    pub model: String,
    pub protocol: String,
    pub normalized_endpoint: String,
    pub profile_scope: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentCredentialState {
    ReusableReference,
    Missing,
    PresentButUnavailable,
    NotRequired,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentConfigOperation {
    Configure,
    Clear,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentConfigReceiptState {
    Planned,
    Applied,
    Verified,
    RolledBack,
    Failed,
}

/// A target is always represented by its redacted display path and bounded
/// diff. The actual bytes remain in ConfigPatcher's private backup records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfigTarget {
    pub display_path: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub redacted_diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfigSnapshot {
    pub agent_id: String,
    pub config_targets: Vec<String>,
    pub current_upstream: Option<UpstreamSignature>,
    pub config_fingerprint: String,
    pub credential_state: AgentCredentialState,
    pub source_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfigPlan {
    pub operation: AgentConfigOperation,
    pub snapshot: AgentConfigSnapshot,
    pub targets: Vec<AgentConfigTarget>,
    pub runtime_reload_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfigReceipt {
    pub operation: AgentConfigOperation,
    pub agent_id: String,
    pub state: AgentConfigReceiptState,
    pub transaction_id: Option<String>,
    pub rollback_ids: Vec<String>,
    pub verified_fingerprint: Option<String>,
    pub runtime_reload_required: bool,
    pub summary: String,
}

/// A durable, metadata-only authorization for following an externally owned
/// cc-switch selection. It never contains a provider endpoint, configuration
/// payload, profile name, or credential reference: the current selection is
/// always re-discovered read-only and re-validated before every sync.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CcSwitchAutoFollowAuthorization {
    pub schema_version: u8,
    pub enabled: bool,
    pub authorized_at_ms: u64,
    pub authorized_source_fingerprint: String,
    /// Hashes of provider/source/protocol/normalized-endpoint identities that
    /// existed when the user approved follow mode. Model changes within an
    /// approved identity may follow automatically; new providers, endpoints,
    /// or protocols require another review.
    pub authorized_upstream_signatures: Vec<String>,
    pub last_observed_source_fingerprint: Option<String>,
    pub last_synced_source_fingerprint: Option<String>,
    pub last_observed_route_signature: Option<String>,
    pub last_synced_route_signature: Option<String>,
    pub last_sync_state: String,
}

/// A redacted receipt for a d-token-only route follow-up. The external
/// cc-switch source is never modified and credentials are never copied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CcSwitchRouteSyncReceipt {
    pub schema_version: u8,
    pub state: String,
    pub source_fingerprint: String,
    pub route_before_sha256: Option<String>,
    pub route_after_sha256: Option<String>,
    pub rollback_id: Option<String>,
    pub runtime_state: String,
    pub occurred_at_ms: u64,
}

/// One credential-free upstream that was visible when the user approved
/// Agent follow mode. The endpoint is normalized and stripped of user-info,
/// query, and fragment data before it reaches this contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRouteFollowBinding {
    pub agent_id: String,
    pub candidate_id: String,
    pub candidate_source: String,
    pub upstream: UpstreamSignature,
}

/// A durable authorization for following already-reviewed Agent metadata.
///
/// This authorization never permits d-token to rewrite the external Agent
/// configuration. It may update only d-token's own active route, and only
/// while the newly observed upstream identity is in the per-Agent approved
/// hash set below.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentAutoFollowAuthorization {
    pub schema_version: u8,
    pub enabled: bool,
    pub authorized_at_ms: u64,
    pub authorized_upstream_signatures: BTreeMap<String, Vec<String>>,
    pub last_synced_bindings: Vec<AgentRouteFollowBinding>,
    pub last_observed_config_fingerprints: BTreeMap<String, String>,
    pub last_observed_source_fingerprint: Option<String>,
    pub last_synced_source_fingerprint: Option<String>,
    pub last_synced_route_signature: Option<String>,
    pub active_config_sha256: String,
    pub runtime_identity: Option<String>,
    pub last_sync_state: String,
}

/// A metadata-only receipt for an Agent-driven d-token route refresh.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRouteSyncReceipt {
    pub schema_version: u8,
    pub state: String,
    pub source_fingerprint: String,
    pub changed_agents: Vec<String>,
    pub route_before_sha256: Option<String>,
    pub route_after_sha256: Option<String>,
    pub rollback_id: Option<String>,
    pub runtime_state: String,
    pub occurred_at_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_config_contract_round_trips_without_configuration_payloads() {
        let plan = AgentConfigPlan {
            operation: AgentConfigOperation::Configure,
            snapshot: AgentConfigSnapshot {
                agent_id: "codex".to_string(),
                config_targets: vec!["~/.codex/config.toml".to_string()],
                current_upstream: Some(UpstreamSignature {
                    provider_id: "openai".to_string(),
                    model: "gpt-5".to_string(),
                    protocol: "responses".to_string(),
                    normalized_endpoint: "https://api.example.test/v1".to_string(),
                    profile_scope: None,
                }),
                config_fingerprint: "a".repeat(64),
                credential_state: AgentCredentialState::ReusableReference,
                source_revision: "source-v1".to_string(),
            },
            targets: vec![AgentConfigTarget {
                display_path: "~/.codex/config.toml".to_string(),
                before_sha256: "b".repeat(64),
                after_sha256: "c".repeat(64),
                redacted_diff: "@@ -1 +1 @@\n-model_provider = \"openai\"".to_string(),
            }],
            runtime_reload_required: true,
        };

        let encoded = serde_json::to_string(&plan).expect("serialize plan");
        assert!(!encoded.contains("api_key"));
        assert!(!encoded.contains("Authorization"));
        assert_eq!(
            serde_json::from_str::<AgentConfigPlan>(&encoded).expect("deserialize plan"),
            plan
        );
    }

    #[test]
    fn cc_switch_follow_contract_is_metadata_only() {
        const SECRET: &str = "CC_SWITCH_SECRET_MUST_NOT_ESCAPE";
        let authorization = CcSwitchAutoFollowAuthorization {
            schema_version: 2,
            enabled: true,
            authorized_at_ms: 42,
            authorized_source_fingerprint: "external-config-abc".to_string(),
            authorized_upstream_signatures: vec!["approved-upstream-hash".to_string()],
            last_observed_source_fingerprint: Some("external-config-def".to_string()),
            last_synced_source_fingerprint: Some("external-config-abc".to_string()),
            last_observed_route_signature: Some("route-def".to_string()),
            last_synced_route_signature: Some("route-abc".to_string()),
            last_sync_state: "applied_verified".to_string(),
        };
        let receipt = CcSwitchRouteSyncReceipt {
            schema_version: 1,
            state: "applied_verified".to_string(),
            source_fingerprint: "external-config-def".to_string(),
            route_before_sha256: Some("a".repeat(64)),
            route_after_sha256: Some("b".repeat(64)),
            rollback_id: Some("rollback-safe".to_string()),
            runtime_state: "runtime_reloaded".to_string(),
            occurred_at_ms: 43,
        };

        let encoded = serde_json::to_string(&(authorization, receipt)).expect("serialize");
        assert!(!encoded.contains(SECRET));
        assert!(!encoded.contains("api_key"));
        assert!(!encoded.contains("Authorization"));
    }

    #[test]
    fn agent_follow_contract_keeps_only_redacted_route_metadata() {
        const SECRET: &str = "AGENT_FOLLOW_SECRET_MUST_NOT_ESCAPE";
        let authorization = AgentAutoFollowAuthorization {
            schema_version: 1,
            enabled: true,
            authorized_at_ms: 42,
            authorized_upstream_signatures: BTreeMap::from([(
                "cline".to_string(),
                vec!["approved-upstream-hash".to_string()],
            )]),
            last_synced_bindings: vec![AgentRouteFollowBinding {
                agent_id: "cline".to_string(),
                candidate_id: "cline-local".to_string(),
                candidate_source: "agent-config:cline-cli-v1".to_string(),
                upstream: UpstreamSignature {
                    provider_id: "cline-local".to_string(),
                    model: "model-a".to_string(),
                    protocol: "open_ai".to_string(),
                    normalized_endpoint: "http://127.0.0.1:5001/v1".to_string(),
                    profile_scope: Some("agent-config:cline-cli-v1".to_string()),
                },
            }],
            last_observed_config_fingerprints: BTreeMap::from([(
                "cline".to_string(),
                "agent-config-a".to_string(),
            )]),
            last_observed_source_fingerprint: Some("agent-source-a".to_string()),
            last_synced_source_fingerprint: Some("agent-source-a".to_string()),
            last_synced_route_signature: Some("route-a".to_string()),
            active_config_sha256: "a".repeat(64),
            runtime_identity: Some("dtoken-runtime-local".to_string()),
            last_sync_state: "applied_verified".to_string(),
        };
        let receipt = AgentRouteSyncReceipt {
            schema_version: 1,
            state: "applied_verified".to_string(),
            source_fingerprint: "agent-source-b".to_string(),
            changed_agents: vec!["cline".to_string()],
            route_before_sha256: Some("a".repeat(64)),
            route_after_sha256: Some("b".repeat(64)),
            rollback_id: Some("rollback-safe".to_string()),
            runtime_state: "runtime_reloaded".to_string(),
            occurred_at_ms: 43,
        };

        let encoded = serde_json::to_string(&(authorization, receipt)).expect("serialize");
        assert!(!encoded.contains(SECRET));
        assert!(!encoded.contains("api_key"));
        assert!(!encoded.contains("Authorization"));
        assert!(!encoded.contains("cookie"));
    }
}
