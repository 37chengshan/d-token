# d-token 产品恢复实施计划

> **For agentic workers:** 每次只执行一个可独立验收的纵向切片。实现前先写失败回归测试；实现后必须通过 targeted verification 与真实链路门。历史 release DAG 只作追溯，不得直接恢复执行。

更新时间：2026-08-13
状态：ACTIVE

**Goal:** 先把 d-token 三条核心用户 Golden Path 收敛为稳定、可验证的产品，再从当前事实重新生成发布计划。

**Architecture:** 产品行为与状态合同先收敛，React 只消费 Rust/Tauri 的真实状态与 receipt；配置、路由、同步、验证继续由 Rust-owned runtime/config-patcher/native command 承担。恢复阶段采用“测试钉住行为 → 最小纵向修复 → 真实 App/链路验收 → 更新事实表”的串行方式，不再以大型并行 DAG 推进用户核心流程。

**Tech Stack:** Rust workspace、Tauri 2、React 18、TypeScript、Vite、Vitest、Playwright、macOS/Windows native integration。

## Global Constraints

- 产品真理源：`docs/PRD.md`，自动配置以 P0.0.10 最新合同为准。
- 当前恢复合同：`.ai-bridge/release-iteration/PRODUCT_CONTRACT.md`。
- 正式 Living Plan：`docs/PLAN.md`；本文件是恢复阶段的 Agent 详细执行计划。
- 八个一级页面 IA 不变，不重写 shell，不换技术栈。
- cc-switch 永远只读。
- Provider/model/protocol/endpoint/cc-switch profile 不提供 chooser；用户只选择 Agent。
- 每 Agent 独立事务；失败项不得回滚其他已成功 Agent。
- 任何 PASS 必须满足源码 + 回归测试 + 当前测试通过 + 必要真实证据。
- 同一缺陷第二次出现必须先写失败回归测试。
- 不覆盖当前工作树中用户/其他 Agent 的未提交修改。
- 不用历史“44/65”、旧百分比、旧报告通过数衡量进度。
- 同时最多 1 个会修改核心用户状态流的实现切片；只读调查可并行，但不得产生第二套计划。
- 活跃 Blocker/High 总数保持 ≤12；新增问题必须先判断是否影响 Golden Path，否则进入冻结 backlog。

---

## 0. 恢复后的执行模型

只保留四种文档职责：

```text
PRODUCT_CONTRACT.md  = 这一轮绝不能改错的用户行为
PLAN.md              = 怎么执行
TASKS.md             = 当前真实状态与下一项工作
archive/             = 旧蓝图、旧 DAG、旧审计、旧交接，只用于追溯
```

每个实现切片都遵循同一闭环：

```text
确认当前事实
→ 写失败回归测试
→ 运行并确认 FAIL
→ 最小实现
→ targeted tests
→ 类型/编译检查
→ 当前 diff 审查
→ 真实 App / runtime / routed-flow 验收（如该行为需要）
→ 更新 TASKS.md
→ 才允许进入下一切片
```

禁止“先写一份完成报告，再把实现留到以后”。

---

## R0 — 建立可信当前事实

### Task R0.1: 文档权威收敛

**Files:**
- Active: `.ai-bridge/release-iteration/README.md`
- Active: `.ai-bridge/release-iteration/PRODUCT_CONTRACT.md`
- Active: `.ai-bridge/release-iteration/PLAN.md`
- Active: `.ai-bridge/release-iteration/TASKS.md`
- Active: `docs/PLAN.md`
- Archive: `.ai-bridge/release-iteration/archive/2026-08-12-pre-recovery/`
- Archive: `docs/archive/release/2026-08-12-PLAN-pre-product-recovery.md`

**Produces:** 单一执行入口，不再让旧 FINAL_BLUEPRINT / UNIFIED_RELEASE_DESIGN / 旧 TASKS 作为活跃输入。

完成门：活动目录没有第二份计划或状态账本；旧材料明确标记只读追溯。

### Task R0.2: CURRENT_PRODUCT_FACTS

**Read / inspect:**
- `code/apps/desktop/src/app/components/pages/Onboarding.tsx`
- `code/apps/desktop/src/app/components/pages/Install.tsx`
- `code/apps/desktop/src/app/lib/installMachine.ts`
- `code/apps/desktop/src-tauri/src/upstream_discovery.rs`
- `code/crates/dtoken-config-patcher/src/cc_switch.rs`
- `code/crates/dtoken-config-patcher/src/agent_transport.rs`
- `code/apps/desktop/src-tauri/src/main.rs`
- 与上述路径直接关联的 tests

**Output:** 只更新 `TASKS.md`；每项只能是 `PASS / FAIL / UNKNOWN / BLOCKED / FROZEN`。

规则：
- `PASS` 必须给出当前源码与当前测试证据。
- `FAIL` 必须能指出当前代码行为与产品合同的直接冲突。
- `UNKNOWN` 表示尚未核清事实，不得自动升级为任务完成。
- `BLOCKED` 表示事实和验收要求已明确，但被环境、平台、外部依赖或前置 gate 阻止；必须写解除条件，不能用 BLOCKED 掩盖实现缺陷。
- 历史报告只用来提示检查位置，不直接决定状态。

### Task R0.3: 最小基线验证

运行：

```text
python3 support/scripts/check_structure.py
cd code && cargo test -p dtoken-config-patcher
cd code/apps/desktop && npm run test:run -- Onboarding
cd code/apps/desktop && npm run test:run -- Install
cd code/apps/desktop && npx tsc --noEmit
```

如果现有命令形态不支持测试名过滤，使用项目已有等价 targeted 命令；必须记录实际执行命令与结果。

### Task R0.4: 工作区 / worktree 所有权冻结

- 重新记录当前 branch、HEAD/基线、dirty/untracked 文件及可能来自用户/其他 Agent 的修改；不覆盖、不自动清理。
- pre-recovery 历史 worktree 一律视为非权威，不因名字含 final/release 就恢复使用。
- 核心恢复阶段同时只允许一个“可写实施工作区”。如果当前 dirty workspace 无法安全隔离，则从重新确认的当前基线新建一个 recovery worktree；不得同时开多个修改同一状态流的 worktree。
- 在 `TASKS.md` 写清当前实施工作区、受保护 dirty 路径和解除条件；任何 Agent 开工前先核对。

出口门 `G-R0`：
- 文档权威与活动合同已收敛。
- 当前事实表完成。
- 最小基线结果已记录。
- 当前唯一可写实施工作区与受保护 dirty 路径已记录。
- `IMPLEMENTATION_STATUS` 已按本轮当前事实刷新或明确 BLOCKED 原因。
- 活跃用户阻断 ≤12 项。

---

## R1 — 在每条 Golden Path 修复前先钉住行为测试

R1 只允许新增/修改行为测试和必要测试 helper；先不要大改业务实现。**R1.A / R1.B / R1.C 不作为一个“大测试阶段”一次性全部执行，而是与对应 R2 切片交错：`R1.A → R2.A → R1.B → R2.B → R1.C → R2.C`。** 文档把三组测试合同集中写在本节仅为了便于查阅，不改变实际执行顺序。

### Task R1.A: 首次启动 Golden Path 测试

**Test files:**
- `code/apps/desktop/src/app/components/pages/Onboarding.test.tsx`
- `code/apps/desktop/src/app/components/shell/AppShell.onboarding.test.tsx`
- 如需要：`code/apps/desktop/src/app/components/common/StepProgress.test.tsx`

必须新增能证明以下行为的失败测试：

1. 五段，不出现独立 upstream / verify 用户步骤。
2. service 未完成不能进入扫描后的配置阶段。
3. service 启动时存在真实阶段/indeterminate presentation；Reduce Motion 有静态等价。
4. Agent 扫描结果支持已安装默认勾选、取消勾选、未安装禁用。
5. 完成配置后进入开始使用；右上角可以重新进入 onboarding。
6. service running 不能冒充 route verified。

出口：上述失败测试在当前旧实现上至少覆盖已知问题并明确 FAIL。

### Task R1.B: 一键配置独立事务测试

**Test files:**
- `code/apps/desktop/src/app/components/pages/Install.test.tsx`
- `code/apps/desktop/src/app/lib/installMachine.test.ts`
- `code/crates/dtoken-config-patcher/src/agent_integration.rs` 直接测试或现有 integration tests

必须新增：

1. Agent A 成功、Agent B 失败时，A 保持成功。
2. B 只回退自己的未完成写入并可单独重试。
3. 一次点击后不出现第二层用户确认。
4. 未选择 Agent 不写入。
5. 已配置且无变化时只验证、不重复写。
6. Route / MCP-Hook / Detection-only 结果不互相冒充。

出口：当前 whole-batch rollback 行为被红测试钉住。

### Task R1.C: cc-switch 只读自动跟随测试

**Test files:**
- `code/crates/dtoken-config-patcher/tests/cc_switch_fixture.rs`
- `code/crates/dtoken-config-patcher/tests/cc_switch_snapshot.rs`
- `code/apps/desktop/src-tauri/src/upstream_discovery.rs` 直接测试
- 必要时 desktop connection setup tests

必须新增：

1. 授权后 A→B→A 自动更新 d-token 自有 route，不再次要求确认。
2. `wrote_cc_switch=false` 永久成立。
3. 同步失败保留 last-known-good。
4. 每次同步产生 verify / receipt / ledger evidence。
5. cc-switch 实际 app type 不静默丢失。

出口：当前 `automatic_apply_allowed=false` 的旧合同被失败测试钉住。

不存在要求 A/B/C 三组红测试全部先完成的全局 `G-R1`。每组测试只作为对应实现切片的入口门：R1.A 红测成立才进入 R2.A；R2.A 通过后才执行 R1.B；依此类推。

---

## R2 — 一次一个纵向切片修复

### Task R2.A: Onboarding 五段收敛

**Files:**
- Modify: `code/apps/desktop/src/app/components/pages/Onboarding.tsx`
- Modify: `code/apps/desktop/src/app/components/pages/Onboarding.test.tsx`
- Modify: `code/apps/desktop/src/app/components/shell/AppShell.onboarding.test.tsx`
- Reuse/Modify: `code/apps/desktop/src/app/components/common/StepProgress.tsx`
- Modify only if required: `code/apps/desktop/src/app/components/shell/AppShell.tsx`
- Modify only if real stage events are required: `code/apps/desktop/src-tauri/src/main.rs`
- Modify copy only as needed: `code/apps/desktop/src/app/lib/i18n.ts`
- Modify motion only as needed: `code/apps/desktop/src/styles/shell.css`

**Implementation contract:**
- `STEP_KEYS` 最终对应五段用户流程。
- upstream 信息并入扫描/配置，不单独成段。
- verify 作为后台/结果证据，不单独成段。
- stepper 只允许已解锁阶段；不能任意跳过 service/config 前置条件。
- `StepProgress` 或等价组件接入真实 service transaction presentation。
- Reduce Motion 下不依赖动画表达状态。

**内部再拆 3 个小切片，仍串行完成：**
1. `R2.A1 state-machine`：先只收敛五段状态机、解锁条件和 service/configured/connected/verified 语义，前端行为测试通过。
2. `R2.A2 service-progress`：再接真实 service transaction presentation / native stage evidence 与 Reduce Motion 等价，不伪造阶段。
3. `R2.A3 tauri-acceptance`：最后在真实 Tauri 走首次启动、失败重试、完成后重新进入 onboarding；A1/A2 未过不得开始 A3。

**Verification:** targeted Vitest → `tsc --noEmit` → 真实 Tauri 首次流程。

`G-R2A` 未通过前禁止进入 R2.B。

### Task R2.B: Agent 配置事务语义收敛

**Files:**
- Modify: `code/apps/desktop/src/app/components/pages/Install.tsx`
- Modify: `code/apps/desktop/src/app/lib/installMachine.ts`
- Modify related tests
- Modify as required: `code/crates/dtoken-config-patcher/src/agent_integration.rs`
- Modify as required: `code/apps/desktop/src-tauri/src/main.rs`
- Reuse existing receipt/transaction types before新增平行类型

**Implementation contract:**
- 删除 UI 层 whole-batch reverse rollback。
- 每 Agent 独立 transaction/result/retry。
- 一次点击直接执行后台 plan/backup/apply/readback/verify。
- 成功项保持成功；失败项单独回退。
- 不把全局 route/service 失败误当作“回滚所有 Agent”的理由；需要区分共享基础设施失败与单 Agent 配置失败。
- 普通 UI 只显示用户结果；技术 receipt 可展开查看。

**内部再拆 3 个小切片，仍串行完成：**
1. `R2.B1 transaction-domain`：先只修 Rust/状态机的 per-Agent transaction、失败隔离、幂等和 rollback 语义，并让领域测试通过。
2. `R2.B2 UI-orchestration`：再让 Install/Onboarding 只编排选中 Agent、展示逐项结果/重试，不在 React 重建事务状态机。
3. `R2.B3 acceptance`：最后做隔离真实配置的 success + partial failure + retry + exact restore；B1/B2 没过不得开始 B3。

**Verification:** targeted frontend tests + config-patcher tests + `tsc` + 真实隔离配置 preview/apply/verify/failure/retry/restore。

`G-R2B` 未通过前禁止进入 R2.C。

### Task R2.C: cc-switch 自动跟随收敛

**Files:**
- Modify: `code/apps/desktop/src-tauri/src/upstream_discovery.rs`
- Modify: `code/crates/dtoken-config-patcher/src/cc_switch.rs`
- Modify related fixture/snapshot tests
- Modify native route reload path only as required
- Frontend只消费同步结果，不重新实现状态机

**Implementation contract:**
- 永不写 cc-switch。
- 首次配置授权建立允许跟随的 app/identity 边界。
- 后续有效变化自动更新 d-token-owned route/runtime，无第二次用户确认。
- 每次有 snapshot/version/digest、backup、verify、receipt、ledger。
- reload 失败保留 last-known-good。
- 实际 cc-switch app type 全量核对，不静默忽略 openclaw 或未来已识别类型。

**内部再拆 3 个小切片，仍串行完成：**
1. `R2.C1 observe-contract`：只读 discovery/snapshot/app-type 覆盖与变化判定，不发生 route 写入。
2. `R2.C2 auto-follow`：在已授权 identity 上实现 d-token-owned route/runtime 更新与 last-known-good，不写 cc-switch。
3. `R2.C3 evidence-acceptance`：补齐 receipt/ledger/readback，并执行本机 A→B→A + reload failure 验收。

**Verification:** Rust unit/integration → desktop native tests → 本机只读 cc-switch A→B→A 验收。

`G-R2C` 未通过前禁止进入 R2.D。

### Task R2.D: Codex / OpenCodex 真实链路

**Files:**
- Inspect/Modify: `code/apps/desktop/src-tauri/src/upstream_discovery.rs`
- Inspect/Modify: `code/crates/dtoken-config-patcher/src/agent_transport.rs`
- Existing/new acceptance harness under `support/scripts/` / `support/tests/`

**Implementation contract:**
- 支持真实 Codex 顶层 `openai_base_url` + `model_catalog_json` 配置形态，不要求伪造 `[model_providers]`。
- Responses 协议、动态 model 透传、无重复 OpenAI auth 假设。
- 真实证明 `Codex → d-token → ocxlocal → upstream`，关联 runtime identity、route decision、request/ledger evidence。
- 验收后精确恢复用户原配置；不得打印 auth/token。

`G-R2D`：真实 routed evidence 成立才 PASS。

### Task R2.E: 通用 Agent 压缩层（Runtime Discovery & Adaptive Compatibility）

产品合同：`docs/PRD.md` P0.0.11（用户 2026-08-13 确认）。在 `G-R2D` 通过后启动，`G-R2E` 通过前禁止进入 R3。

**Files:**
- Inspect/Modify: `code/crates/dtoken-compress`（strategy registry / 默认策略出口）
- Inspect/Modify: `code/crates/dtoken-router` 或 `code/crates/dtoken-config-patcher`（Runtime Capability Registry 落点，以现有类型为准）
- Inspect/Modify: `code/crates/dtoken-event-ledger`（unknown capability / 学习更新记录）
- Modify as required: `code/apps/desktop/src-tauri/src/upstream_discovery.rs`（每请求 model 动态解析）
- 前端只消费 receipt 展示，不重建 registry

**Implementation contract（P0.0.11）：**
- Runtime Capability Registry 按 model identity 记录 `{context_window, protocol, supports_tool_call, supports_stream, compression strategy, fallback strategy}`；小版本/能力变化重新评估；注册表是运行时事实缓存，不是固定白名单。
- 一次配置后，新增模型 / Provider / 用户改配置 / cc-switch 切换 / 新 route 均不得要求重新配置或重新发布。
- 未知模型禁止失败：发现 → 读请求/config → 推断协议 → 默认压缩策略 → 记录 unknown → 学习更新；route 无法唯一解析仍 fail-closed（P0.0.9/P0.0.10），不得静默猜上游。
- 禁止写死 Claude/GPT/DeepSeek 等固定模型列表作为唯一依据。
- 每次请求产出绑定 `model identity + capability + strategy + route + ledger evidence` 的 receipt。

**内部再拆 3 个小切片，仍串行完成：**
1. `R2.E1 registry`：Capability Registry 类型、seed + 学习更新、变化重新评估，领域测试通过。
2. `R2.E2 inference`：未知模型推断 + 默认压缩策略 + fail-closed 边界 + 每请求动态解析，测试钉住「禁止失败」与「不猜上游」。
3. `R2.E3 acceptance`：一次配置后新增模型 A / Provider B / route C 无需重配；真实请求自动识别/匹配/压缩 + 完整 receipt 验收。

**Verification:** Rust unit/integration → 真实请求链路（含新增模型与 route 修改场景）→ receipt 关联 ledger。

`G-R2E`：P0.0.11 验收场景全部成立才 PASS。

---

## R3 — 八页用户任务与稳定性验收

只有 `G-R2A/B/C` 全部通过后启动；R2.D 可在不破坏前三条 Golden Path 的前提下并行准备验收 harness，但最终 routed 验收仍单独过门；R2.E 在 R3 启动前完成（R3 依赖其真实状态）。

逐页只回答四个问题：

1. 用户为什么来到这里？
2. 页面第一主动作是什么？
3. 成功后用户看到什么、下一步是什么？
4. 失败后用户如何理解并继续？

页面范围固定：

- 总体概览
- 安装与连接
- 代码图谱（实验性）
- 分析
- 视觉压缩（Beta）
- Doctor
- 变更与回退
- 设置

重点：
- 不出现空白死循环、无效按钮、工程术语主导流程。
- 920×640 与 1180×780 无关键内容横向滚动/遮挡。
- 中文/英文可读。
- Reduce Motion 可用。
- stale/partial/unavailable 状态诚实。
- 真实 Tauri 点击链路，而不是只看浏览器 preview。

修复仍遵循“回归测试 → 最小修改”，禁止在 R3 顺手重做设计系统。

出口门 `G-R3`：八页无 Blocker/High 用户流程缺陷；关键页面真实 Tauri 验收完成。

---

## R4 — 后端与发布工作重新开放

R4 当前 `FROZEN`。只有 `G-R2A + G-R2B + G-R2C + G-R2D + G-R3` 全绿后才重新规划，不复用旧 49+12 节点或 44/65 数字。Codex/OpenCodex 真实 routed-flow 是解冻条件，不允许把它拖回 release backlog 里再补。

重新开放时按当前代码事实重新生成不超过 4 条 lane：

1. Runtime / canonical / receipt correctness。
2. Compression / CCR / Optical / quality/perf。
3. Agent/CBM/platform capability closure。
4. Release candidate / packaging / evidence。

每条 lane 再拆成可独立验收的小切片；只有真正共享文件冲突时串行，不提前制造几十个分支和 worktree。

旧 `.ai-bridge/release-iteration/archive/2026-08-12-pre-recovery/PLAN-release-dag.md` 只能用来找遗漏，不允许直接恢复状态。

---

## R5 — 最终候选

R4 完成后才冻结 `C_final`：

- structure
- Rust fmt/clippy/tests
- frontend Vitest/tsc/build/E2E
- macOS/Windows 同 commit/source digest
- 关键真实 Agent routed-flow
- 真实 Tauri UX
- 配置 exact restore
- privacy/security evidence
- packaging evidence

任一关键证据缺失即 `blocked`。

---

## 状态与进度规则

不再计算“完成百分比”。只报告：

```text
Golden Path gates: x/4 PASS
Active Blocker: n
Active High: n
Unknown requiring verification: n
Frozen backlog: n
```

其中 Golden Path gates 为：

- `G-R0` 当前事实可信
- `G-R2A` 首次使用
- `G-R2B` 一键配置
- `G-R2C` cc-switch 日常跟随

R2.D、R3、R4/R5 单独报告，不混成虚假总体百分比。

## 完成前自审

每次准备将任务改成 PASS 前检查：

- 是否真的修的是当前源码，而不是只改文档？
- 原问题是否有回归测试？
- 失败路径是否覆盖？
- 是否引入第二套状态机/合同/类型？
- 用户可见状态是否来自真实 runtime/native evidence？
- 是否破坏其他 Agent 已成功事务？
- 是否把 service/configured/connected/verified 混在一起？
- 需要真实 App/真实路由的行为是否真的跑过？

任一回答为“否/不确定”，状态不能写 PASS。
