# Phase 5：UI 完整度、稳定性与发布验收实施规格

## 0. 阶段目标

Phase 5 是发布收口阶段。目标不是再新增大功能，而是确认 Phase 1-4 的所有能力都达到“可公开试用”的质量：UI 完整、文案友好、真实功能闭环、跨平台稳定、安全脱敏、备份回退可靠、文档可指导新用户。

本阶段产物应该包括：发布验收清单、测试矩阵、阻断条件、发布文档、已知限制和最终审查报告。

## 1. 多轮审查发现与修正

### 第一轮：UI 完整性审查

发现：原计划说“UI 完整”，但缺少逐页验收标准。

修正：新增页面完整度矩阵，要求每页都有标题、副标题、状态、主操作、空状态、错误状态、成功状态、数据来源。

### 第二轮：稳定性审查

发现：原计划说“稳定”，但缺少跨平台和真实链路测试矩阵。

修正：新增 Windows/macOS、Agent、cc-switch、智能学习、Doctor、回退测试矩阵。

### 第三轮：发布阻断审查

发现：没有定义什么情况不能发布。

修正：新增 release blocker 列表，任何一项出现都不得发布。

### 第四轮：文档审查

发现：发布文档需要指导新用户，而不是只写开发报告。

修正：新增 README、安装说明、安全隐私、已知限制、回退指南的内容要求。

## 2. 页面完整度矩阵

| 页面 | 必须有的完成项 |
|---|---|
| 总体概览 | 服务状态、Agent 摘要、cc-switch 同步摘要、估算节省、最近问题、下一步按钮、数据来源 |
| 安装与连接 | 四个上栏、一键安装流程、智能学习入口、高级安装、过程日志、配置计划、验证结果 |
| 分析 | 估算节省、趋势、Agent 分布、上游分布、错误分布、智能学习覆盖、数据不足提示 |
| Doctor | 问题列表、严重程度、影响说明、自动修复、确认修复、修复提示词、日志/回退入口 |
| 变更与回退 | 时间线、变更来源、影响对象、diff、备份、恢复、恢复后验证 |
| 设置 | 语言、主题、端口、日志脱敏、扫描强度、敏感文件规则、数据保留、开发模式 |

每页都必须覆盖：loading、empty、ready、warning、error、success。

## 3. 测试矩阵

### 平台矩阵

| 平台 | 必测内容 |
|---|---|
| Windows | 盘符路径、空格路径、UTF-8、`.cmd` shim、权限弹窗、端口占用、配置目录 |
| macOS | app bundle 权限、shell 环境、配置路径、端口占用、文件监听、签名/隔离提示 |

### Agent 矩阵

| Agent | 必测 |
|---|---|
| OpenCode | 检测、配置计划、连接 d-token、验证、日志、回退 |
| Claude Code | 检测、配置读取、连接计划、提示词/受控配置、验证 |
| Codex | 检测、配置读取、连接计划、委托智能学习、验证 |
| Cursor | 检测、配置建议、工作区关联、智能学习扫描 |
| Cline | 检测、配置建议、工作区关联、智能学习扫描 |
| CommandCode | descriptor 占位、检测器可扩展、配置路径补齐后进入完整验收 |

### cc-switch 矩阵

1. 未安装/未检测到。
2. 已安装但未运行。
3. 已运行但端口未知。
4. 有多个 config/profile。
5. 当前 profile 可识别。
6. 当前上游可识别。
7. profile 切换后 d-token 同步。
8. profile 切换后 d-token 不同步并触发 Doctor。
9. 配置含敏感字段且 UI/log/export 脱敏。

### 智能学习矩阵

1. 全面扫描启动。
2. 大目录不阻塞 UI。
3. 可停止/跳过/重新扫描。
4. 排序解释可见。
5. 敏感文件排除。
6. Claude/Codex 委托失败可降级提示词模式。
7. 直接写入只更新 d-token 管理块。
8. 非管理内容不变。
9. 写入生成回退点。
10. 回退恢复原文档。

### 安全矩阵

1. API Key 不显示原文。
2. token/cookie/auth header 不落盘。
3. 完整 prompt 不导出。
4. 完整 response 不导出。
5. `.env` 不进入学习结果。
6. 日志导出默认脱敏。
7. Debug 模式也不能导出真实密钥。
8. 配置 diff 不显示密钥原文。

## 4. Release Blockers

出现以下任一情况，不允许发布：

1. d-token 本地服务无法稳定启动。
2. 首页无法区分真实数据和示例数据。
3. 第三方工具被自动安装或静默修改。
4. cc-switch profile 切换后 d-token 仍可能使用旧上游但 UI 不提示。
5. 配置写入没有备份。
6. 配置写入没有回退点。
7. 智能学习会改动非 d-token 管理块内容。
8. 日志或导出泄露密钥、token、cookie、认证头、完整 prompt、完整 response。
9. Doctor 对高风险修复没有确认流程。
10. 任意页面出现无法解释的冷错误或空白页。
11. Windows 或 macOS 核心流程完全不可用且文档未说明。
12. README 不能指导新用户完成安装、连接、验证和回退。

## 5. 发布文档要求

### README 必须包含

1. d-token 是什么。
2. 第一版能做什么。
3. 当前重点支持 Agent。
4. cc-switch 只读同步策略。
5. 智能学习如何扫描和写入。
6. 安全与隐私边界。
7. 安装步骤。
8. 第一次连接步骤。
9. 常见问题。
10. 如何回退。
11. 已知限制。

### 用户说明必须包含

1. 什么是本地服务。
2. 什么是 Agent 连接。
3. 什么是 cc-switch 同步。
4. 什么是智能学习。
5. 为什么要备份和回退。
6. 日志会记录什么，不会记录什么。

## 6. 最终验收流程

```text
runFrontendBuild()
  -> runBackendTests()
  -> runDesktopSmokeCheck()
  -> runDtokenServiceLifecycleCheck()
  -> runAgentDetectionCheck()
  -> runCcSwitchReadOnlyAndSyncCheck()
  -> runSmartLearningScanAndWriteCheck()
  -> runDoctorRepairCheck()
  -> runRollbackCheck()
  -> runRedactionExportCheck()
  -> reviewDocsAndKnownLimitations()
  -> releaseCandidateApproval()
```

如果某个自动化脚本暂时不存在，也必须手工记录等价验收结果，不能用“UI 看起来正常”代替真实链路证明。

## 7. 直接开工任务清单

1. 新建 release readiness checklist。
2. 新建 release blocker checklist。
3. 增加前端 build gate。
4. 增加核心 Tauri command smoke gate。
5. 增加 cc-switch 同步手工/自动测试。
6. 增加智能学习写入不变性测试。
7. 增加日志导出脱敏测试。
8. 更新 README 和用户说明。
9. 生成已知限制文档。
10. 发布前人工过一遍全部主页面。

## 8. 最终审查结论

Phase 5 已补齐页面完整度、测试矩阵、发布阻断条件、文档要求和最终验收流程。当前没有已知发布计划缺口，可以作为发布收口阶段直接执行。
