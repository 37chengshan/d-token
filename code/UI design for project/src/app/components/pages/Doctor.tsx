import { useState } from "react";
import { motion } from "motion/react";
import { RefreshCw, Wrench, RotateCcw, Trash2, PackageOpen, ChevronRight } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Button } from "../ui/button";
import { useI18n, useT } from "../../lib/i18n";
import { doctorChecks, type DoctorCheck } from "../../lib/fixtures";
import type { PageId } from "../../lib/nav";

const LAYERS: DoctorCheck["layer"][] = ["runtime", "port", "route", "provider", "security"];

export function Doctor({ setPage }: { setPage: (p: PageId) => void }) {
  const t = useT();
  const { lang } = useI18n();
  const [running, setRunning] = useState(false);

  const repairTarget = (r?: string): PageId => (r === "advanced" ? "advanced" : r === "logs" ? "logs" : "changes");

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3"><h1>{t("dr.title")}</h1><DataSourceBadge /></div>
        <Button variant="outline" onClick={() => { setRunning(true); setTimeout(() => setRunning(false), 1200); }} disabled={running}>
          <RefreshCw className={`size-4 ${running ? "animate-spin" : ""}`} />{t("dr.runAll")}
        </Button>
      </div>

      <Tabs defaultValue="checks">
        <TabsList>
          <TabsTrigger value="checks">{t("dr.tab.checks")}</TabsTrigger>
          <TabsTrigger value="runtime">{t("dr.tab.runtime")}</TabsTrigger>
          <TabsTrigger value="maintenance">{t("dr.tab.maintenance")}</TabsTrigger>
        </TabsList>

        <TabsContent value="checks" className="mt-4 space-y-4">
          {LAYERS.map((layer) => (
            <GlassCard key={layer} className="p-5">
              <h3 className="mb-3">{t(`dr.layer.${layer}`)}</h3>
              <div className="space-y-2">
                {doctorChecks.filter((c) => c.layer === layer).map((c, i) => (
                  <motion.div key={c.id} initial={{ opacity: 0, x: -8 }} animate={{ opacity: 1, x: 0 }} transition={{ delay: i * 0.04 }}
                    className="glass-inset flex flex-wrap items-center justify-between gap-2 p-3">
                    <div>
                      <div>{lang === "zh" ? c.name : c.nameEn}</div>
                      <div className="text-xs text-muted-foreground">{lang === "zh" ? c.detail : c.detailEn}</div>
                    </div>
                    <div className="flex items-center gap-2">
                      <StatusBadge status={c.status} label={c.status === "healthy" ? t("common.healthy") : c.status === "warning" ? t("common.warning") : t("common.error")} />
                      {c.status !== "healthy" && (
                        <Button size="sm" variant="ghost" onClick={() => setPage(repairTarget(c.repair))} className="text-primary">
                          {t("dr.repair")}<ChevronRight className="size-3.5" />
                        </Button>
                      )}
                    </div>
                  </motion.div>
                ))}
              </div>
            </GlassCard>
          ))}
        </TabsContent>

        <TabsContent value="runtime" className="mt-4 space-y-3">
          {[
            { n: "d-token runtime", s: "healthy" as const, d: "PID 4821 · v1.1.8" },
            { n: "压缩层 (RTK)", s: "healthy" as const, d: "v0.9 已集成" },
            { n: "本地代理", s: "healthy" as const, d: ":8787 监听中" },
            { n: "Provider 连接", s: "warning" as const, d: "自定义 OpenAI 401" },
          ].map((r) => (
            <GlassCard key={r.n} className="flex items-center justify-between p-4">
              <div><div>{r.n}</div><div className="text-xs text-muted-foreground">{r.d}</div></div>
              <StatusBadge status={r.s} label={r.s === "healthy" ? t("common.healthy") : t("common.warning")} />
            </GlassCard>
          ))}
        </TabsContent>

        <TabsContent value="maintenance" className="mt-4 space-y-4">
          <p className="text-sm text-muted-foreground">{t("dr.maint.desc")}</p>
          <div className="grid gap-4 sm:grid-cols-2">
            {[
              { icon: RotateCcw, label: t("dr.maint.restart") },
              { icon: PackageOpen, label: t("dr.maint.release") },
              { icon: Wrench, label: t("dr.maint.rebuild") },
              { icon: Trash2, label: t("dr.maint.clean") },
            ].map((m) => (
              <GlassCard key={m.label} className="flex items-center justify-between p-4">
                <span className="flex items-center gap-3"><m.icon className="size-5 text-primary" />{m.label}</span>
                <Button size="sm" variant="outline">{t("common.confirm")}</Button>
              </GlassCard>
            ))}
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}
