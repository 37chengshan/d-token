import { useCallback, useMemo, useState } from "react";
import { Activity, Coins, Gauge, PanelTop, Power, Server } from "lucide-react";
import { AppProviders } from "./lib/theme-context";
import { RuntimeEvidenceProvider, shallowEqualSlice, useRuntimeDashboardSelector, type RuntimeDashboardState } from "./lib/useRuntimeDashboard";
import { useI18n, translateCopy } from "./lib/i18n";
import { overviewFromPayload } from "./lib/overview/derivations";
import { OverviewTrendChart } from "./lib/overview/components";
import { invokeNative, isTauriRuntime } from "./lib/native";
import { presentOperationFailure } from "./lib/userCopy";

/**
 * Resident monitoring surface behind the tray icon.
 *
 * Every number here comes from the same shared runtime evidence the control
 * plane uses, so the panel cannot present a state the main window contradicts.
 * Writes stay in the main window, where confirmation and rollback live; the
 * panel only starts or stops the service it already reports on.
 */

type PanelSlice = {
  payload: RuntimeDashboardState["payload"];
  status: RuntimeDashboardState["status"];
  operational: RuntimeDashboardState["operational"];
  refresh: RuntimeDashboardState["refresh"];
};

const selectPanelRuntime = (state: RuntimeDashboardState): PanelSlice => ({
  payload: state.payload,
  status: state.status,
  operational: state.operational,
  refresh: state.refresh,
});

function formatCount(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return String(value);
}

function TrayPanel() {
  const { lang } = useI18n();
  const { payload, status, operational, refresh } = useRuntimeDashboardSelector(
    selectPanelRuntime,
    shallowEqualSlice,
  );
  const [pendingAction, setPendingAction] = useState<"start" | "stop" | null>(null);
  const [actionNotice, setActionNotice] = useState("");

  const overview = useMemo(() => overviewFromPayload(payload, status, lang), [payload, status, lang]);
  const service = operational.service.data;
  const serviceRunning = service?.state === "running";
  const serviceUrl = typeof service?.service_url === "string" ? service.service_url : "";

  const openControlPlane = useCallback(() => {
    if (!isTauriRuntime()) return;
    void invokeNative("show_control_plane").catch(() => setActionNotice(
      presentOperationFailure("generic", "native_unavailable", lang).message,
    ));
  }, [lang]);

  const toggleService = useCallback(async () => {
    if (!isTauriRuntime() || pendingAction) return;
    const action = serviceRunning ? "stop" : "start";
    setPendingAction(action);
    setActionNotice("");
    try {
      const result = await invokeNative<{ ok?: boolean; summary?: string }>(
        action === "start" ? "start_dtoken_service" : "stop_dtoken_service",
      );
      // Native lifecycle commands report failure in the payload, not by throwing.
      if (!result?.ok) throw new Error(result?.summary || "native_action_failed");
      await refresh();
    } catch (reason) {
      setActionNotice(presentOperationFailure("service", reason, lang).message);
    } finally {
      setPendingAction(null);
    }
  }, [lang, pendingAction, refresh, serviceRunning]);

  return (
    <div className="tray-panel">
      <header className="tray-panel-header">
        <div className="min-w-0">
          <div className="tray-panel-title">d-token</div>
          <div className="tray-panel-source" title={overview.sourceLabel}>{overview.sourceLabel}</div>
        </div>
        <button
          type="button"
          className="tray-panel-icon-button"
          onClick={openControlPlane}
          title={translateCopy("tray.openTheControlPlane", lang)}
          aria-label={translateCopy("tray.openTheControlPlane", lang)}
        >
          <PanelTop className="size-4" />
        </button>
      </header>

      <section className="tray-panel-metrics">
        <Metric
          icon={Coins}
          label={translateCopy("tray.physicalTokenReduction", lang)}
          value={overview.metricsAvailable ? formatCount(overview.stats.savedTotal) : "—"}
        />
        <Metric
          icon={Gauge}
          label={translateCopy("tray.physicalReductionRate", lang)}
          value={overview.compressionAvailable ? `${(overview.stats.compressionRate * 100).toFixed(1)}%` : "—"}
        />
        <Metric
          icon={Activity}
          label={translateCopy("tray.routedRequests", lang)}
          value={overview.trafficAvailable ? formatCount(overview.stats.requestCount) : "—"}
        />
      </section>

      <section className="tray-panel-trend">
        {overview.trend.length > 0
          ? <OverviewTrendChart data={overview.trend} lang={lang} />
          : <div className="tray-panel-empty">{translateCopy("tray.noRequestRecordsToChartYet", lang)}</div>}
      </section>

      <section className="tray-panel-service">
        <div className="flex min-w-0 items-center gap-2">
          <Server className={`size-4 shrink-0 ${serviceRunning ? "text-emerald-500" : "text-muted-foreground"}`} />
          <div className="min-w-0">
            <div className="tray-panel-service-state">
              {operational.service.state === "loading"
                ? (translateCopy("tray.readingServiceState", lang))
                : serviceRunning
                  ? (translateCopy("tray.localServiceRunning", lang))
                  : service
                    ? (translateCopy("tray.localServiceStopped", lang))
                    : (translateCopy("tray.serviceStateUnavailable", lang))}
            </div>
            {serviceUrl && <div className="tray-panel-service-url">{serviceUrl}</div>}
          </div>
        </div>
        <button
          type="button"
          className="tray-panel-button"
          onClick={() => void toggleService()}
          disabled={!isTauriRuntime() || pendingAction !== null || !service}
        >
          <Power className="size-3.5" />
          {pendingAction === "start"
            ? (translateCopy("cbm.state.starting", lang))
            : pendingAction === "stop"
              ? (translateCopy("tray.stopping", lang))
              : serviceRunning
                ? (translateCopy("tray.stopService", lang))
                : (translateCopy("tray.startService", lang))}
        </button>
      </section>

      {actionNotice && <div role="status" className="tray-panel-notice">{actionNotice}</div>}
    </div>
  );
}

function Metric({ icon: Icon, label, value }: { icon: typeof Coins; label: string; value: string }) {
  return (
    <div className="tray-panel-metric">
      <div className="tray-panel-metric-label">
        <Icon className="size-3.5 shrink-0" />
        <span className="min-w-0 truncate" title={label}>{label}</span>
      </div>
      <div className="tray-panel-metric-value">{value}</div>
    </div>
  );
}

export default function TrayPanelApp() {
  return (
    <AppProviders>
      <RuntimeEvidenceProvider>
        <TrayPanel />
      </RuntimeEvidenceProvider>
    </AppProviders>
  );
}
