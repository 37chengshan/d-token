import { useEffect, useState, useCallback } from "react";
import { Cpu, Network, Sparkles, ArrowRight, ShieldCheck, Activity, CheckCircle2 } from "lucide-react";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { EngineModeCard } from "../context-engine/EngineModeCard";
import { WorkflowSummary } from "../context-engine/WorkflowSummary";
import { Install } from "./Install";
import { CodebaseMemory } from "./CodebaseMemory";
import {
  fetchEngineState,
  storeJevCredential,
  clearJevCredential,
  testJevConnection,
  updateEngineMode,
  type EngineMode,
  type EngineSetupState,
} from "../../lib/contextEngine";
import type { PageId } from "../../lib/nav";
import { useLang } from "../../lib/i18n";

interface ConfigurationProps {
  setPage?: (page: PageId) => void;
  initialTab?: "compression" | "code-graph";
}

const INITIAL_ENGINE_STATE: EngineSetupState = {
  mode: "local",
  health: {
    source: "unknown",
    freshnessMs: null,
    credentialState: "not-configured",
    overall: "unknown",
    reasonCodes: ["native_unavailable"],
  },
  evidenceState: "unknown",
  hasCredential: false,
  workflows: [
    {
      id: "tool_output_retention",
      title: "工具输出在线裁剪",
      description: "在执行大段命令输出或测试堆栈进入上下文前裁剪噪声，保持关键错误与返回",
      enabled: true,
      active: false,
      status: "shadow",
      fallback: "local-heuristic",
    },
    {
      id: "arrival_span_retention",
      title: "增量历史选择 (Sticky)",
      description: "多轮对话间缓存已验证的重要判定，避免每次重复调用语义评估",
      enabled: true,
      active: false,
      status: "shadow",
      fallback: "local-heuristic",
    },
    {
      id: "session_compaction",
      title: "长会话 Compaction",
      description: "当会话接近窗口上限时自动提取紧凑状态，淘汰过期历史",
      enabled: true,
      active: false,
      status: "shadow",
      fallback: "passthrough",
    },
    {
      id: "context_ranking",
      title: "代码与文件上下文排序",
      description: "根据当前任务意图重排代码片段与检索结果优先级",
      enabled: true,
      active: false,
      status: "shadow",
      fallback: "local-heuristic",
    },
    {
      id: "rule_skill_selection",
      title: "规则与 Skill 动态注入",
      description: "仅激活与当前请求相关的指南规则与能力模式，减少通用提示词冗余",
      enabled: false,
      active: false,
      status: "disabled",
      fallback: "passthrough",
    },
  ],
  recentRequestObserved: false,
  lastRequestSummary: null,
};

export function Configuration({ setPage, initialTab = "compression" }: ConfigurationProps) {
  const lang = useLang();
  const zh = lang === "zh";
  const [activeTab, setActiveTab] = useState<string>(initialTab);
  const [engineState, setEngineState] = useState<EngineSetupState>(INITIAL_ENGINE_STATE);

  const loadState = useCallback(async () => {
    try {
      const state = await fetchEngineState();
      if (state) setEngineState(state);
    } catch {
      // keep initial fallback
    }
  }, []);

  useEffect(() => {
    loadState();
  }, [loadState]);

  const handleSelectMode = async (mode: EngineMode) => {
    const res = await updateEngineMode(mode);
    if (res.ok) {
      setEngineState((prev) => ({ ...prev, mode: res.mode }));
    }
  };

  const handleStoreCredential = async (key: string): Promise<boolean> => {
    const res = await storeJevCredential(key);
    if (res.ok) {
      await loadState();
      return true;
    }
    return false;
  };

  const handleClearCredential = async () => {
    const res = await clearJevCredential();
    if (res.ok) {
      await loadState();
    }
  };

  const handleTestConnection = async () => {
    return await testJevConnection();
  };

  const isJevActive = engineState.mode === "jev-shadow" || engineState.mode === "jev-active";
  const runtimeConfirmed = engineState.health.source === "runtime" || engineState.health.source === "event-ledger";
  const healthLabel = engineState.health.overall === "healthy"
    ? "运行时已确认"
    : engineState.health.overall === "partial"
      ? "运行时部分可用"
      : engineState.health.overall === "unavailable"
        ? "运行时不可用"
        : "等待运行时证据";
  const healthClass = engineState.health.overall === "healthy"
    ? "bg-emerald-50 border-emerald-200 text-emerald-800"
    : engineState.health.overall === "partial"
      ? "bg-amber-50 border-amber-200 text-amber-800"
      : "bg-[#f0ede5] border-[rgba(73,61,47,0.12)] text-[#716a61]";

  return (
    <div className="configuration-page space-y-6 max-w-[1240px] mx-auto p-4 sm:p-6" data-testid="configuration-page">
      <Tabs value={activeTab} onValueChange={setActiveTab} className="w-full space-y-6">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-2 border-b border-[rgba(73,61,47,0.12)]">
          <div>
            <h1 className="sr-only">{zh ? "配置" : "Configuration"}</h1>
            <h2 className="text-xl font-bold tracking-tight text-[#29251f]">
              {zh ? "配置与连接" : "Configuration & Connections"}
            </h2>
            <p className="text-xs text-[#716a61] mt-0.5">
              {zh
                ? "配置本地上下文压缩引擎、连接已发现的编码 Agent、以及管理项目代码图谱"
                : "Configure local context compression engine, connect detected coding agents, and manage project code graph"}
            </p>
          </div>

          <TabsList className="bg-[#f0ede5] border border-[rgba(73,61,47,0.12)] p-1 rounded-lg">
            <TabsTrigger
              value="compression"
              className="flex items-center gap-2 px-3.5 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <Cpu className="size-3.5" />
              <span>{zh ? "压缩引擎" : "Compression Engine"}</span>
            </TabsTrigger>
            <TabsTrigger
              value="code-graph"
              className="flex items-center gap-2 px-3.5 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <Network className="size-3.5" />
              <span>{zh ? "代码图谱" : "Code Graph"}</span>
              <span className="rounded-full bg-purple-500/15 px-1.5 py-0.2 text-[9px] font-semibold text-purple-700">
                Exp
              </span>
            </TabsTrigger>
          </TabsList>
        </div>

        {/* 1. 压缩引擎页签内容 */}
        <TabsContent value="compression" className="space-y-6 focus-visible:outline-none">
          {/* 状态与最近请求概览条 */}
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4 text-xs">
            <div className="flex items-center gap-3">
              <div className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 font-medium ${healthClass}`}>
                <span className={`size-2 rounded-full ${runtimeConfirmed ? "bg-emerald-500" : "bg-[#9b9287]"}`}></span>
                <span>{healthLabel}</span>
              </div>
              <span className="text-[#716a61]">
                当前模式: <strong className="text-[#29251f]">{engineState.mode === "local" ? "本地确定性压缩" : "Jev 语义增强"}</strong>
              </span>
            </div>

            <div className="flex items-center gap-3">
              {engineState.lastRequestSummary ? (
                <div className="flex items-center gap-2 text-[#716a61]">
                  <span>最近一次请求已优化: 节省 {engineState.lastRequestSummary.tokensSaved} tokens</span>
                  {setPage && (
                    <button
                      type="button"
                      onClick={() => setPage("analysis-logs")}
                      className="inline-flex items-center gap-1 text-[#c7652a] font-medium hover:underline"
                    >
                      <span>查看效果</span>
                      <ArrowRight className="size-3" />
                    </button>
                  )}
                </div>
              ) : (
                <span className="text-[#716a61]">尚未观察到经过 d-token 的真实请求</span>
              )}
            </div>
          </div>

          {/* 引擎模式选择与配置 */}
          <section className="space-y-3">
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-semibold text-[#29251f]">1. 引擎模式与决策层</h2>
              <span className="text-xs text-[#716a61]">
                无需 Jev Key 即可享受完整的离线本地压缩能力
              </span>
            </div>

            <EngineModeCard
              selectedMode={engineState.mode}
              onSelectMode={handleSelectMode}
              hasCredential={engineState.hasCredential}
              health={engineState.health}
              onStoreCredential={handleStoreCredential}
              onClearCredential={handleClearCredential}
              onTestConnection={handleTestConnection}
            />
          </section>

          {/* 编码工具连接与发现 */}
          <section className="space-y-3">
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-semibold text-[#29251f]">2. 已发现的编码工具与自动连接</h2>
              <span className="text-xs text-[#716a61]">
                单次事务配置，自动备份并支持一键恢复
              </span>
            </div>

            <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4">
              <Install />
            </div>
          </section>

          {/* 实时工作流状态摘要 */}
          <section className="space-y-3">
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-semibold text-[#29251f]">3. 上下文优化工作流执行状态</h2>
              {setPage && (
                <button
                  type="button"
                      onClick={() => setPage("advanced-configuration")}
                  className="text-xs text-[#c7652a] font-medium hover:underline"
                >
                  前往高级配置微调策略 →
                </button>
              )}
            </div>

            <WorkflowSummary
              workflows={engineState.workflows}
              isJevActive={isJevActive}
              evidenceState={engineState.evidenceState ?? engineState.health.overall}
            />
          </section>
        </TabsContent>

        {/* 2. 代码图谱页签内容 */}
        <TabsContent value="code-graph" className="space-y-4 focus-visible:outline-none">
          <CodebaseMemory embedded />
        </TabsContent>
      </Tabs>
    </div>
  );
}
