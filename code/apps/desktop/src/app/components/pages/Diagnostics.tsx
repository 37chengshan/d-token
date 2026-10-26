import { useEffect, useState } from "react";
import {
  Stethoscope,
  CheckCircle2,
  AlertTriangle,
  HelpCircle,
  Laptop,
  ArrowRight,
  ShieldCheck,
  X,
  Layers,
} from "lucide-react";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Doctor } from "./Doctor";
import { Compatibility } from "./Compatibility";
import type { PageId } from "../../lib/nav";
import { useLang } from "../../lib/i18n";

interface DiagnosticsProps {
  setPage?: (page: PageId) => void;
  initialTab?: "doctor" | "compatibility";
  correlationId?: string;
  onClearCorrelationId?: () => void;
}

export function Diagnostics({
  setPage,
  initialTab = "doctor",
  correlationId,
  onClearCorrelationId,
}: DiagnosticsProps) {
  const lang = useLang();
  const zh = lang === "zh";
  const [activeTab, setActiveTab] = useState<string>(initialTab);
  const [activeCorrelationId, setActiveCorrelationId] = useState<string | undefined>(
    correlationId
  );

  useEffect(() => {
    setActiveCorrelationId(correlationId);
  }, [correlationId]);

  const handleClear = () => {
    setActiveCorrelationId(undefined);
    onClearCorrelationId?.();
  };

  return (
    <div
      className="diagnostics-page space-y-6 max-w-[1240px] mx-auto p-4 sm:p-6"
      data-testid="diagnostics-page"
    >
      <Tabs value={activeTab} onValueChange={setActiveTab} className="w-full space-y-6">
        {/* 顶部主导航 */}
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-2 border-b border-[rgba(73,61,47,0.12)]">
          <div>
            <h1 className="sr-only">{zh ? "诊断" : "Diagnostics"}</h1>
            <h2 className="text-xl font-bold tracking-tight text-[#29251f]">
              {zh ? "系统诊断与兼容性" : "Diagnostics & Compatibility"}
            </h2>
            <p className="text-xs text-[#716a61] mt-0.5">
              {zh
                ? "检查本地 Loopback 监听、运行时健康度、上游 Provider 连通性、环境兼容性与一键事务修复"
                : "Check local loopback listeners, runtime health, upstream provider reachability, environment compatibility, and transactional repair"}
            </p>
          </div>

          <TabsList className="bg-[#f0ede5] border border-[rgba(73,61,47,0.12)] p-1 rounded-lg">
            <TabsTrigger
              value="doctor"
              className="flex items-center gap-2 px-3.5 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <Stethoscope className="size-3.5" />
              <span>{zh ? "系统与运行时诊断" : "Doctor"}</span>
            </TabsTrigger>
            <TabsTrigger
              value="compatibility"
              className="flex items-center gap-2 px-3.5 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <Laptop className="size-3.5" />
              <span>{zh ? "兼容性与平台矩阵" : "Compatibility"}</span>
            </TabsTrigger>
          </TabsList>
        </div>

        {/* 关联请求过滤条（若从分析日志异常记录跳转而来） */}
        {activeCorrelationId && (
          <div className="flex items-center justify-between gap-3 rounded-lg bg-amber-50 border border-amber-200 p-3 text-xs text-amber-900">
            <div className="flex items-center gap-2">
              <AlertTriangle className="size-4 text-amber-600 shrink-0" />
              <span>
                正在针对失败请求进行定向诊断分析: <strong className="font-mono">{activeCorrelationId}</strong>
              </span>
            </div>
            <button
              type="button"
              onClick={handleClear}
              className="inline-flex items-center gap-1 rounded px-2 py-1 bg-amber-100 hover:bg-amber-200 text-amber-800 transition-colors"
            >
              <X className="size-3" />
              <span>清除过滤</span>
            </button>
          </div>
        )}

        {/* 诊断四分类产品契约提示条 */}
        <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4 text-xs">
          <div className="font-semibold text-[#29251f] mb-2 flex items-center gap-1.5">
            <ShieldCheck className="size-4 text-[#c7652a]" />
            <span>诊断健康分类规则契约</span>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-3 text-[11px]">
            <div className="rounded-lg bg-[#f0ede5] p-2.5 border border-[rgba(73,61,47,0.08)]">
              <div className="font-semibold text-rose-700 flex items-center gap-1">
                <AlertTriangle className="size-3" /> 1. 需处理 (Action Required)
              </div>
              <p className="text-[#716a61] mt-1 leading-normal">
                影响核心请求转发或凭据受损的问题，提供一键原子修复与回退
              </p>
            </div>

            <div className="rounded-lg bg-[#f0ede5] p-2.5 border border-[rgba(73,61,47,0.08)]">
              <div className="font-semibold text-amber-800 flex items-center gap-1">
                <HelpCircle className="size-3" /> 2. 可选未配置 (Optional)
              </div>
              <p className="text-[#716a61] mt-1 leading-normal">
                未配置 Jev Key 为正常本地模式，未建立代码图谱为可选能力，不判定为故障
              </p>
            </div>

            <div className="rounded-lg bg-[#f0ede5] p-2.5 border border-[rgba(73,61,47,0.08)]">
              <div className="font-semibold text-emerald-800 flex items-center gap-1">
                <CheckCircle2 className="size-3" /> 3. 正常 (Healthy)
              </div>
              <p className="text-[#716a61] mt-1 leading-normal">
                所有服务、监听端口与环境检测项均通过，运行证据充分且最新
              </p>
            </div>

            <div className="rounded-lg bg-[#f0ede5] p-2.5 border border-[rgba(73,61,47,0.08)]">
              <div className="font-semibold text-[#716a61] flex items-center gap-1">
                <Layers className="size-3" /> 4. 证据不足 (Insufficient)
              </div>
              <p className="text-[#716a61] mt-1 leading-normal">
                尚未执行扫描或缺乏最新心跳，明确标注状态待刷新，拒绝伪造绿色假象
              </p>
            </div>
          </div>
        </div>

        {/* 1. Doctor 系统与运行时诊断页签 */}
        <TabsContent value="doctor" className="space-y-4 focus-visible:outline-none">
          <Doctor setPage={setPage || (() => {})} showHeading={false} />
        </TabsContent>

        {/* 2. 兼容性与平台矩阵页签 */}
        <TabsContent value="compatibility" className="space-y-4 focus-visible:outline-none">
          <Compatibility />
        </TabsContent>
      </Tabs>
    </div>
  );
}
