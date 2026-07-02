import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  Layers,
  DollarSign,
  ShieldCheck,
  Server,
  Route as RouteIcon,
  Boxes,
  FolderGit2,
  Stethoscope,
  GitCompareArrows,
  Archive,
  PlugZap,
  Terminal,
  Gauge,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  ChevronRight,
  Sparkles,
  Info,
  Bot,
  Cpu,
  ScrollText,
  Sun,
  Moon,
} from "lucide-react";
import {
  Card,
  CardHead,
  SectionTitle,
  StatCard,
  StateBadge,
  Badge,
  Progress,
  Sparkline,
  Tabs,
  KV,
} from "./ui";
import { useI18n, useTheme } from "./context";
import * as fx from "./fixtures";
import { formatTokens } from "./fixtures";

const stateIcon = (s: "ok" | "warn" | "err", size = 16) =>
  s === "ok" ? (
    <CheckCircle2 size={size} style={{ color: "var(--success)" }} />
  ) : s === "warn" ? (
    <AlertTriangle size={size} style={{ color: "var(--warning)" }} />
  ) : (
    <XCircle size={size} style={{ color: "var(--danger)" }} />
  );

const supportTone = (s: string) =>
  s === "ok" ? "success" : s === "partial" ? "warning" : s === "no" ? "danger" : "neutral";

type ToolCheck = {
  id: string;
  label: string;
  installed: boolean;
  version?: string | null;
  path?: string | null;
  status: string;
  detail: string;
};

type ActionLog = {
  time: string;
  level: "info" | "warn" | "error" | string;
  message: string;
};

type ActionResult = {
  ok: boolean;
  summary: string;
  logs: ActionLog[];
  tools: ToolCheck[];
};

type InstallPlan = {
  target: string;
  mode: string;
  installRoot: string;
  targets: string[];
  writesEnabled: boolean;
  opencodeTestPort: number;
  testScope: string;
  excludedAgents: string[];
  transaction: { id: string; label: string; detail: string }[];
  diff: { type: "ctx" | "add" | "del"; text: string }[];
  warnings: string[];
};

type HeadroomEndpoint = {
  port: number;
  label: string;
  healthy: boolean;
  detail: string;
};

type DesktopData = {
  tools: ToolCheck[];
  health: typeof fx.health;
  doctorChecks: typeof fx.doctorChecks;
  agents: typeof fx.agents;
  projects: typeof fx.projects;
  savings: typeof fx.savings;
  rtkTrend: number[];
  headroomTrend: number[];
  commandFamilies: typeof fx.commandFamilies;
  recentDecisions: typeof fx.recentDecisions;
  recentEvents: typeof fx.recentEvents;
  routes: typeof fx.routes;
  providers: typeof fx.providers;
  logs: typeof fx.logs;
  changes: typeof fx.changes;
  backupPoints: typeof fx.backupPoints;
  headroomEndpoints: HeadroomEndpoint[];
};

const defaultDesktopData: DesktopData = {
  tools: [
    { id: "rtk", label: "RTK", installed: false, status: "unknown", detail: "等待检测" },
    { id: "headroom", label: "Headroom", installed: false, status: "unknown", detail: "等待检测" },
    { id: "opencode", label: "OpenCode", installed: false, status: "unknown", detail: "等待检测" },
  ],
  health: fx.health,
  doctorChecks: fx.doctorChecks,
  agents: fx.agents,
  projects: fx.projects,
  savings: fx.savings,
  rtkTrend: fx.rtkTrend,
  headroomTrend: fx.headroomTrend,
  commandFamilies: fx.commandFamilies,
  recentDecisions: fx.recentDecisions,
  recentEvents: fx.recentEvents,
  routes: fx.routes,
  providers: fx.providers,
  logs: fx.logs,
  changes: fx.changes,
  backupPoints: fx.backupPoints,
  headroomEndpoints: [],
};

const initialTools: ToolCheck[] = [
  { id: "rtk", label: "RTK", installed: false, status: "unknown", detail: "等待检测" },
  { id: "headroom", label: "Headroom", installed: false, status: "unknown", detail: "等待检测" },
  { id: "opencode", label: "OpenCode", installed: false, status: "unknown", detail: "等待检测" },
];

const initialLogs: ActionLog[] = [
  { time: "--:--:--", level: "info", message: "安装日志会显示每一步检测、安装、验证结果。" },
];

async function runDesktopAction(command: string, args?: Record<string, unknown>): Promise<ActionResult> {
  try {
    return await invoke<ActionResult>(command, args);
  } catch (error) {
    return {
      ok: false,
      summary: "桌面后端命令调用失败",
      logs: [
        {
          time: new Date().toLocaleTimeString(),
          level: "error",
          message: error instanceof Error ? error.message : String(error),
        },
      ],
      tools: initialTools,
    };
  }
}

function logClass(level: string) {
  return level === "error" ? "err" : level === "warn" ? "warn" : "info";
}

function useDesktopData() {
  const [data, setData] = useState<DesktopData>(defaultDesktopData);

  useEffect(() => {
    invoke<Partial<DesktopData>>("get_desktop_data")
      .then((next) => setData((prev) => ({ ...prev, ...next })))
      .catch(() => {
        // Keep the development fallback visible when the desktop backend is unavailable.
      });
  }, []);

  return data;
}

function SafetyBanner() {
  const { t } = useI18n();
  return (
    <div className="banner safe">
      <ShieldCheck className="banner-ico" size={22} />
      <div className="banner-txt">
        <strong>{t("banner.noTouch.title")}</strong>
        <span>{t("banner.noTouch.desc")}</span>
      </div>
      <div className="right">
        <Badge tone="success" dot pulse>
          No-Touch
        </Badge>
      </div>
    </div>
  );
}

/* ============================= Overview ============================= */
export function Overview({ go }: { go: (id: string) => void }) {
  const { t } = useI18n();
  const data = useDesktopData();
  const coveredAgents = data.agents.filter((a) => a.rtk === "ok" || a.headroom === "ok");
  const coveragePct = data.agents.length > 0 ? Math.round((coveredAgents.length / data.agents.length) * 100) : 0;
  return (
    <div className="page">
      <SafetyBanner />

      <div className="grid cols-4 stagger">
        <StatCard
          icon={<Layers size={20} />}
          label={t("ov.totalSaved")}
          value={data.savings.totalTokens}
          format={formatTokens}
          delta={data.savings.deltaTotal}
          deltaLabel={t("ov.thisWeek")}
          accent="#6366f1"
        />
        <StatCard
          icon={<Terminal size={20} />}
          label={t("ov.rtkSaved")}
          value={data.savings.rtkTokens}
          format={formatTokens}
          delta={data.savings.deltaRtk}
          deltaLabel={t("ov.thisWeek")}
          accent="#22c55e"
        />
        <StatCard
          icon={<Gauge size={20} />}
          label={t("ov.headroomSaved")}
          value={data.savings.headroomTokens}
          format={formatTokens}
          delta={data.savings.deltaHeadroom}
          deltaLabel={t("ov.thisWeek")}
          accent="#0ea5e9"
        />
        <StatCard
          icon={<DollarSign size={20} />}
          label={t("ov.costSaved")}
          value={data.savings.costUsd}
          format={(n) => "$" + n.toFixed(1)}
          delta={data.savings.deltaCost}
          deltaLabel={t("ov.thisWeek")}
          accent="#f59e0b"
        />
      </div>

      <div className="grid cols-3 mt-16">
        <Card className="stagger" style={{ gridColumn: "span 1" }}>
          <CardHead icon={<Activity size={18} />} title={t("ov.health")} />
          <div className="list">
            {data.health.map((h) => (
              <div className="list-row" key={h.key}>
                {stateIcon(h.state)}
                <span className="row-title">{h.label}</span>
                <span className="spacer" />
                <StateBadge
                  state={h.state}
                  label={
                    h.state === "ok"
                      ? t("common.healthy")
                      : h.state === "warn"
                      ? t("common.warning")
                      : t("common.error")
                  }
                />
              </div>
            ))}
          </div>
          <div className="kv mt-16" style={{ borderTop: "1px solid var(--border)", paddingTop: 12 }}>
            <span className="k">{t("ov.activeProject")}</span>
            <span className="v mono">~/work/d-token</span>
          </div>
          <div className="kv">
            <span className="k">{t("ov.activeAgent")}</span>
            <span className="v">Claude Code</span>
          </div>
        </Card>

        <Card style={{ gridColumn: "span 2" }}>
          <CardHead
            icon={<RouteIcon size={18} />}
            title={t("ov.recentRoutes")}
            action={
              <button className="btn ghost sm" onClick={() => go("logs")}>
                {t("common.viewAll")} <ChevronRight size={14} />
              </button>
            }
          />
          <div className="table-wrap">
            <table className="tbl">
              <thead>
                <tr>
                  <th>Request</th>
                  <th>Agent</th>
                  <th>{t("route.id")}</th>
                  <th>{t("common.provider")}</th>
                  <th>Saved</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {data.recentDecisions.map((d) => (
                  <tr key={d.id}>
                    <td className="mono">{d.id}</td>
                    <td className="name">{d.agent}</td>
                    <td>{d.route}</td>
                    <td>{d.provider}</td>
                    <td>
                      <Badge tone="success">{d.saved}</Badge>
                    </td>
                    <td className="muted">{d.when}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>
      </div>

      <div className="grid cols-2 mt-16">
        <Card>
          <CardHead
            icon={<Terminal size={18} />}
            title={t("ov.rtkSummary")}
            action={
              <button className="btn ghost sm" onClick={() => go("rtkDash")}>
                {t("ov.jumpRtk")} <ChevronRight size={14} />
              </button>
            }
          />
          <Sparkline data={data.rtkTrend} color="#22c55e" width={320} height={56} />
          <div className="kv mt-16">
            <span className="k">Saved (7d)</span>
            <span className="v">{formatTokens(data.savings.rtkTokens)}</span>
          </div>
          <div className="kv">
            <span className="k">Top family</span>
            <span className="v">search · 72%</span>
          </div>
        </Card>

        <Card>
          <CardHead
            icon={<Gauge size={18} />}
            title={t("ov.headroomSummary")}
            action={
              <button className="btn ghost sm" onClick={() => go("headroomDash")}>
                {t("ov.jumpHeadroom")} <ChevronRight size={14} />
              </button>
            }
          />
          <Sparkline data={data.headroomTrend} color="#0ea5e9" width={320} height={56} />
          <div className="kv mt-16">
            <span className="k">Saved (7d)</span>
            <span className="v">{formatTokens(data.savings.headroomTokens)}</span>
          </div>
          <div className="kv">
            <span className="k">{t("common.level")}</span>
            <span className="v">L2 · Balanced</span>
          </div>
        </Card>
      </div>

      <div className="grid cols-2 mt-16">
        <Card>
          <CardHead icon={<AlertTriangle size={18} />} title={t("ov.recentErrors")} />
          <div className="list">
            {data.recentEvents.map((e, i) => (
              <div className="list-row" key={i}>
                {stateIcon(e.level === "info" ? "ok" : e.level === "warn" ? "warn" : "err")}
                <div>
                  <div className="row-title" style={{ fontWeight: 500 }}>
                    {e.msg}
                  </div>
                </div>
                <span className="spacer" />
                <span className="muted mono">{e.time}</span>
              </div>
            ))}
          </div>
        </Card>

        <Card>
          <CardHead icon={<Bot size={18} />} title={t("ov.agentCoverage")} />
          <div className="row" style={{ alignItems: "baseline" }}>
            <div style={{ fontSize: 34, fontWeight: 800 }}>
              {coveredAgents.length}
            </div>
            <div className="muted">/ {data.agents.length} {t("ov.agentsCovered")}</div>
          </div>
          <div className="mt-16">
            <Progress value={coveragePct} tone="success" />
          </div>
          <div className="wrap mt-16">
            {data.agents.slice(0, 8).map((a) => (
              <Badge key={a.name} tone={a.rtk === "ok" ? "success" : a.rtk === "partial" ? "warning" : "neutral"}>
                {a.name}
              </Badge>
            ))}
            {data.agents.length > 8 && <Badge>+{data.agents.length - 8}</Badge>}
          </div>
        </Card>
      </div>
    </div>
  );
}

/* ===================== RTK Dashboard / RTK ===================== */
export function RtkDashboard() {
  const { t } = useI18n();
  const data = useDesktopData();
  const rtkTool = data.tools.find((tool) => tool.id === "rtk");
  return (
    <div className="page">
      <div className="grid cols-4 stagger">
        <StatCard icon={<Terminal size={20} />} label={t("ov.rtkSaved")} value={data.savings.rtkTokens} format={formatTokens} delta={data.savings.deltaRtk} deltaLabel={t("ov.thisWeek")} accent="#22c55e" />
        <StatCard icon={<Activity size={20} />} label="rtk gain (sessions)" value={128} accent="#6366f1" />
        <StatCard icon={<Sparkles size={20} />} label="rtk discover (missed)" value={9} accent="#f59e0b" />
        <StatCard icon={<Gauge size={20} />} label="Adoption" value={83} format={(n) => Math.round(n) + "%"} accent="#0ea5e9" />
      </div>

      <div className="grid cols-2 mt-16">
        <Card>
          <CardHead icon={<Activity size={18} />} title="Command-output savings" sub="7-day trend" />
          <Sparkline data={data.rtkTrend} color="#22c55e" width={360} height={70} />
        </Card>
        <Card>
          <CardHead icon={<Layers size={18} />} title="Savings by command family" />
          {data.commandFamilies.map((c) => (
            <div key={c.name} style={{ marginBottom: 12 }}>
              <div className="row" style={{ justifyContent: "space-between", marginBottom: 5 }}>
                <span className="soft" style={{ fontSize: 13 }}>{c.name}</span>
                <span style={{ fontWeight: 700, fontSize: 13 }}>{c.pct}%</span>
              </div>
              <Progress value={c.pct} tone="success" />
            </div>
          ))}
        </Card>
      </div>

      <div className="grid cols-3 mt-16">
        <Card>
          <CardHead icon={<PlugZap size={18} />} title="Integration" />
          <KV k={t("common.mode")} v={<Badge tone="info">{t("mode.fixture")}</Badge>} />
          <KV k={t("common.version")} v={<span className="mono">{rtkTool?.version ?? "not detected"}</span>} />
          <KV k="Hook integrity" v={<StateBadge state={rtkTool?.installed ? "ok" : "warn"} label={rtkTool?.installed ? t("common.ok") : t("common.warning")} />} />
          <KV k="Tee recovery" v={<StateBadge state="ok" label={t("common.enabled")} />} />
          <KV k="Telemetry" v={<Badge>opt-out</Badge>} />
        </Card>
        <Card>
          <CardHead icon={<ShieldCheck size={18} />} title="Permission precedence" />
          <div className="wrap">
            <Badge tone="danger">Deny</Badge>
            <ChevronRight size={14} className="muted" />
            <Badge tone="warning">Ask</Badge>
            <ChevronRight size={14} className="muted" />
            <Badge tone="success">Allow</Badge>
            <ChevronRight size={14} className="muted" />
            <Badge>Default</Badge>
          </div>
          <p className="muted mt-16" style={{ fontSize: 12.5, lineHeight: 1.6 }}>
            Deny &gt; Ask &gt; Allow &gt; Default. Hooks respect the strictest matching rule.
          </p>
        </Card>
        <Card>
          <CardHead icon={<Cpu size={18} />} title="Platform" />
          <KV k="macOS" v={<StateBadge state="ok" label={t("common.supported")} />} />
          <KV k="Windows native" v={<StateBadge state="warn" label={t("common.partial")} />} />
          <KV k="Windows WSL" v={<StateBadge state="ok" label={t("common.supported")} />} />
        </Card>
      </div>
    </div>
  );
}

export function RtkConfig() {
  const { t } = useI18n();
  const [tools, setTools] = useState<ToolCheck[]>(initialTools);
  const [logs, setLogs] = useState<ActionLog[]>(initialLogs);
  const [busy, setBusy] = useState<string | null>(null);
  const [rtkProfile, setRtkProfile] = useState("balanced");
  const [telemetryOptIn, setTelemetryOptIn] = useState(false);
  const [rawOutputTee, setRawOutputTee] = useState(true);
  const [hookIntegrity, setHookIntegrity] = useState(true);
  const [fallbackPreprocessor, setFallbackPreprocessor] = useState(true);
  const agentModes = [
    ["Claude Code", "Shell wrapper + AGENTS.md", "ok"],
    ["Codex CLI", "rtk prefix + local rules", "ok"],
    ["Cursor", "Manual instructions", "warn"],
    ["VS Code Agent", "Command filter profile", "warn"],
  ];
  const commandProfiles = [
    ["safe-default", t("rtk.profile.safe"), 74],
    ["build-test", t("rtk.profile.build"), 91],
    ["forensics", t("rtk.profile.forensics"), 46],
  ];
  const rtkTool = tools.find((tool) => tool.id === "rtk") ?? initialTools[0];
  const runRtkAction = async (label: string, command: string, args?: Record<string, unknown>) => {
    setBusy(label);
    const result = await runDesktopAction(command, args);
    setLogs((prev) => [...result.logs, ...prev].slice(0, 60));
    if (result.tools.length > 0) setTools(result.tools);
    setBusy(null);
  };
  const saveRtkSettings = () =>
    runRtkAction("保存 RTK 配置", "save_app_settings", {
      section: "rtk",
      payload: {
        profile: rtkProfile,
        telemetryOptIn,
        rawOutputTee,
        hookIntegrity,
        fallbackPreprocessor,
        permissionPrecedence: ["Deny", "Ask", "Allow", "Default"],
      },
    });
  const runRtkInstallTransaction = () =>
    runRtkAction("事务安装/验证 RTK", "run_install_transaction", {
      request: {
        target: "rtk",
        install_root: "C:\\Users\\10777\\.local\\bin",
        proxy_port: 8788,
        auto_backup: true,
        run_doctor_after: true,
        rollback_point: true,
        mode: "install",
      },
    });

  useEffect(() => {
    void runRtkAction("检测 RTK", "detect_tools");
  }, []);

  return (
    <div className="page">
      <div className="banner safe">
        <ShieldCheck className="banner-ico" size={20} />
        <div className="banner-txt">
          <strong>{t("rtkcfg.banner.title")}</strong>
          <span>{t("rtkcfg.banner.desc")}</span>
        </div>
        <div className="right">
          <Badge tone="info">{t("common.planOnly")}</Badge>
        </div>
      </div>

      <div className="grid cols-3 stagger">
        <Card>
          <CardHead icon={<Terminal size={18} />} title={t("rtkcfg.install")} />
          <KV k={t("common.status")} v={rtkTool.installed ? <StateBadge state="ok" label={t("rtkcfg.detected")} /> : <StateBadge state="warn" label="未检测到" />} />
          <KV k={t("common.version")} v={<span className="mono">{rtkTool.version ?? "unknown"}</span>} />
          <KV k={t("rtkcfg.source")} v={<Badge tone={rtkTool.installed ? "info" : "warning"}>{rtkTool.path ? "PATH" : "missing"}</Badge>} />
          <KV k={t("rtkcfg.owner")} v="d-token managed plan" />
        </Card>
        <Card>
          <CardHead icon={<ShieldCheck size={18} />} title={t("rtkcfg.integrity")} />
          <div className="toggle-list">
            <div className="toggle-row">
              <div><strong>Hook integrity</strong><span>检查 hook 是否被其它工具覆盖</span></div>
              <Toggle on={hookIntegrity} onClick={() => setHookIntegrity((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>Raw-output tee</strong><span>保留本地恢复文件，不进入日志</span></div>
              <Toggle on={rawOutputTee} onClick={() => setRawOutputTee((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>Fallback preprocessor</strong><span>RTK 不可用时启用基础清洗</span></div>
              <Toggle on={fallbackPreprocessor} onClick={() => setFallbackPreprocessor((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>Telemetry</strong><span>默认关闭，只允许显式开启</span></div>
              <Toggle on={telemetryOptIn} onClick={() => setTelemetryOptIn((v) => !v)} />
            </div>
          </div>
          <KV k="Recovery file" v={<span className="mono">%LOCALAPPDATA%\\d-token\\rtk</span>} />
          <KV k="Telemetry" v={<Badge>{telemetryOptIn ? "opt-in" : "opt-out"}</Badge>} />
        </Card>
        <Card>
          <CardHead icon={<Cpu size={18} />} title={t("rtkcfg.platform")} />
          <KV k="macOS" v={<StateBadge state="ok" label={t("common.supported")} />} />
          <KV k="Windows native" v={<StateBadge state="warn" label={t("common.partial")} />} />
          <KV k="Windows WSL" v={<StateBadge state="ok" label={t("common.supported")} />} />
          <KV k={t("rtkcfg.shell")} v={<span className="mono">PowerShell / bash</span>} />
        </Card>
      </div>

      <div className="grid cols-2 mt-16">
        <Card>
          <CardHead icon={<Bot size={18} />} title={t("rtkcfg.agentModes")} />
          <div className="list">
            {agentModes.map(([agent, method, state]) => (
              <div className="list-row" key={agent}>
                {stateIcon(state as "ok" | "warn" | "err")}
                <div>
                  <div className="row-title">{agent}</div>
                  <div className="row-sub">{method}</div>
                </div>
                <span className="spacer" />
                <Badge tone={state === "ok" ? "success" : "warning"}>
                  {state === "ok" ? t("common.enabled") : t("common.planned")}
                </Badge>
              </div>
            ))}
          </div>
        </Card>
        <Card>
          <CardHead icon={<ShieldCheck size={18} />} title={t("rtkcfg.permission")} />
          <div className="wrap">
            <Badge tone="danger">Deny</Badge>
            <ChevronRight size={14} className="muted" />
            <Badge tone="warning">Ask</Badge>
            <ChevronRight size={14} className="muted" />
            <Badge tone="success">Allow</Badge>
            <ChevronRight size={14} className="muted" />
            <Badge>Default</Badge>
          </div>
          <p className="muted mt-16" style={{ fontSize: 12.5, lineHeight: 1.6 }}>
            {t("rtkcfg.permissionDesc")}
          </p>
          <SectionTitle>{t("rtkcfg.filters")}</SectionTitle>
          <label className="field" style={{ marginBottom: 12 }}>
            <span>RTK profile</span>
            <select value={rtkProfile} onChange={(e) => setRtkProfile(e.target.value)}>
              <option value="safe">safe</option>
              <option value="balanced">balanced</option>
              <option value="build-test">build-test</option>
              <option value="forensics">forensics</option>
            </select>
          </label>
          {commandProfiles.map(([name, desc, pct]) => (
            <div key={name} style={{ marginBottom: 12 }}>
              <div className="row" style={{ justifyContent: "space-between", marginBottom: 5 }}>
                <span className="soft" style={{ fontSize: 13 }}>{name} · {desc}</span>
                <span style={{ fontWeight: 700, fontSize: 13 }}>{pct}%</span>
              </div>
              <Progress value={Number(pct)} tone={Number(pct) > 80 ? "success" : undefined} />
            </div>
          ))}
        </Card>
      </div>

      <Card className="mt-16">
        <CardHead
          icon={<GitCompareArrows size={18} />}
          title={t("rtkcfg.lifecycle")}
          action={
            <div className="row">
              <button className="btn sm" disabled={!!busy} onClick={() => runRtkAction("检测 RTK", "detect_tools")}><Stethoscope size={14} /> {t("setup.action.detect")}</button>
              <button className="btn sm" disabled={!!busy} onClick={() => runRtkAction("运行 Doctor", "run_doctor")}><GitCompareArrows size={14} /> Doctor</button>
              <button className="btn sm" disabled={!!busy} onClick={saveRtkSettings}><Archive size={14} /> 保存配置</button>
              <button className="btn primary sm" disabled={!!busy} onClick={runRtkInstallTransaction}><Archive size={14} /> 安装 RTK</button>
            </div>
          }
        />
        <div className="grid cols-3">
          <KV k={t("rtkcfg.installAction")} v={t("rtkcfg.installActionDesc")} />
          <KV k={t("rtkcfg.updateAction")} v={t("rtkcfg.updateActionDesc")} />
          <KV k={t("rtkcfg.removeAction")} v={t("rtkcfg.removeActionDesc")} />
        </div>
      </Card>

      <Card className="mt-16">
        <CardHead icon={<ScrollText size={18} />} title="RTK 动作日志" sub={busy ?? "最近的检测、安装、Doctor 结果"} />
        <div className="log install-log">
          {logs.map((entry, i) => (
            <div className="log-row" key={`${entry.time}-${i}`}>
              <span className="log-time">{entry.time}</span>
              <span className={`log-lvl ${logClass(entry.level)}`}>{entry.level.toUpperCase()}</span>
              <span>{entry.message}</span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/* ===================== Headroom Dashboard / Headroom ===================== */
export function HeadroomDashboard() {
  const { t } = useI18n();
  const data = useDesktopData();
  const features = [
    "CCR", "memory", "learn", "output shaper",
    "cache aligner", "code compressor", "JSON SmartCrusher", "text Kompress",
  ];
  return (
    <div className="page">
      <div className="grid cols-4 stagger">
        <StatCard icon={<Gauge size={20} />} label={t("ov.headroomSaved")} value={data.savings.headroomTokens} format={formatTokens} delta={data.savings.deltaHeadroom} deltaLabel={t("ov.thisWeek")} accent="#0ea5e9" />
        <StatCard icon={<DollarSign size={20} />} label={t("ov.costSaved")} value={112.3} format={(n) => "$" + n.toFixed(1)} accent="#f59e0b" />
        <StatCard icon={<Layers size={20} />} label={t("common.level")} value={2} format={() => "L2"} accent="#6366f1" />
        <StatCard icon={<Activity size={20} />} label="Fallback events" value={3} accent="#ef4444" />
      </div>

      <div className="grid cols-2 mt-16">
        <Card>
          <CardHead icon={<Activity size={18} />} title="Context compression savings" sub="7-day trend" />
          <Sparkline data={data.headroomTrend} color="#0ea5e9" width={360} height={70} />
        </Card>
        <Card>
          <CardHead icon={<Server size={18} />} title="Proxy / SDK / MCP" />
          <KV k={t("common.mode")} v={<Badge tone="warning">{t("mode.external")}</Badge>} />
          <KV
            k="Proxy"
            v={
              <StateBadge
                state={data.headroomEndpoints.length > 0 && data.headroomEndpoints.every((e) => e.healthy) ? "ok" : "warn"}
                label={data.headroomEndpoints.length > 0 && data.headroomEndpoints.every((e) => e.healthy) ? "healthy" : "check"}
              />
            }
          />
          <KV k="SDK" v={<StateBadge state="ok" label={t("common.ok")} />} />
          <KV k="MCP" v={<StateBadge state="ok" label={t("common.ok")} />} />
          <KV k="Recommended" v={<Badge tone="info">L3 · Aggressive</Badge>} />
        </Card>
      </div>

      <Card className="mt-16">
        <CardHead icon={<Boxes size={18} />} title="Feature status" />
        {data.headroomEndpoints.length > 0 && (
          <div className="grid cols-2" style={{ marginBottom: 12 }}>
            {data.headroomEndpoints.map((endpoint) => (
              <div className="list-row" key={endpoint.port} style={{ borderBottom: "none" }}>
                {stateIcon(endpoint.healthy ? "ok" : "warn")}
                <div>
                  <div className="row-title">{endpoint.port} · {endpoint.label}</div>
                  <div className="row-sub">{endpoint.detail}</div>
                </div>
              </div>
            ))}
          </div>
        )}
        <div className="grid cols-4">
          {features.map((f, i) => (
            <div className="list-row" key={f} style={{ borderBottom: "none" }}>
              {stateIcon(i % 4 === 3 ? "warn" : "ok")}
              <span className="row-title" style={{ fontWeight: 500, fontSize: 13 }}>{f}</span>
            </div>
          ))}
        </div>
        <p className="muted mt-16" style={{ fontSize: 12.5 }}>
          CCR raw content is never read by default — only summaries and counts are shown.
        </p>
      </Card>
    </div>
  );
}

export function HeadroomConfig() {
  const { t } = useI18n();
  const data = useDesktopData();
  const [tools, setTools] = useState<ToolCheck[]>(initialTools);
  const [logs, setLogs] = useState<ActionLog[]>(initialLogs);
  const [busy, setBusy] = useState<string | null>(null);
  const [proxyPort, setProxyPort] = useState("8787");
  const [enableCcr, setEnableCcr] = useState(true);
  const [enableMemory, setEnableMemory] = useState(true);
  const [enableLearn, setEnableLearn] = useState(true);
  const [enableMcp, setEnableMcp] = useState(true);
  const [enableOutputShaper, setEnableOutputShaper] = useState(true);
  const [enableCacheAligner, setEnableCacheAligner] = useState(true);
  const [enableCodeCompressor, setEnableCodeCompressor] = useState(true);
  const [enableJsonSmartcrusher, setEnableJsonSmartcrusher] = useState(true);
  const [enableTextKompress, setEnableTextKompress] = useState(true);
  const [compressionLevel, setCompressionLevel] = useState("balanced");
  const [outputVerbosity, setOutputVerbosity] = useState("auto");
  const features = [
    ["CCR", enableCcr ? "ok" : "warn"],
    ["memory", enableMemory ? "ok" : "warn"],
    ["learn", enableLearn ? "ok" : "warn"],
    ["output shaper", enableOutputShaper ? "ok" : "warn"],
    ["cache aligner", enableCacheAligner ? "ok" : "warn"],
    ["code compressor", enableCodeCompressor ? "ok" : "warn"],
    ["JSON SmartCrusher", enableJsonSmartcrusher ? "ok" : "warn"],
    ["text Kompress", enableTextKompress ? "ok" : "warn"],
  ];
  const headroomTool = tools.find((tool) => tool.id === "headroom") ?? initialTools[1];
  const selectedPort = Number.parseInt(proxyPort, 10) || 8787;
  const runHeadroomAction = async (label: string, command: string, args?: Record<string, unknown>) => {
    setBusy(label);
    const result = await runDesktopAction(command, args);
    setLogs((prev) => [...result.logs, ...prev].slice(0, 60));
    if (result.tools.length > 0) setTools(result.tools);
    setBusy(null);
  };
  const saveHeadroomPolicy = () =>
    runHeadroomAction("保存 Headroom 配置", "save_app_settings", {
      section: "headroom",
      payload: {
        proxyPort: selectedPort,
        compressionLevel,
        outputVerbosity,
        enableCcr,
        enableMemory,
        enableLearn,
        enableMcp,
        enableOutputShaper,
        enableCacheAligner,
        enableCodeCompressor,
        enableJsonSmartcrusher,
        enableTextKompress,
        testScope: selectedPort === 8788 ? "opencode-only" : "managed-headroom",
      },
    });
  const runHeadroomInstallTransaction = () =>
    runHeadroomAction("事务安装/验证 Headroom", "run_install_transaction", {
      request: {
        target: "headroom",
        install_root: "C:\\Users\\10777\\.local\\bin",
        proxy_port: selectedPort,
        auto_backup: true,
        run_doctor_after: true,
        rollback_point: true,
        mode: "install",
      },
    });

  useEffect(() => {
    void runHeadroomAction("检测 Headroom", "detect_tools");
  }, []);

  const policies = [
    ["D:\\D-TOKEN\\d-token", "L2 · Balanced", "RTK + Headroom"],
    ["D:\\scholar-ai", "L3 · Aggressive", "Headroom first"],
    ["Default", "L1 · Conservative", "SDK only"],
  ];
  return (
    <div className="page">
      <div className="banner safe">
        <ShieldCheck className="banner-ico" size={20} />
        <div className="banner-txt">
          <strong>{t("hcfg.banner.title")}</strong>
          <span>{t("hcfg.banner.desc")}</span>
        </div>
        <div className="right">
          <Badge tone="warning">{t("mode.external")}</Badge>
        </div>
      </div>

      <div className="grid cols-3 stagger">
        <Card>
          <CardHead icon={<Server size={18} />} title={t("hcfg.endpoints")} />
          <KV k="Proxy" v={<span className="mono">127.0.0.1:{proxyPort}</span>} />
          <KV k="SDK" v={<StateBadge state="ok" label={t("common.ok")} />} />
          <KV k="MCP" v={enableMcp ? <StateBadge state="ok" label={t("common.ok")} /> : <Badge>{t("common.disabled")}</Badge>} />
          <KV k={t("hcfg.mode")} v={<Badge tone="warning">{t("common.readOnly")}</Badge>} />
          {data.headroomEndpoints.map((endpoint) => (
            <KV
              key={endpoint.port}
              k={`${endpoint.port}`}
              v={<StateBadge state={endpoint.healthy ? "ok" : "warn"} label={endpoint.healthy ? "healthy" : "check"} />}
            />
          ))}
        </Card>
        <Card>
          <CardHead icon={<Gauge size={18} />} title={t("hcfg.policy")} />
          <label className="field" style={{ marginBottom: 10 }}>
            <span>{t("common.level")}</span>
            <select value={compressionLevel} onChange={(e) => setCompressionLevel(e.target.value)}>
              <option value="safe">safe</option>
              <option value="balanced">balanced</option>
              <option value="aggressive">aggressive</option>
              <option value="max">max</option>
            </select>
          </label>
          <label className="field" style={{ marginBottom: 10 }}>
            <span>Output verbosity</span>
            <select value={outputVerbosity} onChange={(e) => setOutputVerbosity(e.target.value)}>
              <option value="auto">auto</option>
              <option value="terse">terse</option>
              <option value="normal">normal</option>
              <option value="detailed">detailed</option>
            </select>
          </label>
          <KV k={t("hcfg.raw")} v={<Badge tone="success">{t("hcfg.rawOff")}</Badge>} />
          <KV k={t("hcfg.history")} v={<span className="mono">{headroomTool.version ?? "~/.headroom/stats"}</span>} />
          <KV k={t("hcfg.retention")} v="30 days" />
        </Card>
        <Card>
          <CardHead icon={<ShieldCheck size={18} />} title={t("hcfg.secrets")} />
          <KV k="API keys" v={<Badge tone="success">{t("hcfg.redacted")}</Badge>} />
          <KV k="Authorization" v={<Badge tone="success">{t("hcfg.redacted")}</Badge>} />
          <KV k="Cookies" v={<Badge tone="success">{t("hcfg.redacted")}</Badge>} />
          <KV k={t("hcfg.credentials")} v={t("hcfg.credentialsDesc")} />
        </Card>
      </div>

      <div className="grid cols-2 mt-16">
        <Card>
          <CardHead icon={<Boxes size={18} />} title={t("hcfg.features")} sub="关键功能可以直接开关，后续写入前进入 diff / backup" />
          <div className="toggle-list" style={{ marginBottom: 12 }}>
            <div className="toggle-row">
              <div><strong>CCR</strong><span>可逆压缩缓存摘要，默认不读原文</span></div>
              <Toggle on={enableCcr} onClick={() => setEnableCcr((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>Memory</strong><span>跨 Agent 记忆与项目上下文</span></div>
              <Toggle on={enableMemory} onClick={() => setEnableMemory((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>Learn</strong><span>从本地会话提取可复用模式</span></div>
              <Toggle on={enableLearn} onClick={() => setEnableLearn((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>MCP</strong><span>向支持的工具暴露 Headroom 能力</span></div>
              <Toggle on={enableMcp} onClick={() => setEnableMcp((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>Output shaper</strong><span>控制输出长度和结构</span></div>
              <Toggle on={enableOutputShaper} onClick={() => setEnableOutputShaper((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>Cache aligner</strong><span>减少重复上下文与缓存错位</span></div>
              <Toggle on={enableCacheAligner} onClick={() => setEnableCacheAligner((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>Code compressor</strong><span>对代码片段使用结构化压缩</span></div>
              <Toggle on={enableCodeCompressor} onClick={() => setEnableCodeCompressor((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>JSON SmartCrusher</strong><span>压缩大 JSON 与日志对象</span></div>
              <Toggle on={enableJsonSmartcrusher} onClick={() => setEnableJsonSmartcrusher((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>Text Kompress</strong><span>长文本压缩器</span></div>
              <Toggle on={enableTextKompress} onClick={() => setEnableTextKompress((v) => !v)} />
            </div>
          </div>
          <div className="grid cols-2">
            {features.map(([name, state]) => (
              <div className="list-row" key={name} style={{ borderBottom: "none" }}>
                {stateIcon(state as "ok" | "warn" | "err")}
                <span className="row-title">{name}</span>
                <span className="spacer" />
                <Badge tone={state === "ok" ? "success" : "warning"}>
                  {state === "ok" ? t("common.enabled") : t("common.planned")}
                </Badge>
              </div>
            ))}
          </div>
          <div className="row mt-16" style={{ justifyContent: "flex-end" }}>
            <button className="btn primary sm" disabled={!!busy} onClick={saveHeadroomPolicy}>
              <Archive size={14} /> 保存 Headroom 配置
            </button>
          </div>
        </Card>
        <Card>
          <CardHead icon={<FolderGit2 size={18} />} title={t("hcfg.projectMap")} />
          <label className="field" style={{ marginBottom: 12 }}>
            <span>Headroom 代理端口</span>
            <input value={proxyPort} onChange={(e) => setProxyPort(e.target.value)} />
          </label>
          <div className="table-wrap">
            <table className="tbl">
              <thead>
                <tr>
                  <th>{t("proj.root")}</th>
                  <th>{t("common.level")}</th>
                  <th>{t("common.method")}</th>
                </tr>
              </thead>
              <tbody>
                {policies.map(([root, level, method]) => (
                  <tr key={root}>
                    <td className="mono">{root}</td>
                    <td><Badge tone="info">{level}</Badge></td>
                    <td>{method}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>
      </div>

      <Card className="mt-16">
        <CardHead
          icon={<GitCompareArrows size={18} />}
          title={t("hcfg.lifecycle")}
          action={
            <div className="row">
              <button className="btn sm" disabled={!!busy} onClick={() => runHeadroomAction("检测 Headroom", "detect_tools")}><Stethoscope size={14} /> {t("setup.action.detect")}</button>
              <button className="btn sm" disabled={!!busy} onClick={() => runHeadroomAction("运行 Doctor", "run_doctor")}><GitCompareArrows size={14} /> Doctor</button>
              <button className="btn sm" disabled={!!busy} onClick={() => runHeadroomAction("检查 Headroom 端口", "manage_headroom", { action: "health", port: selectedPort })}><Activity size={14} /> Health</button>
              <button className="btn sm" disabled={!!busy} onClick={() => runHeadroomAction("启动 Headroom 端口", "manage_headroom", { action: "start", port: selectedPort })}><PlugZap size={14} /> Start</button>
              <button className="btn sm" disabled={!!busy} onClick={() => runHeadroomAction("重启 Headroom 端口", "manage_headroom", { action: "restart", port: selectedPort })}><Gauge size={14} /> Restart</button>
              <button className="btn ghost sm" disabled={!!busy} onClick={() => runHeadroomAction("停止 Headroom 端口", "manage_headroom", { action: "stop", port: selectedPort })}><XCircle size={14} /> Stop</button>
              <button className="btn primary sm" disabled={!!busy} onClick={runHeadroomInstallTransaction}><Archive size={14} /> 安装 Headroom</button>
            </div>
          }
        />
        <div className="grid cols-4">
          <KV k={t("hcfg.installAction")} v={t("hcfg.installActionDesc")} />
          <KV k={t("hcfg.startAction")} v={`${t("hcfg.startActionDesc")} · 127.0.0.1:${selectedPort}`} />
          <KV k={t("hcfg.restartAction")} v={`${t("hcfg.restartActionDesc")} · 127.0.0.1:${selectedPort}`} />
          <KV k={t("hcfg.stopAction")} v={`${t("hcfg.stopActionDesc")} · explicit only`} />
        </div>
      </Card>

      <Card className="mt-16">
        <CardHead icon={<ScrollText size={18} />} title="Headroom 动作日志" sub={busy ?? "最近的检测、安装、Doctor 结果"} />
        <div className="log install-log">
          {logs.map((entry, i) => (
            <div className="log-row" key={`${entry.time}-${i}`}>
              <span className="log-time">{entry.time}</span>
              <span className={`log-lvl ${logClass(entry.level)}`}>{entry.level.toUpperCase()}</span>
              <span>{entry.message}</span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/* ============================= Setup ============================= */
export function Setup() {
  const { t } = useI18n();
  const [sel, setSel] = useState("connect");
  const [tools, setTools] = useState<ToolCheck[]>(initialTools);
  const [logs, setLogs] = useState<ActionLog[]>(initialLogs);
  const [busy, setBusy] = useState<string | null>(null);
  const [autoBackup, setAutoBackup] = useState(true);
  const [runDoctorAfter, setRunDoctorAfter] = useState(true);
  const [rollbackPoint, setRollbackPoint] = useState(true);
  const [installRoot, setInstallRoot] = useState("C:\\Users\\10777\\.local\\bin");
  const [proxyPort, setProxyPort] = useState("8788");
  const [opencodeModel, setOpencodeModel] = useState("opencode/deepseek-v4-flash-free");
  const [summary, setSummary] = useState("等待检测本机环境");
  const [planTarget, setPlanTarget] = useState("all");
  const [installPlan, setInstallPlan] = useState<InstallPlan | null>(null);
  const modes = [
    { id: "connect", icon: <PlugZap size={22} />, title: t("setup.connect.title"), desc: t("setup.connect.desc") },
    { id: "install", icon: <Server size={22} />, title: t("setup.install.title"), desc: t("setup.install.desc") },
    { id: "repair", icon: <Stethoscope size={22} />, title: t("setup.repair.title"), desc: t("setup.repair.desc") },
    { id: "manual", icon: <Terminal size={22} />, title: t("setup.manual.title"), desc: t("setup.manual.desc") },
  ];
  const flow: Array<[string, string, boolean]> = [
    ["检测平台", "识别 Windows / PATH / 已安装工具", true],
    ["选择工具", "RTK、Headroom、d-token core 可以单独或全部安装", true],
    ["生成计划", "列出安装路径、端口、配置变更和风险", sel !== "connect"],
    ["展示 diff", "写入配置前必须能看到变更内容", sel !== "connect"],
    ["备份", autoBackup ? "开启：写入前保存原始配置" : "关闭：不建议关闭", autoBackup],
    ["应用", "只执行用户点击的安装动作", busy !== null],
    ["校验", "安装后读取版本、路径和服务状态", true],
    ["OpenCode 隔离测试", "测试端口只接管 OpenCode，其它 Agent 全部排除", true],
    ["Doctor", runDoctorAfter ? "开启：安装后自动诊断" : "关闭：可手动运行", runDoctorAfter],
    ["回滚点", rollbackPoint ? "开启：失败时可恢复" : "关闭：仅适合只读连接", rollbackPoint],
  ];

  const appendResult = (result: ActionResult) => {
    setSummary(result.summary);
    setLogs((prev) => [...result.logs, ...prev].slice(0, 80));
    if (result.tools.length > 0) setTools(result.tools);
  };

  const buildInstallRequest = (target = planTarget) => ({
    target,
    install_root: installRoot,
    proxy_port: Number.parseInt(proxyPort, 10) || 8788,
    auto_backup: autoBackup,
    run_doctor_after: runDoctorAfter,
    rollback_point: rollbackPoint,
    mode: sel,
  });

  const refreshInstallPlan = async (target = planTarget) => {
    setPlanTarget(target);
    try {
      const next = await invoke<InstallPlan>("preview_install_plan", { request: buildInstallRequest(target) });
      setInstallPlan(next);
    } catch (error) {
      setLogs((prev) => [
        {
          time: new Date().toLocaleTimeString(),
          level: "error",
          message: `安装计划生成失败：${error instanceof Error ? error.message : String(error)}`,
        },
        ...prev,
      ]);
    }
  };

  const runAction = async (label: string, command: string, args?: Record<string, unknown>) => {
    setBusy(label);
    setLogs((prev) => [
      {
        time: new Date().toLocaleTimeString(),
        level: "info",
        message: `开始执行：${label}`,
      },
      ...prev,
    ]);
    const result = await runDesktopAction(command, args);
    appendResult(result);
    setBusy(null);
  };

  const runInstallTransaction = async (target: string, label: string) => {
    await refreshInstallPlan(target);
    await runAction(label, "run_install_transaction", { request: buildInstallRequest(target) });
  };

  useEffect(() => {
    void runAction("自动检测本机工具", "detect_tools");
  }, []);

  useEffect(() => {
    void refreshInstallPlan(planTarget);
  }, [sel, installRoot, proxyPort, autoBackup, runDoctorAfter, rollbackPoint]);

  const toolState = (tool: ToolCheck) =>
    tool.installed ? (
      <StateBadge state="ok" label="已安装" />
    ) : tool.status === "unknown" ? (
      <Badge>待检测</Badge>
    ) : (
      <StateBadge state="warn" label="未安装" />
    );

  const selectedModeCopy: Record<string, string> = {
    connect: "只读连接已有环境，不主动写配置。",
    install: "托管安装会进入计划、diff、备份、应用、校验流程。",
    repair: "修复模式先跑 Doctor，再给出可回滚修复计划。",
    manual: "高级模式只生成配置片段，由用户决定何时应用。",
  };

  const modelOptions = [
    "opencode/deepseek-v4-flash-free",
    "opencode/mimo-v2.5-free",
    "opencode/nemotron-3-ultra-free",
    "opencode/north-mini-code-free",
    "opencode/big-pickle",
  ];
  const plannedSteps =
    installPlan?.transaction.map((step) => {
      const active =
        step.id === "backup"
          ? autoBackup
          : step.id === "doctor"
          ? runDoctorAfter
          : step.id === "rollback"
          ? rollbackPoint
          : step.id === "apply"
          ? sel === "install" || sel === "repair"
          : true;
      return [step.label, step.detail, active] as [string, string, boolean];
    }) ?? flow;
  const plannedDiff = installPlan?.diff ?? [];
  return (
    <div className="page">
      <div className="banner">
        <Info className="banner-ico" size={20} />
        <div className="banner-txt">
          <strong>{t("setup.banner.title")}</strong>
          <span>{t("setup.m3note")}</span>
        </div>
        <div className="right">
          <Badge tone={busy ? "warning" : "info"} dot={!!busy} pulse={!!busy}>
            {busy ? "执行中" : summary}
          </Badge>
        </div>
      </div>

      <SectionTitle>{t("setup.chooseMode")}</SectionTitle>
      <div className="grid cols-4 stagger">
        {modes.map((m) => (
          <button
            key={m.id}
            className={`mode-card ${sel === m.id ? "selected" : ""}`}
            onClick={() => setSel(m.id)}
          >
            <div className="mode-ico">{m.icon}</div>
            <h4>{m.title}</h4>
            <p>{m.desc}</p>
          </button>
        ))}
      </div>

      <div className="grid cols-2 mt-16">
        <Card>
          <CardHead
            icon={<RouteIcon size={18} />}
            title={t("setup.flowTitle")}
            sub={selectedModeCopy[sel]}
          />
          <div className="steps">
            {plannedSteps.map(([title, sub, active], i) => (
              <div className="step" key={title}>
                <div className="step-marker">{i + 1}</div>
                <div className="step-body">
                  <div className="st-title">{title}</div>
                  <div className="st-sub">{sub}</div>
                </div>
                <span className="spacer" />
                <Badge tone={active ? "success" : "neutral"}>
                  {active ? "启用" : "未启用"}
                </Badge>
              </div>
            ))}
          </div>
        </Card>
        <Card>
          <CardHead icon={<Terminal size={18} />} title="安装控制" sub="每个按钮都会调用本机 Tauri 后端并写入日志" />
          <div className="action-grid">
            <button className="btn" disabled={!!busy} onClick={() => runAction("检测本机工具", "detect_tools")}>
              <Stethoscope size={15} /> 检测环境
            </button>
            <button className="btn" disabled={!!busy} onClick={() => runInstallTransaction("rtk", "事务安装/验证 RTK")}>
              <Terminal size={15} /> 安装 RTK
            </button>
            <button className="btn" disabled={!!busy} onClick={() => runInstallTransaction("headroom", "事务安装/验证 Headroom")}>
              <Gauge size={15} /> 安装 Headroom
            </button>
            <button className="btn primary" disabled={!!busy} onClick={() => runInstallTransaction("all", "一键事务安装全部组件")}>
              <Archive size={15} /> 一键全部安装
            </button>
            <button className="btn ghost" disabled={!!busy} onClick={() => runAction("运行 Doctor", "run_doctor")}>
              <ShieldCheck size={15} /> Doctor
            </button>
            <button className="btn ghost" disabled={!!busy} onClick={() => runAction("隔离测试 OpenCode 模型", "test_opencode", { model: opencodeModel })}>
              <Bot size={15} /> 隔离测试模型
            </button>
          </div>

          <SectionTitle>安装选项</SectionTitle>
          <div className="form-grid">
            <label className="field">
              <span>安装目录</span>
              <input value={installRoot} onChange={(e) => setInstallRoot(e.target.value)} />
            </label>
            <label className="field">
              <span>OpenCode 测试端口</span>
              <input value={proxyPort} onChange={(e) => setProxyPort(e.target.value)} />
            </label>
            <label className="field">
              <span>OpenCode 测试模型</span>
              <select value={opencodeModel} onChange={(e) => setOpencodeModel(e.target.value)}>
                {modelOptions.map((model) => (
                  <option key={model} value={model}>{model}</option>
                ))}
              </select>
            </label>
          </div>

          <div className="toggle-list mt-16">
            <div className="toggle-row">
              <div><strong>自动备份</strong><span>写配置前保存原始文件</span></div>
              <Toggle on={autoBackup} onClick={() => setAutoBackup((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>安装后 Doctor</strong><span>安装完成后自动诊断</span></div>
              <Toggle on={runDoctorAfter} onClick={() => setRunDoctorAfter((v) => !v)} />
            </div>
            <div className="toggle-row">
              <div><strong>创建回滚点</strong><span>失败时按事务回退</span></div>
              <Toggle on={rollbackPoint} onClick={() => setRollbackPoint((v) => !v)} />
            </div>
          </div>
          <div className="banner" style={{ marginTop: 12, marginBottom: 0, background: "var(--info-bg)" }}>
            <Info className="banner-ico" size={18} />
            <div className="banner-txt">
              <strong>测试隔离</strong>
              <span>模型测试只对 OpenCode 子进程生效，不写 Claude Code、Cursor、Codex CLI 或系统代理配置。</span>
            </div>
          </div>
        </Card>
      </div>

      <div className="grid cols-2 mt-16">
        <Card>
          <CardHead icon={<Stethoscope size={18} />} title={t("setup.scanTitle")} />
          <div className="list">
            {tools.map((tool) => (
              <div className="list-row" key={tool.id}>
                {stateIcon(tool.installed ? "ok" : tool.status === "unknown" ? "warn" : "err")}
                <div>
                  <div className="row-title">{tool.label}</div>
                  <div className="row-sub">
                    {tool.version ?? tool.detail}
                    {tool.path && <span className="mono"> · {tool.path}</span>}
                  </div>
                </div>
                <span className="spacer" />
                {toolState(tool)}
              </div>
            ))}
          </div>
        </Card>
        <Card>
          <CardHead icon={<ScrollText size={18} />} title="实时安装日志" sub="最新日志在最上方" />
          <div className="log install-log">
            {logs.map((entry, i) => (
              <div className="log-row" key={`${entry.time}-${i}`}>
                <span className="log-time">{entry.time}</span>
                <span className={`log-lvl ${logClass(entry.level)}`}>{entry.level.toUpperCase()}</span>
                <span>{entry.message}</span>
              </div>
            ))}
          </div>
        </Card>
      </div>

      <Card className="mt-16">
        <CardHead
          icon={<GitCompareArrows size={18} />}
          title={t("setup.previewTitle")}
          sub={installPlan ? `${installPlan.targets.join(" + ")} · ${installPlan.writesEnabled ? "托管应用" : "只读验证"}` : "等待后端生成计划"}
          action={
            <button className="btn ghost sm" disabled={!!busy} onClick={() => refreshInstallPlan(planTarget)}>
              刷新计划
            </button>
          }
        />
        {installPlan?.warnings.length ? (
          <div className="banner" style={{ marginBottom: 12, background: "var(--warning-bg)" }}>
            <AlertTriangle className="banner-ico" size={18} />
            <div className="banner-txt">
              {installPlan.warnings.map((warning) => (
                <span key={warning}>{warning}</span>
              ))}
            </div>
          </div>
        ) : null}
        <div className="diff">
          {(plannedDiff.length > 0
            ? plannedDiff
            : [{ type: "ctx" as const, text: "# 后端安装计划生成中" }]
          ).map((line, i) => (
            <div className={`diff-line ${line.type}`} key={`${line.text}-${i}`}>
              {line.text}
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/* ============================= Agents ============================= */
export function Agents() {
  const { t } = useI18n();
  const data = useDesktopData();
  const supLabel = (s: string) =>
    s === "ok" ? t("common.supported") : s === "partial" ? t("common.partial") : s === "no" ? t("common.notSupported") : t("common.planned");
  return (
    <div className="page">
      <div className="banner">
        <Info className="banner-ico" size={20} />
        <div className="banner-txt">
          <span>{t("agents.matrixNote")}</span>
        </div>
      </div>
      <Card pad={false}>
        <div className="table-wrap">
          <table className="tbl">
            <thead>
              <tr>
                <th>{t("common.name")}</th>
                <th>{t("agents.rtk")}</th>
                <th>{t("common.method")}</th>
                <th>{t("agents.headroom")}</th>
                <th>{t("agents.macos")}</th>
                <th>{t("agents.win")}</th>
                <th>{t("agents.wsl")}</th>
                <th>{t("common.limitations")}</th>
              </tr>
            </thead>
            <tbody>
              {data.agents.map((a) => (
                <tr key={a.name}>
                  <td className="name">{a.name}</td>
                  <td><Badge tone={supportTone(a.rtk)}>{supLabel(a.rtk)}</Badge></td>
                  <td className="muted">{a.rtkMethod}</td>
                  <td><Badge tone={supportTone(a.headroom)}>{supLabel(a.headroom)}</Badge></td>
                  <td><Badge tone={supportTone(a.macos)}>{supLabel(a.macos)}</Badge></td>
                  <td><Badge tone={supportTone(a.win)}>{supLabel(a.win)}</Badge></td>
                  <td><Badge tone={supportTone(a.wsl)}>{supLabel(a.wsl)}</Badge></td>
                  <td className="muted">{a.note}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>
    </div>
  );
}

/* ============================= Projects ============================= */
export function Projects() {
  const { t } = useI18n();
  const data = useDesktopData();
  const firstProject = data.projects[0];
  const [level, setLevel] = useState("balanced");
  const [route, setRoute] = useState(firstProject?.route ?? "opencode-test");
  const [verbosity, setVerbosity] = useState("auto");
  const [logs, setLogs] = useState<ActionLog[]>(initialLogs);
  const [busy, setBusy] = useState(false);
  const saveProjectPolicy = async () => {
    setBusy(true);
    const result = await runDesktopAction("save_app_settings", {
      section: "project-policy",
      payload: {
        root: firstProject?.root ?? "D:\\D-TOKEN\\d-token",
        level,
        route,
        outputVerbosity: verbosity,
        enableCcr: true,
        enableMemory: true,
        enableLearn: true,
        enableMcp: true,
        savedFrom: "projects-page",
      },
    });
    setLogs((prev) => [...result.logs, ...prev].slice(0, 60));
    setBusy(false);
  };
  return (
    <div className="page">
      <Card className="mb-16">
        <CardHead
          icon={<FolderGit2 size={18} />}
          title="项目压缩策略"
          sub="按项目保存 Headroom/RTK 策略；外部配置写入仍走 diff / backup / rollback"
          action={<button className="btn primary sm" disabled={busy} onClick={saveProjectPolicy}>保存项目策略</button>}
        />
        <div className="form-grid">
          <label className="field">
            <span>{t("common.level")}</span>
            <select value={level} onChange={(e) => setLevel(e.target.value)}>
              <option value="safe">safe</option>
              <option value="balanced">balanced</option>
              <option value="aggressive">aggressive</option>
              <option value="max">max</option>
            </select>
          </label>
          <label className="field">
            <span>{t("proj.route")}</span>
            <select value={route} onChange={(e) => setRoute(e.target.value)}>
              {data.routes.map((r) => (
                <option key={r.name} value={r.name}>{r.name}</option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>Output verbosity</span>
            <select value={verbosity} onChange={(e) => setVerbosity(e.target.value)}>
              <option value="auto">auto</option>
              <option value="terse">terse</option>
              <option value="normal">normal</option>
              <option value="detailed">detailed</option>
            </select>
          </label>
        </div>
      </Card>
      <div className="grid cols-1 stagger" style={{ gridTemplateColumns: "1fr" }}>
        {data.projects.map((p) => (
          <Card key={p.root}>
            <CardHead
              icon={<FolderGit2 size={18} />}
              title={p.root}
              sub={p.stack}
              action={<Badge tone="info">{p.level}</Badge>}
            />
            <div className="grid cols-3">
              <KV k={t("proj.route")} v={p.route} />
              <KV k={t("proj.rtkProfile")} v={p.rtkProfile} />
              <KV k={t("proj.policy")} v={p.policy} />
            </div>
            <div className="kv" style={{ borderBottom: "none" }}>
              <span className="k">{t("proj.agents")}</span>
              <span className="v wrap" style={{ justifyContent: "flex-end" }}>
                {p.agents.map((a) => (
                  <Badge key={a}>{a}</Badge>
                ))}
              </span>
            </div>
            <div className="banner" style={{ marginBottom: 0, marginTop: 8, background: "var(--info-bg)" }}>
              <Info className="banner-ico" size={18} />
              <div className="banner-txt">
                <strong>{t("proj.reason")}</strong>
                <span>{p.reason}</span>
              </div>
            </div>
            <SectionTitle>{t("proj.features")}</SectionTitle>
            <div className="wrap">
              {p.features.map((f) => (
                <Badge key={f} tone="success" dot>
                  {f}
                </Badge>
              ))}
            </div>
          </Card>
        ))}
      </div>
      <Card className="mt-16">
        <CardHead icon={<ScrollText size={18} />} title="项目策略日志" sub={busy ? "保存中" : "最近保存结果"} />
        <div className="log install-log">
          {logs.map((entry, i) => (
            <div className="log-row" key={`${entry.time}-${i}`}>
              <span className="log-time">{entry.time}</span>
              <span className={`log-lvl ${logClass(entry.level)}`}>{entry.level.toUpperCase()}</span>
              <span>{entry.message}</span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/* ============================= Routes ============================= */
export function Routes() {
  const { t } = useI18n();
  const data = useDesktopData();
  const [routeName, setRouteName] = useState("opencode-test");
  const [protocol, setProtocol] = useState("OpenAI-compatible");
  const [host, setHost] = useState("127.0.0.1:8788");
  const [provider, setProvider] = useState("Headroom OpenCode test proxy");
  const [policy, setPolicy] = useState("opencode-only");
  const [logs, setLogs] = useState<ActionLog[]>(initialLogs);
  const [busy, setBusy] = useState(false);
  const saveRouteDraft = async () => {
    setBusy(true);
    const result = await runDesktopAction("save_app_settings", {
      section: "app",
      payload: {
        kind: "route-draft",
        name: routeName,
        protocol,
        host,
        provider,
        policy,
        testScope: routeName.includes("opencode") ? "opencode-only" : "managed-route",
      },
    });
    setLogs((prev) => [...result.logs, ...prev].slice(0, 60));
    setBusy(false);
  };
  return (
    <div className="page">
      <Card className="mb-16">
        <CardHead
          icon={<RouteIcon size={18} />}
          title="路由草案"
          sub="保存为 d-token 本地草案；真正写 provider/agent 配置前必须进入 diff 和备份"
          action={<button className="btn primary sm" disabled={busy} onClick={saveRouteDraft}>保存路由草案</button>}
        />
        <div className="form-grid">
          <label className="field">
            <span>{t("route.id")}</span>
            <input value={routeName} onChange={(e) => setRouteName(e.target.value)} />
          </label>
          <label className="field">
            <span>{t("common.protocol")}</span>
            <select value={protocol} onChange={(e) => setProtocol(e.target.value)}>
              <option value="OpenAI-compatible">OpenAI-compatible</option>
              <option value="Anthropic-compatible">Anthropic-compatible</option>
            </select>
          </label>
          <label className="field">
            <span>{t("route.host")}</span>
            <input value={host} onChange={(e) => setHost(e.target.value)} />
          </label>
          <label className="field">
            <span>{t("common.provider")}</span>
            <input value={provider} onChange={(e) => setProvider(e.target.value)} />
          </label>
          <label className="field">
            <span>{t("route.policy")}</span>
            <select value={policy} onChange={(e) => setPolicy(e.target.value)}>
              <option value="opencode-only">opencode-only</option>
              <option value="adaptive">adaptive</option>
              <option value="safe">safe</option>
              <option value="aggressive">aggressive</option>
            </select>
          </label>
        </div>
      </Card>
      <Card pad={false}>
        <div className="table-wrap">
          <table className="tbl">
            <thead>
              <tr>
                <th>{t("route.id")}</th>
                <th>{t("common.protocol")}</th>
                <th>{t("route.host")}</th>
                <th>{t("common.provider")}</th>
                <th>{t("route.chain")}</th>
                <th>{t("route.policy")}</th>
                <th>{t("common.error")}</th>
              </tr>
            </thead>
            <tbody>
              {data.routes.map((r) => (
                <tr key={r.id}>
                  <td className="name">{r.name}</td>
                  <td>
                    <Badge tone={r.protocol.startsWith("Anthropic") ? "info" : "neutral"}>
                      {r.protocol}
                    </Badge>
                  </td>
                  <td className="mono">{r.host}</td>
                  <td>{r.provider}</td>
                  <td>{r.chain}</td>
                  <td>{r.policy}</td>
                  <td>
                    {r.errors === 0 ? (
                      <Badge tone="success">0</Badge>
                    ) : (
                      <Badge tone="danger">{r.errors}</Badge>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>

      <Card className="mt-16">
        <CardHead icon={<ShieldCheck size={18} />} title={t("route.rulesTitle")} />
        <div className="grid cols-2">
          {["route.rule1", "route.rule2", "route.rule3", "route.rule4"].map((k) => (
            <div className="list-row" key={k} style={{ borderBottom: "none" }}>
              <CheckCircle2 size={16} style={{ color: "var(--success)" }} />
              <span className="row-title" style={{ fontWeight: 500 }}>{t(k as never)}</span>
            </div>
          ))}
        </div>
      </Card>
      <Card className="mt-16">
        <CardHead icon={<ScrollText size={18} />} title="路由配置日志" sub={busy ? "保存中" : "最近保存结果"} />
        <div className="log install-log">
          {logs.map((entry, i) => (
            <div className="log-row" key={`${entry.time}-${i}`}>
              <span className="log-time">{entry.time}</span>
              <span className={`log-lvl ${logClass(entry.level)}`}>{entry.level.toUpperCase()}</span>
              <span>{entry.message}</span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/* ============================= Providers ============================= */
export function Providers() {
  const { t } = useI18n();
  const data = useDesktopData();
  const [name, setName] = useState("Headroom OpenCode test proxy");
  const [protocol, setProtocol] = useState("OpenAI");
  const [baseUrl, setBaseUrl] = useState("http://127.0.0.1:8788");
  const [models, setModels] = useState("OpenCode model-test traffic only");
  const [timeout, setTimeoutValue] = useState("60s");
  const [credentialRef, setCredentialRef] = useState("local-env");
  const [logs, setLogs] = useState<ActionLog[]>(initialLogs);
  const [busy, setBusy] = useState(false);
  const saveProviderDraft = async () => {
    setBusy(true);
    const result = await runDesktopAction("save_app_settings", {
      section: "app",
      payload: {
        kind: "provider-draft",
        name,
        protocol,
        baseUrl,
        models,
        timeout,
        credentialRef,
        redaction: "credentials are references only; secrets are not stored in d-token state",
      },
    });
    setLogs((prev) => [...result.logs, ...prev].slice(0, 60));
    setBusy(false);
  };
  return (
    <div className="page">
      <div className="banner">
        <ShieldCheck className="banner-ico" size={20} />
        <div className="banner-txt">
          <span>{t("prov.credNote")}</span>
        </div>
      </div>
      <Card className="mb-16">
        <CardHead
          icon={<Server size={18} />}
          title="Provider 草案"
          sub="只保存引用和 base URL，不保存 API key 明文"
          action={<button className="btn primary sm" disabled={busy} onClick={saveProviderDraft}>保存 Provider 草案</button>}
        />
        <div className="form-grid">
          <label className="field">
            <span>{t("common.name")}</span>
            <input value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <label className="field">
            <span>{t("common.protocol")}</span>
            <select value={protocol} onChange={(e) => setProtocol(e.target.value)}>
              <option value="OpenAI">OpenAI</option>
              <option value="Anthropic">Anthropic</option>
              <option value="OpenAI-compatible">OpenAI-compatible</option>
              <option value="Anthropic-compatible">Anthropic-compatible</option>
            </select>
          </label>
          <label className="field">
            <span>{t("prov.baseUrl")}</span>
            <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
          </label>
          <label className="field">
            <span>{t("prov.models")}</span>
            <input value={models} onChange={(e) => setModels(e.target.value)} />
          </label>
          <label className="field">
            <span>{t("prov.timeout")}</span>
            <input value={timeout} onChange={(e) => setTimeoutValue(e.target.value)} />
          </label>
          <label className="field">
            <span>{t("prov.cred")}</span>
            <input value={credentialRef} onChange={(e) => setCredentialRef(e.target.value)} />
          </label>
        </div>
      </Card>
      <div className="grid cols-2 stagger">
        {data.providers.map((p) => (
          <Card key={p.name}>
            <CardHead
              icon={<Server size={18} />}
              title={p.name}
              sub={p.kind}
              action={<StateBadge state={p.health} label={p.health === "ok" ? t("common.ok") : p.health === "warn" ? t("common.warning") : t("common.error")} />}
            />
            <KV k={t("common.protocol")} v={p.protocol} />
            <KV k={t("prov.baseUrl")} v={<span className="mono" style={{ fontSize: 12 }}>{p.baseUrl}</span>} />
            <KV k={t("prov.models")} v={<span style={{ fontSize: 12.5 }}>{p.models}</span>} />
            <KV k={t("prov.timeout")} v={p.timeout} />
            <KV k={t("prov.cred")} v={<Badge tone="info" dot>{p.cred}</Badge>} />
          </Card>
        ))}
      </div>
      <Card className="mt-16">
        <CardHead icon={<ScrollText size={18} />} title="Provider 配置日志" sub={busy ? "保存中" : "最近保存结果"} />
        <div className="log install-log">
          {logs.map((entry, i) => (
            <div className="log-row" key={`${entry.time}-${i}`}>
              <span className="log-time">{entry.time}</span>
              <span className={`log-lvl ${logClass(entry.level)}`}>{entry.level.toUpperCase()}</span>
              <span>{entry.message}</span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/* ============================= Doctor ============================= */
export function Doctor() {
  const { t } = useI18n();
  const data = useDesktopData();
  const [logs, setLogs] = useState<ActionLog[]>(initialLogs);
  const [checks, setChecks] = useState(data.doctorChecks);
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState("等待运行 Doctor");
  useEffect(() => {
    setChecks(data.doctorChecks);
  }, [data.doctorChecks]);
  const passed = checks.filter((c) => c.state === "ok").length;
  const warns = checks.filter((c) => c.state === "warn").length;
  const fails = checks.filter((c) => c.state === "err").length;
  const runDoctorNow = async () => {
    setBusy(true);
    const result = await runDesktopAction("run_doctor");
    setSummary(result.summary);
    setLogs((prev) => [...result.logs, ...prev].slice(0, 80));
    try {
      const next = await invoke<Partial<DesktopData>>("get_desktop_data");
      if (next.doctorChecks) setChecks(next.doctorChecks);
    } catch {
      // Keep the last Doctor table if refresh is unavailable.
    }
    setBusy(false);
  };
  return (
    <div className="page">
      <div className="banner safe">
        <ShieldCheck className="banner-ico" size={20} />
        <div className="banner-txt">
          <span>{t("doctor.noTouch")}</span>
        </div>
        <div className="right">
          <button className="btn sm" disabled={busy} onClick={runDoctorNow}>
            <Stethoscope size={14} /> {t("doctor.rerun")}
          </button>
        </div>
      </div>

      <div className="grid cols-3 stagger">
        <StatCard icon={<CheckCircle2 size={20} />} label={t("doctor.passed")} value={passed} accent="#22c55e" />
        <StatCard icon={<AlertTriangle size={20} />} label={t("doctor.warnings")} value={warns} accent="#f59e0b" />
        <StatCard icon={<XCircle size={20} />} label={t("doctor.failed")} value={fails} accent="#ef4444" />
      </div>

      <Card pad={false} className="mt-16">
        <div className="table-wrap">
          <table className="tbl">
            <thead>
              <tr>
                <th style={{ width: 40 }}></th>
                <th>{t("doctor.check")}</th>
                <th>{t("doctor.detail")}</th>
              </tr>
            </thead>
            <tbody>
              {checks.map((c) => (
                <tr key={c.check}>
                  <td>{stateIcon(c.state)}</td>
                  <td className="name">{c.check}</td>
                  <td className="muted">{c.detail}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>

      <Card className="mt-16">
        <CardHead icon={<ScrollText size={18} />} title="Doctor 实时日志" sub={busy ? "诊断运行中" : summary} />
        <div className="log install-log">
          {logs.map((entry, i) => (
            <div className="log-row" key={`${entry.time}-${i}`}>
              <span className="log-time">{entry.time}</span>
              <span className={`log-lvl ${logClass(entry.level)}`}>{entry.level.toUpperCase()}</span>
              <span>{entry.message}</span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/* ============================= Logs ============================= */
export function Logs() {
  const { t } = useI18n();
  const data = useDesktopData();
  const [filter, setFilter] = useState("all");
  const tabs = [
    { id: "all", label: t("logs.all") },
    { id: "request", label: t("logs.request") },
    { id: "route", label: t("logs.routeDec") },
    { id: "rtk", label: t("logs.rtk") },
    { id: "headroom", label: t("logs.headroom") },
    { id: "error", label: t("logs.errors") },
  ];
  const rows = data.logs.filter((l) => filter === "all" || l.kind === filter);
  return (
    <div className="page">
      <div className="banner">
        <ShieldCheck className="banner-ico" size={20} />
        <div className="banner-txt">
          <span>{t("logs.redactNote")}</span>
        </div>
      </div>
      <Tabs tabs={tabs} active={filter} onChange={setFilter} />
      <Card>
        <div className="log">
          {rows.map((l, i) => (
            <div className="log-row" key={i}>
              <span className="log-time">{l.time}</span>
              <span className={`log-lvl ${l.level}`}>{l.level.toUpperCase()}</span>
              <span>{l.msg}</span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/* ============================= Changes ============================= */
export function Changes() {
  const { t } = useI18n();
  const data = useDesktopData();
  const [activeChange, setActiveChange] = useState(fx.changes[0]?.id ?? "");
  const riskTone = (r: string) => (r === "low" ? "success" : r === "medium" ? "warning" : "danger");
  const changes = data.changes.length > 0 ? data.changes : fx.changes;
  return (
    <div className="page">
      <div className="banner">
        <Info className="banner-ico" size={20} />
        <div className="banner-txt">
          <span>{t("changes.planOnly")}</span>
        </div>
      </div>
      <div className="grid stagger" style={{ gridTemplateColumns: "1fr" }}>
        {changes.map((c) => {
          const active = activeChange === c.id;
          return (
          <Card key={c.id} style={{ borderColor: active ? "var(--brand-1)" : undefined }}>
            <CardHead
              icon={<GitCompareArrows size={18} />}
              title={c.summary}
              sub={c.target}
              action={
                <div className="row">
                  {active && <Badge tone="info">当前预览</Badge>}
                  <Badge tone={riskTone(c.risk)}>{t("common.risk")}: {c.risk}</Badge>
                  <button className="btn sm" onClick={() => setActiveChange(c.id)}>{t("changes.preview")}</button>
                </div>
              }
            />
            {active ? (
              <>
                <div className="diff">
                  {c.diff.map((d, i) => (
                    <div className={`diff-line ${d.type}`} key={i}>
                      {d.text}
                    </div>
                  ))}
                </div>
                <div className="grid cols-2 mt-16">
                  <KV k={t("changes.backupAt")} v={<span className="mono" style={{ fontSize: 11.5 }}>{c.backup}</span>} />
                  <div className="kv" style={{ borderBottom: "none" }}>
                    <span className="k">{t("changes.postDoctor")}</span>
                    <span className="v wrap" style={{ justifyContent: "flex-end" }}>
                      {c.postChecks.map((p) => (
                        <Badge key={p}>{p}</Badge>
                      ))}
                    </span>
                  </div>
                </div>
              </>
            ) : (
              <p className="muted" style={{ margin: 0, fontSize: 12.5 }}>
                点击预览查看这项变更的 diff、备份位置和应用后检查。
              </p>
            )}
          </Card>
        )})}
      </div>
    </div>
  );
}
/* ============================= Backup & Rollback ============================= */
export function Backup() {
  const { t } = useI18n();
  const data = useDesktopData();
  const [logs, setLogs] = useState<ActionLog[]>(initialLogs);
  const [busy, setBusy] = useState<string | null>(null);
  const runBackupAction = async (action: "restore" | "cleanup", id: string) => {
    setBusy(`${action}:${id}`);
    const result = await runDesktopAction("manage_backup", { action, id });
    setLogs((prev) => [...result.logs, ...prev].slice(0, 60));
    setBusy(null);
  };
  return (
    <div className="page">
      <div className="banner">
        <Archive className="banner-ico" size={20} />
        <div className="banner-txt">
          <span>{t("backup.note")}</span>
        </div>
      </div>
      <div className="grid cols-2 stagger">
        {data.backupPoints.map((b) => (
          <Card key={b.id}>
            <CardHead
              icon={<Archive size={18} />}
              title={b.label}
              sub={b.when}
              action={
                b.verified ? (
                  <StateBadge state="ok" label={t("common.ok")} />
                ) : (
                  <StateBadge state="warn" label={t("common.warning")} />
                )
              }
            />
            <KV k={t("backup.filesChanged")} v={b.files} />
            <KV k={t("backup.toolsInstalled")} v={<span className="wrap">{b.tools.map((x) => <Badge key={x}>{x}</Badge>)}</span>} />
            <div className="row mt-16" style={{ gap: 8 }}>
              <button className="btn sm" disabled={!!busy || b.id === "local-plan"} onClick={() => runBackupAction("restore", b.id)}>{t("backup.restore")}</button>
              <button className="btn ghost sm" disabled={!!busy || b.id === "local-plan"} onClick={() => runBackupAction("cleanup", b.id)}>{t("backup.cleanup")}</button>
              {b.id === "local-plan" && <Badge tone="warning" dot>waiting for transaction</Badge>}
            </div>
          </Card>
        ))}
      </div>
      <Card className="mt-16">
        <CardHead icon={<ScrollText size={18} />} title="回滚操作日志" sub={busy ?? "恢复会先加载计划，不会静默覆盖外部配置"} />
        <div className="log install-log">
          {logs.map((entry, i) => (
            <div className="log-row" key={`${entry.time}-${i}`}>
              <span className="log-time">{entry.time}</span>
              <span className={`log-lvl ${logClass(entry.level)}`}>{entry.level.toUpperCase()}</span>
              <span>{entry.message}</span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/* ============================= Settings ============================= */
function Toggle({ on, onClick }: { on: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      style={{
        width: 46,
        height: 26,
        borderRadius: 999,
        border: "none",
        cursor: "pointer",
        padding: 3,
        background: on ? "var(--brand-grad)" : "var(--border-strong)",
        transition: "background .25s var(--ease)",
        display: "flex",
        justifyContent: on ? "flex-end" : "flex-start",
      }}
    >
      <span
        style={{
          width: 20,
          height: 20,
          borderRadius: "50%",
          background: "#fff",
          boxShadow: "0 1px 3px rgba(0,0,0,.3)",
          transition: "all .25s var(--ease)",
        }}
      />
    </button>
  );
}

export function Settings() {
  const { t, lang, setLang } = useI18n();
  const { theme, setTheme } = useTheme();
  const [debug, setDebug] = useState(false);
  const [defaultMode, setDefaultMode] = useState("external");
  const [retentionDays, setRetentionDays] = useState("30");
  const [apiPort, setApiPort] = useState("8787");
  const [logs, setLogs] = useState<ActionLog[]>(initialLogs);
  const [busy, setBusy] = useState(false);
  const saveSettings = async () => {
    setBusy(true);
    const result = await runDesktopAction("save_app_settings", {
      section: "app",
      payload: {
        kind: "app-settings",
        theme,
        lang,
        defaultMode,
        retentionDays,
        redaction: "standard",
        updateChannel: "stable",
        apiPort,
        debugLogEnabled: debug,
      },
    });
    setLogs((prev) => [...result.logs, ...prev].slice(0, 60));
    setBusy(false);
  };

  return (
    <div className="page">
      <Card>
        <CardHead icon={<Sun size={18} />} title={t("set.appearance")} />
        <div className="kv">
          <div>
            <div className="row-title" style={{ fontWeight: 600 }}>{t("set.theme")}</div>
            <div className="muted" style={{ fontSize: 12.5 }}>{t("set.themeDesc")}</div>
          </div>
          <div className="tabs" style={{ marginBottom: 0 }}>
            <button className={`tab ${theme === "light" ? "active" : ""}`} onClick={() => setTheme("light")}>
              <Sun size={14} style={{ marginRight: 6, verticalAlign: -2 }} />{t("set.light")}
            </button>
            <button className={`tab ${theme === "dark" ? "active" : ""}`} onClick={() => setTheme("dark")}>
              <Moon size={14} style={{ marginRight: 6, verticalAlign: -2 }} />{t("set.dark")}
            </button>
          </div>
        </div>
        <div className="kv" style={{ borderBottom: "none" }}>
          <div>
            <div className="row-title" style={{ fontWeight: 600 }}>{t("set.language")}</div>
            <div className="muted" style={{ fontSize: 12.5 }}>{t("set.languageDesc")}</div>
          </div>
          <div className="tabs" style={{ marginBottom: 0 }}>
            <button className={`tab ${lang === "zh" ? "active" : ""}`} onClick={() => setLang("zh")}>中文</button>
            <button className={`tab ${lang === "en" ? "active" : ""}`} onClick={() => setLang("en")}>English</button>
          </div>
        </div>
      </Card>

      <Card className="mt-16">
        <CardHead
          icon={<Boxes size={18} />}
          title={t("set.general")}
          action={<button className="btn primary sm" disabled={busy} onClick={saveSettings}>保存设置</button>}
        />
        <div className="form-grid">
          <label className="field">
            <span>{t("set.defaultMode")}</span>
            <select value={defaultMode} onChange={(e) => setDefaultMode(e.target.value)}>
              <option value="external">{t("mode.external")}</option>
              <option value="managed">{t("mode.managed")}</option>
              <option value="fixture">{t("mode.fixture")}</option>
            </select>
          </label>
          <label className="field">
            <span>{t("set.retention")}</span>
            <select value={retentionDays} onChange={(e) => setRetentionDays(e.target.value)}>
              <option value="7">{t("set.retention7")}</option>
              <option value="30">{t("set.retention30")}</option>
            </select>
          </label>
          <label className="field">
            <span>{t("set.apiPort")}</span>
            <input value={apiPort} onChange={(e) => setApiPort(e.target.value)} />
          </label>
        </div>
        <KV k={t("set.redaction")} v={t("set.standard")} />
        <KV k={t("set.updateChannel")} v={t("set.stable")} />
      </Card>

      <Card className="mt-16" style={{ borderColor: "var(--danger)" }}>
        <CardHead icon={<AlertTriangle size={18} />} title={t("set.danger")} />
        <div className="kv" style={{ borderBottom: "none" }}>
          <div>
            <div className="row-title" style={{ fontWeight: 600, color: "var(--danger)" }}>{t("set.debugLog")}</div>
            <div className="muted" style={{ fontSize: 12.5, maxWidth: 460 }}>{t("set.debugDesc")}</div>
          </div>
          <Toggle on={debug} onClick={() => setDebug((v) => !v)} />
        </div>
      </Card>
      <Card className="mt-16">
        <CardHead icon={<ScrollText size={18} />} title="设置保存日志" sub={busy ? "保存中" : "最近保存结果"} />
        <div className="log install-log">
          {logs.map((entry, i) => (
            <div className="log-row" key={`${entry.time}-${i}`}>
              <span className="log-time">{entry.time}</span>
              <span className={`log-lvl ${logClass(entry.level)}`}>{entry.level.toUpperCase()}</span>
              <span>{entry.message}</span>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}
