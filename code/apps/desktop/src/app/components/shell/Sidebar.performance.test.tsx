import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "./Sidebar";

// Counts AnimatedNavIcon renders per motion id so tests can prove that
// hovering/clicking one sidebar item never re-renders the other items
// (engaged + motion-token state is local to each item, not sidebar-wide).
const renderCounts: Record<string, number> = {};

vi.mock("./AnimatedNavIcon", async () => {
  const actual =
    await vi.importActual<typeof import("./AnimatedNavIcon")>(
      "./AnimatedNavIcon",
    );
  return {
    ...actual,
    AnimatedNavIcon: (props: { id: string }) => {
      renderCounts[props.id] = (renderCounts[props.id] ?? 0) + 1;
      return <svg data-testid={`icon-${props.id}`} data-motion-id={props.id} />;
    },
  };
});

beforeEach(() => {
  for (const key of Object.keys(renderCounts)) delete renderCounts[key];
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("Sidebar render isolation", () => {
  it("keeps hover in CSS without re-rendering the hovered item or siblings", () => {
    render(
      <Sidebar
        page="configuration"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const before = { ...renderCounts };
    fireEvent.pointerEnter(screen.getByRole("button", { name: "分析日志" }));
    const afterEnter = { ...renderCounts };

    expect(afterEnter["analysis-logs"]).toBe(before["analysis-logs"] ?? 0);
    expect(afterEnter.diagnostics).toBe(before.diagnostics ?? 0);
    expect(afterEnter.settings).toBe(before.settings ?? 0);

    fireEvent.pointerLeave(screen.getByRole("button", { name: "分析日志" }));
    const afterLeave = { ...renderCounts };
    expect(afterLeave["analysis-logs"]).toBe(afterEnter["analysis-logs"] ?? 0);
    expect(afterLeave.diagnostics).toBe(afterEnter.diagnostics ?? 0);
  });

  it("bumps the motion token only for the clicked item", () => {
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

    const before = { ...renderCounts };
    fireEvent.click(screen.getByRole("button", { name: "分析日志" }));

    expect(setPage).toHaveBeenCalledWith("analysis-logs");
    expect(renderCounts["analysis-logs"]).toBe((before["analysis-logs"] ?? 0) + 1);
    expect(renderCounts.diagnostics).toBe(before.diagnostics ?? 0);
    expect(renderCounts.configuration).toBe(before.configuration ?? 0);
  });

  it("keeps theme and collapse controls isolated from navigation items", () => {
    render(
      <Sidebar
        page="configuration"
        setPage={vi.fn()}
        collapsed
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const before = { ...renderCounts };
    fireEvent.pointerEnter(screen.getByRole("button", { name: "深色" }));

    expect(renderCounts.theme).toBe(before.theme ?? 0);
    expect(renderCounts.configuration).toBe(before.configuration ?? 0);
    expect(renderCounts.collapse).toBe(before.collapse ?? 0);
  });
});
