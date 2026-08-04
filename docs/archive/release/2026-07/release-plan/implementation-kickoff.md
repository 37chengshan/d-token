# d-token 实施启动指令

本文件用于把 `docs/release-plan-2026-07/` 的规划转成实现阶段入口。当前计划已经完成多轮审查，可以从 Phase 1 直接开始。

## 总原则

1. 先做产品信息架构，再做真实功能接入。
2. 不把开发测试、benchmark、fixture、payload 等概念暴露给正式 UI。
3. 第三方工具只检测，不自动安装。
4. cc-switch 默认只读，但必须同步当前 profile/config/upstream。
5. 所有真实写入必须有 diff、backup、verify、rollback。
6. 智能学习可以直接写入，但只能插入或追加 d-token 管理块。
7. 日志和导出默认脱敏。
8. 所有页面都要有 loading、empty、ready、warning、error、success。

## 开工顺序

### Sprint 1：Phase 1 UI 骨架

目标：把用户可见导航和第二页工作台结构重构出来。

任务：

1. 修改 `code/apps/desktop/src/app/lib/nav.ts`。
2. 修改 `code/apps/desktop/src/app/lib/i18n.ts`。
3. 修改 `code/apps/desktop/src/app/components/shell/AppShell.tsx`。
4. 新建或重构“安装与连接工作台”页面容器。
5. 在第二页放入四个标签：一键安装、智能学习、高级安装、日志。
6. 首页增加服务、Agent、cc-switch、估算节省、最近问题摘要。
7. 增加统一数据来源标识和空状态。

验收：用户打开 App 后看到的是新信息架构，不再是旧工程页面集合。

### Sprint 2：Phase 2 真实连接闭环

目标：d-token 服务、Agent 检测、cc-switch 只读读取和上游同步可用。

任务：

1. 新建 AgentDescriptor registry。
2. 新建 cc-switch detector。
3. 新建 upstream normalization/comparison。
4. 新建 cc-switch sync status calculator。
5. 新建 install transaction state machine。
6. 将配置计划接入 diff、backup、verify、rollback。
7. 至少完成一个重点 Agent 的真实闭环。

验收：cc-switch profile 切换后，d-token 上游能同步或明确提示不同步。

### Sprint 3：Phase 3 智能学习

目标：全面扫描、快速排序、生成学习结果、受控写入文档。

任务：

1. 新建 candidate discovery。
2. 新建 light indexer。
3. 新建 scoring engine。
4. 新建 deep scanner。
5. 新建 LearningContext schema。
6. 新建 managed block writer。
7. 新建 non-managed-content invariant checker。
8. 接入 rollback。

验收：智能学习只新增/更新 d-token 管理块，不改动用户其他内容。

### Sprint 4：Phase 4 可观测与回退

目标：分析、Doctor、日志、回退形成可信闭环。

任务：

1. 新建 AnalyticsSnapshot 聚合层。
2. 新建 IssueDefinition registry。
3. 新建 RepairAction executor。
4. 新建 LogEvent schema 和 RedactionPolicy。
5. 扩展 RollbackPoint 支持智能学习文档。
6. 新建 AppOperationalState。

验收：同一问题在首页、安装页、Doctor、日志和回退页状态一致。

### Sprint 5：Phase 5 发布收口

目标：补齐测试矩阵、release blockers、README 和用户说明。

任务：

1. 逐页 UI 完整度审查。
2. Windows/macOS 核心流程验收。
3. cc-switch 同步测试。
4. 智能学习写入不变性测试。
5. 日志导出脱敏测试。
6. 更新 README。
7. 发布前执行 release blocker checklist。

验收：无 release blocker，可进入公开 Beta / 候选发布。

## 立即开始建议

先执行 Sprint 1，不要先写智能学习或 Doctor。页面骨架稳定后，再把 Phase 2 的真实连接能力接进去。
