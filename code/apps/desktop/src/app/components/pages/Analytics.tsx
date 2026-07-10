import { useMemo } from "react";
import { motion } from "motion/react";
import {
  ArrowDownWideNarrow,
  Boxes,
  Coins,
  Database,
  DollarSign,
  Gauge,
  GitBranch,
  Timer,
} from "lucide-react";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  Cell,
  ResponsiveContainer,
  Tooltip,
  XAxis,
} from "recharts";
import { GlassCard } from "../common/GlassCard";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Badge } from "../ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { useI18n, useT } from "../../lib/i18n";
import { hr } from "../../lib/fixtures";
import { allocateIntegerTotal, deriveLatencyTotal, deriveTokenMetrics } from "../../lib/analytics/metrics";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";

const nf = (n: number) => n.toLocaleString("en-US");

function num(value: unknown, fallback = 0) {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

export function Analytics() {
  const t = useT();
  const { lang } = useI18n();
  const { payload, status: payloadStatus, updatedAt } = useRuntimeDashboard();
  const sourceLabel = payloadStatus === "runtime"
    ? lang === "zh" ? "d-token 本地服务实时数据" : "Live d-token local-service data"
    : payloadStatus === "browser-preview"
      ? lang === "zh" ? "浏览器预览数据" : "Browser preview data"
      : lang === "zh" ? "等待 d-token 本地服务数据" : "Waiting for d-token local service data";

  const runtimeStats = payload?.stats ?? {};
  const hasRuntimePayload = payloadStatus === "runtime";
  const isBrowserPreview = payloadStatus === "browser-preview";
  const shouldShowSample = isBrowserPreview;
  const runtimeCompressionCount = hasRuntimePayload && typeof runtimeStats.compressions === "number" ? runtimeStats.compressions : shouldShowSample ? hr.agentTotals.requests : 0;
  const dashboardSections = Array.isArray(payload?.dashboard_sections) ? payload.dashboard_sections : [];
  const desktopDataSource = typeof payload?.desktop_data_source === "string" ? payload.desktop_data_source : payloadStatus;
  const runtimeTokens = runtimeStats.tokens ?? {};
  const runtimeRequests = runtimeStats.requests ?? {};
  const runtimeBefore = hasRuntimePayload && typeof runtimeTokens.input === "number" ? runtimeTokens.input : shouldShowSample ? hr.agentTotals.before : 0;
  const runtimeAfter = hasRuntimePayload && typeof runtimeTokens.input_after === "number" ? runtimeTokens.input_after : shouldShowSample ? hr.agentTotals.after : 0;
  const runtimeTokenMetrics = deriveTokenMetrics(runtimeBefore, runtimeAfter);
  const runtimeTokensSaved = runtimeTokenMetrics.saved;
  const runtimeSavingsPercent = runtimeTokenMetrics.savingsPercent;
  const runtimeSaved = runtimeTokenMetrics.saved;
  const runtimeRequestTotal = hasRuntimePayload && typeof runtimeRequests.total === "number" ? runtimeRequests.total : runtimeCompressionCount;
  const reportedRuntimeAvgLatencyMs = hasRuntimePayload && typeof runtimeStats.avg_latency_ms === "number" ? runtimeStats.avg_latency_ms : 0;
  const cacheSummary = hasRuntimePayload ? payload?.cache_summary ?? {} : {};
  const memorySummary = hasRuntimePayload ? payload?.memory ?? {} : {};
  const requestLogs = Array.isArray(payload?.request_logs) ? payload.request_logs : [];
  const performanceTransforms = Array.isArray(payload?.metrics?.compression_performance?.transforms)
    ? payload.metrics.compression_performance.transforms
    : [];
  const routePerformance = payload?.metrics?.route_performance ?? {};
  const latencyBreakdown = hasRuntimePayload
    ? runtimeStats.latency_breakdown ?? routePerformance.latency_breakdown ?? {}
    : {};
  const routeOverhead = hasRuntimePayload
    ? runtimeStats.route_overhead ?? routePerformance.route_overhead ?? {}
    : {};
  const compressionLatencyMs = hasRuntimePayload ? num(latencyBreakdown.compression_ms) : shouldShowSample ? num(hr.throughput.compP50, 0) : 0;
  const routeAdapterLatencyMs = hasRuntimePayload ? num(latencyBreakdown.route_adapter_ms) : shouldShowSample ? hr.throughput.routeP50 : 0;
  const providerLatencyMs = hasRuntimePayload ? num(latencyBreakdown.provider_ms) : shouldShowSample ? num(hr.throughput.fwdP50, 0) : 0;
  const ccSwitchLatencyMs = hasRuntimePayload ? num(latencyBreakdown.cc_switch_ms) : 0;
  const derivedRequestPathLatencyMs = deriveLatencyTotal({
    compression: compressionLatencyMs,
    routeAdapter: routeAdapterLatencyMs,
    ccSwitch: ccSwitchLatencyMs,
    provider: providerLatencyMs,
  });
  const runtimeAvgLatencyMs = derivedRequestPathLatencyMs || reportedRuntimeAvgLatencyMs;
  const latencyModel = typeof latencyBreakdown.model === "string" ? latencyBreakdown.model : hasRuntimePayload ? (lang === "zh" ? "真实运行统计" : "runtime metrics") : (lang === "zh" ? "示例延迟" : "sample latency");
  const routeOverheadTokens = hasRuntimePayload ? num(routeOverhead.tokens) : 0;
  const routeOverheadPercent = hasRuntimePayload ? num(routeOverhead.percent) : 0;
  const routeOverheadBaseline = typeof routeOverhead.baseline === "string" ? routeOverhead.baseline : (lang === "zh" ? "上游输入 − 压缩后输入" : "provider input − runtime final");
  const fallbackPolicy = typeof routeOverhead.fallback_policy === "string" ? routeOverhead.fallback_policy : (lang === "zh" ? "示例未配置回退策略" : "No fallback policy in sample");
  const routeRows = hasRuntimePayload && Array.isArray(routePerformance.routes) ? routePerformance.routes : [];
  const providerRows = hasRuntimePayload && Array.isArray(routePerformance.providers) ? routePerformance.providers : [];
  const policyRows = hasRuntimePayload && Array.isArray(routePerformance.policies) ? routePerformance.policies : [];
  const runtimeHistory = hasRuntimePayload && Array.isArray(payload?.history?.history) && payload.history.history.length > 0
    ? payload.history.history
    : shouldShowSample
      ? hr.history
      : [];
  const runtimeDaily = runtimeHistory.map((item: any, index: number) => ({
    d: typeof item.ts === "string" ? item.ts : `#${index + 1}`,
    saved: typeof item.saved === "number" ? item.saved : 0,
  }));
  const activeDayCount = new Set(
    runtimeHistory.map((item: any) => typeof item.ts === "string" ? item.ts.split(" ")[0] : "unknown"),
  ).size;
  const runtimeAgentRows = useMemo(() => {
    if (!hasRuntimePayload || requestLogs.length === 0) return shouldShowSample ? hr.agents : [];
    const byAgent = new Map<string, { agent: string; label: string; source: string; requests: number; before: number; after: number; saved: number; savingsPercent: number; share: number; dot: string }>();
    for (const log of requestLogs) {
      const key = typeof log.agent === "string" ? log.agent : "unknown";
      const row = byAgent.get(key) ?? {
        agent: key,
        label: key,
        source: "d-token runtime",
        requests: 0,
        before: 0,
        after: 0,
        saved: 0,
        savingsPercent: 0,
        share: 0,
        dot: key === "OpenCode" ? "#38bdf8" : "#a78bfa",
      };
      row.requests += 1;
      const logMetrics = deriveTokenMetrics(
        Number(log.input_tokens_before ?? 0),
        Number(log.input_tokens_after ?? log.input_tokens_before ?? 0),
      );
      row.before += logMetrics.before;
      row.after += logMetrics.after;
      row.saved += logMetrics.saved;
      row.savingsPercent = row.before > 0 ? Number(((row.saved / row.before) * 100).toFixed(1)) : 0;
      byAgent.set(key, row);
    }
    const totalSaved = Array.from(byAgent.values()).reduce((sum, row) => sum + row.saved, 0);
    return Array.from(byAgent.values()).map((row) => ({
      ...row,
      share: totalSaved > 0 ? Number(((row.saved / totalSaved) * 100).toFixed(1)) : 0,
    }));
  }, [hasRuntimePayload, requestLogs, shouldShowSample]);
  const runtimePipeline = useMemo(() => {
    if (hasRuntimePayload && performanceTransforms.length > 0) {
      const attributed = allocateIntegerTotal(
        runtimeSaved,
        performanceTransforms.map((item: any) => Number(item.tokens_saved ?? 0)),
      );
      return performanceTransforms.map((item: any, index: number) => ({
        name: typeof item.name === "string" ? item.name : "runtime_route",
        kind: "按策略权重归因",
        requests: Number(item.requests ?? 0),
        tokens: attributed[index],
        ratio: runtimeSaved > 0 ? attributed[index] / runtimeSaved : 0,
      }));
    }
    if (!hasRuntimePayload || requestLogs.length === 0) return shouldShowSample ? hr.pipeline : [];
    const byTransform = new Map<string, { name: string; kind: string; requests: number; tokens: number; ratio: number }>();
    for (const log of requestLogs) {
      const transforms = Array.isArray(log.transforms_applied) && log.transforms_applied.length > 0 ? log.transforms_applied : ["runtime_route"];
      for (const transform of transforms) {
        const row = byTransform.get(transform) ?? { name: transform, kind: "runtime", requests: 0, tokens: 0, ratio: 0 };
        row.requests += 1;
        const logMetrics = deriveTokenMetrics(
          Number(log.input_tokens_before ?? 0),
          Number(log.input_tokens_after ?? log.input_tokens_before ?? 0),
        );
        row.tokens += logMetrics.saved / transforms.length;
        byTransform.set(transform, row);
      }
    }
    const rows = Array.from(byTransform.values());
    const attributed = allocateIntegerTotal(runtimeSaved, rows.map((row) => row.tokens));
    return rows.map((row, index) => ({
      ...row,
      tokens: attributed[index],
      ratio: runtimeSaved > 0 ? attributed[index] / runtimeSaved : 0,
    }));
  }, [hasRuntimePayload, performanceTransforms, requestLogs, runtimeSaved, shouldShowSample]);
  const pipelineTotals = useMemo(
    () => ({
      requests: runtimePipeline.reduce((sum: number, item: { requests: number }) => sum + item.requests, 0),
      tokens: runtimePipeline.reduce((sum: number, item: { tokens: number }) => sum + item.tokens, 0),
    }),
    [runtimePipeline],
  );
  const runtimeHasLogs = hasRuntimePayload && requestLogs.length > 0;
  const estimatedRuntimeCostSaved = runtimeHasLogs ? runtimeSaved / 1_000_000 * 3 : shouldShowSample ? hr.costSavedUsd : 0;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <h1>{t("an.title")}</h1>
        <span className="text-xs text-muted-foreground">{sourceLabel}</span>
        <DataSourceBadge mode={payloadStatus === "runtime" ? "runtime" : payloadStatus === "browser-preview" ? "preview" : "unavailable"} />
      </div>

      <GlassCard className="p-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <div className="text-sm font-semibold">{lang === "zh" ? "数据状态" : "Data status"}</div>
            <div className="text-xs text-muted-foreground">
              {payloadStatus === "runtime"
                ? lang === "zh"
                  ? `来自 d-token 本地服务 · ${dashboardSections.length} 类数据 · ${desktopDataSource}`
                  : `From d-token local service · ${dashboardSections.length} data groups · ${desktopDataSource}`
                : payloadStatus === "browser-preview"
                  ? "浏览器预览：桌面运行时不可读，仅展示预览数据。"
                  : "d-token 本地服务暂不可用，等待真实运行数据。"}
            </div>
          </div>
          <Badge variant={payloadStatus === "runtime" ? "default" : "secondary"}>
            {payloadStatus === "runtime"
              ? `${lang === "zh" ? "真实运行数据" : "Live runtime data"}${updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}`
              : payloadStatus === "browser-preview"
                ? lang === "zh" ? "示例数据" : "Sample data"
                : lang === "zh" ? "暂不可用" : "Unavailable"}
          </Badge>
        </div>
      </GlassCard>

      <Tabs defaultValue="live">
        <TabsList>
          <TabsTrigger value="live">{lang === "zh" ? "价值概览" : "Value overview"}</TabsTrigger>
          <TabsTrigger value="history">{lang === "zh" ? "历史趋势" : "History"}</TabsTrigger>
          <TabsTrigger value="parity">{lang === "zh" ? "数据范围" : "Data scope"}</TabsTrigger>
        </TabsList>

        <TabsContent value="live" className="mt-3 space-y-4">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-6">
            <Metric
              i={0}
              icon={<DollarSign className="size-3.5" />}
              label={lang === "zh" ? "成本节省估算" : "Estimated cost saved"}
              value={`$${estimatedRuntimeCostSaved.toFixed(3)}`}
              sub={runtimeHasLogs ? (lang === "zh" ? "按已节省 Token 粗略估算" : "Estimated from saved tokens") : shouldShowSample ? (lang === "zh" ? "示例估算，不代表实际账单" : "Sample estimate, not a bill") : (lang === "zh" ? "等待真实数据" : "Waiting for runtime")}
              color="var(--chart-2)"
            />
            <Metric
              i={1}
              icon={<Coins className="size-3.5" />}
              label={lang === "zh" ? "已节省 Token" : "Tokens saved"}
              value={nf(runtimeTokensSaved)}
              sub={`${runtimeSavingsPercent}% · ${hasRuntimePayload ? (lang === "zh" ? "真实运行数据" : "runtime data") : shouldShowSample ? (lang === "zh" ? "示例数据" : "sample data") : (lang === "zh" ? "等待数据" : "waiting")}`}
              color="var(--chart-accent)"
              spark
            />
            <Metric
              i={2}
              icon={<ArrowDownWideNarrow className="size-3.5" />}
              label={lang === "zh" ? "已处理请求" : "Routed requests"}
              value={nf(runtimeRequestTotal)}
              sub={runtimeHasLogs ? (lang === "zh" ? "已通过 d-token 路由" : "Routed through d-token") : shouldShowSample ? (lang === "zh" ? "示例数据" : "sample data") : (lang === "zh" ? "等待真实数据" : "waiting for runtime")}
              color="var(--chart-2)"
            />
            <Metric
              i={3}
              icon={<Timer className="size-3.5" />}
              label={lang === "zh" ? "平均请求耗时" : "Average request time"}
              value={`${runtimeAvgLatencyMs.toFixed(1)}ms`}
              sub={hasRuntimePayload ? (lang === "zh" ? "压缩、路由与上游合计" : "Compression, routing and upstream") : shouldShowSample ? (lang === "zh" ? "示例延迟" : "sample latency") : (lang === "zh" ? "等待真实数据" : "waiting for runtime")}
              color="var(--chart-4)"
            />
            <Metric
              i={4}
              icon={<Gauge className="size-3.5" />}
              label={lang === "zh" ? "输入 Token 基线" : "Input token baseline"}
              value={nf(Math.max(runtimeBefore, runtimeAfter))}
              sub={runtimeHasLogs ? (lang === "zh" ? "压缩前输入总量" : "Total input before compression") : shouldShowSample ? (lang === "zh" ? "示例输入总量" : "sample input total") : (lang === "zh" ? "等待真实数据" : "waiting for runtime")}
              color="var(--chart-3)"
            />
            <Metric
              i={5}
              icon={<GitBranch className="size-3.5" />}
              label={lang === "zh" ? "路由额外 Token" : "Route-added tokens"}
              value={nf(routeOverheadTokens)}
              sub={`${routeOverheadPercent.toFixed(2)}% · ${hasRuntimePayload ? (lang === "zh" ? "真实路由口径" : "runtime route baseline") : shouldShowSample ? (lang === "zh" ? "示例边界" : "sample boundary") : (lang === "zh" ? "等待真实数据" : "waiting")}`}
              color="var(--chart-5)"
            />
          </div>

          <div className="grid gap-3 lg:grid-cols-3">
            <GlassCard className="p-4 lg:col-span-2">
              <h3 className="mb-3 text-sm">{lang === "zh" ? "请求链路延迟参考" : "Request-path latency"} (p50 / p95)</h3>
              <div className="grid grid-cols-2 gap-x-6 gap-y-2 text-sm sm:grid-cols-4">
                <Kv label={lang === "zh" ? "压缩" : "Compression"} v={shouldShowSample ? `${hr.throughput.compP50} / ${hr.throughput.compP95} ms` : "0 / 0 ms"} c="text-emerald-500" />
                <Kv label={lang === "zh" ? "路由" : "Routing"} v={shouldShowSample ? `${hr.throughput.routeP50} / ${hr.throughput.routeP95} ms` : "0 / 0 ms"} c="text-[color:var(--chart-2)]" />
                <Kv label={lang === "zh" ? "上游转发" : "Upstream"} v={shouldShowSample ? `${nf(hr.throughput.fwdP50)} / ${nf(hr.throughput.fwdP95)} ms` : "0 / 0 ms"} c="text-sky-500" />
                <Kv label={lang === "zh" ? "生成" : "Generation"} v={shouldShowSample ? `${nf(hr.throughput.genP50)} / ${nf(hr.throughput.genP95)} ms` : "0 / 0 ms"} c="text-amber-500" />
              </div>
            </GlassCard>

            <GlassCard className="p-4">
              <h3 className="mb-2 text-sm">{lang === "zh" ? "输出精简估算" : "Estimated output reduction"}</h3>
              <div className="text-2xl font-semibold">{shouldShowSample ? hr.output.percent : 0}%</div>
              <div className="mt-1 text-xs text-muted-foreground">
                {shouldShowSample
                  ? lang === "zh"
                    ? `示例估算 · ${nf(hr.output.requests)} 次请求 · 区间 ${hr.output.ciLow}%–${hr.output.ciHigh}%`
                    : `Sample estimate · ${nf(hr.output.requests)} requests · range ${hr.output.ciLow}%–${hr.output.ciHigh}%`
                  : lang === "zh" ? "等待真实输出精简数据" : "Waiting for runtime output metrics"}
              </div>
            </GlassCard>
          </div>

          <GlassCard className="p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <h3 className="text-sm">
                {lang === "zh" ? "Agent 使用情况" : "Agent usage"}
              </h3>
              <Badge variant="secondary">{runtimeCompressionCount} {lang === "zh" ? "次压缩" : "compressions"}</Badge>
            </div>
            <div className="mb-4 grid grid-cols-2 gap-2 sm:grid-cols-4">
              <Summary label={lang === "zh" ? "压缩前" : "Before"} v={nf(runtimeBefore)} />
              <Summary label={lang === "zh" ? "压缩后" : "After"} v={nf(runtimeAfter)} />
              <Summary label={lang === "zh" ? "已节省" : "Saved"} v={nf(runtimeSaved)} c="var(--chart-accent)" />
              <Summary label={lang === "zh" ? "节省率" : "Savings"} v={`${runtimeSavingsPercent.toFixed(1)}%`} c="var(--chart-2)" />
            </div>
            <div className="space-y-3">
              {runtimeAgentRows.map((agent) => (
                <div key={agent.agent} className="glass-inset p-3">
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <span className="flex min-w-0 items-center gap-2 text-sm">
                      <span className="size-2 rounded-full" style={{ background: agent.dot }} />
                      {agent.label}
                      <span className="truncate text-xs text-muted-foreground">
                        · {agent.requests} {lang === "zh" ? "次请求" : "requests"} · {agent.source}
                      </span>
                    </span>
                    <span className="text-xs text-emerald-500">{lang === "zh" ? `节省 ${agent.savingsPercent}%` : `${agent.savingsPercent}% saved`}</span>
                  </div>
                  <div className="flex h-2.5 overflow-hidden rounded-full bg-muted">
                    <motion.div
                      initial={{ width: 0 }}
                      animate={{ width: `${agent.savingsPercent}%` }}
                      transition={{ duration: 0.6 }}
                      className="bg-emerald-500"
                    />
                    <div className="flex-1" style={{ background: "var(--chart-accent)", opacity: 0.55 }} />
                  </div>
                  <div className="mt-2 grid grid-cols-4 gap-2 text-xs">
                    <Kv label={lang === "zh" ? "压缩前" : "Before"} v={nf(agent.before)} />
                    <Kv label={lang === "zh" ? "压缩后" : "After"} v={nf(agent.after)} />
                    <Kv label={lang === "zh" ? "已节省" : "Saved"} v={nf(agent.saved)} c="text-[color:var(--chart-accent)]" />
                    <Kv label={lang === "zh" ? "占总节省" : "Share"} v={`${agent.share}%`} />
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>

          <div className="grid gap-3 lg:grid-cols-2">
            <GlassCard className="p-4">
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <DollarSign className="size-4" />
                {lang === "zh" ? "节省口径" : "Savings baseline"}
              </h3>
              <div className="grid grid-cols-2 gap-3">
                <div className="rounded-md border border-emerald-500/30 bg-emerald-500/5 p-3">
                  <div className="text-xs text-muted-foreground">{lang === "zh" ? "输入压缩估算" : "Input compression estimate"}</div>
                  <div className="text-xl font-semibold text-emerald-500">${(shouldShowSample ? hr.savings.compressionUsd : 0).toFixed(2)}</div>
                  <div className="text-xs text-muted-foreground">{shouldShowSample ? `${nf(hr.tokens.proxySaved)} ${lang === "zh" ? "个输入 Token" : "input tokens"}` : (lang === "zh" ? "等待真实数据" : "waiting for runtime")}</div>
                </div>
                <div className="rounded-md border border-sky-500/30 bg-sky-500/5 p-3">
                  <div className="text-xs text-muted-foreground">{lang === "zh" ? "上游缓存收益" : "Upstream cache savings"}</div>
                  <div className="text-xl font-semibold text-sky-500">{shouldShowSample ? (hr.savings.cacheUsd > 0 ? `$${hr.savings.cacheUsd.toFixed(2)}` : lang === "zh" ? "未计入" : "Not included") : "$0.00"}</div>
                  <div className="text-xs text-muted-foreground">{shouldShowSample ? (lang === "zh" ? "示例未提供上游账单口径" : "No provider billing baseline in sample") : (lang === "zh" ? "等待缓存数据" : "waiting for cache metrics")}</div>
                </div>
              </div>
            </GlassCard>

            <GlassCard className="p-4">
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Database className="size-4" />
                {lang === "zh" ? "本地缓存与记忆" : "Local cache and memory"}
              </h3>
              <div className="grid grid-cols-4 gap-2 text-center">
                <Summary label="CCR" v={nf(Number(cacheSummary.entries ?? 0))} />
                <Summary label={lang === "zh" ? "记忆" : "Memory"} v={nf(Number(memorySummary.entries ?? 0))} />
                <Summary label={lang === "zh" ? "原文恢复" : "Raw recovery"} v={cacheSummary.raw_available ? (lang === "zh" ? "可用" : "on") : (lang === "zh" ? "未启用" : "off")} c="var(--chart-2)" />
                <Summary label={lang === "zh" ? "请求" : "Requests"} v={nf(runtimeRequestTotal)} />
              </div>
              <div className="mt-3 flex flex-wrap gap-1.5">
                {(lang === "zh" ? ["d-token", "压缩缓存", "项目记忆"] : ["d-token", "compression cache", "project memory"]).map((provider) => (
                  <Badge key={provider} variant="outline" className="text-xs">
                    {provider}
                  </Badge>
                ))}
              </div>
            </GlassCard>
          </div>

          <GlassCard className="p-4">
            <div className="mb-3 flex items-center justify-between gap-2">
              <h3 className="flex items-center gap-2 text-sm">
                <Boxes className="size-4" />
                {lang === "zh" ? "压缩策略贡献" : "Compression strategy attribution"}
              </h3>
              <Badge variant="secondary">{nf(pipelineTotals.tokens)} {lang === "zh" ? "个 Token（不重复）" : "tokens, deduplicated"}</Badge>
            </div>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-[var(--glass-inset-border)] text-left text-xs text-muted-foreground">
                    <th className="pb-2 font-normal">{lang === "zh" ? "策略" : "Strategy"}</th>
                    <th className="pb-2 font-normal">{lang === "zh" ? "触发请求" : "Requests"}</th>
                    <th className="pb-2 font-normal">{lang === "zh" ? "归因节省" : "Attributed tokens"}</th>
                    <th className="pb-2 font-normal">{lang === "zh" ? "占比" : "Share"}</th>
                  </tr>
                </thead>
                <tbody>
                  {runtimePipeline.map((item: { name: string; kind: string; requests: number; tokens: number; ratio: number }) => (
                    <tr key={item.name} className="border-b border-border/30 last:border-0">
                      <td className="py-2">
                        <span className="flex items-center gap-2">
                          <GitBranch className="size-3.5 text-muted-foreground" />
                          {item.name}
                          <span className="text-xs text-muted-foreground">· {item.kind}</span>
                        </span>
                      </td>
                      <td className="py-2 text-muted-foreground">{nf(item.requests)}</td>
                      <td className="py-2">{nf(item.tokens)}</td>
                      <td className="py-2">
                        <Badge variant="secondary" className="text-xs">{(item.ratio * 100).toFixed(1)}%</Badge>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </GlassCard>

          <GlassCard className="p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <h3 className="flex items-center gap-2 text-sm">
                <Timer className="size-4" />
                {lang === "zh" ? "延迟与路由开销拆分" : "Latency and Route Overhead"}
              </h3>
              <Badge variant="secondary" className="max-w-full truncate text-xs">
                {latencyModel}
              </Badge>
            </div>
            <div className="grid gap-3 lg:grid-cols-[1.35fr_0.65fr]">
              <div className="glass-inset p-3">
                <div className="mb-3 flex items-center justify-between text-xs text-muted-foreground">
                  <span>{lang === "zh" ? "平均请求耗时" : "Average request path"}</span>
                  <span>{runtimeAvgLatencyMs.toFixed(1)}ms {lang === "zh" ? "合计" : "total"}</span>
                </div>
                <div className="space-y-3">
                  <LatencyPart
                    label={lang === "zh" ? "压缩" : "Compression"}
                    value={compressionLatencyMs}
                    total={runtimeAvgLatencyMs}
                    color="var(--chart-accent)"
                  />
                  <LatencyPart
                    label={lang === "zh" ? "路由适配" : "Route adapter"}
                    value={routeAdapterLatencyMs}
                    total={runtimeAvgLatencyMs}
                    color="var(--chart-2)"
                  />
                  <LatencyPart
                    label={lang === "zh" ? "cc-switch / 路由桥接" : "cc-switch / chain bridge"}
                    value={ccSwitchLatencyMs}
                    total={runtimeAvgLatencyMs}
                    color="var(--chart-4)"
                  />
                  <LatencyPart
                    label={lang === "zh" ? "上游服务" : "Upstream provider"}
                    value={providerLatencyMs}
                    total={runtimeAvgLatencyMs}
                    color="var(--chart-3)"
                  />
                </div>
              </div>
              <div className="glass-inset p-3">
                <div className="text-xs text-muted-foreground">{lang === "zh" ? "路由开销" : "Route overhead"}</div>
                <div className="mt-1 text-2xl font-semibold" style={{ color: "var(--chart-5)" }}>
                  {nf(routeOverheadTokens)}
                </div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {routeOverheadPercent.toFixed(2)}% · {routeOverheadBaseline}
                </div>
                <div className="mt-3 rounded-md border border-emerald-500/25 bg-emerald-500/5 p-2 text-xs text-emerald-600 dark:text-emerald-300">
                  {fallbackPolicy}
                </div>
                <div className="mt-3 text-xs text-muted-foreground">
                  {lang === "zh"
                    ? "这里单独列出压缩后到上游输入之间新增的 Token，避免把不同阶段的收益重复相加。"
                    : "Route overhead separates runtime-final tokens from provider-input tokens so RTK/runtime/provider savings are not double-counted."}
                </div>
              </div>
            </div>
          </GlassCard>

          <GlassCard className="p-4">
            <div className="mb-3 flex items-center justify-between gap-2">
              <h3 className="flex items-center gap-2 text-sm">
                <GitBranch className="size-4" />
                {lang === "zh" ? "路由与上游表现" : "Route and provider performance"}
              </h3>
              <Badge variant="secondary">{routeRows.length || providerRows.length ? (lang === "zh" ? "真实运行数据" : "runtime metrics") : (lang === "zh" ? "等待真实数据" : "waiting for runtime")}</Badge>
            </div>
            <div className="grid gap-3 lg:grid-cols-3">
              <PerfColumn lang={lang} title={lang === "zh" ? "路由" : "Routes"} rows={routeRows} />
              <PerfColumn lang={lang} title={lang === "zh" ? "上游" : "Providers"} rows={providerRows} />
              <PerfColumn lang={lang} title={lang === "zh" ? "策略" : "Policies"} rows={policyRows} />
            </div>
          </GlassCard>
        </TabsContent>

        <TabsContent value="history" className="mt-3 space-y-4">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Summary big label={lang === "zh" ? "累计成本节省估算" : "Estimated lifetime cost saved"} v={`$${(shouldShowSample ? hr.costSavedUsd : estimatedRuntimeCostSaved).toFixed(2)}`} c="var(--chart-2)" />
            <Summary big label={lang === "zh" ? "累计节省 Token" : "Lifetime tokens saved"} v={nf(shouldShowSample ? hr.tokens.saved : runtimeSaved)} />
            <Summary big label={lang === "zh" ? "活跃日平均" : "Average per active day"} v={nf(activeDayCount > 0 ? Math.round((shouldShowSample ? hr.tokens.saved : runtimeSaved) / activeDayCount) : 0)} />
            <Summary big label={lang === "zh" ? "活跃天数" : "Active days"} v={String(activeDayCount)} />
          </div>

          <GlassCard className="p-4">
            <h3 className="mb-3 text-sm">{lang === "zh" ? "各检查点节省 Token" : "Saved tokens by checkpoint"}</h3>
            <div className="h-56">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={runtimeDaily}>
                  <XAxis dataKey="d" tick={{ fontSize: 10, fill: "var(--muted-foreground)" }} axisLine={false} tickLine={false} />
                  <Tooltip
                    cursor={{ fill: "var(--muted)" }}
                    contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }}
                  />
                  <Bar dataKey="saved" radius={[4, 4, 0, 0]}>
                    {runtimeDaily.map((_: unknown, index: number) => (
                      <Cell key={index} fill="var(--chart-accent)" />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          </GlassCard>

          <GlassCard className="p-4">
            <h3 className="mb-3 text-sm">{lang === "zh" ? "最近统计检查点" : "Recent statistics checkpoints"}</h3>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-border/50 text-left text-xs text-muted-foreground">
                    <th className="pb-2 font-normal">{lang === "zh" ? "时间" : "Timestamp"}</th>
                    <th className="pb-2 font-normal">{lang === "zh" ? "请求" : "Requests"}</th>
                    <th className="pb-2 font-normal">{lang === "zh" ? "压缩前" : "Before"}</th>
                    <th className="pb-2 font-normal">{lang === "zh" ? "压缩后" : "After"}</th>
                    <th className="pb-2 font-normal">{lang === "zh" ? "节省" : "Saved"}</th>
                    <th className="pb-2 font-normal">{lang === "zh" ? "节省率" : "Ratio"}</th>
                    <th className="pb-2 font-normal">{lang === "zh" ? "成本估算" : "Estimate"}</th>
                  </tr>
                </thead>
                <tbody>
                  {runtimeHistory.map((item: any) => (
                    <tr key={item.ts} className="border-b border-border/30 font-mono text-xs last:border-0">
                      <td className="py-2">{item.ts}</td>
                      <td className="py-2">{item.requests}</td>
                      <td className="py-2 text-muted-foreground">{nf(item.before)}</td>
                      <td className="py-2 text-muted-foreground">{nf(item.after)}</td>
                      <td className="py-2" style={{ color: "var(--chart-accent)" }}>{nf(item.saved)}</td>
                      <td className="py-2">{item.ratio.toFixed(2)}</td>
                      <td className="py-2">${item.cost.toFixed(3)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </GlassCard>
        </TabsContent>

        <TabsContent value="parity" className="mt-3">
          <GlassCard className="p-4">
            <h3 className="mb-1 text-sm">{lang === "zh" ? "分析页数据范围" : "Analytics data scope"}</h3>
            <p className="mb-3 text-xs text-muted-foreground">
              {lang === "zh" ? "真实运行时仅展示 d-token 本地服务已经记录的数据；缺失项保持为空，不用示例值冒充。" : "Runtime mode only shows data recorded by the d-token local service. Missing values stay empty rather than being replaced by samples."}
            </p>
            <div className="grid gap-2 sm:grid-cols-2">
              {(lang === "zh" ? [
                "总体节省与请求数",
                "各 Agent 使用情况",
                "输入压缩前后对比",
                "压缩与路由延迟",
                "压缩策略贡献",
                "路由与上游表现",
                "缓存与记忆状态",
                "最近请求与历史趋势",
                "成本节省估算",
                "错误与回退状态",
              ] : [
                "Overall savings and requests",
                "Usage by agent",
                "Input before and after compression",
                "Compression and routing latency",
                "Compression strategy attribution",
                "Route and provider performance",
                "Cache and memory status",
                "Recent requests and history",
                "Estimated cost savings",
                "Errors and fallback status",
              ]).map((item) => (
                <div key={item} className="glass-inset flex items-center justify-between p-2 text-sm">
                  <span>{item}</span>
                  <Badge variant="secondary">{lang === "zh" ? "本地数据" : "Local data"}</Badge>
                </div>
              ))}
            </div>
          </GlassCard>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function Metric({
  i,
  icon,
  label,
  value,
  sub,
  color,
  spark,
}: {
  i: number;
  icon: React.ReactNode;
  label: string;
  value: string;
  sub: string;
  color: string;
  spark?: boolean;
}) {
  return (
    <motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: i * 0.04 }}>
      <GlassCard className="relative overflow-hidden p-3.5">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">{icon}{label}</div>
        <div className="mt-1.5 text-xl font-semibold tracking-tight" style={{ color }}>{value}</div>
        <div className="mt-0.5 truncate text-xs text-muted-foreground">{sub}</div>
        {spark && (
          <div className="mt-1 h-6 opacity-70">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={[3, 5, 4, 7, 6, 9, 8, 12].map((v, x) => ({ x, v }))}>
                <Area type="monotone" dataKey="v" stroke={color} strokeWidth={1.5} fill={color} fillOpacity={0.15} />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        )}
      </GlassCard>
    </motion.div>
  );
}

function Kv({ label, v, c }: { label: string; v: string; c?: string }) {
  return (
    <div>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={c ?? ""}>{v}</div>
    </div>
  );
}

function Summary({ label, v, c, big }: { label: string; v: string; c?: string; big?: boolean }) {
  return (
    <div className="glass-inset p-2.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={big ? "text-lg font-semibold" : "text-base font-semibold"} style={{ color: c }}>{v}</div>
    </div>
  );
}

function LatencyPart({ label, value, total, color }: { label: string; value: number; total: number; color: string }) {
  const pct = total > 0 ? Math.min(100, Math.max(0, (value / total) * 100)) : 0;
  return (
    <div>
      <div className="mb-1 flex items-center justify-between gap-2 text-xs">
        <span>{label}</span>
        <span className="text-muted-foreground">{value.toFixed(1)}ms</span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-muted">
        <motion.div
          initial={{ width: 0 }}
          animate={{ width: `${pct}%` }}
          transition={{ duration: 0.55 }}
          className="h-full"
          style={{ background: color }}
        />
      </div>
    </div>
  );
}

function PerfColumn({ title, rows, lang }: { title: string; rows: any[]; lang: "zh" | "en" }) {
  const visible = rows.length > 0 ? rows : [{ name: lang === "zh" ? "暂无真实运行数据" : "No runtime data", requests: 0, failed: 0, tokens_saved: 0, savings_percent: 0, avg_latency_ms: 0 }];
  return (
    <div className="glass-inset p-3">
      <div className="mb-2 text-xs font-semibold text-muted-foreground">{title}</div>
      <div className="space-y-2">
        {visible.slice(0, 4).map((row) => (
          <div key={`${title}-${row.name}`} className="rounded-md border border-border/40 p-2">
            <div className="flex items-center justify-between gap-2 text-sm">
              <span className="truncate">{row.name}</span>
              <Badge variant={Number(row.failed ?? 0) > 0 ? "destructive" : "secondary"} className="text-xs">
                {nf(Number(row.requests ?? 0))} {lang === "zh" ? "次" : "req"}
              </Badge>
            </div>
            <div className="mt-1 grid grid-cols-3 gap-2 text-xs text-muted-foreground">
              <span>{nf(Number(row.tokens_saved ?? 0))} {lang === "zh" ? "已节省" : "saved"}</span>
              <span>{Number(row.savings_percent ?? 0).toFixed(1)}%</span>
              <span>{Number(row.avg_latency_ms ?? 0).toFixed(1)}ms</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
