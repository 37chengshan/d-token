import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(cleanup);

vi.mock("../../lib/native", () => ({
  invokeNative: vi.fn().mockRejectedValue(new Error("native_unavailable")),
  isTauriRuntime: vi.fn().mockReturnValue(false),
}));

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboard: () => ({
    payload: null,
    status: "browser-preview",
    updatedAt: null,
    error: "",
    refresh: vi.fn(),
  }),
}));

import {
  assertNativeActionSucceeded,
  desktopInstallRoot,
  deriveServicePresentation,
  Install,
} from "./Install";

describe("Install", () => {
  it("stops a configuration flow when a native transaction reports failure", () => {
    expect(() =>
      assertNativeActionSucceeded({
        ok: false,
        summary: "Provider route would loop back into d-token",
        logs: [],
        tools: [],
      }),
    ).toThrow("Provider route would loop back into d-token");
  });

  it("never presents an offline service as running while an install is in progress", () => {
    expect(deriveServicePresentation(false, true)).toEqual({
      isOnline: false,
      status: "warning",
      phase: "installing",
    });
    expect(deriveServicePresentation(false, false)).toEqual({
      isOnline: false,
      status: "error",
      phase: "stopped",
    });
    expect(deriveServicePresentation(true, false)).toEqual({
      isOnline: true,
      status: "healthy",
      phase: "running",
    });
  });

  it("uses the platform app-data location instead of a Windows path on macOS", () => {
    expect(desktopInstallRoot("Mozilla/5.0 (Macintosh; Intel Mac OS X)")).toBe("~/Library/Application Support/d-token");
    expect(desktopInstallRoot("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("%LOCALAPPDATA%\\d-token");
  });

  it("presents one primary action when the local service is unavailable", async () => {
    render(<Install />);

    expect(
      await screen.findByRole("button", { name: "安装并启动 d-token" }),
    ).toBeEnabled();
    expect(screen.queryByRole("button", { name: "启动 d-token" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "一键启动并检查" })).not.toBeInTheDocument();
    expect(screen.queryByText("/usr/local/bin/codex")).not.toBeInTheDocument();
    expect(screen.getAllByText("未检测到").length).toBeGreaterThan(0);
    expect(screen.getByText("预览中不执行检测")).toBeInTheDocument();
  });
});
