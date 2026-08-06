import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  Bot,
  Boxes,
  DatabaseZap,
  Download,
  ExternalLink,
  FolderOpen,
  Loader2,
  MonitorUp,
  PackageCheck,
  RefreshCw,
  RotateCcw,
  Search,
  Square,
  Trash2,
} from "lucide-react";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { EmptyState } from "../common/EmptyState";
import { OperationDialog } from "../common/OperationDialog";
import { FlatRow, FlatRows, Surface } from "../common/Surface";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { Checkbox } from "../ui/checkbox";
import { Input } from "../ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { cn } from "../ui/utils";
import { translateCopy, useI18n, useT } from "../../lib/i18n";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";
import {
  applyCodebaseMemoryIntegration,
  cancelCodebaseMemoryIndex,
  deleteCodebaseMemoryIndex,
  detectCodebaseMemoryAgents,
  getCodebaseMemoryStatus,
  integrationDigest,
  integrationDraft,
  openCodebaseMemoryGraph,
  openCodebaseMemoryDownloadPage,
  openCodebaseMemoryPackageDirectory,
  previewCodebaseMemoryIntegration,
  rollbackCodebaseMemoryIntegration,
  setCodebaseMemoryPackageDirectory,
  startCodebaseMemoryIndex,
  startCodebaseMemoryUi,
  stopCodebaseMemoryUi,
  verifyCodebaseMemoryIntegration,
  type CodebaseMemoryDimension,
  type CodebaseMemoryDetectedAgent,
  type CodebaseMemoryIntegrationPreview,
  type CodebaseMemoryIntegrationReceipt,
  type CodebaseMemoryIntegrationDraft,
  type CodebaseMemoryStatus,
} from "../../lib/codebaseMemory";

type BusyAction = "refresh" | "agents" | "package" | "preview" | "apply" | "rollback" | "ui" | "index" | "delete" | "open" | "";

type PreparedIntegration = {
  agentId: CodebaseMemoryIntegrationDraft["agent_id"];
  preview: CodebaseMemoryIntegrationPreview;
};

type AppliedIntegration = {
  agentId: CodebaseMemoryIntegrationDraft["agent_id"];
  receipt: CodebaseMemoryIntegrationReceipt;
};

type OperationProgress = {
  state: "working" | "review" | "success" | "error";
  message: string;
  current?: number;
  total?: number;
};

function yieldToPaint(): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, 0));
}

function OperationProgressStatus({
  progress,
  title,
  compact = false,
}: {
  progress: OperationProgress;
  title: string;
  compact?: boolean;
}) {
  const total = progress.total ?? 0;
  const current = progress.current ?? 0;
  const percent = total > 0 ? Math.min(100, Math.round((current / total) * 100)) : 0;
  return (
    <div className={cn("flex gap-3", compact ? "p-3" : "px-4 py-3")} role="status" aria-live="polite" aria-atomic="true">
      {progress.state === "working"
        ? <Loader2 className="mt-0.5 size-4 shrink-0 animate-spin text-primary" aria-hidden />
        : <PackageCheck className={cn("mt-0.5 size-4 shrink-0", progress.state === "error" ? "text-destructive" : "text-primary")} aria-hidden />}
      <div className="min-w-0 flex-1">
        <p className="text-xs font-semibold">{title}</p>
        <p className={cn("mt-0.5 text-xs", progress.state === "error" ? "text-destructive" : "text-muted-foreground")}>{progress.message}</p>
        {total > 0 ? (
          <div
            className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={total}
            aria-valuenow={current}
          >
            <div className="h-full rounded-full bg-primary transition-[width] duration-200" style={{ width: `${percent}%` }} />
          </div>
        ) : null}
      </div>
    </div>
  );
}

const UNAVAILABLE_DIMENSION: CodebaseMemoryDimension = {
  state: "unavailable",
  reason_code: "status_unavailable",
  detail: "",
  checked_at_ms: 0,
};

export function CodebaseMemory() {
  const { lang = "zh" } = useI18n();
  const t = useT();
  const { addNotification } = useNotifications();
  const mountedRef = useRef(true);
  const refreshSequence = useRef(0);
  const refreshRef = useRef<(manual?: boolean) => Promise<void>>(async () => undefined);
  const [status, setStatus] = useState<CodebaseMemoryStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<BusyAction>("");
  const [error, setError] = useState("");
  const [stale, setStale] = useState(false);
  const [previews, setPreviews] = useState<PreparedIntegration[]>([]);
  const [receipts, setReceipts] = useState<AppliedIntegration[]>([]);
  const [integrationDialogOpen, setIntegrationDialogOpen] = useState(false);
  const [rollbackDialogOpen, setRollbackDialogOpen] = useState(false);
  const [stopDialogOpen, setStopDialogOpen] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [manualProjectRoot, setManualProjectRoot] = useState("");
  const [packageDirectoryDraft, setPackageDirectoryDraft] = useState("");
  const [agents, setAgents] = useState<CodebaseMemoryDetectedAgent[]>([]);
  const [selectedAgentIds, setSelectedAgentIds] = useState<CodebaseMemoryIntegrationDraft["agent_id"][]>([]);
  const [scope, setScope] = useState<CodebaseMemoryIntegrationDraft["scope"]>("project");
  const [surface, setSurface] = useState<CodebaseMemoryIntegrationDraft["surface"]>("mcp");
  const [operationProgress, setOperationProgress] = useState<OperationProgress | null>(null);

  const refresh = useCallback(async (manual = false) => {
    const sequence = ++refreshSequence.current;
    if (manual) {
      setBusy("refresh");
      await yieldToPaint();
    }
    try {
      const next = await getCodebaseMemoryStatus();
      if (!mountedRef.current || sequence !== refreshSequence.current) return;
      setStatus(next);
      if (next.active_integrations) {
        const persisted = next.active_integrations
          .filter((item) => item.transaction_id && ["opencode", "codex", "claude-code"].includes(item.agent_id))
          .map((item) => ({
            agentId: item.agent_id as CodebaseMemoryIntegrationDraft["agent_id"],
            receipt: {
              ok: true,
              state: item.state,
              transaction_id: item.transaction_id,
              rollback_id: item.rollback_id,
              verified: item.state === "verified",
            },
          }));
        setReceipts(persisted);
      }
      setPackageDirectoryDraft((current) => current || next.package_directory || "");
      setError("");
      setStale(false);
    } catch (reason) {
      if (!mountedRef.current || sequence !== refreshSequence.current) return;
      const failure = presentOperationFailure("codebaseMemory", reason, lang);
      const browserPreview = reason instanceof Error && reason.message === "native_unavailable";
      setError(browserPreview ? t("cbm.browserPreview") : failure.message);
      setStale(Boolean(status));
      if (manual) {
        addNotification({
          dedupeKey: "codebase-memory-refresh-failed",
          level: status ? "warning" : "error",
          title: t("cbm.refreshFailed"),
          message: failure.message,
          details: failure.details,
        });
      }
    } finally {
      if (mountedRef.current && sequence === refreshSequence.current) {
        setLoading(false);
        if (manual) setBusy("");
      }
    }
  }, [addNotification, lang, status, t]);
  refreshRef.current = refresh;

  const scanAgents = useCallback(async (selectAll = false) => {
    if (selectAll) {
      setBusy("agents");
      await yieldToPaint();
    }
    try {
      const detected = await detectCodebaseMemoryAgents();
      if (!mountedRef.current) return;
      setAgents(detected);
      const supported = detected
        .filter((agent) => agent.installed && agent.integration_supported)
        .map((agent) => agent.id as CodebaseMemoryIntegrationDraft["agent_id"]);
      setSelectedAgentIds((current) => selectAll
        ? supported
        : current.filter((id) => supported.includes(id)));
    } catch (reason) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("codebaseMemory", reason, lang);
      setAgents([]);
      setSelectedAgentIds([]);
      if (selectAll) {
        addNotification({
          dedupeKey: "codebase-memory-agent-scan-failed",
          level: "error",
          title: t("cbm.scanAgentsFailed"),
          message: failure.message,
          details: failure.details,
        });
      }
    } finally {
      if (mountedRef.current && selectAll) setBusy("");
    }
  }, [addNotification, t]);

  useEffect(() => {
    mountedRef.current = true;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) => listen("dtoken:codebase-memory-status", () => {
        void refreshRef.current();
      }))
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => undefined);
    void refresh();
    void scanAgents(true);
    return () => {
      disposed = true;
      mountedRef.current = false;
      unlisten?.();
    };
  }, []); // App-level invalidation owns fallback refresh; this page performs one activation read only.

  const approvedProjectRoot = status?.approved_project_root?.trim() ?? "";
  const projectRoot = manualProjectRoot.trim() || approvedProjectRoot;
  const packageReady = status?.package.state === "ready";
  const uiOwned = status?.ui_session.owner === "dtoken";
  const uiReady = status?.ui_session.state === "ready" && uiOwned;
  const indexBusy = status?.index.state === "busy" || status?.index.state === "starting";
  const actionPending = busy !== "";
  const visibleProgress = actionPending && !["preview", "apply", "rollback"].includes(busy)
    ? { state: "working", message: t("cbm.progressGeneric") } as OperationProgress
    : operationProgress;
  const selectedAgents = selectedAgentIds.filter((id) => agents.some((agent) => agent.id === id && agent.installed && agent.integration_supported));
  const unsupportedAgentIds = selectedAgents.filter((id) =>
    surface === "hook" ? id !== "claude-code" : id === "claude-code" && scope === "global");
  const unsupportedSelection = unsupportedAgentIds.length > 0;
  const integrationBlockedReason = !packageReady
    ? status?.package.state === "untrusted"
      ? t("cbm.packageUntrusted")
      : t("cbm.packageRequired")
    : !projectRoot
      ? t("cbm.projectRequired")
      : selectedAgents.length === 0
        ? t("cbm.noAgentSelected")
        : unsupportedSelection
          ? t("cbm.unsupportedSelection")
          : "";

  const notifyResult = (
    key: string,
    ok: boolean,
    title: string,
    message: string,
    transactionId?: string,
    rollbackId?: string,
  ) => {
    addNotification({
      dedupeKey: key,
      level: ok ? "success" : "error",
      title,
      message,
      persistent: !ok || Boolean(transactionId),
      receipt: transactionId
        ? { requestId: createOperationRequestId(key), transactionId, rollbackId, targetId: "codebase-memory" }
        : undefined,
    });
  };

  const previewIntegration = async () => {
    if (integrationBlockedReason) {
      setError(integrationBlockedReason);
      addNotification({
        dedupeKey: "codebase-memory-project-required",
        level: "action-required",
        title: t("cbm.integrationBlocked"),
        message: integrationBlockedReason,
      });
      return;
    }
    setBusy("preview");
    setError("");
    setOperationProgress({ state: "working", message: t("cbm.progressGeneric"), current: 0, total: selectedAgents.length });
    await yieldToPaint();
    try {
      const next: PreparedIntegration[] = [];
      for (const [index, agentId] of selectedAgents.entries()) {
        const name = agentName(agentId, agents);
        setOperationProgress({
          state: "working",
          current: index + 1,
          total: selectedAgents.length,
          message: translateCopy("cbm.progressPreparePreview", lang, { agent: name, current: index + 1, total: selectedAgents.length }),
        });
        await yieldToPaint();
        next.push({
          agentId,
          preview: await previewCodebaseMemoryIntegration(projectRoot, { agent_id: agentId, scope, surface }),
        });
      }
      if (!mountedRef.current) return;
      const invalid = next.find((item) => !item.preview.ok || !integrationDigest(item.preview));
      if (invalid) {
        const message = `${agentName(invalid.agentId, agents)}: ${t("cbm.previewInvalid")}`;
        setError(message);
        setOperationProgress({ state: "error", message });
        notifyResult("codebase-memory-preview-failed", false, t("cbm.previewFailed"), message);
        return;
      }
      setPreviews(next);
      setOperationProgress({ state: "review", message: t("cbm.progressReview"), current: selectedAgents.length, total: selectedAgents.length });
      setIntegrationDialogOpen(true);
    } catch (reason) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("codebaseMemory", reason, lang);
      setError(failure.message);
      setOperationProgress({ state: "error", message: failure.message });
      notifyResult("codebase-memory-preview-error", false, failure.title, failure.message);
    } finally {
      if (mountedRef.current) setBusy("");
    }
  };

  const rollbackApplied = async (applied: AppliedIntegration[]) => {
    const failed: AppliedIntegration[] = [];
    const reversed = [...applied].reverse();
    for (const [index, item] of reversed.entries()) {
      const transactionId = item.receipt.transaction_id;
      if (!transactionId) continue;
      const name = agentName(item.agentId, agents);
      setOperationProgress({
        state: "working",
        current: index + 1,
        total: reversed.length,
        message: translateCopy("cbm.progressRollback", lang, { agent: name, current: index + 1, total: reversed.length }),
      });
      await yieldToPaint();
      try {
        const rolledBack = await rollbackCodebaseMemoryIntegration(transactionId);
        if (!rolledBack.ok) failed.push(item);
      } catch {
        failed.push(item);
      }
    }
    return failed.reverse();
  };

  const applyAndVerifyIntegration = async () => {
    if (!previews.length || !projectRoot) return;
    setBusy("apply");
    const appliedBatch: AppliedIntegration[] = [];
    try {
      for (const [index, item] of previews.entries()) {
        const digest = integrationDigest(item.preview);
        const selection = { agent_id: item.agentId, scope, surface } as const;
        const name = agentName(item.agentId, agents);
        setOperationProgress({
          state: "working",
          current: index + 1,
          total: previews.length,
          message: translateCopy("cbm.progressApply", lang, { agent: name, current: index + 1, total: previews.length }),
        });
        await yieldToPaint();
        const applied = await applyCodebaseMemoryIntegration(integrationDraft(item.preview, projectRoot, selection), digest);
        if (!applied.ok || !applied.transaction_id) {
          if (applied.transaction_id) {
            // A post-write persistence failure may return a recovery handle.
            // Keep it in the batch so the compensating rollback path retries
            // instead of orphaning a successfully written Agent config.
            appliedBatch.push({ agentId: item.agentId, receipt: applied });
          }
          throw new Error(`${agentName(item.agentId, agents)}: ${applied.error || applied.summary || t("cbm.applyFailed")}`);
        }
        appliedBatch.push({ agentId: item.agentId, receipt: applied });
        setOperationProgress({
          state: "working",
          current: index + 1,
          total: previews.length,
          message: translateCopy("cbm.progressVerify", lang, { agent: name, current: index + 1, total: previews.length }),
        });
        await yieldToPaint();
        const verified = await verifyCodebaseMemoryIntegration(applied.transaction_id);
        const verifiedOk = verified.ok && (verified.verified === true || verified.state === "verified");
        const combined = { ...applied, ...verified, transaction_id: applied.transaction_id };
        appliedBatch[appliedBatch.length - 1] = { agentId: item.agentId, receipt: combined };
        if (!verifiedOk) {
          throw new Error(`${agentName(item.agentId, agents)}: ${verified.error || verified.summary || t("cbm.verifyFailed")}`);
        }
      }
      if (!mountedRef.current) return;
      setReceipts(appliedBatch);
      setIntegrationDialogOpen(false);
      setPreviews([]);
      setError("");
      setOperationProgress({ state: "success", message: t("cbm.progressComplete"), current: appliedBatch.length, total: appliedBatch.length });
      const last = appliedBatch[appliedBatch.length - 1]?.receipt;
      notifyResult(
        `codebase-memory-verified-${last?.transaction_id ?? "batch"}`,
        true,
        t("cbm.integrationVerified"),
        translateCopy("cbm.integrationVerifiedMessage", lang, { count: appliedBatch.length }),
        last?.transaction_id,
        last?.rollback_id,
      );
      await refresh();
    } catch (reason) {
      const failedRollbacks = await rollbackApplied(appliedBatch);
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("codebaseMemory", reason, lang);
      const finalMessage = failedRollbacks.length
        ? `${failure.message} · ${translateCopy("cbm.rollbackPartial", lang, { count: failedRollbacks.length })}`
        : `${failure.message} · ${translateCopy("cbm.rollbackBatchRestored", lang)}`;
      setReceipts(failedRollbacks);
      setError(finalMessage);
      setOperationProgress({ state: "error", message: finalMessage });
      notifyResult("codebase-memory-apply-error", false, t("cbm.applyFailed"), finalMessage);
    } finally {
      if (mountedRef.current) setBusy("");
    }
  };

  const rollbackIntegration = async () => {
    if (!receipts.length) return;
    setBusy("rollback");
    try {
      const failed = await rollbackApplied(receipts);
      if (!mountedRef.current) return;
      const ok = failed.length === 0;
      const message = ok ? t("cbm.rollbackDone") : `${t("cbm.rollbackFailed")} (${failed.length})`;
      const last = receipts[receipts.length - 1]?.receipt;
      notifyResult(`codebase-memory-rollback-${last?.transaction_id ?? "batch"}`, ok, ok ? t("cbm.rollbackDone") : t("cbm.rollbackFailed"), message, last?.transaction_id, last?.rollback_id);
      setReceipts(failed);
      if (ok) {
        setOperationProgress({ state: "success", message: t("cbm.rollbackDone"), current: receipts.length, total: receipts.length });
        setRollbackDialogOpen(false);
        await refresh();
      } else {
        setError(message);
        setOperationProgress({ state: "error", message });
      }
    } catch (reason) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("codebaseMemory", reason, lang);
      setError(failure.message);
      setOperationProgress({ state: "error", message: failure.message });
      notifyResult("codebase-memory-rollback-error", false, failure.title, failure.message);
    } finally {
      if (mountedRef.current) setBusy("");
    }
  };

  const runSimpleAction = async (
    action: Exclude<BusyAction, "refresh" | "agents" | "preview" | "apply" | "rollback" | "">,
    operation: () => Promise<{ ok: boolean; error?: string | null; summary?: string }>,
    successTitle: string,
    failureTitle: string,
    refreshAfter = true,
  ) => {
    setBusy(action);
    setError("");
    await yieldToPaint();
    try {
      const result = await operation();
      if (!mountedRef.current) return;
      const failure = result.ok ? null : presentOperationFailure("codebaseMemory", result.error || result.summary, lang);
      const message = result.ok ? successTitle : failure!.message;
      notifyResult(`codebase-memory-${action}-${result.ok ? "ok" : "failed"}`, result.ok, result.ok ? successTitle : failure!.title, message);
      if (!result.ok) setError(message);
      if (refreshAfter) await refresh();
    } catch (reason) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("codebaseMemory", reason, lang);
      setError(failure.message);
      notifyResult(`codebase-memory-${action}-error`, false, failure.title, failure.message);
    } finally {
      if (mountedRef.current) setBusy("");
    }
  };

  const savePackageDirectory = async (restoreDefault = false) => {
    setBusy("package");
    setError("");
    await yieldToPaint();
    try {
      const result = await setCodebaseMemoryPackageDirectory(restoreDefault ? undefined : packageDirectoryDraft);
      if (!mountedRef.current) return;
      if (!result.ok) throw new Error(result.error || result.summary || t("cbm.packageSaveFailed"));
      notifyResult("codebase-memory-package-directory", true, t("cbm.packageSaved"), t("cbm.packageSaved"));
      setPackageDirectoryDraft(restoreDefault ? "" : packageDirectoryDraft.trim());
      await refresh();
    } catch (reason) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("codebaseMemory", reason, lang);
      setError(failure.message);
      notifyResult("codebase-memory-package-directory-failed", false, failure.title, failure.message);
    } finally {
      if (mountedRef.current) setBusy("");
    }
  };

  const toggleAgent = (id: string, checked: boolean | "indeterminate") => {
    if (!isCodebaseMemoryAgentId(id)) return;
    setSelectedAgentIds((current) => checked === true
      ? current.includes(id) ? current : [...current, id]
      : current.filter((item) => item !== id));
    setPreviews([]);
    setOperationProgress(null);
  };

  const dimensions = [
    { id: "package", label: t("cbm.dimension.package"), icon: PackageCheck, value: status?.package ?? UNAVAILABLE_DIMENSION },
    { id: "integration", label: t("cbm.dimension.integration"), icon: Boxes, value: status?.integration ?? UNAVAILABLE_DIMENSION },
    { id: "agent", label: t("cbm.dimension.agentSession"), icon: Bot, value: status?.agent_session ?? UNAVAILABLE_DIMENSION },
    { id: "ui", label: t("cbm.dimension.uiSession"), icon: MonitorUp, value: status?.ui_session ?? UNAVAILABLE_DIMENSION },
    { id: "index", label: t("cbm.dimension.index"), icon: DatabaseZap, value: status?.index ?? UNAVAILABLE_DIMENSION },
  ] as const;

  const dataMode = status ? "native" : error === t("cbm.browserPreview") ? "preview" : "unavailable";

  return (
    <div className="space-y-4" data-flat-page data-page="codebase-memory">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <h1>{t("cbm.title")}</h1>
            <DataSourceBadge mode={dataMode} />
          </div>
          <p className="max-w-3xl text-sm text-muted-foreground">{t("cbm.subtitle")}</p>
          <p className="max-w-3xl text-xs text-muted-foreground">{t("cbm.privacy")}</p>
          {stale ? <p className="text-xs text-amber-700 dark:text-amber-300" role="status">{t("cbm.staleEvidence")}</p> : null}
          {error ? <p className="text-xs text-destructive" role="status">{error}</p> : null}
        </div>
        <Button type="button" variant="outline" size="sm" disabled={actionPending || loading} onClick={() => void refresh(true)}>
          <RefreshCw className={cn("size-3.5", busy === "refresh" && "animate-spin")} aria-hidden />
          {t("cbm.refresh")}
        </Button>
      </div>

      {visibleProgress ? (
        <Surface strong className="overflow-hidden border-primary/25" aria-label={t("cbm.progressTitle")}>
          <OperationProgressStatus progress={visibleProgress} title={t("cbm.progressTitle")} />
        </Surface>
      ) : null}

      {loading && !status ? (
        <EmptyState status="loading" title={t("cbm.loading")} description={t("cbm.loadingDetail")} />
      ) : null}

      <Surface strong className="overflow-hidden" aria-label={t("cbm.installTitle")}>
        <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border/60 px-4 py-3">
          <div className="max-w-3xl">
            <h2 className="text-base font-semibold">{t("cbm.installTitle")}</h2>
            <p className="mt-1 text-xs leading-relaxed text-muted-foreground">{t("cbm.installDetail")}</p>
          </div>
          <Badge variant={packageReady ? "secondary" : "outline"}>{packageReady ? t("cbm.state.ready") : t("cbm.state.unavailable")}</Badge>
        </div>
        <div className="space-y-3 px-4 py-4">
          <label className="grid gap-1.5 text-xs font-medium" htmlFor="codebase-memory-package-directory">
            {t("cbm.packageDirectory")}
            <Input
              id="codebase-memory-package-directory"
              value={packageDirectoryDraft}
              disabled={actionPending}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => setPackageDirectoryDraft(event.target.value)}
            />
          </label>
          <p className="text-xs text-muted-foreground">{t("cbm.packageDirectoryHint")}</p>
          <div className="flex flex-wrap gap-2">
            <Button type="button" size="sm" onClick={() => void runSimpleAction("package", openCodebaseMemoryDownloadPage, t("cbm.download"), t("cbm.packageSaveFailed"), false)} disabled={actionPending}>
              <Download aria-hidden />{t("cbm.download")}
            </Button>
            <Button type="button" size="sm" variant="outline" onClick={() => void runSimpleAction("package", openCodebaseMemoryPackageDirectory, t("cbm.openDirectory"), t("cbm.packageSaveFailed"), false)} disabled={actionPending}>
              <FolderOpen aria-hidden />{t("cbm.openDirectory")}
            </Button>
            <Button type="button" size="sm" variant="outline" onClick={() => void savePackageDirectory(false)} disabled={actionPending || !packageDirectoryDraft.trim()}>
              {busy === "package" ? <Loader2 className="animate-spin" aria-hidden /> : null}{t("cbm.saveDirectory")}
            </Button>
            {status?.package_directory_custom ? (
              <Button type="button" size="sm" variant="ghost" onClick={() => void savePackageDirectory(true)} disabled={actionPending}>{t("cbm.restoreDefault")}</Button>
            ) : null}
          </div>
        </div>
      </Surface>

      <Surface strong className="overflow-hidden" aria-label={t("cbm.statusTitle")}>
        <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border/60 px-4 py-3">
          <div>
            <h2 className="text-base font-semibold">{t("cbm.statusTitle")}</h2>
            <p className="mt-1 text-xs text-muted-foreground">{t("cbm.statusDetail")}</p>
          </div>
          {approvedProjectRoot
            ? <Badge variant="secondary">{t("cbm.projectApproved")}</Badge>
            : projectRoot
              ? <Badge variant="outline">{t("cbm.projectSelected")}</Badge>
              : <Badge variant="outline">{t("cbm.projectMissing")}</Badge>}
        </div>
        <div className="border-b border-border/60 px-4 py-3">
          <label className="grid gap-1.5 text-xs font-medium" htmlFor="codebase-memory-project-root">
            {t("cbm.projectRootLabel")}
            <Input
              id="codebase-memory-project-root"
              value={manualProjectRoot || approvedProjectRoot}
              placeholder={t("cbm.projectRootPlaceholder")}
              disabled={actionPending}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => setManualProjectRoot(event.target.value)}
            />
          </label>
          <p className="mt-1.5 text-xs text-muted-foreground">{t("cbm.projectRootHint")}</p>
          <div className="mt-3 rounded-xl border border-border/70 p-3">
            <div className="flex items-center justify-between gap-3">
              <span className="text-xs font-medium">{t("cbm.agentLabel")}</span>
              <Button type="button" size="sm" variant="ghost" disabled={actionPending} onClick={() => void scanAgents(true)}>
                <Search className={cn("size-3.5", busy === "agents" && "animate-pulse")} aria-hidden />{t("cbm.scanAgents")}
              </Button>
            </div>
            {agents.length ? (
              <div className="mt-2 grid gap-2 sm:grid-cols-3">
                {agents.map((agent) => {
                  const supported = Boolean(agent.installed && agent.integration_supported);
                  return (
                    <label key={agent.id} className="flex items-center justify-between gap-3 rounded-lg border border-border/60 px-3 py-2 text-sm">
                      <span className="min-w-0">
                        <span className="block truncate font-medium">{agent.name}</span>
                        <span className="block text-[11px] text-muted-foreground">
                          {!agent.installed ? t("cbm.agentNotDetected") : agent.integration_supported === false ? t("cbm.agentUnsupported") : t("cbm.agentDetected")}
                        </span>
                      </span>
                      <Checkbox
                        checked={selectedAgentIds.includes(agent.id as CodebaseMemoryIntegrationDraft["agent_id"])}
                        disabled={!supported || actionPending}
                        onCheckedChange={(checked) => toggleAgent(agent.id, checked)}
                        aria-label={`${agent.name} ${t("cbm.agentLabel")}`}
                      />
                    </label>
                  );
                })}
              </div>
            ) : <p className="mt-2 text-xs text-muted-foreground">{t("cbm.noAgents")}</p>}
          </div>
          <div className="mt-3 grid gap-3 sm:grid-cols-2">
            <label className="grid gap-1.5 text-xs font-medium">
              {t("cbm.scopeLabel")}
              <Select value={scope} disabled={actionPending} onValueChange={(value) => setScope(value as CodebaseMemoryIntegrationDraft["scope"])}>
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="project">{t("cbm.scopeProject")}</SelectItem>
                  <SelectItem value="global">{t("cbm.scopeGlobal")}</SelectItem>
                </SelectContent>
              </Select>
            </label>
            <label className="grid gap-1.5 text-xs font-medium">
              {t("cbm.surfaceLabel")}
              <Select value={surface} disabled={actionPending} onValueChange={(value) => setSurface(value as CodebaseMemoryIntegrationDraft["surface"])}>
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="mcp">MCP</SelectItem>
                  <SelectItem value="hook">Hook · SessionStart</SelectItem>
                </SelectContent>
              </Select>
            </label>
          </div>
          {integrationBlockedReason ? <p className="mt-2 text-xs text-amber-700 dark:text-amber-300">{integrationBlockedReason}</p> : null}
        </div>
        <FlatRows>
          {dimensions.map(({ id, label, icon: Icon, value }) => (
            <DimensionRow
              key={id}
              label={label}
              icon={<Icon className="size-4" aria-hidden />}
              dimension={value}
              stateLabel={stateLabel(value.state, t)}
              action={
                id === "integration" ? (
                  <div className="flex flex-wrap gap-2">
                    <Button type="button" size="sm" disabled={actionPending || Boolean(integrationBlockedReason)} onClick={() => void previewIntegration()} title={integrationBlockedReason || undefined}>
                      {busy === "preview" ? <Loader2 className="animate-spin" aria-hidden /> : null}
                      {surface === "hook" ? t("cbm.configureHook") : t("cbm.configureMcp")}
                    </Button>
                    {receipts.length ? (
                      <Button type="button" size="sm" variant="outline" disabled={actionPending} onClick={() => setRollbackDialogOpen(true)}>
                        <RotateCcw aria-hidden />{t("cbm.rollback")}
                      </Button>
                    ) : null}
                  </div>
                ) : id === "ui" ? (
                  <div className="flex flex-wrap gap-2">
                    {uiReady ? (
                      <>
                        <Button type="button" size="sm" variant="outline" disabled={actionPending} onClick={() => void runSimpleAction("open", openCodebaseMemoryGraph, t("cbm.graphOpened"), t("cbm.graphOpenFailed"))}>
                          <ExternalLink aria-hidden />{t("cbm.openGraph")}
                        </Button>
                        <Button type="button" size="sm" variant="outline" disabled={actionPending || !uiOwned} onClick={() => setStopDialogOpen(true)}>
                          <Square aria-hidden />{t("cbm.stopUi")}
                        </Button>
                      </>
                    ) : (
                      <Button type="button" size="sm" disabled={actionPending || !packageReady || !projectRoot} onClick={() => void runSimpleAction("ui", async () => {
                        const started = await startCodebaseMemoryUi(projectRoot);
                        return started.ok ? openCodebaseMemoryGraph() : started;
                      }, t("cbm.graphOpened"), t("cbm.startOpenFailed"))}>
                        {busy === "ui" ? <Loader2 className="animate-spin" aria-hidden /> : null}{t("cbm.startUi")}
                      </Button>
                    )}
                  </div>
                ) : id === "index" ? (
                  indexBusy ? (
                    <Button type="button" size="sm" variant="outline" disabled={actionPending} onClick={() => void runSimpleAction("index", cancelCodebaseMemoryIndex, t("cbm.indexCancelled"), t("cbm.indexCancelFailed"))}>
                      <Square aria-hidden />{t("cbm.cancelIndex")}
                    </Button>
                  ) : (
                    <div className="flex flex-wrap gap-2">
                      <Button type="button" size="sm" disabled={actionPending || !packageReady || !projectRoot} onClick={() => void runSimpleAction("index", () => startCodebaseMemoryIndex(projectRoot), t("cbm.indexStarted"), t("cbm.indexStartFailed"))}>
                        {busy === "index" ? <Loader2 className="animate-spin" aria-hidden /> : null}{t("cbm.startIndex")}
                      </Button>
                      <Button type="button" size="sm" variant="outline" disabled={actionPending || !projectRoot || uiReady} onClick={() => setDeleteDialogOpen(true)}>
                        <Trash2 aria-hidden />{t("cbm.deleteIndex")}
                      </Button>
                    </div>
                  )
                ) : undefined
              }
            />
          ))}
        </FlatRows>
      </Surface>

      <OperationDialog
        open={integrationDialogOpen}
        onOpenChange={(open) => {
          setIntegrationDialogOpen(open);
          if (!open && busy !== "apply") setOperationProgress(null);
        }}
        title={t("cbm.confirmIntegrationTitle")}
        summary={t("cbm.confirmIntegrationSummary")}
        impact={t("cbm.confirmIntegrationImpact")}
        details={previews.length ? <div className="max-h-72 space-y-3 overflow-auto">{visibleProgress ? <div className="rounded-lg border border-primary/25"><OperationProgressStatus compact progress={visibleProgress} title={t("cbm.progressTitle")} /></div> : null}{previews.map((item) => <div key={item.agentId} className="rounded-lg border border-border bg-muted/20 p-3"><h3 className="mb-2 text-sm font-semibold">{agentName(item.agentId, agents)}</h3><pre className="text-xs whitespace-pre-wrap">{item.preview.diff || t("cbm.previewDetailUnavailable")}</pre></div>)}</div> : undefined}
        confirmLabel={t("cbm.confirmApplyVerify")}
        cancelLabel={t("common.cancel")}
        risk="warning"
        pending={busy === "apply"}
        confirmDisabled={!previews.length || previews.some((item) => !integrationDigest(item.preview))}
        onConfirm={applyAndVerifyIntegration}
      />
      <OperationDialog
        open={rollbackDialogOpen}
        onOpenChange={setRollbackDialogOpen}
        title={t("cbm.rollbackTitle")}
        summary={t("cbm.rollbackSummary")}
        impact={t("cbm.rollbackImpact")}
        confirmLabel={t("cbm.rollbackConfirm")}
        cancelLabel={t("common.cancel")}
        risk="danger"
        pending={busy === "rollback"}
        onConfirm={rollbackIntegration}
      />
      <OperationDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        title={t("cbm.deleteIndexTitle")}
        summary={t("cbm.deleteIndexSummary")}
        impact={t("cbm.deleteIndexImpact")}
        confirmLabel={t("cbm.deleteIndexConfirm")}
        cancelLabel={t("common.cancel")}
        risk="danger"
        pending={busy === "delete"}
        onConfirm={async () => {
          await runSimpleAction("delete", () => deleteCodebaseMemoryIndex(projectRoot), t("cbm.indexDeleted"), t("cbm.indexDeleteFailed"));
          if (mountedRef.current) setDeleteDialogOpen(false);
        }}
      />
      <OperationDialog
        open={stopDialogOpen}
        onOpenChange={setStopDialogOpen}
        title={t("cbm.stopUiTitle")}
        summary={t("cbm.stopUiSummary")}
        impact={t("cbm.stopUiImpact")}
        confirmLabel={t("cbm.stopUiConfirm")}
        cancelLabel={t("common.cancel")}
        risk="warning"
        pending={busy === "ui"}
        onConfirm={async () => {
          await runSimpleAction("ui", stopCodebaseMemoryUi, t("cbm.uiStopped"), t("cbm.uiStopFailed"));
          if (mountedRef.current) setStopDialogOpen(false);
        }}
      />
    </div>
  );
}

function DimensionRow({
  label,
  icon,
  dimension,
  stateLabel: labelForState,
  action,
}: {
  label: string;
  icon: ReactNode;
  dimension: CodebaseMemoryDimension;
  stateLabel: string;
  action?: ReactNode;
}) {
  const t = useT();
  const detail = localizedDimensionDetail(dimension, t);
  return (
    <FlatRow className="grid gap-3 px-4 py-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-muted-foreground">{icon}</span>
          <h3 className="text-sm font-semibold">{label}</h3>
          <DimensionBadge dimension={dimension} label={labelForState} />
          {dimension.owner === "agent" || dimension.owner === "dtoken" ? <Badge variant="outline">{ownerLabel(dimension.owner, t)}</Badge> : null}
        </div>
        <p className="mt-1 text-xs text-muted-foreground">{detail}</p>
        {dimension.warnings?.length ? <p className="mt-1 text-xs text-amber-700 dark:text-amber-300">{t("cbm.detail.warning")}</p> : null}
      </div>
      {action ? <div className="flex justify-start sm:justify-end">{action}</div> : null}
    </FlatRow>
  );
}

function localizedDimensionDetail(dimension: CodebaseMemoryDimension, t: (key: string) => string) {
  const key = `cbm.detail.${dimension.reason_code}`;
  const resolved = t(key);
  return resolved === key ? t("cbm.detail.generic") : resolved;
}

function ownerLabel(owner: NonNullable<CodebaseMemoryDimension["owner"]>, t: (key: string) => string) {
  if (owner === "agent") return t("cbm.owner.agent");
  if (owner === "dtoken") return t("cbm.owner.dtoken");
  return "";
}

function DimensionBadge({ dimension, label }: { dimension: CodebaseMemoryDimension; label: string }) {
  if (dimension.state === "ready") {
    return <Badge className="bg-emerald-600 text-white">{label}</Badge>;
  }
  if (dimension.state === "failed" || dimension.state === "untrusted") {
    return <Badge variant="destructive">{label}</Badge>;
  }
  if (dimension.state === "starting" || dimension.state === "busy" || dimension.state === "partial") {
    return <Badge className="bg-amber-500/15 text-amber-800 dark:text-amber-200">{label}</Badge>;
  }
  return <Badge variant="secondary">{label}</Badge>;
}

function stateLabel(state: CodebaseMemoryDimension["state"], t: (key: string) => string) {
  return t(`cbm.state.${state}`);
}

function isCodebaseMemoryAgentId(id: string): id is CodebaseMemoryIntegrationDraft["agent_id"] {
  return id === "opencode" || id === "codex" || id === "claude-code";
}

function agentName(id: CodebaseMemoryIntegrationDraft["agent_id"], agents: CodebaseMemoryDetectedAgent[]) {
  return agents.find((agent) => agent.id === id)?.name
    ?? ({ opencode: "OpenCode", codex: "Codex", "claude-code": "Claude Code" } as const)[id];
}
