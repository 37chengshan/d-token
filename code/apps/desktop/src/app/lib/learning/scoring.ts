// d-token 学习价值评分引擎：按计划公式计算 LearningScore 并产出可解释排序原因。
// 关联计划：release-plan-2026-07/phase-03 §4 学习价值评分算法。
import type { LightIndex, LearningScore } from "./types";

export type ScoreInput = {
  lightIndex: LightIndex;
  nowMs: number;
  agentActivityHits: number;
  agentActivityFreshMs: number;
  userPreference: number; // -1..1
  dtokenHistoryHits: number;
  riskPenaltyRaw: number; // 0..1
};

export const SCORE_WEIGHTS = {
  recency: 0.22,
  agentActivity: 0.18,
  projectConfidence: 0.18,
  docGap: 0.14,
  complexity: 0.1,
  userPreference: 0.08,
  dtokenHistory: 0.06,
  riskPenalty: 0.1,
} as const;

function recencyScore(lastModifiedMs: number, nowMs: number): number {
  const dayMs = 24 * 60 * 60 * 1000;
  const days = Math.max(0, (nowMs - lastModifiedMs) / dayMs);
  if (days <= 1) return 1;
  if (days <= 7) return 0.85;
  if (days <= 30) return 0.6;
  if (days <= 90) return 0.35;
  return 0.1;
}

function activityScore(hits: number, freshMs: number): number {
  if (hits <= 0) return 0;
  const dayMs = 24 * 60 * 60 * 1000;
  const freshDays = freshMs / dayMs;
  const freshBoost = freshDays <= 1 ? 1 : freshDays <= 7 ? 0.8 : freshDays <= 30 ? 0.5 : 0.25;
  return Math.min(1, Math.log10(hits + 1) / 2) * freshBoost;
}

function projectConfidence(idx: LightIndex): number {
  let s = 0;
  if (idx.hasGit) s += 0.4;
  if (idx.projectMarkers.length > 0) s += 0.3;
  if (idx.languageFingerprints.length > 0) s += 0.2;
  if (idx.fileCountBucket !== "tiny" && idx.fileCountBucket !== "huge") s += 0.1;
  return Math.min(1, s);
}

function docGapScore(idx: LightIndex): number {
  // 缺少 Agent 项目说明文档时更高
  return idx.hasAgentDoc ? 0.2 : 0.9;
}

function complexityScore(idx: LightIndex): number {
  const langCount = idx.languageFingerprints.length;
  const sizeFactor =
    idx.fileCountBucket === "huge" ? 0.4 : idx.fileCountBucket === "large" ? 0.7 : 0.8;
  return Math.min(1, (langCount / 4) * 0.5 + sizeFactor * 0.5);
}

export function scoreCandidate(input: ScoreInput): LearningScore {
  const recency = recencyScore(input.lightIndex.lastModifiedMs, input.nowMs);
  const agentActivity = activityScore(input.agentActivityHits, input.agentActivityFreshMs);
  const projConfidence = projectConfidence(input.lightIndex);
  const docGap = docGapScore(input.lightIndex);
  const complexity = complexityScore(input.lightIndex);
  const userPreference = Math.max(-1, Math.min(1, input.userPreference));
  const dtokenHistory = Math.min(1, Math.log10(input.dtokenHistoryHits + 1) / 2);
  const riskPenalty = Math.max(0, Math.min(1, input.riskPenaltyRaw));

  const total =
    SCORE_WEIGHTS.recency * recency +
    SCORE_WEIGHTS.agentActivity * agentActivity +
    SCORE_WEIGHTS.projectConfidence * projConfidence +
    SCORE_WEIGHTS.docGap * docGap +
    SCORE_WEIGHTS.complexity * complexity +
    SCORE_WEIGHTS.userPreference * userPreference +
    SCORE_WEIGHTS.dtokenHistory * dtokenHistory -
    SCORE_WEIGHTS.riskPenalty * riskPenalty;

  return { total, recency, agentActivity, projectConfidence: projConfidence, docGap, complexity, userPreference, dtokenHistory, riskPenalty };
}

/** 产出可解释排序原因（中文），满足"排序解释可见"要求。 */
export function explainScore(score: LearningScore, input: ScoreInput): string[] {
  const out: string[] = [];
  const dayMs = 24 * 60 * 60 * 1000;
  const days = Math.round((input.nowMs - input.lightIndex.lastModifiedMs) / dayMs);
  if (days <= 1) out.push(`最近 1 天内有修改${input.agentActivityHits > 0 ? "且有 Agent 会话" : ""}。`);
  else if (days <= 7) out.push(`最近 ${days} 天内有修改。`);
  if (input.agentActivityHits > 0) out.push(`被 Agent 访问 ${input.agentActivityHits} 次。`);
  if (input.lightIndex.hasGit) out.push("包含 Git 仓库结构。");
  if (input.lightIndex.languageFingerprints.length > 0) {
    out.push(`包含 ${input.lightIndex.languageFingerprints.join(" + ")} 项目结构。`);
  }
  if (score.docGap > 0.5) out.push("缺少 Agent 项目说明文档。");
  if (input.dtokenHistoryHits > 0) out.push(`最近 d-token 在该项目出现 ${input.dtokenHistoryHits} 次上下文过长。`);
  if (input.lightIndex.fileCountBucket === "huge") out.push("目录较大，已采用轻量索引，建议手动确认深度扫描。");
  if (score.riskPenalty > 0.3) out.push("存在敏感文件或权限风险，已降低优先级。");
  return out;
}
