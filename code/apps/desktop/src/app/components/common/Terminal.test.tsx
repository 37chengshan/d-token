import React from "react";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Terminal } from "./Terminal";

afterEach(cleanup);

describe("Terminal", () => {
  it("shows a truthful empty state without a fake shell", () => {
    render(<Terminal lines={[]} />);

    expect(screen.getByText("脱敏运行日志 · 0 条")).toBeVisible();
    expect(screen.getByText("暂无真实日志记录")).toBeVisible();
    expect(screen.queryByText(/tail -f|dtoken@local|logs --follow/)).not.toBeInTheDocument();
  });

  it("renders supplied runtime rows immediately", () => {
    render(
      <Terminal
        lines={[{
          id: "request-1",
          time: "12:00:00",
          type: "route",
          agent: "OpenCode",
          msg: "请求已命中 route-a",
          msgEn: "Request matched route-a",
        }]}
      />,
    );

    expect(screen.getByText("脱敏运行日志 · 1 条")).toBeVisible();
    expect(screen.getByText("请求已命中 route-a")).toBeVisible();
    expect(screen.queryByText(/tail -f|dtoken@local|logs --follow/)).not.toBeInTheDocument();
  });
});
