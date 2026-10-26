import { useState } from "react";
import {
  CheckCircle2,
  AlertCircle,
  Clock,
  ShieldCheck,
  Search,
  Filter,
  ArrowRight,
  ExternalLink,
  ChevronRight,
  Sparkles,
  Cpu,
  Archive,
} from "lucide-react";
import type { EvidenceState } from "../../lib/contextEngine";

export interface DecisionTimelineItem {
  requestId: string;
  timestamp: string;
  workflow: string;
  agent: string;
  model: string;
  decisionEngine: "local" | "jev";
  tokensBefore: number;
  tokensAfter: number;
  reductionPercentage: number;
  reason: string;
  confidenceBand: "high" | "medium" | "fallback" | "n/a";
  validation: "accepted" | "rejected" | "bypassed";
  recoveryPresent: boolean;
  sendStatus: "ok" | "degraded" | "failed";
  errorMessage?: string;
}

interface DecisionTimelineProps {
  items?: DecisionTimelineItem[];
  onNavigateToDiagnostics?: (correlationId?: string) => void;
  evidenceState?: EvidenceState;
}

export function DecisionTimeline({
  items = [],
  onNavigateToDiagnostics,
  evidenceState = "unknown",
}: DecisionTimelineProps) {
  const [filter, setFilter] = useState<"all" | "ok" | "degraded" | "failed">("all");
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const filtered = items.filter((item) => {
    if (filter !== "all" && item.sendStatus !== filter) return false;
    if (search.trim()) {
      const q = search.toLowerCase();
      return (
        item.requestId.toLowerCase().includes(q) ||
        item.agent.toLowerCase().includes(q) ||
        item.workflow.toLowerCase().includes(q) ||
        item.reason.toLowerCase().includes(q)
      );
    }
    return true;
  });
  const hasRuntimeEvidence = evidenceState === "runtime" || evidenceState === "event-ledger";
  const evidenceLabel = hasRuntimeEvidence
    ? "运行时脱敏记录"
    : evidenceState === "partial"
      ? "部分运行时记录"
      : evidenceState === "preview"
        ? "预览数据"
        : "数据源未知";

  return (
    <div className="space-y-4" data-testid="decision-timeline">
      {/* 搜索与筛选栏 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div className="relative flex-1 max-w-sm">
          <Search className="absolute left-2.5 top-2.5 size-3.5 text-[#716a61]" />
          <label htmlFor="decision-timeline-search" className="sr-only">搜索请求记录</label>
          <input
            id="decision-timeline-search"
            type="text"
            placeholder="搜索 Request ID、Agent 或工作流..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white pl-8 pr-3 py-1.5 text-xs text-[#29251f] placeholder:text-[#716a61]/60 focus:border-[#c7652a] focus:outline-none"
          />
        </div>

        <div className="flex items-center gap-1 bg-[#f0ede5] p-1 rounded-lg border border-[rgba(73,61,47,0.1)] text-xs">
          <button
            type="button"
            aria-pressed={filter === "all"}
            onClick={() => setFilter("all")}
            className={`px-2.5 py-1 rounded-md font-medium transition-all ${
              filter === "all"
                ? "bg-[#fbf8f2] text-[#29251f] shadow-xs"
                : "text-[#716a61] hover:text-[#29251f]"
            }`}
          >
            全部 ({items.length})
          </button>
          <button
            type="button"
            aria-pressed={filter === "ok"}
            onClick={() => setFilter("ok")}
            className={`px-2.5 py-1 rounded-md font-medium transition-all ${
              filter === "ok"
                ? "bg-[#fbf8f2] text-emerald-800 shadow-xs"
                : "text-[#716a61] hover:text-[#29251f]"
            }`}
          >
            已优化 ({items.filter((i) => i.sendStatus === "ok").length})
          </button>
          <button
            type="button"
            aria-pressed={filter === "degraded"}
            onClick={() => setFilter("degraded")}
            className={`px-2.5 py-1 rounded-md font-medium transition-all ${
              filter === "degraded"
                ? "bg-[#fbf8f2] text-amber-800 shadow-xs"
                : "text-[#716a61] hover:text-[#29251f]"
            }`}
          >
            降级/跳过 ({items.filter((i) => i.sendStatus === "degraded").length})
          </button>
          <button
            type="button"
            aria-pressed={filter === "failed"}
            onClick={() => setFilter("failed")}
            className={`px-2.5 py-1 rounded-md font-medium transition-all ${
              filter === "failed"
                ? "bg-[#fbf8f2] text-rose-800 shadow-xs"
                : "text-[#716a61] hover:text-[#29251f]"
            }`}
          >
            失败 ({items.filter((i) => i.sendStatus === "failed").length})
          </button>
        </div>
      </div>

      {/* 提示信息：无原始 Prompt/Response 泄露 */}
      <div className="rounded-lg bg-[#f0ede5] p-2.5 border border-[rgba(73,61,47,0.08)] flex items-center justify-between text-[11px] text-[#716a61]">
        <div className="flex items-center gap-1.5">
          <ShieldCheck className="size-3.5 text-emerald-600" />
          <span>脱敏保护已生效：此时间线仅展示结构化优化指标与决策理由，严禁记录或展示代码与 Prompt 明文。</span>
        </div>
        <span>{evidenceLabel}</span>
      </div>

      {/* 时间线列表 */}
      <div className="space-y-3">
        {filtered.length === 0 ? (
          <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-8 text-center text-xs text-[#716a61]">
            未匹配到符合条件的请求记录
          </div>
        ) : (
          filtered.map((item) => (
            <div
              key={item.requestId}
              className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4 text-xs transition-all hover:border-[rgba(73,61,47,0.25)] space-y-3"
            >
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-2 border-b border-[rgba(73,61,47,0.08)]">
                <div className="flex items-center gap-2">
                  {item.sendStatus === "ok" ? (
                    <CheckCircle2 className="size-4 text-emerald-600 shrink-0" />
                  ) : item.sendStatus === "degraded" ? (
                    <AlertCircle className="size-4 text-amber-600 shrink-0" />
                  ) : (
                    <AlertCircle className="size-4 text-rose-600 shrink-0" />
                  )}
                  <span className="font-mono font-semibold text-[#29251f]">
                    {item.requestId}
                  </span>
                  <span className="text-[10px] bg-[#f0ede5] px-1.5 py-0.5 rounded text-[#716a61]">
                    {item.agent}
                  </span>
                  <span className="text-[10px] text-[#716a61]">
                    → {item.model}
                  </span>
                </div>

                <div className="flex items-center gap-2">
                  <span className="text-[11px] text-[#716a61] flex items-center gap-1">
                    <Clock className="size-3" />
                    {item.timestamp}
                  </span>
                  {item.decisionEngine === "jev" ? (
                    <span className="inline-flex items-center gap-1 rounded bg-amber-500/15 px-2 py-0.5 text-[10px] font-semibold text-amber-900">
                      <Sparkles className="size-2.5" /> Jev
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1 rounded bg-[#c7652a]/15 px-2 py-0.5 text-[10px] font-semibold text-[#a94f1c]">
                      <Cpu className="size-2.5" /> Local
                    </span>
                  )}
                </div>
              </div>

              {/* 决策理由与结构化信息 */}
              <div className="text-xs text-[#29251f] leading-relaxed">
                <strong className="text-[#716a61] font-normal">优化判定: </strong>
                {item.reason}
              </div>

              {/* 决策指标与状态标签 */}
              <div className="flex flex-wrap items-center justify-between gap-3 pt-1 text-[11px]">
                <div className="flex items-center gap-3 text-[#716a61]">
                  <span>
                    Tokens: <strong className="text-[#29251f]">{item.tokensBefore.toLocaleString()}</strong> → <strong className="text-[#29251f]">{item.tokensAfter.toLocaleString()}</strong>
                  </span>
                  {item.reductionPercentage > 0 && (
                    <span className="text-emerald-700 font-semibold bg-emerald-50 px-1.5 py-0.2 rounded">
                      -{item.reductionPercentage}%
                    </span>
                  )}
                  {item.recoveryPresent && (
                    <span className="inline-flex items-center gap-1 text-emerald-700">
                      <Archive className="size-3" /> 可无损恢复
                    </span>
                  )}
                </div>

                {/* 失败时的诊断跳转 */}
                {item.sendStatus === "failed" && onNavigateToDiagnostics && (
                  <button
                    type="button"
                    onClick={() => onNavigateToDiagnostics(item.requestId)}
                    className="inline-flex items-center gap-1 font-semibold text-rose-700 hover:text-rose-900 hover:underline"
                  >
                    <span>前往诊断此请求</span>
                    <ArrowRight className="size-3" />
                  </button>
                )}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
