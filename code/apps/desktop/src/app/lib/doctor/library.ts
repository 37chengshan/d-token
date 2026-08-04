// d-token Doctor 问题库初版。
// 关联计划：release-plan-2026-07/phase-04 §4 Doctor 问题库初版。
import type { IssueDefinition } from "./registry";

export const ISSUE_LIBRARY: IssueDefinition[] = [
  {
    id: "service-not-running", title: "d-token 本地服务未启动", severity: "critical", category: "service",
    detect: (c) => !c.serviceRunning,
    impact: "Agent 请求无法经过 d-token，所有节省、路由与可观测能力不可用。",
    likelyCauses: ["服务从未启动", "服务异常退出", "端口冲突导致启动失败"],
    repairActions: [
      { id: "start-service", label: "启动本地服务", riskLevel: "low", requiresConfirmation: false, preflightChecks: ["端口未被占用"], execute: "start_dtoken_service", verify: "健康检查通过" },
    ],
  },
  {
    id: "port-conflict", title: "d-token 端口被占用", severity: "critical", category: "port",
    detect: (c) => c.portConflict,
    impact: "服务无法在预期端口启动。",
    likelyCauses: ["其他进程占用端口", "残留 d-token 进程"],
    repairActions: [
      { id: "suggest-port", label: "建议更换端口", riskLevel: "low", requiresConfirmation: false, preflightChecks: ["确认占用进程身份"], execute: "suggest_alternative_port", verify: "新端口可用", rollback: "恢复原端口配置" },
    ],
  },
  {
    id: "agent-not-pointing", title: "Agent 未指向 d-token", severity: "critical", category: "agent",
    detect: (c) => c.agentsDetected > 0 && c.agentsPointingToDtoken === 0,
    impact: "Agent 请求绕过 d-token，无法获得压缩与可观测。",
    likelyCauses: ["Agent 上游仍指向官方/第三方", "配置未写入", "写入后未验证"],
    repairActions: [
      { id: "gen-config-plan", label: "生成配置计划并预览 diff", riskLevel: "medium", requiresConfirmation: true, preflightChecks: ["已备份 Agent 配置"], execute: "build_agent_config_plan", verify: "连接检查通过", rollback: "恢复 Agent 配置备份" },
    ],
    promptForAgent: "请将本机 AI Agent 的上游指向 d-token 本地服务（127.0.0.1:18001），并在写入前预览 diff 与备份。",
  },
  {
    id: "ccswitch-out-of-sync", title: "cc-switch 上游不同步", severity: "critical", category: "cc-switch",
    detect: (c) => c.ccSwitch.syncStatus === "out-of-sync",
    impact: "d-token 仍使用旧上游，用户切换 cc-switch 后未生效。",
    likelyCauses: ["cc-switch profile 已切换", "d-token active upstream 未更新"],
    repairActions: [
      { id: "sync-upstream", label: "同步 cc-switch 上游（只改 d-token）", riskLevel: "low", requiresConfirmation: false, preflightChecks: ["上游可达"], execute: "sync_dtoken_upstream_from_ccswitch", verify: "上游一致", rollback: "恢复 d-token active upstream" },
      { id: "write-ccswitch", label: "受控写入 cc-switch 配置", riskLevel: "high", requiresConfirmation: true, preflightChecks: ["已备份 cc-switch 配置"], execute: "apply_ccswitch_config_plan", verify: "上游一致", rollback: "恢复 cc-switch 配置备份" },
    ],
  },
  {
    id: "upstream-unreachable", title: "上游不可达", severity: "critical", category: "upstream",
    detect: (c) => c.serviceRunning && !c.upstreamReachable,
    impact: "请求无法转发到上游，Agent 会收到错误。",
    likelyCauses: ["上游地址错误", "网络问题", "凭据失效"],
    repairActions: [],
    promptForAgent: "请检查 d-token 上游地址、网络与凭据引用是否正确。",
  },
  {
    id: "learning-write-failed", title: "智能学习写入失败", severity: "warning", category: "learning",
    detect: (c) => c.learningWriteFailed,
    impact: "学习结果未写入，但已保留备份与提示词。",
    likelyCauses: ["非管理块内容发生变化", "无法备份", "编码不可识别"],
    repairActions: [],
    rollbackHint: "原文档未改动，可重新尝试或使用提示词模式。",
  },
  {
    id: "rollback-unavailable", title: "回退点不可用", severity: "critical", category: "rollback",
    detect: (c) => !c.rollbackAvailable,
    impact: "无法恢复已应用的变更。",
    likelyCauses: ["回退点损坏", "回退点未创建"],
    repairActions: [],
    rollbackHint: "必须提示手动恢复，使用最近备份。",
  },
  {
    id: "secret-in-logs", title: "检测到敏感字段可能进入日志", severity: "critical", category: "security",
    detect: (c) => c.secretInLogs,
    impact: "日志或导出可能泄露密钥、token、cookie、认证头。",
    likelyCauses: ["脱敏策略未启用", "新字段未纳入禁止表"],
    repairActions: [
      { id: "strict-redaction", label: "立即启用严格脱敏", riskLevel: "low", requiresConfirmation: false, preflightChecks: [], execute: "enable_strict_redaction", verify: "导出审计无敏感字段" },
    ],
  },
];
