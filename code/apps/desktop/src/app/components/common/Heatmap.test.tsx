import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { Heatmap } from "./Heatmap";

afterEach(cleanup);

describe("Heatmap runtime evidence", () => {
  it("defaults to an evidence-only empty state instead of fixture activity", () => {
    const { container } = render(<Heatmap />);

    expect(screen.getByText("暂无真实请求记录")).toBeInTheDocument();
    expect(screen.getByText("连接本地服务后显示")).toBeInTheDocument();
    expect(screen.getByText("等待真实本地请求记录")).toBeInTheDocument();
    expect(container.querySelector("svg")).toBeNull();
    expect(container.querySelector(".activity-summary")).toBeNull();
    expect(container.textContent).not.toMatch(/\d{1,2}\/\d{1,2}/);
    expect(screen.queryByText("rtk compression")).not.toBeInTheDocument();
    expect(screen.queryByText("3141")).not.toBeInTheDocument();
  });

  it("does not generate token charts or tool rankings for empty runtime input", async () => {
    const user = userEvent.setup();
    const { container } = render(<Heatmap activityDays={[]} tools={[]} />);

    await user.click(screen.getByRole("button", { name: "切换统计视图: Token" }));
    expect(await screen.findByText("暂无真实请求记录")).toBeVisible();
    expect(container.querySelector(".token-svg")).toBeNull();
    expect(screen.queryByText("99%")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "切换统计视图: 工具" }));
    expect(await screen.findByText("暂无真实请求记录")).toBeVisible();
    expect(container.querySelector(".activity-tool-row")).toBeNull();
    expect(container.querySelector(".activity-donut-ring")).toBeNull();
    expect(screen.queryByText("route_decision")).not.toBeInTheDocument();
  });

  it("preserves a recorded zero call count instead of inflating it to one", async () => {
    const user = userEvent.setup();
    const { container } = render(<Heatmap activityDays={[]} tools={[{ name: "真实工具", calls: 0 }]} />);

    await user.click(screen.getByRole("button", { name: "切换统计视图: 工具" }));

    expect(await screen.findByText("真实工具")).toBeInTheDocument();
    expect(container.querySelector<HTMLElement>(".activity-tool-bar")?.style.width).toBe("0%");
    expect(screen.queryByText("1")).not.toBeInTheDocument();
  });
});
