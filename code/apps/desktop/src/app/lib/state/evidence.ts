export type EvidenceState = "loading" | "verified" | "preview" | "unavailable" | "stale" | "partial";

type EvidenceOptions = {
  payload: Record<string, any> | null;
  status?: string;
  requireRuntimeIdentity?: boolean;
  requireTransactionProvenance?: boolean;
};

export function classifyEvidence({
  payload,
  status = "",
  requireRuntimeIdentity = false,
  requireTransactionProvenance = false,
}: EvidenceOptions): EvidenceState {
  const normalizedStatus = normalizeState(status);
  if (normalizedStatus === "loading") return "loading";
  if (normalizedStatus === "preview") return "preview";
  if (normalizedStatus === "unavailable") return "unavailable";
  if (normalizedStatus === "stale" || normalizedStatus === "partial") return normalizedStatus;

  const marker = explicitEvidenceMarker(payload);
  if (marker) return marker;
  if (!payload) return normalizedStatus === "runtime" || normalizedStatus === "native" ? "partial" : "unavailable";

  if (requireRuntimeIdentity || requireTransactionProvenance) {
    const source = String(payload.source ?? "");
    const desktopSource = String(payload.desktop_data_source ?? "");
    if (source !== "dtoken-owned-runtime" || desktopSource !== "dtoken-local-service") return "partial";
  }

  if (requireTransactionProvenance && !hasTransactionProvenance(payload)) return "partial";

  return "verified";
}

function hasTransactionProvenance(payload: Record<string, any>): boolean {
  const freshness = normalizeState(payload.freshness ?? payload.meta?.freshness);
  if (freshness !== "fresh" && freshness !== "current") return false;
  if (payload.complete !== true) return false;

  const provenance = payload.transaction_provenance;
  if (!provenance || typeof provenance !== "object" || Array.isArray(provenance)) return false;

  const schemaVersion = Number(provenance.schema_version);
  return provenance.source === "dtoken-owned-runtime"
    && provenance.command === "get_desktop_data"
    && Number.isInteger(schemaVersion)
    && schemaVersion >= 1
    && typeof provenance.generated_at === "string"
    && provenance.generated_at.trim().length > 0;
}

function explicitEvidenceMarker(payload: Record<string, any> | null): Exclude<EvidenceState, "loading" | "verified"> | null {
  if (!payload) return null;
  if (payload.stale === true || payload.is_stale === true) return "stale";
  if (payload.partial === true || payload.is_partial === true || payload.complete === false) return "partial";

  const candidates = [
    payload.evidence_state,
    payload.data_state,
    payload.data_status,
    payload.freshness,
    payload.runtime_status,
    payload.source_status,
    payload.status,
    payload.source,
    payload.meta?.freshness,
    payload.meta?.status,
  ];
  for (const candidate of candidates) {
    const normalized = normalizeState(candidate);
    if (normalized === "stale" || normalized === "partial" || normalized === "preview" || normalized === "unavailable") {
      return normalized;
    }
  }
  return null;
}

function normalizeState(value: unknown): string {
  const state = typeof value === "string" ? value.trim().toLowerCase().replace(/_/g, "-") : "";
  if (state === "browser-preview" || state === "preview") return "preview";
  if (state === "unavailable" || state === "error" || state === "offline") return "unavailable";
  if (state === "stale" || state === "expired" || state === "outdated") return "stale";
  if (state === "partial" || state === "incomplete" || state === "degraded") return "partial";
  return state;
}
