# docs 目录整理规范

## 1. 当前状态

文档迁移已于 2026-07-10 完成。`docs/` 根目录现有 8 个文件（7 个长期稳定事实源/入口，加 `index.md` 导航页）；阶段报告、发布计划、测试计划、实验记录、产品矩阵和笔记均已归入对应子目录。

`docs/PRD.md` 是唯一产品真理源。历史审查、阶段报告和旧发布结论只能作为证据或背景，不得覆盖 PRD 的当前产品契约。

## 2. 根目录保留规则

`docs/` 根目录只保留当前主线入口和长期稳定事实源：

```text
APP_FUNCTIONS_AND_PAGES.md
ARCHITECTURE.md
DOCS_ORGANIZATION.md
PLAN.md
PRD.md
SECURITY_BOUNDARIES.md
TECH_STACK.md
index.md
```

新增阶段报告、旧审查、实验记录、发布报告或零散笔记时，不得放回根目录。

## 3. 规范目录结构

```text
docs/
├── index.md
├── DOCS_ORGANIZATION.md
├── PRD.md
├── PLAN.md
├── ARCHITECTURE.md
├── SECURITY_BOUNDARIES.md
├── TECH_STACK.md
├── APP_FUNCTIONS_AND_PAGES.md
├── product/       产品与 runtime 能力矩阵
├── release/       发布计划、发布就绪报告及分阶段实施计划
├── testing/       测试指南与测试计划
├── experiments/   图片/光学压缩实验计划与实验报告
├── research/      技术调研和开源参考
├── reviews/       专题审查报告
├── history/       阶段交付、旧状态和历史最终审查
├── archive/       已退出当前执行线的路线图
├── notes/         杂项与技术笔记
├── optical/       当前图片/光学压缩规范
└── prd/           PRD 的专题清单与迁移矩阵
```

## 4. 已完成迁移

| 分类 | 规范位置 |
|---|---|
| 产品矩阵 | `docs/product/UI_CAPABILITY_MATRIX.md`、`docs/product/RUNTIME_CAPABILITY_MATRIX.md` |
| 发布资料 | `docs/release/RELEASE_COMPLETION_PLAN.md`、`docs/release/RELEASE_READINESS_REPORT.md`、`docs/release/release-plan-2026-07/` |
| 测试资料 | `docs/testing/TESTING_NO_TOUCH.md`、`docs/testing/OPENCODE_LOCAL_TEST_PLAN.md` |
| 图片压缩实验 | `docs/experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md`、`docs/experiments/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md` |
| 阶段历史 | `docs/history/M1_STATUS.md` 至 `docs/history/M4_OPENCODE_TEST_RESULTS_2026-07-04.md` 的里程碑证据 |
| 当前审查快照 | `docs/history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md`、`docs/history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md` |
| 历史总审查 | `docs/history/FINAL_REVIEW.md` |
| 笔记 | `docs/notes/NOTES.md`、`docs/notes/tech-notes.md` |

完整导航以 [`index.md`](index.md) 为准。

## 5. 链接规则

1. 从仓库根目录引用文档时使用完整规范路径，例如 `docs/history/FINAL_REVIEW.md`。
2. `docs/` 内 Markdown 链接优先使用相对路径，并确保目标在移动后真实存在。
3. `AGENTS.md`、`CLAUDE.md`、`docs/PRD.md`、`docs/PLAN.md` 和 `docs/index.md` 必须始终指向规范位置。
4. 历史文档正文保留当时语境；其中旧路径只作为历史证据，不是当前导航契约。
5. 不创建第二份 PRD 或兼容占位副本；所有产品变更直接修改 `docs/PRD.md`。

## 6. 新文档归档规则

1. 当前事实源使用简短大写文件名，例如 `PRD.md`、`ARCHITECTURE.md`。
2. 发布计划使用 `release/release-plan-YYYY-MM/`。
3. 阶段交付和状态报告进入 `history/`。
4. 专题审查进入 `reviews/`，命名为 `<TOPIC>_REVIEW_YYYY-MM-DD.md`。
5. 研究报告进入 `research/`，实验计划和实验报告进入 `experiments/`。
6. 新增、移动或删除文档后同步更新 `docs/index.md` 与 `support/scripts/check_structure.py`。

## 7. 变更检查清单

1. 确认 `docs/PRD.md` 仍是唯一 PRD。
2. 检查根目录入口、PRD、PLAN 和发布计划中的路径。
3. 检查 Markdown 相对链接目标存在。
4. 运行 `python3 support/scripts/check_structure.py`。
5. 搜索已迁移的旧根路径，区分需要修复的当前契约与应保留的历史原文。
