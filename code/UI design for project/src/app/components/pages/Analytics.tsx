import { motion } from "motion/react";
import { DollarSign, Coins, ArrowDownWideNarrow, Timer, Gauge, Boxes, Database, GitBranch } from "lucide-react";
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
          {/* headline metrics */}
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
            <Metric i={0} icon={<DollarSign className="size-3.5" />} label="Proxy $ Saved" value={`$${hr.costSavedUsd.toFixed(2)}`} sub="累计压缩成本节省" color="var(--chart-2)" />
            <Metric i={1} icon={<Coins className="size-3.5" />} label="Token Savings" value={nf(hr.tokens.saved)} sub={`${hr.tokens.savedPercent}% · of wire ${hr.tokens.ofWirePercent}%`} color="var(--chart-accent)" spark />
            <Metric i={2} icon={<ArrowDownWideNarrow className="size-3.5" />} label="Output Tokens Saved" value={nf(hr.output.saved)} sub={`${hr.output.percent}% · ${hr.output.method}`} color="var(--chart-2)" />
            <Metric i={3} icon={<Timer className="size-3.5" />} label="Overhead" value={`${hr.overheadMs}ms`} sub={`TTFB ${(hr.ttfbMs / 1000).toFixed(2)}s avg`} color="var(--chart-4)" />
            <Metric i={4} icon={<Gauge className="size-3.5" />} label="Throughput" value={`${nf(hr.throughput.inputP50)}`} sub="input active p50 tok/s" color="var(--chart-3)" />
          </div>

          {/* throughput detail + output CI */}
          <div className="grid gap-3 lg:grid-cols-3">
            <GlassCard className="p-4 lg:col-span-2">
              <h3 className="mb-3 text-sm">吞吐 Throughput (p50 / p95)</h3>
              <div className="grid grid-cols-2 gap-x-6 gap-y-2 text-sm sm:grid-cols-4">
                <Kv label="Input (wall/active)" v={`${nf(hr.throughput.inputWall)} / ${nf(hr.throughput.inputP50)}`} />
                <Kv label="Compression" v={`${hr.throughput.compP50} / ${hr.throughput.compP95}`} c="text-emerald-500" />
                <Kv label="Forward" v={`${nf(hr.throughput.fwdP50)} / ${nf(hr.throughput.fwdP95)}`} c="text-sky-500" />
                <Kv label="Generation" v={`${nf(hr.throughput.genP50)} / ${nf(hr.throughput.genP95)}`} c="text-amber-500" />
              </div>
            </GlassCard>
            <GlassCard className="p-4">
              <h3 className="mb-2 text-sm">Output Reduction</h3>
              <div className="text-2xl" style={{ fontWeight: 600 }}>{hr.output.percent}%</div>
              <div className="mt-1 text-xs text-muted-foreground">
                {hr.output.method} · {nf(hr.output.requests)} shaped · 95% CI {hr.output.ciLow}%…{hr.output.ciHigh}%
              </div>
            </GlassCard>
          </div>

          {/* agent usage */}
          <GlassCard className="p-4">
            <div className="mb-3 flex items-center justify-between">
              <h3 className="text-sm">Agent Usage — 按客户端的前后 token 用量</h3>
              <Badge variant="secondary">{hr.agentTotals.requests} requests</Badge>
            </div>
            <div className="mb-4 grid grid-cols-2 gap-2 sm:grid-cols-4">
              <Summary label="Before" v={nf(hr.agentTotals.before)} />
              <Summary label="After" v={nf(hr.agentTotals.after)} />
              <Summary label="Saved" v={nf(hr.agentTotals.saved)} c="var(--chart-accent)" />
              <Summary label="Savings" v={`${hr.agentTotals.savingsPercent}%`} c="var(--chart-2)" />
            </div>
            <div className="space-y-3">
              {hr.agents.map((a) => (
                <div key={a.agent} className="glass-inset p-3">
                  <div className="mb-2 flex items-center justify-between">
                    <span className="flex items-center gap-2 text-sm">
                      <span className="size-2 rounded-full" style={{ background: a.dot }} />
                      {a.label}
                      <span className="text-xs text-muted-foreground">· {a.requests} req · {a.source}</span>
                    </span>
                    <span className="text-xs text-emerald-500">{a.savingsPercent}% saved</span>
                  </div>
                  <div className="flex h-2.5 overflow-hidden rounded-full bg-muted">
                    <motion.div initial={{ width: 0 }} animate={{ width: `${a.savingsPercent}%` }} transition={{ duration: 0.6 }} className="bg-emerald-500" />
                    <div className="flex-1" style={{ background: "var(--chart-accent)", opacity: 0.55 }} />
                  </div>
                  <div className="mt-2 grid grid-cols-4 gap-2 text-xs">
                    <Kv label="Before" v={nf(a.before)} />
                    <Kv label="After" v={nf(a.after)} />
                    <Kv label="Saved" v={nf(a.saved)} c="text-[color:var(--chart-accent)]" />
                    <Kv label="Share" v={`${a.share}%`} />
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>

          <div className="grid gap-3 lg:grid-cols-2">
            {/* savings breakdown */}
            <GlassCard className="p-4">
              <h3 className="mb-3 flex items-center gap-2 text-sm"><DollarSign className="size-4" />Savings Breakdown</h3>
              <div className="grid grid-cols-2 gap-3">
                <div className="rounded-md border border-emerald-500/30 bg-emerald-500/5 p-3">
                  <div className="text-xs text-muted-foreground">Compression</div>
                  <div className="text-xl text-emerald-500" style={{ fontWeight: 600 }}>${hr.savings.compressionUsd.toFixed(2)}</div>
                  <div className="text-xs text-muted-foreground">{nf(hr.tokens.proxySaved)} proxy tokens</div>
                </div>
                <div className="rounded-md border border-sky-500/30 bg-sky-500/5 p-3">
                  <div className="text-xs text-muted-foreground">Prefix Cache Discount</div>
                  <div className="text-xl text-sky-500" style={{ fontWeight: 600 }}>${hr.savings.cacheUsd.toFixed(2)}</div>
                  <div className="text-xs text-muted-foreground">Provider-native caching</div>
                </div>
              </div>
            </GlassCard>

            {/* prefix cache impact */}
            <GlassCard className="p-4">
              <h3 className="mb-3 flex items-center gap-2 text-sm"><Database className="size-4" />Prefix Cache Impact</h3>
              <div className="grid grid-cols-4 gap-2 text-center">
                <Summary label="Reads" v={nf(hr.prefixCache.reads)} />
                <Summary label="Writes" v={nf(hr.prefixCache.writes)} />
                <Summary label="Hit Rate" v={`${hr.prefixCache.hitRate}%`} c="var(--chart-2)" />
                <Summary label="Busts" v={String(hr.prefixCache.busts)} />
              </div>
              <div className="mt-3 flex flex-wrap gap-1.5">
                {hr.prefixCache.providers.map((p) => <Badge key={p} variant="outline" className="text-xs">{p}</Badge>)}
              </div>
            </GlassCard>
          </div>

          {/* quota windows */}
          <GlassCard className="p-4">
            <h3 className="mb-3 text-sm">订阅额度窗口 Rate-Limit Windows</h3>
            <div className="grid gap-4 sm:grid-cols-2">
              {hr.quotaWindows.map((w) => {
                const color = w.util < 60 ? "bg-emerald-500" : w.util < 80 ? "bg-amber-500" : "bg-rose-500";
                return (
                  <div key={w.label}>
                    <div className="mb-1 flex justify-between text-sm">
                      <span>{lang === "zh" ? w.label : w.labelEn}</span>
                      <span className="text-muted-foreground">{w.util}% · resets {w.resetIn}</span>
                    </div>
                    <div className="h-2 overflow-hidden rounded-full bg-muted">
                      <motion.div initial={{ width: 0 }} animate={{ width: `${w.util}%` }} transition={{ duration: 0.6 }} className={`h-full ${color}`} />
                    </div>
                  </div>
                );
              })}
            </div>
          </GlassCard>

          {/* pipeline: what headroom removed */}
          <GlassCard className="p-4">
            <h3 className="mb-3 flex items-center gap-2 text-sm"><Boxes className="size-4" />What Headroom Removed — Pipeline</h3>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-[var(--glass-inset-border)] text-left text-xs text-muted-foreground">
                    <th className="pb-2 font-normal">Transform</th>
                    <th className="pb-2 font-normal">Requests</th>
                    <th className="pb-2 font-normal">Tokens</th>
                    <th className="pb-2 font-normal">Ratio</th>
                  </tr>
                </thead>
                <tbody>
                  {hr.pipeline.map((p) => (
                    <tr key={p.name} className="border-b border-border/30 last:border-0">
                      <td className="py-2"><span className="flex items-center gap-2"><GitBranch className="size-3.5 text-muted-foreground" />{p.name}<span className="text-xs text-muted-foreground">· {p.kind}</span></span></td>
                      <td className="py-2 text-muted-foreground">{nf(p.requests)}</td>
                      <td className="py-2">{nf(p.tokens)}</td>
                      <td className="py-2"><Badge variant="secondary" className="text-xs">{p.ratio.toFixed(2)}</Badge></td>
                    </tr>
                  ))}
                </tbody>
              </table>
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
