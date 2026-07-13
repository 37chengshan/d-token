import { useEffect, useMemo, useState } from "react";
import { RefreshCw, Search, ShieldCheck } from "lucide-react";
import { Surface } from "../common/Surface";
import { EmptyState } from "../common/EmptyState";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Badge } from "../ui/badge";
import { Checkbox } from "../ui/checkbox";
import { OperationDialog } from "../common/OperationDialog";
import { useI18n, useT } from "../../lib/i18n";
import { invokeNative, type NativeActionResult } from "../../lib/native";
import { useRuntimeDashboard } from "../../lib/useRuntimeDashboard";
import { createOperationRequestId, useNotifications } from "../../lib/notifications";
import { projectPolicyInputFromPayload } from "./Advanced";

export function SmartLearning() {
  const t = useT();
  const { lang } = useI18n();
  const { payload, status, refresh } = useRuntimeDashboard();
  const { addNotification } = useNotifications();
  const [projectRoot, setProjectRoot] = useState("");
  const [result, setResult] = useState<Record<string, any> | null>(null);
  const [saved, setSaved] = useState<Record<string, any> | null>(null);
  const [running, setRunning] = useState(false);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState("");
  const [candidates, setCandidates] = useState<LearningCandidate[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [managedText, setManagedText] = useState("");
  const [targetName, setTargetName] = useState("AGENTS.md");
  const [review, setReview] = useState<LearningReview | null>(null);
  const [applying, setApplying] = useState(false);
  const [mode, setMode] = useState<"prompt" | "direct">("prompt");
  const [delegationAgents, setDelegationAgents] = useState<DelegationAgent[]>([]);
  const [delegationAgent, setDelegationAgent] = useState("claude");
  const [delegationReview, setDelegationReview] = useState(false);
  const [delegationRunning, setDelegationRunning] = useState(false);
  const [delegationRequestId, setDelegationRequestId] = useState("");
  const input = useMemo(() => projectPolicyInputFromPayload(payload, projectRoot) ?? learningInputFromOwnedRuntime(payload, projectRoot), [payload, projectRoot]);

  useEffect(() => {
    void invokeNative<{ state: string; payload?: Record<string, any> }>("get_app_settings", { section: "smart-learning" })
      .then((value) => setSaved(value.state === "loaded" ? value.payload ?? null : null))
      .catch(() => setSaved(null));
    void invokeNative<DelegationCapabilities>("get_learning_delegation_capabilities")
      .then((value) => setDelegationAgents(value.agents))
      .catch(() => setDelegationAgents([]));
  }, []);

  const scan = async () => {
    if (!input) {
      setNotice(lang === "zh" ? "需要真实项目目录和完整的聚合运行指标后才能分析。" : "A real project path and complete aggregate runtime metrics are required.");
      return;
    }
    const requestId = createOperationRequestId("smart-learning-scan");
    setRunning(true);
    try {
      const [next, discovery] = await Promise.all([
        invokeNative<Record<string, any>>("recommend_project_policy", { request: input }),
        invokeNative<LearningDiscovery>("discover_learning_sources", { request: { roots: [projectRoot], skip_paths: [], stop: false } }),
      ]);
      setResult(next);
      setCandidates(discovery.candidates);
      setSelected(discovery.candidates.map((candidate) => candidate.id));
      setNotice(lang === "zh" ? "分析完成；只使用聚合指标，尚未写入设置。" : "Analysis complete using aggregate metrics only; nothing has been saved yet.");
      addNotification({ dedupeKey: "smart-learning-scan", level: "success", title: lang === "zh" ? "智能学习分析完成" : "Smart Learning analysis complete", message: lang === "zh" ? "分析仅使用聚合指标，未读取原始提示词、响应、源码或凭据。" : "Analysis used aggregate metrics only; no raw prompts, responses, source code, or credentials were read.", receipt: { requestId, targetId: input.project_id } });
    } catch (error) {
      setResult(null);
      setNotice(lang === "zh" ? "分析失败，未保存任何策略。" : "Analysis failed and no policy was saved.");
      addNotification({ dedupeKey: "smart-learning-scan-failed", level: "error", title: lang === "zh" ? "智能学习分析失败" : "Smart Learning analysis failed", message: String(error), receipt: { requestId, targetId: input.project_id } });
    } finally {
      setRunning(false);
    }
  };

  const generatePrompt = () => {
    if (!result || !input || selected.length === 0) return;
    const zeroBaseline = input.avg_request_tokens_7d === 0 && input.evidence_source === "dtoken-owned-runtime-empty-or-aggregate-request-logs";
    const reasons = zeroBaseline
      ? (lang === "zh" ? "本机尚无请求，仅使用所选目录 metadata 生成基础模板；未读取或理解源码" : "No local requests yet; this baseline template uses selected-path metadata only and does not read or understand source code")
      : Array.isArray(result.recommendation?.reasons) ? result.recommendation.reasons.join("; ") : "aggregate runtime evidence";
    setManagedText(lang === "zh"
      ? `项目策略：${String(result.recommendation?.recommended_level ?? "balanced")}。\n依据：${reasons}\n执行边界：仅使用 d-token 本地路由与聚合指标，不记录原始提示词、响应或凭据。`
      : `Project policy: ${String(result.recommendation?.recommended_level ?? "balanced")}.\nEvidence: ${reasons}\nBoundary: use d-token local routing and aggregate metrics only; never record raw prompts, responses, or credentials.`);
  };

  const previewManagedBlock = async () => {
    const candidate = candidates.find((item) => selected.includes(item.id));
    if (!candidate || !managedText.trim()) return;
    const target = `${candidate.path.replace(/[\\/]+$/g, "")}/${targetName}`;
    const next = await invokeNative<LearningReview>("preview_learning_managed_block", { request: { target, content: managedText, reviewed_before_sha256: null, reviewed_after_sha256: null } });
    if (!next.ok) {
      setNotice(next.error ?? (lang === "zh" ? "无法生成受控写入预览。" : "Could not prepare the managed-write preview."));
      return;
    }
    setReview(next);
  };

  const applyManagedBlock = async () => {
    if (!review) return;
    const requestId = createOperationRequestId("learning-managed-write");
    setApplying(true);
    try {
      const receipt = await invokeNative<LearningReceipt>("apply_learning_managed_block", { request: { target: review.target, content: managedText, reviewed_before_sha256: review.before_sha256, reviewed_after_sha256: review.after_sha256 } });
      if (!receipt.ok) throw new Error(receipt.summary ?? receipt.state);
      addNotification({ dedupeKey: `learning-write-${receipt.rollback_id}`, level: "success", title: lang === "zh" ? "受控学习区块已写入" : "Managed learning block written", message: lang === "zh" ? "学习区块已原子写入、验证并创建回滚点。" : "The learning block was written atomically, verified, and given a rollback point.", persistent: true, details: `before=${receipt.before_sha256?.slice(0, 12)} · after=${receipt.after_sha256?.slice(0, 12)}`, receipt: { requestId, transactionId: receipt.transaction_id, rollbackId: receipt.rollback_id, targetId: receipt.target } });
      setNotice(lang === "zh" ? "写入已原子完成并验证；回滚点已显示在“变更与回退”。" : "The write completed atomically and was verified; its rollback point is available in Changes & Rollback.");
      setReview(null);
      await refresh();
    } catch (error) {
      addNotification({ dedupeKey: "learning-write-failed", level: "error", title: lang === "zh" ? "受控写入失败" : "Managed write failed", message: String(error), persistent: true, receipt: { requestId, targetId: review.target } });
    } finally {
      setApplying(false);
    }
  };

  const startDelegation = async () => {
    const requestId = createOperationRequestId("learning-delegation");
    setDelegationRequestId(requestId);
    setDelegationRunning(true);
    try {
      const receipt = await invokeNative<DelegationReceipt>("spawn_learning_delegation", { request: { request_id: requestId, agent: delegationAgent, prompt: managedText, timeout_ms: 120_000, confirmed: true } });
      addNotification({ dedupeKey: `learning-delegation-${requestId}`, level: receipt.ok ? "success" : "warning", title: receipt.ok ? (lang === "zh" ? "直接委派已完成" : "Direct delegation complete") : (lang === "zh" ? "直接委派未完成" : "Direct delegation did not complete"), message: receipt.ok ? (lang === "zh" ? "委派已完成，仅保留元数据回执。" : "Delegation completed with a metadata-only receipt.") : (lang === "zh" ? "委派未完成，请查看状态回执。" : "Delegation did not complete; review the status receipt."), persistent: true, details: `state=${receipt.state} · elapsed_ms=${receipt.elapsed_ms ?? "—"} · output_persisted=false`, receipt: { requestId, targetId: delegationAgent } });
      setDelegationReview(false);
    } catch (error) {
      addNotification({ dedupeKey: `learning-delegation-${requestId}-failed`, level: "error", title: lang === "zh" ? "直接委派失败" : "Direct delegation failed", message: String(error), receipt: { requestId, targetId: delegationAgent } });
    } finally {
      setDelegationRunning(false);
    }
  };

  const cancelDelegation = async () => {
    if (!delegationRequestId) return;
    await invokeNative("cancel_learning_delegation", { requestId: delegationRequestId });
  };

  const save = async () => {
    if (!result || !input) return;
    const requestId = createOperationRequestId("smart-learning-save");
    setSaving(true);
    try {
      const data = { project: { root: projectRoot, project_id: input.project_id }, recommendation: result.recommendation, mapping: result.mapping, source: result.source, privacy: result.privacy };
      const action = await invokeNative<NativeActionResult>("save_app_settings", { section: "smart-learning", payload: data });
      if (!action.ok) throw new Error(action.summary);
      setSaved(data);
      setNotice(lang === "zh" ? "策略已保存到 d-token 本地状态，重启后仍会保留。" : "The policy is saved in d-token local state and persists across restarts.");
      addNotification({ dedupeKey: `smart-learning-save-${input.project_id}`, level: "success", title: lang === "zh" ? "智能学习策略已保存" : "Smart Learning policy saved", message: lang === "zh" ? "智能学习策略已保存到 d-token 本地状态。" : "The Smart Learning policy was saved to local d-token state.", persistent: true, receipt: { requestId, targetId: input.project_id } });
      await refresh();
    } catch (error) {
      addNotification({ dedupeKey: "smart-learning-save-failed", level: "error", title: lang === "zh" ? "智能学习策略保存失败" : "Smart Learning policy save failed", message: String(error), receipt: { requestId, targetId: input.project_id } });
    } finally {
      setSaving(false);
    }
  };

  const recommendation = result?.recommendation ?? saved?.recommendation;
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-base font-semibold">{t("sl.title")}</h2>
          <p className="mt-1 text-sm text-muted-foreground">{t("sl.subtitle")}</p>
        </div>
        <Badge variant={status === "runtime" ? "default" : "outline"}>{status === "runtime" ? (lang === "zh" ? "真实聚合数据" : "Real aggregate data") : (lang === "zh" ? "运行数据不可用" : "Runtime data unavailable")}</Badge>
      </div>

      <div className="flex items-center gap-2 rounded-lg border border-border/70 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        <ShieldCheck className="size-3.5 shrink-0 text-emerald-600" />
        <span>{lang === "zh" ? "只使用 d-token 聚合运行指标；不读取原始提示词、响应、源代码或凭据。" : "Uses d-token aggregate runtime metrics only; raw prompts, responses, source code, and credentials are not read."}</span>
      </div>

      <Surface className="p-4" hover={false}>
        <div className="flex flex-wrap gap-2">
          <Input value={projectRoot} onChange={(event) => setProjectRoot(event.target.value)} placeholder={lang === "zh" ? "输入真实项目目录" : "Enter a real project directory"} className="min-w-64 flex-1" />
          <Button size="sm" onClick={() => void scan()} disabled={running || !input}>
            {running ? <RefreshCw className="size-3.5 animate-spin" /> : <Search className="size-3.5" />}
            {running ? (lang === "zh" ? "分析中" : "Analyzing") : (lang === "zh" ? "分析项目" : "Analyze project")}
          </Button>
        </div>
        {notice && <p className="mt-3 text-xs text-muted-foreground">{notice}</p>}
      </Surface>

      <div className="grid gap-4 lg:grid-cols-2">
        <Surface className="p-4" hover={false}>
          <h3 className="mb-2 text-sm font-medium">{t("sl.candidates")}</h3>
          {candidates.length > 0 ? <div className="space-y-2"><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={lang === "zh" ? "搜索路径或 Agent" : "Search path or agent"} />{candidates.filter((candidate) => `${candidate.path} ${candidate.agent}`.toLowerCase().includes(query.toLowerCase())).map((candidate) => <label key={candidate.id} className="flex items-start gap-2 border-t border-border/60 py-2 text-sm"><Checkbox checked={selected.includes(candidate.id)} onCheckedChange={() => setSelected((items) => items.includes(candidate.id) ? items.filter((id) => id !== candidate.id) : [...items, candidate.id])} /><span className="min-w-0"><span className="block truncate font-mono">{candidate.path}</span><span className="text-xs text-muted-foreground">{candidate.agent} · {candidate.activity} · {candidate.confidence} · {candidate.freshness}</span></span></label>)}</div> : <EmptyState status="empty" title={lang === "zh" ? "等待真实项目与运行指标" : "Waiting for a real project and runtime metrics"} />}
        </Surface>
        <Surface className="p-4" hover={false}>
          <h3 className="mb-2 text-sm font-medium">{t("sl.output")}</h3>
          {recommendation ? (
            <div className="space-y-3 text-sm">
              <div className="flex items-center justify-between"><span>{input?.avg_request_tokens_7d === 0 ? (lang === "zh" ? "基础策略" : "Baseline policy") : (lang === "zh" ? "推荐等级" : "Recommended level")}</span><Badge>{String(recommendation.recommended_level ?? "unknown")}</Badge></div>
              <div className="text-xs text-muted-foreground">{input?.avg_request_tokens_7d === 0 && input.evidence_source === "dtoken-owned-runtime-empty-or-aggregate-request-logs" ? (lang === "zh" ? "本机尚无请求；仅基于所选路径 metadata 生成基础模板，未读取或理解源码。" : "No local requests yet; this baseline uses selected-path metadata only and does not read or understand source code.") : Array.isArray(recommendation.reasons) ? recommendation.reasons.join(" · ") : (lang === "zh" ? "未返回推荐依据" : "No recommendation reasons returned")}</div>
              {result && <Button size="sm" onClick={() => void save()} disabled={saving}>{saving ? <RefreshCw className="size-3.5 animate-spin" /> : null}{lang === "zh" ? "保存本地策略" : "Save local policy"}</Button>}
              {result && <Button size="sm" variant="outline" onClick={generatePrompt} disabled={selected.length === 0}>{lang === "zh" ? "生成结构化提示词" : "Generate structured prompt"}</Button>}
            </div>
          ) : <EmptyState status="empty" title={lang === "zh" ? "尚无真实分析结果" : "No real analysis result yet"} />}
        </Surface>
      </div>
      {managedText && (
        <Surface className="space-y-3 p-4" hover={false}>
          <div className="flex flex-wrap items-center justify-between gap-2"><div className="flex items-center gap-1"><Button size="sm" variant={mode === "prompt" ? "default" : "outline"} onClick={() => setMode("prompt")}>{lang === "zh" ? "提示词模式（默认）" : "Prompt mode (default)"}</Button><Button size="sm" variant={mode === "direct" ? "default" : "outline"} onClick={() => setMode("direct")}>{lang === "zh" ? "直接委派" : "Direct delegation"}</Button></div>{mode === "prompt" ? <select className="rounded-md border border-border bg-background px-2 py-1 text-sm" value={targetName} onChange={(event) => setTargetName(event.target.value)}>{["AGENTS.md", "CLAUDE.md", "GEMINI.md", "d-token.md"].map((name) => <option key={name}>{name}</option>)}</select> : <select className="rounded-md border border-border bg-background px-2 py-1 text-sm" value={delegationAgent} onChange={(event) => setDelegationAgent(event.target.value)}>{delegationAgents.map((agent) => <option key={agent.id} value={agent.id} disabled={!agent.available}>{agent.id} · {agent.available ? (lang === "zh" ? "可用" : "available") : (lang === "zh" ? "不可用" : "unavailable")}</option>)}</select>}</div>
          <textarea className="min-h-36 w-full rounded-md border border-border bg-background p-3 text-sm" value={managedText} onChange={(event) => setManagedText(event.target.value)} />
          <div className="flex justify-end"><Button onClick={() => mode === "prompt" ? void previewManagedBlock() : setDelegationReview(true)} disabled={mode === "prompt" ? selected.length !== 1 : !delegationAgents.some((agent) => agent.id === delegationAgent && agent.available)}>{mode === "prompt" ? (lang === "zh" ? "预览受控写入" : "Preview managed write") : (lang === "zh" ? "审阅直接委派" : "Review direct delegation")}</Button></div>
        </Surface>
      )}
      <OperationDialog open={review !== null} onOpenChange={(open) => { if (!open && !applying) setReview(null); }} title={lang === "zh" ? "确认写入 d-token 管理区块" : "Confirm d-token managed block write"} summary={lang === "zh" ? "只新增或更新 d-token 标记区块；区块外内容保持不变。" : "Only the d-token marked block is added or updated; content outside it remains unchanged."} impact={review?.target} confirmLabel={lang === "zh" ? "确认原子写入" : "Confirm atomic write"} cancelLabel={t("common.cancel")} risk="warning" pending={applying} confirmDisabled={!review} onConfirm={applyManagedBlock} details={review ? <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-md bg-muted/50 p-3 text-xs">{review.diff}</pre> : null} />
      <OperationDialog open={delegationReview} onOpenChange={setDelegationReview} title={lang === "zh" ? `确认委派给 ${delegationAgent}？` : `Delegate to ${delegationAgent}?`} summary={lang === "zh" ? "提示词仅通过 stdin 发送；不使用 shell 拼接，不保存或返回模型输出正文。" : "The prompt is sent through stdin without shell composition; model output bodies are neither persisted nor returned."} impact={`${managedText.length} bytes · timeout 120s`} confirmLabel={lang === "zh" ? "确认直接委派" : "Confirm delegation"} cancelLabel={t("common.cancel")} risk="warning" pending={delegationRunning} onConfirm={startDelegation} details={delegationRunning ? <Button variant="outline" onClick={() => void cancelDelegation()}>{lang === "zh" ? "取消正在运行的委派" : "Cancel running delegation"}</Button> : null} />
    </div>
  );
}

type LearningCandidate = { id: string; path: string; agent: string; mtime?: number; activity: string; confidence: string; value: string; reasons: string[]; freshness: string };
type LearningDiscovery = { ok: boolean; state: string; source: string; candidates: LearningCandidate[]; writes_performed: false; privacy: string };
type LearningReview = { ok: boolean; state: string; source: string; target: string; before_sha256: string; after_sha256: string; diff: string; error?: string };
type LearningReceipt = { ok: boolean; state: string; source: string; transaction_id: string; rollback_id: string; target: string; before_sha256?: string; after_sha256?: string; verified: boolean; summary: string };
type DelegationAgent = { id: "claude" | "codex"; available: boolean; version?: string | null; execution: string };
type DelegationCapabilities = { ok: boolean; source: string; agents: DelegationAgent[]; default_mode: "prompt"; output_policy: string };
type DelegationReceipt = { ok: boolean; state: string; source: string; request_id: string; agent: string; exit_code?: number | null; elapsed_ms?: number; prompt_bytes?: number; output_persisted: false; output_returned: false; summary: string };

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
