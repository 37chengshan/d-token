# OSS_MIGRATION — v0.0.1 开源轨道与 provenance 迁移

- 原则：每个外部来源必须有**唯一 pin**（release/commit）、许可证、NOTICE、本地只读副本路径、分发边界、当前事实与缺口；任何漂移（多 pin 并存、旧 pin 未评估）都是 OSS-1 下的显式任务，不得静默保留。
- 权威登记位置：`docs/legal/THIRD_PARTY_NOTICES.md`（源码/安装包分发声明）、根 `THIRD_PARTY_NOTICES.md`、`code/runtimes/*/README.md`；本文件是 v0.0.1 的迁移与收口计划。
- 本地只读副本一律在仓库外 `/Users/cc/Documents/d-token-external/`（不提交、不复制进生产、不新增运行依赖）。
- 未实现前任何轨道状态只能 `partial`/`unavailable`；「已登记来源」不等于「已迁移」。

## 1. 轨道总览（pin 现状 2026-08-08）

| 轨道 | 上游 | 当前登记 pin | 新事实/漂移 | 许可证 | 分发边界 | 收敛切片 |
|---|---|---|---|---|---|---|
| Headroom | `headroomlabs-ai/headroom` | `01df2452`（旧基线，main HEAD 2026-08-02） | **三候选并存（pin 冲突）**：旧基线 `01df2452`、release `v0.34.0`/`9fd5ae3d`（2026-08-05 发布）、main `7f6950be`（新克隆 `headroom-main-2026-08-08`）；后两者相对旧基线的增量均未评估；历史 v0.33.0 `28aa53dc`、`1725cd1f` 只解释旧证据 | Apache-2.0 + NOTICE | 行为对照；`code/runtimes/headroom/` 为 provenance/迁移边界，非 Python sidecar/外部 8787 依赖 | PR-H1, PR-H2, PR-H3 |
| LeanCTX | `yvgude/lean-ctx` | `v3.9.17` / `cf8fce6b`（2026-08-04 release） | **pin 漂移候选**：新克隆 `lean-ctx-main-2026-08-08` HEAD `95a1b7bc`（main）未评估 | Apache-2.0（`clients/python`、`rust/crates/grammar-addons/lua` 子包 MIT） | 行为对照；九边界由 d-token-owned Rust service 提供 | PR-L0, PR-L1..L3 |
| RTK | `rtk-ai/rtk` | managed RTK（receipt 2026-07-31）；未登记 release pin | **pin 待审计**：新克隆 `rtk-v0.45.0` HEAD `b34be37`（v0.45.0，PR #3468） | Apache-2.0（已核对官方 `LICENSE`；无根 NOTICE，PR-R1 仍需登记） | d-token 内建 managed 能力；不独立分发；`rtk gain` 不作为产品轮询路径 | PR-R1, PR-R2, PR-R3 |
| pxpipe | `teamchong/pxpipe` | `0ceabbed` / v0.12.0（2026-08-06 release；法律声明、代码常量、计划文档已同步） | **已闭合（PR-O1，2026-08-08）**：v0.12.0 评估完成并固定为唯一参考 pin；旧 pin `7b7d6efa`/v0.8.0 标注为历史；1928×1928 非生产 profile 声明保留 | MIT（代码）；Spleen 字体 BSD-2-Clause | 评测配方参考；d-token Rust renderer 独立实现 | PR-O1 已闭合（PR-O2 管 Spleen） |
| Codebase Memory | `DeusData/codebase-memory-mcp` | `v0.9.1-rc.1` / `ee9833df`（manifest） | **provenance 漂移（阻断级）**：根 `THIRD_PARTY_NOTICES.md` 仍 pin `v0.9.0` / `b637e333`，与 manifest 不一致 | MIT | 可选外部 sidecar：用户显式导入 pinned archive，App 不携带大文件；Cosign/OIDC 校验 | PR-CBM1, PR-CBM2 |
| AiMaMi | `borawong/AiMaMi` | `297c7af5` | 无漂移（LICENSE 副本在 `docs/legal/AIMAMI_APACHE_2_0_LICENSE.txt`）；该 commit 无根 NOTICE | Apache-2.0 | 适配声明（PageStage/AnimatedSegmentedControl 等，见 legal notices 适配范围表）；不复制品牌资产 | PR-A |
| Spleen | `fcambus/spleen` | `2.2.0` / `0493c34`（`spleen-5x8.bdf`） | 无漂移；克隆 `spleen-2.2.0` 已核验 | BSD-2-Clause | 字体数据随 Optical 分发声明；渲染实现 d-token-owned | PR-O2 |

## 2. Headroom 轨道与 PR-H 切片

### 2.1 现状

- 评估基线候选三选一：旧基线 `01df2452`（Apache-2.0 + NOTICE）、release `v0.34.0`/`9fd5ae3d`（2026-08-05 发布）、main `7f6950be`（新克隆 `headroom-main-2026-08-08`，外部路径 `/Users/cc/Documents/d-token-external/headroom/headroom-main-2026-08-08`）；增量（行为/接口/许可证/NOTICE）均未评估。已完成 v0.33.0→旧基线迁移决策（PHP codec 吸收；#2694/#2718/#2691 契约对齐评估；frozen_message_count 缺口登记于 `code/runtimes/headroom/README.md`）。
- 迁移状态：PLAN §1C 逐能力矩阵（117 项）多数为 active candidate/partial；`partial`/`占位`/`缺失` 为发布阻断（PLAN 1C.2）。
- Headroom 完整迁移是长期发布门（PRD §8 矩阵、PLAN 1C.2 G0–G9），不等于 v0.2.0-beta.1 全部 MUST（PRD P0.0.7 只要求核心可信压缩子集）。用户口径「所有要完成都考虑」→ 完整迁移列入 v0.0.1 计划，release claim 为 BOUNDARY（见 [RISKS_DECISIONS.md](./RISKS_DECISIONS.md#d2)）。

### 2.2 PR-H 切片

| 切片 | 内容 | files（允许范围） | 验收 | 依赖 |
|---|---|---|---|---|
| PR-H1 | **pin 决议与单一事实源**：评估 release `v0.34.0`/`9fd5ae3d` 与 main `7f6950be`（`headroom-main-2026-08-08` 新克隆）相对旧基线 `01df2452` 的增量（行为/接口/许可证/NOTICE），从三候选中一次性决定评估基线；更新 `code/runtimes/headroom/README.md`、`docs/legal/THIRD_PARTY_NOTICES.md`、根 NOTICE；漂移清单清零 | `code/runtimes/headroom/README.md`、`docs/legal/THIRD_PARTY_NOTICES.md`、`THIRD_PARTY_NOTICES.md`（根）、`docs/releases/v0.0.1/OSS_MIGRATION.md`（同步） | 全仓唯一 headroom pin；评估记录落盘；不把 v0.34.0 / main 增量未评估写成完成 | P0（基线） |
| PR-H2 | **残余能力矩阵闭合**：按 PLAN 1C.1 B0→B6 顺序，把 117 项矩阵中 `占位`/`缺失`/`不安全默认` 逐项转为真实实现或显式 not-in-scope 记录；重点：MCP streamable HTTP、tool-schema 残余、跨协议、CCR crash 剩余证据 | `code/crates/`（对应 crate 与测试）、`docs/PLAN.md` §1C 矩阵同步（仅执行事实列） | 矩阵零占位/缺失（或显式记录+用户裁决）；G0–G5 相关项证据绑定候选 | PR-H1, PR-1 |
| PR-H3 | **§8/G9 证据绑定**：Headroom §8 能力逐项 runtime/eval/Agent 证据绑定 `C_final`；G9 Frontend completeness gate 只在 G0–G8 全绿后开放 | `support/scripts/`（证据脚本）、`docs/PLAN.md` §1C.2、`docs/releases/v0.0.1/VERIFICATION.md`（同步） | G9 判定表落盘；任何未绑定候选的 §8 项仍 partial | PR-H2, PR-7, PR-9 |

## 3. LeanCTX 轨道与 PR-L 切片（九边界）

### 3.1 现状

- 来源登记完成（`code/runtimes/headroom/README.md` LeanCTX 节、legal notices）：`yvgude/lean-ctx` v3.9.17 / `cf8fce6b`，Apache-2.0（子包 MIT 例外），NOTICE 存在，本地副本 `/Users/cc/Documents/d-token-external/lean-ctx/`（另有 `lean-ctx-archive`、`lean-ctx-main-2026-08-08` HEAD `95a1b7bc`）。
- d-token 差异：九条能力边界（PRD P0.0.5）必须由 d-token-owned Rust service 提供；无真实跨 Agent 证据时只能 `partial`/`unavailable`。
- PRD 版本边界：v0.3 MUST 完成 Context Bus/Session Cache/Context Relay/Map 复用/Query-aware Retrieval/Recovery Pointer/Stale 与 CCR crash recovery → v0.0.1 release claim = BOUNDARY。

### 3.2 PR-L 切片（九边界分组）

| 切片 | 覆盖边界 | 验收（每项） | 依赖 |
|---|---|---|---|
| PR-L0 | **pin 决议**：评估 main `95a1b7bc` 相对 v3.9.17 增量；决定对照基线；更新登记文档 | pin 唯一；评估记录落盘 | P0 |
| PR-L1 | Context Bus、Session Cache、同 Agent 自命中防护 | 三类边界各有 d-token-owned 实现 + 隔离/缓存命中/自命中拒绝测试；无 secret/未脱敏原文进 cache | PR-L0 |
| PR-L2 | Context Relay、Query-aware Retrieval、收益分离计量 | Relay 命中复用记录来源/命中/节省/新鲜度；Relay 避免的传输不计入压缩收益；有测试 | PR-L1 |
| PR-L3 | Recovery Pointer/Manifest、Stale/Superseded Context、SharedContext | 失效条件（文件变化/TTL/权限/来源不可信）有测试；新鲜/过期/被替代显式区分 | PR-L2 |

## 4. Optical 轨道与 PR-O 切片（pxpipe / Spleen）

| 切片 | 内容 | 验收 | 依赖 |
|---|---|---|---|
| PR-O1 | **pxpipe pin 重评估（已完成 2026-08-08）**：评估 `v0.12.0`/`0ceabbed`（2026-08-08 克隆）相对旧 pin `7b7d6efa` 的行为差异与许可证/NOTICE 变化；刷新 capability matrix（768×1932 在 v0.12.0 为 OpenAI 兜底 profile、Sol/Grok 原生 14px JetBrains Mono 84 列条带、Spleen 5×8、reflow/newline marker、IDS、factsheet、recent-six）；旧 pin 结论标注为历史 | 已完成：新旧 pin 对比记录（80 commits；LICENSE 未变、上游无 NOTICE）；法律声明（`docs/legal/THIRD_PARTY_NOTICES.md` + 根 NOTICE）、代码常量（`PXPIPE_SOURCE_COMMIT`）、计划文档已同步；1928×1928 非生产 profile 声明保留；`v0.12.0` 固定为唯一参考 pin | P0（已闭合） |
| PR-O2 | **Spleen 5×8 收口 + 渲染确定性**：Spleen 2.2.0/`0493c34` 来源/BSD-2-Clause/分发边界复核；renderer 跨 Windows/macOS 确定性测试（灰度 AA、同输入同 PNG） | 字体声明随分发完整；确定性测试通过；与 pxpipe 独立实现边界重申 | PR-O1 |
| PR-O3 | **active 门槛与 shadow 证据**：active 不可达断言（默认关闭 feature gate、空可信 registry、sealed ReadyToSend）；shadow 保留 raw ingress 测试；门槛文档（renderer/font/profile hash、eval digest、可信 authority）落盘 | active 无 UI 开关；post-commit 永不自动原文重试；评估记录完整 | PR-O2, PR-1 |

## 5. Codebase Memory 轨道与 PR-CBM 切片（Windows 收口）

### 5.1 现状与漂移

- manifest pin：`v0.9.1-rc.1` / `ee9833df`（2026-07-30 immutable prerelease），四平台 archive/binary SHA 记录于 `code/runtimes/codebase-memory/artifact-manifest.json`；Cosign blob bundle 校验 + OIDC issuer + GitHub Actions workflow identity 校验流程已定义；launcher 校验 Apple Team ID / Windows 证书 thumbprint（pre-sign hash 与 release 后签名分离）。
- **provenance 漂移（OSS-1 阻断任务）**：根 `THIRD_PARTY_NOTICES.md` 仍登记 `v0.9.0` / `b637e333`，与 manifest 的 v0.9.1-rc.1 不一致。两个事实源并存，任何发布物证都不可信。
- 用户要求（GOAL G9）：明确标识 prerelease；不得放宽校验；独立修复 v0.9.0 正确 release 目录仍无法配置的 Bug（系统化复现 → 找到 `package ready` 之后的真实阻塞 → 先补失败测试 → 临时 Agent 配置 preview/apply/verify/rollback + 原始 hash/权限恢复）。

### 5.2 PR-CBM 切片

| 切片 | 内容 | 验收 | 依赖 |
|---|---|---|---|
| PR-CBM1 | **漂移修复**：根 `THIRD_PARTY_NOTICES.md` CBM 条目统一为 v0.9.1-rc.1 / `ee9833df`（或按 OSS-1 决议改为引用单一事实源），与 `code/runtimes/codebase-memory/README.md`、`artifact-manifest.json` 一致；保留 v0.9.0 历史条目但标注 superseded | 全仓 CBM pin 唯一；结构检查通过；无「v0.9.0 为当前 pin」的活跃声明 | P0 |
| PR-CBM2 | **Windows 收口 + 用户 Bug 修复**：(a) Windows 目标 archive/binary SHA 与 Cosign/OIDC 校验在真实 Windows 环境执行并落证（runner 或实机）；(b) launcher 的 Windows 证书 thumbprint 校验路径候选证据；(c) 复现并修复「官方 v0.9.0 正确 release 目录仍无法配置」Bug：先补失败测试，再以临时 Agent 配置完成 preview/apply/verify/rollback，恢复原始 hash/权限；(d) 首期验收（自动识别目录根/bin/单层解压、坏 hash 平台 fail-closed、`agent_mcp`/`ui_host` 所有权分离、persistence=false 强制、状态五维）绑定候选 | 失败测试先红后绿；Windows 证据落盘；原始 hash/权限恢复断言；状态不冒充 | PR-CBM1, PR-6（配置事务复用） |

## 6. RTK 轨道与 PR-R 切片

| 切片 | 内容 | 验收 | 依赖 |
|---|---|---|---|
| PR-R1 | **v0.45.0 pin 审计**：核验 `rtk-ai/rtk` v0.45.0（`b34be37`）官方 LICENSE/NOTICE/版本语义；登记 managed RTK 来源 pin；与 PRD「RTK 为 Headroom wrap/d-token 内建能力、非独立产品路径」对齐；更新 legal notices 与根 NOTICE（如缺 RTK 条目则补） | pin 唯一；许可证/NOTICE 声明完整；无独立分发声明 | P0 |
| PR-R2 | **Windows/WSL runner 证据 + hook integrity**：`run_wsl_rtk_integrity_test.ps1` 在真实 runner 执行并落证；managed lifecycle（install/verify/filter/exit propagation/usage metadata/uninstall/cleanup）Windows/WSL 证据；hook integrity 状态绑定候选 | runner 通过报告或 external/blocked；无冒充 | PR-R1 |
| PR-R3 | **savings 分账契约**：验证 `rtk_savings` 与 `runtime_savings` 分账（PRD §10）；禁止直接相加的断言测试；`rtk gain` 不作为产品轮询路径 | 契约测试通过；仪表/导出无双计路径 | PR-R2 |

## 7. AiMaMi 轨道与 PR-A 切片

| 切片 | 内容 | 验收 | 依赖 |
|---|---|---|---|
| PR-A | **适配 provenance 复核**：pin `297c7af5` 与 LICENSE 副本哈希复核；适配范围表（PageStage/AnimatedSegmentedControl 等）与源码注释一致；「明确未包含」品牌资产声明复验；NOTICE 缺失事实记录 | 适配表与实际派生文件一致；LICENSE 副本在仓；无品牌资产进入分发 | P0 |

## 8. provenance 漂移任务（OSS-1 明确清单）

以下漂移每一项都有一个命名切片 owner，禁止用「历史已完成」替代核验：

| # | 漂移 | 事实 | owner |
|---|---|---|---|
| D1 | CBM 根 NOTICE v0.9.0 vs manifest v0.9.1-rc.1 | 根 `THIRD_PARTY_NOTICES.md` 第 52–56 行 pin v0.9.0/`b637e333`；manifest pin v0.9.1-rc.1/`ee9833df` | PR-CBM1 |
| D2 | Headroom 基线 pin 冲突 | 三候选并存：旧基线 `01df2452` vs release `v0.34.0`/`9fd5ae3d` vs main `7f6950be`（`headroom-main-2026-08-08` 新克隆）；后两者增量均未评估 | PR-H1 |
| D3 | pxpipe 旧 pin 未评估 | 已闭合（PR-O1，2026-08-08）：唯一 pin `0ceabbed`/v0.12.0；法律声明（`docs/legal/THIRD_PARTY_NOTICES.md` + 根 NOTICE）、代码常量（`PXPIPE_SOURCE_COMMIT`）、计划文档已同步；旧 pin `7b7d6efa` 标注为历史 | PR-O1（已闭合） |
| D4 | lean-ctx release vs main | 登记 pin v3.9.17/`cf8fce6b`；新克隆 main `95a1b7bc` 未评估 | PR-L0 |
| D5 | RTK 无 release pin 登记 | managed probe 有 receipt 但无来源 pin；新克隆 v0.45.0/`b34be37` 待审计 | PR-R1 |

## 9. 与发布门的绑定

- OSS-1（pin 唯一）与 OSS-2（NOTICE 随分发）是 **MUST**：D1 未闭合前 `release_ready=false`。
- 各轨道迁移完成度（CONTEXT/OPTICAL/OSS 类 requirement）release claim 见 [REQUIREMENTS.md](./REQUIREMENTS.md) 与 [RISKS_DECISIONS.md](./RISKS_DECISIONS.md)；用户最终裁决是否阻断 v0.0.1。
- 每轨道完成时同步更新 [EVIDENCE.md](./EVIDENCE.md) 证据索引，禁止复用过期 pin 的证据。
