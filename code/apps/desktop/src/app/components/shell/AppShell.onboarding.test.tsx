import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: (...args: unknown[]) => mocks.invokeNative(...args),
  RUNTIME_INVALIDATED_EVENT: "dtoken:runtime-invalidated",
}));

vi.mock("./Sidebar", () => ({
  Sidebar: ({ page, setPage }: { page: string; setPage: (page: "analysis-logs") => void }) => (
    <nav>
      <span data-testid="sidebar-page">{page}</span>
      <button type="button" onClick={() => setPage("analysis-logs")}>打开分析</button>
    </nav>
  ),
}));
vi.mock("./StatusBar", () => ({ StatusBar: () => null }));
vi.mock("../common/NotificationCenter", () => ({ NotificationBanner: () => null, NotificationLiveRegion: () => null, NotificationCenter: () => <span>通知</span> }));
vi.mock("../pages/Configuration", () => ({ Configuration: () => <div data-testid="configuration-page"><h1>配置</h1><span data-testid="install-agent">none</span></div> }));
vi.mock("../pages/AdvancedConfiguration", () => ({ AdvancedConfiguration: () => <h1>高级配置</h1> }));
vi.mock("../pages/AnalysisLogs", () => ({ AnalysisLogs: () => <div data-testid="analysis-logs-page"><h1>分析日志</h1></div> }));
vi.mock("../pages/Diagnostics", () => ({ Diagnostics: () => <h1>诊断</h1> }));
vi.mock("../pages/Onboarding", () => ({
  OnboardingPage: ({
    gateState,
    onComplete,
    onDismiss,
    onStartSetup,
    onOpenOverview,
    onStartService,
  }: {
    gateState: string;
    onComplete: () => Promise<boolean>;
    onDismiss: () => void;
    onStartSetup: () => void;
    onOpenOverview: () => void;
    onStartService: () => Promise<{ ok: boolean; summary: string; refreshed: boolean }>;
  }) => {
    const [serviceResult, setServiceResult] = React.useState<string>("none");
    return (
      <div data-testid="onboarding-page">
        <span data-testid="onboarding-gate">{gateState}</span>
        <button type="button" onClick={() => void onComplete()}>完成首次配置</button>
        <button type="button" onClick={onDismiss}>稍后再说</button>
        <button type="button" onClick={() => onStartSetup()}>开始配置</button>
        <button
          type="button"
          onClick={() => void onStartService().then((result) => setServiceResult(`${result.ok}:${result.refreshed}`))}
        >
          启动并检查本地服务
        </button>
        <span data-testid="service-result">{serviceResult}</span>
        <button type="button" onClick={onOpenOverview}>返回配置</button>
      </div>
    );
  },
}));

import { AppShell } from "./AppShell";

function stubMatchMedia(reducedMotion: boolean) {
  vi.stubGlobal("matchMedia", vi.fn((query: string) => ({
    matches: query.includes("prefers-reduced-motion") ? reducedMotion : false,
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })));
}

function missingMarker() {
  mocks.invokeNative.mockImplementation(async (command: string) => {
    if (command === "get_app_settings") return { state: "missing", payload: {} };
    if (command === "save_app_settings") return { ok: true, summary: "Settings saved" };
    throw new Error(`unexpected command ${command}`);
  });
}

function completedMarker() {
  mocks.invokeNative.mockImplementation(async (command: string) => {
    if (command === "get_app_settings") {
      return { state: "loaded", payload: { onboarding: { completed: true, completed_at_ms: 1, mode: "user-confirmed" } } };
    }
    throw new Error(`unexpected command ${command}`);
  });
}

/** Native store reachable plus the real service-start transaction and discovery. */
function serviceReadyNative(transactionOk: boolean) {
  mocks.invokeNative.mockImplementation(async (command: string) => {
    if (command === "get_app_settings") return { state: "missing", payload: {} };
    if (command === "save_app_settings") return { ok: true, summary: "Settings saved" };
    if (command === "run_install_transaction") {
      return transactionOk
        ? { ok: true, summary: "runtime started", logs: [], tools: [] }
        : { ok: false, summary: "transaction failed", logs: [], tools: [] };
    }
    if (command === "discover_upstream_candidates") {
      return {
        source: "dtoken-desktop-read-only-discovery",
        freshness: "unix:1",
        revision: "setup-1",
        state: "loaded",
        candidates: [],
        agent_bindings: [],
        sources: [],
        selection_required: false,
        writes_performed: false,
        privacy: "metadata only",
      };
    }
    throw new Error("native_unavailable");
  });
}

function discoveryCallCount() {
  return mocks.invokeNative.mock.calls.filter(([command]) => command === "discover_upstream_candidates").length;
}

beforeEach(() => {
  window.sessionStorage.clear();
  window.localStorage.clear();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    callback(0);
    return 1;
  });
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  mocks.invokeNative.mockReset();
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("AppShell first-run onboarding gate", () => {
  it("forces the wizard on first open when no first-config marker exists", async () => {
    stubMatchMedia(false);
    missingMarker();
    render(<AppShell />);

    expect(await screen.findByTestId("onboarding-page")).toBeInTheDocument();
    expect(screen.getByTestId("onboarding-gate")).toHaveTextContent("pending");
    expect(screen.queryByTestId("configuration-page")).not.toBeInTheDocument();
  });

  it("returns to Configuration and persists the marker after completion", async () => {
    stubMatchMedia(false);
    missingMarker();
    render(<AppShell />);

    fireEvent.click(await screen.findByRole("button", { name: "完成首次配置" }));
    await waitFor(() => expect(screen.getByTestId("configuration-page")).toBeInTheDocument());
    expect(mocks.invokeNative).toHaveBeenCalledWith("save_app_settings", expect.objectContaining({
      section: "preferences",
      payload: expect.objectContaining({ onboarding: expect.objectContaining({ completed: true }) }),
    }));
    expect(screen.queryByTestId("onboarding-page")).not.toBeInTheDocument();
  });

  it("shows the normal shell when the first-config marker is already persisted", async () => {
    stubMatchMedia(false);
    completedMarker();
    render(<AppShell />);

    expect(await screen.findByTestId("configuration-page")).toBeInTheDocument();
    expect(screen.queryByTestId("onboarding-page")).not.toBeInTheDocument();
  });

  it("does not lock the shell when the settings store is unreachable (browser preview)", async () => {
    stubMatchMedia(false);
    mocks.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    render(<AppShell />);

    expect(await screen.findByTestId("configuration-page")).toBeInTheDocument();
    expect(screen.queryByTestId("onboarding-page")).not.toBeInTheDocument();
  });

  it("reopens the wizard from the top-right entry without displacing notifications", async () => {
    stubMatchMedia(false);
    completedMarker();
    render(<AppShell />);

    await screen.findByTestId("configuration-page");
    expect(screen.getByRole("button", { name: "首次配置" })).toBeInTheDocument();
    expect(screen.getByText("通知")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "首次配置" }));
    expect(await screen.findByTestId("onboarding-page")).toBeInTheDocument();
    expect(screen.getByTestId("onboarding-gate")).toHaveTextContent("completed");
    expect(screen.getByText("通知")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "返回配置" }));
    expect(await screen.findByTestId("configuration-page")).toBeInTheDocument();
  });

  it("navigates to Configuration for the automatic batch without an Agent preselection", async () => {
    stubMatchMedia(false);
    completedMarker();
    render(<AppShell />);

    await screen.findByTestId("configuration-page");
    fireEvent.click(screen.getByRole("button", { name: "首次配置" }));
    fireEvent.click(await screen.findByRole("button", { name: "开始配置" }));

    expect(await screen.findByRole("heading", { name: "配置" })).toBeInTheDocument();
    expect(screen.getByTestId("install-agent")).toHaveTextContent("none");
  });

  it("dismisses the forced wizard for the session without persisting a marker", async () => {
    stubMatchMedia(false);
    missingMarker();
    render(<AppShell />);

    fireEvent.click(await screen.findByRole("button", { name: "稍后再说" }));
    await waitFor(() => expect(screen.getByTestId("configuration-page")).toBeInTheDocument());
    expect(mocks.invokeNative).not.toHaveBeenCalledWith("save_app_settings", expect.anything());
  });

  it("treats navigating away while the wizard is forced as an explicit later", async () => {
    stubMatchMedia(false);
    missingMarker();
    render(<AppShell />);

    await screen.findByTestId("onboarding-page");
    fireEvent.click(screen.getByRole("button", { name: "打开分析" }));

    expect(await screen.findByTestId("analysis-logs-page")).toBeInTheDocument();
    expect(screen.queryByTestId("onboarding-page")).not.toBeInTheDocument();

    // The session dismissal is not persisted: a fresh launch re-enters the wizard.
    cleanup();
    render(<AppShell />);
    expect(await screen.findByTestId("onboarding-page")).toBeInTheDocument();
  });

  it("re-enters the wizard on a fresh launch while the marker is still missing (refresh/reload recovery)", async () => {
    stubMatchMedia(false);
    missingMarker();
    const { unmount } = render(<AppShell />);

    await screen.findByTestId("onboarding-page");
    fireEvent.click(screen.getByRole("button", { name: "稍后再说" }));
    await waitFor(() => expect(screen.getByTestId("configuration-page")).toBeInTheDocument());

    unmount();
    render(<AppShell />);
    expect(await screen.findByTestId("onboarding-page")).toBeInTheDocument();
  });

  it("enters the wizard with reduced-motion honored on the stage", async () => {
    stubMatchMedia(true);
    missingMarker();
    render(<AppShell />);

    await screen.findByTestId("onboarding-page");
    const stage = document.querySelector(".shell-page-stage");
    expect(stage).toHaveAttribute("data-reduced-motion", "true");
  });

  it("starts the local service through the typed callback with a native receipt and refreshed evidence", async () => {
    stubMatchMedia(false);
    serviceReadyNative(true);
    render(<AppShell />);

    await screen.findByTestId("onboarding-page");
    expect(screen.getByTestId("service-result")).toHaveTextContent("none");
    const discoveryCallsBefore = discoveryCallCount();

    fireEvent.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    await waitFor(() => expect(screen.getByTestId("service-result")).toHaveTextContent("true:true"));
    expect(mocks.invokeNative).toHaveBeenCalledWith("run_install_transaction", expect.objectContaining({
      request: expect.objectContaining({ target: "dtoken", proxy_port: 18001, mode: "connect" }),
    }));
    // Success must include refresh evidence, not just the native receipt.
    expect(discoveryCallCount()).toBeGreaterThan(discoveryCallsBefore);
  });

  it("reports a failed service transaction without fabricating success", async () => {
    stubMatchMedia(false);
    serviceReadyNative(false);
    render(<AppShell />);

    await screen.findByTestId("onboarding-page");
    fireEvent.click(screen.getByRole("button", { name: "启动并检查本地服务" }));

    await waitFor(() => expect(screen.getByTestId("service-result")).toHaveTextContent("false:false"));
  });
});
