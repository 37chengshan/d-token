# docs 目录整理规范

## 1. 当前问题

`docs/` 根目录现在混合了当前事实源、PRD、架构、安全、测试计划、发布计划、M1-M4 阶段报告、审查报告、实验计划和杂项笔记。文件数量不算特别多，但类型混杂，导致两个问题：

1. 新人不知道应该先看哪份文档。
2. 历史报告容易和当前事实源混在一起，造成实现时引用过期结论。

当前整理策略采用“先建立索引与规范，再逐步迁移文件”的方式。原因是仓库里已有大量文档互相引用，直接批量移动可能破坏相对链接。

## 2. 根目录保留规则

`docs/` 根目录只应该保留当前主线入口和长期稳定事实源。

建议保留：

```text
DOCS_ORGANIZATION.md
index.md
PRD.md
PLAN.md
ARCHITECTURE.md
SECURITY_BOUNDARIES.md
TECH_STACK.md
APP_FUNCTIONS_AND_PAGES.md
```

可选保留：

```text
TESTING_NO_TOUCH.md
RUNTIME_CAPABILITY_MATRIX.md
UI_CAPABILITY_MATRIX.md
```

不建议长期留在根目录：阶段报告、旧审查、实验计划、旧 release readiness、旧 M1/M2/M3/M4 报告。

## 3. 推荐目标结构

建议未来将 `docs/` 整理成以下结构：

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
├── product/
│   ├── UI_CAPABILITY_MATRIX.md
│   └── RUNTIME_CAPABILITY_MATRIX.md
├── release/
│   ├── RELEASE_COMPLETION_PLAN.md
│   ├── RELEASE_READINESS_REPORT.md
│   └── release-plan-2026-07/
├── testing/
│   ├── TESTING_NO_TOUCH.md
│   ├── OPENCODE_LOCAL_TEST_PLAN.md
│   └── M4_OPENCODE_TEST_RESULTS_2026-07-04.md
├── research/
│   ├── open-source-reference.md
│   ├── REPORT_01_RTK_HEADROOM_INTERNALS.md
│   ├── REPORT_02_DESKTOP_INTEGRATION_ONE_CLICK_APP.md
│   └── REPORT_03_FEATURES_AND_FIXES_IMPLEMENTATION.md
├── reviews/
│   ├── PRD_MULTIDIMENSIONAL_GAP_REVIEW_2026-07-01.md
│   ├── RESEARCH_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-01.md
│   └── NO_TOUCH_TESTING_REVIEW_2026-07-01.md
├── history/
│   ├── M1_STATUS.md
│   ├── M1_REVIEW.md
│   ├── M1_FINAL_REVIEW.md
│   ├── M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md
│   ├── M1_M3_DELIVERY_AUDIT.md
│   ├── M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md
│   ├── M3_GUI_CONTROL_PLANE_MVP_REPORT.md
│   ├── M4_REAL_CAPABILITY_OPENCODE_PLAN.md
│   └── FINAL_REVIEW.md
├── experiments/
│   └── OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md
├── archive/
│   └── ROADMAP_M0_M6_HISTORICAL.md
└── notes/
    ├── NOTES.md
    └── tech-notes.md
```

## 4. 文件迁移表

| 当前文件 | 建议位置 | 原因 |
|---|---|---|
| `PRD.md` | 保留根目录 | 当前产品事实源 |
| `PLAN.md` | 保留根目录 | 长期执行计划 |
| `ARCHITECTURE.md` | 保留根目录 | 架构事实源 |
| `SECURITY_BOUNDARIES.md` | 保留根目录 | 安全事实源 |
| `TECH_STACK.md` | 保留根目录 | 技术栈事实源 |
| `APP_FUNCTIONS_AND_PAGES.md` | 保留根目录 | App 页面事实源 |
| `UI_CAPABILITY_MATRIX.md` | `product/` | 产品/UI 能力矩阵 |
| `RUNTIME_CAPABILITY_MATRIX.md` | `product/` | Runtime 能力矩阵 |
| `RELEASE_COMPLETION_PLAN.md` | `release/` | 发布计划 |
| `RELEASE_READINESS_REPORT.md` | `release/` | 发布就绪报告 |
| `release-plan-2026-07/` | `release/release-plan-2026-07/` | 当前发布实施计划 |
| `TESTING_NO_TOUCH.md` | `testing/` 或保留根目录 | 测试指南，若常用可保留 |
| `OPENCODE_LOCAL_TEST_PLAN.md` | `testing/` | 测试计划 |
| `M4_OPENCODE_TEST_RESULTS_2026-07-04.md` | `testing/` 或 `history/` | 测试结果/历史证据 |
| `OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md` | `experiments/` | 实验计划 |
| `M1_STATUS.md` | `history/` | 历史阶段状态 |
| `M1_REVIEW.md` | `history/` | 历史审查 |
| `M1_FINAL_REVIEW.md` | `history/` | 历史审查 |
| `M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md` | `history/` | 历史交付报告 |
| `M1_M3_DELIVERY_AUDIT.md` | `history/` | 历史审计 |
| `M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md` | `history/` | 历史阶段报告 |
| `M3_GUI_CONTROL_PLANE_MVP_REPORT.md` | `history/` | 历史阶段报告 |
| `M4_REAL_CAPABILITY_OPENCODE_PLAN.md` | `history/` | 历史阶段计划 |
| `FINAL_REVIEW.md` | `history/` | 历史最终审查 |
| `NOTES.md` | `notes/` | 杂项笔记 |
| `tech-notes.md` | `notes/` | 技术笔记 |

## 5. 迁移步骤建议

### Step 1：索引先行

已完成：`docs/index.md` 已改为分类导航页。

### Step 2：迁移低风险文件

优先迁移历史报告和实验计划，因为它们通常不应被当前主流程直接引用。

建议优先移动：

```text
M1_STATUS.md
M1_REVIEW.md
M1_FINAL_REVIEW.md
M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md
M1_M3_DELIVERY_AUDIT.md
M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md
M3_GUI_CONTROL_PLANE_MVP_REPORT.md
M4_REAL_CAPABILITY_OPENCODE_PLAN.md
FINAL_REVIEW.md
OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md
NOTES.md
tech-notes.md
```

### Step 3：迁移发布和测试文档

迁移前检查 README、PRD 和发布计划中的相对链接。

建议移动：

```text
RELEASE_COMPLETION_PLAN.md
RELEASE_READINESS_REPORT.md
OPENCODE_LOCAL_TEST_PLAN.md
M4_OPENCODE_TEST_RESULTS_2026-07-04.md
```

### Step 4：暂缓迁移当前主线文档

暂时不要移动：

```text
PRD.md
PLAN.md
ARCHITECTURE.md
SECURITY_BOUNDARIES.md
TECH_STACK.md
APP_FUNCTIONS_AND_PAGES.md
release-plan-2026-07/
```

原因：这些文档现在是当前实现主线，移动会增加链接维护成本。

## 6. 命名规范

新文档建议使用以下命名：

1. 当前事实源：简短大写，例如 `PRD.md`、`ARCHITECTURE.md`。
2. 发布计划：`release-plan-YYYY-MM/`。
3. 阶段报告：`history/M1_...md` 或 `history/M4_...md`。
4. 审查报告：`reviews/<topic>_REVIEW_YYYY-MM-DD.md`。
5. 研究报告：`research/REPORT_XX_<topic>.md`。
6. 实验计划：`experiments/<topic>_EXPERIMENT_PLAN.md`。

## 7. 执行迁移前检查清单

移动文件前必须检查：

1. 是否被 `README.md` 引用。
2. 是否被 `docs/index.md` 引用。
3. 是否被 `docs/PRD.md` 引用。
4. 是否被 `release-plan-2026-07/` 引用。
5. 移动后相对链接是否需要更新。
6. 是否应该保留兼容跳转文件，避免旧链接失效。

## 8. 推荐最终目标

最终 `docs/` 根目录应控制在 8-12 个入口文件以内。历史报告、实验计划、旧发布报告都应进入子目录；当前主线文档通过 `index.md` 导航即可。
