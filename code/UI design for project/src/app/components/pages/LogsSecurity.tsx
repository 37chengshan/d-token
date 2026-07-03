import { useState } from "react";
import { motion } from "motion/react";
import { Download, KeyRound, RefreshCcw, ShieldCheck, Filter } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Terminal } from "../common/Terminal";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { useI18n, useT } from "../../lib/i18n";
import { logs, secrets, type LogEntry } from "../../lib/fixtures";

export function LogsSecurity() {
  const t = useT();
  const { lang } = useI18n();
  const [filter, setFilter] = useState<LogEntry["type"] | "all">("all");
  const types: (LogEntry["type"] | "all")[] = ["all", "route", "compression", "provider", "error"];
  const shown = logs.filter((l) => filter === "all" || l.type === filter);

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3"><h1>{t("ls.title")}</h1><DataSourceBadge /></div>

      <Tabs defaultValue="logs">
        <TabsList>
          <TabsTrigger value="logs">{t("ls.tab.logs")}</TabsTrigger>
          <TabsTrigger value="secrets">{t("ls.tab.secrets")}</TabsTrigger>
        </TabsList>

        <TabsContent value="logs" className="mt-4 space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex flex-wrap items-center gap-1.5">
              <Filter className="size-4 text-muted-foreground" />
              {types.map((ty) => (
                <button key={ty} onClick={() => setFilter(ty)}
                  className={filter === ty ? "chip-active" : "chip"}>
                  {ty === "all" ? t("ls.allTypes") : ty}
                </button>
              ))}
            </div>
            <Button variant="outline" size="sm"><Download className="size-4" />{t("ls.export")}</Button>
          </div>

          <Terminal lines={shown} />
        </TabsContent>

        <TabsContent value="secrets" className="mt-4 space-y-3">
          <div className="glass-inset flex items-center gap-2 p-3 text-sm text-muted-foreground">
            <ShieldCheck className="size-4 text-emerald-500" />密钥仅以脱敏形式展示，绝不在界面明文显示。
          </div>
          {secrets.map((s, i) => (
            <motion.div key={s.id} initial={{ opacity: 0, x: -8 }} animate={{ opacity: 1, x: 0 }} transition={{ delay: i * 0.05 }}>
              <GlassCard className="flex flex-wrap items-center justify-between gap-3 p-4">
                <div className="flex items-center gap-3">
                  <KeyRound className="size-5 text-primary" />
                  <div>
                    <div className="font-mono text-sm">{s.name}</div>
                    <div className="font-mono text-xs text-muted-foreground">{s.masked}</div>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="secondary">{s.store === "keychain" ? t("ls.storeKeychain") : s.store}</Badge>
                  <StatusBadge status={s.status === "valid" ? "healthy" : "warning"} label={s.status === "valid" ? t("ls.keyValid") : t("ls.keyExpired")} />
                  <Button size="sm" variant="outline"><RefreshCcw className="size-3.5" />{t("ls.rotate")}</Button>
                  <Button size="sm" variant="ghost">{t("common.test")}</Button>
                </div>
              </GlassCard>
            </motion.div>
          ))}
        </TabsContent>
      </Tabs>
    </div>
  );
}
