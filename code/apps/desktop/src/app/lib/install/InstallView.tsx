import { AlertTriangle, ChevronDown, RefreshCw, Rocket, Server, Settings2, ShieldCheck } from "lucide-react";
import { Surface } from "../../components/common/Surface";
import { StatusBadge } from "../../components/common/StatusBadge";
import { DataSourceBadge } from "../../components/common/DataSourceBadge";
import { OperationDialog } from "../../components/common/OperationDialog";
import { Button } from "../../components/ui/button";
import { Badge } from "../../components/ui/badge";
import type { Health } from "../runtime/types";
import type { NativeActionLog } from "../native";
import type { AgentUpstreamBinding, ConnectionSetupSnapshot } from "../connectionSetup";
import type { InstallAgentReceipt, InstallMachineEvent, InstallMachineState } from "../installMachine";
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
  OpenCodeConnectionReview,
  RouteVerification,
} from "./presentation";

type PendingAction = "discover" | "preview" | "apply" | "verify" | "restart" | "stop" | "credential" | null;

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
  setAgentIntegrationReviews: (value: Record<string, HybridAgentIntegrationReview>) => void;
  actionableBindings: AgentUpstreamBinding[];
  serviceEndpoint: string;
  routePlanReview: ConnectionRoutePlanReview | null;
  connectionReview: OpenCodeConnectionReview | null;
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
};

export function InstallView(props: InstallViewProps) {
  const { lang, t, snapshot, discoveryStatus, phasePresentation, discoveryCounts, refreshAll, busy, pendingAction, handlePrimaryAction, primaryAction, serviceInstanceDetail, servicePresentation, restartRequired, serviceOnline, serviceManaged, setStopError, setConfirmServiceStop, visibleBindings, primaryBindings, machine, dispatch, secondaryBindings, ccSwitchPresentation, notice, transactionLogs, agentIntegrationReceipts, routeVerification, showReview, setShowReview, setConnectionReview, setRoutePlanReview, setAgentIntegrationReviews, actionableBindings, serviceEndpoint, routePlanReview, connectionReview, agentIntegrationReviews, openCodeSelection, applyConnectionPlan, credentialDialogOpen, setCredentialDialogOpen, setProviderCredential, setCredentialError, credentialBinding, credentialProviderId, providerCredential, credentialError, saveInlineCredential, confirmServiceStop, stopError, serviceStatus, serviceIdentifiersCurrent, stopService } = props;
  return (
    <div className="page-enter mx-auto w-full max-w-6xl space-y-4">
      <Surface className="overflow-hidden">
        <div className="flex flex-wrap items-start justify-between gap-4 border-b border-border/60 px-6 py-5">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-xl font-semibold tracking-tight">{lang === "zh" ? "连接 d-token" : "Connect d-token"}</h2>
              <StatusBadge status={phasePresentation.health} label={phasePresentation.label} />
              <DataSourceBadge
                mode={snapshot ? "native" : discoveryStatus === "unavailable" ? "unavailable" : "preview"}
                label={snapshot ? (lang === "zh" ? "本机只读发现" : "Local read-only discovery") : undefined}
              />
            </div>
            <p className="mt-2 max-w-3xl text-sm leading-6 text-muted-foreground">
              {phasePresentation.description}
            </p>
            {snapshot && (
              <p className="mt-1 text-xs text-muted-foreground">
                {lang === "zh"
                  ? `已自动读取 ${discoveryCounts.agents} 个已安装工具、${discoveryCounts.upstreams} 个可用上游、${discoveryCounts.ccSwitchProfiles} 个 cc-switch profile。`
                  : `Automatically read ${discoveryCounts.agents} installed tools, ${discoveryCounts.upstreams} available upstreams, and ${discoveryCounts.ccSwitchProfiles} cc-switch profiles.`}
              </p>
            )}
          </div>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="icon" aria-label={lang === "zh" ? "刷新连接状态" : "Refresh connection state"} onClick={() => void refreshAll()} disabled={busy}>
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
                <div className="font-medium">{lang === "zh" ? "d-token 本地服务" : "d-token local service"}</div>
                <div className="mt-0.5 truncate text-xs text-muted-foreground">
                  {serviceInstanceDetail}
                </div>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <StatusBadge
                status={servicePresentation.status}
                label={restartRequired
                  ? (lang === "zh" ? "需要重启" : "Restart required")
                  : serviceOnline ? (lang === "zh" ? "运行中" : "Running") : (lang === "zh" ? "未运行" : "Stopped")}
              />
              {serviceManaged && (
                <Button variant="ghost" size="sm" onClick={() => { setStopError(""); setConfirmServiceStop(true); }} disabled={busy}>
                  {lang === "zh" ? "停止" : "Stop"}
                </Button>
              )}
            </div>
          </div>

          <div className="py-4">
            <div className="mb-3 flex items-center justify-between gap-3">
              <div>
                <div className="font-medium">{lang === "zh" ? "本机 AI 编码工具" : "Local AI coding Agents"}</div>
                <div className="mt-0.5 text-xs text-muted-foreground">
                  {lang === "zh" ? "每个 Agent 独立保留自己的 provider、模型与协议。" : "Each Agent keeps its own provider, model, and protocol."}
                </div>
              </div>
              <Badge variant="secondary">{visibleBindings.length} {lang === "zh" ? "个已检测" : "detected"}</Badge>
            </div>
            {discoveryStatus === "discovering" && !snapshot ? (
              <div className="space-y-2 py-2" aria-label={lang === "zh" ? "正在发现 Agent" : "Discovering Agents"}>
                {[0, 1, 2].map((item) => <div key={item} className="h-14 animate-pulse rounded-lg bg-muted/50" />)}
              </div>
            ) : (
              <div>
                <div className="divide-y divide-border/50">
                {primaryBindings.map((binding) => (
                  <AgentBindingRow
                    key={binding.agent_id}
                    binding={binding}
                    selection={machine.selections[binding.agent_id]}
                    included={machine.selectedAgentIds.includes(binding.agent_id)}
                    selectable={binding.capability === "one-click"}
                    lang={lang}
                    busy={busy}
                    onSelect={(candidateKey, model) => dispatch({ type: "SELECT_UPSTREAM", agentId: binding.agent_id, selection: { candidateKey, model } })}
                    onToggle={(selected) => dispatch({ type: "TOGGLE_AGENT", agentId: binding.agent_id, selected })}
                  />
                ))}
                {primaryBindings.length === 0 && visibleBindings.length > 0 && (
                  <div className="py-4 text-sm text-muted-foreground">
                    {lang === "zh" ? "检测到的工具当前需要手动确认，尚无可直接加入本批次的 Agent。" : "Detected tools currently require manual confirmation; none can join this batch automatically."}
                  </div>
                )}
                </div>
                {secondaryBindings.length > 0 && (
                  <details className="group border-t border-border/60 py-1">
                    <summary className="flex cursor-pointer list-none items-center justify-between gap-3 py-3 text-sm font-medium">
                      <span>{lang === "zh" ? `另外检测到 ${secondaryBindings.length} 个工具` : `${secondaryBindings.length} other detected tools`}</span>
                      <span className="flex items-center gap-1 text-xs font-normal text-muted-foreground">
                        {lang === "zh" ? "按需展开" : "Expand when needed"}<ChevronDown className="size-3.5 transition-transform group-open:rotate-180" />
                      </span>
                    </summary>
                    <div className="divide-y divide-border/50 border-t border-border/50">
                      {secondaryBindings.map((binding) => (
                        <AgentBindingRow
                          key={binding.agent_id}
                          binding={binding}
                          selection={machine.selections[binding.agent_id]}
                          included={false}
                          selectable={false}
                          lang={lang}
                          busy={busy}
                          onSelect={(candidateKey, model) => dispatch({ type: "SELECT_UPSTREAM", agentId: binding.agent_id, selection: { candidateKey, model } })}
                          onToggle={() => undefined}
                        />
                      ))}
                    </div>
                  </details>
                )}
                {visibleBindings.length === 0 && (
                  <div className="py-5 text-sm text-muted-foreground">
                    {lang === "zh" ? "本机尚未检测到可接入的 AI 编码工具。" : "No connectable AI coding Agent was detected locally."}
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
                  ? [snapshot.cc_switch_sync.profile, snapshot.cc_switch_sync.upstream, snapshot.cc_switch_sync.connection_path].filter(Boolean).join(" · ") || (lang === "zh" ? "已只读检测" : "Detected read-only")
                  : (lang === "zh" ? "未发现 cc-switch；不会自动安装或改写第三方配置。" : "cc-switch was not found; third-party configuration is never installed or rewritten automatically.")}
              </div>
            </div>
            <StatusBadge
              status={ccSwitchPresentation.health}
              label={ccSwitchPresentation.label}
            />
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
            {lang === "zh" ? "技术证据与操作记录" : "Technical evidence and action log"}
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
        open={showReview}
        onOpenChange={(open) => {
          setShowReview(open);
          if (!open) {
            setConnectionReview(null);
            setRoutePlanReview(null);
            setAgentIntegrationReviews({});
          }
        }}
        title={lang === "zh" ? "确认连接方案" : "Confirm connection plan"}
        summary={lang === "zh" ? "d-token 只会应用下方已确认的变更。源配置变化后，本方案会自动失效。" : "d-token applies only the confirmed changes below. Any source-configuration change invalidates this plan."}
        impact={actionableBindings.map((binding) => connectionPath(binding, machine.selections[binding.agent_id], serviceEndpoint)).join("\n")}
        details={(
          <div className="max-h-[46vh] space-y-3 overflow-y-auto pr-1 text-sm">
            {pendingAction === "preview" && <div className="flex items-center gap-2 text-muted-foreground"><RefreshCw className="size-4 animate-spin" />{lang === "zh" ? "正在生成 hash 绑定的脱敏 diff…" : "Building a hash-bound redacted diff…"}</div>}
            {routePlanReview && (
              <div className="rounded-lg border border-border bg-muted/30 p-3">
                <div className="font-medium">{lang === "zh" ? "d-token 自动路由" : "Automatic d-token routes"}</div>
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
            {Object.entries(agentIntegrationReviews).map(([agentId, review]) => {
              const binding = actionableBindings.find((item) => item.agent_id === agentId);
              return (
                <div key={agentId} className="rounded-lg border border-border bg-muted/30 p-3">
                  <div className="font-medium">{binding?.agent_name ?? agentId} · {lang === "zh" ? "Agent 接入" : "Agent integration"}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {review.ok
                      ? (lang === "zh" ? "指令、Hook/插件与 Skill 将按下方哈希绑定方案写入。" : "Instructions, hook/plugin, and skill changes are bound to the hashes shown below.")
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
                  {lang === "zh" ? "当前适配器只提供已解析的安全连接步骤；不会冒充已自动写入。" : "This adapter currently provides a resolved safe setup step and never pretends the config was auto-written."}
                </div>
              </div>
            ))}
          </div>
        )}
        confirmLabel={lang === "zh" ? "确认并连接" : "Confirm and connect"}
        cancelLabel={t("common.cancel")}
        pending={pendingAction === "apply" || pendingAction === "preview"}
        confirmDisabled={!routePlanReview?.ok
          || Boolean(openCodeSelection && !connectionReview?.ok)
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
