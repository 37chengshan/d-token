import { useMemo } from "react";
import { AreaChart, Area, ResponsiveContainer, Tooltip, XAxis } from "recharts";
import {
  Activity,
  ArrowRight,
  Bot,
  CircleAlert,
  Coins,
  Gauge,
  RefreshCw,
  Route,
  Server,
  TrendingUp,
} from "lucide-react";
import { StatCard } from "../common/StatCard";
import { GlassCard } from "../common/GlassCard";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Heatmap } from "../common/Heatmap";
import { useI18n, useT, type Lang } from "../../lib/i18n";
import { activityCalendar, overviewStats, toolUsage, trend14, type ActivityDay } from "../../lib/fixtures";
import type { PageId } from "../../lib/nav";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";

function fmt(n: number) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

export function Overview({ setPage }: { setPage: (p: PageId) => void }) {
  const t = useT();
  const { lang } = useI18n();
  const { payload, status: payloadStatus, updatedAt } = useRuntimeDashboard();

  const runtime = useMemo(() => overviewFromPayload(payload, payloadStatus), [payload, payloadStatus]);
  const operational = useMemo(
    () => deriveOverviewOperationalSummary(payload, payloadStatus, lang),
    [lang, payload, payloadStatus],
  );
  const s = runtime.stats;

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center gap-3">
        <h1>{t("ov.title")}</h1>
        <DataSourceBadge mode={payloadStatus === "runtime" ? "runtime" : payloadStatus === "unavailable" ? "unavailable" : "preview"} />
        <Badge variant={payloadStatus === "runtime" ? "default" : "secondary"}>
          {runtime.sourceLabel}{updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}
        </Badge>
      </div>

      <section data-overview-section="operational" className="space-y-4">
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_19rem]">
          <GlassCard className="overflow-hidden p-0" strong hover={false}>
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/50 px-5 py-4">
              <div>
                <h2 className="text-base font-semibold">{t("ov.operational")}</h2>
                <p className="mt-1 text-sm text-muted-foreground">{t("ov.operationalDesc")}</p>
              </div>
              <Badge variant={payloadStatus === "runtime" ? "default" : "outline"}>
                {payloadStatus === "runtime" ? t("ov.localConnected") : t("ov.waitingConnection")}
              </Badge>
            </div>
            <div className="grid sm:grid-cols-2">
              <OperationalItem icon={Server} title={t("ov.localService")} {...operational.service} />
              <OperationalItem icon={Bot} title={t("ov.connectedAgents")} {...operational.agents} />
              <OperationalItem icon={Route} title={t("ov.currentPath")} {...operational.route} />
              <OperationalItem icon={CircleAlert} title={t("ov.recentIssues")} {...operational.issues} />
            </div>
          </GlassCard>

          <GlassCard className="flex min-h-52 flex-col justify-between border-primary/25 p-5" strong hover={false}>
            <div>
              <div className="mb-3 flex size-9 items-center justify-center rounded-md bg-primary/10 text-primary">
                <ArrowRight className="size-5" />
              </div>
              <div className="text-sm text-muted-foreground">{t("ov.nextStep")}</div>
              <h2 className="mt-1 text-lg font-semibold">{operational.nextStep.label}</h2>
              <p className="mt-2 text-sm leading-6 text-muted-foreground">{operational.nextStep.detail}</p>
            </div>
            <Button className="mt-5 w-full" onClick={() => setPage(operational.nextStep.page)}>
              {operational.nextStep.action}
              <ArrowRight className="size-4" />
            </Button>
          </GlassCard>
        </div>

        <CcSwitchSummary summary={operational.ccSwitch} />
      </section>

      <section data-overview-section="savings" className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard index={0} label={t("ov.savedToday")} value={fmt(s.savedToday)} sub="tokens" icon={<Coins className="size-4" />} accentBar="var(--chart-1)" />
        <StatCard index={1} label={t("ov.savedWeek")} value={fmt(s.savedWeek)} sub="tokens" icon={<TrendingUp className="size-4" />} accentBar="var(--chart-2)" />
        <StatCard index={2} label={t("ov.compressionRate")} value={`${Math.round(s.compressionRate * 100)}%`} icon={<Gauge className="size-4" />} accentBar="var(--chart-3)" />
        <StatCard index={3} label={t("ov.routedRequests")} value={fmt(s.requestCount)} icon={<Activity className="size-4" />} accentBar="var(--chart-4)" />
      </section>

      <section data-overview-section="trend">
        <GlassCard className="p-5" hover={false}>
          <div className="mb-3 flex items-center justify-between">
            <h3>{t("ov.trend")}</h3>
            <span className="text-sm text-muted-foreground">{runtime.sourceLabel}</span>
          </div>
          <div className="h-52">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={runtime.trend}>
                <defs>
                  <linearGradient id="ovGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="var(--chart-accent)" stopOpacity={0.5} />
                    <stop offset="95%" stopColor="var(--chart-accent)" stopOpacity={0} />
                  </linearGradient>
                </defs>
                <XAxis dataKey="day" tick={{ fontSize: 11, fill: "var(--muted-foreground)" }} axisLine={false} tickLine={false} />
                <Tooltip contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }} />
                <Area type="monotone" dataKey="saved" stroke="var(--chart-accent)" strokeWidth={2} fill="url(#ovGrad)" />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        </GlassCard>
      </section>

      <section data-overview-section="activity">
        <GlassCard className="p-5" hover={false}>
          <Heatmap activityDays={runtime.activityDays} tools={runtime.tools} source={runtime.activitySource} />
        </GlassCard>
      </section>
    </div>
  );
}

function OperationalItem({
  icon: Icon,
  title,
  label,
  detail,
  tone,
}: {
  icon: typeof Server;
  title: string;
  label: string;
  detail: string;
  tone: OperationalTone;
}) {
  const color = tone === "positive"
    ? "text-emerald-600 dark:text-emerald-400"
    : tone === "warning"
      ? "text-amber-600 dark:text-amber-400"
      : tone === "negative"
        ? "text-red-600 dark:text-red-400"
        : "text-muted-foreground";
  return (
    <div className="flex min-h-32 gap-3 border-b border-border/40 p-5 last:border-b-0 sm:[&:nth-child(odd)]:border-r sm:[&:nth-last-child(-n+2)]:border-b-0">
      <Icon className={`mt-0.5 size-5 shrink-0 ${color}`} />
      <div className="min-w-0">
        <div className="text-sm text-muted-foreground">{title}</div>
        <div className="mt-1 break-words font-medium">{label}</div>
        <div className="mt-1 text-sm leading-5 text-muted-foreground">{detail}</div>
      </div>
    </div>
  );
}

function CcSwitchSummary({ summary }: { summary: CcSwitchStatus }) {
  const t = useT();
  const tone = summary.state === "out_of_sync"
    ? "border-amber-500/30 bg-amber-500/5 text-amber-700 dark:text-amber-300"
    : summary.state === "synced"
      ? "border-emerald-500/30 bg-emerald-500/5 text-emerald-700 dark:text-emerald-300"
      : "border-border/40 bg-muted/30 text-muted-foreground";
  return (
    <GlassCard className={`flex items-center gap-3 p-4 ${tone}`} hover={false}>
      <div className="flex items-center gap-3">
        <RefreshCw className={`size-5 ${summary.state === "out_of_sync" ? "text-amber-500" : summary.state === "synced" ? "text-emerald-500" : "text-muted-foreground"}`} />
        <div className="text-sm">
          <div className="font-medium">{t("ov.upstreamStatus")}</div>
          <div className="text-muted-foreground">{summary.label} · {summary.detail}</div>
        </div>
      </div>
    </GlassCard>
  );
}

type OperationalTone = "positive" | "warning" | "negative" | "neutral";

type OperationalStatus = {
  label: string;
  detail: string;
  tone: OperationalTone;
};

type CcSwitchStatus = {
  state: "unknown" | "unverified" | "synced" | "out_of_sync";
  label: string;
  detail: string;
};

export type OverviewOperationalSummary = {
  service: OperationalStatus;
  agents: OperationalStatus;
  route: OperationalStatus;
  issues: OperationalStatus;
  ccSwitch: CcSwitchStatus;
  nextStep: {
    label: string;
    detail: string;
    action: string;
    page: PageId;
  };
};

export function deriveOverviewOperationalSummary(
  payload: Record<string, any> | null,
  status: string,
  lang: Lang = "zh",
): OverviewOperationalSummary {
  const zh = lang === "zh";
  const requestLogs = arrayValue(payload?.request_logs);
  const runtimeConfig = objectValue(payload?.runtime_active_config);
  const desktopConfig = objectValue(payload?.desktop_active_config);
  const routes = firstPopulatedArray(runtimeConfig.routes, payload?.routes);
  const providers = firstPopulatedArray(runtimeConfig.providers, payload?.providers);
  const instances = firstPopulatedArray(runtimeConfig.instances, payload?.instances);
  const route = routes[0] ?? objectValue(desktopConfig.route);
  const providerId = safeText(route?.target_provider_id, "");
  const provider = providers.find((item) => safeText(item?.id, "") === providerId)
    ?? providers[0]
    ?? objectValue(desktopConfig.provider);
  const routeName = safeText(route?.name, safeText(route?.id, ""));
  const providerName = safeText(provider?.name, safeText(provider?.id, ""));
  const routeConfigured = Boolean(routeName || providerName);

  const trafficAgents = uniqueText(requestLogs.map((item) => item?.agent));
  const configuredAgents = uniqueText([
    ...instances.map((item) => item?.agent),
    objectValue(desktopConfig.instance).agent,
  ]);
  const errorCount = requestLogs.filter((item) => Boolean(item?.error)).length;
  const configState = safeText(desktopConfig.state, "");
  const configProblem = configState === "invalid_json" || configState === "unreadable";
  const issueCount = errorCount + (status === "runtime" && !routeConfigured ? 1 : 0) + (configProblem ? 1 : 0);
  const isRuntime = status === "runtime";

  const service: OperationalStatus = isRuntime
    ? {
        label: zh ? "正在运行" : "Running",
        detail: safeText(payload?.desktop_service_url, "http://127.0.0.1:18001"),
        tone: "positive",
      }
    : status === "loading"
      ? {
          label: zh ? "正在检查" : "Checking",
          detail: zh ? "正在连接 d-token 本地服务" : "Connecting to the d-token local service",
          tone: "neutral",
        }
      : status === "browser-preview"
        ? {
            label: zh ? "浏览器预览" : "Browser preview",
            detail: zh ? "未连接真实本地服务" : "No real local service is connected",
            tone: "neutral",
          }
        : {
            label: zh ? "未连接" : "Not connected",
            detail: zh ? "本地服务未启动或暂时无法访问" : "The local service is stopped or unavailable",
            tone: "negative",
          };

  const agents: OperationalStatus = trafficAgents.length > 0
    ? {
        label: trafficAgents.join("、"),
        detail: zh
          ? `${requestLogs.length} 条请求提供了连接证据`
          : `${requestLogs.length} request(s) provide connection evidence`,
        tone: "positive",
      }
    : configuredAgents.length > 0
      ? {
          label: zh ? "尚无已验证连接" : "No verified connection yet",
          detail: zh
            ? `已配置：${configuredAgents.join("、")}`
            : `Configured: ${configuredAgents.join(", ")}`,
          tone: "warning",
        }
      : {
          label: zh ? "尚未连接" : "Not connected",
          detail: zh ? "完成配置并发送一次真实请求后显示" : "Configure an agent and send one real request",
          tone: isRuntime ? "warning" : "neutral",
        };

  const routeStatus: OperationalStatus = isRuntime && routeConfigured
    ? {
        label: [routeName, providerName].filter(Boolean).join(" → "),
        detail: requestLogs.length > 0
          ? (zh ? "最近请求已经过 d-token" : "Recent requests passed through d-token")
          : (zh ? "配置已加载，尚无请求证据" : "Configuration loaded; no request evidence yet"),
        tone: requestLogs.length > 0 ? "positive" : "warning",
      }
    : {
        label: isRuntime
          ? (zh ? "等待连接配置" : "Waiting for connection setup")
          : (zh ? "尚未连接真实路径" : "No real connection path"),
        detail: zh ? "不会用示例连接冒充真实状态" : "Preview paths are never presented as real",
        tone: isRuntime ? "warning" : "neutral",
      };

  const issues: OperationalStatus = !isRuntime
    ? {
        label: zh ? "服务未启动" : "Service is not running",
        detail: zh ? "启动后才能检查请求与连接问题" : "Start the service to inspect request and connection issues",
        tone: "warning",
      }
    : issueCount > 0
      ? {
          label: zh ? `${issueCount} 个问题` : `${issueCount} issue(s)`,
          detail: errorCount > 0
            ? (zh ? `${errorCount} 条请求失败，需要查看原因` : `${errorCount} request(s) failed`)
            : (zh ? "连接配置尚未完整" : "Connection setup is incomplete"),
          tone: "negative",
        }
      : {
          label: zh ? "没有发现问题" : "No issues found",
          detail: zh ? "以当前本地服务记录为准" : "Based on current local service records",
          tone: "positive",
        };

  const ccSwitch = deriveCcSwitchStatus(payload, providers, lang);
  const nextStep = !isRuntime
    ? {
        label: zh ? "启动并连接 d-token" : "Start and connect d-token",
        detail: zh ? "先启动本地服务，再检测和连接你的 AI 编码工具。" : "Start the local service, then detect and connect your AI coding tools.",
        action: zh ? "前往安装与连接" : "Open Install & Connect",
        page: "install" as PageId,
      }
    : !routeConfigured || trafficAgents.length === 0
      ? {
          label: zh ? "连接 Agent 并验证" : "Connect and verify an agent",
          detail: zh ? "完成连接后发送一次真实请求，确认流量经过 d-token。" : "Send one real request after setup to verify traffic passes through d-token.",
          action: zh ? "前往安装与连接" : "Open Install & Connect",
          page: "install" as PageId,
        }
      : issueCount > 0
        ? {
            label: zh ? "检查并修复问题" : "Inspect and fix issues",
            detail: zh ? "Doctor 会解释失败原因，并只对可安全处理的问题提供修复。" : "Doctor explains failures and only offers safe fixes.",
            action: zh ? "打开 Doctor" : "Open Doctor",
            page: "doctor" as PageId,
          }
        : {
            label: zh ? "查看节省详情" : "Review savings details",
            detail: zh ? "连接已经工作，可以查看请求、节省与耗时趋势。" : "The connection works; review request, savings, and latency trends.",
            action: zh ? "查看分析" : "View analytics",
            page: "analytics" as PageId,
          };

  return { service, agents, route: routeStatus, issues, ccSwitch, nextStep };
}

function deriveCcSwitchStatus(
  payload: Record<string, any> | null,
  providers: any[],
  lang: Lang,
): CcSwitchStatus {
  const zh = lang === "zh";
  const evidence = objectValue(payload?.cc_switch_sync ?? payload?.ccSwitchSync);
  const state = safeText(evidence.status ?? evidence.state, "").toLowerCase();
  const verified = evidence.verified === true || evidence.evidence_verified === true;
  const detected = evidence.detected === true || providers.some((provider) => {
    const kind = safeText(provider?.kind, "").toLowerCase();
    return kind === "cc_switch" || kind === "cc-switch" || kind === "ccswitch";
  });
  const evidenceDetail = uniqueText([
    evidence.profile,
    evidence.upstream,
    evidence.connection_path,
  ]).join(" · ");

  if (verified && (state === "synced" || state === "in_sync")) {
    return {
      state: "synced",
      label: zh ? "已验证同步" : "Sync verified",
      detail: evidenceDetail || (zh ? "同步证据来自本地检测结果" : "Backed by local detection evidence"),
    };
  }
  if (verified && (state === "out_of_sync" || state === "mismatch")) {
    return {
      state: "out_of_sync",
      label: zh ? "连接不一致" : "Connection mismatch",
      detail: evidenceDetail || (zh ? "需要在安装与连接页确认当前上游" : "Confirm the current upstream in Install & Connect"),
    };
  }
  if (detected || Object.keys(evidence).length > 0) {
    return {
      state: "unverified",
      label: zh ? "检测到配置，尚未验证同步" : "Configuration detected; sync not verified",
      detail: zh ? "没有同步时间或校验结果，不显示为正常" : "No sync time or verification result; not shown as healthy",
    };
  }
  return {
    state: "unknown",
    label: zh ? "尚未检测" : "Not detected yet",
    detail: zh ? "不会自动猜测或改写第三方配置" : "Third-party settings are not guessed or rewritten",
  };
}

function arrayValue(value: unknown): any[] {
  return Array.isArray(value) ? value : [];
}

function objectValue(value: unknown): Record<string, any> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, any> : {};
}

function firstPopulatedArray(...values: unknown[]): any[] {
  for (const value of values) {
    if (Array.isArray(value) && value.length > 0) return value;
  }
  return [];
}

function uniqueText(values: unknown[]): string[] {
  return Array.from(new Set(values.map((value) => safeText(value, "")).filter(Boolean)));
}

function overviewFromPayload(payload: Record<string, any> | null, status: string) {
  const stats = payload?.stats ?? {};
  const requestLogs = Array.isArray(payload?.request_logs) ? payload.request_logs : [];
  const history = Array.isArray(payload?.history?.history) ? payload.history.history : [];
  const hasRuntimeData = status === "runtime" && (requestLogs.length > 0 || typeof stats.tokens_saved === "number" || history.length > 0);

  if (!hasRuntimeData) {
    const isPreview = status === "browser-preview";
    return {
      source: isPreview ? "browser-preview" : status === "runtime" ? "runtime-empty" : status,
      sourceLabel:
        isPreview
          ? "浏览器预览 · 示例数据"
          : status === "runtime"
            ? "本地服务在线 · 暂无请求"
            : "d-token 本地服务暂不可用",
      stats: isPreview
        ? {
            ...overviewStats,
            requestCount: activityCalendar.reduce((total, day) => total + day.sessions, 0),
          }
        : emptyOverviewStats,
      trend: isPreview ? trend14 : [],
      activityDays: isPreview ? activityCalendar : [],
      tools: isPreview ? toolUsage : [],
      activitySource: isPreview ? "浏览器预览" : "等待本地请求记录",
      errorLabel: isPreview ? "示例错误" : "等待本地请求记录",
    };
  }

  const runtimeTokens = stats.tokens ?? {};
  const totalSaved = Number(runtimeTokens.saved ?? stats.tokens_saved ?? sum(requestLogs, "tokens_saved"));
  const totalBefore = Number(runtimeTokens.input ?? sum(requestLogs, "input_tokens_before"));
  const savingsPercent = Number(stats.savings_percent ?? (totalBefore > 0 ? (totalSaved / totalBefore) * 100 : 0));
  const errorCount = requestLogs.filter((log: any) => log?.error).length;
  const agents = new Set(requestLogs.map((log: any) => safeText(log?.agent, "")).filter(Boolean));
  const activeAgents = agents.size > 0 ? agents.size : 1;
  const runtimeTrend = history.length > 0
    ? [...history]
      .sort((a: any, b: any) => Number(a?.bucket_ms ?? a?.ts ?? 0) - Number(b?.bucket_ms ?? b?.ts ?? 0))
      .slice(-14)
      .map((item: any, index: number) => ({
        day: formatTrendTime(item?.bucket_ms ?? item?.ts, `#${index + 1}`),
        saved: Number(item.saved ?? 0),
      }))
    : buildTrendFromLogs(requestLogs);

  return {
    source: "d-token-runtime",
    sourceLabel: `d-token 本地服务 · ${requestLogs.length} 条请求`,
    stats: {
      savedToday: savedFromHistoryOrLogs(history, requestLogs, startOfLocalDay(Date.now()), Date.now()),
      savedWeek: savedFromHistoryOrLogs(history, requestLogs, Date.now() - 7 * 24 * 60 * 60 * 1000, Date.now()),
      costSaved: totalSaved / 1_000_000 * 3,
      compressionRate: Math.max(0, Math.min(1, savingsPercent / 100)),
      activeAgents,
      errorsToday: errorCount,
      requestCount: requestLogs.length,
      layered: {
        rtk: 0,
        runtime: Math.max(0, Math.min(1, savingsPercent / 100)),
        route: 0,
      },
    },
    trend: runtimeTrend,
    activityDays: buildActivityDaysFromLogs(requestLogs),
    tools: buildToolsFromLogs(requestLogs),
    activitySource: "d-token 本地请求记录",
    errorLabel: errorCount > 0 ? "存在请求失败" : "没有请求失败",
  };
}

const emptyOverviewStats = {
  savedToday: 0,
  savedWeek: 0,
  costSaved: 0,
  compressionRate: 0,
  activeAgents: 0,
  errorsToday: 0,
  requestCount: 0,
  layered: {
    rtk: 0,
    runtime: 0,
    route: 0,
  },
};

function startOfLocalDay(now: number) {
  const date = new Date(now);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

function savedInWindow(logs: any[], from: number, to: number) {
  return logs.reduce((total, log) => {
    const timestamp = Number(log?.timestamp_ms);
    if (!Number.isFinite(timestamp) || timestamp < from || timestamp > to) return total;
    return total + Number(log?.tokens_saved ?? 0);
  }, 0);
}

function savedFromHistoryOrLogs(history: any[], logs: any[], from: number, to: number) {
  const historyRows = history.filter((item) => Number.isFinite(Number(item?.bucket_ms ?? item?.ts)));
  if (historyRows.length === 0) return savedInWindow(logs, from, to);
  return historyRows.reduce((total, item) => {
    const timestamp = Number(item?.bucket_ms ?? item?.ts);
    if (timestamp < from || timestamp > to) return total;
    return total + Number(item?.saved ?? 0);
  }, 0);
}

function formatTrendTime(value: unknown, fallback: string) {
  const timestamp = Number(value);
  if (!Number.isFinite(timestamp)) return safeText(value, fallback);
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    hour12: false,
  }).format(new Date(timestamp));
}

function buildActivityDaysFromLogs(logs: any[]): ActivityDay[] {
  if (logs.length === 0) return [];
  const byDay = new Map<string, ActivityDay>();
  for (const log of logs) {
    const ms = Number(log?.timestamp_ms ?? Date.now());
    const date = Number.isFinite(ms) ? new Date(ms).toISOString().slice(0, 10) : new Date().toISOString().slice(0, 10);
    const current = byDay.get(date) ?? {
      date,
      week: 0,
      weekday: new Date(`${date}T00:00:00`).getDay(),
      month: date.slice(5, 7),
      level: 0,
      sessions: 0,
      minutes: 0,
      tokens: 0,
      tools: 0,
      changes: 0,
    };
    current.sessions += 1;
    current.minutes += Math.max(1, Math.round(Number(log?.latency_ms ?? 0) / 1000));
    current.tokens += Number(log?.input_tokens_before ?? 0);
    current.tools += Array.isArray(log?.transforms_applied) ? Math.max(1, log.transforms_applied.length) : 1;
    current.changes += Number(log?.tokens_saved ?? 0) > 0 ? 1 : 0;
    current.level = activityLevelFromSessions(current.sessions);
    byDay.set(date, current);
  }
  return fillRecentActivity(Array.from(byDay.values()).sort((a, b) => a.date.localeCompare(b.date)));
}

function fillRecentActivity(days: ActivityDay[]) {
  if (days.length === 0) return [];
  const byDate = new Map(days.map((day) => [day.date, day]));
  const end = new Date(`${days[days.length - 1].date}T00:00:00`);
  const start = new Date(end);
  start.setDate(end.getDate() - 30);
  const filled: ActivityDay[] = [];
  for (let d = new Date(start); d <= end; d.setDate(d.getDate() + 1)) {
    const date = d.toISOString().slice(0, 10);
    const existing = byDate.get(date);
    filled.push(existing ?? {
      date,
      week: Math.floor(filled.length / 7),
      weekday: d.getDay(),
      month: date.slice(5, 7),
      level: 0,
      sessions: 0,
      minutes: 0,
      tokens: 0,
      tools: 0,
      changes: 0,
    });
  }
  return filled;
}

function buildToolsFromLogs(logs: any[]) {
  const counts = new Map<string, number>();
  for (const log of logs) {
    const transforms = Array.isArray(log?.transforms_applied) && log.transforms_applied.length > 0 ? log.transforms_applied : ["runtime_route"];
    for (const transform of transforms) {
      counts.set(transform, (counts.get(transform) ?? 0) + 1);
    }
  }
  return Array.from(counts.entries())
    .sort((a, b) => b[1] - a[1])
    .map(([name, calls]) => ({ name, calls }));
}

function activityLevelFromSessions(sessions: number): 0 | 1 | 2 | 3 | 4 {
  if (sessions <= 0) return 0;
  if (sessions >= 5) return 4;
  if (sessions >= 3) return 3;
  if (sessions >= 2) return 2;
  return 1;
}

function buildTrendFromLogs(logs: any[]) {
  if (logs.length === 0) return [];
  const byDay = new Map<string, number>();
  for (const log of logs) {
    const ms = Number(log?.timestamp_ms ?? Date.now());
    const day = Number.isFinite(ms) ? new Date(ms).toISOString().slice(5, 10) : "runtime";
    byDay.set(day, (byDay.get(day) ?? 0) + Number(log?.tokens_saved ?? 0));
  }
  return Array.from(byDay.entries()).slice(-14).map(([day, saved]) => ({ day, saved }));
}

function sum(rows: any[], key: string) {
  return rows.reduce((total, row) => total + Number(row?.[key] ?? 0), 0);
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}
