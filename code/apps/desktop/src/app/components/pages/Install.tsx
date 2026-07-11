import { useEffect, useMemo, useState } from "react";
import { motion, AnimatePresence } from "motion/react";
import { Rocket, RefreshCw, Search, ShieldCheck, Cpu, Server, CheckCircle2, Route, Settings2 } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StepProgress, type Step } from "../common/StepProgress";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { CapabilitySummary } from "../common/CapabilitySummary";
import { Button } from "../ui/button";
import { Checkbox } from "../ui/checkbox";
import { Badge } from "../ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "../ui/dialog";
import { useI18n, useT } from "../../lib/i18n";
import { AGENT_REGISTRY, type AgentDescriptor } from "../../lib/agents/registry";
import { invokeNative, isTauriRuntime, type NativeActionLog, type NativeActionResult, type NativeToolCheck } from "../../lib/native";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";

export function assertNativeActionSucceeded(result: NativeActionResult): void {
  if (!result.ok) {
    throw new Error(result.summary || "本地操作未完成");
  }
}

export function deriveServicePresentation(serviceOnline: boolean, installing: boolean) {
  if (serviceOnline) {
    return { isOnline: true, status: "healthy" as const, phase: "running" as const };
  }
  if (installing) {
    return { isOnline: false, status: "warning" as const, phase: "installing" as const };
  }
  return { isOnline: false, status: "error" as const, phase: "stopped" as const };
}

export function Install() {
  const t = useT();
  const { lang } = useI18n();
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
  const [nativeTools, setNativeTools] = useState<NativeToolCheck[]>([]);
  const [nativeNotice, setNativeNotice] = useState<string>("");
  const [transactionLogs, setTransactionLogs] = useState<NativeActionLog[]>([]);
  const { payload, status: payloadStatus, updatedAt, refresh: refreshRuntime } = useRuntimeDashboard();

  const done = stepIdx >= steps.length;
  const serviceOnline = payloadStatus === "runtime" || serviceStatus?.state === "running" || serviceStatus?.state === "unmanaged_running";
  const serviceManaged = serviceStatus?.state === "running" && serviceStatus.managed;
  const servicePresentation = deriveServicePresentation(serviceOnline, installing);
  const hasNativeData = isTauriRuntime() && (nativeTools.length > 0 || transactionLogs.length > 0 || Boolean(routeVerification) || payloadStatus === "runtime");
  const primaryAgent = agents.find((agent) => agent.id === "opencode") ?? agents[0];
  const primaryAgentName = primaryAgent?.name ?? "Agent";
  const runtimeSummary = useMemo(() => installRuntimeSummary(payload, payloadStatus, updatedAt, lang), [payload, payloadStatus, updatedAt, lang]);
  const ccSwitchEvidence = useMemo(() => deriveCcSwitchEvidence(payload, payloadStatus, lang), [payload, payloadStatus, lang]);
  const serviceEndpoint = serviceStatus?.service_url || safeRuntimeText(payload?.desktop_service_url, "http://127.0.0.1:18001");
  const serviceAddress = serviceEndpoint.replace(/^https?:\/\//, "");

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
    try {
      const status = await invokeNative<DtokenServiceStatus>("get_dtoken_service_status");
      setServiceStatus(status);
      await refreshRuntime();
    } catch {
      setServiceStatus(null);
    }
  };

  useEffect(() => {
    void refreshServiceStatus();
    const timer = window.setInterval(() => {
      void refreshServiceStatus();
    }, 3000);
    return () => window.clearInterval(timer);
  }, []);

  const runServiceAction = async (action: "start" | "stop") => {
    setServiceAction(action);
    setNativeNotice("");
    try {
      const result = await invokeNative<NativeActionResult>(action === "start" ? "start_dtoken_service" : "stop_dtoken_service");
      setTransactionLogs(result.logs);
      assertNativeActionSucceeded(result);
      setNativeNotice(result.summary);
      applyToolChecks(result.tools);
      await refreshServiceStatus();
    } catch (error) {
      const summary = error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，服务生命周期需要在 Tauri 桌面窗口中运行。"
        : `服务${action === "start" ? "启动" : "停止"}失败：${String(error)}`;
      setNativeNotice(summary);
      setTransactionLogs([{ time: new Date().toLocaleTimeString(), level: "error", message: summary }]);
    } finally {
      setServiceAction(null);
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
      setNativeNotice(result.summary);
      setStepIdx(steps.length);
      await refreshServiceStatus();
    } catch (error) {
      const summary = error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，真实安装事务需要在 Tauri 桌面窗口中运行。"
        : `安装事务失败：${String(error)}`;
      setNativeNotice(summary);
      setTransactionLogs([{ time: new Date().toLocaleTimeString(), level: "error", message: summary }]);
      setStepIdx(-1);
    } finally {
      setInstalling(false);
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
      await refreshRuntime();
    } catch (error) {
      const message = error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，真实检测需要在 Tauri 桌面窗口中运行。"
        : `检测失败：${String(error)}`;
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
        ? "当前是浏览器预览，真实配置验证需要在 Tauri 桌面窗口中运行。"
        : `配置验证失败：${String(error)}`;
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

      <GlassCard strong className="p-6">
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
                      : t("in.serviceStopped")}
                </span>
                <StatusBadge
                  status={servicePresentation.status}
                  label={servicePresentation.phase === "running"
                    ? t("status.running")
                    : servicePresentation.phase === "installing"
                      ? t("in.installing")
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
                    onClick={() => void runServiceAction("stop")}
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
          <div className="mt-4 max-h-36 overflow-auto rounded-md bg-black/70 p-3 font-mono text-[11px] text-white/80">
            {transactionLogs.slice(-8).map((entry, index) => (
              <div key={`${entry.time}-${index}`} className={entry.level === "error" ? "text-rose-300" : entry.level === "warn" ? "text-amber-200" : ""}>
                [{entry.time}] {entry.level}: {entry.message}
              </div>
            ))}
          </div>
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
      </GlassCard>

      <GlassCard className="p-6">
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
          <div className="glass-inset mb-4 p-3 text-sm text-muted-foreground">{nativeNotice}</div>
        )}

        <div className="glass-inset mb-4 flex flex-wrap items-center justify-between gap-3 p-4">
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
        <div className="space-y-2">
          {agents.map((agent, index) => (
            <motion.div
              key={agent.id}
              initial={{ opacity: 0, x: -10 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: index * 0.05 }}
              className="glass-inset flex flex-wrap items-center gap-3 p-3"
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
          <Button disabled={selected.length === 0} onClick={() => setShowDiff(true)}>
            <ShieldCheck className="size-4" />
            {lang === "zh" ? "查看连接步骤" : "View connection steps"} {selected.length > 0 && `(${selected.length})`}
          </Button>
        </div>
      </GlassCard>

      <GlassCard className="p-6">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h3 className="flex items-center gap-2">
              <Route className="size-5" />
              {lang === "zh" ? "连接 AI 编码工具" : "Connect coding agents"}
            </h3>
            <p className="mt-1 text-sm text-muted-foreground">
              {lang === "zh"
                ? "先在高级设置中配置真实上游，再把所选工具指向 d-token 本地服务；本页不会代替你修改第三方配置。"
                : "Configure a real upstream first, then point the selected tool to the local d-token service. This page does not modify third-party settings for you."}
            </p>
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
              disabled={detecting || verifying}
            >
              {detecting || verifying ? <RefreshCw className="size-4 animate-spin" /> : <Settings2 className="size-4" />}
              {primaryAgent?.installed
                ? (lang === "zh" ? `连接 ${primaryAgentName}` : `Connect ${primaryAgentName}`)
                : (lang === "zh" ? "检测工具" : "Detect agents")}
            </Button>
          </div>
        </div>

        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
          <Metric label={lang === "zh" ? "主要工具" : "Primary agent"} value={primaryAgent?.installed ? `${primaryAgentName} · ${lang === "zh" ? "已检测" : "detected"}` : (lang === "zh" ? "尚未检测到" : "not detected")} />
          <Metric label={lang === "zh" ? "本地服务" : "Runtime"} value={runtimeSummary.runtimeLabel} />
          <Metric label={lang === "zh" ? "服务管理" : "Service lifecycle"} value={serviceStatus ? `${serviceStatus.state} · ${serviceStatus.managed ? (lang === "zh" ? "由应用管理" : "managed") : (lang === "zh" ? "仅检测" : "observed")}` : (lang === "zh" ? "等待桌面状态" : "desktop status pending")} />
          <Metric label={lang === "zh" ? "当前路由" : "Route data"} value={runtimeSummary.routeLabel} />
        </div>

        <div className="mt-4 grid gap-3 lg:grid-cols-3">
          {[
            { title: lang === "zh" ? "1. 启动本地服务" : "1. Start local service", detail: lang === "zh" ? "确认 d-token 自有服务在 127.0.0.1 上可用。" : "Confirm the d-token-owned service is available on 127.0.0.1." },
            { title: lang === "zh" ? "2. 手动连接工具" : "2. Connect the agent", detail: lang === "zh" ? "查看连接字段，在对应工具中手动填写；d-token 不会静默改写。" : "Review the fields and enter them in the selected tool; d-token never rewrites them silently." },
            { title: lang === "zh" ? "3. 验证真实流量" : "3. Verify real traffic", detail: lang === "zh" ? "发送一次真实请求，再从概览、日志和 Doctor 核对路由证据。" : "Send one real request, then verify routing evidence in Overview, Logs, and Doctor." },
          ].map((item) => (
            <div key={item.title} className="glass-inset p-3">
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
            <div className="grid gap-2 md:grid-cols-2">
              {readinessResult.checks.map((check) => (
                <div key={check.id} className="glass-inset flex items-start justify-between gap-3 p-3">
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
          <div className="mt-4 grid gap-2 md:grid-cols-2">
            {routeVerification.checks.map((check) => (
              <div key={check.id} className="glass-inset flex items-start justify-between gap-3 p-3">
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
      </GlassCard>

      <CapabilitySummary payload={payload} status={payloadStatus} />

      <Dialog open={showDiff} onOpenChange={setShowDiff}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{lang === "zh" ? "连接字段（不会自动执行）" : "Connection fields (not executed automatically)"}</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            {selected.map((id) => {
              const agent = agents.find((item) => item.id === id)!;
              return (
                <div key={id} className="glass-inset p-3 text-sm">
                  <div className="mb-1 font-semibold">{agent.name}</div>
                  <div className="text-muted-foreground">
                    {t("in.configMethod")}: {lang === "zh" ? agent.configMethod : agent.configMethodEn}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {lang === "zh"
                      ? "请在该工具自己的设置中手动填写。关闭窗口或点击验证都不会写入第三方配置。"
                      : "Enter these values in the tool's own settings. Closing or verifying does not write third-party configuration."}
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
            <Button onClick={() => void verifyAfterManualConnection()} disabled={verifying || !serviceOnline}>
              {verifying ? <RefreshCw className="size-4 animate-spin" /> : <CheckCircle2 className="size-4" />}
              {lang === "zh" ? "完成后验证" : "Verify after setup"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="glass-inset p-3">
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
  configured: false;
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
  const serviceUrl = safeRuntimeText(payload?.desktop_service_url, "http://127.0.0.1:18001");
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
    routeLabel: `${serviceUrl}/v1`,
    routePreview: zh ? "AI 编码工具 -> d-token 本地服务 -> 路由适配 -> 真实上游" : "agent -> d-token local service -> route adapter -> upstream",
    verificationLine: zh ? "连接后发送一次真实请求，再由概览、日志和 Doctor 核对" : "Send a real request, then confirm it in Overview, Logs, and Doctor",
  };
}

function safeRuntimeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
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
