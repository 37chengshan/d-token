# d-token 文档组织规范

更新时间：2026-07-11 晚间

## 1. 文档层级

仓库文档分为四个层级：

1. **产品契约**：`docs/PRD.md`，唯一产品真理源。
2. **执行与代码事实**：`docs/PLAN.md`、`docs/product/IMPLEMENTATION_STATUS.md`。
3. **长期规范与当前专题**：架构、技术栈、安全、页面、测试、Optical 当前规范；设计要求直接进入唯一 PRD。
4. **历史与归档**：`docs/history/` 和 `docs/archive/`，仅用于追溯。

历史审查、旧发布报告、测试结果和 Agent 会话记录不得覆盖当前产品契约或当前源码证据。

## 2. `docs/` 根目录

根目录只保留长期入口和稳定事实源：

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

阶段报告、实验版本、专题审查、旧发布计划和零散 notes 禁止重新放入 `docs/` 根目录。

## 3. 当前目录职责

```text
docs/
├── index.md          唯一导航入口
├── product/          当前产品实现状态
├── testing/          当前测试边界和验收计划
├── experiments/      仍在执行的实验计划
├── optical/          Optical 当前架构、状态、测试和发布门槛
├── research/         仍有效的外部参考，不是产品事实源
├── history/          里程碑、阶段交付和历史审查证据
└── archive/          已退出执行线的旧计划、旧矩阵、旧研究和过程文件
```

`release/`、`reviews/`、`notes/` 目录不再作为当前文档分类使用；其中旧内容已于 2026-07-11 归入 `archive/`。

## 4. 当前有效文档

### 4.1 当前事实源

- `docs/PRD.md`
- `docs/PLAN.md`
- `docs/product/IMPLEMENTATION_STATUS.md`
- `docs/ARCHITECTURE.md`
- `docs/TECH_STACK.md`
- `docs/SECURITY_BOUNDARIES.md`
- `docs/APP_FUNCTIONS_AND_PAGES.md`
- `docs/PRD.md` 第 12.5 节（唯一 UI 设计产品契约）

### 4.2 当前专题

- `docs/testing/TESTING_NO_TOUCH.md`
- `docs/testing/OPENCODE_LOCAL_TEST_PLAN.md`
- `docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`
- `docs/optical/README.md`
- `docs/optical/implementation-status.md`
- `docs/optical/test-matrix.md`
- `docs/optical/release-gates.md`
- `docs/optical/review-round-5.md`

## 5. 历史与归档的区别

`history/` 保存正式里程碑、交付报告和当时的审查证据。文件通常与 M1–M4 或具体日期绑定，正文保持历史原貌。

`archive/` 保存已经被替代、与现架构冲突、仅为过程记录或不应继续出现在活动目录中的材料，例如：

- 旧 release plan 和 readiness 报告。
- 旧 UI/runtime capability matrix。
- Optical V6 和早期审查。
- 已过期研究报告和技术 notes。
- Agent 临时 `task_plan/findings/progress`。
- support 根目录的占位和重复说明。

两者均不是当前实现依据。

## 6. 代码文档规则

1. `README.md` 必须描述实际代码，不得继续使用“占位”“skeleton”“M1 当前结构”等过期表述。
2. `code/apps/desktop/` 是唯一正式桌面入口。
3. `code/UI design for project/` 是设计原型，`code/apps/desktop/src-old/` 是旧实现；两者不得被文档描述为生产入口。
4. `code/runtimes/headroom/` 只表示 Headroom-derived provenance/迁移边界，不是 Python runtime 或外部 Headroom 依赖。
5. 尚未实现的目录必须明确标为预留，不得列为已交付能力。

## 7. 新文档放置规则

- 产品边界变化：直接修改 `docs/PRD.md`，禁止创建 PRD v2/v3。
- 执行顺序变化：修改 `docs/PLAN.md`。
- 代码事实变化：修改 `docs/product/IMPLEMENTATION_STATUS.md` 和对应代码 README。
- 当前专题规范：放入对应专题目录。
- 里程碑交付报告：放入 `docs/history/`，文件名带阶段或日期。
- 已被替代的计划/报告：移动到 `docs/archive/`，不要删除追溯证据。
- Agent 临时工作文件：使用 `.ai-bridge/` 或任务结束后归档，不得长期留在仓库根目录。

## 8. 链接和状态规则

1. `docs/index.md` 必须同步反映活动文档和归档入口。
2. 当前文档不得链接到已经移动的旧路径。
3. 历史/归档正文中的旧路径可以保留，但必须由上级索引明确标记为历史语境。
4. 任何“已完成、可发布、已通过”的结论必须注明日期、候选 commit 和验证范围；没有当前证据时只能写“实现存在”或“待验证”。
5. 测试数量易变化，活动入口文档优先引用验证命令，不长期硬编码旧计数。

## 9. 变更检查

文档新增、移动或删除后至少执行：

```bash
python3 support/scripts/check_structure.py
```

并检查：

1. 根目录是否只保留项目入口。
2. 是否仍只有一个 `docs/PRD.md`。
3. 活动文档是否引用已归档旧路径。
4. Markdown 相对链接是否存在。
5. README 是否与 Cargo workspace、Tauri invoke、React 导航和实际目录一致。
