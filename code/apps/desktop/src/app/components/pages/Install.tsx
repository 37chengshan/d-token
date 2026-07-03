import { useMemo, useState } from "react";
import { motion, AnimatePresence } from "motion/react";
import { Rocket, Power, RefreshCw, Search, ShieldCheck, Cpu, Server, CheckCircle2 } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StepProgress, type Step } from "../common/StepProgress";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Button } from "../ui/button";
import { Checkbox } from "../ui/checkbox";
import { Badge } from "../ui/badge";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "../ui/dialog";
import { useI18n, useT } from "../../lib/i18n";
import { service, agents as agentData, ccSwitch, type Agent } from "../../lib/fixtures";

export function Install() {
  const t = useT();
  const { lang } = useI18n();
  const steps: Step[] = [
    { key: "scan", label: t("in.step.scan") },
    { key: "plan", label: t("in.step.plan") },
    { key: "apply", label: t("in.step.apply") },
    { key: "verify", label: t("in.step.verify") },
    { key: "doctor", label: t("in.step.doctor") },
  ];

  const [running, setRunning] = useState(service.running);
  const [installing, setInstalling] = useState(false);
  const [stepIdx, setStepIdx] = useState(-1);
  const [detecting, setDetecting] = useState(false);
  const [agents, setAgents] = useState<Agent[]>(agentData);
  const [selected, setSelected] = useState<string[]>([]);
  const [showDiff, setShowDiff] = useState(false);
  const [configuring, setConfiguring] = useState(false);

  const done = stepIdx >= steps.length;

  const transactionLine = useMemo(() => {
    if (!running) return "> transaction idle · service stopped";
    if (installing) {
      const current = steps[Math.min(stepIdx, steps.length - 1)]?.key ?? "scan";
      return `> transaction ${current} · scan -> plan -> apply -> verify -> doctor`;
    }
    if (done) return "> transaction complete · rollback point: fixture-preview";
    return "> transaction ready · waiting for operator action";
  }, [done, installing, running, stepIdx, steps]);

  const runtimeLine = running
    ? `> runtime ${service.bindHost}:${service.port} running · mode=${service.mode}`
    : `> runtime ${service.bindHost}:${service.port} stopped · no process touched`;

  const runInstall = () => {
    setRunning(true);
    setInstalling(true);
    setStepIdx(0);

    let i = 0;
    const timer = window.setInterval(() => {
      i += 1;
      if (i >= steps.length) {
        window.clearInterval(timer);
        setStepIdx(steps.length);
        window.setTimeout(() => setInstalling(false), 650);
      } else {
        setStepIdx(i);
      }
    }, 650);
  };

  const runDetect = () => {
    setDetecting(true);
    window.setTimeout(() => setDetecting(false), 900);
  };

  const toggle = (id: string) => {
    setSelected((current) =>
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id],
    );
  };

  const confirmConfigure = () => {
    setConfiguring(true);
    window.setTimeout(() => {
      setAgents((current) =>
        current.map((agent) =>
          selected.includes(agent.id) ? { ...agent, configured: true } : agent,
        ),
      );
      setSelected([]);
      setConfiguring(false);
      setShowDiff(false);
    }, 900);
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <h1>{t("in.title")}</h1>
        <DataSourceBadge />
      </div>
      <p className="-mt-3 text-sm text-muted-foreground">{t("in.subtitle")}</p>

      <GlassCard strong className="p-6">
        <div className="flex flex-col gap-5 md:flex-row md:items-center md:justify-between">
          <div className="flex items-center gap-4">
            <div
              className="flex size-12 items-center justify-center rounded-2xl text-white shadow-lg"
              style={{ background: running ? "var(--accent-500)" : "var(--muted-foreground)" }}
            >
              <Server className="size-6" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <span className="font-semibold">
                  {running ? t("in.serviceRunning") : t("in.serviceStopped")}
                </span>
                <StatusBadge
                  status={running ? "healthy" : "error"}
                  label={running ? t("status.running") : t("status.stopped")}
                />
              </div>
              <div className="mt-1 text-sm text-muted-foreground">
                {t("in.version")} {service.version} · {t("in.port")} {service.port} · {service.bindHost}
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {running ? (
              <>
                <Button variant="outline" onClick={runInstall} disabled={installing}>
                  <RefreshCw className="size-4" />
                  {t("in.restart")}
                </Button>
                <Button
                  variant="outline"
                  onClick={() => {
                    setRunning(false);
                    setInstalling(false);
                    setStepIdx(-1);
                  }}
                >
                  <Power className="size-4" />
                  {t("in.stop")}
                </Button>
              </>
            ) : (
              <Button onClick={runInstall} disabled={installing} className="shadow-lg">
                <Rocket className="size-4" />
                {installing ? t("in.installing") : t("in.oneClick")}
              </Button>
            )}
          </div>
        </div>

        <div className="install-status-strip mt-5">
          <div>{runtimeLine}</div>
          <div>{transactionLine}</div>
        </div>

        <AnimatePresence>
          {(installing || done) && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              className="mt-6 overflow-hidden border-t border-border/50 pt-6"
            >
              <StepProgress steps={steps} current={Math.min(stepIdx, steps.length - 1)} done={done} />
            </motion.div>
          )}
        </AnimatePresence>
      </GlassCard>

      <GlassCard className="p-6">
        <div className="mb-4 flex items-center justify-between">
          <h3 className="flex items-center gap-2">
            <Search className="size-5" />
            {t("in.detect")}
          </h3>
          <Button variant="outline" onClick={runDetect} disabled={detecting}>
            {detecting ? <RefreshCw className="size-4 animate-spin" /> : <Search className="size-4" />}
            {detecting ? t("in.detecting") : t("in.detect")}
          </Button>
        </div>

        <div className="glass-inset mb-4 flex flex-wrap items-center justify-between gap-3 p-4">
          <div className="flex items-center gap-3">
            <Cpu className="size-5 text-primary" />
            <div>
              <div>{t("in.ccswitch")}</div>
              <div className="text-sm text-muted-foreground">:{ccSwitch.port}</div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <StatusBadge
              status={ccSwitch.detected ? "healthy" : "error"}
              label={ccSwitch.detected ? t("in.installed") : t("in.notFound")}
            />
            <Badge variant={ccSwitch.proxyEnabled ? "default" : "secondary"}>
              {ccSwitch.proxyEnabled ? t("in.ccProxyOn") : t("in.ccProxyOff")}
            </Badge>
          </div>
        </div>

        <div className="mb-3 text-sm text-muted-foreground">{t("in.selectAgents")}</div>
        <div className="space-y-2">
          {agents.map((agent, index) => (
            <motion.div
              key={agent.id}
              initial={{ opacity: 0, x: -10 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: index * 0.05 }}
              className="glass-inset flex flex-wrap items-center gap-3 p-3"
            >
              <Checkbox
                checked={selected.includes(agent.id)}
                disabled={!agent.installed || agent.configured}
                onCheckedChange={() => toggle(agent.id)}
              />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span>{agent.name}</span>
                  {agent.version && <span className="text-xs text-muted-foreground">v{agent.version}</span>}
                  {agent.configured && (
                    <Badge variant="secondary" className="gap-1">
                      <CheckCircle2 className="size-3" />
                      {t("in.configured")}
                    </Badge>
                  )}
                </div>
                <div className="truncate text-xs text-muted-foreground">{agent.binaryPath ?? "—"}</div>
              </div>
              <div className="hidden text-xs text-muted-foreground sm:block">
                {t("in.configMethod")}: {lang === "zh" ? agent.configMethod : agent.configMethodEn}
              </div>
              <StatusBadge
                status={agent.installed ? "healthy" : "warning"}
                label={agent.installed ? t("in.installed") : t("in.notFound")}
              />
            </motion.div>
          ))}
        </div>

        <div className="mt-4 flex justify-end">
          <Button disabled={selected.length === 0} onClick={() => setShowDiff(true)}>
            <ShieldCheck className="size-4" />
            {t("in.configure")} {selected.length > 0 && `(${selected.length})`}
          </Button>
        </div>
      </GlassCard>

      <Dialog open={showDiff} onOpenChange={setShowDiff}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("in.previewDiff")}</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            {selected.map((id) => {
              const agent = agents.find((item) => item.id === id)!;
              return (
                <div key={id} className="glass-inset p-3 text-sm">
                  <div className="mb-1 font-semibold">{agent.name}</div>
                  <div className="text-muted-foreground">
                    {t("in.configMethod")}: {lang === "zh" ? agent.configMethod : agent.configMethodEn}
                  </div>
                  <pre className="mt-2 overflow-x-auto rounded bg-muted/60 p-2 text-xs">
{`+ ANTHROPIC_BASE_URL="http://127.0.0.1:${service.port}"
+ NO_PROXY="localhost,127.0.0.1"`}
                  </pre>
                </div>
              );
            })}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowDiff(false)}>{t("common.cancel")}</Button>
            <Button onClick={confirmConfigure} disabled={configuring}>
              {configuring ? <RefreshCw className="size-4 animate-spin" /> : <CheckCircle2 className="size-4" />}
              {configuring ? t("in.configuring") : t("common.confirm")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
