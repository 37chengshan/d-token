// d-token 共享运行状态 AppOperationalState + 跨页面一致性规则。
// 关联计划：release-plan-2026-07/phase-04 §7 共享运行状态模型、§8 跨页面一致性规则。
import type { AgentDetectionResult, UpstreamDescriptor } from "../agents/registry";
import type { CcSwitchState } from "../ccswitch/types";
import type { AnalyticsSnapshot } from "../analytics/snapshot";
import type { IssueInstance } from "../doctor/registry";
import type { LogEvent } from "../logs/event";
import type { RollbackPoint } from "../rollback/point";

export type AppDataSource = "real" | "preview" | "empty" | "error";

export type ServiceState = {
  running: boolean;
  managed: boolean;
  port: number;
  bindHost: string;
  version?: string;
};

export type AppOperationalState = {
  service: ServiceState;
  agents: AgentDetectionResult[];
  ccSwitch: CcSwitchState;
  activeUpstream?: UpstreamDescriptor;
  analytics: AnalyticsSnapshot;
  openIssues: IssueInstance[];
  recentLogs: LogEvent[];
  recentChanges: RollbackPoint[];
  dataSource: AppDataSource;
};

/** 跨页面一致性规则校验，返回违规列表（空表示一致）。 */
export function checkConsistency(state: AppOperationalState): string[] {
  const violations: string[] = [];
  // 规则1：cc-switch out-of-sync 时，首页/安装页/Doctor 都必须显示风险
  if (state.ccSwitch.syncStatus === "out-of-sync") {
    const hasCcIssue = state.openIssues.some((i) => i.category === "cc-switch");
    if (!hasCcIssue) violations.push("cc-switch out-of-sync 未在 Doctor 中显示为问题");
  }
  // 规则2：Agent 配置验证失败，分析页不得把该 Agent 显示为正常贡献来源
  const failedAgents = state.agents.filter((a) => a.capability === "unsupported" && a.installed === false);
  for (const agent of failedAgents) {
    const inBreakdown = state.analytics.agentBreakdown.some((b) => b.label.includes(agent.agentId));
    if (inBreakdown) violations.push(`未验证 Agent ${agent.agentId} 不应出现在分析贡献来源`);
  }
  // 规则3：日志写入失败，Doctor 必须出现日志问题，导出应禁用/提示
  const logFailed = state.recentLogs.some((l) => l.category === "security" && l.level === "error");
  if (logFailed) {
    const hasLogIssue = state.openIssues.some((i) => i.category === "logs" || i.category === "security");
    if (!hasLogIssue) violations.push("日志写入失败未在 Doctor 中显示");
  }
  // 规则4：智能学习写入文档，变更与回退必须出现对应回退点
  const hasLearningLog = state.recentLogs.some((l) => l.category === "learning" && l.level === "info");
  if (hasLearningLog) {
    const hasLearningRollback = state.recentChanges.some((c) => c.source === "smart-learning");
    if (!hasLearningRollback) violations.push("智能学习写入未生成回退点");
  }
  // 规则5：preview 数据源，分析页与首页必须标注示例数据
  if (state.dataSource === "preview" && state.analytics.source !== "preview") {
    violations.push("preview 数据源未在分析页标注示例数据");
  }
  return violations;
}

/** 由运行状态派生首页摘要：服务、Agent、cc-switch、估算节省、最近问题、下一步。 */
export function deriveOverviewSummary(state: AppOperationalState) {
  const criticalIssues = state.openIssues.filter((i) => i.severity === "critical");
  const nextAction = !state.service.running
    ? "启动本地服务"
    : state.ccSwitch.syncStatus === "out-of-sync"
      ? "同步 cc-switch 上游"
      : criticalIssues.length > 0
        ? "打开 Doctor 修复关键问题"
        : state.agents.filter((a) => a.installed).length === 0
          ? "前往安装与连接检测 Agent"
          : "一切正常，可继续使用";
  return {
    serviceRunning: state.service.running,
    connectedAgents: state.agents.filter((a) => a.installed).length,
    ccSwitchSynced: state.ccSwitch.syncStatus === "synced",
    estimatedSavedToday: state.analytics.estimatedSavedTokens,
    recentErrors: state.analytics.errorCount,
    criticalIssues: criticalIssues.length,
    nextAction,
    dataSource: state.dataSource,
  };
}
