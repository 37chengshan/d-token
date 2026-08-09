# OSS_MIGRATION — v0.0.1 开源轨道与 provenance 迁移

- 原则：每个外部来源必须有**唯一 pin**（release/commit）、许可证、NOTICE、本地只读副本路径、分发边界、当前事实与缺口；任何漂移（多 pin 并存、旧 pin 未评估）都是 OSS-1 下的显式任务，不得静默保留。
- 权威登记位置：`docs/legal/THIRD_PARTY_NOTICES.md`（源码/安装包分发声明）、根 `THIRD_PARTY_NOTICES.md`、`code/runtimes/*/README.md`；本文件是 v0.0.1 的迁移与收口计划。
- 本地只读副本一律在仓库外 `/Users/cc/Documents/d-token-external/`（不提交、不复制进生产、不新增运行依赖）。
- 未实现前任何轨道状态只能 `partial`/`unavailable`；「已登记来源」不等于「已迁移」。

## 1. 轨道总览（pin 现状 2026-08-09）

| 轨道 | 上游 | 当前登记 pin | 新事实/漂移 | 许可证 | 分发边界 | 收敛切片 |
|---|---|---|---|---|---|---|
| Headroom | `headroomlabs-ai/headroom` | `01df2452`（旧基线，main HEAD 2026-08-02） | **三候选并存（pin 冲突）**：旧基线 `01df2452`、release `v0.34.0`/`9fd5ae3d`（2026-08-05 发布）、main `7f6950be`（新克隆 `headroom-main-2026-08-08`）；后两者相对旧基线的增量均未评估；历史 v0.33.0 `28aa53dc`、`1725cd1f` 只解释旧证据 | Apache-2.0 + NOTICE | 行为对照；`code/runtimes/headroom/` 为 provenance/迁移边界，非 Python sidecar/外部 8787 依赖 | PR-H1, PR-H2, PR-H3 |
| LeanCTX | `yvgude/lean-ctx` | `v3.9.17` / `cf8fce6b`（2026-08-04 release） | **pin 漂移候选**：main `95a1b7bc` 未评估 | Apache-2.0（部分子包 MIT） | 行为对照；九边界生产实现唯一归 PR-C3A/B/C | PR-L0（pin）+ PR-C3*（实现） |
| RTK | `rtk-ai/rtk` | 未登记 release pin | v0.45.0/`b34be37` 待审计 | Apache-2.0 | 行为对照；不分发 standalone runtime；生产实现唯一归 PR-C4A/B/C | PR-R1（pin）+ PR-C4*（实现） |
| pxpipe | `teamchong/pxpipe` | `7b7d6efa`（旧法律声明） | v0.12.0/`0ceabbe` 待评估 | MIT；Spleen BSD-2-Clause | 配方/provenance 参考；renderer/active 唯一归 PR-C5A–D | PR-O1/O2（pin/provenance）+ PR-C5*（实现） |
| Codebase Memory | `DeusData/codebase-memory-mcp` | `v0.9.1-rc.1` / `ee9833df`（manifest） | **provenance 漂移（阻断级）**：根 `THIRD_PARTY_NOTICES.md` 仍 pin `v0.9.0` / `b637e333`，与 manifest 不一致 | MIT | 可选外部 sidecar：用户显式导入 pinned archive，App 不携带大文件；Cosign/OIDC 校验 | PR-CBM1, PR-CBM2 |
| AiMaMi | `borawong/AiMaMi` | `297c7af5` | 无漂移（LICENSE 副本在 `docs/legal/AIMAMI_APACHE_2_0_LICENSE.txt`）；该 commit 无根 NOTICE | Apache-2.0 | 适配声明（PageStage/AnimatedSegmentedControl 等，见 legal notices 适配范围表）；不复制品牌资产 | PR-A |
| Spleen | `fcambus/spleen` | `2.2.0` / `0493c34`（`spleen-5x8.bdf`） | 无漂移；克隆 `spleen-2.2.0` 已核验 | BSD-2-Clause | 字体数据随 Optical 分发声明；渲染实现 d-token-owned | PR-O2 |

## 2. Headroom 轨道与 PR-H 切片

### 2.1 现状

- 评估基线候选三选一：旧基线 `01df2452`（Apache-2.0 + NOTICE）、release `v0.34.0`/`9fd5ae3d`（2026-08-05 发布）、main `7f6950be`（新克隆 `headroom-main-2026-08-08`，外部路径 `/Users/cc/Documents/d-token-external/headroom/headroom-main-2026-08-08`）；增量（行为/接口/许可证/NOTICE）均未评估。已完成 v0.33.0→旧基线迁移决策（PHP codec 吸收；#2694/#2718/#2691 契约对齐评估；frozen_message_count 缺口登记于 `code/runtimes/headroom/README.md`）。
- 迁移状态：PLAN §1C 逐能力矩阵（117 项）多数为 active candidate/partial；`partial`/`占位`/`缺失` 为发布阻断（PLAN 1C.2）。
- Headroom 117 项完整迁移是 v0.0.1 MUST。PR-H/PR-C 切片必须把每项接入 canonical production dispatch 与候选证据；任何 `partial`/占位/缺失阻断。

### 2.2 PR-H 切片

| 切片 | 内容 | files（允许范围） | 验收 | 依赖 |
|---|---|---|---|---|
| PR-H1 | **pin 决议与单一事实源**：评估 release `v0.34.0`/`9fd5ae3d` 与 main `7f6950be`（`headroom-main-2026-08-08` 新克隆）相对旧基线 `01df2452` 的增量（行为/接口/许可证/NOTICE），从三候选中一次性决定评估基线；更新 `code/runtimes/headroom/README.md`、`docs/legal/THIRD_PARTY_NOTICES.md`、根 NOTICE；漂移清单清零 | `code/runtimes/headroom/README.md`、`docs/legal/THIRD_PARTY_NOTICES.md`、`THIRD_PARTY_NOTICES.md`（根）、`docs/releases/v0.0.1/OSS_MIGRATION.md`（同步） | 全仓唯一 headroom pin；评估记录落盘；不把 v0.34.0 / main 增量未评估写成完成 | P0（基线） |
| PR-H2 | **117 项机器注册表与残余能力闭合**：建立 tracked 117-item registry；每项必须映射 source capability、d-token owner、production dispatch symbol、normal/error/recovery tests、real evidence node 与 gate；按 C2 切片完成全部缺口（含 streamable HTTP MCP、跨协议、tool schema、CCR tool/crash、memory/learning/output/observability/platform） | `support/fixtures/unified-capability-registry-v1.json`（新）、`support/scripts/validate_unified_capability_registry.py`（新）、对应 `code/crates/` 与 tests | **恰 117 项**；不接受 not-in-scope/placeholder/missing；每项 production dispatch + test + evidence + gate 均非空且可解析；全量 MUST | PR-H1, PR-C0C, PR-C1C |
| PR-H3 | **§8/G9 候选证据绑定**：由 registry 生成能力矩阵与 coverage report；逐项绑定 `C_final` + `source_digest`；G9 只在 G0–G8 与 117 项 proof 全绿后开放 | `support/scripts/`（生成/验证脚本）、`docs/PLAN.md` §1C.2、`docs/releases/v0.0.1/VERIFICATION.md` | 117/117 为 verified production；任一缺证据仍 blocked；报告不能手工覆盖 registry | PR-H2, PR-C8A, PR-9 |

## 3. LeanCTX 轨道与 PR-L 切片（九边界）

### 3.1 现状

- 来源登记完成（`code/runtimes/headroom/README.md` LeanCTX 节、legal notices）：`yvgude/lean-ctx` v3.9.17 / `cf8fce6b`，Apache-2.0（子包 MIT 例外），NOTICE 存在，本地副本 `/Users/cc/Documents/d-token-external/lean-ctx/`（另有 `lean-ctx-archive`、`lean-ctx-main-2026-08-08` HEAD `95a1b7bc`）。
- d-token 差异：九条能力边界（PRD P0.0.5）必须由 d-token-owned Rust service 提供；无真实跨 Agent 证据时只能 `partial`/`unavailable`。
- LeanCTX 九边界全部是 v0.0.1 MUST；统一归属 `dtoken-context-bus`，不再以历史 v0.3 版本边界降级。

### 3.2 LeanCTX 差异追踪与唯一实现 owner

| 层 | 内容 | 验收 | 依赖 |
|---|---|---|---|
| PR-L0 | 评估 main `95a1b7bc` 相对 v3.9.17 增量；决定唯一 pin；更新登记 | pin/NOTICE/license/行为差异单一 | P0 |
| PR-C3A | Context Bus、Session Cache、map/signature、自命中/scope/size；从旧模块搬移 policy | 新 crate 单一 domain authority；store 仅 persistence；secret/raw 禁入 | PR-L0, PR-C0A |
| PR-C3B | Context Relay、Query-aware Retrieval、RelayHit/CacheReuse accounting | 两 Agent 同项目命中；跨项目污染=0；Relay 不算 compression | PR-C3A, PR-C1C |
| PR-C3C | Recovery Pointer/Manifest、Stale/Superseded、SharedContext grant/revoke | 文件/TTL/权限/source 变化拒绝；失败正常读取 | PR-C3B, PR-C2C |

## 4. Optical 轨道（pxpipe / Spleen provenance；PR-C5 唯一生产 owner）

| 层 | 内容 | 验收 | 依赖 |
|---|---|---|---|
| PR-O1 | pxpipe v0.12.0/`0ceabbe` 相对旧 pin 的 8 项差异、archive/license/NOTICE | pin 唯一；1928×1928 历史 profile 非生产；零 runtime dependency | P0 |
| PR-O2 | Spleen 2.2.0 字体/provenance/字形差异 | 字体声明完整；实现不复制 pxpipe runtime | PR-O1 |
| PR-C5A | deterministic renderer、Unicode/CJK fallback、resource/self-decode、双平台 digest | macOS/Windows 同 fixture digest；secret/exact 不入图 | PR-O1/O2, PR-C0C |
| PR-C5B | history collapse/static slab/freeze grid/cache anchor/billing boundary/pin semantics | current/recent/open/exact 文本不变；frozen prefix byte-identical | PR-C5A, PR-C2B |
| PR-C5C | exact capability/economics/paired eval lifecycle | wildcard/过期/失败拒绝；Provider paired evidence 可归因 | PR-C5B, PR-C7B |
| PR-C5D | exact automatic active production path | 至少一个 Anthropic exact route 同候选真实 active；其他 PreserveText；无 UI switch | PR-C5C, PR-C6C |

## 5. Codebase Memory 轨道与 PR-CBM 切片（Windows 收口）

### 5.1 现状与漂移

- 支持范围按 2026-08-09 CBM 五版本审计修正为**上游最近 5 个正式发布版本**：`v0.9.0`（`b637e333`，35 资产，含 windows-arm64）、`v0.8.1`（`f0c9be19`，31 资产，**无 windows-arm64**）、`0.8.0`（`v0.8.0`/`244039e6`，资产 0 已归档）、`0.7.0`（`v0.7.0`/`b133a204`，资产 0 已归档）、`0.6.1`（`v0.6.1`/`6aab9d56`，资产 0 已归档）；另保留已批准 prerelease pin `v0.9.1-rc.1`（`ee9833df`，2026-07-30，prerelease=true，35 资产），**不计入正式五版本**，UI 持续标注 pre-release。
- 逐版本验证按官方资产可得性 fail-closed：v0.9.0/v0.8.1（及 v0.9.1-rc.1）有官方二进制 → 逐版本真实验证 indexing/graph query/call chain/dependency/impact；0.8.0/0.7.0/0.6.1 官方无资产（release 已归档 assets=0）→ 只做 source-tag 级登记（tag+commit+官方归档链接）+ fail-closed（incompatible/untrusted），**绝不冒充 runtime 兼容**；缺上游资产/证据的版本保持 blocked，release 保持 blocked。
- manifest pin：当前 `artifact-manifest.json`（schema v1）为单 `v0.9.1-rc.1`/`ee9833df` 硬编码于三处（manifest、`lib.rs ARTIFACT_SPECS`、`codebase_memory_control.rs UPSTREAM_RELEASE_URL`），支持数=1；须升级 v2 多版本 manifest。Cosign blob bundle 校验 + OIDC issuer + GitHub Actions workflow identity 校验流程已定义；launcher 校验 Apple Team ID / Windows 证书 thumbprint（pre-sign hash 与 release 后签名分离）。版本身份只来自 release tag+commit+资产 hash（上游 server.json version 字段滞后，不读）。
- **provenance 漂移（OSS-1 阻断任务）**：根 `THIRD_PARTY_NOTICES.md` 仍登记 `v0.9.0` / `b637e333`，与 manifest 的 v0.9.1-rc.1 不一致。两个事实源并存，任何发布物证都不可信。
- 用户要求（GOAL G9）：明确标识 prerelease；不得放宽校验；独立修复 v0.9.0 正确 release 目录仍无法配置的 Bug（系统化复现 → 找到 `package ready` 之后的真实阻塞 → 先补失败测试 → 临时 Agent 配置 preview/apply/verify/rollback + 原始 hash/权限恢复）。

### 5.2 PR-CBM 切片

| 切片 | 内容 | 验收 | 依赖 |
|---|---|---|---|
| PR-CBM1 | **漂移修复**：根 `THIRD_PARTY_NOTICES.md` CBM 条目统一为 v0.9.1-rc.1 / `ee9833df`（或按 OSS-1 决议改为引用单一事实源），与 `code/runtimes/codebase-memory/README.md`、`artifact-manifest.json` 一致；保留 v0.9.0 历史条目但标注 superseded | 全仓 CBM pin 唯一；结构检查通过；无「v0.9.0 为当前 pin」的活跃声明 | P0 |
| PR-CBM2 | **Windows 收口 + 用户 Bug 修复**：(a) Windows 目标 archive/binary SHA 与 Cosign/OIDC 校验在真实 Windows 环境执行并落证（runner 或实机）；(b) launcher 的 Windows 证书 thumbprint 校验路径候选证据；(c) 复现并修复「官方 v0.9.0 正确 release 目录仍无法配置」Bug：先补失败测试，再以临时 Agent 配置完成 preview/apply/verify/rollback，恢复原始 hash/权限；(d) 首期验收（自动识别目录根/bin/单层解压、坏 hash 平台 fail-closed、`agent_mcp`/`ui_host` 所有权分离、persistence=false 强制、状态五维）绑定候选 | 失败测试先红后绿；Windows 证据落盘；原始 hash/权限恢复断言；状态不冒充 | PR-CBM1, PR-6（配置事务复用） |
| PR-CBM3 | **多版本 manifest schema v2**：五正式版 v0.9.0/v0.8.1/0.8.0/0.7.0/0.6.1 + 批准 prerelease pin v0.9.1-rc.1 并列标注；每版本 tag+commit+平台 archive/binary SHA+协议适配器+平台矩阵（v0.8.1 无 windows-arm64 → 显式 incompatible）；版本身份只用 tag/commit/hash（不读 server.json version）；构建期 v2 manifest 摘要 + 完整性测试 | 五正式版 + prerelease pin 全可寻址；UI 标注 pre-release；任一版本无资产/无证据 → incompatible/blocked，release 保持 blocked | PR-CBM1, P0 |
| PR-CBM4 | **逐版本真实验证 harness**：v0.9.0/v0.8.1（有官方二进制）临时工程 indexing/graph query/call chain/dependency/impact 真实验证落证（repo zero-write + persistence=false + endpoint allowlist + macOS/Windows）；0.8.0/0.7.0/0.6.1（官方无资产）source-tag 登记 + fail-closed（untrusted/incompatible），UI 不开放配置/索引/图谱 | 有官方二进制版本逐版本真实通过；无资产版本 fail-closed 呈现；绝不冒充 runtime 兼容；缺上游资产 → blocked | PR-CBM3, PR-6 |

## 6. RTK 轨道（PR-R1 provenance；PR-C4 唯一生产 owner）

| 层 | 内容 | 验收 | 依赖 |
|---|---|---|---|
| PR-R1 | v0.45.0/`b34be37` pin/license/NOTICE/差异矩阵 | pin 唯一；无外部 runtime dependency | P0 |
| PR-C4A | classification registry、TOML DSL、git/cargo/npm/pytest/json filters | reconstruction/negative/fuzz；secret/exact/error/unknown Preserve | PR-R1, PR-C0A |
| PR-C4B | production Agent hook/tee + macOS/Windows/WSL lifecycle | config exact restore；exit/timeout/truncation；tee ACL/TTL；ordinary persistence 无 argv/raw/credential | PR-C4A, PR-6 |
| PR-C4C | RtkCommandOutput NodeAction、Ledger/accounting、uninstall | production dispatch 可达；不与 compression/Relay/cache 双计 | PR-C4B, PR-C1C |

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
| D3 | pxpipe 旧 pin 未评估 | 法律声明 pin `7b7d6efa`；上游已发布 v0.12.0/`0ceabbe` | PR-O1 |
| D4 | lean-ctx release vs main | 登记 pin v3.9.17/`cf8fce6b`；新克隆 main `95a1b7bc` 未评估 | PR-L0 |
| D5 | RTK 无 release pin 登记 | managed probe 有 receipt 但无来源 pin；新克隆 v0.45.0/`b34be37` 待审计 | PR-R1 |

## 9. 与发布门的绑定

- OSS-1（pin 唯一）、OSS-2（NOTICE 随分发）以及每条迁移轨道的功能闭环都是 **MUST**；任一漂移、占位或缺同候选证据时 `release_ready=false`。
- 每轨道完成时同步更新 [EVIDENCE.md](./EVIDENCE.md)；禁止复用过期 pin 的证据。
