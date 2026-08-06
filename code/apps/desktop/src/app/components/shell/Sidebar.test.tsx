import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "./Sidebar";
import { I18nContext } from "../../lib/i18n";
import { NAV } from "../../lib/nav";

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

beforeEach(() => {
  themeMock.theme = "light";
  themeMock.resolvedTheme = "light";
  themeMock.setTheme.mockReset();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    callback(0);
    return 1;
  });
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

describe("Sidebar", () => {
  it("keeps eight bilingual product destinations accessible and marks the current page", () => {
    render(
      <Sidebar
        page="overview"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    expect(screen.getByRole("button", { name: "总体概览" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    for (const label of [
      "总体概览",
      "安装与连接",
      "代码图谱",
      "分析",
      "视觉压缩 Beta",
      "Doctor",
      "变更与回退",
      "设置",
    ]) {
      expect(screen.getByRole("button", { name: label })).toBeVisible();
    }
    expect(
      screen.getByRole("button", { name: "视觉压缩 Beta" }).textContent,
    ).toMatch(/Beta/);
    expect(NAV.slice(0, 4).map((item) => item.id)).toEqual([
      "overview",
      "install",
      "codebase-memory",
      "analytics",
    ]);
  });

  it("uses radio keyboard semantics for the expanded light and dark control", () => {
    render(
      <Sidebar
        page="overview"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const light = screen.getByRole("radio", { name: "浅色" });
    const dark = screen.getByRole("radio", { name: "深色" });
    expect(light).toHaveAttribute("aria-checked", "true");
    expect(dark).toHaveAttribute("tabindex", "-1");

    fireEvent.keyDown(light, { key: "ArrowRight" });
    expect(themeMock.setTheme).toHaveBeenCalledWith("dark");
    expect(dark).toHaveFocus();
  });

  it("uses icon-only theme and expand controls when collapsed", () => {
    const setCollapsed = vi.fn();
    render(
      <Sidebar
        page="overview"
        setPage={vi.fn()}
        collapsed
        setCollapsed={setCollapsed}
        reducedMotion
      />,
    );

    expect(screen.queryByRole("radiogroup")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "深色" }));
    expect(themeMock.setTheme).toHaveBeenCalledWith("dark");
    fireEvent.click(screen.getByRole("button", { name: "展开侧栏" }));
    expect(setCollapsed).toHaveBeenCalledWith(false);
    expect(document.querySelector('[data-motion-id="theme"]')).toHaveAttribute(
      "data-reduced-motion",
      "true",
    );
  });

  it("shows collapsed navigation names through tooltips instead of native title only", async () => {
    render(
      <Sidebar
        page="overview"
        setPage={vi.fn()}
        collapsed
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const analytics = screen.getByRole("button", { name: "分析" });
    expect(analytics).not.toHaveAttribute("title");
    fireEvent.pointerMove(analytics);
    fireEvent.focus(analytics);
    expect(await screen.findByRole("tooltip", { name: "分析" })).toBeVisible();
  });

  it("pulses motion on click without requiring remount of the pointer target", () => {
    const setPage = vi.fn();
    render(
      <Sidebar
        page="overview"
        setPage={setPage}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const analytics = () => screen.getByRole("button", { name: "分析" });
    const token = () =>
      Number(
        analytics()
          .querySelector('[data-motion-id="analytics"]')
          ?.getAttribute("data-motion-trigger"),
      );
    const tokenBefore = token();
    fireEvent.click(analytics());
    const iconAfterClick = analytics().querySelector(
      '[data-motion-id="analytics"]',
    );

    expect(token()).toBeGreaterThan(tokenBefore);
    expect(iconAfterClick).toHaveAttribute("data-motion-play", "true");
    expect(setPage).toHaveBeenCalledWith("analytics");
  });
  it("keeps hover and focus in CSS without React engaged attributes", () => {
    render(
      <Sidebar
        page="overview"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );
    const settings = screen.getByRole("button", { name: "设置" });
    fireEvent.pointerEnter(settings);
    fireEvent.focus(settings);
    expect(settings).not.toHaveAttribute("data-nav-engaged");
    expect(
      settings.querySelector('[data-motion-id="settings"]'),
    ).toHaveAttribute("data-motion-engaged", "false");
  });

  it("uses the shared English product labels without changing the navigation model", () => {
    render(
      <I18nContext.Provider value={{ lang: "en", setLang: vi.fn() }}>
        <Sidebar
          page="analytics"
          setPage={vi.fn()}
          collapsed={false}
          setCollapsed={vi.fn()}
          reducedMotion={false}
        />
      </I18nContext.Provider>,
    );

    expect(screen.getByRole("button", { name: "Analytics" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      screen.getByRole("button", { name: "Install & Connect" }),
    ).toBeVisible();
    expect(screen.getByRole("radio", { name: "Light" })).toBeVisible();
    expect(screen.getByRole("radio", { name: "Dark" })).toBeVisible();
  });
});
