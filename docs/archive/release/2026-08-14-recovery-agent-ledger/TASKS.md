# d-token 产品恢复状态表

更新时间：2026-08-14（R2.B/C/D/E + 全项目 P0/P1 修复完成，commit 066c22e + 1b 计量闭环）
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
| GP-B1 | 每 Agent 独立事务 | PASS | `installMachine.ts:runInstallAgentBatch` per-Agent：失败仅回滚自身并 continue 其余 Agent（成功项保持 verified）；`Install.tsx` catch 只回滚共享 route（P0-6 后同步撤销成功 Agent 指向已撤销 route 的配置）；红测试 `keeps a successful Agent successful when a later Agent fails verification` 通过；前端 483/483 | — |
| GP-B2 | 一次点击直接配置，无二次普通确认 | PASS | `Install.tsx handlePrimaryAction`：ready 时 preview+apply 同 tick（ref 同步 reviews）；`预览/确认` 双按钮路径移除；测试 `binds the apply action to the reviewed revision` 验证一次点击链 | — |
| GP-B3 | Agent 默认勾选、可取消、未安装禁用 | PASS | Onboarding + Install 均渲染 Agent checkbox（`canJoinInstallBatch` 控制 disabled、默认勾选、取消写 store）；守卫测试放行 Agent 级选择、禁 provider chooser；真实 Tauri：13 Agent checkbox、默认 8 勾选、取消后计数 8→7 | — |
| GP-C1 | cc-switch 授权后自动跟随 | PASS | `cc_switch_follow_decision` app 级 DB 路径边界（同 DB 内 provider/endpoint/protocol 变化自动 `sync_required`）；`discover_upstream_candidates` 用 cc-switch 自身授权；`discover_with_auto_follow` 授权驱动；30 测试绿 | — |
| GP-C2 | cc-switch 永远只读 | PASS | `cc_switch.rs` 全函数只读；discovery `writes_performed=false`（upstream_discovery.rs:503-507,560-580）；`wrote_cc_switch=false` 断言存在 | — |
| GP-C3 | cc-switch app type 全量不丢失 | PASS | P1-1：SQL 不再硬限定 app_type；未知 app 用自身 id 作 agent_id（openclaw 等不再静默丢失）；30 测试绿 | — |
| GP-D1 | Codex 顶层 `openai_base_url` / `model_catalog_json` 发现 | PASS | `candidate_from_codex_toml` 支持顶层形态（fixture `codex-ocxlocal-top-level.toml` + 测试 `codex_toml_parser_reads_top_level_openai_base_url_and_model_catalog`）；10 codex 测试绿 | — |
| GP-D2 | Codex → d-token → ocxlocal 真实 routed flow | PASS | `support/scripts/run_codex_real_routed_test_macos.sh`（隔离 CODEX_HOME）真实 PASS：nonce 往返 + ledger 请求（support/artifacts/codex-real-routed/real-routed-20260814T014733Z-19536.json）；--dry-run 生效 + URL 脱敏 | — |
| R2-E | 通用 Agent 压缩层（Runtime Capability Registry + 未知模型推断） | PASS | `dtoken-capability-registry` crate（seed/observed/learned 三层、CAS、LRU、未知模型不失败、持久化）；capability_bridge 接入 CompressionDecision（capability_summary/compression_hint 实际应用策略）；观测回写生产接线（E3 验证 store observed/learned 落盘）；`decide_with_model`；strategy_hint 映射 | 诚实记录：Native Responses wire 路径压缩仍 Off（协议透传，见下文 UX/计量项） |
| UX-1 | 八页真实 Tauri 用户任务闭环 | UNKNOWN | 历史证据不绑定当前候选 | R3（当前候选重跑） |
| REL-1 | Canonical / receipt / C0-C8 | FROZEN | Golden Path 未稳定前禁止继续扩张 | G-R2A/B/C + G-R3 后重新规划 |
| REL-2 | CBM 五版本 / 117 能力 | FROZEN | 同上 | R4 重新生成 lane |
| REL-3 | Optical active / paired eval / perf | FROZEN | 同上 | R4 重新生成 lane |
| REL-4 | 双平台最终 release candidate | FROZEN | 同上 | R5 |

## 2. 当前真正阻断用户使用的优先级

当前无 FAIL 阻断项（R2.B/C/D/E + 全项目 P0/P1 修复已闭环）。剩余诚实 UNKNOWN/BLOCKED：

1. `UX-1` 八页真实 Tauri 验收（R3）——当前候选（066c22e+1b）未重跑全矩阵。
2. `REL-4` 双平台同 commit release evidence（R5）——Windows 侧未实测。
3. Native OpenAI Responses wire 压缩 Off（协议透传）——诚实 passthrough，非压缩成功证据。

## 2.1 合同冲突记录（用户规则：发现冲突必须记录，不自行选择）

| ID | 冲突位置 | 差异 | 当前采用 | 状态 |
|---|---|---|---|---|
| C1 | `cc_switch.rs follow_decision` vs PRODUCT_CONTRACT §6 | 代码：upstream identity 变化 → `ReauthorizationRequired`；合同：同 app 自动跟随 | **已解决（R2.C/P0-4）**：app 级 DB 路径边界，同 DB 内变化自动 `sync_required` | 已收敛 |
| C2 | `selection-controls.test.ts` vs PRD P0.0.10.1 | 守卫禁 checkbox vs 用户可取消 | **已解决**：放行 Agent 级选择、禁 provider chooser；Onboarding+Install 均可取消 | 已收敛 |
| C3 | `Onboarding.tsx` 六步 vs 五段 | 独立 upstream/verify 步骤 | 五段已实现 | 已收敛 |
| C4 | whole-batch rollback vs per-Agent 事务 | 旧实现逆序回滚 | per-Agent 已实现 | 已收敛 |
| C5 | `automatic_apply_allowed=false` vs 自动跟随 | 旧合同 | 授权驱动已实现 | 已收敛 |

## 3. 下一执行项

```text
R3 八页真实 Tauri 验收（当前候选）
→ R4 从当前事实生成 ≤4 条 release lane
→ R5 双平台同 commit 候选 + 全证据
```

> 2026-08-14：R2.B/C/D/E、R3 前置（Onboarding/Install/Codex/cc-switch）已 PASS（见第 1 节）；1b 计量闭环完成（learning 观测 + Analytics provider actual 投影 + 文档刷新）。

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
