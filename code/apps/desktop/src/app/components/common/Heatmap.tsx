import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { translateCopy, useI18n, type Lang } from "../../lib/i18n";
import { AnimatedSegmentedControl } from "../shell/AnimatedSegmentedControl";

const LEVELS = [0, 1, 2, 3, 4] as const;

type Range = "week" | "month" | "year";
type View = "activity" | "tokens" | "tools";
export type ActivityDay = {
  date: string;
  sessions: number;
  minutes: number;
  tokens: number;
  tools: number;
};
type ToolUsageRow = { name: string; calls: number };

const rangeDays: Record<Range, number> = { week: 7, month: 31, year: 365 };
const EMPTY_ACTIVITY_DAYS: ActivityDay[] = [];
const EMPTY_TOOL_USAGE: ToolUsageRow[] = [];

function rangeLabel(range: Range, lang: Lang) {
  return translateCopy(`hm.range.${range}`, lang);
}

function viewLabel(view: View, lang: Lang) {
  return translateCopy(`hm.view.${view}`, lang);
}

function formatNumber(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return String(value);
}

export function activityLevel(day: ActivityDay): 0 | 1 | 2 | 3 | 4 {
  if (day.sessions <= 0) return 0;
  // Request volume is intentionally mapped to stable absolute bands instead of
  // the largest value in the current range. A quiet day must stay visually
  // quiet even when it is the only active day in an otherwise empty year.
  if (day.sessions >= 200) return 4;
  if (day.sessions >= 80) return 3;
  if (day.sessions >= 25) return 2;
  return 1;
}

function tokenLevel(day: ActivityDay): 0 | 1 | 2 | 3 | 4 {
  if (day.tokens <= 0) return 0;
  if (day.tokens > 3900) return 4;
  if (day.tokens > 2700) return 3;
  if (day.tokens > 1400) return 2;
  return 1;
}

function pathFromPoints(points: { x: number; y: number }[]) {
  return points.map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`).join(" ");
}

export function Heatmap({
  activityDays = EMPTY_ACTIVITY_DAYS,
  tools = EMPTY_TOOL_USAGE,
  source,
  now = new Date(),
}: {
  activityDays?: ActivityDay[];
  tools?: ToolUsageRow[];
  source?: string;
  now?: Date;
}) {
  const { lang } = useI18n();
  const resolvedSource = source ?? translateCopy("hm.waitingForRealLocalRequestRecords", lang);
  const [range, setRange] = useState<Range>("year");
  const [view, setView] = useState<View>("activity");
  const [reducedMotion, setReducedMotion] = useState(
    () =>
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const updatePreference = (event: MediaQueryListEvent | MediaQueryList) => setReducedMotion(event.matches);
    updatePreference(preference);
    preference.addEventListener("change", updatePreference);
    return () => preference.removeEventListener("change", updatePreference);
  }, []);

  const visibleDays = useMemo(() => activityDays.slice(-rangeDays[range]), [activityDays, range]);
  const activeDays = visibleDays.filter((day) => day.sessions > 0).length;
  const totalSessions = visibleDays.reduce((sum, day) => sum + day.sessions, 0);
  const totalTokens = visibleDays.reduce((sum, day) => sum + day.tokens, 0);
  const totalSteps = visibleDays.reduce((sum, day) => sum + day.tools, 0);

  const viewItems = useMemo(
    () =>
      (["activity", "tokens", "tools"] as const).map((item) => ({
        value: item,
        label: viewLabel(item, lang),
      })),
    [lang],
  );
  const rangeItems = useMemo(
    () =>
      (["week", "month", "year"] as const).map((item) => ({
        value: item,
        label: rangeLabel(item, lang),
      })),
    [lang],
  );

  return (
    <div className="activity-panel">
      <div className="activity-toolbar">
        <AnimatedSegmentedControl
          items={viewItems}
          value={view}
          onValueChange={setView}
          ariaLabel={translateCopy("hm.metricView", lang)}
          reducedMotion={reducedMotion}
          className="activity-tabs"
        />

        <AnimatedSegmentedControl
          items={rangeItems}
          value={range}
          onValueChange={setRange}
          ariaLabel={translateCopy("hm.timeRange", lang)}
          reducedMotion={reducedMotion}
          className="activity-range-tabs"
        />

        <span className="rounded-full border border-border/60 px-2 py-1 text-xs text-muted-foreground">{resolvedSource}</span>
      </div>

      <AnimatePresence mode="wait">
        <motion.div
          key={`${view}-${range}`}
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -8 }}
          transition={{ duration: 0.18 }}
        >
          {view === "activity" && (
            <ActivityDateSkeleton range={range} activityDays={activityDays} now={now} lang={lang} />
          )}
          {view === "tokens" && <TokenTrend range={range} lang={lang} activityDays={activityDays} />}
          {view === "tools" && <ToolRanking lang={lang} tools={tools} />}
        </motion.div>
      </AnimatePresence>

      <div className="activity-summary">
          <div className="activity-summary-item">
            <div className="text-xs text-muted-foreground">{translateCopy("hm.sessions", lang)}</div>
            <div className="mt-1 text-lg font-semibold">{totalSessions}</div>
          </div>
          <div className="activity-summary-item">
            <div className="text-xs text-muted-foreground">{translateCopy("hm.tokens", lang)}</div>
            <div className="mt-1 text-lg font-semibold">{formatNumber(totalTokens)}</div>
          </div>
          <div className="activity-summary-item">
            <div className="text-xs text-muted-foreground">{translateCopy("hm.processingSteps", lang)}</div>
            <div className="mt-1 text-lg font-semibold">{totalSteps}</div>
          </div>
          <div className="activity-summary-item">
            <div className="text-xs text-muted-foreground">{translateCopy("hm.activeDays", lang)}</div>
            <div className="mt-1 text-lg font-semibold">
              {activeDays}
              {translateCopy("hm.days", lang)}
            </div>
          </div>
      </div>
    </div>
  );
}

function NoRequestEvidence({ lang }: { lang: Lang }) {
  return (
    <div
      className="flex min-h-[250px] flex-col items-center justify-center rounded-2xl border border-border/50 bg-background/25 px-6 text-center"
      role="status"
    >
      <div className="text-sm font-semibold text-foreground">
        {translateCopy("hm.noRealRequestRecordsYet", lang)}
      </div>
      <div className="mt-2 text-xs text-muted-foreground">
        {translateCopy("hm.connectTheLocalServiceToViewThem", lang)}
      </div>
    </div>
  );
}

type DateCell = { date: Date; key: string; inRange: boolean; activity: ActivityDay };

const emptyDay = (key: string): ActivityDay => ({ date: key, sessions: 0, minutes: 0, tokens: 0, tools: 0 });

export function localDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function localDate(year: number, month: number, day: number): Date {
  return new Date(year, month, day, 12, 0, 0, 0);
}

function addLocalDays(date: Date, amount: number): Date {
  return localDate(date.getFullYear(), date.getMonth(), date.getDate() + amount);
}

function mondayIndex(date: Date): number {
  return (date.getDay() + 6) % 7;
}

function activityMap(days: ActivityDay[]): Map<string, ActivityDay> {
  return new Map(days.map((day) => [day.date, day]));
}

export function buildWeekDays(now: Date, days: ActivityDay[]): DateCell[] {
  const map = activityMap(days);
  const start = addLocalDays(now, -mondayIndex(now));
  return Array.from({ length: 7 }, (_, index) => {
    const date = addLocalDays(start, index);
    const key = localDateKey(date);
    return { date, key, inRange: true, activity: map.get(key) ?? emptyDay(key) };
  });
}

export function buildMonthCells(now: Date, days: ActivityDay[]): DateCell[] {
  const map = activityMap(days);
  const first = localDate(now.getFullYear(), now.getMonth(), 1);
  const last = localDate(now.getFullYear(), now.getMonth() + 1, 0);
  const start = addLocalDays(first, -mondayIndex(first));
  const occupied = mondayIndex(first) + last.getDate();
  const cellCount = Math.max(35, Math.ceil(occupied / 7) * 7);
  return Array.from({ length: cellCount }, (_, index) => {
    const date = addLocalDays(start, index);
    const key = localDateKey(date);
    return { date, key, inRange: date.getMonth() === now.getMonth(), activity: map.get(key) ?? emptyDay(key) };
  });
}

export function buildYearCells(now: Date, days: ActivityDay[]): DateCell[] {
  const map = activityMap(days);
  const first = localDate(now.getFullYear(), 0, 1);
  const last = localDate(now.getFullYear(), 11, 31);
  const start = addLocalDays(first, -mondayIndex(first));
  const end = addLocalDays(last, 6 - mondayIndex(last));
  const cellCount = Math.round((end.getTime() - start.getTime()) / 86_400_000) + 1;
  return Array.from({ length: cellCount }, (_, index) => {
    const date = addLocalDays(start, index);
    const key = localDateKey(date);
    return { date, key, inRange: date.getFullYear() === now.getFullYear(), activity: map.get(key) ?? emptyDay(key) };
  });
}

export function buildYearMonthLabelColumns(cells: DateCell[], year: number): number[] {
  return Array.from({ length: 12 }, (_, month) => {
    const monthStart = `${year}-${String(month + 1).padStart(2, "0")}-01`;
    const index = cells.findIndex((cell) => cell.key === monthStart);
    return index < 0 ? 0 : Math.floor(index / 7);
  });
}

function ActivityDateSkeleton({ range, activityDays, now, lang }: { range: Range; activityDays: ActivityDay[]; now: Date; lang: Lang }) {
  const weekdays = localizedWeekdays(lang);
  const today = localDateKey(now);
  const empty = activityDays.every((day) => day.sessions === 0);
  return (
    <div className="activity-date-skeleton" data-activity-range={range}>
      <div className="activity-section-title">{translateCopy("hm.activityTrend", lang)}</div>
      {range === "week" && <WeekActivity cells={buildWeekDays(now, activityDays)} weekdays={weekdays} today={today} lang={lang} />}
      {range === "month" && <MonthActivity cells={buildMonthCells(now, activityDays)} weekdays={weekdays} today={today} now={now} lang={lang} />}
      {range === "year" && <YearActivity cells={buildYearCells(now, activityDays)} weekdays={weekdays} today={today} now={now} lang={lang} />}
      {empty && <p className="activity-empty-note" role="status">{translateCopy("hm.noActivityYet", lang)}</p>}
      <ActivityLegend lang={lang} />
    </div>
  );
}

function WeekActivity({ cells, weekdays, today, lang }: { cells: DateCell[]; weekdays: string[]; today: string; lang: Lang }) {
  return <div className="activity-week-grid activity-week-strip" data-testid="activity-week">{cells.map((cell, index) => (
    <div key={cell.key} className="activity-week-day" data-date={cell.key} data-today={cell.key === today ? "true" : "false"}>
      <span className="activity-weekday">{weekdays[index]}</span>
      <span className="activity-week-date">{cell.date.getMonth() + 1}/{cell.date.getDate()}</span>
      <ActivityCell cell={cell} lang={lang} />
      <span className="activity-week-count">{cell.activity.sessions}{translateCopy("hm.copy", lang)}</span>
    </div>
  ))}</div>;
}

function MonthActivity({ cells, weekdays, today, now, lang }: { cells: DateCell[]; weekdays: string[]; today: string; now: Date; lang: Lang }) {
  const title = new Intl.DateTimeFormat(lang === "zh" ? "zh-CN" : "en-US", { year: "numeric", month: "long" }).format(now);
  return <div className="activity-month activity-compact-calendar" data-testid="activity-month">
    <div className="activity-calendar-title">{title}</div>
    <CompactGrid cells={cells} weekdays={weekdays} today={today} lang={lang} />
  </div>;
}

function YearActivity({ cells, weekdays, today, now, lang }: { cells: DateCell[]; weekdays: string[]; today: string; now: Date; lang: Lang }) {
  const monthColumns = buildYearMonthLabelColumns(cells, now.getFullYear());
  return <div className="activity-year activity-compact-calendar" data-testid="activity-year">
    <div className="activity-year-month-labels" aria-hidden="true">
      {Array.from({ length: 12 }, (_, month) => (
        <span
          key={month}
          className="activity-year-month-label"
          data-column={monthColumns[month]}
          style={{ gridColumn: `${monthColumns[month] + 1} / span 4`, gridRow: 1 }}
        >
          {new Intl.DateTimeFormat(lang === "zh" ? "zh-CN" : "en-US", { month: "short" }).format(localDate(now.getFullYear(), month, 1))}
        </span>
      ))}
    </div>
    <CompactGrid cells={cells} weekdays={weekdays} today={today} lang={lang} />
  </div>;
}

function CompactGrid({ cells, weekdays, today, lang }: { cells: DateCell[]; weekdays: string[]; today: string; lang: Lang }) {
  return <div className="activity-compact-layout">
    <div className="activity-compact-weekdays" aria-hidden="true">
      {weekdays.map((day, index) => <span key={day}>{index % 2 === 0 ? (lang === "zh" ? day.slice(-1) : day.slice(0, 1)) : ""}</span>)}
    </div>
    <div className="activity-compact-scroll">
      <div className="activity-compact-grid" role="grid" aria-label={translateCopy("hm.activityDateHeatmap", lang)}>
        {cells.map((cell) => <ActivityCell key={cell.key} cell={cell} lang={lang} today={today} outside={!cell.inRange} />)}
      </div>
    </div>
  </div>;
}

function ActivityCell({ cell, lang, today = "", outside = false }: { cell: DateCell; lang: Lang; today?: string; outside?: boolean }) {
  const label = translateCopy("hm.activities", lang, { v1: cell.key, v2: cell.activity.sessions });
  return <span role="gridcell" tabIndex={0} className="activity-date-cell" data-date={cell.key} data-level={activityLevel(cell.activity)} data-outside={outside ? "true" : "false"} data-today={cell.key === today ? "true" : "false"} aria-label={label} title={label} />;
}

function ActivityLegend({ lang }: { lang: Lang }) {
  return <div className="activity-legend"><span>{translateCopy("an.less", lang)}</span>{LEVELS.map((level) => <span key={level} className="activity-legend-cell" data-level={level} />)}<span>{translateCopy("an.more", lang)}</span></div>;
}

function localizedWeekdays(lang: Lang): string[] {
  const monday = localDate(2024, 0, 1);
  return Array.from({ length: 7 }, (_, index) => new Intl.DateTimeFormat(lang === "zh" ? "zh-CN" : "en-US", { weekday: "short" }).format(addLocalDays(monday, index)));
}

function TokenTrend({ range, lang, activityDays }: { range: Range; lang: Lang; activityDays: ActivityDay[] }) {
  const series = activityDays.slice(-rangeDays[range]).reduce<Array<{ date: string; period: number; cumulative: number }>>(
    (rows, day) => {
      const period = Math.max(0, day.tokens / 1000);
      rows.push({
        date: day.date.slice(5).replace("-", "/"),
        period,
        cumulative: period + (rows.length > 0 ? rows[rows.length - 1].cumulative : 0),
      });
      return rows;
    },
    [],
  );

  if (series.length === 0) return <NoRequestEvidence lang={lang} />;

  const max = Math.max(...series.map((item) => item.cumulative), 1);
  const chartWidth = 760;
  const chartHeight = 250;
  const left = 48;
  const right = 24;
  const bottom = 34;
  const top = 20;
  const plotWidth = chartWidth - left - right;
  const plotHeight = chartHeight - top - bottom;
  const step = series.length > 1 ? plotWidth / (series.length - 1) : plotWidth;
  const points = series.map((item, index) => ({
    x: left + index * step,
    y: top + plotHeight - (item.cumulative / max) * plotHeight,
  }));
  const total = series.reduce((sum, item) => sum + item.period, 0);

  return (
    <div className="token-trend-layout">
      <div className="token-chart">
        <div className="activity-section-title">{translateCopy("hm.tokenTrend", lang)}</div>
        <div className="token-legend">
          <span><i className="bar" />{translateCopy("hm.periodTokenUsage", lang)}</span>
          <span><i className="line" />{translateCopy("hm.cumulativeTokens", lang)}</span>
        </div>
        <svg viewBox={`0 0 ${chartWidth} ${chartHeight}`} className="token-svg" role="img">
          {[0, 1, 2].map((line) => (
            <line
              key={line}
              x1={left}
              x2={chartWidth - right}
              y1={top + (plotHeight / 2) * line}
              y2={top + (plotHeight / 2) * line}
              className="token-grid-line"
            />
          ))}
          {series.map((item, index) => {
            const barHeight = item.period > 0 ? Math.max(2, (item.period / max) * plotHeight) : 0;
            const barW = Math.min(48, Math.max(4, step * 0.6));
            const x = left + index * step - barW / 2;
            const y = top + plotHeight - barHeight;
            return (
              <g key={item.date}>
                <rect x={x} y={y} width={barW} height={barHeight} rx={6} className="token-bar" />
                <text x={left + index * step} y={chartHeight - 8} textAnchor="middle" className="token-axis-label">
                  {item.date}
                </text>
              </g>
            );
          })}
          <path d={pathFromPoints(points)} className="token-line" />
          {points.map((point, index) => (
            <circle key={series[index].date} cx={point.x} cy={point.y} r={5} className="token-point">
              <title>{`${series[index].date} · ${series[index].cumulative.toFixed(1)}k`}</title>
            </circle>
          ))}
        </svg>
      </div>

      <div className="rounded-2xl border border-border/60 bg-background/25 px-4 py-6 text-center">
        <div className="text-xs text-muted-foreground">{translateCopy("hm.recordedTokens", lang)}</div>
        <strong className="mt-2 block text-xl">{formatNumber(Math.round(total * 1_000))}</strong>
        <div className="mt-2 text-xs text-muted-foreground">
          {translateCopy("hm.fromLocalRequestRecords", lang)}
        </div>
      </div>
    </div>
  );
}

function ToolRanking({ lang, tools }: { lang: Lang; tools: ToolUsageRow[] }) {
  if (tools.length === 0) return <NoRequestEvidence lang={lang} />;

  const rankedTools = tools.map((item, index) => ({
    ...item,
    calls: Math.max(0, item.calls),
    color: index % 2 === 0 ? "var(--chart-accent, #f97316)" : "#68c7ee",
  }));
  const maxTool = Math.max(...rankedTools.map((item) => item.calls), 1);
  const totalToolCalls = rankedTools.reduce((sum, item) => sum + item.calls, 0);

  return (
    <div className="activity-tools-layout">
      <div className="activity-tools-list">
        <div className="activity-section-title">{translateCopy("hm.processingStepRanking", lang)}</div>
        {rankedTools.map((tool) => (
          <div key={tool.name} className="activity-tool-row">
            <div className="activity-tool-name">{tool.name}</div>
            <div className="activity-tool-track">
              <div
                className="activity-tool-bar"
                style={{
                  width: `${(tool.calls / maxTool) * 100}%`,
                  background: tool.color,
                }}
              />
            </div>
            <div className="activity-tool-value">{tool.calls}</div>
          </div>
        ))}
      </div>

      <div
        className="rounded-2xl border border-border/60 bg-background/25 px-4 py-6 text-center"
        aria-label={translateCopy("hm.totalProcessingSteps", lang)}
      >
        <div className="text-xs text-muted-foreground">{translateCopy("hm.recordedCalls", lang)}</div>
        <strong className="mt-2 block text-xl">{totalToolCalls}</strong>
        <div className="mt-2 text-xs text-muted-foreground">
          {translateCopy("hm.fromLocalRequestRecords", lang)}
        </div>
      </div>
    </div>
  );
}
