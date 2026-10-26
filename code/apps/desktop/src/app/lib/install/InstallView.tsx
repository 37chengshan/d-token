import { ChevronDown, RefreshCw, Rocket, Server, ShieldCheck } from "lucide-react";
import { StatusBadge } from "../../components/common/StatusBadge";
import { OperationDialog } from "../../components/common/OperationDialog";
import { Button } from "../../components/ui/button";
import { Badge } from "../../components/ui/badge";
import type { Health } from "../runtime/types";
import type { NativeActionLog } from "../native";
import type { AgentUpstreamBinding, ConnectionSetupSnapshot } from "../connectionSetup";
import {
  canJoinInstallBatch,
  type InstallAgentReceipt,
  type InstallMachineEvent,
  type InstallMachineState,
} from "../installMachine";
import { redactString } from "../security/redaction";
import {
  AgentBindingRow,
  machineMessage,
  presentConnectionPrivacy,
  presentRouteVerificationSummary,
  shortHash,
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
  onConfigureSelected: () => Promise<void>;
  primaryAction: { label: string; disabled: boolean; icon: string };
  serviceInstanceDetail: string;
  servicePresentation: { isOnline: boolean; status: Health; phase: string };
  restartRequired: boolean;
  serviceOnline: boolean;
  serviceManaged: boolean;
  setStopError: (value: string) => void;
  setConfirmServiceStop: (value: boolean) => void;
  visibleBindings: AgentUpstreamBinding[];
  machine: InstallMachineState;
  dispatch: (event: InstallMachineEvent) => void;
  ccSwitchPresentation: { health: Health; label: string };
  notice: string;
  transactionLogs: NativeActionLog[];
  agentIntegrationReceipts: InstallAgentReceipt[];
  routeVerification: RouteVerification | null;
  serviceEndpoint: string;
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
  agentActionFeedback: Record<string, { message: string; failed: boolean }>;
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
  const { lang, t, snapshot, discoveryStatus, phasePresentation, discoveryCounts, refreshAll, busy, pendingAction, handlePrimaryAction, onConfigureSelected, primaryAction, serviceInstanceDetail, servicePresentation, restartRequired, serviceOnline, serviceManaged, setStopError, setConfirmServiceStop, visibleBindings, machine, dispatch, ccSwitchPresentation, notice, transactionLogs, agentIntegrationReceipts, routeVerification, serviceEndpoint, credentialDialogOpen, setCredentialDialogOpen, setProviderCredential, setCredentialError, credentialBinding, credentialProviderId, providerCredential, credentialError, saveInlineCredential, confirmServiceStop, stopError, serviceStatus, serviceIdentifiersCurrent, stopService, transportBusyAgent, agentActionFeedback, onConfigureAgent, onClearAgent, agentAutoFollow, setAgentAutoFollow, ccSwitchSyncReview, setCcSwitchSyncReview, ccSwitchAutoFollow, setCcSwitchAutoFollow, ccSwitchSyncBusy, onPreviewCcSwitchSync, onApplyCcSwitchSync } = props;
  const zh = lang === "zh";
  const batchBindings = visibleBindings.filter(canJoinInstallBatch);
  const selectedCount = batchBindings.filter((binding) => machine.selectedAgentIds.includes(binding.agent_id)).length;
  const allSelected = batchBindings.length > 0 && selectedCount === batchBindings.length;
  const receiptByAgent = new Map(agentIntegrationReceipts.map((receipt) => [receipt.agentId, receipt]));
  const showActionableStatus = phasePresentation.health !== "healthy" || discoveryStatus === "unavailable" || discoveryStatus === "stale";
  const toggleAll = () => {
    dispatch({ type: "SET_AGENT_SELECTION", agentIds: allSelected ? [] : batchBindings.map((binding) => binding.agent_id) });
  };
  const renderBindingCard = (binding: AgentUpstreamBinding) => {
    const selected = machine.selectedAgentIds.includes(binding.agent_id);
    const selectable = canJoinInstallBatch(binding);
    return (
      <AgentBindingRow
        key={binding.agent_id}
        binding={binding}
        lang={lang}
        busy={busy}
        layout="card"
        transportBusy={transportBusyAgent === binding.agent_id}
        onConfigure={() => onConfigureAgent(binding)}
        onClear={() => onClearAgent(binding)}
        selected={selected}
        selectable={selectable}
        onToggle={selectable ? () => dispatch({ type: "TOGGLE_AGENT", agentId: binding.agent_id }) : undefined}
        receipt={receiptByAgent.get(binding.agent_id)}
        actionFeedback={agentActionFeedback[binding.agent_id]}
      />
    );
  };
  return (
    <div className="page-enter mx-auto w-full max-w-6xl space-y-4">
      <section className="overflow-hidden rounded-xl">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border/60 px-5 py-3">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <label className="inline-flex items-center gap-2 text-sm font-medium">
              <input
                type="checkbox"
                checked={allSelected}
                ref={(input) => {
                  if (input) input.indeterminate = selectedCount > 0 && !allSelected;
                }}
                disabled={busy || batchBindings.length === 0}
                onChange={toggleAll}
              />
              {zh ? "全选" : "Select all"}
            </label>
            <Badge variant="secondary">
              {zh
                ? `已选 ${selectedCount} / 可配置 ${batchBindings.length}`
                : `${selectedCount} / ${batchBindings.length} selected`}
            </Badge>
            {showActionableStatus && <StatusBadge status={phasePresentation.health} label={phasePresentation.label} />}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="ghost" size="sm" onClick={() => void refreshAll()} disabled={busy}>
              <RefreshCw className={`size-4 ${pendingAction === "discover" ? "animate-spin" : ""}`} />
              {pendingAction === "discover" ? (zh ? "扫描中..." : "Scanning...") : (zh ? "重新扫描" : "Scan again")}
            </Button>
            <Button size="sm" onClick={() => void onConfigureSelected()} disabled={busy || selectedCount === 0}>
              {busy ? <RefreshCw className="size-4 animate-spin" /> : <Rocket className="size-4" />}
              {busy ? (zh ? "处理中..." : "Working...") : (zh ? `配置所选${selectedCount > 0 ? ` (${selectedCount})` : ""}` : `Configure selected${selectedCount > 0 ? ` (${selectedCount})` : ""}`)}
            </Button>
          </div>
        </div>

        <div className="px-5 py-4">
            {discoveryStatus === "discovering" && !snapshot ? (
              <div className="grid items-start gap-2 sm:grid-cols-2 xl:grid-cols-4" aria-label={zh ? "正在发现 Agent" : "Discovering Agents"}>
                {[0, 1, 2, 3].map((item) => <div key={item} className="h-24 animate-pulse rounded-lg bg-muted/50" />)}
              </div>
            ) : (
              <div>
                <div className="grid items-start gap-2 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                  {visibleBindings.map(renderBindingCard)}
                </div>
                {visibleBindings.length === 0 && !notice && !machine.message && (
                  <div className="py-5 text-sm text-muted-foreground">
                    {discoveryStatus === "unavailable"
                      ? (zh ? "当前是浏览器预览，不读取或改写本机配置。" : "Browser preview does not read or modify local configuration.")
                      : (zh ? "本机尚未检测到可接入的 AI 编码工具。" : "No connectable AI coding Agent was detected locally.")}
                  </div>
                )}
              </div>
            )}
        </div>

        <div className="grid gap-3 border-t border-border/60 px-5 py-3 text-xs text-muted-foreground md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
          <div className="flex min-w-0 flex-wrap items-center gap-3">
            <span className="inline-flex items-center gap-1.5">
              <Server className="size-3.5" />
              {serviceInstanceDetail}
            </span>
            <StatusBadge
              status={servicePresentation.status}
              label={restartRequired
                ? (zh ? "需要重启" : "Restart required")
                : serviceOnline ? (zh ? "运行中" : "Running") : (zh ? "未运行" : "Stopped")}
            />
            <span>
              {zh
                ? `工具 ${discoveryCounts.agents} · 上游 ${discoveryCounts.upstreams} · cc-switch ${discoveryCounts.ccSwitchProfiles}`
                : `${discoveryCounts.agents} tools · ${discoveryCounts.upstreams} upstreams · ${discoveryCounts.ccSwitchProfiles} cc-switch`}
            </span>
          </div>
          <div className="flex flex-wrap items-center justify-end gap-2">
            {restartRequired && (
              <Button variant="outline" size="sm" onClick={() => void handlePrimaryAction()} disabled={busy || primaryAction.disabled}>
                <RefreshCw className="size-4" />
                {zh ? "重启服务" : "Restart service"}
              </Button>
            )}
            {serviceManaged && (
              <Button variant="ghost" size="sm" onClick={() => { setStopError(""); setConfirmServiceStop(true); }} disabled={busy}>
                {zh ? "停止" : "Stop"}
              </Button>
            )}
            {["already-connected", "verified"].includes(machine.phase) && (
              <Button variant="outline" size="sm" onClick={() => void handlePrimaryAction()} disabled={busy || primaryAction.disabled}>
                <ShieldCheck className="size-4" />
                {serviceOnline ? (zh ? "重新验证" : "Verify again") : (zh ? "启动并验证" : "Start and verify")}
              </Button>
            )}
          </div>
        </div>

        <div className="border-t border-border/60 px-5 py-3">
          <details className="group text-xs text-muted-foreground">
            <summary className="flex cursor-pointer list-none items-center justify-between gap-3 font-medium text-foreground">
              <span>{zh ? "连接选项与操作记录" : "Connection options and action log"}</span>
              <ChevronDown className="size-3.5 transition-transform group-open:rotate-180" />
            </summary>
            <div className="mt-3 space-y-3">
              {batchBindings.length > 0 && (
                <label className="flex items-start gap-2 leading-5">
                  <input
                    type="checkbox"
                    className="mt-0.5"
                    checked={agentAutoFollow}
                    disabled={busy}
                    onChange={(event) => setAgentAutoFollow(event.target.checked)}
                  />
                  <span>{zh ? "模型设置变化时自动更新连接；需要新授权时提醒我。" : "Sync model setting changes and ask again when new authorization is needed."}</span>
                </label>
              )}
              <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-center">
                <div className="min-w-0">
                  <div className="font-medium">cc-switch</div>
                  <div className="mt-1 truncate text-xs text-muted-foreground">
                    {snapshot?.cc_switch_sync?.detected
                      ? [snapshot.cc_switch_sync.profile, snapshot.cc_switch_sync.upstream, snapshot.cc_switch_sync.connection_path].filter(Boolean).join(" · ") || (lang === "zh" ? "已只读检测" : "Detected read-only")
                      : (lang === "zh" ? "未发现 cc-switch；不会自动安装或改写第三方配置。" : "cc-switch was not found; third-party configuration is never installed or rewritten automatically.")}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  {snapshot?.cc_switch_sync?.status === "out-of-sync" && (
                    <Button size="sm" variant="outline" disabled={busy || ccSwitchSyncBusy} onClick={onPreviewCcSwitchSync}>
                      {lang === "zh" ? "同步当前上游" : "Sync current upstream"}
                    </Button>
                  )}
                  <StatusBadge
                    status={ccSwitchPresentation.health}
                    label={ccSwitchPresentation.label}
                  />
                </div>
              </div>
              <div>{presentConnectionPrivacy(snapshot?.privacy, lang)}</div>
              <div className="break-all font-mono text-[11px]">revision {snapshot?.revision ?? "—"} · source {snapshot?.source ?? "unavailable"}</div>
              {transactionLogs.slice(-8).map((entry, index) => (
                <div key={`${entry.time}-${index}`} className={`break-words font-mono text-[11px] ${entry.level === "error" ? "text-rose-600" : entry.level === "warn" ? "text-amber-700" : ""}`}>
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
        </div>

        {(notice || machine.message) && (
          <div className="border-t border-border/60 bg-muted/25 px-6 py-3 text-sm text-muted-foreground" role="status">
            {notice || machineMessage(machine.message, lang)}
          </div>
        )}
      </section>

      <OperationDialog
        open={Boolean(ccSwitchSyncReview)}
        onOpenChange={(open) => { if (!open) setCcSwitchSyncReview(null); }}
        title={lang === "zh" ? "同步 cc-switch 当前上游" : "Sync current cc-switch upstream"}
        summary={lang === "zh" ? "只更新 d-token 自己的路由；不会改写 cc-switch、Agent 现有 provider 或凭据。" : "Only d-token's route changes; cc-switch, existing Agent providers, and credentials are not rewritten."}
        impact={ccSwitchSyncReview?.target ?? "d-token active route"}
        confirmLabel={lang === "zh" ? "确认同步并重新验证" : "Confirm sync and reverify"}
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
                {lang === "zh"
                  ? "跟随当前上游：仅自动切换到本次确认且无需新凭据的 provider/endpoint/protocol；新身份会停止并要求重新确认。"
                  : "Follow current upstream: only identities confirmed in this step and requiring no new credential may sync automatically; a new provider, endpoint, or protocol stops for confirmation."}
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
        open={credentialDialogOpen}
        onOpenChange={(open) => {
          setCredentialDialogOpen(open);
          if (!open) {
            setProviderCredential("");
            setCredentialError("");
          }
        }}
        title={lang === "zh" ? "补充所需凭据" : "Add required credential"}
        summary={lang === "zh" ? "凭据仅写入系统凭据库；配置只保存不透明引用。" : "The credential is stored only in the operating-system credential store; configuration keeps an opaque reference."}
        impact={credentialBinding ? `${credentialBinding.agent_name} · ${credentialProviderId || "provider"}` : "provider"}
        confirmLabel={lang === "zh" ? "安全保存并继续" : "Save securely and continue"}
        cancelLabel={t("common.cancel")}
        pending={pendingAction === "credential"}
        confirmDisabled={!credentialProviderId || !providerCredential.trim()}
        onConfirm={saveInlineCredential}
        details={(
          <div className="space-y-2 text-sm">
            <label className="grid gap-1">
              <span>{lang === "zh" ? "凭据" : "Credential"}</span>
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
          title={lang === "zh" ? "停止 d-token 本地服务？" : "Stop the d-token local service?"}
          summary={lang === "zh" ? "停止后新的 Agent 请求将无法经过 d-token，现有配置与回退点会保留。" : "New Agent requests cannot pass through d-token after stopping; configuration and rollback points remain."}
          impact={serviceStatus?.pid && serviceIdentifiersCurrent
            ? `PID ${serviceStatus.pid}${serviceEndpoint ? ` · ${serviceEndpoint}` : ""}`
            : serviceEndpoint || (lang === "zh" ? "当前实例" : "Current instance")}
          confirmLabel={lang === "zh" ? "确认停止" : "Stop service"}
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
