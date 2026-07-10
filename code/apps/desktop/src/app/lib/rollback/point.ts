// d-token 回退点模型：统一配置回退、文档回退、Doctor 修复回退。
// 关联计划：release-plan-2026-07/phase-04 §6 回退点模型。
import type { BackupRef, VerificationResult } from "../install/transaction";

export type RollbackSource =
  | "install" | "advanced-install" | "smart-learning" | "doctor-repair" | "settings";

export type AffectedTarget = {
  kind: "agent-config" | "dtoken-active-config" | "provider-route" | "ccswitch-config"
    | "project-agent-doc" | "global-agent-doc" | "learned-context" | "doctor-repair-file";
  path: string;
};

export type RestorePlan = {
  steps: Array<{ action: string; target: AffectedTarget }>;
};

export type RestoreStatus = "available" | "restored" | "failed" | "partial" | "unavailable";

export type RollbackPoint = {
  id: string;
  createdAt: string;
  source: RollbackSource;
  affectedTargets: AffectedTarget[];
  humanSummary: string;
  diffSummary: string;
  backupRefs: BackupRef[];
  verificationBefore?: VerificationResult;
  verificationAfter?: VerificationResult;
  restorePlan: RestorePlan;
  restoreStatus: RestoreStatus;
};

/** 创建回退点。 */
export function createRollbackPoint(params: {
  source: RollbackSource;
  affectedTargets: AffectedTarget[];
  humanSummary: string;
  diffSummary: string;
  backupRefs: BackupRef[];
  restorePlan: RestorePlan;
  verificationBefore?: VerificationResult;
}): RollbackPoint {
  return {
    id: `rb-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    createdAt: new Date().toISOString(),
    source: params.source,
    affectedTargets: params.affectedTargets,
    humanSummary: params.humanSummary,
    diffSummary: params.diffSummary,
    backupRefs: params.backupRefs,
    verificationBefore: params.verificationBefore,
    restorePlan: params.restorePlan,
    restoreStatus: "available",
  };
}

/** 标记回退结果。 */
export function markRestored(point: RollbackPoint, status: RestoreStatus, verificationAfter?: VerificationResult): RollbackPoint {
  return { ...point, restoreStatus: status, verificationAfter };
}

/** 智能学习写入回退点：覆盖项目级/全局 Agent 文档与 learned-context。 */
export function createLearningRollbackPoint(params: {
  targets: AffectedTarget[];
  backupRefs: BackupRef[];
  diffSummary: string;
}): RollbackPoint {
  return createRollbackPoint({
    source: "smart-learning",
    affectedTargets: params.targets,
    humanSummary: "智能学习写入 d-token 管理块，原文档已备份。",
    diffSummary: params.diffSummary,
    backupRefs: params.backupRefs,
    restorePlan: { steps: params.targets.map((t) => ({ action: "restore-from-backup", target: t })) },
  });
}
