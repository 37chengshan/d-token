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

export type AgentRouteFollowReceipt = {
  schema_version: number;
  state: string;
  source_fingerprint: string;
  changed_agents: string[];
  route_before_sha256?: string | null;
  route_after_sha256?: string | null;
  rollback_id?: string | null;
  runtime_state: string;
  occurred_at_ms: number;
};

export type AgentRouteFollowStatus = {
  authorized: boolean;
  enabled: boolean;
  state: string;
  transport_out_of_sync: boolean;
  next_action:
    | "none"
    | "configure"
    | "advanced_configuration"
    | "verify_runtime"
    | "review_follow"
    | "enable_follow_in_connection"
    | "retry_review";
  current_upstream?: {
    provider_id: string;
    model: string;
    protocol: string;
    normalized_endpoint: string;
    profile_scope?: string | null;
  } | null;
  last_receipt?: AgentRouteFollowReceipt | null;
};

export type AgentUpstreamBinding = {
  agent_id: string;
  agent_name: string;
  installed: boolean;
  binary_path?: string | null;
  config_path?: string | null;
  config_paths?: string[];
  current_config_source: string;
  candidates: UpstreamCandidate[];
  resolved_candidate?: UpstreamCandidate | null;
  resolution: "auto-resolved" | "already-connected" | "needs-choice" | "needs-credential" | "unsupported" | "unavailable";
  confidence: number;
  reasons: string[];
  capability: "one-click" | "confirm-required" | "prompt-only" | "manual-only";
  // Native discovery always sends these. Missing values deliberately fail
  // closed so an older or malformed bridge cannot enable a write action.
  can_configure?: boolean;
  can_clear?: boolean;
  transport_supported?: boolean;
  transport_status?: "supported" | "configure_only" | "advanced_required" | "schema_unverified";
  transport_reason?: string | null;
  already_connected: boolean;
  follow?: AgentRouteFollowStatus;
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
    source_state?: string;
    profile?: string | null;
    upstream?: string | null;
    connection_path?: string | null;
    source: string;
    writes_performed: false;
  } | null;
  external_config_refresh?: {
    state: "external_config_snapshot" | "change_pending_debounce" | "external_config_review_required" | "unchanged" | "metadata_unavailable";
    mode: string;
    snapshot_fingerprint: string;
    previous_fingerprint?: string | null;
    changed_sources: string[];
    review_required: boolean;
    automatic_apply_allowed: false;
    next_poll_not_before_ms: number;
  };
  external_config_snapshot?: {
    fingerprint: string;
    cc_switch?: {
      fingerprint: string;
      revision: string;
      selection_state: string;
    };
    automatic_apply_allowed: false;
  };
  agent_route_follow?: {
    schema_version: number;
    enabled: boolean;
    state: string;
    authorized_at_ms: number;
    runtime_identity?: string | null;
    last_synced_source_fingerprint?: string | null;
    last_receipt?: AgentRouteFollowReceipt | null;
  };
  writes_performed: false;
  privacy: string;
};

type ConnectionSetupStore = {
  snapshot: ConnectionSetupSnapshot | null;
  status: "idle" | "discovering" | "ready" | "stale" | "unavailable";
  error: string;
  refresh: () => Promise<ConnectionSetupSnapshot | null>;
};

type ConnectionSetupContextValue = ConnectionSetupStore & { activate: () => () => void };

const ConnectionSetupContext = createContext<ConnectionSetupContextValue | null>(null);
// Upstream discovery may invoke provider CLIs. Runtime invalidation events keep
// writes responsive; this slower fallback avoids re-running expensive scans on
// every dashboard polling tick.
// External Agent/cc-switch edits do not emit d-token's runtime-invalidated
// event. Keep a bounded metadata-only fallback so provider/profile changes
// become visible quickly while the native coordinator remains fail-closed.
const DEFAULT_CONNECTION_POLL_INTERVAL_MS = 5_000;
const CONNECTION_SETUP_REFRESH_KEY = "connection-setup-discovery";

export function ConnectionSetupProvider({ children }: { children: ReactNode }) {
  // Discovery may invoke provider CLIs, so it stays idle until a page that
  // reads it mounts. Startup no longer waits on a scan nobody is showing.
  const [consumers, setConsumers] = useState(0);
  const activate = useCallback(() => {
    setConsumers((count) => count + 1);
    let released = false;
    return () => {
      if (released) return;
      released = true;
      setConsumers((count) => Math.max(0, count - 1));
    };
  }, []);
  const store = useConnectionSetupState(consumers > 0);
  const value = useMemo<ConnectionSetupContextValue>(
    () => ({ ...store, activate }),
    [store, activate],
  );
  return createElement(ConnectionSetupContext.Provider, { value }, children);
}

export function useConnectionSetup(): ConnectionSetupStore {
  const shared = useContext(ConnectionSetupContext);
  const standalone = useConnectionSetupState(shared === null);
  useEffect(() => shared?.activate(), [shared?.activate]);
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
  const failureCount = useRef(0);

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
        failureCount.current = 0;
        if (!mounted.current) return next;
        snapshotRef.current = next;
        setSnapshot(next);
        setStatus("ready");
        return next;
      })
      .catch((_reason) => {
        const message = "connection_setup_unavailable";
        failureCount.current += 1;
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
      // A failing coordinator should not be re-scanned every five seconds.
      fallbackDelayMs: () =>
        DEFAULT_CONNECTION_POLL_INTERVAL_MS * Math.min(8, 2 ** failureCount.current),
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
  }, [enabled, performRefresh]);

  return useMemo(() => ({ snapshot, status, error, refresh }), [snapshot, status, error, refresh]);
}
