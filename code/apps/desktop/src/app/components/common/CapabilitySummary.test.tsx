import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CapabilitySummary } from "./CapabilitySummary";

afterEach(cleanup);

describe("CapabilitySummary evidence semantics", () => {
  it("does not present browser preview as healthy capability evidence", () => {
    const { container } = render(<CapabilitySummary payload={null} status="browser-preview" />);

    expect(screen.getByText("本地能力状态")).toBeInTheDocument();
    expect(screen.getAllByText("—")).toHaveLength(8);
    expect(container.querySelectorAll(".text-emerald-600")).toHaveLength(0);
    expect(screen.queryByText("ok")).not.toBeInTheDocument();
    expect(screen.queryByText("available")).not.toBeInTheDocument();
    expect(screen.queryByText("d-token-owned-runtime")).not.toBeInTheDocument();
    expect(screen.queryByText("Route adapter")).not.toBeInTheDocument();
  });

  it("fails closed when a runtime-shaped payload lacks d-token provenance", () => {
    const { container } = render(
      <CapabilitySummary
        status="runtime"
        payload={{
          source: "unknown",
          desktop_data_source: "unknown",
          stats: { requests: { total: 12 }, tokens: { saved: 300 } },
        }}
      />,
    );

    expect(screen.getByText("数据不完整")).toBeInTheDocument();
    expect(screen.queryByText("估算节省 300 Token")).not.toBeInTheDocument();
    expect(container.querySelectorAll(".text-emerald-600")).toHaveLength(0);
  });

  it("shows positive state only for fields backed by verified local evidence", () => {
    render(
      <CapabilitySummary
        status="runtime"
        payload={{
          source: "dtoken-owned-runtime",
          desktop_data_source: "dtoken-local-service",
          stats: { requests: { total: 2 }, tokens: { saved: 200 } },
          metrics: {
            compression_performance: { transforms: [{ name: "compact_json" }] },
            route_performance: { routes: [{ name: "route-main" }] },
          },
          cache_summary: { entries: 0, raw_available: false },
          memory: { entries: 1, raw_available: false },
          runtime: { rtk: "integrated" },
          dashboard_sections: ["summary"],
        }}
      />,
    );

    expect(screen.getByText("估算节省 200 Token")).toBeInTheDocument();
    expect(screen.getByText("2 条本地请求 · 1 类处理步骤")).toBeInTheDocument();
    expect(screen.getByText("产品内置能力")).toBeInTheDocument();
    expect(screen.getByText("暂无可核验的输出精简状态")).toBeInTheDocument();
    expect(screen.getAllByText("有运行记录")).toHaveLength(2);
  });
});
