import { memo, useEffect, useState } from "react";
import type { PageId } from "../../lib/nav";
import { cn } from "../ui/utils";

type LegacyAnimatedNavIconId =
  | "overview"
  | "install"
  | "codebase-memory"
  | "advanced"
  | "analytics"
  | "visual"
  | "doctor"
  | "changes"
  | "logs"
  | "compatibility";
/** Legacy icon IDs remain renderable for old embedded callers during migration. */
export type AnimatedNavIconId = LegacyAnimatedNavIconId | "settings" | "theme" | "collapse";
export type AnimatedNavIconIdWithLegacy = PageId | AnimatedNavIconId;
export type VisibleAnimatedNavIconId = Extract<
  AnimatedNavIconIdWithLegacy,
  | "configuration"
  | "advanced-configuration"
  | "analysis-logs"
  | "diagnostics"
  | "settings"
  | "theme"
  | "collapse"
  | "overview"
  | "install"
  | "codebase-memory"
  | "advanced"
  | "analytics"
  | "visual"
  | "doctor"
  | "changes"
  | "logs"
  | "compatibility"
>;

/**
 * Per-icon recipes. `durationMs` is the FULL play window the click pulse must
 * stay on: it must cover max(leading transition-delay) + the icon-part
 * transition length in shell.css (var(--duration-slow) = 280ms). The pulse
 * timer uses durationMs + a small margin, so a value shorter than the real
 * total would flip data-motion-play off mid-transition and reverse the icon.
 * Overview staggers 0/40/80/120ms (total 400), analytics 0/40/80ms (total
 * 360); every other recipe has no leading delay (total 280ms).
 */
export const NAV_MOTION_RECIPES: Record<
  VisibleAnimatedNavIconId,
  { durationMs: number; iterationCount: 1; recipe: string }
> = {
  configuration: { durationMs: 280, iterationCount: 1, recipe: "arrow-land-tray-bounce" },
  "advanced-configuration": { durationMs: 280, iterationCount: 1, recipe: "gear-quarter-turn" },
  "analysis-logs": { durationMs: 360, iterationCount: 1, recipe: "staggered-bar-grow" },
  diagnostics: { durationMs: 280, iterationCount: 1, recipe: "path-trace-status-pulse" },
  overview: { durationMs: 400, iterationCount: 1, recipe: "staggered-grid-lift" },
  install: { durationMs: 280, iterationCount: 1, recipe: "arrow-land-tray-bounce" },
  "codebase-memory": { durationMs: 280, iterationCount: 1, recipe: "graph-nodes-connect" },
  advanced: { durationMs: 280, iterationCount: 1, recipe: "gear-quarter-turn" },
  analytics: { durationMs: 360, iterationCount: 1, recipe: "staggered-bar-grow" },
  visual: { durationMs: 280, iterationCount: 1, recipe: "image-frame-focus" },
  doctor: { durationMs: 280, iterationCount: 1, recipe: "path-trace-status-pulse" },
  changes: { durationMs: 280, iterationCount: 1, recipe: "paired-arrow-converge" },
  logs: { durationMs: 280, iterationCount: 1, recipe: "paired-arrow-converge" },
  compatibility: { durationMs: 280, iterationCount: 1, recipe: "path-trace-status-pulse" },
  settings: { durationMs: 280, iterationCount: 1, recipe: "gear-quarter-turn" },
  theme: { durationMs: 280, iterationCount: 1, recipe: "sun-rays-or-moon-arc" },
  collapse: { durationMs: 280, iterationCount: 1, recipe: "panel-fold" },
};

export type AnimatedNavIconProps = {
  id: AnimatedNavIconIdWithLegacy;
  active: boolean;
  reducedMotion: boolean;
  className?: string;
  expanded?: boolean;
  dark?: boolean;
  /** Bumped on click so the recipe can replay without relying on :hover. */
  motionToken?: number;
};

/**
 * Product-owned semantic icon set. Each navigation action has its own
 * independent, recognizable glyph. Parts are plain SVG currentColor strokes;
 * motion is CSS-driven via `.nav-motion-item` hover/focus and
 * `[data-motion-play]` click pulses. Decorative SVGs stay aria-hidden and
 * focusable=false while button labels provide semantics. The d-token cat-face
 * mark lives only in the brand mark, not in per-item nav icons.
 */
export const AnimatedNavIcon = memo(function AnimatedNavIcon({
  id,
  active,
  reducedMotion,
  className,
  expanded = true,
  dark = false,
  motionToken = 0,
}: AnimatedNavIconProps) {
  const recipe = id in NAV_MOTION_RECIPES ? NAV_MOTION_RECIPES[id as VisibleAnimatedNavIconId] : null;
  const [playing, setPlaying] = useState(false);

  useEffect(() => {
    if (reducedMotion || motionToken <= 0) {
      setPlaying(false);
      return;
    }
    setPlaying(true);
    const timer = window.setTimeout(() => setPlaying(false), (recipe?.durationMs ?? 240) + 40);
    return () => window.clearTimeout(timer);
  }, [motionToken, reducedMotion, recipe?.durationMs]);

  const motionActive = !reducedMotion && playing;

  const common = {
    "aria-hidden": true as const,
    focusable: "false" as const,
    viewBox: "0 0 24 24",
    fill: "none" as const,
    stroke: "currentColor",
    strokeWidth: 1.75,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    className: cn("animated-nav-icon", className),
    "data-motion-id": id,
    "data-active": active ? "true" : "false",
    "data-reduced-motion": reducedMotion ? "true" : "false",
    "data-motion-state": reducedMotion ? "color-only" : "enabled",
    "data-motion-trigger": String(motionToken),
    "data-motion-play": motionActive ? "true" : "false",
    ...(recipe
      ? {
          "data-motion-recipe": recipe.recipe,
          "data-motion-duration": String(recipe.durationMs),
          "data-motion-iterations": "1",
        }
      : {}),
  };

  switch (id) {
    case "overview":
      return (
        <svg {...common}>
          <rect className="icon-part icon-grid-1" x="3.5" y="3.5" width="7" height="7" rx="1.4" />
          <rect className="icon-part icon-grid-2" x="13.5" y="3.5" width="7" height="7" rx="1.4" />
          <rect className="icon-part icon-grid-3" x="3.5" y="13.5" width="7" height="7" rx="1.4" />
          <rect className="icon-part icon-grid-4" x="13.5" y="13.5" width="7" height="7" rx="1.4" />
        </svg>
      );
    case "configuration":
    case "install":
      return (
        <svg {...common}>
          <path className="icon-part icon-download-arrow" d="M12 3.5v10m-3.5-3 3.5 3.5 3.5-3.5" />
          <path className="icon-part icon-download-tray" d="M4.5 15.5v3.2c0 1 .8 1.8 1.8 1.8h11.4c1 0 1.8-.8 1.8-1.8v-3.2" />
        </svg>
      );
    case "codebase-memory":
      return (
        <svg {...common}>
          <path className="icon-part icon-memory-edge" d="M7.2 7.2 12 4.7l4.8 2.5M7.2 7.2v5.6L12 16m4.8-8.8v5.6L12 16m0 0v3.3" />
          <circle className="icon-part icon-memory-node-1" cx="12" cy="4.5" r="2" />
          <circle className="icon-part icon-memory-node-2" cx="6.8" cy="8" r="2" />
          <circle className="icon-part icon-memory-node-3" cx="17.2" cy="8" r="2" />
          <circle className="icon-part icon-memory-node-4" cx="12" cy="19.5" r="2" />
        </svg>
      );
    case "analysis-logs":
    case "analytics":
    case "logs":
      return (
        <svg {...common}>
          <path d="M3.5 20.5h17" />
          <path className="icon-part icon-bar-1" d="M6.5 18v-5" />
          <path className="icon-part icon-bar-2" d="M12 18V7" />
          <path className="icon-part icon-bar-3" d="M17.5 18v-8" />
        </svg>
      );
    case "visual":
      return (
        <svg {...common}>
          <rect className="icon-part icon-frame" x="3.5" y="5" width="17" height="14" rx="2" />
          <circle className="icon-part icon-lens" cx="9" cy="10.5" r="1.6" />
          <path className="icon-part icon-landscape" d="m5.5 16.5 3.2-3.4 2.4 2.1 3.1-3.8 4.3 5.1" />
        </svg>
      );
    case "diagnostics":
    case "doctor":
    case "compatibility":
      return (
        <svg {...common}>
          <path className="icon-part icon-doctor-path" d="M6 4.5v5a6 6 0 0 0 12 0v-5M4 4.5h4M16 4.5h4" />
          <path d="M12 15.5v1.2a3.8 3.8 0 0 0 3.8 3.8" />
          <circle className="icon-part icon-doctor-dot" cx="18.4" cy="20.2" r="1.6" />
        </svg>
      );
    case "changes":
      return (
        <svg {...common}>
          <path className="icon-part icon-change-top" d="M4 8h14m-3-3 3 3-3 3" />
          <path className="icon-part icon-change-bottom" d="M20 16H6m3 3-3-3 3-3" />
        </svg>
      );
    case "advanced-configuration":
    case "advanced":
    case "settings":
      return (
        <svg {...common}>
          <path
            className="icon-part icon-gear"
            d="M9.5 3.7h5l.7 2.3 2 .9 2.1-1.1 2.5 4.3-1.7 1.6v2.4l1.7 1.6-2.5 4.3-2.1-1.1-2 .9-.7 2.3h-5l-.7-2.3-2-.9L4.7 20l-2.5-4.3 1.7-1.6v-2.4l-1.7-1.6 2.5-4.3 2.1 1.1 2-.9.7-2.3Z"
          />
          <circle className="icon-part icon-gear-hub" cx="12" cy="12.9" r="2.8" />
        </svg>
      );
    case "theme":
      return dark ? (
        <svg {...common}>
          <path className="icon-part icon-moon" d="M19.5 15.2A8 8 0 0 1 8.8 4.5 8.1 8.1 0 1 0 19.5 15.2Z" />
        </svg>
      ) : (
        <svg {...common}>
          <circle className="icon-part icon-sun-core" cx="12" cy="12" r="3.2" />
          <path
            className="icon-part icon-sun-rays"
            d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M18.7 5.3l-1.4 1.4M6.7 17.3l-1.4 1.4"
          />
        </svg>
      );
    case "collapse":
      return (
        <svg {...common}>
          <rect x="3.5" y="4" width="17" height="16" rx="2" />
          <path className="icon-part icon-panel-line" d="M9 4v16" />
          <path className="icon-part icon-panel-arrow" d={expanded ? "m14.5 9-3 3 3 3" : "m12 9 3 3-3 3"} />
        </svg>
      );
    default:
      return null;
  }
});
