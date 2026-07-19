import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  Bot,
  CheckCircle2,
  ChevronRight,
  HardDrive,
  MonitorCog,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Sparkles,
  Stethoscope,
  Wrench,
} from "lucide-react";
import { Surface } from "../../components/common/Surface";
import { StatusBadge } from "../../components/common/StatusBadge";
import { DataSourceBadge } from "../../components/common/DataSourceBadge";
import { OperationDialog } from "../../components/common/OperationDialog";
import { Button } from "../../components/ui/button";
import { Badge } from "../../components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import { translateCopy, useI18n } from "../i18n";
import type { DoctorCheck } from "./types";
import type { PageId } from "../nav";
import { invokeNative, type NativeActionResult } from "../native";
import { createOperationRequestId, useNotifications } from "../notifications";
import { presentOperationFailure } from "../userCopy";
import { classifyEvidence } from "../state/evidence";

const LAYERS: DoctorCheck["layer"][] = ["runtime", "port", "route", "provider", "security"];

type VerificationCheck = {
  id: string;
  label: string;
  ok: boolean;
  summary: string;
};

type VerificationResult = {
  ok: boolean;
  passed: number;
  total: number;
  summary: string;
  checks: VerificationCheck[];
  service_url?: string;
};

type ProductReadinessResult = {
  ok: boolean;
  mode: "product-readiness-check";
  passed: number;
  total: number;
  summary: string;
  privacy: string;
  external_services: string;
  checks: Array<{ id: string; label: string; ok: boolean; summary: unknown; detail?: unknown }>;
};

export const DOCTOR_READINESS_TITLE = "产品就绪检查";

type DoctorSource = "preview" | "native" | "unavailable" | "stale" | "partial";
type DoctorIssueState = "repairable" | "manual" | "blocked";
type DoctorIssueSeverity = "warning" | "error";

type DoctorRemediation = {
  kind: string;
  label: string;
  risk: "low" | "medium" | "high";
  writes_config: boolean;
  requires_confirmation: boolean;
  destination?: PageId;
};

export type DoctorIssue = {
  id: string;
  category: DoctorCheck["layer"];
  severity: DoctorIssueSeverity;
  state: DoctorIssueState;
  title: string;
  impact: string;
  cause: string;
  evidence: string;
  remediation?: DoctorRemediation;
};

export type DoctorReport = {
  ok: boolean;
  mode: "doctor-report-v2";
  revision: string;
  source: "dtoken-desktop-control-plane";
  freshness: "fresh" | "stale" | "unknown";
  collected_at: string;
  summary: string;
  counts: { healthy: number; repairable: number; manual: number; blocked: number };
  issues: DoctorIssue[];
};

type DoctorRepairPlan = {
  ok: boolean;
  mode: "doctor-repair-plan";
  report_revision: string;
  issue_ids: string[];
  risk: "low" | "medium" | "high";
  summary: string;
};

type DoctorActionReceipt = {
  ok: boolean;
  mode: "doctor-action-receipt";
  operation_id: string;
  summary: string;
  report: DoctorReport;
};

type DoctorModuleDirectory = "core" | "agent" | "optical" | "platform" | "recovery";
type DoctorModuleId = DoctorModuleDirectory | string;

type DoctorAgentOption = {
  id: string;
  name: string;
  detected?: boolean;
  supported?: boolean;
  state?: string;
  managed_scopes?: Array<"global" | "project">;
  managed_targets?: Array<{ scope: "global" | "project"; project_root?: string | null }>;
};

type DoctorModuleV3 = {
  module_id: DoctorModuleId;
  state: string;
  severity: string;
  summary?: string;
  reason_codes?: string[];
  evidence_revision?: string;
  agents?: DoctorAgentOption[];
};

type DoctorSnapshotV3 = {
  schema_version: 3;
  revision: string;
  source: string;
  state: string;
  overall_state?: string;
  collected_at_ms: number;
  ttl_ms: number;
  coverage?: {
    completed?: number;
    total?: number;
    agents_detected?: number;
    agents_selected?: number;
  };
  counts: { healthy: number; repairable: number; manual: number; blocked: number };
  modules: DoctorModuleV3[];
};

type DoctorRepairSelectionV3 = {
  agent_ids: string[];
  route_ids: string[];
  scope: "global" | "project";
  project_root?: string | null;
  install_instructions?: boolean;
  install_hook_or_plugin?: boolean;
  install_skill?: boolean;
  install_mcp?: boolean;
};

type DoctorRepairTargetV3 = {
  target_path: string;
  before_sha256?: string;
  after_sha256?: string;
  diff?: string;
};

type DoctorAgentPlanV3 = {
  agent_id: string;
  name?: string;
  state: string;
  summary?: string;
  targets?: DoctorRepairTargetV3[];
};

type DoctorRepairPlanV3 = {
  schema_version: 3;
  ok?: boolean;
  plan_id: string;
  snapshot_revision: string;
  module_id: DoctorModuleId;
  selection: DoctorRepairSelectionV3;
  risk: string;
  repair_class?: string;
  confirmation_required?: boolean;
  digest_algorithm?: string;
  created_at_ms: number;
  expires_at_ms: number;
  review_digest: string;
  summary?: string;
  agents?: DoctorAgentPlanV3[];
  actions?: Array<{
    action_id: string;
    summary: string;
    targets?: DoctorRepairTargetV3[];
  }>;
};

type DoctorAgentReceiptV3 = {
  agent_id: string;
  name?: string;
  outcome: string;
  summary?: string;
};

type DoctorActionReceiptV3 = {
  schema_version: 3;
  operation_id: string;
  plan_id: string;
  plan_review_digest?: string;
  plan_digest?: string;
  snapshot_revision: string;
  module_id: DoctorModuleId;
  outcome: string;
  post_verification?: string;
  verification?: string;
  started_at_ms: number;
  completed_at_ms: number;
  post_snapshot_revision?: string;
  post_scan_revision?: string;
  agents?: DoctorAgentReceiptV3[];
  snapshot?: DoctorSnapshotV3;
};

type DoctorSnapshot = DoctorReport | DoctorSnapshotV3;
type DoctorPhase = "idle" | "scanning" | "planning" | "applying" | "verifying";

const MODULE_PRESETS = [
  {
    id: "core",
    icon: Activity,
    zh: { title: "本地服务与端口", detail: "检查 runtime identity、进程归属、listener 与健康状态。" },
    en: { title: "Local service & ports", detail: "Check runtime identity, process ownership, listener, and health." },
  },
  {
    id: "agent",
    icon: Bot,
    zh: { title: "Agent 接入", detail: "按 Agent 配置、Hook/Plugin、Skill 与可选元数据诊断 MCP 检查。" },
    en: { title: "Agent integration", detail: "Check Agent configuration, Hook/Plugin, Skill, and optional metadata diagnostics MCP." },
  },
  {
    id: "optical",
    icon: Sparkles,
    zh: { title: "混合上下文优化", detail: "检查安全 Shadow、能力证据与接入前置；不在 Doctor 开启生产 active。" },
    en: { title: "Hybrid context optimization", detail: "Check safe Shadow, capability evidence, and prerequisites; production active is never enabled here." },
  },
  {
    id: "platform",
    icon: MonitorCog,
    zh: { title: "平台安装", detail: "检查平台签名、自动启动、路径与凭据库边界。" },
    en: { title: "Platform installation", detail: "Check platform signing, autostart, paths, and credential-store boundaries." },
  },
  {
    id: "recovery",
    icon: RotateCcw,
    zh: { title: "安全与恢复", detail: "检查备份完整性、当前可恢复性、事务与脱敏证据。" },
    en: { title: "Safety & recovery", detail: "Check backup integrity, current restorability, transactions, and redacted evidence." },
  },
] as const;

const SPECIALIZED_REPAIR_PRESETS = [
  { id: "service", source: "core", icon: Activity, title: "d-token 服务修复", detail: "启动或重启身份已验证的 d-token 自有服务，并自动复检。", action: "core" },
  { id: "listener", source: "core", icon: ShieldCheck, title: "本地监听与进程归属复检", detail: "重新检查 PID、进程出生标识、可执行文件、端口所有权与 runtime instance。", action: "scan" },
  { id: "route", source: "core", icon: Activity, title: "d-token 路由修复", detail: "重新发现 Provider/Route，并在来源事务中查看连接配置。", action: "install" },
  { id: "agent-hybrid", source: "agent", icon: Bot, title: "Agent Hybrid 接入重新注入", detail: "按 Agent 恢复 Instructions、Hook/Plugin 与 Recovery Skill 受管组合。", action: "agent", preset: "hybrid" },
  { id: "agent-hook", source: "agent", icon: Bot, title: "Agent Hook / Plugin 重新配置", detail: "仅查看并修复 SessionStart Hook 或 OpenCode Plugin。", action: "agent", preset: "hook" },
  { id: "agent-rules", source: "agent", icon: Bot, title: "Agent Instructions / Recovery Skill 修复", detail: "恢复受管规则和恢复 Skill，保留用户非受管内容。", action: "agent", preset: "instructions_skill" },
  { id: "agent-mcp", source: "agent", icon: Bot, title: "Agent metadata MCP 修复", detail: "高级模块；只恢复元数据诊断 MCP，不参与真实压缩。", action: "agent", preset: "mcp" },
  { id: "agent-transport", source: "agent", icon: Bot, title: "已配置 Agent 连接修复", detail: "OpenCode 使用专用 transport executor；其他 Agent 无 executor 时明确阻止。", action: "install" },
  { id: "optical", source: "optical", icon: Sparkles, title: "Optical Shadow 配置修复", detail: "只允许 off / shadow_preview，Doctor 不开放 production active。", action: "visual" },
  { id: "autostart", source: "platform", icon: MonitorCog, title: "平台自启动修复", detail: "在来源工作流中修复 d-token 自有 LaunchAgent 或 HKCU Run。", action: "install" },
  { id: "cc-switch", source: "core", icon: RefreshCw, title: "cc-switch 同步检查", detail: "只读重新发现 cc-switch，仅修复 d-token 侧 Route，不写第三方 profile。", action: "scan" },
  { id: "recovery", source: "recovery", icon: RotateCcw, title: "备份与回退链路复检", detail: "检查完整性、事务归属和当前可恢复性；损坏证据保持锁定。", action: "changes" },
] as const;

type AgentRepairPreset = "hybrid" | "hook" | "instructions_skill" | "mcp";

const DOCTOR_V3_MODULE_DIRECTORY: Record<string, DoctorModuleDirectory> = {
  core_service_lifecycle: "core",
  core_port_process_identity: "core",
  core_route_verification: "core",
  agent_discovery: "agent",
  agent_transport: "agent",
  agent_instructions: "agent",
  agent_hook_plugin: "agent",
  agent_skill: "agent",
  agent_metadata_mcp: "agent",
  agent_cc_switch_sync: "agent",
  optical_observability: "optical",
  optical_shadow_policy: "optical",
  platform_autostart: "platform",
  platform_runtime_package_identity: "platform",
  recovery_rollback_evidence: "recovery",
  recovery_specialized_transaction_rollback: "recovery",
};

type ReadinessCountEvidence = { passed: number | null; total: number | null; complete: boolean };
type ActiveConfigEvidence = { ok: boolean; summary: string };

export function resolveDoctorDestination(target?: string): PageId {
  if (target === "advanced" || target === "install") return "install";
  return "changes";
}

export function doctorFailureSummary(error: unknown, lang: "zh" | "en" = "zh"): string {
  if (error instanceof Error && error.message === "native_unavailable") {
    return translateCopy("operation.failure.nativeUnavailable.message", lang);
  }
  return translateCopy("operation.failure.generic", lang);
}

export function doctorFailureDetail(_error: unknown, _lang: "zh" | "en"): string {
  // Native diagnostic prose remains only in redacted operational evidence.
  return "";
}

export function readinessCountEvidence(result: Pick<ProductReadinessResult, "passed" | "total" | "checks">): ReadinessCountEvidence {
  const passed = finiteCountOrNull(result.passed);
  const total = finiteCountOrNull(result.total);
  const complete = passed !== null && total !== null && total > 0 && passed <= total && Array.isArray(result.checks) && result.checks.length === total;
  return { passed, total, complete };
}

export function doctorSourceFromPayload(payload: Record<string, any> | null): DoctorSource {
  const evidence = classifyEvidence({ payload, status: "runtime", requireRuntimeIdentity: true });
  if (evidence === "verified") return "native";
  if (evidence === "preview") return "preview";
  if (evidence === "stale" || evidence === "partial") return evidence;
  return "unavailable";
}

export function hasCompressionEvidence(payload: Record<string, any> | null): boolean {
  if (!Array.isArray(payload?.request_logs)) return false;
  return payload.request_logs.some((entry: any) => {
    const elapsed = entry?.compression_elapsed_us ?? entry?.compression?.elapsed_us;
    if (isNonNegativeFiniteNumber(elapsed)) return true;
    const transforms = Array.isArray(entry?.transforms_applied)
      ? entry.transforms_applied.filter((item: unknown) => typeof item === "string" && item.trim().length > 0)
      : [];
    return isNonNegativeFiniteNumber(entry?.input_tokens_before)
      && isNonNegativeFiniteNumber(entry?.input_tokens_after)
      && transforms.some((item: string) => item !== "compression_error_fallback");
  });
}

export function resolveActiveConfigEvidence(
  payload: Record<string, any> | null,
  desktopStatus: Record<string, any> | null,
  lang: "zh" | "en" = "zh",
): ActiveConfigEvidence {
  const zh = lang === "zh";
  const runtimeEnvelope = payload?.runtime_active_config ?? payload?.active_config;
  const runtimeConfig = runtimeEnvelope?.config;
  if (runtimeConfig && typeof runtimeConfig === "object") {
    const routeCount = finiteCountOrNull(runtimeConfig.route_count);
    const providerCount = finiteCountOrNull(runtimeConfig.provider_count);
    const fallbackDisabled = runtimeConfig.fallback_disabled === true;
    const source = safeStatusText(runtimeConfig.source, zh ? "来源未知" : "source unknown");
    return {
      ok: runtimeEnvelope?.source === "dtoken-owned-runtime" && runtimeConfig.loaded === true && (routeCount ?? 0) > 0 && (providerCount ?? 0) > 0 && fallbackDisabled,
      summary: zh
        ? `来源=${source} · 路由=${routeCount ?? "—"} · 上游=${providerCount ?? "—"} · 回退=${fallbackDisabled ? "已禁用" : "未证明已禁用"}`
        : `source=${source} · routes=${routeCount ?? "—"} · providers=${providerCount ?? "—"} · fallback=${fallbackDisabled ? "disabled" : "not proven disabled"}`,
    };
  }
  const state = safeStatusText(desktopStatus?.state, zh ? "未知" : "unknown");
  const source = safeStatusText(desktopStatus?.source, zh ? "未知" : "unknown");
  const routeCount = finiteCountOrNull(desktopStatus?.route_count);
  const providerCount = finiteCountOrNull(desktopStatus?.provider_count);
  const fallbackDisabled = desktopStatus?.fallback_disabled === true;
  return {
    ok: source === "d-token-desktop" && state === "loaded" && (routeCount ?? 0) > 0 && (providerCount ?? 0) > 0 && fallbackDisabled,
    summary: zh
      ? `来源=${source} · 状态=${state} · 路由=${routeCount ?? "—"} · 上游=${providerCount ?? "—"} · 回退=${fallbackDisabled ? "已禁用" : "未证明已禁用"}`
      : `source=${source} · state=${state} · routes=${routeCount ?? "—"} · providers=${providerCount ?? "—"} · fallback=${fallbackDisabled ? "disabled" : "not proven disabled"}`,
  };
}

export function Doctor({ setPage }: { setPage: (page: PageId) => void }) {
  const { lang } = useI18n();
  const zh = lang === "zh";
  const { addNotification } = useNotifications();
  const [snapshot, setSnapshot] = useState<DoctorSnapshot | null>(null);
  const [source, setSource] = useState<DoctorSource>("unavailable");
  const [loadingSnapshot, setLoadingSnapshot] = useState(true);
  const [phase, setPhase] = useState<DoctorPhase>("idle");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [plan, setPlan] = useState<DoctorRepairPlan | null>(null);
  const [v3Plan, setV3Plan] = useState<DoctorRepairPlanV3 | null>(null);
  const [v3Receipt, setV3Receipt] = useState<DoctorActionReceiptV3 | null>(null);
  const [agentDialogOpen, setAgentDialogOpen] = useState(false);
  const [agentScope, setAgentScope] = useState<"global" | "project">("global");
  const [projectRoot, setProjectRoot] = useState("");
  const [agentRepairPreset, setAgentRepairPreset] = useState<AgentRepairPreset>("hybrid");
  const [installInstructions, setInstallInstructions] = useState(true);
  const [installHookOrPlugin, setInstallHookOrPlugin] = useState(true);
  const [installSkill, setInstallSkill] = useState(true);
  const [installMcp, setInstallMcp] = useState(false);
  const [selectedAgents, setSelectedAgents] = useState<Set<string>>(new Set());
  const [notice, setNotice] = useState("");
  const [noticeTone, setNoticeTone] = useState<"success" | "warning" | "error">("warning");

  const report = isDoctorReport(snapshot) ? snapshot : null;
  const v3Snapshot = isDoctorSnapshotV3(snapshot) ? snapshot : null;
  const v3Expired = v3Snapshot ? isDoctorSnapshotExpired(v3Snapshot) : false;
  const v3Actionable = Boolean(v3Snapshot && source === "native" && !v3Expired && isV3SnapshotComplete(v3Snapshot));

  useEffect(() => {
    let active = true;
    invokeNative<unknown>("get_doctor_snapshot")
      .then((nextSnapshot) => {
        if (!active) return;
        const normalizedV3 = normalizeDoctorSnapshotV3(nextSnapshot);
        if (normalizedV3) {
          setSnapshot(normalizedV3);
          setSource(doctorV3Source(normalizedV3));
        } else if (isDoctorReport(nextSnapshot)) {
          setSnapshot(nextSnapshot);
          const nextSource = doctorV2Source(nextSnapshot);
          setSource(nextSource);
          setSelected(nextSource === "native" ? defaultRepairSelection(nextSnapshot) : new Set());
        }
      })
      .catch((error) => {
        if (!active) return;
        setSource(error instanceof Error && error.message === "native_unavailable" ? "preview" : "unavailable");
      })
      .finally(() => { if (active) setLoadingSnapshot(false); });
    return () => { active = false; };
  }, []);

  const repairableIssues = useMemo(
    () => source === "native"
      ? report?.issues.filter((issue) => issue.state === "repairable" && issue.remediation?.risk === "low") ?? []
      : [],
    [report, source],
  );

  const openAgentModule = (current: DoctorSnapshotV3, module: DoctorModuleV3, preset: AgentRepairPreset = "hybrid") => {
    if (!isV3SnapshotComplete(current) || isDoctorSnapshotExpired(current)) return;
    const target = defaultV3AgentTarget(module);
    setSelectedAgents(defaultV3AgentSelection(module, target.scope));
    setAgentScope(target.scope);
    setProjectRoot(target.projectRoot);
    setAgentRepairPreset(preset);
    setInstallInstructions(preset === "hybrid" || preset === "instructions_skill");
    setInstallHookOrPlugin(preset === "hybrid" || preset === "hook");
    setInstallSkill(preset === "hybrid" || preset === "instructions_skill");
    setInstallMcp(preset === "mcp");
    setV3Plan(null);
    setV3Receipt(null);
    setAgentDialogOpen(true);
  };

  const prepareV3Plan = async (
    current: DoctorSnapshotV3,
    module: DoctorModuleV3,
    selection: DoctorRepairSelectionV3,
    requestedModuleId: DoctorModuleId = module.module_id,
  ) => {
    if (!isV3SnapshotComplete(current) || isDoctorSnapshotExpired(current)) {
      setNotice(zh ? "诊断快照不完整或已过期，请重新扫描。" : "The diagnostic snapshot is incomplete or stale. Scan again.");
      setNoticeTone("error");
      return;
    }
    setPhase("planning");
    try {
      const nextPlan = await invokeNative<DoctorRepairPlanV3>("preview_doctor_repair", {
        request: {
          snapshot_revision: current.revision,
          module_id: requestedModuleId,
          selection,
        },
      });
      if (!isDoctorRepairPlanV3(nextPlan)
        || nextPlan.snapshot_revision !== current.revision
        || nextPlan.module_id !== requestedModuleId
        || nextPlan.expires_at_ms <= Date.now()) {
        throw new Error("stale_doctor_v3_plan");
      }
      setV3Plan(nextPlan);
      setV3Receipt(null);
    } catch {
      setV3Plan(null);
      setNotice(zh ? "无法生成当前模块的修复计划，请重新扫描后再试。" : "The module repair plan is unavailable. Scan again and retry.");
      setNoticeTone("error");
    } finally {
      setPhase("idle");
    }
  };

  const runScan = async (intent: "scan" | "repair" = "scan") => {
    const requestId = createOperationRequestId("doctor-scan");
    setPhase("scanning");
    setNotice(zh ? "正在扫描本地服务、端口、Agent、路由与安全边界…" : "Scanning local services, ports, agents, routes, and safety boundaries…");
    setNoticeTone("warning");
    try {
      const next = await invokeNative<unknown>("run_doctor_scan");
      const normalizedV3 = normalizeDoctorSnapshotV3(next);
      if (normalizedV3) {
        setSnapshot(normalizedV3);
        setSource(doctorV3Source(normalizedV3));
        setSelected(new Set());
        setPlan(null);
        setV3Plan(null);
        setV3Receipt(null);
        const complete = isV3SnapshotComplete(normalizedV3) && !isDoctorSnapshotExpired(normalizedV3);
        setNotice(complete
          ? normalizedV3.counts.repairable > 0
            ? (zh ? `扫描完成，发现 ${normalizedV3.counts.repairable} 个可准备修复的模块。` : `Scan complete. ${normalizedV3.counts.repairable} module(s) can be prepared for repair.`)
            : (zh ? "扫描完成，当前没有可自动准备的修复。" : "Scan complete. No repairs can be prepared automatically.")
          : (zh ? "扫描完成，但证据不完整或已过期；修复保持锁定。" : "Scan complete, but evidence is incomplete or stale; repairs remain locked."));
        const clean = complete
          && normalizedV3.state === "healthy"
          && normalizedV3.counts.repairable === 0
          && normalizedV3.counts.manual === 0
          && normalizedV3.counts.blocked === 0;
        setNoticeTone(clean ? "success" : "warning");
        addNotification({
          dedupeKey: `doctor-scan-${normalizedV3.revision}`,
          level: clean ? "success" : "warning",
          title: clean ? (zh ? "Doctor 扫描已通过" : "Doctor scan passed") : (zh ? "Doctor 扫描完成" : "Doctor scan complete"),
          message: zh ? `覆盖 ${normalizedV3.coverage?.completed ?? 0}/${normalizedV3.coverage?.total ?? normalizedV3.modules.length} 个模块。` : `Covered ${normalizedV3.coverage?.completed ?? 0}/${normalizedV3.coverage?.total ?? normalizedV3.modules.length} modules.`,
          receipt: { requestId, targetId: normalizedV3.revision },
        });
        if (intent === "repair" && complete) {
          const module = normalizedV3.modules.find((item) => item.state === "repairable");
          if (module?.module_id === "agent") {
            openAgentModule(normalizedV3, module);
          } else if (module) {
            await prepareV3Plan(normalizedV3, module, emptyV3Selection());
          }
        }
      } else if (isDoctorReport(next)) {
        setSnapshot(next);
        setSource("native");
        const nextSelection = defaultRepairSelection(next);
        setSelected(nextSelection);
        setNotice(next.ok
          ? (zh ? "扫描完成，当前未发现需要处理的常规问题。" : "Scan complete. No common issues need attention.")
          : (zh ? `扫描完成，发现 ${next.issues.length} 项需要处理的问题。` : `Scan complete. ${next.issues.length} issue(s) need attention.`));
        setNoticeTone(next.ok ? "success" : "warning");
        addNotification({
          dedupeKey: `doctor-scan-${next.revision}`,
          level: next.ok ? "success" : "warning",
          title: next.ok ? (zh ? "Doctor 扫描已通过" : "Doctor scan passed") : (zh ? "Doctor 发现问题" : "Doctor found issues"),
          message: next.ok ? (zh ? "常规检查全部通过。" : "All common checks passed.") : (zh ? `发现 ${next.issues.length} 项问题，其中 ${next.counts.repairable} 项可常规修复。` : `${next.issues.length} issue(s) found; ${next.counts.repairable} can be repaired safely.`),
          receipt: { requestId, targetId: next.revision },
        });
        if (intent === "repair" && nextSelection.size > 0) {
          setPhase("planning");
          const nextPlan = await invokeNative<DoctorRepairPlan>("preview_doctor_common_repair", {
            request: { report_revision: next.revision, issue_ids: [...nextSelection] },
          });
          if (!nextPlan.ok || nextPlan.report_revision !== next.revision) throw new Error("stale_doctor_repair_plan");
          setPlan(nextPlan);
        }
      } else {
        throw new Error("invalid_doctor_report");
      }
    } catch (error) {
      setSource(error instanceof Error && error.message === "native_unavailable" ? "preview" : "unavailable");
      setNotice(doctorFailureSummary(error, lang));
      setNoticeTone("error");
      addNotification({ dedupeKey: "doctor-scan-failed", level: "error", title: zh ? "Doctor 扫描失败" : "Doctor scan failed", message: doctorFailureSummary(error, lang), receipt: { requestId, targetId: "doctor" } });
    } finally {
      setPhase("idle");
    }
  };

  const previewRepair = async () => {
    if (!report || source !== "native" || selected.size === 0) return;
    const requestId = createOperationRequestId("doctor-repair-preview");
    setPhase("planning");
    try {
      const nextPlan = await invokeNative<DoctorRepairPlan>("preview_doctor_common_repair", {
        request: { report_revision: report.revision, issue_ids: [...selected] },
      });
      if (!nextPlan.ok || nextPlan.report_revision !== report.revision) throw new Error("stale_doctor_repair_plan");
      setPlan(nextPlan);
      addNotification({ dedupeKey: `doctor-repair-preview-${nextPlan.report_revision}`, level: "info", title: zh ? "修复预览已准备" : "Repair preview ready", message: zh ? "请确认修复摘要；确认前不会修改配置。" : "Confirm the repair summary; no configuration changes happen before confirmation.", receipt: { requestId, targetId: nextPlan.report_revision } });
    } catch (error) {
      const failure = zh ? "无法生成当前修复计划，请重新扫描后再试。" : "The repair plan is unavailable. Scan again and retry.";
      setNotice(failure);
      setNoticeTone("error");
      setPlan(null);
      const presentation = presentOperationFailure("doctor", error, lang);
      addNotification({ dedupeKey: "doctor-repair-preview-failed", level: "error", title: presentation.title, message: failure, details: presentation.details, receipt: { requestId, targetId: "doctor" } });
    } finally {
      setPhase("idle");
    }
  };

  const applyRepair = async () => {
    if (!report || source !== "native" || !plan) return;
    const requestId = createOperationRequestId("doctor-repair");
    setPhase("applying");
    try {
      const receipt = await invokeNative<DoctorActionReceipt>("apply_doctor_common_repair", {
        request: { report_revision: report.revision, issue_ids: plan.issue_ids, confirmed: true },
      });
      if (!isDoctorReport(receipt.report)) throw new Error("invalid_doctor_action_receipt");
      setSnapshot(receipt.report);
      setSelected(defaultRepairSelection(receipt.report));
      setPlan(null);
      const verified = receipt.ok && receipt.report.ok;
      setNotice(verified
        ? (zh ? "常规问题已修复并通过复检" : "Common issues were repaired and verified")
        : (zh ? "修复已执行，但复检仍有未通过项。" : "Repair ran, but verification still needs attention."));
      setNoticeTone(verified ? "success" : "warning");
      addNotification({ dedupeKey: `doctor-repair-${report.revision}`, level: verified ? "success" : "warning", title: verified ? (zh ? "Doctor 修复已验证" : "Doctor repair verified") : (zh ? "Doctor 修复需要处理" : "Doctor repair needs attention"), message: verified ? (zh ? "修复和复检均已完成。" : "Repair and verification completed.") : (zh ? "修复已执行，但仍有检查项需要处理。" : "Repair ran, but some checks still need attention."), receipt: { requestId, targetId: report.revision } });
    } catch (error) {
      setPlan(null);
      const failure = zh ? "修复结果未确认，请重新扫描当前状态。" : "The repair result is unconfirmed. Scan the current state again.";
      setNotice(failure);
      setNoticeTone("error");
      const presentation = presentOperationFailure("doctor", error, lang);
      addNotification({ dedupeKey: "doctor-repair-failed", level: "error", title: presentation.title, message: failure, details: presentation.details, receipt: { requestId, targetId: report.revision } });
    } finally {
      setPhase("idle");
    }
  };

  const previewAgentRepair = async () => {
    const module = v3Snapshot?.modules.find((item) => item.module_id === "agent");
    if (!v3Snapshot || !module || !v3Actionable || selectedAgents.size === 0) return;
    if (agentScope === "project" && projectRoot.trim().length === 0) {
      setNotice(zh ? "项目作用域需要填写绝对路径。" : "Project scope requires an absolute path.");
      setNoticeTone("error");
      return;
    }
    await prepareV3Plan(v3Snapshot, module, {
      agent_ids: [...selectedAgents],
      route_ids: [],
      scope: agentScope,
      project_root: agentScope === "project" ? projectRoot.trim() : null,
      install_instructions: installInstructions,
      install_hook_or_plugin: installHookOrPlugin,
      install_skill: installSkill,
      install_mcp: installMcp,
    }, agentRepairModuleId(agentRepairPreset));
  };

  const applyV3Repair = async () => {
    if (!v3Snapshot || !v3Plan || !v3Actionable || v3Plan.expires_at_ms <= Date.now()) {
      setNotice(zh ? "修复计划已过期，请重新扫描并生成计划。" : "The repair plan expired. Scan and prepare it again.");
      setNoticeTone("error");
      setV3Plan(null);
      return;
    }
    setPhase("applying");
    const requestId = createOperationRequestId("doctor-v3-repair");
    try {
      const receipt = await invokeNative<DoctorActionReceiptV3>("apply_doctor_repair", {
        request: {
          plan_id: v3Plan.plan_id,
          review_digest: v3Plan.review_digest,
          snapshot_revision: v3Plan.snapshot_revision,
          module_id: v3Plan.module_id,
          selection: v3Plan.selection,
          confirmed: true,
        },
      });
      if (!isDoctorActionReceiptV3(receipt)
        || receipt.plan_id !== v3Plan.plan_id
        || receipt.snapshot_revision !== v3Plan.snapshot_revision
        || receipt.module_id !== v3Plan.module_id
        || (receipt.plan_review_digest ?? receipt.plan_digest) !== v3Plan.review_digest) {
        throw new Error("invalid_doctor_v3_receipt");
      }
      setPhase("verifying");
      setV3Receipt(receipt);
      const normalizedReceiptSnapshot = normalizeDoctorSnapshotV3(receipt.snapshot);
      if (normalizedReceiptSnapshot) {
        setSnapshot(normalizedReceiptSnapshot);
        setSource(doctorV3Source(normalizedReceiptSnapshot));
      }
      const receiptStatus = doctorReceiptStatus(receipt);
      setNotice(receiptStatus === "verified"
        ? (zh ? "修复已完成并通过复检。" : "Repair completed and passed verification.")
        : receiptStatus === "partial"
          ? (zh ? "修复部分完成；请查看逐项结果。" : "Repair partially completed; view the per-item results.")
          : (zh ? "修复未完成；当前状态仍保持保护性锁定。" : "Repair did not complete; the current state remains safely locked."));
      setNoticeTone(receiptStatus === "verified" ? "success" : "warning");
      addNotification({ dedupeKey: `doctor-v3-repair-${v3Plan.plan_id}`, level: receiptStatus === "verified" ? "success" : "warning", title: receiptStatus === "verified" ? (zh ? "专项修复已验证" : "Specialized repair verified") : (zh ? "专项修复需要处理" : "Specialized repair needs attention"), message: receiptStatus === "verified" ? (zh ? "修复和复检均已完成。" : "Repair and verification completed.") : (zh ? "修复已执行，但请查看逐项结果。" : "Repair ran; view the per-item results."), receipt: { requestId, targetId: v3Plan.plan_id } });
    } catch (error) {
      setV3Plan(null);
      setV3Receipt(null);
      const failure = zh ? "修复结果未确认，请重新扫描当前状态。" : "The repair result is unconfirmed. Scan the current state again.";
      setNotice(failure);
      setNoticeTone("error");
      const presentation = presentOperationFailure("doctor", error, lang);
      addNotification({ dedupeKey: "doctor-v3-repair-failed", level: "error", title: presentation.title, message: failure, details: presentation.details, receipt: { requestId, targetId: v3Plan.plan_id } });
    } finally {
      setPhase("idle");
    }
  };

  const sourceLabel = v3Snapshot
    ? doctorV3EvidenceLabel(v3Snapshot, lang)
    : source === "native"
      ? (zh ? "本地诊断快照" : "Local diagnostic snapshot")
    : source === "stale"
      ? (zh ? "诊断快照已过期" : "Diagnostic snapshot is stale")
    : source === "preview"
      ? (zh ? "浏览器预览" : "Browser preview")
      : (zh ? "诊断数据不可用" : "Diagnostic data unavailable");
  const busy = phase !== "idle";
  const phaseLabel = doctorPhaseLabel(phase, Boolean(snapshot), lang);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-3">
            <h1>{zh ? "Doctor" : "Doctor"}</h1>
            <DataSourceBadge mode={source === "native" ? "native" : source === "preview" ? "preview" : "unavailable"} label={sourceLabel} />
          </div>
          <p className="mt-2 max-w-2xl text-sm text-muted-foreground">
            {zh ? "主动扫描常见的本地服务、连接与配置问题；扫描只读取本机状态，不调用 Provider，也不会自动修改外部 Agent 配置。" : "Scan common local service, connection, and configuration issues. Scans are local and read-only, with no Provider calls or silent Agent changes."}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => void runScan("repair")} disabled={busy || loadingSnapshot} aria-busy={phase === "scanning"}>
            {phase === "scanning" ? <RefreshCw className="size-4 animate-spin" aria-hidden /> : <Wrench className="size-4" />}
            {phase === "scanning" ? (zh ? "扫描中…" : "Scanning…") : (zh ? "自动扫描并修复" : "Scan & prepare repairs")}
          </Button>
          <Button
            variant="outline"
            onClick={() => void runScan("scan")}
            disabled={busy || loadingSnapshot}
            aria-label={zh ? "扫描常规问题" : "Scan common issues"}
          >
            <RefreshCw className={`size-4 ${phase === "scanning" ? "animate-spin" : ""}`} />
            {zh ? "仅扫描" : "Scan only"}
          </Button>
        </div>
      </div>

      <Surface className="p-4" aria-live="polite">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-sm font-medium">
            <HardDrive className="size-4 text-muted-foreground" aria-hidden />
            <span>{phaseLabel}</span>
          </div>
          <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
            {[zh ? "扫描" : "Scan", zh ? "计划" : "Plan", zh ? "确认" : "Confirm", zh ? "执行" : "Apply", zh ? "复检" : "Verify"].map((item) => (
              <span key={item} className="rounded-full border border-border/70 px-2 py-1">{item}</span>
            ))}
          </div>
        </div>
      </Surface>

      {notice && (
        <div role={noticeTone === "error" ? "alert" : "status"} aria-live={noticeTone === "error" ? "assertive" : "polite"} className={`rounded-lg border px-4 py-3 text-sm ${noticeTone === "success" ? "border-emerald-500/25 bg-emerald-500/[0.06] text-emerald-700 dark:text-emerald-300" : noticeTone === "error" ? "border-rose-500/25 bg-rose-500/[0.06] text-rose-700 dark:text-rose-300" : "border-amber-500/25 bg-amber-500/[0.06] text-amber-700 dark:text-amber-300"}`}>
          {notice}
        </div>
      )}

      {!snapshot && !loadingSnapshot && (
        <Surface className="p-8 text-center">
          <Stethoscope className="mx-auto size-8 text-muted-foreground" />
          <h2 className="mt-3 text-base">{zh ? "尚未扫描常规问题" : "Common issues have not been scanned"}</h2>
          <p className="mt-1 text-sm text-muted-foreground">{zh ? "点击“仅扫描”生成一份统一、本地、可复验的诊断快照。" : "Choose Scan only to create one unified, local, verifiable diagnostic snapshot."}</p>
        </Surface>
      )}

      {report && source !== "native" && (
        <Surface className="border-amber-500/25 bg-amber-500/[0.04] p-5">
          <div className="flex items-start gap-3">
            <AlertTriangle className="mt-0.5 size-5 shrink-0 text-amber-500" aria-hidden />
            <div>
              <h2 className="text-base">{zh ? "旧诊断快照不可用于修复" : "Legacy diagnostic snapshot cannot be used for repair"}</h2>
              <p className="mt-1 text-sm text-muted-foreground">
                {zh ? "该 Doctor v2 快照已过期或时间证据无效。请重新扫描生成 v3 快照；旧快照中的字面量 fresh 不会被信任。" : "This Doctor v2 snapshot is stale or has invalid time evidence. Scan again for a v3 snapshot; a persisted literal fresh value is not trusted."}
              </p>
            </div>
          </div>
        </Surface>
      )}

      {v3Snapshot && (
        <Surface className="p-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <div className="flex items-center gap-2">
                {v3Snapshot.counts.repairable === 0 && v3Actionable
                  ? <CheckCircle2 className="size-5 text-emerald-500" />
                  : <AlertTriangle className="size-5 text-amber-500" />}
                <h2 className="text-base">{zh ? "统一诊断快照" : "Unified diagnostic snapshot"}</h2>
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                {formatTimestampMs(v3Snapshot.collected_at_ms)} · revision {v3Snapshot.revision.slice(0, 12)}
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              <Badge variant="outline">
                {zh ? "覆盖" : "Coverage"} {v3Snapshot.coverage?.completed ?? 0}/{v3Snapshot.coverage?.total ?? v3Snapshot.modules.length}
              </Badge>
              <Badge variant={v3Actionable ? "secondary" : "outline"}>{v3Expired ? (zh ? "已过期" : "Stale") : (zh ? "新鲜" : "Fresh")}</Badge>
              <Badge variant="secondary">{zh ? "可修复" : "Repairable"} {v3Snapshot.counts.repairable}</Badge>
              <Badge variant="outline">{zh ? "需手动" : "Manual"} {v3Snapshot.counts.manual}</Badge>
              <Badge variant="outline">{zh ? "已阻止" : "Blocked"} {v3Snapshot.counts.blocked}</Badge>
            </div>
          </div>
          {!v3Actionable ? (
            <p className="mt-3 text-sm text-amber-700 dark:text-amber-300">
              {zh ? "证据不完整或已过期；所有修复动作已锁定，只允许重新扫描。" : "Evidence is incomplete or stale. All repair actions are locked; only rescanning is allowed."}
            </p>
          ) : null}
        </Surface>
      )}

      <Surface className="overflow-hidden p-0">
        <div className="border-b border-border/70 px-5 py-4">
          <h2 className="text-base font-medium">{zh ? "预制诊断任务" : "Diagnostic task modules"}</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            {zh ? "按模块检查和准备修复；任何写入仍需先查看逐项 diff 并明确确认。" : "Inspect and prepare repairs by module. Every write still requires per-item diffs and explicit confirmation."}
          </p>
        </div>
        <div className="divide-y divide-border/60">
          {MODULE_PRESETS.map((preset) => {
            const module = v3Snapshot?.modules.find((item) => item.module_id === preset.id);
            const copy = preset[lang];
            const Icon = preset.icon;
            const canPrepare = Boolean(module && module.state === "repairable" && v3Actionable);
            const actionLabel = preset.id === "agent"
              ? (zh ? "配置 Agent 修复" : "Configure Agent repair")
              : canPrepare
                ? (zh ? "准备修复" : "Prepare repair")
                : preset.id === "optical"
                  ? (zh ? "前往视觉压缩" : "Open Visual Compression")
                  : preset.id === "recovery"
                    ? (zh ? "查看备份与回退" : "Open backups & rollback")
                    : preset.id === "platform"
                      ? (zh ? "前往安装与连接" : "Open Install & Connect")
                      : (zh ? "重新检查" : "Check again");
            const actionDisabled = preset.id === "agent" && !canPrepare;
            return (
              <div key={preset.id} className="grid gap-3 px-5 py-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
                <div className="flex min-w-0 gap-3">
                  <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg border border-border/70 bg-muted/30">
                    <Icon className="size-4 text-muted-foreground" aria-hidden />
                  </span>
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <h3 className="text-sm font-medium">{copy.title}</h3>
                      <StatusBadge status={moduleStatusTone(module?.state)} label={moduleStateLabel(module?.state, lang)} />
                    </div>
                    <p className="mt-1 text-xs leading-5 text-muted-foreground">{module?.summary || copy.detail}</p>
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={busy || actionDisabled}
                  onClick={() => {
                    if (preset.id === "agent" && v3Snapshot && module) openAgentModule(v3Snapshot, module);
                    else if (canPrepare && v3Snapshot && module) void prepareV3Plan(v3Snapshot, module, emptyV3Selection());
                    else if (preset.id === "optical") setPage("visual");
                    else if (preset.id === "recovery") setPage("changes");
                    else if (preset.id === "platform") setPage("install");
                    else void runScan("scan");
                  }}
                >
                  {actionLabel}<ChevronRight className="size-3.5" aria-hidden />
                </Button>
              </div>
            );
          })}
        </div>
      </Surface>

      <Surface className="overflow-hidden p-0">
        <div className="border-b border-border/70 px-5 py-4">
          <h2 className="text-base font-medium">{zh ? "专项修复工具" : "Specialized repair tools"}</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            {zh ? "每个工具都复用其来源事务执行器；Agent 类工具会先让你选择 Agent 和作用域。" : "Each tool reuses its source transaction executor. Agent tools ask for Agents and scope first."}
          </p>
        </div>
        <div className="divide-y divide-border/60">
          {SPECIALIZED_REPAIR_PRESETS.map((item) => {
            const module = v3Snapshot?.modules.find((candidate) => candidate.module_id === item.source);
            const Icon = item.icon;
            const agentAvailable = item.action === "agent" && Boolean(module?.agents?.some((agent) => agent.detected !== false && agent.supported !== false && managedAgentScopes(agent).length > 0));
            const disabled = busy || !v3Actionable || (item.action === "agent" && !agentAvailable);
            return (
              <div key={item.id} className="grid gap-3 px-5 py-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
                <div className="flex min-w-0 gap-3">
                  <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg border border-border/70 bg-muted/30">
                    <Icon className="size-4 text-muted-foreground" aria-hidden />
                  </span>
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <h3 className="text-sm font-medium">{item.title}</h3>
                      <StatusBadge status={moduleStatusTone(module?.state)} label={moduleStateLabel(module?.state, lang)} />
                    </div>
                    <p className="mt-1 text-xs leading-5 text-muted-foreground">{item.detail}</p>
                  </div>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={disabled}
                  onClick={() => {
                    if (!v3Snapshot || !module) return;
                    if (item.action === "agent") openAgentModule(v3Snapshot, module, item.preset);
                    else if (item.action === "core") void prepareV3Plan(v3Snapshot, module, emptyV3Selection(), "core");
                    else if (item.action === "visual") setPage("visual");
                    else if (item.action === "changes") setPage("changes");
                    else if (item.action === "install") setPage("install");
                    else void runScan("scan");
                  }}
                >
                  {item.action === "agent" ? (zh ? "选择 Agent" : "Select Agents") : item.action === "scan" ? (zh ? "重新检查" : "Check again") : item.action === "core" ? (zh ? "准备修复" : "Prepare repair") : (zh ? "打开" : "Open")}
                  <ChevronRight className="size-3.5" aria-hidden />
                </Button>
              </div>
            );
          })}
        </div>
      </Surface>

      {report && source === "native" && (
        <>
          <Surface className="p-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  {report.ok ? <CheckCircle2 className="size-5 text-emerald-500" /> : <AlertTriangle className="size-5 text-amber-500" />}
                  <h2 className="text-base">{report.ok ? (zh ? "常规检查已通过" : "Common checks passed") : (zh ? "发现需要处理的问题" : "Issues need attention")}</h2>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">{zh ? "快照时间" : "Snapshot"}：{formatTimestamp(report.collected_at)} · revision {report.revision.slice(0, 12)}</p>
              </div>
              <div className="flex flex-wrap gap-2">
                <Badge variant="secondary">{zh ? "可修复" : "Repairable"} {report.counts.repairable}</Badge>
                <Badge variant="outline">{zh ? "需手动" : "Manual"} {report.counts.manual}</Badge>
                <Badge variant="outline">{zh ? "已阻止" : "Blocked"} {report.counts.blocked}</Badge>
              </div>
            </div>
          </Surface>

          {report.issues.length === 0 ? (
            <Surface className="p-6 text-sm text-muted-foreground">{zh ? "当前未发现需要处理的常规问题。" : "No common issues currently need attention."}</Surface>
          ) : (
            <div className="space-y-4">
              {LAYERS.map((category) => {
                const issues = report.issues.filter((issue) => issue.category === category);
                if (issues.length === 0) return null;
                return (
                  <Surface key={category} className="p-5">
                    <h2 className="mb-3 text-sm font-medium">{layerLabel(category, lang)}</h2>
                    <div className="divide-y divide-border/60">
                      {issues.map((issue) => {
                        const selectable = issue.state === "repairable" && issue.remediation?.risk === "low";
                        return (
                          <div key={issue.id} className="grid gap-3 py-4 first:pt-0 last:pb-0 md:grid-cols-[minmax(0,1fr)_auto]">
                            <div className="flex min-w-0 gap-3">
                              {selectable ? (
                                <input
                                  type="checkbox"
                                  className="mt-1 size-4 accent-primary"
                                  aria-label={`${zh ? "选择" : "Select"} ${issue.title}`}
                                  checked={selected.has(issue.id)}
                                  onChange={(event) => setSelected((current) => {
                                    const next = new Set(current);
                                    if (event.target.checked) next.add(issue.id); else next.delete(issue.id);
                                    return next;
                                  })}
                                />
                              ) : <ShieldCheck className="mt-0.5 size-4 shrink-0 text-muted-foreground" />}
                              <div className="min-w-0">
                                <div className="flex flex-wrap items-center gap-2">
                                  <h3 className="text-sm font-medium">{issue.title}</h3>
                                  <StatusBadge status={issue.severity === "error" ? "error" : "warning"} label={issue.state === "repairable" ? (zh ? "可常规修复" : "Repairable") : issue.state === "blocked" ? (zh ? "已保护性阻止" : "Blocked safely") : (zh ? "需手动处理" : "Manual action")} />
                                </div>
                                <dl className="mt-2 grid gap-2 text-xs leading-5 text-muted-foreground">
                                  <div><dt className="inline font-medium text-foreground">{zh ? "影响" : "Impact"}：</dt><dd className="inline">{issue.impact}</dd></div>
                                  <div><dt className="inline font-medium text-foreground">{zh ? "原因" : "Cause"}：</dt><dd className="inline">{issue.cause}</dd></div>
                                  <div><dt className="inline font-medium text-foreground">{zh ? "证据" : "Evidence"}：</dt><dd className="inline break-words">{issue.evidence}</dd></div>
                                </dl>
                              </div>
                            </div>
                            {issue.state === "manual" && issue.remediation?.destination ? (
                              <Button size="sm" variant="outline" onClick={() => setPage(issue.remediation!.destination!)}>
                                {issue.remediation.label}<ChevronRight className="size-3.5" />
                              </Button>
                            ) : null}
                          </div>
                        );
                      })}
                    </div>
                  </Surface>
                );
              })}
            </div>
          )}

          {repairableIssues.length > 0 && (
            <div className="sticky bottom-4 z-10 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border/80 bg-background/95 p-4 shadow-lg backdrop-blur">
              <div className="text-sm"><strong>{selected.size}</strong> {zh ? "项低风险常规修复已选择；每次修改都需要再次确认。" : "low-risk repair(s) selected; changes always require confirmation."}</div>
              <Button onClick={() => void previewRepair()} disabled={busy || selected.size === 0}>
                <Wrench className="size-4" />{phase === "planning" ? (zh ? "正在生成计划" : "Preparing plan") : (zh ? "修复所选问题" : "Repair selected")}
              </Button>
            </div>
          )}
        </>
      )}

      <OperationDialog
        open={plan !== null}
        onOpenChange={(open) => { if (!open && phase !== "applying") setPlan(null); }}
        title={zh ? "确认常规修复" : "Confirm common repairs"}
        summary={zh ? "只会启动或重启身份已验证的 d-token 自有服务，并在操作后自动重新扫描。不会停止外部进程，也不会改写 Agent、Hook、Provider 或 cc-switch 配置。" : "Only verified d-token-owned services may be started or restarted, followed by an automatic rescan. External processes and Agent, Hook, Provider, or cc-switch configuration remain untouched."}
        confirmLabel={zh ? "确认修复并复检" : "Repair and verify"}
        cancelLabel={zh ? "取消" : "Cancel"}
        risk="warning"
        pending={phase === "applying"}
        onConfirm={() => void applyRepair()}
        details={plan ? <div className="rounded-lg border border-border bg-muted/30 p-3 text-sm text-muted-foreground">{zh ? `将处理 ${plan.issue_ids.length} 项问题，风险等级：${plan.risk}。` : `${plan.issue_ids.length} issue(s), risk: ${plan.risk}.`}</div> : null}
      />

      <OperationDialog
        open={Boolean(v3Plan && !isAgentRepairModule(v3Plan.module_id))}
        onOpenChange={(open) => {
          if (!open && phase !== "applying") {
            setV3Plan(null);
            setV3Receipt(null);
          }
        }}
        title={zh ? "确认模块修复" : "Confirm module repair"}
        summary={v3Plan?.summary ?? (zh ? "请查看当前计划。" : "View the current plan.")}
        impact={v3Plan ? `${moduleTitle(v3Plan.module_id, lang)} · ${v3Plan.risk}` : undefined}
        confirmLabel={zh ? "确认、执行并复检" : "Confirm, apply & verify"}
        cancelLabel={zh ? "取消" : "Cancel"}
        risk="warning"
        pending={phase === "applying" || phase === "verifying"}
        confirmDisabled={!v3Plan || Boolean(v3Receipt)}
        onConfirm={() => void applyV3Repair()}
        details={v3Plan ? <V3PlanDetails plan={v3Plan} lang={lang} receipt={v3Receipt} /> : null}
      />

      <Dialog
        open={agentDialogOpen}
        onOpenChange={(open) => {
          if (phase === "planning" || phase === "applying" || phase === "verifying") return;
          setAgentDialogOpen(open);
          if (!open) {
            setV3Plan(null);
            setV3Receipt(null);
          }
        }}
      >
        <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle>{zh ? "Agent 接入诊断与修复" : "Agent integration diagnosis & repair"}</DialogTitle>
            <DialogDescription>
              {zh
                ? "选择 Agent 与作用域后先生成逐 Agent 差异。只有当前内容与计划摘要仍一致时，确认按钮才可执行写入。"
                : "Select Agents and scope, then generate per-Agent differences. Writes are enabled only while the displayed content still matches the plan digest."}
            </DialogDescription>
          </DialogHeader>

          {v3Receipt ? (
            <div className="space-y-4">
              <div className="rounded-xl border border-border/70 bg-muted/25 p-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <h3 className="text-sm font-medium">{zh ? "逐 Agent 结果" : "Per-Agent results"}</h3>
                  <Badge variant={doctorReceiptStatus(v3Receipt) === "verified" ? "default" : "secondary"}>
                    {receiptOutcomeLabel(doctorReceiptStatus(v3Receipt), lang)}
                  </Badge>
                </div>
                <div className="mt-3 divide-y divide-border/60">
                  {(v3Receipt.agents ?? []).map((agent) => (
                    <div key={agent.agent_id} className="py-3 text-sm first:pt-0 last:pb-0">
                      <div className="font-medium">{agent.name || agent.agent_id} · {receiptOutcomeLabel(agent.outcome, lang)}</div>
                      {agent.summary ? <p className="mt-1 text-xs text-muted-foreground">{agent.summary}</p> : null}
                    </div>
                  ))}
                </div>
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={() => setAgentDialogOpen(false)}>{zh ? "关闭" : "Close"}</Button>
                <Button onClick={() => void runScan("scan")} disabled={busy} aria-busy={phase === "scanning"}>
                  {phase === "scanning" ? <RefreshCw className="size-4 animate-spin" aria-hidden /> : null}
                  {phase === "scanning" ? (zh ? "扫描中…" : "Scanning…") : (zh ? "重新扫描" : "Scan again")}
                </Button>
              </DialogFooter>
            </div>
          ) : v3Plan ? (
            <div className="space-y-4">
              <V3PlanDetails plan={v3Plan} lang={lang} receipt={null} />
              <div className="rounded-lg border border-amber-500/25 bg-amber-500/[0.06] p-3 text-xs text-amber-800 dark:text-amber-200">
                {zh
                  ? "确认后只应用上方显示的差异；Provider 凭据不会被修改，元数据诊断 MCP 仍按你的选择保持默认关闭。"
                  : "Confirmation applies only the differences shown above. Provider credentials remain unchanged, and metadata diagnostics MCP stays off unless explicitly selected."}
              </div>
              <DialogFooter>
                <Button variant="outline" disabled={busy} onClick={() => setV3Plan(null)}>{zh ? "返回选择" : "Back"}</Button>
                <Button disabled={busy || !agentPlanDiffsComplete(v3Plan)} onClick={() => void applyV3Repair()}>
                  {phase === "applying" || phase === "verifying"
                    ? (zh ? "正在应用并复检…" : "Applying & verifying…")
                    : (zh ? "确认、应用并验证" : "Confirm, apply & verify")}
                </Button>
              </DialogFooter>
            </div>
          ) : (
            <div className="space-y-4">
              <div className="grid gap-2 sm:grid-cols-3">
                {(v3Snapshot?.modules.find((item) => item.module_id === "agent")?.agents ?? []).map((agent) => {
                  const selectable = agent.detected !== false && agent.supported !== false && managedAgentScopes(agent).includes(agentScope);
                  return (
                    <label key={agent.id} className="flex items-center justify-between gap-3 rounded-lg border border-border/70 px-3 py-2 text-sm">
                      <span>
                        <span className="block font-medium">{agent.name}</span>
                        <span className="block text-[11px] text-muted-foreground">
                          {selectable ? (zh ? "已有受管目标 · 可配置" : "Managed target · configurable") : (zh ? "当前作用域无受管目标" : "No managed target in this scope")}
                        </span>
                      </span>
                      <input
                        type="checkbox"
                        className="size-4 accent-primary"
                        aria-label={`${zh ? "选择" : "Select"} ${agent.name}`}
                        checked={selectedAgents.has(agent.id)}
                        disabled={!selectable || busy}
                        onChange={(event) => {
                          setSelectedAgents((current) => {
                            const next = new Set(current);
                            if (event.target.checked) next.add(agent.id); else next.delete(agent.id);
                            return next;
                          });
                          setV3Plan(null);
                          setV3Receipt(null);
                        }}
                      />
                    </label>
                  );
                })}
              </div>

              <div className="grid gap-3 sm:grid-cols-[180px_1fr]">
                <label className="space-y-1 text-sm">
                  <span className="text-xs text-muted-foreground">{zh ? "作用域" : "Scope"}</span>
                  <select
                    aria-label={zh ? "作用域" : "Scope"}
                    className="h-9 w-full rounded-md border border-border bg-background px-2 text-sm"
                    value={agentScope}
                    disabled={busy}
                    onChange={(event) => {
                      const nextScope = event.target.value === "project" ? "project" : "global";
                      const agentModule = v3Snapshot?.modules.find((item) => item.module_id === "agent");
                      setAgentScope(nextScope);
                      if (agentModule) {
                        setSelectedAgents(defaultV3AgentSelection(agentModule, nextScope));
                        if (nextScope === "project") setProjectRoot(defaultV3ProjectRoot(agentModule));
                      }
                      setV3Plan(null);
                    }}
                  >
                    <option value="global">{zh ? "全局" : "Global"}</option>
                    <option value="project">{zh ? "当前项目" : "Current project"}</option>
                  </select>
                </label>
                {agentScope === "project" ? (
                  <label className="space-y-1 text-sm">
                    <span className="text-xs text-muted-foreground">{zh ? "项目绝对路径" : "Absolute project path"}</span>
                    <input
                      aria-label={zh ? "项目绝对路径" : "Absolute project path"}
                      className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
                      value={projectRoot}
                      disabled={busy}
                      placeholder="/path/to/project"
                      onChange={(event) => {
                        setProjectRoot(event.target.value);
                        setV3Plan(null);
                      }}
                    />
                  </label>
                ) : (
                  <p className="self-end pb-2 text-xs text-muted-foreground">
                    {zh ? "全局目标仍会逐 Agent 展示 diff。" : "Global targets still show per-Agent diffs."}
                  </p>
                )}
              </div>

              <div className="grid gap-2 sm:grid-cols-3">
                <SurfaceToggle label={zh ? "Instructions" : "Instructions"} checked={installInstructions} disabled={busy} onChange={(checked) => { setInstallInstructions(checked); setV3Plan(null); }} />
                <SurfaceToggle label={zh ? "Hook / Plugin" : "Hook / Plugin"} checked={installHookOrPlugin} disabled={busy} onChange={(checked) => { setInstallHookOrPlugin(checked); setV3Plan(null); }} />
                <SurfaceToggle label={zh ? "Recovery Skill" : "Recovery Skill"} checked={installSkill} disabled={busy} onChange={(checked) => { setInstallSkill(checked); setV3Plan(null); }} />
              </div>

              <label className="flex items-center justify-between gap-3 rounded-lg border border-border/70 px-3 py-2 text-sm">
                <span>
                  <span className="block font-medium">{zh ? "元数据诊断 MCP（默认关闭）" : "Metadata diagnostics MCP (off by default)"}</span>
                  <span className="block text-[11px] text-muted-foreground">{zh ? "只提供状态与恢复诊断，不参与真实压缩。" : "Provides status and recovery diagnostics only; never performs compression."}</span>
                </span>
                <input
                  type="checkbox"
                  className="size-4 accent-primary"
                  aria-label={zh ? "元数据诊断 MCP（默认关闭）" : "Metadata diagnostics MCP (off by default)"}
                  checked={installMcp}
                  disabled={busy}
                  onChange={(event) => {
                    setInstallMcp(event.target.checked);
                    setV3Plan(null);
                  }}
                />
              </label>

              <DialogFooter>
                <Button variant="outline" disabled={busy} onClick={() => setAgentDialogOpen(false)}>{zh ? "取消" : "Cancel"}</Button>
                <Button disabled={busy || selectedAgents.size === 0 || (!installInstructions && !installHookOrPlugin && !installSkill && !installMcp) || (agentScope === "project" && projectRoot.trim().length === 0)} onClick={() => void previewAgentRepair()}>
                  {phase === "planning" ? (zh ? "正在生成计划…" : "Preparing plan…") : (zh ? "生成修复计划" : "Prepare repair plan")}
                </Button>
              </DialogFooter>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

function SurfaceToggle({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-3 rounded-lg border border-border/70 px-3 py-2 text-sm">
      <span className="font-medium">{label}</span>
      <input
        type="checkbox"
        className="size-4 accent-primary"
        aria-label={label}
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
    </label>
  );
}

function V3PlanDetails({
  plan,
  lang,
  receipt,
}: {
  plan: DoctorRepairPlanV3;
  lang: "zh" | "en";
  receipt: DoctorActionReceiptV3 | null;
}) {
  const targets = [
    ...(plan.agents ?? []).flatMap((agent) => (agent.targets ?? []).map((target) => ({
      ...target,
      owner: agent.name || agent.agent_id,
    }))),
    ...(plan.actions ?? []).flatMap((action) => (action.targets ?? []).map((target) => ({
      ...target,
      owner: action.summary,
    }))),
  ];
  return (
    <div className="max-h-[48vh] space-y-3 overflow-auto pr-1 text-xs text-muted-foreground">
      <div className="rounded-lg border border-border/70 bg-muted/20 p-3">
        <div>{lang === "zh" ? "计划" : "Plan"}：{plan.plan_id}</div>
        <div className="mt-1">{lang === "zh" ? "变更摘要" : "Change digest"} {shortHashV3(plan.review_digest)} · {lang === "zh" ? "有效期至" : "expires"} {formatTimestampMs(plan.expires_at_ms)}</div>
      </div>
      {(plan.agents ?? []).map((agent) => (
        <div key={agent.agent_id} className="rounded-lg border border-border/70 p-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="font-medium text-foreground">{agent.name || agent.agent_id}</span>
            <span>{agent.state}</span>
          </div>
          {agent.summary ? <p className="mt-1">{agent.summary}</p> : null}
          {(agent.targets ?? []).map((target) => <V3TargetDiff key={`${agent.agent_id}:${target.target_path}`} target={target} />)}
        </div>
      ))}
      {(plan.actions ?? []).map((action) => (
        <div key={action.action_id} className="rounded-lg border border-border/70 p-3">
          <div className="font-medium text-foreground">{action.summary}</div>
          {(action.targets ?? []).map((target) => <V3TargetDiff key={`${action.action_id}:${target.target_path}`} target={target} />)}
        </div>
      ))}
      {targets.length === 0 ? (
        <div className="rounded-lg border border-border/70 p-3">
          {lang === "zh" ? "该计划不写配置文件；执行前仍会重新校验快照与运行身份。" : "This plan does not write config files; snapshot and runtime identity are still revalidated before execution."}
        </div>
      ) : null}
      {receipt ? (
        <div className="rounded-lg border border-border/70 bg-muted/20 p-3 font-medium text-foreground">
          {lang === "zh" ? "结果" : "Result"}：{receiptOutcomeLabel(doctorReceiptStatus(receipt), lang)}
        </div>
      ) : null}
    </div>
  );
}

function V3TargetDiff({ target }: { target: DoctorRepairTargetV3 }) {
  return (
    <div className="mt-2 rounded-md bg-background p-2">
      <div className="break-all">{target.target_path}</div>
      <div className="mt-1">{shortHashV3(target.before_sha256)} → {shortHashV3(target.after_sha256)}</div>
      <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap rounded border border-border/70 p-2 font-mono text-[11px]">
        {target.diff || "—"}
      </pre>
    </div>
  );
}

function isDoctorSnapshotV3(value: unknown): value is DoctorSnapshotV3 {
  if (!value || typeof value !== "object") return false;
  const snapshot = value as Partial<DoctorSnapshotV3>;
  return snapshot.schema_version === 3
    && typeof snapshot.revision === "string"
    && snapshot.revision.length > 0
    && typeof snapshot.source === "string"
    && typeof snapshot.state === "string"
    && typeof snapshot.collected_at_ms === "number"
    && Number.isFinite(snapshot.collected_at_ms)
    && typeof snapshot.ttl_ms === "number"
    && snapshot.ttl_ms >= 0
    && Array.isArray(snapshot.modules)
    && Boolean(snapshot.counts);
}

function normalizeDoctorSnapshotV3(value: unknown): DoctorSnapshotV3 | null {
  if (!value || typeof value !== "object") return null;
  const snapshot = value as Record<string, unknown>;
  if (snapshot.schema_version !== 3
    || typeof snapshot.revision !== "string"
    || snapshot.revision.length === 0
    || typeof snapshot.source !== "string"
    || !isSafeNonNegativeInteger(snapshot.collected_at_ms)
    || !isSafeNonNegativeInteger(snapshot.ttl_ms)
    || !Array.isArray(snapshot.modules)) {
    return null;
  }

  const state = typeof snapshot.state === "string"
    ? snapshot.state
    : typeof snapshot.overall_state === "string"
      ? snapshot.overall_state
      : "unknown";
  const rawModules = snapshot.modules.filter(isDoctorModuleV3Shape);
  if (rawModules.length !== snapshot.modules.length) return null;
  const usesDomainModules = rawModules.length > 0
    && rawModules.every((module) => Boolean(DOCTOR_V3_MODULE_DIRECTORY[module.module_id]));
  const usesProductModules = rawModules.every((module) => MODULE_PRESETS.some((preset) => preset.id === module.module_id));
  if (!usesDomainModules && !usesProductModules) return null;
  const modules = usesDomainModules ? aggregateDoctorDomainModules(rawModules) : rawModules;
  const coverage = normalizeDoctorCoverage(snapshot.coverage, rawModules, usesDomainModules);
  const counts = normalizeDoctorCounts(snapshot.counts) ?? deriveDoctorCounts(modules);

  return {
    schema_version: 3,
    revision: snapshot.revision,
    source: snapshot.source,
    state,
    overall_state: typeof snapshot.overall_state === "string" ? snapshot.overall_state : undefined,
    collected_at_ms: Number(snapshot.collected_at_ms),
    ttl_ms: Number(snapshot.ttl_ms),
    coverage,
    counts,
    modules,
  };
}

function isDoctorModuleV3Shape(value: unknown): value is DoctorModuleV3 {
  if (!value || typeof value !== "object") return false;
  const module = value as Partial<DoctorModuleV3>;
  return typeof module.module_id === "string" && typeof module.state === "string";
}

function aggregateDoctorDomainModules(modules: DoctorModuleV3[]): DoctorModuleV3[] {
  return MODULE_PRESETS.map((preset) => {
    const children = modules.filter((module) => DOCTOR_V3_MODULE_DIRECTORY[module.module_id] === preset.id);
    const state = aggregateDoctorModuleState(children.map((module) => module.state));
    return {
      module_id: preset.id,
      state,
      severity: state === "blocked" ? "error" : state === "healthy" ? "info" : "warning",
      reason_codes: children.flatMap((module) => module.reason_codes ?? []),
      agents: children.flatMap((module) => module.agents ?? []),
    };
  });
}

function aggregateDoctorModuleState(states: string[]): string {
  if (states.length === 0) return "unavailable";
  for (const candidate of ["unavailable", "unknown", "blocked", "attention", "healthy"]) {
    if (states.includes(candidate)) return candidate;
  }
  return "unknown";
}

function normalizeDoctorCoverage(
  value: unknown,
  modules: DoctorModuleV3[],
  usesDomainModules: boolean,
): DoctorSnapshotV3["coverage"] {
  if (value && typeof value === "object") {
    const coverage = value as NonNullable<DoctorSnapshotV3["coverage"]>;
    return {
      completed: finiteCountOrNull(coverage.completed) ?? 0,
      total: finiteCountOrNull(coverage.total) ?? (usesDomainModules ? Object.keys(DOCTOR_V3_MODULE_DIRECTORY).length : modules.length),
      agents_detected: finiteCountOrNull(coverage.agents_detected) ?? undefined,
      agents_selected: finiteCountOrNull(coverage.agents_selected) ?? undefined,
    };
  }
  return {
    completed: modules.filter((module) => !["unavailable", "unknown"].includes(module.state)).length,
    total: usesDomainModules ? Object.keys(DOCTOR_V3_MODULE_DIRECTORY).length : modules.length,
  };
}

function normalizeDoctorCounts(value: unknown): DoctorSnapshotV3["counts"] | null {
  if (!value || typeof value !== "object") return null;
  const counts = value as Record<string, unknown>;
  const healthy = finiteCountOrNull(counts.healthy);
  const repairable = finiteCountOrNull(counts.repairable);
  const manual = finiteCountOrNull(counts.manual);
  const blocked = finiteCountOrNull(counts.blocked);
  if (healthy === null || repairable === null || manual === null || blocked === null) return null;
  return { healthy, repairable, manual, blocked };
}

function deriveDoctorCounts(modules: DoctorModuleV3[]): DoctorSnapshotV3["counts"] {
  return modules.reduce((counts, module) => {
    if (module.state === "healthy") counts.healthy += 1;
    else if (module.state === "repairable") counts.repairable += 1;
    else if (module.state === "blocked") counts.blocked += 1;
    else counts.manual += 1;
    return counts;
  }, { healthy: 0, repairable: 0, manual: 0, blocked: 0 });
}

function isDoctorRepairPlanV3(value: unknown): value is DoctorRepairPlanV3 {
  if (!value || typeof value !== "object") return false;
  const plan = value as Partial<DoctorRepairPlanV3>;
  return plan.schema_version === 3
    && typeof plan.plan_id === "string"
    && plan.plan_id.length > 0
    && typeof plan.snapshot_revision === "string"
    && typeof plan.module_id === "string"
    && typeof plan.review_digest === "string"
    && plan.review_digest.length > 0
    && isSafeNonNegativeInteger(plan.created_at_ms)
    && isSafeNonNegativeInteger(plan.expires_at_ms)
    && plan.expires_at_ms > plan.created_at_ms
    && isDoctorRepairSelectionV3(plan.selection);
}

function isDoctorRepairSelectionV3(value: unknown): value is DoctorRepairSelectionV3 {
  if (!value || typeof value !== "object") return false;
  const selection = value as Partial<DoctorRepairSelectionV3>;
  return (selection.scope === "global" || selection.scope === "project")
    && Array.isArray(selection.agent_ids)
    && selection.agent_ids.every((item) => typeof item === "string")
    && Array.isArray(selection.route_ids)
    && selection.route_ids.every((item) => typeof item === "string");
}

function isDoctorActionReceiptV3(value: unknown): value is DoctorActionReceiptV3 {
  if (!value || typeof value !== "object") return false;
  const receipt = value as Partial<DoctorActionReceiptV3>;
  return receipt.schema_version === 3
    && typeof receipt.operation_id === "string"
    && typeof receipt.plan_id === "string"
    && typeof receipt.snapshot_revision === "string"
    && typeof receipt.module_id === "string"
    && typeof receipt.outcome === "string";
}

function isDoctorSnapshotExpired(snapshot: DoctorSnapshotV3): boolean {
  return Date.now() > snapshot.collected_at_ms + snapshot.ttl_ms;
}

function isV3SnapshotComplete(snapshot: DoctorSnapshotV3): boolean {
  if (["partial", "stale", "unavailable", "unknown"].includes(snapshot.state)) return false;
  if (snapshot.modules.some((module) => ["unavailable", "unknown"].includes(module.state))) return false;
  const completed = snapshot.coverage?.completed ?? snapshot.modules.length;
  const total = snapshot.coverage?.total ?? snapshot.modules.length;
  return total > 0 && completed >= total;
}

function doctorV3Source(snapshot: DoctorSnapshotV3): DoctorSource {
  if (isDoctorSnapshotExpired(snapshot) || snapshot.state === "stale") return "stale";
  if (!isV3SnapshotComplete(snapshot)) return "partial";
  return "native";
}

function doctorV3EvidenceLabel(snapshot: DoctorSnapshotV3, lang: "zh" | "en"): string {
  const expired = isDoctorSnapshotExpired(snapshot) || snapshot.state === "stale";
  const partial = !isV3SnapshotComplete(snapshot) && snapshot.state !== "stale";
  if (partial && expired) return lang === "zh" ? "部分证据 · 已过期" : "Partial evidence · stale";
  if (partial) return lang === "zh" ? "部分证据" : "Partial evidence";
  if (expired) return lang === "zh" ? "本地诊断快照 · 已过期" : "Local diagnostic snapshot · stale";
  return lang === "zh" ? "本地诊断快照" : "Local diagnostic snapshot";
}

function doctorPhaseLabel(phase: DoctorPhase, hasSnapshot: boolean, lang: "zh" | "en"): string {
  if (phase === "scanning") return lang === "zh" ? "正在生成统一诊断快照" : "Creating a unified diagnostic snapshot";
  if (phase === "planning") return lang === "zh" ? "正在生成修复计划" : "Preparing a repair plan";
  if (phase === "applying") return lang === "zh" ? "正在执行已确认计划" : "Applying the confirmed plan";
  if (phase === "verifying") return lang === "zh" ? "正在复检结果" : "Verifying results";
  return hasSnapshot ? (lang === "zh" ? "等待下一次操作" : "Waiting for the next action") : (lang === "zh" ? "尚未开始" : "Not started");
}

function managedAgentScopes(agent: DoctorAgentOption): Array<"global" | "project"> {
  if (Array.isArray(agent.managed_scopes)) {
    return agent.managed_scopes.filter((scope): scope is "global" | "project" => scope === "global" || scope === "project");
  }
  return [];
}

function defaultV3ProjectRoot(module: DoctorModuleV3): string {
  for (const agent of module.agents ?? []) {
    const target = agent.managed_targets?.find((candidate) => candidate.scope === "project" && typeof candidate.project_root === "string" && candidate.project_root.length > 0);
    if (target?.project_root) return target.project_root;
  }
  return "";
}

function defaultV3AgentTarget(module: DoctorModuleV3): { scope: "global" | "project"; projectRoot: string } {
  const agents = module.agents ?? [];
  if (agents.some((agent) => managedAgentScopes(agent).includes("global"))) {
    return { scope: "global", projectRoot: "" };
  }
  return { scope: "project", projectRoot: defaultV3ProjectRoot(module) };
}

function defaultV3AgentSelection(module: DoctorModuleV3, scope: "global" | "project"): Set<string> {
  return new Set((module.agents ?? [])
    .filter((agent) => agent.detected !== false && agent.supported !== false && managedAgentScopes(agent).includes(scope))
    .map((agent) => agent.id));
}

function emptyV3Selection(): DoctorRepairSelectionV3 {
  return { agent_ids: [], route_ids: [], scope: "global", project_root: null, install_mcp: false };
}

function agentRepairModuleId(preset: AgentRepairPreset): DoctorModuleId {
  if (preset === "hook") return "agent_hook_plugin";
  if (preset === "instructions_skill") return "agent_instructions";
  if (preset === "mcp") return "agent_metadata_mcp";
  return "agent_hybrid_reinject";
}

function isAgentRepairModule(moduleId: DoctorModuleId | undefined): boolean {
  return typeof moduleId === "string" && (moduleId === "agent" || moduleId.startsWith("agent_"));
}

function agentPlanDiffsComplete(plan: DoctorRepairPlanV3): boolean {
  const targets = (plan.agents ?? []).flatMap((agent) => agent.targets ?? []);
  return targets.length > 0 && targets.every((target) => Boolean(
    target.target_path
    && target.before_sha256
    && target.after_sha256
    && typeof target.diff === "string",
  ));
}

function moduleTitle(moduleId: DoctorModuleId, lang: "zh" | "en"): string {
  const preset = MODULE_PRESETS.find((item) => item.id === moduleId);
  return preset ? preset[lang].title : moduleId;
}

function moduleStateLabel(state: string | undefined, lang: "zh" | "en"): string {
  if (!state) return lang === "zh" ? "未扫描" : "Not scanned";
  if (["healthy", "verified", "passed", "ok"].includes(state)) return lang === "zh" ? "已通过" : "Passed";
  if (state === "repairable") return lang === "zh" ? "可准备修复" : "Repairable";
  if (state === "blocked") return lang === "zh" ? "已保护性阻止" : "Blocked safely";
  if (state === "manual") return lang === "zh" ? "需手动处理" : "Manual action";
  if (state === "partial" || state === "stale") return lang === "zh" ? "证据不完整" : "Incomplete evidence";
  return lang === "zh" ? "需要关注" : "Needs attention";
}

function moduleStatusTone(state: string | undefined): DoctorCheck["status"] {
  if (state && ["healthy", "verified", "passed", "ok"].includes(state)) return "healthy";
  if (state === "blocked" || state === "error") return "error";
  return "warning";
}

function receiptOutcomeLabel(outcome: string, lang: "zh" | "en"): string {
  if (outcome === "verified") return lang === "zh" ? "已验证" : "Verified";
  if (outcome === "partial") return lang === "zh" ? "部分完成" : "Partially completed";
  if (outcome === "blocked") return lang === "zh" ? "已阻止" : "Blocked";
  if (outcome === "rolled_back") return lang === "zh" ? "已回退" : "Rolled back";
  if (outcome === "failed") return lang === "zh" ? "失败" : "Failed";
  return lang === "zh" ? "未知" : "Unknown";
}

function doctorReceiptStatus(receipt: DoctorActionReceiptV3): string {
  const verification = receipt.post_verification ?? receipt.verification;
  if (verification === "verified" && ["applied", "no_change", "verified"].includes(receipt.outcome)) return "verified";
  if (verification === "partial") return "partial";
  if (verification === "failed") return "failed";
  if (verification === "unavailable" || verification === "unknown") return "unknown";
  return receipt.outcome;
}

function formatTimestampMs(value: number): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "—" : date.toLocaleString();
}

function shortHashV3(value?: string): string {
  if (!value) return "—";
  return value.length > 16 ? `${value.slice(0, 8)}…${value.slice(-6)}` : value;
}

function defaultRepairSelection(report: DoctorReport): Set<string> {
  return new Set(report.issues.filter((issue) => issue.state === "repairable" && issue.remediation?.risk === "low").map((issue) => issue.id));
}

function isDoctorReport(value: unknown): value is DoctorReport {
  if (!value || typeof value !== "object") return false;
  const report = value as Partial<DoctorReport>;
  return report.mode === "doctor-report-v2"
    && report.source === "dtoken-desktop-control-plane"
    && ["fresh", "stale", "unknown"].includes(report.freshness ?? "")
    && typeof report.revision === "string"
    && report.revision.length > 0
    && Array.isArray(report.issues)
    && Boolean(report.counts);
}

function doctorV2Source(report: DoctorReport): DoctorSource {
  if (report.freshness === "fresh") return "native";
  if (report.freshness === "stale") return "stale";
  return "unavailable";
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function layerLabel(layer: DoctorCheck["layer"], lang: "zh" | "en"): string {
  const labels: Record<DoctorCheck["layer"], { zh: string; en: string }> = {
    runtime: { zh: "本地运行时", en: "Local runtime" },
    port: { zh: "端口与进程", en: "Ports and processes" },
    route: { zh: "Agent 与路由", en: "Agents and routes" },
    provider: { zh: "Provider", en: "Provider" },
    security: { zh: "安全边界", en: "Safety boundaries" },
  };
  return labels[layer][lang];
}

function isNonNegativeFiniteNumber(value: unknown): boolean {
  if (typeof value !== "number" && typeof value !== "string") return false;
  if (typeof value === "string" && value.trim().length === 0) return false;
  const number = Number(value);
  return Number.isFinite(number) && number >= 0;
}

function isSafeNonNegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function finiteCountOrNull(value: unknown): number | null {
  return isNonNegativeFiniteNumber(value) ? Math.floor(Number(value)) : null;
}

function safeStatusText(value: unknown, fallback: string): string {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : fallback;
}

type LocalizedCheckCopy = { label: string; summary: string };

export function localizeReadinessCheck(id: string, ok: boolean, lang: "zh" | "en"): LocalizedCheckCopy {
  const key = `doctor.readiness.${id}`;
  const label = translateCopy(key, lang) === key ? translateCopy("doctor.readiness.default", lang) : translateCopy(key, lang);
  return { label, summary: translateCopy(ok ? "doctor.check.verified" : "doctor.check.notPassed", lang, { label }) };
}

export function localizeVerificationCheck(id: string, ok: boolean, lang: "zh" | "en"): LocalizedCheckCopy {
  const key = `doctor.verification.${id}`;
  const label = translateCopy(key, lang) === key ? translateCopy("doctor.verification.default", lang) : translateCopy(key, lang);
  return { label, summary: translateCopy(ok ? "doctor.check.verified" : "doctor.check.needsAttention", lang, { label }) };
}

export function buildDoctorChecks(
  result: NativeActionResult | null,
  source: DoctorSource,
  state: {
    agentOk: boolean | null;
    agentSummary: string;
    runtimeOk: boolean | null;
    runtimeSummary: string;
    activeConfigOk: boolean | null;
    activeConfigSummary: string;
    verificationOk: boolean | null;
    verificationSummary: string;
    verificationResult: VerificationResult | null;
  },
): DoctorCheck[] {
  if (source !== "native") return nonNativeDoctorChecks(source);
  const statusFor = (ok: boolean | null): DoctorCheck["status"] => ok === null ? "warning" : ok ? "healthy" : "error";
  const liveChecks: DoctorCheck[] = [
    { id: "live-agent", layer: "route", name: "Agent 连接状态", nameEn: "Agent connection", status: statusFor(state.agentOk), detail: state.agentSummary, detailEn: state.agentSummary, repair: state.agentOk ? undefined : "advanced" },
    { id: "live-runtime", layer: "runtime", name: "d-token runtime payload", nameEn: "d-token runtime payload", status: statusFor(state.runtimeOk), detail: state.runtimeSummary, detailEn: state.runtimeSummary, repair: state.runtimeOk ? undefined : "advanced" },
    { id: "live-active-config", layer: "route", name: "d-token active route config", nameEn: "d-token active route config", status: statusFor(state.activeConfigOk), detail: state.activeConfigSummary, detailEn: state.activeConfigSummary, repair: state.activeConfigOk ? undefined : "changes" },
    { id: "live-route-verification", layer: "security", name: "Agent route verification", nameEn: "Agent route verification", status: statusFor(state.verificationOk), detail: state.verificationSummary, detailEn: state.verificationSummary, repair: state.verificationOk ? undefined : "logs" },
  ];
  const toolChecks: DoctorCheck[] = (result?.tools ?? []).map((tool) => ({
    id: `tool-${tool.id}`,
    layer: tool.id === "opencode" ? "route" : "runtime",
    name: `${tool.label} ${tool.installed ? "已检测" : "未检测到"}`,
    nameEn: `${tool.label} ${tool.installed ? "detected" : "missing"}`,
    status: tool.installed || tool.id !== "opencode" ? "healthy" : "error",
    detail: tool.version ?? tool.path ?? tool.detail,
    detailEn: tool.version ?? tool.path ?? tool.detail,
    repair: !tool.installed && tool.id === "opencode" ? "advanced" : undefined,
  }));
  return [...liveChecks, ...toolChecks];
}

const previewDoctorChecks: DoctorCheck[] = [
  { id: "preview-runtime", layer: "runtime", name: "浏览器预览", nameEn: "Browser preview", status: "warning", detail: "真实 Doctor 需要在 Tauri 桌面窗口中读取 d-token 本地服务。", detailEn: "Real Doctor checks require the Tauri desktop window." },
  { id: "preview-route", layer: "route", name: "Agent 路由验证", nameEn: "Agent route verification", status: "warning", detail: "预览模式不会读取本机 Agent 配置或密钥。", detailEn: "Preview mode does not read local agent configuration or secrets." },
];

function nonNativeDoctorChecks(source: Exclude<DoctorSource, "native">): DoctorCheck[] {
  if (source === "preview") return previewDoctorChecks;
  const detail = source === "stale" ? "诊断数据已过期，不能作为当前健康状态。" : source === "partial" ? "诊断数据不完整，不能确认当前健康状态。" : "无法读取真实诊断数据，当前健康状态未知。";
  return LAYERS.map((layer) => ({ id: `${source}-${layer}`, layer, name: `${layerLabel(layer, "zh")}状态未知`, nameEn: `${layer} status unknown`, status: "warning", detail, detailEn: detail }));
}
