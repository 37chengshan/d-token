import { useMemo } from "react";
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
} from "lucide-react";
import {
  Bar,
  BarChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
} from "recharts";
import { GlassCard } from "../common/GlassCard";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Badge } from "../ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { useI18n, useT } from "../../lib/i18n";
import {
  buildAnalyticsSnapshot,
  type AnalyticsSnapshot,
  type BreakdownItem,
  type RequestLogRow,
} from "../../lib/analytics/snapshot";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";
import { classifyEvidence } from "../../lib/state/evidence";

const nf = (value: number) => value.toLocaleString("en-US");
const unavailableValue = "—";

export function Analytics() {
  const t = useT();
  const { lang } = useI18n();
  const { payload, status: payloadStatus, updatedAt } = useRuntimeDashboard();
  const evidenceState = useMemo(
    () => classifyEvidence({ payload, status: payloadStatus, requireRuntimeIdentity: true }),
    [payload, payloadStatus],
  );
  const snapshot = useMemo(() => {
    const logs = evidenceState === "verified" && Array.isArray(payload?.request_logs)
      ? payload.request_logs as RequestLogRow[]
      : [];
    return buildAnalyticsSnapshot(logs, {
      source: evidenceState === "verified"
        ? "dtoken-local-service"
        : evidenceState === "preview"
          ? "preview"
          : "empty",
    });
  }, [evidenceState, payload]);

  const sourceLabel = analyticsSourceLabel(evidenceState, snapshot.hasRequests, lang);
  const dataSourceMode = evidenceState === "verified"
    ? "runtime"
    : evidenceState === "preview" || evidenceState === "loading"
      ? "preview"
      : "unavailable";
  const requestEvidence = snapshot.hasRequests
    ? lang === "zh" ? "来自 d-token 本地请求记录" : "From local d-token request records"
    : sourceLabel;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1>{t("an.title")}</h1>
        <div className="flex flex-wrap items-center gap-2">
          <DataSourceBadge mode={dataSourceMode} />
          <Badge variant={evidenceState === "verified" ? "default" : "secondary"}>
            {sourceLabel}{evidenceState === "verified" && updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}
          </Badge>
        </div>
      </div>

      <GlassCard className="p-3" hover={false}>
        <div className="flex items-start gap-3">
          <Activity className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <div>
            <div className="text-sm font-semibold">{lang === "zh" ? "分析数据状态" : "Analytics data status"}</div>
            <div className="mt-0.5 text-xs text-muted-foreground">
              {analyticsStatusDetail(evidenceState, snapshot.hasRequests, lang)}
            </div>
          </div>
        </div>
      </GlassCard>

      <Tabs defaultValue="live">
        <TabsList>
          <TabsTrigger value="live">{lang === "zh" ? "价值概览" : "Value overview"}</TabsTrigger>
          <TabsTrigger value="history">{lang === "zh" ? "历史趋势" : "History"}</TabsTrigger>
          <TabsTrigger value="parity">{lang === "zh" ? "数据范围" : "Data scope"}</TabsTrigger>
        </TabsList>

        <TabsContent value="live" className="mt-3 space-y-4">
          <section
            data-testid="analytics-kpis"
            className="grid grid-cols-2 gap-3 lg:grid-cols-3 xl:grid-cols-6"
          >
            <Metric
              index={0}
              icon={<ArrowDownWideNarrow className="size-3.5" />}
              label={lang === "zh" ? "已处理请求" : "Routed requests"}
              value={snapshot.hasRequests ? nf(snapshot.requestCount) : unavailableValue}
              detail={requestEvidence}
              color="var(--chart-2)"
            />
            <Metric
              index={1}
              icon={<Gauge className="size-3.5" />}
              label={lang === "zh" ? "估算输入 Token" : "Estimated input tokens"}
              value={optionalNumber(snapshot.estimatedInputTokens)}
              detail={metricDetail(snapshot.estimatedInputTokens, sourceLabel, lang)}
              color="var(--chart-3)"
            />
            <Metric
              index={2}
              icon={<Coins className="size-3.5" />}
              label={lang === "zh" ? "估算节省 Token" : "Estimated tokens saved"}
              value={optionalNumber(snapshot.estimatedSavedTokens)}
              detail={metricDetail(snapshot.estimatedSavedTokens, sourceLabel, lang)}
              color="var(--chart-accent)"
            />
            <Metric
              index={3}
              icon={<GitBranch className="size-3.5" />}
              label={lang === "zh" ? "估算节省率" : "Estimated savings rate"}
              value={snapshot.estimatedSavingsRate === null
                ? unavailableValue
                : `${(snapshot.estimatedSavingsRate * 100).toFixed(1)}%`}
              detail={metricDetail(snapshot.estimatedSavingsRate, sourceLabel, lang)}
              color="var(--chart-4)"
            />
            <Metric
              index={4}
              icon={<Bot className="size-3.5" />}
              label={lang === "zh" ? "估算输出 Token" : "Estimated output tokens"}
              value={optionalNumber(snapshot.estimatedOutputTokens)}
              detail={metricDetail(snapshot.estimatedOutputTokens, sourceLabel, lang)}
              color="var(--chart-5)"
            />
            <Metric
              index={5}
              icon={<AlertTriangle className="size-3.5" />}
              label={lang === "zh" ? "请求错误" : "Request errors"}
              value={snapshot.hasRequests ? nf(snapshot.errorCount) : unavailableValue}
              detail={requestEvidence}
              color={snapshot.errorCount > 0 ? "var(--destructive)" : "var(--muted-foreground)"}
            />
          </section>

          {snapshot.warnings.length > 0 && (
            <GlassCard className="p-3" hover={false}>
              <div className="flex items-start gap-2 text-xs text-muted-foreground">
                <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-500" />
                <div className="space-y-1">
                  {snapshot.warnings.map((warning) => <div key={warning}>{warning}</div>)}
                </div>
              </div>
            </GlassCard>
          )}

          <div className="grid gap-3 lg:grid-cols-2">
            <GlassCard className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Timer className="size-4" />
                {lang === "zh" ? "请求耗时" : "Request latency"}
              </h3>
              {snapshot.latency ? (
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                  <Summary label={lang === "zh" ? "有耗时记录" : "Samples"} value={nf(snapshot.latency.sampleCount)} />
                  <Summary label={lang === "zh" ? "平均" : "Average"} value={`${snapshot.latency.averageMs.toFixed(1)}ms`} />
                  <Summary label="p50" value={`${snapshot.latency.p50Ms.toFixed(1)}ms`} />
                  <Summary label="p95" value={`${snapshot.latency.p95Ms.toFixed(1)}ms`} />
                </div>
              ) : (
                <NoData
                  title={lang === "zh" ? "暂无请求耗时记录" : "No request latency records"}
                  detail={lang === "zh" ? "收到包含耗时字段的真实请求后显示" : "Shown after real requests include latency evidence"}
                />
              )}
            </GlassCard>

            <GlassCard className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Bot className="size-4" />
                {lang === "zh" ? "Agent 分布" : "Agent distribution"}
              </h3>
              <BreakdownList rows={snapshot.agentBreakdown} lang={lang} />
            </GlassCard>
          </div>

          <div className="grid gap-3 lg:grid-cols-2">
            <GlassCard className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <Route className="size-4" />
                {lang === "zh" ? "路由分布" : "Route distribution"}
              </h3>
              <BreakdownList rows={snapshot.routeBreakdown} lang={lang} />
            </GlassCard>
            <GlassCard className="p-4" hover={false}>
              <h3 className="mb-3 flex items-center gap-2 text-sm">
                <GitBranch className="size-4" />
                {lang === "zh" ? "上游分布" : "Provider distribution"}
              </h3>
              <BreakdownList rows={snapshot.upstreamBreakdown} lang={lang} />
            </GlassCard>
          </div>

          <GlassCard className="p-4" hover={false}>
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
          </GlassCard>
        </TabsContent>

        <TabsContent value="history" className="mt-3 space-y-4">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
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
          </div>

          <GlassCard className="p-4" hover={false}>
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
          </GlassCard>

          <GlassCard className="p-4" hover={false}>
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
          </GlassCard>
        </TabsContent>

        <TabsContent value="parity" className="mt-3">
          <GlassCard className="p-4" hover={false}>
            <h3 className="mb-1 text-sm">{lang === "zh" ? "当前数据覆盖" : "Current data coverage"}</h3>
            <p className="mb-3 text-xs text-muted-foreground">
              {lang === "zh"
                ? "此页只展示当前 d-token 本地请求快照能够核验的数据；缺失指标保持为空。"
                : "This page only shows metrics verifiable from the current local d-token request snapshot."}
            </p>
            <div className="grid gap-2 sm:grid-cols-2">
              {coverageRows(snapshot, lang).map((item) => (
                <div key={item.label} className="glass-inset flex items-center justify-between gap-2 p-2 text-sm">
                  <span>{item.label}</span>
                  <Badge variant={item.available ? "default" : "secondary"}>
                    {item.available
                      ? lang === "zh" ? "有真实记录" : "Recorded"
                      : lang === "zh" ? "暂无记录" : "No record"}
                  </Badge>
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
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: index * 0.035 }}>
      <GlassCard className="h-full min-w-0 p-3.5" hover={false}>
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">{icon}{label}</div>
        <div className="mt-1.5 text-xl font-semibold tracking-tight" style={{ color }}>{value}</div>
        <div className="mt-0.5 truncate text-xs text-muted-foreground" title={detail}>{detail}</div>
      </GlassCard>
    </motion.div>
  );
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="glass-inset p-2.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 text-base font-semibold">{value}</div>
    </div>
  );
}

function SummaryCard({ label, value }: { label: string; value: string }) {
  return (
    <GlassCard className="p-3.5" hover={false}>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 text-lg font-semibold">{value}</div>
    </GlassCard>
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
        <div key={row.label} className="glass-inset p-2.5">
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
      ? "所有指标由同一份 d-token 本地请求快照生成，不混用示例或补零数据。"
      : "Every metric is generated from one local d-token request snapshot.";
  }
  if (state === "verified") {
    return lang === "zh"
      ? "本地服务已连接；收到真实请求后，这里才会显示指标与分布。"
      : "The local service is connected; metrics appear after real requests arrive.";
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
    { label: lang === "zh" ? "请求耗时" : "Request latency", available: snapshot.latency !== null },
    { label: lang === "zh" ? "Agent 标识" : "Agent identity", available: snapshot.agentBreakdown.length > 0 },
    { label: lang === "zh" ? "路由标识" : "Route identity", available: snapshot.routeBreakdown.length > 0 },
    { label: lang === "zh" ? "上游标识" : "Provider identity", available: snapshot.upstreamBreakdown.length > 0 },
    { label: lang === "zh" ? "时间趋势" : "Time trend", available: snapshot.trend30d.length > 0 },
    { label: lang === "zh" ? "处理步骤" : "Processing steps", available: snapshot.strategyBreakdown.length > 0 },
  ];
}
