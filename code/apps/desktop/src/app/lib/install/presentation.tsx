import { Badge } from "../../components/ui/badge";
import { StatusBadge } from "../../components/common/StatusBadge";
import { Button } from "../../components/ui/button";
import { resolveAgentBinding, selectedCandidate, type InstallPhase } from "../installMachine";
import type { AgentUpstreamBinding, ConnectionSetupSnapshot, UpstreamCandidate } from "../connectionSetup";
import { protocolLabel } from "../protocol";

function AgentBindingRow({
  binding,
  lang,
  busy,
  transportBusy,
  onConfigure,
  onClear,
}: {
  binding: AgentUpstreamBinding;
  lang: "zh" | "en";
  busy: boolean;
  transportBusy: boolean;
  onConfigure: () => void;
  onClear: () => void;
}) {
  const presentation = presentBinding(binding, lang);
  const isMcpOrHook = binding.integration_kind === "mcp" || binding.integration_kind === "hook";
  // MCP/Hook integrations act on the tool config only; the write action exists
  // whenever a verified MCP config path was discovered. Route adapters gate on
  // native capability flags so missing values fail closed.
  const mcpPathReady = Boolean(binding.mcp_config_path) && !binding.mcp_config_path?.startsWith("<local>/");
  const canConfigure = isMcpOrHook ? mcpPathReady : binding.can_configure === true;
  const canClear = isMcpOrHook ? mcpPathReady : binding.can_clear === true;
  const configureLabel = isMcpOrHook
    ? (lang === "zh" ? "配置工具集成" : "Configure tools")
    : (lang === "zh" ? "一键配置" : "One-click configure");
  const clearLabel = isMcpOrHook
    ? (lang === "zh" ? "清除工具集成" : "Clear tools")
    : (lang === "zh" ? "清除 d-token 配置" : "Clear d-token config");
  // The card is read-only: provider/model/protocol/endpoint/source come from
  // the uniquely auto-resolved candidate. Ambiguity blocks the plan and is
  // surfaced by the binding summary; no chooser is rendered.
  const resolved = binding.resolved_candidate;
  const model = resolved?.models?.[0];
  const follow = presentAgentFollow(binding, lang);
  // Truthful labels for MCP/Hook — these integrations provide only
  // tool-level access, never route evidence or compression.
  return (
    <div className="grid gap-3 py-3 md:grid-cols-[minmax(0,1fr)_minmax(220px,0.8fr)_auto_auto] md:items-center">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-medium">{binding.agent_name}</span>
          <Badge variant="secondary">{capabilityLabel(binding, lang)}</Badge>
          {binding.follow && <Badge variant="outline">{follow.label}</Badge>}
          {isMcpOrHook && (
            <Badge variant="outline" className="border-amber-500/30 text-amber-700 dark:text-amber-300">
              {binding.integration_kind === "mcp"
                ? (lang === "zh" ? "仅 MCP 工具" : "MCP tools only")
                : (lang === "zh" ? "仅 Hook 工具" : "Hook tools only")}
            </Badge>
          )}
          {isMcpOrHook && (
            <Badge variant="outline" className="border-amber-500/30 text-amber-700 dark:text-amber-300">
              {lang === "zh" ? "工具能力，不改变模型上游" : "Tool capability; model upstream unchanged"}
            </Badge>
          )}
        </div>
        <div className="mt-1 truncate text-xs text-muted-foreground">
          {presentAgentLocation(binding, lang)}
        </div>
        {isMcpOrHook && binding.truthful_limitation && (
          <div className="mt-1 text-xs text-amber-700 dark:text-amber-300">
            {binding.truthful_limitation}
          </div>
        )}
        {!isMcpOrHook && binding.follow && (
          <div
            className={binding.follow.transport_out_of_sync
              ? "mt-1 text-xs text-amber-700 dark:text-amber-300"
              : "mt-1 text-xs text-muted-foreground"}
          >
            {follow.nextAction}
          </div>
        )}
      </div>
      <div className="min-w-0">
        <div className="truncate text-sm text-muted-foreground">
          {presentBindingSummary(binding, resolved, model, lang)}
        </div>
        {binding.follow?.current_upstream && (
          <div className="mt-1 truncate text-[11px] text-muted-foreground">
            {lang === "zh" ? "d-token 路由" : "d-token route"} · {binding.follow.current_upstream.provider_id} · {binding.follow.current_upstream.model} · {binding.follow.current_upstream.protocol}
          </div>
        )}
        {binding.follow?.last_receipt && (
          <div className="mt-1 truncate font-mono text-[10px] text-muted-foreground">
            receipt {binding.follow.last_receipt.state} · {binding.follow.last_receipt.runtime_state}
          </div>
        )}
      </div>
      <StatusBadge status={presentation.health} label={presentation.label} />
      <div className="flex items-center justify-end gap-1">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy || transportBusy || !canConfigure}
          title={!canConfigure ? (lang === "zh" ? "此 Agent 未提供可执行的一键配置写入动作" : "This Agent exposes no executable one-click write action") : undefined}
          onClick={onConfigure}
        >
          {transportBusy ? (lang === "zh" ? "处理中…" : "Working…") : configureLabel}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={busy || transportBusy || !canClear}
          title={!canClear ? (lang === "zh" ? "没有 d-token ownership receipt，不能安全清除" : "No d-token ownership receipt is available for a safe clear") : undefined}
          onClick={onClear}
        >
          {clearLabel}
        </Button>
      </div>
    </div>
  );
}

function presentAgentFollow(binding: AgentUpstreamBinding, lang: "zh" | "en") {
  const follow = binding.follow;
  const zh = lang === "zh";
  if (!follow || follow.state === "not_authorized") {
    return {
      label: zh ? "未启用跟随" : "Follow off",
      nextAction: zh
        ? "下一步：在连接方案中启用“跟随已确认上游”。"
        : "Next: enable follow for confirmed upstreams in the connection plan.",
    };
  }
  if (follow.state === "transport_out_of_sync") {
    return {
      label: zh ? "流量可能绕过 d-token" : "Transport out of sync",
      nextAction: follow.next_action === "configure"
        ? (zh ? "下一步：重新配置此 Agent，使请求重新经过 d-token。" : "Next: configure this Agent again so requests pass through d-token.")
        : (zh ? "当前版本未开放安全自动写入；d-token 保留上一次可用路由。" : "Safe automatic writes are not available for this version; d-token keeps the last-known-good route."),
    };
  }
  if (follow.state === "following" || follow.state === "applied_verified") {
    return {
      label: zh ? "自动跟随中" : "Following",
      nextAction: zh
        ? "当前 Agent transport 与 d-token route 一致。"
        : "The Agent transport and d-token route currently agree.",
    };
  }
  if (follow.next_action === "verify_runtime") {
    return {
      label: zh ? "等待 runtime 验证" : "Runtime verification needed",
      nextAction: zh
        ? "下一步：重启或重新验证 d-token runtime。"
        : "Next: restart or reverify the d-token runtime.",
    };
  }
  if (follow.next_action === "review_follow") {
    return {
      label: zh ? "需要重新确认" : "Confirmation needed",
      nextAction: zh
        ? "下一步：检查新的 provider、endpoint 或 protocol 并重新确认。"
        : "Next: confirm the new provider, endpoint, or protocol.",
    };
  }
  return {
    label: zh ? "跟随未完成" : "Follow incomplete",
    nextAction: zh
      ? "下一步：重新查看连接方案；失败时 d-token 保留上一次可用路由。"
      : "Next: reopen the connection plan; d-token keeps the last-known-good route on failure.",
  };
}

function presentAgentLocation(binding: AgentUpstreamBinding, lang: "zh" | "en") {
  if (!binding.installed) return lang === "zh" ? "本机未检测到" : "Not detected locally";
  const location = binding.binary_path || binding.config_path;
  if (!location) return lang === "zh" ? "已检测" : "Detected";
  if (location.startsWith("<local>/")) return lang === "zh" ? "已检测本机应用" : "Local app detected";
  return location;
}

function presentCcSwitch(status: string | undefined, detected: boolean, sourceState: string | undefined, lang: "zh" | "en") {
  if (!detected) {
    const incompatible = ["unsupported_schema", "locked_or_corrupt", "unsupported_format"].includes(sourceState ?? "");
    return { health: "warning" as const, label: incompatible ? (lang === "zh" ? "已发现但需要兼容处理" : "Detected, compatibility action needed") : (lang === "zh" ? "未检测到" : "Not detected") };
  }
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
  if (resolved) {
    // Read-only auto-resolution: provider · model · protocol · endpoint · source.
    return [
      resolved.name,
      model,
      protocolLabel(resolved.protocol, lang) ?? (lang === "zh" ? "未知" : "Unknown"),
      resolved.base_url,
      resolved.source,
    ].filter(Boolean).join(" · ");
  }
  if (lang === "en") return binding.reasons[0] || "No reusable upstream configuration was found";
  if (!binding.installed || binding.resolution === "unavailable") return `未检测到可用的 ${binding.agent_name} 配置`;
  if (binding.resolution === "needs-source-fix") return "存在多个候选上游 / 模型，无法唯一解析；请修复来源配置后重新扫描";
  if (binding.resolution === "needs-credential") return "已发现上游，需要补充凭据";
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
  if (phase === "ready") return { label: zh ? "查看变更" : "View changes", disabled: !reviewReady, icon: "rocket" };
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
  if (binding.resolution === "auto-resolved" && binding.capability === "confirm-required") {
    return { health: "healthy" as const, label: zh ? "已准备好，需你确认" : "Ready, needs confirmation" };
  }
  if (binding.resolution === "auto-resolved") return { health: "healthy" as const, label: zh ? "已自动解析" : "Auto-resolved" };
  if (binding.resolution === "needs-source-fix") return { health: "error" as const, label: zh ? "来源冲突" : "Source conflict" };
  if (binding.resolution === "needs-credential") return { health: "warning" as const, label: zh ? "需要凭据" : "Credential" };
  if (binding.transport_status === "advanced_required" || binding.transport_status === "schema_unverified") {
    return { health: "warning" as const, label: zh ? "仅检测" : "Detection only" };
  }
  return { health: "warning" as const, label: zh ? "等待安全适配" : "Awaiting safe adapter" };
}

function capabilityLabel(binding: AgentUpstreamBinding, lang: "zh" | "en") {
  const zh = lang === "zh";
  if (binding.can_configure === true) return zh ? "一键配置" : "One-click";
  const { capability } = binding;
  if (capability === "one-click") return zh ? "一键配置" : "One-click";
  if (capability === "confirm-required") return zh ? "确认后连接" : "Confirm to connect";
  if (capability === "prompt-only") return zh ? "仅检测" : "Detection only";
  return zh ? "安全锁定" : "Write locked";
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
      // Only the uniquely auto-resolved model (or the Codex Responses
      // passthrough marker "") may enter the route plan. Ambiguity never falls
      // back to a first model: the binding is omitted and the batch stays
      // blocked at the machine level until the source is fixed.
      const resolved = resolveAgentBinding(binding, selections[binding.agent_id]);
      if (!resolved) return [];
      return [{
        agent_id: binding.agent_id,
        candidate_id: resolved.candidate.id,
        candidate_source: resolved.candidate.source,
        model: resolved.model,
      }];
    }),
  };
}

function machineMessage(message: string, lang: "zh" | "en") {
  if (message === "native_unavailable" || message === "connection_setup_unavailable") return lang === "zh" ? "当前是浏览器预览，不读取或改写本机配置。" : "Browser preview does not read or modify local configuration.";
  if (message === "source-config-changed") return lang === "zh" ? "检测到源配置变化，旧连接方案已失效。" : "The source configuration changed, so the previous connection plan is no longer valid.";
  if (message === "agent-write-unavailable") return lang === "zh" ? "部分 Agent 当前仅检测，现有配置保持不变。" : "Some Agents are detection-only in this version; their existing configuration remains unchanged.";
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
  source_fingerprint?: string;
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
  runtime?: {
    ok: boolean;
    state: string;
    summary: string;
  };
  auto_follow?: {
    enabled?: boolean;
    state?: string;
    summary?: string;
  };
};

type ManagedAgentTransportReview = {
  ok: boolean;
  state?: string;
  summary?: string;
  review?: {
    agent_id: string;
    target_path: string;
    revision: string;
    before_sha256: string;
    after_sha256: string;
    changed: boolean;
    redacted_diff: string;
  };
};

type ManagedAgentTransportReceipt = {
  ok?: boolean;
  state?: string;
  changed?: boolean;
  verification?: {
    ok: boolean;
    detail: string;
  };
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

/**
 * Whether a discovered Agent exposes a real, verified backend write action.
 * Route adapters require native capability flags; MCP/Hook integrations act
 * on the discovered tool config path. Agents without an action must never be
 * rendered with fake Configure/Clear buttons.
 */
function hasRealAgentAction(binding: AgentUpstreamBinding) {
  if (binding.integration_kind === "mcp" || binding.integration_kind === "hook") {
    return Boolean(binding.mcp_config_path) && !binding.mcp_config_path?.startsWith("<local>/");
  }
  return binding.can_configure === true || binding.can_clear === true;
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
  hasRealAgentAction,
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
  ManagedAgentTransportReceipt,
  ManagedAgentTransportReview,
  OpenCodeConnectionReceipt,
  OpenCodeConnectionReview,
  RouteVerification,
};
