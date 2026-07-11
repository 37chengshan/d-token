import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Topbar } from "./Topbar";

afterEach(cleanup);

describe("Topbar", () => {
  it("keeps the page title in page content instead of duplicating it in the window bar", () => {
    render(<Topbar collapsed={false} setCollapsed={vi.fn()} />);

    expect(screen.getByRole("img", { name: "d-token" })).toBeVisible();
    expect(screen.queryByRole("heading")).not.toBeInTheDocument();
    for (const name of ["AI Agent 连接", "GitHub", "本地服务工具", "连接路径监控", "通知"]) {
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    }
  });

  it("offers only the sidebar restore control when the sidebar is collapsed", () => {
    const setCollapsed = vi.fn();
    render(<Topbar collapsed setCollapsed={setCollapsed} />);

    fireEvent.click(screen.getByRole("button", { name: "Expand sidebar" }));
    expect(setCollapsed).toHaveBeenCalledWith(false);
  });
});
