import { redactString } from "../security/redaction";
import { deriveTokenMetrics } from "./metrics";

export type BreakdownItem = {
  label: string;
  count: number;
  tokensSaved: number | null;
};

export type TrendPoint = {
  day: string;
  saved: number | null;
  requests: number;
};

export type LatencySummary = {
  sampleCount: number;
  averageMs: number;
  p50Ms: number;
  p95Ms: number;
};

export type AnalyticsSnapshot = {
  generatedAt: string;
  source: "dtoken-local-service" | "tauri-command" | "empty" | "preview";
  hasRequests: boolean;
  requestCount: number;
  estimatedInputTokens: number | null;
  estimatedOutputTokens: number | null;
  estimatedSavedTokens: number | null;
  estimatedSavingsRate: number | null;
  compressionCount: number | null;
  routeDecisionCount: number;
  errorCount: number;
  latency: LatencySummary | null;
  agentBreakdown: BreakdownItem[];
  upstreamBreakdown: BreakdownItem[];
  routeBreakdown: BreakdownItem[];
  strategyBreakdown: BreakdownItem[];
  trend7d: TrendPoint[];
  trend30d: TrendPoint[];
  warnings: string[];
};

export type RequestLogRow = {
  timestamp_ms?: number;
  agent?: string;
  provider_id?: string;
  route_id?: string;
  /** 兼容旧版已持久化请求日志；新运行时使用 provider_id。 */
  provider?: string;
  /** 兼容旧版已持久化请求日志；新运行时使用 route_id。 */
  route?: string;
  input_tokens_before?: number;
  input_tokens_after?: number;
  output_tokens?: number;
  tokens_saved?: number;
  transforms_applied?: string[];
  latency_ms?: number;
  error?: string | null;
};

type ProcessedLog = {
  timestamp: number | null;
  saved: number | null;
};

type MutableBreakdown = {
  label: string;
  count: number;
  tokensSaved: number;
  hasSavedEvidence: boolean;
};

/** 仅由 d-token 自有请求日志生成页面所需的一个分析快照。 */
export function buildAnalyticsSnapshot(
  logs: RequestLogRow[],
  opts: { source?: AnalyticsSnapshot["source"]; lang?: "zh" | "en" } = {},
): AnalyticsSnapshot {
  const source = opts.source ?? "dtoken-local-service";
  const zh = (opts.lang ?? "zh") === "zh";
  const records = logs.filter(isRequestLogRecord);
  let estimatedInputTokens = 0;
  let estimatedOutputTokens = 0;
  let estimatedSavedTokens = 0;
  let inputEvidenceCount = 0;
  let outputEvidenceCount = 0;
  let savedEvidenceCount = 0;
  let compressionCount = 0;
  let errorCount = 0;
  let rateInputTokens = 0;
  let rateSavedTokens = 0;
  let rateEvidenceCount = 0;
  let inconsistentSavingsCount = 0;
  let missingTimestampCount = 0;
  const latencySamples: number[] = [];
  const processed: ProcessedLog[] = [];
  const agentMap = new Map<string, MutableBreakdown>();
  const upstreamMap = new Map<string, MutableBreakdown>();
  const routeMap = new Map<string, MutableBreakdown>();
  const strategyMap = new Map<string, MutableBreakdown>();

  for (const log of records) {
    const before = nonNegativeNumber(log.input_tokens_before);
    const after = nonNegativeNumber(log.input_tokens_after);
    const output = nonNegativeNumber(log.output_tokens);
    const reportedSaved = nonNegativeNumber(log.tokens_saved);
    let saved: number | null = reportedSaved;

    if (before !== null) {
      estimatedInputTokens += before;
      inputEvidenceCount += 1;
    }
    if (output !== null) {
      estimatedOutputTokens += output;
      outputEvidenceCount += 1;
    }
    if (before !== null && after !== null) {
      const derived = deriveTokenMetrics(before, after);
      saved = derived.saved;
      if (reportedSaved !== null && reportedSaved !== saved) inconsistentSavingsCount += 1;
    }
    if (saved !== null) {
      estimatedSavedTokens += saved;
      savedEvidenceCount += 1;
      if (saved > 0) compressionCount += 1;
    }
    if (before !== null && saved !== null) {
      rateInputTokens += before;
      rateSavedTokens += saved;
      rateEvidenceCount += 1;
    }
    if (log.error) errorCount += 1;

    const timestamp = timestampNumber(log.timestamp_ms);
    if (timestamp === null) missingTimestampCount += 1;
    processed.push({ timestamp, saved });

    const latency = nonNegativeNumber(log.latency_ms);
    if (latency !== null) latencySamples.push(latency);

    bump(agentMap, safeLabel(log.agent), saved);
    bump(upstreamMap, safeLabel(log.provider_id) ?? safeLabel(log.provider), saved);
    bump(routeMap, safeLabel(log.route_id) ?? safeLabel(log.route), saved);

    const transforms = Array.isArray(log.transforms_applied)
      ? Array.from(new Set(log.transforms_applied.map(safeLabel).filter((item): item is string => item !== null)))
      : [];
    for (const transform of transforms) bump(strategyMap, transform, null);
  }

  const warnings: string[] = [];
  if (records.length > 0 && inputEvidenceCount < records.length) {
    warnings.push(zh
      ? "部分请求缺少输入 Token 字段，输入总量仅覆盖有记录的请求。"
      : "Some requests lack input-token fields; the input total covers recorded values only.");
  }
  if (records.length > 0 && savedEvidenceCount < records.length) {
    warnings.push(zh
      ? "部分请求缺少节省 Token 证据，节省量与节省率仅覆盖有记录的请求。"
      : "Some requests lack token-savings evidence; savings totals and rates cover recorded values only.");
  }
  if (records.length > 0 && outputEvidenceCount < records.length) {
    warnings.push(zh
      ? "部分请求缺少输出 Token 字段，输出总量仅覆盖有记录的请求。"
      : "Some requests lack output-token fields; the output total covers recorded values only.");
  }
  if (missingTimestampCount > 0) {
    warnings.push(zh
      ? `${missingTimestampCount} 条请求缺少有效时间戳，未计入趋势。`
      : `${missingTimestampCount} request(s) lack valid timestamps and are excluded from trends.`);
  }
  if (inconsistentSavingsCount > 0) {
    warnings.push(zh
      ? `${inconsistentSavingsCount} 条请求的节省量与输入前后值不一致，已按输入前后值重新计算。`
      : `${inconsistentSavingsCount} request(s) report savings inconsistent with before/after values; savings were recalculated.`);
  }

  return {
    generatedAt: new Date().toISOString(),
    source,
    hasRequests: records.length > 0,
    requestCount: records.length,
    estimatedInputTokens: inputEvidenceCount > 0 ? estimatedInputTokens : null,
    estimatedOutputTokens: outputEvidenceCount > 0 ? estimatedOutputTokens : null,
    estimatedSavedTokens: savedEvidenceCount > 0 ? estimatedSavedTokens : null,
    estimatedSavingsRate: rateEvidenceCount > 0 && rateInputTokens > 0
      ? rateSavedTokens / rateInputTokens
      : null,
    compressionCount: savedEvidenceCount > 0 ? compressionCount : null,
    routeDecisionCount: records.filter((log) => safeLabel(log.route_id) ?? safeLabel(log.route)).length,
    errorCount,
    latency: summarizeLatency(latencySamples),
    agentBreakdown: toArray(agentMap),
    upstreamBreakdown: toArray(upstreamMap),
    routeBreakdown: toArray(routeMap),
    strategyBreakdown: toArray(strategyMap),
    trend7d: buildTrend(processed, 7),
    trend30d: buildTrend(processed, 30),
    warnings,
  };
}

function isRequestLogRecord(log: RequestLogRow): boolean {
  return Boolean(log) && typeof log === "object" && !Array.isArray(log);
}

function nonNegativeNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

function timestampNumber(value: unknown): number | null {
  const timestamp = nonNegativeNumber(value);
  if (timestamp === null) return null;
  return Number.isFinite(new Date(timestamp).getTime()) ? timestamp : null;
}

function safeLabel(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? redactString(trimmed) : null;
}

function bump(map: Map<string, MutableBreakdown>, label: string | null, saved: number | null) {
  if (!label) return;
  const current = map.get(label) ?? {
    label,
    count: 0,
    tokensSaved: 0,
    hasSavedEvidence: false,
  };
  current.count += 1;
  if (saved !== null) {
    current.tokensSaved += saved;
    current.hasSavedEvidence = true;
  }
  map.set(label, current);
}

function toArray(map: Map<string, MutableBreakdown>): BreakdownItem[] {
  return Array.from(map.values())
    .map((item) => ({
      label: item.label,
      count: item.count,
      tokensSaved: item.hasSavedEvidence ? item.tokensSaved : null,
    }))
    .sort((left, right) => right.count - left.count || left.label.localeCompare(right.label));
}

function summarizeLatency(samples: number[]): LatencySummary | null {
  if (samples.length === 0) return null;
  const sorted = [...samples].sort((left, right) => left - right);
  return {
    sampleCount: sorted.length,
    averageMs: roundOne(sorted.reduce((total, value) => total + value, 0) / sorted.length),
    p50Ms: roundOne(quantile(sorted, 0.5)),
    p95Ms: roundOne(quantile(sorted, 0.95)),
  };
}

function quantile(sorted: number[], percentile: number): number {
  if (sorted.length === 1) return sorted[0];
  const position = (sorted.length - 1) * percentile;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  const weight = position - lower;
  return sorted[lower] * (1 - weight) + sorted[upper] * weight;
}

function roundOne(value: number): number {
  return Number(value.toFixed(1));
}

function buildTrend(logs: ProcessedLog[], days: number): TrendPoint[] {
  const timestamped = logs.filter(
    (log): log is ProcessedLog & { timestamp: number } => log.timestamp !== null,
  );
  if (timestamped.length === 0) return [];
  const latest = Math.max(...timestamped.map((log) => log.timestamp));
  const cutoff = latest - days * 24 * 60 * 60 * 1000;
  const byDay = new Map<string, { day: string; requests: number; saved: number; hasSavedEvidence: boolean }>();

  for (const log of timestamped) {
    if (log.timestamp < cutoff) continue;
    const day = new Date(log.timestamp).toISOString().slice(0, 10);
    const current = byDay.get(day) ?? { day, requests: 0, saved: 0, hasSavedEvidence: false };
    current.requests += 1;
    if (log.saved !== null) {
      current.saved += log.saved;
      current.hasSavedEvidence = true;
    }
    byDay.set(day, current);
  }

  return Array.from(byDay.values())
    .sort((left, right) => left.day.localeCompare(right.day))
    .map((point) => ({
      day: point.day,
      requests: point.requests,
      saved: point.hasSavedEvidence ? point.saved : null,
    }));
}
