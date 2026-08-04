import { useEffect, useRef } from "react";
import { useI18n } from "../../lib/i18n";
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
        title: lang === "zh" ? "本地服务已连接" : "Local service connected",
        message: lang === "zh"
          ? "d-token 本地服务恢复运行，页面数据正在同步。"
          : "The d-token local service is running again and page data is syncing.",
        receipt: { requestId, targetId: "local-service" },
      });
      return;
    }

    addNotification({
      dedupeKey: `runtime-service-${current}`,
      level: current === "stopped" ? "info" : "warning",
      title: current === "stopped"
        ? (lang === "zh" ? "本地服务已停止" : "Local service stopped")
        : (lang === "zh" ? "本地服务连接中断" : "Local service connection lost"),
      message: current === "stopped"
        ? (lang === "zh" ? "服务已按操作停止；重新启动后将继续同步真实数据。" : "The service stopped as requested. Real data will resume after restart.")
        : (lang === "zh" ? "无法读取本地服务状态，请在安装与连接或 Doctor 中检查。" : "The local service state cannot be read. Check Install & Connect or Doctor."),
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
      ? (lang === "zh" ? "发现经过签名验证的更新" : "Signed update available")
      : verifiedCurrent
        ? (lang === "zh" ? "当前已是最新版本" : "d-token is up to date")
        : (lang === "zh" ? "签名更新源不可用" : "Signed update feed unavailable"),
    message: updateAvailable
      ? (lang === "zh" ? "已发现经过签名验证的可用版本；尚未下载或安装。" : "A signed version is available; nothing has been downloaded or installed.")
      : verifiedCurrent
        ? (lang === "zh" ? "当前版本已通过签名更新源确认。" : "The current version was confirmed by the signed update feed.")
        : (lang === "zh" ? "未读取到经过签名验证的更新信息；未下载或安装任何内容。" : "No signed update information was read; nothing was downloaded or installed."),
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
