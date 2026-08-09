import { useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { useI18n, useT } from "../../lib/i18n";
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
  selectedCandidate,
  subscribeInstallMachine,
  type InstallAgentReceipt,
  type InstallPhase,
} from "../../lib/installMachine";
import { useConnectionSetup, type AgentUpstreamBinding, type ConnectionSetupSnapshot, type UpstreamCandidate } from "../../lib/connectionSetup";
import { InstallView } from "../../lib/install/InstallView";
import { OperationDialog } from "../common/OperationDialog";
import { buildConnectionRoutePlanRequest, connectionDiscoveryCounts, derivePrimaryAction, desktopInstallRoot, hasRealAgentAction, hybridAgentIntegrationDraft, presentCcSwitch, presentInstallPhase, presentRouteVerificationSummary, presentServiceInstance, safeText, supportsAutomaticAgentIntegration } from "../../lib/install/presentation";
import type { ConnectionRoutePlanReceipt, ConnectionRoutePlanReview, DtokenServiceStatus, HybridAgentIntegrationApply, HybridAgentIntegrationReview, HybridAgentIntegrationVerification, ManagedAgentTransportReceipt, ManagedAgentTransportReview, OpenCodeConnectionReceipt, OpenCodeConnectionReview, RouteVerification } from "../../lib/install/presentation";
export { connectionDiscoveryCounts, desktopInstallRoot, presentConnectionPrivacy, presentRouteVerificationSummary, presentServiceInstance } from "../../lib/install/presentation";
export { buildConnectionRoutePlanRequest } from "../../lib/install/presentation";

/**
 * UI-11: per-agent managed operations (managed transport / MCP) confirm
 * through the shared OperationDialog instead of the native `window.confirm`.
 * Preview populates this state; the native apply runs only after the user
 * explicitly confirms the exact reviewed plan (hashes/revision).
 */
type ManagedOperationReview = { revision?: string; before_sha256: string; after_sha256: string };
type ManagedOperationConfirm =
  | { kind: "transport"; binding: AgentUpstreamBinding; operation: "configure" | "clear"; request: Record<string, unknown>; review: ManagedOperationReview }
  | { kind: "mcp"; binding: AgentUpstreamBinding; operation: "configure" | "clear"; request: Record<string, unknown>; review: ManagedOperationReview };

function shortHash(value: string | undefined): string {
  return value ? `${value.slice(0, 8)}…` : "—";
}

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
  const [managedConfirm, setManagedConfirm] = useState<ManagedOperationConfirm | null>(null);

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
    setShowReview(false);
    setConnectionReview(null);
    setRoutePlanReview(null);
    setCcSwitchSyncReview((current) =>
      current?.source_fingerprint === snapshot.external_config_snapshot?.cc_switch?.fingerprint
        ? current
        : null);
    setAgentIntegrationReviews({});
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
    setManagedTransportReviews({});
    setAgentIntegrationReviews({});
    const requestId = createOperationRequestId("connection-preview");
    const routePlan = buildConnectionRoutePlanRequest(snapshot.revision, actionableBindings, machine.selections);
    if (routePlan.bindings.length === 0) return;
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
      const [routeReview, openCodeReview, managedTransportReviewEntries, integrationReviewEntries] = await Promise.all([
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
      setRoutePlanReview(routeReview);
      setConnectionReview(openCodeReview);
      setManagedTransportReviews(Object.fromEntries(managedTransportReviewEntries));
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
    let appliedManagedTransports: AgentUpstreamBinding[] = [];
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
          enable_auto_follow: agentAutoFollow,
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
      dispatch({
        type: "SET_PHASE",
        phase: fullyVerified ? "verified" : "partial",
        message: fullyVerified ? "" : "agent-write-unavailable",
      });
      setShowReview(false);
      setNotice(fullyVerified
        ? (lang === "zh" ? "已确认配置生效、本地服务健康且路由可用。" : "Configuration is active, the local service is healthy, and the route is available.")
        : (lang === "zh" ? "d-token 服务已就绪；未通过安全 adapter 的 Agent 保持原配置。" : "d-token is ready; Agents without a verified safe adapter retain their existing configuration."));
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
      setNotice(lang === "zh"
        ? `${binding.agent_name} 未提供可执行的 d-token 写入动作；未修改任何配置。`
        : `${binding.agent_name} exposes no executable d-token write action; no configuration was modified.`);
      return;
    }
    const targetPath = binding.config_path;
    if (!targetPath || targetPath.startsWith("<local>/")) {
      setNotice(lang === "zh" ? `${binding.agent_name} 的配置路径不可安全定位，未写入任何文件。` : `${binding.agent_name} has no safe local config path, so no file was written.`);
      return;
    }
    if (operation === "configure" && !serviceEndpoint.startsWith("http://127.0.0.1:") && !serviceEndpoint.startsWith("http://localhost:")) {
      setNotice(lang === "zh" ? "请先启动并验证 d-token 本地服务，再配置 Agent。" : "Start and verify the d-token local service before configuring an Agent.");
      return;
    }
    setTransportBusyAgent(binding.agent_id);
    try {
      // Model comes only from the uniquely auto-resolved model stored in the
      // machine selection, or the Codex Responses passthrough marker ("").
      // Multi-model or unresolvable Agents block the whole batch; the plan
      // fails closed rather than guessing a first model.
      const model = operation === "configure"
        ? machine.selections[binding.agent_id]?.model
        : undefined;
      if (operation === "configure" && !model) {
        setNotice(lang === "zh"
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
      // The unified OperationDialog replaces the native confirm (UI-11): the
      // reviewed plan (revision + hashes) is shown and the apply below runs
      // only after an explicit confirm.
      setManagedConfirm({
        kind: "transport",
        binding,
        operation,
        request,
        review: { revision: review.revision, before_sha256: review.before_sha256, after_sha256: review.after_sha256 },
      });
    } catch (error) {
      setNotice(lang === "zh" ? "操作未完成，用户配置未被修改；请重新扫描后重试。" : "The operation did not complete and user configuration was not changed. Rescan and try again.");
    } finally {
      setTransportBusyAgent(null);
    }
  };

  const runManagedMcpIntegration = async (binding: AgentUpstreamBinding, operation: "configure" | "clear") => {
    const configPath = binding.mcp_config_path;
    if (!configPath || configPath.startsWith("<local>/")) {
      setNotice(lang === "zh"
        ? `${binding.agent_name} 的 MCP 配置路径不可安全定位，未写入任何文件。`
        : `${binding.agent_name} MCP config path is not safely resolvable; no file was written.`);
      return;
    }
    if (operation === "configure" && !serviceEndpoint.startsWith("http://127.0.0.1:") && !serviceEndpoint.startsWith("http://localhost:")) {
      setNotice(lang === "zh" ? "请先启动并验证 d-token 本地服务，再配置 MCP。" : "Start and verify the d-token local service before configuring MCP.");
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
      // The unified OperationDialog replaces the native confirm (UI-11): the
      // reviewed plan is shown and the apply runs only after explicit confirm.
      setManagedConfirm({
        kind: "mcp",
        binding,
        operation,
        request,
        review: { before_sha256: review.before_sha256, after_sha256: review.after_sha256 },
      });
    } catch (error) {
      setNotice(lang === "zh" ? "MCP 操作未完成，用户配置未被修改。" : "MCP operation did not complete; user configuration was not changed.");
    } finally {
      setTransportBusyAgent(null);
    }
  };

  /**
   * Applies the reviewed managed operation after the unified OperationDialog
   * confirm (UI-11). The dialog stays open with a pending spinner while the
   * native apply runs, so the confirm→apply boundary is explicit.
   */
  const applyManagedOperation = async () => {
    const confirm = managedConfirm;
    if (!confirm) return;
    setTransportBusyAgent(confirm.binding.agent_id);
    try {
      if (confirm.kind === "transport") {
        const result = await invokeNative<Record<string, unknown>>("apply_reviewed_managed_agent_transport", {
          request: {
            ...confirm.request,
          },
          operation: confirm.operation,
          reviewedRevision: confirm.review.revision,
          reviewedBeforeSha256: confirm.review.before_sha256,
          reviewedAfterSha256: confirm.review.after_sha256,
        });
        const verification = result.verification as Record<string, unknown> | undefined;
        const ok = result.ok !== false && result.state !== "failed" && verification?.ok !== false;
        const changed = result.changed !== false;
        const outcomeNotice = ok && (confirm.operation !== "clear" || changed)
          ? (confirm.operation === "configure"
            ? (lang === "zh" ? `${confirm.binding.agent_name} 已完成 d-token 传输配置并验证。` : `${confirm.binding.agent_name} now uses a verified d-token transport.`)
            : (lang === "zh" ? `${confirm.binding.agent_name} 的 d-token 托管传输已清除。` : `The d-token-owned transport for ${confirm.binding.agent_name} was cleared.`))
          : (confirm.operation === "clear" && !changed
            ? (lang === "zh" ? "未发现可安全清除的 d-token 托管标记，未修改用户配置。" : "No d-token-owned marker was found; user configuration was not changed.")
            : (lang === "zh" ? "操作未完成，用户配置未被修改；请重新扫描后重试。" : "The operation did not complete and user configuration was not changed. Rescan and try again."));
        await refreshAll();
        setNotice(outcomeNotice);
      } else {
        const result = await invokeNative<Record<string, unknown>>("apply_managed_agent_mcp_integration", {
          request: confirm.request,
          reviewed_before_sha256: confirm.review.before_sha256,
          reviewed_after_sha256: confirm.review.after_sha256,
        });
        const verify = result.verification as Record<string, unknown> | undefined;
        const ok = result.ok !== false && result.state !== "failed" && verify?.ok !== false;
        const outcomeNotice = ok
          ? (confirm.operation === "configure"
            ? (lang === "zh" ? `${confirm.binding.agent_name} MCP 工具已配置并验证。` : `${confirm.binding.agent_name} MCP tool configured and verified.`)
            : (lang === "zh" ? `${confirm.binding.agent_name} MCP 条目已清除。` : `${confirm.binding.agent_name} MCP entry cleared.`))
          : (lang === "zh" ? "MCP 操作未完成，用户配置未被修改。" : "MCP operation did not complete; user configuration was not changed.");
        await refreshAll();
        setNotice(outcomeNotice);
      }
    } catch (error) {
      setNotice(confirm.kind === "mcp"
        ? (lang === "zh" ? "MCP 操作未完成，用户配置未被修改。" : "MCP operation did not complete; user configuration was not changed.")
        : (lang === "zh" ? "操作未完成，用户配置未被修改；请重新扫描后重试。" : "The operation did not complete and user configuration was not changed. Rescan and try again."));
    } finally {
      setTransportBusyAgent(null);
      setManagedConfirm(null);
    }
  };

  return (
    <>
      <InstallView {...{ lang, t, snapshot, discoveryStatus, phasePresentation, discoveryCounts, refreshAll, busy, pendingAction, handlePrimaryAction, primaryAction, serviceInstanceDetail, servicePresentation, restartRequired, serviceOnline, serviceManaged, setStopError, setConfirmServiceStop, visibleBindings, primaryBindings, machine, dispatch, secondaryBindings, ccSwitchPresentation, notice, transactionLogs, agentIntegrationReceipts, routeVerification, showReview, setShowReview, setConnectionReview, setRoutePlanReview, setManagedTransportReviews, setAgentIntegrationReviews, actionableBindings, serviceEndpoint, routePlanReview, connectionReview, managedTransportReviews, agentIntegrationReviews, openCodeSelection, applyConnectionPlan, credentialDialogOpen, setCredentialDialogOpen, setProviderCredential, setCredentialError, credentialBinding, credentialProviderId, providerCredential, credentialError, saveInlineCredential, confirmServiceStop, stopError, serviceStatus, serviceIdentifiersCurrent, stopService, transportBusyAgent, onConfigureAgent: (binding: AgentUpstreamBinding) => { void runManagedTransport(binding, "configure"); }, onClearAgent: (binding: AgentUpstreamBinding) => { void runManagedTransport(binding, "clear"); }, agentAutoFollow, setAgentAutoFollow, ccSwitchSyncReview, setCcSwitchSyncReview, ccSwitchAutoFollow, setCcSwitchAutoFollow, ccSwitchSyncBusy: pendingAction === "sync", onPreviewCcSwitchSync: () => { void previewCcSwitchSync(); }, onApplyCcSwitchSync: () => { void applyCcSwitchSync(); } }} />
      <OperationDialog
        open={managedConfirm !== null}
        onOpenChange={(open) => { if (!open && transportBusyAgent === null) setManagedConfirm(null); }}
        title={managedConfirm
          ? (managedConfirm.operation === "configure"
            ? (managedConfirm.kind === "mcp" ? t("install.mcpConfigureTitle") : t("install.transportConfigureTitle"))
            : (managedConfirm.kind === "mcp" ? t("install.mcpClearTitle") : t("install.transportClearTitle")))
          : ""}
        summary={managedConfirm
          ? (managedConfirm.operation === "configure"
            ? (managedConfirm.kind === "mcp"
              ? t("install.mcpConfigureSummary", { path: String(managedConfirm.request.config_path ?? "") })
              : t("install.transportConfigureSummary", { path: String(managedConfirm.request.target_path ?? "") }))
            : (managedConfirm.kind === "mcp"
              ? t("install.mcpClearSummary", { path: String(managedConfirm.request.config_path ?? "") })
              : t("install.transportClearSummary", { path: String(managedConfirm.request.target_path ?? "") })))
          : ""}
        impact={managedConfirm
          ? (managedConfirm.kind === "mcp" ? String(managedConfirm.request.config_path ?? "") : String(managedConfirm.request.target_path ?? ""))
          : undefined}
        details={managedConfirm ? (
          <div className="space-y-1 font-mono text-xs text-muted-foreground">
            {managedConfirm.review.revision && (
              <div>{t("install.reviewRevision")}: {managedConfirm.review.revision}</div>
            )}
            <div>
              {t("install.reviewHashes")}: before {shortHash(managedConfirm.review.before_sha256)} → after {shortHash(managedConfirm.review.after_sha256)}
            </div>
          </div>
        ) : null}
        confirmLabel={t("common.apply")}
        cancelLabel={t("common.cancel")}
        risk={managedConfirm?.operation === "clear" ? "warning" : "normal"}
        pending={transportBusyAgent !== null}
        onConfirm={() => { void applyManagedOperation(); }}
      />
    </>
  );
}
