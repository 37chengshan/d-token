// d-token 分析数据模型 AnalyticsSnapshot：所有统计来自 d-token 自有记录，估算节省明确标注"估算"。
// 关联计划：release-plan-2026-07/phase-04 §2 分析数据模型、估算节省计算原则。
import { redactString } from "../security/redaction";
import { deriveTokenMetrics } from "./metrics";

export type BreakdownItem = { label: string; count: number; tokensSaved: number };

export type TrendPoint = { day: string; saved: number; requests: number };

export type LearningCoverage = {
  scannedFolders: number;
  writtenDocs: number;
  coverageRate: number;
};

export type AnalyticsSnapshot = {
  generatedAt: string;
  source: "dtoken-local-service" | "tauri-command" | "empty" | "preview";
  requestCount: number;
  estimatedInputTokens: number;
  estimatedOutputTokens: number;
  estimatedSavedTokens: number;
  estimatedSavingsRate: number;
  compressionCount: number;
  routeDecisionCount: number;
  errorCount: number;
  agentBreakdown: BreakdownItem[];
  upstreamBreakdown: BreakdownItem[];
  ccSwitchBreakdown: BreakdownItem[];
  projectBreakdown: BreakdownItem[];
  trend7d: TrendPoint[];
  trend30d: TrendPoint[];
  learningCoverage: LearningCoverage;
  warnings: string[];
};

export type RequestLogRow = {
  timestamp_ms?: number;
  agent?: string;
  provider?: string;
  route?: string;
  input_tokens_before?: number;
  input_tokens_after?: number;
  output_tokens?: number;
  tokens_saved?: number;
  transforms_applied?: string[];
  latency_ms?: number;
  error?: string | null;
};

/** 由 d-token 自有请求日志聚合 AnalyticsSnapshot。 */
export function buildAnalyticsSnapshot(
  logs: RequestLogRow[],
  opts: { source?: AnalyticsSnapshot["source"]; learning?: LearningCoverage } = {},
): AnalyticsSnapshot {
  const source = opts.source ?? "dtoken-local-service";
  const requestCount = logs.length;
  let estimatedInputTokens = 0;
  let estimatedOutputTokens = 0;
  let estimatedSavedTokens = 0;
  let compressionCount = 0;
  let errorCount = 0;
  let inconsistentSavingsCount = 0;
  const withTokenField = logs.filter((l) => typeof l.input_tokens_before === "number");
  const agentMap = new Map<string, BreakdownItem>();
  const upstreamMap = new Map<string, BreakdownItem>();
  const projectMap = new Map<string, BreakdownItem>();
  for (const log of logs) {
    const before = Number(log.input_tokens_before ?? 0);
    const after = Number(log.input_tokens_after ?? before);
    const out = Number(log.output_tokens ?? 0);
    const derived = deriveTokenMetrics(before, after);
    const reportedSaved = typeof log.tokens_saved === "number" ? Number(log.tokens_saved) : derived.saved;
    const saved = derived.saved;
    if (reportedSaved !== saved) inconsistentSavingsCount += 1;
    estimatedInputTokens += before;
    estimatedOutputTokens += out;
    estimatedSavedTokens += saved;
    if (saved > 0) compressionCount += 1;
    if (log.error) errorCount += 1;
    if (log.agent) bump(agentMap, redactString(String(log.agent)), saved);
    if (log.provider) bump(upstreamMap, redactString(String(log.provider)), saved);
    if (log.route) bump(projectMap, redactString(String(log.route)), saved);
  }
  const estimatedSavingsRate = estimatedInputTokens > 0 ? estimatedSavedTokens / estimatedInputTokens : 0;
  const warnings: string[] = [];
  if (withTokenField.length < requestCount) {
    warnings.push("部分请求缺少 token 字段，只参与请求数，不参与节省率。");
  }
  if (requestCount < 7) {
    warnings.push("数据不足，暂不计算趋势。");
  }
  if (inconsistentSavingsCount > 0) {
    warnings.push(`${inconsistentSavingsCount} 条请求的节省量与输入前后值不一致，已按输入前后值重新计算。`);
  }
  return {
    generatedAt: new Date().toISOString(),
    source,
    requestCount,
    estimatedInputTokens,
    estimatedOutputTokens,
    estimatedSavedTokens,
    estimatedSavingsRate,
    compressionCount,
    routeDecisionCount: logs.length,
    errorCount,
    agentBreakdown: toArray(agentMap),
    upstreamBreakdown: toArray(upstreamMap),
    ccSwitchBreakdown: [],
    projectBreakdown: toArray(projectMap),
    trend7d: buildTrend(logs, 7),
    trend30d: buildTrend(logs, 30),
    learningCoverage: opts.learning ?? { scannedFolders: 0, writtenDocs: 0, coverageRate: 0 },
    warnings,
  };
}

function bump(map: Map<string, BreakdownItem>, label: string, saved: number) {
  const cur = map.get(label) ?? { label, count: 0, tokensSaved: 0 };
  cur.count += 1;
  cur.tokensSaved += saved;
  map.set(label, cur);
}

function toArray(map: Map<string, BreakdownItem>): BreakdownItem[] {
  return Array.from(map.values()).sort((a, b) => b.tokensSaved - a.tokensSaved);
}

function buildTrend(logs: RequestLogRow[], days: number): TrendPoint[] {
  const byDay = new Map<string, TrendPoint>();
  const now = Date.now();
  const dayMs = 24 * 60 * 60 * 1000;
  for (let d = days - 1; d >= 0; d--) {
    const date = new Date(now - d * dayMs).toISOString().slice(0, 10);
    byDay.set(date, { day: date.slice(5), saved: 0, requests: 0 });
  }
  for (const log of logs) {
    const ms = Number(log.timestamp_ms ?? now);
    const date = new Date(ms).toISOString().slice(0, 10);
    const point = byDay.get(date);
    if (point) {
      point.saved += deriveTokenMetrics(
        Number(log.input_tokens_before ?? 0),
        Number(log.input_tokens_after ?? log.input_tokens_before ?? 0),
      ).saved;
      point.requests += 1;
    }
  }
  return Array.from(byDay.values());
}
