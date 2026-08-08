import { describe, expect, it, vi } from "vitest";

import {
  buildPriceCache,
  estimateRequestInputCost,
  estimateSavingsCost,
  loadModelPriceCache,
  normalizeModelId,
  resolveModelPrice,
  type ModelPriceCache,
} from "./pricing";

const SAMPLE_PRICES = [
  { model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
  { model: "claude-sonnet-4-5", currency: "USD", inputPerMillion: 3, outputPerMillion: 15, source: "user", updatedAt: "unix:2" },
];

function sampleCache(overrides: Partial<ModelPriceCache> = {}): ModelPriceCache {
  return buildPriceCache(SAMPLE_PRICES, { source: "user-settings", freshness: "fresh", checkedAt: "unix:9" })!;
}

describe("normalizeModelId alias normalization", () => {
  it("trims, lowercases and collapses whitespace", () => {
    expect(normalizeModelId("  GPT-4O  ")).toBe("gpt-4o");
    expect(normalizeModelId("claude Sonnet 4 5")).toBe("claude sonnet 4 5");
  });

  it("strips provider/ prefixes", () => {
    expect(normalizeModelId("openai/gpt-4o")).toBe("gpt-4o");
    expect(normalizeModelId("anthropic/claude-sonnet-4-5")).toBe("claude-sonnet-4-5");
  });

  it("maps known versioned aliases to canonical ids", () => {
    expect(normalizeModelId("gpt-4o-2024-08-06")).toBe("gpt-4o");
    expect(normalizeModelId("gpt-4o-mini-2024-07-18")).toBe("gpt-4o-mini");
    expect(normalizeModelId("claude-3-5-sonnet-20241022")).toBe("claude-3-5-sonnet");
  });

  it("returns null for non-strings and empty values", () => {
    expect(normalizeModelId(null)).toBeNull();
    expect(normalizeModelId(42)).toBeNull();
    expect(normalizeModelId("   ")).toBeNull();
    expect(normalizeModelId("provider/  ")).toBeNull();
  });
});

describe("buildPriceCache", () => {
  it("builds a cache with source, version, freshness and checkedAt metadata", () => {
    const cache = buildPriceCache(SAMPLE_PRICES, { source: "user-settings", freshness: "fresh", checkedAt: "unix:9" });
    expect(cache).not.toBeNull();
    expect(cache!.source).toBe("user-settings");
    expect(cache!.freshness).toBe("fresh");
    expect(cache!.checkedAt).toBe("unix:9");
    expect(cache!.prices).toHaveLength(2);
  });

  it("returns null for a non-array or empty table", () => {
    expect(buildPriceCache(null)).toBeNull();
    expect(buildPriceCache("nope")).toBeNull();
    expect(buildPriceCache([])).toBeNull();
  });

  it("drops rows with malformed models, non-finite or negative prices, never carrying NaN/negative into the cache", () => {
    const cache = buildPriceCache([
      { model: "ok-model", currency: "USD", inputPerMillion: 1, outputPerMillion: 2, source: "user", updatedAt: "unix:1" },
      { model: "", currency: "USD", inputPerMillion: 1, outputPerMillion: 2, source: "user", updatedAt: "unix:1" },
      { model: "nan-model", currency: "USD", inputPerMillion: Number.NaN, outputPerMillion: 2, source: "user", updatedAt: "unix:1" },
      { model: "negative-model", currency: "USD", inputPerMillion: -1, outputPerMillion: 2, source: "user", updatedAt: "unix:1" },
      { model: "string-price", currency: "USD", inputPerMillion: "2", outputPerMillion: 2, source: "user", updatedAt: "unix:1" },
    ]);
    expect(cache).not.toBeNull();
    expect(cache!.prices).toEqual([
      expect.objectContaining({ model: "ok-model", inputPerMillion: 1 }),
    ]);
  });

  it("dedupes the same model+currency and normalizes model ids", () => {
    const cache = buildPriceCache([
      { model: "GPT-4O", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
      { model: "gpt-4o", currency: "USD", inputPerMillion: 9, outputPerMillion: 9, source: "user", updatedAt: "unix:2" },
      { model: "gpt-4o", currency: "CNY", inputPerMillion: 18, outputPerMillion: 72, source: "user", updatedAt: "unix:3" },
    ]);
    expect(cache!.prices).toHaveLength(2);
    expect(cache!.prices.map((row) => row.model)).toEqual(["gpt-4o", "gpt-4o"]);
    expect(cache!.prices.map((row) => row.currency)).toEqual(["USD", "CNY"]);
  });
});

describe("resolveModelPrice", () => {
  it("resolves an exact normalized match", () => {
    const resolved = resolveModelPrice(sampleCache(), "gpt-4o");
    expect(resolved).toEqual(expect.objectContaining({ matchedModel: "gpt-4o", currency: "USD", inputPerMillion: 2.5 }));
  });

  it("resolves provider-prefixed and versioned aliases to the configured model", () => {
    const cache = sampleCache();
    expect(resolveModelPrice(cache, "openai/gpt-4o")?.matchedModel).toBe("gpt-4o");
    expect(resolveModelPrice(cache, "gpt-4o-2024-08-06")?.matchedModel).toBe("gpt-4o");
  });

  it("prefers USD when no currency is requested, falling back to CNY", () => {
    const cache = buildPriceCache([
      { model: "cn-only", currency: "CNY", inputPerMillion: 18, outputPerMillion: 90, source: "user", updatedAt: "unix:1" },
    ])!;
    expect(resolveModelPrice(cache, "cn-only")?.currency).toBe("CNY");
  });

  it("respects an explicit currency preference", () => {
    const cache = buildPriceCache([
      { model: "dual", currency: "USD", inputPerMillion: 2, outputPerMillion: 8, source: "user", updatedAt: "unix:1" },
      { model: "dual", currency: "CNY", inputPerMillion: 14, outputPerMillion: 56, source: "user", updatedAt: "unix:2" },
    ])!;
    expect(resolveModelPrice(cache, "dual", "CNY")?.currency).toBe("CNY");
    expect(resolveModelPrice(cache, "dual", "USD")?.currency).toBe("USD");
  });

  it("returns null for unknown models and null caches, never guessing", () => {
    expect(resolveModelPrice(sampleCache(), "unknown-model-xyz")).toBeNull();
    expect(resolveModelPrice(null, "gpt-4o")).toBeNull();
    expect(resolveModelPrice(sampleCache(), "")).toBeNull();
  });

  it("resolves a custom model only when explicitly configured", () => {
    const cache = buildPriceCache([
      { model: "my-custom-llm", currency: "USD", inputPerMillion: 1.2, outputPerMillion: 4.8, source: "user", updatedAt: "unix:1" },
    ])!;
    expect(resolveModelPrice(cache, "my-custom-llm")?.inputPerMillion).toBe(1.2);
    expect(resolveModelPrice(cache, "my-custom-llm-v2")).toBeNull();
  });
});

describe("estimateSavingsCost", () => {
  it("estimates the cost delta from saved input tokens when every model resolves", () => {
    const cache = sampleCache();
    const estimate = estimateSavingsCost(
      [
        { model: "gpt-4o", before: 1_000_000, after: 800_000 },
        { model: "gpt-4o", before: 500_000, after: 400_000 },
      ],
      cache,
    );
    expect(estimate).toEqual({ available: true, currency: "USD", estimatedCost: 0.75, resolvedCount: 2 });
  });

  it("returns no-requests when no log carries both before and after values", () => {
    expect(estimateSavingsCost([{ model: "gpt-4o", before: 100, after: null }], sampleCache())).toEqual({
      available: false,
      reason: "no-requests",
      unknownModels: [],
    });
    expect(estimateSavingsCost([], sampleCache())).toEqual({ available: false, reason: "no-requests", unknownModels: [] });
  });

  it("returns no-cache when no pricing source is available", () => {
    expect(estimateSavingsCost([{ model: "gpt-4o", before: 100, after: 50 }], null)).toEqual({
      available: false,
      reason: "no-cache",
      unknownModels: [],
    });
  });

  it("is unavailable with unknown-models when any contributing model lacks an explicit price", () => {
    const cache = sampleCache();
    const estimate = estimateSavingsCost(
      [
        { model: "gpt-4o", before: 1_000_000, after: 800_000 },
        { model: "no-price-model", before: 500_000, after: 400_000 },
      ],
      cache,
    );
    expect(estimate.available).toBe(false);
    if (!estimate.available) {
      expect(estimate.reason).toBe("unknown-models");
      expect(estimate.unknownModels).toContain("no-price-model");
    }
  });

  it("is unavailable with mixed-currency instead of merging currencies", () => {
    const cache = buildPriceCache([
      { model: "usd-model", currency: "USD", inputPerMillion: 2, outputPerMillion: 8, source: "user", updatedAt: "unix:1" },
      { model: "cny-model", currency: "CNY", inputPerMillion: 14, outputPerMillion: 56, source: "user", updatedAt: "unix:2" },
    ])!;
    const estimate = estimateSavingsCost(
      [
        { model: "usd-model", before: 1_000_000, after: 900_000 },
        { model: "cny-model", before: 1_000_000, after: 900_000 },
      ],
      cache,
    );
    expect(estimate).toEqual({ available: false, reason: "mixed-currency", unknownModels: [] });
  });

  it("still estimates from a stale cache (freshness is surfaced metadata, not a blocker)", () => {
    const cache = buildPriceCache(SAMPLE_PRICES, { source: "user-settings", freshness: "stale", checkedAt: "unix:1" })!;
    expect(cache.freshness).toBe("stale");
    const estimate = estimateSavingsCost([{ model: "gpt-4o", before: 1_000_000, after: 900_000 }], cache);
    expect(estimate.available).toBe(true);
  });
});

describe("estimateRequestInputCost", () => {
  it("estimates pre/post input cost and the difference when model, tokens and price are all complete", () => {
    const estimate = estimateRequestInputCost(sampleCache(), "gpt-4o", 1_000_000, 900_000);
    expect(estimate).toEqual({
      available: true,
      currency: "USD",
      preCost: 2.5,
      postCost: 2.25,
      savedCost: 0.25,
      source: "user",
      matchedModel: "gpt-4o",
      cacheSource: "user-settings",
      freshness: "fresh",
      checkedAt: "unix:9",
    });
  });

  it("resolves provider-prefixed and versioned aliases against the configured price", () => {
    const estimate = estimateRequestInputCost(sampleCache(), "openai/gpt-4o-2024-08-06", 500_000, 400_000);
    expect(estimate).toEqual(expect.objectContaining({ available: true, matchedModel: "gpt-4o", preCost: 1.25, postCost: 1, savedCost: 0.25 }));
  });

  it("uses the entry currency and source for the pricing basis", () => {
    const cache = buildPriceCache([
      { model: "cny-model", currency: "CNY", inputPerMillion: 14, outputPerMillion: 56, source: "runtime", updatedAt: "unix:1" },
    ])!;
    const estimate = estimateRequestInputCost(cache, "cny-model", 1_000_000, 800_000);
    expect(estimate).toEqual(expect.objectContaining({ available: true, currency: "CNY", source: "runtime", preCost: 14, postCost: 11.2, savedCost: 2.8 }));
  });

  it("returns no-cache when no pricing source is available", () => {
    expect(estimateRequestInputCost(null, "gpt-4o", 1_000_000, 900_000)).toEqual({ available: false, reason: "no-cache" });
  });

  it("returns no-tokens when before/after evidence is missing or non-finite, never guessing", () => {
    expect(estimateRequestInputCost(sampleCache(), "gpt-4o", null, 900_000)).toEqual({ available: false, reason: "no-tokens" });
    expect(estimateRequestInputCost(sampleCache(), "gpt-4o", 1_000_000, undefined)).toEqual({ available: false, reason: "no-tokens" });
    expect(estimateRequestInputCost(sampleCache(), "gpt-4o", -5, 900_000)).toEqual({ available: false, reason: "no-tokens" });
    expect(estimateRequestInputCost(sampleCache(), "gpt-4o", Number.NaN, 900_000)).toEqual({ available: false, reason: "no-tokens" });
  });

  it("returns no-model for missing or empty model ids", () => {
    expect(estimateRequestInputCost(sampleCache(), null, 1_000_000, 900_000)).toEqual({ available: false, reason: "no-model" });
    expect(estimateRequestInputCost(sampleCache(), "   ", 1_000_000, 900_000)).toEqual({ available: false, reason: "no-model" });
  });

  it("returns no-price when the model has no matching cache entry", () => {
    expect(estimateRequestInputCost(sampleCache(), "no-price-model", 1_000_000, 900_000)).toEqual({ available: false, reason: "no-price" });
  });

  it("surfaces a stale cache with its freshness instead of blocking the estimate", () => {
    const cache = buildPriceCache(SAMPLE_PRICES, { source: "user-settings", freshness: "stale", checkedAt: "unix:1" })!;
    const estimate = estimateRequestInputCost(cache, "gpt-4o", 1_000_000, 900_000);
    expect(estimate).toEqual(expect.objectContaining({ available: true, freshness: "stale", checkedAt: "unix:1" }));
  });
});

describe("loadModelPriceCache", () => {
  it("returns null when the native settings contract is unavailable", async () => {
    const native = await import("../native");
    const spy = vi.spyOn(native, "invokeNative").mockRejectedValue(new Error("native_unavailable"));
    await expect(loadModelPriceCache()).resolves.toBeNull();
    spy.mockRestore();
  });

  it("builds a fresh cache from the model-pricing settings payload", async () => {
    const native = await import("../native");
    const spy = vi.spyOn(native, "invokeNative").mockResolvedValue({
      state: "loaded",
      payload: { prices: SAMPLE_PRICES, unit: "per_1m_tokens" },
    });
    const cache = await loadModelPriceCache();
    expect(cache).not.toBeNull();
    expect(cache!.source).toBe("user-settings");
    expect(cache!.freshness).toBe("fresh");
    expect(cache!.checkedAt).not.toBeNull();
    expect(cache!.prices).toHaveLength(2);
    spy.mockRestore();
  });

  it("returns null for a missing settings section", async () => {
    const native = await import("../native");
    const spy = vi.spyOn(native, "invokeNative").mockResolvedValue({ state: "missing", payload: {} });
    await expect(loadModelPriceCache()).resolves.toBeNull();
    spy.mockRestore();
  });
});
