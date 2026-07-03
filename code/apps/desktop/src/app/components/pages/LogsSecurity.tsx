import { useMemo, useState } from "react";
import { motion } from "motion/react";
import { Download, KeyRound, RefreshCcw, ShieldCheck, Filter, Route, Gauge, Server, AlertTriangle, Layers } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Terminal } from "../common/Terminal";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { useI18n, useT } from "../../lib/i18n";
import { logs, secrets, type LogEntry } from "../../lib/fixtures";

const filterMeta: Record<
  LogEntry["type"] | "all",
  { zh: string; en: string; icon: typeof Layers }
> = {
  all: { zh: "全部", en: "All", icon: Layers },
  route: { zh: "route", en: "route", icon: Route },
  compression: { zh: "compression", en: "compression", icon: Gauge },
  provider: { zh: "provider", en: "provider", icon: Server },
  error: { zh: "error", en: "error", icon: AlertTriangle },
};

export function LogsSecurity() {
  const t = useT();
  const { lang } = useI18n();
  const [filter, setFilter] = useState<LogEntry["type"] | "all">("all");
  const types: (LogEntry["type"] | "all")[] = ["all", "route", "compression", "provider", "error"];

  const counts = useMemo(() => {
    const next = { all: logs.length, route: 0, compression: 0, provider: 0, error: 0 };
    logs.forEach((entry) => {
      next[entry.type] += 1;
    });
    return next;
  }, []);

  const shown = logs.filter((l) => filter === "all" || l.type === filter);

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <h1>{t("ls.title")}</h1>
        <DataSourceBadge />
      </div>

      <Tabs defaultValue="logs">
        <TabsList>
          <TabsTrigger value="logs">{t("ls.tab.logs")}</TabsTrigger>
          <TabsTrigger value="secrets">{t("ls.tab.secrets")}</TabsTrigger>
        </TabsList>

        <TabsContent value="logs" className="mt-4 space-y-4">
          <GlassCard className="p-3" hover={false}>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Filter className="size-4" />
                <span>{lang === "zh" ? "日志筛选" : "Log filters"}</span>
              </div>

              <div className="log-filter-bar">
                {types.map((ty) => {
                  const meta = filterMeta[ty];
                  const Icon = meta.icon;
                  return (
                    <motion.button
                      key={ty}
                      type="button"
                      onClick={() => setFilter(ty)}
                      className={filter === ty ? "log-filter-item active" : "log-filter-item"}
                      whileHover={{ y: -1 }}
                      whileTap={{ scale: 0.98 }}
                    >
                      <Icon className="size-3.5" />
                      <span>{lang === "zh" ? meta.zh : meta.en}</span>
                      <span className="log-filter-count">{counts[ty]}</span>
                    </motion.button>
                  );
                })}
              </div>

              <Button variant="outline" size="sm">
                <Download className="size-4" />
                {t("ls.export")}
              </Button>
            </div>
          </GlassCard>

          <Terminal lines={shown} />
        </TabsContent>

        <TabsContent value="secrets" className="mt-4 space-y-3">
          <div className="glass-inset flex items-center gap-2 p-3 text-sm text-muted-foreground">
            <ShieldCheck className="size-4 text-emerald-500" />
            {lang === "zh"
              ? "密钥仅以脱敏形式展示，界面中不会显示明文。"
              : "Secrets are shown only in redacted form; plaintext keys are never displayed."}
          </div>
          {secrets.map((s, i) => (
            <motion.div
              key={s.id}
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: i * 0.05 }}
            >
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
                  <StatusBadge
                    status={s.status === "valid" ? "healthy" : "warning"}
                    label={s.status === "valid" ? t("ls.keyValid") : t("ls.keyExpired")}
                  />
                  <Button size="sm" variant="outline">
                    <RefreshCcw className="size-3.5" />
                    {t("ls.rotate")}
                  </Button>
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
