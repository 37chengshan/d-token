import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./Sidebar", () => ({
  Sidebar: ({ page, setPage }: { page: string; setPage: (page: "analytics" | "codebase-memory") => void }) => (
    <nav>
      <span data-testid="sidebar-page">{page}</span>
      <button type="button" onClick={() => setPage("codebase-memory")}>打开代码图谱</button>
      <button type="button" onClick={() => setPage("analytics")}>打开分析</button>
    </nav>
  ),
}));
vi.mock("./Topbar", () => ({ Topbar: ({ page }: { page: string }) => <div data-testid="topbar-page">{page}</div> }));
vi.mock("./StatusBar", () => ({ StatusBar: () => null }));
vi.mock("../common/NotificationCenter", () => ({ NotificationBanner: () => null, NotificationLiveRegion: () => null }));
vi.mock("../pages/Overview", () => ({ Overview: () => <div data-testid="overview-page"><h1>总体概览</h1></div> }));
vi.mock("../pages/Analytics", () => ({ Analytics: () => <div data-testid="analytics-page"><h1>分析</h1></div> }));
vi.mock("../pages/InstallWorkbench", () => ({ InstallWorkbench: () => <h1>安装与连接</h1> }));
vi.mock("../pages/CodebaseMemory", () => ({ CodebaseMemory: () => <h1>代码图谱</h1> }));
vi.mock("../pages/VisualCompression", () => ({ VisualCompression: () => <h1>视觉压缩</h1> }));
vi.mock("../pages/Doctor", () => ({ Doctor: () => <h1>Doctor</h1> }));
vi.mock("../pages/Changes", () => ({ Changes: () => <h1>变更与回退</h1> }));
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

    const restoredHeading = await screen.findByRole("heading", { level: 1, name: "代码图谱" });
    expect(restoredHeading).not.toHaveFocus();
    expect(restoredHeading).not.toHaveAttribute("tabindex");
  });

  it("routes the dedicated Codebase Memory destination through the shared shell", async () => {
    stubMatchMedia(false);
    render(<AppShell />);

    expect(await screen.findByTestId("overview-page")).toBeInTheDocument();
    const initialHeading = screen.getByRole("heading", { level: 1, name: "总体概览" });
    expect(initialHeading).not.toHaveFocus();
    expect(initialHeading).not.toHaveAttribute("tabindex");
    fireEvent.click(screen.getByRole("button", { name: "打开代码图谱" }));

    expect(screen.getByTestId("topbar-page")).toHaveTextContent("codebase-memory");
    const graphHeading = await screen.findByRole("heading", { level: 1, name: "代码图谱" });
    expect(graphHeading).not.toHaveFocus();
    expect(graphHeading).not.toHaveAttribute("tabindex");
  });

  it("replaces page content immediately when reduced motion is requested", async () => {
    stubMatchMedia(true);
    render(<AppShell />);

    expect(await screen.findByTestId("overview-page")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "打开分析" }));

    expect(screen.getByTestId("topbar-page")).toHaveTextContent("analytics");
    expect(await screen.findByTestId("analytics-page")).toBeInTheDocument();
    expect(screen.queryByTestId("overview-page")).not.toBeInTheDocument();
    expect(document.querySelector("[data-reduced-motion='true']")).toBeInTheDocument();
  });

  it("keeps the animated page stage for the normal-motion path", async () => {
    stubMatchMedia(false);
    render(<AppShell />);

    expect(await screen.findByTestId("overview-page")).toBeInTheDocument();
    expect(document.querySelector("[data-reduced-motion='false']")).toHaveAttribute("data-page-motion", "enter");
    fireEvent.click(screen.getByRole("button", { name: "打开分析" }));
    expect(await screen.findByTestId("analytics-page")).toBeInTheDocument();
    expect(screen.queryByTestId("overview-page")).not.toBeInTheDocument();
    expect(document.querySelector("[data-reduced-motion='false']")).toHaveAttribute("data-page-motion", "enter");
  });
});
