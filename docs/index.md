# d-token 文档导航

更新时间：2026-08-08

本页是 `docs/` 的唯一导航入口。当前事实、执行计划、历史证据和归档材料必须分开理解。当前 `0.0.1` 仅是 macOS ARM64 / Windows x64 的未签名技术预览，不是已签名、公证或公开发布的安装包；系统安全警告、不可用能力与未完成的发布门禁均以 README、变更记录和实现状态为准。

## 1. 当前必读

1. [`PRD.md`](PRD.md) — 唯一产品真理源：产品边界、信息架构、安全要求和验收标准。
2. [`PLAN.md`](PLAN.md) — 唯一 Living Plan：当前实施顺序、发布阻断和下一步。
3. [`releases/v0.0.1/README.md`](releases/v0.0.1/README.md) — v0.0.1 最终发布计划包（权威入口）：九文件执行分解、版本冲突（VERSION-0）、切片 DAG、验证门（N0–N9/G0–G9）与证据索引。
4. [`product/IMPLEMENTATION_STATUS.md`](product/IMPLEMENTATION_STATUS.md) — 2026-07-15 按实际源码与最终本地门整理的当前能力与限制。
5. [`ARCHITECTURE.md`](ARCHITECTURE.md) — Rust-owned runtime、Tauri 桌面壳和本地服务架构。
6. [`TECH_STACK.md`](TECH_STACK.md) — Windows/macOS 技术选型和平台边界。
7. [`SECURITY_BOUNDARIES.md`](SECURITY_BOUNDARIES.md) — loopback、凭据、脱敏、配置写入和进程边界。
8. [`APP_FUNCTIONS_AND_PAGES.md`](APP_FUNCTIONS_AND_PAGES.md) — 当前八个一级页面（含视觉压缩 Beta 与代码图谱实验页）与实际代码组件映射。
9. [`../CHANGELOG.md`](../CHANGELOG.md) — 未签名技术预览的版本措辞、用户可见变化与已知限制。
10. [`../SECURITY.md`](../SECURITY.md) — 漏洞报告、支持范围、系统安全警告与发布完整性策略。
11. [`../THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) — 面向源码与安装包分发的第三方声明。
12. [`../LICENSE`](../LICENSE) — d-token 项目的 Apache License 2.0 授权文本。

## 2. 当前代码与产品专题

- [`product/IMPLEMENTATION_STATUS.md`](product/IMPLEMENTATION_STATUS.md) — 当前代码事实快照。
- [`PRD.md`](PRD.md#125-桌面视觉布局与交互产品契约) — UI 视觉、布局、交互、真实性和验收的唯一产品契约。
- [`research/open-source-reference.md`](research/open-source-reference.md) — 外部项目参考，只用于设计与研究，不是运行时依赖。
- [`research/aimami-dtoken-deep-study-2026-07-12/README.md`](research/aimami-dtoken-deep-study-2026-07-12/README.md) — AiMaMi 源码、桌面产品、前端 UI/动画、Tauri/Rust、状态、算法、安全、差距与三轮审核的完整专题研究。
- [`research/DESKTOP_UI_OPTIMIZATION_RESEARCH_2026-07-13.md`](research/DESKTOP_UI_OPTIMIZATION_RESEARCH_2026-07-13.md) — 2026-07-13 基于生产入口源码的桌面 UI 优化研究：UX、性能、Claude/神话感视觉、二级页与切页、侧栏 Tooltip/图标动画对照 AiMaMi。
- [`research/FRONTEND_UI_PERFORMANCE_AUDIT_2026-07-15.md`](research/FRONTEND_UI_PERFORMANCE_AUDIT_2026-07-15.md) — 当前 React/Tauri 前端 UI、启动、渲染、IPC、长列表、动画与性能门禁的系统性审计。
- [`research/AGENT_ONE_CLICK_AND_CCSWITCH_REPAIR_PLAN_2026-07-19.md`](research/AGENT_ONE_CLICK_AND_CCSWITCH_REPAIR_PLAN_2026-07-19.md) — Agent 一键配置、cc-switch 发现/同步、配置漂移与 runtime reload 的根因研究和分轮 TDD 修复计划。
- [`research/BACKEND_PERFORMANCE_AUDIT_2026-07-15.md`](research/BACKEND_PERFORMANCE_AUDIT_2026-07-15.md) — 当前 Rust runtime、Dashboard、持久化、Tauri snapshot、智能学习和并发性能的系统性审计。
- [`research/PERFORMANCE_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-15.md`](research/PERFORMANCE_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-15.md) — 对前后端性能报告的事实、优先级、安全、跨平台与实施依赖交叉复核。
- [`research/RELEASE_READINESS_SYNTHESIS_2026-07-16.md`](research/RELEASE_READINESS_SYNTHESIS_2026-07-16.md) — 2026-07-16 发布前全面审查的**综合总览与统一迭代路线图**（三份子报告的交叉综合、把关与决策项）。
- [`research/claude report/FRONTEND_AND_INTEGRATION_REVIEW_2026-07-16.md`](<research/claude report/FRONTEND_AND_INTEGRATION_REVIEW_2026-07-16.md>) — 2026-07-16 前端 UI 与前后端对接审查（信息架构、设计系统、统一状态模型、对接契约、可访问性）。
- [`research/claude report/BACKEND_CAPABILITIES_REVIEW_2026-07-16.md`](<research/claude report/BACKEND_CAPABILITIES_REVIEW_2026-07-16.md>) — 2026-07-16 后端各层能力审查（17 crate 能力完整性、Headroom §8/G0-G8 迁移达成度、安全边界、事务回退、占位识别）。
- [`research/claude report/UX_FLOW_AND_PERFORMANCE_REVIEW_2026-07-16.md`](<research/claude report/UX_FLOW_AND_PERFORMANCE_REVIEW_2026-07-16.md>) — 2026-07-16 用户流程与整体性能审查（七页旅程贯通度、一键安装状态机、性能预算达成度、release 实测缺口）。
- [`research/CODEBASE_MEMORY_MCP_INTEGRATION_RESEARCH_2026-07-16.md`](research/CODEBASE_MEMORY_MCP_INTEGRATION_RESEARCH_2026-07-16.md) — `codebase-memory-mcp` 作为 d-token 实验性受管 sidecar 的上游能力、生命周期、安全、许可证、产品边界与实施前门槛研究。
- [`legal/THIRD_PARTY_NOTICES.md`](legal/THIRD_PARTY_NOTICES.md) — 直接适配第三方实现的版权、固定来源、许可证与修改声明唯一入口。
- [`media/promo/README.md`](media/promo/README.md) — v2.1 README 宣传图需求说明：横幅素材现状、og:image 生成提示词与产品实拍截图需求。
- [`agent-mcp-parallel-dev-prompts.md`](agent-mcp-parallel-dev-prompts.md) — Codex 主 agent → Agent-MCP → AtomCode worker 的并行开发提示词模板（工作流参考，不是产品功能）。

2026-07-01 的 Headroom Dashboard Draft 清单/迁移矩阵已归入 `archive/product-matrices/2026-07-01/`；当前页面与数据合同以 PRD、App 页面文档和实际代码为准。

## 3. Optical / Hybrid Context Optimization

当前 Optical 只允许 shadow preview 与 metadata-only observability，不允许生产 active mutation。

- [`experiments/HYBRID_CONTEXT_OPTIMIZATION_ENGINE_PLAN_V8.md`](experiments/HYBRID_CONTEXT_OPTIMIZATION_ENGINE_PLAN_V8.md) — 当前 Hybrid Context Optimization Engine 完整后端迭代合同。
- [`experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`](experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md) — 上一轮实验架构记录。
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
- [`history/OPTICAL_RELEASE_ITERATION_REPORT_2026-08-06.md`](history/OPTICAL_RELEASE_ITERATION_REPORT_2026-08-06.md) — 本轮桌面 Optical 研究报告切片的实现与验证证据。

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
