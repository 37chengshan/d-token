# Phase 3：智能学习、全面扫描与安全文档写入实施规格

## 0. 阶段目标

Phase 3 要把“智能学习”做成 d-token 的核心差异化能力：全面发现本机 Agent 会话和项目工作区，快速准确排序，高价值目录深度理解，生成 Agent 可用上下文，并以受控追加/插入方式直接写入项目级或全局 Agent 文档。

用户已确认：必须全面扫描，算法要快速准确；可以直接写入，但只允许插入或增加内容，不能改动其他内容。

## 1. 多轮审查发现与修正

### 第一轮：扫描边界审查

发现：原计划写了“全面扫描”，但没有区分轻量索引和深度扫描，可能导致性能和隐私风险。

修正：新增两层扫描架构：轻量索引用于发现和排序，深度扫描只对高价值目录或用户重点选择目录执行。

### 第二轮：算法审查

发现：原计划写了“快速准确排序”，但缺少评分公式。

修正：新增学习价值评分公式、字段权重、置信度、排序解释。

### 第三轮：写入安全审查

发现：原计划写了“直接写入”，但需要避免覆盖用户文档。

修正：新增 d-token 管理块协议、插入/追加算法、冲突处理和回退策略。

### 第四轮：委托模式审查

发现：委托 Claude/Codex 有敏感信息风险。

修正：新增委托输入包结构、脱敏规则、过程状态和失败回退。

## 2. 扫描架构

智能学习采用“发现 → 轻量索引 → 评分排序 → 深度扫描 → 生成结果 → 写入/提示词”的流水线。

```text
discoverCandidates()
  -> buildLightIndex()
  -> scoreCandidates()
  -> showRankedFolders()
  -> userFocusSelection()
  -> deepScanSelected()
  -> generateLearningContext()
  -> writeManagedBlockOrGeneratePrompt()
  -> createRollbackPoint()
```

### 轻量索引只读取

1. 目录名。
2. 最近修改时间。
3. 文件数量和大小区间。
4. 是否存在 `.git`。
5. 是否存在常见项目标记文件。
6. 是否存在 Agent 文档。
7. 是否存在 Agent 会话关联记录。
8. 语言和框架指纹。

轻量索引不得读取完整源代码、完整会话和敏感文件。

### 深度扫描可读取

只对高价值目录或用户重点选择目录执行。读取范围包括：README、docs、package/script、Cargo、pyproject、配置摘要、项目结构、已有 Agent 文档、最近变更摘要。

深度扫描仍必须跳过 `.env`、密钥、认证文件、cookie、token、大型二进制、依赖目录、构建产物。

## 3. 候选来源

| 来源 | 发现方式 | 价值 |
|---|---|---|
| Git 仓库 | 常用项目目录 + `.git` | 判断真实项目 |
| Claude Code | 会话/项目路径记录 | 判断 Claude 使用痕迹 |
| Codex | 会话/项目路径记录 | 判断 Codex 使用痕迹 |
| OpenCode | 配置/历史/工作区记录 | 判断 OpenCode 使用痕迹 |
| Cursor | workspace / recent projects | 判断编辑器使用痕迹 |
| Cline | VSCode 扩展配置/会话 | 判断 Cline 使用痕迹 |
| CommandCode | descriptor 待补 | 纳入统一发现模型 |
| d-token | 本地操作记录 | 判断用户历史偏好 |

CommandCode 的目录来源通过 `AgentDescriptor.sessionPathHints` 后续补齐，不影响扫描框架开工。

## 4. 学习价值评分算法

每个候选目录计算 `LearningScore`。

```text
score =
  0.22 * recencyScore +
  0.18 * agentActivityScore +
  0.18 * projectConfidenceScore +
  0.14 * docGapScore +
  0.10 * complexityScore +
  0.08 * userPreferenceScore +
  0.06 * dtokenHistoryScore -
  0.10 * riskPenalty
```

### 字段定义

1. `recencyScore`：最近修改、最近打开、最近会话时间。
2. `agentActivityScore`：被 Claude/Codex/OpenCode/Cursor/Cline/CommandCode 访问的次数和新鲜度。
3. `projectConfidenceScore`：是否有 `.git`、src、README、package、Cargo、pyproject、docs 等项目结构。
4. `docGapScore`：缺少 `CLAUDE.md`、`AGENTS.md`、项目约定文档时更高。
5. `complexityScore`：多语言、多模块、脚本多、目录复杂时更高，但过大项目需要降低深扫优先级。
6. `userPreferenceScore`：用户曾经选择、收藏、忽略、回退的历史。
7. `dtokenHistoryScore`：d-token 最近路由、压缩、错误、学习记录。
8. `riskPenalty`：敏感文件过多、目录过大、权限不足、不可读文件过多。

### 排序解释

UI 必须显示排序原因，例如：

1. 最近 1 小时有 Codex 会话。
2. 包含 Tauri + React + Rust 项目结构。
3. 缺少 Agent 项目说明文档。
4. 最近 d-token 在该项目出现 3 次上下文过长。
5. 目录较大，已采用轻量索引，建议手动确认深度扫描。

## 5. 索引缓存设计

```ts
type LearningIndexRecord = {
  folderPath: string;
  folderHash: string;
  lastIndexedAt: string;
  lightIndex: LightIndex;
  score: LearningScore;
  explanations: string[];
  riskFlags: string[];
  deepScanStatus: 'not-started' | 'queued' | 'running' | 'completed' | 'failed' | 'skipped';
};
```

缓存规则：

1. 轻量索引可缓存。
2. 文件修改时间变化后增量刷新。
3. 用户手动重新扫描时强制刷新。
4. 深度扫描结果要记录来源和时间。
5. 缓存不得保存密钥、完整会话、完整 prompt、完整 response。

## 6. 学习结果结构

```ts
type LearningContext = {
  projectName: string;
  projectPath: string;
  summary: string;
  techStack: string[];
  architectureNotes: string[];
  importantFiles: string[];
  commonCommands: string[];
  conventions: string[];
  risks: string[];
  agentInstructions: string[];
  dtokenRecommendations: string[];
  openQuestions: string[];
  generatedAt: string;
  sourceSummary: SourceSummary;
};
```

输出必须面向 Agent 使用，不能只是普通项目介绍。它要告诉 Agent 如何工作、不要碰什么、怎么运行项目、怎么理解目录、如何配合 d-token。

## 7. 文档写入协议

### 管理块格式

```md
<!-- d-token:start smart-learning id="project-context" updated="2026-07-08" -->
## d-token 智能学习上下文

...内容...
<!-- d-token:end smart-learning -->
```

### 写入算法

```text
readTargetDocument()
  -> createBackup()
  -> findExistingManagedBlock()
  -> if exists: replaceOnlyManagedBlock()
  -> if not exists: appendManagedBlockAtEnd()
  -> verifyNonManagedContentUnchanged()
  -> writeFile()
  -> createRollbackPoint()
  -> appendChangeLog()
```

强制规则：

1. 如果非管理块内容变化，写入失败。
2. 如果无法备份，写入失败。
3. 如果文件编码无法安全识别，写入失败并生成提示词模式。
4. 如果目标文件过大，优先写入 `.d-token/learned-context.md`。
5. 如果用户手动编辑过 d-token 管理块，更新前保留备份。

## 8. 写入目标矩阵

| 目标 | 策略 | 风险 |
|---|---|---|
| 项目级 `CLAUDE.md` | 追加/更新 d-token 管理块 | 中 |
| 项目级 `AGENTS.md` | 追加/更新 d-token 管理块 | 中 |
| 项目级 `GEMINI.md` | 追加/更新 d-token 管理块 | 中 |
| `.d-token/learned-context.md` | 可创建/更新整份 d-token 自有文件 | 低 |
| 全局 Claude/Codex/Agent 文档 | 追加/更新 d-token 管理块 | 高，必须确认或有设置授权 |
| CommandCode 文档 | 通过 descriptor 补路径后同样处理 | 待补 |

## 9. 委托 Claude/Codex 流程

委托模式必须构造受控输入包：

```ts
type DelegationPackage = {
  task: 'generate-project-agent-context';
  allowedScope: string[];
  excludedScope: string[];
  projectSummary: LightIndex;
  selectedFilesSummary: SourceSummary;
  outputSchema: 'LearningContext';
  safetyRules: string[];
};
```

流程：

1. 构造脱敏输入包。
2. 显示准备委托。
3. 调用 Claude/Codex。
4. 校验返回结构。
5. 生成写入计划。
6. 只写入管理块。
7. 创建回退点。

失败处理：

1. 委托失败：保留提示词模式。
2. 结构不合法：展示错误并允许重试。
3. 写入失败：不改文件，保留生成内容和修复建议。

## 10. UI 模块拆解

智能学习页包括：

1. 扫描状态卡片：全面扫描进度、目录数、耗时、停止按钮。
2. 候选目录列表：分数、排序原因、Agent 关联、风险标签。
3. 搜索和过滤：Agent、项目类型、最近活跃、风险等级。
4. 深度扫描面板：已选目录、扫描阶段、文件摘要。
5. 输出预览：项目理解、命令、风险、Agent 指令。
6. 执行方式：直接写入 / Claude 委托 / Codex 委托 / 生成提示词。
7. 写入结果：目标文档、管理块、备份、回退入口。

## 11. 直接开工任务清单

1. 新建 learning candidate discovery 模块。
2. 新建 light indexer。
3. 新建 scoring engine。
4. 新建 risk filter 和 sensitive path rules。
5. 新建 deep scanner。
6. 新建 LearningContext schema。
7. 新建 managed block writer。
8. 新建 non-managed-content invariant checker。
9. 新建 rollback integration。
10. 新建 delegation package builder。
11. 改造安装与连接页中的智能学习标签。

## 12. 最终审查结论

Phase 3 已补齐扫描架构、评分算法、索引缓存、学习结果结构、文档写入协议、委托流程和 UI 模块拆解。当前没有已知算法或设计缺口，可以直接开始实现；CommandCode 仅需补具体路径提示，不影响智能学习框架。
