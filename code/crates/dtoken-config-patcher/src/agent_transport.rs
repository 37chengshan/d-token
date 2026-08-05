//! Narrow, fail-closed transport adapters for Agent configuration.
//!
//! Claude Code, Codex, Cline CLI, OpenCode, OpenClaw, Goose, Kilo, Gemini CLI,
//! Roo Code, Continue, and Aider use versioned schema adapters plus d-token
//! ownership evidence that captures only the previous non-secret selection. The
//! adapters never search recursively for provider fields and never copy user
//! credential values into receipts or ownership manifests. Multi-provider
//! adapters add a dedicated d-token entry and preserve every user-owned
//! provider. Agents whose provider schema or wire protocol is not verified
//! remain fail-closed.

use anyhow::{bail, Context, Result};
use dtoken_compress::optical::contains_secret_like;
use dtoken_types::{ConfigToolKind, RiskLevel};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::agent_kind::{AgentIntegrationKind, TruthfulLimitation};
use crate::{ApplyReceipt, ConfigPatcher, ExpectedFileState, TransactionReceipt};

pub const DTOKEN_MANAGED_KEY: &str = "dtoken";
pub const DTOKEN_MANAGED_BY: &str = "d-token";
pub const DTOKEN_TRANSPORT: &str = "d-token-loopback";
/// Canonical OpenCode provider identity written by both the managed transport
/// and the dedicated desktop connection flow. The desktop adapter must reuse
/// these constants instead of duplicating the literals.
pub const OPENCODE_PROVIDER_ID: &str = "dtoken-local-route";
pub const OPENCODE_PROVIDER_NPM: &str = "@ai-sdk/openai-compatible";
pub const OPENCODE_PROVIDER_NAME: &str = "d-token local route";
/// Opaque environment reference; real credentials never enter the config.
pub const OPENCODE_TOKEN_REFERENCE: &str = "{env:DTOKEN_LOCAL_LISTENER_TOKEN}";
const GOOSE_PROVIDER_ID: &str = "dtoken-local-route";
const GOOSE_PROVIDER_NAME: &str = "d-token local route";
const GOOSE_MANIFEST_FILE: &str = "dtoken-agent-transport-goose.json";
const KILO_MANIFEST_FILE: &str = "dtoken-agent-transport-kilo.json";
const CLINE_PROVIDER_ID: &str = "openai-compatible";
const CLINE_PUBLIC_API_KEY_MARKER: &str = "dtoken-loopback";
const CLINE_MANIFEST_FILE: &str = "dtoken-agent-transport-cline.json";
const OPENCLAW_PROVIDER_ID: &str = "dtoken-local-route";
const OPENCLAW_PROVIDER_NAME: &str = "d-token local route";
const OPENCLAW_MANIFEST_FILE: &str = "dtoken-agent-transport-openclaw.json";

const GEMINI_MANIFEST_FILE: &str = "dtoken-agent-transport-gemini.json";
const GEMINI_PROVIDER_ID: &str = "gemini-env";
const GEMINI_ENV_FILE: &str = ".env";
const GEMINI_ENV_API_KEY: &str = "GEMINI_API_KEY";
const GEMINI_ENV_MODEL: &str = "GEMINI_MODEL";
const GEMINI_ENV_BASE_URL: &str = "GOOGLE_GEMINI_BASE_URL";
const GEMINI_AUTH_SELECTED_TYPE: &str = "selectedType";
const GEMINI_AUTH_TYPE_API_KEY: &str = "gemini-api-key";
const GEMINI_PUBLIC_API_KEY_MARKER: &str = "dtoken-loopback";

const ROO_PROVIDER_ID: &str = "dtoken-local-route";
const ROO_PROVIDER_NAME: &str = "d-token local route";
const ROO_API_PROVIDER: &str = "openai";
const ROO_MANIFEST_FILE: &str = "dtoken-agent-transport-roo.json";
const ROO_PUBLIC_API_KEY_MARKER: &str = "dtoken-loopback";

const CONTINUE_PROVIDER_ID: &str = "openai";
const CONTINUE_MODEL_TITLE: &str = "d-token local route";
const CONTINUE_YAML_CONFIG: &str = "config.yaml";
const CONTINUE_MANIFEST_FILE: &str = "dtoken-agent-transport-continue.json";
const CONTINUE_PUBLIC_API_KEY_MARKER: &str = "dtoken-loopback";

const AIDER_PROVIDER_ID: &str = "openai";
const AIDER_MANIFEST_FILE: &str = "dtoken-agent-transport-aider.json";
const AIDER_PUBLIC_API_KEY_MARKER: &str = "dtoken-loopback";

// Hermes: ~/.hermes/config.yaml — model.provider/custom/base_url/default
const HERMES_PROVIDER_ID: &str = "dtoken-local-route";
const HERMES_MANIFEST_FILE: &str = "dtoken-agent-transport-hermes.json";
const HERMES_PUBLIC_API_KEY_MARKER: &str = "dtoken-loopback";

// OpenHands V0: CWD config.toml [llm]
const OPENHANDS_PROVIDER_ID: &str = "dtoken-local-route";
const OPENHANDS_MANIFEST_FILE: &str = "dtoken-agent-transport-openhands.json";
const OPENHANDS_PUBLIC_API_KEY_MARKER: &str = "dtoken-loopback";

// Pi: ~/.pi/agent/models.json — providers.dtoken + settings
const PI_PROVIDER_ID: &str = "dtoken";
const PI_MANIFEST_FILE: &str = "dtoken-agent-transport-pi.json";
const PI_PUBLIC_API_KEY_MARKER: &str = "dtoken-loopback";

// Mistral Vibe: ~/.vibe/config.toml — providers/models/active_model
const MISTRAL_VIBE_PROVIDER_ID: &str = "dtoken-local-route";
const MISTRAL_VIBE_MANIFEST_FILE: &str = "dtoken-agent-transport-mistral-vibe.json";
const MISTRAL_VIBE_PUBLIC_API_KEY_MARKER: &str = "dtoken-loopback";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransportAdapterKind {
    ClaudeCode,
    Codex,
    ClineCli,
    Goose,
    KiloCode,
    OpenClaw,
    GeminiCli,
    RooCode,
    Continue,
    Aider,
    JsonEnv,
    Toml,
    ExtensionSetting,
    OpenCodeProvider,
    /// Hermes Route adapter: ~/.hermes/config.yaml model.provider/custom/base_url/default.
    /// Does NOT read .env files.
    Hermes,
    /// OpenHands Route adapter (V0): CWD config.toml [llm] section. V1 env-only
    /// fails closed with no file to write.
    OpenHands,
    /// Pi Route adapter: ~/.pi/agent/models.json providers.dtoken + settings
    /// defaultProvider/defaultModel. Does NOT touch auth.json/apiKey.
    Pi,
    /// Mistral Vibe Route adapter: ~/.vibe/config.toml (or project .vibe/config.toml)
    /// providers/models/active_model. Does NOT touch .env.
    MistralVibe,
    /// Cortex Code Hook adapter: ~/.snowflake/cortex/hooks.json hooks.SessionStart
    /// with verified dtoken-agent-bridge mcp stdio hook session-start.
    /// Does NOT touch ~/.snowflake/config.toml or connections.toml.
    CortexCodeHook,
    /// MCP-based integration: adds d-token as an MCP server in the agent's
    /// MCP configuration file. Does NOT provide route evidence or transparent
    /// compression. Model requests continue to use the agent's existing upstream.
    CursorMcp,
    ClaudeDesktopMcp,
    WindsurfMcp,
    CommandCodeMcp,
    CopilotCliMcp,
    VscodeCopilotChatMcp,
    AntigravityMcp,
    DedicatedUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTransportAdapterDescriptor {
    pub id: &'static str,
    pub kind: AgentTransportAdapterKind,
    /// Whether this adapter has a verified, credential-free configure target.
    pub can_configure: bool,
    /// Whether this adapter has a verified way to remove its own routing
    /// changes. Configure support alone is not evidence that a generic
    /// d-token metadata block can safely be cleared.
    pub can_clear: bool,
    /// Human-readable, non-secret reason exposed when an adapter deliberately
    /// refuses automatic configuration. `None` means the adapter schema is
    /// verified for the version-independent surface it writes.
    pub advanced_required_reason: Option<&'static str>,
    /// The kind of integration this adapter provides (Route, MCP, or Hook).
    pub integration_kind: AgentIntegrationKind,
    /// Truthful limitation describing what this integration can and cannot do.
    /// `None` for Route adapters that provide full route evidence.
    pub truthful_limitation: Option<TruthfulLimitation>,
}

const ADAPTERS: &[AgentTransportAdapterDescriptor] = &[
    AgentTransportAdapterDescriptor {
        id: "claude-code",
        kind: AgentTransportAdapterKind::ClaudeCode,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "codex",
        kind: AgentTransportAdapterKind::Codex,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "goose",
        kind: AgentTransportAdapterKind::Goose,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "kilo-code",
        kind: AgentTransportAdapterKind::KiloCode,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "cursor",
        kind: AgentTransportAdapterKind::CursorMcp,
        can_configure: false,
        can_clear: false,
        advanced_required_reason: Some(MCP_VERIFIED_SCHEMA_NO_COMMAND),
        integration_kind: AgentIntegrationKind::Mcp,
        truthful_limitation: Some(TruthfulLimitation::mcp()),
    },
    AgentTransportAdapterDescriptor {
        id: "cline",
        kind: AgentTransportAdapterKind::ClineCli,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "gemini-cli",
        kind: AgentTransportAdapterKind::GeminiCli,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "opencode",
        kind: AgentTransportAdapterKind::OpenCodeProvider,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "openclaw",
        kind: AgentTransportAdapterKind::OpenClaw,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "commandcode",
        kind: AgentTransportAdapterKind::CommandCodeMcp,
        can_configure: false,
        can_clear: false,
        advanced_required_reason: Some(MCP_VERIFIED_SCHEMA_NO_COMMAND),
        integration_kind: AgentIntegrationKind::Mcp,
        truthful_limitation: Some(TruthfulLimitation::mcp()),
    },
    AgentTransportAdapterDescriptor {
        id: "claude-desktop",
        kind: AgentTransportAdapterKind::ClaudeDesktopMcp,
        can_configure: false,
        can_clear: false,
        advanced_required_reason: Some(MCP_VERIFIED_SCHEMA_NO_COMMAND),
        integration_kind: AgentIntegrationKind::Mcp,
        truthful_limitation: Some(TruthfulLimitation::mcp()),
    },
    AgentTransportAdapterDescriptor {
        id: "hermes",
        kind: AgentTransportAdapterKind::Hermes,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "roo-code",
        kind: AgentTransportAdapterKind::RooCode,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "continue",
        kind: AgentTransportAdapterKind::Continue,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "aider",
        kind: AgentTransportAdapterKind::Aider,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "openhands",
        kind: AgentTransportAdapterKind::OpenHands,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "copilot-cli",
        kind: AgentTransportAdapterKind::CopilotCliMcp,
        can_configure: false,
        can_clear: false,
        advanced_required_reason: Some(MCP_VERIFIED_SCHEMA_NO_COMMAND),
        integration_kind: AgentIntegrationKind::Mcp,
        truthful_limitation: Some(TruthfulLimitation::mcp()),
    },
    AgentTransportAdapterDescriptor {
        id: "vscode-copilot-chat",
        kind: AgentTransportAdapterKind::VscodeCopilotChatMcp,
        can_configure: false,
        can_clear: false,
        advanced_required_reason: Some(
            "VS Code Copilot Chat's MCP config is VS Code's mcp.json (`servers` key), which is not a verified write surface in this candidate; detection remains metadata-only",
        ),
        integration_kind: AgentIntegrationKind::Mcp,
        truthful_limitation: Some(TruthfulLimitation::mcp()),
    },
    AgentTransportAdapterDescriptor {
        id: "pi",
        kind: AgentTransportAdapterKind::Pi,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
    AgentTransportAdapterDescriptor {
        id: "cortex-code",
        kind: AgentTransportAdapterKind::CortexCodeHook,
        can_configure: false,
        can_clear: false,
        advanced_required_reason: Some(
            "Cortex Code hooks.json SessionStart requires the desktop-resolved dtoken-agent-bridge mcp stdio hook session-start executable path; the transport adapter does not guess paths",
        ),
        integration_kind: AgentIntegrationKind::Hook,
        truthful_limitation: Some(TruthfulLimitation::hook()),
    },
    AgentTransportAdapterDescriptor {
        id: "windsurf",
        kind: AgentTransportAdapterKind::WindsurfMcp,
        can_configure: false,
        can_clear: false,
        advanced_required_reason: Some(MCP_VERIFIED_SCHEMA_NO_COMMAND),
        integration_kind: AgentIntegrationKind::Mcp,
        truthful_limitation: Some(TruthfulLimitation::mcp()),
    },
    AgentTransportAdapterDescriptor {
        id: "antigravity",
        kind: AgentTransportAdapterKind::AntigravityMcp,
        can_configure: false,
        can_clear: false,
        advanced_required_reason: Some(
            "Antigravity has no verified MCP config path/schema in this candidate (reported paths conflict); detection remains metadata-only",
        ),
        integration_kind: AgentIntegrationKind::Mcp,
        truthful_limitation: Some(TruthfulLimitation::mcp()),
    },
    AgentTransportAdapterDescriptor {
        id: "mistral-vibe",
        kind: AgentTransportAdapterKind::MistralVibe,
        can_configure: true,
        can_clear: true,
        advanced_required_reason: None,
        integration_kind: AgentIntegrationKind::Route,
        truthful_limitation: None,
    },
];

pub fn known_agent_transport_adapters() -> &'static [AgentTransportAdapterDescriptor] {
    ADAPTERS
}

pub fn agent_transport_adapter(id: &str) -> Option<&'static AgentTransportAdapterDescriptor> {
    ADAPTERS.iter().find(|adapter| adapter.id == id)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTransportRequest {
    pub agent_id: String,
    pub target_path: PathBuf,
    /// A loopback endpoint only (for example `http://127.0.0.1:8787`).
    pub endpoint: String,
    pub enabled: bool,
    /// Required for the dedicated OpenCode provider adapter. Other adapters do
    /// not read or modify a model field.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTransportOperation {
    Configure,
    Clear,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTransportVerification {
    pub agent_id: String,
    pub kind: AgentTransportAdapterKind,
    pub ok: bool,
    pub detail: String,
    /// A signature derived only from the d-token-owned entry. It deliberately
    /// never includes user-owned upstream fields, API keys, or environment
    /// variable values.
    pub current_upstream: Option<AgentTransportUpstreamSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTransportUpstreamSignature {
    pub provider_id: String,
    pub endpoint: String,
    pub model: Option<String>,
    pub enabled: bool,
    pub signature_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTransportReceipt {
    pub operation: AgentTransportOperation,
    pub agent_id: String,
    pub target_path: PathBuf,
    pub before_sha256: String,
    pub after_sha256: String,
    pub changed: bool,
    pub transaction: Option<TransactionReceipt>,
    pub verification: AgentTransportVerification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTransportReview {
    pub operation: AgentTransportOperation,
    pub agent_id: String,
    pub target_path: PathBuf,
    pub before_sha256: String,
    pub after_sha256: String,
    pub revision: String,
    pub changed: bool,
    pub redacted_diff: String,
    pub current_upstream: Option<AgentTransportUpstreamSignature>,
}

pub fn configure_agent_transport(
    request: &AgentTransportRequest,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    apply_agent_transport(request, AgentTransportOperation::Configure, backup_root)
}

pub fn clear_agent_transport(
    request: &AgentTransportRequest,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    if !can_clear_agent_transport(&request.agent_id, &request.target_path) {
        let descriptor = agent_transport_adapter(&request.agent_id);
        if descriptor.map(|d| d.can_clear).unwrap_or(false) {
            // For manifest-based adapters, a missing manifest means "never
            // configured or already cleared" — let apply_agent_transport
            // handle it as a no-op. A present manifest with mismatched
            // selection means external edit → fail-closed.
            if !manifest_artifact_exists(&request.agent_id, &request.target_path) {
                // No artifact → let apply_agent_transport handle it (no-op)
            } else {
                bail!(
                    "{} selection changed outside d-token; automatic clear is unsafe",
                    request.agent_id
                );
            }
        }
        // For unsupported adapters (can_clear=false), fall through to
        // apply_agent_transport which calls prepare_request and gives the
        // proper error message.
    }
    apply_agent_transport(request, AgentTransportOperation::Clear, backup_root)
}

/// Returns true when a manifest-based adapter has a d-token ownership
/// manifest file on disk.  Non-manifest adapters always return false
/// (they cannot distinguish "no artifact" from "external edit" at the
/// manifest level, so can_clear_agent_transport alone governs them).
fn manifest_artifact_exists(agent_id: &str, target_path: &Path) -> bool {
    let Some(descriptor) = agent_transport_adapter(agent_id) else {
        return false;
    };
    match descriptor.kind {
        AgentTransportAdapterKind::Pi => pi_manifest_path(target_path)
            .map(|p| p.is_file())
            .unwrap_or(false),
        AgentTransportAdapterKind::Hermes => hermes_manifest_path(target_path)
            .map(|p| p.is_file())
            .unwrap_or(false),
        AgentTransportAdapterKind::OpenHands => openhands_manifest_path(target_path)
            .map(|p| p.is_file())
            .unwrap_or(false),
        AgentTransportAdapterKind::MistralVibe => mistral_vibe_manifest_path(target_path)
            .map(|p| p.is_file())
            .unwrap_or(false),
        _ => false,
    }
}

/// Produces a hash-bound, no-write review for a configure operation.
pub fn preview_configure_agent_transport(
    request: &AgentTransportRequest,
) -> Result<AgentTransportReview> {
    preview_agent_transport(request, AgentTransportOperation::Configure)
}

/// Produces a hash-bound, no-write review for a clear operation.
pub fn preview_clear_agent_transport(
    request: &AgentTransportRequest,
) -> Result<AgentTransportReview> {
    preview_agent_transport(request, AgentTransportOperation::Clear)
}

/// Applies only the exact configuration content reviewed by the caller. The
/// patcher rechecks the on-disk hash before making its durable backup, so a
/// concurrent edit fails closed even after this comparison succeeds.
pub fn apply_reviewed_agent_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
    reviewed_revision: &str,
    reviewed_before_sha256: &str,
    reviewed_after_sha256: &str,
) -> Result<AgentTransportReceipt> {
    let review = preview_agent_transport(request, operation)?;
    if review.revision != reviewed_revision
        || review.before_sha256 != reviewed_before_sha256
        || review.after_sha256 != reviewed_after_sha256
    {
        bail!("Agent transport configuration changed after review; generate a new preview");
    }
    apply_agent_transport(request, operation, backup_root)
}

pub fn verify_agent_transport(
    request: &AgentTransportRequest,
) -> Result<AgentTransportVerification> {
    let descriptor = descriptor_for(request)?;
    match descriptor.kind {
        AgentTransportAdapterKind::ClineCli => {
            return verify_cline_transport(request);
        }
        AgentTransportAdapterKind::Goose => {
            return verify_goose_transport(request);
        }
        AgentTransportAdapterKind::KiloCode => {
            return verify_kilo_transport(request);
        }
        AgentTransportAdapterKind::OpenClaw => {
            return verify_openclaw_transport(request);
        }
        AgentTransportAdapterKind::GeminiCli => {
            return verify_gemini_transport(request);
        }
        AgentTransportAdapterKind::RooCode => {
            return verify_roo_transport(request);
        }
        AgentTransportAdapterKind::Continue => {
            return verify_continue_transport(request);
        }
        AgentTransportAdapterKind::Aider => {
            return verify_aider_transport(request);
        }
        AgentTransportAdapterKind::Hermes => {
            return verify_hermes_transport(request);
        }
        AgentTransportAdapterKind::OpenHands => {
            return verify_openhands_transport(request);
        }
        AgentTransportAdapterKind::Pi => {
            return verify_pi_transport(request);
        }
        AgentTransportAdapterKind::MistralVibe => {
            return verify_mistral_vibe_transport(request);
        }
        _ => {}
    }
    let text = match fs::read_to_string(&request.target_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AgentTransportVerification {
                agent_id: request.agent_id.clone(),
                kind: descriptor.kind,
                ok: false,
                detail: "configuration file is missing".into(),
                current_upstream: None,
            })
        }
        Err(error) => return Err(error).context("failed to read Agent configuration"),
    };
    verify_document(
        descriptor,
        &text,
        &request.endpoint,
        request.enabled,
        request.model.as_deref(),
    )
}

/// Reports whether the on-disk configuration has a complete, unmodified
/// d-token ownership record that can be safely cleared. Schema support alone
/// is deliberately insufficient: legacy or user-owned d-token-shaped entries
/// keep Clear disabled in the UI.
pub fn can_clear_agent_transport(agent_id: &str, target_path: &Path) -> bool {
    let Some(descriptor) = agent_transport_adapter(agent_id) else {
        return false;
    };
    if !descriptor.can_clear {
        return false;
    }
    match descriptor.kind {
        AgentTransportAdapterKind::ClineCli => {
            return cline_can_clear(target_path);
        }
        AgentTransportAdapterKind::Goose => {
            return goose_can_clear(target_path);
        }
        AgentTransportAdapterKind::KiloCode => {
            return kilo_can_clear(target_path);
        }
        AgentTransportAdapterKind::OpenClaw => {
            return openclaw_can_clear(target_path);
        }
        AgentTransportAdapterKind::GeminiCli => {
            return gemini_can_clear(target_path);
        }
        AgentTransportAdapterKind::RooCode => {
            return roo_can_clear(target_path);
        }
        AgentTransportAdapterKind::Continue => {
            return continue_can_clear(target_path);
        }
        AgentTransportAdapterKind::Aider => {
            return aider_can_clear(target_path);
        }
        AgentTransportAdapterKind::Hermes => {
            return hermes_can_clear(target_path);
        }
        AgentTransportAdapterKind::OpenHands => {
            return openhands_can_clear(target_path);
        }
        AgentTransportAdapterKind::Pi => {
            return pi_can_clear(target_path);
        }
        AgentTransportAdapterKind::MistralVibe => {
            return mistral_vibe_can_clear(target_path);
        }
        _ => {}
    }
    let Ok(text) = fs::read_to_string(target_path) else {
        return false;
    };
    match descriptor.kind {
        AgentTransportAdapterKind::ClaudeCode => {
            let Ok(root) = serde_json::from_str::<Map<String, Value>>(&text) else {
                return false;
            };
            let Ok(Some(marker)) = claude_marker(&root) else {
                return false;
            };
            let Ok(current) = claude_current_selection(&root) else {
                return false;
            };
            ensure_claude_current_matches(&marker, &current).is_ok()
        }
        AgentTransportAdapterKind::Codex => {
            let Ok(document) = text.parse::<toml_edit::DocumentMut>() else {
                return false;
            };
            let Ok(Some(marker)) = codex_marker(&document) else {
                return false;
            };
            let Ok(current) = codex_current_selection(&document) else {
                return false;
            };
            ensure_codex_current_matches(&marker, &document, &current).is_ok()
        }
        AgentTransportAdapterKind::OpenCodeProvider => serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|root| {
                root.pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
                    .cloned()
            })
            .is_some_and(|provider| ensure_owned_opencode_provider(&provider).is_ok()),
        _ => false,
    }
}

/// Pure document-level configure operation; it performs no filesystem or
/// credential access and is suitable for preview/tests.
pub fn configure_agent_transport_document(
    agent_id: &str,
    existing: &str,
    endpoint: &str,
    enabled: bool,
) -> Result<String> {
    let request = AgentTransportRequest {
        agent_id: agent_id.to_string(),
        target_path: PathBuf::from("agent-config"),
        endpoint: endpoint.to_string(),
        enabled,
        model: None,
    };
    let descriptor = prepare_request(&request, AgentTransportOperation::Configure)?;
    transform_document(
        descriptor,
        existing,
        AgentTransportOperation::Configure,
        endpoint,
        enabled,
        None,
    )
}

/// Pure document-level clear operation. Unmarked fields are never guessed.
pub fn clear_agent_transport_document(agent_id: &str, existing: &str) -> Result<String> {
    let request = AgentTransportRequest {
        agent_id: agent_id.to_string(),
        target_path: PathBuf::from("agent-config"),
        endpoint: String::new(),
        enabled: false,
        model: None,
    };
    let descriptor = prepare_request(&request, AgentTransportOperation::Clear)?;
    transform_document(
        descriptor,
        existing,
        AgentTransportOperation::Clear,
        "",
        false,
        None,
    )
}

/// Pure document-level verification operation.
pub fn verify_agent_transport_document(
    agent_id: &str,
    content: &str,
    endpoint: &str,
    enabled: bool,
) -> Result<AgentTransportVerification> {
    let request = AgentTransportRequest {
        agent_id: agent_id.to_string(),
        target_path: PathBuf::from("agent-config"),
        endpoint: endpoint.to_string(),
        enabled,
        model: None,
    };
    let descriptor = descriptor_for(&request)?;
    verify_document(descriptor, content, endpoint, enabled, None)
}

/// Pure document-level OpenCode configure operation. Keeping this separate
/// makes the model requirement explicit instead of fabricating a default.
pub fn configure_opencode_transport_document(
    existing: &str,
    endpoint: &str,
    model: &str,
    enabled: bool,
) -> Result<String> {
    let request = AgentTransportRequest {
        agent_id: "opencode".to_string(),
        target_path: PathBuf::from("opencode.json"),
        endpoint: endpoint.to_string(),
        enabled,
        model: Some(model.to_string()),
    };
    prepare_request(&request, AgentTransportOperation::Configure)?;
    transform_document(
        descriptor_for(&request)?,
        existing,
        AgentTransportOperation::Configure,
        endpoint,
        enabled,
        request.model.as_deref(),
    )
}

fn apply_agent_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let descriptor = prepare_request(request, operation)?;
    match descriptor.kind {
        AgentTransportAdapterKind::ClineCli => {
            return apply_cline_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::Goose => {
            return apply_goose_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::KiloCode => {
            return apply_kilo_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::OpenClaw => {
            return apply_openclaw_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::GeminiCli => {
            return apply_gemini_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::RooCode => {
            return apply_roo_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::Continue => {
            return apply_continue_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::Aider => {
            return apply_aider_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::Hermes => {
            return apply_hermes_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::OpenHands => {
            return apply_openhands_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::Pi => {
            return apply_pi_transport(request, operation, backup_root);
        }
        AgentTransportAdapterKind::MistralVibe => {
            return apply_mistral_vibe_transport(request, operation, backup_root);
        }
        _ => {}
    }
    let before = match fs::read_to_string(&request.target_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).context("failed to read Agent configuration"),
    };
    let after = transform_document(
        descriptor,
        &before,
        operation,
        &request.endpoint,
        request.enabled,
        request.model.as_deref(),
    )?;
    let before_hash = crate::stable_hash(&before);
    let after_hash = crate::stable_hash(&after);
    let transaction = if before == after {
        None
    } else {
        let patcher = ConfigPatcher::new(backup_root);
        let change = patcher.prepare_change(
            &request.target_path,
            &after,
            ConfigToolKind::Agent,
            Some(request.agent_id.clone()),
            RiskLevel::Medium,
        )?;
        Some(patcher.apply_transaction(&[change])?)
    };
    let verification = if operation == AgentTransportOperation::Clear {
        verify_cleared(descriptor, &after)?
    } else {
        verify_document(
            descriptor,
            &after,
            &request.endpoint,
            request.enabled,
            request.model.as_deref(),
        )?
    };
    if !verification.ok {
        if let Some(transaction) = transaction.as_ref() {
            let receipt = transaction
                .changes
                .first()
                .context("Agent transport transaction has no change receipt")?;
            let patcher = ConfigPatcher::new(backup_root);
            patcher.restore_backup(
                &receipt.backup,
                &ExpectedFileState::Hash(receipt.after.sha256.clone().unwrap_or_default()),
            )?;
        }
        bail!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        );
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: before_hash,
        after_sha256: after_hash,
        changed: transaction.is_some(),
        transaction,
        verification,
    })
}

fn preview_agent_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let descriptor = prepare_request(request, operation)?;
    match descriptor.kind {
        AgentTransportAdapterKind::ClineCli => {
            return preview_cline_transport(request, operation);
        }
        AgentTransportAdapterKind::Goose => {
            return preview_goose_transport(request, operation);
        }
        AgentTransportAdapterKind::KiloCode => {
            return preview_kilo_transport(request, operation);
        }
        AgentTransportAdapterKind::OpenClaw => {
            return preview_openclaw_transport(request, operation);
        }
        AgentTransportAdapterKind::GeminiCli => {
            return preview_gemini_transport(request, operation);
        }
        AgentTransportAdapterKind::RooCode => {
            return preview_roo_transport(request, operation);
        }
        AgentTransportAdapterKind::Continue => {
            return preview_continue_transport(request, operation);
        }
        AgentTransportAdapterKind::Aider => {
            return preview_aider_transport(request, operation);
        }
        AgentTransportAdapterKind::Hermes => {
            return preview_hermes_transport(request, operation);
        }
        AgentTransportAdapterKind::OpenHands => {
            return preview_openhands_transport(request, operation);
        }
        AgentTransportAdapterKind::Pi => {
            return preview_pi_transport(request, operation);
        }
        AgentTransportAdapterKind::MistralVibe => {
            return preview_mistral_vibe_transport(request, operation);
        }
        _ => {}
    }
    let before = match fs::read_to_string(&request.target_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).context("failed to read Agent configuration"),
    };
    let after = transform_document(
        descriptor,
        &before,
        operation,
        &request.endpoint,
        request.enabled,
        request.model.as_deref(),
    )?;
    let before_sha256 = crate::stable_hash(&before);
    let after_sha256 = crate::stable_hash(&after);
    let current_upstream = current_upstream_signature(descriptor, &before)?;
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: crate::stable_hash(&format!(
            "agent-transport-v2\n{}\n{:?}\n{}\n{}",
            request.agent_id, operation, before_sha256, after_sha256
        )),
        changed: before != after,
        before_sha256,
        after_sha256,
        redacted_diff: crate::simple_diff(&before, &after),
        current_upstream,
    })
}

fn descriptor_for(
    request: &AgentTransportRequest,
) -> Result<&'static AgentTransportAdapterDescriptor> {
    let descriptor = agent_transport_adapter(&request.agent_id).ok_or_else(|| {
        anyhow::anyhow!("unsupported Agent transport adapter: {}", request.agent_id)
    })?;
    if !descriptor.can_configure && !descriptor.can_clear {
        bail!(
            "Agent {} is advanced_required: {}",
            request.agent_id,
            descriptor.advanced_required_reason.unwrap_or(
                "its provider schema is not verified; configure it manually and do not mark it as routed"
            )
        );
    }
    Ok(descriptor)
}

fn prepare_request(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<&'static AgentTransportAdapterDescriptor> {
    let descriptor = descriptor_for(request)?;
    let supported = match operation {
        AgentTransportOperation::Configure => descriptor.can_configure,
        AgentTransportOperation::Clear => descriptor.can_clear,
    };
    if !supported {
        let operation = match operation {
            AgentTransportOperation::Configure => "configure",
            AgentTransportOperation::Clear => "clear",
        };
        bail!(
            "Agent {} does not support safe {}: {}",
            request.agent_id,
            operation,
            descriptor.advanced_required_reason.unwrap_or(
                "the required provider schema is not verified; configure it manually and do not mark it as routed"
            )
        );
    }
    if operation == AgentTransportOperation::Configure {
        validate_endpoint(&request.endpoint)?;
        if matches!(
            descriptor.kind,
            AgentTransportAdapterKind::ClaudeCode
                | AgentTransportAdapterKind::Codex
                | AgentTransportAdapterKind::ClineCli
                | AgentTransportAdapterKind::Goose
                | AgentTransportAdapterKind::KiloCode
                | AgentTransportAdapterKind::OpenClaw
                | AgentTransportAdapterKind::GeminiCli
                | AgentTransportAdapterKind::RooCode
                | AgentTransportAdapterKind::Continue
                | AgentTransportAdapterKind::Aider
                | AgentTransportAdapterKind::OpenCodeProvider
                | AgentTransportAdapterKind::Hermes
                | AgentTransportAdapterKind::OpenHands
                | AgentTransportAdapterKind::Pi
                | AgentTransportAdapterKind::MistralVibe
        ) {
            validate_model(request.model.as_deref())?;
        }
    }
    Ok(descriptor)
}

fn validate_endpoint(endpoint: &str) -> Result<()> {
    if endpoint.is_empty()
        || contains_secret_like(endpoint)
        || endpoint.contains('@')
        || endpoint.contains('?')
        || endpoint.contains('#')
    {
        bail!("transport endpoint is empty or contains a credential-like value");
    }
    if !(endpoint.starts_with("http://127.0.0.1:")
        || endpoint.starts_with("http://localhost:")
        || endpoint.starts_with("https://127.0.0.1:")
        || endpoint.starts_with("https://localhost:"))
    {
        bail!("transport endpoint must be loopback-only");
    }
    Ok(())
}

fn validate_model(model: Option<&str>) -> Result<()> {
    let model = model.context("Agent transport requires an explicit model")?;
    if model.trim().is_empty()
        || model.len() > 256
        || model.contains(['\n', '\r', '\0'])
        || contains_secret_like(model)
    {
        bail!("Agent model is empty or contains unsupported characters");
    }
    Ok(())
}

fn transform_document(
    descriptor: &AgentTransportAdapterDescriptor,
    text: &str,
    operation: AgentTransportOperation,
    endpoint: &str,
    enabled: bool,
    model: Option<&str>,
) -> Result<String> {
    match descriptor.kind {
        AgentTransportAdapterKind::ClaudeCode => transform_claude(text, operation, endpoint, model),
        AgentTransportAdapterKind::Codex => transform_codex(text, operation, endpoint, model),
        AgentTransportAdapterKind::ClineCli
        | AgentTransportAdapterKind::Goose
        | AgentTransportAdapterKind::KiloCode
        | AgentTransportAdapterKind::OpenClaw
        | AgentTransportAdapterKind::GeminiCli
        | AgentTransportAdapterKind::RooCode
        | AgentTransportAdapterKind::Continue
        | AgentTransportAdapterKind::Aider => {
            bail!("Agent provider schema requires filesystem context")
        }
        AgentTransportAdapterKind::JsonEnv
        | AgentTransportAdapterKind::Toml
        | AgentTransportAdapterKind::ExtensionSetting => {
            bail!("Agent provider schema is unavailable")
        }
        AgentTransportAdapterKind::OpenCodeProvider => {
            transform_opencode(text, operation, endpoint, enabled, model)
        }
        AgentTransportAdapterKind::Hermes => {
            let model = model.context("Hermes transport requires an explicit model")?;
            transform_hermes(text, operation, endpoint, model)
        }
        AgentTransportAdapterKind::OpenHands => {
            bail!("OpenHands provider schema requires filesystem context")
        }
        AgentTransportAdapterKind::Pi => {
            bail!("Pi provider schema requires filesystem context")
        }
        AgentTransportAdapterKind::MistralVibe => {
            bail!("Mistral Vibe provider schema requires filesystem context")
        }
        AgentTransportAdapterKind::CortexCodeHook => bail!(MCP_TRANSPORT_FAIL_CLOSED),
        AgentTransportAdapterKind::CursorMcp
        | AgentTransportAdapterKind::ClaudeDesktopMcp
        | AgentTransportAdapterKind::WindsurfMcp
        | AgentTransportAdapterKind::CommandCodeMcp
        | AgentTransportAdapterKind::CopilotCliMcp
        | AgentTransportAdapterKind::VscodeCopilotChatMcp
        | AgentTransportAdapterKind::AntigravityMcp => bail!(MCP_TRANSPORT_FAIL_CLOSED),
        AgentTransportAdapterKind::DedicatedUnavailable => {
            bail!("Agent provider schema is unavailable")
        }
    }
}

fn merge_transactions(transactions: Vec<TransactionReceipt>) -> Option<TransactionReceipt> {
    let mut iter = transactions.into_iter();
    let mut merged = iter.next()?;
    for next in iter {
        merged.changes.extend(next.changes);
        merged.applied_at = next.applied_at;
    }
    Some(merged)
}

fn rollback_apply_receipts(changes: &[ApplyReceipt], backup_root: &Path) -> Result<()> {
    let patcher = ConfigPatcher::new(backup_root);
    for receipt in changes.iter().rev() {
        patcher.restore_backup(
            &receipt.backup,
            &ExpectedFileState::Hash(receipt.after.sha256.clone().unwrap_or_default()),
        )?;
    }
    Ok(())
}

fn apply_manifest_change(
    target_path: &Path,
    content: &str,
    agent_id: &str,
    backup_root: &Path,
) -> Result<Option<TransactionReceipt>> {
    let before = match fs::read_to_string(target_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).context("failed to read Agent transport manifest"),
    };
    if before == content {
        return Ok(None);
    }
    let patcher = ConfigPatcher::new(backup_root);
    let change = patcher.prepare_change(
        target_path,
        content,
        ConfigToolKind::Agent,
        Some(agent_id.to_string()),
        RiskLevel::Medium,
    )?;
    Ok(Some(patcher.apply_transaction(&[change])?))
}

fn revision_for_hashes(
    agent_id: &str,
    operation: AgentTransportOperation,
    hashes: &[(&str, String, String)],
) -> String {
    let mut material = format!("agent-transport-v3\n{agent_id}\n{operation:?}\n");
    for (label, before, after) in hashes {
        material.push_str(label);
        material.push('\n');
        material.push_str(before);
        material.push('\n');
        material.push_str(after);
        material.push('\n');
    }
    crate::stable_hash(&material)
}

fn redacted_diff_sections(sections: &[(&str, &str, &str)]) -> String {
    sections
        .iter()
        .map(|(label, before, after)| {
            format!(
                "### {label}\n{}",
                crate::simple_diff(before, after).trim_end()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn optional_text(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn remove_owned_file(path: &Path) -> Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn manifest_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)? + "\n")
}

fn goose_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Goose configuration path has no parent directory")?
        .join(GOOSE_MANIFEST_FILE))
}

fn goose_provider_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Goose configuration path has no parent directory")?
        .join("custom_providers")
        .join(format!("{GOOSE_PROVIDER_ID}.json")))
}

fn kilo_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Kilo configuration path has no parent directory")?
        .join(KILO_MANIFEST_FILE))
}

fn cline_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Cline provider configuration path has no parent directory")?
        .join(CLINE_MANIFEST_FILE))
}

fn openclaw_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("OpenClaw configuration path has no parent directory")?
        .join(OPENCLAW_MANIFEST_FILE))
}

fn parse_goose_manifest(path: &Path) -> Result<Option<GooseTransportManifest>> {
    let Some(text) = optional_text(path)? else {
        return Ok(None);
    };
    let manifest = serde_json::from_str::<GooseTransportManifest>(&text)
        .context("Goose transport manifest is invalid JSON")?;
    validate_goose_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn parse_kilo_manifest(path: &Path) -> Result<Option<KiloTransportManifest>> {
    let Some(text) = optional_text(path)? else {
        return Ok(None);
    };
    let manifest = serde_json::from_str::<KiloTransportManifest>(&text)
        .context("Kilo transport manifest is invalid JSON")?;
    validate_kilo_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn parse_cline_manifest(path: &Path) -> Result<Option<ClineTransportManifest>> {
    let Some(text) = optional_text(path)? else {
        return Ok(None);
    };
    let manifest = serde_json::from_str::<ClineTransportManifest>(&text)
        .context("Cline transport manifest is invalid JSON")?;
    validate_cline_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn parse_openclaw_manifest(path: &Path) -> Result<Option<OpenClawTransportManifest>> {
    let Some(text) = optional_text(path)? else {
        return Ok(None);
    };
    let manifest = serde_json::from_str::<OpenClawTransportManifest>(&text)
        .context("OpenClaw transport manifest is invalid JSON")?;
    validate_openclaw_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn validate_goose_manifest(manifest: &GooseTransportManifest) -> Result<()> {
    if manifest.schema_version != TRANSPORT_SCHEMA_VERSION
        || manifest.managed_by != DTOKEN_MANAGED_BY
        || manifest.agent_id != "goose"
        || manifest.transport != DTOKEN_TRANSPORT
        || manifest.provider_id != GOOSE_PROVIDER_ID
    {
        bail!("Goose transport manifest is incomplete or modified; automatic clear is unsafe");
    }
    validate_endpoint(&manifest.endpoint)?;
    validate_model(Some(&manifest.model))?;
    validate_goose_provider_id(manifest.previous_provider.as_deref())?;
    validate_prior_selection(None, manifest.previous_model.as_deref())?;
    Ok(())
}

fn validate_kilo_manifest(manifest: &KiloTransportManifest) -> Result<()> {
    if manifest.schema_version != TRANSPORT_SCHEMA_VERSION
        || manifest.managed_by != DTOKEN_MANAGED_BY
        || manifest.agent_id != "kilo-code"
        || manifest.transport != DTOKEN_TRANSPORT
        || manifest.provider_id != OPENCODE_PROVIDER_ID
    {
        bail!("Kilo transport manifest is incomplete or modified; automatic clear is unsafe");
    }
    validate_endpoint(&manifest.endpoint)?;
    validate_model(Some(&manifest.model))?;
    validate_prior_selection(None, manifest.previous_model.as_deref())?;
    Ok(())
}

fn validate_cline_manifest(manifest: &ClineTransportManifest) -> Result<()> {
    if manifest.schema_version != TRANSPORT_SCHEMA_VERSION
        || manifest.managed_by != DTOKEN_MANAGED_BY
        || manifest.agent_id != "cline"
        || manifest.transport != DTOKEN_TRANSPORT
        || manifest.provider_id != CLINE_PROVIDER_ID
    {
        bail!("Cline transport manifest is incomplete or modified; automatic clear is unsafe");
    }
    validate_endpoint(&manifest.endpoint)?;
    validate_model(Some(&manifest.model))?;
    validate_goose_provider_id(manifest.previous_provider.as_deref())?;
    if manifest.previous_provider_present != manifest.previous_provider.is_some() {
        bail!("Cline transport manifest has an inconsistent previous provider");
    }
    Ok(())
}

fn validate_openclaw_manifest(manifest: &OpenClawTransportManifest) -> Result<()> {
    if manifest.schema_version != TRANSPORT_SCHEMA_VERSION
        || manifest.managed_by != DTOKEN_MANAGED_BY
        || manifest.agent_id != "openclaw"
        || manifest.transport != DTOKEN_TRANSPORT
        || manifest.provider_id != OPENCLAW_PROVIDER_ID
    {
        bail!("OpenClaw transport manifest is incomplete or modified; automatic clear is unsafe");
    }
    validate_endpoint(&manifest.endpoint)?;
    validate_model(Some(&manifest.model))?;
    if let Some(previous_primary) = manifest.previous_primary.as_deref() {
        validate_model(Some(previous_primary))?;
    }
    if manifest.previous_primary_present != manifest.previous_primary.is_some() {
        bail!("OpenClaw transport manifest has an inconsistent previous primary model");
    }
    Ok(())
}

fn gemini_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Gemini CLI configuration path has no parent directory")?
        .join(GEMINI_MANIFEST_FILE))
}

fn gemini_env_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Gemini CLI configuration path has no parent directory")?
        .join(GEMINI_ENV_FILE))
}

fn roo_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Roo Code configuration path has no parent directory")?
        .join(ROO_MANIFEST_FILE))
}

fn continue_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Continue configuration path has no parent directory")?
        .join(CONTINUE_MANIFEST_FILE))
}

fn aider_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Aider configuration path has no parent directory")?
        .join(AIDER_MANIFEST_FILE))
}

fn parse_gemini_manifest(path: &Path) -> Result<Option<GeminiTransportManifest>> {
    let Some(text) = optional_text(path)? else {
        return Ok(None);
    };
    let manifest = serde_json::from_str::<GeminiTransportManifest>(&text)
        .context("Gemini CLI transport manifest is invalid JSON")?;
    validate_gemini_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn parse_roo_manifest(path: &Path) -> Result<Option<RooTransportManifest>> {
    let Some(text) = optional_text(path)? else {
        return Ok(None);
    };
    let manifest = serde_json::from_str::<RooTransportManifest>(&text)
        .context("Roo Code transport manifest is invalid JSON")?;
    validate_roo_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn parse_continue_manifest(path: &Path) -> Result<Option<ContinueTransportManifest>> {
    let Some(text) = optional_text(path)? else {
        return Ok(None);
    };
    let manifest = serde_json::from_str::<ContinueTransportManifest>(&text)
        .context("Continue transport manifest is invalid JSON")?;
    validate_continue_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn parse_aider_manifest(path: &Path) -> Result<Option<AiderTransportManifest>> {
    let Some(text) = optional_text(path)? else {
        return Ok(None);
    };
    let manifest = serde_json::from_str::<AiderTransportManifest>(&text)
        .context("Aider transport manifest is invalid JSON")?;
    validate_aider_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn validate_gemini_manifest(manifest: &GeminiTransportManifest) -> Result<()> {
    if manifest.schema_version != TRANSPORT_SCHEMA_VERSION
        || manifest.managed_by != DTOKEN_MANAGED_BY
        || manifest.agent_id != "gemini-cli"
        || manifest.transport != DTOKEN_TRANSPORT
        || manifest.provider_id != GEMINI_PROVIDER_ID
    {
        bail!("Gemini CLI transport manifest is incomplete or modified; automatic clear is unsafe");
    }
    validate_endpoint(&manifest.endpoint)?;
    validate_model(Some(&manifest.model))?;
    if let Some(previous) = manifest.previous_auth_type.as_deref() {
        validate_prior_selection(None, Some(previous))?;
    }
    if manifest.previous_auth_type_present != manifest.previous_auth_type.is_some() {
        bail!("Gemini CLI transport manifest has an inconsistent previous auth type");
    }
    validate_prior_selection(
        manifest.previous_base_url.as_deref(),
        manifest.previous_model.as_deref(),
    )?;
    if manifest.previous_base_url_present != manifest.previous_base_url.is_some()
        || manifest.previous_model_present != manifest.previous_model.is_some()
    {
        bail!("Gemini CLI transport manifest has an inconsistent previous model");
    }
    Ok(())
}

fn validate_roo_manifest(manifest: &RooTransportManifest) -> Result<()> {
    if manifest.schema_version != TRANSPORT_SCHEMA_VERSION
        || manifest.managed_by != DTOKEN_MANAGED_BY
        || manifest.agent_id != "roo-code"
        || manifest.transport != DTOKEN_TRANSPORT
        || manifest.provider_id != ROO_PROVIDER_ID
    {
        bail!("Roo Code transport manifest is incomplete or modified; automatic clear is unsafe");
    }
    validate_endpoint(&manifest.endpoint)?;
    validate_model(Some(&manifest.model))?;
    validate_prior_selection(
        manifest.previous_base_url.as_deref(),
        manifest.previous_model_id.as_deref(),
    )?;
    if manifest.previous_base_url_present != manifest.previous_base_url.is_some()
        || manifest.previous_model_id_present != manifest.previous_model_id.is_some()
        || manifest.previous_config_name_present != manifest.previous_config_name.is_some()
        || manifest.previous_list_present != manifest.previous_list.is_some()
    {
        bail!("Roo Code transport manifest has an inconsistent previous selection");
    }
    if let Some(previous) = manifest.previous_api_provider.as_deref() {
        if previous.trim().is_empty() || previous.len() > 256 || contains_secret_like(previous) {
            bail!("Roo Code transport manifest has an invalid previous api provider");
        }
    }
    if manifest.previous_api_provider_present != manifest.previous_api_provider.is_some() {
        bail!("Roo Code transport manifest has an inconsistent previous api provider");
    }
    Ok(())
}

fn validate_continue_manifest(manifest: &ContinueTransportManifest) -> Result<()> {
    if manifest.schema_version != TRANSPORT_SCHEMA_VERSION
        || manifest.managed_by != DTOKEN_MANAGED_BY
        || manifest.agent_id != "continue"
        || manifest.transport != DTOKEN_TRANSPORT
        || manifest.provider_id != CONTINUE_PROVIDER_ID
    {
        bail!("Continue transport manifest is incomplete or modified; automatic clear is unsafe");
    }
    validate_endpoint(&manifest.endpoint)?;
    validate_model(Some(&manifest.model))?;
    if let Some(previous) = manifest.previous_default_model.as_deref() {
        validate_prior_selection(None, Some(previous))?;
    }
    if manifest.previous_default_model_present != manifest.previous_default_model.is_some()
        || manifest.previous_models_present != manifest.previous_models.is_some()
    {
        bail!("Continue transport manifest has an inconsistent previous selection");
    }
    Ok(())
}

fn validate_aider_manifest(manifest: &AiderTransportManifest) -> Result<()> {
    if manifest.schema_version != TRANSPORT_SCHEMA_VERSION
        || manifest.managed_by != DTOKEN_MANAGED_BY
        || manifest.agent_id != "aider"
        || manifest.transport != DTOKEN_TRANSPORT
        || manifest.provider_id != AIDER_PROVIDER_ID
    {
        bail!("Aider transport manifest is incomplete or modified; automatic clear is unsafe");
    }
    validate_endpoint(&manifest.endpoint)?;
    validate_model(Some(&manifest.model))?;
    validate_prior_selection(
        manifest.previous_base.as_deref(),
        manifest.previous_model.as_deref(),
    )?;
    if manifest.previous_model_present != manifest.previous_model.is_some()
        || manifest.previous_base_present != manifest.previous_base.is_some()
    {
        bail!("Aider transport manifest has an inconsistent previous selection");
    }
    Ok(())
}

fn validate_goose_provider_id(value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        if value.trim().is_empty()
            || value.len() > 256
            || value.contains(['\n', '\r', '\0', ':', '@', '?', '#'])
            || contains_secret_like(value)
        {
            bail!("Goose provider selection is invalid or may contain credential material");
        }
    }
    Ok(())
}

fn goose_line_index(lines: &[String], key: &str) -> Result<Option<usize>> {
    let mut found = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || line.len() != trimmed.len() {
            continue;
        }
        if trimmed
            .strip_prefix(key)
            .is_some_and(|rest| rest.starts_with(':'))
        {
            if found.is_some() {
                bail!("Goose configuration contains duplicate {key} entries");
            }
            found = Some(index);
        }
    }
    Ok(found)
}

fn goose_unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

fn goose_current_selection(text: &str) -> Result<PriorSelection> {
    let lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    let provider = goose_line_index(&lines, "GOOSE_PROVIDER")?.map(|index| {
        let line = &lines[index];
        goose_unquote(
            line.split_once(':')
                .map(|(_, value)| value)
                .unwrap_or_default(),
        )
    });
    let model = goose_line_index(&lines, "GOOSE_MODEL")?.map(|index| {
        let line = &lines[index];
        goose_unquote(
            line.split_once(':')
                .map(|(_, value)| value)
                .unwrap_or_default(),
        )
    });
    validate_goose_provider_id(provider.as_deref())?;
    validate_prior_selection(None, model.as_deref())?;
    Ok(PriorSelection {
        provider_present: provider.is_some(),
        provider,
        model_present: model.is_some(),
        model,
    })
}

fn goose_set_selection(text: &str, key: &str, value: Option<&str>) -> Result<String> {
    let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    let index = goose_line_index(&lines, key)?;
    match (index, value) {
        (Some(index), Some(value)) => lines[index] = format!("{key}: {value}"),
        (Some(index), None) => {
            lines.remove(index);
        }
        (None, Some(value)) => lines.push(format!("{key}: {value}")),
        (None, None) => {}
    }
    if lines.is_empty() {
        return Ok(String::new());
    }
    Ok(lines.join("\n") + "\n")
}

fn transform_goose_config(
    text: &str,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<String> {
    let text = goose_set_selection(text, "GOOSE_PROVIDER", provider)?;
    goose_set_selection(&text, "GOOSE_MODEL", model)
}

fn goose_provider_value(endpoint: &str, model: &str) -> Value {
    serde_json::json!({
        "name": GOOSE_PROVIDER_ID,
        "engine": "openai",
        "display_name": GOOSE_PROVIDER_NAME,
        "description": "Custom d-token local route provider",
        "api_key_env": "",
        "base_url": endpoint,
        "models": [{
            "name": model,
            "resolved_model": null,
            "context_limit": 128000,
            "input_token_cost": null,
            "output_token_cost": null,
            "currency": null,
            "supports_cache_control": null,
            "reasoning": false
        }],
        "headers": null,
        "timeout_seconds": null,
        "supports_streaming": true,
        "requires_auth": false,
        "catalog_provider_id": "openai",
        "base_path": null,
        "env_vars": null,
        "dynamic_models": false,
        "skip_canonical_filtering": false,
        "model_doc_link": null,
        "setup_steps": [],
        "fast_model": null,
        "preserves_thinking": true
    })
}

fn goose_provider_json(endpoint: &str, model: &str) -> Result<String> {
    Ok(serde_json::to_string_pretty(&goose_provider_value(endpoint, model))? + "\n")
}

fn ensure_owned_goose_provider_text(text: &str, endpoint: &str, model: &str) -> Result<()> {
    let actual: Value =
        serde_json::from_str(text).context("Goose custom provider file must be strict JSON")?;
    if actual != goose_provider_value(endpoint, model) {
        bail!("Goose custom provider file is not provably d-token-owned");
    }
    Ok(())
}

fn kilo_selection(model: &str) -> String {
    format!("{OPENCODE_PROVIDER_ID}/{model}")
}

fn kilo_provider_value(endpoint: &str, model: &str) -> Value {
    serde_json::json!({
        "npm": OPENCODE_PROVIDER_NPM,
        "name": OPENCODE_PROVIDER_NAME,
        "options": {
            "baseURL": endpoint
        },
        "models": {
            model: { "name": format!("d-token routed {model}") }
        }
    })
}

fn ensure_owned_kilo_provider(value: &Value, endpoint: &str, model: &str) -> Result<()> {
    if *value != kilo_provider_value(endpoint, model) {
        bail!("Kilo dtoken-local-route provider is not provably d-token-owned");
    }
    Ok(())
}

fn parse_cline_root(text: &str) -> Result<Map<String, Value>> {
    let root = if text.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str::<Map<String, Value>>(text)
            .context("Cline provider configuration must be a strict JSON object")?
    };
    if let Some(version) = root.get("version") {
        if version.as_i64() != Some(1) {
            bail!("Cline provider configuration schema version is unsupported");
        }
    } else if !root.is_empty() {
        bail!("Cline provider configuration is missing schema version 1");
    }
    if root
        .get("providers")
        .is_some_and(|value| !value.is_object())
    {
        bail!("Cline providers must be an object");
    }
    if root
        .get("lastUsedProvider")
        .is_some_and(|value| !value.is_string())
    {
        bail!("Cline lastUsedProvider must be a string");
    }
    Ok(root)
}

fn cline_current_provider(root: &Map<String, Value>) -> Result<Option<String>> {
    let provider = root
        .get("lastUsedProvider")
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("Cline lastUsedProvider must be a string")
        })
        .transpose()?;
    validate_goose_provider_id(provider.as_deref())?;
    Ok(provider)
}

fn cline_provider_value(endpoint: &str, model: &str) -> Value {
    serde_json::json!({
        "settings": {
            "provider": CLINE_PROVIDER_ID,
            "apiKey": CLINE_PUBLIC_API_KEY_MARKER,
            "model": model,
            "baseUrl": opencode_base_url(endpoint),
        },
        "tokenSource": "manual",
    })
}

fn ensure_owned_cline_provider(value: &Value, endpoint: &str, model: &str) -> Result<()> {
    if *value != cline_provider_value(endpoint, model) {
        bail!("Cline openai-compatible provider is not provably d-token-owned");
    }
    Ok(())
}

fn set_cline_provider(root: &mut Map<String, Value>, endpoint: &str, model: &str) -> Result<()> {
    root.entry("version").or_insert(Value::from(1));
    let providers = root
        .entry("providers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("Cline providers must be an object")?;
    providers.insert(
        CLINE_PROVIDER_ID.into(),
        cline_provider_value(endpoint, model),
    );
    root.insert(
        "lastUsedProvider".into(),
        Value::String(CLINE_PROVIDER_ID.into()),
    );
    Ok(())
}

fn remove_cline_provider(root: &mut Map<String, Value>) -> Result<()> {
    let providers = root
        .get_mut("providers")
        .and_then(Value::as_object_mut)
        .context("Cline providers are missing while d-token transport is managed")?;
    providers.remove(CLINE_PROVIDER_ID);
    if providers.is_empty() {
        root.remove("providers");
    }
    Ok(())
}

fn parse_openclaw_root(text: &str) -> Result<Map<String, Value>> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    json5::from_str::<Map<String, Value>>(text)
        .context("OpenClaw configuration must be a JSON5 object")
}

fn openclaw_primary(root: &Map<String, Value>) -> Result<Option<String>> {
    let Some(agents) = root.get("agents") else {
        return Ok(None);
    };
    let agents = agents
        .as_object()
        .context("OpenClaw agents must be an object")?;
    let Some(defaults) = agents.get("defaults") else {
        return Ok(None);
    };
    let defaults = defaults
        .as_object()
        .context("OpenClaw agents.defaults must be an object")?;
    let Some(model) = defaults.get("model") else {
        return Ok(None);
    };
    let model = model
        .as_object()
        .context("OpenClaw agents.defaults.model must be an object")?;
    let primary = model
        .get("primary")
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .context("OpenClaw agents.defaults.model.primary must be a string")
        })
        .transpose()?;
    if let Some(primary) = primary.as_deref() {
        validate_model(Some(primary))?;
    }
    Ok(primary)
}

fn set_openclaw_primary(root: &mut Map<String, Value>, primary: &str) -> Result<()> {
    let agents = root
        .entry("agents")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("OpenClaw agents must be an object")?;
    let defaults = agents
        .entry("defaults")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("OpenClaw agents.defaults must be an object")?;
    let model = defaults
        .entry("model")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("OpenClaw agents.defaults.model must be an object")?;
    model.insert("primary".into(), Value::String(primary.into()));
    Ok(())
}

fn restore_openclaw_primary(
    root: &mut Map<String, Value>,
    was_present: bool,
    previous: Option<String>,
) -> Result<()> {
    match (was_present, previous) {
        (true, Some(previous)) => set_openclaw_primary(root, &previous),
        (false, None) => {
            let Some(agents) = root.get_mut("agents").and_then(Value::as_object_mut) else {
                return Ok(());
            };
            let Some(defaults) = agents.get_mut("defaults").and_then(Value::as_object_mut) else {
                return Ok(());
            };
            let Some(model) = defaults.get_mut("model").and_then(Value::as_object_mut) else {
                return Ok(());
            };
            model.remove("primary");
            if model.is_empty() {
                defaults.remove("model");
            }
            if defaults.is_empty() {
                agents.remove("defaults");
            }
            if agents.is_empty() {
                root.remove("agents");
            }
            Ok(())
        }
        _ => bail!("OpenClaw transport manifest has an inconsistent previous primary model"),
    }
}

fn openclaw_selection(model: &str) -> String {
    format!("{OPENCLAW_PROVIDER_ID}/{model}")
}

fn openclaw_provider_value(endpoint: &str, model: &str) -> Value {
    serde_json::json!({
        "baseUrl": opencode_base_url(endpoint),
        "apiKey": "",
        "api": "openai-completions",
        "models": [{
            "id": model,
            "name": format!("{OPENCLAW_PROVIDER_NAME} · {model}"),
            "reasoning": false,
            "input": ["text"],
        }],
    })
}

fn openclaw_providers_mut(root: &mut Map<String, Value>) -> Result<&mut Map<String, Value>> {
    root.entry("models")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("OpenClaw models must be an object")?
        .entry("providers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("OpenClaw models.providers must be an object")
}

fn openclaw_provider<'a>(
    root: &'a Map<String, Value>,
    provider_id: &str,
) -> Result<Option<&'a Value>> {
    let Some(models) = root.get("models") else {
        return Ok(None);
    };
    let models = models
        .as_object()
        .context("OpenClaw models must be an object")?;
    let Some(providers) = models.get("providers") else {
        return Ok(None);
    };
    let providers = providers
        .as_object()
        .context("OpenClaw models.providers must be an object")?;
    Ok(providers.get(provider_id))
}

fn ensure_owned_openclaw_provider(value: &Value, endpoint: &str, model: &str) -> Result<()> {
    if *value != openclaw_provider_value(endpoint, model) {
        bail!("OpenClaw dtoken-local-route provider is not provably d-token-owned");
    }
    Ok(())
}

fn set_openclaw_provider(root: &mut Map<String, Value>, endpoint: &str, model: &str) -> Result<()> {
    openclaw_providers_mut(root)?.insert(
        OPENCLAW_PROVIDER_ID.into(),
        openclaw_provider_value(endpoint, model),
    );
    set_openclaw_primary(root, &openclaw_selection(model))
}

fn remove_openclaw_provider(root: &mut Map<String, Value>) -> Result<()> {
    let models = root
        .get_mut("models")
        .and_then(Value::as_object_mut)
        .context("OpenClaw models are missing while d-token transport is managed")?;
    let providers = models
        .get_mut("providers")
        .and_then(Value::as_object_mut)
        .context("OpenClaw models.providers are missing while d-token transport is managed")?;
    providers.remove(OPENCLAW_PROVIDER_ID);
    if providers.is_empty() {
        models.remove("providers");
    }
    if models.is_empty() {
        root.remove("models");
    }
    Ok(())
}

fn serialize_openclaw_root(root: &Map<String, Value>) -> Result<String> {
    Ok(json5::to_string(&Value::Object(root.clone()))? + "\n")
}

fn cline_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let Some(text) = optional_text(config_path)? else {
        return Ok(None);
    };
    let root = parse_cline_root(&text)?;
    let Some(manifest) = parse_cline_manifest(&cline_manifest_path(config_path)?)? else {
        return Ok(None);
    };
    let Some(provider) = root
        .get("providers")
        .and_then(Value::as_object)
        .and_then(|providers| providers.get(CLINE_PROVIDER_ID))
    else {
        return Ok(None);
    };
    ensure_owned_cline_provider(provider, &manifest.endpoint, &manifest.model)?;
    let enabled = cline_current_provider(&root)?.as_deref() == Some(CLINE_PROVIDER_ID);
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: CLINE_PROVIDER_ID.into(),
        endpoint: manifest.endpoint.clone(),
        model: Some(manifest.model.clone()),
        enabled,
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}\n{}",
            "cline", manifest.endpoint, manifest.model, enabled
        )),
    }))
}

fn openclaw_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let Some(text) = optional_text(config_path)? else {
        return Ok(None);
    };
    let root = parse_openclaw_root(&text)?;
    let Some(manifest) = parse_openclaw_manifest(&openclaw_manifest_path(config_path)?)? else {
        return Ok(None);
    };
    let Some(provider) = openclaw_provider(&root, OPENCLAW_PROVIDER_ID)? else {
        return Ok(None);
    };
    ensure_owned_openclaw_provider(provider, &manifest.endpoint, &manifest.model)?;
    let selection = openclaw_selection(&manifest.model);
    let enabled = openclaw_primary(&root)?.as_deref() == Some(selection.as_str());
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: OPENCLAW_PROVIDER_ID.into(),
        endpoint: manifest.endpoint.clone(),
        model: Some(manifest.model.clone()),
        enabled,
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}\n{}",
            "openclaw", manifest.endpoint, manifest.model, enabled
        )),
    }))
}

fn cline_can_clear(config_path: &Path) -> bool {
    cline_current_upstream_signature(config_path)
        .ok()
        .flatten()
        .is_some()
}

fn openclaw_can_clear(config_path: &Path) -> bool {
    openclaw_current_upstream_signature(config_path)
        .ok()
        .flatten()
        .is_some()
}

fn goose_can_clear(config_path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(config_path) else {
        return false;
    };
    let Ok(current) = goose_current_selection(&text) else {
        return false;
    };
    let Ok(Some(manifest)) =
        parse_goose_manifest(&goose_manifest_path(config_path).unwrap_or_default())
    else {
        return false;
    };
    current.provider.as_deref() == Some(manifest.provider_id.as_str())
        && current.model.as_deref() == Some(manifest.model.as_str())
        && optional_text(&goose_provider_path(config_path).unwrap_or_default())
            .ok()
            .flatten()
            .is_some_and(|provider| {
                ensure_owned_goose_provider_text(&provider, &manifest.endpoint, &manifest.model)
                    .is_ok()
            })
}

fn kilo_can_clear(config_path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(config_path) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let Ok(Some(manifest)) =
        parse_kilo_manifest(&kilo_manifest_path(config_path).unwrap_or_default())
    else {
        return false;
    };
    root.get("model").and_then(Value::as_str) == Some(kilo_selection(&manifest.model).as_str())
        && root
            .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
            .is_some_and(|provider| {
                ensure_owned_kilo_provider(provider, &manifest.endpoint, &manifest.model).is_ok()
            })
}

fn goose_has_owned_artifacts(
    selection: &PriorSelection,
    provider_text: Option<&str>,
    manifest_present: bool,
) -> bool {
    manifest_present
        || selection.provider.as_deref() == Some(GOOSE_PROVIDER_ID)
        || provider_text.is_some()
}

fn goose_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let config_text = match optional_text(config_path)? {
        Some(text) => text,
        None => return Ok(None),
    };
    let selection = goose_current_selection(&config_text)?;
    let manifest_path = goose_manifest_path(config_path)?;
    let Some(manifest) = parse_goose_manifest(&manifest_path)? else {
        return Ok(None);
    };
    let Some(provider_text) = optional_text(&goose_provider_path(config_path)?)? else {
        return Ok(None);
    };
    if selection.provider.as_deref() != Some(manifest.provider_id.as_str())
        || selection.model.as_deref() != Some(manifest.model.as_str())
    {
        return Ok(None);
    }
    ensure_owned_goose_provider_text(&provider_text, &manifest.endpoint, &manifest.model)?;
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: manifest.provider_id.clone(),
        endpoint: manifest.endpoint.clone(),
        model: Some(manifest.model.clone()),
        enabled: true,
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}\n{}",
            "goose", manifest.endpoint, manifest.model, true
        )),
    }))
}

fn kilo_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let text = match optional_text(config_path)? {
        Some(text) => text,
        None => return Ok(None),
    };
    let root: Value =
        serde_json::from_str(&text).context("Kilo configuration must be strict JSON")?;
    let Some(manifest) = parse_kilo_manifest(&kilo_manifest_path(config_path)?)? else {
        return Ok(None);
    };
    if root.get("model").and_then(Value::as_str) != Some(kilo_selection(&manifest.model).as_str()) {
        return Ok(None);
    }
    let Some(provider) = root.pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}")) else {
        return Ok(None);
    };
    ensure_owned_kilo_provider(provider, &manifest.endpoint, &manifest.model)?;
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: manifest.provider_id.clone(),
        endpoint: manifest.endpoint.clone(),
        model: Some(manifest.model.clone()),
        enabled: true,
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}\n{}",
            "kilo-code", manifest.endpoint, manifest.model, true
        )),
    }))
}

fn preview_cline_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = cline_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let existing_manifest = parse_cline_manifest(&manifest_path)?;
    let current_upstream = cline_current_upstream_signature(&request.target_path)?;
    let mut root = parse_cline_root(&config_before)?;
    let current_provider = cline_current_provider(&root)?;
    let existing_owned_provider = root
        .get("providers")
        .and_then(Value::as_object)
        .and_then(|providers| providers.get(CLINE_PROVIDER_ID))
        .cloned();

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Cline transport requires an explicit model")?;
            let (previous_provider_present, previous_provider) = if let Some(manifest) =
                existing_manifest.as_ref()
            {
                if current_upstream.is_none() {
                    bail!("Cline provider changed outside d-token; automatic update is unsafe");
                }
                (
                    manifest.previous_provider_present,
                    manifest.previous_provider.clone(),
                )
            } else {
                if existing_owned_provider.is_some()
                    || current_provider.as_deref() == Some(CLINE_PROVIDER_ID)
                {
                    bail!("Cline has an unowned openai-compatible provider; use rollback or Advanced setup");
                }
                (current_provider.is_some(), current_provider.clone())
            };
            set_cline_provider(&mut root, &request.endpoint, model)?;
            (
                serde_json::to_string_pretty(&Value::Object(root))? + "\n",
                manifest_json(&ClineTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "cline".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: CLINE_PROVIDER_ID.into(),
                    previous_provider_present,
                    previous_provider,
                })?,
            )
        }
        AgentTransportOperation::Clear => match existing_manifest.as_ref() {
            None => {
                if existing_owned_provider.is_some()
                    || current_provider.as_deref() == Some(CLINE_PROVIDER_ID)
                {
                    bail!("Cline has an unowned openai-compatible provider; automatic clear is unsafe");
                }
                (config_before.clone(), String::new())
            }
            Some(manifest) => {
                if current_upstream.is_none() {
                    bail!("Cline provider changed outside d-token; automatic clear is unsafe");
                }
                remove_cline_provider(&mut root)?;
                if current_provider.as_deref() == Some(CLINE_PROVIDER_ID) {
                    restore_json_scalar(
                        &mut root,
                        "lastUsedProvider",
                        manifest.previous_provider_present,
                        manifest.previous_provider.clone(),
                    )?;
                }
                (
                    serde_json::to_string_pretty(&Value::Object(root))? + "\n",
                    String::new(),
                )
            }
        },
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn preview_openclaw_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = openclaw_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let existing_manifest = parse_openclaw_manifest(&manifest_path)?;
    let current_upstream = openclaw_current_upstream_signature(&request.target_path)?;
    let mut root = parse_openclaw_root(&config_before)?;
    let current_primary = openclaw_primary(&root)?;
    let existing_owned_provider = openclaw_provider(&root, OPENCLAW_PROVIDER_ID)?.cloned();

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("OpenClaw transport requires an explicit model")?;
            let (previous_primary_present, previous_primary) = if let Some(manifest) =
                existing_manifest.as_ref()
            {
                if current_upstream.is_none() {
                    bail!("OpenClaw provider changed outside d-token; automatic update is unsafe");
                }
                (
                    manifest.previous_primary_present,
                    manifest.previous_primary.clone(),
                )
            } else {
                if existing_owned_provider.is_some()
                    || current_primary.as_deref().is_some_and(|primary| {
                        primary.starts_with(&format!("{OPENCLAW_PROVIDER_ID}/"))
                    })
                {
                    bail!("OpenClaw has an unowned dtoken-local-route provider; use rollback or Advanced setup");
                }
                (current_primary.is_some(), current_primary.clone())
            };
            set_openclaw_provider(&mut root, &request.endpoint, model)?;
            (
                serialize_openclaw_root(&root)?,
                manifest_json(&OpenClawTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "openclaw".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: OPENCLAW_PROVIDER_ID.into(),
                    previous_primary_present,
                    previous_primary,
                })?,
            )
        }
        AgentTransportOperation::Clear => match existing_manifest.as_ref() {
            None => {
                if existing_owned_provider.is_some()
                    || current_primary.as_deref().is_some_and(|primary| {
                        primary.starts_with(&format!("{OPENCLAW_PROVIDER_ID}/"))
                    })
                {
                    bail!("OpenClaw has an unowned dtoken-local-route provider; automatic clear is unsafe");
                }
                (config_before.clone(), String::new())
            }
            Some(manifest) => {
                if current_upstream.is_none() {
                    bail!("OpenClaw provider changed outside d-token; automatic clear is unsafe");
                }
                remove_openclaw_provider(&mut root)?;
                let selection = openclaw_selection(&manifest.model);
                if current_primary.as_deref() == Some(selection.as_str()) {
                    restore_openclaw_primary(
                        &mut root,
                        manifest.previous_primary_present,
                        manifest.previous_primary.clone(),
                    )?;
                }
                (serialize_openclaw_root(&root)?, String::new())
            }
        },
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn preview_goose_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let provider_path = goose_provider_path(&request.target_path)?;
    let manifest_path = goose_manifest_path(&request.target_path)?;
    let provider_before = optional_text(&provider_path)?.unwrap_or_default();
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let selection = goose_current_selection(&config_before)?;
    let existing_manifest = parse_goose_manifest(&manifest_path)?;
    let current_upstream = goose_current_upstream_signature(&request.target_path)?;

    let (config_after, provider_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Goose transport requires an explicit model")?;
            if let Some(manifest) = existing_manifest.as_ref() {
                if current_upstream.is_none() {
                    bail!("Goose selection changed outside d-token; automatic update is unsafe");
                }
                (
                    transform_goose_config(&config_before, Some(GOOSE_PROVIDER_ID), Some(model))?,
                    goose_provider_json(&request.endpoint, model)?,
                    manifest_json(&GooseTransportManifest {
                        schema_version: TRANSPORT_SCHEMA_VERSION,
                        managed_by: DTOKEN_MANAGED_BY.into(),
                        agent_id: "goose".into(),
                        transport: DTOKEN_TRANSPORT.into(),
                        endpoint: request.endpoint.clone(),
                        model: model.to_string(),
                        provider_id: GOOSE_PROVIDER_ID.into(),
                        previous_provider_present: manifest.previous_provider_present,
                        previous_provider: manifest.previous_provider.clone(),
                        previous_model_present: manifest.previous_model_present,
                        previous_model: manifest.previous_model.clone(),
                    })?,
                )
            } else {
                if goose_has_owned_artifacts(
                    &selection,
                    (!provider_before.is_empty()).then_some(provider_before.as_str()),
                    !manifest_before.is_empty(),
                ) {
                    bail!("Goose has an unowned d-token route artifact; use rollback or Advanced setup");
                }
                (
                    transform_goose_config(&config_before, Some(GOOSE_PROVIDER_ID), Some(model))?,
                    goose_provider_json(&request.endpoint, model)?,
                    manifest_json(&GooseTransportManifest {
                        schema_version: TRANSPORT_SCHEMA_VERSION,
                        managed_by: DTOKEN_MANAGED_BY.into(),
                        agent_id: "goose".into(),
                        transport: DTOKEN_TRANSPORT.into(),
                        endpoint: request.endpoint.clone(),
                        model: model.to_string(),
                        provider_id: GOOSE_PROVIDER_ID.into(),
                        previous_provider_present: selection.provider_present,
                        previous_provider: selection.provider.clone(),
                        previous_model_present: selection.model_present,
                        previous_model: selection.model.clone(),
                    })?,
                )
            }
        }
        AgentTransportOperation::Clear => match existing_manifest.as_ref() {
            None => {
                if goose_has_owned_artifacts(
                    &selection,
                    (!provider_before.is_empty()).then_some(provider_before.as_str()),
                    false,
                ) {
                    bail!("Goose has an unowned d-token route artifact; automatic clear is unsafe");
                }
                (
                    config_before.clone(),
                    provider_before.clone(),
                    String::new(),
                )
            }
            Some(manifest) => {
                if current_upstream.is_some() {
                    (
                        transform_goose_config(
                            &config_before,
                            manifest.previous_provider.as_deref(),
                            manifest.previous_model.as_deref(),
                        )?,
                        String::new(),
                        String::new(),
                    )
                } else if !goose_has_owned_artifacts(&selection, None, true) {
                    (config_before.clone(), String::new(), String::new())
                } else {
                    bail!("Goose selection changed outside d-token; automatic clear is unsafe");
                }
            }
        },
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "provider",
                    crate::stable_hash(&provider_before),
                    crate::stable_hash(&provider_after),
                ),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after
            || provider_before != provider_after
            || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("provider", &provider_before, &provider_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn preview_kilo_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = kilo_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let current_upstream = kilo_current_upstream_signature(&request.target_path)?;
    let existing_manifest = parse_kilo_manifest(&manifest_path)?;
    let mut root: Map<String, Value> = if config_before.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(&config_before).context("Kilo configuration must be strict JSON")?
    };

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Kilo transport requires an explicit model")?;
            if let Some(manifest) = existing_manifest.as_ref() {
                if current_upstream.is_none() {
                    bail!("Kilo selection changed outside d-token; automatic update is unsafe");
                }
                ensure_kilo_root_provider_state(&root, &manifest.endpoint, &manifest.model)?;
                set_kilo_provider(&mut root, &request.endpoint, model)?;
                root.insert("model".into(), Value::String(kilo_selection(model)));
                (
                    serde_json::to_string_pretty(&Value::Object(root.clone()))? + "\n",
                    manifest_json(&KiloTransportManifest {
                        schema_version: TRANSPORT_SCHEMA_VERSION,
                        managed_by: DTOKEN_MANAGED_BY.into(),
                        agent_id: "kilo-code".into(),
                        transport: DTOKEN_TRANSPORT.into(),
                        endpoint: request.endpoint.clone(),
                        model: model.to_string(),
                        provider_id: OPENCODE_PROVIDER_ID.into(),
                        previous_model_present: manifest.previous_model_present,
                        previous_model: manifest.previous_model.clone(),
                    })?,
                )
            } else {
                let prior_model = root.get("model").and_then(Value::as_str).map(str::to_owned);
                if Value::Object(root.clone())
                    .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
                    .is_some()
                {
                    bail!("Kilo has an unowned dtoken-local-route provider; use rollback or Advanced setup");
                }
                set_kilo_provider(&mut root, &request.endpoint, model)?;
                root.insert("model".into(), Value::String(kilo_selection(model)));
                (
                    serde_json::to_string_pretty(&Value::Object(root.clone()))? + "\n",
                    manifest_json(&KiloTransportManifest {
                        schema_version: TRANSPORT_SCHEMA_VERSION,
                        managed_by: DTOKEN_MANAGED_BY.into(),
                        agent_id: "kilo-code".into(),
                        transport: DTOKEN_TRANSPORT.into(),
                        endpoint: request.endpoint.clone(),
                        model: model.to_string(),
                        provider_id: OPENCODE_PROVIDER_ID.into(),
                        previous_model_present: prior_model.is_some(),
                        previous_model: prior_model,
                    })?,
                )
            }
        }
        AgentTransportOperation::Clear => match existing_manifest.as_ref() {
            None => {
                if Value::Object(root.clone())
                    .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
                    .is_some()
                {
                    bail!("Kilo has an unowned dtoken-local-route provider; automatic clear is unsafe");
                }
                (config_before.clone(), String::new())
            }
            Some(manifest) => {
                if current_upstream.is_some() {
                    remove_kilo_provider(&mut root)?;
                    restore_json_scalar(
                        &mut root,
                        "model",
                        manifest.previous_model_present,
                        manifest.previous_model.clone(),
                    )?;
                    (
                        serde_json::to_string_pretty(&Value::Object(root.clone()))? + "\n",
                        String::new(),
                    )
                } else if Value::Object(root.clone())
                    .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
                    .is_none()
                {
                    (config_before.clone(), String::new())
                } else {
                    bail!("Kilo selection changed outside d-token; automatic clear is unsafe");
                }
            }
        },
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn set_kilo_provider(root: &mut Map<String, Value>, endpoint: &str, model: &str) -> Result<()> {
    let providers = root
        .entry("provider")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("Kilo provider must be an object")?;
    if let Some(existing) = providers.get(OPENCODE_PROVIDER_ID) {
        ensure_owned_kilo_provider(existing, endpoint, model)?;
    }
    providers.insert(
        OPENCODE_PROVIDER_ID.into(),
        kilo_provider_value(endpoint, model),
    );
    Ok(())
}

fn remove_kilo_provider(root: &mut Map<String, Value>) -> Result<()> {
    let providers = root
        .get_mut("provider")
        .and_then(Value::as_object_mut)
        .context("Kilo provider is missing while d-token transport is active")?;
    providers.remove(OPENCODE_PROVIDER_ID);
    if providers.is_empty() {
        root.remove("provider");
    }
    Ok(())
}

fn restore_json_scalar(
    root: &mut Map<String, Value>,
    key: &str,
    was_present: bool,
    previous: Option<String>,
) -> Result<()> {
    match (was_present, previous) {
        (true, Some(value)) => {
            root.insert(key.into(), Value::String(value));
        }
        (false, None) => {
            root.remove(key);
        }
        _ => bail!("stored Agent transport manifest is inconsistent; automatic clear is unsafe"),
    }
    Ok(())
}

fn ensure_kilo_root_provider_state(
    root: &Map<String, Value>,
    endpoint: &str,
    model: &str,
) -> Result<()> {
    let current_model = root
        .get("model")
        .and_then(Value::as_str)
        .context("Kilo model is missing while d-token transport is active")?;
    if current_model != kilo_selection(model) {
        bail!("Kilo selection changed outside d-token; automatic clear is unsafe");
    }
    let root_value = Value::Object(root.clone());
    let provider = root_value
        .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
        .context("Kilo dtoken-local-route provider is absent")?;
    ensure_owned_kilo_provider(provider, endpoint, model)
}

fn apply_cline_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let review = preview_cline_transport(request, operation)?;
    if !review.changed {
        let verification = if operation == AgentTransportOperation::Clear {
            verify_cline_cleared(&request.target_path)?
        } else {
            verify_cline_transport(request)?
        };
        return Ok(AgentTransportReceipt {
            operation,
            agent_id: request.agent_id.clone(),
            target_path: request.target_path.clone(),
            before_sha256: review.before_sha256,
            after_sha256: review.after_sha256,
            changed: false,
            transaction: None,
            verification,
        });
    }
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = cline_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let existing_manifest = parse_cline_manifest(&manifest_path)?;
    let current_upstream = cline_current_upstream_signature(&request.target_path)?;
    let mut root = parse_cline_root(&config_before)?;
    let current_provider = cline_current_provider(&root)?;

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Cline transport requires an explicit model")?;
            let (previous_provider_present, previous_provider) = if let Some(manifest) =
                existing_manifest.as_ref()
            {
                if current_upstream.is_none() {
                    bail!("Cline provider changed outside d-token; automatic update is unsafe");
                }
                (
                    manifest.previous_provider_present,
                    manifest.previous_provider.clone(),
                )
            } else {
                if root
                    .get("providers")
                    .and_then(Value::as_object)
                    .is_some_and(|providers| providers.contains_key(CLINE_PROVIDER_ID))
                    || current_provider.as_deref() == Some(CLINE_PROVIDER_ID)
                {
                    bail!("Cline has an unowned openai-compatible provider; use rollback or Advanced setup");
                }
                (current_provider.is_some(), current_provider.clone())
            };
            set_cline_provider(&mut root, &request.endpoint, model)?;
            (
                serde_json::to_string_pretty(&Value::Object(root))? + "\n",
                manifest_json(&ClineTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "cline".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: CLINE_PROVIDER_ID.into(),
                    previous_provider_present,
                    previous_provider,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let manifest = existing_manifest
                .as_ref()
                .context("Cline transport manifest is required for clear")?;
            if current_upstream.is_none() {
                bail!("Cline provider changed outside d-token; automatic clear is unsafe");
            }
            remove_cline_provider(&mut root)?;
            if current_provider.as_deref() == Some(CLINE_PROVIDER_ID) {
                restore_json_scalar(
                    &mut root,
                    "lastUsedProvider",
                    manifest.previous_provider_present,
                    manifest.previous_provider.clone(),
                )?;
            }
            (
                serde_json::to_string_pretty(&Value::Object(root))? + "\n",
                String::new(),
            )
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions = Vec::new();
    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if manifest_before != manifest_after {
            changes.push(patcher.prepare_change(
                &manifest_path,
                &manifest_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
    } else {
        if config_before != config_after {
            let change = patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?;
            transactions.push(patcher.apply_transaction(&[change])?);
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for transaction in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&transaction.changes, backup_root);
            }
            return Err(error);
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_cline_cleared(&request.target_path)?
    } else {
        verify_cline_transport(request)?
    };
    if !verification.ok {
        for transaction in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&transaction.changes, backup_root);
        }
        if operation == AgentTransportOperation::Clear && !manifest_before.is_empty() {
            let _ = apply_manifest_change(
                &manifest_path,
                &manifest_before,
                &request.agent_id,
                backup_root,
            );
        }
        bail!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        );
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: review.before_sha256,
        after_sha256: review.after_sha256,
        changed: true,
        transaction: merge_transactions(transactions),
        verification,
    })
}

fn apply_openclaw_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let review = preview_openclaw_transport(request, operation)?;
    if !review.changed {
        let verification = if operation == AgentTransportOperation::Clear {
            verify_openclaw_cleared(&request.target_path)?
        } else {
            verify_openclaw_transport(request)?
        };
        return Ok(AgentTransportReceipt {
            operation,
            agent_id: request.agent_id.clone(),
            target_path: request.target_path.clone(),
            before_sha256: review.before_sha256,
            after_sha256: review.after_sha256,
            changed: false,
            transaction: None,
            verification,
        });
    }
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = openclaw_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let existing_manifest = parse_openclaw_manifest(&manifest_path)?;
    let current_upstream = openclaw_current_upstream_signature(&request.target_path)?;
    let mut root = parse_openclaw_root(&config_before)?;
    let current_primary = openclaw_primary(&root)?;

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("OpenClaw transport requires an explicit model")?;
            let (previous_primary_present, previous_primary) = if let Some(manifest) =
                existing_manifest.as_ref()
            {
                if current_upstream.is_none() {
                    bail!("OpenClaw provider changed outside d-token; automatic update is unsafe");
                }
                (
                    manifest.previous_primary_present,
                    manifest.previous_primary.clone(),
                )
            } else {
                if openclaw_provider(&root, OPENCLAW_PROVIDER_ID)?.is_some()
                    || current_primary.as_deref().is_some_and(|primary| {
                        primary.starts_with(&format!("{OPENCLAW_PROVIDER_ID}/"))
                    })
                {
                    bail!("OpenClaw has an unowned dtoken-local-route provider; use rollback or Advanced setup");
                }
                (current_primary.is_some(), current_primary.clone())
            };
            set_openclaw_provider(&mut root, &request.endpoint, model)?;
            (
                serialize_openclaw_root(&root)?,
                manifest_json(&OpenClawTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "openclaw".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: OPENCLAW_PROVIDER_ID.into(),
                    previous_primary_present,
                    previous_primary,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let manifest = existing_manifest
                .as_ref()
                .context("OpenClaw transport manifest is required for clear")?;
            if current_upstream.is_none() {
                bail!("OpenClaw provider changed outside d-token; automatic clear is unsafe");
            }
            remove_openclaw_provider(&mut root)?;
            let selection = openclaw_selection(&manifest.model);
            if current_primary.as_deref() == Some(selection.as_str()) {
                restore_openclaw_primary(
                    &mut root,
                    manifest.previous_primary_present,
                    manifest.previous_primary.clone(),
                )?;
            }
            (serialize_openclaw_root(&root)?, String::new())
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions = Vec::new();
    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_json5_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if manifest_before != manifest_after {
            changes.push(patcher.prepare_change(
                &manifest_path,
                &manifest_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
    } else {
        if config_before != config_after {
            let change = patcher.prepare_json5_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?;
            transactions.push(patcher.apply_transaction(&[change])?);
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for transaction in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&transaction.changes, backup_root);
            }
            return Err(error);
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_openclaw_cleared(&request.target_path)?
    } else {
        verify_openclaw_transport(request)?
    };
    if !verification.ok {
        for transaction in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&transaction.changes, backup_root);
        }
        if operation == AgentTransportOperation::Clear && !manifest_before.is_empty() {
            let _ = apply_manifest_change(
                &manifest_path,
                &manifest_before,
                &request.agent_id,
                backup_root,
            );
        }
        bail!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        );
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: review.before_sha256,
        after_sha256: review.after_sha256,
        changed: true,
        transaction: merge_transactions(transactions),
        verification,
    })
}

fn apply_goose_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let review = preview_goose_transport(request, operation)?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let provider_path = goose_provider_path(&request.target_path)?;
    let manifest_path = goose_manifest_path(&request.target_path)?;
    let provider_before = optional_text(&provider_path)?.unwrap_or_default();
    let existing_manifest = parse_goose_manifest(&manifest_path)?;

    let (config_after, provider_after, manifest_after, changed) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Goose transport requires an explicit model")?;
            let selection = goose_current_selection(&config_before)?;
            let manifest = existing_manifest.as_ref();
            let current_upstream = goose_current_upstream_signature(&request.target_path)?;
            let prior_provider_present = manifest
                .map(|item| item.previous_provider_present)
                .unwrap_or(selection.provider_present);
            let prior_provider = manifest
                .and_then(|item| item.previous_provider.clone())
                .or_else(|| selection.provider.clone());
            let prior_model_present = manifest
                .map(|item| item.previous_model_present)
                .unwrap_or(selection.model_present);
            let prior_model = manifest
                .and_then(|item| item.previous_model.clone())
                .or_else(|| selection.model.clone());
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("Goose selection changed outside d-token; automatic update is unsafe");
            }
            (
                transform_goose_config(&config_before, Some(GOOSE_PROVIDER_ID), Some(model))?,
                goose_provider_json(&request.endpoint, model)?,
                manifest_json(&GooseTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "goose".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: GOOSE_PROVIDER_ID.into(),
                    previous_provider_present: prior_provider_present,
                    previous_provider: prior_provider,
                    previous_model_present: prior_model_present,
                    previous_model: prior_model,
                })?,
                review.changed,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: review.before_sha256,
                    after_sha256: review.after_sha256,
                    changed: false,
                    transaction: None,
                    verification: verify_goose_cleared(&request.target_path)?,
                });
            };
            (
                transform_goose_config(
                    &config_before,
                    manifest.previous_provider.as_deref(),
                    manifest.previous_model.as_deref(),
                )?,
                String::new(),
                String::new(),
                review.changed,
            )
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions = Vec::new();
    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if provider_before != provider_after {
            changes.push(patcher.prepare_change(
                &provider_path,
                &provider_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else {
        if config_before != config_after {
            let change = patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?;
            transactions.push(patcher.apply_transaction(&[change])?);
        }
        if !provider_before.is_empty() {
            ensure_owned_goose_provider_text(
                &provider_before,
                &existing_manifest
                    .as_ref()
                    .context("Goose manifest is required for clear")?
                    .endpoint,
                &existing_manifest
                    .as_ref()
                    .context("Goose manifest is required for clear")?
                    .model,
            )?;
            if let Err(error) = remove_owned_file(&provider_path) {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for tx in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&tx.changes, backup_root);
            }
            return Err(error);
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_goose_cleared(&request.target_path)?
    } else {
        verify_goose_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        bail!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        );
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: review.before_sha256,
        after_sha256: review.after_sha256,
        changed,
        transaction: merge_transactions(transactions),
        verification,
    })
}

fn apply_kilo_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let review = preview_kilo_transport(request, operation)?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = kilo_manifest_path(&request.target_path)?;
    let existing_manifest = parse_kilo_manifest(&manifest_path)?;
    let mut root: Map<String, Value> = if config_before.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(&config_before).context("Kilo configuration must be strict JSON")?
    };

    let (config_after, manifest_after, changed) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Kilo transport requires an explicit model")?;
            let current_upstream = kilo_current_upstream_signature(&request.target_path)?;
            let prior_model = existing_manifest
                .as_ref()
                .and_then(|manifest| manifest.previous_model.clone())
                .or_else(|| root.get("model").and_then(Value::as_str).map(str::to_owned));
            let prior_model_present = existing_manifest
                .as_ref()
                .map(|manifest| manifest.previous_model_present)
                .unwrap_or_else(|| root.get("model").is_some());
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("Kilo selection changed outside d-token; automatic update is unsafe");
            }
            set_kilo_provider(&mut root, &request.endpoint, model)?;
            root.insert("model".into(), Value::String(kilo_selection(model)));
            (
                serde_json::to_string_pretty(&Value::Object(root))? + "\n",
                manifest_json(&KiloTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "kilo-code".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: OPENCODE_PROVIDER_ID.into(),
                    previous_model_present: prior_model_present,
                    previous_model: prior_model,
                })?,
                review.changed,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: review.before_sha256,
                    after_sha256: review.after_sha256,
                    changed: false,
                    transaction: None,
                    verification: verify_kilo_cleared(&request.target_path)?,
                });
            };
            remove_kilo_provider(&mut root)?;
            restore_json_scalar(
                &mut root,
                "model",
                manifest.previous_model_present,
                manifest.previous_model.clone(),
            )?;
            (
                serde_json::to_string_pretty(&Value::Object(root))? + "\n",
                String::new(),
                review.changed,
            )
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions = Vec::new();
    if config_before != config_after {
        let change = patcher.prepare_change(
            &request.target_path,
            &config_after,
            ConfigToolKind::Agent,
            Some(request.agent_id.clone()),
            RiskLevel::Medium,
        )?;
        transactions.push(patcher.apply_transaction(&[change])?);
    }

    if operation == AgentTransportOperation::Configure {
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else if let Err(error) = remove_owned_file(&manifest_path) {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        return Err(error);
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_kilo_cleared(&request.target_path)?
    } else {
        verify_kilo_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        bail!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        );
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: review.before_sha256,
        after_sha256: review.after_sha256,
        changed,
        transaction: merge_transactions(transactions),
        verification,
    })
}

fn verify_cline_transport(request: &AgentTransportRequest) -> Result<AgentTransportVerification> {
    let text = match optional_text(&request.target_path)? {
        Some(text) => text,
        None => {
            return Ok(AgentTransportVerification {
                agent_id: request.agent_id.clone(),
                kind: AgentTransportAdapterKind::ClineCli,
                ok: false,
                detail: "Cline provider configuration is missing".into(),
                current_upstream: None,
            })
        }
    };
    let result: Result<()> = (|| {
        // A mismatched Cline schema (for example a non-object document) must
        // fail closed into a verification result, never propagate as an error.
        let root = parse_cline_root(&text)?;
        let Some(manifest) = parse_cline_manifest(&cline_manifest_path(&request.target_path)?)?
        else {
            bail!("Cline transport manifest is absent");
        };
        if manifest.endpoint != request.endpoint
            || request.model.as_deref() != Some(manifest.model.as_str())
        {
            bail!("Cline transport manifest does not match the reviewed request");
        }
        if cline_current_provider(&root)?.as_deref() != Some(CLINE_PROVIDER_ID) {
            bail!("Cline selection does not point at the d-token-owned provider");
        }
        let provider = root
            .get("providers")
            .and_then(Value::as_object)
            .and_then(|providers| providers.get(CLINE_PROVIDER_ID))
            .context("Cline d-token-owned provider is absent")?;
        ensure_owned_cline_provider(provider, &manifest.endpoint, &manifest.model)
    })();
    let current_upstream = if result.is_ok() {
        cline_current_upstream_signature(&request.target_path)?
    } else {
        None
    };
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::ClineCli,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed transport verified".into()),
        current_upstream,
    })
}

fn verify_cline_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if let Some(text) = optional_text(config_path)? {
            let root = parse_cline_root(&text)?;
            if root
                .get("providers")
                .and_then(Value::as_object)
                .is_some_and(|providers| providers.contains_key(CLINE_PROVIDER_ID))
            {
                bail!("d-token Cline provider is still present");
            }
            if cline_current_provider(&root)?.as_deref() == Some(CLINE_PROVIDER_ID) {
                bail!("Cline still selects the removed d-token provider");
            }
        }
        if optional_text(&cline_manifest_path(config_path)?)?.is_some() {
            bail!("d-token Cline transport manifest is still present");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "cline".into(),
        kind: AgentTransportAdapterKind::ClineCli,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed transport is clear".into()),
        current_upstream: None,
    })
}

fn verify_openclaw_transport(
    request: &AgentTransportRequest,
) -> Result<AgentTransportVerification> {
    let text = match optional_text(&request.target_path)? {
        Some(text) => text,
        None => {
            return Ok(AgentTransportVerification {
                agent_id: request.agent_id.clone(),
                kind: AgentTransportAdapterKind::OpenClaw,
                ok: false,
                detail: "OpenClaw configuration is missing".into(),
                current_upstream: None,
            })
        }
    };
    let root = parse_openclaw_root(&text)?;
    let Some(manifest) = parse_openclaw_manifest(&openclaw_manifest_path(&request.target_path)?)?
    else {
        return Ok(AgentTransportVerification {
            agent_id: request.agent_id.clone(),
            kind: AgentTransportAdapterKind::OpenClaw,
            ok: false,
            detail: "OpenClaw transport manifest is absent".into(),
            current_upstream: None,
        });
    };
    let result: Result<()> = (|| {
        if manifest.endpoint != request.endpoint
            || request.model.as_deref() != Some(manifest.model.as_str())
        {
            bail!("OpenClaw transport manifest does not match the reviewed request");
        }
        let selection = openclaw_selection(&manifest.model);
        if openclaw_primary(&root)?.as_deref() != Some(selection.as_str()) {
            bail!("OpenClaw primary model does not point at the d-token provider");
        }
        let provider = openclaw_provider(&root, OPENCLAW_PROVIDER_ID)?
            .context("OpenClaw dtoken-local-route provider is absent")?;
        ensure_owned_openclaw_provider(provider, &manifest.endpoint, &manifest.model)
    })();
    let current_upstream = if result.is_ok() {
        openclaw_current_upstream_signature(&request.target_path)?
    } else {
        None
    };
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::OpenClaw,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed transport verified".into()),
        current_upstream,
    })
}

fn verify_openclaw_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if let Some(text) = optional_text(config_path)? {
            let root = parse_openclaw_root(&text)?;
            if openclaw_provider(&root, OPENCLAW_PROVIDER_ID)?.is_some() {
                bail!("d-token OpenClaw provider is still present");
            }
            if openclaw_primary(&root)?
                .as_deref()
                .is_some_and(|primary| primary.starts_with(&format!("{OPENCLAW_PROVIDER_ID}/")))
            {
                bail!("OpenClaw still selects the removed d-token provider");
            }
        }
        if optional_text(&openclaw_manifest_path(config_path)?)?.is_some() {
            bail!("d-token OpenClaw transport manifest is still present");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "openclaw".into(),
        kind: AgentTransportAdapterKind::OpenClaw,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed transport is clear".into()),
        current_upstream: None,
    })
}

fn verify_goose_transport(request: &AgentTransportRequest) -> Result<AgentTransportVerification> {
    let config_text = match optional_text(&request.target_path)? {
        Some(text) => text,
        None => {
            return Ok(AgentTransportVerification {
                agent_id: request.agent_id.clone(),
                kind: AgentTransportAdapterKind::Goose,
                ok: false,
                detail: "configuration file is missing".into(),
                current_upstream: None,
            })
        }
    };
    let current = goose_current_selection(&config_text)?;
    let Some(manifest) = parse_goose_manifest(&goose_manifest_path(&request.target_path)?)? else {
        return Ok(AgentTransportVerification {
            agent_id: request.agent_id.clone(),
            kind: AgentTransportAdapterKind::Goose,
            ok: false,
            detail: "Goose transport manifest is absent".into(),
            current_upstream: None,
        });
    };
    let Some(provider_text) = optional_text(&goose_provider_path(&request.target_path)?)? else {
        return Ok(AgentTransportVerification {
            agent_id: request.agent_id.clone(),
            kind: AgentTransportAdapterKind::Goose,
            ok: false,
            detail: "Goose custom provider file is absent".into(),
            current_upstream: None,
        });
    };
    let result: Result<()> = (|| {
        if manifest.endpoint != request.endpoint
            || request.model.as_deref() != Some(manifest.model.as_str())
        {
            bail!("Goose transport manifest does not match the reviewed request");
        }
        if current.provider.as_deref() != Some(GOOSE_PROVIDER_ID)
            || current.model.as_deref() != Some(manifest.model.as_str())
        {
            bail!("Goose selection does not point at the d-token provider");
        }
        ensure_owned_goose_provider_text(&provider_text, &manifest.endpoint, &manifest.model)
    })();
    let current_upstream = if result.is_ok() {
        goose_current_upstream_signature(&request.target_path)?
    } else {
        None
    };
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::Goose,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed transport verified".into()),
        current_upstream,
    })
}

fn verify_goose_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if let Some(text) = optional_text(config_path)? {
            let selection = goose_current_selection(&text)?;
            if selection.provider.as_deref() == Some(GOOSE_PROVIDER_ID) {
                bail!("d-token Goose transport is still selected");
            }
        }
        if optional_text(&goose_provider_path(config_path)?)?.is_some() {
            bail!("d-token Goose custom provider file is still present");
        }
        if optional_text(&goose_manifest_path(config_path)?)?.is_some() {
            bail!("d-token Goose transport manifest is still present");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "goose".into(),
        kind: AgentTransportAdapterKind::Goose,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed transport is clear".into()),
        current_upstream: None,
    })
}

fn verify_kilo_transport(request: &AgentTransportRequest) -> Result<AgentTransportVerification> {
    let text = match optional_text(&request.target_path)? {
        Some(text) => text,
        None => {
            return Ok(AgentTransportVerification {
                agent_id: request.agent_id.clone(),
                kind: AgentTransportAdapterKind::KiloCode,
                ok: false,
                detail: "configuration file is missing".into(),
                current_upstream: None,
            })
        }
    };
    let root: Value =
        serde_json::from_str(&text).context("Kilo configuration must be strict JSON")?;
    let Some(manifest) = parse_kilo_manifest(&kilo_manifest_path(&request.target_path)?)? else {
        return Ok(AgentTransportVerification {
            agent_id: request.agent_id.clone(),
            kind: AgentTransportAdapterKind::KiloCode,
            ok: false,
            detail: "Kilo transport manifest is absent".into(),
            current_upstream: None,
        });
    };
    let result: Result<()> = (|| {
        if manifest.endpoint != request.endpoint
            || request.model.as_deref() != Some(manifest.model.as_str())
        {
            bail!("Kilo transport manifest does not match the reviewed request");
        }
        if root.get("model").and_then(Value::as_str)
            != Some(kilo_selection(&manifest.model).as_str())
        {
            bail!("Kilo selection does not point at the d-token provider");
        }
        let provider = root
            .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
            .context("Kilo dtoken-local-route provider is absent")?;
        ensure_owned_kilo_provider(provider, &manifest.endpoint, &manifest.model)
    })();
    let current_upstream = if result.is_ok() {
        kilo_current_upstream_signature(&request.target_path)?
    } else {
        None
    };
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::KiloCode,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed transport verified".into()),
        current_upstream,
    })
}

fn verify_kilo_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if let Some(text) = optional_text(config_path)? {
            let root: Value =
                serde_json::from_str(&text).context("Kilo configuration must be strict JSON")?;
            if root
                .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
                .is_some()
            {
                bail!("d-token Kilo provider is still present");
            }
        }
        if optional_text(&kilo_manifest_path(config_path)?)?.is_some() {
            bail!("d-token Kilo transport manifest is still present");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "kilo-code".into(),
        kind: AgentTransportAdapterKind::KiloCode,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed transport is clear".into()),
        current_upstream: None,
    })
}

/// MCP agents are deliberately fail-closed in the transport adapter: the
/// d-token MCP entry is not a fabricated `d-token --mcp-stdio` shape (no such
/// executable exists), and the real metadata-only `dtoken-agent-bridge mcp
/// stdio` entry requires a desktop-resolved absolute executable path plus
/// managed launch context that the transport request does not carry. The
/// desktop-managed flow owns the verified write path.
const MCP_TRANSPORT_FAIL_CLOSED: &str =
    "Agent MCP configuration is fail-closed in the transport adapter: the d-token MCP entry requires the desktop-resolved metadata-only bridge executable and managed launch context";

/// Fail-closed reason for agents whose MCP config schema is verified (JSON
/// `mcpServers` in a known file) but whose d-token MCP entry cannot be built
/// truthfully by the transport adapter: no `d-token` executable exists, and
/// the real metadata-only `dtoken-agent-bridge mcp stdio` entry needs the
/// desktop-resolved absolute executable path plus managed launch context.
/// Automatic configure would fabricate a broken entry.
const MCP_VERIFIED_SCHEMA_NO_COMMAND: &str = "The agent exposes a verified MCP config schema, but the transport adapter cannot write a runnable d-token MCP entry: no `d-token` executable exists, and the metadata-only `dtoken-agent-bridge mcp stdio` entry requires the desktop-resolved absolute executable path and managed launch context. Automatic configure would fabricate a broken entry; the desktop-managed flow is the verified write path";

const TRANSPORT_SCHEMA_VERSION: i64 = 1;
const CLAUDE_ENV: &str = "env";
const CLAUDE_BASE_URL: &str = "ANTHROPIC_BASE_URL";
const CLAUDE_MODEL: &str = "ANTHROPIC_MODEL";
const CODEX_PROVIDER_ID: &str = "dtoken";

#[derive(Clone)]
struct PriorSelection {
    provider_present: bool,
    provider: Option<String>,
    model_present: bool,
    model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct GooseTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_provider_present: bool,
    previous_provider: Option<String>,
    previous_model_present: bool,
    previous_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct KiloTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_model_present: bool,
    previous_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ClineTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_provider_present: bool,
    previous_provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct OpenClawTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_primary_present: bool,
    previous_primary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct GeminiTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_auth_type_present: bool,
    previous_auth_type: Option<String>,
    previous_base_url_present: bool,
    previous_base_url: Option<String>,
    previous_model_present: bool,
    previous_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RooTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_api_provider_present: bool,
    previous_api_provider: Option<String>,
    previous_base_url_present: bool,
    previous_base_url: Option<String>,
    previous_model_id_present: bool,
    previous_model_id: Option<String>,
    previous_config_name_present: bool,
    previous_config_name: Option<String>,
    previous_list_present: bool,
    previous_list: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ContinueTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_models_present: bool,
    previous_models: Option<Vec<Value>>,
    previous_default_model_present: bool,
    previous_default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AiderTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_model_present: bool,
    previous_model: Option<String>,
    previous_base_present: bool,
    previous_base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HermesTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_base_url_present: bool,
    previous_base_url: Option<String>,
    previous_model_present: bool,
    previous_model: Option<String>,
    previous_default_present: bool,
    previous_default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct OpenHandsTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_llm_model_present: bool,
    previous_llm_model: Option<String>,
    previous_llm_base_url_present: bool,
    previous_llm_base_url: Option<String>,
    previous_llm_api_key_present: bool,
    previous_llm_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PiTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_default_provider_present: bool,
    previous_default_provider: Option<String>,
    previous_default_model_present: bool,
    previous_default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct MistralVibeTransportManifest {
    schema_version: i64,
    managed_by: String,
    agent_id: String,
    transport: String,
    endpoint: String,
    model: String,
    provider_id: String,
    previous_active_model_present: bool,
    previous_active_model: Option<String>,
    previous_provider_present: bool,
    previous_provider: Option<String>,
}

fn gemini_auth_type(root: &Map<String, Value>) -> Option<String> {
    root.get("security")
        .and_then(Value::as_object)
        .and_then(|security| security.get("auth"))
        .and_then(Value::as_object)
        .and_then(|auth| auth.get(GEMINI_AUTH_SELECTED_TYPE))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn set_gemini_auth_type(root: &mut Map<String, Value>, auth_type: Option<&str>) -> Result<()> {
    let security = root
        .entry("security")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("Gemini CLI settings security must be an object")?;
    let auth = security
        .entry("auth")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("Gemini CLI settings security.auth must be an object")?;
    match auth_type {
        Some(value) => {
            auth.insert(
                GEMINI_AUTH_SELECTED_TYPE.into(),
                Value::String(value.into()),
            );
        }
        None => {
            auth.remove(GEMINI_AUTH_SELECTED_TYPE);
            if auth.is_empty() {
                security.remove("auth");
            }
            if security.is_empty() {
                root.remove("security");
            }
        }
    }
    Ok(())
}

fn transform_gemini_settings(
    text: &str,
    operation: AgentTransportOperation,
    auth_type: Option<&str>,
) -> Result<String> {
    let mut root: Map<String, Value> = if text.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(text).context("Gemini CLI settings must be strict JSON")?
    };
    let next = match operation {
        AgentTransportOperation::Configure => Some(GEMINI_AUTH_TYPE_API_KEY),
        AgentTransportOperation::Clear => auth_type,
    };
    set_gemini_auth_type(&mut root, next)?;
    Ok(serde_json::to_string_pretty(&Value::Object(root))? + "\n")
}

fn dotenv_unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

fn dotenv_key_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let body = line
            .trim_start()
            .strip_prefix("export ")
            .unwrap_or(line.trim_start());
        let Some((candidate, value)) = body.split_once('=') else {
            continue;
        };
        if candidate.trim() == key {
            return Some(dotenv_unquote(value));
        }
    }
    None
}

fn dotenv_set_key(text: &str, key: &str, value: Option<&str>) -> Result<String> {
    let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    let mut replaced = false;
    for line in lines.iter_mut() {
        let body = line
            .trim_start()
            .strip_prefix("export ")
            .unwrap_or(line.trim_start());
        let Some((candidate, _)) = body.split_once('=') else {
            continue;
        };
        if candidate.trim() == key {
            *line = match value {
                Some(value) => format!("{key}={value}"),
                None => String::new(),
            };
            replaced = true;
            break;
        }
    }
    if !replaced {
        if let Some(value) = value {
            lines.push(format!("{key}={value}"));
        }
    }
    if value.is_none() {
        lines.retain(|line| !line.is_empty());
    }
    let mut result = lines.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    Ok(result)
}

fn gemini_env_api_key_owned(env_text: &str) -> Result<()> {
    match dotenv_key_value(env_text, GEMINI_ENV_API_KEY) {
        None => Ok(()),
        Some(value) if value == GEMINI_PUBLIC_API_KEY_MARKER => Ok(()),
        Some(_) => bail!(
            "Gemini CLI .env contains a credential value for GEMINI_API_KEY; automatic routing is unsafe"
        ),
    }
}

fn transform_gemini_env(
    text: &str,
    operation: AgentTransportOperation,
    endpoint: Option<&str>,
    model: Option<&str>,
) -> Result<String> {
    match operation {
        AgentTransportOperation::Configure => {
            gemini_env_api_key_owned(text)?;
            let endpoint = endpoint.context("Gemini CLI transport requires an endpoint")?;
            let model = model.context("Gemini CLI transport requires an explicit model")?;
            let mut out = dotenv_set_key(text, GEMINI_ENV_BASE_URL, Some(endpoint))?;
            out = dotenv_set_key(&out, GEMINI_ENV_MODEL, Some(model))?;
            out = dotenv_set_key(&out, GEMINI_ENV_API_KEY, Some(GEMINI_PUBLIC_API_KEY_MARKER))?;
            Ok(out)
        }
        AgentTransportOperation::Clear => {
            gemini_env_api_key_owned(text)?;
            let mut out = dotenv_set_key(text, GEMINI_ENV_BASE_URL, endpoint)?;
            out = dotenv_set_key(&out, GEMINI_ENV_MODEL, model)?;
            out = dotenv_set_key(&out, GEMINI_ENV_API_KEY, None)?;
            Ok(out)
        }
    }
}

fn gemini_env_selection(env_text: &str) -> Result<PriorSelection> {
    let model = dotenv_key_value(env_text, GEMINI_ENV_MODEL);
    validate_prior_selection(None, model.as_deref())?;
    let base_url = dotenv_key_value(env_text, GEMINI_ENV_BASE_URL);
    validate_prior_selection(base_url.as_deref(), None)?;
    Ok(PriorSelection {
        provider_present: base_url.is_some(),
        provider: base_url,
        model_present: model.is_some(),
        model,
    })
}

fn gemini_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let manifest_path = gemini_manifest_path(config_path)?;
    let Some(manifest) = parse_gemini_manifest(&manifest_path)? else {
        return Ok(None);
    };
    let settings = optional_text(config_path)?.unwrap_or_default();
    let root: Map<String, Value> =
        serde_json::from_str(&settings).context("Gemini CLI settings must be strict JSON")?;
    let env_text = optional_text(&gemini_env_path(config_path)?)?.unwrap_or_default();
    let selection = gemini_env_selection(&env_text)?;
    if gemini_auth_type(&root).as_deref() != Some(GEMINI_AUTH_TYPE_API_KEY)
        || selection.provider.as_deref() != Some(manifest.endpoint.as_str())
        || selection.model.as_deref() != Some(manifest.model.as_str())
        || dotenv_key_value(&env_text, GEMINI_ENV_API_KEY).as_deref()
            != Some(GEMINI_PUBLIC_API_KEY_MARKER)
    {
        return Ok(None);
    }
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: GEMINI_PROVIDER_ID.to_string(),
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}",
            "gemini-cli", manifest.endpoint, manifest.model
        )),
        endpoint: manifest.endpoint,
        model: Some(manifest.model),
        enabled: true,
    }))
}

fn preview_gemini_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let env_path = gemini_env_path(&request.target_path)?;
    let env_before = optional_text(&env_path)?.unwrap_or_default();
    let manifest_path = gemini_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let current_upstream = gemini_current_upstream_signature(&request.target_path)?;
    let existing_manifest = parse_gemini_manifest(&manifest_path)?;
    let (config_after, env_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Gemini CLI transport requires an explicit model")?;
            if let Some(manifest) = existing_manifest.as_ref() {
                if current_upstream.is_none() {
                    bail!(
                        "Gemini CLI selection changed outside d-token; automatic update is unsafe"
                    );
                }
                ensure_gemini_current_matches(manifest, &env_before, &config_before)?;
            }
            let previous_auth_type_present = existing_manifest
                .as_ref()
                .map(|item| item.previous_auth_type_present)
                .unwrap_or_else(|| gemini_auth_type(&parse_gemini_root(&config_before)).is_some());
            let previous_auth_type = existing_manifest
                .as_ref()
                .and_then(|item| item.previous_auth_type.clone())
                .or_else(|| gemini_auth_type(&parse_gemini_root(&config_before)));
            let previous_env = gemini_env_selection(&env_before)?;
            let previous_base_url_present = existing_manifest
                .as_ref()
                .map(|item| item.previous_base_url_present)
                .unwrap_or(previous_env.provider_present);
            let previous_base_url = existing_manifest
                .as_ref()
                .and_then(|item| item.previous_base_url.clone())
                .or_else(|| previous_env.provider.clone());
            let previous_model_present = existing_manifest
                .as_ref()
                .map(|item| item.previous_model_present)
                .unwrap_or(previous_env.model_present);
            let previous_model = existing_manifest
                .as_ref()
                .and_then(|item| item.previous_model.clone())
                .or_else(|| previous_env.model.clone());
            (
                transform_gemini_settings(&config_before, operation, None)?,
                transform_gemini_env(&env_before, operation, Some(&request.endpoint), Some(model))?,
                manifest_json(&GeminiTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "gemini-cli".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: GEMINI_PROVIDER_ID.into(),
                    previous_auth_type_present,
                    previous_auth_type,
                    previous_base_url_present,
                    previous_base_url,
                    previous_model_present,
                    previous_model,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                if dotenv_key_value(&env_before, GEMINI_ENV_API_KEY).as_deref()
                    == Some(GEMINI_PUBLIC_API_KEY_MARKER)
                    || gemini_auth_type(&parse_gemini_root(&config_before)).as_deref()
                        == Some(GEMINI_AUTH_TYPE_API_KEY)
                {
                    bail!("Gemini CLI has unowned d-token-shaped routing state; automatic clear is unsafe");
                }
                return Ok(AgentTransportReview {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    revision: revision_for_hashes(
                        &request.agent_id,
                        operation,
                        &[
                            (
                                "config",
                                crate::stable_hash(&config_before),
                                crate::stable_hash(&config_before),
                            ),
                            (
                                "env",
                                crate::stable_hash(&env_before),
                                crate::stable_hash(&env_before),
                            ),
                        ],
                    ),
                    changed: false,
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    redacted_diff: String::new(),
                    current_upstream: None,
                });
            };
            if current_upstream.is_some() {
                (
                    transform_gemini_settings(
                        &config_before,
                        operation,
                        manifest.previous_auth_type.as_deref(),
                    )?,
                    transform_gemini_env(
                        &env_before,
                        operation,
                        manifest.previous_base_url.as_deref(),
                        manifest.previous_model.as_deref(),
                    )?,
                    String::new(),
                )
            } else {
                bail!("Gemini CLI selection changed outside d-token; automatic clear is unsafe");
            }
        }
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "env",
                    crate::stable_hash(&env_before),
                    crate::stable_hash(&env_after),
                ),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after
            || env_before != env_after
            || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("environment", &env_before, &env_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn parse_gemini_root(text: &str) -> Map<String, Value> {
    if text.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(text).unwrap_or_default()
    }
}

fn ensure_gemini_current_matches(
    manifest: &GeminiTransportManifest,
    env_text: &str,
    settings_text: &str,
) -> Result<()> {
    let selection = gemini_env_selection(env_text)?;
    let root = parse_gemini_root(settings_text);
    if gemini_auth_type(&root).as_deref() != Some(GEMINI_AUTH_TYPE_API_KEY)
        || selection.provider.as_deref() != Some(manifest.endpoint.as_str())
        || selection.model.as_deref() != Some(manifest.model.as_str())
        || dotenv_key_value(env_text, GEMINI_ENV_API_KEY).as_deref()
            != Some(GEMINI_PUBLIC_API_KEY_MARKER)
    {
        bail!("Gemini CLI selection changed outside d-token; automatic update is unsafe");
    }
    Ok(())
}

fn apply_gemini_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let review = preview_gemini_transport(request, operation)?;
    let settings_before = optional_text(&request.target_path)?.unwrap_or_default();
    let env_path = gemini_env_path(&request.target_path)?;
    let env_before = optional_text(&env_path)?.unwrap_or_default();
    let manifest_path = gemini_manifest_path(&request.target_path)?;
    let existing_manifest = parse_gemini_manifest(&manifest_path)?;

    let (settings_after, env_after, manifest_after, changed) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Gemini CLI transport requires an explicit model")?;
            let manifest = existing_manifest.as_ref();
            let current_upstream = gemini_current_upstream_signature(&request.target_path)?;
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("Gemini CLI selection changed outside d-token; automatic update is unsafe");
            }
            let previous_auth_type_present = manifest
                .map(|item| item.previous_auth_type_present)
                .unwrap_or_else(|| {
                    gemini_auth_type(&parse_gemini_root(&settings_before)).is_some()
                });
            let previous_auth_type = manifest
                .and_then(|item| item.previous_auth_type.clone())
                .or_else(|| gemini_auth_type(&parse_gemini_root(&settings_before)));
            let previous_env = gemini_env_selection(&env_before)?;
            let previous_base_url_present = manifest
                .map(|item| item.previous_base_url_present)
                .unwrap_or(previous_env.provider_present);
            let previous_base_url = manifest
                .and_then(|item| item.previous_base_url.clone())
                .or_else(|| previous_env.provider.clone());
            let previous_model_present = manifest
                .map(|item| item.previous_model_present)
                .unwrap_or(previous_env.model_present);
            let previous_model = manifest
                .and_then(|item| item.previous_model.clone())
                .or_else(|| previous_env.model.clone());
            (
                transform_gemini_settings(&settings_before, operation, None)?,
                transform_gemini_env(&env_before, operation, Some(&request.endpoint), Some(model))?,
                manifest_json(&GeminiTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "gemini-cli".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: GEMINI_PROVIDER_ID.into(),
                    previous_auth_type_present,
                    previous_auth_type,
                    previous_base_url_present,
                    previous_base_url,
                    previous_model_present,
                    previous_model,
                })?,
                review.changed,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: review.before_sha256,
                    after_sha256: review.after_sha256,
                    changed: false,
                    transaction: None,
                    verification: verify_gemini_cleared(&request.target_path)?,
                });
            };
            (
                transform_gemini_settings(
                    &settings_before,
                    operation,
                    manifest.previous_auth_type.as_deref(),
                )?,
                transform_gemini_env(
                    &env_before,
                    operation,
                    manifest.previous_base_url.as_deref(),
                    manifest.previous_model.as_deref(),
                )?,
                String::new(),
                review.changed,
            )
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions = Vec::new();
    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if settings_before != settings_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &settings_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if env_before != env_after {
            changes.push(patcher.prepare_change(
                &env_path,
                &env_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else {
        if settings_before != settings_after {
            let change = patcher.prepare_change(
                &request.target_path,
                &settings_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?;
            transactions.push(patcher.apply_transaction(&[change])?);
        }
        if env_before != env_after {
            if env_after.is_empty() {
                if let Err(error) = remove_owned_file(&env_path) {
                    for tx in transactions.iter().rev() {
                        let _ = rollback_apply_receipts(&tx.changes, backup_root);
                    }
                    return Err(error);
                }
            } else {
                let change = patcher.prepare_change(
                    &env_path,
                    &env_after,
                    ConfigToolKind::Agent,
                    Some(request.agent_id.clone()),
                    RiskLevel::Medium,
                )?;
                transactions.push(patcher.apply_transaction(&[change])?);
            }
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for tx in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&tx.changes, backup_root);
            }
            return Err(error);
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_gemini_cleared(&request.target_path)?
    } else {
        verify_gemini_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        bail!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        );
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: review.before_sha256,
        after_sha256: review.after_sha256,
        changed,
        transaction: merge_transactions(transactions),
        verification,
    })
}

fn verify_gemini_transport(request: &AgentTransportRequest) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        let manifest = parse_gemini_manifest(&gemini_manifest_path(&request.target_path)?)?
            .context("Gemini CLI ownership manifest is absent")?;
        if manifest.endpoint != request.endpoint
            || Some(manifest.model.as_str()) != request.model.as_deref()
        {
            bail!("Gemini CLI selection does not match the reviewed request");
        }
        let settings = optional_text(&request.target_path)?.unwrap_or_default();
        let env_text = optional_text(&gemini_env_path(&request.target_path)?)?.unwrap_or_default();
        ensure_gemini_current_matches(&manifest, &env_text, &settings)
    })();
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::GeminiCli,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed Gemini CLI transport verified".into()),
        current_upstream: gemini_current_upstream_signature(&request.target_path)
            .ok()
            .flatten(),
    })
}

fn verify_gemini_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if parse_gemini_manifest(&gemini_manifest_path(config_path)?)?.is_some() {
            bail!("Gemini CLI ownership manifest is still present");
        }
        let settings = optional_text(config_path)?.unwrap_or_default();
        if gemini_auth_type(&parse_gemini_root(&settings)).as_deref()
            == Some(GEMINI_AUTH_TYPE_API_KEY)
        {
            bail!("Gemini CLI auth type is still d-token-managed");
        }
        let env_text = optional_text(&gemini_env_path(config_path)?)?.unwrap_or_default();
        // A restored prior GEMINI_MODEL / GOOGLE_GEMINI_BASE_URL is legitimate
        // user state; only the d-token loopback marker key must be absent.
        if dotenv_key_value(&env_text, GEMINI_ENV_API_KEY).is_some() {
            bail!("Gemini CLI .env still contains a d-token routing API key marker");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "gemini-cli".into(),
        kind: AgentTransportAdapterKind::GeminiCli,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed Gemini CLI transport is clear".into()),
        current_upstream: None,
    })
}

fn gemini_can_clear(config_path: &Path) -> bool {
    let Ok(manifest) =
        parse_gemini_manifest(&gemini_manifest_path(config_path).unwrap_or_default())
    else {
        return false;
    };
    let Some(manifest) = manifest else {
        return false;
    };
    let Ok(settings) = optional_text(config_path) else {
        return false;
    };
    let Ok(env_text) = optional_text(&gemini_env_path(config_path).unwrap_or_default()) else {
        return false;
    };
    let settings = settings.unwrap_or_default();
    let env_text = env_text.unwrap_or_default();
    ensure_gemini_current_matches(&manifest, &env_text, &settings).is_ok()
}

// ---------- Roo Code (global settings.json) ----------

#[derive(Clone)]
struct RooSelection {
    api_provider_present: bool,
    api_provider: Option<String>,
    base_url_present: bool,
    base_url: Option<String>,
    model_id_present: bool,
    model_id: Option<String>,
    config_name_present: bool,
    config_name: Option<String>,
    list_present: bool,
    list: Option<Vec<Value>>,
}

fn roo_selection(root: &Map<String, Value>) -> RooSelection {
    RooSelection {
        api_provider_present: root.get("apiProvider").is_some(),
        api_provider: roo_string(root, "apiProvider"),
        base_url_present: root.get("openAiBaseUrl").is_some(),
        base_url: roo_string(root, "openAiBaseUrl"),
        model_id_present: root.get("openAiModelId").is_some(),
        model_id: roo_string(root, "openAiModelId"),
        config_name_present: root.get("currentApiConfigName").is_some(),
        config_name: roo_string(root, "currentApiConfigName"),
        list_present: root.get("listApiConfigMeta").is_some(),
        list: root
            .get("listApiConfigMeta")
            .and_then(Value::as_array)
            .cloned(),
    }
}

fn roo_string(root: &Map<String, Value>, key: &str) -> Option<String> {
    root.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn roo_owned_list_entry(model: &str) -> Value {
    serde_json::json!({
        "id": ROO_PROVIDER_ID,
        "name": ROO_PROVIDER_NAME,
        "apiProvider": ROO_API_PROVIDER,
        "modelId": model,
    })
}

fn roo_ensure_api_key_owned(root: &Map<String, Value>) -> Result<()> {
    match roo_string(root, "openAiApiKey") {
        None => Ok(()),
        Some(value) if value == ROO_PUBLIC_API_KEY_MARKER => Ok(()),
        Some(_) => bail!(
            "Roo Code settings contain a credential value for openAiApiKey; automatic routing is unsafe"
        ),
    }
}

fn roo_ensure_list_entry_owned(list: Option<&Vec<Value>>, model: &str) -> Result<()> {
    let Some(list) = list else {
        return Ok(());
    };
    for entry in list {
        if entry.get("id").and_then(Value::as_str) == Some(ROO_PROVIDER_ID) {
            if *entry != roo_owned_list_entry(model) {
                bail!("Roo Code has an unowned dtoken-local-route API config; automatic routing is unsafe");
            }
            return Ok(());
        }
    }
    Ok(())
}

fn set_roo_transport(root: &mut Map<String, Value>, endpoint: &str, model: &str) -> Result<()> {
    roo_ensure_api_key_owned(root)?;
    roo_ensure_list_entry_owned(
        root.get("listApiConfigMeta").and_then(Value::as_array),
        model,
    )?;
    root.insert("apiProvider".into(), Value::String(ROO_API_PROVIDER.into()));
    root.insert(
        "openAiBaseUrl".into(),
        Value::String(opencode_base_url(endpoint)),
    );
    root.insert("openAiModelId".into(), Value::String(model.into()));
    root.insert(
        "openAiApiKey".into(),
        Value::String(ROO_PUBLIC_API_KEY_MARKER.into()),
    );
    root.insert(
        "currentApiConfigName".into(),
        Value::String(ROO_PROVIDER_NAME.into()),
    );
    let entry = roo_owned_list_entry(model);
    let list = root
        .entry("listApiConfigMeta")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("Roo Code listApiConfigMeta must be an array")?;
    if !list
        .iter()
        .any(|item| item.get("id").and_then(Value::as_str) == Some(ROO_PROVIDER_ID))
    {
        list.push(entry);
    }
    Ok(())
}

fn remove_roo_transport(
    root: &mut Map<String, Value>,
    manifest: &RooTransportManifest,
) -> Result<()> {
    if let Some(key) = roo_string(root, "openAiApiKey") {
        if key != ROO_PUBLIC_API_KEY_MARKER {
            bail!("Roo Code openAiApiKey changed outside d-token; automatic clear is unsafe");
        }
        root.remove("openAiApiKey");
    }
    restore_roo_scalar(
        root,
        "apiProvider",
        manifest.previous_api_provider_present,
        manifest.previous_api_provider.clone(),
    );
    restore_roo_scalar(
        root,
        "openAiBaseUrl",
        manifest.previous_base_url_present,
        manifest.previous_base_url.clone(),
    );
    restore_roo_scalar(
        root,
        "openAiModelId",
        manifest.previous_model_id_present,
        manifest.previous_model_id.clone(),
    );
    restore_roo_scalar(
        root,
        "currentApiConfigName",
        manifest.previous_config_name_present,
        manifest.previous_config_name.clone(),
    );
    match &manifest.previous_list {
        Some(previous) => {
            root.insert("listApiConfigMeta".into(), Value::Array(previous.clone()));
        }
        None => {
            root.remove("listApiConfigMeta");
        }
    }
    Ok(())
}

fn restore_roo_scalar(
    root: &mut Map<String, Value>,
    key: &str,
    was_present: bool,
    previous: Option<String>,
) {
    match (was_present, previous) {
        (true, Some(value)) => {
            root.insert(key.into(), Value::String(value));
        }
        _ => {
            root.remove(key);
        }
    }
}

fn ensure_roo_current_matches(
    manifest: &RooTransportManifest,
    root: &Map<String, Value>,
) -> Result<()> {
    let current = roo_selection(root);
    if current.api_provider.as_deref() != Some(ROO_API_PROVIDER)
        || current.base_url.as_deref() != Some(opencode_base_url(&manifest.endpoint).as_str())
        || current.model_id.as_deref() != Some(manifest.model.as_str())
        || current.config_name.as_deref() != Some(ROO_PROVIDER_NAME)
        || roo_string(root, "openAiApiKey").as_deref() != Some(ROO_PUBLIC_API_KEY_MARKER)
    {
        bail!("Roo Code selection changed outside d-token");
    }
    if current.list.is_some()
        && !current.list.as_deref().is_some_and(|list| {
            list.iter()
                .any(|entry| entry.get("id").and_then(Value::as_str) == Some(ROO_PROVIDER_ID))
        })
    {
        bail!("Roo Code dtoken-local-route API config is missing from listApiConfigMeta");
    }
    Ok(())
}

fn roo_serialize(root: &Map<String, Value>) -> Result<String> {
    Ok(serde_json::to_string_pretty(&Value::Object(root.clone()))? + "\n")
}

fn roo_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let manifest_path = roo_manifest_path(config_path)?;
    let Some(manifest) = parse_roo_manifest(&manifest_path)? else {
        return Ok(None);
    };
    let root = roo_root(config_path)?;
    if ensure_roo_current_matches(&manifest, &root).is_err() {
        return Ok(None);
    }
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: ROO_PROVIDER_ID.to_string(),
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}",
            "roo-code", manifest.endpoint, manifest.model
        )),
        endpoint: manifest.endpoint,
        model: Some(manifest.model),
        enabled: true,
    }))
}

fn roo_root(config_path: &Path) -> Result<Map<String, Value>> {
    let text = optional_text(config_path)?.unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    serde_json::from_str(&text).context("Roo Code settings must be strict JSON")
}

fn preview_roo_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = roo_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let current_upstream = roo_current_upstream_signature(&request.target_path)?;
    let existing_manifest = parse_roo_manifest(&manifest_path)?;
    let mut root = roo_root(&request.target_path)?;

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Roo Code transport requires an explicit model")?;
            if let Some(manifest) = existing_manifest.as_ref() {
                if current_upstream.is_none() {
                    bail!("Roo Code selection changed outside d-token; automatic update is unsafe");
                }
                ensure_roo_current_matches(manifest, &root)?;
            }
            let previous = existing_manifest
                .as_ref()
                .map(|item| RooSelection {
                    api_provider_present: item.previous_api_provider_present,
                    api_provider: item.previous_api_provider.clone(),
                    base_url_present: item.previous_base_url_present,
                    base_url: item.previous_base_url.clone(),
                    model_id_present: item.previous_model_id_present,
                    model_id: item.previous_model_id.clone(),
                    config_name_present: item.previous_config_name_present,
                    config_name: item.previous_config_name.clone(),
                    list_present: item.previous_list_present,
                    list: item.previous_list.clone(),
                })
                .unwrap_or_else(|| roo_selection(&root));
            set_roo_transport(&mut root, &request.endpoint, model)?;
            (
                roo_serialize(&root)?,
                manifest_json(&RooTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "roo-code".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: ROO_PROVIDER_ID.into(),
                    previous_api_provider_present: previous.api_provider_present,
                    previous_api_provider: previous.api_provider,
                    previous_base_url_present: previous.base_url_present,
                    previous_base_url: previous.base_url,
                    previous_model_id_present: previous.model_id_present,
                    previous_model_id: previous.model_id,
                    previous_config_name_present: previous.config_name_present,
                    previous_config_name: previous.config_name,
                    previous_list_present: previous.list_present,
                    previous_list: previous.list,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                // Fail closed only on clear evidence of d-token ownership;
                // ordinary user provider fields without a manifest are not ours
                // to clear and are treated as an idempotent no-op.
                if roo_string(&root, "openAiApiKey").as_deref() == Some(ROO_PUBLIC_API_KEY_MARKER)
                    || roo_string(&root, "currentApiConfigName").as_deref()
                        == Some(ROO_PROVIDER_NAME)
                    || root
                        .get("listApiConfigMeta")
                        .and_then(Value::as_array)
                        .is_some_and(|list| {
                            list.iter().any(|entry| {
                                entry.get("id").and_then(Value::as_str) == Some(ROO_PROVIDER_ID)
                            })
                        })
                {
                    bail!("Roo Code has unowned d-token-shaped provider state; automatic clear is unsafe");
                }
                return Ok(AgentTransportReview {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    revision: revision_for_hashes(
                        &request.agent_id,
                        operation,
                        &[(
                            "config",
                            crate::stable_hash(&config_before),
                            crate::stable_hash(&config_before),
                        )],
                    ),
                    changed: false,
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    redacted_diff: String::new(),
                    current_upstream: None,
                });
            };
            if current_upstream.is_none() {
                bail!("Roo Code selection changed outside d-token; automatic clear is unsafe");
            }
            remove_roo_transport(&mut root, manifest)?;
            (roo_serialize(&root)?, String::new())
        }
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn apply_roo_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let review = preview_roo_transport(request, operation)?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = roo_manifest_path(&request.target_path)?;
    let existing_manifest = parse_roo_manifest(&manifest_path)?;
    let mut root = roo_root(&request.target_path)?;

    let (config_after, manifest_after, changed) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Roo Code transport requires an explicit model")?;
            let manifest = existing_manifest.as_ref();
            let current_upstream = roo_current_upstream_signature(&request.target_path)?;
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("Roo Code selection changed outside d-token; automatic update is unsafe");
            }
            let previous = manifest
                .map(|item| RooSelection {
                    api_provider_present: item.previous_api_provider_present,
                    api_provider: item.previous_api_provider.clone(),
                    base_url_present: item.previous_base_url_present,
                    base_url: item.previous_base_url.clone(),
                    model_id_present: item.previous_model_id_present,
                    model_id: item.previous_model_id.clone(),
                    config_name_present: item.previous_config_name_present,
                    config_name: item.previous_config_name.clone(),
                    list_present: item.previous_list_present,
                    list: item.previous_list.clone(),
                })
                .unwrap_or_else(|| roo_selection(&root));
            set_roo_transport(&mut root, &request.endpoint, model)?;
            (
                roo_serialize(&root)?,
                manifest_json(&RooTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "roo-code".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: ROO_PROVIDER_ID.into(),
                    previous_api_provider_present: previous.api_provider_present,
                    previous_api_provider: previous.api_provider,
                    previous_base_url_present: previous.base_url_present,
                    previous_base_url: previous.base_url,
                    previous_model_id_present: previous.model_id_present,
                    previous_model_id: previous.model_id,
                    previous_config_name_present: previous.config_name_present,
                    previous_config_name: previous.config_name,
                    previous_list_present: previous.list_present,
                    previous_list: previous.list,
                })?,
                review.changed,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: review.before_sha256,
                    after_sha256: review.after_sha256,
                    changed: false,
                    transaction: None,
                    verification: verify_roo_cleared(&request.target_path)?,
                });
            };
            remove_roo_transport(&mut root, manifest)?;
            (roo_serialize(&root)?, String::new(), review.changed)
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions = Vec::new();
    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else {
        if config_before != config_after {
            let change = patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?;
            transactions.push(patcher.apply_transaction(&[change])?);
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for tx in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&tx.changes, backup_root);
            }
            return Err(error);
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_roo_cleared(&request.target_path)?
    } else {
        verify_roo_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        bail!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        );
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: review.before_sha256,
        after_sha256: review.after_sha256,
        changed,
        transaction: merge_transactions(transactions),
        verification,
    })
}

fn verify_roo_transport(request: &AgentTransportRequest) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        let manifest = parse_roo_manifest(&roo_manifest_path(&request.target_path)?)?
            .context("Roo Code ownership manifest is absent")?;
        if manifest.endpoint != request.endpoint
            || Some(manifest.model.as_str()) != request.model.as_deref()
        {
            bail!("Roo Code selection does not match the reviewed request");
        }
        ensure_roo_current_matches(&manifest, &roo_root(&request.target_path)?)
    })();
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::RooCode,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed Roo Code transport verified".into()),
        current_upstream: roo_current_upstream_signature(&request.target_path)
            .ok()
            .flatten(),
    })
}

fn verify_roo_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if parse_roo_manifest(&roo_manifest_path(config_path)?)?.is_some() {
            bail!("Roo Code ownership manifest is still present");
        }
        let root = roo_root(config_path)?;
        // Restored prior provider/model fields are legitimate user state; only
        // d-token-owned markers must be gone.
        if roo_string(&root, "openAiApiKey").as_deref() == Some(ROO_PUBLIC_API_KEY_MARKER)
            || roo_string(&root, "currentApiConfigName").as_deref() == Some(ROO_PROVIDER_NAME)
            || root
                .get("listApiConfigMeta")
                .and_then(Value::as_array)
                .is_some_and(|list| {
                    list.iter().any(|entry| {
                        entry.get("id").and_then(Value::as_str) == Some(ROO_PROVIDER_ID)
                    })
                })
        {
            bail!("Roo Code still contains the d-token local route API config");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "roo-code".into(),
        kind: AgentTransportAdapterKind::RooCode,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed Roo Code transport is clear".into()),
        current_upstream: None,
    })
}

fn roo_can_clear(config_path: &Path) -> bool {
    let Ok(manifest) = parse_roo_manifest(&roo_manifest_path(config_path).unwrap_or_default())
    else {
        return false;
    };
    let Some(manifest) = manifest else {
        return false;
    };
    let Ok(root) = roo_root(config_path) else {
        return false;
    };
    ensure_roo_current_matches(&manifest, &root).is_ok()
}

// ---------- Continue (config.json) ----------

fn continue_ensure_json_authoritative(config_path: &Path) -> Result<()> {
    let yaml_path = config_path
        .parent()
        .context("Continue configuration path has no parent directory")?
        .join(CONTINUE_YAML_CONFIG);
    if yaml_path.is_file() {
        bail!(
            "Continue YAML config at {} takes precedence; the JSON adapter is not authoritative",
            yaml_path.display()
        );
    }
    Ok(())
}

fn continue_root(config_path: &Path) -> Result<Map<String, Value>> {
    let text = optional_text(config_path)?.unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    serde_json::from_str(&text).context("Continue config must be strict JSON")
}

fn continue_default_model(root: &Map<String, Value>) -> Option<String> {
    root.get("default")
        .and_then(Value::as_object)
        .and_then(|default| default.get("model"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn continue_owned_model(endpoint: &str, model: &str) -> Value {
    serde_json::json!({
        "title": CONTINUE_MODEL_TITLE,
        "provider": CONTINUE_PROVIDER_ID,
        "model": model,
        "apiBase": opencode_base_url(endpoint),
        "apiKey": CONTINUE_PUBLIC_API_KEY_MARKER,
    })
}

fn continue_models(root: &Map<String, Value>) -> Vec<Value> {
    root.get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn ensure_continue_model_owned(
    root: &Map<String, Value>,
    endpoint: &str,
    model: &str,
) -> Result<()> {
    let owned = continue_owned_model(endpoint, model);
    for entry in continue_models(root) {
        if entry.get("title").and_then(Value::as_str) == Some(CONTINUE_MODEL_TITLE) {
            if entry != owned {
                bail!("Continue has an unowned d-token local route model; automatic routing is unsafe");
            }
            return Ok(());
        }
    }
    Ok(())
}

fn set_continue_transport(
    root: &mut Map<String, Value>,
    endpoint: &str,
    model: &str,
) -> Result<()> {
    ensure_continue_model_owned(root, endpoint, model)?;
    let owned = continue_owned_model(endpoint, model);
    let models = root
        .entry("models")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("Continue models must be an array")?;
    models.retain(|entry| entry.get("title").and_then(Value::as_str) != Some(CONTINUE_MODEL_TITLE));
    models.push(owned);
    let default = root
        .entry("default")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("Continue default must be an object")?;
    default.insert("model".into(), Value::String(CONTINUE_MODEL_TITLE.into()));
    Ok(())
}

fn remove_continue_transport(
    root: &mut Map<String, Value>,
    manifest: &ContinueTransportManifest,
) -> Result<()> {
    if let Some(models) = root.get_mut("models").and_then(Value::as_array_mut) {
        models.retain(|entry| {
            entry.get("title").and_then(Value::as_str) != Some(CONTINUE_MODEL_TITLE)
        });
    }
    match &manifest.previous_models {
        Some(previous) => {
            root.insert("models".into(), Value::Array(previous.clone()));
        }
        None => {
            root.remove("models");
        }
    }
    if let Some(default) = root.get_mut("default").and_then(Value::as_object_mut) {
        match &manifest.previous_default_model {
            Some(previous) => {
                default.insert("model".into(), Value::String(previous.clone()));
            }
            None => {
                default.remove("model");
                if default.is_empty() {
                    root.remove("default");
                }
            }
        }
    }
    Ok(())
}

fn ensure_continue_current_matches(
    manifest: &ContinueTransportManifest,
    root: &Map<String, Value>,
) -> Result<()> {
    let owned = continue_owned_model(&manifest.endpoint, &manifest.model);
    let present = continue_models(root)
        .iter()
        .any(|entry| entry.get("title").and_then(Value::as_str) == Some(CONTINUE_MODEL_TITLE));
    if !present {
        bail!("Continue d-token local route model is absent");
    }
    if !continue_models(root).contains(&owned) {
        bail!("Continue d-token local route model changed outside d-token");
    }
    if continue_default_model(root).as_deref() != Some(CONTINUE_MODEL_TITLE) {
        bail!("Continue default model is not the d-token local route");
    }
    Ok(())
}

fn continue_serialize(root: &Map<String, Value>) -> Result<String> {
    Ok(serde_json::to_string_pretty(&Value::Object(root.clone()))? + "\n")
}

fn continue_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let manifest_path = continue_manifest_path(config_path)?;
    let Some(manifest) = parse_continue_manifest(&manifest_path)? else {
        return Ok(None);
    };
    let root = continue_root(config_path)?;
    if ensure_continue_current_matches(&manifest, &root).is_err() {
        return Ok(None);
    }
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: CONTINUE_PROVIDER_ID.to_string(),
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}",
            "continue", manifest.endpoint, manifest.model
        )),
        endpoint: manifest.endpoint,
        model: Some(manifest.model),
        enabled: true,
    }))
}

fn preview_continue_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    continue_ensure_json_authoritative(&request.target_path)?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = continue_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let current_upstream = continue_current_upstream_signature(&request.target_path)?;
    let existing_manifest = parse_continue_manifest(&manifest_path)?;
    let mut root = continue_root(&request.target_path)?;

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Continue transport requires an explicit model")?;
            if let Some(manifest) = existing_manifest.as_ref() {
                if current_upstream.is_none() {
                    bail!("Continue selection changed outside d-token; automatic update is unsafe");
                }
                ensure_continue_current_matches(manifest, &root)?;
            }
            let previous_models_present = existing_manifest
                .as_ref()
                .map(|item| item.previous_models_present)
                .unwrap_or(root.get("models").is_some());
            let previous_models = existing_manifest
                .as_ref()
                .and_then(|item| item.previous_models.clone())
                .or_else(|| root.get("models").and_then(Value::as_array).cloned());
            let previous_default_model_present = existing_manifest
                .as_ref()
                .map(|item| item.previous_default_model_present)
                .unwrap_or_else(|| continue_default_model(&root).is_some());
            let previous_default_model = existing_manifest
                .as_ref()
                .and_then(|item| item.previous_default_model.clone())
                .or_else(|| continue_default_model(&root));
            set_continue_transport(&mut root, &request.endpoint, model)?;
            (
                continue_serialize(&root)?,
                manifest_json(&ContinueTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "continue".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: CONTINUE_PROVIDER_ID.into(),
                    previous_models_present,
                    previous_models,
                    previous_default_model_present,
                    previous_default_model,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                if continue_models(&root).iter().any(|entry| {
                    entry.get("title").and_then(Value::as_str) == Some(CONTINUE_MODEL_TITLE)
                }) {
                    bail!("Continue has an unowned d-token local route model; automatic clear is unsafe");
                }
                return Ok(AgentTransportReview {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    revision: revision_for_hashes(
                        &request.agent_id,
                        operation,
                        &[(
                            "config",
                            crate::stable_hash(&config_before),
                            crate::stable_hash(&config_before),
                        )],
                    ),
                    changed: false,
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    redacted_diff: String::new(),
                    current_upstream: None,
                });
            };
            if current_upstream.is_none() {
                bail!("Continue selection changed outside d-token; automatic clear is unsafe");
            }
            remove_continue_transport(&mut root, manifest)?;
            (continue_serialize(&root)?, String::new())
        }
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn apply_continue_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let review = preview_continue_transport(request, operation)?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = continue_manifest_path(&request.target_path)?;
    let existing_manifest = parse_continue_manifest(&manifest_path)?;
    let mut root = continue_root(&request.target_path)?;

    let (config_after, manifest_after, changed) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Continue transport requires an explicit model")?;
            let manifest = existing_manifest.as_ref();
            let current_upstream = continue_current_upstream_signature(&request.target_path)?;
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("Continue selection changed outside d-token; automatic update is unsafe");
            }
            let previous_models_present = manifest
                .map(|item| item.previous_models_present)
                .unwrap_or(root.get("models").is_some());
            let previous_models = manifest
                .and_then(|item| item.previous_models.clone())
                .or_else(|| root.get("models").and_then(Value::as_array).cloned());
            let previous_default_model_present = manifest
                .map(|item| item.previous_default_model_present)
                .unwrap_or_else(|| continue_default_model(&root).is_some());
            let previous_default_model = manifest
                .and_then(|item| item.previous_default_model.clone())
                .or_else(|| continue_default_model(&root));
            set_continue_transport(&mut root, &request.endpoint, model)?;
            (
                continue_serialize(&root)?,
                manifest_json(&ContinueTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "continue".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: CONTINUE_PROVIDER_ID.into(),
                    previous_models_present,
                    previous_models,
                    previous_default_model_present,
                    previous_default_model,
                })?,
                review.changed,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: review.before_sha256,
                    after_sha256: review.after_sha256,
                    changed: false,
                    transaction: None,
                    verification: verify_continue_cleared(&request.target_path)?,
                });
            };
            remove_continue_transport(&mut root, manifest)?;
            (continue_serialize(&root)?, String::new(), review.changed)
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions = Vec::new();
    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else {
        if config_before != config_after {
            let change = patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?;
            transactions.push(patcher.apply_transaction(&[change])?);
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for tx in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&tx.changes, backup_root);
            }
            return Err(error);
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_continue_cleared(&request.target_path)?
    } else {
        verify_continue_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        bail!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        );
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: review.before_sha256,
        after_sha256: review.after_sha256,
        changed,
        transaction: merge_transactions(transactions),
        verification,
    })
}

fn verify_continue_transport(
    request: &AgentTransportRequest,
) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        continue_ensure_json_authoritative(&request.target_path)?;
        let manifest = parse_continue_manifest(&continue_manifest_path(&request.target_path)?)?
            .context("Continue ownership manifest is absent")?;
        if manifest.endpoint != request.endpoint
            || Some(manifest.model.as_str()) != request.model.as_deref()
        {
            bail!("Continue selection does not match the reviewed request");
        }
        ensure_continue_current_matches(&manifest, &continue_root(&request.target_path)?)
    })();
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::Continue,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed Continue transport verified".into()),
        current_upstream: continue_current_upstream_signature(&request.target_path)
            .ok()
            .flatten(),
    })
}

fn verify_continue_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if parse_continue_manifest(&continue_manifest_path(config_path)?)?.is_some() {
            bail!("Continue ownership manifest is still present");
        }
        let root = continue_root(config_path)?;
        if continue_models(&root)
            .iter()
            .any(|entry| entry.get("title").and_then(Value::as_str) == Some(CONTINUE_MODEL_TITLE))
        {
            bail!("Continue still contains the d-token local route model");
        }
        if continue_default_model(&root).as_deref() == Some(CONTINUE_MODEL_TITLE) {
            bail!("Continue default model is still the d-token local route");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "continue".into(),
        kind: AgentTransportAdapterKind::Continue,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed Continue transport is clear".into()),
        current_upstream: None,
    })
}

fn continue_can_clear(config_path: &Path) -> bool {
    let Ok(manifest) =
        parse_continue_manifest(&continue_manifest_path(config_path).unwrap_or_default())
    else {
        return false;
    };
    let Some(manifest) = manifest else {
        return false;
    };
    if continue_ensure_json_authoritative(config_path).is_err() {
        return false;
    }
    let Ok(root) = continue_root(config_path) else {
        return false;
    };
    ensure_continue_current_matches(&manifest, &root).is_ok()
}

// ---------- Aider (.aider.conf.yml) ----------

fn aider_key_value(text: &str, key: &str) -> Result<Option<String>> {
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(key) {
            continue;
        }
        let Some((candidate, value)) = trimmed.split_once(':') else {
            continue;
        };
        if candidate.trim() != key {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            bail!("Aider config key {key} has nested YAML content; automatic routing is unsafe");
        }
        return Ok(Some(dotenv_unquote(value)));
    }
    Ok(None)
}

fn aider_set_key(text: &str, key: &str, value: Option<&str>) -> Result<String> {
    let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    let mut replaced = false;
    for line in lines.iter_mut() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(key) {
            continue;
        }
        let Some((candidate, _)) = trimmed.split_once(':') else {
            continue;
        };
        if candidate.trim() != key {
            continue;
        }
        *line = match value {
            Some(value) => format!("{key}: {value}"),
            None => String::new(),
        };
        replaced = true;
        break;
    }
    if !replaced {
        if let Some(value) = value {
            lines.push(format!("{key}: {value}"));
        }
    }
    if value.is_none() {
        lines.retain(|line| !line.is_empty());
    }
    let mut result = lines.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    Ok(result)
}

fn aider_api_key_owned(text: &str) -> Result<()> {
    match aider_key_value(text, "openai-api-key")? {
        None => Ok(()),
        Some(value) if value == AIDER_PUBLIC_API_KEY_MARKER => Ok(()),
        Some(_) => bail!(
            "Aider config contains a credential value for openai-api-key; automatic routing is unsafe"
        ),
    }
}

fn transform_aider(
    text: &str,
    operation: AgentTransportOperation,
    endpoint: Option<&str>,
    model: Option<&str>,
) -> Result<String> {
    match operation {
        AgentTransportOperation::Configure => {
            aider_api_key_owned(text)?;
            let endpoint = endpoint.context("Aider transport requires an endpoint")?;
            let model = model.context("Aider transport requires an explicit model")?;
            let mut out = aider_set_key(text, "model", Some(model))?;
            out = aider_set_key(&out, "openai-api-base", Some(&opencode_base_url(endpoint)))?;
            out = aider_set_key(&out, "openai-api-key", Some(AIDER_PUBLIC_API_KEY_MARKER))?;
            Ok(out)
        }
        AgentTransportOperation::Clear => {
            aider_api_key_owned(text)?;
            let mut out = aider_set_key(text, "model", model)?;
            out = aider_set_key(&out, "openai-api-base", endpoint)?;
            out = aider_set_key(&out, "openai-api-key", None)?;
            Ok(out)
        }
    }
}

fn aider_selection(text: &str) -> Result<PriorSelection> {
    let model = aider_key_value(text, "model")?;
    validate_prior_selection(None, model.as_deref())?;
    let base_url = aider_key_value(text, "openai-api-base")?;
    validate_prior_selection(base_url.as_deref(), None)?;
    Ok(PriorSelection {
        provider_present: base_url.is_some(),
        provider: base_url,
        model_present: model.is_some(),
        model,
    })
}

fn aider_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let manifest_path = aider_manifest_path(config_path)?;
    let Some(manifest) = parse_aider_manifest(&manifest_path)? else {
        return Ok(None);
    };
    let text = optional_text(config_path)?.unwrap_or_default();
    let selection = aider_selection(&text)?;
    if selection.provider.as_deref() != Some(opencode_base_url(&manifest.endpoint).as_str())
        || selection.model.as_deref() != Some(manifest.model.as_str())
        || aider_key_value(&text, "openai-api-key")?.as_deref() != Some(AIDER_PUBLIC_API_KEY_MARKER)
    {
        return Ok(None);
    }
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: AIDER_PROVIDER_ID.to_string(),
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}",
            "aider", manifest.endpoint, manifest.model
        )),
        endpoint: manifest.endpoint,
        model: Some(manifest.model),
        enabled: true,
    }))
}

fn preview_aider_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = aider_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let current_upstream = aider_current_upstream_signature(&request.target_path)?;
    let existing_manifest = parse_aider_manifest(&manifest_path)?;

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Aider transport requires an explicit model")?;
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("Aider selection changed outside d-token; automatic update is unsafe");
            }
            let previous = existing_manifest
                .as_ref()
                .map(|item| PriorSelection {
                    provider_present: item.previous_base_present,
                    provider: item.previous_base.clone(),
                    model_present: item.previous_model_present,
                    model: item.previous_model.clone(),
                })
                .unwrap_or(aider_selection(&config_before)?);
            (
                transform_aider(
                    &config_before,
                    operation,
                    Some(&request.endpoint),
                    Some(model),
                )?,
                manifest_json(&AiderTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "aider".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: AIDER_PROVIDER_ID.into(),
                    previous_model_present: previous.model_present,
                    previous_model: previous.model,
                    previous_base_present: previous.provider_present,
                    previous_base: previous.provider,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                // Fail closed only on the d-token API key marker; ordinary user
                // routing keys without a manifest are an idempotent no-op.
                if aider_key_value(&config_before, "openai-api-key")?.as_deref()
                    == Some(AIDER_PUBLIC_API_KEY_MARKER)
                {
                    bail!(
                        "Aider has unowned d-token-shaped routing state; automatic clear is unsafe"
                    );
                }
                return Ok(AgentTransportReview {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    revision: revision_for_hashes(
                        &request.agent_id,
                        operation,
                        &[(
                            "config",
                            crate::stable_hash(&config_before),
                            crate::stable_hash(&config_before),
                        )],
                    ),
                    changed: false,
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    redacted_diff: String::new(),
                    current_upstream: None,
                });
            };
            if current_upstream.is_none() {
                bail!("Aider selection changed outside d-token; automatic clear is unsafe");
            }
            (
                transform_aider(
                    &config_before,
                    operation,
                    manifest.previous_base.as_deref(),
                    manifest.previous_model.as_deref(),
                )?,
                String::new(),
            )
        }
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn apply_aider_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let review = preview_aider_transport(request, operation)?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = aider_manifest_path(&request.target_path)?;
    let existing_manifest = parse_aider_manifest(&manifest_path)?;

    let (config_after, manifest_after, changed) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Aider transport requires an explicit model")?;
            let manifest = existing_manifest.as_ref();
            let current_upstream = aider_current_upstream_signature(&request.target_path)?;
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("Aider selection changed outside d-token; automatic update is unsafe");
            }
            let previous = manifest
                .map(|item| PriorSelection {
                    provider_present: item.previous_base_present,
                    provider: item.previous_base.clone(),
                    model_present: item.previous_model_present,
                    model: item.previous_model.clone(),
                })
                .unwrap_or(aider_selection(&config_before)?);
            (
                transform_aider(
                    &config_before,
                    operation,
                    Some(&request.endpoint),
                    Some(model),
                )?,
                manifest_json(&AiderTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "aider".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: AIDER_PROVIDER_ID.into(),
                    previous_model_present: previous.model_present,
                    previous_model: previous.model,
                    previous_base_present: previous.provider_present,
                    previous_base: previous.provider,
                })?,
                review.changed,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: review.before_sha256,
                    after_sha256: review.after_sha256,
                    changed: false,
                    transaction: None,
                    verification: verify_aider_cleared(&request.target_path)?,
                });
            };
            (
                transform_aider(
                    &config_before,
                    operation,
                    manifest.previous_base.as_deref(),
                    manifest.previous_model.as_deref(),
                )?,
                String::new(),
                review.changed,
            )
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions = Vec::new();
    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else {
        if config_before != config_after {
            let change = patcher.prepare_change(
                &request.target_path,
                &config_after,
                ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                RiskLevel::Medium,
            )?;
            transactions.push(patcher.apply_transaction(&[change])?);
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for tx in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&tx.changes, backup_root);
            }
            return Err(error);
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_aider_cleared(&request.target_path)?
    } else {
        verify_aider_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        bail!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        );
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: review.before_sha256,
        after_sha256: review.after_sha256,
        changed,
        transaction: merge_transactions(transactions),
        verification,
    })
}

fn verify_aider_transport(request: &AgentTransportRequest) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        let manifest = parse_aider_manifest(&aider_manifest_path(&request.target_path)?)?
            .context("Aider ownership manifest is absent")?;
        if manifest.endpoint != request.endpoint
            || Some(manifest.model.as_str()) != request.model.as_deref()
        {
            bail!("Aider selection does not match the reviewed request");
        }
        let text = optional_text(&request.target_path)?.unwrap_or_default();
        let selection = aider_selection(&text)?;
        if selection.provider.as_deref() != Some(opencode_base_url(&manifest.endpoint).as_str())
            || selection.model.as_deref() != Some(manifest.model.as_str())
            || aider_key_value(&text, "openai-api-key")?.as_deref()
                != Some(AIDER_PUBLIC_API_KEY_MARKER)
        {
            bail!("Aider routing keys do not match the d-token manifest");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::Aider,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed Aider transport verified".into()),
        current_upstream: aider_current_upstream_signature(&request.target_path)
            .ok()
            .flatten(),
    })
}

fn verify_aider_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if parse_aider_manifest(&aider_manifest_path(config_path)?)?.is_some() {
            bail!("Aider ownership manifest is still present");
        }
        let text = optional_text(config_path)?.unwrap_or_default();
        // A restored prior model / openai-api-base is legitimate user state;
        // only the d-token loopback API key marker must be gone.
        if aider_key_value(&text, "openai-api-key")?.is_some() {
            bail!("Aider still contains a d-token routing API key marker");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "aider".into(),
        kind: AgentTransportAdapterKind::Aider,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "d-token managed Aider transport is clear".into()),
        current_upstream: None,
    })
}

fn aider_can_clear(config_path: &Path) -> bool {
    let Ok(manifest) = parse_aider_manifest(&aider_manifest_path(config_path).unwrap_or_default())
    else {
        return false;
    };
    let Some(manifest) = manifest else {
        return false;
    };
    let Ok(text) = optional_text(config_path) else {
        return false;
    };
    let text = text.unwrap_or_default();
    let Ok(selection) = aider_selection(&text) else {
        return false;
    };
    selection.provider.as_deref() == Some(opencode_base_url(&manifest.endpoint).as_str())
        && selection.model.as_deref() == Some(manifest.model.as_str())
        && aider_key_value(&text, "openai-api-key")
            .ok()
            .flatten()
            .as_deref()
            == Some(AIDER_PUBLIC_API_KEY_MARKER)
}

// ---------- Hermes (~/.hermes/config.yaml) ----------
// Schema: model.provider/custom/base_url/default
// Does NOT read .env files.

const HERMES_API_KEY: &str = "apiKey";

fn hermes_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Hermes configuration path has no parent directory")?
        .join(HERMES_MANIFEST_FILE))
}

fn hermes_selection(text: &str) -> Result<PriorSelection> {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(text).context("Hermes config must be valid YAML")?;
    let provider = doc
        .get("model")
        .and_then(|m| m.get("provider"))
        .and_then(|p| p.as_mapping())
        .and_then(|p| p.get("custom"))
        .and_then(|c| c.as_mapping())
        .and_then(|c| c.get("base_url"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let model = doc
        .get("model")
        .and_then(|m| m.get("default"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    validate_prior_selection(provider.as_deref(), model.as_deref())?;
    Ok(PriorSelection {
        provider_present: provider.is_some(),
        provider,
        model_present: model.is_some(),
        model,
    })
}

fn hermes_default_present(text: &str) -> bool {
    serde_yaml::from_str::<serde_yaml::Value>(text)
        .ok()
        .and_then(|doc| doc.get("model").map(|m| m.get("default").is_some()))
        .unwrap_or(false)
}

fn transform_hermes(
    text: &str,
    operation: AgentTransportOperation,
    endpoint: &str,
    model: &str,
) -> Result<String> {
    if operation == AgentTransportOperation::Clear && text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let mut doc: serde_yaml::Value = if text.trim().is_empty() {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    } else {
        serde_yaml::from_str(text).context("Hermes config must be valid YAML")?
    };
    let root = doc
        .as_mapping_mut()
        .context("Hermes config must be a mapping")?;
    match operation {
        AgentTransportOperation::Configure => {
            let model_entry = root
                .entry("model".into())
                .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
            let model_map = model_entry
                .as_mapping_mut()
                .context("Hermes model must be a mapping")?;
            let provider = model_map
                .entry("provider".into())
                .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
            let provider_map = provider
                .as_mapping_mut()
                .context("Hermes model.provider must be a mapping")?;
            let custom = provider_map
                .entry("custom".into())
                .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
            let custom_map = custom
                .as_mapping_mut()
                .context("Hermes model.provider.custom must be a mapping")?;
            custom_map.insert(
                "base_url".into(),
                serde_yaml::Value::String(opencode_base_url(endpoint)),
            );
            custom_map.insert(
                HERMES_API_KEY.into(),
                serde_yaml::Value::String(HERMES_PUBLIC_API_KEY_MARKER.into()),
            );
            model_map.insert("default".into(), serde_yaml::Value::String(model.into()));
        }
        AgentTransportOperation::Clear => {
            if let Some(model_map) = root.get_mut("model").and_then(|m| m.as_mapping_mut()) {
                // Remove the d-token apiKey marker
                if let Some(provider) = model_map.get_mut("provider") {
                    if let Some(provider_map) = provider.as_mapping_mut() {
                        if let Some(custom) = provider_map.get_mut("custom") {
                            if let Some(custom_map) = custom.as_mapping_mut() {
                                custom_map.remove(HERMES_API_KEY);
                            }
                        }
                    }
                }
                // Restore previous base_url if provided
                if !endpoint.is_empty() {
                    if let Some(provider) = model_map.get_mut("provider") {
                        if let Some(provider_map) = provider.as_mapping_mut() {
                            let custom = provider_map.entry("custom".into()).or_insert_with(|| {
                                serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
                            });
                            if let Some(custom_map) = custom.as_mapping_mut() {
                                custom_map.insert(
                                    "base_url".into(),
                                    serde_yaml::Value::String(endpoint.into()),
                                );
                            }
                        }
                    }
                } else {
                    model_map.remove("provider");
                }
                // Restore previous model if provided
                if !model.is_empty() {
                    model_map.insert("default".into(), serde_yaml::Value::String(model.into()));
                } else {
                    model_map.remove("default");
                }
                if model_map.is_empty() {
                    root.remove("model");
                }
            }
        }
    }
    let mut out = Vec::new();
    serde_yaml::to_writer(&mut out, &doc).context("failed to serialize Hermes YAML")?;
    Ok(String::from_utf8(out)?)
}

fn hermes_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let manifest_path = hermes_manifest_path(config_path)?;
    let Some(manifest) = parse_hermes_manifest(&manifest_path)? else {
        return Ok(None);
    };
    let text = optional_text(config_path)?.unwrap_or_default();
    let selection = hermes_selection(&text)?;
    let expected_base = opencode_base_url(&manifest.endpoint);
    if selection.provider.as_deref() != Some(&expected_base)
        || selection.model.as_deref() != Some(manifest.model.as_str())
    {
        return Ok(None);
    }
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: HERMES_PROVIDER_ID.to_string(),
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}",
            "hermes", manifest.endpoint, manifest.model
        )),
        endpoint: manifest.endpoint,
        model: Some(manifest.model),
        enabled: true,
    }))
}

fn parse_hermes_manifest(path: &Path) -> Result<Option<HermesTransportManifest>> {
    let text = match optional_text(path)? {
        Some(text) => text,
        None => return Ok(None),
    };
    let manifest: HermesTransportManifest =
        serde_json::from_str(&text).context("Hermes transport manifest is invalid JSON")?;
    if manifest.managed_by != DTOKEN_MANAGED_BY || manifest.transport != DTOKEN_TRANSPORT {
        return Ok(None);
    }
    Ok(Some(manifest))
}

fn preview_hermes_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = hermes_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let current_upstream = hermes_current_upstream_signature(&request.target_path)?;
    let existing_manifest = parse_hermes_manifest(&manifest_path)?;

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Hermes transport requires an explicit model")?;
            if let Some(manifest) = existing_manifest.as_ref() {
                if current_upstream.is_none() {
                    bail!("Hermes selection changed outside d-token; automatic update is unsafe");
                }
                ensure_hermes_current_matches(manifest, &config_before)?;
            }
            let previous = existing_manifest
                .as_ref()
                .map(|item| PriorSelection {
                    provider_present: item.previous_base_url_present,
                    provider: item.previous_base_url.clone(),
                    model_present: item.previous_model_present,
                    model: item.previous_model.clone(),
                })
                .unwrap_or(hermes_selection(&config_before)?);
            (
                transform_hermes(&config_before, operation, &request.endpoint, model)?,
                manifest_json(&HermesTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "hermes".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: HERMES_PROVIDER_ID.into(),
                    previous_base_url_present: previous.provider_present,
                    previous_base_url: previous.provider,
                    previous_model_present: previous.model_present,
                    previous_model: previous.model,
                    previous_default_present: hermes_default_present(&config_before),
                    previous_default: None,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                if hermes_selection(&config_before)?.provider.is_some() {
                    bail!("Hermes has unowned d-token-shaped routing state; automatic clear is unsafe");
                }
                return Ok(AgentTransportReview {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    revision: revision_for_hashes(
                        &request.agent_id,
                        operation,
                        &[(
                            "config",
                            crate::stable_hash(&config_before),
                            crate::stable_hash(&config_before),
                        )],
                    ),
                    changed: false,
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    redacted_diff: String::new(),
                    current_upstream: None,
                });
            };
            if current_upstream.is_none() {
                bail!("Hermes selection changed outside d-token; automatic clear is unsafe");
            }
            (
                transform_hermes(
                    &config_before,
                    operation,
                    manifest.previous_base_url.as_deref().unwrap_or(""),
                    manifest.previous_model.as_deref().unwrap_or(""),
                )?,
                String::new(),
            )
        }
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn ensure_hermes_current_matches(
    manifest: &HermesTransportManifest,
    config_text: &str,
) -> Result<()> {
    let selection = hermes_selection(config_text)?;
    let expected_base = opencode_base_url(&manifest.endpoint);
    if selection.provider.as_deref() != Some(&expected_base)
        || selection.model.as_deref() != Some(manifest.model.as_str())
    {
        bail!("Hermes selection changed outside d-token; automatic update is unsafe");
    }
    Ok(())
}

fn apply_hermes_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let model = request
        .model
        .as_deref()
        .context("Hermes transport requires an explicit model")?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = hermes_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let prior = hermes_selection(&config_before)?;
            let out = transform_hermes(&config_before, operation, &request.endpoint, model)?;
            let manifest = manifest_json(&HermesTransportManifest {
                schema_version: TRANSPORT_SCHEMA_VERSION,
                managed_by: DTOKEN_MANAGED_BY.into(),
                agent_id: "hermes".into(),
                transport: DTOKEN_TRANSPORT.into(),
                endpoint: request.endpoint.clone(),
                model: model.to_string(),
                provider_id: HERMES_PROVIDER_ID.into(),
                previous_base_url_present: prior.provider_present,
                previous_base_url: prior.provider,
                previous_model_present: prior.model_present,
                previous_model: prior.model,
                previous_default_present: hermes_default_present(&config_before),
                previous_default: None,
            })?;
            (out, manifest)
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = parse_hermes_manifest(&manifest_path)? else {
                // No ownership manifest → no-op
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    changed: false,
                    transaction: None,
                    verification: verify_hermes_cleared(&request.target_path)?,
                });
            };
            let out = transform_hermes(
                &config_before,
                operation,
                manifest.previous_base_url.as_deref().unwrap_or(""),
                manifest.previous_model.as_deref().unwrap_or(""),
            )?;
            (out, manifest_before.clone())
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions: Vec<TransactionReceipt> = Vec::new();

    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                dtoken_types::ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                dtoken_types::RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                dtoken_types::ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                dtoken_types::RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for tx in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&tx.changes, backup_root);
            }
            return Err(error).context("failed to remove Hermes ownership manifest");
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_hermes_cleared(&request.target_path)?
    } else {
        verify_hermes_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        return Err(anyhow::anyhow!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        ));
    }

    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: crate::stable_hash(&config_before),
        after_sha256: crate::stable_hash(&config_after),
        changed: !transactions.is_empty(),
        transaction: transactions.into_iter().reduce(|mut a, b| {
            a.changes.extend(b.changes);
            a
        }),
        verification,
    })
}

fn verify_hermes_transport(request: &AgentTransportRequest) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        let manifest = parse_hermes_manifest(&hermes_manifest_path(&request.target_path)?)?
            .context("Hermes ownership manifest is absent")?;
        if manifest.endpoint != request.endpoint
            || Some(manifest.model.as_str()) != request.model.as_deref()
        {
            bail!("Hermes manifest does not match the reviewed request");
        }
        let text = optional_text(&request.target_path)?.unwrap_or_default();
        let selection = hermes_selection(&text)?;
        let expected_base = opencode_base_url(&manifest.endpoint);
        if selection.provider.as_deref() != Some(&expected_base)
            || selection.model.as_deref() != Some(manifest.model.as_str())
        {
            bail!("Hermes routing keys do not match the d-token manifest");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::Hermes,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed Hermes transport verified".into()),
        current_upstream: hermes_current_upstream_signature(&request.target_path)
            .ok()
            .flatten(),
    })
}

fn verify_hermes_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if parse_hermes_manifest(&hermes_manifest_path(config_path)?)?.is_some() {
            bail!("Hermes ownership manifest is still present");
        }
        let text = optional_text(config_path)?.unwrap_or_default();
        if !text.trim().is_empty() {
            let doc: serde_yaml::Value = serde_yaml::from_str(&text)?;
            // A restored prior base_url / model is legitimate user state;
            // only the d-token loopback API key marker must be gone.
            let has_dtoken_key = doc
                .get("model")
                .and_then(|m| m.get("provider"))
                .and_then(|p| p.get("custom"))
                .and_then(|c| c.get("apiKey"))
                .and_then(|v| v.as_str())
                == Some(HERMES_PUBLIC_API_KEY_MARKER);
            if has_dtoken_key {
                bail!("Hermes still contains a d-token routing API key marker");
            }
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "hermes".into(),
        kind: AgentTransportAdapterKind::Hermes,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed Hermes transport is clear".into()),
        current_upstream: None,
    })
}

fn hermes_can_clear(config_path: &Path) -> bool {
    let manifest_path = match hermes_manifest_path(config_path) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let Ok(manifest) = parse_hermes_manifest(&manifest_path) else {
        return false;
    };
    let Some(manifest) = manifest else {
        return false;
    };
    let Ok(text) = optional_text(config_path) else {
        return false;
    };
    let text = text.unwrap_or_default();
    let Ok(selection) = hermes_selection(&text) else {
        return false;
    };
    let expected_base = opencode_base_url(&manifest.endpoint);
    selection.provider.as_deref() == Some(&expected_base)
        && selection.model.as_deref() == Some(manifest.model.as_str())
}

// ---------- OpenHands (V0: CWD config.toml [llm]; V1: env-only fail closed) ----------

fn openhands_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("OpenHands configuration path has no parent directory")?
        .join(OPENHANDS_MANIFEST_FILE))
}

fn openhands_is_v1_environment() -> bool {
    std::env::var("OPENHANDS_LLM_API_KEY").is_ok()
        || std::env::var("LLM_API_KEY").is_ok()
        || std::env::var("OPENHANDS_MODEL").is_ok()
}

fn openhands_llm_selection(text: &str) -> Result<PriorSelection> {
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .context("OpenHands config must be valid TOML")?;
    let llm = doc.get("llm").and_then(|v| v.as_table());
    let model = llm
        .and_then(|t| t.get("model"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let base_url = llm
        .and_then(|t| t.get("base_url"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    if let Some(ref url) = base_url {
        if contains_secret_like(url) || url.contains(['@', '?', '#']) {
            bail!("OpenHands llm.base_url may contain credential material; automatic configuration is unsafe");
        }
    }
    validate_prior_selection(base_url.as_deref(), model.as_deref())?;
    Ok(PriorSelection {
        provider_present: base_url.is_some(),
        provider: base_url,
        model_present: model.is_some(),
        model,
    })
}

fn transform_openhands(
    text: &str,
    operation: AgentTransportOperation,
    endpoint: &str,
    model: &str,
) -> Result<String> {
    if operation == AgentTransportOperation::Clear && text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let mut doc: toml_edit::DocumentMut = if text.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        text.parse::<toml_edit::DocumentMut>()
            .context("OpenHands config must be valid TOML")?
    };
    match operation {
        AgentTransportOperation::Configure => {
            let llm = doc
                .entry("llm")
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                .as_table_mut()
                .context("OpenHands llm is not a table")?;
            llm["model"] = toml_edit::value(model);
            llm["base_url"] = toml_edit::value(endpoint);
            llm["api_key"] = toml_edit::value(OPENHANDS_PUBLIC_API_KEY_MARKER);
        }
        AgentTransportOperation::Clear => {
            if let Some(llm) = doc.get_mut("llm").and_then(|v| v.as_table_mut()) {
                llm.remove("api_key");
                if !model.is_empty() {
                    llm["model"] = toml_edit::value(model);
                } else {
                    llm.remove("model");
                }
                if !endpoint.is_empty() {
                    llm["base_url"] = toml_edit::value(endpoint);
                } else {
                    llm.remove("base_url");
                }
                if llm.is_empty() {
                    doc.remove("llm");
                }
            }
        }
    }
    Ok(doc.to_string())
}

fn openhands_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let manifest_path = openhands_manifest_path(config_path)?;
    let Some(manifest) = parse_openhands_manifest(&manifest_path)? else {
        return Ok(None);
    };
    let text = optional_text(config_path)?.unwrap_or_default();
    let selection = openhands_llm_selection(&text)?;
    if selection.provider.as_deref() != Some(&manifest.endpoint)
        || selection.model.as_deref() != Some(manifest.model.as_str())
    {
        return Ok(None);
    }
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: OPENHANDS_PROVIDER_ID.to_string(),
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}",
            "openhands", manifest.endpoint, manifest.model
        )),
        endpoint: manifest.endpoint,
        model: Some(manifest.model),
        enabled: true,
    }))
}

fn parse_openhands_manifest(path: &Path) -> Result<Option<OpenHandsTransportManifest>> {
    let text = match optional_text(path)? {
        Some(text) => text,
        None => return Ok(None),
    };
    let manifest: OpenHandsTransportManifest =
        serde_json::from_str(&text).context("OpenHands transport manifest is invalid JSON")?;
    if manifest.managed_by != DTOKEN_MANAGED_BY || manifest.transport != DTOKEN_TRANSPORT {
        return Ok(None);
    }
    Ok(Some(manifest))
}

fn preview_openhands_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    // V1 environment: fail closed
    if openhands_is_v1_environment() {
        bail!("OpenHands V1 uses environment variables for LLM configuration; the transport adapter cannot safely modify environment variables. Use the desktop-managed flow instead");
    }
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = openhands_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let current_upstream = openhands_current_upstream_signature(&request.target_path)?;
    let existing_manifest = parse_openhands_manifest(&manifest_path)?;

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("OpenHands transport requires an explicit model")?;
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("OpenHands selection changed outside d-token; automatic update is unsafe");
            }
            let previous = existing_manifest
                .as_ref()
                .map(|item| PriorSelection {
                    provider_present: item.previous_llm_base_url_present,
                    provider: item.previous_llm_base_url.clone(),
                    model_present: item.previous_llm_model_present,
                    model: item.previous_llm_model.clone(),
                })
                .unwrap_or(openhands_llm_selection(&config_before)?);
            (
                transform_openhands(&config_before, operation, &request.endpoint, model)?,
                manifest_json(&OpenHandsTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "openhands".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: OPENHANDS_PROVIDER_ID.into(),
                    previous_llm_model_present: previous.model_present,
                    previous_llm_model: previous.model,
                    previous_llm_base_url_present: previous.provider_present,
                    previous_llm_base_url: previous.provider,
                    previous_llm_api_key_present: false,
                    previous_llm_api_key: None,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                return Ok(AgentTransportReview {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    revision: revision_for_hashes(
                        &request.agent_id,
                        operation,
                        &[(
                            "config",
                            crate::stable_hash(&config_before),
                            crate::stable_hash(&config_before),
                        )],
                    ),
                    changed: false,
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    redacted_diff: String::new(),
                    current_upstream: None,
                });
            };
            if current_upstream.is_none() {
                bail!("OpenHands selection changed outside d-token; automatic clear is unsafe");
            }
            (
                transform_openhands(
                    &config_before,
                    operation,
                    manifest.previous_llm_base_url.as_deref().unwrap_or(""),
                    manifest.previous_llm_model.as_deref().unwrap_or(""),
                )?,
                String::new(),
            )
        }
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn apply_openhands_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    if operation == AgentTransportOperation::Configure && openhands_is_v1_environment() {
        bail!("OpenHands V1 uses environment variables for LLM configuration; the transport adapter cannot safely modify environment variables");
    }
    let model = request
        .model
        .as_deref()
        .context("OpenHands transport requires an explicit model")?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = openhands_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let prior = openhands_llm_selection(&config_before)?;
            let out = transform_openhands(&config_before, operation, &request.endpoint, model)?;
            let manifest = manifest_json(&OpenHandsTransportManifest {
                schema_version: TRANSPORT_SCHEMA_VERSION,
                managed_by: DTOKEN_MANAGED_BY.into(),
                agent_id: "openhands".into(),
                transport: DTOKEN_TRANSPORT.into(),
                endpoint: request.endpoint.clone(),
                model: model.to_string(),
                provider_id: OPENHANDS_PROVIDER_ID.into(),
                previous_llm_model_present: prior.model_present,
                previous_llm_model: prior.model,
                previous_llm_base_url_present: prior.provider_present,
                previous_llm_base_url: prior.provider,
                previous_llm_api_key_present: false,
                previous_llm_api_key: None,
            })?;
            (out, manifest)
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = parse_openhands_manifest(&manifest_path)? else {
                // No ownership manifest → no-op
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    changed: false,
                    transaction: None,
                    verification: verify_openhands_cleared(&request.target_path)?,
                });
            };
            let out = transform_openhands(
                &config_before,
                operation,
                manifest.previous_llm_base_url.as_deref().unwrap_or(""),
                manifest.previous_llm_model.as_deref().unwrap_or(""),
            )?;
            (out, manifest_before.clone())
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions: Vec<TransactionReceipt> = Vec::new();

    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                dtoken_types::ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                dtoken_types::RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                dtoken_types::ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                dtoken_types::RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for tx in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&tx.changes, backup_root);
            }
            return Err(error).context("failed to remove OpenHands ownership manifest");
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_openhands_cleared(&request.target_path)?
    } else {
        verify_openhands_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        return Err(anyhow::anyhow!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        ));
    }

    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: crate::stable_hash(&config_before),
        after_sha256: crate::stable_hash(&config_after),
        changed: !transactions.is_empty(),
        transaction: transactions.into_iter().reduce(|mut a, b| {
            a.changes.extend(b.changes);
            a
        }),
        verification,
    })
}

fn verify_openhands_transport(
    request: &AgentTransportRequest,
) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        let manifest = parse_openhands_manifest(&openhands_manifest_path(&request.target_path)?)?
            .context("OpenHands ownership manifest is absent")?;
        if manifest.endpoint != request.endpoint
            || Some(manifest.model.as_str()) != request.model.as_deref()
        {
            bail!("OpenHands manifest does not match the reviewed request");
        }
        let text = optional_text(&request.target_path)?.unwrap_or_default();
        let selection = openhands_llm_selection(&text)?;
        if selection.provider.as_deref() != Some(&manifest.endpoint)
            || selection.model.as_deref() != Some(manifest.model.as_str())
        {
            bail!("OpenHands routing keys do not match the d-token manifest");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::OpenHands,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed OpenHands transport verified".into()),
        current_upstream: openhands_current_upstream_signature(&request.target_path)
            .ok()
            .flatten(),
    })
}

fn verify_openhands_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if parse_openhands_manifest(&openhands_manifest_path(config_path)?)?.is_some() {
            bail!("OpenHands ownership manifest is still present");
        }
        let text = optional_text(config_path)?.unwrap_or_default();
        if !text.trim().is_empty() {
            let doc = text
                .parse::<toml_edit::DocumentMut>()
                .context("OpenHands config must be valid TOML")?;
            // A restored prior model/base_url is legitimate user state;
            // only the d-token loopback API key marker must be gone.
            let has_dtoken_key = doc
                .get("llm")
                .and_then(|v| v.get("api_key"))
                .and_then(|v| v.as_str())
                == Some(OPENHANDS_PUBLIC_API_KEY_MARKER);
            if has_dtoken_key {
                bail!("OpenHands still contains a d-token routing API key marker");
            }
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "openhands".into(),
        kind: AgentTransportAdapterKind::OpenHands,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed OpenHands transport is clear".into()),
        current_upstream: None,
    })
}

fn openhands_can_clear(config_path: &Path) -> bool {
    let manifest_path = match openhands_manifest_path(config_path) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let Ok(manifest) = parse_openhands_manifest(&manifest_path) else {
        return false;
    };
    let Some(manifest) = manifest else {
        return false;
    };
    let Ok(text) = optional_text(config_path) else {
        return false;
    };
    let text = text.unwrap_or_default();
    let Ok(selection) = openhands_llm_selection(&text) else {
        return false;
    };
    selection.provider.as_deref() == Some(&manifest.endpoint)
        && selection.model.as_deref() == Some(manifest.model.as_str())
}

// ---------- Pi (~/.pi/agent/models.json) ----------
// Schema: providers.dtoken + settings.defaultProvider/defaultModel
// Does NOT touch auth.json/apiKey.

fn pi_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Pi configuration path has no parent directory")?
        .join(PI_MANIFEST_FILE))
}

fn pi_selection(root: &Map<String, Value>) -> Result<PriorSelection> {
    let default_provider = root
        .get("settings")
        .and_then(|s| s.as_object())
        .and_then(|s| s.get("defaultProvider"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let default_model = root
        .get("settings")
        .and_then(|s| s.as_object())
        .and_then(|s| s.get("defaultModel"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    validate_prior_selection(None, default_model.as_deref())?;
    Ok(PriorSelection {
        provider_present: default_provider.is_some(),
        provider: default_provider,
        model_present: default_model.is_some(),
        model: default_model,
    })
}

fn transform_pi(
    text: &str,
    operation: AgentTransportOperation,
    endpoint: &str,
    model: &str,
) -> Result<String> {
    if operation == AgentTransportOperation::Clear && text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let mut root: Map<String, Value> = if text.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(text).context("Pi models.json must be strict JSON")?
    };
    match operation {
        AgentTransportOperation::Configure => {
            let providers = root
                .entry("providers")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .context("Pi providers must be an object")?;
            let dtoken = providers
                .entry(PI_PROVIDER_ID)
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .context("Pi dtoken provider must be an object")?;
            dtoken.insert("type".into(), Value::String("openai".into()));
            dtoken.insert("baseUrl".into(), Value::String(endpoint.into()));
            dtoken.insert(
                "apiKey".into(),
                Value::String(PI_PUBLIC_API_KEY_MARKER.into()),
            );
            dtoken.insert(
                "models".into(),
                Value::Array(vec![Value::String(model.into())]),
            );

            let settings = root
                .entry("settings")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .context("Pi settings must be an object")?;
            settings.insert(
                "defaultProvider".into(),
                Value::String(PI_PROVIDER_ID.into()),
            );
            settings.insert("defaultModel".into(), Value::String(model.into()));
        }
        AgentTransportOperation::Clear => {
            if let Some(providers) = root.get_mut("providers").and_then(|v| v.as_object_mut()) {
                providers.remove(PI_PROVIDER_ID);
                if providers.is_empty() {
                    root.remove("providers");
                }
            }
            if let Some(settings) = root.get_mut("settings").and_then(|v| v.as_object_mut()) {
                settings.remove("defaultProvider");
                settings.remove("defaultModel");
                if settings.is_empty() {
                    root.remove("settings");
                }
            }
            // Restore previous provider and model if provided
            let settings = root
                .entry("settings")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .context("Pi settings must be an object")?;
            if !endpoint.is_empty() {
                settings.insert("defaultProvider".into(), Value::String(endpoint.into()));
            }
            if !model.is_empty() {
                settings.insert("defaultModel".into(), Value::String(model.into()));
            }
            if settings.is_empty() {
                root.remove("settings");
            }
        }
    }
    Ok(serde_json::to_string_pretty(&Value::Object(root))? + "\n")
}

fn pi_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let manifest_path = pi_manifest_path(config_path)?;
    let Some(manifest) = parse_pi_manifest(&manifest_path)? else {
        return Ok(None);
    };
    let text = optional_text(config_path)?.unwrap_or_default();
    let root: Map<String, Value> = serde_json::from_str(&text).unwrap_or_default();
    let selection = pi_selection(&root)?;
    if selection.provider.as_deref() != Some(PI_PROVIDER_ID)
        || selection.model.as_deref() != Some(manifest.model.as_str())
    {
        return Ok(None);
    }
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: PI_PROVIDER_ID.to_string(),
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}",
            "pi", manifest.endpoint, manifest.model
        )),
        endpoint: manifest.endpoint,
        model: Some(manifest.model),
        enabled: true,
    }))
}

fn parse_pi_manifest(path: &Path) -> Result<Option<PiTransportManifest>> {
    let text = match optional_text(path)? {
        Some(text) => text,
        None => return Ok(None),
    };
    let manifest: PiTransportManifest =
        serde_json::from_str(&text).context("Pi transport manifest is invalid JSON")?;
    if manifest.managed_by != DTOKEN_MANAGED_BY || manifest.transport != DTOKEN_TRANSPORT {
        return Ok(None);
    }
    Ok(Some(manifest))
}

fn preview_pi_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = pi_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let current_upstream = pi_current_upstream_signature(&request.target_path)?;
    let existing_manifest = parse_pi_manifest(&manifest_path)?;
    let root: Map<String, Value> = serde_json::from_str(&config_before).unwrap_or_default();

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Pi transport requires an explicit model")?;
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("Pi selection changed outside d-token; automatic update is unsafe");
            }
            let previous = existing_manifest
                .as_ref()
                .map(|item| PriorSelection {
                    provider_present: item.previous_default_provider_present,
                    provider: item.previous_default_provider.clone(),
                    model_present: item.previous_default_model_present,
                    model: item.previous_default_model.clone(),
                })
                .unwrap_or(pi_selection(&root)?);
            (
                transform_pi(&config_before, operation, &request.endpoint, model)?,
                manifest_json(&PiTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "pi".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: PI_PROVIDER_ID.into(),
                    previous_default_provider_present: previous.provider_present,
                    previous_default_provider: previous.provider,
                    previous_default_model_present: previous.model_present,
                    previous_default_model: previous.model,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                if pi_selection(&root)
                    .ok()
                    .map(|s| s.provider_present)
                    .unwrap_or(false)
                {
                    bail!(
                        "Pi has unowned d-token-shaped provider state; automatic clear is unsafe"
                    );
                }
                return Ok(AgentTransportReview {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    revision: revision_for_hashes(
                        &request.agent_id,
                        operation,
                        &[(
                            "config",
                            crate::stable_hash(&config_before),
                            crate::stable_hash(&config_before),
                        )],
                    ),
                    changed: false,
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    redacted_diff: String::new(),
                    current_upstream: None,
                });
            };
            if current_upstream.is_none() {
                bail!("Pi selection changed outside d-token; automatic clear is unsafe");
            }
            (
                transform_pi(
                    &config_before,
                    operation,
                    manifest.previous_default_provider.as_deref().unwrap_or(""),
                    manifest.previous_default_model.as_deref().unwrap_or(""),
                )?,
                String::new(),
            )
        }
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn apply_pi_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let model = request
        .model
        .as_deref()
        .context("Pi transport requires an explicit model")?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = pi_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let root: Map<String, Value> = serde_json::from_str(&config_before).unwrap_or_default();
            let prior = pi_selection(&root)?;
            let out = transform_pi(&config_before, operation, &request.endpoint, model)?;
            let manifest = manifest_json(&PiTransportManifest {
                schema_version: TRANSPORT_SCHEMA_VERSION,
                managed_by: DTOKEN_MANAGED_BY.into(),
                agent_id: "pi".into(),
                transport: DTOKEN_TRANSPORT.into(),
                endpoint: request.endpoint.clone(),
                model: model.to_string(),
                provider_id: PI_PROVIDER_ID.into(),
                previous_default_provider_present: prior.provider_present,
                previous_default_provider: prior.provider,
                previous_default_model_present: prior.model_present,
                previous_default_model: prior.model,
            })?;
            (out, manifest)
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = parse_pi_manifest(&manifest_path)? else {
                // No ownership manifest → no-op
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    changed: false,
                    transaction: None,
                    verification: verify_pi_cleared(&request.target_path)?,
                });
            };
            let out = transform_pi(
                &config_before,
                operation,
                manifest.previous_default_provider.as_deref().unwrap_or(""),
                manifest.previous_default_model.as_deref().unwrap_or(""),
            )?;
            (out, manifest_before.clone())
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions: Vec<TransactionReceipt> = Vec::new();

    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                dtoken_types::ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                dtoken_types::RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                dtoken_types::ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                dtoken_types::RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for tx in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&tx.changes, backup_root);
            }
            return Err(error).context("failed to remove Pi ownership manifest");
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_pi_cleared(&request.target_path)?
    } else {
        verify_pi_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        return Err(anyhow::anyhow!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        ));
    }

    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: crate::stable_hash(&config_before),
        after_sha256: crate::stable_hash(&config_after),
        changed: !transactions.is_empty(),
        transaction: transactions.into_iter().reduce(|mut a, b| {
            a.changes.extend(b.changes);
            a
        }),
        verification,
    })
}

fn verify_pi_transport(request: &AgentTransportRequest) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        let manifest = parse_pi_manifest(&pi_manifest_path(&request.target_path)?)?
            .context("Pi ownership manifest is absent")?;
        if manifest.endpoint != request.endpoint
            || Some(manifest.model.as_str()) != request.model.as_deref()
        {
            bail!("Pi manifest does not match the reviewed request");
        }
        let text = optional_text(&request.target_path)?.unwrap_or_default();
        let root: Map<String, Value> = serde_json::from_str(&text).unwrap_or_default();
        let selection = pi_selection(&root)?;
        if selection.provider.as_deref() != Some(PI_PROVIDER_ID)
            || selection.model.as_deref() != Some(manifest.model.as_str())
        {
            bail!("Pi routing keys do not match the d-token manifest");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::Pi,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed Pi transport verified".into()),
        current_upstream: pi_current_upstream_signature(&request.target_path)
            .ok()
            .flatten(),
    })
}

fn verify_pi_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if parse_pi_manifest(&pi_manifest_path(config_path)?)?.is_some() {
            bail!("Pi ownership manifest is still present");
        }
        let text = optional_text(config_path)?.unwrap_or_default();
        if !text.trim().is_empty() {
            let root: Map<String, Value> = serde_json::from_str(&text).unwrap_or_default();
            // A restored prior provider/model is legitimate user state;
            // only the d-token loopback API key marker must be gone.
            let has_dtoken_key = root
                .get("providers")
                .and_then(|v| v.as_object())
                .and_then(|p| p.get(PI_PROVIDER_ID))
                .and_then(|v| v.as_object())
                .and_then(|p| p.get("apiKey"))
                .and_then(|v| v.as_str())
                == Some(PI_PUBLIC_API_KEY_MARKER);
            if has_dtoken_key {
                bail!("Pi still contains a d-token routing API key marker");
            }
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "pi".into(),
        kind: AgentTransportAdapterKind::Pi,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed Pi transport is clear".into()),
        current_upstream: None,
    })
}

fn pi_can_clear(config_path: &Path) -> bool {
    let manifest_path = match pi_manifest_path(config_path) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let Ok(manifest) = parse_pi_manifest(&manifest_path) else {
        return false;
    };
    let Some(manifest) = manifest else {
        return false;
    };
    let Ok(text) = optional_text(config_path) else {
        return false;
    };
    let text = text.unwrap_or_default();
    let Ok(root) = serde_json::from_str::<Map<String, Value>>(&text) else {
        return false;
    };
    let Ok(selection) = pi_selection(&root) else {
        return false;
    };
    selection.provider.as_deref() == Some(PI_PROVIDER_ID)
        && selection.model.as_deref() == Some(manifest.model.as_str())
}

// ---------- Mistral Vibe (~/.vibe/config.toml) ----------
// Schema: providers/models/active_model. Does NOT touch .env.

fn mistral_vibe_manifest_path(config_path: &Path) -> Result<PathBuf> {
    Ok(config_path
        .parent()
        .context("Mistral Vibe configuration path has no parent directory")?
        .join(MISTRAL_VIBE_MANIFEST_FILE))
}

fn mistral_vibe_selection(text: &str) -> Result<PriorSelection> {
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .context("Mistral Vibe config must be valid TOML")?;
    let active_model = doc
        .get("active_model")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let provider = doc
        .get("providers")
        .and_then(|v| v.as_table())
        .and_then(|t| t.get(MISTRAL_VIBE_PROVIDER_ID))
        .map(|_| MISTRAL_VIBE_PROVIDER_ID.to_string());
    let provider_present = doc
        .get("providers")
        .and_then(|v| v.as_table())
        .is_some_and(|t| t.contains_key(MISTRAL_VIBE_PROVIDER_ID));
    validate_prior_selection(None, active_model.as_deref())?;
    Ok(PriorSelection {
        provider_present,
        provider,
        model_present: active_model.is_some(),
        model: active_model,
    })
}

fn transform_mistral_vibe(
    text: &str,
    operation: AgentTransportOperation,
    endpoint: &str,
    model: &str,
) -> Result<String> {
    if operation == AgentTransportOperation::Clear && text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let mut doc: toml_edit::DocumentMut = if text.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        text.parse::<toml_edit::DocumentMut>()
            .context("Mistral Vibe config must be valid TOML")?
    };
    match operation {
        AgentTransportOperation::Configure => {
            let providers = doc
                .entry("providers")
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                .as_table_mut()
                .context("Mistral Vibe providers must be a table")?;
            let mut dtoken = toml_edit::Table::new();
            dtoken["type"] = toml_edit::value("openai-compatible");
            dtoken["base_url"] = toml_edit::value(endpoint);
            dtoken["api_key"] = toml_edit::value(MISTRAL_VIBE_PUBLIC_API_KEY_MARKER);
            dtoken["models"] = toml_edit::value(model);
            providers.insert(MISTRAL_VIBE_PROVIDER_ID, toml_edit::Item::Table(dtoken));
            doc["active_model"] =
                toml_edit::value(format!("{}/{}", MISTRAL_VIBE_PROVIDER_ID, model));
        }
        AgentTransportOperation::Clear => {
            if let Some(providers) = doc.get_mut("providers").and_then(|v| v.as_table_mut()) {
                providers.remove(MISTRAL_VIBE_PROVIDER_ID);
                if !endpoint.is_empty() {
                    let mut restored = toml_edit::Table::new();
                    restored["type"] = toml_edit::value("openai-compatible");
                    restored["base_url"] = toml_edit::value(endpoint);
                    if !model.is_empty() {
                        restored["models"] = toml_edit::value(model);
                    }
                    providers.insert(MISTRAL_VIBE_PROVIDER_ID, toml_edit::Item::Table(restored));
                }
                if providers.is_empty() {
                    doc.remove("providers");
                }
            }
            if !model.is_empty() {
                doc["active_model"] = toml_edit::value(model);
            } else {
                doc.remove("active_model");
            }
        }
    }
    Ok(doc.to_string())
}

fn mistral_vibe_current_upstream_signature(
    config_path: &Path,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let manifest_path = mistral_vibe_manifest_path(config_path)?;
    let Some(manifest) = parse_mistral_vibe_manifest(&manifest_path)? else {
        return Ok(None);
    };
    let text = optional_text(config_path)?.unwrap_or_default();
    let selection = mistral_vibe_selection(&text)?;
    let expected_active = format!("{}/{}", MISTRAL_VIBE_PROVIDER_ID, manifest.model);
    if selection.model.as_deref() != Some(&expected_active) {
        return Ok(None);
    }
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: MISTRAL_VIBE_PROVIDER_ID.to_string(),
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}",
            "mistral-vibe", manifest.endpoint, manifest.model
        )),
        endpoint: manifest.endpoint,
        model: Some(manifest.model),
        enabled: true,
    }))
}

fn parse_mistral_vibe_manifest(path: &Path) -> Result<Option<MistralVibeTransportManifest>> {
    let text = match optional_text(path)? {
        Some(text) => text,
        None => return Ok(None),
    };
    let manifest: MistralVibeTransportManifest =
        serde_json::from_str(&text).context("Mistral Vibe transport manifest is invalid JSON")?;
    if manifest.managed_by != DTOKEN_MANAGED_BY || manifest.transport != DTOKEN_TRANSPORT {
        return Ok(None);
    }
    Ok(Some(manifest))
}

fn preview_mistral_vibe_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = mistral_vibe_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();
    let current_upstream = mistral_vibe_current_upstream_signature(&request.target_path)?;
    let existing_manifest = parse_mistral_vibe_manifest(&manifest_path)?;

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let model = request
                .model
                .as_deref()
                .context("Mistral Vibe transport requires an explicit model")?;
            if existing_manifest.is_some() && current_upstream.is_none() {
                bail!("Mistral Vibe selection changed outside d-token; automatic update is unsafe");
            }
            let previous = existing_manifest
                .as_ref()
                .map(|item| PriorSelection {
                    provider_present: item.previous_provider_present,
                    provider: item.previous_provider.clone(),
                    model_present: item.previous_active_model_present,
                    model: item.previous_active_model.clone(),
                })
                .unwrap_or(mistral_vibe_selection(&config_before)?);
            (
                transform_mistral_vibe(&config_before, operation, &request.endpoint, model)?,
                manifest_json(&MistralVibeTransportManifest {
                    schema_version: TRANSPORT_SCHEMA_VERSION,
                    managed_by: DTOKEN_MANAGED_BY.into(),
                    agent_id: "mistral-vibe".into(),
                    transport: DTOKEN_TRANSPORT.into(),
                    endpoint: request.endpoint.clone(),
                    model: model.to_string(),
                    provider_id: MISTRAL_VIBE_PROVIDER_ID.into(),
                    previous_active_model_present: previous.model_present,
                    previous_active_model: previous.model,
                    previous_provider_present: previous.provider_present,
                    previous_provider: previous.provider,
                })?,
            )
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = existing_manifest.as_ref() else {
                let selection = mistral_vibe_selection(&config_before)?;
                if selection.provider_present {
                    bail!("Mistral Vibe has unowned d-token-shaped provider state; automatic clear is unsafe");
                }
                return Ok(AgentTransportReview {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    revision: revision_for_hashes(
                        &request.agent_id,
                        operation,
                        &[(
                            "config",
                            crate::stable_hash(&config_before),
                            crate::stable_hash(&config_before),
                        )],
                    ),
                    changed: false,
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    redacted_diff: String::new(),
                    current_upstream: None,
                });
            };
            if current_upstream.is_none() {
                bail!("Mistral Vibe selection changed outside d-token; automatic clear is unsafe");
            }
            (
                transform_mistral_vibe(
                    &config_before,
                    operation,
                    "",
                    manifest.previous_active_model.as_deref().unwrap_or(""),
                )?,
                String::new(),
            )
        }
    };

    let before_sha256 = crate::stable_hash(&config_before);
    let after_sha256 = crate::stable_hash(&config_after);
    Ok(AgentTransportReview {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        revision: revision_for_hashes(
            &request.agent_id,
            operation,
            &[
                ("config", before_sha256.clone(), after_sha256.clone()),
                (
                    "manifest",
                    crate::stable_hash(&manifest_before),
                    crate::stable_hash(&manifest_after),
                ),
            ],
        ),
        changed: config_before != config_after || manifest_before != manifest_after,
        before_sha256,
        after_sha256,
        redacted_diff: redacted_diff_sections(&[
            ("config", &config_before, &config_after),
            ("ownership manifest", &manifest_before, &manifest_after),
        ]),
        current_upstream,
    })
}

fn apply_mistral_vibe_transport(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let model = request
        .model
        .as_deref()
        .context("Mistral Vibe transport requires an explicit model")?;
    let config_before = optional_text(&request.target_path)?.unwrap_or_default();
    let manifest_path = mistral_vibe_manifest_path(&request.target_path)?;
    let manifest_before = optional_text(&manifest_path)?.unwrap_or_default();

    let (config_after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            let prior = mistral_vibe_selection(&config_before)?;
            let out = transform_mistral_vibe(&config_before, operation, &request.endpoint, model)?;
            let manifest = manifest_json(&MistralVibeTransportManifest {
                schema_version: TRANSPORT_SCHEMA_VERSION,
                managed_by: DTOKEN_MANAGED_BY.into(),
                agent_id: "mistral-vibe".into(),
                transport: DTOKEN_TRANSPORT.into(),
                endpoint: request.endpoint.clone(),
                model: model.to_string(),
                provider_id: MISTRAL_VIBE_PROVIDER_ID.into(),
                previous_active_model_present: prior.model_present,
                previous_active_model: prior.model,
                previous_provider_present: prior.provider_present,
                previous_provider: prior.provider,
            })?;
            (out, manifest)
        }
        AgentTransportOperation::Clear => {
            let Some(manifest) = parse_mistral_vibe_manifest(&manifest_path)? else {
                // No ownership manifest → no-op
                return Ok(AgentTransportReceipt {
                    operation,
                    agent_id: request.agent_id.clone(),
                    target_path: request.target_path.clone(),
                    before_sha256: crate::stable_hash(&config_before),
                    after_sha256: crate::stable_hash(&config_before),
                    changed: false,
                    transaction: None,
                    verification: verify_mistral_vibe_cleared(&request.target_path)?,
                });
            };
            let out = transform_mistral_vibe(
                &config_before,
                operation,
                "",
                manifest.previous_active_model.as_deref().unwrap_or(""),
            )?;
            (out, manifest_before.clone())
        }
    };

    let patcher = ConfigPatcher::new(backup_root);
    let mut transactions: Vec<TransactionReceipt> = Vec::new();

    if operation == AgentTransportOperation::Configure {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                dtoken_types::ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                dtoken_types::RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        match apply_manifest_change(
            &manifest_path,
            &manifest_after,
            &request.agent_id,
            backup_root,
        ) {
            Ok(Some(tx)) => transactions.push(tx),
            Ok(None) => {}
            Err(error) => {
                for tx in transactions.iter().rev() {
                    let _ = rollback_apply_receipts(&tx.changes, backup_root);
                }
                return Err(error);
            }
        }
    } else {
        let mut changes = Vec::new();
        if config_before != config_after {
            changes.push(patcher.prepare_change(
                &request.target_path,
                &config_after,
                dtoken_types::ConfigToolKind::Agent,
                Some(request.agent_id.clone()),
                dtoken_types::RiskLevel::Medium,
            )?);
        }
        if !changes.is_empty() {
            transactions.push(patcher.apply_transaction(&changes)?);
        }
        if let Err(error) = remove_owned_file(&manifest_path) {
            for tx in transactions.iter().rev() {
                let _ = rollback_apply_receipts(&tx.changes, backup_root);
            }
            return Err(error).context("failed to remove Mistral Vibe ownership manifest");
        }
    }

    let verification = if operation == AgentTransportOperation::Clear {
        verify_mistral_vibe_cleared(&request.target_path)?
    } else {
        verify_mistral_vibe_transport(request)?
    };
    if !verification.ok {
        for tx in transactions.iter().rev() {
            let _ = rollback_apply_receipts(&tx.changes, backup_root);
        }
        return Err(anyhow::anyhow!(
            "post-write Agent transport verification failed: {}",
            verification.detail
        ));
    }

    Ok(AgentTransportReceipt {
        operation,
        agent_id: request.agent_id.clone(),
        target_path: request.target_path.clone(),
        before_sha256: crate::stable_hash(&config_before),
        after_sha256: crate::stable_hash(&config_after),
        changed: !transactions.is_empty(),
        transaction: transactions.into_iter().reduce(|mut a, b| {
            a.changes.extend(b.changes);
            a
        }),
        verification,
    })
}

fn verify_mistral_vibe_transport(
    request: &AgentTransportRequest,
) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        let manifest =
            parse_mistral_vibe_manifest(&mistral_vibe_manifest_path(&request.target_path)?)?
                .context("Mistral Vibe ownership manifest is absent")?;
        if manifest.endpoint != request.endpoint
            || Some(manifest.model.as_str()) != request.model.as_deref()
        {
            bail!("Mistral Vibe manifest does not match the reviewed request");
        }
        let text = optional_text(&request.target_path)?.unwrap_or_default();
        let selection = mistral_vibe_selection(&text)?;
        let expected_active = format!("{}/{}", MISTRAL_VIBE_PROVIDER_ID, manifest.model);
        if selection.model.as_deref() != Some(&expected_active) {
            bail!("Mistral Vibe routing keys do not match the d-token manifest");
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: request.agent_id.clone(),
        kind: AgentTransportAdapterKind::MistralVibe,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed Mistral Vibe transport verified".into()),
        current_upstream: mistral_vibe_current_upstream_signature(&request.target_path)
            .ok()
            .flatten(),
    })
}

fn verify_mistral_vibe_cleared(config_path: &Path) -> Result<AgentTransportVerification> {
    let result: Result<()> = (|| {
        if parse_mistral_vibe_manifest(&mistral_vibe_manifest_path(config_path)?)?.is_some() {
            bail!("Mistral Vibe ownership manifest is still present");
        }
        let text = optional_text(config_path)?.unwrap_or_default();
        if !text.trim().is_empty() {
            let doc = text
                .parse::<toml_edit::DocumentMut>()
                .context("Mistral Vibe config must be valid TOML")?;
            // A restored prior provider/model is legitimate user state;
            // only the d-token loopback API key marker must be gone.
            let has_dtoken_key = doc
                .get("providers")
                .and_then(|v| v.get(MISTRAL_VIBE_PROVIDER_ID))
                .and_then(|v| v.get("api_key"))
                .and_then(|v| v.as_str())
                == Some(MISTRAL_VIBE_PUBLIC_API_KEY_MARKER);
            if has_dtoken_key {
                bail!("Mistral Vibe still contains a d-token routing API key marker");
            }
        }
        Ok(())
    })();
    Ok(AgentTransportVerification {
        agent_id: "mistral-vibe".into(),
        kind: AgentTransportAdapterKind::MistralVibe,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed Mistral Vibe transport is clear".into()),
        current_upstream: None,
    })
}

fn mistral_vibe_can_clear(config_path: &Path) -> bool {
    let manifest_path = match mistral_vibe_manifest_path(config_path) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let Ok(manifest) = parse_mistral_vibe_manifest(&manifest_path) else {
        return false;
    };
    let Some(manifest) = manifest else {
        return false;
    };
    let Ok(text) = optional_text(config_path) else {
        return false;
    };
    let text = text.unwrap_or_default();
    let Ok(selection) = mistral_vibe_selection(&text) else {
        return false;
    };
    let expected_active = format!("{}/{}", MISTRAL_VIBE_PROVIDER_ID, manifest.model);
    selection.model.as_deref() == Some(&expected_active)
}

fn transform_claude(
    text: &str,
    operation: AgentTransportOperation,
    endpoint: &str,
    model: Option<&str>,
) -> Result<String> {
    if operation == AgentTransportOperation::Clear && text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let mut root: Map<String, Value> = if text.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(text).context("Claude Code settings must be a JSON object")?
    };
    let marker = claude_marker(&root)?;
    match operation {
        AgentTransportOperation::Configure => {
            let model = model.context("Claude Code transport requires an explicit model")?;
            let current = claude_current_selection(&root)?;
            if let Some(existing) = marker.as_ref() {
                ensure_claude_current_matches(existing, &current)?;
                if existing.endpoint == endpoint && existing.model == model {
                    return Ok(text.to_string());
                }
            } else if claude_has_legacy_trace(&root)? {
                bail!(
                    "Claude Code has an unowned d-token selection; use rollback or Advanced setup"
                );
            }
            let prior = marker.map(|marker| marker.prior).unwrap_or(current);
            let env = root
                .entry(CLAUDE_ENV)
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .context("Claude Code env must be an object")?;
            env.insert(CLAUDE_BASE_URL.into(), Value::String(endpoint.into()));
            env.insert(CLAUDE_MODEL.into(), Value::String(model.into()));
            set_claude_marker(&mut root, endpoint, model, &prior)?;
        }
        AgentTransportOperation::Clear => {
            let Some(marker) = marker else {
                if claude_has_legacy_trace(&root)? {
                    bail!(
                        "Claude Code has an unowned d-token selection; automatic clear is unsafe"
                    );
                }
                return Ok(text.to_string());
            };
            ensure_claude_current_matches(&marker, &claude_current_selection(&root)?)?;
            let env = root
                .get_mut(CLAUDE_ENV)
                .and_then(Value::as_object_mut)
                .context("Claude Code env is missing while d-token transport is active")?;
            restore_json_selection(
                env,
                CLAUDE_BASE_URL,
                marker.prior.provider_present,
                marker.prior.provider,
            )?;
            restore_json_selection(
                env,
                CLAUDE_MODEL,
                marker.prior.model_present,
                marker.prior.model,
            )?;
            if env.is_empty() {
                root.remove(CLAUDE_ENV);
            }
            remove_claude_marker(&mut root)?;
        }
    }
    Ok(serde_json::to_string_pretty(&Value::Object(root))? + "\n")
}

fn transform_codex(
    text: &str,
    operation: AgentTransportOperation,
    endpoint: &str,
    model: Option<&str>,
) -> Result<String> {
    if operation == AgentTransportOperation::Clear && text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let mut document = if text.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        text.parse::<toml_edit::DocumentMut>()
            .context("Codex configuration is invalid TOML")?
    };
    let marker = codex_marker(&document)?;
    match operation {
        AgentTransportOperation::Configure => {
            let model = model.context("Codex transport requires an explicit model")?;
            let current = codex_current_selection(&document)?;
            if let Some(existing) = marker.as_ref() {
                ensure_codex_current_matches(existing, &document, &current)?;
                if existing.endpoint == endpoint && existing.model == model {
                    return Ok(text.to_string());
                }
            } else if codex_has_legacy_trace(&document)? {
                bail!("Codex has an unowned d-token provider or selection; use rollback or Advanced setup");
            }
            let prior = marker.map(|marker| marker.prior).unwrap_or(current);
            set_codex_provider(&mut document, endpoint)?;
            set_toml_selection(&mut document, "model_provider", Some(CODEX_PROVIDER_ID));
            set_toml_selection(&mut document, "model", Some(model));
            set_codex_marker(&mut document, endpoint, model, &prior)?;
        }
        AgentTransportOperation::Clear => {
            let Some(marker) = marker else {
                if codex_has_legacy_trace(&document)? {
                    bail!("Codex has an unowned d-token provider or selection; automatic clear is unsafe");
                }
                return Ok(text.to_string());
            };
            let current = codex_current_selection(&document)?;
            ensure_codex_current_matches(&marker, &document, &current)?;
            set_toml_selection(
                &mut document,
                "model_provider",
                marker.prior.provider.as_deref(),
            );
            set_toml_selection(&mut document, "model", marker.prior.model.as_deref());
            remove_codex_provider(&mut document)?;
            remove_codex_marker(&mut document)?;
        }
    }
    Ok(document.to_string())
}

#[derive(Clone)]
struct OwnedSelection {
    endpoint: String,
    model: String,
    prior: PriorSelection,
}

fn checked_string(object: &Map<String, Value>, key: &str) -> Result<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("d-token Claude Code ownership record is missing {key}"))
}

fn checked_bool(object: &Map<String, Value>, key: &str) -> Result<bool> {
    object
        .get(key)
        .and_then(Value::as_bool)
        .with_context(|| format!("d-token Claude Code ownership record is missing {key}"))
}

fn claude_marker(root: &Map<String, Value>) -> Result<Option<OwnedSelection>> {
    let Some(dtoken) = root.get(DTOKEN_MANAGED_KEY) else {
        return Ok(None);
    };
    let dtoken = dtoken
        .as_object()
        .context("d-token Claude Code namespace is not an object")?;
    let Some(marker) = dtoken.get("agent_transport") else {
        return Ok(None);
    };
    let marker = marker
        .as_object()
        .context("d-token Claude Code ownership record is not an object")?;
    let base_present = checked_bool(marker, "previous_base_url_present")?;
    let model_present = checked_bool(marker, "previous_model_present")?;
    let expected_keys = 8 + usize::from(base_present) + usize::from(model_present);
    if marker.len() != expected_keys
        || marker.get("schema_version").and_then(Value::as_i64) != Some(TRANSPORT_SCHEMA_VERSION)
        || marker.get("managed_by").and_then(Value::as_str) != Some(DTOKEN_MANAGED_BY)
        || marker.get("agent_id").and_then(Value::as_str) != Some("claude-code")
        || marker.get("transport").and_then(Value::as_str) != Some(DTOKEN_TRANSPORT)
    {
        bail!("Claude Code ownership record is incomplete or modified; automatic clear is unsafe");
    }
    let prior_base = base_present
        .then(|| checked_string(marker, "previous_base_url"))
        .transpose()?;
    let prior_model = model_present
        .then(|| checked_string(marker, "previous_model"))
        .transpose()?;
    if (!base_present && marker.contains_key("previous_base_url"))
        || (!model_present && marker.contains_key("previous_model"))
    {
        bail!("Claude Code ownership record is inconsistent; automatic clear is unsafe");
    }
    let endpoint = checked_string(marker, "endpoint")?;
    validate_endpoint(&endpoint)?;
    let model = checked_string(marker, "model")?;
    validate_model(Some(&model))?;
    validate_prior_selection(prior_base.as_deref(), prior_model.as_deref())?;
    Ok(Some(OwnedSelection {
        endpoint,
        model,
        prior: PriorSelection {
            provider_present: base_present,
            provider: prior_base,
            model_present,
            model: prior_model,
        },
    }))
}

fn claude_current_selection(root: &Map<String, Value>) -> Result<PriorSelection> {
    let Some(env) = root.get(CLAUDE_ENV) else {
        return Ok(PriorSelection {
            provider_present: false,
            provider: None,
            model_present: false,
            model: None,
        });
    };
    let env = env
        .as_object()
        .context("Claude Code env must be an object")?;
    let provider = json_optional_string(env, CLAUDE_BASE_URL)?;
    let model = json_optional_string(env, CLAUDE_MODEL)?;
    validate_prior_selection(provider.as_deref(), model.as_deref())?;
    Ok(PriorSelection {
        provider_present: provider.is_some(),
        provider,
        model_present: model.is_some(),
        model,
    })
}

fn json_optional_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("Claude Code {key} is not a string; refusing to alter selection"),
    }
}

fn validate_prior_selection(base_url: Option<&str>, model: Option<&str>) -> Result<()> {
    if let Some(base_url) = base_url {
        if contains_secret_like(base_url) || base_url.contains(['@', '?', '#', '\n', '\r', '\0']) {
            bail!("existing provider selection may contain credential material; automatic configuration is unsafe");
        }
    }
    if let Some(model) = model {
        validate_model(Some(model))?;
    }
    Ok(())
}

fn ensure_claude_current_matches(marker: &OwnedSelection, current: &PriorSelection) -> Result<()> {
    if current.provider.as_deref() != Some(marker.endpoint.as_str())
        || current.model.as_deref() != Some(marker.model.as_str())
    {
        bail!("Claude Code selection changed outside d-token; automatic clear is unsafe");
    }
    Ok(())
}

fn claude_has_legacy_trace(root: &Map<String, Value>) -> Result<bool> {
    Ok(claude_current_selection(root)?
        .provider
        .as_deref()
        .is_some_and(is_loopback_endpoint))
}

fn is_loopback_endpoint(value: &str) -> bool {
    value.starts_with("http://127.0.0.1:")
        || value.starts_with("http://localhost:")
        || value.starts_with("https://127.0.0.1:")
        || value.starts_with("https://localhost:")
}

fn set_claude_marker(
    root: &mut Map<String, Value>,
    endpoint: &str,
    model: &str,
    prior: &PriorSelection,
) -> Result<()> {
    let dtoken = root
        .entry(DTOKEN_MANAGED_KEY)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("d-token Claude Code namespace is not an object")?;
    let mut marker = Map::new();
    marker.insert(
        "schema_version".into(),
        Value::from(TRANSPORT_SCHEMA_VERSION),
    );
    marker.insert("managed_by".into(), Value::String(DTOKEN_MANAGED_BY.into()));
    marker.insert("agent_id".into(), Value::String("claude-code".into()));
    marker.insert("transport".into(), Value::String(DTOKEN_TRANSPORT.into()));
    marker.insert("endpoint".into(), Value::String(endpoint.into()));
    marker.insert("model".into(), Value::String(model.into()));
    marker.insert(
        "previous_base_url_present".into(),
        Value::Bool(prior.provider_present),
    );
    marker.insert(
        "previous_model_present".into(),
        Value::Bool(prior.model_present),
    );
    if let Some(value) = prior.provider.as_ref() {
        marker.insert("previous_base_url".into(), Value::String(value.clone()));
    }
    if let Some(value) = prior.model.as_ref() {
        marker.insert("previous_model".into(), Value::String(value.clone()));
    }
    dtoken.insert("agent_transport".into(), Value::Object(marker));
    Ok(())
}

fn remove_claude_marker(root: &mut Map<String, Value>) -> Result<()> {
    let dtoken = root
        .get_mut(DTOKEN_MANAGED_KEY)
        .and_then(Value::as_object_mut)
        .context("d-token Claude Code namespace is missing while transport is active")?;
    dtoken.remove("agent_transport");
    if dtoken.is_empty() {
        root.remove(DTOKEN_MANAGED_KEY);
    }
    Ok(())
}

fn restore_json_selection(
    env: &mut Map<String, Value>,
    key: &str,
    was_present: bool,
    previous: Option<String>,
) -> Result<()> {
    match (was_present, previous) {
        (true, Some(value)) => {
            env.insert(key.into(), Value::String(value));
        }
        (false, None) => {
            env.remove(key);
        }
        _ => bail!("Claude Code ownership record is inconsistent; automatic clear is unsafe"),
    }
    Ok(())
}

fn toml_optional_string(document: &toml_edit::DocumentMut, key: &str) -> Result<Option<String>> {
    match document.get(key) {
        None => Ok(None),
        Some(item) => {
            item.as_str().map(str::to_owned).map(Some).with_context(|| {
                format!("Codex {key} is not a string; refusing to alter selection")
            })
        }
    }
}

fn codex_current_selection(document: &toml_edit::DocumentMut) -> Result<PriorSelection> {
    let provider = toml_optional_string(document, "model_provider")?;
    let model = toml_optional_string(document, "model")?;
    validate_prior_selection(None, model.as_deref())?;
    Ok(PriorSelection {
        provider_present: provider.is_some(),
        provider,
        model_present: model.is_some(),
        model,
    })
}

fn codex_marker(document: &toml_edit::DocumentMut) -> Result<Option<OwnedSelection>> {
    let Some(dtoken) = document.get(DTOKEN_MANAGED_KEY) else {
        return Ok(None);
    };
    let dtoken = dtoken
        .as_table()
        .context("d-token Codex namespace is not a table")?;
    let Some(marker) = dtoken.get("agent_transport") else {
        return Ok(None);
    };
    let marker = marker
        .as_table()
        .context("d-token Codex ownership record is not a table")?;
    let provider_present = marker
        .get("previous_model_provider_present")
        .and_then(toml_edit::Item::as_bool)
        .context("d-token Codex ownership record is missing previous_model_provider_present")?;
    let model_present = marker
        .get("previous_model_present")
        .and_then(toml_edit::Item::as_bool)
        .context("d-token Codex ownership record is missing previous_model_present")?;
    let expected_keys = 8 + usize::from(provider_present) + usize::from(model_present);
    if marker.len() != expected_keys
        || marker
            .get("schema_version")
            .and_then(toml_edit::Item::as_integer)
            != Some(TRANSPORT_SCHEMA_VERSION)
        || marker.get("managed_by").and_then(toml_edit::Item::as_str) != Some(DTOKEN_MANAGED_BY)
        || marker.get("agent_id").and_then(toml_edit::Item::as_str) != Some("codex")
        || marker.get("transport").and_then(toml_edit::Item::as_str) != Some(DTOKEN_TRANSPORT)
    {
        bail!("Codex ownership record is incomplete or modified; automatic clear is unsafe");
    }
    let optional = |key: &str, present: bool| -> Result<Option<String>> {
        if !present {
            return Ok(None);
        }
        marker
            .get(key)
            .and_then(toml_edit::Item::as_str)
            .map(str::to_owned)
            .map(Some)
            .with_context(|| format!("Codex ownership record is missing {key}"))
    };
    let endpoint = marker
        .get("endpoint")
        .and_then(toml_edit::Item::as_str)
        .map(str::to_owned)
        .context("Codex ownership record is missing endpoint")?;
    let model = marker
        .get("model")
        .and_then(toml_edit::Item::as_str)
        .map(str::to_owned)
        .context("Codex ownership record is missing model")?;
    validate_endpoint(&endpoint)?;
    validate_model(Some(&model))?;
    let prior_provider = optional("previous_model_provider", provider_present)?;
    let prior_model = optional("previous_model", model_present)?;
    validate_prior_selection(None, prior_model.as_deref())?;
    Ok(Some(OwnedSelection {
        endpoint,
        model,
        prior: PriorSelection {
            provider_present,
            provider: prior_provider,
            model_present,
            model: prior_model,
        },
    }))
}

fn codex_has_legacy_trace(document: &toml_edit::DocumentMut) -> Result<bool> {
    Ok(
        toml_optional_string(document, "model_provider")?.as_deref() == Some(CODEX_PROVIDER_ID)
            || document
                .get("model_providers")
                .and_then(toml_edit::Item::as_table)
                .is_some_and(|providers| providers.contains_key(CODEX_PROVIDER_ID)),
    )
}

fn ensure_codex_current_matches(
    marker: &OwnedSelection,
    document: &toml_edit::DocumentMut,
    current: &PriorSelection,
) -> Result<()> {
    if current.provider.as_deref() != Some(CODEX_PROVIDER_ID)
        || current.model.as_deref() != Some(marker.model.as_str())
    {
        bail!("Codex selection changed outside d-token; automatic clear is unsafe");
    }
    ensure_owned_codex_provider(document, &marker.endpoint)
}

fn ensure_owned_codex_provider(document: &toml_edit::DocumentMut, endpoint: &str) -> Result<()> {
    let provider = document
        .get("model_providers")
        .and_then(toml_edit::Item::as_table)
        .and_then(|providers| providers.get(CODEX_PROVIDER_ID))
        .and_then(toml_edit::Item::as_table)
        .context("d-token Codex provider is absent")?;
    if provider.len() != 3
        || provider.get("name").and_then(toml_edit::Item::as_str) != Some("d-token local route")
        || provider.get("base_url").and_then(toml_edit::Item::as_str) != Some(endpoint)
        || provider.get("wire_api").and_then(toml_edit::Item::as_str) != Some("responses")
    {
        bail!("d-token Codex provider is incomplete or modified; automatic clear is unsafe");
    }
    Ok(())
}

fn set_codex_provider(document: &mut toml_edit::DocumentMut, endpoint: &str) -> Result<()> {
    let providers = document
        .entry("model_providers")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("Codex model_providers is not a table")?;
    let mut provider = toml_edit::Table::new();
    provider["name"] = toml_edit::value("d-token local route");
    provider["base_url"] = toml_edit::value(endpoint);
    provider["wire_api"] = toml_edit::value("responses");
    providers.insert(CODEX_PROVIDER_ID, toml_edit::Item::Table(provider));
    Ok(())
}

fn remove_codex_provider(document: &mut toml_edit::DocumentMut) -> Result<()> {
    let providers = document
        .get_mut("model_providers")
        .and_then(toml_edit::Item::as_table_mut)
        .context("d-token Codex provider table is missing while transport is active")?;
    providers.remove(CODEX_PROVIDER_ID);
    if providers.is_empty() {
        document.remove("model_providers");
    }
    Ok(())
}

fn set_toml_selection(document: &mut toml_edit::DocumentMut, key: &str, value: Option<&str>) {
    match value {
        Some(value) => document[key] = toml_edit::value(value),
        None => {
            document.remove(key);
        }
    }
}

fn set_codex_marker(
    document: &mut toml_edit::DocumentMut,
    endpoint: &str,
    model: &str,
    prior: &PriorSelection,
) -> Result<()> {
    let dtoken = document
        .entry(DTOKEN_MANAGED_KEY)
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("d-token Codex namespace is not a table")?;
    let marker = dtoken
        .entry("agent_transport")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("d-token Codex ownership record is not a table")?;
    marker.clear();
    marker["schema_version"] = toml_edit::value(TRANSPORT_SCHEMA_VERSION);
    marker["managed_by"] = toml_edit::value(DTOKEN_MANAGED_BY);
    marker["agent_id"] = toml_edit::value("codex");
    marker["transport"] = toml_edit::value(DTOKEN_TRANSPORT);
    marker["endpoint"] = toml_edit::value(endpoint);
    marker["model"] = toml_edit::value(model);
    marker["previous_model_provider_present"] = toml_edit::value(prior.provider_present);
    marker["previous_model_present"] = toml_edit::value(prior.model_present);
    if let Some(value) = prior.provider.as_deref() {
        marker["previous_model_provider"] = toml_edit::value(value);
    }
    if let Some(value) = prior.model.as_deref() {
        marker["previous_model"] = toml_edit::value(value);
    }
    Ok(())
}

fn remove_codex_marker(document: &mut toml_edit::DocumentMut) -> Result<()> {
    let dtoken = document
        .get_mut(DTOKEN_MANAGED_KEY)
        .and_then(toml_edit::Item::as_table_mut)
        .context("d-token Codex namespace is missing while transport is active")?;
    dtoken.remove("agent_transport");
    if dtoken.is_empty() {
        document.remove(DTOKEN_MANAGED_KEY);
    }
    Ok(())
}

fn transform_opencode(
    text: &str,
    operation: AgentTransportOperation,
    endpoint: &str,
    enabled: bool,
    model: Option<&str>,
) -> Result<String> {
    if operation == AgentTransportOperation::Clear && text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let mut root: Map<String, Value> = if text.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(text).context("OpenCode configuration must be strict JSON")?
    };
    if operation == AgentTransportOperation::Clear {
        let Some(providers) = root.get("provider") else {
            return Ok(text.to_string());
        };
        let providers = providers
            .as_object()
            .context("OpenCode provider must be an object")?;
        let Some(existing) = providers.get(OPENCODE_PROVIDER_ID) else {
            return Ok(text.to_string());
        };
        ensure_owned_opencode_provider(existing)?;
    }
    match operation {
        AgentTransportOperation::Configure => {
            let model = model.context("OpenCode transport requires an explicit model")?;
            let has_enabled_allowlist = root.contains_key("enabled_providers");
            // Validate any user-owned provider lists before staging a change.
            let _ =
                opencode_provider_list_contains(&root, "enabled_providers", OPENCODE_PROVIDER_ID)?;
            if !enabled && !has_enabled_allowlist {
                bail!(
                    "OpenCode cannot safely configure a disabled provider without an existing enabled_providers allowlist"
                );
            }
            if opencode_provider_list_contains(&root, "disabled_providers", OPENCODE_PROVIDER_ID)? {
                bail!("dtoken-local-route is explicitly disabled in OpenCode");
            }
            let providers = root
                .entry("provider".to_string())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .context("OpenCode provider must be an object")?;
            if let Some(existing) = providers.get(OPENCODE_PROVIDER_ID) {
                ensure_owned_opencode_provider(existing)?;
            }
            providers.insert(
                OPENCODE_PROVIDER_ID.into(),
                opencode_provider_value(endpoint, model),
            );
            if let Some(entries) = opencode_provider_list(&mut root, "enabled_providers")? {
                entries.retain(|entry| entry.as_str() != Some(OPENCODE_PROVIDER_ID));
                if enabled {
                    entries.push(Value::String(OPENCODE_PROVIDER_ID.into()));
                }
            }
        }
        AgentTransportOperation::Clear => {
            let mut removed_owned_provider = false;
            if let Some(providers) = root.get_mut("provider") {
                let providers = providers
                    .as_object_mut()
                    .context("OpenCode provider must be an object")?;
                if let Some(existing) = providers.get(OPENCODE_PROVIDER_ID) {
                    ensure_owned_opencode_provider(existing)?;
                    providers.remove(OPENCODE_PROVIDER_ID);
                    removed_owned_provider = true;
                }
                if providers.is_empty() {
                    root.remove("provider");
                }
            }
            if removed_owned_provider {
                if let Some(entries) = opencode_provider_list(&mut root, "enabled_providers")? {
                    entries.retain(|entry| entry.as_str() != Some(OPENCODE_PROVIDER_ID));
                }
            }
        }
    }
    Ok(serde_json::to_string_pretty(&Value::Object(root))? + "\n")
}

/// OpenCode documents `enabled_providers` as a global allowlist. We only
/// modify it when it already exists, preserving every user-selected provider.
fn opencode_provider_list<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> Result<Option<&'a mut Vec<Value>>> {
    let Some(value) = root.get_mut(key) else {
        return Ok(None);
    };
    let entries = value
        .as_array_mut()
        .with_context(|| format!("OpenCode {key} must be an array"))?;
    if !entries.iter().all(Value::is_string) {
        bail!("OpenCode {key} must contain only strings");
    }
    Ok(Some(entries))
}

fn opencode_provider_list_contains(root: &Map<String, Value>, key: &str, id: &str) -> Result<bool> {
    let Some(value) = root.get(key) else {
        return Ok(false);
    };
    let entries = value
        .as_array()
        .with_context(|| format!("OpenCode {key} must be an array"))?;
    if !entries.iter().all(Value::is_string) {
        bail!("OpenCode {key} must contain only strings");
    }
    Ok(entries.iter().any(|entry| entry.as_str() == Some(id)))
}

/// Canonical OpenCode `dtoken-local-route` provider value. Both the managed
/// transport and the dedicated desktop connection flow must build the provider
/// through this single Rust-owned builder so the two never drift apart.
pub fn opencode_provider_value(endpoint: &str, model: &str) -> Value {
    let base_url = opencode_base_url(endpoint);
    serde_json::json!({
        "npm": OPENCODE_PROVIDER_NPM,
        "name": OPENCODE_PROVIDER_NAME,
        "options": {
            "baseURL": base_url,
            "apiKey": OPENCODE_TOKEN_REFERENCE,
        },
        "models": {
            model: { "name": format!("d-token routed {model}") },
        },
    })
}

/// Canonical OpenCode base URL construction: a loopback endpoint plus `/v1`.
pub fn opencode_base_url(endpoint: &str) -> String {
    format!("{}/v1", endpoint.trim_end_matches('/'))
}

/// The provider ID alone is not enough ownership proof: only the d-token
/// provider shape (including its opaque environment reference) may be replaced
/// or removed. User-created entries with the same key fail closed. This is the
/// single canonical ownership predicate shared with the desktop adapter.
pub fn ensure_owned_opencode_provider(value: &Value) -> Result<()> {
    let provider = value
        .as_object()
        .context("OpenCode dtoken-local-route provider must be an object")?;
    if provider.len() != 4
        || !["npm", "name", "options", "models"]
            .iter()
            .all(|key| provider.contains_key(*key))
        || value.get("npm").and_then(Value::as_str) != Some(OPENCODE_PROVIDER_NPM)
        || value.get("name").and_then(Value::as_str) != Some(OPENCODE_PROVIDER_NAME)
        || value.pointer("/options/apiKey").and_then(Value::as_str)
            != Some(OPENCODE_TOKEN_REFERENCE)
    {
        bail!("OpenCode dtoken-local-route provider is not provably d-token-owned");
    }
    let options = value
        .get("options")
        .and_then(Value::as_object)
        .context("OpenCode dtoken-local-route options must be an object")?;
    if options.len() != 2
        || !["apiKey", "baseURL"]
            .iter()
            .all(|key| options.contains_key(*key))
        || value
            .pointer("/options/baseURL")
            .and_then(Value::as_str)
            .is_none()
    {
        bail!("OpenCode dtoken-local-route provider is not provably d-token-owned");
    }
    let models = value
        .get("models")
        .and_then(Value::as_object)
        .context("OpenCode dtoken-local-route models must be an object")?;
    let Some((model, definition)) = (models.len() == 1).then(|| models.iter().next()).flatten()
    else {
        bail!("OpenCode dtoken-local-route provider is not provably d-token-owned");
    };
    if definition.as_object().is_none_or(|definition| {
        definition.len() != 1
            || definition.get("name").and_then(Value::as_str)
                != Some(format!("d-token routed {model}").as_str())
    }) {
        bail!("OpenCode dtoken-local-route provider is not provably d-token-owned");
    }
    Ok(())
}

fn verify_cleared(
    descriptor: &AgentTransportAdapterDescriptor,
    text: &str,
) -> Result<AgentTransportVerification> {
    let result: Result<()> = match descriptor.kind {
        AgentTransportAdapterKind::ClaudeCode => verify_claude_cleared(text),
        AgentTransportAdapterKind::Codex => verify_codex_cleared(text),
        AgentTransportAdapterKind::ClineCli
        | AgentTransportAdapterKind::Goose
        | AgentTransportAdapterKind::KiloCode
        | AgentTransportAdapterKind::OpenClaw
        | AgentTransportAdapterKind::GeminiCli
        | AgentTransportAdapterKind::RooCode
        | AgentTransportAdapterKind::Continue
        | AgentTransportAdapterKind::Aider
        | AgentTransportAdapterKind::Hermes
        | AgentTransportAdapterKind::OpenHands
        | AgentTransportAdapterKind::Pi
        | AgentTransportAdapterKind::MistralVibe => {
            bail!("Agent provider schema requires filesystem context")
        }
        AgentTransportAdapterKind::JsonEnv
        | AgentTransportAdapterKind::Toml
        | AgentTransportAdapterKind::ExtensionSetting => {
            bail!("Agent provider schema is unavailable")
        }
        AgentTransportAdapterKind::OpenCodeProvider => verify_opencode_cleared(text),
        AgentTransportAdapterKind::CortexCodeHook
        | AgentTransportAdapterKind::CursorMcp
        | AgentTransportAdapterKind::ClaudeDesktopMcp
        | AgentTransportAdapterKind::WindsurfMcp
        | AgentTransportAdapterKind::CommandCodeMcp
        | AgentTransportAdapterKind::CopilotCliMcp
        | AgentTransportAdapterKind::VscodeCopilotChatMcp
        | AgentTransportAdapterKind::AntigravityMcp => bail!(MCP_TRANSPORT_FAIL_CLOSED),
        AgentTransportAdapterKind::DedicatedUnavailable => {
            bail!("Agent provider schema is unavailable")
        }
    };
    Ok(AgentTransportVerification {
        agent_id: descriptor.id.into(),
        kind: descriptor.kind,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed transport is clear".into()),
        current_upstream: None,
    })
}

fn verify_document(
    descriptor: &AgentTransportAdapterDescriptor,
    text: &str,
    endpoint: &str,
    enabled: bool,
    model: Option<&str>,
) -> Result<AgentTransportVerification> {
    let result: Result<()> = match descriptor.kind {
        AgentTransportAdapterKind::ClaudeCode => verify_claude(text, endpoint, model),
        AgentTransportAdapterKind::Codex => verify_codex(text, endpoint, model),
        AgentTransportAdapterKind::ClineCli
        | AgentTransportAdapterKind::Goose
        | AgentTransportAdapterKind::KiloCode
        | AgentTransportAdapterKind::OpenClaw
        | AgentTransportAdapterKind::GeminiCli
        | AgentTransportAdapterKind::RooCode
        | AgentTransportAdapterKind::Continue
        | AgentTransportAdapterKind::Aider
        | AgentTransportAdapterKind::Hermes
        | AgentTransportAdapterKind::OpenHands
        | AgentTransportAdapterKind::Pi
        | AgentTransportAdapterKind::MistralVibe => Err(anyhow::anyhow!(
            "Agent provider schema requires filesystem context"
        )),
        AgentTransportAdapterKind::JsonEnv
        | AgentTransportAdapterKind::Toml
        | AgentTransportAdapterKind::ExtensionSetting => {
            Err(anyhow::anyhow!("Agent provider schema is unavailable"))
        }
        AgentTransportAdapterKind::OpenCodeProvider => {
            verify_opencode(text, endpoint, enabled, model)
        }
        AgentTransportAdapterKind::CortexCodeHook
        | AgentTransportAdapterKind::CursorMcp
        | AgentTransportAdapterKind::ClaudeDesktopMcp
        | AgentTransportAdapterKind::WindsurfMcp
        | AgentTransportAdapterKind::CommandCodeMcp
        | AgentTransportAdapterKind::CopilotCliMcp
        | AgentTransportAdapterKind::VscodeCopilotChatMcp
        | AgentTransportAdapterKind::AntigravityMcp => {
            Err(anyhow::anyhow!(MCP_TRANSPORT_FAIL_CLOSED))
        }
        AgentTransportAdapterKind::DedicatedUnavailable => {
            Err(anyhow::anyhow!("Agent provider schema is unavailable"))
        }
    };
    let current_upstream = if result.is_ok() {
        current_upstream_signature(descriptor, text)?
    } else {
        None
    };
    Ok(AgentTransportVerification {
        agent_id: descriptor.id.into(),
        kind: descriptor.kind,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "d-token managed transport verified".into()),
        current_upstream,
    })
}

fn verify_claude(text: &str, endpoint: &str, model: Option<&str>) -> Result<()> {
    let root: Map<String, Value> =
        serde_json::from_str(text).context("Claude Code settings must be a JSON object")?;
    let marker = claude_marker(&root)?.context("d-token Claude Code ownership record is absent")?;
    if marker.endpoint != endpoint || Some(marker.model.as_str()) != model {
        bail!("d-token Claude Code selection does not match the reviewed request");
    }
    ensure_claude_current_matches(&marker, &claude_current_selection(&root)?)
}

fn verify_codex(text: &str, endpoint: &str, model: Option<&str>) -> Result<()> {
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .context("Codex configuration is invalid TOML")?;
    let marker = codex_marker(&document)?.context("d-token Codex ownership record is absent")?;
    if marker.endpoint != endpoint || Some(marker.model.as_str()) != model {
        bail!("d-token Codex selection does not match the reviewed request");
    }
    ensure_codex_current_matches(&marker, &document, &codex_current_selection(&document)?)
}

fn verify_claude_cleared(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let root: Map<String, Value> =
        serde_json::from_str(text).context("Claude Code settings must be a JSON object")?;
    if claude_marker(&root)?.is_some() || claude_has_legacy_trace(&root)? {
        bail!("d-token Claude Code transport is still present");
    }
    Ok(())
}

fn verify_codex_cleared(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .context("Codex configuration is invalid TOML")?;
    if codex_marker(&document)?.is_some() || codex_has_legacy_trace(&document)? {
        bail!("d-token Codex transport is still present");
    }
    Ok(())
}

fn verify_opencode(text: &str, endpoint: &str, enabled: bool, model: Option<&str>) -> Result<()> {
    let model = model.context("OpenCode transport requires an explicit model")?;
    let root: Value =
        serde_json::from_str(text).context("OpenCode configuration must be strict JSON")?;
    let provider = root
        .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
        .context("d-token OpenCode provider is absent")?;
    ensure_owned_opencode_provider(provider)?;
    if provider.pointer("/options/baseURL").and_then(Value::as_str)
        != Some(opencode_base_url(endpoint).as_str())
        || provider
            .pointer("/models")
            .and_then(Value::as_object)
            .is_none_or(|models| !models.contains_key(model))
    {
        bail!("d-token OpenCode provider does not match requested endpoint/model");
    }
    if opencode_provider_list_contains(
        root.as_object()
            .context("OpenCode configuration must be an object")?,
        "disabled_providers",
        OPENCODE_PROVIDER_ID,
    )? {
        bail!("d-token OpenCode provider is explicitly disabled");
    }
    let enabled_entries = root.get("enabled_providers");
    let present = opencode_provider_list_contains(
        root.as_object()
            .context("OpenCode configuration must be an object")?,
        "enabled_providers",
        OPENCODE_PROVIDER_ID,
    )?;
    if enabled_entries.is_some() && enabled != present {
        bail!(if enabled {
            "d-token OpenCode provider is not enabled"
        } else {
            "d-token OpenCode provider is unexpectedly enabled"
        });
    }
    if !enabled && enabled_entries.is_none() {
        bail!("OpenCode cannot verify a disabled provider without an enabled_providers allowlist");
    }
    Ok(())
}

fn verify_opencode_cleared(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let root: Value =
        serde_json::from_str(text).context("OpenCode configuration must be strict JSON")?;
    if root
        .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
        .is_some()
    {
        bail!("d-token OpenCode provider is still present");
    }
    Ok(())
}

fn current_upstream_signature(
    descriptor: &AgentTransportAdapterDescriptor,
    text: &str,
) -> Result<Option<AgentTransportUpstreamSignature>> {
    let (endpoint, model, enabled) = match descriptor.kind {
        AgentTransportAdapterKind::ClaudeCode => {
            let root: Map<String, Value> =
                serde_json::from_str(text).context("Claude Code settings must be a JSON object")?;
            let Some(marker) = claude_marker(&root)? else {
                return Ok(None);
            };
            ensure_claude_current_matches(&marker, &claude_current_selection(&root)?)?;
            (marker.endpoint, Some(marker.model), true)
        }
        AgentTransportAdapterKind::Codex => {
            let document = text
                .parse::<toml_edit::DocumentMut>()
                .context("Codex configuration is invalid TOML")?;
            let Some(marker) = codex_marker(&document)? else {
                return Ok(None);
            };
            ensure_codex_current_matches(&marker, &document, &codex_current_selection(&document)?)?;
            (marker.endpoint, Some(marker.model), true)
        }
        AgentTransportAdapterKind::OpenCodeProvider => {
            let root: Value =
                serde_json::from_str(text).context("OpenCode configuration must be strict JSON")?;
            let Some(provider) = root.pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}")) else {
                return Ok(None);
            };
            ensure_owned_opencode_provider(provider)?;
            let endpoint = provider
                .pointer("/options/baseURL")
                .and_then(Value::as_str)
                .context("d-token OpenCode provider endpoint is absent")?;
            let endpoint = endpoint
                .strip_suffix("/v1")
                .context("d-token OpenCode provider endpoint is invalid")?;
            validate_endpoint(endpoint)?;
            let model = provider
                .get("models")
                .and_then(Value::as_object)
                .and_then(|models| {
                    (models.len() == 1)
                        .then(|| models.keys().next().cloned())
                        .flatten()
                });
            let object = root
                .as_object()
                .context("OpenCode configuration must be an object")?;
            let explicitly_disabled = opencode_provider_list_contains(
                object,
                "disabled_providers",
                OPENCODE_PROVIDER_ID,
            )?;
            let enabled = if root.get("enabled_providers").is_some() {
                opencode_provider_list_contains(object, "enabled_providers", OPENCODE_PROVIDER_ID)?
                    && !explicitly_disabled
            } else {
                !explicitly_disabled
            };
            (endpoint.to_string(), model, enabled)
        }
        AgentTransportAdapterKind::ClineCli
        | AgentTransportAdapterKind::Goose
        | AgentTransportAdapterKind::KiloCode
        | AgentTransportAdapterKind::OpenClaw
        | AgentTransportAdapterKind::GeminiCli
        | AgentTransportAdapterKind::RooCode
        | AgentTransportAdapterKind::Continue
        | AgentTransportAdapterKind::Aider
        | AgentTransportAdapterKind::Hermes
        | AgentTransportAdapterKind::OpenHands
        | AgentTransportAdapterKind::Pi
        | AgentTransportAdapterKind::MistralVibe => return Ok(None),
        AgentTransportAdapterKind::JsonEnv
        | AgentTransportAdapterKind::Toml
        | AgentTransportAdapterKind::ExtensionSetting
        | AgentTransportAdapterKind::CortexCodeHook
        | AgentTransportAdapterKind::CursorMcp
        | AgentTransportAdapterKind::ClaudeDesktopMcp
        | AgentTransportAdapterKind::WindsurfMcp
        | AgentTransportAdapterKind::CommandCodeMcp
        | AgentTransportAdapterKind::CopilotCliMcp
        | AgentTransportAdapterKind::VscodeCopilotChatMcp
        | AgentTransportAdapterKind::AntigravityMcp
        | AgentTransportAdapterKind::DedicatedUnavailable => return Ok(None),
    };
    Ok(Some(AgentTransportUpstreamSignature {
        provider_id: match descriptor.kind {
            AgentTransportAdapterKind::OpenCodeProvider => OPENCODE_PROVIDER_ID.to_string(),
            AgentTransportAdapterKind::ClaudeCode => "anthropic-env".to_string(),
            AgentTransportAdapterKind::Codex => CODEX_PROVIDER_ID.to_string(),
            _ => DTOKEN_MANAGED_KEY.to_string(),
        },
        signature_sha256: crate::stable_hash(&format!(
            "agent-upstream-v1\n{}\n{}\n{}\n{}",
            descriptor.id,
            endpoint,
            model.as_deref().unwrap_or(""),
            enabled
        )),
        endpoint,
        model,
        enabled,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace_tempdir() -> tempfile::TempDir {
        tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap()
    }

    fn request(agent_id: &str, path: PathBuf) -> AgentTransportRequest {
        AgentTransportRequest {
            agent_id: agent_id.into(),
            target_path: path,
            endpoint: "http://127.0.0.1:8787".into(),
            enabled: true,
            model: Some("claude-sonnet-test".into()),
        }
    }

    fn reviewed_apply(
        request: &AgentTransportRequest,
        operation: AgentTransportOperation,
        backups: &Path,
    ) {
        let review = match operation {
            AgentTransportOperation::Configure => preview_configure_agent_transport(request),
            AgentTransportOperation::Clear => preview_clear_agent_transport(request),
        }
        .expect("transport preview");
        let receipt = apply_reviewed_agent_transport(
            request,
            operation,
            backups,
            &review.revision,
            &review.before_sha256,
            &review.after_sha256,
        )
        .expect("reviewed transport apply");
        assert!(receipt.verification.ok);
    }

    #[test]
    fn claude_code_configure_clear_restores_exact_previous_selection() {
        let root = workspace_tempdir();
        let path = root.path().join("settings.json");
        fs::write(&path, r#"{"theme":"dark","env":{"ANTHROPIC_BASE_URL":"https://claude.example/v1","ANTHROPIC_MODEL":"claude-old","ANTHROPIC_API_KEY":"redacted-test-secret"}}"#).unwrap();
        let req = request("claude-code", path.clone());
        let backup = root.path().join("backups");
        let configured_receipt = configure_agent_transport(&req, &backup).unwrap();
        assert!(configured_receipt.verification.ok);
        let configured = fs::read_to_string(&path).unwrap();
        assert!(configured.contains("ANTHROPIC_BASE_URL"));
        assert!(configured.contains("\"theme\": \"dark\""));
        assert!(!serde_json::to_string(&configured_receipt)
            .unwrap()
            .contains("redacted-test-secret"));
        let clear_receipt = clear_agent_transport(&req, &backup).unwrap();
        assert!(clear_receipt.verification.ok);
        let cleared: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(
            cleared
                .pointer("/env/ANTHROPIC_BASE_URL")
                .and_then(Value::as_str),
            Some("https://claude.example/v1")
        );
        assert_eq!(
            cleared
                .pointer("/env/ANTHROPIC_MODEL")
                .and_then(Value::as_str),
            Some("claude-old")
        );
        assert_eq!(
            cleared
                .pointer("/env/ANTHROPIC_API_KEY")
                .and_then(Value::as_str),
            Some("redacted-test-secret")
        );
        assert!(cleared.pointer("/dtoken/agent_transport").is_none());
    }

    #[test]
    fn agent_transport_preview_exposes_a_redacted_diff_only() {
        const SECRET: &str = "AGENT_TRANSPORT_PREVIEW_SECRET";
        let root = workspace_tempdir();
        let path = root.path().join("settings.json");
        fs::write(
            &path,
            format!(
                r#"{{"env":{{"ANTHROPIC_BASE_URL":"https://claude.example/v1","ANTHROPIC_MODEL":"claude-old","ANTHROPIC_API_KEY":"{SECRET}"}}}}"#
            ),
        )
        .unwrap();
        let review = preview_configure_agent_transport(&request("claude-code", path))
            .expect("preview Claude transport");

        assert!(review.changed);
        assert!(review.redacted_diff.contains("ANTHROPIC_BASE_URL"));
        assert!(!review.redacted_diff.contains(SECRET));
        assert!(!serde_json::to_string(&review)
            .expect("serialize review")
            .contains(SECRET));
    }

    #[test]
    fn codex_configure_clear_restores_previous_provider_without_touching_others() {
        let root = workspace_tempdir();
        let path = root.path().join("config.toml");
        fs::write(&path, "model_provider = \"openai\"\nmodel = \"gpt-old\"\n[model_providers.openai]\nname = \"OpenAI\"\nbase_url = \"https://api.example/v1\"\nwire_api = \"responses\"\n").unwrap();
        let req = request("codex", path.clone());
        let backup = root.path().join("backups");
        configure_agent_transport(&req, &backup).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("model_provider = \"dtoken\""));
        assert!(after.contains("[model_providers.dtoken]"));
        clear_agent_transport(&req, &backup).unwrap();
        let cleared = fs::read_to_string(&path).unwrap();
        assert!(cleared.contains("model_provider = \"openai\""));
        assert!(cleared.contains("model = \"gpt-old\""));
        assert!(cleared.contains("[model_providers.openai]"));
        assert!(!cleared.contains("[model_providers.dtoken]"));
    }

    #[test]
    fn goose_configure_clear_restores_previous_selection_and_deletes_owned_files() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".config/goose");
        fs::create_dir_all(config_dir.join("custom_providers")).unwrap();
        let path = config_dir.join("config.yaml");
        fs::write(
            &path,
            "GOOSE_PROVIDER: anthropic\nGOOSE_MODEL: claude-sonnet\nextensions:\n  developer:\n    enabled: true\n",
        )
        .unwrap();
        let mut req = request("goose", path.clone());
        req.model = Some("gpt-5.6".into());
        let backup = root.path().join("backups");

        let receipt = configure_agent_transport(&req, &backup).unwrap();
        assert!(receipt.verification.ok);
        let configured = fs::read_to_string(&path).unwrap();
        assert!(configured.contains("GOOSE_PROVIDER: dtoken-local-route"));
        assert!(configured.contains("GOOSE_MODEL: gpt-5.6"));
        let provider_path = config_dir.join("custom_providers/dtoken-local-route.json");
        let provider = fs::read_to_string(&provider_path).unwrap();
        assert!(provider.contains("\"engine\": \"openai\""));
        assert!(provider.contains("\"requires_auth\": false"));
        let manifest_path = config_dir.join(GOOSE_MANIFEST_FILE);
        assert!(manifest_path.exists());
        assert!(can_clear_agent_transport("goose", &path));

        let cleared = clear_agent_transport(&req, &backup).unwrap();
        assert!(cleared.verification.ok);
        let restored = fs::read_to_string(&path).unwrap();
        assert!(restored.contains("GOOSE_PROVIDER: anthropic"));
        assert!(restored.contains("GOOSE_MODEL: claude-sonnet"));
        assert!(!provider_path.exists());
        assert!(!manifest_path.exists());
    }

    #[test]
    fn kilo_configure_clear_restores_previous_model_and_omits_api_key() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".kilocode");
        fs::create_dir_all(&config_dir).unwrap();
        let path = config_dir.join("settings.json");
        fs::write(
            &path,
            r#"{"model":"anthropic/claude-sonnet-4","provider":{"anthropic":{"name":"Anthropic"}},"snapshot":true}"#,
        )
        .unwrap();
        let mut req = request("kilo-code", path.clone());
        req.model = Some("gpt-5.6".into());
        let backup = root.path().join("backups");

        let receipt = configure_agent_transport(&req, &backup).unwrap();
        assert!(receipt.verification.ok);
        let configured: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            configured.get("model").and_then(Value::as_str),
            Some("dtoken-local-route/gpt-5.6")
        );
        assert_eq!(
            configured
                .pointer("/provider/dtoken-local-route/options/baseURL")
                .and_then(Value::as_str),
            Some("http://127.0.0.1:8787")
        );
        assert!(configured
            .pointer("/provider/dtoken-local-route/options/apiKey")
            .is_none());
        let manifest_path = config_dir.join(KILO_MANIFEST_FILE);
        assert!(manifest_path.exists());
        assert!(can_clear_agent_transport("kilo-code", &path));

        let cleared = clear_agent_transport(&req, &backup).unwrap();
        assert!(cleared.verification.ok);
        let restored: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            restored.get("model").and_then(Value::as_str),
            Some("anthropic/claude-sonnet-4")
        );
        assert!(restored.pointer("/provider/dtoken-local-route").is_none());
        assert!(restored.pointer("/provider/anthropic/name").is_some());
        assert!(!manifest_path.exists());
    }

    #[test]
    fn adapter_operation_matrix_is_explicit_and_fail_closed() {
        let expected = [
            ("opencode", true, true),
            ("claude-code", true, true),
            ("codex", true, true),
            ("goose", true, true),
            ("kilo-code", true, true),
            ("cursor", false, false),
            ("gemini-cli", true, true),
            ("cline", true, true),
            ("openclaw", true, true),
            ("commandcode", false, false),
            ("claude-desktop", false, false),
            ("hermes", true, true),
            ("roo-code", true, true),
            ("continue", true, true),
            ("aider", true, true),
            ("openhands", true, true),
            ("copilot-cli", false, false),
            ("vscode-copilot-chat", false, false),
            ("pi", true, true),
            ("cortex-code", false, false),
            ("windsurf", false, false),
            ("antigravity", false, false),
            ("mistral-vibe", true, true),
        ];
        for (id, can_configure, can_clear) in expected {
            let adapter = agent_transport_adapter(id).expect("known Agent adapter");
            assert_eq!(adapter.can_configure, can_configure, "{id} configure");
            assert_eq!(adapter.can_clear, can_clear, "{id} clear");
            if !can_configure {
                assert!(
                    adapter.advanced_required_reason.is_some(),
                    "{id} must expose an explicit fail-closed reason"
                );
            }
        }
        let mut detected_ids = crate::agent::known_agent_adapters()
            .iter()
            .map(|adapter| adapter.id)
            .collect::<Vec<_>>();
        let mut transport_ids = known_agent_transport_adapters()
            .iter()
            .map(|adapter| adapter.id)
            .collect::<Vec<_>>();
        detected_ids.sort_unstable();
        transport_ids.sort_unstable();
        assert_eq!(
            transport_ids, detected_ids,
            "every detected Agent needs an explicit transport capability decision"
        );
    }

    #[test]
    fn unverified_agents_refuse_generic_metadata_configure_and_clear() {
        let root = workspace_tempdir();
        let path = root.path().join("settings.json");
        let original = r#"{"dtoken":true}"#;
        fs::write(&path, original).unwrap();
        for agent_id in [
            "cursor",
            "commandcode",
            "claude-desktop",
            "copilot-cli",
            "vscode-copilot-chat",
            "cortex-code",
            "windsurf",
            "antigravity",
        ] {
            let req = request(agent_id, path.clone());
            for result in [
                configure_agent_transport(&req, &root.path().join("backups")),
                clear_agent_transport(&req, &root.path().join("backups")),
            ] {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("advanced_required"));
            }
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
        }
        let mut manual = request("opencode", root.path().join("opencode.json"));
        manual.model = None;
        assert!(configure_agent_transport(&manual, &root.path().join("backups")).is_err());
    }

    #[test]
    fn gemini_cli_configure_clear_restores_previous_env_and_auth() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".gemini");
        fs::create_dir_all(&config_dir).unwrap();
        let settings_path = config_dir.join("settings.json");
        let env_path = config_dir.join(".env");
        fs::write(
            &settings_path,
            r#"{"general":{"vimMode":true},"security":{"auth":{"selectedType":"google-login"}}}"#,
        )
        .unwrap();
        fs::write(
            &env_path,
            "CUSTOM_VAR=keep\nGEMINI_MODEL=gemini-old\nGOOGLE_GEMINI_BASE_URL=https://prior.example\n",
        )
        .unwrap();
        let req = request("gemini-cli", settings_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        assert!(verify_agent_transport(&req).unwrap().ok);
        assert!(can_clear_agent_transport("gemini-cli", &settings_path));

        let configured: Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
        assert_eq!(
            configured
                .pointer("/security/auth/selectedType")
                .and_then(Value::as_str),
            Some("gemini-api-key")
        );
        let env = fs::read_to_string(&env_path).unwrap();
        assert!(env.contains("CUSTOM_VAR=keep"));
        assert!(env.contains("GEMINI_MODEL=claude-sonnet-test"));
        assert!(env.contains("GOOGLE_GEMINI_BASE_URL=http://127.0.0.1:8787"));
        assert!(env.contains("GEMINI_API_KEY=dtoken-loopback"));

        reviewed_apply(
            &req,
            AgentTransportOperation::Clear,
            &root.path().join("backups"),
        );
        let restored: Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
        assert_eq!(
            restored
                .pointer("/security/auth/selectedType")
                .and_then(Value::as_str),
            Some("google-login")
        );
        let restored_env = fs::read_to_string(&env_path).unwrap();
        assert!(restored_env.contains("CUSTOM_VAR=keep"));
        assert!(restored_env.contains("GEMINI_MODEL=gemini-old"));
        assert!(restored_env.contains("GOOGLE_GEMINI_BASE_URL=https://prior.example"));
        assert!(!restored_env.contains("GEMINI_API_KEY"));
        assert!(!can_clear_agent_transport("gemini-cli", &settings_path));
        assert!(!config_dir
            .join("dtoken-agent-transport-gemini.json")
            .exists());
    }

    #[test]
    fn gemini_cli_refuses_credential_collision_and_external_edit() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".gemini");
        fs::create_dir_all(&config_dir).unwrap();
        let settings_path = config_dir.join("settings.json");
        let env_path = config_dir.join(".env");
        fs::write(&settings_path, r#"{}"#).unwrap();
        fs::write(&env_path, "GEMINI_API_KEY=user-real-secret\n").unwrap();
        let req = request("gemini-cli", settings_path.clone());
        assert!(
            configure_agent_transport(&req, &root.path().join("backups"))
                .unwrap_err()
                .to_string()
                .contains("credential")
        );

        fs::write(&env_path, "GEMINI_MODEL=gemini-old\n").unwrap();
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        fs::write(&env_path, "GEMINI_MODEL=gemini-tampered\n").unwrap();
        assert!(!can_clear_agent_transport("gemini-cli", &settings_path));
        assert!(clear_agent_transport(&req, &root.path().join("backups"))
            .unwrap_err()
            .to_string()
            .contains("changed outside d-token"));
    }

    #[test]
    fn roo_code_configure_clear_preserves_other_settings_and_profile() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".roo");
        fs::create_dir_all(&config_dir).unwrap();
        let settings_path = config_dir.join("settings.json");
        fs::write(
            &settings_path,
            r#"{"mode":"code","customInstructions":"keep","apiProvider":"anthropic","openAiModelId":"claude-old","listApiConfigMeta":[{"id":"user","name":"User","apiProvider":"anthropic"}]}"#,
        )
        .unwrap();
        let req = request("roo-code", settings_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        assert!(verify_agent_transport(&req).unwrap().ok);
        assert!(can_clear_agent_transport("roo-code", &settings_path));

        let configured: Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
        assert_eq!(configured["apiProvider"], json!("openai"));
        assert_eq!(
            configured["openAiBaseUrl"],
            json!("http://127.0.0.1:8787/v1")
        );
        assert_eq!(configured["openAiModelId"], json!("claude-sonnet-test"));
        assert_eq!(configured["openAiApiKey"], json!("dtoken-loopback"));
        assert_eq!(
            configured["currentApiConfigName"],
            json!("d-token local route")
        );
        assert_eq!(configured["mode"], json!("code"));
        assert_eq!(configured["customInstructions"], json!("keep"));
        assert_eq!(configured["listApiConfigMeta"].as_array().unwrap().len(), 2);

        reviewed_apply(
            &req,
            AgentTransportOperation::Clear,
            &root.path().join("backups"),
        );
        let restored: Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
        assert_eq!(restored["apiProvider"], json!("anthropic"));
        assert_eq!(restored["openAiModelId"], json!("claude-old"));
        assert!(restored.get("openAiApiKey").is_none());
        assert!(restored.get("currentApiConfigName").is_none());
        assert_eq!(
            restored["listApiConfigMeta"],
            json!([{"id":"user","name":"User","apiProvider":"anthropic"}])
        );
        assert_eq!(restored["mode"], json!("code"));
        assert!(!config_dir.join("dtoken-agent-transport-roo.json").exists());
    }

    #[test]
    fn continue_configure_clear_restores_models_and_default() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".continue");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.json");
        fs::write(
            &config_path,
            r#"{"models":[{"title":"Prior","provider":"openai","model":"gpt-prior","apiBase":"https://prior.example/v1"}],"default":{"model":"Prior"},"slashCommands":[{"name":"x"}]}"#,
        )
        .unwrap();
        let req = request("continue", config_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        assert!(verify_agent_transport(&req).unwrap().ok);
        assert!(can_clear_agent_transport("continue", &config_path));

        let configured: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(configured["default"]["model"], json!("d-token local route"));
        let models = configured["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert!(models
            .iter()
            .any(|model| model["title"] == json!("d-token local route")
                && model["provider"] == json!("openai")
                && model["apiBase"] == json!("http://127.0.0.1:8787/v1")
                && model["apiKey"] == json!("dtoken-loopback")));
        assert_eq!(configured["slashCommands"], json!([{"name":"x"}]));

        reviewed_apply(
            &req,
            AgentTransportOperation::Clear,
            &root.path().join("backups"),
        );
        let restored: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(restored["default"]["model"], json!("Prior"));
        assert_eq!(
            restored["models"],
            json!([{"title":"Prior","provider":"openai","model":"gpt-prior","apiBase":"https://prior.example/v1"}])
        );
        assert_eq!(restored["slashCommands"], json!([{"name":"x"}]));
        assert!(!config_dir
            .join("dtoken-agent-transport-continue.json")
            .exists());
    }

    #[test]
    fn continue_refuses_when_yaml_config_is_authoritative() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".continue");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.json");
        fs::write(&config_path, r#"{"models":[]}"#).unwrap();
        fs::write(
            config_dir.join("config.yaml"),
            "name: Main Config\nmodels: []\n",
        )
        .unwrap();
        let req = request("continue", config_path.clone());
        assert!(
            configure_agent_transport(&req, &root.path().join("backups"))
                .unwrap_err()
                .to_string()
                .contains("not authoritative")
        );
        assert!(!can_clear_agent_transport("continue", &config_path));
    }

    #[test]
    fn aider_configure_clear_restores_previous_lines() {
        let root = workspace_tempdir();
        let config_path = root.path().join(".aider.conf.yml");
        fs::write(
            &config_path,
            "model: gpt-old\nopenai-api-base: https://prior.example/v1\n# keep comment\nmap-tokens: 2048\n",
        )
        .unwrap();
        let req = request("aider", config_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        assert!(verify_agent_transport(&req).unwrap().ok);
        assert!(can_clear_agent_transport("aider", &config_path));

        let configured = fs::read_to_string(&config_path).unwrap();
        assert!(configured.contains("model: claude-sonnet-test"));
        assert!(configured.contains("openai-api-base: http://127.0.0.1:8787/v1"));
        assert!(configured.contains("openai-api-key: dtoken-loopback"));
        assert!(configured.contains("# keep comment"));
        assert!(configured.contains("map-tokens: 2048"));

        reviewed_apply(
            &req,
            AgentTransportOperation::Clear,
            &root.path().join("backups"),
        );
        let restored = fs::read_to_string(&config_path).unwrap();
        assert!(restored.contains("model: gpt-old"));
        assert!(restored.contains("openai-api-base: https://prior.example/v1"));
        assert!(!restored.contains("openai-api-key"));
        assert!(restored.contains("# keep comment"));
        assert!(!can_clear_agent_transport("aider", &config_path));
    }

    #[test]
    fn aider_refuses_credential_collision_and_nested_yaml() {
        let root = workspace_tempdir();
        let config_path = root.path().join(".aider.conf.yml");
        fs::write(&config_path, "openai-api-key: user-real-secret\n").unwrap();
        let req = request("aider", config_path.clone());
        assert!(
            configure_agent_transport(&req, &root.path().join("backups"))
                .unwrap_err()
                .to_string()
                .contains("credential")
        );
        fs::write(&config_path, "model:\n  - nested\n").unwrap();
        assert!(
            configure_agent_transport(&req, &root.path().join("backups"))
                .unwrap_err()
                .to_string()
                .contains("nested YAML")
        );
    }

    #[test]
    fn cline_cli_v1_configure_clear_preserves_other_providers_and_previous_selection() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".cline/settings");
        fs::create_dir_all(&config_dir).unwrap();
        let path = config_dir.join("providers.json");
        let original = r#"{
  "version": 1,
  "lastUsedProvider": "anthropic",
  "providers": {
    "anthropic": {
      "settings": {
        "provider": "anthropic",
        "apiKey": "existing-user-secret",
        "model": "claude-existing"
      },
      "tokenSource": "manual"
    }
  }
}"#;
        fs::write(&path, original).unwrap();
        let mut request = request("cline", path.clone());
        request.model = Some("gpt-routed".into());
        let backup = root.path().join("backups");

        let adapter = agent_transport_adapter("cline").unwrap();
        assert_eq!(adapter.kind, AgentTransportAdapterKind::ClineCli);
        assert!(adapter.can_configure);
        assert!(adapter.can_clear);

        let configured = configure_agent_transport(&request, &backup).unwrap();
        assert!(configured.verification.ok);
        assert!(!serde_json::to_string(&configured)
            .unwrap()
            .contains("existing-user-secret"));
        let document: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            document.get("lastUsedProvider").and_then(Value::as_str),
            Some(CLINE_PROVIDER_ID)
        );
        assert_eq!(
            document
                .pointer("/providers/openai-compatible/settings/baseUrl")
                .and_then(Value::as_str),
            Some("http://127.0.0.1:8787/v1")
        );
        assert_eq!(
            document
                .pointer("/providers/openai-compatible/settings/apiKey")
                .and_then(Value::as_str),
            Some(CLINE_PUBLIC_API_KEY_MARKER)
        );
        assert_eq!(
            document
                .pointer("/providers/anthropic/settings/apiKey")
                .and_then(Value::as_str),
            Some("existing-user-secret")
        );
        assert!(can_clear_agent_transport("cline", &path));

        let cleared = clear_agent_transport(&request, &backup).unwrap();
        assert!(cleared.verification.ok);
        let restored: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            restored.get("lastUsedProvider").and_then(Value::as_str),
            Some("anthropic")
        );
        assert!(restored.pointer("/providers/openai-compatible").is_none());
        assert_eq!(
            restored
                .pointer("/providers/anthropic/settings/apiKey")
                .and_then(Value::as_str),
            Some("existing-user-secret")
        );
        assert!(!cline_manifest_path(&path).unwrap().exists());
    }

    #[test]
    fn openclaw_json5_multi_provider_configure_clear_preserves_user_provider() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".openclaw");
        fs::create_dir_all(&config_dir).unwrap();
        let path = config_dir.join("openclaw.json");
        fs::write(
            &path,
            r#"{
  // JSON5 comments and trailing commas are valid OpenClaw configuration.
  models: {
    providers: {
      existing: {
        baseUrl: "https://existing.example/v1",
        apiKey: "${EXISTING_API_KEY}",
        api: "openai-completions",
        models: [{ id: "existing-model", name: "Existing model" }],
      },
    },
  },
  agents: { defaults: { model: { primary: "existing/existing-model" } } },
}"#,
        )
        .unwrap();
        let mut request = request("openclaw", path.clone());
        request.model = Some("gpt-routed".into());
        let backup = root.path().join("backups");

        let configured = configure_agent_transport(&request, &backup).unwrap();
        assert!(configured.verification.ok);
        let configured_value: Value = json5::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            configured_value
                .pointer("/agents/defaults/model/primary")
                .and_then(Value::as_str),
            Some("dtoken-local-route/gpt-routed")
        );
        assert_eq!(
            configured_value
                .pointer("/models/providers/dtoken-local-route/apiKey")
                .and_then(Value::as_str),
            Some("")
        );
        assert_eq!(
            configured_value
                .pointer("/models/providers/existing/apiKey")
                .and_then(Value::as_str),
            Some("${EXISTING_API_KEY}")
        );
        assert!(can_clear_agent_transport("openclaw", &path));

        let cleared = clear_agent_transport(&request, &backup).unwrap();
        assert!(cleared.verification.ok);
        let restored: Value = json5::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            restored
                .pointer("/agents/defaults/model/primary")
                .and_then(Value::as_str),
            Some("existing/existing-model")
        );
        assert!(restored
            .pointer("/models/providers/dtoken-local-route")
            .is_none());
        assert!(restored.pointer("/models/providers/existing").is_some());
        assert!(!openclaw_manifest_path(&path).unwrap().exists());
    }

    #[test]
    fn multi_provider_clear_preserves_a_user_selection_changed_after_configure() {
        let root = workspace_tempdir();
        let backup = root.path().join("backups");

        let cline_dir = root.path().join(".cline/settings");
        fs::create_dir_all(&cline_dir).unwrap();
        let cline_path = cline_dir.join("providers.json");
        fs::write(
            &cline_path,
            r#"{"version":1,"lastUsedProvider":"anthropic","providers":{"anthropic":{"settings":{"provider":"anthropic","model":"claude"}}}}"#,
        )
        .unwrap();
        let mut cline_request = request("cline", cline_path.clone());
        cline_request.model = Some("gpt-routed".into());
        configure_agent_transport(&cline_request, &backup).unwrap();
        let mut cline_value: Value =
            serde_json::from_str(&fs::read_to_string(&cline_path).unwrap()).unwrap();
        cline_value["lastUsedProvider"] = Value::String("anthropic".into());
        fs::write(
            &cline_path,
            serde_json::to_string_pretty(&cline_value).unwrap(),
        )
        .unwrap();
        assert!(can_clear_agent_transport("cline", &cline_path));
        clear_agent_transport(&cline_request, &backup).unwrap();
        let cline_cleared: Value =
            serde_json::from_str(&fs::read_to_string(&cline_path).unwrap()).unwrap();
        assert_eq!(
            cline_cleared
                .get("lastUsedProvider")
                .and_then(Value::as_str),
            Some("anthropic")
        );
        assert!(cline_cleared
            .pointer("/providers/openai-compatible")
            .is_none());

        let openclaw_dir = root.path().join(".openclaw");
        fs::create_dir_all(&openclaw_dir).unwrap();
        let openclaw_path = openclaw_dir.join("openclaw.json");
        fs::write(
            &openclaw_path,
            r#"{"models":{"providers":{"existing":{"baseUrl":"https://existing.example/v1","api":"openai-completions","models":[{"id":"existing-model"}]}}},"agents":{"defaults":{"model":{"primary":"existing/existing-model"}}}}"#,
        )
        .unwrap();
        let mut openclaw_request = request("openclaw", openclaw_path.clone());
        openclaw_request.model = Some("gpt-routed".into());
        configure_agent_transport(&openclaw_request, &backup).unwrap();
        let mut openclaw_value: Value =
            json5::from_str(&fs::read_to_string(&openclaw_path).unwrap()).unwrap();
        openclaw_value["agents"]["defaults"]["model"]["primary"] =
            Value::String("existing/existing-model".into());
        fs::write(&openclaw_path, json5::to_string(&openclaw_value).unwrap()).unwrap();
        assert!(can_clear_agent_transport("openclaw", &openclaw_path));
        clear_agent_transport(&openclaw_request, &backup).unwrap();
        let openclaw_cleared: Value =
            json5::from_str(&fs::read_to_string(&openclaw_path).unwrap()).unwrap();
        assert_eq!(
            openclaw_cleared
                .pointer("/agents/defaults/model/primary")
                .and_then(Value::as_str),
            Some("existing/existing-model")
        );
        assert!(openclaw_cleared
            .pointer("/models/providers/dtoken-local-route")
            .is_none());
    }

    #[test]
    fn reviewed_transport_rejects_stale_content_before_creating_a_backup() {
        let root = workspace_tempdir();
        let path = root.path().join("settings.json");
        fs::write(&path, r#"{"theme":"dark"}"#).unwrap();
        let request = request("claude-code", path.clone());
        let backup = root.path().join("backups");
        let review = preview_configure_agent_transport(&request).unwrap();

        fs::write(&path, r#"{"theme":"light"}"#).unwrap();
        let error = apply_reviewed_agent_transport(
            &request,
            AgentTransportOperation::Configure,
            &backup,
            &review.revision,
            &review.before_sha256,
            &review.after_sha256,
        )
        .unwrap_err();

        assert!(error.to_string().contains("changed after review"));
        assert_eq!(fs::read_to_string(path).unwrap(), r#"{"theme":"light"}"#);
        assert!(!backup.exists());
    }

    #[test]
    fn clear_fails_closed_after_external_selection_change_and_keeps_user_edit() {
        let root = workspace_tempdir();
        let path = root.path().join("settings.json");
        fs::write(&path, r#"{"env":{"ANTHROPIC_MODEL":"claude-old"}}"#).unwrap();
        let request = request("claude-code", path.clone());
        let backup = root.path().join("backups");
        configure_agent_transport(&request, &backup).unwrap();
        let mut changed: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        changed["env"]["ANTHROPIC_MODEL"] = Value::String("user-changed-model".into());
        fs::write(&path, serde_json::to_string_pretty(&changed).unwrap()).unwrap();

        let error = clear_agent_transport(&request, &backup).unwrap_err();
        assert!(error.to_string().contains("changed outside d-token"));
        assert_eq!(
            serde_json::from_str::<Value>(&fs::read_to_string(path).unwrap())
                .unwrap()
                .pointer("/env/ANTHROPIC_MODEL")
                .and_then(Value::as_str),
            Some("user-changed-model")
        );
    }

    #[test]
    fn clear_is_idempotent_when_no_dtoken_transport_is_present() {
        let root = workspace_tempdir();
        let path = root.path().join("config.toml");
        let original = "model_provider = \"openai\"\nmodel = \"gpt\"\n";
        fs::write(&path, original).unwrap();
        let request = request("codex", path.clone());
        let receipt = clear_agent_transport(&request, &root.path().join("backups")).unwrap();
        assert!(!receipt.changed);
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn rollback_receipt_restores_configured_file_exactly() {
        let root = workspace_tempdir();
        let path = root.path().join("config.toml");
        let original = "model_provider = \"openai\"\nmodel = \"gpt-old\"\n";
        fs::write(&path, original).unwrap();
        let request = request("codex", path.clone());
        let backup = root.path().join("backups");
        let receipt = configure_agent_transport(&request, &backup).unwrap();
        let change = &receipt.transaction.unwrap().changes[0];
        ConfigPatcher::new(&backup)
            .restore_backup(
                &change.backup,
                &ExpectedFileState::Hash(change.after.sha256.clone().unwrap()),
            )
            .unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn can_clear_requires_complete_owned_current_state_and_reviews_are_redacted() {
        let root = workspace_tempdir();
        let path = root.path().join("settings.json");
        fs::write(
            &path,
            r#"{"env":{"ANTHROPIC_API_KEY":"should-never-appear"}}"#,
        )
        .unwrap();
        let request = request("claude-code", path.clone());
        assert!(!can_clear_agent_transport("claude-code", &path));
        let review = preview_configure_agent_transport(&request).unwrap();
        assert!(!serde_json::to_string(&review)
            .unwrap()
            .contains("should-never-appear"));
        configure_agent_transport(&request, &root.path().join("backups")).unwrap();
        assert!(can_clear_agent_transport("claude-code", &path));
    }

    #[test]
    fn opencode_configure_preserves_multi_provider_default_without_global_allowlist() {
        let original = r#"{
  "provider": {
    "existing": { "name": "Existing provider" }
  }
}"#;
        let configured = configure_opencode_transport_document(
            original,
            "http://127.0.0.1:8787",
            "gpt-5.6",
            true,
        )
        .unwrap();
        let document: Value = serde_json::from_str(&configured).unwrap();
        assert_eq!(
            document
                .pointer("/provider/existing/name")
                .and_then(Value::as_str),
            Some("Existing provider")
        );
        assert_eq!(
            document
                .pointer("/provider/dtoken-local-route/options/baseURL")
                .and_then(Value::as_str),
            Some("http://127.0.0.1:8787/v1")
        );
        assert_eq!(
            document
                .pointer("/provider/dtoken-local-route/options/apiKey")
                .and_then(Value::as_str),
            Some(OPENCODE_TOKEN_REFERENCE)
        );
        assert!(document.get("enabled_providers").is_none());

        let verification =
            verify_opencode(&configured, "http://127.0.0.1:8787", true, Some("gpt-5.6"));
        assert!(verification.is_ok());
        let upstream =
            current_upstream_signature(agent_transport_adapter("opencode").unwrap(), &configured)
                .unwrap()
                .unwrap();
        assert_eq!(upstream.model.as_deref(), Some("gpt-5.6"));
        assert_eq!(upstream.endpoint, "http://127.0.0.1:8787");
        assert_ne!(upstream.signature_sha256, OPENCODE_TOKEN_REFERENCE);
    }

    #[test]
    fn opencode_allowlist_is_extended_and_restored_without_touching_other_providers() {
        let original = r#"{
  "provider": { "existing": { "name": "Existing provider" } },
  "enabled_providers": ["existing"]
}"#;
        let configured = configure_opencode_transport_document(
            original,
            "http://127.0.0.1:8787",
            "gpt-5.6",
            true,
        )
        .unwrap();
        let configured_value: Value = serde_json::from_str(&configured).unwrap();
        assert_eq!(
            configured_value["enabled_providers"],
            serde_json::json!(["existing", OPENCODE_PROVIDER_ID])
        );

        let cleared = clear_agent_transport_document("opencode", &configured).unwrap();
        let cleared_value: Value = serde_json::from_str(&cleared).unwrap();
        assert!(cleared_value
            .pointer("/provider/dtoken-local-route")
            .is_none());
        assert_eq!(
            cleared_value["enabled_providers"],
            serde_json::json!(["existing"])
        );
        assert_eq!(
            cleared_value
                .pointer("/provider/existing/name")
                .and_then(Value::as_str),
            Some("Existing provider")
        );
        assert_eq!(
            clear_agent_transport_document("opencode", &cleared).unwrap(),
            cleared
        );
    }

    #[test]
    fn opencode_only_supports_disabled_state_through_existing_allowlist() {
        let error = configure_opencode_transport_document(
            r#"{"provider":{"existing":{"name":"Existing provider"}}}"#,
            "http://127.0.0.1:8787",
            "gpt-5.6",
            false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("enabled_providers allowlist"));

        let configured = configure_opencode_transport_document(
            r#"{"enabled_providers":["existing"]}"#,
            "http://127.0.0.1:8787",
            "gpt-5.6",
            false,
        )
        .unwrap();
        assert!(
            verify_opencode(&configured, "http://127.0.0.1:8787", false, Some("gpt-5.6")).is_ok()
        );
        assert_eq!(
            serde_json::from_str::<Value>(&configured).unwrap()["enabled_providers"],
            serde_json::json!(["existing"])
        );
    }

    #[test]
    fn opencode_never_replaces_unowned_provider_or_clears_unowned_list_entry() {
        let collision = r#"{
            "provider": {
                "dtoken-local-route": { "name": "user-owned" }
            },
            "enabled_providers": ["dtoken-local-route"]
        }"#;
        assert!(configure_opencode_transport_document(
            collision,
            "http://127.0.0.1:8787",
            "gpt-5.6",
            true,
        )
        .is_err());
        assert!(clear_agent_transport_document("opencode", collision).is_err());

        let mut lookalike = opencode_provider_value("http://127.0.0.1:8787", "gpt-5.6");
        lookalike
            .as_object_mut()
            .unwrap()
            .insert("user_note".into(), Value::String("do not overwrite".into()));
        let lookalike = serde_json::json!({
            "provider": { OPENCODE_PROVIDER_ID: lookalike }
        })
        .to_string();
        assert!(configure_opencode_transport_document(
            &lookalike,
            "http://127.0.0.1:8787",
            "gpt-5.6",
            true,
        )
        .is_err());
        assert!(clear_agent_transport_document("opencode", &lookalike).is_err());

        let dangling = r#"{"enabled_providers":["dtoken-local-route","existing"]}"#;
        assert_eq!(
            clear_agent_transport_document("opencode", dangling).unwrap(),
            dangling
        );
        assert!(verify_opencode_cleared(dangling).is_ok());
    }

    #[test]
    fn opencode_refuses_explicitly_disabled_provider_and_openclaw_uses_filesystem_context() {
        let error = configure_opencode_transport_document(
            r#"{"disabled_providers":["dtoken-local-route"]}"#,
            "http://127.0.0.1:8787",
            "gpt-5.6",
            true,
        )
        .unwrap_err();
        assert!(error.to_string().contains("explicitly disabled"));

        let openclaw = agent_transport_adapter("openclaw").unwrap();
        assert_eq!(openclaw.kind, AgentTransportAdapterKind::OpenClaw);
        assert!(openclaw.can_configure);
        assert!(openclaw.can_clear);
        let error = clear_agent_transport_document("openclaw", "{}").unwrap_err();
        assert!(error.to_string().contains("filesystem context"));
    }

    #[test]
    fn hermes_configure_clear_restores_previous_model_and_base_url() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".hermes");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.yaml");
        let prior = r#"model:
  provider:
    custom:
      base_url: https://prior.example/v1
      apiKey: prior-key
  default: hermes-prior-model
"#;
        fs::write(&config_path, prior).unwrap();
        let req = request("hermes", config_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        assert!(verify_agent_transport(&req).unwrap().ok);
        assert!(can_clear_agent_transport("hermes", &config_path));

        let configured = fs::read_to_string(&config_path).unwrap();
        assert!(configured.contains("http://127.0.0.1:8787"));
        assert!(configured.contains("claude-sonnet-test"));
        assert!(configured.contains("dtoken-loopback"));
        assert!(!configured.contains("prior-key"));

        reviewed_apply(
            &req,
            AgentTransportOperation::Clear,
            &root.path().join("backups"),
        );
        let restored = fs::read_to_string(&config_path).unwrap();
        assert!(restored.contains("https://prior.example/v1"));
        assert!(restored.contains("hermes-prior-model"));
        assert!(!restored.contains("dtoken-loopback"));
        assert!(!can_clear_agent_transport("hermes", &config_path));
        assert!(!config_dir
            .join("dtoken-agent-transport-hermes.json")
            .exists());
    }

    #[test]
    fn hermes_refuses_credential_collision_and_external_edit() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".hermes");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.yaml");
        fs::write(&config_path, "model:\n  default: hermes-model\n").unwrap();
        let req = request("hermes", config_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        fs::write(&config_path, "model:\n  default: tampered-model\n").unwrap();
        assert!(!can_clear_agent_transport("hermes", &config_path));
        assert!(clear_agent_transport(&req, &root.path().join("backups"))
            .unwrap_err()
            .to_string()
            .contains("changed outside d-token"));
    }

    #[test]
    fn hermes_clear_is_idempotent_when_no_dtoken_transport() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".hermes");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.yaml");
        fs::write(&config_path, "model:\n  default: hermes-model\n").unwrap();
        let receipt = clear_agent_transport(
            &request("hermes", config_path),
            &root.path().join("backups"),
        )
        .unwrap();
        assert!(!receipt.changed);
    }

    #[test]
    fn openhands_configure_clear_restores_previous_llm_section() {
        let root = workspace_tempdir();
        let config_path = root.path().join("config.toml");
        let prior = r#"[llm]
model = "openhands-prior"
base_url = "https://prior.example/v1"
api_key = "prior-key"
"#;
        fs::write(&config_path, prior).unwrap();
        let req = request("openhands", config_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        assert!(verify_agent_transport(&req).unwrap().ok);
        assert!(can_clear_agent_transport("openhands", &config_path));

        let configured = fs::read_to_string(&config_path).unwrap();
        assert!(configured.contains("http://127.0.0.1:8787"));
        assert!(configured.contains("claude-sonnet-test"));
        assert!(configured.contains("dtoken-loopback"));

        reviewed_apply(
            &req,
            AgentTransportOperation::Clear,
            &root.path().join("backups"),
        );
        let restored = fs::read_to_string(&config_path).unwrap();
        assert!(restored.contains("openhands-prior"));
        assert!(restored.contains("https://prior.example/v1"));
        assert!(!restored.contains("dtoken-loopback"));
        assert!(!config_path
            .parent()
            .unwrap()
            .join("dtoken-agent-transport-openhands.json")
            .exists());
    }

    #[test]
    fn openhands_v1_environment_fails_closed() {
        let root = workspace_tempdir();
        let config_path = root.path().join("config.toml");
        fs::write(&config_path, "[llm]\nmodel = \"test\"\n").unwrap();
        let req = request("openhands", config_path.clone());
        std::env::set_var("OPENHANDS_LLM_API_KEY", "test");
        let result = configure_agent_transport(&req, &root.path().join("backups"));
        std::env::remove_var("OPENHANDS_LLM_API_KEY");
        assert!(result.unwrap_err().to_string().contains("V1"));
    }

    #[test]
    fn openhands_clear_is_idempotent_when_no_dtoken_transport() {
        let root = workspace_tempdir();
        let config_path = root.path().join("config.toml");
        fs::write(&config_path, "[llm]\nmodel = \"test\"\n").unwrap();
        let receipt = clear_agent_transport(
            &request("openhands", config_path),
            &root.path().join("backups"),
        )
        .unwrap();
        assert!(!receipt.changed);
    }

    #[test]
    fn pi_configure_clear_restores_previous_provider_and_model() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".pi/agent");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("models.json");
        let prior = r#"{
    "settings": {
        "defaultProvider": "user-provider",
        "defaultModel": "user-model"
    }
}"#;
        fs::write(&config_path, prior).unwrap();
        let req = request("pi", config_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        assert!(verify_agent_transport(&req).unwrap().ok);
        assert!(can_clear_agent_transport("pi", &config_path));

        let configured: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(
            configured
                .pointer("/settings/defaultProvider")
                .and_then(Value::as_str),
            Some("dtoken")
        );
        assert_eq!(
            configured
                .pointer("/settings/defaultModel")
                .and_then(Value::as_str),
            Some("claude-sonnet-test")
        );
        assert!(configured.pointer("/providers/dtoken").is_some());

        reviewed_apply(
            &req,
            AgentTransportOperation::Clear,
            &root.path().join("backups"),
        );
        let restored: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(
            restored
                .pointer("/settings/defaultProvider")
                .and_then(Value::as_str),
            Some("user-provider")
        );
        assert_eq!(
            restored
                .pointer("/settings/defaultModel")
                .and_then(Value::as_str),
            Some("user-model")
        );
        assert!(restored.pointer("/providers").is_none());
        assert!(!can_clear_agent_transport("pi", &config_path));
    }

    #[test]
    fn pi_refuses_external_edit() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".pi/agent");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("models.json");
        fs::write(&config_path, "{}").unwrap();
        let req = request("pi", config_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        fs::write(
            &config_path,
            r#"{"settings":{"defaultProvider":"tampered"}}"#,
        )
        .unwrap();
        assert!(!can_clear_agent_transport("pi", &config_path));
        assert!(clear_agent_transport(&req, &root.path().join("backups"))
            .unwrap_err()
            .to_string()
            .contains("changed outside d-token"));
    }

    #[test]
    fn pi_clear_is_idempotent_after_full_cycle() {
        // configure → clear → clear again → changed=false
        let root = workspace_tempdir();
        let config_dir = root.path().join(".pi/agent");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("models.json");
        let prior = r#"{
    "settings": {
        "defaultProvider": "user-provider",
        "defaultModel": "user-model"
    }
}"#;
        fs::write(&config_path, prior).unwrap();
        let req = request("pi", config_path.clone());
        let backups = root.path().join("backups");

        // Configure
        reviewed_apply(&req, AgentTransportOperation::Configure, &backups);
        assert!(can_clear_agent_transport("pi", &config_path));
        let configured: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(
            configured
                .pointer("/settings/defaultProvider")
                .and_then(Value::as_str),
            Some("dtoken")
        );

        // First clear → actually changed
        let receipt1 = clear_agent_transport(&req, &backups).unwrap();
        assert!(receipt1.changed);
        let restored: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        assert_eq!(
            restored
                .pointer("/settings/defaultProvider")
                .and_then(Value::as_str),
            Some("user-provider")
        );
        assert!(!can_clear_agent_transport("pi", &config_path));

        // Second clear → no-op (no d-token transport to clear)
        let receipt2 = clear_agent_transport(&req, &backups).unwrap();
        assert!(!receipt2.changed);
    }

    #[test]
    fn mistral_vibe_configure_clear_restores_active_model() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".vibe");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.toml");
        let prior = r#"active_model = "user-provider/user-model"

[providers]
user-provider = { type = "openai", base_url = "https://prior.example/v1" }
"#;
        fs::write(&config_path, prior).unwrap();
        let req = request("mistral-vibe", config_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        assert!(verify_agent_transport(&req).unwrap().ok);
        assert!(can_clear_agent_transport("mistral-vibe", &config_path));

        let configured = fs::read_to_string(&config_path).unwrap();
        assert!(configured.contains("http://127.0.0.1:8787"));
        assert!(configured.contains("dtoken-local-route/claude-sonnet-test"));
        assert!(configured.contains("dtoken-loopback"));

        reviewed_apply(
            &req,
            AgentTransportOperation::Clear,
            &root.path().join("backups"),
        );
        let restored = fs::read_to_string(&config_path).unwrap();
        assert!(restored.contains("user-provider/user-model"));
        assert!(!restored.contains("dtoken-local-route"));
        assert!(!config_dir
            .join("dtoken-agent-transport-mistral-vibe.json")
            .exists());
    }

    #[test]
    fn mistral_vibe_refuses_external_edit() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".vibe");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.toml");
        fs::write(&config_path, "active_model = \"test\"\n").unwrap();
        let req = request("mistral-vibe", config_path.clone());
        reviewed_apply(
            &req,
            AgentTransportOperation::Configure,
            &root.path().join("backups"),
        );
        fs::write(&config_path, "active_model = \"tampered\"\n").unwrap();
        assert!(!can_clear_agent_transport("mistral-vibe", &config_path));
        assert!(clear_agent_transport(&req, &root.path().join("backups"))
            .unwrap_err()
            .to_string()
            .contains("changed outside d-token"));
    }

    #[test]
    fn new_adapter_verify_cleared_returns_proper_errors() {
        let root = workspace_tempdir();
        let config_dir = root.path().join(".hermes");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.yaml");
        fs::write(&config_path, "model:\n  default: test\n").unwrap();
        let req = request("hermes", config_path.clone());
        let receipt = configure_agent_transport(&req, &root.path().join("backups")).unwrap();
        assert!(receipt.verification.ok);
        let cleared = clear_agent_transport(&req, &root.path().join("backups")).unwrap();
        assert!(cleared.verification.ok);
        assert!(cleared.changed);
    }
}
