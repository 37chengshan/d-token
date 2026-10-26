import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
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
  it("keeps the five product destinations accessible and marks the current page", () => {
    render(
      <Sidebar
        page="configuration"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    expect(screen.getByRole("button", { name: "配置" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    for (const label of [
      "配置",
      "高级配置",
      "分析日志",
      "诊断",
      "设置",
    ]) {
      expect(screen.getByRole("button", { name: label })).toBeVisible();
    }
    expect(screen.queryByRole("button", { name: /视觉压缩/ })).toBeNull();
    expect(NAV.map((item) => item.id)).toEqual([
      "configuration",
      "advanced-configuration",
      "analysis-logs",
      "diagnostics",
    ]);
  });

  it("uses radio keyboard semantics for the expanded light and dark control", () => {
    render(
      <Sidebar
        page="configuration"
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
        page="configuration"
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
    // The .nav-motion-item parent mirrors the flag so the press scale is
    // disabled without :has() (unsupported on macOS 10.14 WKWebView).
    expect(document.querySelector(".nav-motion-item")).toHaveAttribute(
      "data-reduced-motion",
      "true",
    );
  });

  it("marks nav-motion-item parents with data-reduced-motion=false when motion is enabled", () => {
    render(
      <Sidebar
        page="configuration"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    expect(document.querySelector(".nav-motion-item")).toHaveAttribute(
      "data-reduced-motion",
      "false",
    );
  });

  it("shows collapsed navigation names through tooltips instead of native title only", async () => {
    render(
      <Sidebar
        page="configuration"
        setPage={vi.fn()}
        collapsed
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const analytics = screen.getByRole("button", { name: "分析日志" });
    expect(analytics).not.toHaveAttribute("title");
    fireEvent.pointerMove(analytics);
    fireEvent.focus(analytics);
    expect(await screen.findByRole("tooltip", { name: "分析日志" })).toBeVisible();
  });

  it("pulses motion on click without requiring remount of the pointer target", () => {
    const setPage = vi.fn();
    render(
      <Sidebar
        page="configuration"
        setPage={setPage}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const analytics = () => screen.getByRole("button", { name: "分析日志" });
    const token = () =>
      Number(
        analytics()
          .querySelector('[data-motion-id="analysis-logs"]')
          ?.getAttribute("data-motion-trigger"),
      );
    const tokenBefore = token();
    fireEvent.click(analytics());
    const iconAfterClick = analytics().querySelector(
      '[data-motion-id="analysis-logs"]',
    );

    expect(token()).toBeGreaterThan(tokenBefore);
    expect(iconAfterClick).toHaveAttribute("data-motion-play", "true");
    expect(setPage).toHaveBeenCalledWith("analysis-logs");
  });
  it("keeps hover purely CSS-driven without writing data-motion-play", () => {
    render(
      <Sidebar
        page="configuration"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );
    const settings = screen.getByRole("button", { name: "设置" });
    const icon = () => settings.querySelector('[data-motion-id="settings"]');
    expect(icon()).toHaveAttribute("data-motion-play", "false");
    fireEvent.pointerEnter(settings);
    expect(icon()).toHaveAttribute("data-motion-play", "false");
    fireEvent.pointerLeave(settings);
    expect(icon()).toHaveAttribute("data-motion-play", "false");
    fireEvent.focus(settings);
    expect(icon()).toHaveAttribute("data-motion-play", "false");
    fireEvent.blur(settings);
    expect(icon()).toHaveAttribute("data-motion-play", "false");
  });

  it("clears the click pulse after the recipe window", () => {
    vi.useFakeTimers();
    render(
      <Sidebar
        page="configuration"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );
    const analytics = () => screen.getByRole("button", { name: "分析日志" });
    const icon = () =>
      analytics().querySelector('[data-motion-id="analysis-logs"]');
    fireEvent.click(analytics());
    expect(icon()).toHaveAttribute("data-motion-play", "true");
    act(() => {
      vi.advanceTimersByTime(401); // analytics durationMs 360 + 40 margin
    });
    expect(icon()).toHaveAttribute("data-motion-play", "false");
    vi.useRealTimers();
  });

  it("uses the shared English product labels without changing the navigation model", () => {
    render(
      <I18nContext.Provider value={{ lang: "en", setLang: vi.fn() }}>
        <Sidebar
          page="analysis-logs"
          setPage={vi.fn()}
          collapsed={false}
          setCollapsed={vi.fn()}
          reducedMotion={false}
        />
      </I18nContext.Provider>,
    );

    expect(screen.getByRole("button", { name: "Analysis & Logs" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      screen.getByRole("button", { name: "Configuration" }),
    ).toBeVisible();
    expect(screen.getByRole("radio", { name: "Light" })).toBeVisible();
    expect(screen.getByRole("radio", { name: "Dark" })).toBeVisible();
  });
});
