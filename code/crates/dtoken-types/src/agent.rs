use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
