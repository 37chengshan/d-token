import { useEffect, useRef, useState } from "react";
import {
  Banknote,
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
  Trash2,
} from "lucide-react";
import desktopPackage from "../../../../package.json";
import { Surface } from "../common/Surface";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Switch } from "../ui/switch";
import { useI18n, useT, translateCopy } from "../../lib/i18n";
import {
  ACCENTS,
  CHART_ACCENTS,
  useAppearance,
} from "../../lib/theme-context";
import { cn } from "../ui/utils";
import { invokeNative, type NativeActionResult } from "../../lib/native";
import { useRuntimeDashboardSelector, shallowEqualSlice, type RuntimeDashboardState } from "../../lib/useRuntimeDashboard";
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

/** One user-maintained price row in the local model pricing table. */
type ModelPriceEntry = {
  model: string;
  currency: "USD" | "CNY";
  inputPerMillion: number;
  outputPerMillion: number;
  source: string;
  updatedAt: string;
};

const MODEL_PRICING_CURRENCIES = ["USD", "CNY"] as const;
const MODEL_PRICING_UNIT = "per_1m_tokens";
const MODEL_PRICING_MAX_MODEL_LENGTH = 128;

function createEmptyPriceRow(): ModelPriceEntry {
  return {
    model: "",
    currency: "USD",
    inputPerMillion: 0,
    outputPerMillion: 0,
    source: "user",
    updatedAt: "",
  };
}

/** Settings reads service evidence only; payload changes must not repaint it. */
type SettingsRuntimeSlice = {
  operational: RuntimeDashboardState["operational"];
  refresh: RuntimeDashboardState["refresh"];
};

const selectSettingsRuntime = (state: RuntimeDashboardState): SettingsRuntimeSlice => ({
  operational: state.operational,
  refresh: state.refresh,
});

export function Settings() {
  const t = useT();
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const { operational, refresh: refreshOperational } = useRuntimeDashboardSelector(
    selectSettingsRuntime,
    shallowEqualSlice,
  );
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
  const [pricingRows, setPricingRows] = useState<ModelPriceEntry[]>([]);
  const pricingPersisted = useRef<ModelPriceEntry[]>([]);
  const [pricingLoaded, setPricingLoaded] = useState(false);
  const [pricingMissing, setPricingMissing] = useState(false);
  const [pricingSaving, setPricingSaving] = useState(false);
  const [pricingLoadError, setPricingLoadError] = useState("");
  const [pricingInvalid, setPricingInvalid] = useState("");
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
          ? (translateCopy("set.browserPreviewCannotReadOrSaveLocalPreferences", lang))
          : (translateCopy("set.localPreferencesCouldNotBeReadRelatedSettingsAreUnavailable", lang));
        setPreferencesAvailable(false);
        setPreferencesLoadError(message);
        const failure = presentOperationFailure("settings", error, lang);
        addNotification({ dedupeKey: "settings-preferences-load-failed", level: "warning", title: translateCopy("set.settingsUnavailable", lang), message, details: failure.details });
      });
  }, [addNotification, lang]);

  useEffect(() => {
    void invokeNative<{ state: string; payload?: Record<string, any> }>("get_app_settings", { section: "model-pricing" })
      .then((saved) => {
        const rows = normalizeModelPriceRows(saved.payload?.prices);
        setPricingRows(rows);
        pricingPersisted.current = rows;
        setPricingLoaded(true);
        setPricingMissing(saved.state === "missing");
        setPricingLoadError("");
      })
      .catch((error) => {
        const message = error instanceof Error && error.message === "native_unavailable"
          ? translateCopy("set.pricingPreviewMsg", lang)
          : translateCopy("set.pricingLoadFailedMsg", lang);
        setPricingLoaded(false);
        setPricingMissing(false);
        setPricingLoadError(message);
        const failure = presentOperationFailure("settings", error, lang);
        addNotification({ dedupeKey: "settings-pricing-load-failed", level: "warning", title: translateCopy("set.pricingUnavailable", lang), message, details: failure.details });
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
          summary: translateCopy("set.autostartStatusCouldNotBeRead", lang),
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
      addNotification({ dedupeKey: "settings-preferences-saved", level: "success", title: translateCopy("set.localPreferencesSaved", lang), message: translateCopy("set.localPreferencesWereSavedToDToken", lang), persistent: true, receipt: { requestId, targetId: "preferences" } });
    } catch (error) {
      setPreferences(preferences);
      const failure = presentOperationFailure("settings", error, lang);
      addNotification({ dedupeKey: "settings-preferences-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "preferences" } });
    } finally {
      setPreferencesSaving(false);
    }
  };

  const pricingAvailable = pricingLoaded || pricingMissing;

  const validatePricingRows = (rows: ModelPriceEntry[]): string => {
    const seen = new Set<string>();
    for (let index = 0; index < rows.length; index += 1) {
      const row = rows[index];
      const model = row.model.trim();
      if (!model) {
        return translateCopy("set.pricingErrorEmptyModel", lang, { row: index + 1 });
      }
      if (model.length > MODEL_PRICING_MAX_MODEL_LENGTH) {
        return translateCopy("set.pricingErrorLongModel", lang, { row: index + 1, max: MODEL_PRICING_MAX_MODEL_LENGTH });
      }
      const key = `${model}|${row.currency}`;
      if (seen.has(key)) {
        return translateCopy("set.pricingErrorDuplicate", lang, { model });
      }
      seen.add(key);
      for (const field of ["inputPerMillion", "outputPerMillion"] as const) {
        const value = row[field];
        if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
          return translateCopy("set.pricingErrorPrice", lang, { row: index + 1 });
        }
      }
    }
    return "";
  };

  const savePricing = async (rows: ModelPriceEntry[]) => {
    if (!pricingAvailable) return;
    const invalid = validatePricingRows(rows);
    if (invalid) {
      setPricingInvalid(invalid);
      addNotification({ dedupeKey: "settings-pricing-invalid", level: "warning", title: translateCopy("set.pricingInvalidTitle", lang), message: invalid, persistent: true });
      return;
    }
    setPricingInvalid("");
    const next = rows.map((row) => ({ ...row, model: row.model.trim() }));
    setPricingRows(next);
    setPricingMissing(false);
    setPricingSaving(true);
    const requestId = createOperationRequestId("settings-pricing");
    try {
      const result = await invokeNative<NativeActionResult>("save_app_settings", {
        section: "model-pricing",
        payload: { prices: next, unit: MODEL_PRICING_UNIT, scope: "d-token-local-settings", no_external_agent_write: true },
      });
      if (!result.ok) throw new Error(result.summary || "settings_save_failed");
      pricingPersisted.current = next;
      addNotification({ dedupeKey: "settings-pricing-saved", level: "success", title: translateCopy("set.pricingSavedTitle", lang), message: translateCopy("set.pricingSavedMsg", lang), persistent: true, receipt: { requestId, targetId: "model-pricing" } });
    } catch (error) {
      // Roll back to the last successfully persisted table, not to the
      // current (possibly edited) rows.
      setPricingRows(pricingPersisted.current);
      setPricingInvalid(translateCopy("set.pricingSaveRestored", lang));
      const failure = presentOperationFailure("settings", error, lang);
      addNotification({ dedupeKey: "settings-pricing-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "model-pricing" } });
    } finally {
      setPricingSaving(false);
    }
  };

  const updatePricingRow = (index: number, patch: Partial<ModelPriceEntry>) => {
    setPricingRows((rows) => rows.map((row, rowIndex) => (rowIndex === index ? { ...row, ...patch } : row)));
    if (pricingInvalid) setPricingInvalid("");
  };

  const removePricingRow = (index: number) => {
    setPricingRows((rows) => rows.filter((_, rowIndex) => rowIndex !== index));
    if (pricingInvalid) setPricingInvalid("");
  };

  const addPricingRow = () => {
    setPricingRows((rows) => [...rows, createEmptyPriceRow()]);
    if (pricingInvalid) setPricingInvalid("");
  };

  const restorePricingDefault = () => {
    void savePricing([]);
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
        title: result.ok ? (translateCopy("set.updateCheckComplete", lang)) : (translateCopy("set.updateSourceUnavailable", lang)),
        message: result.ok
          ? (translateCopy("set.signedUpdateInformationWasReadNothingWasDownloadedOrInstalled", lang))
          : (translateCopy("set.noSignedUpdateFeedIsConfiguredNothingWasDownloadedOrInstalled", lang)),
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
      title: translateCopy("set.appearanceSettingsWereNotSaved", lang),
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
        ? (translateCopy("set.theLocalDTokenServiceStarted", lang))
        : (translateCopy("set.theLocalDTokenServiceStopped", lang));
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
        ? (translateCopy("set.autostartAfterSignInIsEnabled", lang))
        : (translateCopy("set.autostartAfterSignInIsDisabled", lang));
      setNotice(success);
      addNotification({
        dedupeKey: enable ? "autostart-enabled" : "autostart-disabled",
        level: "success",
        title: success,
        message: enable
          ? (translateCopy("set.afterSignInTheDTokenAppStartsWithItsControlledServiceArgumentAndAttemptsToStartItsOwnedLocalService", lang))
          : (translateCopy("set.theCurrentUserSDTokenStartupEntryWasRemovedAndVerified", lang)),
        persistent: true,
        receipt: { requestId, targetId: result.status.source },
      });
      setAutostartReview(null);
    } catch (error) {
      addNotification({
        dedupeKey: "autostart-change-failed",
        level: "error",
        title: translateCopy("set.autostartChangeFailed", lang),
        message: error instanceof Error && error.message === "native_unavailable"
          ? (translateCopy("set.theBrowserPreviewCannotChangeSystemStartupEntries", lang))
          : (translateCopy("set.theSystemStartupEntryDidNotPassWriteOrVerificationThePriorStateWasRestored", lang)),
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
        <h1 className="sr-only">{t("set.title")}</h1>
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
        description={translateCopy("set.colorsAndThemeApplyToTheCurrentWindowImmediately", lang)}
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
        title={translateCopy("set.runtimeStartup", lang)}
        description={translateCopy("set.inspectAndManageTheDTokenOwnedLocalService", lang)}
        icon={Server}
      >
        <Row title={translateCopy("onb.service.heading", lang)} desc={serviceView.detail}>
          <div className="flex flex-wrap items-center justify-end gap-2">
            <Badge variant={serviceView.badge}>{serviceView.label}</Badge>
            {serviceCommand ? (
              <Button
                size="sm"
                variant={serviceCommand === "stop" ? "outline" : "default"}
                aria-label={serviceCommand === "start" ? (translateCopy("tray.startService", lang)) : (translateCopy("tray.stopService", lang))}
                disabled={serviceAction !== null}
                onClick={() => serviceCommand === "stop" ? setConfirmServiceStop(true) : void runServiceAction("start")}
              >
                {serviceAction ? <RefreshCw className="size-4 animate-spin" /> : <Power className="size-4" />}
                {serviceAction
                  ? (translateCopy("set.working", lang))
                  : serviceCommand === "start"
                    ? (translateCopy("set.start", lang))
                    : (translateCopy("set.stop", lang))}
              </Button>
            ) : null}
          </div>
        </Row>
        <Row
          title={translateCopy("set.localServiceAddress", lang)}
          desc={translateCopy("set.loopbackOnlyNoLanOrPublicConnections", lang)}
        >
          <span className="break-all font-mono text-sm text-muted-foreground">
            {serviceStatus?.service_url || "—"}
          </span>
        </Row>
        <Row
          title={translateCopy("set.startDTokenAutomatically", lang)}
          desc={describeAutostart(autostartStatus, lang)}
        >
          <div className="flex items-center gap-3">
            <Badge variant={autostartStatus?.state === "enabled" ? "default" : autostartStatus?.state === "error" || autostartStatus?.state === "drift" ? "destructive" : "outline"}>
              {autostartBadge(autostartStatus, lang)}
            </Badge>
            <Switch
              aria-label={translateCopy("set.startDTokenAutomatically", lang)}
              checked={autostartStatus?.enabled === true}
              disabled={!autostartStatus?.supported || autostartStatus.state === "error" || autostartPending}
              onCheckedChange={(checked) => setAutostartReview(checked)}
            />
          </div>
        </Row>
      </Section>

      <Section
        id="privacy"
        title={translateCopy("set.privacyData", lang)}
        description={translateCopy("set.controlLocalRetentionLogsStayHighRedactionAndDiagnosticsAreNeverUploaded", lang)}
        icon={ShieldCheck}
      >
        {preferencesLoadError ? (
          <div className="mb-3 rounded-lg border border-amber-500/25 bg-amber-500/[0.06] p-3 text-sm text-amber-800 dark:text-amber-200" role="status">
            <strong>{translateCopy("set.localPreferencesUnavailable", lang)}</strong>
            <p className="mt-1 text-xs text-muted-foreground">{preferencesLoadError} {translateCopy("set.retryInTheDesktopAppOrOpenDoctorToInspectTheLocalControlPlane", lang)}</p>
          </div>
        ) : null}
        <Row
          title={translateCopy("set.dataRetention", lang)}
          desc={translateCopy("set.usedForEventLedgerCleanupAndExportBoundaries", lang)}
        >
          <AnimatedSegmentedControl
            ariaLabel={translateCopy("set.dataRetention", lang)}
            reducedMotion={reducedMotion}
            value={String(preferences.retentionDays) as "7" | "30"}
            onValueChange={(next) => void savePreferences({ retentionDays: Number(next) as 7 | 30 })}
            items={[
              {
                value: "7",
                label: `7 ${translateCopy("set.days", lang)}`,
                disabled: preferencesSaving || !preferencesAvailable,
              },
              {
                value: "30",
                label: `30 ${translateCopy("set.days", lang)}`,
                disabled: preferencesSaving || !preferencesAvailable,
              },
            ] as const}
          />
        </Row>
        <Row
          title={translateCopy("set.logUploadBoundary", lang)}
          desc={translateCopy("set.keysPathsAndSensitiveAddressesStayRedactedPromptsCodeAndFullOutputsAreNeverUploaded", lang)}
        >
          <Badge variant="secondary">{translateCopy("set.highRedactionUploadOff", lang)}</Badge>
        </Row>
      </Section>

      <Section
        id="learning"
        title={translateCopy("sl.title", lang)}
        description={translateCopy("set.localAggregateLearningProjectScanLivesUnderInstallSmartLearning", lang)}
        icon={BrainCircuit}
      >
        <Row
          title={translateCopy("set.smartLearningFullScan", lang)}
          desc={translateCopy("set.enablesLearningFromLocalAggregateMetricsWithoutReadingRawPromptsResponsesOrCredentials", lang)}
        >
          <Switch aria-label={translateCopy("set.smartLearningFullScan", lang)} checked={preferences.learningEnabled} disabled={preferencesSaving || !preferencesAvailable} onCheckedChange={(checked) => void savePreferences({ learningEnabled: checked })} />
        </Row>
        <Row
          title={translateCopy("set.scanDepth", lang)}
          desc={translateCopy("set.controlsDiscoveryScopeAndLocalResourceUse", lang)}
        >
          <AnimatedSegmentedControl
            ariaLabel={translateCopy("set.scanDepth", lang)}
            reducedMotion={reducedMotion}
            value={preferences.scanDepth}
            onValueChange={(next) => void savePreferences({ scanDepth: next })}
            items={[
              {
                value: "standard",
                label: translateCopy("set.standard", lang),
                disabled: preferencesSaving || !preferencesAvailable,
              },
              {
                value: "deep",
                label: translateCopy("set.deep", lang),
                disabled: preferencesSaving || !preferencesAvailable,
              },
            ] as const}
          />
        </Row>
      </Section>

      <Section
        id="updates"
        title={translateCopy("set.updates", lang)}
        description={translateCopy("set.onlySignedReleaseFeedsAreAcceptedUnavailableIsShownWhenNoFeedIsConfigured", lang)}
        icon={RefreshCw}
      >
        <Row title={translateCopy("set.updateChannel", lang)} desc={translateCopy("set.savedLocallyChecksOnlyUseTheMatchingSignedReleaseFeed", lang)}>
          <AnimatedSegmentedControl
            ariaLabel={translateCopy("set.updateChannel", lang)}
            reducedMotion={reducedMotion}
            value={preferences.updateChannel}
            onValueChange={(next) => void savePreferences({ updateChannel: next })}
            items={[
              {
                value: "stable",
                label: translateCopy("set.stable", lang),
                disabled: preferencesSaving || !preferencesAvailable,
              },
              {
                value: "preview",
                label: translateCopy("set.preview", lang),
                disabled: preferencesSaving || !preferencesAvailable,
              },
            ] as const}
          />
        </Row>
        <Row
          title={translateCopy("set.checkForUpdatesAutomatically", lang)}
          desc={translateCopy("set.checksTheSelectedChannelAfterAppLaunchRecordsUnavailableWhenNoSignedFeedExists", lang)}
        >
          <Switch aria-label={translateCopy("set.checkForUpdatesAutomatically", lang)} checked={preferences.autoUpdateCheck} disabled={preferencesSaving || !preferencesAvailable} onCheckedChange={(checked) => void savePreferences({ autoUpdateCheck: checked })} />
        </Row>
        <Row title={translateCopy("set.checkNow", lang)} desc={updateCheck ? `${updateCheck.state} · ${updateCheck.summary}` : (translateCopy("set.readTheNativeUpdateControlPlaneState", lang))}>
          <Button size="sm" variant="outline" disabled={checkingUpdates || !preferencesAvailable} onClick={() => void checkUpdates()}>
            <RefreshCw className={cn("size-4", checkingUpdates && "animate-spin")} />
            {checkingUpdates ? (translateCopy("set.checking", lang)) : (translateCopy("set.checkUpdates", lang))}
          </Button>
        </Row>
      </Section>

      <Section
        id="modelPricing"
        title={t("set.pricing")}
        description={t("set.pricingDesc")}
        icon={Banknote}
      >
        {pricingLoadError ? (
          <div className="mx-5 mt-4 rounded-lg border border-amber-500/25 bg-amber-500/[0.06] p-3 text-sm text-amber-800 dark:text-amber-200" role="status">
            <strong>{t("set.pricingUnavailable")}</strong>
            <p className="mt-1 text-xs text-muted-foreground">{pricingLoadError}</p>
          </div>
        ) : null}
        <div className="p-5">
          {pricingRows.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              {pricingMissing ? t("set.pricingEmpty") : t("set.pricingEmptySaved")}
            </p>
          ) : (
            <div className="space-y-3">
              {pricingRows.map((row, index) => (
                <div key={index} className="grid grid-cols-2 gap-2 sm:grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_auto]">
                  <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                    <span>{t("set.pricingModel")}</span>
                    <input
                      className="h-9 rounded-md border border-border/70 bg-background px-2.5 text-sm text-foreground outline-none focus:border-primary/60"
                      value={row.model}
                      maxLength={MODEL_PRICING_MAX_MODEL_LENGTH}
                      aria-label={t("set.pricingRowModelAria", { row: index + 1 })}
                      placeholder="gpt-4o"
                      onChange={(event) => updatePricingRow(index, { model: event.target.value })}
                    />
                  </label>
                  <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                    <span>{t("set.pricingCurrency")}</span>
                    <select
                      className="h-9 rounded-md border border-border/70 bg-background px-2.5 text-sm text-foreground outline-none focus:border-primary/60"
                      value={row.currency}
                      aria-label={t("set.pricingRowCurrencyAria", { row: index + 1 })}
                      onChange={(event) => updatePricingRow(index, { currency: event.target.value as "USD" | "CNY" })}
                    >
                      {MODEL_PRICING_CURRENCIES.map((currency) => (
                        <option key={currency} value={currency}>{currency}</option>
                      ))}
                    </select>
                  </label>
                  <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                    <span>{t("set.pricingInput")}</span>
                    <input
                      className="h-9 rounded-md border border-border/70 bg-background px-2.5 text-sm text-foreground outline-none focus:border-primary/60"
                      type="number"
                      min="0"
                      step="0.01"
                      value={Number.isFinite(row.inputPerMillion) ? String(row.inputPerMillion) : ""}
                      aria-label={t("set.pricingRowInputAria", { row: index + 1 })}
                      onChange={(event) => updatePricingRow(index, { inputPerMillion: Number(event.target.value) })}
                    />
                  </label>
                  <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                    <span>{t("set.pricingOutput")}</span>
                    <input
                      className="h-9 rounded-md border border-border/70 bg-background px-2.5 text-sm text-foreground outline-none focus:border-primary/60"
                      type="number"
                      min="0"
                      step="0.01"
                      value={Number.isFinite(row.outputPerMillion) ? String(row.outputPerMillion) : ""}
                      aria-label={t("set.pricingRowOutputAria", { row: index + 1 })}
                      onChange={(event) => updatePricingRow(index, { outputPerMillion: Number(event.target.value) })}
                    />
                  </label>
                  <button
                    type="button"
                    className="self-end rounded-md p-2 text-muted-foreground hover:text-destructive"
                    aria-label={t("set.pricingRowRemoveAria", { row: index + 1 })}
                    onClick={() => removePricingRow(index)}
                  >
                    <Trash2 className="size-4" />
                  </button>
                </div>
              ))}
            </div>
          )}
          {pricingInvalid ? (
            <p className="mt-3 text-sm text-destructive" role="status">{pricingInvalid}</p>
          ) : null}
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <Button size="sm" variant="outline" disabled={pricingSaving || !pricingAvailable} onClick={addPricingRow}>
              {t("set.pricingAdd")}
            </Button>
            <Button size="sm" disabled={pricingSaving || !pricingAvailable} onClick={() => void savePricing(pricingRows)}>
              {pricingSaving ? <RefreshCw className="size-4 animate-spin" /> : <Check className="size-4" />}
              {pricingSaving ? t("set.pricingSaving") : t("set.pricingSave")}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={pricingSaving || !pricingAvailable || (pricingRows.length === 0 && !pricingMissing)}
              onClick={restorePricingDefault}
            >
              {t("set.pricingRestore")}
            </Button>
          </div>
          <p className="mt-3 text-xs text-muted-foreground">
            {t("set.pricingUnitNote")}
          </p>
        </div>
      </Section>

      <Section
        id="about"
        title={t("set.about")}
        description={translateCopy("set.versionInfoSettingsStayInDTokenLocalStateOnly", lang)}
        icon={Info}
      >
        <Row title={t("set.versionLabel")}>
          <span className="font-mono text-sm text-muted-foreground">{desktopPackage.version}</span>
        </Row>
      </Section>
      <OperationDialog
        open={confirmServiceStop}
        onOpenChange={setConfirmServiceStop}
        title={translateCopy("set.stopTheDTokenLocalService", lang)}
        summary={translateCopy("set.newAgentRequestsCannotRouteThroughDTokenAfterItStops", lang)}
        impact={serviceStatus?.service_url ?? "127.0.0.1"}
        confirmLabel={translateCopy("set.stopService", lang)}
        cancelLabel={t("common.cancel")}
        risk="warning"
        pending={serviceAction === "stop"}
        onConfirm={async () => { await runServiceAction("stop"); setConfirmServiceStop(false); }}
      />
      <OperationDialog
        open={autostartReview !== null}
        onOpenChange={(open) => { if (!open) setAutostartReview(null); }}
        title={autostartReview
          ? (translateCopy("set.enableAutostartAfterSignIn", lang))
          : (translateCopy("set.disableAutostartAfterSignIn", lang))}
        summary={autostartReview
          ? (translateCopy("set.createsACurrentUserSystemStartupEntryForDTokenItStartsOnlyTheDTokenAppWithAControlledArgumentThatAttemptsToStartTheDTokenOwnedService", lang))
          : (translateCopy("set.removesAndVerifiesTheCurrentUserSDTokenStartupEntryWithoutStoppingTheCurrentlyRunningService", lang))}
        impact={translateCopy("set.noThirdPartyAgentCcSwitchOrUpstreamConfigurationIsReadOrChanged", lang)}
        confirmLabel={autostartReview ? (translateCopy("set.enable", lang)) : (translateCopy("set.disable", lang))}
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
  id: "runtime" | "privacy" | "learning" | "updates" | "appearance" | "modelPricing" | "about";
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
      label: translateCopy("set.serviceRunning", lang),
      detail: translateCopy("set.managedByThisDTokenDesktopWindow", lang),
      badge: "default",
    };
  }
  if (state === "unmanaged_running") {
    return {
      label: translateCopy("set.serviceAlreadyRunning", lang),
      detail: translateCopy("set.thisProcessWasNotStartedByThisWindowAndWillNotBeForceStoppedHere", lang),
      badge: "secondary",
    };
  }
  if (state === "stopped") {
    return {
      label: translateCopy("set.serviceStopped", lang),
      detail: translateCopy("set.startItBeforeAgentsCanConnectUpstreamThroughDToken", lang),
      badge: "outline",
    };
  }
  if (state === "preview") {
    return {
      label: translateCopy("cp.source.preview", lang),
      detail: translateCopy("set.noRealLocalServiceIsReadOrControlled", lang),
      badge: "outline",
    };
  }
  if (state === "error") {
    return {
      label: translateCopy("set.statusUnavailable", lang),
      detail: translateCopy("set.serviceStateCouldNotBeConfirmedOpenDoctor", lang),
      badge: "destructive",
    };
  }
  return {
    label: translateCopy("set.checking2", lang),
    detail: translateCopy("set.readingLocalServiceState", lang),
    badge: "outline",
  };
}

function settingsBadge(state: SettingsState, saving: boolean, lang: "zh" | "en") {
  if (saving) return translateCopy("set.pricingSaving", lang);
  if (state === "loading") return translateCopy("adv.preferenceLoading", lang);
  if (state === "loaded") return translateCopy("set.localSettingsConnected", lang);
  if (state === "missing") return translateCopy("set.firstUse", lang);
  if (state === "preview") return translateCopy("cp.source.preview", lang);
  return translateCopy("set.loadFailed", lang);
}

function autostartBadge(status: AutostartStatus | null, lang: "zh" | "en") {
  if (!status) return translateCopy("set.checking2", lang);
  if (status.state === "enabled") return translateCopy("common.enabled", lang);
  if (status.state === "disabled") return translateCopy("set.disabled", lang);
  if (status.state === "drift") return translateCopy("set.confirmationRequired", lang);
  if (status.state === "preview") return translateCopy("cp.source.preview", lang);
  if (status.state === "unsupported") return translateCopy("set.unsupportedPlatform", lang);
  return translateCopy("set.statusUnavailable", lang);
}

function describeAutostart(status: AutostartStatus | null, lang: "zh" | "en") {
  if (!status) return translateCopy("set.readingTheCurrentUserSSystemStartupEntry", lang);
  if (status.state === "enabled") {
    return translateCopy("set.startsTheDTokenAppAfterSignInAndAttemptsItsOwnedLocalServiceOnlyThroughAControlledArgument", lang);
  }
  if (status.state === "disabled") return translateCopy("set.dTokenWillNotStartAutomaticallyAfterThisUserSignsIn", lang);
  if (status.state === "drift") return translateCopy("set.aMismatchedDTokenStartupEntryWasFoundAndIsNotReportedAsEnabled", lang);
  if (status.state === "preview") return translateCopy("set.theBrowserPreviewCannotReadOrChangeSystemStartupEntries", lang);
  if (status.state === "unsupported") return translateCopy("set.autostartIsSupportedOnMacosAndWindowsOnly", lang);
  return translateCopy("set.theSystemStartupEntryCouldNotBeVerifiedChangesAreUnavailable", lang);
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
    return translateCopy("set.browserPreviewAppearanceAppliesToThisSessionOnlyAndIsNotPresentedAsSaved", lang);
  }
  if (notice.startsWith("save_failed:")) {
    return translateCopy("set.saveFailedTheCurrentWindowKeepsYourAppearanceChoice", lang);
  }
  return translateCopy("set.settingsSaved", lang);
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

/**
 * Normalizes a `model-pricing` payload from the native settings contract into
 * editable rows. Unknown entries are dropped; malformed prices stay "unknown"
 * (never coerced to zero) by falling back to a fresh empty row for the model
 * id, so the user sees exactly what was persisted instead of a fabricated 0.
 */
function normalizeModelPriceRows(value: unknown): ModelPriceEntry[] {
  if (!Array.isArray(value)) return [];
  const rows: ModelPriceEntry[] = [];
  for (const raw of value) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) continue;
    const entry = raw as Record<string, unknown>;
    const model = typeof entry.model === "string" ? entry.model.trim() : "";
    if (!model) continue;
    const currency = entry.currency === "CNY" ? "CNY" : "USD";
    const inputPerMillion = typeof entry.inputPerMillion === "number" && Number.isFinite(entry.inputPerMillion) && entry.inputPerMillion >= 0
      ? entry.inputPerMillion
      : Number.NaN;
    const outputPerMillion = typeof entry.outputPerMillion === "number" && Number.isFinite(entry.outputPerMillion) && entry.outputPerMillion >= 0
      ? entry.outputPerMillion
      : Number.NaN;
    rows.push({
      model,
      currency,
      inputPerMillion,
      outputPerMillion,
      source: typeof entry.source === "string" ? entry.source : "user",
      updatedAt: typeof entry.updatedAt === "string" ? entry.updatedAt : "",
    });
  }
  return rows;
}
