import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Topbar } from "./Topbar";
import { StatusBar } from "./StatusBar";
import { Sidebar } from "./Sidebar";

const themeMock = vi.hoisted(() => ({
  theme: "light",
  resolvedTheme: "light",
  setTheme: vi.fn(),
}));

vi.mock("../../lib/theme-context", async () => {
  const actual = await vi.importActual<
    typeof import("../../lib/theme-context")
  >("../../lib/theme-context");
  return { ...actual, useAppearance: () => themeMock };
});

vi.mock("../../lib/useRuntimeDashboard", () => ({
  useRuntimeDashboardSelector: () => ({
    payload: null,
    status: "loading",
    operational: { service: { data: null } },
    updatedAt: null,
    error: "",
  }),
}));

beforeEach(() => {
  // 920x640 window: compact landscape shell target.
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 920 });
  Object.defineProperty(window, "innerHeight", { configurable: true, value: 640 });
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    callback(0);
    return 1;
  });
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal("ResizeObserver", ResizeObserver);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("920x640 compact shell layout", () => {
  it("wraps the status row instead of scrolling it horizontally", () => {
    const { container } = render(<StatusBar />);

    const row = container.querySelector(".shell-statusbar .flex-1");
    expect(row).toHaveClass("flex-wrap");
    expect(row).not.toHaveClass("overflow-x-auto");
    expect(row).not.toHaveClass("flex-nowrap");
    expect(container.querySelector(".shell-statusbar")).toBeInTheDocument();
  });

  it("rotates the details chevron with classes and disables it under reduced motion", () => {
    const { container } = render(<StatusBar />);

    const chevron = container.querySelector(".shell-statusbar [class*='rotate-']");
    expect(chevron).toHaveClass("rotate-180");
    expect(chevron).toHaveClass("transition-transform");
    expect(chevron).toHaveClass("motion-reduce:rotate-0");
    expect(chevron).toHaveClass("motion-reduce:transition-none");
    expect(chevron?.getAttribute("style")).toBeNull();
  });

  it("expanding the sidebar frees the topbar context slot for page content", () => {
    const { container, rerender } = render(
      <Topbar collapsed setCollapsed={vi.fn()} />,
    );
    expect(container.querySelector(".shell-topbar-context")).toBeInTheDocument();

    rerender(<Topbar collapsed={false} setCollapsed={vi.fn()} />);
    expect(container.querySelector(".shell-topbar-context")).not.toBeInTheDocument();
    expect(container.querySelector(".sr-only")).toHaveTextContent("d-token 本地连接中心");
  });

  it("marks the experimental codebase-memory entry with an accessible badge", () => {
    render(
      <Sidebar
        page="overview"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const entry = screen.getByRole("button", { name: "代码图谱 实验性" });
    expect(entry).toBeVisible();
    expect(entry.textContent).toMatch(/实验性/);
  });

  it("keeps the sidebar nav and topbar on the same compact row budget", () => {
    const { container } = render(
      <Topbar collapsed={false} setCollapsed={vi.fn()} />,
    );

    const topbar = container.querySelector(".shell-topbar");
    expect(topbar).toHaveClass("h-12");
    expect(topbar).toHaveClass("shrink-0");
    expect(topbar).toHaveClass("min-w-0");
    expect(topbar).not.toHaveClass("overflow-x-auto");
  });
});
