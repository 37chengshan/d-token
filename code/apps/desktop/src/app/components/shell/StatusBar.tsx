import { useState } from "react";
import { motion, AnimatePresence } from "motion/react";
import { ChevronUp } from "lucide-react";
import { useT } from "../../lib/i18n";
import { service } from "../../lib/fixtures";
import { StatusDot } from "../common/StatusBadge";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";

export function StatusBar() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const { payload, status, updatedAt, error } = useRuntimeDashboard();
  const summary = buildStatusSummary(payload, status, updatedAt, error);

  const Item = ({ label, value, health }: { label: string; value: string; health?: "healthy" | "warning" | "error" }) => (
    <div className="flex shrink-0 items-center gap-1.5 whitespace-nowrap">
      {health && <StatusDot status={health} />}
      <span className="text-muted-foreground">{label}:</span>
      <span>{value}</span>
    </div>
  );

  return (
    <div className="shell-statusbar glass-light shrink-0 border-t border-border/40 px-3.5 py-2 text-[12px]">
      <div className="glass-backdrop" />
      <div className="flex flex-nowrap items-center gap-x-4 overflow-x-auto">
        <Item label={t("status.runtime")} value={summary.runtimeLabel} health={summary.runtimeHealth} />
        <Item label={t("status.route")} value={summary.routeLabel} />
        <Item label={t("status.ports")} value={summary.portsLabel} />
        <Item label={t("status.mode")} value={summary.modeLabel} />
        <Item label={t("status.doctor")} value={summary.doctorLabel} health={summary.doctorHealth} />
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          aria-label={t("status.details")}
          title={t("status.details")}
          className="ml-auto flex items-center gap-1 text-muted-foreground hover:text-foreground"
        >
          {t("status.details")}
          <motion.span animate={{ rotate: open ? 0 : 180 }}><ChevronUp className="size-4" /></motion.span>
        </button>
      </div>
      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            className="overflow-hidden"
          >
            <div className="mt-2 grid grid-cols-2 gap-2 border-t border-[var(--glass-inset-border)] pt-2 text-muted-foreground md:grid-cols-4">
              <span>source: {summary.sourceLabel}</span>
              <span>requests: {summary.requestCount}</span>
              <span>routes: {summary.routeCount}</span>
              <span>updated: {summary.updatedLabel}</span>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export function buildStatusSummary(
  payload: Record<string, any> | null,
  status: string,
  updatedAt: Date | null,
  error: string,
) {
  const isRuntime = status === "runtime";
  const isPreview = status === "browser-preview";
  const stats = payload?.stats ?? {};
  const requests = Number(stats?.requests?.total ?? 0);
  const failed = Number(stats?.requests?.failed ?? 0);
  const providers = Array.isArray(payload?.providers) ? payload.providers : [];
  const routes = Array.isArray(payload?.routes) ? payload.routes : [];
  const activeConfig = payload?.runtime_active_config?.config ?? payload?.active_config?.config ?? payload?.desktop_active_config?.config ?? null;
  const serviceUrl = safeText(payload?.desktop_service_url, `http://127.0.0.1:${service.port}`);
  const source = safeText(payload?.desktop_data_source, safeText(payload?.source, isPreview ? "browser-preview" : status));
  const activeRoute =
    safeText(activeConfig?.routes?.[0]?.name, "") ||
    safeText(activeConfig?.routes?.[0]?.id, "") ||
    safeText(routes?.[0]?.name, "") ||
    safeText(routes?.[0]?.id, "");
  const fallbackDisabled =
    activeRoute.length > 0 && (
      activeConfig?.fallback_disabled === true ||
      routes.every((route: any) => !route?.fallback_provider_id && !route?.fallbackProviderId)
    );
  const doctorLabel = !isRuntime
    ? isPreview ? "未运行诊断" : status === "loading" ? "等待数据" : "需检查"
    : failed > 0
      ? `${failed} 个错误`
      : error
        ? "需检查"
        : !activeRoute
          ? "等待配置"
          : fallbackDisabled
            ? "正常"
            : "待确认";
  const doctorHealth = !isRuntime || failed > 0 || Boolean(error) || !activeRoute
    ? "warning" as const
    : "healthy" as const;

  return {
    runtimeLabel: isRuntime ? (requests > 0 ? "运行中" : "已连接") : isPreview ? "浏览器预览" : status === "loading" ? "读取中" : "暂不可用",
    runtimeHealth: isRuntime ? "healthy" as const : isPreview || status === "loading" ? "warning" as const : "error" as const,
    routeLabel: activeRoute || (isRuntime ? "等待路由配置" : isPreview ? "尚未连接真实路由" : "路由不可用"),
    portsLabel: isRuntime ? serviceUrl.replace("http://", "") : isPreview ? "未监听（预览）" : "未监听",
    modeLabel: isRuntime ? "运行时数据" : isPreview ? "浏览器预览" : "运行时不可用",
    doctorLabel,
    doctorHealth,
    sourceLabel: source,
    requestCount: requests,
    routeCount: routes.length,
    updatedLabel: updatedAt ? updatedAt.toLocaleTimeString() : "—",
    providerCount: providers.length,
  };
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}
