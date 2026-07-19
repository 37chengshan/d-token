import { Badge } from "../../components/ui/badge";
import { Checkbox } from "../../components/ui/checkbox";
import { StatusBadge } from "../../components/common/StatusBadge";
import { bindingOptions, selectedCandidate, type InstallPhase } from "../installMachine";
import type { AgentUpstreamBinding, ConnectionSetupSnapshot, UpstreamCandidate } from "../connectionSetup";

function AgentBindingRow({
  binding,
  selection,
  included,
  selectable,
  lang,
  busy,
  onSelect,
  onToggle,
}: {
  binding: AgentUpstreamBinding;
  selection?: { candidateKey: string; model: string };
  included: boolean;
  selectable: boolean;
  lang: "zh" | "en";
  busy: boolean;
  onSelect: (candidateKey: string, model: string) => void;
  onToggle: (selected: boolean) => void;
}) {
  const presentation = presentBinding(binding, lang);
  const options = bindingOptions(binding);
  const selected = selectedCandidate(binding, selection);
  const resolved = selected?.candidate ?? binding.resolved_candidate;
  const model = selected?.model || resolved?.models?.[0];
  return (
    <div className="grid gap-3 py-3 md:grid-cols-[28px_minmax(0,1fr)_minmax(220px,0.8fr)_auto] md:items-center">
      <div className="flex items-center justify-center">
        {selectable ? (
          <Checkbox
            checked={included}
            disabled={busy}
            aria-label={`${lang === "zh" ? "加入本次连接" : "Include in this connection"}: ${binding.agent_name}`}
            onCheckedChange={(checked) => onToggle(checked === true)}
          />
        ) : <span className="size-4" aria-hidden="true" />}
      </div>
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-medium">{binding.agent_name}</span>
          <Badge variant="secondary">{capabilityLabel(binding.capability, lang)}</Badge>
        </div>
        <div className="mt-1 truncate text-xs text-muted-foreground">
          {presentAgentLocation(binding, lang)}
        </div>
      </div>
      <div className="min-w-0">
        {selectable && binding.resolution === "needs-choice" ? (
          <select
            aria-label={`${binding.agent_name} ${lang === "zh" ? "上游" : "upstream"}`}
            className="w-full rounded-md border border-border bg-background px-2 py-2 text-sm"
            value={selection?.candidateKey ?? ""}
            disabled={busy}
            onChange={(event) => {
              const option = options.find((item) => item.key === event.target.value);
              if (option) onSelect(option.key, option.model);
            }}
          >
            <option value="">{lang === "zh" ? "选择已发现的上游 / 模型" : "Select a discovered upstream / model"}</option>
            {options.map((option) => (
              <option key={option.key} value={option.key}>{option.candidate.name} · {option.model || (lang === "zh" ? "模型待确认" : "model required")} · {option.candidate.protocol}</option>
            ))}
          </select>
        ) : (
          <div className="truncate text-sm text-muted-foreground">
            {presentBindingSummary(binding, resolved, model, lang)}
          </div>
        )}
      </div>
      <StatusBadge status={presentation.health} label={presentation.label} />
    </div>
  );
}

function presentAgentLocation(binding: AgentUpstreamBinding, lang: "zh" | "en") {
  if (!binding.installed) return lang === "zh" ? "本机未检测到" : "Not detected locally";
  const location = binding.binary_path || binding.config_path;
  if (!location) return lang === "zh" ? "已检测" : "Detected";
  if (location.startsWith("<local>/")) return lang === "zh" ? "已检测本机应用" : "Local app detected";
  return location;
}

function presentCcSwitch(status: string | undefined, detected: boolean, lang: "zh" | "en") {
  if (!detected) return { health: "error" as const, label: lang === "zh" ? "未检测到" : "Not detected" };
  if (status === "synced") return { health: "healthy" as const, label: lang === "zh" ? "已同步" : "Synced" };
  if (status === "out-of-sync") return { health: "warning" as const, label: lang === "zh" ? "与 d-token 不一致" : "Out of sync" };
  return { health: "warning" as const, label: lang === "zh" ? "待连接确认" : "Needs confirmation" };
}

function presentBindingSummary(
  binding: AgentUpstreamBinding,
  resolved: UpstreamCandidate | null | undefined,
  model: string | undefined,
  lang: "zh" | "en",
) {
  if (resolved) return `${resolved.name}${model ? ` · ${model}` : ""} · ${resolved.protocol}`;
  if (lang === "en") return binding.reasons[0] || "No reusable upstream configuration was found";
  if (!binding.installed || binding.resolution === "unavailable") return `未检测到可用的 ${binding.agent_name} 配置`;
  if (binding.resolution === "needs-credential") return "已发现上游，需要补充凭据";
  if (binding.resolution === "unsupported") {
    return binding.capability === "prompt-only" ? "已检测应用，可生成配置建议" : "已检测应用，需要手动配置";
  }
  return "未发现可安全复用的上游配置";
}

function derivePrimaryAction(
  phase: InstallPhase,
  serviceOnline: boolean,
  reviewReady: boolean,
  lang: "zh" | "en",
  restartRequired = false,
) {
  const zh = lang === "zh";
  if (["discovering", "applying-runtime", "applying-agent-configs", "verifying", "review"].includes(phase)) {
    return { label: phase === "discovering" ? (zh ? "正在读取" : "Discovering") : (zh ? "正在处理" : "Working"), disabled: true, icon: "rocket" };
  }
  if (restartRequired) return { label: zh ? "重启服务" : "Restart service", disabled: false, icon: "restart" };
  if (phase === "ready") return { label: zh ? "查看连接方案" : "View connection plan", disabled: !reviewReady, icon: "rocket" };
  if (phase === "needs-choice") return { label: zh ? "完成选择后继续" : "Choose before continuing", disabled: true, icon: "rocket" };
  if (phase === "needs-credential") return { label: zh ? "补充所需凭据" : "Add required credential", disabled: false, icon: "settings" };
  if (["already-connected", "verified"].includes(phase)) {
    return { label: serviceOnline ? (zh ? "重新验证" : "Verify again") : (zh ? "启动并验证" : "Start and verify"), disabled: false, icon: "verify" };
  }
  return { label: zh ? "重新扫描" : "Scan again", disabled: false, icon: "rocket" };
}

function presentInstallPhase(phase: InstallPhase, lang: "zh" | "en") {
  const zh = lang === "zh";
  const map: Record<InstallPhase, { health: "healthy" | "warning" | "error"; label: string; description: string }> = {
    idle: { health: "warning", label: zh ? "等待读取" : "Waiting", description: zh ? "进入页面后会自动读取本机配置。" : "Local configuration is read automatically when this view opens." },
    discovering: { health: "warning", label: zh ? "正在发现" : "Discovering", description: zh ? "正在只读扫描 Agent 当前配置、OpenCode catalog、cc-switch 与 d-token 状态。" : "Reading Agent config, the OpenCode catalog, cc-switch, and d-token state without writing." },
    "already-connected": { health: "healthy", label: zh ? "已连接" : "Connected", description: zh ? "Agent 已指向 d-token；相同配置不会重复写入。" : "Agents already point to d-token; identical configuration will not be written again." },
    ready: { health: "healthy", label: zh ? "可继续" : "Ready to continue", description: zh ? "已找到唯一且可信的连接方案。查看变更后确认继续。" : "One reliable connection plan was found. View the changes, then confirm to continue." },
    "needs-choice": { health: "warning", label: zh ? "需要选择" : "Choice required", description: zh ? "只展开真正存在歧义的 Agent；其他字段继续保留自动结果。" : "Only genuinely ambiguous Agents need input; all other detected fields stay resolved." },
    "needs-credential": { health: "warning", label: zh ? "缺少凭据" : "Credential required", description: zh ? "地址、协议和模型已保留，只需补充无法安全复用的凭据。" : "Address, protocol, and model are preserved; only a credential that cannot be reused is required." },
    review: { health: "warning", label: zh ? "等待确认" : "Awaiting confirmation", description: zh ? "正在核对摘要、差异、源哈希与回退边界。" : "Checking the summary, differences, source hash, and rollback boundary." },
    "applying-runtime": { health: "warning", label: zh ? "正在启动服务" : "Starting service", description: zh ? "正在安装或启动 d-token runtime。" : "Installing or starting the d-token runtime." },
    "applying-agent-configs": { health: "warning", label: zh ? "正在应用配置" : "Applying config", description: zh ? "正在应用已确认的 Agent 配置批次。" : "Applying the confirmed Agent configuration batch." },
    verifying: { health: "warning", label: zh ? "正在验证" : "Verifying", description: zh ? "正在确认配置生效，并检查服务健康与真实路由证据。" : "Confirming that configuration is active and checking service health and route evidence." },
    verified: { health: "healthy", label: zh ? "验证通过" : "Verified", description: zh ? "配置、服务和路由证据一致。" : "Configuration, service, and route evidence agree." },
    partial: { health: "warning", label: zh ? "部分完成" : "Partial", description: zh ? "已安全保留完成项；仍需处理的 Agent 没有被静默写入。" : "Completed work was kept safely; remaining Agents were not silently modified." },
    blocked: { health: "error", label: zh ? "需要处理" : "Blocked", description: zh ? "没有足够证据生成安全连接方案；可重新扫描或使用高级安装回退。" : "There is not enough evidence for a safe plan; rescan or use Advanced setup as a fallback." },
    "rolled-back": { health: "warning", label: zh ? "已回退" : "Rolled back", description: zh ? "本批次 Agent 配置已恢复；d-token 服务可能仍安全保留。" : "This Agent config batch was restored; the safe d-token service installation may remain." },
  };
  return map[phase];
}

function presentBinding(binding: AgentUpstreamBinding, lang: "zh" | "en") {
  const zh = lang === "zh";
  if (!binding.installed) return { health: "error" as const, label: zh ? "未检测到" : "Not detected" };
  if (binding.already_connected) return { health: "healthy" as const, label: zh ? "已连接" : "Connected" };
  if (binding.resolution === "auto-resolved") return { health: "healthy" as const, label: zh ? "已自动解析" : "Auto-resolved" };
  if (binding.resolution === "needs-choice") return { health: "warning" as const, label: zh ? "需要选择" : "Choose" };
  if (binding.resolution === "needs-credential") return { health: "warning" as const, label: zh ? "需要凭据" : "Credential" };
  return { health: "warning" as const, label: binding.capability === "prompt-only" ? (zh ? "提供配置建议" : "Guided setup") : (zh ? "高级安装" : "Advanced") };
}

function capabilityLabel(capability: AgentUpstreamBinding["capability"], lang: "zh" | "en") {
  const zh = lang === "zh";
  if (capability === "one-click") return zh ? "一键配置" : "One-click";
  if (capability === "confirm-required") return zh ? "已识别，需手动连接" : "Detected, manual connect";
  if (capability === "prompt-only") return zh ? "配置建议" : "Guided";
  return zh ? "手动" : "Manual";
}

function connectionPath(binding: AgentUpstreamBinding, selection: { candidateKey: string; model: string } | undefined, endpoint: string) {
  const chosen = selectedCandidate(binding, selection);
  const candidate = chosen?.candidate ?? binding.resolved_candidate;
  return `${binding.agent_name} → ${endpoint} → ${candidate?.name ?? "upstream"}${chosen?.model ? ` / ${chosen.model}` : ""}`;
}

export function buildConnectionRoutePlanRequest(
  discoveryRevision: string,
  bindings: AgentUpstreamBinding[],
  selections: Record<string, { candidateKey: string; model: string }>,
): ConnectionRoutePlanRequest {
  return {
    discovery_revision: discoveryRevision,
    bindings: bindings.flatMap((binding) => {
      if (binding.already_connected) return [];
      const chosen = selectedCandidate(binding, selections[binding.agent_id]);
      const candidate = chosen?.candidate ?? binding.resolved_candidate;
      const model = chosen?.model || selections[binding.agent_id]?.model || candidate?.models?.[0];
      if (!candidate || !model) return [];
      return [{
        agent_id: binding.agent_id,
        candidate_id: candidate.id,
        candidate_source: candidate.source,
        model,
      }];
    }),
  };
}

function machineMessage(message: string, lang: "zh" | "en") {
  if (message === "native_unavailable" || message === "connection_setup_unavailable") return lang === "zh" ? "当前是浏览器预览，不读取或改写本机配置。" : "Browser preview does not read or modify local configuration.";
  if (message === "source-config-changed") return lang === "zh" ? "检测到源配置变化，旧连接方案已失效。" : "The source configuration changed, so the previous connection plan is no longer valid.";
  if (message === "manual-agent-step-required") return lang === "zh" ? "仍有 Agent 需要按当前变更手动完成。" : "One or more Agents still need a manual configuration step.";
  if (message === "select-at-least-one-agent") return lang === "zh" ? "请至少选择一个可一键连接的 Agent。" : "Select at least one one-click Agent.";
  return message;
}

export function connectionDiscoveryCounts(snapshot: ConnectionSetupSnapshot | null) {
  if (!snapshot) return { agents: 0, upstreams: 0, ccSwitchProfiles: 0 };
  const installedAgentIds = new Set(snapshot.agent_bindings
    .filter((binding) => binding.installed || binding.already_connected)
    .map((binding) => binding.agent_id));
  const upstreams = new Set(snapshot.candidates
    .filter((candidate) => candidate.agent_ids.some((agentId) => installedAgentIds.has(agentId)))
    .filter((candidate) => Boolean(candidate.base_url) || candidate.models.length > 0)
    .map((candidate) => `${candidate.source}:${candidate.id}:${candidate.base_url ?? ""}`));
  const ccSwitchSource = snapshot.sources.find((source) => source.id === "cc-switch-sqlite");
  const profileCount = Number(ccSwitchSource?.profile_count ?? 0);
  return {
    agents: installedAgentIds.size,
    upstreams: upstreams.size,
    ccSwitchProfiles: Number.isFinite(profileCount) && profileCount > 0 ? Math.trunc(profileCount) : 0,
  };
}

export function desktopInstallRoot(userAgent: string) {
  return /windows/i.test(userAgent) ? "%LOCALAPPDATA%\\d-token" : "~/Library/Application Support/d-token";
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}

export function presentServiceInstance(
  serviceStatus: DtokenServiceStatus | null,
  serviceOnline: boolean,
  serviceEndpoint: string,
  updatedAt: Date | null,
  lang: "zh" | "en",
) {
  const timestamp = updatedAt?.toLocaleTimeString();
  const suffix = timestamp ? ` · ${timestamp}` : "";
  if (serviceStatus?.manifest_stale === true || (serviceStatus?.pid != null && serviceStatus.pid_running === false)) {
    return `${lang === "zh" ? "历史运行记录已失效，当前实例标识已隐藏" : "Historical runtime record is stale; current instance identifiers are hidden"}${suffix}`;
  }
  if (serviceStatus?.pid_running === false && serviceOnline) {
    return `${lang === "zh" ? "本地监听已验证，托管实例标识不可用" : "Local listener verified; managed instance identifiers unavailable"}${suffix}`;
  }
  if (serviceOnline) {
    const instance = [serviceStatus?.pid ? `PID ${serviceStatus.pid}` : "", serviceEndpoint]
      .filter(Boolean)
      .join(" · ");
    return `${instance || (lang === "zh" ? "本地监听已验证" : "Local listener verified")}${suffix}`;
  }
  return `${lang === "zh" ? "当前未检测到监听实例" : "No listening instance detected"}${suffix}`;
}

export function presentConnectionPrivacy(privacy: string | undefined, lang: "zh" | "en") {
  if (lang === "zh") return "隐私边界：未读取或显示凭据、认证文件、提示词或响应原文。";
  return privacy || "Privacy boundary: credentials, auth files, prompts, and responses were not read or displayed.";
}

export function presentRouteVerificationSummary(
  verification: Pick<RouteVerification, "ok" | "summary">,
  lang: "zh" | "en",
) {
  if (lang === "zh") {
    return verification.ok ? "配置、服务和路由证据一致。" : "连接证据仍需处理。";
  }
  return verification.summary;
}

function shortHash(value?: string | null) {
  return value ? value.slice(0, 12) : "—";
}

type DtokenServiceStatus = {
  state: string;
  managed: boolean;
  restart_required?: boolean;
  pid?: number | null;
  pid_running?: boolean;
  manifest_stale?: boolean;
  service_url?: string;
};

type OpenCodeConnectionReview = {
  ok: boolean;
  state: string;
  target: string;
  revision?: string | null;
  before_sha256?: string | null;
  after_sha256?: string | null;
  diff?: string | null;
  summary: string;
};

type OpenCodeConnectionReceipt = {
  ok: boolean;
  state: string;
  verified: boolean;
  summary: string;
  rollback_id?: string | null;
};

type ConnectionRoutePlanRequest = {
  discovery_revision: string;
  bindings: Array<{
    agent_id: string;
    candidate_id: string;
    candidate_source: string;
    model: string;
  }>;
};

type ConnectionRoutePlanReview = {
  ok: boolean;
  state: string;
  target?: string;
  discovery_revision?: string;
  before_sha256?: string | null;
  after_sha256?: string | null;
  diff?: string | null;
  agents?: string[];
  summary?: string;
  error?: string;
};

type ConnectionRoutePlanReceipt = {
  ok: boolean;
  state: string;
  verified: boolean;
  summary: string;
  rollback_id?: string | null;
  before_sha256?: string | null;
  after_sha256?: string | null;
};

type RouteVerification = {
  ok: boolean;
  passed: number;
  total: number;
  summary: string;
  checks: Array<{ id: string; label: string; ok: boolean; summary: string }>;
};

type HybridAgentIntegrationTarget = {
  target_path: string;
  before_sha256: string;
  after_sha256: string;
  diff?: string | null;
};

type HybridAgentIntegrationReview = {
  ok: boolean;
  state: string;
  error?: string;
  plan?: {
    review_digest: string;
    targets: HybridAgentIntegrationTarget[];
  };
};

type HybridAgentIntegrationApply = {
  ok: boolean;
  state: string;
  summary?: string;
  error?: string;
  receipt?: { transaction_id: string };
};

type HybridAgentIntegrationVerification = {
  ok: boolean;
  state: string;
  summary?: string;
};

function supportsAutomaticAgentIntegration(agentId: string) {
  return ["claude-code", "codex", "opencode"].includes(agentId);
}

function hybridAgentIntegrationDraft(agentId: string) {
  return {
    agent_ids: [agentId],
    scope: "global",
    project_root: null,
    install_instructions: true,
    install_hook_or_plugin: true,
    install_skill: true,
    install_mcp: false,
  };
}

export {
  AgentBindingRow,
  connectionPath,
  derivePrimaryAction,
  hybridAgentIntegrationDraft,
  machineMessage,
  presentCcSwitch,
  presentInstallPhase,
  safeText,
  shortHash,
  supportsAutomaticAgentIntegration,
};
export type {
  ConnectionRoutePlanReceipt,
  ConnectionRoutePlanReview,
  DtokenServiceStatus,
  HybridAgentIntegrationApply,
  HybridAgentIntegrationReview,
  HybridAgentIntegrationVerification,
  OpenCodeConnectionReceipt,
  OpenCodeConnectionReview,
  RouteVerification,
};
