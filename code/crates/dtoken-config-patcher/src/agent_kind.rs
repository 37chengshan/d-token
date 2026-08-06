use serde::{Deserialize, Serialize};

/// The kind of integration between d-token and an AI coding agent.
///
/// Each integration kind has a different truthfulness contract: `Route`
/// integration provides transparent compression and route evidence, while
/// `Mcp` and `Hook` integration provide tool-based or lifecycle-based
/// interactions without modifying the agent's upstream model provider.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentIntegrationKind {
    /// Direct upstream provider/route configuration. The agent's model requests
    /// are routed through d-token, enabling transparent compression, route
    /// decisions, and route evidence in the Event Ledger.
    Route,
    /// MCP server registration. The agent uses d-token as an MCP tool provider
    /// by adding a d-token MCP server entry to the agent's MCP configuration
    /// file (e.g. `mcpServers` in JSON or `mcp_servers` in TOML).
    ///
    /// This does NOT provide transparent model compression or route evidence.
    /// Model requests continue to use the agent's existing upstream provider.
    /// The d-token MCP server provides tool-based interactions only.
    Mcp,
    /// Hook/plugin integration. The agent's session lifecycle hooks or plugins
    /// are modified to include d-token instrumentation.
    ///
    /// This does NOT provide transparent model compression or route evidence.
    /// Model requests continue to use the agent's existing upstream provider.
    /// The d-token hook provides session-level context injection only.
    Hook,
}

/// A truthful description of what a given integration can and cannot do.
///
/// This is exposed in every Rust-owned descriptor, snapshot, plan, and receipt
/// so that callers (UI, API, event ledger) can distinguish between a full
/// route integration and a more limited MCP or hook integration.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TruthfulLimitation {
    /// The kind of integration.
    pub kind: AgentIntegrationKind,
    /// Human-readable description of what this integration provides and what
    /// it does NOT provide. Never contains credentials or secrets.
    pub description: &'static str,
    /// Whether this integration provides transparent compression and route
    /// evidence. `true` only for `Route` kind.
    pub provides_route_evidence: bool,
    /// Whether this integration modifies the agent's upstream model provider.
    /// `true` only for `Route` kind.
    pub modifies_model_provider: bool,
}

impl TruthfulLimitation {
    /// Build the truthful limitation for a Route integration.
    pub const fn route() -> Self {
        TruthfulLimitation {
            kind: AgentIntegrationKind::Route,
            description: "d-token routes the agent's model requests through the local runtime, enabling transparent compression, route decisions, and verifiable route evidence in the Event Ledger",
            provides_route_evidence: true,
            modifies_model_provider: true,
        }
    }

    /// Build the truthful limitation for an MCP integration.
    pub const fn mcp() -> Self {
        TruthfulLimitation {
            kind: AgentIntegrationKind::Mcp,
            description: "d-token is registered as an MCP tool server. Model requests continue to use the agent's existing upstream provider; d-token does not provide transparent compression, route decisions, or route evidence for model requests",
            provides_route_evidence: false,
            modifies_model_provider: false,
        }
    }

    /// Build the truthful limitation for a Hook integration.
    pub const fn hook() -> Self {
        TruthfulLimitation {
            kind: AgentIntegrationKind::Hook,
            description: "d-token is registered as a session lifecycle hook. Model requests continue to use the agent's existing upstream provider; d-token does not provide transparent compression, route decisions, or route evidence for model requests",
            provides_route_evidence: false,
            modifies_model_provider: false,
        }
    }

    /// Build the truthful limitation for an agent that has no verified safe
    /// write surface in this candidate.
    pub const fn unsupported(reason: &'static str) -> Self {
        TruthfulLimitation {
            kind: AgentIntegrationKind::Hook,
            description: reason,
            provides_route_evidence: false,
            modifies_model_provider: false,
        }
    }
}
