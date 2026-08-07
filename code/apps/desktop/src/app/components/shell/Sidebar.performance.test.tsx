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
        page="overview"
        setPage={vi.fn()}
        collapsed={false}
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const before = { ...renderCounts };
    fireEvent.pointerEnter(screen.getByRole("button", { name: "分析" }));
    const afterEnter = { ...renderCounts };

    expect(afterEnter.analytics).toBe(before.analytics ?? 0);
    expect(afterEnter.doctor).toBe(before.doctor ?? 0);
    expect(afterEnter.settings).toBe(before.settings ?? 0);

    fireEvent.pointerLeave(screen.getByRole("button", { name: "分析" }));
    const afterLeave = { ...renderCounts };
    expect(afterLeave.analytics).toBe(afterEnter.analytics ?? 0);
    expect(afterLeave.doctor).toBe(afterEnter.doctor ?? 0);
  });

  it("bumps the motion token only for the clicked item", () => {
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

    const before = { ...renderCounts };
    fireEvent.click(screen.getByRole("button", { name: "分析" }));

    expect(setPage).toHaveBeenCalledWith("analytics");
    expect(renderCounts.analytics).toBe((before.analytics ?? 0) + 1);
    expect(renderCounts.doctor).toBe(before.doctor ?? 0);
    expect(renderCounts.overview).toBe(before.overview ?? 0);
  });

  it("keeps theme and collapse controls isolated from navigation items", () => {
    render(
      <Sidebar
        page="overview"
        setPage={vi.fn()}
        collapsed
        setCollapsed={vi.fn()}
        reducedMotion={false}
      />,
    );

    const before = { ...renderCounts };
    fireEvent.pointerEnter(screen.getByRole("button", { name: "深色" }));

    expect(renderCounts.theme).toBe(before.theme ?? 0);
    expect(renderCounts.overview).toBe(before.overview ?? 0);
    expect(renderCounts.collapse).toBe(before.collapse ?? 0);
  });
});
