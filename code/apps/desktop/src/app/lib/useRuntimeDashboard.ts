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
import { classifyEvidence } from "./state/evidence";

export type RuntimeDashboardStatus = "loading" | "runtime" | "browser-preview" | "unavailable" | "stale" | "partial";
export type OperationalSliceState = "loading" | "verified" | "partial" | "stale" | "unavailable";

export type OperationalSlice<T> = {
  data: T | null;
  source: string;
  state: OperationalSliceState;
  freshness: string;
  collectedAt: string | null;
  error: string;
};

export type RuntimeOperationalSnapshot = {
  service: OperationalSlice<Record<string, any>>;
  dashboard: OperationalSlice<Record<string, any>>;
  traffic: OperationalSlice<any[]>;
  routes: OperationalSlice<{ providers: any[]; routes: any[]; instances: any[]; decisions: any[] }>;
  transactions: OperationalSlice<{ events: any[]; backups: any[]; changes: any[] }>;
  doctor: OperationalSlice<Record<string, any>>;
  collectedAt: string | null;
};

export type RuntimeDashboardState = {
  payload: Record<string, any> | null;
  status: RuntimeDashboardStatus;
  updatedAt: Date | null;
  error: string;
  operational: RuntimeOperationalSnapshot;
  refresh: () => Promise<void>;
  invalidate: () => Promise<void>;
};

const emptySlice = <T,>(): OperationalSlice<T> => ({
  data: null,
  source: "unavailable",
  state: "loading",
  freshness: "unknown",
  collectedAt: null,
  error: "",
});

export const emptyOperationalSnapshot = (): RuntimeOperationalSnapshot => ({
  service: emptySlice<Record<string, any>>(),
  dashboard: emptySlice<Record<string, any>>(),
  traffic: emptySlice<any[]>(),
  routes: emptySlice<{ providers: any[]; routes: any[]; instances: any[]; decisions: any[] }>(),
  transactions: emptySlice<{ events: any[]; backups: any[]; changes: any[] }>(),
  doctor: emptySlice<Record<string, any>>(),
  collectedAt: null,
});

const RuntimeEvidenceContext = createContext<RuntimeDashboardState | null>(null);

export function RuntimeEvidenceProvider({ children }: { children: ReactNode }) {
  const state = useRuntimeEvidence(2500, true);
  return createElement(RuntimeEvidenceContext.Provider, { value: state }, children);
}

/** One app-wide snapshot. Pages must not infer service state from mount-local defaults. */
export function useRuntimeDashboard(intervalMs = 2500): RuntimeDashboardState {
  const shared = useContext(RuntimeEvidenceContext);
  const standalone = useRuntimeEvidence(intervalMs, shared === null);
  return shared ?? standalone;
}

function useRuntimeEvidence(intervalMs: number, enabled: boolean): RuntimeDashboardState {
  const [payload, setPayload] = useState<Record<string, any> | null>(null);
  const [status, setStatus] = useState<RuntimeDashboardStatus>("loading");
  const [operational, setOperational] = useState<RuntimeOperationalSnapshot>(emptyOperationalSnapshot);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [error, setError] = useState("");
  const mounted = useRef(false);
  const refreshing = useRef(false);
  const failureCount = useRef(0);

  const refresh = useCallback(async () => {
    if (!enabled || refreshing.current) return;
    refreshing.current = true;
    const collectedAt = new Date().toISOString();
    try {
      const [desktopResult, serviceResult] = await Promise.allSettled([
        invokeNative<Record<string, any>>("get_desktop_data"),
        invokeNative<Record<string, any>>("get_dtoken_service_status"),
      ]);
      if (!mounted.current) return;

      const nativeUnavailable = [desktopResult, serviceResult].some(
        (result) => result.status === "rejected" && result.reason instanceof Error && result.reason.message === "native_unavailable",
      );
      if (nativeUnavailable) {
        setStatus("browser-preview");
        setError("");
        failureCount.current = 0;
        return;
      }

      const desktop = desktopResult.status === "fulfilled" ? desktopResult.value : null;
      const service = serviceResult.status === "fulfilled" ? serviceResult.value : null;
      const serviceState = serviceStateFromPayload(service);
      const classification = desktop ? classifyRuntimePayload(desktop) : { status: "unavailable" as const, error: safeErrorMessage(desktopResult.status === "rejected" ? desktopResult.reason : "") };

      // get_desktop_data deliberately keeps desktop transaction/rollback evidence while
      // the runtime is offline. Never replace it with an empty object on a slice failure.
      if (desktop) setPayload(desktop);
      setOperational(buildOperationalSnapshot(desktop, service, collectedAt, desktopResult, serviceResult));
      const nextStatus: RuntimeDashboardStatus = classification.status === "runtime"
        ? "runtime"
        : serviceState === "running" || serviceState === "unmanaged_running"
          ? "partial"
          : classification.status;
      setStatus(nextStatus);
      setUpdatedAt(new Date(collectedAt));
      setError(nextStatus === "runtime" ? "" : classification.error);
      failureCount.current = nextStatus === "runtime" || nextStatus === "partial" ? 0 : failureCount.current + 1;
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
      timer = window.setTimeout(async () => { await refresh(); schedule(); }, intervalMs * multiplier);
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
      } else void refreshAndSchedule();
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
    () => ({ payload, status, operational, updatedAt, error, refresh, invalidate: refresh }),
    [payload, status, operational, updatedAt, error, refresh],
  );
}

function buildOperationalSnapshot(
  desktop: Record<string, any> | null,
  service: Record<string, any> | null,
  collectedAt: string,
  desktopResult: PromiseSettledResult<Record<string, any>>,
  serviceResult: PromiseSettledResult<Record<string, any>>,
): RuntimeOperationalSnapshot {
  const evidence = desktop ? classifyEvidence({ payload: desktop, status: "runtime", requireRuntimeIdentity: true }) : "unavailable";
  const runtimeVerified = evidence === "verified";
  const desktopState: OperationalSliceState = runtimeVerified ? "verified" : evidence === "stale" ? "stale" : desktop ? "partial" : "unavailable";
  const source = safeSource(desktop?.desktop_data_source ?? desktop?.source);
  const freshness = typeof desktop?.freshness === "string" ? desktop.freshness : runtimeVerified ? "fresh" : "local";
  const desktopError = runtimeVerified ? "" : safeErrorMessage(desktop?.error ?? desktop?.desktop_service_error ?? (desktopResult.status === "rejected" ? desktopResult.reason : ""));
  const serviceState = serviceStateFromPayload(service);
  const serviceVerified = Boolean(service && ["running", "unmanaged_running", "stopped"].includes(serviceState));
  const requestLogs = Array.isArray(desktop?.request_logs) ? desktop.request_logs : [];
  const routeData = {
    providers: Array.isArray(desktop?.providers) ? desktop.providers : [],
    routes: Array.isArray(desktop?.routes) ? desktop.routes : [],
    instances: Array.isArray(desktop?.instances) ? desktop.instances : [],
    decisions: Array.isArray(desktop?.route_decisions) ? desktop.route_decisions : Array.isArray(desktop?.recentDecisions) ? desktop.recentDecisions : [],
  };
  const transactionData = {
    events: Array.isArray(desktop?.desktopTransactionEvents) ? desktop.desktopTransactionEvents : [],
    backups: Array.isArray(desktop?.backupPoints) ? desktop.backupPoints : [],
    changes: Array.isArray(desktop?.changes) ? desktop.changes : [],
  };
  const desktopSliceErrors = desktop?.desktop_slice_errors && typeof desktop.desktop_slice_errors === "object"
    ? desktop.desktop_slice_errors as Record<string, { error?: unknown }>
    : {};
  const transactionError = ["route_drafts", "rollback_points"]
    .map((key) => desktopSliceErrors[key]?.error)
    .find((value) => typeof value === "string" && value.trim());
  const makeDesktopSlice = <T,>(data: T | null): OperationalSlice<T> => ({ data, source, state: desktopState, freshness, collectedAt, error: desktopError });
  return {
    service: {
      data: service,
      source: serviceVerified ? "dtoken-desktop-control-plane" : "unavailable",
      state: serviceVerified ? "verified" : "unavailable",
      freshness: serviceVerified ? "fresh" : "unknown",
      collectedAt,
      error: serviceVerified ? "" : safeErrorMessage(serviceResult.status === "rejected" ? serviceResult.reason : "service status unavailable"),
    },
    dashboard: makeDesktopSlice<Record<string, any>>(desktop),
    traffic: makeDesktopSlice(requestLogs),
    routes: makeDesktopSlice(routeData),
    transactions: transactionError
      ? { data: transactionData, source, state: "partial", freshness, collectedAt, error: safeErrorMessage(transactionError) }
      : makeDesktopSlice(transactionData),
    doctor: makeDesktopSlice((desktop?.doctor ?? desktop?.doctor_summary ?? desktop?.product_readiness ?? {}) as Record<string, any>),
    collectedAt,
  };
}

function serviceStateFromPayload(service: Record<string, any> | null): string {
  return typeof service?.state === "string" ? service.state : "unknown";
}

function safeSource(value: unknown): string {
  return typeof value === "string" && value.trim() ? value : "unavailable";
}

export function classifyRuntimePayload(next: Record<string, any>): Pick<RuntimeDashboardState, "status" | "error"> {
  const hasRuntimeIdentity = next?.source === "dtoken-owned-runtime" && next?.desktop_data_source === "dtoken-local-service";
  const evidence = classifyEvidence({ payload: next, status: "runtime", requireRuntimeIdentity: true });
  const status: RuntimeDashboardStatus = !hasRuntimeIdentity
    ? evidence === "stale" ? "stale" : next?.desktop_data_source === "dtoken-desktop-control-plane" ? "partial" : "unavailable"
    : evidence === "verified" ? "runtime" : evidence === "stale" || evidence === "partial" ? evidence : "unavailable";
  return { status, error: status === "runtime" ? "" : safeErrorMessage(next?.error ?? next?.desktop_service_error ?? "d-token 本地服务暂不可用") };
}

function safeErrorMessage(value: unknown) {
  if (value instanceof Error) return value.message;
  if (typeof value === "string" && value.trim()) return value;
  return "无法读取 d-token 本地服务";
}
