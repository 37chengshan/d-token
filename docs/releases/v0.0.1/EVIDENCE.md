# EVIDENCE — v0.0.1 证据索引

- 规则：证据分为「已有」「过期」「待生成」。最终候选只接受 [VERIFICATION.md](./VERIFICATION.md#1-证据公共-envelope所有-n0n10-必填) 的统一 envelope，并精确绑定 `C_final`、`source_digest`、117 registry digest 与各 corpus revision；旧证据、`candidate_id=null`、mtime/latest/pass-only 一律拒绝。
- 运行 artifact 统一位于 `support/artifacts/final-candidate/<C_final>/`（gitignored）；tracked schema/registry/corpus/template 位于 `support/fixtures/`、`support/schemas/` 或 `support/scripts/`，由任务首次创建。
- 本索引随每个 C0–C8 切片与 N0–N10 节点滚动更新；“路径存在”不构成证据。

## 1. 已有证据（2026-08-09 可引用，但未绑定 `C_final` 前只作过程事实）

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
| provenance 登记 | `docs/legal/THIRD_PARTY_NOTICES.md`、根 `THIRD_PARTY_NOTICES.md`、`code/runtimes/headroom/README.md`、`code/runtimes/codebase-memory/README.md`+`artifact-manifest.json` | 七轨道来源/pin/许可证/NOTICE | 2026-08-06 更新 | OSS 漂移 D1–D5 基线 |

## 2. 过期证据隔离清单（禁止绑定 `C_final`，禁止写入最终证据表）

| 证据 | 过期原因 | 处置 |
|---|---|---|
| 2026-08-04 G7 OpenCode real-routed（candidate_id=G7） | candidate_id ≠ C_final | N3 重跑；只作历史 |
| 旧 DMG（c124d9f / 60f7d063 ad-hoc 审计） | 非 C_final commit；未签名 | N7 重跑 |
| 任何浏览器 preview 截图 | 只证明 UI，不证明 native | N6 主代理 Computer 重做 |
| 任何非 `C_final` commit 的测试报告（含 2026-08-07 全仓复证、G2 evidence revision `d0cb2e03…`） | 候选变更后失效 | N0 完整重跑 |
| `support/artifacts/` 中旧 schema/旧 mtime 的 artifact | 聚合器不得按 mtime/latest/pass-only 接受 | PR-7/PR-9 candidate binding 拒绝 |
| 历史 docs（`docs/history/`、`docs/archive/`）中任何「已发布/已通过」表述 | 历史快照 | 只追溯，不作为当前证据 |

## 3. 待生成证据（全部 MUST）

| 证据 | 计划路径 | owner/node | 必需绑定 |
|---|---|---|---|
| C_final source manifest | `support/artifacts/final-candidate/<C_final>/source-manifest.json` | N0 | commit/source digest/dirty=false |
| 117 capability registry | `support/fixtures/unified-capability-registry-v1.json` + validator/coverage schema | C0C/C8A/U1/U9 | registry digest；恰 117；dispatch/tests/evidence/gate |
| fixed corpus manifests | `support/fixtures/unified-compression/{benchmark,replay,fuzz,fault,quality,optical}/manifest.json` | C0C/C7/N10 | corpus revision/digest/license/privacy |
| contract/surface parity | `.../<C_final>/N10-U0-contract.json` | C1/U0 | route decision + IR/schema/producer digests |
| property/fuzz | `.../<C_final>/N10-U2-property-fuzz.json` | C2/C4/C5/U2 | codec/NodeAction/corpus/failure counts |
| fault/durability | `.../<C_final>/N10-U3-fault.json` | C6/U3 | failpoint registry + send terminal receipts |
| replay/task quality | `.../<C_final>/N10-U4-replay-quality.json` | C7B/U4 | baseline/optimized body+decision digests、outcomes |
| perf fresh baseline/SLO | `.../<C_final>/N10-U5-perf.json` + tracked thresholds | C7A/C7C/U5 | hardware/OS/release profile/corpus/noise band |
| Provider paired usage | `.../<C_final>/N10-U6-provider-paired.json` | C7B/U6 | exact provider/model/API/route；actual usage/cost/cache/latency |
| Optical exact active | `.../<C_final>/N10-U7-optical-active.json` | C5D/U7 | capability/renderer/font/profile/corpus/tokenizer/pricing/cache/eval digests |
| cross-platform determinism | `.../<C_final>/N10-U8-platform.json` | C5A/N8/U8 | macOS/Windows PNG/body/plan digest |
| 117 coverage seal | `.../<C_final>/N10-U9-coverage.json` | C8A/U9 | 117/117 verified、零 placeholder |
| N0 preflight | `.../<C_final>/N0-preflight.json` | N0 | full gates + source/registry/corpus digests |
| N1 receipt/accounting | `.../<C_final>/N1-receipt.json` | N1 | request/route/validation/send/provider outcome digests |
| N2 Codex routed | `.../<C_final>/N2-codex.json` | N2 | agent_id=codex、route/model/protocol/restore/privacy |
| N3 OpenCode routed | `.../<C_final>/N3-opencode.json` | N3 | **两条长工程验收**：OpenCode Free `opencode/deepseek-v4-flash-free` 与 Go dsv4flash 工程链；真实 routed path、长时/长上下文 workload、C_final/source digest、same-model baseline/optimized paired、exact restore；旧 bounded replay（2026-08-04 G7）不替代 |
| N4 config | `.../<C_final>/N4-config.json` | N4 | per-target before/after/restore hash+permissions |
| N5 cc-switch | `.../<C_final>/N5-cc-switch.json` | N5 | A→B→A、wrote=false、identity restore、Computer evidence |
| N6 Tauri UI | `.../<C_final>/N6-ui.json` | N6 | real window, size/theme/language/motion/actions |
| N7 macOS | `.../<C_final>/N7-macos.json` | N7 | app/DMG/autostart/keychain/RTK/install rollback |
| N8 Windows/G6/G8 | `.../<C_final>/N8-windows.json` + fan-in | N8 | NSIS/DACL/MSVC/pwsh/CBM/RTK/signature/same commit |
| N9 final aggregation | `.../<C_final>/N9-release-status.json` + release report | N9 | explicit artifact paths；G0–G9；三结论 |
| OSS/provenance closure | `.../<C_final>/N0-provenance.json` + tracked NOTICE/SBOM | H1/L0/R1/O1/O2/CBM1/A/N0 | unique pins/archive/license/distribution boundaries |
| CBM v0.9.0 bug + v0.9.1-rc.1 | N4/N8 subreceipts | CBM2 | failing-path reproduction + exact restore + Windows Cosign |
| CBM 五版本 v2 manifest | `code/runtimes/codebase-memory/artifact-manifest.json`（v2 schema）+ 构建期摘要测试 | CBM3 | 五正式版 v0.9.0/v0.8.1/0.8.0/0.7.0/0.6.1 + 批准 prerelease pin v0.9.1-rc.1 并列；tag/commit/平台 SHA/协议适配器/平台矩阵 |
| CBM 逐版本真实验证 | `.../<C_final>/N4-cbm-versions.json`（子 receipt）+ `support/fixtures/cbm-versions/` | CBM4 | v0.9.0/v0.8.1 官方二进制临时工程 indexing/graph/call chain/dependency/impact 真实证据；0.8.0/0.7.0/0.6.1 source-tag 登记 + fail-closed（untrusted/incompatible） |

任何计划路径在脚本/producer/schema 未实现前状态都是 `missing`/blocked，禁止先在本表标 completed。

## 4. 证据卫生规则

1. artifact 只能由其 tracked producer 生成；producer/schema/registry/corpus digest 必须进入统一 envelope。
2. 聚合器必须消费显式 artifact path manifest，并逐项复算 SHA-256；禁止目录扫描、`latest_file()`、mtime、文件名或 `status=pass` 选证。
3. 候选变化即失效；`candidate_commit`、`source_digest`、registry/corpus/eval/platform 任一不匹配都 blocked。
4. 截图/Computer evidence 必须来自主代理真实 Tauri/cc-switch 操作，附窗口/坐标/时间；浏览器 preview 不能证明 native/runtime。
5. 任何 raw prompt/response/PNG/base64/secret/auth/config content 不进入 artifact；只保存 bounded metadata、hash、size、reason、receipt。
6. `support/artifacts/` 保持 gitignored；tracked schema/fixture/threshold/provenance 不得写入 runtime artifact 目录。
7. 每完成一个切片/节点即更新索引；final report 只引用 N9 manifest 列出的 artifact。
