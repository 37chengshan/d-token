import { useEffect, useMemo, useState } from "react";
import { motion } from "motion/react";
import { Download, KeyRound, RefreshCcw, ShieldCheck, Filter, Route, Gauge, Server, AlertTriangle, Layers, ListChecks } from "lucide-react";
import { Surface } from "../common/Surface";
import { StatusBadge } from "../common/StatusBadge";
import { Terminal } from "../common/Terminal";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { useI18n, useT } from "../../lib/i18n";
import type { RuntimeLogEntry as LogEntry } from "../../lib/runtime/types";
import { invokeNative, isTauriRuntime } from "../../lib/native";
import { useSessionTab } from "../../lib/sessionTab";
import { useRuntimeDashboardDetail } from "../../lib/useRuntimeDashboard";
import { useNotifications } from "../../lib/notifications";
import { AnimatedSegmentedControl } from "../shell/AnimatedSegmentedControl";
import { presentOperationFailure } from "../../lib/userCopy";
import { protocolLabel } from "../../lib/protocol";
const LOGS_TABS = ["logs", "ledger", "secrets"] as const;
const LOG_FILTERS = ["all", "route", "compression", "provider", "error"] as const;
const LOG_PAGE_SIZE = 80;

const filterMeta: Record<
  LogEntry["type"] | "all",
  { zh: string; en: string; icon: typeof Layers }
> = {
  all: { zh: "全部", en: "All", icon: Layers },
  route: { zh: "route", en: "route", icon: Route },
  compression: { zh: "compression", en: "compression", icon: Gauge },
  provider: { zh: "provider", en: "provider", icon: Server },
  error: { zh: "error", en: "error", icon: AlertTriangle },
};

export function LogsSecurity({ embedded = false }: { embedded?: boolean } = {}) {
  const t = useT();
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const nativeRuntime = isTauriRuntime();
  const [tab, setTab] = useSessionTab("d-token.logs.tab", LOGS_TABS, "logs");
  const [filter, setFilter] = useState<LogEntry["type"] | "all">("all");
  const [visibleLogCount, setVisibleLogCount] = useState(LOG_PAGE_SIZE);
  const [reducedMotion, setReducedMotion] = useState(
    () =>
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const { payload, status: payloadStatus, updatedAt } = useRuntimeDashboardDetail();
  const [exportState, setExportState] = useState<{ state: "idle" | "exporting" | "done" | "failed"; format?: "json" | "csv" }>({ state: "idle" });
  const [securityState, setSecurityState] = useState<"idle" | "checking" | "done" | "preview" | "failed">("idle");
  const [securityVerification, setSecurityVerification] = useState<AgentRouteVerification | null>(null);

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const updatePreference = (event: MediaQueryListEvent | MediaQueryList) => setReducedMotion(event.matches);
    updatePreference(preference);
    preference.addEventListener("change", updatePreference);
    return () => preference.removeEventListener("change", updatePreference);
  }, []);

  const runtimeLogs = useMemo(() => runtimePayloadToLogs(payload), [payload]);
  const ledgerRows = useMemo(() => runtimePayloadToLedgerRows(payload, payloadStatus, lang), [payload, payloadStatus, lang]);
  const ledgerSummary = useMemo(() => summarizeLedgerRows(ledgerRows), [ledgerRows]);
  const activeLogs = runtimeLogs;
  const canExport = nativeRuntime && payloadStatus === "runtime";
  const dataSourceLabel =
    payloadStatus === "runtime"
      ? `${payload?.desktop_data_source ?? payload?.source ?? "d-token-owned-runtime"} · ${activeLogs.length} logs${updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}`
      : payloadStatus === "browser-preview"
        ? lang === "zh" ? "浏览器预览 · 无本机日志" : "Browser preview · no local logs"
        : payloadStatus === "loading"
          ? lang === "zh" ? "正在读取 d-token runtime" : "Loading d-token runtime"
          : lang === "zh" ? "runtime 暂不可用" : "Runtime unavailable";

  const counts = useMemo(() => {
    const next = { all: activeLogs.length, route: 0, compression: 0, provider: 0, error: 0 };
    activeLogs.forEach((entry) => {
      next[entry.type] += 1;
    });
    return next;
  }, [activeLogs]);

  const shown = activeLogs.filter((l) => filter === "all" || l.type === filter);
  const visibleLogs = shown.slice(0, visibleLogCount);

  useEffect(() => {
    setVisibleLogCount(LOG_PAGE_SIZE);
  }, [filter, payload?.desktop_revision]);
  const securityRows = useMemo(
    () => buildSecurityRows(payload, payloadStatus, securityVerification, lang),
    [payload, payloadStatus, securityVerification, lang],
  );

  const exportLogs = async (format: "json" | "csv") => {
    if (!canExport) {
      setExportState({ state: "failed", format });
      addNotification({ dedupeKey: `export-${format}-unavailable`, level: "warning", title: lang === "zh" ? "无法导出" : "Export unavailable", message: lang === "zh" ? "需要实时本地数据后才能生成脱敏导出。" : "Live local data is required for a redacted export." });
      return;
    }
    setExportState({ state: "exporting", format });
    try {
      const exported = await invokeNative<RuntimeExport>("export_dtoken_runtime", { format });
      downloadText(exported.content, exported.mime, exported.filename);
      setExportState({ state: "done", format });
      addNotification({ dedupeKey: `export-${format}-${exported.filename}`, level: "success", title: lang === "zh" ? "脱敏导出已生成" : "Redacted export created", message: exported.filename, persistent: true, details: `format=${exported.format} · source=${exported.source} · redacted=${exported.redacted}` });
    } catch (error) {
      setExportState({ state: "failed", format });
      const failure = presentOperationFailure("settings", error, lang);
      addNotification({ dedupeKey: `export-${format}-failed`, level: "error", title: failure.title, message: failure.message, details: failure.details });
    }
  };

  const verifySecurity = async () => {
    if (!nativeRuntime) {
      setSecurityState("preview");
      addNotification({ dedupeKey: "security-verification-unavailable", level: "warning", title: lang === "zh" ? "安全验证不可用" : "Security verification unavailable", message: lang === "zh" ? "浏览器预览不能读取本机安全状态；请在 d-token 桌面窗口中验证。" : "Browser preview cannot read local safety state; verify in the d-token desktop app." });
      return;
    }

    setSecurityState("checking");
    try {
      const result = await invokeNative<AgentRouteVerification>("verify_agent_route_config");
      setSecurityVerification(result);
      setSecurityState("done");
      addNotification({ dedupeKey: "security-verification", level: result.ok ? "success" : "warning", title: result.ok ? (lang === "zh" ? "安全验证已通过" : "Security verification passed") : (lang === "zh" ? "安全验证需要处理" : "Security verification needs attention"), message: `${result.passed}/${result.total}`, persistent: !result.ok, details: `passed=${result.passed} · total=${result.total}` });
    } catch (error) {
      setSecurityState("failed");
      const failure = presentOperationFailure("route", error, lang);
      addNotification({ dedupeKey: "security-verification-failed", level: "error", title: failure.title, message: failure.message, details: failure.details });
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        {!embedded && <h1 className="sr-only">{t("ls.title")}</h1>}
        <h2 className="text-xl font-bold tracking-tight">{t("ls.title")}</h2>
      </div>

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="logs">{t("ls.tab.logs")}</TabsTrigger>
          <TabsTrigger value="ledger">{lang === "zh" ? "Event Ledger" : "Event Ledger"}</TabsTrigger>
          <TabsTrigger value="secrets">{t("ls.tab.secrets")}</TabsTrigger>
        </TabsList>

        <TabsContent value="logs" className="mt-4 space-y-4">
          <Surface className="p-3" hover={false}>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Filter className="size-4" />
                <span>{lang === "zh" ? "日志筛选" : "Log filters"}</span>
              </div>

              <AnimatedSegmentedControl
                items={LOG_FILTERS.map((ty) => {
                  const meta = filterMeta[ty];
                  const Icon = meta.icon;
                  return {
                    value: ty,
                    label: (
                      <span className="inline-flex items-center gap-1.5">
                        <Icon className="size-3.5" />
                        <span>{lang === "zh" ? meta.zh : meta.en}</span>
                        <span className="log-filter-count">{counts[ty]}</span>
                      </span>
                    ),
                  };
                })}
                value={filter}
                onValueChange={(next) => setFilter(next as LogEntry["type"] | "all")}
                ariaLabel={lang === "zh" ? "日志筛选" : "Log filters"}
                reducedMotion={reducedMotion}
                className="log-filter-bar min-w-0 max-w-full"
              />

              <div className="flex items-center gap-2">
                <Button variant="outline" size="sm" onClick={() => exportLogs("json")} disabled={exportState.state === "exporting"} aria-busy={exportState.state === "exporting" && exportState.format === "json"}>
                  {exportState.state === "exporting" && exportState.format === "json" ? <RefreshCcw className="size-4 animate-spin" aria-hidden /> : <Download className="size-4" />}
                  {exportState.state === "exporting" && exportState.format === "json"
                    ? lang === "zh" ? "导出中" : "Exporting"
                    : "JSON"}
                </Button>
                <Button variant="outline" size="sm" onClick={() => exportLogs("csv")} disabled={exportState.state === "exporting"} aria-busy={exportState.state === "exporting" && exportState.format === "csv"}>
                  {exportState.state === "exporting" && exportState.format === "csv" ? <RefreshCcw className="size-4 animate-spin" aria-hidden /> : <Download className="size-4" />}
                  {exportState.state === "exporting" && exportState.format === "csv"
                    ? lang === "zh" ? "导出中" : "Exporting"
                    : "CSV"}
                </Button>
              </div>
            </div>
          </Surface>

          {exportState.state !== "idle" && exportState.state !== "exporting" && (
            <div className="flat-inset p-3 text-sm text-muted-foreground">
              {exportState.state === "done"
                ? lang === "zh"
                  ? `已从 d-token runtime 导出脱敏 ${exportState.format?.toUpperCase() ?? ""}。`
                  : `Exported redacted ${exportState.format?.toUpperCase() ?? ""} from d-token runtime.`
                : lang === "zh"
                  ? "导出失败，请确认 d-token runtime 是否运行。"
                  : "Export failed. Check whether the d-token runtime is running."}
            </div>
          )}

          {payloadStatus === "runtime" && shown.length === 0 && (
            <div className="flat-inset p-3 text-sm text-muted-foreground">
              {lang === "zh"
                ? "d-token runtime 当前没有可显示的 request log / route decision / event。完成 Agent 连接并验证路由后，这里会显示真实链路。"
                : "d-token runtime has no request log / route decision / event yet. Connect an agent and verify the route to populate this view."}
            </div>
          )}
          <Terminal lines={visibleLogs} />
          {shown.length > visibleLogs.length && (
            <div className="flex justify-center pt-2">
              <Button variant="outline" size="sm" onClick={() => setVisibleLogCount((count) => count + LOG_PAGE_SIZE)}>
                {lang === "zh" ? `加载更多（已显示 ${visibleLogs.length}/${shown.length}）` : `Load more (${visibleLogs.length}/${shown.length})`}
              </Button>
            </div>
          )}
        </TabsContent>

        <TabsContent value="ledger" className="mt-4 space-y-4">
          <div className="grid gap-3 sm:grid-cols-4">
            <LedgerSummaryCard label="Events" value={ledgerSummary.events} />
            <LedgerSummaryCard label="Routes" value={ledgerSummary.routes} />
            <LedgerSummaryCard label="Requests" value={ledgerSummary.requests} />
            <LedgerSummaryCard label="Failed" value={ledgerSummary.failed} tone={ledgerSummary.failed > 0 ? "error" : "ok"} />
          </div>

          <Surface className="p-4" hover={false}>
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <ListChecks className="mt-0.5 size-5 text-primary" />
                <div>
                  <div className="font-medium">
                    {lang === "zh" ? "运行证据链" : "Runtime evidence chain"}
                  </div>
                  <div className="mt-1 text-sm text-muted-foreground">
                    {payloadStatus === "runtime"
                      ? lang === "zh"
                        ? "来自 d-token-owned Event Ledger、route decisions 和 request logs。这里不显示 raw prompt、raw response 或明文凭据。"
                        : "From d-token-owned Event Ledger, route decisions, and request logs. Raw prompts, raw responses, and plaintext credentials are not displayed."
                      : payloadStatus === "browser-preview"
                        ? lang === "zh"
                          ? "浏览器预览不会读取本机 Event Ledger；桌面 runtime 有数据后这里会实时刷新。"
                          : "Browser preview does not read the local Event Ledger. This refreshes when desktop runtime data is available."
                        : lang === "zh"
                          ? "d-token runtime 暂不可用，等待本地服务数据。"
                          : "d-token runtime is unavailable; waiting for local service data."}
                  </div>
                </div>
              </div>
                  </div>
          </Surface>

          {ledgerRows.length === 0 ? (
            <div className="flat-inset p-3 text-sm text-muted-foreground">
              {payloadStatus === "browser-preview"
                ? lang === "zh"
                  ? "预览模式没有真实 ledger 行。完成 Agent 连接并让请求经过 d-token 后，事件、路由决策和请求日志会显示在这里。"
                  : "Preview mode has no real ledger rows. After agent traffic goes through d-token, events, route decisions, and request logs appear here."
                : lang === "zh"
                  ? "当前没有 Event Ledger 记录。"
                  : "There are no Event Ledger records yet."}
            </div>
          ) : (
            <div className="space-y-2">
              {ledgerRows.map((row, index) => (
                <motion.div
                  key={row.id}
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ delay: Math.min(index * 0.025, 0.2) }}
                  className="flat-inset grid gap-3 p-3 text-sm md:grid-cols-[140px_1fr_auto]"
                >
                  <div>
                    <Badge variant={row.status === "failed" ? "destructive" : row.kind === "request" ? "default" : "secondary"}>
                      {row.kind}
                    </Badge>
                    <div className="mt-1 text-xs text-muted-foreground">{row.time}</div>
                  </div>
                  <div>
                    <div className="font-medium">{row.title}</div>
                    <div className="mt-1 text-xs text-muted-foreground">{row.detail}</div>
                  </div>
                  <div className="flex items-center justify-start gap-2 md:justify-end">
                    <Badge variant="outline">{row.tokens}</Badge>
                    <StatusBadge
                      status={row.status === "failed" ? "error" : row.status === "warning" ? "warning" : "healthy"}
                      label={row.status}
                    />
                  </div>
                </motion.div>
              ))}
            </div>
          )}
        </TabsContent>

        <TabsContent value="secrets" className="mt-4 space-y-3">
          <Surface className="p-4" hover={false}>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-start gap-3">
                <ShieldCheck className="mt-0.5 size-5 text-emerald-500" />
                <div>
                  <div className="font-medium">
                    {lang === "zh" ? "凭据与脱敏状态" : "Credential and redaction status"}
                  </div>
                  <div className="mt-1 text-sm text-muted-foreground">
                    {lang === "zh"
                      ? "这里只显示 d-token 配置引用和安全状态，不展示明文密钥、认证头或会话凭据。"
                      : "This view shows d-token config references and safety state, never plaintext keys, auth headers, or session credentials."}
                  </div>
                </div>
              </div>
              <Button size="sm" variant="outline" onClick={verifySecurity} disabled={securityState === "checking"} aria-busy={securityState === "checking"}>
                <RefreshCcw className={`size-3.5 ${securityState === "checking" ? "animate-spin" : ""}`} />
                {securityState === "checking"
                  ? lang === "zh" ? "验证中" : "Verifying"
                  : t("common.verify")}
              </Button>
            </div>
          </Surface>

          {securityState !== "idle" && (
            <div className="flat-inset p-3 text-sm text-muted-foreground">
              {securityState === "done" && securityVerification
                ? lang === "zh"
                  ? `配置验证 ${securityVerification.passed}/${securityVerification.total} 通过。`
                  : `Configuration verification passed ${securityVerification.passed}/${securityVerification.total}.`
                : securityState === "preview"
                  ? lang === "zh"
                    ? "浏览器预览不能读取本机 d-token 安全状态，请在桌面窗口中验证。"
                    : "Browser preview cannot read local d-token safety state. Verify in the desktop app."
                  : securityState === "failed"
                    ? lang === "zh"
                      ? "验证失败，请确认 d-token runtime 和 active config 是否可用。"
                      : "Verification failed. Check whether d-token runtime and active config are available."
                    : lang === "zh"
                      ? "正在读取 d-token 安全状态。"
                      : "Reading d-token safety state."}
            </div>
          )}

          {securityRows.map((row, i) => (
            <motion.div
              key={row.id}
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: i * 0.05 }}
            >
              <Surface className="flex flex-wrap items-center justify-between gap-3 p-4">
                <div className="flex items-center gap-3">
                  <KeyRound className="size-5 text-primary" />
                  <div>
                    <div className="text-sm font-medium">{row.title}</div>
                    <div className="mt-1 text-xs text-muted-foreground">{row.detail}</div>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="secondary">{row.meta}</Badge>
                  <StatusBadge status={row.status} label={row.label} />
                </div>
              </Surface>
            </motion.div>
          ))}
        </TabsContent>
      </Tabs>
    </div>
  );
}

type SecurityRow = {
  id: string;
  title: string;
  detail: string;
  meta: string;
  status: "healthy" | "warning" | "error";
  label: string;
};

type LedgerRow = {
  id: string;
  kind: "event" | "route" | "request";
  time: string;
  sortTime: number;
  title: string;
  detail: string;
  status: "ok" | "warning" | "failed";
  tokens: string;
};

function LedgerSummaryCard({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: number;
  tone?: "neutral" | "ok" | "error";
}) {
  const color = tone === "error" ? "text-rose-500" : tone === "ok" ? "text-emerald-500" : "text-foreground";
  return (
    <Surface className="p-3" hover={false}>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={`mt-1 text-2xl font-semibold ${color}`}>{value.toLocaleString("en-US")}</div>
    </Surface>
  );
}

function summarizeLedgerRows(rows: LedgerRow[]) {
  return {
    events: rows.filter((row) => row.kind === "event").length,
    routes: rows.filter((row) => row.kind === "route").length,
    requests: rows.filter((row) => row.kind === "request").length,
    failed: rows.filter((row) => row.status === "failed").length,
  };
}

type AgentRouteVerification = {
  ok: boolean;
  passed: number;
  total: number;
  route?: unknown;
  provider?: unknown;
  checks?: Array<{ id: string; ok: boolean; summary?: string }>;
};

function buildSecurityRows(
  payload: Record<string, any> | null,
  payloadStatus: ReturnType<typeof useRuntimeDashboardDetail>["status"],
  verification: AgentRouteVerification | null,
  lang: "zh" | "en",
): SecurityRow[] {
  const active = payload?.desktop_active_config ?? payload?.active_config;
  const config = active?.config ?? active;
  const provider = active?.provider ?? config?.provider ?? {};
  const route = active?.route ?? config?.route ?? {};
  const exportPayload = payload?.export ?? {};
  const verificationChecks = Array.isArray(verification?.checks) ? verification.checks : [];
  const exportCheck = verificationChecks.find((check) => check.id === "export_redaction");
  const fallbackCheck = verificationChecks.find((check) => check.id === "fallback");
  const activeLoaded = active?.state === "loaded" || config?.loaded === true || Number(config?.route_count ?? 0) > 0;
  const fallbackDisabled = Boolean(active?.fallback_disabled ?? config?.fallback_disabled ?? fallbackCheck?.ok ?? false);
  const hasKeyRef = Boolean(provider?.has_api_key_ref);
  const exportRedacted = Boolean(exportPayload?.redacted ?? exportCheck?.ok ?? false);
  const rawUnavailable = exportPayload?.raw_available === false || exportCheck?.ok === true;
  const runtimeMode = payloadStatus === "runtime";

  if (!runtimeMode && !verification) {
    return [
      {
        id: "preview",
        title: lang === "zh" ? "浏览器预览" : "Browser preview",
        detail: lang === "zh"
          ? "桌面运行时可用后，这里会显示 active config、凭据引用和导出脱敏状态。"
          : "When the desktop runtime is available, this shows active config, credential references, and export redaction state.",
        meta: lang === "zh" ? "无本机证据" : "No local evidence",
        status: "warning",
        label: lang === "zh" ? "待连接" : "Pending",
      },
    ];
  }

  return [
    {
      id: "active-config",
      title: lang === "zh" ? "Active d-token config" : "Active d-token config",
      detail: activeLoaded
        ? lang === "zh"
          ? `${safeText(route?.id, "route")} 已加载，provider=${safeText(provider?.id, "provider")}`
          : `${safeText(route?.id, "route")} loaded, provider=${safeText(provider?.id, "provider")}`
        : lang === "zh"
          ? "尚未读取到已应用的 d-token active config。"
          : "No applied d-token active config has been read yet.",
      meta: safeText(config?.source ?? active?.source, "d-token"),
      status: activeLoaded ? "healthy" : "warning",
      label: activeLoaded ? (lang === "zh" ? "已加载" : "Loaded") : (lang === "zh" ? "待应用" : "Pending"),
    },
    {
      id: "credential-reference",
      title: lang === "zh" ? "凭据引用" : "Credential reference",
      detail: hasKeyRef
        ? lang === "zh"
          ? "provider 只暴露 api_key_ref 是否存在，不解析或显示明文密钥。"
          : "The provider exposes only whether api_key_ref exists; plaintext keys are not resolved or displayed."
        : lang === "zh"
          ? "当前 provider 没有明文密钥引用，或还未配置凭据引用。"
          : "The current provider has no plaintext key reference, or no credential reference is configured yet.",
      meta: hasKeyRef ? "api_key_ref" : "no plaintext",
      status: hasKeyRef ? "healthy" : "warning",
      label: hasKeyRef
        ? (lang === "zh" ? "已引用" : "Referenced")
        : (lang === "zh" ? "待配置" : "Pending"),
    },
    {
      id: "fallback-boundary",
      title: lang === "zh" ? "Fallback 边界" : "Fallback boundary",
      detail: fallbackDisabled
        ? lang === "zh"
          ? "当前路由未启用静默 fallback。"
          : "Silent fallback is disabled for the current route."
        : lang === "zh"
          ? "还没有证明 fallback 已关闭。"
          : "Fallback has not been proven disabled yet.",
      meta: safeText(route?.target_provider_id ?? provider?.id, "route"),
      status: fallbackDisabled ? "healthy" : "warning",
      label: fallbackDisabled ? (lang === "zh" ? "已关闭" : "Disabled") : (lang === "zh" ? "待验证" : "Verify"),
    },
    {
      id: "export-redaction",
      title: lang === "zh" ? "导出脱敏" : "Export redaction",
      detail: exportRedacted && rawUnavailable
        ? lang === "zh"
          ? "runtime 导出已证明为脱敏，raw export 不可用。"
          : "Runtime export is proven redacted and raw export is unavailable."
        : lang === "zh"
          ? "导出脱敏状态需要运行配置验证或 runtime 导出后确认。"
          : "Export redaction state needs route verification or a runtime export to confirm.",
      meta: rawUnavailable ? "raw=false" : "raw=?",
      status: exportRedacted && rawUnavailable ? "healthy" : "warning",
      label: exportRedacted && rawUnavailable ? (lang === "zh" ? "已脱敏" : "Redacted") : (lang === "zh" ? "待验证" : "Verify"),
    },
  ];
}

function runtimePayloadToLogs(payload: Record<string, any> | null): LogEntry[] {
  if (!payload) return [];

  const lines: SortableLogEntry[] = [];
  const requestLogs = Array.isArray(payload.request_logs) ? payload.request_logs : [];
  const routeDecisions = Array.isArray(payload.route_decisions) ? payload.route_decisions : [];
  const events = Array.isArray(payload.events) ? payload.events : [];

  for (const decision of routeDecisions) {
    const route = safeText(decision.route_id, "—");
    const provider = safeText(decision.provider_id, "—");
    const chain = safeText(decision.chain_mode, "—");
    const fallback = fallbackLabel(decision.fallback);
    lines.push(sortableLine({
      id: `route-${safeText(decision.request_id, route)}-${lines.length}`,
      time: timeFromUnknown(decision.timestamp_ms),
      type: "route",
      agent: safeText(decision.agent, "未知 Agent"),
      msg: `路由 ${route} -> ${provider} · ${chain} · fallback=${fallback}`,
      msgEn: `Route ${route} -> ${provider} · ${chain} · fallback=${fallback}`,
    }, decision.timestamp_ms));
  }

  for (const log of requestLogs) {
    const agent = safeText(log.agent, "未知 Agent");
    const before = finiteNumber(log.input_tokens_before);
    const after = finiteNumber(log.input_tokens_after);
    const explicitSaved = finiteNumber(log.tokens_saved);
    const physicalSaved = before !== null && after !== null ? Math.max(0, before - after) : null;
    const saved = explicitSaved ?? physicalSaved;
    const pendingAttribution = (physicalSaved ?? 0) > 0 && explicitSaved === 0 && log.context_accounting != null;
    const route = safeText(log.route_id, "—");
    const provider = safeText(log.provider_id, "—");
    const model = safeText(log.model, "—");
    const latency = finiteNumber(log.latency_ms);
    const transforms = Array.isArray(log.transforms_applied)
      ? log.transforms_applied.filter((item: unknown): item is string => typeof item === "string" && item.trim().length > 0)
      : [];

    if ((before ?? 0) > 0 || (saved ?? 0) > 0) {
      lines.push(sortableLine({
        id: `compression-${safeText(log.request_id, route)}-${lines.length}`,
        time: timeFromUnknown(log.timestamp_ms),
        type: "compression",
        agent,
        msg: pendingAttribution
          ? `压缩 ${formatNumber(before)} -> ${formatNumber(after)} tokens，物理减少 ${formatNumber(physicalSaved)}；可计入节省 0（待上游实测归因）${transforms.length > 0 ? ` · ${transforms.join(",")}` : ""}`
          : `压缩 ${formatNumber(before)} -> ${formatNumber(after)} tokens，节省 ${formatNumber(saved)}${transforms.length > 0 ? ` · ${transforms.join(",")}` : ""}`,
        msgEn: pendingAttribution
          ? `Compressed ${formatNumber(before)} -> ${formatNumber(after)} tokens, physically reduced ${formatNumber(physicalSaved)}; claimable savings 0 (provider attribution pending)${transforms.length > 0 ? ` · ${transforms.join(",")}` : ""}`
          : `Compressed ${formatNumber(before)} -> ${formatNumber(after)} tokens, saved ${formatNumber(saved)}${transforms.length > 0 ? ` · ${transforms.join(",")}` : ""}`,
      }, log.timestamp_ms));
    }

    lines.push(sortableLine({
      id: `provider-${safeText(log.request_id, provider)}-${lines.length}`,
      time: timeFromUnknown(log.timestamp_ms),
      type: log.error ? "error" : "provider",
      agent,
      msg: log.error
        ? presentOperationFailure("connection", log.error, "zh").message
        : `Provider ${provider} · model=${model} · route=${route} · latency=${latency === null ? "—" : `${latency}ms`}`,
      msgEn: log.error
        ? presentOperationFailure("connection", log.error, "en").message
        : `Provider ${provider} · model=${model} · route=${route} · latency=${latency === null ? "—" : `${latency}ms`}`,
    }, log.timestamp_ms));
  }

  for (const event of events) {
    const stage = safeText(event.stage, "event");
    const status = safeText(event.status, "—");
    const source = safeText(event.source, "dtoken");
    const saved = Number(event.saved_tokens ?? 0);
    lines.push(sortableLine({
      id: `event-${safeText(event.event_id, stage)}-${lines.length}`,
      time: timeFromUnknown(event.timestamp_ms),
      type: status === "failed" || event.error_code ? "error" : saved > 0 || stage.includes("compress") ? "compression" : "route",
      agent: safeText(event.agent_id, "未知 Agent"),
      msg: `事件 ${stage} · ${source} · status=${status}${saved > 0 ? ` · saved=${saved}` : ""}`,
      msgEn: `Event ${stage} · ${source} · status=${status}${saved > 0 ? ` · saved=${saved}` : ""}`,
    }, event.timestamp_ms));
  }

  return lines.sort((a, b) => b.sortTime - a.sortTime).map(({ sortTime: _sortTime, ...line }) => line);
}

function runtimePayloadToLedgerRows(
  payload: Record<string, any> | null,
  payloadStatus: ReturnType<typeof useRuntimeDashboardDetail>["status"],
  lang: "zh" | "en",
): LedgerRow[] {
  if (payloadStatus !== "runtime" || !payload) return [];

  const rows: LedgerRow[] = [];
  const requestLogs = Array.isArray(payload.request_logs) ? payload.request_logs : [];
  const routeDecisions = Array.isArray(payload.route_decisions) ? payload.route_decisions : [];
  const events = Array.isArray(payload.events) ? payload.events : [];
  const backupPoints = Array.isArray(payload.backupPoints) ? payload.backupPoints : [];

  for (const backup of backupPoints) {
    if (backup.scope !== "learning-managed-block") continue;
    rows.push({
      id: `learning-${safeText(backup.id, "rollback")}-${rows.length}`,
      kind: "event",
      time: typeof backup.when === "string" ? backup.when : "—",
      sortTime: Date.parse(String(backup.when ?? "")) || 0,
      title: lang === "zh" ? "智能学习受控写入" : "Smart Learning managed write",
      detail: `rollback_id=${safeText(backup.id, "—")} · target=${safeText(backup.target, "—")} · verified=${backup.verified === true}`,
      status: backup.verified === true ? "ok" : "warning",
      tokens: "managed-block",
    });
  }

  for (const event of events) {
    const stage = safeText(event.stage, "event");
    const status = safeText(event.status, "—");
    const saved = Number(event.saved_tokens ?? 0);
    const inputTokens = Number(event.input_tokens ?? 0);
    const outputTokens = Number(event.output_tokens ?? 0);
    const error = safeText(event.error_code, "");
    rows.push({
      id: `event-${safeText(event.event_id, stage)}-${rows.length}`,
      kind: "event",
      time: timeFromUnknown(event.timestamp_ms),
      sortTime: sortTimeFromUnknown(event.timestamp_ms),
      title: lang === "zh" ? `事件 ${stage}` : `Event ${stage}`,
      detail: [
        `source=${safeText(event.source, "—")}`,
        `agent=${safeText(event.agent_id, "—")}`,
        `route=${safeText(event.route_id, "—")}`,
        `provider=${safeText(event.provider_id, "—")}`,
        error ? "needs_attention=true" : "",
      ].filter(Boolean).join(" · "),
      status: status === "failed" || error ? "failed" : status === "ok" ? "ok" : "warning",
      tokens: tokenSummary(inputTokens, outputTokens, saved),
    });
  }

  for (const decision of routeDecisions) {
    const route = safeText(decision.route_id, "—");
    const provider = safeText(decision.provider_id, "—");
    const fallback = fallbackLabel(decision.fallback);
    const chain = safeText(decision.chain_mode, "—");
    rows.push({
      id: `route-${safeText(decision.request_id, route)}-${rows.length}`,
      kind: "route",
      time: timeFromUnknown(decision.timestamp_ms),
      sortTime: sortTimeFromUnknown(decision.timestamp_ms),
      title: lang === "zh" ? `路由决策 ${route} -> ${provider}` : `Route decision ${route} -> ${provider}`,
      detail: `chain=${chain} · fallback=${fallback} · input=${protocolLabel(decision.input_protocol, lang) ?? "—"} · output=${protocolLabel(decision.output_protocol, lang) ?? "—"}`,
      status: fallback === "disabled" ? "ok" : "warning",
      tokens: "route",
    });
  }

  for (const log of requestLogs) {
    const beforeValue = finiteNumber(log.input_tokens_before);
    const afterValue = finiteNumber(log.input_tokens_after);
    const before = beforeValue ?? 0;
    const after = afterValue ?? 0;
    const physicalSaved = beforeValue !== null && afterValue !== null ? Math.max(0, before - after) : 0;
    const claimableSaved = finiteNumber(log.tokens_saved);
    const saved = claimableSaved ?? physicalSaved;
    const pendingAttribution = physicalSaved > 0 && claimableSaved === 0 && log.context_accounting != null;
    const error = safeText(log.error, "");
    rows.push({
      id: `request-${safeText(log.request_id, safeText(log.route_id, "request"))}-${rows.length}`,
      kind: "request",
      time: timeFromUnknown(log.timestamp_ms),
      sortTime: sortTimeFromUnknown(log.timestamp_ms),
      title: lang === "zh"
        ? `${safeText(log.agent, "Agent")} 请求 · ${safeText(log.model, "model")}`
        : `${safeText(log.agent, "Agent")} request · ${safeText(log.model, "model")}`,
      detail: [
        `route=${safeText(log.route_id, "—")}`,
        `provider=${safeText(log.provider_id, "—")}`,
        `protocol=${protocolLabel(log.input_protocol ?? log.protocol, lang) ?? "—"}`,
        `latency=${finiteNumber(log.latency_ms) === null ? "—" : `${finiteNumber(log.latency_ms)}ms`}`,
      ].join(" · "),
      status: error ? "failed" : "ok",
      tokens: pendingAttribution
        ? `${physicalSaved.toLocaleString("en-US")} physical · 0 claimable`
        : tokenSummary(before, after, saved),
    });
  }

  return rows.sort((a, b) => b.sortTime - a.sortTime).slice(0, 80);
}

type SortableLogEntry = LogEntry & { sortTime: number };

type RuntimeExport = {
  format: "json" | "csv";
  mime: string;
  filename: string;
  content: string;
  source: string;
  redacted: boolean;
};

function downloadText(content: string, mime: string, filename: string) {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

function timeFromUnknown(value: unknown) {
  const ms = Number(value);
  if (!Number.isFinite(ms) || ms <= 0) return "—";
  return new Date(ms).toLocaleTimeString("en-GB", { hour12: false });
}

function sortTimeFromUnknown(value: unknown) {
  const ms = Number(value);
  return Number.isFinite(ms) ? ms : 0;
}

function tokenSummary(input: number, output: number, saved: number) {
  if (input <= 0 && output <= 0 && saved <= 0) return "—";
  if (saved > 0) return `${saved.toLocaleString("en-US")} saved`;
  return `${input.toLocaleString("en-US")} -> ${output.toLocaleString("en-US")}`;
}

function sortableLine(line: LogEntry, value: unknown): SortableLogEntry {
  const ms = Number(value);
  return { ...line, sortTime: Number.isFinite(ms) ? ms : 0 };
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}

function finiteNumber(value: unknown): number | null {
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function formatNumber(value: number | null): string {
  return value === null ? "—" : value.toLocaleString("en-US");
}

function fallbackLabel(value: unknown): string {
  if (value === false || value === "disabled") return "disabled";
  if (value === true || value === "enabled") return "enabled";
  return safeText(value, "—");
}
