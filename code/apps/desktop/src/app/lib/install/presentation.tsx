import { Badge } from "../../components/ui/badge";
import { translateCopy } from "../i18n";
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
    ? (translateCopy("in.configureTools", lang))
    : (translateCopy("in.oneClickConfigure", lang));
  const clearLabel = isMcpOrHook
    ? (translateCopy("in.clearTools", lang))
    : (translateCopy("in.clearDTokenConfig", lang));
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
                ? (translateCopy("in.mcpToolsOnly", lang))
                : (translateCopy("in.hookToolsOnly", lang))}
            </Badge>
          )}
          {isMcpOrHook && (
            <Badge variant="outline" className="border-amber-500/30 text-amber-700 dark:text-amber-300">
              {translateCopy("in.toolCapabilityModelUpstreamUnchanged", lang)}
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
            {translateCopy("in.dTokenRoute", lang)} · {binding.follow.current_upstream.provider_id} · {binding.follow.current_upstream.model} · {binding.follow.current_upstream.protocol}
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
          title={!canConfigure ? (translateCopy("in.thisAgentExposesNoExecutableOneClickWriteAction", lang)) : undefined}
          onClick={onConfigure}
        >
          {transportBusy ? (translateCopy("in.working", lang)) : configureLabel}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={busy || transportBusy || !canClear}
          title={!canClear ? (translateCopy("in.noDTokenOwnershipReceiptIsAvailableForASafeClear", lang)) : undefined}
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
  if (!follow || follow.state === "not_authorized") {
    return {
      label: translateCopy("in.followOff", lang),
      nextAction: translateCopy("in.nextEnableFollowForConfirmedUpstreamsInTheConnectionPlan", lang),
    };
  }
  if (follow.state === "transport_out_of_sync") {
    return {
      label: translateCopy("in.transportOutOfSync", lang),
      nextAction: follow.next_action === "configure"
        ? (translateCopy("in.nextConfigureThisAgentAgainSoRequestsPassThroughDToken", lang))
        : (translateCopy("in.safeAutomaticWritesAreNotAvailableForThisVersionDTokenKeepsTheLastKnownGoodRoute", lang)),
    };
  }
  if (follow.state === "following" || follow.state === "applied_verified") {
    return {
      label: translateCopy("in.following", lang),
      nextAction: translateCopy("in.theAgentTransportAndDTokenRouteCurrentlyAgree", lang),
    };
  }
  if (follow.next_action === "verify_runtime") {
    return {
      label: translateCopy("in.runtimeVerificationNeeded", lang),
      nextAction: translateCopy("in.nextRestartOrReverifyTheDTokenRuntime", lang),
    };
  }
  if (follow.next_action === "review_follow") {
    return {
      label: translateCopy("in.confirmationNeeded", lang),
      nextAction: translateCopy("in.nextConfirmTheNewProviderEndpointOrProtocol", lang),
    };
  }
  return {
    label: translateCopy("in.followIncomplete", lang),
    nextAction: translateCopy("in.nextReopenTheConnectionPlanDTokenKeepsTheLastKnownGoodRouteOnFailure", lang),
  };
}

function presentAgentLocation(binding: AgentUpstreamBinding, lang: "zh" | "en") {
  if (!binding.installed) return translateCopy("in.notDetectedLocally", lang);
  const location = binding.binary_path || binding.config_path;
  if (!location) return translateCopy("cbm.agentDetected", lang);
  if (location.startsWith("<local>/")) return translateCopy("in.localAppDetected", lang);
  return location;
}

function presentCcSwitch(status: string | undefined, detected: boolean, sourceState: string | undefined, lang: "zh" | "en") {
  if (!detected) {
    const incompatible = ["unsupported_schema", "locked_or_corrupt", "unsupported_format"].includes(sourceState ?? "");
    return { health: "warning" as const, label: incompatible ? (translateCopy("in.detectedCompatibilityActionNeeded", lang)) : (translateCopy("onb.upstream.ccSwitchNone", lang)) };
  }
  if (status === "synced") return { health: "healthy" as const, label: translateCopy("in.synced", lang) };
  if (status === "out-of-sync") return { health: "warning" as const, label: translateCopy("in.outOfSync", lang) };
  return { health: "warning" as const, label: translateCopy("in.needsConfirmation", lang) };
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
      protocolLabel(resolved.protocol, lang) ?? (translateCopy("cbm.state.unknown", lang)),
      resolved.base_url,
      resolved.source,
    ].filter(Boolean).join(" · ");
  }
  if (lang === "en") return binding.reasons[0] || translateCopy("in.noReusableUpstreamConfigurationWasFound", lang);
  if (!binding.installed || binding.resolution === "unavailable") return translateCopy("in.noUsableAgentConfigurationDetected", lang, { agent: binding.agent_name });
  if (binding.resolution === "needs-choice") return translateCopy("in.multipleCandidateUpstreamsModelsCannotBeUniquelyResolvedFixTheSourceConfigurationAndRescan", lang);
  if (binding.resolution === "needs-credential") return translateCopy("in.upstreamFoundCredentialRequired", lang);
  return translateCopy("in.noReusableUpstreamConfigurationWasFound", lang);
}

function derivePrimaryAction(
  phase: InstallPhase,
  serviceOnline: boolean,
  reviewReady: boolean,
  lang: "zh" | "en",
  restartRequired = false,
) {
  if (["discovering", "applying-runtime", "applying-agent-configs", "verifying", "review"].includes(phase)) {
    return { label: phase === "discovering" ? (translateCopy("in.discovering", lang)) : (translateCopy("in.working2", lang)), disabled: true, icon: "rocket" };
  }
  if (restartRequired) return { label: translateCopy("in.restartService", lang), disabled: false, icon: "restart" };
  if (phase === "ready") return { label: translateCopy("in.viewChanges", lang), disabled: !reviewReady, icon: "rocket" };
  if (phase === "needs-credential") return { label: translateCopy("in.addRequiredCredential", lang), disabled: false, icon: "settings" };
  if (["already-connected", "verified"].includes(phase)) {
    return { label: serviceOnline ? (translateCopy("in.verifyAgain", lang)) : (translateCopy("in.startAndVerify", lang)), disabled: false, icon: "verify" };
  }
  return { label: translateCopy("in.scanAgain", lang), disabled: false, icon: "rocket" };
}

function presentInstallPhase(phase: InstallPhase, lang: "zh" | "en") {
  const map: Record<InstallPhase, { health: "healthy" | "warning" | "error"; label: string; description: string }> = {
    idle: { health: "warning", label: translateCopy("in.waiting", lang), description: translateCopy("in.localConfigurationIsReadAutomaticallyWhenThisViewOpens", lang) },
    discovering: { health: "warning", label: translateCopy("in.discovering2", lang), description: translateCopy("in.readingAgentConfigTheOpencodeCatalogCcSwitchAndDTokenStateWithoutWriting", lang) },
    "already-connected": { health: "healthy", label: translateCopy("sb.connected", lang), description: translateCopy("in.agentsAlreadyPointToDTokenIdenticalConfigurationWillNotBeWrittenAgain", lang) },
    ready: { health: "healthy", label: translateCopy("in.readyToContinue", lang), description: translateCopy("in.oneReliableConnectionPlanWasFoundViewTheChangesThenConfirmToContinue", lang) },
    "needs-credential": { health: "warning", label: translateCopy("in.credentialRequired", lang), description: translateCopy("in.addressProtocolAndModelArePreservedOnlyACredentialThatCannotBeReusedIsRequired", lang) },
    review: { health: "warning", label: translateCopy("in.awaitingConfirmation", lang), description: translateCopy("in.checkingTheSummaryDifferencesSourceHashAndRollbackBoundary", lang) },
    "applying-runtime": { health: "warning", label: translateCopy("in.startingService", lang), description: translateCopy("in.installingOrStartingTheDTokenRuntime", lang) },
    "applying-agent-configs": { health: "warning", label: translateCopy("in.applyingConfig", lang), description: translateCopy("in.applyingTheConfirmedAgentConfigurationBatch", lang) },
    verifying: { health: "warning", label: translateCopy("in.verifying", lang), description: translateCopy("in.confirmingThatConfigurationIsActiveAndCheckingServiceHealthAndRouteEvidence", lang) },
    verified: { health: "healthy", label: translateCopy("in.verified", lang), description: translateCopy("in.configurationServiceAndRouteEvidenceAgree", lang) },
    partial: { health: "warning", label: translateCopy("in.partial", lang), description: translateCopy("in.completedWorkWasKeptSafelyRemainingAgentsWereNotSilentlyModified", lang) },
    blocked: { health: "error", label: translateCopy("in.blocked", lang), description: translateCopy("in.thereIsNotEnoughEvidenceForASafePlanRescanOrUseAdvancedSetupAsAFallback", lang) },
    "rolled-back": { health: "warning", label: translateCopy("in.rolledBack", lang), description: translateCopy("in.thisAgentConfigBatchWasRestoredTheSafeDTokenServiceInstallationMayRemain", lang) },
  };
  return map[phase];
}

function presentBinding(binding: AgentUpstreamBinding, lang: "zh" | "en") {
  if (!binding.installed) return { health: "error" as const, label: translateCopy("onb.upstream.ccSwitchNone", lang) };
  if (binding.already_connected) return { health: "healthy" as const, label: translateCopy("sb.connected", lang) };
  if (binding.resolution === "auto-resolved" && binding.capability === "confirm-required") {
    return { health: "healthy" as const, label: translateCopy("in.readyNeedsConfirmation", lang) };
  }
  if (binding.resolution === "auto-resolved") return { health: "healthy" as const, label: translateCopy("in.autoResolved", lang) };
  if (binding.resolution === "needs-choice") return { health: "error" as const, label: translateCopy("onb.agents.conflict", lang) };
  if (binding.resolution === "needs-credential") return { health: "warning" as const, label: translateCopy("in.credential2", lang) };
  if (binding.transport_status === "advanced_required" || binding.transport_status === "schema_unverified") {
    return { health: "warning" as const, label: translateCopy("in.detectionOnly", lang) };
  }
  return { health: "warning" as const, label: translateCopy("in.awaitingSafeAdapter", lang) };
}

function capabilityLabel(binding: AgentUpstreamBinding, lang: "zh" | "en") {
  if (binding.can_configure === true) return translateCopy("in.oneClick2", lang);
  const { capability } = binding;
  if (capability === "one-click") return translateCopy("in.oneClick2", lang);
  if (capability === "confirm-required") return translateCopy("in.confirmToConnect", lang);
  if (capability === "prompt-only") return translateCopy("in.detectionOnly", lang);
  return translateCopy("in.writeLocked", lang);
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
  if (message === "native_unavailable" || message === "connection_setup_unavailable") return translateCopy("in.browserPreviewDoesNotReadOrModifyLocalConfiguration", lang);
  if (message === "source-config-changed") return translateCopy("in.theSourceConfigurationChangedSoThePreviousConnectionPlanIsNoLongerValid", lang);
  if (message === "agent-write-unavailable") return translateCopy("in.someAgentsAreDetectionOnlyInThisVersionTheirExistingConfigurationRemainsUnchanged", lang);
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
    return `${translateCopy("in.historicalRuntimeRecordIsStaleCurrentInstanceIdentifiersAreHidden", lang)}${suffix}`;
  }
  if (serviceStatus?.pid_running === false && serviceOnline) {
    return `${translateCopy("in.localListenerVerifiedManagedInstanceIdentifiersUnavailable", lang)}${suffix}`;
  }
  if (serviceOnline) {
    const instance = [serviceStatus?.pid ? `PID ${serviceStatus.pid}` : "", serviceEndpoint]
      .filter(Boolean)
      .join(" · ");
    return `${instance || (translateCopy("in.localListenerVerified", lang))}${suffix}`;
  }
  return `${translateCopy("in.noListeningInstanceDetected", lang)}${suffix}`;
}

export function presentConnectionPrivacy(privacy: string | undefined, lang: "zh" | "en") {
  if (lang === "zh") return translateCopy("in.privacyBoundary", lang);
  return privacy || translateCopy("in.privacyBoundary", lang);
}

export function presentRouteVerificationSummary(
  verification: Pick<RouteVerification, "ok" | "summary">,
  lang: "zh" | "en",
) {
  if (lang === "zh") {
    return verification.ok ? translateCopy("in.routeEvidenceConsistent", lang) : translateCopy("in.routeEvidenceNeedsAttention", lang);
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
