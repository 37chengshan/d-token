import { useEffect, useMemo, useState } from "react";
import { motion } from "motion/react";
import { Check, X, RotateCcw, FileCode, Folder, RefreshCw } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Button } from "../ui/button";
import { Badge } from "../ui/badge";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "../ui/alert-dialog";
import { useI18n, useT } from "../../lib/i18n";
import { invokeNative, type NativeActionResult } from "../../lib/native";
import { classifyEvidence } from "../../lib/state/evidence";
import {
  isUsableRouteReview,
  routeApplyFailureMessage,
  routeApplyRequest,
  routeReviewFailureMessage,
  type RouteDraftApplyReview,
} from "../../lib/routes/review";

export function Changes() {
  const t = useT();
  const { lang } = useI18n();
  const [desktopData, setDesktopData] = useState<Record<string, any> | null>(null);
  const [source, setSource] = useState<ChangeSource>("loading");
  const [notice, setNotice] = useState("");
  const [hiddenChanges, setHiddenChanges] = useState<Set<string>>(new Set());
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null);
  const [isExecuting, setIsExecuting] = useState(false);
  const [reviewingChangeId, setReviewingChangeId] = useState("");

  const refreshDesktopData = async () => {
    try {
      const next = await invokeNative<Record<string, any>>("get_desktop_data");
      setDesktopData(next);
      setSource(sourceFromDesktopPayload(next));
    } catch (error) {
      setSource(error instanceof Error && error.message === "native_unavailable" ? "browser-preview" : "unavailable");
    }
  };

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const next = await invokeNative<Record<string, any>>("get_desktop_data");
        if (!cancelled) {
          setDesktopData(next);
          setSource(sourceFromDesktopPayload(next));
        }
      } catch (error) {
        if (!cancelled) {
          setSource(error instanceof Error && error.message === "native_unavailable" ? "browser-preview" : "unavailable");
        }
      }
    };
    void load();
    const timer = window.setInterval(() => {
      void load();
    }, 3000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, []);

  const pendingChanges = useMemo(
    () => buildChanges(desktopData, source).filter((change) => !hiddenChanges.has(change.id)),
    [desktopData, hiddenChanges, source],
  );
  const backupRows = useMemo(() => buildBackups(desktopData, source), [desktopData, source]);
  const ownershipRows = useMemo(() => buildOwnership(desktopData, source), [desktopData, source]);
  const verifiedBackups = useMemo(
    () => backupRows.filter((backup) => getRollbackGate(source, backup).allowed),
    [backupRows, source],
  );
  const pendingConfirmation = pendingAction
    ? buildConfirmation(pendingAction, source, verifiedBackups, lang)
    : null;
  const sourceLabel =
    source === "desktop"
      ? `d-token 桌面端 · ${pendingChanges.length} 项待处理 · ${backupRows.length} 个回滚点`
      : source === "browser-preview"
        ? "浏览器预览 · 不可执行"
        : source === "loading"
          ? "正在读取 d-token 桌面数据"
          : source === "stale"
            ? "桌面数据已过期 · 动作已锁定"
            : source === "partial"
              ? "桌面数据不完整 · 动作已锁定"
              : "桌面数据不可用";

  const startApproval = async (change: UiChange) => {
    const gate = getApprovalGate(source, change, verifiedBackups);
    if (!gate.allowed) {
      setNotice(gate.reason);
      return;
    }
    if (change.action !== "apply_route_draft") {
      setPendingAction({ kind: "approve", change, review: null });
      return;
    }
    if (!change.routeId) {
      setNotice(lang === "zh" ? "无法审阅：变更没有可核验的路由标识。" : "Review unavailable: the change has no verifiable route identifier.");
      return;
    }

    setReviewingChangeId(change.id);
    setNotice("");
    try {
      const review = await invokeNative<RouteDraftApplyReview>("preview_route_profile_draft_apply", {
        request: { route_id: change.routeId },
      });
      if (!isUsableRouteReview(review, change.routeId)) {
        setNotice(routeReviewFailureMessage(review, lang));
        return;
      }
      setPendingAction({ kind: "approve", change, review });
    } catch (error) {
      setNotice(error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh" ? "浏览器预览无法审阅真实路由草稿。" : "Browser preview cannot review a real route draft.")
        : (lang === "zh" ? "桌面控制面未能生成路由审阅；未执行写入，请稍后重试。" : "The desktop control plane could not prepare the route review. Nothing was written; try again."));
    } finally {
      setReviewingChangeId("");
    }
  };

  const approve = async (action: Extract<PendingAction, { kind: "approve" }>) => {
    const { change, review } = action;
    try {
      let result: NativeActionResult;
      if (change.action === "apply_route_draft") {
        if (!change.routeId || !review || !isUsableRouteReview(review, change.routeId)) {
          setNotice(lang === "zh" ? "应用已阻止：请先审阅当前路由差异。" : "Apply was blocked: review the current route diff first.");
          return;
        }
        result = await invokeNative<NativeActionResult>("apply_route_profile_draft", {
          request: routeApplyRequest(review),
        });
      } else {
        result = await invokeNative<NativeActionResult>("save_app_settings", {
          section: "app",
          payload: {
            change_id: change.id,
            target: change.file,
            reason: change.reasonEn,
            diff: change.diff,
            approval: "approved-draft",
            scope: "d-token-owned-local-state",
          },
        });
      }
      if (!result.ok) {
        setNotice(change.action === "apply_route_draft"
          ? routeApplyFailureMessage(result.summary, lang)
          : (result.summary || (lang === "zh" ? "批准失败，变更仍保留在待处理列表中。" : "Approval failed; the change remains pending.")));
        return;
      }
      setNotice(change.action === "apply_route_draft"
        ? (lang === "zh"
            ? "路由草稿已应用并完成写后验证；事务已创建持久回滚点。"
            : "The route draft was applied and verified; the transaction created a durable rollback point.")
        : result.summary);
      setHiddenChanges((prev) => new Set(prev).add(change.id));
      await refreshDesktopData();
    } catch (error) {
      setNotice(error instanceof Error && error.message === "native_unavailable"
        ? (lang === "zh" ? "当前是浏览器预览，无法批准真实变更草稿。" : "Browser preview cannot approve a real change draft.")
        : (lang === "zh" ? "桌面控制面调用失败，未确认写入结果；请重新审阅后再试。" : "The desktop control-plane call failed and the write result is unconfirmed. Review again before retrying."));
    }
  };

  const reject = (id: string) => {
    setHiddenChanges((prev) => new Set(prev).add(id));
    setNotice(lang === "zh" ? "已从本地待处理列表隐藏；未写入任何外部配置。" : "Hidden from the local pending list; no external config was written.");
  };

  const rollback = async (id: string) => {
    try {
      const result = await invokeNative<NativeActionResult>("manage_backup", { action: "restore", id });
      if (!result.ok) {
        setNotice(result.summary || "回退失败，当前配置保持不变。");
        return;
      }
      setNotice(result.summary);
      await refreshDesktopData();
    } catch (error) {
      setNotice(error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，无法读取真实回滚点。"
        : `回滚计划读取失败：${String(error)}`);
    }
  };

  const confirmPendingAction = async () => {
    if (!pendingAction || !pendingConfirmation || !pendingConfirmation.allowed || isExecuting) {
      if (pendingConfirmation && !pendingConfirmation.allowed) {
        setNotice(pendingConfirmation.blockedReason);
      }
      return;
    }

    const action = pendingAction;
    setPendingAction(null);
    setIsExecuting(true);
    try {
      if (action.kind === "approve") {
        if (action.change.action === "apply_route_draft" && (!action.review || !action.change.routeId || !isUsableRouteReview(action.review, action.change.routeId))) {
          setNotice(lang === "zh" ? "应用已阻止：请先审阅当前路由差异。" : "Apply was blocked: review the current route diff first.");
          return;
        }
        await approve(action);
      } else if (action.kind === "reject") {
        reject(action.change.id);
      } else {
        await rollback(action.backup.id);
      }
    } finally {
      setIsExecuting(false);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center gap-3">
        <h1>{t("ch.title")}</h1>
        <DataSourceBadge
          mode={source === "desktop" ? "native" : source === "unavailable" || source === "stale" || source === "partial" ? "unavailable" : "preview"}
          label={source === "desktop" ? "真实桌面数据" : sourceLabel}
        />
        <Badge variant={source === "desktop" ? "default" : "secondary"}>{sourceLabel}</Badge>
      </div>
      {notice && <div className="glass-inset p-3 text-sm text-muted-foreground">{notice}</div>}
      {source === "browser-preview" && (
        <div className="rounded-md border border-amber-500/25 bg-amber-500/[0.06] px-4 py-3 text-sm text-amber-800 dark:text-amber-200">
          浏览器预览仅供查看，批准、拒绝和回退均不可执行。
        </div>
      )}
      {source === "unavailable" && (
        <div className="rounded-md border border-rose-500/25 bg-rose-500/[0.06] px-4 py-3 text-sm text-rose-700 dark:text-rose-200">
          无法读取桌面端真实变更数据。为保护本地配置，所有动作已锁定。
        </div>
      )}
      {source === "stale" && (
        <div className="rounded-md border border-amber-500/25 bg-amber-500/[0.06] px-4 py-3 text-sm text-amber-800 dark:text-amber-200">
          桌面数据已过期，不能作为当前配置证据。批准、拒绝和回退均已锁定。
        </div>
      )}
      {source === "partial" && (
        <div className="rounded-md border border-amber-500/25 bg-amber-500/[0.06] px-4 py-3 text-sm text-amber-800 dark:text-amber-200">
          桌面数据不完整，无法确认影响范围与恢复保障。批准、拒绝和回退均已锁定。
        </div>
      )}

      <Tabs defaultValue="pending">
        <TabsList className="flex-wrap">
          <TabsTrigger value="pending">{t("ch.tab.pending")}</TabsTrigger>
          <TabsTrigger value="diff">{t("ch.tab.diff")}</TabsTrigger>
          <TabsTrigger value="backup">{t("ch.tab.backup")}</TabsTrigger>
          <TabsTrigger value="ownership">{t("ch.tab.ownership")}</TabsTrigger>
        </TabsList>

        <TabsContent value="pending" className="mt-4 space-y-3">
          {pendingChanges.length === 0 && (
            <div className="glass-inset p-4 text-sm text-muted-foreground">
              {lang === "zh" ? "当前没有待处理变更" : "There are no pending changes"}
            </div>
          )}
          {pendingChanges.map((c) => {
            const approvalGate = getApprovalGate(source, c, verifiedBackups);
            const rejectGate = getRejectGate(source, c);
            return (
              <GlassCard key={c.id} className="p-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2"><FileCode className="size-4 text-muted-foreground" /><span className="font-mono text-sm">{c.file}</span></div>
                  <div className="flex items-center gap-2">
                    <Badge variant={c.risk === "low" ? "secondary" : "destructive"}>{t("ch.risk")}: {c.risk}</Badge>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!rejectGate.allowed}
                      title={rejectGate.allowed ? "先查看影响，再确认拒绝" : rejectGate.reason}
                      onClick={() => setPendingAction({ kind: "reject", change: c })}
                    >
                      <X className="size-3.5" />{t("ch.reject")}
                    </Button>
                    <Button
                      size="sm"
                      disabled={!approvalGate.allowed || Boolean(reviewingChangeId) || isExecuting}
                      title={approvalGate.allowed ? "先查看影响、备份与恢复方式" : approvalGate.reason}
                      onClick={() => void startApproval(c)}
                    >
                      {reviewingChangeId === c.id ? <RefreshCw className="size-3.5 animate-spin" /> : <Check className="size-3.5" />}
                      {reviewingChangeId === c.id
                        ? (lang === "zh" ? "正在生成审阅" : "Preparing review")
                        : t("ch.approve")}
                    </Button>
                  </div>
                </div>
                <div className="mt-1 text-sm text-muted-foreground">{lang === "zh" ? c.reason : c.reasonEn}</div>
                {!approvalGate.allowed && source === "desktop" && (
                  <div className="mt-2 text-xs text-amber-700 dark:text-amber-300">{approvalGate.reason}</div>
                )}
              </GlassCard>
            );
          })}
        </TabsContent>

        <TabsContent value="diff" className="mt-4 space-y-4">
          {pendingChanges.length === 0 && (
            <div className="glass-inset p-4 text-sm text-muted-foreground">
              {lang === "zh" ? "当前没有可审阅的配置差异" : "There are no configuration differences to review"}
            </div>
          )}
          {pendingChanges.map((c) => (
            <GlassCard key={c.id} className="overflow-hidden p-5">
              <div className="mb-3 flex items-center justify-between">
                <span className="font-mono text-sm">{c.file}</span>
                <Badge variant="outline"># dtoken managed</Badge>
              </div>
              <div className="grid gap-3 md:grid-cols-2">
                <div>
                  <div className="mb-1 text-xs text-rose-500">— before</div>
                  <pre className="overflow-x-auto rounded-lg border border-rose-500/15 bg-rose-500/[0.04] p-3 text-xs text-rose-700 dark:text-rose-300">{c.before}</pre>
                </div>
                <div>
                  <div className="mb-1 text-xs text-emerald-500">+ after</div>
                  <pre className="overflow-x-auto rounded-lg border border-emerald-500/15 bg-emerald-500/[0.04] p-3 text-xs text-emerald-700 dark:text-emerald-300">{c.after}</pre>
                </div>
              </div>
            </GlassCard>
          ))}
        </TabsContent>

        <TabsContent value="backup" className="mt-4">
          {backupRows.length === 0 && (
            <div className="glass-inset p-4 text-sm text-muted-foreground">
              {lang === "zh" ? "当前没有可用回滚点" : "There are no rollback points available"}
            </div>
          )}
          <div className="relative space-y-4 pl-6">
            <div className="absolute left-[9px] top-2 bottom-2 w-px bg-border" />
            {backupRows.map((b, i) => {
              const rollbackGate = getRollbackGate(source, b);
              return (
                <motion.div key={b.id} initial={{ opacity: 0, x: -8 }} animate={{ opacity: 1, x: 0 }} transition={{ delay: i * 0.06 }} className="relative">
                  <span className="absolute -left-6 top-3 size-[10px] rounded-full border-2 border-background bg-primary" />
                  <GlassCard className="p-4">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div>
                        <div>{b.time}</div>
                        <div className="text-xs text-muted-foreground">
                          {t("ch.affected")}: {b.files ?? "—"} {t("ch.files")} · {t("status.ports")} {b.ports.length > 0 ? b.ports.join(",") : "—"} · {b.procs ?? "—"} proc
                        </div>
                      </div>
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={!rollbackGate.allowed}
                        title={rollbackGate.allowed ? "先查看恢复影响，再确认回退" : rollbackGate.reason}
                        onClick={() => setPendingAction({ kind: "rollback", backup: b })}
                      >
                        <RotateCcw className="size-3.5" />{t("ch.rollback")}
                      </Button>
                    </div>
                    <div className="mt-3 flex flex-wrap items-center gap-3 text-xs">
                      <span className="text-muted-foreground">{t("ch.beforeDoctor")}:</span>
                      <StatusBadge status={b.before} label={b.before} />
                      <span className="text-muted-foreground">→ {t("ch.afterDoctor")}:</span>
                      <StatusBadge status={b.after} label={b.after} />
                      {!rollbackGate.allowed && source === "desktop" && (
                        <span className="text-amber-700 dark:text-amber-300">{rollbackGate.reason}</span>
                      )}
                    </div>
                  </GlassCard>
                </motion.div>
              );
            })}
          </div>
        </TabsContent>

        <TabsContent value="ownership" className="mt-4 grid gap-4 sm:grid-cols-2">
          {ownershipRows.length === 0 && (
            <div className="glass-inset p-4 text-sm text-muted-foreground sm:col-span-2">
              {lang === "zh" ? "尚未检测到可核验的配置归属路径" : "No verifiable configuration ownership paths were detected"}
            </div>
          )}
          {ownershipRows.map((o) => (
            <GlassCard key={o.id} className="p-5">
              <h3 className="mb-3 flex items-center gap-2"><Folder className="size-4 text-primary" />{t(o.labelKey)}</h3>
              <div className="space-y-1.5">
                {o.items.map((it) => <div key={it} className="glass-inset px-3 py-2 font-mono text-xs">{it}</div>)}
              </div>
            </GlassCard>
          ))}
        </TabsContent>
      </Tabs>

      <AlertDialog
        open={pendingAction !== null}
        onOpenChange={(open) => {
          if (!open && !isExecuting) setPendingAction(null);
        }}
      >
        {pendingConfirmation && (
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{pendingConfirmation.title}</AlertDialogTitle>
              <AlertDialogDescription asChild>
                <div className="space-y-4 text-left">
                  <p>{pendingConfirmation.summary}</p>
                  <div className="grid gap-3 rounded-md border border-border/70 bg-muted/30 p-4">
                    <ConfirmationRow label="影响范围" value={pendingConfirmation.impact} />
                    <ConfirmationRow label="备份保障" value={pendingConfirmation.backup} />
                    <ConfirmationRow label="恢复方式" value={pendingConfirmation.recovery} />
                  </div>
                  {pendingConfirmation.review && (
                    <RouteReviewEvidence review={pendingConfirmation.review} lang={lang} />
                  )}
                  {!pendingConfirmation.allowed && (
                    <p className="rounded-md border border-rose-500/25 bg-rose-500/[0.06] p-3 text-rose-700 dark:text-rose-200">
                      {pendingConfirmation.blockedReason}
                    </p>
                  )}
                </div>
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={isExecuting}>取消</AlertDialogCancel>
              <AlertDialogAction
                disabled={isExecuting || !pendingConfirmation.allowed}
                className={pendingAction?.kind === "rollback" ? "bg-destructive text-white hover:bg-destructive/90" : undefined}
                onClick={() => void confirmPendingAction()}
              >
                {pendingConfirmation.confirmLabel}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        )}
      </AlertDialog>
    </div>
  );
}

function ConfirmationRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[5rem_1fr] sm:gap-3">
      <strong className="text-xs font-medium text-foreground">{label}</strong>
      <span className="break-words text-xs leading-5 text-muted-foreground">{value}</span>
    </div>
  );
}

function RouteReviewEvidence({ review, lang }: { review: RouteDraftApplyReview; lang: "zh" | "en" }) {
  return (
    <div className="space-y-3 rounded-md border border-border/70 bg-muted/20 p-4 text-left">
      <div className="grid gap-3 sm:grid-cols-2">
        <ConfirmationRow label={lang === "zh" ? "目标文件" : "Target"} value={review.target} />
        <ConfirmationRow label={lang === "zh" ? "风险等级" : "Risk"} value={review.risk_level} />
      </div>
      <div>
        <div className="mb-1.5 text-xs font-medium text-foreground">{lang === "zh" ? "脱敏差异" : "Redacted diff"}</div>
        <pre className="max-h-56 overflow-auto whitespace-pre-wrap break-words rounded-md border border-border/60 bg-background/70 p-3 text-xs leading-5 text-foreground">{review.diff}</pre>
      </div>
      <div className="grid gap-2 text-[11px] text-muted-foreground">
        <ReviewHash label={lang === "zh" ? "应用前 SHA-256" : "Before SHA-256"} value={review.before_sha256} />
        <ReviewHash label={lang === "zh" ? "应用后 SHA-256" : "After SHA-256"} value={review.after_sha256} />
      </div>
    </div>
  );
}

function ReviewHash({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[132px_minmax(0,1fr)]">
      <span>{label}</span>
      <code className="break-all font-mono text-foreground/80">{value}</code>
    </div>
  );
}

type ChangeSource = "loading" | "desktop" | "browser-preview" | "unavailable" | "stale" | "partial";

type PendingAction =
  | { kind: "approve"; change: UiChange; review: RouteDraftApplyReview | null }
  | { kind: "reject"; change: UiChange }
  | { kind: "rollback"; backup: UiBackup };

type ActionGate = { allowed: boolean; reason: string };

type ActionConfirmation = {
  allowed: boolean;
  blockedReason: string;
  title: string;
  confirmLabel: string;
  summary: string;
  impact: string;
  backup: string;
  recovery: string;
  review?: RouteDraftApplyReview;
};

type UiChange = {
  id: string;
  routeId?: string;
  action?: "apply_route_draft" | "record_approval";
  file: string;
  reason: string;
  reasonEn: string;
  risk: "low" | "medium" | "high";
  before: string;
  after: string;
  diff: any[];
  safetyComplete: boolean;
};

type UiBackup = {
  id: string;
  time: string;
  files: number | null;
  ports: number[];
  procs: number | null;
  before: "healthy" | "warning" | "error";
  after: "healthy" | "warning" | "error";
  verified: boolean;
};

type UiOwnership = {
  id: string;
  labelKey: string;
  items: string[];
};

function sourceActionBlockedReason(source: ChangeSource): string {
  if (source === "browser-preview") return "浏览器预览没有本机变更证据，不能执行真实动作。";
  if (source === "loading") return "正在读取桌面端真实数据，请稍候。";
  if (source === "unavailable") return "桌面端真实数据不可用，动作已锁定。";
  if (source === "stale") return "桌面端数据已过期，不能作为当前状态证据。";
  if (source === "partial") return "桌面端数据不完整，无法确认动作影响。";
  return "";
}

function sourceFromDesktopPayload(payload: Record<string, any> | null): ChangeSource {
  const evidence = classifyEvidence({
    payload,
    status: "native",
    requireRuntimeIdentity: true,
    requireTransactionProvenance: true,
  });
  if (evidence === "verified") return "desktop";
  if (evidence === "preview") return "browser-preview";
  return evidence;
}

function getApprovalGate(source: ChangeSource, change: UiChange, verifiedBackups: UiBackup[]): ActionGate {
  if (source !== "desktop") return { allowed: false, reason: sourceActionBlockedReason(source) };
  if (!change.safetyComplete) return { allowed: false, reason: "变更详情不完整，已阻止批准。" };
  if (verifiedBackups.length === 0) {
    return { allowed: false, reason: "需要已验证的回滚点后才能批准此变更。" };
  }
  return { allowed: true, reason: "" };
}

function getRejectGate(source: ChangeSource, change: UiChange): ActionGate {
  if (source !== "desktop") return { allowed: false, reason: sourceActionBlockedReason(source) };
  if (!change.id || !change.safetyComplete) return { allowed: false, reason: "变更详情不完整，已阻止拒绝。" };
  return { allowed: true, reason: "" };
}

function getRollbackGate(source: ChangeSource, backup: UiBackup): ActionGate {
  if (source !== "desktop") return { allowed: false, reason: sourceActionBlockedReason(source) };
  if (!backup.verified) return { allowed: false, reason: "该回滚点尚未验证，已阻止恢复。" };
  if (!backup.id || !backup.time || backup.files === null || !Number.isFinite(backup.files) || backup.files < 1) {
    return { allowed: false, reason: "回滚点影响信息不完整，已阻止恢复。" };
  }
  return { allowed: true, reason: "" };
}

function buildConfirmation(
  action: PendingAction,
  source: ChangeSource,
  verifiedBackups: UiBackup[],
  lang: "zh" | "en",
): ActionConfirmation {
  if (action.kind === "approve") {
    const gate = getApprovalGate(source, action.change, verifiedBackups);
    const backup = verifiedBackups[0];
    return lang === "zh"
      ? {
          ...gate,
          blockedReason: gate.reason,
          title: action.review ? "审阅并批准路由" : "确认批准变更",
          confirmLabel: action.review ? "确认应用" : "确认批准",
          summary: action.review
            ? "以下内容由桌面控制面只读生成。确认时会提交本次审阅的前后 SHA-256；如草稿或活动配置变化，应用会被拒绝。"
            : "批准后将更新 d-token 本地状态；不会修改外部 Agent、Headroom、RTK 或 cc-switch 配置。",
          impact: action.review?.target ?? action.change.file,
          backup: backup
            ? `当前有已验证恢复点 ${backup.id} · ${backup.time}。本次预览未写文件；确认后事务会在写入前创建新的持久回滚点，并执行写后验证。`
            : "没有可验证的恢复点。",
          recovery: "如配置异常，可在“备份与回滚”中回退，再运行 Doctor 复核路由与服务状态。",
          review: action.review ?? undefined,
        }
      : {
          ...gate,
          blockedReason: gate.reason,
          title: action.review ? "Review and approve route" : "Confirm change approval",
          confirmLabel: action.review ? "Confirm apply" : "Confirm approval",
          summary: action.review
            ? "This review was prepared read-only by the desktop control plane. Confirmation submits its before/after SHA-256 pair; apply is rejected if the draft or active config changes."
            : "This updates d-token local state without changing external Agent, Headroom, RTK, or cc-switch configuration.",
          impact: action.review?.target ?? action.change.file,
          backup: backup
            ? `A verified recovery point exists: ${backup.id} · ${backup.time}. This preview writes nothing; confirmation creates a new durable rollback point before writing and verifies afterward.`
            : "No verifiable recovery point is available.",
          recovery: "If the route is unhealthy, restore it from Backup & Rollback, then run Doctor.",
          review: action.review ?? undefined,
        };
  }

  if (action.kind === "reject") {
    const gate = getRejectGate(source, action.change);
    const allowed = gate.allowed;
    const blockedReason = gate.reason;
    return lang === "zh"
      ? {
          allowed,
          blockedReason,
          title: "确认拒绝变更",
          confirmLabel: "确认拒绝",
          summary: "拒绝只会从本次待处理列表隐藏该草稿，不会写入或删除任何配置。",
          impact: action.change.file,
          backup: "此动作不改文件，因此无需创建备份。",
          recovery: "重新打开页面或重新载入桌面数据后，可再次查看该草稿。",
        }
      : {
          allowed,
          blockedReason,
          title: "Confirm rejection",
          confirmLabel: "Confirm rejection",
          summary: "Rejecting only hides this draft from the current pending list; no configuration is written or deleted.",
          impact: action.change.file,
          backup: "No backup is needed because this action does not change files.",
          recovery: "Reopen the page or reload desktop data to see the draft again.",
        };
  }

  const gate = getRollbackGate(source, action.backup);
  return lang === "zh"
    ? {
        ...gate,
        blockedReason: gate.reason,
        title: "确认回退",
        confirmLabel: "确认回退",
        summary: "回退会恢复 d-token 当前活动路由配置，并可能短暂影响经由该路由的新请求。",
        impact: `${action.backup.files ?? "未知数量"} 个文件 · d-token 当前活动路由配置`,
        backup: `使用已验证恢复点 ${action.backup.id} · ${action.backup.time}。`,
        recovery: "完成后将重新读取本地数据；请运行 Doctor 确认路由与服务状态。",
      }
    : {
        ...gate,
        blockedReason: gate.reason,
        title: "Confirm rollback",
        confirmLabel: "Confirm rollback",
        summary: "Rollback restores the active d-token route configuration and may briefly affect new requests using that route.",
        impact: `${action.backup.files ?? "unknown"} file(s) · active d-token route configuration`,
        backup: `Using verified recovery point ${action.backup.id} · ${action.backup.time}.`,
        recovery: "Desktop data reloads afterward; run Doctor to verify route and service health.",
      };
}

function buildChanges(data: Record<string, any> | null, source: string): UiChange[] {
  const drafts = Array.isArray(data?.routeDrafts) ? data.routeDrafts : [];
  const hasNativePayload = source === "desktop" || source === "stale" || source === "partial";
  if (hasNativePayload && drafts.length > 0) {
    return drafts.map((draft: any) => {
      const config = draft.config ?? {};
      const route = Array.isArray(config.routes) ? config.routes[0] : {};
      const provider = Array.isArray(config.providers) ? config.providers[0] : {};
      const diff = Array.isArray(draft.diff) ? draft.diff : [];
      const routeId = safeText(route?.id, safeText(provider?.id, "未提供 route"));
      const baseUrl = safeText(provider?.base_url, "—");
      const port = Number(route?.local_port);
      const portLabel = Number.isInteger(port) && port > 0 && port <= 65535 ? String(port) : "—";
      const before = diff
        .filter((item: any) => item.type === "ctx" || item.type === "remove")
        .map((item: any) => safeText(item.text, ""))
        .filter(Boolean)
        .join("\n");
      const after = diff
        .filter((item: any) => item.type === "ctx" || item.type === "add")
        .map((item: any) => safeText(item.text, ""))
        .filter(Boolean)
        .join("\n");
      const safetyComplete =
        typeof draft.path === "string" && draft.path.trim().length > 0 &&
        typeof route?.id === "string" && route.id.trim().length > 0 &&
        typeof provider?.base_url === "string" && provider.base_url.trim().length > 0 &&
        Number.isInteger(port) && port > 0 && port <= 65535 &&
        diff.length > 0;
      return {
        id: `draft-${routeId}`,
        routeId,
        action: "apply_route_draft",
        file: safeText(draft.path, "未提供草稿路径"),
        reason: `审阅 ${routeId}：${baseUrl} -> 127.0.0.1:${portLabel}`,
        reasonEn: `Review ${routeId}: ${baseUrl} -> 127.0.0.1:${portLabel}`,
        risk: normalizeRisk(draft.risk),
        before: before || "—",
        after: after || "—",
        diff,
        safetyComplete,
      };
    });
  }

  const rows = Array.isArray(data?.changes) ? data.changes : [];
  if (hasNativePayload && rows.length > 0) {
    return rows.map((row: any) => {
      const diff = Array.isArray(row.diff) ? row.diff : [];
      const before = diff.filter((item: any) => item.type === "ctx" || item.type === "remove").map((item: any) => safeText(item.text, "")).filter(Boolean).join("\n");
      const after = diff.filter((item: any) => item.type === "ctx" || item.type === "add").map((item: any) => safeText(item.text, "")).filter(Boolean).join("\n");
      return {
        id: safeText(row.id, safeText(row.target, "change")),
        action: "record_approval",
        file: safeText(row.target, "未提供变更目标"),
        reason: safeText(row.summary, "未提供变更摘要"),
        reasonEn: safeText(row.summary, "Change summary unavailable"),
        risk: normalizeRisk(row.risk),
        before: before || "—",
        after: after || "—",
        diff,
        safetyComplete:
          typeof row.id === "string" && row.id.trim().length > 0 &&
          typeof row.target === "string" && row.target.trim().length > 0 &&
          typeof row.summary === "string" && row.summary.trim().length > 0 &&
          diff.length > 0,
      };
    });
  }
  return [];
}

function buildBackups(data: Record<string, any> | null, source: string): UiBackup[] {
  const rows = Array.isArray(data?.backupPoints) ? data.backupPoints : [];
  const hasNativePayload = source === "desktop" || source === "stale" || source === "partial";
  if (hasNativePayload && rows.length > 0) {
    return rows.map((row: any) => {
      const verified = source === "desktop" && row.verified === true;
      const files = finiteNumber(row.files);
      const procs = finiteNumber(row.procs);
      const ports = Array.isArray(row.ports)
        ? (row.ports as unknown[])
            .map(finiteNumber)
            .filter((port): port is number => port !== null && Number.isInteger(port) && port > 0 && port <= 65535)
        : [];
      return {
        id: safeText(row.id, ""),
        time: safeText(row.when, ""),
        files,
        ports,
        procs,
        before: normalizeHealth(row.before),
        after: normalizeHealth(row.after),
        verified,
      };
    });
  }
  return [];
}

function buildOwnership(data: Record<string, any> | null, source: ChangeSource): UiOwnership[] {
  if (source !== "desktop" && source !== "stale" && source !== "partial") return [];
  const active = data?.active_config ?? data?.desktop_active_config;
  const drafts = Array.isArray(data?.routeDrafts) ? data.routeDrafts : [];
  const items = [active?.path, active?.config_path, ...drafts.map((draft: any) => draft?.path)]
    .filter((value): value is string => typeof value === "string" && value.trim().length > 0);
  const uniqueItems = [...new Set(items)];
  return uniqueItems.length > 0
    ? [{ id: "dtoken-owned-paths", labelKey: "ch.own.dtoken", items: uniqueItems }]
    : [];
}

function finiteNumber(value: unknown): number | null {
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function normalizeHealth(value: unknown): UiBackup["before"] {
  return value === "healthy" || value === "warning" || value === "error" ? value : "warning";
}

function normalizeRisk(value: unknown): "low" | "medium" | "high" {
  const text = safeText(value, "low");
  return text === "high" || text === "medium" || text === "low" ? text : "low";
}

function safeText(value: unknown, fallback: string) {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}
