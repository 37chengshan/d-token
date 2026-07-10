import { useEffect, useMemo, useState } from "react";
import { Laptop, Monitor, RefreshCw, Route, ShieldCheck, TerminalSquare } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { StatusBadge } from "../common/StatusBadge";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { useI18n, useT } from "../../lib/i18n";
import { invokeNative } from "../../lib/native";

type CompatibilityAgent = {
  name: string;
  rtk?: string;
  rtkMethod?: string;
  headroom?: string;
  headroomMethod?: string;
  macos?: string;
  win?: string;
  wsl?: string;
  note?: string;
};

type ProjectPolicyRow = {
  root?: string;
  stack?: string;
  agents?: string[];
  route?: string;
  rtkProfile?: string;
  policy?: string;
  level?: string;
  reason?: string;
  features?: string[];
};

type DesktopCompatibilityData = {
  agents?: CompatibilityAgent[];
  routes?: Array<Record<string, any>>;
  providers?: Array<Record<string, any>>;
  projects?: ProjectPolicyRow[];
  health?: Array<{ key: string; label: string; state: string }>;
  runtimeCompatibility?: {
    source?: string;
    agents?: RuntimeAgentProfile[];
    protocolInjection?: RuntimeProtocolProfile[];
    chainStatus?: Record<string, any>;
    privacy?: string;
  };
};

type RuntimeAgentProfile = {
  agent?: string;
  rtk_supported?: boolean;
  rtk_method?: string;
  rtk_transparent_rewrite?: string;
  headroom_supported?: boolean;
  headroom_method?: string;
  macos_support?: string;
  windows_native_support?: string;
  windows_wsl_support?: string;
  notes?: string;
};

type RuntimeProtocolProfile = {
  protocol?: string;
  compatible_clients?: string[];
  injection_method?: string;
  managed_mode_action?: string;
  safety_notes?: string[];
};

const previewAgents: CompatibilityAgent[] = [
  {
    name: "OpenCode",
    rtk: "partial",
    rtkMethod: "integrated runtime metrics",
    headroom: "partial",
    headroomMethod: "OpenAI-compatible d-token route adapter",
    macos: "ok",
    win: "partial",
    wsl: "ok",
    note: "First real validation entry; still one agent inside the shared model.",
  },
  {
    name: "Claude Code",
    rtk: "planned",
    rtkMethod: "integrated RTK via d-token runtime",
    headroom: "planned",
    headroomMethod: "Anthropic-shaped bridge through d-token route adapter",
    macos: "ok",
    win: "partial",
    wsl: "ok",
    note: "Uses the same install/connect/configure/verify product flow.",
  },
  {
    name: "Codex CLI",
    rtk: "planned",
    rtkMethod: "command filter + route adapter",
    headroom: "planned",
    headroomMethod: "OpenAI Responses / OpenAI-compatible route support",
    macos: "ok",
    win: "ok",
    wsl: "ok",
    note: "Compatibility model present; real validation is not expanded beyond OpenCode yet.",
  },
];

export function Compatibility() {
  const t = useT();
  const { lang } = useI18n();
  const [data, setData] = useState<DesktopCompatibilityData | null>(null);
  const [state, setState] = useState<"loading" | "native" | "preview" | "error">("loading");
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);

  const refresh = async () => {
    try {
      const next = await invokeNative<DesktopCompatibilityData>("get_desktop_data");
      setData(next);
      setState("native");
      setUpdatedAt(new Date());
    } catch (error) {
      setData(null);
      setState(error instanceof Error && error.message === "native_unavailable" ? "preview" : "error");
      setUpdatedAt(new Date());
    }
  };

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(timer);
  }, []);

  const runtimeAgents =
    state === "native" && Array.isArray(data?.runtimeCompatibility?.agents)
      ? data.runtimeCompatibility.agents
      : [];
  const protocolProfiles =
    state === "native" && Array.isArray(data?.runtimeCompatibility?.protocolInjection)
      ? data.runtimeCompatibility.protocolInjection
      : [];
  const chainStatus = state === "native" ? data?.runtimeCompatibility?.chainStatus : null;
  const agents = runtimeAgents.length > 0
    ? runtimeAgents.map(toCompatibilityAgent)
    : state === "native" && Array.isArray(data?.agents)
      ? data.agents
      : previewAgents;
  const routes: Array<Record<string, any>> = state === "native" && Array.isArray(data?.routes) ? data.routes : [];
  const projects = state === "native" && Array.isArray(data?.projects) ? data.projects : [];
  const summary = useMemo(() => summarizeAgents(agents), [agents]);
  const sourceText =
    state === "native"
      ? t("cp.source.native")
      : state === "preview"
        ? t("cp.source.preview")
        : state === "loading"
          ? lang === "zh" ? "读取中" : "Loading"
          : lang === "zh" ? "读取失败" : "Unavailable";

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex flex-wrap items-center gap-3">
            <h1>{t("cp.title")}</h1>
            <DataSourceBadge mode={state === "native" ? "native" : state === "preview" ? "preview" : "unavailable"} />
            <Badge variant={state === "native" ? "default" : "secondary"}>
              {sourceText}{updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}
            </Badge>
          </div>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">{t("cp.subtitle")}</p>
        </div>
        <Button variant="outline" onClick={refresh}>
          <RefreshCw className="size-4" />
          {lang === "zh" ? "刷新" : "Refresh"}
        </Button>
      </div>

      <div className="grid gap-4 md:grid-cols-4">
        <SummaryCard icon={<Monitor className="size-4" />} label="Agents" value={String(agents.length)} />
        <SummaryCard icon={<ShieldCheck className="size-4" />} label="OK" value={String(summary.ok)} tone="ok" />
        <SummaryCard icon={<TerminalSquare className="size-4" />} label="Partial" value={String(summary.partial)} tone="warn" />
        <SummaryCard icon={<Route className="size-4" />} label="Routes" value={String(routes.length)} />
      </div>

      <GlassCard className="p-5">
        <div className="mb-4 flex items-center justify-between gap-3">
          <h3>{t("cp.summary")}</h3>
          <Badge variant="outline">multi-agent control plane</Badge>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[860px] text-sm">
            <thead>
              <tr className="border-b border-[var(--glass-inset-border)] text-left text-xs text-muted-foreground">
                <th className="pb-2 font-medium">{t("cp.agent")}</th>
                <th className="pb-2 font-medium">{t("cp.rtk")}</th>
                <th className="pb-2 font-medium">{t("cp.runtime")}</th>
                <th className="pb-2 font-medium">{t("cp.macos")}</th>
                <th className="pb-2 font-medium">{t("cp.windows")}</th>
                <th className="pb-2 font-medium">{t("cp.wsl")}</th>
                <th className="pb-2 font-medium">{t("cp.notes")}</th>
              </tr>
            </thead>
            <tbody>
              {agents.map((agent) => (
                <tr key={agent.name} className="border-b border-border/30 last:border-0">
                  <td className="py-3">
                    <div className="font-medium">{agent.name}</div>
                    <div className="text-xs text-muted-foreground">{agent.headroomMethod ?? "d-token-owned runtime"}</div>
                  </td>
                  <td className="py-3"><MatrixStatus value={agent.rtk} detail={agent.rtkMethod} /></td>
                  <td className="py-3"><MatrixStatus value={agent.headroom} detail={agent.headroomMethod} /></td>
                  <td className="py-3"><MatrixStatus value={agent.macos} /></td>
                  <td className="py-3"><MatrixStatus value={agent.win} /></td>
                  <td className="py-3"><MatrixStatus value={agent.wsl} /></td>
                  <td className="py-3 text-xs text-muted-foreground">{agent.note ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </GlassCard>

      <div className="grid gap-4 lg:grid-cols-2">
        <GlassCard className="p-5">
          <h3 className="mb-3 flex items-center gap-2">
            <Route className="size-4 text-primary" />
            {t("cp.routeModes")}
          </h3>
          <div className="space-y-2">
            {(routes.length > 0 ? routes : previewRouteRows() as Array<Record<string, any>>).map((route, index) => (
              <div key={String(route.id ?? route.name ?? index)} className="glass-inset flex flex-wrap items-center justify-between gap-3 p-3 text-sm">
                <div>
                  <div>{String(route.name ?? route.id ?? "d-token route")}</div>
                  <div className="text-xs text-muted-foreground">
                    {String(route.protocol ?? "OpenAI/Anthropic")} · {String(route.chain ?? route.policy ?? "compress_then_provider")} · {String(route.fallback ?? "fallback visible")}
                  </div>
                </div>
                <Badge variant={String(route.fallback ?? "").includes("disabled") ? "default" : "outline"}>
                  {String(route.provider ?? route.host ?? "d-token")}
                </Badge>
              </div>
            ))}
          </div>
        </GlassCard>

        <GlassCard className="p-5">
          <h3 className="mb-3 flex items-center gap-2">
            <Laptop className="size-4 text-primary" />
            {t("cp.projects")}
          </h3>
          <div className="space-y-2">
            {(projects.length > 0 ? projects : previewProjectRows()).map((project, index) => (
              <div key={`${project.root ?? "project"}-${index}`} className="glass-inset p-3 text-sm">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="font-medium">{project.root ?? "current workspace"}</span>
                  <Badge variant="secondary">{project.level ?? project.policy ?? "policy pending"}</Badge>
                </div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {project.stack ?? "workspace"} · {(project.agents ?? []).join(", ") || "multi-agent"} · {project.route ?? "d-token local route"}
                </div>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {(project.features ?? ["Doctor", "config diff", "backup", "route verification"]).map((feature) => (
                    <Badge key={feature} variant="outline" className="text-xs">{feature}</Badge>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </GlassCard>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <GlassCard className="p-5">
          <h3 className="mb-3 flex items-center gap-2">
            <ShieldCheck className="size-4 text-primary" />
            {lang === "zh" ? "协议接入" : "Protocol injection"}
          </h3>
          <div className="space-y-2">
            {(protocolProfiles.length > 0 ? protocolProfiles : previewProtocolRows()).map((profile, index) => (
              <div key={String(profile.protocol ?? index)} className="glass-inset p-3 text-sm">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="font-medium">{String(profile.protocol ?? "open_ai")}</span>
                  <Badge variant="outline">{lang === "zh" ? "无静默回退" : "no silent fallback"}</Badge>
                </div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {String(profile.injection_method ?? "d-token route adapter")}
                </div>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {(profile.compatible_clients ?? []).slice(0, 5).map((client) => (
                    <Badge key={client} variant="secondary" className="text-xs">{client}</Badge>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </GlassCard>

        <GlassCard className="p-5">
          <h3 className="mb-3 flex items-center gap-2">
            <Route className="size-4 text-primary" />
            {lang === "zh" ? "链路 / cc-switch 状态" : "Chain / cc-switch status"}
          </h3>
          <div className="glass-inset p-3 text-sm">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="font-medium">{String(chainStatus?.mode ?? "compress_then_chain_bridge")}</span>
              <Badge variant={chainStatus?.status === "configured" ? "default" : "secondary"}>
                {String(chainStatus?.status ?? (state === "preview" ? "preview" : "unavailable"))}
              </Badge>
            </div>
            <div className="mt-1 text-xs text-muted-foreground">
              {String(chainStatus?.privacy ?? "d-token reads local route state only; external cc-switch is not mutated.")}
            </div>
            <div className="mt-3 space-y-2">
              {Array.isArray(chainStatus?.routes) && chainStatus.routes.length > 0 ? (
                chainStatus.routes.map((route: Record<string, any>, index: number) => (
                  <div key={String(route.route_id ?? index)} className="flex flex-wrap items-center justify-between gap-2 text-xs">
                    <span>{String(route.name ?? route.route_id)}</span>
                    <Badge variant="outline">{String(route.fallback ?? "disabled")}</Badge>
                  </div>
                ))
              ) : (
                <div className="text-xs text-muted-foreground">
                  {lang === "zh" ? "等待 d-token runtime 暴露链路状态。" : "Waiting for d-token runtime chain status."}
                </div>
              )}
            </div>
          </div>
        </GlassCard>
      </div>
    </div>
  );
}

function SummaryCard({ icon, label, value, tone }: { icon: React.ReactNode; label: string; value: string; tone?: "ok" | "warn" }) {
  const color = tone === "ok" ? "text-emerald-500" : tone === "warn" ? "text-amber-500" : "text-primary";
  return (
    <GlassCard className="p-4">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">{icon}{label}</div>
      <div className={`mt-2 text-2xl font-semibold ${color}`}>{value}</div>
    </GlassCard>
  );
}

function MatrixStatus({ value, detail }: { value?: string; detail?: string }) {
  const normalized = String(value ?? "planned").toLowerCase();
  const status = normalized === "ok" ? "healthy" : normalized === "partial" || normalized === "planned" ? "warning" : "error";
  return (
    <div className="space-y-1">
      <StatusBadge status={status} label={value ?? "planned"} />
      {detail ? <div className="max-w-[180px] text-xs text-muted-foreground">{detail}</div> : null}
    </div>
  );
}

function summarizeAgents(agents: CompatibilityAgent[]) {
  return agents.reduce(
    (acc, agent) => {
      for (const value of [agent.rtk, agent.headroom, agent.macos, agent.win, agent.wsl]) {
        const normalized = String(value ?? "").toLowerCase();
        if (normalized === "ok") acc.ok += 1;
        else if (normalized === "partial" || normalized === "planned") acc.partial += 1;
        else if (normalized) acc.fail += 1;
      }
      return acc;
    },
    { ok: 0, partial: 0, fail: 0 },
  );
}

function previewRouteRows() {
  return [
    { name: "agent -> d-token route adapter", protocol: "OpenAI/Anthropic", chain: "compress_then_provider", fallback: "fallback disabled", provider: "official/custom/local" },
    { name: "agent -> d-token -> cc-switch", protocol: "bridge", chain: "compress_then_cc_switch", fallback: "visible only", provider: "cc-switch interface" },
  ];
}

function previewProjectRows(): ProjectPolicyRow[] {
  return [
    {
      root: "browser preview",
      stack: "TypeScript / Rust / Tauri",
      agents: ["OpenCode", "Claude Code", "Codex CLI"],
      route: "d-token local service",
      level: "balanced recommendation pending",
      features: ["Doctor", "Project policy", "Event Ledger", "Backup"],
    },
  ];
}

function toCompatibilityAgent(profile: RuntimeAgentProfile): CompatibilityAgent {
  return {
    name: profile.agent ?? "Agent",
    rtk: supportToStatus(profile.rtk_supported, profile.rtk_transparent_rewrite),
    rtkMethod: profile.rtk_method,
    headroom: supportToStatus(profile.headroom_supported, undefined),
    headroomMethod: profile.headroom_method,
    macos: supportToStatus(undefined, profile.macos_support),
    win: supportToStatus(undefined, profile.windows_native_support),
    wsl: supportToStatus(undefined, profile.windows_wsl_support),
    note: profile.notes,
  };
}

function supportToStatus(supported?: boolean, level?: string) {
  const normalized = String(level ?? "").toLowerCase();
  if (normalized === "full") return "ok";
  if (normalized === "partial") return "partial";
  if (normalized === "none") return "none";
  if (supported === true) return "ok";
  if (supported === false) return "planned";
  return "planned";
}

function previewProtocolRows(): RuntimeProtocolProfile[] {
  return [
    {
      protocol: "open_ai",
      compatible_clients: ["OpenAI SDK", "OpenAI-compatible clients", "Codex CLI"],
      injection_method: "Proxy/base-url injection for chat or responses traffic",
    },
    {
      protocol: "anthropic",
      compatible_clients: ["Claude Code", "Cline", "Anthropic-compatible proxy clients"],
      injection_method: "Anthropic messages bridge through d-token route adapter",
    },
  ];
}
