import { describe, expect, it } from "vitest";

import { buildAnalyticsSnapshot } from "./snapshot";
import {
  allocateIntegerTotal,
  aggregateTokenMetrics,
  deriveLatencyTotal,
  deriveTokenMetrics,
} from "./metrics";

describe("analytics metric conservation", () => {
  it("derives saved tokens and percentage from one before/after baseline", () => {
    expect(deriveTokenMetrics(45_000, 34_500)).toEqual({
      before: 45_000,
      after: 34_500,
      saved: 10_500,
      savingsPercent: 23.3,
    });
    expect(aggregateTokenMetrics([
      { before: 1_000, after: 800 },
      { before: 500, after: 400 },
    ])).toEqual({ before: 1_500, after: 1_200, saved: 300, savingsPercent: 20 });
  });

  it("derives a latency total from explicit parts", () => {
    expect(deriveLatencyTotal({ compression: 12, routeAdapter: 8, provider: 80, ccSwitch: 0 })).toBe(100);
  });

  it("attributes an integer saved-token total without double counting", () => {
    const attributed = allocateIntegerTotal(10_500, [142_000, 98_400, 61_700, 24_800]);

    expect(attributed.reduce((sum, value) => sum + value, 0)).toBe(10_500);
    expect(attributed.every((value) => value >= 0)).toBe(true);
    expect(attributed).toEqual([...attributed].sort((a, b) => b - a));
  });
});

describe("analytics runtime snapshot truthfulness", () => {
  it("keeps an empty request snapshot empty instead of creating zero-filled trends or distributions", () => {
    const snapshot = buildAnalyticsSnapshot([]);

    expect(snapshot.hasRequests).toBe(false);
    expect(snapshot.requestCount).toBe(0);
    expect(snapshot.estimatedInputTokens).toBeNull();
    expect(snapshot.estimatedOutputTokens).toBeNull();
    expect(snapshot.estimatedSavedTokens).toBeNull();
    expect(snapshot.estimatedSavingsRate).toBeNull();
    expect(snapshot.latency).toBeNull();
    expect(snapshot.agentBreakdown).toEqual([]);
    expect(snapshot.upstreamBreakdown).toEqual([]);
    expect(snapshot.routeBreakdown).toEqual([]);
    expect(snapshot.strategyBreakdown).toEqual([]);
    expect(snapshot.trend7d).toEqual([]);
    expect(snapshot.trend30d).toEqual([]);
  });

  it("uses real RequestLog field names and derives one internally consistent snapshot", () => {
    const snapshot = buildAnalyticsSnapshot([
      {
        timestamp_ms: Date.UTC(2026, 6, 1),
        agent: "OpenCode",
        provider_id: "provider-main",
        provider: "legacy-provider-must-not-win",
        route_id: "route-main",
        route: "legacy-route-must-not-win",
        input_tokens_before: 1_000,
        input_tokens_after: 800,
        output_tokens: 80,
        tokens_saved: 200,
        transforms_applied: ["compact_json"],
        latency_ms: 100,
      },
      {
        timestamp_ms: Date.UTC(2026, 6, 2),
        agent: "OpenCode",
        provider_id: "provider-main",
        route_id: "route-main",
        input_tokens_before: 500,
        input_tokens_after: 400,
        output_tokens: 20,
        tokens_saved: 100,
        transforms_applied: ["compact_json", "drop_redundant_context"],
        latency_ms: 200,
        error: "provider_timeout",
      },
    ]);

    expect(snapshot.requestCount).toBe(2);
    expect(snapshot.estimatedInputTokens).toBe(1_500);
    expect(snapshot.estimatedOutputTokens).toBe(100);
    expect(snapshot.estimatedSavedTokens).toBe(300);
    expect(snapshot.estimatedSavingsRate).toBe(0.2);
    expect(snapshot.compressionCount).toBe(2);
    expect(snapshot.routeDecisionCount).toBe(2);
    expect(snapshot.errorCount).toBe(1);
    expect(snapshot.latency).toEqual({ sampleCount: 2, averageMs: 150, p50Ms: 150, p95Ms: 195 });
    expect(snapshot.agentBreakdown).toEqual([{ label: "OpenCode", count: 2, tokensSaved: 300 }]);
    expect(snapshot.upstreamBreakdown).toEqual([{ label: "provider-main", count: 2, tokensSaved: 300 }]);
    expect(snapshot.routeBreakdown).toEqual([{ label: "route-main", count: 2, tokensSaved: 300 }]);
    expect(snapshot.strategyBreakdown).toEqual([
      { label: "compact_json", count: 2, tokensSaved: null },
      { label: "drop_redundant_context", count: 1, tokensSaved: null },
    ]);
    expect(snapshot.trend30d).toEqual([
      { day: "2026-07-01", requests: 1, saved: 200 },
      { day: "2026-07-02", requests: 1, saved: 100 },
    ]);
    expect(JSON.stringify(snapshot)).not.toContain("legacy-provider-must-not-win");
    expect(JSON.stringify(snapshot)).not.toContain("legacy-route-must-not-win");
  });

  it("supports old persisted provider and route keys without replacing current contract fields", () => {
    const snapshot = buildAnalyticsSnapshot([
      { provider: "legacy-provider", route: "legacy-route" },
    ]);

    expect(snapshot.upstreamBreakdown[0]?.label).toBe("legacy-provider");
    expect(snapshot.routeBreakdown[0]?.label).toBe("legacy-route");
  });

  it("does not place requests without valid timestamps into trend data", () => {
    const snapshot = buildAnalyticsSnapshot([
      { agent: "OpenCode", input_tokens_before: 1_000, input_tokens_after: 800 },
      { timestamp_ms: Number.POSITIVE_INFINITY, tokens_saved: 25 },
    ]);

    expect(snapshot.trend7d).toEqual([]);
    expect(snapshot.trend30d).toEqual([]);
    expect(snapshot.warnings).toContain("2 条请求缺少有效时间戳，未计入趋势。");
  });

  it("corrects inconsistent reported savings in request logs", () => {
    const snapshot = buildAnalyticsSnapshot([
      {
        input_tokens_before: 1_000,
        input_tokens_after: 800,
        tokens_saved: 900,
      },
    ]);

    expect(snapshot.estimatedSavedTokens).toBe(200);
    expect(snapshot.estimatedSavingsRate).toBe(0.2);
    expect(snapshot.warnings).toContain("1 条请求的节省量与输入前后值不一致，已按输入前后值重新计算。");
  });

  it("consumes the extended request-log contract fields (status, protocols, provider usage, end-to-end timing)", () => {
    const snapshot = buildAnalyticsSnapshot([
      {
        request_id: "req-1",
        timestamp_ms: Date.UTC(2026, 6, 1),
        agent: "OpenCode",
        input_protocol: "openai",
        output_protocol: "openai",
        provider_id: "provider-main",
        route_id: "route-main",
        status: "ok",
        input_tokens_before: 1_000,
        input_tokens_after: 800,
        output_tokens: 80,
        provider_input_tokens: 800,
        tokens_saved: 200,
        transforms_applied: ["compact_json"],
        compression_ms: 12,
        received_at_ms: 1_000,
        completed_at_ms: 1_250,
        end_to_end_ms: 250,
        latency_ms: 12,
      },
      {
        request_id: "req-2",
        timestamp_ms: Date.UTC(2026, 6, 2),
        agent: "OpenCode",
        input_protocol: "openai",
        output_protocol: "openai",
        provider_id: "provider-main",
        route_id: "route-main",
        status: "failed",
        error_type: "provider_timeout",
        input_tokens_before: 500,
        input_tokens_after: 500,
        output_tokens: undefined,
        provider_input_tokens: undefined,
        tokens_saved: undefined,
        transforms_applied: [],
        compression_ms: 8,
        received_at_ms: 2_000,
        completed_at_ms: 2_100,
        end_to_end_ms: 100,
      },
    ]);

    expect(snapshot.requestCount).toBe(2);
    expect(snapshot.errorCount).toBe(1);
    // Provider input tokens never fabricate a 0; output aggregate covers the
    // one request with evidence only.
    expect(snapshot.estimatedOutputTokens).toBe(80);
    expect(snapshot.estimatedInputTokens).toBe(1_500);
    expect(snapshot.estimatedSavedTokens).toBe(200);
    expect(snapshot.pipeline).toEqual({
      compressionMs: 10,
      routeMs: null,
      upstreamTtfbMs: null,
      endToEndMs: 175,
    });
    expect(snapshot.warnings).toContain("部分请求缺少输出 Token 字段，输出总量仅覆盖有记录的请求。");
    expect(JSON.stringify(snapshot)).not.toContain("provider_input_tokens");
  });
});
