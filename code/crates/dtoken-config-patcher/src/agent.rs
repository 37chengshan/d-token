pub use dtoken_types::agent::{
    AgentDetectionEvidence, AgentDiscoverySnapshot, AgentEvidenceKind, AgentInstallCapability,
    AgentSessionCapability, DetectedAgent, HostPlatform,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    env, fmt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentPathBase {
    Home,
    Applications,
    RoamingAppData,
    LocalAppData,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPathSpec {
    pub platform: Option<HostPlatform>,
    pub base: AgentPathBase,
    pub relative: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentAdapterDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub executables: &'static [&'static str],
    pub install_capability: AgentInstallCapability,
    pub session_capability: AgentSessionCapability,
    pub config_paths: &'static [AgentPathSpec],
    pub session_paths: &'static [AgentPathSpec],
    pub app_paths: &'static [AgentPathSpec],
    pub extension_prefixes: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentScanContext {
    pub platform: HostPlatform,
    pub home_dir: PathBuf,
    pub path_dirs: Vec<PathBuf>,
    pub applications_dir: Option<PathBuf>,
    pub roaming_app_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub additional_evidence: Vec<AgentEvidenceOverride>,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentEvidenceOverride {
    pub agent_id: String,
    pub kind: AgentEvidenceKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRegistryError {
    InvalidAdapterId(String),
    EmptyAdapterName(String),
    DuplicateAdapterId(String),
    UnknownEvidenceAdapter(String),
    MissingHomeDirectory,
    InvalidObservationTimestamp,
}

impl fmt::Display for AgentRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAdapterId(id) => write!(formatter, "invalid Agent adapter id: {id}"),
            Self::EmptyAdapterName(id) => write!(formatter, "Agent adapter name is empty: {id}"),
            Self::DuplicateAdapterId(id) => write!(formatter, "duplicate Agent adapter id: {id}"),
            Self::UnknownEvidenceAdapter(id) => {
                write!(formatter, "Agent evidence references unknown adapter: {id}")
            }
            Self::MissingHomeDirectory => {
                formatter.write_str("Agent scan requires an explicit home directory")
            }
            Self::InvalidObservationTimestamp => {
                formatter.write_str("Agent scan observation timestamp must be positive")
            }
        }
    }
}

impl std::error::Error for AgentRegistryError {}

const fn path(
    platform: Option<HostPlatform>,
    base: AgentPathBase,
    relative: &'static str,
) -> AgentPathSpec {
    AgentPathSpec {
        platform,
        base,
        relative,
    }
}

const OPENCODE_CONFIG: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".config/opencode/opencode.json"),
    path(None, AgentPathBase::Home, ".config/opencode/opencode.jsonc"),
];
const OPENCODE_SESSIONS: &[AgentPathSpec] = &[
    path(
        None,
        AgentPathBase::Home,
        ".local/share/opencode/storage/session",
    ),
    path(
        None,
        AgentPathBase::Home,
        ".local/share/opencode/opencode.db",
    ),
];
const CLAUDE_CONFIG: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".claude/settings.json"),
    path(None, AgentPathBase::Home, ".claude.json"),
];
const CLAUDE_SESSIONS: &[AgentPathSpec] = &[path(None, AgentPathBase::Home, ".claude/projects")];
const CODEX_CONFIG: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".codex/config.toml"),
    path(None, AgentPathBase::Home, ".codex/config.json"),
];
const CODEX_SESSIONS: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".codex/sessions"),
    path(None, AgentPathBase::Home, ".codex/archived_sessions"),
];
const CURSOR_CONFIG: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".cursor/settings.json"),
    path(
        Some(HostPlatform::Macos),
        AgentPathBase::Home,
        "Library/Application Support/Cursor/User/settings.json",
    ),
    path(
        Some(HostPlatform::Windows),
        AgentPathBase::RoamingAppData,
        "Cursor/User/settings.json",
    ),
];
const CURSOR_SESSIONS: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".cursor/projects"),
    path(None, AgentPathBase::Home, ".cursor/workspaceStorage"),
    path(
        Some(HostPlatform::Macos),
        AgentPathBase::Home,
        "Library/Application Support/Cursor/User/workspaceStorage",
    ),
    path(
        Some(HostPlatform::Windows),
        AgentPathBase::RoamingAppData,
        "Cursor/User/workspaceStorage",
    ),
];
const CURSOR_APPS: &[AgentPathSpec] = &[path(
    Some(HostPlatform::Macos),
    AgentPathBase::Applications,
    "Cursor.app",
)];
const CLINE_CONFIG: &[AgentPathSpec] = &[
    // Cline CLI 3.x stores provider selection in this versioned JSON file.
    // The VS Code extension paths below remain discovery-only evidence and
    // must not be treated as an interchangeable transport schema.
    path(
        None,
        AgentPathBase::Home,
        ".cline/data/settings/providers.json",
    ),
    // Retain the pre-3.0 location as read/write-compatible legacy evidence.
    path(None, AgentPathBase::Home, ".cline/settings/providers.json"),
    path(
        Some(HostPlatform::Macos),
        AgentPathBase::Home,
        "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings.json",
    ),
    path(
        Some(HostPlatform::Windows),
        AgentPathBase::RoamingAppData,
        "Code/User/globalStorage/saoudrizwan.claude-dev/settings.json",
    ),
];
const CLINE_SESSIONS: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".cline/data/sessions"),
    path(None, AgentPathBase::Home, ".cline/tasks"),
    path(
        None,
        AgentPathBase::Home,
        ".cursor/globalStorage/saoudrizwan.claude-dev/tasks",
    ),
    path(
        Some(HostPlatform::Macos),
        AgentPathBase::Home,
        "Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks",
    ),
    path(
        Some(HostPlatform::Macos),
        AgentPathBase::Home,
        "Library/Application Support/Cursor/User/globalStorage/saoudrizwan.claude-dev/tasks",
    ),
    path(
        Some(HostPlatform::Windows),
        AgentPathBase::RoamingAppData,
        "Code/User/globalStorage/saoudrizwan.claude-dev/tasks",
    ),
    path(
        Some(HostPlatform::Windows),
        AgentPathBase::RoamingAppData,
        "Cursor/User/globalStorage/saoudrizwan.claude-dev/tasks",
    ),
];
const COMMANDCODE_CONFIG: &[AgentPathSpec] =
    &[path(None, AgentPathBase::Home, ".commandcode/config.json")];
const COMMANDCODE_SESSIONS: &[AgentPathSpec] = &[path(None, AgentPathBase::Home, ".commandcode")];
const CLAUDE_DESKTOP_CONFIG: &[AgentPathSpec] = &[
    path(
        Some(HostPlatform::Macos),
        AgentPathBase::Home,
        "Library/Application Support/Claude/claude_desktop_config.json",
    ),
    path(
        Some(HostPlatform::Windows),
        AgentPathBase::RoamingAppData,
        "Claude/claude_desktop_config.json",
    ),
];
const CLAUDE_DESKTOP_APPS: &[AgentPathSpec] = &[path(
    Some(HostPlatform::Macos),
    AgentPathBase::Applications,
    "Claude.app",
)];

macro_rules! home_config {
    ($name:ident, $relative:literal) => {
        const $name: &[AgentPathSpec] = &[path(None, AgentPathBase::Home, $relative)];
    };
}

home_config!(OPENCLAW_CONFIG, ".openclaw/openclaw.json");
home_config!(GEMINI_CONFIG, ".gemini/settings.json");
home_config!(HERMES_CONFIG, ".hermes/config.yaml");
const OPENCLAW_SESSIONS: &[AgentPathSpec] = &[path(None, AgentPathBase::Home, ".openclaw/agents")];
const GEMINI_SESSIONS: &[AgentPathSpec] = &[path(None, AgentPathBase::Home, ".gemini/tmp")];
const HERMES_SESSIONS: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".hermes/state.db"),
    path(None, AgentPathBase::Home, ".hermes/sessions"),
];
home_config!(ROO_CONFIG, ".roo/settings.json");
const CONTINUE_CONFIG: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".continue/config.json"),
    path(None, AgentPathBase::Home, ".continue/config.yaml"),
];
home_config!(AIDER_CONFIG, ".aider.conf.yml");
home_config!(GOOSE_CONFIG, ".config/goose/config.yaml");
home_config!(OPENHANDS_CONFIG, ".openhands/config.toml");
home_config!(COPILOT_CONFIG, ".copilot/config.json");
const VSCODE_COPILOT_CONFIG: &[AgentPathSpec] = &[
    path(
        Some(HostPlatform::Macos),
        AgentPathBase::Home,
        "Library/Application Support/Code/User/settings.json",
    ),
    path(
        Some(HostPlatform::Macos),
        AgentPathBase::Home,
        "Library/Application Support/Code - Insiders/User/settings.json",
    ),
    path(
        Some(HostPlatform::Windows),
        AgentPathBase::RoamingAppData,
        "Code/User/settings.json",
    ),
    path(
        Some(HostPlatform::Windows),
        AgentPathBase::RoamingAppData,
        "Code - Insiders/User/settings.json",
    ),
    path(
        Some(HostPlatform::Linux),
        AgentPathBase::Home,
        ".config/Code/User/settings.json",
    ),
    path(
        Some(HostPlatform::Linux),
        AgentPathBase::Home,
        ".config/Code - Insiders/User/settings.json",
    ),
];
const PI_CONFIG: &[AgentPathSpec] = &[
    path(None, AgentPathBase::Home, ".pi/agent/settings.json"),
    path(None, AgentPathBase::Home, ".pi/agent/models.json"),
];
home_config!(CORTEX_CODE_CONFIG, ".snowflake/cortex/hooks.json");
home_config!(
    WINDSURF_CONFIG,
    ".codeium/windsurf/memories/global_rules.md"
);
home_config!(KILO_CONFIG, ".kilocode/settings.json");
home_config!(ANTIGRAVITY_CONFIG, ".antigravity/settings.json");
home_config!(VIBE_CONFIG, ".vibe/config.toml");

const WINDSURF_APPS: &[AgentPathSpec] = &[path(
    Some(HostPlatform::Macos),
    AgentPathBase::Applications,
    "Windsurf.app",
)];
const ANTIGRAVITY_APPS: &[AgentPathSpec] = &[path(
    Some(HostPlatform::Macos),
    AgentPathBase::Applications,
    "Antigravity.app",
)];

pub const KNOWN_AGENT_ADAPTERS: &[AgentAdapterDescriptor] = &[
    AgentAdapterDescriptor {
        id: "opencode",
        name: "OpenCode",
        executables: &["opencode"],
        install_capability: AgentInstallCapability::OneClick,
        session_capability: AgentSessionCapability::Metadata,
        config_paths: OPENCODE_CONFIG,
        session_paths: OPENCODE_SESSIONS,
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "claude-code",
        name: "Claude Code",
        executables: &["claude"],
        install_capability: AgentInstallCapability::ConfirmRequired,
        session_capability: AgentSessionCapability::Metadata,
        config_paths: CLAUDE_CONFIG,
        session_paths: CLAUDE_SESSIONS,
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "codex",
        name: "Codex",
        executables: &["codex"],
        install_capability: AgentInstallCapability::ConfirmRequired,
        session_capability: AgentSessionCapability::Metadata,
        config_paths: CODEX_CONFIG,
        session_paths: CODEX_SESSIONS,
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "cursor",
        name: "Cursor",
        executables: &["cursor"],
        install_capability: AgentInstallCapability::PromptOnly,
        session_capability: AgentSessionCapability::Metadata,
        config_paths: CURSOR_CONFIG,
        session_paths: CURSOR_SESSIONS,
        app_paths: CURSOR_APPS,
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "cline",
        name: "Cline",
        executables: &["cline"],
        install_capability: AgentInstallCapability::PromptOnly,
        session_capability: AgentSessionCapability::Metadata,
        config_paths: CLINE_CONFIG,
        session_paths: CLINE_SESSIONS,
        app_paths: &[],
        extension_prefixes: &["saoudrizwan.claude-dev-", "cline.cline-"],
    },
    AgentAdapterDescriptor {
        id: "commandcode",
        name: "CommandCode",
        executables: &["commandcode", "command-code"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::DescriptorOnly,
        config_paths: COMMANDCODE_CONFIG,
        session_paths: COMMANDCODE_SESSIONS,
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "claude-desktop",
        name: "Claude Desktop",
        executables: &[],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: CLAUDE_DESKTOP_CONFIG,
        session_paths: &[],
        app_paths: CLAUDE_DESKTOP_APPS,
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "openclaw",
        name: "OpenClaw",
        executables: &["openclaw"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Metadata,
        config_paths: OPENCLAW_CONFIG,
        session_paths: OPENCLAW_SESSIONS,
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "gemini-cli",
        name: "Gemini CLI",
        executables: &["gemini"],
        install_capability: AgentInstallCapability::PromptOnly,
        session_capability: AgentSessionCapability::Metadata,
        config_paths: GEMINI_CONFIG,
        session_paths: GEMINI_SESSIONS,
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "hermes",
        name: "Hermes",
        executables: &["hermes"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Metadata,
        config_paths: HERMES_CONFIG,
        session_paths: HERMES_SESSIONS,
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "roo-code",
        name: "Roo Code",
        executables: &["roo"],
        install_capability: AgentInstallCapability::PromptOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: ROO_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &["rooveterinaryinc.roo-cline-"],
    },
    AgentAdapterDescriptor {
        id: "continue",
        name: "Continue",
        executables: &["continue"],
        install_capability: AgentInstallCapability::PromptOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: CONTINUE_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &["continue.continue-"],
    },
    AgentAdapterDescriptor {
        id: "aider",
        name: "Aider",
        executables: &["aider"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: AIDER_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "goose",
        name: "Goose",
        executables: &["goose"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: GOOSE_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "openhands",
        name: "OpenHands",
        executables: &["openhands"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: OPENHANDS_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "copilot-cli",
        name: "Copilot CLI",
        executables: &["copilot"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: COPILOT_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "vscode-copilot-chat",
        name: "VS Code Copilot Chat",
        executables: &[],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: VSCODE_COPILOT_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &["github.copilot-chat-"],
    },
    AgentAdapterDescriptor {
        id: "pi",
        name: "Pi",
        executables: &["pi"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: PI_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "cortex-code",
        name: "Cortex Code",
        executables: &[],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: CORTEX_CODE_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "windsurf",
        name: "Windsurf",
        executables: &["windsurf"],
        install_capability: AgentInstallCapability::PromptOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: WINDSURF_CONFIG,
        session_paths: &[],
        app_paths: WINDSURF_APPS,
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "kilo-code",
        name: "Kilo Code",
        executables: &["kilo"],
        install_capability: AgentInstallCapability::PromptOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: KILO_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &["kilocode.kilo-code-"],
    },
    AgentAdapterDescriptor {
        id: "antigravity",
        name: "Google Antigravity",
        executables: &["antigravity"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: ANTIGRAVITY_CONFIG,
        session_paths: &[],
        app_paths: ANTIGRAVITY_APPS,
        extension_prefixes: &[],
    },
    AgentAdapterDescriptor {
        id: "mistral-vibe",
        name: "Mistral Vibe",
        executables: &["vibe"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: VIBE_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &[],
    },
];

pub fn known_agent_adapters() -> &'static [AgentAdapterDescriptor] {
    KNOWN_AGENT_ADAPTERS
}

pub fn agent_adapter(id: &str) -> Option<&'static AgentAdapterDescriptor> {
    KNOWN_AGENT_ADAPTERS.iter().find(|adapter| adapter.id == id)
}

/// Returns every registry-owned configuration target that the current Agent
/// adapter may use in the supplied scan context. Unlike discovery evidence,
/// this includes known targets that do not exist yet so a caller can validate
/// a guarded first-time file creation without accepting an arbitrary path.
pub fn allowed_agent_config_paths(
    agent_id: &str,
    context: &AgentScanContext,
) -> Result<Vec<PathBuf>, AgentRegistryError> {
    let adapter = agent_adapter(agent_id)
        .ok_or_else(|| AgentRegistryError::InvalidAdapterId(agent_id.to_string()))?;
    let mut paths = adapter
        .config_paths
        .iter()
        .filter(|spec| spec.platform.is_none() || spec.platform == Some(context.platform))
        .flat_map(|spec| resolve_paths(*spec, context))
        .collect::<Vec<_>>();
    paths.extend(
        context
            .additional_evidence
            .iter()
            .filter(|evidence| {
                evidence.agent_id == agent_id
                    && evidence.kind == AgentEvidenceKind::Config
                    && evidence.path.is_absolute()
            })
            .map(|evidence| evidence.path.clone()),
    );
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub fn scan_current_process_agents() -> Result<AgentDiscoverySnapshot, AgentRegistryError> {
    let observed_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AgentRegistryError::InvalidObservationTimestamp)?
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    let context = current_process_scan_context(observed_at_ms)?;
    scan_agent_adapters(known_agent_adapters(), &context)
}

pub fn current_process_scan_context(
    observed_at_ms: u64,
) -> Result<AgentScanContext, AgentRegistryError> {
    let platform = if cfg!(target_os = "macos") {
        HostPlatform::Macos
    } else if cfg!(windows) {
        HostPlatform::Windows
    } else {
        HostPlatform::Linux
    };
    let home_dir = env::var_os(if platform == HostPlatform::Windows {
        "USERPROFILE"
    } else {
        "HOME"
    })
    .map(PathBuf::from)
    .filter(|path| path.is_absolute())
    .ok_or(AgentRegistryError::MissingHomeDirectory)?;
    let path_dirs = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default();
    let codex_home = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| Some(home_dir.join(".codex")));
    let additional_evidence = environment_evidence(
        env::var_os("DTOKEN_OPENCODE_CONFIG_PATH")
            .or_else(|| env::var_os("OPENCODE_CONFIG"))
            .map(PathBuf::from),
        env::var_os("OPENCODE_CONFIG_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute()),
        codex_home,
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute()),
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute()),
    );
    Ok(AgentScanContext {
        platform,
        home_dir,
        path_dirs,
        applications_dir: (platform == HostPlatform::Macos).then(|| PathBuf::from("/Applications")),
        roaming_app_data: env::var_os("APPDATA").map(PathBuf::from),
        local_app_data: env::var_os("LOCALAPPDATA").map(PathBuf::from),
        additional_evidence,
        observed_at_ms,
    })
}

fn environment_evidence(
    opencode_config_path: Option<PathBuf>,
    opencode_config_dir: Option<PathBuf>,
    codex_home: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
    xdg_data_home: Option<PathBuf>,
) -> Vec<AgentEvidenceOverride> {
    let mut evidence = Vec::new();
    if let Some(path) = opencode_config_path {
        evidence.push(AgentEvidenceOverride {
            agent_id: "opencode".to_string(),
            kind: AgentEvidenceKind::Config,
            path,
        });
    }
    for root in [
        opencode_config_dir,
        xdg_config_home.map(|root| root.join("opencode")),
    ]
    .into_iter()
    .flatten()
    {
        for file_name in ["config.json", "opencode.json", "opencode.jsonc"] {
            evidence.push(AgentEvidenceOverride {
                agent_id: "opencode".to_string(),
                kind: AgentEvidenceKind::Config,
                path: root.join(file_name),
            });
        }
    }
    if let Some(codex_home) = codex_home {
        evidence.push(AgentEvidenceOverride {
            agent_id: "codex".to_string(),
            kind: AgentEvidenceKind::Config,
            path: codex_home.join("config.toml"),
        });
        if let Ok(entries) = std::fs::read_dir(&codex_home) {
            evidence.extend(entries.filter_map(Result::ok).filter_map(|entry| {
                let path = entry.path();
                let name = path.file_name()?.to_str()?;
                (path.is_file() && name.ends_with(".config.toml")).then(|| AgentEvidenceOverride {
                    agent_id: "codex".to_string(),
                    kind: AgentEvidenceKind::Config,
                    path,
                })
            }));
        }
        for path in [
            codex_home.join("sessions"),
            codex_home.join("archived_sessions"),
        ] {
            evidence.push(AgentEvidenceOverride {
                agent_id: "codex".to_string(),
                kind: AgentEvidenceKind::SessionRoot,
                path,
            });
        }
    }
    if let Some(xdg_data_home) = xdg_data_home {
        for path in [
            xdg_data_home.join("opencode/opencode.db"),
            xdg_data_home.join("opencode/storage/session"),
        ] {
            evidence.push(AgentEvidenceOverride {
                agent_id: "opencode".to_string(),
                kind: AgentEvidenceKind::SessionRoot,
                path,
            });
        }
    }
    evidence
}

pub fn scan_agent_adapters(
    adapters: &[AgentAdapterDescriptor],
    context: &AgentScanContext,
) -> Result<AgentDiscoverySnapshot, AgentRegistryError> {
    validate_registry(adapters, context)?;
    let mut agents = Vec::new();
    for adapter in adapters {
        let mut evidence = executable_evidence(adapter, context);
        evidence.extend(path_evidence(
            adapter.config_paths,
            context,
            AgentEvidenceKind::Config,
        ));
        evidence.extend(path_evidence(
            adapter.session_paths,
            context,
            AgentEvidenceKind::SessionRoot,
        ));
        evidence.extend(path_evidence(
            adapter.app_paths,
            context,
            AgentEvidenceKind::AppBundle,
        ));
        evidence.extend(extension_evidence(adapter, context));
        if adapter.id == "vscode-copilot-chat"
            && !evidence
                .iter()
                .any(|item| item.kind == AgentEvidenceKind::Extension)
        {
            // VS Code's shared settings.json is not evidence that Copilot Chat
            // is installed. Keep it as an allowed target path, but require the
            // extension directory before projecting this Agent as detected.
            evidence.retain(|item| item.kind != AgentEvidenceKind::Config);
        }
        evidence.extend(
            context
                .additional_evidence
                .iter()
                .filter(|item| item.agent_id == adapter.id && item.path.exists())
                .map(|item| AgentDetectionEvidence {
                    kind: item.kind.clone(),
                    path: item.path.clone(),
                }),
        );
        evidence.sort_by(|left, right| (&left.kind, &left.path).cmp(&(&right.kind, &right.path)));
        evidence.dedup();
        if !evidence.is_empty() {
            agents.push(DetectedAgent {
                id: adapter.id.to_string(),
                name: adapter.name.to_string(),
                install_capability: adapter.install_capability,
                session_capability: adapter.session_capability,
                evidence,
            });
        }
    }
    agents.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(AgentDiscoverySnapshot {
        observed_at_ms: context.observed_at_ms,
        evidence_revision: evidence_revision(&agents),
        agents,
    })
}

fn validate_registry(
    adapters: &[AgentAdapterDescriptor],
    context: &AgentScanContext,
) -> Result<(), AgentRegistryError> {
    if context.observed_at_ms == 0 {
        return Err(AgentRegistryError::InvalidObservationTimestamp);
    }
    let mut ids = HashSet::new();
    for adapter in adapters {
        if adapter.id.is_empty()
            || !adapter
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(AgentRegistryError::InvalidAdapterId(adapter.id.to_string()));
        }
        if adapter.name.trim().is_empty() {
            return Err(AgentRegistryError::EmptyAdapterName(adapter.id.to_string()));
        }
        if !ids.insert(adapter.id) {
            return Err(AgentRegistryError::DuplicateAdapterId(
                adapter.id.to_string(),
            ));
        }
    }
    for evidence in &context.additional_evidence {
        if !ids.contains(evidence.agent_id.as_str()) {
            return Err(AgentRegistryError::UnknownEvidenceAdapter(
                evidence.agent_id.clone(),
            ));
        }
    }
    Ok(())
}

fn extension_evidence(
    adapter: &AgentAdapterDescriptor,
    context: &AgentScanContext,
) -> Vec<AgentDetectionEvidence> {
    if adapter.extension_prefixes.is_empty() {
        return Vec::new();
    }
    [
        context.home_dir.join(".vscode/extensions"),
        context.home_dir.join(".vscode-insiders/extensions"),
        context.home_dir.join(".cursor/extensions"),
        context.home_dir.join(".windsurf/extensions"),
    ]
    .into_iter()
    .filter_map(|root| std::fs::read_dir(root).ok())
    .flatten()
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|candidate| {
        candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                adapter
                    .extension_prefixes
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            })
    })
    .map(|path| AgentDetectionEvidence {
        kind: AgentEvidenceKind::Extension,
        path,
    })
    .collect()
}

fn executable_evidence(
    adapter: &AgentAdapterDescriptor,
    context: &AgentScanContext,
) -> Vec<AgentDetectionEvidence> {
    let mut evidence = Vec::new();
    for directory in &context.path_dirs {
        for executable in adapter.executables {
            let direct = directory.join(executable);
            if direct.is_file() {
                evidence.push(AgentDetectionEvidence {
                    kind: AgentEvidenceKind::Executable,
                    path: direct,
                });
            }
            if context.platform == HostPlatform::Windows {
                for extension in ["exe", "cmd", "bat"] {
                    let candidate = directory.join(format!("{executable}.{extension}"));
                    if candidate.is_file() {
                        evidence.push(AgentDetectionEvidence {
                            kind: AgentEvidenceKind::Executable,
                            path: candidate,
                        });
                    }
                }
            }
        }
    }
    evidence
}

fn path_evidence(
    specs: &[AgentPathSpec],
    context: &AgentScanContext,
    kind: AgentEvidenceKind,
) -> Vec<AgentDetectionEvidence> {
    specs
        .iter()
        .filter(|spec| spec.platform.is_none() || spec.platform == Some(context.platform))
        .flat_map(|spec| resolve_paths(*spec, context))
        .filter(|candidate| candidate.exists())
        .map(|path| AgentDetectionEvidence {
            kind: kind.clone(),
            path,
        })
        .collect()
}

fn resolve_paths(spec: AgentPathSpec, context: &AgentScanContext) -> Vec<PathBuf> {
    if spec.base == AgentPathBase::Applications {
        let mut paths = context
            .applications_dir
            .iter()
            .map(|base| base.join(spec.relative))
            .collect::<Vec<_>>();
        paths.push(context.home_dir.join("Applications").join(spec.relative));
        paths.sort();
        paths.dedup();
        return paths;
    }
    let base = match spec.base {
        AgentPathBase::Home => Some(&context.home_dir),
        AgentPathBase::RoamingAppData => context.roaming_app_data.as_ref(),
        AgentPathBase::LocalAppData => context.local_app_data.as_ref(),
        AgentPathBase::Applications => unreachable!("handled above"),
    };
    base.map(|base| vec![base.join(spec.relative)])
        .unwrap_or_default()
}

fn evidence_revision(agents: &[DetectedAgent]) -> String {
    let mut digest = Sha256::new();
    for agent in agents {
        digest.update(agent.id.as_bytes());
        digest.update([0]);
        for evidence in &agent.evidence {
            digest.update(format!("{:?}", evidence.kind).as_bytes());
            digest.update([0]);
            digest.update(evidence.path.to_string_lossy().as_bytes());
            digest.update([0xff]);
        }
    }
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_roots_include_all_safe_opencode_and_codex_metadata_paths() {
        let root = tempfile::tempdir().expect("temporary directory");
        let opencode_dir = root.path().join("opencode-config");
        let xdg_config = root.path().join("xdg-config");
        let xdg_data = root.path().join("xdg-data");
        let codex_home = root.path().join("codex-home");
        std::fs::create_dir_all(&codex_home).expect("Codex home");
        std::fs::write(codex_home.join("work.config.toml"), "model = \"test\"")
            .expect("Codex profile config");

        let evidence = environment_evidence(
            Some(root.path().join("explicit-opencode.json")),
            Some(opencode_dir.clone()),
            Some(codex_home.clone()),
            Some(xdg_config.clone()),
            Some(xdg_data.clone()),
        );
        let config_paths = evidence
            .iter()
            .filter(|item| item.kind == AgentEvidenceKind::Config)
            .map(|item| item.path.as_path())
            .collect::<HashSet<_>>();

        assert!(config_paths.contains(root.path().join("explicit-opencode.json").as_path()));
        assert!(config_paths.contains(opencode_dir.join("opencode.json").as_path()));
        assert!(config_paths.contains(opencode_dir.join("opencode.jsonc").as_path()));
        assert!(config_paths.contains(opencode_dir.join("config.json").as_path()));
        assert!(config_paths.contains(xdg_config.join("opencode/opencode.json").as_path()));
        assert!(config_paths.contains(codex_home.join("config.toml").as_path()));
        assert!(config_paths.contains(codex_home.join("work.config.toml").as_path()));
        assert!(!evidence.iter().any(|item| {
            item.path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains("auth"))
        }));
    }

    #[test]
    fn allowed_config_paths_are_registry_and_config_evidence_only() {
        let root = tempfile::tempdir().expect("temporary directory");
        let explicit = root.path().join("explicit-opencode.json");
        let unrelated = root.path().join("unrelated.json");
        let context = AgentScanContext {
            platform: HostPlatform::Macos,
            home_dir: root.path().to_path_buf(),
            path_dirs: vec![],
            applications_dir: Some(root.path().join("Applications")),
            roaming_app_data: None,
            local_app_data: None,
            additional_evidence: vec![
                AgentEvidenceOverride {
                    agent_id: "opencode".to_string(),
                    kind: AgentEvidenceKind::Config,
                    path: explicit.clone(),
                },
                AgentEvidenceOverride {
                    agent_id: "opencode".to_string(),
                    kind: AgentEvidenceKind::SessionRoot,
                    path: unrelated.clone(),
                },
            ],
            observed_at_ms: 1,
        };

        let paths =
            allowed_agent_config_paths("opencode", &context).expect("known OpenCode adapter");
        assert!(paths.contains(&root.path().join(".config/opencode/opencode.json")));
        assert!(paths.contains(&root.path().join(".config/opencode/opencode.jsonc")));
        assert!(paths.contains(&explicit));
        assert!(!paths.contains(&unrelated));
        assert!(allowed_agent_config_paths("unknown-agent", &context).is_err());
    }

    #[test]
    fn cline_cli_3_config_and_session_paths_are_registered_before_legacy_paths() {
        let root = tempfile::tempdir().expect("temporary directory");
        let context = AgentScanContext {
            platform: HostPlatform::Macos,
            home_dir: root.path().to_path_buf(),
            path_dirs: vec![],
            applications_dir: Some(root.path().join("Applications")),
            roaming_app_data: None,
            local_app_data: None,
            additional_evidence: vec![],
            observed_at_ms: 1,
        };

        let config_paths =
            allowed_agent_config_paths("cline", &context).expect("known Cline adapter");
        assert_eq!(
            config_paths.first(),
            Some(&root.path().join(".cline/data/settings/providers.json"))
        );
        assert!(config_paths.contains(&root.path().join(".cline/settings/providers.json")));

        let cline = agent_adapter("cline").expect("Cline registry entry");
        assert_eq!(
            cline.session_paths.first().map(|path| path.relative),
            Some(".cline/data/sessions")
        );
    }

    #[test]
    fn vscode_copilot_requires_extension_evidence_instead_of_shared_settings_only() {
        let root = tempfile::tempdir().expect("temporary directory");
        let mut context = AgentScanContext {
            platform: HostPlatform::Macos,
            home_dir: root.path().to_path_buf(),
            path_dirs: vec![],
            applications_dir: Some(root.path().join("Applications")),
            roaming_app_data: None,
            local_app_data: None,
            additional_evidence: vec![],
            observed_at_ms: 1,
        };
        let settings = context
            .home_dir
            .join("Library/Application Support/Code/User/settings.json");
        std::fs::create_dir_all(settings.parent().expect("settings parent"))
            .expect("settings directory");
        std::fs::write(&settings, "{}").expect("VS Code settings");

        let snapshot =
            scan_agent_adapters(known_agent_adapters(), &context).expect("settings-only scan");
        assert!(!snapshot
            .agents
            .iter()
            .any(|agent| agent.id == "vscode-copilot-chat"));

        let extension = context
            .home_dir
            .join(".vscode/extensions/github.copilot-chat-1.0.0");
        std::fs::create_dir_all(&extension).expect("Copilot Chat extension");
        context.observed_at_ms = 2;
        let snapshot =
            scan_agent_adapters(known_agent_adapters(), &context).expect("extension scan");
        let copilot = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == "vscode-copilot-chat")
            .expect("Copilot Chat detected");
        assert!(copilot
            .evidence
            .iter()
            .any(|item| item.kind == AgentEvidenceKind::Extension));
        assert!(copilot
            .evidence
            .iter()
            .any(|item| item.kind == AgentEvidenceKind::Config));
    }
}
