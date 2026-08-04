import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { RefreshCw, Search, ShieldCheck } from "lucide-react";
import { Surface } from "../common/Surface";
import { EmptyState } from "../common/EmptyState";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Badge } from "../ui/badge";
import { OperationDialog } from "../common/OperationDialog";
import { useI18n, useT } from "../../lib/i18n";
import { invokeNative, type NativeActionResult } from "../../lib/native";
import { useRuntimeDashboardDetail } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { presentOperationFailure } from "../../lib/userCopy";
import { projectPolicyInputFromPayload } from "./Advanced";

const PROJECTS_PER_PAGE = 20;

export function SmartLearning() {
  const t = useT();
  const { lang } = useI18n();
  const { payload, refresh } = useRuntimeDashboardDetail();
  const { addNotification } = useNotifications();
  const [projectRoot, setProjectRoot] = useState("");
  const learningDiscovery = useLearningDiscovery();
  const [result, setResult] = useState<Record<string, any> | null>(null);
  const [analysis, setAnalysis] = useState<LearningAnalysis | null>(null);
  const [saved, setSaved] = useState<Record<string, any> | null>(null);
  const [running, setRunning] = useState(false);
  const [discoveryAction, setDiscoveryAction] = useState<"start" | "cancel" | "">("");
  const [saving, setSaving] = useState(false);
  const [actionFeedback, setActionFeedback] = useState<LearningActionFeedback | null>(null);
  const [notice, setNotice] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [projectPage, setProjectPage] = useState(0);
  const [managedText, setManagedText] = useState("");
  const [targetName, setTargetName] = useState("AGENTS.md");
  const [review, setReview] = useState<LearningReview | null>(null);
  const [writeError, setWriteError] = useState("");
  const [applying, setApplying] = useState(false);
  const [mode, setMode] = useState<"prompt" | "direct">("prompt");
  const [delegationAgents, setDelegationAgents] = useState<DelegationAgent[]>([]);
  const [delegationAgent, setDelegationAgent] = useState("claude");
  const [delegationReview, setDelegationReview] = useState(false);
  const [delegationRunning, setDelegationRunning] = useState(false);
  const [delegationRequestId, setDelegationRequestId] = useState("");
  const draftSectionRef = useRef<HTMLDivElement>(null);
  const [draftRevealToken, setDraftRevealToken] = useState(0);
  const candidates = useMemo(
    () => (learningDiscovery.snapshot?.candidates ?? []).filter((candidate) => candidate.kind === "project" && candidate.write_eligible),
    [learningDiscovery.snapshot?.candidates],
  );
  const filteredCandidates = useMemo(
    () => candidates
      .filter((candidate) => `${candidate.path} ${candidate.agent} ${candidate.language_fingerprints?.join(" ") ?? ""}`.toLowerCase().includes(query.toLowerCase())),
    [candidates, query],
  );
  const projectPageCount = Math.max(1, Math.ceil(filteredCandidates.length / PROJECTS_PER_PAGE));
  const visibleCandidates = useMemo(
    () => filteredCandidates.slice(projectPage * PROJECTS_PER_PAGE, (projectPage + 1) * PROJECTS_PER_PAGE),
    [filteredCandidates, projectPage],
  );
  const selectedProject = candidates.find((candidate) => candidate.write_eligible && selected.includes(candidate.id));
  const effectiveProjectRoot = projectRoot.trim() || selectedProject?.path || "";
  const input = useMemo(
    () => projectPolicyInputFromPayload(payload, effectiveProjectRoot)
      ?? learningInputFromOwnedRuntime(payload, effectiveProjectRoot)
      ?? learningInputFromProjectMetadata(selectedProject),
    [effectiveProjectRoot, payload, selectedProject],
  );
  const savedAnalysis = saved?.analysis as LearningAnalysis | undefined;
  const activeResult = result ?? (saved?.recommendation ? saved : null);
  const recommendation = activeResult?.recommendation;
  const analysisResult = analysis?.context.project_path === effectiveProjectRoot
    ? analysis
    : (savedAnalysis?.context.project_path === effectiveProjectRoot ? savedAnalysis : null);
  const progressPercent = learningDiscovery.snapshot?.progress.total_agents
    ? Math.round((learningDiscovery.snapshot.progress.scanned_agents / learningDiscovery.snapshot.progress.total_agents) * 100)
    : 0;

  const runDiscovery = async (refreshDiscovery: boolean, roots: string[] = []) => {
    const requestId = createOperationRequestId("learning-discovery");
    setDiscoveryAction("start");
    try {
      const next = await learningDiscovery.start(refreshDiscovery, roots);
      addNotification({ dedupeKey: "learning-discovery-started", level: "success", title: lang === "zh" ? "项目扫描已开始" : "Project scan started", message: lang === "zh" ? "正在本机枚举白名单元数据；不会读取提示词、响应、源码正文或凭据。" : "Allowlisted metadata is being enumerated locally; prompts, responses, source bodies, and credentials are not read.", receipt: { requestId, targetId: next?.job_id ?? "learning-discovery" } });
    } catch (error) {
      const failure = presentOperationFailure("learning", error, lang);
      addNotification({ dedupeKey: "learning-discovery-start-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "learning-discovery" } });
    } finally {
      setDiscoveryAction("");
    }
  };

  const cancelDiscovery = async () => {
    const requestId = createOperationRequestId("learning-discovery-cancel");
    setDiscoveryAction("cancel");
    try {
      const next = await learningDiscovery.cancel();
      addNotification({ dedupeKey: "learning-discovery-cancelled", level: "info", title: lang === "zh" ? "项目扫描已停止" : "Project scan stopped", message: lang === "zh" ? "扫描任务已取消，已完成的只读结果仍会保留。" : "The scan was cancelled; completed read-only results remain available.", receipt: { requestId, targetId: next?.job_id ?? "learning-discovery" } });
    } catch (error) {
      const failure = presentOperationFailure("learning", error, lang);
      addNotification({ dedupeKey: "learning-discovery-cancel-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: "learning-discovery" } });
    } finally {
      setDiscoveryAction("");
    }
  };

  useEffect(() => {
    const available = new Set(candidates.filter((candidate) => candidate.write_eligible).map((candidate) => candidate.id));
    setSelected((current) => {
      const next = current.filter((id) => available.has(id)).slice(0, 1);
      if (next.length === 0 && current.length === 0) {
        const first = candidates.find((candidate) => candidate.write_eligible);
        return first ? [first.id] : [];
      }
      return next.length === current.length ? current : next;
    });
  }, [candidates]);

  useEffect(() => {
    setProjectPage(0);
  }, [query]);

  useEffect(() => {
    setProjectPage((current) => Math.min(current, projectPageCount - 1));
  }, [projectPageCount]);

  useEffect(() => {
    if (!draftRevealToken) return;
    const section = draftSectionRef.current;
    const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    section?.scrollIntoView?.({ behavior: reducedMotion ? "auto" : "smooth", block: "nearest" });
    section?.querySelector("textarea")?.focus({ preventScroll: true });
  }, [draftRevealToken]);

  useEffect(() => {
    void invokeNative<{ state: string; payload?: Record<string, any> }>("get_app_settings", { section: "smart-learning" })
      .then((value) => setSaved(value.state === "loaded" ? value.payload ?? null : null))
      .catch(() => setSaved(null));
    void invokeNative<DelegationCapabilities>("get_learning_delegation_capabilities")
      .then((value) => setDelegationAgents(value.agents))
      .catch(() => setDelegationAgents([]));
  }, []);

  const scan = async () => {
    if (!input || !effectiveProjectRoot) {
      setNotice(lang === "zh" ? "请先选择一个已关联的真实项目。" : "Select a real linked project first.");
      return;
    }
    const requestId = createOperationRequestId("smart-learning-scan");
    setRunning(true);
    try {
      const [next, projectAnalysis] = await Promise.all([
        invokeNative<Record<string, any>>("recommend_project_policy", { request: input }),
        invokeNative<LearningAnalysis>("analyze_learning_project", { request: { project_path: effectiveProjectRoot } }),
      ]);
      if (!projectAnalysis.ok || projectAnalysis.state !== "complete") {
        throw new Error(projectAnalysis.error ?? "project_analysis_failed");
      }
      setResult(next);
      setAnalysis(projectAnalysis);
      setManagedText("");
      setActionFeedback(null);
      setNotice(lang === "zh" ? "项目结构、技术栈、命令、风险与策略分析完成；尚未写入任何文件。" : "Project structure, stack, commands, risks, and policy analysis is complete; no file has been written.");
      addNotification({ dedupeKey: "smart-learning-scan", level: "success", title: lang === "zh" ? "项目理解已生成" : "Project understanding generated", message: lang === "zh" ? "分析在本机完成；敏感路径、依赖目录、源码正文、会话正文与凭据均被排除。" : "Analysis completed locally; sensitive paths, dependencies, source bodies, session bodies, and credentials were excluded.", receipt: { requestId, targetId: input.project_id } });
    } catch (error) {
      setResult(null);
      setAnalysis(null);
      const failure = presentOperationFailure("learning", error, lang);
      setNotice(failure.message);
      addNotification({ dedupeKey: "smart-learning-scan-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: input.project_id } });
    } finally {
      setRunning(false);
    }
  };

  const generatePrompt = () => {
    if (!activeResult || !input || !analysisResult) {
      setActionFeedback({ level: "error", message: lang === "zh" ? "当前没有可生成草稿的真实项目分析结果。" : "No real project analysis is available for a document draft." });
      return;
    }
    setMode("prompt");
    setManagedText(buildManagedLearningText(analysisResult.context, activeResult.recommendation, lang));
    setDraftRevealToken((token) => token + 1);
    setActionFeedback({ level: "success", message: lang === "zh" ? "文档草稿已生成并显示在下方；尚未写入任何文件。" : "The document draft is ready below; no file has been written." });
    addNotification({
      dedupeKey: `learning-draft-${input.project_id}`,
      level: "info",
      title: lang === "zh" ? "项目文档草稿已生成" : "Project document draft generated",
      message: lang === "zh" ? "草稿来自当前真实分析结果。确认内容后才能进入受控写入。" : "The draft is derived from the current real analysis. Confirm its contents before a managed write.",
      receipt: { requestId: createOperationRequestId("learning-draft"), targetId: input.project_id },
    });
  };

  const previewManagedBlock = async (content = managedText) => {
    const candidate = candidates.find((item) => item.write_eligible && selected.includes(item.id));
    const targetRoot = projectRoot.trim() || candidate?.path || "";
    if (!targetRoot || !content.trim()) {
      setActionFeedback({ level: "error", message: lang === "zh" ? "缺少已验证的项目目录或文档草稿，无法准备写入预览。" : "A verified project directory or document draft is missing, so the write preview cannot be prepared." });
      return;
    }
    const requestId = createOperationRequestId("learning-write-preview");
    const target = `${targetRoot.replace(/[\\/]+$/g, "")}/${targetName}`;
    setSaving(true);
    setActionFeedback({ level: "pending", message: lang === "zh" ? "正在准备受控写入差异预览…" : "Preparing the managed-write diff…" });
    try {
      const next = await invokeNative<LearningReview>("preview_learning_managed_block", { request: { target, content, reviewed_before_sha256: null, reviewed_after_sha256: null } });
      if (!next.ok) throw new Error(next.error ?? "managed_write_preview_failed");
      setManagedText(content);
      setWriteError("");
      setReview(next);
      setActionFeedback({ level: "success", message: lang === "zh" ? "差异预览已准备；确认前不会写入任何文件。" : "The diff is ready; no file will be written before confirmation." });
    } catch (error) {
      const failure = presentOperationFailure("learning", error, lang);
      setActionFeedback({ level: "error", message: failure.message });
      setNotice(failure.message);
      addNotification({ dedupeKey: "learning-write-preview-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: target } });
    } finally {
      setSaving(false);
    }
  };

  const save = async () => {
    if (!activeResult || !input || !analysisResult) {
      setActionFeedback({ level: "error", message: lang === "zh" ? "当前没有可保存的真实项目理解。" : "No real project understanding is available to save." });
      return;
    }
    const content = managedText.trim()
      ? managedText
      : buildManagedLearningText(analysisResult.context, activeResult.recommendation, lang);
    await previewManagedBlock(content);
  };

  const applyManagedBlock = async () => {
    if (!review) return;
    const requestId = createOperationRequestId("learning-managed-write");
    setApplying(true);
    setWriteError("");
    try {
      const receipt = await invokeNative<LearningReceipt>("apply_learning_managed_block", { request: { target: review.target, content: managedText, reviewed_before_sha256: review.before_sha256, reviewed_after_sha256: review.after_sha256 } });
      if (!receipt.ok) throw new Error(receipt.summary ?? receipt.state);
      const data = { project: { root: effectiveProjectRoot, project_id: input?.project_id }, analysis: analysisResult, recommendation: activeResult?.recommendation, mapping: activeResult?.mapping, source: activeResult?.source, privacy: activeResult?.privacy };
      let localStateSaved = false;
      try {
        const action = await invokeNative<NativeActionResult>("save_app_settings", { section: "smart-learning", payload: data });
        localStateSaved = action.ok;
        if (action.ok) setSaved(data);
      } catch {
        localStateSaved = false;
      }
      addNotification({ dedupeKey: `learning-write-${receipt.rollback_id}`, level: "success", title: lang === "zh" ? "受控学习区块已写入" : "Managed learning block written", message: lang === "zh" ? "学习区块已原子写入、验证并创建回滚点。" : "The learning block was written atomically, verified, and given a rollback point.", persistent: true, details: `before=${receipt.before_sha256?.slice(0, 12)} · after=${receipt.after_sha256?.slice(0, 12)}`, receipt: { requestId, transactionId: receipt.transaction_id, rollbackId: receipt.rollback_id, targetId: receipt.target } });
      const completion = localStateSaved
        ? (lang === "zh" ? "写入已原子完成并验证；项目理解已持久化，回滚点可在“变更与回退”查看。" : "The write was verified, the project understanding was persisted, and its rollback point is available in Changes & Rollback.")
        : (lang === "zh" ? "文件写入已验证并创建回滚点，但 d-token 本地理解缓存未能同步保存。" : "The file write was verified with a rollback point, but the local d-token understanding cache could not be saved.");
      setNotice(completion);
      setActionFeedback({ level: localStateSaved ? "success" : "warning", message: completion });
      setReview(null);
      await refresh();
    } catch (error) {
      const failure = presentOperationFailure("learning", error, lang);
      setWriteError(failure.message);
      setActionFeedback({ level: "error", message: failure.message });
      addNotification({ dedupeKey: "learning-write-failed", level: "error", title: failure.title, message: failure.message, details: failure.details, persistent: true, receipt: { requestId, targetId: review.target } });
    } finally {
      setApplying(false);
    }
  };

  const startDelegation = async () => {
    const requestId = createOperationRequestId("learning-delegation");
    setDelegationRequestId(requestId);
    setDelegationRunning(true);
    try {
      const receipt = await invokeNative<DelegationReceipt>("spawn_learning_delegation", { request: { request_id: requestId, agent: delegationAgent, project_root: effectiveProjectRoot, prompt: managedText, timeout_ms: 120_000, confirmed: true } });
      if (receipt.ok && receipt.context) {
        setManagedText((current) => mergeDelegatedContext(current, receipt.context!, lang));
        setNotice(lang === "zh" ? "Agent 已返回经过校验的结构化项目理解；原始模型输出已丢弃。确认草稿后可继续写入。" : "The Agent returned a validated structured project context; raw model output was discarded. Confirm the draft before writing it.");
      }
      addNotification({ dedupeKey: `learning-delegation-${requestId}`, level: receipt.ok ? "success" : "warning", title: receipt.ok ? (lang === "zh" ? "直接委派已完成" : "Direct delegation complete") : (lang === "zh" ? "直接委派未完成" : "Direct delegation did not complete"), message: receipt.ok ? (lang === "zh" ? "安全结构化结果已合并到草稿。确认内容后才能写入。" : "The safe structured result was merged into the draft. Confirm its contents before writing.") : (lang === "zh" ? "没有得到符合安全结构的结果，原始输出已丢弃。" : "No safe structured result was produced; raw output was discarded."), persistent: true, details: `state=${receipt.state} · elapsed_ms=${receipt.elapsed_ms ?? "—"} · raw_output_persisted=false`, receipt: { requestId, targetId: delegationAgent } });
      setDelegationReview(false);
    } catch (error) {
      const failure = presentOperationFailure("learning", error, lang);
      addNotification({ dedupeKey: `learning-delegation-${requestId}-failed`, level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: delegationAgent } });
    } finally {
      setDelegationRunning(false);
    }
  };

  const cancelDelegation = async () => {
    if (!delegationRequestId) return;
    const requestId = createOperationRequestId("learning-delegation-cancel");
    try {
      await invokeNative("cancel_learning_delegation", { requestId: delegationRequestId });
      addNotification({ dedupeKey: `learning-delegation-cancel-${delegationRequestId}`, level: "info", title: lang === "zh" ? "委派取消请求已发送" : "Delegation cancellation requested", message: lang === "zh" ? "正在停止本地 Agent 任务；不会保留原始模型输出。" : "The local Agent task is stopping; raw model output is not retained.", receipt: { requestId, targetId: delegationAgent } });
    } catch (error) {
      const failure = presentOperationFailure("learning", error, lang);
      addNotification({ dedupeKey: `learning-delegation-cancel-${delegationRequestId}-failed`, level: "error", title: failure.title, message: failure.message, details: failure.details, receipt: { requestId, targetId: delegationAgent } });
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-base font-semibold">{t("sl.title")}</h2>
          <p className="mt-1 text-sm text-muted-foreground">{t("sl.subtitle")}</p>
        </div>
        <Badge variant={learningDiscovery.snapshot?.state === "complete" ? "default" : "outline"}>
          {learningDiscoveryStatusLabel(learningDiscovery.snapshot?.state, lang)}
        </Badge>
      </div>

      <div className="flex items-center gap-2 rounded-lg border border-border/70 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        <ShieldCheck className="size-3.5 shrink-0 text-emerald-600" />
        <span>{lang === "zh" ? "本地枚举受支持 Agent 的会话记录，只索引路径、时间、数量和格式状态等白名单元数据；不返回或缓存原始提示词、响应、源码与凭据。" : "Supported Agent sessions are enumerated locally using allowlisted path, time, count, and format metadata; raw prompts, responses, source code, and credentials are not returned or cached."}</span>
      </div>

      <Surface className="p-4" hover={false}>
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div className="text-sm">
            <span className="font-medium">{lang === "zh" ? "会话发现" : "Session discovery"}</span>
            <span className="ml-2 text-xs text-muted-foreground">
              {learningDiscovery.snapshot
                ? `${learningDiscovery.snapshot.progress.scanned_agents} ${lang === "zh" ? "个来源已检查" : "sources checked"} · ${learningDiscovery.snapshot.progress.session_records} ${lang === "zh" ? "条已枚举记录" : "enumerated records"} · ${candidates.length} ${lang === "zh" ? "个项目" : "projects"}`
                : (lang === "zh" ? "正在准备会话发现" : "Preparing session discovery")}
            </span>
            {learningDiscovery.snapshot && (
              <span className="ml-2 text-xs text-muted-foreground">
                {learningStageLabel(learningDiscovery.snapshot.progress.current_stage, lang)} · {learningCacheStateLabel(learningDiscovery.snapshot.cache_state, learningDiscovery.running, lang)}
              </span>
            )}
          </div>
          <div className="flex gap-2">
            {learningDiscovery.running && <Button size="sm" variant="outline" onClick={() => void cancelDiscovery()} disabled={discoveryAction !== ""} aria-busy={discoveryAction === "cancel"}>{discoveryAction === "cancel" ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}{discoveryAction === "cancel" ? (lang === "zh" ? "停止中…" : "Stopping…") : (lang === "zh" ? "停止扫描" : "Stop scan")}</Button>}
            <Button size="sm" variant="outline" onClick={() => void runDiscovery(true, effectiveProjectRoot ? [effectiveProjectRoot] : [])} disabled={learningDiscovery.running || discoveryAction !== ""} aria-busy={discoveryAction === "start"}>
              <RefreshCw className={`size-3.5 ${learningDiscovery.running || discoveryAction === "start" ? "animate-spin" : ""}`} />
              {discoveryAction === "start" ? (lang === "zh" ? "启动中…" : "Starting…") : (lang === "zh" ? "重新扫描" : "Rescan")}
            </Button>
          </div>
        </div>
        {learningDiscovery.running && (
          <div className="mb-3 h-1.5 overflow-hidden rounded-full bg-muted" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progressPercent} aria-label={lang === "zh" ? "智能学习扫描进度" : "Smart Learning scan progress"}>
            <div className="h-full rounded-full bg-primary transition-[width] duration-200" style={{ width: `${progressPercent}%` }} />
          </div>
        )}
        {learningDiscovery.snapshot?.sources.length ? (
          <div className="mb-3 divide-y divide-border/60 border-y border-border/60">
            {learningDiscovery.snapshot.sources.map((source) => (
              <div key={source.id} className="grid gap-1 py-2 text-xs sm:grid-cols-[minmax(120px,0.55fr)_minmax(0,1fr)_auto] sm:items-center">
                <span className="font-medium">{source.label}</span>
                <span className="text-muted-foreground">{sourceSummary(source, lang)}</span>
                <span className="text-muted-foreground">{sourceStateLabel(source.state, lang)}</span>
              </div>
            ))}
          </div>
        ) : null}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="min-w-0 text-xs text-muted-foreground">
            {selectedProject
              ? (lang === "zh" ? `已选择 ${selectedProject.path}；深度分析仅在点击后开始。` : `Selected ${selectedProject.path}; deep analysis starts only after you click.`)
              : (lang === "zh" ? "选择一个真实项目后再做深度分析；自动发现阶段不读取项目正文。" : "Select a real project before deep analysis; automatic discovery does not read project bodies.")}
          </div>
          <Button size="sm" onClick={() => void scan()} disabled={running || !input}>
            {running ? <RefreshCw className="size-3.5 animate-spin" /> : <Search className="size-3.5" />}
            {running ? (lang === "zh" ? "分析中" : "Analyzing") : (lang === "zh" ? "理解选中项目" : "Understand selected project")}
          </Button>
        </div>
        <details className="group mt-3 text-xs text-muted-foreground">
          <summary className="cursor-pointer list-none font-medium text-foreground">{lang === "zh" ? "没有找到目标项目？添加目录" : "Project not found? Add a directory"}</summary>
          <div className="mt-2 flex flex-wrap gap-2">
            <Input value={projectRoot} onChange={(event) => { setProjectRoot(event.target.value); setAnalysis(null); setResult(null); setManagedText(""); }} placeholder={lang === "zh" ? "输入真实项目目录" : "Enter a real project path"} className="min-w-64 flex-1" />
            <Button size="sm" variant="outline" onClick={() => void runDiscovery(true, projectRoot.trim() ? [projectRoot.trim()] : [])} disabled={learningDiscovery.running || discoveryAction !== "" || !projectRoot.trim()} aria-busy={discoveryAction === "start"}>{discoveryAction === "start" ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}{discoveryAction === "start" ? (lang === "zh" ? "验证中…" : "Verifying…") : (lang === "zh" ? "验证并加入" : "Verify and add")}</Button>
          </div>
        </details>
        {notice && <p className="mt-3 text-xs text-muted-foreground">{notice}</p>}
      </Surface>

      <div className="grid gap-4 lg:grid-cols-2">
        <Surface className="p-4" hover={false}>
          <h3 className="mb-2 text-sm font-medium">{lang === "zh" ? "发现的项目" : "Discovered projects"}</h3>
          {candidates.length > 0 ? (
            <div className="space-y-2">
              <Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={lang === "zh" ? "搜索项目、Agent 或技术栈" : "Search project, Agent, or stack"} />
              <div role="radiogroup" aria-label={lang === "zh" ? "选择需要理解的项目" : "Select a project to understand"}>
                {visibleCandidates.map((candidate) => (
                  <LearningCandidateRow key={candidate.id} candidate={candidate} selected={selected.includes(candidate.id)} lang={lang} onSelect={() => { setSelected([candidate.id]); setProjectRoot(""); setAnalysis(null); setResult(null); setManagedText(""); }} />
                ))}
              </div>
              {projectPageCount > 1 && (
                <div className="flex items-center justify-between gap-2 border-t border-border/60 pt-2 text-xs text-muted-foreground">
                  <span>{lang === "zh" ? `第 ${projectPage + 1} / ${projectPageCount} 页 · 共 ${filteredCandidates.length} 个项目` : `Page ${projectPage + 1} of ${projectPageCount} · ${filteredCandidates.length} projects`}</span>
                  <div className="flex gap-1.5">
                    <Button size="sm" variant="outline" onClick={() => setProjectPage((page) => Math.max(0, page - 1))} disabled={projectPage === 0}>{lang === "zh" ? "上一页" : "Previous"}</Button>
                    <Button size="sm" variant="outline" onClick={() => setProjectPage((page) => Math.min(projectPageCount - 1, page + 1))} disabled={projectPage >= projectPageCount - 1}>{lang === "zh" ? "下一页" : "Next"}</Button>
                  </div>
                </div>
              )}
            </div>
          ) : <EmptyState status={learningDiscovery.running ? "loading" : "empty"} title={learningDiscovery.running ? (lang === "zh" ? "正在关联会话与项目" : "Linking sessions to projects") : (lang === "zh" ? "尚未从可识别会话中关联到真实项目" : "No real project could be linked from recognizable sessions yet")} />}
        </Surface>
        <Surface className="p-4" hover={false}>
          <h3 className="mb-2 text-sm font-medium">{t("sl.output")}</h3>
          {analysisResult ? (
            <div className="space-y-3 text-sm">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="font-medium">{analysisResult.context.project_name}</span>
                <div className="flex items-center gap-2"><Badge variant="secondary">{analysisResult.index.score}/100</Badge>{recommendation && <Badge>{String(recommendation.recommended_level ?? "unknown")}</Badge>}</div>
              </div>
              <p className="text-xs leading-5 text-muted-foreground">{localizeContextText(analysisResult.context.summary, lang)}</p>
              <div className="flex flex-wrap gap-1.5">
                {analysisResult.context.tech_stack.length > 0 ? analysisResult.context.tech_stack.map((item) => <Badge key={item} variant="outline">{item}</Badge>) : <span className="text-xs text-muted-foreground">{lang === "zh" ? "未从安全标记中识别技术栈" : "No stack identified from safe markers"}</span>}
              </div>
              <div className="grid gap-3 border-y border-border/60 py-3 sm:grid-cols-2">
                <LearningEvidenceList title={lang === "zh" ? "常用命令" : "Common commands"} items={analysisResult.context.common_commands} empty={lang === "zh" ? "未发现可信命令入口" : "No trusted command entry found"} />
                <LearningEvidenceList title={lang === "zh" ? "风险与缺口" : "Risks and gaps"} items={[...analysisResult.context.risks, ...analysisResult.context.open_questions].map((item) => localizeContextText(item, lang))} empty={lang === "zh" ? "未发现需要提示的风险" : "No notable risk found"} />
              </div>
              <div className="text-xs text-muted-foreground">
                {lang === "zh"
                  ? `${analysisResult.index.file_count} 个文件已纳入有界结构索引 · ${analysisResult.context.source_summary.sensitive_entries_skipped} 个敏感项已排除 · 未读取源码正文或会话正文`
                  : `${analysisResult.index.file_count} files in the bounded structural index · ${analysisResult.context.source_summary.sensitive_entries_skipped} sensitive entries excluded · no source or session bodies read`}
              </div>
              {activeResult && (
                <div className="space-y-2">
                  <div className="flex flex-wrap gap-2">
                    <Button size="sm" onClick={() => void save()} disabled={saving || applying}>
                      {saving ? <RefreshCw className="size-3.5 animate-spin" /> : null}
                      {saving ? (lang === "zh" ? "正在准备写入预览" : "Preparing write preview") : (lang === "zh" ? "保存项目理解" : "Save project understanding")}
                    </Button>
                    <Button size="sm" variant="outline" onClick={generatePrompt} disabled={saving || applying}>{lang === "zh" ? "生成文档草稿" : "Generate document draft"}</Button>
                  </div>
                  {actionFeedback && (
                    <p className={`text-xs ${actionFeedback.level === "error" ? "text-destructive" : "text-muted-foreground"}`} role={actionFeedback.level === "error" ? "alert" : "status"} aria-live="polite">
                      {actionFeedback.message}
                    </p>
                  )}
                </div>
              )}
            </div>
          ) : <EmptyState status="empty" title={lang === "zh" ? "尚无真实分析结果" : "No real analysis result yet"} />}
        </Surface>
      </div>
      {managedText && (
        <div ref={draftSectionRef}>
          <Surface className="space-y-3 p-4" hover={false}>
            <div className="flex flex-wrap items-center justify-between gap-2"><div className="flex items-center gap-1"><Button size="sm" variant={mode === "prompt" ? "default" : "outline"} onClick={() => setMode("prompt")}>{lang === "zh" ? "提示词模式（默认）" : "Prompt mode (default)"}</Button><Button size="sm" variant={mode === "direct" ? "default" : "outline"} onClick={() => setMode("direct")}>{lang === "zh" ? "直接委派" : "Direct delegation"}</Button></div>{mode === "prompt" ? <select className="rounded-md border border-border bg-background px-2 py-1 text-sm" value={targetName} onChange={(event) => setTargetName(event.target.value)}>{["AGENTS.md", "CLAUDE.md", "GEMINI.md", "d-token.md"].map((name) => <option key={name}>{name}</option>)}</select> : <select className="rounded-md border border-border bg-background px-2 py-1 text-sm" value={delegationAgent} onChange={(event) => setDelegationAgent(event.target.value)}>{delegationAgents.map((agent) => <option key={agent.id} value={agent.id} disabled={!agent.available}>{agent.id} · {agent.available ? (lang === "zh" ? "可用" : "available") : (lang === "zh" ? "不可用" : "unavailable")}</option>)}</select>}</div>
            <textarea aria-label={lang === "zh" ? "项目理解文档草稿" : "Project understanding document draft"} className="min-h-36 w-full rounded-md border border-border bg-background p-3 text-sm" value={managedText} onChange={(event) => setManagedText(event.target.value)} />
            <div className="flex justify-end"><Button onClick={() => mode === "prompt" ? void previewManagedBlock() : setDelegationReview(true)} disabled={saving || (mode === "prompt" ? selected.length > 1 || !effectiveProjectRoot : !delegationAgents.some((agent) => agent.id === delegationAgent && agent.available))}>{saving ? <RefreshCw className="size-3.5 animate-spin" /> : null}{mode === "prompt" ? (lang === "zh" ? "预览受控写入" : "Preview managed write") : (lang === "zh" ? "确认直接委派" : "Confirm direct delegation")}</Button></div>
          </Surface>
        </div>
      )}
      <OperationDialog open={review !== null} onOpenChange={(open) => { if (!open && !applying) { setReview(null); setWriteError(""); } }} title={lang === "zh" ? "确认写入 d-token 管理区块" : "Confirm d-token managed block write"} summary={lang === "zh" ? "只新增或更新 d-token 标记区块；区块外内容保持不变。" : "Only the d-token marked block is added or updated; content outside it remains unchanged."} impact={review?.target} confirmLabel={lang === "zh" ? "确认原子写入" : "Confirm atomic write"} cancelLabel={t("common.cancel")} risk="warning" pending={applying} confirmDisabled={!review} onConfirm={applyManagedBlock} details={review ? <div className="space-y-2">{writeError && <p className="text-xs text-destructive" role="alert">{writeError}</p>}<pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md bg-muted/50 p-3 text-xs">{review.diff}</pre></div> : null} />
      <OperationDialog open={delegationReview} onOpenChange={setDelegationReview} title={lang === "zh" ? `确认委派给 ${delegationAgent}？` : `Delegate to ${delegationAgent}?`} summary={lang === "zh" ? "Agent 只在选中项目目录运行，提示词直接传入进程，不经过命令行解释。原始模型输出会在内存中有界读取后立即丢弃，只返回经过字段、长度和敏感内容校验的结构化项目理解。" : "The Agent runs only in the selected project directory, and the prompt is passed straight to the process without shell interpretation. Raw output is read with an in-memory bound and discarded; only a field-, length-, and sensitivity-validated project context is returned."} impact={`${effectiveProjectRoot} · ${managedText.length} bytes · timeout 120s`} confirmLabel={lang === "zh" ? "确认直接委派" : "Confirm delegation"} cancelLabel={t("common.cancel")} risk="warning" pending={delegationRunning} onConfirm={startDelegation} details={delegationRunning ? <Button variant="outline" onClick={() => void cancelDelegation()}>{lang === "zh" ? "取消正在运行的委派" : "Cancel running delegation"}</Button> : null} />
    </div>
  );
}

function useLearningDiscovery() {
  const [snapshot, setSnapshot] = useState<LearningDiscovery | null>(null);
  const jobIdRef = useRef("");
  const mountedRef = useRef(false);
  const runningRef = useRef(false);

  const accept = useCallback((next: LearningDiscovery | null) => {
    if (!next || !next.job_id || !Array.isArray(next.candidates) || !Array.isArray(next.sources)) return;
    if (jobIdRef.current && next.job_id !== jobIdRef.current) return;
    jobIdRef.current = next.job_id;
    runningRef.current = ["running", "cancelling"].includes(next.state);
    if (mountedRef.current) setSnapshot(next);
  }, []);

  const start = useCallback(async (refresh = false, projectRoots: string[] = []) => {
    if (jobIdRef.current && runningRef.current) {
      await invokeNative("cancel_learning_discovery", { jobId: jobIdRef.current }).catch(() => undefined);
    }
    const next = await invokeNative<LearningDiscovery>("start_learning_discovery", {
      request: { refresh, project_roots: projectRoots, skip_paths: [] },
    });
    if (next?.job_id && Array.isArray(next.candidates) && Array.isArray(next.sources)) {
      jobIdRef.current = next.job_id;
      runningRef.current = ["running", "cancelling"].includes(next.state);
      if (mountedRef.current) setSnapshot(next);
    }
    return next;
  }, []);

  const cancel = useCallback(async () => {
    if (!jobIdRef.current) return null;
    const next = await invokeNative<LearningDiscovery>("cancel_learning_discovery", { jobId: jobIdRef.current });
    accept(next);
    return next;
  }, [accept]);

  useEffect(() => {
    mountedRef.current = true;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) => listen<LearningDiscovery>("dtoken:learning-discovery", (event) => accept(event.payload)))
      .then((stop) => { unlisten = stop; })
      .catch(() => undefined);
    void start(false).catch(() => undefined);
    return () => {
      mountedRef.current = false;
      unlisten?.();
      if (jobIdRef.current && runningRef.current) void invokeNative("cancel_learning_discovery", { jobId: jobIdRef.current }).catch(() => undefined);
    };
  }, [accept, start]);

  useEffect(() => {
    if (!snapshot || !["running", "cancelling"].includes(snapshot.state)) return;
    const timer = window.setTimeout(() => {
      void invokeNative<LearningDiscovery>("get_learning_discovery_status", { jobId: snapshot.job_id })
        .then(accept)
        .catch(() => undefined);
    }, 1200);
    return () => window.clearTimeout(timer);
  }, [accept, snapshot]);

  return {
    snapshot,
    running: Boolean(snapshot && ["running", "cancelling"].includes(snapshot.state)),
    start,
    cancel,
  };
}

function sourceStateLabel(state: string, lang: "zh" | "en") {
  const labels: Record<string, [string, string]> = {
    scanned: ["已扫描", "Scanned"],
    partial: ["部分完成", "Partial"],
    missing: ["未发现", "Missing"],
    unsupported: ["格式待支持", "Unsupported format"],
    error: ["读取失败", "Read error"],
  };
  const label = labels[state] ?? ["未知", "Unknown"];
  return lang === "zh" ? label[0] : label[1];
}

function learningDiscoveryStatusLabel(state: string | undefined, lang: "zh" | "en") {
  const labels: Record<string, [string, string]> = {
    running: ["正在扫描会话目录", "Scanning session roots"],
    cancelling: ["正在停止扫描", "Stopping discovery"],
    complete: ["本机项目发现完成", "Local project discovery complete"],
    partial: ["本机项目发现部分完成", "Local project discovery partially complete"],
    cancelled: ["扫描已停止", "Discovery stopped"],
    error: ["本机项目发现失败", "Local project discovery failed"],
    failed: ["本机项目发现失败", "Local project discovery failed"],
  };
  const label = labels[state ?? ""] ?? ["等待本机发现", "Waiting for local discovery"];
  return lang === "zh" ? label[0] : label[1];
}

function learningCacheStateLabel(state: string, running: boolean, lang: "zh" | "en") {
  const labels: Record<string, [string, string]> = {
    fresh: ["缓存已更新", "cache fresh"],
    stale: running ? ["正在更新旧缓存", "refreshing stale cache"] : ["缓存需要刷新", "cache needs refresh"],
    unavailable: ["缓存不可用，结果未持久化", "cache unavailable; result not persisted"],
    missing: running ? ["正在建立缓存", "building cache"] : ["尚无缓存", "no cache yet"],
  };
  const label = labels[state] ?? ["缓存状态未知", "cache state unknown"];
  return lang === "zh" ? label[0] : label[1];
}

function sourceSummary(source: LearningSourceState, lang: "zh" | "en") {
  if (source.state === "scanned") {
    const linked = source.linked_count ?? (source.project_count > 0 ? source.session_count : 0);
    const unlinked = source.unlinked_count ?? (source.project_count > 0 ? 0 : source.session_count);
    const corrupt = source.corrupt_count ?? 0;
    const subagents = source.subagent_count ?? 0;
    return lang === "zh"
      ? `${source.session_count} 条已枚举 · ${linked} 条已关联 · ${unlinked} 条未关联 · ${subagents} 条子代理 · ${corrupt} 条损坏`
      : `${source.session_count} enumerated · ${linked} linked · ${unlinked} unlinked · ${subagents} subagents · ${corrupt} corrupt`;
  }
  if (source.state === "unsupported") {
    return source.roots_found > 0
      ? (lang === "zh" ? `发现 ${source.roots_found} 个目录，尚未解析会话格式` : `${source.roots_found} roots found; session format not parsed yet`)
      : (lang === "zh" ? "已识别工具，尚未支持会话格式" : "Tool detected; session format is not supported yet");
  }
  if (source.state === "missing") {
    return lang === "zh" ? "本机未发现会话目录" : "No session directory found on this device";
  }
  return lang === "zh" ? "未能读取该来源的白名单元数据" : "Could not read allowlisted metadata from this source";
}

function LearningCandidateRow({ candidate, selected, lang, onSelect }: {
  candidate: LearningCandidate;
  selected: boolean;
  lang: "zh" | "en";
  onSelect: () => void;
}) {
  const projectName = candidate.path.replace(/[\\/]+$/g, "").split(/[\\/]/).filter(Boolean).pop() ?? candidate.path;
  const reasons = (candidate.ranking_reasons ?? candidate.reasons).slice(0, 2);
  return (
    <label className={`flex cursor-pointer gap-3 border-b border-border/60 py-3 transition-colors last:border-b-0 ${selected ? "text-foreground" : "text-muted-foreground hover:text-foreground"}`}>
      <input className="mt-1 size-4 accent-primary" type="radio" checked={selected} onChange={onSelect} />
      <span className="min-w-0 flex-1">
        <span className="flex flex-wrap items-center gap-2">
          <span className="truncate text-sm font-medium">{projectName}</span>
          {typeof candidate.score === "number" && <Badge variant={selected ? "default" : "secondary"}>{candidate.score}/100</Badge>}
          <span className="text-xs">{candidate.agent}</span>
        </span>
        <span className="mt-1 block truncate text-xs">{candidate.path}</span>
        <span className="mt-1.5 flex flex-wrap gap-x-3 gap-y-1 text-xs">
          {candidate.session_count > 0 && <span>{candidate.session_count} {lang === "zh" ? "条会话记录" : "session records"}</span>}
          {(candidate.language_fingerprints ?? []).slice(0, 3).map((item) => <span key={item}>{item}</span>)}
          {reasons.map((reason) => <span key={reason}>{candidateReasonLabel(reason, lang)}</span>)}
        </span>
      </span>
    </label>
  );
}

function LearningEvidenceList({ title, items, empty }: { title: string; items: string[]; empty: string }) {
  return (
    <div>
      <h4 className="mb-1.5 text-xs font-medium text-foreground">{title}</h4>
      {items.length > 0
        ? <ul className="space-y-1 text-xs leading-5 text-muted-foreground">{items.slice(0, 8).map((item) => <li key={item}>· {item}</li>)}</ul>
        : <p className="text-xs text-muted-foreground">{empty}</p>}
    </div>
  );
}

function learningStageLabel(stage: string, lang: "zh" | "en") {
  const labels: Record<string, [string, string]> = {
    preparing: ["正在准备", "Preparing"],
    "identifying-agent-sources": ["正在识别 Agent 来源", "Identifying Agent sources"],
    "scanning-session-metadata": ["正在扫描会话元数据", "Scanning session metadata"],
    "scoring-candidates": ["正在评估项目", "Scoring projects"],
    complete: ["发现完成", "Discovery complete"],
    partial: ["部分完成", "Partially complete"],
    cancelling: ["正在停止", "Stopping"],
    cancelled: ["已停止", "Stopped"],
  };
  const label = labels[stage] ?? ["正在发现", "Discovering"];
  return lang === "zh" ? label[0] : label[1];
}

function candidateReasonLabel(reason: string, lang: "zh" | "en") {
  const labels: Record<string, [string, string]> = {
    "recent-activity": ["近期活跃", "Recently active"],
    "agent-sessions": ["有 Agent 会话", "Agent sessions"],
    "multi-agent": ["多 Agent 使用", "Multiple Agents"],
    "git-project": ["Git 项目", "Git project"],
    "missing-agent-doc": ["缺少 Agent 说明", "Agent guide missing"],
    "bounded-index-partial": ["索引部分完成", "Partial index"],
  };
  const label = labels[reason] ?? [reason, reason];
  return lang === "zh" ? label[0] : label[1];
}

function localizeContextText(text: string, lang: "zh" | "en") {
  if (lang === "en") return text;
  const exact: Record<string, string> = {
    "The bounded scan reached its entry limit; results are partial": "有界扫描已达到条目上限，当前结果不完整",
    "No README was found in the bounded project scan": "有界项目扫描中未发现 README",
    "No project-level Agent instruction document was found": "未发现项目级 Agent 指令文档",
  };
  if (exact[text]) return exact[text];
  return text
    .replace(/^(\d+) sensitive entries were excluded from analysis$/, "$1 个敏感项已从分析中排除")
    .replace(/^(\d+) entries were unreadable and skipped$/, "$1 个条目无法读取，已跳过")
    .replace(/^Top-level `(.+)` directory is present$/, "识别到顶层 `$1` 目录");
}

function buildManagedLearningText(context: LearningContext, recommendation: Record<string, any>, lang: "zh" | "en") {
  const section = (title: string, items: string[], empty: string) => [
    `## ${title}`,
    ...(items.length > 0 ? items.slice(0, 16).map((item) => `- ${item}`) : [`- ${empty}`]),
  ].join("\n");
  const level = String(recommendation?.recommended_level ?? "unknown");
  const reasonItems = Array.isArray(recommendation?.reasons)
    ? recommendation.reasons.map(String)
    : [];
  const blocks = lang === "zh"
    ? [
      `# ${context.project_name} 项目理解`,
      context.summary,
      section("技术栈", context.tech_stack, "未从安全结构标记中识别"),
      section("结构与重要文件", [...context.architecture_notes, ...context.important_files], "暂无可信结构证据"),
      section("常用命令", context.common_commands, "未发现可信命令入口"),
      section("项目约定", context.conventions, "未发现可确认的项目约定"),
      section("风险与待确认事项", [...context.risks, ...context.open_questions].map((item) => localizeContextText(item, lang)), "当前未发现需要提示的风险"),
      section("Agent 工作约束", context.agent_instructions, "遵循仓库现有指令并验证变更"),
      section("d-token 建议", context.dtoken_recommendations, "保持显式路由与可回滚写入"),
      section("上下文优化策略", [`建议级别：${level}`, ...reasonItems], "没有足够证据推荐主动优化"),
    ]
    : [
      `# ${context.project_name} project understanding`,
      context.summary,
      section("Technology stack", context.tech_stack, "Not identified from safe structure markers"),
      section("Structure and important files", [...context.architecture_notes, ...context.important_files], "No trusted structure evidence"),
      section("Common commands", context.common_commands, "No trusted command entry found"),
      section("Project conventions", context.conventions, "No confirmed project conventions found"),
      section("Risks and open questions", [...context.risks, ...context.open_questions], "No notable risk found"),
      section("Agent constraints", context.agent_instructions, "Follow repository instructions and verify changes"),
      section("d-token recommendations", context.dtoken_recommendations, "Keep routing explicit and writes reversible"),
      section("Context optimization policy", [`Recommended level: ${level}`, ...reasonItems], "Insufficient evidence for active optimization"),
    ];
  const text = blocks.join("\n\n").trim();
  return text.length <= 16_000 ? text : `${text.slice(0, 15_900)}\n\n- ${lang === "zh" ? "草稿已按 16 KiB 安全上限截断" : "Draft truncated at the 16 KiB safety limit"}`;
}

function mergeDelegatedContext(current: string, context: DelegatedLearningContext, lang: "zh" | "en") {
  const marker = lang === "zh" ? "## Agent 委派补充" : "## Agent delegation supplement";
  const base = current.split(marker)[0].trimEnd();
  const list = (title: string, values: string[]) => [
    `### ${title}`,
    ...(values.length > 0 ? values.map((value) => `- ${value}`) : [`- ${lang === "zh" ? "无" : "None"}`]),
  ].join("\n");
  const supplement = lang === "zh"
    ? [marker, context.summary, list("项目约定", context.conventions), list("风险", context.risks), list("建议", context.recommendations), list("待确认问题", context.open_questions)]
    : [marker, context.summary, list("Conventions", context.conventions), list("Risks", context.risks), list("Recommendations", context.recommendations), list("Open questions", context.open_questions)];
  const merged = `${base}\n\n${supplement.join("\n\n")}`.trim();
  return merged.length <= 16_000 ? merged : current;
}

type LearningCandidate = {
  id: string;
  path: string;
  agent: string;
  kind: "session-root" | "project";
  mtime?: number;
  activity: string;
  confidence: string;
  value: string;
  reasons: string[];
  freshness: string;
  session_count: number;
  project_count: number;
  skipped_count: number;
  write_eligible: boolean;
  score?: number;
  project_markers?: string[];
  language_fingerprints?: string[];
  has_agent_doc?: boolean;
  risk_flags?: string[];
  file_count_bucket?: string;
  ranking_reasons?: string[];
};
type LearningSourceState = { id: string; label: string; state: string; roots_found: number; session_count: number; conversation_count?: number; subagent_count?: number; linked_count?: number; unlinked_count?: number; corrupt_count?: number; unsupported_count?: number; truncated?: boolean; project_count: number; skipped_count: number; detail: string };
type LearningProgress = { scanned_agents: number; total_agents: number; session_records: number; parsed_projects: number; candidate_count: number; skipped: number; risks: number; current_stage: string; cancellable: boolean };
type LearningDiscovery = { ok: boolean; job_id: string; state: string; phase: string; source: string; freshness: string; cache_state: string; progress: LearningProgress; sources: LearningSourceState[]; candidates: LearningCandidate[]; writes_performed: false; privacy: string };
type LearningSourceSummary = { readme: boolean; docs: number; scripts: number; configs: number; agent_docs: string[]; sensitive_entries_skipped: number; unreadable_entries: number; truncated: boolean };
type LearningProjectIndex = { project_name: string; project_path: string; last_modified_unix?: number | null; file_count: number; file_count_bucket: string; has_git: boolean; project_markers: string[]; agent_docs: string[]; language_fingerprints: string[]; top_level_directories: string[]; score: number; ranking_reasons: string[]; risk_flags: string[]; source_summary: LearningSourceSummary };
type LearningContext = { project_name: string; project_path: string; summary: string; tech_stack: string[]; architecture_notes: string[]; important_files: string[]; common_commands: string[]; conventions: string[]; risks: string[]; agent_instructions: string[]; dtoken_recommendations: string[]; open_questions: string[]; generated_at: string; source_summary: LearningSourceSummary };
type LearningAnalysis = { ok: boolean; state: string; source: string; freshness?: string; index: LearningProjectIndex; context: LearningContext; writes_performed: false; privacy: string; error?: string };
type LearningActionFeedback = { level: "pending" | "success" | "warning" | "error"; message: string };
type LearningReview = { ok: boolean; state: string; source: string; target: string; before_sha256: string; after_sha256: string; diff: string; error?: string };
type LearningReceipt = { ok: boolean; state: string; source: string; transaction_id: string; rollback_id: string; target: string; before_sha256?: string; after_sha256?: string; verified: boolean; summary: string };
type DelegationAgent = { id: "claude" | "codex"; available: boolean; version?: string | null; execution: string };
type DelegationCapabilities = { ok: boolean; source: string; agents: DelegationAgent[]; default_mode: "prompt"; output_policy: string };
type DelegatedLearningContext = { summary: string; conventions: string[]; risks: string[]; recommendations: string[]; open_questions: string[] };
type DelegationReceipt = { ok: boolean; state: string; source: string; request_id: string; agent: string; exit_code?: number | null; elapsed_ms?: number; prompt_bytes?: number; raw_output_persisted: false; raw_output_returned: false; structured_context_returned: boolean; context?: DelegatedLearningContext | null; summary: string };

export function learningInputFromOwnedRuntime(payload: Record<string, any> | null, root: string) {
  if (!root.trim() || payload?.source !== "dtoken-owned-runtime" || payload?.desktop_data_source !== "dtoken-local-service") return null;
  const logs = Array.isArray(payload.request_logs) ? payload.request_logs : [];
  const inputs = logs.map((item: any) => Number(item.input_tokens_before ?? item.input_tokens ?? 0)).filter((value: number) => Number.isFinite(value) && value >= 0);
  const failures = logs.filter((item: any) => Boolean(item.error)).length;
  const name = root.trim().replace(/[\\/]+$/g, "").split(/[\\/]/).filter(Boolean).pop() ?? "project";
  return {
    project_id: name.toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "") || "project",
    language_stack: ["unclassified-metadata-only"],
    avg_request_tokens_7d: inputs.length > 0 ? Math.round(inputs.reduce((sum: number, value: number) => sum + value, 0) / inputs.length) : 0,
    avg_cli_noise_ratio_7d: 0,
    retrieve_rate_7d: 0,
    failure_rate_7d: logs.length > 0 ? failures / logs.length : 0,
    high_risk_task: false,
    manual_override: false,
    evidence_source: "dtoken-owned-runtime-empty-or-aggregate-request-logs",
  };
}

export function learningInputFromProjectMetadata(candidate?: LearningCandidate) {
  if (!candidate?.write_eligible || candidate.kind !== "project" || !candidate.path.trim()) return null;
  const name = candidate.path.trim().replace(/[\\/]+$/g, "").split(/[\\/]/).filter(Boolean).pop() ?? "project";
  return {
    project_id: name.toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "") || "project",
    language_stack: ["unclassified-session-metadata-only"],
    avg_request_tokens_7d: 0,
    avg_cli_noise_ratio_7d: 0,
    retrieve_rate_7d: 0,
    failure_rate_7d: 0,
    high_risk_task: false,
    manual_override: false,
    evidence_source: "local-session-project-metadata-only",
  };
}

function isMetadataOnlyBaseline(input: Record<string, any>) {
  return input.avg_request_tokens_7d === 0 && [
    "dtoken-owned-runtime-empty-or-aggregate-request-logs",
    "local-session-project-metadata-only",
  ].includes(String(input.evidence_source));
}
