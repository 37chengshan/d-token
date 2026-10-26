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
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { useI18n, useT } from "../../lib/i18n";
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
  normalizeModelId,
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

type ModelUsageRow = {
  label: string;
  count: number;
  tokensSaved: number | null;
  cost: string;
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
  const nativePageErrorMessage = nativePageError === "invalid_cursor" ? (lang === "zh" ? "记录已更新，已返回第一页。" : "Records changed. Showing the first page.") : nativePageError === "invalid_evidence" ? (lang === "zh" ? "新读取的记录未通过检查，暂时显示上次记录。" : "The new records could not be verified. Showing the previous records.") : (lang === "zh" ? "暂时无法加载请求记录，请检查本地服务后重试。" : "Request records could not be loaded. Check the local service and retry.");
  const requestEvidence = trafficEvidenceAvailable
    ? evidenceState === "stale"
      ? lang === "zh" ? "来自上次验证的 d-token 本地请求记录" : "From the last verified local d-token request records"
      : lang === "zh" ? "来自 d-token 本地请求记录" : "From local d-token request records"
    : sourceLabel;
  const modelUsageRows = useMemo(
    () => buildModelUsageRows(snapshot.modelBreakdown, requestRows, priceCache, snapshot.requestCount),
    [priceCache, requestRows, snapshot.modelBreakdown, snapshot.requestCount],
  );

  return (
    <div className="space-y-4">
      <h1 className="sr-only">{t("an.title")}</h1>
      {evidenceState === "stale" && <p role="status" className="text-xs text-muted-foreground">{lang === "zh" ? "当前显示上次记录，连接恢复后会自动更新。" : "Showing previous records; they update when the connection returns."}</p>}

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="live">{lang === "zh" ? "价值概览" : "Value overview"}</TabsTrigger>
          <TabsTrigger value="history">{lang === "zh" ? "历史趋势" : "Trend history"}</TabsTrigger>
          <TabsTrigger value="parity">{lang === "zh" ? "数据范围" : "Data scope"}</TabsTrigger>
          <TabsTrigger value="logs">{lang === "zh" ? "日志" : "Logs"}</TabsTrigger>
        </TabsList>

        <TabsContent value="live" className="mt-3 space-y-4">
          <MetricStrip
            data-testid="analytics-kpis"
            className="metric-strip-6"
          >
            <Metric
              index={0}
              icon={<ArrowDownWideNarrow className="size-3.5" />}
              label={lang === "zh" ? "已处理请求" : "Routed requests"}
              value={trafficEvidenceAvailable ? nf(snapshot.requestCount) : unavailableValue}
              detail={requestEvidence}
              color="var(--chart-2)"
            />
            <Metric
              index={1}
              icon={<Gauge className="size-3.5" />}
              label={lang === "zh" ? "估算输入 Token" : "Estimated input tokens"}
              value={trafficEvidenceAvailable ? optionalNumber(snapshot.estimatedInputTokens) : unavailableValue}
              detail={metricDetail(snapshot.estimatedInputTokens, sourceLabel, lang)}
              color="var(--chart-3)"
            />
            <Metric
              index={2}
              icon={<Coins className="size-3.5" />}
              label={lang === "zh" ? "物理减少 Token" : "Physical token reduction"}
              value={trafficEvidenceAvailable ? optionalNumber(snapshot.estimatedSavedTokens) : unavailableValue}
              detail={metricDetail(snapshot.estimatedSavedTokens, sourceLabel, lang)}
              color="var(--chart-accent)"
            />
            <Metric
              index={3}
              icon={<GitBranch className="size-3.5" />}
              label={lang === "zh" ? "物理减少比例" : "Physical reduction rate"}
              value={trafficEvidenceAvailable && snapshot.estimatedSavingsRate !== null
                ? `${(snapshot.estimatedSavingsRate * 100).toFixed(1)}%`
                : unavailableValue}
              detail={metricDetail(snapshot.estimatedSavingsRate, sourceLabel, lang)}
              color="var(--chart-4)"
            />
            <Metric
              index={4}
              icon={<Bot className="size-3.5" />}
              label={lang === "zh" ? "估算输出 Token" : "Estimated output tokens"}
              value={trafficEvidenceAvailable ? optionalNumber(snapshot.estimatedOutputTokens) : unavailableValue}
              detail={metricDetail(snapshot.estimatedOutputTokens, sourceLabel, lang)}
              color="var(--chart-5)"
            />
            <Metric
              index={5}
              icon={<AlertTriangle className="size-3.5" />}
              label={lang === "zh" ? "请求错误" : "Request errors"}
              value={trafficEvidenceAvailable ? nf(snapshot.errorCount) : unavailableValue}
              detail={requestEvidence}
              color={snapshot.errorCount > 0 ? "var(--destructive)" : "var(--muted-foreground)"}
            />
          </MetricStrip>

          {!snapshot.hasRequests && requestRows.length === 0 ? (
            <div data-testid="analytics-empty-state" className="px-4 py-10">
              <p role={nativePageError ? "alert" : "status"} className="text-sm font-medium">
                {nativeInitialLoading
                  ? (lang === "zh" ? "正在读取请求记录…" : "Loading request records…")
                  : nativePageError
                    ? nativePageErrorMessage
                    : (lang === "zh" ? "暂无请求记录" : "No request records yet")}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">
                {lang === "zh" ? "连接编码工具并发送请求后，这里会显示用量、压缩效果和详细记录。" : "Connect a coding tool and send a request to see usage, compression results, and detailed records."}
              </p>
            </div>
          ) : <>
          <div className="grid gap-3 lg:grid-cols-[minmax(0,1.4fr)_minmax(240px,0.8fr)]">
            <Surface className="overflow-hidden p-0" hover={false}>
              <div className="flex items-center justify-between gap-3 border-b border-border/60 px-4 py-3">
                <h3 className="flex items-center gap-2 text-sm">
                  <Layers3 className="size-4" />
                  {lang === "zh" ? "模型使用" : "Model usage"}
                </h3>
              </div>
              <ModelUsageTable rows={modelUsageRows} lang={lang} />
            </Surface>

            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Bot className="size-4" />
                {lang === "zh" ? "工具流量" : "Tool traffic"}
              </h3>
              <ToolTrafficBars rows={snapshot.agentBreakdown} lang={lang} />
            </Surface>
          </div>

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
                {lang === "zh" ? "压缩耗时" : "Compression latency"}
              </h3>
              {snapshot.latency ? (
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                  <Summary label={lang === "zh" ? "有耗时记录" : "Records with timing"} value={nf(snapshot.latency.sampleCount)} />
                  <Summary label={lang === "zh" ? "平均" : "Average"} value={`${snapshot.latency.averageMs.toFixed(1)}ms`} />
                  <Summary label="p50" value={`${snapshot.latency.p50Ms.toFixed(1)}ms`} />
                  <Summary label="p95" value={`${snapshot.latency.p95Ms.toFixed(1)}ms`} />
                </div>
              ) : (
                <NoData
                  title={lang === "zh" ? "暂无压缩耗时记录" : "No compression latency records"}
                  detail={lang === "zh" ? "记录的耗时只覆盖压缩阶段，不用于推算上游或端到端延迟" : "Recorded timing covers the compression stage only, and is not used to infer upstream or end-to-end latency"}
                />
              )}
            </Surface>

            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Bot className="size-4" />
                {lang === "zh" ? "Agent 分布" : "Agent distribution"}
              </h3>
              <BreakdownList rows={snapshot.agentBreakdown} lang={lang} />
            </Surface>
          </div>

          <div className="grid gap-3 lg:grid-cols-2">
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Route className="size-4" />
                {lang === "zh" ? "路由分布" : "Route distribution"}
              </h3>
              <BreakdownList rows={snapshot.routeBreakdown} lang={lang} />
            </Surface>
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <GitBranch className="size-4" />
                {lang === "zh" ? "上游分布" : "Provider distribution"}
              </h3>
              <BreakdownList rows={snapshot.upstreamBreakdown} lang={lang} />
            </Surface>
          </div>

          <Surface className="p-4" hover={false}>
            <div className="mb-3 flex items-center justify-between gap-2">
              <h3 className="flex items-center gap-2 text-sm">
                <Boxes className="size-4" />
                {lang === "zh" ? "实际处理步骤" : "Recorded processing steps"}
              </h3>
              <Badge variant="secondary">
                {snapshot.compressionCount === null
                  ? unavailableValue
                  : `${nf(snapshot.compressionCount)} ${lang === "zh" ? "次有节省记录" : "records with savings"}`}
              </Badge>
            </div>
            <BreakdownList rows={snapshot.strategyBreakdown} lang={lang} showSaved={false} />
          </Surface>

          <div className="grid gap-3 lg:grid-cols-2">
            <AnalyticsBreakdownCard icon={<Layers3 className="size-4" />} title={lang === "zh" ? "模型明细" : "Model detail"} rows={snapshot.modelBreakdown} lang={lang} />
            <AnalyticsBreakdownCard icon={<ListFilter className="size-4" />} title={lang === "zh" ? "输入协议" : "Input protocol"} rows={snapshot.protocolBreakdown} lang={lang} />
            <AnalyticsBreakdownCard icon={<ListFilter className="size-4" />} title={lang === "zh" ? "项目" : "Project"} rows={snapshot.projectBreakdown} lang={lang} />
            <AnalyticsBreakdownCard icon={<Route className="size-4" />} title={lang === "zh" ? "中转链模式" : "Relay chain mode"} rows={snapshot.chainBreakdown} lang={lang} />
          </div>

          <div className="grid gap-3 lg:grid-cols-2">
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm"><Database className="size-4" />{lang === "zh" ? "缓存对齐与命中" : "Cache alignment"}</h3>
              {snapshot.cache ? (
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                  <Summary label={lang === "zh" ? "对齐" : "Aligned"} value={nf(snapshot.cache.aligned)} />
                  <Summary label={lang === "zh" ? "未对齐" : "Misaligned"} value={nf(snapshot.cache.misaligned)} />
                  <Summary label={lang === "zh" ? "读取" : "Reads"} value={nf(snapshot.cache.reads)} />
                  <Summary label={lang === "zh" ? "写入" : "Writes"} value={nf(snapshot.cache.writes)} />
                  <Summary label={lang === "zh" ? "未命中" : "Misses"} value={nf(snapshot.cache.misses)} />
                  <Summary label={lang === "zh" ? "Bust" : "Busts"} value={nf(snapshot.cache.busts)} />
                </div>
              ) : <NoData title={lang === "zh" ? "暂无缓存证据" : "No cache evidence"} detail={lang === "zh" ? "当前请求没有记录缓存读写或对齐信息" : "Requests contain no cache read/write or alignment evidence"} />}
            </Surface>
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm"><Timer className="size-4" />{lang === "zh" ? "流水线耗时" : "Pipeline timing"}</h3>
              {snapshot.pipeline ? (
                <div className="grid grid-cols-2 gap-2">
                  <Summary label={lang === "zh" ? "压缩" : "Compression"} value={timingValue(snapshot.pipeline.compressionMs)} />
                  <Summary label={lang === "zh" ? "路由" : "Route"} value={timingValue(snapshot.pipeline.routeMs)} />
                  <Summary label="TTFB" value={timingValue(snapshot.pipeline.upstreamTtfbMs)} />
                  <Summary label={lang === "zh" ? "端到端" : "End-to-end"} value={timingValue(snapshot.pipeline.endToEndMs)} />
                </div>
              ) : <NoData title={lang === "zh" ? "暂无分段耗时" : "No stage timing"} detail={lang === "zh" ? "当前仅有压缩耗时时不会推算端到端延迟" : "End-to-end latency is not inferred from compression-only timing"} />}
            </Surface>
          </div>

          <div className="grid gap-3 lg:grid-cols-2">
            <AnalyticsBreakdownCard icon={<AlertTriangle className="size-4" />} title={lang === "zh" ? "失败原因" : "Failure reasons"} rows={snapshot.errorBreakdown} lang={lang} />
            <Surface className="p-4" hover={false}>
              <h3 className="mb-3 text-sm">{lang === "zh" ? "浪费信号" : "Waste signals"}</h3>
              {snapshot.wasteSignals.length > 0 ? <div className="space-y-2">{snapshot.wasteSignals.map((signal) => <div key={signal.label} className="flat-inset flex items-center justify-between gap-3 p-2.5"><div><div className="text-sm">{signal.label}</div><div className="text-xs text-muted-foreground">{signal.detail}</div></div><Badge variant="secondary">{nf(signal.count)}</Badge></div>)}</div> : <NoData title={lang === "zh" ? "暂无浪费信号" : "No waste signals"} detail={lang === "zh" ? "需要请求级缓存或变换证据" : "Request-level cache or transform evidence is required"} />}
            </Surface>
          </div>

          <Surface className="p-4" hover={false}>
            <div className="mb-3 flex items-center justify-between gap-2"><h3 className="text-sm">{lang === "zh" ? "请求明细" : "Request detail"}</h3><span className="text-xs text-muted-foreground">{requestTotal > 0 ? `${requestTotal} ${lang === "zh" ? (nativePagination ? "条 runtime 记录" : "条当前快照") : (nativePagination ? "runtime records" : "in current snapshot")}` : sourceLabel}</span></div>
            {nativePageError && <div role="status" className="mb-2 text-xs text-amber-600 dark:text-amber-400">{nativePageErrorMessage}</div>}
            {nativeRequestPage && <div className="mb-2 text-xs text-muted-foreground">{lang === "zh" ? `明细源 d-token runtime · fresh · r${requestRevision}${summaryRevision === null ? "" : `；摘要 r${summaryRevision}`}${requestRevisionAligned === false ? "。runtime 在两次读取间已更新，明细与摘要需分别解读。" : ""}` : `Detail source d-token runtime · fresh · r${requestRevision}${summaryRevision === null ? "" : `; summary r${summaryRevision}`}${requestRevisionAligned === false ? ". The runtime changed between reads; interpret detail and summary as separate snapshots." : ""}`}</div>}
            {nativeInitialLoading ? <div role="status" className="rounded-lg border border-dashed border-border/60 bg-muted/15 px-3 py-3 text-xs text-muted-foreground">{lang === "zh" ? "正在读取请求记录…" : "Loading request records…"}</div> : visibleRequestRows.length > 0 ? <>
              <div className="overflow-x-auto">
                <table className="min-w-[720px] w-full text-sm">
                  <thead>
                    <tr className="border-b border-border/50 text-left text-xs text-muted-foreground">
                      {[lang === "zh" ? "时间" : "Time", "Agent", lang === "zh" ? "模型" : "Model", lang === "zh" ? "路由" : "Route", lang === "zh" ? "节省" : "Saved", lang === "zh" ? "状态" : "Status"].map((label) => <th key={label} className="whitespace-nowrap pb-2 font-normal">{label}</th>)}
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
                        <td className="whitespace-nowrap py-2">{row.error || row.error_type ? <Badge variant="destructive">{lang === "zh" ? "失败" : "Error"}</Badge> : <Badge variant="secondary">{lang === "zh" ? "完成" : "OK"}</Badge>}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {nativeRequestPage?.coverage?.complete === false && <div className="mt-2 text-xs text-muted-foreground">{lang === "zh" ? `明细最多覆盖最近 ${nf(nativeRequestPage.coverage.loaded_limit ?? 10_000)} 条 runtime 记录。` : `Detail coverage is limited to the latest ${nf(nativeRequestPage.coverage.loaded_limit ?? 10_000)} runtime records.`}</div>}
              {requestTotal > REQUEST_PAGE_SIZE && <RequestPaginationBar lang={lang} currentRequestPage={currentRequestPage} requestPageCount={requestPageCount} requestHasMore={requestHasMore} nativePageStatus={nativePageStatus} onPrevious={() => nativePagination ? setNativePendingPageIndex(Math.max(0, currentRequestPage - 1)) : setRequestPage(Math.max(0, currentRequestPage - 1))} onNext={() => { if (nativePagination) { const next = nativeRequestPage.next_cursor; if (!next) return; setNativeCursors((cursors) => [...cursors.slice(0, nativePageIndex + 1), next]); setNativePendingPageIndex(nativePageIndex + 1); } else { setRequestPage(Math.min(requestPageCount - 1, currentRequestPage + 1)); } }} />}
            </> : requestRows.length > 0 ? <div className="rounded-lg border border-dashed border-border/60 bg-muted/15 px-3 py-2 text-xs text-muted-foreground">{lang === "zh" ? "当前快照少于一页；请求维度已在上方分布中展示。" : "The current snapshot fits on one page; request dimensions are shown in the breakdowns above."}</div> : <NoData title={lang === "zh" ? "暂无请求明细" : "No request detail"} detail={lang === "zh" ? "此列表只展示 runtime 提供的有界快照，不代表全量历史" : "This table shows only the bounded runtime snapshot, not full history"} />}
          </Surface>
          </>}
        </TabsContent>

        <TabsContent value="history" className="mt-3 space-y-4">
          <MetricStrip>
            <SummaryCard
              label={lang === "zh" ? "请求记录" : "Request records"}
              value={snapshot.hasRequests ? nf(snapshot.requestCount) : unavailableValue}
            />
            <SummaryCard
              label={lang === "zh" ? "估算累计节省" : "Estimated saved tokens"}
              value={optionalNumber(snapshot.estimatedSavedTokens)}
            />
            <SummaryCard
              label={lang === "zh" ? "有记录日期" : "Recorded days"}
              value={snapshot.trend30d.length > 0 ? nf(snapshot.trend30d.length) : unavailableValue}
            />
            <SummaryCard
              label={lang === "zh" ? "请求错误" : "Request errors"}
              value={snapshot.hasRequests ? nf(snapshot.errorCount) : unavailableValue}
            />
          </MetricStrip>

          <Surface className="p-4" hover={false}>
            <h3 className="mb-3 text-sm">{lang === "zh" ? "近 30 日有记录日期" : "Recorded days in the latest 30-day window"}</h3>
            {snapshot.trend30d.length > 0 ? (
              <div className="h-56" data-testid="analytics-trend">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={snapshot.trend30d}>
                    <XAxis dataKey="day" tick={{ fontSize: 10, fill: "var(--muted-foreground)" }} axisLine={false} tickLine={false} />
                    <Tooltip
                      cursor={{ fill: "var(--muted)" }}
                      contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }}
                    />
                    <Bar dataKey="requests" name={lang === "zh" ? "请求" : "Requests"} fill="var(--chart-2)" radius={[4, 4, 0, 0]} />
                    <Bar dataKey="saved" name={lang === "zh" ? "估算节省 Token" : "Estimated saved tokens"} fill="var(--chart-accent)" radius={[4, 4, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            ) : (
              <NoData
                title={lang === "zh" ? "暂无趋势记录" : "No trend records"}
                detail={lang === "zh" ? "只有带有效时间戳的真实请求才会进入趋势" : "Only real requests with valid timestamps appear here"}
              />
            )}
          </Surface>

          <Surface className="p-4" hover={false}>
            <h3 className="mb-3 text-sm">{lang === "zh" ? "日期汇总" : "Daily summary"}</h3>
            {snapshot.trend30d.length > 0 ? (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-border/50 text-left text-xs text-muted-foreground">
                      <th className="pb-2 font-normal">{lang === "zh" ? "日期" : "Date"}</th>
                      <th className="pb-2 font-normal">{lang === "zh" ? "请求" : "Requests"}</th>
                      <th className="pb-2 font-normal">{lang === "zh" ? "估算节省 Token" : "Estimated saved tokens"}</th>
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
                title={lang === "zh" ? "暂无日期汇总" : "No daily summary"}
                detail={lang === "zh" ? "连接本地服务并产生真实请求后显示" : "Connect the local service and send real requests"}
              />
            )}
          </Surface>
        </TabsContent>

        <TabsContent value="parity" className="mt-3">
          <Surface className="p-4" hover={false}>
            <h3 className="mb-1 text-sm">{lang === "zh" ? "当前数据覆盖" : "Current data coverage"}</h3>
            <p className="mb-3 text-xs text-muted-foreground">
              {lang === "zh"
                ? "数据范围：当前 d-token 本地请求快照中可核验的数据。缺失指标保持为空。"
                : "Data scope: metrics verifiable from the current local d-token request snapshot. Missing metrics remain empty."}
            </p>
            <div className="grid gap-2 sm:grid-cols-2">
              {coverageRows(snapshot, lang, priceCacheStatus).map((item) => (
                <div key={item.label} className="flat-inset flex items-center justify-between gap-2 p-2 text-sm">
                  <span>{item.label}</span>
                  <Badge variant={item.available ? "default" : "secondary"}>
                    {item.available
                      ? lang === "zh" ? "有真实记录" : "Recorded"
                      : lang === "zh" ? "暂无记录" : "No record"}
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
                  {lang === "zh" ? "请求日志" : "Request log"}
                </h3>
              </div>
              <span className="text-xs text-muted-foreground">
                {requestTotal > 0
                  ? `${requestTotal} ${lang === "zh" ? (nativePagination ? "条 runtime 记录" : "条当前快照") : (nativePagination ? "runtime records" : "in current snapshot")}`
                  : sourceLabel}
              </span>
            </div>
            <div className="mb-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-[minmax(0,1fr)_10rem_12rem_auto]">
              <label className="min-w-0">
                <span className="sr-only">{lang === "zh" ? "搜索请求日志" : "Search request logs"}</span>
                <input
                  type="search"
                  aria-label={lang === "zh" ? "搜索请求日志" : "Search request logs"}
                  value={logSearch}
                  onChange={(event) => { setLogSearch(event.target.value); if (!nativePagination) setRequestPage(0); }}
                  placeholder={lang === "zh" ? "搜索 request_id / Provider / 模型 / 协议 / 状态" : "Search request id, provider, model, protocol, or status"}
                  className="h-9 w-full min-w-0 rounded-md border border-input bg-background px-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
                />
              </label>
              <select aria-label={lang === "zh" ? "状态筛选" : "Status filter"} value={logStatusFilter} onChange={(event) => { setLogStatusFilter(event.target.value as typeof logStatusFilter); if (!nativePagination) setRequestPage(0); }} className="h-9 min-w-0 rounded-md border border-input bg-background px-2 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring">
                <option value="all">{lang === "zh" ? "全部状态" : "All statuses"}</option>
                <option value="ok">{lang === "zh" ? "完成" : "Completed"}</option>
                <option value="failed">{lang === "zh" ? "失败" : "Failed"}</option>
                <option value="unknown">{lang === "zh" ? "未知" : "Unknown"}</option>
              </select>
              <select aria-label={lang === "zh" ? "Provider 筛选" : "Provider filter"} value={logProviderFilter} onChange={(event) => { setLogProviderFilter(event.target.value); if (!nativePagination) setRequestPage(0); }} className="h-9 min-w-0 rounded-md border border-input bg-background px-2 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring">
                <option value="all">{lang === "zh" ? "全部 Provider" : "All providers"}</option>
                {logProviders.map((provider) => <option key={provider} value={provider}>{provider}</option>)}
              </select>
              <div className="flex h-9 items-center justify-end whitespace-nowrap text-xs text-muted-foreground">
                {lang === "zh" ? `显示 ${filteredLogRows.length} / ${requestRows.length}` : `Showing ${filteredLogRows.length} / ${requestRows.length}`}
              </div>
            </div>
            {nativePageError && <div role="status" className="mb-2 text-xs text-amber-600 dark:text-amber-400">{nativePageErrorMessage}</div>}
            {nativeRequestPage && <div className="mb-2 text-xs text-muted-foreground">{lang === "zh" ? `明细源 d-token runtime · fresh · r${requestRevision}${summaryRevision === null ? "" : `；摘要 r${summaryRevision}`}${requestRevisionAligned === false ? "。runtime 在两次读取间已更新，明细与摘要需分别解读。" : ""}` : `Detail source d-token runtime · fresh · r${requestRevision}${summaryRevision === null ? "" : `; summary r${summaryRevision}`}${requestRevisionAligned === false ? ". The runtime changed between reads; interpret detail and summary as separate snapshots." : ""}`}</div>}
            {nativeInitialLoading ? <div role="status" className="rounded-lg border border-dashed border-border/60 bg-muted/15 px-3 py-3 text-xs text-muted-foreground">{lang === "zh" ? "正在读取请求记录…" : "Loading request records…"}</div> : visibleLogRows.length > 0 ? <>
              <div data-testid="analytics-log-grid" className="grid min-w-0 gap-2">
                {visibleLogRows.map((row, index) => {
                  const rowKey = `${row.request_id ?? "row"}-${row.timestamp_ms ?? index}`;
                  const expanded = expandedRequestId === rowKey;
                  return <RequestLogRowView key={rowKey} row={row} rowKey={rowKey} index={index} lang={lang} expanded={expanded} priceCache={priceCache} onToggle={() => setExpandedRequestId(expanded ? null : rowKey)} />;
                })}
              </div>
              {nativeRequestPage?.coverage?.complete === false && <div className="mt-2 text-xs text-muted-foreground">{lang === "zh" ? `明细最多覆盖最近 ${nf(nativeRequestPage.coverage.loaded_limit ?? 10_000)} 条 runtime 记录。` : `Detail coverage is limited to the latest ${nf(nativeRequestPage.coverage.loaded_limit ?? 10_000)} runtime records.`}</div>}
            </> : requestRows.length > 0 ? <NoData title={lang === "zh" ? "没有匹配的请求日志" : "No matching request logs"} detail={lang === "zh" ? "筛选只作用于当前有界页；可继续翻页查看其他 runtime 记录。" : "Filters apply only to the current bounded page; continue paging to inspect other runtime records."} /> : <NoData title={lang === "zh" ? "暂无请求日志" : "No request logs"} detail={lang === "zh" ? "此列表只展示 runtime 提供的有界快照，不代表全量历史" : "This list shows only the bounded runtime snapshot, not full history"} />}
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

function ModelUsageTable({ rows, lang }: { rows: ModelUsageRow[]; lang: "zh" | "en" }) {
  if (rows.length === 0) {
    return (
      <div className="p-4">
        <NoData
          title={lang === "zh" ? "暂无模型记录" : "No model records"}
          detail={lang === "zh" ? "收到真实请求后按模型展示" : "Models appear after real requests arrive"}
        />
      </div>
    );
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[520px] text-sm" data-testid="analytics-model-table">
        <thead>
          <tr className="border-b border-border/50 text-left text-xs text-muted-foreground">
            <th className="px-4 py-2 font-normal">{lang === "zh" ? "模型" : "Model"}</th>
            <th className="px-4 py-2 text-right font-normal">{lang === "zh" ? "请求" : "Requests"}</th>
            <th className="px-4 py-2 text-right font-normal">{lang === "zh" ? "节省 Token" : "Saved tokens"}</th>
            <th className="px-4 py-2 text-right font-normal">{lang === "zh" ? "估算节省费用" : "Estimated savings"}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.label} className="border-b border-border/30 last:border-0">
              <td className="max-w-64 truncate px-4 py-2.5 font-medium" title={row.label}>{row.label}</td>
              <td className="px-4 py-2.5 text-right tabular-nums">{nf(row.count)}</td>
              <td className="px-4 py-2.5 text-right tabular-nums">{optionalNumber(row.tokensSaved)}</td>
              <td className="px-4 py-2.5 text-right tabular-nums">{row.cost}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ToolTrafficBars({ rows, lang }: { rows: BreakdownItem[]; lang: "zh" | "en" }) {
  if (rows.length === 0) {
    return (
      <NoData
        title={lang === "zh" ? "暂无工具流量" : "No tool traffic"}
        detail={lang === "zh" ? "真实请求出现后显示工具占比" : "Tool share appears after real requests arrive"}
      />
    );
  }
  const total = rows.reduce((sum, row) => sum + row.count, 0);
  return (
    <div className="space-y-2.5" data-testid="analytics-tool-traffic">
      {rows.map((row) => {
        const share = total > 0 ? (row.count / total) * 100 : 0;
        return (
          <div key={row.label} className="min-w-0">
            <div className="flex items-center justify-between gap-3 text-sm">
              <span className="min-w-0 truncate">{row.label}</span>
              <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
                {share.toFixed(1)}% · {nf(row.count)}
              </span>
            </div>
            <div
              className="mt-1.5 h-2 overflow-hidden rounded-full bg-muted"
              role="meter"
              aria-label={`${row.label} ${share.toFixed(1)}%`}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Number(share.toFixed(1))}
            >
              <div
                aria-hidden
                role="presentation"
                className="h-full rounded-full bg-[var(--chart-accent)] transition-[width] duration-300 motion-reduce:transition-none"
                style={{ width: `${share}%` }}
              />
            </div>
          </div>
        );
      })}
    </div>
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
        title={lang === "zh" ? "暂无真实请求记录" : "No real request records"}
        detail={lang === "zh" ? "连接本地服务并产生请求后显示" : "Connect the local service and send a request"}
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
              {row.count} {lang === "zh" ? "次请求" : "requests"}
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
              {lang === "zh" ? "估算节省 Token" : "Estimated saved tokens"} · {optionalNumber(row.tokensSaved)}
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
    : lang === "zh" ? "按本地请求日志中的 Token 字段估算" : "Estimated from token fields in local request logs";
}

function buildModelUsageRows(
  breakdown: BreakdownItem[],
  requestRows: RequestLogRow[],
  priceCache: ModelPriceCache | null,
  displayedRequestCount: number,
): ModelUsageRow[] {
  return breakdown.map((item) => ({
    label: item.label,
    count: item.count,
    tokensSaved: item.tokensSaved,
    cost: modelCostLabel(item, requestRows, priceCache, displayedRequestCount),
  }));
}

function modelCostLabel(item: BreakdownItem, requestRows: RequestLogRow[], priceCache: ModelPriceCache | null, displayedRequestCount: number): string {
  if (!priceCache) return unavailableValue;
  if (requestRows.length === 0 || requestRows.length !== displayedRequestCount) return unavailableValue;
  const canonical = normalizeModelId(item.label);
  if (!canonical) return unavailableValue;
  const matchingRows = requestRows.filter((row) => normalizeModelId(row.model_id ?? row.model) === canonical);
  if (matchingRows.length === 0 || matchingRows.length !== item.count) return unavailableValue;
  let currency: string | null = null;
  let savedCost = 0;
  for (const row of matchingRows) {
    const estimate = estimateRequestInputCost(priceCache, row.model_id ?? row.model, row.input_tokens_before, row.input_tokens_after);
    if (!estimate.available) return unavailableValue;
    if (currency !== null && currency !== estimate.currency) return unavailableValue;
    currency = estimate.currency;
    savedCost += estimate.savedCost;
  }
  if (currency === null) return unavailableValue;
  const symbol = currency === "CNY" ? "¥" : "$";
  return `${symbol}${savedCost.toFixed(6)}`;
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
  const scopeWarning = lang === "zh"
    ? "总量指标来自持久化聚合；分布、耗时和趋势只覆盖当前返回的最近请求窗口。"
    : "Totals come from durable aggregates; breakdowns, latency, and trends cover the returned recent-request window.";
  const runtimeDaily = runtimeTrend(analytics.daily);
  const coverage = recordValue(analytics.coverage);
  const coverageWarning = coverage.complete === false
    ? (lang === "zh" ? `分析聚合最多覆盖最近 ${nf(nonNegativeMetric(coverage.loaded_limit) ?? 10_000)} 条 runtime 记录。` : `Analytics aggregates cover at most the latest ${nf(nonNegativeMetric(coverage.loaded_limit) ?? 10_000)} runtime records.`)
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
      ? lang === "zh" ? "d-token 本地请求数据" : "Local d-token request data"
      : lang === "zh" ? "本地服务在线 · 暂无请求记录" : "Local service online · no request records";
  }
  if (state === "preview") return lang === "zh" ? "浏览器预览 · 未连接真实数据" : "Browser preview · no live data";
  if (state === "loading") return lang === "zh" ? "正在连接 d-token 本地服务" : "Connecting to the local d-token service";
  if (state === "stale") return lang === "zh" ? "本地数据已过期" : "Local data is stale";
  if (state === "partial") return lang === "zh" ? "本地数据不完整" : "Local data is incomplete";
  return lang === "zh" ? "d-token 本地服务暂不可用" : "Local d-token service unavailable";
}

function analyticsStatusDetail(state: string, hasRequests: boolean, lang: "zh" | "en") {
  if (state === "verified" && hasRequests) {
    return lang === "zh"
      ? "所有指标由同一份 d-token 本地请求快照生成，缺失字段保持为空。"
      : "Every metric is generated from one local d-token request snapshot.";
  }
  if (state === "verified") {
    return lang === "zh"
      ? "本地服务已连接；收到真实请求后，这里才会显示指标与分布。"
      : "The local service is connected; metrics appear after real requests arrive.";
  }
  if (state === "stale") {
    return lang === "zh"
      ? "保留上次可验证指标用于追溯；这些数值不代表当前服务状态。"
      : "The last verified metrics are retained for traceability; they do not represent the current service state.";
  }
  return lang === "zh"
    ? "当前没有可核验的实时请求数据，缺失指标保持为空。"
    : "No verifiable live request data is available; missing metrics remain empty.";
}

function coverageRows(snapshot: AnalyticsSnapshot, lang: "zh" | "en", priceCacheStatus: "loading" | "ready" | "unavailable") {
  return [
    { label: lang === "zh" ? "请求记录" : "Request records", available: snapshot.hasRequests },
    { label: lang === "zh" ? "输入 Token 估算" : "Estimated input tokens", available: snapshot.estimatedInputTokens !== null },
    { label: lang === "zh" ? "输出 Token 估算" : "Estimated output tokens", available: snapshot.estimatedOutputTokens !== null },
    { label: lang === "zh" ? "节省 Token 估算" : "Estimated saved tokens", available: snapshot.estimatedSavedTokens !== null },
    { label: lang === "zh" ? "压缩耗时" : "Compression latency", available: snapshot.latency !== null },
    { label: lang === "zh" ? "Agent 标识" : "Agent identity", available: snapshot.agentBreakdown.length > 0 },
    { label: lang === "zh" ? "路由标识" : "Route identity", available: snapshot.routeBreakdown.length > 0 },
    { label: lang === "zh" ? "上游标识" : "Provider identity", available: snapshot.upstreamBreakdown.length > 0 },
    { label: lang === "zh" ? "时间趋势" : "Time trend", available: snapshot.trend30d.length > 0 },
    { label: lang === "zh" ? "处理步骤" : "Processing steps", available: snapshot.strategyBreakdown.length > 0 },
    { label: lang === "zh" ? "模型标识" : "Model identity", available: snapshot.modelBreakdown.length > 0 },
    { label: lang === "zh" ? "输入协议" : "Input protocol", available: snapshot.protocolBreakdown.length > 0 },
    { label: lang === "zh" ? "项目标识" : "Project identity", available: snapshot.projectBreakdown.length > 0 },
    { label: lang === "zh" ? "中转链模式" : "Relay chain mode", available: snapshot.chainBreakdown.length > 0 },
    { label: lang === "zh" ? "失败原因" : "Failure reasons", available: snapshot.errorBreakdown.length > 0 },
    { label: lang === "zh" ? "缓存读写与对齐" : "Cache alignment", available: snapshot.cache !== null },
    { label: lang === "zh" ? "流水线分段耗时" : "Pipeline stage timing", available: snapshot.pipeline !== null },
    { label: lang === "zh" ? "TTFB 与吞吐" : "TTFB and throughput", available: snapshot.pipeline?.upstreamTtfbMs !== null && snapshot.pipeline?.upstreamTtfbMs !== undefined },
    { label: lang === "zh" ? "缓存失效与未命中归因" : "Cache expiry and miss attribution", available: snapshot.cache !== null && (snapshot.cache.busts > 0 || snapshot.cache.misses > 0) },
    { label: lang === "zh" ? "成本与定价来源" : "Cost and pricing source", available: priceCacheStatus === "ready" },
    { label: lang === "zh" ? "输出整形反事实" : "Output-shaping counterfactual", available: false },
    { label: lang === "zh" ? "工具定义压缩节省" : "Tool definition savings", available: false },
    { label: lang === "zh" ? "Checkpoint / 周 / 月历史" : "Checkpoint / weekly / monthly history", available: false },
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
      <span className="text-xs text-muted-foreground">{lang === "zh" ? `第 ${currentRequestPage + 1} / ${requestPageCount} 页` : `Page ${currentRequestPage + 1} / ${requestPageCount}`}</span>
      <div className="flex gap-2">
        <Button size="sm" variant="outline" disabled={currentRequestPage === 0 || nativePageStatus === "loading"} onClick={onPrevious}>{lang === "zh" ? "上一页" : "Previous"}</Button>
        <Button size="sm" variant="outline" disabled={!requestHasMore || nativePageStatus === "loading"} onClick={onNext}>{nativePageStatus === "loading" ? (lang === "zh" ? "加载中…" : "Loading…") : (lang === "zh" ? "下一页" : "Next")}</Button>
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
          {status === "failed" ? <Badge variant="destructive">{lang === "zh" ? "失败" : "Error"}</Badge> : status === "ok" ? <Badge variant="secondary">{lang === "zh" ? "完成" : "OK"}</Badge> : <Badge variant="outline">{lang === "zh" ? "未知" : "Unknown"}</Badge>}
          <span className="whitespace-nowrap font-mono">{formatTimestamp(row.timestamp_ms)}</span>
          <span className="min-w-0 truncate">{safeCell(row.agent)}</span>
          <span className="min-w-0 truncate">{safeCell(row.model_id ?? row.model)}</span>
          <span className="min-w-0 truncate">{safeCell(row.provider_id ?? row.provider)}</span>
          <span className="whitespace-nowrap">{protocol}</span>
        </div>
        <Button size="sm" variant="ghost" aria-expanded={expanded} aria-controls={detailId} onClick={onToggle} className="shrink-0 gap-1 px-2 text-xs">
          {expanded ? <ChevronUp className="size-3.5" /> : <ChevronDown className="size-3.5" />}
          {expanded ? (lang === "zh" ? "收起" : "Collapse") : (lang === "zh" ? "查看详情" : "Details")}
        </Button>
      </div>

      <div className="mt-3 grid min-w-0 grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6">
        <LogValue label="Local Token" value={tokenEvidenceCell(row.input_tokens_before)} />
        <LogValue label="Provider Token" value={tokenEvidenceCell(row.provider_input_tokens)} />
        <LogValue label={lang === "zh" ? "压缩前后" : "Before → after"} value={beforeAfterCell(row)} />
        <LogValue label={lang === "zh" ? "节省 Token" : "Saved tokens"} value={tokenEvidenceCell(row.tokens_saved)} />
        <LogValue label={lang === "zh" ? "耗时" : "Latency"} value={durationCell(row.latency_ms ?? row.end_to_end_ms ?? row.compression_ms)} />
        <LogValue label={lang === "zh" ? "路由" : "Route"} value={safeCell(row.route_id ?? row.route)} />
      </div>

      {expanded && (
        <div id={detailId} className="mt-3 grid min-w-0 gap-3 border-t border-border/50 pt-3 sm:grid-cols-2">
          <LogDetailSection title={lang === "zh" ? "路由身份" : "Route identity"} rows={routeIdentityRows(row, lang)} />
          <LogDetailSection title={lang === "zh" ? "Token 证据" : "Token evidence"} rows={tokenDetailRows(row, lang)} />
          <LogDetailSection title={lang === "zh" ? "性能与费用" : "Performance and cost"} rows={performanceCostRows(row, lang, priceCache)} />
          <section className="min-w-0 rounded-md bg-muted/20 p-3">
            <h4 className="text-[11px] font-semibold">{lang === "zh" ? "证据 Receipt" : "Evidence receipt"}</h4>
            {row.transforms_applied && row.transforms_applied.length > 0 && (
              <div className="mt-2">
                <div className="mb-1.5 text-[11px] text-muted-foreground">{lang === "zh" ? "变换证据" : "Transform evidence"}</div>
                <div className="flex flex-wrap gap-1.5">
                  {row.transforms_applied.map((transform) => <Badge key={transform} variant="secondary" className="text-[11px]">{safeCell(transform)}</Badge>)}
                </div>
              </div>
            )}
            <div className="mt-2 text-[11px] text-muted-foreground">{receipt.length > 0 ? (lang === "zh" ? "Optimization Receipt（脱敏白名单）" : "Optimization Receipt (redacted whitelist)") : (lang === "zh" ? "Optimization Receipt（runtime 未合并 · 暂不可用）" : "Optimization Receipt (runtime unavailable)")}</div>
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
  const zh = lang === "zh";
  return [
    { label: zh ? "请求标识" : "Request id", value: evidenceValue(row.request_id) },
    { label: zh ? "时间" : "Time", value: typeof row.timestamp_ms === "number" ? formatTimestamp(row.timestamp_ms) : unavailableValue },
    { label: "Agent", value: evidenceValue(row.agent) },
    { label: zh ? "界面（实例）" : "Interface (instance)", value: evidenceValue(row.instance_id) },
    { label: zh ? "模型" : "Model", value: evidenceValue(row.model_id ?? row.model) },
    { label: "Provider", value: evidenceValue(row.provider_id ?? row.provider) },
    { label: zh ? "路由" : "Route", value: evidenceValue(row.route_id ?? row.route) },
    { label: zh ? "输入协议" : "Input protocol", value: displayProtocol(row.input_protocol ?? row.protocol) },
    { label: zh ? "输出协议" : "Output protocol", value: displayProtocol(row.output_protocol) },
    { label: zh ? "中转链模式" : "Relay chain mode", value: evidenceValue(row.chain_mode) },
  ];
}

function tokenDetailRows(row: RequestLogRow, lang: "zh" | "en"): LogEvidenceRow[] {
  return [
    { label: "Local Token", value: tokenEvidenceCell(row.input_tokens_before) },
    { label: lang === "zh" ? "Local 压缩后 Token" : "Local compressed tokens", value: tokenEvidenceCell(row.input_tokens_after) },
    { label: "Provider Token", value: tokenEvidenceCell(row.provider_input_tokens) },
    { label: lang === "zh" ? "输出 Token" : "Output tokens", value: tokenEvidenceCell(row.output_tokens) },
    { label: lang === "zh" ? "节省 Token" : "Saved tokens", value: tokenEvidenceCell(row.tokens_saved) },
  ];
}

function performanceCostRows(row: RequestLogRow, lang: "zh" | "en", priceCache: ModelPriceCache | null): LogEvidenceRow[] {
  const zh = lang === "zh";
  const source = row as RequestLogRow & { cost_usd?: unknown; provider_cost_usd?: unknown };
  const estimate = estimateRequestInputCost(priceCache, row.model_id ?? row.model, row.input_tokens_before, row.input_tokens_after);
  const rows: LogEvidenceRow[] = [
    { label: zh ? "压缩耗时" : "Compression latency", value: durationCell(row.compression_ms) },
    { label: zh ? "路由耗时" : "Route latency", value: durationCell(row.route_ms) },
    { label: zh ? "上游 TTFB" : "Upstream TTFB", value: durationCell(row.upstream_ttfb_ms) },
    { label: zh ? "端到端耗时" : "End-to-end latency", value: durationCell(row.end_to_end_ms ?? row.latency_ms) },
    { label: zh ? "上游账单费用" : "Upstream billed cost", value: evidenceValue(source.provider_cost_usd ?? source.cost_usd, (value) => `$${value.toFixed(6)}`) },
  ];
  if (estimate.available) {
    const symbol = estimate.currency === "CNY" ? "¥" : "$";
    const money = (value: number) => `${symbol}${value.toFixed(6)}`;
    rows.push(
      { label: zh ? "估算压缩前输入成本" : "Est. pre-compression input cost", value: money(estimate.preCost) },
      { label: zh ? "估算压缩后输入成本" : "Est. post-compression input cost", value: money(estimate.postCost) },
      { label: zh ? "估算费用差额" : "Est. cost difference", value: money(estimate.savedCost) },
      { label: zh ? "定价依据" : "Pricing basis", value: `${estimate.currency} ${estimate.source} ${estimate.freshness}` },
    );
  } else {
    rows.push(
      { label: zh ? "估算压缩前输入成本" : "Est. pre-compression input cost", value: unavailableValue },
      { label: zh ? "估算压缩后输入成本" : "Est. post-compression input cost", value: unavailableValue },
      { label: zh ? "估算费用差额" : "Est. cost difference", value: unavailableValue },
      { label: zh ? "定价依据" : "Pricing basis", value: zh ? "价格未知" : "price unknown" },
    );
  }
  rows.push({ label: zh ? "失败类型" : "Failure type", value: evidenceValue(row.error_type ?? row.error) });
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
  const zh = lang === "zh";
  const flat: Array<[string, string]> = [
    ["disposition", zh ? "优化决策" : "Disposition"],
    ["sent_payload", zh ? "发送载荷" : "Sent payload"],
    ["upstream_state", zh ? "上游状态" : "Upstream state"],
    ["attribution_basis", zh ? "归因依据" : "Attribution basis"],
    ["provider_reconciled_input_savings", zh ? "Provider 对账节省" : "Provider reconciled savings"],
    ["fallback_reason_code", zh ? "回退原因" : "Fallback reason"],
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
      for (const [key, label] of [["input_tokens", zh ? "估算节省输入 Token" : "Estimated saved input tokens"], ["total_tokens", zh ? "估算节省总 Token" : "Estimated saved total tokens"]] as Array<[string, string]>) {
        const raw = signed[key];
        if (typeof raw === "number" && Number.isFinite(raw)) rows.push({ label, value: String(raw) });
      }
    }
  }
  return rows;
}
