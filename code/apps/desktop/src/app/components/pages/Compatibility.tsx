import { useEffect, useMemo, useState } from "react";
import { Laptop, Monitor, RefreshCw, Route, ShieldCheck, TerminalSquare } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { StatusBadge } from "../common/StatusBadge";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { useI18n, useT } from "../../lib/i18n";
import { invokeNative } from "../../lib/native";
import { classifyEvidence, type EvidenceState } from "../../lib/state/evidence";

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
  policy?: string;
  level?: string;
  features?: string[];
};

type DesktopCompatibilityData = Record<string, any> & {
  agents?: CompatibilityAgent[];
  routes?: Array<Record<string, any>>;
  projects?: ProjectPolicyRow[];
  runtimeCompatibility?: {
    source?: string;
    agents?: RuntimeAgentProfile[];
    protocolInjection?: RuntimeProtocolProfile[];
    chainStatus?: Record<string, any> | null;
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

export const compatibilityCatalogEvidence = {
  source: "dtoken-capability-manifest",
  evidence_kind: "implementation_catalog",
  verified_on_this_machine: false,
} as const;

type LoadState = "loading" | "native" | "preview" | "error";

export function Compatibility() {
  const t = useT();
  const { lang } = useI18n();
  const [data, setData] = useState<DesktopCompatibilityData | null>(null);
  const [loadState, setLoadState] = useState<LoadState>("loading");
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);

  const refresh = async () => {
    try {
      const next = await invokeNative<DesktopCompatibilityData>("get_desktop_data");
      setData(next);
      setLoadState("native");
      setUpdatedAt(new Date());
    } catch (error) {
      setData(null);
      setLoadState(error instanceof Error && error.message === "native_unavailable" ? "preview" : "error");
      setUpdatedAt(new Date());
    }
  };

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5_000);
    return () => window.clearInterval(timer);
  }, []);

  const evidenceState = useMemo(
    () => classifyEvidence({
      payload: data,
      status: loadState === "native" ? "native" : loadState,
      requireRuntimeIdentity: true,
      requireTransactionProvenance: true,
    }),
    [data, loadState],
  );
  const verifiedData = evidenceState === "verified" ? data : null;
  const runtimeCompatibility = verifiedData?.runtimeCompatibility;
  const hasRuntimeAgentCatalog = Array.isArray(runtimeCompatibility?.agents);
  const agents = hasRuntimeAgentCatalog
    ? (runtimeCompatibility?.agents ?? []).map(toCompatibilityAgent)
    : Array.isArray(verifiedData?.agents)
      ? verifiedData.agents
      : [];
  const protocolProfiles = Array.isArray(runtimeCompatibility?.protocolInjection)
    ? runtimeCompatibility.protocolInjection
    : [];
  const routes = Array.isArray(verifiedData?.routes) ? verifiedData.routes : [];
  const projects = Array.isArray(verifiedData?.projects) ? verifiedData.projects : [];
  const chainStatus = isRecord(runtimeCompatibility?.chainStatus) ? runtimeCompatibility.chainStatus : null;
  const summary = useMemo(() => summarizeAgents(agents), [agents]);
  const sourceText = evidenceLabel(evidenceState, lang);
  const hasVerifiedPayload = evidenceState === "verified";

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex flex-wrap items-center gap-3">
            <h1>{t("cp.title")}</h1>
            <DataSourceBadge
              mode={hasVerifiedPayload ? "native" : evidenceState === "preview" || evidenceState === "loading" ? "preview" : "unavailable"}
            />
            <Badge variant={hasVerifiedPayload ? "default" : "secondary"}>
              {sourceText}{updatedAt ? ` · ${updatedAt.toLocaleTimeString()}` : ""}
            </Badge>
          </div>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
            {lang === "zh"
              ? "区分产品实现能力目录与当前机器的真实配置状态；目录能力不等于本机已验证。"
              : "Separates the implementation catalog from configuration verified on this machine."}
          </p>
        </div>
        <Button variant="outline" onClick={refresh}>
          <RefreshCw className="size-4" />
          {lang === "zh" ? "刷新" : "Refresh"}
        </Button>
      </div>

      {!hasVerifiedPayload && (
        <GlassCard className="p-4" hover={false}>
          <EmptyState
            title={emptyTitle(evidenceState, lang)}
            detail={lang === "zh"
              ? "未取得通过来源、完整性与事务证明校验的本机数据；不会使用预览内容补位。"
              : "No local payload passed provenance, completeness, and transaction verification."}
          />
        </GlassCard>
      )}

      <div className="grid gap-4 md:grid-cols-4" data-testid="compatibility-summary">
        <SummaryCard
          icon={<Monitor className="size-4" />}
          label={lang === "zh" ? "目录 Agent" : "Catalog agents"}
          value={hasVerifiedPayload ? String(agents.length) : "—"}
        />
        <SummaryCard
          icon={<ShieldCheck className="size-4" />}
          label={lang === "zh" ? "完整支持项" : "Full support entries"}
          value={hasVerifiedPayload ? String(summary.ok) : "—"}
          tone="ok"
        />
        <SummaryCard
          icon={<TerminalSquare className="size-4" />}
          label={lang === "zh" ? "部分 / 计划项" : "Partial / planned entries"}
          value={hasVerifiedPayload ? String(summary.partial) : "—"}
          tone="warn"
        />
        <SummaryCard
          icon={<Route className="size-4" />}
          label={lang === "zh" ? "本机路由" : "Local routes"}
          value={hasVerifiedPayload ? String(routes.length) : "—"}
        />
      </div>

      <GlassCard className="p-5" hover={false}>
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <h3>{lang === "zh" ? "Agent 实现能力目录" : "Agent implementation catalog"}</h3>
          <CatalogEvidenceBadge lang={lang} />
        </div>
        {agents.length > 0 ? (
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
                      {agent.headroomMethod && <div className="text-xs text-muted-foreground">{agent.headroomMethod}</div>}
                    </td>
                    <td className="py-3"><MatrixStatus value={agent.rtk} detail={agent.rtkMethod} lang={lang} /></td>
                    <td className="py-3"><MatrixStatus value={agent.headroom} detail={agent.headroomMethod} lang={lang} /></td>
                    <td className="py-3"><MatrixStatus value={agent.macos} lang={lang} /></td>
                    <td className="py-3"><MatrixStatus value={agent.win} lang={lang} /></td>
                    <td className="py-3"><MatrixStatus value={agent.wsl} lang={lang} /></td>
                    <td className="py-3 text-xs text-muted-foreground">{agent.note || "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState
            title={lang === "zh" ? "暂无 Agent 能力目录" : "No agent capability catalog"}
            detail={lang === "zh" ? "本地服务没有返回静态 Agent 能力条目。" : "The local service returned no static agent capability rows."}
          />
        )}
      </GlassCard>

      <div className="grid gap-4 lg:grid-cols-2">
        <GlassCard className="p-5" hover={false}>
          <h3 className="mb-3 flex items-center gap-2">
            <Route className="size-4 text-primary" />
            {lang === "zh" ? "本机路由配置" : "Local route configuration"}
          </h3>
          {routes.length > 0 ? (
            <div className="space-y-2">
              {routes.map((route, index) => <RouteRow key={safeText(route.id) ?? safeText(route.name) ?? String(index)} route={route} lang={lang} />)}
            </div>
          ) : (
            <EmptyState
              title={lang === "zh" ? "暂无本机路由" : "No local routes"}
              detail={lang === "zh" ? "当前机器没有返回已配置路由。" : "No configured routes were returned for this machine."}
            />
          )}
        </GlassCard>

        <GlassCard className="p-5" hover={false}>
          <h3 className="mb-3 flex items-center gap-2">
            <Laptop className="size-4 text-primary" />
            {lang === "zh" ? "本机项目策略" : "Local project policies"}
          </h3>
          {projects.length > 0 ? (
            <div className="space-y-2">
              {projects.map((project, index) => <ProjectRow key={`${project.root ?? "project"}-${index}`} project={project} lang={lang} />)}
            </div>
          ) : (
            <EmptyState
              title={lang === "zh" ? "暂无本机项目策略" : "No local project policies"}
              detail={lang === "zh" ? "当前机器没有返回项目级策略。" : "No project-level policy was returned for this machine."}
            />
          )}
        </GlassCard>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <GlassCard className="p-5" hover={false}>
          <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
            <h3 className="flex items-center gap-2">
              <ShieldCheck className="size-4 text-primary" />
              {lang === "zh" ? "协议实现能力目录" : "Protocol implementation catalog"}
            </h3>
            <CatalogEvidenceBadge lang={lang} />
          </div>
          {protocolProfiles.length > 0 ? (
            <div className="space-y-2">
              {protocolProfiles.map((profile, index) => <ProtocolRow key={safeText(profile.protocol) ?? String(index)} profile={profile} lang={lang} />)}
            </div>
          ) : (
            <EmptyState
              title={lang === "zh" ? "暂无协议能力目录" : "No protocol capability catalog"}
              detail={lang === "zh" ? "本地服务没有返回静态协议能力条目。" : "The local service returned no static protocol capability rows."}
            />
          )}
        </GlassCard>

        <GlassCard className="p-5" hover={false}>
          <h3 className="mb-3 flex items-center gap-2">
            <Route className="size-4 text-primary" />
            {lang === "zh" ? "本机链路状态" : "Local chain status"}
          </h3>
          {chainStatus ? (
            <ChainStatusCard status={chainStatus} lang={lang} />
          ) : (
            <EmptyState
              title={lang === "zh" ? "暂无本机链路状态" : "No local chain status"}
              detail={lang === "zh" ? "当前机器没有返回可核验的链路状态。" : "No verifiable chain status was returned for this machine."}
            />
          )}
        </GlassCard>
      </div>
    </div>
  );
}

function CatalogEvidenceBadge({ lang }: { lang: "zh" | "en" }) {
  return (
    <Badge
      variant="outline"
      data-source={compatibilityCatalogEvidence.source}
      data-evidence-kind={compatibilityCatalogEvidence.evidence_kind}
      data-verified-on-this-machine={String(compatibilityCatalogEvidence.verified_on_this_machine)}
    >
      {lang === "zh" ? "实现能力目录 · 非本机验证" : "Implementation catalog · not verified locally"}
    </Badge>
  );
}

function SummaryCard({ icon, label, value, tone }: { icon: React.ReactNode; label: string; value: string; tone?: "ok" | "warn" }) {
  const color = tone === "ok" ? "text-emerald-500" : tone === "warn" ? "text-amber-500" : "text-primary";
  return (
    <GlassCard className="p-4" hover={false}>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">{icon}{label}</div>
      <div className={`mt-2 text-2xl font-semibold ${color}`}>{value}</div>
    </GlassCard>
  );
}

function MatrixStatus({ value, detail, lang }: { value?: string; detail?: string; lang: "zh" | "en" }) {
  if (!value) {
    return <span className="text-xs text-muted-foreground">{lang === "zh" ? "未说明" : "Not specified"}</span>;
  }
  const normalized = value.toLowerCase();
  const status = normalized === "ok" || normalized === "full"
    ? "healthy"
    : normalized === "partial" || normalized === "planned"
      ? "warning"
      : "error";
  return (
    <div className="space-y-1">
      <StatusBadge status={status} label={value} />
      {detail ? <div className="max-w-[180px] text-xs text-muted-foreground">{detail}</div> : null}
    </div>
  );
}

function RouteRow({ route, lang }: { route: Record<string, any>; lang: "zh" | "en" }) {
  const title = safeText(route.name) ?? safeText(route.id) ?? (lang === "zh" ? "未命名路由" : "Unnamed route");
  const details = [safeText(route.protocol), safeText(route.chain), safeText(route.policy)].filter(Boolean).join(" · ");
  const provider = safeText(route.target_provider_id) ?? safeText(route.provider_id) ?? safeText(route.provider);
  return (
    <div className="glass-inset flex flex-wrap items-center justify-between gap-3 p-3 text-sm">
      <div>
        <div>{title}</div>
        {details && <div className="text-xs text-muted-foreground">{details}</div>}
      </div>
      {provider && <Badge variant="outline">{provider}</Badge>}
    </div>
  );
}

function ProjectRow({ project, lang }: { project: ProjectPolicyRow; lang: "zh" | "en" }) {
  const details = [project.stack, project.agents?.join(", "), project.route].filter(Boolean).join(" · ");
  return (
    <div className="glass-inset p-3 text-sm">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="font-medium">{project.root || (lang === "zh" ? "未命名项目" : "Unnamed project")}</span>
        {(project.level || project.policy) && <Badge variant="secondary">{project.level ?? project.policy}</Badge>}
      </div>
      {details && <div className="mt-1 text-xs text-muted-foreground">{details}</div>}
      {Array.isArray(project.features) && project.features.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {project.features.map((feature) => <Badge key={feature} variant="outline" className="text-xs">{feature}</Badge>)}
        </div>
      )}
    </div>
  );
}

function ProtocolRow({ profile, lang }: { profile: RuntimeProtocolProfile; lang: "zh" | "en" }) {
  return (
    <div className="glass-inset p-3 text-sm">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="font-medium">{safeText(profile.protocol) ?? (lang === "zh" ? "未命名协议" : "Unnamed protocol")}</span>
        <Badge variant="outline">{lang === "zh" ? "目录声明" : "Catalog declaration"}</Badge>
      </div>
      {safeText(profile.injection_method) && <div className="mt-1 text-xs text-muted-foreground">{profile.injection_method}</div>}
      {Array.isArray(profile.compatible_clients) && profile.compatible_clients.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {profile.compatible_clients.slice(0, 5).map((client) => <Badge key={client} variant="secondary" className="text-xs">{client}</Badge>)}
        </div>
      )}
    </div>
  );
}

function ChainStatusCard({ status, lang }: { status: Record<string, any>; lang: "zh" | "en" }) {
  const mode = safeText(status.mode);
  const state = safeText(status.status);
  const privacy = safeText(status.privacy);
  const routes = Array.isArray(status.routes) ? status.routes.filter(isRecord) : [];
  return (
    <div className="glass-inset p-3 text-sm">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="font-medium">{mode ?? (lang === "zh" ? "未提供链路模式" : "Chain mode not provided")}</span>
        {state && <Badge variant={state === "configured" ? "default" : "secondary"}>{state}</Badge>}
      </div>
      {privacy && <div className="mt-1 text-xs text-muted-foreground">{privacy}</div>}
      {routes.length > 0 ? (
        <div className="mt-3 space-y-2">
          {routes.map((route, index) => (
            <div key={safeText(route.route_id) ?? String(index)} className="flex flex-wrap items-center justify-between gap-2 text-xs">
              <span>{safeText(route.name) ?? safeText(route.route_id) ?? (lang === "zh" ? "未命名路由" : "Unnamed route")}</span>
              {safeText(route.fallback) && <Badge variant="outline">{route.fallback}</Badge>}
            </div>
          ))}
        </div>
      ) : (
        <div className="mt-3 text-xs text-muted-foreground">{lang === "zh" ? "没有链路路由明细。" : "No chain route details."}</div>
      )}
    </div>
  );
}

function EmptyState({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex min-h-28 flex-col items-center justify-center rounded-xl border border-dashed border-border/60 bg-muted/15 px-4 text-center">
      <div className="text-sm font-medium">{title}</div>
      <div className="mt-1 text-xs text-muted-foreground">{detail}</div>
    </div>
  );
}

function summarizeAgents(agents: CompatibilityAgent[]) {
  return agents.reduce(
    (summary, agent) => {
      for (const value of [agent.rtk, agent.headroom, agent.macos, agent.win, agent.wsl]) {
        const normalized = String(value ?? "").toLowerCase();
        if (normalized === "ok" || normalized === "full") summary.ok += 1;
        else if (normalized === "partial" || normalized === "planned") summary.partial += 1;
        else if (normalized) summary.fail += 1;
      }
      return summary;
    },
    { ok: 0, partial: 0, fail: 0 },
  );
}

function toCompatibilityAgent(profile: RuntimeAgentProfile): CompatibilityAgent {
  return {
    name: safeText(profile.agent) ?? "Agent",
    rtk: supportToStatus(profile.rtk_supported, profile.rtk_transparent_rewrite),
    rtkMethod: safeText(profile.rtk_method) ?? undefined,
    headroom: supportToStatus(profile.headroom_supported),
    headroomMethod: safeText(profile.headroom_method) ?? undefined,
    macos: supportToStatus(undefined, profile.macos_support),
    win: supportToStatus(undefined, profile.windows_native_support),
    wsl: supportToStatus(undefined, profile.windows_wsl_support),
    note: safeText(profile.notes) ?? undefined,
  };
}

function supportToStatus(supported?: boolean, level?: string) {
  const normalized = String(level ?? "").toLowerCase();
  if (normalized === "full") return "ok";
  if (normalized === "partial") return "partial";
  if (normalized === "none") return "none";
  if (supported === true) return "ok";
  if (supported === false) return "planned";
  return undefined;
}

function evidenceLabel(state: EvidenceState, lang: "zh" | "en") {
  if (state === "verified") return lang === "zh" ? "本机实时数据" : "Live local data";
  if (state === "preview") return lang === "zh" ? "浏览器预览 · 无本机数据" : "Browser preview · no local data";
  if (state === "loading") return lang === "zh" ? "正在读取本机数据" : "Loading local data";
  if (state === "stale") return lang === "zh" ? "本机数据已过期" : "Local data is stale";
  if (state === "partial") return lang === "zh" ? "本机数据不完整" : "Local data is incomplete";
  return lang === "zh" ? "本机数据暂不可用" : "Local data unavailable";
}

function emptyTitle(state: EvidenceState, lang: "zh" | "en") {
  if (state === "preview") return lang === "zh" ? "浏览器预览不提供本机兼容性证据" : "Browser preview has no local compatibility evidence";
  if (state === "loading") return lang === "zh" ? "正在读取本机兼容性证据" : "Loading local compatibility evidence";
  if (state === "stale") return lang === "zh" ? "本机兼容性证据已过期" : "Local compatibility evidence is stale";
  if (state === "partial") return lang === "zh" ? "本机兼容性证据不完整" : "Local compatibility evidence is incomplete";
  return lang === "zh" ? "本机兼容性证据暂不可用" : "Local compatibility evidence unavailable";
}

function safeText(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function isRecord(value: unknown): value is Record<string, any> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
