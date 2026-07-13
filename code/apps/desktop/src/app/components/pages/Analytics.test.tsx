import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const dashboard = vi.hoisted(() => ({
  state: {
    payload: null as Record<string, unknown> | null,
    status: "unavailable",
    updatedAt: null as Date | null,
    error: "",
    refresh: vi.fn(),
  },
}));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => dashboard.state,
}));

vi.mock("recharts", () => ({
  ResponsiveContainer: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  BarChart: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  Bar: () => null,
  Tooltip: () => null,
  XAxis: () => null,
}));

import { Analytics } from "./Analytics";

const runtimeIdentity = {
  source: "dtoken-owned-runtime",
  desktop_data_source: "dtoken-local-service",
};

beforeEach(() => {
  dashboard.state = {
    payload: null,
    status: "unavailable",
    updatedAt: null,
    error: "",
    refresh: vi.fn(),
  };
});

afterEach(cleanup);

describe("Analytics runtime data truthfulness", () => {
  it("renders explicit unavailable states without zero walls or production samples", () => {
    render(<Analytics />);

    expect(screen.getAllByText("d-token 本地服务暂不可用")).not.toHaveLength(0);
    const kpis = screen.getByTestId("analytics-kpis");
    expect(within(kpis).getAllByText("—")).toHaveLength(6);
    expect(screen.queryByText("$0.000")).not.toBeInTheDocument();
    expect(screen.queryByText("0 / 0 ms")).not.toBeInTheDocument();
    expect(screen.queryByText("示例数据")).not.toBeInTheDocument();
    expect(screen.getByText("暂无请求耗时记录")).toBeInTheDocument();
    expect(screen.getAllByText("暂无真实请求记录")).not.toHaveLength(0);
  });

  it("ignores stats-only values while showing verified zeroes for an empty request snapshot", () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        stats: {
          compressions: 99,
          avg_latency_ms: 777,
          tokens: { input: 9_999, input_after: 1, output: 8_888, saved: 9_998 },
          requests: { total: 99 },
        },
        request_logs: [],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    expect(screen.getAllByText("本地服务在线 · 暂无请求记录")).not.toHaveLength(0);
    const kpis = screen.getByTestId("analytics-kpis");
    expect(within(kpis).getAllByText("0")).toHaveLength(5);
    expect(within(kpis).getByText("0.0%")).toBeInTheDocument();
    expect(screen.queryByText("9,999")).not.toBeInTheDocument();
    expect(screen.queryByText("777.0ms")).not.toBeInTheDocument();
    expect(screen.queryByText("99")).not.toBeInTheDocument();
  });

  it("displays one internally consistent snapshot from real RequestLog fields", () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          {
            timestamp_ms: Date.UTC(2026, 6, 1),
            agent: "OpenCode",
            provider_id: "provider-main",
            provider: "legacy-provider-must-not-appear",
            route_id: "route-main",
            route: "legacy-route-must-not-appear",
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
            transforms_applied: ["drop_redundant_context"],
            latency_ms: 200,
            error: "provider_timeout",
          },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    expect(metric("已处理请求")).toHaveTextContent("2");
    expect(metric("估算输入 Token")).toHaveTextContent("1,500");
    expect(metric("估算节省 Token")).toHaveTextContent("300");
    expect(metric("估算节省率")).toHaveTextContent("20.0%");
    expect(metric("估算输出 Token")).toHaveTextContent("100");
    expect(metric("请求错误")).toHaveTextContent("1");
    expect(screen.getAllByText("150.0ms")).toHaveLength(2);
    expect(screen.getByText("195.0ms")).toBeInTheDocument();
    expect(screen.getByText("OpenCode")).toBeInTheDocument();
    expect(screen.getByText("provider-main")).toBeInTheDocument();
    expect(screen.getByText("route-main")).toBeInTheDocument();
    expect(screen.queryByText("legacy-provider-must-not-appear")).not.toBeInTheDocument();
    expect(screen.queryByText("legacy-route-must-not-appear")).not.toBeInTheDocument();
    expect(screen.queryByText("成本节省估算")).not.toBeInTheDocument();
  });
});

function metric(label: string) {
  const cell = screen.getByText(label).closest("[data-analytics-metric]");
  expect(cell).not.toBeNull();
  return cell as HTMLElement;
}
