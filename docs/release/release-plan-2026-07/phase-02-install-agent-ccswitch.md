# Phase 2：一键安装、Agent 检测与 cc-switch 上游同步实施规格

## 0. 阶段目标

Phase 2 要把“安装与连接”做成真实可用闭环：d-token 自有本地服务可安装/启动/停止，第三方 Agent 可检测，cc-switch 可只读读取，当前上游可同步，配置写入可预览、可备份、可验证、可回退。

本阶段的发布级验收不是“界面显示检测结果”，而是至少完成一个重点 Agent 的真实配置闭环，并能在首页、日志、Doctor、变更与回退中看到一致状态。

## 1. 多轮审查发现与修正

### 第一轮：边界审查

发现：原计划写了“第三方工具只检测”，但没有明确对每种工具的允许动作。

修正：新增动作权限矩阵：d-token 服务可安装管理；第三方工具只能检测、读取、生成计划、受控写入，不自动安装。

### 第二轮：cc-switch 审查

发现：原计划写了“cc-switch 自动同步”，但缺少同步状态机和不一致判断。

修正：新增 cc-switch profile/config/upstream 同步状态机、比较算法和 UI 状态。

### 第三轮：Agent 抽象审查

发现：每个 Agent 差异很大，必须先统一数据模型，否则实现会散。

修正：新增 `AgentDescriptor`、`AgentDetectionResult`、`AgentConfigPlan`、`VerificationResult` 模型。

### 第四轮：事务安全审查

发现：配置写入有风险，必须严格事务化。

修正：新增 InstallationTransaction 算法，强制 scan、plan、diff、backup、apply、verify、ledger、rollback。

## 2. 动作权限矩阵

| 对象 | 可检测 | 可读取配置 | 可生成计划 | 可写入 | 可安装 | 可启动/停止 |
|---|---:|---:|---:|---:|---:|---:|
| d-token 自有本地服务 | 是 | 是 | 是 | 是 | 是 | 是 |
| Claude Code | 是 | 是 | 是 | 需确认 | 否 | 否 |
| Codex | 是 | 是 | 是 | 需确认 | 否 | 否 |
| OpenCode | 是 | 是 | 是 | 需确认 | 否 | 否 |
| Cursor | 是 | 是 | 是 | 需确认 | 否 | 否 |
| Cline | 是 | 是 | 是 | 需确认 | 否 | 否 |
| CommandCode | 是 | 是 | 是 | 需确认 | 否 | 否 |
| cc-switch | 是 | 是 | 是 | 默认否，仅高级确认 | 否 | 默认否 |

说明：cc-switch 默认只读。只有用户在高级安装中明确要求并确认 diff 后，才允许进行受控写入；常规一键安装不得静默改写 cc-switch 配置。

## 3. 核心数据模型

### AgentDescriptor

```ts
type AgentDescriptor = {
  id: 'claude-code' | 'codex' | 'opencode' | 'cursor' | 'cline' | 'commandcode';
  displayName: string;
  executableHints: string[];
  configPathHints: string[];
  supportedConfigMethods: Array<'config-file' | 'env-var' | 'hook' | 'local-proxy' | 'manual-prompt'>;
  canAutoConfigure: boolean;
  requiresConfirmation: true;
};
```

### AgentDetectionResult

```ts
type AgentDetectionResult = {
  agentId: string;
  installed: boolean;
  executablePath?: string;
  version?: string;
  configFiles: Array<DetectedConfigFile>;
  currentUpstream?: UpstreamDescriptor;
  configSource: 'official' | 'cc-switch' | 'custom' | 'unknown' | 'not-configured';
  capability: 'one-click' | 'confirm-required' | 'prompt-only' | 'manual-only' | 'unsupported';
  risks: RiskItem[];
  nextAction: string;
};
```

### UpstreamDescriptor

```ts
type UpstreamDescriptor = {
  source: 'official' | 'cc-switch' | 'custom' | 'local-provider' | 'unknown';
  protocol: 'openai-compatible' | 'anthropic-compatible' | 'local' | 'unknown';
  baseUrl?: string;
  port?: number;
  profileName?: string;
  providerName?: string;
  redactedCredentialRef?: string;
  confidence: number;
};
```

### CcSwitchState

```ts
type CcSwitchState = {
  detected: boolean;
  configRoot?: string;
  configFiles: CcSwitchConfigFile[];
  activeProfile?: string;
  activeUpstream?: UpstreamDescriptor;
  processStatus: 'running' | 'not-running' | 'unknown';
  portStatus: 'open' | 'closed' | 'conflict' | 'unknown';
  lastSyncAt?: string;
  syncStatus: 'synced' | 'out-of-sync' | 'needs-confirmation' | 'unsupported' | 'unknown';
};
```

### AgentConfigPlan

```ts
type AgentConfigPlan = {
  planId: string;
  agentId: string;
  purpose: 'connect-agent-to-dtoken' | 'sync-upstream' | 'repair-config';
  currentPath: UpstreamDescriptor;
  targetPath: UpstreamDescriptor;
  filesToRead: string[];
  filesToModify: PlannedFileChange[];
  envChanges: PlannedEnvChange[];
  backupPlan: BackupPlan;
  verificationPlan: VerificationStep[];
  riskLevel: 'low' | 'medium' | 'high';
  requiresUserConfirmation: true;
};
```

## 4. cc-switch 同步状态机

```text
unknown
  ↓ scan
not-detected ───────────────→ manual-or-skip
  ↓ detected
profile-detected
  ↓ parse-upstream
upstream-detected
  ↓ compare-with-dtoken-active-upstream
synced / out-of-sync / needs-confirmation
  ↓ if out-of-sync
sync-plan-created
  ↓ user confirms or auto-safe-sync if no write to third-party config
synced
```

### 同步比较算法

1. 读取 cc-switch 当前 active profile/config。
2. 提取 active upstream：协议、base URL、端口、provider 名称、模型族、凭据引用。
3. 读取 d-token active upstream。
4. 规范化 URL：去除尾部斜杠，统一 localhost/127.0.0.1，解析端口。
5. 比较协议、host、port、path、provider/profile。
6. 如果完全一致：`synced`。
7. 如果 cc-switch 已切换而 d-token 未更新：`out-of-sync`。
8. 如果字段不足或协议未知：`needs-confirmation`。
9. 如果配置格式不支持：`unsupported`。

### UI 状态

| 状态 | 用户文案 | 主按钮 |
|---|---|---|
| synced | d-token 已跟随当前 cc-switch 上游 | 查看连接路径 |
| out-of-sync | d-token 当前上游与 cc-switch 不一致 | 同步上游 |
| needs-confirmation | 检测到 cc-switch 配置，但需要你确认上游 | 确认上游 |
| unsupported | 当前 cc-switch 配置格式暂不支持自动识别 | 生成修复提示词 |
| unknown | 还没有检测 cc-switch | 开始检测 |

## 5. 安装与配置事务算法

```text
scanEnvironment()
  -> detectDtokenService()
  -> detectPorts()
  -> detectCcSwitch()
  -> detectAgents()
  -> buildAgentConfigPlan()
  -> renderHumanSummary()
  -> renderDetailedDiff()
  -> createBackup()
  -> applyChange()
  -> verifyConfig()
  -> appendEventLedger()
  -> createRollbackPoint()
  -> refreshDashboardState()
```

强制规则：

1. 没有 diff，不允许 apply。
2. 没有 backup，不允许 apply。
3. 没有 verification result，不允许显示成功。
4. 没有 rollback point，不允许结束事务。
5. 密钥、token、cookie、认证头不得进入 summary、diff、日志、导出。

## 6. 重点 Agent 适配计划

| Agent | 第一目标 | 检测优先级 | 配置策略 | 验证策略 |
|---|---|---|---|---|
| OpenCode | 优先真实闭环 | 命令 + 配置文件 + 当前 provider | 指向 d-token 本地服务，再由 d-token 转上游/cc-switch | 轻量连接检查 + Event Ledger 入账 |
| Claude Code | 高优先级 | 命令 + 配置路径 + hook/env | Anthropic-compatible 路径接入 d-token | 配置读取 + 连接检查 |
| Codex | 高优先级 | 命令 + 配置路径 | OpenAI-compatible 路径接入 d-token | 配置读取 + 连接检查 |
| Cursor | 高优先级 | 工作区/配置检测 | 优先生成配置建议，能写则受控写入 | 配置状态检查 |
| Cline | 高优先级 | VSCode 扩展/配置检测 | 优先生成配置建议，能写则受控写入 | 配置状态检查 |
| CommandCode | 高优先级但资料待补 | 命令 + 配置路径待补 | 暂按 AgentDescriptor 接入 | 待补验证器 |

CommandCode 不再作为设计缺口处理：计划层已把它纳入统一 AgentDescriptor 模型。实现前只需要补具体适配器数据，不影响架构开工。

## 7. UI 模块拆解

一键安装页建议模块：

1. 本地服务卡片：启动、停止、状态、端口。
2. cc-switch 卡片：检测、active profile、active upstream、同步状态。
3. Agent 列表：重点 Agent 检测结果和能力分级。
4. 配置计划面板：摘要、连接路径图、风险、diff。
5. 验证结果面板：成功/失败、原因、Doctor 入口。
6. 事务日志：仅显示本次安装与连接过程。

高级安装页建议模块：

1. 自定义 d-token 端口。
2. 自定义 Agent 上游。
3. 自定义 cc-switch profile。
4. 协议转换设置。
5. fallback 策略。
6. 保存前 diff/backup。

## 8. 直接开工任务清单

1. 新建 agent descriptor registry。
2. 新建 cc-switch detector。
3. 新建 upstream normalization/comparison 模块。
4. 新建 sync status calculator。
5. 新建 install transaction state machine。
6. 新建 redaction helper，统一处理密钥脱敏。
7. 改造 Install 页面为“一键安装”标签。
8. 把 Advanced 中 route/provider 配置迁到“高级安装”标签。
9. 将 connection verification 结果写入 Dashboard/Doctor/Logs/Changes 共享状态。
10. 为 CommandCode 留出 descriptor 和 adapter 占位，不阻塞其他 Agent。

## 9. 最终审查结论

Phase 2 已补齐动作权限、数据模型、cc-switch 同步状态机、事务算法、Agent 适配矩阵和 UI 模块拆解。当前没有已知架构缺口，可以直接开始实现；CommandCode 需要补具体适配字段，但不影响整体框架开工。
