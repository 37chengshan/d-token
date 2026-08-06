import { useEffect, useMemo, useState } from "react";
import { motion } from "motion/react";
import {
  Activity,
  AlertTriangle,
  ArrowDownWideNarrow,
  Bot,
  Boxes,
  Coins,
  Gauge,
  GitBranch,
  Route,
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
import { useI18n, useT } from "../../lib/i18n";
import { protocolLabel } from "../../lib/protocol";
import {
  buildAnalyticsSnapshot,
  type AnalyticsSnapshot,
  type BreakdownItem,
  type RequestLogRow,
} from "../../lib/analytics/snapshot";
import { useSessionTab } from "../../lib/sessionTab";
import { useRuntimeDashboardSelector, shallowEqualSlice, type RuntimeDashboardState } from "../../lib/useRuntimeDashboard";
import { classifyEvidence } from "../../lib/state/evidence";
import { redactString } from "../../lib/security/redaction";
import { invokeNative, isTauriRuntime } from "../../lib/native";

const nf = (value: number) => value.toLocaleString("en-US");
const unavailableValue = "—";
const ANALYTICS_TABS = ["live", "history", "parity"] as const;
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
  const [nativePageIndex, setNativePageIndex] = useState(0);
  const [nativePendingPageIndex, setNativePendingPageIndex] = useState<number | null>(null);
  const [nativeCursors, setNativeCursors] = useState<Array<string | null>>([null]);
  const [nativeRequestPage, setNativeRequestPage] = useState<AnalyticsRequestPage | null>(null);
  const [nativePageStatus, setNativePageStatus] = useState<"idle" | "loading" | "ready" | "error">("idle");
  const [nativePageError, setNativePageError] = useState<"invalid_cursor" | "invalid_evidence" | "unavailable" | null>(null);
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
      ? lang === "zh" ? "来自上次验证的 d-token 本地请求记录" : "From the last verified local d-token request records"
      : lang === "zh" ? "来自 d-token 本地请求记录" : "From local d-token request records"
    : sourceLabel;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1>{t("an.title")}</h1>
        <div className="flex flex-wrap items-center gap-2">
          <DataSourceBadge mode={dataSourceMode} label={sourceLabel} />
          <Badge variant={evidenceState === "verified" ? "default" : "secondary"}>
            {sourceLabel}{hasLastVerifiedEvidence && updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}
          </Badge>
        </div>
      </div>

      <Surface className="p-3" hover={false}>
        <div className="flex items-start gap-3">
          <Activity className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <div>
            <div className="text-sm font-semibold">{lang === "zh" ? "分析数据状态" : "Analytics data status"}</div>
            <div className="mt-0.5 text-xs text-muted-foreground">
              {analyticsStatusDetail(evidenceState, snapshot.hasRequests, lang)}
            </div>
          </div>
        </div>
      </Surface>

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="live">{lang === "zh" ? "价值概览" : "Value overview"}</TabsTrigger>
          <TabsTrigger value="history">{lang === "zh" ? "历史趋势" : "Trend history"}</TabsTrigger>
          <TabsTrigger value="parity">{lang === "zh" ? "数据范围" : "Data scope"}</TabsTrigger>
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
            <AnalyticsBreakdownCard icon={<Layers3 className="size-4" />} title={lang === "zh" ? "模型" : "Model"} rows={snapshot.modelBreakdown} lang={lang} />
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
            <div className="mb-3 flex items-center justify-between gap-2"><h3 className="text-sm">{lang === "zh" ? "请求明细（有界分页）" : "Request detail (bounded pages)"}</h3><span className="text-xs text-muted-foreground">{requestTotal > 0 ? `${requestTotal} ${lang === "zh" ? (nativePagination ? "条 runtime 记录" : "条当前快照") : (nativePagination ? "runtime records" : "in current snapshot")}` : sourceLabel}</span></div>
            {nativePageError && <div role="status" className="mb-2 text-xs text-amber-600 dark:text-amber-400">{nativePageError === "invalid_cursor" ? (lang === "zh" ? "分页游标已失效，已显示第一页数据。" : "The page cursor expired. First-page data is shown.") : nativePageError === "invalid_evidence" ? (lang === "zh" ? "分页结果缺少可信的来源与时效标记，已显示当前摘要快照。" : "The page result lacks trusted source and freshness markers. The current summary snapshot is shown.") : (lang === "zh" ? "请求分页暂不可用，已显示当前摘要快照。" : "Request pagination is unavailable. The current summary snapshot is shown.")}</div>}
            {nativeRequestPage && <div className="mb-2 text-xs text-muted-foreground">{lang === "zh" ? `明细源 d-token runtime · fresh · r${requestRevision}${summaryRevision === null ? "" : `；摘要 r${summaryRevision}`}${requestRevisionAligned === false ? "。runtime 在两次读取间已更新，明细与摘要需分别解读。" : ""}` : `Detail source d-token runtime · fresh · r${requestRevision}${summaryRevision === null ? "" : `; summary r${summaryRevision}`}${requestRevisionAligned === false ? ". The runtime changed between reads; interpret detail and summary as separate snapshots." : ""}`}</div>}
            {nativeInitialLoading ? <div role="status" className="rounded-lg border border-dashed border-border/60 bg-muted/15 px-3 py-3 text-xs text-muted-foreground">{lang === "zh" ? "正在加载有界请求明细…" : "Loading bounded request detail…"}</div> : visibleRequestRows.length > 0 ? <>
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
              {requestTotal > REQUEST_PAGE_SIZE && <div className="mt-3 flex items-center justify-between"><span className="text-xs text-muted-foreground">{lang === "zh" ? `第 ${currentRequestPage + 1} / ${requestPageCount} 页` : `Page ${currentRequestPage + 1} / ${requestPageCount}`}</span><div className="flex gap-2"><Button size="sm" variant="outline" disabled={currentRequestPage === 0 || nativePageStatus === "loading"} onClick={() => nativePagination ? setNativePendingPageIndex(Math.max(0, currentRequestPage - 1)) : setRequestPage(Math.max(0, currentRequestPage - 1))}>{lang === "zh" ? "上一页" : "Previous"}</Button><Button size="sm" variant="outline" disabled={!requestHasMore || nativePageStatus === "loading"} onClick={() => { if (nativePagination) { const next = nativeRequestPage.next_cursor; if (!next) return; setNativeCursors((cursors) => [...cursors.slice(0, nativePageIndex + 1), next]); setNativePendingPageIndex(nativePageIndex + 1); } else { setRequestPage(Math.min(requestPageCount - 1, currentRequestPage + 1)); } }}>{nativePageStatus === "loading" ? (lang === "zh" ? "加载中…" : "Loading…") : (lang === "zh" ? "下一页" : "Next")}</Button></div></div>}
            </> : requestRows.length > 0 ? <div className="rounded-lg border border-dashed border-border/60 bg-muted/15 px-3 py-2 text-xs text-muted-foreground">{lang === "zh" ? "当前快照少于一页；请求维度已在上方分布中展示。" : "The current snapshot fits on one page; request dimensions are shown in the breakdowns above."}</div> : <NoData title={lang === "zh" ? "暂无请求明细" : "No request detail"} detail={lang === "zh" ? "此列表只展示 runtime 提供的有界快照，不代表全量历史" : "This table shows only the bounded runtime snapshot, not full history"} />}
          </Surface>
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
              {coverageRows(snapshot, lang).map((item) => (
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
    const label = raw === null ? null : (normalizeLabel ? normalizeLabel(raw) : redactString(raw).slice(0, 96));
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

function coverageRows(snapshot: AnalyticsSnapshot, lang: "zh" | "en") {
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
    { label: lang === "zh" ? "成本与定价来源" : "Cost and pricing source", available: false },
    { label: lang === "zh" ? "输出整形反事实" : "Output-shaping counterfactual", available: false },
    { label: lang === "zh" ? "工具定义压缩节省" : "Tool definition savings", available: false },
    { label: lang === "zh" ? "Checkpoint / 周 / 月历史" : "Checkpoint / weekly / monthly history", available: false },
  ];
}
