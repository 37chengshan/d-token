import { useEffect, useState } from "react";
import {
  BarChart3,
  ListFilter,
  History,
  ShieldCheck,
  FileText,
} from "lucide-react";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { WorkflowMetrics, type MetricsSummaryData } from "../analysis/WorkflowMetrics";
import { DecisionTimeline, type DecisionTimelineItem } from "../analysis/DecisionTimeline";
import { Changes } from "./Changes";
import { LogsSecurity } from "./LogsSecurity";
import type { PageId } from "../../lib/nav";
import { useLang } from "../../lib/i18n";
import { fetchAnalysisSnapshot, type EvidenceState } from "../../lib/contextEngine";

interface AnalysisLogsProps {
  setPage?: (page: PageId) => void;
  initialTab?: "overview" | "requests" | "changes" | "logs";
  onNavigateToDiagnostics?: (correlationId?: string) => void;
  metricsData?: MetricsSummaryData;
  timelineItems?: DecisionTimelineItem[];
}

export function AnalysisLogs({
  setPage,
  initialTab = "overview",
  onNavigateToDiagnostics,
  metricsData,
  timelineItems,
}: AnalysisLogsProps) {
  const lang = useLang();
  const zh = lang === "zh";
  const [activeTab, setActiveTab] = useState<string>(initialTab);
  const [snapshotState, setSnapshotState] = useState<{
    evidenceState: EvidenceState;
    metrics: MetricsSummaryData | null;
    timeline: DecisionTimelineItem[];
  }>({ evidenceState: "unknown", metrics: null, timeline: [] });

  useEffect(() => {
    if (metricsData !== undefined && timelineItems !== undefined) return;
    let mounted = true;
    void fetchAnalysisSnapshot().then((snapshot) => {
      if (!mounted) return;
      setSnapshotState({
        evidenceState: snapshot.evidenceState,
        metrics: snapshot.metrics,
        timeline: snapshot.timeline,
      });
    });
    return () => {
      mounted = false;
    };
  }, [metricsData, timelineItems]);

  const handleDiagnosticsNavigation = (correlationId?: string) => {
    if (onNavigateToDiagnostics) {
      onNavigateToDiagnostics(correlationId);
    } else if (setPage) {
      setPage("diagnostics");
    }
  };

  return (
    <div
      className="analysis-logs-page space-y-6 max-w-[1240px] mx-auto p-4 sm:p-6"
      data-testid="analysis-logs-page"
    >
      <Tabs value={activeTab} onValueChange={setActiveTab} className="w-full space-y-6">
        {/* 顶部主导航 */}
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-2 border-b border-[rgba(73,61,47,0.12)]">
          <div>
            <h1 className="sr-only">{zh ? "分析日志" : "Analysis & Logs"}</h1>
            <h2 className="text-xl font-bold tracking-tight text-[#29251f]">
              {zh ? "分析与运行日志" : "Analysis & Runtime Logs"}
            </h2>
            <p className="text-xs text-[#716a61] mt-0.5">
              {zh
                ? "监控上下文压缩效果、追踪请求决策时间线、查看配置变更记录与导出安全日志"
                : "Monitor context savings, trace decision timeline, view config changes, and export security logs"}
            </p>
          </div>

          <TabsList className="bg-[#f0ede5] border border-[rgba(73,61,47,0.12)] p-1 rounded-lg">
            <TabsTrigger
              value="overview"
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <BarChart3 className="size-3.5" />
              <span>{zh ? "运行概览" : "Overview"}</span>
            </TabsTrigger>
            <TabsTrigger
              value="requests"
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <ListFilter className="size-3.5" />
              <span>{zh ? "请求记录" : "Requests"}</span>
            </TabsTrigger>
            <TabsTrigger
              value="changes"
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <History className="size-3.5" />
              <span>{zh ? "变更与恢复" : "Changes"}</span>
            </TabsTrigger>
            <TabsTrigger
              value="logs"
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <FileText className="size-3.5" />
              <span>{zh ? "安全日志与导出" : "Logs"}</span>
            </TabsTrigger>
          </TabsList>
        </div>

        {/* 1. 运行概览页签 */}
        <TabsContent value="overview" className="space-y-4 focus-visible:outline-none">
          <WorkflowMetrics
            data={metricsData !== undefined ? metricsData : snapshotState.metrics}
            evidenceState={metricsData !== undefined && metricsData !== null ? "runtime" : snapshotState.evidenceState}
          />
        </TabsContent>

        {/* 2. 请求记录时间线页签 */}
        <TabsContent value="requests" className="space-y-4 focus-visible:outline-none">
          <DecisionTimeline
            items={timelineItems !== undefined ? timelineItems : snapshotState.timeline}
            evidenceState={timelineItems !== undefined && timelineItems.length > 0 ? "runtime" : snapshotState.evidenceState}
            onNavigateToDiagnostics={handleDiagnosticsNavigation}
          />
        </TabsContent>

        {/* 3. 变更与恢复事务页签 */}
        <TabsContent value="changes" className="space-y-4 focus-visible:outline-none">
          <Changes setPage={setPage} embedded />
        </TabsContent>

        {/* 4. 安全日志与导出页签 */}
        <TabsContent value="logs" className="space-y-4 focus-visible:outline-none">
          <LogsSecurity embedded />
        </TabsContent>
      </Tabs>
    </div>
  );
}
