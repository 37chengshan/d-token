import type { ActivityDay } from "../../components/common/Heatmap";
import type { ConnectionSetupSnapshot } from "../connectionSetup";
import type { Lang } from "../i18n";
import { classifyEvidence, type EvidenceState } from "../state/evidence";

export type AccountQuotaWindow = {
  key: string;
  label: string;
  usedPercent: number | null;
  resetsAt: string | null;
};

export type AccountQuotaSummary = {
  provider: "codex" | "claude";
  label: string;
  accountRef: string | null;
  state: "verified" | "partial" | "stale" | "unavailable";
  source: string;
  windows: AccountQuotaWindow[];
};

export function deriveAccountQuotaSummary(payload: Record<string, any> | null, evidence: EvidenceState, lang: Lang): AccountQuotaSummary[] {
  const zh = lang === "zh";
  const quotaRoot = objectValue(payload?.account_quotas ?? payload?.quota);
  const claude = objectValue(quotaRoot.claude ?? payload?.subscription_window);
  const claudeLatest = objectValue(claude.latest ?? claude);
  const codex = objectValue(quotaRoot.codex ?? payload?.codex_rate_limits);
  const codexLatest = objectValue(codex.latest ?? codex);
  const stateFor = (source: Record<string, any>, windows: AccountQuotaWindow[]) => {
    if (evidence !== "verified" && evidence !== "stale") return "unavailable" as const;
    if (!quotaSourceAllowed(source.source) || !windows.some((window) => window.usedPercent !== null)) return "unavailable" as const;
    const declared = safeText(source.freshness ?? source.state, "").toLowerCase();
    const capturedAt = parseQuotaTimestamp(source.captured_at ?? source.capturedAt ?? source.observed_at);
    if (capturedAt === null) return "unavailable" as const;
    const staleAfterMs = firstNumberEvidence(source.stale_after_ms, source.staleAfterMs) ?? 15 * 60 * 1000;
    if (evidence === "stale" || declared === "stale" || declared === "expired" || Date.now() - capturedAt > staleAfterMs) return "stale" as const;
    return windows.every((window) => window.usedPercent !== null) ? "verified" as const : "partial" as const;
  };
  const claudeWindows = [
    quotaWindow("five_hour", zh ? "5 小时" : "5 hours", claudeLatest.five_hour),
    quotaWindow("seven_day", zh ? "7 日" : "7 days", claudeLatest.seven_day),
  ];
  const primary = objectValue(codexLatest.primary ?? codexLatest.primary_window);
  const secondary = objectValue(codexLatest.secondary ?? codexLatest.secondary_window);
  const codexWindows = [
    quotaWindow("primary", zh ? "5 小时" : "5 hours", primary, 300),
    quotaWindow("secondary", zh ? "7 日" : "7 days", secondary, 10080),
  ];
  return [
    { provider: "codex", label: "Codex", accountRef: quotaAccountRef(codexLatest) ?? quotaAccountRef(codex), state: stateFor(codexLatest, codexWindows), source: quotaSource(codexLatest, lang), windows: codexWindows },
    { provider: "claude", label: "Claude", accountRef: quotaAccountRef(claudeLatest) ?? quotaAccountRef(claude), state: stateFor(claudeLatest, claudeWindows), source: quotaSource(claudeLatest, lang), windows: claudeWindows },
  ];
}

export type OperationalTone = "positive" | "warning" | "negative" | "neutral";

export type OperationalStatus = {
  label: string;
  detail: string;
  tone: OperationalTone;
};

export type CcSwitchStatus = {
  state: "unknown" | "unverified" | "synced" | "out_of_sync";
  label: string;
  detail: string;
};

export type OverviewOperationalSummary = {
  service: OperationalStatus;
  agents: OperationalStatus;
  route: OperationalStatus;
  issues: OperationalStatus;
  ccSwitch: CcSwitchStatus;
};

export function deriveOverviewOperationalSummary(
  payload: Record<string, any> | null,
  status: string,
  lang: Lang = "zh",
  connection: {
    snapshot: ConnectionSetupSnapshot | null;
    status: "idle" | "discovering" | "ready" | "stale" | "unavailable";
  } = { snapshot: null, status: "unavailable" },
): OverviewOperationalSummary {
  const zh = lang === "zh";
  const evidenceState = classifyEvidence({ payload, status, requireRuntimeIdentity: true });
  const requestLogs = recordArray(payload?.request_logs);
  const runtimeConfig = objectValue(payload?.runtime_active_config);
  const desktopConfig = objectValue(payload?.desktop_active_config);
  const routes = firstPopulatedArray(runtimeConfig.routes, payload?.routes);
  const providers = firstPopulatedArray(runtimeConfig.providers, payload?.providers);
  const instances = firstPopulatedArray(runtimeConfig.instances, payload?.instances);
  const route = routes[0] ?? objectValue(desktopConfig.route);
  const providerId = safeText(route?.target_provider_id, "");
  const provider = providers.find((item) => safeText(item?.id, "") === providerId)
    ?? providers[0]
    ?? objectValue(desktopConfig.provider);
  const routeName = safeText(route?.name, safeText(route?.id, ""));
  const providerName = safeText(provider?.name, safeText(provider?.id, ""));
  const routeConfigured = Boolean(routeName || providerName);

  const trafficAgents = uniqueText(requestLogs.map((item) => item?.agent));
  const setupFresh = connection.status === "ready" && connection.snapshot?.state === "loaded";
  const setupAvailable = (setupFresh || connection.status === "stale") && connection.snapshot?.state === "loaded";
  const setupBindings = setupAvailable ? connection.snapshot?.agent_bindings ?? [] : [];
  const setupConnectedAgents = uniqueText(
    setupBindings.filter((binding) => binding.already_connected).map((binding) => binding.agent_name),
  );
  const setupDetectedAgents = uniqueText(
    setupBindings.filter((binding) => binding.installed).map((binding) => binding.agent_name),
  );
  const configuredAgents = uniqueText([
    ...instances.map((item) => item?.agent),
    objectValue(desktopConfig.instance).agent,
  ]);
  const errorCount = requestLogs.filter((item) => Boolean(item?.error)).length;
  const configState = safeText(desktopConfig.state, "");
  const configProblem = configState === "invalid_json" || configState === "unreadable";
  const issueCount = errorCount + (status === "runtime" && !routeConfigured ? 1 : 0) + (configProblem ? 1 : 0);
  const isRuntime = evidenceState === "verified";

  const service: OperationalStatus = isRuntime
    ? {
        label: zh ? "正在运行" : "Running",
        detail: safeText(
          payload?.desktop_service_url,
          zh ? "本地服务已响应，未提供监听地址" : "Local service responded without a listening address",
        ),
        tone: "positive",
      }
    : evidenceState === "loading"
      ? { label: zh ? "正在检查" : "Checking", detail: zh ? "正在连接 d-token 本地服务" : "Connecting to the d-token local service", tone: "neutral" }
      : evidenceState === "preview"
        ? { label: zh ? "浏览器预览" : "Browser preview", detail: zh ? "未连接真实本地服务" : "No real local service is connected", tone: "neutral" }
        : evidenceState === "stale"
          ? { label: zh ? "状态已过期" : "Status is stale", detail: zh ? "旧数据不作为当前服务状态" : "Old data is not treated as current service status", tone: "warning" }
          : evidenceState === "partial"
            ? { label: zh ? "状态不完整" : "Status is incomplete", detail: zh ? "缺少完整的本地服务证据" : "Complete local service evidence is missing", tone: "warning" }
            : { label: zh ? "未连接" : "Not connected", detail: zh ? "本地服务未启动或暂时无法访问" : "The local service is stopped or unavailable", tone: "negative" };

  const agents: OperationalStatus = setupFresh && setupConnectedAgents.length > 0
    ? { label: setupConnectedAgents.join("、"), detail: zh ? "本机当前配置已指向 d-token" : "The current local configuration points to d-token", tone: "positive" }
    : connection.status === "stale" && setupConnectedAgents.length > 0
      ? { label: setupConnectedAgents.join("、"), detail: zh ? "上次检测到本机配置指向 d-token；配置证据已过期" : "The last check found local config pointing to d-token; configuration evidence is stale", tone: "warning" }
      : isRuntime && trafficAgents.length > 0
        ? { label: trafficAgents.join("、"), detail: zh ? `历史中有 ${requestLogs.length} 条请求；不能据此判断当前仍已连接` : `${requestLogs.length} historical request(s) exist; they do not prove a current connection`, tone: "warning" }
        : !isRuntime
          ? {
              label: zh ? "尚无已验证连接" : "No verified connection",
              detail: setupDetectedAgents.length > 0
                ? (connection.status === "stale"
                    ? (zh ? `上次检测：${setupDetectedAgents.join("、")}；配置证据已过期` : `Last detected: ${setupDetectedAgents.join(", ")}; configuration evidence is stale`)
                    : (zh ? `已检测：${setupDetectedAgents.join("、")}；只有真实请求才能证明连接` : `Detected: ${setupDetectedAgents.join(", ")}; only real requests prove a connection`))
                : (zh ? "只有实时本地请求才能证明 Agent 已连接" : "Only live local requests can prove an agent connection"),
              tone: "neutral",
            }
          : configuredAgents.length > 0
            ? { label: zh ? "尚无已验证连接" : "No verified connection yet", detail: zh ? `已配置：${configuredAgents.join("、")}` : `Configured: ${configuredAgents.join(", ")}`, tone: "warning" }
            : setupDetectedAgents.length > 0
              ? { label: zh ? "尚无已验证连接" : "No verified connection yet", detail: zh ? `已检测：${setupDetectedAgents.join("、")}` : `Detected: ${setupDetectedAgents.join(", ")}`, tone: "warning" }
              : { label: zh ? "尚未连接" : "Not connected", detail: zh ? "完成配置并发送一次真实请求后显示" : "Configure an agent and send one real request", tone: isRuntime ? "warning" : "neutral" };

  const routeStatus: OperationalStatus = isRuntime && routeConfigured
    ? {
        label: [routeName, providerName].filter(Boolean).join(" → "),
        detail: requestLogs.length > 0
          ? (zh ? "最近请求已经过 d-token" : "Recent requests passed through d-token")
          : (zh ? "配置已加载，尚无请求证据" : "Configuration loaded; no request evidence yet"),
        tone: requestLogs.length > 0 ? "positive" : "warning",
      }
    : {
        label: isRuntime ? (zh ? "等待连接配置" : "Waiting for connection setup") : (zh ? "尚未连接真实路径" : "No real connection path"),
        detail: zh ? "缺少实时配置证据时保持未连接状态" : "The connection remains unverified without live configuration evidence",
        tone: isRuntime ? "warning" : "neutral",
      };

  const issues: OperationalStatus = !isRuntime
    ? { label: zh ? "尚无当前问题证据" : "No current issue evidence", detail: zh ? "连接实时本地服务后才能判断最近问题" : "Connect the live local service to inspect recent issues", tone: "warning" }
    : issueCount > 0
      ? { label: zh ? `${issueCount} 个问题` : `${issueCount} issue(s)`, detail: errorCount > 0 ? (zh ? `${errorCount} 条请求失败，需要查看原因` : `${errorCount} request(s) failed`) : (zh ? "连接配置尚未完整" : "Connection setup is incomplete"), tone: "negative" }
      : requestLogs.length === 0
        ? { label: zh ? "尚无请求记录" : "No request records yet", detail: zh ? "收到真实请求后才能判断请求错误" : "Request errors can be evaluated after real traffic arrives", tone: "neutral" }
        : { label: zh ? "没有发现问题" : "No issues found", detail: zh ? "以当前本地服务记录为准" : "Based on current local service records", tone: "positive" };

  const ccSwitch = isRuntime
    ? deriveCcSwitchStatus(payload, providers, lang, setupAvailable ? connection.snapshot?.cc_switch_sync ?? null : null, setupFresh)
    : setupAvailable
      ? deriveCcSwitchStatus(payload, providers, lang, connection.snapshot?.cc_switch_sync ?? null, setupFresh)
      : { state: "unknown" as const, label: zh ? "尚未获得实时验证" : "No live verification yet", detail: zh ? "非实时数据不会显示同步成功" : "Non-live data is never shown as successfully synced" };
  return { service, agents, route: routeStatus, issues, ccSwitch };
}

export function overviewFromPayload(payload: Record<string, any> | null, status: string, lang: Lang = "zh") {
  const zh = lang === "zh";
  const evidenceState = classifyEvidence({ payload, status, requireRuntimeIdentity: true });
  const stats = objectValue(payload?.stats);
  const runtimeTokens = objectValue(stats.tokens);
  const requestLogs = recordArray(payload?.request_logs);
  const isLastVerifiedSnapshot = evidenceState === "verified" || evidenceState === "stale";
  const trafficAvailable = isLastVerifiedSnapshot && Array.isArray(payload?.request_logs);
  const physicalPairs = requestLogs.flatMap((log) => {
    const before = numberEvidence(log.input_tokens_before);
    const after = numberEvidence(log.input_tokens_after);
    return before === null || after === null ? [] : [{ before, saved: Math.max(0, before - after) }];
  });
  const recentPhysicalSaved = physicalPairs.length > 0 ? physicalPairs.reduce((total, pair) => total + pair.saved, 0) : null;
  const recentPhysicalBefore = physicalPairs.length > 0 ? physicalPairs.reduce((total, pair) => total + pair.before, 0) : null;
  const aggregatePhysicalBefore = firstNumberEvidence(runtimeTokens.input);
  const aggregatePhysicalAfter = firstNumberEvidence(runtimeTokens.input_after);
  const physicalSaved = aggregatePhysicalBefore !== null && aggregatePhysicalAfter !== null ? Math.max(0, aggregatePhysicalBefore - aggregatePhysicalAfter) : recentPhysicalSaved;
  const physicalBefore = aggregatePhysicalBefore ?? recentPhysicalBefore;
  const claimableValues = requestLogs.map((log) => numberEvidence(log.tokens_saved)).filter((value): value is number => value !== null);
  const claimableSaved = firstNumberEvidence(runtimeTokens.saved, stats.tokens_saved) ?? (claimableValues.length > 0 ? claimableValues.reduce((total, value) => total + value, 0) : 0);
  const savingsPercent = physicalSaved !== null && physicalBefore !== null && physicalBefore > 0 ? (physicalSaved / physicalBefore) * 100 : null;
  const metricsAvailable = physicalSaved !== null || trafficAvailable;
  const compressionAvailable = savingsPercent !== null || trafficAvailable;
  const hasRuntimeData = isLastVerifiedSnapshot && (trafficAvailable || metricsAvailable || compressionAvailable);

  if (!hasRuntimeData) {
    return {
      source: evidenceState === "verified" ? "runtime-empty" : evidenceState,
      metricsAvailable: false,
      compressionAvailable: false,
      trafficAvailable: false,
      sourceLabel: evidenceState === "verified"
        ? (zh ? "本地服务在线 · 暂无请求记录" : "Local service online · no request records")
        : evidenceState === "preview"
          ? (zh ? "浏览器预览 · 未连接实时数据" : "Browser preview · no live data")
          : evidenceState === "stale"
            ? (zh ? "本地数据已过期 · 不作为当前状态" : "Local data is stale · not treated as current")
            : evidenceState === "partial"
              ? (zh ? "本地数据不完整 · 未确认当前状态" : "Local data is incomplete · current state unverified")
              : evidenceState === "loading"
                ? (zh ? "正在连接 d-token 本地服务" : "Connecting to the local d-token service")
                : (zh ? "d-token 本地服务暂不可用" : "Local d-token service unavailable"),
      stats: emptyOverviewStats,
      trend: [],
      activityDays: [],
      tools: [],
      activitySource: zh ? "等待实时本地请求记录" : "Waiting for live local request records",
      errorLabel: zh ? "尚无当前错误证据" : "No current error evidence",
    };
  }

  const requestCount = firstNumberEvidence(objectValue(stats.requests).total) ?? requestLogs.length;
  const errorCount = firstNumberEvidence(objectValue(stats.requests).failed) ?? requestLogs.filter((log) => Boolean(log.error)).length;
  const activeAgents = new Set(requestLogs.map((log) => safeText(log.agent, "")).filter(Boolean)).size;
  return {
    source: evidenceState === "stale" ? "d-token-runtime-stale" : trafficAvailable ? "d-token-runtime" : "d-token-runtime-metrics",
    metricsAvailable,
    compressionAvailable,
    trafficAvailable,
    sourceLabel: evidenceState === "stale"
      ? requestCount === 0 ? (zh ? "上次验证 · 暂无请求记录 · 数据已过期" : "Last verified · no requests · data is stale") : (zh ? `上次验证 · ${requestCount} 条请求 · 数据已过期` : `Last verified · ${requestCount} request(s) · data is stale`)
      : trafficAvailable
        ? requestCount === 0 ? (zh ? "本地服务在线 · 暂无请求记录" : "Local service online · no request records") : (zh ? `d-token 本地服务 · ${requestCount} 条请求` : `Local d-token service · ${requestCount} request(s)`)
        : (zh ? "本地服务在线 · 暂无请求记录" : "Local service online · no request records"),
    stats: { savedTotal: physicalSaved ?? 0, claimableSaved, compressionRate: savingsPercent === null ? 0 : Math.max(0, Math.min(1, savingsPercent / 100)), activeAgents, errorsToday: errorCount, requestCount },
    trend: buildTrendFromLogs(requestLogs),
    activityDays: buildActivityDaysFromLogs(requestLogs),
    tools: buildToolsFromLogs(requestLogs),
    activitySource: trafficAvailable
      ? evidenceState === "stale" ? (zh ? "上次验证的 d-token 本地请求记录" : "Last verified local d-token request records") : (zh ? "d-token 本地请求记录" : "Local d-token request records")
      : (zh ? "等待实时本地请求记录" : "Waiting for live local request records"),
    errorLabel: trafficAvailable
      ? evidenceState === "stale"
        ? errorCount > 0 ? (zh ? `上次验证记录 ${errorCount} 条失败；非当前状态` : `${errorCount} failures in last verified data; not current`) : (zh ? "上次验证没有请求失败；非当前状态" : "No failures in last verified data; not current")
        : errorCount > 0 ? (zh ? "存在请求失败" : "Request failures recorded") : (zh ? "没有请求失败" : "No request failures recorded")
      : (zh ? "尚无实时请求记录" : "No live request records"),
  };
}

const emptyOverviewStats = { savedTotal: 0, claimableSaved: 0, compressionRate: 0, activeAgents: 0, errorsToday: 0, requestCount: 0 };

function deriveCcSwitchStatus(payload: Record<string, any> | null, providers: any[], lang: Lang, connectionEvidence: Record<string, unknown> | null = null, connectionVerified = false): CcSwitchStatus {
  const zh = lang === "zh";
  const evidence = objectValue(connectionEvidence ?? payload?.cc_switch_sync ?? payload?.ccSwitchSync);
  const state = safeText(evidence.status ?? evidence.state, "").toLowerCase();
  const verified = evidence.verified === true || evidence.evidence_verified === true || (connectionVerified && evidence.source === "cc-switch-read-only" && ["synced", "in_sync", "out-of-sync", "out_of_sync", "mismatch"].includes(state));
  const detected = evidence.detected === true || providers.some((provider) => ["cc_switch", "cc-switch", "ccswitch"].includes(safeText(provider?.kind, "").toLowerCase()));
  const evidenceDetail = uniqueText([evidence.profile, evidence.upstream, evidence.connection_path]).join(" · ");
  if (verified && (state === "synced" || state === "in_sync")) return { state: "synced", label: zh ? "已验证同步" : "Sync verified", detail: evidenceDetail || (zh ? "同步证据来自本地检测结果" : "Backed by local detection evidence") };
  if (verified && (state === "out-of-sync" || state === "out_of_sync" || state === "mismatch")) return { state: "out_of_sync", label: zh ? "连接不一致" : "Connection mismatch", detail: evidenceDetail || (zh ? "需要在安装与连接页确认当前上游" : "Confirm the current upstream in Install & Connect") };
  if (detected || Object.keys(evidence).length > 0) return { state: "unverified", label: zh ? "检测到配置，尚未验证同步" : "Configuration detected; sync not verified", detail: zh ? "没有同步时间或校验结果，不显示为正常" : "No sync time or verification result; not shown as healthy" };
  return { state: "unknown", label: zh ? "尚未检测" : "Not detected yet", detail: zh ? "不会自动猜测或改写第三方配置" : "Third-party settings are not guessed or rewritten" };
}

function quotaWindow(key: string, label: string, value: unknown, expectedMinutes?: number): AccountQuotaWindow {
  const window = objectValue(value);
  const used = firstNumberEvidence(window.used_percent, window.utilization_pct, window.usedPercent);
  const minutes = firstNumberEvidence(window.window_minutes, window.windowMinutes, window.window_seconds !== undefined ? Number(window.window_seconds) / 60 : null, window.limit_window_seconds !== undefined ? Number(window.limit_window_seconds) / 60 : null);
  const validUsed = (expectedMinutes === undefined || minutes === expectedMinutes) && used !== null && used >= 0 && used <= 100 ? used : null;
  return { key, label, usedPercent: validUsed, resetsAt: normalizeQuotaReset(window.resets_at ?? window.reset_at ?? window.resetsAt) };
}

function quotaSource(value: Record<string, any>, lang: Lang) { return quotaSourceAllowed(value.source) ? (lang === "zh" ? "d-token 本地额度证据" : "Local d-token quota evidence") : (lang === "zh" ? "等待本地额度证据" : "Waiting for local quota evidence"); }
function quotaAccountRef(value: Record<string, any>) { const ref = safeText(value.account_ref, "").replace(/[\u0000-\u001F\u007F]/g, "").trim(); return ref && !ref.includes("@") ? ref.slice(0, 64) : null; }
function quotaSourceAllowed(value: unknown) { return ["passive-runtime-metadata", "codex-response-headers", "codex-wham-usage", "claude-usage-metadata", "dtoken-runtime-quota"].includes(safeText(value, "")); }
function parseQuotaTimestamp(value: unknown): number | null { if (typeof value === "number" && Number.isFinite(value)) { const milliseconds = value < 10_000_000_000 ? value * 1000 : value; return Number.isFinite(new Date(milliseconds).getTime()) ? milliseconds : null; } if (typeof value !== "string" || !value.trim()) return null; const parsed = new Date(value).getTime(); return Number.isFinite(parsed) ? parsed : null; }
function normalizeQuotaReset(value: unknown): string | null { const parsed = parseQuotaTimestamp(value); return parsed === null ? null : new Date(parsed).toISOString(); }

function buildActivityDaysFromLogs(logs: any[]): ActivityDay[] {
  if (logs.length === 0) return [];
  const byDay = new Map<string, ActivityDay>();
  for (const log of logs) {
    const timestamp = timestampEvidence(log?.timestamp_ms);
    if (timestamp === null) continue;
    const date = new Date(timestamp).toISOString().slice(0, 10);
    const current = byDay.get(date) ?? { date, sessions: 0, minutes: 0, tokens: 0, tools: 0 };
    current.sessions += 1;
    const latencyMs = numberEvidence(log?.latency_ms);
    if (latencyMs !== null && latencyMs > 0) current.minutes += Math.round((latencyMs / 60_000) * 100) / 100;
    const inputTokens = numberEvidence(log?.input_tokens_before);
    if (inputTokens !== null && inputTokens > 0) current.tokens += inputTokens;
    current.tools += transformNames(log).length;
    byDay.set(date, current);
  }
  return fillRecentActivity(Array.from(byDay.values()).sort((a, b) => a.date.localeCompare(b.date)));
}

function fillRecentActivity(days: ActivityDay[]) {
  if (days.length === 0) return [];
  const byDate = new Map(days.map((day) => [day.date, day]));
  const start = new Date(`${days[0].date}T00:00:00Z`);
  const end = new Date(`${days[days.length - 1].date}T00:00:00Z`);
  const filled: ActivityDay[] = [];
  for (let d = new Date(start); d <= end; d.setUTCDate(d.getUTCDate() + 1)) {
    const date = d.toISOString().slice(0, 10);
    filled.push(byDate.get(date) ?? { date, sessions: 0, minutes: 0, tokens: 0, tools: 0 });
  }
  return filled;
}

function buildToolsFromLogs(logs: any[]) { const counts = new Map<string, number>(); for (const log of logs) { if (timestampEvidence(log?.timestamp_ms) === null) continue; for (const transform of transformNames(log)) counts.set(transform, (counts.get(transform) ?? 0) + 1); } return Array.from(counts.entries()).sort((a, b) => b[1] - a[1]).map(([name, calls]) => ({ name, calls })); }
function buildTrendFromLogs(logs: any[]) { if (logs.length === 0) return []; const byDay = new Map<string, number>(); for (const log of logs) { const timestamp = timestampEvidence(log?.timestamp_ms); const before = numberEvidence(log?.input_tokens_before); const after = numberEvidence(log?.input_tokens_after); if (timestamp === null || before === null || after === null) continue; const day = new Date(timestamp).toISOString().slice(5, 10); byDay.set(day, (byDay.get(day) ?? 0) + Math.max(0, before - after)); } return Array.from(byDay.entries()).sort(([left], [right]) => left.localeCompare(right)).slice(-14).map(([day, saved]) => ({ day, saved })); }
function arrayValue(value: unknown): any[] { return Array.isArray(value) ? value : []; }
function recordArray(value: unknown): Record<string, any>[] { return arrayValue(value).filter((item): item is Record<string, any> => Boolean(item) && typeof item === "object" && !Array.isArray(item)); }
function objectValue(value: unknown): Record<string, any> { return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, any> : {}; }
function numberEvidence(value: unknown): number | null { if (typeof value !== "number" && typeof value !== "string") return null; if (typeof value === "string" && value.trim().length === 0) return null; const parsed = Number(value); return Number.isFinite(parsed) ? parsed : null; }
function firstNumberEvidence(...values: unknown[]): number | null { for (const value of values) { const parsed = numberEvidence(value); if (parsed !== null) return parsed; } return null; }
function timestampEvidence(value: unknown): number | null { const timestamp = numberEvidence(value); return timestamp === null || !Number.isFinite(new Date(timestamp).getTime()) ? null : timestamp; }
function firstPopulatedArray(...values: unknown[]): any[] { for (const value of values) if (Array.isArray(value) && value.length > 0) return value; return []; }
function uniqueText(values: unknown[]): string[] { return Array.from(new Set(values.map((value) => safeText(value, "")).filter(Boolean))); }
function transformNames(log: any): string[] { return arrayValue(log?.transforms_applied).map((transform) => safeText(transform, "").trim()).filter(Boolean); }
function safeText(value: unknown, fallback: string) { return typeof value === "string" && value.length > 0 ? value : fallback; }
