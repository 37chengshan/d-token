# d-token 产品恢复状态表

更新时间：2026-08-13
状态规则：仅允许 `PASS / FAIL / UNKNOWN / BLOCKED / FROZEN`

本文件不记录历史完成百分比，不继承旧 44/65、49+12 节点或旧报告中的状态。每个状态只反映当前源码与当前验证事实。`UNKNOWN` 表示尚未完成事实核对；`BLOCKED` 表示已明确知道需要什么证据/实现，但当前被环境、外部依赖、平台或前置 gate 阻止，必须写清 blocker 和解除条件；二者不得混用。

## 1. 当前事实

| ID | 项目 | 状态 | 当前证据 | 下一步 |
|---|---|---|---|---|
| R0-1 | 文档权威收敛 | PASS | 旧 release DAG / 蓝图 / 审计已移入 `archive/2026-08-12-pre-recovery/`；当前只保留 README / PRODUCT_CONTRACT / PLAN / TASKS；R0.1 产物 `CURRENT_DOCUMENT_FACTS.md` 已生成 | 完成最小基线验证 |
| R0-1B | 活动合同一致性 | PASS | `AGENTS.md`、PRD P0.0.10 冲突消解、`IMPLEMENTATION_STATUS` 恢复警告、`.ai-bridge/current-plan.md`、v0.0.1 README FROZEN 状态已同步；旧 whole-batch/chooser/二次确认口径不再作为活动执行指令 | R0 结束时再做一次 active-doc scan |
| R0-2 | 最小基线测试 | PASS | 2026-08-13 R0 实测：`check_structure.py` PASS；`cargo test -p dtoken-config-patcher` 138 passed/0 failed；`npm run test:run -- Onboarding Install` 137 passed；`npx tsc --noEmit` exit 0 | 已记录；进入 R1 前无需重跑 |
| R0-3 | IMPLEMENTATION_STATUS 当前化 | UNKNOWN | 主体仍是 2026-08-07 快照，顶部恢复模式警告已生效（不误导）；R0 报告已列出需刷新的数字项（测试数、G 门状态） | R1 首个切片前由恢复流程刷新 |
| R0-4 | 唯一可写实施工作区 | PASS | 2026-08-13 冻结：唯一可写 workspace = `~/Documents/d-token`（branch `codex/final-release-iteration`，HEAD `8ca86e7d`）；受保护 dirty/untracked 清单见 `WORKSPACE_STATE.md`；55+ 历史 worktree 全部 FROZEN | 每个切片开工前核对 |
| GP-A1 | Onboarding 五段 | PASS | `Onboarding.tsx:32` `STEP_KEYS = [welcome, service, agents, config, verify]`，upstream 信息并入 config 步骤，独立 upstream/verify 步骤已删除 | — |
| GP-A2 | 服务启动阶段反馈 | PASS | `StepProgress` 已接入 service 步骤（`Onboarding.tsx` service case），展示「启动本地服务 / 确认运行状态」两阶段；`StepProgress.tsx` 新增 `reducedMotion` 静态等价（无 scale 动画/spinner）；真实 Tauri 验收：阶段文本渲染 + 服务启动后进度完成态 | — |
| GP-A3 | Onboarding 顺序门 | PASS | `canEnterStep`（Onboarding.tsx）：agents 需服务真实运行、config 需服务+discovery 完成、verify 需 configStarted（用户已开始一键配置）；stepper 与「下一步」共用同一门；真实 Tauri 验收：服务未运行时 3/4/5 锁定、点击「下一步」不跳步、服务启动后 agents/config 解锁、verify 在未点配置前点击无效 | — |
| GP-A4 | service / configured / connected / verified 分离 | PASS | verify 步骤三证据组（检测结果/接入声明/验证证据）依赖真实 snapshot/runtime 状态推导（`evidenceState` + `agent_route_follow.state === applied_verified`）；真实 Tauri 验收覆盖诚实 unavailable/conflict 状态（本机 24 上游候选 + 来源冲突 → 一键配置 disabled） | R1.A/R2.A 已完成；R3 八页验收再全量复核 |
| GP-B1 | 每 Agent 独立事务 | FAIL | `installMachine.ts:runInstallAgentBatch`（约 316-337 行）失败后对全部 receipts 逆序调用 `operations.rollback`；`Install.tsx:495-501` catch 中同样对 `appliedAgentReceipts` 逆序 `rollback_hybrid_agent_integration`（含 managed transports + openCode + route rollback，502-539 行）—— whole-batch reverse rollback 双重存在 | R1.B 红测试 → R2.B 移除整批回滚 |
| GP-B2 | 一次点击直接配置，无二次普通确认 | FAIL | `Install.tsx` 存在 `OPEN_REVIEW` → `review` phase（installMachine.ts:158-160）与「请查看当前变更后再应用」通知（Install.tsx:309），即第二层确认路径仍在主流程；Onboarding config 步骤 `onStartSetup()` 进入该工作台 | R1.B 建行为测试钉住边界 |
| GP-B3 | Agent 默认勾选、可取消、未安装禁用 | FAIL | **Onboarding 侧已闭环（R2.A 增补）**：`Onboarding.tsx` agents 步骤渲染 `checkbox`（默认勾选、`canJoinInstallBatch` 控制 disabled、取消写入 `installMachine` store 并保留跨轮询）；`installMachine.ts` 新增 `TOGGLE_AGENT`，`DISCOVERED` 保留用户取消（revision 未变）；守卫测试已更新为「Agent 级选择放行 / provider 级 chooser 仍禁」；真实 Tauri 验收：13 个 Agent checkbox 渲染、默认 8 个勾选、取消 OpenCode 后 config 计数 8→7。**剩余冲突**：`Install.tsx` 工作台页内仍无选择 UI（守卫测试仍禁 Checkbox on Install.tsx），P0.0.10.1 要求安装页也可取消 | R2.B 在 Install 页补选择 UI |
| GP-C1 | cc-switch 授权后自动跟随 | FAIL | `upstream_discovery.rs:298,598,653,3171` `automatic_apply_allowed: false` 硬编码；`main.rs:stage_agent_auto_follow_authorization`（6918 行）`enabled: false` 硬编码；前端类型 `connectionSetup.tsx:128,138` 固定 false；测试还断言 `!automatic_apply_allowed`（upstream_discovery.rs:3413,3427） | R1.C 红测试 → R2.C 收敛 |
| GP-C2 | cc-switch 永远只读 | UNKNOWN | 静态核对：`cc_switch.rs` 全部函数只读（read/snapshot/poll/hash，无 write/insert/update/exec）；`wrote_cc_switch=false` 断言需 R1.C 测试钉住 | R1.C 验证 `wrote_cc_switch=false` |
| GP-C3 | cc-switch app type 全量不丢失 | UNKNOWN | 本轮未重新做全量 app type 事实核对 | R1.C 当前 fixture + 本机只读核对 |
| GP-D1 | Codex 顶层 `openai_base_url` / `model_catalog_json` 发现 | FAIL | `upstream_discovery.rs` 与 `agent_transport.rs` 均无 `openai_base_url`/`model_catalog_json` 字面解析；Codex 写入形态仍围绕 `[model_providers]`（agent_transport.rs:8744-8814）→ 与 P0.0.10.5 顶层形态要求冲突 | R2.D 前先补 fixture 失败测试 |
| GP-D2 | Codex → d-token → ocxlocal 真实 routed flow | UNKNOWN | 无 Codex/ocxlocal 专属 routed artifact；`support/artifacts/opencode-real-routed/` 有 32 个 2026-07-12~08-09 OpenCode 历史证据，不绑定当前候选 | R2.D 真实验收 |
| R2-E | 通用 Agent 压缩层（Runtime Capability Registry + 未知模型推断） | UNKNOWN | PRD P0.0.11 已冻结（2026-08-13 用户确认）；docs/PLAN §2.5 + release-iteration PLAN Task R2.E 已写；尚未实现 | R2.D 通过后实施（R2.E1 registry → E2 inference → E3 acceptance） |
| UX-1 | 八页真实 Tauri 用户任务闭环 | UNKNOWN | 旧视觉/流程审计已归档，当前候选未重新执行全矩阵真实 Tauri 验收 | R3 |
| REL-1 | Canonical / receipt / C0-C8 | FROZEN | Golden Path 未稳定前禁止继续扩张 | G-R2A/B/C + G-R3 后重新规划 |
| REL-2 | CBM 五版本 / 117 能力 | FROZEN | 同上 | R4 重新生成 lane |
| REL-3 | Optical active / paired eval / perf | FROZEN | 同上 | R4 重新生成 lane |
| REL-4 | 双平台最终 release candidate | FROZEN | 同上 | R5 |

## 2. 当前真正阻断用户使用的优先级

当前保留以下 5 个确定 FAIL 作为阻断（GP-A1/A2/A3 已于 R2.A 收敛为 PASS）：

1. `GP-B1` 一个 Agent 失败会回滚其他已成功 Agent（双重 whole-batch rollback）。
2. `GP-B2` 主流程仍存在第二层 review 确认（OPEN_REVIEW → 请查看变更后再应用）。
3. `GP-B3` **部分解决**：Onboarding 已可取消 Agent（真实验收通过）；`Install.tsx` 工作台页内选择 UI 仍缺失，留 R2.B。
4. `GP-C1` cc-switch 仍停留在 review-gated / `automatic_apply_allowed=false` 旧合同（Rust 硬编码 false + `enabled:false` + 前端固定 false）。
5. `GP-D1` Codex 顶层 `openai_base_url` / `model_catalog_json` 形态无发现/写入路径。

其他问题保持 `UNKNOWN`，必须通过当前代码与测试再升级，不得因为历史报告写过就直接列成 Blocker。

## 2.1 合同冲突记录（用户规则：发现冲突必须记录，不自行选择）

| ID | 冲突位置 | 差异 | 当前采用 | 待决策 |
|---|---|---|---|---|
| C1 | `cc_switch.rs:118 follow_decision` vs PRODUCT_CONTRACT §6 / PRD P0.0.10 | 代码：upstream identity（provider+source+protocol+endpoint）变化 → `ReauthorizationRequired`；合同：同一已授权 app 的 Provider/model/protocol/endpoint/profile 变化后自动更新，不再弹第二次确认 | 代码 fail-closed（保守） | R2.C 前用户定夺：provider/endpoint 变化是否允许免确认自动跟随 |
| C2 | `selection-controls.test.ts`（禁止 Checkbox/onToggle）vs PRD P0.0.10.1（用户可以取消） | **已解决（R2.A 增补）**：守卫测试更新为「Agent 级 Checkbox/TOGGLE_AGENT 放行、provider 级 `<select>`/chooser 仍禁」；Onboarding 已实现可取消；Install.tsx 页内选择 UI 留 R2.B | — |
| C3 | `Onboarding.tsx` 六步 vs 五段合同 | 独立 upstream/verify 步骤 | 合同五段 | R2.A 收敛 |
| C4 | whole-batch rollback vs per-Agent 事务 | 见 GP-B1 | 合同 per-Agent | R2.B 移除 |
| C5 | `automatic_apply_allowed=false` / `enabled:false` vs 自动跟随 | 见 GP-C1 | 合同自动跟随 | R2.C 收敛 |

## 3. 下一执行项

严格顺序：

```text
R0.3 最小基线验证
→ R1.A Onboarding 红测试
→ R2.A Onboarding 修复并真实 Tauri 验收
→ R1.B 配置独立事务红测试
→ R2.B 配置语义修复
→ R1.C cc-switch 红测试
→ R2.C cc-switch 自动跟随修复
→ R2.D Codex/OpenCodex
→ R3 八页用户任务验收
```

前一项没有达到对应 gate，下一项不得开始实施。

> R2.A 完成（2026-08-13）：Onboarding 五段收敛 + 真实门禁 + StepProgress 阶段展示 + reducedMotion 静态等价；66 targeted 测试 + 全量 480 前端测试 + tsc 通过；真实 Tauri 首次流程验收完成（五段 stepper、门禁锁定/解锁、真实服务启动、上游信息合并 config、来源冲突 fail-closed）。下一项：R1.B 红测试（per-Agent 事务隔离 + 取消交互边界 C2）。

## 4. 状态变更要求

从 `FAIL/UNKNOWN` 改为 `PASS` 时必须在“当前证据”栏补充：

- 当前源码路径；
- 回归测试名/路径；
- 实际测试命令与结果；
- 如果需要真实 App / route，附真实验收证据位置。

只修改文档、计划、类型、fixture 或 mock，不允许改成 PASS。

## 5. 冻结 backlog 的处理

旧 release DAG、C0-C8、CBM、Optical、117、签名/发布问题仍然保留在归档材料中，不删除需求。它们现在只是 `FROZEN`，不是“取消”。

恢复条件：`G-R2A + G-R2B + G-R2C + G-R2D + G-R3` 全 PASS 后，从当前源码重新生成最多 4 条 release lane；不恢复旧任务编号和旧完成率。R2.D 的真实 Codex/OpenCodex routed-flow 不得作为“以后 release 再补”的事项。
