import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invokeNative: vi.fn() }));

vi.mock("../../lib/native", () => ({
  invokeNative: (...args: unknown[]) => mocks.invokeNative(...args),
  RUNTIME_INVALIDATED_EVENT: "dtoken:runtime-invalidated",
}));

vi.mock("./Sidebar", () => ({
  Sidebar: ({ page, setPage }: { page: string; setPage: (page: "analytics") => void }) => (
    <nav>
      <span data-testid="sidebar-page">{page}</span>
      <button type="button" onClick={() => setPage("analytics")}>打开分析</button>
    </nav>
  ),
}));
vi.mock("./StatusBar", () => ({ StatusBar: () => null }));
vi.mock("../common/NotificationCenter", () => ({ NotificationBanner: () => null, NotificationLiveRegion: () => null, NotificationCenter: () => <span>通知</span> }));
vi.mock("../pages/Overview", () => ({ Overview: () => <div data-testid="overview-page"><h1>总体概览</h1></div> }));
vi.mock("../pages/Analytics", () => ({ Analytics: () => <div data-testid="analytics-page"><h1>分析</h1></div> }));
vi.mock("../pages/InstallWorkbench", () => ({ InstallWorkbench: () => <h1>安装与连接</h1> }));
vi.mock("../pages/Onboarding", () => ({
  OnboardingPage: ({
    gateState,
    onComplete,
    onDismiss,
    onStartSetup,
    onOpenOverview,
  }: {
    gateState: string;
    onComplete: () => Promise<boolean>;
    onDismiss: () => void;
    onStartSetup: () => void;
    onOpenOverview: () => void;
  }) => (
    <div data-testid="onboarding-page">
      <span data-testid="onboarding-gate">{gateState}</span>
      <button type="button" onClick={() => void onComplete()}>完成首次配置</button>
      <button type="button" onClick={onDismiss}>稍后再说</button>
      <button type="button" onClick={onStartSetup}>开始配置</button>
      <button type="button" onClick={onOpenOverview}>返回总体概览</button>
    </div>
  ),
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
    expect(screen.queryByTestId("overview-page")).not.toBeInTheDocument();
  });

  it("returns to Overview and persists the marker after completion", async () => {
    stubMatchMedia(false);
    missingMarker();
    render(<AppShell />);

    fireEvent.click(await screen.findByRole("button", { name: "完成首次配置" }));
    await waitFor(() => expect(screen.getByTestId("overview-page")).toBeInTheDocument());
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

    expect(await screen.findByTestId("overview-page")).toBeInTheDocument();
    expect(screen.queryByTestId("onboarding-page")).not.toBeInTheDocument();
  });

  it("does not lock the shell when the settings store is unreachable (browser preview)", async () => {
    stubMatchMedia(false);
    mocks.invokeNative.mockRejectedValue(new Error("native_unavailable"));
    render(<AppShell />);

    expect(await screen.findByTestId("overview-page")).toBeInTheDocument();
    expect(screen.queryByTestId("onboarding-page")).not.toBeInTheDocument();
  });

  it("reopens the wizard from the top-right entry without displacing notifications", async () => {
    stubMatchMedia(false);
    completedMarker();
    render(<AppShell />);

    await screen.findByTestId("overview-page");
    expect(screen.getByRole("button", { name: "首次配置" })).toBeInTheDocument();
    expect(screen.getByText("通知")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "首次配置" }));
    expect(await screen.findByTestId("onboarding-page")).toBeInTheDocument();
    expect(screen.getByTestId("onboarding-gate")).toHaveTextContent("completed");
    expect(screen.getByText("通知")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "返回总体概览" }));
    expect(await screen.findByTestId("overview-page")).toBeInTheDocument();
  });

  it("dismisses the forced wizard for the session without persisting a marker", async () => {
    stubMatchMedia(false);
    missingMarker();
    render(<AppShell />);

    fireEvent.click(await screen.findByRole("button", { name: "稍后再说" }));
    await waitFor(() => expect(screen.getByTestId("overview-page")).toBeInTheDocument());
    expect(mocks.invokeNative).not.toHaveBeenCalledWith("save_app_settings", expect.anything());
  });

  it("treats navigating away while the wizard is forced as an explicit later", async () => {
    stubMatchMedia(false);
    missingMarker();
    render(<AppShell />);

    await screen.findByTestId("onboarding-page");
    fireEvent.click(screen.getByRole("button", { name: "打开分析" }));

    expect(await screen.findByTestId("analytics-page")).toBeInTheDocument();
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
    await waitFor(() => expect(screen.getByTestId("overview-page")).toBeInTheDocument());

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
});
