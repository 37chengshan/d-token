# d-token 并行开发提示词模板（Codex 主 agent → Agent-MCP → AtomCode worker）

> 用法：在 Codex 主会话中粘贴下面对应场景的提示词。Codex 作为 admin 做规划/审查/汇合，
> 通过 Agent-MCP 的 `spawn_agent` 把子任务派发给 AtomCode（DeepSeek V4 Flash）执行。
> AtomCode 是 one-shot task-only worker，每个子任务独立派发，不维持 session。

## 0. 前置：Agent-MCP 对 AtomCode 的硬约束（主 agent 必须遵守）

1. **`target_cli = "atomcode"`**（或 MCP daemon 识别的对应键）
2. **`model = "deepseek-v4-flash"`** — 必须是**纯 API 名**
   - ❌ `AtomGit-deepseek-v4-flash`（带 provider 前缀 → 403）
   - ❌ `AtomGit/deepseek-v4-flash`（带 / → 403）
   - ✅ `deepseek-v4-flash`
3. **不要显式传 `provider`** — AtomCode 会用自身 `default_provider`，显式传会 403
4. **`cwd` 必填** — 指向 d-token worktree 目录（见下）
5. **AtomCode 是 one-shot** — 不支持 `steer_agent` 续接同一会话；要返工就用 `followup_task`（复用 agent 节点重新 spawn）
6. **AtomCode 不是 capable model**（`capable_model = 0`）— 只派**明确边界、可独立验证**的子任务，不要派规划/架构类需要强推理的任务
7. **binary 路径**：`~/.local/bin/atomcode`（已在 PATH）
8. **回传协议**：每个 worker 的 prompt 必须要求以 `FINAL_ANSWER: <摘要>` 结尾

## 1. 通用并行开发模板（Codex 主会话粘贴）

```
你是 d-token 项目的主编排 agent。任务：<用一句话描述本次工作目标，例如"优化 Analytics 页面的数据加载性能并同步更新对应的 Rust crate 测试">。

## 编排规则

1. 先读 AGENTS.md 和对应子目录的 per-directory AGENTS.md，理解边界。
2. 把任务拆成可独立验证的子任务，每个标注：读密集 / 写密集、涉及文件范围、验收点。
3. 判定并行度：
   - 无数据依赖 → 并行派发
   - 写同一批文件 → 串行（或按依赖分批）
   - 有依赖 → 父任务先跑，产出作 context 传给子任务
4. 用 Agent-MCP spawn_agent 派发到 AtomCode（DeepSeek V4 Flash）：
   - target_cli = "atomcode"
   - model = "deepseek-v4-flash"（纯 API 名，别带前缀，别传 provider）
   - cwd = "/Users/cc/Documents/d-token"（或对应 worktree 路径）
   - permission_mode：读密集用 "plan"，写密集用 "acceptEdits"
   - timeout_seconds = 600（默认）
5. 监控：用 list_agents / get_agent_activity 轮询；顺序依赖处用 wait_agent。
6. 汇合：每个 worker 以 FINAL_ANSWER: 回传摘要，你综合核对、识别冲突、决定返工。
7. 不合格分支 → followup_task（复用同一 agent 节点，不要 steer）。
8. 关键路径（凭据/进程/回退）必须由你（Codex）亲自审查，不交给 AtomCode。

## 拆解示例（按本次任务调整）

- 子任务 A（写密集，code/apps/desktop/src/app/components/pages/Analytics.tsx）：
  worker prompt = "优化 Analytics 页面的数据加载，用 React Suspense + concurrent loading。
  只改 Analytics.tsx 和对应 .test.tsx。跑 npx tsc --noEmit 验证。
  以 FINAL_ANSWER: <改动摘要 + 验证结果> 结尾。"
  → spawn_agent model="deepseek-v4-flash" cwd=d-token permission="acceptEdits"

- 子任务 B（写密集，code/crates/dtoken-core-api/tests/analytics_contract.rs）：
  worker prompt = "新增 Analytics 数据契约测试，覆盖空数据/部分数据/错误回退三态。
  只改 analytics_contract.rs。跑 cargo test --test analytics_contract 验证。
  以 FINAL_ANSWER: <改动摘要 + 验证结果> 结尾。"
  → spawn_agent model="deepseek-v4-flash" cwd=d-token permission="acceptEdits"

- A 和 B 写不同文件 → 并行派发
- 汇合后由你（Codex）跑整合验证：cd code && cargo test --workspace && cd apps/desktop && npm run test:run

## 安全边界（不可违反）

- 凭据/auth/token/cookie 不得进日志/配置/导出（AGENTS.md §4.6）
- 凭据只进 OS 凭据库，配置只存不透明引用（§4.7）
- 所有本地服务默认只绑定 127.0.0.1（§4.4）
- Provider/Route 禁止静默 fallback（§4.5）
- Git 操作前必须跑 python3 support/scripts/git_operation_audit.py record（§9）

现在开始：先拆解任务，列出子任务清单和并行度判定，然后派发。
```

## 2. worktree 并行开发模板（多分支同时跑）

```
你是 d-token 项目的主编排 agent。任务：<目标>。

## worktree 并行策略

本次需要同时在多个分支上工作，避免文件冲突。先创建 worktree：

1. 为每个子任务创建独立 worktree：
   ./support/scripts/wt-create.sh <branch-name> main

2. 把每个子任务派发到对应 worktree 的 AtomCode：
   spawn_agent:
     target_cli = "atomcode"
     model = "deepseek-v4-flash"
     cwd = "/Users/cc/Documents/d-token-worktrees/<branch-name>"  ← 指向 worktree
     permission_mode = "acceptEdits"
     prompt = "在当前 worktree 中执行：<子任务描述>。
              只改本 worktree 范围内的文件。
              跑 <验证命令> 验证。
              以 FINAL_ANSWER: <摘要> 结尾。"

3. 所有 worker 完成后，你（Codex）在主 worktree 汇合：
   - 逐个 worktree 跑 git diff 审查改动
   - 合并到 main（逐个 worktree 合并，解决冲突）
   - 跑整合验证
   - 清理 worktree：./support/scripts/wt-remove.sh <branch-name>

## 示例拆解

- wt-feat-ui: 改 Analytics.tsx + Overview.tsx（纯 React，不碰 Rust）
- wt-feat-test: 改 core-api/tests/（纯测试，不碰产品代码）
- wt-feat-doc: 改 docs/PLAN.md + IMPLEMENTATION_STATUS.md（纯文档）
- 三个 worktree 互不冲突 → 全部并行

现在开始：先创建 worktree，再派发，最后汇合。
```

## 3. 前后端同步开发模板（IPC 契约变更时）

```
你是 d-token 项目的主编排 agent。任务：新增/修改 Tauri IPC 命令 <command_name>。

## 前后端同步策略

IPC 契约变更必须前后端同步改，不能并行。分两阶段：

### 阶段 1：Rust 侧（由 Codex 亲自做，不派发）

1. 在 code/apps/desktop/src-tauri/src/main.rs 中：
   - 加 #[tauri::command] #[specta::specta] fn <command_name>(...)
   - 注册到 specta_commands! 宏
2. 跑 cargo check 验证 Rust 侧
3. 跑 npm run tauri:dev 重新生成 bindings.ts

### 阶段 2：前端侧（派发给 AtomCode）

spawn_agent:
  target_cli = "atomcode"
  model = "deepseek-v4-flash"
  cwd = "/Users/cc/Documents/d-token"
  permission_mode = "acceptEdits"
  prompt = "前端对接新 IPC 命令 <command_name>。

  bindings.ts 已重新生成，类型在 src/app/lib/bindings.ts。
  在 src/app/lib/native.ts 中用 spectaCommands.<commandName>() 调用。
  在对应页面组件 src/app/components/pages/<Page>.tsx 中接入。
  跑 npx tsc --noEmit 和 npm run test:run 验证。

  约束：
  - 不碰 Rust 文件（src-tauri/）
  - 不碰 bindings.ts（自动生成）
  - 遵循 AGENTS.md §6 页面边界

  以 FINAL_ANSWER: <改动摘要 + 验证结果> 结尾。"

### 阶段 3：汇合验证（由 Codex 亲自做）

1. cd code/apps/desktop && npm run test:run && npm run test:e2e
2. cd code && cargo test --workspace
3. 审查前端改动是否符合产品语义（AGENTS.md §6）

现在开始：先做阶段 1（Rust 侧），完成后派发阶段 2。
```

## 4. 纯探索/审查模板（读密集，并行多 worker）

```
你是 d-token 项目的主编排 agent。任务：审查 <范围> 的代码质量和安全边界。

## 并行审查策略

把审查范围按目录拆分，每个 worker 审查一块，并行派发：

- Worker A: code/crates/dtoken-compress/ — codec evidence 和 optical 边界
- Worker B: code/crates/dtoken-core-api/ — API surface 和 MCP gateway
- Worker C: code/apps/desktop/src-tauri/ — Tauri command 和凭据边界
- Worker D: code/apps/desktop/src/app/ — React UI 和页面边界

每个 worker 的 prompt：
  "审查 <目录> 的代码。重点检查：
   1. 凭据/auth/token 是否泄漏到日志/配置（AGENTS.md §4.6）
   2. 凭据是否只存不透明引用（§4.7）
   3. 本地服务是否只绑定 127.0.0.1（§4.4）
   4. Provider/Route 是否有静默 fallback（§4.5）
   5. 页面边界是否违反（§6，仅 src/app/）
   6. 是否有死代码/未使用 import

   只读不改。输出格式：
   - 问题清单（按严重度排序）
   - 每个问题：文件:行号 + 严重度 + 建议

   以 FINAL_ANSWER: <问题清单> 结尾。"

spawn_agent:
  target_cli = "atomcode"
  model = "deepseek-v4-flash"
  cwd = "/Users/cc/Documents/d-token"
  permission_mode = "plan"  ← 读密集，plan 模式即可

四个 worker 并行派发。汇合后你（Codex）综合四份报告，去重，按优先级排序。
```

## 5. 关键提醒（每次都要遵守）

1. **AtomCode model 永远是 `deepseek-v4-flash`**（纯名，不带前缀，不传 provider）
2. **AtomCode 是 one-shot** — 返工用 followup_task，不用 steer_agent
3. **写同一批文件的子任务必须串行** — 由你（Codex）分批，不是 MCP 的事
4. **cwd 必填** — 指向具体 worktree 或主仓库
5. **关键路径（凭据/进程/回退/发布）由 Codex 亲自做** — 不派给 AtomCode
6. **每个 worker prompt 必须要求 FINAL_ANSWER: 结尾**
7. **Git 操作前必须审计** — python3 support/scripts/git_operation_audit.py record
8. **汇合后跑整合验证** — cargo test --workspace + npm run test:run + tsc --noEmit
