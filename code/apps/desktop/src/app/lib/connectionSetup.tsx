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
import { invokeNative, RUNTIME_INVALIDATED_EVENT } from "./native";
import { appRefreshScheduler, type ScheduledRefresh } from "./refreshScheduler";

export type UpstreamCandidate = {
  id: string;
  name: string;
  protocol: string;
  base_url?: string | null;
  models: string[];
  source: string;
  freshness: string;
  auth_mode: string;
  active: boolean;
  port?: number | null;
  state: string;
  agent_ids: string[];
  confidence: number;
  reasons: string[];
};

export type AgentUpstreamBinding = {
  agent_id: string;
  agent_name: string;
  installed: boolean;
  binary_path?: string | null;
  config_path?: string | null;
  current_config_source: string;
  candidates: UpstreamCandidate[];
  resolved_candidate?: UpstreamCandidate | null;
  resolution: "auto-resolved" | "already-connected" | "needs-choice" | "needs-credential" | "unsupported" | "unavailable";
  confidence: number;
  reasons: string[];
  capability: "one-click" | "confirm-required" | "prompt-only" | "manual-only";
  already_connected: boolean;
};

export type ConnectionSetupSnapshot = {
  source: string;
  freshness: string;
  revision: string;
  state: string;
  candidates: UpstreamCandidate[];
  agent_bindings: AgentUpstreamBinding[];
  sources: Array<Record<string, unknown>>;
  selection_required: boolean;
  cc_switch_sync?: {
    detected: boolean;
    status: string;
    profile?: string | null;
    upstream?: string | null;
    connection_path?: string | null;
    source: string;
    writes_performed: false;
  } | null;
  writes_performed: false;
  privacy: string;
};

type ConnectionSetupStore = {
  snapshot: ConnectionSetupSnapshot | null;
  status: "idle" | "discovering" | "ready" | "stale" | "unavailable";
  error: string;
  refresh: () => Promise<ConnectionSetupSnapshot | null>;
};

const ConnectionSetupContext = createContext<ConnectionSetupStore | null>(null);
// Upstream discovery may invoke provider CLIs. Runtime invalidation events keep
// writes responsive; this slower fallback avoids re-running expensive scans on
// every dashboard polling tick.
const DEFAULT_CONNECTION_POLL_INTERVAL_MS = 60_000;
const CONNECTION_SETUP_REFRESH_KEY = "connection-setup-discovery";

export function ConnectionSetupProvider({ children }: { children: ReactNode }) {
  const store = useConnectionSetupState(true);
  return createElement(ConnectionSetupContext.Provider, { value: store }, children);
}

export function useConnectionSetup(): ConnectionSetupStore {
  const shared = useContext(ConnectionSetupContext);
  const standalone = useConnectionSetupState(shared === null);
  return shared ?? standalone;
}

function useConnectionSetupState(enabled: boolean): ConnectionSetupStore {
  const [snapshot, setSnapshot] = useState<ConnectionSetupSnapshot | null>(null);
  const [status, setStatus] = useState<ConnectionSetupStore["status"]>("idle");
  const [error, setError] = useState("");
  const inFlight = useRef<Promise<ConnectionSetupSnapshot | null> | null>(null);
  const scheduledRefresh = useRef<ScheduledRefresh<ConnectionSetupSnapshot | null> | null>(null);
  const mounted = useRef(false);
  const snapshotRef = useRef<ConnectionSetupSnapshot | null>(null);

  const performRefresh = useCallback(() => {
    if (!enabled) return Promise.resolve(null);
    if (inFlight.current) return inFlight.current;
    setStatus((current) => current === "idle" || current === "unavailable" ? "discovering" : current);
    setError("");
    const task = Promise.resolve(invokeNative<ConnectionSetupSnapshot>("discover_upstream_candidates"))
      .then((next) => {
        if (!next || typeof next !== "object" || typeof next.revision !== "string") {
          throw new Error("connection_setup_unavailable");
        }
        if (next.state !== "loaded") {
          throw new Error("connection_setup_unavailable");
        }
        if (!mounted.current) return next;
        snapshotRef.current = next;
        setSnapshot(next);
        setStatus("ready");
        return next;
      })
      .catch((_reason) => {
        const message = "connection_setup_unavailable";
        if (!mounted.current) return null;
        setError(message);
        setStatus(snapshotRef.current ? "stale" : "unavailable");
        return null;
      })
      .finally(() => {
        inFlight.current = null;
      });
    inFlight.current = task;
    return task;
  }, [enabled]);

  const refresh = useCallback(
    () => scheduledRefresh.current?.refresh() ?? performRefresh(),
    [performRefresh],
  );

  useEffect(() => {
    if (!enabled) return;
    mounted.current = true;
    const scheduler = appRefreshScheduler.register(CONNECTION_SETUP_REFRESH_KEY, performRefresh, {
      fallbackDelayMs: () => DEFAULT_CONNECTION_POLL_INTERVAL_MS,
    });
    scheduledRefresh.current = scheduler;
    const handleInvalidation = () => {
      void scheduler.invalidate();
    };
    window.addEventListener(RUNTIME_INVALIDATED_EVENT, handleInvalidation);
    return () => {
      mounted.current = false;
      window.removeEventListener(RUNTIME_INVALIDATED_EVENT, handleInvalidation);
      scheduledRefresh.current = null;
      scheduler.release();
    };
  }, [enabled, performRefresh]);

  return useMemo(() => ({ snapshot, status, error, refresh }), [snapshot, status, error, refresh]);
}
