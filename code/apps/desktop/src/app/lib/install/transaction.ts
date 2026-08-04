// d-token 安装与配置事务状态机：scan -> plan -> diff -> backup -> apply -> verify -> ledger -> rollback-point。
// 关联计划：release-plan-2026-07/phase-02 §5 安装与配置事务算法、强制规则。
import { redactObject, type RedactionLevel } from "../security/redaction";

export type TxStage =
  | "idle"
  | "scan"
  | "plan"
  | "diff"
  | "backup"
  | "confirm"
  | "apply"
  | "verify"
  | "ledger"
  | "rollback-point"
  | "done"
  | "failed"
  | "rolled-back";

export type PlannedFileChange = {
  path: string;
  action: "create" | "modify" | "delete";
  /** 脱敏后的 diff 摘要，不含密钥原文。 */
  diffSummary: string;
};

export type BackupRef = { path: string; createdAt: string; bytes: number };

export type VerificationStep = { id: string; label: string; ok: boolean; detail?: string };

export type VerificationResult = { ok: boolean; steps: VerificationStep[]; summary: string };

export type InstallTransactionState = {
  txId: string;
  stage: TxStage;
  purpose: "connect-agent-to-dtoken" | "sync-upstream" | "repair-config";
  filesToModify: PlannedFileChange[];
  backupRefs: BackupRef[];
  verification?: VerificationResult;
  rollbackPointId?: string;
  error?: string;
  startedAt: string;
};

/** 强制规则：没有 diff 不允许 apply；没有 backup 不允许 apply；
 *  没有 verification 不允许显示成功；没有 rollback point 不允许结束事务。 */
export function canApply(state: InstallTransactionState): { ok: boolean; reason?: string } {
  if (state.stage !== "confirm") return { ok: false, reason: "事务未进入确认阶段" };
  if (state.filesToModify.length === 0) return { ok: false, reason: "没有变更计划（diff 为空），不允许应用" };
  if (state.backupRefs.length === 0) return { ok: false, reason: "没有备份，不允许应用" };
  return { ok: true };
}

export function canComplete(state: InstallTransactionState): { ok: boolean; reason?: string } {
  if (state.stage !== "verify" && state.stage !== "ledger" && state.stage !== "rollback-point") {
    return { ok: false, reason: "事务未到达验证阶段" };
  }
  if (!state.verification) return { ok: false, reason: "没有验证结果，不允许显示成功" };
  if (!state.verification.ok) return { ok: false, reason: "验证未通过" };
  if (!state.rollbackPointId) return { ok: false, reason: "没有回退点，不允许结束事务" };
  return { ok: true };
}

/** 推进事务到下一阶段（纯状态转换，副作用由调用层执行）。 */
export function advance(state: InstallTransactionState, next: Partial<InstallTransactionState>): InstallTransactionState {
  return { ...state, ...next };
}

/** 构造人话摘要（脱敏），用于确认弹窗。密钥/token/cookie/认证头不得进入摘要。 */
export function buildHumanSummary(
  state: InstallTransactionState,
  level: RedactionLevel = "high",
): string {
  const safe = redactObject({ files: state.filesToModify, purpose: state.purpose }, level);
  const lines: string[] = [];
  lines.push(`本次变更目的：${humanPurpose(state.purpose)}`);
  lines.push(`将修改 ${state.filesToModify.length} 个文件，已准备 ${state.backupRefs.length} 个备份。`);
  if (state.verification) {
    lines.push(state.verification.ok ? "应用后将立即验证连接。" : "验证未通过，请查看详情。");
  }
  lines.push("如果结果不符合预期，你可以在“变更与回退”中恢复。");
  void safe;
  return lines.join(" ");
}

function humanPurpose(p: InstallTransactionState["purpose"]): string {
  switch (p) {
    case "connect-agent-to-dtoken": return "把 Agent 连接到 d-token 本地服务";
    case "sync-upstream": return "同步 cc-switch 上游到 d-token";
    case "repair-config": return "修复配置问题";
  }
}

export function createTransaction(purpose: InstallTransactionState["purpose"]): InstallTransactionState {
  return {
    txId: `tx-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    stage: "idle",
    purpose,
    filesToModify: [],
    backupRefs: [],
    startedAt: new Date().toISOString(),
  };
}
