import { useMemo, useState } from "react";
import { motion } from "motion/react";
import { Download, KeyRound, RefreshCcw, ShieldCheck, Filter, Route, Gauge, Server, AlertTriangle, Layers, ListChecks } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Terminal } from "../common/Terminal";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { useI18n, useT } from "../../lib/i18n";
import { logs, type LogEntry } from "../../lib/fixtures";
import { invokeNative, isTauriRuntime } from "../../lib/native";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";

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

export function LogsSecurity() {
  const t = useT();
  const { lang } = useI18n();
  const [filter, setFilter] = useState<LogEntry["type"] | "all">("all");
  const { payload, status: payloadStatus, updatedAt } = useRuntimeDashboard();
  const [exportState, setExportState] = useState<{ state: "idle" | "exporting" | "done" | "fallback" | "failed"; format?: "json" | "csv" }>({ state: "idle" });
  const [securityState, setSecurityState] = useState<"idle" | "checking" | "done" | "preview" | "failed">("idle");
  const [securityVerification, setSecurityVerification] = useState<AgentRouteVerification | null>(null);
  const types: (LogEntry["type"] | "all")[] = ["all", "route", "compression", "provider", "error"];

  const runtimeLogs = useMemo(() => runtimePayloadToLogs(payload), [payload]);
  const ledgerRows = useMemo(() => runtimePayloadToLedgerRows(payload, payloadStatus, lang), [payload, payloadStatus, lang]);
  const ledgerSummary = useMemo(() => summarizeLedgerRows(ledgerRows), [ledgerRows]);
  const activeLogs = payloadStatus === "browser-preview" ? logs : runtimeLogs;
  const dataSourceLabel =
    payloadStatus === "runtime"
      ? `${payload?.desktop_data_source ?? payload?.source ?? "d-token-owned-runtime"} · ${activeLogs.length} logs${updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}`
      : payloadStatus === "browser-preview"
        ? lang === "zh" ? "浏览器预览 · 示例数据" : "Browser preview · sample data"
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
  const securityRows = useMemo(
    () => buildSecurityRows(payload, payloadStatus, securityVerification, lang),
    [payload, payloadStatus, securityVerification, lang],
  );

  const exportLogs = async (format: "json" | "csv") => {
    setExportState({ state: "exporting", format });
    if (isTauriRuntime()) {
      try {
        const exported = await invokeNative<RuntimeExport>("export_dtoken_runtime", { format });
        downloadText(exported.content, exported.mime, exported.filename);
        setExportState({ state: "done", format });
        return;
      } catch (_error) {
        setExportState({ state: "fallback", format });
      }
    }

    const fallback = buildVisibleLogExport(format, shown, payloadStatus);
    downloadText(fallback.content, fallback.mime, fallback.filename);
    if (!isTauriRuntime()) {
      setExportState({ state: "fallback", format });
    }
  };

  const verifySecurity = async () => {
    if (!isTauriRuntime()) {
      setSecurityState("preview");
      return;
    }

    setSecurityState("checking");
    try {
      const result = await invokeNative<AgentRouteVerification>("verify_agent_route_config");
      setSecurityVerification(result);
      setSecurityState("done");
    } catch (_error) {
      setSecurityState("failed");
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <h1>{t("ls.title")}</h1>
        <DataSourceBadge mode={payloadStatus === "runtime" ? "runtime" : payloadStatus === "unavailable" ? "unavailable" : "preview"} />
        <Badge variant={payloadStatus === "runtime" ? "default" : "secondary"}>{dataSourceLabel}</Badge>
      </div>

      <Tabs defaultValue="logs">
        <TabsList>
          <TabsTrigger value="logs">{t("ls.tab.logs")}</TabsTrigger>
          <TabsTrigger value="ledger">{lang === "zh" ? "Event Ledger" : "Event Ledger"}</TabsTrigger>
          <TabsTrigger value="secrets">{t("ls.tab.secrets")}</TabsTrigger>
        </TabsList>

        <TabsContent value="logs" className="mt-4 space-y-4">
          <GlassCard className="p-3" hover={false}>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Filter className="size-4" />
                <span>{lang === "zh" ? "日志筛选" : "Log filters"}</span>
              </div>

              <div className="log-filter-bar">
                {types.map((ty) => {
                  const meta = filterMeta[ty];
                  const Icon = meta.icon;
                  return (
                    <motion.button
                      key={ty}
                      type="button"
                      onClick={() => setFilter(ty)}
                      className={filter === ty ? "log-filter-item active" : "log-filter-item"}
                      whileHover={{ y: -1 }}
                      whileTap={{ scale: 0.98 }}
                    >
                      <Icon className="size-3.5" />
                      <span>{lang === "zh" ? meta.zh : meta.en}</span>
                      <span className="log-filter-count">{counts[ty]}</span>
                    </motion.button>
                  );
                })}
              </div>

              <div className="flex items-center gap-2">
                <Button variant="outline" size="sm" onClick={() => exportLogs("json")} disabled={exportState.state === "exporting"}>
                  <Download className="size-4" />
                  {exportState.state === "exporting" && exportState.format === "json"
                    ? lang === "zh" ? "导出中" : "Exporting"
                    : "JSON"}
                </Button>
                <Button variant="outline" size="sm" onClick={() => exportLogs("csv")} disabled={exportState.state === "exporting"}>
                  <Download className="size-4" />
                  {exportState.state === "exporting" && exportState.format === "csv"
                    ? lang === "zh" ? "导出中" : "Exporting"
                    : "CSV"}
                </Button>
              </div>
            </div>
          </GlassCard>

          {exportState.state !== "idle" && exportState.state !== "exporting" && (
            <div className="glass-inset p-3 text-sm text-muted-foreground">
              {exportState.state === "done"
                ? lang === "zh"
                  ? `已从 d-token runtime 导出脱敏 ${exportState.format?.toUpperCase() ?? ""}。`
                  : `Exported redacted ${exportState.format?.toUpperCase() ?? ""} from d-token runtime.`
                : exportState.state === "fallback"
                  ? lang === "zh"
                    ? `runtime 导出不可用，已导出当前页面可见的脱敏 ${exportState.format?.toUpperCase() ?? ""} 预览。`
                    : `Runtime export was unavailable; exported the current redacted visible ${exportState.format?.toUpperCase() ?? ""} preview.`
                  : lang === "zh"
                    ? "导出失败，请确认 d-token runtime 是否运行。"
                    : "Export failed. Check whether the d-token runtime is running."}
            </div>
          )}

          {payloadStatus === "runtime" && shown.length === 0 && (
            <div className="glass-inset p-3 text-sm text-muted-foreground">
              {lang === "zh"
                ? "d-token runtime 当前没有可显示的 request log / route decision / event。完成 Agent 连接并验证路由后，这里会显示真实链路。"
                : "d-token runtime has no request log / route decision / event yet. Connect an agent and verify the route to populate this view."}
            </div>
          )}
          <Terminal lines={shown} />
        </TabsContent>

        <TabsContent value="ledger" className="mt-4 space-y-4">
          <div className="grid gap-3 sm:grid-cols-4">
            <LedgerSummaryCard label="Events" value={ledgerSummary.events} />
            <LedgerSummaryCard label="Routes" value={ledgerSummary.routes} />
            <LedgerSummaryCard label="Requests" value={ledgerSummary.requests} />
            <LedgerSummaryCard label="Failed" value={ledgerSummary.failed} tone={ledgerSummary.failed > 0 ? "error" : "ok"} />
          </div>

          <GlassCard className="p-4" hover={false}>
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
              <Badge variant={payloadStatus === "runtime" ? "default" : "secondary"}>{dataSourceLabel}</Badge>
            </div>
          </GlassCard>

          {ledgerRows.length === 0 ? (
            <div className="glass-inset p-3 text-sm text-muted-foreground">
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
                  className="glass-inset grid gap-3 p-3 text-sm md:grid-cols-[140px_1fr_auto]"
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
          <GlassCard className="p-4" hover={false}>
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
              <Button size="sm" variant="outline" onClick={verifySecurity} disabled={securityState === "checking"}>
                <RefreshCcw className="size-3.5" />
                {securityState === "checking"
                  ? lang === "zh" ? "验证中" : "Verifying"
                  : t("common.verify")}
              </Button>
            </div>
          </GlassCard>

          {securityState !== "idle" && (
            <div className="glass-inset p-3 text-sm text-muted-foreground">
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
              <GlassCard className="flex flex-wrap items-center justify-between gap-3 p-4">
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
              </GlassCard>
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
    <GlassCard className="p-3" hover={false}>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={`mt-1 text-2xl font-semibold ${color}`}>{value.toLocaleString("en-US")}</div>
    </GlassCard>
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
  payloadStatus: ReturnType<typeof useRuntimeDashboard>["status"],
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
        meta: lang === "zh" ? "示例状态" : "Preview state",
        status: "warning",
        label: lang === "zh" ? "待连接" : "Pending",
      },
      {
        id: "redaction-default",
        title: lang === "zh" ? "默认脱敏策略" : "Default redaction policy",
        detail: lang === "zh"
          ? "报告与日志导出默认不包含明文密钥、认证头或完整模型输出。"
          : "Reports and log exports default to excluding plaintext keys, auth headers, and full model output.",
        meta: lang === "zh" ? "本地优先" : "Local-first",
        status: "healthy",
        label: lang === "zh" ? "已启用" : "Enabled",
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
      status: "healthy",
      label: lang === "zh" ? "脱敏" : "Redacted",
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
    const route = safeText(decision.route_id, "unknown-route");
    const provider = safeText(decision.provider_id, "unknown-provider");
    const chain = safeText(decision.chain_mode, "unknown-chain");
    const fallback = safeText(decision.fallback, "disabled");
    lines.push(sortableLine({
      id: `route-${safeText(decision.request_id, route)}-${lines.length}`,
      time: timeFromUnknown(decision.timestamp_ms),
      type: "route",
      agent: "OpenCode",
      msg: `路由 ${route} -> ${provider} · ${chain} · fallback=${fallback}`,
      msgEn: `Route ${route} -> ${provider} · ${chain} · fallback=${fallback}`,
    }, decision.timestamp_ms));
  }

  for (const log of requestLogs) {
    const agent = safeText(log.agent, "OpenCode");
    const before = Number(log.input_tokens_before ?? 0);
    const after = Number(log.input_tokens_after ?? 0);
    const saved = Number(log.tokens_saved ?? Math.max(0, before - after));
    const route = safeText(log.route_id, "unknown-route");
    const provider = safeText(log.provider_id, "unknown-provider");
    const model = safeText(log.model, "unknown-model");
    const latency = Number(log.latency_ms ?? 0);
    const transforms = Array.isArray(log.transforms_applied) ? log.transforms_applied.join(",") : "runtime_route";

    if (before > 0 || saved > 0) {
      lines.push(sortableLine({
        id: `compression-${safeText(log.request_id, route)}-${lines.length}`,
        time: timeFromUnknown(log.timestamp_ms),
        type: "compression",
        agent,
        msg: `压缩 ${before.toLocaleString()} -> ${after.toLocaleString()} tokens，节省 ${saved.toLocaleString()} · ${transforms}`,
        msgEn: `Compressed ${before.toLocaleString()} -> ${after.toLocaleString()} tokens, saved ${saved.toLocaleString()} · ${transforms}`,
      }, log.timestamp_ms));
    }

    lines.push(sortableLine({
      id: `provider-${safeText(log.request_id, provider)}-${lines.length}`,
      time: timeFromUnknown(log.timestamp_ms),
      type: log.error ? "error" : "provider",
      agent,
      msg: log.error
        ? `Provider ${provider} 失败：${safeText(log.error, "unknown error")}，未静默回退`
        : `Provider ${provider} · model=${model} · route=${route} · ${latency}ms`,
      msgEn: log.error
        ? `Provider ${provider} failed: ${safeText(log.error, "unknown error")}, no silent fallback`
        : `Provider ${provider} · model=${model} · route=${route} · ${latency}ms`,
    }, log.timestamp_ms));
  }

  for (const event of events) {
    const stage = safeText(event.stage, "event");
    const status = safeText(event.status, "ok");
    const source = safeText(event.source, "dtoken");
    const saved = Number(event.saved_tokens ?? 0);
    lines.push(sortableLine({
      id: `event-${safeText(event.event_id, stage)}-${lines.length}`,
      time: timeFromUnknown(event.timestamp_ms),
      type: status === "failed" || event.error_code ? "error" : saved > 0 || stage.includes("compress") ? "compression" : "route",
      agent: safeText(event.agent_id, "d-token"),
      msg: `事件 ${stage} · ${source} · status=${status}${saved > 0 ? ` · saved=${saved}` : ""}`,
      msgEn: `Event ${stage} · ${source} · status=${status}${saved > 0 ? ` · saved=${saved}` : ""}`,
    }, event.timestamp_ms));
  }

  return lines.sort((a, b) => b.sortTime - a.sortTime).map(({ sortTime: _sortTime, ...line }) => line);
}

function runtimePayloadToLedgerRows(
  payload: Record<string, any> | null,
  payloadStatus: ReturnType<typeof useRuntimeDashboard>["status"],
  lang: "zh" | "en",
): LedgerRow[] {
  if (payloadStatus !== "runtime" || !payload) return [];

  const rows: LedgerRow[] = [];
  const requestLogs = Array.isArray(payload.request_logs) ? payload.request_logs : [];
  const routeDecisions = Array.isArray(payload.route_decisions) ? payload.route_decisions : [];
  const events = Array.isArray(payload.events) ? payload.events : [];

  for (const event of events) {
    const stage = safeText(event.stage, "event");
    const status = safeText(event.status, "ok");
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
        `source=${safeText(event.source, "dtoken")}`,
        `agent=${safeText(event.agent_id, "d-token")}`,
        `route=${safeText(event.route_id, "n/a")}`,
        `provider=${safeText(event.provider_id, "n/a")}`,
        error ? `error=${error}` : "",
      ].filter(Boolean).join(" · "),
      status: status === "failed" || error ? "failed" : "ok",
      tokens: tokenSummary(inputTokens, outputTokens, saved),
    });
  }

  for (const decision of routeDecisions) {
    const route = safeText(decision.route_id, "unknown-route");
    const provider = safeText(decision.provider_id, "unknown-provider");
    const fallback = safeText(decision.fallback, "disabled");
    const chain = safeText(decision.chain_mode, "unknown-chain");
    rows.push({
      id: `route-${safeText(decision.request_id, route)}-${rows.length}`,
      kind: "route",
      time: timeFromUnknown(decision.timestamp_ms),
      sortTime: sortTimeFromUnknown(decision.timestamp_ms),
      title: lang === "zh" ? `路由决策 ${route} -> ${provider}` : `Route decision ${route} -> ${provider}`,
      detail: `chain=${chain} · fallback=${fallback} · input=${safeText(decision.input_protocol, "unknown")} · output=${safeText(decision.output_protocol, "unknown")}`,
      status: fallback === "disabled" ? "ok" : "warning",
      tokens: "route",
    });
  }

  for (const log of requestLogs) {
    const before = Number(log.input_tokens_before ?? 0);
    const after = Number(log.input_tokens_after ?? 0);
    const saved = Number(log.tokens_saved ?? Math.max(0, before - after));
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
        `route=${safeText(log.route_id, "unknown-route")}`,
        `provider=${safeText(log.provider_id, "unknown-provider")}`,
        `latency=${Number(log.latency_ms ?? 0)}ms`,
        error ? `error=${error}` : "redacted=true",
      ].join(" · "),
      status: error ? "failed" : "ok",
      tokens: tokenSummary(before, after, saved),
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

function buildVisibleLogExport(format: "json" | "csv", rows: LogEntry[], source: string) {
  const timestamp = Date.now();
  if (format === "csv") {
    const header = ["time", "type", "agent", "message"].join(",");
    const body = rows.map((row) => [
      csvCell(row.time),
      csvCell(row.type),
      csvCell(row.agent),
      csvCell(row.msgEn || row.msg),
    ].join(",")).join("\n");
    return {
      content: `${header}\n${body}`,
      mime: "text/csv",
      filename: `dtoken-redacted-logs-${timestamp}.csv`,
    };
  }

  return {
    content: JSON.stringify(
      {
        exported_at: new Date().toISOString(),
        source,
        export_source: "frontend-visible-lines-fallback",
        redacted: true,
        logs: rows,
      },
      null,
      2,
    ),
    mime: "application/json",
    filename: `dtoken-redacted-logs-${timestamp}.json`,
  };
}

function csvCell(value: unknown) {
  const text = String(value ?? "");
  return `"${text.replace(/"/g, '""')}"`;
}

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
  if (!Number.isFinite(ms) || ms <= 0) return new Date().toLocaleTimeString("en-GB", { hour12: false });
  return new Date(ms).toLocaleTimeString("en-GB", { hour12: false });
}

function sortTimeFromUnknown(value: unknown) {
  const ms = Number(value);
  return Number.isFinite(ms) ? ms : 0;
}

function tokenSummary(input: number, output: number, saved: number) {
  if (input <= 0 && output <= 0 && saved <= 0) return "0 tok";
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
