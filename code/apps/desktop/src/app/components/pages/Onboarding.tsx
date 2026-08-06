import { useMemo } from "react";
import { AlertTriangle, CheckCircle2, RefreshCw, Rocket, ShieldAlert } from "lucide-react";
import { useI18n, useT } from "../../lib/i18n";
import { useConnectionSetup, type AgentUpstreamBinding } from "../../lib/connectionSetup";
import { useRuntimeDashboardSelector, shallowEqualSlice, type RuntimeDashboardState } from "../../lib/useRuntimeDashboard";
import { connectionDiscoveryCounts, presentServiceInstance, safeText } from "../../lib/install/presentation";
import type { DtokenServiceStatus } from "../../lib/install/presentation";
import { deriveOnboardingEvidence, type OnboardingGateState } from "../../lib/onboarding";
import { Surface, FlatRows, FlatRow } from "../common/Surface";
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
 */

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

function bindingStatus(binding: AgentUpstreamBinding, t: (key: string) => string): { label: string; health: "healthy" | "warning" | "error" } {
  if (binding.already_connected) return { label: t("onb.agents.connected"), health: "healthy" };
  if (binding.resolution === "needs-credential") return { label: t("onb.agents.needsCredential"), health: "warning" };
  if (binding.resolution === "unsupported") return { label: t("onb.agents.unsupported"), health: "warning" };
  if (binding.resolution === "unavailable") return { label: t("onb.agents.unavailable"), health: "error" };
  return { label: t("onb.agents.needsChoice"), health: "warning" };
}

export function OnboardingPage({
  gateState,
  reducedMotion = false,
  onComplete,
  onDismiss,
  onStartSetup,
  onOpenOverview,
}: {
  gateState: OnboardingGateState;
  reducedMotion?: boolean;
  /** Persists the first-config marker; resolves true only when the write succeeded. */
  onComplete: () => Promise<boolean>;
  /** Dismisses the wizard for the current session without persisting completion. */
  onDismiss: () => void;
  /** Enters the real Install & Connect flow (preview → confirm → apply → verify). */
  onStartSetup: () => void;
  /** Returns to the Overview page. */
  onOpenOverview: () => void;
}) {
  const t = useT();
  const { lang } = useI18n();
  const { snapshot, status: discoveryStatus, refresh: refreshDiscovery } = useConnectionSetup();
  const runtimeSlice = useRuntimeDashboardSelector(selectRuntimeSlice, shallowEqualSlice);

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
  const evidence = useMemo(() => deriveOnboardingEvidence(snapshot), [snapshot]);
  const visibleBindings = useMemo(
    () => (snapshot?.agent_bindings ?? []).filter((binding) => binding.installed || binding.already_connected),
    [snapshot?.agent_bindings],
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

  const completeDisabled = nativeUnavailable || !evidence.hasConnectedEvidence;

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

      <Surface className="mb-5">
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
              {!discovering && !discoveryUnavailable && visibleBindings.length > 0 && (
                <ul className="mt-2 flex flex-wrap gap-1.5">
                  {visibleBindings.map((binding) => {
                    const status = bindingStatus(binding, t);
                    return (
                      <li
                        key={binding.agent_id}
                        className="inline-flex items-center gap-1.5 rounded-full border border-border/60 bg-card/60 px-2.5 py-1 text-xs text-muted-foreground"
                      >
                        <span className="max-w-40 truncate font-medium text-foreground">{binding.agent_name || binding.agent_id}</span>
                        <span className={cn("inline-flex items-center gap-1", status.health === "healthy" ? "text-emerald-600" : status.health === "warning" ? "text-amber-600" : "text-rose-600")}>
                          {status.health === "healthy" ? <CheckCircle2 aria-hidden="true" className="size-3" /> : <AlertTriangle aria-hidden="true" className="size-3" />}
                          {status.label}
                        </span>
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

      <p className="mb-6 flex items-start gap-2 text-xs leading-relaxed text-muted-foreground">
        <Rocket aria-hidden="true" className="mt-0.5 size-3.5 shrink-0" />
        <span>{t("onb.previewNote")}</span>
      </p>

      {gateState === "completed" ? (
        <div role="status" className="mb-6 rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-4 py-3 text-sm text-emerald-700 dark:text-emerald-300">
          {t("onb.completedDetail")}
        </div>
      ) : evidence.hasConnectedEvidence ? (
        <div role="status" className="mb-6 rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-4 py-3 text-sm text-emerald-700 dark:text-emerald-300">
          {t("onb.detectedConnected")}
        </div>
      ) : (
        <div role="status" className="mb-6 rounded-lg border border-border/70 bg-card/60 px-4 py-3 text-sm text-muted-foreground">
          {t("onb.notConfigured")}
        </div>
      )}

      <div className="flex flex-wrap items-center gap-3">
        {gateState === "completed" ? (
          <Button type="button" onClick={onOpenOverview}>{t("onb.backOverview")}</Button>
        ) : (
          <>
            <Button
              type="button"
              onClick={() => void onComplete()}
              disabled={completeDisabled}
              aria-disabled={completeDisabled}
              title={completeDisabled ? (nativeUnavailable ? t("onb.preview.unavailable") : t("onb.completeDesc")) : undefined}
            >
              {t("onb.complete")}
            </Button>
            <Button type="button" variant="outline" onClick={onStartSetup}>{t("onb.startSetup")}</Button>
            <Button type="button" variant="ghost" onClick={onDismiss}>{t("onb.later")}</Button>
          </>
        )}
      </div>

      <p className="mt-3 text-xs text-muted-foreground">
        {gateState === "completed" ? t("onb.completedHint") : t("onb.laterDesc")}
      </p>
    </div>
  );
}
