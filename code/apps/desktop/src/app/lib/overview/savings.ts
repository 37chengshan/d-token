import type { Lang } from "../i18n";
import type { EvidenceState } from "../state/evidence";

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

export type SavingsCostSummary = {
  available: false;
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

type ParsedLog = { timestamp: number | null; before: number | null; after: number | null };

const WINDOW_KEYS: readonly SavingsWindowKey[] = ["total", "week", "month", "day"];

export function deriveSavingsWindows(
  payload: Record<string, any> | null,
  evidence: EvidenceState,
  lang: Lang = "zh",
  now: Date = new Date(),
): SavingsDerivation {
  const zh = lang === "zh";
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
        ? (zh ? "浏览器预览 · 未连接实时数据" : "Browser preview · no live data")
        : evidence === "loading"
          ? (zh ? "正在连接 d-token 本地服务" : "Connecting to the local d-token service")
          : evidence === "partial"
            ? (zh ? "本地数据不完整 · 未确认当前状态" : "Local data is incomplete · current state unverified")
            : (zh ? "d-token 本地服务暂不可用" : "Local d-token service unavailable");
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
    return { timestamp, before, after };
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
      ? (zh ? "上次验证的 d-token 本地请求记录" : "Last verified local d-token request records")
      : requestLogs.length === 0
        ? (zh ? "本地服务在线 · 暂无请求记录" : "Local service online · no request records")
        : timestampedLogs.length === 0
          ? (zh ? "本地服务在线 · 请求缺少时间戳" : "Local service online · requests lack timestamps")
          : (zh ? "d-token 本地请求记录" : "Local d-token request records");

  return { available: anyEvidence, source, windows, cost: missingCost(lang) };
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
  const zh = lang === "zh";
  const label = zh ? "累计" : "Total";
  // 优先使用 durable 聚合值，避免被近期截断窗口低估。
  if (aggregateBefore !== null && aggregateAfter !== null) {
    return {
      key: "total",
      label,
      tokensSaved: Math.max(0, aggregateBefore - aggregateAfter),
      requestCount: aggregateTotal,
      state: "verified",
      note: zh ? "来自运行时的持久聚合 Token 记录" : "From the runtime's durable aggregate token records",
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
        ? zh ? "部分请求缺少压缩前后值，节省按可得请求计算" : "Some requests lack before/after values; savings cover available requests only"
        : zh ? "按压缩前后值计算；账单影响待确认。" : "Based on before/after values; billing impact pending",
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
      note: zh ? "暂无请求记录" : "No request records",
    };
  }
  // 有请求但完全没有压缩前后值 → 部分证据，不冒充 verified 0。
  return {
    key: "total",
    label,
    tokensSaved: null,
    requestCount: aggregateTotal ?? rawLogCount,
    state: "partial",
    note: zh ? "有请求但缺少压缩前后值" : "Requests exist but lack before/after values",
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
  const zh = lang === "zh";
  const label = key === "week" ? (zh ? "当周" : "This week") : key === "month" ? (zh ? "当月" : "This month") : (zh ? "当天" : "Today");
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
        note: zh ? "暂无本地请求记录" : "No local request records yet",
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
        note: zh ? "请求缺少时间戳，无法归入该窗口" : "Requests lack timestamps; cannot attribute to this window",
      };
    }
    return {
      key,
      label,
      tokensSaved: 0,
      requestCount: 0,
      state: "verified",
      note: zh ? "该窗口内没有请求记录" : "No requests in this window",
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
      note: zh ? "该窗口有请求但缺少压缩前后值" : "Requests exist but lack before/after values",
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
      ? zh ? "部分请求缺少压缩前后值，节省按可得请求计算" : "Some requests lack before/after values; savings cover available requests only"
      : zh ? "按压缩前后值计算；账单影响待确认" : "Based on before/after values; billing impact pending",
  };
}

function unavailableWindow(key: SavingsWindowKey, lang: Lang, evidence: EvidenceState): SavingsWindowSummary {
  const zh = lang === "zh";
  const label = key === "total" ? (zh ? "累计" : "Total") : key === "week" ? (zh ? "当周" : "This week") : key === "month" ? (zh ? "当月" : "This month") : (zh ? "当天" : "Today");
  const note =
    evidence === "preview"
      ? (zh ? "浏览器预览 · 未连接实时数据" : "Browser preview · no live data")
      : evidence === "loading"
        ? (zh ? "正在连接 d-token 本地服务" : "Connecting to the local d-token service")
        : evidence === "partial"
          ? (zh ? "本地数据不完整 · 未确认当前状态" : "Local data is incomplete · current state unverified")
          : (zh ? "d-token 本地服务暂不可用" : "Local d-token service unavailable");
  return { key, label, tokensSaved: null, requestCount: null, state: "unavailable", note };
}

function missingCost(lang: Lang): SavingsCostSummary {
  return {
    available: false,
    detail: lang === "zh"
      ? "缺少可信价格来源；不估算账单节省。Token 差额不等于账单节省。"
      : "No trusted pricing source; billing savings are not estimated. Token deltas are not billing savings.",
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
