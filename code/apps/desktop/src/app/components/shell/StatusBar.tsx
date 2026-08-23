import { useState } from "react";
import { ChevronUp } from "lucide-react";
import { useI18n, useT } from "../../lib/i18n";
import { StatusDot } from "../common/StatusBadge";
import { useRuntimeDashboardSelector } from "../../lib/useRuntimeDashboard";

type StatusBarRuntimeSlice = {
  payload: Record<string, any> | null;
  status: string;
  operational: { service: { data: Record<string, any> | null } };
  updatedAt: Date | null;
  error: string;
  fingerprint: string;
};

function statusBarFingerprint(payload: Record<string, any> | null, service: Record<string, any> | null) {
  const stats = payload?.stats?.requests;
  const activeConfig = payload?.runtime_active_config?.config ?? payload?.active_config?.config ?? payload?.desktop_active_config?.config;
  return JSON.stringify({
    source: payload?.source,
    dataSource: payload?.desktop_data_source,
    serviceUrl: payload?.desktop_service_url,
    requestTotal: stats?.total,
    requestFailed: stats?.failed,
    providers: Array.isArray(payload?.providers) ? payload.providers.map((row: Record<string, any>) => [row.id, row.name]) : [],
    routes: Array.isArray(payload?.routes) ? payload.routes.map((row: Record<string, any>) => [row.id, row.name]) : [],
    activeRoutes: Array.isArray(activeConfig?.routes) ? activeConfig.routes.map((row: Record<string, any>) => [row.id, row.name]) : [],
    fallbackDisabled: activeConfig?.fallback_disabled,
    doctorOk: (payload?.doctor ?? payload?.doctor_summary ?? payload?.product_readiness)?.ok,
    service: service && [service.state, service.manifest_stale, service.pid_running, service.restart_required, service.service_url],
  });
}

function selectStatusBarRuntime(state: import("../../lib/useRuntimeDashboard").RuntimeDashboardState): StatusBarRuntimeSlice {
  const service = state.operational.service.data;
  return {
    payload: state.payload,
    status: state.status,
    operational: { service: { data: service } },
    updatedAt: state.updatedAt,
    error: state.error,
    fingerprint: statusBarFingerprint(state.payload, service),
  };
}

function equalStatusBarRuntime(left: StatusBarRuntimeSlice, right: StatusBarRuntimeSlice) {
  return left.status === right.status
    && left.error === right.error
    && left.updatedAt?.getTime() === right.updatedAt?.getTime()
    && left.fingerprint === right.fingerprint;
}

export function StatusBar() {
  const t = useT();
  const { lang } = useI18n();
  const [open, setOpen] = useState(false);
  const { payload, status, operational, updatedAt, error } = useRuntimeDashboardSelector(
    selectStatusBarRuntime,
    equalStatusBarRuntime,
  );
  const summary = buildStatusSummary(payload, status, updatedAt, error, lang, operational?.service?.data ?? null);

  const Item = ({ label, value, health }: { label: string; value: string; health?: "healthy" | "warning" | "error" }) => (
    <div className="flex shrink-0 items-center gap-1.5 whitespace-nowrap">
      {health && <StatusDot status={health} />}
      <span className="text-muted-foreground">{label}:</span>
      <span>{value}</span>
    </div>
  );

  return (
    <div className="shell-statusbar shrink-0 border-t border-border/40 px-3.5 py-2 text-[12px]">
      <div className="flex min-w-0 items-center gap-x-3">
        <div className="flex min-w-0 flex-1 flex-nowrap items-center gap-x-4 overflow-x-auto">
          <Item label={t("status.runtime")} value={summary.runtimeLabel} health={summary.runtimeHealth} />
          <Item label={t("status.route")} value={summary.routeLabel} />
          <Item label={t("status.ports")} value={summary.portsLabel} />
          <Item label={t("status.mode")} value={summary.modeLabel} />
          <Item label={t("status.doctor")} value={summary.doctorLabel} health={summary.doctorHealth} />
        </div>
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          aria-label={t("status.details")}
          title={t("status.details")}
          className="flex shrink-0 items-center gap-1 text-muted-foreground hover:text-foreground"
        >
          {t("status.details")}
          <span className="shell-statusbar-chevron transition-transform duration-200" style={{ transform: `rotate(${open ? 0 : 180}deg)` }}><ChevronUp className="size-4" /></span>
        </button>
      </div>
      {open && (
          <div className="overflow-hidden animate-in fade-in duration-200">
            <div className="mt-2 grid grid-cols-2 gap-2 border-t border-sidebar-border/70 pt-2 text-muted-foreground md:grid-cols-4">
              <span>{t("status.source")}: {summary.sourceLabel}</span>
              <span>{t("status.requests")}: {summary.requestCount}</span>
              <span>{t("status.routes")}: {summary.routeCount}</span>
              <span>{t("status.updated")}: {summary.updatedLabel}</span>
            </div>
          </div>
      )}
    </div>
  );
}

export function buildStatusSummary(
  payload: Record<string, any> | null,
  status: string,
  updatedAt: Date | null,
  error: string,
  lang: "zh" | "en" = "zh",
  serviceStatus: Record<string, any> | null = null,
) {
  const zh = lang === "zh";
  const isRuntime = status === "runtime";
  const isPreview = status === "browser-preview";
  const isStale = status === "stale";
  const isPartial = status === "partial";
  const staleServiceIdentifiers = serviceStatus?.manifest_stale === true || serviceStatus?.pid_running === false;
  const serviceRestartRequired = !staleServiceIdentifiers
    && (serviceStatus?.state === "restart_required" || serviceStatus?.restart_required === true);
  const serviceStopped = serviceStatus?.state === "stopped";
  const runtimeEvidenceUsable = isRuntime
    && !serviceRestartRequired
    && !serviceStopped
    && serviceStatus?.manifest_stale !== true;
  const serviceRunning = !serviceStatus?.manifest_stale
    && (serviceStatus?.state === "running" || serviceStatus?.state === "unmanaged_running");
  const stats = payload?.stats ?? {};
  const requests = finiteNumber(stats?.requests?.total);
  const failed = finiteNumber(stats?.requests?.failed);
  const providers = Array.isArray(payload?.providers) ? payload.providers : null;
  const routes = Array.isArray(payload?.routes) ? payload.routes : null;
  const activeConfig = payload?.runtime_active_config?.config ?? payload?.active_config?.config ?? payload?.desktop_active_config?.config ?? null;
  const serviceUrl = safeText(payload?.desktop_service_url, "");
  const source = safeText(payload?.desktop_data_source, safeText(payload?.source, isPreview ? "browser-preview" : status));
  const activeRoute =
    safeText(activeConfig?.routes?.[0]?.name, "") ||
    safeText(activeConfig?.routes?.[0]?.id, "") ||
    safeText(routes?.[0]?.name, "") ||
    safeText(routes?.[0]?.id, "");
  const fallbackDisabled = activeRoute.length > 0 && activeConfig?.fallback_disabled === true;
  const doctorEvidence = payload?.doctor ?? payload?.doctor_summary ?? payload?.product_readiness;
  const doctorOk = doctorEvidence?.ok === true;
  const doctorFailed = doctorEvidence?.ok === false;
  const doctorLabel = serviceRestartRequired
    ? (zh ? "需重启服务" : "Restart required")
    : !runtimeEvidenceUsable
    ? isPreview
      ? (zh ? "未运行诊断" : "Not run")
      : status === "loading"
        ? (zh ? "等待数据" : "Loading")
        : (zh ? "需要处理" : "Needs attention")
    : doctorOk
      ? (zh ? "正常" : "Healthy")
      : doctorFailed
      ? (zh ? "需要处理" : "Needs attention")
        : failed !== null && failed > 0
          ? zh ? `${failed} 个请求错误` : `${failed} request errors`
      : error
        ? (zh ? "需要处理" : "Needs attention")
        : !activeRoute
          ? (zh ? "等待配置" : "Waiting for configuration")
          : fallbackDisabled
            ? (zh ? "未运行诊断" : "Not run")
            : (zh ? "配置待确认" : "Configuration unverified");
  const doctorHealth = doctorOk && !serviceRestartRequired ? "healthy" as const : "warning" as const;

  return {
    runtimeLabel: serviceRestartRequired
      ? (zh ? "需要重启" : "Restart required")
      : runtimeEvidenceUsable
      ? requests !== null && requests > 0 ? (zh ? "运行中" : "Running") : (zh ? "已连接" : "Connected")
      : serviceRunning
        ? (zh ? "运行中（数据不完整）" : "Running (partial data)")
      : serviceStopped
        ? (zh ? "已停止" : "Stopped")
      : isPreview
        ? (zh ? "浏览器预览" : "Browser preview")
        : isStale
          ? (zh ? "数据已过期" : "Data stale")
          : isPartial
            ? (zh ? "数据不完整" : "Data incomplete")
            : status === "loading" ? (zh ? "读取中" : "Loading") : (zh ? "暂不可用" : "Unavailable"),
    runtimeHealth: runtimeEvidenceUsable ? "healthy" as const : serviceRestartRequired || serviceRunning || isPreview || status === "loading" ? "warning" as const : "error" as const,
    routeLabel: serviceRestartRequired
      ? (zh ? "重启后重新验证" : "Reverify after restart")
      : runtimeEvidenceUsable
      ? activeRoute || (zh ? "等待路由配置" : "Waiting for route")
      : isPreview
        ? (zh ? "尚未连接真实路由" : "No verified route")
        : isStale
          ? (zh ? "路由数据已过期" : "Route data stale")
          : isPartial
            ? (zh ? "路由数据不完整" : "Route data incomplete")
            : (zh ? "路由不可用" : "Route unavailable"),
    portsLabel: serviceRestartRequired
      ? (!staleServiceIdentifiers && (serviceUrl || serviceStatus?.service_url)
          ? `${String(serviceUrl || serviceStatus?.service_url).replace(/^https?:\/\//, "")} · ${zh ? "需重启" : "restart required"}`
          : (zh ? "需重启后确认" : "Confirm after restart"))
      : runtimeEvidenceUsable || serviceRunning
      ? staleServiceIdentifiers
        ? (zh ? "监听已验证，实例标识不可用" : "Listener verified; instance identifiers unavailable")
        : (serviceUrl || serviceStatus?.service_url ? String(serviceUrl || serviceStatus?.service_url).replace(/^https?:\/\//, "") : "—")
      : serviceStopped
        ? (zh ? "未监听" : "Not listening")
        : isPreview
          ? (zh ? "未监听（预览）" : "Not listening (preview)")
          : status === "loading"
            ? (zh ? "读取中" : "Loading")
            : (zh ? "端口状态不可用" : "Port status unavailable"),
    modeLabel: serviceRestartRequired
      ? (zh ? "旧进程待替换" : "Stale process")
      : runtimeEvidenceUsable
      ? (zh ? "运行时数据" : "Runtime data")
      : isPreview
        ? (zh ? "浏览器预览" : "Browser preview")
        : isStale
          ? (zh ? "过期数据" : "Stale data")
          : isPartial
            ? (zh ? "不完整数据" : "Partial data")
            : (zh ? "运行时不可用" : "Runtime unavailable"),
    doctorLabel,
    doctorHealth,
    sourceLabel: isStale
      ? (zh ? "本地数据已过期" : "Local data stale")
      : isPartial
        ? (zh ? "本地数据不完整" : "Local data incomplete")
        : source === "browser-preview" ? (zh ? "浏览器预览" : "Browser preview") : source,
    requestCount: runtimeEvidenceUsable && requests !== null ? requests : "—",
    routeCount: runtimeEvidenceUsable && routes ? routes.length : "—",
    updatedLabel: updatedAt ? updatedAt.toLocaleTimeString() : "—",
    providerCount: providers?.length ?? 0,
  };
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}

function finiteNumber(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}
