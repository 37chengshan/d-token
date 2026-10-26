import { useState } from "react";
import { Sparkles, BookOpen, Layers } from "lucide-react";
import { SmartLearning } from "../pages/SmartLearning";

export function LearningPolicyPanel() {
  const [scope, setScope] = useState<"global" | "project">("global");

  return (
    <div className="space-y-4" data-testid="learning-policy-panel">
      {/* 头部控制栏：作用域选择与说明 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4 text-xs">
        <div>
          <div className="flex items-center gap-2">
            <BookOpen className="size-4 text-[#c7652a]" />
            <h3 className="text-sm font-semibold text-[#29251f]">
              智能学习与项目规则沉淀
            </h3>
          </div>
          <p className="text-xs text-[#716a61] mt-0.5">
            从实际交互沉淀上下文优化偏好、项目规则规范与指南，避免重复提示词开销
          </p>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          <span className="text-xs text-[#716a61]">生效作用域:</span>
          <div className="inline-flex rounded-lg bg-[#f0ede5] p-0.5 border border-[rgba(73,61,47,0.1)] text-xs">
            <button
              type="button"
              onClick={() => setScope("global")}
              className={`px-3 py-1 rounded-md font-medium transition-all ${
                scope === "global"
                  ? "bg-[#fbf8f2] text-[#29251f] shadow-xs"
                  : "text-[#716a61] hover:text-[#29251f]"
              }`}
            >
              全局默认
            </button>
            <button
              type="button"
              onClick={() => setScope("project")}
              className={`px-3 py-1 rounded-md font-medium transition-all ${
                scope === "project"
                  ? "bg-[#fbf8f2] text-[#29251f] shadow-xs"
                  : "text-[#716a61] hover:text-[#29251f]"
              }`}
            >
              项目专属覆盖
            </button>
          </div>
        </div>
      </div>

      {/* 嵌入实际的 SmartLearning 发现与管理工作台 */}
      <div className="rounded-xl border border-[rgba(73,61,47,0.13)] bg-[#fbf8f2] p-4">
        <SmartLearning />
      </div>
    </div>
  );
}
