import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { activityLevel, buildMonthCells, buildWeekDays, buildYearCells, buildYearMonthLabelColumns, Heatmap } from "./Heatmap";

afterEach(cleanup);

describe("Heatmap runtime evidence", () => {
  it("keeps low request volume in a light absolute intensity band", () => {
    const day = (sessions: number) => ({ date: "2026-07-13", sessions, minutes: 0, tokens: 0, tools: 0 });

    expect(activityLevel(day(0))).toBe(0);
    expect(activityLevel(day(1))).toBe(1);
    expect(activityLevel(day(20))).toBe(1);
    expect(activityLevel(day(25))).toBe(2);
    expect(activityLevel(day(80))).toBe(3);
    expect(activityLevel(day(200))).toBe(4);
  });

  it("defaults to a compact real zero-data year heatmap instead of an empty placeholder", () => {
    const { container } = render(<Heatmap now={new Date(2024, 1, 15, 12)} />);

    expect(screen.getByTestId("activity-year")).toBeInTheDocument();
    expect(screen.getByText("1月")).toBeInTheDocument();
    expect(screen.getByText("12月")).toBeInTheDocument();
    expect(screen.getByText("暂无活动")).toBeInTheDocument();
    expect(screen.getByText("等待真实本地请求记录")).toBeInTheDocument();
    expect(container.querySelectorAll('[data-testid="activity-year"] .activity-date-cell')).toHaveLength(371);
    expect(screen.getByTestId("activity-year")).toHaveClass("activity-compact-calendar");
    expect(container.querySelector('[data-testid="activity-year"] .activity-compact-grid')).not.toBeNull();
    expect(container.querySelector(".activity-compact-weekdays")?.textContent).toBe("一三五日");
    expect(container.querySelector('[data-date="2024-02-29"]')).toHaveAttribute("data-level", "0");
    expect(container.querySelectorAll('[data-testid="activity-year"] .activity-date-cell[data-level="0"]')).toHaveLength(371);
    expect(container.querySelector(".activity-summary")).not.toBeNull();
    expect(screen.queryByText("rtk compression")).not.toBeInTheDocument();
    expect(screen.queryByText("3141")).not.toBeInTheDocument();
  });

  it("uses distinct week, month and year layouts with complete zero-date cells", async () => {
    const user = userEvent.setup();
    const { container } = render(<Heatmap now={new Date(2026, 2, 1, 12)} />);
    await user.click(screen.getByRole("radio", { name: "本月" }));
    const month = await screen.findByTestId("activity-month");
    expect(month.querySelectorAll(".activity-date-cell")).toHaveLength(42);
    expect(month.querySelector('[data-date="2026-02-23"]')).toHaveAttribute("data-outside", "true");

    await user.click(screen.getByRole("radio", { name: "本周" }));
    expect((await screen.findByTestId("activity-week")).querySelectorAll(".activity-week-day")).toHaveLength(7);
    expect(screen.getByTestId("activity-week")).toHaveClass("activity-week-strip");
    expect(screen.queryByTestId("activity-month")).not.toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "年" }));
    expect((await screen.findByTestId("activity-year")).querySelectorAll(".activity-year-month-label")).toHaveLength(12);
    expect(screen.queryByTestId("activity-week")).not.toBeInTheDocument();
    expect(container.querySelectorAll('[data-testid="activity-year"] .activity-date-cell[data-level="0"]')).toHaveLength(371);
    expect(container.querySelectorAll('[data-testid="activity-year"] [role="gridcell"]')).toHaveLength(371);
    expect(screen.getByTestId("activity-year")).toHaveClass("activity-compact-calendar");
  });

  it("builds local Monday-first leap-year and cross-month ranges", () => {
    const week = buildWeekDays(new Date(2024, 1, 29, 12), []);
    expect(week.map((cell) => cell.key)).toEqual([
      "2024-02-26", "2024-02-27", "2024-02-28", "2024-02-29", "2024-03-01", "2024-03-02", "2024-03-03",
    ]);
    const month = buildMonthCells(new Date(2026, 2, 1, 12), []);
    expect(month[0].key).toBe("2026-02-23");
    expect(month[month.length - 1]?.key).toBe("2026-04-05");
    const year = buildYearCells(new Date(2024, 6, 1, 12), []);
    expect(year).toHaveLength(371);
    expect(year[0].key).toBe("2024-01-01");
    expect(year[year.length - 1]?.key).toBe("2025-01-05");
    const columns = buildYearMonthLabelColumns(year, 2024);
    expect(columns).toHaveLength(12);
    expect(columns[0]).toBe(0);
    expect(columns.every((column, index) => index === 0 || column > columns[index - 1])).toBe(true);
  });

  it("keeps every range compact while retaining date and intensity semantics", async () => {
    const { container } = render(<Heatmap now={new Date(2026, 6, 12, 12)} />);

    expect(container.querySelector('[data-date="2026-07-12"]')).toHaveAttribute("aria-label", "2026-07-12 · 0 次活动");
    expect(container.querySelectorAll(".activity-legend-cell")).toHaveLength(5);
    expect(container.querySelector(".activity-date-cell")?.textContent).toBe("");

    await screen.findByTestId("activity-year");
    expect(screen.getByText("1月")).toBeInTheDocument();
    expect(screen.getByText("12月")).toBeInTheDocument();
    expect(container.querySelector('.activity-year-month-label[data-column="0"]')).toHaveTextContent("1月");
    expect(container.querySelector('[data-date="2026-07-12"]')).toHaveAttribute("data-today", "true");
  });

  it("does not generate token charts or tool rankings for empty runtime input", async () => {
    const user = userEvent.setup();
    const { container } = render(<Heatmap activityDays={[]} tools={[]} />);

    await user.click(screen.getByRole("radio", { name: "Token" }));
    expect(await screen.findByText("暂无真实请求记录")).toBeInTheDocument();
    expect(container.querySelector(".token-svg")).toBeNull();
    expect(screen.queryByText("99%")).not.toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "处理步骤" }));
    expect(await screen.findByText("暂无真实请求记录")).toBeInTheDocument();
    expect(container.querySelector(".activity-tool-row")).toBeNull();
    expect(container.querySelector(".activity-donut-ring")).toBeNull();
    expect(screen.queryByText("route_decision")).not.toBeInTheDocument();
  });

  it("preserves a recorded zero call count instead of inflating it to one", async () => {
    const user = userEvent.setup();
    const { container } = render(<Heatmap activityDays={[]} tools={[{ name: "真实工具", calls: 0 }]} />);

    await user.click(screen.getByRole("radio", { name: "处理步骤" }));

    expect(await screen.findByText("真实工具")).toBeInTheDocument();
    expect(container.querySelector<HTMLElement>(".activity-tool-bar")?.style.width).toBe("0%");
    expect(screen.queryByText("1")).not.toBeInTheDocument();
  });
});
