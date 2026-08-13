# d-token 执行计划（Living Plan）

状态：ACTIVE — 产品恢复模式
更新时间：2026-08-13

维护规则：本文件是仓库唯一正式 Living Plan。产品边界以 `docs/PRD.md` 为准；当前代码事实以源码、当前测试和 `docs/product/IMPLEMENTATION_STATUS.md` 为准。`.ai-bridge/release-iteration/PLAN.md` 是本轮 Agent 的详细执行分解，不替代本文件。

## 0. 当前结论

当前不继续推进旧的“大型发布 DAG”。原因不是发布需求取消，而是核心用户流程仍存在已经被多轮报告指出但当前源码尚未收敛的冲突；继续同时推进 canonical / Optical / CBM / 117 / release evidence 会扩大状态面和返工成本。

当前进入产品恢复模式：

```text
先稳定首次使用
→ 再稳定一键配置
→ 再稳定日常运行与 cc-switch 跟随
→ 再做八页真实用户任务验收
→ 最后从当前事实重新生成发布 lane
```

旧计划已保存为：

`docs/archive/release/2026-08-12-PLAN-pre-product-recovery.md`

旧 `.ai-bridge/release-iteration` 蓝图、审计与 release DAG 已进入其 `archive/2026-08-12-pre-recovery/`，只用于追溯。

### 0.1 合同一致性门（先于业务修复）

恢复计划本身不能成为另一套真理源。任何 Agent 开始 R1/R2 前必须先确认以下入口没有互相冲突：`AGENTS.md`、`docs/PRD.md` P0.0.10、本文件、`docs/product/IMPLEMENTATION_STATUS.md` 顶部恢复标记、`.ai-bridge/current-plan.md`、`docs/releases/v0.0.1/README.md`。发现“再次确认 / Provider chooser / 自动纳入不可取消 / whole-batch rollback / 旧 release DAG 可执行”等旧口径时，先修活动文档再改业务代码。

R0 的完成门必须包含一次“活动合同扫描”：旧语义只允许出现在明确的 archive/history 或带 FROZEN 警告的历史工作集中，不能作为当前执行指令。

## 1. 当前最高优先级：三条 Golden Path

### GP-A 首次启动与首次配置

目标流程固定为五段：

```text
欢迎简介
→ 启动本地服务（真实进度/反馈）
→ 扫描已安装 Agent
→ 用户调整默认勾选并一次点击“配置”
→ 开始使用
```

当前已确认缺口：

- `Onboarding.tsx` 仍保留 `welcome/service/agents/upstream/config/verify` 六步。
- service 启动尚未接入已有 `StepProgress` 或等价真实阶段反馈。
- 当前 stepper 可直接跳到任意步骤，关键前置流程没有严格 gate。

完成标准：

- 行为先由失败回归测试锁定。
- 五段流程当前源码成立。
- service running 与 configured / connected / verified 状态分离。
- Reduce Motion 有静态等价。
- 当前候选真实 Tauri 首次流程通过。

### GP-B Agent 一键配置

`docs/PRD.md` P0.0.10 是当前最高优先级自动配置合同：

- 已安装 Agent 默认勾选，用户可以取消；未安装不可勾选。
- 用户点击一次“配置”即构成本次明确授权。
- 后台执行 plan / redacted diff / backup / apply / readback verify / receipt。
- Provider/model/protocol/endpoint/cc-switch profile 不提供 chooser。
- 每个 Agent 是独立事务。

当前已确认缺口：

`Install.tsx` 当前失败路径仍会逆序遍历已成功的 `appliedAgentReceipts` 并调用 `rollback_hybrid_agent_integration`，与 P0.0.10 “失败 Agent 只回退自己，成功 Agent 保持成功”冲突。

完成标准：

- Agent A 成功、Agent B 失败时 A 仍成功。
- B 可单独回退、显示原因、单独重试。
- 已配置且未变化时只验证，不重复写。
- 普通用户不再被第二层工程确认弹窗打断。
- Route / MCP-Hook / Detection-only 结果诚实分层。
- 隔离真实配置完成 apply → readback verify → failure → retry → exact restore 验收。

### GP-C 日常运行与 cc-switch 跟随

目标：首次授权后，d-token 持续只读观察 cc-switch 当前生效配置；变化后自动更新 d-token 自有 route/runtime，不再次要求用户确认。

当前已确认缺口：

`upstream_discovery.rs` 和前端合同仍存在 `automatic_apply_allowed=false` 的旧 review-gated 语义。

完成标准：

- `wrote_cc_switch=false` 永久成立。
- A→B→A 变化能自动更新 d-token route。
- 每次同步有 backup / verify / receipt / Event Ledger。
- 同步失败保留 last-known-good。
- cc-switch 实际 app type 不静默丢失。
- 本机只读 A→B→A 验收通过。

## 2. 第二优先级：Codex / OpenCodex 真实链路

在 GP-A/B/C 收敛之后闭合：

```text
Codex
→ d-token listener
→ ocxlocal / OpenCodex
→ 当前动态模型上游
```

要求：

- 支持真实顶层 `openai_base_url` + `model_catalog_json` 形态。
- 保持 Responses / dynamic model 语义。
- 不读取或输出用户 auth/token。
- 同一候选产生真实 route decision、runtime identity、request / ledger evidence。
- 验收后用户配置精确恢复。

没有真实 routed-flow 证据前不能把 Codex 标为已验收。

## 2.5 第三优先级：通用 Agent 压缩层（R2.E，用户 2026-08-13 确认）

产品合同：`docs/PRD.md` P0.0.11（Runtime Discovery & Adaptive Compatibility）。

目标：实现「一次配置，新增模型 / Provider / route 无需重新配置」，作为「通用 Agent 压缩层」完成门。

- Runtime Capability Registry（按 model identity 记录 context_window / protocol / tool_call / stream / compression strategy / fallback），小版本与能力变化重新评估；不是固定白名单。
- 未知模型禁止失败：推断协议 → 默认压缩策略 → 记录 unknown → 学习更新；route 无法唯一解析仍 fail-closed。
- 不写死模型列表；新模型不要求重新发布或重新配置。
- 与 R2.C（cc-switch 跟随）、R2.D（Codex 动态透传）衔接；压缩策略复用统一压缩框架。

完成标准：一次配置后新增模型 A / Provider B / route C，下一次真实请求自动识别 model → 匹配 upstream → 选择 compression → 生成含 identity/capability/strategy/route/ledger 的完整 receipt。

执行位置：R2.D 之后、R3 之前（R3 八页验收需依赖本层真实状态）。

## 3. 第三优先级：八页用户任务验收

固定八页：

1. 总体概览
2. 安装与连接
3. 代码图谱（实验性）
4. 分析
5. 视觉压缩（Beta）
6. Doctor
7. 变更与回退
8. 设置

每页必须回答：

- 用户为什么进入此页；
- 第一主动作是什么；
- 成功后发生什么；
- 失败后如何继续。

验收：

- 1180×780 与 920×640。
- 中文 / 英文。
- Reduce Motion。
- 真实 Tauri 点击链，而非仅浏览器 preview。
- 无空白死循环、无效按钮、关键横向滚动、工程术语主导普通流程。
- stale / partial / unavailable / unknown 状态诚实。

## 4. 暂停的发布 backlog

以下需求没有取消，只是当前 `FROZEN`：

- canonical pipeline / sealed receipt / SendCommitReceipt；
- compression / CCR / quality / performance；
- Optical exact active / paired eval；
- CBM 五版本与兼容矩阵；
- 117 capability proof；
- 双平台最终证据与 packaging；
- 完整 release candidate / gate aggregation。

恢复条件：GP-A、GP-B、GP-C、Codex/OpenCodex 真实 routed-flow（R2.D）与八页用户任务验收均无 Blocker/High。R2.D 不是 release backlog 项，必须在重新开放 C0–C8/CBM/Optical 等扩张工作前闭环。

恢复时不复用旧 49+12 节点，也不复用旧 44/65 或任何历史完成百分比。根据当时当前源码重新生成最多四条 lane：

1. Runtime / canonical / receipt correctness。
2. Compression / CCR / Optical / quality/perf。
3. Agent / CBM / platform capability closure。
4. Release candidate / packaging / evidence。

## 5. 实施纪律

### 5.1 一次只修一个纵向用户切片

核心用户状态流同时只允许一个实现切片进入修改阶段。只读调查可以并行，但不得创建第二套计划、第二套状态机或第二份事实账本。R0 必须先冻结本轮唯一可写实施工作区和受保护 dirty 路径；pre-recovery 旧 worktree 默认不可信，不得因为名称含 final/release 就直接续跑。

### 5.2 TDD 优先

同一问题再次出现时：

```text
先写失败回归测试
→ 运行并确认 FAIL
→ 最小实现
→ targeted verification
→ 真实行为验收
```

不能再次只写审计报告。

### 5.3 PASS 的唯一标准

必须同时满足：

1. 当前源码实现；
2. 回归测试存在；
3. 当前候选测试实际通过；
4. 需要真实 App/runtime/routed flow 的行为有真实证据。

类型、fixture、mock、静态页面、历史报告、Agent 自述都不能单独构成 PASS。

### 5.4 状态报告不用百分比

当前进度只报告：

```text
Golden Path gates: x/4 PASS
Active Blocker: n
Active High: n
Unknown requiring verification: n
Frozen backlog: n
```

活动 Blocker/High 保持不超过 12 项；与当前 Golden Path 无直接关系的新发现进入 FROZEN backlog，不立即扩大实施范围。

## 6. 当前执行顺序

```text
R0 重新建立当前事实 + 最小基线验证
→ R1.A Onboarding 行为红测试
→ R2.A Onboarding 五段修复 + 真实 Tauri
→ R1.B 一键配置独立事务红测试
→ R2.B 配置事务收敛
→ R1.C cc-switch 自动跟随红测试
→ R2.C cc-switch 收敛 + 本机 A→B→A
→ R2.D Codex/OpenCodex 真实 routed
→ R2.E 通用 Agent 压缩层（Runtime Registry + 未知模型推断）
→ R3 八页用户任务验收
→ R4 根据当前事实重新规划发布 lane
→ R5 最终候选
```

详细切片、文件范围与门禁见：

- `.ai-bridge/release-iteration/PRODUCT_CONTRACT.md`
- `.ai-bridge/release-iteration/PLAN.md`
- `.ai-bridge/release-iteration/TASKS.md`

## 7. 当前基础验证

仓库级基础检查仍保留：

```bash
python3 support/scripts/check_structure.py

cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
```

恢复阶段每个切片先跑 targeted verification；只有进入候选冻结时才要求全量门全部绑定同一 candidate/source digest。

## 8. 文档权威与归档

当前权威顺序：

```text
用户最新明确要求
→ docs/PRD.md 最新高优先级条款
→ docs/PLAN.md
→ 当前源码与当前测试事实 / IMPLEMENTATION_STATUS
→ 当前专题文档
→ history/
→ archive/
```

归档文件不删除历史信息，也不代表需求取消；它们只是退出当前执行线。

`docs/releases/v0.0.1/` 当前保留原位置，因为其中存在尚未吸收的本地修改；在产品恢复门通过前，它不是当前执行入口，不得覆盖本 Living Plan。后续进入 R4/R5 时再根据当前事实统一重写或归档。
