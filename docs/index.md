# d-token 文档导航

本页是 `docs/` 的唯一入口。需要找文档时先看这里，不要直接在根目录里翻文件。

## 0. 当前最重要的文档

1. [`PRD.md`](PRD.md) — 当前产品定义、发布边界、页面结构和验收标准。优先级最高。
2. [`PLAN.md`](PLAN.md) — 当前唯一 Living Plan；记录已交付事实、发布阻断和下一实施顺序。
3. [`history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md`](history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md) — 当前工作树后端问题、已修复项与剩余发布阻断。
4. [`history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md`](history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md) — 当前前端 UI 问题、安装工作台重构规格与视觉验收方案。
5. [`ARCHITECTURE.md`](ARCHITECTURE.md) — 当前 Rust core + Tauri 2 + React/TypeScript 架构总览。
6. [`TECH_STACK.md`](TECH_STACK.md) — Windows/macOS 接受的技术栈与平台边界。
7. [`optical/README.md`](optical/README.md) — Hybrid Context Optimization / Optical 当前规范、实现状态和最新审查入口。
8. [`../support/pending-commits.json`](../support/pending-commits.json) — 按阶段记录文件、时间、验证、是否已提交和提交哈希。
9. [`SECURITY_BOUNDARIES.md`](SECURITY_BOUNDARIES.md) — 本地优先、安全、脱敏和不可越界规则。
10. [`DOCS_ORGANIZATION.md`](DOCS_ORGANIZATION.md) — 文档目录整理规范和后续迁移表。

## 1. 产品与页面规格

这些文档回答"产品是什么、页面怎么组织、用户看到什么"。

- [`PRD.md`](PRD.md) — Living PRD，当前唯一产品事实源。
- [`APP_FUNCTIONS_AND_PAGES.md`](APP_FUNCTIONS_AND_PAGES.md) — App 功能和页面说明。
- [`product/UI_CAPABILITY_MATRIX.md`](product/UI_CAPABILITY_MATRIX.md) — UI 能力矩阵。
- [`product/RUNTIME_CAPABILITY_MATRIX.md`](product/RUNTIME_CAPABILITY_MATRIX.md) — Runtime 能力矩阵。
- [`prd/headroom-dashboard-full-checklist.md`](prd/headroom-dashboard-full-checklist.md) — Headroom dashboard 对照清单。
- [`prd/headroom-dashboard-migration-matrix.md`](prd/headroom-dashboard-migration-matrix.md) — Headroom dashboard 迁移矩阵。

## 2. 发布计划历史快照

这些文件保留 2026-07 当时的阶段设计、审查和追溯证据，不再与 `PLAN.md` 并列为当前执行真理源。计划中的“可直接开工”“未启用打包”等表述必须按历史时间理解；当前状态只看 `PRD.md`、`PLAN.md` 和最新前后端审查。

- [`release/release-plan-2026-07/README.md`](release/release-plan-2026-07/README.md) — 历史发布计划总索引。
- [`release/release-plan-2026-07/index.html`](release/release-plan-2026-07/index.html) — 可打开浏览的发布计划网页。
- [`release/release-plan-2026-07/implementation-kickoff.md`](release/release-plan-2026-07/implementation-kickoff.md) — 历史实施启动指令。
- [`release/release-plan-2026-07/phase-01-product-ia-ui-copy.md`](release/release-plan-2026-07/phase-01-product-ia-ui-copy.md) — Phase 1：信息架构、页面重构、UI 文案。
- [`release/release-plan-2026-07/phase-02-install-agent-ccswitch.md`](release/release-plan-2026-07/phase-02-install-agent-ccswitch.md) — Phase 2：一键安装、Agent 检测、cc-switch 同步。
- [`release/release-plan-2026-07/phase-03-smart-learning.md`](release/release-plan-2026-07/phase-03-smart-learning.md) — Phase 3：智能学习、扫描算法、受控写入。
- [`release/release-plan-2026-07/phase-04-analytics-doctor-logs-rollback.md`](release/release-plan-2026-07/phase-04-analytics-doctor-logs-rollback.md) — Phase 4：分析、Doctor、日志、回退。
- [`release/release-plan-2026-07/phase-05-ui-stability-release.md`](release/release-plan-2026-07/phase-05-ui-stability-release.md) — Phase 5：UI 完整度、稳定性、发布验收。
- [`release/release-plan-2026-07/multi-round-audit.md`](release/release-plan-2026-07/multi-round-audit.md) — 多轮审查报告。
- [`release/release-plan-2026-07/overall-review.md`](release/release-plan-2026-07/overall-review.md) — 跨阶段总审查。
- [`release/release-plan-2026-07/local-remaining-plan.md`](release/release-plan-2026-07/local-remaining-plan.md) — 本轮已完成事项与剩余本地真实环境任务。
- [`release/RELEASE_COMPLETION_PLAN.md`](release/RELEASE_COMPLETION_PLAN.md) — 历史发布完成计划，不代表当前 readiness。
- [`release/RELEASE_READINESS_REPORT.md`](release/RELEASE_READINESS_REPORT.md) — 历史发布就绪报告，不代表当前候选版。

## 3. 架构、安全与技术栈

这些文档回答"系统怎么设计、边界在哪里、技术怎么选"。

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — 架构总览。
- [`TECH_STACK.md`](TECH_STACK.md) — 技术栈决策。
- [`notes/tech-notes.md`](notes/tech-notes.md) — 技术笔记。
- [`SECURITY_BOUNDARIES.md`](SECURITY_BOUNDARIES.md) — 安全边界。
- [`testing/TESTING_NO_TOUCH.md`](testing/TESTING_NO_TOUCH.md) — no-touch 测试指南。

## 4. 研究资料

这些文档回答"为什么这么做、外部参考是什么、技术调研结论是什么"。

- [`research/open-source-reference.md`](research/open-source-reference.md) — 开源参考。
- [`research/REPORT_01_RTK_HEADROOM_INTERNALS.md`](research/REPORT_01_RTK_HEADROOM_INTERNALS.md) — RTK / Headroom 内部机制研究。
- [`research/REPORT_02_DESKTOP_INTEGRATION_ONE_CLICK_APP.md`](research/REPORT_02_DESKTOP_INTEGRATION_ONE_CLICK_APP.md) — 桌面集成与一键 App 研究。
- [`research/REPORT_03_FEATURES_AND_FIXES_IMPLEMENTATION.md`](research/REPORT_03_FEATURES_AND_FIXES_IMPLEMENTATION.md) — 功能与修复实现蓝图。
- [`experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`](experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md) — 当前 Hybrid Context Optimization / Optical 架构与实施真理源。
- [`experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md`](experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md) — V6 光学/图片压缩基础规格。
- [`experiments/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md`](experiments/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md) — 光学/图片压缩 Phase A 实现报告。
- [`optical/README.md`](optical/README.md) — 当前规范、状态、测试和审查总入口。
- [`optical/review-round-5.md`](optical/review-round-5.md) — Capability 生命周期、稳定指纹、Provider 状态机和提交审计的第五轮深度审查。

## 5. 阶段报告与历史审查

这些文档是阶段交付记录，默认只作为历史证据，不应作为当前产品事实源。

- [`history/M1_STATUS.md`](history/M1_STATUS.md) — M1 状态。
- [`history/M1_REVIEW.md`](history/M1_REVIEW.md) — M1 审查。
- [`history/M1_FINAL_REVIEW.md`](history/M1_FINAL_REVIEW.md) — M1 最终审查。
- [`history/M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md`](history/M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md) — M1 no-touch 开发基础报告。
- [`history/M1_M3_DELIVERY_AUDIT.md`](history/M1_M3_DELIVERY_AUDIT.md) — M1-M3 交付审计。
- [`history/M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md`](history/M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md) — M2 外部只读集成报告。
- [`history/M3_GUI_CONTROL_PLANE_MVP_REPORT.md`](history/M3_GUI_CONTROL_PLANE_MVP_REPORT.md) — M3 GUI 控制面 MVP 报告。
- [`history/M4_REAL_CAPABILITY_OPENCODE_PLAN.md`](history/M4_REAL_CAPABILITY_OPENCODE_PLAN.md) — M4 OpenCode 真实能力计划。
- [`history/M4_OPENCODE_TEST_RESULTS_2026-07-04.md`](history/M4_OPENCODE_TEST_RESULTS_2026-07-04.md) — M4 OpenCode 测试结果。
- [`history/FINAL_REVIEW.md`](history/FINAL_REVIEW.md) — 历史总体审查；当前状态以 2026-07-10 前后端审查为准。
- [`history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md`](history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md) — 2026-07-10 当前工作树后端审查与迭代建议。
- [`history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md`](history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md) — 2026-07-10 当前前端 UI 审查与迭代建议。
- [`history/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md`](history/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md) — 原始 Optical Phase A 阶段报告快照；当前状态以 `experiments/` 与 `optical/` 文档为准。
- [`reviews/PRD_MULTIDIMENSIONAL_GAP_REVIEW_2026-07-01.md`](reviews/PRD_MULTIDIMENSIONAL_GAP_REVIEW_2026-07-01.md) — PRD 多维缺口审查。
- [`reviews/RESEARCH_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-01.md`](reviews/RESEARCH_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-01.md) — 研究报告多维审查。
- [`reviews/NO_TOUCH_TESTING_REVIEW_2026-07-01.md`](reviews/NO_TOUCH_TESTING_REVIEW_2026-07-01.md) — no-touch 测试审查。

## 6. 测试计划与实验计划

这些文档回答"如何验证、如何实验"。

- [`testing/OPENCODE_LOCAL_TEST_PLAN.md`](testing/OPENCODE_LOCAL_TEST_PLAN.md) — OpenCode 本地测试计划。
- [`history/M4_OPENCODE_TEST_RESULTS_2026-07-04.md`](history/M4_OPENCODE_TEST_RESULTS_2026-07-04.md) — OpenCode 真实测试结果。
- [`testing/TESTING_NO_TOUCH.md`](testing/TESTING_NO_TOUCH.md) — no-touch 测试指南。
- [`experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md`](experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md) — 图片/光学压缩实验计划。
- [`experiments/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md`](experiments/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md) — 图片/光学压缩 Phase A 实现报告。

## 7. 归档与杂项

这些文档不是当前主线入口。

- [`archive/ROADMAP_M0_M6_HISTORICAL.md`](archive/ROADMAP_M0_M6_HISTORICAL.md) — M0-M6 历史路线图。
- [`notes/NOTES.md`](notes/NOTES.md) — 杂项笔记。

## 8. 当前整理原则

1. 产品事实源只看 `PRD.md`，当前执行状态只看 `PLAN.md`；最新审查是证据和整改输入，不取代二者。
2. 根目录历史阶段报告已迁入 `docs/history/`。
3. 历史发布计划迁入 `docs/release/`，测试计划迁入 `docs/testing/`，研究报告在 `docs/research/`。
4. 移动文件前必须检查相对链接，避免破坏 README、PRD 或历史报告中的引用。
