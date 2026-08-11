import type { Lang } from "../i18n"
import { translateCopy } from "../i18n";
import type { EvidenceState } from "../state/evidence";
import {
  estimateSavingsCost,
  type ModelCurrency,
  type ModelPriceCache,
  type PriceCacheFreshness,
} from "../analytics/pricing";

/**
 * Overview 节省区间派生：累计 / 当周 / 当月 / 当天。
 *
 * 证据规则（与 overview/derivations.ts 一致）：
 * - 只有 verified / stale（最近验证快照）才计入真实数据；preview/loading/unavailable/partial
 *   一律返回 unavailable，绝不把浏览器预览或等待状态当作真实节省。
 * - 物理节省 = max(0, input_tokens_before - input_tokens_after)，只使用请求日志中
 *   同时具备压缩前后值的记录；缺少字段的记录计入 partial 而不是补零。
 * - 累计窗口优先使用 durable 聚合值（stats.tokens.input / input_after），与
 *   overviewFromPayload 一致，避免被近期截断窗口低估；聚合缺失时才回退到逐条求和。
 * - 时间窗口边界按用户本地时区计算（当天 00:00、当周周一 00:00、当月 1 日 00:00），
 *   跨月/跨年由 Date 日历运算处理，不使用 UTC 切片。
 * - 真实 zero 与 unavailable 严格区分：verified 空集合是真实 zero；没有时间戳或
 *   没有请求集合才是 unavailable。
 * - 费用差额：当前 runtime 合同没有可信价格来源，一律显示 unavailable 并给出说明，
 *   禁止用自造价格或 Token 差额直接冒充账单节省。
 */

export type SavingsWindowKey = "total" | "week" | "month" | "day";

export type SavingsWindowState = "verified" | "partial" | "unavailable";

export type SavingsWindowSummary = {
  key: SavingsWindowKey;
  label: string;
  /** 窗口内物理节省 Token；null 表示该窗口没有可用的压缩前后值证据。 */
  tokensSaved: number | null;
  /** 窗口内请求数；null 表示没有请求计数证据。 */
  requestCount: number | null;
  state: SavingsWindowState;
  note: string;
};

export type SavingsCostSummary =
  | { available: false; detail: string }
  | {
      available: true;
      /** 估算币种（仅当所有计入请求同币种时才可用）。 */
      currency: ModelCurrency;
      /** 成本差额估算（节省 Token × 输入价格）。 */
      estimatedCost: number;
      /** 价格缓存来源，例如 "user-settings"。 */
      source: string;
      freshness: PriceCacheFreshness;
      /** 价格缓存检查时间；未提供时为 null。 */
      checkedAt: string | null;
      detail: string;
    };

export type SavingsDerivation = {
  /** 是否存在任何可展示的节省证据（任一窗口可用）。 */
  available: boolean;
  source: string;
  /** 固定顺序：[累计, 当周, 当月, 当天]。 */
  windows: SavingsWindowSummary[];
  cost: SavingsCostSummary;
};

type ParsedLog = { timestamp: number | null; before: number | null; after: number | null; model: string | null };

const WINDOW_KEYS: readonly SavingsWindowKey[] = ["total", "week", "month", "day"];

export function deriveSavingsWindows(
  payload: Record<string, any> | null,
  evidence: EvidenceState,
  lang: Lang = "zh",
  now: Date = new Date(),
  pricing: ModelPriceCache | null = null,
): SavingsDerivation {
  const lastVerified = evidence === "verified" || evidence === "stale";
  const hasCollection = Array.isArray(payload?.request_logs);
  const requestLogs = hasCollection ? (payload as Record<string, any>).request_logs : [];
  const stats = payload && typeof payload.stats === "object" && !Array.isArray(payload.stats) ? payload.stats : {};
  const tokens = stats.tokens && typeof stats.tokens === "object" && !Array.isArray(stats.tokens) ? stats.tokens : {};
  const aggregateBefore = numberEvidence(tokens.input);
  const aggregateAfter = numberEvidence(tokens.input_after);
  const aggregateTotal = firstNumberEvidence(stats.requests?.total, stats.requests?.count);

  const boundaries = localWindowBoundaries(now);

  if (!lastVerified) {
    const source =
      evidence === "preview"
        ? (translateCopy("ov.browserPreviewNoLiveData", lang))
        : evidence === "loading"
          ? (translateCopy("an.connectingToTheLocalDTokenService", lang))
          : evidence === "partial"
            ? (translateCopy("ov.localDataIsIncompleteCurrentStateUnverified", lang))
            : (translateCopy("an.localDTokenServiceUnavailable", lang));
    return {
      available: false,
      source,
      windows: WINDOW_KEYS.map((key) => unavailableWindow(key, lang, evidence)),
      cost: missingCost(lang),
    };
  }

  const parsedLogs: ParsedLog[] = requestLogs.map((log: Record<string, any>) => {
    const timestamp = timestampEvidence(log?.timestamp_ms);
    const before = numberEvidence(log.input_tokens_before);
    const after = numberEvidence(log.input_tokens_after);
    const model = typeof log.model === "string" && log.model.trim() ? log.model : null;
    return { timestamp, before, after, model };
  });
  const timestampedLogs = parsedLogs.filter((log) => log.timestamp !== null);
  const totalPairs = parsedLogs.filter((log) => log.before !== null && log.after !== null);

  const windows: SavingsWindowSummary[] = [
    buildTotalWindow(aggregateBefore, aggregateAfter, aggregateTotal, parsedLogs, totalPairs, requestLogs.length, lang),
    buildBoundedWindow("week", boundaries.weekStart, boundaries.now, hasCollection, parsedLogs, lang),
    buildBoundedWindow("month", boundaries.monthStart, boundaries.now, hasCollection, parsedLogs, lang),
    buildBoundedWindow("day", boundaries.dayStart, boundaries.now, hasCollection, parsedLogs, lang),
  ];

  const anyEvidence = windows.some((window) => window.state !== "unavailable");
  const source =
    evidence === "stale"
      ? (translateCopy("ov.lastVerifiedLocalDTokenRequestRecords", lang))
      : requestLogs.length === 0
        ? (translateCopy("an.localServiceOnlineNoRequestRecords", lang))
        : timestampedLogs.length === 0
          ? (translateCopy("ov.localServiceOnlineRequestsLackTimestamps", lang))
          : (translateCopy("ov.localDTokenRequestRecords", lang));

  return { available: anyEvidence, source, windows, cost: deriveCost(parsedLogs, pricing, lang) };
}

/**
 * 依据价格缓存估算成本差额。没有价格缓存时保持 unavailable（禁止自造价格）；
 * 部分模型无价格或币种混合时同样不可用，绝不用 0 或部分值冒充账单节省。
 */
function deriveCost(
  parsedLogs: ParsedLog[],
  pricing: ModelPriceCache | null,
  lang: Lang,
): SavingsCostSummary {
  if (!pricing) return missingCost(lang);
  const estimate = estimateSavingsCost(parsedLogs, pricing);
  if (!estimate.available) {
    const detail =
      estimate.reason === "unknown-models"
        ? translateCopy("ov.someModelsLackExplicitPricesBillingSavingsCannotBeEstimatedReliably", lang, { v1: estimate.unknownModels.join("、"), v2: estimate.unknownModels.join(", ") })
        : estimate.reason === "mixed-currency"
          ? translateCopy("ov.requestsSpanMultipleCurrenciesBillingSavingsCannotBeMerged", lang)
          : missingCost(lang).detail;
    return { available: false, detail };
  }
  return {
    available: true,
    currency: estimate.currency,
    estimatedCost: estimate.estimatedCost,
    source: pricing.source,
    freshness: pricing.freshness,
    checkedAt: pricing.checkedAt,
    detail:
      translateCopy("ov.estimatedFromTheLocalPricingTableTokenDeltasAreNotBillingSavings", lang, { v1: estimate.currency }),
  };
}

function buildTotalWindow(
  aggregateBefore: number | null,
  aggregateAfter: number | null,
  aggregateTotal: number | null,
  parsedLogs: Array<{ timestamp: number | null; before: number | null; after: number | null }>,
  totalPairs: Array<{ timestamp: number | null; before: number | null; after: number | null }>,
  rawLogCount: number,
  lang: Lang,
): SavingsWindowSummary {
  const label = translateCopy("ov.total", lang);
  // 优先使用 durable 聚合值，避免被近期截断窗口低估。
  if (aggregateBefore !== null && aggregateAfter !== null) {
    return {
      key: "total",
      label,
      tokensSaved: Math.max(0, aggregateBefore - aggregateAfter),
      requestCount: aggregateTotal,
      state: "verified",
      note: translateCopy("ov.fromTheRuntimeSDurableAggregateTokenRecords", lang),
    };
  }
  if (totalPairs.length > 0) {
    const saved = totalPairs.reduce((sum, pair) => sum + Math.max(0, pair.before! - pair.after!), 0);
    const partial = parsedLogs.some((log) => log.before === null || log.after === null);
    return {
      key: "total",
      label,
      tokensSaved: saved,
      requestCount: aggregateTotal ?? rawLogCount,
      state: partial ? "partial" : "verified",
      note: partial
        ? translateCopy("ov.someRequestsLackBeforeAfterValuesSavingsCoverAvailableRequestsOnly", lang)
        : translateCopy("ov.basedOnBeforeAfterValuesBillingImpactPending", lang),
    };
  }
  // verified 空集合是真实 zero，不是 unavailable。
  if (rawLogCount === 0) {
    return {
      key: "total",
      label,
      tokensSaved: 0,
      requestCount: aggregateTotal ?? 0,
      state: "verified",
      note: translateCopy("ov.noRequestRecords", lang),
    };
  }
  // 有请求但完全没有压缩前后值 → 部分证据，不冒充 verified 0。
  return {
    key: "total",
    label,
    tokensSaved: null,
    requestCount: aggregateTotal ?? rawLogCount,
    state: "partial",
    note: translateCopy("ov.requestsExistButLackBeforeAfterValues", lang),
  };
}

function buildBoundedWindow(
  key: "week" | "month" | "day",
  start: number,
  now: number,
  hasCollection: boolean,
  parsedLogs: Array<{ timestamp: number | null; before: number | null; after: number | null }>,
  lang: Lang,
): SavingsWindowSummary {
  const label = key === "week" ? (translateCopy("ov.thisWeek", lang)) : key === "month" ? (translateCopy("ov.thisMonth", lang)) : (translateCopy("ov.today", lang));
  const inWindow = parsedLogs.filter((log) => log.timestamp !== null && log.timestamp >= start && log.timestamp <= now);
  if (inWindow.length === 0) {
    // verified 空集合 → 真实 zero；有时间戳但不在窗口内 → 真实 zero；
    // 完全没有时间戳或没有请求集合 → unavailable。
    if (!hasCollection) {
      return {
        key,
        label,
        tokensSaved: null,
        requestCount: null,
        state: "unavailable",
        note: translateCopy("ov.noLocalRequestRecordsYet", lang),
      };
    }
    const hasAnyTimestamp = parsedLogs.some((log) => log.timestamp !== null);
    if (!hasAnyTimestamp && parsedLogs.length > 0) {
      return {
        key,
        label,
        tokensSaved: null,
        requestCount: null,
        state: "unavailable",
        note: translateCopy("ov.requestsLackTimestampsCannotAttributeToThisWindow", lang),
      };
    }
    return {
      key,
      label,
      tokensSaved: 0,
      requestCount: 0,
      state: "verified",
      note: translateCopy("ov.noRequestsInThisWindow", lang),
    };
  }
  const pairs = inWindow.filter((log) => log.before !== null && log.after !== null);
  if (pairs.length === 0) {
    return {
      key,
      label,
      tokensSaved: null,
      requestCount: inWindow.length,
      state: "partial",
      note: translateCopy("ov.requestsExistButLackBeforeAfterValues2", lang),
    };
  }
  const saved = pairs.reduce((sum, pair) => sum + Math.max(0, pair.before! - pair.after!), 0);
  const partial = inWindow.length > pairs.length;
  return {
    key,
    label,
    tokensSaved: saved,
    requestCount: inWindow.length,
    state: partial ? "partial" : "verified",
    note: partial
      ? translateCopy("ov.someRequestsLackBeforeAfterValuesSavingsCoverAvailableRequestsOnly", lang)
      : translateCopy("ov.basedOnBeforeAfterValuesBillingImpactPending2", lang),
  };
}

function unavailableWindow(key: SavingsWindowKey, lang: Lang, evidence: EvidenceState): SavingsWindowSummary {
  const label = key === "total" ? (translateCopy("ov.total", lang)) : key === "week" ? (translateCopy("ov.thisWeek", lang)) : key === "month" ? (translateCopy("ov.thisMonth", lang)) : (translateCopy("ov.today", lang));
  const note =
    evidence === "preview"
      ? (translateCopy("ov.browserPreviewNoLiveData", lang))
      : evidence === "loading"
        ? (translateCopy("an.connectingToTheLocalDTokenService", lang))
        : evidence === "partial"
          ? (translateCopy("ov.localDataIsIncompleteCurrentStateUnverified", lang))
          : (translateCopy("an.localDTokenServiceUnavailable", lang));
  return { key, label, tokensSaved: null, requestCount: null, state: "unavailable", note };
}

function missingCost(lang: Lang): SavingsCostSummary {
  return {
    available: false,
    detail: translateCopy("ov.noTrustedPricingSourceBillingSavingsAreNotEstimatedTokenDeltasAreNotBillingSavings", lang),
  };
}

/** 本地时区窗口边界（毫秒时间戳）。周一为一周起点，与产品日历习惯一致。 */
function localWindowBoundaries(now: Date): { now: number; dayStart: number; weekStart: number; monthStart: number } {
  const dayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const weekStart = (() => {
    const d = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const daysSinceMonday = (d.getDay() + 6) % 7;
    d.setDate(d.getDate() - daysSinceMonday);
    return d.getTime();
  })();
  const monthStart = new Date(now.getFullYear(), now.getMonth(), 1).getTime();
  return { now: now.getTime(), dayStart, weekStart, monthStart };
}

/** 与 derivations.ts 相同：数字或纯数字字符串；秒级时间戳（< 10^10）转换为毫秒。 */
function timestampEvidence(value: unknown): number | null {
  const parsed = numberEvidence(value);
  if (parsed === null || !Number.isFinite(new Date(parsed).getTime())) return null;
  return parsed < 10_000_000_000 ? parsed * 1000 : parsed;
}

function numberEvidence(value: unknown): number | null {
  if (typeof value !== "number" && typeof value !== "string") return null;
  if (typeof value === "string" && value.trim().length === 0) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function firstNumberEvidence(...values: unknown[]): number | null {
  for (const value of values) {
    const parsed = numberEvidence(value);
    if (parsed !== null) return parsed;
  }
  return null;
}
