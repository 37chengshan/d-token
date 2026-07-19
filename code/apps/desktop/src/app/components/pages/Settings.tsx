import { useEffect, useState } from "react";
import {
  BrainCircuit,
  Check,
  Globe,
  Info,
  Monitor,
  Moon,
  Palette,
  Power,
  RefreshCw,
  Server,
  ShieldCheck,
  Sun,
} from "lucide-react";
import desktopPackage from "../../../../package.json";
import { Surface } from "../common/Surface";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Switch } from "../ui/switch";
import { useI18n, useT } from "../../lib/i18n";
import {
  ACCENTS,
  CHART_ACCENTS,
  useAppearance,
} from "../../lib/theme-context";
import { cn } from "../ui/utils";
import { invokeNative, type NativeActionResult } from "../../lib/native";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";
import { OperationDialog } from "../common/OperationDialog";
import { AnimatedSegmentedControl } from "../shell/AnimatedSegmentedControl";

type SettingsState = "loading" | "loaded" | "preview" | "missing" | "error";
type ServiceState = "loading" | "running" | "unmanaged_running" | "stopped" | "preview" | "error";
type AutostartStatus = {
  supported: boolean;
  state: "enabled" | "disabled" | "drift" | "unsupported" | "preview" | "error";
  enabled: boolean;
  source: string;
  target?: string | null;
  executable?: string | null;
  service_argument: string;
  summary: string;
};
type AutostartActionResult = {
  ok: boolean;
  summary: string;
  rollback_performed: boolean;
  status: AutostartStatus;
};
type ProductPreferences = {
  retentionDays: 7 | 30;
  redaction: "high";
  learningEnabled: boolean;
  scanDepth: "standard" | "deep";
  updateChannel: "stable" | "preview";
  autoUpdateCheck: boolean;
};

const DEFAULT_PRODUCT_PREFERENCES: ProductPreferences = {
  retentionDays: 30,
  redaction: "high",
  learningEnabled: false,
  scanDepth: "standard",
  updateChannel: "stable",
  autoUpdateCheck: false,
};

export function Settings() {
  const t = useT();
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const { operational, refresh: refreshOperational } = useRuntimeDashboard();
  const {
    theme,
    setTheme,
    accent,
    setAccent,
    chartAccent,
    setChartAccent,
    loadState: settingsState,
    saving,
    persistenceNotice,
    updateAppearance,
  } = useAppearance();
  const [notice, setNotice] = useState("");
  const [serviceStatus, setServiceStatus] = useState<DtokenServiceStatus | null>(null);
  const [serviceState, setServiceState] = useState<ServiceState>("loading");
  const [serviceAction, setServiceAction] = useState<"start" | "stop" | null>(null);
  const [confirmServiceStop, setConfirmServiceStop] = useState(false);
  const [autostartStatus, setAutostartStatus] = useState<AutostartStatus | null>(null);
  const [autostartPending, setAutostartPending] = useState(false);
  const [autostartReview, setAutostartReview] = useState<boolean | null>(null);
  const [preferences, setPreferences] = useState(DEFAULT_PRODUCT_PREFERENCES);
  const [preferencesSaving, setPreferencesSaving] = useState(false);
  const [preferencesAvailable, setPreferencesAvailable] = useState(false);
  const [preferencesLoadError, setPreferencesLoadError] = useState("");
  const [updateCheck, setUpdateCheck] = useState<UpdateCheckResult | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [reducedMotion, setReducedMotion] = useState(
    () =>
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const sync = () => setReducedMotion(preference.matches);
    sync();
    preference.addEventListener("change", sync);
    return () => preference.removeEventListener("change", sync);
  }, []);

  useEffect(() => {
    void invokeNative<{ state: string; payload?: Record<string, any> }>("get_app_settings", { section: "preferences" })
      .then((saved) => {
        setPreferences(normalizeProductPreferences(saved.payload?.preferences ?? saved.payload));
        setPreferencesAvailable(saved.state === "loaded" || saved.state === "missing");
        setPreferencesLoadError("");
      })
      .catch((error) => {
        const message = error instanceof Error && error.message === "native_unavailable"
          ? (lang === "zh" ? "浏览器预览不能读取或保存本机偏好。" : "Browser preview cannot read or save local preferences.")
          : (lang === "zh" ? "本机偏好读取失败；相关设置暂不可用。" : "Local preferences could not be read; related settings are unavailable.");
        setPreferencesAvailable(false);
        setPreferencesLoadError(message);
        const failure = presentOperationFailure("settings", error, lang);
        addNotification({ dedupeKey: "settings-preferences-load-failed", level: "warning", title: lang === "zh" ? "设置暂不可用" : "Settings unavailable", message, details: failure.details });
      });
  }, [addNotification, lang]);

  useEffect(() => {
    let cancelled = false;
    void invokeNative<AutostartStatus>("get_autostart_status")
      .then((status) => { if (!cancelled) setAutostartStatus(status); })
      .catch((error) => {
        if (cancelled) return;
        setAutostartStatus({
          supported: false,
          state: error instanceof Error && error.message === "native_unavailable" ? "preview" : "error",
          enabled: false,
          source: "unavailable",
          service_argument: "--dtoken-autostart-service",
          summary: lang === "zh" ? "无法读取自动启动状态。" : "Autostart status could not be read.",
        });
      });
    return () => { cancelled = true; };
  }, []);

  const savePreferences = async (patch: Partial<ProductPreferences>) => {
    if (!preferencesAvailable) return;
    const next = { ...preferences, ...patch };
    const requestId = createOperationRequestId("settings-preferences");
    setPreferences(next);
    setPreferencesSaving(true);
    try {
      const result = await invokeNative<NativeActionResult>("save_app_settings", {
        section: "preferences",
        payload: { preferences: next, scope: "d-token-local-settings", no_external_agent_write: true },
      });
      if (!result.ok) throw new Error(result.summary || "settings_save_failed");
      addNotification({ dedupeKey: "settings-preferences-saved", level: "success", title: lang === "zh" ? "本地偏好已保存" : "Local preferences saved", message: lang === "zh" ? "本地偏好已保存到 d-token。" : "Local preferences were saved to d-token.", persistent: true, receipt: { requestId, targetId: "preferences" } });
    } catch (error) {
      setPreferences(preferences);
      const failure = presentOperationFailure("settings", error, lang);
      addNotification({ dedupeKey: "settings-preferences-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "preferences" } });
    } finally {
      setPreferencesSaving(false);
    }
  };

  const checkUpdates = async () => {
    const requestId = createOperationRequestId("update-check");
    setCheckingUpdates(true);
    try {
      const result = await invokeNative<UpdateCheckResult>("check_for_updates", { channel: preferences.updateChannel });
      setUpdateCheck(result);
      addNotification({
        dedupeKey: "update-check-result",
        level: result.ok ? "success" : "warning",
        title: result.ok ? (lang === "zh" ? "更新检查完成" : "Update check complete") : (lang === "zh" ? "更新源不可用" : "Update source unavailable"),
        message: result.ok
          ? (lang === "zh" ? "已读取经过签名验证的更新信息；未下载或安装任何内容。" : "Signed update information was read; nothing was downloaded or installed.")
          : (lang === "zh" ? "尚未配置经过签名验证的发布源；没有下载或安装任何内容。" : "No signed update feed is configured; nothing was downloaded or installed."),
        persistent: !result.ok,
        receipt: { requestId, targetId: `update:${preferences.updateChannel}` },
      });
    } catch (error) {
      setUpdateCheck(null);
      const failure = presentOperationFailure("update", error, lang);
      addNotification({ dedupeKey: "update-check-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "update" } });
    } finally {
      setCheckingUpdates(false);
    }
  };

  const refreshServiceStatus = refreshOperational;

  useEffect(() => {
    const status = operational.service.data as DtokenServiceStatus | null;
    if (status && ["running", "unmanaged_running", "stopped"].includes(status.state)) {
      setServiceStatus(status);
      setServiceState(status.state);
    } else if (operational.service.state === "unavailable") {
      setServiceStatus(null);
      setServiceState("error");
    }
  }, [operational.service.data, operational.service.state]);

  useEffect(() => {
    if (!persistenceNotice.startsWith("save_failed:")) return;
    addNotification({
      dedupeKey: "appearance-save-failed",
      level: "warning",
      title: lang === "zh" ? "外观设置保存失败" : "Appearance settings were not saved",
      message: describePersistenceNotice(persistenceNotice, lang),
    });
  }, [addNotification, lang, persistenceNotice]);

  const runServiceAction = async (action: "start" | "stop") => {
    setServiceAction(action);
    setNotice("");
    try {
      const result = await invokeNative<NativeActionResult>(
        action === "start" ? "start_dtoken_service" : "stop_dtoken_service",
      );
      if (!result.ok) throw new Error(result.summary || "service_action_failed");
      const success = action === "start"
        ? (lang === "zh" ? "d-token 本地服务已启动。" : "The local d-token service started.")
        : (lang === "zh" ? "d-token 本地服务已停止。" : "The local d-token service stopped.");
      setNotice(success);
      addNotification({ dedupeKey: `settings-service-${action}`, level: "success", title: success, message: success });
      await refreshServiceStatus();
    } catch (error) {
      const failure = presentOperationFailure("service", error, lang);
      setNotice(failure.message);
      addNotification({ dedupeKey: `settings-service-${action}-failed`, level: "error", title: failure.title, message: failure.message, details: failure.details });
    } finally {
      setServiceAction(null);
    }
  };

  const applyAutostart = async () => {
    if (autostartReview === null) return;
    const enable = autostartReview;
    const requestId = createOperationRequestId(enable ? "autostart-enable" : "autostart-disable");
    setAutostartPending(true);
    try {
      const result = await invokeNative<AutostartActionResult>(enable ? "enable_autostart" : "disable_autostart");
      setAutostartStatus(result.status);
      if (!result.ok) throw new Error(result.summary);
      const success = enable
        ? (lang === "zh" ? "已启用登录后自动启动。" : "Autostart after sign-in is enabled.")
        : (lang === "zh" ? "已关闭登录后自动启动。" : "Autostart after sign-in is disabled.");
      setNotice(success);
      addNotification({
        dedupeKey: enable ? "autostart-enabled" : "autostart-disabled",
        level: "success",
        title: success,
        message: enable
          ? (lang === "zh" ? "系统登录后会使用受控参数启动 d-token App，并尝试启动 d-token 自有本地服务。" : "After sign-in, the d-token App starts with its controlled service argument and attempts to start its owned local service.")
          : (lang === "zh" ? "当前用户的 d-token 启动项已移除并验证。" : "The current user's d-token startup entry was removed and verified."),
        persistent: true,
        receipt: { requestId, targetId: result.status.source },
      });
      setAutostartReview(null);
    } catch (error) {
      addNotification({
        dedupeKey: "autostart-change-failed",
        level: "error",
        title: lang === "zh" ? "自动启动设置失败" : "Autostart change failed",
        message: error instanceof Error && error.message === "native_unavailable"
          ? (lang === "zh" ? "浏览器预览无法修改系统启动项。" : "The browser preview cannot change system startup entries.")
          : (lang === "zh" ? "系统启动项未通过写入或验证，已恢复原有状态。" : "The system startup entry did not pass write or verification. The prior state was restored."),
        receipt: { requestId, targetId: "autostart" },
      });
    } finally {
      setAutostartPending(false);
    }
  };

  const serviceView = describeService(serviceState, lang);
  const canStart = serviceState === "stopped";
  const canStop = serviceState === "running" && serviceStatus?.managed === true;
  const serviceCommand = canStart ? "start" : canStop ? "stop" : null;
  const themeOpts = [
    { k: "light" as const, label: t("set.themeLight"), Icon: Sun },
    { k: "dark" as const, label: t("set.themeDark"), Icon: Moon },
    { k: "system" as const, label: t("set.themeSystem"), Icon: Monitor },
  ];
  return (
    <div className="space-y-8">
      <div className="flex flex-wrap items-center gap-3">
        <h1>{t("set.title")}</h1>
        <Badge variant={saving ? "secondary" : settingsState === "loaded" ? "default" : "outline"}>
          {settingsBadge(settingsState, saving, lang)}
        </Badge>
      </div>
      {(notice || persistenceNotice) && (
        <Surface variant="row" role="status" className="p-3 text-sm text-muted-foreground">
          {notice || describePersistenceNotice(persistenceNotice, lang)}
        </Surface>
      )}

      {/* Appearance first: color is the highest-frequency visual control; theme/lang follow. */}
      <Section
        id="appearance"
        title={t("set.appearance")}
        description={lang === "zh" ? "颜色与主题立即作用于当前窗口。" : "Colors and theme apply to the current window immediately."}
        icon={Palette}
      >
        <Row title={t("set.accent")} desc={t("set.accentDesc")}>
          <div className="flex flex-wrap items-center gap-2.5">
            {ACCENTS.map((item) => (
              <Swatch
                key={item.key}
                color={item.color}
                active={accent === item.key}
                label={`${t("set.accent")}: ${item.key}`}
                onClick={() => setAccent(item.key)}
              />
            ))}
          </div>
        </Row>

        <Row title={t("set.chartAccent")} desc={t("set.chartAccentDesc")}>
          <div className="flex flex-wrap items-center gap-2.5">
            {CHART_ACCENTS.map((item) => (
              <Swatch
                key={item.key}
                color={item.color}
                active={chartAccent === item.key}
                label={`${t("set.chartAccent")}: ${item.key}`}
                onClick={() => setChartAccent(item.key)}
              />
            ))}
          </div>
        </Row>

        <Row title={t("set.theme")}>
          <AnimatedSegmentedControl
            ariaLabel={t("set.theme")}
            reducedMotion={reducedMotion}
            value={theme}
            onValueChange={setTheme}
            items={themeOpts.map((option) => ({
              value: option.k,
              label: option.label,
              icon: <option.Icon className="size-4" />,
            }))}
          />
        </Row>

        <Row title={t("set.language")}>
          <AnimatedSegmentedControl
            ariaLabel={t("set.language")}
            reducedMotion={reducedMotion}
            value={lang}
            onValueChange={(next) => updateAppearance({ lang: next })}
            items={[
              { value: "zh", label: "中文", icon: <Globe className="size-4" /> },
              { value: "en", label: "English", icon: <Globe className="size-4" /> },
            ] as const}
          />
        </Row>
      </Section>

      <Section
        id="runtime"
        title={lang === "zh" ? "运行与启动" : "Runtime & Startup"}
        description={lang === "zh" ? "查看并管理 d-token 自有本地服务。" : "Inspect and manage the d-token-owned local service."}
        icon={Server}
      >
        <Row title={lang === "zh" ? "本地服务" : "Local service"} desc={serviceView.detail}>
          <div className="flex flex-wrap items-center justify-end gap-2">
            <Badge variant={serviceView.badge}>{serviceView.label}</Badge>
            {serviceCommand ? (
              <Button
                size="sm"
                variant={serviceCommand === "stop" ? "outline" : "default"}
                aria-label={serviceCommand === "start" ? (lang === "zh" ? "启动服务" : "Start service") : (lang === "zh" ? "停止服务" : "Stop service")}
                disabled={serviceAction !== null}
                onClick={() => serviceCommand === "stop" ? setConfirmServiceStop(true) : void runServiceAction("start")}
              >
                {serviceAction ? <RefreshCw className="size-4 animate-spin" /> : <Power className="size-4" />}
                {serviceAction
                  ? (lang === "zh" ? "处理中" : "Working")
                  : serviceCommand === "start"
                    ? (lang === "zh" ? "启动" : "Start")
                    : (lang === "zh" ? "停止" : "Stop")}
              </Button>
            ) : null}
          </div>
        </Row>
        <Row
          title={lang === "zh" ? "本地服务地址" : "Local service address"}
          desc={lang === "zh" ? "只监听本机，不接受局域网或公网连接。" : "Loopback-only; no LAN or public connections."}
        >
          <span className="break-all font-mono text-sm text-muted-foreground">
            {serviceStatus?.service_url || "—"}
          </span>
        </Row>
        <Row
          title={lang === "zh" ? "自动启动 d-token 服务" : "Start d-token automatically"}
          desc={describeAutostart(autostartStatus, lang)}
        >
          <div className="flex items-center gap-3">
            <Badge variant={autostartStatus?.state === "enabled" ? "default" : autostartStatus?.state === "error" || autostartStatus?.state === "drift" ? "destructive" : "outline"}>
              {autostartBadge(autostartStatus, lang)}
            </Badge>
            <Switch
              aria-label={lang === "zh" ? "自动启动 d-token 服务" : "Start d-token automatically"}
              checked={autostartStatus?.enabled === true}
              disabled={!autostartStatus?.supported || autostartStatus.state === "error" || autostartPending}
              onCheckedChange={(checked) => setAutostartReview(checked)}
            />
          </div>
        </Row>
      </Section>

      <Section
        id="privacy"
        title={lang === "zh" ? "隐私与数据" : "Privacy & Data"}
        description={lang === "zh" ? "控制本机记录保留；日志始终高级隐藏，不上传诊断。" : "Control local retention; logs stay high-redaction and diagnostics are never uploaded."}
        icon={ShieldCheck}
      >
        {preferencesLoadError ? (
          <div className="mb-3 rounded-lg border border-amber-500/25 bg-amber-500/[0.06] p-3 text-sm text-amber-800 dark:text-amber-200" role="status">
            <strong>{lang === "zh" ? "本机偏好暂不可用" : "Local preferences unavailable"}</strong>
            <p className="mt-1 text-xs text-muted-foreground">{preferencesLoadError} {lang === "zh" ? "请在桌面窗口重试，或打开 Doctor 检查本地控制面。" : "Retry in the desktop app or open Doctor to inspect the local control plane."}</p>
          </div>
        ) : null}
        <Row
          title={lang === "zh" ? "数据保留时间" : "Data retention"}
          desc={lang === "zh" ? "供 Event Ledger 清理与导出边界使用。" : "Used for Event Ledger cleanup and export boundaries."}
        >
          <AnimatedSegmentedControl
            ariaLabel={lang === "zh" ? "数据保留时间" : "Data retention"}
            reducedMotion={reducedMotion}
            value={String(preferences.retentionDays) as "7" | "30"}
            onValueChange={(next) => void savePreferences({ retentionDays: Number(next) as 7 | 30 })}
            items={[
              {
                value: "7",
                label: `7 ${lang === "zh" ? "天" : "days"}`,
                disabled: preferencesSaving || !preferencesAvailable,
              },
              {
                value: "30",
                label: `30 ${lang === "zh" ? "天" : "days"}`,
                disabled: preferencesSaving || !preferencesAvailable,
              },
            ] as const}
          />
        </Row>
        <Row
          title={lang === "zh" ? "日志与上传边界" : "Log & upload boundary"}
          desc={lang === "zh" ? "密钥、路径与敏感地址始终隐藏；不上传提示词、代码或完整输出。" : "Keys, paths, and sensitive addresses stay redacted; prompts, code, and full outputs are never uploaded."}
        >
          <Badge variant="secondary">{lang === "zh" ? "高隐藏 · 上传关闭" : "High redaction · upload off"}</Badge>
        </Row>
      </Section>

      <Section
        id="learning"
        title={lang === "zh" ? "智能学习" : "Smart Learning"}
        description={lang === "zh" ? "本地聚合指标学习；项目扫描在「安装与连接 → 智能学习」。" : "Local aggregate learning; project scan lives under Install → Smart Learning."}
        icon={BrainCircuit}
      >
        <Row
          title={lang === "zh" ? "智能学习全面扫描" : "Smart Learning full scan"}
          desc={lang === "zh" ? "启用本地聚合指标学习；不会读取原始提示词、响应或凭据。" : "Enables learning from local aggregate metrics without reading raw prompts, responses, or credentials."}
        >
          <Switch aria-label={lang === "zh" ? "智能学习全面扫描" : "Smart Learning full scan"} checked={preferences.learningEnabled} disabled={preferencesSaving || !preferencesAvailable} onCheckedChange={(checked) => void savePreferences({ learningEnabled: checked })} />
        </Row>
        <Row
          title={lang === "zh" ? "扫描强度" : "Scan depth"}
          desc={lang === "zh" ? "决定识别范围与本机资源占用。" : "Controls discovery scope and local resource use."}
        >
          <AnimatedSegmentedControl
            ariaLabel={lang === "zh" ? "扫描强度" : "Scan depth"}
            reducedMotion={reducedMotion}
            value={preferences.scanDepth}
            onValueChange={(next) => void savePreferences({ scanDepth: next })}
            items={[
              {
                value: "standard",
                label: lang === "zh" ? "标准" : "Standard",
                disabled: preferencesSaving || !preferencesAvailable,
              },
              {
                value: "deep",
                label: lang === "zh" ? "深入" : "Deep",
                disabled: preferencesSaving || !preferencesAvailable,
              },
            ] as const}
          />
        </Row>
      </Section>

      <Section
        id="updates"
        title={lang === "zh" ? "更新" : "Updates"}
        description={lang === "zh" ? "只接受经过签名验证的发布源；当前未配置发布源时会明确显示不可用。" : "Only signed release feeds are accepted; unavailable is shown when no feed is configured."}
        icon={RefreshCw}
      >
        <Row title={lang === "zh" ? "更新通道" : "Update channel"} desc={lang === "zh" ? "选择会保存到 d-token 本地设置；检查时只使用经过签名验证的对应发布源。" : "Saved locally; checks only use the matching signed release feed."}>
          <AnimatedSegmentedControl
            ariaLabel={lang === "zh" ? "更新通道" : "Update channel"}
            reducedMotion={reducedMotion}
            value={preferences.updateChannel}
            onValueChange={(next) => void savePreferences({ updateChannel: next })}
            items={[
              {
                value: "stable",
                label: lang === "zh" ? "稳定版" : "Stable",
                disabled: preferencesSaving || !preferencesAvailable,
              },
              {
                value: "preview",
                label: lang === "zh" ? "预览版" : "Preview",
                disabled: preferencesSaving || !preferencesAvailable,
              },
            ] as const}
          />
        </Row>
        <Row
          title={lang === "zh" ? "自动检查更新" : "Check for updates automatically"}
          desc={lang === "zh" ? "应用启动后按所选通道检查；没有签名发布源时明确记录为不可用。" : "Checks the selected channel after app launch; records unavailable when no signed feed exists."}
        >
          <Switch aria-label={lang === "zh" ? "自动检查更新" : "Check for updates automatically"} checked={preferences.autoUpdateCheck} disabled={preferencesSaving || !preferencesAvailable} onCheckedChange={(checked) => void savePreferences({ autoUpdateCheck: checked })} />
        </Row>
        <Row title={lang === "zh" ? "立即检查" : "Check now"} desc={updateCheck ? `${updateCheck.state} · ${updateCheck.summary}` : (lang === "zh" ? "读取原生更新控制面的真实状态。" : "Read the native update control-plane state.")}>
          <Button size="sm" variant="outline" disabled={checkingUpdates || !preferencesAvailable} onClick={() => void checkUpdates()}>
            <RefreshCw className={cn("size-4", checkingUpdates && "animate-spin")} />
            {checkingUpdates ? (lang === "zh" ? "检查中" : "Checking") : (lang === "zh" ? "检查更新" : "Check updates")}
          </Button>
        </Row>
      </Section>

      <Section
        id="about"
        title={t("set.about")}
        description={lang === "zh" ? "版本信息；设置仅保存在 d-token 本地。" : "Version info; settings stay in d-token local state only."}
        icon={Info}
      >
        <Row title={t("set.versionLabel")}>
          <span className="font-mono text-sm text-muted-foreground">{desktopPackage.version}</span>
        </Row>
      </Section>
      <OperationDialog
        open={confirmServiceStop}
        onOpenChange={setConfirmServiceStop}
        title={lang === "zh" ? "停止 d-token 本地服务？" : "Stop the d-token local service?"}
        summary={lang === "zh" ? "停止后新的 Agent 请求将无法通过 d-token。" : "New agent requests cannot route through d-token after it stops."}
        impact={serviceStatus?.service_url ?? "127.0.0.1"}
        confirmLabel={lang === "zh" ? "确认停止" : "Stop service"}
        cancelLabel={t("common.cancel")}
        risk="warning"
        pending={serviceAction === "stop"}
        onConfirm={async () => { await runServiceAction("stop"); setConfirmServiceStop(false); }}
      />
      <OperationDialog
        open={autostartReview !== null}
        onOpenChange={(open) => { if (!open) setAutostartReview(null); }}
        title={autostartReview
          ? (lang === "zh" ? "启用登录后自动启动？" : "Enable autostart after sign-in?")
          : (lang === "zh" ? "关闭登录后自动启动？" : "Disable autostart after sign-in?")}
        summary={autostartReview
          ? (lang === "zh" ? "将为当前用户创建 d-token 系统启动项。它只启动 d-token App，并携带受控参数来尝试启动 d-token 自有服务。" : "Creates a current-user system startup entry for d-token. It starts only the d-token App with a controlled argument that attempts to start the d-token-owned service.")
          : (lang === "zh" ? "将移除并验证当前用户的 d-token 系统启动项，不会停止当前正在运行的服务。" : "Removes and verifies the current user's d-token startup entry without stopping the currently running service.")}
        impact={lang === "zh" ? "不会读取或修改第三方 Agent、cc-switch 或上游配置。" : "No third-party Agent, cc-switch, or upstream configuration is read or changed."}
        confirmLabel={autostartReview ? (lang === "zh" ? "确认启用" : "Enable") : (lang === "zh" ? "确认关闭" : "Disable")}
        cancelLabel={t("common.cancel")}
        risk={autostartStatus?.state === "drift" ? "warning" : "normal"}
        pending={autostartPending}
        onConfirm={applyAutostart}
      />
    </div>
  );
}

function Section({
  id,
  title,
  description,
  icon: Icon,
  children,
}: {
  id: "runtime" | "privacy" | "learning" | "updates" | "appearance" | "about";
  title: string;
  description: string;
  icon: typeof Server;
  children: React.ReactNode;
}) {
  return (
    <section data-settings-section={id}>
      <div className="mb-3 flex items-start gap-3">
        <div className="mt-0.5 flex size-8 items-center justify-center rounded-md bg-primary/10 text-primary">
          <Icon className="size-4" />
        </div>
        <div>
          <h2 className="text-base font-semibold">{title}</h2>
          <p className="mt-0.5 text-sm text-muted-foreground">{description}</p>
        </div>
      </div>
      <div className="settings-group divide-y divide-border/55 overflow-hidden rounded-xl border border-border/70 bg-card/72 shadow-[0_1px_2px_rgba(15,23,42,0.035)]">
        {children}
      </div>
    </section>
  );
}

function Row({ title, desc, children }: { title: string; desc?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-4 p-5 sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0 sm:pr-6">
        <div className="font-medium">{title}</div>
        {desc && <div className="mt-1 max-w-2xl text-sm leading-5 text-muted-foreground">{desc}</div>}
      </div>
      <div className="min-w-0 sm:flex sm:max-w-[62%] sm:shrink-0 sm:justify-end">{children}</div>
    </div>
  );
}

function Swatch({ color, active, label, onClick }: { color: string; active: boolean; label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      aria-pressed={active}
      title={label}
      className={cn(
        "flex size-7 items-center justify-center rounded-full ring-2 ring-offset-2 ring-offset-background transition-all",
        active ? "ring-foreground/40" : "ring-transparent hover:ring-border",
      )}
      style={{ background: color }}
    >
      {active && <Check className="size-3.5 text-white" />}
    </button>
  );
}

function describeService(
  state: ServiceState,
  lang: "zh" | "en",
): { label: string; detail: string; badge: "default" | "secondary" | "outline" | "destructive" } {
  if (state === "running") {
    return {
      label: lang === "zh" ? "服务正在运行" : "Service running",
      detail: lang === "zh" ? "由当前 d-token 桌面窗口管理。" : "Managed by this d-token desktop window.",
      badge: "default",
    };
  }
  if (state === "unmanaged_running") {
    return {
      label: lang === "zh" ? "服务已在运行" : "Service already running",
      detail: lang === "zh" ? "该进程不是由当前窗口启动，因此这里不会强行停止。" : "This process was not started by this window and will not be force-stopped here.",
      badge: "secondary",
    };
  }
  if (state === "stopped") {
    return {
      label: lang === "zh" ? "服务未启动" : "Service stopped",
      detail: lang === "zh" ? "启动后，Agent 才能通过 d-token 连接上游。" : "Start it before agents can connect upstream through d-token.",
      badge: "outline",
    };
  }
  if (state === "preview") {
    return {
      label: lang === "zh" ? "浏览器预览" : "Browser preview",
      detail: lang === "zh" ? "没有读取或控制真实本地服务。" : "No real local service is read or controlled.",
      badge: "outline",
    };
  }
  if (state === "error") {
    return {
      label: lang === "zh" ? "状态读取失败" : "Status unavailable",
      detail: lang === "zh" ? "无法确认服务是否运行，请打开 Doctor。" : "Service state could not be confirmed; open Doctor.",
      badge: "destructive",
    };
  }
  return {
    label: lang === "zh" ? "正在检查" : "Checking",
    detail: lang === "zh" ? "正在读取本地服务状态。" : "Reading local service state.",
    badge: "outline",
  };
}

function settingsBadge(state: SettingsState, saving: boolean, lang: "zh" | "en") {
  if (saving) return lang === "zh" ? "保存中" : "Saving";
  if (state === "loading") return lang === "zh" ? "读取中" : "Loading";
  if (state === "loaded") return lang === "zh" ? "本地设置已连接" : "Local settings connected";
  if (state === "missing") return lang === "zh" ? "首次使用" : "First use";
  if (state === "preview") return lang === "zh" ? "浏览器预览" : "Browser preview";
  return lang === "zh" ? "读取失败" : "Load failed";
}

function autostartBadge(status: AutostartStatus | null, lang: "zh" | "en") {
  if (!status) return lang === "zh" ? "正在检查" : "Checking";
  if (status.state === "enabled") return lang === "zh" ? "已启用" : "Enabled";
  if (status.state === "disabled") return lang === "zh" ? "已关闭" : "Disabled";
  if (status.state === "drift") return lang === "zh" ? "需要确认" : "Confirmation required";
  if (status.state === "preview") return lang === "zh" ? "浏览器预览" : "Browser preview";
  if (status.state === "unsupported") return lang === "zh" ? "当前平台不支持" : "Unsupported platform";
  return lang === "zh" ? "状态读取失败" : "Status unavailable";
}

function describeAutostart(status: AutostartStatus | null, lang: "zh" | "en") {
  if (!status) return lang === "zh" ? "正在读取当前用户的系统启动项。" : "Reading the current user's system startup entry.";
  if (status.state === "enabled") {
    return lang === "zh"
      ? "登录后启动 d-token App，并仅通过受控参数尝试启动 d-token 自有本地服务。"
      : "Starts the d-token App after sign-in and attempts its owned local service only through a controlled argument.";
  }
  if (status.state === "disabled") return lang === "zh" ? "当前用户登录后不会自动启动 d-token。" : "d-token will not start automatically after this user signs in.";
  if (status.state === "drift") return lang === "zh" ? "检测到内容不匹配的 d-token 启动项；不会把它误报为已启用。" : "A mismatched d-token startup entry was found and is not reported as enabled.";
  if (status.state === "preview") return lang === "zh" ? "浏览器预览不能读取或修改系统启动项。" : "The browser preview cannot read or change system startup entries.";
  if (status.state === "unsupported") return lang === "zh" ? "自动启动仅支持 macOS 与 Windows。" : "Autostart is supported on macOS and Windows only.";
  return lang === "zh" ? "无法验证系统启动项；暂不能修改。" : "The system startup entry could not be verified. Changes are unavailable.";
}

type DtokenServiceStatus = {
  state: "running" | "unmanaged_running" | "stopped";
  managed: boolean;
  service_url: string;
  summary: string;
};

type UpdateCheckResult = {
  ok: boolean;
  state: "available" | "up_to_date" | "unavailable" | "invalid_channel";
  source: string;
  channel: "stable" | "preview";
  current_version?: string;
  latest_version?: string | null;
  signature_verified?: boolean;
  summary: string;
};

function describePersistenceNotice(notice: string, lang: "zh" | "en") {
  if (notice === "session_only") {
    return lang === "zh"
      ? "浏览器预览：外观只在当前会话生效，没有伪装成本地已保存。"
      : "Browser preview: appearance applies to this session only and is not presented as saved.";
  }
  if (notice.startsWith("save_failed:")) {
    return lang === "zh" ? "保存失败；当前窗口的外观选择仍会保留。" : "Save failed; the current window keeps your appearance choice.";
  }
  return lang === "zh" ? "设置已保存。" : "Settings saved.";
}

function normalizeProductPreferences(value: unknown): ProductPreferences {
  const item = value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
  return {
    retentionDays: item.retentionDays === 7 ? 7 : 30,
    redaction: "high",
    learningEnabled: item.learningEnabled === true,
    scanDepth: item.scanDepth === "deep" ? "deep" : "standard",
    updateChannel: item.updateChannel === "preview" ? "preview" : "stable",
    autoUpdateCheck: item.autoUpdateCheck === true,
  };
}
