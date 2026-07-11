# d-token 发布实施 — 实施轮多维度审查报告（第 1 轮）

审查对象：`docs/release-plan-2026-07/` Phase 1-5 的实际实现
审查日期：2026-07-08
审查方法：7 维度（PRD 一致性 / 可实现性 / 用户体验 / 安全性 / 回退性 / 稳定性 / 验收性）

## 0. 审查范围

本轮审查覆盖本轮实际产出的全部代码与文档变更：
- Phase 1：导航收敛、安装与连接工作台 4 上栏、智能学习 UI、首页 cc-switch 摘要、统一状态组件、i18n 文案。
- Phase 2：AgentDescriptor 注册表、cc-switch 类型/检测/上游比较、安装事务状态机、脱敏核心。
- Phase 3：候选发现、轻量索引、评分引擎、深度扫描、管理块写入器、委托流程。
- Phase 4：AnalyticsSnapshot、Doctor 问题库、日志事件/脱敏导出、回退点模型、共享运行状态。
- Phase 5：发布就绪检查清单、Release Blockers、README。

## 1. PRD 一致性

### 通过

- 页面结构对齐 PRD P0.5：5 主页面 + 设置，旧工程页降级为工作台内部上栏。
- 第三方工具只检测不安装，对齐 PRD P0.2 第 6 条 + Phase 2 动作权限矩阵。
- cc-switch 默认只读 + 上游同步，对齐 README 规划边界。
- 智能学习受控写入管理块 + 非管理内容不变性，对齐 Phase 3 §7 强制规则。
- 日志脱敏 + 禁止落盘字段，对齐 PRD §15 安全原则。
- 所有写入有备份和回退点，对齐 PRD P0.3 第二优先级。

### 待补

- CommandCode 的具体命令名、配置路径仍未填入 AgentDescriptor（计划层已知，不阻塞）。
- 真实 cc-switch 配置文件格式需在本地环境验证 parseCcSwitchConfig 的字段假设。

## 2. 可实现性

### 通过

- 所有 Phase 2-4 模块为纯 TypeScript 函数，无外部依赖，可在浏览器和 Tauri 两种环境运行。
- 评分公式、同步比较算法、事务状态机、管理块写入算法均有明确输入输出和边界条件。
- SmartLearning UI 在浏览器预览模式下用示例候选演示排序与结构，Tauri 模式下由 native 层驱动真实扫描。
- `vite build` 通过，页面级 chunk 正常生成（InstallWorkbench 133.91 kB）。

### 待补

- 真实文件系统枚举（候选发现、轻量索引）需要 Tauri native command 桥接，当前为纯逻辑层。
- managed-block writer 的 `planManagedWrite` 是纯函数，真实 IO（读写文件）需调用层实现。

## 3. 用户体验

### 通过

- 首页 10 秒内可判断 d-token 是否工作：服务状态卡片 + cc-switch 摘要 + 估算节省 + 最近问题。
- 安装与连接页 4 个上栏清晰，每个上栏有标题、副标题、状态、主操作。
- 智能学习有安全提示横幅、空状态、扫描进度、排序原因、写入结果。
- 主路径文案已从工程语言转为用户任务语言（"本地服务"/"连接路径"/"模型服务商"）。
- EmptyState 统一组件覆盖 loading/empty/warning/error/success/confirming。

### 待补

- 智能学习写入结果目前是演示态，真实写入成功/失败反馈需接 native 层。
- 首页 cc-switch 摘要当前在浏览器预览下显示"未检测"，真实状态需接 detector。

## 4. 安全性

### 通过

- redaction.ts 覆盖字段名禁止表 + 值指纹检测 + 递归脱敏 + 导出审计。
- 日志禁止落盘字段明确（API Key、token、cookie、认证头、完整 prompt/response、.env、凭据）。
- 智能学习 SENSITIVE_PATTERNS + SKIP_DIRS 排除敏感文件和依赖目录。
- 安装事务 buildHumanSummary 脱敏，密钥不进入摘要/diff/日志。
- auditExportForSecrets 可校验导出内容是否含未脱敏敏感字段。

### 待补

- 真实 Agent 配置文件读取时 hasCredentials 标记需要 native 层实现，当前为模型层占位。

## 5. 回退性

### 通过

- RollbackPoint 统一覆盖配置、文档、Doctor 修复，source 字段区分来源。
- InstallationTransaction 强制规则：无 diff 不 apply、无 backup 不 apply、无 verification 不成功、无 rollback point 不结束。
- createLearningRollbackPoint 专门覆盖智能学习文档写入。
- managed-block writer 的 verifyNonManagedInvariant 确保非管理块内容不变。

### 待补

- 真实回退执行（从备份恢复文件）需 native 层实现，当前为模型层。

## 6. 稳定性

### 通过

- 所有新模块为纯函数，无副作用，无竞态风险。
- useRuntimeDashboard 已有轮询 + mounted guard，避免内存泄漏。
- SmartLearning 扫描用 setTimeout 模拟异步，不阻塞 UI。
- 轻量索引 + 深度扫描分层，大目录不会卡死 UI。

### 待补

- 大项目真实扫描的性能阈值（文件数上限、超时）需在本地环境调参。
- Windows 盘符路径、UTF-8 编码、.cmd shim 需在本地环境验证。

## 7. 验收性

### 通过

- 前端 build gate 通过。
- release-readiness-checklist.md 逐项列出通过/待验收状态。
- release-blockers.md 逐项标注解决状态。
- README 覆盖安装、连接、验证、回退、常见问题、已知限制。

### 待补

- Rust 后端测试、Tauri 桌面窗口验收、真实 Agent 闭环需本地环境执行。
- 日志导出脱敏的端到端验证需 native 层导出 + 审计。

## 8. 第 1 轮发现的问题与修正

| # | 维度 | 问题 | 严重度 | 修正 |
|---|---|---|---|---|
| 1 | 可实现性 | SmartLearning 写入结果为演示态，未接真实 managed-block writer | 中 | 已在 lib 层实现 planManagedWrite，UI 层待接 native |
| 2 | 用户体验 | 首页 cc-switch 摘要在预览下永远显示"未检测" | 低 | 设计如此：预览态明确标注，真实态接 detector |
| 3 | 安全性 | AgentDetectionResult.hasCredentials 为模型层占位 | 中 | native 层读取配置时需实现真实标记 |
| 4 | 验收性 | Rust 后端测试未在本轮执行 | 中 | 需本地 cargo test --workspace |
| 5 | PRD 一致性 | CommandCode 资料未补齐 | 低 | 计划层已知，不阻塞架构 |

## 9. 第 1 轮结论

本轮实现已覆盖 Phase 1-5 的全部仓库内可落地内容。5 个 Phase 的数据模型、算法、UI 骨架、发布文档均已就位。`vite build` 通过。需要本地真实环境验收的项已明确标注，不能用"UI 看起来正常"代替。建议进入第 2 轮审查。
