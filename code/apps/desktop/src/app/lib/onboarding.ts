import { useCallback, useEffect, useState } from "react";
import { invokeNative } from "./native";

// First-run onboarding gate. The only product-owned completion fact is the
// first-config marker persisted in the d-token local settings store
// (preferences section). Discovery evidence is never treated as completion by
// itself: the wizard surfaces real detected state and the user explicitly
// confirms completion, after which the marker is written once.
//
// The marker deliberately reuses the native settings contract instead of
// localStorage: App restart must restore the same first-run decision, and the
// native store is the single source of truth for d-token local settings.

export const ONBOARDING_SETTINGS_SECTION = "preferences";

export type OnboardingGateState = "checking" | "pending" | "completed" | "unavailable";

export type OnboardingMarker = {
  completed: boolean;
  completed_at_ms?: number;
  mode?: string;
};

export type OnboardingEvidence = {
  connectedAgents: string[];
  routeFollowEnabled: boolean;
  hasConnectedEvidence: boolean;
};

/** Reads `payload.onboarding` from a native settings payload; null when absent or malformed. */
export function parseOnboardingMarker(payload: unknown): OnboardingMarker | null {
  if (!payload || typeof payload !== "object") return null;
  const onboarding = (payload as Record<string, unknown>).onboarding;
  if (!onboarding || typeof onboarding !== "object") return null;
  const record = onboarding as Record<string, unknown>;
  if (typeof record.completed !== "boolean") return null;
  return {
    completed: record.completed,
    completed_at_ms: typeof record.completed_at_ms === "number" ? record.completed_at_ms : undefined,
    mode: typeof record.mode === "string" ? record.mode : undefined,
  };
}

/** Reads the persisted marker from the native settings store. Rejects when the store is unreachable. */
export async function readOnboardingMarker(): Promise<OnboardingMarker | null> {
  const saved = await invokeNative<{ state?: string; payload?: unknown }>("get_app_settings", {
    section: ONBOARDING_SETTINGS_SECTION,
  });
  if (saved?.state !== "loaded" && saved?.state !== "missing") {
    throw new Error("onboarding_settings_unavailable");
  }
  return parseOnboardingMarker(saved?.payload);
}

/** Persists the first-config completion marker. Rejects when the write fails. */
export async function writeOnboardingMarker(mode: string): Promise<void> {
  const result = await invokeNative<{ ok: boolean; summary?: string }>("save_app_settings", {
    section: ONBOARDING_SETTINGS_SECTION,
    payload: {
      onboarding: { completed: true, completed_at_ms: Date.now(), mode },
      scope: "d-token-local-settings",
      no_external_agent_write: true,
    },
  });
  if (!result?.ok) throw new Error(result?.summary || "onboarding_marker_save_failed");
}

/**
 * Derives connected evidence from the real discovery snapshot. Only native
 * connected-state claims count: `already_connected` bindings and an enabled
 * agent route follow. Detection without connection is never reported here.
 */
export function deriveOnboardingEvidence(
  snapshot: {
    agent_bindings?: Array<{ agent_id: string; already_connected?: boolean }>;
    agent_route_follow?: { enabled?: boolean };
  } | null,
): OnboardingEvidence {
  const bindings = snapshot?.agent_bindings ?? [];
  const connectedAgents = bindings
    .filter((binding) => binding.already_connected === true)
    .map((binding) => binding.agent_id);
  const routeFollowEnabled = snapshot?.agent_route_follow?.enabled === true;
  return {
    connectedAgents,
    routeFollowEnabled,
    hasConnectedEvidence: connectedAgents.length > 0 || routeFollowEnabled,
  };
}

export function useOnboardingGate() {
  const [state, setState] = useState<OnboardingGateState>("checking");

  useEffect(() => {
    let cancelled = false;
    readOnboardingMarker()
      .then((marker) => {
        if (cancelled) return;
        setState(marker?.completed === true ? "completed" : "pending");
      })
      .catch(() => {
        // The store is unreachable (browser preview or transient failure).
        // Fail open for the shell: do not lock the app behind a wizard that
        // cannot read or write anything. The top-right entry still opens the
        // wizard, which surfaces honest unavailable states.
        if (cancelled) return;
        setState("unavailable");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const complete = useCallback(async (): Promise<boolean> => {
    try {
      await writeOnboardingMarker("user-confirmed");
      setState("completed");
      return true;
    } catch {
      return false;
    }
  }, []);

  return { state, complete };
}
