# Phase 4：分析、Doctor、日志与变更回退实施规格

## 0. 阶段目标

Phase 4 要把 d-token 的真实行为变成用户可理解、可追踪、可修复、可恢复的系统。完成后，用户不仅知道 d-token 是否工作，还能知道它为什么失败、如何修复、改了什么、能否回退、节省是否可信。

本阶段核心是四个统一：统一分析数据模型、统一 Doctor 问题模型、统一日志脱敏模型、统一回退点模型。

## 1. 多轮审查发现与修正

### 第一轮：分析页审查

发现：原计划把指标列出来了，但没有定义数据来源和估算口径。

修正：新增 AnalyticsSnapshot 数据结构和“估算节省”计算原则，明确所有统计来自 d-token 自有记录。

### 第二轮：Doctor 审查

发现：Doctor 如果只是 pass/fail，用户不知道怎么修。

修正：新增 IssueDefinition 和 RepairAction 模型，按“问题、影响、原因、修复、提示词、回退”组织。

### 第三轮：日志审查

发现：日志和导出最容易泄露敏感信息。

修正：新增 RedactionPolicy、LogEvent 模型和禁止落盘字段。

### 第四轮：回退审查

发现：回退不能只覆盖配置，还必须覆盖智能学习写入。

修正：新增 RollbackPoint 模型，统一配置回退、文档回退、Doctor 修复回退。

### 第五轮：跨页面一致性审查

发现：如果各页面各自判断状态，会互相矛盾。

修正：新增 AppOperationalState，作为首页、安装页、分析页、Doctor、日志、回退的共享状态来源。

## 2. 分析数据模型

```ts
type AnalyticsSnapshot = {
  generatedAt: string;
  source: 'dtoken-local-service' | 'tauri-command' | 'empty' | 'preview';
  requestCount: number;
  estimatedInputTokens: number;
  estimatedOutputTokens: number;
  estimatedSavedTokens: number;
  estimatedSavingsRate: number;
  compressionCount: number;
  routeDecisionCount: number;
  errorCount: number;
  agentBreakdown: BreakdownItem[];
  upstreamBreakdown: BreakdownItem[];
  ccSwitchBreakdown: BreakdownItem[];
  projectBreakdown: BreakdownItem[];
  trend7d: TrendPoint[];
  trend30d: TrendPoint[];
  learningCoverage: LearningCoverage;
  warnings: string[];
};
```

### 估算节省计算原则

1. 所有节省数据必须显示“估算”。
2. 估算来源包括压缩前 token 估算、压缩后 token 估算、路由记录和本地请求记录。
3. 如果某条请求缺少 token 字段，只参与请求数，不参与节省率。
4. 如果数据样本不足，显示“数据不足，暂不计算趋势”。
5. 不承诺等于真实账单。

## 3. Doctor 问题模型

```ts
type IssueDefinition = {
  id: string;
  title: string;
  severity: 'info' | 'warning' | 'critical';
  category: 'service' | 'port' | 'agent' | 'cc-switch' | 'upstream' | 'learning' | 'logs' | 'rollback' | 'security';
  detect: DiagnosticCheck;
  impact: string;
  likelyCauses: string[];
  repairActions: RepairAction[];
  promptForAgent?: PromptTemplate;
  relatedLogsQuery?: string;
  rollbackHint?: string;
};
```

```ts
type RepairAction = {
  id: string;
  label: string;
  riskLevel: 'low' | 'medium' | 'high';
  requiresConfirmation: boolean;
  preflightChecks: string[];
  planPreview?: ChangePlan;
  execute: string;
  verify: string;
  rollback?: string;
};
```

## 4. Doctor 问题库初版

| 问题 | 严重度 | 自动修复 | 说明 |
|---|---|---:|---|
| d-token 本地服务未启动 | critical | 是 | 可启动服务 |
| d-token 端口被占用 | critical | 部分 | 可建议换端口，不能杀进程除非确认 |
| Agent 未检测到 | warning | 否 | 提供安装指引，不自动安装 |
| Agent 未指向 d-token | critical | 需确认 | 生成配置计划和 diff |
| cc-switch 未检测到 | info/warning | 否 | 可跳过或手动配置 |
| cc-switch 未运行 | warning | 默认否 | 只检测，不默认启动第三方 |
| cc-switch 上游不同步 | critical | 是/需确认 | 若只改 d-token active upstream，可自动；若写第三方配置需确认 |
| 上游不可达 | critical | 否 | 提供修复提示词 |
| 协议不匹配 | critical | 需确认 | 需要修改 route/provider |
| 智能学习扫描卡住 | warning | 是 | 可停止、清理任务、重建索引 |
| 智能学习写入失败 | warning | 否 | 保留备份和提示词 |
| 日志写入失败 | warning | 是 | 可重建日志目录或降低日志级别 |
| 回退点不可用 | critical | 否 | 必须提示手动恢复 |
| 检测到敏感字段可能进入日志 | critical | 是 | 立即启用严格脱敏 |

## 5. 日志模型与脱敏策略

### LogEvent

```ts
type LogEvent = {
  id: string;
  timestamp: string;
  level: 'debug' | 'info' | 'warning' | 'error';
  category: 'service' | 'agent' | 'cc-switch' | 'route' | 'learning' | 'doctor' | 'change' | 'security';
  summary: string;
  details: Record<string, RedactedValue>;
  correlationId?: string;
  relatedAgent?: string;
  relatedProject?: string;
  relatedRollbackPoint?: string;
};
```

### 禁止落盘字段

1. 完整 API Key。
2. token、cookie、认证头。
3. 完整 prompt。
4. 完整 response。
5. 完整源代码。
6. 私密会话正文。
7. `.env` 内容。
8. SSH/GPG/云服务凭据。
9. 用户系统认证文件。

### 脱敏策略

```text
redact(value)
  -> detectSecretPattern()
  -> preserveTypeAndLast4IfSafe()
  -> replaceWith[REDACTED]
  -> recordRedactionCountOnly()
```

导出必须默认脱敏。Debug 模式也不得导出完整密钥和认证字段。

## 6. 回退点模型

```ts
type RollbackPoint = {
  id: string;
  createdAt: string;
  source: 'install' | 'advanced-install' | 'smart-learning' | 'doctor-repair' | 'settings';
  affectedTargets: AffectedTarget[];
  humanSummary: string;
  diffSummary: string;
  backupRefs: BackupRef[];
  verificationBefore?: VerificationResult;
  verificationAfter?: VerificationResult;
  restorePlan: RestorePlan;
  restoreStatus: 'available' | 'restored' | 'failed' | 'partial' | 'unavailable';
};
```

回退目标必须覆盖：

1. Agent 配置文件。
2. d-token active config。
3. provider/route 配置。
4. cc-switch 相关受控写入。
5. 项目级 Agent 文档。
6. 全局 Agent 文档。
7. `.d-token/learned-context.md`。
8. Doctor 自动修复产生的文件修改。

## 7. 共享运行状态模型

```ts
type AppOperationalState = {
  service: ServiceState;
  agents: AgentDetectionResult[];
  ccSwitch: CcSwitchState;
  activeUpstream?: UpstreamDescriptor;
  analytics: AnalyticsSnapshot;
  openIssues: IssueInstance[];
  recentLogs: LogEvent[];
  recentChanges: RollbackPoint[];
  dataSource: 'real' | 'preview' | 'empty' | 'error';
};
```

所有主页面必须从同一状态源派生，避免首页说“已同步”、Doctor 说“未同步”、日志说“未知”。

## 8. 跨页面一致性规则

1. 如果 `ccSwitch.syncStatus = out-of-sync`，首页、安装页和 Doctor 都必须显示风险。
2. 如果 Agent 配置验证失败，分析页不得把该 Agent 显示为正常贡献来源。
3. 如果日志写入失败，Doctor 必须出现日志问题，导出按钮必须禁用或提示风险。
4. 如果智能学习写入文档，变更与回退必须出现对应回退点。
5. 如果数据来源是 preview，分析页和首页必须标注示例数据。

## 9. UI 模块拆解

### 分析页

1. 估算节省总览。
2. 请求趋势。
3. Agent 分布。
4. 上游/cc-switch 分布。
5. 错误分布。
6. 高消耗项目。
7. 智能学习覆盖。
8. 数据来源说明。

### Doctor

1. 总体健康状态。
2. 严重问题。
3. 可自动修复问题。
4. 需确认修复问题。
5. 可复制给 Agent 的修复提示词。
6. 相关日志和回退入口。

### 变更与回退

1. 时间线。
2. 来源筛选。
3. 影响对象。
4. diff 摘要。
5. 备份引用。
6. 恢复按钮。
7. 恢复后验证。

## 10. 直接开工任务清单

1. 新建 AnalyticsSnapshot 聚合层。
2. 新建 IssueDefinition registry。
3. 新建 RepairAction executor。
4. 新建 RedactionPolicy 和 redact helper。
5. 新建 LogEvent schema 和导出器。
6. 新建 RollbackPoint schema。
7. 扩展 rollback manager 支持智能学习文档。
8. 新建 AppOperationalState 聚合 command/hook。
9. 改造 Doctor UI 为问题修复中心。
10. 改造 Changes UI 为多目标回退时间线。
11. 增加跨页面一致性测试。

## 11. 最终审查结论

Phase 4 已补齐分析数据结构、Doctor 问题库、修复动作模型、日志脱敏模型、回退点模型和跨页面一致性规则。当前没有已知设计缺口，可以直接进入实现。
