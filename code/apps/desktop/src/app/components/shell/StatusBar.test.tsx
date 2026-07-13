import { describe, expect, it } from "vitest";

import { buildStatusSummary } from "./StatusBar";

describe("StatusBar runtime truthfulness", () => {
  it("does not present browser preview as a healthy diagnosed service", () => {
    const summary = buildStatusSummary(
      { source: "browser-preview", routes: [], providers: [], stats: { requests: { total: 0, failed: 0 } } },
      "browser-preview",
      null,
      "",
    );

    expect(summary.runtimeLabel).toBe("浏览器预览");
    expect(summary.routeLabel).toBe("尚未连接真实路由");
    expect(summary.portsLabel).toBe("未监听（预览）");
    expect(summary.doctorLabel).toBe("未运行诊断");
    expect(summary.doctorHealth).toBe("warning");
    expect(summary.requestCount).toBe("—");
    expect(summary.routeCount).toBe("—");
  });

  it("keeps a connected runtime without routes in a needs-configuration state", () => {
    const summary = buildStatusSummary(
      { source: "dtoken-owned-runtime", routes: [], providers: [], stats: { requests: { total: 0, failed: 0 } } },
      "runtime",
      null,
      "",
    );

    expect(summary.routeLabel).toBe("等待路由配置");
    expect(summary.doctorLabel).toBe("等待配置");
    expect(summary.doctorHealth).toBe("warning");
  });

  it("localizes preview evidence values instead of mixing Chinese into English UI", () => {
    const summary = buildStatusSummary(
      { source: "browser-preview", routes: [], providers: [], stats: { requests: { total: 0, failed: 0 } } },
      "browser-preview",
      null,
      "",
      "en",
    );

    expect(summary.runtimeLabel).toBe("Browser preview");
    expect(summary.routeLabel).toBe("No verified route");
    expect(summary.portsLabel).toBe("Not listening (preview)");
    expect(summary.modeLabel).toBe("Browser preview");
    expect(summary.doctorLabel).toBe("Not run");
    expect(summary.sourceLabel).toBe("Browser preview");
  });

  it.each([
    ["stale", "数据已过期", "路由数据已过期", "过期数据"],
    ["partial", "数据不完整", "路由数据不完整", "不完整数据"],
  ])("never promotes %s runtime evidence to a connected or healthy state", (status, runtime, route, mode) => {
    const summary = buildStatusSummary(
      {
        source: "dtoken-owned-runtime",
        desktop_data_source: "dtoken-local-service",
        routes: [{ id: "must-not-surface" }],
        stats: { requests: { total: 12, failed: 0 } },
      },
      status,
      new Date(),
      "",
    );

    expect(summary.runtimeLabel).toBe(runtime);
    expect(summary.runtimeHealth).not.toBe("healthy");
    expect(summary.routeLabel).toBe(route);
    expect(summary.modeLabel).toBe(mode);
    expect(summary.requestCount).toBe("—");
    expect(summary.routeCount).toBe("—");
    expect(summary.doctorLabel).toBe("需检查");
  });
});
