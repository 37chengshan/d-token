import { useEffect, useMemo, useState } from "react";
import { ArrowRight, FileClock, History, RefreshCw, RotateCcw, ShieldAlert } from "lucide-react";
import { Surface } from "../common/Surface";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { OperationDialog } from "../common/OperationDialog";
import { useI18n } from "../../lib/i18n";
import { invokeNative } from "../../lib/native";
import type { PageId } from "../../lib/nav";
import { useSessionTab } from "../../lib/sessionTab";
import { useConfigChangeRecords } from "../../lib/useConfigChangeRecords";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";

const CHANGES_TABS = ["backup", "history"] as const;
// Rollback points and change history grow with use; render a bounded window.
const CHANGES_PAGE_SIZE = 40;

type ChangeSource = "loading" | "desktop" | "browser-preview" | "unavailable" | "stale" | "partial";

type RollbackScope = "managed-agent-config" | "opencode-config" | "dtoken-active-config" | "learning-managed-block" | "optical-config" | "agent-integration" | "unknown";
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

export function Changes({
  setPage,
  embedded = false,
}: {
  setPage?: (page: PageId) => void;
  embedded?: boolean;
} = {}) {
  const { lang } = useI18n();
  const zh = lang === "zh";
  const { addNotification } = useNotifications();
  const [tab, setTab] = useSessionTab("d-token.changes.tab", CHANGES_TABS, "backup");
  const { payload, status, error, refresh } = useConfigChangeRecords();
  const [notice, setNotice] = useState("");
  const [preview, setPreview] = useState<RollbackPreview | null>(null);
  const [previewingId, setPreviewingId] = useState("");
  const [rollbackError, setRollbackError] = useState("");
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
    const available = source === "desktop";
    const incomplete = source === "partial" || source === "stale";
    addNotification({
      dedupeKey: available ? "changes-refresh" : incomplete ? "changes-refresh-incomplete" : "changes-refresh-failed",
      level: available ? "success" : incomplete ? "warning" : "error",
      title: available ? (zh ? "变更记录已更新" : "Change records refreshed") : incomplete ? (zh ? "变更记录刷新不完整" : "Change refresh incomplete") : (zh ? "变更记录刷新失败" : "Change refresh failed"),
      message: available ? (zh ? "已重新读取回退点与事务记录。" : "Rollback points and transaction records were refreshed.") : incomplete ? sourceBlockedReason(source, lang) : sourceBlockedReason(source, lang),
      persistent: !available,
      receipt: { requestId: refreshRequestId, targetId: "changes" },
    });
    setRefreshRequestId("");
  }, [addNotification, error, lang, refreshRequestId, refreshing, source, zh]);

  const refreshChanges = async () => {
    const requestId = createOperationRequestId("changes-refresh");
    setRefreshing(true);
    try {
      await refresh();
      setRefreshRequestId(requestId);
    } catch (error) {
      const failure = presentOperationFailure("generic", error, lang);
      const message = zh ? "变更记录刷新失败，请重试或打开 Doctor 检查本地控制面。" : "Change refresh failed. Retry or open Doctor to inspect the local control plane.";
      setNotice(message);
      addNotification({
        dedupeKey: "changes-refresh-request-failed",
        level: "error",
        title: zh ? "变更记录刷新失败" : "Change refresh failed",
        message,
        details: failure.details,
        persistent: true,
        receipt: { requestId, targetId: "changes" },
      });
    } finally {
      setRefreshing(false);
    }
  };

  const previewRollback = async (point: UiRollbackPoint) => {
    if (!point.currentlyRestorable || source !== "desktop") {
      const failure = point.blockedReason || sourceBlockedReason(source, lang);
      setNotice(failure);
      addNotification({ dedupeKey: `rollback-preview-${point.id}-blocked`, level: "warning", title: zh ? "回退预览不可用" : "Rollback preview unavailable", message: failure, receipt: { requestId: createOperationRequestId("rollback-preview"), rollbackId: point.id } });
      return;
    }
    const requestId = createOperationRequestId("rollback-preview");
    setPreviewingId(point.id);
    setNotice("");
    setRollbackError("");
    try {
      const next = await invokeNative<RollbackPreview>("preview_rollback", { request: { rollback_id: point.id } });
      if (!isUsableRollbackPreview(next, point.id)) {
        const failure = zh ? "当前回滚点已不可恢复，请刷新后重试。" : "This rollback point is no longer restorable. Refresh and retry.";
        setNotice(failure);
        addNotification({ dedupeKey: `rollback-preview-${point.id}-failed`, level: "error", title: zh ? "回退预览失败" : "Rollback preview failed", message: failure, receipt: { requestId, rollbackId: point.id } });
        return;
      }
      setPreview(next);
      addNotification({ dedupeKey: `rollback-preview-${point.id}-${next.revision}`, level: "info", title: zh ? "回退预览已准备" : "Rollback preview ready", message: zh ? "已读取当前完整性证据；确认前不会修改配置。" : "Integrity evidence is ready; no configuration changes happen before confirmation.", receipt: { requestId, rollbackId: point.id } });
    } catch (error) {
      const failure = presentOperationFailure("rollback", error, lang);
      setNotice(failure.message);
      addNotification({ dedupeKey: `rollback-preview-${point.id}-error`, level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, rollbackId: point.id } });
    } finally {
      setPreviewingId("");
    }
  };

  const applyRollback = async () => {
    if (!preview || executing || source !== "desktop") return;
    const requestId = createOperationRequestId("rollback");
    setExecuting(true);
    setRollbackError("");
    try {
      const receipt = await invokeNative<RollbackReceipt>("apply_rollback", {
        request: {
          rollback_id: preview.rollback_id,
          reviewed_revision: preview.revision,
          expected_current_sha256: preview.expected_current_sha256,
          confirmed: true,
        },
      });
      if (!receipt.ok) {
        const failure = presentOperationFailure("rollback", receipt.summary, lang);
        setNotice(failure.message);
        setRollbackError(failure.message);
        addNotification({ dedupeKey: `rollback-${preview.rollback_id}-failed`, level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, rollbackId: preview.rollback_id } });
        return;
      }
      const message = receipt.verification_state === "verified"
        ? (zh ? "回退完成，文件与运行时复检均已通过。" : "Rollback completed; file and runtime verification passed.")
        : (zh ? "文件已恢复，但运行时当前离线；请启动服务后运行 Doctor 复检。" : "Files were restored, but the runtime is offline. Start it and run Doctor." );
      setNotice(message);
      addNotification({ dedupeKey: `rollback-${preview.rollback_id}`, level: receipt.verification_state === "verified" ? "success" : "warning", title: zh ? "回退已执行" : "Rollback applied", message, persistent: true, receipt: { requestId, rollbackId: preview.rollback_id } });
      setPreview(null);
      await refresh();
    } catch (error) {
      const failure = zh ? "回退结果未确认，请刷新列表并运行 Doctor 检查当前状态。" : "The rollback result is unconfirmed. Refresh and run Doctor.";
      setNotice(failure);
      setRollbackError(failure);
      const presentation = presentOperationFailure("rollback", error, lang);
      addNotification({ dedupeKey: `rollback-${preview.rollback_id}-error`, level: "error", title: presentation.title, message: failure, details: presentation.details, persistent: true, receipt: { requestId, rollbackId: preview.rollback_id } });
    } finally {
      setExecuting(false);
    }
  };


  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="flex flex-wrap items-center gap-3">
            {!embedded && <h1 className="sr-only">{zh ? "变更与回退" : "Changes & Rollback"}</h1>}
          </div>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {zh ? "恢复前会重新检查备份、当前文件和事务归属。" : "Before restoring, d-token rechecks the backup, current files, and transaction ownership."}
          </p>
        </div>
        <Button variant="outline" onClick={() => void refreshChanges()} disabled={status === "loading" || refreshing} aria-busy={refreshing}>
          <RefreshCw className={`size-4 ${status === "loading" || refreshing ? "animate-spin" : ""}`} />{refreshing ? (zh ? "刷新中…" : "Refreshing…") : (zh ? "刷新" : "Refresh")}
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
            <div className="font-medium">{zh ? `有 ${unfinishedDrafts} 项未完成配置` : `${unfinishedDrafts} unfinished configuration item(s)`}</div>
            <div className="mt-1 text-sm text-muted-foreground">{zh ? "请返回安装与连接流程查看当前变更；差异会在写入前由本机预览生成。" : "Return to Install & Connect to view the current changes; the local preview is generated before writing."}</div>
          </div>
          {setPage ? <Button size="sm" variant="outline" onClick={() => setPage("configuration")}>{zh ? "前往配置与连接" : "Open Configuration"}<ArrowRight className="size-3.5" /></Button> : null}
        </Surface>
      )}

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="backup">{zh ? "备份与回滚" : "Backup & Rollback"}</TabsTrigger>
          <TabsTrigger value="history">{zh ? "变更记录" : "Change history"}</TabsTrigger>
        </TabsList>

        <TabsContent value="backup" className="mt-4 space-y-4">
          {rollbackPoints.length === 0 ? (
            <Surface className="p-4 text-sm text-muted-foreground">{source === "loading" ? (zh ? "正在读取恢复记录…" : "Loading restore points…") : (["unavailable", "stale", "partial"].includes(source) ? (zh ? "恢复记录暂时无法读取，请刷新后重试。" : "Restore records are unavailable. Refresh and retry.") : (zh ? "当前没有可用回滚点" : "No rollback points are available"))}</Surface>
          ) : visibleRollbackPoints.map((point) => (
            <Surface key={point.id} variant="row" className="p-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <FileClock className="size-4 text-primary" />
                    <h2 className="text-sm font-medium">{point.label}</h2>
                    <Badge variant={point.currentlyRestorable ? "secondary" : "outline"}>{scopeLabel(point.scope, lang)}</Badge>
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{point.time} · {point.files ?? "—"} {zh ? "个文件" : "file(s)"}</div>
                  {(point.transactionId || point.target) ? (
                    <details className="mt-2 text-xs text-muted-foreground">
                      <summary className="cursor-pointer">{zh ? "文件与记录信息" : "Files and record details"}</summary>
                      {point.transactionId && <div className="mt-1 break-all text-xs text-muted-foreground">{zh ? "事务编号" : "Transaction"} <span className="font-mono">{point.transactionId}</span></div>}
                      {point.target && <div className="mt-2 break-all font-mono text-xs text-muted-foreground">{point.target}</div>}
                    </details>
                  ) : null}
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!point.currentlyRestorable || Boolean(previewingId) || executing || source !== "desktop"}
                  title={point.currentlyRestorable ? (zh ? "先检查完整性与当前状态" : "Check integrity and current state first") : point.blockedReason}
                  onClick={() => void previewRollback(point)}
                >
                  {previewingId === point.id ? <RefreshCw className="size-3.5 animate-spin" /> : <RotateCcw className="size-3.5" />}
                  {previewingId === point.id ? (zh ? "正在检查" : "Checking") : (zh ? "预览回退" : "Preview rollback")}
                </Button>
              </div>
              <div className="mt-3 grid gap-2 text-xs sm:grid-cols-2">
                <EvidenceRow label={zh ? "备份完整性" : "Backup integrity"} ok={point.integrityVerified} detail={point.integrityVerified ? (zh ? "元数据与备份内容已验证" : "Metadata and content verified") : (zh ? "完整性未验证" : "Integrity unverified")} />
                <EvidenceRow label={zh ? "当前可恢复性" : "Currently restorable"} ok={point.currentlyRestorable} detail={point.currentlyRestorable ? (zh ? "当前哈希与事务前置条件匹配" : "Current hash and transaction preconditions match") : point.blockedReason} />
              </div>
            </Surface>
          ))}
          {rollbackPoints.length > visibleRollbackPoints.length && (
            <div className="flex justify-center pt-2">
              <Button variant="outline" size="sm" onClick={() => setVisibleRollbackCount((count) => count + CHANGES_PAGE_SIZE)}>
                {zh
                  ? `加载更多（已显示 ${visibleRollbackPoints.length}/${rollbackPoints.length}）`
                  : `Load more (${visibleRollbackPoints.length}/${rollbackPoints.length})`}
              </Button>
            </div>
          )}
        </TabsContent>

        <TabsContent value="history" className="mt-4 space-y-3">
          {historyEntries.length === 0 ? (
            <Surface className="p-6 text-sm text-muted-foreground">{zh ? "尚无已提交的持久变更记录" : "No committed persistent changes are recorded"}</Surface>
          ) : visibleHistoryEntries.map((entry) => (
            <Surface key={entry.id} variant="row" className="p-3">
              <div className="flex items-start gap-3">
                <History className="mt-0.5 size-4 shrink-0 text-primary" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <h2 className="text-sm font-medium">{entry.summary}</h2>
                    <Badge variant={entry.status === "ok" ? "secondary" : "outline"}>{historyStatusLabel(entry.status, lang)}</Badge>
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{entry.timestamp} · {entry.kind}</div>
                  {entry.target && <div className="mt-2 break-all font-mono text-xs text-muted-foreground">{entry.target}</div>}
                  {entry.rollbackId && <div className="mt-1 break-all text-xs text-muted-foreground">{zh ? "回退点" : "Rollback point"} <span className="font-mono">{entry.rollbackId}</span></div>}
                  {entry.diff && (
                    <details className="mt-3 text-xs">
                      <summary className="cursor-pointer font-medium text-foreground">{zh ? "查看变更详情" : "View change details"}</summary>
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
                {zh
                  ? `加载更多（已显示 ${visibleHistoryEntries.length}/${historyEntries.length}）`
                  : `Load more (${visibleHistoryEntries.length}/${historyEntries.length})`}
              </Button>
            </div>
          )}
        </TabsContent>
      </Tabs>

      <OperationDialog
        open={preview !== null}
        onOpenChange={(open) => { if (!open && !executing) setPreview(null); }}
        title={zh ? "确认回退" : "Confirm rollback"}
        summary={zh ? "确认后会先备份当前状态，再按原事务恢复，并执行文件与运行时专项复检。并发修改、未知范围或损坏备份会被拒绝。" : "Confirmation first protects the current state, then restores the transaction and verifies files and runtime. Concurrent edits, unknown scope, or damaged backups are rejected."}
        confirmLabel={zh ? "确认回退并复检" : "Rollback and verify"}
        cancelLabel={zh ? "取消" : "Cancel"}
        risk="danger"
        pending={executing}
        confirmDisabled={source !== "desktop"}
        error={source !== "desktop" ? sourceBlockedReason(source, lang) : rollbackError}
        onConfirm={() => void applyRollback()}
        details={preview ? (
          <div className="space-y-1 border-l border-border pl-3 text-sm text-muted-foreground">
            <div><strong className="text-foreground">{zh ? "影响" : "Impact"}：</strong>{preview.files} {zh ? "个文件" : "file(s)"} · {preview.target}</div>
            <div><strong className="text-foreground">{zh ? "完整性" : "Integrity"}：</strong>{preview.integrity_verified ? (zh ? "已验证" : "verified") : (zh ? "未验证" : "unverified")}</div>
            <div><strong className="text-foreground">{zh ? "当前状态" : "Current state"}：</strong>{preview.currently_restorable ? (zh ? "允许恢复" : "restorable") : preview.blocked_reason}</div>
            {preview.transaction_id ? <div><strong className="text-foreground">{zh ? "事务" : "Transaction"}：</strong><span className="break-all font-mono">{preview.transaction_id}</span></div> : null}
          </div>
        ) : null}
      />
    </div>
  );
}

function EvidenceRow({ label, ok, detail }: { label: string; ok: boolean; detail: string }) {
  const { lang } = useI18n();
  return (
    <div className="border-t border-border/60 pt-2">
      <div className="flex items-center justify-between gap-2"><span className="font-medium text-foreground">{label}</span><Badge variant={ok ? "secondary" : "outline"}>{ok ? (lang === "zh" ? "已通过" : "Passed") : (lang === "zh" ? "未通过" : "Not passed")}</Badge></div>
      <div className="mt-1 leading-5 text-muted-foreground">{detail || "—"}</div>
    </div>
  );
}

function sourceFromPayload(payload: Record<string, any> | null, status: string): ChangeSource {
  if (status === "loading") return "loading";
  if (status === "browser-preview") return "browser-preview";
  if (status === "partial") return "partial";
  if (status === "stale") return "stale";
  if (status === "unavailable") return "unavailable";
  return payload?.source === "dtoken-desktop-control-plane" && payload.complete === true ? "desktop" : "unavailable";
}

function sourceBlockedReason(source: ChangeSource, lang: "zh" | "en"): string {
  const zh = lang === "zh";
  if (source === "browser-preview") return zh ? "浏览器预览仅展示结构，不提供本机备份证据，回退已锁定。" : "Browser preview has no local backup evidence; rollback is locked.";
  if (source === "partial") return zh ? "部分恢复记录未能读取，请刷新后重试。" : "Some restore records could not be read. Refresh and retry.";
  if (source === "stale") return zh ? "数据已过期，不能证明当前文件状态，回退已锁定。" : "Data is stale and cannot prove current file state; rollback is locked.";
  if (source === "loading") return zh ? "正在读取本地事务数据。" : "Loading local transaction data.";
  return zh ? "无法读取可信的本地事务证据，回退已锁定。" : "Trusted local transaction evidence is unavailable; rollback is locked.";
}

function buildRollbackPoints(payload: Record<string, any> | null, source: ChangeSource, lang: "zh" | "en"): UiRollbackPoint[] {
  if (!["desktop", "partial", "stale"].includes(source)) return [];
  const rows = Array.isArray(payload?.backupPoints) ? payload.backupPoints : [];
  return rows.map((row: any) => {
    const scope = normalizeScope(row.scope);
    const integrityVerified = typeof row.integrity_verified === "boolean" ? row.integrity_verified : row.verified === true;
    const currentlyRestorable = integrityVerified && scope !== "unknown" && row.currently_restorable === true;
    return {
      id: safeText(row.id, ""),
      time: safeText(row.when, "unknown"),
      label: `${scopeLabel(scope, lang)}${lang === "zh" ? "恢复点" : " restore point"}`,
      files: finiteNumber(row.files),
      target: safeText(row.target, ""),
      scope,
      executorKind: normalizeExecutorKind(row.executor_kind),
      transactionId: safeText(row.transaction_id, ""),
      integrityVerified,
      currentlyRestorable,
      blockedReason: currentlyRestorable ? "" : rollbackBlockedReason(row.blocked_reason ?? row.reason, lang),
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

function rollbackBlockedReason(value: unknown, lang: "zh" | "en"): string {
  const reason = safeText(value, "").toLowerCase();
  const copy = (zh: string, en: string) => lang === "zh" ? zh : en;
  if (reason.includes("already rolled back")) return copy("这份配置已经恢复，无需再次操作。", "This configuration has already been restored.");
  if (reason.includes("changed")) return copy("文件在备份后发生了变化。请先查看当前配置，避免覆盖后续修改。", "The file changed after this backup. Check the current settings to avoid overwriting later edits.");
  if (reason.includes("legacy")) return copy("这是旧版恢复记录，缺少完整备份信息。请返回安装与连接查看当前配置。", "This older record lacks complete backup information. Open Install & Connect to check the current settings.");
  if (reason.includes("allowlist") || reason.includes("unknown")) return copy("暂不能确认这份备份对应的配置。请返回安装与连接，从对应工具处理。", "This backup's configuration could not be identified. Open the corresponding tool in Install & Connect.");
  if (reason.includes("integrity") || reason.includes("metadata") || reason.includes("damaged")) return copy("备份不完整或校验未通过，暂不能恢复。请使用其他恢复点。", "This backup is incomplete or failed validation. Use another restore point.");
  return copy("暂不能安全恢复这份配置。请刷新后重试，或返回安装与连接检查。", "This configuration cannot be restored safely yet. Refresh and retry, or check Install & Connect.");
}


function normalizeScope(value: unknown): RollbackScope {
  return ["managed-agent-config", "opencode-config", "dtoken-active-config", "learning-managed-block", "optical-config", "agent-integration"].includes(String(value)) ? value as RollbackScope : "unknown";
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

function scopeLabel(scope: RollbackScope, lang: "zh" | "en"): string {
  const labels: Record<RollbackScope, { zh: string; en: string }> = {
    "managed-agent-config": { zh: "工具配置", en: "Tool configuration" },
    "opencode-config": { zh: "OpenCode 配置", en: "OpenCode config" },
    "dtoken-active-config": { zh: "d-token 路由", en: "d-token route" },
    "learning-managed-block": { zh: "学习文档", en: "Learning document" },
    "optical-config": { zh: "视觉压缩事务", en: "Optical transaction" },
    "agent-integration": { zh: "Agent 集成事务", en: "Agent integration" },
    unknown: { zh: "未知范围", en: "Unknown scope" },
  };
  return labels[scope][lang];
}

function historyStatusLabel(status: string, lang: "zh" | "en"): string {
  if (status === "ok") return lang === "zh" ? "已提交" : "Committed";
  if (status === "recovered") return lang === "zh" ? "已恢复" : "Recovered";
  if (status === "failed") return lang === "zh" ? "失败" : "Failed";
  return lang === "zh" ? "结果未知" : "Unknown";
}

function changeSummary(kind: string, lang: "zh" | "en"): string {
  if (kind.includes("rollback")) return lang === "zh" ? "配置已回退" : "Configuration rolled back";
  if (kind.includes("install")) return lang === "zh" ? "安装与连接配置已提交" : "Install and connection changes committed";
  if (kind.includes("optical")) return lang === "zh" ? "视觉压缩配置已提交" : "Visual compression changes committed";
  if (kind.includes("learning")) return lang === "zh" ? "项目学习配置已提交" : "Project learning changes committed";
  return lang === "zh" ? "本地配置已变更" : "Local configuration changed";
}
