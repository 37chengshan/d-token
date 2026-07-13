import "@testing-library/jest-dom/vitest";
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  AnimatedNavIcon,
  NAV_MOTION_RECIPES,
  type AnimatedNavIconId,
  type VisibleAnimatedNavIconId,
} from "./AnimatedNavIcon";

afterEach(cleanup);

const recipes: Record<AnimatedNavIconId, string> = {
  overview: "icon-grid-1",
  install: "icon-download-arrow",
  analytics: "icon-bar-1",
  doctor: "icon-doctor-path",
  changes: "icon-change-top",
  settings: "icon-gear",
  theme: "icon-sun-rays",
  collapse: "icon-panel-line",
  advanced: "",
  logs: "",
  compatibility: "",
};

describe("AnimatedNavIcon", () => {
  it("exposes a distinct semantic motion recipe for every visible navigation action", () => {
    const visibleIds: AnimatedNavIconId[] = [
      "overview",
      "install",
      "analytics",
      "doctor",
      "changes",
      "settings",
      "theme",
      "collapse",
    ];

    for (const id of visibleIds) {
      const { container, unmount } = render(
        <AnimatedNavIcon id={id} active={id === "overview"} reducedMotion={false} />,
      );
      const icon = container.querySelector(`[data-motion-id="${id}"]`);
      expect(icon).toBeInTheDocument();
      expect(icon?.querySelector(`.${recipes[id]}`)).toBeInTheDocument();
      unmount();
    }

    expect(new Set(visibleIds.map((id) => recipes[id])).size).toBe(visibleIds.length);
    expect(new Set(Object.values(NAV_MOTION_RECIPES).map((item) => item.recipe)).size).toBe(8);
  });

  it("defines finite 160–220ms one-shot recipes", () => {
    for (const [id, recipe] of Object.entries(NAV_MOTION_RECIPES) as [VisibleAnimatedNavIconId, (typeof NAV_MOTION_RECIPES)[VisibleAnimatedNavIconId]][]) {
      expect(recipe.durationMs, id).toBeGreaterThanOrEqual(160);
      expect(recipe.durationMs, id).toBeLessThanOrEqual(220);
      expect(recipe.iterationCount, id).toBe(1);
    }
  });

  it("marks active and reduced-motion state without exposing decorative SVGs", () => {
    const { container } = render(<AnimatedNavIcon id="doctor" active reducedMotion />);
    const icon = container.querySelector("svg");

    expect(icon).toHaveAttribute("aria-hidden", "true");
    expect(icon).toHaveAttribute("data-active", "true");
    expect(icon).toHaveAttribute("data-reduced-motion", "true");
    expect(icon).toHaveAttribute("data-motion-state", "color-only");
    expect(icon).toHaveAttribute("data-motion-iterations", "1");
  });
});
