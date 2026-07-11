import { describe, expect, it } from "vitest";

import { classifyRuntimePayload } from "./useRuntimeDashboard";

describe("runtime evidence classification", () => {
  it("accepts only the d-token-owned local runtime as live evidence", () => {
    expect(classifyRuntimePayload({
      source: "dtoken-owned-runtime",
      desktop_data_source: "dtoken-local-service",
    })).toEqual({ status: "runtime", error: "" });

    expect(classifyRuntimePayload({
      source: "browser-preview",
      desktop_data_source: "sample",
    })).toEqual({ status: "unavailable", error: "d-token 本地服务暂不可用" });
  });

  it("preserves a sanitized runtime error without inventing healthy state", () => {
    expect(classifyRuntimePayload({ desktop_service_error: "连接被拒绝" })).toEqual({
      status: "unavailable",
      error: "连接被拒绝",
    });
  });
});
