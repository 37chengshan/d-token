import { useEffect, useMemo, useState } from "react";
import { ArrowRight, FileClock, History, RefreshCw, RotateCcw, ShieldAlert } from "lucide-react";
import { Surface } from "../common/Surface";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../ui/tabs";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import { OperationDialog } from "../common/OperationDialog";
import { useI18n } from "../../lib/i18n";
import { invokeNative } from "../../lib/native";
import type { PageId } from "../../lib/nav";
import { classifyEvidence } from "../../lib/state/evidence";
import { useSessionTab } from "../../lib/sessionTab";
import { useRuntimeDashboardDetail } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";

const CHANGES_TABS = ["backup", "history"] as const;

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
  const zh = lang === "zh";
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
  const rollbackPoints = useMemo(() => buildRollbackPoints(payload, source), [payload, source]);
  const historyEntries = useMemo(() => buildChangeHistory(payload, lang), [lang, payload]);
  const unfinishedDrafts = useMemo(() => countUnfinishedDrafts(payload), [payload]);

  useEffect(() => {
    if (refreshing || !refreshRequestId) return;
    const available = source === "desktop" || source === "partial";
    const stale = source === "stale";
    addNotification({
      dedupeKey: available ? "changes-refresh" : stale ? "changes-refresh-stale" : "changes-refresh-failed",
      level: available ? "success" : stale ? "warning" : "error",
      title: available ? (zh ? "变更记录已更新" : "Change records refreshed") : stale ? (zh ? "变更记录刷新不完整" : "Change refresh incomplete") : (zh ? "变更记录刷新失败" : "Change refresh failed"),
      message: available ? (zh ? "回退点与事务记录已重新读取。" : "Rollback points and transaction records were refreshed.") : stale ? (zh ? "仍显示上一份可验证记录。" : "The last verified records remain visible.") : sourceBlockedReason(source, lang),
      receipt: { requestId: refreshRequestId, targetId: "changes" },
    });
    setRefreshRequestId("");
  }, [addNotification, error, lang, refreshRequestId, refreshing, source, zh]);

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
      addNotification({ dedupeKey: `rollback-preview-${point.id}-blocked`, level: "warning", title: zh ? "回退预览不可用" : "Rollback preview unavailable", message: failure, receipt: { requestId: createOperationRequestId("rollback-preview"), rollbackId: point.id } });
      return;
    }
    const requestId = createOperationRequestId("rollback-preview");
    setPreviewingId(point.id);
    setNotice("");
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
        ? (zh ? "回退完成，文件与运行时复检均已通过。" : "Rollback completed; file and runtime verification passed.")
        : (zh ? "文件已恢复，但运行时当前离线；请启动服务后运行 Doctor 复检。" : "Files were restored, but the runtime is offline. Start it and run Doctor." );
      setNotice(message);
      addNotification({ dedupeKey: `rollback-${preview.rollback_id}`, level: receipt.verification_state === "verified" ? "success" : "warning", title: zh ? "回退已执行" : "Rollback applied", message, persistent: true, receipt: { requestId, rollbackId: preview.rollback_id } });
      await refresh();
    } catch (error) {
      setPreview(null);
      const failure = zh ? "回退结果未确认，请刷新列表并运行 Doctor 检查当前状态。" : "The rollback result is unconfirmed. Refresh and run Doctor.";
      setNotice(failure);
      const presentation = presentOperationFailure("rollback", error, lang);
      addNotification({ dedupeKey: `rollback-${preview.rollback_id}-error`, level: "error", title: presentation.title, message: failure, details: presentation.details, receipt: { requestId, rollbackId: preview.rollback_id } });
    } finally {
      setExecuting(false);
    }
  };

  const sourceLabel = source === "desktop"
    ? (zh ? `实时桌面数据 · ${rollbackPoints.length} 个回滚点` : `Live desktop data · ${rollbackPoints.length} rollback point(s)`)
    : source === "loading"
      ? (zh ? "正在读取本地记录" : "Loading local records")
      : source === "browser-preview"
        ? (zh ? "浏览器预览 · 只读" : "Browser preview · read-only")
        : source === "partial"
          ? (zh ? "运行时离线 · 仅显示本地事务证据" : "Runtime offline · local transaction evidence only")
          : source === "stale"
            ? (zh ? "数据已过期 · 操作锁定" : "Data stale · actions locked")
            : (zh ? "本地变更数据不可用" : "Local change data unavailable");

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="flex flex-wrap items-center gap-3">
            <h1>{zh ? "变更与回退" : "Changes & Rollback"}</h1>
            <DataSourceBadge mode={source === "desktop" ? "native" : source === "browser-preview" ? "preview" : "unavailable"} label={sourceLabel} />
          </div>
          <p className="mt-2 max-w-2xl text-sm text-muted-foreground">
            {zh ? "查看已提交的持久变更与可恢复点。配置差异仍在每次写入前的确认流程和变更记录详情中展示。" : "View committed changes and recovery points. Configuration differences remain in pre-write confirmation and history details."}
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
          {setPage ? <Button size="sm" variant="outline" onClick={() => setPage("install")}>{zh ? "前往安装与连接" : "Open Install & Connect"}<ArrowRight className="size-3.5" /></Button> : null}
        </Surface>
      )}

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="backup">{zh ? "备份与回滚" : "Backup & Rollback"}</TabsTrigger>
          <TabsTrigger value="history">{zh ? "变更记录" : "Change history"}</TabsTrigger>
        </TabsList>

        <TabsContent value="backup" className="mt-4 space-y-4">
          {rollbackPoints.length === 0 ? (
            <Surface className="p-6 text-sm text-muted-foreground">{zh ? "当前没有可用回滚点" : "No rollback points are available"}</Surface>
          ) : rollbackPoints.map((point) => (
            <Surface key={point.id} variant="row" className="p-4">
              <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <FileClock className="size-4 text-primary" />
                    <h2 className="text-sm font-medium">{point.label}</h2>
                    <Badge variant={point.currentlyRestorable ? "secondary" : "outline"}>{scopeLabel(point.scope, lang)}</Badge>
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{point.time} · {point.files ?? "—"} {zh ? "个文件" : "file(s)"}</div>
                  {point.transactionId && <div className="mt-1 break-all font-mono text-xs text-muted-foreground">transaction_id={point.transactionId}</div>}
                  {point.target && <div className="mt-2 break-all font-mono text-xs text-muted-foreground">{point.target}</div>}
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!point.currentlyRestorable || Boolean(previewingId) || executing || (source !== "desktop" && source !== "partial")}
                  title={point.currentlyRestorable ? (zh ? "先执行完整性与当前状态预检" : "Run integrity and current-state preflight first") : point.blockedReason}
                  onClick={() => void previewRollback(point)}
                >
                  {previewingId === point.id ? <RefreshCw className="size-3.5 animate-spin" /> : <RotateCcw className="size-3.5" />}
                  {previewingId === point.id ? (zh ? "正在预检" : "Preflighting") : (zh ? "预览回退" : "Preview rollback")}
                </Button>
              </div>
              <div className="mt-3 grid gap-2 text-xs sm:grid-cols-2">
                <EvidenceRow label={zh ? "备份完整性" : "Backup integrity"} ok={point.integrityVerified} detail={point.integrityVerified ? (zh ? "元数据与备份内容已验证" : "Metadata and content verified") : (zh ? "完整性未验证" : "Integrity unverified")} />
                <EvidenceRow label={zh ? "当前可恢复性" : "Currently restorable"} ok={point.currentlyRestorable} detail={point.currentlyRestorable ? (zh ? "当前哈希与事务前置条件匹配" : "Current hash and transaction preconditions match") : point.blockedReason} />
              </div>
            </Surface>
          ))}
        </TabsContent>

        <TabsContent value="history" className="mt-4 space-y-3">
          {historyEntries.length === 0 ? (
            <Surface className="p-6 text-sm text-muted-foreground">{zh ? "尚无已提交的持久变更记录" : "No committed persistent changes are recorded"}</Surface>
          ) : historyEntries.map((entry) => (
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
                  {entry.rollbackId && <div className="mt-1 text-xs text-muted-foreground">rollback_id={entry.rollbackId}</div>}
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
        onConfirm={() => void applyRollback()}
        details={preview ? (
          <div className="space-y-2 rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted-foreground">
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
  const zh = lang === "zh";
  if (source === "browser-preview") return zh ? "浏览器预览仅展示结构，不提供本机备份证据，回退已锁定。" : "Browser preview has no local backup evidence; rollback is locked.";
  if (source === "partial") return zh ? "运行时离线；仅当桌面控制面能重新验证备份完整性与当前哈希时才允许回退。" : "The runtime is offline; rollback is allowed only when the desktop control plane revalidates integrity and current hashes.";
  if (source === "stale") return zh ? "数据已过期，不能证明当前文件状态，回退已锁定。" : "Data is stale and cannot prove current file state; rollback is locked.";
  if (source === "loading") return zh ? "正在读取本地事务数据。" : "Loading local transaction data.";
  return zh ? "无法读取可信的本地事务证据，回退已锁定。" : "Trusted local transaction evidence is unavailable; rollback is locked.";
}

function buildRollbackPoints(payload: Record<string, any> | null, source: ChangeSource): UiRollbackPoint[] {
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
      blockedReason: safeText(row.blocked_reason ?? row.reason, currentlyRestorable ? "" : "当前状态尚未通过可恢复性预检。"),
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

function scopeLabel(scope: RollbackScope, lang: "zh" | "en"): string {
  const labels: Record<RollbackScope, { zh: string; en: string }> = {
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
