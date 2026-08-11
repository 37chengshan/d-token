import { useEffect, useMemo, useState } from "react";
import { ArrowRight, FileClock, History, RefreshCw, RotateCcw, ShieldAlert } from "lucide-react";
import { Surface } from "../common/Surface";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { OperationDialog } from "../common/OperationDialog";
import { translateCopy, useI18n } from "../../lib/i18n";
import { invokeNative } from "../../lib/native";
import type { PageId } from "../../lib/nav";
import { classifyEvidence } from "../../lib/state/evidence";
import { useSessionTab } from "../../lib/sessionTab";
import { useRuntimeDashboardDetail } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";

const CHANGES_TABS = ["backup", "history"] as const;
// Rollback points and change history grow with use; render a bounded window.
const CHANGES_PAGE_SIZE = 40;

type ChangeSource = "loading" | "desktop" | "browser-preview" | "unavailable" | "stale" | "partial";

type RollbackScope = "opencode-config" | "dtoken-active-config" | "learning-managed-block" | "optical-config" | "agent-integration" | "unknown";
type RollbackExecutorKind = "generic" | "agent_integration" | "optical" | "locked";

type UiRollbackPoint = {
  id: string;
  time: string;
  label: string;
  files: number | null;
  target: string;
  scope: RollbackScope;
  executorKind: RollbackExecutorKind;
  transactionId: string;
  integrityVerified: boolean;
  currentlyRestorable: boolean;
  blockedReason: string;
  expectedCurrentSha256: string;
};

type RollbackPreview = {
  ok: boolean;
  mode: "rollback-preview";
  revision: string;
  rollback_id: string;
  scope: RollbackScope;
  executor_kind: RollbackExecutorKind;
  transaction_id?: string;
  target: string;
  files: number;
  integrity_verified: boolean;
  currently_restorable: boolean;
  blocked_reason: string;
  expected_current_sha256: string;
  summary: string;
};

type RollbackReceipt = {
  ok: boolean;
  mode: "rollback-receipt";
  rollback_id: string;
  summary: string;
  verification_state: "verified" | "runtime_offline" | "failed";
};

type ChangeHistoryEntry = {
  id: string;
  timestamp: string;
  kind: string;
  status: string;
  summary: string;
  target: string;
  rollbackId: string;
  diff: string;
};

export function Changes({ setPage }: { setPage?: (page: PageId) => void } = {}) {
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const [tab, setTab] = useSessionTab("d-token.changes.tab", CHANGES_TABS, "backup");
  const { payload, status, error, refresh } = useRuntimeDashboardDetail();
  const [notice, setNotice] = useState("");
  const [preview, setPreview] = useState<RollbackPreview | null>(null);
  const [previewingId, setPreviewingId] = useState("");
  const [executing, setExecuting] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshRequestId, setRefreshRequestId] = useState("");

  const source = useMemo(() => sourceFromPayload(payload, status), [payload, status]);
  const rollbackPoints = useMemo(() => buildRollbackPoints(payload, source, lang), [payload, source, lang]);
  const historyEntries = useMemo(() => buildChangeHistory(payload, lang), [lang, payload]);
  const [visibleRollbackCount, setVisibleRollbackCount] = useState(CHANGES_PAGE_SIZE);
  const [visibleHistoryCount, setVisibleHistoryCount] = useState(CHANGES_PAGE_SIZE);
  const visibleRollbackPoints = rollbackPoints.slice(0, visibleRollbackCount);
  const visibleHistoryEntries = historyEntries.slice(0, visibleHistoryCount);

  useEffect(() => {
    setVisibleRollbackCount(CHANGES_PAGE_SIZE);
    setVisibleHistoryCount(CHANGES_PAGE_SIZE);
  }, [payload?.desktop_revision]);
  const unfinishedDrafts = useMemo(() => countUnfinishedDrafts(payload), [payload]);

  useEffect(() => {
    if (refreshing || !refreshRequestId) return;
    const available = source === "desktop" || source === "partial";
    const stale = source === "stale";
    addNotification({
      dedupeKey: available ? "changes-refresh" : stale ? "changes-refresh-stale" : "changes-refresh-failed",
      level: available ? "success" : stale ? "warning" : "error",
      title: available ? translateCopy("ch.changeRecordsRefreshed", lang) : stale ? translateCopy("ch.changeRefreshIncomplete", lang) : translateCopy("ch.changeRefreshFailed", lang),
      message: available ? translateCopy("ch.rollbackPointsAndTransactionRecordsWereRefreshed", lang) : stale ? translateCopy("ch.theLastVerifiedRecordsRemainVisible", lang) : sourceBlockedReason(source, lang),
      receipt: { requestId: refreshRequestId, targetId: "changes" },
    });
    setRefreshRequestId("");
  }, [addNotification, error, lang, refreshRequestId, refreshing, source]);

  const refreshChanges = async () => {
    setRefreshRequestId(createOperationRequestId("changes-refresh"));
    setRefreshing(true);
    try {
      await refresh();
    } finally {
      setRefreshing(false);
    }
  };

  const previewRollback = async (point: UiRollbackPoint) => {
    if (!point.currentlyRestorable || (source !== "desktop" && source !== "partial")) {
      const failure = point.blockedReason || sourceBlockedReason(source, lang);
      setNotice(failure);
      addNotification({ dedupeKey: `rollback-preview-${point.id}-blocked`, level: "warning", title: translateCopy("ch.rollbackPreviewUnavailable", lang), message: failure, receipt: { requestId: createOperationRequestId("rollback-preview"), rollbackId: point.id } });
      return;
    }
    const requestId = createOperationRequestId("rollback-preview");
    setPreviewingId(point.id);
    setNotice("");
    try {
      const next = await invokeNative<RollbackPreview>("preview_rollback", { request: { rollback_id: point.id } });
      if (!isUsableRollbackPreview(next, point.id)) {
        const failure = translateCopy("ch.thisRollbackPointIsNoLongerRestorableRefreshAndRetry", lang);
        setNotice(failure);
        addNotification({ dedupeKey: `rollback-preview-${point.id}-failed`, level: "error", title: translateCopy("ch.rollbackPreviewFailed", lang), message: failure, receipt: { requestId, rollbackId: point.id } });
        return;
      }
      setPreview(next);
      addNotification({ dedupeKey: `rollback-preview-${point.id}-${next.revision}`, level: "info", title: translateCopy("ch.rollbackPreviewReady", lang), message: translateCopy("ch.integrityEvidenceIsReadyNoConfigurationChangesHappenBeforeConfirmation", lang), receipt: { requestId, rollbackId: point.id } });
    } catch (error) {
      const failure = presentOperationFailure("rollback", error, lang);
      setNotice(failure.message);
      addNotification({ dedupeKey: `rollback-preview-${point.id}-error`, level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, rollbackId: point.id } });
    } finally {
      setPreviewingId("");
    }
  };

  const applyRollback = async () => {
    if (!preview || executing) return;
    const requestId = createOperationRequestId("rollback");
    setExecuting(true);
    try {
      const receipt = await invokeNative<RollbackReceipt>("apply_rollback", {
        request: {
          rollback_id: preview.rollback_id,
          reviewed_revision: preview.revision,
          expected_current_sha256: preview.expected_current_sha256,
          confirmed: true,
        },
      });
      setPreview(null);
      if (!receipt.ok) {
        const failure = presentOperationFailure("rollback", receipt.summary, lang);
        setNotice(failure.message);
        addNotification({ dedupeKey: `rollback-${preview.rollback_id}-failed`, level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, rollbackId: preview.rollback_id } });
        return;
      }
      const message = receipt.verification_state === "verified"
        ? translateCopy("ch.rollbackCompletedFileAndRuntimeVerificationPassed", lang)
        : translateCopy("ch.filesWereRestoredButTheRuntimeIsOfflineStartItAndRunDoctor", lang);
      setNotice(message);
      addNotification({ dedupeKey: `rollback-${preview.rollback_id}`, level: receipt.verification_state === "verified" ? "success" : "warning", title: translateCopy("ch.rollbackApplied", lang), message, persistent: true, receipt: { requestId, rollbackId: preview.rollback_id } });
      await refresh();
    } catch (error) {
      setPreview(null);
      const failure = translateCopy("ch.theRollbackResultIsUnconfirmedRefreshAndRunDoctor", lang);
      setNotice(failure);
      const presentation = presentOperationFailure("rollback", error, lang);
      addNotification({ dedupeKey: `rollback-${preview.rollback_id}-error`, level: "error", title: presentation.title, message: failure, details: presentation.details, receipt: { requestId, rollbackId: preview.rollback_id } });
    } finally {
      setExecuting(false);
    }
  };

  const sourceLabel = source === "desktop"
    ? translateCopy("ch.liveDesktopDataRollbackPointS", lang, { v1: rollbackPoints.length })
    : source === "loading"
      ? translateCopy("ch.loadingLocalRecords", lang)
      : source === "browser-preview"
        ? translateCopy("ch.browserPreviewReadOnly", lang)
        : source === "partial"
          ? translateCopy("ch.runtimeOfflineLocalTransactionEvidenceOnly", lang)
          : source === "stale"
            ? translateCopy("ch.dataStaleActionsLocked", lang)
            : translateCopy("ch.localChangeDataUnavailable", lang);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="flex flex-wrap items-center gap-3">
            <h1 className="sr-only">{translateCopy("nav.changes", lang)}</h1>
            <DataSourceBadge mode={source === "desktop" ? "native" : source === "browser-preview" ? "preview" : "unavailable"} label={sourceLabel} />
          </div>
          <p className="mt-2 max-w-2xl text-sm text-muted-foreground">
            {translateCopy("ch.viewCommittedChangesAndRecoveryPointsConfigurationDifferencesRemainInPreWriteConfirmationAndHistoryDetails", lang)}
          </p>
        </div>
        <Button variant="outline" onClick={() => void refreshChanges()} disabled={status === "loading" || refreshing} aria-busy={refreshing}>
          <RefreshCw className={`size-4 ${status === "loading" || refreshing ? "animate-spin" : ""}`} />{refreshing ? translateCopy("ch.refreshing", lang) : translateCopy("ch.refresh", lang)}
        </Button>
      </div>

      {notice && <div role="status" className="rounded-lg border border-border bg-muted/35 px-4 py-3 text-sm text-muted-foreground">{notice}</div>}

      {source !== "desktop" && source !== "loading" && (
        <div className="flex gap-3 rounded-lg border border-amber-500/25 bg-amber-500/[0.06] px-4 py-3 text-sm text-amber-800 dark:text-amber-200">
          <ShieldAlert className="mt-0.5 size-4 shrink-0" />
          <span>{sourceBlockedReason(source, lang)}</span>
        </div>
      )}

      {unfinishedDrafts > 0 && (
        <Surface className="flex flex-wrap items-center justify-between gap-4 p-4">
          <div>
            <div className="font-medium">{translateCopy("ch.unfinishedConfigurationItemS", lang, { v1: unfinishedDrafts })}</div>
            <div className="mt-1 text-sm text-muted-foreground">{translateCopy("ch.returnToInstallConnectToViewTheCurrentChangesTheLocalPreviewIsGeneratedBeforeWriting", lang)}</div>
          </div>
          {setPage ? <Button size="sm" variant="outline" onClick={() => setPage("install")}>{translateCopy("ch.openInstallConnect", lang)}<ArrowRight className="size-3.5" /></Button> : null}
        </Surface>
      )}

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="backup">{translateCopy("ch.tab.backup", lang)}</TabsTrigger>
          <TabsTrigger value="history">{translateCopy("ch.changeHistory", lang)}</TabsTrigger>
        </TabsList>

        <TabsContent value="backup" className="mt-4 space-y-4">
          {rollbackPoints.length === 0 ? (
            <Surface className="p-6 text-sm text-muted-foreground">{translateCopy("ch.noRollbackPointsAreAvailable", lang)}</Surface>
          ) : visibleRollbackPoints.map((point) => (
            <Surface key={point.id} variant="row" className="p-4">
              <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <FileClock className="size-4 text-primary" />
                    <h2 className="text-sm font-medium">{point.label}</h2>
                    <Badge variant={point.currentlyRestorable ? "secondary" : "outline"}>{scopeLabel(point.scope, lang)}</Badge>
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{point.time} · {point.files ?? "—"} {translateCopy("ch.fileS", lang)}</div>
                  {point.transactionId && <div className="mt-1 break-all text-xs text-muted-foreground">{translateCopy("ch.transaction", lang)} <span className="font-mono">{point.transactionId}</span></div>}
                  {point.target && <div className="mt-2 break-all font-mono text-xs text-muted-foreground">{point.target}</div>}
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!point.currentlyRestorable || Boolean(previewingId) || executing || (source !== "desktop" && source !== "partial")}
                  title={point.currentlyRestorable ? translateCopy("ch.runIntegrityAndCurrentStatePreflightFirst", lang) : point.blockedReason}
                  onClick={() => void previewRollback(point)}
                >
                  {previewingId === point.id ? <RefreshCw className="size-3.5 animate-spin" /> : <RotateCcw className="size-3.5" />}
                  {previewingId === point.id ? translateCopy("ch.preflighting", lang) : translateCopy("ch.previewRollback", lang)}
                </Button>
              </div>
              <div className="mt-3 grid gap-2 text-xs sm:grid-cols-2">
                <EvidenceRow label={translateCopy("ch.backupIntegrity", lang)} ok={point.integrityVerified} detail={point.integrityVerified ? translateCopy("ch.metadataAndContentVerified", lang) : translateCopy("ch.integrityUnverified", lang)} />
                <EvidenceRow label={translateCopy("ch.currentlyRestorable", lang)} ok={point.currentlyRestorable} detail={point.currentlyRestorable ? translateCopy("ch.currentHashAndTransactionPreconditionsMatch", lang) : point.blockedReason} />
              </div>
            </Surface>
          ))}
          {rollbackPoints.length > visibleRollbackPoints.length && (
            <div className="flex justify-center pt-2">
              <Button variant="outline" size="sm" onClick={() => setVisibleRollbackCount((count) => count + CHANGES_PAGE_SIZE)}>
                {translateCopy("ch.loadMore", lang, { v1: visibleRollbackPoints.length, v2: rollbackPoints.length })}
              </Button>
            </div>
          )}
        </TabsContent>

        <TabsContent value="history" className="mt-4 space-y-3">
          {historyEntries.length === 0 ? (
            <Surface className="p-6 text-sm text-muted-foreground">{translateCopy("ch.noCommittedPersistentChangesAreRecorded", lang)}</Surface>
          ) : visibleHistoryEntries.map((entry) => (
            <Surface key={entry.id} variant="row" className="p-4">
              <div className="flex items-start gap-3">
                <History className="mt-0.5 size-4 shrink-0 text-primary" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <h2 className="text-sm font-medium">{entry.summary}</h2>
                    <Badge variant={entry.status === "ok" ? "secondary" : "outline"}>{historyStatusLabel(entry.status, lang)}</Badge>
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{entry.timestamp} · {entry.kind}</div>
                  {entry.target && <div className="mt-2 break-all font-mono text-xs text-muted-foreground">{entry.target}</div>}
                  {entry.rollbackId && <div className="mt-1 break-all text-xs text-muted-foreground">{translateCopy("ch.rollbackPoint", lang)} <span className="font-mono">{entry.rollbackId}</span></div>}
                  {entry.diff && (
                    <details className="mt-3 text-xs">
                      <summary className="cursor-pointer font-medium text-foreground">{translateCopy("ch.viewChangeDetails", lang)}</summary>
                      <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap break-words rounded-md border border-border/70 bg-background p-3 font-mono text-muted-foreground">{entry.diff}</pre>
                    </details>
                  )}
                </div>
              </div>
            </Surface>
          ))}
          {historyEntries.length > visibleHistoryEntries.length && (
            <div className="flex justify-center pt-2">
              <Button variant="outline" size="sm" onClick={() => setVisibleHistoryCount((count) => count + CHANGES_PAGE_SIZE)}>
                {translateCopy("ch.loadMore", lang, { v1: visibleHistoryEntries.length, v2: historyEntries.length })}
              </Button>
            </div>
          )}
        </TabsContent>
      </Tabs>

      <OperationDialog
        open={preview !== null}
        onOpenChange={(open) => { if (!open && !executing) setPreview(null); }}
        title={translateCopy("ch.confirmRollback", lang)}
        summary={translateCopy("ch.confirmationFirstProtectsTheCurrentStateThenRestoresTheTransactionAndVerifiesFilesAndRuntimeConcurrentEditsUnknownScopeOrDamagedBackupsAreRejected", lang)}
        confirmLabel={translateCopy("ch.rollbackAndVerify", lang)}
        cancelLabel={translateCopy("common.cancel", lang)}
        risk="danger"
        pending={executing}
        onConfirm={() => void applyRollback()}
        details={preview ? (
          <div className="space-y-2 rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted-foreground">
            <div><strong className="text-foreground">{translateCopy("ch.impact", lang)}：</strong>{preview.files} {translateCopy("ch.fileS", lang)} · {preview.target}</div>
            <div><strong className="text-foreground">{translateCopy("ch.integrity", lang)}：</strong>{preview.integrity_verified ? translateCopy("ch.verified", lang) : translateCopy("ch.unverified", lang)}</div>
            <div><strong className="text-foreground">{translateCopy("ch.currentState", lang)}：</strong>{preview.currently_restorable ? translateCopy("ch.restorable", lang) : preview.blocked_reason}</div>
            {preview.transaction_id ? <div><strong className="text-foreground">{translateCopy("ch.transaction2", lang)}：</strong><span className="break-all font-mono">{preview.transaction_id}</span></div> : null}
          </div>
        ) : null}
      />
    </div>
  );
}

function EvidenceRow({ label, ok, detail }: { label: string; ok: boolean; detail: string }) {
  return (
    <div className="rounded-md border border-border/70 bg-muted/20 p-3">
      <div className="flex items-center justify-between gap-2"><span className="font-medium text-foreground">{label}</span><Badge variant={ok ? "secondary" : "outline"}>{ok ? "OK" : "LOCKED"}</Badge></div>
      <div className="mt-1 text-muted-foreground">{detail || "—"}</div>
    </div>
  );
}

function sourceFromPayload(payload: Record<string, any> | null, status: string): ChangeSource {
  if (status === "loading") return "loading";
  if (status === "browser-preview") return "browser-preview";
  const evidence = classifyEvidence({ payload, status: "native", requireRuntimeIdentity: true, requireTransactionProvenance: true });
  if (evidence === "verified") return "desktop";
  if (evidence === "preview") return "browser-preview";
  return evidence;
}

function sourceBlockedReason(source: ChangeSource, lang: "zh" | "en"): string {
  if (source === "browser-preview") return translateCopy("ch.browserPreviewHasNoLocalBackupEvidenceRollbackIsLocked", lang);
  if (source === "partial") return translateCopy("ch.theRuntimeIsOfflineRollbackIsAllowedOnlyWhenTheDesktopControlPlaneRevalidatesIntegrityAndCurrentHashes", lang);
  if (source === "stale") return translateCopy("ch.dataIsStaleAndCannotProveCurrentFileStateRollbackIsLocked", lang);
  if (source === "loading") return translateCopy("ch.loadingLocalTransactionData", lang);
  return translateCopy("ch.trustedLocalTransactionEvidenceIsUnavailableRollbackIsLocked", lang);
}

function buildRollbackPoints(payload: Record<string, any> | null, source: ChangeSource, lang: "zh" | "en"): UiRollbackPoint[] {
  if (!["desktop", "partial", "stale"].includes(source)) return [];
  const rows = Array.isArray(payload?.backupPoints) ? payload.backupPoints : [];
  return rows.map((row: any) => {
    const scope = normalizeScope(row.scope);
    const integrityVerified = row.integrity_verified === true || row.verified === true;
    const currentlyRestorable = row.currently_restorable === true;
    return {
      id: safeText(row.id, ""),
      time: safeText(row.when, "unknown"),
      label: safeText(row.label, `Rollback point ${safeText(row.id, "unknown")}`),
      files: finiteNumber(row.files),
      target: safeText(row.target, ""),
      scope,
      executorKind: normalizeExecutorKind(row.executor_kind),
      transactionId: safeText(row.transaction_id, ""),
      integrityVerified,
      currentlyRestorable,
      blockedReason: safeText(
        row.blocked_reason ?? row.reason,
        currentlyRestorable ? "" : translateCopy("ch.thisPointHasNotPassedTheRestorabilityPreflight", lang),
      ),
      expectedCurrentSha256: safeText(row.expected_current_sha256 ?? row.expectedCurrentSha256, ""),
    };
  });
}

function countUnfinishedDrafts(payload: Record<string, any> | null): number {
  const drafts = Array.isArray(payload?.routeDrafts) ? payload.routeDrafts : [];
  return drafts.filter((draft: any) => !["applied", "rejected"].includes(draft?.resolution?.state)).length;
}

function buildChangeHistory(payload: Record<string, any> | null, lang: "zh" | "en"): ChangeHistoryEntry[] {
  const rows = Array.isArray(payload?.changeHistory)
    ? payload.changeHistory
    : Array.isArray(payload?.desktopTransactionEvents)
      ? payload.desktopTransactionEvents
      : [];
  return rows
    .filter((row: any) => isPersistentChangeStage(safeText(row.kind ?? row.stage, "")))
    .map((row: any, index: number) => {
      const metadata = row.metadata && typeof row.metadata === "object" ? row.metadata : {};
      const kind = safeText(row.kind ?? row.stage, "change");
      return {
        id: safeText(row.id, `${safeText(row.timestamp, "local")}-${kind}-${index}`),
        timestamp: safeText(row.timestamp, "local"),
        kind,
        status: safeText(row.status, "unknown"),
        summary: changeSummary(kind, lang),
        target: safeText(row.target ?? metadata.target, ""),
        rollbackId: safeText(row.rollback_id ?? metadata.rollback_id, ""),
        diff: normalizeDiff(row.diff ?? metadata.diff),
      };
    });
}

function normalizeDiff(value: unknown): string {
  if (typeof value === "string") return value.trim().slice(0, 32_000);
  if (!Array.isArray(value)) return "";
  return value
    .map((item) => typeof item === "string" ? item : safeText(item?.text, ""))
    .filter(Boolean)
    .join("\n")
    .slice(0, 32_000);
}

function isPersistentChangeStage(stage: string): boolean {
  if (!stage || stage.includes("preview") || stage.includes("scan") || stage.startsWith("service_") || stage.startsWith("doctor_")) return false;
  return /(apply|commit|rollback|recovery|autostart_(enable|disable))/.test(stage);
}

function isUsableRollbackPreview(value: RollbackPreview | null, id: string): boolean {
  return Boolean(value?.ok
    && value.mode === "rollback-preview"
    && value.rollback_id === id
    && value.integrity_verified
    && value.currently_restorable
    && value.revision
    && value.expected_current_sha256);
}

function normalizeScope(value: unknown): RollbackScope {
  return ["opencode-config", "dtoken-active-config", "learning-managed-block", "optical-config", "agent-integration"].includes(String(value)) ? value as RollbackScope : "unknown";
}

function normalizeExecutorKind(value: unknown): RollbackExecutorKind {
  return ["generic", "agent_integration", "optical"].includes(String(value))
    ? value as RollbackExecutorKind
    : "locked";
}

function finiteNumber(value: unknown): number | null {
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function safeText(value: unknown, fallback: string): string {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : fallback;
}

const SCOPE_LABEL_KEYS: Record<RollbackScope, string> = {
  "opencode-config": "ch.scope.opencodeConfig",
  "dtoken-active-config": "ch.scope.dtokenActiveConfig",
  "learning-managed-block": "ch.scope.learningManagedBlock",
  "optical-config": "ch.scope.opticalConfig",
  "agent-integration": "ch.scope.agentIntegration",
  unknown: "ch.scope.unknown",
};

function scopeLabel(scope: RollbackScope, lang: "zh" | "en"): string {
  return translateCopy(SCOPE_LABEL_KEYS[scope], lang);
}

const HISTORY_STATUS_KEYS: Record<string, string> = {
  ok: "ch.committed",
  recovered: "ch.recovered",
  failed: "ch.failed",
  unknown: "ch.unknown",
};

function historyStatusLabel(status: string, lang: "zh" | "en"): string {
  return translateCopy(HISTORY_STATUS_KEYS[status] ?? HISTORY_STATUS_KEYS.unknown, lang);
}

const CHANGE_SUMMARY_KEYS: Record<string, string> = {
  rollback: "ch.configurationRolledBack",
  install: "ch.installAndConnectionChangesCommitted",
  optical: "ch.visualCompressionChangesCommitted",
  learning: "ch.projectLearningChangesCommitted",
  other: "ch.localConfigurationChanged",
};

function changeSummary(kind: string, lang: "zh" | "en"): string {
  const key = ["rollback", "install", "optical", "learning"].find((candidate) => kind.includes(candidate)) ?? "other";
  return translateCopy(CHANGE_SUMMARY_KEYS[key], lang);
}
