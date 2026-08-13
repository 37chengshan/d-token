import { useEffect, useMemo, useSyncExternalStore, useState } from "react";
import { AlertTriangle, CheckCircle2, ClipboardCopy, Copy, RefreshCw, Rocket, ShieldAlert } from "lucide-react";
import { useI18n, useT } from "../../lib/i18n";
import { useConnectionSetup, type AgentUpstreamBinding, type ConnectionSetupSnapshot } from "../../lib/connectionSetup";
import { useRuntimeDashboardSelector, shallowEqualSlice, type RuntimeDashboardState } from "../../lib/useRuntimeDashboard";
import { connectionDiscoveryCounts, presentServiceInstance, safeText } from "../../lib/install/presentation";
import type { DtokenServiceStatus } from "../../lib/install/presentation";
import type { OnboardingGateState } from "../../lib/onboarding";
import { canJoinInstallBatch, dispatchInstallMachine, getInstallMachineState, subscribeInstallMachine } from "../../lib/installMachine";
import { Surface, FlatRows, FlatRow } from "../common/Surface";
import { StepProgress } from "../common/StepProgress";
import { StatusBadge } from "../common/StatusBadge";
import { Button } from "../ui/button";
import { cn } from "../ui/utils";

/**
 * First-run onboarding wizard. It never re-implements discovery or
 * configuration: it consumes the same ConnectionSetup (native discovery) and
 * runtime dashboard slices the rest of the app uses, presents honest states
 * (detected ≠ connected ≠ verified), and hands the real
 * preview → confirm → backup → apply → verify → rollback flow to the existing
 * Install & Connect page. Completion is only ever a user-confirmed action that
 * persists the first-config marker in the d-token settings store.
 *
 * The wizard is a 5-step guided flow: welcome → service → agents →
 * config (upstream facts folded in, no standalone upstream step) → verify.
 * Step position is local component state so re-entering a completed wizard
 * still starts at welcome and lets the user browse every step again; the
 * completed marker only decides whether the wizard auto-opens on launch, it
 * never freezes the flow.
 *
 * Steps unlock only on real state, never on intent: agents requires the local
 * service to be really running, config additionally requires discovery to
 * have completed, and verify requires the user to have started the one-click
 * setup. The stepper and "下一步" both respect the same gates; a locked step
 * cannot be entered by clicking.
 */

/** The five wizard steps; index doubles as the active-step state. */
const STEP_KEYS = ["welcome", "service", "agents", "config", "verify"] as const;
type StepKey = (typeof STEP_KEYS)[number];
const TOTAL_STEPS = STEP_KEYS.length;

/**
 * Result of the real service-start transaction the shell owns. `ok` is the
 * native receipt; `refreshed` is whether discovery was re-read successfully
 * after the transaction. Onboarding never fabricates success from the receipt
 * alone — the UI only reports success when both are true.
 */
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

const selectRuntimeSlice = (state: RuntimeDashboardState): OnboardingRuntimeSlice => ({
  payload: state.payload,
  status: state.status,
  operational: state.operational,
  updatedAt: state.updatedAt,
});

function deriveServicePresentation(serviceOnline: boolean, loading: boolean, restartRequired: boolean) {
  if (restartRequired) return { isOnline: false, status: "warning" as const, phase: "restart-required" as const };
  if (serviceOnline) return { isOnline: true, status: "healthy" as const, phase: "running" as const };
  if (loading) return { isOnline: false, status: "warning" as const, phase: "loading" as const };
  return { isOnline: false, status: "error" as const, phase: "stopped" as const };
}

/**
 * Batch inclusion status for the read-only Agent step. Eligibility is owned by
 * installMachine so onboarding cannot drift from the real one-click batch.
 */
function agentBatchStatus(binding: AgentUpstreamBinding, t: (key: string) => string): { autoIncludable: boolean; label: string; conflict: boolean } {
  if (binding.already_connected) return { autoIncludable: true, label: t("onb.agents.connected"), conflict: false };
  if (binding.resolution === "auto-resolved" && canJoinInstallBatch(binding)) return { autoIncludable: true, label: t("onb.agents.autoInclude"), conflict: false };
  return { autoIncludable: false, label: t("onb.agents.conflict"), conflict: true };
}

/** Current route facts (metadata only) shown per Agent on the config step. */
function bindingRouteFacts(binding: AgentUpstreamBinding): { providerModel: string; protocol: string; endpoint: string; target: string } {
  const candidate = binding.resolved_candidate ?? binding.candidates?.[0] ?? null;
  const current = binding.follow?.current_upstream ?? null;
  const provider = current?.provider_id || candidate?.name || "—";
  const model = current?.model || candidate?.models?.[0] || "—";
  return {
    providerModel: `${provider} / ${model}`,
    protocol: current?.protocol || candidate?.protocol || "—",
    endpoint: current?.normalized_endpoint || candidate?.base_url || "—",
    target: binding.config_path || binding.config_paths?.[0] || "—",
  };
}

type EvidenceState = { value: "true" | "false" | "unknown"; health: "healthy" | "warning" | "muted" };

function evidenceState(value: boolean | null): EvidenceState {
  if (value === true) return { value: "true", health: "healthy" };
  if (value === false) return { value: "false", health: "warning" };
  return { value: "unknown", health: "muted" };
}

/** Copies text to the clipboard with a fallback for non-secure/preview contexts. */
async function copyText(text: string): Promise<boolean> {
  try {
    if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* fall through to legacy fallback */
  }
  try {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.setAttribute("readonly", "");
    textarea.style.position = "absolute";
    textarea.style.left = "-9999px";
    document.body.appendChild(textarea);
    textarea.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(textarea);
    return ok;
  } catch {
    return false;
  }
}

/** Builds a metadata-only detection summary (never credentials, prompts, or responses). */
function buildDetectionSummary(
  snapshot: ConnectionSetupSnapshot | null,
  visibleBindings: AgentUpstreamBinding[],
  lang: "zh" | "en",
): string {
  const lines: string[] = [lang === "zh" ? "d-token 检测摘要（元数据，不含凭据）" : "d-token detection summary (metadata only, no credentials)"];
  lines.push(`${lang === "zh" ? "来源" : "Source"}: ${safeText(snapshot?.source, "—")}`);
  lines.push(`${lang === "zh" ? "修订" : "Revision"}: ${safeText(snapshot?.revision, "—")}`);
  const batchLabel = lang === "zh" ? "自动纳入批次" : "Automatic batch";
  const batch = visibleBindings.map((binding) => {
    const facts = bindingRouteFacts(binding);
    const resolved = binding.resolved_candidate ?? binding.candidates?.[0] ?? null;
    const route = resolved || binding.follow?.current_upstream
      ? `${facts.providerModel} / ${facts.protocol} / ${facts.endpoint}`
      : binding.already_connected
        ? lang === "zh" ? "已接入" : "connected"
        : lang === "zh" ? "来源冲突" : "source conflict";
    return `${binding.agent_name || binding.agent_id} → ${route}`;
  }).join("; ");
  lines.push(`${batchLabel}: ${batch || "—"}`);
  const candidates = snapshot?.candidates ?? [];
  lines.push(`${lang === "zh" ? "相关上游候选" : "Related upstream candidates"}: ${candidates.map((candidate) => candidate.name || candidate.id).join(", ") || "—"}`);
  const ccSwitch = snapshot?.cc_switch_sync?.detected
    ? lang === "zh" ? "已检测到" : "Detected"
    : lang === "zh" ? "未检测到" : "Not detected";
  lines.push(`cc-switch: ${ccSwitch}`);
  return lines.join("\n");
}

export function OnboardingPage({
  gateState,
  reducedMotion = false,
  onComplete,
  onDismiss,
  onStartSetup,
  onOpenOverview,
  onStartService,
}: {
  gateState: OnboardingGateState;
  reducedMotion?: boolean;
  /** Persists the first-config marker; resolves true only when the write succeeded. */
  onComplete: () => Promise<boolean>;
  /** Dismisses the wizard for the current session without persisting completion. */
  onDismiss: () => void;
  /** Enters the real Install & Connect flow for the whole automatic batch. */
  onStartSetup: () => void;
  /** Returns to the Overview page. */
  onOpenOverview: () => void;
  /** Runs the shell-owned service-start transaction and re-reads discovery/runtime. */
  onStartService: () => Promise<OnboardingServiceStartResult>;
}) {
  const t = useT();
  const { lang } = useI18n();
  const { snapshot, status: discoveryStatus, refresh: refreshDiscovery } = useConnectionSetup();
  const runtimeSlice = useRuntimeDashboardSelector(selectRuntimeSlice, shallowEqualSlice);
  // The selectable Agent batch lives in the installMachine store (the same
  // store the Install workbench consumes), so onboarding selection survives
  // navigation into the one-click flow.
  const selectedAgentIds = useSyncExternalStore(subscribeInstallMachine, getInstallMachineState).selectedAgentIds;
  // Keep the installMachine store in sync with the discovery snapshot this
  // wizard consumes: the store is the single source for the selectable batch
  // and its default selections (installed Agents default to checked). The
  // reducer is idempotent for an unchanged revision, so polls preserve any
  // user opt-out.
  useEffect(() => {
    if (snapshot) dispatchInstallMachine({ type: "DISCOVERED", snapshot });
  }, [snapshot]);

  // Local step state: re-entering a completed wizard still starts at welcome.
  const [step, setStep] = useState<StepKey>("welcome");
  const CONFIG_STARTED_KEY = "d-token.onboarding.config-started";
  const [configStarted, setConfigStarted] = useState(() => {
    try {
      return window.sessionStorage.getItem(CONFIG_STARTED_KEY) === "1";
    } catch {
      return false;
    }
  });
  const [copied, setCopied] = useState(false);
  const [serviceAction, setServiceAction] = useState<{ phase: "idle" | "loading" | "success" | "error"; summary: string }>({
    phase: "idle",
    summary: "",
  });
  const serviceStages = useMemo(
    () => [
      { key: "start", label: t("onb.service.stage.start") },
      { key: "confirm", label: t("onb.service.stage.confirm") },
    ],
    [t],
  );

  const serviceStatus = runtimeSlice.operational?.service?.data as DtokenServiceStatus | null;
  const restartRequired = serviceStatus?.state === "restart_required" || serviceStatus?.restart_required === true;
  const serviceOnline = !restartRequired && (serviceStatus
    ? ["running", "unmanaged_running"].includes(serviceStatus.state)
    : runtimeSlice.status === "runtime");
  const serviceLoading = runtimeSlice.status === "loading" && serviceStatus === null;
  const servicePresentation = deriveServicePresentation(serviceOnline, serviceLoading, restartRequired);
  const serviceEndpoint = serviceOnline
    ? serviceStatus?.service_url || safeText(runtimeSlice.payload?.desktop_service_url, "")
    : "";
  const serviceInstanceDetail = presentServiceInstance(
    serviceStatus,
    serviceOnline,
    serviceEndpoint,
    runtimeSlice.updatedAt,
    lang,
  );

  const counts = useMemo(() => connectionDiscoveryCounts(snapshot), [snapshot]);
  const visibleBindings = useMemo(
    () => (snapshot?.agent_bindings ?? []).filter((binding) => binding.installed || binding.already_connected),
    [snapshot?.agent_bindings],
  );
  const batchStatuses = useMemo(
    () => visibleBindings.map((binding) => ({ binding, status: agentBatchStatus(binding, t) })),
    [t, visibleBindings],
  );
  // A conflict blocks the batch only when the conflicting Agent is actually
  // selected. Conflict-only Agents render disabled (not selectable) and never
  // block the user's chosen batch — consistent with P0.0.10.1 independent
  // results: one unsupported Agent must not block every other Agent.
  const batchBlocked = batchStatuses.some(
    ({ binding, status }) => status.conflict && selectedAgentIds.includes(binding.agent_id),
  );
  const ccSwitchStatus = snapshot?.cc_switch_sync?.detected
    ? t("onb.upstream.ccSwitchDetected")
    : t("onb.upstream.ccSwitchNone");
  const discovering = discoveryStatus === "idle" || discoveryStatus === "discovering";
  const discoveryUnavailable = discoveryStatus === "unavailable" || snapshot === null;
  const nativeUnavailable = gateState === "unavailable";

  const serviceCopy = servicePresentation.phase === "running"
    ? { label: t("onb.service.running"), health: "healthy" as const }
    : servicePresentation.phase === "restart-required"
      ? { label: t("onb.service.restartRequired"), health: "warning" as const }
      : servicePresentation.phase === "loading"
        ? { label: t("onb.checking"), health: "warning" as const }
        : { label: t("onb.service.stopped"), health: "error" as const };

  // Completion is an explicit user acknowledgement of the first-run wizard,
  // NOT a connection/verified claim: the ConnectionSetup contract has no
  // PRD P0.7.1 #5 verified field, so the marker must never be gated on (or
  // derived from) `already_connected` / route-follow state. The button stays
  // enabled whenever the native settings store is reachable so the user can
  // explicitly mark the wizard acknowledged as a preference.
  const completeDisabled = nativeUnavailable;

  const stepIndex = STEP_KEYS.indexOf(step);
  const detectionText = buildDetectionSummary(snapshot, visibleBindings, lang);
  const detectedEvidence = evidenceState(discoveryUnavailable ? null : visibleBindings.length > 0);
  const anyConnected = visibleBindings.some((binding) => binding.already_connected);
  const connectedEvidence = evidenceState(
    discoveryUnavailable
      ? null
      : anyConnected
        ? true
        : snapshot?.agent_route_follow?.state === "disabled"
          ? false
          : null,
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

  // Real-state unlock gates. A step is enterable only when its prerequisite
  // fact holds; locked steps are not reachable via the stepper or "下一步".
  const discoveryReady = !discovering && !discoveryUnavailable;
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
    const next = STEP_KEYS[Math.min(STEP_KEYS.indexOf(step) + 1, TOTAL_STEPS - 1)];
    if (next !== step && canEnterStep(next)) setStep(next);
  };
  const goPrev = () => setStep((current) => STEP_KEYS[Math.max(STEP_KEYS.indexOf(current) - 1, 0)]);

  const handleCopy = async () => {
    const ok = await copyText(detectionText);
    setCopied(ok);
  };

  // The service step runs the shell-owned transaction and reports success only
  // when the native receipt AND the post-transaction discovery refresh agree;
  // a receipt without refresh evidence is surfaced as an error, never success.
  const handleStartService = async () => {
    if (nativeUnavailable) return;
    setServiceAction({ phase: "loading", summary: "" });
    const result = await onStartService();
    setServiceAction(
      result.ok && result.refreshed
        ? { phase: "success", summary: result.summary }
        : { phase: "error", summary: result.summary },
    );
  };

  const renderStep = () => {
    switch (step) {
      case "welcome":
        return (
          <div className="space-y-4">
            <p className="text-sm leading-relaxed text-muted-foreground">{t("onb.welcome.leading")}</p>
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="flat-inset flex flex-col gap-2 rounded-lg border border-border/60 p-4">
                <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <Rocket aria-hidden="true" className="size-4 text-primary" />
                  {t("onb.welcome.mode.guided")}
                </div>
                <p className="text-xs leading-relaxed text-muted-foreground">{t("onb.welcome.mode.guidedDesc")}</p>
                <Button type="button" className="mt-2 self-start" onClick={goNext}>{t("onb.welcome.start")}</Button>
              </div>
              <div className="flat-inset flex flex-col gap-2 rounded-lg border border-border/60 p-4">
                <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <ClipboardCopy aria-hidden="true" className="size-4 text-muted-foreground" />
                  {t("onb.welcome.mode.manual")}
                </div>
                <p className="text-xs leading-relaxed text-muted-foreground">{t("onb.welcome.mode.manualDesc")}</p>
                <Button type="button" variant="outline" className="mt-2 self-start" onClick={() => onStartSetup()}>{t("onb.welcome.jump")}</Button>
              </div>
            </div>
          </div>
        );

      case "service":
        return (
          <div className="space-y-3">
            <p className="text-sm leading-relaxed text-muted-foreground">{t("onb.service.desc")}</p>
            <Surface>
              <div className="p-4">
                <StepProgress
                  steps={serviceStages}
                  current={serviceAction.phase === "loading" ? 0 : 1}
                  done={serviceAction.phase === "success"}
                  reducedMotion={reducedMotion}
                />
              </div>
            </Surface>
            <Surface>
              <FlatRows>
                <FlatRow className="items-center gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-foreground">{t("onb.section.service")}</div>
                    <div className="mt-0.5 truncate text-xs text-muted-foreground" title={serviceInstanceDetail}>
                      {serviceInstanceDetail}
                    </div>
                  </div>
                  <StatusBadge status={serviceCopy.health} label={serviceCopy.label} />
                </FlatRow>
              </FlatRows>
            </Surface>
            <div className="flex flex-wrap items-center gap-3">
              <Button
                type="button"
                onClick={() => void handleStartService()}
                disabled={nativeUnavailable || serviceAction.phase === "loading"}
              >
                {serviceAction.phase === "loading"
                  ? t("onb.service.starting")
                  : serviceAction.phase === "error"
                    ? t("onb.service.retry")
                    : t("onb.service.start")}
              </Button>
              <p className="min-w-0 flex-1 text-xs text-muted-foreground" role="status" aria-live="polite">
                {serviceAction.phase === "success"
                  ? t("onb.service.startSuccess")
                  : serviceAction.phase === "error"
                    ? t("onb.service.startFailed")
                    : t("onb.previewNote")}
              </p>
            </div>
          </div>
        );

      case "agents":
        return (
          <div className="space-y-3">
            <p className="text-sm leading-relaxed text-muted-foreground">{t("onb.agents.desc")}</p>
            <Surface>
              <FlatRows>
                <FlatRow className="items-center gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-foreground">{t("onb.section.agents")}</div>
                    <div className="mt-0.5 text-xs text-muted-foreground">
                      {discovering
                        ? t("onb.checking")
                        : discoveryUnavailable
                          ? t("onb.agents.unavailable")
                          : counts.agents === 0
                            ? t("onb.agents.none")
                            : t("onb.agents.detected", { count: counts.agents })}
                    </div>
                    {!discovering && !discoveryUnavailable && batchStatuses.length > 0 && (
                      <ul className="mt-2 grid gap-2" aria-label={t("onb.agents.heading")}>
                        {batchStatuses.map(({ binding, status }) => {
                          const selectable = canJoinInstallBatch(binding);
                          const checked = selectedAgentIds.includes(binding.agent_id);
                          return (
                            <li
                              key={binding.agent_id}
                              className="flex items-center gap-2 rounded-lg border border-border/60 bg-card/60 px-3 py-2 text-xs text-muted-foreground"
                            >
                              <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2">
                                <input
                                  type="checkbox"
                                  checked={checked}
                                  disabled={!selectable}
                                  onChange={() => dispatchInstallMachine({ type: "TOGGLE_AGENT", agentId: binding.agent_id })}
                                  aria-label={t("onb.agents.select", { name: binding.agent_name || binding.agent_id })}
                                />
                                <span className="min-w-0 flex-1 truncate font-medium text-foreground">{binding.agent_name || binding.agent_id}</span>
                              </label>
                              <span className={cn("inline-flex items-center gap-1", status.autoIncludable ? "text-emerald-600" : "text-rose-600")}>
                                {status.autoIncludable ? <CheckCircle2 aria-hidden="true" className="size-3" /> : <AlertTriangle aria-hidden="true" className="size-3" />}
                                {status.label}
                              </span>
                              {status.conflict && (
                                <span className="text-xs text-amber-600" title={t("onb.agents.conflictFix")}>{t("onb.agents.conflictFix")}</span>
                              )}
                            </li>
                          );
                        })}
                      </ul>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => void refreshDiscovery()}
                      disabled={nativeUnavailable}
                      aria-label={t("visual.refresh")}
                    >
                      <RefreshCw aria-hidden="true" className="size-3.5" />
                    </Button>
                  </div>
                </FlatRow>
              </FlatRows>
            </Surface>
          </div>
        );

      case "config": {
        return (
          <div className="space-y-3">
            <p className="text-sm leading-relaxed text-muted-foreground">{t("onb.config.desc")}</p>
            <Surface>
              <FlatRows>
                <FlatRow className="items-center gap-4">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-foreground">{t("onb.section.upstream")}</div>
                    <div className="mt-0.5 text-xs text-muted-foreground">
                      {discovering
                        ? t("onb.checking")
                        : discoveryUnavailable
                          ? t("onb.status.unavailable")
                          : counts.upstreams === 0
                            ? t("onb.upstream.none")
                            : t("onb.upstream.detected", { count: counts.upstreams })}
                    </div>
                  </div>
                  <div className="shrink-0 text-xs text-muted-foreground">{t("onb.upstream.ccSwitch", { status: ccSwitchStatus })}</div>
                </FlatRow>
              </FlatRows>
            </Surface>
            <Surface className="overflow-hidden">
              <section role="region" aria-labelledby="onboarding-config-batch">
                <h2 id="onboarding-config-batch" className="px-4 pt-4 text-sm font-medium text-foreground">
                  {t("onb.config.batchCount", { count: selectedAgentIds.length })}
                </h2>
                <div className="px-4 pb-4 pt-2">
                  {batchStatuses.map(({ binding, status }) => {
                    const facts = bindingRouteFacts(binding);
                    return (
                      <dl
                        key={binding.agent_id}
                        className="mt-2 grid gap-x-4 gap-y-1.5 rounded-lg border border-border/60 bg-card/40 p-3 text-xs sm:grid-cols-2"
                      >
                        <div className="col-span-full flex items-center gap-2">
                          <dt className="font-medium text-foreground">{binding.agent_name || binding.agent_id}</dt>
                          <span className={cn("inline-flex items-center gap-1", status.autoIncludable ? "text-emerald-600" : "text-rose-600")}>
                            {status.autoIncludable ? <CheckCircle2 aria-hidden="true" className="size-3" /> : <AlertTriangle aria-hidden="true" className="size-3" />}
                            {status.label}
                          </span>
                        </div>
                        <div className="min-w-0">
                          <dt className="text-muted-foreground">{t("onb.config.providerModel")}</dt>
                          <dd className="mt-0.5 truncate text-foreground" title={facts.providerModel}>{facts.providerModel}</dd>
                        </div>
                        <div className="min-w-0">
                          <dt className="text-muted-foreground">{t("onb.config.protocol")}</dt>
                          <dd className="mt-0.5 truncate text-foreground" title={facts.protocol}>{facts.protocol}</dd>
                        </div>
                        <div className="min-w-0">
                          <dt className="text-muted-foreground">{t("onb.config.endpoint")}</dt>
                          <dd className="mt-0.5 truncate text-foreground" title={facts.endpoint}>{facts.endpoint}</dd>
                        </div>
                        <div className="min-w-0">
                          <dt className="text-muted-foreground">{t("onb.config.targetPath")}</dt>
                          <dd className="mt-0.5 truncate text-foreground" title={facts.target}>{facts.target}</dd>
                        </div>
                      </dl>
                    );
                  })}
                </div>
              </section>
            </Surface>
            {batchBlocked && (
              <div role="status" className="flex items-start gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300">
                <AlertTriangle aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
                <span>{t("onb.config.conflictBlocked")}</span>
              </div>
            )}
            <Surface className="overflow-hidden">
              <section role="region" aria-labelledby="onboarding-detection-summary">
                <h2 id="onboarding-detection-summary" className="px-4 pt-4 text-sm font-medium text-foreground">{t("onb.config.heading")}</h2>
                <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted/40 p-4 text-xs leading-relaxed text-foreground">
                  {detectionText}
                </pre>
              </section>
            </Surface>
            <div className="flex flex-wrap items-center gap-3">
              <Button type="button" variant="outline" onClick={() => void handleCopy()} disabled={nativeUnavailable}>
                {copied ? <CheckCircle2 aria-hidden="true" className="size-4 text-emerald-600" /> : <Copy aria-hidden="true" className="size-4" />}
                {copied ? t("onb.config.copied") : t("onb.config.copy")}
              </Button>
              <Button
                type="button"
                onClick={() => {
                  setConfigStarted(true);
                  try {
                    window.sessionStorage.setItem(CONFIG_STARTED_KEY, "1");
                  } catch { /* non-persistent context */ }
                  onStartSetup();
                }}
                disabled={batchBlocked || nativeUnavailable || selectedAgentIds.length === 0}
              >
                {t("onb.config.start")}
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">{t("onb.config.startDesc")}</p>
          </div>
        );
      }

      case "verify": {
        const evidence = [
          { key: "detected", state: detectedEvidence },
          { key: "connected", state: connectedEvidence },
          { key: "verified", state: verifiedEvidence },
        ] as const;
        return (
          <div className="space-y-4">
            <p className="text-sm leading-relaxed text-muted-foreground">{t("onb.verify.desc")}</p>
            <div className="grid gap-3 sm:grid-cols-3">
              {evidence.map(({ key, state }) => (
                <div
                  key={key}
                  role="group"
                  aria-label={t(`onb.verify.${key}Group`)}
                  data-evidence-health={state.health}
                  className="flat-inset rounded-lg border border-border/60 p-4"
                >
                  <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                    {state.health === "healthy"
                      ? <CheckCircle2 aria-hidden="true" className="size-4 text-emerald-600" />
                      : state.health === "warning"
                        ? <AlertTriangle aria-hidden="true" className="size-4 text-amber-600" />
                        : <span aria-hidden="true" className="size-4 rounded-full border border-border/70" />}
                    {t(`onb.verify.${key}`)}
                  </div>
                  <div className="mt-1.5 text-xs font-medium text-muted-foreground">{t(`onb.verify.state.${state.value}`)}</div>
                  <p className="mt-1 text-xs leading-relaxed text-muted-foreground">{t(`onb.verify.${key}Desc`)}</p>
                </div>
              ))}
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <Button
                type="button"
                onClick={() => void onComplete()}
                disabled={completeDisabled}
                aria-disabled={completeDisabled}
                title={nativeUnavailable ? t("onb.preview.unavailable") : undefined}
              >
                {t("onb.verify.finish")}
              </Button>
              <Button type="button" variant="outline" onClick={() => onStartSetup()}>{t("onb.verify.enterWorkbench")}</Button>
            </div>
            <p className="text-xs text-muted-foreground">{t("onb.verify.finishDesc")}</p>
          </div>
        );
      }
    }
  };

  return (
    <div
      className="onboarding-wizard mx-auto w-full max-w-3xl px-6 py-8"
      data-reduced-motion={reducedMotion ? "true" : "false"}
    >
      <h1 className="mb-2">{t("onb.title")}</h1>
      <p className="mb-6 max-w-2xl text-sm leading-relaxed text-muted-foreground">{t("onb.subtitle")}</p>

      {nativeUnavailable && (
        <div role="status" className="mb-4 flex items-start gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300">
          <ShieldAlert aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
          <span>{t("onb.preview.unavailable")}</span>
        </div>
      )}

      {/* Stepper */}
      <ol className="mb-6 flex flex-wrap items-center gap-1.5" aria-label={t("onb.stepOf", { current: stepIndex + 1, total: TOTAL_STEPS })}>
        {STEP_KEYS.map((key, index) => (
          <li key={key} className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => canEnterStep(key) && setStep(key)}
              disabled={!canEnterStep(key)}
              aria-current={key === step ? "step" : undefined}
              className={cn(
                "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs transition-colors",
                key === step
                  ? "border-primary/40 bg-primary/10 font-medium text-foreground"
                  : "border-border/60 bg-card/40 text-muted-foreground hover:bg-card/70 hover:text-foreground",
                !canEnterStep(key) && "cursor-not-allowed opacity-40 hover:bg-card/40",
              )}
            >
              <span className={cn(
                "flex size-4 items-center justify-center rounded-full text-[10px]",
                key === step ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground",
              )}>
                {index + 1}
              </span>
              {t(`onb.step.${key}`)}
            </button>
            {index < TOTAL_STEPS - 1 && <span className="text-muted-foreground/40" aria-hidden="true">→</span>}
          </li>
        ))}
      </ol>

      <p className="mb-4 text-xs text-muted-foreground" aria-live="polite">
        {t("onb.stepOf", { current: stepIndex + 1, total: TOTAL_STEPS })}
      </p>

      {renderStep()}

      {step === "welcome" || step === "verify" ? null : (
        <div className="mt-6 flex flex-wrap items-center gap-3">
          <Button type="button" variant="ghost" onClick={goPrev}>{t("onb.prev")}</Button>
          <Button
            type="button"
            onClick={goNext}
            disabled={!canEnterStep(STEP_KEYS[Math.min(STEP_KEYS.indexOf(step) + 1, TOTAL_STEPS - 1)])}
          >
            {t("onb.next")}
          </Button>
        </div>
      )}

      <div className="mt-6 flex flex-wrap items-center gap-3 border-t border-border/50 pt-4">
        {gateState === "completed" ? (
          <>
            <Button type="button" variant="ghost" onClick={onOpenOverview}>{t("onb.backOverview")}</Button>
            <span className="text-xs text-muted-foreground">{t("onb.completedHint")}</span>
          </>
        ) : (
          <>
            <Button type="button" variant="ghost" onClick={onDismiss}>{t("onb.later")}</Button>
            <span className="text-xs text-muted-foreground">{t("onb.laterDesc")}</span>
          </>
        )}
      </div>
    </div>
  );
}