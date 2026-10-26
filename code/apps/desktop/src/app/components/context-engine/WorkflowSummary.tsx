import { Layers, ShieldCheck, Activity, Clock, Sliders } from "lucide-react";
import type { JevWorkflowStatus } from "../../lib/contextEngine";
import type { EvidenceState } from "../../lib/contextEngine";

interface WorkflowSummaryProps {
  workflows: JevWorkflowStatus[];
  isJevActive: boolean;
  evidenceState?: EvidenceState | "healthy" | "unavailable";
}

export function WorkflowSummary({ workflows, isJevActive, evidenceState = "unknown" }: WorkflowSummaryProps) {
  const hasRuntimeEvidence = evidenceState === "runtime" || evidenceState === "event-ledger" || evidenceState === "healthy";
  return (
    <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-5">
      <div className="flex items-center justify-between pb-3 border-b border-[rgba(73,61,47,0.1)]">
        <div className="flex items-center gap-2">
          <Layers className="size-4 text-[#c7652a]" />
          <h3 className="text-sm font-semibold text-[#29251f]">实时上下文决策工作流</h3>
        </div>
        <span className="text-xs text-[#716a61]">
          {hasRuntimeEvidence
            ? isJevActive ? "Jev 决策增强已由运行时确认" : "本地基础模式已由运行时确认"
            : "配置预览：等待运行时证据"}
        </span>
      </div>

      <div className="mt-4 grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
        {workflows.map((wf) => {
          const isOperational = wf.enabled && (wf.status === "operating" || wf.status === "shadow");
          return (
            <div
              key={wf.id}
              className="flex flex-col justify-between p-3.5 rounded-lg border border-[rgba(73,61,47,0.1)] bg-[#fbf8f2] hover:border-[rgba(73,61,47,0.2)] transition-all"
            >
              <div>
                <div className="flex items-center justify-between pb-1.5">
                  <span className="font-medium text-xs text-[#29251f]">{wf.title}</span>
                  <span
                    className={`inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-semibold ${
                      hasRuntimeEvidence && wf.status === "operating"
                        ? "bg-emerald-50 text-emerald-700 border border-emerald-200"
                        : hasRuntimeEvidence && wf.status === "shadow"
                        ? "bg-amber-50 text-amber-700 border border-amber-200"
                        : "bg-gray-100 text-gray-600"
                    }`}
                  >
                    {!hasRuntimeEvidence
                      ? "配置预览"
                      : wf.status === "operating"
                      ? "实时生效"
                      : wf.status === "shadow"
                      ? "Shadow 评估"
                      : "已停用"}
                  </span>
                </div>
                <p className="text-[11px] text-[#716a61] line-clamp-2 leading-relaxed">
                  {wf.description}
                </p>
              </div>

              <div className="mt-3 pt-2 border-t border-[rgba(73,61,47,0.06)] flex items-center justify-between text-[10px] text-[#716a61]">
                <span className="flex items-center gap-1">
                  <ShieldCheck className={`size-3 ${hasRuntimeEvidence ? "text-emerald-600" : "text-[#9b9287]"}`} />
                  <span>回退: {wf.fallback === "local-heuristic" ? "本地规则" : "原文透传"}</span>
                </span>
                <span className="font-mono">{wf.id}</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
