# d-token v0.0.1 最终发布计划包（权威入口）

- 计划日期：2026-08-09（首次 2026-08-08；2026-08-09 依四份审计 + OMP 交叉评审修订基线/日期口径）
- 目标版本：**v0.0.1**（用户指定最终发布目标）
- 当前事实版本：`0.2.0-beta.1`（manifest / PRD 现行口径，见 [版本冲突](#2-版本冲突v0-0-1-vs-0-2-0-beta-1)）
- 执行基线：分支 `codex/final-release-iteration`；历史 freeze/HEAD 仅作过程事实，真正执行从 [TASKS.md](./TASKS.md) T-P0 重新冻结 clean `C_final` 开始。
- 权威输入：`docs/PRD.md`（唯一产品真理源）、`docs/d-token_AI_product_target_spec_v2.md`、`docs/PLAN.md`（唯一 Living Plan）、`docs/product/IMPLEMENTATION_STATUS.md`（代码事实快照）、`.ai-bridge/release-iteration/*`（过程输入，非权威链接）

> 本包是 **v0.0.1 的具体执行分解**，不是新的 Living Plan。`docs/PLAN.md` 仍是仓库唯一 Living Plan；本包只把 v0.0.1 收口阶段的任务、切片、验收与证据索引化。任何执行顺序变化先改本包，再同步 `docs/PLAN.md` 对应节；任何产品边界变化只改 `docs/PRD.md`。

## 1. 包结构（九文件）

| 文件 | 职责 |
|---|---|
| [README.md](./README.md) | 本文件：权威入口、版本冲突、完成定义、文档优先级、状态词 |
| [GOALS.md](./GOALS.md) | 用户目标（13 项 + 侧栏专项 + 硬边界）/ 非目标 / 不变量，全部映射 requirement |
| [REQUIREMENTS.md](./REQUIREMENTS.md) | requirement registry：全部为 v0.0.1 MUST；含 canonical pipeline、117 registry、quality/performance/fault/active 证明门 |
| [OSS_MIGRATION.md](./OSS_MIGRATION.md) | Headroom/LeanCTX/RTK/pxpipe/Codebase Memory/AiMaMi/Spleen 唯一 pin、provenance、差异化 stage 与分发边界 |
| [EXECUTION.md](./EXECUTION.md) | 细粒度 PR/worktree DAG：既有收口切片 + C0–C8 统一框架 + provenance + V-1；共享文件串行、独立 stage 并行 |
| [TASKS.md](./TASKS.md) | 可勾选任务清单：每个 requirement owner、依赖、输出与 N0–N10 验证节点 |
| [VERIFICATION.md](./VERIFICATION.md) | 统一 evidence envelope、N0→N1–N8/N10→N9、U0–U9、G0–G9、117 proof、exact Optical active、双平台/签名结论 |
| [RISKS_DECISIONS.md](./RISKS_DECISIONS.md) | 固定决策：v0.0.1、全部 MUST、单 pipeline、context-bus ownership、Optical 自动 exact active、外部证据不降级 |
| [EVIDENCE.md](./EVIDENCE.md) | 证据索引：已有 / 过期 / 待生成路径；禁止旧 artifact 冒充最终候选 |

## 2. 版本冲突：v0.0.1 vs 0.2.0-beta.1

用户指定目标版本 **v0.0.1**，但当前仓库事实版本为 `0.2.0-beta.1`：

| 事实源 | 当前值 |
|---|---|
| `code/apps/desktop/src-tauri/tauri.conf.json` | `0.2.0-beta.1` |
| `code/apps/desktop/package.json` | `0.2.0-beta.1` |
| `docs/PRD.md`（P0.0.7「v0.2.0-beta.1 发布门」及多处版本边界） | `0.2.0-beta.1` |
| `docs/PLAN.md` / `docs/product/IMPLEMENTATION_STATUS.md` / `CHANGELOG.md` | `0.2.0-beta.1` |
| 发布 workflow（tag 模式 `v*`） | 未绑定具体版本号 |
| `support/scripts/check_release_version.py` | 依赖 manifest 版本一致性 |

**处理原则**：

1. 用户已明确指定目标版本 **v0.0.1**，无需再次确认。当前 `0.2.0-beta.1` 仅是尚未执行 V-1 的事实，不构成范围降级。
2. 切片 V-1 必须在 C0–C8、既有功能/平台切片完成后一次性统一 manifest、PRD 版本门、PLAN/status/CHANGELOG/workflow/check script。
3. V-1 前不得生成最终候选；V-1 统一版本身份，PR-9/C8C 完成编排与 gate wiring，N0 才冻结 v0.0.1 `C_final`。所有真实候选证据只能在 N0 通过后生成并绑定该 `C_final`；旧版本、旧 commit 或 pre-N0 evidence 全部过期/拒绝。
4. 旧版本字符串只允许存在于 history/archive/CHANGELOG 历史条目。

## 3. 完成定义（Definition of Done）

v0.0.1 满足以下全部条件才算完成（与 `.ai-bridge` GOAL 完成定义一致，并绑定版本/候选）：

1. 本包所有 requirement 均为 **MUST**；Headroom 117 项、LeanCTX 九边界、RTK 生产 stage、Optical 至少一个 exact active route、quality/perf/fault/replay/evidence 任一未闭合都 `release_ready=false`。
2. 所有公共 surface 调用一个 d-token-owned canonical pipeline；每个 IR node 恰一个终态动作；ReadyToSend/commit/recovery receipts 不可由 adapter 伪造。
3. 117 项 tracked registry 最终 117/117 `verified`，每项具备 production dispatch、正常/错误/恢复测试、真实 evidence 与 gate；零 placeholder/missing/not-in-scope。
4. N10 U0–U9 全过：property/fuzz、fault/durability、deterministic replay、coding-task/Provider paired quality、fresh-baseline performance、Optical exact active、跨平台确定性、coverage seal。
5. 每切片独立分支/worktree/PR；每个受管 git 操作先写 append-only audit；主代理审查与必要返工后合并。
6. N0 冻结 clean `C_final`、source/registry/corpus/threshold digests；N1–N8/N10 同候选执行；N9 只消费显式 artifact manifest，旧/mtime/latest/pass-only 证据不可穿透。
7. UI 在真实 Tauri 双尺寸/双主题/双语/Reduce Motion 下通过；cc-switch Computer A→B→A（含 openclaw app type）、Agent config exact restore、CBM 五正式版（v0.9.0/v0.8.1/0.8.0/0.7.0/0.6.1，有官方二进制者逐版本真实验证、官方无资产者 source-tag 登记 + fail-closed，v0.9.1-rc.1 标注 prerelease 不计五正式版）、macOS/Windows same commit package evidence 全部闭环。
8. 正式签名发布还需 macOS Developer ID/notarization/Gatekeeper 与 Windows Authenticode；仅缺签名可生成明确未签名技术预览，但 `release_ready=false` 且不算目标完成。其余任何 MUST 缺失只能 blocked。

## 4. 文档优先级（冲突裁决顺序）

发生冲突时按以下顺序处理（与 `docs/index.md` 第 7 节一致，本包插入其与 PLAN 之间）：

```text
实际源码与当前测试证据
→ docs/PRD.md（唯一产品真理源）
→ docs/PLAN.md（唯一 Living Plan）
→ docs/releases/v0.0.1/（本包：v0.0.1 具体执行分解）
→ docs/product/IMPLEMENTATION_STATUS.md
→ ARCHITECTURE / TECH_STACK / SECURITY_BOUNDARIES
→ 当前专题文档（optical/、testing/、research/）
→ history/ → archive/
```

- 本包与 PRD 冲突：以 PRD 为准；本包与 PLAN 冲突：以 PLAN 为准（本包必须同步修正）。
- 本包只描述 v0.0.1 收口；v0.2.0-beta.1 期间的历史结论不得直接带入 v0.0.1 证据（见 [EVIDENCE.md](./EVIDENCE.md#2-过期证据隔离清单)）。
- `.ai-bridge/release-iteration/` 是过程输入（蓝图、目标、任务映射、审计），**不作为权威链接**；本包不链接 `.ai-bridge` 路径。

## 5. 状态词（全包统一语义）

| 词 | 含义 |
|---|---|
| MUST | 本包所有 requirement 的唯一 release claim；任一缺失即 blocked |
| verified | 有当前 `C_final` + source/registry/corpus digests 的真实运行 evidence，且由对应 gate 消费 |
| implemented_unverified | production dispatch 存在但尚无同候选真实 evidence；仍 blocked |
| partial / unavailable / blocked | 只覆盖子集 / 无可用实现或证据 / 已明确失败；三者均不得通过 release gate |
| external | 证据需外部平台/凭据；不降低 MUST，缺失时仍 blocked |
| detected / configured / supported | 发现/写入/API 支持，不等于 connected/verified/healthy/active |
| claimable | Provider actual usage、定价/缓存/路线归因与 paired evidence 完整；estimated 永不自动提升 |

任何“已完成/可发布/已通过”结论必须写：版本、C_final、source/registry/corpus digests、节点/门、平台、时间与 artifact manifest。

## 6. 入口地图

- 用户目标与不变量 → [GOALS.md](./GOALS.md)
- 需求注册表（每个需求唯一 ID）→ [REQUIREMENTS.md](./REQUIREMENTS.md)
- 开源轨道与 provenance 漂移 → [OSS_MIGRATION.md](./OSS_MIGRATION.md)
- 切片 DAG 与执行规则 → [EXECUTION.md](./EXECUTION.md)
- 勾选式任务清单 → [TASKS.md](./TASKS.md)
- 验证与门禁 → [VERIFICATION.md](./VERIFICATION.md)
- 决策与风险 → [RISKS_DECISIONS.md](./RISKS_DECISIONS.md)
- 证据路径索引 → [EVIDENCE.md](./EVIDENCE.md)
