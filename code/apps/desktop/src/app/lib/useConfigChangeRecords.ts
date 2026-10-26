import { useCallback, useEffect, useRef, useState } from "react";
import { invokeNative, RUNTIME_INVALIDATED_EVENT } from "./native";

/** Configuration backups are local files, independent of model-service metrics. */
export function useConfigChangeRecords() {
  const [payload, setPayload] = useState<Record<string, any> | null>(null);
  const [status, setStatus] = useState("loading");
  const [error, setError] = useState("");
  const mounted = useRef(false);
  const hasPayload = useRef(false);
  const inFlight = useRef<Promise<void> | null>(null);
  const refresh = useCallback(() => {
    if (inFlight.current) return inFlight.current;
    const task = invokeNative<Record<string, any>>("get_config_change_records")
      .then((next) => {
        if (!mounted.current) return;
        const provenance = next?.transaction_provenance;
        if (next?.source !== "dtoken-desktop-control-plane"
          || provenance?.source !== "dtoken-desktop-control-plane"
          || provenance?.command !== "get_config_change_records"
          || provenance?.schema_version !== 1 || provenance?.runtime_required !== false
          || !Array.isArray(next.backupPoints)) throw new Error("invalid_configuration_records");
        hasPayload.current = true;
        setPayload(next);
        setStatus([next.freshness, next.data_status, next.evidence_state].includes("stale") ? "stale" : next.complete === true ? "native" : "partial");
        setError("");
      })
      .catch((reason: unknown) => {
        if (!mounted.current) return;
        if (reason instanceof Error && reason.message === "native_unavailable") {
          setStatus("browser-preview");
          setError("");
        } else {
          setStatus(hasPayload.current ? "stale" : "unavailable");
          setError("configuration_records_unavailable");
        }
      })
      .finally(() => { inFlight.current = null; });
    inFlight.current = task;
    return task;
  }, []);
  useEffect(() => {
    mounted.current = true;
    const invalidate = () => { void refresh(); };
    window.addEventListener(RUNTIME_INVALIDATED_EVENT, invalidate);
    void refresh();
    return () => {
      mounted.current = false;
      window.removeEventListener(RUNTIME_INVALIDATED_EVENT, invalidate);
    };
  }, [refresh]);
  return { payload, status, error, refresh };
}
