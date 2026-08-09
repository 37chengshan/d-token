/** Pure draft helpers for visual-compression (Optical) product config. */
import { translateCopy } from "../i18n";

export type OpticalSafeMode = "off" | "shadow_preview";

export type OpticalGlobalDraft = {
  enabled: boolean;
  default_mode: OpticalSafeMode;
};

export type OpticalRouteDraft = {
  route_id: string;
  enabled: boolean;
  mode: OpticalSafeMode;
  allow_tool_result: boolean;
  allow_history: boolean;
  confirmed_experimental_lossy: boolean;
};

export type OpticalConfigDraft = {
  global: OpticalGlobalDraft;
  routes: OpticalRouteDraft[];
};

export type OpticalRouteOption = {
  route_id: string;
  label?: string;
};

export type OpticalDraftValidation =
  | { ok: true; draft: OpticalConfigDraft }
  | { ok: false; error: string };

const SAFE_MODES = new Set<OpticalSafeMode>(["off", "shadow_preview"]);

export function clampOpticalMode(value: unknown): OpticalSafeMode {
  if (value === "off" || value === "shadow_preview") return value;
  // Any active-leaning or unknown mode collapses to shadow_preview for product safety.
  if (
    value === "tool_results_only" ||
    value === "history_only" ||
    value === "full_experimental"
  ) {
    return "shadow_preview";
  }
  return "shadow_preview";
}

export function isOpticalSafeMode(value: unknown): value is OpticalSafeMode {
  return typeof value === "string" && SAFE_MODES.has(value as OpticalSafeMode);
}

export function defaultOpticalDraft(routes: OpticalRouteOption[] = []): OpticalConfigDraft {
  return {
    global: {
      enabled: false,
      default_mode: "shadow_preview",
    },
    routes: routes.map((route) => ({
      route_id: route.route_id,
      enabled: false,
      mode: "off",
      allow_tool_result: false,
      allow_history: false,
      confirmed_experimental_lossy: false,
    })),
  };
}

export function normalizeOpticalDraft(
  input: Partial<OpticalConfigDraft> | null | undefined,
  knownRoutes: OpticalRouteOption[],
): OpticalDraftValidation {
  const knownIds = new Set(knownRoutes.map((route) => route.route_id));
  const globalEnabled = Boolean(input?.global?.enabled);
  const globalMode = clampOpticalMode(input?.global?.default_mode);

  const routeInputs = Array.isArray(input?.routes) ? input.routes : [];
  const byId = new Map<string, OpticalRouteDraft>();

  for (const raw of routeInputs) {
    if (!raw || typeof raw !== "object") {
      return { ok: false, error: "route policy entry is invalid" };
    }
    const routeId = typeof raw.route_id === "string" ? raw.route_id.trim() : "";
    if (!routeId) {
      return { ok: false, error: "route policy is missing route_id" };
    }
    if (!knownIds.has(routeId)) {
      return { ok: false, error: `unknown route_id: ${routeId}` };
    }
    byId.set(routeId, {
      route_id: routeId,
      enabled: Boolean(raw.enabled),
      mode: clampOpticalMode(raw.mode),
      allow_tool_result: Boolean(raw.allow_tool_result),
      allow_history: Boolean(raw.allow_history),
      confirmed_experimental_lossy: Boolean(raw.confirmed_experimental_lossy),
    });
  }

  // Ensure every known route has a row (defaults off).
  const routes: OpticalRouteDraft[] = knownRoutes.map((route) => {
    const existing = byId.get(route.route_id);
    if (existing) return existing;
    return {
      route_id: route.route_id,
      enabled: false,
      mode: "off",
      allow_tool_result: false,
      allow_history: false,
      confirmed_experimental_lossy: false,
    };
  });

  return {
    ok: true,
    draft: {
      global: {
        enabled: globalEnabled,
        default_mode: globalMode,
      },
      routes,
    },
  };
}

/** Build the optical JSON object fragment that will be merged into active config. */
export function buildOpticalConfigPatch(draft: OpticalConfigDraft): Record<string, unknown> {
  return {
    schema_version: 1,
    global_policy: {
      enabled: draft.global.enabled,
      default_mode: draft.global.default_mode,
      // Product path hard-closes active transform.
      allow_active: false,
      public_summary_only: true,
    },
    route_policies: draft.routes.map((route) => ({
      route_id: route.route_id,
      policy: {
        enabled: route.enabled,
        mode: route.mode,
        allow_tool_result: route.allow_tool_result,
        allow_history: route.allow_history,
        allow_schema_annotations: false,
        allow_system_static_slab: false,
        min_net_saved_tokens: 1024,
        confirmed_experimental_lossy: route.confirmed_experimental_lossy,
      },
    })),
  };
}

export function opticalDraftsEqual(a: OpticalConfigDraft, b: OpticalConfigDraft): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function summarizeOpticalDraft(draft: OpticalConfigDraft, lang: "zh" | "en"): string {
  const enabledRoutes = draft.routes.filter((route) => route.enabled).length;
  const state = draft.global.enabled ? translateCopy("optical.summary.on", lang) : translateCopy("optical.summary.off", lang);
  const mode = draft.global.default_mode === "off" ? translateCopy("optical.summary.off", lang) : translateCopy("optical.summary.shadowPreview", lang);
  return [
    translateCopy("optical.summary.globalEngine", lang, { state }),
    translateCopy("optical.summary.defaultMode", lang, { mode }),
    translateCopy("optical.summary.enabledRoutes", lang, { enabled: enabledRoutes, total: draft.routes.length }),
    translateCopy("optical.summary.activeForcedOff", lang),
  ].join(" · ");
}
