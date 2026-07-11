import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
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

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => dashboard.state,
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

import { Overview, deriveOverviewOperationalSummary, overviewFromPayload } from "./Overview";

const runtimePayload = {
  source: "dtoken-owned-runtime",
  desktop_data_source: "dtoken-local-service",
  providers: [{ id: "provider-main", name: "本地主上游", kind: "custom" }],
  routes: [{ id: "route-main", name: "主连接", target_provider_id: "provider-main" }],
  instances: [{ id: "instance-opencode", agent: "OpenCode", route_id: "route-main" }],
  request_logs: [
    { agent: "OpenCode", route_id: "route-main", tokens_saved: 48, input_tokens_before: 100 },
    { agent: "OpenCode", route_id: "route-main", error: "provider_timeout", tokens_saved: 0, input_tokens_before: 30 },
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
});

afterEach(cleanup);

describe("Overview information hierarchy and truthfulness", () => {
  it("derives service, agent, route and issue status from runtime evidence", () => {
    const summary = deriveOverviewOperationalSummary(runtimePayload, "runtime");

    expect(summary.service.label).toBe("正在运行");
    expect(summary.agents.label).toBe("OpenCode");
    expect(summary.route.label).toBe("主连接 → 本地主上游");
    expect(summary.issues.label).toBe("1 个问题");
    expect(summary.nextStep.page).toBe("doctor");
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
    expect(summary.nextStep.page).toBe("install");
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

    expect(screen.getAllByText("浏览器预览 · 未连接真实数据")).not.toHaveLength(0);
    expect(screen.queryByText("浏览器预览 · 示例数据")).not.toBeInTheDocument();
    const kpis = screen.getByTestId("overview-kpis");
    expect(within(kpis).getAllByText("—")).toHaveLength(4);
    expect(within(kpis).queryByText("0")).not.toBeInTheDocument();
  });

  it("keeps request and error KPIs unavailable when runtime exposes stats but no request logs", () => {
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
    expect(within(kpis).getByText("120")).toBeVisible();
    expect(within(kpis).getByText("25%")).toBeVisible();
    expect(within(kpis).getAllByText("—")).toHaveLength(2);
    expect(within(kpis).queryByText("0")).not.toBeInTheDocument();
    expect(screen.getAllByText("本地服务在线 · 暂无请求记录")).not.toHaveLength(0);
    expect(screen.getByText("尚无请求记录")).toBeVisible();
  });

  it("shows no numeric KPI when the verified runtime has neither traffic nor metric evidence", () => {
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
    expect(within(kpis).getAllByText("—")).toHaveLength(4);
    expect(within(kpis).queryByText("0")).not.toBeInTheDocument();
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
    expect(view.metricsAvailable).toBe(false);
    expect(view.compressionAvailable).toBe(false);
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

  it("puts verified KPIs before operational status, trends, activity and anomalies", () => {
    const { container } = render(<Overview setPage={vi.fn()} />);
    const sections = Array.from(container.querySelectorAll("[data-overview-section]"))
      .map((node) => node.getAttribute("data-overview-section"));

    expect(sections).toEqual(["kpis", "operational", "issues", "trend", "activity"]);
    const kpis = container.querySelector('[data-overview-section="kpis"]');
    expect(kpis).not.toBeNull();
    expect(within(kpis as HTMLElement).getByText("2")).toBeVisible();
    expect(within(kpis as HTMLElement).getByText("48")).toBeVisible();
    expect(within(kpis as HTMLElement).getByText("37%")).toBeVisible();
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
});
