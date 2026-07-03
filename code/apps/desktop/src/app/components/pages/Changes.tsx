import { motion } from "motion/react";
import { Check, X, RotateCcw, FileCode, Folder } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { useI18n, useT } from "../../lib/i18n";
import { changes, backups, ownership } from "../../lib/fixtures";

export function Changes() {
  const t = useT();
  const { lang } = useI18n();

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3"><h1>{t("ch.title")}</h1><DataSourceBadge /></div>

      <Tabs defaultValue="pending">
        <TabsList className="flex-wrap">
          <TabsTrigger value="pending">{t("ch.tab.pending")}</TabsTrigger>
          <TabsTrigger value="diff">{t("ch.tab.diff")}</TabsTrigger>
          <TabsTrigger value="backup">{t("ch.tab.backup")}</TabsTrigger>
          <TabsTrigger value="ownership">{t("ch.tab.ownership")}</TabsTrigger>
        </TabsList>

        <TabsContent value="pending" className="mt-4 space-y-3">
          {changes.map((c) => (
            <GlassCard key={c.id} className="p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2"><FileCode className="size-4 text-muted-foreground" /><span className="font-mono text-sm">{c.file}</span></div>
                <div className="flex items-center gap-2">
                  <Badge variant={c.risk === "low" ? "secondary" : "destructive"}>{t("ch.risk")}: {c.risk}</Badge>
                  <Button size="sm" variant="outline"><X className="size-3.5" />{t("ch.reject")}</Button>
                  <Button size="sm"><Check className="size-3.5" />{t("ch.approve")}</Button>
                </div>
              </div>
              <div className="mt-1 text-sm text-muted-foreground">{lang === "zh" ? c.reason : c.reasonEn}</div>
            </GlassCard>
          ))}
        </TabsContent>

        <TabsContent value="diff" className="mt-4 space-y-4">
          {changes.map((c) => (
            <GlassCard key={c.id} className="overflow-hidden p-5">
              <div className="mb-3 flex items-center justify-between">
                <span className="font-mono text-sm">{c.file}</span>
                <Badge variant="outline"># dtoken managed</Badge>
              </div>
              <div className="grid gap-3 md:grid-cols-2">
                <div>
                  <div className="mb-1 text-xs text-rose-500">— before</div>
                  <pre className="overflow-x-auto rounded-lg border border-rose-500/15 bg-rose-500/[0.04] p-3 text-xs text-rose-700 dark:text-rose-300">{c.before}</pre>
                </div>
                <div>
                  <div className="mb-1 text-xs text-emerald-500">+ after</div>
                  <pre className="overflow-x-auto rounded-lg border border-emerald-500/15 bg-emerald-500/[0.04] p-3 text-xs text-emerald-700 dark:text-emerald-300">{c.after}</pre>
                </div>
              </div>
            </GlassCard>
          ))}
        </TabsContent>

        <TabsContent value="backup" className="mt-4">
          <div className="relative space-y-4 pl-6">
            <div className="absolute left-[9px] top-2 bottom-2 w-px bg-border" />
            {backups.map((b, i) => (
              <motion.div key={b.id} initial={{ opacity: 0, x: -8 }} animate={{ opacity: 1, x: 0 }} transition={{ delay: i * 0.06 }} className="relative">
                <span className="absolute -left-6 top-3 size-[10px] rounded-full border-2 border-background bg-primary" />
                <GlassCard className="p-4">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div>
                      <div>{b.time}</div>
                      <div className="text-xs text-muted-foreground">
                        {t("ch.affected")}: {b.files} {t("ch.files")} · {t("status.ports")} {b.ports.join(",")} · {b.procs} proc
                      </div>
                    </div>
                    <Button size="sm" variant="outline"><RotateCcw className="size-3.5" />{t("ch.rollback")}</Button>
                  </div>
                  <div className="mt-3 flex items-center gap-3 text-xs">
                    <span className="text-muted-foreground">{t("ch.beforeDoctor")}:</span>
                    <StatusBadge status={b.before} label={b.before} />
                    <span className="text-muted-foreground">→ {t("ch.afterDoctor")}:</span>
                    <StatusBadge status={b.after} label={b.after} />
                  </div>
                </GlassCard>
              </motion.div>
            ))}
          </div>
        </TabsContent>

        <TabsContent value="ownership" className="mt-4 grid gap-4 sm:grid-cols-2">
          {ownership.map((o) => (
            <GlassCard key={o.id} className="p-5">
              <h3 className="mb-3 flex items-center gap-2"><Folder className="size-4 text-primary" />{t(o.labelKey)}</h3>
              <div className="space-y-1.5">
                {o.items.map((it) => <div key={it} className="glass-inset px-3 py-2 font-mono text-xs">{it}</div>)}
              </div>
            </GlassCard>
          ))}
        </TabsContent>
      </Tabs>
    </div>
  );
}
