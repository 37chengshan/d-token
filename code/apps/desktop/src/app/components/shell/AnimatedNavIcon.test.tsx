import "@testing-library/jest-dom/vitest";
// @ts-ignore Node runtime module
import fs from "node:fs";
// @ts-ignore Node runtime module
import path from "node:path";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  AnimatedNavIcon,
  NAV_MOTION_RECIPES,
  type AnimatedNavIconId,
  type VisibleAnimatedNavIconId,
} from "./AnimatedNavIcon";

declare const process: { cwd(): string };

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
        <AnimatedNavIcon
          id={id}
          active={id === "overview"}
          reducedMotion={false}
        />,
      );
      const icon = container.querySelector(`[data-motion-id="${id}"]`);
      expect(icon).toBeInTheDocument();
      expect(icon?.querySelector(`.${recipes[id]}`)).toBeInTheDocument();
      unmount();
    }

    expect(new Set(visibleIds.map((id) => recipes[id])).size).toBe(
      visibleIds.length,
    );
    expect(
      new Set(Object.values(NAV_MOTION_RECIPES).map((item) => item.recipe))
        .size,
    ).toBe(10);
  });

  it("defines one-shot recipes whose duration covers the full CSS play window", () => {
    // shell.css drives the icon-part transition at var(--duration-slow) = 280ms
    // with leading transition-delays per part. durationMs is the full play
    // window the pulse timer must stay on, so it must cover max(delay + 280).
    // Overview staggers 0/40/80/120ms (400), analytics 0/40/80ms (360); every
    // other recipe has no leading delay and needs at least the 280ms transition.
    const maxDelayMs: Partial<Record<VisibleAnimatedNavIconId, number>> = {
      overview: 120,
      analytics: 80,
    };
    const iconPartTransitionMs = 280;

    for (const [id, recipe] of Object.entries(NAV_MOTION_RECIPES) as [
      VisibleAnimatedNavIconId,
      (typeof NAV_MOTION_RECIPES)[VisibleAnimatedNavIconId],
    ][]) {
      expect(recipe.durationMs, id).toBeGreaterThanOrEqual(
        (maxDelayMs[id] ?? 0) + iconPartTransitionMs,
      );
      expect(recipe.iterationCount, id).toBe(1);
    }
    expect(NAV_MOTION_RECIPES.overview.durationMs).toBe(400);
    expect(NAV_MOTION_RECIPES.analytics.durationMs).toBe(360);
  });

  it("exposes a deterministic CSS color-only override for prop-level reduced motion", () => {
    // The data-reduced-motion state alone must be backed by a CSS selector so
    // hover/focus/pressed icon transforms and the parent press scale are
    // disabled even when the OS prefers-reduced-motion media query does not
    // match. The icon-part rule must win over the recipe selectors. The parent
    // .nav-motion-item mirrors the flag on its own data-reduced-motion attribute
    // so the press-scale kill uses a plain attribute selector — no :has(), which
    // macOS 10.14 WKWebView does not support.
    const css = fs.readFileSync(
      path.resolve(process.cwd(), "src/styles/shell.css"),
      "utf8",
    );
    expect(css).toContain(
      '.animated-nav-icon[data-reduced-motion="true"] .icon-part',
    );
    expect(css).toMatch(
      /\.animated-nav-icon\[data-reduced-motion="true"\] \.icon-part\s*\{[^}]*transform: none !important;[^}]*transition: none !important;[^}]*animation: none !important;/,
    );
    expect(css).toContain(
      '.nav-motion-item[data-reduced-motion="true"]:active > svg',
    );
    expect(css).toContain(
      '.nav-motion-item[data-reduced-motion="true"]:active > span',
    );
    expect(css).not.toContain(".nav-motion-item:has(");
  });

  it("marks active and reduced-motion state without exposing decorative SVGs", () => {
    const { container } = render(
      <AnimatedNavIcon id="doctor" active reducedMotion />,
    );
    const icon = container.querySelector("svg");

    expect(icon).toHaveAttribute("aria-hidden", "true");
    expect(icon).toHaveAttribute("data-active", "true");
    expect(icon).toHaveAttribute("data-reduced-motion", "true");
    expect(icon).toHaveAttribute("data-motion-state", "color-only");
    expect(icon).toHaveAttribute("data-motion-iterations", "1");
    expect(icon).toHaveAttribute("data-motion-play", "false");
    expect(icon).toHaveAttribute("data-motion-engaged", "false");
  });

  it("keeps every navigation icon free of the duplicate cat-face mark", () => {
    const allIds: AnimatedNavIconId[] = [
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

    for (const id of allIds) {
      const { container, unmount } = render(
        <AnimatedNavIcon id={id} active={false} reducedMotion={false} />,
      );
      const icon = container.querySelector(`[data-motion-id="${id}"]`);
      expect(icon).toHaveAttribute("stroke", "currentColor");
      // The cat-face mark belongs only to the brand; per-item nav icons must
      // render their own distinct semantic glyph without a duplicated face.
      expect(
        icon?.querySelector(".icon-cat-face"),
        `${id} must not carry the per-item cat-face mark`,
      ).not.toBeInTheDocument();
      // Each nav still exposes its own independent semantic glyph class.
      expect(
        icon?.querySelector(`.${recipes[id]}`),
        `${id} must render its own semantic glyph class`,
      ).toBeInTheDocument();
      unmount();
    }
  });

  it("sets data-motion-engaged when parent reports hover/focus for enter/leave", () => {
    const { container, rerender } = render(
      <AnimatedNavIcon
        id="settings"
        active={false}
        reducedMotion={false}
        engaged={false}
      />,
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-engaged",
      "false",
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-play",
      "false",
    );

    rerender(
      <AnimatedNavIcon
        id="settings"
        active={false}
        reducedMotion={false}
        engaged
      />,
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-engaged",
      "true",
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-play",
      "true",
    );

    rerender(
      <AnimatedNavIcon
        id="settings"
        active={false}
        reducedMotion={false}
        engaged={false}
      />,
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-engaged",
      "false",
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-play",
      "false",
    );
  });

  it("sets data-motion-play when motionToken bumps so click can pulse without remount", () => {
    vi.useFakeTimers();
    const { container, rerender } = render(
      <AnimatedNavIcon
        id="settings"
        active={false}
        reducedMotion={false}
        motionToken={0}
      />,
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-play",
      "false",
    );

    rerender(
      <AnimatedNavIcon
        id="settings"
        active={false}
        reducedMotion={false}
        motionToken={1}
      />,
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-play",
      "true",
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-trigger",
      "1",
    );

    act(() => {
      vi.advanceTimersByTime(NAV_MOTION_RECIPES.settings.durationMs + 80);
    });
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-play",
      "false",
    );
    vi.useRealTimers();
  });

  it("ignores engaged and motionToken under reduced motion", () => {
    const { container } = render(
      <AnimatedNavIcon
        id="settings"
        active={false}
        reducedMotion
        engaged
        motionToken={3}
      />,
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-play",
      "false",
    );
    expect(container.querySelector("svg")).toHaveAttribute(
      "data-motion-engaged",
      "false",
    );
  });
});
