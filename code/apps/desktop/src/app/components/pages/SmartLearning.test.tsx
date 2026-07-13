import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const native = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({ invokeNative: native.invokeNative }));
vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => ({ payload: null, status: "unavailable", refresh: vi.fn() }),
}));

import { learningInputFromOwnedRuntime, SmartLearning } from "./SmartLearning";

beforeEach(() => {
  native.invokeNative.mockReset();
  native.invokeNative.mockResolvedValue({ state: "missing", payload: {} });
});

afterEach(cleanup);

describe("SmartLearning", () => {
  it("requires real runtime evidence before analysis", async () => {
    render(<SmartLearning />);
    expect(screen.getByText("只使用 d-token 聚合运行指标；不读取原始提示词、响应、源代码或凭据。")).toBeVisible();
    expect(screen.getByRole("button", { name: "分析项目" })).toBeDisabled();
  });

  it("does not fabricate candidates when runtime evidence is absent", () => {
    render(<SmartLearning />);
    expect(screen.getByText("等待真实项目与运行指标")).toBeVisible();
    expect(screen.getByText("尚无真实分析结果")).toBeVisible();
    expect(screen.queryByText("D:/D-TOKEN/d-token")).not.toBeInTheDocument();
  });

  it("uses an honest zero aggregate for an owned runtime with no requests", () => {
    const input = learningInputFromOwnedRuntime({ source: "dtoken-owned-runtime", desktop_data_source: "dtoken-local-service", request_logs: [] }, "/tmp/real-project");
    expect(input).toMatchObject({ project_id: "real-project", avg_request_tokens_7d: 0, failure_rate_7d: 0, evidence_source: "dtoken-owned-runtime-empty-or-aggregate-request-logs" });
  });
});
