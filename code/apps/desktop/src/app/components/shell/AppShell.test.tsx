import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./Sidebar", () => ({
  Sidebar: ({ page, setPage }: { page: string; setPage: (page: "analysis-logs" | "advanced-configuration") => void }) => (
    <nav>
      <span data-testid="sidebar-page">{page}</span>
      <button type="button" onClick={() => setPage("advanced-configuration")}>打开高级配置</button>
      <button type="button" onClick={() => setPage("analysis-logs")}>打开分析</button>
    </nav>
  ),
}));
vi.mock("./Topbar", () => ({ Topbar: ({ page }: { page: string }) => <div data-testid="topbar-page">{page}</div> }));
vi.mock("./StatusBar", () => ({ StatusBar: () => null }));
vi.mock("../common/NotificationCenter", () => ({ NotificationBanner: () => null, NotificationLiveRegion: () => null }));
vi.mock("../pages/Configuration", () => ({ Configuration: () => <div data-testid="configuration-page"><h1>配置</h1></div> }));
vi.mock("../pages/AdvancedConfiguration", () => ({ AdvancedConfiguration: () => <h1>高级配置</h1> }));
vi.mock("../pages/AnalysisLogs", () => ({ AnalysisLogs: () => <div data-testid="analysis-logs-page"><h1>分析日志</h1></div> }));
vi.mock("../pages/Diagnostics", () => ({ Diagnostics: () => <h1>诊断</h1> }));
vi.mock("../pages/Settings", () => ({ Settings: () => <h1>设置</h1> }));

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

beforeEach(() => {
  window.sessionStorage.clear();
  window.localStorage.clear();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    callback(0);
    return 1;
  });
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("AppShell page transitions", () => {
  it("does not focus the restored page heading during app startup", async () => {
    stubMatchMedia(false);
    window.sessionStorage.setItem("d-token.shell.page", "codebase-memory");
    render(<AppShell />);

    const restoredHeading = await screen.findByRole("heading", { level: 1, name: "配置" });
    expect(restoredHeading).not.toHaveFocus();
    expect(restoredHeading).not.toHaveAttribute("tabindex");
    expect(window.sessionStorage.getItem("d-token.shell.page")).toBe("configuration");
  });

  it("routes the five-page destinations through the shared shell", async () => {
    stubMatchMedia(false);
    render(<AppShell />);

    expect(await screen.findByTestId("configuration-page")).toBeInTheDocument();
    const initialHeading = screen.getByRole("heading", { level: 1, name: "配置" });
    expect(initialHeading).not.toHaveFocus();
    expect(initialHeading).not.toHaveAttribute("tabindex");
    fireEvent.click(screen.getByRole("button", { name: "打开高级配置" }));

    expect(screen.getByTestId("topbar-page")).toHaveTextContent("advanced-configuration");
    const graphHeading = await screen.findByRole("heading", { level: 1, name: "高级配置" });
    expect(graphHeading).not.toHaveFocus();
    expect(graphHeading).not.toHaveAttribute("tabindex");
  });

  it("replaces page content immediately when reduced motion is requested", async () => {
    stubMatchMedia(true);
    render(<AppShell />);

    expect(await screen.findByTestId("configuration-page")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "打开分析" }));

    expect(screen.getByTestId("topbar-page")).toHaveTextContent("analysis-logs");
    expect(await screen.findByTestId("analysis-logs-page")).toBeInTheDocument();
    expect(screen.queryByTestId("configuration-page")).not.toBeInTheDocument();
    expect(document.querySelector("[data-reduced-motion='true']")).toBeInTheDocument();
  });

  it("keeps the animated page stage for the normal-motion path", async () => {
    stubMatchMedia(false);
    render(<AppShell />);

    expect(await screen.findByTestId("configuration-page")).toBeInTheDocument();
    expect(document.querySelector("[data-reduced-motion='false']")).toHaveAttribute("data-page-motion", "enter");
    fireEvent.click(screen.getByRole("button", { name: "打开分析" }));
    expect(await screen.findByTestId("analysis-logs-page")).toBeInTheDocument();
    expect(screen.queryByTestId("configuration-page")).not.toBeInTheDocument();
    expect(document.querySelector("[data-reduced-motion='false']")).toHaveAttribute("data-page-motion", "enter");
  });
});
