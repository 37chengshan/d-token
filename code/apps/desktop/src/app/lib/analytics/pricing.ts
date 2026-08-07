import { invokeNative } from "../native";

/**
 * 模型价格缓存与解析（lib 层，非 UI）。
 *
 * 数据契约（与 Settings 的 model-pricing 段一致）：
 * - 价格单位：每 1M Tokens（per_1m_tokens），币种 USD / CNY。
 * - 价格只来自显式配置（用户价格表 / 运行时价格源），绝不内置大表、绝不猜测。
 * - 未知价格一律返回 null / unavailable，禁止用 0 冒充。
 * - 只保留 model、币种与每百万价格，不记录凭据、prompt/response 或请求内容。
 *
 * 缓存元数据：source（来源）、version（内容版本）、freshness（新鲜度）、
 * checkedAt（检查时间）；调用方按需展示。
 */

export type ModelCurrency = "USD" | "CNY";

export type ModelPriceEntry = {
  /** 规范化后的模型标识。 */
  model: string;
  currency: ModelCurrency;
  /** 每 1M Tokens 输入价格；必须为有限非负数。 */
  inputPerMillion: number;
  /** 每 1M Tokens 输出价格；必须为有限非负数。 */
  outputPerMillion: number;
  /** 价格来源，例如 "user"（Settings 用户表）或运行时来源标识。 */
  source: string;
  /** 写入/更新时间，例如 "unix:N" 或 ISO 字符串。 */
  updatedAt: string;
};

export type PriceCacheFreshness = "fresh" | "stale" | "unknown";

export type ModelPriceCache = {
  /** 缓存来源，例如 "user-settings"。 */
  source: string;
  /** 内容版本；来自 payload revision 或 entries 的 updatedAt 摘要。 */
  version: string;
  freshness: PriceCacheFreshness;
  /** 检查时间（ISO 或 "unix:N"）；未检查时为 null。 */
  checkedAt: string | null;
  prices: ModelPriceEntry[];
};

export type ResolvedModelPrice = {
  inputPerMillion: number;
  outputPerMillion: number;
  currency: ModelCurrency;
  source: string;
  /** 实际命中的条目模型标识（规范化后）。 */
  matchedModel: string;
};

export type CostEstimate =
  | {
      available: true;
      currency: ModelCurrency;
      /** 估算的成本差额（节省 Token × 输入价格），单位为该币种。 */
      estimatedCost: number;
      /** 成功解析价格的请求数。 */
      resolvedCount: number;
    }
  | {
      available: false;
      reason: "no-requests" | "no-cache" | "unknown-models" | "mixed-currency";
      /** 无法解析价格的规范化模型 id（去重、有界）。 */
      unknownModels: string[];
    };

/**
 * 已知模型别名：版本化/厂商变体 → 规范 id。
 * 只做标识规范化，不携带任何价格；保持小表，价格一律来自显式配置。
 * 新增别名时需同时补充行为测试。
 */
const KNOWN_MODEL_ALIASES: Record<string, string> = {
  "gpt-4o-2024-08-06": "gpt-4o",
  "gpt-4o-2024-05-13": "gpt-4o",
  "gpt-4o-mini-2024-07-18": "gpt-4o-mini",
  "claude-3-5-sonnet-20241022": "claude-3-5-sonnet",
  "claude-3-5-haiku-20241022": "claude-3-5-haiku",
  "claude-sonnet-4-5-20250929": "claude-sonnet-4-5",
};

/**
 * 规范化模型标识：trim、小写、折叠空白、去掉 `provider/` 前缀，
 * 再应用已知别名表。非字符串或空串返回 null。
 */
export function normalizeModelId(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim().toLowerCase().replace(/\s+/g, " ");
  if (!trimmed) return null;
  const withoutProvider = trimmed.includes("/") ? trimmed.slice(trimmed.lastIndexOf("/") + 1) : trimmed;
  const canonical = withoutProvider.trim();
  if (!canonical) return null;
  return KNOWN_MODEL_ALIASES[canonical] ?? canonical;
}

function finiteNonNegative(value: unknown): number | null {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return null;
  return value;
}

/**
 * 从 Settings model-pricing 段（或等价运行时来源）的原始行构建价格缓存。
 * - 非对象 / 无 model / 价格非有限非负的行会被丢弃（绝不带 NaN 或负数进缓存）。
 * - model 规范化后写入；同一 model+currency 只保留首条。
 * - 空表返回 null（等价于没有价格来源）。
 */
export function buildPriceCache(
  rawPrices: unknown,
  meta: { source?: string; version?: string; freshness?: PriceCacheFreshness; checkedAt?: string | null } = {},
): ModelPriceCache | null {
  if (!Array.isArray(rawPrices)) return null;
  const prices: ModelPriceEntry[] = [];
  const seen = new Set<string>();
  for (const raw of rawPrices) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) continue;
    const entry = raw as Record<string, unknown>;
    const model = normalizeModelId(entry.model);
    if (!model) continue;
    const currency: ModelCurrency = entry.currency === "CNY" ? "CNY" : "USD";
    const inputPerMillion = finiteNonNegative(entry.inputPerMillion);
    const outputPerMillion = finiteNonNegative(entry.outputPerMillion);
    if (inputPerMillion === null || outputPerMillion === null) continue;
    const key = `${model}|${currency}`;
    if (seen.has(key)) continue;
    seen.add(key);
    prices.push({
      model,
      currency,
      inputPerMillion,
      outputPerMillion,
      source: typeof entry.source === "string" && entry.source ? entry.source : "user",
      updatedAt: typeof entry.updatedAt === "string" ? entry.updatedAt : "",
    });
  }
  if (prices.length === 0) return null;
  const updatedAtValues = prices.map((row) => row.updatedAt).filter((value) => value.length > 0).sort();
  const latestUpdatedAt = updatedAtValues.length > 0 ? updatedAtValues[updatedAtValues.length - 1] : undefined;
  return {
    source: meta.source ?? "user-settings",
    version: meta.version ?? (latestUpdatedAt ? `updated:${latestUpdatedAt}` : `rows:${prices.length}`),
    freshness: meta.freshness ?? "unknown",
    checkedAt: meta.checkedAt ?? null,
    prices,
  };
}

/**
 * 解析模型价格：规范化 model 后精确匹配缓存条目。
 * 优先请求的币种；未指定时 USD 优先、CNY 兜底。未知返回 null，绝不猜测。
 */
export function resolveModelPrice(
  cache: ModelPriceCache | null,
  model: unknown,
  currency?: ModelCurrency,
): ResolvedModelPrice | null {
  if (!cache) return null;
  const canonical = normalizeModelId(model);
  if (!canonical) return null;
  const candidates = cache.prices.filter((row) => row.model === canonical);
  if (candidates.length === 0) return null;
  const preferred = currency
    ? candidates.find((row) => row.currency === currency) ?? candidates[0]
    : candidates.find((row) => row.currency === "USD") ?? candidates[0];
  return {
    inputPerMillion: preferred.inputPerMillion,
    outputPerMillion: preferred.outputPerMillion,
    currency: preferred.currency,
    source: preferred.source,
    matchedModel: preferred.model,
  };
}

/**
 * 估算成本差额：Σ max(0, before - after) / 1_000_000 × inputPerMillion。
 *
 * 严格证据规则：
 * - 任一计入请求的模型价格无法解析 → unavailable（unknown-models），绝不部分估算或补 0。
 * - 多条请求解析出不同币种 → unavailable（mixed-currency），绝不混币合并。
 * - 没有可计费请求 → unavailable（no-requests）。
 */
export function estimateSavingsCost(
  logs: ReadonlyArray<{ model: unknown; before: number | null; after: number | null }>,
  cache: ModelPriceCache | null,
): CostEstimate {
  if (!cache) return { available: false, reason: "no-cache", unknownModels: [] };
  const byCurrency = new Map<ModelCurrency, { cost: number; count: number }>();
  const unknownModels = new Set<string>();
  let resolvedCount = 0;
  for (const log of logs) {
    if (log.before === null || log.after === null) continue;
    const saved = Math.max(0, log.before - log.after);
    if (saved <= 0) continue;
    const price = resolveModelPrice(cache, log.model);
    if (!price) {
      const canonical = normalizeModelId(log.model);
      if (canonical) unknownModels.add(canonical);
      continue;
    }
    const current = byCurrency.get(price.currency) ?? { cost: 0, count: 0 };
    current.cost += (saved / 1_000_000) * price.inputPerMillion;
    current.count += 1;
    byCurrency.set(price.currency, current);
    resolvedCount += 1;
  }
  if (resolvedCount === 0) {
    return {
      available: false,
      reason: unknownModels.size > 0 ? "unknown-models" : "no-requests",
      unknownModels: [...unknownModels].slice(0, 8),
    };
  }
  if (unknownModels.size > 0) {
    return { available: false, reason: "unknown-models", unknownModels: [...unknownModels].slice(0, 8) };
  }
  if (byCurrency.size !== 1) {
    return { available: false, reason: "mixed-currency", unknownModels: [] };
  }
  const [currency, summary] = [...byCurrency.entries()][0];
  return {
    available: true,
    currency,
    estimatedCost: Number(summary.cost.toFixed(4)),
    resolvedCount: summary.count,
  };
}

export type RequestInputCostEstimate =
  | {
      available: true;
      /** 估算币种（命中条目的币种）。 */
      currency: ModelCurrency;
      /** 估算的压缩前输入成本（该币种金额）。 */
      preCost: number;
      /** 估算的压缩后输入成本（该币种金额）。 */
      postCost: number;
      /** 估算的成本差额 = max(0, preCost - postCost)，该币种金额。 */
      savedCost: number;
      /** 命中价格条目的来源（如 user）。 */
      source: string;
      /** 命中条目规范化后的模型标识。 */
      matchedModel: string;
      /** 缓存来源（如 user-settings）。 */
      cacheSource: string;
      freshness: PriceCacheFreshness;
      checkedAt: string | null;
    }
  | {
      available: false;
      reason: "no-cache" | "no-model" | "no-price" | "no-tokens";
    };

/**
 * 单条请求的输入成本估算：压缩前 / 压缩后 / 差额。
 *
 * 严格证据规则（与 estimateSavingsCost 一致）：
 * - cache 缺失 → no-cache；model 无法规范化 → no-model；
 *   before/after 任一缺失或非有限非负 → no-tokens；价格无法解析 → no-price。
 * - 全部齐全才给出估算，绝不补 0、绝不猜测。
 * - 只估算输入成本；输出 token 不参与 before/after 计算。
 */
export function estimateRequestInputCost(
  cache: ModelPriceCache | null,
  model: unknown,
  before: unknown,
  after: unknown,
): RequestInputCostEstimate {
  if (!cache) return { available: false, reason: "no-cache" };
  const beforeTokens = finiteNonNegative(before);
  const afterTokens = finiteNonNegative(after);
  if (beforeTokens === null || afterTokens === null) return { available: false, reason: "no-tokens" };
  const price = resolveModelPrice(cache, model);
  if (!price) {
    const canonical = normalizeModelId(model);
    return { available: false, reason: canonical ? "no-price" : "no-model" };
  }
  const preCost = (beforeTokens / 1_000_000) * price.inputPerMillion;
  const postCost = (afterTokens / 1_000_000) * price.inputPerMillion;
  return {
    available: true,
    currency: price.currency,
    preCost: Number(preCost.toFixed(6)),
    postCost: Number(postCost.toFixed(6)),
    savedCost: Number(Math.max(0, preCost - postCost).toFixed(6)),
    source: price.source,
    matchedModel: price.matchedModel,
    cacheSource: cache.source,
    freshness: cache.freshness,
    checkedAt: cache.checkedAt,
  };
}

/**
 * 从当前有效配置（Settings model-pricing 段）加载价格缓存。
 * 读取失败或原生不可用时返回 null（价格未知，不猜测）。
 */
export async function loadModelPriceCache(): Promise<ModelPriceCache | null> {
  try {
    const saved = await invokeNative<{ state?: string; payload?: Record<string, unknown> }>("get_app_settings", {
      section: "model-pricing",
    });
    if (saved?.state === "missing") return null;
    return buildPriceCache(saved?.payload?.prices, {
      source: "user-settings",
      freshness: "fresh",
      checkedAt: new Date().toISOString(),
    });
  } catch {
    return null;
  }
}
