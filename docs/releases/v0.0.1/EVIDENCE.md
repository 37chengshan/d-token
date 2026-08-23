# EVIDENCE — v0.0.1 证据索引

- 规则：证据分为「已有」「过期」「待生成」三类。**过期证据禁止冒充最终候选**；最终候选只接受绑定 `C_final` + 同一 `source_digest` 的证据（见 [VERIFICATION.md](./VERIFICATION.md#8)）。
- 路径均为相对仓库根；`support/artifacts/` 下为 gitignored 运行时证据，不作为 PR 交付物。
- 本索引随执行滚动更新（[TASKS.md](./TASKS.md) T-DOC）。

## 1. 已有证据（2026-08-08 可引用，但未绑定 `C_final` 前只作过程事实）

| 证据 | 路径/位置 | 事实 | 绑定 | 用途 |
|---|---|---|---|---|
| G7 真实 OpenCode（2026-08-04） | `support/artifacts/opencode-real-routed/real-routed-20260804T112429Z-76558.json` | schema v2，candidate_id=G7，status=pass；10 请求成功；compression 139116→137858；claimable=false；CCR 全生命周期通过 | candidate_id=G7（**非 C_final**） | 历史参考；N3 必须重跑 |
| macOS managed RTK probe | `support/artifacts/managed-rtk-candidate/candidate-20260731T223754Z-7536.json` | install/verify/filter/exit propagation/usage metadata/uninstall/cleanup；command_argv_persisted=false、raw_output_returned=false、credential_value_read=false | 2026-07-31 候选 | N7 重跑（v0.45.0） |
| 全仓复证（2026-08-07） | `docs/PLAN.md` 当前坐标 / `docs/product/IMPLEMENTATION_STATUS.md` | `cargo test --workspace` 936 pass/5 ignored；clippy strict pass；Vitest 634 pass；tsc/build pass；Chromium+WebKit E2E 各 13 pass | 2026-08-07 working tree（非 C_final） | N0 重跑基线参考 |
| macOS rebuilt app+DMG 审计 | SHA-256 `60f7d063…`（PLAN 记录） | ad-hoc 未签名/未公证；DMG 挂载/安装/启动/退出/卸载链路通过 | 2026-08-04/07 候选 | N7 重跑 |
| G2 codec release probe | evidence revision `d0cb2e03…`（PLAN 记录） | 每类 ≥128 property/semantic/rollback cases；negative/budget/cancel 门零失败 | 旧 revision | N0/N1 重跑 |
| cc-switch A→B→A 静态识别（2026-07-31） | `docs/PLAN.md` 当前坐标 10 | 静态 provider 切换识别与恢复；active route identity 与初始一致 | 2026-07-31 | N5 以结构化 artifact 重做 |
| Claude/Codex/Cline 真实临时配置事务 | `docs/PLAN.md` 当前坐标 8（OpenCode 配置事务见当前坐标 7） | preview→apply→reread→clear→exact restore；恢复 SHA-256 与 0600 权限；未启动 Agent/未读凭据 | 2026-07-31/08-05 | N4 用 harness 重做 |
| 视觉审计原生截图 | `/tmp/dtoken_audit/native_1180x780.png`（2026-08-07，进程外临时目录） | 1180×780 中文浅色折叠态实测；C1/C2 确认 | 2026-08-07（非 C_final） | N6 全组合重做 |
| release gate 聚合状态 | `support/artifacts/release-gate-status-20260807T092350Z.json`、`...T110417Z.json` | fail-closed 输出；G6/G8 未通过 | 2026-08-07 | N9 重新生成 |
| RTK 研究/内部实现 | `docs/archive/` REPORT_01_RTK_HEADROOM_INTERNALS 等 | RTK 行为/边界研究 | 历史 | 背景 |
| provenance 登记 | `docs/legal/THIRD_PARTY_NOTICES.md`、根 `THIRD_PARTY_NOTICES.md`、`code/runtimes/headroom/README.md`、`code/runtimes/codebase-memory/README.md`+`artifact-manifest.json` | 七轨道来源/pin/许可证/NOTICE | 2026-08-08 更新 | OSS 漂移 D1–D5 已全部闭合（见本表 Headroom/pxpipe 行与下述 CBM/LeanCTX/RTK 闭合行） |
| Headroom 三候选 pin 审计（PR-H1，2026-08-08） | `code/runtimes/headroom/README.md` §三候选漂移审计与唯一 pin 决议；`docs/legal/THIRD_PARTY_NOTICES.md`；根 `THIRD_PARTY_NOTICES.md` | 唯一 current comparison pin = `v0.34.0`/`9fd5ae3d`；`01df2452`→`v0.34.0`（33 commit/129 文件/+8936 −1021）、`v0.34.0`→main `7f6950be`（33 commit/74 文件/+4874 −428）漂移已审计；许可证 Apache-2.0 三 pin 无差异；NOTICE 增量（#2734 vendored dashboard assets）已登记；无必须采用 main 的实际证据；`01df2452` 标历史、main 只作 post-release delta | worktree `codex/final/headroom-pin`（base `c5a5849`）；三候选 commit 于外部克隆 `headroom-main-2026-08-08` 核验 | OSS 漂移 D2 闭合（只闭合 pin 漂移，不涉及能力迁移完成度） |
| CBM pin 统一（PR-CBM1，2026-08-08） | 根 `THIRD_PARTY_NOTICES.md` CBM 条目；`code/runtimes/codebase-memory/artifact-manifest.json` | 根 NOTICE 已统一为 v0.9.1-rc.1/`ee9833df2c72…4f2`（2026-07-30 immutable prerelease），与 manifest 一致；v0.9.0/`b637e333…` 标 superseded（仅作用户 v0.9.0 Bug 复现基线） | worktree `codex/final/cbm-notice-drift` | OSS 漂移 D1 闭合 |
| LeanCTX pin 决议（PR-L0，2026-08-08） | `code/runtimes/headroom/README.md` LeanCTX 节；`docs/legal/THIRD_PARTY_NOTICES.md`；根 `THIRD_PARTY_NOTICES.md` | 唯一参考 pin = `v3.9.17`/`cf8fce6bb175…5a9`；main `95a1b7bc60a0…93ba` 评估后仅作 post-pin 研究：93 commit/160 文件/+11209 −2665（ADR-001..013、`lean-ctx-protocol` v0.1.0、enterprise gating、r15–r18 系列、安全修复 #1428/#1431–35/#1418/#1437），无版本号变化、无 LICENSE/NOTICE 变化 | worktree `codex/final/leanctx-pin`；delta 于外部克隆 `lean-ctx-main-2026-08-08` 核验 | OSS 漂移 D4 闭合 |
| RTK pin 审计（PR-R1，2026-08-08） | `docs/legal/THIRD_PARTY_NOTICES.md` RTK 节；根 `THIRD_PARTY_NOTICES.md` | 唯一参考 pin = `v0.45.0`/`b34be37caf3…ad43`（tag `v0.45.0`/`latest` 同 commit）；LICENSE Apache-2.0 SHA-256 `4044ade9…` 核验（标准条款 1–9 完整）；固定 commit 无根 NOTICE | worktree `codex/final/rtk-pin`；外部克隆 `rtk-v0.45.0` 核验 | OSS 漂移 D5 闭合 |

## 2. 过期证据隔离清单（禁止绑定 `C_final`，禁止写入最终证据表）

| 证据 | 过期原因 | 处置 |
|---|---|---|
| 2026-08-04 G7 OpenCode real-routed（candidate_id=G7） | candidate_id ≠ C_final | N3 重跑；只作历史 |
| 旧 DMG（c124d9f / 60f7d063 ad-hoc 审计） | 非 C_final commit；未签名 | N7 重跑 |
| 任何浏览器 preview 截图 | 只证明 UI，不证明 native | N6 主代理 Computer 重做 |
| 任何非 `C_final` commit 的测试报告（含 2026-08-07 全仓复证、G2 evidence revision `d0cb2e03…`） | 候选变更后失效 | N0 完整重跑 |
| `support/artifacts/` 中旧 schema/旧 mtime 的 artifact | 聚合器不得按 mtime/latest/pass-only 接受 | PR-7/PR-9 candidate binding 拒绝 |
| 历史 docs（`docs/history/`、`docs/archive/`）中任何「已发布/已通过」表述 | 历史快照 | 只追溯，不作为当前证据 |

## 3. 待生成证据（owner 切片 / 验证节点）

| 证据 | 路径（建议） | owner | 绑定 |
|---|---|---|---|
| baseline freeze 报告（更新版） | `.ai-bridge/release-iteration/preflight-report.md` | P0 | 冻结 commit |
| OptimizationReceipt 单测 + 投影测试 | `code/crates/dtoken-types/tests/`、`code/crates/dtoken-core-api/tests/` | PR-1 | 切片 commit |
| Codex routed acceptance artifact | `support/artifacts/codex-ocxlocal-routed/` | PR-2 → N2 | C_final |
| 无 chooser 残留 grep 输出 | 切片报告 | PR-3 | 切片 commit |
| 920×640 Playwright layout 证据 | e2e 报告 | PR-4 | 切片 commit |
| SecretClassifier 覆盖矩阵 + export 回归 | 切片报告 | PR-5 | 切片 commit |
| 配置验收契约测试 + 真实验收 | `support/tests/test_agent_config_acceptance.py`；N4 | PR-6 → N4 | C_final |
| cc-switch A→B→A artifact | `support/artifacts/cc-switch-aba/` | PR-7 → N5 | C_final |
| macOS upgrade/rollback + autostart/credential round-trip | `support/artifacts/`（N7） | PR-8 → N7 | C_final |
| fan-in artifact contract | `support/artifacts/fan-in-artifact-contract-*.json` | PR-7/PR-8 → N8 | C_final + digest |
| 版本统一检查输出 | `check_release_version.py` 输出 | V-1 | v0.0.1 |
| N0 门禁报告 | `support/artifacts/release-gate-status-<ts>.json` | N0 | C_final |
| N1–N8 各域证据 | 对应 artifact 目录 | Main/外部 | C_final |
| N9 最终聚合 + 三结论判定 + 发布报告 | `.ai-bridge/release-iteration/final-candidate-evidence.md`（模板）+ 发布报告 | PR-9 → N9 | C_final |
| OSS 漂移闭合记录（D1–D5） | 各切片报告（D1–D5 已全部闭合：D1 PR-CBM1（根 NOTICE v0.9.1-rc.1/`ee9833df`，v0.9.0 superseded）、D2 PR-H1（`code/runtimes/headroom/README.md` §三候选漂移审计与唯一 pin 决议）、D3 PR-O1（pxpipe v0.12.0/`0ceabbe` 唯一 pin）、D4 PR-L0（lean-ctx v3.9.17/`cf8fce6b` 唯一 pin，main `95a1b7bc` 93-commit delta 已评估）、D5 PR-R1（RTK v0.45.0/`b34be37` + LICENSE SHA-256 `4044ade9…`）） | PR-CBM1/H1✅/O1✅/L0/R1（已闭合） | 切片 commit |
| Headroom 矩阵/G9 判定表 | `docs/PLAN.md` §1C、切片报告 | PR-H2/H3 | C_final |
| LeanCTX 九边界实现与测试 | 对应 crate tests | PR-L1..L3 | 切片 commit |
| pxpipe v0.12.0 评估记录 | 切片报告 | PR-O1 | 切片 commit |
| Spleen/渲染确定性测试 | `code/crates/dtoken-compress/tests/` | PR-O2 | 切片 commit |
| Optical active 不可达断言 | 对应测试 + `docs/optical/release-gates.md` | PR-O3 | 切片 commit |
| CBM Windows 证据 + v0.9.0 Bug 修复测试 | `support/tests/`、runner 报告 | PR-CBM2 | C_final |
| RTK v0.45.0 pin 记录 + Windows/WSL 证据 | 切片报告、runner 报告 | PR-R1/R2 | C_final |
| AiMaMi 适配复核记录 | 切片报告 | PR-A | 切片 commit |

## 4. 证据卫生规则

1. 任何已生成证据不得在候选 commit 变化后复用（包括 mtime 更新）；必须重新执行。
2. artifact 必须携带 `candidate_commit`/`source_digest`；聚合器强校验。
3. 截图证据必须来自主代理 Computer 操作（真实 Tauri 窗口），并含时间戳/窗口信息。
4. `support/artifacts/` 只放运行时证据；可版本化 schema/template 走 tracked 路径。
5. 本索引每完成一个切片/节点即更新；发布报告以最终版索引为准。
