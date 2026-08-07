/**
 * 视觉压缩（Beta）工作台：一键启用 + 高级配置 + 状态与预览。
 * 对齐「安装与连接」信息架构，而不是工程 checklist 向导。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ChevronDown,
  Eye,
  RefreshCw,
  Rocket,
  Settings2,
  Shield,
  Activity,
  Cable,
} from "lucide-react";
import { MetricStrip, Surface } from "../../components/common/Surface";
import { DataSourceBadge } from "../../components/common/DataSourceBadge";
import { EmptyState } from "../../components/common/EmptyState";
import { OperationDialog } from "../../components/common/OperationDialog";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { Switch } from "../../components/ui/switch";
import { Input } from "../../components/ui/input";
import { Textarea } from "../../components/ui/textarea";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../../components/ui/tabs";
import { useI18n, useT } from "../i18n";
import { createOperationRequestId, useNotifications } from "../notifications";
import { presentOperationFailure } from "../userCopy";
import {
  applyOpticalConfigChange,
  loadOpticalConfigEditor,
  loadOpticalObservability,
  previewOpticalConfigChange,
  previewOpticalPlan,
  restoreOpticalConfigBackup,
} from "../optical/client";
import {
  opticalDraftsEqual,
  summarizeOpticalDraft,
  type OpticalConfigDraft as DraftModel,
  type OpticalSafeMode,
} from "../optical/configDraft";
import {
  displayCount,
  formatOpticalMode,
  formatOpticalStage,
  formatOpticalStatus,
  formatTimestampMs,
} from "../optical/format";
import type {
  OpticalConfigApplyResult,
  OpticalConfigEditorSnapshot,
  OpticalConfigReview,
  OpticalObservabilitySnapshot,
  OpticalPlanSummary,
} from "../optical/types";
import { OPTICAL_SAMPLE_BODY } from "../optical/types";
import {
  applyHybridAgentIntegration,
  getHybridAgentIntegrationTransaction,
  loadHybridAgentIntegrations,
  previewHybridAgentIntegration,
  rollbackHybridAgentIntegration,
  startHybridService,
  verifyHybridAgentIntegration,
  type HybridAgentDiscovery,
  type HybridAgentIntegrationDraft,
  type HybridAgentPreview,
  type HybridAgentReceipt,
  type HybridAgentTransactionState,
  type HybridAgentVerification,
} from "../optical/agentIntegration";
import { cn } from "../../components/ui/utils";

const REFRESH_MS = 20_000;
const PREVIEW_MAX_CHARS = 512 * 1024;
const TAB_KEY = "d-token.visual.workbench-tab";
const AGENT_TRANSACTION_KEY = "d-token.hybrid.agent-transaction";
const TABS = ["one-click", "advanced", "status"] as const;
type WorkbenchTab = (typeof TABS)[number];

type PageState = "loading" | "runtime" | "partial" | "unavailable";

function verifiedAgentState(result: HybridAgentVerification): HybridAgentTransactionState {
  if (result.ok && result.state === "verified") return "verified";
  if (result.state === "partial") return "partial";
  if (result.state === "changed") return "changed";
  return "failed";
}

function readTab(): WorkbenchTab {
  if (typeof window === "undefined") return "one-click";
  try {
    const saved = window.sessionStorage.getItem(TAB_KEY);
    if (saved && (TABS as readonly string[]).includes(saved)) return saved as WorkbenchTab;
  } catch {
    /* ignore */
  }
  return "one-click";
}

function snapshotState(snapshot: OpticalObservabilitySnapshot | null, loading: boolean): PageState {
  if (!snapshot && loading) return "loading";
  if (!snapshot) return "unavailable";
  if (snapshot.complete && snapshot.ok) return "runtime";
  if (snapshot.partial || snapshot.ok) return "partial";
  return "unavailable";
}

function isUsefulSnapshot(snapshot: OpticalObservabilitySnapshot): boolean {
  return Boolean(snapshot.ok || snapshot.partial || snapshot.complete);
}

function shortHash(value?: string): string {
  if (!value) return "—";
  return value.length > 12 ? `${value.slice(0, 8)}…${value.slice(-4)}` : value;
}

/** One-click safe defaults: global shadow on; every known route shadow + tool_result. */
function buildOneClickEnableDraft(editor: OpticalConfigEditorSnapshot): DraftModel {
  return {
    global: { enabled: true, default_mode: "shadow_preview" },
    routes: editor.routes.map((route) => ({
      route_id: route.route_id,
      enabled: true,
      mode: "shadow_preview" as const,
      allow_tool_result: true,
      allow_history: false,
      confirmed_experimental_lossy: false,
    })),
  };
}

function buildOneClickDisableDraft(editor: OpticalConfigEditorSnapshot): DraftModel {
  return {
    global: { enabled: false, default_mode: "shadow_preview" },
    routes: editor.routes.map((route) => ({
      route_id: route.route_id,
      enabled: false,
      mode: "off" as const,
      allow_tool_result: false,
      allow_history: false,
      confirmed_experimental_lossy: false,
    })),
  };
}

export function VisualCompression() {
  const t = useT();
  const { lang } = useI18n();
  const { addNotification } = useNotifications();
  const [tab, setTabState] = useState<WorkbenchTab>(readTab);
  const setTab = (value: string) => {
    if (!(TABS as readonly string[]).includes(value)) return;
    const next = value as WorkbenchTab;
    setTabState(next);
    try {
      window.sessionStorage.setItem(TAB_KEY, next);
    } catch {
      /* ignore */
    }
  };

  const [snapshot, setSnapshot] = useState<OpticalObservabilitySnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");
  const [staleNotice, setStaleNotice] = useState("");

  const [editor, setEditor] = useState<OpticalConfigEditorSnapshot | null>(null);
  const [baselineDraft, setBaselineDraft] = useState<DraftModel | null>(null);
  const [draft, setDraft] = useState<DraftModel | null>(null);
  const [editorError, setEditorError] = useState("");
  const [editorLoading, setEditorLoading] = useState(true);

  const [review, setReview] = useState<OpticalConfigReview | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmKind, setConfirmKind] = useState<"one-click-enable" | "one-click-disable" | "advanced">("advanced");
  const [pendingDraft, setPendingDraft] = useState<DraftModel | null>(null);
  const [configBusy, setConfigBusy] = useState<"preview" | "apply" | "rollback" | "">("");
  const [configNotice, setConfigNotice] = useState("");
  const [lastApply, setLastApply] = useState<OpticalConfigApplyResult | null>(null);

  const [agentDiscovery, setAgentDiscovery] = useState<HybridAgentDiscovery | null>(null);
  const [agentDraft, setAgentDraft] = useState<HybridAgentIntegrationDraft>({
    agent_ids: [],
    scope: "global",
    project_root: null,
    install_instructions: true,
    install_hook_or_plugin: true,
    install_skill: true,
    install_mcp: false,
  });
  const [agentReview, setAgentReview] = useState<HybridAgentPreview | null>(null);
  const [agentReceipt, setAgentReceipt] = useState<HybridAgentReceipt | null>(null);
  const [agentTransactionState, setAgentTransactionState] = useState<HybridAgentTransactionState | null>(null);
  const [agentConfirmOpen, setAgentConfirmOpen] = useState(false);
  const [agentBusy, setAgentBusy] = useState<"load" | "preview" | "apply" | "verify" | "rollback" | "service" | "">("load");
  const [agentNotice, setAgentNotice] = useState("");

  const [previewOpen, setPreviewOpen] = useState(false);
  const [previewText, setPreviewText] = useState(() => JSON.stringify(OPTICAL_SAMPLE_BODY, null, 2));
  const [previewRouteId, setPreviewRouteId] = useState("");
  const [previewRunning, setPreviewRunning] = useState(false);
  const [previewError, setPreviewError] = useState("");
  const [previewResult, setPreviewResult] = useState<OpticalPlanSummary | null>(null);

  const hasEvidenceRef = useRef(false);
  const mountedRef = useRef(true);
  const refreshSeqRef = useRef(0);
  const previewSeqRef = useRef(0);
  const lastPollRef = useRef(0);

  const loadEditor = useCallback(async () => {
    setEditorLoading(true);
    try {
      const next = await loadOpticalConfigEditor();
      if (!mountedRef.current) return;
      setEditor(next);
      setBaselineDraft(next.draft);
      setDraft(next.draft);
      setEditorError(
        next.ok
          ? ""
          : presentOperationFailure("visual", next.error, lang).message,
      );
      setReview(null);
    } catch (err) {
      if (!mountedRef.current) return;
      setEditorError(presentOperationFailure("visual", err, lang).message);
    } finally {
      if (mountedRef.current) setEditorLoading(false);
    }
  }, [lang]);

  const loadAgentIntegration = useCallback(async (initializeDraft = false) => {
    setAgentBusy((current) => current || "load");
    try {
      let transactionNotice = "";
      const next = await loadHybridAgentIntegrations();
      if (!mountedRef.current) return;
      setAgentDiscovery(next);
      const transactionId = window.localStorage.getItem(AGENT_TRANSACTION_KEY);
      if (transactionId) {
        const transactionResult = await getHybridAgentIntegrationTransaction(transactionId);
        const transaction = transactionResult.transaction;
        if (mountedRef.current && !transaction) {
          setAgentTransactionState("failed");
          setAgentReceipt(null);
          transactionNotice = transactionResult.error
            ? presentOperationFailure("visual", transactionResult.error, lang).message
            : (lang === "zh" ? "无法读取已保存的 Agent 接入事务证据。" : "Could not read the saved Agent integration evidence.");
        } else if (mountedRef.current && transaction) {
          if (transaction.state === "rolled_back") {
            setAgentTransactionState("rolled_back");
            setAgentReceipt(null);
          } else {
            // A persisted "verified" state is historical evidence only. Re-check the
            // planned hashes before presenting it after an App restart.
            setAgentTransactionState("applied");
            setAgentReceipt({
              transaction_id: transaction.transaction_id,
              review_digest: transaction.review_digest,
              rollback_ids: transaction.rollback_ids,
              created_directories: transaction.created_directories,
              verified_targets: [],
              trust_required: transaction.targets.some((target) => target.trust_required),
              new_session_required: transaction.targets.some((target) => target.new_session_required),
            });
            try {
              const verification = await verifyHybridAgentIntegration(transaction.transaction_id);
              if (mountedRef.current) {
                setAgentTransactionState(verifiedAgentState(verification));
              }
            } catch {
              if (mountedRef.current) setAgentTransactionState("failed");
            }
          }
        }
      }
      if (initializeDraft) {
        const installed = next.agents
          .filter((agent) => agent.installed && agent.integration_supported !== false)
          .map((agent) => agent.id);
        setAgentDraft({
          agent_ids: installed,
          scope: next.defaults.scope,
          project_root: next.defaults.project_root ?? null,
          install_instructions: next.defaults.install_instructions,
          install_hook_or_plugin: next.defaults.install_hook_or_plugin,
          install_skill: next.defaults.install_skill,
          install_mcp: next.defaults.install_mcp,
        });
      }
      setAgentNotice(transactionNotice || (next.bridge.available ? "" : next.bridge.error
        ? presentOperationFailure("visual", next.bridge.error, lang).message
        : (lang === "zh" ? "Agent bridge 尚未构建。" : "Agent bridge is not built yet.")));
    } catch (err) {
      if (!mountedRef.current) return;
      setAgentReceipt(null);
      setAgentTransactionState(window.localStorage.getItem(AGENT_TRANSACTION_KEY) ? "failed" : null);
      setAgentNotice(presentOperationFailure("visual", err, lang).message);
    } finally {
      if (mountedRef.current) setAgentBusy((current) => current === "load" ? "" : current);
    }
  }, [lang]);

  const refresh = useCallback(async (mode: "initial" | "manual" | "poll" | "action" = "manual") => {
    const seq = ++refreshSeqRef.current;
    const requestId = mode === "manual" ? createOperationRequestId("optical-refresh") : "";
    if (mode === "initial" && !hasEvidenceRef.current) setLoading(true);
    if (mode === "manual") setRefreshing(true);
    try {
      const next = await loadOpticalObservability();
      if (!mountedRef.current || seq !== refreshSeqRef.current) return;
      if (isUsefulSnapshot(next)) {
        setSnapshot(next);
        setError(next.error ? presentOperationFailure("visual", next.error, lang).message : "");
        setStaleNotice("");
        hasEvidenceRef.current = true;
        if (mode === "manual") addNotification({ dedupeKey: "optical-refresh", level: "success", title: lang === "zh" ? "视觉压缩状态已更新" : "Visual compression status refreshed", message: lang === "zh" ? "状态、能力、生命周期与近期事件已重新读取。" : "Status, capabilities, lifecycle, and recent events were refreshed.", receipt: { requestId, targetId: "optical-observability" } });
        return;
      }
      if (mode !== "initial" && hasEvidenceRef.current) {
        setStaleNotice(
          lang === "zh"
            ? "最新刷新未拿到完整状态，仍显示上一份可用证据。"
            : "Latest refresh was incomplete; keeping the last good evidence.",
        );
        const failure = next.error ? presentOperationFailure("visual", next.error, lang) : undefined;
        setError(failure?.message ?? "");
        if (mode === "manual") addNotification({ dedupeKey: "optical-refresh-partial", level: "warning", title: lang === "zh" ? "视觉压缩刷新不完整" : "Visual compression refresh incomplete", message: lang === "zh" ? "仍显示上一份可验证证据。" : "The last verified evidence remains visible.", details: failure?.details, receipt: { requestId, targetId: "optical-observability" } });
        return;
      }
      setSnapshot(next);
      setError(next.error ? presentOperationFailure("visual", next.error, lang).message : "");
      setStaleNotice("");
      hasEvidenceRef.current = true;
    } catch (err) {
      if (!mountedRef.current || seq !== refreshSeqRef.current) return;
      const failure = presentOperationFailure("visual", err, lang);
      const display = failure.message;
      if (mode !== "initial" && hasEvidenceRef.current) {
        setStaleNotice(
          lang === "zh"
            ? "刷新失败，仍显示上一份可用证据。"
            : "Refresh failed; keeping the last good evidence.",
        );
        setError(display);
        if (mode === "manual") addNotification({ dedupeKey: "optical-refresh-failed", level: "error", title: lang === "zh" ? "视觉压缩刷新失败" : "Visual compression refresh failed", message: lang === "zh" ? "仍显示上一份可验证证据。" : "The last verified evidence remains visible.", details: failure.details, receipt: { requestId, targetId: "optical-observability" } });
        return;
      }
      setError(display);
      setStaleNotice("");
      if (!hasEvidenceRef.current) setSnapshot(null);
      if (mode === "manual") addNotification({ dedupeKey: "optical-refresh-error", level: "error", title: lang === "zh" ? "视觉压缩刷新失败" : "Visual compression refresh failed", message: display, receipt: { requestId, targetId: "optical-observability" } });
    } finally {
      if (!mountedRef.current || seq !== refreshSeqRef.current) return;
      setLoading(false);
      setRefreshing(false);
    }
  }, [addNotification, lang]);

  useEffect(() => {
    mountedRef.current = true;
    void refresh("initial");
    void loadEditor();
    void loadAgentIntegration(true);
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "visible") return;
      lastPollRef.current = Date.now();
      void refresh("poll");
    }, REFRESH_MS);
    const onFocus = () => {
      if (document.visibilityState !== "visible") return;
      // Skip a focus-triggered poll when the interval already polled recently.
      if (Date.now() - lastPollRef.current < REFRESH_MS - 1_000) return;
      lastPollRef.current = Date.now();
      void refresh("poll");
    };
    window.addEventListener("focus", onFocus);
    return () => {
      mountedRef.current = false;
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [loadAgentIntegration, loadEditor, refresh]);

  const state = snapshotState(snapshot, loading);
  const status = snapshot?.status.data;
  const capabilities = snapshot?.capabilities.data;
  const lifecycle = snapshot?.lifecycle.data;
  const events = snapshot?.events.data ?? [];
  const statusOk = Boolean(snapshot?.status.ok && status);
  const capabilitiesOk = Boolean(snapshot?.capabilities.ok && capabilities);
  const lifecycleOk = Boolean(snapshot?.lifecycle.ok && lifecycle);
  const eventsOk = Boolean(snapshot?.events.ok && Array.isArray(snapshot.events.data));
  const dataSourceMode =
    state === "runtime" || state === "partial"
      ? "runtime"
      : state === "loading"
        ? "preview"
        : "unavailable";

  const dirty = Boolean(draft && baselineDraft && !opticalDraftsEqual(draft, baselineDraft));
  const canConfigure = Boolean(editor?.ok && editor.active_config_exists && editor.route_count > 0);
  // Runtime status is the single source of truth for "enabled". The config draft is
  // only the editing baseline; a written-but-unverified config must not read as enabled.
  const runtimeEnabled = Boolean(statusOk && status?.enabled);
  const oneClickPhase = !editor && editorLoading
    ? "loading"
    : !canConfigure
      ? "blocked"
      : runtimeEnabled
        ? "enabled"
        : "ready";

  const workbenchTabs = [
    {
      value: "one-click" as const,
      title: t("visual.tab.oneClick"),
      description: t("visual.tab.oneClickDesc"),
      icon: Rocket,
    },
    {
      value: "advanced" as const,
      title: t("visual.tab.advanced"),
      description: t("visual.tab.advancedDesc"),
      icon: Settings2,
    },
    {
      value: "status" as const,
      title: t("visual.tab.status"),
      description: t("visual.tab.statusDesc"),
      icon: Activity,
    },
  ];

  const beginApply = async (nextDraft: DraftModel, kind: typeof confirmKind) => {
    const requestId = createOperationRequestId("optical-config-preview");
    setConfigBusy("preview");
    setConfigNotice("");
    setPendingDraft(nextDraft);
    setConfirmKind(kind);
    try {
      const next = await previewOpticalConfigChange(nextDraft);
      if (!mountedRef.current) return;
      setReview(next);
      if (!next.ok) {
        const failure = presentOperationFailure("visual", next.error, lang);
        setConfigNotice(failure.message);
        addNotification({ dedupeKey: "optical-config-preview-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "optical-config" } });
        return;
      }
      setConfirmOpen(true);
      addNotification({ dedupeKey: `optical-config-preview-${next.after_sha256}`, level: "info", title: lang === "zh" ? "视觉压缩变更已准备" : "Visual compression changes are ready", message: lang === "zh" ? "请查看脱敏差异；确认前不会写入配置。" : "View the redacted differences; no configuration is written before confirmation.", receipt: { requestId, targetId: "optical-config" } });
    } catch (err) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("visual", err, lang);
      setConfigNotice(failure.message);
      addNotification({ dedupeKey: "optical-config-preview-error", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "optical-config" } });
    } finally {
      if (mountedRef.current) setConfigBusy("");
    }
  };

  const runConfigApply = async () => {
    if (!pendingDraft || !review?.ok || !review.before_sha256 || !review.after_sha256) return;
    setConfigBusy("apply");
    setConfigNotice("");
    const requestId = createOperationRequestId("optical-config-apply");
    try {
      const result = await applyOpticalConfigChange(
        pendingDraft,
        { before: review.before_sha256, after: review.after_sha256 },
        true,
      );
      if (!mountedRef.current) return;
      setLastApply(result);
      if (!result.ok) {
        if (result.config_applied === true) {
          // Written to disk, but the runtime did not verify it (restart/verify failed).
          // Close the stale confirm dialog, keep the rollback entry, and say exactly
          // that — do not present a generic failure.
          const unverified = t("visual.config.writeUnverified");
          setConfigNotice(unverified);
          addNotification({ dedupeKey: `optical-config-apply-unverified-${result.rollback_id ?? "done"}`, level: "warning", title: lang === "zh" ? "配置已写入，服务未验证" : "Config written, service not verified", message: unverified, persistent: true, receipt: { requestId, rollbackId: result.rollback_id ?? undefined, targetId: "optical-config" } });
          setConfirmOpen(false);
          setReview(null);
          setPendingDraft(null);
          await loadEditor();
          await refresh("action");
        } else {
          const failure = presentOperationFailure("visual", result.error || result.summary, lang);
          setConfigNotice(failure.message);
          addNotification({ dedupeKey: "optical-config-apply-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "optical-config" } });
        }
        return;
      }
      setConfirmOpen(false);
      if (result.runtime_verified === false) {
        // Config is on disk but the service has not confirmed it yet — keep the
        // rollback entry visible instead of claiming a fully verified apply.
        const unverified = t("visual.config.writeUnverified");
        setConfigNotice(unverified);
        addNotification({ dedupeKey: `optical-config-applied-unverified-${result.rollback_id ?? "done"}`, level: "warning", title: lang === "zh" ? "配置已写入，服务未验证" : "Config written, service not verified", message: unverified, persistent: true, receipt: { requestId, rollbackId: result.rollback_id ?? undefined, targetId: "optical-config" } });
      } else {
        const applied = t("visual.config.applied");
        setConfigNotice(applied);
        addNotification({ dedupeKey: `optical-config-applied-${result.rollback_id ?? "done"}`, level: "success", title: lang === "zh" ? "视觉压缩配置已应用" : "Visual compression config applied", message: applied, persistent: true, receipt: { requestId, rollbackId: result.rollback_id ?? undefined, targetId: "optical-config" } });
      }
      setReview(null);
      setPendingDraft(null);
      await loadEditor();
      await refresh("action");
    } catch (err) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("visual", err, lang);
      setConfigNotice(failure.message);
      addNotification({ dedupeKey: "optical-config-apply-error", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "optical-config" } });
    } finally {
      if (mountedRef.current) setConfigBusy("");
    }
  };

  const runRollback = async () => {
    if (!lastApply?.rollback_id) return;
    setConfigBusy("rollback");
    setConfigNotice("");
    const requestId = createOperationRequestId("optical-config-rollback");
    try {
      const result = await restoreOpticalConfigBackup(lastApply.rollback_id);
      if (!mountedRef.current) return;
      if (!result.ok) {
        const failure = presentOperationFailure("visual", result.summary, lang);
        setConfigNotice(failure.message);
        addNotification({ dedupeKey: "optical-config-rollback-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, rollbackId: lastApply.rollback_id, targetId: "optical-config" } });
        return;
      }
      const rolledBack = t("visual.config.rollbackDone");
      setConfigNotice(rolledBack);
      addNotification({ dedupeKey: `optical-config-rollback-${lastApply.rollback_id}`, level: "success", title: lang === "zh" ? "视觉压缩配置已回退" : "Visual compression config rolled back", message: rolledBack, persistent: true, receipt: { requestId, rollbackId: lastApply.rollback_id, targetId: "optical-config" } });
      setLastApply(null);
      await loadEditor();
      await refresh("action");
    } catch (err) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("visual", err, lang);
      setConfigNotice(failure.message);
      addNotification({ dedupeKey: "optical-config-rollback-error", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, rollbackId: lastApply.rollback_id, targetId: "optical-config" } });
    } finally {
      if (mountedRef.current) setConfigBusy("");
    }
  };

  const toggleAgent = (agentId: HybridAgentIntegrationDraft["agent_ids"][number], enabled: boolean) => {
    setAgentDraft((current) => ({
      ...current,
      agent_ids: enabled
        ? Array.from(new Set([...current.agent_ids, agentId]))
        : current.agent_ids.filter((id) => id !== agentId),
    }));
    setAgentReview(null);
    setAgentNotice("");
  };

  const beginAgentIntegration = async () => {
    if (agentDraft.agent_ids.length === 0) {
      setAgentNotice(lang === "zh" ? "请至少选择一个已安装的 Agent。" : "Select at least one installed Agent.");
      return;
    }
    if (agentDraft.scope === "project" && !agentDraft.project_root?.trim()) {
      setAgentNotice(lang === "zh" ? "项目作用域需要绝对项目路径。" : "Project scope requires an absolute project path.");
      return;
    }
    setAgentBusy("preview");
    setAgentNotice("");
    const requestId = createOperationRequestId("optical-agent-preview");
    try {
      const next = await previewHybridAgentIntegration(agentDraft);
      if (!mountedRef.current) return;
      setAgentReview(next);
      if (!next.ok || !next.plan) {
        const failure = presentOperationFailure("visual", next.error, lang);
        setAgentNotice(failure.message);
        addNotification({ dedupeKey: "optical-agent-preview-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: agentDraft.agent_ids.join(",") } });
        return;
      }
      setAgentConfirmOpen(true);
      addNotification({ dedupeKey: `optical-agent-preview-${next.plan.review_digest}`, level: "info", title: lang === "zh" ? "Agent 接入变更已准备" : "Agent integration changes are ready", message: lang === "zh" ? "请查看逐文件差异；确认前不会写入 Agent 配置。" : "View each file difference; no Agent configuration is written before confirmation.", receipt: { requestId, targetId: agentDraft.agent_ids.join(",") } });
    } catch (err) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("visual", err, lang);
      setAgentNotice(failure.message);
      addNotification({ dedupeKey: "optical-agent-preview-error", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: agentDraft.agent_ids.join(",") } });
    } finally {
      if (mountedRef.current) setAgentBusy("");
    }
  };

  const runAgentIntegration = async () => {
    const plan = agentReview?.plan;
    if (!plan) return;
    setAgentBusy("apply");
    setAgentNotice("");
    const requestId = createOperationRequestId("optical-agent-apply");
    try {
      const result = await applyHybridAgentIntegration(agentDraft, plan.review_digest);
      if (!mountedRef.current) return;
      if (!result.ok || !result.receipt) {
        const failure = presentOperationFailure("visual", result.error || result.summary, lang);
        setAgentNotice(failure.message);
        addNotification({ dedupeKey: "optical-agent-apply-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: agentDraft.agent_ids.join(",") } });
        return;
      }
      setAgentReceipt(result.receipt);
      window.localStorage.setItem(AGENT_TRANSACTION_KEY, result.receipt.transaction_id);
      setAgentTransactionState("applied");
      setAgentConfirmOpen(false);
      setAgentBusy("verify");
      const verification = await verifyHybridAgentIntegration(result.receipt.transaction_id);
      if (!mountedRef.current) return;
      const verified = verification.ok && verification.state === "verified";
      setAgentTransactionState(verifiedAgentState(verification));
      setAgentNotice([
        verified
          ? lang === "zh" ? "文件校验通过。请新建 Agent 会话；Codex Hook 还需在 /hooks 中信任。" : "Files verified. Start a new Agent session; trust the Codex Hook in /hooks."
          : lang === "zh" ? "写入完成，但验证未通过；可立即回退。" : "Writing completed, but verification did not pass; rollback is available.",
      ].filter(Boolean).join(" "));
      addNotification({ dedupeKey: `optical-agent-apply-${result.receipt.transaction_id}`, level: verified ? "success" : "warning", title: verified ? (lang === "zh" ? "Agent 接入已验证" : "Agent integration verified") : (lang === "zh" ? "Agent 接入需要处理" : "Agent integration needs attention"), message: verified ? (lang === "zh" ? "文件校验通过；请新建 Agent 会话，并按提示信任 Codex Hook。" : "File verification passed; start a new Agent session and trust the Codex Hook when prompted.") : (lang === "zh" ? "写入完成但验证未通过，可立即回退。" : "Writing completed but verification did not pass; rollback is available."), persistent: true, receipt: { requestId, transactionId: result.receipt.transaction_id, targetId: agentDraft.agent_ids.join(",") } });
      await loadAgentIntegration(false);
    } catch (err) {
      if (!mountedRef.current) return;
      setAgentTransactionState((current) => current === "applied" ? "failed" : current);
      const failure = presentOperationFailure("visual", err, lang);
      setAgentNotice(failure.message);
      addNotification({ dedupeKey: "optical-agent-apply-error", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: agentDraft.agent_ids.join(",") } });
    } finally {
      if (mountedRef.current) setAgentBusy("");
    }
  };

  const runAgentRollback = async () => {
    if (!agentReceipt?.transaction_id) return;
    setAgentBusy("rollback");
    setAgentNotice("");
    const requestId = createOperationRequestId("optical-agent-rollback");
    try {
      const result = await rollbackHybridAgentIntegration(agentReceipt.transaction_id);
      if (!mountedRef.current) return;
      const success = lang === "zh" ? "接入前状态已恢复。" : "The pre-integration state was restored.";
      const failure = presentOperationFailure("visual", result.error || result.summary, lang);
      setAgentNotice(result.ok ? success : failure.message);
      addNotification({ dedupeKey: `optical-agent-rollback-${agentReceipt.transaction_id}`, level: result.ok ? "success" : "error", title: result.ok ? (lang === "zh" ? "Agent 接入已回退" : "Agent integration rolled back") : failure.title, message: result.ok ? success : failure.message, details: result.ok ? undefined : failure.details, persistent: true, receipt: { requestId, transactionId: agentReceipt.transaction_id, targetId: agentDraft.agent_ids.join(",") } });
      if (result.ok) {
        setAgentTransactionState("rolled_back");
        window.localStorage.removeItem(AGENT_TRANSACTION_KEY);
        setAgentReceipt(null);
        setAgentReview(null);
      }
      await loadAgentIntegration(false);
    } catch (err) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("visual", err, lang);
      setAgentNotice(failure.message);
      addNotification({ dedupeKey: "optical-agent-rollback-error", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, transactionId: agentReceipt.transaction_id, targetId: agentDraft.agent_ids.join(",") } });
    } finally {
      if (mountedRef.current) setAgentBusy("");
    }
  };

  const runServiceStart = async () => {
    setAgentBusy("service");
    setAgentNotice("");
    const requestId = createOperationRequestId("optical-service-start");
    try {
      const result = await startHybridService();
      if (!mountedRef.current) return;
      const success = lang === "zh" ? "本地服务已启动；请查看当前状态。" : "The local service started. Check the current status.";
      const failure = presentOperationFailure("service", result.summary, lang);
      setAgentNotice(result.ok ? success : failure.message);
      addNotification({ dedupeKey: "optical-service-started", level: result.ok ? "success" : "warning", title: result.ok ? (lang === "zh" ? "本地服务已启动" : "Local service started") : failure.title, message: result.ok ? success : failure.message, details: result.ok ? undefined : failure.details, receipt: { requestId, targetId: "local-service" } });
      await Promise.all([loadAgentIntegration(false), loadEditor(), refresh("action")]);
    } catch (err) {
      if (!mountedRef.current) return;
      const failure = presentOperationFailure("service", err, lang);
      setAgentNotice(failure.message);
      addNotification({ dedupeKey: "optical-service-start-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "local-service" } });
    } finally {
      if (mountedRef.current) setAgentBusy("");
    }
  };

  const updateGlobal = (patch: Partial<DraftModel["global"]>) => {
    setDraft((current) => {
      if (!current) return current;
      return {
        ...current,
        global: {
          ...current.global,
          ...patch,
          default_mode: (patch.default_mode ?? current.global.default_mode) as OpticalSafeMode,
        },
      };
    });
    setReview(null);
    setConfigNotice("");
  };

  const updateRoute = (routeId: string, patch: Partial<DraftModel["routes"][number]>) => {
    setDraft((current) => {
      if (!current) return current;
      return {
        ...current,
        routes: current.routes.map((route) =>
          route.route_id === routeId
            ? {
                ...route,
                ...patch,
                mode: (patch.mode ?? route.mode) as OpticalSafeMode,
              }
            : route,
        ),
      };
    });
    setReview(null);
    setConfigNotice("");
  };

  const runPreview = async () => {
    const seq = ++previewSeqRef.current;
    const requestId = createOperationRequestId("optical-shadow-preview");
    setPreviewError("");
    setPreviewResult(null);
    if (previewText.length > PREVIEW_MAX_CHARS) {
      const failure = t("visual.preview.tooLarge");
      setPreviewError(failure);
      addNotification({ dedupeKey: "optical-shadow-preview-too-large", level: "warning", title: lang === "zh" ? "Shadow 预览已阻止" : "Shadow preview blocked", message: failure, receipt: { requestId, targetId: "optical-shadow-preview" } });
      return;
    }
    let body: unknown;
    try {
      body = JSON.parse(previewText);
    } catch {
      const failure = t("visual.preview.invalidJson");
      setPreviewError(failure);
      addNotification({ dedupeKey: "optical-shadow-preview-invalid", level: "warning", title: lang === "zh" ? "Shadow 预览已阻止" : "Shadow preview blocked", message: failure, receipt: { requestId, targetId: "optical-shadow-preview" } });
      return;
    }
    setPreviewRunning(true);
    try {
      const result = await previewOpticalPlan(body, { route_id: previewRouteId || undefined });
      if (!mountedRef.current || seq !== previewSeqRef.current) return;
      if (!result.ok || !result.data) {
        const failure = presentOperationFailure("visual", result.error, lang);
        setPreviewError(failure.message);
        addNotification({ dedupeKey: "optical-shadow-preview-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "optical-shadow-preview" } });
        return;
      }
      setPreviewResult(result.data);
      addNotification({ dedupeKey: `optical-shadow-preview-${requestId}`, level: "success", title: lang === "zh" ? "Shadow 预览完成" : "Shadow preview complete", message: lang === "zh" ? "已生成仅含元数据的规划摘要，未改写真实请求。" : "A metadata-only planning summary was created without modifying a real request.", receipt: { requestId, targetId: "optical-shadow-preview" } });
    } catch (err) {
      if (!mountedRef.current || seq !== previewSeqRef.current) return;
      const failure = presentOperationFailure("visual", err, lang);
      setPreviewError(failure.message);
      addNotification({ dedupeKey: "optical-shadow-preview-error", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "optical-shadow-preview" } });
    } finally {
      if (!mountedRef.current || seq !== previewSeqRef.current) return;
      setPreviewRunning(false);
    }
  };

  const kpiItems = useMemo(() => [
    {
      label: t("visual.kpi.enabled"),
      value: !statusOk ? "—" : status?.enabled ? t("common.enabled") : t("common.disabled"),
      detail: statusOk
        ? formatOpticalMode(status?.effective_mode ?? status?.default_mode, lang)
        : t("visual.unavailableDetail"),
    },
    {
      label: t("visual.kpi.mode"),
      value: statusOk ? formatOpticalMode(status?.effective_mode ?? status?.default_mode, lang) : "—",
      detail: statusOk
        ? `${t("visual.kpi.configured")}: ${formatOpticalMode(status?.configured_mode, lang)}`
        : t("visual.unavailableDetail"),
    },
    {
      label: t("visual.kpi.active"),
      value: !statusOk ? "—" : status?.active_reachable ? t("visual.activeReachable") : t("visual.activeOff"),
      detail: statusOk
        ? `${t("visual.activeReadonly")} · ${status?.active_execution_observed
            ? t("visual.activeObserved")
            : t("visual.activeNotObserved")}`
        : t("visual.unavailableDetail"),
    },
    {
      label: t("visual.kpi.failClosed"),
      value: !statusOk ? "—" : status?.fail_closed === false ? t("common.disabled") : t("common.enabled"),
      detail: statusOk
        ? `${t("visual.kpi.issues")}: ${displayCount(status?.config_issue_count, true)}`
        : t("visual.unavailableDetail"),
    },
  ], [lang, status, statusOk, t]);

  const capabilityRows = useMemo(() => [
    [t("visual.cap.shadow"), capabilitiesOk ? capabilities?.shadow_preview_supported : undefined],
    [t("visual.cap.active"), capabilitiesOk ? capabilities?.active_transform_enabled : undefined],
    [t("visual.cap.renderer"), capabilitiesOk ? capabilities?.deterministic_png_renderer : undefined],
    [t("visual.cap.patcher"), statusOk ? status?.patcher_ready : undefined],
    [t("visual.cap.validator"), capabilitiesOk ? capabilities?.artifact_post_validation : undefined],
    [t("visual.cap.attestation"), capabilitiesOk ? capabilities?.capability_attestation_required : undefined],
  ] as const, [capabilities, capabilitiesOk, status, statusOk, t]);

  const lifecycleRows = useMemo(() => [
    [t("visual.life.configured"), lifecycle?.configured_snapshots],
    [t("visual.life.verified"), lifecycle?.verified_rules ?? lifecycle?.active_snapshots],
    [t("visual.life.revoked"), lifecycle?.revoked_snapshots],
    [t("visual.life.superseded"), lifecycle?.superseded_snapshots],
    [t("visual.life.authorities"), lifecycle?.trusted_authorities],
    [t("visual.life.eval"), lifecycle?.passing_eval_artifacts],
  ] as const, [lifecycle, t]);

  const confirmTitle =
    confirmKind === "one-click-enable"
      ? t("visual.oneClick.confirmEnableTitle")
      : confirmKind === "one-click-disable"
        ? t("visual.oneClick.confirmDisableTitle")
        : t("visual.config.confirmTitle");
  const confirmSummary =
    confirmKind === "one-click-enable"
      ? t("visual.oneClick.confirmEnableSummary")
      : confirmKind === "one-click-disable"
        ? t("visual.oneClick.confirmDisableSummary")
        : t("visual.config.confirmSummary");

  return (
    <div className="space-y-4" data-flat-page data-page="visual">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <h1>{t("visual.title")}</h1>
            <Badge variant="secondary" className="rounded-full px-2 py-0.5 text-[11px] font-medium">
              Beta
            </Badge>
            <DataSourceBadge mode={dataSourceMode} />
          </div>
          <p className="max-w-3xl text-sm text-muted-foreground">{t("visual.limit")}</p>
          {staleNotice ? (
            <p className="text-xs text-amber-700 dark:text-amber-300" role="status">{staleNotice}</p>
          ) : null}
          {configNotice ? (
            <p className="text-xs text-muted-foreground" role="status">{configNotice}</p>
          ) : null}
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => {
            void refresh("manual");
            void loadEditor();
            void loadAgentIntegration(false);
          }}
          disabled={refreshing || loading || editorLoading || agentBusy === "load"}
          className="gap-1.5"
          aria-label={t("visual.refresh")}
        >
          <RefreshCw className={cn("size-3.5", (refreshing || editorLoading) && "animate-spin")} aria-hidden />
          {t("visual.refresh")}
        </Button>
      </div>

      <Tabs value={tab} onValueChange={setTab} className="gap-5">
        <TabsList className="install-workbench-nav grid h-20 w-full grid-cols-3 overflow-hidden rounded-lg border border-border/70 bg-card/80 p-0 shadow-sm">
          {workbenchTabs.map((item) => {
            const Icon = item.icon;
            return (
              <TabsTrigger
                key={item.value}
                value={item.value}
                className="install-workbench-nav-item relative h-full min-w-0 flex-none flex-col items-stretch justify-center gap-1 rounded-none border-0 border-r border-border/60 px-3 py-2 text-left whitespace-normal last:border-r-0"
              >
                <span className="flex min-w-0 items-center gap-2">
                  <span className="install-workbench-nav-icon flex size-7 shrink-0 items-center justify-center rounded-md">
                    <Icon aria-hidden className="size-4" />
                  </span>
                  <span className="truncate text-sm font-medium text-foreground">{item.title}</span>
                </span>
                <span className="install-workbench-nav-description pl-9 text-xs font-normal text-muted-foreground">
                  {item.description}
                </span>
              </TabsTrigger>
            );
          })}
        </TabsList>

        {/* —— 一键启用 —— */}
        <TabsContent value="one-click" className="space-y-4">
          <Surface className="p-5" data-testid="hybrid-agent-integration">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div className="min-w-0 space-y-2">
                <div className="flex items-center gap-2">
                  <Cable className="size-4 text-muted-foreground" aria-hidden />
                  <h2 className="text-base font-semibold">
                    {lang === "zh" ? "Agent 接入" : "Agent integration"}
                  </h2>
                </div>
                <p className="max-w-3xl text-sm text-muted-foreground">
                  {lang === "zh"
                    ? "为 Claude Code、Codex 和 OpenCode 安装精简规则、会话状态 Hook/Plugin 与恢复 Skill。真实压缩仍由 d-token 本地代理完成，MCP 只提供元数据诊断。"
                    : "Install concise rules, session status Hook/Plugin, and recovery Skill for Claude Code, Codex, and OpenCode. The local d-token proxy still performs compression; MCP is metadata-only diagnostics."}
                </p>
              </div>
              <Badge variant={agentTransactionState === "verified" ? "default" : "secondary"}>
                {agentTransactionState === "verified"
                  ? lang === "zh" ? "已验证" : "Verified"
                  : agentTransactionState
                    ? agentTransactionState
                  : agentDiscovery?.bridge.available
                    ? lang === "zh" ? "可配置" : "Ready"
                    : lang === "zh" ? "不可用" : "Unavailable"}
              </Badge>
            </div>

            <div className="mt-4 grid gap-3 md:grid-cols-3">
              <div className="rounded-xl border border-border/70 bg-muted/15 p-3 text-sm">
                <div className="text-xs text-muted-foreground">d-token service</div>
                <div className="mt-1 font-medium">{agentDiscovery?.service.state ?? "unknown"}</div>
                <div className="mt-1 text-xs text-muted-foreground">
                  PID {agentDiscovery?.service.pid ?? "—"} · port {agentDiscovery?.service.port ?? 18001}
                </div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {agentDiscovery?.service.runtime_identity ?? "identity unavailable"}
                  {" · "}
                  {agentDiscovery?.service.runtime_version
                    ? `v${agentDiscovery.service.runtime_version}`
                    : "version unavailable"}
                </div>
                {agentDiscovery?.service.state !== "running" ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    className="mt-3"
                    disabled={agentBusy !== ""}
                    onClick={() => void runServiceStart()}
                    aria-busy={agentBusy === "service"}
                  >
                    {agentBusy === "service" ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}
                    {agentBusy === "service"
                      ? lang === "zh" ? "正在启动…" : "Starting…"
                      : lang === "zh" ? "启动 d-token 服务" : "Start d-token service"}
                  </Button>
                ) : null}
              </div>
              <div className="rounded-xl border border-border/70 bg-muted/15 p-3 text-sm">
                <div className="text-xs text-muted-foreground">Transport</div>
                <div className="mt-1 font-medium">
                  {agentDiscovery?.transport.configured
                    ? lang === "zh" ? "已配置路由" : "Route configured"
                    : lang === "zh" ? "需在“安装与连接”配置" : "Configure in Install & Connect"}
                </div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {lang === "zh"
                    ? `已发现 ${agentDiscovery?.transport.route_count ?? 0} 条路由；本事务不修改 Provider 凭据。`
                    : `${agentDiscovery?.transport.route_count ?? 0} routes found; this transaction never changes provider credentials.`}
                </div>
              </div>
              <div className="rounded-xl border border-border/70 bg-muted/15 p-3 text-sm">
                <div className="text-xs text-muted-foreground">Agent bridge</div>
                <div className="mt-1 font-medium">
                  {agentDiscovery?.bridge.available
                    ? lang === "zh" ? "已打包" : "Bundled"
                    : lang === "zh" ? "尚未构建" : "Not built"}
                </div>
                <div className="mt-1 truncate text-xs text-muted-foreground" title={agentDiscovery?.bridge.path ?? undefined}>
                  {agentDiscovery?.bridge.path ?? agentDiscovery?.bridge.error ?? "—"}
                </div>
              </div>
            </div>

            <div className="mt-4 grid gap-4 lg:grid-cols-[1.1fr_0.9fr]">
              <div className="space-y-3 rounded-xl border border-border/70 p-4">
                <h3 className="text-sm font-semibold">{lang === "zh" ? "目标 Agent" : "Target Agents"}</h3>
                <div className="grid gap-2 sm:grid-cols-3">
                  {(agentDiscovery?.agents ?? []).map((agent) => (
                    <label key={agent.id} className="flex items-center justify-between gap-2 rounded-lg border border-border/60 px-3 py-2 text-sm">
                      <span>
                        <span className="block font-medium">{agent.name}</span>
                        <span className="block text-[11px] text-muted-foreground">
                          {!agent.installed
                            ? lang === "zh" ? "未检测" : "Not detected"
                            : agent.integration_supported === false
                              ? lang === "zh" ? "已检测 · 暂不支持自动接入" : "Detected · automatic integration unavailable"
                              : lang === "zh" ? "已检测" : "Detected"}
                        </span>
                      </span>
                      <Switch
                        checked={agentDraft.agent_ids.includes(agent.id)}
                        disabled={!agent.installed || agent.integration_supported === false || agentBusy !== ""}
                        onCheckedChange={(checked) => toggleAgent(agent.id, checked)}
                        aria-label={`${agent.name} ${lang === "zh" ? "接入" : "integration"}`}
                      />
                    </label>
                  ))}
                </div>

                <div className="grid gap-3 sm:grid-cols-[160px_1fr]">
                  <label className="space-y-1 text-sm">
                    <span className="text-xs text-muted-foreground">{lang === "zh" ? "作用域" : "Scope"}</span>
                    <select
                      className="h-9 w-full rounded-md border border-border bg-background px-2 text-sm"
                      value={agentDraft.scope}
                      disabled={agentBusy !== ""}
                      onChange={(event) => {
                        const scope = event.target.value === "project" ? "project" : "global";
                        setAgentDraft((current) => ({ ...current, scope }));
                        setAgentReview(null);
                      }}
                    >
                      <option value="project">{lang === "zh" ? "当前项目" : "Current project"}</option>
                      <option value="global">{lang === "zh" ? "全局" : "Global"}</option>
                    </select>
                  </label>
                  {agentDraft.scope === "project" ? (
                    <label className="space-y-1 text-sm">
                      <span className="text-xs text-muted-foreground">{lang === "zh" ? "项目绝对路径" : "Absolute project path"}</span>
                      <Input
                        value={agentDraft.project_root ?? ""}
                        disabled={agentBusy !== ""}
                        placeholder="/path/to/project"
                        onChange={(event) => {
                          setAgentDraft((current) => ({ ...current, project_root: event.target.value }));
                          setAgentReview(null);
                        }}
                      />
                    </label>
                  ) : (
                    <div className="self-end pb-2 text-xs text-muted-foreground">
                      {lang === "zh" ? "全局文件仍会先预览 diff，再由你确认。" : "Global file diffs are still previewed before confirmation."}
                    </div>
                  )}
                </div>
              </div>

              <div className="space-y-3 rounded-xl border border-border/70 p-4">
                <h3 className="text-sm font-semibold">{lang === "zh" ? "接入组件" : "Integration surfaces"}</h3>
                {([
                  ["install_instructions", lang === "zh" ? "规则 / Instructions" : "Rules / Instructions", true],
                  ["install_hook_or_plugin", lang === "zh" ? "Hook / Plugin（动态状态）" : "Hook / Plugin (dynamic status)", true],
                  ["install_skill", lang === "zh" ? "恢复 Skill" : "Recovery Skill", true],
                  ["install_mcp", lang === "zh" ? "诊断 MCP（默认关闭）" : "Diagnostic MCP (off by default)", false],
                ] as const).map(([key, label, recommended]) => (
                  <label key={key} className="flex items-center justify-between gap-3 text-sm">
                    <span>
                      {label}
                      {recommended ? <span className="ml-1 text-[11px] text-muted-foreground">recommended</span> : null}
                    </span>
                    <Switch
                      checked={agentDraft[key]}
                      disabled={agentBusy !== ""}
                      onCheckedChange={(checked) => {
                        setAgentDraft((current) => ({ ...current, [key]: checked }));
                        setAgentReview(null);
                      }}
                      aria-label={label}
                    />
                  </label>
                ))}
                <p className="text-xs text-muted-foreground">
                  {lang === "zh"
                    ? "Codex Hook 写入后仍需你在 /hooks 中确认信任；不会绕过 hash trust。"
                    : "After writing, the Codex Hook still requires your trust in /hooks; hash trust is never bypassed."}
                </p>
              </div>
            </div>

            {agentNotice ? <p className="mt-4 text-xs text-muted-foreground" role="status">{agentNotice}</p> : null}
            <div className="mt-4 flex flex-wrap gap-2">
              <Button
                type="button"
                disabled={!agentDiscovery?.bridge.available || agentDraft.agent_ids.length === 0 || agentBusy !== ""}
                onClick={() => void beginAgentIntegration()}
                aria-busy={agentBusy === "preview"}
              >
                {agentBusy === "preview" ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}
                {agentBusy === "preview"
                  ? lang === "zh" ? "正在生成预览…" : "Preparing preview…"
                  : lang === "zh" ? "预览接入变更" : "Preview integration changes"}
              </Button>
              {agentReceipt ? (
                <Button type="button" variant="outline" disabled={agentBusy !== ""} onClick={() => void runAgentRollback()} aria-busy={agentBusy === "rollback"}>
                  {agentBusy === "rollback" ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}
                  {agentBusy === "rollback"
                    ? lang === "zh" ? "正在回退…" : "Rolling back…"
                    : lang === "zh" ? "回退本次 Agent 接入" : "Rollback this integration"}
                </Button>
              ) : null}
            </div>
          </Surface>

          <Surface className="p-5">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div className="min-w-0 space-y-2">
                <h2 className="text-base font-semibold">{t("visual.oneClick.title")}</h2>
                <p className="max-w-2xl text-sm text-muted-foreground">{t("visual.oneClick.detail")}</p>
                <p className="text-xs text-muted-foreground">{t("visual.config.activeForced")}</p>
              </div>
              <Badge variant={oneClickPhase === "enabled" ? "default" : "secondary"}>
                {oneClickPhase === "loading"
                  ? t("visual.loading")
                  : oneClickPhase === "blocked"
                    ? t("visual.oneClick.blocked")
                    : oneClickPhase === "enabled"
                      ? t("visual.oneClick.enabled")
                      : t("visual.oneClick.ready")}
              </Badge>
            </div>

            {oneClickPhase === "loading" ? (
              <EmptyState status="loading" title={t("visual.loading")} description={t("visual.loadingDetail")} />
            ) : null}

            {oneClickPhase === "blocked" ? (
              <div className="mt-4 rounded-xl border border-amber-500/25 bg-amber-500/5 p-4 text-sm">
                <p className="font-medium text-amber-800 dark:text-amber-200">{t("visual.oneClick.blockedTitle")}</p>
                <p className="mt-1 text-muted-foreground">{t("visual.oneClick.blockedDetail")}</p>
                {editorError ? <p className="mt-2 text-xs text-muted-foreground">{editorError}</p> : null}
              </div>
            ) : null}

            {oneClickPhase === "ready" && editor ? (
              <div className="mt-4 space-y-4">
                <div className="rounded-xl border border-border/70 bg-muted/20 p-4 text-sm">
                  <div className="font-medium">{t("visual.oneClick.readyTitle")}</div>
                  <ul className="mt-2 list-disc space-y-1 pl-5 text-muted-foreground">
                    <li>{t("visual.oneClick.bulletService")}</li>
                    <li>{t("visual.oneClick.bulletRoutes").replace("{n}", String(editor.route_count))}</li>
                    <li>{t("visual.oneClick.bulletShadow")}</li>
                    <li>{t("visual.oneClick.bulletNoActive")}</li>
                  </ul>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    size="lg"
                    className="gap-2"
                    disabled={configBusy !== ""}
                    onClick={() => void beginApply(buildOneClickEnableDraft(editor), "one-click-enable")}
                    aria-busy={configBusy === "preview" || configBusy === "apply"}
                  >
                    {configBusy === "preview" || configBusy === "apply" ? <RefreshCw className="size-4 animate-spin" aria-hidden /> : <Rocket className="size-4" aria-hidden />}
                    {configBusy === "preview" || configBusy === "apply"
                      ? t("visual.config.applying")
                      : t("visual.oneClick.enableCta")}
                  </Button>
                  {lastApply?.rollback_id ? (
                    <Button
                      type="button"
                      variant="outline"
                      disabled={configBusy !== ""}
                      onClick={() => void runRollback()}
                      aria-busy={configBusy === "rollback"}
                    >
                      {configBusy === "rollback" ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}
                      {t("visual.config.rollback")}
                    </Button>
                  ) : null}
                </div>
              </div>
            ) : null}

            {oneClickPhase === "enabled" && editor ? (
              <div className="mt-4 space-y-4">
                <div className="rounded-xl border border-emerald-500/25 bg-emerald-500/5 p-4 text-sm">
                  <div className="font-medium text-emerald-800 dark:text-emerald-200">
                    {t("visual.oneClick.enabledTitle")}
                  </div>
                  <p className="mt-1 text-muted-foreground">{t("visual.oneClick.enabledDetail")}</p>
                  <p className="mt-2 text-xs text-muted-foreground">
                    {statusOk
                      ? `${t("visual.kpi.mode")}: ${formatOpticalMode(status?.effective_mode ?? status?.default_mode, lang)}`
                      : t("visual.unavailableDetail")}
                  </p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button type="button" variant="outline" onClick={() => setTab("status")}>
                    {t("visual.oneClick.viewStatus")}
                  </Button>
                  <Button type="button" variant="outline" onClick={() => setTab("advanced")}>
                    {t("visual.oneClick.openAdvanced")}
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    disabled={configBusy !== ""}
                    onClick={() => void beginApply(buildOneClickDisableDraft(editor), "one-click-disable")}
                    aria-busy={configBusy === "preview" || configBusy === "apply"}
                  >
                    {configBusy === "preview" || configBusy === "apply" ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}
                    {t("visual.oneClick.disableCta")}
                  </Button>
                  {lastApply?.rollback_id ? (
                    <Button
                      type="button"
                      variant="outline"
                      disabled={configBusy !== ""}
                      onClick={() => void runRollback()}
                      aria-busy={configBusy === "rollback"}
                    >
                      {configBusy === "rollback" ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}
                      {t("visual.config.rollback")}
                    </Button>
                  ) : null}
                </div>
              </div>
            ) : null}
          </Surface>
        </TabsContent>

        {/* —— 高级配置 —— */}
        <TabsContent value="advanced" className="space-y-4">
          <Surface className="p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
              <div>
                <h2 className="text-base font-semibold">{t("visual.config.title")}</h2>
                <p className="mt-1 text-xs text-muted-foreground">{t("visual.tab.advancedDesc")}</p>
              </div>
              <div className="flex flex-wrap gap-2">
                {lastApply?.rollback_id ? (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={configBusy !== ""}
                    onClick={() => void runRollback()}
                  >
                    {t("visual.config.rollback")}
                  </Button>
                ) : null}
                <Button
                  type="button"
                  size="sm"
                  disabled={!canConfigure || !dirty || !draft || configBusy !== ""}
                  onClick={() => draft && void beginApply(draft, "advanced")}
                >
                  {configBusy === "preview" ? t("visual.config.previewing") : t("visual.config.preview")}
                </Button>
              </div>
            </div>
            <p className="mb-3 text-xs text-muted-foreground">{t("visual.config.activeForced")}</p>
            {dirty ? (
              <p className="mb-3 text-xs text-amber-700 dark:text-amber-300" role="status">
                {t("visual.config.dirty")}
              </p>
            ) : null}

            {!canConfigure ? (
              <EmptyState
                status="empty"
                title={t("visual.oneClick.blockedTitle")}
                description={t("visual.oneClick.blockedDetail")}
              />
            ) : draft ? (
              <div className="space-y-4">
                <div className="rounded-xl border border-border/70 p-3">
                  <h3 className="text-sm font-semibold">{t("visual.config.global")}</h3>
                  <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
                    <label className="flex items-center gap-2 text-sm">
                      <Switch
                        checked={draft.global.enabled}
                        onCheckedChange={(checked) => updateGlobal({ enabled: checked })}
                        aria-label={t("visual.config.enable")}
                      />
                      {t("visual.config.enable")}
                    </label>
                    <label className="flex items-center gap-2 text-sm">
                      <span className="text-muted-foreground">{t("visual.config.mode")}</span>
                      <select
                        className="rounded-md border border-border bg-background px-2 py-1 text-sm"
                        value={draft.global.default_mode}
                        onChange={(event) =>
                          updateGlobal({ default_mode: event.target.value as OpticalSafeMode })
                        }
                      >
                        <option value="off">{t("visual.config.mode.off")}</option>
                        <option value="shadow_preview">{t("visual.config.mode.shadow")}</option>
                      </select>
                    </label>
                  </div>
                </div>

                <div className="rounded-xl border border-border/70 p-3">
                  <h3 className="mb-2 text-sm font-semibold">{t("visual.config.routes")}</h3>
                  {draft.routes.length === 0 ? (
                    <p className="text-sm text-muted-foreground">{t("visual.config.routesEmpty")}</p>
                  ) : (
                    <div className="space-y-2">
                      {draft.routes.map((route) => {
                        const label =
                          editor?.routes.find((item) => item.route_id === route.route_id)?.label
                          ?? route.route_id;
                        return (
                          <div
                            key={route.route_id}
                            className="grid gap-2 rounded-lg border border-border/50 p-3 sm:grid-cols-2"
                          >
                            <div className="text-sm font-medium">{label}</div>
                            <div className="text-xs text-muted-foreground">{route.route_id}</div>
                            <label className="flex items-center gap-2 text-sm">
                              <Switch
                                checked={route.enabled}
                                onCheckedChange={(checked) => updateRoute(route.route_id, { enabled: checked })}
                              />
                              {t("visual.config.routeEnabled")}
                            </label>
                            <label className="flex items-center gap-2 text-sm">
                              <span className="text-muted-foreground">{t("visual.config.mode")}</span>
                              <select
                                className="rounded-md border border-border bg-background px-2 py-1 text-sm"
                                value={route.mode}
                                onChange={(event) =>
                                  updateRoute(route.route_id, {
                                    mode: event.target.value as OpticalSafeMode,
                                  })
                                }
                              >
                                <option value="off">{t("visual.config.mode.off")}</option>
                                <option value="shadow_preview">{t("visual.config.mode.shadow")}</option>
                              </select>
                            </label>
                            <label className="flex items-center gap-2 text-sm">
                              <Switch
                                checked={route.allow_tool_result}
                                onCheckedChange={(checked) =>
                                  updateRoute(route.route_id, { allow_tool_result: checked })
                                }
                              />
                              {t("visual.config.toolResult")}
                            </label>
                            <label className="flex items-center gap-2 text-sm">
                              <Switch
                                checked={route.allow_history}
                                onCheckedChange={(checked) =>
                                  updateRoute(route.route_id, { allow_history: checked })
                                }
                              />
                              {t("visual.config.history")}
                            </label>
                            <label className="flex items-center gap-2 text-sm sm:col-span-2">
                              <Switch
                                checked={route.confirmed_experimental_lossy}
                                onCheckedChange={(checked) =>
                                  updateRoute(route.route_id, {
                                    confirmed_experimental_lossy: checked,
                                  })
                                }
                              />
                              {t("visual.config.lossy")}
                            </label>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
                <p className="text-xs text-muted-foreground">{summarizeOpticalDraft(draft, lang)}</p>
              </div>
            ) : null}
          </Surface>
        </TabsContent>

        {/* —— 状态与预览 —— */}
        <TabsContent value="status" className="space-y-4">
          {state === "loading" && !snapshot ? (
            <EmptyState status="loading" title={t("visual.loading")} description={t("visual.loadingDetail")} />
          ) : null}
          {state === "unavailable" && !snapshot ? (
            <EmptyState
              status="empty"
              title={t("visual.emptyTitle")}
              description={error || t("visual.emptyDetail")}
            />
          ) : null}
          {error && snapshot ? (
            <p className="text-xs text-muted-foreground" role="status">{error}</p>
          ) : null}

          {snapshot ? (
            <>
              <MetricStrip className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                {kpiItems.map((item) => (
                  <div key={item.label} className="rounded-xl border border-border/60 bg-background/40 p-4">
                    <div className="text-xs font-medium text-muted-foreground">{item.label}</div>
                    <div className="mt-2 text-2xl font-semibold tracking-tight tabular-nums">{item.value}</div>
                    <div className="mt-1 text-xs text-muted-foreground">{item.detail}</div>
                  </div>
                ))}
              </MetricStrip>

              <div className="grid gap-3 lg:grid-cols-2">
                <Surface className="p-4">
                  <div className="mb-3 flex items-center gap-2">
                    <Eye className="size-4 text-muted-foreground" aria-hidden />
                    <h2 className="text-base font-semibold">{t("visual.capabilities")}</h2>
                  </div>
                  <div className="divide-y divide-border/60">
                    {capabilityRows.map(([label, value]) => (
                      <div key={label} className="flex items-center justify-between gap-3 py-2 text-sm">
                        <span className="text-muted-foreground">{label}</span>
                        <span className="font-medium tabular-nums">
                          {value === undefined ? "—" : value ? t("common.enabled") : t("common.disabled")}
                        </span>
                      </div>
                    ))}
                  </div>
                </Surface>
                <Surface className="p-4">
                  <div className="mb-3 flex items-center gap-2">
                    <Shield className="size-4 text-muted-foreground" aria-hidden />
                    <h2 className="text-base font-semibold">{t("visual.lifecycle")}</h2>
                  </div>
                  <div className="divide-y divide-border/60">
                    {lifecycleRows.map(([label, value]) => (
                      <div key={label} className="flex items-center justify-between gap-3 py-2 text-sm">
                        <span className="text-muted-foreground">{label}</span>
                        <span className="font-medium tabular-nums">{displayCount(value, lifecycleOk)}</span>
                      </div>
                    ))}
                  </div>
                </Surface>
              </div>

              <Surface className="p-4">
                <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                  <h2 className="text-base font-semibold">{t("visual.events")}</h2>
                  <span className="text-xs text-muted-foreground">
                    {eventsOk
                      ? `${events.length} ${lang === "zh" ? "条" : "items"} · ${t("visual.estimateNote")}`
                      : t("visual.eventsUnavailable")}
                  </span>
                </div>
                {!eventsOk ? (
                  <EmptyState status="warning" title={t("visual.eventsUnavailable")} description={snapshot?.events.error || undefined} />
                ) : events.length === 0 ? (
                  <EmptyState status="empty" title={t("visual.eventsEmpty")} description={t("visual.eventsEmptyDetail")} />
                ) : (
                  <div className="overflow-x-auto">
                    <table className="w-full min-w-[640px] text-left text-sm">
                      <thead className="text-xs text-muted-foreground">
                        <tr className="border-b border-border/70">
                          <th className="px-2 py-2 font-medium">{t("visual.col.time")}</th>
                          <th className="px-2 py-2 font-medium">{t("visual.col.stage")}</th>
                          <th className="px-2 py-2 font-medium">{t("visual.col.status")}</th>
                          <th className="px-2 py-2 font-medium">{t("visual.col.mode")}</th>
                          <th className="px-2 py-2 font-medium">{t("visual.col.candidates")}</th>
                          <th className="px-2 py-2 font-medium">{t("visual.col.images")}</th>
                          <th className="px-2 py-2 font-medium">{t("visual.col.saved")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {events.map((event, index) => (
                          <tr key={event.event_id ?? `${event.timestamp_ms}-${index}`} className="border-b border-border/40 last:border-0">
                            <td className="px-2 py-2 tabular-nums text-muted-foreground">{formatTimestampMs(event.timestamp_ms, lang)}</td>
                            <td className="px-2 py-2">{formatOpticalStage(event.stage ?? event.metadata?.kind, lang)}</td>
                            <td className="px-2 py-2">{formatOpticalStatus(event.status ?? event.metadata?.final_status, lang)}</td>
                            <td className="px-2 py-2">{formatOpticalMode(event.metadata?.mode, lang)}</td>
                            <td className="px-2 py-2 tabular-nums">{displayCount(event.metadata?.candidate_blocks, true)}</td>
                            <td className="px-2 py-2 tabular-nums">{displayCount(event.metadata?.estimated_images, true)}</td>
                            <td className="px-2 py-2 tabular-nums">
                              {displayCount(event.metadata?.net_saved_tokens, true)}
                              <span className="ml-1 text-[11px] text-muted-foreground">{t("visual.estimate")}</span>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </Surface>

              <Surface className="p-4">
                <button
                  type="button"
                  className="flex w-full items-center justify-between gap-2 text-left"
                  onClick={() => setPreviewOpen((open) => !open)}
                  aria-expanded={previewOpen}
                  aria-controls="visual-preview-panel"
                  id="visual-preview-toggle"
                >
                  <div>
                    <h2 className="text-base font-semibold">{t("visual.preview.title")}</h2>
                    <p className="mt-1 text-xs text-muted-foreground">{t("visual.preview.detail")}</p>
                  </div>
                  <ChevronDown
                    className={cn("size-4 shrink-0 text-muted-foreground transition-transform", previewOpen && "rotate-180")}
                    aria-hidden
                  />
                </button>
                {previewOpen ? (
                  <div id="visual-preview-panel" className="mt-4 space-y-3" role="region" aria-labelledby="visual-preview-toggle">
                    <div className="flex flex-wrap items-center gap-2">
                      {editor && editor.routes.length > 0 ? (
                        <label className="flex items-center gap-2 text-sm">
                          <span className="text-xs text-muted-foreground">{t("visual.preview.route")}</span>
                          <select
                            className="h-9 rounded-md border border-border bg-background px-2 text-sm"
                            value={previewRouteId}
                            onChange={(event) => setPreviewRouteId(event.target.value)}
                            aria-label={t("visual.preview.route")}
                          >
                            <option value="">{t("visual.preview.routeNone")}</option>
                            {editor.routes.map((route) => (
                              <option key={route.route_id} value={route.route_id}>
                                {route.label ?? route.route_id}
                              </option>
                            ))}
                          </select>
                        </label>
                      ) : null}
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => setPreviewText(JSON.stringify(OPTICAL_SAMPLE_BODY, null, 2))}
                      >
                        {t("visual.preview.sample")}
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        onClick={() => void runPreview()}
                        disabled={previewRunning}
                        aria-busy={previewRunning}
                      >
                        {previewRunning ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}
                        {previewRunning ? t("visual.preview.running") : t("visual.preview.run")}
                      </Button>
                    </div>
                    <Textarea
                      value={previewText}
                      onChange={(event) => setPreviewText(event.target.value)}
                      className="min-h-[180px] font-mono text-xs"
                      spellCheck={false}
                      aria-label={t("visual.preview.inputLabel")}
                    />
                    {previewError ? (
                      <p className="text-sm text-amber-700 dark:text-amber-300" role="alert">{previewError}</p>
                    ) : null}
                    {previewResult ? (
                      <div className="rounded-xl border border-border/70 bg-muted/20 p-3 text-sm">
                        <div className="grid gap-2 sm:grid-cols-2">
                          <div>
                            <div className="text-xs text-muted-foreground">{t("visual.preview.finalStatus")}</div>
                            <div className="font-medium">{formatOpticalStatus(previewResult.final_status, lang)}</div>
                          </div>
                          <div>
                            <div className="text-xs text-muted-foreground">{t("visual.kpi.mode")}</div>
                            <div className="font-medium">{formatOpticalMode(previewResult.mode, lang)}</div>
                          </div>
                          <div>
                            <div className="text-xs text-muted-foreground">{t("visual.col.candidates")}</div>
                            <div className="font-medium tabular-nums">{displayCount(previewResult.counters?.candidate_blocks, true)}</div>
                          </div>
                          <div>
                            <div className="text-xs text-muted-foreground">{t("visual.col.images")}</div>
                            <div className="font-medium tabular-nums">{displayCount(previewResult.counters?.estimated_images, true)}</div>
                          </div>
                          <div>
                            <div className="text-xs text-muted-foreground">{t("visual.col.saved")}</div>
                            <div className="font-medium tabular-nums">
                              {displayCount(previewResult.economics?.net_saved_tokens, true)}
                              <span className="ml-1 text-[11px] text-muted-foreground">{t("visual.estimate")}</span>
                            </div>
                          </div>
                        </div>
                        {previewResult.gates && previewResult.gates.length > 0 ? (
                          <div className="mt-3">
                            <div className="text-xs text-muted-foreground">{t("visual.preview.gates")}</div>
                            <ul className="mt-1 space-y-1">
                              {previewResult.gates.map((gate, index) => (
                                <li key={`${gate.name ?? "gate"}-${index}`} className="flex flex-wrap items-baseline gap-x-2 text-xs">
                                  <span className="font-medium">{gate.name ?? `#${index + 1}`}</span>
                                  <span className={gate.passed ? "text-emerald-700 dark:text-emerald-300" : "text-amber-700 dark:text-amber-300"}>
                                    {gate.passed ? t("visual.preview.gatePassed") : t("visual.preview.gateFailed")}
                                  </span>
                                  {gate.reason ? <span className="text-muted-foreground">{gate.reason}</span> : null}
                                </li>
                              ))}
                            </ul>
                          </div>
                        ) : null}
                        {previewResult.warnings && previewResult.warnings.length > 0 ? (
                          <div className="mt-3">
                            <div className="text-xs text-muted-foreground">{t("visual.preview.warnings")}</div>
                            <ul className="mt-1 list-disc space-y-1 pl-4 text-xs text-amber-700 dark:text-amber-300">
                              {previewResult.warnings.map((warning, index) => (
                                <li key={index}>{warning}</li>
                              ))}
                            </ul>
                          </div>
                        ) : null}
                      </div>
                    ) : null}
                  </div>
                ) : null}
              </Surface>
            </>
          ) : null}
        </TabsContent>
      </Tabs>

      <OperationDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={confirmTitle}
        summary={confirmSummary}
        impact={pendingDraft ? summarizeOpticalDraft(pendingDraft, lang) : undefined}
        risk="warning"
        pending={configBusy === "apply"}
        confirmDisabled={!review?.ok || !review.before_sha256 || !review.after_sha256}
        confirmLabel={t("visual.config.apply")}
        cancelLabel={lang === "zh" ? "取消" : "Cancel"}
        onConfirm={() => void runConfigApply()}
        details={
          review?.ok ? (
            <div className="space-y-2 text-xs text-muted-foreground">
              <div>before {shortHash(review.before_sha256)} → after {shortHash(review.after_sha256)}</div>
              {review.target ? <div>{review.target}</div> : null}
              {review.diff ? (
                <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background p-2 font-mono text-[11px]">
                  {review.diff}
                </pre>
              ) : null}
            </div>
          ) : null
        }
      />
      <OperationDialog
        open={agentConfirmOpen}
        onOpenChange={setAgentConfirmOpen}
        title={lang === "zh" ? "确认 Agent 接入变更" : "Confirm Agent integration changes"}
        summary={lang === "zh"
          ? "d-token 将按下列 diff 创建备份并原子写入；不会修改 Provider 凭据或自动开启视觉 Active。"
          : "d-token will create backups and atomically apply the diffs below; provider credentials and visual Active remain unchanged."}
        impact={agentReview?.plan
          ? `${agentReview.plan.targets.length} ${lang === "zh" ? "个目标文件" : "target files"} · ${agentReview.plan.scope}`
          : undefined}
        risk="warning"
        pending={agentBusy === "apply" || agentBusy === "verify"}
        confirmDisabled={!agentReview?.ok || !agentReview.plan}
        confirmLabel={agentBusy === "verify"
          ? lang === "zh" ? "正在验证…" : "Verifying…"
          : lang === "zh" ? "确认、应用并验证" : "Confirm, apply, and verify"}
        cancelLabel={lang === "zh" ? "取消" : "Cancel"}
        onConfirm={() => void runAgentIntegration()}
        details={agentReview?.plan ? (
          <div className="max-h-[50vh] space-y-3 overflow-auto pr-1 text-xs text-muted-foreground">
            {agentReview.plan.targets.map((target) => (
              <div key={`${target.surface}:${target.target_path}`} className="rounded-lg border border-border/70 p-2">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="font-medium text-foreground">{target.surface}</span>
                  <span>{target.agent_ids.join(", ")}</span>
                </div>
                <div className="mt-1 break-all">{target.target_path}</div>
                <div className="mt-1">{shortHash(target.before_sha256)} → {shortHash(target.after_sha256)}</div>
                {target.trust_required ? (
                  <div className="mt-1 text-amber-700 dark:text-amber-300">
                    {lang === "zh" ? "需要在 Codex /hooks 中信任" : "Requires trust in Codex /hooks"}
                  </div>
                ) : null}
                <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap rounded-md bg-background p-2 font-mono text-[11px]">
                  {target.diff || (lang === "zh" ? "内容无变化" : "No content change")}
                </pre>
              </div>
            ))}
          </div>
        ) : null}
      />
    </div>
  );
}
