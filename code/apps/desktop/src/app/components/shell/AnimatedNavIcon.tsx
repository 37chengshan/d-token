import type { PageId } from "../../lib/nav";
import { cn } from "../ui/utils";

export type AnimatedNavIconId = PageId | "theme" | "collapse";
export type VisibleAnimatedNavIconId = Extract<
  AnimatedNavIconId,
  "overview" | "install" | "analytics" | "doctor" | "changes" | "settings" | "theme" | "collapse"
>;

export const NAV_MOTION_RECIPES: Record<
  VisibleAnimatedNavIconId,
  { durationMs: number; iterationCount: 1; recipe: string }
> = {
  overview: { durationMs: 180, iterationCount: 1, recipe: "staggered-grid-lift" },
  install: { durationMs: 200, iterationCount: 1, recipe: "arrow-land-tray-bounce" },
  analytics: { durationMs: 180, iterationCount: 1, recipe: "staggered-bar-grow" },
  doctor: { durationMs: 220, iterationCount: 1, recipe: "path-trace-status-pulse" },
  changes: { durationMs: 190, iterationCount: 1, recipe: "paired-arrow-converge" },
  settings: { durationMs: 200, iterationCount: 1, recipe: "gear-quarter-turn" },
  theme: { durationMs: 200, iterationCount: 1, recipe: "sun-rays-or-moon-arc" },
  collapse: { durationMs: 180, iterationCount: 1, recipe: "panel-fold" },
};

export type AnimatedNavIconProps = {
  id: AnimatedNavIconId;
  active: boolean;
  reducedMotion: boolean;
  className?: string;
  expanded?: boolean;
  dark?: boolean;
  motionToken?: number;
};

/**
 * Product-owned icon set. Each glyph has a motion recipe tied to its meaning;
 * no AiMaMi brand or icon assets are copied here.
 */
export function AnimatedNavIcon({
  id,
  active,
  reducedMotion,
  className,
  expanded = true,
  dark = false,
  motionToken = 0,
}: AnimatedNavIconProps) {
  const common = {
    "aria-hidden": true,
    focusable: "false" as const,
    viewBox: "0 0 24 24",
    fill: "none",
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
    ...(id in NAV_MOTION_RECIPES
      ? {
          "data-motion-recipe": NAV_MOTION_RECIPES[id as VisibleAnimatedNavIconId].recipe,
          "data-motion-duration": String(NAV_MOTION_RECIPES[id as VisibleAnimatedNavIconId].durationMs),
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
    case "install":
      return (
        <svg {...common}>
          <path className="icon-part icon-download-arrow" d="M12 3.5v10m-3.5-3 3.5 3.5 3.5-3.5" />
          <path className="icon-part icon-download-tray" d="M4.5 15.5v3.2c0 1 .8 1.8 1.8 1.8h11.4c1 0 1.8-.8 1.8-1.8v-3.2" />
        </svg>
      );
    case "analytics":
      return (
        <svg {...common}>
          <path d="M3.5 20.5h17" />
          <path className="icon-part icon-bar-1" d="M6.5 18v-5" />
          <path className="icon-part icon-bar-2" d="M12 18V7" />
          <path className="icon-part icon-bar-3" d="M17.5 18v-8" />
        </svg>
      );
    case "doctor":
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
    case "settings":
      return (
        <svg {...common}>
          <path className="icon-part icon-gear" d="M9.5 3.7h5l.7 2.3 2 .9 2.1-1.1 2.5 4.3-1.7 1.6v2.4l1.7 1.6-2.5 4.3-2.1-1.1-2 .9-.7 2.3h-5l-.7-2.3-2-.9L4.7 20l-2.5-4.3 1.7-1.6v-2.4l-1.7-1.6 2.5-4.3 2.1 1.1 2-.9.7-2.3Z" />
          <circle cx="12" cy="12.9" r="2.8" />
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
          <path className="icon-part icon-sun-rays" d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M18.7 5.3l-1.4 1.4M6.7 17.3l-1.4 1.4" />
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
}
