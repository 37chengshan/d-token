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
});
