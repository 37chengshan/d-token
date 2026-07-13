import { useMemo } from "react";
import { AreaChart, Area, ResponsiveContainer, Tooltip, XAxis } from "recharts";
import {
  Activity,
  Bot,
  CircleAlert,
  Coins,
  Gauge,
  RefreshCw,
  Route,
  Server,
} from "lucide-react";
import { MetricStrip, Surface } from "../common/Surface";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Heatmap, type ActivityDay } from "../common/Heatmap";
import { useI18n, useT, type Lang } from "../../lib/i18n";
import type { PageId } from "../../lib/nav";
import { Badge } from "../ui/badge";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";
import { classifyEvidence, type EvidenceState } from "../../lib/state/evidence";

function fmt(n: number) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

export function Overview(_props: { setPage: (p: PageId) => void }) {
  const t = useT();
  const { lang } = useI18n();
  const { payload, status: payloadStatus, updatedAt } = useRuntimeDashboard();
  const evidenceState = useMemo(
    () => classifyEvidence({ payload, status: payloadStatus, requireRuntimeIdentity: true }),
    [payload, payloadStatus],
  );
  const effectiveStatus = evidenceState === "verified" ? "runtime" : evidenceState === "preview" ? "browser-preview" : evidenceState;

  const runtime = useMemo(() => overviewFromPayload(payload, effectiveStatus, lang), [effectiveStatus, lang, payload]);
  const operational = useMemo(
    () => deriveOverviewOperationalSummary(payload, effectiveStatus, lang),
    [effectiveStatus, lang, payload],
  );
  const s = runtime.stats;
  const metricsAvailable = runtime.metricsAvailable;
  const compressionAvailable = runtime.compressionAvailable;
  const trafficAvailable = runtime.trafficAvailable;
  const unavailableDetail = runtime.sourceLabel;

  return (
    <div className="overview-page space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1>{t("ov.title")}</h1>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <DataSourceBadge
            mode={evidenceState === "verified" ? "runtime" : evidenceState === "preview" || evidenceState === "loading" ? "preview" : "unavailable"}
            label={overviewEvidenceLabel(evidenceState, lang)}
          />
          <Badge variant={evidenceState === "verified" ? "default" : "secondary"}>
            {runtime.sourceLabel}{evidenceState === "verified" && updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}
          </Badge>
        </div>
      </div>

      <MetricStrip data-overview-section="kpis" data-testid="overview-kpis">
        <OverviewKpi
          label={t("ov.routedRequests")}
          value={trafficAvailable ? fmt(s.requestCount) : "—"}
          detail={trafficAvailable ? (lang === "zh" ? "来自 d-token 本地请求记录" : "From local d-token request records") : unavailableDetail}
          icon={Activity}
        />
        <OverviewKpi
          label={lang === "zh" ? "物理减少 Token" : "Physical Token Reduction"}
          value={metricsAvailable ? fmt(s.savedTotal) : "—"}
          detail={metricsAvailable ? (lang === "zh" ? `按压缩前后值计算；可计入节省 ${fmt(s.claimableSaved)}（待上游归因）` : `Derived from before/after values; claimable ${fmt(s.claimableSaved)} (provider attribution pending)`) : unavailableDetail}
          icon={Coins}
        />
        <OverviewKpi
          label={lang === "zh" ? "物理减少比例" : "Physical Reduction Rate"}
          value={compressionAvailable ? `${(s.compressionRate * 100).toFixed(1)}%` : "—"}
          detail={compressionAvailable ? (lang === "zh" ? "仅基于同时具备压缩前后值的请求" : "Only requests with both before and after values are included") : unavailableDetail}
          icon={Gauge}
        />
        <OverviewKpi
          label={lang === "zh" ? "请求错误" : "Request errors"}
          value={trafficAvailable ? fmt(s.errorsToday) : "—"}
          detail={trafficAvailable ? runtime.errorLabel : unavailableDetail}
          icon={CircleAlert}
          tone={trafficAvailable && s.errorsToday > 0 ? "negative" : trafficAvailable ? "positive" : "neutral"}
        />
      </MetricStrip>

      <section data-overview-section="activity">
        <Surface className="p-4" hover={false}>
          <Heatmap activityDays={runtime.activityDays} tools={runtime.tools} source={runtime.activitySource} />
        </Surface>
      </section>

      <section data-overview-section="issues">
        <IssueSummary status={operational.issues} />
      </section>

      <section data-overview-section="trend">
        <Surface className="p-4" hover={false}>
          <div className="mb-3 flex items-center justify-between">
            <h3>{t("ov.trend")}</h3>
            <span className="text-sm text-muted-foreground">{runtime.sourceLabel}</span>
          </div>
          {runtime.trend.length > 0 ? (
            <div className="h-44">
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
          ) : (
            <div className="flex h-28 items-center justify-center rounded-lg border border-dashed border-border/60 bg-muted/20 px-4 text-center text-sm text-muted-foreground">
              {unavailableDetail}
            </div>
          )}
        </Surface>
      </section>

      <section data-overview-section="operational">
        <Surface className="overflow-hidden p-0" strong hover={false}>
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/50 px-4 py-3">
            <div>
              <h2 className="text-base font-semibold">{t("ov.operational")}</h2>
              <p className="mt-0.5 text-xs text-muted-foreground">{t("ov.operationalDesc")}</p>
            </div>
            <div className="flex items-center gap-2">
              <Badge variant={evidenceState === "verified" ? "default" : "outline"}>
                {evidenceState === "verified" ? t("ov.localConnected") : t("ov.waitingConnection")}
              </Badge>
            </div>
          </div>
          <div className="overview-status-grid grid grid-cols-1 gap-px bg-border/40 sm:grid-cols-2 min-[1080px]:grid-cols-4">
            <OperationalItem icon={Server} title={t("ov.localService")} {...operational.service} />
            <OperationalItem icon={Bot} title={t("ov.connectedAgents")} {...operational.agents} />
            <OperationalItem icon={Route} title={t("ov.currentPath")} {...operational.route} />
            <CcSwitchOperationalItem summary={operational.ccSwitch} />
          </div>
        </Surface>
      </section>
    </div>
  );
}

function OverviewKpi({
  label,
  value,
  detail,
  icon: Icon,
  tone = "neutral",
}: {
  label: string;
  value: string;
  detail: string;
  icon: typeof Activity;
  tone?: OperationalTone;
}) {
  const iconTone = tone === "negative"
    ? "text-red-600 dark:text-red-400"
    : tone === "positive"
      ? "text-emerald-600 dark:text-emerald-400"
      : "text-muted-foreground";

  return (
    <div className="overview-kpi min-w-0">
      <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
        <span className="truncate">{label}</span>
        <Icon className={`size-4 shrink-0 ${iconTone}`} />
      </div>
      <div className="mt-2 text-2xl font-semibold tracking-tight">{value}</div>
      <div className="mt-1 truncate text-[11px] text-muted-foreground" title={detail}>{detail}</div>
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
    <div className="overview-status-item flat-status-cell flex min-h-[92px] gap-3 p-4">
      <Icon className={`mt-0.5 size-5 shrink-0 ${color}`} />
      <div className="min-w-0">
        <div className="text-sm text-muted-foreground">{title}</div>
        <div className="mt-1 break-words font-medium">{label}</div>
        <div className="mt-1 text-sm leading-5 text-muted-foreground">{detail}</div>
      </div>
    </div>
  );
}

function CcSwitchOperationalItem({ summary }: { summary: CcSwitchStatus }) {
  const t = useT();
  const color = summary.state === "out_of_sync"
    ? "text-amber-600 dark:text-amber-400"
    : summary.state === "synced"
      ? "text-emerald-600 dark:text-emerald-400"
      : "text-muted-foreground";

  return (
    <div className="overview-status-item flat-status-cell flex min-h-[92px] gap-3 p-4">
      <RefreshCw className={`mt-0.5 size-5 shrink-0 ${color}`} />
      <div className="min-w-0">
        <div className="text-sm text-muted-foreground">{t("ov.upstreamStatus")}</div>
        <div className="mt-1 break-words font-medium">{summary.label}</div>
        <div className="mt-1 text-sm leading-5 text-muted-foreground">{summary.detail}</div>
      </div>
    </div>
  );
}

function IssueSummary({ status }: { status: OperationalStatus }) {
  const t = useT();
  const color = status.tone === "negative"
    ? "text-red-600 dark:text-red-400"
    : status.tone === "positive"
      ? "text-emerald-600 dark:text-emerald-400"
      : status.tone === "warning"
        ? "text-amber-600 dark:text-amber-400"
        : "text-muted-foreground";

  return (
    <div className="flex min-h-[58px] items-center gap-3 border-y border-border/60 px-1 py-3">
      <CircleAlert className={`size-5 shrink-0 ${color}`} />
      <div className="min-w-0 flex-1">
        <div className="text-xs text-muted-foreground">{t("ov.recentIssues")}</div>
        <div className="mt-0.5 truncate text-sm font-medium" title={`${status.label} · ${status.detail}`}>
          {status.label} <span className="font-normal text-muted-foreground">· {status.detail}</span>
        </div>
      </div>
    </div>
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
};

export function deriveOverviewOperationalSummary(
  payload: Record<string, any> | null,
  status: string,
  lang: Lang = "zh",
): OverviewOperationalSummary {
  const zh = lang === "zh";
  const evidenceState = classifyEvidence({ payload, status, requireRuntimeIdentity: true });
  const requestLogs = recordArray(payload?.request_logs);
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
  const isRuntime = evidenceState === "verified";

  const service: OperationalStatus = isRuntime
    ? {
        label: zh ? "正在运行" : "Running",
        detail: safeText(
          payload?.desktop_service_url,
          zh ? "本地服务已响应，未提供监听地址" : "Local service responded without a listening address",
        ),
        tone: "positive",
      }
    : evidenceState === "loading"
      ? {
          label: zh ? "正在检查" : "Checking",
          detail: zh ? "正在连接 d-token 本地服务" : "Connecting to the d-token local service",
          tone: "neutral",
        }
      : evidenceState === "preview"
        ? {
            label: zh ? "浏览器预览" : "Browser preview",
            detail: zh ? "未连接真实本地服务" : "No real local service is connected",
            tone: "neutral",
          }
        : evidenceState === "stale"
          ? {
              label: zh ? "状态已过期" : "Status is stale",
              detail: zh ? "旧数据不作为当前服务状态" : "Old data is not treated as current service status",
              tone: "warning",
            }
          : evidenceState === "partial"
            ? {
                label: zh ? "状态不完整" : "Status is incomplete",
                detail: zh ? "缺少完整的本地服务证据" : "Complete local service evidence is missing",
                tone: "warning",
              }
            : {
            label: zh ? "未连接" : "Not connected",
            detail: zh ? "本地服务未启动或暂时无法访问" : "The local service is stopped or unavailable",
            tone: "negative",
          };

  const agents: OperationalStatus = !isRuntime
    ? {
        label: zh ? "尚无已验证连接" : "No verified connection",
        detail: zh ? "只有实时本地请求才能证明 Agent 已连接" : "Only live local requests can prove an agent connection",
        tone: "neutral",
      }
    : trafficAgents.length > 0
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
        detail: zh ? "缺少实时配置证据时保持未连接状态" : "The connection remains unverified without live configuration evidence",
        tone: isRuntime ? "warning" : "neutral",
      };

  const issues: OperationalStatus = !isRuntime
    ? {
        label: zh ? "尚无当前问题证据" : "No current issue evidence",
        detail: zh ? "连接实时本地服务后才能判断最近问题" : "Connect the live local service to inspect recent issues",
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
      : requestLogs.length === 0
        ? {
            label: zh ? "尚无请求记录" : "No request records yet",
            detail: zh ? "收到真实请求后才能判断请求错误" : "Request errors can be evaluated after real traffic arrives",
            tone: "neutral",
          }
      : {
          label: zh ? "没有发现问题" : "No issues found",
          detail: zh ? "以当前本地服务记录为准" : "Based on current local service records",
          tone: "positive",
        };

  const ccSwitch = isRuntime
    ? deriveCcSwitchStatus(payload, providers, lang)
    : {
        state: "unknown" as const,
        label: zh ? "尚未获得实时验证" : "No live verification yet",
        detail: zh ? "非实时数据不会显示同步成功" : "Non-live data is never shown as successfully synced",
      };
  return { service, agents, route: routeStatus, issues, ccSwitch };
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

function recordArray(value: unknown): Record<string, any>[] {
  return arrayValue(value).filter(
    (item): item is Record<string, any> => Boolean(item) && typeof item === "object" && !Array.isArray(item),
  );
}

function objectValue(value: unknown): Record<string, any> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, any> : {};
}

function numberEvidence(value: unknown): number | null {
  if (typeof value !== "number" && typeof value !== "string") return null;
  if (typeof value === "string" && value.trim().length === 0) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function firstNumberEvidence(...values: unknown[]): number | null {
  for (const value of values) {
    const parsed = numberEvidence(value);
    if (parsed !== null) return parsed;
  }
  return null;
}

function timestampEvidence(value: unknown): number | null {
  const timestamp = numberEvidence(value);
  if (timestamp === null) return null;
  return Number.isFinite(new Date(timestamp).getTime()) ? timestamp : null;
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

function overviewEvidenceLabel(state: EvidenceState, lang: Lang): string {
  const zh = lang === "zh";
  if (state === "verified") return zh ? "实时本地数据" : "Live local data";
  if (state === "preview") return zh ? "浏览器预览" : "Browser preview";
  if (state === "loading") return zh ? "正在检查本地服务" : "Checking local service";
  if (state === "stale") return zh ? "数据已过期" : "Data is stale";
  if (state === "partial") return zh ? "数据不完整" : "Data is incomplete";
  return zh ? "本地数据不可用" : "Local data unavailable";
}

export function overviewFromPayload(payload: Record<string, any> | null, status: string, lang: Lang = "zh") {
  const zh = lang === "zh";
  const evidenceState = classifyEvidence({ payload, status, requireRuntimeIdentity: true });
  const stats = objectValue(payload?.stats);
  const runtimeTokens = objectValue(stats.tokens);
  const requestLogs = recordArray(payload?.request_logs);
  // An owned, verified runtime may legitimately have zero requests. The
  // presence of the typed collection is evidence; its length is the value.
  const trafficAvailable = evidenceState === "verified" && Array.isArray(payload?.request_logs);

  const physicalPairs = requestLogs.flatMap((log) => {
    const before = numberEvidence(log.input_tokens_before);
    const after = numberEvidence(log.input_tokens_after);
    return before === null || after === null ? [] : [{ before, saved: Math.max(0, before - after) }];
  });
  const physicalSaved = physicalPairs.length > 0
    ? physicalPairs.reduce((total, pair) => total + pair.saved, 0)
    : null;
  const physicalBefore = physicalPairs.length > 0
    ? physicalPairs.reduce((total, pair) => total + pair.before, 0)
    : null;
  const claimableValues = requestLogs
    .map((log) => numberEvidence(log.tokens_saved))
    .filter((value): value is number => value !== null);
  const claimableSaved = firstNumberEvidence(runtimeTokens.saved, stats.tokens_saved)
    ?? (claimableValues.length > 0 ? claimableValues.reduce((total, value) => total + value, 0) : 0);
  const savingsPercent = physicalSaved !== null && physicalBefore !== null && physicalBefore > 0
    ? (physicalSaved / physicalBefore) * 100
    : null;
  const metricsAvailable = physicalSaved !== null || trafficAvailable;
  const compressionAvailable = savingsPercent !== null || trafficAvailable;

  const hasRuntimeData = evidenceState === "verified"
    && (trafficAvailable || metricsAvailable || compressionAvailable);

  if (!hasRuntimeData) {
    return {
      source: evidenceState === "verified" ? "runtime-empty" : evidenceState,
      metricsAvailable: false,
      compressionAvailable: false,
      trafficAvailable: false,
      sourceLabel:
        evidenceState === "verified"
            ? (zh ? "本地服务在线 · 暂无请求记录" : "Local service online · no request records")
            : evidenceState === "preview"
              ? (zh ? "浏览器预览 · 未连接实时数据" : "Browser preview · no live data")
              : evidenceState === "stale"
                ? (zh ? "本地数据已过期 · 不作为当前状态" : "Local data is stale · not treated as current")
                : evidenceState === "partial"
                  ? (zh ? "本地数据不完整 · 未确认当前状态" : "Local data is incomplete · current state unverified")
                  : evidenceState === "loading"
                    ? (zh ? "正在连接 d-token 本地服务" : "Connecting to the local d-token service")
                    : (zh ? "d-token 本地服务暂不可用" : "Local d-token service unavailable"),
      stats: emptyOverviewStats,
      trend: [],
      activityDays: [],
      tools: [],
      activitySource: zh ? "等待实时本地请求记录" : "Waiting for live local request records",
      errorLabel: zh ? "尚无当前错误证据" : "No current error evidence",
    };
  }

  const requestCount = firstNumberEvidence(objectValue(stats.requests).total) ?? requestLogs.length;
  const errorCount = firstNumberEvidence(objectValue(stats.requests).failed)
    ?? requestLogs.filter((log) => Boolean(log.error)).length;
  const agents = new Set(requestLogs.map((log) => safeText(log.agent, "")).filter(Boolean));
  const activeAgents = agents.size;
  const runtimeTrend = buildTrendFromLogs(requestLogs);

  return {
    source: trafficAvailable ? "d-token-runtime" : "d-token-runtime-metrics",
    metricsAvailable,
    compressionAvailable,
    trafficAvailable,
    sourceLabel: trafficAvailable
      ? requestCount === 0
        ? (zh ? "本地服务在线 · 暂无请求记录" : "Local service online · no request records")
        : (zh ? `d-token 本地服务 · ${requestCount} 条请求` : `Local d-token service · ${requestCount} request(s)`)
      : (zh ? "本地服务在线 · 暂无请求记录" : "Local service online · no request records"),
    stats: {
      savedTotal: physicalSaved ?? 0,
      claimableSaved,
      compressionRate: savingsPercent === null ? 0 : Math.max(0, Math.min(1, savingsPercent / 100)),
      activeAgents,
      errorsToday: errorCount,
      requestCount,
    },
    trend: runtimeTrend,
    activityDays: buildActivityDaysFromLogs(requestLogs),
    tools: buildToolsFromLogs(requestLogs),
    activitySource: trafficAvailable
      ? (zh ? "d-token 本地请求记录" : "Local d-token request records")
      : (zh ? "等待实时本地请求记录" : "Waiting for live local request records"),
    errorLabel: trafficAvailable
      ? (errorCount > 0
          ? (zh ? "存在请求失败" : "Request failures recorded")
          : (zh ? "没有请求失败" : "No request failures recorded"))
      : (zh ? "尚无实时请求记录" : "No live request records"),
  };
}

const emptyOverviewStats = {
  savedTotal: 0,
  claimableSaved: 0,
  compressionRate: 0,
  activeAgents: 0,
  errorsToday: 0,
  requestCount: 0,
};

function buildActivityDaysFromLogs(logs: any[]): ActivityDay[] {
  if (logs.length === 0) return [];
  const byDay = new Map<string, ActivityDay>();
  for (const log of logs) {
    const timestamp = timestampEvidence(log?.timestamp_ms);
    if (timestamp === null) continue;
    const date = new Date(timestamp).toISOString().slice(0, 10);
    const current = byDay.get(date) ?? {
      date,
      sessions: 0,
      minutes: 0,
      tokens: 0,
      tools: 0,
    };
    current.sessions += 1;
    const latencyMs = numberEvidence(log?.latency_ms);
    if (latencyMs !== null && latencyMs > 0) {
      current.minutes += Math.round((latencyMs / 60_000) * 100) / 100;
    }
    const inputTokens = numberEvidence(log?.input_tokens_before);
    if (inputTokens !== null && inputTokens > 0) current.tokens += inputTokens;
    current.tools += transformNames(log).length;
    byDay.set(date, current);
  }
  return fillRecentActivity(Array.from(byDay.values()).sort((a, b) => a.date.localeCompare(b.date)));
}

function fillRecentActivity(days: ActivityDay[]) {
  if (days.length === 0) return [];
  const byDate = new Map(days.map((day) => [day.date, day]));
  const start = new Date(`${days[0].date}T00:00:00Z`);
  const end = new Date(`${days[days.length - 1].date}T00:00:00Z`);
  const filled: ActivityDay[] = [];
  for (let d = new Date(start); d <= end; d.setUTCDate(d.getUTCDate() + 1)) {
    const date = d.toISOString().slice(0, 10);
    const existing = byDate.get(date);
    filled.push(existing ?? {
      date,
      sessions: 0,
      minutes: 0,
      tokens: 0,
      tools: 0,
    });
  }
  return filled;
}

function buildToolsFromLogs(logs: any[]) {
  const counts = new Map<string, number>();
  for (const log of logs) {
    if (timestampEvidence(log?.timestamp_ms) === null) continue;
    for (const transform of transformNames(log)) {
      counts.set(transform, (counts.get(transform) ?? 0) + 1);
    }
  }
  return Array.from(counts.entries())
    .sort((a, b) => b[1] - a[1])
    .map(([name, calls]) => ({ name, calls }));
}

function buildTrendFromLogs(logs: any[]) {
  if (logs.length === 0) return [];
  const byDay = new Map<string, number>();
  for (const log of logs) {
    const timestamp = timestampEvidence(log?.timestamp_ms);
    const before = numberEvidence(log?.input_tokens_before);
    const after = numberEvidence(log?.input_tokens_after);
    if (timestamp === null || before === null || after === null) continue;
    const saved = Math.max(0, before - after);
    const day = new Date(timestamp).toISOString().slice(5, 10);
    byDay.set(day, (byDay.get(day) ?? 0) + saved);
  }
  return Array.from(byDay.entries())
    .sort(([left], [right]) => left.localeCompare(right))
    .slice(-14)
    .map(([day, saved]) => ({ day, saved }));
}

function transformNames(log: any): string[] {
  return arrayValue(log?.transforms_applied)
    .map((transform) => safeText(transform, "").trim())
    .filter(Boolean);
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}
