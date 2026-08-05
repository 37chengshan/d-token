import { useState } from "react";
import { motion, AnimatePresence } from "motion/react";
import { DollarSign, Coins, ArrowDownWideNarrow, Timer, Gauge, Boxes, Database, GitBranch, ChevronDown } from "lucide-react";
import { AreaChart, Area, BarChart, Bar, ResponsiveContainer, XAxis, Tooltip, Cell } from "recharts";
import { GlassCard } from "../common/GlassCard";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Badge } from "../ui/badge";
import { useI18n, useT } from "../../lib/i18n";
import { hr } from "../../lib/fixtures";

const nf = (n: number) => n.toLocaleString("en-US");

export function Analytics() {
  const t = useT();
  const { lang } = useI18n();
  const [showThroughput, setShowThroughput] = useState(false);

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <h1>{t("an.title")}</h1>
        <span className="text-xs text-muted-foreground">Headroom Dashboard · /stats</span>
        <DataSourceBadge />
      </div>

      <Tabs defaultValue="live">
        <TabsList>
          <TabsTrigger value="live">实时 Live</TabsTrigger>
          <TabsTrigger value="history">历史 Historical</TabsTrigger>
        </TabsList>

        {/* ---------------- LIVE ---------------- */}
        <TabsContent value="live" className="mt-3 space-y-4">
          {/* Core metrics - reduced to 3 */}
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <Metric i={0} icon={<Coins className="size-3.5" />} label="Token Savings" value={nf(hr.tokens.saved)} sub={`${hr.tokens.savedPercent}% compression rate`} color="var(--chart-accent)" spark />
            <Metric i={1} icon={<DollarSign className="size-3.5" />} label="Cost Saved" value={`$${hr.costSavedUsd.toFixed(2)}`} sub="累计压缩成本节省" color="var(--chart-2)" />
            <Metric i={2} icon={<ArrowDownWideNarrow className="size-3.5" />} label="Output Reduced" value={`${hr.output.percent}%`} sub={hr.output.method} color="var(--chart-4)" />
          </div>

          {/* Throughput - collapsible */}
          <GlassCard className="p-4">
            <button
              onClick={() => setShowThroughput(!showThroughput)}
              className="flex w-full items-center justify-between"
            >
              <div className="flex items-center gap-3">
                <Gauge className="size-4 text-muted-foreground" />
                <h3 className="text-sm">Throughput details</h3>
              </div>
              <motion.div animate={{ rotate: showThroughput ? 180 : 0 }} transition={{ duration: 0.2 }}>
                <ChevronDown className="size-4 text-muted-foreground" />
              </motion.div>
            </button>

            <AnimatePresence>
              {showThroughput && (
                <motion.div
                  initial={{ height: 0, opacity: 0 }}
                  animate={{ height: "auto", opacity: 1 }}
                  exit={{ height: 0, opacity: 0 }}
                  transition={{ duration: 0.25 }}
                  className="overflow-hidden"
                >
                  <div className="mt-4 grid grid-cols-2 gap-x-6 gap-y-3 text-sm sm:grid-cols-4">
                    <Kv label="Input (wall/active)" v={`${nf(hr.throughput.inputWall)} / ${nf(hr.throughput.inputP50)}`} />
                    <Kv label="Compression" v={`${hr.throughput.compP50} / ${hr.throughput.compP95}`} c="text-success" />
                    <Kv label="Forward" v={`${nf(hr.throughput.fwdP50)} / ${nf(hr.throughput.fwdP95)}`} c="text-sky-500" />
                    <Kv label="Generation" v={`${nf(hr.throughput.genP50)} / ${nf(hr.throughput.genP95)}`} c="text-amber-500" />
                  </div>
                  <div className="mt-4 rounded-lg glass-inset p-3">
                    <div className="text-xs text-muted-foreground">Overhead</div>
                    <div className="text-sm">{hr.overheadMs}ms · TTFB {(hr.ttfbMs / 1000).toFixed(2)}s avg</div>
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </GlassCard>

          {/* agent usage - simplified */}
          <GlassCard className="p-4">
            <div className="mb-3 flex items-center justify-between">
              <h3 className="text-sm">Agent token usage</h3>
              <div className="flex items-center gap-3 text-xs">
                <span className="text-muted-foreground">{nf(hr.agentTotals.requests)} requests</span>
                <Badge variant="secondary">{hr.agentTotals.savingsPercent}% saved</Badge>
              </div>
            </div>
            <div className="space-y-2">
              {hr.agents.map((a) => (
                <div key={a.agent} className="glass-inset p-2.5">
                  <div className="flex items-center justify-between">
                    <span className="flex items-center gap-2 text-sm">
                      <span className="size-2 rounded-full" style={{ background: a.dot }} />
                      {a.label}
                    </span>
                    <span className="text-xs font-medium text-success">{a.savingsPercent}%</span>
                  </div>
                  <div className="mt-1.5 flex h-1.5 overflow-hidden rounded-full bg-muted">
                    <motion.div initial={{ width: 0 }} animate={{ width: `${a.savingsPercent}%` }} transition={{ duration: 0.6 }} className="bg-success" />
                  </div>
                  <div className="mt-1.5 flex items-center justify-between text-xs text-muted-foreground">
                    <span>{nf(a.before)} → {nf(a.after)}</span>
                    <span>-{nf(a.saved)} tokens</span>
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>

          {/* savings & cache combined */}
          <div className="grid gap-3 lg:grid-cols-3">
            <GlassCard className="p-4">
              <div className="mb-2 flex items-center gap-2 text-xs text-muted-foreground">
                <DollarSign className="size-3.5" />
                Compression savings
              </div>
              <div className="text-xl font-semibold text-success">${hr.savings.compressionUsd.toFixed(2)}</div>
              <div className="text-xs text-muted-foreground">{nf(hr.tokens.proxySaved)} proxy tokens</div>
            </GlassCard>
            <GlassCard className="p-4">
              <div className="mb-2 flex items-center gap-2 text-xs text-muted-foreground">
                <Database className="size-3.5" />
                Cache discount
              </div>
              <div className="text-xl font-semibold text-sky-500">${hr.savings.cacheUsd.toFixed(2)}</div>
              <div className="text-xs text-muted-foreground">{hr.prefixCache.hitRate}% hit rate</div>
            </GlassCard>
            <GlassCard className="p-4">
              <div className="mb-2 text-xs text-muted-foreground">Cache providers</div>
              <div className="flex flex-wrap gap-1.5">
                {hr.prefixCache.providers.map((p) => <Badge key={p} variant="outline" className="text-xs">{p}</Badge>)}
              </div>
              <div className="mt-2 grid grid-cols-2 gap-2 text-xs">
                <Kv label="Reads" v={nf(hr.prefixCache.reads)} />
                <Kv label="Writes" v={nf(hr.prefixCache.writes)} />
              </div>
            </GlassCard>
          </div>

          {/* quota windows - compact */}
          <GlassCard className="p-4">
            <h3 className="mb-3 text-sm">Rate-limit windows</h3>
            <div className="space-y-3">
              {hr.quotaWindows.map((w) => {
                const color = w.util < 60 ? "bg-success" : w.util < 80 ? "bg-warning" : "bg-destructive";
                return (
                  <div key={w.label}>
                    <div className="mb-1 flex items-center justify-between text-sm">
                      <span>{lang === "zh" ? w.label : w.labelEn}</span>
                      <span className="text-xs text-muted-foreground">{w.util}% · resets {w.resetIn}</span>
                    </div>
                    <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                      <motion.div initial={{ width: 0 }} animate={{ width: `${w.util}%` }} transition={{ duration: 0.6 }} className={`h-full ${color}`} />
                    </div>
                  </div>
                );
              })}
            </div>
          </GlassCard>

          {/* pipeline - simplified */}
          <GlassCard className="p-4">
            <h3 className="mb-3 flex items-center gap-2 text-sm">
              <Boxes className="size-4" />
              Compression pipeline stages
            </h3>
            <div className="space-y-2">
              {hr.pipeline.map((p) => (
                <div key={p.name} className="glass-inset flex items-center justify-between p-2.5">
                  <div className="flex items-center gap-2 text-sm">
                    <GitBranch className="size-3.5 text-muted-foreground" />
                    <span>{p.name}</span>
                    <span className="text-xs text-muted-foreground">({p.kind})</span>
                  </div>
                  <div className="flex items-center gap-3 text-xs">
                    <span className="text-muted-foreground">{nf(p.tokens)} tokens</span>
                    <Badge variant="secondary">{p.ratio.toFixed(2)}</Badge>
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>
        </TabsContent>

        {/* ---------------- HISTORICAL ---------------- */}
        <TabsContent value="history" className="mt-3 space-y-4">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Summary big label="Lifetime $ Saved" v={`$${hr.costSavedUsd.toFixed(2)}`} c="var(--chart-2)" />
            <Summary big label="Lifetime Tokens Saved" v={nf(hr.tokens.saved)} />
            <Summary big label="Avg / Day" v={nf(Math.round(hr.tokens.saved / 30))} />
            <Summary big label="Active Days" v="28" />
          </div>

          <GlassCard className="p-4">
            <h3 className="mb-3 text-sm">Daily Savings (k tokens)</h3>
            <div className="h-56">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={hr.daily}>
                  <XAxis dataKey="d" tick={{ fontSize: 10, fill: "var(--muted-foreground)" }} axisLine={false} tickLine={false} />
                  <Tooltip cursor={{ fill: "var(--muted)" }} contentStyle={{ background: "var(--popover)", border: "1px solid var(--border)", borderRadius: 8, fontSize: 12 }} />
                  <Bar dataKey="saved" radius={[4, 4, 0, 0]}>
                    {hr.daily.map((_, i) => <Cell key={i} fill="var(--chart-accent)" />)}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          </GlassCard>

          <GlassCard className="p-4">
            <h3 className="mb-3 text-sm">Recent Checkpoints — /stats-history</h3>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-border/50 text-left text-xs text-muted-foreground">
                    <th className="pb-2 font-normal">Timestamp</th><th className="pb-2 font-normal">Req</th>
                    <th className="pb-2 font-normal">Before</th><th className="pb-2 font-normal">After</th>
                    <th className="pb-2 font-normal">Saved</th><th className="pb-2 font-normal">Ratio</th><th className="pb-2 font-normal">$</th>
                  </tr>
                </thead>
                <tbody>
                  {hr.history.map((h) => (
                    <tr key={h.ts} className="border-b border-border/30 font-mono text-xs last:border-0">
                      <td className="py-2">{h.ts}</td><td className="py-2">{h.requests}</td>
                      <td className="py-2 text-muted-foreground">{nf(h.before)}</td><td className="py-2 text-muted-foreground">{nf(h.after)}</td>
                      <td className="py-2" style={{ color: "var(--chart-accent)" }}>{nf(h.saved)}</td>
                      <td className="py-2">{h.ratio.toFixed(2)}</td><td className="py-2">${h.cost.toFixed(3)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </GlassCard>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function Metric({ i, icon, label, value, sub, color, spark }: { i: number; icon: React.ReactNode; label: string; value: string; sub: string; color: string; spark?: boolean }) {
  return (
    <motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: i * 0.04 }}>
      <GlassCard className="relative overflow-hidden p-3.5">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">{icon}{label}</div>
        <div className="mt-1.5 text-xl tracking-tight" style={{ fontWeight: 600, color }}>{value}</div>
        <div className="mt-0.5 truncate text-xs text-muted-foreground">{sub}</div>
        {spark && (
          <div className="mt-1 h-6 opacity-70">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={[3, 5, 4, 7, 6, 9, 8, 12].map((v, x) => ({ x, v }))}>
                <Area type="monotone" dataKey="v" stroke={color} strokeWidth={1.5} fill={color} fillOpacity={0.15} />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        )}
      </GlassCard>
    </motion.div>
  );
}

function Kv({ label, v, c }: { label: string; v: string; c?: string }) {
  return (
    <div>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={c ?? ""}>{v}</div>
    </div>
  );
}

function Summary({ label, v, c, big }: { label: string; v: string; c?: string; big?: boolean }) {
  return (
    <div className="glass-inset p-2.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={big ? "text-lg" : "text-base"} style={{ fontWeight: 600, color: c }}>{v}</div>
    </div>
  );
}
