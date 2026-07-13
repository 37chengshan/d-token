import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "./Sidebar";
import { I18nContext } from "../../lib/i18n";

const themeMock = vi.hoisted(() => ({
  theme: "light",
  resolvedTheme: "light",
  setTheme: vi.fn(),
}));

vi.mock("../../lib/theme-context", async () => {
  const actual = await vi.importActual<typeof import("../../lib/theme-context")>("../../lib/theme-context");
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
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Sidebar", () => {
  it("keeps six bilingual product destinations accessible and marks the current page", () => {
    render(<Sidebar page="overview" setPage={vi.fn()} collapsed={false} setCollapsed={vi.fn()} reducedMotion={false} />);

    expect(screen.getByRole("button", { name: "总体概览" })).toHaveAttribute("aria-current", "page");
    for (const label of ["总体概览", "安装与连接", "分析", "Doctor", "变更与回退", "设置"]) {
      expect(screen.getByRole("button", { name: label })).toBeVisible();
    }
  });

  it("uses radio keyboard semantics for the expanded light and dark control", () => {
    render(<Sidebar page="overview" setPage={vi.fn()} collapsed={false} setCollapsed={vi.fn()} reducedMotion={false} />);

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
    render(<Sidebar page="overview" setPage={vi.fn()} collapsed setCollapsed={setCollapsed} reducedMotion />);

    expect(screen.queryByRole("radiogroup")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "深色" }));
    expect(themeMock.setTheme).toHaveBeenCalledWith("dark");
    fireEvent.click(screen.getByRole("button", { name: "展开侧栏" }));
    expect(setCollapsed).toHaveBeenCalledWith(false);
    expect(document.querySelector('[data-motion-id="theme"]')).toHaveAttribute("data-reduced-motion", "true");
  });

  it("remounts a navigation icon after activation without replacing the pointer target first", () => {
    const setPage = vi.fn();
    render(<Sidebar page="overview" setPage={setPage} collapsed={false} setCollapsed={vi.fn()} reducedMotion={false} />);

    const analytics = () => screen.getByRole("button", { name: "分析" });
    const token = () => Number(analytics().querySelector('[data-motion-id="analytics"]')?.getAttribute("data-motion-trigger"));
    const iconBefore = analytics().querySelector('[data-motion-id="analytics"]');
    const tokenBefore = token();
    fireEvent.click(analytics());
    const tokenAfterClick = token();
    const iconAfterClick = analytics().querySelector('[data-motion-id="analytics"]');

    expect(tokenAfterClick).toBeGreaterThan(tokenBefore);
    expect(iconAfterClick).not.toBe(iconBefore);
    expect(setPage).toHaveBeenCalledWith("analytics");
  });

  it("uses the shared English product labels without changing the navigation model", () => {
    render(
      <I18nContext.Provider value={{ lang: "en", setLang: vi.fn() }}>
        <Sidebar page="analytics" setPage={vi.fn()} collapsed={false} setCollapsed={vi.fn()} reducedMotion={false} />
      </I18nContext.Provider>,
    );

    expect(screen.getByRole("button", { name: "Analytics" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "Install & Connect" })).toBeVisible();
    expect(screen.getByRole("radio", { name: "Light" })).toBeVisible();
    expect(screen.getByRole("radio", { name: "Dark" })).toBeVisible();
  });
});
