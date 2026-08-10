import { useState } from "react";
import { ChevronUp } from "lucide-react";
import { useI18n, useT, translateCopy } from "../../lib/i18n";
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
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-4">
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
          <span className={`transition-transform duration-200 motion-reduce:rotate-0 motion-reduce:transition-none ${open ? "rotate-0" : "rotate-180"}`}><ChevronUp className="size-4" /></span>
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
    ? (translateCopy("sb.restartRequired", lang))
    : !runtimeEvidenceUsable
    ? isPreview
      ? (translateCopy("sb.notRun", lang))
      : status === "loading"
        ? (translateCopy("sb.loading", lang))
        : (translateCopy("cap.needsAttention", lang))
    : doctorOk
      ? (translateCopy("common.healthy", lang))
      : doctorFailed
      ? (translateCopy("cap.needsAttention", lang))
        : failed !== null && failed > 0
          ? translateCopy("sb.requestErrors", lang, { v1: failed })
      : error
        ? (translateCopy("cap.needsAttention", lang))
        : !activeRoute
          ? (translateCopy("sb.waitingForConfiguration", lang))
          : fallbackDisabled
            ? (translateCopy("sb.notRun", lang))
            : (translateCopy("sb.configurationUnverified", lang));
  const doctorHealth = doctorOk && !serviceRestartRequired ? "healthy" as const : "warning" as const;

  return {
    runtimeLabel: serviceRestartRequired
      ? (translateCopy("onb.service.restartRequired", lang))
      : runtimeEvidenceUsable
      ? requests !== null && requests > 0 ? (translateCopy("status.running", lang)) : (translateCopy("sb.connected", lang))
      : serviceRunning
        ? (translateCopy("sb.runningPartialData", lang))
      : serviceStopped
        ? (translateCopy("cbm.state.stopped", lang))
      : isPreview
        ? (translateCopy("cp.source.preview", lang))
        : isStale
          ? (translateCopy("sb.dataStale", lang))
          : isPartial
            ? (translateCopy("sb.dataIncomplete", lang))
            : status === "loading" ? (translateCopy("adv.preferenceLoading", lang)) : (translateCopy("sb.unavailable", lang)),
    runtimeHealth: runtimeEvidenceUsable ? "healthy" as const : serviceRestartRequired || serviceRunning || isPreview || status === "loading" ? "warning" as const : "error" as const,
    routeLabel: serviceRestartRequired
      ? (translateCopy("sb.reverifyAfterRestart", lang))
      : runtimeEvidenceUsable
      ? activeRoute || (translateCopy("sb.waitingForRoute", lang))
      : isPreview
        ? (translateCopy("sb.noVerifiedRoute", lang))
        : isStale
          ? (translateCopy("sb.routeDataStale", lang))
          : isPartial
            ? (translateCopy("sb.routeDataIncomplete", lang))
            : (translateCopy("sb.routeUnavailable", lang)),
    portsLabel: serviceRestartRequired
      ? (!staleServiceIdentifiers && (serviceUrl || serviceStatus?.service_url)
          ? `${String(serviceUrl || serviceStatus?.service_url).replace(/^https?:\/\//, "")} · ${translateCopy("sb.restartRequired2", lang)}`
          : (translateCopy("sb.confirmAfterRestart", lang)))
      : runtimeEvidenceUsable || serviceRunning
      ? staleServiceIdentifiers
        ? (translateCopy("sb.listenerVerifiedInstanceIdentifiersUnavailable", lang))
        : (serviceUrl || serviceStatus?.service_url ? String(serviceUrl || serviceStatus?.service_url).replace(/^https?:\/\//, "") : "—")
      : serviceStopped
        ? (translateCopy("sb.notListening", lang))
        : isPreview
          ? (translateCopy("sb.notListeningPreview", lang))
          : status === "loading"
            ? (translateCopy("adv.preferenceLoading", lang))
            : (translateCopy("sb.portStatusUnavailable", lang)),
    modeLabel: serviceRestartRequired
      ? (translateCopy("sb.staleProcess", lang))
      : runtimeEvidenceUsable
      ? (translateCopy("sb.runtimeData", lang))
      : isPreview
        ? (translateCopy("cp.source.preview", lang))
        : isStale
          ? (translateCopy("sb.staleData", lang))
          : isPartial
            ? (translateCopy("sb.partialData", lang))
            : (translateCopy("sb.runtimeUnavailable", lang)),
    doctorLabel,
    doctorHealth,
    sourceLabel: isStale
      ? (translateCopy("sb.localDataStale", lang))
      : isPartial
        ? (translateCopy("sb.localDataIncomplete", lang))
        : source === "browser-preview" ? (translateCopy("cp.source.preview", lang)) : source,
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
