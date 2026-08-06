import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Topbar } from "./Topbar";

afterEach(cleanup);

describe("Topbar", () => {
  it("keeps the page title in page content instead of duplicating it in the window bar", () => {
    const { container } = render(<Topbar collapsed={false} setCollapsed={vi.fn()} />);

    expect(screen.queryByRole("heading")).not.toBeInTheDocument();
    expect(container.querySelector(".shell-topbar-context")).toHaveTextContent("总体概览");
    expect(screen.getByRole("button", { name: "收起侧栏" })).toBeVisible();
    for (const name of ["AI Agent 连接", "GitHub", "本地服务工具", "连接路径监控"]) {
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    }
    expect(screen.getByRole("button", { name: "通知" })).toBeVisible();
  });

  it("restores the sidebar from the integrated workspace header", () => {
    const setCollapsed = vi.fn();
    render(<Topbar collapsed setCollapsed={setCollapsed} />);

    fireEvent.click(screen.getByRole("button", { name: "展开侧栏" }));
    expect(setCollapsed).toHaveBeenCalledWith(false);
  });

  it("opens the repeatable first-run setup entry without displacing the notification center", () => {
    const onOpenOnboarding = vi.fn();
    render(<Topbar collapsed={false} setCollapsed={vi.fn()} onOpenOnboarding={onOpenOnboarding} onboardingActive />);

    const entry = screen.getByRole("button", { name: "首次配置" });
    expect(entry).toHaveAttribute("aria-pressed", "true");
    expect(entry).toHaveAttribute("data-onboarding-active", "true");
    expect(screen.getByRole("button", { name: "通知" })).toBeVisible();

    fireEvent.click(entry);
    expect(onOpenOnboarding).toHaveBeenCalledTimes(1);
  });

  it("omits the setup entry when no handler is wired", () => {
    render(<Topbar collapsed={false} setCollapsed={vi.fn()} />);

    expect(screen.queryByRole("button", { name: "首次配置" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "通知" })).toBeVisible();
  });
});
