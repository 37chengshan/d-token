import { useState, useEffect } from "react";
import {
  ShieldAlert,
  Sliders,
  CheckCircle2,
  AlertTriangle,
  RotateCcw,
  Sparkles,
  Lock,
  ChevronDown,
  Info,
} from "lucide-react";
import {
  type WorkflowPolicyItem,
  type ActivationReadiness,
  type PolicyChangePreview,
  fetchWorkflowPolicies,
  previewWorkflowPolicyChanges,
  applyWorkflowPolicyChanges,
  rollbackWorkflowPolicy,
  validateWorkflowPolicy,
} from "../../lib/contextEngine";
import { OperationDialog } from "../common/OperationDialog";

export function WorkflowPolicyEditor() {
  const [revision, setRevision] = useState("rev-init");
  const [workflows, setWorkflows] = useState<WorkflowPolicyItem[]>([]);
  const [readiness, setReadiness] = useState<ActivationReadiness | null>(null);
  const [loading, setLoading] = useState(true);
  const [draft, setDraft] = useState<WorkflowPolicyItem[]>([]);
  const [validationErrors, setValidationErrors] = useState<Record<string, string[]>>({});
  
  // Preview & Transaction
  const [preview, setPreview] = useState<PolicyChangePreview | null>(null);
  const [applying, setApplying] = useState(false);
  const [applyError, setApplyError] = useState<string | null>(null);
  const [lastRollbackId, setLastRollbackId] = useState<string | null>(null);
  const [rollbackNotice, setRollbackNotice] = useState<string | null>(null);

  const loadData = async () => {
    setLoading(true);
    try {
      const data = await fetchWorkflowPolicies();
      setRevision(data.revision);
      // Filter out any reserved workflows - only executable ones are editable
      const editable = data.workflows.filter((w) => !w.isReserved);
      setWorkflows(editable);
      setDraft(JSON.parse(JSON.stringify(editable)));
      setReadiness(data.readiness);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData();
  }, []);

  const handleFieldChange = (
    id: string,
    field: keyof WorkflowPolicyItem,
    value: any
  ) => {
    setDraft((prev) =>
      prev.map((item) => {
        if (item.id === id) {
          const updated = { ...item, [field]: value };
          const validation = validateWorkflowPolicy(updated);
          setValidationErrors((errs) => ({
            ...errs,
            [id]: validation.ok ? [] : validation.errors,
          }));
          return updated;
        }
        return item;
      })
    );
  };

  const hasErrors = Object.values(validationErrors).some((errs) => errs.length > 0);

  const handlePreview = async () => {
    setApplyError(null);
    const res = await previewWorkflowPolicyChanges(draft, revision);
    if (!res.ok) {
      setApplyError(res.error || "配置校验失败");
      return;
    }
    setPreview(res);
  };

  const handleApply = async () => {
    if (!preview) return;
    setApplying(true);
    setApplyError(null);
    try {
      const res = await applyWorkflowPolicyChanges(draft, revision);
      if (!res.ok) {
        setApplyError(res.error || res.summary || "应用失败");
        return;
      }
      setLastRollbackId(res.rollbackId || null);
      setRollbackNotice(null);
      setPreview(null);
      await loadData();
    } catch (err: any) {
      setApplyError(err?.message || "应用异常");
    } finally {
      setApplying(false);
    }
  };

  const handleRollback = async () => {
    if (!lastRollbackId) return;
    const res = await rollbackWorkflowPolicy(lastRollbackId);
    if (res.ok) {
      setRollbackNotice(res.summary);
      setLastRollbackId(null);
      await loadData();
    } else {
      setApplyError(res.error || "回退失败");
    }
  };

  if (loading) {
    return (
      <div className="p-8 text-center text-xs text-[#716a61]">
        正在加载上下文工作流策略...
      </div>
    );
  }

  return (
    <div className="space-y-6" data-testid="workflow-policy-editor">
      {/* 1. Shadow-to-Active 准入门槛状态栏 */}
      <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4 text-xs space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Sparkles className="size-4 text-[#c7652a]" />
            <span className="font-semibold text-[#29251f]">Shadow → Active 实时接入准入准则</span>
          </div>
          {readiness?.canActivate ? (
            <span className="inline-flex items-center gap-1 rounded-full bg-emerald-50 border border-emerald-200 px-2.5 py-0.5 font-medium text-emerald-800">
              <CheckCircle2 className="size-3.5" /> 满足准入，允许开启 Active
            </span>
          ) : readiness?.evidenceState === "unknown" ? (
            <span className="inline-flex items-center gap-1 rounded-full bg-[#f0ede5] border border-[rgba(73,61,47,0.12)] px-2.5 py-0.5 font-medium text-[#716a61]">
              <Lock className="size-3.5" /> 运行时准入证据未知，Active 保持锁定
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 rounded-full bg-amber-50 border border-amber-200 px-2.5 py-0.5 font-medium text-amber-800">
              <Lock className="size-3.5" /> 尚未达到准入门槛（受保卫拦截）
            </span>
          )}
        </div>

        {readiness && (
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 pt-1">
            <div className="rounded-lg bg-[#f0ede5] p-2.5 border border-[rgba(73,61,47,0.08)]">
              <div className="text-[11px] text-[#716a61]">Shadow 回放样本</div>
              <div className="text-sm font-semibold text-[#29251f] mt-0.5">
                {readiness.shadowReplayCount === null ? "未知" : readiness.shadowReplayCount} / {readiness.shadowReplayRequired === null ? "未知" : readiness.shadowReplayRequired} 轮
              </div>
            </div>
            <div className="rounded-lg bg-[#f0ede5] p-2.5 border border-[rgba(73,61,47,0.08)]">
              <div className="text-[11px] text-[#716a61]">模型一致性率</div>
              <div className="text-sm font-semibold text-[#29251f] mt-0.5">
                {readiness.agreementRate === null ? "未知" : `${(readiness.agreementRate * 100).toFixed(1)}%`}
                {readiness.agreementRateRequired === null ? "（要求未知）" : `（要求 ≥ ${(readiness.agreementRateRequired * 100).toFixed(0)}%）`}
              </div>
            </div>
            <div className="rounded-lg bg-[#f0ede5] p-2.5 border border-[rgba(73,61,47,0.08)]">
              <div className="text-[11px] text-[#716a61]">P95 决策耗时</div>
              <div className="text-sm font-semibold text-[#29251f] mt-0.5">
                {readiness.latencyP95Ms === null ? "未知" : `${readiness.latencyP95Ms}ms`}
                {readiness.latencyP95MaxMs === null ? "（上限未知）" : `（上限 ≤ ${readiness.latencyP95MaxMs}ms）`}
              </div>
            </div>
          </div>
        )}

        {readiness && !readiness.canActivate && readiness.unmetGates.length > 0 && (
          <div className="rounded-lg bg-amber-50/70 border border-amber-200/60 p-2.5 text-[11px] text-amber-900 space-y-1">
            <div className="font-medium flex items-center gap-1">
              <Info className="size-3.5" /> 当前未达成门槛：
            </div>
            <ul className="list-disc list-inside space-y-0.5 pl-1">
              {readiness.unmetGates.map((gate, i) => (
                <li key={i}>{gate}</li>
              ))}
            </ul>
          </div>
        )}
      </div>

      {/* 回退提示条 */}
      {lastRollbackId && (
        <div className="flex items-center justify-between gap-2 rounded-lg bg-emerald-50 border border-emerald-200 p-3 text-xs text-emerald-900">
          <span>策略已成功应用。如遇异常可一键回退 (快照 ID: {lastRollbackId})</span>
          <button
            type="button"
            onClick={handleRollback}
            className="inline-flex items-center gap-1 rounded bg-emerald-700 text-white px-2.5 py-1 hover:bg-emerald-800 transition-colors shrink-0"
          >
            <RotateCcw className="size-3" /> 一键回退
          </button>
        </div>
      )}

      {rollbackNotice && (
        <div className="rounded-lg bg-blue-50 border border-blue-200 p-3 text-xs text-blue-900">
          {rollbackNotice}
        </div>
      )}

      {applyError && (
        <div className="rounded-lg bg-rose-50 border border-rose-200 p-3 text-xs text-rose-800 flex items-center gap-2">
          <AlertTriangle className="size-4 shrink-0" />
          <span>{applyError}</span>
        </div>
      )}

      {/* 2. 5 个执行工作流独立卡片列表 */}
      <div className="space-y-4">
        {draft.map((workflow) => {
          const errors = validationErrors[workflow.id] || [];
          return (
            <div
              key={workflow.id}
              className={`rounded-xl border p-4 transition-all ${
                workflow.killSwitch
                  ? "border-rose-300 bg-rose-50/20"
                  : "border-[rgba(73,61,47,0.13)] bg-[#fbf8f2]"
              }`}
            >
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-[rgba(73,61,47,0.08)]">
                <div>
                  <div className="flex items-center gap-2">
                    <h3 className="text-sm font-semibold text-[#29251f]">
                      {workflow.title}
                    </h3>
                    <span className="font-mono text-[10px] text-[#716a61] bg-[#f0ede5] px-1.5 py-0.5 rounded">
                      {workflow.id}
                    </span>
                    {workflow.killSwitch && (
                      <span className="inline-flex items-center gap-1 text-[10px] font-semibold text-rose-700 bg-rose-100 px-2 py-0.5 rounded-full">
                        <ShieldAlert className="size-3" /> 熔断已停用
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-[#716a61] mt-0.5">
                    {workflow.description}
                  </p>
                </div>

                {/* 独立工作流模式选择 */}
                <div className="flex items-center gap-2 shrink-0">
                  <label htmlFor={`workflow-${workflow.id}-mode`} className="text-xs text-[#716a61]">接入模式:</label>
                  <select
                    id={`workflow-${workflow.id}-mode`}
                    value={workflow.mode}
                    onChange={(e) =>
                      handleFieldChange(
                        workflow.id,
                        "mode",
                        e.target.value as any
                      )
                    }
                    className="text-xs rounded-md border border-[rgba(73,61,47,0.2)] bg-white px-2.5 py-1 text-[#29251f] focus:border-[#c7652a] focus:outline-none"
                  >
                    <option value="shadow">Shadow (预览比对)</option>
                    <option
                      value="active"
                      disabled={!readiness?.canActivate}
                    >
                      Active (实时生效 {!readiness?.canActivate ? "· 未达准入" : ""})
                    </option>
                    <option value="disabled">Disabled (停用)</option>
                  </select>
                </div>
              </div>

              {/* 工作流参数与高级策略微调 */}
              <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3 pt-3 text-xs">
                <div>
                  <label htmlFor={`workflow-${workflow.id}-threshold`} className="block text-[11px] text-[#716a61] mb-1">
                    触发阈值 (tokens)
                  </label>
                  <input
                    id={`workflow-${workflow.id}-threshold`}
                    type="number"
                    value={workflow.thresholdTokens}
                    onChange={(e) =>
                      handleFieldChange(
                        workflow.id,
                        "thresholdTokens",
                        parseInt(e.target.value, 10) || 0
                      )
                    }
                    className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-2.5 py-1 text-xs text-[#29251f] focus:border-[#c7652a] focus:outline-none"
                  />
                </div>

                <div>
                  <label htmlFor={`workflow-${workflow.id}-timeout`} className="block text-[11px] text-[#716a61] mb-1">
                    判定超时上限 (ms)
                  </label>
                  <input
                    id={`workflow-${workflow.id}-timeout`}
                    type="number"
                    value={workflow.timeoutMs}
                    onChange={(e) =>
                      handleFieldChange(
                        workflow.id,
                        "timeoutMs",
                        parseInt(e.target.value, 10) || 0
                      )
                    }
                    className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-2.5 py-1 text-xs text-[#29251f] focus:border-[#c7652a] focus:outline-none"
                  />
                </div>

                <div>
                  <label htmlFor={`workflow-${workflow.id}-sticky-ttl`} className="block text-[11px] text-[#716a61] mb-1">
                    Sticky 缓存生命周期 (s)
                  </label>
                  <input
                    id={`workflow-${workflow.id}-sticky-ttl`}
                    type="number"
                    value={workflow.stickyTtlSeconds}
                    onChange={(e) =>
                      handleFieldChange(
                        workflow.id,
                        "stickyTtlSeconds",
                        parseInt(e.target.value, 10) || 0
                      )
                    }
                    className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-2.5 py-1 text-xs text-[#29251f] focus:border-[#c7652a] focus:outline-none"
                  />
                </div>

                <div>
                  <label htmlFor={`workflow-${workflow.id}-fallback`} className="block text-[11px] text-[#716a61] mb-1">
                    降级处理行为
                  </label>
                  <select
                    id={`workflow-${workflow.id}-fallback`}
                    value={workflow.fallback}
                    onChange={(e) =>
                      handleFieldChange(
                        workflow.id,
                        "fallback",
                        e.target.value as any
                      )
                    }
                    className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-2 py-1 text-xs text-[#29251f] focus:border-[#c7652a] focus:outline-none"
                  >
                    <option value="local-heuristic">回退本地启发式</option>
                    <option value="passthrough">直接透传原文</option>
                  </select>
                </div>
              </div>

              {/* 独立 Kill switch */}
              <div className="flex items-center justify-between pt-3 mt-3 border-t border-[rgba(73,61,47,0.06)] text-xs">
                <label className="flex items-center gap-2 text-[#716a61] cursor-pointer">
                  <input
                    type="checkbox"
                    checked={workflow.killSwitch}
                    onChange={(e) =>
                      handleFieldChange(workflow.id, "killSwitch", e.target.checked)
                    }
                    className="rounded border-[rgba(73,61,47,0.3)] text-[#c7652a] focus:ring-[#c7652a]"
                  />
                  <span>紧急熔断 (Kill Switch) — 强制跳过该工作流</span>
                </label>
                {workflow.killSwitch && (
                  <span className="text-[11px] text-rose-600 font-medium">
                    已启用单项熔断，所有相关请求将安全透传
                  </span>
                )}
              </div>

              {/* 校验错误提示 */}
              {errors.length > 0 && (
                <div className="mt-2 text-[11px] text-rose-600 space-y-0.5">
                  {errors.map((err, idx) => (
                    <div key={idx}>• {err}</div>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* 3. 专家模式原始细节折叠面板 */}
      <details className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4 text-xs group">
        <summary className="font-semibold text-[#29251f] cursor-pointer flex items-center justify-between select-none">
          <span className="flex items-center gap-1.5">
            <Sliders className="size-3.5 text-[#716a61]" />
            <span>专家模式与原始配置快照 (Expert Details)</span>
          </span>
          <ChevronDown className="size-4 text-[#716a61] transition-transform group-open:rotate-180" />
        </summary>
        <div className="mt-3 pt-3 border-t border-[rgba(73,61,47,0.08)] space-y-2">
          <div className="flex items-center justify-between text-[#716a61]">
            <span>当前策略版本 Revision: <strong className="text-[#29251f] font-mono">{revision}</strong></span>
            <span>可编辑工作流数量: {draft.length} 项</span>
          </div>
          <pre className="max-h-60 overflow-auto rounded-lg bg-[#29251f] text-[#fbf8f2] p-3 font-mono text-[11px] leading-relaxed">
            {JSON.stringify(draft, null, 2)}
          </pre>
        </div>
      </details>

      {/* 4. 底部动作条 */}
      <div className="flex items-center justify-between pt-2">
        <button
          type="button"
          onClick={loadData}
          className="rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-3 py-1.5 text-xs text-[#716a61] hover:text-[#29251f] hover:bg-[#eee8df] transition-colors"
        >
          放弃修改
        </button>

        <button
          type="button"
          onClick={handlePreview}
          disabled={hasErrors}
          className="rounded-md bg-[#c7652a] px-4 py-1.5 text-xs font-semibold text-white shadow-sm hover:bg-[#a94f1c] disabled:opacity-50 transition-colors"
        >
          预览策略变更
        </button>
      </div>

      {/* 预览与确认对话框 */}
      {preview && (
        <OperationDialog
          open={preview !== null}
          onOpenChange={(open) => {
            if (!open && !applying) setPreview(null);
          }}
          title="确认应用工作流策略变更"
          summary={preview.summary}
          impact="变更应用后将以原子事务写入配置，并立即刷新运行时优化策略；系统会自动保存配置快照以备回退。"
          risk={preview.requiresConfirmation ? "warning" : "normal"}
          confirmLabel={applying ? "正在应用..." : "确认并写入"}
          cancelLabel="取消"
          pending={applying}
          error={applyError || undefined}
          onConfirm={handleApply}
          details={
            <div className="mt-2 space-y-2">
              <div className="text-xs text-[#716a61]">
                变更 Diff 概览 (版本: <span className="font-mono">{preview.revision}</span>):
              </div>
              <pre className="max-h-48 overflow-auto rounded-md bg-[#f0ede5] p-2.5 font-mono text-[11px] text-[#29251f] whitespace-pre-wrap">
                {preview.diff}
              </pre>
            </div>
          }
        />
      )}
    </div>
  );
}
