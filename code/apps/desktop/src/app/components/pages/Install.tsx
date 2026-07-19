import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { useI18n, useT } from "../../lib/i18n";
import { invokeNative, type NativeActionLog, type NativeActionResult } from "../../lib/native";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";
import {
  bindingOptions,
  canReviewInstall,
  deriveInstallPhase,
  dispatchInstallMachine,
  getInstallMachineState,
  runInstallAgentBatch,
  selectedCandidate,
  subscribeInstallMachine,
  type InstallAgentReceipt,
  type InstallPhase,
} from "../../lib/installMachine";
import { useConnectionSetup, type AgentUpstreamBinding, type ConnectionSetupSnapshot, type UpstreamCandidate } from "../../lib/connectionSetup";
import { InstallView } from "../../lib/install/InstallView";
import { buildConnectionRoutePlanRequest, connectionDiscoveryCounts, derivePrimaryAction, desktopInstallRoot, hybridAgentIntegrationDraft, presentCcSwitch, presentInstallPhase, presentRouteVerificationSummary, presentServiceInstance, safeText, supportsAutomaticAgentIntegration } from "../../lib/install/presentation";
import type { ConnectionRoutePlanReceipt, ConnectionRoutePlanReview, DtokenServiceStatus, HybridAgentIntegrationApply, HybridAgentIntegrationReview, HybridAgentIntegrationVerification, OpenCodeConnectionReceipt, OpenCodeConnectionReview, RouteVerification } from "../../lib/install/presentation";
export { connectionDiscoveryCounts, desktopInstallRoot, presentConnectionPrivacy, presentRouteVerificationSummary, presentServiceInstance } from "../../lib/install/presentation";

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

export function Install({ onOpenAdvanced }: { onOpenAdvanced?: () => void } = {}) {
  const t = useT();
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const { snapshot, status: discoveryStatus, error: discoveryError, refresh: refreshDiscovery } = useConnectionSetup();
  const { payload, status: payloadStatus, operational, updatedAt, refresh: refreshRuntime } = useRuntimeDashboard();
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
  const [agentIntegrationReviews, setAgentIntegrationReviews] = useState<Record<string, HybridAgentIntegrationReview>>({});
  const [agentIntegrationReceipts, setAgentIntegrationReceipts] = useState<InstallAgentReceipt[]>([]);
  const [routeVerification, setRouteVerification] = useState<RouteVerification | null>(null);
  const [transactionLogs, setTransactionLogs] = useState<NativeActionLog[]>([]);
  const [notice, setNotice] = useState("");
  const [pendingAction, setPendingAction] = useState<"discover" | "preview" | "apply" | "verify" | "restart" | "stop" | "credential" | null>(null);

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
  const selectedOpenCode = actionableBindings.find((binding) => binding.agent_id === "opencode");
  const credentialBinding = actionableBindings.find((binding) => binding.resolution === "needs-credential") ?? null;
  const credentialProviderId = credentialBinding?.resolved_candidate?.id
    ?? credentialBinding?.candidates[0]?.id
    ?? "";
  const openCodeSelection = selectedOpenCode
    ? selectedCandidate(selectedOpenCode, machine.selections[selectedOpenCode.agent_id])
    : null;
  const reviewReady = snapshot ? canReviewInstall(snapshot, machine.selections, machine.selectedAgentIds) : false;
  const primaryBindings = useMemo(
    () => visibleBindings.filter((binding) => binding.capability === "one-click" || binding.already_connected),
    [visibleBindings],
  );
  const secondaryBindings = useMemo(
    () => visibleBindings.filter((binding) => !primaryBindings.includes(binding)),
    [primaryBindings, visibleBindings],
  );
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
  const ccSwitchPresentation = presentCcSwitch(snapshot?.cc_switch_sync?.status, Boolean(snapshot?.cc_switch_sync?.detected), lang);

  useEffect(() => {
    if (snapshot) dispatch({ type: "DISCOVERED", snapshot });
  }, [snapshot]);

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
    setNotice(lang === "zh" ? "正在重新读取 Agent、上游、cc-switch 与 d-token 状态…" : "Refreshing Agents, upstreams, cc-switch, and d-token state…");
    dispatch({ type: "DISCOVER_START" });
    try {
      const [discovery] = await Promise.all([refreshDiscovery(), refreshRuntime()]);
      setNotice(discovery
        ? (lang === "zh" ? "本机连接状态已更新。" : "Local connection state refreshed.")
        : (lang === "zh" ? "当前是浏览器预览，不读取或改写本机配置。" : "Browser preview does not read or modify local configuration."));
      addNotification({
        dedupeKey: "connection-refresh",
        level: discovery ? "success" : "info",
        title: discovery ? (lang === "zh" ? "连接状态已更新" : "Connection state refreshed") : (lang === "zh" ? "连接状态不可用" : "Connection state unavailable"),
        message: discovery ? (lang === "zh" ? "Agent、上游、cc-switch 与 d-token 状态已重新读取。" : "Agent, upstream, cc-switch, and d-token state were refreshed.") : (lang === "zh" ? "浏览器预览不会读取本机配置。" : "Browser preview does not read local configuration."),
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
    setAgentIntegrationReviews({});
    const requestId = createOperationRequestId("connection-preview");
    const routePlan = buildConnectionRoutePlanRequest(snapshot.revision, actionableBindings, machine.selections);
    if (routePlan.bindings.length === 0) return;
    setPendingAction("preview");
    try {
      const integrationBindings = actionableBindings.filter((binding) =>
        supportsAutomaticAgentIntegration(binding.agent_id) && !binding.already_connected,
      );
      const [routeReview, openCodeReview, integrationReviewEntries] = await Promise.all([
        invokeNative<ConnectionRoutePlanReview>("preview_connection_route_plan", { request: routePlan }),
        openCodeSelection?.model
          ? invokeNative<OpenCodeConnectionReview>("preview_opencode_connection", {
              request: {
                model: openCodeSelection.model,
                listener_port: 18001,
                reviewed_revision: null,
                reviewed_before_sha256: null,
                reviewed_after_sha256: null,
              },
            })
          : Promise.resolve(null),
        Promise.all(integrationBindings.map(async (binding) => [
          binding.agent_id,
          await invokeNative<HybridAgentIntegrationReview>("preview_hybrid_agent_integration", {
            request: hybridAgentIntegrationDraft(binding.agent_id),
          }),
        ] as const)),
      ]);
      setRoutePlanReview(routeReview);
      setConnectionReview(openCodeReview);
      setAgentIntegrationReviews(Object.fromEntries(integrationReviewEntries));
      if (!routeReview.ok) {
        const failure = presentOperationFailure("connection", routeReview.error, lang);
        setNotice(failure.message);
        dispatch({ type: "SET_PHASE", phase: "blocked", message: failure.message });
        addNotification({ dedupeKey: "connection-preview-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "connection-plan" } });
      } else {
        addNotification({ dedupeKey: `connection-preview-${routeReview.after_sha256 ?? "ready"}`, level: "info", title: lang === "zh" ? "连接变更已准备" : "Connection changes are ready", message: lang === "zh" ? "差异和验证证据已准备。请查看当前变更后再应用。" : "Differences and verification evidence are ready. View the current changes before applying.", receipt: { requestId, targetId: "connection-plan" } });
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
      setNotice(lang === "zh" ? "源配置已变化，旧方案已失效，正在重新读取。" : "Source configuration changed; the old plan was invalidated and will be rebuilt.");
      await refreshAll();
      return;
    }
    setPendingAction("apply");
    const requestId = createOperationRequestId("connection-apply");
    const routePlan = buildConnectionRoutePlanRequest(snapshot.revision, actionableBindings, machine.selections);
    const routeChanges = routePlanReview?.before_sha256 !== routePlanReview?.after_sha256;
    const wasManaged = serviceManaged;
    let routeReceipt: ConnectionRoutePlanReceipt | null = null;
    let openCodeReceipt: OpenCodeConnectionReceipt | null = null;
    let appliedAgentReceipts: InstallAgentReceipt[] = [];
    let rolledBack = false;
    try {
      if (!routePlanReview?.ok || !routePlanReview.before_sha256 || !routePlanReview.after_sha256) {
        throw new Error("connection_preview_incomplete");
      }
      if (routeChanges && serviceOnline && !serviceManaged) {
        throw new Error(lang === "zh" ? "18001 端口由非本应用启动的 d-token 服务占用，无法安全重载路由。" : "Port 18001 is owned by an unmanaged d-token service, so the route cannot be reloaded safely.");
      }
      if (routeChanges && serviceManaged) {
        const stopped = await invokeNative<NativeActionResult>("stop_dtoken_service");
        assertNativeActionSucceeded(stopped);
      }
      routeReceipt = await invokeNative<ConnectionRoutePlanReceipt>("apply_connection_route_plan", {
        request: {
          plan: routePlan,
          reviewed_before_sha256: routePlanReview.before_sha256,
          reviewed_after_sha256: routePlanReview.after_sha256,
        },
      });
      if (!routeReceipt.ok || !routeReceipt.verified) throw new Error(routeReceipt.summary);
      if (!serviceOnline || routeChanges) await installRuntime();

      dispatch({ type: "SET_PHASE", phase: "applying-agent-configs" });
      if (openCodeSelection?.model) {
        if (!connectionReview?.ok || !connectionReview.revision || !connectionReview.before_sha256 || !connectionReview.after_sha256) {
          throw new Error("opencode_preview_incomplete");
        }
        openCodeReceipt = await invokeNative<OpenCodeConnectionReceipt>("apply_opencode_connection", {
          request: {
            model: openCodeSelection.model,
            listener_port: 18001,
            reviewed_revision: connectionReview.revision,
            reviewed_before_sha256: connectionReview.before_sha256,
            reviewed_after_sha256: connectionReview.after_sha256,
          },
        });
        if (!openCodeReceipt.ok) throw new Error(openCodeReceipt.summary);
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
      const manualBindings = actionableBindings.filter((binding) =>
        !supportsAutomaticAgentIntegration(binding.agent_id) && !binding.already_connected,
      );
      const fullyVerified = (openCodeReceipt === null || openCodeReceipt.verified)
        && agentBatch.ok
        && manualBindings.length === 0;
      dispatch({
        type: "SET_PHASE",
        phase: fullyVerified ? "verified" : "partial",
        message: fullyVerified ? "" : "manual-agent-step-required",
      });
      setShowReview(false);
      setNotice(fullyVerified
        ? (lang === "zh" ? "已确认配置生效、本地服务健康且路由可用。" : "Configuration is active, the local service is healthy, and the route is available.")
        : (lang === "zh" ? "d-token 服务已就绪；仍有 Agent 需要按当前变更手动完成配置。" : "d-token is ready; one or more Agents still require a manual configuration step."));
      addNotification({
        dedupeKey: `connection-plan-${fullyVerified ? "verified" : "partial"}`,
        receipt: { requestId },
        level: fullyVerified ? "success" : "warning",
        title: fullyVerified ? (lang === "zh" ? "Agent 连接已验证" : "Agent connection verified") : (lang === "zh" ? "连接部分完成" : "Connection partially complete"),
        message: fullyVerified
          ? (lang === "zh" ? "配置、服务和路由证据已通过验证。" : "Configuration, service, and route evidence passed verification.")
          : (lang === "zh" ? "已保留服务与已验证配置，未支持的 Agent 没有被静默写入。" : "The service and verified changes were kept; unsupported Agent configs were not silently modified."),
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

  return <InstallView {...{ lang, t, snapshot, discoveryStatus, phasePresentation, discoveryCounts, refreshAll, busy, pendingAction, handlePrimaryAction, primaryAction, serviceInstanceDetail, servicePresentation, restartRequired, serviceOnline, serviceManaged, setStopError, setConfirmServiceStop, visibleBindings, primaryBindings, machine, dispatch, secondaryBindings, ccSwitchPresentation, notice, transactionLogs, agentIntegrationReceipts, routeVerification, showReview, setShowReview, setConnectionReview, setRoutePlanReview, setAgentIntegrationReviews, actionableBindings, serviceEndpoint, routePlanReview, connectionReview, agentIntegrationReviews, openCodeSelection, applyConnectionPlan, credentialDialogOpen, setCredentialDialogOpen, setProviderCredential, setCredentialError, credentialBinding, credentialProviderId, providerCredential, credentialError, saveInlineCredential, confirmServiceStop, stopError, serviceStatus, serviceIdentifiersCurrent, stopService }} />;
}
