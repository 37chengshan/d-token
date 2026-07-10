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
import { service, agents as agentData, ccSwitch, type Agent } from "../../lib/fixtures";
import { invokeNative, type NativeActionLog, type NativeActionResult, type NativeToolCheck } from "../../lib/native";
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
  const [agents, setAgents] = useState<Agent[]>(agentData);
  const [selected, setSelected] = useState<string[]>([]);
  const [showDiff, setShowDiff] = useState(false);
  const [configuring, setConfiguring] = useState(false);
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
  const hasNativeData = nativeTools.length > 0 || transactionLogs.length > 0 || Boolean(nativeNotice) || Boolean(routeVerification) || payloadStatus === "runtime";
  const primaryAgent = agents.find((agent) => agent.id === "opencode") ?? agents[0];
  const primaryAgentName = primaryAgent?.name ?? "Agent";
  const runtimeSummary = useMemo(() => installRuntimeSummary(payload, payloadStatus, updatedAt), [payload, payloadStatus, updatedAt]);

  const transactionLine = useMemo(() => {
    if (installing) {
      const current = steps[Math.min(stepIdx, steps.length - 1)]?.key ?? "scan";
      return `> transaction ${current} · scan -> plan -> apply -> verify -> doctor`;
    }
    if (done) return "> transaction complete · d-token rollback point recorded when enabled";
    if (!serviceOnline) return "> transaction idle · waiting for d-token local service";
    return "> transaction ready · d-token local service observable";
  }, [done, installing, serviceOnline, stepIdx, steps]);

  const runtimeLine = serviceOnline
    ? `> runtime 127.0.0.1:18001 ${serviceManaged ? "managed" : "observable"} - agents connect through d-token route adapter`
    : `> runtime 127.0.0.1:18001 unavailable - start/connect then verify configuration`;

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
          install_root: "%LOCALAPPDATA%\\d-token",
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
      setNativeNotice(result.summary);
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

  const confirmConfigure = async () => {
    setConfiguring(true);
    setNativeNotice("");
    try {
      const selectedAgents = agents.filter((agent) => selected.includes(agent.id));
      const logs: NativeActionLog[] = [];
      let tools: NativeToolCheck[] = [];
      for (const agent of selectedAgents) {
        const routeId = `${agent.id}-dtoken-local`;
        const draft = await invokeNative<NativeActionResult>("save_route_profile_draft", {
          request: {
            route_id: routeId,
            route_name: `${agent.name} d-token local route`,
            provider_id: `${agent.id}-configured-upstream`,
            provider_name: `${agent.name} configured upstream`,
            provider_kind: "custom",
            protocol: agent.id === "opencode" ? "openai" : "anthropic",
            base_url: "http://127.0.0.1:18001/v1",
            api_key_ref: null,
            models: agent.id === "opencode" ? ["dtoken/deepseek-v4-flash-free"] : ["dtoken/default"],
            chain_mode: "direct",
            compression_policy: "balanced",
            local_port: 18001,
            agent: agent.name,
          },
        });
        assertNativeActionSucceeded(draft);
        logs.push(...draft.logs);
        tools = draft.tools;
        const applied = await invokeNative<NativeActionResult>("apply_route_profile_draft", {
          request: { route_id: routeId },
        });
        assertNativeActionSucceeded(applied);
        logs.push(...applied.logs);
        tools = applied.tools;
      }
      if (tools.length > 0) {
        applyToolChecks(tools);
      }
      setTransactionLogs(logs);
      setNativeNotice(
        lang === "zh"
          ? `已保存并应用 ${selectedAgents.length} 个 d-token 路由配置，外部 agent 配置未被直接修改。`
          : `Saved and applied ${selectedAgents.length} d-token route profile(s); external agent configs were not modified directly.`,
      );
      await refreshRuntime();
      await runRouteVerification({ quiet: true });
      const configuredIds = selectedAgents.map((agent) => agent.id);
      setAgents((current) =>
        current.map((agent) =>
          configuredIds.includes(agent.id) ? { ...agent, configured: true } : agent,
        ),
      );
      setSelected([]);
      setShowDiff(false);
    } catch (error) {
      const summary = error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，无法保存 agent 配置草案。"
        : `配置保存失败：${String(error)}`;
      setNativeNotice(summary);
      setTransactionLogs([{ time: new Date().toLocaleTimeString(), level: "error", message: summary }]);
    } finally {
      setConfiguring(false);
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
          mode={hasNativeData ? "native" : "preview"}
          label={hasNativeData ? (lang === "zh" ? "桌面运行时" : "Desktop native") : (lang === "zh" ? "浏览器预览" : "Browser preview")}
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
                {t("in.version")} {service.version} · {t("in.port")} {service.port} · {service.bindHost}
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
              <div className="text-sm text-muted-foreground">:{ccSwitch.port}</div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <StatusBadge
              status={ccSwitch.detected ? "healthy" : "error"}
              label={ccSwitch.detected ? t("in.installed") : t("in.notFound")}
            />
            <Badge variant={ccSwitch.proxyEnabled ? "default" : "secondary"}>
              {ccSwitch.proxyEnabled ? t("in.ccProxyOn") : t("in.ccProxyOff")}
            </Badge>
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
            {t("in.configure")} {selected.length > 0 && `(${selected.length})`}
          </Button>
        </div>
      </GlassCard>

      <GlassCard className="p-6">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h3 className="flex items-center gap-2">
              <Route className="size-5" />
              Agent connect
            </h3>
            <p className="mt-1 text-sm text-muted-foreground">
              Connect any supported coding agent through the shared d-token service, route adapter, and dashboard model.
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
                if (primaryAgent?.installed && !primaryAgent.configured) {
                  setSelected((current) => (current.includes(primaryAgent.id) ? current : [...current, primaryAgent.id]));
                  setShowDiff(true);
                } else {
                  void runDetect();
                }
              }}
              disabled={detecting || configuring}
            >
              {detecting || configuring ? <RefreshCw className="size-4 animate-spin" /> : <Settings2 className="size-4" />}
              {primaryAgent?.installed ? `Configure ${primaryAgentName}` : "Detect agents"}
            </Button>
          </div>
        </div>

        <div className="grid gap-3 md:grid-cols-3">
          <Metric label="Primary agent" value={primaryAgent?.installed ? `${primaryAgentName} · ${primaryAgent.configured ? "connected" : "ready to configure"}` : "not detected"} />
          <Metric label="Runtime" value={runtimeSummary.runtimeLabel} />
          <Metric label="Service lifecycle" value={serviceStatus ? `${serviceStatus.state} · ${serviceStatus.managed ? "managed" : "not managed"}` : "desktop status pending"} />
          <Metric label="Route data" value={runtimeSummary.routeLabel} />
        </div>

        <div className="mt-4 grid gap-3 lg:grid-cols-3">
          {[
            { title: "1. Connect runtime", detail: "Use the d-token-owned runtime on 127.0.0.1 only, then verify it is observable." },
            { title: "2. Connect agent", detail: "Preview the diff, back up the original config, and apply only the route fields d-token needs." },
            { title: "3. Observe results", detail: "Requests, compression, route decisions, and errors flow into Dashboard, Logs, and Doctor." },
          ].map((item) => (
            <div key={item.title} className="glass-inset p-3">
              <div className="text-sm font-medium">{item.title}</div>
              <div className="mt-1 text-xs text-muted-foreground">{item.detail}</div>
            </div>
          ))}
        </div>

        <div className="install-status-strip mt-4">
          <div>&gt; route preview - {runtimeSummary.routePreview}</div>
          <div>&gt; verification - {routeVerification ? `${routeVerification.passed}/${routeVerification.total} checks · ${routeVerification.summary}` : runtimeSummary.verificationLine}</div>
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
                {tool.label}: {tool.installed ? tool.version ?? tool.path ?? "installed" : "missing"}
              </Badge>
            ))}
          </div>
        )}
      </GlassCard>

      <CapabilitySummary payload={payload} status={payloadStatus} />

      <Dialog open={showDiff} onOpenChange={setShowDiff}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("in.previewDiff")}</DialogTitle>
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
                  <pre className="mt-2 overflow-x-auto rounded bg-muted/60 p-2 text-xs">
{`+ ANTHROPIC_BASE_URL="http://127.0.0.1:${service.port}"
+ NO_PROXY="localhost,127.0.0.1"`}
                  </pre>
                </div>
              );
            })}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowDiff(false)}>{t("common.cancel")}</Button>
            <Button onClick={confirmConfigure} disabled={configuring}>
              {configuring ? <RefreshCw className="size-4 animate-spin" /> : <CheckCircle2 className="size-4" />}
              {configuring ? t("in.configuring") : t("common.confirm")}
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

function installRuntimeSummary(payload: Record<string, any> | null, status: string, updatedAt: Date | null) {
  const routes = Array.isArray(payload?.routes) ? payload.routes : [];
  const providers = Array.isArray(payload?.providers) ? payload.providers : [];
  const requestLogs = Array.isArray(payload?.request_logs) ? payload.request_logs : [];
  const activeConfig = payload?.runtime_active_config?.config ?? payload?.active_config?.config ?? payload?.desktop_active_config?.config ?? null;
  const activeRoutes = Array.isArray(activeConfig?.routes) ? activeConfig.routes : routes;
  const activeProviders = Array.isArray(activeConfig?.providers) ? activeConfig.providers : providers;
  const route = activeRoutes[0] ?? {};
  const provider = activeProviders[0] ?? {};
  const routeName = safeRuntimeText(route.name, safeRuntimeText(route.id, "waiting for route"));
  const providerName = safeRuntimeText(provider.name, safeRuntimeText(provider.id, "upstream pending"));
  const serviceUrl = safeRuntimeText(payload?.desktop_service_url, "http://127.0.0.1:18001");
  const source = safeRuntimeText(payload?.desktop_data_source, status);

  if (status === "runtime") {
    return {
      runtimeLabel: `${source} · ${requestLogs.length} requests`,
      routeLabel: `${routeName} -> ${providerName}`,
      routePreview: `agent -> d-token local service -> ${routeName} -> ${providerName}`,
      verificationLine: `Dashboard payload refreshed${updatedAt ? ` at ${updatedAt.toLocaleTimeString()}` : ""}; ${requestLogs.length} request log(s) visible`,
    };
  }

  return {
    runtimeLabel: status === "browser-preview" ? "browser preview" : status,
    routeLabel: `${serviceUrl}/v1`,
    routePreview: "agent -> d-token local service -> route adapter -> upstream",
    verificationLine: "Doctor and Dashboard confirm live traffic after configuration",
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
