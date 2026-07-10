# d-token 文档导航

本页是 `docs/` 的唯一入口。需要找文档时先看这里，不要直接在根目录里翻文件。

## 0. 当前最重要的文档

1. [`PRD.md`](PRD.md) — 当前产品定义、发布边界、页面结构和验收标准。优先级最高。
2. [`BACKEND_AUDIT_AND_ITERATION_2026-07-10.md`](BACKEND_AUDIT_AND_ITERATION_2026-07-10.md) — 当前工作树后端问题、发布阻断项与分阶段整改方案。
3. [`FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md`](FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md) — 当前前端 UI 问题、安装工作台重构规格与视觉验收方案。
4. [`release-plan-2026-07/README.md`](release-plan-2026-07/README.md) — 最新发布实施计划总索引。
5. [`release-plan-2026-07/implementation-kickoff.md`](release-plan-2026-07/implementation-kickoff.md) — 当前可以直接开始实现的 Sprint 顺序。
6. [`release-plan-2026-07/multi-round-audit.md`](release-plan-2026-07/multi-round-audit.md) — 多轮多维度审查报告，说明为什么当前计划可以开工。
7. [`PLAN.md`](PLAN.md) — 长期执行计划和里程碑任务。
8. [`ARCHITECTURE.md`](ARCHITECTURE.md) — d-token 架构总览。
9. [`SECURITY_BOUNDARIES.md`](SECURITY_BOUNDARIES.md) — 本地优先、安全、脱敏和不可越界规则。
10. [`DOCS_ORGANIZATION.md`](DOCS_ORGANIZATION.md) — 文档目录整理规范和后续迁移表。

## 1. 产品与页面规格

这些文档回答“产品是什么、页面怎么组织、用户看到什么”。

- [`PRD.md`](PRD.md) — Living PRD，当前唯一产品事实源。
- [`APP_FUNCTIONS_AND_PAGES.md`](APP_FUNCTIONS_AND_PAGES.md) — App 功能和页面说明。
- [`UI_CAPABILITY_MATRIX.md`](UI_CAPABILITY_MATRIX.md) — UI 能力矩阵。
- [`RUNTIME_CAPABILITY_MATRIX.md`](RUNTIME_CAPABILITY_MATRIX.md) — Runtime 能力矩阵。
- [`prd/headroom-dashboard-full-checklist.md`](prd/headroom-dashboard-full-checklist.md) — Headroom dashboard 对照清单。
- [`prd/headroom-dashboard-migration-matrix.md`](prd/headroom-dashboard-migration-matrix.md) — Headroom dashboard 迁移矩阵。

## 2. 发布实施计划

这些文档回答“从现在到发布怎么做”。

- [`release-plan-2026-07/README.md`](release-plan-2026-07/README.md) — 发布计划总索引。
- [`release-plan-2026-07/index.html`](release-plan-2026-07/index.html) — 可打开浏览的发布计划网页。
- [`release-plan-2026-07/implementation-kickoff.md`](release-plan-2026-07/implementation-kickoff.md) — 实施启动指令。
- [`release-plan-2026-07/phase-01-product-ia-ui-copy.md`](release-plan-2026-07/phase-01-product-ia-ui-copy.md) — Phase 1：信息架构、页面重构、UI 文案。
- [`release-plan-2026-07/phase-02-install-agent-ccswitch.md`](release-plan-2026-07/phase-02-install-agent-ccswitch.md) — Phase 2：一键安装、Agent 检测、cc-switch 同步。
- [`release-plan-2026-07/phase-03-smart-learning.md`](release-plan-2026-07/phase-03-smart-learning.md) — Phase 3：智能学习、扫描算法、受控写入。
- [`release-plan-2026-07/phase-04-analytics-doctor-logs-rollback.md`](release-plan-2026-07/phase-04-analytics-doctor-logs-rollback.md) — Phase 4：分析、Doctor、日志、回退。
- [`release-plan-2026-07/phase-05-ui-stability-release.md`](release-plan-2026-07/phase-05-ui-stability-release.md) — Phase 5：UI 完整度、稳定性、发布验收。
- [`release-plan-2026-07/multi-round-audit.md`](release-plan-2026-07/multi-round-audit.md) — 多轮审查报告。
- [`release-plan-2026-07/overall-review.md`](release-plan-2026-07/overall-review.md) — 跨阶段总审查。
- [`release-plan-2026-07/local-remaining-plan.md`](release-plan-2026-07/local-remaining-plan.md) — 本轮已完成事项与剩余本地真实环境任务。
- [`RELEASE_COMPLETION_PLAN.md`](RELEASE_COMPLETION_PLAN.md) — 旧发布完成计划。
- [`RELEASE_READINESS_REPORT.md`](RELEASE_READINESS_REPORT.md) — 旧发布就绪报告。

## 3. 架构、安全与技术栈

这些文档回答“系统怎么设计、边界在哪里、技术怎么选”。

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — 架构总览。
- [`TECH_STACK.md`](TECH_STACK.md) — 技术栈决策。
- [`tech-notes.md`](tech-notes.md) — 技术笔记。
- [`SECURITY_BOUNDARIES.md`](SECURITY_BOUNDARIES.md) — 安全边界。
- [`TESTING_NO_TOUCH.md`](TESTING_NO_TOUCH.md) — no-touch 测试指南。

## 4. 研究资料

这些文档回答“为什么这么做、外部参考是什么、技术调研结论是什么”。

- [`research/open-source-reference.md`](research/open-source-reference.md) — 开源参考。
- [`research/REPORT_01_RTK_HEADROOM_INTERNALS.md`](research/REPORT_01_RTK_HEADROOM_INTERNALS.md) — RTK / Headroom 内部机制研究。
- [`research/REPORT_02_DESKTOP_INTEGRATION_ONE_CLICK_APP.md`](research/REPORT_02_DESKTOP_INTEGRATION_ONE_CLICK_APP.md) — 桌面集成与一键 App 研究。
- [`research/REPORT_03_FEATURES_AND_FIXES_IMPLEMENTATION.md`](research/REPORT_03_FEATURES_AND_FIXES_IMPLEMENTATION.md) — 功能与修复实现蓝图。
- [`OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md`](OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md) — 光学/图片压缩实验计划。

## 5. 阶段报告与历史审查

这些文档是阶段交付记录，默认只作为历史证据，不应作为当前产品事实源。

- [`M1_STATUS.md`](M1_STATUS.md) — M1 状态。
- [`M1_REVIEW.md`](M1_REVIEW.md) — M1 审查。
- [`M1_FINAL_REVIEW.md`](M1_FINAL_REVIEW.md) — M1 最终审查。
- [`M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md`](M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md) — M1 no-touch 开发基础报告。
- [`M1_M3_DELIVERY_AUDIT.md`](M1_M3_DELIVERY_AUDIT.md) — M1-M3 交付审计。
- [`M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md`](M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md) — M2 外部只读集成报告。
- [`M3_GUI_CONTROL_PLANE_MVP_REPORT.md`](M3_GUI_CONTROL_PLANE_MVP_REPORT.md) — M3 GUI 控制面 MVP 报告。
- [`M4_REAL_CAPABILITY_OPENCODE_PLAN.md`](M4_REAL_CAPABILITY_OPENCODE_PLAN.md) — M4 OpenCode 真实能力计划。
- [`M4_OPENCODE_TEST_RESULTS_2026-07-04.md`](M4_OPENCODE_TEST_RESULTS_2026-07-04.md) — M4 OpenCode 测试结果。
- [`FINAL_REVIEW.md`](FINAL_REVIEW.md) — 总体最终审查。
- [`BACKEND_AUDIT_AND_ITERATION_2026-07-10.md`](BACKEND_AUDIT_AND_ITERATION_2026-07-10.md) — 2026-07-10 当前工作树后端审查与迭代建议。
- [`FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md`](FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md) — 2026-07-10 当前前端 UI 审查与迭代建议。
- [`reviews/PRD_MULTIDIMENSIONAL_GAP_REVIEW_2026-07-01.md`](reviews/PRD_MULTIDIMENSIONAL_GAP_REVIEW_2026-07-01.md) — PRD 多维缺口审查。
- [`reviews/RESEARCH_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-01.md`](reviews/RESEARCH_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-01.md) — 研究报告多维审查。
- [`reviews/NO_TOUCH_TESTING_REVIEW_2026-07-01.md`](reviews/NO_TOUCH_TESTING_REVIEW_2026-07-01.md) — no-touch 测试审查。

## 6. 测试计划与实验计划

这些文档回答“如何验证、如何实验”。

- [`OPENCODE_LOCAL_TEST_PLAN.md`](OPENCODE_LOCAL_TEST_PLAN.md) — OpenCode 本地测试计划。
- [`M4_OPENCODE_TEST_RESULTS_2026-07-04.md`](M4_OPENCODE_TEST_RESULTS_2026-07-04.md) — OpenCode 真实测试结果。
- [`TESTING_NO_TOUCH.md`](TESTING_NO_TOUCH.md) — no-touch 测试指南。
- [`OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md`](OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md) — 图片/光学压缩实验计划。

## 7. 归档与杂项

这些文档不是当前主线入口。

- [`archive/ROADMAP_M0_M6_HISTORICAL.md`](archive/ROADMAP_M0_M6_HISTORICAL.md) — M0-M6 历史路线图。
- [`NOTES.md`](NOTES.md) — 杂项笔记。

## 8. 当前整理原则

1. 当前事实源只看 `PRD.md` 和 `release-plan-2026-07/`。
2. 根目录历史阶段报告后续应迁入 `docs/history/` 或 `docs/archive/`。
3. 发布计划、测试计划、研究报告、审查报告应分目录，不再继续堆在根目录。
4. 移动文件前必须检查相对链接，避免破坏 README、PRD 或历史报告中的引用。
