import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { useI18n, useT, translateCopy } from "../../lib/i18n";
import { invokeNative, type NativeActionLog, type NativeActionResult } from "../../lib/native";
import { useRuntimeDashboardSelector, shallowEqualSlice, type RuntimeDashboardState } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";
import {
  canReviewInstall,
  deriveInstallPhase,
  dispatchInstallMachine,
  getInstallMachineState,
  runInstallAgentBatch,
  subscribeInstallMachine,
  type InstallAgentReceipt,
  type InstallPhase,
} from "../../lib/installMachine";
import { useConnectionSetup, type AgentUpstreamBinding, type ConnectionSetupSnapshot, type CredentialUpstreamIdentity, type UpstreamCandidate } from "../../lib/connectionSetup";
import { InstallView } from "../../lib/install/InstallView";
import { buildConnectionRoutePlanRequest, connectionDiscoveryCounts, derivePrimaryAction, desktopInstallRoot, hasRealAgentAction, hybridAgentIntegrationDraft, presentCcSwitch, presentInstallPhase, presentRouteVerificationSummary, presentServiceInstance, safeText, supportsAutomaticAgentIntegration } from "../../lib/install/presentation";
import type { CompoundRouteOpencodeReceipt, CompoundRouteOpencodeReview, ConnectionRoutePlanReceipt, ConnectionRoutePlanReview, DtokenServiceStatus, HybridAgentIntegrationApply, HybridAgentIntegrationReview, HybridAgentIntegrationVerification, ManagedAgentTransportReceipt, ManagedAgentTransportReview, OpenCodeConnectionReceipt, OpenCodeConnectionReview, RouteVerification } from "../../lib/install/presentation";
export { connectionDiscoveryCounts, desktopInstallRoot, presentConnectionPrivacy, presentRouteVerificationSummary, presentServiceInstance } from "../../lib/install/presentation";
export { buildConnectionRoutePlanRequest } from "../../lib/install/presentation";

export function assertNativeActionSucceeded(result: NativeActionResult): void {
  if (!result.ok) throw new Error(result.summary || "native_action_failed");
}

export function deriveServicePresentation(serviceOnline: boolean, installing: boolean, loading = false, restartRequired = false) {
  if (restartRequired) return { isOnline: false, status: "warning" as const, phase: "restart-required" as const };
  if (serviceOnline) return { isOnline: true, status: "healthy" as const, phase: "running" as const };
  if (installing) return { isOnline: false, status: "warning" as const, phase: "installing" as const };
  if (loading) return { isOnline: false, status: "warning" as const, phase: "loading" as const };
  return { isOnline: false, status: "error" as const, phase: "stopped" as const };
}

/** Install reads summary and service evidence; detail slices must not repaint it. */
type InstallRuntimeSlice = {
  payload: RuntimeDashboardState["payload"];
  status: RuntimeDashboardState["status"];
  operational: RuntimeDashboardState["operational"];
  updatedAt: RuntimeDashboardState["updatedAt"];
  refresh: RuntimeDashboardState["refresh"];
};

const selectInstallRuntime = (state: RuntimeDashboardState): InstallRuntimeSlice => ({
  payload: state.payload,
  status: state.status,
  operational: state.operational,
  updatedAt: state.updatedAt,
  refresh: state.refresh,
});

export function Install({ onOpenAdvanced }: { onOpenAdvanced?: () => void; initialAgentId?: string } = {}) {
  const t = useT();
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const { snapshot, status: discoveryStatus, error: discoveryError, refresh: refreshDiscovery } = useConnectionSetup();
  const { payload, status: payloadStatus, operational, updatedAt, refresh: refreshRuntime } = useRuntimeDashboardSelector(
    selectInstallRuntime,
    shallowEqualSlice,
  );
  const machine = useSyncExternalStore(subscribeInstallMachine, getInstallMachineState, getInstallMachineState);
  const dispatch = dispatchInstallMachine;
  const [showReview, setShowReview] = useState(false);
  const [credentialDialogOpen, setCredentialDialogOpen] = useState(false);
  const [providerCredential, setProviderCredential] = useState("");
  const [credentialError, setCredentialError] = useState("");
  const [confirmServiceStop, setConfirmServiceStop] = useState(false);
  const [stopError, setStopError] = useState("");
  const [connectionReview, setConnectionReview] = useState<OpenCodeConnectionReview | null>(null);
  const [routePlanReview, setRoutePlanReview] = useState<ConnectionRoutePlanReview | null>(null);
  // One native review for a batch that includes OpenCode: the route and
  // OpenCode changes are prepared together, so apply echoes the single
  // compound revision and both hash pairs back to the compound apply.
  const [compoundReview, setCompoundReview] = useState<CompoundRouteOpencodeReview | null>(null);
  const [ccSwitchSyncReview, setCcSwitchSyncReview] = useState<ConnectionRoutePlanReview | null>(null);
  const [ccSwitchAutoFollow, setCcSwitchAutoFollow] = useState(true);
  const [agentAutoFollow, setAgentAutoFollow] = useState(true);
  const [managedTransportReviews, setManagedTransportReviews] = useState<Record<string, ManagedAgentTransportReview>>({});
  const [agentIntegrationReviews, setAgentIntegrationReviews] = useState<Record<string, HybridAgentIntegrationReview>>({});
  const [agentIntegrationReceipts, setAgentIntegrationReceipts] = useState<InstallAgentReceipt[]>([]);
  const [routeVerification, setRouteVerification] = useState<RouteVerification | null>(null);
  const [transactionLogs, setTransactionLogs] = useState<NativeActionLog[]>([]);
  const [notice, setNotice] = useState("");
  const [pendingAction, setPendingAction] = useState<"discover" | "preview" | "apply" | "verify" | "restart" | "stop" | "credential" | "sync" | null>(null);
  const [transportBusyAgent, setTransportBusyAgent] = useState<string | null>(null);
  // Previewed managed write confirmations (M-3): the preview → confirm →
  // apply step for a single Agent's transport or MCP integration now goes
  // through the unified OperationDialog instead of the browser window.confirm.
  const [transportConfirm, setTransportConfirm] = useState<{
    binding: AgentUpstreamBinding;
    operation: "configure" | "clear";
    request: { agent_id: string; target_path: string; endpoint: string; enabled: boolean; model?: string };
    review: { revision: string; before_sha256: string; after_sha256: string };
  } | null>(null);
  const [mcpConfirm, setMcpConfirm] = useState<{
    binding: AgentUpstreamBinding;
    operation: "configure" | "clear";
    request: { agent_id: string; config_path: string; operation: "configure" | "clear" };
    review: { before_sha256: string; after_sha256: string };
  } | null>(null);

  const serviceStatus = operational?.service?.data as DtokenServiceStatus | null;
  const serviceManifestStale = serviceStatus?.manifest_stale === true;
  const serviceIdentifiersCurrent = !serviceManifestStale && serviceStatus?.pid_running !== false;
  const restartRequired = serviceStatus?.state === "restart_required" || serviceStatus?.restart_required === true;
  const serviceOnline = !restartRequired && (serviceStatus
    ? ["running", "unmanaged_running"].includes(serviceStatus.state)
    : payloadStatus === "runtime");
  const serviceManaged = serviceStatus?.state === "running"
    && serviceStatus.managed === true
    && serviceIdentifiersCurrent;
  const serviceEndpoint = serviceIdentifiersCurrent
    ? serviceStatus?.service_url || safeText(payload?.desktop_service_url, "")
    : "";
  const serviceInstanceDetail = presentServiceInstance(
    serviceStatus,
    serviceOnline,
    serviceEndpoint,
    updatedAt,
    lang,
  );
  const servicePresentation = deriveServicePresentation(
    serviceOnline,
    ["applying-runtime"].includes(machine.phase),
    payloadStatus === "loading" && serviceStatus === null,
    restartRequired,
  );
  const visibleBindings = useMemo(
    () => (snapshot?.agent_bindings ?? []).filter((binding) =>
      binding.installed || binding.already_connected),
    [snapshot?.agent_bindings],
  );
  const actionableBindings = useMemo(
    () => (snapshot?.agent_bindings ?? []).filter((binding) => machine.selectedAgentIds.includes(binding.agent_id)),
    [machine.selectedAgentIds, snapshot?.agent_bindings],
  );
  const credentialBinding = actionableBindings.find((binding) => binding.resolution === "needs-credential") ?? null;
  const credentialProviderId = credentialBinding?.resolved_candidate?.id
    ?? credentialBinding?.candidates[0]?.id
    ?? "";
  const reviewReady = snapshot ? canReviewInstall(snapshot, machine.selections, machine.selectedAgentIds) : false;
  // One route plan for the whole page: preview and apply must agree on the
  // exact rows. OpenCode contributes one row per safe catalog candidate.
  const routePlan = useMemo(
    () => (snapshot
      ? buildConnectionRoutePlanRequest(snapshot.revision, actionableBindings, machine.selections)
      : null),
    [snapshot, actionableBindings, machine.selections],
  );
  // The union of OpenCode models entering the plan (already-validated by
  // resolveAgentBindings inside the route plan). preview/apply_opencode_
  // connection receive the whole array; a non-empty array is required before
  // the OpenCode review is requested.
  const openCodeModels = useMemo(
    () => (routePlan?.bindings
      .filter((binding) => binding.agent_id === "opencode")
      .map((binding) => binding.model) ?? []),
    [routePlan],
  );
  const primaryBindings = useMemo(
    // Only Agents with a real, verified backend write action are shown as
    // configurable rows. Detected-but-not-writable Agents stay in the counts
    // and technical evidence instead of getting fake buttons.
    () => visibleBindings.filter(hasRealAgentAction),
    [visibleBindings],
  );
  const secondaryBindings = useMemo(
    () => [] as AgentUpstreamBinding[],
    [],
  );
  const busy = pendingAction !== null || ["applying-runtime", "applying-agent-configs", "verifying"].includes(machine.phase);
  const discoveryCounts = useMemo(() => connectionDiscoveryCounts(snapshot), [snapshot]);
  const phasePresentation = restartRequired
    ? {
        health: "warning" as const,
        label: translateCopy("onb.service.restartRequired", lang),
        description: translateCopy("in.theRuntimeOnDiskWasUpdatedButTheCurrentProcessIsStaleRestartAndReverifyItBeforeContinuing", lang),
      }
    : presentInstallPhase(machine.phase, lang);
  const primaryAction = derivePrimaryAction(machine.phase, serviceOnline, reviewReady, lang, restartRequired);
  const ccSwitchPresentation = presentCcSwitch(snapshot?.cc_switch_sync?.status, Boolean(snapshot?.cc_switch_sync?.detected), snapshot?.cc_switch_sync?.source_state, lang);

  useEffect(() => {
    if (snapshot) dispatch({ type: "DISCOVERED", snapshot });
  }, [snapshot]);

  useEffect(() => {
    if (!snapshot?.external_config_refresh?.review_required) return;
    // A third-party Agent or cc-switch config changed after discovery. Do not
    // let a review built from the earlier fingerprint reach apply; refresh is
    // read-only and the user must explicitly review a new d-token plan.
    setShowReview(false);
    setConnectionReview(null);
    setRoutePlanReview(null);
    setCompoundReview(null);
    setCcSwitchSyncReview((current) =>
      current?.source_fingerprint === snapshot.external_config_snapshot?.cc_switch?.fingerprint
        ? current
        : null);
    setAgentIntegrationReviews({});
    const sources = snapshot.external_config_refresh.changed_sources.join(", ");
    setNotice(translateCopy("in.externalConfigurationChangedTheOldPlanIsStaleRescanAndReviewAgain", lang, { v1: sources ? `（${sources}）` : "", v2: sources ? ` (${sources})` : "" }));
  }, [lang, snapshot?.external_config_refresh]);

  useEffect(() => {
    if (discoveryStatus === "idle") {
      dispatch({ type: "DISCOVER_START" });
      void refreshDiscovery();
    } else if (discoveryStatus === "discovering") {
      dispatch({ type: "DISCOVER_START" });
    }
  }, [discoveryStatus, refreshDiscovery]);

  useEffect(() => {
    if (discoveryError) dispatch({ type: "DISCOVERY_FAILED", message: discoveryError });
  }, [discoveryError]);

  const refreshAll = async () => {
    setPendingAction("discover");
    const requestId = createOperationRequestId("connection-refresh");
    setNotice(translateCopy("in.refreshingAgentsUpstreamsCcSwitchAndDTokenState", lang));
    dispatch({ type: "DISCOVER_START" });
    try {
      const [discovery] = await Promise.all([refreshDiscovery(), refreshRuntime()]);
      setNotice(discovery
        ? (translateCopy("in.localConnectionStateRefreshed", lang))
        : (translateCopy("in.browserPreviewDoesNotReadOrModifyLocalConfiguration", lang)));
      addNotification({
        dedupeKey: "connection-refresh",
        level: discovery ? "success" : "info",
        title: discovery ? (translateCopy("in.connectionStateRefreshed", lang)) : (translateCopy("in.connectionStateUnavailable", lang)),
        message: discovery ? (translateCopy("in.agentUpstreamCcSwitchAndDTokenStateWereRefreshed", lang)) : (translateCopy("in.browserPreviewDoesNotReadLocalConfiguration", lang)),
        receipt: { requestId, targetId: "connection-discovery" },
      });
    } catch (error) {
      const failure = presentOperationFailure("connection", error, lang);
      setNotice(failure.message);
      addNotification({ dedupeKey: "connection-refresh-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "connection-discovery" } });
    } finally {
      setPendingAction(null);
    }
  };

  const installRuntime = async () => {
    dispatch({ type: "SET_PHASE", phase: "applying-runtime" });
    const result = await invokeNative<NativeActionResult>("run_install_transaction", {
      request: {
        target: "dtoken",
        install_root: desktopInstallRoot(typeof navigator === "undefined" ? "" : navigator.userAgent),
        proxy_port: 18001,
        auto_backup: true,
        run_doctor_after: true,
        rollback_point: true,
        mode: "connect",
      },
    });
    setTransactionLogs(result.logs);
    assertNativeActionSucceeded(result);
    await refreshRuntime();
  };

  const previewConnectionPlan = async () => {
    if (!snapshot || !reviewReady) return;
    dispatch({ type: "OPEN_REVIEW" });
    setShowReview(true);
    setConnectionReview(null);
    setRoutePlanReview(null);
    setCompoundReview(null);
    setManagedTransportReviews({});
    setAgentIntegrationReviews({});
    const requestId = createOperationRequestId("connection-preview");
    if (!routePlan || routePlan.bindings.length === 0) return;
    setPendingAction("preview");
    try {
      const integrationBindings = actionableBindings.filter((binding) =>
        supportsAutomaticAgentIntegration(binding.agent_id) && !binding.already_connected,
      );
      const managedTransportBindings = actionableBindings.filter((binding) =>
        binding.agent_id !== "opencode"
        && binding.can_configure === true
        && !binding.already_connected
        && Boolean(binding.config_path),
      );
      const [compound, managedTransportReviewEntries, integrationReviewEntries] = await Promise.all([
        // A batch with OpenCode uses one compound preview: the native side
        // re-discovers, prepares the active route change AND the OpenCode
        // provider change, and returns one compound revision plus both
        // diffs/hashes. Batches without OpenCode keep the route-only preview.
        openCodeModels.length > 0
          ? invokeNative<CompoundRouteOpencodeReview>("preview_compound_route_opencode", {
              request: {
                plan: routePlan,
                opencode: { models: openCodeModels, listener_port: 18001 },
                // The preview projects the follow-safe subset so the review
                // can show which routes would be skipped for auto-follow
                // before confirmation: OpenCode multi-route follow is never
                // authorized, and non-OpenCode follow stays unchanged.
                enable_auto_follow: agentAutoFollow,
              },
            })
          : Promise.resolve(null),
        Promise.all(managedTransportBindings.map(async (binding) => {
          const model = machine.selections[binding.agent_id]?.model;
          if (!binding.config_path || !model) {
            return [binding.agent_id, {
              ok: false,
              state: "blocked",
              summary: "managed_transport_target_unavailable",
            }] as const;
          }
          return [
            binding.agent_id,
            await invokeNative<ManagedAgentTransportReview>("preview_managed_agent_transport", {
              request: {
                agent_id: binding.agent_id,
                target_path: binding.config_path,
                endpoint: serviceEndpoint,
                enabled: true,
                model,
              },
              operation: "configure",
            }),
          ] as const;
        })),
        Promise.all(integrationBindings.map(async (binding) => [
          binding.agent_id,
          await invokeNative<HybridAgentIntegrationReview>("preview_hybrid_agent_integration", {
            request: hybridAgentIntegrationDraft(binding.agent_id),
          }),
        ] as const)),
      ]);
      let routeReview: ConnectionRoutePlanReview | null = null;
      let openCodeReview: OpenCodeConnectionReview | null = null;
      let nextCompoundReview: CompoundRouteOpencodeReview | null = null;
      if (compound) {
        // The compound review keeps one visible confirmation; the split
        // views are populated from its nested route/opencode reviews.
        nextCompoundReview = compound;
        routeReview = compound.route ?? null;
        openCodeReview = compound.opencode ?? null;
      } else {
        routeReview = await invokeNative<ConnectionRoutePlanReview>("preview_connection_route_plan", {
          request: routePlan,
        });
      }
      setRoutePlanReview(routeReview);
      setConnectionReview(openCodeReview);
      setCompoundReview(nextCompoundReview);
      setManagedTransportReviews(Object.fromEntries(managedTransportReviewEntries));
      setAgentIntegrationReviews(Object.fromEntries(integrationReviewEntries));
      if (!routeReview?.ok) {
        const failure = presentOperationFailure("connection", routeReview?.error, lang);
        setNotice(failure.message);
        dispatch({ type: "SET_PHASE", phase: "blocked", message: failure.message });
        addNotification({ dedupeKey: "connection-preview-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "connection-plan" } });
      } else {
        addNotification({ dedupeKey: `connection-preview-${routeReview.after_sha256 ?? "ready"}`, level: "info", title: translateCopy("in.connectionChangesAreReady", lang), message: translateCopy("in.differencesAndVerificationEvidenceAreReadyViewTheCurrentChangesBeforeApplying", lang), receipt: { requestId, targetId: "connection-plan" } });
      }
    } catch (error) {
      const failure = presentOperationFailure("connection", error, lang);
      setNotice(failure.message);
      dispatch({ type: "SET_PHASE", phase: "blocked", message: failure.message });
      addNotification({ dedupeKey: "connection-preview-error", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "connection-plan" } });
    } finally {
      setPendingAction(null);
    }
  };

  const applyConnectionPlan = async () => {
    if (!snapshot || machine.reviewDiscoveryRevision !== snapshot.revision) {
      setShowReview(false);
      setConnectionReview(null);
      setCompoundReview(null);
      setNotice(translateCopy("in.sourceConfigurationChangedTheOldPlanWasInvalidatedAndWillBeRebuilt", lang));
      await refreshAll();
      return;
    }
    setPendingAction("apply");
    const requestId = createOperationRequestId("connection-apply");
    if (!routePlan) return;
    const routeChanges = routePlanReview?.before_sha256 !== routePlanReview?.after_sha256;
    const wasManaged = serviceManaged;
    const hasCompound = openCodeModels.length > 0;
    let routeReceipt: ConnectionRoutePlanReceipt | null = null;
    let openCodeReceipt: OpenCodeConnectionReceipt | null = null;
    let compoundReceipt: CompoundRouteOpencodeReceipt | null = null;
    let appliedManagedTransports: AgentUpstreamBinding[] = [];
    let appliedAgentReceipts: InstallAgentReceipt[] = [];
    let rolledBack = false;
    try {
      if (!routePlanReview?.ok || !routePlanReview.before_sha256 || !routePlanReview.after_sha256) {
        throw new Error("connection_preview_incomplete");
      }
      if (routeChanges && serviceOnline && !serviceManaged) {
        throw new Error(translateCopy("in.port18001IsOwnedByAnUnmanagedDTokenServiceSoTheRouteCannotBeReloadedSafely", lang));
      }
      if (routeChanges && serviceManaged) {
        const stopped = await invokeNative<NativeActionResult>("stop_dtoken_service");
        assertNativeActionSucceeded(stopped);
      }
      if (hasCompound) {
        // One native compound transaction applies the active route change
        // and the OpenCode provider change together: both files share one
        // backup set and any apply failure reverses the prior change. The
        // exact compound revision and both hash pairs must match the review.
        if (!compoundReview?.ok || !compoundReview.compound_revision
          || !compoundReview.route?.before_sha256 || !compoundReview.route?.after_sha256
          || !compoundReview.opencode?.before_sha256 || !compoundReview.opencode?.after_sha256) {
          throw new Error("opencode_preview_incomplete");
        }
        compoundReceipt = await invokeNative<CompoundRouteOpencodeReceipt>("apply_compound_route_opencode", {
          request: {
            plan: routePlan,
            opencode: { models: openCodeModels, listener_port: 18001 },
            reviewed_compound_revision: compoundReview.compound_revision,
            reviewed_route_before_sha256: compoundReview.route.before_sha256,
            reviewed_route_after_sha256: compoundReview.route.after_sha256,
            reviewed_opencode_before_sha256: compoundReview.opencode.before_sha256,
            reviewed_opencode_after_sha256: compoundReview.opencode.after_sha256,
            enable_auto_follow: agentAutoFollow,
          },
        });
        if (!compoundReceipt.ok || !compoundReceipt.verified) throw new Error(compoundReceipt.summary);
        routeReceipt = {
          ok: compoundReceipt.ok,
          state: compoundReceipt.state,
          verified: compoundReceipt.verified,
          summary: compoundReceipt.summary,
          rollback_id: compoundReceipt.route?.rollback_id ?? null,
          before_sha256: compoundReceipt.route?.before_sha256 ?? null,
          after_sha256: compoundReceipt.route?.after_sha256 ?? null,
          auto_follow: compoundReceipt.auto_follow ?? undefined,
        };
        openCodeReceipt = {
          ok: compoundReceipt.ok,
          state: compoundReceipt.state,
          verified: compoundReceipt.verified,
          summary: compoundReceipt.summary,
          rollback_id: compoundReceipt.opencode?.rollback_id ?? null,
        };
      } else {
        // Route-only batches keep the single-command behavior.
        routeReceipt = await invokeNative<ConnectionRoutePlanReceipt>("apply_connection_route_plan", {
          request: {
            plan: routePlan,
            reviewed_before_sha256: routePlanReview.before_sha256,
            reviewed_after_sha256: routePlanReview.after_sha256,
            enable_auto_follow: agentAutoFollow,
          },
        });
        if (!routeReceipt.ok || !routeReceipt.verified) throw new Error(routeReceipt.summary);
      }
      // Runtime install/start/reload happens only after the compound (or
      // route-only) receipt is verified; the native apply never starts it.
      if (!serviceOnline || routeChanges) await installRuntime();

      dispatch({ type: "SET_PHASE", phase: "applying-agent-configs" });

      const managedTransportBindings = actionableBindings.filter((binding) =>
        binding.agent_id !== "opencode"
        && binding.can_configure === true
        && !binding.already_connected
        && Boolean(binding.config_path),
      );
      for (const binding of managedTransportBindings) {
        const review = managedTransportReviews[binding.agent_id]?.review;
        const model = machine.selections[binding.agent_id]?.model;
        if (!binding.config_path || !model || !review) {
          throw new Error("managed_transport_preview_incomplete");
        }
        const receipt = await invokeNative<ManagedAgentTransportReceipt>("apply_reviewed_managed_agent_transport", {
          request: {
            agent_id: binding.agent_id,
            target_path: binding.config_path,
            endpoint: serviceEndpoint,
            enabled: true,
            model,
          },
          operation: "configure",
          reviewedRevision: review.revision,
          reviewedBeforeSha256: review.before_sha256,
          reviewedAfterSha256: review.after_sha256,
        });
        if (receipt.ok === false || receipt.state === "failed" || receipt.verification?.ok === false) {
          throw new Error("managed_transport_apply_failed");
        }
        appliedManagedTransports.push(binding);
      }

      const agentBatch = await runInstallAgentBatch(
        actionableBindings
          .filter((binding) => supportsAutomaticAgentIntegration(binding.agent_id))
          .map((binding) => ({
            agentId: binding.agent_id,
            agentName: binding.agent_name,
            alreadyConnected: binding.already_connected,
            reviewDigest: agentIntegrationReviews[binding.agent_id]?.plan?.review_digest,
          })),
        {
          apply: async (item) => {
            const result = await invokeNative<HybridAgentIntegrationApply>("apply_hybrid_agent_integration", {
              request: {
                draft: hybridAgentIntegrationDraft(item.agentId),
                reviewed_digest: item.reviewDigest,
              },
            });
            return {
              ok: result.ok,
              transactionId: result.receipt?.transaction_id,
              summary: result.summary || result.error || "Agent integration apply failed",
            };
          },
          verify: async (receipt) => {
            const result = await invokeNative<HybridAgentIntegrationVerification>("verify_hybrid_agent_integration", {
              request: { transaction_id: receipt.transactionId },
            });
            return { ok: result.ok, summary: result.summary || result.state };
          },
          rollback: async (receipt) => {
            const result = await invokeNative<HybridAgentIntegrationApply>("rollback_hybrid_agent_integration", {
              request: { transaction_id: receipt.transactionId },
            });
            return { ok: result.ok, summary: result.summary || result.error || result.state };
          },
        },
      );
      appliedAgentReceipts = agentBatch.receipts;
      setAgentIntegrationReceipts(agentBatch.receipts);
      if (!agentBatch.ok) throw new Error(agentBatch.error);

      dispatch({ type: "SET_PHASE", phase: "verifying" });
      const verification = await invokeNative<RouteVerification>("verify_agent_route_config");
      setRouteVerification(verification);
      if (!verification.ok) throw new Error(verification.summary);
      const fullyVerified = (openCodeReceipt === null || openCodeReceipt.verified)
        && agentBatch.ok;
      if (
        fullyVerified
        && agentAutoFollow
        && (routeReceipt.auto_follow?.state === "pending_runtime_verification"
          || routeReceipt.auto_follow?.state === "partial")
        && routeReceipt.after_sha256
      ) {
        const follow = await invokeNative<{ ok: boolean; state: string; summary?: string }>(
          "finalize_agent_auto_follow_authorization",
          {
            request: {
              expected_route_sha256: routeReceipt.after_sha256,
            },
          },
        );
        if (!follow.ok) throw new Error(follow.summary || "agent_auto_follow_authorization_failed");
      }
      dispatch({
        type: "SET_PHASE",
        phase: fullyVerified ? "verified" : "partial",
        message: fullyVerified ? "" : "agent-write-unavailable",
      });
      setShowReview(false);
      setNotice(fullyVerified
        ? (translateCopy("in.configurationIsActiveTheLocalServiceIsHealthyAndTheRouteIsAvailable", lang))
        : (translateCopy("in.dTokenIsReadyAgentsWithoutAVerifiedSafeAdapterRetainTheirExistingConfiguration", lang)));
      addNotification({
        dedupeKey: `connection-plan-${fullyVerified ? "verified" : "partial"}`,
        receipt: { requestId },
        level: fullyVerified ? "success" : "warning",
        title: fullyVerified ? (translateCopy("in.agentConnectionVerified", lang)) : (translateCopy("in.connectionPartiallyComplete", lang)),
        message: fullyVerified
          ? (translateCopy("in.configurationServiceAndRouteEvidencePassedVerification", lang))
          : (translateCopy("in.theServiceAndVerifiedChangesWereKeptUnsupportedAgentConfigsWereNotSilentlyModified", lang)),
        persistent: !fullyVerified,
      });
      await Promise.all([refreshRuntime(), refreshDiscovery()]);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const failure = presentOperationFailure("connection", error, lang);
      for (const receipt of [...appliedAgentReceipts].reverse()) {
        if (!receipt.transactionId || !["applied", "verified"].includes(receipt.state)) continue;
        const restored = await invokeNative<HybridAgentIntegrationApply>("rollback_hybrid_agent_integration", {
          request: { transaction_id: receipt.transactionId },
        }).catch(() => null);
        rolledBack = rolledBack || restored?.ok === true;
      }
      for (const binding of [...appliedManagedTransports].reverse()) {
        if (!binding.config_path) continue;
        const request = {
          agent_id: binding.agent_id,
          target_path: binding.config_path,
          endpoint: serviceEndpoint || "http://127.0.0.1:18001",
          enabled: false,
        };
        const preview = await invokeNative<ManagedAgentTransportReview>("preview_managed_agent_transport", {
          request,
          operation: "clear",
        }).catch(() => null);
        const review = preview?.review;
        if (!review) continue;
        const restored = await invokeNative<ManagedAgentTransportReceipt>("apply_reviewed_managed_agent_transport", {
          request,
          operation: "clear",
          reviewedRevision: review.revision,
          reviewedBeforeSha256: review.before_sha256,
          reviewedAfterSha256: review.after_sha256,
        }).catch(() => null);
        rolledBack = rolledBack || (restored?.verification?.ok === true && restored?.state !== "failed");
      }
      if (compoundReceipt?.ok && compoundReceipt.route?.rollback_id && compoundReceipt.opencode?.rollback_id) {
        // Compound rollback restores both files in reverse order using the
        // exact per-file rollback targets from the compound receipt, and
        // reports all-or-fail truthfully.
        if (routeChanges) await invokeNative<NativeActionResult>("stop_dtoken_service").catch(() => null);
        const restored = await invokeNative<NativeActionResult>("rollback_compound_route_opencode", {
          request: {
            route: {
              rollback_id: compoundReceipt.route.rollback_id,
              expected_after_sha256: compoundReceipt.route.after_sha256 ?? "",
            },
            opencode: {
              rollback_id: compoundReceipt.opencode.rollback_id,
              expected_after_sha256: compoundReceipt.opencode.after_sha256 ?? "",
            },
          },
        }).catch(() => null);
        rolledBack = rolledBack || restored?.ok === true;
        if (restored?.ok && wasManaged) await installRuntime().catch(() => undefined);
      } else {
        if (openCodeReceipt?.ok && openCodeReceipt.rollback_id) {
          const restored = await invokeNative<NativeActionResult>("manage_backup", { action: "restore", id: openCodeReceipt.rollback_id }).catch(() => null);
          rolledBack = rolledBack || restored?.ok === true;
        }
        if (routeReceipt?.ok && routeReceipt.rollback_id && routeReceipt.after_sha256) {
          if (routeChanges) await invokeNative<NativeActionResult>("stop_dtoken_service").catch(() => null);
          const restored = await invokeNative<NativeActionResult>("rollback_connection_route_plan", {
            request: {
              rollback_id: routeReceipt.rollback_id,
              expected_after_sha256: routeReceipt.after_sha256,
            },
          }).catch(() => null);
          rolledBack = rolledBack || restored?.ok === true;
          if (restored?.ok && wasManaged) await installRuntime().catch(() => undefined);
        }
      }
      rolledBack = rolledBack || /rollback|rolled.back|恢复/i.test(message);
      dispatch({ type: "SET_PHASE", phase: rolledBack ? "rolled-back" : "blocked", message: failure.message });
      setNotice(failure.message);
      addNotification({
        dedupeKey: "connection-plan-failed",
        receipt: { requestId },
        level: "error",
        title: failure.title,
        message: failure.message,
        details: failure.details,
        persistent: true,
      });
    } finally {
      setPendingAction(null);
    }
  };

  const restartService = async () => {
    setPendingAction("restart");
    const requestId = createOperationRequestId("service-restart");
    dispatch({ type: "SET_PHASE", phase: "applying-runtime" });
    try {
      const result = await invokeNative<NativeActionResult>("restart_dtoken_service");
      assertNativeActionSucceeded(result);
      setTransactionLogs(result.logs);
      const [nextSnapshot] = await Promise.all([refreshDiscovery(), refreshRuntime()]);
      if (nextSnapshot) {
        dispatch({ type: "DISCOVERED", snapshot: nextSnapshot });
        const current = getInstallMachineState();
        dispatch({
          type: "SET_PHASE",
          phase: deriveInstallPhase(nextSnapshot, current.selections, current.selectedAgentIds),
        });
      } else {
        dispatch({ type: "SET_PHASE", phase: "partial", message: "connection_setup_unavailable" });
      }
      const message = translateCopy("in.theDTokenServiceRestartedWithTheCurrentRuntimeAndConnectionStateWasRefreshed", lang);
      setNotice(message);
      addNotification({
        dedupeKey: "service-restarted",
        receipt: { requestId },
        level: nextSnapshot ? "success" : "warning",
        title: translateCopy("in.serviceRestarted", lang),
        message,
        persistent: !nextSnapshot,
      });
    } catch (error) {
      const failure = presentOperationFailure("service", error, lang);
      dispatch({ type: "SET_PHASE", phase: "blocked", message: failure.message });
      setNotice(failure.message);
      addNotification({
        dedupeKey: "service-restart-failed",
        receipt: { requestId },
        level: "error",
        title: failure.title,
        message: failure.message,
        details: failure.details,
        persistent: true,
      });
    } finally {
      setPendingAction(null);
    }
  };

  const verifyConnection = async () => {
    setPendingAction("verify");
    const requestId = createOperationRequestId("connection-verify");
    try {
      if (!serviceOnline) await installRuntime();
      dispatch({ type: "SET_PHASE", phase: "verifying" });
      const verification = await invokeNative<RouteVerification>("verify_agent_route_config");
      setRouteVerification(verification);
      const summary = presentRouteVerificationSummary(verification, lang);
      dispatch({ type: "SET_PHASE", phase: verification.ok ? "verified" : "partial", message: summary });
      setNotice(summary);
      addNotification({ dedupeKey: "connection-verify", level: verification.ok ? "success" : "warning", title: verification.ok ? (translateCopy("in.connectionVerified", lang)) : (translateCopy("in.connectionVerificationNeedsAttention", lang)), message: summary, receipt: { requestId, targetId: "agent-route" } });
      await refreshRuntime();
    } catch (error) {
      const failure = presentOperationFailure("connection", error, lang);
      dispatch({ type: "SET_PHASE", phase: "blocked", message: failure.message });
      setNotice(failure.message);
      addNotification({ dedupeKey: "connection-verify-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "agent-route" } });
    } finally {
      setPendingAction(null);
    }
  };

  const handlePrimaryAction = async () => {
    if (busy) return;
    if (restartRequired) return restartService();
    if (machine.phase === "ready") return previewConnectionPlan();
    if (machine.phase === "needs-credential") {
      setCredentialError("");
      setCredentialDialogOpen(true);
      return;
    }
    if (["already-connected", "verified"].includes(machine.phase)) return verifyConnection();
    if (["blocked", "partial", "rolled-back", "idle"].includes(machine.phase)) return refreshAll();
  };

  const saveInlineCredential = async () => {
    if (!credentialProviderId || !providerCredential.trim()) {
      setCredentialError(translateCopy("in.enterAValidCredential", lang));
      return;
    }
    setPendingAction("credential");
    setCredentialError("");
    try {
      const candidate = credentialBinding?.resolved_candidate ?? credentialBinding?.candidates[0];
      const isOpenCode = credentialBinding?.agent_id === "opencode";
      // OpenCode Install credentials must bind to the exact reviewed
      // upstream identity (candidate id + source + endpoint) plus the
      // reviewed discovery revision; provider_id alone is never accepted by
      // the native store for this flow, so fail closed instead of falling
      // back to the generic provider path.
      if (isOpenCode && (!candidate || !snapshot?.revision)) {
        setCredentialError(translateCopy("in.sourceConfigurationChangedTheOldPlanWasInvalidatedAndWillBeRebuilt", lang));
        return;
      }
      const request: {
        provider_id: string;
        credential: string;
        upstream_identity?: CredentialUpstreamIdentity;
        discovery_revision?: string;
      } = {
        provider_id: credentialProviderId,
        credential: providerCredential,
      };
      if (isOpenCode && candidate && snapshot) {
        request.upstream_identity = {
          candidate_id: candidate.id,
          source: candidate.source,
          endpoint: candidate.base_url ?? "",
        };
        request.discovery_revision = snapshot.revision;
      }
      const result = await invokeNative<NativeActionResult>("store_provider_credential", {
        request,
      });
      assertNativeActionSucceeded(result);
      setProviderCredential("");
      setCredentialDialogOpen(false);
      dispatch({ type: "SET_PHASE", phase: "ready" });
      void refreshDiscovery();
    } catch {
      setProviderCredential("");
      // Native/keychain errors can include provider-supplied text. Do not let
      // an error from this sensitive boundary echo the submitted credential.
      setCredentialError(translateCopy("in.theCredentialCouldNotBeStoredSecurelyCheckItAndTryAgain", lang));
    } finally {
      setPendingAction(null);
    }
  };

  const stopService = async () => {
    setPendingAction("stop");
    setStopError("");
    const requestId = createOperationRequestId("service-stop");
    try {
      const result = await invokeNative<NativeActionResult>("stop_dtoken_service");
      assertNativeActionSucceeded(result);
      setTransactionLogs(result.logs);
      setNotice(translateCopy("in.theDTokenLocalServiceStoppedConfigurationAndRollbackPointsRemain", lang));
      addNotification({ dedupeKey: "service-stopped", level: "success", title: translateCopy("tray.localServiceStopped", lang), message: translateCopy("in.configurationAndRollbackPointsRemainAvailableForTheNextStart", lang), receipt: { requestId, targetId: "local-service" } });
      setConfirmServiceStop(false);
      // Stopping succeeded already. Let this handler return so React commits
      // the closed dialog before the now-offline refresh waits on loopback.
      void refreshRuntime().catch(() => {
        setNotice(translateCopy("in.theDTokenLocalServiceStoppedStatusRefreshIsTemporarilyUnavailable", lang));
      });
    } catch (error) {
      const failure = presentOperationFailure("service", error, lang);
      setStopError(failure.message);
      setNotice(failure.message);
      addNotification({ dedupeKey: "service-stop-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "local-service" } });
    } finally {
      setPendingAction(null);
    }
  };

  const previewCcSwitchSync = async () => {
    const ccSwitchEvidence = snapshot?.external_config_snapshot?.cc_switch;
    if (!snapshot || !ccSwitchEvidence?.fingerprint || !ccSwitchEvidence.revision) {
      setNotice(translateCopy("in.ccSwitchConfigurationEvidenceIsNotReadyRescanFirst", lang));
      return;
    }
    setPendingAction("sync");
    try {
      const review = await invokeNative<ConnectionRoutePlanReview>("preview_cc_switch_sync_plan", {
        request: {
          discovery_revision: ccSwitchEvidence.revision,
          source_fingerprint: ccSwitchEvidence.fingerprint,
        },
      });
      if (!review.ok) throw new Error(review.error || review.summary || "cc_switch_sync_preview_failed");
      setCcSwitchSyncReview(review);
      setNotice(translateCopy("in.ccSwitchSyncChangesAreReadyToApply", lang));
    } catch (error) {
      const failure = presentOperationFailure("connection", error, lang);
      setNotice(failure.message);
    } finally {
      setPendingAction(null);
    }
  };

  const applyCcSwitchSync = async () => {
    const ccSwitchEvidence = snapshot?.external_config_snapshot?.cc_switch;
    if (!snapshot || !ccSwitchEvidence?.fingerprint || !ccSwitchEvidence.revision || !ccSwitchSyncReview?.ok || !ccSwitchSyncReview.before_sha256 || !ccSwitchSyncReview.after_sha256) {
      setCcSwitchSyncReview(null);
      setNotice(translateCopy("in.theSyncChangesAreNoLongerCurrentRescanAndTryAgain", lang));
      return;
    }
    setPendingAction("sync");
    try {
      const receipt = await invokeNative<ConnectionRoutePlanReceipt>("apply_cc_switch_sync_plan", {
        request: {
          plan: {
            discovery_revision: ccSwitchEvidence.revision,
            source_fingerprint: ccSwitchEvidence.fingerprint,
          },
          reviewed_before_sha256: ccSwitchSyncReview.before_sha256,
          reviewed_after_sha256: ccSwitchSyncReview.after_sha256,
          enable_auto_follow: ccSwitchAutoFollow,
        },
      });
      if (!receipt.ok || !receipt.verified) throw new Error(receipt.summary || "cc_switch_sync_apply_failed");
      const reload = receipt.runtime
        ?? await invokeNative<{ ok: boolean; state: string; summary: string }>("reload_managed_runtime_after_route_sync");
      setCcSwitchSyncReview(null);
      setNotice(reload.ok
        ? (ccSwitchAutoFollow && receipt.auto_follow?.enabled === true
          ? (translateCopy("in.theCurrentUpstreamIsSyncedFutureAutoFollowStaysWithinTheCcSwitchIdentitiesConfirmedInThisStep", lang))
          : ccSwitchAutoFollow
            ? (translateCopy("in.theCurrentUpstreamIsSyncedButAutoFollowWasNotAuthorizedCheckTheRuntimeReloadReceiptAndRetry", lang))
          : (translateCopy("in.currentCcSwitchUpstreamWasSyncedAndTheDTokenRuntimeWasReverified", lang)))
        : (reload.state === "restart_required"
          ? (translateCopy("in.theRouteWasSyncedRestartTheUnmanagedServiceManuallyThenVerify", lang))
          : (translateCopy("in.theRouteWasSyncedButManagedRuntimeReloadFailedRetryRestartAndVerification", lang))));
      await refreshAll();
    } catch (error) {
      const failure = presentOperationFailure("connection", error, lang);
      setNotice(failure.message);
    } finally {
      setPendingAction(null);
    }
  };

  const runManagedTransport = async (binding: AgentUpstreamBinding, operation: "configure" | "clear") => {
    // Dispatch to MCP integration for MCP/Hook agents.
    if (binding.integration_kind === "mcp" || binding.integration_kind === "hook") {
      await runManagedMcpIntegration(binding, operation);
      return;
    }
    const canOperate = operation === "configure"
      ? binding.can_configure === true
      : binding.can_clear === true;
    if (!canOperate) {
      setNotice(translateCopy("in.exposesNoExecutableDTokenWriteActionNoConfigurationWasModified", lang, { v1: binding.agent_name }));
      return;
    }
    const targetPath = binding.config_path;
    if (!targetPath || targetPath.startsWith("<local>/")) {
      setNotice(translateCopy("in.hasNoSafeLocalConfigPathSoNoFileWasWritten", lang, { v1: binding.agent_name }));
      return;
    }
    if (operation === "configure" && !serviceEndpoint.startsWith("http://127.0.0.1:") && !serviceEndpoint.startsWith("http://localhost:")) {
      setNotice(translateCopy("in.startAndVerifyTheDTokenLocalServiceBeforeConfiguringAnAgent", lang));
      return;
    }
    setTransportBusyAgent(binding.agent_id);
    try {
      {
        // Model comes only from the uniquely auto-resolved model stored in the
        // machine selection, or the Codex Responses passthrough marker ("").
        // Multi-model or unresolvable Agents block the whole batch; the plan
        // fails closed rather than guessing a first model.
        const model = operation === "configure"
          ? machine.selections[binding.agent_id]?.model
          : undefined;
        if (operation === "configure" && !model) {
          setNotice(translateCopy("in.sUpstreamModelCannotBeUniquelyResolvedSoThePlanIsBlockedFixTheSourceConfigurationAndRescan", lang, { v1: binding.agent_name }));
          return;
        }
        const request = {
          agent_id: binding.agent_id,
          target_path: targetPath,
          // Clear is ownership-only and must remain available while the
          // service is offline; the endpoint is ignored by the clear parser.
          endpoint: serviceEndpoint || "http://127.0.0.1:18001",
          enabled: operation === "configure",
          model,
        };
        const preview = await invokeNative<Record<string, unknown>>("preview_managed_agent_transport", {
          request,
          operation,
        });
        const review = preview.review as Record<string, unknown> | undefined;
        if (preview.ok !== true || !review || typeof review.revision !== "string" || typeof review.before_sha256 !== "string" || typeof review.after_sha256 !== "string") {
          throw new Error("managed_transport_preview_failed");
        }
        // Unified OperationDialog confirmation (M-3); the write happens only
        // from applyManagedTransport after the user confirms the plan.
        setTransportConfirm({
          binding,
          operation,
          request: request as { agent_id: string; target_path: string; endpoint: string; enabled: boolean; model?: string },
          review: { revision: review.revision, before_sha256: review.before_sha256, after_sha256: review.after_sha256 },
        });
        return;
      }
    } catch (error) {
      setNotice(translateCopy("in.theOperationDidNotCompleteAndUserConfigurationWasNotChangedRescanAndTryAgain", lang));
    } finally {
      setTransportBusyAgent(null);
    }
  };

  const applyManagedTransport = async () => {
    const pending = transportConfirm;
    if (!pending) return;
    setTransportBusyAgent(pending.binding.agent_id);
    try {
      const result = await invokeNative<Record<string, unknown>>("apply_reviewed_managed_agent_transport", {
        request: pending.request,
        operation: pending.operation,
        reviewedRevision: pending.review.revision,
        reviewedBeforeSha256: pending.review.before_sha256,
        reviewedAfterSha256: pending.review.after_sha256,
      });
      const verification = result.verification as Record<string, unknown> | undefined;
      const ok = result.ok !== false && result.state !== "failed" && verification?.ok !== false;
      const changed = result.changed !== false;
      const outcomeNotice = ok && (pending.operation !== "clear" || changed)
        ? (pending.operation === "configure"
          ? (translateCopy("in.nowUsesAVerifiedDTokenTransport", lang, { v1: pending.binding.agent_name }))
          : (translateCopy("in.theDTokenOwnedTransportForWasCleared", lang, { v1: pending.binding.agent_name })))
        : (pending.operation === "clear" && !changed
          ? (translateCopy("in.noDTokenOwnedMarkerWasFoundUserConfigurationWasNotChanged", lang))
          : (translateCopy("in.theOperationDidNotCompleteAndUserConfigurationWasNotChangedRescanAndTryAgain", lang)));
      await refreshAll();
      setNotice(outcomeNotice);
    } catch (error) {
      setNotice(translateCopy("in.theOperationDidNotCompleteAndUserConfigurationWasNotChangedRescanAndTryAgain", lang));
    } finally {
      setTransportBusyAgent(null);
      setTransportConfirm(null);
    }
  };

  const runManagedMcpIntegration = async (binding: AgentUpstreamBinding, operation: "configure" | "clear") => {
    const configPath = binding.mcp_config_path;
    if (!configPath || configPath.startsWith("<local>/")) {
      setNotice(translateCopy("in.mcpConfigPathIsNotSafelyResolvableNoFileWasWritten", lang, { v1: binding.agent_name }));
      return;
    }
    if (operation === "configure" && !serviceEndpoint.startsWith("http://127.0.0.1:") && !serviceEndpoint.startsWith("http://localhost:")) {
      setNotice(translateCopy("in.startAndVerifyTheDTokenLocalServiceBeforeConfiguringMcp", lang));
      return;
    }
    setTransportBusyAgent(binding.agent_id);
    try {
      const request = {
        agent_id: binding.agent_id,
        config_path: configPath,
        operation,
      };
      const preview = await invokeNative<Record<string, unknown>>("preview_managed_agent_mcp_integration", { request });
      const review = preview.review as Record<string, unknown> | undefined;
      if (preview.ok !== true || !review || typeof review.before_sha256 !== "string" || typeof review.after_sha256 !== "string") {
        throw new Error("mcp_preview_failed");
      }
      // Unified OperationDialog confirmation (M-3); the write happens only
      // from applyManagedMcpIntegration after the user confirms the plan.
      setMcpConfirm({
        binding,
        operation,
        request: request as { agent_id: string; config_path: string; operation: "configure" | "clear" },
        review: { before_sha256: review.before_sha256, after_sha256: review.after_sha256 },
      });
      return;
    } catch (error) {
      setNotice(translateCopy("in.mcpOperationDidNotCompleteUserConfigurationWasNotChanged", lang));
    } finally {
      setTransportBusyAgent(null);
    }
  };

  const applyManagedMcpIntegration = async () => {
    const pending = mcpConfirm;
    if (!pending) return;
    setTransportBusyAgent(pending.binding.agent_id);
    try {
      const result = await invokeNative<Record<string, unknown>>("apply_managed_agent_mcp_integration", {
        request: pending.request,
        reviewed_before_sha256: pending.review.before_sha256,
        reviewed_after_sha256: pending.review.after_sha256,
      });
      const verify = result.verification as Record<string, unknown> | undefined;
      const ok = result.ok !== false && result.state !== "failed" && verify?.ok !== false;
      const outcomeNotice = ok
        ? (pending.operation === "configure"
          ? (translateCopy("in.mcpToolConfiguredAndVerified", lang, { v1: pending.binding.agent_name }))
          : (translateCopy("in.mcpEntryCleared", lang, { v1: pending.binding.agent_name })))
        : (translateCopy("in.mcpOperationDidNotCompleteUserConfigurationWasNotChanged", lang));
      await refreshAll();
      setNotice(outcomeNotice);
    } catch (error) {
      setNotice(translateCopy("in.mcpOperationDidNotCompleteUserConfigurationWasNotChanged", lang));
    } finally {
      setTransportBusyAgent(null);
      setMcpConfirm(null);
    }
  };

  return <InstallView {...{ lang, t, snapshot, discoveryStatus, phasePresentation, discoveryCounts, refreshAll, busy, pendingAction, handlePrimaryAction, primaryAction, serviceInstanceDetail, servicePresentation, restartRequired, serviceOnline, serviceManaged, setStopError, setConfirmServiceStop, visibleBindings, primaryBindings, machine, dispatch, secondaryBindings, ccSwitchPresentation, notice, transactionLogs, agentIntegrationReceipts, routeVerification, showReview, setShowReview, setConnectionReview, setRoutePlanReview, setManagedTransportReviews, setAgentIntegrationReviews, actionableBindings, serviceEndpoint, routePlanReview, connectionReview, managedTransportReviews, agentIntegrationReviews, openCodeSelection: openCodeModels.length > 0 ? { model: openCodeModels[0] } : null, applyConnectionPlan, credentialDialogOpen, setCredentialDialogOpen, setProviderCredential, setCredentialError, credentialBinding, credentialProviderId, providerCredential, credentialError, saveInlineCredential, confirmServiceStop, stopError, serviceStatus, serviceIdentifiersCurrent, stopService, transportBusyAgent, onConfigureAgent: (binding: AgentUpstreamBinding) => { void runManagedTransport(binding, "configure"); }, onClearAgent: (binding: AgentUpstreamBinding) => { void runManagedTransport(binding, "clear"); }, transportConfirm, setTransportConfirm, applyManagedTransport: () => applyManagedTransport(), mcpConfirm, setMcpConfirm, applyManagedMcpIntegration: () => applyManagedMcpIntegration(), agentAutoFollow, setAgentAutoFollow, ccSwitchSyncReview, setCcSwitchSyncReview, ccSwitchAutoFollow, setCcSwitchAutoFollow, ccSwitchSyncBusy: pendingAction === "sync", onPreviewCcSwitchSync: () => { void previewCcSwitchSync(); }, onApplyCcSwitchSync: () => { void applyCcSwitchSync(); } }} />;
}
