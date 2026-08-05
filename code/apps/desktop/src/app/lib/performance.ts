export type PerformanceResultState = "ok" | "error" | "unsupported";

export type PerformanceObservation = {
  schema_version: 1;
  perf_run_id: string;
  layer: "webview";
  operation: string;
  phase: string;
  duration_us: number;
  result_state: PerformanceResultState;
  workload_count?: number;
  payload_bytes?: number;
  error_code?: string;
};

const MAX_OBSERVATIONS = 256;
const observations: PerformanceObservation[] = [];
const perfRunId = createRunId();
let initialized = false;
let longTaskObserver: PerformanceObserver | null = null;

declare global {
  interface Window {
    __DTOKEN_PERFORMANCE__?: {
      snapshot: () => PerformanceObservation[];
    };
  }
}

function createRunId() {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `perf-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function performanceNow() {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}

export function recordPerformanceObservation(
  observation: Omit<PerformanceObservation, "schema_version" | "perf_run_id" | "layer">,
) {
  observations.push({
    schema_version: 1,
    perf_run_id: perfRunId,
    layer: "webview",
    ...observation,
    duration_us: Math.max(0, Math.round(observation.duration_us)),
  });
  if (observations.length > MAX_OBSERVATIONS) {
    observations.splice(0, observations.length - MAX_OBSERVATIONS);
  }
}

export function recordPerformanceDuration(
  operation: string,
  phase: string,
  startedAtMs: number,
  result_state: PerformanceResultState,
  error_code?: string,
) {
  recordPerformanceObservation({
    operation,
    phase,
    duration_us: (performanceNow() - startedAtMs) * 1_000,
    result_state,
    ...(error_code ? { error_code } : {}),
  });
}

export function markPerformancePhase(operation: string, phase: string) {
  if (typeof performance !== "undefined" && typeof performance.mark === "function") {
    performance.mark(`dtoken:${operation}:${phase}`);
  }
  recordPerformanceObservation({
    operation,
    phase,
    duration_us: 0,
    result_state: "ok",
  });
}

export function initializePerformanceCapture() {
  if (initialized) return;
  initialized = true;
  if (typeof window !== "undefined") {
    Object.defineProperty(window, "__DTOKEN_PERFORMANCE__", {
      configurable: true,
      enumerable: false,
      value: Object.freeze({ snapshot: getPerformanceSnapshot }),
    });
  }
  markPerformancePhase("app_boot", "script_start");

  if (
    typeof PerformanceObserver === "undefined" ||
    !PerformanceObserver.supportedEntryTypes?.includes("longtask")
  ) {
    recordPerformanceObservation({
      operation: "long_task",
      phase: "observer_support",
      duration_us: 0,
      result_state: "unsupported",
      error_code: "longtask_unsupported",
    });
    return;
  }

  longTaskObserver = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      recordPerformanceObservation({
        operation: "long_task",
        phase: "observed",
        duration_us: entry.duration * 1_000,
        result_state: "ok",
      });
    }
  });
  longTaskObserver.observe({ entryTypes: ["longtask"] });
}

export function getPerformanceSnapshot() {
  return observations.map((observation) => ({ ...observation }));
}

export function resetPerformanceCaptureForTests() {
  observations.length = 0;
  longTaskObserver?.disconnect();
  longTaskObserver = null;
  initialized = false;
  if (typeof window !== "undefined") {
    delete window.__DTOKEN_PERFORMANCE__;
  }
}
