import { motion } from "motion/react";
import { AreaChart, Area, ResponsiveContainer, Tooltip, XAxis } from "recharts";
import { Coins, TrendingUp, DollarSign, Gauge, Bot, AlertTriangle, Download, Stethoscope, BarChart3, ArrowRight } from "lucide-react";
import { StatCard } from "../common/StatCard";
import { GlassCard } from "../common/GlassCard";
import { CompressionFlow } from "../common/CompressionFlow";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { useT } from "../../lib/i18n";
import { overviewStats, trend14 } from "../../lib/fixtures";
import type { PageId } from "../../lib/nav";
import { ACCENTS } from "../../lib/theme-context";

function fmt(n: number) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "k";
  return String(n);
}

export function Overview({ setPage }: { setPage: (p: PageId) => void }) {
  const t = useT();
  const s = overviewStats;

  const quick = [
    { icon: Download, label: t("ov.goInstall"), page: "install" as PageId },
    { icon: Stethoscope, label: t("ov.goDoctor"), page: "doctor" as PageId },
    { icon: BarChart3, label: t("ov.goAnalytics"), page: "analytics" as PageId },
  ];

  const layers = [
    { label: t("ov.layerRtk"), v: s.layered.rtk, color: ACCENTS[0].color },
    { label: t("ov.layerRuntime"), v: s.layered.runtime, color: ACCENTS[1].color },
    { label: t("ov.layerRoute"), v: s.layered.route, color: ACCENTS[3].color },
  ];

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <h1>{t("ov.title")}</h1>
        <DataSourceBadge />
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard index={0} label={t("ov.savedToday")} value={fmt(s.savedToday)} sub="tokens" icon={<Coins className="size-4" />} accentBar="var(--chart-1)" />
        <StatCard index={1} label={t("ov.savedWeek")} value={fmt(s.savedWeek)} sub="tokens" icon={<TrendingUp className="size-4" />} accentBar="var(--chart-2)" />
        <StatCard index={2} label={t("ov.costSaved")} value={`$${s.costSaved.toFixed(1)}`} icon={<DollarSign className="size-4" />} accentBar="var(--chart-4)" />
        <StatCard index={3} label={t("ov.compressionRate")} value={`${Math.round(s.compressionRate * 100)}%`} icon={<Gauge className="size-4" />} accentBar="var(--chart-3)" />
      </div>

      <CompressionFlow savedToday={s.savedToday} compressionRate={s.compressionRate} />

      <div className="grid gap-4 lg:grid-cols-3">
        <GlassCard className="p-5 lg:col-span-2">
          <div className="mb-3 flex items-center justify-between">
            <h3>{t("ov.trend")}</h3>
            <span className="text-sm text-muted-foreground">{fmt(s.savedToday)} tokens</span>
          </div>
          <div className="h-52">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={trend14}>
                <defs>
                  <linearGradient id="ovGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="var(--chart-accent)" stopOpacity={0.5} />
                    <stop offset="95%" stopColor="var(--chart-accent)" stopOpacity={0} />
                  </linearGradient>
                </defs>
                <XAxis dataKey="day" tick={{ fontSize: 11, fill: "var(--muted-foreground)" }} axisLine={false} tickLine={false} />
                <Tooltip contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }} />
                <Area type="monotone" dataKey="saved" stroke="var(--chart-accent)" strokeWidth={2} fill="url(#ovGrad)" />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        </GlassCard>

        <div className="space-y-4">
          <GlassCard className="p-5">
            <div className="mb-3 flex items-center justify-between">
              <h3>{t("ov.layered")}</h3>
              <div className="flex items-center gap-2 text-sm text-muted-foreground"><Bot className="size-4" />{s.activeAgents} · {t("ov.activeAgents")}</div>
            </div>
            <div className="space-y-3">
              {layers.map((l) => (
                <div key={l.label}>
                  <div className="mb-1 flex justify-between text-sm">
                    <span className="text-muted-foreground">{l.label}</span>
                    <span>{Math.round(l.v * 100)}%</span>
                  </div>
                  <div className="h-2 overflow-hidden rounded-full bg-muted">
                    <motion.div initial={{ width: 0 }} animate={{ width: `${l.v * 100}%` }} transition={{ duration: 0.6 }} className="h-full rounded-full" style={{ background: l.color }} />
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>

          <GlassCard className="flex items-center gap-3 p-4">
            <AlertTriangle className="size-5 text-amber-500" />
            <div className="text-sm">
              <div>{t("ov.errorsToday")}</div>
              <div className="text-muted-foreground">{s.errorsToday} · Provider 401</div>
            </div>
          </GlassCard>
        </div>
      </div>

      <div>
        <h3 className="mb-3">{t("ov.quickActions")}</h3>
        <div className="grid gap-4 sm:grid-cols-3">
          {quick.map((q, i) => (
            <motion.button
              key={q.page}
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.1 + i * 0.05 }}
              onClick={() => setPage(q.page)}
            >
              <GlassCard className="flex items-center justify-between p-4">
                <span className="flex items-center gap-3"><q.icon className="size-5 text-primary" />{q.label}</span>
                <ArrowRight className="size-4 text-muted-foreground" />
              </GlassCard>
            </motion.button>
          ))}
        </div>
      </div>
    </div>
  );
}
