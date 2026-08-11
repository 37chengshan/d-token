import type { ActivityDay } from "../../components/common/Heatmap";
import type { ConnectionSetupSnapshot } from "../connectionSetup";
import type { Lang } from "../i18n"
import { translateCopy } from "../i18n";
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
    quotaWindow("five_hour", translateCopy("ov.5Hours", lang), claudeLatest.five_hour),
    quotaWindow("seven_day", translateCopy("ov.7Days", lang), claudeLatest.seven_day),
  ];
  const primary = objectValue(codexLatest.primary ?? codexLatest.primary_window);
  const secondary = objectValue(codexLatest.secondary ?? codexLatest.secondary_window);
  const codexWindows = [
    quotaWindow("primary", translateCopy("ov.5Hours", lang), primary, 300),
    quotaWindow("secondary", translateCopy("ov.7Days", lang), secondary, 10080),
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
        label: translateCopy("ov.running", lang),
        detail: safeText(
          payload?.desktop_service_url,
          translateCopy("ov.localServiceRespondedWithoutAListeningAddress", lang),
        ),
        tone: "positive",
      }
    : evidenceState === "loading"
      ? { label: translateCopy("set.checking2", lang), detail: translateCopy("ov.connectingToTheDTokenLocalService", lang), tone: "neutral" }
      : evidenceState === "preview"
        ? { label: translateCopy("cp.source.preview", lang), detail: translateCopy("ov.noRealLocalServiceIsConnected", lang), tone: "neutral" }
        : evidenceState === "stale"
          ? { label: translateCopy("ov.statusIsStale", lang), detail: translateCopy("ov.oldDataIsNotTreatedAsCurrentServiceStatus", lang), tone: "warning" }
          : evidenceState === "partial"
            ? { label: translateCopy("ov.statusIsIncomplete", lang), detail: translateCopy("ov.completeLocalServiceEvidenceIsMissing", lang), tone: "warning" }
            : { label: translateCopy("ov.notConnected", lang), detail: translateCopy("ov.theLocalServiceIsStoppedOrUnavailable", lang), tone: "negative" };

  const agents: OperationalStatus = setupFresh && setupConnectedAgents.length > 0
    ? { label: setupConnectedAgents.join("、"), detail: translateCopy("ov.theCurrentLocalConfigurationPointsToDToken", lang), tone: "positive" }
    : connection.status === "stale" && setupConnectedAgents.length > 0
      ? { label: setupConnectedAgents.join("、"), detail: translateCopy("ov.theLastCheckFoundLocalConfigPointingToDTokenConfigurationEvidenceIsStale", lang), tone: "warning" }
      : isRuntime && trafficAgents.length > 0
        ? { label: trafficAgents.join("、"), detail: translateCopy("ov.historicalRequestSExistTheyDoNotProveACurrentConnection", lang, { v1: requestLogs.length }), tone: "warning" }
        : !isRuntime
          ? {
              label: translateCopy("ov.noVerifiedConnection", lang),
              detail: setupDetectedAgents.length > 0
                ? (connection.status === "stale"
                    ? (translateCopy("ov.lastDetectedConfigurationEvidenceIsStale", lang, { v1: setupDetectedAgents.join("、"), v2: setupDetectedAgents.join(", ") }))
                    : (translateCopy("ov.detectedOnlyRealRequestsProveAConnection", lang, { v1: setupDetectedAgents.join("、"), v2: setupDetectedAgents.join(", ") })))
                : (translateCopy("ov.onlyLiveLocalRequestsCanProveAnAgentConnection", lang)),
              tone: "neutral",
            }
          : configuredAgents.length > 0
            ? { label: translateCopy("ov.noVerifiedConnectionYet", lang), detail: translateCopy("ov.configured", lang, { v1: configuredAgents.join("、"), v2: configuredAgents.join(", ") }), tone: "warning" }
            : setupDetectedAgents.length > 0
              ? { label: translateCopy("ov.noVerifiedConnectionYet", lang), detail: translateCopy("ov.detected", lang, { v1: setupDetectedAgents.join("、"), v2: setupDetectedAgents.join(", ") }), tone: "warning" }
              : { label: translateCopy("ov.notConnected2", lang), detail: translateCopy("ov.configureAnAgentAndSendOneRealRequest", lang), tone: isRuntime ? "warning" : "neutral" };

  const routeStatus: OperationalStatus = isRuntime && routeConfigured
    ? {
        label: [routeName, providerName].filter(Boolean).join(" → "),
        detail: requestLogs.length > 0
          ? (translateCopy("ov.recentRequestsPassedThroughDToken", lang))
          : (translateCopy("ov.configurationLoadedNoRequestEvidenceYet", lang)),
        tone: requestLogs.length > 0 ? "positive" : "warning",
      }
    : {
        label: isRuntime ? (translateCopy("ov.waitingForConnectionSetup", lang)) : (translateCopy("ov.noRealConnectionPath", lang)),
        detail: translateCopy("ov.theConnectionRemainsUnverifiedWithoutLiveConfigurationEvidence", lang),
        tone: isRuntime ? "warning" : "neutral",
      };

  const issues: OperationalStatus = !isRuntime
    ? { label: translateCopy("ov.noCurrentIssueEvidence", lang), detail: translateCopy("ov.connectTheLiveLocalServiceToInspectRecentIssues", lang), tone: "warning" }
    : issueCount > 0
      ? { label: translateCopy("ov.issueS", lang, { v1: issueCount }), detail: errorCount > 0 ? (translateCopy("ov.requestSFailed", lang, { v1: errorCount })) : (translateCopy("ov.connectionSetupIsIncomplete", lang)), tone: "negative" }
      : requestLogs.length === 0
        ? { label: translateCopy("ov.noRequestRecordsYet", lang), detail: translateCopy("ov.requestErrorsCanBeEvaluatedAfterRealTrafficArrives", lang), tone: "neutral" }
        : { label: translateCopy("ov.noIssuesFound", lang), detail: translateCopy("ov.basedOnCurrentLocalServiceRecords", lang), tone: "positive" };

  const ccSwitch = isRuntime
    ? deriveCcSwitchStatus(payload, providers, lang, setupAvailable ? connection.snapshot?.cc_switch_sync ?? null : null, setupFresh)
    : setupAvailable
      ? deriveCcSwitchStatus(payload, providers, lang, connection.snapshot?.cc_switch_sync ?? null, setupFresh)
      : { state: "unknown" as const, label: translateCopy("ov.noLiveVerificationYet", lang), detail: translateCopy("ov.nonLiveDataIsNeverShownAsSuccessfullySynced", lang) };
  return { service, agents, route: routeStatus, issues, ccSwitch };
}

export function overviewFromPayload(payload: Record<string, any> | null, status: string, lang: Lang = "zh") {
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
        ? (translateCopy("an.localServiceOnlineNoRequestRecords", lang))
        : evidenceState === "preview"
          ? (translateCopy("ov.browserPreviewNoLiveData", lang))
          : evidenceState === "stale"
            ? (translateCopy("ov.localDataIsStaleNotTreatedAsCurrent", lang))
            : evidenceState === "partial"
              ? (translateCopy("ov.localDataIsIncompleteCurrentStateUnverified", lang))
              : evidenceState === "loading"
                ? (translateCopy("an.connectingToTheLocalDTokenService", lang))
                : (translateCopy("an.localDTokenServiceUnavailable", lang)),
      stats: emptyOverviewStats,
      trend: [],
      activityDays: [],
      tools: [],
      activitySource: translateCopy("ov.waitingForLiveLocalRequestRecords", lang),
      errorLabel: translateCopy("ov.noCurrentErrorEvidence", lang),
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
      ? requestCount === 0 ? (translateCopy("ov.lastVerifiedNoRequestsDataIsStale", lang)) : (translateCopy("ov.lastVerifiedRequestSDataIsStale", lang, { v1: requestCount }))
      : trafficAvailable
        ? requestCount === 0 ? (translateCopy("an.localServiceOnlineNoRequestRecords", lang)) : (translateCopy("ov.localDTokenServiceRequestS", lang, { v1: requestCount }))
        : (translateCopy("an.localServiceOnlineNoRequestRecords", lang)),
    stats: { savedTotal: physicalSaved ?? 0, claimableSaved, compressionRate: savingsPercent === null ? 0 : Math.max(0, Math.min(1, savingsPercent / 100)), activeAgents, errorsToday: errorCount, requestCount },
    trend: buildTrendFromLogs(requestLogs),
    activityDays: buildActivityDaysFromLogs(requestLogs),
    tools: buildToolsFromLogs(requestLogs),
    activitySource: trafficAvailable
      ? evidenceState === "stale" ? (translateCopy("ov.lastVerifiedLocalDTokenRequestRecords", lang)) : (translateCopy("ov.localDTokenRequestRecords", lang))
      : (translateCopy("ov.waitingForLiveLocalRequestRecords", lang)),
    errorLabel: trafficAvailable
      ? evidenceState === "stale"
        ? errorCount > 0 ? (translateCopy("ov.failuresInLastVerifiedDataNotCurrent", lang, { v1: errorCount })) : (translateCopy("ov.noFailuresInLastVerifiedDataNotCurrent", lang))
        : errorCount > 0 ? (translateCopy("ov.requestFailuresRecorded", lang)) : (translateCopy("ov.noRequestFailuresRecorded", lang))
      : (translateCopy("ov.noLiveRequestRecords", lang)),
  };
}

const emptyOverviewStats = { savedTotal: 0, claimableSaved: 0, compressionRate: 0, activeAgents: 0, errorsToday: 0, requestCount: 0 };

function deriveCcSwitchStatus(payload: Record<string, any> | null, providers: any[], lang: Lang, connectionEvidence: Record<string, unknown> | null = null, connectionVerified = false): CcSwitchStatus {
  const evidence = objectValue(connectionEvidence ?? payload?.cc_switch_sync ?? payload?.ccSwitchSync);
  const state = safeText(evidence.status ?? evidence.state, "").toLowerCase();
  const verified = evidence.verified === true || evidence.evidence_verified === true || (connectionVerified && evidence.source === "cc-switch-read-only" && ["synced", "in_sync", "out-of-sync", "out_of_sync", "mismatch"].includes(state));
  const detected = evidence.detected === true || providers.some((provider) => ["cc_switch", "cc-switch", "ccswitch"].includes(safeText(provider?.kind, "").toLowerCase()));
  const evidenceDetail = uniqueText([evidence.profile, evidence.upstream, evidence.connection_path]).join(" · ");
  if (verified && (state === "synced" || state === "in_sync")) return { state: "synced", label: translateCopy("ov.syncVerified", lang), detail: evidenceDetail || (translateCopy("ov.backedByLocalDetectionEvidence", lang)) };
  if (verified && (state === "out-of-sync" || state === "out_of_sync" || state === "mismatch")) return { state: "out_of_sync", label: translateCopy("ov.connectionMismatch", lang), detail: evidenceDetail || (translateCopy("ov.confirmTheCurrentUpstreamInInstallConnect", lang)) };
  if (detected || Object.keys(evidence).length > 0) return { state: "unverified", label: translateCopy("ov.configurationDetectedSyncNotVerified", lang), detail: translateCopy("ov.noSyncTimeOrVerificationResultNotShownAsHealthy", lang) };
  return { state: "unknown", label: translateCopy("ov.notDetectedYet", lang), detail: translateCopy("ov.thirdPartySettingsAreNotGuessedOrRewritten", lang) };
}

function quotaWindow(key: string, label: string, value: unknown, expectedMinutes?: number): AccountQuotaWindow {
  const window = objectValue(value);
  const used = firstNumberEvidence(window.used_percent, window.utilization_pct, window.usedPercent);
  const minutes = firstNumberEvidence(window.window_minutes, window.windowMinutes, window.window_seconds !== undefined ? Number(window.window_seconds) / 60 : null, window.limit_window_seconds !== undefined ? Number(window.limit_window_seconds) / 60 : null);
  const validUsed = (expectedMinutes === undefined || minutes === expectedMinutes) && used !== null && used >= 0 && used <= 100 ? used : null;
  return { key, label, usedPercent: validUsed, resetsAt: normalizeQuotaReset(window.resets_at ?? window.reset_at ?? window.resetsAt) };
}

function quotaSource(value: Record<string, any>, lang: Lang) { return quotaSourceAllowed(value.source) ? (translateCopy("ov.localDTokenQuotaEvidence", lang)) : (translateCopy("ov.waitingForLocalQuotaEvidence", lang)); }
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
