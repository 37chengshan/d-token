import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { invokeNative } from "./native";
import { appRefreshScheduler, type ScheduledRefresh } from "./refreshScheduler";
import { DesktopDataSchema, type DesktopData } from "./runtime/types";
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
  dashboard: OperationalSlice<DesktopData>;
  traffic: OperationalSlice<any[]>;
  routes: OperationalSlice<{ providers: any[]; routes: any[]; instances: any[]; decisions: any[] }>;
  transactions: OperationalSlice<{ events: any[]; backups: any[]; changes: any[] }>;
  doctor: OperationalSlice<Record<string, any>>;
  collectedAt: string | null;
};

export type RuntimeDashboardState = {
  payload: DesktopData | null;
  status: RuntimeDashboardStatus;
  updatedAt: Date | null;
  error: string;
  operational: RuntimeOperationalSnapshot;
  refresh: () => Promise<void>;
  invalidate: () => Promise<void>;
};

export type RuntimeDetailState = Pick<
  RuntimeDashboardState,
  "payload" | "status" | "updatedAt" | "error" | "refresh" | "invalidate"
>;
type RuntimeDetailContextValue = RuntimeDetailState & { activate: () => () => void };
type RuntimeDashboardStore = {
  getSnapshot: () => RuntimeDashboardState;
  setSnapshot: (next: RuntimeDashboardState) => void;
  subscribe: (listener: () => void) => () => void;
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
  dashboard: emptySlice<DesktopData>(),
  traffic: emptySlice<any[]>(),
  routes: emptySlice<{ providers: any[]; routes: any[]; instances: any[]; decisions: any[] }>(),
  transactions: emptySlice<{ events: any[]; backups: any[]; changes: any[] }>(),
  doctor: emptySlice<Record<string, any>>(),
  collectedAt: null,
});

const RuntimeEvidenceContext = createContext<RuntimeDashboardStore | null>(null);
const RuntimeDetailContext = createContext<RuntimeDetailContextValue | null>(null);
const DEFAULT_RUNTIME_POLL_INTERVAL_MS = 8000;
const RUNTIME_INVALIDATED_EVENT = "dtoken:runtime-invalidated";
const RUNTIME_SUMMARY_REFRESH_KEY = "runtime-dashboard-summary";

/**
 * Compares selected runtime slices field by field. Every poll rebuilds the
 * snapshot object, so a reference check would repaint each consumer on the
 * fallback interval even when the fields it reads are unchanged.
 */
export function shallowEqualSlice<T extends Record<string, unknown>>(left: T, right: T): boolean {
  const keys = Object.keys(left);
  if (keys.length !== Object.keys(right).length) return false;
  return keys.every((key) => Object.is(left[key], right[key]));
}

function createRuntimeDashboardStore(initial: RuntimeDashboardState): RuntimeDashboardStore {
  let snapshot = initial;
  const listeners = new Set<() => void>();
  return {
    getSnapshot: () => snapshot,
    setSnapshot: (next) => {
      snapshot = next;
      listeners.forEach((listener) => listener());
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

export function RuntimeEvidenceProvider({ children }: { children: ReactNode }) {
  const state = useRuntimeEvidence(DEFAULT_RUNTIME_POLL_INTERVAL_MS, true);
  const storeRef = useRef<RuntimeDashboardStore | null>(null);
  if (!storeRef.current) storeRef.current = createRuntimeDashboardStore(state);
  useEffect(() => {
    storeRef.current?.setSnapshot(state);
  }, [state]);
  const [detailConsumers, setDetailConsumers] = useState(0);
  const activateDetail = useCallback(() => {
    setDetailConsumers((count) => count + 1);
    let released = false;
    return () => {
      if (released) return;
      released = true;
      setDetailConsumers((count) => Math.max(0, count - 1));
    };
  }, []);
  const detail = useRuntimeDetailEvidence(
    typeof state.payload?.desktop_revision === "string" ? state.payload.desktop_revision : "",
    detailConsumers > 0,
  );
  const detailValue = useMemo(
    () => ({ ...detail, activate: activateDetail }),
    [activateDetail, detail],
  );
  return createElement(
    RuntimeEvidenceContext.Provider,
    { value: storeRef.current },
    createElement(RuntimeDetailContext.Provider, { value: detailValue }, children),
  );
}

/**
 * One app-wide snapshot. Pages must not infer service state from mount-local
 * defaults. Prefer useRuntimeDashboardSelector with an explicit slice: this
 * hook observes every field, so each poll repaints the caller.
 */
export function useRuntimeDashboard(intervalMs = DEFAULT_RUNTIME_POLL_INTERVAL_MS): RuntimeDashboardState {
  const store = useContext(RuntimeEvidenceContext);
  const standalone = useRuntimeEvidence(intervalMs, store === null);
  const shared = useRuntimeDashboardSelector((state) => state, Object.is, store);
  return store ? shared : standalone;
}

/**
 * Subscribe to one runtime slice so unrelated snapshot updates do not repaint
 * the consumer. Without a provider the caller still needs real evidence, so a
 * mount-local poll backs the selector instead of an empty snapshot.
 */
export function useRuntimeDashboardSelector<T>(
  selector: (state: RuntimeDashboardState) => T,
  isEqual: (left: T, right: T) => boolean = Object.is,
  suppliedStore?: RuntimeDashboardStore | null,
): T {
  const contextStore = useContext(RuntimeEvidenceContext);
  const store = suppliedStore === undefined ? contextStore : suppliedStore;
  const standaloneEnabled = suppliedStore === undefined && contextStore === null;
  const standalone = useRuntimeEvidence(DEFAULT_RUNTIME_POLL_INTERVAL_MS, standaloneEnabled);
  const selectedRef = useRef<T | undefined>(undefined);
  const selectedInitializedRef = useRef(false);
  const selectorRef = useRef(selector);
  const equalRef = useRef(isEqual);
  selectorRef.current = selector;
  equalRef.current = isEqual;
  const fallback = useRef<RuntimeDashboardState | null>(null);
  if (!fallback.current) {
    fallback.current = {
      payload: null,
      status: "loading",
      updatedAt: null,
      error: "",
      operational: emptyOperationalSnapshot(),
      refresh: async () => {},
      invalidate: async () => {},
    };
  }
  const fallbackStoreRef = useRef<RuntimeDashboardStore | null>(null);
  if (!fallbackStoreRef.current) fallbackStoreRef.current = createRuntimeDashboardStore(fallback.current);
  const activeStore = store ?? fallbackStoreRef.current;
  const currentSelectionSource = standaloneEnabled ? standalone : activeStore.getSnapshot();
  const currentSelection = selector(currentSelectionSource);
  if (
    !selectedInitializedRef.current
    || !equalRef.current(selectedRef.current as T, currentSelection)
  ) {
    selectedRef.current = currentSelection;
    selectedInitializedRef.current = true;
  }

  const subscribe = useCallback((notify: () => void) => activeStore.subscribe(() => {
    const next = selectorRef.current(activeStore.getSnapshot());
    if (!equalRef.current(selectedRef.current as T, next)) {
      selectedRef.current = next;
      notify();
    }
  }), [activeStore]);

  const subscribed = useSyncExternalStore(subscribe, () => selectedRef.current as T, () => selectedRef.current as T);
  return standaloneEnabled ? selector(standalone) : subscribed;
}

/** Full logs, events and transaction history are loaded only by pages that need them. */
export function useRuntimeDashboardDetail(): RuntimeDetailState {
  const shared = useContext(RuntimeDetailContext);
  const standalone = useRuntimeDetailEvidence("", shared === null);
  useEffect(() => shared?.activate(), [shared?.activate]);
  return shared ?? standalone;
}

function useRuntimeEvidence(intervalMs: number, enabled: boolean): RuntimeDashboardState {
  const [payload, setPayload] = useState<DesktopData | null>(null);
  const [status, setStatus] = useState<RuntimeDashboardStatus>("loading");
  const [operational, setOperational] = useState<RuntimeOperationalSnapshot>(emptyOperationalSnapshot);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [error, setError] = useState("");
  const mounted = useRef(false);
  const inFlightRefresh = useRef<Promise<void> | null>(null);
  const scheduledRefresh = useRef<ScheduledRefresh<void> | null>(null);
  const failureCount = useRef(0);
  const operationalRef = useRef<RuntimeOperationalSnapshot>(emptyOperationalSnapshot());
  const hasVerifiedPayloadRef = useRef(false);
  const snapshotKeyRef = useRef("");

  const performRefresh = useCallback(() => {
    if (!enabled) return Promise.resolve();
    if (inFlightRefresh.current) return inFlightRefresh.current;

    const task = (async () => {
      const collectedAt = new Date().toISOString();
      const [desktopResult, serviceResult] = await Promise.allSettled([
        invokeNative<unknown>("get_desktop_summary"),
        invokeNative<Record<string, any>>("get_dtoken_service_status"),
      ]);
      if (!mounted.current) return;

      const nativeUnavailable = [desktopResult, serviceResult].some(
        (result) => result.status === "rejected" && result.reason instanceof Error && result.reason.message === "native_unavailable",
      );
      if (nativeUnavailable) {
        if (snapshotKeyRef.current !== "browser-preview") {
          snapshotKeyRef.current = "browser-preview";
          setStatus("browser-preview");
          setError("");
        }
        failureCount.current = 0;
        return;
      }

      const parsedDesktop = desktopResult.status === "fulfilled"
        ? DesktopDataSchema.safeParse(desktopResult.value)
        : null;
      const desktop = parsedDesktop?.success ? parsedDesktop.data : null;
      const service = serviceResult.status === "fulfilled" ? serviceResult.value : null;
      const serviceState = serviceStateFromPayload(service);
      const classification = desktop
        ? classifyRuntimePayload(desktop)
        : parsedDesktop && !parsedDesktop.success
          ? { status: "partial" as const, error: "desktop_payload_invalid" }
          : { status: "unavailable" as const, error: safeErrorMessage(desktopResult.status === "rejected" ? desktopResult.reason : "") };
      const retainVerifiedPayload = hasVerifiedPayloadRef.current && classification.status !== "runtime";

      const nextStatus: RuntimeDashboardStatus = retainVerifiedPayload
        ? "stale"
        : classification.status === "runtime"
        ? "runtime"
        : serviceState === "running" || serviceState === "unmanaged_running"
          ? "partial"
          : classification.status;
      const nextError = nextStatus === "runtime" ? "" : classification.error;
      const nextOperational = buildOperationalSnapshot(
        desktop,
        service,
        collectedAt,
        desktopResult,
        serviceResult,
        operationalRef.current,
      );
      const desktopRevision = typeof desktop?.desktop_revision === "string"
        ? desktop.desktop_revision
        : desktop ? `legacy:${safeStableString(desktop)}` : `missing:${nextError}`;
      const serviceRevision = safeStableString({
        state: service?.state,
        managed: service?.managed,
        pid: service?.pid,
        pid_running: service?.pid_running,
        manifest_stale: service?.manifest_stale,
        restart_required: service?.restart_required,
        service_url: service?.service_url,
        manifest_identity_ok: service?.manifest_identity_ok,
        executable_owned: service?.executable_owned,
        port_owned: service?.port_owned,
      });
      const snapshotKey = `${desktopRevision}|${serviceRevision}|${nextStatus}|${nextError}`;
      if (snapshotKey !== snapshotKeyRef.current) {
        snapshotKeyRef.current = snapshotKey;
        if (classification.status === "runtime" && desktop) {
          hasVerifiedPayloadRef.current = true;
          setPayload(desktop);
        } else if (!hasVerifiedPayloadRef.current && desktop) {
          setPayload(desktop);
        }
        operationalRef.current = nextOperational;
        setOperational(nextOperational);
        setStatus(nextStatus);
        if (!retainVerifiedPayload) {
          const nativeCollectedAt = typeof desktop?.desktop_collected_at === "string" ? desktop.desktop_collected_at : collectedAt;
          setUpdatedAt(parseNativeTimestamp(nativeCollectedAt, collectedAt));
        }
        setError(nextError);
      }
      failureCount.current = nextStatus === "runtime" || nextStatus === "partial" ? 0 : failureCount.current + 1;
    })().finally(() => {
      inFlightRefresh.current = null;
    });

    inFlightRefresh.current = task;
    return task;
  }, [enabled]);

  const refresh = useCallback(
    () => scheduledRefresh.current?.refresh() ?? performRefresh(),
    [performRefresh],
  );

  useEffect(() => {
    if (!enabled) return;
    mounted.current = true;
    const scheduler = appRefreshScheduler.register(RUNTIME_SUMMARY_REFRESH_KEY, performRefresh, {
      fallbackDelayMs: () => intervalMs * Math.min(8, 2 ** failureCount.current),
    });
    scheduledRefresh.current = scheduler;
    const handleInvalidation = () => {
      failureCount.current = 0;
      void scheduler.invalidate();
    };
    window.addEventListener(RUNTIME_INVALIDATED_EVENT, handleInvalidation);
    return () => {
      mounted.current = false;
      window.removeEventListener(RUNTIME_INVALIDATED_EVENT, handleInvalidation);
      scheduledRefresh.current = null;
      scheduler.release();
    };
  }, [enabled, intervalMs, performRefresh]);

  return useMemo(
    () => ({ payload, status, operational, updatedAt, error, refresh, invalidate: refresh }),
    [payload, status, operational, updatedAt, error, refresh],
  );
}

function useRuntimeDetailEvidence(summaryRevision: string, enabled: boolean): RuntimeDetailState {
  const [payload, setPayload] = useState<DesktopData | null>(null);
  const [status, setStatus] = useState<RuntimeDashboardStatus>("loading");
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [error, setError] = useState("");
  const mounted = useRef(false);
  const inFlightRefresh = useRef<Promise<void> | null>(null);
  const snapshotKeyRef = useRef("");
  const hasPayloadRef = useRef(false);
  const hasVerifiedPayloadRef = useRef(false);
  const summaryRevisionRef = useRef(summaryRevision);

  const refresh = useCallback(() => {
    if (!enabled) return Promise.resolve();
    if (inFlightRefresh.current) return inFlightRefresh.current;
    const collectedAt = new Date().toISOString();
    const task = invokeNative<unknown>("get_desktop_data")
      .then((rawDesktop) => {
        if (!mounted.current) return;
        const parsedDesktop = DesktopDataSchema.safeParse(rawDesktop);
        if (!parsedDesktop.success) {
          setStatus("partial");
          setError("desktop_payload_invalid");
          return;
        }
        const desktop = parsedDesktop.data;
        const classification = classifyRuntimePayload(desktop);
        const retainVerifiedPayload = hasVerifiedPayloadRef.current && classification.status !== "runtime";
        const revision = typeof desktop.desktop_revision === "string"
          ? desktop.desktop_revision
          : safeStableString(desktop);
        const nextStatus = retainVerifiedPayload ? "stale" : classification.status;
        const snapshotKey = `${revision}|${nextStatus}|${classification.error}`;
        if (snapshotKey === snapshotKeyRef.current) return;
        snapshotKeyRef.current = snapshotKey;
        if (!retainVerifiedPayload) {
          hasPayloadRef.current = true;
          hasVerifiedPayloadRef.current = classification.status === "runtime";
          setPayload(desktop);
        }
        setStatus(nextStatus);
        setError(classification.error);
        if (!retainVerifiedPayload) {
          const nativeCollectedAt = typeof desktop.desktop_collected_at === "string"
            ? desktop.desktop_collected_at
            : collectedAt;
          setUpdatedAt(parseNativeTimestamp(nativeCollectedAt, collectedAt));
        }
      })
      .catch((reason) => {
        if (!mounted.current) return;
        const message = safeErrorMessage(reason);
        if (reason instanceof Error && reason.message === "native_unavailable") {
          setStatus("browser-preview");
          setError("");
        } else if (hasPayloadRef.current) {
          setStatus("stale");
          setError(message);
        } else {
          setStatus("unavailable");
          setError(message);
        }
      })
      .finally(() => {
        inFlightRefresh.current = null;
      });
    inFlightRefresh.current = task;
    return task;
  }, [enabled]);

  useEffect(() => {
    if (!enabled) return;
    mounted.current = true;
    const handleInvalidation = () => {
      if (document.visibilityState !== "hidden") void refresh();
    };
    window.addEventListener(RUNTIME_INVALIDATED_EVENT, handleInvalidation);
    void refresh();
    return () => {
      mounted.current = false;
      window.removeEventListener(RUNTIME_INVALIDATED_EVENT, handleInvalidation);
    };
  }, [enabled, refresh]);

  useEffect(() => {
    if (!enabled || !summaryRevision || summaryRevisionRef.current === summaryRevision) return;
    summaryRevisionRef.current = summaryRevision;
    if (document.visibilityState !== "hidden" && hasPayloadRef.current) void refresh();
  }, [enabled, refresh, summaryRevision]);

  return useMemo(
    () => ({ payload, status, updatedAt, error, refresh, invalidate: refresh }),
    [payload, status, updatedAt, error, refresh],
  );
}

function parseNativeTimestamp(value: string, fallback: string): Date {
  const unixMatch = /^unix:(\d+)$/.exec(value.trim());
  const parsed = unixMatch ? new Date(Number(unixMatch[1]) * 1000) : new Date(value);
  if (Number.isFinite(parsed.getTime())) return parsed;
  const fallbackDate = new Date(fallback);
  return Number.isFinite(fallbackDate.getTime()) ? fallbackDate : new Date();
}

function buildOperationalSnapshot(
  desktop: DesktopData | null,
  service: Record<string, any> | null,
  collectedAt: string,
  desktopResult: PromiseSettledResult<unknown>,
  serviceResult: PromiseSettledResult<Record<string, any>>,
  previous: RuntimeOperationalSnapshot,
): RuntimeOperationalSnapshot {
  const evidence = desktop ? classifyEvidence({ payload: desktop, status: "runtime", requireRuntimeIdentity: true }) : "unavailable";
  const runtimeVerified = evidence === "verified";
  const desktopState: OperationalSliceState = runtimeVerified ? "verified" : evidence === "stale" ? "stale" : desktop ? "partial" : "unavailable";
  const source = safeSource(desktop?.desktop_data_source ?? desktop?.source);
  const freshness = typeof desktop?.freshness === "string" ? desktop.freshness : runtimeVerified ? "fresh" : "local";
  const desktopError = runtimeVerified ? "" : safeErrorMessage(desktop?.error ?? desktop?.desktop_service_error ?? (desktopResult.status === "rejected" ? desktopResult.reason : ""));
  const serviceState = serviceStateFromPayload(service);
  const serviceVerified = Boolean(service && ["running", "unmanaged_running", "stopped"].includes(serviceState));
  const serviceSlice: OperationalSlice<Record<string, any>> = {
    data: service,
    source: serviceVerified ? "dtoken-desktop-control-plane" : "unavailable",
    state: serviceVerified ? "verified" : "unavailable",
    freshness: serviceVerified ? "fresh" : "unknown",
    collectedAt,
    error: serviceVerified ? "" : safeErrorMessage(serviceResult.status === "rejected" ? serviceResult.reason : "service status unavailable"),
  };
  if ((!desktop || !runtimeVerified) && previous.dashboard.data !== null && ["verified", "stale"].includes(previous.dashboard.state)) {
    const staleSlice = <T,>(slice: OperationalSlice<T>): OperationalSlice<T> => ({
      ...slice,
      state: "stale",
      error: desktopError || safeErrorMessage(desktopResult.status === "rejected" ? desktopResult.reason : "runtime snapshot refresh failed"),
    });
    return {
      ...previous,
      service: serviceSlice,
      dashboard: staleSlice(previous.dashboard),
      traffic: staleSlice(previous.traffic),
      routes: staleSlice(previous.routes),
      transactions: staleSlice(previous.transactions),
      doctor: staleSlice(previous.doctor),
    };
  }
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
    service: serviceSlice,
    dashboard: makeDesktopSlice<DesktopData>(desktop),
    traffic: makeDesktopSlice(requestLogs),
    routes: makeDesktopSlice(routeData),
    transactions: transactionError
      ? { data: transactionData, source, state: "partial", freshness, collectedAt, error: safeErrorMessage(transactionError) }
      : makeDesktopSlice(transactionData),
    doctor: makeDesktopSlice((desktop?.doctor ?? desktop?.doctor_summary ?? desktop?.product_readiness ?? {}) as Record<string, any>),
    collectedAt,
  };
}

function safeStableString(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function serviceStateFromPayload(service: Record<string, any> | null): string {
  return typeof service?.state === "string" ? service.state : "unknown";
}

function safeSource(value: unknown): string {
  return typeof value === "string" && value.trim() ? value : "unavailable";
}

export function classifyRuntimePayload(next: Record<string, unknown>): Pick<RuntimeDashboardState, "status" | "error"> {
  const hasRuntimeIdentity = next?.source === "dtoken-owned-runtime" && next?.desktop_data_source === "dtoken-local-service";
  const evidence = classifyEvidence({ payload: next, status: "runtime", requireRuntimeIdentity: true });
  const status: RuntimeDashboardStatus = !hasRuntimeIdentity
    ? evidence === "stale" ? "stale" : next?.desktop_data_source === "dtoken-desktop-control-plane" ? "partial" : "unavailable"
    : evidence === "verified" ? "runtime" : evidence === "stale" || evidence === "partial" ? evidence : "unavailable";
  return { status, error: status === "runtime" ? "" : safeErrorMessage(next?.error ?? next?.desktop_service_error ?? "d-token 本地服务暂不可用") };
}

function safeErrorMessage(_value: unknown) {
  // The source value may be a native exception or an upstream summary. UI
  // consumers use this only as an availability signal and must not render it.
  return "RUNTIME_UNAVAILABLE";
}
