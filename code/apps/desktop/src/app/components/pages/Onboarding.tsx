import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { AlertTriangle, CheckCircle2, ClipboardCopy, Copy, RefreshCw, Rocket, ShieldAlert } from "lucide-react";
import { useI18n, type Lang } from "../../lib/i18n";
import { useConnectionSetup, type AgentUpstreamBinding, type ConnectionSetupSnapshot } from "../../lib/connectionSetup";
import { useRuntimeDashboardSelector, shallowEqualSlice, type RuntimeDashboardState } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";
import { connectionDiscoveryCounts, presentServiceInstance, safeText } from "../../lib/install/presentation";
import type { DtokenServiceStatus } from "../../lib/install/presentation";
import type { OnboardingGateState } from "../../lib/onboarding";
import { canJoinInstallBatch, dispatchInstallMachine, getInstallMachineState, subscribeInstallMachine } from "../../lib/installMachine";
import { Surface, FlatRows, FlatRow } from "../common/Surface";
import { StepProgress } from "../common/StepProgress";
import { StatusBadge } from "../common/StatusBadge";
import { Button } from "../ui/button";
import { cn } from "../ui/utils";

const STEP_KEYS = ["welcome", "service", "agents", "config", "verify"] as const;
type StepKey = (typeof STEP_KEYS)[number];
const TOTAL_STEPS = STEP_KEYS.length;
const CONFIG_STARTED_KEY = "d-token.onboarding.config-started";

export type OnboardingServiceStartResult = {
  ok: boolean;
  summary: string;
  refreshed: boolean;
};

type OnboardingRuntimeSlice = {
  payload: RuntimeDashboardState["payload"];
  status: RuntimeDashboardState["status"];
  operational: RuntimeDashboardState["operational"];
  updatedAt: RuntimeDashboardState["updatedAt"];
};

type AgentDisplayStatus = {
  autoIncludable: boolean;
  label: string;
  blocked: boolean;
  toolIntegration: boolean;
};

type EvidenceState = { value: "true" | "false" | "unknown"; health: "healthy" | "warning" | "muted" };

const COPY: Record<string, { zh: string; en: string }> = {
  "onb.title": { zh: "欢迎使用 d-token", en: "Welcome to d-token" },
  "onb.subtitle": {
    zh: "连接常用编码工具，减少重复上下文。",
    en: "Connect your coding tools and reduce repeated context.",
  },
  "onb.checking": { zh: "正在检查首次配置状态…", en: "Checking first-run setup status…" },
  "onb.preview.unavailable": {
    zh: "当前为浏览器预览，不会读取或修改本机配置。请在 d-token 桌面窗口中继续。",
    en: "Browser preview does not read or modify local configuration. Continue in the d-token desktop window.",
  },
  "onb.step.welcome": { zh: "欢迎", en: "Welcome" },
  "onb.step.service": { zh: "本地服务", en: "Service" },
  "onb.step.agents": { zh: "编码工具", en: "Tools" },
  "onb.step.config": { zh: "配置", en: "Configure" },
  "onb.step.verify": { zh: "验证", en: "Use" },
  "onb.next": { zh: "下一步", en: "Next" },
  "onb.prev": { zh: "上一步", en: "Back" },
  "onb.stepOf": { zh: "第 {current} 步，共 {total} 步", en: "Step {current} of {total}" },
  "onb.welcome.leading": {
    zh: "启动服务后，选择要连接的工具。",
    en: "Start the service, then choose the tools to connect.",
  },
  "onb.welcome.mode.guided": { zh: "按步骤检查", en: "Guided check" },
  "onb.welcome.mode.guidedDesc": {
    zh: "启动服务、扫描工具、选择要连接的工具。",
    en: "Start the service, scan tools, and choose what to connect.",
  },
  "onb.welcome.mode.manual": { zh: "查看连接状态", en: "View connection status" },
  "onb.welcome.mode.manualDesc": {
    zh: "打开安装与连接页面查看当前状态。",
    en: "Open Install & Connect to view current status.",
  },
  "onb.welcome.start": { zh: "开始引导", en: "Start guided setup" },
  "onb.welcome.jump": { zh: "查看连接状态", en: "View connection status" },
  "onb.section.service": { zh: "本地服务", en: "Local service" },
  "onb.service.desc": {
    zh: "本机服务启动后，d-token 会重新检查连接状态。",
    en: "After the local service starts, d-token checks connection status again.",
  },
  "onb.service.running": { zh: "服务运行中", en: "Service running" },
  "onb.service.stopped": { zh: "服务未运行", en: "Service not running" },
  "onb.service.restartRequired": { zh: "需要重启", en: "Restart required" },
  "onb.service.start": { zh: "启动并检查本地服务", en: "Start & check local service" },
  "onb.service.starting": { zh: "正在启动并检查本地服务…", en: "Starting and checking local service…" },
  "onb.service.retry": { zh: "重试", en: "Retry" },
  "onb.service.stage.start": { zh: "启动本地服务", en: "Start local service" },
  "onb.service.stage.confirm": { zh: "确认运行状态", en: "Confirm running state" },
  "onb.previewNote": {
    zh: "下一步会扫描并选择要接入的编码工具。",
    en: "Next, scan and choose the coding tools to connect.",
  },
  "onb.feedback.serviceSuccessTitle": { zh: "本地服务已启动", en: "Local service started" },
  "onb.feedback.serviceSuccess": {
    zh: "本地服务已启动并完成状态检查。",
    en: "The local service started and state was checked.",
  },
  "onb.feedback.servicePartialTitle": { zh: "本地服务状态待确认", en: "Local service state needs confirmation" },
  "onb.feedback.servicePartial": {
    zh: "已收到启动结果，但状态还未确认。请重新扫描或打开安装与连接检查。{reason}",
    en: "The start result was received, but state is not confirmed. Rescan or open Install & Connect to check. {reason}",
  },
  "onb.feedback.serviceFailed": {
    zh: "启动未完成，请重试或打开安装与连接检查。{reason}",
    en: "Start did not finish. Retry or open Install & Connect to check. {reason}",
  },
  "onb.feedback.serviceFailedFallback": {
    zh: "启动未完成，请重试或打开安装与连接检查。",
    en: "Start did not finish. Retry or open Install & Connect to check.",
  },
  "onb.section.agents": { zh: "编码工具发现", en: "Coding-tool discovery" },
  "onb.agents.heading": { zh: "AI 编码工具接入", en: "AI coding-tool connections" },
  "onb.agents.desc": {
    zh: "已安装且可配置的工具会默认勾选，你可以取消不想连接的工具。",
    en: "Installed configurable tools are checked by default; uncheck any you do not want to connect.",
  },
  "onb.agents.none": { zh: "未检测到已安装的编码工具", en: "No installed coding tools detected" },
  "onb.agents.detected": { zh: "已检测到 {count} 个编码工具", en: "{count} coding tools detected" },
  "onb.agents.unavailable": { zh: "状态不可用", en: "Unavailable" },
  "onb.agents.connected": { zh: "已配置", en: "Configured" },
  "onb.agents.autoInclude": { zh: "未配置", en: "Not configured" },
  "onb.agents.toolIntegration": { zh: "工具集成待配置", en: "Tool integration pending" },
  "onb.agents.needsCredential": { zh: "需要 API Key", en: "API key needed" },
  "onb.agents.configFailed": { zh: "配置失败", en: "Setup failed" },
  "onb.agents.configFailedFix": { zh: "检查该编码工具后重新扫描", en: "Check this coding tool, then scan again" },
  "onb.agents.select": { zh: "选择 {name}", en: "Select {name}" },
  "onb.refresh": { zh: "重新扫描", en: "Rescan" },
  "onb.section.upstream": { zh: "现有连接", en: "Current connections" },
  "onb.upstream.none": { zh: "未检测到可用连接", en: "No usable connections detected" },
  "onb.upstream.detected": { zh: "已检查 {count} 个现有连接", en: "{count} current connections checked" },
  "onb.upstream.ccSwitch": { zh: "cc-switch：{status}", en: "cc-switch: {status}" },
  "onb.upstream.ccSwitchDetected": { zh: "已检测到", en: "Detected" },
  "onb.upstream.ccSwitchNone": { zh: "未检测到", en: "Not detected" },
  "onb.config.heading": { zh: "准备一键接入", en: "Prepare one-click setup" },
  "onb.config.desc": {
    zh: "确认要连接的工具后，开始一键接入。",
    en: "Confirm the tools you want to connect, then start one-click setup.",
  },
  "onb.config.batchCount": { zh: "本次将接入 {count} 个编码工具", en: "{count} coding tools will be connected" },
  "onb.config.noneSelected": { zh: "请至少选择一个可接入的编码工具。", en: "Select at least one connectable coding tool." },
  "onb.config.conflictBlocked": {
    zh: "部分所选工具需要处理；可配置的工具会继续，失败项会保留单独重试入口。",
    en: "Some selected tools need attention; configurable tools can continue, and failed items keep their own retry path.",
  },
  "onb.config.currentConnection": { zh: "当前连接", en: "Current connection" },
  "onb.config.protocol": { zh: "协议", en: "Protocol" },
  "onb.config.targetPath": { zh: "检查的位置", en: "Checked location" },
  "onb.config.copy": { zh: "复制检查结果", en: "Copy check result" },
  "onb.config.copied": { zh: "已复制到剪贴板", en: "Copied to clipboard" },
  "onb.config.start": { zh: "开始一键接入", en: "Start one-click setup" },
  "onb.config.startDesc": {
    zh: "安装与连接会显示每个工具的接入结果。",
    en: "Install & Connect will show the result for each tool.",
  },
  "onb.feedback.copySuccess": { zh: "检查结果已复制到剪贴板。", en: "Check result copied to the clipboard." },
  "onb.feedback.copyFailed": { zh: "无法复制检查结果，请手动选择并复制。", en: "Could not copy the check result. Select and copy it manually." },
  "onb.verify.desc": {
    zh: "开始使用前，确认工具已接入并通过验证。",
    en: "Before you use d-token, confirm the tools are connected and verified.",
  },
  "onb.verify.detected": { zh: "检测到", en: "Detected" },
  "onb.verify.detectedDesc": { zh: "已发现相关编码工具或连接状态。", en: "Related coding tools or connection state were found." },
  "onb.verify.connected": { zh: "已接入", en: "Connected" },
  "onb.verify.connectedDesc": { zh: "已发现本地连接配置。", en: "A local connection setting was found." },
  "onb.verify.verified": { zh: "已验证", en: "Verified" },
  "onb.verify.verifiedDesc": { zh: "已通过连接检查。", en: "Connection checks passed." },
  "onb.verify.detectedGroup": { zh: "检测结果", en: "Detection evidence" },
  "onb.verify.connectedGroup": { zh: "接入声明", en: "Connection claim" },
  "onb.verify.verifiedGroup": { zh: "验证证据", en: "Verification evidence" },
  "onb.verify.state.true": { zh: "证据成立", en: "Evidence present" },
  "onb.verify.state.false": { zh: "未通过", en: "Not passed" },
  "onb.verify.state.unknown": { zh: "未知", en: "Unknown" },
  "onb.verify.finish": { zh: "完成引导", en: "Finish guide" },
  "onb.verify.enterWorkbench": { zh: "进入安装与连接完成验证", en: "Enter Install & Connect to verify" },
  "onb.verify.finishDesc": {
    zh: "完成后会进入安装与连接继续查看状态。",
    en: "Finishing opens Install & Connect so you can keep checking status.",
  },
  "onb.later": { zh: "稍后再说", en: "Maybe later" },
  "onb.laterDesc": { zh: "本次先跳过；完成配置前，下次启动仍会提示。", en: "Skip for now; you will be reminded on next launch until setup is complete." },
  "onb.completedHint": { zh: "你可以随时从右上角再次打开配置向导。", en: "You can reopen the setup wizard anytime from the top-right." },
  "onb.backOverview": { zh: "返回总体概览", en: "Back to Overview" },
};

const selectRuntimeSlice = (state: RuntimeDashboardState): OnboardingRuntimeSlice => ({
  payload: state.payload,
  status: state.status,
  operational: state.operational,
  updatedAt: state.updatedAt,
});

function text(lang: Lang, key: string, values: Record<string, string | number> = {}) {
  let value = COPY[key]?.[lang] ?? key;
  for (const [name, replacement] of Object.entries(values)) {
    value = value.split(`{${name}}`).join(String(replacement));
  }
  return value;
}

function deriveServicePresentation(serviceOnline: boolean, loading: boolean, restartRequired: boolean) {
  if (restartRequired) return { status: "warning" as const, phase: "restart-required" as const };
  if (serviceOnline) return { status: "healthy" as const, phase: "running" as const };
  if (loading) return { status: "warning" as const, phase: "loading" as const };
  return { status: "error" as const, phase: "stopped" as const };
}

function agentBatchStatus(binding: AgentUpstreamBinding, lang: Lang): AgentDisplayStatus {
  if (binding.installed && (binding.integration_kind === "mcp" || binding.integration_kind === "hook")) return { autoIncludable: false, label: text(lang, "onb.agents.toolIntegration"), blocked: false, toolIntegration: true };
  if (binding.already_connected && canJoinInstallBatch(binding)) return { autoIncludable: true, label: text(lang, "onb.agents.connected"), blocked: false, toolIntegration: false };
  if (binding.resolution === "auto-resolved" && canJoinInstallBatch(binding)) return { autoIncludable: true, label: text(lang, "onb.agents.autoInclude"), blocked: false, toolIntegration: false };
  if (binding.resolution === "needs-credential") return { autoIncludable: false, label: text(lang, "onb.agents.needsCredential"), blocked: true, toolIntegration: false };
  return { autoIncludable: false, label: text(lang, "onb.agents.configFailed"), blocked: true, toolIntegration: false };
}

function canSelectAgent(binding: AgentUpstreamBinding) {
  return canJoinInstallBatch(binding) && (binding.already_connected || binding.resolution === "auto-resolved");
}

function bindingRouteFacts(binding: AgentUpstreamBinding): { providerModel: string; protocol: string; target: string } {
  const candidate = binding.resolved_candidate ?? binding.candidates?.[0] ?? null;
  const current = binding.follow?.current_upstream ?? null;
  const provider = current?.provider_id || candidate?.name || "—";
  const model = current?.model || candidate?.models?.[0] || "—";
  return {
    providerModel: `${provider} / ${model}`,
    protocol: current?.protocol || candidate?.protocol || "—",
    target: binding.config_path || binding.config_paths?.[0] || "—",
  };
}

function evidenceState(value: boolean | null): EvidenceState {
  if (value === true) return { value: "true", health: "healthy" };
  if (value === false) return { value: "false", health: "warning" };
  return { value: "unknown", health: "muted" };
}

async function copyText(value: string): Promise<boolean> {
  try {
    if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return true;
    }
  } catch {
    /* use fallback */
  }
  if (typeof document === "undefined") return false;
  let textarea: HTMLTextAreaElement | null = null;
  try {
    textarea = document.createElement("textarea");
    textarea.value = value;
    textarea.setAttribute("readonly", "");
    textarea.style.position = "absolute";
    textarea.style.left = "-9999px";
    document.body.appendChild(textarea);
    textarea.select();
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    textarea?.remove();
  }
}

function readConfigStarted() {
  if (typeof window === "undefined") return false;
  try {
    return window.sessionStorage.getItem(CONFIG_STARTED_KEY) === "1";
  } catch {
    return false;
  }
}

function markConfigStarted() {
  if (typeof window === "undefined") return;
  try {
    window.sessionStorage.setItem(CONFIG_STARTED_KEY, "1");
  } catch {
    /* unavailable in some preview contexts */
  }
}

function buildDetectionSummary(snapshot: ConnectionSetupSnapshot | null, visibleBindings: AgentUpstreamBinding[], lang: Lang): string {
  const lines: string[] = [lang === "zh" ? "d-token 检测摘要（元数据，不含凭据）" : "d-token detection summary (metadata only, no credentials)"];
  lines.push(`${lang === "zh" ? "来源" : "Source"}: ${safeText(snapshot?.source, "—")}`);
  lines.push(`${lang === "zh" ? "修订" : "Revision"}: ${safeText(snapshot?.revision, "—")}`);
  const batch = visibleBindings.map((binding) => {
    const facts = bindingRouteFacts(binding);
    const resolved = binding.resolved_candidate ?? binding.candidates?.[0] ?? null;
    const route = resolved || binding.follow?.current_upstream
      ? `${facts.providerModel} / ${facts.protocol}`
      : binding.already_connected
        ? lang === "zh" ? "已配置" : "configured"
        : lang === "zh" ? "未配置" : "not configured";
    return `${binding.agent_name || binding.agent_id} → ${route}`;
  }).join("; ");
  lines.push(`${lang === "zh" ? "本次检查" : "Current check"}: ${batch || "—"}`);
  const candidates = snapshot?.candidates ?? [];
  lines.push(`${lang === "zh" ? "现有连接" : "Current connections"}: ${candidates.map((candidate) => candidate.name || candidate.id).join(", ") || "—"}`);
  const ccSwitch = snapshot?.cc_switch_sync?.detected ? (lang === "zh" ? "已检测到" : "Detected") : (lang === "zh" ? "未检测到" : "Not detected");
  lines.push(`cc-switch: ${ccSwitch}`);
  return lines.join("\n");
}

export function OnboardingPage({
  gateState,
  reducedMotion = false,
  onComplete,
  onDismiss,
  onStartSetup,
  onOpenConnections,
  onOpenOverview,
  onStartService,
}: {
  gateState: OnboardingGateState;
  reducedMotion?: boolean;
  onComplete: () => Promise<boolean>;
  onDismiss: () => void;
  onStartSetup: () => void;
  onOpenConnections?: () => void;
  onOpenOverview: () => void;
  onStartService: () => Promise<OnboardingServiceStartResult>;
}) {
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const { snapshot, status: discoveryStatus, refresh: refreshDiscovery } = useConnectionSetup();
  const runtimeSlice = useRuntimeDashboardSelector(selectRuntimeSlice, shallowEqualSlice);
  const selectedAgentIds = useSyncExternalStore(subscribeInstallMachine, getInstallMachineState).selectedAgentIds;

  useEffect(() => {
    if (!snapshot) return;
    dispatchInstallMachine({ type: "DISCOVERED", snapshot });
    const selectable = new Set(snapshot.agent_bindings.filter(canSelectAgent).map((binding) => binding.agent_id));
    const selected = getInstallMachineState().selectedAgentIds;
    const filtered = selected.filter((id) => selectable.has(id));
    if (filtered.length !== selected.length) {
      dispatchInstallMachine({ type: "SET_AGENT_SELECTION", agentIds: filtered });
    }
  }, [snapshot]);

  const [step, setStep] = useState<StepKey>("welcome");
  const [configStarted, setConfigStarted] = useState(readConfigStarted);
  const [copied, setCopied] = useState(false);
  const [copyFeedback, setCopyFeedback] = useState<"success" | "error" | null>(null);
  const [serviceAction, setServiceAction] = useState<{ phase: "idle" | "loading" | "success" | "partial" | "error"; summary: string }>({
    phase: "idle",
    summary: "",
  });

  const serviceStages = useMemo(
    () => [
      { key: "start", label: text(lang, "onb.service.stage.start") },
      { key: "confirm", label: text(lang, "onb.service.stage.confirm") },
    ],
    [lang],
  );

  const serviceStatus = runtimeSlice.operational?.service?.data as DtokenServiceStatus | null;
  const restartRequired = serviceStatus?.state === "restart_required" || serviceStatus?.restart_required === true;
  const serviceOnline = !restartRequired && (serviceStatus ? ["running", "unmanaged_running"].includes(serviceStatus.state) : runtimeSlice.status === "runtime");
  const serviceLoading = runtimeSlice.status === "loading" && serviceStatus === null;
  const servicePresentation = deriveServicePresentation(serviceOnline, serviceLoading, restartRequired);
  const serviceEndpoint = serviceOnline ? serviceStatus?.service_url || safeText(runtimeSlice.payload?.desktop_service_url, "") : "";
  const serviceInstanceDetail = presentServiceInstance(serviceStatus, serviceOnline, serviceEndpoint, runtimeSlice.updatedAt, lang);

  const counts = useMemo(() => connectionDiscoveryCounts(snapshot), [snapshot]);
  const visibleBindings = useMemo(
    () => (snapshot?.agent_bindings ?? []).filter((binding) => binding.installed || binding.already_connected),
    [snapshot?.agent_bindings],
  );
  const batchStatuses = useMemo(
    () => visibleBindings.map((binding) => ({ binding, status: agentBatchStatus(binding, lang) })),
    [lang, visibleBindings],
  );
  const selectedStatuses = batchStatuses.filter(({ binding }) => selectedAgentIds.includes(binding.agent_id));
  const selectedReadyCount = selectedStatuses.filter(({ status }) => status.autoIncludable).length;
  const batchHasBlockedSelection = selectedStatuses.some(({ status }) => status.blocked);
  const setupDisabled = gateState === "unavailable" || selectedReadyCount === 0;
  const ccSwitchStatus = snapshot?.cc_switch_sync?.detected ? text(lang, "onb.upstream.ccSwitchDetected") : text(lang, "onb.upstream.ccSwitchNone");
  const discovering = discoveryStatus === "idle" || discoveryStatus === "discovering";
  const discoveryUnavailable = discoveryStatus === "unavailable" || snapshot === null;
  const nativeUnavailable = gateState === "unavailable";

  const serviceCopy = servicePresentation.phase === "running"
    ? { label: text(lang, "onb.service.running"), health: "healthy" as const }
    : servicePresentation.phase === "restart-required"
      ? { label: text(lang, "onb.service.restartRequired"), health: "warning" as const }
      : servicePresentation.phase === "loading"
        ? { label: text(lang, "onb.checking"), health: "warning" as const }
        : { label: text(lang, "onb.service.stopped"), health: "error" as const };

  const stepIndex = STEP_KEYS.indexOf(step);
  const discoveryReady = !discovering && !discoveryUnavailable;
  const detectionText = buildDetectionSummary(snapshot, visibleBindings, lang);
  const detectedEvidence = evidenceState(discoveryUnavailable ? null : visibleBindings.length > 0);
  const anyConnected = visibleBindings.some((binding) => binding.already_connected);
  const connectedEvidence = evidenceState(
    discoveryUnavailable ? null : anyConnected ? true : snapshot?.agent_route_follow?.state === "disabled" ? false : null,
  );
  const verifiedEvidence = evidenceState(
    discoveryUnavailable
      ? null
      : anyConnected && serviceOnline && snapshot?.agent_route_follow?.state === "applied_verified"
        ? true
        : anyConnected && (!serviceOnline || snapshot?.agent_route_follow?.enabled === false)
          ? false
          : null,
  );

  const canEnterStep = (key: StepKey): boolean => {
    switch (key) {
      case "welcome":
      case "service":
        return true;
      case "agents":
        return !nativeUnavailable && serviceOnline;
      case "config":
        return !nativeUnavailable && serviceOnline && discoveryReady;
      case "verify":
        return !nativeUnavailable && configStarted;
    }
  };

  const goNext = () => {
    const next = STEP_KEYS[Math.min(stepIndex + 1, TOTAL_STEPS - 1)];
    if (next !== step && canEnterStep(next)) setStep(next);
  };
  const goPrev = () => setStep((current) => STEP_KEYS[Math.max(STEP_KEYS.indexOf(current) - 1, 0)]);

  const handleCopy = async () => {
    setCopyFeedback(null);
    try {
      const ok = await copyText(detectionText);
      setCopied(ok);
      setCopyFeedback(ok ? "success" : "error");
    } catch {
      setCopied(false);
      setCopyFeedback("error");
    }
  };

  const handleStartService = async () => {
    if (nativeUnavailable) return;
    setServiceAction({ phase: "loading", summary: "" });
    const requestId = createOperationRequestId("onboarding-service-start");
    let nextAction: { phase: "success" | "partial" | "error"; summary: string } | null = null;
    try {
      const result = await onStartService();
      if (result.ok && result.refreshed) {
        const message = text(lang, "onb.feedback.serviceSuccess");
        nextAction = { phase: "success", summary: message };
        addNotification({
          dedupeKey: "onboarding-service-started",
          level: "success",
          title: text(lang, "onb.feedback.serviceSuccessTitle"),
          message,
          receipt: { requestId, targetId: "local-service" },
        });
        return;
      }

      const failure = presentOperationFailure("service", result.summary, lang);
      const partial = result.ok;
      const message = text(lang, partial ? "onb.feedback.servicePartial" : "onb.feedback.serviceFailed", { reason: failure.message });
      nextAction = { phase: partial ? "partial" : "error", summary: message };
      addNotification({
        dedupeKey: partial ? "onboarding-service-state-unconfirmed" : "onboarding-service-start-failed",
        level: partial ? "warning" : "error",
        title: partial ? text(lang, "onb.feedback.servicePartialTitle") : failure.title,
        message,
        details: failure.details,
        receipt: { requestId, targetId: "local-service" },
      });
    } catch (error) {
      const failure = presentOperationFailure("service", error, lang);
      const message = text(lang, "onb.feedback.serviceFailed", { reason: failure.message });
      nextAction = { phase: "error", summary: message };
      addNotification({
        dedupeKey: "onboarding-service-start-error",
        level: "error",
        title: failure.title,
        message,
        details: failure.details,
        receipt: { requestId, targetId: "local-service" },
      });
    } finally {
      setServiceAction(nextAction ?? { phase: "error", summary: text(lang, "onb.feedback.serviceFailedFallback") });
    }
  };

  const handleStartSetup = () => {
    setConfigStarted(true);
    markConfigStarted();
    onStartSetup();
  };

  const handleOpenConnections = () => {
    (onOpenConnections ?? onStartSetup)();
  };

  const renderStep = () => {
    switch (step) {
      case "welcome":
        return (
          <div className="space-y-4">
            <p className="text-sm leading-relaxed text-muted-foreground">{text(lang, "onb.welcome.leading")}</p>
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="flat-inset flex flex-col gap-2 rounded-lg border border-border/60 p-4">
                <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <Rocket aria-hidden="true" className="size-4 text-primary" />
                  {text(lang, "onb.welcome.mode.guided")}
                </div>
                <p className="text-xs leading-relaxed text-muted-foreground">{text(lang, "onb.welcome.mode.guidedDesc")}</p>
                <Button type="button" className="mt-2 self-start" onClick={goNext}>{text(lang, "onb.welcome.start")}</Button>
              </div>
              <div className="flat-inset flex flex-col gap-2 rounded-lg border border-border/60 p-4">
                <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <ClipboardCopy aria-hidden="true" className="size-4 text-muted-foreground" />
                  {text(lang, "onb.welcome.mode.manual")}
                </div>
                <p className="text-xs leading-relaxed text-muted-foreground">{text(lang, "onb.welcome.mode.manualDesc")}</p>
                <Button type="button" variant="outline" className="mt-2 self-start" onClick={handleOpenConnections}>{text(lang, "onb.welcome.jump")}</Button>
              </div>
            </div>
          </div>
        );

      case "service":
        return (
          <div className="space-y-3">
            <p className="text-sm leading-relaxed text-muted-foreground">{text(lang, "onb.service.desc")}</p>
            <Surface>
              <div className="p-4">
                <StepProgress steps={serviceStages} current={serviceAction.phase === "loading" ? 0 : 1} done={serviceAction.phase === "success"} reducedMotion={reducedMotion} />
              </div>
            </Surface>
            <Surface>
              <FlatRows>
                <FlatRow className="items-center gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-foreground">{text(lang, "onb.section.service")}</div>
                    <div className="mt-0.5 truncate text-xs text-muted-foreground" title={serviceInstanceDetail}>{serviceInstanceDetail}</div>
                  </div>
                  <StatusBadge status={serviceCopy.health} label={serviceCopy.label} />
                </FlatRow>
              </FlatRows>
            </Surface>
            <div className="flex flex-wrap items-center gap-3">
              <Button type="button" onClick={() => void handleStartService()} disabled={nativeUnavailable || serviceAction.phase === "loading"}>
                {serviceAction.phase === "loading"
                  ? text(lang, "onb.service.starting")
                  : serviceAction.phase === "error" || serviceAction.phase === "partial"
                    ? text(lang, "onb.service.retry")
                    : text(lang, "onb.service.start")}
              </Button>
              <p className="min-w-0 flex-1 text-xs text-muted-foreground" role="status" aria-live="polite">
                {serviceAction.phase === "idle" ? text(lang, "onb.previewNote") : serviceAction.summary}
              </p>
            </div>
          </div>
        );

      case "agents":
        return (
          <div className="space-y-3">
            <p className="text-sm leading-relaxed text-muted-foreground">{text(lang, "onb.agents.desc")}</p>
            <Surface>
              <FlatRows>
                <FlatRow className="items-center gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-foreground">{text(lang, "onb.section.agents")}</div>
                    <div className="mt-0.5 text-xs text-muted-foreground">
                      {discovering
                        ? text(lang, "onb.checking")
                        : discoveryUnavailable
                          ? text(lang, "onb.agents.unavailable")
                          : counts.agents === 0
                            ? text(lang, "onb.agents.none")
                            : text(lang, "onb.agents.detected", { count: counts.agents })}
                    </div>
                    {!discovering && !discoveryUnavailable && batchStatuses.length > 0 && (
                      <ul className="mt-2 grid gap-2" aria-label={text(lang, "onb.agents.heading")}>
                        {batchStatuses.map(({ binding, status }) => {
                          const selectable = canSelectAgent(binding);
                          const checked = selectedAgentIds.includes(binding.agent_id) && selectable;
                          return (
                            <li key={binding.agent_id} className="flex items-center gap-2 rounded-lg border border-border/60 bg-card/60 px-3 py-2 text-xs text-muted-foreground">
                              <label className={cn("flex min-w-0 flex-1 items-center gap-2", selectable ? "cursor-pointer" : "cursor-not-allowed opacity-70")}>
                                <input
                                  type="checkbox"
                                  checked={checked}
                                  disabled={!selectable}
                                  onChange={() => dispatchInstallMachine({ type: "TOGGLE_AGENT", agentId: binding.agent_id })}
                                  aria-label={text(lang, "onb.agents.select", { name: binding.agent_name || binding.agent_id })}
                                />
                                <span className="min-w-0 flex-1 truncate font-medium text-foreground">{binding.agent_name || binding.agent_id}</span>
                              </label>
                              <span className={cn(
                                "inline-flex items-center gap-1",
                                status.autoIncludable ? "text-emerald-600" : status.toolIntegration ? "text-muted-foreground" : "text-rose-600",
                              )}>
                                {status.autoIncludable ? <CheckCircle2 aria-hidden="true" className="size-3" /> : status.blocked ? <AlertTriangle aria-hidden="true" className="size-3" /> : null}
                                {status.label}
                              </span>
                              {status.blocked && <span className="text-xs text-amber-600">{text(lang, "onb.agents.configFailedFix")}</span>}
                            </li>
                          );
                        })}
                      </ul>
                    )}
                  </div>
                  <Button type="button" variant="ghost" size="sm" onClick={() => void refreshDiscovery()} disabled={nativeUnavailable} aria-label={text(lang, "onb.refresh")}>
                    <RefreshCw aria-hidden="true" className="size-3.5" />
                  </Button>
                </FlatRow>
              </FlatRows>
            </Surface>
          </div>
        );

      case "config":
        return (
          <div className="space-y-3">
            <p className="text-sm leading-relaxed text-muted-foreground">{text(lang, "onb.config.desc")}</p>
            <Surface>
              <FlatRows>
                <FlatRow className="items-center gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-foreground">{text(lang, "onb.section.upstream")}</div>
                    <div className="mt-0.5 text-xs text-muted-foreground">
                      {counts.upstreams === 0 ? text(lang, "onb.upstream.none") : text(lang, "onb.upstream.detected", { count: counts.upstreams })}
                    </div>
                  </div>
                  <div className="shrink-0 text-xs text-muted-foreground">{text(lang, "onb.upstream.ccSwitch", { status: ccSwitchStatus })}</div>
                </FlatRow>
              </FlatRows>
            </Surface>
            <Surface className="overflow-hidden">
              <section role="region" aria-labelledby="onboarding-config-batch">
                <h2 id="onboarding-config-batch" className="px-4 pt-4 text-sm font-medium text-foreground">{text(lang, "onb.config.batchCount", { count: selectedReadyCount })}</h2>
                <div className="px-4 pb-4 pt-2">
                  {batchStatuses.map(({ binding, status }) => {
                    const facts = bindingRouteFacts(binding);
                    const selected = selectedAgentIds.includes(binding.agent_id) && status.autoIncludable;
                    return (
                      <dl key={binding.agent_id} className="mt-2 grid gap-x-4 gap-y-1.5 rounded-lg border border-border/60 bg-card/40 p-3 text-xs sm:grid-cols-2" data-selected={selected ? "true" : "false"}>
                        <div className="col-span-full flex items-center gap-2">
                          <dt className="font-medium text-foreground">{binding.agent_name || binding.agent_id}</dt>
                          <span className={cn("inline-flex items-center gap-1", status.autoIncludable ? "text-emerald-600" : "text-rose-600")}>
                            {status.autoIncludable ? <CheckCircle2 aria-hidden="true" className="size-3" /> : <AlertTriangle aria-hidden="true" className="size-3" />}
                            {status.label}
                          </span>
                        </div>
                        <div className="min-w-0">
                          <dt className="text-muted-foreground">{text(lang, "onb.config.currentConnection")}</dt>
                          <dd className="mt-0.5 truncate text-foreground" title={facts.providerModel}>{facts.providerModel}</dd>
                        </div>
                        <div className="min-w-0">
                          <dt className="text-muted-foreground">{text(lang, "onb.config.protocol")}</dt>
                          <dd className="mt-0.5 truncate text-foreground" title={facts.protocol}>{facts.protocol}</dd>
                        </div>
                        <div className="min-w-0 sm:col-span-2">
                          <dt className="text-muted-foreground">{text(lang, "onb.config.targetPath")}</dt>
                          <dd className="mt-0.5 truncate text-foreground" title={facts.target}>{facts.target}</dd>
                        </div>
                      </dl>
                    );
                  })}
                </div>
              </section>
            </Surface>
            {batchHasBlockedSelection && (
              <div role="status" className="flex items-start gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300">
                <AlertTriangle aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
                <span>{text(lang, "onb.config.conflictBlocked")}</span>
              </div>
            )}
            {selectedReadyCount === 0 && (
              <div role="status" className="flex items-start gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300">
                <AlertTriangle aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
                <span>{text(lang, "onb.config.noneSelected")}</span>
              </div>
            )}
            <Surface className="overflow-hidden">
              <section role="region" aria-labelledby="onboarding-detection-summary">
                <h2 id="onboarding-detection-summary" className="px-4 pt-4 text-sm font-medium text-foreground">{text(lang, "onb.config.heading")}</h2>
                <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted/40 p-4 text-xs leading-relaxed text-foreground">{detectionText}</pre>
              </section>
            </Surface>
            <div className="flex flex-wrap items-center gap-3">
              <Button type="button" variant="outline" onClick={() => void handleCopy()} disabled={nativeUnavailable}>
                {copied ? <CheckCircle2 aria-hidden="true" className="size-4 text-emerald-600" /> : <Copy aria-hidden="true" className="size-4" />}
                {copied ? text(lang, "onb.config.copied") : text(lang, "onb.config.copy")}
              </Button>
              {copyFeedback && (
                <p className={cn("text-xs", copyFeedback === "success" ? "text-emerald-700 dark:text-emerald-300" : "text-rose-700 dark:text-rose-300")} role="status" aria-live="polite">
                  {text(lang, copyFeedback === "success" ? "onb.feedback.copySuccess" : "onb.feedback.copyFailed")}
                </p>
              )}
              <Button type="button" onClick={handleStartSetup} disabled={setupDisabled}>{text(lang, "onb.config.start")}</Button>
            </div>
            <p className="text-xs text-muted-foreground">{text(lang, "onb.config.startDesc")}</p>
          </div>
        );

      case "verify": {
        const evidence = [
          { key: "detected", state: detectedEvidence },
          { key: "connected", state: connectedEvidence },
          { key: "verified", state: verifiedEvidence },
        ] as const;
        return (
          <div className="space-y-4">
            <p className="text-sm leading-relaxed text-muted-foreground">{text(lang, "onb.verify.desc")}</p>
            <div className="grid gap-3 sm:grid-cols-3">
              {evidence.map(({ key, state }) => (
                <div key={key} role="group" aria-label={text(lang, `onb.verify.${key}Group`)} data-evidence-health={state.health} className="flat-inset rounded-lg border border-border/60 p-4">
                  <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                    {state.health === "healthy"
                      ? <CheckCircle2 aria-hidden="true" className="size-4 text-emerald-600" />
                      : state.health === "warning"
                        ? <AlertTriangle aria-hidden="true" className="size-4 text-amber-600" />
                        : <span aria-hidden="true" className="size-4 rounded-full border border-border/70" />}
                    {text(lang, `onb.verify.${key}`)}
                  </div>
                  <div className="mt-1.5 text-xs font-medium text-muted-foreground">{text(lang, `onb.verify.state.${state.value}`)}</div>
                  <p className="mt-1 text-xs leading-relaxed text-muted-foreground">{text(lang, `onb.verify.${key}Desc`)}</p>
                </div>
              ))}
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <Button type="button" onClick={() => void onComplete()} disabled={nativeUnavailable} aria-disabled={nativeUnavailable} title={nativeUnavailable ? text(lang, "onb.preview.unavailable") : undefined}>
                {text(lang, "onb.verify.finish")}
              </Button>
              <Button type="button" variant="outline" onClick={handleOpenConnections}>{text(lang, "onb.verify.enterWorkbench")}</Button>
            </div>
            <p className="text-xs text-muted-foreground">{text(lang, "onb.verify.finishDesc")}</p>
          </div>
        );
      }
    }
  };

  const nextStep = STEP_KEYS[Math.min(stepIndex + 1, TOTAL_STEPS - 1)];

  return (
    <div className="onboarding-wizard mx-auto w-full max-w-3xl px-6 py-8" data-reduced-motion={reducedMotion ? "true" : "false"}>
      <h1 className="mb-2">{text(lang, "onb.title")}</h1>
      <p className="mb-6 max-w-2xl text-sm leading-relaxed text-muted-foreground">{text(lang, "onb.subtitle")}</p>

      {nativeUnavailable && (
        <div role="status" className="mb-4 flex items-start gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300">
          <ShieldAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
          <span>{text(lang, "onb.preview.unavailable")}</span>
        </div>
      )}

      <ol className="mb-6 flex flex-wrap items-center gap-1.5" aria-label={text(lang, "onb.stepOf", { current: stepIndex + 1, total: TOTAL_STEPS })}>
        {STEP_KEYS.map((key, index) => {
          const enabled = canEnterStep(key);
          return (
            <li key={key} className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={() => enabled && setStep(key)}
                disabled={!enabled}
                aria-current={key === step ? "step" : undefined}
                className={cn(
                  "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs transition-colors",
                  key === step ? "border-primary/40 bg-primary/10 font-medium text-foreground" : "border-border/60 bg-card/40 text-muted-foreground hover:bg-card/70 hover:text-foreground",
                  !enabled && "cursor-not-allowed opacity-40 hover:bg-card/40",
                )}
              >
                <span className={cn("flex size-4 items-center justify-center rounded-full text-[10px]", key === step ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground")}>
                  {index + 1}
                </span>
                {text(lang, `onb.step.${key}`)}
              </button>
              {index < TOTAL_STEPS - 1 && <span className="text-muted-foreground/40" aria-hidden="true">→</span>}
            </li>
          );
        })}
      </ol>

      <p className="mb-4 text-xs text-muted-foreground" aria-live="polite">
        {text(lang, "onb.stepOf", { current: stepIndex + 1, total: TOTAL_STEPS })}
      </p>

      {renderStep()}

      {step === "welcome" || step === "verify" ? null : (
        <div className="mt-6 flex flex-wrap items-center gap-3">
          <Button type="button" variant="ghost" onClick={goPrev}>{text(lang, "onb.prev")}</Button>
          <Button type="button" onClick={goNext} disabled={!canEnterStep(nextStep)}>{text(lang, "onb.next")}</Button>
        </div>
      )}

      <div className="mt-6 flex flex-wrap items-center gap-3 border-t border-border/50 pt-4">
        {gateState === "completed" ? (
          <>
            <Button type="button" variant="ghost" onClick={onOpenOverview}>{text(lang, "onb.backOverview")}</Button>
            <span className="text-xs text-muted-foreground">{text(lang, "onb.completedHint")}</span>
          </>
        ) : (
          <>
            <Button type="button" variant="ghost" onClick={onDismiss}>{text(lang, "onb.later")}</Button>
            <span className="text-xs text-muted-foreground">{text(lang, "onb.laterDesc")}</span>
          </>
        )}
      </div>
    </div>
  );
}
