# d-token 发布就绪检查清单（Release Readiness Checklist）

状态：Phase 5 发布收口
真理源：`docs/PRD.md`
计划源：`docs/release-plan-2026-07/phase-05-ui-stability-release.md`

## 1. 前端构建

- [x] `npx vite build` 通过，页面级 chunk 正常生成。
- [x] 侧边栏只显示 5 个主页面 + 设置。
- [x] 安装与连接页内含 4 个上栏（一键安装 / 智能学习 / 高级安装 / 日志）。
- [x] 首页含服务状态、Agent 摘要、cc-switch 同步摘要、估算节省、最近问题、下一步操作、数据来源标识。

## 2. 页面完整度矩阵

| 页面 | loading | empty | ready | warning | error | success | 数据来源 |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| 总体概览 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 安装与连接 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 分析 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Doctor | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 变更与回退 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 设置 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

## 3. 核心功能闭环

- [x] d-token 自有本地服务可启动、停止、检测（Tauri command）。
- [x] 第三方工具只检测，不自动安装（AgentDescriptor 动作权限矩阵）。
- [x] cc-switch 默认只读读取（CcSwitchState + detector）。
- [x] cc-switch profile 切换后上游可同步或提示 out-of-sync（computeSyncStatus）。
- [x] 安装与配置事务化（InstallationTransaction：scan→plan→diff→backup→apply→verify→ledger→rollback-point）。
- [x] 智能学习全面扫描 + 排序 + 受控写入（LearningIndexRecord + scoreCandidate + managed-block writer）。
- [x] 智能学习只新增/更新 d-token 管理块（verifyNonManagedInvariant）。
- [x] Doctor 问题库 + 修复动作（IssueDefinition + RepairAction + ISSUE_LIBRARY）。
- [x] 日志与导出默认脱敏（RedactionPolicy + exportLogs + auditExportForSecrets）。
- [x] 回退点覆盖配置、文档、Doctor 修复（RollbackPoint + createLearningRollbackPoint）。
- [x] 跨页面状态一致性（AppOperationalState + checkConsistency）。

## 4. 安全矩阵

- [x] API Key 不显示原文（redactValue + FORBIDDEN_FIELDS）。
- [x] token/cookie/auth header 不落盘（FORBIDDEN_LOG_FIELDS）。
- [x] 完整 prompt/response 不导出（auditExportForSecrets）。
- [x] .env 不进入学习结果（SENSITIVE_PATTERNS + isSensitivePath）。
- [x] 日志导出默认脱敏（exportLogs level=high）。
- [x] Debug 模式也不能导出真实密钥（redactValue high 永不保留原文）。
- [x] 配置 diff 不显示密钥原文（buildHumanSummary 脱敏）。

## 5. 测试矩阵

### 平台

| 平台 | 必测内容 | 状态 |
|---|---|---|
| Windows | 盘符路径、UTF-8、.cmd shim、端口占用 | 代码就绪，需本地真实验收 |
| macOS | app bundle 权限、shell 环境、配置路径 | 代码就绪，需本地真实验收 |

### Agent

| Agent | 检测 | 配置计划 | 验证 | 状态 |
|---|:---:|:---:|:---:|---|
| OpenCode | ✅ | ✅ | ✅ | 优先真实闭环 |
| Claude Code | ✅ | ✅ | ✅ | 高优先级 |
| Codex | ✅ | ✅ | ✅ | 高优先级 |
| Cursor | ✅ | ✅ | ✅ | 高优先级 |
| Cline | ✅ | ✅ | ✅ | 高优先级 |
| CommandCode | ✅ | 占位 | 待补 | 资料待补 |

### cc-switch

| 场景 | 状态 |
|---|---|
| 未检测到 | ✅ emptyCcSwitchState |
| 已运行 + profile 可识别 | ✅ parseCcSwitchConfig |
| profile 切换后同步 | ✅ computeSyncStatus |
| profile 切换后不同步 | ✅ out-of-sync + Doctor |

### 智能学习

| 场景 | 状态 |
|---|---|
| 全面扫描启动 | ✅ |
| 大目录不阻塞 UI | ✅ 轻量索引分层 |
| 排序解释可见 | ✅ explainScore |
| 敏感文件排除 | ✅ SENSITIVE_PATTERNS |
| 直接写入只更新管理块 | ✅ verifyNonManagedInvariant |
| 写入生成回退点 | ✅ createLearningRollbackPoint |

## 6. 最终验收流程

```text
runFrontendBuild() ✅
  -> runBackendTests() （需本地 Rust）
  -> runDesktopSmokeCheck() （需本地 Tauri）
  -> runDtokenServiceLifecycleCheck() （需本地 Tauri）
  -> runAgentDetectionCheck() （需本地 Tauri）
  -> runCcSwitchReadOnlyAndSyncCheck() （需本地真实 cc-switch）
  -> runSmartLearningScanAndWriteCheck() （需本地真实项目）
  -> runDoctorRepairCheck() （需本地 Tauri）
  -> runRollbackCheck() （需本地 Tauri）
  -> runRedactionExportCheck() ✅（lib 层单测可验证）
  -> reviewDocsAndKnownLimitations() ✅
  -> releaseCandidateApproval()
```

标注 ✅ 的项已在仓库内完成；其余项需要本地真实环境验收，不能用"UI 看起来正常"代替。
