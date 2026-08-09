import { Activity, Brain, Database, FileText, Gauge, Route, ShieldCheck, Sparkles } from "lucide-react";
import { Surface } from "./Surface";
import { StatusBadge } from "./StatusBadge";
import { Badge } from "../ui/badge";
import { translateCopy, useI18n, type Lang } from "../../lib/i18n";
import { classifyEvidence } from "../../lib/state/evidence";

type CapabilitySummaryProps = {
  payload: Record<string, any> | null;
  status: string;
  compact?: boolean;
};

type CapabilityRow = {
  key: string;
  icon: typeof Gauge;
  title: string;
  value: string;
  detail: string;
  status: "healthy" | "warning" | "error";
  statusLabel: string;
};

export function CapabilitySummary({ payload, status, compact = false }: CapabilitySummaryProps) {
  const { lang } = useI18n();
  const evidenceState = classifyEvidence({ payload, status, requireRuntimeIdentity: true });
  const verified = evidenceState === "verified";
  const stats = verified && isRecord(payload?.stats) ? payload.stats : {};
  const metrics = verified && isRecord(payload?.metrics) ? payload.metrics : {};
  const cache = verified && isRecord(payload?.cache_summary) ? payload.cache_summary : null;
  const memory = verified && isRecord(payload?.memory) ? payload.memory : null;
  const runtime = verified && isRecord(payload?.runtime) ? payload.runtime : {};
  const outputShape = verified && isRecord(payload?.output_shape) ? payload.output_shape : null;
  const sections = verified && Array.isArray(payload?.dashboard_sections) ? payload.dashboard_sections : [];
  const requestTotal = optionalNumberAt(stats, ["requests", "total"]);
  const tokensSaved = optionalNumberAt(stats, ["tokens", "saved"]);
  const ccrEntries = optionalNumber(cache?.entries);
  const memoryEntries = optionalNumber(memory?.entries);
  const transforms = Array.isArray(metrics?.compression_performance?.transforms)
    ? metrics.compression_performance.transforms.length
    : null;
  const routeRows = Array.isArray(metrics?.route_performance?.routes)
    ? metrics.route_performance.routes.length
    : null;
  const rtkMode = typeof runtime.rtk === "string" ? runtime.rtk : null;
  const noEvidence = unavailableCopy(evidenceState, lang);

  const rows: CapabilityRow[] = [
    {
      key: "compression",
      icon: Gauge,
      title: translateCopy("cap.requestCompression", lang),
      value: verified && requestTotal !== null && requestTotal > 0 && tokensSaved !== null
        ? translateCopy("cap.estimatedTokensSaved", lang, { v1: tokensSaved.toLocaleString("en-US") })
        : "—",
      detail: verified && requestTotal !== null && requestTotal > 0
        ? translateCopy("cap.localRequests", lang, { v1: requestTotal, v2: transforms === null ? "" : ` · ${transforms} 类处理步骤`, v3: transforms === null ? "" : ` · ${transforms} processing steps` })
        : verified ? (translateCopy("cap.noRealRequestRecords", lang)) : noEvidence,
      status: verified && requestTotal !== null && requestTotal > 0 && tokensSaved !== null ? "healthy" : "warning",
      statusLabel: verified && requestTotal !== null && requestTotal > 0 && tokensSaved !== null
        ? translateCopy("cap.recorded", lang)
        : translateCopy("cap.waiting", lang),
    },
    {
      key: "ccr",
      icon: Database,
      title: translateCopy("cap.recoverableCompressionIndex", lang),
      value: ccrEntries === null ? "—" : translateCopy("cap.indexes", lang, { v1: ccrEntries }),
      detail: cache
        ? rawAvailabilityDetail(cache, lang)
        : verified ? (translateCopy("cap.noLocalIndexStatusReturned", lang)) : noEvidence,
      status: rawAvailable(cache) === true ? "error" : rawAvailable(cache) === false ? "healthy" : "warning",
      statusLabel: cache
        ? rawAvailable(cache) === true
          ? translateCopy("cap.needsAttention", lang)
          : rawAvailable(cache) === false
            ? translateCopy("cbm.state.ready", lang)
            : translateCopy("cap.unverified", lang)
        : translateCopy("cap.noEvidence", lang),
    },
    {
      key: "memory",
      icon: Brain,
      title: translateCopy("cap.projectMemory", lang),
      value: memoryEntries === null ? "—" : translateCopy("cap.records", lang, { v1: memoryEntries }),
      detail: memory
        ? rawAvailabilityDetail(memory, lang)
        : verified ? (translateCopy("cap.noProjectMemoryStatusReturned", lang)) : noEvidence,
      status: rawAvailable(memory) === true ? "error" : rawAvailable(memory) === false ? "healthy" : "warning",
      statusLabel: memory
        ? rawAvailable(memory) === true
          ? translateCopy("cap.needsAttention", lang)
          : rawAvailable(memory) === false
            ? translateCopy("cbm.state.ready", lang)
            : translateCopy("cap.unverified", lang)
        : translateCopy("cap.noEvidence", lang),
    },
    {
      key: "output",
      icon: Sparkles,
      title: translateCopy("cap.outputReduction", lang),
      value: outputShape ? (translateCopy("cap.localStatusReturned", lang)) : "—",
      detail: outputShape
        ? translateCopy("cap.basedOnCurrentProjectPolicyAndLocalPreferences", lang)
        : verified ? (translateCopy("cap.noVerifiableOutputReductionStatus", lang)) : noEvidence,
      status: outputShape ? "healthy" : "warning",
      statusLabel: outputShape
        ? translateCopy("cap.loaded", lang)
        : translateCopy("cap.noEvidence", lang),
    },
    {
      key: "rtk",
      icon: Activity,
      title: translateCopy("cap.commandOutputReduction", lang),
      value: rtkMode === "integrated" ? (translateCopy("cap.builtInCapability", lang)) : "—",
      detail: rtkMode === "integrated"
        ? translateCopy("cap.implementationCapabilityNotProofOfLocalActivity", lang)
        : verified ? (translateCopy("cap.noCapabilityDeclarationReturned", lang)) : noEvidence,
      status: "warning",
      statusLabel: rtkMode === "integrated"
        ? translateCopy("cap.catalog", lang)
        : translateCopy("cap.noEvidence", lang),
    },
    {
      key: "route",
      icon: Route,
      title: translateCopy("cap.connectionPaths", lang),
      value: routeRows !== null && routeRows > 0
        ? translateCopy("cap.recordedPaths", lang, { v1: routeRows })
        : "—",
      detail: routeRows !== null && routeRows > 0
        ? translateCopy("cap.routeStatisticsFromLocalRequests", lang)
        : verified ? (translateCopy("cap.noRealRouteRecords", lang)) : noEvidence,
      status: routeRows !== null && routeRows > 0 ? "healthy" : "warning",
      statusLabel: routeRows !== null && routeRows > 0
        ? translateCopy("cap.recorded", lang)
        : translateCopy("cap.waiting", lang),
    },
    {
      key: "dashboard",
      icon: FileText,
      title: translateCopy("cap.localDataSurface", lang),
      value: sections.length > 0
        ? translateCopy("cap.dataGroups", lang, { v1: sections.length })
        : "—",
      detail: sections.length > 0
        ? translateCopy("cap.theLocalServiceReturnedItsAvailableDataScope", lang)
        : verified ? (translateCopy("cap.noDataScopeReturned", lang)) : noEvidence,
      status: sections.length > 0 ? "healthy" : "warning",
      statusLabel: sections.length > 0
        ? translateCopy("cap.loaded", lang)
        : translateCopy("cap.noEvidence", lang),
    },
    {
      key: "privacy",
      icon: ShieldCheck,
      title: translateCopy("cap.privacyBoundary", lang),
      value: privacyVerified(cache, memory)
        ? translateCopy("cap.rawContentUnavailableByDefault", lang)
        : "—",
      detail: privacyVerified(cache, memory)
        ? translateCopy("cap.rawPromptsResponsesAndCredentialsAreNotShown", lang)
        : verified ? (translateCopy("cap.completeLocalPrivacyStateIsMissing", lang)) : noEvidence,
      status: privacyVerified(cache, memory) ? "healthy" : "warning",
      statusLabel: privacyVerified(cache, memory)
        ? translateCopy("cbm.state.ready", lang)
        : translateCopy("cap.unverified", lang),
    },
  ];

  return (
    <Surface className={compact ? "p-4" : "p-5"} hover={false}>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div>
          <h3 className="text-sm font-semibold">{translateCopy("cap.localCapabilityStatus", lang)}</h3>
          <div className="text-xs text-muted-foreground">{capabilitySourceDetail(evidenceState, lang)}</div>
        </div>
        <Badge variant={verified ? "default" : "secondary"}>{capabilitySourceLabel(evidenceState, lang)}</Badge>
      </div>
      <div className={`grid gap-x-5 ${compact ? "md:grid-cols-2" : "sm:grid-cols-2 xl:grid-cols-4"}`}>
        {rows.map((row) => (
          <div key={row.key} data-flat-row className="border-t border-border/60 py-3">
            <div className="flex items-start justify-between gap-2">
              <div className="flex min-w-0 items-center gap-2">
                <row.icon className="size-4 shrink-0 text-primary" />
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{row.title}</div>
                  <div className="truncate text-xs text-muted-foreground" title={row.detail}>{row.detail}</div>
                </div>
              </div>
              <StatusBadge status={row.status} label={row.statusLabel} />
            </div>
            <div className="mt-2 text-xs text-muted-foreground">{row.value}</div>
          </div>
        ))}
      </div>
    </Surface>
  );
}

function optionalNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

function optionalNumberAt(root: any, path: string[]) {
  let value = root;
  for (const key of path) value = value?.[key];
  return optionalNumber(value);
}

function rawAvailable(value: Record<string, any> | null): boolean | null {
  return typeof value?.raw_available === "boolean" ? value.raw_available : null;
}

function privacyVerified(cache: Record<string, any> | null, memory: Record<string, any> | null) {
  return rawAvailable(cache) === false && rawAvailable(memory) === false;
}

function rawAvailabilityDetail(value: Record<string, any>, lang: Lang) {
  if (rawAvailable(value) === false) return translateCopy("cap.redactedIndexesAndSummariesOnlyByDefault", lang);
  if (rawAvailable(value) === true) return translateCopy("cap.rawContentIsLocallyAvailableCheckPrivacySettings", lang);
  return translateCopy("cap.rawContentAvailabilityWasNotReported", lang);
}

function unavailableCopy(state: string, lang: Lang) {
  if (state === "preview") return translateCopy("cap.browserPreviewDoesNotReadLocalCapabilityStatus", lang);
  if (state === "loading") return translateCopy("cap.loadingLocalServiceStatus", lang);
  if (state === "stale") return translateCopy("cap.localCapabilityDataIsStale", lang);
  if (state === "partial") return translateCopy("cap.localCapabilityDataIsIncomplete", lang);
  return translateCopy("cap.localCapabilityStatusUnavailable", lang);
}

function capabilitySourceDetail(state: string, lang: Lang) {
  if (state === "verified") return translateCopy("cap.onlyRuntimeEvidenceReturnedByTheLocalDTokenServiceIsShown", lang);
  return unavailableCopy(state, lang);
}

function capabilitySourceLabel(state: string, lang: Lang) {
  if (state === "verified") return translateCopy("cap.liveLocalData", lang);
  if (state === "preview") return translateCopy("cp.source.preview", lang);
  if (state === "loading") return translateCopy("adv.preferenceLoading", lang);
  if (state === "stale") return translateCopy("cap.staleData", lang);
  if (state === "partial") return translateCopy("cap.partialData", lang);
  return translateCopy("cap.unavailable", lang);
}

function isRecord(value: unknown): value is Record<string, any> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
