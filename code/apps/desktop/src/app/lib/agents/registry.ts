// d-token Agent 抽象模型：统一描述 Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode。
// 第三方工具只检测、读取配置、生成计划、受控写入，不自动安装。
// 关联计划：release-plan-2026-07/phase-02 §3 核心数据模型、§2 动作权限矩阵、§6 重点 Agent 适配计划。

export type AgentId =
  | "claude-code"
  | "codex"
  | "opencode"
  | "cursor"
  | "cline"
  | "commandcode";

export type ConfigMethod = "config-file" | "env-var" | "hook" | "local-proxy" | "manual-prompt";

export type AgentCapability = "one-click" | "confirm-required" | "prompt-only" | "manual-only" | "unsupported";

export type AgentDescriptor = {
  id: AgentId;
  displayName: string;
  executableHints: string[];
  configPathHints: string[];
  sessionPathHints: string[];
  supportedConfigMethods: ConfigMethod[];
  canAutoConfigure: boolean;
  requiresConfirmation: true;
  preferredProtocol: "openai-compatible" | "anthropic-compatible" | "local" | "unknown";
  capabilityLabel: string;
};

export type DetectedConfigFile = {
  path: string;
  exists: boolean;
  readable: boolean;
  hasCredentials: boolean;
};

export type RiskItem = { id: string; level: "low" | "medium" | "high"; message: string };

export type UpstreamDescriptor = {
  source: "official" | "cc-switch" | "custom" | "local-provider" | "unknown";
  protocol: "openai-compatible" | "anthropic-compatible" | "local" | "unknown";
  baseUrl?: string;
  port?: number;
  profileName?: string;
  providerName?: string;
  redactedCredentialRef?: string;
  confidence: number;
};

export type AgentDetectionResult = {
  agentId: AgentId;
  installed: boolean;
  executablePath?: string;
  version?: string;
  configFiles: DetectedConfigFile[];
  currentUpstream?: UpstreamDescriptor;
  configSource: "official" | "cc-switch" | "custom" | "unknown" | "not-configured";
  capability: AgentCapability;
  risks: RiskItem[];
  nextAction: string;
};

/** 重点验收 Agent 的统一 descriptor 注册表。CommandCode 资料待补，但仍纳入统一模型。 */
export const AGENT_REGISTRY: AgentDescriptor[] = [
  {
    id: "opencode", displayName: "OpenCode",
    executableHints: ["opencode", "opencode.exe", "opencode.cmd"],
    configPathHints: ["~/.config/opencode/config.json", "~/.opencode/config.json", "%APPDATA%/opencode/config.json"],
    sessionPathHints: ["~/.local/share/opencode/sessions", "~/.opencode/sessions", "%APPDATA%/opencode/sessions"],
    supportedConfigMethods: ["config-file", "env-var", "local-proxy"],
    canAutoConfigure: true, requiresConfirmation: true,
    preferredProtocol: "openai-compatible", capabilityLabel: "已可一键配置",
  },
  {
    id: "claude-code", displayName: "Claude Code",
    executableHints: ["claude", "claude.exe", "claude.cmd"],
    configPathHints: ["~/.claude/settings.json", "~/.claude.json", "%USERPROFILE%/.claude/settings.json"],
    sessionPathHints: ["~/.claude/projects", "%USERPROFILE%/.claude/projects"],
    supportedConfigMethods: ["config-file", "env-var", "hook"],
    canAutoConfigure: true, requiresConfirmation: true,
    preferredProtocol: "anthropic-compatible", capabilityLabel: "可检测，需用户确认配置",
  },
  {
    id: "codex", displayName: "Codex",
    executableHints: ["codex", "codex.exe", "codex.cmd"],
    configPathHints: ["~/.codex/config.toml", "~/.codex/config.json", "%USERPROFILE%/.codex/config.toml"],
    sessionPathHints: ["~/.codex/sessions", "%USERPROFILE%/.codex/sessions"],
    supportedConfigMethods: ["config-file", "env-var"],
    canAutoConfigure: true, requiresConfirmation: true,
    preferredProtocol: "openai-compatible", capabilityLabel: "可检测，需用户确认配置",
  },
  {
    id: "cursor", displayName: "Cursor",
    executableHints: ["cursor"],
    configPathHints: ["~/.cursor/settings.json", "%APPDATA%/Cursor/User/settings.json"],
    sessionPathHints: ["~/.cursor/workspaceStorage", "%APPDATA%/Cursor/User/workspaceStorage"],
    supportedConfigMethods: ["config-file", "manual-prompt"],
    canAutoConfigure: false, requiresConfirmation: true,
    preferredProtocol: "openai-compatible", capabilityLabel: "可生成配置建议，能写则受控写入",
  },
  {
    id: "cline", displayName: "Cline",
    executableHints: ["cline"],
    configPathHints: ["~/.vscode/extensions/saoudrizwan.cline", "%APPDATA%/Code/User/globalStorage/saoudrizwan.claude-dev"],
    sessionPathHints: ["%APPDATA%/Code/User/globalStorage/saoudrizwan.claude-dev"],
    supportedConfigMethods: ["config-file", "manual-prompt"],
    canAutoConfigure: false, requiresConfirmation: true,
    preferredProtocol: "openai-compatible", capabilityLabel: "可生成配置建议，能写则受控写入",
  },
  {
    id: "commandcode", displayName: "CommandCode",
    executableHints: ["commandcode", "command-code"],
    configPathHints: ["~/.commandcode/config.json"],
    sessionPathHints: ["~/.commandcode/sessions"],
    supportedConfigMethods: ["config-file", "manual-prompt"],
    canAutoConfigure: false, requiresConfirmation: true,
    preferredProtocol: "unknown", capabilityLabel: "仅支持高级手动配置（资料待补）",
  },
];

export function getAgentDescriptor(id: AgentId): AgentDescriptor | undefined {
  return AGENT_REGISTRY.find((agent) => agent.id === id);
}

/** 动作权限矩阵：第三方工具一律不自动安装；写入一律需确认。 */
export function canInstallAgent(_id: AgentId): boolean {
  return false;
}

export function canWriteAgentConfig(id: AgentId): "no" | "confirm-required" {
  const descriptor = getAgentDescriptor(id);
  return descriptor ? "confirm-required" : "no";
}

export function deriveCapability(installed: boolean, hasConfig: boolean, canAutoConfigure: boolean): AgentCapability {
  if (!installed) return "unsupported";
  if (canAutoConfigure && hasConfig) return "one-click";
  if (canAutoConfigure) return "confirm-required";
  if (hasConfig) return "prompt-only";
  return "manual-only";
}
