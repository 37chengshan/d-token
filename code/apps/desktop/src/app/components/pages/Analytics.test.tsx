import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

const native = vi.hoisted(() => ({ enabled: false, invoke: vi.fn() }));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => dashboard.state,
  useRuntimeDashboardDetail: () => dashboard.state,
  useRuntimeDashboardSelector: (selector: (state: unknown) => unknown) => selector(dashboard.state),
  shallowEqualSlice: () => false,
}));

vi.mock("../../lib/native", () => ({
  isTauriRuntime: () => native.enabled,
  invokeNative: (...args: unknown[]) => native.invoke(...args),
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
  try {
    window.sessionStorage.removeItem("d-token.analytics.tab");
  } catch {
    /* ignore */
  }
  dashboard.state = {
    payload: null,
    status: "unavailable",
    updatedAt: null,
    error: "",
    refresh: vi.fn(),
  };
  native.enabled = false;
  native.invoke.mockReset();
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
    expect(screen.getByText("暂无压缩耗时记录")).toBeInTheDocument();
    expect(screen.getAllByText("暂无真实请求记录")).not.toHaveLength(0);
  });

  it("uses durable aggregate totals even when the recent-request detail window is empty", () => {
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

    expect(metric("已处理请求")).toHaveTextContent("99");
    expect(metric("估算输入 Token")).toHaveTextContent("9,999");
    expect(metric("物理减少 Token")).toHaveTextContent("9,998");
    expect(metric("物理减少比例")).toHaveTextContent("100.0%");
    expect(metric("估算输出 Token")).toHaveTextContent("8,888");
    expect(metric("请求错误")).toHaveTextContent("0");
    expect(screen.getByText(/分布、耗时和趋势只覆盖当前返回的最近请求窗口/)).toBeInTheDocument();
    expect(screen.queryByText("777.0ms")).not.toBeInTheDocument();
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
    expect(metric("物理减少 Token")).toHaveTextContent("300");
    expect(metric("物理减少比例")).toHaveTextContent("20.0%");
    expect(metric("估算输出 Token")).toHaveTextContent("100");
    expect(metric("请求错误")).toHaveTextContent("1");
    expect(screen.getAllByText("150.0ms")).toHaveLength(2);
    expect(screen.getByText("195.0ms")).toBeInTheDocument();
    expect(screen.getAllByText("OpenCode").length).toBeGreaterThan(0);
    expect(screen.getAllByText("provider-main").length).toBeGreaterThan(0);
    expect(screen.getAllByText("route-main").length).toBeGreaterThan(0);
    expect(screen.queryByText("legacy-provider-must-not-appear")).not.toBeInTheDocument();
    expect(screen.queryByText("legacy-route-must-not-appear")).not.toBeInTheDocument();
    expect(screen.queryByText("成本节省估算")).not.toBeInTheDocument();
  });

  it("uses durable aggregate totals while keeping recent breakdowns explicitly scoped", () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [{
          timestamp_ms: Date.UTC(2026, 6, 15),
          agent: "OpenCode",
          provider_id: "provider-main",
          route_id: "route-main",
          input_tokens_before: 100,
          input_tokens_after: 100,
          output_tokens: 1,
          tokens_saved: 0,
          latency_ms: 12,
        }],
        stats: {
          requests: { total: 116, failed: 11 },
          tokens: { input: 21_865_560, input_after: 21_585_347, output: 2_129 },
          analytics: {
            coverage: { loaded_limit: 10_000, complete: false },
            models: [{ name: "gpt-5", requests: 116, tokens_saved: 280_213 }],
            protocols: [{ name: "openai", requests: 116, tokens_saved: 280_213 }],
            agents: [{ name: "Codex", requests: 116, tokens_saved: 280_213 }],
            chains: [{ name: "direct", requests: 116, tokens_saved: 280_213 }],
            errors: [{ name: "provider_timeout", requests: 11, tokens_saved: 0 }],
            daily: [{ bucket_ms: Date.UTC(2026, 6, 15), requests: 116, tokens_saved: 280_213 }],
          },
        },
        metrics: { route_performance: { routes: [{ name: "route-main", requests: 116, tokens_saved: 280_213 }], providers: [{ name: "provider-main", requests: 116, tokens_saved: 280_213 }] } },
      },
      status: "runtime",
      updatedAt: new Date("2026-07-15T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    expect(metric("已处理请求")).toHaveTextContent("116");
    expect(metric("估算输入 Token")).toHaveTextContent("21,865,560");
    expect(metric("物理减少 Token")).toHaveTextContent("280,213");
    expect(metric("物理减少比例")).toHaveTextContent("1.3%");
    expect(metric("估算输出 Token")).toHaveTextContent("2,129");
    expect(metric("请求错误")).toHaveTextContent("11");
    expect(screen.getByText(/分布、耗时和趋势只覆盖当前返回的最近请求窗口/)).toBeInTheDocument();
    expect(screen.getByText("gpt-5")).toBeInTheDocument();
    expect(screen.getByText("openai")).toBeInTheDocument();
    expect(screen.getByText("provider_timeout")).toBeInTheDocument();
  });

  it("renders persisted stale aggregates without presenting them as live", () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        evidence_state: "stale",
        freshness: "stale",
        complete: false,
        partial: true,
        runtime_available: false,
        request_logs: [{
          timestamp_ms: Date.UTC(2026, 6, 15),
          agent: "OpenCode",
          input_tokens_before: 100,
          input_tokens_after: 75,
          output_tokens: 5,
          tokens_saved: 10,
          latency_ms: 12,
        }],
        stats: {
          requests: { total: 125, failed: 20 },
          tokens: { input: 1_000, input_after: 750, output: 50 },
        },
      },
      status: "stale",
      updatedAt: new Date("2026-07-15T12:00:00Z"),
      error: "d-token local service is unavailable",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    expect(metric("已处理请求")).toHaveTextContent("125");
    expect(metric("物理减少 Token")).toHaveTextContent("250");
    expect(metric("请求错误")).toHaveTextContent("20");
    expect(screen.getAllByText(/本地数据已过期/)).not.toHaveLength(0);
    expect(screen.getByText(/不代表当前服务状态/)).toBeInTheDocument();
    expect(screen.queryByText("实时本地数据")).not.toBeInTheDocument();
  });

  it("renders Headroom-derived dimensions and bounded request pagination from runtime evidence", () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: Array.from({ length: 11 }, (_, index) => ({
          request_id: `request-${index}`,
          timestamp_ms: Date.UTC(2026, 6, index + 1),
          agent: "Codex",
          provider_id: "provider-main",
          route_id: "route-main",
          model: "gpt-5",
          input_protocol: "openai",
          project_id: "project-opaque",
          chain_mode: "direct",
          input_tokens_before: 100,
          input_tokens_after: 80,
          output_tokens: 10,
          tokens_saved: 20,
          transforms_applied: ["compact_json"],
          cache_alignment: "aligned",
          cache_read: true,
          compression_ms: 3,
          route_ms: 2,
          upstream_ttfb_ms: 40,
          end_to_end_ms: 80,
        })),
      },
      status: "runtime",
      updatedAt: new Date("2026-07-15T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    expect(screen.getAllByText("模型").length).toBeGreaterThan(0);
    expect(screen.getByText("缓存对齐与命中")).toBeInTheDocument();
    expect(screen.getByText("流水线耗时")).toBeInTheDocument();
    expect(screen.getByText("第 1 / 2 页")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "下一页" }));
    expect(screen.getByText("第 2 / 2 页")).toBeInTheDocument();
  });

  it("uses the native cursor page error_type for request status", async () => {
    native.enabled = true;
    native.invoke.mockResolvedValue({
      source: "dtoken-owned-runtime",
      freshness: "fresh",
      revision: 3,
      items: [{ request_id: "request-failed", timestamp_ms: Date.UTC(2026, 6, 16), agent: "Codex", model: "gpt-5", route_id: "route-main", tokens_saved: 0, error_type: "provider_timeout" }],
      returned_count: 1,
      total_count: 1,
      has_more: false,
      next_cursor: null,
      coverage: { loaded_limit: 10_000, complete: true },
    });
    dashboard.state = {
      payload: { ...runtimeIdentity, desktop_revision: "revision-4", slice_revisions: { runtime_summary: 4 }, request_logs: [] },
      status: "runtime",
      updatedAt: new Date("2026-07-16T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    await waitFor(() => expect(native.invoke).toHaveBeenCalledWith("get_analytics_request_page", { cursor: null, limit: 10 }));
    expect(await screen.findByText("失败")).toBeInTheDocument();
    expect(screen.queryByText("完成")).not.toBeInTheDocument();
    expect(screen.getByText(/明细源 d-token runtime · fresh · r3；摘要 r4/)).toBeInTheDocument();
    expect(screen.getByText(/两次读取间已更新/)).toBeInTheDocument();
  });

  it("shows loading while a native cursor page is in flight", async () => {
    native.enabled = true;
    native.invoke
      .mockResolvedValueOnce({
        source: "dtoken-owned-runtime",
        freshness: "fresh",
        revision: 5,
        items: [{ request_id: "request-first", timestamp_ms: Date.UTC(2026, 6, 16), agent: "Codex", model: "gpt-5", route_id: "route-main", tokens_saved: 1 }],
        returned_count: 1,
        total_count: 11,
        has_more: true,
        next_cursor: "cursor-1",
      })
      .mockImplementationOnce(() => new Promise(() => {}));
    dashboard.state = {
      payload: { ...runtimeIdentity, desktop_revision: "revision-5", slice_revisions: { runtime_summary: 5 }, request_logs: [] },
      status: "runtime",
      updatedAt: new Date("2026-07-16T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);
    const next = await screen.findByRole("button", { name: "下一页" });
    fireEvent.click(next);

    const loading = await screen.findByRole("button", { name: "加载中…" });
    expect(loading).toBeDisabled();
    expect(native.invoke).toHaveBeenLastCalledWith("get_analytics_request_page", { cursor: "cursor-1", limit: 10 });
  });

  it("reports a native pagination error and falls back to summary evidence", async () => {
    native.enabled = true;
    native.invoke.mockRejectedValue(new Error("native unavailable"));
    dashboard.state = {
      payload: { ...runtimeIdentity, desktop_revision: "revision-6", slice_revisions: { runtime_summary: 6 }, request_logs: [] },
      status: "runtime",
      updatedAt: new Date("2026-07-16T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    expect(await screen.findByText("请求分页暂不可用，已显示当前摘要快照。")).toBeInTheDocument();
  });

  it("shows initial native loading instead of claiming the request detail is empty", async () => {
    native.enabled = true;
    native.invoke.mockImplementation(() => new Promise(() => {}));
    dashboard.state = {
      payload: { ...runtimeIdentity, desktop_revision: "revision-loading", slice_revisions: { runtime_summary: 8 }, request_logs: [] },
      status: "runtime",
      updatedAt: new Date("2026-07-16T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    expect(await screen.findByText("正在加载有界请求明细…")).toBeInTheDocument();
    expect(screen.queryByText("暂无请求明细")).not.toBeInTheDocument();
  });

  it("maps the Tauri unavailable payload to service unavailable", async () => {
    native.enabled = true;
    native.invoke.mockResolvedValue({
      source: "unavailable",
      freshness: "unknown",
      items: [],
      returned_count: 0,
      total_count: 0,
      has_more: false,
      next_cursor: null,
      error: "service unavailable",
    });
    dashboard.state = {
      payload: { ...runtimeIdentity, desktop_revision: "revision-unavailable", slice_revisions: { runtime_summary: 9 }, request_logs: [] },
      status: "runtime",
      updatedAt: new Date("2026-07-16T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    expect(await screen.findByText("请求分页暂不可用，已显示当前摘要快照。")).toBeInTheDocument();
    expect(screen.queryByText(/缺少可信的 source/)).not.toBeInTheDocument();
  });

  it("recovers an invalid native cursor by returning to the first page", async () => {
    native.enabled = true;
    const firstPage = {
      source: "dtoken-owned-runtime",
      freshness: "fresh",
      revision: 7,
      items: [{ request_id: "request-first", timestamp_ms: Date.UTC(2026, 6, 16), agent: "Codex", model: "gpt-5", route_id: "route-main", tokens_saved: 1 }],
      returned_count: 1,
      total_count: 11,
      has_more: true,
      next_cursor: "cursor-expired",
    };
    native.invoke
      .mockResolvedValueOnce(firstPage)
      .mockResolvedValueOnce({ ...firstPage, items: [], returned_count: 0, has_more: false, next_cursor: null, warning: "invalid_cursor" })
      .mockResolvedValueOnce({ ...firstPage, items: [{ ...firstPage.items[0], request_id: "request-recovered", agent: "RecoveredAgent" }] });
    dashboard.state = {
      payload: { ...runtimeIdentity, desktop_revision: "revision-7", slice_revisions: { runtime_summary: 7 }, request_logs: [] },
      status: "runtime",
      updatedAt: new Date("2026-07-16T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);
    fireEvent.click(await screen.findByRole("button", { name: "下一页" }));

    expect(await screen.findByText("RecoveredAgent")).toBeInTheDocument();
    await waitFor(() => expect(native.invoke).toHaveBeenCalledTimes(3));
    expect(native.invoke).toHaveBeenLastCalledWith("get_analytics_request_page", { cursor: null, limit: 10 });
  });
});

describe("Analytics log subpage (日志)", () => {
  async function openLogsTab() {
    const user = userEvent.setup();
    render(<Analytics />);
    await user.click(screen.getByRole("tab", { name: "日志" }));
  }

  it("renders one log line per real request with all required columns", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          {
            request_id: "request-abc-123",
            timestamp_ms: Date.UTC(2026, 6, 1),
            agent: "OpenCode",
            provider_id: "provider-main",
            route_id: "route-main",
            model: "gpt-5",
            input_protocol: "openai",
            input_tokens_before: 1_000,
            input_tokens_after: 800,
            output_tokens: 80,
            tokens_saved: 200,
            transforms_applied: ["compact_json"],
            latency_ms: 100,
          },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    expect(screen.getByText("请求日志")).toBeInTheDocument();
    expect(screen.getAllByText("OpenCode").length).toBeGreaterThan(0);
    expect(screen.getByText("1,000")).toBeInTheDocument();
    expect(screen.getByText("800")).toBeInTheDocument();
    expect(screen.getByText("1,000 → 800")).toBeInTheDocument();
    expect(screen.getByText("200")).toBeInTheDocument();
    expect(screen.getByText("100ms")).toBeInTheDocument();
    expect(screen.getByText("gpt-5")).toBeInTheDocument();
    expect(screen.getByText("provider-main")).toBeInTheDocument();
    expect(screen.getByText("完成")).toBeInTheDocument();
    expect(screen.getByText("request-abc-123")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看详情" })).toHaveAttribute("aria-expanded", "false");
  });

  it("keeps missing token evidence as em-dash instead of zero-filling", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          {
            request_id: "request-no-tokens",
            timestamp_ms: Date.UTC(2026, 6, 2),
            agent: "Codex",
            provider_id: "provider-main",
            route_id: "route-main",
            model: "gpt-5",
            error: "provider_timeout",
          },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(5);
    expect(screen.queryByText("0")).not.toBeInTheDocument();
    expect(screen.getByText("失败")).toBeInTheDocument();
    expect(screen.queryByText("完成")).not.toBeInTheDocument();
  });

  it("expands a row to redacted route evidence and receipt whitelist only", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          {
            request_id: "request-detail",
            timestamp_ms: Date.UTC(2026, 6, 3),
            agent: "OpenCode",
            instance_id: "interface-main",
            provider_id: "provider-main",
            route_id: "route-main",
            model: "gpt-5",
            input_protocol: "openai",
            chain_mode: "direct",
            output_tokens: 80,
            compression_ms: 5,
            route_ms: 3,
            upstream_ttfb_ms: 40,
            end_to_end_ms: 90,
            cache_alignment: "aligned",
            transforms_applied: ["compact_json", "drop_redundant_context"],
            error_type: "provider_timeout",
            context_accounting: {
              disposition: "active",
              sent_payload: "optimized",
              upstream_state: "completed",
              attribution_basis: "provider_reconciled_actual",
              provider_reconciled_input_savings: 200,
              estimated: { signed_savings: { input_tokens: 200, total_tokens: 210 } },
              api_key: "sk-secret-must-not-appear",
              prompt: "full prompt must not appear",
            },
          },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    const detailButton = screen.getByRole("button", { name: "查看详情" });
    fireEvent.click(detailButton);

    expect(detailButton).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("界面（实例）")).toBeInTheDocument();
    expect(screen.getByText("interface-main")).toBeInTheDocument();
    expect(screen.getByText("route-main")).toBeInTheDocument();
    expect(screen.getByText("变换证据")).toBeInTheDocument();
    expect(screen.getByText("compact_json")).toBeInTheDocument();
    expect(screen.getByText("drop_redundant_context")).toBeInTheDocument();
    expect(screen.getByText("Optimization Receipt（脱敏白名单）")).toBeInTheDocument();
    expect(screen.getByText("active")).toBeInTheDocument();
    expect(screen.getByText("optimized")).toBeInTheDocument();
    expect(screen.getByText("provider_reconciled_actual")).toBeInTheDocument();
    expect(screen.getByText("provider_timeout")).toBeInTheDocument();
    expect(screen.queryByText("sk-secret-must-not-appear")).not.toBeInTheDocument();
    expect(screen.queryByText("full prompt must not appear")).not.toBeInTheDocument();

    fireEvent.click(detailButton);
    expect(screen.queryByText("变换证据")).not.toBeInTheDocument();
  });

  it("paginates the log list with the same bounded request pages", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: Array.from({ length: 11 }, (_, index) => ({
          request_id: `request-log-${index}`,
          timestamp_ms: Date.UTC(2026, 6, index + 1),
          agent: "Codex",
          provider_id: "provider-main",
          route_id: "route-main",
          model: "gpt-5",
          input_tokens_before: 100,
          input_tokens_after: 80,
          tokens_saved: 20,
          latency_ms: 12,
        })),
      },
      status: "runtime",
      updatedAt: new Date("2026-07-15T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    expect(screen.getByText("第 1 / 2 页")).toBeInTheDocument();
    expect(screen.getByText("request-log-0")).toBeInTheDocument();
    expect(screen.queryByText("request-log-10")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "下一页" }));
    expect(screen.getByText("第 2 / 2 页")).toBeInTheDocument();
    expect(screen.getByText("request-log-10")).toBeInTheDocument();
  });

  it("shows native loading before the first bounded page arrives", async () => {
    native.enabled = true;
    native.invoke.mockImplementation(() => new Promise(() => {}));
    dashboard.state = {
      payload: { ...runtimeIdentity, desktop_revision: "revision-log-loading", slice_revisions: { runtime_summary: 8 }, request_logs: [] },
      status: "runtime",
      updatedAt: new Date("2026-07-16T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    expect(await screen.findByText("正在加载有界请求明细…")).toBeInTheDocument();
  });

  it("renders an explicit empty state when the runtime has no request logs", async () => {
    dashboard.state = {
      payload: { ...runtimeIdentity, request_logs: [] },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    expect(screen.getByText("暂无请求日志")).toBeInTheDocument();
  });
});

function metric(label: string) {
  const cell = screen.getByText(label).closest("[data-analytics-metric]");
  expect(cell).not.toBeNull();
  return cell as HTMLElement;
}
