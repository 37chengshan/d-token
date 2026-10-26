import { useState } from "react";
import { CheckCircle2, ShieldCheck, Zap, Sparkles, KeyRound, AlertCircle, RefreshCw } from "lucide-react";
import type { EngineMode, DecisionHealth } from "../../lib/contextEngine";

interface EngineModeCardProps {
  selectedMode: EngineMode;
  onSelectMode: (mode: EngineMode) => void;
  hasCredential: boolean;
  health: DecisionHealth;
  onStoreCredential: (key: string) => Promise<boolean>;
  onClearCredential: () => Promise<void>;
  onTestConnection: () => Promise<{ ok: boolean; latencyMs: number; error?: string }>;
}

export function EngineModeCard({
  selectedMode,
  onSelectMode,
  hasCredential,
  health,
  onStoreCredential,
  onClearCredential,
  onTestConnection,
}: EngineModeCardProps) {
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [savingKey, setSavingKey] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; latencyMs: number; error?: string } | null>(null);

  const handleSaveKey = async () => {
    if (!apiKeyInput.trim()) return;
    setSavingKey(true);
    try {
      const ok = await onStoreCredential(apiKeyInput.trim());
      if (ok) {
        setApiKeyInput(""); // Never keep credential in memory
      }
    } finally {
      setSavingKey(false);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const res = await onTestConnection();
      setTestResult(res);
    } finally {
      setTesting(false);
    }
  };

  const isLocal = selectedMode === "local";
  const isJev = selectedMode === "jev-shadow" || selectedMode === "jev-active";
  const healthLabel = health.overall === "healthy"
    ? "运行时已确认"
    : health.overall === "partial"
      ? "部分可用"
      : health.overall === "unavailable"
        ? "不可用"
        : "待验证";
  const healthEvidenceAvailable = health.source === "runtime" || health.source === "event-ledger";
  const selectMode = (mode: EngineMode) => onSelectMode(mode);
  const handleModeKeyDown = (event: React.KeyboardEvent, mode: EngineMode) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      selectMode(mode);
    }
  };

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* 1. 本地模式卡片 */}
      <div
        role="button"
        tabIndex={0}
        aria-pressed={isLocal}
        onClick={() => selectMode("local")}
        onKeyDown={(event) => handleModeKeyDown(event, "local")}
        className={`relative flex flex-col justify-between p-5 rounded-xl border transition-all cursor-pointer text-left ${
          isLocal
            ? "border-[#c7652a] bg-[#fbf8f2] shadow-sm ring-1 ring-[#c7652a]/20"
            : "border-[rgba(73,61,47,0.13)] bg-[#fbf8f2]/60 hover:border-[rgba(73,61,47,0.25)] hover:bg-[#fbf8f2]"
        }`}
      >
        <div>
          <div className="flex items-center justify-between pb-3">
            <div className="flex items-center gap-2.5">
              <div className="size-8 rounded-lg bg-[#c7652a]/10 text-[#c7652a] flex items-center justify-center font-bold">
                <Zap className="size-4" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-[#29251f]">本地确定性引擎</h3>
                <span className="text-xs text-[#716a61]">Rust 本地高性能运行时</span>
              </div>
            </div>
            {isLocal && (
              <span className="inline-flex items-center gap-1 rounded-full bg-[#c7652a]/15 px-2.5 py-0.5 text-xs font-semibold text-[#a94f1c]">
                <CheckCircle2 className="size-3.5" /> 当前模式
              </span>
            )}
          </div>

          <p className="text-xs text-[#716a61] leading-relaxed pt-1">
            零网络依赖，不需要任何 API 密钥。基于确定的语法结构、工具输出模式和启发式算法在本地完成裁剪与保护，完全保留所有关键文本与诊断结果。
          </p>

          <ul className="mt-4 space-y-2 text-xs text-[#29251f]">
            <li className="flex items-center gap-2">
              <ShieldCheck className="size-3.5 text-emerald-600" />
              <span>100% 本地运行，隐私数据不出境</span>
            </li>
            <li className="flex items-center gap-2">
              <ShieldCheck className="size-3.5 text-emerald-600" />
              <span>不下载外部大模型，内存占用低至 20MB</span>
            </li>
            <li className="flex items-center gap-2">
              <ShieldCheck className="size-3.5 text-emerald-600" />
              <span>处理延迟与资源占用由运行时证据提供</span>
            </li>
          </ul>
        </div>

        <div className="mt-5 pt-3 border-t border-[rgba(73,61,47,0.08)] flex items-center justify-between text-xs text-[#716a61]">
          <span>状态: {isLocal && healthEvidenceAvailable ? "运行时已确认" : isLocal ? "配置可用，等待运行时验证" : "未选择本地模式"}</span>
          <span className="font-mono text-[11px]">Local Engine</span>
        </div>
      </div>

      {/* 2. Jev 决策层卡片 */}
      <div
        role="button"
        tabIndex={0}
        aria-pressed={isJev}
        onClick={() => selectMode(selectedMode === "local" ? "jev-shadow" : selectedMode)}
        onKeyDown={(event) => handleModeKeyDown(event, "jev-shadow")}
        className={`relative flex flex-col justify-between p-5 rounded-xl border transition-all cursor-pointer text-left ${
          isJev
            ? "border-[#c7652a] bg-[#fbf8f2] shadow-sm ring-1 ring-[#c7652a]/20"
            : "border-[rgba(73,61,47,0.13)] bg-[#fbf8f2]/60 hover:border-[rgba(73,61,47,0.25)] hover:bg-[#fbf8f2]"
        }`}
      >
        <div>
          <div className="flex items-center justify-between pb-3">
            <div className="flex items-center gap-2.5">
              <div className="size-8 rounded-lg bg-amber-500/10 text-amber-600 flex items-center justify-center font-bold">
                <Sparkles className="size-4" />
              </div>
              <div>
                <h3 className="text-sm font-semibold text-[#29251f]">Jev 实时语义决策</h3>
                <span className="text-xs text-[#716a61]">智能保留评估与相关性排序</span>
              </div>
            </div>
            {isJev && (
              <span className="inline-flex items-center gap-1 rounded-full bg-amber-500/15 px-2.5 py-0.5 text-xs font-semibold text-amber-800">
                <CheckCircle2 className="size-3.5" />
                {selectedMode === "jev-shadow" ? "Shadow (预览对照)" : "Active (实时接入)"}
              </span>
            )}
          </div>

          <p className="text-xs text-[#716a61] leading-relaxed pt-1">
            在工具结果与增量历史进入上下文前，利用语义推理判断可恢复性与任务相关度。压缩执行与加密存储仍由本地 Rust 引擎负责，失败自动回退。
          </p>

          {/* 凭据输入与状态 */}
          <div className="mt-4 rounded-lg bg-[#f0ede5] p-3 border border-[rgba(73,61,47,0.1)]">
            <div className="flex items-center justify-between text-xs pb-1.5">
              <span className="font-medium text-[#29251f]">Jev 凭据配置</span>
              {hasCredential ? (
                <span className="text-emerald-600 font-medium flex items-center gap-1">
                  <CheckCircle2 className="size-3" /> 已加密保存
                </span>
              ) : (
                <span className="text-amber-700 font-medium flex items-center gap-1">
                  <AlertCircle className="size-3" /> 未配置密钥
                </span>
              )}
            </div>

            {!hasCredential ? (
              <div className="flex gap-2 mt-2" onClick={(e) => e.stopPropagation()}>
                <div className="relative flex-1">
                  <KeyRound className="absolute left-2.5 top-2.5 size-3.5 text-[#716a61]" />
                  <input
                    type="password"
                    placeholder="输入 Jev API Key (仅存操作系统钥匙串)"
                    value={apiKeyInput}
                    onChange={(e) => setApiKeyInput(e.target.value)}
                    className="w-full rounded-md border border-[rgba(73,61,47,0.18)] bg-white pl-8 pr-3 py-1.5 text-xs text-[#29251f] placeholder:text-[#716a61]/60 focus:border-[#c7652a] focus:outline-none"
                  />
                </div>
                <button
                  type="button"
                  onClick={handleSaveKey}
                  disabled={savingKey || !apiKeyInput.trim()}
                  className="rounded-md bg-[#c7652a] px-3 py-1.5 text-xs font-medium text-white hover:bg-[#a94f1c] disabled:opacity-50 transition-colors shrink-0"
                >
                  {savingKey ? "保存中..." : "安全保存"}
                </button>
              </div>
            ) : (
              <div className="flex items-center justify-between pt-1" onClick={(e) => e.stopPropagation()}>
                <span className="text-[11px] text-[#716a61]">密钥已存入系统凭据库，UI 不可读取明文</span>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={handleTest}
                    disabled={testing}
                    className="inline-flex items-center gap-1 rounded-md border border-[rgba(73,61,47,0.18)] bg-white px-2.5 py-1 text-xs font-medium text-[#29251f] hover:bg-[#eee8df] transition-colors"
                  >
                    <RefreshCw className={`size-3 ${testing ? "animate-spin" : ""}`} />
                    <span>{testing ? "测试中..." : "测试连接"}</span>
                  </button>
                  <button
                    type="button"
                    onClick={onClearCredential}
                    className="rounded-md px-2 py-1 text-xs text-rose-600 hover:bg-rose-50 transition-colors"
                  >
                    移除凭据
                  </button>
                </div>
              </div>
            )}

            {testResult && (
              <div
                className={`mt-2 text-xs p-2 rounded-md ${
                  testResult.ok ? "bg-emerald-50 text-emerald-700" : "bg-rose-50 text-rose-700"
                }`}
              >
                {testResult.ok
                  ? `连接成功，延迟 ${testResult.latencyMs}ms`
                  : `连接失败: ${testResult.error || "未知网络错误"}`}
              </div>
            )}
          </div>
        </div>

        <div className="mt-5 pt-3 border-t border-[rgba(73,61,47,0.08)] flex items-center justify-between text-xs text-[#716a61]">
          <span>决策健康度: {healthLabel}</span>
          <span>{hasCredential ? "凭据已配置（明文不可读）" : "Jev 凭据未配置"}</span>
        </div>
      </div>
    </div>
  );
}
