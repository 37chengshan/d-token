import { useEffect, useMemo, useState } from "react";
import { Activity, Bot, CircleAlert, Gauge, Route, Server } from "lucide-react";
import { loadModelPriceCache, type ModelPriceCache } from "../../lib/analytics/pricing";
import { isTauriRuntime } from "../../lib/native";
import { MetricStrip, Surface } from "../common/Surface";
import { Heatmap } from "../common/Heatmap";
import { useI18n, useT, type Lang } from "../../lib/i18n";
import type { PageId } from "../../lib/nav";
import { Badge } from "../ui/badge";
import { useRuntimeDashboardSelector, shallowEqualSlice, type RuntimeDashboardState } from "../../lib/useRuntimeDashboard";
import { classifyEvidence, type EvidenceState } from "../../lib/state/evidence";
import { useConnectionSetup } from "../../lib/connectionSetup";
import {
  deriveAccountQuotaSummary,
  deriveOverviewOperationalSummary,
  overviewFromPayload,
} from "../../lib/overview/derivations";
import { deriveSavingsWindows, type SavingsWindowKey } from "../../lib/overview/savings";
import {
  AccountQuotaCard,
  CcSwitchOperationalItem,
  IssueSummary,
  OperationalItem,
  OverviewKpi,
  OverviewTrendChart,
  SavingsWindowCard,
} from "../../lib/overview/components";

export {
  deriveAccountQuotaSummary,
  deriveOverviewOperationalSummary,
  overviewFromPayload,
} from "../../lib/overview/derivations";
export { deriveSavingsWindows } from "../../lib/overview/savings";
export type {
  AccountQuotaSummary,
  AccountQuotaWindow,
  OverviewOperationalSummary,
} from "../../lib/overview/derivations";
export type { SavingsDerivation, SavingsWindowKey, SavingsWindowSummary } from "../../lib/overview/savings";

function fmt(n: number) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

/** Overview reads the summary evidence only; other slices must not repaint it. */
type SummaryRuntimeSlice = {
  payload: RuntimeDashboardState["payload"];
  status: RuntimeDashboardState["status"];
  updatedAt: RuntimeDashboardState["updatedAt"];
};

const selectSummaryRuntime = (state: RuntimeDashboardState): SummaryRuntimeSlice => ({
  payload: state.payload,
  status: state.status,
  updatedAt: state.updatedAt,
});

export function Overview(_props: { setPage: (p: PageId) => void }) {
  const t = useT();
  const { lang } = useI18n();
  const { payload, status: payloadStatus, updatedAt } = useRuntimeDashboardSelector(
    selectSummaryRuntime,
    shallowEqualSlice,
  );
  const connectionSetup = useConnectionSetup();
  const evidenceState = useMemo(
    () => classifyEvidence({ payload, status: payloadStatus, requireRuntimeIdentity: true }),
    [payload, payloadStatus],
  );
  const effectiveStatus = evidenceState === "verified" ? "runtime" : evidenceState === "preview" ? "browser-preview" : evidenceState;
  const runtime = useMemo(() => overviewFromPayload(payload, effectiveStatus, lang), [effectiveStatus, lang, payload]);
  const operational = useMemo(
    () => deriveOverviewOperationalSummary(payload, effectiveStatus, lang, {
      snapshot: connectionSetup.snapshot,
      status: connectionSetup.status,
    }),
    [connectionSetup.snapshot, connectionSetup.status, effectiveStatus, lang, payload],
  );
  const accountQuotas = useMemo(
    () => deriveAccountQuotaSummary(payload, evidenceState, lang),
    [evidenceState, lang, payload],
  );
  const [priceCache, setPriceCache] = useState<ModelPriceCache | null>(null);
  useEffect(() => {
    // 仅在 native 环境异步读取 Settings model-pricing；错误/预览保持 null（不猜测）。
    // 挂载时读取一次即可：设置保存后重新进入页面会刷新，无需实时广播。
    if (!isTauriRuntime()) return;
    let cancelled = false;
    loadModelPriceCache().then((cache) => {
      if (!cancelled) setPriceCache(cache);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  const savings = useMemo(() => deriveSavingsWindows(payload, evidenceState, lang, undefined, priceCache), [evidenceState, lang, payload, priceCache]);
  const [savingsWindow, setSavingsWindow] = useState<SavingsWindowKey>("total");
  const [reducedMotion] = useState(
    () =>
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const { stats, compressionAvailable, trafficAvailable, sourceLabel: unavailableDetail } = runtime;

  return <div className="overview-page space-y-4">
    <h1 className="sr-only">{t("ov.title")}</h1>

    <MetricStrip data-overview-section="kpis" data-testid="overview-kpis">
      <OverviewKpi label={t("ov.routedRequests")} value={trafficAvailable ? fmt(stats.requestCount) : "—"} detail={trafficAvailable ? (lang === "zh" ? "来自 d-token 本地请求记录" : "From local d-token request records") : unavailableDetail} icon={Activity} />
      <SavingsWindowCard derivation={savings} selected={savingsWindow} onSelect={setSavingsWindow} reducedMotion={reducedMotion} lang={lang} />
      <OverviewKpi label={lang === "zh" ? "物理减少比例" : "Physical reduction rate"} value={compressionAvailable ? `${(stats.compressionRate * 100).toFixed(1)}%` : "—"} detail={compressionAvailable ? (lang === "zh" ? "仅基于同时具备压缩前后值的请求" : "Only requests with both before and after values are included") : unavailableDetail} icon={Gauge} />
      <OverviewKpi label={lang === "zh" ? "请求错误" : "Request errors"} value={trafficAvailable ? fmt(stats.errorsToday) : "—"} detail={trafficAvailable ? runtime.errorLabel : unavailableDetail} icon={CircleAlert} tone={trafficAvailable && stats.errorsToday > 0 ? "negative" : trafficAvailable ? "positive" : "neutral"} />
    </MetricStrip>

    <section data-overview-section="activity"><Surface className="p-4" hover={false}><Heatmap activityDays={runtime.activityDays} tools={runtime.tools} source={runtime.activitySource} /></Surface></section>
    <section data-overview-section="issues"><IssueSummary status={operational.issues} /></section>
    <section data-overview-section="trend"><Surface className="p-4" hover={false}><div className="mb-3 flex items-center justify-between"><h3>{t("ov.trend")}</h3><span className="text-sm text-muted-foreground">{runtime.sourceLabel}</span></div>{runtime.trend.length > 0 ? <OverviewTrendChart data={runtime.trend} lang={lang} /> : <div className="flex h-28 items-center justify-center rounded-lg border border-dashed border-border/60 bg-muted/20 px-4 text-center text-sm text-muted-foreground">{unavailableDetail}</div>}</Surface></section>

    <section data-overview-section="operational"><Surface className="overflow-hidden p-0" strong hover={false}><div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/50 px-4 py-3"><div><h2 className="text-base font-semibold">{t("ov.operational")}</h2><p className="mt-0.5 text-xs text-muted-foreground">{t("ov.operationalDesc")}</p></div><Badge variant={evidenceState === "verified" ? "default" : "outline"}>{evidenceState === "verified" ? t("ov.localConnected") : t("ov.waitingConnection")}</Badge></div><div className="overview-status-grid grid grid-cols-1 gap-px bg-border/40 sm:grid-cols-2 min-[1080px]:grid-cols-4"><OperationalItem icon={Server} title={t("ov.localService")} {...operational.service} /><OperationalItem icon={Bot} title={t("ov.connectedAgents")} {...operational.agents} /><OperationalItem icon={Route} title={t("ov.currentPath")} {...operational.route} /><CcSwitchOperationalItem summary={operational.ccSwitch} /></div></Surface></section>

    <section data-overview-section="account-quotas" data-testid="overview-account-quotas"><Surface className="overflow-hidden p-0" hover={false}><div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/50 px-4 py-3"><div><h2 className="text-base font-semibold">{lang === "zh" ? "账户额度" : "Account quota"}</h2><p className="mt-0.5 text-xs text-muted-foreground">{lang === "zh" ? "仅显示 d-token 本地服务捕获的可信额度窗口" : "Only trusted quota windows captured by the local d-token service"}</p></div><Badge variant="outline">{lang === "zh" ? "只读" : "Read-only"}</Badge></div><div className="grid gap-px bg-border/40 md:grid-cols-2">{accountQuotas.map((quota) => <AccountQuotaCard key={quota.provider} quota={quota} lang={lang} />)}</div></Surface></section>
  </div>;
}

function overviewEvidenceLabel(state: EvidenceState, lang: Lang): string {
  const zh = lang === "zh";
  if (state === "verified") return zh ? "实时本地数据" : "Live local data";
  if (state === "preview") return zh ? "浏览器预览" : "Browser preview";
  if (state === "loading") return zh ? "正在检查本地服务" : "Checking local service";
  if (state === "stale") return zh ? "数据已过期" : "Data is stale";
  if (state === "partial") return zh ? "数据不完整" : "Data is incomplete";
  return zh ? "本地数据不可用" : "Local data unavailable";
}
