//! Verified MCP configuration surface for AI coding agents.
//!
//! This module only knows where each agent stores its MCP server
//! configuration (`mcpServers` JSON documents in well-known files) and how to
//! manipulate that JSON document. It NEVER invents a d-token command: the
//! caller must supply the exact entry to insert, verify, or remove. The
//! desktop-managed flow builds that entry from the resolved absolute
//! `dtoken-agent-bridge` executable and its `mcp stdio` arguments (the
//! metadata-only bridge; raw CCR capabilities are never written into agent
//! configuration).
//!
//! Ownership is strict: an entry is considered d-token-owned only when it
//! matches the caller-supplied entry exactly (command and arguments). A
//! user-created entry under the same key is never removed or overwritten.
//!
//! The agent transport adapter deliberately does not auto-configure these
//! agents: no `d-token` executable exists, and the real metadata-only bridge
//! entry requires a resolved absolute executable path plus managed launch
//! context that the transport request does not carry. Automatic configure
//! would fabricate a broken entry; see the fail-closed `ADAPTERS` entries in
//! `agent_transport`. The desktop-managed MCP/Hook integration flow resolves
//! the bridge executable and drives the verified document transforms here.

use crate::agent::HostPlatform;
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// The well-known d-token MCP server entry key in agent MCP configuration files.
pub const DTOKEN_MCP_ENTRY_KEY: &str = "d-token";

/// A verified MCP configuration target for an AI coding agent.
///
/// Only agents with verified, versioned MCP config schemas (a JSON object
/// with an `mcpServers` or `servers` field in a well-known file) are included.
pub struct AgentMcpConfig {
    pub agent_id: &'static str,
    /// Default home-relative path (macOS/Linux). The caller resolves it against
    /// the user's home directory.
    pub config_path: &'static str,
    /// Optional Windows path relative to `%APPDATA%` (RoamingAppData). When set,
    /// the Windows resolver prefers it over `config_path`.
    pub windows_path: Option<&'static str>,
    /// The JSON field name holding the MCP server definitions.
    pub mcp_field: &'static str,
}

/// Known MCP config paths. Each entry carries a default home-relative path and
/// an optional Windows `%APPDATA%`-relative path.
///
/// Verified sources:
/// - cursor: `.cursor/mcp.json` `mcpServers`
/// - claude-desktop: `claude_desktop_config.json` `mcpServers`
/// - windsurf: `.codeium/windsurf/mcp_config.json` `mcpServers`
/// - commandcode: `~/.commandcode/mcp.json` `mcpServers` (commandcode.ai/docs/mcp)
/// - copilot-cli: `~/.copilot/mcp-config.json` `mcpServers` (docs.github.com Copilot CLI)
/// - vscode-copilot-chat: VS Code user-level `mcp.json` `servers`
///   (code.visualstudio.com/docs/copilot/customization/copilot-mcp)
pub const KNOWN_MCP_CONFIGS: &[AgentMcpConfig] = &[
    AgentMcpConfig {
        agent_id: "cursor",
        config_path: ".cursor/mcp.json",
        windows_path: None,
        mcp_field: "mcpServers",
    },
    AgentMcpConfig {
        agent_id: "claude-desktop",
        config_path: "Library/Application Support/Claude/claude_desktop_config.json",
        windows_path: Some("Claude/claude_desktop_config.json"),
        mcp_field: "mcpServers",
    },
    AgentMcpConfig {
        agent_id: "windsurf",
        config_path: ".codeium/windsurf/mcp_config.json",
        windows_path: None,
        mcp_field: "mcpServers",
    },
    AgentMcpConfig {
        agent_id: "commandcode",
        config_path: ".commandcode/mcp.json",
        windows_path: None,
        mcp_field: "mcpServers",
    },
    AgentMcpConfig {
        agent_id: "copilot-cli",
        config_path: ".copilot/mcp-config.json",
        windows_path: None,
        mcp_field: "mcpServers",
    },
    AgentMcpConfig {
        agent_id: "vscode-copilot-chat",
        config_path: "Library/Application Support/Code/User/mcp.json",
        windows_path: Some("Code/User/mcp.json"),
        mcp_field: "servers",
    },
];

/// Resolve the MCP config path for a given agent ID, relative to `home`
/// (macOS/Linux default layout). Returns `None` if the agent has no verified
/// MCP config path.
pub fn resolve_mcp_config_path(agent_id: &str, home: &Path) -> Option<PathBuf> {
    KNOWN_MCP_CONFIGS
        .iter()
        .find(|c| c.agent_id == agent_id)
        .map(|c| home.join(c.config_path))
}

/// Platform-aware MCP config path resolution.
///
/// On Windows, the `%APPDATA%`-relative path (when registered) is preferred;
/// otherwise the home-relative default is used.
pub fn resolve_mcp_config_path_for(
    agent_id: &str,
    platform: HostPlatform,
    home: &Path,
    roaming_app_data: Option<&Path>,
) -> Option<PathBuf> {
    let config = mcp_config_for(agent_id).ok()?;
    if platform == HostPlatform::Windows {
        if let Some(windows_path) = config.windows_path {
            return Some(roaming_app_data.unwrap_or(home).join(windows_path));
        }
    }
    Some(home.join(config.config_path))
}

/// The d-token ownership marker used in hook configuration documents.
/// Mirrors `agent_integration::merge_session_start_hook` so the desktop-managed
/// hook and the hybrid integration flow share one ownership identity.
pub const DTOKEN_HOOK_MANAGED_ID: &str = "dtoken-hybrid-session-start-v1";

/// A verified Hook configuration target for an AI coding agent.
pub struct AgentHookConfig {
    pub agent_id: &'static str,
    /// Home-relative path. The caller resolves it against the user's home
    /// directory.
    pub config_path: &'static str,
    /// The JSON field holding the hook definitions (e.g. `hooks`).
    pub hook_field: &'static str,
}

/// Known Hook config paths.
///
/// Verified sources:
/// - cortex-code: `~/.snowflake/cortex/hooks.json` `hooks.SessionStart`
pub const KNOWN_HOOK_CONFIGS: &[AgentHookConfig] = &[AgentHookConfig {
    agent_id: "cortex-code",
    config_path: ".snowflake/cortex/hooks.json",
    hook_field: "hooks",
}];

fn hook_config_for(agent_id: &str) -> Result<&'static AgentHookConfig> {
    KNOWN_HOOK_CONFIGS
        .iter()
        .find(|c| c.agent_id == agent_id)
        .context("unknown or unverified Hook agent")
}

/// Resolve the Hook config path for a given agent ID, relative to `home`.
/// Returns `None` if the agent has no verified Hook config path.
pub fn resolve_hook_config_path(agent_id: &str, home: &Path) -> Option<PathBuf> {
    KNOWN_HOOK_CONFIGS
        .iter()
        .find(|c| c.agent_id == agent_id)
        .map(|c| home.join(c.config_path))
}

/// Whether the agent has a verified Hook write target (e.g. cortex-code).
/// The desktop-managed integration flow uses this to dispatch between the MCP
/// document transformer and the Hook document transformer.
pub fn is_verified_hook_agent(agent_id: &str) -> bool {
    KNOWN_HOOK_CONFIGS.iter().any(|c| c.agent_id == agent_id)
}

/// Build the exact, quoted `dtoken-agent-bridge` session-start command that the
/// desktop-managed hook flow writes into `hooks.SessionStart`. The bridge path
/// must be the desktop-resolved absolute executable path; this module never
/// guesses one.
pub fn quoted_bridge_command(bridge: &Path) -> String {
    format!(
        "\"{}\" hook session-start",
        bridge.display().to_string().replace('"', "\\\"")
    )
}

/// Transform a verified Hook configuration document for the given operation.
///
/// `add` selects insert vs remove. The d-token-owned `SessionStart` entry is
/// identified by `DTOKEN_HOOK_MANAGED_ID`; user-owned hook entries are always
/// preserved. `hooks.SessionStart` must be a JSON array; a non-array shape
/// fails closed.
pub fn transform_hook_json_document(
    agent_id: &str,
    text: &str,
    bridge: &Path,
    add: bool,
) -> Result<String> {
    let hook_config = hook_config_for(agent_id)?;
    let mut root: Map<String, Value> = if text.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(text).context("Hook config must be strict JSON")?
    };
    let hooks = root
        .entry(hook_config.hook_field.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("Hook config field must be a JSON object")?;
    let session_start = hooks
        .entry("SessionStart".to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .context("hooks.SessionStart must be a JSON array")?;
    if add {
        session_start.retain(|entry| {
            entry.pointer("/dtoken_managed_id").and_then(Value::as_str)
                != Some(DTOKEN_HOOK_MANAGED_ID)
        });
        session_start.push(json!({
            "dtoken_managed_id": DTOKEN_HOOK_MANAGED_ID,
            "matcher": "startup|resume|compact",
            "hooks": [{
                "type": "command",
                "command": quoted_bridge_command(bridge),
                "timeout": 5
            }]
        }));
    } else {
        session_start.retain(|entry| {
            entry.pointer("/dtoken_managed_id").and_then(Value::as_str)
                != Some(DTOKEN_HOOK_MANAGED_ID)
        });
    }
    Ok(serde_json::to_string_pretty(&root)?)
}

/// Verify that a verified Hook configuration document contains the
/// d-token-owned `SessionStart` entry with the exact desktop-resolved bridge
/// command.
pub fn verify_hook_json_document(agent_id: &str, text: &str, bridge: &Path) -> Result<()> {
    let hook_config = hook_config_for(agent_id)?;
    let root: Map<String, Value> =
        serde_json::from_str(text).context("Hook config must be a JSON object")?;
    let session_start = root
        .get(hook_config.hook_field)
        .and_then(|hooks| hooks.get("SessionStart"))
        .and_then(Value::as_array)
        .context("hooks.SessionStart must be a JSON array")?;
    let command = quoted_bridge_command(bridge);
    let present = session_start.iter().any(|entry| {
        entry.pointer("/dtoken_managed_id").and_then(Value::as_str) == Some(DTOKEN_HOOK_MANAGED_ID)
            && entry.pointer("/hooks/0/command").and_then(Value::as_str) == Some(command.as_str())
    });
    if !present {
        bail!(
            "d-token Hook entry is not present in {} configuration",
            agent_id
        );
    }
    Ok(())
}

/// Verify that a verified Hook configuration document contains no
/// d-token-owned `SessionStart` entry.
pub fn verify_hook_json_cleared(agent_id: &str, text: &str, _bridge: &Path) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let hook_config = hook_config_for(agent_id)?;
    let root: Map<String, Value> =
        serde_json::from_str(text).context("Hook config must be a JSON object")?;
    if let Some(session_start) = root
        .get(hook_config.hook_field)
        .and_then(|hooks| hooks.get("SessionStart"))
        .and_then(Value::as_array)
    {
        if session_start.iter().any(|entry| {
            entry.pointer("/dtoken_managed_id").and_then(Value::as_str)
                == Some(DTOKEN_HOOK_MANAGED_ID)
        }) {
            bail!(
                "d-token Hook entry is still present in {} configuration",
                agent_id
            );
        }
    }
    Ok(())
}

fn mcp_config_for(agent_id: &str) -> Result<&'static AgentMcpConfig> {
    KNOWN_MCP_CONFIGS
        .iter()
        .find(|c| c.agent_id == agent_id)
        .context("unknown or unverified MCP agent")
}

/// Insert the caller-supplied d-token MCP entry into a JSON MCP configuration.
///
/// The entry is added under the specified `field` (e.g. `mcpServers`). If the
/// field does not exist, it is created. If the d-token entry already exists
/// and equals `entry`, the document is left unchanged (idempotent); a
/// different value under the same key fails closed instead of being
/// overwritten. Other entries are preserved.
///
/// The caller must pass the exact, verified d-token entry (resolved
/// `dtoken-agent-bridge` command and its `mcp stdio` arguments). This module
/// never fabricates a command.
pub fn insert_dtoken_mcp_json_entry(
    config: &mut Map<String, Value>,
    field: &str,
    entry: &Value,
) -> Result<()> {
    let servers = config
        .entry(field)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("MCP servers field must be a JSON object")?;
    match servers.get(DTOKEN_MCP_ENTRY_KEY) {
        Some(existing) if existing == entry => Ok(()),
        Some(_) => bail!("d-token MCP entry already exists and is not managed by d-token"),
        None => {
            servers.insert(DTOKEN_MCP_ENTRY_KEY.to_string(), entry.clone());
            Ok(())
        }
    }
}

/// Remove the caller-supplied d-token MCP entry from a JSON MCP configuration.
///
/// Only an entry that matches `expected` exactly is removed. Returns `true`
/// if the d-token-owned entry was present and removed; a user-created entry
/// under the same key is never removed.
pub fn remove_dtoken_mcp_json_entry(
    config: &mut Map<String, Value>,
    field: &str,
    expected: &Value,
) -> bool {
    let Some(servers) = config.get_mut(field).and_then(Value::as_object_mut) else {
        return false;
    };
    if servers.get(DTOKEN_MCP_ENTRY_KEY) == Some(expected) {
        servers.remove(DTOKEN_MCP_ENTRY_KEY).is_some()
    } else {
        false
    }
}

/// Check whether a JSON MCP configuration contains the caller-supplied
/// d-token-owned entry (exact match) under the specified field.
pub fn has_dtoken_mcp_json_entry(
    config: &Map<String, Value>,
    field: &str,
    expected: &Value,
) -> bool {
    config
        .get(field)
        .and_then(Value::as_object)
        .and_then(|servers| servers.get(DTOKEN_MCP_ENTRY_KEY))
        .is_some_and(|entry| entry == expected)
}

/// Transform a verified MCP configuration document for the given operation.
///
/// `add` selects insert vs remove. The caller supplies the exact d-token
/// entry; the module never invents a command.
pub fn transform_mcp_json_document(
    agent_id: &str,
    text: &str,
    entry: &Value,
    add: bool,
) -> Result<String> {
    let mcp_config = mcp_config_for(agent_id)?;
    let mut root: Map<String, Value> = if text.trim().is_empty() {
        Map::new()
    } else {
        serde_json::from_str(text).context("MCP config must be strict JSON")?
    };
    if add {
        insert_dtoken_mcp_json_entry(&mut root, mcp_config.mcp_field, entry)?;
    } else if !remove_dtoken_mcp_json_entry(&mut root, mcp_config.mcp_field, entry) {
        return Ok(text.to_string());
    }
    Ok(serde_json::to_string_pretty(&root)?)
}

/// Verify that a verified MCP configuration document contains the
/// caller-supplied d-token-owned entry (exact match).
pub fn verify_mcp_json_document(agent_id: &str, text: &str, expected: &Value) -> Result<()> {
    let mcp_config = mcp_config_for(agent_id)?;
    let root: Map<String, Value> =
        serde_json::from_str(text).context("MCP config must be a JSON object")?;
    if !has_dtoken_mcp_json_entry(&root, mcp_config.mcp_field, expected) {
        bail!(
            "d-token MCP entry is not present in {} configuration",
            agent_id
        );
    }
    Ok(())
}

/// Verify that a verified MCP configuration document contains no
/// caller-supplied d-token-owned entry.
pub fn verify_mcp_json_cleared(agent_id: &str, text: &str, expected: &Value) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let mcp_config = mcp_config_for(agent_id)?;
    let root: Map<String, Value> =
        serde_json::from_str(text).context("MCP config must be a JSON object")?;
    if has_dtoken_mcp_json_entry(&root, mcp_config.mcp_field, expected) {
        bail!(
            "d-token MCP entry is still present in {} configuration",
            agent_id
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The verified metadata-only bridge entry shape a caller must supply:
    /// a resolved absolute `dtoken-agent-bridge` executable with `mcp stdio`.
    fn bridge_entry(command: &str) -> Value {
        json!({ "command": command, "args": ["mcp", "stdio"] })
    }

    #[test]
    fn insert_and_remove_dtoken_mcp_json_entry_roundtrip() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let mut config = Map::new();
        config.insert("other".to_string(), json!({"key": "value"}));
        insert_dtoken_mcp_json_entry(&mut config, "mcpServers", &entry).unwrap();
        assert!(has_dtoken_mcp_json_entry(&config, "mcpServers", &entry));
        assert_eq!(
            config
                .get("mcpServers")
                .and_then(Value::as_object)
                .and_then(|s| s.get("d-token"))
                .and_then(|e| e.get("command"))
                .and_then(Value::as_str),
            Some("/opt/d-token/dtoken-agent-bridge")
        );
        assert!(config.contains_key("other"));
        assert!(remove_dtoken_mcp_json_entry(
            &mut config,
            "mcpServers",
            &entry
        ));
        assert!(!has_dtoken_mcp_json_entry(&config, "mcpServers", &entry));
        assert!(config.contains_key("other"));
    }

    #[test]
    fn insert_dtoken_mcp_entry_creates_field_if_missing() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let mut config = Map::new();
        insert_dtoken_mcp_json_entry(&mut config, "mcpServers", &entry).unwrap();
        assert!(has_dtoken_mcp_json_entry(&config, "mcpServers", &entry));
    }

    #[test]
    fn insert_is_idempotent_for_identical_entry() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let mut config = Map::new();
        insert_dtoken_mcp_json_entry(&mut config, "mcpServers", &entry).unwrap();
        insert_dtoken_mcp_json_entry(&mut config, "mcpServers", &entry).unwrap();
        assert!(has_dtoken_mcp_json_entry(&config, "mcpServers", &entry));
    }

    #[test]
    fn conflicting_entry_under_same_key_fails_closed() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let mut config = Map::new();
        config.insert(
            "mcpServers".to_string(),
            json!({"d-token": {"command": "user-owned"}}),
        );
        let error = insert_dtoken_mcp_json_entry(&mut config, "mcpServers", &entry).unwrap_err();
        assert!(error.to_string().contains("not managed by d-token"));
    }

    #[test]
    fn remove_requires_exact_match_and_preserves_user_entry() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let mut config = Map::new();
        config.insert(
            "mcpServers".to_string(),
            json!({"d-token": {"command": "user-owned"}}),
        );
        assert!(!remove_dtoken_mcp_json_entry(
            &mut config,
            "mcpServers",
            &entry
        ));
        assert!(config
            .get("mcpServers")
            .and_then(Value::as_object)
            .and_then(|s| s.get("d-token"))
            .is_some());
    }

    #[test]
    fn remove_nonexistent_dtoken_mcp_entry_returns_false() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let mut config = Map::new();
        config.insert("mcpServers".to_string(), json!({"other": {}}));
        assert!(!remove_dtoken_mcp_json_entry(
            &mut config,
            "mcpServers",
            &entry
        ));
    }

    #[test]
    fn has_dtoken_mcp_entry_requires_exact_command_and_args() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let mut config = Map::new();
        config.insert(
            "mcpServers".to_string(),
            json!({"d-token": {"command": "/opt/d-token/dtoken-agent-bridge", "args": ["mcp", "stdio"]}}),
        );
        assert!(has_dtoken_mcp_json_entry(&config, "mcpServers", &entry));
        let mut config2 = Map::new();
        config2.insert(
            "mcpServers".to_string(),
            json!({"d-token": {"command": "malicious"}}),
        );
        assert!(!has_dtoken_mcp_json_entry(&config2, "mcpServers", &entry));
    }

    #[test]
    fn transform_mcp_json_document_configure_clear_roundtrip() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let original = r#"{"existing": true}"#;
        let configured = transform_mcp_json_document("cursor", original, &entry, true).unwrap();
        let parsed: Value = serde_json::from_str(&configured).unwrap();
        assert_eq!(parsed.pointer("/mcpServers/d-token"), Some(&entry));
        assert_eq!(parsed.pointer("/existing"), Some(&json!(true)));
        let cleared = transform_mcp_json_document("cursor", &configured, &entry, false).unwrap();
        let parsed: Value = serde_json::from_str(&cleared).unwrap();
        assert!(parsed.pointer("/mcpServers/d-token").is_none());
        assert_eq!(parsed.pointer("/existing"), Some(&json!(true)));
    }

    #[test]
    fn clear_without_owned_entry_preserves_exact_original_bytes() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let original = "{\n  \"mcpServers\": {\"user\": {\"command\": \"custom\"}}\n}\n";
        let cleared = transform_mcp_json_document("cursor", original, &entry, false).unwrap();
        assert_eq!(cleared, original);
    }

    #[test]
    fn verify_mcp_json_document_passes_for_configured_state() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let text = r#"{"mcpServers":{"d-token":{"command":"/opt/d-token/dtoken-agent-bridge","args":["mcp","stdio"]}}}"#;
        assert!(verify_mcp_json_document("cursor", text, &entry).is_ok());
    }

    #[test]
    fn verify_mcp_json_cleared_fails_when_entry_present() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let text = r#"{"mcpServers":{"d-token":{"command":"/opt/d-token/dtoken-agent-bridge","args":["mcp","stdio"]}}}"#;
        assert!(verify_mcp_json_cleared("cursor", text, &entry).is_err());
    }

    #[test]
    fn verify_mcp_json_cleared_passes_when_empty() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        assert!(verify_mcp_json_cleared("cursor", "", &entry).is_ok());
    }

    #[test]
    fn unknown_agent_has_no_verified_mcp_config() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        assert!(transform_mcp_json_document("antigravity", "{}", &entry, true).is_err());
        assert!(verify_mcp_json_document("antigravity", "{}", &entry).is_err());
        assert!(verify_mcp_json_cleared("antigravity", "{}", &entry).is_err());
        assert!(resolve_mcp_config_path("antigravity", Path::new("/home/user")).is_none());
    }

    #[test]
    fn resolve_mcp_config_path_returns_known_paths() {
        let home = Path::new("/home/user");
        let path = resolve_mcp_config_path("cursor", home);
        assert_eq!(path, Some(home.join(".cursor/mcp.json")));
        let path = resolve_mcp_config_path("claude-desktop", home);
        assert_eq!(
            path,
            Some(home.join("Library/Application Support/Claude/claude_desktop_config.json"))
        );
        let path = resolve_mcp_config_path("unknown", home);
        assert_eq!(path, None);
    }

    #[test]
    fn vscode_copilot_chat_has_verified_macos_and_windows_mcp_json_paths() {
        // macOS: VS Code user-level mcp.json is home-relative.
        let home = Path::new("/home/user");
        assert_eq!(
            resolve_mcp_config_path_for("vscode-copilot-chat", HostPlatform::Macos, home, None,),
            Some(home.join("Library/Application Support/Code/User/mcp.json"))
        );
        // Windows: VS Code user-level mcp.json lives under RoamingAppData.
        let roaming = Path::new("C:/Users/user/AppData/Roaming");
        assert_eq!(
            resolve_mcp_config_path_for(
                "vscode-copilot-chat",
                HostPlatform::Windows,
                home,
                Some(roaming),
            ),
            Some(roaming.join("Code/User/mcp.json"))
        );
        // VS Code mcp.json uses the `servers` key, never `mcpServers`.
        assert_eq!(
            mcp_config_for("vscode-copilot-chat").unwrap().mcp_field,
            "servers"
        );
    }

    #[test]
    fn claude_desktop_windows_resolves_to_roaming_app_data_target() {
        let home = Path::new("C:/Users/test");
        let roaming = Path::new("C:/Users/test/AppData/Roaming");
        let path = resolve_mcp_config_path_for(
            "claude-desktop",
            HostPlatform::Windows,
            home,
            Some(roaming),
        )
        .expect("Windows Claude Desktop target must resolve");
        let rendered = path.to_string_lossy().replace('\\', "/");
        assert!(
            rendered.contains("AppData/Roaming/Claude/claude_desktop_config.json"),
            "Windows Claude Desktop must resolve under %APPDATA%, got {rendered}"
        );
        // macOS Claude Desktop remains home-relative.
        assert_eq!(
            resolve_mcp_config_path_for("claude-desktop", HostPlatform::Macos, home, Some(roaming)),
            Some(home.join("Library/Application Support/Claude/claude_desktop_config.json"))
        );
    }

    #[test]
    fn vscode_copilot_chat_transform_verify_clear_roundtrip_uses_servers_field() {
        let entry = bridge_entry("/opt/d-token/dtoken-agent-bridge");
        let original = r#"{"servers":{"user-tool":{"command":"custom"}}}"#;
        let configured =
            transform_mcp_json_document("vscode-copilot-chat", original, &entry, true).unwrap();
        let parsed: Value = serde_json::from_str(&configured).unwrap();
        assert_eq!(parsed.pointer("/servers/d-token"), Some(&entry));
        assert_eq!(
            parsed.pointer("/servers/user-tool"),
            Some(&json!({"command": "custom"}))
        );
        assert!(parsed.get("mcpServers").is_none());
        assert!(verify_mcp_json_document("vscode-copilot-chat", &configured, &entry).is_ok());

        let cleared =
            transform_mcp_json_document("vscode-copilot-chat", &configured, &entry, false).unwrap();
        let parsed: Value = serde_json::from_str(&cleared).unwrap();
        assert!(parsed.pointer("/servers/d-token").is_none());
        assert_eq!(
            parsed.pointer("/servers/user-tool"),
            Some(&json!({"command": "custom"}))
        );
        assert!(verify_mcp_json_cleared("vscode-copilot-chat", &cleared, &entry).is_ok());
    }

    #[test]
    fn cortex_hook_path_resolves_to_desktop_resolved_hook_config() {
        let home = Path::new("/home/user");
        assert_eq!(
            resolve_hook_config_path("cortex-code", home),
            Some(home.join(".snowflake/cortex/hooks.json"))
        );
        // Unknown or MCP-only agents have no verified hook config.
        assert_eq!(resolve_hook_config_path("cursor", home), None);
        assert_eq!(resolve_hook_config_path("antigravity", home), None);
    }

    #[test]
    fn cortex_hook_document_transform_verify_clear_roundtrip() {
        let bridge = Path::new("/opt/d-token/dtoken-agent-bridge");
        let original = r#"{"hooks":{"SessionStart":[{"dtoken_managed_id":"user-hook","matcher":"startup","hooks":[{"type":"command","command":"echo hi"}]}]}}"#;
        let configured =
            transform_hook_json_document("cortex-code", original, bridge, true).unwrap();
        let parsed: Value = serde_json::from_str(&configured).unwrap();
        let session_start = parsed
            .pointer("/hooks/SessionStart")
            .and_then(Value::as_array)
            .expect("SessionStart array");
        let managed = session_start
            .iter()
            .find(|entry| {
                entry.pointer("/dtoken_managed_id").and_then(Value::as_str)
                    == Some(DTOKEN_HOOK_MANAGED_ID)
            })
            .expect("d-token managed hook entry");
        assert_eq!(
            managed.pointer("/hooks/0/command").and_then(Value::as_str),
            Some(quoted_bridge_command(bridge).as_str())
        );
        assert_eq!(session_start.len(), 2, "user hook is preserved");
        assert!(verify_hook_json_document("cortex-code", &configured, bridge).is_ok());

        let cleared =
            transform_hook_json_document("cortex-code", &configured, bridge, false).unwrap();
        let parsed: Value = serde_json::from_str(&cleared).unwrap();
        let session_start = parsed
            .pointer("/hooks/SessionStart")
            .and_then(Value::as_array)
            .expect("SessionStart array");
        assert!(session_start.iter().all(|entry| entry
            .pointer("/dtoken_managed_id")
            .and_then(Value::as_str)
            != Some(DTOKEN_HOOK_MANAGED_ID)));
        assert_eq!(session_start.len(), 1, "user hook is preserved after clear");
        assert!(verify_hook_json_cleared("cortex-code", &cleared, bridge).is_ok());
    }

    #[test]
    fn cortex_hook_document_fails_closed_on_unknown_agent_and_missing_hooks_field() {
        let bridge = Path::new("/opt/d-token/dtoken-agent-bridge");
        assert!(transform_hook_json_document("cursor", "{}", bridge, true).is_err());
        assert!(verify_hook_json_document("cursor", "{}", bridge).is_err());
        assert!(transform_hook_json_document("cortex-code", "not json", bridge, true).is_err());
        // hooks.SessionStart must be an array; a non-array fails closed.
        assert!(transform_hook_json_document(
            "cortex-code",
            r#"{"hooks":{"SessionStart":"string"}}"#,
            bridge,
            true
        )
        .is_err());
    }
}
