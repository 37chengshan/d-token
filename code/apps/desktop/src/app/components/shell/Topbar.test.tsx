import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Topbar } from "./Topbar";

afterEach(cleanup);

describe("Topbar", () => {
  it("does not expose decorative controls as clickable product actions", () => {
    render(<Topbar page="overview" collapsed={false} setCollapsed={vi.fn()} />);

    expect(screen.getByRole("heading", { name: "概览" })).toBeVisible();
    for (const name of ["AI Agent 连接", "GitHub", "本地服务工具", "连接路径监控", "通知"]) {
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    }
  });
});
