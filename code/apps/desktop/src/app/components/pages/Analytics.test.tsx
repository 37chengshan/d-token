import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { protocolId, protocolLabel } from "../../lib/protocol";


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

// 测试级隔离价格加载：Analytics 挂载时会调用 loadModelPriceCache，它内部走
// invokeNative("get_app_settings")。若不 mock，该调用会占用 native.invoke 的
// mockResolvedValueOnce 序列，吃掉本应属于 get_analytics_request_page 的第一页。
// 默认返回 null（价格未知），成本相关测试再显式 mockResolvedValue 注入缓存。
vi.mock("../../lib/analytics/pricing", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/analytics/pricing")>();
  return {
    ...actual,
    loadModelPriceCache: vi.fn(async () => null),
  };
});

vi.mock("recharts", () => ({
  ResponsiveContainer: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  BarChart: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  Bar: () => null,
  Tooltip: () => null,
  XAxis: () => null,
}));

import { Analytics } from "./Analytics";
import { buildPriceCache, loadModelPriceCache } from "../../lib/analytics/pricing";

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
  vi.mocked(loadModelPriceCache).mockReset();
  vi.mocked(loadModelPriceCache).mockResolvedValue(null);
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
    expect(screen.getByTestId("analytics-empty-state")).toHaveTextContent("暂无请求记录");
    expect(screen.queryByText("暂无压缩耗时记录")).not.toBeInTheDocument();
    expect(screen.queryByText("暂无真实请求记录")).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "模型使用" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "日志" })).toBeInTheDocument();
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
    expect(within(screen.getByTestId("analytics-model-table")).getByText("gpt-5")).toBeInTheDocument();
    expect(within(screen.getByTestId("analytics-model-table")).getByText("—")).toBeInTheDocument();
    expect(screen.getByTestId("analytics-tool-traffic")).toHaveTextContent("Codex");
    expect(screen.getByText("OpenAI")).toBeInTheDocument();
    expect(screen.getByText("provider_timeout")).toBeInTheDocument();
  });

  it("uses actual traffic share for tool bars", () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          { timestamp_ms: Date.UTC(2026, 6, 1), agent: "Codex", input_tokens_before: 100, input_tokens_after: 80 },
          { timestamp_ms: Date.UTC(2026, 6, 2), agent: "Codex", input_tokens_before: 100, input_tokens_after: 80 },
          { timestamp_ms: Date.UTC(2026, 6, 3), agent: "Codex", input_tokens_before: 100, input_tokens_after: 80 },
          { timestamp_ms: Date.UTC(2026, 6, 4), agent: "OpenCode", input_tokens_before: 100, input_tokens_after: 80 },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    const codex = screen.getByRole("meter", { name: "Codex 75.0%" });
    const opencode = screen.getByRole("meter", { name: "OpenCode 25.0%" });
    expect(codex).toHaveAttribute("aria-valuenow", "75");
    expect(codex.querySelector("[role=\"presentation\"]")).toHaveStyle({ width: "75%" });
    expect(opencode).toHaveAttribute("aria-valuenow", "25");
    expect(opencode.querySelector("[role=\"presentation\"]")).toHaveStyle({ width: "25%" });
  });

  it("labels model money as estimated savings and only prices matching displayed coverage", async () => {
    vi.mocked(loadModelPriceCache).mockResolvedValue(
      buildPriceCache(
        [{ model: "gpt-5", currency: "USD", inputPerMillion: 3, outputPerMillion: 15, source: "user-settings", updatedAt: "unix:2" }],
        { source: "user-settings", freshness: "fresh", checkedAt: "unix:9" },
      )!,
    );
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          { timestamp_ms: Date.UTC(2026, 6, 1), agent: "Codex", model: "gpt-5", input_tokens_before: 1_000, input_tokens_after: 800, tokens_saved: 200 },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    const table = screen.getByTestId("analytics-model-table");
    expect(within(table).getByText("估算节省费用")).toBeInTheDocument();
    expect(screen.queryByText("节省费用缺证据显示 —")).not.toBeInTheDocument();
    expect(await within(table).findByText("$0.000600")).toBeInTheDocument();

    cleanup();
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          { timestamp_ms: Date.UTC(2026, 6, 1), agent: "Codex", model: "gpt-5", input_tokens_before: 1_000, input_tokens_after: 800, tokens_saved: 200 },
        ],
        stats: {
          requests: { total: 2 },
          analytics: { models: [{ name: "gpt-5", requests: 2, tokens_saved: 400 }] },
        },
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    render(<Analytics />);

    const mismatchTable = screen.getByTestId("analytics-model-table");
    await waitFor(() => expect(within(mismatchTable).getByText("—")).toBeInTheDocument());
    expect(within(mismatchTable).queryByText("$0.000600")).not.toBeInTheDocument();
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
    expect(screen.getByRole("status")).toHaveTextContent("当前显示上次记录，连接恢复后会自动更新。");
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

    expect(await screen.findByText("暂时无法加载请求记录，请检查本地服务后重试。")).toBeInTheDocument();
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

    expect(await screen.findByText("正在读取请求记录…")).toBeInTheDocument();
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

    expect(await screen.findByText("暂时无法加载请求记录，请检查本地服务后重试。")).toBeInTheDocument();
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
            status: "ok",
            input_tokens_before: 1_000,
            input_tokens_after: 800,
            provider_input_tokens: 750,
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
    const logGrid = screen.getByTestId("analytics-log-grid");
    expect(within(logGrid).getByText("OpenCode")).toBeInTheDocument();
    expect(within(logGrid).getByText("1,000")).toBeInTheDocument();
    expect(within(logGrid).getByText("750")).toBeInTheDocument();
    expect(within(logGrid).getByText("1,000 → 800")).toBeInTheDocument();
    expect(within(logGrid).getByText("200")).toBeInTheDocument();
    expect(within(logGrid).getByText("100ms")).toBeInTheDocument();
    expect(within(logGrid).getByText("gpt-5")).toBeInTheDocument();
    expect(within(logGrid).getByText("provider-main")).toBeInTheDocument();
    expect(within(logGrid).getByText("完成")).toBeInTheDocument();
    expect(within(logGrid).getByText("request-abc-123")).toBeInTheDocument();
    expect(within(logGrid).getByRole("button", { name: "查看详情" })).toHaveAttribute("aria-expanded", "false");
  });
  it("merges secondary metadata into one header row without dropping evidence", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          {
            request_id: "request-merged-header",
            timestamp_ms: Date.UTC(2026, 6, 1),
            agent: "OpenCode",
            provider_id: "provider-main",
            route_id: "route-main",
            model: "gpt-5",
            input_protocol: "openai",
            status: "ok",
            input_tokens_before: 100,
            input_tokens_after: 80,
            tokens_saved: 20,
            latency_ms: 12,
          },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    const card = screen.getByText("request-merged-header").closest("article");
    expect(card).not.toBeNull();
    const cardView = within(card as HTMLElement);

    // 折叠卡片内 request id、状态、时间戳、agent、model、provider、protocol 与请求标识同处一个头部行
    const headerRow = screen.getByText("request-merged-header").parentElement;
    expect(headerRow).not.toBeNull();
    expect(headerRow).toHaveClass("flex");
    expect(within(headerRow as HTMLElement).getByText("完成")).toBeInTheDocument();
    expect(within(headerRow as HTMLElement).getByText("OpenCode")).toBeInTheDocument();
    expect(within(headerRow as HTMLElement).getByText("gpt-5")).toBeInTheDocument();
    expect(within(headerRow as HTMLElement).getByText("provider-main")).toBeInTheDocument();
    expect(within(headerRow as HTMLElement).getByText("OpenAI")).toBeInTheDocument();
    // 时间戳仍在合并行内可见（期望值按测试环境同款 locale 生成）
    const expectedTimestamp = new Date(Date.UTC(2026, 6, 1)).toLocaleString();
    expect(within(headerRow as HTMLElement).getByText(expectedTimestamp)).toBeInTheDocument();

    // 证据网格与展开按钮不受合并影响
    expect(cardView.getByText("100 → 80")).toBeInTheDocument();
    expect(cardView.getByText("route-main")).toBeInTheDocument();
    const detailButton = cardView.getByRole("button", { name: "查看详情" });
    expect(detailButton).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(detailButton);
    expect(detailButton).toHaveAttribute("aria-expanded", "true");
  });
  it("filters request logs by search, status, and provider while showing matched count", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          { request_id: "request-anthropic-ok", timestamp_ms: Date.UTC(2026, 6, 1), agent: "Codex", provider_id: "provider-a", model: "claude-sonnet", input_protocol: "anthropic", status: "ok" },
          { request_id: "request-openai-failed", timestamp_ms: Date.UTC(2026, 6, 2), agent: "OpenCode", provider_id: "provider-b", model: "gpt-5", input_protocol: "openai", status: "failed", error_type: "provider_timeout" },
          { request_id: "request-unknown", timestamp_ms: Date.UTC(2026, 6, 3), agent: "Cline", provider_id: "provider-a", model: "mystery", input_protocol: "vendor-secret" },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    expect(screen.getByText("显示 3 / 3")).toBeInTheDocument();
    const search = screen.getByRole("searchbox", { name: "搜索请求日志" });

    await userEvent.type(search, "anthropic");
    expect(screen.getByText("显示 1 / 3")).toBeInTheDocument();
    expect(within(screen.getByTestId("analytics-log-grid")).getByText("request-anthropic-ok")).toBeInTheDocument();
    expect(screen.queryByText("request-openai-failed")).not.toBeInTheDocument();

    await userEvent.clear(search);
    await userEvent.type(search, "vendor-secret");
    expect(screen.getByText("显示 1 / 3")).toBeInTheDocument();
    expect(within(screen.getByTestId("analytics-log-grid")).getByText("request-unknown")).toBeInTheDocument();

    await userEvent.clear(search);
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "状态筛选" }), "failed");
    expect(screen.getByText("显示 1 / 3")).toBeInTheDocument();
    expect(within(screen.getByTestId("analytics-log-grid")).getByText("request-openai-failed")).toBeInTheDocument();

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "状态筛选" }), "all");
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Provider 筛选" }), "provider-a");
    expect(screen.getByText("显示 2 / 3")).toBeInTheDocument();
    expect(screen.queryByText("request-openai-failed")).not.toBeInTheDocument();
  });

  it("uses responsive cards and groups expanded evidence without converting unknown values", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [{
          request_id: "request-responsive",
          timestamp_ms: Date.UTC(2026, 6, 3),
          agent: "OpenCode",
          provider_id: "provider-main",
          route_id: "route-main",
          model: "claude-sonnet",
          input_protocol: "anthropic",
          input_tokens_before: 100,
          input_tokens_after: 80,
          provider_input_tokens: null,
          tokens_saved: null,
          context_accounting: { disposition: "active" },
        }],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    const logGrid = screen.getByTestId("analytics-log-grid");
    expect(logGrid).toHaveClass("grid");
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(within(logGrid).getAllByText("—").length).toBeGreaterThanOrEqual(2);
    expect(within(logGrid).queryByText("0")).not.toBeInTheDocument();

    const detailButton = within(logGrid).getByRole("button", { name: "查看详情" });
    fireEvent.click(detailButton);
    const detail = document.getElementById(detailButton.getAttribute("aria-controls") ?? "");
    expect(detail).not.toBeNull();
    expect(within(detail as HTMLElement).getByText("路由身份")).toBeInTheDocument();
    expect(within(detail as HTMLElement).getByText("Token 证据")).toBeInTheDocument();
    expect(within(detail as HTMLElement).getByText("Provider Token")).toBeInTheDocument();
    expect(within(detail as HTMLElement).getByText("Local Token")).toBeInTheDocument();
    expect(within(detail as HTMLElement).getByText("性能与费用")).toBeInTheDocument();
    expect(within(detail as HTMLElement).getByText("证据 Receipt")).toBeInTheDocument();
  });
  it("shows OpenAI, Anthropic, and Unknown protocols in cards and details", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          { request_id: "request-protocol-openai", timestamp_ms: Date.UTC(2026, 6, 3), agent: "Codex", input_protocol: "openai" },
          { request_id: "request-protocol-anthropic", timestamp_ms: Date.UTC(2026, 6, 4), agent: "Codex", input_protocol: "anthropic" },
          { request_id: "request-protocol-unknown", timestamp_ms: Date.UTC(2026, 6, 5), agent: "Codex", input_protocol: "vendor-secret" },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    for (const [requestId, label] of [
      ["request-protocol-openai", "OpenAI"],
      ["request-protocol-anthropic", "Anthropic"],
      ["request-protocol-unknown", "Unknown"],
    ] as const) {
      const card = screen.getByText(requestId).closest("article");
      expect(card).not.toBeNull();
      expect(within(card as HTMLElement).getByText(label)).toBeInTheDocument();
      fireEvent.click(within(card as HTMLElement).getByRole("button", { name: "查看详情" }));
      expect(within(card as HTMLElement).getAllByText(label)).toHaveLength(2);
    }
  });


  it("shows Local before, compressed after, and provider usage separately", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          {
            request_id: "request-three-values",
            timestamp_ms: Date.UTC(2026, 6, 2),
            agent: "OpenCode",
            provider_id: "provider-main",
            route_id: "route-main",
            model: "gpt-5",
            status: "ok",
            input_tokens_before: 100,
            input_tokens_after: 80,
            provider_input_tokens: 77,
            output_tokens: 10,
            tokens_saved: 20,
            latency_ms: 12,
          },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    const logGrid = screen.getByTestId("analytics-log-grid");
    // Local Token 卡片读 input_tokens_before
    expect(within(logGrid).getByText("100")).toBeInTheDocument();
    // 压缩前后卡片仍用 input_tokens_after
    expect(within(logGrid).getByText("100 → 80")).toBeInTheDocument();
    // Provider Token 卡片读 provider_input_tokens，而非回退 input_tokens_after
    expect(within(logGrid).getByText("77")).toBeInTheDocument();
    expect(within(logGrid).queryByText("80")).not.toBeInTheDocument();
    expect(within(logGrid).getByText("完成")).toBeInTheDocument();
  });

  it("never falls back to input_tokens_after when provider usage is missing", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          {
            request_id: "request-no-provider-usage",
            timestamp_ms: Date.UTC(2026, 6, 2),
            agent: "Codex",
            provider_id: "provider-main",
            route_id: "route-main",
            model: "gpt-5",
            status: "ok",
            input_tokens_before: 100,
            input_tokens_after: 80,
            tokens_saved: 20,
            latency_ms: 12,
          },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();

    const logGrid = screen.getByTestId("analytics-log-grid");
    // 压缩前后卡片仍显示 Local 压缩后值
    expect(within(logGrid).getByText("100 → 80")).toBeInTheDocument();
    // Provider Token 缺 provider_input_tokens 时必须显示 —，绝不显示 80
    expect(within(logGrid).getAllByText("—").length).toBeGreaterThanOrEqual(1);
    expect(within(logGrid).queryByText("80")).not.toBeInTheDocument();
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

    const logGrid = screen.getByTestId("analytics-log-grid");
    expect(within(logGrid).getAllByText("—").length).toBeGreaterThanOrEqual(5);
    expect(within(logGrid).queryByText("0")).not.toBeInTheDocument();
    expect(within(logGrid).getByText("失败")).toBeInTheDocument();
    expect(within(logGrid).queryByText("完成")).not.toBeInTheDocument();
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

    const logGrid = screen.getByTestId("analytics-log-grid");
    const detailButton = within(logGrid).getByRole("button", { name: "查看详情" });
    fireEvent.click(detailButton);
    const detail = document.getElementById(detailButton.getAttribute("aria-controls") ?? "");
    expect(detail).not.toBeNull();
    const detailView = within(detail as HTMLElement);

    expect(detailButton).toHaveAttribute("aria-expanded", "true");
    expect(detailView.getByText("界面（实例）")).toBeInTheDocument();
    expect(detailView.getByText("interface-main")).toBeInTheDocument();
    expect(detailView.getByText("route-main")).toBeInTheDocument();
    expect(detailView.getByText("变换证据")).toBeInTheDocument();
    expect(detailView.getByText("compact_json")).toBeInTheDocument();
    expect(detailView.getByText("drop_redundant_context")).toBeInTheDocument();
    expect(detailView.getByText("Optimization Receipt（脱敏白名单）")).toBeInTheDocument();
    expect(detailView.getByText("active")).toBeInTheDocument();
    expect(detailView.getByText("optimized")).toBeInTheDocument();
    expect(detailView.getByText("provider_reconciled_actual")).toBeInTheDocument();
    expect(detailView.getByText("provider_timeout")).toBeInTheDocument();
    expect(detailView.queryByText("sk-secret-must-not-appear")).not.toBeInTheDocument();
    expect(detailView.queryByText("full prompt must not appear")).not.toBeInTheDocument();

    fireEvent.click(detailButton);
    expect(screen.queryByText("变换证据")).not.toBeInTheDocument();
  });

  it("marks the receipt unavailable when runtime accounting is absent", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [{ request_id: "request-no-receipt", timestamp_ms: Date.UTC(2026, 6, 3), agent: "Codex", input_tokens_before: 100, input_tokens_after: 80 }],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();
    fireEvent.click(screen.getByRole("button", { name: "查看详情" }));
    expect(screen.getByText("Optimization Receipt（runtime 未合并 · 暂不可用）")).toBeInTheDocument();
    expect(screen.getAllByText("—").length).toBeGreaterThan(0);
  });

  it("expands a row to show upstream billed cost independent of the price cache, then priced before/after/difference and basis when a cache exists", async () => {
    dashboard.state = {
      payload: {
        ...runtimeIdentity,
        request_logs: [
          {
            request_id: "request-cost-evidence",
            timestamp_ms: Date.UTC(2026, 6, 3),
            agent: "OpenCode",
            provider_id: "provider-main",
            route_id: "route-main",
            model: "gpt-5",
            status: "ok",
            input_tokens_before: 1_000_000,
            input_tokens_after: 800_000,
            tokens_saved: 200_000,
            latency_ms: 12,
            provider_cost_usd: 0.001234,
            cost_usd: 0.999999,
          },
        ],
      },
      status: "runtime",
      updatedAt: new Date("2026-07-10T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    // 阶段 1：默认无价格缓存 —— 上游账单费用直接读行字段，与缓存无关；无缓存不估算
    await openLogsTab();
    fireEvent.click(screen.getByRole("button", { name: "查看详情" }));
    expect(screen.getByText("上游账单费用")).toBeInTheDocument();
    expect(screen.getByText("$0.001234")).toBeInTheDocument();
    expect(screen.getByText("价格未知")).toBeInTheDocument();
    expect(screen.queryByText("$0.600000")).not.toBeInTheDocument();
    expect(native.invoke).not.toHaveBeenCalled();

    // 阶段 2：显式注入价格缓存 —— 前/后/差额/来源齐备，上游费用仍独立展示且不触碰 native mock
    cleanup();
    vi.mocked(loadModelPriceCache).mockResolvedValue(
      buildPriceCache(
        [{ model: "gpt-5", currency: "USD", inputPerMillion: 3, outputPerMillion: 15, source: "user-settings", updatedAt: "unix:2" }],
        { source: "user-settings", freshness: "fresh", checkedAt: "unix:9" },
      )!,
    );
    await openLogsTab();
    fireEvent.click(screen.getByRole("button", { name: "查看详情" }));

    expect(await screen.findByText("$3.000000")).toBeInTheDocument();
    expect(screen.getByText("$2.400000")).toBeInTheDocument();
    expect(screen.getByText("$0.600000")).toBeInTheDocument();
    expect(screen.getByText("USD user-settings fresh")).toBeInTheDocument();
    expect(screen.getByText("$0.001234")).toBeInTheDocument();
    expect(native.invoke).not.toHaveBeenCalled();
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
  it("keeps native next-page navigation when the current bounded page has no filter matches", async () => {
    native.enabled = true;
    native.invoke
      .mockResolvedValueOnce({
        source: "dtoken-owned-runtime",
        freshness: "fresh",
        revision: 10,
        items: [{ request_id: "request-current-page", timestamp_ms: Date.UTC(2026, 6, 16), agent: "Codex", provider_id: "provider-a" }],
        returned_count: 1,
        total_count: 11,
        has_more: true,
        next_cursor: "cursor-next",
      })
      .mockResolvedValueOnce({
        source: "dtoken-owned-runtime",
        freshness: "fresh",
        revision: 10,
        items: [{ request_id: "request-next-page", timestamp_ms: Date.UTC(2026, 6, 17), agent: "Codex", provider_id: "provider-b" }],
        returned_count: 1,
        total_count: 11,
        has_more: false,
        next_cursor: null,
      });
    dashboard.state = {
      payload: { ...runtimeIdentity, desktop_revision: "revision-filter-page", slice_revisions: { runtime_summary: 10 }, request_logs: [] },
      status: "runtime",
      updatedAt: new Date("2026-07-16T12:00:00Z"),
      error: "",
      refresh: vi.fn(),
    };

    await openLogsTab();
    await screen.findByText("request-current-page");
    await userEvent.type(screen.getByRole("searchbox", { name: "搜索请求日志" }), "request-next-page");

    expect(screen.getByText("没有匹配的请求日志")).toBeInTheDocument();
    const next = screen.getByRole("button", { name: "下一页" });
    expect(next).toBeEnabled();
    fireEvent.click(next);

    expect(await screen.findByText("request-next-page")).toBeInTheDocument();
    expect(native.invoke).toHaveBeenLastCalledWith("get_analytics_request_page", { cursor: "cursor-next", limit: 10 });
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

    expect(await screen.findByText("正在读取请求记录…")).toBeInTheDocument();
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
describe("Analytics protocol adapter", () => {
  it("maps wire and legacy OpenAI plus Anthropic to canonical labels", () => {
    expect(protocolId("open_ai")).toBe("openai");
    expect(protocolId("openai")).toBe("openai");
    expect(protocolLabel("anthropic")).toBe("Anthropic");
  });

  it("fails closed for unknown and missing protocol values", () => {
    expect(protocolLabel("vendor-secret")).toBe("Unknown");
    expect(protocolId("vendor-secret")).toBeNull();
    expect(protocolLabel(undefined)).toBeNull();
  });
});

function metric(label: string) {
  const cell = screen.getByText(label).closest("[data-analytics-metric]");
  expect(cell).not.toBeNull();
  return cell as HTMLElement;
}
