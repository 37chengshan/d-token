// d-token cc-switch 抽象：默认只读读取当前 profile/config/upstream，支持上游自动同步。
// 关联计划：release-plan-2026-07/phase-02 §4 cc-switch 同步状态机、§3 CcSwitchState。
import type { UpstreamDescriptor } from "../agents/registry";

export type CcSwitchConfigFile = {
  path: string;
  profile?: string;
  exists: boolean;
  readable: boolean;
  /** 是否含敏感字段（只标记，不读取原文）。 */
  hasCredentials: boolean;
};

export type CcSwitchSyncStatus =
  | "synced"
  | "out-of-sync"
  | "needs-confirmation"
  | "unsupported"
  | "unknown";

export type CcSwitchState = {
  detected: boolean;
  configRoot?: string;
  configFiles: CcSwitchConfigFile[];
  activeProfile?: string;
  activeUpstream?: UpstreamDescriptor;
  processStatus: "running" | "not-running" | "unknown";
  portStatus: "open" | "closed" | "conflict" | "unknown";
  lastSyncAt?: string;
  syncStatus: CcSwitchSyncStatus;
};

/** cc-switch 候选配置根目录（Windows/macOS）。 */
export const CCSWITCH_CONFIG_ROOTS: string[] = [
  "~/.cc-switch",
  "~/.config/cc-switch",
  "%APPDATA%/cc-switch",
  "%USERPROFILE%/.cc-switch",
];

export function emptyCcSwitchState(): CcSwitchState {
  return {
    detected: false,
    configFiles: [],
    processStatus: "unknown",
    portStatus: "unknown",
    syncStatus: "unknown",
  };
}
