import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invokeNative: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
  refresh: vi.fn(async () => {}),
  runtime: {
    payload: null as Record<string, any> | null,
    status: "runtime" as string,
    updatedAt: null as Date | null,
    error: "",
    operational: {
      service: { data: null as Record<string, any> | null, state: "runtime", source: "dtoken", freshness: "live", collectedAt: null, error: "" },
    } as Record<string, any>,
  },
}));

vi.mock("./lib/native", () => ({
  invokeNative: mocks.invokeNative,
  isTauriRuntime: mocks.isTauriRuntime,
  RUNTIME_INVALIDATED_EVENT: "dtoken:runtime-invalidated",
}));

vi.mock("./lib/useRuntimeDashboard", () => ({
  RuntimeEvidenceProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  useRuntimeDashboardSelector: (selector: (state: unknown) => unknown) =>
    selector({ ...mocks.runtime, refresh: mocks.refresh }),
  shallowEqualSlice: () => false,
}));

import TrayPanelApp from "./TrayPanelApp";

function stubMatchMedia() {
  vi.stubGlobal("matchMedia", vi.fn((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })));
}

beforeEach(() => {
  stubMatchMedia();
  // AppProviders reads app settings on mount; give it a benign default so
  // tests exercising other commands don't need to special-case it.
  mocks.invokeNative.mockImplementation(async (command: string) =>
    command === "get_app_settings" ? undefined : undefined,
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  mocks.invokeNative.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
  mocks.refresh.mockClear();
  mocks.runtime.payload = null;
  mocks.runtime.status = "runtime";
  mocks.runtime.operational = {
    service: { data: null, state: "runtime", source: "dtoken", freshness: "live", collectedAt: null, error: "" },
  };
});

describe("tray panel", () => {
  it("states the evidence source instead of showing bare numbers", async () => {
    render(<TrayPanelApp />);

    // With no runtime payload the panel must not invent metrics.
    expect(await screen.findByText("d-token")).toBeVisible();
    expect(screen.getAllByText("—").length).toBeGreaterThan(0);
  });

  it("reports metrics from the shared runtime snapshot", async () => {
    mocks.runtime.payload = {
      source: "dtoken-owned-runtime",
      desktop_data_source: "dtoken-local-service",
      desktop_revision: "rev-1",
      stats: { tokens: { input: 1000, input_after: 700 }, requests: { total: 4, failed: 0 } },
      request_logs: [
        { agent: "opencode", input_tokens_before: 500, input_tokens_after: 350, timestamp: "2026-07-26T10:00:00Z" },
      ],
    };

    render(<TrayPanelApp />);

    // 1000 -> 700 is a 300 token physical reduction at 30%.
    expect(await screen.findByText("300")).toBeVisible();
    expect(screen.getByText("30.0%")).toBeVisible();
  });

  it("keeps the service action inert when native is unavailable", async () => {
    mocks.isTauriRuntime.mockReturnValue(false);
    render(<TrayPanelApp />);

    const action = await screen.findByRole("button", { name: /启动服务|Start service/ });
    expect(action).toBeDisabled();
    // Only app-settings bootstrap runs; no lifecycle command fires without a click.
    expect(mocks.invokeNative).not.toHaveBeenCalledWith("start_dtoken_service");
    expect(mocks.invokeNative).not.toHaveBeenCalledWith("stop_dtoken_service");
  });

  it("surfaces release copy when a lifecycle command fails", async () => {
    const user = userEvent.setup();
    mocks.runtime.operational = {
      service: { data: { ok: true, state: "stopped", managed: true }, state: "runtime", source: "dtoken", freshness: "live", collectedAt: null, error: "" },
    };
    mocks.invokeNative.mockImplementation(async (command: string) =>
      command === "get_app_settings" ? undefined : { ok: false, summary: "port 18001 already in use" },
    );

    render(<TrayPanelApp />);
    await user.click(await screen.findByRole("button", { name: /启动服务|Start service/ }));

    // The raw native summary must never become the visible sentence.
    const notice = await screen.findByRole("status");
    expect(notice).toBeVisible();
    expect(notice.textContent).not.toContain("18001");
  });

  it("hands off to the control plane through the native command", async () => {
    const user = userEvent.setup();

    render(<TrayPanelApp />);
    await user.click(await screen.findByRole("button", { name: /打开控制台|Open the control plane/ }));

    expect(mocks.invokeNative).toHaveBeenCalledWith("show_control_plane");
  });
});
