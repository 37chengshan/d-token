import { useState } from "react";
import {
  Sliders,
  BookOpen,
  Route as RouteIcon,
  Archive,
  Cpu,
  Database,
  ShieldCheck,
  CheckCircle2,
  Trash2,
  AlertTriangle,
} from "lucide-react";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { WorkflowPolicyEditor } from "../context-engine/WorkflowPolicyEditor";
import { LearningPolicyPanel } from "../context-engine/LearningPolicyPanel";
import { OperationDialog } from "../common/OperationDialog";
import type { PageId } from "../../lib/nav";
import { useLang } from "../../lib/i18n";
import { clearDecisionCache, cleanContextRetention } from "../../lib/contextEngine";

interface AdvancedConfigurationProps {
  setPage?: (page: PageId) => void;
  initialTab?: "workflows" | "learning" | "routing-cache" | "recovery";
}

export function AdvancedConfiguration({
  setPage,
  initialTab = "workflows",
}: AdvancedConfigurationProps) {
  const lang = useLang();
  const zh = lang === "zh";
  const [activeTab, setActiveTab] = useState<string>(initialTab);

  // Routing & Cache state
  const [routingAuthEnabled, setRoutingAuthEnabled] = useState(false);
  const [cacheTtlSeconds, setCacheTtlSeconds] = useState(7200);
  const [maxCacheEntries, setMaxCacheEntries] = useState(1000);
  const [cacheClearing, setCacheClearing] = useState(false);
  const [cacheClearNotice, setCacheClearNotice] = useState<{ kind: "unknown" | "runtime"; message: string } | null>(null);

  // Recovery & Retention state
  const [retentionDays, setRetentionDays] = useState(7);
  const [maxStorageMb, setMaxStorageMb] = useState(500);
  const [cleanConfirmOpen, setCleanConfirmOpen] = useState(false);
  const [cleanCleaning, setCleanCleaning] = useState(false);
  const [storageCleanNotice, setStorageCleanNotice] = useState<{ kind: "unknown" | "runtime"; message: string } | null>(null);

  const handleClearCache = async () => {
    setCacheClearing(true);
    try {
      const result = await clearDecisionCache();
      setCacheClearNotice({ kind: result.ok ? "runtime" : "unknown", message: result.summary });
    } finally {
      setCacheClearing(false);
    }
  };

  const handleCleanStorage = async () => {
    setCleanCleaning(true);
    try {
      const result = await cleanContextRetention();
      setStorageCleanNotice({ kind: result.ok ? "runtime" : "unknown", message: result.summary });
      if (result.ok) setCleanConfirmOpen(false);
    } finally {
      setCleanCleaning(false);
    }
  };

  return (
    <div
      className="advanced-configuration-page space-y-6 max-w-[1240px] mx-auto p-4 sm:p-6"
      data-testid="advanced-configuration-page"
    >
      <Tabs value={activeTab} onValueChange={setActiveTab} className="w-full space-y-6">
        {/* 顶部主导航与 4 个分组页签 */}
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-2 border-b border-[rgba(73,61,47,0.12)]">
          <div>
            <h1 className="sr-only">{zh ? "高级配置" : "Advanced Configuration"}</h1>
            <h2 className="text-xl font-bold tracking-tight text-[#29251f]">
              {zh ? "高级策略配置" : "Advanced Strategy Configuration"}
            </h2>
            <p className="text-xs text-[#716a61] mt-0.5">
              {zh
                ? "微调上下文工作流阈值、Jev 实时决策策略、智能学习沉淀、模型路由授权与原文留存"
                : "Fine-tune workflow thresholds, Jev real-time decisions, smart learning, model routing, and payload retention"}
            </p>
          </div>

          <TabsList className="bg-[#f0ede5] border border-[rgba(73,61,47,0.12)] p-1 rounded-lg">
            <TabsTrigger
              value="workflows"
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <Sliders className="size-3.5" />
              <span>{zh ? "Jev 工作流" : "Jev Workflows"}</span>
            </TabsTrigger>
            <TabsTrigger
              value="learning"
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <BookOpen className="size-3.5" />
              <span>{zh ? "智能学习" : "Smart Learning"}</span>
            </TabsTrigger>
            <TabsTrigger
              value="routing-cache"
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <RouteIcon className="size-3.5" />
              <span>{zh ? "路由与缓存" : "Routing & Cache"}</span>
            </TabsTrigger>
            <TabsTrigger
              value="recovery"
              className="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded-md data-[state=active]:bg-[#fbf8f2] data-[state=active]:text-[#29251f] data-[state=active]:shadow-sm text-[#716a61]"
            >
              <Archive className="size-3.5" />
              <span>{zh ? "恢复与留存" : "Recovery & Retention"}</span>
            </TabsTrigger>
          </TabsList>
        </div>

        {/* 1. Jev 工作流组 */}
        <TabsContent value="workflows" className="space-y-4 focus-visible:outline-none">
          <WorkflowPolicyEditor />
        </TabsContent>

        {/* 2. 智能学习组 */}
        <TabsContent value="learning" className="space-y-4 focus-visible:outline-none">
          <LearningPolicyPanel />
        </TabsContent>

        {/* 3. 路由与缓存组 */}
        <TabsContent value="routing-cache" className="space-y-6 focus-visible:outline-none">
          {/* 独立路由授权卡片 */}
          <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-5 text-xs space-y-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <div className="size-8 rounded-lg bg-[#c7652a]/10 text-[#c7652a] flex items-center justify-center font-bold">
                  <RouteIcon className="size-4" />
                </div>
                <div>
                  <h3 className="text-sm font-semibold text-[#29251f]">
                    模型路由独立授权
                  </h3>
                  <span className="text-xs text-[#716a61]">
                    模型路由与上下文优化严格分开授权、分开计量、分开配置
                  </span>
                </div>
              </div>

              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={routingAuthEnabled}
                  onChange={(e) => setRoutingAuthEnabled(e.target.checked)}
                  className="rounded border-[rgba(73,61,47,0.3)] text-[#c7652a] focus:ring-[#c7652a]"
                />
                <span className="font-semibold text-[#29251f]">
                  {routingAuthEnabled ? "已单独授权" : "未授权 (仅透传原模型)"}
                </span>
              </label>
            </div>

            <p className="text-xs text-[#716a61] leading-relaxed">
              根据产品契约，启用 Jev 实时语义压缩决不代表授权应用自动更换用户的主模型。仅当此处显式勾选授权后，智能路由候选池才会参与轻量任务重定向。
            </p>

            <div className="rounded-lg bg-[#f0ede5] p-3 text-xs border border-[rgba(73,61,47,0.08)]">
              <div className="font-medium text-[#29251f] mb-1">受保护候选模型规则:</div>
              <ul className="list-disc list-inside space-y-1 text-[#716a61]">
                <li>只针对非结构化解释性提问进行上游降级，绝不更改代码生成与编辑请求的目标模型</li>
                <li>所有路由重定向决策在分析日志中 100% 记录并附带原始模型对照证据</li>
              </ul>
            </div>
          </div>

          {/* 决策缓存配置卡片 */}
          <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-5 text-xs space-y-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <div className="size-8 rounded-lg bg-blue-500/10 text-blue-600 flex items-center justify-center font-bold">
                  <Database className="size-4" />
                </div>
                <div>
                  <h3 className="text-sm font-semibold text-[#29251f]">
                    决策缓存策略 (Sticky Cache)
                  </h3>
                  <span className="text-xs text-[#716a61]">
                    跨轮次复用已验证的语义判定，降低决策耗时与网络请求
                  </span>
                </div>
              </div>

              <button
                type="button"
                onClick={handleClearCache}
                disabled={cacheClearing}
                className="rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-3 py-1.5 text-xs font-medium text-[#29251f] hover:bg-[#eee8df] transition-colors"
              >
                {cacheClearing ? "清空中..." : "清空当前缓存"}
              </button>
            </div>

            {cacheClearNotice && (
              <div className={`rounded-lg border p-2.5 text-xs ${cacheClearNotice.kind === "runtime" ? "bg-blue-50 border-blue-200 text-blue-900" : "bg-amber-50 border-amber-200 text-amber-900"}`}>
                {cacheClearNotice.message}
              </div>
            )}

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-1">
              <div>
                <label htmlFor="context-cache-ttl" className="block text-[11px] text-[#716a61] mb-1 font-medium">
                  默认 Sticky TTL 存活期 (秒)
                </label>
                <input
                  id="context-cache-ttl"
                  type="number"
                  value={cacheTtlSeconds}
                  onChange={(e) => setCacheTtlSeconds(parseInt(e.target.value, 10) || 0)}
                  className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-3 py-1.5 text-xs text-[#29251f] focus:border-[#c7652a] focus:outline-none"
                />
              </div>

              <div>
                <label htmlFor="context-cache-max-entries" className="block text-[11px] text-[#716a61] mb-1 font-medium">
                  最大缓存条目容量 (Entries)
                </label>
                <input
                  id="context-cache-max-entries"
                  type="number"
                  value={maxCacheEntries}
                  onChange={(e) => setMaxCacheEntries(parseInt(e.target.value, 10) || 0)}
                  className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-3 py-1.5 text-xs text-[#29251f] focus:border-[#c7652a] focus:outline-none"
                />
              </div>
            </div>
          </div>
        </TabsContent>

        {/* 4. 原文恢复与留存组 */}
        <TabsContent value="recovery" className="space-y-6 focus-visible:outline-none">
          <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-5 text-xs space-y-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <div className="size-8 rounded-lg bg-emerald-500/10 text-emerald-600 flex items-center justify-center font-bold">
                  <Archive className="size-4" />
                </div>
                <div>
                  <h3 className="text-sm font-semibold text-[#29251f]">
                    原文归档与恢复留存策略
                  </h3>
                  <span className="text-xs text-[#716a61]">
                    被裁剪的工具输出和上下文片段保存在本地加密留存区，确保随时可无损找回
                  </span>
                </div>
              </div>

              <button
                type="button"
                onClick={() => setCleanConfirmOpen(true)}
                className="rounded-md border border-rose-200 bg-rose-50 px-3 py-1.5 text-xs font-medium text-rose-700 hover:bg-rose-100 transition-colors flex items-center gap-1.5"
              >
                <Trash2 className="size-3.5" />
                <span>清理过期归档</span>
              </button>
            </div>

            {storageCleanNotice && (
              <div className={`rounded-lg border p-2.5 text-xs ${storageCleanNotice.kind === "runtime" ? "bg-emerald-50 border-emerald-200 text-emerald-900" : "bg-amber-50 border-amber-200 text-amber-900"}`}>
                {storageCleanNotice.message}
              </div>
            )}

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-1">
              <div>
                <label htmlFor="context-retention-days" className="block text-[11px] text-[#716a61] mb-1 font-medium">
                  归档保留天数 (Retention Days)
                </label>
                <input
                  id="context-retention-days"
                  type="number"
                  value={retentionDays}
                  onChange={(e) => setRetentionDays(parseInt(e.target.value, 10) || 0)}
                  className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-3 py-1.5 text-xs text-[#29251f] focus:border-[#c7652a] focus:outline-none"
                />
              </div>

              <div>
                <label htmlFor="context-retention-max-storage" className="block text-[11px] text-[#716a61] mb-1 font-medium">
                  本地最大磁盘配额 (MB)
                </label>
                <input
                  id="context-retention-max-storage"
                  type="number"
                  value={maxStorageMb}
                  onChange={(e) => setMaxStorageMb(parseInt(e.target.value, 10) || 0)}
                  className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-3 py-1.5 text-xs text-[#29251f] focus:border-[#c7652a] focus:outline-none"
                />
              </div>
            </div>

            <div className="rounded-lg bg-[#f0ede5] p-3 border border-[rgba(73,61,47,0.08)] flex items-center justify-between text-xs">
              <div className="flex items-center gap-2">
                <ShieldCheck className="size-4 text-[#9b9287]" />
                <span className="text-[#29251f]">当前本地加密归档占用: <strong>未知</strong> / {maxStorageMb} MB</span>
              </div>
              <span className="text-[#716a61]">淘汰策略: 等待运行时证据</span>
            </div>
          </div>

          <OperationDialog
            open={cleanConfirmOpen}
            onOpenChange={setCleanConfirmOpen}
            title="确认清理已过期的原文归档"
            summary="清理操作将删除超过保留期限的上下文块备份。当前活跃会话和未过期引用不会受影响。"
            confirmLabel={cleanCleaning ? "正在清理..." : "确认清理"}
            cancelLabel="取消"
            risk="warning"
            pending={cleanCleaning}
            onConfirm={handleCleanStorage}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}
