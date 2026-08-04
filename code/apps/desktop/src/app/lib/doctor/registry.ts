// d-token Doctor 问题模型：IssueDefinition + RepairAction + DiagnosticContext。
// 关联计划：release-plan-2026-07/phase-04 §3 Doctor 问题模型、§4 Doctor 问题库初版。
import type { CcSwitchState } from "../ccswitch/types";
import { ISSUE_LIBRARY } from "./library";

export type IssueSeverity = "info" | "warning" | "critical";

export type IssueCategory =
  | "service" | "port" | "agent" | "cc-switch" | "upstream"
  | "learning" | "logs" | "rollback" | "security";

export type DiagnosticContext = {
  serviceRunning: boolean;
  portConflict: boolean;
  agentsDetected: number;
  agentsPointingToDtoken: number;
  ccSwitch: CcSwitchState;
  upstreamReachable: boolean;
  protocolMismatch: boolean;
  learningScanStuck: boolean;
  learningWriteFailed: boolean;
  logWriteFailed: boolean;
  rollbackAvailable: boolean;
  secretInLogs: boolean;
};

export type RepairAction = {
  id: string;
  label: string;
  riskLevel: "low" | "medium" | "high";
  requiresConfirmation: boolean;
  preflightChecks: string[];
  execute: string;
  verify: string;
  rollback?: string;
};

export type IssueDefinition = {
  id: string;
  title: string;
  severity: IssueSeverity;
  category: IssueCategory;
  detect: (ctx: DiagnosticContext) => boolean;
  impact: string;
  likelyCauses: string[];
  repairActions: RepairAction[];
  promptForAgent?: string;
  rollbackHint?: string;
};

export type IssueInstance = {
  id: string;
  title: string;
  severity: IssueSeverity;
  category: IssueCategory;
  impact: string;
  likelyCauses: string[];
  repairActions: RepairAction[];
  promptForAgent?: string;
  rollbackHint?: string;
};

export { ISSUE_LIBRARY };

/** 由诊断上下文产出问题实例列表。 */
export function detectIssues(ctx: DiagnosticContext): IssueInstance[] {
  return ISSUE_LIBRARY.filter((def) => def.detect(ctx)).map((def) => ({
    id: def.id, title: def.title, severity: def.severity, category: def.category,
    impact: def.impact, likelyCauses: def.likelyCauses, repairActions: def.repairActions,
    promptForAgent: def.promptForAgent, rollbackHint: def.rollbackHint,
  }));
}

/** 低风险可自动修复；高风险必须确认。 */
export function isAutoRepairable(action: RepairAction): boolean {
  return action.riskLevel === "low" && !action.requiresConfirmation;
}
