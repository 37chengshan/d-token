import { useEffect, useMemo, useState } from "react";
import { motion, AnimatePresence } from "motion/react";
import { Rocket, RefreshCw, Search, ShieldCheck, Cpu, Server, CheckCircle2, Route, Settings2 } from "lucide-react";
import { Surface } from "../common/Surface";
import { StepProgress, type Step } from "../common/StepProgress";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { CapabilitySummary } from "../common/CapabilitySummary";
import { OperationDialog } from "../common/OperationDialog";
import { Button } from "../ui/button";
import { Checkbox } from "../ui/checkbox";
import { Badge } from "../ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "../ui/dialog";
import { useI18n, useT } from "../../lib/i18n";
import { AGENT_REGISTRY, type AgentDescriptor } from "../../lib/agents/registry";
import { invokeNative, isTauriRuntime, type NativeActionLog, type NativeActionResult, type NativeToolCheck } from "../../lib/native";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";
import { redactString } from "../../lib/security/redaction";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";

export function assertNativeActionSucceeded(result: NativeActionResult): void {
  if (!result.ok) {
    throw new Error(result.summary || "native_action_failed");
  }
}

export function deriveServicePresentation(serviceOnline: boolean, installing: boolean, loading = false) {
  if (serviceOnline) {
    return { isOnline: true, status: "healthy" as const, phase: "running" as const };
  }
  if (installing) {
    return { isOnline: false, status: "warning" as const, phase: "installing" as const };
  }
  if (loading) {
    return { isOnline: false, status: "warning" as const, phase: "loading" as const };
  }
  return { isOnline: false, status: "error" as const, phase: "stopped" as const };
}

export function Install({ onOpenAdvanced }: { onOpenAdvanced?: () => void } = {}) {
  const t = useT();
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const steps: Step[] = [
    { key: "scan", label: t("in.step.scan") },
    { key: "plan", label: t("in.step.plan") },
    { key: "apply", label: t("in.step.apply") },
    { key: "verify", label: t("in.step.verify") },
    { key: "doctor", label: t("in.step.doctor") },
  ];

  const [installing, setInstalling] = useState(false);
  const [stepIdx, setStepIdx] = useState(-1);
  const [detecting, setDetecting] = useState(false);
  const [agents, setAgents] = useState<InstallAgent[]>(() => AGENT_REGISTRY.map(toInstallAgent));
  const [selected, setSelected] = useState<string[]>([]);
  const [showDiff, setShowDiff] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [routeVerification, setRouteVerification] = useState<RouteVerification | null>(null);
  const [readinessResult, setReadinessResult] = useState<ProductReadinessResult | null>(null);
  const [serviceStatus, setServiceStatus] = useState<DtokenServiceStatus | null>(null);
  const [serviceAction, setServiceAction] = useState<"start" | "stop" | null>(null);
  const [confirmServiceStop, setConfirmServiceStop] = useState(false);
  const [nativeTools, setNativeTools] = useState<NativeToolCheck[]>([]);
  const [nativeNotice, setNativeNotice] = useState<string>("");
  const [transactionLogs, setTransactionLogs] = useState<NativeActionLog[]>([]);
  const [connectionReview, setConnectionReview] = useState<OpenCodeConnectionReview | null>(null);
  const [connectionApplying, setConnectionApplying] = useState(false);
  const [upstreamDiscovery, setUpstreamDiscovery] = useState<UpstreamDiscovery | null>(null);
  const [selectedCandidate, setSelectedCandidate] = useState("");
  const { payload, status: payloadStatus, operational, updatedAt, refresh: refreshRuntime } = useRuntimeDashboard();

  const done = stepIdx >= steps.length;
  const serviceOnline = payloadStatus === "runtime" || serviceStatus?.state === "running" || serviceStatus?.state === "unmanaged_running";
  const serviceManaged = serviceStatus?.state === "running" && serviceStatus.managed;
  const servicePresentation = deriveServicePresentation(serviceOnline, installing, payloadStatus === "loading" && serviceStatus === null);
  const hasNativeData = isTauriRuntime() && (nativeTools.length > 0 || transactionLogs.length > 0 || Boolean(routeVerification) || payloadStatus === "runtime");
  const primaryAgent = agents.find((agent) => agent.id === "opencode") ?? agents[0];
  const primaryAgentName = primaryAgent?.name ?? "Agent";
  const runtimeSummary = useMemo(() => installRuntimeSummary(payload, payloadStatus, updatedAt, lang), [payload, payloadStatus, updatedAt, lang]);
  const ccSwitchEvidence = useMemo(() => deriveCcSwitchEvidence(payload, payloadStatus, lang), [payload, payloadStatus, lang]);
  const activeModels = useMemo(() => activeRouteModels(payload), [payload]);
  const [selectedModel, setSelectedModel] = useState("");
  const candidateOptions = useMemo(() => flattenCandidates(upstreamDiscovery), [upstreamDiscovery]);
  const selectableModels = useMemo(() => Array.from(new Set([...activeModels, ...candidateOptions.map((item) => item.model)])), [activeModels, candidateOptions]);
  const serviceEndpoint = serviceStatus?.service_url || safeRuntimeText(payload?.desktop_service_url, "");
  const serviceAddress = serviceEndpoint ? serviceEndpoint.replace(/^https?:\/\//, "") : "—";

  const transactionLine = useMemo(() => {
    if (installing) {
      const current = steps[Math.min(stepIdx, steps.length - 1)]?.key ?? "scan";
      return lang === "zh"
        ? `> 正在执行安装事务 · 当前步骤 ${current}`
        : `> installation transaction · current step ${current}`;
    }
    if (done) return lang === "zh" ? "> 安装事务已完成 · 已按设置保留回滚点" : "> installation complete · rollback point retained when enabled";
    if (!serviceOnline) return lang === "zh" ? "> 等待启动 d-token 本地服务" : "> waiting for the local d-token service";
    return lang === "zh" ? "> 本地服务已就绪，可以连接并验证工具" : "> local service ready; connect and verify an agent";
  }, [done, installing, lang, serviceOnline, stepIdx, steps]);

  const runtimeLine = serviceOnline
    ? lang === "zh"
      ? `> d-token 本地服务 ${serviceAddress} · ${serviceManaged ? "由应用管理" : "已检测到运行实例"}`
      : `> local d-token service ${serviceAddress} · ${serviceManaged ? "managed by the app" : "running instance detected"}`
    : lang === "zh"
      ? "> d-token 本地服务暂不可用 · 启动后再连接并验证工具"
      : "> local d-token service unavailable · start it before connecting and verifying an agent";

  const refreshServiceStatus = async () => {
    await refreshRuntime();
  };

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void Promise.allSettled([
      invokeNative<NativeActionResult>("detect_tools"),
      invokeNative<UpstreamDiscovery>("discover_upstream_candidates"),
    ]).then(([toolsResult, discoveryResult]) => {
      if (toolsResult.status === "fulfilled") {
        const checks = toolsResult.value.tools;
        setNativeTools(checks);
        setAgents((current) => current.map((agent) => {
          const tool = checks.find((item) => item.id === agent.id);
          return tool ? { ...agent, installed: tool.installed, binaryPath: tool.path ?? null, version: tool.version ?? null } : agent;
        }));
      }
      setUpstreamDiscovery(discoveryResult.status === "fulfilled" ? discoveryResult.value : null);
    });
  }, []);

  useEffect(() => {
    const next = operational?.service?.data;
    if (next && typeof next === "object") setServiceStatus(next as DtokenServiceStatus);
  }, [
    operational?.service?.data?.state,
    operational?.service?.data?.pid,
    operational?.service?.data?.managed,
    operational?.service?.data?.service_url,
  ]);

  const runServiceAction = async (action: "start" | "stop") => {
    setServiceAction(action);
    setNativeNotice("");
    try {
      const result = await invokeNative<NativeActionResult>(action === "start" ? "start_dtoken_service" : "stop_dtoken_service");
      setTransactionLogs(result.logs);
      assertNativeActionSucceeded(result);
      setNativeNotice(result.summary);
      applyToolChecks(result.tools);
      addNotification({
        dedupeKey: `service-action-${action}`,
        level: "success",
        title: action === "start" ? (lang === "zh" ? "本地服务已启动" : "Local service started") : (lang === "zh" ? "本地服务已停止" : "Local service stopped"),
        message: action === "start"
          ? (lang === "zh" ? "d-token 本地服务已启动并通过本地状态检查。" : "The d-token local service started and passed its local status check.")
          : (lang === "zh" ? "d-token 本地服务已按操作停止。" : "The d-token local service stopped as requested."),
      });
    } catch (error) {
      const summary = error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh"
            ? "当前是浏览器预览，服务生命周期需要在 Tauri 桌面窗口中运行。"
            : "Service lifecycle controls require the Tauri desktop window.")
        : (lang === "zh"
            ? `服务${action === "start" ? "启动" : "停止"}失败：${String(error)}`
            : `Failed to ${action} the service: ${String(error)}`);
      setNativeNotice(summary);
      setTransactionLogs([{ time: new Date().toLocaleTimeString(), level: "error", message: summary }]);
      addNotification({ dedupeKey: `service-action-${action}-failed`, level: "error", title: lang === "zh" ? "服务操作失败" : "Service action failed", message: summary });
    } finally {
      setServiceAction(null);
      await refreshServiceStatus();
    }
  };

  const runInstall = async () => {
    setInstalling(true);
    setStepIdx(0);
    setNativeNotice("");
    setTransactionLogs([]);
    try {
      const result = await invokeNative<NativeActionResult>("run_install_transaction", {
        request: {
          target: "dtoken",
          install_root: desktopInstallRoot(typeof navigator === "undefined" ? "" : navigator.userAgent),
          proxy_port: 18001,
          auto_backup: true,
          run_doctor_after: true,
          rollback_point: true,
          mode: "connect",
        },
      });
      setTransactionLogs(result.logs);
      applyToolChecks(result.tools);
      assertNativeActionSucceeded(result);
      setNativeNotice(lang === "zh" ? "安装与启动已完成。" : "Installation and startup completed.");
      setStepIdx(steps.length);
      addNotification({ dedupeKey: "install-transaction-complete", level: "success", title: lang === "zh" ? "安装与启动已完成" : "Installation complete", message: lang === "zh" ? "d-token 本地服务已安装、启动并完成验证。" : "The d-token local service was installed, started, and verified.", persistent: true, details: `transaction=install · stages=${result.logs.length}` });
    } catch (error) {
      const summary = error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh"
            ? "当前是浏览器预览，安装事务需要在 Tauri 桌面窗口中运行。"
            : "The install transaction requires the Tauri desktop window.")
        : (lang === "zh" ? `安装事务失败：${String(error)}` : `Install transaction failed: ${String(error)}`);
      setNativeNotice(summary);
      setTransactionLogs([{ time: new Date().toLocaleTimeString(), level: "error", message: summary }]);
      setStepIdx(-1);
      addNotification({ dedupeKey: "install-transaction-failed", level: "error", title: lang === "zh" ? "安装事务失败" : "Installation failed", message: summary });
    } finally {
      setInstalling(false);
      await refreshServiceStatus();
    }
  };

  const applyToolChecks = (tools: NativeToolCheck[]) => {
    setNativeTools(tools);
    setAgents((current) =>
      current.map((agent) => {
        const tool = tools.find((item) => item.id === agent.id);
        if (!tool) return agent;
        return {
          ...agent,
          installed: tool.installed,
          binaryPath: tool.path ?? null,
          version: tool.version ?? null,
        };
      }),
    );
  };

  const runDetect = async () => {
    setDetecting(true);
    setNativeNotice("");
    try {
      const result = await invokeNative<NativeActionResult>("detect_tools");
      applyToolChecks(result.tools);
      const detected = result.tools.filter((tool) => tool.installed).length;
      setNativeNotice(
        lang === "zh"
          ? `检测完成：${detected}/${result.tools.length} 个可检测工具已找到。未找到的工具不会显示为已安装。`
          : `Detection complete: ${detected}/${result.tools.length} detectable tools found. Missing tools remain unverified.`,
      );
      setUpstreamDiscovery(await invokeNative<UpstreamDiscovery>("discover_upstream_candidates"));
      await refreshRuntime();
    } catch (error) {
      const message = error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh"
            ? "当前是浏览器预览，本机检测需要在 Tauri 桌面窗口中运行。"
            : "Local detection requires the Tauri desktop window.")
        : (lang === "zh" ? `检测失败：${String(error)}` : `Detection failed: ${String(error)}`);
      setNativeNotice(message);
    } finally {
      setDetecting(false);
    }
  };

  const toggle = (id: string) => {
    setSelected((current) =>
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id],
    );
  };

  const verifyAfterManualConnection = async () => {
    setShowDiff(false);
    setSelected([]);
    await runRouteVerification();
  };

  const previewOpenCodeConnection = async () => {
    if (!selectedModel) {
      setConnectionReview(null);
      setNativeNotice(lang === "zh" ? "请先在路由设置中明确选择并应用一个上游模型。" : "Apply an upstream route and explicitly select one model first.");
      return;
    }
    try {
      const review = await invokeNative<OpenCodeConnectionReview>("preview_opencode_connection", {
        request: {
          model: selectedModel,
          listener_port: 18001,
          reviewed_revision: null,
          reviewed_before_sha256: null,
          reviewed_after_sha256: null,
        },
      });
      setConnectionReview(review);
      if (!review.ok) setNativeNotice(review.summary);
    } catch (error) {
      setConnectionReview(null);
      setNativeNotice(lang === "zh" ? `无法生成 OpenCode 配置预览：${String(error)}` : `Could not preview the OpenCode change: ${String(error)}`);
    }
  };

  const applyOpenCodeConnection = async () => {
    if (!connectionReview?.ok || !connectionReview.revision || !connectionReview.before_sha256 || !connectionReview.after_sha256) return;
    const requestId = createOperationRequestId("opencode-apply");
    setConnectionApplying(true);
    try {
      const receipt = await invokeNative<OpenCodeConnectionReceipt>("apply_opencode_connection", {
        request: {
          model: selectedModel,
          listener_port: 18001,
          reviewed_revision: connectionReview.revision,
          reviewed_before_sha256: connectionReview.before_sha256,
          reviewed_after_sha256: connectionReview.after_sha256,
        },
      });
      const localizedReceipt = receipt.ok
        ? (lang === "zh" ? "OpenCode 连接配置已按审阅内容写入并验证。" : "The reviewed OpenCode connection configuration was written and verified.")
        : (lang === "zh" ? "OpenCode 连接配置未应用；原配置保持不变。" : "The OpenCode connection configuration was not applied; the original configuration is unchanged.");
      setNativeNotice(localizedReceipt);
      setTransactionLogs((current) => [...current, {
        time: new Date().toLocaleTimeString(),
        level: receipt.ok ? "info" : "error",
        message: receipt.ok
          ? `${localizedReceipt} · rollback ${receipt.rollback_id ?? "—"}`
          : localizedReceipt,
      }]);
      if (receipt.ok) {
        addNotification({
          dedupeKey: `opencode-apply-${receipt.rollback_id ?? receipt.revision ?? "receipt"}`,
          level: "success",
          title: lang === "zh" ? "OpenCode 连接已应用" : "OpenCode connection applied",
          message: lang === "zh" ? "OpenCode 连接配置已按审阅内容写入并验证。" : "The reviewed OpenCode connection configuration was written and verified.",
          persistent: true,
          details: `rollback_id=${receipt.rollback_id ?? "unavailable"} · revision=${shortHash(receipt.revision)} · before=${shortHash(receipt.before_sha256)} · after=${shortHash(receipt.after_sha256)} · verified=${receipt.verified}`,
          receipt: { requestId, rollbackId: receipt.rollback_id ?? undefined, targetId: "opencode" },
        });
        setAgents((current) => current.map((agent) => agent.id === "opencode" ? { ...agent, configured: true } : agent));
        setShowDiff(false);
        setSelected([]);
        setConnectionReview(null);
        await refreshRuntime();
        await runRouteVerification({ quiet: true });
      } else {
        addNotification({ dedupeKey: "opencode-apply-rejected", level: "error", title: lang === "zh" ? "OpenCode 连接未应用" : "OpenCode connection not applied", message: localizedReceipt, persistent: true, details: `revision=${shortHash(receipt.revision)} · verified=${receipt.verified}`, receipt: { requestId, rollbackId: receipt.rollback_id ?? undefined, targetId: "opencode" } });
        await previewOpenCodeConnection();
      }
    } catch (error) {
      const summary = lang === "zh" ? `OpenCode 配置事务失败：${String(error)}` : `OpenCode configuration transaction failed: ${String(error)}`;
      setNativeNotice(summary);
      addNotification({ dedupeKey: "opencode-apply-failed", level: "error", title: lang === "zh" ? "OpenCode 配置事务失败" : "OpenCode configuration failed", message: summary, receipt: { requestId, targetId: "opencode" } });
    } finally {
      setConnectionApplying(false);
      await refreshRuntime();
    }
  };

  const runRouteVerification = async ({ quiet = false }: { quiet?: boolean } = {}) => {
    setVerifying(true);
    if (!quiet) setNativeNotice("");
    try {
      const readiness = await invokeNative<ProductReadinessResult>("run_product_readiness_check");
      setReadinessResult(readiness);
      const result = await invokeNative<RouteVerification>("verify_agent_route_config");
      setRouteVerification(result);
      if (!quiet) {
        setNativeNotice(
          lang === "zh"
            ? `${result.summary} · ${result.passed}/${result.total} 项通过`
            : `${result.summary} · ${result.passed}/${result.total} checks passed`,
        );
      }
      await refreshRuntime();
    } catch (error) {
      const summary = error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh"
            ? "当前是浏览器预览，配置验证需要在 Tauri 桌面窗口中运行。"
            : "Configuration verification requires the Tauri desktop window.")
        : (lang === "zh" ? `配置验证失败：${String(error)}` : `Configuration verification failed: ${String(error)}`);
      setNativeNotice(summary);
      setRouteVerification(null);
    } finally {
      setVerifying(false);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <h1>{t("in.title")}</h1>
        <DataSourceBadge
          mode={hasNativeData ? "native" : isTauriRuntime() ? "unavailable" : "preview"}
          label={
            hasNativeData
              ? (lang === "zh" ? "桌面运行时" : "Desktop runtime")
              : isTauriRuntime()
                ? (lang === "zh" ? "等待本地运行数据" : "Waiting for local runtime data")
                : (lang === "zh" ? "浏览器预览" : "Browser preview")
          }
        />
      </div>
      <p className="-mt-3 text-sm text-muted-foreground">{t("in.subtitle")}</p>

      <Surface strong className="p-6">
        <div className="flex flex-col gap-5 md:flex-row md:items-center md:justify-between">
          <div className="flex items-center gap-4">
            <div
              className="flex size-12 items-center justify-center rounded-2xl text-white shadow-lg"
              style={{ background: servicePresentation.isOnline ? "var(--accent-500)" : "var(--muted-foreground)" }}
            >
              <Server className="size-6" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <span className="font-semibold">
                  {servicePresentation.phase === "running"
                    ? t("in.serviceRunning")
                    : servicePresentation.phase === "installing"
                      ? t("in.installing")
                      : servicePresentation.phase === "loading"
                        ? (lang === "zh" ? "正在读取本地服务状态" : "Reading local service status")
                      : t("in.serviceStopped")}
                </span>
                <StatusBadge
                  status={servicePresentation.status}
                  label={servicePresentation.phase === "running"
                    ? t("status.running")
                    : servicePresentation.phase === "installing"
                      ? t("in.installing")
                      : servicePresentation.phase === "loading"
                        ? (lang === "zh" ? "读取中" : "Loading")
                      : t("status.stopped")}
                />
              </div>
              <div className="mt-1 text-sm text-muted-foreground">
                {serviceStatus?.pid ? `PID ${serviceStatus.pid} · ` : ""}{serviceAddress}
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {serviceOnline ? (
              <>
                <Button variant="outline" onClick={() => void refreshServiceStatus()} disabled={installing}>
                  <RefreshCw className="size-4" />
                  {lang === "zh" ? "重新检查" : "Re-check"}
                </Button>
                {serviceManaged && (
                  <Button
                    variant="outline"
                    onClick={() => setConfirmServiceStop(true)}
                    disabled={serviceAction === "stop"}
                  >
                    {serviceAction === "stop" ? <RefreshCw className="size-4 animate-spin" /> : <Server className="size-4" />}
                    {lang === "zh" ? "停止 d-token" : "Stop d-token"}
                  </Button>
                )}
                <Button
                  variant="outline"
                  onClick={() => void runRouteVerification()}
                  disabled={verifying}
                >
                  {verifying ? <RefreshCw className="size-4 animate-spin" /> : <ShieldCheck className="size-4" />}
                  {lang === "zh" ? "验证配置" : "Verify config"}
                </Button>
              </>
            ) : activeModels.length === 0 && onOpenAdvanced ? (
              <Button onClick={onOpenAdvanced} className="shadow-lg">
                <Settings2 className="size-4" />
                {lang === "zh" ? "先配置真实上游" : "Configure upstream first"}
              </Button>
            ) : (
              <Button onClick={runInstall} disabled={installing} className="shadow-lg">
                <Rocket className="size-4" />
                {installing ? t("in.installing") : t("in.installAndStart")}
              </Button>
            )}
          </div>
        </div>

        <div className="install-status-strip mt-5">
          <div>{runtimeLine}</div>
          <div>{transactionLine}</div>
        </div>

        {transactionLogs.length > 0 && (
          <details className="mt-4 rounded-md border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
            <summary className="cursor-pointer select-none font-medium text-foreground">
              {lang === "zh"
                ? `操作记录 · ${transactionLogs.length} 个阶段${transactionLogs.some((entry) => entry.level === "error") ? " · 有失败项" : " · 已完成"}`
                : `Action details · ${transactionLogs.length} stage(s)${transactionLogs.some((entry) => entry.level === "error") ? " · attention needed" : " · complete"}`}
            </summary>
            <div className="mt-2 max-h-32 space-y-1 overflow-auto font-mono text-[11px]">
              {transactionLogs.slice(-8).map((entry, index) => (
                <div key={`${entry.time}-${index}`} className={entry.level === "error" ? "text-rose-600 dark:text-rose-300" : entry.level === "warn" ? "text-amber-700 dark:text-amber-200" : ""}>
                  [{entry.time}] {entry.level}: {redactString(entry.message)}
                </div>
              ))}
            </div>
          </details>
        )}

        <AnimatePresence>
          {(installing || done) && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              className="mt-6 overflow-hidden border-t border-border/50 pt-6"
            >
              <StepProgress steps={steps} current={Math.min(stepIdx, steps.length - 1)} done={done} />
            </motion.div>
          )}
        </AnimatePresence>
      </Surface>

      <Surface className="p-6">
        <div className="mb-4 flex items-center justify-between">
          <h3 className="flex items-center gap-2">
            <Search className="size-5" />
            {t("in.detect")}
          </h3>
          <Button variant="outline" onClick={runDetect} disabled={detecting}>
            {detecting ? <RefreshCw className="size-4 animate-spin" /> : <Search className="size-4" />}
            {detecting ? t("in.detecting") : t("in.detect")}
          </Button>
        </div>

        {nativeNotice && (
          <div className="flat-inset mb-4 p-3 text-sm text-muted-foreground">{nativeNotice}</div>
        )}

        <div className="flat-inset mb-4 flex flex-wrap items-center justify-between gap-3 p-4">
          <div className="flex items-center gap-3">
            <Cpu className="size-5 text-primary" />
            <div>
              <div>{t("in.ccswitch")}</div>
              <div className="text-sm text-muted-foreground">{ccSwitchEvidence.detail}</div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <StatusBadge
              status={ccSwitchEvidence.verified ? "healthy" : "warning"}
              label={ccSwitchEvidence.label}
            />
            <Badge variant="secondary">{lang === "zh" ? "只读检测" : "Read-only detection"}</Badge>
          </div>
        </div>

        <div className="mb-3 text-sm text-muted-foreground">{t("in.selectAgents")}</div>
        <div className="divide-y divide-border/60">
          {agents.map((agent, index) => (
            <motion.div
              key={agent.id}
              initial={{ opacity: 0, x: -10 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: index * 0.05 }}
              data-flat-row
              className="flex flex-wrap items-center gap-3 py-3"
            >
              <Checkbox
                checked={selected.includes(agent.id)}
                disabled={!agent.installed || agent.configured}
                aria-label={`${lang === "zh" ? "选择" : "Select"} ${agent.name}`}
                onCheckedChange={() => toggle(agent.id)}
              />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span>{agent.name}</span>
                  {agent.version && <span className="text-xs text-muted-foreground">v{agent.version}</span>}
                  {agent.configured && (
                    <Badge variant="secondary" className="gap-1">
                      <CheckCircle2 className="size-3" />
                      {t("in.configured")}
                    </Badge>
                  )}
                </div>
                <div className="truncate text-xs text-muted-foreground">{agent.binaryPath ?? "—"}</div>
              </div>
              <div className="hidden text-xs text-muted-foreground sm:block">
                {t("in.configMethod")}: {lang === "zh" ? agent.configMethod : agent.configMethodEn}
              </div>
              <StatusBadge
                status={agent.installed ? "healthy" : "warning"}
                label={agent.installed ? t("in.installed") : t("in.notFound")}
              />
            </motion.div>
          ))}
        </div>

        <div className="mt-4 flex justify-end">
          <Button disabled={selected.length === 0 || !serviceOnline || !serviceEndpoint} onClick={() => setShowDiff(true)}>
            <ShieldCheck className="size-4" />
            {lang === "zh" ? "查看连接步骤" : "View connection steps"} {selected.length > 0 && `(${selected.length})`}
          </Button>
        </div>
      </Surface>

      <Surface className="p-6">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h3 className="flex items-center gap-2">
              <Route className="size-5" />
              {lang === "zh" ? "连接 AI 编码工具" : "Connect coding agents"}
            </h3>
            <p className="mt-1 text-sm text-muted-foreground">
              {lang === "zh"
                ? "先应用真实上游路由，再通过可审阅、可验证、可回退的事务连接 OpenCode；其他工具继续使用手动连接。"
                : "Apply a real upstream route, then connect OpenCode through a reviewed, verified, reversible transaction. Other agents remain manual."}
            </p>
            <label className="mt-3 block max-w-xl text-xs text-muted-foreground">
              {lang === "zh" ? "选择已发现的上游（不会自动选择）" : "Select a discovered upstream (never automatic)"}
              <select
                className="mt-1 w-full rounded-md border border-border bg-background px-2 py-2 text-sm text-foreground"
                value={selectedCandidate}
                onChange={(event) => {
                  setSelectedCandidate(event.target.value);
                  const option = candidateOptions.find((item) => item.key === event.target.value);
                  setSelectedModel(option?.model ?? "");
                  setConnectionReview(null);
                }}
              >
                <option value="">{lang === "zh" ? "请选择 provider / model" : "Select provider / model"}</option>
                {candidateOptions.map((item) => <option key={item.key} value={item.key}>{item.label}</option>)}
              </select>
              <span className="mt-1 block">
                {upstreamDiscovery
                  ? `${upstreamDiscovery.candidates.length} ${lang === "zh" ? "个只读候选" : "read-only candidates"} · ${upstreamDiscovery.freshness}`
                  : (lang === "zh" ? "等待只读扫描" : "Waiting for read-only discovery")}
              </span>
            </label>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="outline"
              onClick={() => void runRouteVerification()}
              disabled={verifying}
            >
              {verifying ? <RefreshCw className="size-4 animate-spin" /> : <ShieldCheck className="size-4" />}
              {lang === "zh" ? "验证配置" : "Verify config"}
            </Button>
            <Button
              onClick={() => {
                if (primaryAgent?.installed) {
                  setSelected((current) => (current.includes(primaryAgent.id) ? current : [...current, primaryAgent.id]));
                  setShowDiff(true);
                } else {
                  void runDetect();
                }
              }}
              disabled={detecting || verifying || Boolean(primaryAgent?.installed && (!serviceOnline || !serviceEndpoint || !selectedCandidate))}
            >
              {detecting || verifying ? <RefreshCw className="size-4 animate-spin" /> : <Settings2 className="size-4" />}
              {primaryAgent?.installed
                ? (lang === "zh" ? `连接 ${primaryAgentName}` : `Connect ${primaryAgentName}`)
                : (lang === "zh" ? "检测工具" : "Detect agents")}
            </Button>
          </div>
        </div>

        <div className="grid gap-x-5 md:grid-cols-2 xl:grid-cols-4">
          <Metric label={lang === "zh" ? "主要工具" : "Primary agent"} value={primaryAgent?.installed ? `${primaryAgentName} · ${lang === "zh" ? "已检测" : "detected"}` : (lang === "zh" ? "尚未检测到" : "not detected")} />
          <Metric label={lang === "zh" ? "本地服务" : "Runtime"} value={runtimeSummary.runtimeLabel} />
          <Metric label={lang === "zh" ? "服务管理" : "Service lifecycle"} value={serviceStatus ? `${serviceStatus.state} · ${serviceStatus.managed ? (lang === "zh" ? "由应用管理" : "managed") : (lang === "zh" ? "仅检测" : "observed")}` : (lang === "zh" ? "等待桌面状态" : "desktop status pending")} />
          <Metric label={lang === "zh" ? "当前路由" : "Route data"} value={runtimeSummary.routeLabel} />
        </div>

        <div className="mt-4 divide-y divide-border/60">
          {[
            { title: lang === "zh" ? "1. 配置真实上游" : "1. Configure an upstream", detail: lang === "zh" ? "在高级安装中保存凭据引用并审阅、应用路由。" : "Store a credential reference and review/apply the route in Advanced setup." },
            { title: lang === "zh" ? "2. 启动本地服务" : "2. Start local service", detail: lang === "zh" ? "确认 d-token 自有服务在 127.0.0.1 上可用。" : "Confirm the d-token-owned service is available on 127.0.0.1." },
            { title: lang === "zh" ? "3. 审阅并连接工具" : "3. Review and connect", detail: lang === "zh" ? "OpenCode 显示脱敏 diff 和双 hash 后才允许确认；其他工具显示手动字段。" : "OpenCode requires a redacted diff and both hashes before confirmation; other agents show manual fields." },
            { title: lang === "zh" ? "4. 验证真实流量" : "4. Verify real traffic", detail: lang === "zh" ? "发送一次真实请求，再从概览、日志和 Doctor 核对路由证据。" : "Send a real request, then verify routing evidence in Overview, Logs, and Doctor." },
          ].map((item) => (
            <div key={item.title} data-flat-row className="py-3">
              <div className="text-sm font-medium">{item.title}</div>
              <div className="mt-1 text-xs text-muted-foreground">{item.detail}</div>
            </div>
          ))}
        </div>

        <div className="install-status-strip mt-4">
          <div>&gt; {lang === "zh" ? "连接路径" : "route"} · {runtimeSummary.routePreview}</div>
          <div>&gt; {lang === "zh" ? "验证" : "verification"} · {routeVerification ? `${routeVerification.passed}/${routeVerification.total} · ${routeVerification.summary}` : runtimeSummary.verificationLine}</div>
        </div>

        {readinessResult && (
          <div className="mt-4">
            <div className="mb-2 text-sm font-medium">{lang === "zh" ? "产品就绪检查" : "Product readiness"}</div>
            <div className="divide-y divide-border/60">
              {readinessResult.checks.map((check) => (
                <div key={check.id} data-flat-row className="flex items-start justify-between gap-3 py-3">
                  <div className="min-w-0">
                    <div className="text-sm font-medium">{check.label}</div>
                    <div className="mt-0.5 break-words text-xs text-muted-foreground">{formatReadinessSummary(check.summary)}</div>
                  </div>
                  <StatusBadge status={check.ok ? "healthy" : "warning"} label={check.ok ? t("common.healthy") : t("common.warning")} />
                </div>
              ))}
            </div>
            <div className="mt-2 text-xs text-muted-foreground">{readinessResult.privacy}</div>
          </div>
        )}

        {routeVerification && (
          <div className="mt-4 divide-y divide-border/60">
            {routeVerification.checks.map((check) => (
              <div key={check.id} data-flat-row className="flex items-start justify-between gap-3 py-3">
                <div className="min-w-0">
                  <div className="text-sm font-medium">{check.label}</div>
                  <div className="mt-0.5 text-xs text-muted-foreground">{check.summary}</div>
                </div>
                <StatusBadge
                  status={check.ok ? "healthy" : "warning"}
                  label={check.ok ? t("common.healthy") : t("common.warning")}
                />
              </div>
            ))}
          </div>
        )}

        {nativeTools.length > 0 && (
          <div className="mt-4 flex flex-wrap gap-2">
            {nativeTools.map((tool) => (
              <Badge key={tool.id} variant={tool.installed ? "default" : "secondary"}>
                {tool.label}: {tool.installed ? tool.version ?? tool.path ?? (lang === "zh" ? "已安装" : "installed") : (lang === "zh" ? "未找到" : "missing")}
              </Badge>
            ))}
          </div>
        )}
      </Surface>

      <CapabilitySummary payload={payload} status={payloadStatus} />

      <Dialog open={showDiff} onOpenChange={(open) => { setShowDiff(open); if (!open) setConnectionReview(null); }}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{lang === "zh" ? "审阅连接变更" : "Review connection change"}</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            {selected.map((id) => {
              const agent = agents.find((item) => item.id === id)!;
              if (id === "opencode") {
                return (
                  <div key={id} className="flat-inset space-y-3 p-3 text-sm">
                    <div className="font-semibold">OpenCode → d-token</div>
                    <label className="block text-xs text-muted-foreground">
                      {lang === "zh" ? "上游模型（明确选择）" : "Upstream model (explicit selection)"}
                      <select
                        className="mt-1 w-full rounded-md border border-border bg-background px-2 py-2 text-sm text-foreground"
                        value={selectedModel}
                        onChange={(event) => { setSelectedModel(event.target.value); setConnectionReview(null); }}
                      >
                        <option value="">{lang === "zh" ? "请选择模型" : "Select a model"}</option>
                        {selectableModels.map((model) => <option key={model} value={model}>{model}</option>)}
                      </select>
                    </label>
                    {!connectionReview && (
                      <Button variant="outline" onClick={() => void previewOpenCodeConnection()} disabled={!selectedModel}>
                        <ShieldCheck className="size-4" />
                        {lang === "zh" ? "生成真实配置预览" : "Generate real config preview"}
                      </Button>
                    )}
                    {connectionReview && (
                      <div className="space-y-2">
                        <div className="text-xs text-muted-foreground">{connectionReview.summary}</div>
                        <div className="font-mono text-[11px] text-muted-foreground">
                          <div>{connectionReview.target}</div>
                          <div>revision {shortHash(connectionReview.revision)}</div>
                          <div>before {shortHash(connectionReview.before_sha256)} → after {shortHash(connectionReview.after_sha256)}</div>
                        </div>
                        {connectionReview.diff && <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded bg-muted/60 p-2 text-[11px]">{connectionReview.diff}</pre>}
                        <div className="text-[11px] text-muted-foreground">{connectionReview.privacy}</div>
                      </div>
                    )}
                  </div>
                );
              }
              return (
                <div key={id} className="flat-inset p-3 text-sm">
                  <div className="mb-1 font-semibold">{agent.name}</div>
                  <div className="text-muted-foreground">
                    {t("in.configMethod")}: {lang === "zh" ? agent.configMethod : agent.configMethodEn}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {lang === "zh"
                      ? "该工具当前仍使用手动字段；关闭窗口不会执行写入。"
                      : "This agent still uses manual fields; closing the dialog performs no write."}
                  </div>
                  <pre className="mt-2 overflow-x-auto rounded bg-muted/60 p-2 text-xs">
{connectionInstructions(agent, serviceEndpoint, lang)}
                  </pre>
                </div>
              );
            })}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowDiff(false)}>{t("common.cancel")}</Button>
            {selected.includes("opencode") ? (
              <Button onClick={() => void applyOpenCodeConnection()} disabled={connectionApplying || !connectionReview?.ok}>
                {connectionApplying ? <RefreshCw className="size-4 animate-spin" /> : <CheckCircle2 className="size-4" />}
                {lang === "zh" ? "确认并应用" : "Confirm and apply"}
              </Button>
            ) : (
              <Button onClick={() => void verifyAfterManualConnection()} disabled={verifying || !serviceOnline}>
                {verifying ? <RefreshCw className="size-4 animate-spin" /> : <CheckCircle2 className="size-4" />}
                {lang === "zh" ? "完成后验证" : "Verify after setup"}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <OperationDialog
        open={confirmServiceStop}
        onOpenChange={setConfirmServiceStop}
        title={lang === "zh" ? "停止 d-token 本地服务？" : "Stop the d-token local service?"}
        summary={lang === "zh" ? "停止后新的 Agent 请求将无法通过 d-token，直到服务重新启动。" : "New agent requests cannot route through d-token until the service is restarted."}
        impact={serviceStatus?.pid ? `PID ${serviceStatus.pid} · ${serviceAddress}` : serviceAddress}
        confirmLabel={lang === "zh" ? "确认停止" : "Stop service"}
        cancelLabel={t("common.cancel")}
        risk="warning"
        pending={serviceAction === "stop"}
        onConfirm={async () => {
          await runServiceAction("stop");
          setConfirmServiceStop(false);
        }}
      />
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div data-flat-row className="border-t border-border/60 py-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 truncate font-mono text-sm">{value}</div>
    </div>
  );
}

export function desktopInstallRoot(userAgent: string) {
  return /windows/i.test(userAgent)
    ? "%LOCALAPPDATA%\\d-token"
    : "~/Library/Application Support/d-token";
}

type InstallAgent = {
  id: string;
  name: string;
  installed: boolean;
  binaryPath: string | null;
  version: string | null;
  configMethod: string;
  configMethodEn: string;
  configured: boolean;
  preferredProtocol: AgentDescriptor["preferredProtocol"];
};

function toInstallAgent(agent: AgentDescriptor): InstallAgent {
  const methods = agent.supportedConfigMethods.map((method) => {
    if (method === "config-file") return "配置文件";
    if (method === "env-var") return "环境变量";
    if (method === "local-proxy") return "本地服务地址";
    if (method === "manual-prompt") return "手动设置";
    return "工具集成";
  });
  return {
    id: agent.id,
    name: agent.displayName,
    installed: false,
    binaryPath: null,
    version: null,
    configMethod: Array.from(new Set(methods)).join(" / "),
    configMethodEn: agent.supportedConfigMethods.join(" / "),
    configured: false,
    preferredProtocol: agent.preferredProtocol,
  };
}

function connectionInstructions(agent: InstallAgent, serviceEndpoint: string, lang: "zh" | "en") {
  const endpoint = serviceEndpoint.replace(/\/$/, "");
  const openAiEndpoint = endpoint.endsWith("/v1") ? endpoint : `${endpoint}/v1`;
  if (agent.preferredProtocol === "anthropic-compatible") {
    return `ANTHROPIC_BASE_URL = "${endpoint}"\nNO_PROXY = "localhost,127.0.0.1"`;
  }
  if (agent.preferredProtocol === "openai-compatible") {
    return `Base URL = "${openAiEndpoint}"\nNO_PROXY = "localhost,127.0.0.1"`;
  }
  return lang === "zh"
    ? `d-token 本地服务 = "${endpoint}"\n协议 = "请按工具支持的协议手动选择"`
    : `d-token local service = "${endpoint}"\nProtocol = "Select a protocol supported by the agent"`;
}

function deriveCcSwitchEvidence(payload: Record<string, any> | null, status: string, lang: "zh" | "en") {
  const zh = lang === "zh";
  const evidence = payload?.cc_switch_sync ?? payload?.ccSwitchSync;
  const item = evidence && typeof evidence === "object" && !Array.isArray(evidence) ? evidence : {};
  const state = safeRuntimeText(item.status ?? item.state, "").toLowerCase();
  const verified = item.verified === true || item.evidence_verified === true;
  const providers = Array.isArray(payload?.providers) ? payload.providers : [];
  const detected = item.detected === true || providers.some((provider: any) => {
    const kind = safeRuntimeText(provider?.kind, "").toLowerCase();
    return kind === "cc_switch" || kind === "cc-switch" || kind === "ccswitch";
  });
  const detail = [item.profile, item.upstream, item.connection_path]
    .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    .join(" · ");

  if (status === "runtime" && verified && (state === "synced" || state === "in_sync")) {
    return { verified: true, label: zh ? "已验证同步" : "Sync verified", detail: detail || (zh ? "证据来自 d-token 本地检测" : "Evidence from local d-token detection") };
  }
  if (status === "runtime" && (detected || Object.keys(item).length > 0)) {
    return { verified: false, label: zh ? "已检测，尚未验证" : "Detected, not verified", detail: detail || (zh ? "未确认当前上游是否一致" : "The active upstream has not been confirmed") };
  }
  return {
    verified: false,
    label: status === "browser-preview"
      ? (zh ? "预览中不执行检测" : "Detection disabled in preview")
      : (zh ? "尚无检测证据" : "No detection evidence yet"),
    detail: zh ? "不会自动读取、启动或改写第三方服务" : "Third-party services are never read, started, or modified automatically",
  };
}


function formatReadinessSummary(value: unknown) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (value == null) return "?";
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function installRuntimeSummary(payload: Record<string, any> | null, status: string, updatedAt: Date | null, lang: "zh" | "en") {
  const zh = lang === "zh";
  const routes = Array.isArray(payload?.routes) ? payload.routes : [];
  const providers = Array.isArray(payload?.providers) ? payload.providers : [];
  const requestLogs = Array.isArray(payload?.request_logs) ? payload.request_logs : [];
  const activeConfig = payload?.runtime_active_config?.config ?? payload?.active_config?.config ?? payload?.desktop_active_config?.config ?? null;
  const activeRoutes = Array.isArray(activeConfig?.routes) ? activeConfig.routes : routes;
  const activeProviders = Array.isArray(activeConfig?.providers) ? activeConfig.providers : providers;
  const route = activeRoutes[0] ?? {};
  const provider = activeProviders[0] ?? {};
  const routeName = safeRuntimeText(route.name, safeRuntimeText(route.id, zh ? "等待配置路由" : "waiting for route"));
  const providerName = safeRuntimeText(provider.name, safeRuntimeText(provider.id, zh ? "等待配置上游" : "upstream pending"));
  const source = safeRuntimeText(payload?.desktop_data_source, status);

  if (status === "runtime") {
    return {
      runtimeLabel: `${source} · ${requestLogs.length} ${zh ? "条请求" : "requests"}`,
      routeLabel: `${routeName} -> ${providerName}`,
      routePreview: `${zh ? "AI 编码工具" : "agent"} -> d-token -> ${routeName} -> ${providerName}`,
      verificationLine: zh
        ? `运行数据${updatedAt ? `已于 ${updatedAt.toLocaleTimeString()} 更新` : "已更新"}；可见 ${requestLogs.length} 条请求记录`
        : `Runtime data refreshed${updatedAt ? ` at ${updatedAt.toLocaleTimeString()}` : ""}; ${requestLogs.length} request record(s) visible`,
    };
  }

  return {
    runtimeLabel: status === "browser-preview" ? (zh ? "浏览器预览，不执行本机操作" : "browser preview; local actions disabled") : (zh ? "本地服务暂不可用" : status),
    routeLabel: "—",
    routePreview: zh ? "AI 编码工具 -> d-token 本地服务 -> 路由适配 -> 真实上游" : "agent -> d-token local service -> route adapter -> upstream",
    verificationLine: zh ? "连接后发送一次真实请求，再由概览、日志和 Doctor 核对" : "Send a real request, then confirm it in Overview, Logs, and Doctor",
  };
}

function safeRuntimeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}

function activeRouteModels(payload: Record<string, any> | null): string[] {
  const config = payload?.runtime_active_config?.config
    ?? payload?.active_config?.config
    ?? payload?.desktop_active_config?.config
    ?? payload;
  const providers = Array.isArray(config?.providers) ? [...config.providers] : [];
  const summarizedProvider = payload?.active_config?.provider ?? payload?.desktop_active_config?.provider;
  if (summarizedProvider && typeof summarizedProvider === "object") providers.push(summarizedProvider);
  return Array.from(new Set(providers.flatMap((provider: any) =>
    Array.isArray(provider?.models)
      ? provider.models.filter((model: unknown): model is string => typeof model === "string" && model.length > 0)
      : [],
  )));
}

function shortHash(value?: string | null) {
  return value ? value.slice(0, 12) : "—";
}

type OpenCodeConnectionReview = {
  ok: boolean;
  state: string;
  source: string;
  freshness: string;
  target: string;
  model: string;
  listener: string;
  transaction: string;
  revision?: string | null;
  before_sha256?: string | null;
  after_sha256?: string | null;
  diff?: string | null;
  summary: string;
  privacy: string;
};

type OpenCodeConnectionReceipt = {
  ok: boolean;
  state: string;
  source: string;
  freshness: string;
  target: string;
  model: string;
  listener: string;
  revision?: string | null;
  before_sha256?: string | null;
  after_sha256?: string | null;
  rollback_id?: string | null;
  verified: boolean;
  summary: string;
  privacy: string;
};

type UpstreamCandidate = {
  id: string;
  name: string;
  protocol: string;
  base_url?: string | null;
  models: string[];
  source: string;
  freshness: string;
  auth_mode: string;
  active: boolean;
  port?: number | null;
  state: string;
};

type UpstreamDiscovery = {
  source: string;
  freshness: string;
  state: string;
  candidates: UpstreamCandidate[];
  selection_required: boolean;
  writes_performed: false;
  privacy: string;
};

function flattenCandidates(discovery: UpstreamDiscovery | null) {
  if (!discovery) return [];
  return discovery.candidates.flatMap((candidate) => candidate.models.map((model) => ({
    key: `${candidate.source}::${candidate.id}::${model}`,
    model,
    label: `${candidate.name} / ${model} · ${candidate.protocol} · ${candidate.auth_mode} · ${candidate.source}`,
  })));
}

type ProductReadinessResult = {
  ok: boolean;
  mode: "product-readiness-check";
  passed: number;
  total: number;
  summary: string;
  privacy: string;
  external_services: string;
  checks: Array<{
    id: string;
    label: string;
    ok: boolean;
    summary: unknown;
    detail?: unknown;
  }>;
};

type RouteVerification = {
  ok: boolean;
  mode: "agent-route-verification";
  passed: number;
  total: number;
  summary: string;
  route?: unknown;
  provider?: unknown;
  service_url: string;
  privacy: string;
  checks: Array<{
    id: string;
    label: string;
    ok: boolean;
    summary: string;
    detail?: unknown;
  }>;
};

type DtokenServiceStatus = {
  ok: boolean;
  mode: "dtoken-service-lifecycle";
  state: "running" | "unmanaged_running" | "stopped";
  managed: boolean;
  pid?: number | null;
  pid_running?: boolean;
  service_url: string;
  manifest_path: string;
  log_path: string;
  health?: unknown;
  summary: string;
  privacy: string;
};
