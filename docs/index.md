# d-token 文档导航

更新时间：2026-07-11 晚间

本页是 `docs/` 的唯一导航入口。当前事实、执行计划、历史证据和归档材料必须分开理解。

## 1. 当前必读

1. [`PRD.md`](PRD.md) — 唯一产品真理源：产品边界、信息架构、安全要求和验收标准。
2. [`PLAN.md`](PLAN.md) — 唯一 Living Plan：当前实施顺序、发布阻断和下一步。
3. [`product/IMPLEMENTATION_STATUS.md`](product/IMPLEMENTATION_STATUS.md) — 2026-07-11 晚间按实际源码整理的当前能力与限制。
4. [`ARCHITECTURE.md`](ARCHITECTURE.md) — Rust-owned runtime、Tauri 桌面壳和本地服务架构。
5. [`TECH_STACK.md`](TECH_STACK.md) — Windows/macOS 技术选型和平台边界。
6. [`SECURITY_BOUNDARIES.md`](SECURITY_BOUNDARIES.md) — loopback、凭据、脱敏、配置写入和进程边界。
7. [`APP_FUNCTIONS_AND_PAGES.md`](APP_FUNCTIONS_AND_PAGES.md) — 当前六个一级页面与实际代码组件映射。

## 2. 当前代码与产品专题

- [`product/IMPLEMENTATION_STATUS.md`](product/IMPLEMENTATION_STATUS.md) — 当前代码事实快照。
- [`PRD.md`](PRD.md#125-桌面视觉布局与交互产品契约) — UI 视觉、布局、交互、真实性和验收的唯一产品契约。
- [`research/open-source-reference.md`](research/open-source-reference.md) — 外部项目参考，只用于设计与研究，不是运行时依赖。
- [`research/aimami-dtoken-deep-study-2026-07-12/README.md`](research/aimami-dtoken-deep-study-2026-07-12/README.md) — AiMaMi 源码、桌面产品、前端 UI/动画、Tauri/Rust、状态、算法、安全、差距与三轮审核的完整专题研究。
- [`legal/THIRD_PARTY_NOTICES.md`](legal/THIRD_PARTY_NOTICES.md) — 直接适配第三方实现的版权、固定来源、许可证与修改声明唯一入口。

2026-07-01 的 Headroom Dashboard Draft 清单/迁移矩阵已归入 `archive/product-matrices/2026-07-01/`；当前页面与数据合同以 PRD、App 页面文档和实际代码为准。

## 3. Optical / Hybrid Context Optimization

当前 Optical 只允许 shadow preview 与 metadata-only observability，不允许生产 active mutation。

- [`experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`](experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md) — 当前实验架构与实施计划。
- [`optical/README.md`](optical/README.md) — Optical 文档总入口。
- [`optical/implementation-status.md`](optical/implementation-status.md) — 当前实现矩阵。
- [`optical/test-matrix.md`](optical/test-matrix.md) — 测试合同。
- [`optical/release-gates.md`](optical/release-gates.md) — active transform 发布禁令和门槛。
- [`optical/review-round-5.md`](optical/review-round-5.md) — 最新审查；前四轮已归档。

## 4. 测试与验收

- [`testing/TESTING_NO_TOUCH.md`](testing/TESTING_NO_TOUCH.md) — 开发期 no-touch 测试边界。
- [`testing/OPENCODE_LOCAL_TEST_PLAN.md`](testing/OPENCODE_LOCAL_TEST_PLAN.md) — OpenCode 真实路由验收约束。
- [`history/M4_OPENCODE_TEST_RESULTS_2026-07-04.md`](history/M4_OPENCODE_TEST_RESULTS_2026-07-04.md) — 历史测试证据；不能替代当前候选 commit 的复验。

## 5. 历史审查与里程碑证据

`history/` 中的文件全部是历史快照，仅用于追溯。当前状态以 PRD、PLAN、实现状态和当前源码为准。

- [`history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md`](history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md) — 2026-07-10 后端审查输入。
- [`history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md`](history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md) — 2026-07-10 前端审查输入。
- [`history/FINAL_REVIEW.md`](history/FINAL_REVIEW.md) — 更早的总体审查，包含已过期架构描述。
- `history/M1_*`、`M2_*`、`M3_*`、`M4_*` — 阶段交付和测试记录。

## 6. 归档

[`archive/README.md`](archive/README.md) 说明归档规则和目录。以下材料已退出当前执行线：

- 2026-07 旧 release plan、readiness 报告和网页。
- 2026-07-01 旧专题审查与研究报告。
- 2026-07-07 旧 UI/runtime capability matrix。
- Optical V6、早期 Phase A 报告和 review round 1–4。
- 根目录 Agent 会话文件 `task_plan.md`、`findings.md`、`progress.md`。
- `support/` 根目录重复 notes、placeholder 和仓库整理过程文件。

## 7. 文档优先级

发生冲突时按以下顺序处理：

```text
实际源码与当前测试证据
→ docs/PRD.md
→ docs/PLAN.md
→ docs/product/IMPLEMENTATION_STATUS.md
→ ARCHITECTURE / TECH_STACK / SECURITY_BOUNDARIES
→ 当前专题文档
→ history/
→ archive/
```

产品边界不能仅凭代码偶然实现改变；如源码与 PRD 冲突，必须明确记录差异并决定改代码还是改 PRD，禁止静默选择历史文档。
