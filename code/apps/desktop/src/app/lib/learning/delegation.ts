// d-token 委托 Claude/Codex 流程：构造受控、脱敏的输入包，校验返回结构，失败降级提示词模式。
// 关联计划：release-plan-2026-07/phase-03 §9 委托 Claude/Codex 流程。
import type { DelegationPackage, LearningContext, LightIndex, SourceSummary } from "./types";

export type DelegationTarget = "claude" | "codex" | "prompt-only";

/** 构造脱敏委托输入包：只含 LightIndex 与 SourceSummary 摘要，不含完整源代码/会话/密钥。 */
export function buildDelegationPackage(params: {
  allowedScope: string[];
  excludedScope: string[];
  projectSummary: LightIndex;
  selectedFilesSummary: SourceSummary;
}): DelegationPackage {
  return {
    task: "generate-project-agent-context",
    allowedScope: params.allowedScope,
    excludedScope: params.excludedScope,
    projectSummary: params.projectSummary,
    selectedFilesSummary: params.selectedFilesSummary,
    outputSchema: "LearningContext",
    safetyRules: [
      "不要输出完整源代码、完整 prompt 或完整 response。",
      "不要读取或引用 .env、密钥、认证文件、cookie、token。",
      "只产出 LearningContext 结构：项目理解、命令、风险、Agent 指令。",
      "不得改动用户原有文档内容，只允许在 d-token 管理块内追加。",
    ],
  };
}

/** 校验委托返回是否为合法 LearningContext 结构。 */
export function validateDelegationResult(raw: unknown): { ok: boolean; context?: LearningContext; reason?: string } {
  if (!raw || typeof raw !== "object") return { ok: false, reason: "返回不是对象" };
  const r = raw as Record<string, unknown>;
  const required: (keyof LearningContext)[] = ["projectName", "projectPath", "summary", "techStack", "agentInstructions"];
  for (const key of required) {
    if (!(key in r)) return { ok: false, reason: `缺少字段 ${String(key)}` };
  }
  if (!Array.isArray(r.techStack)) return { ok: false, reason: "techStack 不是数组" };
  return { ok: true, context: r as unknown as LearningContext };
}

/** 委托失败降级策略：保留提示词模式，不改文件，保留生成内容与修复建议。 */
export function fallbackToPrompt(package_: DelegationPackage): string {
  const lines = [
    "# d-token 智能学习提示词（委托不可用时使用）",
    "",
    `任务：${package_.task}`,
    `允许范围：${package_.allowedScope.join(", ")}`,
    `排除范围：${package_.excludedScope.join(", ")}`,
    `项目摘要：${package_.projectSummary.folderName}（${package_.projectSummary.languageFingerprints.join("/") || "未知语言"}）`,
    `输出结构：${package_.outputSchema}`,
    "",
    "安全规则：",
    ...package_.safetyRules.map((s) => `- ${s}`),
  ];
  return lines.join("\n");
}
