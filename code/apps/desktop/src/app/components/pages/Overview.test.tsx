import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const dashboard = vi.hoisted(() => ({
  state: {
    payload: null as Record<string, unknown> | null,
    status: "browser-preview",
    updatedAt: null as Date | null,
    error: "",
    refresh: vi.fn(),
  },
}));

const connection = vi.hoisted(() => ({
  state: {
    snapshot: null as Record<string, unknown> | null,
    status: "unavailable",
    error: "",
    refresh: vi.fn(),
  },
}));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => dashboard.state,
  useRuntimeDashboardSelector: (selector: (state: unknown) => unknown) => selector(dashboard.state),
  shallowEqualSlice: () => false,
}));

vi.mock("../../lib/connectionSetup", () => ({
  useConnectionSetup: () => connection.state,
}));

vi.mock("../common/Heatmap", () => ({
  Heatmap: () => <div data-testid="activity-heatmap">activity heatmap</div>,
}));

vi.mock("recharts", () => ({
  ResponsiveContainer: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  AreaChart: ({ children }: { children: React.ReactNode }) => <svg>{children}</svg>,
  Area: () => null,
  Tooltip: () => null,
  XAxis: () => null,
}));

const pricingMock = vi.hoisted(() => ({
  loadModelPriceCache: vi.fn(),
}));

vi.mock("../../lib/analytics/pricing", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/analytics/pricing")>();
  return { ...actual, loadModelPriceCache: pricingMock.loadModelPriceCache };
});

import { Overview, deriveAccountQuotaSummary, deriveOverviewOperationalSummary, overviewFromPayload, deriveSavingsWindows } from "./Overview";
import { buildPriceCache } from "../../lib/analytics/pricing";

const runtimePayload = {
  source: "dtoken-owned-runtime",
  desktop_data_source: "dtoken-local-service",
  providers: [{ id: "provider-main", name: "本地主上游", kind: "custom" }],
  routes: [{ id: "route-main", name: "主连接", target_provider_id: "provider-main" }],
  instances: [{ id: "instance-opencode", agent: "OpenCode", route_id: "route-main" }],
  request_logs: [
    { agent: "OpenCode", route_id: "route-main", tokens_saved: 0, input_tokens_before: 100, input_tokens_after: 52 },
    { agent: "OpenCode", route_id: "route-main", error: "provider_timeout", tokens_saved: 0, input_tokens_before: 30, input_tokens_after: 30 },
  ],
  stats: { tokens: { saved: 48, input: 130 }, savings_percent: 36.9 },
  history: { history: [] },
};

beforeEach(() => {
  dashboard.state = {
    payload: runtimePayload,
    status: "runtime",
    updatedAt: new Date("2026-07-10T12:00:00Z"),
    error: "",
    refresh: vi.fn(),
  };
  connection.state = {
    snapshot: null,
    status: "unavailable",
    error: "",
    refresh: vi.fn(),
  };
  pricingMock.loadModelPriceCache.mockReset();
  pricingMock.loadModelPriceCache.mockResolvedValue(null);
});

afterEach(cleanup);

describe("Overview information hierarchy and truthfulness", () => {
  it("derives service, agent, route and issue status from runtime evidence", () => {
    const summary = deriveOverviewOperationalSummary(runtimePayload, "runtime");

    expect(summary.service.label).toBe("正在运行");
    expect(summary.agents.label).toBe("OpenCode");
    expect(summary.agents.detail).toContain("历史中有 2 条请求");
    expect(summary.agents.tone).toBe("warning");
    expect(summary.route.label).toBe("主连接 → 本地主上游");
    expect(summary.issues.label).toBe("1 个问题");
  });

  it("never calls cc-switch synced without explicit verification evidence", () => {
    const summary = deriveOverviewOperationalSummary({
      ...runtimePayload,
      providers: [{ id: "cc", name: "cc-switch", kind: "cc_switch" }],
    }, "runtime");

    expect(summary.ccSwitch.state).toBe("unverified");
    expect(summary.ccSwitch.label).not.toContain("已同步");
    expect(summary.ccSwitch.label).not.toContain("正常");
  });

  it("uses the shared native connection snapshot for configured Agents and cc-switch sync", () => {
    const summary = deriveOverviewOperationalSummary({
      source: "dtoken-owned-runtime",
      desktop_data_source: "dtoken-local-service",
      request_logs: [],
      providers: [],
      routes: [],
      instances: [],
    }, "runtime", "zh", {
      status: "ready",
      snapshot: {
        source: "dtoken-desktop-read-only-discovery",
        freshness: "unix:1",
        revision: "setup-1",
        state: "loaded",
        candidates: [],
        agent_bindings: [{
          agent_id: "codex",
          agent_name: "Codex",
          installed: true,
          current_config_source: "agent-config:codex",
          candidates: [],
          resolution: "already-connected",
          confidence: 100,
          reasons: [],
          capability: "one-click",
          already_connected: true,
        }],
        sources: [],
        selection_required: false,
        cc_switch_sync: {
          detected: true,
          status: "synced",
          profile: "profile-a",
          upstream: "https://provider.example/v1",
          connection_path: "Agent -> d-token -> cc-switch",
          source: "cc-switch-read-only",
          writes_performed: false,
        },
        writes_performed: false,
        privacy: "metadata only",
      },
    });

    expect(summary.agents.label).toBe("Codex");
    expect(summary.agents.detail).toContain("当前配置已指向 d-token");
    expect(summary.agents.tone).toBe("positive");
    expect(summary.ccSwitch.state).toBe("synced");
    expect(summary.ccSwitch.detail).toContain("profile-a");
  });

  it("keeps stale connection discovery visible without presenting it as live traffic or verified sync", () => {
    const snapshot = {
      source: "dtoken-desktop-read-only-discovery",
      freshness: "unix:1",
      revision: "setup-1",
      state: "loaded",
      candidates: [],
      agent_bindings: [{
        agent_id: "codex",
        agent_name: "Codex",
        installed: true,
        current_config_source: "agent-config:codex",
        candidates: [],
        resolution: "already-connected" as const,
        confidence: 100,
        reasons: [],
        capability: "one-click" as const,
        already_connected: true,
      }],
      sources: [],
      selection_required: false,
      cc_switch_sync: {
        detected: true,
        status: "synced",
        source: "cc-switch-read-only",
        writes_performed: false as const,
      },
      writes_performed: false as const,
      privacy: "metadata only",
    };
    const summary = deriveOverviewOperationalSummary(null, "unavailable", "zh", {
      status: "stale",
      snapshot,
    });

    expect(summary.agents.label).toBe("Codex");
    expect(summary.agents.detail).toContain("配置证据已过期");
    expect(summary.agents.detail).not.toContain("真实请求证据");
    expect(summary.ccSwitch.state).toBe("unverified");
  });

  it.each([
    ["stale", { ...runtimePayload, data_status: "stale" }, "状态已过期"],
    ["partial", { ...runtimePayload, partial: true }, "状态不完整"],
  ])("does not present %s runtime payloads as healthy", (_state, payload, serviceLabel) => {
    const summary = deriveOverviewOperationalSummary(payload, "runtime");

    expect(summary.service.label).toBe(serviceLabel);
    expect(summary.service.tone).not.toBe("positive");
    expect(summary.agents.label).toBe("尚无已验证连接");
    expect(summary.route.tone).not.toBe("positive");
    expect(summary.issues.label).toBe("尚无当前问题证据");
    expect(summary.ccSwitch.state).toBe("unknown");
  });

  it("shows empty metrics instead of preview success data", () => {
    dashboard.state = {
      payload: null,
      status: "browser-preview",
      updatedAt: null,
      error: "",
      refresh: vi.fn(),
    };
    render(<Overview setPage={vi.fn()} />);

    expect(screen.getAllByText("浏览器预览 · 未连接实时数据")).not.toHaveLength(0);
    expect(screen.queryByText("浏览器预览 · 示例数据")).not.toBeInTheDocument();
    const kpis = screen.getByTestId("overview-kpis");
    expect(within(kpis).getAllByText("—")).toHaveLength(4);
    expect(within(kpis).queryByText("0")).not.toBeInTheDocument();
  });

  it("shows verified zero request and error KPIs when runtime exposes an empty request collection", () => {
    dashboard.state = {
      payload: {
        source: "dtoken-owned-runtime",
        desktop_data_source: "dtoken-local-service",
        providers: [{ id: "provider-main", name: "本地主上游" }],
        routes: [{ id: "route-main", name: "主连接", target_provider_id: "provider-main" }],
        request_logs: [],
        stats: { tokens: { saved: 120, input: 480 }, savings_percent: 25 },
        history: { history: [] },
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Overview setPage={vi.fn()} />);

    const kpis = screen.getByTestId("overview-kpis");
    expect(within(kpis).getAllByText("0")).toHaveLength(3);
    expect(within(kpis).getByText("0.0%")).toBeVisible();
    expect(screen.getAllByText("本地服务在线 · 暂无请求记录")).not.toHaveLength(0);
    expect(screen.getByText("尚无请求记录")).toBeVisible();
  });

  it("keeps last verified KPI values visible while clearly marking stale evidence", () => {
    const stale = overviewFromPayload({
      ...runtimePayload,
      evidence_state: "stale",
      freshness: "stale",
      complete: false,
      partial: true,
      runtime_available: false,
      stats: {
        requests: { total: 125, failed: 20 },
        tokens: { input: 1_000, input_after: 750, saved: 100 },
      },
    }, "stale", "zh");

    expect(stale.trafficAvailable).toBe(true);
    expect(stale.metricsAvailable).toBe(true);
    expect(stale.stats.requestCount).toBe(125);
    expect(stale.stats.savedTotal).toBe(250);
    expect(stale.stats.errorsToday).toBe(20);
    expect(stale.sourceLabel).toContain("上次验证");
    expect(stale.sourceLabel).toContain("数据已过期");
    expect(stale.errorLabel).toContain("非当前状态");
  });

  it("shows real zero KPIs when the verified runtime has a valid empty traffic collection", () => {
    dashboard.state = {
      payload: {
        source: "dtoken-owned-runtime",
        desktop_data_source: "dtoken-local-service",
        request_logs: [],
        stats: {},
        history: { history: [] },
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Overview setPage={vi.fn()} />);

    const kpis = screen.getByTestId("overview-kpis");
    expect(within(kpis).getAllByText("0")).toHaveLength(3);
    expect(within(kpis).getByText("0.0%")).toBeVisible();
  });

  it("skips incomplete timestamps and fields without fabricating activity, tools, trends, or agents", () => {
    const view = overviewFromPayload({
      source: "dtoken-owned-runtime",
      desktop_data_source: "dtoken-local-service",
      request_logs: [
        { transforms_applied: ["must_not_appear"] },
        { timestamp_ms: "invalid", latency_ms: 120_000, transforms_applied: ["also_invalid"] },
        { timestamp_ms: Date.UTC(2026, 6, 10), transforms_applied: [] },
      ],
      stats: {},
      history: { history: [{ saved: 99 }, { bucket_ms: "invalid", saved: 30 }] },
    }, "runtime");

    expect(view.trafficAvailable).toBe(true);
    expect(view.metricsAvailable).toBe(true);
    expect(view.compressionAvailable).toBe(true);
    expect(view.stats.requestCount).toBe(3);
    expect(view.stats.activeAgents).toBe(0);
    expect(view.stats.errorsToday).toBe(0);
    expect(view.activityDays).toEqual([{
      date: "2026-07-10",
      sessions: 1,
      minutes: 0,
      tokens: 0,
      tools: 0,
    }]);
    expect(view.tools).toEqual([]);
    expect(view.trend).toEqual([]);
  });

  it("counts the distinct agents evidenced by request logs without forcing a minimum", () => {
    const view = overviewFromPayload({
      source: "dtoken-owned-runtime",
      desktop_data_source: "dtoken-local-service",
      request_logs: [
        { agent: "OpenCode" },
        { agent: "OpenCode" },
        { agent: "Cline" },
      ],
    }, "runtime");

    expect(view.stats.activeAgents).toBe(2);
  });

  it("shows physical reduction separately from zero claimable attribution", () => {
    dashboard.state = {
      payload: {
        source: "dtoken-owned-runtime",
        desktop_data_source: "dtoken-local-service",
        request_logs: [
          { agent: "OpenCode", input_tokens_before: 3_500_000, input_tokens_after: 3_360_000, tokens_saved: 0 },
          { agent: "OpenCode", input_tokens_before: 3_481_750, input_tokens_after: 3_342_480, tokens_saved: 0 },
        ],
        stats: { tokens: { saved: 0 } },
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Overview setPage={vi.fn()} />);
    const kpis = screen.getByTestId("overview-kpis");
    expect(within(kpis).getByText("279.3k")).toBeVisible();
    expect(within(kpis).getByText("4.0%")).toBeVisible();
    expect(within(kpis).getByText("按压缩前后值计算；账单影响待确认。")).toBeVisible();
  });

  it("uses durable aggregate token totals instead of the truncated recent-request window", () => {
    const view = overviewFromPayload({
      source: "dtoken-owned-runtime",
      desktop_data_source: "dtoken-local-service",
      request_logs: [
        { input_tokens_before: 100, input_tokens_after: 100, tokens_saved: 0 },
      ],
      stats: {
        requests: { total: 116, failed: 11 },
        tokens: { input: 21_865_560, input_after: 21_585_347, saved: 471 },
      },
    }, "runtime");

    expect(view.stats.requestCount).toBe(116);
    expect(view.stats.errorsToday).toBe(11);
    expect(view.stats.savedTotal).toBe(280_213);
    expect(view.stats.claimableSaved).toBe(471);
    expect(view.stats.compressionRate).toBeCloseTo(280_213 / 21_865_560);
  });

  it("puts activity directly after KPIs and keeps operational status at the bottom", () => {
    const { container } = render(<Overview setPage={vi.fn()} />);
    const sections = Array.from(container.querySelectorAll("[data-overview-section]"))
      .map((node) => node.getAttribute("data-overview-section"));

    expect(sections).toEqual(["kpis", "activity", "issues", "trend", "operational", "account-quotas"]);
    const kpis = container.querySelector('[data-overview-section="kpis"]');
    expect(kpis).not.toBeNull();
    expect(kpis).toHaveAttribute("data-surface", "metric-strip");
    expect(kpis?.children).toHaveLength(4);
    expect(kpis?.querySelector("[data-surface]")).toBeNull();
    expect(within(kpis as HTMLElement).getByText("2")).toBeVisible();
    expect(within(kpis as HTMLElement).getByText("48")).toBeVisible();
    expect(within(kpis as HTMLElement).getByText("36.9%")).toBeVisible();
    expect(within(kpis as HTMLElement).getByText("1")).toBeVisible();
    const operational = container.querySelector('[data-overview-section="operational"]');
    expect(operational).not.toBeNull();
    expect(within(operational as HTMLElement).getByText("本地服务")).toBeVisible();
    expect(within(operational as HTMLElement).getByText("已连接 Agent")).toBeVisible();
    expect(within(operational as HTMLElement).getByText("当前连接路径")).toBeVisible();
    expect(within(operational as HTMLElement).getByText("cc-switch 与自定义上游")).toBeVisible();
    expect(within(operational as HTMLElement).queryByText("建议下一步")).not.toBeInTheDocument();
    expect(within(operational as HTMLElement).queryAllByRole("button")).toHaveLength(0);
    const issues = container.querySelector('[data-overview-section="issues"]');
    expect(within(issues as HTMLElement).getByText("最近问题")).toBeVisible();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(screen.getByTestId("activity-heatmap")).toBeVisible();
  });

  it("shows only runtime-backed Claude and Codex quota windows", () => {
    const quotas = deriveAccountQuotaSummary({
      account_quotas: {
        claude: {
          source: "passive-runtime-metadata",
          captured_at: new Date().toISOString(),
          five_hour: { utilization_pct: 42, resets_at: "2026-07-15T14:00:00Z" },
          seven_day: { utilization_pct: 65 },
        },
        codex: {
          source: "passive-runtime-metadata",
          captured_at: new Date().toISOString(),
          account_ref: "codex-account-7b21",
          primary: { used_percent: 20, window_minutes: 300 },
          secondary: { used_percent: 35, window_minutes: 10080 },
        },
      },
    }, "verified", "zh");

    expect(quotas[0].windows.map((window) => [window.label, window.usedPercent])).toEqual([["5 小时", 20], ["7 日", 35]]);
    expect(quotas[0].accountRef).toBe("codex-account-7b21");
    expect(quotas[1].windows.map((window) => [window.label, window.usedPercent])).toEqual([["5 小时", 42], ["7 日", 65]]);
    expect(deriveAccountQuotaSummary(null, "unavailable", "zh").every((quota) => quota.state === "unavailable")).toBe(true);
  });

  it("renders Codex and Claude 5-hour and 7-day account quota cards", () => {
    dashboard.state = {
      ...dashboard.state,
      payload: {
        ...runtimePayload,
        account_quotas: {
          codex: { account_ref: "codex-account-7b21", source: "passive-runtime-metadata", captured_at: new Date().toISOString(), primary: { used_percent: 20, window_minutes: 300 }, secondary: { used_percent: 35, window_minutes: 10080 } },
          claude: { account_ref: "claude-account-018c", source: "passive-runtime-metadata", captured_at: new Date().toISOString(), five_hour: { utilization_pct: 42 }, seven_day: { utilization_pct: 65 } },
        },
      },
    };

    render(<Overview setPage={vi.fn()} />);
    const quota = screen.getByTestId("overview-account-quotas");
    expect(within(quota).getByText("codex-account-7b21")).toBeVisible();
    expect(within(quota).getByText("claude-account-018c")).toBeVisible();
    expect(within(quota).getAllByText("5 小时")).toHaveLength(2);
    expect(within(quota).getAllByText("7 日")).toHaveLength(2);
    expect(within(quota).getByText("20%")).toBeVisible();
    expect(within(quota).getByText("65%")).toBeVisible();
  });

  it("rejects stale, unknown-source, and out-of-range quota evidence", () => {
    const quotas = deriveAccountQuotaSummary({
      account_quotas: {
        codex: { source: "unknown", captured_at: new Date().toISOString(), primary: { used_percent: 140, window_minutes: 300 }, secondary: { used_percent: -1, window_minutes: 10080 } },
        claude: { source: "passive-runtime-metadata", captured_at: "2020-01-01T00:00:00Z", five_hour: { utilization_pct: 42 }, seven_day: { utilization_pct: 65 } },
      },
    }, "verified", "zh");
    expect(quotas[0].state).toBe("unavailable");
    expect(quotas[1].state).toBe("stale");
  });

  it("marks an account partial when only one quota window is available", () => {
    const quotas = deriveAccountQuotaSummary({
      account_quotas: {
        codex: {
          source: "codex-wham-usage",
          captured_at: new Date().toISOString(),
          primary: { used_percent: 20, window_minutes: 300 },
        },
      },
    }, "verified", "zh");

    expect(quotas[0].state).toBe("partial");
    expect(quotas[0].windows.map((window) => window.usedPercent)).toEqual([20, null]);
  });
});

describe("Overview savings windows", () => {
  const basePayload = {
    source: "dtoken-owned-runtime",
    desktop_data_source: "dtoken-local-service",
  };

  function logsAt(entries: Array<{ y: number; m: number; d: number; h: number; saved?: number; before?: number; after?: number }>) {
    return entries.map((entry) => ({
      timestamp_ms: new Date(entry.y, entry.m, entry.d, entry.h).getTime(),
      input_tokens_before: entry.before ?? (entry.saved !== undefined ? entry.saved + 400 : undefined),
      input_tokens_after: entry.after ?? (entry.saved !== undefined ? 400 : undefined),
    }));
  }

  it("attributes cumulative, week, month and day windows by local calendar boundaries", () => {
    // 2026-03-15 是周日：当周起点为 3/9（周一），当月起点为 3/1。
    const now = new Date(2026, 2, 15, 12, 0, 0);
    const derivation = deriveSavingsWindows({
      ...basePayload,
      request_logs: logsAt([
        { y: 2026, m: 2, d: 15, h: 9, saved: 300 },  // 当天
        { y: 2026, m: 2, d: 10, h: 9, saved: 200 },  // 当周（>= 3/9）
        { y: 2026, m: 2, d: 3, h: 9, saved: 100 },   // 当月但非当周
        { y: 2026, m: 1, d: 28, h: 9, saved: 50 },   // 仅累计
      ]),
      stats: {},
    }, "verified", "zh", now);

    const byKey = Object.fromEntries(derivation.windows.map((window) => [window.key, window]));
    expect(byKey.day.tokensSaved).toBe(300);
    expect(byKey.day.requestCount).toBe(1);
    expect(byKey.week.tokensSaved).toBe(500);
    expect(byKey.week.requestCount).toBe(2);
    expect(byKey.month.tokensSaved).toBe(600);
    expect(byKey.month.requestCount).toBe(3);
    expect(byKey.total.tokensSaved).toBe(650);
    expect(byKey.total.requestCount).toBe(4);
    for (const window of derivation.windows) expect(window.state).toBe("verified");
  });

  it("keeps cross-month and cross-year logs out of the current month and week", () => {
    // 2026-01-15 是周四：当周起点为 1/12（周一）。
    const now = new Date(2026, 0, 15, 12, 0, 0);
    const derivation = deriveSavingsWindows({
      ...basePayload,
      request_logs: logsAt([
        { y: 2025, m: 11, d: 31, h: 9, saved: 40 }, // 去年
        { y: 2026, m: 0, d: 10, h: 9, saved: 30 },  // 本月非当周（周六）
        { y: 2026, m: 0, d: 14, h: 9, saved: 20 },  // 本周非当天
        { y: 2026, m: 0, d: 15, h: 9, saved: 10 },  // 当天
      ]),
      stats: {},
    }, "verified", "zh", now);

    const byKey = Object.fromEntries(derivation.windows.map((window) => [window.key, window]));
    expect(byKey.day.tokensSaved).toBe(10);
    expect(byKey.week.tokensSaved).toBe(30);
    expect(byKey.month.tokensSaved).toBe(60);
    expect(byKey.total.tokensSaved).toBe(100);
    expect(byKey.day.state).toBe("verified");
  });

  it("accepts second-granularity timestamps and rejects missing timestamp evidence", () => {
    const now = new Date(2026, 2, 15, 12, 0, 0);
    const derivation = deriveSavingsWindows({
      ...basePayload,
      request_logs: [
        { timestamp_ms: Math.floor(new Date(2026, 2, 15, 9).getTime() / 1000), input_tokens_before: 500, input_tokens_after: 100 },
        { timestamp_ms: "invalid", input_tokens_before: 10, input_tokens_after: 0 },
        { input_tokens_before: 20, input_tokens_after: 10 },
      ],
      stats: {},
    }, "verified", "zh", now);

    const byKey = Object.fromEntries(derivation.windows.map((window) => [window.key, window]));
    expect(byKey.day.tokensSaved).toBe(400);
    expect(byKey.day.requestCount).toBe(1);
    expect(byKey.week.tokensSaved).toBe(400);
    expect(byKey.total.tokensSaved).toBe(420);
    expect(byKey.total.state).toBe("verified");
  });

  it("shows unavailable for every window without verified evidence", () => {
    for (const evidence of ["preview", "loading", "unavailable", "partial"] as const) {
      const derivation = deriveSavingsWindows(null, evidence, "zh");
      expect(derivation.available).toBe(false);
      for (const window of derivation.windows) {
        expect(window.state).toBe("unavailable");
        expect(window.tokensSaved).toBeNull();
        expect(window.requestCount).toBeNull();
      }
    }
  });

  it("treats a verified empty collection as real zero, not unavailable", () => {
    const derivation = deriveSavingsWindows({
      ...basePayload,
      request_logs: [],
      stats: { requests: { total: 0 } },
    }, "verified", "zh");

    const byKey = Object.fromEntries(derivation.windows.map((window) => [window.key, window]));
    for (const key of ["total", "week", "month", "day"] as const) {
      expect(byKey[key].state).toBe("verified");
      expect(byKey[key].tokensSaved).toBe(0);
      expect(byKey[key].requestCount).toBe(0);
    }
  });

  it("keeps the cumulative window on durable aggregates over the recent-request window", () => {
    const derivation = deriveSavingsWindows({
      ...basePayload,
      request_logs: [{ input_tokens_before: 100, input_tokens_after: 100, tokens_saved: 0 }],
      stats: { tokens: { input: 21_865_560, input_after: 21_585_347, saved: 471 } },
    }, "verified", "zh");

    const total = derivation.windows.find((window) => window.key === "total");
    expect(total?.tokensSaved).toBe(280_213);
    expect(total?.state).toBe("verified");
    expect(total?.note).toContain("持久聚合");
  });

  it("marks partial windows when only some requests carry before/after values", () => {
    const now = new Date(2026, 2, 15, 12, 0, 0);
    const derivation = deriveSavingsWindows({
      ...basePayload,
      request_logs: logsAt([
        { y: 2026, m: 2, d: 15, h: 9, saved: 100 },
        { y: 2026, m: 2, d: 15, h: 10 }, // 缺 before/after
      ]),
      stats: {},
    }, "verified", "zh", now);

    const day = derivation.windows.find((window) => window.key === "day");
    expect(day?.state).toBe("partial");
    expect(day?.tokensSaved).toBe(100);
    expect(day?.requestCount).toBe(2);
  });

  it("never fabricates a cost delta without a trusted pricing source", () => {
    const zh = deriveSavingsWindows(basePayload, "verified", "zh");
    const en = deriveSavingsWindows(basePayload, "verified", "en");
    expect(zh.cost.available).toBe(false);
    expect(zh.cost.detail).toContain("价格");
    expect(en.cost.detail).toContain("pricing");
    expect(zh.windows.map((window) => window.label)).toEqual(["累计", "当周", "当月", "当天"]);
    expect(en.windows.map((window) => window.label)).toEqual(["Total", "This week", "This month", "Today"]);
  });

  it("renders the savings window card with a segmented control and switches windows", () => {
    const today = new Date();
    const todayMidnight = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
    const yesterdayMidnight = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1).getTime();
    dashboard.state = {
      payload: {
        ...runtimePayload,
        request_logs: [
          { agent: "OpenCode", timestamp_ms: todayMidnight, input_tokens_before: 500, input_tokens_after: 200 },
          { agent: "OpenCode", timestamp_ms: yesterdayMidnight, input_tokens_before: 300, input_tokens_after: 200 },
        ],
        stats: {},
      },
      status: "runtime",
      updatedAt: new Date(),
      error: "",
      refresh: vi.fn(),
    };

    render(<Overview setPage={vi.fn()} />);
    const card = screen.getByTestId("overview-savings-window");
    expect(card).toHaveAttribute("data-savings-window", "total");
    expect(within(card).getByText("节省 Token")).toBeVisible();
    expect(screen.getByRole("radiogroup", { name: "节省时间区间" })).toBeVisible();

    fireEvent.click(screen.getByRole("radio", { name: "当天" }));
    expect(card).toHaveAttribute("data-savings-window", "day");
    expect(card).toHaveAttribute("data-savings-state", "verified");
    expect(within(card).getByText("300")).toBeVisible();
  });

  it("shows em-dash and unavailable state for savings in browser preview", () => {
    dashboard.state = {
      payload: null,
      status: "browser-preview",
      updatedAt: null,
      error: "",
      refresh: vi.fn(),
    };
    render(<Overview setPage={vi.fn()} />);
    const card = screen.getByTestId("overview-savings-window");
    expect(card).toHaveAttribute("data-savings-state", "unavailable");
    expect(within(card).getByText("—")).toBeVisible();
    expect(within(card).queryByText("0")).not.toBeInTheDocument();
  });

  it("honors prefers-reduced-motion on the savings segmented control", () => {
    vi.stubGlobal("matchMedia", vi.fn((query: string) => ({
      matches: query.includes("prefers-reduced-motion"),
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })));
    try {
      render(<Overview setPage={vi.fn()} />);
      const control = screen.getByRole("radiogroup", { name: "节省时间区间" });
      expect(control).toHaveAttribute("data-reduced-motion", "true");
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("shows an estimated cost delta when a pricing cache is loaded", async () => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    pricingMock.loadModelPriceCache.mockResolvedValue(
      buildPriceCache(
        [
          { model: "gpt-4o", currency: "USD", inputPerMillion: 2.5, outputPerMillion: 10, source: "user", updatedAt: "unix:1" },
        ],
        { source: "user-settings", freshness: "fresh", checkedAt: "unix:9" },
      ),
    );
    dashboard.state = {
      payload: {
        ...runtimePayload,
        request_logs: [{ agent: "OpenCode", route_id: "route-main", model: "gpt-4o", input_tokens_before: 1_000_000, input_tokens_after: 800_000 }],
        stats: { tokens: { input: 1_000_000, input_after: 800_000 } },
      },
      status: "runtime",
      updatedAt: new Date(),
      error: "",
      refresh: vi.fn(),
    };
    try {
      render(<Overview setPage={vi.fn()} />);
      const card = screen.getByTestId("overview-savings-window");
      await waitFor(() => expect(card.textContent).toContain("$0.50（估算）"));
      expect(pricingMock.loadModelPriceCache).toHaveBeenCalledTimes(1);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("keeps the em-dash cost delta when no pricing cache is available", async () => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    try {
      render(<Overview setPage={vi.fn()} />);
      const card = screen.getByTestId("overview-savings-window");
      await waitFor(() => expect(pricingMock.loadModelPriceCache).toHaveBeenCalled());
      expect(card.textContent).toContain("价格证据缺失");
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("keeps the em-dash cost delta when the native pricing read is unavailable", () => {
    // 不 stub __TAURI_INTERNALS__：native 不可用，异步读取不会发起。
    render(<Overview setPage={vi.fn()} />);
    const card = screen.getByTestId("overview-savings-window");
    expect(pricingMock.loadModelPriceCache).not.toHaveBeenCalled();
    expect(card.textContent).toContain("价格证据缺失");
  });
});
