import {
  TrendingDown,
  Cpu,
  Sparkles,
  Zap,
  Clock,
  Database,
  ShieldAlert,
  ShieldCheck,
  ArrowDownRight,
} from "lucide-react";
import type { EvidenceState } from "../../lib/contextEngine";

export interface MetricsSummaryData {
  totalRequests: number;
  originalTokens: number;
  optimizedTokens: number;
  tokensSaved: number;
  reductionPercentage: number;
  estimatedSavingsUsd: number;
  providerConfirmedSavingsUsd: number;
  localVsJevSplit: {
    localPercent: number;
    jevPercent: number;
  };
  fallbackRate: number;
  p95LatencyLocalMs: number;
  p95LatencyJevMs: number;
  stickyHitRate: number;
  recoveryFailures: number;
  freshnessMs: number;
  source: string;
}

export function WorkflowMetrics({
  data,
  evidenceState = "unknown",
}: {
  data?: MetricsSummaryData | null;
  evidenceState?: EvidenceState;
}) {
  const hasData = Boolean(data);
  const display = (value: string | number) => hasData ? value : "—";
  const hasRuntimeEvidence = evidenceState === "runtime" || evidenceState === "event-ledger";
  return (
    <div className="space-y-4" data-testid="workflow-metrics">
      {/* 顶部核心数字卡片 */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Token 节省 */}
        <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4">
          <div className="flex items-center justify-between text-xs text-[#716a61]">
            <span>优化 Token 节省</span>
            <span className="inline-flex items-center text-emerald-600 font-semibold gap-0.5">
              <TrendingDown className="size-3.5" />
              {hasData ? `-${data!.reductionPercentage}%` : "未知"}
            </span>
          </div>
          <div className="text-2xl font-bold tracking-tight text-[#29251f] mt-2">
            {display(data?.tokensSaved?.toLocaleString() ?? "—")}
          </div>
          <div className="text-[11px] text-[#716a61] mt-1">
            {hasData ? `原始 ${data!.originalTokens.toLocaleString()} → 优化后 ${data!.optimizedTokens.toLocaleString()}` : "等待请求级 token 证据"}
          </div>
        </div>

        {/* 费用对照：估算与提供商实测分开展示 */}
        <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4">
          <div className="flex items-center justify-between text-xs text-[#716a61]">
            <span>成本节省对照</span>
            <span className="text-[10px] text-[#716a61] bg-[#f0ede5] px-1.5 py-0.5 rounded">
              分开归因
            </span>
          </div>
          <div className="flex items-baseline gap-2 mt-2">
            <span className="text-2xl font-bold tracking-tight text-[#c7652a]">
              {hasData ? `$${data!.providerConfirmedSavingsUsd.toFixed(2)}` : "—"}
            </span>
            <span className="text-xs text-[#716a61]">上游确认值</span>
          </div>
          <div className="text-[11px] text-[#716a61] mt-1">
            {hasData ? `本地模型定价估算: $${data!.estimatedSavingsUsd.toFixed(2)}` : "估算值等待价格与请求证据"}
          </div>
        </div>

        {/* 本地 vs Jev 分流 */}
        <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4">
          <div className="flex items-center justify-between text-xs text-[#716a61]">
            <span>决策引擎分流</span>
            <span className="text-xs text-[#716a61] font-mono">{display(data?.totalRequests ?? "—")} 请求</span>
          </div>
          <div className="flex items-center gap-3 mt-2">
            <div className="flex items-center gap-1.5 text-xs">
              <Cpu className="size-3.5 text-[#c7652a]" />
              <span className="font-semibold text-[#29251f]">{display(data?.localVsJevSplit.localPercent ?? "—")}{hasData ? "%" : ""}</span>
              <span className="text-[#716a61] text-[11px]">本地确定</span>
            </div>
            <div className="flex items-center gap-1.5 text-xs">
              <Sparkles className="size-3.5 text-amber-600" />
              <span className="font-semibold text-[#29251f]">{display(data?.localVsJevSplit.jevPercent ?? "—")}{hasData ? "%" : ""}</span>
              <span className="text-[#716a61] text-[11px]">Jev 语义</span>
            </div>
          </div>
          <div className="text-[11px] text-[#716a61] mt-1 flex items-center justify-between">
            <span>{hasData ? `降级回退率: ${data!.fallbackRate.toFixed(1)}%` : "降级回退率: 未知"}</span>
            <span>{hasData ? `恢复失败: ${data!.recoveryFailures}` : "恢复失败: 未知"}</span>
          </div>
        </div>

        {/* 决策延迟与 Sticky 缓存命中率 */}
        <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4">
          <div className="flex items-center justify-between text-xs text-[#716a61]">
            <span>性能与缓存</span>
            <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${hasRuntimeEvidence ? "text-emerald-700 bg-emerald-50 border border-emerald-200" : "text-[#716a61] bg-[#f0ede5] border border-[rgba(73,61,47,0.12)]"}`}>
              {hasRuntimeEvidence ? "运行时已确认" : "等待证据"}
            </span>
          </div>
          <div className="text-sm font-semibold text-[#29251f] mt-2">
            {hasData ? `P95: ${data!.p95LatencyLocalMs}ms (本地) / ${data!.p95LatencyJevMs}ms (Jev)` : "P95 决策延迟：未知"}
          </div>
          <div className="text-[11px] text-[#716a61] mt-1 flex items-center gap-1">
            <Database className="size-3" />
            <span>Sticky 缓存命中率: <strong>{hasData ? `${data!.stickyHitRate}%` : "未知"}</strong></span>
          </div>
        </div>
      </div>
    </div>
  );
}
