# d-token v0.0.1 最终发布计划包（权威入口）

- 计划日期：2026-08-08
- 目标版本：**v0.0.1**（用户指定最终发布目标）
- 当前事实版本：`0.2.0-beta.1`（manifest / PRD 现行口径，见 [版本冲突](#2-版本冲突v0-0-1-vs-0-2-0-beta-1)）
- 执行基线：分支 `codex/final-release-iteration`（P0 freeze 时点 `1ddb30e`；截至 2026-08-08 工作树 HEAD 已推进，见 [EXECUTION.md](./EXECUTION.md#1-执行起点事实)）
- 权威输入：`docs/PRD.md`（唯一产品真理源）、`docs/d-token_AI_product_target_spec_v2.md`、`docs/PLAN.md`（唯一 Living Plan）、`docs/product/IMPLEMENTATION_STATUS.md`（代码事实快照）、`.ai-bridge/release-iteration/*`（过程输入，非权威链接）

> 本包是 **v0.0.1 的具体执行分解**，不是新的 Living Plan。`docs/PLAN.md` 仍是仓库唯一 Living Plan；本包只把 v0.0.1 收口阶段的任务、切片、验收与证据索引化。任何执行顺序变化先改本包，再同步 `docs/PLAN.md` 对应节；任何产品边界变化只改 `docs/PRD.md`。

## 1. 包结构（九文件）

| 文件 | 职责 |
|---|---|
| [README.md](./README.md) | 本文件：权威入口、版本冲突、完成定义、文档优先级、状态词 |
| [GOALS.md](./GOALS.md) | 用户目标（13 项 + 侧栏专项 + 硬边界）/ 非目标 / 不变量，全部映射 requirement |
| [REQUIREMENTS.md](./REQUIREMENTS.md) | requirement registry：VERSION/PRODUCT/RUNTIME/AGENT/CONFIG/CONTEXT/OPTICAL/OSS/UI/SECURITY/PLATFORM/EVIDENCE/DOCS；每项 current/gap/owner/acceptance/release claim |
| [OSS_MIGRATION.md](./OSS_MIGRATION.md) | 七条开源轨道（Headroom/lean-ctx/RTK/pxpipe/Codebase Memory/AiMaMi/Spleen）pin/license/NOTICE/本地副本/分发边界/current/gap + provenance 漂移任务 |
| [EXECUTION.md](./EXECUTION.md) | 细粒度 PR/worktree DAG：在途 P0/PR-1..9 + 新增 V-1/H/L/O/CBM/R/A 切片；每切片 files/forbidden/tests/DoD/rollback/deps |
| [TASKS.md](./TASKS.md) | 可勾选任务清单：任务/状态/依赖/owner/output，与 requirement 一一对应 |
| [VERIFICATION.md](./VERIFICATION.md) | N0 候选门 + N1–N9 真实验证、G0–G9 门禁、两个 Agent relay、Optical/RTK、Windows/macOS、签名三结论、same commit/digest |
| [RISKS_DECISIONS.md](./RISKS_DECISIONS.md) | 关键决策记录与风险登记（版本冲突、完整迁移纳入范围、外部签名、Optical active、LeanCTX storage、公开 prerelease、no-touch 等） |
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

**处理原则（本任务不直接改版本）**：

1. 本计划包把版本统一列为 **VERSION-0 阻断**（见 [REQUIREMENTS.md](./REQUIREMENTS.md#version)），由执行切片 **V-1**（`version-unify`，见 [EXECUTION.md](./EXECUTION.md#v-1-version-unify)）一次性统一：manifest、PRD 版本门、PLAN/IMPLEMENTATION_STATUS/CHANGELOG、release workflow、版本检查脚本全部收敛到用户确认的 v0.0.1。
2. V-1 执行前必须由用户对「目标版本 = v0.0.1」做一次明确确认（历史版本语义、`0.2.0-beta.1` 的 prerelease 定位、GitHub public prerelease 命名都会被改写）。
3. 在 V-1 合并前，任何发布结论不得以 v0.0.1 名义落盘；在 V-1 合并后，任何证据不得再以 `0.2.0-beta.1` 名义声称有效。
4. 旧版本字符串只允许存在于历史文档与归档（`docs/history/`、`docs/archive/`、CHANGELOG 历史条目），不得存在于活跃契约。

## 3. 完成定义（Definition of Done）

v0.0.1 满足以下全部条件才算完成（与 `.ai-bridge` GOAL 完成定义一致，并绑定版本/候选）：

1. 所有用户要求均有**代码、测试、文档和真实验收证据**，或在最终发布报告中**显式标记外部平台阻断**并保持 `release_ready=false`；「代码路径存在」不构成完成。
2. 每个切片通过范围内测试并经主代理代码/UI/安全审查；每 PR 独立分支 + worktree + PR，审查通过后合并 `main`；每个受管 git 操作先 `python3 support/scripts/git_operation_audit.py record ...`。
3. UI 在 1180×780 与 920×640、浅/深色、中/英、Reduce Motion 下可读；关键交互由**真实 Tauri 窗口**（主代理 Computer 操作）确认，浏览器 preview 只作补充。
4. Agent 配置与 cc-switch 验收后**恢复原始字节/hash/权限**，不泄漏凭据；cc-switch 全程只读。
5. 最终合并候选运行 structure、Rust、前端、E2E、release gate 与配置场景验证，且全部证据绑定**同一候选 commit 与同一 source digest**。
6. 版本事实源统一为 v0.0.1（VERSION-0 闭合）。
7. 最终结论只能三选一：**正式签名发布**（G0–G8 全 passed + macOS Developer ID/notarization/Gatekeeper + Windows Authenticode + 双平台同 commit fan-in）、**未签名技术预览（明确标注）**（仓库可执行门通过 + 产物显式 `unsigned`/`unnotarized`，保持 `distribution_ready_public=false`、`release_ready=false`）、**不可发布（blocked）**。判定表见 [VERIFICATION.md](./VERIFICATION.md#6-三种发布结论判定)。
8. 更新 `docs/PRD.md`（版本门）、`docs/PLAN.md`、`docs/product/IMPLEMENTATION_STATUS.md`、README 与最终发布报告；本包作为 v0.0.1 正式计划保留在 `docs/releases/v0.0.1/`。

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
| MUST / SHOULD / MAY | PRD P0.0.1 版本语义：MUST=发布阻断，SHOULD=可记录风险后延期，MAY=不阻塞 |
| verified | 有当前候选（`C_final` + source digest）的真实运行/验收证据 |
| partial / unavailable / not_verified | 只覆盖子集 / 无可用实现或本机无法产出 / 有实现但无候选证据 |
| blocked | 证据存在但失败，或必需证据缺席；`release_ready=false` |
| external / pending | 依赖外部平台/凭据，仓库内无法证明；必须显式标注，不得冒充通过 |
| detected / configured / supported | 仅表示发现/已写配置/接口支持，**不等于** connected/verified/healthy/active |
| claimable | 仅当 Provider 计费/usage 证据与归因链完整时为 true；估算只能作 `estimated` |

任何「已完成、可发布、已通过」结论必须同时注明：日期、候选 commit、source digest、验证范围与版本号（v0.0.1 或未签名技术预览）。没有当前证据时只能写「实现存在」或「待验证」。

## 6. 入口地图

- 用户目标与不变量 → [GOALS.md](./GOALS.md)
- 需求注册表（每个需求唯一 ID）→ [REQUIREMENTS.md](./REQUIREMENTS.md)
- 开源轨道与 provenance 漂移 → [OSS_MIGRATION.md](./OSS_MIGRATION.md)
- 切片 DAG 与执行规则 → [EXECUTION.md](./EXECUTION.md)
- 勾选式任务清单 → [TASKS.md](./TASKS.md)
- 验证与门禁 → [VERIFICATION.md](./VERIFICATION.md)
- 决策与风险 → [RISKS_DECISIONS.md](./RISKS_DECISIONS.md)
- 证据路径索引 → [EVIDENCE.md](./EVIDENCE.md)
