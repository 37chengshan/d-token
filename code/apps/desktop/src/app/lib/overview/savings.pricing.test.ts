import { describe, expect, it } from "vitest";

import { buildPriceCache } from "../analytics/pricing";
import { deriveSavingsWindows } from "./savings";

const BASE_PAYLOAD = {
  source: "dtoken-owned-runtime",
  desktop_data_source: "dtoken-local-service",
};

function cacheWithPrices(rows: unknown[], overrides: { freshness?: "fresh" | "stale" | "unknown"; checkedAt?: string | null } = {}) {
  return buildPriceCache(rows, { source: "user-settings", freshness: overrides.freshness ?? "fresh", checkedAt: overrides.checkedAt ?? "unix:9" });
}

describe("savings cost derivation with a pricing cache", () => {
  it("keeps cost unavailable when no pricing source is provided (never fabricates)", () => {
    const zh = deriveSavingsWindows({ ...BASE_PAYLOAD, request_logs: [{ model: "gpt-4o", input_tokens_before: 1000, input_tokens_after: 800 }], stats: {} }, "verified", "zh");
    const en = deriveSavingsWindows({ ...BASE_PAYLOAD, request_logs: [{ model: "gpt-4o", input_tokens_before: 1000, input_tokens_after: 800 }], stats: {} }, "verified", "en");
    expect(zh.cost.available).toBe(false);
    expect(zh.cost.detail).toContain("价格");
    expect(en.cost.detail).toContain("pricing");
  });

  it("estimates cost when every contributing request model resolves to a configured price", () => {
    const pricing = cacheWithPrices([
      { model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
    ]);
    const derivation = deriveSavingsWindows(
      {
        ...BASE_PAYLOAD,
        request_logs: [
          { model: "gpt-4o", input_tokens_before: 1_000_000, input_tokens_after: 800_000 },
          { model: "gpt-4o", input_tokens_before: 500_000, input_tokens_after: 400_000 },
        ],
        stats: {},
      },
      "verified",
      "zh",
      new Date(),
      pricing,
    );
    expect(derivation.cost.available).toBe(true);
    if (derivation.cost.available) {
      expect(derivation.cost.currency).toBe("USD");
      expect(derivation.cost.estimatedCost).toBe(0.75);
      expect(derivation.cost.source).toBe("user-settings");
      expect(derivation.cost.freshness).toBe("fresh");
      expect(derivation.cost.checkedAt).toBe("unix:9");
      expect(derivation.cost.detail).toContain("USD");
    }
  });

  it("matches known model aliases from request logs against configured prices", () => {
    const pricing = cacheWithPrices([
      { model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
    ]);
    const derivation = deriveSavingsWindows(
      {
        ...BASE_PAYLOAD,
        request_logs: [{ model: "gpt-4o-2024-08-06", input_tokens_before: 1_000_000, input_tokens_after: 800_000 }],
        stats: {},
      },
      "verified",
      "zh",
      new Date(),
      pricing,
    );
    expect(derivation.cost.available).toBe(true);
  });

  it("estimates a custom model only when an explicit price is configured", () => {
    const pricing = cacheWithPrices([
      { model: "my-custom-llm", currency: "USD", inputPerMillion: 1.2, outputPerMillion: 4.8, source: "user", updatedAt: "unix:1" },
    ]);
    const derivation = deriveSavingsWindows(
      {
        ...BASE_PAYLOAD,
        request_logs: [{ model: "my-custom-llm", input_tokens_before: 1_000_000, input_tokens_after: 900_000 }],
        stats: {},
      },
      "verified",
      "zh",
      new Date(),
      pricing,
    );
    expect(derivation.cost.available).toBe(true);
    if (derivation.cost.available) expect(derivation.cost.estimatedCost).toBe(0.12);
  });

  it("shows unavailable (never zero) when any contributing model lacks a price", () => {
    const pricing = cacheWithPrices([
      { model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
    ]);
    const zh = deriveSavingsWindows(
      {
        ...BASE_PAYLOAD,
        request_logs: [
          { model: "gpt-4o", input_tokens_before: 1_000_000, input_tokens_after: 800_000 },
          { model: "unknown-price-model", input_tokens_before: 500_000, input_tokens_after: 400_000 },
        ],
        stats: {},
      },
      "verified",
      "zh",
      new Date(),
      pricing,
    );
    expect(zh.cost.available).toBe(false);
    expect(zh.cost.detail).toContain("unknown-price-model");
    const en = deriveSavingsWindows(
      {
        ...BASE_PAYLOAD,
        request_logs: [{ model: "unknown-price-model", input_tokens_before: 500_000, input_tokens_after: 400_000 }],
        stats: {},
      },
      "verified",
      "en",
      new Date(),
      pricing,
    );
    expect(en.cost.available).toBe(false);
    expect(en.cost.detail).toContain("unknown-price-model");
  });

  it("surfaces a stale cache without blocking estimation", () => {
    const pricing = cacheWithPrices(
      [
        { model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
      ],
      { freshness: "stale", checkedAt: "unix:1" },
    );
    const derivation = deriveSavingsWindows(
      {
        ...BASE_PAYLOAD,
        request_logs: [{ model: "gpt-4o", input_tokens_before: 1_000_000, input_tokens_after: 900_000 }],
        stats: {},
      },
      "verified",
      "zh",
      new Date(),
      pricing,
    );
    expect(derivation.cost.available).toBe(true);
    if (derivation.cost.available) {
      expect(derivation.cost.freshness).toBe("stale");
      expect(derivation.cost.checkedAt).toBe("unix:1");
    }
  });

  it("never estimates from preview or loading evidence even with a pricing cache", () => {
    const pricing = cacheWithPrices([
      { model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
    ]);
    for (const evidence of ["preview", "loading", "unavailable", "partial"] as const) {
      const derivation = deriveSavingsWindows(
        {
          ...BASE_PAYLOAD,
          request_logs: [{ model: "gpt-4o", input_tokens_before: 1_000_000, input_tokens_after: 900_000 }],
          stats: {},
        },
        evidence,
        "zh",
        new Date(),
        pricing,
      );
      expect(derivation.cost.available).toBe(false);
    }
  });

  it("keeps mixed-currency estimates unavailable instead of merging currencies", () => {
    const pricing = cacheWithPrices([
      { model: "usd-model", currency: "USD", inputPerMillion: 2, outputPerMillion: 8, source: "user", updatedAt: "unix:1" },
      { model: "cny-model", currency: "CNY", inputPerMillion: 14, outputPerMillion: 56, source: "user", updatedAt: "unix:2" },
    ]);
    const derivation = deriveSavingsWindows(
      {
        ...BASE_PAYLOAD,
        request_logs: [
          { model: "usd-model", input_tokens_before: 1_000_000, input_tokens_after: 900_000 },
          { model: "cny-model", input_tokens_before: 1_000_000, input_tokens_after: 900_000 },
        ],
        stats: {},
      },
      "verified",
      "zh",
      new Date(),
      pricing,
    );
    expect(derivation.cost.available).toBe(false);
    expect(derivation.cost.detail.length).toBeGreaterThan(0);
  });
});
