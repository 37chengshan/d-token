import { describe, expect, it } from "vitest";

import { hr } from "../fixtures";
import { buildAnalyticsSnapshot } from "./snapshot";
import {
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
  });

  it("keeps every preview input-saving view on the same total", () => {
    const agentTotals = aggregateTokenMetrics(hr.agents);
    const pipelineSaved = hr.pipeline.reduce((sum, row) => sum + row.tokens, 0);
    const historySaved = hr.history.reduce((sum, row) => sum + row.saved, 0);

    expect(agentTotals).toEqual(hr.agentTotals);
    expect(hr.tokens.saved).toBe(agentTotals.saved);
    expect(hr.tokens.savedPercent).toBe(agentTotals.savingsPercent);
    expect(hr.tokens.proxySaved + hr.tokens.cliSaved).toBe(hr.tokens.saved);
    expect(pipelineSaved).toBe(hr.tokens.saved);
    expect(historySaved).toBe(hr.tokens.saved);
  });

  it("marks browser preview output reduction as estimated", () => {
    const output = deriveTokenMetrics(hr.output.before, hr.output.after);

    expect(hr.output.method).toBe("ESTIMATED");
    expect(hr.output.saved).toBe(output.saved);
    expect(hr.output.percent).toBe(output.savingsPercent);
  });

  it("derives the preview request-path total from its latency parts", () => {
    expect(
      deriveLatencyTotal({
        compression: hr.throughput.compP50,
        routeAdapter: hr.throughput.routeP50,
        provider: hr.throughput.fwdP50,
        ccSwitch: 0,
      }),
    ).toBe(hr.overheadMs);
  });

  it("corrects inconsistent reported savings in request logs", () => {
    const snapshot = buildAnalyticsSnapshot([
      {
        agent: "OpenCode",
        input_tokens_before: 1_000,
        input_tokens_after: 800,
        tokens_saved: 900,
      },
    ]);

    expect(snapshot.estimatedSavedTokens).toBe(200);
    expect(snapshot.estimatedSavingsRate).toBe(0.2);
    expect(snapshot.warnings).toContain("1 条请求的节省量与输入前后值不一致，已按输入前后值重新计算。");
  });
});
