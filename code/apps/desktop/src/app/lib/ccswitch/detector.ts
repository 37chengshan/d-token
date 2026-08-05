// d-token cc-switch 检测器：只读探测配置目录、profile、active upstream、进程/端口状态。
// 关联计划：release-plan-2026-07/phase-02 §4 同步状态机、§7 UI 模块拆解。
// 注意：cc-switch 默认只读；不启动、不停止、不写第三方配置。
import { CCSWITCH_CONFIG_ROOTS, emptyCcSwitchState, type CcSwitchState } from "./types";
import { computeSyncStatus } from "./upstream";
import type { UpstreamDescriptor } from "../agents/registry";

/** 从一份 cc-switch 配置 JSON（已脱敏）解析 active profile 与 upstream。 */
export function parseCcSwitchConfig(
  raw: Record<string, unknown>,
  redactCredential = true,
): { activeProfile?: string; activeUpstream?: UpstreamDescriptor } {
  const activeProfile = typeof raw.activeProfile === "string" ? raw.activeProfile : undefined;
  const profiles = (raw.profiles ?? raw.configs ?? raw.providers) as Record<string, unknown> | undefined;
  let activeUpstream: UpstreamDescriptor | undefined;
  if (activeProfile && profiles && typeof profiles === "object") {
    const profile = (profiles as Record<string, Record<string, unknown>>)[activeProfile];
    if (profile) {
      activeUpstream = {
        source: "cc-switch",
        protocol: profile.protocol === "anthropic" ? "anthropic-compatible" : "openai-compatible",
        baseUrl: typeof profile.baseUrl === "string" ? profile.baseUrl : undefined,
        port: typeof profile.port === "number" ? profile.port : undefined,
        profileName: activeProfile,
        providerName: typeof profile.provider === "string" ? profile.provider : undefined,
        redactedCredentialRef: redactCredential && profile.apiKeyRef ? "[REDACTED]" : undefined,
        confidence: 0.8,
      };
    }
  }
  return { activeProfile, activeUpstream };
}

export type CcSwitchDetectInput = {
  /** 已发现的配置根目录（来自真实文件系统探测）。 */
  configRoot?: string;
  /** 已读取并脱敏的配置内容。 */
  config?: Record<string, unknown>;
  /** 进程是否在运行。 */
  processRunning?: boolean;
  /** 端口是否开放。 */
  portOpen?: boolean;
  /** 端口是否冲突。 */
  portConflict?: boolean;
  /** d-token 当前 active upstream，用于同步比较。 */
  dtokenUpstream?: UpstreamDescriptor;
};

/** 由探测输入计算 CcSwitchState（纯函数，不执行 IO）。 */
export function buildCcSwitchState(input: CcSwitchDetectInput): CcSwitchState {
  if (!input.configRoot && !input.config) {
    return emptyCcSwitchState();
  }
  const parsed = input.config ? parseCcSwitchConfig(input.config) : {};
  const state: CcSwitchState = {
    detected: true,
    configRoot: input.configRoot,
    configFiles: [],
    activeProfile: parsed.activeProfile,
    activeUpstream: parsed.activeUpstream,
    processStatus: input.processRunning ? "running" : input.processRunning === false ? "not-running" : "unknown",
    portStatus: input.portConflict ? "conflict" : input.portOpen ? "open" : input.portOpen === false ? "closed" : "unknown",
    syncStatus: "unknown",
  };
  state.syncStatus = computeSyncStatus(parsed.activeUpstream, input.dtokenUpstream);
  return state;
}

/** 候选配置根目录（供真实探测层使用）。 */
export function candidateConfigRoots(): string[] {
  return [...CCSWITCH_CONFIG_ROOTS];
}
