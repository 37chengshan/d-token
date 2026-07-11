import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { invokeNative } from "./native";

export type RuntimeDashboardStatus = "loading" | "runtime" | "browser-preview" | "unavailable";

export type RuntimeDashboardState = {
  payload: Record<string, any> | null;
  status: RuntimeDashboardStatus;
  updatedAt: Date | null;
  error: string;
  refresh: () => Promise<void>;
};

const RuntimeEvidenceContext = createContext<RuntimeDashboardState | null>(null);

export function RuntimeEvidenceProvider({ children }: { children: ReactNode }) {
  const state = useRuntimeEvidence(2500, true);
  return createElement(RuntimeEvidenceContext.Provider, { value: state }, children);
}

/**
 * Returns the app-wide runtime evidence when mounted under RuntimeEvidenceProvider.
 * The standalone branch keeps component tests and isolated stories usable without
 * starting a second poller when a provider is present.
 */
export function useRuntimeDashboard(intervalMs = 2500): RuntimeDashboardState {
  const shared = useContext(RuntimeEvidenceContext);
  const standalone = useRuntimeEvidence(intervalMs, shared === null);
  return shared ?? standalone;
}

function useRuntimeEvidence(intervalMs: number, enabled: boolean): RuntimeDashboardState {
  const [payload, setPayload] = useState<Record<string, any> | null>(null);
  const [status, setStatus] = useState<RuntimeDashboardStatus>("loading");
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [error, setError] = useState("");
  const mounted = useRef(false);
  const refreshing = useRef(false);
  const failureCount = useRef(0);

  const refresh = useCallback(async () => {
    if (!enabled || refreshing.current) return;
    refreshing.current = true;
    try {
      const next = await invokeNative<Record<string, any>>("get_dtoken_dashboard_payload");
      if (!mounted.current) return;
      const classification = classifyRuntimePayload(next);
      setPayload(next);
      setStatus(classification.status);
      setUpdatedAt(new Date());
      setError(classification.error);
      failureCount.current = classification.status === "runtime" ? 0 : failureCount.current + 1;
    } catch (nextError) {
      if (!mounted.current) return;
      const isBrowserPreview = nextError instanceof Error && nextError.message === "native_unavailable";
      setPayload(null);
      setStatus(isBrowserPreview ? "browser-preview" : "unavailable");
      setUpdatedAt(new Date());
      setError(isBrowserPreview ? "" : safeErrorMessage(nextError));
      failureCount.current = isBrowserPreview ? 0 : failureCount.current + 1;
    } finally {
      refreshing.current = false;
    }
  }, [enabled]);

  useEffect(() => {
    if (!enabled) return;
    mounted.current = true;
    let timer: number | undefined;

    const schedule = () => {
      if (!mounted.current || document.visibilityState === "hidden") return;
      const multiplier = Math.min(8, 2 ** failureCount.current);
      timer = window.setTimeout(async () => {
        await refresh();
        schedule();
      }, intervalMs * multiplier);
    };

    const refreshAndSchedule = async () => {
      if (timer !== undefined) window.clearTimeout(timer);
      if (document.visibilityState === "hidden") return;
      await refresh();
      schedule();
    };

    const handleVisibility = () => {
      if (document.visibilityState === "hidden") {
        if (timer !== undefined) window.clearTimeout(timer);
        timer = undefined;
        return;
      }
      void refreshAndSchedule();
    };

    document.addEventListener("visibilitychange", handleVisibility);
    void refreshAndSchedule();
    return () => {
      mounted.current = false;
      document.removeEventListener("visibilitychange", handleVisibility);
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [enabled, intervalMs, refresh]);

  return useMemo(
    () => ({ payload, status, updatedAt, error, refresh }),
    [payload, status, updatedAt, error, refresh],
  );
}

export function classifyRuntimePayload(next: Record<string, any>): Pick<RuntimeDashboardState, "status" | "error"> {
  const source = typeof next?.source === "string" ? next.source : "";
  const desktopSource = typeof next?.desktop_data_source === "string" ? next.desktop_data_source : "";
  const isRuntimePayload = source === "dtoken-owned-runtime" && desktopSource === "dtoken-local-service";
  return {
    status: isRuntimePayload ? "runtime" : "unavailable",
    error: isRuntimePayload ? "" : safeErrorMessage(next?.desktop_service_error ?? "d-token 本地服务暂不可用"),
  };
}

function safeErrorMessage(value: unknown) {
  if (value instanceof Error) return value.message;
  if (typeof value === "string" && value.trim()) return value;
  return "无法读取 d-token 本地服务";
}
