import { useState } from "react";
import { motion, AnimatePresence } from "motion/react";
import { ChevronUp } from "lucide-react";
import { useI18n, useT } from "../../lib/i18n";
import { StatusDot } from "../common/StatusBadge";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";

export function StatusBar() {
  const t = useT();
  const { lang } = useI18n();
  const [open, setOpen] = useState(false);
  const { payload, status, updatedAt, error } = useRuntimeDashboard();
  const summary = buildStatusSummary(payload, status, updatedAt, error, lang);

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
              <span>{t("status.source")}: {summary.sourceLabel}</span>
              <span>{t("status.requests")}: {summary.requestCount}</span>
              <span>{t("status.routes")}: {summary.routeCount}</span>
              <span>{t("status.updated")}: {summary.updatedLabel}</span>
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
  lang: "zh" | "en" = "zh",
) {
  const zh = lang === "zh";
  const isRuntime = status === "runtime";
  const isPreview = status === "browser-preview";
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
  const doctorLabel = !isRuntime
    ? isPreview ? (zh ? "未运行诊断" : "Not run") : status === "loading" ? (zh ? "等待数据" : "Loading") : (zh ? "需检查" : "Needs review")
    : doctorOk
      ? (zh ? "正常" : "Healthy")
      : doctorFailed
        ? (zh ? "需检查" : "Needs review")
        : failed !== null && failed > 0
          ? zh ? `${failed} 个请求错误` : `${failed} request errors`
      : error
        ? (zh ? "需检查" : "Needs review")
        : !activeRoute
          ? (zh ? "等待配置" : "Waiting for configuration")
          : fallbackDisabled
            ? (zh ? "未运行诊断" : "Not run")
            : (zh ? "配置待确认" : "Configuration unverified");
  const doctorHealth = doctorOk ? "healthy" as const : "warning" as const;

  return {
    runtimeLabel: isRuntime
      ? requests !== null && requests > 0 ? (zh ? "运行中" : "Running") : (zh ? "已连接" : "Connected")
      : isPreview ? (zh ? "浏览器预览" : "Browser preview") : status === "loading" ? (zh ? "读取中" : "Loading") : (zh ? "暂不可用" : "Unavailable"),
    runtimeHealth: isRuntime ? "healthy" as const : isPreview || status === "loading" ? "warning" as const : "error" as const,
    routeLabel: activeRoute || (isRuntime ? (zh ? "等待路由配置" : "Waiting for route") : isPreview ? (zh ? "尚未连接真实路由" : "No verified route") : (zh ? "路由不可用" : "Route unavailable")),
    portsLabel: isRuntime ? (serviceUrl ? serviceUrl.replace(/^https?:\/\//, "") : "—") : isPreview ? (zh ? "未监听（预览）" : "Not listening (preview)") : (zh ? "未监听" : "Not listening"),
    modeLabel: isRuntime ? (zh ? "运行时数据" : "Runtime data") : isPreview ? (zh ? "浏览器预览" : "Browser preview") : (zh ? "运行时不可用" : "Runtime unavailable"),
    doctorLabel,
    doctorHealth,
    sourceLabel: source === "browser-preview" ? (zh ? "浏览器预览" : "Browser preview") : source,
    requestCount: isRuntime && requests !== null ? requests : "—",
    routeCount: isRuntime && routes ? routes.length : "—",
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
