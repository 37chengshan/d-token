import { AlertTriangle, ChevronDown, RefreshCw, Rocket, Server, Settings2, ShieldCheck } from "lucide-react";
import { translateCopy } from "../i18n";
import { Surface } from "../../components/common/Surface";
import { StatusBadge } from "../../components/common/StatusBadge";
import { DataSourceBadge } from "../../components/common/DataSourceBadge";
import { OperationDialog } from "../../components/common/OperationDialog";
import { Button } from "../../components/ui/button";
import { Badge } from "../../components/ui/badge";
import type { Health } from "../runtime/types";
import type { NativeActionLog } from "../native";
import type { AgentUpstreamBinding, ConnectionSetupSnapshot } from "../connectionSetup";
import {
  type InstallAgentReceipt,
  type InstallMachineEvent,
  type InstallMachineState,
} from "../installMachine";
import { redactString } from "../security/redaction";
import {
  AgentBindingRow,
  connectionPath,
  machineMessage,
  presentConnectionPrivacy,
  presentRouteVerificationSummary,
  shortHash,
  supportsAutomaticAgentIntegration,
} from "./presentation";
import type {
  ConnectionRoutePlanReview,
  DtokenServiceStatus,
  HybridAgentIntegrationReview,
  ManagedAgentTransportReview,
  OpenCodeConnectionReview,
  RouteVerification,
} from "./presentation";

type PendingAction = "discover" | "preview" | "apply" | "verify" | "restart" | "stop" | "credential" | "sync" | null;

export type InstallViewProps = {
  lang: "zh" | "en";
  t: (key: string) => string;
  snapshot: ConnectionSetupSnapshot | null;
  discoveryStatus: "idle" | "discovering" | "ready" | "stale" | "unavailable";
  phasePresentation: { health: Health; label: string; description: string };
  discoveryCounts: { agents: number; upstreams: number; ccSwitchProfiles: number };
  refreshAll: () => Promise<void>;
  busy: boolean;
  pendingAction: PendingAction;
  handlePrimaryAction: () => Promise<void>;
  primaryAction: { label: string; disabled: boolean; icon: string };
  serviceInstanceDetail: string;
  servicePresentation: { isOnline: boolean; status: Health; phase: string };
  restartRequired: boolean;
  serviceOnline: boolean;
  serviceManaged: boolean;
  setStopError: (value: string) => void;
  setConfirmServiceStop: (value: boolean) => void;
  visibleBindings: AgentUpstreamBinding[];
  primaryBindings: AgentUpstreamBinding[];
  machine: InstallMachineState;
  dispatch: (event: InstallMachineEvent) => void;
  secondaryBindings: AgentUpstreamBinding[];
  ccSwitchPresentation: { health: Health; label: string };
  notice: string;
  transactionLogs: NativeActionLog[];
  agentIntegrationReceipts: InstallAgentReceipt[];
  routeVerification: RouteVerification | null;
  showReview: boolean;
  setShowReview: (value: boolean) => void;
  setConnectionReview: (value: OpenCodeConnectionReview | null) => void;
  setRoutePlanReview: (value: ConnectionRoutePlanReview | null) => void;
  setManagedTransportReviews: (value: Record<string, ManagedAgentTransportReview>) => void;
  setAgentIntegrationReviews: (value: Record<string, HybridAgentIntegrationReview>) => void;
  actionableBindings: AgentUpstreamBinding[];
  serviceEndpoint: string;
  routePlanReview: ConnectionRoutePlanReview | null;
  connectionReview: OpenCodeConnectionReview | null;
  managedTransportReviews: Record<string, ManagedAgentTransportReview>;
  agentIntegrationReviews: Record<string, HybridAgentIntegrationReview>;
  openCodeSelection: { model: string } | null;
  applyConnectionPlan: () => Promise<void>;
  credentialDialogOpen: boolean;
  setCredentialDialogOpen: (value: boolean) => void;
  setProviderCredential: (value: string) => void;
  setCredentialError: (value: string) => void;
  credentialBinding: AgentUpstreamBinding | null;
  credentialProviderId: string;
  providerCredential: string;
  credentialError: string;
  saveInlineCredential: () => Promise<void>;
  confirmServiceStop: boolean;
  stopError: string;
  serviceStatus: DtokenServiceStatus | null;
  serviceIdentifiersCurrent: boolean;
  stopService: () => Promise<void>;
  transportBusyAgent: string | null;
  onConfigureAgent: (binding: AgentUpstreamBinding) => void;
  onClearAgent: (binding: AgentUpstreamBinding) => void;
  agentAutoFollow: boolean;
  setAgentAutoFollow: (value: boolean) => void;
  ccSwitchSyncReview: ConnectionRoutePlanReview | null;
  setCcSwitchSyncReview: (value: ConnectionRoutePlanReview | null) => void;
  ccSwitchAutoFollow: boolean;
  setCcSwitchAutoFollow: (value: boolean) => void;
  ccSwitchSyncBusy: boolean;
  onPreviewCcSwitchSync: () => void;
  onApplyCcSwitchSync: () => void;
};

export function InstallView(props: InstallViewProps) {
  const { lang, t, snapshot, discoveryStatus, phasePresentation, discoveryCounts, refreshAll, busy, pendingAction, handlePrimaryAction, primaryAction, serviceInstanceDetail, servicePresentation, restartRequired, serviceOnline, serviceManaged, setStopError, setConfirmServiceStop, visibleBindings, primaryBindings, machine, dispatch, secondaryBindings, ccSwitchPresentation, notice, transactionLogs, agentIntegrationReceipts, routeVerification, showReview, setShowReview, setConnectionReview, setRoutePlanReview, setManagedTransportReviews, setAgentIntegrationReviews, actionableBindings, serviceEndpoint, routePlanReview, connectionReview, managedTransportReviews, agentIntegrationReviews, openCodeSelection, applyConnectionPlan, credentialDialogOpen, setCredentialDialogOpen, setProviderCredential, setCredentialError, credentialBinding, credentialProviderId, providerCredential, credentialError, saveInlineCredential, confirmServiceStop, stopError, serviceStatus, serviceIdentifiersCurrent, stopService, transportBusyAgent, onConfigureAgent, onClearAgent, agentAutoFollow, setAgentAutoFollow, ccSwitchSyncReview, setCcSwitchSyncReview, ccSwitchAutoFollow, setCcSwitchAutoFollow, ccSwitchSyncBusy, onPreviewCcSwitchSync, onApplyCcSwitchSync } = props;
  return (
    <div className="page-enter mx-auto w-full max-w-6xl space-y-4">
      <Surface className="overflow-hidden">
        <div className="flex flex-wrap items-start justify-between gap-4 border-b border-border/60 px-6 py-5">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-xl font-semibold tracking-tight">{translateCopy("in.connectDToken", lang)}</h2>
              <StatusBadge status={phasePresentation.health} label={phasePresentation.label} />
              <DataSourceBadge
                mode={snapshot ? "native" : discoveryStatus === "unavailable" ? "unavailable" : "preview"}
                label={snapshot ? (translateCopy("in.localReadOnlyDiscovery", lang)) : undefined}
              />
            </div>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-muted-foreground">
              {phasePresentation.description}
            </p>
            {snapshot && (
              <p className="mt-1 text-xs text-muted-foreground">
                {translateCopy("in.automaticallyReadInstalledToolsAvailableUpstreamsAndCcSwitchProfiles", lang, { v1: discoveryCounts.agents, v2: discoveryCounts.upstreams, v3: discoveryCounts.ccSwitchProfiles })}
              </p>
            )}
          </div>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="icon" aria-label={translateCopy("in.refreshConnectionState", lang)} onClick={() => void refreshAll()} disabled={busy}>
              <RefreshCw className={`size-4 ${pendingAction === "discover" ? "animate-spin" : ""}`} />
            </Button>
            <Button onClick={() => void handlePrimaryAction()} disabled={busy || primaryAction.disabled}>
              {busy ? <RefreshCw className="size-4 animate-spin" /> : primaryAction.icon === "settings" ? <Settings2 className="size-4" /> : primaryAction.icon === "verify" ? <ShieldCheck className="size-4" /> : primaryAction.icon === "restart" ? <RefreshCw className="size-4" /> : <Rocket className="size-4" />}
              {primaryAction.label}
            </Button>
          </div>
        </div>

        <div className="divide-y divide-border/60 px-6">
          <div className="grid gap-4 py-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
            <div className="flex min-w-0 items-center gap-3">
              <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted/70"><Server className="size-4" /></span>
              <div className="min-w-0">
                <div className="font-medium">{translateCopy("in.dTokenLocalService", lang)}</div>
                <div className="mt-0.5 truncate text-xs text-muted-foreground">
                  {serviceInstanceDetail}
                </div>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <StatusBadge
                status={servicePresentation.status}
                label={restartRequired
                  ? (translateCopy("onb.service.restartRequired", lang))
                  : serviceOnline ? (translateCopy("status.running", lang)) : (translateCopy("in.stopped", lang))}
              />
              {serviceManaged && (
                <Button variant="ghost" size="sm" onClick={() => { setStopError(""); setConfirmServiceStop(true); }} disabled={busy}>
                  {translateCopy("set.stop", lang)}
                </Button>
              )}
            </div>
          </div>

          <div className="py-4">
            <div className="mb-3 flex items-center justify-between gap-3">
              <div>
                <div className="font-medium">{translateCopy("in.localAiCodingAgents", lang)}</div>
                <div className="mt-0.5 text-xs text-muted-foreground">
                  {translateCopy("in.eachAgentKeepsItsOwnProviderModelAndProtocol", lang)}
                </div>
              </div>
              <Badge variant="secondary">{visibleBindings.length} {translateCopy("in.detected", lang)}</Badge>
            </div>
            {discoveryStatus === "discovering" && !snapshot ? (
              <div className="space-y-2 py-2" aria-label={translateCopy("in.discoveringAgents", lang)}>
                {[0, 1, 2].map((item) => <div key={item} className="h-14 animate-pulse rounded-lg bg-muted/50" />)}
              </div>
            ) : (
              <div>
                <div className="divide-y divide-border/50">
                {primaryBindings.map((binding) => (
                  <AgentBindingRow
                    key={binding.agent_id}
                    binding={binding}
                    lang={lang}
                    busy={busy}
                    transportBusy={transportBusyAgent === binding.agent_id}
                    onConfigure={() => onConfigureAgent(binding)}
                    onClear={() => onClearAgent(binding)}
                  />
                ))}
                {primaryBindings.length === 0 && visibleBindings.length > 0 && (
                  <div className="py-4 text-sm text-muted-foreground">
                    {translateCopy("in.toolsWereDetectedButNoneExposesADTokenWriteActionNoConfigurationWasModified", lang, { v1: visibleBindings.length })}
                  </div>
                )}
                </div>
                {secondaryBindings.length > 0 && (
                  <details className="group border-t border-border/60 py-1">
                    <summary className="flex cursor-pointer list-none items-center justify-between gap-3 py-3 text-sm font-medium">
                      <span>{translateCopy("in.otherDetectedTools", lang, { v1: secondaryBindings.length })}</span>
                      <span className="flex items-center gap-1 text-xs font-normal text-muted-foreground">
                        {translateCopy("in.expandWhenNeeded", lang)}<ChevronDown className="size-3.5 transition-transform group-open:rotate-180" />
                      </span>
                    </summary>
                    <div className="divide-y divide-border/50 border-t border-border/50">
                      {secondaryBindings.map((binding) => (
                        <AgentBindingRow
                          key={binding.agent_id}
                          binding={binding}
                          lang={lang}
                          busy={busy}
                          transportBusy={transportBusyAgent === binding.agent_id}
                          onConfigure={() => onConfigureAgent(binding)}
                          onClear={() => onClearAgent(binding)}
                        />
                      ))}
                    </div>
                  </details>
                )}
                {visibleBindings.length === 0 && (
                  <div className="py-5 text-sm text-muted-foreground">
                    {translateCopy("in.noConnectableAiCodingAgentWasDetectedLocally", lang)}
                  </div>
                )}
              </div>
            )}
          </div>

          <div className="grid gap-4 py-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
            <div className="min-w-0">
              <div className="font-medium">cc-switch</div>
              <div className="mt-1 truncate text-xs text-muted-foreground">
                {snapshot?.cc_switch_sync?.detected
                  ? [snapshot.cc_switch_sync.profile, snapshot.cc_switch_sync.upstream, snapshot.cc_switch_sync.connection_path].filter(Boolean).join(" · ") || (translateCopy("in.detectedReadOnly", lang))
                  : (translateCopy("in.ccSwitchWasNotFoundThirdPartyConfigurationIsNeverInstalledOrRewrittenAutomatically", lang))}
              </div>
            </div>
            <div className="flex items-center gap-2">
              {snapshot?.cc_switch_sync?.status === "out-of-sync" && (
                <Button size="sm" variant="outline" disabled={busy || ccSwitchSyncBusy} onClick={onPreviewCcSwitchSync}>
                  {translateCopy("in.syncCurrentUpstream", lang)}
                </Button>
              )}
              <StatusBadge
                status={ccSwitchPresentation.health}
                label={ccSwitchPresentation.label}
              />
            </div>
          </div>
        </div>

        {(notice || machine.message) && (
          <div className="border-t border-border/60 bg-muted/25 px-6 py-3 text-sm text-muted-foreground" role="status">
            {notice || machineMessage(machine.message, lang)}
          </div>
        )}

        <details className="group border-t border-border/60 px-6 py-3 text-xs text-muted-foreground">
          <summary className="flex cursor-pointer list-none items-center gap-2 font-medium text-foreground">
            <ChevronDown className="size-3.5 transition-transform group-open:rotate-180" />
            {translateCopy("in.technicalEvidenceAndActionLog", lang)}
          </summary>
          <div className="mt-3 grid gap-2 font-mono text-[11px]">
            <div>revision {snapshot?.revision ?? "—"}</div>
            <div>source {snapshot?.source ?? "unavailable"}</div>
            <div>{presentConnectionPrivacy(snapshot?.privacy, lang)}</div>
            {transactionLogs.slice(-8).map((entry, index) => (
              <div key={`${entry.time}-${index}`} className={entry.level === "error" ? "text-rose-600" : entry.level === "warn" ? "text-amber-700" : ""}>
                [{entry.time}] {entry.level}: {redactString(entry.message)}
              </div>
            ))}
            {agentIntegrationReceipts.map((receipt) => (
              <div key={`${receipt.agentId}-${receipt.transactionId ?? receipt.state}`}>
                {receipt.agentName} · {receipt.state} · {redactString(receipt.summary)}
              </div>
            ))}
            {routeVerification && <div>{routeVerification.passed}/{routeVerification.total} · {presentRouteVerificationSummary(routeVerification, lang)}</div>}
          </div>
        </details>
      </Surface>

      <OperationDialog
        open={Boolean(ccSwitchSyncReview)}
        onOpenChange={(open) => { if (!open) setCcSwitchSyncReview(null); }}
        title={translateCopy("in.syncCurrentCcSwitchUpstream", lang)}
        summary={translateCopy("in.onlyDTokenSRouteChangesCcSwitchExistingAgentProvidersAndCredentialsAreNotRewritten", lang)}
        impact={ccSwitchSyncReview?.target ?? "d-token active route"}
        confirmLabel={translateCopy("in.confirmSyncAndReverify", lang)}
        cancelLabel={t("common.cancel")}
        pending={ccSwitchSyncBusy}
        confirmDisabled={!ccSwitchSyncReview?.ok}
        onConfirm={onApplyCcSwitchSync}
        details={ccSwitchSyncReview ? (
          <div className="space-y-3 text-[11px] text-muted-foreground">
            <label className="flex items-start gap-2 rounded-md border border-border/60 bg-background p-2">
              <input
                type="checkbox"
                checked={ccSwitchAutoFollow}
                disabled={ccSwitchSyncBusy}
                onChange={(event) => setCcSwitchAutoFollow(event.target.checked)}
                className="mt-0.5"
              />
              <span>
                {translateCopy("in.followCurrentUpstreamOnlyIdentitiesConfirmedInThisStepAndRequiringNoNewCredentialMaySyncAutomaticallyANewProviderEndpointOrProtocolStopsForConfirmation", lang)}
              </span>
            </label>
            <div className="font-mono">
            <div>before {shortHash(ccSwitchSyncReview.before_sha256)} → after {shortHash(ccSwitchSyncReview.after_sha256)}</div>
            {ccSwitchSyncReview.diff && <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-md bg-background p-2">{ccSwitchSyncReview.diff}</pre>}
            </div>
          </div>
        ) : null}
      />

      <OperationDialog
        open={showReview}
        onOpenChange={(open) => {
          setShowReview(open);
          if (!open) {
            setConnectionReview(null);
            setRoutePlanReview(null);
            setManagedTransportReviews({});
            setAgentIntegrationReviews({});
          }
        }}
        title={translateCopy("in.confirmConnectionPlan", lang)}
        summary={translateCopy("in.dTokenAppliesOnlyTheConfirmedChangesBelowAnySourceConfigurationChangeInvalidatesThisPlan", lang)}
        impact={actionableBindings.map((binding) => connectionPath(binding, machine.selections[binding.agent_id], serviceEndpoint)).join("\n")}
        details={(
          <div className="max-h-[46vh] space-y-3 overflow-y-auto pr-1 text-sm">
            {pendingAction === "preview" && <div className="flex items-center gap-2 text-muted-foreground"><RefreshCw className="size-4 animate-spin" />{translateCopy("in.buildingAHashBoundRedactedDiff", lang)}</div>}
            <label className="flex items-start gap-2 rounded-md border border-border/60 bg-background p-3 text-xs text-muted-foreground">
              <input
                type="checkbox"
                checked={agentAutoFollow}
                disabled={pendingAction === "apply" || pendingAction === "preview"}
                onChange={(event) => setAgentAutoFollow(event.target.checked)}
                className="mt-0.5"
              />
              <span>
                {translateCopy("in.followConfirmedAgentUpstreamsProviderModelProtocolOrEndpointChangesUpdateOnlyDTokenSRouteAndReverifyTheRuntimeAgentConfigurationAndCredentialsAreNotRewrittenAutomaticallyNewIdentitiesAmbiguityOrCredentialsStopForConfirmation", lang)}
              </span>
            </label>
            <div className="rounded-lg border border-border bg-muted/30 p-3 text-xs text-muted-foreground">
              <div className="font-medium">{translateCopy("in.rollbackBoundary", lang)}</div>
              <div className="mt-1">
                {translateCopy("in.aBackupAndRollbackPointAreCreatedBeforeApplyOnFailureTransactionReceiptsAreKeptSoAppliedItemsCanBeRolledBack", lang)}
              </div>
            </div>
            {routePlanReview && (
              <div className="rounded-lg border border-border bg-muted/30 p-3">
                <div className="font-medium">{translateCopy("in.automaticDTokenRoutes", lang)}</div>
                <div className="mt-1 text-xs text-muted-foreground">{routePlanReview.ok ? routePlanReview.summary : routePlanReview.error}</div>
                {routePlanReview.ok && (
                  <>
                    <div className="mt-2 font-mono text-[11px] text-muted-foreground">
                      <div>{routePlanReview.target}</div>
                      <div>revision {shortHash(routePlanReview.discovery_revision)}</div>
                      <div>before {shortHash(routePlanReview.before_sha256)} → after {shortHash(routePlanReview.after_sha256)}</div>
                    </div>
                    {routePlanReview.diff && <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-background p-2 text-[11px]">{routePlanReview.diff}</pre>}
                  </>
                )}
              </div>
            )}
            {connectionReview && (
              <div className="rounded-lg border border-border bg-muted/30 p-3">
                <div className="font-medium">OpenCode</div>
                <div className="mt-1 text-xs text-muted-foreground">{connectionReview.summary}</div>
                <div className="mt-2 font-mono text-[11px] text-muted-foreground">
                  <div>{connectionReview.target}</div>
                  <div>revision {shortHash(connectionReview.revision)}</div>
                  <div>before {shortHash(connectionReview.before_sha256)} → after {shortHash(connectionReview.after_sha256)}</div>
                </div>
                {connectionReview.diff && <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-background p-2 text-[11px]">{connectionReview.diff}</pre>}
              </div>
            )}
            {Object.entries(managedTransportReviews).map(([agentId, transport]) => {
              const binding = actionableBindings.find((item) => item.agent_id === agentId);
              const transportPlan = transport.review;
              return (
                <div key={`${agentId}-transport`} className="rounded-lg border border-border bg-muted/30 p-3">
                  <div className="font-medium">{binding?.agent_name ?? agentId} · {translateCopy("in.transportConfiguration", lang)}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {transport.ok && transportPlan
                      ? (translateCopy("in.onlyDTokenOwnedFieldsChangeWithTheOriginalConfigurationAndExactClearEvidencePreserved", lang))
                      : (translateCopy("in.thisConfigurationCannotBeChangedSafelySoNoWriteWillRun", lang))}
                  </div>
                  {transportPlan && (
                    <div className="mt-2 font-mono text-[11px] text-muted-foreground">
                      <div>{transportPlan.target_path}</div>
                      <div>before {shortHash(transportPlan.before_sha256)} → after {shortHash(transportPlan.after_sha256)}</div>
                      {transportPlan.redacted_diff && <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-background p-2">{transportPlan.redacted_diff}</pre>}
                    </div>
                  )}
                </div>
              );
            })}
            {Object.entries(agentIntegrationReviews).map(([agentId, review]) => {
              const binding = actionableBindings.find((item) => item.agent_id === agentId);
              return (
                <div key={agentId} className="rounded-lg border border-border bg-muted/30 p-3">
                  <div className="font-medium">{binding?.agent_name ?? agentId} · {translateCopy("in.agentIntegration", lang)}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {review.ok
                      ? (translateCopy("in.instructionsHookPluginAndSkillChangesAreBoundToTheHashesShownBelow", lang))
                      : review.error}
                  </div>
                  {review.plan?.targets.map((target) => (
                    <div key={`${agentId}-${target.target_path}`} className="mt-2 font-mono text-[11px] text-muted-foreground">
                      <div>{target.target_path}</div>
                      <div>before {shortHash(target.before_sha256)} → after {shortHash(target.after_sha256)}</div>
                      {target.diff && <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap rounded-md bg-background p-2">{target.diff}</pre>}
                    </div>
                  ))}
                </div>
              );
            })}
            {actionableBindings.filter((binding) => !supportsAutomaticAgentIntegration(binding.agent_id) && !binding.already_connected).map((binding) => (
              <div key={binding.agent_id} className="rounded-lg border border-amber-500/25 bg-amber-500/5 p-3">
                <div className="flex items-center gap-2 font-medium"><AlertTriangle className="size-4 text-amber-600" />{binding.agent_name}</div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {translateCopy("in.thisAdapterCurrentlyProvidesAResolvedSafeSetupStepAndNeverPretendsTheConfigWasAutoWritten", lang)}
                </div>
              </div>
            ))}
          </div>
        )}
        confirmLabel={translateCopy("cbm.confirmApplyVerify", lang)}
        cancelLabel={t("common.cancel")}
        pending={pendingAction === "apply" || pendingAction === "preview"}
        confirmDisabled={!routePlanReview?.ok
          || Boolean(openCodeSelection && !connectionReview?.ok)
          || Object.values(managedTransportReviews).some((transport) => !transport.ok || !transport.review)
          || Object.values(agentIntegrationReviews).some((review) => !review.ok || !review.plan?.review_digest)}
        onConfirm={applyConnectionPlan}
      />

      <OperationDialog
        open={credentialDialogOpen}
        onOpenChange={(open) => {
          setCredentialDialogOpen(open);
          if (!open) {
            setProviderCredential("");
            setCredentialError("");
          }
        }}
        title={translateCopy("in.addRequiredCredential", lang)}
        summary={translateCopy("in.theCredentialIsStoredOnlyInTheOperatingSystemCredentialStoreConfigurationKeepsAnOpaqueReference", lang)}
        impact={credentialBinding ? `${credentialBinding.agent_name} · ${credentialProviderId || "provider"}` : "provider"}
        confirmLabel={translateCopy("in.saveSecurelyAndContinue", lang)}
        cancelLabel={t("common.cancel")}
        pending={pendingAction === "credential"}
        confirmDisabled={!credentialProviderId || !providerCredential.trim()}
        onConfirm={saveInlineCredential}
        details={(
          <div className="space-y-2 text-sm">
            <label className="grid gap-1">
              <span>{translateCopy("in.credential", lang)}</span>
              <input
                autoComplete="off"
                className="h-9 rounded-md border border-border bg-background px-3"
                onChange={(event) => setProviderCredential(event.target.value)}
                type="password"
                value={providerCredential}
              />
            </label>
            {credentialError && <p className="text-destructive" role="alert">{credentialError}</p>}
          </div>
        )}
      />

      {confirmServiceStop && (
        <OperationDialog
          open
          onOpenChange={(open) => { setConfirmServiceStop(open); if (!open) setStopError(""); }}
          title={translateCopy("set.stopTheDTokenLocalService", lang)}
          summary={translateCopy("in.newAgentRequestsCannotPassThroughDTokenAfterStoppingConfigurationAndRollbackPointsRemain", lang)}
          impact={serviceStatus?.pid && serviceIdentifiersCurrent
            ? `PID ${serviceStatus.pid}${serviceEndpoint ? ` · ${serviceEndpoint}` : ""}`
            : serviceEndpoint || (translateCopy("in.currentInstance", lang))}
          confirmLabel={translateCopy("set.stopService", lang)}
          cancelLabel={t("common.cancel")}
          risk="warning"
          pending={pendingAction === "stop"}
          onConfirm={stopService}
          details={stopError ? <p className="text-sm text-destructive" role="alert">{stopError}</p> : null}
        />
      )}
    </div>
  );
}
