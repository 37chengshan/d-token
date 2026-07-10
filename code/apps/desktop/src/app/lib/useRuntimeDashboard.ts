import { useCallback, useEffect, useRef, useState } from "react";
import { invokeNative } from "./native";

export type RuntimeDashboardStatus = "loading" | "runtime" | "browser-preview" | "unavailable";

export type RuntimeDashboardState = {
  payload: Record<string, any> | null;
  status: RuntimeDashboardStatus;
  updatedAt: Date | null;
  error: string;
  refresh: () => Promise<void>;
};

export function useRuntimeDashboard(intervalMs = 2500): RuntimeDashboardState {
  const [payload, setPayload] = useState<Record<string, any> | null>(null);
  const [status, setStatus] = useState<RuntimeDashboardStatus>("loading");
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [error, setError] = useState("");
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const next = await invokeNative<Record<string, any>>("get_dtoken_dashboard_payload");
      if (!mounted.current) return;
      const source = typeof next?.source === "string" ? next.source : "";
      const desktopSource = typeof next?.desktop_data_source === "string" ? next.desktop_data_source : "";
      const isRuntimePayload = source === "dtoken-owned-runtime" && desktopSource === "dtoken-local-service";
      setPayload(next);
      setStatus(isRuntimePayload ? "runtime" : "unavailable");
      setUpdatedAt(new Date());
      setError(isRuntimePayload ? "" : String(next?.desktop_service_error ?? "d-token local service unavailable"));
    } catch (nextError) {
      if (!mounted.current) return;
      const isBrowserPreview = nextError instanceof Error && nextError.message === "native_unavailable";
      setStatus(isBrowserPreview ? "browser-preview" : "unavailable");
      setUpdatedAt(new Date());
      setError(isBrowserPreview ? "" : String(nextError));
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    const timer = window.setInterval(() => {
      void refresh();
    }, intervalMs);
    return () => {
      mounted.current = false;
      window.clearInterval(timer);
    };
  }, [intervalMs, refresh]);

  return { payload, status, updatedAt, error, refresh };
}
