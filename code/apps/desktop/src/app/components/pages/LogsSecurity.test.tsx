import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  isTauriRuntime: vi.fn(),
  invokeNative: vi.fn(),
  useRuntimeDashboard: vi.fn(),
}));

vi.mock("../../lib/native", () => ({
  isTauriRuntime: mocks.isTauriRuntime,
  invokeNative: mocks.invokeNative,
}));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: mocks.useRuntimeDashboard,
}));

import { LogsSecurity } from "./LogsSecurity";

beforeEach(() => {
  mocks.isTauriRuntime.mockReturnValue(false);
  mocks.invokeNative.mockReset();
  mocks.useRuntimeDashboard.mockReturnValue({
    payload: null,
    status: "browser-preview",
    updatedAt: null,
    error: "",
    refresh: vi.fn(),
  });
});

afterEach(cleanup);

describe("LogsSecurity", () => {
  it("does not substitute fixture logs or verified security claims in preview", async () => {
    const user = userEvent.setup();
    render(<LogsSecurity />);

    expect(screen.getByText("浏览器预览 · 无本机日志")).toBeVisible();
    expect(screen.getByText("暂无真实日志记录")).toBeVisible();
    expect(screen.getByRole("button", { name: "JSON" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "CSV" })).toBeDisabled();
    expect(screen.queryByText(/上下文压缩 12\.4k|Provider 401/)).not.toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "密钥凭据" }));
    expect(await screen.findByText("无本机证据")).toBeInTheDocument();
    expect(screen.queryByText("默认脱敏策略")).not.toBeInTheDocument();
    expect(screen.queryByText("已启用")).not.toBeInTheDocument();
  });

  it("keeps an authoritative runtime export failure visible", async () => {
    mocks.isTauriRuntime.mockReturnValue(true);
    mocks.invokeNative.mockRejectedValue(new Error("export_failed"));
    mocks.useRuntimeDashboard.mockReturnValue({
      payload: {
        source: "dtoken-owned-runtime",
        desktop_data_source: "dtoken-local-service",
        request_logs: [],
        route_decisions: [],
        events: [],
      },
      status: "runtime",
      updatedAt: null,
      error: "",
      refresh: vi.fn(),
    });

    render(<LogsSecurity />);
    fireEvent.click(screen.getByRole("button", { name: "JSON" }));

    await waitFor(() => {
      expect(screen.getByText("导出失败，请确认 d-token runtime 是否运行。")).toBeVisible();
    });
    expect(mocks.invokeNative).toHaveBeenCalledWith("export_dtoken_runtime", { format: "json" });
    expect(screen.queryByText(/已导出当前页面|预览/)).not.toBeInTheDocument();
  });
});
