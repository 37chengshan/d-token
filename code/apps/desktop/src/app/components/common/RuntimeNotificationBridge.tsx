import { useEffect, useRef } from "react";
import { useI18n, translateCopy } from "../../lib/i18n";
import { useNotifications } from "../../lib/notifications";
import { createOperationRequestId } from "../../lib/notifications";
import { shallowEqualSlice, useRuntimeDashboardSelector, type RuntimeDashboardState, type RuntimeDashboardStatus } from "../../lib/useRuntimeDashboard";
import { invokeNative, isTauriRuntime } from "../../lib/native";
import { presentOperationFailure } from "../../lib/userCopy";

type ObservableServiceState = "running" | "stopped" | "unavailable";

type RuntimeNotificationSlice = {
  runtimeStatus: RuntimeDashboardStatus;
  serviceSliceState: RuntimeDashboardState["operational"]["service"]["state"];
  serviceState: string;
};

const selectRuntimeNotificationSlice = (state: RuntimeDashboardState): RuntimeNotificationSlice => ({
  runtimeStatus: state.status,
  serviceSliceState: state.operational.service.state,
  serviceState: typeof state.operational.service.data?.state === "string" ? state.operational.service.data.state : "",
});

/**
 * Converts shared runtime evidence into user-visible receipts. The first
 * observation is deliberately silent; polling must only report transitions.
 */
export function RuntimeNotificationBridge() {
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const runtime = useRuntimeDashboardSelector(selectRuntimeNotificationSlice, shallowEqualSlice);
  const previous = useRef<ObservableServiceState | null>(null);
  const autoUpdateStarted = useRef(false);

  const current = observableServiceState(runtime.serviceState, runtime.serviceSliceState, runtime.runtimeStatus);

  useEffect(() => {
    if (!current) return;
    if (previous.current === null) {
      previous.current = current;
      return;
    }
    if (previous.current === current) return;

    const before = previous.current;
    previous.current = current;
    const requestId = createOperationRequestId("runtime-transition");
    if (current === "running") {
      addNotification({
        dedupeKey: "runtime-service-running",
        level: "success",
        title: translateCopy("rtb.localServiceConnected", lang),
        message: translateCopy("rtb.theDTokenLocalServiceIsRunningAgainAndPageDataIsSyncing", lang),
        receipt: { requestId, targetId: "local-service" },
      });
      return;
    }

    addNotification({
      dedupeKey: `runtime-service-${current}`,
      level: current === "stopped" ? "info" : "warning",
      title: current === "stopped"
        ? (translateCopy("tray.localServiceStopped", lang))
        : (translateCopy("rtb.localServiceConnectionLost", lang)),
      message: current === "stopped"
        ? (translateCopy("rtb.theServiceStoppedAsRequestedRealDataWillResumeAfterRestart", lang))
        : (translateCopy("rtb.theLocalServiceStateCannotBeReadCheckInstallConnectOrDoctor", lang)),
      details: `transition:${before}->${current}`,
      receipt: { requestId, targetId: "local-service" },
    });
  }, [addNotification, current, lang]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    if (autoUpdateStarted.current) return;
    autoUpdateStarted.current = true;
    let cancelled = false;

    void invokeNative<{ state?: string; payload?: Record<string, any> }>("get_app_settings", { section: "preferences" })
      .then(async (saved) => {
        const preferences = saved.payload?.preferences ?? saved.payload ?? {};
        if (preferences.autoUpdateCheck !== true || cancelled) return;
        const channel = preferences.updateChannel === "preview" ? "preview" : "stable";
        const result = await invokeNative<AutoUpdateCheckResult>("check_for_updates", { channel });
        if (cancelled) return;
        addNotification(autoUpdateNotification(result, channel, lang));
      })
      .catch((error) => {
        if (cancelled) return;
        const failure = presentOperationFailure("update", error, lang);
        addNotification({
          dedupeKey: "automatic-update-check-failed",
          level: "warning",
          title: failure.title,
          message: failure.message,
          details: failure.details,
          receipt: { requestId: createOperationRequestId("automatic-update-check"), targetId: "update-feed" },
        });
      });

    return () => {
      cancelled = true;
    };
  }, [addNotification, lang]);

  return null;
}

type AutoUpdateCheckResult = {
  ok?: boolean;
  state?: string;
  summary?: string;
  latest_version?: string | null;
  signature_verified?: boolean;
};

export function autoUpdateNotification(
  result: AutoUpdateCheckResult,
  channel: "stable" | "preview",
  lang: "zh" | "en",
) {
  const updateAvailable = result.ok === true && result.state === "update_available" && result.signature_verified === true;
  const verifiedCurrent = result.ok === true && result.signature_verified === true;
  return {
    dedupeKey: updateAvailable ? "automatic-update-available" : verifiedCurrent ? "automatic-update-current" : "automatic-update-unavailable",
    level: updateAvailable ? "action-required" as const : verifiedCurrent ? "info" as const : "warning" as const,
    title: updateAvailable
      ? (translateCopy("rtb.signedUpdateAvailable", lang))
      : verifiedCurrent
        ? (translateCopy("rtb.dTokenIsUpToDate", lang))
        : (translateCopy("rtb.signedUpdateFeedUnavailable", lang)),
    message: updateAvailable
      ? (translateCopy("rtb.aSignedVersionIsAvailableNothingHasBeenDownloadedOrInstalled", lang))
      : verifiedCurrent
        ? (translateCopy("rtb.theCurrentVersionWasConfirmedByTheSignedUpdateFeed", lang))
        : (translateCopy("rtb.noSignedUpdateInformationWasReadNothingWasDownloadedOrInstalled", lang)),
    persistent: updateAvailable || !verifiedCurrent,
    receipt: {
      requestId: createOperationRequestId("automatic-update-check"),
      targetId: `update:${channel}${result.latest_version ? `:${result.latest_version}` : ""}`,
    },
  };
}

export function observableServiceState(
  serviceState: string,
  sliceState: string,
  runtimeStatus: RuntimeDashboardStatus = "loading",
): ObservableServiceState | null {
  if (serviceState === "running" || serviceState === "unmanaged_running") return "running";
  if (serviceState === "stopped") return "stopped";
  if (sliceState === "unavailable" && runtimeStatus === "runtime") return "running";
  if (sliceState === "unavailable") return "unavailable";
  return null;
}
