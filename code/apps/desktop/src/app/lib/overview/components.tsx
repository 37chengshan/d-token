import { CircleAlert, Clock3, Coins, RefreshCw, type LucideIcon } from "lucide-react";
import { useT, type Lang, translateCopy } from "../i18n";
import { Badge } from "../../components/ui/badge";
import { AnimatedSegmentedControl } from "../../components/shell/AnimatedSegmentedControl";
import type { AccountQuotaSummary, CcSwitchStatus, OperationalStatus, OperationalTone } from "./derivations";
import type { SavingsCostSummary, SavingsDerivation, SavingsWindowKey } from "./savings";

/** 与 Overview.tsx 共用：1.2k / 2.3M 风格。 */
export function formatTokenCount(n: number) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

export function OverviewTrendChart({ data, lang }: { data: Array<{ day: string; saved: number }>; lang: Lang }) {
  const width = 720;
  const height = 176;
  const left = 8;
  const right = width - 8;
  const top = 12;
  const bottom = height - 28;
  const max = Math.max(1, ...data.map((point) => point.saved));
  const points = data.map((point, index) => ({ ...point, x: data.length === 1 ? width / 2 : left + (index / (data.length - 1)) * (right - left), y: bottom - (point.saved / max) * (bottom - top) }));
  const line = points.map((point, index) => `${index === 0 ? "M" : "L"}${point.x.toFixed(1)},${point.y.toFixed(1)}`).join(" ");
  const area = `${line} L${points[points.length - 1]?.x ?? right},${bottom} L${points[0]?.x ?? left},${bottom} Z`;
  const labelIndexes = Array.from(new Set([0, Math.floor((points.length - 1) / 2), points.length - 1]));
  return <div className="h-44 w-full" data-overview-trend="lightweight-svg">
    <svg viewBox={`0 0 ${width} ${height}`} className="h-full w-full overflow-visible" role="img" aria-label={translateCopy("ov.recentTokenReductionTrend", lang)}>
      <defs><linearGradient id="overview-trend-fill" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stopColor="var(--chart-accent)" stopOpacity="0.28" /><stop offset="100%" stopColor="var(--chart-accent)" stopOpacity="0.02" /></linearGradient></defs>
      <path d={area} fill="url(#overview-trend-fill)" />
      <path d={line} fill="none" stroke="var(--chart-accent)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
      {points.map((point) => <circle key={point.day} cx={point.x} cy={point.y} r="3" fill="var(--background)" stroke="var(--chart-accent)" strokeWidth="2" tabIndex={0} aria-label={`${point.day}: ${point.saved}`}><title>{point.day} · {translateCopy("ov.physicalReduction", lang)} {point.saved.toLocaleString()}</title></circle>)}
      {labelIndexes.map((index) => { const point = points[index]; return point ? <text key={point.day} x={point.x} y={height - 7} textAnchor={index === 0 ? "start" : index === points.length - 1 ? "end" : "middle"} fill="var(--muted-foreground)" fontSize="11">{point.day}</text> : null; })}
    </svg>
  </div>;
}

export function OverviewKpi({ label, value, detail, icon: Icon, tone = "neutral" }: { label: string; value: string; detail: string; icon: LucideIcon; tone?: OperationalTone }) {
  const iconTone = tone === "negative" ? "text-red-600 dark:text-red-400" : tone === "positive" ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground";
  return <div className="overview-kpi min-w-0"><div className="flex items-center justify-between gap-2 text-xs text-muted-foreground"><span className="min-w-0 break-words">{label}</span><Icon className={`size-4 shrink-0 ${iconTone}`} /></div><div className="mt-2 text-2xl font-semibold tracking-tight tabular-nums">{value}</div><div className="overview-kpi-detail mt-1 text-[11px] text-muted-foreground" title={detail}>{detail}</div></div>;
}

/** 节省区间卡：累计 / 当周 / 当月 / 当天 切换，展示该窗口的物理节省 Token 与诚实状态。 */
export function SavingsWindowCard({
  derivation,
  selected,
  onSelect,
  reducedMotion,
  lang,
}: {
  derivation: SavingsDerivation;
  selected: SavingsWindowKey;
  onSelect: (key: SavingsWindowKey) => void;
  reducedMotion: boolean;
  lang: Lang;
}) {
  const t = useT();
  const window = derivation.windows.find((item) => item.key === selected) ?? derivation.windows[0];
  const tone = window.state === "partial" ? "warning" : window.state === "verified" ? "positive" : "neutral";
  const stateLabel =
    window.state === "verified"
      ? translateCopy("cbm.state.ready", lang)
      : window.state === "partial"
        ? translateCopy("cbm.state.partial", lang)
        : translateCopy("cbm.state.unavailable", lang);
  return <div className="overview-kpi min-w-0" data-overview-savings-window="true" data-savings-window={window.key} data-savings-state={window.state} data-testid="overview-savings-window">
    <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground"><span className="min-w-0 break-words">{t("ov.savingsTokens")}</span><Coins className={`size-4 shrink-0 ${tone === "positive" ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground"}`} /></div>
    <div className="mt-2 text-2xl font-semibold tracking-tight tabular-nums">{window.tokensSaved === null ? "—" : formatTokenCount(window.tokensSaved)}</div>
    <div className="mt-1 flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground" title={window.note}><Badge variant={window.state === "verified" ? "default" : "secondary"}>{stateLabel}</Badge><span>{window.note}</span></div>
    <div className="mt-2"><AnimatedSegmentedControl
      ariaLabel={t("ov.savingsWindowAria")}
      reducedMotion={reducedMotion}
      value={window.key}
      onValueChange={onSelect}
      className="overview-savings-period-control"
      items={derivation.windows.map((item) => ({ value: item.key, label: item.label }))}
    /></div>
    <div className="overview-kpi-detail mt-1 text-[11px] text-muted-foreground" title={derivation.cost.detail}>{translateCopy("ov.costDelta", lang)}: {costDeltaText(derivation.cost, lang)}</div>
  </div>;
}

export function OperationalItem({ icon: Icon, title, label, detail, tone }: { icon: LucideIcon; title: string; label: string; detail: string; tone: OperationalTone }) {
  const color = tone === "positive" ? "text-emerald-600 dark:text-emerald-400" : tone === "warning" ? "text-amber-600 dark:text-amber-400" : tone === "negative" ? "text-red-600 dark:text-red-400" : "text-muted-foreground";
  return <div className="overview-status-item flat-status-cell flex min-h-[92px] gap-3 p-4"><Icon className={`mt-0.5 size-5 shrink-0 ${color}`} /><div className="min-w-0"><div className="text-sm text-muted-foreground">{title}</div><div className="mt-1 break-words font-medium">{label}</div><div className="mt-1 text-sm leading-5 text-muted-foreground">{detail}</div></div></div>;
}

export function CcSwitchOperationalItem({ summary }: { summary: CcSwitchStatus }) {
  const t = useT();
  const color = summary.state === "out_of_sync" ? "text-amber-600 dark:text-amber-400" : summary.state === "synced" ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground";
  return <div className="overview-status-item flat-status-cell flex min-h-[92px] gap-3 p-4"><RefreshCw className={`mt-0.5 size-5 shrink-0 ${color}`} /><div className="min-w-0"><div className="text-sm text-muted-foreground">{t("ov.upstreamStatus")}</div><div className="mt-1 break-words font-medium">{summary.label}</div><div className="mt-1 text-sm leading-5 text-muted-foreground">{summary.detail}</div></div></div>;
}

export function IssueSummary({ status }: { status: OperationalStatus }) {
  const t = useT();
  const color = status.tone === "negative" ? "text-red-600 dark:text-red-400" : status.tone === "positive" ? "text-emerald-600 dark:text-emerald-400" : status.tone === "warning" ? "text-amber-600 dark:text-amber-400" : "text-muted-foreground";
  return <div className="flex min-h-[58px] items-center gap-3 border-y border-border/60 px-1 py-3"><CircleAlert className={`size-5 shrink-0 ${color}`} /><div className="min-w-0 flex-1"><div className="text-xs text-muted-foreground">{t("ov.recentIssues")}</div><div className="mt-0.5 break-words text-sm font-medium">{status.label} <span className="font-normal text-muted-foreground">· {status.detail}</span></div></div></div>;
}

export function AccountQuotaCard({ quota, lang }: { quota: AccountQuotaSummary; lang: Lang }) {
  const unavailable = quota.state === "unavailable";
  const stateLabel = quota.state === "verified" ? (translateCopy("cbm.state.ready", lang)) : quota.state === "partial" ? (translateCopy("cbm.state.partial", lang)) : quota.state === "stale" ? (translateCopy("ov.stale", lang)) : (translateCopy("cbm.state.unavailable", lang));
  return <div className="flat-status-cell p-4"><div className="flex items-center justify-between gap-2"><div className="min-w-0"><div className="flex items-center gap-2"><Clock3 className="size-4 text-muted-foreground" /><span className="font-medium">{quota.label}</span></div>{quota.accountRef && <div className="mt-0.5 max-w-52 truncate pl-6 font-mono text-[11px] text-muted-foreground" title={quota.accountRef}>{quota.accountRef}</div>}</div><Badge variant={quota.state === "verified" ? "default" : "secondary"}>{stateLabel}</Badge></div><div className="mt-3 grid grid-cols-2 gap-2">{quota.windows.map((window) => <div key={window.key} className="flat-inset p-2.5"><div className="text-xs text-muted-foreground">{window.label}</div><div className="mt-1 text-lg font-semibold tabular-nums">{unavailable || window.usedPercent === null ? "—" : `${window.usedPercent.toFixed(0)}%`}</div><div className="mt-1 truncate text-[11px] text-muted-foreground" title={window.resetsAt ?? quota.source}>{window.resetsAt ? `${translateCopy("ov.resets", lang)} ${formatQuotaReset(window.resetsAt)}` : quota.source}</div>{!unavailable && window.usedPercent !== null && <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted"><div className="h-full rounded-full bg-[var(--chart-accent)]" style={{ width: `${Math.min(100, Math.max(0, window.usedPercent))}%` }} /></div>}</div>)}</div></div>;
}

function formatQuotaReset(value: string) { const date = new Date(value); return Number.isFinite(date.getTime()) ? date.toLocaleString() : value; }

/** 费用差额文案：有价格证据时显示币种+估算费用（明确标注估算），否则 — + 缺证据。 */
function costDeltaText(cost: SavingsCostSummary, lang: Lang): string {
  if (!cost.available) return `—${translateCopy("ov.noPricingEvidence", lang)}`;
  const symbol = cost.currency === "CNY" ? "¥" : "$";
  const estimate = formatCostEstimate(cost.estimatedCost);
  return translateCopy("ov.est", lang, { v1: symbol, v2: estimate });
}

function formatCostEstimate(value: number): string {
  if (value >= 1000) return value.toFixed(0);
  if (value >= 1) return value.toFixed(2);
  if (value === 0) return "0";
  return value >= 0.01 ? value.toFixed(2) : value.toFixed(4);
}
