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

import { Overview, deriveOverviewOperationalSummary } from "./Overview";

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

  it("puts operational truth and one next step before savings and activity", () => {
    const { container } = render(<Overview setPage={vi.fn()} />);
    const sections = Array.from(container.querySelectorAll("[data-overview-section]"))
      .map((node) => node.getAttribute("data-overview-section"));

    expect(sections).toEqual(["operational", "savings", "trend", "activity"]);
    const operational = container.querySelector('[data-overview-section="operational"]');
    expect(operational).not.toBeNull();
    expect(within(operational as HTMLElement).getByText("本地服务")).toBeVisible();
    expect(within(operational as HTMLElement).getByText("已连接 Agent")).toBeVisible();
    expect(within(operational as HTMLElement).getByText("当前连接路径")).toBeVisible();
    expect(within(operational as HTMLElement).getByText("最近问题")).toBeVisible();
    expect(within(operational as HTMLElement).getByText("建议下一步")).toBeVisible();
    expect(within(operational as HTMLElement).getAllByRole("button")).toHaveLength(1);
    expect(screen.getByTestId("activity-heatmap")).toBeVisible();
  });
});
