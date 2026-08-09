import { useEffect, useMemo, useState } from "react";
import { motion } from "motion/react";
import {
  Activity,
  AlertTriangle,
  ArrowDownWideNarrow,
  Bot,
  Boxes,
  ChevronDown,
  ChevronUp,
  Coins,
  Gauge,
  GitBranch,
  Route,
  ScrollText,
  Timer,
  Database,
  Layers3,
  ListFilter,
} from "lucide-react";
import {
  Bar,
  BarChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
} from "recharts";
import { MetricStrip, Surface } from "../common/Surface";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { useI18n, useT, translateCopy } from "../../lib/i18n";
import { protocolLabel } from "../../lib/protocol";
import {
  buildAnalyticsSnapshot,
  type AnalyticsSnapshot,
  type BreakdownItem,
  type RequestLogRow,
} from "../../lib/analytics/snapshot";
import {
  estimateRequestInputCost,
  loadModelPriceCache,
  type ModelPriceCache,
} from "../../lib/analytics/pricing";
import { useSessionTab } from "../../lib/sessionTab";
import { useRuntimeDashboardSelector, shallowEqualSlice, type RuntimeDashboardState } from "../../lib/useRuntimeDashboard";
import { classifyEvidence } from "../../lib/state/evidence";
import { redactString } from "../../lib/security/redaction";
import { invokeNative, isTauriRuntime } from "../../lib/native";

const nf = (value: number) => value.toLocaleString("en-US");
const unavailableValue = "—";
const ANALYTICS_TABS = ["live", "history", "parity", "logs"] as const;
const REQUEST_PAGE_SIZE = 10;

type AnalyticsRequestPage = {
  source: string;
  freshness: string;
  revision?: number;
  items: RequestLogRow[];
  returned_count: number;
  total_count: number;
  has_more: boolean;
  next_cursor: string | null;
  warning?: string;
  coverage?: { loaded_limit?: number; complete?: boolean };
  error?: string;
};

/** Analytics reads the summary evidence only; other slices must not repaint it. */
type SummaryRuntimeSlice = {
  payload: RuntimeDashboardState["payload"];
  status: RuntimeDashboardState["status"];
  updatedAt: RuntimeDashboardState["updatedAt"];
};

const selectSummaryRuntime = (state: RuntimeDashboardState): SummaryRuntimeSlice => ({
  payload: state.payload,
  status: state.status,
  updatedAt: state.updatedAt,
});

export function Analytics() {
  const t = useT();
  const { lang } = useI18n();
  const [tab, setTab] = useSessionTab("d-token.analytics.tab", ANALYTICS_TABS, "live");
  const [requestPage, setRequestPage] = useState(0);
  const [expandedRequestId, setExpandedRequestId] = useState<string | null>(null);
  const [nativePageIndex, setNativePageIndex] = useState(0);
  const [nativePendingPageIndex, setNativePendingPageIndex] = useState<number | null>(null);
  const [nativeCursors, setNativeCursors] = useState<Array<string | null>>([null]);
  const [nativeRequestPage, setNativeRequestPage] = useState<AnalyticsRequestPage | null>(null);
  const [nativePageStatus, setNativePageStatus] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [nativePageError, setNativePageError] = useState<"invalid_cursor" | "invalid_evidence" | "unavailable" | null>(null);
  const [logSearch, setLogSearch] = useState("");
  const [logStatusFilter, setLogStatusFilter] = useState<"all" | "ok" | "failed" | "unknown">("all");
  const [logProviderFilter, setLogProviderFilter] = useState("all");
  const [priceCache, setPriceCache] = useState<ModelPriceCache | null>(null);
  const [priceCacheStatus, setPriceCacheStatus] = useState<"loading" | "ready" | "unavailable">("loading");
  useEffect(() => {
    let cancelled = false;
    void loadModelPriceCache().then((cache) => {
      if (cancelled) return;
      setPriceCache(cache);
      setPriceCacheStatus(cache ? "ready" : "unavailable");
    });
    return () => { cancelled = true; };
  }, []);
  const { payload, status: payloadStatus, updatedAt } = useRuntimeDashboardSelector(
    selectSummaryRuntime,
    shallowEqualSlice,
  );
  const evidenceState = useMemo(
    () => classifyEvidence({ payload, status: payloadStatus, requireRuntimeIdentity: true }),
    [payload, payloadStatus],
  );
  const hasLastVerifiedEvidence = evidenceState === "verified" || evidenceState === "stale";
  const snapshot = useMemo(() => {
    const logs = hasLastVerifiedEvidence && Array.isArray(payload?.request_logs)
      ? payload.request_logs as RequestLogRow[]
      : [];
    const recent = buildAnalyticsSnapshot(logs, {
      lang,
      source: hasLastVerifiedEvidence
        ? evidenceState === "stale" ? "tauri-command" : "dtoken-local-service"
        : evidenceState === "preview"
          ? "preview"
          : "empty",
    });
    return hasLastVerifiedEvidence
      ? applyRuntimeAggregate(recent, payload?.stats, lang, payload?.metrics)
      : recent;
  }, [evidenceState, hasLastVerifiedEvidence, lang, payload]);
  const trafficEvidenceAvailable = hasLastVerifiedEvidence && Array.isArray(payload?.request_logs);
  const nativeQueryPageIndex = nativePendingPageIndex ?? nativePageIndex;
  const nativeCursor = nativeCursors[nativeQueryPageIndex] ?? null;
  useEffect(() => {
    if (!hasLastVerifiedEvidence || !isTauriRuntime()) {
      setNativeRequestPage(null);
      setNativePageStatus("idle");
      setNativePageError(null);
      return;
    }
    let cancelled = false;
    setNativePageStatus("loading");
    void invokeNative<AnalyticsRequestPage>("get_analytics_request_page", { cursor: nativeCursor, limit: REQUEST_PAGE_SIZE })
      .then((page) => {
        if (cancelled) return;
        if (page?.warning === "invalid_cursor") {
          setNativeCursors([null]);
          setNativePageIndex(0);
          setNativePendingPageIndex(0);
          setNativeRequestPage(null);
          setNativePageStatus("error");
          setNativePageError("invalid_cursor");
          return;
        }
        if (page?.source === "unavailable") {
          setNativeRequestPage(null);
          setNativePendingPageIndex(null);
          setNativePageStatus("error");
          setNativePageError("unavailable");
          return;
        }
        if (page?.source === "dtoken-owned-runtime" && page.freshness === "fresh" && Number.isSafeInteger(page.revision) && Array.isArray(page.items)) {
          setNativeRequestPage(page);
          setNativePageIndex(nativeQueryPageIndex);
          setNativePendingPageIndex(null);
          setNativePageStatus("ready");
          setNativePageError(null);
          return;
        }
        setNativeRequestPage(null);
        setNativePendingPageIndex(null);
        setNativePageStatus("error");
        setNativePageError("invalid_evidence");
      })
      .catch(() => {
        if (!cancelled) {
          setNativeRequestPage(null);
          setNativePendingPageIndex(null);
          setNativePageStatus("error");
          setNativePageError("unavailable");
        }
      });
    return () => { cancelled = true; };
  }, [hasLastVerifiedEvidence, nativeCursor, nativeQueryPageIndex, payload?.desktop_revision]);
  const requestRows = useMemo(
    () => nativeRequestPage?.items ?? (trafficEvidenceAvailable && Array.isArray(payload?.request_logs) ? payload.request_logs as RequestLogRow[] : []),
    [nativeRequestPage, payload, trafficEvidenceAvailable],
  );
  const nativePagination = nativeRequestPage !== null;
  const requestTotal = nativeRequestPage?.total_count ?? requestRows.length;
  const requestPageCount = Math.max(1, Math.ceil(requestTotal / REQUEST_PAGE_SIZE));
  const currentRequestPage = nativePagination ? nativePageIndex : Math.min(requestPage, requestPageCount - 1);
  const visibleRequestRows = nativePagination ? requestRows : requestRows.slice(currentRequestPage * REQUEST_PAGE_SIZE, (currentRequestPage + 1) * REQUEST_PAGE_SIZE);
  const requestHasMore = nativePagination ? nativeRequestPage.has_more : currentRequestPage < requestPageCount - 1;
  const logProviders = useMemo(() => Array.from(new Set(requestRows.flatMap((row) => {
    const provider = row.provider_id ?? row.provider;
    return typeof provider === "string" && provider.trim() ? [provider.trim()] : [];
  }))).sort((left, right) => left.localeCompare(right)), [requestRows]);
  const filteredLogRows = useMemo(() => {
    const query = logSearch.trim().toLocaleLowerCase();
    return requestRows.filter((row) => {
      const status = requestLogStatus(row);
      const provider = row.provider_id ?? row.provider;
      if (logStatusFilter !== "all" && status !== logStatusFilter) return false;
      if (logProviderFilter !== "all" && provider !== logProviderFilter) return false;
      if (!query) return true;
      return [
        row.request_id,
        provider,
        row.model_id ?? row.model,
        row.input_protocol ?? row.protocol,
        row.output_protocol,
        status,
        row.error_type,
        row.error,
      ].some((value) => typeof value === "string" && value.toLocaleLowerCase().includes(query));
    });
  }, [logProviderFilter, logSearch, logStatusFilter, requestRows]);
  const logPageCount = nativePagination ? requestPageCount : Math.max(1, Math.ceil(filteredLogRows.length / REQUEST_PAGE_SIZE));
  const currentLogPage = nativePagination ? currentRequestPage : Math.min(requestPage, logPageCount - 1);
  const visibleLogRows = nativePagination
    ? filteredLogRows
    : filteredLogRows.slice(currentLogPage * REQUEST_PAGE_SIZE, (currentLogPage + 1) * REQUEST_PAGE_SIZE);
  const logHasMore = nativePagination ? requestHasMore : currentLogPage < logPageCount - 1;
  const nativeInitialLoading = nativePageStatus === "loading" && nativeRequestPage === null;
  const sliceRevisions = payload?.slice_revisions as { runtime_summary?: unknown } | undefined;
  const summaryRevision = typeof sliceRevisions?.runtime_summary === "number"
    ? sliceRevisions.runtime_summary
    : null;
  const requestRevision = typeof nativeRequestPage?.revision === "number" ? nativeRequestPage.revision : null;
  const requestRevisionAligned = summaryRevision === null || requestRevision === null
    ? null
    : summaryRevision === requestRevision;

  const sourceLabel = analyticsSourceLabel(evidenceState, snapshot.hasRequests, lang);
  const dataSourceMode = evidenceState === "verified"
    ? "runtime"
    : evidenceState === "preview" || evidenceState === "loading"
      ? "preview"
      : "unavailable";
  const requestEvidence = trafficEvidenceAvailable
    ? evidenceState === "stale"
      ? translateCopy("an.fromTheLastVerifiedLocalDTokenRequestRecords", lang)
      : translateCopy("ov.fromLocalDTokenRequestRecords", lang)
    : sourceLabel;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <h1 className="sr-only">{t("an.title")}</h1>
        <DataSourceBadge mode={dataSourceMode} label={sourceLabel} />
        <Badge variant={evidenceState === "verified" ? "default" : "secondary"}>
          {sourceLabel}{hasLastVerifiedEvidence && updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}
        </Badge>
      </div>

      <Surface className="p-3" hover={false}>
        <div className="flex items-start gap-3">
          <Activity className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <div>
            <div className="text-sm font-semibold">{translateCopy("an.analyticsDataStatus", lang)}</div>
            <div className="mt-0.5 text-xs text-muted-foreground">
              {analyticsStatusDetail(evidenceState, snapshot.hasRequests, lang)}
            </div>
          </div>
        </div>
      </Surface>

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="live">{translateCopy("an.valueOverview", lang)}</TabsTrigger>
          <TabsTrigger value="history">{translateCopy("an.trendHistory", lang)}</TabsTrigger>
          <TabsTrigger value="parity">{translateCopy("an.dataScope", lang)}</TabsTrigger>
          <TabsTrigger value="logs">{translateCopy("in.tab.logs", lang)}</TabsTrigger>
        </TabsList>

        <TabsContent value="live" className="mt-3 space-y-4">
          <MetricStrip
            data-testid="analytics-kpis"
            className="metric-strip-6"
          >
            <Metric
              index={0}
              icon={<ArrowDownWideNarrow className="size-3.5" />}
              label={translateCopy("tray.routedRequests", lang)}
              value={trafficEvidenceAvailable ? nf(snapshot.requestCount) : unavailableValue}
              detail={requestEvidence}
              color="var(--chart-2)"
            />
            <Metric
              index={1}
              icon={<Gauge className="size-3.5" />}
              label={translateCopy("an.estimatedInputTokens", lang)}
              value={trafficEvidenceAvailable ? optionalNumber(snapshot.estimatedInputTokens) : unavailableValue}
              detail={metricDetail(snapshot.estimatedInputTokens, sourceLabel, lang)}
              color="var(--chart-3)"
            />
            <Metric
              index={2}
              icon={<Coins className="size-3.5" />}
              label={translateCopy("tray.physicalTokenReduction", lang)}
              value={trafficEvidenceAvailable ? optionalNumber(snapshot.estimatedSavedTokens) : unavailableValue}
              detail={metricDetail(snapshot.estimatedSavedTokens, sourceLabel, lang)}
              color="var(--chart-accent)"
            />
            <Metric
              index={3}
              icon={<GitBranch className="size-3.5" />}
              label={translateCopy("tray.physicalReductionRate", lang)}
              value={trafficEvidenceAvailable && snapshot.estimatedSavingsRate !== null
                ? `${(snapshot.estimatedSavingsRate * 100).toFixed(1)}%`
                : unavailableValue}
              detail={metricDetail(snapshot.estimatedSavingsRate, sourceLabel, lang)}
              color="var(--chart-4)"
            />
            <Metric
              index={4}
              icon={<Bot className="size-3.5" />}
              label={translateCopy("an.estimatedOutputTokens", lang)}
              value={trafficEvidenceAvailable ? optionalNumber(snapshot.estimatedOutputTokens) : unavailableValue}
              detail={metricDetail(snapshot.estimatedOutputTokens, sourceLabel, lang)}
              color="var(--chart-5)"
            />
            <Metric
              index={5}
              icon={<AlertTriangle className="size-3.5" />}
              label={translateCopy("ov.requestErrors", lang)}
              value={trafficEvidenceAvailable ? nf(snapshot.errorCount) : unavailableValue}
              detail={requestEvidence}
              color={snapshot.errorCount > 0 ? "var(--destructive)" : "var(--muted-foreground)"}
            />
          </MetricStrip>

          {snapshot.warnings.length > 0 && (
            <Surface className="p-3" hover={false}>
              <div className="flex items-start gap-2 text-xs text-muted-foreground">
                <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-500" />
                <div className="space-y-1">
                  {snapshot.warnings.map((warning) => <div key={warning}>{warning}</div>)}
                </div>
              </div>
            </Surface>
          )}

          <div className="grid gap-3 lg:grid-cols-2">
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Timer className="size-4" />
                {translateCopy("an.compressionLatency", lang)}
              </h3>
              {snapshot.latency ? (
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                  <Summary label={translateCopy("an.recordsWithTiming", lang)} value={nf(snapshot.latency.sampleCount)} />
                  <Summary label={translateCopy("an.average", lang)} value={`${snapshot.latency.averageMs.toFixed(1)}ms`} />
                  <Summary label="p50" value={`${snapshot.latency.p50Ms.toFixed(1)}ms`} />
                  <Summary label="p95" value={`${snapshot.latency.p95Ms.toFixed(1)}ms`} />
                </div>
              ) : (
                <NoData
                  title={translateCopy("an.noCompressionLatencyRecords", lang)}
                  detail={translateCopy("an.recordedTimingCoversTheCompressionStageOnlyAndIsNotUsedToInferUpstreamOrEndToEndLatency", lang)}
                />
              )}
            </Surface>

            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Bot className="size-4" />
                {translateCopy("an.agentDistribution", lang)}
              </h3>
              <BreakdownList rows={snapshot.agentBreakdown} lang={lang} />
            </Surface>
          </div>

          <div className="grid gap-3 lg:grid-cols-2">
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Route className="size-4" />
                {translateCopy("an.routeDistribution", lang)}
              </h3>
              <BreakdownList rows={snapshot.routeBreakdown} lang={lang} />
            </Surface>
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <GitBranch className="size-4" />
                {translateCopy("an.providerDistribution", lang)}
              </h3>
              <BreakdownList rows={snapshot.upstreamBreakdown} lang={lang} />
            </Surface>
          </div>

          <Surface className="p-4" hover={false}>
            <div className="mb-3 flex items-center justify-between gap-2">
              <h3 className="flex items-center gap-2 text-sm">
                <Boxes className="size-4" />
                {translateCopy("an.recordedProcessingSteps", lang)}
              </h3>
              <Badge variant="secondary">
                {snapshot.compressionCount === null
                  ? unavailableValue
                  : `${nf(snapshot.compressionCount)} ${translateCopy("an.recordsWithSavings", lang)}`}
              </Badge>
            </div>
            <BreakdownList rows={snapshot.strategyBreakdown} lang={lang} showSaved={false} />
          </Surface>

          <div className="grid gap-3 lg:grid-cols-2">
            <AnalyticsBreakdownCard icon={<Layers3 className="size-4" />} title={translateCopy("an.model", lang)} rows={snapshot.modelBreakdown} lang={lang} />
            <AnalyticsBreakdownCard icon={<ListFilter className="size-4" />} title={translateCopy("an.inputProtocol", lang)} rows={snapshot.protocolBreakdown} lang={lang} />
            <AnalyticsBreakdownCard icon={<ListFilter className="size-4" />} title={translateCopy("an.project", lang)} rows={snapshot.projectBreakdown} lang={lang} />
            <AnalyticsBreakdownCard icon={<Route className="size-4" />} title={translateCopy("an.relayChainMode", lang)} rows={snapshot.chainBreakdown} lang={lang} />
          </div>

          <div className="grid gap-3 lg:grid-cols-2">
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm"><Database className="size-4" />{translateCopy("an.cacheAlignment", lang)}</h3>
              {snapshot.cache ? (
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                  <Summary label={translateCopy("an.aligned", lang)} value={nf(snapshot.cache.aligned)} />
                  <Summary label={translateCopy("an.misaligned", lang)} value={nf(snapshot.cache.misaligned)} />
                  <Summary label={translateCopy("an.reads", lang)} value={nf(snapshot.cache.reads)} />
                  <Summary label={translateCopy("an.writes", lang)} value={nf(snapshot.cache.writes)} />
                  <Summary label={translateCopy("an.misses", lang)} value={nf(snapshot.cache.misses)} />
                  <Summary label={translateCopy("an.busts", lang)} value={nf(snapshot.cache.busts)} />
                </div>
              ) : <NoData title={translateCopy("an.noCacheEvidence", lang)} detail={translateCopy("an.requestsContainNoCacheReadWriteOrAlignmentEvidence", lang)} />}
            </Surface>
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm"><Timer className="size-4" />{translateCopy("an.pipelineTiming", lang)}</h3>
              {snapshot.pipeline ? (
                <div className="grid grid-cols-2 gap-2">
                  <Summary label={translateCopy("an.compression", lang)} value={timingValue(snapshot.pipeline.compressionMs)} />
                  <Summary label={translateCopy("dr.layer.route", lang)} value={timingValue(snapshot.pipeline.routeMs)} />
                  <Summary label="TTFB" value={timingValue(snapshot.pipeline.upstreamTtfbMs)} />
                  <Summary label={translateCopy("an.endToEnd", lang)} value={timingValue(snapshot.pipeline.endToEndMs)} />
                </div>
              ) : <NoData title={translateCopy("an.noStageTiming", lang)} detail={translateCopy("an.endToEndLatencyIsNotInferredFromCompressionOnlyTiming", lang)} />}
            </Surface>
          </div>

          <div className="grid gap-3 lg:grid-cols-2">
            <AnalyticsBreakdownCard icon={<AlertTriangle className="size-4" />} title={translateCopy("an.failureReasons", lang)} rows={snapshot.errorBreakdown} lang={lang} />
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 text-sm">{translateCopy("an.wasteSignals", lang)}</h3>
              {snapshot.wasteSignals.length > 0 ? <div className="space-y-2">{snapshot.wasteSignals.map((signal) => <div key={signal.label} className="flat-inset flex items-center justify-between gap-3 p-2.5"><div><div className="text-sm">{signal.label}</div><div className="text-xs text-muted-foreground">{signal.detail}</div></div><Badge variant="secondary">{nf(signal.count)}</Badge></div>)}</div> : <NoData title={translateCopy("an.noWasteSignals", lang)} detail={translateCopy("an.requestLevelCacheOrTransformEvidenceIsRequired", lang)} />}
            </Surface>
          </div>

          <Surface className="p-4" hover={false}>
            <div className="mb-3 flex items-center justify-between gap-2"><h3 className="text-sm">{translateCopy("an.requestDetailBoundedPages", lang)}</h3><span className="text-xs text-muted-foreground">{requestTotal > 0 ? `${requestTotal} ${translateCopy(nativePagination ? "an.runtimeRecords" : "an.currentSnapshotRecords", lang)}` : sourceLabel}</span></div>
            {nativePageError && <div role="status" className="mb-2 text-xs text-amber-600 dark:text-amber-400">{nativePageError === "invalid_cursor" ? (translateCopy("an.thePageCursorExpiredFirstPageDataIsShown", lang)) : nativePageError === "invalid_evidence" ? (translateCopy("an.thePageResultLacksTrustedSourceAndFreshnessMarkersTheCurrentSummarySnapshotIsShown", lang)) : (translateCopy("an.requestPaginationIsUnavailableTheCurrentSummarySnapshotIsShown", lang))}</div>}
            {nativeRequestPage && <div className="mb-2 text-xs text-muted-foreground">{translateCopy("an.detailSourceRuntimeFresh", lang, { v1: requestRevision, v2: summaryRevision === null ? "" : `；摘要 r${summaryRevision}`, v3: requestRevisionAligned === false ? "。runtime 在两次读取间已更新，明细与摘要需分别解读。" : "", v4: summaryRevision === null ? "" : `; summary r${summaryRevision}`, v5: requestRevisionAligned === false ? ". The runtime changed between reads; interpret detail and summary as separate snapshots." : "" })}</div>}
            {nativeInitialLoading ? <div role="status" className="rounded-lg border border-dashed border-border/60 bg-muted/15 px-3 py-3 text-xs text-muted-foreground">{translateCopy("an.loadingBoundedRequestDetail", lang)}</div> : visibleRequestRows.length > 0 ? <>
              <div className="overflow-x-auto">
                <table className="min-w-[720px] w-full text-sm">
                  <thead>
                    <tr className="border-b border-border/50 text-left text-xs text-muted-foreground">
                      {[translateCopy("visual.col.time", lang), "Agent", translateCopy("an.model", lang), translateCopy("dr.layer.route", lang), translateCopy("an.saved", lang), translateCopy("visual.col.status", lang)].map((label) => <th key={label} className="whitespace-nowrap pb-2 font-normal">{label}</th>)}
                    </tr>
                  </thead>
                  <tbody>
                    {visibleRequestRows.map((row, index) => (
                      <tr key={`${row.request_id ?? "row"}-${row.timestamp_ms ?? index}`} className="border-b border-border/30 text-xs last:border-0">
                        <td className="whitespace-nowrap py-2 font-mono">{formatTimestamp(row.timestamp_ms)}</td>
                        <td className="max-w-32 truncate py-2">{safeCell(row.agent)}</td>
                        <td className="max-w-40 truncate py-2">{safeCell(row.model_id ?? row.model)}</td>
                        <td className="max-w-40 truncate py-2">{safeCell(row.route_id ?? row.route)}</td>
                        <td className="whitespace-nowrap py-2 tabular-nums">{row.tokens_saved ?? "—"}</td>
                        <td className="whitespace-nowrap py-2">{row.error || row.error_type ? <Badge variant="destructive">{translateCopy("an.error", lang)}</Badge> : <Badge variant="secondary">{translateCopy("an.ok", lang)}</Badge>}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {nativeRequestPage?.coverage?.complete === false && <div className="mt-2 text-xs text-muted-foreground">{translateCopy("an.detailCoverageIsLimitedToTheLatestRuntimeRecords", lang, { v1: nf(nativeRequestPage.coverage.loaded_limit ?? 10_000) })}</div>}
              {requestTotal > REQUEST_PAGE_SIZE && <RequestPaginationBar lang={lang} currentRequestPage={currentRequestPage} requestPageCount={requestPageCount} requestHasMore={requestHasMore} nativePageStatus={nativePageStatus} onPrevious={() => nativePagination ? setNativePendingPageIndex(Math.max(0, currentRequestPage - 1)) : setRequestPage(Math.max(0, currentRequestPage - 1))} onNext={() => { if (nativePagination) { const next = nativeRequestPage.next_cursor; if (!next) return; setNativeCursors((cursors) => [...cursors.slice(0, nativePageIndex + 1), next]); setNativePendingPageIndex(nativePageIndex + 1); } else { setRequestPage(Math.min(requestPageCount - 1, currentRequestPage + 1)); } }} />}
            </> : requestRows.length > 0 ? <div className="rounded-lg border border-dashed border-border/60 bg-muted/15 px-3 py-2 text-xs text-muted-foreground">{translateCopy("an.theCurrentSnapshotFitsOnOnePageRequestDimensionsAreShownInTheBreakdownsAbove", lang)}</div> : <NoData title={translateCopy("an.noRequestDetail", lang)} detail={translateCopy("an.thisTableShowsOnlyTheBoundedRuntimeSnapshotNotFullHistory", lang)} />}
          </Surface>
        </TabsContent>

        <TabsContent value="history" className="mt-3 space-y-4">
          <MetricStrip>
            <SummaryCard
              label={translateCopy("an.requestRecords", lang)}
              value={snapshot.hasRequests ? nf(snapshot.requestCount) : unavailableValue}
            />
            <SummaryCard
              label={translateCopy("an.estimatedSavedTokens", lang)}
              value={optionalNumber(snapshot.estimatedSavedTokens)}
            />
            <SummaryCard
              label={translateCopy("an.recordedDays", lang)}
              value={snapshot.trend30d.length > 0 ? nf(snapshot.trend30d.length) : unavailableValue}
            />
            <SummaryCard
              label={translateCopy("ov.requestErrors", lang)}
              value={snapshot.hasRequests ? nf(snapshot.errorCount) : unavailableValue}
            />
          </MetricStrip>

          <Surface className="p-4" hover={false}>
            <h3 className="mb-3 text-sm">{translateCopy("an.recordedDaysInTheLatest30DayWindow", lang)}</h3>
            {snapshot.trend30d.length > 0 ? (
              <div className="h-56" data-testid="analytics-trend">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={snapshot.trend30d}>
                    <XAxis dataKey="day" tick={{ fontSize: 10, fill: "var(--muted-foreground)" }} axisLine={false} tickLine={false} />
                    <Tooltip
                      cursor={{ fill: "var(--muted)" }}
                      contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }}
                    />
                    <Bar dataKey="requests" name={translateCopy("an.requests", lang)} fill="var(--chart-2)" radius={[4, 4, 0, 0]} />
                    <Bar dataKey="saved" name={translateCopy("an.estimatedSavedTokens2", lang)} fill="var(--chart-accent)" radius={[4, 4, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            ) : (
              <NoData
                title={translateCopy("an.noTrendRecords", lang)}
                detail={translateCopy("an.onlyRealRequestsWithValidTimestampsAppearHere", lang)}
              />
            )}
          </Surface>

          <Surface className="p-4" hover={false}>
            <h3 className="mb-3 text-sm">{translateCopy("an.dailySummary", lang)}</h3>
            {snapshot.trend30d.length > 0 ? (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-border/50 text-left text-xs text-muted-foreground">
                      <th className="pb-2 font-normal">{translateCopy("an.date", lang)}</th>
                      <th className="pb-2 font-normal">{translateCopy("an.requests", lang)}</th>
                      <th className="pb-2 font-normal">{translateCopy("an.estimatedSavedTokens2", lang)}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {snapshot.trend30d.map((point) => (
                      <tr key={point.day} className="border-b border-border/30 text-xs last:border-0">
                        <td className="py-2 font-mono">{point.day}</td>
                        <td className="py-2">{nf(point.requests)}</td>
                        <td className="py-2">{optionalNumber(point.saved)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <NoData
                title={translateCopy("an.noDailySummary", lang)}
                detail={translateCopy("an.connectTheLocalServiceAndSendRealRequests", lang)}
              />
            )}
          </Surface>
        </TabsContent>

        <TabsContent value="parity" className="mt-3">
          <Surface className="p-4" hover={false}>
            <h3 className="mb-1 text-sm">{translateCopy("an.currentDataCoverage", lang)}</h3>
            <p className="mb-3 text-xs text-muted-foreground">
              {translateCopy("an.dataScopeMetricsVerifiableFromTheCurrentLocalDTokenRequestSnapshotMissingMetricsRemainEmpty", lang)}
            </p>
            <div className="grid gap-2 sm:grid-cols-2">
              {coverageRows(snapshot, lang, priceCacheStatus).map((item) => (
                <div key={item.label} className="flat-inset flex items-center justify-between gap-2 p-2 text-sm">
                  <span>{item.label}</span>
                  <Badge variant={item.available ? "default" : "secondary"}>
                    {item.available
                      ? translateCopy("an.recorded", lang)
                      : translateCopy("an.noRecord", lang)}
                  </Badge>
                </div>
              ))}
            </div>
          </Surface>
        </TabsContent>

        <TabsContent value="logs" className="mt-3">
          <Surface className="p-4" hover={false}>
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <div>
                <h3 className="flex items-center gap-2 text-sm">
                  <ScrollText className="size-4" />
                  {translateCopy("an.requestLog", lang)}
                </h3>
                <p className="mt-1 text-xs text-muted-foreground">
                  {translateCopy("an.oneRealRuntimeRequestRecordPerLineMissingFieldsStayEmptyNeverEstimatedOrZeroFilled", lang)}
                </p>
              </div>
              <span className="text-xs text-muted-foreground">
                {requestTotal > 0
                  ? `${requestTotal} ${translateCopy(nativePagination ? "an.runtimeRecords" : "an.currentSnapshotRecords", lang)}`
                  : sourceLabel}
              </span>
            </div>
            <div className="mb-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-[minmax(0,1fr)_10rem_12rem_auto]">
              <label className="min-w-0">
                <span className="sr-only">{translateCopy("an.searchRequestLogs", lang)}</span>
                <input
                  type="search"
                  aria-label={translateCopy("an.searchRequestLogs", lang)}
                  value={logSearch}
                  onChange={(event) => { setLogSearch(event.target.value); if (!nativePagination) setRequestPage(0); }}
                  placeholder={translateCopy("an.searchRequestIdProviderModelProtocolOrStatus", lang)}
                  className="h-9 w-full min-w-0 rounded-md border border-input bg-background px-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
                />
              </label>
              <select aria-label={translateCopy("an.statusFilter", lang)} value={logStatusFilter} onChange={(event) => { setLogStatusFilter(event.target.value as typeof logStatusFilter); if (!nativePagination) setRequestPage(0); }} className="h-9 min-w-0 rounded-md border border-input bg-background px-2 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring">
                <option value="all">{translateCopy("an.allStatuses", lang)}</option>
                <option value="ok">{translateCopy("an.completed", lang)}</option>
                <option value="failed">{translateCopy("cbm.state.failed", lang)}</option>
                <option value="unknown">{translateCopy("cbm.state.unknown", lang)}</option>
              </select>
              <select aria-label={translateCopy("an.providerFilter", lang)} value={logProviderFilter} onChange={(event) => { setLogProviderFilter(event.target.value); if (!nativePagination) setRequestPage(0); }} className="h-9 min-w-0 rounded-md border border-input bg-background px-2 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring">
                <option value="all">{translateCopy("an.allProviders", lang)}</option>
                {logProviders.map((provider) => <option key={provider} value={provider}>{provider}</option>)}
              </select>
              <div className="flex h-9 items-center justify-end whitespace-nowrap text-xs text-muted-foreground">
                {translateCopy("an.showing", lang, { v1: filteredLogRows.length, v2: requestRows.length })}
              </div>
            </div>
            {nativePageError && <div role="status" className="mb-2 text-xs text-amber-600 dark:text-amber-400">{nativePageError === "invalid_cursor" ? (translateCopy("an.thePageCursorExpiredFirstPageDataIsShown", lang)) : nativePageError === "invalid_evidence" ? (translateCopy("an.thePageResultLacksTrustedSourceAndFreshnessMarkersTheCurrentSummarySnapshotIsShown", lang)) : (translateCopy("an.requestPaginationIsUnavailableTheCurrentSummarySnapshotIsShown", lang))}</div>}
            {nativeRequestPage && <div className="mb-2 text-xs text-muted-foreground">{translateCopy("an.detailSourceRuntimeFresh", lang, { v1: requestRevision, v2: summaryRevision === null ? "" : `；摘要 r${summaryRevision}`, v3: requestRevisionAligned === false ? "。runtime 在两次读取间已更新，明细与摘要需分别解读。" : "", v4: summaryRevision === null ? "" : `; summary r${summaryRevision}`, v5: requestRevisionAligned === false ? ". The runtime changed between reads; interpret detail and summary as separate snapshots." : "" })}</div>}
            {nativeInitialLoading ? <div role="status" className="rounded-lg border border-dashed border-border/60 bg-muted/15 px-3 py-3 text-xs text-muted-foreground">{translateCopy("an.loadingBoundedRequestDetail", lang)}</div> : visibleLogRows.length > 0 ? <>
              <div data-testid="analytics-log-grid" className="grid min-w-0 gap-2">
                {visibleLogRows.map((row, index) => {
                  const rowKey = `${row.request_id ?? "row"}-${row.timestamp_ms ?? index}`;
                  const expanded = expandedRequestId === rowKey;
                  return <RequestLogRowView key={rowKey} row={row} rowKey={rowKey} index={index} lang={lang} expanded={expanded} priceCache={priceCache} onToggle={() => setExpandedRequestId(expanded ? null : rowKey)} />;
                })}
              </div>
              {nativeRequestPage?.coverage?.complete === false && <div className="mt-2 text-xs text-muted-foreground">{translateCopy("an.detailCoverageIsLimitedToTheLatestRuntimeRecords", lang, { v1: nf(nativeRequestPage.coverage.loaded_limit ?? 10_000) })}</div>}
            </> : requestRows.length > 0 ? <NoData title={translateCopy("an.noMatchingRequestLogs", lang)} detail={translateCopy("an.filtersApplyOnlyToTheCurrentBoundedPageContinuePagingToInspectOtherRuntimeRecords", lang)} /> : <NoData title={translateCopy("an.noRequestLogs", lang)} detail={translateCopy("an.thisListShowsOnlyTheBoundedRuntimeSnapshotNotFullHistory", lang)} />}
            {(requestTotal > REQUEST_PAGE_SIZE || (nativePagination && (requestHasMore || currentRequestPage > 0))) && <RequestPaginationBar lang={lang} currentRequestPage={currentLogPage} requestPageCount={logPageCount} requestHasMore={logHasMore} nativePageStatus={nativePageStatus} onPrevious={() => nativePagination ? setNativePendingPageIndex(Math.max(0, currentLogPage - 1)) : setRequestPage(Math.max(0, currentLogPage - 1))} onNext={() => { if (nativePagination) { const next = nativeRequestPage.next_cursor; if (!next) return; setNativeCursors((cursors) => [...cursors.slice(0, nativePageIndex + 1), next]); setNativePendingPageIndex(nativePageIndex + 1); } else { setRequestPage(Math.min(logPageCount - 1, currentLogPage + 1)); } }} />}
          </Surface>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function Metric({
  index,
  icon,
  label,
  value,
  detail,
  color,
}: {
  index: number;
  icon: React.ReactNode;
  label: string;
  value: string;
  detail: string;
  color: string;
}) {
  return (
    <motion.div data-analytics-metric initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: index * 0.035 }}>
      <div className="flex items-center gap-1.5 text-xs text-muted-foreground">{icon}{label}</div>
      <div className="mt-1.5 text-xl font-semibold tracking-tight tabular-nums" style={{ color }}>{value}</div>
      <div className="mt-0.5 truncate text-xs text-muted-foreground" title={detail}>{detail}</div>
    </motion.div>
  );
}

function AnalyticsBreakdownCard({ icon, title, rows, lang }: { icon: React.ReactNode; title: string; rows: BreakdownItem[]; lang: "zh" | "en" }) {
  return (
    <Surface className="p-4" hover={false}>
      <h3 className="mb-3 flex items-center gap-2 text-sm">{icon}{title}</h3>
      <BreakdownList rows={rows} lang={lang} />
    </Surface>
  );
}

function timingValue(value: number | null) {
  return value === null ? unavailableValue : `${value.toFixed(1)}ms`;
}

function formatTimestamp(value: unknown) {
  if (typeof value !== "number" || !Number.isFinite(value)) return unavailableValue;
  const date = new Date(value);
  return Number.isFinite(date.getTime()) ? date.toLocaleString() : unavailableValue;
}

function safeCell(value: unknown) {
  return typeof value === "string" && value.trim() ? redactString(value.trim()).slice(0, 96) : unavailableValue;
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="flat-inset p-2.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 text-base font-semibold">{value}</div>
    </div>
  );
}

function SummaryCard({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 text-lg font-semibold">{value}</div>
    </div>
  );
}

function BreakdownList({
  rows,
  lang,
  showSaved = true,
}: {
  rows: BreakdownItem[];
  lang: "zh" | "en";
  showSaved?: boolean;
}) {
  if (rows.length === 0) {
    return (
      <NoData
        title={translateCopy("cap.noRealRequestRecords", lang)}
        detail={translateCopy("an.connectTheLocalServiceAndSendARequest", lang)}
      />
    );
  }
  const maxCount = Math.max(...rows.map((row) => row.count), 1);
  return (
    <div className="space-y-2.5">
      {rows.map((row) => (
        <div key={row.label} className="flat-inset p-2.5">
          <div className="flex items-center justify-between gap-3 text-sm">
            <span className="min-w-0 truncate">{row.label}</span>
            <span className="shrink-0 text-xs text-muted-foreground">
              {row.count} {translateCopy("an.requests2", lang)}
            </span>
          </div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted">
            <div
              className="h-full rounded-full bg-[var(--chart-accent)]"
              style={{ width: `${(row.count / maxCount) * 100}%` }}
            />
          </div>
          {showSaved && (
            <div className="mt-1.5 text-xs text-muted-foreground">
              {translateCopy("an.estimatedSavedTokens2", lang)} · {optionalNumber(row.tokensSaved)}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function NoData({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex min-h-28 flex-col items-center justify-center rounded-xl border border-dashed border-border/60 bg-muted/15 px-4 text-center">
      <div className="text-sm font-medium">{title}</div>
      <div className="mt-1 text-xs text-muted-foreground">{detail}</div>
    </div>
  );
}

function optionalNumber(value: number | null) {
  return value === null ? unavailableValue : nf(value);
}

function metricDetail(value: number | null, fallback: string, lang: "zh" | "en") {
  return value === null
    ? fallback
    : translateCopy("an.estimatedFromTokenFieldsInLocalRequestLogs", lang);
}

export function applyRuntimeAggregate(
  snapshot: AnalyticsSnapshot,
  statsValue: unknown,
  lang: "zh" | "en",
  metricsValue?: unknown,
): AnalyticsSnapshot {
  const stats = recordValue(statsValue);
  const requests = recordValue(stats.requests);
  const tokens = recordValue(stats.tokens);
  const analytics = recordValue(stats.analytics);
  const metrics = recordValue(metricsValue);
  const routePerformance = recordValue(metrics.route_performance);
  const requestCount = nonNegativeMetric(requests.total) ?? snapshot.requestCount;
  const errorCount = nonNegativeMetric(requests.failed) ?? snapshot.errorCount;
  const inputBefore = nonNegativeMetric(tokens.input);
  const inputAfter = nonNegativeMetric(tokens.input_after);
  const output = nonNegativeMetric(tokens.output);
  const physicalSaved = inputBefore !== null && inputAfter !== null
    ? Math.max(0, inputBefore - inputAfter)
    : snapshot.estimatedSavedTokens;
  const recentWindowTruncated = requestCount > snapshot.requestCount;
  const scopeWarning = translateCopy("an.totalsComeFromDurableAggregatesBreakdownsLatencyAndTrendsCoverTheReturnedRecentRequestWindow", lang);
  const runtimeDaily = runtimeTrend(analytics.daily);
  const coverage = recordValue(analytics.coverage);
  const coverageWarning = coverage.complete === false
    ? (translateCopy("an.analyticsAggregatesCoverAtMostTheLatestRuntimeRecords", lang, { v1: nf(nonNegativeMetric(coverage.loaded_limit) ?? 10_000) }))
    : null;

  const warnings = recentWindowTruncated && !snapshot.warnings.includes(scopeWarning)
    ? [scopeWarning, ...snapshot.warnings]
    : [...snapshot.warnings];
  if (coverageWarning && !warnings.includes(coverageWarning)) warnings.unshift(coverageWarning);

  return {
    ...snapshot,
    hasRequests: requestCount > 0,
    requestCount,
    errorCount,
    estimatedInputTokens: inputBefore ?? snapshot.estimatedInputTokens,
    estimatedOutputTokens: output ?? snapshot.estimatedOutputTokens,
    estimatedSavedTokens: physicalSaved,
    estimatedSavingsRate: physicalSaved !== null && inputBefore !== null && inputBefore > 0
      ? physicalSaved / inputBefore
      : snapshot.estimatedSavingsRate,
    agentBreakdown: runtimeBreakdown(analytics.agents) ?? snapshot.agentBreakdown,
    modelBreakdown: runtimeBreakdown(analytics.models) ?? snapshot.modelBreakdown,
    protocolBreakdown: runtimeBreakdown(analytics.protocols, (name) => protocolLabel(name, lang)) ?? snapshot.protocolBreakdown,
    chainBreakdown: runtimeBreakdown(analytics.chains) ?? snapshot.chainBreakdown,
    errorBreakdown: runtimeBreakdown(analytics.errors) ?? snapshot.errorBreakdown,
    routeBreakdown: runtimeBreakdown(routePerformance.routes) ?? snapshot.routeBreakdown,
    upstreamBreakdown: runtimeBreakdown(routePerformance.providers) ?? snapshot.upstreamBreakdown,
    trend7d: runtimeDaily.length > 0 ? runtimeDaily.slice(-7) : snapshot.trend7d,
    trend30d: runtimeDaily.length > 0 ? runtimeDaily.slice(-30) : snapshot.trend30d,
    warnings,
  };
}

function runtimeBreakdown(value: unknown, normalizeLabel?: (label: string) => string | null): BreakdownItem[] | null {
  if (!Array.isArray(value)) return null;
  return value.flatMap((item) => {
    const row = recordValue(item);
    const raw = typeof row.name === "string" && row.name.trim() ? row.name.trim() : null;
    const label = raw === null
      ? null
      : (normalizeLabel ? normalizeLabel(raw) : redactString(raw).slice(0, 96));
    const count = nonNegativeMetric(row.requests);
    if (!label || count === null) return [];
    return [{ label, count, tokensSaved: nonNegativeMetric(row.tokens_saved) }];
  });
}

function runtimeTrend(value: unknown): Array<{ day: string; requests: number; saved: number | null }> {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    const row = recordValue(item);
    const bucket = nonNegativeMetric(row.bucket_ms);
    const requests = nonNegativeMetric(row.requests);
    if (bucket === null || requests === null) return [];
    const date = new Date(bucket);
    if (!Number.isFinite(date.getTime())) return [];
    return [{ day: date.toISOString().slice(0, 10), requests, saved: nonNegativeMetric(row.tokens_saved) }];
  });
}

function recordValue(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

function nonNegativeMetric(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

function analyticsSourceLabel(state: string, hasRequests: boolean, lang: "zh" | "en") {
  if (state === "verified") {
    return hasRequests
      ? translateCopy("an.localDTokenRequestData", lang)
      : translateCopy("an.localServiceOnlineNoRequestRecords", lang);
  }
  if (state === "preview") return translateCopy("an.browserPreviewNoLiveData", lang);
  if (state === "loading") return translateCopy("an.connectingToTheLocalDTokenService", lang);
  if (state === "stale") return translateCopy("an.localDataIsStale", lang);
  if (state === "partial") return translateCopy("an.localDataIsIncomplete", lang);
  return translateCopy("an.localDTokenServiceUnavailable", lang);
}

function analyticsStatusDetail(state: string, hasRequests: boolean, lang: "zh" | "en") {
  if (state === "verified" && hasRequests) {
    return translateCopy("an.everyMetricIsGeneratedFromOneLocalDTokenRequestSnapshot", lang);
  }
  if (state === "verified") {
    return translateCopy("an.theLocalServiceIsConnectedMetricsAppearAfterRealRequestsArrive", lang);
  }
  if (state === "stale") {
    return translateCopy("an.theLastVerifiedMetricsAreRetainedForTraceabilityTheyDoNotRepresentTheCurrentServiceState", lang);
  }
  return translateCopy("an.noVerifiableLiveRequestDataIsAvailableMissingMetricsRemainEmpty", lang);
}

function coverageRows(snapshot: AnalyticsSnapshot, lang: "zh" | "en", priceCacheStatus: "loading" | "ready" | "unavailable") {
  return [
    { label: translateCopy("an.requestRecords", lang), available: snapshot.hasRequests },
    { label: translateCopy("an.estimatedInputTokens2", lang), available: snapshot.estimatedInputTokens !== null },
    { label: translateCopy("an.estimatedOutputTokens2", lang), available: snapshot.estimatedOutputTokens !== null },
    { label: translateCopy("an.estimatedSavedTokens3", lang), available: snapshot.estimatedSavedTokens !== null },
    { label: translateCopy("an.compressionLatency", lang), available: snapshot.latency !== null },
    { label: translateCopy("an.agentIdentity", lang), available: snapshot.agentBreakdown.length > 0 },
    { label: translateCopy("an.routeIdentity", lang), available: snapshot.routeBreakdown.length > 0 },
    { label: translateCopy("an.providerIdentity", lang), available: snapshot.upstreamBreakdown.length > 0 },
    { label: translateCopy("an.timeTrend", lang), available: snapshot.trend30d.length > 0 },
    { label: translateCopy("hm.processingSteps", lang), available: snapshot.strategyBreakdown.length > 0 },
    { label: translateCopy("an.modelIdentity", lang), available: snapshot.modelBreakdown.length > 0 },
    { label: translateCopy("an.inputProtocol", lang), available: snapshot.protocolBreakdown.length > 0 },
    { label: translateCopy("an.projectIdentity", lang), available: snapshot.projectBreakdown.length > 0 },
    { label: translateCopy("an.relayChainMode", lang), available: snapshot.chainBreakdown.length > 0 },
    { label: translateCopy("an.failureReasons", lang), available: snapshot.errorBreakdown.length > 0 },
    { label: translateCopy("an.cacheAlignment2", lang), available: snapshot.cache !== null },
    { label: translateCopy("an.pipelineStageTiming", lang), available: snapshot.pipeline !== null },
    { label: translateCopy("an.ttfbAndThroughput", lang), available: snapshot.pipeline?.upstreamTtfbMs !== null && snapshot.pipeline?.upstreamTtfbMs !== undefined },
    { label: translateCopy("an.cacheExpiryAndMissAttribution", lang), available: snapshot.cache !== null && (snapshot.cache.busts > 0 || snapshot.cache.misses > 0) },
    { label: translateCopy("an.costAndPricingSource", lang), available: priceCacheStatus === "ready" },
    { label: translateCopy("an.outputShapingCounterfactual", lang), available: false },
    { label: translateCopy("an.toolDefinitionSavings", lang), available: false },
    { label: translateCopy("an.checkpointWeeklyMonthlyHistory", lang), available: false },
  ];
}

// ---------------------------------------------------------------------------
// Analytics → 日志 子页：逐条展示 runtime 真实请求记录
// ---------------------------------------------------------------------------

function tokenEvidenceCell(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? nf(value) : unavailableValue;
}

function durationCell(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? `${value}ms` : unavailableValue;
}

function beforeAfterCell(row: RequestLogRow) {
  const before = row.input_tokens_before;
  const after = row.input_tokens_after;
  if (typeof before === "number" && Number.isFinite(before) && typeof after === "number" && Number.isFinite(after)) {
    return `${nf(before)} → ${nf(after)}`;
  }
  return unavailableValue;
}

function RequestPaginationBar({
  lang,
  currentRequestPage,
  requestPageCount,
  requestHasMore,
  nativePageStatus,
  onPrevious,
  onNext,
}: {
  lang: "zh" | "en";
  currentRequestPage: number;
  requestPageCount: number;
  requestHasMore: boolean;
  nativePageStatus: "idle" | "loading" | "ready" | "error";
  onPrevious: () => void;
  onNext: () => void;
}) {
  return (
    <div className="mt-3 flex items-center justify-between">
      <span className="text-xs text-muted-foreground">{translateCopy("an.page", lang, { v1: currentRequestPage + 1, v2: requestPageCount })}</span>
      <div className="flex gap-2">
        <Button size="sm" variant="outline" disabled={currentRequestPage === 0 || nativePageStatus === "loading"} onClick={onPrevious}>{translateCopy("an.previous", lang)}</Button>
        <Button size="sm" variant="outline" disabled={!requestHasMore || nativePageStatus === "loading"} onClick={onNext}>{nativePageStatus === "loading" ? (translateCopy("an.loading", lang)) : (translateCopy("an.next", lang))}</Button>
      </div>
    </div>
  );
}

type LogEvidenceRow = { label: string; value: string };

function requestLogStatus(row: RequestLogRow): "ok" | "failed" | "unknown" {
  const status = typeof row.status === "string" ? row.status.trim().toLocaleLowerCase() : "";
  if (status === "failed" || status === "error" || row.error || row.error_type) return "failed";
  if (status === "ok" || status === "completed" || status === "success") return "ok";
  return "unknown";
}

function RequestLogRowView({
  row,
  rowKey,
  lang,
  expanded,
  priceCache,
  onToggle,
}: {
  row: RequestLogRow;
  rowKey: string;
  index: number;
  lang: "zh" | "en";
  expanded: boolean;
  priceCache: ModelPriceCache | null;
  onToggle: () => void;
}) {
  const status = requestLogStatus(row);
  const detailId = `log-detail-${rowKey}`;
  const receipt = receiptRows(row.context_accounting, lang);
  const protocol = displayProtocol(row.input_protocol ?? row.protocol);
  return (
    <article className="min-w-0 rounded-lg border border-border/60 bg-background/70 p-3 text-xs">
      <div className="flex min-w-0 flex-wrap items-start justify-between gap-2">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
          <code className="min-w-0 max-w-full truncate font-mono text-[11px]" title={typeof row.request_id === "string" ? row.request_id : undefined}>{typeof row.request_id === "string" ? redactString(row.request_id) : unavailableValue}</code>
          {status === "failed" ? <Badge variant="destructive">{translateCopy("an.error", lang)}</Badge> : status === "ok" ? <Badge variant="secondary">{translateCopy("an.ok", lang)}</Badge> : <Badge variant="outline">{translateCopy("cbm.state.unknown", lang)}</Badge>}
          <span className="whitespace-nowrap font-mono">{formatTimestamp(row.timestamp_ms)}</span>
          <span className="min-w-0 truncate">{safeCell(row.agent)}</span>
          <span className="min-w-0 truncate">{safeCell(row.model_id ?? row.model)}</span>
          <span className="min-w-0 truncate">{safeCell(row.provider_id ?? row.provider)}</span>
          <span className="whitespace-nowrap">{protocol}</span>
        </div>
        <Button size="sm" variant="ghost" aria-expanded={expanded} aria-controls={detailId} onClick={onToggle} className="shrink-0 gap-1 px-2 text-xs">
          {expanded ? <ChevronUp className="size-3.5" /> : <ChevronDown className="size-3.5" />}
          {expanded ? (translateCopy("an.collapse", lang)) : (translateCopy("an.details", lang))}
        </Button>
      </div>

      <div className="mt-3 grid min-w-0 grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6">
        <LogValue label="Local Token" value={tokenEvidenceCell(row.input_tokens_before)} />
        <LogValue label="Provider Token" value={tokenEvidenceCell(row.provider_input_tokens)} />
        <LogValue label={translateCopy("an.beforeAfter", lang)} value={beforeAfterCell(row)} />
        <LogValue label={translateCopy("an.savedTokens", lang)} value={tokenEvidenceCell(row.tokens_saved)} />
        <LogValue label={translateCopy("an.latency", lang)} value={durationCell(row.latency_ms ?? row.end_to_end_ms ?? row.compression_ms)} />
        <LogValue label={translateCopy("dr.layer.route", lang)} value={safeCell(row.route_id ?? row.route)} />
      </div>

      {expanded && (
        <div id={detailId} className="mt-3 grid min-w-0 gap-3 border-t border-border/50 pt-3 sm:grid-cols-2">
          <LogDetailSection title={translateCopy("an.routeIdentity2", lang)} rows={routeIdentityRows(row, lang)} />
          <LogDetailSection title={translateCopy("an.tokenEvidence", lang)} rows={tokenDetailRows(row, lang)} />
          <LogDetailSection title={translateCopy("an.performanceAndCost", lang)} rows={performanceCostRows(row, lang, priceCache)} />
          <section className="min-w-0 rounded-md bg-muted/20 p-3">
            <h4 className="text-[11px] font-semibold">{translateCopy("an.evidenceReceipt", lang)}</h4>
            {row.transforms_applied && row.transforms_applied.length > 0 && (
              <div className="mt-2">
                <div className="mb-1.5 text-[11px] text-muted-foreground">{translateCopy("an.transformEvidence", lang)}</div>
                <div className="flex flex-wrap gap-1.5">
                  {row.transforms_applied.map((transform) => <Badge key={transform} variant="secondary" className="text-[11px]">{safeCell(transform)}</Badge>)}
                </div>
              </div>
            )}
            <div className="mt-2 text-[11px] text-muted-foreground">{receipt.length > 0 ? (translateCopy("an.optimizationReceiptRedactedWhitelist", lang)) : (translateCopy("an.optimizationReceiptRuntimeUnavailable", lang))}</div>
            {receipt.length > 0 ? <div className="mt-1.5 grid min-w-0 gap-2 sm:grid-cols-2">{receipt.map((item) => <LogValue key={item.label} label={item.label} value={item.value} mono />)}</div> : <div className="mt-1 text-xs text-muted-foreground">—</div>}
          </section>
        </div>
      )}
    </article>
  );
}

function LogValue({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div className="min-w-0"><div className="text-[11px] text-muted-foreground">{label}</div><div className={`mt-0.5 min-w-0 break-all text-xs tabular-nums${mono ? " font-mono" : ""}`}>{value}</div></div>;
}

function LogDetailSection({ title, rows }: { title: string; rows: LogEvidenceRow[] }) {
  return <section className="min-w-0 rounded-md bg-muted/20 p-3"><h4 className="text-[11px] font-semibold">{title}</h4><div className="mt-2 grid min-w-0 gap-2 sm:grid-cols-2">{rows.map((item) => <LogValue key={item.label} label={item.label} value={item.value} />)}</div></section>;
}

function evidenceValue(value: unknown, formatter?: (value: number) => string): string {
  if (typeof value === "number" && Number.isFinite(value)) return formatter ? formatter(value) : nf(value);
  if (typeof value === "string" && value.trim()) return redactString(value.trim()).slice(0, 200);
  return unavailableValue;
}

function routeIdentityRows(row: RequestLogRow, lang: "zh" | "en"): LogEvidenceRow[] {
  return [
    { label: translateCopy("an.requestId", lang), value: evidenceValue(row.request_id) },
    { label: translateCopy("visual.col.time", lang), value: typeof row.timestamp_ms === "number" ? formatTimestamp(row.timestamp_ms) : unavailableValue },
    { label: "Agent", value: evidenceValue(row.agent) },
    { label: translateCopy("an.interfaceInstance", lang), value: evidenceValue(row.instance_id) },
    { label: translateCopy("an.model", lang), value: evidenceValue(row.model_id ?? row.model) },
    { label: "Provider", value: evidenceValue(row.provider_id ?? row.provider) },
    { label: translateCopy("dr.layer.route", lang), value: evidenceValue(row.route_id ?? row.route) },
    { label: translateCopy("an.inputProtocol", lang), value: displayProtocol(row.input_protocol ?? row.protocol) },
    { label: translateCopy("an.outputProtocol", lang), value: displayProtocol(row.output_protocol) },
    { label: translateCopy("an.relayChainMode", lang), value: evidenceValue(row.chain_mode) },
  ];
}

function tokenDetailRows(row: RequestLogRow, lang: "zh" | "en"): LogEvidenceRow[] {
  return [
    { label: "Local Token", value: tokenEvidenceCell(row.input_tokens_before) },
    { label: translateCopy("an.localCompressedTokens", lang), value: tokenEvidenceCell(row.input_tokens_after) },
    { label: "Provider Token", value: tokenEvidenceCell(row.provider_input_tokens) },
    { label: translateCopy("an.outputTokens", lang), value: tokenEvidenceCell(row.output_tokens) },
    { label: translateCopy("an.savedTokens", lang), value: tokenEvidenceCell(row.tokens_saved) },
  ];
}

function performanceCostRows(row: RequestLogRow, lang: "zh" | "en", priceCache: ModelPriceCache | null): LogEvidenceRow[] {
  const source = row as RequestLogRow & { cost_usd?: unknown; provider_cost_usd?: unknown };
  const estimate = estimateRequestInputCost(priceCache, row.model_id ?? row.model, row.input_tokens_before, row.input_tokens_after);
  const rows: LogEvidenceRow[] = [
    { label: translateCopy("an.compressionLatency", lang), value: durationCell(row.compression_ms) },
    { label: translateCopy("an.routeLatency", lang), value: durationCell(row.route_ms) },
    { label: translateCopy("an.upstreamTtfb", lang), value: durationCell(row.upstream_ttfb_ms) },
    { label: translateCopy("an.endToEndLatency", lang), value: durationCell(row.end_to_end_ms ?? row.latency_ms) },
    { label: translateCopy("an.upstreamBilledCost", lang), value: evidenceValue(source.provider_cost_usd ?? source.cost_usd, (value) => `$${value.toFixed(6)}`) },
  ];
  if (estimate.available) {
    const symbol = estimate.currency === "CNY" ? "¥" : "$";
    const money = (value: number) => `${symbol}${value.toFixed(6)}`;
    rows.push(
      { label: translateCopy("an.estPreCompressionInputCost", lang), value: money(estimate.preCost) },
      { label: translateCopy("an.estPostCompressionInputCost", lang), value: money(estimate.postCost) },
      { label: translateCopy("an.estCostDifference", lang), value: money(estimate.savedCost) },
      { label: translateCopy("an.pricingBasis", lang), value: `${estimate.currency} ${estimate.source} ${estimate.freshness}` },
    );
  } else {
    rows.push(
      { label: translateCopy("an.estPreCompressionInputCost", lang), value: unavailableValue },
      { label: translateCopy("an.estPostCompressionInputCost", lang), value: unavailableValue },
      { label: translateCopy("an.estCostDifference", lang), value: unavailableValue },
      { label: translateCopy("an.pricingBasis", lang), value: translateCopy("an.priceUnknown", lang) },
    );
  }
  rows.push({ label: translateCopy("an.failureType", lang), value: evidenceValue(row.error_type ?? row.error) });
  return rows;
}

function displayProtocol(value: unknown): string {
  return protocolLabel(value) ?? unavailableValue;
}

/** context_accounting 白名单：只取标量字段，不转储嵌套对象或原始载荷。 */
function receiptRows(value: unknown, lang: "zh" | "en"): Array<{ label: string; value: string }> {
  if (!value || typeof value !== "object") return [];
  const source = value as Record<string, unknown>;
  const rows: Array<{ label: string; value: string }> = [];
  const flat: Array<[string, string]> = [
    ["disposition", translateCopy("an.disposition", lang)],
    ["sent_payload", translateCopy("an.sentPayload", lang)],
    ["upstream_state", translateCopy("an.upstreamState", lang)],
    ["attribution_basis", translateCopy("an.attributionBasis", lang)],
    ["provider_reconciled_input_savings", translateCopy("an.providerReconciledSavings", lang)],
    ["fallback_reason_code", translateCopy("an.fallbackReason", lang)],
  ];
  for (const [key, label] of flat) {
    const raw = source[key];
    if (typeof raw === "string" && raw.trim()) rows.push({ label, value: raw });
    else if (typeof raw === "number" && Number.isFinite(raw)) rows.push({ label, value: String(raw) });
  }
  const estimated = source["estimated"];
  if (estimated && typeof estimated === "object") {
    const savings = (estimated as Record<string, unknown>)["signed_savings"];
    if (savings && typeof savings === "object") {
      const signed = savings as Record<string, unknown>;
      for (const [key, label] of [["input_tokens", translateCopy("an.estimatedSavedInputTokens", lang)], ["total_tokens", translateCopy("an.estimatedSavedTotalTokens", lang)]] as Array<[string, string]>) {
        const raw = signed[key];
        if (typeof raw === "number" && Number.isFinite(raw)) rows.push({ label, value: String(raw) });
      }
    }
  }
  return rows;
}
