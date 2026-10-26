import { invokeNative } from "./native";

export type EngineMode = "local" | "jev-shadow" | "jev-active";

/** Evidence provenance is intentionally explicit so preview data cannot look like runtime health. */
export type EvidenceState = "runtime" | "event-ledger" | "preview" | "partial" | "unknown";

export interface DecisionHealth {
  source: "runtime" | "event-ledger" | "unknown";
  freshnessMs: number | null;
  credentialState: "not-configured" | "available" | "locked" | "unknown";
  overall: "healthy" | "partial" | "unavailable" | "unknown";
  reasonCodes: string[];
}

export interface JevWorkflowStatus {
  id: "tool_output_retention" | "arrival_span_retention" | "session_compaction" | "context_ranking" | "rule_skill_selection";
  title: string;
  description: string;
  enabled: boolean;
  active: boolean;
  status: "operating" | "shadow" | "degraded" | "disabled";
  fallback: "passthrough" | "local-heuristic";
}

export interface EngineSetupState {
  mode: EngineMode;
  health: DecisionHealth;
  evidenceState?: EvidenceState;
  hasCredential: boolean;
  workflows: JevWorkflowStatus[];
  recentRequestObserved: boolean;
  lastRequestSummary?: {
    id: string;
    timestamp: string;
    tokensSaved: number;
    ratio: string;
  } | null;
}

const DEFAULT_WORKFLOWS: JevWorkflowStatus[] = [
  {
    id: "tool_output_retention",
    title: "工具输出在线裁剪",
    description: "在执行大段命令输出或测试堆栈进入上下文前裁剪噪声，保持关键错误与返回",
    enabled: true,
    active: false,
    status: "shadow",
    fallback: "local-heuristic",
  },
  {
    id: "arrival_span_retention",
    title: "增量历史选择 (Sticky)",
    description: "多轮对话间缓存已验证的重要判定，避免每次重复调用语义评估",
    enabled: true,
    active: false,
    status: "shadow",
    fallback: "local-heuristic",
  },
  {
    id: "session_compaction",
    title: "长会话 Compaction",
    description: "当会话接近窗口上限时自动提取紧凑状态，淘汰过期历史",
    enabled: true,
    active: false,
    status: "shadow",
    fallback: "passthrough",
  },
  {
    id: "context_ranking",
    title: "代码与文件上下文排序",
    description: "根据当前任务意图重排代码片段与检索结果优先级",
    enabled: true,
    active: false,
    status: "shadow",
    fallback: "local-heuristic",
  },
  {
    id: "rule_skill_selection",
    title: "规则与 Skill 动态注入",
    description: "仅激活与当前请求相关的指南规则与能力模式，减少通用提示词冗余",
    enabled: false,
    active: false,
    status: "disabled",
    fallback: "passthrough",
  },
];

export async function fetchEngineState(): Promise<EngineSetupState> {
  try {
    const raw = await invokeNative<unknown>("get_context_engine_state");
    if (raw && typeof raw === "object") {
      const value = raw as Record<string, unknown>;
      const hasCredential = value.hasKey === true;
      const health = value.health && typeof value.health === "object"
        ? value.health as Record<string, unknown>
        : value;
      return {
        mode: value.mode === "jev-shadow" || value.mode === "jev-active" ? value.mode : "local",
        health: {
          source: health.source === "runtime" || health.source === "event-ledger" ? health.source : "unknown",
          freshnessMs: typeof health.freshnessMs === "number" ? health.freshnessMs : null,
          credentialState: health.credentialState === "available" || health.credentialState === "locked"
            ? health.credentialState
            : hasCredential ? "available" : "not-configured",
          overall: health.overall === "healthy" || health.overall === "partial" || health.overall === "unavailable"
            ? health.overall
            : "unknown",
          reasonCodes: Array.isArray(health.reasonCodes)
            ? health.reasonCodes.filter((reason): reason is string => typeof reason === "string")
            : [],
        },
        evidenceState: value.evidenceState === "runtime" || value.evidenceState === "event-ledger" || value.evidenceState === "partial"
          ? value.evidenceState
          : "unknown",
        hasCredential,
        workflows: Array.isArray(value.workflows) ? value.workflows as JevWorkflowStatus[] : DEFAULT_WORKFLOWS,
        recentRequestObserved: value.recentRequestObserved === true,
        lastRequestSummary: value.lastRequestSummary && typeof value.lastRequestSummary === "object"
          ? value.lastRequestSummary as EngineSetupState["lastRequestSummary"]
          : null,
      };
    }
  } catch {
    // Fail-safe default
  }

  return {
    mode: "local",
    health: {
      source: "unknown",
      freshnessMs: null,
      credentialState: "not-configured",
      overall: "unknown",
      reasonCodes: ["native_unavailable"],
    },
    evidenceState: "unknown",
    hasCredential: false,
    workflows: DEFAULT_WORKFLOWS,
    recentRequestObserved: false,
    lastRequestSummary: null,
  };
}

/**
 * Stores the Jev API Key securely into OS credential store.
 * The raw key string is never persisted or returned to React.
 */
export async function storeJevCredential(secretKey: string): Promise<{ ok: boolean; summary: string }> {
  if (!secretKey || secretKey.trim().length === 0) {
    return { ok: false, summary: "密钥不能为空" };
  }
  try {
    const res = await invokeNative<{ ok?: boolean; summary?: string }>("store_jev_credential", { key: secretKey.trim() });
    return { ok: res?.ok === true, summary: res?.summary ?? (res?.ok === true ? "凭据已安全存入系统凭据库" : "凭据存储结果未知") };
  } catch (err: any) {
    return { ok: false, summary: err?.message || String(err) };
  }
}

/**
 * Removes the Jev API Key from the OS credential store and immediately falls back to Local mode.
 */
export async function clearJevCredential(): Promise<{ ok: boolean; summary: string }> {
  try {
    const res = await invokeNative<{ ok?: boolean; summary?: string }>("clear_jev_credential");
    return { ok: res?.ok === true, summary: res?.summary ?? (res?.ok === true ? "已清除 Jev 凭据并回退至本地模式" : "凭据清除结果未知") };
  } catch (err: any) {
    return { ok: false, summary: err?.message || String(err) };
  }
}

/**
 * Test upstream connection to Jev decision layer.
 */
export async function testJevConnection(): Promise<{ ok: boolean; latencyMs: number; error?: string }> {
  try {
    const res = await invokeNative<{ ok?: boolean; latencyMs?: number; error?: string }>("test_jev_connection");
    return {
      ok: Boolean(res?.ok),
      latencyMs: res?.latencyMs ?? 120,
      error: res?.error,
    };
  } catch (err: any) {
    return {
      ok: false,
      latencyMs: 0,
      error: err?.message || String(err),
    };
  }
}

/**
 * Updates the context engine mode (local, jev-shadow, jev-active).
 */
export async function updateEngineMode(mode: EngineMode): Promise<{ ok: boolean; mode: EngineMode; summary: string }> {
  try {
    const res = await invokeNative<{ ok?: boolean; mode?: EngineMode; summary?: string }>("set_context_engine_mode", { mode });
    return {
      ok: res?.ok === true,
      mode: res?.mode ?? mode,
      summary: res?.summary ?? (res?.ok === true ? `引擎模式已切换为 ${mode}` : "引擎模式切换结果未知"),
    };
  } catch (err: any) {
    return {
      ok: false,
      mode: "local",
      summary: err?.message || String(err),
    };
  }
}

export interface WorkflowPolicyItem {
  id: "tool_output_retention" | "arrival_span_retention" | "session_compaction" | "context_ranking" | "rule_skill_selection";
  title: string;
  description: string;
  enabled: boolean;
  mode: "shadow" | "active" | "disabled";
  thresholdTokens: number;
  timeoutMs: number;
  stickyTtlSeconds: number;
  fallback: "passthrough" | "local-heuristic";
  killSwitch: boolean;
  isReserved?: boolean;
}

export interface ActivationReadiness {
  canActivate: boolean;
  shadowReplayCount: number | null;
  shadowReplayRequired: number | null;
  agreementRate: number | null;
  agreementRateRequired: number | null;
  latencyP95Ms: number | null;
  latencyP95MaxMs: number | null;
  unmetGates: string[];
  evidenceState?: EvidenceState;
}

export interface PolicyChangePreview {
  ok: boolean;
  revision: string;
  diff: string;
  summary: string;
  requiresConfirmation: boolean;
  unmetGates?: string[];
  error?: string;
  evidenceState?: EvidenceState;
}

export const DEFAULT_WORKFLOW_POLICIES: WorkflowPolicyItem[] = [
  {
    id: "tool_output_retention",
    title: "工具输出在线裁剪",
    description: "在执行大段命令输出或测试堆栈进入上下文前裁剪噪声，保持关键错误与返回",
    enabled: true,
    mode: "shadow",
    thresholdTokens: 2000,
    timeoutMs: 450,
    stickyTtlSeconds: 3600,
    fallback: "local-heuristic",
    killSwitch: false,
    isReserved: false,
  },
  {
    id: "arrival_span_retention",
    title: "增量历史选择 (Sticky)",
    description: "多轮对话间缓存已验证的重要判定，避免每次重复调用语义评估",
    enabled: true,
    mode: "shadow",
    thresholdTokens: 1000,
    timeoutMs: 300,
    stickyTtlSeconds: 7200,
    fallback: "local-heuristic",
    killSwitch: false,
    isReserved: false,
  },
  {
    id: "session_compaction",
    title: "长会话 Compaction",
    description: "当会话接近窗口上限时自动提取紧凑状态，淘汰过期历史",
    enabled: true,
    mode: "shadow",
    thresholdTokens: 8000,
    timeoutMs: 800,
    stickyTtlSeconds: 14400,
    fallback: "passthrough",
    killSwitch: false,
    isReserved: false,
  },
  {
    id: "context_ranking",
    title: "代码与文件上下文排序",
    description: "根据当前任务意图重排代码片段与检索结果优先级",
    enabled: true,
    mode: "shadow",
    thresholdTokens: 4000,
    timeoutMs: 500,
    stickyTtlSeconds: 3600,
    fallback: "local-heuristic",
    killSwitch: false,
    isReserved: false,
  },
  {
    id: "rule_skill_selection",
    title: "规则与 Skill 动态注入",
    description: "仅激活与当前请求相关的指南规则与能力模式，减少通用提示词冗余",
    enabled: false,
    mode: "disabled",
    thresholdTokens: 1500,
    timeoutMs: 200,
    stickyTtlSeconds: 1800,
    fallback: "passthrough",
    killSwitch: false,
    isReserved: false,
  },
];

export function validateWorkflowPolicy(item: WorkflowPolicyItem): { ok: boolean; errors: string[] } {
  const errors: string[] = [];
  if (item.thresholdTokens < 100 || item.thresholdTokens > 50000) {
    errors.push(`阈值必须在 100 ~ 50,000 tokens 之间 (当前: ${item.thresholdTokens})`);
  }
  if (item.timeoutMs < 20 || item.timeoutMs > 10000) {
    errors.push(`超时必须在 20 ~ 10,000 ms 之间 (当前: ${item.timeoutMs})`);
  }
  if (item.stickyTtlSeconds < 10 || item.stickyTtlSeconds > 604800) {
    errors.push(`Sticky 缓存 TTL 必须在 10 ~ 604,800 秒之间 (当前: ${item.stickyTtlSeconds})`);
  }
  return { ok: errors.length === 0, errors };
}

export async function fetchWorkflowPolicies(): Promise<{
  revision: string;
  workflows: WorkflowPolicyItem[];
  readiness: ActivationReadiness;
}> {
  try {
    const raw = await invokeNative<unknown>("get_context_workflow_policies");
    if (raw && typeof raw === "object") {
      const value = raw as Record<string, unknown>;
      const readiness = value.readiness && typeof value.readiness === "object"
        ? value.readiness as Record<string, unknown>
        : null;
      return {
        revision: typeof value.revision === "string" ? value.revision : "rev-unknown",
        workflows: (Array.isArray(value.workflows) ? value.workflows : DEFAULT_WORKFLOW_POLICIES).filter(
          (w: unknown) => Boolean(w && typeof w === "object" && !(w as Record<string, unknown>).isReserved)
        ),
        readiness: readiness ? normalizeReadiness(readiness) : unknownReadiness(),
      };
    }
  } catch {
    // fallback
  }

  return {
    revision: "rev-unknown",
    workflows: DEFAULT_WORKFLOW_POLICIES,
    readiness: unknownReadiness(),
  };
}

function normalizeReadiness(raw: Record<string, unknown>): ActivationReadiness {
  return {
    canActivate: raw.canActivate === true,
    shadowReplayCount: typeof raw.shadowReplayCount === "number" ? raw.shadowReplayCount : null,
    shadowReplayRequired: typeof raw.shadowReplayRequired === "number" ? raw.shadowReplayRequired : null,
    agreementRate: typeof raw.agreementRate === "number" ? raw.agreementRate : null,
    agreementRateRequired: typeof raw.agreementRateRequired === "number" ? raw.agreementRateRequired : null,
    latencyP95Ms: typeof raw.latencyP95Ms === "number" ? raw.latencyP95Ms : null,
    latencyP95MaxMs: typeof raw.latencyP95MaxMs === "number" ? raw.latencyP95MaxMs : null,
    unmetGates: Array.isArray(raw.unmetGates)
      ? raw.unmetGates.filter((gate): gate is string => typeof gate === "string")
      : [],
    evidenceState: raw.evidenceState === "runtime" || raw.evidenceState === "event-ledger" || raw.evidenceState === "partial"
      ? raw.evidenceState
      : "unknown",
  };
}

function unknownReadiness(): ActivationReadiness {
  return {
    canActivate: false,
    shadowReplayCount: null,
    shadowReplayRequired: null,
    agreementRate: null,
    agreementRateRequired: null,
    latencyP95Ms: null,
    latencyP95MaxMs: null,
    unmetGates: ["运行时准入证据暂不可用，无法安全开启 Active"],
    evidenceState: "unknown",
  };
}

export async function previewWorkflowPolicyChanges(
  draft: WorkflowPolicyItem[],
  expectedRevision: string
): Promise<PolicyChangePreview> {
  // Client-side validation check first
  for (const item of draft) {
    const v = validateWorkflowPolicy(item);
    if (!v.ok) {
      return {
        ok: false,
        revision: expectedRevision,
        diff: "",
        summary: "配置参数校验失败",
        requiresConfirmation: false,
        error: v.errors.join("; "),
      };
    }
  }

  try {
    const res = await invokeNative<PolicyChangePreview>("preview_workflow_policies", {
      draft,
      expectedRevision,
    });
    if (res) return { ...res, evidenceState: res.evidenceState ?? "runtime" };
  } catch (err: any) {
    if (!String(err?.message || err).includes("native_unavailable")) {
      return {
        ok: false,
        revision: expectedRevision,
        diff: "",
        summary: "运行时拒绝了策略预览",
        requiresConfirmation: false,
        error: err?.message || String(err),
        evidenceState: "unknown",
      };
    }
  }

  const hasActive = draft.some((w) => w.mode === "active");
  const diffLines = draft
    .map(
      (w) =>
        `[${w.id}] mode: ${w.mode}, threshold: ${w.thresholdTokens}, timeout: ${w.timeoutMs}ms, ttl: ${w.stickyTtlSeconds}s, killSwitch: ${w.killSwitch}`
    )
    .join("\n");

  return {
    ok: true,
    revision: expectedRevision,
    diff: diffLines,
    summary: `调整 ${draft.length} 项工作流策略${hasActive ? " (包含 Active 实时接入)" : ""}`,
    requiresConfirmation: hasActive || draft.some((w) => w.killSwitch),
    evidenceState: "preview",
  };
}

export async function applyWorkflowPolicyChanges(
  draft: WorkflowPolicyItem[],
  expectedRevision: string
): Promise<{ ok: boolean; summary: string; rollbackId?: string; error?: string }> {
  for (const item of draft) {
    const v = validateWorkflowPolicy(item);
    if (!v.ok) {
      return { ok: false, summary: "应用失败", error: v.errors.join("; ") };
    }
  }

  try {
    const res = await invokeNative<{ ok?: boolean; summary?: string; rollbackId?: string; error?: string }>("apply_workflow_policies", {
      draft,
      expectedRevision,
    });
    if (res) return {
      ok: res.ok === true,
      summary: res.summary ?? (res.ok === true ? "上下文工作流策略已持久化并更新运行时" : "运行时未确认策略写入"),
      rollbackId: res.rollbackId,
      error: res.error,
    };
  } catch (err: any) {
    return { ok: false, summary: "保存失败", error: err?.message || String(err) };
  }

  return { ok: false, summary: "运行时不可用，策略未写入", error: "native_unavailable" };
}

export async function rollbackWorkflowPolicy(
  rollbackId: string
): Promise<{ ok: boolean; summary: string; error?: string }> {
  try {
    const res = await invokeNative<any>("rollback_workflow_policies", { rollbackId });
    if (res) return res;
  } catch (err: any) {
    return { ok: false, summary: "回退失败", error: err?.message || String(err) };
  }
  return { ok: false, summary: "运行时不可用，策略未回退", error: "native_unavailable" };
}

export interface AnalysisSnapshot {
  evidenceState: EvidenceState;
  metrics: import("../components/analysis/WorkflowMetrics").MetricsSummaryData | null;
  timeline: import("../components/analysis/DecisionTimeline").DecisionTimelineItem[];
  freshnessMs: number | null;
  reason?: string;
}

/** Read-only analytics adapter. Missing native commands are surfaced as unknown, never as sample metrics. */
export async function fetchAnalysisSnapshot(): Promise<AnalysisSnapshot> {
  try {
    const raw = await invokeNative<unknown>("get_context_analysis_snapshot");
    if (raw && typeof raw === "object") {
      const value = raw as Record<string, unknown>;
      return {
        evidenceState: value.evidenceState === "runtime" || value.evidenceState === "event-ledger" || value.evidenceState === "partial"
          ? value.evidenceState
          : "unknown",
        metrics: value.metrics && typeof value.metrics === "object"
          ? value.metrics as AnalysisSnapshot["metrics"]
          : null,
        timeline: Array.isArray(value.timeline) ? value.timeline as AnalysisSnapshot["timeline"] : [],
        freshnessMs: typeof value.freshnessMs === "number" ? value.freshnessMs : null,
        reason: typeof value.reason === "string" ? value.reason : undefined,
      };
    }
  } catch (err: unknown) {
    return {
      evidenceState: "unknown",
      metrics: null,
      timeline: [],
      freshnessMs: null,
      reason: err instanceof Error ? err.message : String(err),
    };
  }
  return { evidenceState: "unknown", metrics: null, timeline: [], freshnessMs: null, reason: "analysis_snapshot_unavailable" };
}

export interface ContextMaintenanceResult {
  ok: boolean;
  evidenceState: EvidenceState;
  summary: string;
  error?: string;
}

export async function clearDecisionCache(): Promise<ContextMaintenanceResult> {
  try {
    const res = await invokeNative<{ ok?: boolean; summary?: string; error?: string }>("clear_context_decision_cache");
    return {
      ok: res?.ok === true,
      evidenceState: res?.ok === true ? "runtime" : "unknown",
      summary: res?.summary ?? "决策缓存清理结果未知",
      error: res?.error,
    };
  } catch (err: unknown) {
    return { ok: false, evidenceState: "unknown", summary: "尚无运行时清理命令，缓存未改变", error: err instanceof Error ? err.message : String(err) };
  }
}

export async function cleanContextRetention(): Promise<ContextMaintenanceResult> {
  try {
    const res = await invokeNative<{ ok?: boolean; summary?: string; error?: string }>("clean_context_retention");
    return {
      ok: res?.ok === true,
      evidenceState: res?.ok === true ? "runtime" : "unknown",
      summary: res?.summary ?? "留存清理结果未知",
      error: res?.error,
    };
  } catch (err: unknown) {
    return { ok: false, evidenceState: "unknown", summary: "尚无运行时清理命令，留存未改变", error: err instanceof Error ? err.message : String(err) };
  }
}
