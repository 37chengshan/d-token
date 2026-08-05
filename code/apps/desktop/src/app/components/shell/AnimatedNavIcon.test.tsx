import "@testing-library/jest-dom/vitest";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
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
  "codebase-memory": "icon-memory-edge",
  analytics: "icon-bar-1",
  visual: "icon-frame",
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
      "codebase-memory",
      "analytics",
      "visual",
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
    expect(new Set(Object.values(NAV_MOTION_RECIPES).map((item) => item.recipe)).size).toBe(10);
  });

  it("defines finite 200–280ms one-shot recipes", () => {
    for (const [id, recipe] of Object.entries(NAV_MOTION_RECIPES) as [VisibleAnimatedNavIconId, (typeof NAV_MOTION_RECIPES)[VisibleAnimatedNavIconId]][]) {
      expect(recipe.durationMs, id).toBeGreaterThanOrEqual(200);
      expect(recipe.durationMs, id).toBeLessThanOrEqual(280);
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
    expect(icon).toHaveAttribute("data-motion-play", "false");
    expect(icon).toHaveAttribute("data-motion-engaged", "false");
  });

  it("sets data-motion-engaged when parent reports hover/focus for enter/leave", () => {
    const { container, rerender } = render(
      <AnimatedNavIcon id="settings" active={false} reducedMotion={false} engaged={false} />,
    );
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-engaged", "false");
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-play", "false");

    rerender(<AnimatedNavIcon id="settings" active={false} reducedMotion={false} engaged />);
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-engaged", "true");
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-play", "true");

    rerender(<AnimatedNavIcon id="settings" active={false} reducedMotion={false} engaged={false} />);
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-engaged", "false");
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-play", "false");
  });

  it("sets data-motion-play when motionToken bumps so click can pulse without remount", () => {
    vi.useFakeTimers();
    const { container, rerender } = render(
      <AnimatedNavIcon id="settings" active={false} reducedMotion={false} motionToken={0} />,
    );
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-play", "false");

    rerender(<AnimatedNavIcon id="settings" active={false} reducedMotion={false} motionToken={1} />);
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-play", "true");
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-trigger", "1");

    act(() => {
      vi.advanceTimersByTime(NAV_MOTION_RECIPES.settings.durationMs + 80);
    });
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-play", "false");
    vi.useRealTimers();
  });

  it("ignores engaged and motionToken under reduced motion", () => {
    const { container } = render(
      <AnimatedNavIcon id="settings" active={false} reducedMotion engaged motionToken={3} />,
    );
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-play", "false");
    expect(container.querySelector("svg")).toHaveAttribute("data-motion-engaged", "false");
  });
});
