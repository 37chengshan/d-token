import { useState } from "react";
import { motion } from "motion/react";
import { Power, Network, ScanSearch, RefreshCw, Server, Route as RouteIcon, KeyRound, Wifi } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Switch } from "../ui/switch";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { Input } from "../ui/input";
import { useI18n, useT } from "../../lib/i18n";
import { service, ccSwitch, projects, providers, routes, ports, outputLevels } from "../../lib/fixtures";

export function Advanced() {
  const t = useT();
  const { lang } = useI18n();

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3"><h1>{t("adv.title")}</h1><DataSourceBadge /></div>

      <Tabs defaultValue="service">
        <TabsList className="flex-wrap">
          <TabsTrigger value="service">{t("adv.tab.service")}</TabsTrigger>
          <TabsTrigger value="ccswitch">{t("adv.tab.ccswitch")}</TabsTrigger>
          <TabsTrigger value="headroom">{t("adv.tab.headroom")}</TabsTrigger>
          <TabsTrigger value="routing">{t("adv.tab.routing")}</TabsTrigger>
        </TabsList>

        {/* Service & autostart */}
        <TabsContent value="service" className="mt-4 space-y-4">
          <ServiceRow />
          <GlassCard className="p-5">
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label={t("adv.bindHost")}><Input defaultValue={service.bindHost} /></Field>
              <Field label={t("adv.localPort")}><Input defaultValue={String(service.port)} /></Field>
            </div>
          </GlassCard>
        </TabsContent>

        {/* cc-switch */}
        <TabsContent value="ccswitch" className="mt-4 space-y-4">
          <CcSwitchPanel />
        </TabsContent>

        {/* Headroom scan */}
        <TabsContent value="headroom" className="mt-4 space-y-4">
          <HeadroomScan />
          <GlassCard className="p-5">
            <h3 className="mb-3">{t("adv.projectPolicies")}</h3>
            <div className="space-y-2">
              {projects.map((p) => (
                <div key={p.id} className="glass-inset flex flex-wrap items-center justify-between gap-3 p-3">
                  <div>
                    <div>{p.name}</div>
                    <div className="text-xs text-muted-foreground">{p.root} · {p.stack}</div>
                  </div>
                  <div className="flex flex-wrap items-center gap-1.5">
                    {Object.entries(p.flags).filter(([, v]) => v).map(([k]) => (
                      <Badge key={k} variant="secondary" className="text-xs">{k}</Badge>
                    ))}
                    <Badge>{outputLevels.find((l) => l.key === p.level)?.[lang]}</Badge>
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>
        </TabsContent>

        {/* Providers & routes */}
        <TabsContent value="routing" className="mt-4 space-y-4">
          <GlassCard className="p-5">
            <h3 className="mb-3 flex items-center gap-2"><Server className="size-4" />{t("adv.providers")}</h3>
            <div className="space-y-2">
              {providers.map((p) => (
                <div key={p.id} className="glass-inset flex flex-wrap items-center justify-between gap-3 p-3">
                  <div className="flex items-center gap-3">
                    <div><div>{p.name}</div><div className="text-xs text-muted-foreground">{p.protocol} · {p.baseUrl}</div></div>
                    <Badge variant={p.kind === "official" ? "default" : "outline"}>{p.kind}</Badge>
                  </div>
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <KeyRound className="size-3.5" />{t("adv.credStore")}: {p.store} · {p.keyMasked}
                    <StatusBadge status={p.health} label={p.health === "healthy" ? t("common.healthy") : t("common.warning")} />
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>

          <GlassCard className="p-5">
            <h3 className="mb-3 flex items-center gap-2"><RouteIcon className="size-4" />{t("adv.routes")}</h3>
            <div className="space-y-3">
              {routes.map((r) => (
                <div key={r.id} className="glass-inset p-3">
                  <div className="mb-2 flex items-center justify-between">
                    <span>{r.name}</span>
                    <Badge variant={r.risk === "low" ? "secondary" : "destructive"}>{r.risk}</Badge>
                  </div>
                  <div className="flex flex-wrap items-center gap-2 text-sm">
                    {[r.agent, "压缩", r.mode, r.target].map((n, i, arr) => (
                      <span key={i} className="flex items-center gap-2">
                        <span className="rounded-md bg-muted px-2 py-1 text-xs">{n}</span>
                        {i < arr.length - 1 && <span className="text-muted-foreground">→</span>}
                      </span>
                    ))}
                    <span className="ml-auto text-xs text-muted-foreground">:{r.port} · {r.fallback}</span>
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>

          <GlassCard className="p-5">
            <h3 className="mb-3 flex items-center gap-2"><Wifi className="size-4" />{t("adv.ports")}</h3>
            <div className="space-y-2">
              {ports.map((p) => (
                <div key={p.id} className="glass-inset flex items-center justify-between p-3 text-sm">
                  <span>{p.host}:{p.port} <span className="text-muted-foreground">· {p.kind}</span></span>
                  <div className="flex items-center gap-2">
                    <Badge variant={p.confidence === "confirmed" ? "default" : "outline"}>{p.confidence}</Badge>
                    <StatusBadge status={p.health} label={p.owner} />
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <div><label className="mb-1.5 block text-sm text-muted-foreground">{label}</label>{children}</div>;
}

function ServiceRow() {
  const t = useT();
  const [autostart, setAutostart] = useState(true);
  return (
    <GlassCard className="flex items-center justify-between p-5">
      <div className="flex items-center gap-3">
        <Power className="size-5 text-primary" />
        <div>
          <div>{t("adv.autostart")}</div>
          <div className="max-w-md text-sm text-muted-foreground">{t("adv.autostartDesc")}</div>
        </div>
      </div>
      <Switch checked={autostart} onCheckedChange={setAutostart} />
    </GlassCard>
  );
}

function CcSwitchPanel() {
  const t = useT();
  const { lang } = useI18n();
  const [auto, setAuto] = useState(true);
  const [mode, setMode] = useState<"auto" | "direct" | "proxy">("auto");
  const modes = [
    { k: "auto" as const, label: t("adv.ccMode.auto") },
    { k: "direct" as const, label: t("adv.ccMode.direct") },
    { k: "proxy" as const, label: t("adv.ccMode.proxy") },
  ];
  return (
    <>
      <GlassCard className="flex items-center justify-between p-5">
        <div className="flex items-center gap-3">
          <Network className="size-5 text-primary" />
          <div>
            <div>{t("adv.ccAuto")}</div>
            <div className="max-w-md text-sm text-muted-foreground">{t("adv.ccAutoDesc")}</div>
          </div>
        </div>
        <Switch checked={auto} onCheckedChange={setAuto} />
      </GlassCard>

      <GlassCard className="p-5">
        <div className="mb-3 flex items-center justify-between">
          <span className="text-sm text-muted-foreground">{t("adv.ccMode")}</span>
          <div className="flex items-center gap-2 text-sm">
            {t("adv.ccCurrent")}: <Badge>{lang === "zh" ? ccSwitch.decision : ccSwitch.decisionEn}</Badge>
          </div>
        </div>
        <div className="grid gap-2 sm:grid-cols-3">
          {modes.map((m) => (
            <button key={m.k} onClick={() => setMode(m.k)}
              className={mode === m.k ? "chip-active" : "chip"}>
              {m.label}
            </button>
          ))}
        </div>
      </GlassCard>
    </>
  );
}

function HeadroomScan() {
  const t = useT();
  const { lang } = useI18n();
  const [scanning, setScanning] = useState(false);
  const [result, setResult] = useState<{ level: string; conf: number } | null>(null);

  const scan = () => {
    setScanning(true);
    setResult(null);
    setTimeout(() => { setScanning(false); setResult({ level: "balanced", conf: 0.86 }); }, 1400);
  };

  return (
    <GlassCard className="p-5">
      <div className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ScanSearch className="size-5 text-primary" />
          <Input placeholder="~/code/web-dashboard" defaultValue="~/code/web-dashboard" className="w-64" />
        </div>
        <Button variant="outline" onClick={scan} disabled={scanning}>
          {scanning ? <RefreshCw className="size-4 animate-spin" /> : <ScanSearch className="size-4" />}
          {scanning ? t("adv.scanning") : t("adv.scanProject")}
        </Button>
      </div>

      {result && (
        <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} className="glass-inset p-4">
          <div className="mb-3 flex items-center justify-between">
            <span className="text-sm text-muted-foreground">{t("adv.recommendLevel")}</span>
            <span className="text-sm">{t("common.confidence")}: {Math.round(result.conf * 100)}%</span>
          </div>
          <div className="mb-4 flex flex-wrap gap-2">
            {outputLevels.map((l) => (
              <Badge key={l.key} variant={l.key === result.level ? "default" : "outline"} className="gap-1">
                {l[lang]}{l.key === result.level && ` · ${t("common.recommended")}`}
              </Badge>
            ))}
          </div>
          <div className="mb-4 text-xs text-muted-foreground">映射: HEADROOM_OUTPUT_SHAPER=1 · HEADROOM_OUTPUT_HOLDOUT=0.1</div>
          <Button size="sm">{t("adv.applyPolicy")}</Button>
        </motion.div>
      )}
    </GlassCard>
  );
}
