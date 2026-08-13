# d-token 文档归档

更新时间：2026-08-13

本目录保存已经退出当前执行线、仅用于追溯的计划、审查、实验版本、Agent 工作记录和仓库整理记录。

## 使用规则

1. 归档文件不是当前产品事实源，不得直接作为实现依据。
2. 当前产品边界只看 `docs/PRD.md`。
3. 当前执行顺序只看 `docs/PLAN.md`。
4. 当前代码能力只看 `docs/product/IMPLEMENTATION_STATUS.md` 与实际源码、测试结果。
5. 历史里程碑证据仍保留在 `docs/history/`；它们同样只用于追溯。
6. 归档文件正文原则上不重写，避免丢失当时语境；其中旧路径、旧分支、旧测试数和旧架构描述均不代表当前状态。

## 目录说明

```text
archive/
├── agent-runs/             Agent 临时计划、发现与进度记录
├── legacy-notes/           已被正式文档吸收或与现架构冲突的旧笔记
├── optical/                Optical V6、早期报告和旧轮次审查
├── product-matrices/       已过期的 Dashboard Draft 与 UI/runtime 能力矩阵快照
├── release/                旧发布计划、旧 readiness 报告和网页快照
├── repository-cleanup/     旧占位文档与仓库整理过程记录
├── research/               已不再代表当前架构的早期研究报告
├── reviews/                已被后续代码和审查取代的专题审查
└── ROADMAP_M0_M6_HISTORICAL.md
```

## 本轮归档原因

- 旧发布文档基于 2026-07-06 至 2026-07-07 的 Windows/PowerShell 证据，无法证明 2026-07-11 当前分支和 macOS 候选状态。
- 旧 UI/runtime 矩阵包含已变化的 Tauri command、页面结构、打包状态和测试结论。
- Optical V6 与早期 Phase A 报告被 V7、当前实现状态和第五轮审查取代。
- 早期研究和技术笔记仍描述 Python runtime/外部 Headroom 过渡架构，与当前 Rust-owned runtime 不一致。
- 根目录 `task_plan.md`、`findings.md`、`progress.md` 属于一次 Agent 会话过程，不应继续充当仓库顶层入口。
- `support/` 根目录中的占位、重复说明和零散 notes 不属于正式支持资产，已集中归档。
- 2026-08-13 将旧 `docs/PLAN.md` 归档为 `archive/release/2026-08-12-PLAN-pre-product-recovery.md`；旧大型 release DAG 与 44/65 等历史进度口径退出当前执行线，当前执行只看新的 `docs/PLAN.md`。
