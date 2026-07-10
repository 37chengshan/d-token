import { AnimatePresence, motion } from "motion/react";
import { useMemo, useState } from "react";
import { activityCalendar, toolUsage, type ActivityDay } from "../../lib/fixtures";
import { useI18n } from "../../lib/i18n";

const CELL = 13;
const GAP = 3;
const ROWS = 7;
const LEVELS = [0, 1, 2, 3, 4] as const;

type Range = "week" | "month" | "year";
type View = "activity" | "tokens" | "tools";
type CalendarCell = ActivityDay | null;
type ToolUsageRow = { name: string; calls: number };

const rangeDays: Record<Range, number> = { week: 7, month: 31, year: 365 };

const tokenSeries = [
  { date: "6/27", input: 0, total: 0 },
  { date: "6/28", input: 26, total: 18 },
  { date: "6/29", input: 22, total: 27 },
  { date: "6/30", input: 44, total: 42 },
  { date: "7/1", input: 24, total: 58 },
  { date: "7/2", input: 109, total: 142 },
  { date: "7/3", input: 17, total: 148 },
];

function rangeLabel(range: Range, lang: string) {
  if (lang === "zh") return range === "week" ? "本周" : range === "month" ? "本月" : "年";
  return range === "week" ? "Week" : range === "month" ? "Month" : "Year";
}

function viewLabel(view: View, lang: string) {
  const labels: Record<View, { zh: string; en: string }> = {
    activity: { zh: "活跃趋势", en: "Activity" },
    tokens: { zh: "Token", en: "Token" },
    tools: { zh: "工具", en: "Tools" },
  };
  return lang === "zh" ? labels[view].zh : labels[view].en;
}

function formatMonth(date: string, lang: string) {
  const month = new Date(`${date}T00:00:00`).getMonth() + 1;
  if (lang === "zh") return `${month}月`;
  return new Intl.DateTimeFormat("en-US", { month: "short" }).format(new Date(`${date}T00:00:00`));
}

function formatNumber(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return String(value);
}

function activityLevel(day: ActivityDay): 0 | 1 | 2 | 3 | 4 {
  if (day.sessions <= 0) return 0;
  if (day.sessions >= 5) return 4;
  if (day.sessions >= 3) return 3;
  if (day.sessions >= 2) return 2;
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
  activityDays = activityCalendar,
  tools = toolUsage,
  source = "浏览器预览",
}: {
  activityDays?: ActivityDay[];
  tools?: ToolUsageRow[];
  source?: string;
}) {
  const { lang } = useI18n();
  const [range, setRange] = useState<Range>("week");
  const [view, setView] = useState<View>("activity");
  const [tooltip, setTooltip] = useState<{ x: number; y: number; day: ActivityDay } | null>(null);

  const visibleDays = useMemo(() => activityDays.slice(-rangeDays[range]), [activityDays, range]);
  const selectedDates = useMemo(() => new Set(visibleDays.map((day) => day.date)), [visibleDays]);
  const activeDays = visibleDays.filter((day) => day.sessions > 0).length;
  const totalSessions = visibleDays.reduce((sum, day) => sum + day.sessions, 0);
  const totalTokens = visibleDays.reduce((sum, day) => sum + day.tokens, 0);
  const totalTools = visibleDays.reduce((sum, day) => sum + day.tools, 0);

  const { columns, months } = useMemo(() => {
    const sorted = [...activityDays].sort((a, b) => a.date.localeCompare(b.date));
    const first = sorted[0] ? new Date(`${sorted[0].date}T00:00:00`) : new Date();
    const padded: CalendarCell[] = [];
    for (let i = 0; i < first.getDay(); i += 1) padded.push(null);
    padded.push(...sorted);
    while (padded.length % ROWS !== 0) padded.push(null);

    const nextColumns: CalendarCell[][] = [];
    for (let i = 0; i < padded.length; i += ROWS) nextColumns.push(padded.slice(i, i + ROWS));

    const nextMonths: { label: string; column: number }[] = [];
    let lastMonth = "";
    nextColumns.forEach((column, index) => {
      const firstValid = column.find(Boolean);
      if (!firstValid) return;
      const label = formatMonth(firstValid.date, lang);
      if (label !== lastMonth) {
        lastMonth = label;
        nextMonths.push({ label, column: index });
      }
    });

    return { columns: nextColumns, months: nextMonths };
  }, [activityDays, lang]);

  return (
    <div className="activity-panel">
      <div className="activity-toolbar">
        <div className="activity-tabs" role="radiogroup" aria-label={lang === "zh" ? "统计视图" : "Metric view"}>
          {(["activity", "tokens", "tools"] as const).map((item) => (
            <button
              key={item}
              type="button"
              aria-label={`${lang === "zh" ? "切换统计视图" : "Switch metric view"}: ${viewLabel(item, lang)}`}
              aria-pressed={view === item}
              className={view === item ? "activity-tab active" : "activity-tab"}
              onClick={() => setView(item)}
            >
              {view === item && <motion.span layoutId="activity-tab-indicator" className="activity-tab-indicator" />}
              <span className="relative z-10">{viewLabel(item, lang)}</span>
            </button>
          ))}
        </div>

        <div className="activity-range-tabs">
          {(["week", "month", "year"] as const).map((item) => (
            <button
              key={item}
              type="button"
              aria-label={`${lang === "zh" ? "切换时间范围" : "Switch time range"}: ${rangeLabel(item, lang)}`}
              aria-pressed={range === item}
              className={range === item ? "activity-range active" : "activity-range"}
              onClick={() => setRange(item)}
            >
              {range === item && <motion.span layoutId="activity-range-indicator" className="activity-tab-indicator" />}
              <span className="relative z-10">{rangeLabel(item, lang)}</span>
            </button>
          ))}
        </div>

        <span className="rounded-full border border-border/60 px-2 py-1 text-xs text-muted-foreground">{source}</span>
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
            <ActivityCalendar
              columns={columns}
              months={months}
              selectedDates={selectedDates}
              tooltip={tooltip}
              setTooltip={setTooltip}
              lang={lang}
            />
          )}
          {view === "tokens" && <TokenTrend range={range} lang={lang} activityDays={activityDays} />}
          {view === "tools" && <ToolRanking range={range} lang={lang} tools={tools} />}
        </motion.div>
      </AnimatePresence>

      <div className="activity-summary">
        <div className="activity-summary-item">
          <div className="text-xs text-muted-foreground">{lang === "zh" ? "会话数" : "Sessions"}</div>
          <div className="mt-1 text-lg font-semibold">{totalSessions}</div>
        </div>
        <div className="activity-summary-item">
          <div className="text-xs text-muted-foreground">{lang === "zh" ? "Token 处理量" : "Tokens"}</div>
          <div className="mt-1 text-lg font-semibold">{formatNumber(totalTokens)}</div>
        </div>
        <div className="activity-summary-item">
          <div className="text-xs text-muted-foreground">{lang === "zh" ? "工具调用" : "Tool calls"}</div>
          <div className="mt-1 text-lg font-semibold">{totalTools}</div>
        </div>
        <div className="activity-summary-item">
          <div className="text-xs text-muted-foreground">{lang === "zh" ? "活跃天数" : "Active days"}</div>
          <div className="mt-1 text-lg font-semibold">
            {activeDays}
            {lang === "zh" ? " 天" : " days"}
          </div>
        </div>
      </div>
    </div>
  );
}

function ActivityCalendar({
  columns,
  months,
  selectedDates,
  tooltip,
  setTooltip,
  lang,
}: {
  columns: CalendarCell[][];
  months: { label: string; column: number }[];
  selectedDates: Set<string>;
  tooltip: { x: number; y: number; day: ActivityDay } | null;
  setTooltip: (tooltip: { x: number; y: number; day: ActivityDay } | null) => void;
  lang: string;
}) {
  const width = columns.length * (CELL + GAP) + GAP;
  const height = ROWS * (CELL + GAP) + GAP;

  return (
    <>
      <div className="activity-section-title">{lang === "zh" ? "活跃趋势" : "Activity trend"}</div>
      <div className="activity-calendar-frame">
        <div className="activity-scroll" onMouseLeave={() => setTooltip(null)}>
          <svg
            width={width}
            height={height + 24}
            viewBox={`0 0 ${width} ${height + 24}`}
            className="activity-svg"
            role="img"
            aria-label={lang === "zh" ? "年度活跃日历" : "Yearly activity calendar"}
          >
            {months.map((month) => (
              <text
                key={`${month.label}-${month.column}`}
                x={month.column * (CELL + GAP) + GAP}
                y={12}
                className="activity-month-label"
              >
                {month.label}
              </text>
            ))}

            {columns.map((column, columnIndex) =>
              column.map((day, rowIndex) => {
                if (!day) return null;
                const selected = selectedDates.has(day.date);
                const level = selected ? activityLevel(day) : 0;
                const x = columnIndex * (CELL + GAP) + GAP;
                const y = rowIndex * (CELL + GAP) + GAP + 22;

                return (
                  <rect
                    key={day.date}
                    x={x}
                    y={y}
                    width={CELL}
                    height={CELL}
                    rx={3}
                    className="activity-cell-svg"
                    data-level={level}
                    data-dimmed={selected ? "false" : "true"}
                    onMouseEnter={(event) => setTooltip({ x: event.clientX, y: event.clientY, day })}
                    onMouseMove={(event) => setTooltip({ x: event.clientX, y: event.clientY, day })}
                  >
                    <title>
                      {lang === "zh"
                        ? `${day.date} · ${day.sessions} 会话 · ${day.minutes} 分钟`
                        : `${day.date} · ${day.sessions} sessions · ${day.minutes} min`}
                    </title>
                  </rect>
                );
              }),
            )}
          </svg>
        </div>

        {tooltip && (
          <div className="activity-tooltip" style={{ left: tooltip.x + 12, top: tooltip.y - 42 }}>
            {lang === "zh"
              ? `${tooltip.day.date} · ${tooltip.day.sessions} 会话 · ${tooltip.day.minutes} 分钟`
              : `${tooltip.day.date} · ${tooltip.day.sessions} sessions · ${tooltip.day.minutes} min`}
          </div>
        )}

        <div className="activity-legend">
          <span>Less</span>
          {LEVELS.map((level) => (
            <span key={level} className="activity-legend-cell" data-level={level} />
          ))}
          <span>More</span>
        </div>
      </div>
    </>
  );
}

function TokenTrend({ range, lang, activityDays }: { range: Range; lang: string; activityDays: ActivityDay[] }) {
  const data = activityDays.slice(-rangeDays[range]).map((day) => ({
    date: day.date.slice(5).replace("-", "/"),
    input: Math.max(0, day.tokens / 1000),
    total: Math.max(0, (day.tokens + day.tools * 120) / 1000),
  }));
  const fallback = tokenSeries.slice(-rangeDays[range]);
  const series = data.length > 0 ? data : fallback;
  const max = Math.max(...series.map((item) => Math.max(item.input, item.total)), 1);
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
    y: top + plotHeight - (item.total / max) * plotHeight,
  }));
  const total = series.reduce((sum, item) => sum + item.total, 0);
  const input = series.reduce((sum, item) => sum + item.input, 0);

  return (
    <div className="token-trend-layout">
      <div className="token-chart">
        <div className="activity-section-title">{lang === "zh" ? "Token 趋势" : "Token trend"}</div>
        <div className="token-legend">
          <span><i className="bar" />{lang === "zh" ? "时段 Token 用量" : "Period token usage"}</span>
          <span><i className="line" />{lang === "zh" ? "累计 Token 用量" : "Cumulative tokens"}</span>
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
            const barHeight = Math.max(5, (item.input / max) * plotHeight);
            const x = left + index * step - 24;
            const y = top + plotHeight - barHeight;
            return (
              <g key={item.date}>
                <rect x={x} y={y} width={48} height={barHeight} rx={6} className="token-bar" />
                <text x={left + index * step} y={chartHeight - 8} textAnchor="middle" className="token-axis-label">
                  {item.date}
                </text>
              </g>
            );
          })}
          <path d={pathFromPoints(points)} className="token-line" />
          {points.map((point, index) => (
            <circle key={series[index].date} cx={point.x} cy={point.y} r={5} className="token-point">
              <title>{`${series[index].date} · ${series[index].total.toFixed(1)}k`}</title>
            </circle>
          ))}
        </svg>
      </div>

      <div className="token-donut">
        <div className="token-donut-ring" />
        <div className="token-donut-center">
          <strong>{formatNumber(Math.round(total * 1_000_000))}</strong>
          <span>Total</span>
        </div>
        <div className="activity-donut-legend">
          <span><i className="blue" />{lang === "zh" ? "输入" : "Input"} <b>99%</b></span>
          <span><i className="green" />{lang === "zh" ? "输出" : "Output"} <b>1%</b></span>
          <span><i className="yellow" />{lang === "zh" ? "推理" : "Reasoning"} <b>{input > total ? 1 : 0}%</b></span>
        </div>
      </div>
    </div>
  );
}

function ToolRanking({ range, lang, tools }: { range: Range; lang: string; tools: ToolUsageRow[] }) {
  const multiplier = range === "week" ? 0.32 : range === "month" ? 1 : 3.2;
  const sourceTools = tools.length > 0 ? tools : toolUsage;
  const rankedTools = sourceTools.map((item, index) => ({
    ...item,
    calls: Math.max(1, Math.round(item.calls * multiplier)),
    color: index % 2 === 0 ? "var(--chart-accent, #f97316)" : "#68c7ee",
  }));
  const maxTool = Math.max(...rankedTools.map((item) => item.calls));
  const totalToolCalls = rankedTools.reduce((sum, item) => sum + item.calls, 0);

  return (
    <div className="activity-tools-layout">
      <div className="activity-tools-list">
        <div className="activity-section-title">{lang === "zh" ? "工具调用排行" : "Tool usage ranking"}</div>
        {rankedTools.map((tool) => (
          <div key={tool.name} className="activity-tool-row">
            <div className="activity-tool-name">{tool.name}</div>
            <div className="activity-tool-track">
              <div
                className="activity-tool-bar"
                style={{
                  width: `${Math.max(3, (tool.calls / maxTool) * 100)}%`,
                  background: tool.color,
                }}
              />
            </div>
            <div className="activity-tool-value">{tool.calls}</div>
          </div>
        ))}
      </div>

      <div className="activity-donut" aria-label={lang === "zh" ? "工具调用总量" : "Total tool calls"}>
        <div className="activity-donut-ring" />
        <div className="activity-donut-center">
          <strong>{totalToolCalls}</strong>
          <span>{lang === "zh" ? "总调用" : "calls"}</span>
        </div>
        <div className="activity-donut-legend">
          <span><i className="orange" />{lang === "zh" ? "压缩/路由" : "Compress/route"}</span>
          <span><i className="blue" />{lang === "zh" ? "检查/诊断" : "Inspect/doctor"}</span>
        </div>
      </div>
    </div>
  );
}
