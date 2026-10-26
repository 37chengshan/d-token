import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { useI18n, useT } from "../../lib/i18n";
import { invokeNative, type NativeActionLog, type NativeActionResult } from "../../lib/native";
import { useRuntimeDashboardSelector, shallowEqualSlice, type RuntimeDashboardState } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";
import {
  canReviewInstall,
  canJoinInstallBatch,
  consumeInstallHandoff,
  deriveInstallPhase,
  dispatchInstallMachine,
  getInstallMachineState,
  prepareSelectedInstallBindings,
  requiresAgentRouteRepair,
  runInstallAgentBatch,
  selectedCandidate,
  subscribeInstallMachine,
  type InstallAgentReceipt,
  type InstallSelectedPreparation,
  type InstallPhase,
} from "../../lib/installMachine";
import { useConnectionSetup, type AgentUpstreamBinding, type ConnectionSetupSnapshot, type UpstreamCandidate } from "../../lib/connectionSetup";
import { InstallView } from "../../lib/install/InstallView";
import { buildConnectionRoutePlanRequest, connectionDiscoveryCounts, derivePrimaryAction, desktopInstallRoot, hybridAgentIntegrationDraft, presentCcSwitch, presentInstallPhase, presentRouteVerificationSummary, presentServiceInstance, safeText, supportsAutomaticAgentIntegration } from "../../lib/install/presentation";
import type { ConnectionRoutePlanReceipt, ConnectionRoutePlanReview, DtokenServiceStatus, HybridAgentIntegrationApply, HybridAgentIntegrationReview, HybridAgentIntegrationVerification, ManagedAgentTransportReceipt, ManagedAgentTransportReview, RouteVerification } from "../../lib/install/presentation";
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

function presentPreparationReason(
  reason: InstallSelectedPreparation["reason"],
  lang: "zh" | "en",
) {
  if (lang === "zh") {
    switch (reason) {
      case "already-connected": return "已保留当前连接。";
      case "credential-required": return "请在工具中补充凭据后重新扫描。";
      case "ambiguous-route": return "请在工具中选定当前连接后重新扫描。";
      case "model-unresolved": return "请在工具中选定模型后重新扫描。";
      case "safe-adapter-unavailable": return "当前未提供可执行配置动作。";
      case "ready": return "已准备好配置。";
    }
  }
  switch (reason) {
    case "already-connected": return "The current connection is kept.";
    case "credential-required": return "Add the credential in this tool, then scan again.";
    case "ambiguous-route": return "Choose the active connection in this tool, then scan again.";
    case "model-unresolved": return "Choose a model in this tool, then scan again.";
    case "safe-adapter-unavailable": return "No executable setup action is available.";
    case "ready": return "Ready to configure.";
  }
}

function displayBindingsFromCatalog(snapshot: ConnectionSetupSnapshot | null): AgentUpstreamBinding[] {
  if (!snapshot) return [];
  const detected = new Map(snapshot.agent_bindings.map((binding) => [binding.agent_id, binding]));
  if (!snapshot.agent_catalog?.length) {
    return snapshot.agent_bindings
      .filter((binding) => binding.installed || binding.already_connected)
      .map((binding) => ({ ...binding, catalog_installed: binding.installed }));
  }
  return snapshot.agent_catalog.map((catalogItem) => {
    const binding = detected.get(catalogItem.agent_id);
    if (binding) {
      return {
        ...binding,
        agent_name: binding.agent_name || catalogItem.agent_name,
        catalog_installed: catalogItem.installed,
        integration_kind: binding.integration_kind ?? catalogItem.integration_kind,
        can_configure: binding.can_configure ?? catalogItem.can_configure,
        can_clear: binding.can_clear ?? catalogItem.can_clear,
      };
    }
    return {
      agent_id: catalogItem.agent_id,
      agent_name: catalogItem.agent_name,
      installed: catalogItem.installed === true,
      catalog_installed: catalogItem.installed,
      current_config_source: "catalog",
      candidates: [],
      resolved_candidate: null,
      resolution: catalogItem.installed === null ? "unavailable" : catalogItem.installed ? "unsupported" : "unavailable",
      confidence: 0,
      reasons: [catalogItem.installed === null ? "scan-incomplete" : catalogItem.installed ? "not-configurable" : "not-installed"],
      capability: "manual-only",
      can_configure: false,
      can_clear: false,
      transport_supported: false,
      transport_status: catalogItem.installed ? "advanced_required" : "schema_unverified",
      already_connected: false,
      integration_kind: catalogItem.integration_kind,
    };
  });
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
  const [credentialDialogOpen, setCredentialDialogOpen] = useState(false);
  const [credentialAgentId, setCredentialAgentId] = useState<string | null>(null);
  const [providerCredential, setProviderCredential] = useState("");
  const [credentialError, setCredentialError] = useState("");
  const [confirmServiceStop, setConfirmServiceStop] = useState(false);
  const [stopError, setStopError] = useState("");
  const [routePlanReview, setRoutePlanReview] = useState<ConnectionRoutePlanReview | null>(null);
  const routePlanReviewRef = useRef<ConnectionRoutePlanReview | null>(null);
  const managedTransportReviewsRef = useRef<Record<string, ManagedAgentTransportReview>>({});
  const agentIntegrationReviewsRef = useRef<Record<string, HybridAgentIntegrationReview>>({});
  const plannedBindingsRef = useRef<AgentUpstreamBinding[]>([]);
  const preflightFailuresRef = useRef<InstallAgentReceipt[]>([]);
  const executionDiscoveryRevisionRef = useRef<string | null>(null);
  const executionPreparationRef = useRef<InstallSelectedPreparation[]>([]);
  const setRoutePlanReviewSynced = (review: ConnectionRoutePlanReview | null) => {
    routePlanReviewRef.current = review;
    setRoutePlanReview(review);
  };

  const setManagedTransportReviewsSynced = (reviews: Record<string, ManagedAgentTransportReview>) => {
    managedTransportReviewsRef.current = reviews;
    setManagedTransportReviews(reviews);
  };
  const setAgentIntegrationReviewsSynced = (reviews: Record<string, HybridAgentIntegrationReview>) => {
    agentIntegrationReviewsRef.current = reviews;
    setAgentIntegrationReviews(reviews);
  };
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
  const [agentActionFeedback, setAgentActionFeedback] = useState<Record<string, { message: string; failed: boolean }>>({});
  const [transportBusyAgent, setTransportBusyAgent] = useState<string | null>(null);

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
    () => displayBindingsFromCatalog(snapshot),
    [snapshot],
  );
  const actionableBindings = useMemo(
    () => (snapshot?.agent_bindings ?? []).filter((binding) => machine.selectedAgentIds.includes(binding.agent_id) && canJoinInstallBatch(binding)),
    [machine.selectedAgentIds, snapshot?.agent_bindings],
  );
  const preparedBindings = useMemo(
    () => snapshot ? prepareSelectedInstallBindings(snapshot, machine.selections, machine.selectedAgentIds) : [],
    [machine.selectedAgentIds, machine.selections, snapshot],
  );
  const readyBindings = useMemo(
    () => preparedBindings.filter((item) => item.ready).map((item) => item.binding),
    [preparedBindings],
  );
  const selectedOpenCode = readyBindings.find((binding) => binding.agent_id === "opencode");
  const credentialBinding = (credentialAgentId
    ? visibleBindings.find((binding) => binding.agent_id === credentialAgentId && binding.resolution === "needs-credential")
    : actionableBindings.find((binding) => binding.resolution === "needs-credential")) ?? null;
  const credentialProviderId = credentialBinding?.resolved_candidate?.id
    ?? credentialBinding?.candidates[0]?.id
    ?? "";
  const openCodeSelection = selectedOpenCode
    ? selectedCandidate(selectedOpenCode, machine.selections[selectedOpenCode.agent_id])
    : null;
  const reviewReady = snapshot ? canReviewInstall(snapshot, machine.selections, machine.selectedAgentIds) : false;
  const busy = pendingAction !== null || ["applying-runtime", "applying-agent-configs", "verifying"].includes(machine.phase);
  const discoveryCounts = useMemo(() => connectionDiscoveryCounts(snapshot), [snapshot]);
  const phasePresentation = restartRequired
    ? {
        health: "warning" as const,
        label: lang === "zh" ? "需要重启" : "Restart required",
        description: lang === "zh"
          ? "磁盘中的 runtime 已更新，但当前进程仍是旧版本；重启并重新验证后才能继续使用。"
          : "The runtime on disk was updated, but the current process is stale. Restart and reverify it before continuing.",
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
    setRoutePlanReviewSynced(null);
    setCcSwitchSyncReview((current) =>
      current?.source_fingerprint === snapshot.external_config_snapshot?.cc_switch?.fingerprint
        ? current
        : null);
    setAgentIntegrationReviewsSynced({});
    const sources = snapshot.external_config_refresh.changed_sources.join(", ");
    setNotice(lang === "zh"
      ? `外部配置已变化${sources ? `（${sources}）` : ""}，旧计划已失效；请重新扫描并确认。`
      : `External configuration changed${sources ? ` (${sources})` : ""}. The old plan is stale; rescan and review again.`);
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

  const refreshAll = async (notify = true) => {
    setPendingAction("discover");
    const requestId = createOperationRequestId("connection-refresh");
    if (notify) setNotice(lang === "zh" ? "正在重新读取 Agent、上游、cc-switch 与 d-token 状态…" : "Refreshing Agents, upstreams, cc-switch, and d-token state…");
    dispatch({ type: "DISCOVER_START" });
    try {
      const [discovery] = await Promise.all([refreshDiscovery(), refreshRuntime()]);
      if (notify) setNotice(discovery
        ? (lang === "zh" ? "本机连接状态已更新。" : "Local connection state refreshed.")
        : (lang === "zh" ? "当前是浏览器预览，不读取或改写本机配置。" : "Browser preview does not read or modify local configuration."));
      if (notify) addNotification({
        dedupeKey: "connection-refresh",
        level: discovery ? "success" : "info",
        title: discovery ? (lang === "zh" ? "连接状态已更新" : "Connection state refreshed") : (lang === "zh" ? "连接状态不可用" : "Connection state unavailable"),
        message: discovery ? (lang === "zh" ? "编码工具与连接设置已重新读取。" : "Coding tools and connection settings were refreshed.") : (lang === "zh" ? "浏览器预览不会读取本机配置。" : "Browser preview does not read local configuration."),
        receipt: { requestId, targetId: "connection-discovery" },
      });
    } catch (error) {
      const failure = presentOperationFailure("connection", error, lang);
      if (notify) setNotice(failure.message);
      if (notify) addNotification({ dedupeKey: "connection-refresh-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "connection-discovery" } });
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

  const previewConnectionPlan = async (bindings: AgentUpstreamBinding[] = readyBindings): Promise<boolean> => {
    if (!snapshot || bindings.length === 0) return false;
    setRoutePlanReviewSynced(null);
    setManagedTransportReviewsSynced({});
    setAgentIntegrationReviewsSynced({});
    const requestId = createOperationRequestId("connection-preview");
    const routePlan = buildConnectionRoutePlanRequest(snapshot.revision, bindings, machine.selections);
    if (routePlan.bindings.length === 0) return false;
    dispatch({ type: "OPEN_REVIEW" });
    setPendingAction("preview");
    try {
      const integrationBindings = bindings.filter((binding) =>
        supportsAutomaticAgentIntegration(binding.agent_id)
        && (!binding.already_connected || requiresAgentRouteRepair(binding)),
      );
      const managedTransportBindings = bindings.filter((binding) =>
        binding.can_configure === true
        && (!binding.already_connected || requiresAgentRouteRepair(binding))
        && Boolean(binding.config_path),
      );
      const [routeReview, managedTransportReviewEntries, integrationReviewEntries] = await Promise.all([
        invokeNative<ConnectionRoutePlanReview>("preview_connection_route_plan", { request: routePlan }),
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
            }).catch((): ManagedAgentTransportReview => ({ ok: false, state: "failed", summary: "managed_transport_preview_failed" })),
          ] as const;
        })),
        Promise.all(integrationBindings.map(async (binding) => [
          binding.agent_id,
          await invokeNative<HybridAgentIntegrationReview>("preview_hybrid_agent_integration", {
            request: hybridAgentIntegrationDraft(binding.agent_id),
          }).catch((): HybridAgentIntegrationReview => ({ ok: false, state: "failed", error: "agent_integration_preview_failed" })),
        ] as const)),
      ]);
      const failedIds = new Set<string>();
      for (const [agentId, result] of managedTransportReviewEntries) {
        if (!result.ok || !result.review?.revision || !result.review.before_sha256 || !result.review.after_sha256) failedIds.add(agentId);
      }
      for (const [agentId, result] of integrationReviewEntries) {
        if (!result.ok || !result.plan?.review_digest) failedIds.add(agentId);
      }
      const preparedBindings = bindings.filter((binding) => !failedIds.has(binding.agent_id));
      preflightFailuresRef.current = bindings.filter((binding) => failedIds.has(binding.agent_id)).map((binding) => ({
        agentId: binding.agent_id,
        agentName: binding.agent_name,
        state: "blocked",
        summary: lang === "zh" ? `${binding.agent_name} 的配置预检未通过，请检查工具设置后重试。` : `${binding.agent_name} did not pass setup checks. Check its settings and retry.`,
      }));
      if (failedIds.size > 0) {
        setAgentIntegrationReceipts(preflightFailuresRef.current);
        setAgentActionFeedback((current) => ({ ...current, ...Object.fromEntries(preflightFailuresRef.current.map((item) => [item.agentId, { message: item.summary, failed: true }])) }));
        if (preparedBindings.length === 0) {
          const message = preflightFailuresRef.current.map((item) => item.summary).join(" ");
          setNotice(message);
          dispatch({ type: "SET_PHASE", phase: "blocked", message });
          addNotification({ dedupeKey: "connection-preflight-failed", level: "error", title: lang === "zh" ? "配置尚未开始" : "Configuration not started", message, receipt: { requestId } });
          return false;
        }
      }
      const preparedRouteReview = failedIds.size === 0 ? routeReview : await invokeNative<ConnectionRoutePlanReview>("preview_connection_route_plan", {
        request: buildConnectionRoutePlanRequest(snapshot.revision, preparedBindings, machine.selections),
      });
      setRoutePlanReviewSynced(preparedRouteReview);
      setManagedTransportReviewsSynced(Object.fromEntries(managedTransportReviewEntries));
      setAgentIntegrationReviewsSynced(Object.fromEntries(integrationReviewEntries));
      if (!preparedRouteReview.ok) {
        const failure = presentOperationFailure("connection", preparedRouteReview.error, lang);
        setNotice(failure.message);
        dispatch({ type: "SET_PHASE", phase: "blocked", message: failure.message });
        addNotification({ dedupeKey: "connection-preview-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "connection-plan" } });
        return false;
      } else {
        plannedBindingsRef.current = preparedBindings;
        addNotification({ dedupeKey: `connection-preview-${routeReview.after_sha256 ?? "ready"}`, level: "info", title: lang === "zh" ? "连接变更已准备" : "Connection changes are ready", message: lang === "zh" ? "差异和验证证据已准备，正在应用所选工具。" : "Differences and verification evidence are ready; applying the selected tools.", receipt: { requestId, targetId: "connection-plan" } });
        return true;
      }
    } catch (error) {
      const failure = presentOperationFailure("connection", error, lang);
      setNotice(failure.message);
      dispatch({ type: "SET_PHASE", phase: "blocked", message: failure.message });
      addNotification({ dedupeKey: "connection-preview-error", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "connection-plan" } });
      return false;
    } finally {
      setPendingAction(null);
    }
  };

  const applyConnectionPlan = async () => {
    if (!snapshot || executionDiscoveryRevisionRef.current !== snapshot.revision) {
        setNotice(lang === "zh" ? "源配置已变化，旧方案已失效，正在重新读取。" : "Source configuration changed; the old plan was invalidated and will be rebuilt.");
      await refreshAll(false);
      return;
    }
    setPendingAction("apply");
    const requestId = createOperationRequestId("connection-apply");
    const executionPreparation = executionPreparationRef.current;
    const plannedBindings = plannedBindingsRef.current.length > 0
      ? plannedBindingsRef.current
      : executionPreparation.filter((item) => item.ready).map((item) => item.binding);
    const routePlan = buildConnectionRoutePlanRequest(snapshot.revision, plannedBindings, machine.selections);
    const review = routePlanReviewRef.current ?? routePlanReview;
    const routeChanges = review?.before_sha256 !== review?.after_sha256;
    const wasManaged = serviceManaged;
    let routeReceipt: ConnectionRoutePlanReceipt | null = null;
    const appliedManagedTransports = new Map<string, ManagedAgentTransportReceipt>();
    const verifiedTransportIds = new Set<string>();
    const transportFailures: InstallAgentReceipt[] = [];
    const restoreTransport = async (agentId: string): Promise<boolean> => {
      const receipt = appliedManagedTransports.get(agentId);
      if (!receipt || receipt.changed === false) return true;
      const changes = receipt.transaction?.changes ?? [];
      if (changes.length === 0) return false;
      for (const change of [...changes].reverse()) {
        const restored = await invokeNative<NativeActionResult>("manage_backup", { action: "restore", id: change.backup.rollback_id }).catch(() => null);
        if (!restored?.ok) return false;
        if (receipt.transaction) receipt.transaction.changes = receipt.transaction.changes.filter((remaining) => remaining.backup.rollback_id !== change.backup.rollback_id);
      }
      appliedManagedTransports.delete(agentId);
      return true;
    };
    let appliedAgentReceipts: InstallAgentReceipt[] = [];
    let serviceStoppedForUpdate = false;
    try {
      if (!review?.ok || !review.before_sha256 || !review.after_sha256) {
        throw new Error("connection_preview_incomplete");
      }
      if (routeChanges && serviceOnline && !serviceManaged) {
        throw new Error(lang === "zh" ? "18001 端口由非本应用启动的 d-token 服务占用，无法安全重载路由。" : "Port 18001 is owned by an unmanaged d-token service, so the route cannot be reloaded safely.");
      }
      if (routeChanges && serviceManaged) {
        const stopped = await invokeNative<NativeActionResult>("stop_dtoken_service");
        assertNativeActionSucceeded(stopped);
        serviceStoppedForUpdate = true;
      }
      routeReceipt = await invokeNative<ConnectionRoutePlanReceipt>("apply_connection_route_plan", {
        request: {
          plan: routePlan,
          reviewed_before_sha256: review.before_sha256,
          reviewed_after_sha256: review.after_sha256,
          enable_auto_follow: agentAutoFollow,
        },
      });
      if (!routeReceipt.ok || !routeReceipt.verified) throw new Error(routeReceipt.summary);
      if (!serviceOnline || routeChanges) await installRuntime();

      dispatch({ type: "SET_PHASE", phase: "applying-agent-configs" });
      const managedTransportBindings = plannedBindings.filter((binding) =>
        binding.can_configure === true
        && (!binding.already_connected || requiresAgentRouteRepair(binding))
        && Boolean(binding.config_path),
      );
      for (const binding of managedTransportBindings) {
        try {
          const review = (managedTransportReviewsRef.current[binding.agent_id] ?? managedTransportReviews[binding.agent_id])?.review;
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
          if (receipt.changed) appliedManagedTransports.set(binding.agent_id, receipt);
          if (receipt.ok === false || receipt.state === "failed" || receipt.verification?.ok !== true) {
            throw new Error("managed_transport_apply_failed");
          }
          verifiedTransportIds.add(binding.agent_id);
        } catch (error) {
          const restored = await restoreTransport(binding.agent_id);
          transportFailures.push({ agentId: binding.agent_id, agentName: binding.agent_name,
            state: restored ? "failed" : "rollback-failed",
            summary: restored
              ? (lang === "zh" ? `${binding.agent_name} 配置失败，请重试。` : `${binding.agent_name} could not be configured. Retry.`)
              : (lang === "zh" ? `${binding.agent_name} 恢复未完成，请查看变更与回退。` : `${binding.agent_name} recovery needs attention in Changes.`),
          });
        }
      }

      const agentBatch = await runInstallAgentBatch(
        plannedBindings
          .filter((binding) => supportsAutomaticAgentIntegration(binding.agent_id) && !transportFailures.some((failure) => failure.agentId === binding.agent_id))
          .map((binding) => ({
            agentId: binding.agent_id,
            agentName: binding.agent_name,
            alreadyConnected: binding.already_connected && !requiresAgentRouteRepair(binding),
            reviewDigest: (agentIntegrationReviewsRef.current[binding.agent_id] ?? agentIntegrationReviews[binding.agent_id])?.plan?.review_digest,
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
      for (const receipt of agentBatch.receipts) {
        if (["failed", "blocked", "rolled-back", "rollback-failed"].includes(receipt.state)) {
          if (!await restoreTransport(receipt.agentId)) {
            receipt.state = "rollback-failed";
            receipt.summary = lang === "zh" ? "连接配置恢复未完成，请查看变更与回退。" : "Connection recovery needs attention in Changes.";
          }
        }
      }
      const blockedReceipts: InstallAgentReceipt[] = [...preflightFailuresRef.current, ...transportFailures, ...executionPreparation
        .filter((item) => !item.ready)
        .map((item) => ({
          agentId: item.agentId,
          agentName: item.agentName,
          state: "blocked" as const,
          summary: presentPreparationReason(item.reason, lang),
        }))];
      appliedAgentReceipts = agentBatch.receipts;
      const transportOnlyReceipts: InstallAgentReceipt[] = plannedBindings
        .filter((binding) => !supportsAutomaticAgentIntegration(binding.agent_id)
          && (verifiedTransportIds.has(binding.agent_id) || (binding.already_connected && !requiresAgentRouteRepair(binding))))
        .map((binding) => ({ agentId: binding.agent_id, agentName: binding.agent_name, state: "verified",
          summary: lang === "zh" ? "连接配置已验证。" : "Connection configuration verified.",
        }));
      const allReceipts = [...blockedReceipts, ...agentBatch.receipts, ...transportOnlyReceipts];
      setAgentIntegrationReceipts(allReceipts);
      setAgentActionFeedback((current) => ({ ...current, ...Object.fromEntries(allReceipts.map((item) => [item.agentId, {
        message: item.summary, failed: !["verified", "already-connected"].includes(item.state),
      }])) }));

      dispatch({ type: "SET_PHASE", phase: "verifying" });
      const verification = await invokeNative<RouteVerification>("verify_agent_route_config");
      setRouteVerification(verification);
      if (!verification.ok) throw new Error(verification.summary);
      const successfulAgentIds = new Set(allReceipts.filter((receipt) => ["verified", "already-connected"].includes(receipt.state)).map((receipt) => receipt.agentId));
      const noneCompleted = successfulAgentIds.size === 0;
      const fullyVerified = successfulAgentIds.size === plannedBindings.length && agentBatch.ok
        && blockedReceipts.length === 0;
      if (
        fullyVerified
        && agentAutoFollow
        && routeReceipt.auto_follow?.state === "pending_runtime_verification"
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
      await Promise.all([refreshRuntime(), refreshDiscovery()]);
      dispatch({
        type: "SET_PHASE",
        phase: fullyVerified ? "verified" : noneCompleted ? "blocked" : "partial",
        message: fullyVerified ? "" : "agent-write-unavailable",
      });
      setNotice(fullyVerified
        ? (lang === "zh" ? "已确认配置生效、本地服务健康且路由可用。" : "Configuration is active, the local service is healthy, and the route is available.")
        : noneCompleted ? (lang === "zh" ? "没有工具完成配置，请查看详情后重试。" : "No tools were configured. Check their details and retry.")
        : (lang === "zh" ? "部分工具未完成配置，请查看对应工具的详情后重试。" : "Some tools could not be configured. Check their details and retry."));
      addNotification({
        dedupeKey: `connection-plan-${fullyVerified ? "verified" : "partial"}`,
        receipt: { requestId },
        level: fullyVerified ? "success" : noneCompleted ? "error" : "warning",
        title: fullyVerified ? (lang === "zh" ? "Agent 连接已验证" : "Agent connection verified") : noneCompleted ? (lang === "zh" ? "连接未完成" : "Connection failed") : (lang === "zh" ? "连接部分完成" : "Connection partially complete"),
        message: fullyVerified
          ? (lang === "zh" ? "配置、服务和路由证据已通过验证。" : "Configuration, service, and route evidence passed verification.")
          : noneCompleted ? (lang === "zh" ? "请查看工具详情中的失败原因和处理结果。" : "Check the tool details for the failure and recovery result.")
          : (lang === "zh" ? "已完成的工具可以继续使用；失败工具的处理结果见详情。" : "Completed tools remain available. See details for the failed tools."),
        persistent: !fullyVerified,
      });
    } catch (error) {
      const failure = presentOperationFailure("connection", error, lang);
      let recoveryAttempted = serviceStoppedForUpdate;
      let recoveryFailed = [...transportFailures, ...appliedAgentReceipts].some((receipt) => receipt.state === "rollback-failed");
      for (const receipt of [...appliedAgentReceipts].reverse()) {
        if (!receipt.transactionId || !["applied", "verified"].includes(receipt.state)) continue;
        recoveryAttempted = true;
        const restored = await invokeNative<HybridAgentIntegrationApply>("rollback_hybrid_agent_integration", {
          request: { transaction_id: receipt.transactionId },
        }).catch(() => null);
        if (!restored?.ok) recoveryFailed = true;
      }
      for (const agentId of [...appliedManagedTransports.keys()].reverse()) {
        recoveryAttempted = true;
        if (!await restoreTransport(agentId)) recoveryFailed = true;
      }
      if (routeReceipt?.ok && routeReceipt.rollback_id && routeReceipt.after_sha256) {
        recoveryAttempted = true;
        let canRestore = true;
        if (routeChanges) {
          const status = await invokeNative<DtokenServiceStatus>("get_dtoken_service_status").catch(() => null);
          if (status?.managed) {
            const stopped = await invokeNative<NativeActionResult>("stop_dtoken_service").catch(() => null);
            canRestore = stopped?.ok === true;
          } else {
            canRestore = status?.state === "stopped";
          }
        }
        if (canRestore) {
          const restored = await invokeNative<NativeActionResult>("rollback_connection_route_plan", {
            request: { rollback_id: routeReceipt.rollback_id, expected_after_sha256: routeReceipt.after_sha256 },
          }).catch(() => null);
          if (!restored?.ok) recoveryFailed = true;
        } else recoveryFailed = true;
      }
      if (serviceStoppedForUpdate && wasManaged) {
        const active = await invokeNative<{ source?: string; state?: string; content_sha256?: string }>("get_active_route_config_status").catch(() => null);
        const originalConfigVerified = active?.source === "d-token-desktop"
          && ["loaded", "missing"].includes(active.state ?? "")
          && active.content_sha256 === review?.before_sha256;
        if (originalConfigVerified) {
          try { await installRuntime(); } catch { recoveryFailed = true; }
        } else recoveryFailed = true;
      }
      const message = recoveryFailed
        ? (lang === "zh" ? "操作未完成，部分配置或服务尚未恢复。请运行诊断或查看变更与回退。" : "The operation failed and some configuration or service state still needs recovery. Run diagnostics or open Changes.")
        : failure.message;
      dispatch({ type: "SET_PHASE", phase: recoveryAttempted && !recoveryFailed ? "rolled-back" : "blocked", message });
      setNotice(message);
      addNotification({ dedupeKey: "connection-plan-failed", receipt: { requestId }, level: "error",
        title: failure.title, message, details: failure.details, persistent: true,
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
      const message = lang === "zh"
        ? "d-token 服务已使用当前 runtime 重启，连接状态已重新读取。"
        : "The d-token service restarted with the current runtime and connection state was refreshed.";
      setNotice(message);
      addNotification({
        dedupeKey: "service-restarted",
        receipt: { requestId },
        level: nextSnapshot ? "success" : "warning",
        title: lang === "zh" ? "服务已重启" : "Service restarted",
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
      addNotification({ dedupeKey: "connection-verify", level: verification.ok ? "success" : "warning", title: verification.ok ? (lang === "zh" ? "连接验证完成" : "Connection verified") : (lang === "zh" ? "连接验证需要处理" : "Connection verification needs attention"), message: summary, receipt: { requestId, targetId: "agent-route" } });
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

  const configureSelected = async (scopeAgentIds = machine.selectedAgentIds) => {
    if (busy || !snapshot || scopeAgentIds.length === 0) return;
    const scopePreparation = prepareSelectedInstallBindings(snapshot, machine.selections, scopeAgentIds);
    const ready = scopePreparation.filter((item) => item.ready);
    const needsCredential = scopePreparation.find((item) => item.reason === "credential-required");
    if (ready.length === 0 && needsCredential) {
      setCredentialAgentId(needsCredential.agentId);
      setCredentialError("");
      setCredentialDialogOpen(true);
      return;
    }
    executionDiscoveryRevisionRef.current = snapshot.revision;
    executionPreparationRef.current = scopePreparation;
    plannedBindingsRef.current = [];
    preflightFailuresRef.current = [];
    setAgentActionFeedback((current) => Object.fromEntries(Object.entries(current).filter(([id]) => !scopeAgentIds.includes(id))));
    const blocked = scopePreparation.filter((item) => !item.ready);
    if (blocked.length > 0) {
      setAgentIntegrationReceipts(blocked.map((item) => ({
        agentId: item.agentId,
        agentName: item.agentName,
        state: "blocked",
        summary: presentPreparationReason(item.reason, lang),
      })));
      setNotice(ready.length === 0
        ? (lang === "zh" ? "所选工具还不能配置，请按详情处理后重新扫描。" : "The selected tools are not ready. Use details, then scan again.")
        : (lang === "zh" ? `${blocked.length} 个所选工具需要处理，其余工具会继续配置。` : `${blocked.length} selected tool(s) need attention; the rest will continue.`));
    }
    const readyBindingsForScope = ready.map((item) => item.binding);
    if (readyBindingsForScope.length === 0) return;
    if (buildConnectionRoutePlanRequest(snapshot.revision, readyBindingsForScope, machine.selections).bindings.length === 0) {
      await verifyConnection();
      return;
    }
    const previewed = await previewConnectionPlan(readyBindingsForScope);
    if (previewed) await applyConnectionPlan();
  };


  useEffect(() => {
    if (!snapshot || discoveryStatus !== "ready" || busy) return;
    const handoff = consumeInstallHandoff();
    if (!handoff) return;
    const requestedIds = handoff.agentIds;
    const actionableIds = new Set(snapshot.agent_bindings.filter(canJoinInstallBatch).map((binding) => binding.agent_id));
    const exactScopeAvailable = requestedIds.length > 0 && requestedIds.every((id) => actionableIds.has(id));
    const currentIds = getInstallMachineState().selectedAgentIds;
    const selectionUnchanged = currentIds.length === requestedIds.length && currentIds.every((id) => requestedIds.includes(id));
    const age = Date.now() - handoff.requestedAtMs;
    if (handoff.discoveryRevision !== snapshot.revision || !exactScopeAvailable || !selectionUnchanged || age < 0 || age > 60_000) {
      const message = lang === "zh"
        ? "连接信息已变化，请检查所选工具后重新点击配置。"
        : "Connection details changed. Check the selected tools and configure again.";
      setNotice(message);
      addNotification({
        dedupeKey: "install-handoff-stale",
        level: "warning",
        title: lang === "zh" ? "需要重新确认" : "Confirmation needed",
        message,
        receipt: { requestId: createOperationRequestId("install-handoff"), targetId: "install-handoff" },
      });
      return;
    }
    void configureSelected(requestedIds);
  }, [snapshot, discoveryStatus, busy]);

  const handlePrimaryAction = async () => {
    if (busy) return;
    if (restartRequired) return restartService();
    if (machine.phase === "ready") return configureSelected();
    if (machine.phase === "needs-credential") {
      setCredentialAgentId(null);
      setCredentialError("");
      setCredentialDialogOpen(true);
      return;
    }
    if (["already-connected", "verified"].includes(machine.phase)) return verifyConnection();
    if (["blocked", "partial", "rolled-back", "idle"].includes(machine.phase)) return refreshAll();
  };

  const saveInlineCredential = async () => {
    if (!credentialProviderId || !providerCredential.trim()) {
      setCredentialError(lang === "zh" ? "请输入有效凭据。" : "Enter a valid credential.");
      return;
    }
    setPendingAction("credential");
    setCredentialError("");
    try {
      const result = await invokeNative<NativeActionResult>("store_provider_credential", {
        request: { provider_id: credentialProviderId, credential: providerCredential },
      });
      assertNativeActionSucceeded(result);
      setProviderCredential("");
      setCredentialDialogOpen(false);
      setCredentialAgentId(null);
      dispatch({ type: "SET_PHASE", phase: "ready" });
      void refreshDiscovery();
    } catch {
      setProviderCredential("");
      // Native/keychain errors can include provider-supplied text. Do not let
      // an error from this sensitive boundary echo the submitted credential.
      setCredentialError(lang === "zh" ? "未能安全保存凭据，请检查后重试。" : "The credential could not be stored securely. Check it and try again.");
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
      setNotice(lang === "zh" ? "d-token 本地服务已停止，配置与回退点仍会保留。" : "The d-token local service stopped; configuration and rollback points remain.");
      addNotification({ dedupeKey: "service-stopped", level: "success", title: lang === "zh" ? "本地服务已停止" : "Local service stopped", message: lang === "zh" ? "配置与回退点仍会保留，可在需要时重新启动。" : "Configuration and rollback points remain available for the next start.", receipt: { requestId, targetId: "local-service" } });
      setConfirmServiceStop(false);
      // Stopping succeeded already. Let this handler return so React commits
      // the closed dialog before the now-offline refresh waits on loopback.
      void refreshRuntime().catch(() => {
        setNotice(lang === "zh"
          ? "d-token 本地服务已停止；状态刷新暂不可用，可稍后手动刷新。"
          : "The d-token local service stopped; status refresh is temporarily unavailable.");
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
      setNotice(lang === "zh" ? "cc-switch 配置证据尚未准备好，请重新扫描。" : "cc-switch configuration evidence is not ready; rescan first.");
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
      setNotice(lang === "zh" ? "已生成 cc-switch 同步差异，请确认后应用。" : "cc-switch sync changes are ready to apply.");
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
      setNotice(lang === "zh" ? "同步计划已失效，请重新扫描。" : "The sync changes are no longer current; rescan and try again.");
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
          ? (lang === "zh" ? "已同步当前上游；以后只会自动跟随本次确认的 cc-switch 上游身份。" : "The current upstream is synced; future auto-follow stays within the cc-switch identities confirmed in this step.")
          : ccSwitchAutoFollow
            ? (lang === "zh" ? "当前上游已同步，但自动跟随未获授权；请检查 runtime reload receipt 后重试。" : "The current upstream is synced, but auto-follow was not authorized; check the runtime reload receipt and retry.")
          : (lang === "zh" ? "已同步当前 cc-switch 上游并重新验证 d-token runtime。" : "Current cc-switch upstream was synced and the d-token runtime was reverified."))
        : (reload.state === "restart_required"
          ? (lang === "zh" ? "路由已同步；当前服务不受 d-token 管理，请手动重启后验证。" : "The route was synced; restart the unmanaged service manually, then verify.")
          : (lang === "zh" ? "路由已同步，但受管 runtime 重载失败；请重试重启并验证。" : "The route was synced, but managed runtime reload failed; retry restart and verification.")));
      await refreshAll(false);
    } catch (error) {
      const failure = presentOperationFailure("connection", error, lang);
      setNotice(failure.message);
    } finally {
      setPendingAction(null);
    }
  };

  const reportAgentAction = (binding: AgentUpstreamBinding, message: string, failed: boolean) => {
    setNotice(message);
    setAgentActionFeedback((current) => ({ ...current, [binding.agent_id]: { message, failed } }));
    addNotification({ dedupeKey: `agent-action-${binding.agent_id}`, level: failed ? "error" : "success",
      title: failed ? (lang === "zh" ? "操作未完成" : "Action incomplete") : (lang === "zh" ? "操作完成" : "Action completed"),
      message, receipt: { requestId: createOperationRequestId("agent-action"), targetId: binding.agent_id } });
  };

  const runManagedTransport = async (binding: AgentUpstreamBinding, operation: "configure" | "clear") => {
    const report = (message: string, failed = true) => reportAgentAction(binding, message, failed);
    setAgentActionFeedback((current) => { const next = { ...current }; delete next[binding.agent_id]; return next; });
    // Dispatch to MCP integration for MCP/Hook agents.
    if (binding.integration_kind === "mcp" || binding.integration_kind === "hook") {
      await runManagedMcpIntegration(binding, operation);
      return;
    }
    const canOperate = operation === "configure"
      ? binding.can_configure === true
      : binding.can_clear === true;
    if (!canOperate) {
      report(lang === "zh"
        ? `${binding.agent_name} 未提供可执行的 d-token 写入动作；未修改任何配置。`
        : `${binding.agent_name} exposes no executable d-token write action; no configuration was modified.`);
      return;
    }
    const targetPath = binding.config_path;
    if (!targetPath || targetPath.startsWith("<local>/")) {
      report(lang === "zh" ? `${binding.agent_name} 的配置路径不可安全定位，未写入任何文件。` : `${binding.agent_name} has no safe local config path, so no file was written.`);
      return;
    }
    if (operation === "configure" && !serviceEndpoint.startsWith("http://127.0.0.1:") && !serviceEndpoint.startsWith("http://localhost:")) {
      report(lang === "zh" ? "请先启动并验证 d-token 本地服务，再配置 Agent。" : "Start and verify the d-token local service before configuring an Agent.");
      return;
    }
    setTransportBusyAgent(binding.agent_id);
    try {
      let result: Record<string, unknown>;
      {
        // Model comes only from the uniquely auto-resolved model stored in the
        // machine selection, or the Codex Responses passthrough marker ("").
        // Multi-model or unresolvable Agents block the whole batch; the plan
        // fails closed rather than guessing a first model.
        const model = operation === "configure"
          ? machine.selections[binding.agent_id]?.model
          : undefined;
        if (operation === "configure" && !model) {
          report(lang === "zh"
            ? `${binding.agent_name} 的上游/模型无法唯一解析，连接计划已被阻断；请修复来源配置后重新扫描。`
            : `${binding.agent_name}'s upstream/model cannot be uniquely resolved, so the plan is blocked. Fix the source configuration and rescan.`);
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
        const confirmed = operation === "configure" || typeof window === "undefined" || window.confirm(
          lang === "zh" ? `清除 ${binding.agent_name} 中由 d-token 添加的连接设置？其他设置会保留。` : `Remove the connection settings added by d-token for ${binding.agent_name}? Other settings will remain.`,
        );
        if (!confirmed) return;
        result = await invokeNative<Record<string, unknown>>("apply_reviewed_managed_agent_transport", {
          request: {
            ...request,
          },
          operation,
          reviewedRevision: review.revision,
          reviewedBeforeSha256: review.before_sha256,
          reviewedAfterSha256: review.after_sha256,
        });
      }
      const verification = result.verification as Record<string, unknown> | undefined;
      const ok = result.ok !== false && result.state !== "failed" && verification?.ok !== false;
      const changed = result.changed !== false;
      const outcomeNotice = ok && (operation !== "clear" || changed)
        ? (operation === "configure"
          ? (lang === "zh" ? `${binding.agent_name} 已完成 d-token 传输配置并验证。` : `${binding.agent_name} now uses a verified d-token transport.`)
          : (lang === "zh" ? `${binding.agent_name} 的 d-token 托管传输已清除。` : `The d-token-owned transport for ${binding.agent_name} was cleared.`))
        : (operation === "clear" && !changed
          ? (lang === "zh" ? "未发现可安全清除的 d-token 托管标记，未修改用户配置。" : "No d-token-owned marker was found; user configuration was not changed.")
          : (lang === "zh" ? "操作未完成，请重新扫描并检查该工具的连接状态。" : "The operation did not complete. Rescan and check this tool’s connection."));
      await refreshAll(false);
      report(outcomeNotice, !ok);
    } catch (error) {
      report(lang === "zh" ? "操作未完成，请重新扫描并检查该工具的连接状态。" : "The operation did not complete. Rescan and check this tool’s connection.");
    } finally {
      setTransportBusyAgent(null);
    }
  };

  const runManagedMcpIntegration = async (binding: AgentUpstreamBinding, operation: "configure" | "clear") => {
    const report = (message: string, failed = true) => reportAgentAction(binding, message, failed);
    const configPath = binding.mcp_config_path;
    if (!configPath || configPath.startsWith("<local>/")) {
      report(lang === "zh"
        ? `${binding.agent_name} 的 MCP 配置路径不可安全定位，未写入任何文件。`
        : `${binding.agent_name} MCP config path is not safely resolvable; no file was written.`);
      return;
    }
    if (operation === "configure" && !serviceEndpoint.startsWith("http://127.0.0.1:") && !serviceEndpoint.startsWith("http://localhost:")) {
      report(lang === "zh" ? "请先启动并验证 d-token 本地服务，再配置 MCP。" : "Start and verify the d-token local service before configuring MCP.");
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
      const confirmed = operation === "configure" || typeof window === "undefined" || window.confirm(
        lang === "zh" ? `移除 ${binding.agent_name} 中的 d-token 工具集成？当前模型连接会保留。` : `Remove the d-token tool integration from ${binding.agent_name}? The current model connection will remain.`,
      );
      if (!confirmed) return;
      const result = await invokeNative<Record<string, unknown>>("apply_managed_agent_mcp_integration", {
        request,
        reviewed_before_sha256: review.before_sha256,
        reviewed_after_sha256: review.after_sha256,
      });
      const verify = result.verification as Record<string, unknown> | undefined;
      const ok = result.ok !== false && result.state !== "failed" && verify?.ok !== false;
      const outcomeNotice = ok
        ? (operation === "configure"
          ? (lang === "zh" ? `${binding.agent_name} MCP 工具已配置并验证。` : `${binding.agent_name} MCP tool configured and verified.`)
          : (lang === "zh" ? `${binding.agent_name} MCP 条目已清除。` : `${binding.agent_name} MCP entry cleared.`))
        : (lang === "zh" ? "MCP 操作未完成，请检查连接状态后重试。" : "MCP operation did not complete. Check the connection and retry.");
      await refreshAll(false);
      report(outcomeNotice, !ok);
    } catch (error) {
      report(lang === "zh" ? "MCP 操作未完成，请检查连接状态后重试。" : "MCP operation did not complete. Check the connection and retry.");
    } finally {
      setTransportBusyAgent(null);
    }
  };

  return <InstallView {...{ lang, t, snapshot, discoveryStatus, phasePresentation, discoveryCounts, refreshAll, busy, pendingAction, handlePrimaryAction, onConfigureSelected: configureSelected, primaryAction, serviceInstanceDetail, servicePresentation, restartRequired, serviceOnline, serviceManaged, setStopError, setConfirmServiceStop, visibleBindings, machine, dispatch, ccSwitchPresentation, notice, transactionLogs, agentIntegrationReceipts, routeVerification, serviceEndpoint, credentialDialogOpen, setCredentialDialogOpen, setProviderCredential, setCredentialError, credentialBinding, credentialProviderId, providerCredential, credentialError, saveInlineCredential, confirmServiceStop, stopError, serviceStatus, serviceIdentifiersCurrent, stopService, transportBusyAgent, agentActionFeedback, onConfigureAgent: (binding: AgentUpstreamBinding) => {
    if (binding.resolution === "needs-credential") {
      setCredentialAgentId(binding.agent_id);
      setCredentialError("");
      setCredentialDialogOpen(true);
      return;
    }
    if (binding.integration_kind === "mcp" || binding.integration_kind === "hook") {
      void runManagedTransport(binding, "configure");
      return;
    }
    void configureSelected([binding.agent_id]);
  }, onClearAgent: (binding: AgentUpstreamBinding) => { void runManagedTransport(binding, "clear"); }, agentAutoFollow, setAgentAutoFollow, ccSwitchSyncReview, setCcSwitchSyncReview, ccSwitchAutoFollow, setCcSwitchAutoFollow, ccSwitchSyncBusy: pendingAction === "sync", onPreviewCcSwitchSync: () => { void previewCcSwitchSync(); }, onApplyCcSwitchSync: () => { void applyCcSwitchSync(); } }} />;
}
